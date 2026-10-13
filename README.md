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
| Ctrl+L | wiersz poleceń z `cd ` (przejście do wpisanej ścieżki) |
| Spacja, Insert | zaznacz i przejdź niżej |
| F5 / F6 | kopiuj / przenieś do drugiego panelu |
| F7 | nowy katalog (wpisz nazwę w wierszu poleceń) |
| F8 | do kosza |
| F9 | odśwież |

Nad każdym panelem jest lista wolumenów i edytowalny pasek ścieżki (Enter wykonuje `cd`).

Wiersz poleceń na dole przyjmuje te same komendy: `cd <ścieżka>`, `back`, `forward`,
`volume <numer|nazwa>`, `filter <tekst>`, `mkdir <nazwa>`, `copy`, `move`, `delete`,
`select <nazwa>`, `cursor <n>`, `refresh`.

## Bez okna

```
cargo run -p bluepane-cli -- commands
cargo run -p bluepane-cli -- ls <katalog>
cargo run -p bluepane-cli -- run <lewy> <prawy> "select a.txt" "copy"
```

## Dla developerów

```
cargo xtask ci    # fmt, clippy, testy, reguły warstw (to samo robi GitHub Actions)
```

Układ crate'ów, zasady i sposób pracy z Claude Code: [CLAUDE.md](CLAUDE.md). Plan i stan prac:
[ROADMAP.md](ROADMAP.md).

## Licencja

MIT lub Apache-2.0, do wyboru. Nazwa, kod i zasoby są niezależne od Total Commandera.
