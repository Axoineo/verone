# Decision log

One entry per decision. Never edit an accepted entry's meaning; add a new entry that
supersedes it. Status values: **accepted** (owner approved), **proposed** (designer's
recommendation, awaiting owner approval), **superseded**.

Agents: follow accepted decisions exactly. If a task seems to require breaking one,
stop and raise it with the owner.

| ID | Date | Decision | Status |
|---|---|---|---|
| D-001 | 2026-11-13 | Linux first, then self-hosted server, then Windows. Windows will not depend on WSL2. | accepted |
| D-002 | 2026-11-14 | Container engine (redroid) first. A virtual-machine engine is added later as a full-fidelity mode. All code talks to engines through one interface so the second engine plugs in. | accepted |
| D-003 | 2026-11-14 | The server edition is self-hosted: the owner or their team runs it for themselves. No public multi-tenant hosting. | accepted |
| D-004 | 2026-11-14 | Early phases use prebuilt Android images, modified on the user's machine. Verone's own image, built on a rented server, comes in a later phase. | accepted |
| D-005 | 2026-11-14 | Verone identifies itself honestly. No fingerprint spoofing, no integrity or attestation bypass. Each user registers their own device with Google if they want Google apps. | accepted |
| D-006 | 2026-11-14 | If Google does not use an honestly identified Verone for sign-in prompts, the project continues and documents prompts as unsupported. | accepted |
| D-007 | 2026-11-14 | Verone has its own lock: phone storage is encrypted and unlocked by a Verone password, independent of the computer login. | accepted |
| D-008 | 2026-11-14 | Proprietary components (Google apps, and any ARM translation layer not already present in the upstream image) are never redistributed by Verone. They are fetched on the user's machine after the user opts in. | accepted |
| D-009 | 2026-11-14 | Documentation lives in this repository under `docs/` as Markdown and is the single source of truth for the owner and all agents. | accepted |
| D-010 | 2026-11-14 | Light package: Verone ships only its own binaries. Container runtime, Android images and optional components are system dependencies or on-demand downloads. | accepted |
| D-011 | 2026-11-14 | Core language is Rust: one `verone-core` library, a `verone` command-line tool, and a desktop app built with Tauri on top of the same library. | accepted |
| D-012 | 2026-11-14 | Supported container runtimes: Docker first; Podman evaluated in Phase 0 (assumption A-11) and added if it works. | accepted |
| D-013 | 2026-11-14 | The project licence is Apache-2.0, replacing the MIT licence the repository started with. The copyright notice is in `NOTICE`. | accepted |
| D-014 | 2026-11-14 | Google apps are optional. First-run offers "with Google" or "without Google"; a Google-free phone is a fully supported mode, not a fallback. | accepted |
| D-015 | 2026-11-14 | Verone collects no usage data. | accepted |

## Reasons

**D-002.** Measured on the owner's laptop (8 GB RAM, 4 cores): the container engine
boots Android 14 in 23 s and uses about 860 MB RAM. A virtual machine needs roughly
2–4 GB and is harder to set up, but has its own Android kernel and security model, which
security testing needs. Shipping the light engine first reaches users soonest; the
interface keeps the second engine from being a rewrite.

**D-005.** Copied device identities break Google's terms, get blocked once shared by
many users, and expose an open-source project to takedown. Honest identity plus the
official per-user registration page is the only route that can last.

**D-007.** See the security statement in `vision-and-scope.md` for what the lock does
and does not protect against.

**D-011.** Rust gives one small static binary for the command line and the server, and
Tauri reuses that code in a desktop app that is far lighter than Electron. It is also a
language Claude Code agents write and test reliably. Alternative considered: Go (simpler
Docker integration, but a second language would be needed for the desktop app).

**D-013.** Apache-2.0 gives the same freedoms as MIT and adds a patent grant from
every contributor. Android, redroid and scrcpy use it, so code can move between them
and Verone without licence friction. Code from GPL projects (for example Waydroid)
must not be copied into Verone.

**D-014.** Widens the audience (privacy-minded users, testers who need a clean system)
and removes the hard dependency on Google's acceptance of the device.
