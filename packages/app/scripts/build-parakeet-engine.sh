#!/usr/bin/env bash
#
# Builds and installs the `crispasr` engine used to run Parakeet GGUF models
# on-device (handlers::parakeet). whisper-rs cannot run Parakeet (a TDT
# transducer), so transcription is delegated to CrispStrobe/CrispASR — the
# ggml/Metal C++ runtime the cstr/parakeet-tdt-0.6b-v3-GGUF files target.
#
# The result (binary + its ggml/crispasr dylibs) is installed into the app's
# data dir under `bin/`, which handlers::parakeet::resolve_binary discovers at
# runtime. Re-run this after pulling a new engine version.
#
# Usage: packages/app/scripts/build-parakeet-engine.sh [dest_bin_dir]
# Default dest: ~/Library/Application Support/io.voxfusion.app/bin  (macOS)
set -euo pipefail

REPO_URL="https://github.com/CrispStrobe/CrispASR"
WORK_DIR="${PARAKEET_BUILD_DIR:-/tmp/CrispASR}"
DEST_DIR="${1:-$HOME/Library/Application Support/io.voxfusion.app/bin}"

echo "==> Cloning $REPO_URL into $WORK_DIR"
rm -rf "$WORK_DIR"
git clone --depth 1 "$REPO_URL" "$WORK_DIR"

echo "==> Configuring (Release; tests/server off)"
cmake -S "$WORK_DIR" -B "$WORK_DIR/build" \
  -DCMAKE_BUILD_TYPE=Release \
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
for f in crispasr libcrispasr.1.dylib libggml.0.dylib libggml-base.0.dylib \
         libggml-cpu.0.dylib libggml-metal.0.dylib libggml-blas.0.dylib; do
  install_name_tool -add_rpath @loader_path "$DEST_DIR/$f" 2>/dev/null || true
done

echo "==> Done. Engine installed at: $DEST_DIR/crispasr"
echo "    Quick test:"
echo "    \"$DEST_DIR/crispasr\" -m <parakeet-*.gguf> -f audio.wav -t 8 -nt -np"
