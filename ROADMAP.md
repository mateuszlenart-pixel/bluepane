# Roadmap

Legenda: ✅ gotowe · 🟡 szkielet (działa, ale uproszczone) · ⬜ niezaczęte

Każdy etap kończy się działającą wersją i zielonym `cargo xtask ci` na Windows i Linuksie.
Etapy 1–4 dają użyteczny menedżer plików do codziennej pracy.

| # | Etap | Stan | Gotowe, gdy |
| --- | --- | --- | --- |
| 1 | Fundament | ✅ | workspace, CLAUDE.md, CI na obu systemach, okno z dwoma panelami pokazującymi pliki |
| 2 | Nawigacja | ✅ | po dysku da się poruszać samym klawiaturą |
| 3 | Zaznaczanie | ✅ | katalog z 100 tys. plików nie zacina się (cel) |
| 4 | Operacje F5–F8 | ✅ | operacje w tle z postępem, dialog konfliktów, testy na katalogach tymczasowych |
| 5 | Podgląd i edycja | ✅ | duży plik otwiera się bez wczytywania całości do pamięci |
| 6 | Archiwa | ✅ | Enter wchodzi do archiwum, kopiowanie działa w obie strony |
| 7 | Synchronizacja | ✅ | synchronizacja pokazuje zmiany przed wykonaniem |
| 7b | Wyszukiwanie | ✅ | szukanie po nazwie i treści w wielu katalogach, dyskach i archiwach |
| 7c | Grupowa zmiana nazw | ✅ | zmiana nazw wielu plików z podglądem wyniku przed wykonaniem |
| 8 | Sieć | 🟡 | SFTP i FTP działają jak katalog lokalny |
| 8f | Skaner sieci | ✅ | lista urządzeń z usługami, udziałami i akcjami (jak Advanced IP Scanner) |
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
- [x] Zakładki per panel (`tab new|open|close|next|prev|n`, Ctrl+T/W/↑/Tab/1..9, klik w pasku). Zakładki nie są jeszcze zapamiętywane między uruchomieniami (zapis ustawień to osobne zadanie).
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

### Podgląd dokumentów biurowych (po etapie 8)
- [x] Crate `bluepane-docs`: F3 na PDF pokazuje strony jako obraz (hayro, czysty Rust); DOCX/ODT/ODS/XLSX przez LibreOffice -> PDF (jeśli jest), inaczej tekst dokumentu; F4 otwiera w programie systemu
- [x] Okno stron (zmiana strony, powiększenie, dopasowanie do szerokości), render w tle
- [ ] Nie ma: dokumentów Office jako strony z archiwum i serwera, RTF/PPTX, edycji w programie, PDF chronionych hasłem, OCR skanów, zaznaczania tekstu na stronie; do sprawdzenia ręcznie z prawdziwymi dokumentami i LibreOffice

### Etap 5: podgląd i edycja
- [x] Podgląd (F3) oknami przez seek: plik 1 GB otwiera się od razu (proces ok. 110 MB, tyle co sam interfejs), linie ograniczone do 16 KiB, wykrywanie plików binarnych
- [x] Tryby tekst / hex, przewijanie, skok na początek/koniec/procent (`viewer goto`), przesuwanie w bok
- [x] Szukanie z podświetleniem trafienia; limit 256 MiB na wywołanie z wznawianiem (okno nie zamarza)
- [x] Edycja (F4) w zewnętrznym edytorze (`BLUEPANE_EDITOR` albo domyślna aplikacja: `xdg-open` / `explorer`)
- [ ] Wbudowany edytor tekstu. Świadomie poza etapem: `egui::TextEdit` wczytuje cały tekst do pamięci i nie nadaje się do plików wielogigabajtowych. Do decyzji: edytor tylko dla małych plików albo wyłącznie zewnętrzny.
- [ ] Szukanie bez rozróżniania wielkości liter działa dla ASCII; inne kodowania niż UTF-8 (np. CP1250) pokazują się z znakami zastępczymi
- [ ] Do sprawdzenia ręcznie na Windows: F4 przez `explorer`, wyszukiwanie w dużych plikach

### Etap 6: archiwa
- [x] Enter wchodzi do archiwum (ZIP, 7z, TAR, TAR.GZ, także wieloczęściowe `.001`), przeglądanie jak katalogu (tylko do odczytu), F5 rozpakowuje zaznaczenie
- [x] Tworzenie archiwów (`pack`, okno Alt+F5): format, poziom kompresji, hasło, podział na części; przełączniki w stylu 7-Zip
- [x] Hasła: odczyt ZipCrypto i AES, zapis AES-256 (ZIP i 7z); w 7z także ukrywanie nazw plików (`-mhe`); pytanie o hasło z ponawianiem, hasło zapamiętane dla archiwum
- [x] Podział na części (`nazwa.zip.001`, ...) i odczyt wieloczęściowych; `testarchive`; konflikty jak przy kopiowaniu
- [x] Bezpieczeństwo: odrzucanie ścieżek wychodzących poza cel (zip-slip), pomijanie dowiązań i urządzeń, bez bitów setuid
- [x] Zgodność sprawdzona z prawdziwymi narzędziami: 7-Zip (zwykły ZIP, AES-256, wolumeny), Info-ZIP (ZipCrypto), GNU tar
- [x] 7z: odczyt i zapis (LZMA2, AES-256, szyfrowanie nagłówka, części `.7z.001`), zgodność z prawdziwym 7-Zip w obie strony (archiwa lite i nielite, wolumeny, hasła)
- [x] Zmiany w istniejącym archiwum: dodawanie (F5), usuwanie (F8), zmiana nazwy (F2), nowy katalog (F7); przepisanie przez plik tymczasowy, ZIP bez dekompresji, TAR/TAR.GZ/7z w całości (z postępem i anulowaniem)
- [x] Podgląd (F3) pliku z archiwum: rozpakowanie do katalogu tymczasowego, sprzątanie przy zamknięciu podglądu i programu; limit 256 MB (większe: F5)
- [ ] Nie ma: RAR (świadomie pominięty: wymaga kodu w C), BZip2/PPMd w 7z oraz LZMA/BZip2/Zstd w ZIP (czytelny błąd „nieobsługiwane”)
- [ ] Do sprawdzenia ręcznie na Windows (testy kompilują się i przechodzą w CI, ale GUI i nazwy niedozwolone w Windows nie były oglądane)

### Etap 7: synchronizacja
- [x] Crate `bluepane-sync`: skanowanie, filtry, porównywanie (czas z tolerancją / rozmiar / zawartość), plan (dwukierunkowa, lustro, aktualizacja, własna), stan ostatniej synchronizacji
- [x] Podgląd planu przed wykonaniem, ręczna zmiana kierunku pozycji, wykonanie w tle z postępem i anulowaniem
- [x] Usuwanie: kosz / katalog wersji / trwale; kopia zachowuje datę modyfikacji
- [x] Komendy `sync`, `syncrun`, `syncset`, `syncclose`; okno Alt+S
- [ ] Do sprawdzenia ręcznie na Windows (okno i nazwy niedozwolone w Windows)

### Etap 7b: wyszukiwanie
- [x] Crate `bluepane-search`: nazwy i maski (lub regex), pełny tekst w plikach (strumieniowo; UTF-8, UTF-16, CP1250; numer linii i podświetlenie), filtry rozmiaru i daty, wykluczanie katalogów, limit wyników
- [x] Wiele katalogów i dysków naraz (`all` = wszystkie wolumeny), bez podwójnego przeszukiwania zagnieżdżonych katalogów i bez śledzenia dowiązań
- [x] Archiwa ZIP/TAR/TAR.GZ/7z bez rozpakowywania na dysk (`archive::visit`), hasła, zgłaszanie zaszyfrowanej zawartości
- [x] Komendy `find`, `findset`, `findrun`, `findstop`, `findgo`, `findview`, `findclose`; osobny wątek (nie blokuje kolejki zadań); okno Alt+F7 z wynikami na żywo
- [x] Wyniki jako wirtualny katalog w panelu (`findpanel`, `Panel::show_paths`): kopiuj / przenieś / usuń / podgląd / zmiana nazw na liście; znikające pliki same odpadają z listy
- [ ] Nie ma: zapisu zestawów kryteriów, wyszukiwania w archiwach zagnieżdżonych, podglądu pliku z archiwum prosto z wyniku (jest przejście do archiwum), kodowań innych niż UTF-8/16 i CP1250, indeksu (każde szukanie czyta dysk od nowa)
- [ ] Do sprawdzenia ręcznie na Windows i Linuksie: wygląd okna, szukanie na „wszystkich dyskach”, duże drzewa (czas, pamięć)

### Etap 7c: grupowa zmiana nazw
- [x] Crate `bluepane-rename`: maski `[N]` `[E]` `[C]` `[Y][M][D][h][m][s]` `[P]` z zakresami, szukaj/zamień (tekst, regex, 1x, z rozszerzeniem), wielkość liter, plan z konfliktami
- [x] Wykonanie dwufazowe (przez nazwy tymczasowe): zamiany `a <-> b`, zmiana samej wielkości liter, nigdy bez nadpisania, wycofanie przy błędzie; cofnięcie ostatniej zmiany
- [x] Komendy `multirename`, `mrset`, `mrname`, `mrrun`, `mrundo`, `mrclose`; okno Ctrl+F2 z listą stara/nowa nazwa na żywo i ręczną edycją pojedynczej nazwy
- [ ] Nie ma: wtyczek `[=?]` (pole z zrzutu wzorca), zapisu/odczytu zestawów ustawień (F2 w oknie wzorca), kolumn sortowania w oknie, wstawiania pól w miejscu kursora (przyciski dopisują na końcu maski)
- [ ] Do sprawdzenia ręcznie (Linux i Windows): wygląd okna, klikanie w nową nazwę, nazwy niedozwolone w Windows

### Etap 8f: skaner sieci
Wymaganie właściciela (zamiast wtyczek): funkcje jak Advanced IP Scanner. Zakres: 1) urządzenia w zakresie IP, 2) porty i usługi, 3) udziały (SMB) i akcje na wyniku.
- [x] Crate `bluepane-netscan`: zakresy adresów, wykrywanie hostów (TCP + tablica ARP, `vfs::neighbours`), porty i nazwy usług, odwrotny DNS, producent z MAC (rejestr IEEE), anulowanie, postęp
- [x] Silnik i okno (Ctrl+N): zakres, porty, filtr, tabela na żywo; komendy `scan`, `scanset`, `scanrun`, `scanstop`, `scanclose`
- [x] 3) Udziały SMB (`smbclient` / `net view`), akcje na wyniku (przeglądarka, RDP, zapis jako serwer SFTP/FTP, kopiuj adres), ulubione (zakładka „Ulubione”, plik w konfiguracji); testy na atrapach programów
- [ ] Do sprawdzenia ręcznie: skan prawdziwej sieci na Linuksie i Windows (tablica ARP, nazwy, producenci, udziały, pulpit zdalny, otwieranie udziału); nie ma: logowania do udziałów z hasłem (tylko anonimowo lub konto Windows), banerów usług, wykrywania systemu operacyjnego, skanowania IPv6

### Etap 8: FTP/SFTP
Wymaganie od właściciela projektu (potwierdzone): funkcjonalność na wzór **FileZilli**: menedżer witryn, kolejka transferów z postępem, wznawianie, dialog konfliktów. Kolejność: najpierw SFTP (SSH).

Zrobione (etap 8a, SFTP):
- [x] Crate `bluepane-sftp` (russh + russh-sftp, czysty Rust, API blokujące): logowanie hasłem i kluczem, listowanie, mkdir/rename/usuwanie, pobieranie i wysyłanie z wznawianiem (`.bpart`, tylko świeże), znane klucze serwerów (nieznany: pytanie z odciskiem; zmieniony: odrzucony)
- [x] Menedżer witryn (`sites.conf`), hasło: pytaj / lokalnie / Bitwarden CLI
- [x] Panel serwera w silniku (`Panel::remote`), komendy `sites`, `siteset`, `sitedel`, `connect`, `disconnect`; transfery w kolejce zadań z postępem, anulowaniem i dialogiem konfliktów; okno witryn (Ctrl+S)
- [x] Testy na serwerze SSH/SFTP uruchamianym w procesie testu (`bluepane_sftp::testing`), bez prawdziwych maszyn
- [ ] Do sprawdzenia ręcznie: połączenie z prawdziwym serwerem (OpenSSH, Proxmox, Home Assistant), Windows (klucze, ścieżki), Bitwarden (`bw`)

Do zrobienia (8b i dalej):
- [x] FTP i FTPS (jawny i niejawny; certyfikat przypinany odciskiem SHA-256) przez wspólny interfejs `RemoteFs` (crate `bluepane-remote`, dawniej `bluepane-sftp`); serwery FTP/FTPS w testach
- [ ] FTP: tryb aktywny, ustawianie daty pliku na serwerze, serwery bez SIZE/MDTM (część rzadkich serwerów); sprawdzić ręcznie z prawdziwym serwerem FTP/FTPS
- [x] F6 (przenoszenie w każdą stronę, także w obrębie serwera), podgląd F3 pliku z serwera (plik tymczasowy), kopiowanie serwer -> serwer (przez katalog tymczasowy)
- [x] F4 pliku z serwera (kopia w edytorze, odesłanie `editsend`), zakładki i historia (Alt+Lewo/Prawo) na serwerze
- [x] Uprawnienia/chmod na serwerze: komenda `chmod <ósemkowo>` na zaznaczeniu, SFTP (setstat) i FTP/FTPS (`SITE CHMOD`); bez rekurencji, bez wyświetlania uprawnień w kolumnie (do zrobienia)
- [x] Okno kolejki zadań (Ctrl+J): bieżące z postępem, oczekujące (usuwanie), zakończone (ponawianie), pauza kolejki; testy w silniku
- [ ] Kolejka: zmiana kolejności zadań, równoległe transfery, ograniczenie prędkości
- [ ] ~~Wtyczki~~ pominięte (decyzja właściciela); w zamian ewentualnie osobny etap: skaner sieci IP
- [x] Hasła w systemowym magazynie (Secret Service / Menedżer poświadczeń) i zapis nowego hasła do Bitwarden (`bw create item`); `sitepassword`; testy na atrapach (bez prawdziwego keyringu i Bitwarden)
- [ ] Do sprawdzenia ręcznie: prawdziwy keyring (Linux Mint: Gnome Keyring), Windows Credential Manager, `bw` z odblokowanym sejfem

## Znane uproszczenia szkieletu
- Interfejs sprawdzony ręcznie na Linuksie (Linux Mint, X11, wgpu); **nie był jeszcze uruchamiany
  na Windows** (kompiluje się i przechodzi CI).
- Kursor startuje na wierszu `..`, więc `copy`/`move`/`delete` bez zaznaczenia i bez ruchu kursora
  zgłaszają „nic nie zaznaczono”.
