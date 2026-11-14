#!/usr/bin/env bash
# Phase 0, test T-08 (assumption A-09), candidate 1: Android data on a LUKS-encrypted
# disk image that a privileged helper container opens and mounts.
#
# Usage: t08-luks.sh build | create | unlock | lock | status
# The password is read from the environment variable VERONE_P0_PW (test value only).
set -u
P0="${P0:-$HOME/.local/share/verone-phase0}"
ENC="$P0/enc"
NAME=verone-p0-data
HELPER=verone-p0-crypt
PW="${VERONE_P0_PW:-}"

helper() { docker run --rm -i --privileged -v /dev:/dev -v "$ENC:/work:rshared" "$HELPER" sh -c "$1"; }

case "${1:-}" in
  build)
    printf 'FROM alpine:3\nRUN apk add --no-cache cryptsetup e2fsprogs util-linux\n' | docker build -q -t "$HELPER" - ;;
  create)
    mkdir -p "$ENC/mnt"
    truncate -s "${SIZE:-8G}" "$ENC/data.img"
    printf %s "$PW" | helper "cryptsetup luksFormat --batch-mode --key-file=- /work/data.img" || exit 1
    printf %s "$PW" | helper "cryptsetup open --key-file=- /work/data.img $NAME && mkfs.ext4 -q /dev/mapper/$NAME && cryptsetup close $NAME" ;;
  unlock)
    printf %s "$PW" | helper "cryptsetup open --key-file=- /work/data.img $NAME && mount /dev/mapper/$NAME /work/mnt" ;;
  lock)
    helper "umount /work/mnt; cryptsetup close $NAME" ;;
  status)
    helper "cryptsetup status $NAME 2>&1 | head -4; mount | grep /work/mnt || echo 'not mounted'" ;;
  *) echo "usage: $0 build|create|unlock|lock|status"; exit 2 ;;
esac
