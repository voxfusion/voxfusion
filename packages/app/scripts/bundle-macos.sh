#!/usr/bin/env bash
#
# Builds VoxFusion.app and, on request, the files a release publishes.
#
# Usage: packages/app/scripts/bundle-macos.sh [--target <rust target>] [--debug]
#                                             [--engine-dir <dir>] [--dmg] [--updater]
#
#   --target      aarch64-apple-darwin | x86_64-apple-darwin (default: the
#                 host, built in the same directory `cargo build` uses)
#   --debug       bundle the debug build instead of the release build
#   --engine-dir  directory with the Parakeet engine files to put in
#                 Contents/Resources/bin (see build-parakeet-engine.sh)
#   --dmg         also create VoxFusion.dmg
#   --updater     also create VoxFusion.app.tar.gz and its signature, in the
#                 format installed copies download and verify
#
# Env:
#   SIGN_IDENTITY   codesign identity (default "-": ad-hoc, no hardened runtime).
#                   A release build signed with a real identity gets the
#                   hardened runtime and a timestamp, as notarization requires.
#   HARDENED_RUNTIME=1  sign with the hardened runtime even ad-hoc, to test
#                   that the entitlements cover what the app does
#   VERSION         version written to Info.plist (default: Cargo.toml's)
#   BUNDLE_ID       bundle identifier (default io.voxfusion.app). A different
#                   one gives macOS a separate identity for permissions; pair
#                   it with VOXFUSION_APP_ID at run time for separate data.
#   APP_NAME        bundle name (default VoxFusion)
#   NOTARIZE=1      notarize and staple the app (needs APPLE_ID,
#                   APPLE_PASSWORD, APPLE_TEAM_ID)
#   TAURI_SIGNING_PRIVATE_KEY, TAURI_SIGNING_PRIVATE_KEY_PASSWORD
#                   the update signing key, required by --updater. The key
#                   keeps its Tauri name and format so existing installs
#                   accept the update.
#
# Output: packages/app/dist/<target>/
set -euo pipefail

APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET=""
PROFILE="release"
ENGINE_DIR=""
MAKE_DMG=0
MAKE_UPDATER=0

while [ $# -gt 0 ]; do
  case "$1" in
    --target) TARGET="$2"; shift 2 ;;
    --debug) PROFILE="debug"; shift ;;
    --engine-dir) ENGINE_DIR="$2"; shift 2 ;;
    --dmg) MAKE_DMG=1; shift ;;
    --updater) MAKE_UPDATER=1; shift ;;
    *) echo "Unknown argument: $1" >&2; exit 1 ;;
  esac
done

BUILD_DIR="${CARGO_TARGET_DIR:-target}"
BUILD_FLAGS=()

if [ -n "$TARGET" ]; then
  BUILD_DIR="$BUILD_DIR/$TARGET"
  BUILD_FLAGS+=(--target "$TARGET")
else
  # Names the output directory only: the host build shares its artifacts
  # with `cargo build`, `cargo check` and `cargo test`.
  case "$(uname -m)" in
    arm64) TARGET="aarch64-apple-darwin" ;;
    x86_64) TARGET="x86_64-apple-darwin" ;;
    *) echo "Unsupported host architecture: $(uname -m)" >&2; exit 1 ;;
  esac
fi

if [ "$PROFILE" = "release" ]; then
  BUILD_FLAGS+=(--release)
fi

SIGN_IDENTITY="${SIGN_IDENTITY:--}"
APP_NAME="${APP_NAME:-VoxFusion}"
BUNDLE_ID="${BUNDLE_ID:-io.voxfusion.app}"
VERSION="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$APP_DIR/Cargo.toml" | head -1)}"

OUT_DIR="$APP_DIR/dist/$TARGET"
APP="$OUT_DIR/$APP_NAME.app"

cd "$APP_DIR"

# The version compiled into the binary (shown in Settings, compared by the
# updater) comes from Cargo.toml, so a release sets it there before building.
# (The expansion is spelled this way for the bash 3.2 that ships with macOS,
# which takes an empty array for an unset variable.)
cargo build ${BUILD_FLAGS[@]+"${BUILD_FLAGS[@]}"}

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$BUILD_DIR/$PROFILE/voxfusion-app" "$APP/Contents/MacOS/voxfusion-app"
chmod 755 "$APP/Contents/MacOS/voxfusion-app"
cp macos/icon.icns "$APP/Contents/Resources/icon.icns"

sed "s/__VERSION__/$VERSION/g" macos/Info.plist > "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $BUNDLE_ID" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleName $APP_NAME" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName $APP_NAME" "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"

SIGN_FLAGS=(--force --sign "$SIGN_IDENTITY")
if [ "$SIGN_IDENTITY" != "-" ] && [ "$PROFILE" = "release" ]; then
  # Notarization requires the hardened runtime and a secure timestamp.
  SIGN_FLAGS+=(--options runtime --timestamp)
elif [ "${HARDENED_RUNTIME:-0}" = "1" ]; then
  SIGN_FLAGS+=(--options runtime)
fi

if [ -n "$ENGINE_DIR" ]; then
  mkdir -p "$APP/Contents/Resources/bin"
  cp "$ENGINE_DIR"/* "$APP/Contents/Resources/bin/"

  # Every Mach-O in the bundle has to be signed before the bundle itself.
  for file in "$APP/Contents/Resources/bin"/*; do
    codesign "${SIGN_FLAGS[@]}" "$file"
  done
fi

# The entitlements are what let the hardened runtime use the microphone and
# send Apple Events; without them macOS denies both without asking.
codesign "${SIGN_FLAGS[@]}" --entitlements macos/entitlements.plist "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"

if [ "${NOTARIZE:-0}" = "1" ]; then
  NOTARY_ZIP="$OUT_DIR/notarize.zip"
  ditto -c -k --sequesterRsrc --keepParent "$APP" "$NOTARY_ZIP"
  xcrun notarytool submit "$NOTARY_ZIP" \
    --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait
  rm -f "$NOTARY_ZIP"
  xcrun stapler staple "$APP"
  xcrun stapler validate "$APP"
fi

if [ "$MAKE_UPDATER" = "1" ]; then
  TARBALL="$OUT_DIR/$APP_NAME.app.tar.gz"
  rm -f "$TARBALL" "$TARBALL.sig"
  # COPYFILE_DISABLE keeps AppleDouble (._*) files out of the archive; they
  # would invalidate the signature of the extracted bundle.
  COPYFILE_DISABLE=1 tar -czf "$TARBALL" -C "$OUT_DIR" "$APP_NAME.app"

  # Installed copies verify updates against the key pair the Tauri builds
  # used, so the archive is signed with the same tool and key.
  bunx --bun @tauri-apps/cli@2 signer sign \
    --private-key "$TAURI_SIGNING_PRIVATE_KEY" \
    --password "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" \
    "$TARBALL"
  test -s "$TARBALL.sig"
fi

if [ "$MAKE_DMG" = "1" ]; then
  DMG="$OUT_DIR/$APP_NAME.dmg"
  DMG_SOURCE="$(mktemp -d)"
  trap 'rm -rf "$DMG_SOURCE"' EXIT

  cp -R "$APP" "$DMG_SOURCE/$APP_NAME.app"
  ln -s /Applications "$DMG_SOURCE/Applications"

  rm -f "$DMG"
  hdiutil create -volname "$APP_NAME" -srcfolder "$DMG_SOURCE" -ov -format UDZO "$DMG"

  if [ "$SIGN_IDENTITY" != "-" ]; then
    codesign --force --timestamp --sign "$SIGN_IDENTITY" "$DMG"
  fi
fi

echo "Built $APP"
