#!/usr/bin/env bash
# Przygotowuje katalog do testów ręcznych bluepane (Linux).
# Użycie: tools/dane-testowe.sh [katalog docelowy]   (domyślnie ~/bluepane-test)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$HERE/../testdata"
[ -d "$SRC" ] || SRC="$HERE/testdata"          # wersja rozpakowana z artefaktu
DEST="${1:-$HOME/bluepane-test}"

if [ -e "$DEST" ] && [ -n "$(ls -A "$DEST" 2>/dev/null)" ]; then
  echo "Katalog $DEST już istnieje i nie jest pusty. Podaj inny albo usuń go ręcznie." >&2
  exit 1
fi
mkdir -p "$DEST"
cp -r "$SRC"/. "$DEST"/

# --- nazwy: do zmiany nazw i sortowania -------------------------------------------------
mkdir -p "$DEST/nazwy"
for i in $(seq -w 1 12); do echo "zdjecie $i" > "$DEST/nazwy/IMG_00$i.JPG"; done
echo x > "$DEST/nazwy/Zażółć gęślą.txt"
echo x > "$DEST/nazwy/plik ze spacjami.txt"
echo x > "$DEST/nazwy/WIELKIE.TXT"
echo x > "$DEST/nazwy/a.txt"; echo x > "$DEST/nazwy/b.txt"

# --- synchronizacja: pary z różnymi datami i rozmiarami ---------------------------------
L="$DEST/synchronizacja/lewy"; R="$DEST/synchronizacja/prawy"
mkdir -p "$L/podkatalog" "$R/podkatalog"
echo "to samo" > "$L/takie-same.txt";            cp -p "$L/takie-same.txt" "$R/"
echo "tylko po lewej" > "$L/tylko-lewy.txt"
echo "tylko po prawej" > "$R/tylko-prawy.txt"
echo "nowsza wersja" > "$L/nowszy-w-lewym.txt";  echo "stara wersja" > "$R/nowszy-w-lewym.txt"
touch -d '2026-10-14 12:00' "$L/nowszy-w-lewym.txt"; touch -d '2026-10-10 12:00' "$R/nowszy-w-lewym.txt"
echo "stara wersja" > "$L/nowszy-w-prawym.txt";  echo "nowsza wersja" > "$R/nowszy-w-prawym.txt"
touch -d '2026-10-10 12:00' "$L/nowszy-w-prawym.txt"; touch -d '2026-10-14 12:00' "$R/nowszy-w-prawym.txt"
echo "krótki" > "$L/inny-rozmiar.txt";           echo "znacznie dłuższa zawartość tego pliku" > "$R/inny-rozmiar.txt"
echo "w podkatalogu" > "$L/podkatalog/a.txt";    echo "w podkatalogu, inaczej" > "$R/podkatalog/a.txt"
touch -d '2026-10-12 08:00' "$L/podkatalog/a.txt" "$R/podkatalog/a.txt"

# --- dużo plików i jeden duży: sortowanie, przewijanie, kolejka, limit prędkości -------------
mkdir -p "$DEST/duzo-plikow"
for i in $(seq -w 1 1500); do echo "plik $i" > "$DEST/duzo-plikow/plik-$i.txt"; done
mkdir -p "$DEST/duze"
echo "Tworzę plik 300 MB (do testu kolejki i limitu prędkości)..."
head -c 300M /dev/urandom > "$DEST/duze/duzy-300MB.bin"
head -c 40M  /dev/urandom > "$DEST/duze/sredni-40MB.bin"

# --- inne przypadki -------------------------------------------------------------------
mkdir -p "$DEST/rozne"
echo "ukryty" > "$DEST/rozne/.ukryty.txt"
echo "tylko do odczytu" > "$DEST/rozne/tylko-do-odczytu.txt"; chmod 444 "$DEST/rozne/tylko-do-odczytu.txt"
echo "cel dowiązania" > "$DEST/rozne/cel.txt"; ln -s cel.txt "$DEST/rozne/dowiazanie.txt"
chmod +x "$DEST/teksty/kod.rs" 2>/dev/null || true
mkdir -p "$DEST/rozne/pusty-katalog"

echo
echo "Gotowe: $DEST"
echo "Uruchom:  bluepane \"$DEST\" \"$DEST/synchronizacja\""
