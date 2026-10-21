#!/usr/bin/env bash
# Buduje AppImage z gotowych programów. Używa `appimagetool` w przypiętej wersji (pobierany
# i sprawdzany sumą SHA-256; nic z tego nie trafia do Cargo.toml). Biblioteki systemowe (GTK,
# Wayland, X11) są po stronie systemu użytkownika, nie w pliku.
# Użycie: tools/pakiet-appimage.sh <wersja> <katalog-z-programami> <katalog-wyjściowy>
set -euo pipefail

version="${1:?podaj wersję}"
bins="${2:?podaj katalog z programami}"
out="${3:?podaj katalog wyjściowy}"
repo="$(cd "$(dirname "$0")/.." && pwd)"

TOOL_VERSION="1.9.0"
TOOL_URL="https://github.com/AppImage/appimagetool/releases/download/$TOOL_VERSION/appimagetool-x86_64.AppImage"
TOOL_SHA256="46fdd785094c7f6e545b61afcfb0f3d98d8eab243f644b4b17698c01d06083d1"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

tool="${APPIMAGETOOL:-}"
if [ -z "$tool" ]; then
  tool="$work/appimagetool"
  curl -sSL --fail -o "$tool" "$TOOL_URL"
  echo "$TOOL_SHA256  $tool" | sha256sum -c - >/dev/null || {
    echo "suma SHA-256 appimagetool się nie zgadza (oczekiwano $TOOL_SHA256)" >&2
    exit 1
  }
  chmod +x "$tool"
fi

app="$work/bluepane.AppDir"
install -d "$app/usr/bin" "$app/usr/share/doc/bluepane"
install -m 0755 "$bins/bluepane" "$bins/bluepane-cli" "$app/usr/bin/"
install -m 0644 "$repo/installer/linux/bluepane.desktop" "$app/bluepane.desktop"
install -m 0644 "$repo/installer/linux/bluepane.png" "$app/bluepane.png"
install -m 0644 "$repo/LICENSE-MIT" "$repo/LICENSE-APACHE" "$repo/README.md" "$repo/CHANGELOG.md" \
  "$app/usr/share/doc/bluepane/"
cat > "$app/AppRun" <<'RUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="$HERE/usr/bin:$PATH"
exec "$HERE/usr/bin/bluepane" "$@"
RUN
chmod 0755 "$app/AppRun"

mkdir -p "$out"
file="$out/bluepane-$version-linux-x86_64.AppImage"
# `--appimage-extract-and-run`: działa bez FUSE (runner CI, kontenery).
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$tool" "$app" "$file" >/dev/null
chmod 0755 "$file"
echo "$file"
