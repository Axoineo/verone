#!/usr/bin/env bash
# Phase 0, test T-09 (assumption A-04): build a local Android 14 image with Google apps.
#
# Does by hand what redroid-script (MIT, commit a4951b7) does for `-a 14.0.0 -mtg`:
# download the package it pins, copy its system/ tree over the redroid image, and set
# the same entrypoint. Nothing is uploaded or published; the image stays on this machine.
set -eu
P0="${P0:-$HOME/.local/share/verone-phase0}"
W="$P0/gapps"
URL="https://github.com/s1204IT/MindTheGappsBuilder/releases/download/20240226/MindTheGapps-14.0.0-x86_64-20240226.zip"
MD5="a827a84ccb0cf5914756e8561257ed13"
TAG="redroid/redroid:14.0.0_mindthegapps"

mkdir -p "$W/dl" "$W/build"
ZIP="$W/dl/mindthegapps.zip"
if [ ! -f "$ZIP" ] || [ "$(md5sum "$ZIP" | cut -d' ' -f1)" != "$MD5" ]; then
  # -C - resumes a partial download; the checksum below catches a bad result.
  curl -fsSL -C - --retry 5 --retry-all-errors -o "$ZIP" "$URL"
fi
echo "md5 expected $MD5"
echo "md5 actual   $(md5sum "$ZIP" | cut -d' ' -f1)"
[ "$(md5sum "$ZIP" | cut -d' ' -f1)" = "$MD5" ] || { echo "checksum mismatch"; exit 1; }

rm -rf "$W/build/mindthegapps"
mkdir -p "$W/build/mindthegapps"
bsdtar -xf "$ZIP" -C "$W/build/mindthegapps" system
echo "extracted: $(du -sh "$W/build/mindthegapps" | cut -f1), $(find "$W/build/mindthegapps" -name '*.apk' | wc -l) apk files"

cat > "$W/build/Dockerfile" <<'EOF'
FROM redroid/redroid:14.0.0-latest
COPY mindthegapps /
ENTRYPOINT ["/init", "androidboot.hardware=redroid", "ro.setupwizard.mode=DISABLED"]
EOF
SECONDS=0
docker build -q -t "$TAG" "$W/build"
echo "build took ${SECONDS}s"
docker images --format '{{.Repository}}:{{.Tag}} {{.Size}}' | grep redroid
