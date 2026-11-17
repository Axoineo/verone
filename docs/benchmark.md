# Phone fidelity benchmark

Status: **accepted** (2026-11-16, decision D-016). Not built yet; work starts from the
build plan at the end of this file.

## Why this exists

Verone's first product principle is "behave like a phone". Today nothing measures how
far Verone, or any other virtual Android system, is from that goal.

Published comparisons of virtual Android systems measure speed, mostly for games. No
public test puts the Android Emulator, Cuttlefish, redroid and Waydroid side by side
and scores them against a **real phone**. Google states that Cuttlefish aims to behave
like a physical device, but that is a design goal, not a measurement. (Search done
2026-11-16; if a comparable benchmark turns up later, record it here and compare.)

This benchmark fills that gap. It answers three questions:

1. How close is each virtual Android system to a real phone, area by area?
2. Where does Verone add something over the redroid it is built on?
3. After each phase, did Verone move closer to a real phone?

The method is written so that anyone can run it on any of the systems compared, not
only on Verone. A fair, repeatable and public measuring stick is useful to other
projects too, and gives people outside Verone a reason to trust the results.

## What to expect from the first run

Verone 0.1 runs redroid underneath. On app compatibility and speed it will score about
the same as plain redroid. That is expected and must be published as it is. Verone's
difference shows in the checks that depend on Verone's own work: clean shutdown,
automatic return after a restart, and later the setup wizard, the lock, the camera and
backups. Each failed check is a candidate item for the roadmap.

## What is compared

| Contender | What it is | Configuration used |
|---|---|---|
| **Real phone** | The reference. Its result counts as 100%. | The owner's vivo V2027, Android 12, USB debugging on. |
| **Verone** | This project. | `verone create` and `verone start` with default options. |
| **redroid** | The container Android Verone is built on. | `redroid/redroid:14.0.0-latest` with the baseline command from `phases/phase-0.md`. |
| **Android Emulator** | Google's emulator from the Android SDK. | Android 14 (API 34) x86-64 image without Google apps, default device settings. |
| **Cuttlefish** | Google's virtual-machine Android device. | Android 14 build from Google's published builds, default launch options. |
| **Waydroid** | Container Android for Linux desktops, LineageOS based. | Its standard image, whatever Android version that is (record it). |

### Two Android versions

The reference phone runs **Android 12** (assumption A-21). On 2026-11-16 the owner chose
to run the benchmark in two sets:

| Set | Android | Contenders | Used for |
|---|---|---|---|
| **A (main)** | 12 | Real phone, Verone, redroid (`redroid/redroid:12.0.0-latest`), Android Emulator (`system-images;android-31;default;x86_64`), Cuttlefish where it runs | The phone fidelity score: like-for-like with the real phone, including the compatibility tests (B3-03) |
| **B** | 14 | Verone, redroid, Android Emulator, Cuttlefish, Waydroid | Where Verone's default version stands; checks that do not depend on the Android version are compared with the real phone's Android 12 result, marked so |

Notes:

- **Verone in set A** uses Android 12 support added on 2026-11-16 with the owner's
  approval (`crates/verone-engine-container/src/images.rs`, assumption A-22). Restart
  and power-off inside Android 12 work through the power watcher (D-017). Plain
  redroid 12 has no watcher and is expected to fail B1-03 and B1-04. Verone's power-off
  is abrupt rather than clean, which the report must state against B1-04.
- Waydroid offers LineageOS images (Android 11 and 13 at the time of writing; to be
  confirmed in A-19), not Android 12. It is in set B only, with its version recorded.
- The headline numbers in the README come from set A. Set B is published beside it.

Rules for a fair comparison:

- Every contender runs on the **same computer** in the same run, one at a time, with
  nothing else heavy running.
- Within a set, every contender uses that set's Android version. Where one cannot,
  record the version used next to every result.
- Use each contender **as a new user would get it**: default settings, no tuning. Any
  setting changed to make a check possible is written in the results.
- **Without Google apps** for every virtual contender, so they match Verone's default
  mode. The real phone will have Google apps; this is recorded, not corrected.
- Verone is never tuned for the benchmark in a way the others could not be.
- Results are published in full, including every check Verone fails.

Not every contender may run on the owner's laptop (8 GB RAM). See assumptions A-17 to
A-21. A contender that cannot run is listed as "not run" with the reason, never left out
silently.

## How it works

Every contender, including the real phone, can be controlled with `adb`. That is what
makes a fair comparison possible: one set of checks, sent over `adb`, runs unchanged on
all of them. Only starting, stopping and resetting differ per contender, and each
contender gets a small adapter for those three actions.

A few checks cannot be judged by a script (for example "is the phone's sound heard on
the computer"). These are marked **manual**. The script stops, tells the person running
it exactly what to do and what to look for, and records their answer.

## Scoring

Each check gives one result:

| Result | Points | Meaning |
|---|---|---|
| pass | 1 | Behaves as on the real phone. |
| partial | 0.5 | Works with a limit that the check's own rule describes. |
| fail | 0 | Does not work, or claims to work and does not. |
| not applicable | none | The check does not apply to this contender (written reason required). |

**Area score** = points earned ÷ number of checks that pass on the real phone, as a
percentage. Checks the real phone itself fails are reported but left out of the score.

**Phone fidelity score** = the average of the area scores for areas 1 to 4, with equal
weight. Area 5 (cost and speed) is reported as raw numbers and is not part of the
fidelity score, because a computer's memory use cannot be compared with a phone's.

Every result carries the reason for a partial or failed check, so a reader can see what
is missing, not only that something is.

## The checks

Check IDs (`B1-03` and so on) stay fixed once published, so results from different
dates can be compared. New checks get new IDs. A removed check keeps its ID, marked
retired.

### Area 1 — Behaves like a phone

| ID | Check | Pass rule |
|---|---|---|
| B1-01 | First boot shows a setup wizard | A fresh phone starts in a setup flow, not straight on the home screen. |
| B1-02 | Screen lock | A PIN can be set; after the screen turns off and on, the lock screen asks for it. |
| B1-03 | Restart from inside | A restart chosen inside Android brings the phone back to the home screen by itself. |
| B1-04 | Power off from inside | A power-off chosen inside Android shuts it down cleanly; the next start works. |
| B1-05 | Notifications | A posted notification appears in Android's notification list. |
| B1-06 | Rotation | Switching to landscape turns the display and apps follow. |
| B1-07 | Camera | Android reports at least one camera, and the standard "take a picture" action returns an image. Partial if the image is a test pattern rather than a real camera. |
| B1-08 | Microphone (manual) | A recording app records sound spoken near the computer. |
| B1-09 | Sound out (manual) | A sound played on the phone is heard on the computer. |
| B1-10 | Battery | Android reports a battery level and charging state without errors. |
| B1-11 | Location | An app asking for the location receives a position. Partial if only a manually set position is possible. |
| B1-12 | Sensors | Android lists an accelerometer, gyroscope and proximity sensor. Partial if some are listed. |
| B1-13 | Clipboard with the computer | Text copied on the computer can be pasted on the phone and back. Partial if one direction only. |
| B1-14 | Files from the computer | A file can be moved onto the phone without command-line tools. Partial if only with `adb`. |

For a real phone B1-13 and B1-14 mean "with the computer it is connected to". Real phones
need a helper app for B1-13, so B1-13 counts as pass on the phone if any common tool does it.

### Area 2 — Keeps your things

| ID | Check | Pass rule |
|---|---|---|
| B2-01 | Stop and start | A file, an installed app and a changed setting are all still there after a normal stop and start. |
| B2-02 | Computer restart | The same three survive a restart of the computer itself (for a real phone: a phone restart). |
| B2-03 | Unexpected stop | After the contender is killed without warning, it starts again and the three are intact. Partial if it starts but something is lost. |
| B2-04 | Factory reset | Reset removes the file, the app and the setting, and the next boot behaves like a first boot. |
| B2-05 | Backup and restore | A backup made with the contender's own tool restores the three onto a fresh phone. Not applicable if the contender has no backup tool; the real phone uses its built-in backup. |

### Area 3 — Runs apps

| ID | Check | Pass rule |
|---|---|---|
| B3-01 | App list | Each app in the fixed app list installs, opens, and survives a short automatic tapping run (Android's `monkey` tool) without crashing. Score is the share of apps that pass. |
| B3-02 | ARM-only apps | Same as B3-01 for the ARM-only part of the list. |
| B3-03 | Compatibility tests | A fixed selection of modules from Google's Compatibility Test Suite (CTS). Score is the share of tests passed, relative to the real phone's share. |
| B3-04 | Installs from an app store | F-Droid installs and can install an app from inside itself. |

The app list is about 20 open-source apps from F-Droid across browsers, maps, media,
messaging, office and games, plus at least three apps that ship only ARM code. The list
and the CTS module selection are fixed in `tests/bench/apps.txt` and
`tests/bench/cts-modules.txt` and change only with a new benchmark version.

### Area 4 — Tells the truth

A virtual phone that claims hardware it does not have breaks apps in confusing ways.
These checks reward honesty, not disguise (decision D-005).

| ID | Check | Pass rule |
|---|---|---|
| B4-01 | Declared hardware works | Every hardware feature Android declares (camera, telephony, Wi-Fi, Bluetooth, NFC, sensors) actually works. Fail for each feature declared but not working; the score is the share that is consistent. |
| B4-02 | Network type | The network is presented as Wi-Fi or another type apps handle normally, and apps see the internet as available. |
| B4-03 | Identity is honest | Build properties describe the system truthfully: no copied retail fingerprint. Pass for the real phone by definition. |
| B4-04 | Missing features are explained | Settings that cannot work (SIM, mobile data) are hidden or say why. Partial if they are present but fail quietly. |

**Not scored:** Play Integrity, SafetyNet and hardware key attestation. Their results are
recorded for information only. Improving them is a non-goal (see `vision-and-scope.md`).

### Area 5 — Cost and speed

Raw numbers, not scored. Each timing is run three times; the middle value is reported.

| ID | Measure |
|---|---|
| B5-01 | Steps and time from nothing installed to a booted phone, following the contender's own documentation |
| B5-02 | First boot time (start command to `sys.boot_completed=1`) |
| B5-03 | Later boot time |
| B5-04 | Memory used on the computer, idle at the home screen after 2 minutes |
| B5-05 | Processor use on the computer, idle, averaged over 60 seconds |
| B5-06 | Disk space used: system plus one phone |
| B5-07 | App launch time for three apps from the app list (`am start -W`) |
| B5-08 | Frame rate while scrolling a long list, with and without graphics acceleration |
| B5-09 | Copy speed of a 200 MiB file onto the phone and back |

For the real phone B5-04 to B5-06 are left empty; the rest are measured.

## Results

Each run produces:

- `docs/benchmark-results/<date>.md`: the readable report. It has a summary table
  (contenders across, area scores down), then every check with its result and reason.
- `tests/bench/results/<date>.json`: the same data for scripts and charts.

Every report starts with what was used: the computer (processor, memory, graphics,
kernel, distribution), each contender's version and Android version, the benchmark
version, and the real phone's model.

The benchmark is run at the end of every roadmap phase, and the result is linked from
the phase's exit notes. A drop in any area score is a bug to explain.

### Showing the results in the README

The README gets a short "How Verone compares" section, filled **only from a real run**.
No chart or claim is drawn before there is measured data behind it.

| Visual | What it shows | Why |
|---|---|---|
| Phone fidelity bar chart | One bar per contender, real phone at 100% | The headline: how phone-like each one is |
| Area chart | Areas 1 to 4 side by side for each contender | Shows *where* each one is strong or weak |
| Comparison table with ✓, ½ and ✗ | The ten most telling checks, per contender | Readable at a glance, works everywhere |
| Cost chart | Boot time and memory use per contender | The "light" side of Verone, in numbers |
| Progress chart (from Phase 2 on) | Verone's score after each phase | Shows the project moving towards a real phone |

How they are made:

- The report generator draws the charts as **SVG files** from the JSON results, in
  `docs/benchmark-results/charts/`. GitHub shows SVG images in a README without any
  extra service. Each chart has a light and a dark version, shown with `<picture>` like
  the logo, in the brand colours from `docs/brand/`.
- Every chart has a caption with the date, the computer and the benchmark version, and
  links to the full report.
- Contenders that win a check are shown winning. Where the Android Emulator or
  Cuttlefish is better, the chart says so. A reader who sees that Verone's results are
  fair will believe the places where Verone is ahead.
- The section links to "Run it yourself" instructions, so anyone can check the numbers.

The benchmark itself has a version (starting at 1.0). Changing a check, the app list or
the CTS selection makes a new version; results are only compared within one version.

## Build plan

D-016 was accepted on 2026-11-16. One commit per task, one pull request for the whole
plan.

| ID | Task | Acceptance test |
|---|---|---|
| BM-01 | Test assumptions A-17 to A-21 on the owner's laptop and record the results. | Each has a result and evidence in `assumptions.md`. |
| BM-02 | Adapters: start, stop, kill and reset for each contender that runs, in both sets, under `tests/bench/contenders/`. The real phone's adapter never resets or changes the phone without asking. | Each adapter boots its contender to `sys.boot_completed=1` and stops it, three times in a row. |
| BM-03 | The check runner: runs every automatic check over `adb`, prompts for manual ones, writes the JSON result. | A full run on Verone completes and the JSON lists every check ID. |
| BM-04 | Fix the app list and the CTS module selection. | Both files exist; every app installs on the real phone; the CTS selection runs on the real phone. |
| BM-05 | Report generator: JSON to the Markdown report with the summary table, plus the SVG charts (light and dark) described in "Showing the results in the README". | For a sample JSON the report matches the format above and every chart opens in a browser in both themes. |
| BM-06 | First full run of sets A and B on all contenders that run; publish the report and charts as the Verone 0.1 starting point. | `docs/benchmark-results/<date>.md` exists and the owner has read it. |
| BM-07 | "How Verone compares" section in the README: fidelity chart, area chart, comparison table, cost chart, links to the full report and to "Run it yourself". | The section shows correctly on GitHub in light and dark mode, and every number matches the published report. |

How the runner is written (shell scripts like `tests/e2e/`, or part of the Rust
workspace) is decided in BM-03; shell is the default because every check is an `adb`
command.
