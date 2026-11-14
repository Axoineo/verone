#!/usr/bin/env bash
# Phase 0, test T-04 (assumption A-08): compare software rendering with GPU rendering.
# Usage: t04-gpu.sh guest | t04-gpu.sh host /dev/dri/renderD128
set -u
P0="${P0:-$HOME/.local/share/verone-phase0}"
MODE="$1"; NODE="${2:-}"
IMG=redroid/redroid:14.0.0-latest
GPU="androidboot.redroid_gpu_mode=$MODE"
[ -n "$NODE" ] && GPU="$GPU androidboot.redroid_gpu_node=$NODE"
LABEL="$MODE$(basename "$NODE")"

docker rm -f verone-p0 >/dev/null 2>&1
# shellcheck disable=SC2086
docker run -d --privileged --name verone-p0 -v "$P0/data:/data" $IMG $GPU \
  androidboot.redroid_width=720 androidboot.redroid_height=1280 androidboot.redroid_dpi=320 >/dev/null
SECONDS=0
until [ "$(docker exec verone-p0 getprop sys.boot_completed 2>/dev/null)" = 1 ] || [ $SECONDS -gt 120 ]; do sleep 2; done
if [ "$(docker exec verone-p0 getprop sys.boot_completed 2>/dev/null)" != 1 ]; then
  echo "[$LABEL] DID NOT BOOT within ${SECONDS}s; container: $(docker inspect -f '{{.State.Status}}' verone-p0)"
  docker exec verone-p0 logcat -d 2>/dev/null | grep -iE "gralloc|egl|hwcomposer|minigbm|mesa|drm" | tail -15
  exit 1
fi
echo "[$LABEL] booted in ${SECONDS}s"
docker exec verone-p0 sh -c 'dumpsys SurfaceFlinger | grep -iE "GLES|Vulkan" | head -3'
docker exec verone-p0 sh -c 'echo "egl=$(getprop ro.hardware.egl) gralloc=$(getprop ro.hardware.gralloc) vulkan=$(getprop ro.hardware.vulkan)"'

IP=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' verone-p0)
adb disconnect >/dev/null 2>&1; adb connect "$IP:5555" >/dev/null; sleep 2
docker exec verone-p0 am start -a android.settings.SETTINGS >/dev/null 2>&1
( while docker exec verone-p0 true 2>/dev/null; do
    docker exec verone-p0 input swipe 360 900 360 300 200 2>/dev/null; sleep 1
    docker exec verone-p0 input swipe 360 300 360 900 200 2>/dev/null; sleep 1
  done ) &
GEN=$!
OUT="$P0/rec-gpu-$LABEL.mkv"; rm -f "$OUT"
scrcpy -s "$IP:5555" --no-window --no-audio --max-fps=60 --record="$OUT" --time-limit=60 >"$P0/scrcpy-gpu-$LABEL.log" 2>&1 &
SC=$!
sleep 15
for i in 1 2 3 4 5; do docker stats --no-stream --format '{{.CPUPerc}} {{.MemUsage}}' verone-p0; sleep 6; done
wait $SC; echo "[$LABEL] scrcpy exit=$?"
kill $GEN 2>/dev/null
adb -s "$IP:5555" exec-out screencap -p > "$P0/shot-gpu-$LABEL.png" 2>/dev/null
adb disconnect >/dev/null 2>&1
command -v ffprobe >/dev/null && ffprobe -v error -count_packets -select_streams v:0 \
  -show_entries stream=nb_read_packets -of default=nw=1 "$OUT" 2>&1
