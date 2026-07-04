#!/usr/bin/env bash
#
# Builds and installs the `crispasr` engine used to run Parakeet GGUF models
# on-device (handlers::parakeet). whisper-rs cannot run Parakeet (a TDT
# transducer), so transcription is delegated to CrispStrobe/CrispASR — the
# ggml/Metal C++ runtime the cstr/parakeet-tdt-0.6b-v3-GGUF files target.
#
# The result (binary + its ggml/crispasr dylibs) is installed into the dest
# dir, which handlers::parakeet::resolve_binary discovers at runtime:
#   - default (dev): the app's data dir under `bin/`
#   - release CI: a staging dir copied to `src-tauri/engine/`, which
#     tauri.conf.json bundles into `Contents/Resources/bin`
#
# Usage: packages/app/scripts/build-parakeet-engine.sh [dest_bin_dir]
# Default dest: ~/Library/Application Support/io.voxfusion.app/bin  (macOS)
#
# Env:
#   PARAKEET_ARCH        target arch: arm64 | x86_64 (default: host arch)
#   PARAKEET_ENGINE_REF  CrispASR commit to build (default: pinned below)
#   PARAKEET_BUILD_DIR   scratch dir for the clone/build (default: /tmp/CrispASR)
set -euo pipefail

REPO_URL="https://github.com/CrispStrobe/CrispASR"
# Pinned engine commit. Bump deliberately and re-test Parakeet transcription
# end-to-end before releasing.
ENGINE_REF="${PARAKEET_ENGINE_REF:-2cb51b114a03005195066c16b7a30de51e60506f}"
WORK_DIR="${PARAKEET_BUILD_DIR:-/tmp/CrispASR}"
DEST_DIR="${1:-$HOME/Library/Application Support/io.voxfusion.app/bin}"
ARCH="${PARAKEET_ARCH:-$(uname -m)}"

echo "==> Fetching $REPO_URL @ $ENGINE_REF into $WORK_DIR"
rm -rf "$WORK_DIR"
git init -q "$WORK_DIR"
git -C "$WORK_DIR" remote add origin "$REPO_URL"
git -C "$WORK_DIR" fetch -q --depth 1 origin "$ENGINE_REF"
git -C "$WORK_DIR" checkout -q FETCH_HEAD

echo "==> Configuring (Release; $ARCH; tests/server off)"
# GGML_NATIVE=OFF: the binary ships to other machines, so it must not be tuned
# to the build host's CPU (it is also required for cross-arch builds).
cmake -S "$WORK_DIR" -B "$WORK_DIR/build" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_OSX_ARCHITECTURES="$ARCH" \
  -DCMAKE_OSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}" \
  -DGGML_NATIVE=OFF \
  -DCRISPASR_BUILD_TESTS=OFF \
  -DCRISPASR_BUILD_SERVER=OFF

echo "==> Building crispasr CLI (this is a large compile)"
cmake --build "$WORK_DIR/build" -j --target crispasr-cli

echo "==> Installing engine into: $DEST_DIR"
mkdir -p "$DEST_DIR"
cp "$WORK_DIR/build/bin/crispasr" "$DEST_DIR/"
# The CLI is a thin shell over @rpath dylibs; collect the closure it needs.
cp "$WORK_DIR/build/src/libcrispasr.1.dylib" "$DEST_DIR/"
cp "$WORK_DIR/build/ggml/src/libggml.0.dylib" "$DEST_DIR/"
cp "$WORK_DIR/build/ggml/src/libggml-base.0.dylib" "$DEST_DIR/"
cp "$WORK_DIR/build/ggml/src/libggml-cpu.0.dylib" "$DEST_DIR/"
cp "$WORK_DIR/build/ggml/src/ggml-metal/libggml-metal.0.dylib" "$DEST_DIR/"
cp "$WORK_DIR/build/ggml/src/ggml-blas/libggml-blas.0.dylib" "$DEST_DIR/"

# Resolve @rpath against each file's own directory so the folder is portable
# (independent of the build tree). Idempotent: ignore "already present" errors.
# Signing (release CI) must happen after this — it invalidates signatures.
for f in crispasr libcrispasr.1.dylib libggml.0.dylib libggml-base.0.dylib \
         libggml-cpu.0.dylib libggml-metal.0.dylib libggml-blas.0.dylib; do
  install_name_tool -add_rpath @loader_path "$DEST_DIR/$f" 2>/dev/null || true
done

echo "==> Done. Engine installed at: $DEST_DIR/crispasr"
echo "    Quick test:"
echo "    \"$DEST_DIR/crispasr\" -m <parakeet-*.gguf> -f audio.wav -t 8 -nt -np"
