# Assumptions and risks

The design rests on the assumptions below. Each one is tested once, on a real machine,
and the result is recorded here with the date, the machine and the evidence. No code
may depend on an assumption that is still **untested**.

Result values: **pass**, **fail**, **partial**, **untested**.
"Blocking" means the roadmap cannot continue past the stated phase until it passes or
the design is changed.

## Register

| ID | Assumption | Needed by | Blocking | Result |
|---|---|---|---|---|
| A-01 | A redroid container boots Android to the home screen on a current desktop Linux kernel. | Phase 1 | yes | **pass** 2026-11-13 |
| A-02 | The container gets working internet through the runtime's normal network. | Phase 1 | yes | **pass** 2026-11-14 |
| A-03 | Data survives stop and start. A restart requested inside Android comes back automatically with a restart policy. | Phase 1 | yes | **pass** 2026-11-14 |
| A-04 | Google apps can be added to a prebuilt image entirely on the user's machine, and the Play Store opens. | Phase 3 | no | **pass** 2026-11-15 |
| A-05 | Google sign-in works after the user registers the device on Google's uncertified-device page. | Phase 3 | no | untested |
| A-06 | Google sends "Yes, it's me" sign-in prompts to a signed-in Verone. Informational: see D-006. | none | no | untested |
| A-07 | An app that ships only ARM code installs and runs on an x86-64 host using the translation layer already in the image. | Phase 2 | no | **pass** 2026-11-15 |
| A-08 | GPU-accelerated rendering (`host` mode) works on Intel graphics. Behaviour on NVIDIA and AMD is recorded. | Phase 2 | no | **pass** 2026-11-15 (Intel); NVIDIA proprietary fails; AMD untested |
| A-09 | Phone data can be stored encrypted, unlocked only by a password, with Android running normally on top. | Phase 3 | yes | **pass** 2026-11-15 (gocryptfs); disk-image method not yet tested |
| A-10 | Phone audio plays on the computer. | Phase 2 | no | untested |
| A-11 | The container engine also works with Podman. | Phase 1 | no | untested |
| A-12 | Kernel binder support is available, built in or as a module, on current Ubuntu LTS, Debian stable, Fedora and Arch. | Phase 1 | yes | **pass** 2026-11-15 (by inspection) |
| A-13 | The phone's screen can be shown inside the Tauri app window with acceptable latency. | Phase 2 | yes | untested |
| A-14 | adb can be reached without publishing any port, fast enough for screen streaming. | Phase 1 | no | **pass** 2026-11-15 |
| A-15 | With Google apps added, a fresh phone starts with the standard first-boot setup wizard. | Phase 3 | no | **fail** 2026-11-15 |
| A-16 | The computer's webcam can be presented to Android as its camera on the container engine with a prebuilt image. | Phase 3 | no | untested |
| A-17 | The Android Emulator runs an Android 14 x86-64 image without Google apps on the owner's laptop, and its `adb` works for the benchmark checks. | Benchmark | no | **pass** 2026-11-16 |
| A-18 | Cuttlefish runs Android 14 on the owner's laptop (8 GB RAM) on Arch Linux. Google documents Debian-based hosts and heavy memory use, so a fail here is likely; the alternative is a borrowed or rented larger machine. | Benchmark | no | untested |
| A-19 | Waydroid runs on the owner's laptop, and its image's Android version is recorded. It may not offer Android 14. | Benchmark | no | untested |
| A-20 | The chosen CTS modules can be run over `adb` on every contender, including the container ones. | Benchmark | no | untested |
| A-21 | The owner has a real Android phone (ideally Android 14) that can be used with USB debugging as the reference. | Benchmark | yes | **partial** 2026-11-16 (phone is Android 12) |
| A-22 | redroid's Android 12 image (`redroid/redroid:12.0.0-latest`, 769 MiB compressed for amd64 on Docker Hub, last updated 2024-05-28) boots on the owner's laptop and reaches the home screen. Needed for benchmark set A, and before Verone can offer Android 12. | Benchmark | yes | **pass** 2026-11-16 (restart and power-off from inside do not work) |
| A-23 | The Android Emulator runs the Android 12 image without Google apps (`x86_64-31_r05.zip`, 626 MiB from `dl.google.com`) on the owner's laptop. | Benchmark | no | untested |

## Evidence

**A-01 pass.** 2026-11-13, owner's laptop: Arch Linux, kernel 7.2.14, Docker 29.9.3,
Intel HD 530 + NVIDIA T400, 8 GB RAM, 4 cores. Image `redroid/redroid:14.0.0-latest`,
software rendering, 720×1280. Boot completed in 23 s; 860 MB RAM; 3.5% CPU idle; about
130% of one core while streaming to scrcpy with software rendering.

**A-02 pass.** 2026-11-14, same laptop after reboot into kernel 7.3.4 (test T-01).
Baseline command from `phases/phase-0.md` with Docker's default bridge network and
`-p 127.0.0.1:5555:5555`. Boot completed in 15 s. From inside Android,
`ping -c 3 8.8.8.8` and `ping -c 3 dns.google` each received 3 of 3 replies; the phone's
address was 172.17.0.2. On the host the adb port listened on 127.0.0.1 only. Pulling
the image used 2.7 GB of the root partition.

**A-03 pass.** 2026-11-14, same laptop (test T-02). A marker file in `/sdcard` and an
installed app (F-Droid) survived `docker stop` / `docker start` (boot 11 s). With
`--restart unless-stopped`, `adb shell reboot` brought the phone back fully booted in
12 s (Docker restart count 1) with marker and app intact. After `docker stop` the
container stayed stopped for 30 s. Note for Phase 1: adb must be disconnected and
reconnected after the container is recreated or restarted; a stale connection returns
`error: closed` and the command is silently not run.

**A-03 addendum: how the phone stops.** 2026-11-15, same laptop, measured while
preparing the Phase 1 plan.

| Action | Result |
|---|---|
| `docker stop` (10 s grace) | Android ignores the request; killed after 10.4 s, exit code 137 |
| `docker stop -t 60` | killed after 60.4 s, exit code 137 |
| `reboot -p` inside, restart policy `unless-stopped` or `on-failure` | container exits and the policy starts it again |
| `docker update --restart no`, then `reboot -p` inside | exits in 0.9 s with exit code 130 and stays stopped |
| `reboot` inside, restart policy `no` | exit code 129 |
| Start again after the clean stop | boots in 11 s, marker file intact |
| `svc power shutdown` inside | container still running after 40 s; not investigated |

Design consequences: Verone never uses `docker stop`; it switches the restart policy
off, powers Android off from inside and waits for exit code 130. Exit code 129 means
"restart requested" and 130 means "power off requested"; Docker's restart policy
cannot tell them apart, so until Verone supervises the phone itself (Phase 2), a
power-off chosen inside Android comes back on.

**A-04 pass.** 2026-11-15, reference laptop (test T-09, script
`tests/phase-0/t09-gapps.sh`). The script does by hand what `redroid-script` (commit
`a4951b7`) does for `-a 14.0.0 -mtg`: it downloads
`MindTheGapps-14.0.0-x86_64-20240226.zip` (196 MB) from
`github.com/s1204IT/MindTheGappsBuilder`, checks the MD5 the tool pins
(`a827a84ccb0cf5914756e8561257ed13`, matched), copies its `system/` tree (478 MB, 16
APKs) over `redroid/redroid:14.0.0-latest`, and builds
`redroid/redroid:14.0.0_mindthegapps` locally in 38 s. The image is reported as 3.7 GB
and used about 1.1 GB more of the root partition. A phone with fresh data booted in
36 s. Play Store (30.4.17), Play Services (22.21.16) and the Services Framework were
installed, and the Play Store opened to its "Sign in" screen with no fatal errors.
The owner approved the use of this package for the test; see the risk register about
its origin. Sign-in itself is A-05.

Note for T-10: on this phone the `sqlite3` query for the device identifier aborted
before any sign-in attempt. The identifier may only exist after Play Services has
checked in, or another way of reading it is needed.

**A-07 pass.** 2026-11-15, reference laptop (test T-06). The stock image reports
`ro.product.cpu.abilist=x86_64,arm64-v8a,x86,armeabi-v7a,armeabi` and contains
`libndk_translation.so`. VLC for Android 3.7.1, arm64-v8a build from
`get.videolan.org` (checksum matched the published one; the package contains only
`lib/arm64-v8a/`), installed with `primaryCpuAbi=arm64-v8a` and opened to its welcome
screen. The log showed `Initialized NDK translation (aarch64)` and no fatal errors.
Only one app was tried, and no video was played.

**A-08 pass on Intel.** 2026-11-15, reference laptop (test T-04, script
`tests/phase-0/t04-gpu.sh`). Each run: 720×1280, Settings scrolled by synthetic swipes,
scrcpy recording for 60 s at up to 60 frames per second, container CPU sampled 5 times.

| Mode | Boot | Renderer reported by Android | Frames in 60 s | Container CPU (avg of 5) |
|---|---|---|---|---|
| `guest` (software) | 11 s | SwiftShader through ANGLE | 790 | about 118% of one core |
| `host`, Intel `/dev/dri/renderD128` | 17 s | Mesa Intel HD Graphics 530, OpenGL ES 3.2 | 1420 | about 100% of one core |
| `host`, NVIDIA `/dev/dri/renderD129` (proprietary driver) | did not boot | `GRALLOC-GBM: failed to create gbm device`; graphics service restarts in a loop | – | – |
| `auto`, no node given | 21 s | Mesa Intel HD Graphics 530 | not measured | not measured |

A screenshot in Intel mode showed the Settings screen drawn correctly. The owner has
not yet confirmed this by eye in a live window.

Reading: GPU mode nearly doubles the frame rate for slightly less CPU. CPU did not fall
sharply because the container still encodes the video stream in software; that cost is
a Phase 2 concern (A-13). On this machine `auto` chose the working Intel card.

Design consequences: default to `auto`; `check_host` lists render nodes with their
drivers, never selects a node driven by the proprietary NVIDIA driver, and falls back
to `guest` when that is the only GPU; a boot that does not complete within a time
limit is retried once in `guest` mode.

**A-09 pass with gocryptfs.** 2026-11-15, reference laptop (test T-08, script
`tests/phase-0/t08-gocryptfs.sh`). gocryptfs 2.6.1 runs as root in a privileged helper
container and serves an encrypted folder as a FUSE mount, which the phone uses as
`/data`.

| Check | Result |
|---|---|
| First boot on encrypted storage | 10 s (unencrypted: 11 to 15 s) |
| Second boot | 10 s |
| Marker file and installed app after restart | intact |
| App launch (F-Droid) | started, no fatal errors |
| 200 MiB push to shared storage / pull back | 184 MB/s / 253 MB/s, content identical (unencrypted push: 430 MB/s) |
| After lock: search of all 1981 encrypted files for the marker text | 0 matches; file and folder names are encrypted |
| Wrong password | refused: "Password incorrect" |
| Right password, boot, recheck | marker and app intact |
| Helper cost while the phone is active | about 65 MB RAM |

Findings that the design must carry:

- **The helper must share the host's process namespace** (`--pid=host`). Without it
  the first attempt booted but Android's shared storage was not writable: gocryptfs
  could not look up the calling process's groups, so group-based access failed.
- Needs only FUSE, which was built into the kernel. No kernel module had to be loaded.
- Ownership, permission bits, file sizes and the folder tree shape remain visible on
  disk; contents and names do not.
- gocryptfs logged one `Unimplemented opcode TMPFILE`; nothing visibly failed.
- The test passed the password through the container environment. The product must
  not do that.
- Not tested: power loss during writes, long heavy use, many apps, low disk space.

**Candidate 1 (LUKS disk image) is blocked, not failed.** `cryptsetup luksFormat`
succeeded, but opening the image needs the `dm_crypt` kernel module, which is not
loaded on the reference machine and needs root to load. This is a real cost of that
method: on machines that do not already use disk encryption, Verone would have to load
a kernel module. To finish the comparison the owner runs `sudo modprobe dm_crypt`, then
`tests/phase-0/t08-luks.sh` can be completed.

**A-12 pass on Arch.** Same machine: kernel has `CONFIG_ANDROID_BINDER_IPC_RUST=y` and
the `binder` filesystem; redroid created its binder devices without any extra module.

**A-12 pass by inspection.** 2026-11-15 (test T-13). Method differed from the test
file: instead of downloading kernel packages, the distributions' published package
contents and kernel configuration sources were read. No phone was booted on these
systems.

| Distribution | Finding | Source |
|---|---|---|
| Debian 13 (trixie) | Module `binder_linux.ko.xz` is in the standard `linux-image-6.12.*-amd64` package, so it is present in a default install. | packages.debian.org file search |
| Ubuntu 24.04 (noble) | Module `binder_linux.ko.zst` is in `linux-modules-extra-<version>-generic`. Desktop installs include that package; minimal, server and cloud images may not. Cloud kernels (aws, azure, gcp, oracle) ship it in their own `linux-modules-extra-*` package. | packages.ubuntu.com file search; kernel annotations: `CONFIG_ANDROID_BINDER_IPC` = `m`, `CONFIG_ANDROID_BINDERFS` = `m` |
| Fedora (development kernel) | Built in on x86: `CONFIG_ANDROID_BINDER_IPC_RUST=y`, `CONFIG_ANDROID_BINDERFS=y`, devices `binder,hwbinder,vndbinder`. The older C driver is not set. Released Fedora versions were not checked one by one. | kernel-ark `redhat/configs/fedora/generic` |
| Arch | Built in (Rust driver), verified by booting. | reference machine |

Consequences for `verone doctor`: accept either the C or the Rust binder driver; on
Ubuntu, name `linux-modules-extra-$(uname -r)` as the fix; load `binder_linux` when it
is a module.

**A-14 pass.** 2026-11-15, reference laptop (test T-03, scripts
`tests/phase-0/t03-push.sh` and `t03-stream.sh`). Phone started with no published port.

| Method | 200 MiB `adb push` | 10-minute scrcpy stream | Host listener |
|---|---|---|---|
| A: container address (`172.17.0.2:5555`) | 430 MB/s | completed, 7971 frames | none |
| B: socat bridge through `docker exec` | 21 MB/s | completed, 7946 frames | `127.0.0.1:5555` |
| Baseline: published `127.0.0.1:5555` | 402 MB/s | not run | `127.0.0.1:5555` |

The stream was recorded to a file with synthetic swipes on a software-rendered phone
(about 13 frames per second). Reachability from a second device on the local network
was not tested; no method opened a listener on a network interface.

Design consequence: Phase 1 uses method A (container address) on Linux. Method B is
kept as the fallback for setups where the container address is not reachable from the
host, such as a runtime inside a virtual machine.

**A-15 fail.** 2026-11-15, reference laptop (test T-12). The Google apps image was
started with fresh data and `--entrypoint /init … androidboot.hardware=redroid`, which
leaves out the tool's `ro.setupwizard.mode=DISABLED`. Android booted in 38 s with
`device_provisioned=0` and Google's `WelcomeActivity` in front, but the screen stayed
black: the setup wizard crashed and restarted continuously (196 crashes in about a
minute) with

    java.lang.SecurityException: WifiService: Permission denied
        at android.net.wifi.WifiManager.getCurrentNetwork

The package manager also logged "There should probably be exactly one setup wizard;
found 2". Reading: the wizard is refused a privileged Wi-Fi call on this image, which
is presumably why the community tool switches the wizard off. Not yet tried: granting
the missing permission through an extra file in the image layer. Until that works, a
phone-like first boot is not available from a prebuilt image.

Design consequence: Phase 3 cannot promise the Google first-boot wizard. Options, in
order of cost: (1) try a permissions file added to the local image; (2) Verone's
desktop app provides its own first-run steps (language, optional Google sign-in,
screen lock) and starts Android already provisioned; (3) solve it in Verone's own
image in Phase 5.

**A-17 pass.** 2026-11-16, owner's laptop: Arch Linux, kernel 7.3.4-arch1-2, Intel Core
i5-6500T, 7.6 GB RAM, `/dev/kvm` usable. Benchmark task BM-01.

- Installed without Google's command-line tools or Java: the emulator package
  `emulator-linux_x64-16740729.zip` (emulator 37.2.13, stable channel) and the system
  image `x86_64-34_r04.zip` (`system-images;android-34;default;x86_64`, no Google apps)
  were downloaded from `dl.google.com/android/repository/`, their SHA-1 checked against
  Google's catalogue, and unpacked with `bsdtar` into `~/Android/Sdk`. `adb` is linked
  from the system package. The phone `bench34` was written by hand to
  `~/.android/avd/` (2 cores, 2048 MB, 720×1280 at 320 dpi, GPU `auto`, emulated
  cameras), because `avdmanager` needs Java.
- Started with `emulator -avd bench34 -no-window -no-snapshot-save -no-boot-anim
  -port 5560`, with the phone 'work' stopped. First boot to `sys.boot_completed=1`: 37 s.
- Over `adb` these benchmark checks ran and gave a result: build properties, setup
  state, declared features, a posted notification, battery, cameras (2), sensors
  (accelerometer, gyroscope, proximity), network type, internet ping, rotation, app
  launch time (`am start -W`: Settings 1019 ms), `monkey`, and a 200 MiB push
  (213 MB/s as reported by `adb`).
- Notes for the check runner (BM-03): rotation must be set with
  `cmd window user-rotation lock 1` and read from `mCurrentOrientation` in
  `dumpsys display`; changing `user_rotation` in settings did nothing. The network type
  is in `dumpsys connectivity` as `Transports:`; the emulator shows WIFI and also
  CELLULAR. Use a fresh data partition (`-wipe-data`) for every benchmark run, because
  the checks change settings.
- Early observations, not yet scored: no setup wizard (`user_setup_complete=1` on first
  boot); `userdebug` build with test keys; model "Android SDK built for x86_64" and
  `ro.kernel.qemu=1`, so it identifies itself honestly as an emulator; it declares
  telephony. On the host the emulator used about 3.3 GB of memory (resident) and the
  phone's files took 678 MB.

**A-18 untested.** 2026-11-16: skipped on the owner's laptop by the owner's choice. It
has 7.6 GB of memory and runs Arch, while Google supports Cuttlefish hosts on
Debian-based systems only. To be tested on a larger or rented Debian machine.

**A-19 untested.** 2026-11-16: not attempted. Waydroid itself is in Arch's `extra`
repository (version 1.6.3), but its first-time setup downloads an Android image (about
1 GB) to the system partition, which had only 2.7 GB free. The owner did not approve
it for now. Free space on `/` first, or find a way to keep Waydroid's images on `/home`.

**A-20 untested.** 2026-11-16: not attempted. Google's Android 14 test suite,
`android-cts-14_r14-linux_x86-x86.zip`, is 10.7 GiB (size read from
`dl.google.com`). It also needs Java. Waiting for the owner's approval of the download.
Since set A uses Android 12, the Android 12 suite is needed first; the Android 14 suite
is only for set B. Look up the size of the Android 12 suite before asking.

**A-22 pass, with a limit.** 2026-11-16, owner's laptop (as for A-17). The image
`redroid/redroid@sha256:52332b2d74f337982d5ac281a8020ec297fb1ea05cbdcdaaa9c19a2065ae1adc`
(tag `12.0.0-latest`) is 2.78 GB once pulled. To make room on `/`, the Phase 0 images
`redroid/redroid:14.0.0_mindthegapps`, `verone-p0-gcfs` and `verone-p0-crypt` were
deleted with the owner's approval (they can be rebuilt from `tests/phase-0/`), and the
owner trimmed the system journal; `/` had 1.4 GB free afterwards.

- Plain redroid with the Phase 0 baseline command (software rendering, 720×1280):
  `sys.boot_completed=1` after 7 s, then the Launcher3 home screen; Android 12 (API 31),
  `userdebug`, ARM translation listed (`arm64-v8a`, `armeabi-v7a`); internet ping
  works; about 717 MB memory and under 1% CPU when idle; `user_setup_complete=1` once
  settled, no setup wizard.
- Through Verone, built from this branch: `verone create bench12 --android 12` and
  `verone start bench12` booted in 4.7 to 7.0 s using the GPU. A file written to
  shared storage survived stops and starts.
- **Shared storage is ready a few seconds after boot.** Writing to `/sdcard` right
  after `sys.boot_completed=1` failed with "Operation not permitted"; a few seconds
  later it worked. Scripts must wait for storage, not only for boot.
- **Restart and power-off from inside Android do not end the container.** After
  `reboot` or `reboot -p` inside, `sys.powerctl` is set (`reboot,shell`) but the
  container keeps running for at least 60 s, with restart count 0. Android 14 exits
  with code 129 or 130 instead (A-03 addendum). Effects on Verone with Android 12:
  `verone stop` waits its full 20 s and then kills the phone (exit code 137); a restart
  chosen inside Android is ignored and the phone keeps running normally (same system
  process, home screen and storage still working, checked 2026-11-17). Data survived
  three such kills.
- Cause, as far as found: the kernel log shows init receiving the request
  ("Got shutdown_command 'reboot,shell' Calling HandlePowerctlMessage()") and then
  logging nothing more; it never starts shutting down. The deeper reason inside
  Android 12's init was not found. A fix that does not depend on it is designed in
  `design/android-12-power.md`.

**A-21 partial.** 2026-11-16: the owner's phone, a vivo V2027, connected over USB with
USB debugging and was reached by `adb` without problems. It runs **Android 12** (API
31), not 14: a `user` build with release keys, ARM 64-bit, 720×1600 screen. Only
read-only checks were run, because it is the owner's personal phone. All gave results:
build properties, declared features (camera, front camera, telephony, Wi-Fi,
Bluetooth, GPS, accelerometer, gyroscope, proximity), battery, cameras (5), sensors,
network type (mobile data and Wi-Fi), screen orientation, and setup state. `monkey`,
`cmd` and `am` are present. No root (`adb shell` runs as the `shell` user), as expected
on a retail phone.

Consequence for the benchmark: the reference phone is two Android versions older than
the virtual contenders. Feature and behaviour checks can still be compared, but
Google's compatibility tests (B3-03) are made per Android version and cannot run the
same suite on Android 12 and 14. Options for the owner: (1) also run the virtual
contenders on Android 12 (redroid and the emulator both offer it) for a like-for-like
comparison; (2) keep Android 14 and record the version gap next to every result;
(3) use an Android 14 phone as the reference. Checks that change settings, post
notifications or copy files must ask the owner first when run on this phone.

## Other Phase 0 findings

**T-14, name check (2026-11-15): no conflict found, formal trademark search not done.**

- Package names: `verone` is unused on crates.io, npm, PyPI, the AUR and Flathub.
- GitHub: about a dozen repositories named "verone", all personal or unrelated (a
  WordPress theme, HTML exercises), none with more than a few stars.
- Domains: `verone.org` resolves to a server, so it is taken. `verone.dev`,
  `verone.app` and `verone.io` have no DNS record; that does not prove they are
  unregistered, so check with a registrar before relying on one.
- Trademarks: a web search found no software mark spelled "Verone". Similar marks
  exist in the computing class: "Veroneus", "Verona", "Vero" and "Verosoft".
  Microsoft Research also has a programming-language project named "Verona".
- Not done: searches in the USPTO, EUIPO, WIPO and Indian registers themselves. These
  need their interactive search pages and should be done by the owner before any
  public launch or logo work.

## Known risks

| Risk | Effect | Response |
|---|---|---|
| Google does not accept honestly identified virtual devices for sign-in or prompts. | "With Google" mode is limited or unavailable. | D-006 and D-014: Google-free mode is fully supported; limits are documented. |
| redroid upstream slows or stops. Its documentation repository showed little recent activity when checked. | No new Android versions or fixes. | Engine interface (D-002) allows a second engine; own image build in Phase 5 reduces dependence. |
| Privileged container on the user's machine. | A malicious app inside the phone has a shorter path to the host than on a real phone. | State it plainly; never expose adb; evaluate reduced privileges; offer the VM engine for untrusted workloads. |
| The host can always read the guest. | Verone lock cannot defend against a compromised computer. | Security statement in `vision-and-scope.md`; repeat it in the app. |
| Proprietary components fetched on the user's machine change or disappear. | "With Google" setup breaks. | Pin versions and checksums; fail with a clear message; keep Google-free mode independent. |
| No official x86-64 Google apps package exists. The MindTheGapps project publishes ARM builds only; the x86-64 package that the community tool `redroid-script` pins comes from an individual's repository (`s1204IT/MindTheGappsBuilder`, last built 2024-02-26). | "With Google" mode on x86-64 would depend on a low-profile third party for software that runs with system privileges. | Phase 0 uses that package in a throwaway phone only, with the owner's approval. Before Phase 3, decide on a better-trusted route: build the package on the user's machine from the official MindTheGapps source, or pin and audit a specific build. |
| `redroid-script` builds its images with the setup wizard switched off. (Correction, 2026-11-15: the tool is MIT-licensed; an earlier version of this file wrongly said it had no licence.) | Its default output does not give a phone-like first boot. | Its code may be reused with attribution, but Verone writes its own installer in Phase 3 and enables the wizard. |
| Kernel features differ between distributions. | Verone does not start on some systems. | `check_host` with specific guidance; A-12 covers the main distributions. |
| One maintainer. | Slow progress, no review. | Small phases with exit tests; everything scripted and documented so contributors can join. |
| Low-memory hosts (8 GB or less). | Poor experience next to a browser and IDE. | Measure in every phase on the owner's 8 GB laptop; expose screen-size and memory options. |
