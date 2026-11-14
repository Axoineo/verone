# Phase 1 — Core and command line

Status: **approved by the owner** (2026-11-15). Work can start once the owner preparation
below is done.

Outcome: a `verone` command-line tool that creates, starts, stops, resets, lists and
opens Android phones on Linux, built on a core library that every later front end
reuses.

This file is the executable plan. Each task is one unit of work with a fixed scope and
a test that decides when it is done.

## Before starting

Read `../vision-and-scope.md`, `../decisions.md`, `../architecture.md` and the
evidence in `../assumptions.md`. Everything in the "Fixed design" section below is
either a recorded decision or was measured in Phase 0.

### Owner preparation

Run in a normal terminal.

| # | Action | Command |
|---|---|---|
| 0.1 | Install the Rust toolchain (not installed on 2026-11-15). | `sudo pacman -S --needed rust` |
| 0.2 | Confirm it works. | `cargo --version` |
| 0.3 | Keep at least 4 GB free on the root partition. | `df -h /` |

### Out of scope for Phase 1

Desktop window, encryption, Google apps, backups, snapshots, server mode, Windows,
Podman (unless test T-07 passes and the owner adds task P1-19), any background service.

## Rules for agents

1. One task per branch and pull request. Branch name: `p1/<task number>-<short-name>`,
   created from an up-to-date `main`.
2. Do the tasks in number order unless a task's "Depends on" says otherwise.
3. A task is done only when every command under "Done when" succeeds on the reference
   laptop. Paste the commands and their output into the pull request.
4. Dependencies: only the crates in the allow-list below. Anything else needs the
   owner's approval first.
5. No `unsafe`. No `unwrap()` or `expect()` outside tests. No command built as a shell
   string: every external program is run with an argument list through the process
   module from P1-02.
6. Every external command has a timeout and its output is captured.
7. Errors shown to the user say what went wrong and what to do next.
8. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test`
   pass before every commit.
9. If a task cannot be done as written, stop and tell the owner. Do not redesign.
10. Update `CHANGELOG.md` in every pull request that changes behaviour.

### Crate allow-list

| Crate | Used for | Where |
|---|---|---|
| `clap` (derive feature) | command-line parsing | `verone-cli` |
| `clap_complete` | shell completions | `verone-cli` |
| `serde`, `toml`, `serde_json` | profile files, runtime output, `--json` | all |
| `thiserror` | error types | libraries |
| `anyhow` | top-level error reporting | `verone-cli` only |
| `tempfile`, `assert_cmd`, `predicates` | tests | dev-dependencies only |

Add each with `cargo add` so the current version is taken, and commit `Cargo.lock`.
No async runtime: Phase 1 uses blocking process calls only.

---

## Fixed design

### Workspace

```
Cargo.toml                      workspace
rust-toolchain.toml             pins the stable version installed in step 0.1
crates/verone-core/             profiles, storage, host checks, engine trait, errors
crates/verone-engine-container/ redroid engine on a container runtime
crates/verone-cli/              the `verone` binary
tests/e2e/phase-1.sh            exit test
packaging/arch/PKGBUILD
```

### Storage

- Data home: `$VERONE_HOME` if set, otherwise `$XDG_DATA_HOME/verone`, otherwise
  `~/.local/share/verone`. Tests always set `VERONE_HOME` to a temporary folder.
- One folder per phone: `phones/<id>/` containing `phone.toml`, `state.toml`, `.lock`
  and `data/` (Android's data, owned by root).
- Phone id: lowercase letters, digits and hyphens, 1 to 31 characters, starting with a
  letter or digit. The id is chosen by the user and never changes.

`phone.toml`:

```toml
schema = 1
id = "work"
name = "Work phone"
engine = "container"
android = "14"
created_unix = 1794700000

[display]
width = 720
height = 1280
dpi = 320

[graphics]
mode = "auto"        # auto | software | gpu

[adb]
local_port = 0       # 0 = no port published
```

`state.toml` holds what Verone observed, never what the user chose: for example the
graphics mode that last booted successfully.

### Images

| `android` | Image | Digest (verified 2026-11-15) |
|---|---|---|
| `"14"` | `redroid/redroid:14.0.0-latest` | `sha256:a3e654caad74faed03cd5ead057a989e06f507778cda678e5d3b9772e0092f1c` |

- Phase 1 supports Android 14 on x86-64 only. The table lives in one Rust module so
  later phases add rows.
- Images are pulled and run by digest (`redroid/redroid@sha256:…`), never by tag.
- Helper image for file operations on root-owned data:
  `alpine@sha256:f59abe656b692d901e184be3445e084786602108599d2a51d229c0f5a48c71bf`.

### Container

- Name `verone-<id>`. Labels `io.verone.phone=<id>` and `io.verone.spec=<spec text>`,
  where the spec text is the exact list of options and start parameters used.
- Created with: `--privileged`, `--restart unless-stopped`, `-v <phone>/data:/data`,
  and `-p 127.0.0.1:<port>:5555` only when `adb.local_port` is not 0.
- Start parameters: `androidboot.redroid_width`, `_height`, `_dpi`,
  `androidboot.redroid_gpu_mode` and, for GPU mode, `androidboot.redroid_gpu_node`.
- If the stored spec label differs from the spec computed from `phone.toml`, the
  container is removed and created again. Data is untouched.

### Graphics mode

Verone decides; redroid's own `auto` is not used.

| `graphics.mode` | What Verone does |
|---|---|
| `software` | `redroid_gpu_mode=guest` |
| `gpu` | `redroid_gpu_mode=host` with the first usable render node; error if none |
| `auto` | `gpu` if a usable render node exists, otherwise `software` |

A render node is usable when its driver (the link `/sys/class/drm/renderD*/device/driver`)
is not `nvidia`. Only `i915` was verified; other drivers are attempted. If a phone in
GPU mode does not finish booting within the time limit, Verone recreates it once in
software mode, warns the user and records that in `state.toml`.

### Boot, stop and restart (measured 2026-11-15)

- Booted means `getprop sys.boot_completed` inside the container returns `1`. Poll
  once per second for up to 120 s. Typical boot is 10 to 25 s.
- `docker stop` does not shut Android down: it waits the full grace period and then
  kills the container (exit code 137). Verone therefore never relies on it.
- Clean stop sequence: `docker update --restart no`, then run `reboot -p` inside the
  container, then wait up to 20 s for the container to exit (measured: under 1 s, exit
  code 130). If it has not exited, `docker kill`.
- Start sequence: `docker update --restart unless-stopped`, then `docker start`.
- A restart requested inside Android exits with code 129 and the restart policy brings
  the phone back (measured: 12 s).
- Known limit in Phase 1: "Power off" chosen inside Android also comes back, because
  the restart policy cannot tell exit code 130 from 129. Document it; Phase 2 fixes it
  when the app supervises the phone.

### adb

- Endpoint: the container's address from `docker inspect`, port 5555. No port is
  published unless the user sets `adb.local_port`.
- Before every use: `adb connect <endpoint>`, then `adb -s <endpoint> shell true`. On
  failure, `adb disconnect <endpoint>`, reconnect, and retry up to three times. A stale
  connection returns `error: closed` and otherwise silently runs nothing.
- On stop, `adb disconnect <endpoint>`, because another container can later receive
  the same address.

### Command line

```
verone doctor
verone image pull [--android 14]
verone image list
verone create <id> [--name TEXT] [--android 14] [--size 720x1280] [--dpi 320]
                   [--graphics auto|software|gpu] [--adb-port N]
verone start <id>
verone stop <id>
verone restart <id>
verone list
verone info <id>
verone open <id> [-- scrcpy options]
verone adb <id> -- <adb arguments>
verone install <id> <file.apk>
verone reset <id> [--yes]
verone delete <id> [--yes]
verone completions <shell>
```

Global options: `--json` (machine-readable output for `doctor`, `list`, `info`,
`image list`), `-v` (show the external commands being run), `--version`.

| Exit code | Meaning |
|---|---|
| 0 | success |
| 1 | operation failed |
| 2 | wrong usage |
| 3 | host check failed |
| 4 | phone or image not found |
| 5 | phone is busy (another `verone` command holds its lock) |

---

## Tasks

### Milestone A — Foundation

#### P1-01 Workspace skeleton
- Depends on: owner preparation.
- Do: create the workspace and three crates as laid out above; `rust-toolchain.toml`;
  `.gitignore` for `target/`; `CHANGELOG.md`; a GitHub Actions workflow running the
  three checks from rule 8 on every pull request; `verone --version`.
- Done when: `cargo build --release`, `cargo test`,
  `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` succeed, and
  `target/release/verone --version` prints `verone 0.1.0`.

#### P1-02 Errors, process runner, output
- Depends on: P1-01.
- Do: in `verone-core`, an error type covering the exit codes above, each error
  carrying a message and a "what to do" hint; a process module that runs a program
  with an argument list, a timeout, captured output and optional pass-through of the
  terminal; printing helpers for normal and `--json` output; `-v` logging of commands.
- Done when: unit tests cover timeout, non-zero exit, program-not-found and output
  capture; `verone nonsense` exits with code 2.

#### P1-03 Storage layout and locking
- Depends on: P1-02.
- Do: resolve the data home; create, find, list and remove phone folders; validate
  ids; per-phone exclusive lock on `.lock` using the standard library's file locking,
  failing with exit code 5 after 10 s.
- Done when: unit tests using a temporary `VERONE_HOME` cover each path rule, every
  invalid id form, and two processes contending for one lock.

#### P1-04 Phone profile
- Depends on: P1-03.
- Do: `phone.toml` and `state.toml` types with reading, writing and validation;
  reject unknown `schema` values with a clear message; defaults as in the example.
- Done when: unit tests cover round-trip, each validation failure, and a file with a
  higher schema number.

### Milestone B — Engine

#### P1-05 Engine trait and fake engine
- Depends on: P1-04.
- Do: define the `Engine` trait in `verone-core` with the operations in
  `../architecture.md` (`check_host`, `prepare_image`, `create`, `destroy`, `start`,
  `stop`, `status`, `control_endpoint`, `capabilities`); a `FakeEngine` for tests that
  records calls and simulates states.
- Done when: unit tests drive the fake through create, start, stop and destroy, and
  `cargo doc` builds without warnings.

#### P1-06 Runtime wrapper
- Depends on: P1-05.
- Do: in `verone-engine-container`, find `docker`; typed wrappers for `version`,
  `info`, `pull`, `image inspect`, `create`, `start`, `update`, `kill`, `rm`,
  `inspect`, `exec` and `run --rm`, all through the process module, parsing JSON output.
- Done when: unit tests run against a stub `docker` script placed first on `PATH`;
  with `VERONE_TEST_DOCKER=1`, an integration test reads the real runtime's version.

#### P1-07 Host checks
- Depends on: P1-06.
- Do: implement `check_host` returning a list of checks, each `ok`, `warning` or
  `failed` with a fix. All file reads take a root path so tests can use a fake tree.

  | Check | Passes when | Fix text names |
  |---|---|---|
  | Runtime | `docker info` succeeds | installing Docker, starting the service, the `docker` group |
  | Binder | `/proc/filesystems` lists `binder` | `modprobe binder_linux`; on Ubuntu `linux-modules-extra-$(uname -r)` |
  | Kernel modules | `/lib/modules/<running kernel>` exists | rebooting after a kernel update |
  | Image storage space | 4 GB free where the runtime stores images, when the image is not yet present. Check the runtime's root folder and `/var/lib/containerd` if it exists | freeing space, with the path |
  | Data space | 2 GB free in the data home | freeing space, with the path |
  | Memory | 1.5 GB available (warning only) | closing applications |
  | Graphics | reports each render node, its driver and the mode `auto` would pick (never fails) | – |
  | adb, scrcpy | found on `PATH` (warning only) | the package names |
- Done when: unit tests cover pass and fail for every row using fake trees and a stub
  runtime; on the reference laptop `verone doctor` shows no failure and exits 0;
  `PATH=/nonexistent verone doctor` exits 3 and names the runtime.

#### P1-08 Images
- Depends on: P1-06.
- Do: the image table; `prepare_image` pulls by digest when absent, showing the
  runtime's own progress; `verone image pull` and `verone image list`; the helper
  image is pulled the same way when first needed.
- Done when: unit tests cover table lookup and an unsupported version (exit 4);
  integration: `verone image pull` is a no-op when the image is present and
  `verone image list --json` reports the digest.

#### P1-09 Container lifecycle
- Depends on: P1-07, P1-08.
- Do: implement `create`, `start`, `stop`, `status` and `destroy` exactly as in
  "Container", "Graphics mode" and "Boot, stop and restart" above, including the spec
  label, the software fallback and the clean stop sequence.
- Done when (integration, `VERONE_TEST_DOCKER=1`): a phone boots within 60 s; status
  reads stopped, booting, running in turn; `reboot` inside the container returns to
  running within 60 s; the stop sequence ends with exit code 130 within 20 s; a
  changed display size causes recreation with data intact; a boot forced to time out
  falls back to software mode once.

#### P1-10 adb endpoint
- Depends on: P1-09.
- Do: `control_endpoint` and a helper that returns a ready adb connection following
  the "adb" rules above; optional published port.
- Done when (integration): `getprop ro.build.version.release` through the helper
  returns `14`; it still works after a phone restart without manual reconnection; with
  `--adb-port 5599` the port listens on `127.0.0.1` only.

### Milestone C — Commands

#### P1-11 `create`, `list`, `info`
- Depends on: P1-09.
- Do: `create` writes the profile and data folder and does not start the phone;
  `list` shows id, name, Android version and status; `info` adds address, graphics
  mode in use, data size and uptime. All three support `--json`.
- Done when: `assert_cmd` tests cover create, duplicate id (exit 1), bad id (exit 2),
  empty list, and JSON output that parses.

#### P1-12 `start`, `stop`, `restart`
- Depends on: P1-10, P1-11.
- Do: run host checks first and stop with exit 3 on failure; pull the image if
  needed; show elapsed boot time; each command takes the phone's lock.
- Done when (integration): start on a new phone prints the boot time and exits 0;
  start on a running phone says so and exits 0; stop on a stopped phone says so and
  exits 0; two simultaneous `start` commands give one success and one exit 5.

#### P1-13 `open`, `adb`, `install`
- Depends on: P1-12.
- Do: `open` starts the phone if needed, then runs scrcpy with the window title
  `Verone – <name>` and any options after `--`; `adb` passes its arguments to the
  phone's endpoint; `install` installs an APK and reports the package name.
- Done when (integration): `verone open <id> -- --no-window --time-limit=5` exits 0;
  `verone adb <id> -- shell getprop sys.boot_completed` prints `1`; installing the
  F-Droid APK used in Phase 0 succeeds.

#### P1-14 `reset`, `delete`
- Depends on: P1-12.
- Do: both ask for confirmation unless `--yes`; both stop the phone cleanly first;
  `reset` empties `data/` through the helper image and keeps the profile; `delete`
  removes the container and the whole phone folder.
- Done when (integration): after `reset`, an app installed before is gone and the
  phone boots; after `delete`, the folder and container no longer exist; answering
  "no" changes nothing.

#### P1-15 `doctor`
- Depends on: P1-07.
- Do: print each check with its result and fix; `--json`; exit 3 on any failure.
- Done when: covered by P1-07's tests plus an `assert_cmd` test of the JSON shape.

### Milestone D — Release

#### P1-16 End-to-end exit test
- Depends on: P1-11 to P1-15.
- Do: write `tests/e2e/phase-1.sh` using a temporary `VERONE_HOME`:
  `doctor`; `create e2e`; `start e2e`; `open e2e -- --no-window --time-limit=5`;
  install an app; `stop`; `start`; confirm the app is present; `reset --yes`; `start`;
  confirm the app is gone; `delete --yes`; confirm no `verone-e2e` container remains.
- Done when: the script exits 0 on the reference laptop, twice in a row.

#### P1-17 Packaging
- Depends on: P1-16.
- Do: `packaging/arch/PKGBUILD` building from the repository, depending on `docker`
  and optionally on `android-tools` and `scrcpy`; shell completions installed; release
  profile with size optimisation.
- Done when: `makepkg -f` builds; the package installs; the binary is under 10 MB;
  P1-16 passes using the installed binary.

#### P1-18 Documentation and release
- Depends on: P1-17.
- Do: quick start in the root `README.md`; `CONTRIBUTING.md`; `SECURITY.md` stating
  the privileged-container and local-only adb facts; `docs/user-guide/cli.md` with
  every command and the known limits; update `../architecture.md` with the final
  trait; mark Phase 1 complete in `../roadmap.md`.
- Done when: a person following only the README reaches a running phone, and the
  owner creates the `v0.1.0` tag.

### Optional

#### P1-19 Podman
- Depends on: Phase 0 test T-07 passing, and the owner's approval.
- Do: runtime selection (`VERONE_RUNTIME=podman`, or automatic when Docker is absent).
- Done when: P1-16 passes with Podman.

---

## Phase exit

Phase 1 is complete when P1-01 to P1-18 are merged and the roadmap's exit test holds:
on a supported system, `verone doctor`, `verone create`, `verone start` and
`verone open` give a working phone; after `stop` and `start` an installed app is still
there; after `reset` it is gone.

The designer then reviews what was learned, updates `../assumptions.md`, and writes
`phase-2.md`.
