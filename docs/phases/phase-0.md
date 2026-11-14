# Phase 0 — Proof

Status: **ready for owner review** (2026-11-14).

Purpose: test every assumption in `../assumptions.md` on a real machine before any
product code is written. Phase 0 produces measurements and decisions, not software.

## Rules for this phase

1. Run the tests in order. Later tests reuse what earlier tests set up.
2. Each test has an owner: **Agent** (can be run by Claude Code) or **Owner** (needs
   sudo, a Google account, or a person to look and listen).
3. Commands marked *verified* ran successfully on the reference machine on the date
   given. Commands marked *starting point* are the designer's best first attempt and may
   need adjusting; record what was actually run.
4. After each test, update the matching row and the Evidence section of
   `../assumptions.md`: result, date, machine, the exact command, the observed output.
5. A failed test is a useful result. Record it and continue with the next test unless
   the test says "stop".
6. Agents never type passwords or sign in to accounts. Those steps belong to the owner.
7. Scripts written for these tests go in `tests/phase-0/` in the repository.

## Reference machine

Owner's laptop: Arch Linux, kernel 7.3.4, Docker 29.9.3 with containerd 2.4.2,
Intel HD 530 plus NVIDIA T400, 8 GB RAM, 4 cores, scrcpy 5.0.1, no webcam.

Two facts about this machine that shape the steps:

- Docker here stores images through containerd in `/var/lib/containerd`, on the root
  partition (32 GB, 2.6 GB free on 2026-11-14). Changing Docker's `data-root` would not
  move them. The practical fix is to free space on the root partition.
- Phone data does not need root-partition space; it lives under `/home`.

## Conventions

```bash
export P0="$HOME/.local/share/verone-phase0"
mkdir -p "$P0"
```

- Containers are named `verone-p0` (one at a time).
- Android creates files owned by root inside `$P0`. To delete them without sudo, use a
  container (*verified* pattern, 2026-11-14):

```bash
docker run --rm -v "$HOME/.local/share:/mnt" alpine:3 rm -rf /mnt/verone-phase0
```

- Wait for boot (*verified* 2026-11-13):

```bash
until [ "$(docker exec verone-p0 getprop sys.boot_completed 2>/dev/null)" = 1 ]; do sleep 2; done
```

- Baseline start command (*verified* 2026-11-14):

```bash
docker run -d --privileged --name verone-p0 \
  -v "$P0/data:/data" -p 127.0.0.1:5555:5555 \
  redroid/redroid:14.0.0-latest \
  androidboot.redroid_gpu_mode=guest \
  androidboot.redroid_width=720 androidboot.redroid_height=1280 androidboot.redroid_dpi=320
```

- Stop and remove: `docker rm -f verone-p0`. This keeps the data in `$P0/data`.

---

## Step 0 — Owner preparation

Run in a normal terminal (the Claude app's shell cannot use sudo).

| # | Action | Command or note |
|---|---|---|
| 0.1 | Free space on the root partition. The package cache holds 5.7 GB. | `sudo pacman -Sc` then `df -h /` |
| 0.2 | Confirm at least 8 GB free on `/`. If not, clear the whole cache. | `sudo pacman -Scc` |
| 0.3 | Install tools the tests need. | `sudo pacman -S --needed lzip podman rsync` |
| 0.4 | Confirm the running kernel matches the installed one. If the two differ, reboot. | `uname -r` and `pacman -Q linux` |
| 0.5 | Create a new Google account used only for these tests. | Never use a real account in Phase 0. |

Why 8 GB: the Android image is about 3 GB, the Google apps variant adds to it, and the
root partition must keep some free space for the system.

Exit: `df -h /` shows 8 GB or more available. If this cannot be reached, stop and tell
the designer.

---

## T-01 — Internet inside the phone (A-02) · Agent

1. Start the phone with the baseline command and wait for boot.
2. `adb connect 127.0.0.1:5555`
3. `adb -s 127.0.0.1:5555 shell ping -c 3 8.8.8.8`
4. `adb -s 127.0.0.1:5555 shell ping -c 3 dns.google`

Pass: both pings receive replies (the second proves name resolution).
If the container fails to start with a network error, check step 0.4 first.

## T-02 — Data survives; restart returns (A-03) · Agent

1. With the phone from T-01 running, write a marker and install an app:

```bash
adb -s 127.0.0.1:5555 shell 'echo verone-marker > /sdcard/marker.txt'
curl -L -o "$P0/fdroid.apk" https://f-droid.org/F-Droid.apk
adb -s 127.0.0.1:5555 install "$P0/fdroid.apk"
```

2. `docker stop verone-p0 && docker start verone-p0`, wait for boot, reconnect adb.
3. Check: `adb shell cat /sdcard/marker.txt` prints `verone-marker`, and
   `adb shell pm list packages | grep fdroid` lists the app.
4. Remove the container (`docker rm -f verone-p0`) and start it again with the baseline
   command plus `--restart unless-stopped` placed before `--name`.
5. `adb -s 127.0.0.1:5555 shell reboot`. Time how long until boot completes again.
6. Repeat the checks from step 3.
7. `docker stop verone-p0`; wait 30 s; confirm with `docker ps` that it stays stopped.

Pass: marker and app survive both the stop/start and the in-Android restart; the phone
returns by itself after step 5; a manual stop is respected in step 7.
Record: seconds to return after `reboot`.

## T-03 — adb without a published port (A-14) · Agent

Two methods to compare. For both, start the phone with the baseline command **without**
the `-p` option.

Method A, container address:

```bash
IP=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' verone-p0)
adb connect "$IP:5555"
```

Method B, bridge through the runtime (*verified* to connect, 2026-11-13):

```bash
socat TCP-LISTEN:5555,bind=127.0.0.1,reuseaddr,fork \
  EXEC:"docker exec -i verone-p0 nc 127.0.0.1 5555" &
adb connect 127.0.0.1:5555
```

For each method, and once more with the published port as the baseline:

1. `head -c 200M /dev/urandom > "$P0/blob"` then time `adb push "$P0/blob" /sdcard/`.
2. Run `scrcpy -s <address> --no-audio` for 10 minutes while using the phone.

Pass: at least one method keeps scrcpy connected for 10 minutes and pushes at 10 MB/s
or faster. Record speeds for all three, and whether the phone is reachable from another
device on the local network for each (it must not be).

## T-04 — GPU acceleration (A-08) · Agent, Owner to look

1. Find the Intel render node: `ls -l /dev/dri/by-path/` (the entry for `00:02.0`).
2. Start the phone with `androidboot.redroid_gpu_mode=host` and
   `androidboot.redroid_gpu_node=/dev/dri/renderD12X` (the node from step 1) in place of
   `androidboot.redroid_gpu_mode=guest`.
3. After boot: `adb shell dumpsys SurfaceFlinger | grep -i GLES`.
4. Open scrcpy, scroll through Settings for one minute, and record
   `docker stats --no-stream verone-p0`.
5. Repeat steps 2 to 4 with the NVIDIA node and record what happens.

Pass: the renderer string names Intel hardware (not SwiftShader or llvmpipe), the
screen draws correctly (owner confirms), and CPU while streaming is clearly below the
software-rendering figure of about 130% of one core.
If the phone does not boot in `host` mode, collect `docker logs verone-p0` and the
output of `adb logcat -d | grep -iE "gralloc|egl|hwcomposer"`.

## T-05 — Sound (A-10) · Owner to listen

1. Phone running; start `scrcpy -s 127.0.0.1:5555` (without `--no-audio`).
2. Read scrcpy's output: it states whether audio capture started.
3. In the phone: Settings → Sound → tap a ringtone or alarm sound.

Pass: the sound is heard from the computer. Record scrcpy's audio lines either way.

## T-06 — ARM-only app (A-07) · Agent

1. `adb shell getprop ro.product.cpu.abilist` and record it.
2. Obtain an APK built only for `arm64-v8a` from its developer's own site (for example
   the arm64-v8a build of VLC for Android from `get.videolan.org/vlc-android/`).
   Download it into its own empty folder under `$P0`.
3. Confirm it is ARM-only: `bsdtar -tf app.apk | grep '^lib/'` shows only `lib/arm64-v8a/`.
4. `adb install app.apk`, then start it with
   `adb shell am start -W -n <package>/<launcher activity>`.

Pass: installation succeeds and the app reaches its main screen without crashing.
A failure with `INSTALL_FAILED_NO_MATCHING_ABIS` means the translation layer is not
active; record the property list from step 1.

## T-07 — Podman (A-11) · Agent, Owner for the rootful run

1. Rootless: run the baseline command with `podman` in place of `docker`. Record the
   result; failure is expected to be likely.
2. Rootful (Owner, normal terminal): the same with `sudo podman`.
3. If either boots, repeat T-01.

Pass: the phone boots and has internet under at least one Podman mode. This test is
informational; a fail removes Podman from Phase 1, nothing more.

## T-08 — Encrypted phone storage (A-09) · Agent

Goal: Android's data sits on storage that is unreadable without a password.

Status 2026-11-15: candidate 2 passed using `tests/phase-0/t08-gocryptfs.sh`
(*verified*; the helper needs `--pid=host`). Candidate 1 is waiting for the owner to
run `sudo modprobe dm_crypt`; its script is `tests/phase-0/t08-luks.sh`. The commands
below are the original starting points, kept for reference.

**Candidate 1 — encrypted disk image opened by a helper container** (*starting point*)

```bash
mkdir -p "$P0/enc/mnt" && truncate -s 8G "$P0/enc/data.img"
docker run --rm -it --privileged -v /dev:/dev -v "$P0/enc:/work:rshared" alpine:3 sh -c '
  apk add --no-cache cryptsetup e2fsprogs &&
  cryptsetup luksFormat /work/data.img &&
  cryptsetup open /work/data.img verone-p0 &&
  mkfs.ext4 -q /dev/mapper/verone-p0 &&
  mount /dev/mapper/verone-p0 /work/mnt'
```

Then start the phone with `-v "$P0/enc/mnt:/data"` in place of the usual data option.
To lock: stop the phone, then in a helper container `umount /work/mnt` and
`cryptsetup close verone-p0`.

**Candidate 2 — gocryptfs run as root inside a helper container** (*starting point*):
same idea, with a gocryptfs mount at `$P0/enc/mnt` in place of the disk image.

For the candidate that works:

1. Boot, repeat the marker and app steps of T-02, stop and lock.
2. `grep -a -c verone-marker "$P0/enc/data.img"` must print `0`.
3. Try to open with a wrong password; it must be refused.
4. Unlock with the right password, boot, and confirm marker and app are present.
5. Record boot time compared with T-01, and the size of the image file on disk
   (`du -h`), since the image is created sparse.

Pass: steps 2 to 4 succeed and boot time is at most 25% slower than unencrypted.
If both candidates fail, stop and report to the designer; native filesystem encryption
(fscrypt) is the next option and needs owner approval because it changes a setting on
the `/home` filesystem.

## T-09 — Add Google apps locally (A-04) · Agent

Based on the community tool `redroid-script` (MIT licence).

Status 2026-11-15: run with `tests/phase-0/t09-gapps.sh`, which performs the same
steps as the tool for `-a 14.0.0 -mtg` without running it (the tool needs a Python
module that is not installed and unpacks into `/tmp`). The package it downloads comes
from an individual's repository, not from the MindTheGapps project; the owner approved
its use for this test. The numbered steps below describe the original plan.

1. Clone it into its own folder and record the commit:

```bash
git clone https://github.com/ayasa520/redroid-script "$P0/redroid-script"
git -C "$P0/redroid-script" rev-parse HEAD
```

2. Read `redroid.py` and the `stuffs/` folder before running, and record every address
   it downloads from.
3. Build: `cd "$P0/redroid-script" && python redroid.py -a 14.0.0 -mtg`
   The expected result is an image tagged `redroid/redroid:14.0.0_mindthegapps`.
4. Start a phone from that image with a **new, empty** data folder (`$P0/data-g`).
5. Confirm the Play Store is present: `adb shell pm list packages | grep com.android.vending`,
   then open it.

Pass: the image builds, the phone boots, and the Play Store opens to its sign-in screen.
Record: the size of the new image and the build time.

## T-10 — Registration and Google sign-in (A-05) · Owner, with the test account

1. Agent prepares the device identifier (*starting point*, from the tool's notes):

```bash
adb -s 127.0.0.1:5555 root
adb -s 127.0.0.1:5555 shell 'sqlite3 /data/data/com.google.android.gsf/databases/gservices.db \
  "select * from main where name = \"android_id\";"'
```

2. Owner opens `https://www.google.com/android/uncertified/` in their own browser,
   signs in with the test account and registers the identifier.
3. Restart the phone. Owner signs in to the Play Store with the test account.
4. Owner installs one free app from the Play Store.

Pass: sign-in completes and the app installs. Record how long after registration
sign-in started to work, and any warning shown.

## T-11 — Sign-in prompt (A-06) · Owner, informational

1. Set a screen lock: `adb shell locksettings set-pin 1234`.
2. Owner turns on 2-Step Verification for the test account and checks whether the
   Verone phone is listed as a device for Google prompts.
3. Owner signs in to the test account in a private browser window and watches the phone.

Record one of: prompt arrives and works; device listed but no prompt; device not offered
for prompts. Whatever the result, the project continues (decision D-006).

## T-12 — First-boot setup wizard (A-15) · Agent, Owner to look

The tool in T-09 builds its image with the setup wizard switched off. This test starts
the same image without that switch.

Status 2026-11-15: **failed**. The start command below works, but the wizard crashes in
a loop and the screen stays black. Details and options are in `../assumptions.md` (A-15).

1. New, empty data folder (`$P0/data-w`).
2. Start (*starting point*):

```bash
docker run -d --privileged --name verone-p0 \
  -v "$P0/data-w:/data" -p 127.0.0.1:5555:5555 \
  --entrypoint /init redroid/redroid:14.0.0_mindthegapps \
  androidboot.hardware=redroid \
  androidboot.redroid_width=720 androidboot.redroid_height=1280 androidboot.redroid_dpi=320
```

Pass: the first screen is the welcome and language wizard, and it can be completed to
the home screen (skipping sign-in is acceptable).

## T-13 — Binder on other distributions (A-12) · Agent

For current Ubuntu LTS, Debian stable and Fedora:

1. Download the distribution's standard kernel package into its own empty folder and
   extract it without installing (`bsdtar -xf`).
2. In the kernel config file inside it, read the lines containing `ANDROID_BINDER`.
3. If binder is a module, find which package contains `binder_linux.ko`.

Record for each distribution one of: built in; module in the default install; module
in an extra package (name it); not available.

Pass: binder is available on all three without building a kernel. Optional
confirmation, if a spare machine or virtual machine is available: repeat T-01 there.

## T-14 — Is the name free? · Agent

Search for "Verone" used for software in: the USPTO, EUIPO, WIPO Global Brand Database
and Indian trademark registers; GitHub; crates.io; npm; PyPI; AUR; Flathub; and the
`.org`, `.dev`, `.app` and `.io` domains.

Record what was found. Any registered software trademark or established project with
the same name goes to the owner for a decision.

---

## Clean-up

```bash
docker rm -f verone-p0
docker run --rm -v "$HOME/.local/share:/mnt" alpine:3 rm -rf /mnt/verone-phase0
docker image rm redroid/redroid:14.0.0_mindthegapps
```

Keep `redroid/redroid:14.0.0-latest` if Phase 1 follows soon.

## Exit review

When every test has a recorded result, the designer and the owner review
`../assumptions.md` together:

- Blocking for Phase 1: A-01, A-02, A-03, A-12 must be **pass**.
- A-09 must pass, or an alternative must be approved, before Phase 3.
- A-04, A-05, A-15 decide how much the "with Google" mode can promise in Phase 3.
- Every measurement feeds the detailed design of Phase 1, written next as `phase-1.md`.
