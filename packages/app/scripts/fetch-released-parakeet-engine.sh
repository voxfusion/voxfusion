#!/usr/bin/env bash
#
# Restores the Parakeet engine from a published release instead of compiling
# it, which takes ~2h in CI. Release app bundles carry exactly the files
# build-parakeet-engine.sh installs, in Contents/Resources/bin, so a release
# built from an identical build script ships the same engine. That is also
# what the engine cache key (the build script's hash) assumes.
#
# Usage: packages/app/scripts/fetch-released-parakeet-engine.sh <staging_dir>
# Fills <staging_dir>/aarch64-apple-darwin and <staging_dir>/x86_64-apple-darwin.
# Exits non-zero when no release matches, so the caller compiles instead.
#
# Env:
#   GH_TOKEN           token for the gh CLI
#   GITHUB_REPOSITORY  owner/repo whose releases are searched
set -euo pipefail

STAGING_DIR="${1:?usage: $0 <staging_dir>}"
REPO="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
BUILD_SCRIPT="packages/app/scripts/build-parakeet-engine.sh"
# The files build-parakeet-engine.sh installs.
ENGINE_FILES=(crispasr libcrispasr.1.dylib libggml.0.dylib libggml-base.0.dylib
  libggml-cpu.0.dylib libggml-metal.0.dylib libggml-blas.0.dylib)

WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT

# Copies the engine out of an app tarball and checks its architecture.
unpack() { # <tarball> <rust target> <arch>
  local tarball="$1" dest="$STAGING_DIR/$2" arch="$3" unpacked="$WORK_DIR/$2" file
  mkdir -p "$unpacked"
  tar -xzf "$tarball" -C "$unpacked" || return 1
  rm -rf "$dest"
  mkdir -p "$dest"
  for file in "${ENGINE_FILES[@]}"; do
    cp "$unpacked"/*.app/Contents/Resources/bin/"$file" "$dest/" || return 1
    lipo -archs "$dest/$file" | grep -qw "$arch" || return 1
  done
}

restore_from() { # <tag>
  local tag="$1"
  rm -rf "${WORK_DIR:?}"/*
  gh release download "$tag" -R "$REPO" -D "$WORK_DIR" \
    -p VoxFusion.app.tar.gz -p VoxFusion-intel.app.tar.gz || return 1
  unpack "$WORK_DIR/VoxFusion.app.tar.gz" aarch64-apple-darwin arm64 || return 1
  unpack "$WORK_DIR/VoxFusion-intel.app.tar.gz" x86_64-apple-darwin x86_64 || return 1
}

script_sha=$(git hash-object "$BUILD_SCRIPT")
tags=$(gh release list -R "$REPO" --exclude-drafts --exclude-pre-releases --limit 20 \
  --json tagName --jq '.[].tagName')

for tag in $tags; do
  release_sha=$(gh api "repos/$REPO/contents/$BUILD_SCRIPT?ref=$tag" --jq .sha 2>/dev/null) || continue
  [ "$release_sha" = "$script_sha" ] || continue
  echo "==> $tag was built from the same engine script; reusing its engine"
  if restore_from "$tag"; then
    echo "==> Engine restored from $tag into $STAGING_DIR"
    exit 0
  fi
  echo "::warning::Could not restore the Parakeet engine from $tag"
done

rm -rf "$STAGING_DIR"
echo "No published release was built from the current $BUILD_SCRIPT"
exit 1
