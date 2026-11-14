# Future ideas

Ideas that would make Verone more useful but are not scheduled. Nothing here is a
commitment. An idea enters `roadmap.md` only after the owner approves it; until then
agents must not build it.

Size: **S** fits inside an existing phase, **M** is a noticeable addition to a phase,
**L** is close to a phase of its own.
"Earliest" is the first phase whose foundations the idea needs.

## Recommended by the designer

These three add whole new groups of users for comparatively little work, because they
reuse parts the roadmap already builds.

| ID | Idea | Who benefits | Size | Earliest |
|---|---|---|---|---|
| F-01 | **AI-agent control.** Expose the Phase 4 local API as a standard tool interface (MCP) so AI agents can drive a phone: tap, type, read the screen, install apps. Aimed at automated testing of one's own apps. | Developers, testers, AI builders | M | Phase 4 |
| F-02 | **Move a phone between machines.** Use the encrypted backup format to send a phone from laptop to server and back, so the same phone is used at the desk and remotely. | Everyday users, self-hosters | M | Phase 6 |
| F-03 | **ARM64 hosts.** Run on ARM computers (Raspberry Pi 5, ARM laptops, ARM cloud servers), where Android apps run without translation. | Enthusiasts, self-hosters | M | Phase 1 |

## Everyday use

| ID | Idea | Size | Earliest |
|---|---|---|---|
| F-10 | Android apps as their own desktop windows with their own launcher icons. | L | Phase 2 |
| F-11 | "Open in Verone" for links and files from the desktop. | S | Phase 2 |
| F-12 | Per-phone network route: a VPN or proxy that applies to one phone only. | M | Phase 3 |
| F-13 | Scheduled automatic encrypted backups. | S | Phase 3 |
| F-14 | Import apps and data from a real phone, as far as Android allows. | L | Phase 3 |
| F-15 | Google-free services (microG) as a third mode beside "with Google" and "without Google". | M | Phase 3 |
| F-16 | Android desktop mode on a large window; tablet and foldable profiles. | M | Phase 4 |

## Development and testing

| ID | Idea | Size | Earliest |
|---|---|---|---|
| F-20 | Simulation panel: location routes, battery level, network quality, incoming call and SMS events, sensors. | M | Phase 4 |
| F-21 | Quick toggles for testing: language, font size, dark mode, time zone. | S | Phase 4 |
| F-22 | Bug-report bundle: recording, screenshots and logs in one file. | S | Phase 4 |
| F-23 | Several phones on a private network, for testing apps that talk to each other. | M | Phase 4 |
| F-24 | Shared library of phone recipes: community-maintained setup files. | M | Phase 4 |
| F-25 | IDE plugins (Android Studio, VS Code) to start and pick Verone phones. | M | Phase 4 |
| F-26 | Preview channel for new Android versions as soon as images exist. | S | Phase 5 |

## Security research

| ID | Idea | Size | Earliest |
|---|---|---|---|
| F-30 | Analysis mode: isolated network, traffic capture, before-and-after comparison of a phone's storage. Safe only on the VM engine. | L | Phase 7 |
| F-31 | Pass real USB devices (for example security keys) into a phone. | M | Phase 7 |
| F-32 | Bluetooth and NFC, real or simulated. | L | Phase 7 |
| F-33 | Reduced-privilege container mode, if the container engine can run without full host privileges. | M | Phase 1 |

## Server and teams

| ID | Idea | Size | Earliest |
|---|---|---|---|
| F-40 | Single sign-on for teams. | M | Phase 6 |
| F-41 | Pool of ready phones for test farms: hand one out, wipe it on return. | M | Phase 6 |
| F-42 | Single-app mode: a phone locked to one app, for kiosks and displays. | S | Phase 6 |
| F-43 | Classroom sets: many identical phones created from one template. | S | Phase 6 |

## Platform and project

| ID | Idea | Size | Earliest |
|---|---|---|---|
| F-50 | macOS support. | L | Phase 7 |
| F-51 | Packages for more systems: Flatpak, deb, rpm, Nix. | M | Phase 2 |
| F-52 | Plugin system so others can add toolbar tools without changing Verone. | L | Phase 4 |
| F-53 | Project website with a demo video and searchable documentation. | M | Phase 2 |
| F-54 | Funding page (for example GitHub Sponsors) to pay for build servers. | S | Phase 5 |
| F-55 | Governance and maintainer guide once there are regular contributors. | S | when needed |

## Known impossible or out of scope

Recorded so they are not proposed again without new information.

- **Phone calls and SMS on a real number.** A virtual phone has no SIM or radio. Internet calling and messaging apps work; carrier services do not.
- **Apps that require certified hardware** (many banking and payment apps). See non-goals in `vision-and-scope.md`.
- **Public hosting for strangers.** Decision D-003.
