#!/usr/bin/env bash
# Wypisuje notatki wydania: sekcję `## [<wersja>]` z CHANGELOG.md (bez nagłówka sekcji).
# Użycie: tools/wydanie-notatki.sh <wersja> [CHANGELOG.md]
set -euo pipefail

version="${1:?podaj wersję, np. 0.1.0}"
file="${2:-CHANGELOG.md}"

notes="$(awk -v v="$version" '
  /^## \[/ {
    if (found) exit
    if (index($0, "## [" v "]") == 1) { found = 1; next }
  }
  found { print }
' "$file")"

# Bez pustych wierszy na początku i końcu.
notes="$(printf '%s\n' "$notes" | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}' | sed '/./,$!d')"
if [ -z "$notes" ]; then
  echo "brak sekcji [$version] w $file (albo jest pusta)" >&2
  exit 1
fi
printf '%s\n' "$notes"
