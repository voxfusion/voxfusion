#!/usr/bin/env bash
set -euo pipefail
VF_APP_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$VF_APP_DIR"
if [[ "$(uname -s)" != Linux ]]; then
  echo "Build on Linux; the native WebKitGTK libraries are required." >&2
  exit 1
fi
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
export GGML_NATIVE=OFF
bun run tauri build --no-bundle "$@"
VF_TARGET_DIR="${CARGO_TARGET_DIR:-$VF_APP_DIR/src-tauri/target}"
VF_PACKAGE_DIR="$VF_TARGET_DIR/release/voxfusion-linux-$(uname -m)"
mkdir -p "$VF_PACKAGE_DIR"
install -m755 "$VF_TARGET_DIR/release/voxfusion-app" "$VF_PACKAGE_DIR/voxfusion-app"
install -m755 "$VF_APP_DIR/scripts/install-linux.sh" "$VF_PACKAGE_DIR/install.sh"
install -m644 "$VF_APP_DIR/linux/io.voxfusion.app.desktop" "$VF_PACKAGE_DIR/io.voxfusion.app.desktop"
install -m644 "$VF_APP_DIR/src-tauri/icons/icon.png" "$VF_PACKAGE_DIR/icon.png"
install -m644 "$VF_APP_DIR/../../docs/linux.md" "$VF_PACKAGE_DIR/README.md"
install -m644 "$VF_APP_DIR/../../LICENSE" "$VF_PACKAGE_DIR/LICENSE"
tar -C "$(dirname "$VF_PACKAGE_DIR")" -czf "$VF_PACKAGE_DIR.tar.gz" "$(basename "$VF_PACKAGE_DIR")"
echo "Linux package: $VF_PACKAGE_DIR.tar.gz"
