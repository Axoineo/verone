//! Tests that boot real phones on the real container runtime.
//!
//! They run only when `VERONE_TEST_DOCKER=1` is set, because they need Docker, the
//! kernel's binder support and the Android image, and take a few minutes:
//!
//! ```text
//! VERONE_TEST_DOCKER=1 cargo test -p verone-engine-container --test integration
//! ```

// Test helpers outside `#[test]` functions may unwrap: a failure here is a failed test.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use verone_core::engine::{Engine, PhoneStatus, any_failed};
use verone_core::process::Runner;
use verone_core::profile::{GraphicsMode, GraphicsUsed, Profile, State};
use verone_core::storage::{PhoneDir, PhoneId, Storage};
use verone_engine_container::ContainerEngine;
use verone_engine_container::host::HostProbe;

/// Phones are booted one at a time; the reference laptop has 8 GB of memory.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn enabled() -> bool {
    let on = std::env::var_os("VERONE_TEST_DOCKER").is_some_and(|value| value == "1");
    if !on {
        eprintln!("skipped: set VERONE_TEST_DOCKER=1 to run");
    }
    on
}

/// A phone in its own data home under the build folder, removed afterwards.
struct TestPhone {
    engine: ContainerEngine,
    storage: Storage,
    profile: Profile,
    phone: PhoneDir,
}

impl TestPhone {
    fn new(name: &str) -> Self {
        let home = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("home-{name}"));
        let storage = Storage::new(&home);
        let engine = ContainerEngine::new(Runner::default(), &home);
        let id = PhoneId::new(name).unwrap();

        // Clear anything a previous, interrupted run left behind.
        engine.destroy(&id).unwrap();
        if let Ok(old) = storage.find_phone(&id) {
            engine.erase_data(&old).unwrap();
            storage.remove_phone(&id).unwrap();
        }

        let phone = storage.create_phone(&id).unwrap();
        let profile = Profile::new(id);
        profile.save(&phone).unwrap();
        Self {
            engine,
            storage,
            profile,
            phone,
        }
    }

    fn id(&self) -> &PhoneId {
        &self.profile.id
    }

    fn name(&self) -> String {
        ContainerEngine::container_name(self.id())
    }

    fn exec(&self, args: &[&str]) -> String {
        self.engine
            .docker()
            .exec(&self.name(), args, Duration::from_secs(20))
            .unwrap()
            .stdout
            .trim()
            .to_string()
    }

    fn wait_for(&self, wanted: &PhoneStatus, within: Duration) {
        let deadline = Instant::now() + within;
        loop {
            let status = self.engine.status(self.id()).unwrap();
            if &status == wanted {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "still {status:?} after {within:?}, wanted {wanted:?}"
            );
            thread::sleep(Duration::from_millis(500));
        }
    }
}

impl Drop for TestPhone {
    fn drop(&mut self) {
        let _ = self.engine.stop(self.id());
        let _ = self.engine.destroy(self.id());
        let _ = self.engine.erase_data(&self.phone);
        let _ = self.storage.remove_phone(self.id());
    }
}

#[test]
fn the_runtime_and_host_are_usable() {
    if !enabled() {
        return;
    }
    let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let engine = ContainerEngine::new(Runner::default(), env!("CARGO_TARGET_TMPDIR"));

    let version = engine.docker().server_version().unwrap();
    assert!(!version.is_empty());
    eprintln!("runtime version: {version}");

    let checks = engine.check_host().unwrap();
    for check in &checks {
        eprintln!("{:?} {}: {}", check.outcome, check.name, check.detail);
    }
    assert!(!any_failed(&checks), "{checks:?}");

    // The image is already present on the reference machine, so this is a no-op.
    engine.prepare_image("14").unwrap();
    let statuses = engine.image_statuses().unwrap();
    assert!(statuses.iter().all(|image| image.present), "{statuses:?}");
}

#[test]
fn a_phone_boots_restarts_stops_and_keeps_its_data() {
    if !enabled() {
        return;
    }
    let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let mut t = TestPhone::new("it-life");
    let id = t.id().clone();

    assert_eq!(t.engine.status(&id).unwrap(), PhoneStatus::NotCreated);

    // Boot.
    let report = t.engine.start(&t.profile, &t.phone).unwrap();
    eprintln!("first boot: {:?} in {:?}", report.graphics, report.duration);
    assert!(!report.already_running);
    assert!(report.duration < Duration::from_secs(60), "{report:?}");
    assert_eq!(t.engine.status(&id).unwrap(), PhoneStatus::Running);
    assert_eq!(t.exec(&["getprop", "ro.build.version.release"]), "14");

    // Starting again does nothing.
    assert!(
        t.engine
            .start(&t.profile, &t.phone)
            .unwrap()
            .already_running
    );

    // Leave a marker in the phone's data.
    t.exec(&["touch", "/data/local/tmp/verone-marker"]);

    // A restart requested inside Android comes back by itself.
    let before = t
        .engine
        .docker()
        .inspect(&t.name())
        .unwrap()
        .unwrap()
        .started_at;
    t.exec(&["reboot"]);
    let restarted = Instant::now();
    let deadline = restarted + Duration::from_secs(60);
    loop {
        let container = t.engine.docker().inspect(&t.name()).unwrap().unwrap();
        if container.started_at != before && t.engine.status(&id).unwrap() == PhoneStatus::Running {
            break;
        }
        assert!(Instant::now() < deadline, "the phone did not come back");
        thread::sleep(Duration::from_millis(500));
    }
    eprintln!(
        "returned after in-Android restart in {:?}",
        restarted.elapsed()
    );

    // Clean stop: Android powers off from inside and exits with code 130.
    let stopping = Instant::now();
    t.engine.stop(&id).unwrap();
    let stop_time = stopping.elapsed();
    eprintln!("stopped in {stop_time:?}");
    assert!(stop_time < Duration::from_secs(20), "{stop_time:?}");
    let stopped = t.engine.docker().inspect(&t.name()).unwrap().unwrap();
    assert_eq!(stopped.status, "exited");
    assert_eq!(stopped.exit_code, 130);
    assert_eq!(stopped.restart_policy, "no");
    assert_eq!(t.engine.status(&id).unwrap(), PhoneStatus::Stopped);
    t.engine.stop(&id).unwrap();

    // A changed display size recreates the container and keeps the data.
    t.profile.display.width = 1080;
    t.profile.display.height = 1920;
    t.profile.save(&t.phone).unwrap();
    let report = t.engine.start(&t.profile, &t.phone).unwrap();
    assert!(!report.already_running);
    assert_eq!(t.exec(&["wm", "size"]), "Physical size: 1080x1920");
    assert_eq!(
        t.exec(&["ls", "/data/local/tmp/verone-marker"]),
        "/data/local/tmp/verone-marker"
    );
    let recreated = t.engine.docker().inspect(&t.name()).unwrap().unwrap();
    assert!(recreated.labels["io.verone.spec"].contains("redroid_width=1080"));
    assert_eq!(recreated.restart_policy, "unless-stopped");

    // Stop and remove.
    t.engine.stop(&id).unwrap();
    t.engine.destroy(&id).unwrap();
    assert_eq!(t.engine.status(&id).unwrap(), PhoneStatus::NotCreated);

    // Erasing the data leaves an empty folder the user owns.
    t.engine.erase_data(&t.phone).unwrap();
    assert_eq!(std::fs::read_dir(t.phone.data_dir()).unwrap().count(), 0);
}

#[test]
fn a_gpu_boot_that_times_out_falls_back_to_software_once() {
    if !enabled() {
        return;
    }
    let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let probe = HostProbe::system(Runner::default());
    if probe.usable_render_node().is_none() {
        eprintln!("skipped: this machine has no usable GPU, so auto never tries GPU mode");
        return;
    }

    let mut t = TestPhone::new("it-fallback");
    assert_eq!(t.profile.graphics.mode, GraphicsMode::Auto);
    // Give the GPU attempt no time at all, which forces the fallback path.
    t.engine = t
        .engine
        .clone()
        .with_boot_timeouts(Duration::from_secs(120), Duration::ZERO);

    let report = t.engine.start(&t.profile, &t.phone).unwrap();
    eprintln!("fallback boot in {:?}", report.duration);
    assert_eq!(report.graphics, GraphicsUsed::Software);
    assert!(report.fell_back_to_software);
    assert_eq!(t.engine.status(t.id()).unwrap(), PhoneStatus::Running);

    let container = t.engine.docker().inspect(&t.name()).unwrap().unwrap();
    assert!(container.labels["io.verone.spec"].contains("redroid_gpu_mode=guest"));

    let state = State::load(&t.phone).unwrap();
    let last = state.last_boot.unwrap();
    assert_eq!(last.graphics, GraphicsUsed::Software);
    assert!(last.fell_back_to_software);

    // The next start goes straight to software, without trying the GPU again.
    t.engine.stop(t.id()).unwrap();
    let again = t.engine.start(&t.profile, &t.phone).unwrap();
    assert_eq!(again.graphics, GraphicsUsed::Software);
    assert!(again.duration < Duration::from_secs(60));
}

fn android_release(t: &TestPhone) -> String {
    let endpoint = t.engine.control_endpoint(t.id()).unwrap();
    t.engine
        .adb()
        .run(
            &endpoint,
            &["shell", "getprop", "ro.build.version.release"],
            Duration::from_secs(20),
        )
        .unwrap()
        .stdout
        .trim()
        .to_string()
}

#[test]
fn adb_reaches_the_phone_and_survives_a_restart() {
    if !enabled() {
        return;
    }
    let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let t = TestPhone::new("it-adb");
    let id = t.id().clone();

    t.engine.start(&t.profile, &t.phone).unwrap();
    assert_eq!(android_release(&t), "14");

    // No port is published unless the user asks for one.
    assert_eq!(
        t.engine
            .docker()
            .published_port(&t.name(), "5555/tcp")
            .unwrap(),
        None
    );

    // After a stop and start, adb works again with no manual reconnection.
    t.engine.stop(&id).unwrap();
    t.engine.start(&t.profile, &t.phone).unwrap();
    assert_eq!(android_release(&t), "14");

    // And after a restart requested inside Android.
    t.exec(&["reboot"]);
    thread::sleep(Duration::from_secs(3));
    t.wait_for(&PhoneStatus::Running, Duration::from_secs(60));
    assert_eq!(android_release(&t), "14");

    // A stopped phone has no endpoint.
    t.engine.stop(&id).unwrap();
    assert!(t.engine.control_endpoint(&id).is_err());
}

/// Needs the host to be able to publish container ports. On the reference laptop
/// that requires the `xt_tcpudp` kernel module (`sudo modprobe xt_tcpudp`).
#[test]
fn an_optional_adb_port_is_published_on_the_loopback_address_only() {
    if !enabled() {
        return;
    }
    let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let mut t = TestPhone::new("it-port");
    t.profile.adb.local_port = 5599;
    t.profile.save(&t.phone).unwrap();

    t.engine.start(&t.profile, &t.phone).unwrap();
    assert_eq!(
        t.engine
            .docker()
            .published_port(&t.name(), "5555/tcp")
            .unwrap()
            .as_deref(),
        Some("127.0.0.1:5599")
    );
    assert_eq!(android_release(&t), "14");
}

/// Decision D-017: on Android 12 the power watcher carries out restart and power-off
/// requests made inside Android, which Android 12 itself ignores.
#[test]
fn android_12_restart_and_power_off_work_through_the_power_watcher() {
    if !enabled() {
        return;
    }
    let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let mut t = TestPhone::new("it-android12");
    t.profile.android = "12".to_string();
    t.profile.save(&t.phone).unwrap();
    let id = t.id().clone();
    let docker = t.engine.docker().clone();
    let phones_dir = t.storage.phones_dir().canonicalize().unwrap();
    let watcher = verone_engine_container::power::watcher_name(&phones_dir);

    t.engine.start(&t.profile, &t.phone).unwrap();
    assert_eq!(android_release(&t), "12");
    let running = |name: &str| {
        docker
            .inspect(name)
            .unwrap()
            .is_some_and(|container| container.status == "running")
    };
    assert!(
        running(&watcher),
        "the watcher runs beside an Android 12 phone"
    );

    // Shared storage becomes writable a few seconds after boot (A-22).
    let deadline = Instant::now() + Duration::from_secs(30);
    while t.exec(&["sh", "-c", "echo kept > /sdcard/marker && echo ok"]) != "ok" {
        assert!(
            Instant::now() < deadline,
            "shared storage never became writable"
        );
        thread::sleep(Duration::from_secs(1));
    }

    // A restart requested inside Android: the phone ends and Docker brings it back.
    let first_start = docker.inspect(&t.name()).unwrap().unwrap().started_at;
    let requested = Instant::now();
    // Android's reboot command waits forever after asking, so its result is ignored.
    let _ = docker.exec(&t.name(), &["reboot"], Duration::from_secs(20));
    let deadline = requested + Duration::from_secs(30);
    loop {
        let restarted = docker
            .inspect(&t.name())
            .unwrap()
            .is_some_and(|container| container.started_at != first_start);
        if restarted && t.engine.status(&id).unwrap() == PhoneStatus::Running {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the phone was not back within 30 s of the restart request"
        );
        thread::sleep(Duration::from_millis(500));
    }
    eprintln!("back after a restart request in {:?}", requested.elapsed());
    assert_eq!(t.exec(&["cat", "/sdcard/marker"]), "kept");

    // verone stop: Android powers off through the watcher, quickly and with the
    // power-off exit code, not the runtime's kill.
    let stopping = Instant::now();
    t.engine.stop(&id).unwrap();
    let took = stopping.elapsed();
    eprintln!("stopped in {took:?}");
    assert!(took < Duration::from_secs(5), "stop took {took:?}");
    assert_eq!(docker.inspect(&t.name()).unwrap().unwrap().exit_code, 130);

    // Deleting the last Android 12 phone of this data folder removes its watcher.
    t.engine.destroy(&id).unwrap();
    assert!(docker.inspect(&watcher).unwrap().is_none());
}
