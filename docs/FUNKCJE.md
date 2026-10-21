# Funkcje

Opis tego, co robi program, po obszarach. Skróty: [KLAWISZE.md](KLAWISZE.md); komendy: [KOMENDY.md](KOMENDY.md);
lista kontrolna do testów: [TESTOWANIE.md](TESTOWANIE.md).

## Okno, nawigacja i wygląd

- pasek rozmiaru za kolumną Rozmiar (długość względem największego pliku lub policzonego katalogu) i podsumowanie pod listą: „Zaznaczono: 2 pliki (48.2 KB) z 10 elementów” albo suma katalogu
- **Układ okna jako dok**: oba panele i każde narzędzie (podgląd pliku, strony dokumentu, synchronizacja, szukanie, zmiana nazw, skaner, kolejka, serwery) to zakładki, które można przeciągać, dzielić, wyciągać do osobnego okna i zamykać (×). Otwarte narzędzie dostaje zakładkę po prawej stronie paneli (kilka narzędzi naraz jako zakładki obok siebie); skróty narzędzia działają, gdy jego zakładka ma fokus (ostatnia, w którą kliknięto). Układ paneli zapisuje się w `layout.json` w katalogu konfiguracji; narzędzia przy starcie są zamknięte. **Oba panele są zawsze widoczne**: zakładka upuszczona do grupy panelu wraca obok (drugi panel na prawo od niego, narzędzie do grupy narzędzi). **Nazwane układy**: `layout save <nazwa>` zapisuje bieżący układ (w `layouts/<nazwa>.json`), `layout load <nazwa>` go przywraca, `layout delete <nazwa>` usuwa, `layout` wypisuje zapisane, `layout reset` wraca do dwóch paneli obok siebie. **Quick View w zakładce**: `quick tab` sprawia, że Ctrl+Q otwiera podgląd w osobnej zakładce doku (oba panele zostają; w zakładce strzałki przewijają, ←/→ zmieniają stronę, Esc zamyka), `quick panel` przywraca podgląd w miejscu drugiego panelu; wybór zapisuje się w `settings.conf`
- zmiana nazw (Ctrl+F2): w kolumnie nowych nazw podświetlone tylko dodane znaki, zielony znak = gotowe, ostrzeżenie = konflikt; synchronizacja (Alt+S): zielona strzałka kopiuje w prawo, niebieska w lewo, czerwony kosz usuwa, ostrzeżenie to konflikt
- na dole: pasek bieżącego zadania (nazwa, postęp, bajty, prędkość i czas do końca, Anuluj, przycisk „Kolejka”) oraz dwa rzędy przycisków klawiszy funkcyjnych (klawisz na ciemnej plakietce i nazwa akcji), rozciągnięte na całą szerokość okna
- nad panelem: lista dysków z paskiem zajętości i „wolne z”, ścieżka jako klikalne segmenty (kliknięcie = `cd` do tego miejsca; kliknięcie pustego miejsca paska zamienia ścieżkę w pole do wpisania), a aktywny filtr jako odznaka z ✕
- **szybki filtr** (pisanie nad listą albo `filter <tekst>`): fragmenty nazwy, bez względu na wielkość liter i polskie znaki, oraz kryteria w tym samym tekście, które muszą wszystkie pasować: `typ:obraz` (też `dokument`, `archiwum`, `audio`, `wideo`, `kod`, `tekst`, `program`, `katalog`), `ext:jpg,png`, `>10m`, `<1g` (tylko pliki), `nowsze:7d` / `starsze:30d` (godziny `h`, dni `d`, tygodnie `w`), `katalogi`, `pliki`; np. `raport typ:dokument nowsze:7d`. Nieznane słowo jest szukane w nazwie. Pod listą widać „Widoczne N z M”; ✕ na odznace albo Esc czyści cały filtr jednym ruchem
- ikony i kolory typów plików: katalogi, archiwa, dokumenty, obrazy, audio/wideo, kod, programy (klasa pliku z nazwy, bez otwierania)
- motyw kolorów: komenda `theme` (pokazuje dostępne), `theme dark`, `theme light`, `theme system` (jasny albo ciemny według ustawienia systemu; gdy systemu nie da się zapytać, ciemny); wybór zapisuje się w `settings.conf`
- **kolumny listy**: `columns` pokazuje, które są włączone; `columns ext|size|date|created` pokazuje albo chowa kolumnę (Typ, Rozmiar, Data zmiany, **Utworzono** — data utworzenia pliku tam, gdzie system plików ją zapisuje; na niektórych dyskach Linuksa jej nie ma i kolumna jest pusta), `columns default` przywraca Typ, Rozmiar, Data. Kolejność w wierszu jest stała: Nazwa, Typ, Rozmiar, Utworzono, Data; w wąskim panelu pierwsza znika data utworzenia, potem data, typ, a rozmiar najpóźniej. Po kolumnie Utworzono się nie sortuje
- **gęstość listy**: `density` przełącza wiersze zwarte i wygodne (`density compact`, `density comfortable`); kolumny i gęstość zapisują się w `settings.conf` (`columns = ext size date`, `density = compact`)
- **własne skróty**: `bind ctrl+k sort name` (przypisz), `bind ctrl+k` (pokaż), `bind ctrl+k default` (przywróć domyślny), `unbind f9` (wyłącz), samo `bind` (lista zmian); zapisują się w tym samym `settings.conf` (wiersze `key Ctrl+K = sort name`, `key F9 =` wyłącza klawisz); błędny wiersz jest pomijany z ostrzeżeniem w pasku stanu. Dotyczy skrótów paneli; klawisze okien (podgląd, strony) są na razie stałe

## Paleta komend (F1)

F1 otwiera okno z polem szukania nad wszystkimi komendami. Pisz fragment nazwy albo opisu (polskie litery można pomijać: `sciezka` znajdzie `cd`); wszystkie wpisane słowa muszą pasować. Strzałki ↑↓ zmieniają wybór, Enter uruchamia komendę, Esc zamyka okno, klik w wiersz uruchamia ten wiersz. Wiersz pokazuje składnię i opis; skrót klawiszowy jest w nawiasie na końcu opisu.

Ostatnio uruchomione z palety komendy (pogrubione) są na górze i wygrywają remisy; lista jest zapamiętana między uruchomieniami (plik `recent_commands.txt` w katalogu konfiguracji). Komenda, która wymaga argumentu (np. `cd`, `selmask`), nie uruchamia się od razu: trafia do wiersza poleceń z końcową spacją, żeby dokończyć ją z klawiatury.

Komenda: `palette [tekst]` (otwiera paletę z tekstem), `palette query <tekst>`, `palette move <n>`, `palette run`, `palette close`. Lista pochodzi z tej samej tabeli opisów co [KOMENDY.md](KOMENDY.md), więc nowa komenda pojawia się w palecie sama.

## Ściąga klawiszy

- **`?`** (albo komenda `keys`) otwiera ściągę ze wszystkimi skrótami panelu, pogrupowanymi jak w [KOMENDY.md](KOMENDY.md), z krótkim opisem; widać w niej także klawisze zmienione komendą `bind`. Esc, `?` albo przycisk „Zamknij” ją chowa. Znak `?` nie trafia przy tym do szybkiego filtra.
- **Przytrzymanie samego Ctrl albo samego Alt** przez chwilę (ok. 0,7 s) pokazuje tylko skróty z tym klawiszem; puszczenie klawisza zamyka ściągę. To samo z klawiatury: `keys ctrl`, `keys alt`, `keys shift`.
- **Która zakładka ma klawiaturę**: tytuł zakładki doku, do której trafiają klawisze (aktywny panel albo kliknięte narzędzie), ma ikonę klawiatury i kolor akcentu.

## Pasek akcji

Nad klawiszami funkcyjnymi pojawia się rząd przycisków (ikona błyskawicy) z akcjami, które pasują do tego, co jest pod kursorem albo zaznaczone; gdy nic szczególnego nie jest wskazane, pasek znika:

- **archiwum**: Rozpakuj tutaj, Rozpakuj do podkatalogu (o nazwie archiwum), Rozpakuj do drugiego panelu; wewnątrz otwartego archiwum: rozpakuj całe do drugiego panelu albo obok archiwum,
- **dokument, tekst, kod**: Podgląd (na serwerze także Edytuj na serwerze),
- **inny plik na serwerze**: Edytuj na serwerze, Pobierz do drugiego panelu,
- **kilka zaznaczonych plików**: Spakuj (z liczbą), Zmień nazwy zbiorowo,
- **katalog albo nic, a w panelach dwa różne katalogi na dysku**: Porównaj / synchronizuj katalogi paneli.

Przyciski to te same komendy co klawisze (np. „Rozpakuj tutaj” to `unpack -h`, „do podkatalogu” to `unpack -h -s`); skrót jest podany obok, jeśli jest.

## Zmiana nazwy w wierszu i powiadomienia

**F2** (`rename`) zmienia nazwę w samym wierszu listy: nazwa staje się polem tekstowym, a zaznaczona jest część bez rozszerzenia (dla katalogów cała nazwa), więc wystarczy wpisać nową nazwę, a rozszerzenie zostaje. Enter zatwierdza, Esc anuluje, kliknięcie w inny wiersz też kończy bez zmiany. Gdy wiersz nie jest widoczny (np. przez filtr), program pyta o nazwę w oknie, jak dawniej.

Po zakończeniu zadania (kopiowanie, przenoszenie, usuwanie, pakowanie, synchronizacja, transfer z serwera…) w prawym dolnym rogu okna pojawia się **powiadomienie**: tytuł zadania i jak poszło, z ikoną sukcesu, anulowania albo błędu. Sukces znika po kilku sekundach, **błąd zostaje do zamknięcia** (✕); kliknięcie w tekst powiadomienia otwiera kolejkę zadań. Naraz widać cztery powiadomienia. Zadania pomocnicze (listowanie serwera, logowanie) nie dają powiadomień. Cofania operacji przyciskiem nie ma (zob. decyzje w `ROADMAP.md`).

## Przeciąganie

Zaznaczone pozycje (albo ta, od której zaczynasz przeciąganie) można przeciągnąć myszą:

- **na drugi panel**: kopiowanie, tak jak F5; z przytrzymanym **Shiftem** przenoszenie, tak jak F6 (działa też dla serwerów i archiwów, tak jak F5 / F6),
- **na wiersz katalogu** (w którymkolwiek panelu): do tego katalogu; podświetlenie pokazuje, gdzie wylądują pliki, a przy wskaźniku jest napis „Kopiuj: …” / „Przenieś: …”,
- **Esc** w trakcie przeciągania je anuluje; upuszczenie w tym samym panelu poza katalogiem nic nie robi. Katalogu nie da się upuścić na siebie ani na własny podkatalog, a pliki, które już są w tym katalogu, są pomijane z komunikatem.

Można też **upuścić pliki z menedżera plików systemu** na panel albo na wiersz katalogu: zostaną skopiowane (zawsze kopiowanie, nigdy przeniesienie). Konflikty nazw pyta zwykłe okno, a postęp jest w kolejce zadań.

Komendy: `dragto copy|move <1|2> [wiersz]` (panel 1 = lewy, 2 = prawy) i `drop <1|2> [wiersz] "ścieżka"…`.

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
- **`smb://[użytkownik@]komputer[/udział[/katalog]]`** – udziały sieciowe SMB w panelu, patrz „Udziały SMB” niżej. Bez zainstalowanego `smbclient` otwiera sam udział tak, jak robi to system (UNC na Windows, GVFS na Linuksie); `smb://komputer` pokazuje wtedy udziały komputera (adres IP, jak w skanerze),
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

## Jedna skrzynka (Ctrl+K)

Jedno pole, jedna lista: **komendy** (jak w palecie F1, najwyżej pięć najlepiej pasujących), **miejsca** (jak w „Skocz do”: katalogi z historii, zakładki, serwery) i **pliki bieżącego katalogu** (nazwy zawierające wpisane słowa; polskie litery można pomijać, nazwy nigdy się przez to nie zmieniają). Każdy wiersz ma z przodu swoje źródło (Komenda, Katalog, Zakładka, Serwer, Plik, Folder). Bez wpisanego tekstu pokazuje ostatnie komendy i najczęściej odwiedzane katalogi.

- `>tekst`: tylko komendy (cała lista).
- `?wzorzec`: jedyny wiersz „Szukaj”; Enter uruchamia głębokie szukanie w bieżącym katalogu (okno `find`, można anulować). Nic nie jest skanowane w tle.
- Enter na pliku ustawia na nim kursor, na komendzie z argumentem wpisuje ją do linii poleceń, na miejscu przechodzi tam.
- F1 (paleta) i Ctrl+P („Skocz do”) działają jak dawniej, jako skróty do samych komend i samych miejsc.

Komenda: `box [tekst]`, `box query <tekst>`, `box move <n>`, `box run`, `box close`.

## „Skocz do” (Ctrl+P)

Jedno pole szukania nad miejscami, do których chodzisz najczęściej. Lista składa się z:

- **katalogów** z historii odwiedzin: im częściej i im niedawniej byłeś w katalogu, tym wyżej (ostatnia godzina liczy się czterokrotnie, ostatnia doba podwójnie, ostatni tydzień pojedynczo, starsze za pół); katalogi, których już nie ma, są pomijane; historia (do 500 katalogów) jest zapisana w `visits.conf` w katalogu konfiguracji,
- **zakładek** aktywnego panelu (poza bieżącą),
- **zapisanych serwerów** (Enter łączy aktywny panel).

Wpisane słowa muszą wszystkie pasować do ścieżki (polskie litery można pomijać); katalog, którego nazwa zaczyna się od pierwszego słowa, jest wyżej. Strzałki ↑↓ zmieniają wybór, Enter przechodzi, Esc zamyka, klik w wiersz przechodzi od razu. Odwiedziny liczą się w katalogach lokalnych (nie w archiwach, na serwerach ani na listach wyników szukania).

Komenda: `jump [tekst]`, `jump query <tekst>`, `jump move <n>`, `jump run`, `jump close`. Udziałów sieciowych ze skanera na tej liście jeszcze nie ma.

## Kolumny listy

`columns` pokazuje i chowa kolumny: `ext` (typ), `size`, `date` (zmiana), `created` (utworzenie), `perms` (prawa dostępu, np. `rwxr-xr-x`, także setuid/setgid/sticky) i `owner` (właściciel). Prawa i właściciel istnieją tylko na Linuksie; na Windows te dwie kolumny są puste. `columns default` przywraca zestaw domyślny. W wąskim panelu kolumny znikają od najmniej ważnej (prawa i właściciel pierwsze, rozmiar ostatni).

**Sumy SHA-256 (`hash`):** kolumna `sha256` nic nie liczy sama. `hash` liczy w tle sumy zaznaczonych plików (albo pliku pod kursorem; kolumna pokazuje pierwsze 16 cyfr), `hash stop` przerywa, `hash copy` kopiuje pełne sumy jako wiersze `suma  nazwa` (jak `sha256sum`). Suma jest zapominana, gdy zmieni się rozmiar albo data pliku.

**Status gita (`git`):** po włączeniu kolumny program raz na katalog pyta o stan programu `git` (w tle; gdy git nie jest zainstalowany albo katalog nie jest repozytorium, kolumna jest pusta). Litery: `M` zmieniony, `A` dodany, `D` usunięty, `R` przemianowany, `?` nowy, `U` konflikt; katalog pokazuje najpoważniejszą zmianę w środku. `gitstatus` pyta ponownie o katalog aktywnego panelu.

**Szerokość:** przeciągnij myszą lewą granicę nagłówka kolumny (kursor zmienia się w strzałki); dwuklik na granicy przywraca domyślną szerokość. To samo robi komenda `columnwidth <kolumna> <30-400>|reset` (`columnwidth reset`: wszystkie). Szerokości zapisują się w `settings.conf` (`width size = 120`).

## Wygląd

`appearance` (bez argumentów pokazuje stan) ustawia, zapisując w `settings.conf`:

- `appearance textsize 80..160`: rozmiar tekstu w procentach (czcionki całego okna, także okien dialogowych),
- `appearance rowheight 16..40`: wysokość wiersza listy w punktach (zastępuje `density`; `rowheight reset` wraca do gęstości),
- `appearance accent #rrggbb`: kolor akcentu: ramka kursora, obwódki, linki i pierścień fokusu klawiatury (dwa punkty grubości),
- `appearance reset` (albo `textsize reset`, `rowheight reset`, `accent reset`) przywraca domyślne.

Zmiana działa od razu. Motywy: `theme dark|light|system`.

## Samoodświeżanie list

Gdy inny program zmieni katalog pokazany w panelu (pobieranie, kompilacja, zapis pliku), lista odświeża się sama, a kursor zostaje na swoim pliku i zaznaczenie się nie gubi. Program prosi system o powiadomienia (inotify na Linuksie, ReadDirectoryChangesW na Windows, crate `notify`) i **patrzy tylko na katalogi dwóch aktywnych paneli**, bez podkatalogów; archiwa, serwery i listy wyników nie są obserwowane. Serię zmian czyta raz, po ćwierć sekundy ciszy. Nic nie jest skanowane w tle.

- `watch off` wyłącza (zapisuje `watch = off` w `settings.conf`), `watch on` włącza z powrotem, `watch` pokazuje stan. Ręcznie odświeża F9 (`refresh`).
- Jeśli system nie daje powiadomień (np. wyczerpany limit obserwowanych katalogów inotify), `watch` mówi o tym, a F9 nadal działa.
- Okno, gdy nic się nie dzieje, budzi się cztery razy na sekundę, żeby odebrać powiadomienia; `watch off` to wyłącza.

## Obszary robocze i przywracanie sesji

Obszar roboczy to zapamiętany układ pracy: katalogi i zakładki obu paneli (z sortowaniem i widokiem lista/siatka), aktywny panel, wybrane miejsca i układ okna (zapisany jako układ „obszar <nazwa>”). Pliki leżą w `workspaces/<nazwa>.conf` w katalogu konfiguracji; format ma numer wersji, a nowszej wersji niż znana program odmawia wczytać zamiast ją zepsuć.

- `workspace save <nazwa>`: zapisuje obszar; `workspace load <nazwa>`: przywraca; `workspace delete <nazwa>`; `workspace list` (wymienia obszary). Skrót: `ws`.
- Przy przywracaniu katalogi, których już nie ma, są pomijane, a pasek stanu mówi, ile ich było.
- **Sesja:** przy zamknięciu okna zapisuje się ostatni stan jako ukryty obszar, a przy starcie wraca (chyba że podasz katalogi w wierszu poleceń). Wyłączenie: `workspace restore off` (zapisuje `restore = off` w `settings.conf`); `workspace restore on` włącza z powrotem.

## Makra i skrypty

Makro to zapisany ciąg poleceń. `macro record Porządki` zaczyna nagrywanie (w pasku stanu świeci „● Nagrywam”), potem pracujesz jak zwykle, a `macro stop` zapisuje plik `macros/Porządki.bp` w katalogu konfiguracji. Nagrywają się polecenia z wiersza poleceń, skrótów klawiszy, palety i pola Ctrl+K; **kliknięcia myszą w wiersze listy nie są nagrywane** (zamiast nich użyj `cursor`, `select`, klawiszy). Nie nagrywają się same polecenia `macro`, `box`, `palette`, `jump`, `keys` i `help`.

- `macro play <nazwa> [argumenty]` odtwarza makro i staje na pierwszym poleceniu, które się nie powiedzie. Nazwę ze spacjami zakończ średnikiem: `macro play Moje makro; arg1 arg2`.
- `macro list`, `macro show <nazwa>` (pokazuje polecenia), `macro edit <nazwa>` (otwiera skrypt w edytorze), `macro delete <nazwa>`, `macro discard` (porzuca trwające nagrywanie). Skrót: `mac`.
- Skrypt to zwykły tekst: jedno polecenie w wierszu, puste wiersze i wiersze zaczynające się od `#` są pomijane. Można go napisać ręcznie.
- Parametry w wierszu: `%1`..`%9` to argumenty z `macro play`, `%d` katalog aktywnego panelu, `%o` katalog drugiego panelu, `%f` nazwa pod kursorem, `%%` znak procentu. Wartości liczą się tuż przed wykonaniem danego wiersza.
- Makro może wywołać inne makro (do 4 poziomów, żeby pętla nie zawiesiła programu).
- Bez okna: `bluepane-cli run-script <lewy> <prawy> <skrypt.bp> [argumenty]`. Nie ma okna konfliktów (istniejący plik jest pomijany), a `delete` wymaga w skrypcie następnej linii `confirm`.

## Własne przyciski narzędzi

Własny przycisk uruchamia zewnętrzny program z parametrami. Zastępuje wtyczki, bo nie wczytuje żadnego kodu: tylko startuje program. Przyciski leżą w `buttons.conf` w katalogu konfiguracji (jeden w wierszu: `Nazwa = program argumenty`) i mają osobny rząd pod klawiszami funkcyjnymi; widać je też w polu Ctrl+K (rodzaj „Przycisk”, wystarczy zacząć pisać nazwę, także bez polskich liter).

- `button add Edytor = code %f` dodaje (albo zastępuje) przycisk; `button list`, `button delete <nazwa>`, `button run <nazwa>`. Skrót: `btn`. Plik `buttons.conf` można też edytować ręcznie.
- Parametry: `%f` pełna ścieżka pliku pod kursorem, `%n` sama nazwa, `%d` katalog aktywnego panelu, `%o` katalog drugiego panelu, `%s` zaznaczone pliki (albo plik pod kursorem), każdy jako osobny argument, gdy `%s` jest osobnym słowem; `%%` to znak procentu. Cudzysłowy zachowują spacje w jednym słowie: `foto "C:\Program Files\x" %s`.
- Program startuje w katalogu aktywnego panelu, bez powłoki: nazwy plików ze spacjami, średnikami czy `&` trafiają do programu bez zmian i nic się nie interpretuje. Przycisk działa na katalogu na dysku (nie w archiwum ani na serwerze).
- Błąd uruchomienia (np. brak programu) pokazuje pasek stanu z nazwą programu.

## Harmonogram makr

Makro (zob. „Makra i skrypty”) może się uruchamiać samo o zadanej porze, nawet gdy bluepane jest zamknięte. Zadanie zakłada system: wpis w `crontab` użytkownika na Linuksie albo zadanie w folderze `bluepane` Harmonogramu zadań na Windows; wykonuje je `bluepane-cli run-script` (musi leżeć obok programu).

- `schedule add Porządki; daily 07:30` zakłada zadanie. Pory: `hourly` (co godzinę), `daily GG:MM`, `weekly <dzień> GG:MM` (dni: `pn wt sr cz pt so nd` albo `mon tue wed thu fri sat sun`). Nazwę makra od pory oddziela średnik. Dodanie makra o tej samej nazwie zastępuje poprzedni wpis.
- `schedule list` pokazuje zaplanowane makra, `schedule delete <makro>` usuwa wpis. Program rusza tylko własne wpisy (w cronie oznaczone `# bluepane:<nazwa>`), reszty harmonogramu nie czyta ani nie zmienia.
- Zadanie używa katalogów, które pokazywały oba panele w chwili dodania, oraz pliku makra, więc późniejsza edycja makra zmienia następne uruchomienia. Skrypt bez okna nie ma okna konfliktów: istniejący plik jest pomijany, a `delete` wymaga linii `confirm`.
- Skrót: `sched`. Profile synchronizacji w harmonogramie dojdą z etapem 14h.

## Mapa zajętości

`map` pokazuje katalog aktywnego panelu jako kafelki o polu proporcjonalnym do rozmiaru (jak WizTree), w osobnej zakładce „Mapa zajętości”. Największe elementy są w lewym górnym rogu; katalogi mają kolor, pliki szary. Rozmiary katalogów liczą się **w tle i tylko po tej komendzie** (nic nie skanuje się samo); kafelki pojawiają się w miarę napływu wyników, a pasek nad mapą pokazuje sumę i liczbę katalogów, które jeszcze się liczą.

- Klik w kafelek katalogu wchodzi do niego (liczenie zaczyna się od nowa); `Backspace` albo „W górę” wraca wyżej; „Pokaż w panelu” (`map show`) ustawia panel na katalogu mapy; `Esc` albo „Zamknij” zamyka. Najechanie na kafelek pokazuje nazwę i rozmiar.
- Komendy: `map`, `map into <katalog>`, `map up`, `map show`, `map close`.
- Rysuje się do 300 największych wpisów; liczba pominiętych jest pod mapą. Puste katalogi nie mają kafelka. Dowiązania nie są śledzone, a katalog bez uprawnień liczy się jako pusty.
- Mapa działa tylko na katalogu na dysku (nie w archiwum ani na serwerze).

## Profile synchronizacji

Profil to para katalogów z ustawieniami narzędzia `sync` (wariant, sposób porównywania, tolerancja, usuwanie, wzorce „Tylko” i „Pomiń”), zapisana pod nazwą. Leży jako zwykłe makro `sync-<nazwa>` w katalogu makr, więc działa wszędzie tam, gdzie makra: `macro play`, `bluepane-cli run-script` i harmonogram.

- `syncprofile save <nazwa>` zapisuje to, co jest w otwartej zakładce synchronizacji (przycisk „Zapisz profil”; ustawienia zmienione, ale jeszcze nie zastosowane przyciskiem „Porównaj”, blokują zapis). Ten sam profil pod tą samą nazwą jest aktualizowany.
- `syncprofile run <nazwa>` (przycisk „Wczytaj: nazwa”) ustawia oba panele na zapisane katalogi i porównuje je; **plan czeka na ekranie** do sprawdzenia i dopiero „Synchronizuj” (`syncrun`) coś kopiuje albo usuwa.
- `syncprofile list`, `syncprofile delete <nazwa>`.
- Z harmonogramu profil działa **do końca**, bo plik kończy się poleceniem `syncrun`: `schedule add sync-<nazwa>; daily 03:00`. Usuwanie plików idzie tak, jak zapisano w profilu (domyślnie do kosza); profil z usuwaniem „Trwale” wykonuje je bez pytania, więc zapisuj go świadomie. Wpis w harmonogramie nie znika sam po usunięciu profilu: `schedule delete sync-<nazwa>`.
- Nie są zapisywane kierunki ustawione ręcznie dla pojedynczych plików ani własne reguły kierunków. Wzorce i katalog wersji nie mogą zawierać spacji (zapis odmówi z komunikatem).
- Do skryptów doszła komenda `panel 1|2`, która wybiera aktywny panel.

## Duplikaty

`dupes` szuka plików o tej samej treści w katalogu aktywnego panelu i jego podkatalogach; wynik jest w zakładce „Duplikaty”. Szukanie rusza **dopiero po komendzie**, działa w tle i można je przerwać („Przerwij”, `dupes stop`; znalezione grupy zostają). Kandydaci zawężają się trzema krokami: ten sam rozmiar, ten sam skrót SHA-256, na końcu porównanie bajt po bajcie (więc kolizja skrótu nie zaszkodzi). Puste pliki i dowiązania są pomijane.

- Grupy są posortowane od największej do odzyskania przestrzeni. Każda kopia ma pole wyboru (`dupes mark <grupa> <plik>`).
- Reguły: „Zostaw najnowszą” (`dupes keep newest`), „Zostaw najstarszą” (`oldest`), „Zostaw najkrótszą ścieżkę” (`shortest`), `first` (pierwsza alfabetycznie). Reguła zaznacza wszystkie kopie oprócz jednej w każdej grupie; „Odznacz” (`dupes clear`) czyści zaznaczenie.
- „Do kosza” (`dupes trash`) przenosi zaznaczone kopie do kosza. Jeśli w jakiejś grupie zaznaczono wszystkie kopie, komenda odmawia: jedna musi zostać.
- Działa tylko na katalogu na dysku (nie w archiwum ani na serwerze).

## Porównanie plików i sumy kontrolne

`compare` (alias `cmp`) porównuje dwa pliki w zakładce „Porównanie”: dwa zaznaczone w panelu albo plik pod kursorem z plikiem pod kursorem w drugim panelu. Pliki są czytane **dopiero po komendzie**, w tle.

- **Tekst** (do 4 MB) leży obok siebie: linie zmienione są żółte, tylko po lewej czerwone, tylko po prawej zielone. `F8` / „Następna różnica” i `F7` / „Poprzednia różnica” skaczą między blokami różnic. Linie są porównywane bez końców wierszy, więc pliki różniące się tylko CRLF/LF albo końcowym znakiem nowej linii są uznane za identyczne. Gdy środek (po odcięciu wspólnego początku i końca) jest bardzo duży i różny, pokazuje się jako jeden zmieniony blok.
- **Binarne** (z bajtem zerowym) i pliki powyżej 4 MB: komunikat o pierwszym różniącym się bajcie (offset dziesiętny i szesnastkowy) z kontekstem w hex albo informacja, że krótszy plik jest początkiem dłuższego.

`checksum` (alias `cksum`):
- `checksum create sha256` / `md5` zapisuje obok pliku `nazwa.sha256` albo `nazwa.md5` (dla kilku zaznaczonych plików `SHA256SUMS` / `MD5SUMS` w katalogu panelu) w formacie `suma  nazwa`. Istniejącego pliku nigdy nie nadpisuje.
- `checksum verify` na pliku `.sha256` / `.md5` (albo na pliku, który ma taki obok) sprawdza wszystkie wymienione pliki i pokazuje: zgodny, ZMIENIONY, brak pliku, nie do odczytu. Rozumie też format BSD (`SHA256 (nazwa) = suma`), znacznik trybu binarnego `*`, komentarze `#` i wiersze z samą sumą (dotyczą pliku, od którego pochodzi nazwa pliku z sumą).
- Działa tylko na plikach na dysku. Do MD5 doszła zależność `md-5` (RustCrypto, ta sama rodzina co `sha2`).

## Podgląd plików (F3)

Podgląd czyta tylko widoczne okno pliku, więc plik wielogigabajtowy otwiera się od razu i nie
zajmuje pamięci. Pliki z bajtami zerowymi otwierają się w trybie hex.

**Widok miniatur** (Ctrl+Shift+G, komenda `viewmode [list|grid]`) zamienia listę aktywnego panelu
w siatkę kafelków: obrazy mają miniaturę (liczoną w tle tylko dla widocznych kafelków,
zapamiętywaną do ok. 600 obrazów; przy zmianie pliku liczona od nowa), pozostałe pliki i katalogi
ikonę. Kliknięcia, zaznaczanie, Enter, menu kontekstowe i przeciąganie działają jak na liście;
strzałki w lewo / w prawo przesuwają o kafelek, w górę / w dół o cały rząd. Na serwerach i w
archiwach kafelki mają tylko ikony (miniatura wymagałaby pobrania pliku).

**Obrazy** (PNG, JPEG, GIF, BMP, WebP, ICO) F3 pokazuje jako obraz w zakładce „Strony”, z tym samym
powiększaniem co strony PDF (`pagezoom fit|in|out|<procent>`; 100% = piksel w piksel). To samo
działa w Quick View (Ctrl+Q), dla obrazu w archiwum i na serwerze. Plik z rozszerzeniem obrazu,
który obrazem nie jest, otwiera się jako tekst / hex. Obrazy większe niż 200 MB albo 100 mln
pikseli nie są otwierane; z GIF-a widać pierwszą klatkę.

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
- **Udział w panelu:** „W panelu” przy udziale ustawia aktywny panel na tym udziale (`scanpanel <ip> <udział>`). Windows: ścieżka `\\adres\udział`. Linux: udział jest montowany przez `gio mount` (pakiet `gvfs`) i panel pokazuje jego katalog w `/run/user/.../gvfs`. Udział, który wymaga logowania, trzeba raz otworzyć w menedżerze plików systemu; okno podaje wtedy przyczynę zamiast pokazywać pusty panel.
- **Wake-on-LAN:** „Obudź” (`scanwake <ip albo adres MAC>`) wysyła pakiet budzący (UDP, rozgłoszenie na port 9) na adres MAC urządzenia. MAC pochodzi z ostatniego skanu albo z historii, więc obudzisz też komputer, który jest teraz wyłączony. Odpowiedzi nie ma, więc program potwierdza tylko wysłanie; komputer musi mieć włączone budzenie przez sieć w BIOS-ie i w karcie sieciowej. Działa w sieci lokalnej.
- **Witryna jednym kliknięciem:** „Witryna SFTP” i „Witryna FTP” przy zaznaczonym urządzeniu zapisują je jako serwer w menedżerze (Ctrl+S), niezależnie od tego, czy skan znalazł otwarty port 22 albo 21.
- **Historia skanów:** po każdym **zakończonym** skanie program zapamiętuje urządzenia (po adresie MAC, a gdy go nie ma, po adresie IP) i krótki dziennik skanów w plikach `scan_known.conf` i `scan_log.conf`. Zakładka „Historia” pokazuje znane urządzenia (od ostatnio widzianych) z datą pierwszego i ostatniego razu oraz ostatnie skany. Urządzenie, którego wcześniej nie było, ma w wynikach znacznik **NOWE**; filtr `nowe` pokazuje tylko takie. **Pierwszy skan** zapisuje stan wyjściowy i niczego nie oznacza. Skan zatrzymany przed końcem nie jest zapamiętywany (widział tylko część sieci). Nic nie jest skanowane w tle.
- **Ulubione:** zakładka „Ulubione” trzyma wybrane urządzenia między uruchomieniami (plik `scan_favorites.conf` w katalogu konfiguracji) i pokazuje, które były widoczne w ostatnim skanie.
- **Filtr** zawęża tabelę po nazwie, adresie, producencie, MACu i usługach. Wyniki zostają po zamknięciu okna (`Ctrl+N` pokazuje je znowu). Esc zatrzymuje skanowanie.
- Na Windows tablica ARP jest czytana poleceniem `arp -a`, na Linuksie z `/proc/net/arp`.

Komendy: `scan`, `scanset <pole> <wartość>` (pola: `range`, `ports`, `names`, `arp`, `filter`, `timeout`, `reset`), `scanrun`, `scanstop`, `scanclose`, `scanselect <ip>`, `scanopen <ip> <port>`, `scanshares <ip>`, `scanshare <ip> <udział>`, `scansite <ip> sftp|ftp|ftps`, `scanfav <ip>`, `scanfavorites [off]`, `scancopy <ip>`, `scanwake <ip|MAC>`, `scanpanel <ip> <udział>`, `scanhistory [off]`.

## Chmury przez rclone

Google Drive, OneDrive, S3, WebDAV i reszta chmur, które zna program **rclone**, mogą być katalogiem w panelu, jak serwer. bluepane nie mówi tymi protokołami i **nie trzyma loginów chmur**: konfiguruje je rclone (`rclone config`), a program tylko prosi go o listę, kopiowanie i usuwanie. Wymaga zainstalowanego rclone (rclone.org); bez niego komunikat mówi, co zrobić.

- `cloud` wypisuje chmury skonfigurowane w rclone (nazwa i rodzaj). `cloud <nazwa>` (alias `chmura`) otwiera chmurę w aktywnym panelu i zapisuje ją jako witrynę `<nazwa> (rclone)` (menedżer serwerów, Ctrl+S, ma protokół „Chmura (rclone)”: w polu serwera jest nazwa chmury, o hasło nie pyta). Z pola ścieżki: `rclone://nazwa/katalog`.
- Panel działa jak przy SFTP: Enter / Backspace, F5 pobiera i wysyła (także całe katalogi), F7 nowy katalog, F2 zmiana nazwy (nigdy nie zastępuje istniejącego), F8 usuwa **trwale** (kosz chmury jest sprawą rclone i samej chmury; pytanie mówi to wprost). Pobieranie idzie przez plik `.bpart`, więc przerwane nie zostawia pół pliku pod prawdziwą nazwą; postęp pobierania rośnie razem z plikiem, wysyłanie zalicza się na końcu.
- Czego nie ma: uprawnień (`chmod`), wznawiania przerwanego pobierania i śledzenia postępu wysyłania w trakcie (to wymagałoby parsowania wyjścia rclone). Chmury bez znanego rozmiaru plików (np. dokumenty Google) pokazują 0 B.
- Prawdziwy rclone nie jest uruchamiany w testach: sprawdzają one komendy `lsjson`, `mkdir`, `deletefile`, `rmdir`, `moveto`, `copyto` na atrapie, więc pierwszy prawdziwy test na koncie chmury jest Twój.

## Udziały SMB

Udział Windows / Samby jest katalogiem w panelu **bez montowania w systemie**: bluepane prosi o listę, kopiowanie i usuwanie program **smbclient** (Linux: pakiet `smbclient`; Windows: `smb://` otwiera wtedy udział przez ścieżkę UNC `\\komputer\udział`, o ile smbclient nie jest zainstalowany).

- `go smb://ala@nas/dane/Zdjęcia` łączy się jako `ala` (pyta o hasło), zapisuje witrynę `ala@nas (smb)` (menedżer serwerów, Ctrl+S, ma protokół „Udziały SMB”; puste pole użytkownika to logowanie jako gość) i otwiera katalog `Zdjęcia` w udziale `dane`. Katalog główny serwera (`cd /`) to **lista udziałów** (bez ukrytych `C$`, `IPC$` i drukarek); udział jest katalogiem, jak zwykły.
- Panel działa jak przy SFTP: Enter / Backspace, F5 pobiera i wysyła (także całe katalogi), F7 nowy katalog, F2 zmiana nazwy (nie zastępuje istniejącego; przenosić można w obrębie jednego udziału), F8 usuwa **trwale** (udział nie ma kosza; pytanie mówi to wprost). Pobieranie i wysyłanie idą przez plik `.bpart`, który po dokończeniu dostaje prawdziwą nazwę; postęp pobierania rośnie razem z plikiem, wysyłanie zalicza się na końcu.
- Hasło idzie do smbclienta zmienną środowiskową `PASSWD`, nie w wierszu poleceń, więc nie widać go na liście procesów.
- Czego nie ma: uprawnień (`chmod`), wznawiania przerwanego pobierania, nazw zawierających `"` albo `;` (smbclient dzieli skrypt po średnikach; takie nazwy dostają komunikat), przenoszenia między udziałami jednym ruchem (kopiuj i usuń). Daty plików są czasem serwera zinterpretowanym jako lokalny.
- Prawdziwy serwer SMB nie jest używany w testach: sprawdzają one polecenia `-L`, `ls`, `mkdir`, `del`, `rmdir`, `rename`, `get`, `put` na atrapie, więc pierwszy prawdziwy test na udziale jest Twój.

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

- **Nazwa:** maski (`*.rs;*.md`, `?` = jeden znak) albo wyrażenie regularne; z rozróżnianiem wielkości liter lub bez. Maski domyślnie **nie rozróżniają polskich znaków**: `zolw*` znajduje „żółw.txt”, a `żółw*` także „zolw.txt” (nazwy plików się nie zmieniają). Opcja „polskie znaki” (`findset nameaccents on`) włącza dokładne dopasowanie; wyrażenia regularne i szukanie w treści zawsze są dosłowne.
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
- **Dane z wnętrza pliku:** `[EXIF:YMD]` to data **zrobienia zdjęcia** z EXIF (też `[EXIF:hms]`, `[EXIF:YMDhms]`, jak przy dacie modyfikacji), `[EXIF:Make]` i `[EXIF:Model]` producent i model aparatu; `[TAG:artist]`, `[TAG:title]`, `[TAG:album]`, `[TAG:genre]`, `[TAG:track]` (dwie cyfry: `03`) i `[TAG:year]` to tagi pliku audio (MP3, FLAC, OGG, M4A, WAV i inne). Pliki są otwierane **tylko wtedy, gdy maska używa takiego pola**, i każdy raz na okno. Plik bez takich danych daje pusty fragment, więc przy kilku takich plikach podgląd pokaże konflikt zamiast po cichu nadpisać; znaki niedozwolone w nazwie (`/ \ : * ? " < > |`) w tagach zamieniają się na `_`. Przykłady: `[EXIF:YMD]_[C]`, `[TAG:track] [TAG:artist] - [TAG:title]`. Dochodzą dwie zależności: `kamadak-exif` i `lofty` (obie w czystym Rust).
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
