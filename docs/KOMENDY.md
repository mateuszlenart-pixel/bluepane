# Komendy

Ta lista jest **generowana z kodu** (`crates/engine/src/command_help.rs`); test pilnuje,
żeby była aktualna. Każdą komendę można wpisać w wierszu poleceń na dole okna, użyć w
`bluepane-cli run` i przypisać do klawisza (`bind`). W nawiasach: skrót i alias.

## Panele i nawigacja

| Komenda | Opis |
| --- | --- |
| `switch` | przełącza aktywny panel (Tab) |
| `up` | wychodzi do katalogu nadrzędnego (Backspace) |
| `enter` | wchodzi do katalogu, archiwum albo serwera pod kursorem (Enter) |
| `cd <ścieżka>` | przechodzi do katalogu; obsługuje też ścieżki serwera i wnętrza archiwum |
| `go <adres>` | to, co robi pole ścieżki: ścieżka, `sftp://użytkownik@host:port/katalog` (też ftp://, ftps://), `smb://komputer/udział`, `?maska` szuka plików w katalogu, `>komenda` ją uruchamia |
| `back` | wraca do poprzedniego katalogu z historii panelu (Alt+←) |
| `forward` | idzie naprzód w historii katalogów panelu (Alt+→) |
| `volume <numer\|nazwa>` | przechodzi na dysk (alias: drive); numery jak w liście dysków (Alt+1…9) |
| `refresh` | wczytuje listę katalogu od nowa i czyści policzone rozmiary katalogów (F9) |
| `tab new\|close [n]\|next\|prev\|open\|<n>` | zakładki katalogów w panelu: nowa, zamknij, następna, poprzednia, katalog pod kursorem w nowej, przejdź do numeru |
| `cursor <n>` | przesuwa kursor o n wierszy (ujemne: w górę) |
| `click <wiersz> [ctrl\|shift\|ctrlshift]` | klik myszą w wiersz: przesuwa kursor, z modyfikatorem zaznacza (jak w Windows) |
| `select <nazwa>` | ustawia kursor na pozycji o tej nazwie i zaznacza ją |
| `filter [tekst]` | zawęża listę: fragmenty nazwy (bez względu na wielkość liter i polskie znaki) oraz kryteria `typ:obraz\|dokument\|archiwum\|audio\|wideo\|kod\|tekst\|program`, `ext:jpg,png`, `>10m`, `<1g`, `nowsze:7d`, `starsze:30d`, `katalogi`, `pliki`; bez tekstu czyści filtr (Esc) |
| `dirsize` | liczy w tle rozmiar zaznaczonych katalogów albo tego pod kursorem (Alt+Shift+Enter) |
| `viewmode [list\|grid]` | aktywny panel jako lista albo siatka miniatur (obrazy z podglądem, reszta z ikoną); bez argumentu przełącza (Ctrl+Shift+G) |
| `places` | pokazuje lub chowa pasek „Miejsca”: dyski z wolnym miejscem, ulubione, serwery i ostatnie katalogi (Ctrl+B) |
| `fav [ścieżka]\|del <ścieżka>` | dodaje katalog (domyślnie bieżący) do ulubionych albo go z nich usuwa; `fav del` usuwa po ścieżce |

## Zaznaczanie i sortowanie

| Komenda | Opis |
| --- | --- |
| `toggle` | zaznacza lub odznacza pozycję pod kursorem i schodzi niżej (Spacja, Insert) |
| `selmask <maska>` | zaznacza pliki pasujące do maski, np. `*.txt;*.md` (Ctrl+M) |
| `unselmask <maska>` | odznacza pliki pasujące do maski (Ctrl+Shift+M) |
| `selectall` | zaznacza wszystko (Ctrl+A) |
| `unselectall` | odznacza wszystko (Ctrl+Shift+A) |
| `invert` | odwraca zaznaczenie (Ctrl+I) |
| `sort name\|ext\|date\|size [asc\|desc]` | sortuje listę; ten sam klucz drugi raz odwraca kierunek (Ctrl+F3…F6) |
| `copynames stem\|file\|path` | kopiuje do schowka nazwy zaznaczonych: bez rozszerzeń, z rozszerzeniami albo pełne ścieżki (Ctrl+Shift+1/2/3) |

## Operacje na plikach

| Komenda | Opis |
| --- | --- |
| `copy` | kopiuje zaznaczone do drugiego panelu (alias: cp; F5) |
| `move` | przenosi zaznaczone do drugiego panelu (alias: mv; F6) |
| `mkdir [nazwa]` | tworzy katalog; bez nazwy pyta o nią (F7) |
| `rename [nazwa]` | zmienia nazwę pozycji pod kursorem; bez nazwy pyta o nią (F2) |
| `open` | otwiera plik pod kursorem w domyślnym programie systemu (menu kontekstowe; F4 używa edytora z `BLUEPANE_EDITOR`) |
| `info` | pokazuje w pasku stanu, czym jest pozycja pod kursorem (rodzaj, rozmiar, data, ścieżka) albo podsumowanie zaznaczonych |
| `menu` | otwiera menu kontekstowe pozycji pod kursorem (Shift+F10; myszą: prawy przycisk) |
| `dragto copy\|move <1\|2> [wiersz]` | przeciąganie: zaznaczone z aktywnego panelu do panelu 1 (lewy) lub 2 (prawy), z numerem wiersza do katalogu w tym panelu; tak działa przeciągnięcie myszą (z Shiftem przenosi) |
| `drop <1\|2> [wiersz] "ścieżka"…` | kopiuje pliki spoza programu (upuszczone z menedżera plików systemu) do panelu 1 lub 2, z numerem wiersza do katalogu w nim |
| `delete` | przenosi zaznaczone do kosza po potwierdzeniu; na serwerze usuwa trwale (alias: rm; F8) |
| `chmod <ósemkowo>` | ustawia uprawnienia zaznaczonych na serwerze, np. `chmod 755` |
| `confirm` | potwierdza pytanie (alias: yes; Enter) |
| `cancel` | odrzuca pytanie (alias: no; Esc); przy konflikcie przerywa zadanie |
| `stop` | przerywa bieżące zadanie i usuwa oczekujące |

## Archiwa

| Komenda | Opis |
| --- | --- |
| `pack [opcje]` | pakuje zaznaczone do archiwum w drugim panelu (zip, 7z, tar.gz; okno ustawień bez opcji; Alt+F5) |
| `unpack [opcje]` | rozpakowuje archiwum pod kursorem albo otwarte do drugiego panelu; `-h` obok archiwum, `-s` do podkatalogu o nazwie archiwum (Alt+F9) |
| `testarchive [opcje]` | sprawdza archiwum bez rozpakowywania |

## Podgląd i dokumenty

| Komenda | Opis |
| --- | --- |
| `view` | podgląd pliku pod kursorem: tekst/hex, strony PDF i dokumentów (alias: f3; F3) |
| `edit` | otwiera plik w zewnętrznym programie albo w `BLUEPANE_EDITOR` (F4) |
| `viewer scroll\|page\|hscroll <n>\|top\|bottom\|mode\|find [tekst]\|next\|prev\|close` | sterowanie podglądem pliku (okno F3) |
| `page next\|prev\|first\|last\|<numer>` | zmienia stronę w podglądzie stron dokumentu |
| `pagezoom in\|out\|fit\|<procent>` | powiększenie strony dokumentu (dopasuj do szerokości: fit) |
| `pageclose` | zamyka podgląd stron |
| `quick [scroll <n>\|page <1\|-1>\|tab\|panel]` | Quick View: podgląd pliku pod kursorem; bez argumentów włącza i wyłącza (Ctrl+Q); `tab` otwiera go w osobnej zakładce, `panel` w miejscu drugiego panelu (zapisuje się w `settings.conf`) |
| `editsend` | wysyła na serwer plik edytowany przez F4 (Ctrl+Shift+F4) |

## Kolejka zadań

| Komenda | Opis |
| --- | --- |
| `queue` | otwiera okno kolejki zadań (Ctrl+J) |
| `queueclose` | zamyka okno kolejki |
| `queuepause` | wstrzymuje kolejkę: bieżące zadanie się dokończy, następne czekają |
| `queueresume` | wznawia kolejkę |
| `queuedel <numer>` | usuwa oczekujące zadanie z kolejki |
| `queuemove <numer> up\|down\|top` | zmienia kolejność oczekujących zadań |
| `queueretry <numer>` | ustawia zakończone zadanie w kolejce jeszcze raz (1 = najnowsze) |
| `queueclear` | czyści historię zakończonych zadań |
| `limit [<prędkość>\|off]` | ogranicza prędkość kopiowania i transferów, np. `limit 2m`; bez argumentu pokazuje stan |

## Synchronizacja

| Komenda | Opis |
| --- | --- |
| `sync [-twoway\|-mirror\|-update] [-time\|-size\|-content] [-tol=<s>] [-i<wzorce>] [-x<wzorce>] [-delete=trash\|permanent\|versions:<katalog>]` | porównuje katalogi obu paneli i pokazuje, co zrobiłaby synchronizacja (Alt+S) |
| `syncrun` | wykonuje pokazany plan synchronizacji (F5 w oknie) |
| `syncset <numer> left\|right\|skip` | zmienia kierunek jednej pozycji planu |
| `syncclose` | zamyka okno synchronizacji |

## Szukanie

| Komenda | Opis |
| --- | --- |
| `find` | otwiera okno szukania plików (Ctrl+F, Alt+F7) |
| `findset <pole> <wartość>` | ustawia kryterium szukania (roots, name, namergx, namecase, nameaccents, exclude, text, …); `nameaccents off` (domyślnie) znajduje nazwy bez względu na polskie znaki |
| `findrun` | uruchamia szukanie |
| `findstop` | zatrzymuje szukanie |
| `findgo <numer>` | przechodzi do wyniku (do katalogu lub archiwum z plikiem) |
| `findview <numer>` | podgląd pliku z wyniku |
| `findpanel` | pokazuje wyniki jako listę w panelu (wirtualny katalog) |
| `findclose` | zamyka okno szukania (wyniki zostają w panelu) |

## Zmiana nazw

| Komenda | Opis |
| --- | --- |
| `multirename` | zmiana nazw wielu plików z podglądem nowych nazw (alias: mr; Ctrl+F2) |
| `mrset <pole> <wartość>` | ustawia pole narzędzia: name, ext, find, replace, regex, case, start, step, digits, reset … |
| `mrname <numer> [nazwa]` | ręcznie ustawia nową nazwę jednej pozycji; bez nazwy wraca do wyliczonej |
| `mrrun` | wykonuje zmianę nazw (Start) |
| `mrundo` | cofa ostatnią zmianę nazw (Ctrl+Z w oknie) |
| `mrclose` | zamyka narzędzie zmiany nazw |

## Serwery

| Komenda | Opis |
| --- | --- |
| `sites` | otwiera menedżer serwerów SFTP, FTP i FTPS (Ctrl+S) |
| `siteset <nazwa> <pole> <wartość>` | ustawia pole witryny: protocol, host, port, user, key, start, mode, secret |
| `sitedel <nazwa>` | usuwa zapisaną witrynę |
| `siteclose` | zamyka menedżer serwerów |
| `sitepassword <nazwa> <hasło>` | zapisuje hasło witryny w jej magazynie (system albo Bitwarden) |
| `connect <witryna>` | łączy aktywny panel z serwerem |
| `disconnect` | rozłącza aktywny panel z serwerem |

## Skaner sieci

| Komenda | Opis |
| --- | --- |
| `scan` | otwiera skaner sieci (Ctrl+N) |
| `scanset <pole> <wartość>` | ustawia skaner: range, ports, names, arp, filter, timeout, reset |
| `scanrun` | uruchamia skanowanie zakresu adresów |
| `scanstop` | zatrzymuje skanowanie |
| `scanclose` | zamyka skaner |
| `scanselect <ip>` | zaznacza urządzenie na liście |
| `scanopen <ip> <port>` | otwiera usługę urządzenia (przeglądarka, SSH, pulpit zdalny, …) |
| `scanshares <ip>` | pokazuje udziały sieciowe urządzenia |
| `scanshare <ip> <udział>` | otwiera udział sieciowy |
| `scansite <ip> sftp\|ftp\|ftps` | zapisuje urządzenie jako witrynę serwera |
| `scanfav <ip>` | dodaje lub usuwa urządzenie z ulubionych |
| `scanfavorites [off]` | pokazuje tylko ulubione (off: wszystkie) |
| `scancopy <ip>` | kopiuje adres urządzenia do schowka |

## Wygląd i skróty

| Komenda | Opis |
| --- | --- |
| `theme [nazwa]` | pokazuje dostępne motywy kolorów albo przełącza motyw |
| `bind [<klawisz> [<komenda>\|default]]` | przypisuje klawisz do komendy, pokazuje przypisanie albo przywraca domyślne; zapisuje się w `settings.conf` |
| `unbind <klawisz>` | wyłącza klawisz |
| `workspace [list\|save <nazwa>\|load <nazwa>\|delete <nazwa>\|restore on\|off]` | obszary robocze (alias: ws): zakładki obu paneli z sortowaniem i widokiem, aktywny panel, pasek Miejsca i układ okna pod nazwą; `restore` włącza lub wyłącza powrót do poprzedniej sesji przy starcie |
| `layout [list\|reset\|save <nazwa>\|load <nazwa>\|delete <nazwa>]` | układy okna (gdzie stoją panele i narzędzia): lista zapisanych, powrót do dwóch paneli obok siebie, zapis, wczytanie i usunięcie układu o nazwie |
| `columns [ext\|size\|date\|created\|default]` | kolumny listy obok nazwy: bez argumentu pokazuje, z nazwą kolumny ją pokazuje albo chowa (created = data utworzenia), default przywraca; zapisuje się w `settings.conf` |
| `density [compact\|comfortable]` | wiersze listy zwarte albo wygodne; bez argumentu przełącza; zapisuje się w `settings.conf` |
| `keys [ctrl\|alt\|shift]` | ściąga klawiszy: wszystkie skróty (także własne z `bind`) z opisami; z argumentem tylko te z Ctrl, Alt albo Shift (?, albo przytrzymaj Ctrl lub Alt) |

## Paleta komend i „Skocz do”

| Komenda | Opis |
| --- | --- |
| `palette [tekst]\|query <tekst>\|move <n>\|run\|close` | paleta komend: szukanie wśród wszystkich komend z opisem; Enter uruchamia zaznaczoną, a taką, która wymaga argumentu, wpisuje do wiersza poleceń (F1) |
| `box [tekst]\|query <tekst>\|move <n>\|run\|close` | Jedna skrzynka: komendy, miejsca i pliki bieżącego katalogu w jednej liście; „>” zawęża do komend, „?wzorzec” uruchamia głębokie szukanie w katalogu (Ctrl+K) |
| `jump [tekst]\|query <tekst>\|move <n>\|run\|close` | „Skocz do”: katalogi wybierane według częstości i świeżości odwiedzin, zakładki aktywnego panelu i zapisane serwery; Enter przechodzi tam w aktywnym panelu (Ctrl+P) |
