# CLAUDE.md

Instrukcje dla Claude Code pracującego w tym repozytorium. Czytaj to na początku każdej sesji,
potem `ROADMAP.md` (stan prac).

## Czym jest projekt

**bluepane**: dwupanelowy menedżer plików w stylu Total Commandera. Rust + egui, jedna baza kodu,
**Windows i Linux** są równorzędnymi celami. Czysta implementacja: odtwarzamy funkcje i układ
skrótów, ale nie kopiujemy kodu, zasobów ani nazwy Total Commandera.

## Układ workspace

```
crates/vfs        system plików: listowanie, wolumeny, kod specyficzny dla platformy
crates/ops        kopiuj / przenieś / mkdir / do kosza, postęp, anulowanie, limit prędkości
crates/archive    zip, 7z, tar: czytanie, pakowanie, zmiany w archiwum
crates/sync       porównanie i synchronizacja katalogów
crates/rename     zmiana nazw wielu plików z podglądem
crates/search     szukanie po nazwie i treści (pliki, archiwa, wiele dysków)
crates/remote     SFTP, FTP, FTPS, witryny, hasła (keyring, Bitwarden)
crates/netscan    skaner sieci: hosty, porty, usługi
crates/docs       tekst i strony dokumentów (PDF, Word, Excel, OpenDocument, RTF, PowerPoint)
crates/text       wspólne tabele kodowań (używają docs i search)
crates/engine     panele, zaznaczenie, Command, Keymap, Session (bez egui, bez OS-u)
crates/ui-egui    front egui: rysuje stan silnika, zamienia klawisze na komendy
apps/bluepane     aplikacja okienkowa
apps/bluepane-cli ta sama logika bez okna (do skryptów i testów)
xtask             `cargo xtask ci`
```

## Zasady warstw (egzekwuje `cargo xtask ci`)

Dokładna tabela jest w `ALLOWED` w `xtask/src/main.rs`; w skrócie:

- `vfs`, `ops` i `text` nie zależą od żadnego crate'a workspace; `archive`, `sync`, `rename`,
  `remote`, `netscan`, `search` i `docs` zależą tylko od tych trzech i `archive` (search).
- `engine` zależy od wszystkich crate'ów z logiką, nigdy od `ui-egui` ani aplikacji.
- `ui-egui` zależy tylko od `engine`. Nie ma w nim logiki plików, tylko rysowanie i mapowanie klawiszy.
- `bluepane` (app) zależy od `engine` i `ui-egui`; `bluepane-cli` od `engine` i `vfs`.
- Nowy crate trzeba dopisać do `ALLOWED` w `xtask/src/main.rs`, razem z jego dozwolonymi zależnościami.

## Zasady kodu

1. **Każda akcja użytkownika to `Command`** (`crates/engine/src/command.rs`) z nazwą parsowaną z linii
   poleceń. Nowa funkcja = nowa komenda + wpis w `COMMAND_NAMES` i `COMMAND_HELP` (`command_help.rs`; testy sprawdzają zgodność z parserem) + wpis w `Keymap`, jeśli ma skrót; potem `BLUEPANE_UPDATE_DOCS=1 cargo test -p bluepane-engine command_help` odświeża `docs/KOMENDY.md`. UI nie wywołuje `ops`
   bezpośrednio.
2. **Różnice Windows/Linux tylko w `vfs`** (`crates/vfs/src/platform/`). Poza `vfs` żadnych
   `#[cfg(windows)]` ani `#[cfg(unix)]`. Jedyny wyjątek: atrybut `windows_subsystem` na początku
   `apps/bluepane/src/main.rs` (wersja release na Windows bez okna konsoli; zbudowana z
   `--features console` konsolę zachowuje). Różnica zachowania = trait lub funkcja w `platform`.
3. **Bezpieczeństwo danych:** usuwanie idzie do kosza (`ops::delete_to_trash`), nigdy twardo, dopóki
   użytkownik tego jawnie nie zażąda. Kopiowanie nie nadpisuje bez decyzji użytkownika. Operacje na
   plikach testujemy wyłącznie w katalogach z `tempfile`.
4. Klawisze w `Keymap` to zwykłe stringi (`"F5"`, `"ArrowDown"`), żeby silnik nie znał egui.
5. Komunikaty dla użytkownika po polsku; identyfikatory i komentarze w kodzie po angielsku.
6. Bez `unsafe` (workspace ma `unsafe_code = "forbid"`).
7. Błędy pokazywane użytkownikowi mają być gotowym tekstem (`EngineError`), nie `Debug`.

## Przed każdym commitem

```
cargo xtask ci     # fmt --check, clippy -D warnings, testy, reguły warstw
```

CI na GitHubie uruchamia to samo na `ubuntu-latest` i `windows-latest`. Zmiana nie jest gotowa,
dopóki obie platformy są zielone.

## Jak pracować

- Jeden etap z `ROADMAP.md` = jedna gałąź = jeden PR. Małe kroki, każdy z testami.
- Zaktualizuj `ROADMAP.md` (status etapu) w tym samym PR, który kończy etap.
- Najpierw logika w `engine`/`ops`/`vfs` z testami, dopiero potem UI.
- GUI ma testy bezokienkowe (`crates/ui-egui/tests/gui.rs`, `egui_kittest`: ramki, szukanie widżetów po napisie, kliki, stan silnika; bez porównywania zrzutów). Wygląd i wygoda nadal wymagają ręcznego sprawdzenia na Windows i Linuksie: napisz w PR, co kliknięto. Napisy rysowane „ręcznie” (wiersze list, przyciski klawiszy funkcyjnych, tytuły zakładek doku) nie są widżetami i testy ich nie widzą.
- Nie dodawaj zależności bez potrzeby; każda trafia do `[workspace.dependencies]`.
