//! What each `verone` command does.
//!
//! Commands take the engine through the [`Engine`] trait where they can, so they
//! are tested with the fake engine and no container runtime.

use std::io::Write;

use serde::Serialize;
use verone_core::engine::{CheckOutcome, Engine, HostCheck, any_failed};
use verone_core::{Error, Result};
use verone_engine_container::ContainerEngine;
use verone_engine_container::engine::ImageStatus;

use crate::output::{self, Mode};

/// `verone doctor`: run the host checks and report them. Fails with the host-check
/// exit code if any check failed.
pub fn doctor<E: Engine, W: Write>(engine: &E, mode: Mode, out: &mut W) -> Result<()> {
    let checks = engine.check_host()?;
    output::emit(mode, out, &checks, |checks| format_checks(checks))?;

    let failed = checks
        .iter()
        .filter(|check| check.outcome == CheckOutcome::Failed)
        .count();
    if any_failed(&checks) {
        let noun = if failed == 1 { "check" } else { "checks" };
        return Err(
            Error::host_check(format!("{failed} {noun} failed; phones cannot run yet"))
                .with_hint("Apply the fixes shown above, then run verone doctor again."),
        );
    }
    Ok(())
}

fn format_checks(checks: &[HostCheck]) -> String {
    let mut text = String::new();
    for check in checks {
        let label = match check.outcome {
            CheckOutcome::Ok => "ok      ",
            CheckOutcome::Warning => "warning ",
            CheckOutcome::Failed => "FAILED  ",
        };
        text.push_str(&format!("{label} {}: {}\n", check.name, check.detail));
        if let Some(fix) = &check.fix {
            text.push_str(&format!("         fix: {fix}\n"));
        }
    }
    let warnings = checks
        .iter()
        .filter(|check| check.outcome == CheckOutcome::Warning)
        .count();
    if !any_failed(checks) {
        text.push_str(match warnings {
            0 => "\nThis computer is ready to run phones.\n",
            _ => "\nThis computer can run phones. The warnings above limit some features.\n",
        });
    }
    text
}

#[derive(Serialize)]
struct Pulled<'a> {
    android: &'a str,
    ready: bool,
}

/// `verone image pull`: make an Android image available.
pub fn image_pull<E: Engine, W: Write>(
    engine: &E,
    android: &str,
    mode: Mode,
    out: &mut W,
) -> Result<()> {
    engine.prepare_image(android)?;
    let result = Pulled {
        android,
        ready: true,
    };
    output::emit(mode, out, &result, |result| {
        format!("The Android {} image is ready.", result.android)
    })
}

/// `verone image list`: show the supported images.
pub fn image_list<W: Write>(engine: &ContainerEngine, mode: Mode, out: &mut W) -> Result<()> {
    let images = engine.image_statuses()?;
    output::emit(mode, out, &images, |images| format_images(images))
}

fn format_images(images: &[ImageStatus]) -> String {
    let mut text = String::new();
    for image in images {
        let state = if image.present {
            "downloaded"
        } else {
            "not downloaded"
        };
        text.push_str(&format!(
            "Android {}  {}  {state}\n",
            image.android, image.name
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use verone_core::ErrorKind;
    use verone_core::engine::fake::FakeEngine;

    fn run<F: FnOnce(&mut Vec<u8>) -> Result<()>>(command: F) -> (Result<()>, String) {
        let mut out = Vec::new();
        let result = command(&mut out);
        (result, String::from_utf8(out).unwrap())
    }

    #[test]
    fn doctor_passes_when_every_check_passes() {
        let engine = FakeEngine::new();
        let (result, text) = run(|out| doctor(&engine, Mode::Human, out));
        result.unwrap();
        assert!(
            text.contains("ok       Fake runtime: always available"),
            "{text}"
        );
        assert!(text.contains("ready to run phones"), "{text}");
    }

    #[test]
    fn doctor_shows_fixes_and_fails_with_the_host_check_code() {
        let engine = FakeEngine::new();
        engine.set_host_checks(vec![
            HostCheck::ok("Container runtime", "Docker 29"),
            HostCheck::warning("scrcpy", "not installed", "Install the package scrcpy."),
            HostCheck::failed(
                "Kernel binder support",
                "missing",
                "Run modprobe binder_linux.",
            ),
        ]);
        let (result, text) = run(|out| doctor(&engine, Mode::Human, out));
        let error = result.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::HostCheck);
        assert_eq!(error.exit_code(), 3);
        assert!(error.message().contains("1 check failed"), "{error}");
        assert!(
            text.contains("FAILED   Kernel binder support: missing"),
            "{text}"
        );
        assert!(
            text.contains("         fix: Run modprobe binder_linux."),
            "{text}"
        );
        assert!(text.contains("warning  scrcpy"), "{text}");
        assert!(!text.contains("ready to run"), "{text}");
    }

    #[test]
    fn doctor_with_only_warnings_passes_and_says_so() {
        let engine = FakeEngine::new();
        engine.set_host_checks(vec![HostCheck::warning(
            "adb",
            "not installed",
            "Install it.",
        )]);
        let (result, text) = run(|out| doctor(&engine, Mode::Human, out));
        result.unwrap();
        assert!(
            text.contains("warnings above limit some features"),
            "{text}"
        );
    }

    #[test]
    fn doctor_json_has_one_object_per_check() {
        let engine = FakeEngine::new();
        engine.set_host_checks(vec![
            HostCheck::ok("Memory", "4.0 GB available"),
            HostCheck::failed("Binder", "missing", "Load it."),
        ]);
        let (result, text) = run(|out| doctor(&engine, Mode::Json, out));
        assert!(result.is_err());
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        let checks = parsed.as_array().unwrap();
        assert_eq!(checks.len(), 2);
        assert_eq!(checks[0]["name"], "Memory");
        assert_eq!(checks[0]["outcome"], "ok");
        assert!(checks[0].get("fix").is_none());
        assert_eq!(checks[1]["outcome"], "failed");
        assert_eq!(checks[1]["fix"], "Load it.");
    }

    #[test]
    fn image_pull_reports_readiness_or_an_unsupported_version() {
        let engine = FakeEngine::new();
        let (result, text) = run(|out| image_pull(&engine, "14", Mode::Human, out));
        result.unwrap();
        assert_eq!(text, "The Android 14 image is ready.\n");
        assert!(engine.has_image("14"));

        let (result, _) = run(|out| image_pull(&engine, "99", Mode::Human, out));
        assert_eq!(result.unwrap_err().exit_code(), 4);
    }

    #[test]
    fn images_are_listed_with_their_download_state() {
        let images = [ImageStatus {
            android: "14".to_string(),
            name: "redroid/redroid:14.0.0-latest".to_string(),
            digest: "sha256:abc".to_string(),
            present: false,
        }];
        assert_eq!(
            format_images(&images),
            "Android 14  redroid/redroid:14.0.0-latest  not downloaded\n"
        );
    }
}
