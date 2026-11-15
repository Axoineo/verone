//! Where Verone keeps phones on disk.
//!
//! Everything lives under one folder, the data home:
//!
//! ```text
//! <data home>/phones/<id>/phone.toml   what the user chose
//! <data home>/phones/<id>/state.toml   what Verone observed
//! <data home>/phones/<id>/.lock        held while a command works on the phone
//! <data home>/phones/<id>/data/        Android's data, owned by root
//! ```

use std::ffi::OsString;
use std::fmt;
use std::fs::{self, DirBuilder, File, OpenOptions, TryLockError};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};

/// Longest allowed phone id, in characters.
pub const MAX_ID_LENGTH: usize = 31;

/// How long a command waits for another command to release a phone.
pub const LOCK_TIMEOUT: Duration = Duration::from_secs(10);

/// How often a held lock is retried.
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Folders are created readable by their owner only.
const PRIVATE_DIR_MODE: u32 = 0o700;

const PHONES_DIR: &str = "phones";
const PROFILE_FILE: &str = "phone.toml";
const STATE_FILE: &str = "state.toml";
const LOCK_FILE: &str = ".lock";
const DATA_DIR: &str = "data";

/// The name a user gives a phone. It is also the name of the phone's folder.
///
/// Lowercase letters, digits and hyphens; 1 to 31 characters; starts with a letter
/// or digit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhoneId(String);

impl PhoneId {
    /// Checks and wraps an id.
    pub fn new(id: &str) -> Result<Self> {
        let problem = if id.is_empty() {
            Some("it is empty".to_string())
        } else if id.chars().count() > MAX_ID_LENGTH {
            Some(format!("it is longer than {MAX_ID_LENGTH} characters"))
        } else if let Some(bad) = id
            .chars()
            .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-'))
        {
            Some(format!("it contains '{bad}'"))
        } else if id.starts_with('-') {
            Some("it starts with a hyphen".to_string())
        } else {
            None
        };
        match problem {
            None => Ok(Self(id.to_string())),
            Some(problem) => Err(Error::usage(format!(
                "'{id}' is not a valid phone id: {problem}"
            ))
            .with_hint(format!(
                "Use lowercase letters, digits and hyphens, 1 to {MAX_ID_LENGTH} \
                         characters, starting with a letter or digit. Example: work-phone"
            ))),
        }
    }

    /// The id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PhoneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for PhoneId {
    type Err = Error;

    fn from_str(id: &str) -> Result<Self> {
        Self::new(id)
    }
}

/// Works out the data home from the three environment values that can set it.
///
/// `VERONE_HOME` wins, then `XDG_DATA_HOME/verone`, then `HOME/.local/share/verone`.
/// Empty values count as unset. A relative `XDG_DATA_HOME` is ignored, as the XDG
/// specification requires; a relative `VERONE_HOME` is an error because the user
/// set it on purpose.
pub fn resolve_home(
    verone_home: Option<OsString>,
    xdg_data_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf> {
    let set = |value: Option<OsString>| value.filter(|v| !v.is_empty()).map(PathBuf::from);

    if let Some(path) = set(verone_home) {
        if path.is_absolute() {
            return Ok(path);
        }
        return Err(Error::usage(format!(
            "VERONE_HOME must be a full path, but it is '{}'",
            path.display()
        ))
        .with_hint("Set VERONE_HOME to a path that starts with /, or unset it."));
    }
    if let Some(path) = set(xdg_data_home).filter(|p| p.is_absolute()) {
        return Ok(path.join("verone"));
    }
    if let Some(path) = set(home).filter(|p| p.is_absolute()) {
        return Ok(path.join(".local").join("share").join("verone"));
    }
    Err(
        Error::failed("cannot work out where to keep phones: HOME is not set")
            .with_hint("Set VERONE_HOME to the folder Verone should use."),
    )
}

/// The data home and the phones inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Storage {
    home: PathBuf,
}

impl Storage {
    /// Uses `home` as the data home. Nothing is created until a phone is.
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self { home: home.into() }
    }

    /// Uses the data home chosen by the environment. See [`resolve_home`].
    pub fn from_env() -> Result<Self> {
        resolve_home(
            std::env::var_os("VERONE_HOME"),
            std::env::var_os("XDG_DATA_HOME"),
            std::env::var_os("HOME"),
        )
        .map(Self::new)
    }

    /// The data home.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// The folder that holds one folder per phone.
    pub fn phones_dir(&self) -> PathBuf {
        self.home.join(PHONES_DIR)
    }

    /// The paths of a phone, whether or not it exists.
    pub fn phone(&self, id: &PhoneId) -> PhoneDir {
        PhoneDir {
            id: id.clone(),
            path: self.phones_dir().join(id.as_str()),
        }
    }

    /// Creates the folder for a new phone, with an empty data folder inside.
    pub fn create_phone(&self, id: &PhoneId) -> Result<PhoneDir> {
        let phone = self.phone(id);
        if phone.exists() {
            return Err(
                Error::failed(format!("a phone named '{id}' already exists")).with_hint(
                    "Choose another id, or remove the existing phone with verone delete.",
                ),
            );
        }
        create_private_dir_all(&self.phones_dir())?;
        create_private_dir_all(&phone.data_dir())?;
        Ok(phone)
    }

    /// Finds an existing phone.
    pub fn find_phone(&self, id: &PhoneId) -> Result<PhoneDir> {
        let phone = self.phone(id);
        if phone.exists() {
            Ok(phone)
        } else {
            Err(Error::not_found(format!("there is no phone named '{id}'"))
                .with_hint("Run verone list to see the phones that exist."))
        }
    }

    /// The ids of all phones, in alphabetical order. Folders whose names are not
    /// valid ids are ignored.
    pub fn list_phones(&self) -> Result<Vec<PhoneId>> {
        let dir = self.phones_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(io_error("read", &dir, &error)),
        };
        let mut ids = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| io_error("read", &dir, &error))?;
            let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
            if !is_dir {
                continue;
            }
            if let Some(id) = entry
                .file_name()
                .to_str()
                .and_then(|n| PhoneId::new(n).ok())
            {
                ids.push(id);
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Removes a phone's folder and everything in it.
    ///
    /// Android's data is owned by root, so a phone that has been started must have
    /// its data folder emptied by the engine first.
    pub fn remove_phone(&self, id: &PhoneId) -> Result<()> {
        let phone = self.find_phone(id)?;
        fs::remove_dir_all(phone.path()).map_err(|error| {
            io_error("remove", phone.path(), &error).with_hint(
                "If the phone has been started, its data must be removed through the engine first.",
            )
        })
    }

    /// Takes the phone's lock, waiting up to [`LOCK_TIMEOUT`] for another command.
    pub fn lock_phone(&self, id: &PhoneId) -> Result<PhoneLock> {
        self.lock_phone_within(id, LOCK_TIMEOUT)
    }

    /// Takes the phone's lock, waiting up to `timeout` for another command.
    pub fn lock_phone_within(&self, id: &PhoneId, timeout: Duration) -> Result<PhoneLock> {
        let phone = self.find_phone(id)?;
        let path = phone.lock_path();
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|error| io_error("open", &path, &error))?;

        let deadline = Instant::now() + timeout;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(PhoneLock { _file: file }),
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Error(error)) => return Err(io_error("lock", &path, &error)),
            }
            if Instant::now() >= deadline {
                return Err(Error::busy(format!(
                    "another verone command is working on the phone '{id}'"
                ))
                .with_hint("Wait for it to finish, then try again."));
            }
            thread::sleep(LOCK_POLL_INTERVAL);
        }
    }
}

/// The folder of one phone and the paths inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhoneDir {
    id: PhoneId,
    path: PathBuf,
}

impl PhoneDir {
    /// The phone's id.
    pub fn id(&self) -> &PhoneId {
        &self.id
    }

    /// The phone's folder.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// True if the phone's folder exists.
    pub fn exists(&self) -> bool {
        self.path.is_dir()
    }

    /// The file holding what the user chose.
    pub fn profile_path(&self) -> PathBuf {
        self.path.join(PROFILE_FILE)
    }

    /// The file holding what Verone observed.
    pub fn state_path(&self) -> PathBuf {
        self.path.join(STATE_FILE)
    }

    /// The file locked while a command works on the phone.
    pub fn lock_path(&self) -> PathBuf {
        self.path.join(LOCK_FILE)
    }

    /// Android's data folder.
    pub fn data_dir(&self) -> PathBuf {
        self.path.join(DATA_DIR)
    }
}

/// Proof that this process is the only one working on a phone. The lock is
/// released when this value is dropped, or when the process ends for any reason.
#[derive(Debug)]
pub struct PhoneLock {
    _file: File,
}

fn create_private_dir_all(path: &Path) -> Result<()> {
    DirBuilder::new()
        .recursive(true)
        .mode(PRIVATE_DIR_MODE)
        .create(path)
        .map_err(|error| io_error("create", path, &error))
}

fn io_error(action: &str, path: &Path, error: &std::io::Error) -> Error {
    Error::failed(format!("could not {action} {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};

    fn id(text: &str) -> PhoneId {
        PhoneId::new(text).unwrap()
    }

    fn os(text: &str) -> Option<OsString> {
        Some(OsString::from(text))
    }

    // ---- phone ids ----

    #[test]
    fn valid_ids_are_accepted() {
        for text in [
            "a",
            "7",
            "work",
            "work-phone",
            "a1-b2-c3",
            "0abc",
            &"x".repeat(31),
        ] {
            assert_eq!(id(text).as_str(), text);
        }
    }

    #[test]
    fn every_invalid_id_form_is_rejected_as_a_usage_error() {
        let long = "x".repeat(32);
        let cases = [
            ("", "empty"),
            (long.as_str(), "longer than 31"),
            ("-work", "starts with a hyphen"),
            ("Work", "contains 'W'"),
            ("work phone", "contains ' '"),
            ("work_phone", "contains '_'"),
            ("work.phone", "contains '.'"),
            ("work/phone", "contains '/'"),
            ("..", "contains '.'"),
            ("phöne", "contains 'ö'"),
            ("work\n", "contains '\n'"),
        ];
        for (text, reason) in cases {
            let error = PhoneId::new(text).unwrap_err();
            assert_eq!(error.kind(), ErrorKind::Usage, "{text:?}");
            assert!(error.message().contains(reason), "{text:?}: {error}");
            assert!(error.hint().is_some());
        }
    }

    #[test]
    fn ids_parse_from_text_and_print_as_text() {
        let parsed: PhoneId = "work".parse().unwrap();
        assert_eq!(parsed.to_string(), "work");
        assert!("Bad Id".parse::<PhoneId>().is_err());
    }

    // ---- data home ----

    #[test]
    fn verone_home_wins() {
        let home = resolve_home(os("/v"), os("/xdg"), os("/home/u")).unwrap();
        assert_eq!(home, PathBuf::from("/v"));
    }

    #[test]
    fn xdg_data_home_is_used_next() {
        let home = resolve_home(None, os("/xdg"), os("/home/u")).unwrap();
        assert_eq!(home, PathBuf::from("/xdg/verone"));
    }

    #[test]
    fn home_is_the_fallback() {
        let home = resolve_home(None, None, os("/home/u")).unwrap();
        assert_eq!(home, PathBuf::from("/home/u/.local/share/verone"));
    }

    #[test]
    fn empty_values_count_as_unset() {
        let home = resolve_home(os(""), os(""), os("/home/u")).unwrap();
        assert_eq!(home, PathBuf::from("/home/u/.local/share/verone"));
    }

    #[test]
    fn a_relative_xdg_data_home_is_ignored() {
        let home = resolve_home(None, os("relative"), os("/home/u")).unwrap();
        assert_eq!(home, PathBuf::from("/home/u/.local/share/verone"));
    }

    #[test]
    fn a_relative_verone_home_is_an_error() {
        let error = resolve_home(os("relative"), None, os("/home/u")).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Usage);
        assert!(error.message().contains("VERONE_HOME"));
    }

    #[test]
    fn no_home_at_all_is_an_error_with_a_hint() {
        let error = resolve_home(None, None, None).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Failed);
        assert!(error.hint().unwrap().contains("VERONE_HOME"));
    }

    // ---- phone folders ----

    #[test]
    fn paths_follow_the_documented_layout() {
        let storage = Storage::new("/v");
        let phone = storage.phone(&id("work"));
        assert_eq!(storage.home(), Path::new("/v"));
        assert_eq!(storage.phones_dir(), PathBuf::from("/v/phones"));
        assert_eq!(phone.path(), Path::new("/v/phones/work"));
        assert_eq!(
            phone.profile_path(),
            PathBuf::from("/v/phones/work/phone.toml")
        );
        assert_eq!(
            phone.state_path(),
            PathBuf::from("/v/phones/work/state.toml")
        );
        assert_eq!(phone.lock_path(), PathBuf::from("/v/phones/work/.lock"));
        assert_eq!(phone.data_dir(), PathBuf::from("/v/phones/work/data"));
        assert_eq!(phone.id(), &id("work"));
    }

    #[test]
    fn creating_a_phone_makes_private_folders() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("home"));
        let phone = storage.create_phone(&id("work")).unwrap();

        assert!(phone.exists());
        assert!(phone.data_dir().is_dir());
        for dir in [
            storage.home().to_path_buf(),
            storage.phones_dir(),
            phone.path().to_path_buf(),
        ] {
            let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{}", dir.display());
        }
    }

    #[test]
    fn creating_the_same_phone_twice_fails() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path());
        storage.create_phone(&id("work")).unwrap();
        let error = storage.create_phone(&id("work")).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Failed);
        assert!(error.message().contains("already exists"));
    }

    #[test]
    fn finding_a_missing_phone_is_not_found() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path());
        let error = storage.find_phone(&id("ghost")).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
        assert_eq!(error.exit_code(), 4);
    }

    #[test]
    fn listing_is_empty_before_any_phone_exists() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("never-created"));
        assert_eq!(storage.list_phones().unwrap(), Vec::<PhoneId>::new());
    }

    #[test]
    fn listing_is_sorted_and_skips_stray_entries() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path());
        for name in ["work", "alpha", "test-2"] {
            storage.create_phone(&id(name)).unwrap();
        }
        fs::create_dir(storage.phones_dir().join("Not A Phone")).unwrap();
        fs::write(storage.phones_dir().join("file"), "x").unwrap();

        assert_eq!(
            storage.list_phones().unwrap(),
            vec![id("alpha"), id("test-2"), id("work")]
        );
    }

    #[test]
    fn removing_a_phone_deletes_its_folder() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path());
        let phone = storage.create_phone(&id("work")).unwrap();
        fs::write(phone.profile_path(), "schema = 1\n").unwrap();

        storage.remove_phone(&id("work")).unwrap();
        assert!(!phone.exists());
        assert_eq!(
            storage.remove_phone(&id("work")).unwrap_err().kind(),
            ErrorKind::NotFound
        );
    }

    // ---- locking ----

    #[test]
    fn locking_a_missing_phone_is_not_found() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path());
        let error = storage.lock_phone(&id("ghost")).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn a_held_lock_makes_a_second_attempt_busy_until_released() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path());
        storage.create_phone(&id("work")).unwrap();

        let first = storage.lock_phone(&id("work")).unwrap();
        let started = Instant::now();
        let error = storage
            .lock_phone_within(&id("work"), Duration::from_millis(200))
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Busy);
        assert_eq!(error.exit_code(), 5);
        assert!(started.elapsed() >= Duration::from_millis(200));

        drop(first);
        storage
            .lock_phone_within(&id("work"), Duration::from_millis(200))
            .unwrap();
    }

    #[test]
    fn locks_on_different_phones_do_not_interfere() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path());
        storage.create_phone(&id("one")).unwrap();
        storage.create_phone(&id("two")).unwrap();
        let _one = storage.lock_phone(&id("one")).unwrap();
        let _two = storage
            .lock_phone_within(&id("two"), Duration::from_millis(200))
            .unwrap();
    }

    const CHILD_HOME_VAR: &str = "VERONE_TEST_LOCK_CHILD_HOME";

    /// Not a test of its own: the second process for the test below. It takes the
    /// lock, signals that it has it, and holds it until it is killed.
    #[test]
    #[ignore = "helper process for two_processes_contend_for_one_lock"]
    fn lock_holder_child() {
        let Some(home) = std::env::var_os(CHILD_HOME_VAR) else {
            return;
        };
        let storage = Storage::new(PathBuf::from(&home));
        let _lock = storage.lock_phone(&id("work")).unwrap();
        fs::write(PathBuf::from(&home).join("child-has-lock"), "").unwrap();
        thread::sleep(Duration::from_secs(60));
    }

    #[test]
    fn two_processes_contend_for_one_lock() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path());
        storage.create_phone(&id("work")).unwrap();

        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "storage::tests::lock_holder_child"])
            .env(CHILD_HOME_VAR, temp.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let marker = temp.path().join("child-has-lock");
        let deadline = Instant::now() + Duration::from_secs(20);
        while !marker.exists() {
            assert!(Instant::now() < deadline, "the child never took the lock");
            thread::sleep(Duration::from_millis(20));
        }

        let error = storage
            .lock_phone_within(&id("work"), Duration::from_millis(300))
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Busy);

        // Ending the other process, even abruptly, releases the lock.
        child.kill().unwrap();
        child.wait().unwrap();
        storage
            .lock_phone_within(&id("work"), Duration::from_secs(2))
            .unwrap();
    }
}
