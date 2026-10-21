# bluepane

Dwupanelowy menedżer plików w stylu Total Commandera. Rust + egui, natywny, sterowany
klawiaturą (i myszą), działa na **Windows i Linuksie**. Projekt w fazie testów ręcznych.

## Instalacja

Gotowe paczki są na stronie [Releases](https://github.com/mateuszlenart-pixel/bluepane/releases) (po pierwszym wydaniu; do tego czasu patrz „Uruchomienie” niżej). Program nie sprawdza aktualizacji sam: nowe wersje pobierasz stamtąd.

| System | Plik | Jak zainstalować |
| --- | --- | --- |
| Windows | `bluepane-X.Y.Z-windows-x86_64-setup.exe` | uruchom instalator (domyślnie dla Twojego konta, bez praw administratora); skrót w menu Start |
| Windows | `bluepane-X.Y.Z-windows-x86_64.zip` | rozpakuj gdziekolwiek i uruchom `bluepane.exe` (wersja przenośna) |
| Debian, Ubuntu | `bluepane-X.Y.Z-linux-amd64.deb` | `sudo apt install ./bluepane-X.Y.Z-linux-amd64.deb`; usunięcie: `sudo apt remove bluepane` |
| Dowolny Linux | `bluepane-X.Y.Z-linux-x86_64.AppImage` | `chmod +x plik.AppImage` i uruchom; bez instalacji |
| Dowolny Linux | `bluepane-X.Y.Z-linux-x86_64.tar.gz` | rozpakuj i uruchom `bluepane` |

Kod nie jest podpisany, więc **Windows SmartScreen** może pokazać „System Windows ochronił ten komputer”: kliknij „Więcej informacji”, potem „Uruchom mimo to”. Sumę pliku sprawdzisz z `SHA256SUMS.txt` z wydania (`sha256sum -c SHA256SUMS.txt` na Linuksie, `Get-FileHash plik` w PowerShellu). Wersję pokazuje `bluepane --version` (na Windows: komenda `about` w programie albo `bluepane-cli --version`). Ustawienia leżą w `%APPDATA%\bluepane` (Windows) albo `~/.config/bluepane` (Linux) i zostają po odinstalowaniu. Opcjonalne programy: `git` (kolumna i akcje git), `smbclient` (udziały SMB), `rclone` (chmury), `ssh`. Jak budować wydania: [docs/WYDANIA.md](docs/WYDANIA.md).

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
