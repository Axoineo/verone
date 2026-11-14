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
| A-02 | The container gets working internet through the runtime's normal network. | Phase 1 | yes | untested |
| A-03 | Data survives stop and start. A restart requested inside Android comes back automatically with a restart policy. | Phase 1 | yes | untested |
| A-04 | Google apps can be added to a prebuilt image entirely on the user's machine, and the Play Store opens. | Phase 3 | no | untested |
| A-05 | Google sign-in works after the user registers the device on Google's uncertified-device page. | Phase 3 | no | untested |
| A-06 | Google sends "Yes, it's me" sign-in prompts to a signed-in Verone. Informational: see D-006. | none | no | untested |
| A-07 | An app that ships only ARM code installs and runs on an x86-64 host using the translation layer already in the image. | Phase 2 | no | untested |
| A-08 | GPU-accelerated rendering (`host` mode) works on Intel graphics. Behaviour on NVIDIA and AMD is recorded. | Phase 2 | no | untested |
| A-09 | Phone data can be stored encrypted, unlocked only by a password, with Android running normally on top. | Phase 3 | yes | untested |
| A-10 | Phone audio plays on the computer. | Phase 2 | no | untested |
| A-11 | The container engine also works with Podman. | Phase 1 | no | untested |
| A-12 | Kernel binder support is available, built in or as a module, on current Ubuntu LTS, Debian stable, Fedora and Arch. | Phase 1 | yes | **pass** on Arch only |
| A-13 | The phone's screen can be shown inside the Tauri app window with acceptable latency. | Phase 2 | yes | untested |
| A-14 | adb can be reached without publishing any port, fast enough for screen streaming. | Phase 1 | no | **partial** 2026-11-13 |
| A-15 | With Google apps added, a fresh phone starts with the standard first-boot setup wizard. | Phase 3 | no | untested |
| A-16 | The computer's webcam can be presented to Android as its camera on the container engine with a prebuilt image. | Phase 3 | no | untested |

## Evidence

**A-01 pass.** 2026-11-13, owner's laptop: Arch Linux, kernel 7.2.14, Docker 29.9.3,
Intel HD 530 + NVIDIA T400, 8 GB RAM, 4 cores. Image `redroid/redroid:14.0.0-latest`,
software rendering, 720×1280. Boot completed in 23 s; 860 MB RAM; 3.5% CPU idle; about
130% of one core while streaming to scrcpy with software rendering.

**A-12 pass on Arch.** Same machine: kernel has `CONFIG_ANDROID_BINDER_IPC_RUST=y` and
the `binder` filesystem; redroid created its binder devices without any extra module.
Other distributions untested.

**A-14 partial.** Same machine: with the container on no network, adb was bridged
through the container runtime to `127.0.0.1` and scrcpy displayed the phone. Throughput
and stability were not measured.

## Known risks

| Risk | Effect | Response |
|---|---|---|
| Google does not accept honestly identified virtual devices for sign-in or prompts. | "With Google" mode is limited or unavailable. | D-006 and D-014: Google-free mode is fully supported; limits are documented. |
| redroid upstream slows or stops. Its documentation repository showed little recent activity when checked. | No new Android versions or fixes. | Engine interface (D-002) allows a second engine; own image build in Phase 5 reduces dependence. |
| Privileged container on the user's machine. | A malicious app inside the phone has a shorter path to the host than on a real phone. | State it plainly; never expose adb; evaluate reduced privileges; offer the VM engine for untrusted workloads. |
| The host can always read the guest. | Verone lock cannot defend against a compromised computer. | Security statement in `vision-and-scope.md`; repeat it in the app. |
| Proprietary components fetched on the user's machine change or disappear. | "With Google" setup breaks. | Pin versions and checksums; fail with a clear message; keep Google-free mode independent. |
| Kernel features differ between distributions. | Verone does not start on some systems. | `check_host` with specific guidance; A-12 covers the main distributions. |
| One maintainer. | Slow progress, no review. | Small phases with exit tests; everything scripted and documented so contributors can join. |
| Low-memory hosts (8 GB or less). | Poor experience next to a browser and IDE. | Measure in every phase on the owner's 8 GB laptop; expose screen-size and memory options. |
