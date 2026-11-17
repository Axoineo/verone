# Security

This page says plainly what Verone protects and what it does not, and how to report a
problem.

## What you should know before using Verone

**The phone runs in a privileged container.** Android needs wide access to the kernel,
so Verone starts its container with Docker's `--privileged` option. Software running as
root inside the phone has a much shorter path to your computer than it would on a real
phone. Do not use a Verone phone to open apps or files you believe are malicious. A
virtual-machine engine with real isolation is planned for that purpose.

**The phone shares your computer's kernel.** Android's own protections (verified boot,
hardware-backed keys, strict SELinux) are absent or weakened. A security result
observed in Verone does not automatically hold on a real phone, in either direction.

**Android 12 phones bring one more privileged helper.** While an Android 12 phone
exists, Verone runs a small container named `verone-power-<code>` that can see and
enter every process on the computer, so it can restart or power off Android 12 phones
when asked from inside (Android 12 ignores those requests itself). It has no network
and a read-only file system, runs a fixed script from Verone's source, and only acts on
Verone's own Android 12 phones. It is removed when the last Android 12 phone is
deleted. Its log is `docker logs verone-power-<code>`.

**Anyone who can use Docker controls your phones.** Membership of the `docker` group is
equivalent to administrator rights on the computer, and that includes every phone's data.

**adb gives root access to the phone and has no password.** Verone does not publish it:
by default adb is reachable only at the container's private address on your own
computer. If you create a phone with `--adb-port`, the port is bound to `127.0.0.1`
only, which means any program running under any user on your computer can reach it.
Never forward that port to a network.

**Phone data is not encrypted yet.** In version 0.1 a phone's data is stored under
`~/.local/share/verone` as ordinary files owned by root. Anyone with administrator
access to the computer, or the disk, can read it. Encrypted storage with its own
password is planned.

**A phone on the same computer is a weaker second factor.** If you keep an
authenticator app in a Verone phone on the computer you sign in from, losing or
exposing that computer exposes both factors.

**The Android image is not built by this project.** Version 0.1 runs a published
redroid image, pinned by its digest so it cannot change underneath you. Its Android
security patch level is that of the image, not of today.

## What Verone will never do

Verone does not pretend to be a certified device and does not bypass integrity or
attestation checks. Contributions that add such behaviour are declined.

## Reporting a vulnerability

Please report security problems privately through GitHub's "Report a vulnerability"
button on the repository's Security tab, not in a public issue. Include what you found,
how to reproduce it, and the Verone version (`verone --version`).

This is a small volunteer project. Expect an acknowledgement within a week.
