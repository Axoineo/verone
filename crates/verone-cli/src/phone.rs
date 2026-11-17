//! The commands that work on one phone or list phones.
//!
//! Every command takes the engine through the [`Engine`] trait, so they are tested
//! with the fake engine and no container runtime.

use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use verone_core::engine::{BootReport, Engine, PhoneStatus, any_failed};
use verone_core::process::{Cmd, Runner};
use verone_core::profile::{Display, GraphicsMode, GraphicsUsed, Profile, State};
use verone_core::storage::{PhoneDir, PhoneId, Storage};
use verone_core::{Error, Result};
use verone_engine_container::adb::Adb;

use crate::output::{self, Mode};

/// The screen-mirroring program used by `verone open`.
pub const SCRCPY_PROGRAM: &str = "scrcpy";

/// Time limit for installing an app.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(300);

/// Time limit for short adb queries.
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);

/// What the phone commands work with.
pub struct Env<'a, E: Engine> {
    /// Where phones are kept.
    pub storage: &'a Storage,
    /// The engine that runs them.
    pub engine: &'a E,
    /// How results are printed.
    pub mode: Mode,
}

// ---------------------------------------------------------------- create

/// The choices given to `verone create`.
#[derive(Debug, Clone)]
pub struct CreateOptions {
    pub id: String,
    pub name: Option<String>,
    pub android: String,
    pub size: Option<String>,
    pub dpi: Option<u32>,
    pub graphics: GraphicsMode,
    pub adb_port: u16,
}

/// Reads a screen size written as `WIDTHxHEIGHT`.
pub fn parse_size(text: &str) -> Result<(u32, u32)> {
    let parsed = text
        .split_once(['x', 'X'])
        .and_then(|(width, height)| Some((width.parse().ok()?, height.parse().ok()?)));
    parsed.ok_or_else(|| {
        Error::usage(format!("'{text}' is not a screen size"))
            .with_hint("Write the size as WIDTHxHEIGHT, for example 720x1280.")
    })
}

/// `verone create`: make a new phone. It is not started.
pub fn create<E: Engine, W: Write>(
    env: &Env<E>,
    options: &CreateOptions,
    out: &mut W,
) -> Result<()> {
    let id = PhoneId::new(&options.id)?;
    let mut profile = Profile::new(id.clone());
    if let Some(name) = &options.name {
        profile.name = name.clone();
    }
    profile.android = options.android.clone();
    if let Some(size) = &options.size {
        let (width, height) = parse_size(size)?;
        profile.display = Display {
            width,
            height,
            ..profile.display
        };
    }
    if let Some(dpi) = options.dpi {
        profile.display.dpi = dpi;
    }
    profile.graphics.mode = options.graphics;
    profile.adb.local_port = options.adb_port;

    // Reject bad settings before anything is written.
    profile.validate()?;
    env.engine.supports_android(&profile.android)?;

    let phone = env.storage.create_phone(&id)?;
    if let Err(error) = profile.save(&phone) {
        // Do not leave a half-made phone behind.
        let _ = env.storage.remove_phone(&id);
        return Err(error);
    }

    output::emit(env.mode, out, &profile, |profile| {
        format!(
            "Created the phone '{}' with Android {}.\nStart it with: verone start {}",
            profile.id, profile.android, profile.id
        )
    })
}

// ---------------------------------------------------------------- list, info

#[derive(Debug, Serialize)]
struct ListRow {
    id: String,
    name: String,
    android: String,
    status: String,
}

/// `verone list`: every phone and its state.
pub fn list<E: Engine, W: Write>(env: &Env<E>, out: &mut W) -> Result<()> {
    let mut rows = Vec::new();
    for id in env.storage.list_phones()? {
        let phone = env.storage.phone(&id);
        let row = match Profile::load(&phone) {
            Ok(profile) => ListRow {
                id: id.to_string(),
                name: profile.name,
                android: profile.android,
                status: env.engine.status(&id)?.to_string(),
            },
            Err(error) => ListRow {
                id: id.to_string(),
                name: "?".to_string(),
                android: "?".to_string(),
                status: format!("unreadable: {}", error.message()),
            },
        };
        rows.push(row);
    }
    output::emit(env.mode, out, &rows, |rows| format_list(rows))
}

fn format_list(rows: &[ListRow]) -> String {
    if rows.is_empty() {
        return "There are no phones yet. Create one with: verone create <id>".to_string();
    }
    let width = |pick: fn(&ListRow) -> &str, title: &str| {
        rows.iter()
            .map(|row| pick(row).chars().count())
            .chain([title.len()])
            .max()
            .unwrap_or(0)
    };
    let id_width = width(|row| &row.id, "ID");
    let name_width = width(|row| &row.name, "NAME");
    let mut text = format!(
        "{:<id_width$}  {:<name_width$}  ANDROID  STATUS\n",
        "ID", "NAME"
    );
    for row in rows {
        text.push_str(&format!(
            "{:<id_width$}  {:<name_width$}  {:<7}  {}\n",
            row.id, row.name, row.android, row.status
        ));
    }
    text
}

#[derive(Debug, Serialize)]
struct Info {
    id: String,
    name: String,
    android: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<String>,
    graphics_mode: GraphicsMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    graphics_in_use: Option<GraphicsUsed>,
    display: Display,
    #[serde(skip_serializing_if = "Option::is_none")]
    data_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    uptime_seconds: Option<u64>,
    created_unix: u64,
}

/// `verone info`: everything about one phone.
pub fn info<E: Engine, W: Write>(env: &Env<E>, id: &str, out: &mut W) -> Result<()> {
    let id = PhoneId::new(id)?;
    let phone = env.storage.find_phone(&id)?;
    let profile = Profile::load(&phone)?;
    let state = State::load(&phone)?;
    let status = env.engine.status(&id)?;
    let running = status == PhoneStatus::Running;
    let details = env.engine.details(&id, &phone)?;

    let info = Info {
        id: id.to_string(),
        name: profile.name,
        android: profile.android,
        status: status.to_string(),
        address: if running {
            env.engine.control_endpoint(&id).ok().map(|e| e.to_string())
        } else {
            None
        },
        graphics_mode: profile.graphics.mode,
        graphics_in_use: state
            .last_boot
            .filter(|_| running)
            .map(|boot| boot.graphics),
        display: profile.display,
        data_bytes: details.data_bytes,
        uptime_seconds: details.uptime.map(|uptime| uptime.as_secs()),
        created_unix: profile.created_unix,
    };
    output::emit(env.mode, out, &info, format_info)
}

fn format_info(info: &Info) -> String {
    let mut lines = vec![
        format!("Phone:     {} ({})", info.id, info.name),
        format!("Android:   {}", info.android),
        format!("Status:    {}", info.status),
    ];
    if let Some(uptime) = info.uptime_seconds {
        lines.push(format!("On for:    {}", format_duration(uptime)));
    }
    if let Some(address) = &info.address {
        lines.push(format!("Address:   {address}"));
    }
    lines.push(format!(
        "Screen:    {}x{} at {} dpi",
        info.display.width, info.display.height, info.display.dpi
    ));
    let mode = graphics_mode_word(info.graphics_mode);
    lines.push(match info.graphics_in_use {
        Some(used) => format!("Graphics:  {mode}, using {}", graphics_used_word(used)),
        None => format!("Graphics:  {mode}"),
    });
    if let Some(bytes) = info.data_bytes {
        lines.push(format!("Data:      {}", format_size(bytes)));
    }
    lines.join("\n")
}

fn graphics_mode_word(mode: GraphicsMode) -> &'static str {
    match mode {
        GraphicsMode::Auto => "auto",
        GraphicsMode::Software => "software",
        GraphicsMode::Gpu => "gpu",
    }
}

fn graphics_used_word(used: GraphicsUsed) -> &'static str {
    match used {
        GraphicsUsed::Software => "software rendering",
        GraphicsUsed::Gpu => "the GPU",
    }
}

fn format_duration(seconds: u64) -> String {
    let (hours, minutes) = (seconds / 3600, seconds % 3600 / 60);
    match (hours, minutes) {
        (0, 0) => format!("{seconds} seconds"),
        (0, _) => format!("{minutes} min"),
        _ => format!("{hours} h {minutes} min"),
    }
}

fn format_size(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if bytes >= 1024 * MIB {
        let tenths = bytes * 10 / (1024 * MIB);
        format!("{}.{} GB", tenths / 10, tenths % 10)
    } else {
        format!("{} MB", bytes / MIB)
    }
}

// ---------------------------------------------------------------- start, stop, restart

#[derive(Debug, Serialize)]
struct Started {
    id: String,
    status: &'static str,
    already_running: bool,
    boot_seconds: f32,
    graphics: GraphicsUsed,
    fell_back_to_software: bool,
}

/// Boots a phone whose lock the caller holds.
fn start_locked<E: Engine>(env: &Env<E>, phone: &PhoneDir) -> Result<BootReport> {
    let profile = Profile::load(phone)?;
    if any_failed(&env.engine.check_host()?) {
        return Err(Error::host_check("this computer cannot run phones yet")
            .with_hint("Run verone doctor to see what to fix."));
    }
    env.engine.start(&profile, phone)
}

/// Shuts a phone down whose lock the caller holds. Returns false if it was
/// already off.
fn stop_locked<E: Engine>(env: &Env<E>, id: &PhoneId) -> Result<bool> {
    match env.engine.status(id)? {
        PhoneStatus::NotCreated | PhoneStatus::Stopped => Ok(false),
        _ => env.engine.stop(id).map(|()| true),
    }
}

fn report_start<W: Write>(
    mode: Mode,
    id: &PhoneId,
    report: &BootReport,
    out: &mut W,
) -> Result<()> {
    let started = Started {
        id: id.to_string(),
        status: "running",
        already_running: report.already_running,
        boot_seconds: report.duration.as_secs_f32(),
        graphics: report.graphics,
        fell_back_to_software: report.fell_back_to_software,
    };
    output::emit(mode, out, &started, |started| {
        if started.already_running {
            return format!("The phone '{}' is already running.", started.id);
        }
        let mut text = format!(
            "The phone '{}' is running. It booted in {:.1} seconds using {}.",
            started.id,
            started.boot_seconds,
            graphics_used_word(started.graphics)
        );
        if started.fell_back_to_software {
            text.push_str(
                "\nGPU graphics did not start on this computer, so the phone uses software rendering.",
            );
        }
        text
    })
}

/// `verone start`: boot a phone and wait until Android is ready.
pub fn start<E: Engine, W: Write>(env: &Env<E>, id: &str, out: &mut W) -> Result<()> {
    let id = PhoneId::new(id)?;
    let phone = env.storage.find_phone(&id)?;
    let _lock = env.storage.lock_phone(&id)?;
    let report = start_locked(env, &phone)?;
    report_start(env.mode, &id, &report, out)
}

#[derive(Debug, Serialize)]
struct Stopped {
    id: String,
    status: &'static str,
    already_stopped: bool,
}

/// `verone stop`: shut Android down cleanly.
pub fn stop<E: Engine, W: Write>(env: &Env<E>, id: &str, out: &mut W) -> Result<()> {
    let id = PhoneId::new(id)?;
    env.storage.find_phone(&id)?;
    let _lock = env.storage.lock_phone(&id)?;
    let was_running = stop_locked(env, &id)?;
    let stopped = Stopped {
        id: id.to_string(),
        status: "stopped",
        already_stopped: !was_running,
    };
    output::emit(env.mode, out, &stopped, |stopped| {
        if stopped.already_stopped {
            format!("The phone '{}' is already stopped.", stopped.id)
        } else {
            format!("The phone '{}' is stopped.", stopped.id)
        }
    })
}

/// `verone restart`: stop, then start.
pub fn restart<E: Engine, W: Write>(env: &Env<E>, id: &str, out: &mut W) -> Result<()> {
    let id = PhoneId::new(id)?;
    let phone = env.storage.find_phone(&id)?;
    let _lock = env.storage.lock_phone(&id)?;
    stop_locked(env, &id)?;
    let report = start_locked(env, &phone)?;
    report_start(env.mode, &id, &report, out)
}

// ---------------------------------------------------------------- open, adb, install

/// The programs the connection commands run.
pub struct Tools<'a> {
    /// The adb helper.
    pub adb: &'a Adb,
    /// Runs scrcpy.
    pub runner: Runner,
    /// The scrcpy program and any arguments placed before Verone's own.
    pub scrcpy: Vec<String>,
}

/// `verone open`: show the phone in a window, starting it first if needed.
pub fn open<E: Engine>(env: &Env<E>, tools: &Tools, id: &str, extra: &[String]) -> Result<()> {
    let id = PhoneId::new(id)?;
    let phone = env.storage.find_phone(&id)?;
    let profile = Profile::load(&phone)?;
    {
        let _lock = env.storage.lock_phone(&id)?;
        start_locked(env, &phone)?;
    }
    let endpoint = env.engine.control_endpoint(&id)?;
    tools.adb.ready(&endpoint)?;

    let Some((program, leading)) = tools.scrcpy.split_first() else {
        return Err(Error::failed("no screen-mirroring program is configured"));
    };
    // The redroid images have no Opus audio encoder, scrcpy's default. Without
    // this, scrcpy's helper inside the phone fails and no window appears.
    // Uncompressed audio needs no encoder. The user's own audio options win.
    let audio_chosen = extra
        .iter()
        .any(|option| option.starts_with("--audio") || option == "--no-audio");
    let audio: &[&str] = if audio_chosen {
        &[]
    } else {
        &["--audio-codec=raw"]
    };
    let cmd = Cmd::new(program)
        .args(leading)
        .arg("--serial")
        .arg(endpoint.to_string())
        .arg(format!("--window-title=Verone – {}", profile.name))
        .args(audio)
        .args(extra)
        .no_timeout()
        .passthrough();
    let output = tools.runner.run(&cmd).map_err(|error| {
        if error.message().contains("was not found") {
            Error::failed("scrcpy is not installed; it shows the phone's screen")
                .with_hint("Install the package scrcpy, then run the command again.")
        } else {
            error
        }
    })?;
    match output.code {
        Some(0) => Ok(()),
        Some(code) => Err(Error::failed(format!("scrcpy exited with code {code}"))
            .with_hint("Its own messages above say what went wrong.")),
        None => Err(Error::failed("scrcpy was ended by a signal")),
    }
}

/// `verone adb`: run an adb command against a running phone.
pub fn adb<E: Engine>(env: &Env<E>, tools: &Tools, id: &str, args: &[String]) -> Result<()> {
    let id = PhoneId::new(id)?;
    env.storage.find_phone(&id)?;
    if args.is_empty() {
        return Err(Error::usage("no adb command was given")
            .with_hint(format!("Example: verone adb {id} -- shell getprop")));
    }
    let endpoint = env.engine.control_endpoint(&id)?;
    match tools.adb.run_interactive(&endpoint, args)? {
        Some(0) => Ok(()),
        Some(code) => Err(Error::failed(format!("adb exited with code {code}"))),
        None => Err(Error::failed("adb was ended by a signal")),
    }
}

#[derive(Debug, Serialize)]
struct Installed {
    id: String,
    file: String,
    /// Packages that were not on the phone before. Empty when the app replaced
    /// one that was already installed.
    packages: Vec<String>,
}

fn packages(tools: &Tools, endpoint: &verone_core::engine::ControlEndpoint) -> Result<Vec<String>> {
    let output = tools.adb.run(
        endpoint,
        &["shell", "pm", "list", "packages"],
        QUERY_TIMEOUT,
    )?;
    let mut names: Vec<String> = output
        .stdout
        .lines()
        .filter_map(|line| line.trim().strip_prefix("package:"))
        .map(str::to_string)
        .collect();
    names.sort();
    Ok(names)
}

/// `verone install`: install an APK on a running phone.
pub fn install<E: Engine, W: Write>(
    env: &Env<E>,
    tools: &Tools,
    id: &str,
    apk: &Path,
    out: &mut W,
) -> Result<()> {
    let id = PhoneId::new(id)?;
    env.storage.find_phone(&id)?;
    if !apk.is_file() {
        return Err(
            Error::usage(format!("there is no file at {}", apk.display()))
                .with_hint("Give the path of an .apk file."),
        );
    }
    let endpoint = env.engine.control_endpoint(&id)?;

    let before = packages(tools, &endpoint)?;
    let apk_text = apk.display().to_string();
    let result = tools
        .adb
        .run(&endpoint, &["install", "-r", &apk_text], INSTALL_TIMEOUT)?;
    if !result.success() || !result.stdout.contains("Success") {
        let detail = format!("{}{}", result.stdout, result.stderr);
        return Err(Error::failed(format!(
            "the app could not be installed: {}",
            detail.trim()
        )));
    }
    let after = packages(tools, &endpoint)?;

    let installed = Installed {
        id: id.to_string(),
        file: apk_text,
        packages: after
            .into_iter()
            .filter(|name| !before.contains(name))
            .collect(),
    };
    output::emit(env.mode, out, &installed, |installed| {
        if installed.packages.is_empty() {
            format!(
                "Installed {} on the phone '{}'. It replaced an app that was already there.",
                installed.file, installed.id
            )
        } else {
            format!(
                "Installed {} on the phone '{}'.",
                installed.packages.join(", "),
                installed.id
            )
        }
    })
}

// ---------------------------------------------------------------- reset, delete

/// Asks a yes-or-no question. Anything but "y" or "yes" is no, including no
/// answer at all.
pub fn confirm<R: BufRead, W: Write>(question: &str, input: &mut R, out: &mut W) -> Result<bool> {
    let failed =
        |error: std::io::Error| Error::failed(format!("could not ask for confirmation: {error}"));
    write!(out, "{question} [y/N] ").map_err(failed)?;
    out.flush().map_err(failed)?;
    let mut answer = String::new();
    input.read_line(&mut answer).map_err(failed)?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

#[derive(Debug, Serialize)]
struct Changed {
    id: String,
    done: bool,
}

/// `verone reset`: erase everything on the phone and keep its settings.
pub fn reset<E: Engine, R: BufRead, W: Write>(
    env: &Env<E>,
    id: &str,
    yes: bool,
    input: &mut R,
    out: &mut W,
) -> Result<()> {
    let id = PhoneId::new(id)?;
    let phone = env.storage.find_phone(&id)?;
    let question =
        format!("Erase all apps, accounts and files on the phone '{id}'? This cannot be undone.");
    let done = yes || confirm(&question, input, out)?;
    if done {
        let _lock = env.storage.lock_phone(&id)?;
        stop_locked(env, &id)?;
        env.engine.erase_data(&phone)?;
    }
    output::emit(
        env.mode,
        out,
        &Changed {
            id: id.to_string(),
            done,
        },
        |changed| {
            if changed.done {
                format!(
                    "The phone '{}' was reset. It will start like a new phone.",
                    changed.id
                )
            } else {
                "Nothing was changed.".to_string()
            }
        },
    )
}

/// `verone delete`: remove the phone and everything on it.
pub fn delete<E: Engine, R: BufRead, W: Write>(
    env: &Env<E>,
    id: &str,
    yes: bool,
    input: &mut R,
    out: &mut W,
) -> Result<()> {
    let id = PhoneId::new(id)?;
    let phone = env.storage.find_phone(&id)?;
    let question = format!("Delete the phone '{id}' and everything on it? This cannot be undone.");
    let done = yes || confirm(&question, input, out)?;
    if done {
        {
            let _lock = env.storage.lock_phone(&id)?;
            stop_locked(env, &id)?;
            env.engine.destroy(&id)?;
            env.engine.erase_data(&phone)?;
        }
        env.storage.remove_phone(&id)?;
    }
    output::emit(
        env.mode,
        out,
        &Changed {
            id: id.to_string(),
            done,
        },
        |changed| {
            if changed.done {
                format!("The phone '{}' was deleted.", changed.id)
            } else {
                "Nothing was changed.".to_string()
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use verone_core::ErrorKind;
    use verone_core::engine::HostCheck;
    use verone_core::engine::fake::FakeEngine;

    struct Fixture {
        temp: tempfile::TempDir,
        storage: Storage,
        engine: FakeEngine,
    }

    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            Self {
                storage: Storage::new(temp.path().join("home")),
                engine: FakeEngine::new(),
                temp,
            }
        }

        fn env(&self, mode: Mode) -> Env<'_, FakeEngine> {
            Env {
                storage: &self.storage,
                engine: &self.engine,
                mode,
            }
        }

        fn human(&self) -> Env<'_, FakeEngine> {
            self.env(Mode::Human)
        }

        /// Creates a phone with default settings.
        fn phone(&self, id: &str) {
            create(&self.human(), &options(id), &mut Vec::new()).unwrap();
        }

        fn id(&self, id: &str) -> PhoneId {
            PhoneId::new(id).unwrap()
        }

        /// A stub standing in for adb and scrcpy: logs its arguments, then runs the
        /// given shell `case` arms.
        fn stub(&self, arms: &str) -> std::path::PathBuf {
            let script = self.temp.path().join("stub.sh");
            let log = self.temp.path().join("calls.log");
            fs::write(
                &script,
                format!(
                    "echo \"$*\" >> '{}'\ncase \"$*\" in\n{arms}\n*) ;;\nesac\n",
                    log.display()
                ),
            )
            .unwrap();
            script
        }

        fn calls(&self) -> Vec<String> {
            fs::read_to_string(self.temp.path().join("calls.log"))
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect()
        }
    }

    fn options(id: &str) -> CreateOptions {
        CreateOptions {
            id: id.to_string(),
            name: None,
            android: "14".to_string(),
            size: None,
            dpi: None,
            graphics: GraphicsMode::Auto,
            adb_port: 0,
        }
    }

    fn run<F: FnOnce(&mut Vec<u8>) -> Result<()>>(command: F) -> (Result<()>, String) {
        let mut out = Vec::new();
        let result = command(&mut out);
        (result, String::from_utf8(out).unwrap())
    }

    fn json(text: &str) -> serde_json::Value {
        serde_json::from_str(text).unwrap()
    }

    // ---- create ----

    #[test]
    fn create_writes_the_profile_and_does_not_start_the_phone() {
        let f = Fixture::new();
        let (result, text) = run(|out| create(&f.human(), &options("work"), out));
        result.unwrap();
        assert!(
            text.contains("Created the phone 'work' with Android 14."),
            "{text}"
        );
        assert!(text.contains("verone start work"), "{text}");

        let phone = f.storage.find_phone(&f.id("work")).unwrap();
        let profile = Profile::load(&phone).unwrap();
        assert_eq!(profile.name, "work");
        assert!(phone.data_dir().is_dir());
        assert_eq!(f.engine.calls(), ["supports_android 14"]);
    }

    #[test]
    fn create_applies_every_option() {
        let f = Fixture::new();
        let wanted = CreateOptions {
            name: Some("Work phone".to_string()),
            size: Some("1080x2400".to_string()),
            dpi: Some(420),
            graphics: GraphicsMode::Software,
            adb_port: 5599,
            ..options("work")
        };
        let (result, text) = run(|out| create(&f.env(Mode::Json), &wanted, out));
        result.unwrap();
        let parsed = json(&text);
        assert_eq!(parsed["name"], "Work phone");
        assert_eq!(parsed["display"]["width"], 1080);
        assert_eq!(parsed["display"]["height"], 2400);
        assert_eq!(parsed["display"]["dpi"], 420);
        assert_eq!(parsed["graphics"]["mode"], "software");
        assert_eq!(parsed["adb"]["local_port"], 5599);
    }

    #[test]
    fn a_duplicate_id_fails_and_a_bad_id_is_a_usage_error() {
        let f = Fixture::new();
        f.phone("work");
        let (result, _) = run(|out| create(&f.human(), &options("work"), out));
        let error = result.unwrap_err();
        assert_eq!(error.exit_code(), 1);
        assert!(error.message().contains("already exists"));

        let (result, _) = run(|out| create(&f.human(), &options("Work Phone"), out));
        assert_eq!(result.unwrap_err().exit_code(), 2);
    }

    #[test]
    fn bad_settings_are_rejected_before_anything_is_written() {
        let f = Fixture::new();
        let cases = [
            CreateOptions {
                size: Some("big".to_string()),
                ..options("a")
            },
            CreateOptions {
                size: Some("10x10".to_string()),
                ..options("a")
            },
            CreateOptions {
                dpi: Some(9000),
                ..options("a")
            },
            CreateOptions {
                adb_port: 80,
                ..options("a")
            },
        ];
        for case in cases {
            let (result, _) = run(|out| create(&f.human(), &case, out));
            assert_eq!(result.unwrap_err().kind(), ErrorKind::Usage, "{case:?}");
        }
        let unsupported = CreateOptions {
            android: "99".to_string(),
            ..options("a")
        };
        let (result, _) = run(|out| create(&f.human(), &unsupported, out));
        assert_eq!(result.unwrap_err().exit_code(), 4);
        assert!(f.storage.list_phones().unwrap().is_empty());
    }

    #[test]
    fn sizes_are_read_as_width_by_height() {
        assert_eq!(parse_size("720x1280").unwrap(), (720, 1280));
        assert_eq!(parse_size("1080X1920").unwrap(), (1080, 1920));
        for bad in ["", "720", "720x", "x1280", "a x b", "720x1280x3"] {
            assert_eq!(
                parse_size(bad).unwrap_err().kind(),
                ErrorKind::Usage,
                "{bad}"
            );
        }
    }

    // ---- list ----

    #[test]
    fn an_empty_list_says_how_to_create_a_phone() {
        let f = Fixture::new();
        let (result, text) = run(|out| list(&f.human(), out));
        result.unwrap();
        assert!(text.contains("There are no phones yet"), "{text}");
        assert!(text.contains("verone create"), "{text}");

        let (_, text) = run(|out| list(&f.env(Mode::Json), out));
        assert_eq!(json(&text), serde_json::json!([]));
    }

    #[test]
    fn the_list_shows_each_phone_with_its_status() {
        let f = Fixture::new();
        f.phone("work");
        f.phone("test-2");
        start(&f.human(), "work", &mut Vec::new()).unwrap();

        let (result, text) = run(|out| list(&f.human(), out));
        result.unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "ID      NAME    ANDROID  STATUS");
        assert_eq!(lines[1], "test-2  test-2  14       stopped");
        assert_eq!(lines[2], "work    work    14       running");

        let (_, text) = run(|out| list(&f.env(Mode::Json), out));
        let parsed = json(&text);
        assert_eq!(parsed[1]["id"], "work");
        assert_eq!(parsed[1]["status"], "running");
    }

    #[test]
    fn a_phone_with_a_broken_profile_is_listed_as_unreadable() {
        let f = Fixture::new();
        f.phone("work");
        let phone = f.storage.find_phone(&f.id("work")).unwrap();
        fs::write(phone.profile_path(), "schema = 2\n").unwrap();
        let (result, text) = run(|out| list(&f.human(), out));
        result.unwrap();
        assert!(text.contains("unreadable"), "{text}");
    }

    // ---- info ----

    #[test]
    fn info_of_a_stopped_phone() {
        let f = Fixture::new();
        f.phone("work");
        let (result, text) = run(|out| info(&f.human(), "work", out));
        result.unwrap();
        assert!(text.contains("Phone:     work (work)"), "{text}");
        assert!(text.contains("Status:    stopped"), "{text}");
        assert!(text.contains("Screen:    720x1280 at 320 dpi"), "{text}");
        assert!(text.contains("Graphics:  auto\n"), "{text}");
        assert!(text.contains("Data:      300 MB"), "{text}");
        assert!(!text.contains("Address"), "{text}");
        assert!(!text.contains("On for"), "{text}");
    }

    #[test]
    fn info_of_a_running_phone_adds_address_uptime_and_graphics_in_use() {
        let f = Fixture::new();
        f.phone("work");
        let phone = f.storage.find_phone(&f.id("work")).unwrap();
        start(&f.human(), "work", &mut Vec::new()).unwrap();
        // The fake engine does not write state; record a boot as the real one would.
        State {
            last_boot: Some(verone_core::profile::LastBoot {
                graphics: GraphicsUsed::Gpu,
                fell_back_to_software: false,
            }),
            ..State::default()
        }
        .save(&phone)
        .unwrap();

        let (result, text) = run(|out| info(&f.human(), "work", out));
        result.unwrap();
        assert!(text.contains("Status:    running"), "{text}");
        assert!(text.contains("On for:    1 h 2 min"), "{text}");
        assert!(text.contains("Address:   127.0.0.1:5555"), "{text}");
        assert!(text.contains("Graphics:  auto, using the GPU"), "{text}");

        let (_, text) = run(|out| info(&f.env(Mode::Json), "work", out));
        let parsed = json(&text);
        assert_eq!(parsed["status"], "running");
        assert_eq!(parsed["address"], "127.0.0.1:5555");
        assert_eq!(parsed["graphics_in_use"], "gpu");
        assert_eq!(parsed["uptime_seconds"], 3723);
        assert_eq!(parsed["data_bytes"], 300 * 1024 * 1024);
    }

    #[test]
    fn commands_on_a_missing_phone_are_not_found() {
        let f = Fixture::new();
        let env = f.human();
        let mut out = Vec::new();
        let mut input = std::io::empty();
        let results = [
            info(&env, "ghost", &mut out),
            start(&env, "ghost", &mut out),
            stop(&env, "ghost", &mut out),
            restart(&env, "ghost", &mut out),
            reset(&env, "ghost", true, &mut input, &mut out),
            delete(&env, "ghost", true, &mut input, &mut out),
        ];
        for result in results {
            assert_eq!(result.unwrap_err().exit_code(), 4);
        }
    }

    #[test]
    fn durations_and_sizes_are_shown_in_plain_units() {
        assert_eq!(format_duration(42), "42 seconds");
        assert_eq!(format_duration(125), "2 min");
        assert_eq!(format_duration(3723), "1 h 2 min");
        assert_eq!(format_size(300 * 1024 * 1024), "300 MB");
        assert_eq!(format_size(1536 * 1024 * 1024), "1.5 GB");
    }

    // ---- start, stop, restart ----

    #[test]
    fn start_boots_the_phone_and_reports_the_time() {
        let f = Fixture::new();
        f.phone("work");
        let (result, text) = run(|out| start(&f.human(), "work", out));
        result.unwrap();
        assert_eq!(
            text,
            "The phone 'work' is running. It booted in 1.0 seconds using software rendering.\n"
        );
        assert_eq!(
            f.engine.status(&f.id("work")).unwrap(),
            PhoneStatus::Running
        );
    }

    #[test]
    fn starting_a_running_phone_says_so_and_succeeds() {
        let f = Fixture::new();
        f.phone("work");
        start(&f.human(), "work", &mut Vec::new()).unwrap();
        let (result, text) = run(|out| start(&f.human(), "work", out));
        result.unwrap();
        assert_eq!(text, "The phone 'work' is already running.\n");

        let (_, text) = run(|out| start(&f.env(Mode::Json), "work", out));
        assert_eq!(json(&text)["already_running"], true);
    }

    #[test]
    fn start_refuses_when_the_host_check_fails() {
        let f = Fixture::new();
        f.phone("work");
        f.engine.set_host_checks(vec![HostCheck::failed(
            "Kernel binder support",
            "missing",
            "Load it.",
        )]);
        let (result, _) = run(|out| start(&f.human(), "work", out));
        let error = result.unwrap_err();
        assert_eq!(error.exit_code(), 3);
        assert!(error.hint().unwrap().contains("verone doctor"));
        assert!(
            !f.engine
                .calls()
                .iter()
                .any(|call| call.starts_with("start"))
        );
    }

    #[test]
    fn a_fallback_to_software_is_explained() {
        let report = BootReport {
            duration: Duration::from_secs_f32(13.5),
            graphics: GraphicsUsed::Software,
            fell_back_to_software: true,
            already_running: false,
        };
        let (result, text) =
            run(|out| report_start(Mode::Human, &PhoneId::new("work").unwrap(), &report, out));
        result.unwrap();
        assert!(text.contains("booted in 13.5 seconds"), "{text}");
        assert!(text.contains("GPU graphics did not start"), "{text}");
    }

    #[test]
    fn stop_shuts_the_phone_down_and_is_harmless_when_already_stopped() {
        let f = Fixture::new();
        f.phone("work");
        let (result, text) = run(|out| stop(&f.human(), "work", out));
        result.unwrap();
        assert_eq!(text, "The phone 'work' is already stopped.\n");
        assert!(!f.engine.calls().iter().any(|call| call.starts_with("stop")));

        start(&f.human(), "work", &mut Vec::new()).unwrap();
        let (result, text) = run(|out| stop(&f.human(), "work", out));
        result.unwrap();
        assert_eq!(text, "The phone 'work' is stopped.\n");
        assert_eq!(
            f.engine.status(&f.id("work")).unwrap(),
            PhoneStatus::Stopped
        );
    }

    #[test]
    fn restart_stops_then_starts() {
        let f = Fixture::new();
        f.phone("work");
        start(&f.human(), "work", &mut Vec::new()).unwrap();
        let before = f.engine.calls().len();
        let (result, text) = run(|out| restart(&f.human(), "work", out));
        result.unwrap();
        assert!(text.contains("is running. It booted"), "{text}");
        let calls = f.engine.calls()[before..].to_vec();
        let stop_at = calls.iter().position(|call| call == "stop work").unwrap();
        let start_at = calls.iter().position(|call| call == "start work").unwrap();
        assert!(stop_at < start_at, "{calls:?}");
    }

    #[test]
    fn a_phone_locked_by_another_command_is_busy() {
        let f = Fixture::new();
        f.phone("work");
        let _held = f.storage.lock_phone(&f.id("work")).unwrap();
        // The command waits the full lock time; this is the one slow unit test.
        let (result, _) = run(|out| stop(&f.human(), "work", out));
        let error = result.unwrap_err();
        assert_eq!(error.exit_code(), 5);
    }

    // ---- open, adb, install ----

    fn tools<'a>(adb: &'a Adb, scrcpy: Vec<String>) -> Tools<'a> {
        Tools {
            adb,
            runner: Runner::default(),
            scrcpy,
        }
    }

    fn stub_adb(script: &Path) -> Adb {
        Adb::with_program(
            Runner::default(),
            "sh",
            vec![script.display().to_string(), "adb".to_string()],
        )
        .with_retry_pause(Duration::from_millis(1))
    }

    fn stub_scrcpy(script: &Path) -> Vec<String> {
        vec![
            "sh".to_string(),
            script.display().to_string(),
            "scrcpy".to_string(),
        ]
    }

    #[test]
    fn open_starts_the_phone_and_runs_scrcpy_with_the_title_and_extra_options() {
        let f = Fixture::new();
        create(
            &f.human(),
            &CreateOptions {
                name: Some("Work phone".to_string()),
                ..options("work")
            },
            &mut Vec::new(),
        )
        .unwrap();
        let script = f.stub("");
        let adb = stub_adb(&script);

        open(
            &f.human(),
            &tools(&adb, stub_scrcpy(&script)),
            "work",
            &["--no-window".to_string(), "--time-limit=5".to_string()],
        )
        .unwrap();

        assert_eq!(
            f.engine.status(&f.id("work")).unwrap(),
            PhoneStatus::Running
        );
        assert_eq!(
            f.calls().last().unwrap(),
            "scrcpy --serial 127.0.0.1:5555 --window-title=Verone – Work phone --audio-codec=raw --no-window --time-limit=5"
        );

        // Audio options given by the user replace the default.
        open(
            &f.human(),
            &tools(&adb, stub_scrcpy(&script)),
            "work",
            &["--no-audio".to_string()],
        )
        .unwrap();
        assert_eq!(
            f.calls().last().unwrap(),
            "scrcpy --serial 127.0.0.1:5555 --window-title=Verone – Work phone --no-audio"
        );
    }

    #[test]
    fn open_reports_a_failing_or_missing_scrcpy() {
        let f = Fixture::new();
        f.phone("work");
        let script = f.stub("scrcpy*) exit 2 ;;");
        let adb = stub_adb(&script);
        let error = open(&f.human(), &tools(&adb, stub_scrcpy(&script)), "work", &[]).unwrap_err();
        assert!(
            error.message().contains("scrcpy exited with code 2"),
            "{error}"
        );

        let missing = vec!["verone-no-such-scrcpy".to_string()];
        let error = open(&f.human(), &tools(&adb, missing), "work", &[]).unwrap_err();
        assert!(
            error.message().contains("scrcpy is not installed"),
            "{error}"
        );
        assert!(error.hint().unwrap().contains("Install the package scrcpy"));
    }

    #[test]
    fn adb_passes_its_arguments_to_the_running_phone() {
        let f = Fixture::new();
        f.phone("work");
        let script = f.stub("*'shell false') exit 1 ;;");
        let adb_helper = stub_adb(&script);
        let t = tools(&adb_helper, vec![]);
        let args = |list: &[&str]| list.iter().map(|a| a.to_string()).collect::<Vec<_>>();

        // A stopped phone has no connection.
        let error = adb(&f.human(), &t, "work", &args(&["shell", "true"])).unwrap_err();
        assert!(error.message().contains("not running"), "{error}");

        start(&f.human(), "work", &mut Vec::new()).unwrap();
        adb(
            &f.human(),
            &t,
            "work",
            &args(&["shell", "getprop", "sys.boot_completed"]),
        )
        .unwrap();
        assert_eq!(
            f.calls().last().unwrap(),
            "adb -s 127.0.0.1:5555 shell getprop sys.boot_completed"
        );

        let error = adb(&f.human(), &t, "work", &args(&["shell", "false"])).unwrap_err();
        assert!(
            error.message().contains("adb exited with code 1"),
            "{error}"
        );

        let error = adb(&f.human(), &t, "work", &[]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Usage);
    }

    #[test]
    fn install_reports_the_new_package() {
        let f = Fixture::new();
        f.phone("work");
        start(&f.human(), "work", &mut Vec::new()).unwrap();
        let apk = f.temp.path().join("app.apk");
        fs::write(&apk, "not really an apk").unwrap();
        // The package list gains an entry once `install` has run.
        let script = f.stub(
            r#"*install*) touch "$(dirname "$0")/installed"; echo Success ;;
*'pm list packages') echo 'package:com.android.settings'
  [ -f "$(dirname "$0")/installed" ] && echo 'package:org.fdroid.fdroid' ;;"#,
        );
        let adb_helper = stub_adb(&script);
        let t = tools(&adb_helper, vec![]);

        let (result, text) = run(|out| install(&f.human(), &t, "work", &apk, out));
        result.unwrap();
        assert_eq!(text, "Installed org.fdroid.fdroid on the phone 'work'.\n");

        // Installing again adds nothing new.
        let (result, text) = run(|out| install(&f.env(Mode::Json), &t, "work", &apk, out));
        result.unwrap();
        assert_eq!(json(&text)["packages"], serde_json::json!([]));
        let (_, text) = run(|out| install(&f.human(), &t, "work", &apk, out));
        assert!(
            text.contains("replaced an app that was already there"),
            "{text}"
        );
    }

    #[test]
    fn install_fails_clearly_for_a_missing_file_or_a_rejected_app() {
        let f = Fixture::new();
        f.phone("work");
        start(&f.human(), "work", &mut Vec::new()).unwrap();
        let script = f.stub("*install*) echo 'Failure [INSTALL_FAILED_INVALID_APK]'; exit 1 ;;");
        let adb_helper = stub_adb(&script);
        let t = tools(&adb_helper, vec![]);

        let missing = f.temp.path().join("missing.apk");
        let (result, _) = run(|out| install(&f.human(), &t, "work", &missing, out));
        assert_eq!(result.unwrap_err().kind(), ErrorKind::Usage);

        let apk = f.temp.path().join("bad.apk");
        fs::write(&apk, "x").unwrap();
        let (result, _) = run(|out| install(&f.human(), &t, "work", &apk, out));
        let error = result.unwrap_err();
        assert!(
            error.message().contains("INSTALL_FAILED_INVALID_APK"),
            "{error}"
        );
    }

    // ---- reset, delete ----

    #[test]
    fn confirmation_accepts_only_yes() {
        for (answer, expected) in [
            ("y\n", true),
            ("YES\n", true),
            (" yes \n", true),
            ("n\n", false),
            ("\n", false),
            ("", false),
            ("maybe\n", false),
        ] {
            let mut out = Vec::new();
            let got = confirm("Sure?", &mut answer.as_bytes(), &mut out).unwrap();
            assert_eq!(got, expected, "{answer:?}");
            assert_eq!(String::from_utf8(out).unwrap(), "Sure? [y/N] ");
        }
    }

    #[test]
    fn reset_erases_the_data_keeps_the_profile_and_stops_the_phone_first() {
        let f = Fixture::new();
        f.phone("work");
        let phone = f.storage.find_phone(&f.id("work")).unwrap();
        fs::write(phone.data_dir().join("app-data"), "x").unwrap();
        start(&f.human(), "work", &mut Vec::new()).unwrap();

        let (result, text) = run(|out| reset(&f.human(), "work", true, &mut std::io::empty(), out));
        result.unwrap();
        assert_eq!(
            text,
            "The phone 'work' was reset. It will start like a new phone.\n"
        );
        assert_eq!(fs::read_dir(phone.data_dir()).unwrap().count(), 0);
        assert!(phone.profile_path().is_file());
        let calls = f.engine.calls();
        let stop_at = calls.iter().position(|call| call == "stop work").unwrap();
        let erase_at = calls
            .iter()
            .position(|call| call == "erase_data work")
            .unwrap();
        assert!(stop_at < erase_at, "{calls:?}");

        // The phone still starts afterwards.
        start(&f.human(), "work", &mut Vec::new()).unwrap();
    }

    #[test]
    fn answering_no_changes_nothing() {
        let f = Fixture::new();
        f.phone("work");
        let phone = f.storage.find_phone(&f.id("work")).unwrap();
        fs::write(phone.data_dir().join("app-data"), "x").unwrap();
        let before = f.engine.calls().len();

        let (result, text) =
            run(|out| reset(&f.human(), "work", false, &mut "n\n".as_bytes(), out));
        result.unwrap();
        assert!(
            text.contains("Erase all apps, accounts and files on the phone 'work'?"),
            "{text}"
        );
        assert!(text.ends_with("Nothing was changed.\n"), "{text}");

        let (result, text) =
            run(|out| delete(&f.human(), "work", false, &mut "\n".as_bytes(), out));
        result.unwrap();
        assert!(text.ends_with("Nothing was changed.\n"), "{text}");

        assert!(phone.data_dir().join("app-data").is_file());
        assert_eq!(f.engine.calls().len(), before);
    }

    #[test]
    fn answering_yes_at_the_prompt_goes_ahead() {
        let f = Fixture::new();
        f.phone("work");
        let (result, text) =
            run(|out| delete(&f.human(), "work", false, &mut "yes\n".as_bytes(), out));
        result.unwrap();
        assert!(text.ends_with("The phone 'work' was deleted.\n"), "{text}");
        assert!(f.storage.list_phones().unwrap().is_empty());
    }

    #[test]
    fn delete_removes_the_container_the_data_and_the_folder() {
        let f = Fixture::new();
        f.phone("work");
        let phone = f.storage.find_phone(&f.id("work")).unwrap();
        start(&f.human(), "work", &mut Vec::new()).unwrap();

        let (result, text) =
            run(|out| delete(&f.env(Mode::Json), "work", true, &mut std::io::empty(), out));
        result.unwrap();
        assert_eq!(json(&text)["done"], true);
        assert!(!phone.exists());
        assert_eq!(
            f.engine.status(&f.id("work")).unwrap(),
            PhoneStatus::NotCreated
        );
        let calls = f.engine.calls();
        for wanted in ["stop work", "destroy work", "erase_data work"] {
            assert!(calls.iter().any(|call| call == wanted), "{calls:?}");
        }
    }
}
