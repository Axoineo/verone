# Restart and power-off on Android 12

Status: **accepted and built** (2026-11-17, decision D-017). Tasks P12-01 to P12-05 are done; results under "Results".

## The problem

On Android 14, choosing "Restart" or "Power off" inside Android ends the phone's
container with exit code 129 (restart) or 130 (power off). Verone relies on that.
Docker's restart policy brings a restarted phone back, and `verone stop` waits for
code 130 (A-03 addendum).

On Android 12 (`redroid/redroid:12.0.0-latest`), init receives the request and then
does nothing (A-22). As a result:

| What the user does | What happens now on Android 12 |
|---|---|
| "Restart" inside Android | Nothing. The phone keeps running. |
| "Power off" inside Android | Nothing. The phone keeps running. |
| `verone stop` | Waits the full 20 seconds, then kills the phone (exit code 137). |

This fails benchmark checks B1-03 and B1-04, and makes `verone stop` slow and abrupt.
The deeper cause inside Android 12's init was not found.

## What was measured

All on 2026-11-17, owner's laptop, phone `bench12` (Android 12) created with Verone.

1. After a request inside Android, the property `sys.powerctl` holds the request
   (`reboot,shell` or `shutdown,shell`) and keeps it.
2. A process outside the phone can read it. A helper container started with
   `--privileged --pid=host` from Verone's existing helper image (alpine, pinned by
   digest) ran `nsenter -t <phone's init> -m -u -i -n -p -r -w /system/bin/getprop
   sys.powerctl` and saw the value.
3. The same helper can end the phone exactly as Android 14 would. The kernel ends a
   container when any process inside its process namespace makes the real `reboot`
   system call: code 129 for a restart, 130 for a power-off.
   - `nsenter -t <pid> -p reboot -f`: Docker's restart policy brought the phone back;
     Android was booted again within about 3 s; the request was cleared; files kept.
   - `nsenter -t <pid> -p poweroff -f` with the restart policy off: the phone stopped
     with exit code 130, 2 s after the request.

## Options considered

| Option | Verdict |
|---|---|
| **A. Power watcher: one small helper container per computer** that notices requests and makes the real call. | **Recommended.** Uses only what Verone already ships and pulls; tested by hand above; works for any Android version with the same fault. |
| B. A watcher started inside each phone at `verone start` (`docker exec`) | Rejected. It disappears when Docker restarts the phone, so only the first restart would work. Android 12 also has no tool that makes the real call. |
| C. Wait for a long-running Verone process (desktop app in Phase 2, local API in Phase 4) to do the watching | Rejected for now: Android 12 is needed for the benchmark sooner. The watcher's job can move into that process later. |
| D. Find and fix the cause inside Android 12's init, by changing the image on the user's machine (allowed by D-004) | Not chosen: unknown cost, and a debug build of init would be needed. Worth a time-limited look later; if it succeeds, option A can be switched off for Android 12. |
| E. Make `verone list`, `info` and `stop` notice a pending request and act then | Rejected: nothing happens until the user runs a command, which does not behave like a phone. |

## Design of option A

### What runs

One container per Verone data folder, named `verone-power-<code>`, where `<code>` is
8 hex digits derived from the phones folder. Normally there is one. Tests run phones
from temporary folders; with a single shared name, a test run would replace the
watcher of the user's own phones.

- Image: Verone's helper image (`HELPER_IMAGE`, alpine pinned by digest). No new
  download.
- Options: `--privileged --pid=host --network none --read-only
  --restart unless-stopped`, label `io.verone.role=power-watcher`.
- Command: a short shell script kept in the Verone source and passed to the container
  at creation. It works in two rhythms:
  1. Every 10 seconds, and at once when a watched phone's process has ended, it finds
     Verone phones: processes that are the first process of their own process
     namespace, and whose mount table contains Verone's phone data folder
     (`…/verone/phones/<id>/data` mounted on `/data`). This matches Verone's phones and
     nothing else, including other redroid containers the user runs.
  2. Every second, it reads `sys.powerctl` in each phone found with `nsenter … /system/bin/getprop`.
  3. If it starts with `reboot`: runs `sync` inside the phone, then
     `nsenter -t <pid> -p reboot -f`. If it starts with `shutdown`: `sync`, then
     `nsenter -t <pid> -p poweroff -f`.
  4. Writes one line per action to its log (`docker logs verone-power-<code>`): time, phone,
     request.

### Which phones it acts on

Only phones whose Android version is marked in `images.rs` as ignoring power requests.
A new field on `Image`, `power_watcher: bool`, is true for Android 12 and false for 14.
Verone builds the script with the list of those versions (today just `12`), and the
script reads each phone's `ro.build.version.release` and skips any other version.
Android 14 phones are never touched, so their tested behaviour does not change.

### Lifecycle

- `verone start` of a phone that needs it: create and start the watcher if it is
  not running.
- `verone delete` of the last phone that needs it: remove the watcher.
- `verone doctor`: shows whether the watcher is running when an Android 12 phone exists.
- `verone stop` is unchanged: restart policy off, power-off request, wait up to 20 s,
  kill as a last resort. With the watcher it ends in about 2 s with code 130.

### What it costs

- Measured in task P12-04 (see "Results"): 1.7% of one core on average and at most
  3 MB of memory. The first version looked through every process every second and
  used 21%; finding phones only every 10 seconds brought it down. Most of what is left
  is starting Android's `getprop` once a second per phone. Reading Android's property
  file directly could lower it further; not tried yet.
- No new download, no new dependency.

### Honest limits

- **It is not a clean Android shutdown.** On Android 14, Android stops its services and
  apps before the container ends. On Android 12 with the watcher, apps are not told;
  files are flushed to disk (`sync`) and then the phone ends at once, like pulling the
  power after saving. This is better than today's kill after 20 s, but it is not equal
  to Android 14 and must say so in the user guide and the benchmark (B1-04: partial).
- **Power off inside Android turns the phone back on**, as on Android 14 today, because
  Docker's restart policy brings back any phone that ends while it is on. This is the
  existing known limit, not a new one.
- **Security.** The watcher can see and enter every process on the computer, like
  the phones themselves (privileged containers already give root on the host). It has
  no network, a read-only file system and a fixed script. `SECURITY.md` must list it.

## Tasks

One commit per task; one pull request for all of them.

| ID | Task | Acceptance test |
|---|---|---|
| P12-01 | Add `power_watcher` to `Image` (true for 12, false for 14) and the watcher script as a constant, with unit tests for the script text and the phone matching rule. | `cargo test` passes; the script is shown in the test output for review. |
| P12-02 | Start, check and remove the watcher from the container engine as described in "Lifecycle". | Unit tests with the stub runtime cover start when missing, no second copy, removal after the last Android 12 phone is deleted. |
| P12-03 | Integration test on a real Android 12 phone: restart from inside, power-off through `verone stop`. | Restart: the phone is booted again within 30 s with a marker file intact. Stop: finishes within 5 s with exit code 130. Android 14 phone in the same run: behaviour unchanged. |
| P12-04 | Measure the watcher's CPU and memory over 10 minutes with one Android 12 phone. | Numbers recorded in `assumptions.md`. |
| P12-05 | Docs: user guide known limits, `SECURITY.md`, `architecture.md`, changelog. | The owner has read the changes. |

## Results

Owner's laptop (4 cores, 7.6 GB RAM), 2026-11-17.

| Task | Result |
|---|---|
| P12-01, P12-02 | Unit tests pass: the matching rule (only Verone's Android 12 phones, never Android 14, other redroid containers or processes inside a phone), start before the phone, no second copy, removal with the last Android 12 phone, `verone doctor` line. |
| P12-03 | Real Android 12 phone: back 6.2 s after a restart request inside Android, with a file kept; `verone stop` in 1.2 s with exit code 130; watcher removed with the phone. The Android 14 tests in the same run passed. The adb port test failed for the known, unrelated reason (`xt_tcpudp` not loaded). |
| P12-04 | 10 minutes, one Android 12 phone, sampled every 10 s: CPU 1.7% of one core on average, short peaks up to 26% during the scan every 10 s; memory at most 3 MB; nothing written to the log apart from start-up lines. |
