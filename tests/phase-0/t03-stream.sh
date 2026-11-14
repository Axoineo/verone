#!/usr/bin/env bash
# Phase 0, test T-03 (assumption A-14): 10-minute scrcpy stream stability for
#   A: container address, no published port
#   B: socat bridge through `docker exec`, no published port
# The stream is recorded to a file instead of shown, and synthetic swipes keep the
# screen changing. Expects verone-p0 to be absent or disposable.
set -u
P0="${P0:-$HOME/.local/share/verone-phase0}"
SECS="${SECS:-600}"
IMG=redroid/redroid:14.0.0-latest
ARGS="androidboot.redroid_gpu_mode=guest androidboot.redroid_width=720 androidboot.redroid_height=1280 androidboot.redroid_dpi=320"

docker rm -f verone-p0 >/dev/null 2>&1
# shellcheck disable=SC2086
docker run -d --privileged --name verone-p0 -v "$P0/data:/data" $IMG $ARGS >/dev/null
until [ "$(docker exec verone-p0 getprop sys.boot_completed 2>/dev/null)" = 1 ]; do sleep 2; done
docker exec verone-p0 am start -a android.settings.SETTINGS >/dev/null 2>&1

( while docker exec verone-p0 true 2>/dev/null; do
    docker exec verone-p0 input swipe 360 900 360 300 200 2>/dev/null
    sleep 1
    docker exec verone-p0 input swipe 360 300 360 900 200 2>/dev/null
    sleep 1
  done ) &
GEN=$!

run() { # $1 = label, $2 = adb serial
  adb disconnect >/dev/null 2>&1
  adb connect "$2" >/dev/null
  sleep 2
  local out="$P0/rec-$1.mkv" t0
  rm -f "$out"
  t0=$(date +%s)
  scrcpy -s "$2" --no-window --no-audio --max-fps=60 --record="$out" --time-limit="$SECS" >"$P0/scrcpy-$1.log" 2>&1
  echo "$1: scrcpy exit=$? after $(( $(date +%s) - t0 ))s, file=$(du -h "$out" 2>/dev/null | cut -f1)"
  grep -iE "error|warn|disconnect" "$P0/scrcpy-$1.log" | head -5
}

IP=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' verone-p0)
run A "$IP:5555"

socat TCP-LISTEN:5555,bind=127.0.0.1,reuseaddr,fork EXEC:"docker exec -i verone-p0 nc 127.0.0.1 5555" 2>/dev/null &
SOCAT=$!
sleep 1
run B "127.0.0.1:5555"

kill $SOCAT $GEN 2>/dev/null
adb disconnect >/dev/null 2>&1
docker stats --no-stream --format 'end state: cpu={{.CPUPerc}} mem={{.MemUsage}}' verone-p0
