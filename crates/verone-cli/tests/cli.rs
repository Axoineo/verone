//! Tests that run the built `verone` binary.

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn version_flag_prints_name_and_version() {
    Command::cargo_bin("verone")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::eq("verone 0.1.0\n"));
}

#[test]
fn unknown_argument_is_a_usage_error() {
    Command::cargo_bin("verone")
        .unwrap()
        .arg("--no-such-option")
        .assert()
        .code(2);
}

#[test]
fn unknown_command_is_a_usage_error() {
    Command::cargo_bin("verone")
        .unwrap()
        .arg("nonsense")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("nonsense"));
}

#[test]
fn no_arguments_shows_help_and_is_a_usage_error() {
    Command::cargo_bin("verone")
        .unwrap()
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Usage:"));
}

#[test]
fn doctor_fails_with_exit_code_3_and_names_the_runtime_when_it_is_missing() {
    let home = tempfile::tempdir().unwrap();
    Command::cargo_bin("verone")
        .unwrap()
        .arg("doctor")
        .env("PATH", "/nonexistent")
        .env("VERONE_HOME", home.path())
        .assert()
        .code(3)
        .stdout(predicate::str::contains("FAILED   Container runtime"))
        .stdout(predicate::str::contains("Install Docker"))
        .stderr(predicate::str::contains("phones cannot run yet"));
}

#[test]
fn doctor_json_is_parseable_even_when_checks_fail() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::cargo_bin("verone")
        .unwrap()
        .args(["doctor", "--json"])
        .env("PATH", "/nonexistent")
        .env("VERONE_HOME", home.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let checks = parsed.as_array().unwrap();
    assert_eq!(checks.len(), 9);
    assert_eq!(checks[0]["name"], "Container runtime");
    assert_eq!(checks[0]["outcome"], "failed");
}

#[test]
fn an_unsupported_android_version_is_not_found() {
    let home = tempfile::tempdir().unwrap();
    Command::cargo_bin("verone")
        .unwrap()
        .args(["image", "pull", "--android", "9"])
        .env("VERONE_HOME", home.path())
        .assert()
        .code(4)
        .stderr(predicate::str::contains("no image for Android 9"))
        .stderr(predicate::str::contains("Supported versions: 14"));
}

#[test]
fn a_relative_verone_home_is_a_usage_error() {
    Command::cargo_bin("verone")
        .unwrap()
        .arg("doctor")
        .env("VERONE_HOME", "relative/path")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("VERONE_HOME"));
}

#[test]
fn completion_scripts_are_generated_for_each_shell() {
    for (shell, marker) in [
        ("bash", "complete -F"),
        ("zsh", "#compdef verone"),
        ("fish", "complete -c verone"),
    ] {
        // No data home is needed, so even a broken one must not matter.
        Command::cargo_bin("verone")
            .unwrap()
            .args(["completions", shell])
            .env("VERONE_HOME", "relative/path")
            .assert()
            .success()
            .stdout(predicate::str::contains(marker))
            .stdout(predicate::str::contains("restart"));
    }
    Command::cargo_bin("verone")
        .unwrap()
        .args(["completions", "nonsense"])
        .assert()
        .code(2);
}
