# Command reference

Everything the `verone` command can do in version 0.1.

## Before you start

Run `verone doctor`. It checks the computer and prints a fix for anything missing.

A phone is identified by its **id**, a short name you choose when creating it:
lowercase letters, digits and hyphens, 1 to 31 characters, starting with a letter or
digit. Examples: `work`, `test-2`.

## Options that work with every command

| Option | Effect |
|---|---|
| `--json` | Print the result as JSON, for scripts. |
| `-v`, `--verbose` | Show each external command (Docker, adb, scrcpy) before it runs. |
| `--help` | Explain the command. |
| `-V`, `--version` | Print the Verone version. In a terminal it is shown beside the Verone logo, in colour unless `NO_COLOR` is set; piped or redirected, it is the single line `verone 0.1.0`. |

## Commands

### `verone doctor`

Checks the container runtime, kernel support, disk space, memory, graphics and helper
programs. Each line is `ok`, `warning` or `FAILED`, with a fix under anything that is
not ok. Warnings limit some features; failures stop phones from running.

### `verone create <id>`

Creates a phone. It is not started, and nothing is downloaded.

| Option | Default | Meaning |
|---|---|---|
| `--name TEXT` | the id | The name shown in windows and lists. |
| `--android VERSION` | `14` | Android version. Only 14 is available in version 0.1. |
| `--size WIDTHxHEIGHT` | `720x1280` | Screen size in pixels, each side from 240 to 4096. |
| `--dpi NUMBER` | `320` | Screen density, from 72 to 640. |
| `--graphics auto\|software\|gpu` | `auto` | How the screen is drawn. See "Graphics" below. |
| `--adb-port NUMBER` | none | Publish adb on this port of `127.0.0.1`, so development tools can find the phone. See [SECURITY.md](../../SECURITY.md). |

### `verone start <id>`

Boots the phone and waits until Android is ready, usually 10 to 30 seconds. The first
start of any phone downloads the Android image (about 3 GB). Starting a phone that is
already running does nothing.

### `verone stop <id>`

Shuts Android down cleanly. Everything on the phone is kept.

### `verone restart <id>`

Stops the phone, then starts it.

### `verone list`

Shows every phone with its name, Android version and whether it is running.

### `verone info <id>`

Shows one phone in detail: status, how long it has been on, its address, screen,
graphics mode and the size of its data.

### `verone open <id> [-- scrcpy options]`

Shows the phone in a window, starting it first if needed. The window is provided by
[scrcpy](https://github.com/Genymobile/scrcpy); anything after `--` is passed to it.

```bash
verone open work
verone open work -- --max-fps=30 --always-on-top
```

Closing the window does not stop the phone.

### `verone adb <id> -- <adb arguments>`

Runs an adb command against a running phone.

```bash
verone adb work -- shell
verone adb work -- logcat
verone adb work -- push photo.jpg /sdcard/Pictures/
```

### `verone install <id> <file.apk>`

Installs an app from an APK file on a running phone and prints the app's package name.

### `verone reset <id>`

Erases every app, account and file on the phone and keeps its settings. The next start
is like a new phone. Asks for confirmation; add `--yes` to skip the question.

### `verone delete <id>`

Removes the phone and everything on it. Asks for confirmation; add `--yes` to skip the
question.

### `verone image list` and `verone image pull`

`list` shows the Android images Verone supports and whether each is downloaded. `pull`
downloads an image ahead of time, so that the first `start` does not have to.

### `verone completions <shell>`

Prints a completion script for `bash`, `zsh`, `fish`, `elvish` or `powershell`. The
Arch package installs these for you.

## Graphics

| Mode | What happens |
|---|---|
| `auto` | Uses the GPU if the computer has one Verone can use, otherwise software rendering. If a GPU start fails once, the phone switches to software and stays there. |
| `software` | Always draws in software. Slower drawing, quicker start, works everywhere. |
| `gpu` | Always uses the GPU. The phone does not start if that fails. |

A GPU driven by NVIDIA's proprietary driver cannot be used; Intel graphics are known to
work. `verone doctor` shows what `auto` will choose.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success. |
| 1 | The operation failed. |
| 2 | The command was written wrongly. |
| 3 | The computer cannot run phones; run `verone doctor`. |
| 4 | There is no such phone or Android version. |
| 5 | Another `verone` command is working on the same phone. |

## Where things are kept

Phones live in `~/.local/share/verone/phones/<id>/`. Set the `VERONE_HOME` environment
variable to a full path to keep them somewhere else. The Android image is stored by
Docker.

## Known limits in version 0.1

- **"Power off" inside Android turns the phone back on.** Use `verone stop`.
- **No Google apps or Play Store.** Install apps from APK files.
- **One Android version (14), x86-64 computers, Docker only.**
- **No sound has been verified.** scrcpy forwards audio on Android 11 and later, but it
  has not been tested with Verone.
- **Phone data is not encrypted.** See [SECURITY.md](../../SECURITY.md).
- **`--adb-port` may fail after a restart of the computer** if the firewall module
  `xt_tcpudp` is not loaded. The error message says how to load it.
