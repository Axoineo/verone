//! Running external programs.
//!
//! Every external program Verone uses (the container runtime, adb, scrcpy) is run
//! through this module. Programs are always started with an argument list, never
//! through a shell, and every captured run has a time limit.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};

/// Time limit used when a command does not set its own.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// How often a running program is checked for completion.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// How long to wait for the rest of a program's output after it has exited.
/// A program can leave a background child holding its output open; this bounds the
/// wait in that case.
const DRAIN_LIMIT: Duration = Duration::from_secs(2);

/// How much of a failed program's error output is quoted in the error message.
const ERROR_TAIL_CHARS: usize = 400;

/// A program to run, with its arguments and limits.
#[derive(Debug, Clone)]
pub struct Cmd {
    program: OsString,
    args: Vec<OsString>,
    timeout: Option<Duration>,
    passthrough: bool,
}

impl Cmd {
    /// Describes a run of `program` with no arguments and the default time limit.
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            program: program.as_ref().to_os_string(),
            args: Vec::new(),
            timeout: Some(DEFAULT_TIMEOUT),
            passthrough: false,
        }
    }

    /// Adds one argument.
    #[must_use]
    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    /// Adds several arguments.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|arg| arg.as_ref().to_os_string()));
        self
    }

    /// Sets the time limit.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Removes the time limit. Only for programs the user watches and ends
    /// themselves, such as a phone window.
    #[must_use]
    pub fn no_timeout(mut self) -> Self {
        self.timeout = None;
        self
    }

    /// Connects the program to the user's terminal instead of capturing its output.
    #[must_use]
    pub fn passthrough(mut self) -> Self {
        self.passthrough = true;
        self
    }

    /// The program name, for messages.
    pub fn program(&self) -> String {
        self.program.to_string_lossy().into_owned()
    }

    /// The command as one line, for logging. Not suitable for running in a shell.
    pub fn display(&self) -> String {
        let mut line = self.program();
        for arg in &self.args {
            line.push(' ');
            line.push_str(&arg.to_string_lossy());
        }
        line
    }
}

/// What a finished program produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The exit code, or `None` if the program was ended by a signal.
    pub code: Option<i32>,
    /// Captured standard output. Empty for pass-through runs.
    pub stdout: String,
    /// Captured standard error. Empty for pass-through runs.
    pub stderr: String,
}

impl Output {
    /// True if the program exited with code 0.
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// Runs external programs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Runner {
    verbose: bool,
}

impl Runner {
    /// Creates a runner. With `verbose`, each command is printed to standard error
    /// before it runs.
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }

    /// Runs the command and returns its output, whatever its exit code.
    ///
    /// Fails only if the program cannot be started or exceeds its time limit.
    pub fn run(&self, cmd: &Cmd) -> Result<Output> {
        if self.verbose {
            eprintln!("+ {}", cmd.display());
        }

        let mut command = Command::new(&cmd.program);
        command.args(&cmd.args);
        if cmd.passthrough {
            command
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
        } else {
            command
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
        }

        let mut child = command.spawn().map_err(|error| spawn_error(cmd, &error))?;
        let stdout = Capture::start(child.stdout.take());
        let stderr = Capture::start(child.stderr.take());

        let status = wait(&mut child, cmd)?;

        // Both streams share one limit, so a held-open output costs it only once.
        let drain_deadline = Instant::now() + DRAIN_LIMIT;
        Ok(Output {
            code: status.code(),
            stdout: stdout.finish(drain_deadline),
            stderr: stderr.finish(drain_deadline),
        })
    }

    /// Runs the command and fails unless it exits with code 0.
    pub fn run_ok(&self, cmd: &Cmd) -> Result<Output> {
        let output = self.run(cmd)?;
        if output.success() {
            return Ok(output);
        }
        let outcome = match output.code {
            Some(code) => format!("exited with code {code}"),
            None => "was ended by a signal".to_string(),
        };
        let mut message = format!("{} {outcome}", cmd.program());
        let detail = tail(output.stderr.trim(), ERROR_TAIL_CHARS);
        if !detail.is_empty() {
            message.push_str(": ");
            message.push_str(detail);
        }
        Err(Error::failed(message).with_hint(format!("The command was: {}", cmd.display())))
    }
}

/// Waits for the program to exit, ending it if it passes its time limit.
fn wait(child: &mut Child, cmd: &Cmd) -> Result<ExitStatus> {
    let deadline = cmd.timeout.map(|timeout| Instant::now() + timeout);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(error) => {
                return Err(Error::failed(format!(
                    "could not wait for {}: {error}",
                    cmd.program()
                )));
            }
        }
        if let (Some(deadline), Some(timeout)) = (deadline, cmd.timeout)
            && Instant::now() >= deadline
        {
            // Best effort: the program may already have exited.
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::failed(format!(
                "{} did not finish within {} seconds",
                cmd.program(),
                timeout.as_secs_f32()
            ))
            .with_hint(format!("The command was: {}", cmd.display())));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn spawn_error(cmd: &Cmd, error: &std::io::Error) -> Error {
    let program = cmd.program();
    if error.kind() == std::io::ErrorKind::NotFound {
        Error::failed(format!("the program '{program}' was not found"))
            .with_hint(format!("Install {program}, or add its folder to PATH."))
    } else {
        Error::failed(format!("could not start '{program}': {error}"))
    }
}

/// Returns at most the last `max_chars` characters of `text`.
fn tail(text: &str, max_chars: usize) -> &str {
    let count = text.chars().count();
    if count <= max_chars {
        return text;
    }
    match text.char_indices().nth(count - max_chars) {
        Some((index, _)) => &text[index..],
        None => text,
    }
}

/// Collects one output stream of a program on a background thread.
struct Capture {
    buffer: Arc<Mutex<Vec<u8>>>,
    done: Option<Receiver<()>>,
}

impl Capture {
    fn start<R: Read + Send + 'static>(stream: Option<R>) -> Self {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let Some(mut stream) = stream else {
            return Self { buffer, done: None };
        };
        let (sender, receiver) = mpsc::channel();
        let shared = Arc::clone(&buffer);
        thread::spawn(move || {
            let mut chunk = [0_u8; 8192];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => lock(&shared).extend_from_slice(&chunk[..read]),
                }
            }
            // The receiver may be gone if the wait was abandoned; that is fine.
            let _ = sender.send(());
        });
        Self {
            buffer,
            done: Some(receiver),
        }
    }

    /// Returns what was collected, waiting until `deadline` for the stream to end.
    fn finish(self, deadline: Instant) -> String {
        if let Some(done) = &self.done {
            // On timeout the thread keeps running until the stream closes; what
            // has arrived so far is returned.
            let _ = done.recv_timeout(deadline.saturating_duration_since(Instant::now()));
        }
        String::from_utf8_lossy(&lock(&self.buffer)).into_owned()
    }
}

/// Locks the buffer. A poisoned lock still holds valid bytes, so it is used as is.
fn lock(buffer: &Mutex<Vec<u8>>) -> std::sync::MutexGuard<'_, Vec<u8>> {
    match buffer.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn sh(script: &str) -> Cmd {
        Cmd::new("sh").arg("-c").arg(script)
    }

    #[test]
    fn captures_stdout_and_stderr_separately() {
        let output = Runner::default()
            .run(&sh("echo out; echo err >&2"))
            .unwrap();
        assert_eq!(output.code, Some(0));
        assert_eq!(output.stdout, "out\n");
        assert_eq!(output.stderr, "err\n");
        assert!(output.success());
    }

    #[test]
    fn run_returns_a_non_zero_exit_code_without_failing() {
        let output = Runner::default().run(&sh("exit 7")).unwrap();
        assert_eq!(output.code, Some(7));
        assert!(!output.success());
    }

    #[test]
    fn run_ok_fails_on_non_zero_exit_and_quotes_stderr() {
        let error = Runner::default()
            .run_ok(&sh("echo broken >&2; exit 3"))
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Failed);
        assert!(error.message().contains("exited with code 3"), "{error}");
        assert!(error.message().contains("broken"), "{error}");
        assert!(error.hint().unwrap().contains("sh -c"));
    }

    #[test]
    fn run_ok_passes_through_success() {
        let output = Runner::default().run_ok(&sh("echo fine")).unwrap();
        assert_eq!(output.stdout, "fine\n");
    }

    #[test]
    fn missing_program_is_reported_with_a_hint() {
        let error = Runner::default()
            .run(&Cmd::new("verone-test-no-such-program"))
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Failed);
        assert!(error.message().contains("was not found"), "{error}");
        assert!(error.hint().unwrap().contains("PATH"));
    }

    #[test]
    fn a_program_over_its_time_limit_is_stopped() {
        let started = Instant::now();
        let error = Runner::default()
            .run(
                &Cmd::new("sleep")
                    .arg("30")
                    .timeout(Duration::from_millis(200)),
            )
            .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(error.kind(), ErrorKind::Failed);
        assert!(error.message().contains("did not finish"), "{error}");
    }

    #[test]
    fn large_output_does_not_block() {
        let output = Runner::default()
            .run(&sh("head -c 300000 /dev/zero | tr '\\0' a"))
            .unwrap();
        assert_eq!(output.stdout.len(), 300_000);
    }

    #[test]
    fn a_background_child_holding_the_output_does_not_hang_the_run() {
        let started = Instant::now();
        let output = Runner::default()
            .run(&sh("echo first; sleep 30 &"))
            .unwrap();
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(output.stdout, "first\n");
    }

    #[test]
    fn a_program_ended_by_a_signal_has_no_exit_code() {
        let output = Runner::default().run(&sh("kill -9 $$")).unwrap();
        assert_eq!(output.code, None);
        let error = Runner::default().run_ok(&sh("kill -9 $$")).unwrap_err();
        assert!(error.message().contains("signal"), "{error}");
    }

    #[test]
    fn passthrough_runs_without_capturing() {
        let output = Runner::default().run(&sh("exit 0").passthrough()).unwrap();
        assert_eq!(output.code, Some(0));
        assert_eq!(output.stdout, "");
    }

    #[test]
    fn arguments_are_passed_literally_not_through_a_shell() {
        let output = Runner::default()
            .run(&Cmd::new("printf").args(["%s", "$HOME; echo injected"]))
            .unwrap();
        assert_eq!(output.stdout, "$HOME; echo injected");
    }

    #[test]
    fn display_joins_program_and_arguments() {
        let cmd = Cmd::new("docker").arg("inspect").args(["a", "b"]);
        assert_eq!(cmd.display(), "docker inspect a b");
        assert_eq!(cmd.program(), "docker");
    }

    #[test]
    fn tail_keeps_the_end_of_long_text() {
        assert_eq!(tail("abcdef", 3), "def");
        assert_eq!(tail("abc", 10), "abc");
        assert_eq!(tail("añb", 2), "ñb");
    }
}
