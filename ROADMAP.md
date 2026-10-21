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
| 10 | Nowoczesne GUI | ✅ | okno wygląda jak dopracowane narzędzie, a nie prototyp; logika w `engine` bez zmian |
| T | Poprawki po testach ręcznych | ⬜ | błędy z `docs/TESTOWANIE.md` (Linux i Windows) naprawione, każdy z testem |
| 11 | Fundamenty pod dalszy rozwój | ⬜ | nowe narzędzie to jeden moduł; 100 tys. plików bez zacinania; jedna konfiguracja; testy GUI w CI |
| 12 | Nawigacja i wygoda | ⬜ | każdą funkcję da się znaleźć bez README; mysz jest pełnoprawna obok klawiatury |
| 13 | Automatyzacja | ⬜ | makra, własne przyciski narzędzi, dziennik operacji z cofaniem |
| 14 | Funkcje dodatkowe | ⬜ | porządek na dysku, sieć, środowisko pracy, lepszy podgląd |

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
- [x] RTF, PPTX i ODP: strony przez LibreOffice, bez niego tekst (RTF: własny prosty czytnik z cp1250/Unicode; PPTX i ODP: tekst slajdów)
- [x] PDF chronione hasłem: F3 pyta o hasło (RC4/AES, test z plikiem z Ghostscript)
- [x] Crate `bluepane-docs`: F3 na PDF pokazuje strony jako obraz (hayro, czysty Rust); DOCX/ODT/ODS/XLSX przez LibreOffice -> PDF (jeśli jest), inaczej tekst dokumentu; F4 otwiera w programie systemu
- [x] Okno stron (zmiana strony, powiększenie, dopasowanie do szerokości), render w tle
- [x] Dokumenty z archiwum i serwera otwierają się jak lokalne: strony (PDF, Office przez LibreOffice) albo tekst, PDF z hasłem pyta o hasło
- [ ] Nie ma: edycji w programie, zaznaczania tekstu na stronie (OCR skanów odrzucony przez właściciela); do sprawdzenia ręcznie z prawdziwymi dokumentami i LibreOffice

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

### Etap 10: nowoczesne GUI (plan)

Źródło: analiza właściciela (2026-10-15). Silnik jest niezależny od `ui-egui`, więc zmiana dotyczy tylko rysowania; reguły warstw i komendy zostają.
Każdy podetap = osobna gałąź i PR, GUI sprawdzane ręcznie na Windows i Linuksie.

Decyzje właściciela (2026-10-15):
- Wariant **B**: `egui` z **dokowaniem** (`egui_dock` albo `egui_tiles`; wybór biblioteki po próbie w 10h, ma działać na egui 0.36): panele plików, podgląd, kolejka jako przesuwalne zakładki. Inny toolkit (`iced`/`gpui`) odpada.
- **Motywy** (korekta 2026-10-15): Classic Blue odpada; **Modern Dark** na pewno; jasny motyw (albo Catppuccin) dołożymy, jeśli właściciel zdecyduje po obejrzeniu. Mechanizm gotowy na kolejne motywy: nazwa w `bluepane_engine::THEMES`, paleta w `ui-egui/src/theme.rs`, komenda `theme <nazwa>`, wybór zapisany w `settings.conf`.
- Ikony: **czcionka ikon wbudowana w program** (np. `egui-phosphor`); zgoda na tę jedną zależność, trafia do `[workspace.dependencies]`.
- Zakres: Quick View w drugim panelu, pasek rozmiaru w kolumnie `Size`, mini-pasek zadania na dole, podświetlanie zmian w oknach zmiany nazw i synchronizacji.

- [x] 10a Fundament: moduł `theme.rs` (paleta, `Visuals`, gęstsze odstępy, ostrzejsze rogi), motyw Modern Dark, komenda `theme` i zapis wyboru; ramka i tło odróżniają panel aktywny od nieaktywnego, kursor z obrysem, wiersze naprzemienne, katalogi/zaznaczone/ukryte w kolorach palety
- [x] 10b Ikony i kolory typów plików (klasa pliku liczona w silniku: `class_of`; czcionka ikon Phosphor): ikona przy nazwie (katalog, archiwum, dokument, serwer, plik wykonywalny), kolory semantyczne (katalogi, archiwa, dokumenty, ukryte), zaznaczenie (Insert/Spacja) odróżnione od kursora
- [x] 10c Lista plików: nagłówki kolumn z kierunkiem sortowania i kursor/zaznaczenie rysowane osobno były już w 10a; dodane: pasek rozmiaru w komórce rozmiaru, komenda `dirsize` (rozmiary katalogów w tle na życzenie, Alt+Shift+Enter) i podsumowanie zaznaczenia pod listą
- [x] 10d Nawigacja (nowa zależność `fs4` do wolnego miejsca; wolne miejsce pytane w tle, nie blokuje okna): ścieżka jako klikalne segmenty (breadcrumbs), edycja ścieżki po kliknięciu pustego miejsca (jak `cd`), pasek napędów z zajętością i wolnym miejscem, odznaka aktywnego filtra
- [x] 10e Dolny pasek (dwa rzędy przycisków, nic się nie ucina; pasek zadania z prędkością i czasem do końca; długa ścieżka okruszków mieści się w panelu): klawisze funkcyjne jako dwukolorowe przyciski (skrót + nazwa), mini-pasek bieżącego zadania z prędkością i postępem oraz skrótem do okna kolejki (Ctrl+J)
- [x] 10f Zmiana nazw i synchronizacja czytelne na pierwszy rzut oka: w zmianie nazw tylko nowe znaki podświetlone (`changed_spans` w crate `rename`), znak gotowości/konfliktu; w synchronizacji ikony i kolory kierunków (zielona strzałka w prawo, niebieska w lewo, czerwony kosz i ostrzeżenie, szare „równa się”) oraz delikatne tło wiersza
- [x] 10g Quick View w drugim panelu (Ctrl+Q): podgląd pliku pod kursorem (tekst/hex, strona PDF, tekst dokumentów, katalog) zamiast listy, bez osobnego okna; wyłączony skrót egui Ctrl+Q = wyjście; tylko pliki lokalne, bez obrazów (do zrobienia)
- [x] 10h Dokowanie (`egui_dock` 0.21, `serde`/`serde_json` do zapisu układu w `layout.json`; wymagany Rust 1.95): panele i wszystkie narzędzia jako zakładki, kilka narzędzi naraz, domyślnie po prawej stronie paneli; fokus zakładki kieruje skróty. Do zrobienia: wąskie panele (nagłówki kolumn nakładają się), „Zapisz/Resetuj układ” jako komendy, Quick View jako zakładka zamiast zastępowania panelu

Koncepcja wyglądu od właściciela (2026-10-15, tylko inspiracja, **nie zatwierdzony projekt**): `docs/concepts/gui-mockup.html` (otwórz w przeglądarce; przełączniki motywu, narzędzi i wariantu). Z niej wynika m.in.: zakładki nad panelem z ikoną typu (katalog lokalny, serwer), plakietka woluminu z rozmiarem obok ścieżki-okruszków, nagłówek kolumn `Nazwa ▲ / Typ / Rozmiar / Zmodyfikowano`, paski rozmiaru w komórce rozmiaru, zaznaczone pliki bursztynowe i kursor z obrysem, pasek stanu panelu („Zaznaczono: 2 pliki (48.2 KB) z 10 elementów”, „Filtr: brak (pisz, aby filtrować)”), dok po prawej z zakładkami (Inspektor PDF/Docs, Skaner LAN), na dole pasek kolejki z prędkością nad wierszem poleceń i rzędem klawiszy funkcyjnych z dwukolorowymi przyciskami (F2, F3, F4, F5, F6, F7, F8, ^F2, Alt+S, ^Q). Motywy w makiecie: Modern Dark, Classic Blue (TC) i Catppuccin; w planie trzeci motyw to jasny, więc do rozstrzygnięcia przy 10a (może być i Catppuccin).

Już jest (nie robimy od nowa): zakładki katalogów w panelu (`tab`, Ctrl+T), pasek napędów jako przyciski, własne rysowanie wierszy w kolumnach, filtr „wpisz, żeby zawęzić”, okno kolejki.
Skróty (decyzja właściciela): zostają nasze obecne (Ctrl+S witryny, Ctrl+M maska zaznaczenia itd.); skrótów z analizy nie przejmujemy, nowe komendy dostają wolne skróty, poprawki w codziennym użyciu. Dokowanie (10h) robimy (zakładki katalogów w panelu już są, to osobna rzecz); każdy element to `Command` z nazwą w wierszu poleceń. Nowe zależności (czcionka ikon, biblioteka dokowania, ewentualnie `egui_extras`) tylko przez `[workspace.dependencies]`.

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
- [x] FTP: tryb aktywny (przełącznik w witrynie, `siteset <n> mode active`; testy FTP i FTPS)
- [ ] FTP: ustawianie daty pliku na serwerze, serwery bez SIZE/MDTM (część rzadkich serwerów); sprawdzić ręcznie z prawdziwym serwerem FTP/FTPS (też tryb aktywny)
- [x] F6 (przenoszenie w każdą stronę, także w obrębie serwera), podgląd F3 pliku z serwera (plik tymczasowy), kopiowanie serwer -> serwer (przez katalog tymczasowy)
- [x] F4 pliku z serwera (kopia w edytorze, odesłanie `editsend`), zakładki i historia (Alt+Lewo/Prawo) na serwerze
- [x] Uprawnienia/chmod na serwerze: komenda `chmod <ósemkowo>` na zaznaczeniu, SFTP (setstat) i FTP/FTPS (`SITE CHMOD`); bez rekurencji, bez wyświetlania uprawnień w kolumnie (do zrobienia)
- [x] Okno kolejki zadań (Ctrl+J): bieżące z postępem, oczekujące (usuwanie), zakończone (ponawianie), pauza kolejki; testy w silniku
- [x] Kolejka: zmiana kolejności oczekujących zadań (`queuemove`, przyciski w oknie Ctrl+J)
- [x] Kolejka: ograniczenie prędkości (`limit`, lista w oknie Ctrl+J)
- [ ] Kolejka: równoległe transfery: pominięte na razie (decyzja właściciela, 2026-10-15); wymagają przebudowy (jedno `current` w sesji, jedno pytanie o konflikt naraz); ewentualnie wąska wersja tylko dla serwerów
- [ ] ~~Wtyczki~~ pominięte (decyzja właściciela); w zamian ewentualnie osobny etap: skaner sieci IP
- [x] Hasła w systemowym magazynie (Secret Service / Menedżer poświadczeń) i zapis nowego hasła do Bitwarden (`bw create item`); `sitepassword`; testy na atrapach (bez prawdziwego keyringu i Bitwarden)
- [ ] Do sprawdzenia ręcznie: prawdziwy keyring (Linux Mint: Gnome Keyring), Windows Credential Manager, `bw` z odblokowanym sejfem

### Etapy T i 11–14: plan po audycie (2026-10-15)

Źródło: audyt repozytorium „świeżym okiem" i propozycje zaakceptowane przez właściciela do
wprowadzenia (kolejność: T, 11, 12, 13, 14). Zasady jak dotąd: jeden podetap = jedna gałąź = jeden
PR, scalanie dopiero po „scalaj" i zielonym CI na obu systemach, `cargo clean` po etapie, GUI
oglądane na zrzutach i sprawdzane ręcznie przez właściciela. **Każda nowa zależność wymaga zgody
właściciela** (kandydaci wypisani przy podetapach). Skróty: nowe komendy dostają wolne klawisze
(wolne dziś m.in. F1, Ctrl+P, Ctrl+G, Ctrl+B, Ctrl+K), istniejących nie zmieniamy bez uzgodnienia.

#### Etap T: poprawki po testach ręcznych

- [ ] Zebrać wyniki z `docs/TESTOWANIE.md` (tabela, opisy błędów, zrzuty) i ułożyć listę według wagi
- [ ] Naprawiać od najpoważniejszych (utrata danych, zawieszenia, Windows), każdy błąd z testem odtwarzającym
- [ ] Skróty i drobiazgi z uwag „na później" po decyzji właściciela

#### Etap 11: fundamenty pod dalszy rozwój

Wnioski z audytu: `session.rs` ma ok. 4400 linii i 55 publicznych pól (stan każdego narzędzia
osobno, UI czyta pola wprost); część danych panelu liczy się w każdej klatce; pięć różnych
formatów konfiguracji; skróty nie są konfigurowalne; brak testów GUI.

- [x] 11a (wersja okrojona: lista narzędzi `Tool` w silniku, dok i klawisze z jednego miejsca; stan narzędzi nadal w `Session`) Narzędzia jako moduły: wspólny interfejs narzędzia (stan, komendy, klawisze, zamykanie, tytuł zakładki) zamiast osobnych pól i gałęzi w `Session`; dok i obsługa klawiszy biorą listę narzędzi z jednego miejsca. Bez zmian dla użytkownika, wszystkie testy przechodzą bez zmian
- [x] 11b (statystyki panelu liczone raz i buforowane: `panel_stats`, liczniki zmian zaznaczenia/listy/rozmiarów katalogów; kolumny panelu chowają się w wąskim panelu, nazwy zostają czytelne; testy 100 tys. wpisów) Wydajność listy: podsumowanie panelu, największy rozmiar (paski), klasa pliku i okruszki liczone przy zmianie listy, nie w każdej klatce; test na 100 tys. plików (czas klatki, pamięć) jako cel etapu 3
- [x] 11c (zrobione: `settings.conf` ma motyw i skróty, `bind`/`unbind` z zapisem, błędne wiersze pomijane z ostrzeżeniem; nie zrobione celowo: format TOML, przenoszenie ulubionych skanera i układu doku (mają osobny cykl życia), skróty okien podglądu i stron) Jedna konfiguracja: `settings.toml` (albo JSON) z wersją formatu i przeniesieniem starych plików (`settings.conf`, `scan_favorites.conf`, układ; `sites.conf` i `known_hosts` zostają osobno lub wchodzą po decyzji); **konfigurowalne skróty** (sekcja `[keys]`, komenda `bind <klawisz> <komenda>`), z walidacją, że komenda istnieje
- [x] 11d (zrobione bez porównywania zrzutów: testy struktury widżetów i klików, bez GPU; 6 scenariuszy w `crates/ui-egui/tests/gui.rs`) Testy GUI w CI: `egui_kittest` (zależność do zgody) — scenariusze: start, otwarcie narzędzia w doku, Quick View, zmiana nazw z podświetleniem; porównanie zrzutów z wzorcem
- [x] 11e (zrobione: `suppaftp` 12.2.1 z poprawką od autorów zamiast `vendor/`, crate `bluepane-text` ze wspólną tabelą CP1250/CP1252, Windows release bez konsoli (testowe buildy z `--features console`), `bluepane.log` z uruchomieniami i panikami, ikona okna; ikona pliku .exe w Eksploratorze wymaga zasobu w pliku wykonywalnym i jest do zrobienia z etapem 9) Dług techniczny: łatka `suppaftp` zgłoszona upstream (i usunięcie `vendor/` po wydaniu), wspólna tabela CP1250 (dziś w dwóch crate'ach), na Windows bez okna konsoli w wersji release (z plikiem logów zamiast niej), ikona aplikacji, plik logów (`bluepane.log` w katalogu konfiguracji) do zgłaszania błędów
- [x] 11f (zrobione: README krótki, `docs/FUNKCJE.md`, `docs/KLAWISZE.md`, `docs/KOMENDY.md` generowane z `command_help.rs`; testy pilnują zgodności; przy okazji dopisane 12 komend brakujących w `COMMAND_NAMES`) Dokumentacja: README podzielone na „dla użytkownika" (krótko) i dokumentację funkcji (`docs/`); tabela klawiszy tylko ze skrótami (opisy funkcji osobno); lista komend z opisami generowana z kodu

#### Etap 12: nawigacja i wygoda

- [x] 12a **Paleta komend** (F1): wyszukiwanie rozmyte po wszystkich komendach z opisem po polsku i skrótem; ostatnio używane na górze; opisy komend w silniku (jedno źródło dla palety, pomocy i README)
- [x] 12b **„Skocz do"** (Ctrl+P): katalogi z rankingiem częstość + świeżość (historia zapisywana między uruchomieniami), zakładki, witryny serwerów, udziały ze skanera; Enter przechodzi w aktywnym panelu
- [x] 12c **Pasek „Miejsca"** (chowany, Ctrl+B): dyski z zajętością, ulubione katalogi (dodaj/usuń), serwery, udziały sieciowe, ostatnie; zastępuje rozproszone listy (dyski nad panelem, Ctrl+S, Ctrl+N jako punkty wejścia)
- [x] 12d **Omnibox** zamiast pola ścieżki: ścieżka, `sftp://użytkownik@host/katalog`, `ftp://`, `smb://`, wnętrze archiwum, `?wzorzec` (szukanie), `>komenda`; podpowiedzi przy pisaniu
- [x] 12e Okruszki z rozwijaniem: klik w `›` między segmentami pokazuje katalogi rodzeństwa
- [x] 12f **Menu kontekstowe** pod prawym przyciskiem (otwórz w programie, kopiuj ścieżkę/nazwę, spakuj, rozpakuj, porównaj, właściwości, chmod na serwerze, dodaj do ulubionych); pozycje to komendy
- [x] 12g **Przeciąganie**: między panelami (kopiuj; z Shift przenieś), upuszczanie plików z pulpitu/menedżera systemu do panelu
- [x] 12h **Zmiana nazwy w wierszu** (F2 edytuje nazwę na liście; rozszerzenie nie jest zaznaczane) i **powiadomienia** po skończonych zadaniach (w rogu okna, z przyciskiem „Cofnij", gdzie się da)
- [x] 12i **Pasek akcji zależny od zaznaczenia**: archiwum → „Rozpakuj tutaj / do podkatalogu", PDF/dokument → „Podgląd", dwa katalogi → „Porównaj / Synchronizuj", plik na serwerze → „Edytuj"
- [x] 12j **Ściąga klawiszy**: przytrzymanie Ctrl (albo `?`) nakłada skróty na przyciski; wyraźny wskaźnik, która zakładka doku ma klawiaturę
- [x] 12k (zrobione: podgląd obrazów w F3, Quick View, archiwach i na serwerze; widok miniatur Ctrl+Shift+G / `viewmode`, miniatury w tle dla widocznych kafelków; zależność `image` tylko z PNG/JPEG/GIF/BMP/WebP/ICO) **Obrazy**: widok miniatur (przełącznik lista/siatka) i podgląd obrazów w Quick View i F3 (zależność `image` do zgody; tylko popularne formaty)
- [x] 12l (część zrobiona: wybór kolumn Typ / Rozmiar / Data / Utworzono i gęstość zwarta / wygodna, zapis w `settings.conf`; jasny i systemowy motyw (`theme light|system`); kolumny Prawa i Właściciel (Linux; na Windows puste) i szerokość przeciągana myszą; SHA-256 (`hash`) i status gita (`gitstatus`) na żądanie, bez pracy w tle bez zgody) **Kolumny**: wybór kolumn (uprawnienia, właściciel, data utworzenia, suma SHA-256, status gita), szerokość przeciągana myszą, zapamiętana; gęstość zwarta/wygodna; **jasny motyw** (po decyzji właściciela) 
- [x] 12m Dok — poprawki z etapu 10 (wąskie panele: zrobione w 11b): komendy `layout save`/`layout reset`, Quick View jako zakładka doku (do wyboru zamiast zastępowania panelu), oba panele zawsze widoczne
- [x] 12n (F1 i Ctrl+P zostały osobnymi oknami; brak jeszcze akcji „otwórz katalog nadrzędny / pokaż w drugim panelu / kopiuj ścieżkę” na wierszach) **Ctrl+K: jedna skrzynka** (decyzja 2026-10-16, opcja 2): grupy Komendy (ranking z palety), Miejsca (z „Skocz do”: katalogi, zakładki, serwery) i Pliki w bieżącym katalogu (filtr po liście, bez skanowania); `?` uruchamia głębokie szukanie (okno `find`, z anulowaniem). F1 i Ctrl+P zostają jako skróty otwierające skrzynkę w trybie komend / miejsc. Wyniki mają jawne źródło i przejście do lokalizacji (Enter, otwórz katalog nadrzędny, pokaż w drugim panelu, kopiuj ścieżkę). Bez niejawnego skanowania dysków; grupa „obszary robocze” po 12s
- [x] 12o (zrobione dla nazw: maski, opcja „polskie znaki” / `findset nameaccents`; treść plików i wyrażenia regularne bez zmian) **Szukanie plików bez diakrytyki**: opcja w oknie `find` (nazwy i treść), domyślnie włączona dla nazw; dokładne dopasowania nadal wyżej, nazwy plików się nie zmieniają; ten sam sposób składania liter co w palecie i „Skocz do”
- [x] 12p (zrobione: rozmiar tekstu, wysokość wiersza, akcent, widoczny fokus; motyw systemowy czeka na jasny motyw) **Wygląd**: motyw systemowy (jasny/ciemny według ustawienia systemu, jeśli da się to wiarygodnie wykryć bez nowych zależności), rozmiar tekstu, wysokość wiersza i kolor akcentu w `settings.conf`; tokeny odstępów i typografii obok tokenów kolorów; widoczny fokus klawiatury. Jasny motyw nadal czeka na decyzję właściciela (patrz 12l)
- [x] 12q (zrobione bez presetów: kryteria w tekście filtra, licznik „Widoczne N z M”) **Filtr katalogu**: oprócz nazwy także typ, rozszerzenie, rozmiar, data, tylko katalogi / tylko pliki; licznik „widoczne z wszystkich” i czyszczenie filtrów jednym działaniem; presety dopiero po ustaleniu zapisu między sesjami
- [x] 12s **Obszary robocze i przywracanie sesji** (przeniesione z 14i, decyzja 2026-10-16): zapis katalogów obu paneli, zakładek, aktywnego panelu, układu doku i widoczności pasków; odtwarzanie przy starcie (domyślnie włączone, wyłączane w ustawieniach); nazwane obszary („Praca", „Domowy serwer"): zapisz, nazwij, duplikuj, przywróć, usuń; przed przywróceniem sprawdzenie dostępności katalogów i serwerów z czytelnym zastępstwem; wersja formatu zapisu, bez nadpisywania istniejącej konfiguracji

#### Etap 13: automatyzacja (wykorzystanie modelu komend)

- [x] 13a **Nagrywanie makr**: zapis wykonanych komend do pliku `.bp`; odtwarzanie skrótem, z palety i z `bluepane-cli run-script`; edycja makra jako tekstu; makra z parametrami (zaznaczenie, katalog drugiego panelu)
- [x] 13b **Własne przyciski narzędzi**: zewnętrzne programy z parametrami `%f` (plik), `%d` (katalog), `%s` (zaznaczone), `%o` (drugi panel); w osobnym rzędzie przycisków i w palecie; zastępują wtyczki bez kodu wtyczek
- [x] 13c ~~Dziennik operacji z cofaniem~~ — **odrzucone przez właściciela (2026-10-16)**; zostaje tylko `mrundo` dla zmiany nazw
- [x] 13d Harmonogram (makra gotowe; profile synchronizacji dojdą z 14h): makro albo profil synchronizacji uruchamiane o zadanej porze (przez `bluepane-cli` i zadanie systemu: cron / Harmonogram zadań Windows; bluepane tylko je zakłada i pokazuje)

#### Etap 14: funkcje dodatkowe

Porządek na dysku:
- [x] 14a **Mapa zajętości** (kafelki jak WizTree) dla katalogu, na bazie `dirsize`; klik w kafelek wchodzi do katalogu
- [x] 14b **Duplikaty**: rozmiar → skrót → porównanie bajt po bajcie; zaznaczanie kopii regułą (najnowsza/najstarsza zostaje), usuwanie do kosza
- [x] 14c **Porównanie dwóch plików** (tekst obok tekstu z podświetleniem różnic, dla binarnych pierwsza różnica) oraz **sumy kontrolne**: obliczanie i sprawdzanie `.sha256` / `.md5`
- [x] 14d Zmiana nazw z danymi z pliku: data **EXIF** zdjęcia (`[EXIF:YMD]`), tagi audio (zależności do zgody)

Sieć:
- [x] 14e **Udziały SMB jako katalog w panelu** (`smb://host/udział`) bez montowania w systemie (`smbclient` jako proces, bez nowej zależności; bez smbclienta stara droga: UNC / GVFS)
- [x] 14f Skaner: **Wake-on-LAN**, „dodaj jako witrynę" i „otwórz udział w panelu" jednym kliknięciem, historia skanów (nowe urządzenia oznaczone)
- [x] 14g Chmury przez **`rclone`** (gdy zainstalowany): WebDAV, S3, Google Drive, OneDrive jako katalogi w panelu
- [x] 14h Zapisane **profile synchronizacji** (para katalogów + ustawienia) i uruchamianie z harmonogramu (13d)

Środowisko pracy:
- [x] 14i przeniesione do 12s (obszary robocze i przywracanie sesji, tuż po etapie 12)
- [x] 14j **Terminal tutaj i polecenie z wynikiem** (zamiast wbudowanego terminala, decyzja właściciela 2026-10-21; bez zależności): komenda `terminal` otwiera systemowy terminal w katalogu aktywnego panelu; `run` / `!polecenie` uruchamia program w katalogu panelu (bez powłoki, ze znacznikami `%f` `%d` `%s` jak we własnych przyciskach) i pokazuje wyjście w podglądzie; wstawianie nazwy pliku do linii poleceń zastępują znaczniki `%f` `%n` `%s`; komenda `runstop` przerywa program, skrót Ctrl+Shift+T otwiera terminal. Wbudowany terminal (`portable-pty` plus emulacja) odłożony: systemowy będzie zawsze lepszy, a koszt emulacji duży
- [x] 14k **Obserwacja katalogów**: lista odświeża się sama po zmianach na dysku (`notify`, za zgodą; `watch on|off`)
- [x] 14l **Git**: status plików w kolumnie (był w 12l), gałąź przy ścieżce, podstawowe akcje `git add|unstage|commit|diff|log` (`git` jako proces, bez nowej zależności)
- [x] 14m **Szukanie z indeksem** (`index`): wyniki natychmiast, także po treści PDF/Word/Excel (tekst z crate'a `docs`), tylko na polecenie użytkownika; zapisane wyszukiwania jako wirtualne katalogi
- [x] 14n **Podgląd**: kolorowanie składni kodu, renderowany Markdown, edytor hex (z zapisem przez kopię i potwierdzenie), metadane plików multimedialnych (bez nowych zależności: własny tokenizer i renderer, `lofty` i `kamadak-exif` z 14d)

### Testy ręczne (przygotowane 2026-10-15)

`docs/TESTOWANIE.md` (uruchomienie na Linuksie i Windows, lista kontrolna z oczekiwanymi wynikami), `testdata/` (gotowe pliki), `tools/dane-testowe.sh` i `.ps1` (reszta danych), workflow `Build testowy` (gotowe pliki release do pobrania, nie jest to dystrybucja).

Rozszerzone 2026-10-21 o etapy 11–14: `docs/TESTOWANIE.md` ma teraz **dym** (ok. 20 minut), tabelę priorytetów (P1 = nigdy nie uruchomione na Windows ani na prawdziwych danych i usługach) oraz sekcje 4.17–4.30 dla nowych narzędzi; skrypty danych tworzą pliki do kolorowania składni, Markdown, edytora hex, duplikatów, porównania, indeksu, EXIF i tagów oraz repozytorium git. Wyniki wpisuje się partiami (tabela w sekcji 5) i trafiają do etapu T.

## Znane uproszczenia szkieletu
- Interfejs sprawdzony ręcznie na Linuksie (Linux Mint, X11, wgpu); **nie był jeszcze uruchamiany
  na Windows** (kompiluje się i przechodzi CI).
- Kursor startuje na wierszu `..`, więc `copy`/`move`/`delete` bez zaznaczenia i bez ruchu kursora
  zgłaszają „nic nie zaznaczono”.
