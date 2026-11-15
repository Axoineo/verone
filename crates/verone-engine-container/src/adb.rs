//! The debug connection (adb) to a running phone.
//!
//! adb keeps connections in a background server. After a phone restarts, an old
//! connection still looks alive but answers `error: closed` and runs nothing, so
//! every use starts by making sure the connection really works.

use std::thread;
use std::time::Duration;

use verone_core::engine::ControlEndpoint;
use verone_core::process::{Cmd, Output, Runner};
use verone_core::{Error, Result};

/// The adb program used unless another is chosen.
pub const DEFAULT_PROGRAM: &str = "adb";

/// How many times a connection is attempted before giving up.
const CONNECT_ATTEMPTS: u32 = 3;

/// Pause between connection attempts.
const RETRY_PAUSE: Duration = Duration::from_millis(500);

/// Time limit for connecting and for the liveness check.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Runs adb against phones.
#[derive(Debug, Clone)]
pub struct Adb {
    runner: Runner,
    program: String,
    leading_args: Vec<String>,
    retry_pause: Duration,
}

impl Adb {
    /// Uses the `adb` program found on `PATH`.
    pub fn new(runner: Runner) -> Self {
        Self::with_program(runner, DEFAULT_PROGRAM, Vec::new())
    }

    /// Uses another program, with arguments placed before every command. Tests use
    /// this to run a stub script.
    pub fn with_program(
        runner: Runner,
        program: impl Into<String>,
        leading_args: Vec<String>,
    ) -> Self {
        Self {
            runner,
            program: program.into(),
            leading_args,
            retry_pause: RETRY_PAUSE,
        }
    }

    /// Sets the pause between connection attempts. Tests shorten it.
    #[must_use]
    pub fn with_retry_pause(mut self, pause: Duration) -> Self {
        self.retry_pause = pause;
        self
    }

    fn cmd(&self) -> Cmd {
        Cmd::new(&self.program).args(&self.leading_args)
    }

    /// Makes sure adb has a working connection to the phone.
    pub fn ready(&self, endpoint: &ControlEndpoint) -> Result<()> {
        let address = endpoint.to_string();
        for attempt in 1..=CONNECT_ATTEMPTS {
            self.runner.run(
                &self
                    .cmd()
                    .args(["connect", &address])
                    .timeout(CONNECT_TIMEOUT),
            )?;
            let alive = self.runner.run(
                &self
                    .cmd()
                    .args(["-s", &address, "shell", "true"])
                    .timeout(CONNECT_TIMEOUT),
            )?;
            if alive.success() {
                return Ok(());
            }
            // Drop the stale connection so the next attempt makes a fresh one.
            self.disconnect(endpoint);
            if attempt < CONNECT_ATTEMPTS {
                thread::sleep(self.retry_pause);
            }
        }
        Err(
            Error::failed(format!("could not connect adb to the phone at {address}"))
                .with_hint("Check that the phone is running with verone list, then try again."),
        )
    }

    /// Forgets the connection to a phone. Failure is ignored: there may be none.
    pub fn disconnect(&self, endpoint: &ControlEndpoint) {
        let _ = self.runner.run(
            &self
                .cmd()
                .args(["disconnect", &endpoint.to_string()])
                .timeout(CONNECT_TIMEOUT),
        );
    }

    /// Runs an adb command against the phone and captures its output.
    pub fn run(
        &self,
        endpoint: &ControlEndpoint,
        args: &[&str],
        timeout: Duration,
    ) -> Result<Output> {
        self.ready(endpoint)?;
        self.runner.run(
            &self
                .cmd()
                .args(["-s", &endpoint.to_string()])
                .args(args)
                .timeout(timeout),
        )
    }

    /// Runs an adb command against the phone, connected to the user's terminal.
    /// Returns the command's exit code.
    pub fn run_interactive(
        &self,
        endpoint: &ControlEndpoint,
        args: &[String],
    ) -> Result<Option<i32>> {
        self.ready(endpoint)?;
        let output = self.runner.run(
            &self
                .cmd()
                .args(["-s", &endpoint.to_string()])
                .args(args)
                .no_timeout()
                .passthrough(),
        )?;
        Ok(output.code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::testing::Stub;

    fn endpoint() -> ControlEndpoint {
        ControlEndpoint {
            host: "172.17.0.2".to_string(),
            port: 5555,
        }
    }

    fn adb(stub: &Stub) -> Adb {
        Adb::with_program(
            Runner::default(),
            "sh",
            vec![stub.script().display().to_string()],
        )
        .with_retry_pause(Duration::from_millis(1))
    }

    #[test]
    fn a_working_connection_is_ready_after_one_attempt() {
        let stub = Stub::new("");
        adb(&stub).ready(&endpoint()).unwrap();
        assert_eq!(
            stub.calls(),
            ["connect 172.17.0.2:5555", "-s 172.17.0.2:5555 shell true"]
        );
    }

    #[test]
    fn a_stale_connection_is_dropped_and_retried() {
        // The liveness check fails until the stale connection has been dropped.
        // `$0` is the stub script, so the flag file lives beside it.
        let stub = Stub::new(
            r#"disconnect*) touch "$(dirname "$0")/dropped" ;;
*'shell true') [ -f "$(dirname "$0")/dropped" ] || { echo 'error: closed' >&2; exit 1; } ;;"#,
        );

        adb(&stub).ready(&endpoint()).unwrap();
        assert_eq!(
            stub.calls(),
            [
                "connect 172.17.0.2:5555",
                "-s 172.17.0.2:5555 shell true",
                "disconnect 172.17.0.2:5555",
                "connect 172.17.0.2:5555",
                "-s 172.17.0.2:5555 shell true",
            ]
        );
    }

    #[test]
    fn a_phone_that_never_answers_gives_an_error_after_three_attempts() {
        let stub = Stub::new("*'shell true') echo 'error: closed' >&2; exit 1 ;;");
        let error = adb(&stub).ready(&endpoint()).unwrap_err();
        assert!(error.message().contains("172.17.0.2:5555"), "{error}");
        assert!(error.hint().unwrap().contains("verone list"));
        let attempts = stub
            .calls()
            .iter()
            .filter(|call| call.starts_with("connect"))
            .count();
        assert_eq!(attempts, 3);
    }

    #[test]
    fn a_command_is_sent_to_the_phone_after_the_readiness_check() {
        let stub = Stub::new("*getprop*) echo 14 ;;");
        let output = adb(&stub)
            .run(
                &endpoint(),
                &["shell", "getprop", "ro.build.version.release"],
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(output.stdout.trim(), "14");
        assert_eq!(
            stub.calls().last().unwrap(),
            "-s 172.17.0.2:5555 shell getprop ro.build.version.release"
        );
    }

    #[test]
    fn an_interactive_command_reports_its_exit_code() {
        let stub = Stub::new("*install*) exit 4 ;;");
        let code = adb(&stub)
            .run_interactive(&endpoint(), &["install".to_string(), "app.apk".to_string()])
            .unwrap();
        assert_eq!(code, Some(4));
    }

    #[test]
    fn a_missing_adb_program_is_reported() {
        let adb = Adb::with_program(Runner::default(), "verone-no-such-adb", vec![]);
        let error = adb.ready(&endpoint()).unwrap_err();
        assert!(error.message().contains("was not found"), "{error}");
    }
}
