# Roadmap

Legenda: ✅ gotowe · 🟡 szkielet (działa, ale uproszczone) · ⬜ niezaczęte

Każdy etap kończy się działającą wersją i zielonym `cargo xtask ci` na Windows i Linuksie.
Etapy 1–4 dają użyteczny menedżer plików do codziennej pracy.

| # | Etap | Stan | Gotowe, gdy |
| --- | --- | --- | --- |
| 1 | Fundament | ✅ | workspace, CLAUDE.md, CI na obu systemach, okno z dwoma panelami pokazującymi pliki |
| 2 | Nawigacja | 🟡 | po dysku da się poruszać samym klawiaturą |
| 3 | Zaznaczanie | 🟡 | katalog z 100 tys. plików nie zacina się (cel) |
| 4 | Operacje F5–F8 | 🟡 | operacje w tle z postępem, dialog konfliktów, testy na katalogach tymczasowych |
| 5 | Podgląd i edycja | ⬜ | duży plik otwiera się bez wczytywania całości do pamięci |
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
- [ ] `+` i `-` (zaznacz/odznacz po masce), Ctrl+A, odwrócenie zaznaczenia
- [ ] Sortowanie po nazwie, rozszerzeniu, rozmiarze, dacie; klik w nagłówek kolumny
- [ ] Prawdziwe kolumny (zamiast tekstu monospace) i formatowanie dat
- [ ] Test wydajności: katalog ze 100 tys. plików

### Etap 4: operacje
- [ ] Wykonywanie w wątku w tle (`Session` dostaje kolejkę zadań), pasek postępu, przycisk Anuluj
- [ ] Kopiowanie w kawałkach (postęp bajtów w trakcie dużego pliku)
- [ ] Dialog konfliktów: nadpisz / pomiń / zmień nazwę / dla wszystkich
- [ ] F2 zmiana nazwy, F7 jako okno dialogowe
- [ ] Potwierdzenie przed usunięciem
- [ ] Windows: długie ścieżki (`\\?\`), atrybuty tylko do odczytu; Linux: symlinki, uprawnienia

## Notatki do późniejszych etapów

### Etap 8: FTP/SFTP
Wymaganie od właściciela projektu (potwierdzone): funkcjonalność na wzór **FileZilli**:
- menedżer witryn z zapisanymi połączeniami, w tym rozbudowany wybór i konfiguracja
  połączenia (protokół FTP / FTPS / SFTP, host, port, tryb logowania, dane uwierzytelniające,
  katalog startowy; szczegóły do dopracowania przy starcie etapu),
- kolejka transferów z postępem, wznawianie przerwanych transferów,
- dialog konfliktów (nadpisz / pomiń / zmień nazwę).

## Znane uproszczenia szkieletu
- Operacje na plikach działają synchronicznie w wątku UI (duże kopie zamrożą okno do etapu 4).
- Kopiowanie symlinków podąża za celem (`fs::copy`), nie odtwarza dowiązania.
- Brak dialogów: F7 wpisuje `mkdir ` w wiersz poleceń, a F8 usuwa od razu (do kosza).
- Interfejs sprawdzony ręcznie na Linuksie (Linux Mint, X11, wgpu); **nie był jeszcze uruchamiany
  na Windows** (kompiluje się i przechodzi CI).
- Kursor startuje na wierszu `..`, więc `copy`/`move`/`delete` bez zaznaczenia i bez ruchu kursora
  zgłaszają „nic nie zaznaczono”.
