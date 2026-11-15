//! The two files that describe a phone.
//!
//! `phone.toml` (the [`Profile`]) holds what the user chose and is only changed at
//! the user's request. `state.toml` (the [`State`]) holds what Verone observed and
//! can be deleted without losing anything the user decided.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::storage::{PhoneDir, PhoneId};

/// The file format version this build reads and writes.
pub const SCHEMA: u32 = 1;

/// Android version used when the user does not choose one.
pub const DEFAULT_ANDROID: &str = "14";

/// Longest allowed display name, in characters.
pub const MAX_NAME_LENGTH: usize = 64;

/// Allowed range for each side of the screen, in pixels.
pub const SCREEN_SIDE_RANGE: std::ops::RangeInclusive<u32> = 240..=4096;

/// Allowed range for the screen density, in dots per inch.
pub const DPI_RANGE: std::ops::RangeInclusive<u32> = 72..=640;

/// Lowest port a user may publish adb on. Lower ports need administrator rights.
pub const MIN_LOCAL_PORT: u16 = 1024;

/// What the user chose for a phone. Stored as `phone.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// File format version.
    pub schema: u32,
    /// The phone's id; always equal to its folder name.
    pub id: PhoneId,
    /// The name shown to the user.
    pub name: String,
    /// Which engine runs the phone.
    pub engine: EngineKind,
    /// Android version, for example `"14"`.
    pub android: String,
    /// When the phone was created, in seconds since 1970.
    pub created_unix: u64,
    /// Screen size and density.
    #[serde(default)]
    pub display: Display,
    /// How the screen is drawn.
    #[serde(default)]
    pub graphics: Graphics,
    /// Debug connection settings.
    #[serde(default)]
    pub adb: Adb,
}

/// The engines a phone can run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind {
    /// Android in a container that shares the host kernel.
    Container,
}

/// Screen size and density.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Display {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Density in dots per inch.
    pub dpi: u32,
}

impl Default for Display {
    fn default() -> Self {
        Self {
            width: 720,
            height: 1280,
            dpi: 320,
        }
    }
}

/// How the screen is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Graphics {
    /// The mode the user asked for.
    pub mode: GraphicsMode,
}

/// The graphics mode the user asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphicsMode {
    /// Use the GPU if a usable one exists, otherwise software.
    #[default]
    Auto,
    /// Always draw in software.
    Software,
    /// Always use the GPU; fail if none is usable.
    Gpu,
}

/// Debug connection settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Adb {
    /// Port on `127.0.0.1` where adb is published. `0` means not published.
    pub local_port: u16,
}

impl Profile {
    /// A profile with default settings, created now. The display name is the id
    /// until the user gives one.
    pub fn new(id: PhoneId) -> Self {
        Self {
            schema: SCHEMA,
            name: id.to_string(),
            id,
            engine: EngineKind::Container,
            android: DEFAULT_ANDROID.to_string(),
            created_unix: now_unix(),
            display: Display::default(),
            graphics: Graphics::default(),
            adb: Adb::default(),
        }
    }

    /// Checks every value. Problems are usage errors: the user asked for something
    /// that is not allowed.
    pub fn validate(&self) -> Result<()> {
        if self.schema != SCHEMA {
            return Err(schema_error("phone profile", self.schema));
        }

        let name_length = self.name.chars().count();
        if self.name.trim().is_empty() {
            return Err(invalid("the name is empty", "Give the phone a name."));
        }
        if name_length > MAX_NAME_LENGTH {
            return Err(invalid(
                format!("the name is longer than {MAX_NAME_LENGTH} characters"),
                "Use a shorter name.",
            ));
        }
        if self.name.chars().any(char::is_control) {
            return Err(invalid(
                "the name contains a control character such as a line break",
                "Use plain text for the name.",
            ));
        }

        let android_ok = !self.android.is_empty()
            && self.android.chars().all(|c| c.is_ascii_digit() || c == '.')
            && self.android.starts_with(|c: char| c.is_ascii_digit());
        if !android_ok {
            return Err(invalid(
                format!("'{}' is not an Android version", self.android),
                "Use a version number such as 14.",
            ));
        }

        for (side, value) in [
            ("width", self.display.width),
            ("height", self.display.height),
        ] {
            if !SCREEN_SIDE_RANGE.contains(&value) {
                return Err(invalid(
                    format!("the screen {side} is {value}"),
                    format!(
                        "Use a {side} from {} to {} pixels.",
                        SCREEN_SIDE_RANGE.start(),
                        SCREEN_SIDE_RANGE.end()
                    ),
                ));
            }
        }
        if !DPI_RANGE.contains(&self.display.dpi) {
            return Err(invalid(
                format!("the screen density is {} dpi", self.display.dpi),
                format!(
                    "Use a density from {} to {} dpi.",
                    DPI_RANGE.start(),
                    DPI_RANGE.end()
                ),
            ));
        }

        let port = self.adb.local_port;
        if port != 0 && port < MIN_LOCAL_PORT {
            return Err(invalid(
                format!("the adb port is {port}"),
                format!("Use 0 for no port, or a port from {MIN_LOCAL_PORT} to 65535."),
            ));
        }
        Ok(())
    }

    /// Reads a profile from TOML text and checks it.
    pub fn from_toml(text: &str) -> Result<Self> {
        check_schema(text, "phone profile")?;
        let profile: Self = toml::from_str(text)
            .map_err(|error| Error::failed(format!("the phone profile is not valid: {error}")))?;
        profile.validate()?;
        Ok(profile)
    }

    /// Writes the profile as TOML text.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string(self)
            .map_err(|error| Error::failed(format!("could not write the phone profile: {error}")))
    }

    /// Loads a phone's `phone.toml`. The id inside must match the folder.
    pub fn load(phone: &PhoneDir) -> Result<Self> {
        let path = phone.profile_path();
        let text = read(&path)?;
        let profile = Self::from_toml(&text).map_err(|error| in_file(error, &path))?;
        if &profile.id != phone.id() {
            return Err(Error::failed(format!(
                "{} says its id is '{}', but it is in the folder of '{}'",
                path.display(),
                profile.id,
                phone.id()
            ))
            .with_hint("Set id in the file to match the folder name."));
        }
        Ok(profile)
    }

    /// Checks the profile and saves it as the phone's `phone.toml`.
    pub fn save(&self, phone: &PhoneDir) -> Result<()> {
        self.validate()?;
        if &self.id != phone.id() {
            return Err(Error::failed(format!(
                "cannot save the profile of '{}' into the folder of '{}'",
                self.id,
                phone.id()
            )));
        }
        write_atomically(&phone.profile_path(), &self.to_toml()?)
    }
}

/// What Verone observed about a phone. Stored as `state.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    /// File format version.
    pub schema: u32,
    /// The last successful boot, if there has been one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_boot: Option<LastBoot>,
}

/// Facts about the last successful boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LastBoot {
    /// The graphics mode the phone actually booted in.
    pub graphics: GraphicsUsed,
    /// True if GPU mode was tried first and the phone had to fall back.
    pub fell_back_to_software: bool,
}

/// The graphics mode a phone actually ran in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphicsUsed {
    /// Drawn in software.
    Software,
    /// Drawn by the GPU.
    Gpu,
}

impl Default for State {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            last_boot: None,
        }
    }
}

impl State {
    /// Reads state from TOML text.
    pub fn from_toml(text: &str) -> Result<Self> {
        check_schema(text, "phone state")?;
        toml::from_str(text)
            .map_err(|error| Error::failed(format!("the phone state is not valid: {error}")))
    }

    /// Writes the state as TOML text.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string(self)
            .map_err(|error| Error::failed(format!("could not write the phone state: {error}")))
    }

    /// Loads a phone's `state.toml`. A phone that has never been started has no
    /// such file, which gives the default state.
    pub fn load(phone: &PhoneDir) -> Result<Self> {
        let path = phone.state_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = read(&path)?;
        Self::from_toml(&text).map_err(|error| {
            in_file(error, &path)
                .with_hint("This file only records observations. It is safe to delete it.")
        })
    }

    /// Saves the state as the phone's `state.toml`.
    pub fn save(&self, phone: &PhoneDir) -> Result<()> {
        write_atomically(&phone.state_path(), &self.to_toml()?)
    }
}

/// Reads only the `schema` value and rejects files this build cannot understand,
/// before the rest of the file is interpreted.
fn check_schema(text: &str, what: &str) -> Result<()> {
    let table: toml::Table = toml::from_str(text)
        .map_err(|error| Error::failed(format!("the {what} is not valid TOML: {error}")))?;
    match table.get("schema").and_then(toml::Value::as_integer) {
        Some(schema) if schema == i64::from(SCHEMA) => Ok(()),
        Some(schema) => Err(schema_error(what, u32::try_from(schema).unwrap_or(0))),
        None => Err(Error::failed(format!("the {what} has no schema number"))
            .with_hint(format!("Add the line: schema = {SCHEMA}"))),
    }
}

fn schema_error(what: &str, found: u32) -> Error {
    if found > SCHEMA {
        Error::failed(format!(
            "the {what} uses file format {found}, but this version of Verone only understands format {SCHEMA}"
        ))
        .with_hint("It was written by a newer Verone. Upgrade Verone to use this phone.")
    } else {
        Error::failed(format!(
            "the {what} uses file format {found}, which is not supported (expected {SCHEMA})"
        ))
        .with_hint(format!(
            "Set schema = {SCHEMA} if the file was edited by hand."
        ))
    }
}

fn invalid(problem: impl Into<String>, hint: impl Into<String>) -> Error {
    Error::usage(format!(
        "the phone settings are not valid: {}",
        problem.into()
    ))
    .with_hint(hint)
}

/// Adds the file path to an error found while reading a file, keeping its hint.
fn in_file(error: Error, path: &Path) -> Error {
    let located = Error::failed(format!("{}: {}", path.display(), error.message()));
    match error.hint() {
        Some(hint) => located.with_hint(hint),
        None => located,
    }
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path)
        .map_err(|error| Error::failed(format!("could not read {}: {error}", path.display())))
}

/// Writes a file so that a crash leaves either the old content or the new, never
/// a partial file: the text goes to a temporary file that then replaces the target.
fn write_atomically(path: &Path, text: &str) -> Result<()> {
    let failed = |error: std::io::Error| {
        Error::failed(format!("could not write {}: {error}", path.display()))
    };
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(".tmp");
    let temporary = Path::new(&temporary);
    fs::write(temporary, text).map_err(failed)?;
    fs::rename(temporary, path).map_err(failed)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use crate::storage::Storage;

    const EXAMPLE: &str = r#"
schema = 1
id = "work"
name = "Work phone"
engine = "container"
android = "14"
created_unix = 1794700000

[display]
width = 720
height = 1280
dpi = 320

[graphics]
mode = "auto"

[adb]
local_port = 0
"#;

    fn id(text: &str) -> PhoneId {
        PhoneId::new(text).unwrap()
    }

    fn profile() -> Profile {
        Profile::new(id("work"))
    }

    fn phone_dir(temp: &tempfile::TempDir, name: &str) -> PhoneDir {
        Storage::new(temp.path()).create_phone(&id(name)).unwrap()
    }

    // ---- defaults and round trips ----

    #[test]
    fn a_new_profile_has_the_documented_defaults() {
        let profile = profile();
        assert_eq!(profile.schema, 1);
        assert_eq!(profile.name, "work");
        assert_eq!(profile.engine, EngineKind::Container);
        assert_eq!(profile.android, "14");
        assert_eq!(
            profile.display,
            Display {
                width: 720,
                height: 1280,
                dpi: 320
            }
        );
        assert_eq!(profile.graphics.mode, GraphicsMode::Auto);
        assert_eq!(profile.adb.local_port, 0);
        assert!(profile.created_unix > 1_700_000_000);
        profile.validate().unwrap();
    }

    #[test]
    fn the_documented_example_is_read_correctly() {
        let profile = Profile::from_toml(EXAMPLE).unwrap();
        assert_eq!(profile.id, id("work"));
        assert_eq!(profile.name, "Work phone");
        assert_eq!(profile.created_unix, 1_794_700_000);
        assert_eq!(profile.graphics.mode, GraphicsMode::Auto);
    }

    #[test]
    fn a_profile_survives_writing_and_reading() {
        let mut original = profile();
        original.name = "Tëst phone ✓".to_string();
        original.display = Display {
            width: 1080,
            height: 2400,
            dpi: 420,
        };
        original.graphics.mode = GraphicsMode::Software;
        original.adb.local_port = 5599;

        let text = original.to_toml().unwrap();
        assert_eq!(Profile::from_toml(&text).unwrap(), original);
    }

    #[test]
    fn sections_left_out_of_the_file_take_their_defaults() {
        let text = "schema = 1\nid = \"a\"\nname = \"A\"\nengine = \"container\"\nandroid = \"14\"\ncreated_unix = 1\n";
        let profile = Profile::from_toml(text).unwrap();
        assert_eq!(profile.display, Display::default());
        assert_eq!(profile.graphics, Graphics::default());
        assert_eq!(profile.adb, Adb::default());
    }

    #[test]
    fn a_profile_is_saved_to_and_loaded_from_its_folder() {
        let temp = tempfile::tempdir().unwrap();
        let phone = phone_dir(&temp, "work");
        let original = profile();
        original.save(&phone).unwrap();

        assert!(phone.profile_path().is_file());
        assert!(!phone.path().join("phone.toml.tmp").exists());
        assert_eq!(Profile::load(&phone).unwrap(), original);
    }

    // ---- validation ----

    fn assert_invalid(change: impl FnOnce(&mut Profile), expected: &str) {
        let mut profile = profile();
        change(&mut profile);
        let error = profile.validate().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Usage, "{error}");
        assert!(error.message().contains(expected), "{error}");
        assert!(error.hint().is_some());
    }

    #[test]
    fn each_invalid_value_is_rejected() {
        assert_invalid(|p| p.name = String::new(), "name is empty");
        assert_invalid(|p| p.name = "   ".to_string(), "name is empty");
        assert_invalid(|p| p.name = "x".repeat(65), "longer than 64");
        assert_invalid(|p| p.name = "two\nlines".to_string(), "control character");
        assert_invalid(|p| p.android = String::new(), "not an Android version");
        assert_invalid(
            |p| p.android = "latest".to_string(),
            "not an Android version",
        );
        assert_invalid(|p| p.android = ".14".to_string(), "not an Android version");
        assert_invalid(|p| p.display.width = 239, "width is 239");
        assert_invalid(|p| p.display.width = 4097, "width is 4097");
        assert_invalid(|p| p.display.height = 0, "height is 0");
        assert_invalid(|p| p.display.dpi = 71, "density is 71");
        assert_invalid(|p| p.display.dpi = 641, "density is 641");
        assert_invalid(|p| p.adb.local_port = 80, "adb port is 80");
        assert_invalid(|p| p.adb.local_port = 1023, "adb port is 1023");
    }

    #[test]
    fn values_at_the_edges_of_the_ranges_are_accepted() {
        let mut profile = profile();
        profile.name = "x".repeat(64);
        profile.display = Display {
            width: 240,
            height: 4096,
            dpi: 72,
        };
        profile.adb.local_port = 1024;
        profile.android = "8.1".to_string();
        profile.validate().unwrap();

        profile.display.dpi = 640;
        profile.adb.local_port = 65535;
        profile.validate().unwrap();
    }

    #[test]
    fn invalid_values_in_a_file_are_rejected_on_reading() {
        let text = EXAMPLE.replace("dpi = 320", "dpi = 9000");
        let error = Profile::from_toml(&text).unwrap_err();
        assert!(error.message().contains("density is 9000"), "{error}");
    }

    #[test]
    fn an_invalid_id_in_a_file_is_rejected() {
        let text = EXAMPLE.replace("id = \"work\"", "id = \"Not Valid\"");
        let error = Profile::from_toml(&text).unwrap_err();
        assert!(error.message().contains("not a valid phone id"), "{error}");
    }

    #[test]
    fn unknown_settings_and_values_are_rejected() {
        let extra = EXAMPLE.replace("android = \"14\"", "android = \"14\"\ncolour = \"red\"");
        let error = Profile::from_toml(&extra).unwrap_err();
        assert!(error.message().contains("colour"), "{error}");

        let mode = EXAMPLE.replace("mode = \"auto\"", "mode = \"turbo\"");
        let error = Profile::from_toml(&mode).unwrap_err();
        assert!(error.message().contains("turbo"), "{error}");

        let engine = EXAMPLE.replace("engine = \"container\"", "engine = \"magic\"");
        assert!(Profile::from_toml(&engine).is_err());
    }

    #[test]
    fn text_that_is_not_toml_is_rejected() {
        let error = Profile::from_toml("this is not toml").unwrap_err();
        assert!(error.message().contains("not valid TOML"), "{error}");
    }

    // ---- schema ----

    #[test]
    fn a_file_from_a_newer_verone_is_refused_with_an_upgrade_hint() {
        let text = EXAMPLE.replace("schema = 1", "schema = 2");
        let error = Profile::from_toml(&text).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Failed);
        assert!(error.message().contains("file format 2"), "{error}");
        assert!(error.hint().unwrap().contains("Upgrade Verone"));
    }

    #[test]
    fn a_newer_file_is_refused_even_if_its_other_settings_are_unknown() {
        let text = "schema = 7\nsomething_new = true\n";
        let error = Profile::from_toml(text).unwrap_err();
        assert!(error.message().contains("file format 7"), "{error}");
    }

    #[test]
    fn schema_zero_and_a_missing_schema_are_refused() {
        let zero = EXAMPLE.replace("schema = 1", "schema = 0");
        let error = Profile::from_toml(&zero).unwrap_err();
        assert!(error.message().contains("not supported"), "{error}");

        let missing = EXAMPLE.replace("schema = 1\n", "");
        let error = Profile::from_toml(&missing).unwrap_err();
        assert!(error.message().contains("no schema number"), "{error}");
    }

    #[test]
    fn a_profile_with_the_wrong_schema_cannot_be_saved() {
        let temp = tempfile::tempdir().unwrap();
        let phone = phone_dir(&temp, "work");
        let mut profile = profile();
        profile.schema = 2;
        assert!(profile.save(&phone).is_err());
        assert!(!phone.profile_path().exists());
    }

    // ---- files ----

    #[test]
    fn loading_names_the_file_in_its_error() {
        let temp = tempfile::tempdir().unwrap();
        let phone = phone_dir(&temp, "work");
        fs::write(
            phone.profile_path(),
            EXAMPLE.replace("schema = 1", "schema = 2"),
        )
        .unwrap();
        let error = Profile::load(&phone).unwrap_err();
        assert!(error.message().contains("phone.toml"), "{error}");
        assert!(error.hint().unwrap().contains("Upgrade Verone"));
    }

    #[test]
    fn loading_a_missing_profile_fails() {
        let temp = tempfile::tempdir().unwrap();
        let phone = phone_dir(&temp, "work");
        let error = Profile::load(&phone).unwrap_err();
        assert!(error.message().contains("could not read"), "{error}");
    }

    #[test]
    fn a_profile_in_the_wrong_folder_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        let phone = phone_dir(&temp, "other");
        fs::write(phone.profile_path(), EXAMPLE).unwrap();
        let error = Profile::load(&phone).unwrap_err();
        assert!(error.message().contains("says its id is 'work'"), "{error}");

        assert!(profile().save(&phone).is_err());
    }

    // ---- state ----

    #[test]
    fn a_phone_that_never_started_has_the_default_state() {
        let temp = tempfile::tempdir().unwrap();
        let phone = phone_dir(&temp, "work");
        assert_eq!(State::load(&phone).unwrap(), State::default());
        assert_eq!(State::default().to_toml().unwrap().trim(), "schema = 1");
    }

    #[test]
    fn state_survives_saving_and_loading() {
        let temp = tempfile::tempdir().unwrap();
        let phone = phone_dir(&temp, "work");
        let state = State {
            schema: SCHEMA,
            last_boot: Some(LastBoot {
                graphics: GraphicsUsed::Software,
                fell_back_to_software: true,
            }),
        };
        state.save(&phone).unwrap();
        assert_eq!(State::load(&phone).unwrap(), state);
    }

    #[test]
    fn a_newer_or_broken_state_file_is_refused_and_called_safe_to_delete() {
        let temp = tempfile::tempdir().unwrap();
        let phone = phone_dir(&temp, "work");

        fs::write(phone.state_path(), "schema = 3\n").unwrap();
        let error = State::load(&phone).unwrap_err();
        assert!(error.message().contains("file format 3"), "{error}");
        assert!(error.hint().unwrap().contains("safe to delete"));

        fs::write(phone.state_path(), "schema = 1\nsurprise = 1\n").unwrap();
        assert!(State::load(&phone).is_err());
    }
}
