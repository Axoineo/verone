#!/usr/bin/env bash
# Phase 1 exit test: every command against a real phone.
#
# Needs Docker, kernel binder support, adb and scrcpy. Uses its own data home, so
# it never touches the user's phones.
#
#   tests/e2e/phase-1.sh                 # uses target/release/verone
#   VERONE=/usr/bin/verone tests/e2e/phase-1.sh
#   APK=/path/to/app.apk tests/e2e/phase-1.sh   # skip the F-Droid download
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
VERONE="${VERONE:-$ROOT/target/release/verone}"
WORK="${E2E_DIR:-$ROOT/target/e2e}"
export VERONE_HOME="$WORK/home"
ID=e2e
PACKAGE=org.fdroid.fdroid

step() { printf '\n== %s\n' "$*"; }
fail() { printf 'FAILED: %s\n' "$*" >&2; exit 1; }
v() { "$VERONE" "$@"; }

# Runs a command that must exit with the given code.
expect_code() {
  local want="$1"; shift
  local got=0
  "$@" >/dev/null 2>&1 || got=$?
  [ "$got" = "$want" ] || fail "expected exit $want, got $got: $*"
}

has_app() { v adb "$ID" -- shell pm list packages | grep -q "package:$PACKAGE"; }

cleanup() {
  v delete "$ID" --yes >/dev/null 2>&1 || true
}
trap cleanup EXIT

[ -x "$VERONE" ] || fail "no verone binary at $VERONE (run: cargo build --release)"
mkdir -p "$WORK"
cleanup

APK="${APK:-$WORK/fdroid.apk}"
if [ ! -f "$APK" ]; then
  step "download a test app"
  curl -fsSL --retry 3 -o "$APK" https://f-droid.org/F-Droid.apk
fi

step "doctor"
v doctor

step "create"
v create "$ID" --name "End to end"
expect_code 1 "$VERONE" create "$ID"
expect_code 2 "$VERONE" create "Bad Id"
v list | grep -q "^$ID .* stopped$" || fail "new phone is not listed as stopped"

step "start"
v start "$ID"
v start "$ID" | grep -q "already running" || fail "second start did not say already running"
[ "$(v info "$ID" --json | grep -c '"status": "running"')" = 1 ] || fail "info does not say running"
v info "$ID"

step "open (no window, 5 seconds)"
v open "$ID" -- --no-window --no-audio --time-limit=5

step "adb"
[ "$(v adb "$ID" -- shell getprop sys.boot_completed | tr -d '\r')" = 1 ] || fail "adb getprop"
[ "$(v adb "$ID" -- shell getprop ro.build.version.release | tr -d '\r')" = 14 ] || fail "not Android 14"

step "install"
v install "$ID" "$APK"
has_app || fail "the app is not installed"

step "stop, start: the app survives"
v stop "$ID"
v stop "$ID" | grep -q "already stopped" || fail "second stop did not say already stopped"
expect_code 1 "$VERONE" adb "$ID" -- shell true
v start "$ID"
has_app || fail "the app did not survive stop and start"

step "restart"
v restart "$ID"
has_app || fail "the app did not survive restart"

step "two commands at once: one succeeds, one is busy"
v stop "$ID"
"$VERONE" start "$ID" >"$WORK/first.out" 2>&1 & FIRST=$!
sleep 1
"$VERONE" start "$ID" >"$WORK/second.out" 2>&1 & SECOND=$!
FIRST_CODE=0; SECOND_CODE=0
wait "$FIRST" || FIRST_CODE=$?
wait "$SECOND" || SECOND_CODE=$?
printf 'first exit %s, second exit %s\n' "$FIRST_CODE" "$SECOND_CODE"
[ "$FIRST_CODE" = 0 ] || fail "the first command failed: $(cat "$WORK/first.out")"
if [ "$SECOND_CODE" != 5 ] && [ "$SECOND_CODE" != 0 ]; then
  fail "the second command exited $SECOND_CODE: $(cat "$WORK/second.out")"
fi

step "reset: the app is gone"
expect_code 0 sh -c "printf 'n\n' | '$VERONE' reset '$ID'"
v reset "$ID" --yes
v start "$ID"
if has_app; then fail "the app survived a reset"; fi

step "delete"
v delete "$ID" --yes
expect_code 4 "$VERONE" info "$ID"
if docker ps -a --format '{{.Names}}' | grep -q "^verone-$ID$"; then
  fail "the container still exists"
fi
[ ! -e "$VERONE_HOME/phones/$ID" ] || fail "the phone folder still exists"
v list | grep -q "no phones yet" || fail "the list is not empty"

trap - EXIT
printf '\nPhase 1 exit test passed.\n'
