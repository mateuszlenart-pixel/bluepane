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

# --- kod i Markdown: kolorowanie składni, renderowany Markdown (F3) ---------------------
mkdir -p "$DEST/kod"
cat > "$DEST/kod/przyklad.rs" <<'KONIEC'
//! Przykład do kolorowania składni.
/* komentarz blokowy
   przez kilka linii */
fn main() {
    let tekst = "Zażółć gęślą jaźń"; // komentarz
    let liczba = 42;
    println!("{tekst} {liczba}");
}
KONIEC
cat > "$DEST/kod/skrypt.py" <<'KONIEC'
# komentarz
def powitanie(imie):
    if imie:
        return "Cześć, " + imie  # zażółć
    return None
KONIEC
echo '{"nazwa": "Zażółć", "liczby": [1, 2.5], "aktywny": true, "brak": null}' > "$DEST/kod/dane.json"
printf '# ustawienia\n[sekcja]\nklucz = "wartość"\nliczba = 7\nflaga = true\n' > "$DEST/kod/ustawienia.toml"
printf -- '-- zapytanie\nSELECT imie, COUNT(*) FROM osoby WHERE wiek > 18 GROUP BY imie;\n' > "$DEST/kod/zapytanie.sql"
printf '<!-- strona -->\n<html><body><p class="a">Zażółć gęślą</p></body></html>\n' > "$DEST/kod/strona.html"
cat > "$DEST/kod/README.md" <<'KONIEC'
# Nagłówek pierwszego poziomu

Akapit ze **zdaniem mocnym**, *kursywą*, `kodem w tekście` i [odnośnikiem](https://example.org).
Drugi wiersz tego samego akapitu zawija się do szerokości okna, jeśli jest dostatecznie długi, żeby sprawdzić zawijanie.

## Lista

- pierwszy punkt
- drugi punkt z **mocnym** słowem
  - zagnieżdżony punkt

1. numer jeden
2. numer dwa

> Cytat o zażółć gęślą jaźń
> w dwóch liniach.

---

```rust
let x = 1;
```

| tabela | zostaje | tak jak napisano |
| --- | --- | --- |
KONIEC

# --- hex: edytor hex w podglądzie ------------------------------------------------------
mkdir -p "$DEST/hex"
for i in $(seq 0 255); do printf "\\x$(printf %02x "$i")"; done > "$DEST/hex/maly.bin"
echo "zwykły tekst do przełączenia na hex" > "$DEST/hex/tekst.txt"

# --- duplikaty: te same treści w różnych katalogach ---------------------------------------
mkdir -p "$DEST/duplikaty/a" "$DEST/duplikaty/b" "$DEST/duplikaty/c"
echo "ta sama treść pliku, powtórzona trzy razy" > "$DEST/duplikaty/a/zdjecie.txt"
cp -p "$DEST/duplikaty/a/zdjecie.txt" "$DEST/duplikaty/b/zdjecie-kopia.txt"
cp -p "$DEST/duplikaty/a/zdjecie.txt" "$DEST/duplikaty/c/Zażółć kopia.txt"
touch -d '2026-01-10 10:00' "$DEST/duplikaty/a/zdjecie.txt"
touch -d '2026-05-20 10:00' "$DEST/duplikaty/b/zdjecie-kopia.txt"
touch -d '2026-09-30 10:00' "$DEST/duplikaty/c/Zażółć kopia.txt"
echo "unikalny plik" > "$DEST/duplikaty/a/unikalny.txt"
echo "ABCDEFGHIJKLMNOPQRSTUVWXYZ-1" > "$DEST/duplikaty/a/ten-sam-rozmiar.txt"
echo "ABCDEFGHIJKLMNOPQRSTUVWXYZ-2" > "$DEST/duplikaty/b/ten-sam-rozmiar.txt"

# --- porównanie plików i sumy kontrolne ------------------------------------------------------
mkdir -p "$DEST/porownanie"
printf 'pierwszy wiersz\ndrugi wiersz\ntrzeci wiersz zażółć\nczwarty wiersz\npiąty wiersz\n' > "$DEST/porownanie/lewy.txt"
printf 'pierwszy wiersz\ndrugi wiersz zmieniony\ntrzeci wiersz zażółć\nnowy wiersz\nczwarty wiersz\npiąty wiersz\n' > "$DEST/porownanie/prawy.txt"
head -c 4096 /dev/zero > "$DEST/porownanie/bin1.bin"
{ head -c 2000 /dev/zero; printf '\xff'; head -c 2095 /dev/zero; } > "$DEST/porownanie/bin2.bin"

# --- indeks do szukania: teksty z polskimi znakami i różne kodowania ------------------------------
mkdir -p "$DEST/indeks/głęboko"
echo "Umowa o dzieło: żółw, gęś i jaźń. Termin płatności: 30 dni." > "$DEST/indeks/umowa.txt"
printf '# Notatki\n\nZakupy: mleko, chleb, masło.\n' > "$DEST/indeks/notatki.md"
echo "Plik w głębokim katalogu o słowie wyjątkowe." > "$DEST/indeks/głęboko/Zażółć gęślą.txt"
printf '\xa3\xf3d\x9f noc\xb9\n' > "$DEST/indeks/stary-cp1250.txt"

# --- zdjęcia i utwory: zmiana nazw z EXIF i tagów (kopie z media) ----------------------------------
mkdir -p "$DEST/zdjecia" "$DEST/muzyka"
cp "$DEST/media/zdjecie-exif.jpg" "$DEST/zdjecia/IMG_0001.jpg"
cp "$DEST/media/zdjecie-exif-2.jpg" "$DEST/zdjecia/IMG_0002.jpg"
cp "$DEST/media/zdjecie-bez-exif.jpg" "$DEST/zdjecia/IMG_0003.jpg"
cp "$DEST/media/utwor-tagi.wav" "$DEST/muzyka/01.wav"
cp "$DEST/media/utwor-2.wav" "$DEST/muzyka/02.wav"

# --- repozytorium git: kolumna git, gałąź, git add/commit/diff/log ------------------------------------
if command -v git >/dev/null 2>&1; then
  G="$DEST/repozytorium-git"
  mkdir -p "$G/src"
  git -C "$G" init -q -b main
  git -C "$G" config user.email "test@example.com"
  git -C "$G" config user.name "Tester"
  git -C "$G" config commit.gpgsign false
  echo "Projekt testowy" > "$G/README.md"
  echo 'fn main() { println!("cześć"); }' > "$G/src/main.rs"
  echo "stały plik" > "$G/stale.txt"
  git -C "$G" add . && git -C "$G" commit -q -m "Pierwszy commit"
  echo "dopisek" >> "$G/README.md"                                              # zmieniony (M)
  echo "nowy plik" > "$G/Zażółć nowy.txt"                                       # nieśledzony (?)
  rm "$G/stale.txt"                                                             # usunięty (D)
  echo "kolejny" > "$G/src/dodany.rs"; git -C "$G" add "$G/src/dodany.rs"       # dodany (A)
else
  echo "Brak programu git: pomijam repozytorium-git (testy gita pominąć)."
fi

echo
echo "Gotowe: $DEST"
echo "Uruchom:  bluepane \"$DEST\" \"$DEST/synchronizacja\""
