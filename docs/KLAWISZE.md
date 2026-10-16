# Klawisze

Pełna lista skrótów. Każdy skrót to komenda (patrz [KOMENDY.md](KOMENDY.md)); własne przypisania:
`bind <klawisz> <komenda>`, `unbind <klawisz>` (zapisują się w `settings.conf`). Klawisze okien podglądu i stron
są na razie stałe.

| Klawisz | Akcja |
| --- | --- |
| Tab | przełącz panel |
| Strzałki, PageUp/PageDown | kursor |
| Enter / Backspace | wejdź do katalogu / w górę (wiersz `..` też wychodzi w górę) |
| Alt+← / Alt+→ | historia katalogów: wstecz / naprzód (osobna dla każdego panelu) |
| litery, spacja | szukanie w locie: lista zawęża się do nazw zawierających wpisany tekst (spacja jest częścią nazwy, gdy filtr już trwa); Backspace skraca filtr, Esc czyści; wiersz poleceń dostaje klawiaturę po kliknięciu lub Ctrl+L |
| Enter na archiwum | wejdź do archiwum (zip, 7z, tar, tar.gz, tgz, także części `*.001`); `..` na górze wychodzi |
| F5 w archiwum | rozpakuj zaznaczone do katalogu drugiego panelu |
| F3 w archiwum | podgląd pliku z archiwum (rozpakowuje go do katalogu tymczasowego, który znika po zamknięciu podglądu; limit 256 MB, większe rozpakuj F5) |
| F5 do panelu z archiwum | dodaj zaznaczone pliki do archiwum (do katalogu, który tam oglądasz) |
| F8 / F2 / F7 w archiwum | usuń z archiwum (po potwierdzeniu, bez kosza) / zmień nazwę / nowy katalog |
| Ctrl+Shift+1 / 2 / 3 | skopiuj do schowka zaznaczone nazwy: bez rozszerzeń / z rozszerzeniami / pełne ścieżki (po jednej w linii; bez zaznaczenia: element pod kursorem) |
| Ctrl+N | skaner sieci (urządzenia w zakresie adresów IP, ich porty i usługi) |
| Alt+Shift+Enter | `dirsize`: policz w tle rozmiar zaznaczonych katalogów (albo tego pod kursorem); wynik pojawia się w kolumnie Rozmiar i w podsumowaniu pod listą (odświeżenie F9 i operacje na plikach czyszczą policzone rozmiary) |
| Ctrl+Q | Quick View: drugi panel pokazuje na żywo plik pod kursorem aktywnego (tekst/hex, strona PDF, tekst dokumentów); kółko przewija, Ctrl+Q zamyka; opis w [FUNKCJE.md](FUNKCJE.md) |
| F1 | paleta komend: wyszukiwarka wszystkich komend z opisem (↑↓ wybór, Enter uruchamia, Esc zamyka); opis w [FUNKCJE.md](FUNKCJE.md) |
| Ctrl+P | „Skocz do”: katalogi według częstości i świeżości odwiedzin, zakładki i serwery w jednym polu szukania (Enter przechodzi w aktywnym panelu); opis w [FUNKCJE.md](FUNKCJE.md) |
| Ctrl+J | kolejka zadań: bieżące, oczekujące (usuń z kolejki), zakończone (ponów), wstrzymanie kolejki |
| Ctrl+Shift+F4 | wyślij na serwer plik edytowany przez F4 |
| Ctrl+S | serwery SFTP / FTP / FTPS (zapisane połączenia) |
| Ctrl+F, Alt+F7 | wyszukiwanie plików (okno; wyniki zostają po zamknięciu) |
| Ctrl+F2 | zmiana nazw wielu plików (okno z podglądem nowych nazw) |
| Alt+S | synchronizacja katalogów obu paneli (okno) |
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
| Ctrl+klik / Shift+klik / Ctrl+Shift+klik | zaznacz wiersz / zakres od ostatnio klikniętego / dodaj zakres (jak w Windows; zwykły klik tylko przesuwa kursor); komenda `click <wiersz> [ctrl\|shift\|ctrlshift]` |
| Spacja, Insert | zaznacz i przejdź niżej |
| F2 | zmień nazwę (okno z bieżącą nazwą; nie nadpisuje istniejących) |
| F3 | podgląd pliku (tekst lub hex, dowolnie duży) |
| F4 | edycja w zewnętrznym edytorze (`BLUEPANE_EDITOR`, np. `code -w`; domyślnie aplikacja systemu) |
| F5 / F6 | kopiuj / przenieś do drugiego panelu; F5 przy tym samym katalogu w obu panelach proponuje kopię „nazwa (kopia).ext” (Enter = tak, Esc = nie) |
| F7 | nowy katalog (okno z polem na nazwę) |
| F8 | do kosza (po potwierdzeniu: Enter = tak, Esc = nie) |
| F9 | odśwież |

Każdy panel ma własny rząd zakładek (klik przełącza, × albo środkowy przycisk zamyka, + dodaje); zakładka pamięta katalog, historię, sortowanie, filtr i zaznaczenie. Nad każdym panelem jest lista wolumenów i edytowalny pasek ścieżki (Enter wykonuje `cd`).
