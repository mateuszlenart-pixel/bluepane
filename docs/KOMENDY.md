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
| `go <adres>` | to, co robi pole ścieżki: ścieżka, `sftp://użytkownik@host:port/katalog` (też ftp://, ftps://), `smb://użytkownik@komputer/udział/katalog` (udziały SMB w panelu przez smbclient), `?maska` szuka plików w katalogu, `>komenda` ją uruchamia |
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
| `scanwake <ip\|adres MAC>` | Wake-on-LAN: wysyła pakiet budzący na adres MAC urządzenia (z ostatniego skanu albo z historii skanów; można podać sam adres MAC); odpowiedzi nie ma, więc program tylko potwierdza wysłanie, a komputer musi mieć włączone budzenie przez sieć w BIOS-ie i karcie sieciowej |
| `scanpanel <ip> <nazwa udziału>` | otwiera udział sieciowy w aktywnym panelu: na Windows jako \\komputer\udział, na Linuksie przez zamontowanie `gio mount` (pakiet gvfs); udział wymagający logowania trzeba raz otworzyć w menedżerze plików systemu |
| `scanhistory [off]` | pokazuje historię skanów: znane urządzenia (ostatnio widziane, kiedy pojawiły się pierwszy raz) i dziennik skanów; po zakończonym skanie urządzenia, których wcześniej nie było, mają znacznik NOWE (filtr `nowe` pokazuje tylko je; pierwszy skan zapisuje stan wyjściowy i nic nie oznacza); `off` wraca do wyników |
| `scanfav <ip>` | dodaje lub usuwa urządzenie z ulubionych |
| `scanfavorites [off]` | pokazuje tylko ulubione (off: wszystkie) |
| `scancopy <ip>` | kopiuje adres urządzenia do schowka |

## Wygląd i skróty

| Komenda | Opis |
| --- | --- |
| `theme [nazwa]` | pokazuje dostępne motywy kolorów albo przełącza motyw |
| `bind [<klawisz> [<komenda>\|default]]` | przypisuje klawisz do komendy, pokazuje przypisanie albo przywraca domyślne; zapisuje się w `settings.conf` |
| `unbind <klawisz>` | wyłącza klawisz |
| `schedule [list\|add <makro>; <pora>\|delete <makro>]` | harmonogram makr (alias: sched): system uruchamia makro przez bluepane-cli o zadanej porze (crontab na Linuksie, Harmonogram zadań na Windows); pora to hourly, daily GG:MM albo weekly <pn\|wt\|sr\|cz\|pt\|so\|nd> GG:MM; wpis używa katalogów obu paneli z chwili dodania i pliku makra, więc późniejsza edycja makra zmienia kolejne uruchomienia; program usuwa i wypisuje tylko własne wpisy |
| `panel 1\|2` | robi aktywnym lewy (1) albo prawy (2) panel; przydatne w makrach i skryptach, które ustawiają katalogi obu paneli |
| `syncprofile [list\|save <nazwa>\|run <nazwa>\|delete <nazwa>]` | profile synchronizacji (alias: sprofile): `save` zapisuje oba katalogi i ustawienia otwartego narzędzia sync (wariant, porównywanie, tolerancja, usuwanie, wzorce bez spacji) jako makro `sync-<nazwa>`; `run` ustawia katalogi paneli i porównuje, a plan czeka na ekranie do sprawdzenia (syncrun); z harmonogramu profil działa do końca: `schedule add sync-<nazwa>; daily 03:00`; kierunki ustawione ręcznie dla pojedynczych plików nie są zapisywane |

## Serwery

| Komenda | Opis |
| --- | --- |
| `cloud [<nazwa>]` | chmury przez program rclone (alias: chmura): bez nazwy wypisuje chmury skonfigurowane w rclone (`rclone config`), z nazwą otwiera taką chmurę (Google Drive, OneDrive, S3, WebDAV…) jako katalog w aktywnym panelu i zapisuje ją jako witrynę `<nazwa> (rclone)`; loginów chmur bluepane nie przechowuje, trzyma je rclone; wymaga zainstalowanego rclone; z pola ścieżki: `rclone://nazwa/katalog` |

## Wygląd i skróty

| Komenda | Opis |
| --- | --- |
| `compare [close\|next\|prev]` | porównuje dwa pliki (alias: cmp): dwa zaznaczone w panelu albo plik pod kursorem z plikiem pod kursorem w drugim panelu; tekst obok tekstu z zaznaczonymi różnicami (do 4 MB; różnice w samych końcach wierszy są pomijane), pliki binarne: pierwszy różniący się bajt; `next` i `prev` skaczą między różnicami (F8, F7 w zakładce); czyta pliki dopiero po tej komendzie |
| `checksum create sha256\|md5\|verify` | sumy kontrolne (alias: cksum): `create` zapisuje obok pliku `nazwa.sha256` albo `nazwa.md5` (dla kilku plików `SHA256SUMS` / `MD5SUMS` w katalogu panelu) w formacie `suma  nazwa` i nigdy nie nadpisuje istniejącego pliku; `verify` na pliku .sha256 / .md5 (albo na pliku, który ma taki obok) sprawdza wszystkie wymienione pliki i pokazuje zgodne, zmienione i brakujące; rozumie też format BSD i wiersze z samą sumą; liczy w tle dopiero po komendzie |
| `dupes [close\|stop\|keep newest\|oldest\|shortest\|first\|clear\|mark <grupa> <plik>\|trash]` | duplikaty (alias: dup): pliki aktywnego katalogu i podkatalogów o tej samej treści, w osobnej zakładce; szukanie rusza dopiero po komendzie, działa w tle i da się je przerwać (`stop`): najpierw ten sam rozmiar, potem skrót SHA-256, na końcu porównanie bajt po bajcie; puste pliki i dowiązania są pomijane; `keep` zaznacza wszystkie kopie oprócz jednej wybranej regułą (newest: najnowsza zostaje, oldest: najstarsza, shortest: najkrótsza ścieżka, first: pierwsza alfabetycznie), `mark` przełącza zaznaczenie jednego pliku, `trash` przenosi zaznaczone do kosza, ale nigdy ostatniej kopii grupy |
| `map [close\|into <katalog>\|up\|show]` | mapa zajętości: katalog aktywnego panelu jako kafelki o polu proporcjonalnym do rozmiaru (jak WizTree) w osobnej zakładce; rozmiary katalogów liczą się w tle dopiero po tej komendzie; `into` wchodzi w katalog mapy (klik w kafelek), `up` wraca wyżej, `show` ustawia panel na katalogu mapy, `close` zamyka |
| `button [list\|add <nazwa> = <program argumenty>\|delete <nazwa>\|run <nazwa>]` | własne przyciski (alias: btn): zewnętrzny program z parametrami, zapisany w buttons.conf; w słowie %f to pełna ścieżka pliku pod kursorem, %n jego nazwa, %d katalog tego panelu, %o drugiego, %s zaznaczone pliki (każdy jako osobny argument), %% znak procentu; cudzysłowy zachowują spacje; bez powłoki, więc znaki jak ; w nazwach plików są bezpieczne; przyciski są w osobnym rzędzie pod klawiszami funkcyjnymi i w polu Ctrl+K |
| `macro [list\|record <nazwa>\|stop\|discard\|play <nazwa> [argumenty]\|show <nazwa>\|edit <nazwa>\|delete <nazwa>]` | makra (alias: mac): `record` nagrywa polecenia z wiersza poleceń, skrótów, palety i pola Ctrl+K do pliku .bp w katalogu konfiguracji, `stop` zapisuje, `play` odtwarza (nazwę ze spacjami zakończ średnikiem: `play Moje makro; arg1 arg2`); w poleceniach %1..%9 to argumenty, %d katalog tego panelu, %o drugiego, %f nazwa pod kursorem; `edit` otwiera skrypt w edytorze; kliknięcia myszą w wiersze nie są nagrywane |
| `workspace [list\|save <nazwa>\|load <nazwa>\|delete <nazwa>\|restore on\|off]` | obszary robocze (alias: ws): zakładki obu paneli z sortowaniem i widokiem, aktywny panel, pasek Miejsca i układ okna pod nazwą; `restore` włącza lub wyłącza powrót do poprzedniej sesji przy starcie |
| `layout [list\|reset\|save <nazwa>\|load <nazwa>\|delete <nazwa>]` | układy okna (gdzie stoją panele i narzędzia): lista zapisanych, powrót do dwóch paneli obok siebie, zapis, wczytanie i usunięcie układu o nazwie |
| `columns [ext\|size\|date\|created\|perms\|owner\|sha256\|git\|default]` | kolumny listy obok nazwy: bez argumentu pokazuje, z nazwą kolumny ją pokazuje albo chowa (created = data utworzenia, perms = prawa dostępu, owner = właściciel; na Windows te dwie są puste; sha256 = sumy policzone przez `hash`; git = stan plików z gita), default przywraca; zapisuje się w `settings.conf` |
| `hash [stop\|copy]` | liczy w tle sumy SHA-256 zaznaczonych plików (albo pliku pod kursorem); wynik pokazuje kolumna `sha256`, `hash copy` kopiuje pełne sumy jako wiersze `suma  nazwa`, `hash stop` przerywa; nic nie jest liczone bez tej komendy |
| `gitstatus` | pyta gita jeszcze raz o stan plików katalogu aktywnego panelu (kolumna `git`, która sama pyta raz na katalog) |
| `columnwidth <kolumna> <30-400>\|reset` | szerokość kolumny listy w punktach (tak samo działa przeciągnięcie granicy nagłówka myszą); `reset` przywraca domyślne; zapisuje się w `settings.conf` |
| `appearance [textsize <80-160>\|rowheight <16-40>\|accent <#rrggbb>\|reset]` | rozmiar tekstu w procentach, wysokość wiersza listy w punktach (zastępuje gęstość) i kolor akcentu; `reset` (lub `<ustawienie> reset`) przywraca domyślne; zapisuje się w `settings.conf` |
| `density [compact\|comfortable]` | wiersze listy zwarte albo wygodne; bez argumentu przełącza; zapisuje się w `settings.conf` |
| `keys [ctrl\|alt\|shift]` | ściąga klawiszy: wszystkie skróty (także własne z `bind`) z opisami; z argumentem tylko te z Ctrl, Alt albo Shift (?, albo przytrzymaj Ctrl lub Alt) |

## Paleta komend i „Skocz do”

| Komenda | Opis |
| --- | --- |
| `palette [tekst]\|query <tekst>\|move <n>\|run\|close` | paleta komend: szukanie wśród wszystkich komend z opisem; Enter uruchamia zaznaczoną, a taką, która wymaga argumentu, wpisuje do wiersza poleceń (F1) |
| `box [tekst]\|query <tekst>\|move <n>\|run\|close` | Jedna skrzynka: komendy, miejsca i pliki bieżącego katalogu w jednej liście; „>” zawęża do komend, „?wzorzec” uruchamia głębokie szukanie w katalogu (Ctrl+K) |
| `jump [tekst]\|query <tekst>\|move <n>\|run\|close` | „Skocz do”: katalogi wybierane według częstości i świeżości odwiedzin, zakładki aktywnego panelu i zapisane serwery; Enter przechodzi tam w aktywnym panelu (Ctrl+P) |
