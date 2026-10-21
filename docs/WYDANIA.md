# Jak wydać nową wersję

Wydanie robi workflow `.github/workflows/release.yml` po wypchnięciu tagu `vX.Y.Z`. Wydanie jest
publiczne na stronie Releases, więc robi je właściciel projektu (albo Claude na jego wyraźne
polecenie), nigdy samo z siebie.

## Kroki

1. **Wersja:** podnieś `version` w `[workspace.package]` w `Cargo.toml` (numeracja `0.MINOR.PATCH`,
   patrz `CHANGELOG.md`) i przebuduj (`cargo check` odświeża `Cargo.lock`).
2. **Historia zmian:** w `CHANGELOG.md` zamień „Niewydane” na `## [X.Y.Z]: krótki opis` i dodaj
   puste „Niewydane” na górze. Z tej sekcji powstają notatki wydania.
3. **PR i `main`:** zmiana przechodzi zwykłą drogą (PR, `cargo xtask ci`, CI na obu systemach,
   „scalaj”). Tag stawiamy dopiero na zielonym `main`.
4. **Próba generalna (opcjonalnie):** Actions > „Wydanie” > „Run workflow” buduje same paczki jako
   artefakty, bez wydania. To samo robi każdy PR zmieniający workflow, skrypty lub `CHANGELOG.md`.
5. **Tag:** `git tag vX.Y.Z && git push origin vX.Y.Z`. Workflow sprawdza, że tag zgadza się
   z wersją w `Cargo.toml` (inaczej kończy się błędem, nic nie jest publikowane), buduje paczki,
   liczy SHA-256 i tworzy Release z notatkami z `CHANGELOG.md`.
6. **Po wydaniu:** pobierz paczkę z Releases na czystej maszynie i sprawdź (lista w 9e), zweryfikuj
   sumę: `sha256sum -c SHA256SUMS.txt` (Linux) albo `Get-FileHash` (Windows).

## Co jest w wydaniu

| Plik | Zawartość |
| --- | --- |
| `bluepane-X.Y.Z-linux-x86_64.tar.gz` | `bluepane`, `bluepane-cli`, licencje, README, CHANGELOG, `docs/` (budowane na Ubuntu 22.04, działa też na starszych dystrybucjach) |
| `bluepane-X.Y.Z-linux-amd64.deb` | paczka Debiana/Ubuntu (`sudo apt install ./plik.deb`), budowana `tools/pakiet-deb.sh` samym `dpkg-deb`: programy w `/usr/bin`, wpis w menu aplikacji, ikona, dokumentacja w `/usr/share/doc/bluepane`; zależności systemowe pobiera apt, `git`, `smbclient`, `rclone` są opcjonalne |
| `bluepane-X.Y.Z-linux-x86_64.AppImage` | jeden plik do uruchomienia na dowolnej dystrybucji (`chmod +x` i dwuklik), budowany `tools/pakiet-appimage.sh` z `appimagetool 1.9.0` (pobierany i sprawdzany sumą SHA-256); bez wbudowanych bibliotek, korzysta z systemowych (GL, X11/Wayland) |
| `bluepane-X.Y.Z-windows-x86_64.zip` | to samo z `.exe`, bez okna konsoli |
| `bluepane-X.Y.Z-windows-x86_64-setup.exe` | instalator (Inno Setup, `installer/bluepane.iss`): domyślnie dla bieżącego użytkownika bez praw administratora, okno pozwala wybrać instalację dla wszystkich; skrót w menu Start (opcjonalnie na pulpicie), odinstalowanie w „Aplikacje”; ustawienia w `%APPDATA%\bluepane` zostają po odinstalowaniu |
| `SHA256SUMS.txt` | sumy kontrolne wszystkich plików powyżej |

Workflow sam sprawdza paczki. Instalator Windows: cicha instalacja do katalogu z polskimi znakami, `bluepane-cli --version`, odinstalowanie. `.deb`: instalacja przez `apt`, walidacja pliku `.desktop`, `--version`, usunięcie. AppImage: uruchomienie `--version`. Kod nie jest podpisany:
Windows SmartScreen pokaże ostrzeżenie o nieznanym wydawcy („Więcej informacji” > „Uruchom mimo to”).

## Skrypty

- `tools/wersja.sh [tag]`: wypisuje wersję z `Cargo.toml`; z tagiem sprawdza zgodność.
- `tools/wydanie-notatki.sh <wersja>`: wypisuje sekcję tej wersji z `CHANGELOG.md`.
- `tools/pakiet-deb.sh <wersja> <katalog-programów> <wyjście>` i `tools/pakiet-appimage.sh …`: budują paczki Linuksa (można uruchomić lokalnie po `cargo build --release`; zmienna `APPIMAGETOOL` wskazuje własną kopię `appimagetool`).
