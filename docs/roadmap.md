# Roadmap

Status: **draft for owner review** (2026-11-14).

## How to work from this roadmap

- Phases run in order. A phase starts only when the previous phase's exit test passes.
- Before a phase starts, the designer writes its detailed task list in
  `docs/phases/phase-N.md`: one task per unit of work, each with an ID, the files it
  touches and an acceptance test. Agents work from that file, not from this overview.
- Every task ends with its acceptance test passing on the owner's laptop (8 GB RAM).
- Results of assumption tests go into `assumptions.md` with date and evidence.
- A task that would contradict `decisions.md` or `vision-and-scope.md` stops and goes to the owner.
- No time estimates are given here. Phase 0 will show how fast the work really moves.

## Phases at a glance

| Phase | Name | Outcome |
|---|---|---|
| 0 | Proof | Every blocking assumption tested; go or change the design. |
| 1 | Core and command line | `verone` creates, starts, stops, resets and lists phones. |
| 2 | Desktop app | A phone in a window, usable without a terminal. |
| 3 | Phone fidelity and lock | First-boot setup, optional Google apps, encrypted storage, backups, camera. |
| 4 | Developer and tester toolkit | Many phones, snapshots, profiles, automation, CI. |
| 5 | Verone's own Android image | Release-quality system built for Verone. |
| 6 | Self-hosted server | Phones on a server, reached from a browser or the app. |
| 7 | VM engine | Full-fidelity phones with their own Android kernel. |
| 8 | Windows | Native Windows app on the VM engine. |

Packaging, user documentation and release work run inside every phase from Phase 1 on.

---

## Phase 0 — Proof

Purpose: replace guesses with measurements before any product code is written.
Detailed steps: [phases/phase-0.md](phases/phase-0.md).

Owner prerequisite: at least 8 GB free on the root partition, where the container
runtime stores images on the owner's laptop. One Android image needs about 3 GB.

Work:

- Test assumptions A-02, A-03, A-07, A-08, A-10, A-11, A-14 on the owner's laptop.
- Test A-04, A-05, A-06, A-15 (Google apps path) with a throwaway Google account.
- Test A-09 (encrypted storage) with at least two candidate mechanisms.
- Check A-12 (binder on other distributions) by inspecting their kernel packages; confirm
  on a non-Arch system if one is available.
- Check that the name "Verone" is free to use for software: trademark registers, package
  repositories, existing projects.
- Record every result in `assumptions.md`.

Exit test: every assumption marked "blocking" for Phase 1 has passed, or the owner has
approved a design change. Proposed decisions D-011 to D-014 are accepted or replaced.

## Phase 1 — Core and command line

Purpose: the engine layer and a complete command-line tool. Everything later sits on this.
Detailed tasks: [phases/phase-1.md](phases/phase-1.md).

Deliverables:

- `verone-core` with the engine interface and the container engine.
- `verone doctor`: host checks with a specific fix for each failure.
- `verone create | start | stop | restart | list | info | reset | delete`.
- `verone open`: show a running phone using the system's scrcpy.
- Image download with progress and checksum verification.
- Automatic return after a restart requested inside Android.
- Clean shutdown of Android on `verone stop` (the runtime's own stop only kills it).
- adb reachable locally only; optional `127.0.0.1` port for IDEs.
- Automated tests that boot a real phone.
- Install instructions for Arch; first tagged release.

Exit test: on a fresh supported system, `verone doctor`, `verone create`, `verone start`
and `verone open` give a working phone; after `stop` and `start` an installed app is
still there; after `reset` it is gone.

## Phase 2 — Desktop app

Purpose: the experience for people who never open a terminal.

Starts with an experiment on A-13 (showing the screen inside the app window).

Deliverables:

- App window with the phone screen and a side toolbar: power, volume, rotate, back,
  home, recent apps, screenshot.
- First-run flow: host check, image download, create first phone.
- Phone list: create, rename, start, stop, reset, delete.
- Clipboard shared both ways; drag a file or APK onto the phone.
- A shared folder between the computer and the phone.
- Screen recording to a video file.
- All interface text translatable; English first.
- No usage data collected or sent anywhere.
- Sound from the phone; keyboard and mouse wheel input.
- GPU acceleration when available, software rendering otherwise.
- Settings: screen size and density, memory limit, start with the computer.
- Packages: AppImage and Arch package; others as contributors appear.

Exit test: a non-technical user installs the package, follows the first-run flow and
reaches a running phone without using a terminal.

## Phase 3 — Phone fidelity and lock

Purpose: behave like a phone someone could rely on.

Deliverables:

- "With Google" and "without Google" at phone creation (D-014). The Google path builds
  the image locally and walks the user through device registration (D-005, D-008).
- First-boot setup wizard where the image provides one.
- Verone lock (D-007): encrypted storage, password, auto-lock, lock on app close.
- Encrypted backup and restore; export to a file.
- Camera from the computer's webcam (A-16); scan a QR code from the computer screen.
- Microphone; notifications shown on the desktop; battery and location shown sensibly.
- Hide or disable settings that cannot work (SIM, mobile data) as far as a prebuilt image allows.
- User guide, including the security statement and known app limits.

Exit test: a user sets up a phone with an account, locks it, restores it from a backup
on a second machine and finds everything intact; the data on disk is unreadable without
the password.

## Phase 4 — Developer and tester toolkit

Purpose: make Verone the easiest Android target for people who build and test apps.

Deliverables:

- Several phones running at once; device profiles (sizes, densities, Android versions).
- Snapshots: save, restore, clone a phone.
- Temporary phones: created for one session and erased when closed.
- Root access switchable per phone; clear labelling of a rooted phone.
- Network controls for testing one's own apps: proxy, offline, slow network, custom
  certificate authority.
- Scripted setup: a file that describes a phone and what to install on it.
- Headless mode and a container image for use in CI.
- Local API that the app and scripts share.
- Developer guide and example CI configuration.

Exit test: a CI job creates a phone from a script, installs an app, runs its tests and
tears the phone down, with no display attached.

## Phase 5 — Verone's own Android image

Purpose: remove the limits of prebuilt images. Needs a rented build server (D-004).

Deliverables:

- Reproducible build recipe; images published by the project for each supported Android version.
- Release-type build, modern launcher, clean app set, phone-appropriate settings.
- Camera and other hardware support built in.
- Update mechanism: new system image, same data.

Exit test: a phone created from Verone's image passes the Phase 1 to 3 exit tests and
updates to a newer image without losing data.

## Phase 6 — Self-hosted server

Purpose: phones that live on the owner's or a team's server (D-003).

Deliverables:

- `verone serve`: background service with an authenticated API.
- Web client and desktop-app connection to remote phones; encrypted transport.
- Users and permissions suitable for a team; audit log.
- Resource limits per phone; installation guide for a clean server.

Exit test: a phone on a remote server is used from a browser on another machine over
an encrypted, authenticated connection; an unauthenticated visitor reaches nothing.

## Phase 7 — VM engine

Purpose: full fidelity for security work and the foundation for Windows.

Deliverables:

- Second engine behind the existing interface, with its own Android kernel.
- Engine choice per phone in the app and command line.
- Documented comparison of what each engine does and does not reproduce of a real phone.

Exit test: the Phase 1 exit test passes on the VM engine with no change to front-end code.

## Phase 8 — Windows

Purpose: native Windows application without WSL2 (D-001).

Deliverables: Windows port of the app on the VM engine; installer; Windows user guide.

Exit test: the Phase 2 exit test passes on Windows 11.

---

## Not scheduled

Ideas kept for later are listed in [future-ideas.md](future-ideas.md). Each needs the
owner's approval before entering a phase.
