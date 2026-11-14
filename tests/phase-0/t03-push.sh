#!/usr/bin/env bash
# Phase 0, test T-03 (assumption A-14): adb push speed for three ways of reaching adb.
#   A: container address, no published port
#   B: socat bridge through `docker exec`, no published port
#   P: published port on 127.0.0.1 (baseline)
set -u
P0="${P0:-$HOME/.local/share/verone-phase0}"
IMG=redroid/redroid:14.0.0-latest
ARGS="androidboot.redroid_gpu_mode=guest androidboot.redroid_width=720 androidboot.redroid_height=1280 androidboot.redroid_dpi=320"

start() { # $1 = extra docker options
  docker rm -f verone-p0 >/dev/null 2>&1
  # shellcheck disable=SC2086
  docker run -d --privileged --name verone-p0 -v "$P0/data:/data" $1 $IMG $ARGS >/dev/null
  SECONDS=0
  until [ "$(docker exec verone-p0 getprop sys.boot_completed 2>/dev/null)" = 1 ] || [ $SECONDS -gt 150 ]; do sleep 2; done
  echo "booted in ${SECONDS}s"
}

push() { # $1 = label, $2 = adb serial
  adb disconnect >/dev/null 2>&1
  adb connect "$2" >/dev/null
  sleep 2
  if ! adb -s "$2" shell true 2>/dev/null; then echo "$1: adb not reachable at $2"; return; fi
  local t0 t1
  t0=$(date +%s.%N)
  adb -s "$2" push "$P0/blob" /sdcard/blob 2>&1 | tail -1
  t1=$(date +%s.%N)
  echo "$1: 200 MiB in $(echo "$t1 - $t0" | bc) s = $(echo "scale=1; 200 / ($t1 - $t0)" | bc) MiB/s"
  adb -s "$2" shell rm /sdcard/blob
}

[ -f "$P0/blob" ] || head -c 200M /dev/urandom > "$P0/blob"

echo "== no published port"
start ""
IP=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' verone-p0)
echo "container address: $IP"
push "A (container address)" "$IP:5555"

socat TCP-LISTEN:5555,bind=127.0.0.1,reuseaddr,fork EXEC:"docker exec -i verone-p0 nc 127.0.0.1 5555" &
SOCAT=$!
sleep 1
push "B (socat bridge)" "127.0.0.1:5555"
echo "host listeners on 5555 during B:"; ss -ltn | grep ':5555' || echo "  none"
kill $SOCAT 2>/dev/null; wait $SOCAT 2>/dev/null

echo "== published port"
start "-p 127.0.0.1:5555:5555"
push "P (published 127.0.0.1)" "127.0.0.1:5555"
adb disconnect >/dev/null 2>&1
