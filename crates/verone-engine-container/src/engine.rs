//! The container engine: Android (redroid) in a privileged container.

use std::fs::DirBuilder;
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use verone_core::engine::{
    BootReport, Capabilities, ControlEndpoint, Engine, HostCheck, PhoneStatus,
};
use verone_core::process::Runner;
use verone_core::profile::{EngineKind, GraphicsMode, GraphicsUsed, LastBoot, Profile, State};
use verone_core::storage::{PhoneDir, PhoneId};
use verone_core::{Error, Result};

use crate::adb::Adb;
use crate::host::HostProbe;
use crate::images::{self, HELPER_IMAGE, IMAGES};
use crate::runtime::{Container, Docker};

/// How long Android may take to boot.
pub const BOOT_TIMEOUT: Duration = Duration::from_secs(120);

/// How long a clean shutdown may take before the container is ended by force.
pub const STOP_TIMEOUT: Duration = Duration::from_secs(20);

/// The port adb listens on inside the container.
pub const ADB_PORT: u16 = 5555;

/// Label holding the phone id.
pub const LABEL_PHONE: &str = "io.verone.phone";

/// Label holding the exact options the container was created with.
pub const LABEL_SPEC: &str = "io.verone.spec";

const BOOT_POLL_INTERVAL: Duration = Duration::from_secs(1);
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(250);
const EXEC_TIMEOUT: Duration = Duration::from_secs(10);
const ERASE_TIMEOUT: Duration = Duration::from_secs(120);

/// Restart policy while a phone is meant to be on: a restart requested inside
/// Android brings it back.
const RESTART_ON: &str = "unless-stopped";

/// Restart policy while a phone is being switched off.
const RESTART_OFF: &str = "no";

/// The graphics settings a phone is started with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedGraphics {
    /// Software or GPU.
    pub used: GraphicsUsed,
    /// The render node, in GPU mode.
    pub node: Option<String>,
}

impl ResolvedGraphics {
    fn software() -> Self {
        Self {
            used: GraphicsUsed::Software,
            node: None,
        }
    }
}

/// Everything needed to create a phone's container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerSpec {
    /// Options placed before the image.
    pub options: Vec<String>,
    /// The image reference.
    pub image: String,
    /// Android start parameters placed after the image.
    pub parameters: Vec<String>,
}

impl ContainerSpec {
    /// One line that changes whenever any part of the spec changes. Stored as a
    /// label so a later start can tell whether the container is still current.
    pub fn fingerprint(&self) -> String {
        let mut parts = self.options.clone();
        parts.push(self.image.clone());
        parts.extend(self.parameters.iter().cloned());
        parts.join(" ")
    }
}

/// Whether an image is downloaded.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ImageStatus {
    /// Android version.
    pub android: String,
    /// Name shown to people.
    pub name: String,
    /// Content digest.
    pub digest: String,
    /// True if the image is available locally.
    pub present: bool,
}

/// Runs phones as containers.
#[derive(Debug, Clone)]
pub struct ContainerEngine {
    docker: Docker,
    probe: HostProbe,
    adb: Adb,
    data_home: PathBuf,
    boot_timeout: Duration,
    gpu_boot_timeout: Duration,
}

impl ContainerEngine {
    /// An engine using the system's Docker and adb.
    pub fn new(runner: Runner, data_home: impl Into<PathBuf>) -> Self {
        Self::with_parts(
            Docker::new(runner),
            HostProbe::system(runner),
            Adb::new(runner),
            data_home,
        )
    }

    /// An engine built from the given parts. For tests.
    pub fn with_parts(
        docker: Docker,
        probe: HostProbe,
        adb: Adb,
        data_home: impl Into<PathBuf>,
    ) -> Self {
        Self {
            docker,
            probe,
            adb,
            data_home: data_home.into(),
            boot_timeout: BOOT_TIMEOUT,
            gpu_boot_timeout: BOOT_TIMEOUT,
        }
    }

    /// Sets how long a boot may take: `gpu` for a first attempt in GPU mode,
    /// `normal` otherwise. Tests shorten these.
    #[must_use]
    pub fn with_boot_timeouts(mut self, normal: Duration, gpu: Duration) -> Self {
        self.boot_timeout = normal;
        self.gpu_boot_timeout = gpu;
        self
    }

    /// The container runtime.
    pub fn docker(&self) -> &Docker {
        &self.docker
    }

    /// The adb helper.
    pub fn adb(&self) -> &Adb {
        &self.adb
    }

    /// The name of a phone's container.
    pub fn container_name(id: &PhoneId) -> String {
        format!("verone-{id}")
    }

    /// Decides the graphics settings from what the user asked for, what the host
    /// offers and what happened last time.
    pub fn resolve_graphics(&self, profile: &Profile, state: &State) -> Result<ResolvedGraphics> {
        let node = self.probe.usable_render_node();
        match profile.graphics.mode {
            GraphicsMode::Software => Ok(ResolvedGraphics::software()),
            GraphicsMode::Gpu => match node {
                Some(node) => Ok(ResolvedGraphics {
                    used: GraphicsUsed::Gpu,
                    node: Some(node.device),
                }),
                None => Err(Error::failed(format!(
                    "the phone '{}' is set to GPU graphics, but this computer has no usable GPU",
                    profile.id
                ))
                .with_hint(
                    "Run verone doctor to see the graphics check, or set the phone to auto.",
                )),
            },
            GraphicsMode::Auto => {
                let fell_back_before = state
                    .last_boot
                    .is_some_and(|boot| boot.fell_back_to_software);
                match node {
                    Some(node) if !fell_back_before => Ok(ResolvedGraphics {
                        used: GraphicsUsed::Gpu,
                        node: Some(node.device),
                    }),
                    _ => Ok(ResolvedGraphics::software()),
                }
            }
        }
    }

    /// The container options for a phone.
    pub fn spec(
        &self,
        profile: &Profile,
        phone: &PhoneDir,
        graphics: &ResolvedGraphics,
    ) -> Result<ContainerSpec> {
        let image = images::image_for(&profile.android)?;

        let mut options = vec![
            "--privileged".to_string(),
            "--restart".to_string(),
            RESTART_ON.to_string(),
            "--volume".to_string(),
            format!("{}:/data", phone.data_dir().display()),
        ];
        if profile.adb.local_port != 0 {
            options.push("--publish".to_string());
            options.push(format!("127.0.0.1:{}:{ADB_PORT}", profile.adb.local_port));
        }

        let mut parameters = vec![
            format!("androidboot.redroid_width={}", profile.display.width),
            format!("androidboot.redroid_height={}", profile.display.height),
            format!("androidboot.redroid_dpi={}", profile.display.dpi),
        ];
        match graphics.used {
            GraphicsUsed::Software => {
                parameters.push("androidboot.redroid_gpu_mode=guest".to_string());
            }
            GraphicsUsed::Gpu => {
                parameters.push("androidboot.redroid_gpu_mode=host".to_string());
                if let Some(node) = &graphics.node {
                    parameters.push(format!("androidboot.redroid_gpu_node={node}"));
                }
            }
        }

        Ok(ContainerSpec {
            options,
            image: image.reference(),
            parameters,
        })
    }

    /// Makes sure a container exists and matches the spec. A container made from
    /// an older spec is replaced; the phone's data is untouched.
    fn ensure_container(&self, profile: &Profile, spec: &ContainerSpec) -> Result<()> {
        let name = Self::container_name(&profile.id);
        let fingerprint = spec.fingerprint();
        if let Some(existing) = self.docker.inspect(&name)? {
            if existing.labels.get(LABEL_SPEC) == Some(&fingerprint) {
                return Ok(());
            }
            self.docker.remove(&name)?;
        }
        let mut options = vec![
            "--label".to_string(),
            format!("{LABEL_PHONE}={}", profile.id),
            "--label".to_string(),
            format!("{LABEL_SPEC}={fingerprint}"),
        ];
        options.extend(spec.options.iter().cloned());
        self.docker
            .create(&name, &options, &spec.image, &spec.parameters)
    }

    /// Creates the container if needed and starts it.
    fn launch(
        &self,
        profile: &Profile,
        phone: &PhoneDir,
        graphics: &ResolvedGraphics,
    ) -> Result<()> {
        let name = Self::container_name(&profile.id);
        let spec = self.spec(profile, phone, graphics)?;
        self.ensure_container(profile, &spec)?;
        self.docker.update_restart(&name, RESTART_ON)?;
        self.docker.start(&name).map_err(|error| {
            // Publishing a port needs a firewall module the runtime cannot always
            // load by itself.
            if profile.adb.local_port != 0 && error.message().contains("missing kernel module") {
                Error::failed(error.message().to_string()).with_hint(
                    "The runtime could not publish the adb port. Load the firewall module \
                     with: sudo modprobe xt_tcpudp. Or create the phone without --adb-port.",
                )
            } else {
                error
            }
        })
    }

    /// True once Android reports that it has finished booting.
    fn booted(&self, name: &str) -> bool {
        self.docker
            .exec(name, &["getprop", "sys.boot_completed"], EXEC_TIMEOUT)
            .map(|output| output.stdout.trim() == "1")
            .unwrap_or(false)
    }

    /// Waits for Android to finish booting. Returns false on timeout.
    fn wait_for_boot(&self, name: &str, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if self.booted(name) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(BOOT_POLL_INTERVAL);
        }
    }

    /// Ends a phone that failed to boot, so it does not keep restarting.
    fn abandon(&self, name: &str) {
        let _ = self.docker.update_restart(name, RESTART_OFF);
        let _ = self.docker.kill(name);
    }

    fn boot_failed(id: &PhoneId, timeout: Duration) -> Error {
        Error::failed(format!(
            "the phone '{id}' did not finish booting within {} seconds",
            timeout.as_secs()
        ))
        .with_hint(format!(
            "Run verone doctor. To see what Android reported, run: docker logs {}",
            Self::container_name(id)
        ))
    }

    /// Deletes a phone's Android data and leaves an empty data folder.
    ///
    /// The data is owned by root, so it is removed by a small helper container.
    /// The phone must be stopped.
    pub fn erase_data(&self, phone: &PhoneDir) -> Result<()> {
        if !self.docker.image_present(HELPER_IMAGE)? {
            self.docker.pull(HELPER_IMAGE)?;
        }
        self.docker.run_once(
            &[
                "--network".to_string(),
                "none".to_string(),
                "--volume".to_string(),
                format!("{}:/phone", phone.path().display()),
            ],
            HELPER_IMAGE,
            &[
                "rm".to_string(),
                "-rf".to_string(),
                "/phone/data".to_string(),
            ],
            ERASE_TIMEOUT,
        )?;
        DirBuilder::new()
            .mode(0o700)
            .create(phone.data_dir())
            .map_err(|error| {
                Error::failed(format!(
                    "could not recreate {}: {error}",
                    phone.data_dir().display()
                ))
            })
    }

    /// Every supported image and whether it is downloaded.
    pub fn image_statuses(&self) -> Result<Vec<ImageStatus>> {
        IMAGES
            .iter()
            .map(|image| {
                Ok(ImageStatus {
                    android: image.android.to_string(),
                    name: image.display_name(),
                    digest: image.digest.to_string(),
                    present: self.docker.image_present(&image.reference())?,
                })
            })
            .collect()
    }

    /// Maps the runtime's view of a container to a phone status.
    fn status_of(&self, name: &str, container: Option<&Container>) -> PhoneStatus {
        let Some(container) = container else {
            return PhoneStatus::NotCreated;
        };
        match container.status.as_str() {
            "running" if self.booted(name) => PhoneStatus::Running,
            "running" | "restarting" => PhoneStatus::Booting,
            "created" | "exited" => PhoneStatus::Stopped,
            other => PhoneStatus::Failed {
                reason: format!("the container is {other}"),
            },
        }
    }

    fn endpoint_of(container: &Container) -> Option<ControlEndpoint> {
        container.addresses.first().map(|address| ControlEndpoint {
            host: address.clone(),
            port: ADB_PORT,
        })
    }
}

impl Engine for ContainerEngine {
    fn kind(&self) -> EngineKind {
        EngineKind::Container
    }

    fn check_host(&self) -> Result<Vec<HostCheck>> {
        let image_present = IMAGES
            .first()
            .map(|image| {
                self.docker
                    .image_present(&image.reference())
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        Ok(self
            .probe
            .check_all(&self.docker, image_present, &self.data_home))
    }

    fn prepare_image(&self, android: &str) -> Result<()> {
        let image = images::image_for(android)?;
        let reference = image.reference();
        if self.docker.image_present(&reference)? {
            return Ok(());
        }
        self.docker.pull(&reference)
    }

    fn create(&self, profile: &Profile, phone: &PhoneDir) -> Result<()> {
        let state = State::load(phone)?;
        let graphics = self.resolve_graphics(profile, &state)?;
        let spec = self.spec(profile, phone, &graphics)?;
        self.ensure_container(profile, &spec)
    }

    fn destroy(&self, id: &PhoneId) -> Result<()> {
        let name = Self::container_name(id);
        if self.docker.inspect(&name)?.is_some() {
            self.docker.remove(&name)?;
        }
        Ok(())
    }

    fn start(&self, profile: &Profile, phone: &PhoneDir) -> Result<BootReport> {
        let name = Self::container_name(&profile.id);
        let mut state = State::load(phone)?;
        let mut graphics = self.resolve_graphics(profile, &state)?;
        let mut fell_back = state
            .last_boot
            .is_some_and(|boot| boot.fell_back_to_software);

        let existing = self.docker.inspect(&name)?;
        let running = existing
            .as_ref()
            .is_some_and(|container| container.status == "running");
        if running && self.booted(&name) {
            return Ok(BootReport {
                duration: Duration::ZERO,
                graphics: state.last_boot.map_or(graphics.used, |boot| boot.graphics),
                fell_back_to_software: fell_back,
                already_running: true,
            });
        }

        self.prepare_image(&profile.android)?;
        let started = Instant::now();
        if !running {
            self.launch(profile, phone, &graphics)?;
        }

        let first_timeout = match graphics.used {
            GraphicsUsed::Gpu => self.gpu_boot_timeout,
            GraphicsUsed::Software => self.boot_timeout,
        };
        if !self.wait_for_boot(&name, first_timeout) {
            let can_fall_back =
                graphics.used == GraphicsUsed::Gpu && profile.graphics.mode == GraphicsMode::Auto;
            if !can_fall_back {
                self.abandon(&name);
                return Err(Self::boot_failed(&profile.id, first_timeout));
            }
            // GPU mode did not come up. Try once more in software.
            self.docker.remove(&name)?;
            graphics = ResolvedGraphics::software();
            fell_back = true;
            self.launch(profile, phone, &graphics)?;
            if !self.wait_for_boot(&name, self.boot_timeout) {
                self.abandon(&name);
                return Err(Self::boot_failed(&profile.id, self.boot_timeout));
            }
        }

        state.last_boot = Some(LastBoot {
            graphics: graphics.used,
            fell_back_to_software: fell_back,
        });
        state.save(phone)?;

        Ok(BootReport {
            duration: started.elapsed(),
            graphics: graphics.used,
            fell_back_to_software: fell_back,
            already_running: false,
        })
    }

    fn stop(&self, id: &PhoneId) -> Result<()> {
        let name = Self::container_name(id);
        let Some(container) = self.docker.inspect(&name)? else {
            return Ok(());
        };
        if matches!(container.status.as_str(), "created" | "exited") {
            return Ok(());
        }
        if let Some(endpoint) = Self::endpoint_of(&container) {
            // Another container can receive this address later.
            self.adb.disconnect(&endpoint);
        }

        // The runtime's own stop only kills Android after a grace period. Switch
        // the restart policy off, then power Android off from inside.
        self.docker.update_restart(&name, RESTART_OFF)?;
        if container.status == "running" {
            let _ = self.docker.exec(&name, &["reboot", "-p"], EXEC_TIMEOUT);
            let deadline = Instant::now() + STOP_TIMEOUT;
            while Instant::now() < deadline {
                let stopped = self
                    .docker
                    .inspect(&name)?
                    .is_none_or(|current| current.status == "exited");
                if stopped {
                    return Ok(());
                }
                thread::sleep(STOP_POLL_INTERVAL);
            }
        }
        self.docker.kill(&name)
    }

    fn status(&self, id: &PhoneId) -> Result<PhoneStatus> {
        let name = Self::container_name(id);
        let container = self.docker.inspect(&name)?;
        Ok(self.status_of(&name, container.as_ref()))
    }

    fn control_endpoint(&self, id: &PhoneId) -> Result<ControlEndpoint> {
        let name = Self::container_name(id);
        let not_running = || {
            Error::failed(format!("the phone '{id}' is not running"))
                .with_hint(format!("Start it with: verone start {id}"))
        };
        let container = self.docker.inspect(&name)?.ok_or_else(not_running)?;
        if container.status != "running" {
            return Err(not_running());
        }
        Self::endpoint_of(&container).ok_or_else(|| {
            Error::failed(format!("the phone '{id}' has no network address"))
                .with_hint("Restart it with: verone restart ".to_string() + id.as_str())
        })
    }

    fn capabilities(&self) -> Result<Capabilities> {
        Ok(Capabilities {
            gpu_rendering: self.probe.usable_render_node().is_some(),
            root_access: true,
            own_kernel: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::testing::Stub;
    use std::path::Path;
    use std::sync::Arc;
    use verone_core::ErrorKind;
    use verone_core::storage::Storage;

    struct Fixture {
        temp: tempfile::TempDir,
        stub: Stub,
        profile: Profile,
        phone: PhoneDir,
    }

    impl Fixture {
        /// `arms` are the stub runtime's answers.
        fn new(arms: &str) -> Self {
            let temp = tempfile::tempdir().unwrap();
            let id = PhoneId::new("work").unwrap();
            let phone = Storage::new(temp.path().join("home"))
                .create_phone(&id)
                .unwrap();
            Self {
                stub: Stub::new(arms),
                profile: Profile::new(id),
                phone,
                temp,
            }
        }

        /// A fake host with the given render nodes as (name, driver).
        fn engine(&self, nodes: &[(&str, &str)]) -> ContainerEngine {
            let root = self.temp.path().join("root");
            for (name, driver) in nodes {
                let device = root.join("sys/class/drm").join(name).join("device");
                std::fs::create_dir_all(&device).unwrap();
                std::os::unix::fs::symlink(format!("../drivers/{driver}"), device.join("driver"))
                    .unwrap();
            }
            let probe = HostProbe::custom(root, vec![], Arc::new(|_: &Path| Ok(u64::MAX)));
            let adb = Adb::with_program(
                Runner::default(),
                "sh",
                vec![self.stub.script().display().to_string(), "adb".to_string()],
            );
            ContainerEngine::with_parts(self.stub.docker(), probe, adb, self.temp.path())
                .with_boot_timeouts(Duration::from_millis(50), Duration::from_millis(50))
        }

        fn id(&self) -> &PhoneId {
            &self.profile.id
        }
    }

    fn inspect_arm(status: &str, spec: &str) -> String {
        format!(
            r#"inspect*) echo '[{{"State":{{"Status":"{status}","ExitCode":0}},"Config":{{"Labels":{{"io.verone.spec":"{spec}"}}}},"NetworkSettings":{{"Networks":{{"bridge":{{"IPAddress":"172.17.0.9"}}}}}}}}]' ;;"#
        )
    }

    const NO_CONTAINER: &str =
        "inspect*) echo 'Error response from daemon: No such container: x' >&2; exit 1 ;;";

    // ---- graphics ----

    #[test]
    fn graphics_follow_the_mode_and_the_host() {
        let f = Fixture::new("");
        let state = State::default();
        let with_gpu = f.engine(&[("renderD129", "nvidia"), ("renderD128", "i915")]);

        let mut profile = f.profile.clone();
        let auto = with_gpu.resolve_graphics(&profile, &state).unwrap();
        assert_eq!(auto.used, GraphicsUsed::Gpu);
        assert_eq!(auto.node.as_deref(), Some("/dev/dri/renderD128"));

        profile.graphics.mode = GraphicsMode::Software;
        assert_eq!(
            with_gpu.resolve_graphics(&profile, &state).unwrap(),
            ResolvedGraphics::software()
        );

        profile.graphics.mode = GraphicsMode::Gpu;
        assert_eq!(
            with_gpu.resolve_graphics(&profile, &state).unwrap().used,
            GraphicsUsed::Gpu
        );
    }

    #[test]
    fn without_a_usable_gpu_auto_is_software_and_gpu_is_an_error() {
        let f = Fixture::new("");
        let engine = f.engine(&[("renderD128", "nvidia")]);
        let state = State::default();

        let mut profile = f.profile.clone();
        assert_eq!(
            engine.resolve_graphics(&profile, &state).unwrap(),
            ResolvedGraphics::software()
        );

        profile.graphics.mode = GraphicsMode::Gpu;
        let error = engine.resolve_graphics(&profile, &state).unwrap_err();
        assert!(error.message().contains("no usable GPU"), "{error}");
        assert!(error.hint().unwrap().contains("verone doctor"));
    }

    #[test]
    fn auto_stays_in_software_after_a_recorded_fallback() {
        let f = Fixture::new("");
        let engine = f.engine(&[("renderD128", "i915")]);
        let state = State {
            last_boot: Some(LastBoot {
                graphics: GraphicsUsed::Software,
                fell_back_to_software: true,
            }),
            ..State::default()
        };
        assert_eq!(
            engine.resolve_graphics(&f.profile, &state).unwrap(),
            ResolvedGraphics::software()
        );
    }

    // ---- spec ----

    #[test]
    fn the_spec_matches_the_fixed_design() {
        let f = Fixture::new("");
        let engine = f.engine(&[]);
        let spec = engine
            .spec(&f.profile, &f.phone, &ResolvedGraphics::software())
            .unwrap();
        assert_eq!(
            spec.options,
            [
                "--privileged",
                "--restart",
                "unless-stopped",
                "--volume",
                &format!("{}:/data", f.phone.data_dir().display()),
            ]
        );
        assert_eq!(
            spec.image,
            "redroid/redroid@sha256:a3e654caad74faed03cd5ead057a989e06f507778cda678e5d3b9772e0092f1c"
        );
        assert_eq!(
            spec.parameters,
            [
                "androidboot.redroid_width=720",
                "androidboot.redroid_height=1280",
                "androidboot.redroid_dpi=320",
                "androidboot.redroid_gpu_mode=guest",
            ]
        );
    }

    #[test]
    fn gpu_mode_and_a_published_port_appear_in_the_spec() {
        let f = Fixture::new("");
        let engine = f.engine(&[]);
        let mut profile = f.profile.clone();
        profile.adb.local_port = 5599;
        let graphics = ResolvedGraphics {
            used: GraphicsUsed::Gpu,
            node: Some("/dev/dri/renderD128".to_string()),
        };
        let spec = engine.spec(&profile, &f.phone, &graphics).unwrap();
        assert!(
            spec.options
                .ends_with(&["--publish".to_string(), "127.0.0.1:5599:5555".to_string()])
        );
        assert!(spec.parameters.ends_with(&[
            "androidboot.redroid_gpu_mode=host".to_string(),
            "androidboot.redroid_gpu_node=/dev/dri/renderD128".to_string()
        ]));
    }

    #[test]
    fn the_fingerprint_changes_with_any_setting() {
        let f = Fixture::new("");
        let engine = f.engine(&[]);
        let software = ResolvedGraphics::software();
        let base = engine
            .spec(&f.profile, &f.phone, &software)
            .unwrap()
            .fingerprint();

        let mut wider = f.profile.clone();
        wider.display.width = 1080;
        assert_ne!(
            engine
                .spec(&wider, &f.phone, &software)
                .unwrap()
                .fingerprint(),
            base
        );

        let mut port = f.profile.clone();
        port.adb.local_port = 5599;
        assert_ne!(
            engine
                .spec(&port, &f.phone, &software)
                .unwrap()
                .fingerprint(),
            base
        );

        assert_eq!(
            engine
                .spec(&f.profile, &f.phone, &software)
                .unwrap()
                .fingerprint(),
            base
        );
    }

    #[test]
    fn an_unsupported_android_version_is_not_found() {
        let f = Fixture::new("");
        let engine = f.engine(&[]);
        let mut profile = f.profile.clone();
        profile.android = "9".to_string();
        let error = engine
            .spec(&profile, &f.phone, &ResolvedGraphics::software())
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
        assert_eq!(engine.prepare_image("9").unwrap_err().exit_code(), 4);
    }

    // ---- create ----

    #[test]
    fn create_makes_a_labelled_container_when_none_exists() {
        let f = Fixture::new(NO_CONTAINER);
        f.engine(&[]).create(&f.profile, &f.phone).unwrap();
        let calls = f.stub.calls();
        let create = calls
            .iter()
            .find(|call| call.starts_with("create"))
            .unwrap();
        assert!(create.starts_with("create --name verone-work --label io.verone.phone=work --label io.verone.spec=--privileged"), "{create}");
        assert!(create.contains("androidboot.redroid_gpu_mode=guest"));
    }

    #[test]
    fn create_leaves_a_current_container_alone() {
        let probe = Fixture::new("");
        let fingerprint = probe
            .engine(&[])
            .spec(&probe.profile, &probe.phone, &ResolvedGraphics::software())
            .unwrap()
            .fingerprint();
        // The fingerprint contains the data path, so reuse the same fixture folder.
        let stub = Stub::new(&inspect_arm("exited", &fingerprint));
        let f = Fixture { stub, ..probe };
        f.engine(&[]).create(&f.profile, &f.phone).unwrap();
        assert!(
            f.stub
                .calls()
                .iter()
                .all(|call| call.starts_with("inspect"))
        );
    }

    #[test]
    fn create_replaces_a_container_made_from_an_older_spec() {
        let f = Fixture::new(&inspect_arm("exited", "something older"));
        f.engine(&[]).create(&f.profile, &f.phone).unwrap();
        let calls = f.stub.calls();
        assert!(calls.iter().any(|call| call == "rm --force verone-work"));
        assert!(
            calls
                .iter()
                .any(|call| call.starts_with("create --name verone-work"))
        );
    }

    // ---- status ----

    #[test]
    fn status_maps_the_runtime_states() {
        let cases = [
            (NO_CONTAINER.to_string(), PhoneStatus::NotCreated),
            (inspect_arm("created", ""), PhoneStatus::Stopped),
            (inspect_arm("exited", ""), PhoneStatus::Stopped),
            (inspect_arm("restarting", ""), PhoneStatus::Booting),
            (inspect_arm("running", ""), PhoneStatus::Booting),
            (
                format!("{}\nexec*) echo 1 ;;", inspect_arm("running", "")),
                PhoneStatus::Running,
            ),
        ];
        for (arms, expected) in cases {
            let f = Fixture::new(&arms);
            assert_eq!(f.engine(&[]).status(f.id()).unwrap(), expected, "{arms}");
        }

        let f = Fixture::new(&inspect_arm("dead", ""));
        assert!(matches!(
            f.engine(&[]).status(f.id()).unwrap(),
            PhoneStatus::Failed { .. }
        ));
    }

    // ---- stop ----

    #[test]
    fn stopping_a_phone_with_no_container_or_a_stopped_one_does_nothing() {
        let f = Fixture::new(NO_CONTAINER);
        f.engine(&[]).stop(f.id()).unwrap();
        assert_eq!(f.stub.calls().len(), 1);

        let f = Fixture::new(&inspect_arm("exited", ""));
        f.engine(&[]).stop(f.id()).unwrap();
        assert_eq!(f.stub.calls().len(), 1);
    }

    #[test]
    fn stop_powers_android_off_from_inside_and_never_uses_the_runtime_stop() {
        // The container reports "running" until `reboot -p` has been run.
        let f = Fixture::new(
            r#"exec*) touch "$(dirname "$0")/off" ;;
inspect*) if [ -f "$(dirname "$0")/off" ]; then S=exited; else S=running; fi
  echo "[{\"State\":{\"Status\":\"$S\",\"ExitCode\":130},\"NetworkSettings\":{\"Networks\":{\"bridge\":{\"IPAddress\":\"172.17.0.9\"}}}}]" ;;"#,
        );
        f.engine(&[]).stop(f.id()).unwrap();
        let calls = f.stub.calls();
        assert_eq!(
            calls,
            [
                "inspect --type container verone-work",
                "adb disconnect 172.17.0.9:5555",
                "update --restart no verone-work",
                "exec verone-work reboot -p",
                "inspect --type container verone-work",
            ]
        );
        assert!(calls.iter().all(|call| !call.starts_with("stop")));
    }

    #[test]
    fn a_container_that_is_restarting_is_ended_without_a_shutdown() {
        let f = Fixture::new(&inspect_arm("restarting", ""));
        f.engine(&[]).stop(f.id()).unwrap();
        let calls = f.stub.calls();
        assert!(calls.contains(&"update --restart no verone-work".to_string()));
        assert!(calls.contains(&"kill verone-work".to_string()));
        assert!(calls.iter().all(|call| !call.starts_with("exec")));
    }

    // ---- start ----

    #[test]
    fn starting_a_running_phone_does_nothing() {
        let f = Fixture::new(&format!("{}\nexec*) echo 1 ;;", inspect_arm("running", "")));
        let report = f.engine(&[]).start(&f.profile, &f.phone).unwrap();
        assert!(report.already_running);
        assert_eq!(report.duration, Duration::ZERO);
        assert!(f.stub.calls().iter().all(|call| !call.starts_with("start")));
    }

    #[test]
    fn a_boot_that_never_completes_is_abandoned_with_guidance() {
        let f = Fixture::new(NO_CONTAINER);
        let error = f.engine(&[]).start(&f.profile, &f.phone).unwrap_err();
        assert!(
            error.message().contains("did not finish booting"),
            "{error}"
        );
        assert!(error.hint().unwrap().contains("docker logs verone-work"));
        let calls = f.stub.calls();
        assert!(calls.contains(&"start verone-work".to_string()));
        assert!(calls.contains(&"update --restart no verone-work".to_string()));
        assert!(calls.contains(&"kill verone-work".to_string()));
        assert_eq!(State::load(&f.phone).unwrap(), State::default());
    }

    #[test]
    fn auto_falls_back_to_software_once_when_gpu_mode_does_not_boot() {
        // Android only "boots" when the container was created in software mode.
        let f = Fixture::new(&format!(
            r#"create*guest*) touch "$(dirname "$0")/software" ;;
exec*) [ -f "$(dirname "$0")/software" ] && echo 1 ;;
{NO_CONTAINER}"#
        ));
        let report = f
            .engine(&[("renderD128", "i915")])
            .start(&f.profile, &f.phone)
            .unwrap();
        assert_eq!(report.graphics, GraphicsUsed::Software);
        assert!(report.fell_back_to_software);
        assert!(!report.already_running);

        let creates: Vec<_> = f
            .stub
            .calls()
            .into_iter()
            .filter(|call| call.starts_with("create"))
            .collect();
        assert_eq!(creates.len(), 2);
        assert!(creates[0].contains("redroid_gpu_mode=host"));
        assert!(creates[1].contains("redroid_gpu_mode=guest"));

        let state = State::load(&f.phone).unwrap();
        assert_eq!(
            state.last_boot,
            Some(LastBoot {
                graphics: GraphicsUsed::Software,
                fell_back_to_software: true
            })
        );
    }

    #[test]
    fn an_explicit_gpu_phone_does_not_fall_back() {
        let f = Fixture::new(NO_CONTAINER);
        let mut profile = f.profile.clone();
        profile.graphics.mode = GraphicsMode::Gpu;
        let error = f
            .engine(&[("renderD128", "i915")])
            .start(&profile, &f.phone)
            .unwrap_err();
        assert!(
            error.message().contains("did not finish booting"),
            "{error}"
        );
        let creates = f
            .stub
            .calls()
            .iter()
            .filter(|call| call.starts_with("create"))
            .count();
        assert_eq!(creates, 1);
    }

    #[test]
    fn a_port_that_cannot_be_published_explains_the_missing_module() {
        let arms = format!(
            "start*) echo 'Unable to enable LOOPBACK FILTERING - DROP rule: missing kernel module?' >&2; exit 1 ;;\n{NO_CONTAINER}"
        );
        let f = Fixture::new(&arms);
        let mut profile = f.profile.clone();
        profile.adb.local_port = 5599;
        let error = f.engine(&[]).start(&profile, &f.phone).unwrap_err();
        assert!(
            error.hint().unwrap().contains("sudo modprobe xt_tcpudp"),
            "{error:?}"
        );

        // Without a published port the runtime's own error is passed on unchanged.
        let f = Fixture::new(&arms);
        let error = f.engine(&[]).start(&f.profile, &f.phone).unwrap_err();
        assert!(
            error.hint().unwrap().contains("The command was"),
            "{error:?}"
        );
    }

    // ---- endpoint, destroy, images, capabilities ----

    #[test]
    fn the_endpoint_is_the_container_address_while_running() {
        let f = Fixture::new(&inspect_arm("running", ""));
        assert_eq!(
            f.engine(&[]).control_endpoint(f.id()).unwrap().to_string(),
            "172.17.0.9:5555"
        );

        for arms in [inspect_arm("exited", ""), NO_CONTAINER.to_string()] {
            let f = Fixture::new(&arms);
            let error = f.engine(&[]).control_endpoint(f.id()).unwrap_err();
            assert!(error.message().contains("not running"), "{error}");
            assert!(error.hint().unwrap().contains("verone start work"));
        }
    }

    #[test]
    fn destroy_removes_an_existing_container_only() {
        let f = Fixture::new(&inspect_arm("exited", ""));
        f.engine(&[]).destroy(f.id()).unwrap();
        assert!(
            f.stub
                .calls()
                .contains(&"rm --force verone-work".to_string())
        );

        let f = Fixture::new(NO_CONTAINER);
        f.engine(&[]).destroy(f.id()).unwrap();
        assert_eq!(f.stub.calls().len(), 1);
    }

    #[test]
    fn an_image_is_pulled_only_when_absent() {
        let present = Fixture::new("image*) echo sha256:abc ;;");
        present.engine(&[]).prepare_image("14").unwrap();
        assert!(
            present
                .stub
                .calls()
                .iter()
                .all(|call| !call.starts_with("pull"))
        );

        let absent = Fixture::new("image*) echo 'No such image' >&2; exit 1 ;;");
        absent.engine(&[]).prepare_image("14").unwrap();
        assert!(
            absent
                .stub
                .calls()
                .iter()
                .any(|call| call.starts_with("pull redroid/redroid@sha256:a3e654"))
        );

        let statuses = absent.engine(&[]).image_statuses().unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].android, "14");
        assert!(!statuses[0].present);
    }

    #[test]
    fn capabilities_reflect_the_host() {
        let f = Fixture::new("");
        let with_gpu = f.engine(&[("renderD128", "i915")]).capabilities().unwrap();
        assert!(with_gpu.gpu_rendering);
        assert!(with_gpu.root_access);
        assert!(!with_gpu.own_kernel);
        assert_eq!(f.engine(&[]).kind(), EngineKind::Container);
    }

    #[test]
    fn host_checks_run_through_the_engine() {
        let f = Fixture::new("info*) exit 1 ;;");
        let checks = f.engine(&[]).check_host().unwrap();
        assert_eq!(checks.len(), 9);
        assert!(verone_core::engine::any_failed(&checks));
    }
}
