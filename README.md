# bluepane

Dwupanelowy menedżer plików w stylu Total Commandera. Rust + egui, natywny, sterowany
klawiaturą (i myszą), działa na **Windows i Linuksie**. Projekt w fazie testów ręcznych.

## Uruchomienie

Gotowe pliki do testów (bez Rusta) buduje workflow **Build testowy** w zakładce Actions na GitHubie (artefakty
`bluepane-linux` i `bluepane-windows`); instrukcja, dane testowe i lista kontrolna: [docs/TESTOWANIE.md](docs/TESTOWANIE.md).

Ze źródeł (wymagany Rust 1.95 lub nowszy, `rustup`):

```
cargo run --release -p bluepane              # oba panele w bieżącym katalogu
cargo run --release -p bluepane -- ~/a ~/b   # lewy i prawy katalog
```

Linux: do zbudowania okna potrzebne są biblioteki systemowe (przykład dla Debiana/Ubuntu):

```
sudo apt install libxkbcommon-dev libwayland-dev libx11-dev libxcb1-dev libgtk-3-dev
```

## Najważniejsze skróty

| Klawisz | Akcja |
| --- | --- |
| Tab | przełącz panel |
| Strzałki, PgUp/PgDn, Enter, Backspace | kursor; wejdź do katalogu lub archiwum; w górę |
| Spacja, Insert | zaznacz i zejdź niżej (też Ctrl+klik, Shift+klik) |
| litery | szukanie w locie: lista zawęża się do pasujących nazw (Esc czyści) |
| F2, F3, F4 | zmień nazwę; podgląd (tekst, hex, strony PDF i dokumentów); edycja w programie systemu |
| F5, F6, F7, F8 | kopiuj, przenieś, nowy katalog, do kosza |
| Ctrl+Q | Quick View: drugi panel pokazuje plik pod kursorem |
| Ctrl+F, Alt+S, Ctrl+F2 | szukanie, synchronizacja, zmiana nazw wielu plików |
| Ctrl+J, Ctrl+S, Ctrl+N | kolejka zadań, serwery SFTP/FTP/FTPS, skaner sieci |
| Ctrl+T, Ctrl+W, Ctrl+1…9 | zakładki katalogów w panelu |
| Ctrl+L | wiersz poleceń (komendy: patrz niżej) |

Pełna lista: [docs/KLAWISZE.md](docs/KLAWISZE.md). Wszystko, co robi program, to komendy (okno poleceń na dole, `bind`
przypisuje je do klawiszy): [docs/KOMENDY.md](docs/KOMENDY.md).

## Dokumentacja

| Plik | Zawartość |
| --- | --- |
| [docs/FUNKCJE.md](docs/FUNKCJE.md) | co program robi: podgląd i dokumenty, archiwa, synchronizacja, skaner sieci, kolejka, serwery, szukanie, zmiana nazw, kopiowanie |
| [docs/KLAWISZE.md](docs/KLAWISZE.md) | wszystkie skróty klawiszowe |
| [docs/KOMENDY.md](docs/KOMENDY.md) | wszystkie komendy z opisem (generowane z kodu) |
| [docs/TESTOWANIE.md](docs/TESTOWANIE.md) | uruchomienie na Linuksie i Windows, dane testowe, lista kontrolna |
| [ROADMAP.md](ROADMAP.md) | plan i stan prac |
| [CLAUDE.md](CLAUDE.md) | układ crate'ów, zasady kodu i sposób pracy |

## Bez okna

```
cargo run -p bluepane-cli -- commands        # komendy z opisami
cargo run -p bluepane-cli -- ls <katalog>
cargo run -p bluepane-cli -- run <lewy> <prawy> "select a.txt" "copy"
cargo run -p bluepane-cli -- run <lewy> <prawy> "select a.txt" "delete" "confirm"
```

## Dla developerów

```
cargo xtask ci    # fmt, clippy, testy, reguły warstw (to samo robi GitHub Actions)
```

Układ crate'ów, zasady i sposób pracy z Claude Code: [CLAUDE.md](CLAUDE.md). Plan i stan prac: [ROADMAP.md](ROADMAP.md).
Dokument docs/KOMENDY.md odświeża się poleceniem `BLUEPANE_UPDATE_DOCS=1 cargo test -p bluepane-engine command_help`.

## Licencja

MIT lub Apache-2.0, do wyboru. Nazwa, kod i zasoby są niezależne od Total Commandera.
