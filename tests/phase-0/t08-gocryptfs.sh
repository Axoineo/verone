#!/usr/bin/env bash
# Phase 0, test T-08 (assumption A-09), candidate 2: Android data on a gocryptfs
# (FUSE) mount served by root inside a helper container. Needs no kernel module
# beyond FUSE.
#
# Usage: t08-gocryptfs.sh build | create | unlock | lock | status
# The password is read from the environment variable VERONE_P0_PW (test value only;
# passing it through the container environment is not acceptable for the product).
set -u
P0="${P0:-$HOME/.local/share/verone-phase0}"
ENC="$P0/enc2"
HELPER=verone-p0-gcfs
PW="${VERONE_P0_PW:-}"

run() { docker run --rm -i --privileged -e PW="$PW" -v "$ENC:/work:rshared" "$HELPER" sh -c "$1"; }

case "${1:-}" in
  build)
    printf 'FROM alpine:3\nRUN apk add --no-cache gocryptfs fuse fuse3\n' | docker build -q -t "$HELPER" - ;;
  create)
    mkdir -p "$ENC/cipher" "$ENC/mnt"
    run 'printf %s "$PW" > /dev/shm/pw && gocryptfs -init -q -passfile /dev/shm/pw /work/cipher' ;;
  unlock)
    docker rm -f "$HELPER" >/dev/null 2>&1
    docker run -d --name "$HELPER" --privileged --pid=host -e PW="$PW" -v "$ENC:/work:rshared" "$HELPER" \
      sh -c 'printf %s "$PW" > /dev/shm/pw && exec gocryptfs -fg -allow_other -passfile /dev/shm/pw /work/cipher /work/mnt' >/dev/null
    for _ in 1 2 3 4 5 6 7 8 9 10; do
      mountpoint -q "$ENC/mnt" && { echo unlocked; exit 0; }
      [ "$(docker inspect -f '{{.State.Running}}' "$HELPER" 2>/dev/null)" = true ] || break
      sleep 1
    done
    echo "unlock failed:"; docker logs "$HELPER" 2>&1 | tail -3; exit 1 ;;
  lock)
    docker exec "$HELPER" sh -c 'fusermount3 -u /work/mnt || umount /work/mnt' 2>&1
    docker rm -f "$HELPER" >/dev/null 2>&1
    mountpoint -q "$ENC/mnt" && { echo "STILL MOUNTED"; exit 1; } || echo locked ;;
  status)
    mountpoint "$ENC/mnt"; docker ps --filter "name=$HELPER" --format '{{.Status}}' ;;
  *) echo "usage: $0 build|create|unlock|lock|status"; exit 2 ;;
esac
