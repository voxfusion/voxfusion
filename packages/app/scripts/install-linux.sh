#!/usr/bin/env bash
set -euo pipefail
VF_SOURCE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
VF_PREFIX="${1:-$HOME/.local}"
if [[ ! -f "$VF_SOURCE/voxfusion-app" ]]; then
  echo "Run install.sh from the extracted VoxFusion Linux archive." >&2
  exit 1
fi
if [[ "$VF_PREFIX" != /* || "$VF_PREFIX" == *$'\n'* || "$VF_PREFIX" == *'"'* || "$VF_PREFIX" == *'`'* || "$VF_PREFIX" == *'\'* || "$VF_PREFIX" == *'$'* || "$VF_PREFIX" == *'%'* ]]; then
  echo "Use an absolute install path without shell or desktop-entry metacharacters." >&2
  exit 1
fi
install -Dm755 "$VF_SOURCE/voxfusion-app" "$VF_PREFIX/bin/voxfusion-app"
install -Dm644 "$VF_SOURCE/icon.png" "$VF_PREFIX/share/icons/hicolor/512x512/apps/io.voxfusion.app.png"
mkdir -p "$VF_PREFIX/share/applications"
while IFS= read -r line; do
  if [[ "$line" == Exec=* ]]; then
    printf 'Exec="%s/bin/voxfusion-app"\n' "$VF_PREFIX"
  else
    printf '%s\n' "$line"
  fi
done < "$VF_SOURCE/io.voxfusion.app.desktop" > "$VF_PREFIX/share/applications/io.voxfusion.app.desktop"
if command -v update-desktop-database >/dev/null; then
  update-desktop-database "$VF_PREFIX/share/applications"
fi
echo "Installed. Launch VoxFusion from the application menu or $VF_PREFIX/bin/voxfusion-app"
