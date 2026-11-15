# Changelog

All notable changes to Verone are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] - 2026-11-15

First release: a command-line tool that runs Android 14 phones on Linux with Docker.

### Added

- Rust workspace with three crates: `verone-core`, `verone-engine-container` and
  `verone-cli` (the `verone` binary).
- `verone --version`.
- Continuous integration running the format, lint and test checks on every pull request.
- Error type with a message, a "what to do" hint and one exit code per kind of failure
  (1 failed, 2 usage, 3 host check, 4 not found, 5 busy).
- Process runner for external programs: argument lists only, time limits, captured or
  pass-through output.
- Global options `--json` and `-v`/`--verbose`.
- Storage layout: the data home (`VERONE_HOME`, then `XDG_DATA_HOME/verone`, then
  `~/.local/share/verone`), one private folder per phone, and phone id rules.
- Per-phone lock so two commands cannot work on the same phone at once.
- Phone profile (`phone.toml`) and state (`state.toml`) files: reading, writing and
  validation, with files from a newer Verone refused and an upgrade hint given.
- Engine interface that every engine implements, and an in-memory fake engine for tests.
- Container runtime wrapper: typed calls to Docker for images and containers.
- Host checks: runtime, kernel binder, kernel modules, disk space, memory, graphics and
  helper programs, each with a fix when it does not pass.
- Container engine: Android 14 image pinned by digest, container creation from the phone
  profile, boot with a wait for Android to be ready, clean shutdown from inside Android,
  automatic return after an in-Android restart, and a one-time fallback from GPU to
  software rendering.
- adb helper that drops stale connections and reconnects before every use.
- `verone doctor`, `verone image pull` and `verone image list`.
- Phone commands: `create`, `list`, `info`, `start`, `stop`, `restart`, `open`, `adb`,
  `install`, `reset` and `delete`. `reset` and `delete` ask for confirmation unless
  `--yes` is given.
- End-to-end test script `tests/e2e/phase-1.sh` that runs every command on a real phone.
- `verone completions <shell>` for bash, zsh, fish, elvish and powershell.
- Arch package recipe in `packaging/arch`, and a release build optimised for size.
- User documentation: quick start in the README, a command reference, `SECURITY.md` and
  `CONTRIBUTING.md`.
