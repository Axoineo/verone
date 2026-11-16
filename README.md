<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/brand/verone-logo-on-dark.svg">
    <img src="docs/brand/verone-logo.svg" alt="Verone" width="400">
  </picture>
</h1>

<p align="center">
  <strong>A virtual Android phone that runs on your PC, laptop or server.</strong><br>
  It starts like a new phone, keeps everything you add, and comes back as you left it.
</p>

<p align="center">
  <a href="https://github.com/Axoineo/verone/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Axoineo/verone/ci.yml?branch=main&label=CI&logo=github" alt="CI status"></a>
  <a href="https://github.com/Axoineo/verone/releases/latest"><img src="https://img.shields.io/github/v/release/Axoineo/verone?label=release&color=A4C639" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/Axoineo/verone?color=1F2A14" alt="Licence: Apache-2.0"></a>
  <a href="rust-toolchain.toml"><img src="https://img.shields.io/badge/rust-1.99%2B-B7410E?logo=rust" alt="Rust 1.99 or newer"></a>
  <img src="https://img.shields.io/badge/Android-14-A4C639?logo=android&logoColor=white" alt="Android 14">
  <img src="https://img.shields.io/badge/platform-Linux%20x86--64-1F2A14?logo=linux&logoColor=white" alt="Platform: Linux x86-64">
</p>

<p align="center">
  <a href="#quick-start">Quick start</a> ·
  <a href="docs/user-guide/cli.md">Command reference</a> ·
  <a href="docs/roadmap.md">Roadmap</a> ·
  <a href="CONTRIBUTING.md">Contributing</a> ·
  <a href="SECURITY.md">Security</a> ·
  <a href="CHANGELOG.md">Changelog</a>
</p>

---

## About

Verone is an open-source virtual Android phone that runs as an application on a PC,
laptop or server. Each phone is kept on your computer: switch it off and everything on
it is still there the next time you start it.

- **One command per job.** Create, start, open, stop, reset and delete phones from the
  terminal.
- **Many phones.** Keep separate phones for work, testing or anything else, each with
  its own name, screen size and data.
- **Made for developers.** Install APK files, get an `adb` shell with root access, and
  script everything with `--json` output.
- **Checks before it fails.** `verone doctor` tests this computer and says how to fix
  what is missing.

> [!NOTE]
> **Status: early.** Version 0.1 is a command-line tool for Linux. A desktop app, Google
> apps, encryption and Windows support are planned; see the [roadmap](docs/roadmap.md).

## What you need

- Linux on an x86-64 computer, with about 4 GB of free disk space and 2 GB of free memory.
- [Docker](https://docs.docker.com/engine/install/), with your user in the `docker` group.
- A kernel with binder support. Arch, Fedora and Debian have it; on Ubuntu install
  `linux-modules-extra-$(uname -r)`.
- `adb` (package `android-tools`) and [`scrcpy`](https://github.com/Genymobile/scrcpy)
  to see and control the phone.

`verone doctor` checks all of this and tells you how to fix what is missing.

## Install

On Arch Linux, from a checkout of this repository:

```bash
cd packaging/arch
makepkg -f
sudo pacman -U verone-*.pkg.tar.zst
```

On other distributions, with [Rust](https://www.rust-lang.org/tools/install) installed:

```bash
cargo install --path crates/verone-cli
```

Check the installation with `verone --version`.

## Quick start

```bash
verone doctor            # check that this computer can run phones
verone create work       # make a phone called "work"
verone start work        # boot it; the first start downloads Android (about 3 GB)
verone open work         # show it in a window
```

When you are done:

```bash
verone stop work         # switch it off; everything on it is kept
```

Other things you can do:

```bash
verone list                     # all phones and whether they are running
verone info work                # details about one phone
verone install work app.apk     # install an app
verone adb work -- shell        # a shell on the phone
verone reset work               # erase the phone, keep its settings
verone delete work              # remove it completely
```

The [command reference](docs/user-guide/cli.md) describes every command and option.

## What to expect

- It is a plain Android 14 system without Google apps. Install apps from APK files or
  an open app store such as F-Droid.
- The phone has root access through `adb`, which suits development and testing.
- Apps that demand certified hardware, such as many banking apps, will not run.

> [!WARNING]
> Verone runs Android in a privileged container that shares your computer's kernel.
> Read [SECURITY.md](SECURITY.md) before running software you do not trust in it.

## Documentation

| Read this | To learn |
|---|---|
| [Vision and scope](docs/vision-and-scope.md) | What Verone is and who it is for |
| [Command reference](docs/user-guide/cli.md) | Every command, option and exit code |
| [Roadmap](docs/roadmap.md) | What comes after version 0.1 |
| [Architecture](docs/architecture.md) | How the pieces fit together |
| [All design documentation](docs/README.md) | Decisions, assumptions and phase plans |
| [Brand](docs/brand/README.md) | The logo, colours and how to use them |

## Contributing

Bug reports, ideas and pull requests are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md)
for how the project works and how to build and test it, and [SECURITY.md](SECURITY.md) for how to
report a security problem privately.

## Licence

Verone is released under the [Apache License 2.0](LICENSE); see also [NOTICE](NOTICE).
