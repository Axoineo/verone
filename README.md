# Verone

Verone is an open-source virtual Android phone that runs as an application on a PC,
laptop or server. It starts like a new phone, keeps everything you add, and comes back
as you left it.

**Status: early.** Version 0.1 is a command-line tool for Linux. A desktop app, Google
apps, encryption and Windows support are planned; see the [roadmap](docs/roadmap.md).

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
- Verone runs Android in a privileged container that shares your computer's kernel.
  Read [SECURITY.md](SECURITY.md) before running software you do not trust in it.

## Documentation

- [What Verone is and who it is for](docs/vision-and-scope.md)
- [Command reference](docs/user-guide/cli.md)
- [Roadmap](docs/roadmap.md)
- [All design documentation](docs/README.md)
- [Contributing](CONTRIBUTING.md)

Licence: Apache-2.0.
