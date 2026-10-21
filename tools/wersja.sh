#!/usr/bin/env bash
# Wypisuje wersję programu z Cargo.toml ([workspace.package]). Jeśli podano tag (np. v0.1.0),
# sprawdza, że zgadza się z wersją, i kończy błędem, gdy nie.
# Użycie: tools/wersja.sh [tag]
set -euo pipefail

cd "$(dirname "$0")/.."
version="$(awk '/^\[workspace.package\]/ {on=1; next} /^\[/ {on=0} on && /^version[ ]*=/ {gsub(/[" ]/, "", $0); sub(/^version=/, "", $0); print; exit}' Cargo.toml)"
if [ -z "$version" ]; then
  echo "nie znaleziono wersji w Cargo.toml" >&2
  exit 1
fi
if [ -n "${1:-}" ] && [ "${1#v}" != "$version" ]; then
  echo "tag $1 nie zgadza się z wersją $version z Cargo.toml (podnieś wersję przed tagiem)" >&2
  exit 1
fi
echo "$version"
