# Funkcje

Opis tego, co robi program, po obszarach. Skróty: [KLAWISZE.md](KLAWISZE.md); komendy: [KOMENDY.md](KOMENDY.md);
lista kontrolna do testów: [TESTOWANIE.md](TESTOWANIE.md).

## Okno, nawigacja i wygląd

- pasek rozmiaru za kolumną Rozmiar (długość względem największego pliku lub policzonego katalogu) i podsumowanie pod listą: „Zaznaczono: 2 pliki (48.2 KB) z 10 elementów” albo suma katalogu
- **Układ okna jako dok**: oba panele i każde narzędzie (podgląd pliku, strony dokumentu, synchronizacja, szukanie, zmiana nazw, skaner, kolejka, serwery) to zakładki, które można przeciągać, dzielić, wyciągać do osobnego okna i zamykać (×). Otwarte narzędzie dostaje zakładkę po prawej stronie paneli (kilka narzędzi naraz jako zakładki obok siebie); skróty narzędzia działają, gdy jego zakładka ma fokus (ostatnia, w którą kliknięto). Układ paneli zapisuje się w `layout.json` w katalogu konfiguracji; narzędzia przy starcie są zamknięte
- zmiana nazw (Ctrl+F2): w kolumnie nowych nazw podświetlone tylko dodane znaki, zielony znak = gotowe, ostrzeżenie = konflikt; synchronizacja (Alt+S): zielona strzałka kopiuje w prawo, niebieska w lewo, czerwony kosz usuwa, ostrzeżenie to konflikt
- na dole: pasek bieżącego zadania (nazwa, postęp, bajty, prędkość i czas do końca, Anuluj, przycisk „Kolejka”) oraz dwa rzędy przycisków klawiszy funkcyjnych (klawisz na ciemnej plakietce i nazwa akcji), rozciągnięte na całą szerokość okna
- nad panelem: lista dysków z paskiem zajętości i „wolne z”, ścieżka jako klikalne segmenty (kliknięcie = `cd` do tego miejsca; kliknięcie pustego miejsca paska zamienia ścieżkę w pole do wpisania), a aktywny filtr jako odznaka z ✕
- ikony i kolory typów plików: katalogi, archiwa, dokumenty, obrazy, audio/wideo, kod, programy (klasa pliku z nazwy, bez otwierania)
- motyw kolorów: komenda `theme` (pokazuje dostępne), `theme dark`; wybór zapisuje się w `settings.conf`
- **własne skróty**: `bind ctrl+k sort name` (przypisz), `bind ctrl+k` (pokaż), `bind ctrl+k default` (przywróć domyślny), `unbind f9` (wyłącz), samo `bind` (lista zmian); zapisują się w tym samym `settings.conf` (wiersze `key Ctrl+K = sort name`, `key F9 =` wyłącza klawisz); błędny wiersz jest pomijany z ostrzeżeniem w pasku stanu. Dotyczy skrótów paneli; klawisze okien (podgląd, strony) są na razie stałe

## Paleta komend (F1)

F1 otwiera okno z polem szukania nad wszystkimi komendami. Pisz fragment nazwy albo opisu (polskie litery można pomijać: `sciezka` znajdzie `cd`); wszystkie wpisane słowa muszą pasować. Strzałki ↑↓ zmieniają wybór, Enter uruchamia komendę, Esc zamyka okno, klik w wiersz uruchamia ten wiersz. Wiersz pokazuje składnię i opis; skrót klawiszowy jest w nawiasie na końcu opisu.

Ostatnio uruchomione z palety komendy (pogrubione) są na górze i wygrywają remisy; lista jest zapamiętana między uruchomieniami (plik `recent_commands.txt` w katalogu konfiguracji). Komenda, która wymaga argumentu (np. `cd`, `selmask`), nie uruchamia się od razu: trafia do wiersza poleceń z końcową spacją, żeby dokończyć ją z klawiatury.

Komenda: `palette [tekst]` (otwiera paletę z tekstem), `palette query <tekst>`, `palette move <n>`, `palette run`, `palette close`. Lista pochodzi z tej samej tabeli opisów co [KOMENDY.md](KOMENDY.md), więc nowa komenda pojawia się w palecie sama.

## Menu kontekstowe

Prawy przycisk myszy (albo Shift+F10 z klawiatury) otwiera menu dla pozycji pod kursorem. Prawy klik na wierszu przesuwa na niego kursor (jeśli wiersz jest zaznaczony, zaznaczenie zostaje i menu dotyczy całego zaznaczenia); na pustym miejscu listy menu dotyczy katalogu. Każda pozycja to zwykła komenda (obok jest skrót klawiszowy, jeśli jakiś jest), więc robi dokładnie to samo co skrót, paleta czy wiersz poleceń:

- **katalog**: Otwórz, Otwórz w nowej zakładce, Dodaj do ulubionych,
- **archiwum**: Otwórz archiwum, Rozpakuj do drugiego panelu, Sprawdź archiwum,
- **plik**: Otwórz w programie (domyślny program systemu, komenda `open`), Podgląd, Edytuj,
- **dla wszystkich**: Kopiuj / Przenieś do drugiego panelu, Zmień nazwę (jeden element) albo Zmień nazwy zbiorowo (kilka), Spakuj, Usuń, Kopiuj ścieżkę / nazwę, Uprawnienia (chmod) na serwerze, Właściwości (komenda `info`: rodzaj, rozmiar, data i ścieżka w pasku stanu albo podsumowanie zaznaczonych),
- **puste miejsce**: Nowy katalog, Odśwież, Zaznacz wszystko, Odwróć zaznaczenie, Szukaj plików, Synchronizuj katalogi, Nowa zakładka, Dodaj ten katalog do ulubionych.

W archiwach menu nie ma pozycji, które zmieniałyby archiwum (przenoszenie, zmiana nazwy, usuwanie). Esc albo klik poza menu zamyka je.

## Okruszki z rozwijaniem

Strzałka `›` między segmentami ścieżki nad panelem otwiera menu z katalogami leżącymi w segmencie po jej lewej stronie (posortowane po nazwie, bez plików; do 300 pozycji); klik w katalog przechodzi do niego w tym panelu. Dzięki temu do katalogu obok można przejść bez wchodzenia wyżej. Menu działa dla katalogów na dysku (dla serwerów strzałka nic nie wyświetla).

## Pole ścieżki (omnibox)

Klik w puste miejsce za okruszkami nad panelem zamienia je w pole tekstowe; Enter robi to, co oznacza wpisany tekst (komenda `go <adres>`):

- **ścieżka** (też względna, wnętrze archiwum, katalog na serwerze) – przejście tam,
- **`sftp://użytkownik@host:port/katalog`** (też `ftp://`, `ftps://`) – połączenie z serwerem i przejście do katalogu. Znaną witrynę (ten sam host, protokół, port i użytkownik) program wykorzysta, nieznaną zapisze jako `użytkownik@host` (hasło pyta przy łączeniu; dla FTP bez użytkownika jest `anonymous`; dla SFTP użytkownik jest wymagany),
- **`smb://komputer/udział`** – otwiera udział w menedżerze plików systemu; `smb://komputer` pokazuje udziały komputera (adres IP, jak w skanerze),
- **`?maska`** – szuka plików o tej nazwie w bieżącym katalogu (otwiera okno szukania z wynikami),
- **`>komenda`** – uruchamia komendę, np. `>sort size`.

Podczas pisania pod polem pojawiają się podpowiedzi: podkatalogi (od trzech znaków, albo po separatorze), komendy po `>` (z palety komend) i zapisane serwery po `sftp://`; klik w podpowiedź wstawia ją do pola.

## Pasek „Miejsca” (Ctrl+B)

Chowany pasek po lewej stronie okna zbiera punkty wejścia w jednym miejscu; klik w wiersz przechodzi tam w aktywnym panelu, a wiersz miejsca, w którym panel jest teraz, jest podświetlony:

- **Dyski** z paskiem zajętości i wolnym miejscem (to samo co `volume`; ostrzeżenie kolorem od 85% i 95% zajętości),
- **Ulubione**: katalogi, które sam dodałeś (`fav` dodaje bieżący katalog albo go usuwa, `fav <ścieżka>` robi to samo dla podanej ścieżki, `fav del <ścieżka>` usuwa; przy wierszu jest też przycisk ✕); zapisane w `favorites.conf` w katalogu konfiguracji; tylko katalogi na dysku,
- **Serwery**: zapisane witryny SFTP/FTP/FTPS (klik łączy aktywny panel),
- **Ostatnie**: osiem ostatnio odwiedzonych katalogów (bez ulubionych i takich, których już nie ma).

Sekcje bez wierszy są ukryte. Komendy: `places` (pokazuje i chowa pasek), `fav`. Udziałów sieciowych ze skanera (Ctrl+N) w pasku jeszcze nie ma.

## „Skocz do” (Ctrl+P)

Jedno pole szukania nad miejscami, do których chodzisz najczęściej. Lista składa się z:

- **katalogów** z historii odwiedzin: im częściej i im niedawniej byłeś w katalogu, tym wyżej (ostatnia godzina liczy się czterokrotnie, ostatnia doba podwójnie, ostatni tydzień pojedynczo, starsze za pół); katalogi, których już nie ma, są pomijane; historia (do 500 katalogów) jest zapisana w `visits.conf` w katalogu konfiguracji,
- **zakładek** aktywnego panelu (poza bieżącą),
- **zapisanych serwerów** (Enter łączy aktywny panel).

Wpisane słowa muszą wszystkie pasować do ścieżki (polskie litery można pomijać); katalog, którego nazwa zaczyna się od pierwszego słowa, jest wyżej. Strzałki ↑↓ zmieniają wybór, Enter przechodzi, Esc zamyka, klik w wiersz przechodzi od razu. Odwiedziny liczą się w katalogach lokalnych (nie w archiwach, na serwerach ani na listach wyników szukania).

Komenda: `jump [tekst]`, `jump query <tekst>`, `jump move <n>`, `jump run`, `jump close`. Udziałów sieciowych ze skanera na tej liście jeszcze nie ma.

## Podgląd plików (F3)

Podgląd czyta tylko widoczne okno pliku, więc plik wielogigabajtowy otwiera się od razu i nie
zajmuje pamięci. Pliki z bajtami zerowymi otwierają się w trybie hex.

| Klawisz | Akcja |
| --- | --- |
| strzałki góra/dół, PgUp/PgDn, Spacja | przewijanie o wiersz / stronę |
| Home / End | początek / koniec pliku |
| strzałki lewo/prawo | przesunięcie w bok (długie linie) |
| H lub F4 | przełącz tekst / hex |
| `/`, F7 lub Ctrl+F | szukaj (bez rozróżniania wielkości liter, ASCII) |
| N / Shift+N (F3 / Shift+F3) | następne / poprzednie trafienie |
| Esc lub Q | zamknij podgląd |

Szukanie przeszukuje do 256 MiB na raz; jeśli nic nie znajdzie w tej części, pokazuje dokąd doszło
i kolejne **N** szuka dalej. Komendy: `view`, `viewer scroll|page|hscroll|top|bottom|mode|goto|find|next|prev|close`.

**Dokumenty jako strony (graficznie):** F3 na pliku `.pdf` pokazuje **strony jako obraz** (renderowanie w czystym Rust, bez programów zewnętrznych): PgUp/PgDn,
strzałki ←/→, Spacja i Backspace zmieniają stronę, Home/End skaczą na pierwszą i ostatnią, `+`/`-` powiększają i pomniejszają, `0` dopasowuje do szerokości okna, Esc/Q zamyka.
Pliki Word (`.docx`, `.docm`), RTF (`.rtf`), PowerPoint (`.pptx`, `.pptm`, `.ppsx`), OpenDocument (`.odt`, `.ods`, `.odp`) i Excel (`.xlsx`) są najpierw zamieniane na PDF przez **LibreOffice** (jeśli jest zainstalowany;
uruchamiany w tle, bez okna, we własnym profilu, z limitem 90 s; `BLUEPANE_OFFICE=<ścieżka>` wskazuje program, `BLUEPANE_NO_OFFICE=1` wyłącza to) i pokazywane tak samo.
Bez LibreOffice, a także gdy konwersja się nie uda, F3 pokazuje **tekst** dokumentu (akapity, nagłówki, listy, tabele, strony PDF). Pliki z archiwum i z serwera
otwierają się tak samo (pobierane do katalogu tymczasowego): strony przez LibreOffice albo tekst, a PDF z hasłem też pyta o hasło. PDF chroniony hasłem: F3 pyta o hasło (pole ukryte; błędne hasło pyta ponownie, Esc rezygnuje). Limit 32 MB dla PDF i 64 MB dla pozostałych.
**F4** otwiera plik w programie systemu (Word, LibreOffice, czytnik PDF) albo w `BLUEPANE_EDITOR`; bluepane nie edytuje dokumentów samodzielnie.


## Archiwa

Obsługiwane formaty: **ZIP**, **7z**, **TAR**, **TAR.GZ** (`.tgz`). Archiwum otwiera się w panelu jak
katalog: F5 rozpakowuje zaznaczone pliki i katalogi do drugiego panelu, a
**Alt+F9** (`unpack`) całe archiwum (`unpack -s` do podkatalogu o nazwie archiwum).

**Pakowanie** (**Alt+F5** lub przycisk „Pakuj”) otwiera okno: nazwa, format, kompresja (bez /
szybka / normalna / maksymalna), hasło, podział na części. Archiwum powstaje w katalogu drugiego
panelu. To samo z linii poleceń, z przełącznikami jak w 7-Zip:

```
pack raport.zip -psekret -mx=9 -v100m     # ZIP z hasłem, maksymalna kompresja, części po 100 MB
pack kopia -ttgz                          # TAR.GZ o nazwie wziętej z nazwy
pack tajne -t7z -psekret -mhe             # 7z z hasłem i ukrytymi nazwami plików
```

| Przełącznik | Znaczenie |
| --- | --- |
| `-tzip`, `-t7z`, `-ttar`, `-ttgz` | format (domyślnie z rozszerzenia nazwy, inaczej ZIP) |
| `-mx=0` … `-mx=9` | kompresja: 0 = bez, 1–3 szybka, 4–6 normalna, 7–9 maksymalna |
| `-p<hasło>` | hasło (ZIP i 7z, szyfrowanie AES-256) |
| `-mhe` | 7z: zaszyfruj też nazwy plików (wymaga `-p`) |
| `-v<rozmiar>` | podział na części `nazwa.zip.001`, `.002`, … (np. `-v700m`, `-v4g`) |

- **Hasła** działają w ZIP i 7z (TAR nie szyfruje, więc hasło dla TAR jest odrzucane, a nie
  ignorowane). W ZIP czyta się ZipCrypto i AES, tworzy AES-256; w 7z AES-256. Przy zaszyfrowanym
  archiwum okno pyta o hasło, a po błędnym pyta ponownie. **Nazwy plików** da się ukryć tylko w
  7z (okno: „Szyfruj też nazwy plików”, komenda: `-mhe`); wtedy hasło jest potrzebne już do
  samego otwarcia archiwum. W ZIP nazwy są zawsze widoczne.
- **7z** używa kompresji LZMA2 (poziomy jak w ZIP). Archiwa 7z utworzone tu są nielite (każdy plik
  osobno), więc rozpakowanie jednego pliku nie wymaga odczytu reszty; archiwa lite z 7-Zip
  czyta się w całości do żądanego pliku (tak działa format). Metody BZip2 i PPMd w 7z nie są
  obsługiwane (dają czytelny błąd).
- **Części**: odczyt wskazuje się na pierwszej części (`*.001`), reszta jest doczytywana
  automatycznie. Podział potrzebuje na dysku miejsca na archiwum dwukrotnie w trakcie pracy.
- `testarchive` sprawdza integralność archiwum. Dowiązania symboliczne i pliki urządzeń są
  pomijane przy rozpakowaniu (z informacją), a nazwy wychodzące poza katalog docelowy
  (`../x`, ścieżki bezwzględne) są odrzucane.
- Istniejące archiwum nie jest nadpisywane bez pytania (nadpisz / pomiń / zachowaj oba).
- **Zmiany w istniejącym archiwum**: dodawanie (F5 z drugiego panelu), usuwanie (F8), zmiana nazwy
  (F2) i nowy katalog (F7). Archiwum jest przepisywane do pliku tymczasowego obok i podmieniane
  dopiero po zakończeniu, więc błąd lub anulowanie nie psuje oryginału. W ZIP wpisy są kopiowane
  bez dekompresji (szybko, a usuwanie i zmiana nazwy nie wymagają hasła); TAR, TAR.GZ i 7z
  trzeba odczytać i zapisać w całości, co trwa proporcjonalnie do rozmiaru (jest pasek postępu i
  Anuluj). Archiwa wieloczęściowe zostają podzielone na części tej samej wielkości, a szyfrowanie
  i ukryte nazwy w 7z są zachowane. Konflikt nazw przy dodawaniu pyta jak przy kopiowaniu.
  Archiwum zmienione na dysku po otwarciu trzeba otworzyć ponownie (program tego pilnuje).

## Synchronizacja katalogów (Alt+S)

Porównuje katalogi obu paneli i **pokazuje listę zmian przed wykonaniem**; nic nie jest
kopiowane ani usuwane, dopóki nie uruchomisz synchronizacji. Warianty: dwukierunkowa, lustro
(lewy -> prawy, z usuwaniem nadmiarowych), aktualizacja (lewy -> prawy, bez usuwania). Porównanie
po czasie (z tolerancją), rozmiarze albo zawartości; filtry wzorców „uwzględnij” / „pomiń”.
Kierunek pojedynczej pozycji można zmienić ręcznie. Usuwanie: do kosza (domyślnie),
do katalogu z wersjami albo trwale (tylko na jawne życzenie). Pozycje niedostępne do odczytu
są zgłaszane i nie są ruszane. Z wiersza poleceń:
`sync [-twoway|-mirror|-update] [-time|-size|-content] [-tol=<s>] [-i<wzorce>] [-x<wzorce>] [-delete=trash|permanent|versions:<katalog>]`,
`syncset <n> <kierunek>`, `syncrun`, `syncclose`.

## Skaner sieci (Ctrl+N)

Pokazuje urządzenia w podanym zakresie adresów IP, jak Advanced IP Scanner. Skanuje tylko to, co wskażesz (domyślnie Twoją sieć /24); nic nie wysyła,
tylko nawiązuje i zrywa połączenia TCP, więc nie potrzeba uprawnień administratora.

- **Zakres:** `192.168.0.1-254`, `192.168.0.90-110`, `10.0.0.0/24`, pojedyncze adresy, kilka po przecinku (najwyżej 65 536 adresów). Przycisk „Moja sieć” wstawia zakres tego komputera.
- **Skąd wiadomo, że urządzenie jest:** przyjmuje połączenie, odmawia go (zamknięty port odpowiada, więc ktoś tam jest) albo ma wpis w tablicy ARP systemu
  (urządzenie z zablokowanymi wszystkimi portami też odpowiada na ARP w sieci lokalnej). Ping ICMP nie jest używany (wymagałby uprawnień).
- **Co o nim wiadomo:** adres IP, nazwa (odwrotny DNS, można wyłączyć), adres MAC (tylko w sieci lokalnej), producent (z wbudowanego rejestru IEEE), opóźnienie
  i **usługi**: otwarte porty z nazwami (HTTP, HTTPS, SSH/SFTP, FTP, RDP, SMB, drukarki, Proxmox 8006, Home Assistant 8123 i inne; własna lista portów, np. `22,80,8000-8010`).
- **Akcje na wyniku:** kliknij wiersz, a pod filtrem pojawią się przyciski usług tego urządzenia:
  HTTP/HTTPS (też Proxmox 8006, Home Assistant 8123) otwierają **przeglądarkę**; RDP uruchamia **pulpit zdalny** (Windows: `mstsc`, Linux: `remmina` albo `xfreerdp`);
  SSH i FTP zapisują urządzenie jako **serwer** w menedżerze (Ctrl+S) i czekają na użytkownika i hasło; SMB (445/139) **szuka udziałów**. Do tego „Kopiuj adres” i gwiazdka „ulubione”.
- **Udziały sieciowe (SMB):** listę udziałów pobiera program systemu (Linux: `smbclient -L`, pakiet `smbclient`; Windows: `net view`), anonimowo albo na Twoim koncie Windows. Gdy serwer
  wymaga logowania, okno mówi to wprost zamiast pokazywać pustą listę. Udziały administracyjne (`C$`, `IPC$`) są ukryte. „Otwórz” otwiera udział w menedżerze plików systemu
  (`\\adres\udział` na Windows, `smb://adres/udział` na Linuksie).
- **Ulubione:** zakładka „Ulubione” trzyma wybrane urządzenia między uruchomieniami (plik `scan_favorites.conf` w katalogu konfiguracji) i pokazuje, które były widoczne w ostatnim skanie.
- **Filtr** zawęża tabelę po nazwie, adresie, producencie, MACu i usługach. Wyniki zostają po zamknięciu okna (`Ctrl+N` pokazuje je znowu). Esc zatrzymuje skanowanie.
- Na Windows tablica ARP jest czytana poleceniem `arp -a`, na Linuksie z `/proc/net/arp`.

Komendy: `scan`, `scanset <pole> <wartość>` (pola: `range`, `ports`, `names`, `arp`, `filter`, `timeout`, `reset`), `scanrun`, `scanstop`, `scanclose`, `scanselect <ip>`, `scanopen <ip> <port>`, `scanshares <ip>`, `scanshare <ip> <udział>`, `scansite <ip> sftp|ftp|ftps`, `scanfav <ip>`, `scanfavorites [off]`, `scancopy <ip>`.

## Kolejka zadań (Ctrl+J)

Kopiowanie, przenoszenie, pakowanie, transfery z serwerów i inne długie zadania idą jedno po drugim. Okno kolejki pokazuje bieżące zadanie z postępem,
oczekujące (każde można **usunąć z kolejki** albo **przesunąć**: ▲ wyżej, ▼ niżej, „Na początek”; to samo robi `queuemove <numer> up|down|top`) i ostatnie zakończone (każde można **ponowić**, np. po błędzie). **Wstrzymaj kolejkę** pozwala
bieżącemu zadaniu się dokończyć, a następne czekają, dopóki ich nie wznowisz; odświeżanie list serwerów i logowanie nie czekają. `stop` przerywa bieżące
i czyści oczekujące.

**Limit prędkości:** `limit 2m` (albo `500k`, `limit off`, samo `limit` pokazuje stan) albo lista w oknie kolejki ogranicza prędkość kopiowania, przenoszenia, pakowania i transferów z serwerami (średnio, w bajtach na sekundę). Działa od razu także na trwające zadanie; obowiązuje do końca sesji (nie jest zapisywany).

## Serwery: SFTP, FTP, FTPS (Ctrl+S)

Panel może pokazywać katalog na serwerze jak zwykły katalog. Obsługiwane: **SFTP** (SSH), **FTP** i **FTPS** (jawny, `AUTH TLS`, oraz niejawny, zwykle port 990). Implementacja w Rust (russh, suppaftp, rustls z ring), bez zewnętrznych programów ani bibliotek do doinstalowania.

- **Witryny (Ctrl+S):** zapisane połączenia: serwer, port, użytkownik, plik klucza (albo hasło), katalog startowy. Plik `sites.conf` w katalogu konfiguracji.
- **Hasło** (dla klucza: fraza klucza), cztery sposoby do wyboru przy każdej witrynie:
  - „pytaj przy łączeniu”: nigdzie nie zapisywane;
  - „w pliku witryn”: jawnym tekstem w `sites.conf` (plik tylko dla Ciebie na Linuksie; okno ostrzega);
  - **magazyn haseł systemu** (Gnome Keyring / KWallet przez Secret Service na Linuksie, Menedżer poświadczeń na Windows): hasło nie trafia do pliku witryn;
  - **Bitwarden** (program `bw`): hasło jest pobierane z istniejącego elementu przy łączeniu, a wpisane w oknie nowe hasło **zapisuje się jako nowy element**
    („bluepane: <nazwa>” albo nazwa, którą podasz); istniejącego elementu bluepane nigdy nie nadpisuje. Sejf musi być odblokowany (`bw unlock`), a `BW_SESSION` ustawione.
  Zapis do magazynu systemu i do Bitwarden dzieje się w tle; komunikaty nigdy nie zawierają hasła.
- **Klucz serwera:** pierwsze połączenie pokazuje odcisk klucza i pyta (Enter = zaufaj i zapamiętaj). Zmieniony klucz znanego serwera jest zawsze odrzucany z ostrzeżeniem.
  W FTPS to samo dotyczy certyfikatu serwera: nie sprawdzamy urzędów certyfikacji (serwery domowe mają własne certyfikaty), tylko przypinamy odcisk SHA-256 certyfikatu.
- **FTP bez szyfrowania:** hasło i pliki idą otwartym tekstem (okno witryny ostrzega); używaj w zaufanej sieci albo wybierz FTPS. Klucze SSH działają tylko z SFTP.
- **Tryb FTP:** domyślnie pasywny (klient łączy się po dane). W oknie witryny (FTP/FTPS) można zaznaczyć **tryb aktywny** (`siteset <nazwa> mode active`): serwer łączy się wtedy do portu otwartego na tym komputerze, co wymaga, by serwer mógł do Ciebie dotrzeć (bez NAT-a i zapory); dane przyjmujemy tylko od adresu serwera, który obsługuje sesję. Brak połączenia zwrotnego kończy się błędem po 15 s.
- **Praca w panelu:** Enter / Backspace / `cd` / F9 przeglądają serwer; F5 pobiera (z drugiego panelu) lub wysyła (do panelu serwera), także całe katalogi;
  konflikty pytają jak przy kopiowaniu (nadpisz / starsze / pomiń / zachowaj oba); F7 nowy katalog, F2 zmiana nazwy, F8 usuwa **trwale** (serwer nie ma kosza, pytanie to mówi wprost).
- **Uprawnienia:** `chmod 755` (albo Ctrl+Shift+P i wpisz liczbę) ustawia uprawnienia zaznaczonych plików lub katalogów na serwerze; zapis ósemkowy, 3 lub 4 cyfry. Bez rekurencji. SFTP zmienia je wprost, FTP/FTPS poleceniem `SITE CHMOD` (część serwerów go nie ma, wtedy dostaniesz komunikat o błędzie). Na lokalnym panelu komenda nic nie robi i mówi dlaczego.
  Pobieranie i wysyłanie idzie przez plik `.bpart` i jest wznawiane po przerwaniu (tylko gdy plik częściowy jest nowszy niż źródło). Postęp i Anuluj jak przy zwykłym kopiowaniu.
- **F6 (przenoszenie):** z serwera na dysk (oryginał na serwerze znika dopiero po pobraniu całości; pominięty konflikt zostawia go), z dysku na serwer
  (oryginał trafia do kosza po wysłaniu całości), między dwoma serwerami (przez katalog tymczasowy) i w obrębie jednego serwera (to zmiana nazwy, bez kopiowania, nigdy nie zastępuje).
- **F4:** plik z serwera jest pobierany do katalogu tymczasowego i otwierany w edytorze (`BLUEPANE_EDITOR` albo domyślny program); po zapisaniu wyślij go z powrotem **Ctrl+Shift+F4** (`editsend`): zastępuje oryginał na serwerze, ale tylko gdy kopia się zmieniła.
- **Historia i zakładki na serwerze:** Alt+Lewo / Alt+Prawo cofa i przywraca odwiedzone katalogi serwera; nowa zakładka (Ctrl+T, Ctrl+Strzałka w górę) wspólnie z panelem używa tego samego połączenia.
- **F3:** plik z serwera jest pobierany do katalogu tymczasowego (limit 256 MB) i pokazany w podglądzie; kopia znika po zamknięciu.
- **F5 między dwoma serwerami** kopiuje przez katalog tymczasowy (każdy bajt idzie dwa razy; potrzebne miejsce na dysku na największy element).
- **Nie ma jeszcze:** uprawnień (chmod), trybu aktywnego FTP.

Komendy: `sites`, `siteset <nazwa> <protocol|host|port|user|key|start|secret> <wartość>` (`secret`: `ask`, `local:<hasło>`, `keyring`, `bitwarden:<element>`; `protocol`: `sftp`, `ftp`, `ftps`, `ftps-implicit`) (nazwa ze spacjami w cudzysłowie; `secret`: `ask`, `local:<hasło>`, `bitwarden:<element>`),
`sitepassword <nazwa> <hasło>` (zapis hasła tam, gdzie witryna je trzyma), `sitedel <nazwa>`, `connect <nazwa> [hasło]`, `disconnect`, `siteclose`.

## Wyszukiwanie (Ctrl+F lub Alt+F7)

Alt+F7 bywa zajęty przez menedżer okien w Linuksie (przesuwanie okna), dlatego jest też Ctrl+F. Szuka w tle (równolegle z kopiowaniem itp.), a wyniki pojawiają się na bieżąco. Nic nie jest zapisywane.

- **Nazwa:** maski (`*.rs;*.md`, `?` = jeden znak) albo wyrażenie regularne; z rozróżnianiem wielkości liter lub bez.
- **Szukaj w:** aktywny panel, oba panele, **wszystkie dyski** albo lista katalogów oddzielonych `;`.
  Katalog zawarty w innym z listy nie jest przeszukiwany dwa razy; dowiązania do katalogów nie są śledzone (brak pętli).
- **Pomiń katalogi:** maski, np. `target;node_modules;.git`.
- **Zawiera tekst:** pełnotekstowo, bez wczytywania pliku w całości (dowolnie duże pliki), z numerem linii i podświetleniem
  trafienia. Zwykły tekst albo wyrażenie regularne, wielkość liter, całe słowa. Kodowanie: automatyczne (UTF-8 / UTF-16 z BOM /
  Windows-1250 dla linii, które nie są poprawnym UTF-8, więc polskie znaki działają też w starych plikach), UTF-8, CP1250 albo UTF-16.
  Pliki binarne są pomijane (można to wyłączyć).
- **Archiwa:** ZIP, TAR, TAR.GZ, 7z (także wieloczęściowe) przeszukiwane bez rozpakowywania na dysk; wynik pokazuje `archiwum.zip > katalog/plik.txt`.
  Zaszyfrowane archiwa: podaj hasło; bez hasła wynik jest policzony i zgłoszony („zaszyfrowane”), a nazwy w ZIP nadal da się szukać.
  Archiwa w archiwach nie są otwierane.
- **Filtry:** rozmiar od/do (`100k`, `5m`), data zmiany od/do (`RRRR-MM-DD`), pliki ukryte, limit wyników.
- **Do panelu:** przycisk „Do panelu” (komenda `findpanel`) pokazuje wyniki jako wirtualny katalog w aktywnym panelu: pliki z różnych
  katalogów w jednej liście, na której działają F3, F5, F6, F8, F2, zaznaczanie, sortowanie i zmiana nazw (Ctrl+F2). Pliki, które znikną
  (np. po przeniesieniu), same znikają z listy. Backspace lub `..` wraca do katalogu sprzed listy. Do listy nie da się kopiować ani zakładać
  w niej katalogów. Wyniki z archiwów nie trafiają na listę.
- **Wyniki:** klik zaznacza, Enter lub dwuklik przechodzi do pliku w aktywnym panelu (w archiwum: do archiwum), F3 otwiera podgląd
  w miejscu trafienia. Esc zatrzymuje szukanie, drugi Esc zamyka okno; wyniki zostają, `Ctrl+F` pokazuje je znowu.

Komendy: `find`, `findset <pole> <wartość>` (pola: `roots` (`here`, `both`, `all` lub ścieżki), `name`, `namergx`, `namecase`, `exclude`,
`text`, `regex`, `case`, `words`, `enc` (`auto|utf8|cp1250|utf16`), `binary`, `archives`, `password`, `hidden`, `min`, `max`, `after`,
`before`, `limit`, `reset`), `findrun`, `findstop`, `findgo <n>`, `findview <n>`, `findpanel`, `findclose`.

## Zmiana nazw wielu plików (Ctrl+F2)

Okno działa na zaznaczonych plikach (bez zaznaczenia: na pliku pod kursorem). Lista pokazuje
**starą i nową nazwę na żywo**, nic nie jest zmieniane przed **Start** (F5); **Cofnij** (Ctrl+Z)
przywraca poprzednie nazwy. Kliknięcie w nową nazwę pozwala wpisać ją ręcznie.

- **Maska nazwy i rozszerzenia:** `[N]` nazwa, `[N2-5]` znaki od-do, `[N3-]` do końca, `[N2,3]` od-ile,
  `[E]` rozszerzenie (też z zakresem), `[C]` licznik, `[C10+5:3]` licznik (start 10, krok 5, 3 cyfry),
  `[Y]` `[M]` `[D]` `[h]` `[m]` `[s]` data i czas modyfikacji (też razem: `[YMD]`, `[hms]`),
  `[P]` nazwa katalogu nadrzędnego, `[[]` i `[]]` to dosłowne nawiasy.
- **Szukaj i zamień:** zwykły tekst albo wyrażenie regularne (RegWy; `$1` = grupa), tylko pierwsze
  trafienie (1x), opcjonalnie razem z rozszerzeniem, bez rozróżniania wielkości liter.
- **Duża/mała:** bez zmian, WIELKIE, małe, Każdy Wyraz Wielką, Pierwsza wielka.
- **Bezpieczeństwo:** konflikty (te same nazwy, istniejący plik, niedozwolona nazwa) są zaznaczone
  na czerwono i blokują Start; istniejące pliki nigdy nie są nadpisywane; zamiana nazw `a <-> b` działa;
  gdy coś się nie uda w trakcie, wszystko wraca do stanu sprzed Start.

Komendy: `multirename`, `mrset <pole> <wartość>` (pola: `name`, `ext`, `find`, `replace`, `regex`,
`once`, `inclext`, `ignorecase`, `case`, `start`, `step`, `digits`, `reset`; wartość w cudzysłowie
zachowuje spacje), `mrname <n> [nazwa]`, `mrrun`, `mrundo`, `mrclose`. Przykład bez okna:
`bluepane-cli run <lewy> <prawy> "selectall" "multirename" "mrset name foto_[C]" "mrset digits 3" "mrrun"`.

## Kopiowanie i przenoszenie

F5/F6 działają w tle: pasek postępu pod panelami pokazuje bajty, bieżący plik i przycisk
**Anuluj** (niepełne pliki są usuwane). Kolejne operacje ustawiają się w kolejce. Gdy plik już
istnieje w celu, okno pyta: **Nadpisz**, **Nadpisz starsze** (tylko gdy nowy plik ma nowszą datę),
**Pomiń**, **Zachowaj oba** (nowy dostaje nazwę `plik (kopia).ext`) albo **Anuluj**. Pole „Dla
wszystkich kolejnych plików” stosuje wybór do reszty operacji. Katalogi istniejące po obu stronach
są scalane. Nadpisywanie nigdy nie następuje bez Twojej decyzji.
