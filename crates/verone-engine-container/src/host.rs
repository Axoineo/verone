//! Checks that this machine can run phones in containers.
//!
//! Every check reads the system through a [`HostProbe`], whose root folder, search
//! path and free-space lookup can be replaced, so tests run against a fake system.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use verone_core::engine::HostCheck;
use verone_core::process::{Cmd, Runner};
use verone_core::{Error, Result};

use crate::runtime::Docker;

/// Free space needed where the runtime stores images, when the image is absent.
pub const IMAGE_SPACE_BYTES: u64 = 4 * GIB;

/// Free space needed where phone data is kept.
pub const DATA_SPACE_BYTES: u64 = 2 * GIB;

/// Available memory below which the user is warned.
pub const MEMORY_WARNING_BYTES: u64 = 3 * GIB / 2;

const GIB: u64 = 1024 * 1024 * 1024;

/// The folder where containerd keeps images when the runtime delegates to it.
const CONTAINERD_ROOT: &str = "var/lib/containerd";

/// The graphics driver that cannot be used for rendering in a container.
const UNUSABLE_DRIVER: &str = "nvidia";

/// Looks up the free space, in bytes, of the filesystem holding a path.
pub type FreeSpace = Arc<dyn Fn(&Path) -> Result<u64> + Send + Sync>;

/// A GPU render node and the driver behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderNode {
    /// The device path, for example `/dev/dri/renderD128`.
    pub device: String,
    /// The kernel driver, for example `i915`.
    pub driver: String,
}

impl RenderNode {
    /// True if Android can render through this node from a container.
    pub fn usable(&self) -> bool {
        self.driver != UNUSABLE_DRIVER
    }
}

/// Reads facts about the host.
#[derive(Clone)]
pub struct HostProbe {
    root: PathBuf,
    path_dirs: Vec<PathBuf>,
    free_space: FreeSpace,
}

impl std::fmt::Debug for HostProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostProbe")
            .field("root", &self.root)
            .field("path_dirs", &self.path_dirs)
            .finish_non_exhaustive()
    }
}

impl HostProbe {
    /// Probes the real system.
    pub fn system(runner: Runner) -> Self {
        let path_dirs = std::env::var_os("PATH")
            .map(|path| std::env::split_paths(&path).collect())
            .unwrap_or_default();
        Self {
            root: PathBuf::from("/"),
            path_dirs,
            free_space: Arc::new(move |path| free_space_from_df(runner, path)),
        }
    }

    /// Probes a system described by the caller. For tests.
    pub fn custom(
        root: impl Into<PathBuf>,
        path_dirs: Vec<PathBuf>,
        free_space: FreeSpace,
    ) -> Self {
        Self {
            root: root.into(),
            path_dirs,
            free_space,
        }
    }

    fn at(&self, absolute: &str) -> PathBuf {
        self.root.join(absolute.trim_start_matches('/'))
    }

    /// True if the kernel offers the binder filesystem Android needs.
    pub fn has_binder(&self) -> bool {
        fs::read_to_string(self.at("/proc/filesystems"))
            .map(|text| {
                text.lines()
                    .any(|line| line.split_whitespace().last() == Some("binder"))
            })
            .unwrap_or(false)
    }

    /// The release name of the running kernel.
    pub fn kernel_release(&self) -> Option<String> {
        fs::read_to_string(self.at("/proc/sys/kernel/osrelease"))
            .ok()
            .map(|text| text.trim().to_string())
            .filter(|release| !release.is_empty())
    }

    /// True if the modules of the running kernel are installed. They go missing
    /// when the kernel package is updated and the machine has not been restarted.
    pub fn has_modules_for(&self, release: &str) -> bool {
        self.at("/lib/modules").join(release).is_dir()
    }

    /// Memory available for new programs, in bytes.
    pub fn available_memory(&self) -> Option<u64> {
        let text = fs::read_to_string(self.at("/proc/meminfo")).ok()?;
        let line = text
            .lines()
            .find(|line| line.starts_with("MemAvailable:"))?;
        let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(kib * 1024)
    }

    /// The GPU render nodes, in device order.
    pub fn render_nodes(&self) -> Vec<RenderNode> {
        let Ok(entries) = fs::read_dir(self.at("/sys/class/drm")) else {
            return Vec::new();
        };
        let mut nodes: Vec<RenderNode> = entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let name = entry.file_name().to_str()?.to_string();
                if !name.starts_with("renderD") {
                    return None;
                }
                let link = fs::read_link(entry.path().join("device").join("driver")).ok()?;
                let driver = link.file_name()?.to_str()?.to_string();
                Some(RenderNode {
                    device: format!("/dev/dri/{name}"),
                    driver,
                })
            })
            .collect();
        nodes.sort_by(|a, b| a.device.cmp(&b.device));
        nodes
    }

    /// The first render node Android can use, if any.
    pub fn usable_render_node(&self) -> Option<RenderNode> {
        self.render_nodes().into_iter().find(RenderNode::usable)
    }

    /// True if a program of this name is on the search path.
    pub fn has_program(&self, name: &str) -> bool {
        self.path_dirs.iter().any(|dir| dir.join(name).is_file())
    }

    /// Free space of the filesystem holding `path`, or of its nearest existing
    /// parent if `path` does not exist yet.
    pub fn free_space(&self, path: &Path) -> Result<u64> {
        let mut existing = path;
        while !existing.exists() {
            match existing.parent() {
                Some(parent) => existing = parent,
                None => break,
            }
        }
        (self.free_space)(existing)
    }

    // ---- checks ----

    /// Is the runtime installed and reachable?
    pub fn check_runtime(&self, docker: &Docker) -> HostCheck {
        const NAME: &str = "Container runtime";
        match docker.info() {
            Ok(info) => HostCheck::ok(NAME, format!("Docker {}", info.server_version)),
            Err(error) if error.message().contains("was not found") => HostCheck::failed(
                NAME,
                format!("the program '{}' is not installed", docker.program()),
                "Install Docker (on Arch: sudo pacman -S docker), then start it: \
                 sudo systemctl enable --now docker",
            ),
            Err(error) => HostCheck::failed(
                NAME,
                format!(
                    "Docker is installed but cannot be used: {}",
                    error.message()
                ),
                "Start the service (sudo systemctl enable --now docker) and make sure your \
                 user is in the docker group (sudo usermod -aG docker $USER, then log in again).",
            ),
        }
    }

    /// Does the kernel provide binder?
    pub fn check_binder(&self) -> HostCheck {
        const NAME: &str = "Kernel binder support";
        if self.has_binder() {
            HostCheck::ok(NAME, "available")
        } else {
            HostCheck::failed(
                NAME,
                "the kernel does not offer the binder filesystem",
                "Load it with: sudo modprobe binder_linux. On Ubuntu, first install the \
                 package linux-modules-extra-$(uname -r).",
            )
        }
    }

    /// Are the running kernel's modules installed?
    pub fn check_kernel_modules(&self) -> HostCheck {
        const NAME: &str = "Kernel modules";
        match self.kernel_release() {
            Some(release) if self.has_modules_for(&release) => {
                HostCheck::ok(NAME, format!("installed for kernel {release}"))
            }
            Some(release) => HostCheck::failed(
                NAME,
                format!("no modules are installed for the running kernel {release}"),
                "The kernel was updated since the last start. Restart the computer.",
            ),
            None => HostCheck::warning(
                NAME,
                "could not read the running kernel's version",
                "If phones fail to start, restart the computer.",
            ),
        }
    }

    /// Is there room for the Android image where the runtime keeps images?
    pub fn check_image_space(&self, docker: &Docker, image_present: bool) -> HostCheck {
        const NAME: &str = "Space for the Android image";
        if image_present {
            return HostCheck::ok(NAME, "the image is already downloaded");
        }
        let Ok(info) = docker.info() else {
            return HostCheck::warning(
                NAME,
                "could not ask the runtime where it stores images",
                "Fix the container runtime first.",
            );
        };
        let mut places = vec![PathBuf::from(&info.docker_root_dir)];
        let containerd = self.at(CONTAINERD_ROOT);
        if info.uses_containerd_image_store() && containerd.is_dir() {
            places.push(PathBuf::from("/").join(CONTAINERD_ROOT));
        }
        self.check_space(NAME, &places, IMAGE_SPACE_BYTES)
    }

    /// Is there room for phone data?
    pub fn check_data_space(&self, data_home: &Path) -> HostCheck {
        self.check_space(
            "Space for phone data",
            &[data_home.to_path_buf()],
            DATA_SPACE_BYTES,
        )
    }

    fn check_space(&self, name: &str, places: &[PathBuf], needed: u64) -> HostCheck {
        let mut tightest: Option<(u64, &PathBuf)> = None;
        for place in places {
            match self.free_space(place) {
                Ok(free) => {
                    if tightest.is_none_or(|(least, _)| free < least) {
                        tightest = Some((free, place));
                    }
                }
                Err(error) => {
                    return HostCheck::warning(
                        name,
                        format!(
                            "could not measure free space at {}: {}",
                            place.display(),
                            error.message()
                        ),
                        format!("Make sure {} is free there.", gib(needed)),
                    );
                }
            }
        }
        match tightest {
            Some((free, place)) if free < needed => HostCheck::failed(
                name,
                format!(
                    "{} free at {}, {} needed",
                    gib(free),
                    place.display(),
                    gib(needed)
                ),
                format!(
                    "Free at least {} at {}.",
                    gib(needed - free),
                    place.display()
                ),
            ),
            Some((free, place)) => {
                HostCheck::ok(name, format!("{} free at {}", gib(free), place.display()))
            }
            None => HostCheck::ok(name, "nothing to check"),
        }
    }

    /// Is there enough memory to run a phone comfortably?
    pub fn check_memory(&self) -> HostCheck {
        const NAME: &str = "Memory";
        match self.available_memory() {
            Some(available) if available < MEMORY_WARNING_BYTES => HostCheck::warning(
                NAME,
                format!("{} available; a phone uses about 1 GB", gib(available)),
                "Close some applications before starting a phone.",
            ),
            Some(available) => HostCheck::ok(NAME, format!("{} available", gib(available))),
            None => HostCheck::ok(NAME, "could not be measured"),
        }
    }

    /// Which graphics mode will `auto` use? Never fails.
    pub fn check_graphics(&self) -> HostCheck {
        const NAME: &str = "Graphics";
        let nodes = self.render_nodes();
        let listed = nodes
            .iter()
            .map(|node| format!("{} ({})", node.device, node.driver))
            .collect::<Vec<_>>()
            .join(", ");
        match nodes.iter().find(|node| node.usable()) {
            Some(node) => HostCheck::ok(
                NAME,
                format!("auto will use the GPU at {}; found: {listed}", node.device),
            ),
            None if nodes.is_empty() => {
                HostCheck::ok(NAME, "no GPU found; auto will use software rendering")
            }
            None => HostCheck::ok(
                NAME,
                format!(
                    "auto will use software rendering, because the only GPU uses the \
                     {UNUSABLE_DRIVER} driver; found: {listed}"
                ),
            ),
        }
    }

    /// Is a helper program installed? Missing ones only limit some commands.
    pub fn check_program(&self, program: &str, used_for: &str, package: &str) -> HostCheck {
        if self.has_program(program) {
            HostCheck::ok(program, "installed")
        } else {
            HostCheck::warning(
                program,
                format!("not installed; needed for {used_for}"),
                format!("Install the package {package}."),
            )
        }
    }

    /// Runs every check, in the order they are shown to the user.
    pub fn check_all(
        &self,
        docker: &Docker,
        image_present: bool,
        data_home: &Path,
    ) -> Vec<HostCheck> {
        vec![
            self.check_runtime(docker),
            self.check_binder(),
            self.check_kernel_modules(),
            self.check_image_space(docker, image_present),
            self.check_data_space(data_home),
            self.check_memory(),
            self.check_graphics(),
            self.check_program(
                "adb",
                "verone adb, verone install and verone open",
                "android-tools",
            ),
            self.check_program("scrcpy", "verone open", "scrcpy"),
        ]
    }
}

/// Formats a byte count as gigabytes with one decimal.
fn gib(bytes: u64) -> String {
    let tenths = bytes.saturating_mul(10) / GIB;
    format!("{}.{} GB", tenths / 10, tenths % 10)
}

/// Measures free space by running `df`, which every Linux system has.
fn free_space_from_df(runner: Runner, path: &Path) -> Result<u64> {
    let output = runner.run_ok(
        &Cmd::new("df")
            .arg("-Pk")
            .arg(path)
            .timeout(Duration::from_secs(10)),
    )?;
    parse_df(&output.stdout)
}

/// Reads the "available" column of `df -Pk` output and returns it in bytes.
fn parse_df(text: &str) -> Result<u64> {
    text.lines()
        .nth(1)
        .and_then(|line| line.split_whitespace().nth(3))
        .and_then(|kib| kib.parse::<u64>().ok())
        .map(|kib| kib * 1024)
        .ok_or_else(|| Error::failed("could not understand the output of df"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::testing::Stub;
    use std::os::unix::fs::symlink;
    use verone_core::engine::CheckOutcome;

    const INFO: &str = r#"info*) echo '{"ServerVersion":"29.9.3","DockerRootDir":"/var/lib/docker","Driver":"overlayfs","DriverStatus":[["driver-type","io.containerd.snapshotter.v1"]]}' ;;"#;

    /// A fake system tree in a temporary folder.
    struct FakeHost {
        temp: tempfile::TempDir,
    }

    impl FakeHost {
        fn new() -> Self {
            Self {
                temp: tempfile::tempdir().unwrap(),
            }
        }

        fn root(&self) -> &Path {
            self.temp.path()
        }

        fn write(&self, path: &str, text: &str) -> &Self {
            let full = self.root().join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, text).unwrap();
            self
        }

        fn dir(&self, path: &str) -> &Self {
            fs::create_dir_all(self.root().join(path)).unwrap();
            self
        }

        fn render_node(&self, name: &str, driver: &str) -> &Self {
            let device = self.root().join("sys/class/drm").join(name).join("device");
            fs::create_dir_all(&device).unwrap();
            symlink(
                format!("../../../bus/drivers/{driver}"),
                device.join("driver"),
            )
            .unwrap();
            self
        }

        fn probe(&self, free: u64) -> HostProbe {
            HostProbe::custom(
                self.root(),
                vec![self.root().join("bin")],
                Arc::new(move |_| Ok(free)),
            )
        }

        /// A probe whose free space depends on the path asked about.
        fn probe_with(
            &self,
            free: impl Fn(&Path) -> Result<u64> + Send + Sync + 'static,
        ) -> HostProbe {
            HostProbe::custom(self.root(), vec![self.root().join("bin")], Arc::new(free))
        }
    }

    fn assert_check(check: &HostCheck, outcome: CheckOutcome, detail: &str) {
        assert_eq!(check.outcome, outcome, "{check:?}");
        assert!(check.detail.contains(detail), "{check:?}");
        assert_eq!(
            check.fix.is_some(),
            outcome != CheckOutcome::Ok,
            "{check:?}"
        );
    }

    // ---- runtime ----

    #[test]
    fn runtime_passes_when_info_succeeds() {
        let stub = Stub::new(INFO);
        let check = FakeHost::new().probe(0).check_runtime(&stub.docker());
        assert_check(&check, CheckOutcome::Ok, "Docker 29.9.3");
    }

    #[test]
    fn runtime_fails_when_the_program_is_missing() {
        let docker = Docker::with_program(Runner::default(), "verone-no-such-runtime", vec![]);
        let check = FakeHost::new().probe(0).check_runtime(&docker);
        assert_check(&check, CheckOutcome::Failed, "not installed");
        assert!(check.fix.unwrap().contains("Install Docker"));
    }

    #[test]
    fn runtime_fails_when_the_service_is_unreachable() {
        let stub = Stub::new("info*) echo 'permission denied on docker.sock' >&2; exit 1 ;;");
        let check = FakeHost::new().probe(0).check_runtime(&stub.docker());
        assert_check(&check, CheckOutcome::Failed, "cannot be used");
        assert!(check.fix.unwrap().contains("docker group"));
    }

    // ---- binder ----

    #[test]
    fn binder_passes_when_listed() {
        let host = FakeHost::new();
        host.write("proc/filesystems", "nodev\tsysfs\nnodev\tbinder\n\text4\n");
        assert_check(&host.probe(0).check_binder(), CheckOutcome::Ok, "available");
    }

    #[test]
    fn binder_fails_when_absent_or_unreadable() {
        let host = FakeHost::new();
        assert_check(
            &host.probe(0).check_binder(),
            CheckOutcome::Failed,
            "does not offer",
        );

        host.write(
            "proc/filesystems",
            "nodev\tsysfs\nnodev\tbinderfs2\n\text4\n",
        );
        let check = host.probe(0).check_binder();
        assert_check(&check, CheckOutcome::Failed, "does not offer");
        let fix = check.fix.unwrap();
        assert!(fix.contains("modprobe binder_linux"));
        assert!(fix.contains("linux-modules-extra"));
    }

    // ---- kernel modules ----

    #[test]
    fn kernel_modules_pass_when_the_folder_matches() {
        let host = FakeHost::new();
        host.write("proc/sys/kernel/osrelease", "7.3.4-arch1-1\n")
            .dir("lib/modules/7.3.4-arch1-1");
        assert_check(
            &host.probe(0).check_kernel_modules(),
            CheckOutcome::Ok,
            "7.3.4-arch1-1",
        );
    }

    #[test]
    fn kernel_modules_fail_after_an_update_without_restart() {
        let host = FakeHost::new();
        host.write("proc/sys/kernel/osrelease", "7.2.14-arch1-1\n")
            .dir("lib/modules/7.3.4-arch1-1");
        let check = host.probe(0).check_kernel_modules();
        assert_check(&check, CheckOutcome::Failed, "7.2.14-arch1-1");
        assert!(check.fix.unwrap().contains("Restart the computer"));
    }

    #[test]
    fn kernel_modules_warn_when_the_version_is_unreadable() {
        let check = FakeHost::new().probe(0).check_kernel_modules();
        assert_check(&check, CheckOutcome::Warning, "could not read");
    }

    // ---- space ----

    #[test]
    fn image_space_passes_when_the_image_is_present() {
        let stub = Stub::new(INFO);
        let check = FakeHost::new()
            .probe(0)
            .check_image_space(&stub.docker(), true);
        assert_check(&check, CheckOutcome::Ok, "already downloaded");
        assert!(stub.calls().is_empty());
    }

    #[test]
    fn image_space_passes_with_room() {
        let stub = Stub::new(INFO);
        let check = FakeHost::new()
            .probe(5 * GIB)
            .check_image_space(&stub.docker(), false);
        assert_check(&check, CheckOutcome::Ok, "5.0 GB free");
    }

    #[test]
    fn image_space_fails_when_short_and_says_how_much_to_free() {
        let stub = Stub::new(INFO);
        let check = FakeHost::new()
            .probe(GIB + GIB / 2)
            .check_image_space(&stub.docker(), false);
        assert_check(&check, CheckOutcome::Failed, "1.5 GB free");
        assert!(check.detail.contains("/var/lib/docker"));
        assert!(check.fix.unwrap().contains("2.5 GB"));
    }

    #[test]
    fn image_space_also_checks_containerd_when_it_stores_images() {
        let stub = Stub::new(INFO);
        let host = FakeHost::new();
        host.dir("var/lib/containerd");
        let probe = host.probe_with(|path| {
            if path.ends_with("containerd") {
                Ok(GIB)
            } else {
                Ok(50 * GIB)
            }
        });
        let check = probe.check_image_space(&stub.docker(), false);
        assert_check(
            &check,
            CheckOutcome::Failed,
            "1.0 GB free at /var/lib/containerd",
        );
    }

    #[test]
    fn image_space_warns_when_the_runtime_cannot_be_asked() {
        let stub = Stub::new("info*) exit 1 ;;");
        let check = FakeHost::new()
            .probe(0)
            .check_image_space(&stub.docker(), false);
        assert_check(&check, CheckOutcome::Warning, "could not ask");
    }

    #[test]
    fn data_space_passes_and_fails_at_the_limit() {
        let host = FakeHost::new();
        let home = host.root().join("not/yet/created");
        assert_check(
            &host.probe(2 * GIB).check_data_space(&home),
            CheckOutcome::Ok,
            "2.0 GB free",
        );
        assert_check(
            &host.probe(2 * GIB - 1).check_data_space(&home),
            CheckOutcome::Failed,
            "2.0 GB needed",
        );
    }

    #[test]
    fn space_that_cannot_be_measured_is_a_warning() {
        let host = FakeHost::new();
        let probe = host.probe_with(|_| Err(Error::failed("df went missing")));
        let check = probe.check_data_space(host.root());
        assert_check(&check, CheckOutcome::Warning, "df went missing");
    }

    #[test]
    fn free_space_is_asked_of_the_nearest_existing_parent() {
        let host = FakeHost::new();
        let root = host.root().to_path_buf();
        let expected = root.clone();
        let probe = host.probe_with(move |path| {
            assert_eq!(path, expected);
            Ok(7)
        });
        assert_eq!(probe.free_space(&root.join("a/b/c")).unwrap(), 7);
    }

    // ---- memory ----

    #[test]
    fn memory_passes_with_enough_and_warns_when_low() {
        let host = FakeHost::new();
        host.write(
            "proc/meminfo",
            "MemTotal: 8000000 kB\nMemAvailable:    3145728 kB\n",
        );
        assert_check(
            &host.probe(0).check_memory(),
            CheckOutcome::Ok,
            "3.0 GB available",
        );

        host.write(
            "proc/meminfo",
            "MemTotal: 8000000 kB\nMemAvailable: 1048576 kB\n",
        );
        let check = host.probe(0).check_memory();
        assert_check(&check, CheckOutcome::Warning, "1.0 GB available");
        assert!(check.fix.unwrap().contains("Close some applications"));
    }

    #[test]
    fn memory_that_cannot_be_read_does_not_fail() {
        assert_check(
            &FakeHost::new().probe(0).check_memory(),
            CheckOutcome::Ok,
            "could not be measured",
        );
    }

    // ---- graphics ----

    #[test]
    fn graphics_reports_the_node_auto_will_use() {
        let host = FakeHost::new();
        host.render_node("renderD129", "nvidia")
            .render_node("renderD128", "i915")
            .dir("sys/class/drm/card0");
        let probe = host.probe(0);

        assert_eq!(
            probe.render_nodes(),
            [
                RenderNode {
                    device: "/dev/dri/renderD128".to_string(),
                    driver: "i915".to_string()
                },
                RenderNode {
                    device: "/dev/dri/renderD129".to_string(),
                    driver: "nvidia".to_string()
                },
            ]
        );
        assert_eq!(
            probe.usable_render_node().unwrap().device,
            "/dev/dri/renderD128"
        );

        let check = probe.check_graphics();
        assert_check(
            &check,
            CheckOutcome::Ok,
            "use the GPU at /dev/dri/renderD128",
        );
        assert!(check.detail.contains("/dev/dri/renderD129 (nvidia)"));
    }

    #[test]
    fn graphics_falls_back_when_only_the_unusable_driver_exists() {
        let host = FakeHost::new();
        host.render_node("renderD128", "nvidia");
        let probe = host.probe(0);
        assert_eq!(probe.usable_render_node(), None);
        assert_check(
            &probe.check_graphics(),
            CheckOutcome::Ok,
            "software rendering",
        );
    }

    #[test]
    fn graphics_without_any_gpu_never_fails() {
        let probe = FakeHost::new().probe(0);
        assert!(probe.render_nodes().is_empty());
        assert_check(&probe.check_graphics(), CheckOutcome::Ok, "no GPU found");
    }

    // ---- programs ----

    #[test]
    fn a_helper_program_is_found_on_the_search_path_or_warned_about() {
        let host = FakeHost::new();
        host.write("bin/adb", "");
        let probe = host.probe(0);
        assert_check(
            &probe.check_program("adb", "verone adb", "android-tools"),
            CheckOutcome::Ok,
            "installed",
        );
        let check = probe.check_program("scrcpy", "verone open", "scrcpy");
        assert_check(&check, CheckOutcome::Warning, "needed for verone open");
        assert!(check.fix.unwrap().contains("scrcpy"));
    }

    // ---- everything ----

    #[test]
    fn all_checks_pass_on_a_healthy_system() {
        let stub = Stub::new(INFO);
        let host = FakeHost::new();
        host.write("proc/filesystems", "nodev\tbinder\n")
            .write("proc/sys/kernel/osrelease", "7.3.4-arch1-1\n")
            .dir("lib/modules/7.3.4-arch1-1")
            .write("proc/meminfo", "MemAvailable: 4194304 kB\n")
            .write("bin/adb", "")
            .write("bin/scrcpy", "")
            .render_node("renderD128", "i915");
        let checks =
            host.probe(20 * GIB)
                .check_all(&stub.docker(), false, &host.root().join("data"));

        assert_eq!(checks.len(), 9);
        for check in &checks {
            assert_eq!(check.outcome, CheckOutcome::Ok, "{check:?}");
        }
        assert!(!verone_core::engine::any_failed(&checks));
    }

    // ---- helpers ----

    #[test]
    fn df_output_is_parsed_to_bytes() {
        let text = "Filesystem     1024-blocks     Used Available Capacity Mounted on\n/dev/nvme0n1p2    32845584 26012345   4296704      86% /\n";
        assert_eq!(parse_df(text).unwrap(), 4_296_704 * 1024);
        assert!(parse_df("garbage").is_err());
        assert!(parse_df("").is_err());
    }

    #[test]
    fn the_real_df_can_be_read() {
        let free = free_space_from_df(Runner::default(), Path::new("/")).unwrap();
        assert!(free > 0);
    }

    #[test]
    fn sizes_are_shown_in_gigabytes() {
        assert_eq!(gib(0), "0.0 GB");
        assert_eq!(gib(GIB + GIB / 2), "1.5 GB");
        assert_eq!(gib(20 * GIB), "20.0 GB");
    }
}
