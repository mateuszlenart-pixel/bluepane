# Roadmap

Legenda: ✅ gotowe · 🟡 szkielet (działa, ale uproszczone) · ⬜ niezaczęte

Każdy etap kończy się działającą wersją i zielonym `cargo xtask ci` na Windows i Linuksie.
Etapy 1–4 dają użyteczny menedżer plików do codziennej pracy.

| # | Etap | Stan | Gotowe, gdy |
| --- | --- | --- | --- |
| 1 | Fundament | ✅ | workspace, CLAUDE.md, CI na obu systemach, okno z dwoma panelami pokazującymi pliki |
| 2 | Nawigacja | 🟡 | po dysku da się poruszać samym klawiaturą |
| 3 | Zaznaczanie | ✅ | katalog z 100 tys. plików nie zacina się (cel) |
| 4 | Operacje F5–F8 | ✅ | operacje w tle z postępem, dialog konfliktów, testy na katalogach tymczasowych |
| 5 | Podgląd i edycja | ✅ | duży plik otwiera się bez wczytywania całości do pamięci |
| 6 | Archiwa | ⬜ | Enter wchodzi do archiwum, kopiowanie działa w obie strony |
| 7 | Zaawansowane | ⬜ | synchronizacja pokazuje zmiany przed wykonaniem |
| 8 | Sieć i wtyczki | ⬜ | SFTP działa jak katalog lokalny |
| 9 | Dystrybucja | ⬜ | instalacja z gotowych paczek na czystej maszynie |

## Stan szkieletu (etap 1) i co dalej

### Działa już teraz
- Dwa panele, Tab, Enter, Backspace, strzałki, PageUp/PageDown, Spacja i Insert (zaznaczanie).
- F5 kopiuj, F6 przenieś, F7 nowy katalog (przez wiersz poleceń), F8 do kosza, F9 odśwież.
- Wiersz poleceń: `cd`, `mkdir`, `copy`, `move`, `delete`, `select`, `cursor`, `refresh`.
- `bluepane-cli run <lewy> <prawy> "select a.txt" "copy"`: te same komendy bez okna.
- Kosz zamiast twardego usuwania, kopiowanie bez nadpisywania.

### Etap 2: nawigacja
- [x] Pasek ścieżki z edycją, lista wolumenów (`bluepane_vfs::volumes`) w UI, skok do wolumenu (`volume`, Alt+1..9)
- [x] Historia katalogów (wstecz/naprzód) per panel (`back`, `forward`, Alt+←/→)
- [ ] Zakładki (reszta etapu 2; stan etapu zostaje 🟡 do ich zrobienia)
- [x] Szybkie filtrowanie po wpisaniu liter (`filter`, Esc czyści, Backspace skraca)
- [x] Wpis `..` na górze listy (nie jest celem operacji, nie da się go zaznaczyć)

### Etap 3: zaznaczanie
- [x] Zaznacz/odznacz po masce (`selmask`/`unselmask`, Ctrl+M / Ctrl+Shift+M zamiast klawiszy `+`/`-`, bo litery i znaki idą do szybkiego filtra), Ctrl+A, odwrócenie zaznaczenia
- [x] Sortowanie po nazwie, rozszerzeniu, rozmiarze, dacie (Ctrl+F3..F6); klik w nagłówek kolumny
- [x] Prawdziwe kolumny (nazwa, typ, rozmiar, data) i daty w czasie lokalnym (`chrono`)
- [x] Test wydajności: 100 tys. wpisów (test w CI na liście w pamięci; ręczny `--ignored` na 100 tys. prawdziwych plików: listing ok. 1 s, razem z zaznaczeniem i sortowaniem ok. 2,7 s w release)

### Etap 4: operacje
- [x] Wykonywanie w wątku w tle (`Session` z kolejką zadań, `poll`), pasek postępu, przycisk Anuluj (`stop`)
- [x] Kopiowanie w kawałkach 256 KiB (postęp bajtów w trakcie dużego pliku, anulowanie w trakcie pliku, niepełny plik usuwany)
- [x] Dialog konfliktów: nadpisz / nadpisz starsze / pomiń / zachowaj oba / anuluj, każdy też „dla wszystkich"; scalanie katalogów; nadpisywanie przez plik tymczasowy
- [x] F2 zmiana nazwy, F7 jako okno dialogowe
- [x] Potwierdzenie przed usunięciem (Enter/Esc)
- [x] Windows: długie ścieżki (`\\?\`, tylko dla ścieżek > 240 znaków), odczyt-tylko przy nadpisywaniu; Linux/Windows: symlinki odtwarzane (z awaryjnym kopiowaniem celu). Różnice systemowe siedzą w `vfs` i trafiają do `ops` przez trait `Host`.
- [ ] Do sprawdzenia ręcznie na Windows: długie ścieżki, pliki tylko do odczytu, symlinki (wymagają uprawnień). Kod tych ścieżek kompiluje się w CI, ale nie był uruchamiany.

## Notatki do późniejszych etapów

### Etap 5: podgląd i edycja
- [x] Podgląd (F3) oknami przez seek: plik 1 GB otwiera się od razu (proces ok. 110 MB, tyle co sam interfejs), linie ograniczone do 16 KiB, wykrywanie plików binarnych
- [x] Tryby tekst / hex, przewijanie, skok na początek/koniec/procent (`viewer goto`), przesuwanie w bok
- [x] Szukanie z podświetleniem trafienia; limit 256 MiB na wywołanie z wznawianiem (okno nie zamarza)
- [x] Edycja (F4) w zewnętrznym edytorze (`BLUEPANE_EDITOR` albo domyślna aplikacja: `xdg-open` / `explorer`)
- [ ] Wbudowany edytor tekstu. Świadomie poza etapem: `egui::TextEdit` wczytuje cały tekst do pamięci i nie nadaje się do plików wielogigabajtowych. Do decyzji: edytor tylko dla małych plików albo wyłącznie zewnętrzny.
- [ ] Szukanie bez rozróżniania wielkości liter działa dla ASCII; inne kodowania niż UTF-8 (np. CP1250) pokazują się z znakami zastępczymi
- [ ] Do sprawdzenia ręcznie na Windows: F4 przez `explorer`, wyszukiwanie w dużych plikach

### Etap 8: FTP/SFTP
Wymaganie od właściciela projektu (potwierdzone): funkcjonalność na wzór **FileZilli**:
- menedżer witryn z zapisanymi połączeniami, w tym rozbudowany wybór i konfiguracja
  połączenia (protokół FTP / FTPS / SFTP, host, port, tryb logowania, dane uwierzytelniające,
  katalog startowy; szczegóły do dopracowania przy starcie etapu),
- kolejka transferów z postępem, wznawianie przerwanych transferów,
- dialog konfliktów (nadpisz / pomiń / zmień nazwę).

## Znane uproszczenia szkieletu
- Interfejs sprawdzony ręcznie na Linuksie (Linux Mint, X11, wgpu); **nie był jeszcze uruchamiany
  na Windows** (kompiluje się i przechodzi CI).
- Kursor startuje na wierszu `..`, więc `copy`/`move`/`delete` bez zaznaczenia i bez ruchu kursora
  zgłaszają „nic nie zaznaczono”.
