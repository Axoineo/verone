//! Typed access to the container runtime's command line.
//!
//! Every call goes through the core process runner with an argument list and a
//! time limit. Nothing here knows about phones; it only speaks the runtime's
//! language.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::Deserialize;
use verone_core::process::{Cmd, Output, Runner};
use verone_core::{Error, Result};

/// The runtime program used unless another is chosen.
pub const DEFAULT_PROGRAM: &str = "docker";

/// Time limit for pulling an image. Images are several gigabytes.
const PULL_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);

/// Time limit for calls that change a container.
const CHANGE_TIMEOUT: Duration = Duration::from_secs(60);

/// What `info` reports about the runtime.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Info {
    /// Version of the runtime's server.
    pub server_version: String,
    /// The runtime's own data folder.
    pub docker_root_dir: String,
    /// The storage driver in use.
    #[serde(default)]
    pub driver: String,
    /// Key and value pairs describing the storage driver.
    #[serde(default)]
    pub driver_status: Vec<(String, String)>,
}

impl Info {
    /// True if images are stored by containerd, outside the runtime's data folder.
    pub fn uses_containerd_image_store(&self) -> bool {
        self.driver_status
            .iter()
            .any(|(key, value)| key == "driver-type" && value.contains("containerd"))
    }
}

/// What `inspect` reports about a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    /// The runtime's state word: `created`, `running`, `exited`, `restarting`, …
    pub status: String,
    /// Exit code of the last run.
    pub exit_code: i64,
    /// When the container last started, as the runtime prints it.
    pub started_at: String,
    /// Labels set at creation.
    pub labels: BTreeMap<String, String>,
    /// The container's addresses, one per network that gave it one.
    pub addresses: Vec<String>,
    /// The restart policy name: `no`, `unless-stopped`, …
    pub restart_policy: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawContainer {
    state: RawState,
    #[serde(default)]
    config: RawConfig,
    #[serde(default)]
    network_settings: RawNetworkSettings,
    #[serde(default)]
    host_config: RawHostConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawState {
    status: String,
    #[serde(default)]
    exit_code: i64,
    #[serde(default)]
    started_at: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawConfig {
    #[serde(default)]
    labels: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawNetworkSettings {
    #[serde(default)]
    networks: Option<BTreeMap<String, RawNetwork>>,
}

#[derive(Debug, Default, Deserialize)]
struct RawNetwork {
    #[serde(rename = "IPAddress", default)]
    ip_address: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawHostConfig {
    #[serde(default)]
    restart_policy: RawRestartPolicy,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawRestartPolicy {
    #[serde(default)]
    name: String,
}

impl From<RawContainer> for Container {
    fn from(raw: RawContainer) -> Self {
        Self {
            status: raw.state.status,
            exit_code: raw.state.exit_code,
            started_at: raw.state.started_at,
            labels: raw.config.labels.unwrap_or_default(),
            addresses: raw
                .network_settings
                .networks
                .unwrap_or_default()
                .into_values()
                .map(|network| network.ip_address)
                .filter(|address| !address.is_empty())
                .collect(),
            restart_policy: raw.host_config.restart_policy.name,
        }
    }
}

/// The container runtime.
#[derive(Debug, Clone)]
pub struct Docker {
    runner: Runner,
    program: String,
    leading_args: Vec<String>,
}

impl Docker {
    /// Uses the `docker` program found on `PATH`.
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
        }
    }

    /// The program name, for messages.
    pub fn program(&self) -> &str {
        &self.program
    }

    fn cmd(&self) -> Cmd {
        Cmd::new(&self.program).args(&self.leading_args)
    }

    /// The version of the runtime's server.
    pub fn server_version(&self) -> Result<String> {
        let output =
            self.runner.run_ok(
                &self
                    .cmd()
                    .args(["version", "--format", "{{.Server.Version}}"]),
            )?;
        Ok(output.stdout.trim().to_string())
    }

    /// Facts about the runtime. Fails if its service cannot be reached.
    pub fn info(&self) -> Result<Info> {
        let output = self
            .runner
            .run_ok(&self.cmd().args(["info", "--format", "{{json .}}"]))?;
        parse_json(&output.stdout, "info")
    }

    /// True if the image is available locally.
    pub fn image_present(&self, reference: &str) -> Result<bool> {
        let output = self.runner.run(
            &self
                .cmd()
                .args(["image", "inspect", "--format", "{{.Id}}", reference]),
        )?;
        if output.success() {
            return Ok(true);
        }
        if is_no_such(&output) {
            return Ok(false);
        }
        Err(command_failed(&self.program, "image inspect", &output))
    }

    /// Downloads an image, showing the runtime's own progress to the user.
    pub fn pull(&self, reference: &str) -> Result<()> {
        let output = self.runner.run(
            &self
                .cmd()
                .args(["pull", reference])
                .timeout(PULL_TIMEOUT)
                .passthrough(),
        )?;
        if output.success() {
            Ok(())
        } else {
            Err(
                Error::failed(format!("could not download the image {reference}")).with_hint(
                    "Check the internet connection and free disk space, then try again.",
                ),
            )
        }
    }

    /// Creates a container without starting it.
    pub fn create(
        &self,
        name: &str,
        options: &[String],
        image: &str,
        parameters: &[String],
    ) -> Result<()> {
        let cmd = self
            .cmd()
            .args(["create", "--name", name])
            .args(options)
            .arg(image)
            .args(parameters)
            .timeout(CHANGE_TIMEOUT);
        self.runner.run_ok(&cmd).map(|_| ())
    }

    /// Starts a created or stopped container.
    pub fn start(&self, name: &str) -> Result<()> {
        self.change(&["start", name])
    }

    /// Sets the container's restart policy.
    pub fn update_restart(&self, name: &str, policy: &str) -> Result<()> {
        self.change(&["update", "--restart", policy, name])
    }

    /// Ends the container at once, without a clean shutdown.
    pub fn kill(&self, name: &str) -> Result<()> {
        self.change(&["kill", name])
    }

    /// Removes the container, ending it first if it is running.
    pub fn remove(&self, name: &str) -> Result<()> {
        self.change(&["rm", "--force", name])
    }

    fn change(&self, args: &[&str]) -> Result<()> {
        self.runner
            .run_ok(&self.cmd().args(args).timeout(CHANGE_TIMEOUT))
            .map(|_| ())
    }

    /// Facts about a container, or `None` if there is no container of that name.
    pub fn inspect(&self, name: &str) -> Result<Option<Container>> {
        let output = self
            .runner
            .run(&self.cmd().args(["inspect", "--type", "container", name]))?;
        if !output.success() {
            if is_no_such(&output) {
                return Ok(None);
            }
            return Err(command_failed(&self.program, "inspect", &output));
        }
        let mut containers: Vec<RawContainer> = parse_json(&output.stdout, "inspect")?;
        match containers.pop() {
            Some(raw) => Ok(Some(raw.into())),
            None => Ok(None),
        }
    }

    /// Runs a program inside a running container and returns its output, whatever
    /// its exit code.
    pub fn exec(&self, name: &str, args: &[&str], timeout: Duration) -> Result<Output> {
        self.runner
            .run(&self.cmd().args(["exec", name]).args(args).timeout(timeout))
    }

    /// Runs a throwaway container to completion and removes it.
    pub fn run_once(
        &self,
        options: &[String],
        image: &str,
        args: &[String],
        timeout: Duration,
    ) -> Result<Output> {
        let cmd = self
            .cmd()
            .args(["run", "--rm"])
            .args(options)
            .arg(image)
            .args(args)
            .timeout(timeout);
        self.runner.run_ok(&cmd)
    }

    /// The host address a container port is published on, such as
    /// `127.0.0.1:5599`, or `None` if it is not published.
    pub fn published_port(&self, name: &str, container_port: &str) -> Result<Option<String>> {
        let output = self
            .runner
            .run(&self.cmd().args(["port", name, container_port]))?;
        if !output.success() {
            return Ok(None);
        }
        Ok(output
            .stdout
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(str::to_string))
    }
}

fn is_no_such(output: &Output) -> bool {
    output.stderr.to_lowercase().contains("no such")
}

fn command_failed(program: &str, action: &str, output: &Output) -> Error {
    Error::failed(format!(
        "{program} {action} failed: {}",
        output.stderr.trim()
    ))
}

fn parse_json<T: for<'de> Deserialize<'de>>(text: &str, action: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|error| {
        Error::failed(format!(
            "could not understand the runtime's {action} output: {error}"
        ))
    })
}

#[cfg(test)]
pub(crate) mod testing {
    //! A stand-in for the runtime: a shell script that logs its arguments and
    //! prints canned answers.

    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// A stub runtime living in a temporary folder.
    pub struct Stub {
        pub temp: tempfile::TempDir,
    }

    impl Stub {
        /// Creates a stub whose behaviour is the given shell `case` arms, matched
        /// against all arguments joined by spaces. Unmatched calls succeed silently.
        pub fn new(arms: &str) -> Self {
            let temp = tempfile::tempdir().unwrap();
            let log = temp.path().join("calls.log");
            let script = format!(
                "echo \"$*\" >> '{}'\ncase \"$*\" in\n{arms}\n*) ;;\nesac\n",
                log.display()
            );
            fs::write(temp.path().join("stub.sh"), script).unwrap();
            Self { temp }
        }

        pub fn docker(&self) -> Docker {
            // Run through `sh` so no freshly written file is executed directly.
            Docker::with_program(
                Runner::default(),
                "sh",
                vec![self.script().display().to_string()],
            )
        }

        pub fn script(&self) -> PathBuf {
            self.temp.path().join("stub.sh")
        }

        /// Every call made so far, one line per call.
        pub fn calls(&self) -> Vec<String> {
            fs::read_to_string(self.temp.path().join("calls.log"))
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::Stub;
    use super::*;

    const INSPECT_RUNNING: &str = r#"[{"Id":"abc","State":{"Status":"running","Running":true,"ExitCode":0,"StartedAt":"2026-11-15T10:00:00Z"},"Config":{"Labels":{"io.verone.phone":"work","io.verone.spec":"a b"}},"NetworkSettings":{"Networks":{"bridge":{"IPAddress":"172.17.0.2"},"none":{"IPAddress":""}}},"HostConfig":{"RestartPolicy":{"Name":"unless-stopped"}}}]"#;

    #[test]
    fn version_is_trimmed() {
        let stub = Stub::new("version*) echo ' 29.9.3 ' ;;");
        assert_eq!(stub.docker().server_version().unwrap(), "29.9.3");
        assert_eq!(stub.calls(), ["version --format {{.Server.Version}}"]);
    }

    #[test]
    fn info_is_parsed() {
        let stub = Stub::new(
            r#"info*) echo '{"ServerVersion":"29.9.3","DockerRootDir":"/var/lib/docker","Driver":"overlayfs","DriverStatus":[["driver-type","io.containerd.snapshotter.v1"]],"Extra":1}' ;;"#,
        );
        let info = stub.docker().info().unwrap();
        assert_eq!(info.server_version, "29.9.3");
        assert_eq!(info.docker_root_dir, "/var/lib/docker");
        assert!(info.uses_containerd_image_store());
    }

    #[test]
    fn info_without_containerd_store() {
        let stub = Stub::new(
            r#"info*) echo '{"ServerVersion":"24.0","DockerRootDir":"/d","Driver":"overlay2","DriverStatus":[["Backing Filesystem","extfs"]]}' ;;"#,
        );
        assert!(!stub.docker().info().unwrap().uses_containerd_image_store());
    }

    #[test]
    fn an_unreachable_service_is_an_error() {
        let stub = Stub::new("info*) echo 'Cannot connect to the Docker daemon' >&2; exit 1 ;;");
        let error = stub.docker().info().unwrap_err();
        assert!(error.message().contains("Cannot connect"), "{error}");
    }

    #[test]
    fn unreadable_output_is_an_error() {
        let stub = Stub::new("info*) echo 'not json' ;;");
        let error = stub.docker().info().unwrap_err();
        assert!(error.message().contains("could not understand"), "{error}");
    }

    #[test]
    fn image_presence_distinguishes_missing_from_broken() {
        let present = Stub::new("image*) echo sha256:abc ;;");
        assert!(present.docker().image_present("x").unwrap());

        let missing =
            Stub::new("image*) echo 'Error response from daemon: No such image: x' >&2; exit 1 ;;");
        assert!(!missing.docker().image_present("x").unwrap());

        let broken = Stub::new("image*) echo 'permission denied' >&2; exit 1 ;;");
        assert!(broken.docker().image_present("x").is_err());
    }

    #[test]
    fn pull_reports_failure_with_a_hint() {
        let ok = Stub::new("");
        ok.docker().pull("repo@sha256:1").unwrap();
        assert_eq!(ok.calls(), ["pull repo@sha256:1"]);

        let failing = Stub::new("pull*) exit 1 ;;");
        let error = failing.docker().pull("repo@sha256:1").unwrap_err();
        assert!(error.hint().unwrap().contains("internet"));
    }

    #[test]
    fn create_places_options_image_and_parameters_in_order() {
        let stub = Stub::new("");
        stub.docker()
            .create(
                "verone-work",
                &[
                    "--privileged".to_string(),
                    "-v".to_string(),
                    "/d:/data".to_string(),
                ],
                "repo@sha256:1",
                &["androidboot.redroid_width=720".to_string()],
            )
            .unwrap();
        assert_eq!(
            stub.calls(),
            [
                "create --name verone-work --privileged -v /d:/data repo@sha256:1 androidboot.redroid_width=720"
            ]
        );
    }

    #[test]
    fn change_commands_use_the_expected_arguments() {
        let stub = Stub::new("");
        let docker = stub.docker();
        docker.start("c").unwrap();
        docker.update_restart("c", "no").unwrap();
        docker.kill("c").unwrap();
        docker.remove("c").unwrap();
        assert_eq!(
            stub.calls(),
            ["start c", "update --restart no c", "kill c", "rm --force c"]
        );
    }

    #[test]
    fn a_failed_change_is_an_error() {
        let stub = Stub::new("start*) echo 'boom' >&2; exit 125 ;;");
        let error = stub.docker().start("c").unwrap_err();
        assert!(error.message().contains("boom"), "{error}");
    }

    #[test]
    fn inspect_reads_the_fields_verone_uses() {
        let stub = Stub::new(&format!("inspect*) echo '{INSPECT_RUNNING}' ;;"));
        let container = stub.docker().inspect("verone-work").unwrap().unwrap();
        assert_eq!(container.status, "running");
        assert_eq!(container.exit_code, 0);
        assert_eq!(container.started_at, "2026-11-15T10:00:00Z");
        assert_eq!(container.labels["io.verone.spec"], "a b");
        assert_eq!(container.addresses, ["172.17.0.2"]);
        assert_eq!(container.restart_policy, "unless-stopped");
    }

    #[test]
    fn inspect_tolerates_missing_optional_sections() {
        let stub = Stub::new(
            r#"inspect*) echo '[{"State":{"Status":"created"},"Config":{"Labels":null},"NetworkSettings":{"Networks":null}}]' ;;"#,
        );
        let container = stub.docker().inspect("c").unwrap().unwrap();
        assert_eq!(container.status, "created");
        assert!(container.labels.is_empty());
        assert!(container.addresses.is_empty());
        assert_eq!(container.restart_policy, "");
    }

    #[test]
    fn inspect_of_a_missing_container_is_none() {
        let stub = Stub::new(
            "inspect*) echo 'Error response from daemon: No such container: c' >&2; exit 1 ;;",
        );
        assert_eq!(stub.docker().inspect("c").unwrap(), None);

        let broken = Stub::new("inspect*) echo 'daemon exploded' >&2; exit 1 ;;");
        assert!(broken.docker().inspect("c").is_err());
    }

    #[test]
    fn exec_returns_output_whatever_the_exit_code() {
        let stub = Stub::new("exec*) echo 1; exit 3 ;;");
        let output = stub
            .docker()
            .exec(
                "c",
                &["getprop", "sys.boot_completed"],
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(output.stdout.trim(), "1");
        assert_eq!(output.code, Some(3));
        assert_eq!(stub.calls(), ["exec c getprop sys.boot_completed"]);
    }

    #[test]
    fn run_once_removes_the_container_and_fails_on_error() {
        let stub = Stub::new("");
        stub.docker()
            .run_once(
                &["-v".to_string(), "/p:/p".to_string()],
                "alpine@sha256:1",
                &["rm".to_string(), "-rf".to_string(), "/p/data".to_string()],
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            stub.calls(),
            ["run --rm -v /p:/p alpine@sha256:1 rm -rf /p/data"]
        );

        let failing = Stub::new("run*) exit 1 ;;");
        assert!(
            failing
                .docker()
                .run_once(&[], "i", &[], Duration::from_secs(5))
                .is_err()
        );
    }

    #[test]
    fn published_port_is_read_or_absent() {
        let stub = Stub::new("port*) echo '127.0.0.1:5599' ;;");
        assert_eq!(
            stub.docker()
                .published_port("c", "5555/tcp")
                .unwrap()
                .as_deref(),
            Some("127.0.0.1:5599")
        );
        let none = Stub::new("port*) exit 1 ;;");
        assert_eq!(none.docker().published_port("c", "5555/tcp").unwrap(), None);
    }

    #[test]
    fn a_missing_runtime_program_is_reported() {
        let docker = Docker::with_program(Runner::default(), "verone-no-such-runtime", vec![]);
        let error = docker.info().unwrap_err();
        assert!(error.message().contains("was not found"), "{error}");
        assert_eq!(docker.program(), "verone-no-such-runtime");
    }
}
