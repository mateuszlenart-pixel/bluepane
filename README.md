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
| Alt+1 … Alt+9 | skok aktywnego panelu do wolumenu o tym numerze |
| Ctrl+A / Ctrl+Shift+A | zaznacz wszystko / odznacz wszystko |
| Ctrl+I | odwróć zaznaczenie |
| Ctrl+M / Ctrl+Shift+M | zaznacz / odznacz po masce (np. `*.txt;*.md`, `?` = jeden znak) |
| Ctrl+F3 … F6 | sortuj po nazwie / rozszerzeniu / dacie / rozmiarze (drugi raz = odwrotnie); klik w nagłówek kolumny też |
| Ctrl+L | wiersz poleceń z `cd ` (przejście do wpisanej ścieżki) |
| Spacja, Insert | zaznacz i przejdź niżej |
| F2 | zmień nazwę (okno z bieżącą nazwą; nie nadpisuje istniejących) |
| F5 / F6 | kopiuj / przenieś do drugiego panelu; F5 przy tym samym katalogu w obu panelach proponuje kopię „nazwa (kopia).ext” (Enter = tak, Esc = nie) |
| F7 | nowy katalog (okno z polem na nazwę) |
| F8 | do kosza (po potwierdzeniu: Enter = tak, Esc = nie) |
| F9 | odśwież |

Nad każdym panelem jest lista wolumenów i edytowalny pasek ścieżki (Enter wykonuje `cd`).

Wiersz poleceń na dole przyjmuje te same komendy: `cd <ścieżka>`, `back`, `forward`,
`volume <numer|nazwa>`, `filter <tekst>`, `mkdir <nazwa>`, `copy`, `move`, `delete`,
`selmask <maska>`, `unselmask <maska>`, `selectall`, `unselectall`, `invert`,
`sort <name|ext|size|date> [asc|desc]`, `select <nazwa>`, `cursor <n>`, `rename [nazwa]`, `confirm` / `cancel` (odpowiedź na pytanie), `stop` (przerwij zadanie),
`overwrite`, `newer` (nadpisz starsze), `skip`, `keepboth` (z końcówką `-all` = dla wszystkich;
odpowiedzi na „plik już istnieje”), `refresh`.

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
