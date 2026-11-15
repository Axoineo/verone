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
fn global_options_are_accepted() {
    Command::cargo_bin("verone")
        .unwrap()
        .args(["--json", "--verbose"])
        .assert()
        .success();
}
