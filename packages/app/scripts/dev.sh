#!/usr/bin/env bash
#
# Runs the debug build for development.
#
# On macOS the build is started the way the installed app is: as a bundle,
# by LaunchServices. A binary started from a terminal (`cargo run`) is held
# to the terminal's permissions instead of its own, so the permission prompts
# name the terminal, and the one for System Audio Recording never appears.
#
# Usage: packages/app/scripts/dev.sh [arguments for the app]
#
# Env:
#   SIGN_IDENTITY  codesign identity. Default: the first "Apple Development"
#                  identity in the keychain, else "-" (ad-hoc). macOS ties the
#                  permissions it granted to the signature. An ad-hoc one
#                  changes with every rebuild, so macOS then asks again.
#   BUNDLE_ID      default io.voxfusion.app.dev: the permissions of the dev
#                  build are its own, not the installed app's.
#   APP_NAME       default "VoxFusion Dev"
#   VOXFUSION_*, RUST_BACKTRACE
#                  passed on to the app. Settings, history and models are the
#                  installed app's unless VOXFUSION_APP_ID names another
#                  profile.
set -euo pipefail

APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$APP_DIR"

if [ "$(uname)" != "Darwin" ]; then
  exec cargo run -- "$@"
fi

export BUNDLE_ID="${BUNDLE_ID:-io.voxfusion.app.dev}"
export APP_NAME="${APP_NAME:-VoxFusion Dev}"

if [ -z "${SIGN_IDENTITY:-}" ]; then
  SIGN_IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
    | sed -n 's/.*"\(Apple Development: [^"]*\)".*/\1/p' | head -1)"
  SIGN_IDENTITY="${SIGN_IDENTITY:--}"
fi
export SIGN_IDENTITY

bash scripts/bundle-macos.sh --debug

case "$(uname -m)" in
  arm64) OUT_DIR="$APP_DIR/dist/aarch64-apple-darwin" ;;
  *) OUT_DIR="$APP_DIR/dist/x86_64-apple-darwin" ;;
esac
APP="$OUT_DIR/$APP_NAME.app"
EXECUTABLE="$APP/Contents/MacOS/voxfusion-app"

if [ "$SIGN_IDENTITY" = "-" ]; then
  # An ad-hoc signature is the hash of the code, so macOS takes a rebuilt app
  # for a different one. The permissions of the old one stay in System
  # Settings, switched on and without effect, until they are forgotten.
  STAMP="$OUT_DIR/.dev-code-hash"
  HASH="$(codesign -dvvv "$APP" 2>&1 | sed -n 's/^CDHash=//p')"
  if [ "$HASH" != "$(cat "$STAMP" 2>/dev/null || true)" ]; then
    tccutil reset All "$BUNDLE_ID" > /dev/null 2>&1 || true
    echo "$HASH" > "$STAMP"
  fi
  echo "No signing identity: macOS asks for permissions again after every rebuild." >&2
  echo "Set SIGN_IDENTITY to keep them." >&2
fi

LOG="$HOME/Library/Logs/${VOXFUSION_APP_ID:-io.voxfusion.app}/voxfusion.log"
OUTPUT="$(mktemp -t voxfusion-dev)"
mkdir -p "$(dirname "$LOG")"
touch "$LOG"

ENVIRONMENT=()
while IFS= read -r variable; do
  ENVIRONMENT+=(--env "$variable")
done < <(env | grep -E '^(VOXFUSION_[A-Z_]+|RUST_BACKTRACE)=' || true)

# A copy left over from the last run would only be asked to show its window.
pkill -f "$EXECUTABLE" 2> /dev/null || true

# The app writes its log to a file, and panics to standard error.
tail -q -n 0 -F "$LOG" "$OUTPUT" 2> /dev/null &
TAIL=$!

stop() {
  kill "$TAIL" 2> /dev/null || true
  pkill -f "$EXECUTABLE" 2> /dev/null || true
  rm -f "$OUTPUT"
}
trap stop EXIT
trap 'exit 130' INT TERM

# Waited for in the background, so that a signal to this script is handled at
# once instead of after the app quits. (The expansion is spelled this way for
# the bash 3.2 that ships with macOS, which takes an empty array for an unset
# variable.)
open -W -n "$APP" --stdout "$OUTPUT" --stderr "$OUTPUT" \
  ${ENVIRONMENT[@]+"${ENVIRONMENT[@]}"} --args "$@" &
wait $!
