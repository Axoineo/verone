# Vision and scope

Status: **approved baseline** (2026-11-14). Changes to this file need the owner's approval.

## What Verone is

Verone is an open-source virtual Android phone that runs as an ordinary application on a
PC, laptop or server. A new Verone starts like a new phone: first-boot setup, accounts,
apps. When it is stopped and started again it comes back exactly as it was left, like a
phone that was switched off and on.

It is a real Android system, not a skinned emulator: no game overlays, no ads, no
modified app store. The experience to aim for is "a phone connected over USB and shown
with scrcpy", except that the phone lives inside the computer.

## Who it is for

| Audience | What they get |
|---|---|
| Everyday users | A second (or first) Android phone on the computer: Play Store or Google-free, messaging and authenticator apps, a persistent phone they can lock, back up and restore. |
| App developers | A fast, light Android target that shows up in `adb` and the IDE like a real device; several Android versions and screen sizes; clean phones on demand. |
| Testers and QA | Reproducible phones: snapshots, clones, reset to factory, scripted setup, headless runs in CI. |
| Security researchers | A disposable Android to inspect and attack **their own** apps and setups: root on demand, traffic inspection, snapshots before and after. Fidelity limits per engine are documented in `architecture.md`. |
| Enthusiasts | Try new Android versions and Google-free setups without flashing a device. |
| Self-hosters and teams | Phones that run on their own server and are reached from a browser or the desktop app. |

## Product principles

These settle design arguments. Earlier ones win over later ones.

1. **Behave like a phone.** If a real phone does it (setup wizard, lock screen, restart,
   notifications, camera, rotation), Verone should do it or clearly say why it cannot.
2. **Honest identity.** Verone never claims to be a certified retail device and never
   fakes integrity or attestation results. See non-goals.
3. **Reuse before build.** Use maintained open-source components. Verone's own code is
   the glue and the parts that do not exist yet.
4. **Light package, system dependencies.** The Verone package contains only Verone.
   Large or shared components (container runtime, Android image, optional Google apps)
   are installed or downloaded on the user's system, on demand.
5. **Adapt to the machine.** Detect what the host offers (GPU, RAM, kernel features,
   webcam) and degrade gracefully instead of refusing to run.
6. **Local by default.** Nothing is reachable from the network unless the user turns it on.
7. **Many uses from one core.** The desktop app, the command line and the server are
   front ends over the same engine layer, so every feature is scriptable.

## In scope

- Linux desktop application (first target).
- Command-line tool with the same capabilities as the app.
- Self-hosted server edition: the owner or their team runs phones on their own server.
- Windows application (later; see `roadmap.md`).
- Two engines behind one interface: a container engine (light) first, a virtual-machine
  engine (full fidelity) later.
- Optional Google apps, installed on the user's machine at the user's request.
- Verone's own lock: encrypted phone storage, separate from the computer's login.

## Non-goals

- **Device identity spoofing.** No copied retail fingerprints, no Play Integrity or
  key-attestation bypass, no leaked keyboxes. Not in the code, images, docs or issues.
- **Guaranteeing apps that demand certified hardware.** Banking, payment and some
  streaming apps may refuse to run. Verone documents this; it does not work around it.
- **Guaranteeing Google sign-in prompts.** Whether Google treats a Verone as a trusted
  prompt device is Google's decision. It is tested in Phase 0 and documented either way.
- **Public hosting for strangers.** The server edition is self-hosted. Multi-tenant
  isolation, billing and abuse controls are out of scope.
- **Account farming or automation at scale against third-party services.**
- **Gaming performance.** Key mapping, macros and frame-rate tuning are not targets.
- **Shipping proprietary software.** Google apps and other proprietary components are
  never included in Verone's packages or images.

## Honest security statement

Verone's lock protects a phone when the computer is lost, stolen or used by someone
else, because the phone's storage is encrypted with a key that only the Verone password
unlocks. It does **not** protect against malware or an attacker already controlling the
computer while the phone is unlocked: the host can always read the guest. A Verone used
as a second factor on the same computer is therefore weaker than a separate physical
phone, and users must be told this in the app and the user guide.

## What success looks like

- A new user on a supported Linux system goes from install to a booted phone in under
  five minutes without typing a command.
- A stopped phone starts again with all apps and accounts intact, every time.
- A developer can use Verone as their daily `adb` target.
- Every limitation is written down where users will see it.
