# bluepane

Dwupanelowy menedżer plików w stylu Total Commandera. Rust + egui, natywny, sterowany
klawiaturą, działa na **Windows i Linuksie**. Projekt we wczesnej fazie (szkielet, etap 1 z 9).

## Uruchomienie

Wymagany Rust 1.85 lub nowszy (`rustup`).

```
cargo run --release -p bluepane              # oba panele w bieżącym katalogu
cargo run --release -p bluepane -- ~/a ~/b   # lewy i prawy katalog
```

Linux: do zbudowania okna potrzebne są biblioteki systemowe (przykład dla Debiana/Ubuntu):

```
sudo apt install libxkbcommon-dev libwayland-dev libx11-dev libxcb1-dev libgtk-3-dev
```

## Klawisze

| Klawisz | Akcja |
| --- | --- |
| Tab | przełącz panel |
| Strzałki, PageUp/PageDown | kursor |
| Enter / Backspace | wejdź do katalogu / w górę (wiersz `..` też wychodzi w górę) |
| Alt+← / Alt+→ | historia katalogów: wstecz / naprzód (osobna dla każdego panelu) |
| litery | szybki filtr listy (nazwa zawiera wpisany tekst); Backspace skraca filtr, Esc czyści |
| Enter na archiwum | wejdź do archiwum (zip, tar, tar.gz, tgz, `*.zip.001`); `..` na górze wychodzi |
| F5 w archiwum | rozpakuj zaznaczone do katalogu drugiego panelu |
| F3 w archiwum | podgląd pliku z archiwum (rozpakowuje go do katalogu tymczasowego, który znika po zamknięciu podglądu; limit 256 MB, większe rozpakuj F5) |
| Alt+F5 / Alt+F9 | pakuj (okno) / rozpakuj archiwum pod kursorem lub otwarte |
| Ctrl+T / Ctrl+W | nowa zakładka (ten sam katalog) / zamknij zakładkę |
| Ctrl+↑ | katalog pod kursorem w nowej zakładce |
| Ctrl+Tab, Ctrl+PgDn / Ctrl+Shift+Tab, Ctrl+PgUp | następna / poprzednia zakładka |
| Ctrl+1 … Ctrl+9 | przejdź do zakładki o tym numerze |
| Alt+1 … Alt+9 | skok aktywnego panelu do wolumenu o tym numerze |
| Ctrl+A / Ctrl+Shift+A | zaznacz wszystko / odznacz wszystko |
| Ctrl+I | odwróć zaznaczenie |
| Ctrl+M / Ctrl+Shift+M | zaznacz / odznacz po masce (np. `*.txt;*.md`, `?` = jeden znak) |
| Ctrl+F3 … F6 | sortuj po nazwie / rozszerzeniu / dacie / rozmiarze (drugi raz = odwrotnie); klik w nagłówek kolumny też |
| Ctrl+L | wiersz poleceń z `cd ` (przejście do wpisanej ścieżki) |
| Spacja, Insert | zaznacz i przejdź niżej |
| F2 | zmień nazwę (okno z bieżącą nazwą; nie nadpisuje istniejących) |
| F3 | podgląd pliku (tekst lub hex, dowolnie duży) |
| F4 | edycja w zewnętrznym edytorze (`BLUEPANE_EDITOR`, np. `code -w`; domyślnie aplikacja systemu) |
| F5 / F6 | kopiuj / przenieś do drugiego panelu; F5 przy tym samym katalogu w obu panelach proponuje kopię „nazwa (kopia).ext” (Enter = tak, Esc = nie) |
| F7 | nowy katalog (okno z polem na nazwę) |
| F8 | do kosza (po potwierdzeniu: Enter = tak, Esc = nie) |
| F9 | odśwież |

Każdy panel ma własny rząd zakładek (klik przełącza, × albo środkowy przycisk zamyka, + dodaje); zakładka pamięta katalog, historię, sortowanie, filtr i zaznaczenie. Nad każdym panelem jest lista wolumenów i edytowalny pasek ścieżki (Enter wykonuje `cd`).

Wiersz poleceń na dole przyjmuje te same komendy: `cd <ścieżka>`, `back`, `forward`,
`volume <numer|nazwa>`, `tab <new|open|close [n]|next|prev|n>`, `filter <tekst>`, `mkdir <nazwa>`, `copy`, `move`, `delete`,
`selmask <maska>`, `unselmask <maska>`, `selectall`, `unselectall`, `invert`,
`sort <name|ext|size|date> [asc|desc]`, `pack`, `unpack`, `testarchive`, `select <nazwa>`, `cursor <n>`, `rename [nazwa]`, `confirm` / `cancel` (odpowiedź na pytanie), `stop` (przerwij zadanie),
`overwrite`, `newer` (nadpisz starsze), `skip`, `keepboth` (z końcówką `-all` = dla wszystkich;
odpowiedzi na „plik już istnieje”), `refresh`.

## Podgląd plików (F3)

Podgląd czyta tylko widoczne okno pliku, więc plik wielogigabajtowy otwiera się od razu i nie
zajmuje pamięci. Pliki z bajtami zerowymi otwierają się w trybie hex.

| Klawisz | Akcja |
| --- | --- |
| strzałki góra/dół, PgUp/PgDn, Spacja | przewijanie o wiersz / stronę |
| Home / End | początek / koniec pliku |
| strzałki lewo/prawo | przesunięcie w bok (długie linie) |
| H lub F4 | przełącz tekst / hex |
| `/`, F7 lub Ctrl+F | szukaj (bez rozróżniania wielkości liter, ASCII) |
| N / Shift+N (F3 / Shift+F3) | następne / poprzednie trafienie |
| Esc lub Q | zamknij podgląd |

Szukanie przeszukuje do 256 MiB na raz; jeśli nic nie znajdzie w tej części, pokazuje dokąd doszło
i kolejne **N** szuka dalej. Komendy: `view`, `viewer scroll|page|hscroll|top|bottom|mode|goto|find|next|prev|close`.

## Archiwa

Obsługiwane formaty: **ZIP**, **TAR**, **TAR.GZ** (`.tgz`). Archiwum otwiera się w panelu jak
katalog (tylko do odczytu): F5 rozpakowuje zaznaczone pliki i katalogi do drugiego panelu, a
**Alt+F9** (`unpack`) całe archiwum (`unpack -s` do podkatalogu o nazwie archiwum).

**Pakowanie** (**Alt+F5** lub przycisk „Pakuj”) otwiera okno: nazwa, format, kompresja (bez /
szybka / normalna / maksymalna), hasło, podział na części. Archiwum powstaje w katalogu drugiego
panelu. To samo z linii poleceń, z przełącznikami jak w 7-Zip:

```
pack raport.zip -psekret -mx=9 -v100m     # ZIP z hasłem, maksymalna kompresja, części po 100 MB
pack kopia -ttgz                          # TAR.GZ o nazwie wziętej z nazwy
```

| Przełącznik | Znaczenie |
| --- | --- |
| `-tzip`, `-ttar`, `-ttgz` | format (domyślnie z rozszerzenia nazwy, inaczej ZIP) |
| `-mx=0` … `-mx=9` | kompresja: 0 = bez, 1–3 szybka, 4–6 normalna, 7–9 maksymalna |
| `-p<hasło>` | hasło (tylko ZIP, szyfrowanie AES-256) |
| `-v<rozmiar>` | podział na części `nazwa.zip.001`, `.002`, … (np. `-v700m`, `-v4g`) |

- **Hasła** działają tylko w ZIP (TAR nie szyfruje, więc hasło dla TAR jest odrzucane, a nie
  ignorowane). Czyta się ZipCrypto i AES, tworzy AES-256 (zgodne z 7-Zip). Przy zaszyfrowanym
  archiwum okno pyta o hasło, a po błędnym pyta ponownie. Nazw plików w ZIP nie da się ukryć.
- **Części**: odczyt wskazuje się na pierwszej części (`*.001`), reszta jest doczytywana
  automatycznie. Podział potrzebuje na dysku miejsca na archiwum dwukrotnie w trakcie pracy.
- `testarchive` sprawdza integralność archiwum. Dowiązania symboliczne i pliki urządzeń są
  pomijane przy rozpakowaniu (z informacją), a nazwy wychodzące poza katalog docelowy
  (`../x`, ścieżki bezwzględne) są odrzucane.
- Istniejące archiwum nie jest nadpisywane bez pytania (nadpisz / pomiń / zachowaj oba).

## Kopiowanie i przenoszenie

F5/F6 działają w tle: pasek postępu pod panelami pokazuje bajty, bieżący plik i przycisk
**Anuluj** (niepełne pliki są usuwane). Kolejne operacje ustawiają się w kolejce. Gdy plik już
istnieje w celu, okno pyta: **Nadpisz**, **Nadpisz starsze** (tylko gdy nowy plik ma nowszą datę),
**Pomiń**, **Zachowaj oba** (nowy dostaje nazwę `plik (kopia).ext`) albo **Anuluj**. Pole „Dla
wszystkich kolejnych plików” stosuje wybór do reszty operacji. Katalogi istniejące po obu stronach
są scalane. Nadpisywanie nigdy nie następuje bez Twojej decyzji.

## Bez okna

```
cargo run -p bluepane-cli -- commands
cargo run -p bluepane-cli -- ls <katalog>
cargo run -p bluepane-cli -- run <lewy> <prawy> "select a.txt" "copy"
cargo run -p bluepane-cli -- run <lewy> <prawy> "select a.txt" "delete" "confirm"
```

## Dla developerów

```
cargo xtask ci    # fmt, clippy, testy, reguły warstw (to samo robi GitHub Actions)
```

Układ crate'ów, zasady i sposób pracy z Claude Code: [CLAUDE.md](CLAUDE.md). Plan i stan prac:
[ROADMAP.md](ROADMAP.md).

## Licencja

MIT lub Apache-2.0, do wyboru. Nazwa, kod i zasoby są niezależne od Total Commandera.
