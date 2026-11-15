//! The interface every engine implements.
//!
//! An engine runs Android. The rest of Verone talks to engines only through the
//! [`Engine`] trait, so it does not know whether a phone is a container or a
//! virtual machine.

pub mod fake;

use std::fmt;
use std::time::Duration;

use serde::Serialize;

use crate::error::Result;
use crate::profile::{EngineKind, GraphicsUsed, Profile};
use crate::storage::{PhoneDir, PhoneId};

/// Something that can run Android phones.
pub trait Engine {
    /// Which engine this is.
    fn kind(&self) -> EngineKind;

    /// Reports whether this machine can run the engine and what is missing.
    ///
    /// Returns every check, passed or not. An `Err` means the checks themselves
    /// could not be carried out.
    fn check_host(&self) -> Result<Vec<HostCheck>>;

    /// Makes the image for an Android version available locally.
    ///
    /// Fails with a not-found error if the engine has no image for that version.
    fn prepare_image(&self, android: &str) -> Result<()>;

    /// Creates the phone's runtime resources. Does not start it. Creating a phone
    /// that already exists brings its resources in line with the profile.
    fn create(&self, profile: &Profile, phone: &PhoneDir) -> Result<()>;

    /// Removes the phone's runtime resources and keeps its data. Removing a phone
    /// that has none is not an error.
    fn destroy(&self, id: &PhoneId) -> Result<()>;

    /// Boots the phone and waits until Android is ready. Starting a running phone
    /// is not an error.
    fn start(&self, profile: &Profile, phone: &PhoneDir) -> Result<BootReport>;

    /// Shuts Android down cleanly. Stopping a stopped phone is not an error.
    fn stop(&self, id: &PhoneId) -> Result<()>;

    /// The phone's current state.
    fn status(&self, id: &PhoneId) -> Result<PhoneStatus>;

    /// Where the debug connection (adb) of a running phone can be reached.
    fn control_endpoint(&self, id: &PhoneId) -> Result<ControlEndpoint>;

    /// What this engine supports on this machine.
    fn capabilities(&self) -> Result<Capabilities>;
}

/// The result of one host check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostCheck {
    /// Short name of what was checked, for example "Container runtime".
    pub name: String,
    /// Whether it passed.
    pub outcome: CheckOutcome,
    /// What was found.
    pub detail: String,
    /// What the user should do, when the check did not pass.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

impl HostCheck {
    /// A check that passed.
    pub fn ok(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            outcome: CheckOutcome::Ok,
            detail: detail.into(),
            fix: None,
        }
    }

    /// A check that found a problem Verone can work around.
    pub fn warning(
        name: impl Into<String>,
        detail: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            outcome: CheckOutcome::Warning,
            detail: detail.into(),
            fix: Some(fix.into()),
        }
    }

    /// A check that found a problem that stops phones from running.
    pub fn failed(
        name: impl Into<String>,
        detail: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            outcome: CheckOutcome::Failed,
            detail: detail.into(),
            fix: Some(fix.into()),
        }
    }
}

/// Whether a host check passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckOutcome {
    /// Nothing to do.
    Ok,
    /// Phones will run, with a limitation.
    Warning,
    /// Phones cannot run until this is fixed.
    Failed,
}

/// True if any check failed. Warnings do not count.
pub fn any_failed(checks: &[HostCheck]) -> bool {
    checks
        .iter()
        .any(|check| check.outcome == CheckOutcome::Failed)
}

/// The state of a phone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum PhoneStatus {
    /// The phone has no runtime resources yet. It has never been started, or they
    /// were removed.
    NotCreated,
    /// Android is not running.
    Stopped,
    /// Android is starting.
    Booting,
    /// Android is ready.
    Running,
    /// The phone stopped unexpectedly.
    Failed {
        /// What the engine knows about why.
        reason: String,
    },
}

impl fmt::Display for PhoneStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhoneStatus::NotCreated | PhoneStatus::Stopped => f.write_str("stopped"),
            PhoneStatus::Booting => f.write_str("booting"),
            PhoneStatus::Running => f.write_str("running"),
            PhoneStatus::Failed { reason } => write!(f, "failed ({reason})"),
        }
    }
}

/// What happened during a successful start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootReport {
    /// How long the boot took. Zero if the phone was already running.
    pub duration: Duration,
    /// The graphics mode the phone booted in.
    pub graphics: GraphicsUsed,
    /// True if GPU mode was tried first and the phone had to fall back.
    pub fell_back_to_software: bool,
    /// True if the phone was already running and nothing was done.
    pub already_running: bool,
}

/// The network address of a phone's debug connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlEndpoint {
    /// Host name or address.
    pub host: String,
    /// Port number.
    pub port: u16,
}

impl fmt::Display for ControlEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

/// What an engine supports on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    /// The screen can be drawn by the GPU here.
    pub gpu_rendering: bool,
    /// Phones give root access through the debug connection.
    pub root_access: bool,
    /// Phones run their own Android kernel. When false they share the host's, and
    /// security findings made in them do not automatically apply to a real phone.
    pub own_kernel: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_constructors_set_outcome_and_fix() {
        let ok = HostCheck::ok("Runtime", "Docker 29.9.3");
        assert_eq!(ok.outcome, CheckOutcome::Ok);
        assert_eq!(ok.fix, None);

        let warning = HostCheck::warning("Memory", "1.0 GB available", "Close applications.");
        assert_eq!(warning.outcome, CheckOutcome::Warning);
        assert_eq!(warning.fix.as_deref(), Some("Close applications."));

        let failed = HostCheck::failed("Binder", "not available", "Run modprobe binder_linux.");
        assert_eq!(failed.outcome, CheckOutcome::Failed);
    }

    #[test]
    fn only_failures_count_as_failed() {
        let ok = HostCheck::ok("a", "fine");
        let warning = HostCheck::warning("b", "meh", "do x");
        let failed = HostCheck::failed("c", "bad", "do y");
        assert!(!any_failed(&[]));
        assert!(!any_failed(&[ok.clone(), warning.clone()]));
        assert!(any_failed(&[ok, warning, failed]));
    }

    #[test]
    fn status_is_shown_in_plain_words() {
        assert_eq!(PhoneStatus::NotCreated.to_string(), "stopped");
        assert_eq!(PhoneStatus::Stopped.to_string(), "stopped");
        assert_eq!(PhoneStatus::Booting.to_string(), "booting");
        assert_eq!(PhoneStatus::Running.to_string(), "running");
        let failed = PhoneStatus::Failed {
            reason: "exit code 137".to_string(),
        };
        assert_eq!(failed.to_string(), "failed (exit code 137)");
    }

    #[test]
    fn endpoint_is_shown_as_host_and_port() {
        let endpoint = ControlEndpoint {
            host: "172.17.0.2".to_string(),
            port: 5555,
        };
        assert_eq!(endpoint.to_string(), "172.17.0.2:5555");
    }
}
