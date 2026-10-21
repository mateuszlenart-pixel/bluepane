#!/usr/bin/env bash
# Buduje paczkę .deb z gotowych programów (bez cargo-deb: tylko dpkg-deb).
# Użycie: tools/pakiet-deb.sh <wersja> <katalog-z-programami> <katalog-wyjściowy>
# Katalog z programami ma zawierać `bluepane` i `bluepane-cli`; w katalogu repozytorium leżą
# licencje, README, docs/ i installer/linux/. Wynik: <wyjście>/bluepane-<wersja>-linux-amd64.deb
set -euo pipefail

version="${1:?podaj wersję}"
bins="${2:?podaj katalog z programami}"
out="${3:?podaj katalog wyjściowy}"
repo="$(cd "$(dirname "$0")/.." && pwd)"

root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
pkg="$root/bluepane"
install -d "$pkg/DEBIAN" "$pkg/usr/bin" "$pkg/usr/share/applications" \
  "$pkg/usr/share/icons/hicolor/256x256/apps" "$pkg/usr/share/doc/bluepane"

install -m 0755 "$bins/bluepane" "$bins/bluepane-cli" "$pkg/usr/bin/"
install -m 0644 "$repo/installer/linux/bluepane.desktop" "$pkg/usr/share/applications/bluepane.desktop"
install -m 0644 "$repo/installer/linux/bluepane.png" "$pkg/usr/share/icons/hicolor/256x256/apps/bluepane.png"
install -m 0644 "$repo/LICENSE-MIT" "$repo/LICENSE-APACHE" "$repo/README.md" "$repo/CHANGELOG.md" \
  "$pkg/usr/share/doc/bluepane/"
cp -r "$repo/docs" "$pkg/usr/share/doc/bluepane/docs"
# Plik `copyright` w tradycyjnym, krótkim zapisie: licencje są obok.
cat > "$pkg/usr/share/doc/bluepane/copyright" <<'COPY'
bluepane jest dostępny na licencji MIT albo Apache-2.0 (do wyboru).
Pełne teksty: /usr/share/doc/bluepane/LICENSE-MIT i /usr/share/doc/bluepane/LICENSE-APACHE.
COPY
find "$pkg/usr/share/doc" -type d -exec chmod 0755 {} +
find "$pkg/usr/share/doc" -type f -exec chmod 0644 {} +

size_kb="$(du -sk "$pkg/usr" | cut -f1)"
cat > "$pkg/DEBIAN/control" <<CONTROL
Package: bluepane
Version: $version
Section: utils
Priority: optional
Architecture: amd64
Maintainer: bluepane <https://github.com/mateuszlenart-pixel/bluepane>
Installed-Size: $size_kb
Depends: libc6 (>= 2.35), libgcc-s1, libxkbcommon0, libwayland-client0 | libx11-6, libgl1
Recommends: git, xdg-utils
Suggests: smbclient, rclone, openssh-client
Homepage: https://github.com/mateuszlenart-pixel/bluepane
Description: dwupanelowy menedzer plikow w stylu Total Commandera
 Rust i egui, Windows i Linux. Dwa panele, operacje w tle, archiwa, szukanie,
 synchronizacja, SFTP/FTP/SMB/rclone, podglad plikow. Program nie sprawdza
 aktualizacji sam.
CONTROL

mkdir -p "$out"
file="$out/bluepane-$version-linux-amd64.deb"
fakeroot dpkg-deb --root-owner-group --build "$pkg" "$file" >/dev/null
echo "$file"
