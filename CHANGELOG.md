# Changelog

All notable changes to Verone are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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
