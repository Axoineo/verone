//! An engine that runs nothing, for tests.
//!
//! It keeps phone states in memory, records every call, and can be told to fail
//! the next call. Front ends use it to test their own logic without a container
//! runtime.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use super::{BootReport, Capabilities, ControlEndpoint, Engine, HostCheck, PhoneStatus};
use crate::error::{Error, Result};
use crate::profile::{EngineKind, GraphicsUsed, Profile};
use crate::storage::{PhoneDir, PhoneId};

/// The only Android version the fake engine has an image for.
pub const FAKE_ANDROID: &str = "14";

#[derive(Debug, Default)]
struct Inner {
    calls: Vec<String>,
    phones: BTreeMap<PhoneId, PhoneStatus>,
    images: BTreeSet<String>,
    fail_next: Option<Error>,
    host_checks: Vec<HostCheck>,
}

/// An in-memory engine for tests.
#[derive(Debug, Default)]
pub struct FakeEngine {
    inner: Mutex<Inner>,
}

impl FakeEngine {
    /// A fake engine with no phones, no images and one passing host check.
    pub fn new() -> Self {
        let engine = Self::default();
        engine.lock().host_checks = vec![HostCheck::ok("Fake runtime", "always available")];
        engine
    }

    /// The calls made so far, oldest first, as `operation` or `operation id`.
    pub fn calls(&self) -> Vec<String> {
        self.lock().calls.clone()
    }

    /// Makes the next engine call fail with `error`. Later calls work again.
    pub fn fail_next(&self, error: Error) {
        self.lock().fail_next = Some(error);
    }

    /// Replaces the host checks that `check_host` reports.
    pub fn set_host_checks(&self, checks: Vec<HostCheck>) {
        self.lock().host_checks = checks;
    }

    /// Puts a phone into a state directly, as if something outside Verone had
    /// changed it.
    pub fn set_status(&self, id: &PhoneId, status: PhoneStatus) {
        self.lock().phones.insert(id.clone(), status);
    }

    /// True if `prepare_image` has been called for this Android version.
    pub fn has_image(&self, android: &str) -> bool {
        self.lock().images.contains(android)
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Records a call and returns the state, or the error queued by `fail_next`.
    fn enter(&self, call: String) -> Result<MutexGuard<'_, Inner>> {
        let mut inner = self.lock();
        inner.calls.push(call);
        match inner.fail_next.take() {
            Some(error) => Err(error),
            None => Ok(inner),
        }
    }
}

fn not_created(id: &PhoneId) -> Error {
    Error::not_found(format!(
        "the phone '{id}' has not been created in the engine"
    ))
}

impl Engine for FakeEngine {
    fn kind(&self) -> EngineKind {
        EngineKind::Container
    }

    fn check_host(&self) -> Result<Vec<HostCheck>> {
        Ok(self.enter("check_host".to_string())?.host_checks.clone())
    }

    fn prepare_image(&self, android: &str) -> Result<()> {
        let mut inner = self.enter(format!("prepare_image {android}"))?;
        if android != FAKE_ANDROID {
            return Err(Error::not_found(format!(
                "there is no image for Android {android}"
            )));
        }
        inner.images.insert(android.to_string());
        Ok(())
    }

    fn create(&self, profile: &Profile, _phone: &PhoneDir) -> Result<()> {
        let mut inner = self.enter(format!("create {}", profile.id))?;
        inner
            .phones
            .entry(profile.id.clone())
            .or_insert(PhoneStatus::Stopped);
        Ok(())
    }

    fn destroy(&self, id: &PhoneId) -> Result<()> {
        let mut inner = self.enter(format!("destroy {id}"))?;
        inner.phones.remove(id);
        Ok(())
    }

    fn start(&self, profile: &Profile, _phone: &PhoneDir) -> Result<BootReport> {
        let mut inner = self.enter(format!("start {}", profile.id))?;
        let Some(status) = inner.phones.get_mut(&profile.id) else {
            return Err(not_created(&profile.id));
        };
        let already_running = *status == PhoneStatus::Running;
        *status = PhoneStatus::Running;
        Ok(BootReport {
            duration: if already_running {
                Duration::ZERO
            } else {
                Duration::from_secs(1)
            },
            graphics: GraphicsUsed::Software,
            fell_back_to_software: false,
            already_running,
        })
    }

    fn stop(&self, id: &PhoneId) -> Result<()> {
        let mut inner = self.enter(format!("stop {id}"))?;
        match inner.phones.get_mut(id) {
            Some(status) => {
                *status = PhoneStatus::Stopped;
                Ok(())
            }
            None => Err(not_created(id)),
        }
    }

    fn status(&self, id: &PhoneId) -> Result<PhoneStatus> {
        let inner = self.enter(format!("status {id}"))?;
        Ok(inner
            .phones
            .get(id)
            .cloned()
            .unwrap_or(PhoneStatus::NotCreated))
    }

    fn control_endpoint(&self, id: &PhoneId) -> Result<ControlEndpoint> {
        let inner = self.enter(format!("control_endpoint {id}"))?;
        match inner.phones.get(id) {
            Some(PhoneStatus::Running) => Ok(ControlEndpoint {
                host: "127.0.0.1".to_string(),
                port: 5555,
            }),
            Some(_) => Err(Error::failed(format!("the phone '{id}' is not running"))
                .with_hint(format!("Start it with: verone start {id}"))),
            None => Err(not_created(id)),
        }
    }

    fn capabilities(&self) -> Result<Capabilities> {
        let _inner = self.enter("capabilities".to_string())?;
        Ok(Capabilities {
            gpu_rendering: false,
            root_access: true,
            own_kernel: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{CheckOutcome, any_failed};
    use crate::error::ErrorKind;
    use crate::storage::Storage;

    struct Fixture {
        _temp: tempfile::TempDir,
        engine: FakeEngine,
        profile: Profile,
        phone: PhoneDir,
    }

    fn fixture() -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let id = PhoneId::new("work").unwrap();
        let phone = Storage::new(temp.path()).create_phone(&id).unwrap();
        Fixture {
            _temp: temp,
            engine: FakeEngine::new(),
            profile: Profile::new(id),
            phone,
        }
    }

    /// The engine is used through the trait, as front ends will use it.
    fn as_engine(engine: &FakeEngine) -> &dyn Engine {
        engine
    }

    #[test]
    fn a_phone_goes_through_create_start_stop_destroy() {
        let f = fixture();
        let engine = as_engine(&f.engine);
        let id = &f.profile.id;

        assert_eq!(engine.status(id).unwrap(), PhoneStatus::NotCreated);

        engine.create(&f.profile, &f.phone).unwrap();
        assert_eq!(engine.status(id).unwrap(), PhoneStatus::Stopped);

        let report = engine.start(&f.profile, &f.phone).unwrap();
        assert!(!report.already_running);
        assert!(report.duration > Duration::ZERO);
        assert_eq!(engine.status(id).unwrap(), PhoneStatus::Running);

        engine.stop(id).unwrap();
        assert_eq!(engine.status(id).unwrap(), PhoneStatus::Stopped);

        engine.destroy(id).unwrap();
        assert_eq!(engine.status(id).unwrap(), PhoneStatus::NotCreated);
    }

    #[test]
    fn every_call_is_recorded_in_order() {
        let f = fixture();
        let engine = as_engine(&f.engine);
        engine.create(&f.profile, &f.phone).unwrap();
        engine.start(&f.profile, &f.phone).unwrap();
        engine.stop(&f.profile.id).unwrap();
        engine.destroy(&f.profile.id).unwrap();
        assert_eq!(
            f.engine.calls(),
            ["create work", "start work", "stop work", "destroy work"]
        );
    }

    #[test]
    fn repeated_operations_are_not_errors() {
        let f = fixture();
        let engine = as_engine(&f.engine);
        let id = &f.profile.id;

        engine.create(&f.profile, &f.phone).unwrap();
        engine.start(&f.profile, &f.phone).unwrap();
        engine.create(&f.profile, &f.phone).unwrap();
        assert_eq!(engine.status(id).unwrap(), PhoneStatus::Running);

        let again = engine.start(&f.profile, &f.phone).unwrap();
        assert!(again.already_running);
        assert_eq!(again.duration, Duration::ZERO);

        engine.stop(id).unwrap();
        engine.stop(id).unwrap();
        engine.destroy(id).unwrap();
        engine.destroy(id).unwrap();
    }

    #[test]
    fn starting_or_stopping_a_phone_that_was_never_created_is_not_found() {
        let f = fixture();
        let engine = as_engine(&f.engine);
        assert_eq!(
            engine.start(&f.profile, &f.phone).unwrap_err().kind(),
            ErrorKind::NotFound
        );
        assert_eq!(
            engine.stop(&f.profile.id).unwrap_err().kind(),
            ErrorKind::NotFound
        );
    }

    #[test]
    fn the_endpoint_exists_only_while_running() {
        let f = fixture();
        let engine = as_engine(&f.engine);
        let id = &f.profile.id;

        assert_eq!(
            engine.control_endpoint(id).unwrap_err().kind(),
            ErrorKind::NotFound
        );
        engine.create(&f.profile, &f.phone).unwrap();
        let stopped = engine.control_endpoint(id).unwrap_err();
        assert_eq!(stopped.kind(), ErrorKind::Failed);
        assert!(stopped.hint().unwrap().contains("verone start work"));

        engine.start(&f.profile, &f.phone).unwrap();
        assert_eq!(
            engine.control_endpoint(id).unwrap().to_string(),
            "127.0.0.1:5555"
        );
    }

    #[test]
    fn only_the_known_android_version_has_an_image() {
        let f = fixture();
        let engine = as_engine(&f.engine);
        engine.prepare_image("14").unwrap();
        assert!(f.engine.has_image("14"));

        let error = engine.prepare_image("99").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
        assert_eq!(error.exit_code(), 4);
        assert!(!f.engine.has_image("99"));
    }

    #[test]
    fn a_queued_failure_hits_the_next_call_only() {
        let f = fixture();
        let engine = as_engine(&f.engine);
        engine.create(&f.profile, &f.phone).unwrap();

        f.engine.fail_next(Error::failed("the runtime went away"));
        let error = engine.start(&f.profile, &f.phone).unwrap_err();
        assert_eq!(error.message(), "the runtime went away");
        assert_eq!(engine.status(&f.profile.id).unwrap(), PhoneStatus::Stopped);

        engine.start(&f.profile, &f.phone).unwrap();
        assert_eq!(engine.status(&f.profile.id).unwrap(), PhoneStatus::Running);
    }

    #[test]
    fn host_checks_and_outside_changes_can_be_simulated() {
        let f = fixture();
        let engine = as_engine(&f.engine);

        let checks = engine.check_host().unwrap();
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].outcome, CheckOutcome::Ok);

        f.engine.set_host_checks(vec![HostCheck::failed(
            "Binder",
            "not available",
            "Run modprobe binder_linux.",
        )]);
        assert!(any_failed(&engine.check_host().unwrap()));

        f.engine.set_status(
            &f.profile.id,
            PhoneStatus::Failed {
                reason: "exit code 137".to_string(),
            },
        );
        assert!(matches!(
            engine.status(&f.profile.id).unwrap(),
            PhoneStatus::Failed { .. }
        ));
    }

    #[test]
    fn it_describes_itself() {
        let f = fixture();
        let engine = as_engine(&f.engine);
        assert_eq!(engine.kind(), EngineKind::Container);
        let capabilities = engine.capabilities().unwrap();
        assert!(capabilities.root_access);
        assert!(!capabilities.own_kernel);
        assert!(!capabilities.gpu_rendering);
    }
}
