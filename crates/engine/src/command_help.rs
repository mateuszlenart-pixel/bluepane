//! One-line help for every command: the source of `bluepane-cli commands`, of the generated
//! `docs/KOMENDY.md` and (later) of the command palette. A test keeps this table, the list of
//! command names and the parser in step, so a new command cannot be added without its help.

/// What one command does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandHelp {
    /// The heading it is listed under in the docs.
    pub group: &'static str,
    pub name: &'static str,
    /// How to type it, e.g. `sort name|ext|date|size [asc|desc]`.
    pub usage: &'static str,
    pub description: &'static str,
}

const fn h(
    group: &'static str,
    name: &'static str,
    usage: &'static str,
    description: &'static str,
) -> CommandHelp {
    CommandHelp {
        group,
        name,
        usage,
        description,
    }
}

const NAV: &str = "Panele i nawigacja";
const SEL: &str = "Zaznaczanie i sortowanie";
const OPS: &str = "Operacje na plikach";
const ARC: &str = "Archiwa";
const VIEW: &str = "Podgląd i dokumenty";
const QUEUE: &str = "Kolejka zadań";
const SYNC: &str = "Synchronizacja";
const FIND: &str = "Szukanie";
const REN: &str = "Zmiana nazw";
const SERVERS: &str = "Serwery";
const SCAN: &str = "Skaner sieci";
const LOOK: &str = "Wygląd i skróty";
const PAL: &str = "Paleta komend i „Skocz do”";

/// Every command, in the order of the docs.
pub const COMMAND_HELP: &[CommandHelp] = &[
    h(NAV, "switch", "switch", "przełącza aktywny panel (Tab)"),
    h(NAV, "up", "up", "wychodzi do katalogu nadrzędnego (Backspace)"),
    h(NAV, "enter", "enter", "wchodzi do katalogu, archiwum albo serwera pod kursorem (Enter)"),
    h(NAV, "cd", "cd <ścieżka>", "przechodzi do katalogu; obsługuje też ścieżki serwera i wnętrza archiwum"),
    h(NAV, "go", "go <adres>", "to, co robi pole ścieżki: ścieżka, `sftp://użytkownik@host:port/katalog` (też ftp://, ftps://), `smb://użytkownik@komputer/udział/katalog` (udziały SMB w panelu przez smbclient), `?maska` szuka plików w katalogu, `>komenda` ją uruchamia"),
    h(NAV, "back", "back", "wraca do poprzedniego katalogu z historii panelu (Alt+←)"),
    h(NAV, "forward", "forward", "idzie naprzód w historii katalogów panelu (Alt+→)"),
    h(NAV, "volume", "volume <numer|nazwa>", "przechodzi na dysk (alias: drive); numery jak w liście dysków (Alt+1…9)"),
    h(NAV, "refresh", "refresh", "wczytuje listę katalogu od nowa i czyści policzone rozmiary katalogów (F9)"),
    h(NAV, "tab", "tab new|close [n]|next|prev|open|<n>", "zakładki katalogów w panelu: nowa, zamknij, następna, poprzednia, katalog pod kursorem w nowej, przejdź do numeru"),
    h(NAV, "cursor", "cursor <n>", "przesuwa kursor o n wierszy (ujemne: w górę)"),
    h(NAV, "click", "click <wiersz> [ctrl|shift|ctrlshift]", "klik myszą w wiersz: przesuwa kursor, z modyfikatorem zaznacza (jak w Windows)"),
    h(NAV, "select", "select <nazwa>", "ustawia kursor na pozycji o tej nazwie i zaznacza ją"),
    h(NAV, "filter", "filter [tekst]", "zawęża listę: fragmenty nazwy (bez względu na wielkość liter i polskie znaki) oraz kryteria `typ:obraz|dokument|archiwum|audio|wideo|kod|tekst|program`, `ext:jpg,png`, `>10m`, `<1g`, `nowsze:7d`, `starsze:30d`, `katalogi`, `pliki`; bez tekstu czyści filtr (Esc)"),
    h(NAV, "dirsize", "dirsize", "liczy w tle rozmiar zaznaczonych katalogów albo tego pod kursorem (Alt+Shift+Enter)"),
    h(NAV, "viewmode", "viewmode [list|grid]", "aktywny panel jako lista albo siatka miniatur (obrazy z podglądem, reszta z ikoną); bez argumentu przełącza (Ctrl+Shift+G)"),
    h(NAV, "places", "places", "pokazuje lub chowa pasek „Miejsca”: dyski z wolnym miejscem, ulubione, serwery i ostatnie katalogi (Ctrl+B)"),
    h(NAV, "fav", "fav [ścieżka]|del <ścieżka>", "dodaje katalog (domyślnie bieżący) do ulubionych albo go z nich usuwa; `fav del` usuwa po ścieżce"),
    h(SEL, "toggle", "toggle", "zaznacza lub odznacza pozycję pod kursorem i schodzi niżej (Spacja, Insert)"),
    h(SEL, "selmask", "selmask <maska>", "zaznacza pliki pasujące do maski, np. `*.txt;*.md` (Ctrl+M)"),
    h(SEL, "unselmask", "unselmask <maska>", "odznacza pliki pasujące do maski (Ctrl+Shift+M)"),
    h(SEL, "selectall", "selectall", "zaznacza wszystko (Ctrl+A)"),
    h(SEL, "unselectall", "unselectall", "odznacza wszystko (Ctrl+Shift+A)"),
    h(SEL, "invert", "invert", "odwraca zaznaczenie (Ctrl+I)"),
    h(SEL, "sort", "sort name|ext|date|size [asc|desc]", "sortuje listę; ten sam klucz drugi raz odwraca kierunek (Ctrl+F3…F6)"),
    h(SEL, "copynames", "copynames stem|file|path", "kopiuje do schowka nazwy zaznaczonych: bez rozszerzeń, z rozszerzeniami albo pełne ścieżki (Ctrl+Shift+1/2/3)"),
    h(OPS, "copy", "copy", "kopiuje zaznaczone do drugiego panelu (alias: cp; F5)"),
    h(OPS, "move", "move", "przenosi zaznaczone do drugiego panelu (alias: mv; F6)"),
    h(OPS, "mkdir", "mkdir [nazwa]", "tworzy katalog; bez nazwy pyta o nią (F7)"),
    h(OPS, "rename", "rename [nazwa]", "zmienia nazwę pozycji pod kursorem; bez nazwy pyta o nią (F2)"),
    h(OPS, "open", "open", "otwiera plik pod kursorem w domyślnym programie systemu (menu kontekstowe; F4 używa edytora z `BLUEPANE_EDITOR`)"),
    h(OPS, "info", "info", "pokazuje w pasku stanu, czym jest pozycja pod kursorem (rodzaj, rozmiar, data, ścieżka) albo podsumowanie zaznaczonych"),
    h(OPS, "menu", "menu", "otwiera menu kontekstowe pozycji pod kursorem (Shift+F10; myszą: prawy przycisk)"),
    h(OPS, "dragto", "dragto copy|move <1|2> [wiersz]", "przeciąganie: zaznaczone z aktywnego panelu do panelu 1 (lewy) lub 2 (prawy), z numerem wiersza do katalogu w tym panelu; tak działa przeciągnięcie myszą (z Shiftem przenosi)"),
    h(OPS, "drop", "drop <1|2> [wiersz] \"ścieżka\"…", "kopiuje pliki spoza programu (upuszczone z menedżera plików systemu) do panelu 1 lub 2, z numerem wiersza do katalogu w nim"),
    h(OPS, "delete", "delete", "przenosi zaznaczone do kosza po potwierdzeniu; na serwerze usuwa trwale (alias: rm; F8)"),
    h(OPS, "chmod", "chmod <ósemkowo>", "ustawia uprawnienia zaznaczonych na serwerze, np. `chmod 755`"),
    h(OPS, "confirm", "confirm", "potwierdza pytanie (alias: yes; Enter)"),
    h(OPS, "cancel", "cancel", "odrzuca pytanie (alias: no; Esc); przy konflikcie przerywa zadanie"),
    h(OPS, "stop", "stop", "przerywa bieżące zadanie i usuwa oczekujące"),
    h(ARC, "pack", "pack [opcje]", "pakuje zaznaczone do archiwum w drugim panelu (zip, 7z, tar.gz; okno ustawień bez opcji; Alt+F5)"),
    h(ARC, "unpack", "unpack [opcje]", "rozpakowuje archiwum pod kursorem albo otwarte do drugiego panelu; `-h` obok archiwum, `-s` do podkatalogu o nazwie archiwum (Alt+F9)"),
    h(ARC, "testarchive", "testarchive [opcje]", "sprawdza archiwum bez rozpakowywania"),
    h(VIEW, "view", "view", "podgląd pliku pod kursorem: tekst/hex, strony PDF i dokumentów (alias: f3; F3)"),
    h(VIEW, "edit", "edit", "otwiera plik w zewnętrznym programie albo w `BLUEPANE_EDITOR` (F4)"),
    h(VIEW, "viewer", "viewer scroll|page|hscroll <n>|top|bottom|mode|find [tekst]|next|prev|close", "sterowanie podglądem pliku (okno F3)"),
    h(VIEW, "page", "page next|prev|first|last|<numer>", "zmienia stronę w podglądzie stron dokumentu"),
    h(VIEW, "pagezoom", "pagezoom in|out|fit|<procent>", "powiększenie strony dokumentu (dopasuj do szerokości: fit)"),
    h(VIEW, "pageclose", "pageclose", "zamyka podgląd stron"),
    h(VIEW, "quick", "quick [scroll <n>|page <1|-1>|tab|panel]", "Quick View: podgląd pliku pod kursorem; bez argumentów włącza i wyłącza (Ctrl+Q); `tab` otwiera go w osobnej zakładce, `panel` w miejscu drugiego panelu (zapisuje się w `settings.conf`)"),
    h(VIEW, "editsend", "editsend", "wysyła na serwer plik edytowany przez F4 (Ctrl+Shift+F4)"),
    h(QUEUE, "queue", "queue", "otwiera okno kolejki zadań (Ctrl+J)"),
    h(QUEUE, "queueclose", "queueclose", "zamyka okno kolejki"),
    h(QUEUE, "queuepause", "queuepause", "wstrzymuje kolejkę: bieżące zadanie się dokończy, następne czekają"),
    h(QUEUE, "queueresume", "queueresume", "wznawia kolejkę"),
    h(QUEUE, "queuedel", "queuedel <numer>", "usuwa oczekujące zadanie z kolejki"),
    h(QUEUE, "queuemove", "queuemove <numer> up|down|top", "zmienia kolejność oczekujących zadań"),
    h(QUEUE, "queueretry", "queueretry <numer>", "ustawia zakończone zadanie w kolejce jeszcze raz (1 = najnowsze)"),
    h(QUEUE, "queueclear", "queueclear", "czyści historię zakończonych zadań"),
    h(VIEW, "details", "details", "pokazuje w podglądzie wszystko, co da się odczytać o pliku: rozmiar, daty, prawa oraz dla zdjęć pola EXIF (aparat, ekspozycja, data, GPS), a dla dźwięku czas trwania, przepływność i tagi (wykonawca, tytuł, album); w podglądzie klawisz I (alias: meta)"),
    h(FIND, "index", "index [status|add <katalog>|remove <katalog>|update|stop|clear|find <słowa>|save <nazwa> [słowa]|open <nazwa>|list|forget <nazwa>]", "indeks do natychmiastowego szukania (alias: idx): `add` (albo `add here`) dodaje katalog i buduje indeks w tle (dopiero na Twoje polecenie), `find` szuka po nazwach i treści plików (też PDF, Word, Excel) bez polskich znaków i po początku słowa, wyniki trafiają do panelu jako lista; `name:`, `text:` i `ext:` zawężają, `save`/`open` zapisują wyszukiwanie pod nazwą jako wirtualny katalog; `update` czyta tylko zmienione pliki"),
    h(NAV, "watch", "watch [on|off]", "listy w panelach odświeżają się same, gdy inny program zmieni katalog na dysku (domyślnie włączone; patrzy tylko na katalogi obu paneli, nie na archiwa ani serwery); bez argumentu pokazuje stan"),
    h(OPS, "terminal", "terminal", "otwiera terminal systemu w katalogu aktywnego panelu (Linux: zmienna TERMINAL albo pierwszy z x-terminal-emulator, gnome-terminal, konsole, xfce4-terminal, alacritty, kitty, xterm; Windows: Windows Terminal albo zwykła konsola); tylko dla katalogów na dysku"),
    h(OPS, "run", "run <program argumenty>  (albo !program argumenty)", "uruchamia program w katalogu aktywnego panelu, bez powłoki, w tle, i pokazuje jego wyjście w podglądzie; w słowach działają `%f` (ścieżka pod kursorem), `%n` (nazwa), `%d` i `%o` (katalogi paneli) oraz `%s` (zaznaczone pliki jako osobne argumenty), jak we własnych przyciskach; pipe'y i przekierowania nie działają (nie ma powłoki), wywołaj wtedy `run sh -c ...` albo `run powershell -Command ...`"),
    h(OPS, "runstop", "runstop", "przerywa program uruchomiony przez `run`"),
    h(QUEUE, "limit", "limit [<prędkość>|off]", "ogranicza prędkość kopiowania i transferów, np. `limit 2m`; bez argumentu pokazuje stan"),
    h(SYNC, "sync", "sync [-twoway|-mirror|-update] [-time|-size|-content] [-tol=<s>] [-i<wzorce>] [-x<wzorce>] [-delete=trash|permanent|versions:<katalog>]", "porównuje katalogi obu paneli i pokazuje, co zrobiłaby synchronizacja (Alt+S)"),
    h(SYNC, "syncrun", "syncrun", "wykonuje pokazany plan synchronizacji (F5 w oknie)"),
    h(SYNC, "syncset", "syncset <numer> left|right|skip", "zmienia kierunek jednej pozycji planu"),
    h(SYNC, "syncclose", "syncclose", "zamyka okno synchronizacji"),
    h(FIND, "find", "find", "otwiera okno szukania plików (Ctrl+F, Alt+F7)"),
    h(FIND, "findset", "findset <pole> <wartość>", "ustawia kryterium szukania (roots, name, namergx, namecase, nameaccents, exclude, text, …); `nameaccents off` (domyślnie) znajduje nazwy bez względu na polskie znaki"),
    h(FIND, "findrun", "findrun", "uruchamia szukanie"),
    h(FIND, "findstop", "findstop", "zatrzymuje szukanie"),
    h(FIND, "findgo", "findgo <numer>", "przechodzi do wyniku (do katalogu lub archiwum z plikiem)"),
    h(FIND, "findview", "findview <numer>", "podgląd pliku z wyniku"),
    h(FIND, "findpanel", "findpanel", "pokazuje wyniki jako listę w panelu (wirtualny katalog)"),
    h(FIND, "findclose", "findclose", "zamyka okno szukania (wyniki zostają w panelu)"),
    h(REN, "multirename", "multirename", "zmiana nazw wielu plików z podglądem nowych nazw (alias: mr; Ctrl+F2)"),
    h(REN, "mrset", "mrset <pole> <wartość>", "ustawia pole narzędzia: name, ext, find, replace, regex, case, start, step, digits, reset …"),
    h(REN, "mrname", "mrname <numer> [nazwa]", "ręcznie ustawia nową nazwę jednej pozycji; bez nazwy wraca do wyliczonej"),
    h(REN, "mrrun", "mrrun", "wykonuje zmianę nazw (Start)"),
    h(REN, "mrundo", "mrundo", "cofa ostatnią zmianę nazw (Ctrl+Z w oknie)"),
    h(REN, "mrclose", "mrclose", "zamyka narzędzie zmiany nazw"),
    h(SERVERS, "sites", "sites", "otwiera menedżer serwerów SFTP, FTP i FTPS (Ctrl+S)"),
    h(SERVERS, "siteset", "siteset <nazwa> <pole> <wartość>", "ustawia pole witryny: protocol, host, port, user, key, start, mode, secret"),
    h(SERVERS, "sitedel", "sitedel <nazwa>", "usuwa zapisaną witrynę"),
    h(SERVERS, "siteclose", "siteclose", "zamyka menedżer serwerów"),
    h(SERVERS, "sitepassword", "sitepassword <nazwa> <hasło>", "zapisuje hasło witryny w jej magazynie (system albo Bitwarden)"),
    h(SERVERS, "connect", "connect <witryna>", "łączy aktywny panel z serwerem"),
    h(SERVERS, "disconnect", "disconnect", "rozłącza aktywny panel z serwerem"),
    h(SCAN, "scan", "scan", "otwiera skaner sieci (Ctrl+N)"),
    h(SCAN, "scanset", "scanset <pole> <wartość>", "ustawia skaner: range, ports, names, arp, filter, timeout, reset"),
    h(SCAN, "scanrun", "scanrun", "uruchamia skanowanie zakresu adresów"),
    h(SCAN, "scanstop", "scanstop", "zatrzymuje skanowanie"),
    h(SCAN, "scanclose", "scanclose", "zamyka skaner"),
    h(SCAN, "scanselect", "scanselect <ip>", "zaznacza urządzenie na liście"),
    h(SCAN, "scanopen", "scanopen <ip> <port>", "otwiera usługę urządzenia (przeglądarka, SSH, pulpit zdalny, …)"),
    h(SCAN, "scanshares", "scanshares <ip>", "pokazuje udziały sieciowe urządzenia"),
    h(SCAN, "scanshare", "scanshare <ip> <udział>", "otwiera udział sieciowy"),
    h(SCAN, "scansite", "scansite <ip> sftp|ftp|ftps", "zapisuje urządzenie jako witrynę serwera"),
    h(SCAN, "scanwake", "scanwake <ip|adres MAC>", "Wake-on-LAN: wysyła pakiet budzący na adres MAC urządzenia (z ostatniego skanu albo z historii skanów; można podać sam adres MAC); odpowiedzi nie ma, więc program tylko potwierdza wysłanie, a komputer musi mieć włączone budzenie przez sieć w BIOS-ie i karcie sieciowej"),
    h(SCAN, "scanpanel", "scanpanel <ip> <nazwa udziału>", "otwiera udział sieciowy w aktywnym panelu: na Windows jako \\\\komputer\\udział, na Linuksie przez zamontowanie `gio mount` (pakiet gvfs); udział wymagający logowania trzeba raz otworzyć w menedżerze plików systemu"),
    h(SCAN, "scanhistory", "scanhistory [off]", "pokazuje historię skanów: znane urządzenia (ostatnio widziane, kiedy pojawiły się pierwszy raz) i dziennik skanów; po zakończonym skanie urządzenia, których wcześniej nie było, mają znacznik NOWE (filtr `nowe` pokazuje tylko je; pierwszy skan zapisuje stan wyjściowy i nic nie oznacza); `off` wraca do wyników"),
    h(SCAN, "scanfav", "scanfav <ip>", "dodaje lub usuwa urządzenie z ulubionych"),
    h(SCAN, "scanfavorites", "scanfavorites [off]", "pokazuje tylko ulubione (off: wszystkie)"),
    h(SCAN, "scancopy", "scancopy <ip>", "kopiuje adres urządzenia do schowka"),
    h(LOOK, "theme", "theme [nazwa]", "pokazuje dostępne motywy kolorów albo przełącza motyw"),
    h(LOOK, "bind", "bind [<klawisz> [<komenda>|default]]", "przypisuje klawisz do komendy, pokazuje przypisanie albo przywraca domyślne; zapisuje się w `settings.conf`"),
    h(LOOK, "unbind", "unbind <klawisz>", "wyłącza klawisz"),
    h(LOOK, "schedule", "schedule [list|add <makro>; <pora>|delete <makro>]", "harmonogram makr (alias: sched): system uruchamia makro przez bluepane-cli o zadanej porze (crontab na Linuksie, Harmonogram zadań na Windows); pora to hourly, daily GG:MM albo weekly <pn|wt|sr|cz|pt|so|nd> GG:MM; wpis używa katalogów obu paneli z chwili dodania i pliku makra, więc późniejsza edycja makra zmienia kolejne uruchomienia; program usuwa i wypisuje tylko własne wpisy"),
    h(LOOK, "panel", "panel 1|2", "robi aktywnym lewy (1) albo prawy (2) panel; przydatne w makrach i skryptach, które ustawiają katalogi obu paneli"),
    h(LOOK, "syncprofile", "syncprofile [list|save <nazwa>|run <nazwa>|delete <nazwa>]", "profile synchronizacji (alias: sprofile): `save` zapisuje oba katalogi i ustawienia otwartego narzędzia sync (wariant, porównywanie, tolerancja, usuwanie, wzorce bez spacji) jako makro `sync-<nazwa>`; `run` ustawia katalogi paneli i porównuje, a plan czeka na ekranie do sprawdzenia (syncrun); z harmonogramu profil działa do końca: `schedule add sync-<nazwa>; daily 03:00`; kierunki ustawione ręcznie dla pojedynczych plików nie są zapisywane"),
    h(SERVERS, "cloud", "cloud [<nazwa>]", "chmury przez program rclone (alias: chmura): bez nazwy wypisuje chmury skonfigurowane w rclone (`rclone config`), z nazwą otwiera taką chmurę (Google Drive, OneDrive, S3, WebDAV…) jako katalog w aktywnym panelu i zapisuje ją jako witrynę `<nazwa> (rclone)`; loginów chmur bluepane nie przechowuje, trzyma je rclone; wymaga zainstalowanego rclone; z pola ścieżki: `rclone://nazwa/katalog`"),
    h(LOOK, "compare", "compare [close|next|prev]", "porównuje dwa pliki (alias: cmp): dwa zaznaczone w panelu albo plik pod kursorem z plikiem pod kursorem w drugim panelu; tekst obok tekstu z zaznaczonymi różnicami (do 4 MB; różnice w samych końcach wierszy są pomijane), pliki binarne: pierwszy różniący się bajt; `next` i `prev` skaczą między różnicami (F8, F7 w zakładce); czyta pliki dopiero po tej komendzie"),
    h(LOOK, "checksum", "checksum create sha256|md5|verify", "sumy kontrolne (alias: cksum): `create` zapisuje obok pliku `nazwa.sha256` albo `nazwa.md5` (dla kilku plików `SHA256SUMS` / `MD5SUMS` w katalogu panelu) w formacie `suma  nazwa` i nigdy nie nadpisuje istniejącego pliku; `verify` na pliku .sha256 / .md5 (albo na pliku, który ma taki obok) sprawdza wszystkie wymienione pliki i pokazuje zgodne, zmienione i brakujące; rozumie też format BSD i wiersze z samą sumą; liczy w tle dopiero po komendzie"),
    h(LOOK, "dupes", "dupes [close|stop|keep newest|oldest|shortest|first|clear|mark <grupa> <plik>|trash]", "duplikaty (alias: dup): pliki aktywnego katalogu i podkatalogów o tej samej treści, w osobnej zakładce; szukanie rusza dopiero po komendzie, działa w tle i da się je przerwać (`stop`): najpierw ten sam rozmiar, potem skrót SHA-256, na końcu porównanie bajt po bajcie; puste pliki i dowiązania są pomijane; `keep` zaznacza wszystkie kopie oprócz jednej wybranej regułą (newest: najnowsza zostaje, oldest: najstarsza, shortest: najkrótsza ścieżka, first: pierwsza alfabetycznie), `mark` przełącza zaznaczenie jednego pliku, `trash` przenosi zaznaczone do kosza, ale nigdy ostatniej kopii grupy"),
    h(LOOK, "map", "map [close|into <katalog>|up|show]", "mapa zajętości: katalog aktywnego panelu jako kafelki o polu proporcjonalnym do rozmiaru (jak WizTree) w osobnej zakładce; rozmiary katalogów liczą się w tle dopiero po tej komendzie; `into` wchodzi w katalog mapy (klik w kafelek), `up` wraca wyżej, `show` ustawia panel na katalogu mapy, `close` zamyka"),
    h(LOOK, "button", "button [list|add <nazwa> = <program argumenty>|delete <nazwa>|run <nazwa>]", "własne przyciski (alias: btn): zewnętrzny program z parametrami, zapisany w buttons.conf; w słowie %f to pełna ścieżka pliku pod kursorem, %n jego nazwa, %d katalog tego panelu, %o drugiego, %s zaznaczone pliki (każdy jako osobny argument), %% znak procentu; cudzysłowy zachowują spacje; bez powłoki, więc znaki jak ; w nazwach plików są bezpieczne; przyciski są w osobnym rzędzie pod klawiszami funkcyjnymi i w polu Ctrl+K"),
    h(LOOK, "macro", "macro [list|record <nazwa>|stop|discard|play <nazwa> [argumenty]|show <nazwa>|edit <nazwa>|delete <nazwa>]", "makra (alias: mac): `record` nagrywa polecenia z wiersza poleceń, skrótów, palety i pola Ctrl+K do pliku .bp w katalogu konfiguracji, `stop` zapisuje, `play` odtwarza (nazwę ze spacjami zakończ średnikiem: `play Moje makro; arg1 arg2`); w poleceniach %1..%9 to argumenty, %d katalog tego panelu, %o drugiego, %f nazwa pod kursorem; `edit` otwiera skrypt w edytorze; kliknięcia myszą w wiersze nie są nagrywane"),
    h(LOOK, "workspace", "workspace [list|save <nazwa>|load <nazwa>|delete <nazwa>|restore on|off]", "obszary robocze (alias: ws): zakładki obu paneli z sortowaniem i widokiem, aktywny panel, pasek Miejsca i układ okna pod nazwą; `restore` włącza lub wyłącza powrót do poprzedniej sesji przy starcie"),
    h(LOOK, "layout", "layout [list|reset|save <nazwa>|load <nazwa>|delete <nazwa>]", "układy okna (gdzie stoją panele i narzędzia): lista zapisanych, powrót do dwóch paneli obok siebie, zapis, wczytanie i usunięcie układu o nazwie"),
    h(LOOK, "columns", "columns [ext|size|date|created|perms|owner|sha256|git|default]", "kolumny listy obok nazwy: bez argumentu pokazuje, z nazwą kolumny ją pokazuje albo chowa (created = data utworzenia, perms = prawa dostępu, owner = właściciel; na Windows te dwie są puste; sha256 = sumy policzone przez `hash`; git = stan plików z gita), default przywraca; zapisuje się w `settings.conf`"),
    h(LOOK, "hash", "hash [stop|copy]", "liczy w tle sumy SHA-256 zaznaczonych plików (albo pliku pod kursorem); wynik pokazuje kolumna `sha256`, `hash copy` kopiuje pełne sumy jako wiersze `suma  nazwa`, `hash stop` przerywa; nic nie jest liczone bez tej komendy"),
    h(OPS, "git", "git add|unstage|commit <opis>|diff|log", "podstawowe akcje gita na zaznaczonych plikach (albo pliku pod kursorem) w katalogu aktywnego panelu: `add` dodaje do poczekalni, `unstage` zdejmuje, `commit` zatwierdza to, co jest w poczekalni, `diff` i `log` pokazują zmiany i historię w podglądzie; nic nie jest wysyłane ani pobierane z sieci. Gałąź pokazuje się przy ścieżce, gdy kolumna `git` jest włączona"),
    h(LOOK, "gitstatus", "gitstatus", "pyta gita jeszcze raz o stan plików katalogu aktywnego panelu (kolumna `git`, która sama pyta raz na katalog)"),
    h(LOOK, "columnwidth", "columnwidth <kolumna> <30-400>|reset", "szerokość kolumny listy w punktach (tak samo działa przeciągnięcie granicy nagłówka myszą); `reset` przywraca domyślne; zapisuje się w `settings.conf`"),
    h(LOOK, "appearance", "appearance [textsize <80-160>|rowheight <16-40>|accent <#rrggbb>|reset]", "rozmiar tekstu w procentach, wysokość wiersza listy w punktach (zastępuje gęstość) i kolor akcentu; `reset` (lub `<ustawienie> reset`) przywraca domyślne; zapisuje się w `settings.conf`"),
    h(LOOK, "density", "density [compact|comfortable]", "wiersze listy zwarte albo wygodne; bez argumentu przełącza; zapisuje się w `settings.conf`"),
    h(LOOK, "keys", "keys [ctrl|alt|shift]", "ściąga klawiszy: wszystkie skróty (także własne z `bind`) z opisami; z argumentem tylko te z Ctrl, Alt albo Shift (?, albo przytrzymaj Ctrl lub Alt)"),
    h(PAL, "palette", "palette [tekst]|query <tekst>|move <n>|run|close", "paleta komend: szukanie wśród wszystkich komend z opisem; Enter uruchamia zaznaczoną, a taką, która wymaga argumentu, wpisuje do wiersza poleceń (F1)"),
    h(PAL, "box", "box [tekst]|query <tekst>|move <n>|run|close", "Jedna skrzynka: komendy, miejsca i pliki bieżącego katalogu w jednej liście; „>” zawęża do komend, „?wzorzec” uruchamia głębokie szukanie w katalogu (Ctrl+K)"),
    h(PAL, "jump", "jump [tekst]|query <tekst>|move <n>|run|close", "„Skocz do”: katalogi wybierane według częstości i świeżości odwiedzin, zakładki aktywnego panelu i zapisane serwery; Enter przechodzi tam w aktywnym panelu (Ctrl+P)"),
];

/// Help for one command.
pub fn command_help(name: &str) -> Option<&'static CommandHelp> {
    COMMAND_HELP.iter().find(|h| h.name == name)
}

/// The text of `docs/KOMENDY.md`, made from the table.
pub fn commands_markdown() -> String {
    let mut out = String::from(
        "# Komendy\n\nTa lista jest **generowana z kodu** (`crates/engine/src/command_help.rs`); test pilnuje,\n\
         żeby była aktualna. Każdą komendę można wpisać w wierszu poleceń na dole okna, użyć w\n\
         `bluepane-cli run` i przypisać do klawisza (`bind`). W nawiasach: skrót i alias.\n",
    );
    let mut group = "";
    for help in COMMAND_HELP {
        if help.group != group {
            group = help.group;
            out.push_str(&format!(
                "\n## {group}\n\n| Komenda | Opis |\n| --- | --- |\n"
            ));
        }
        out.push_str(&format!(
            "| `{}` | {} |\n",
            help.usage.replace('|', "\\|"),
            help.description.replace('|', "\\|")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Command, COMMAND_NAMES};

    #[test]
    fn every_command_has_help_and_nothing_else_does() {
        let mut named: Vec<&str> = COMMAND_NAMES.to_vec();
        let mut helped: Vec<&str> = COMMAND_HELP.iter().map(|h| h.name).collect();
        named.sort_unstable();
        helped.sort_unstable();
        assert_eq!(
            named, helped,
            "COMMAND_NAMES and COMMAND_HELP list the same commands"
        );
        let mut unique = helped.clone();
        unique.dedup();
        assert_eq!(unique.len(), helped.len(), "no command twice");
        for help in COMMAND_HELP {
            assert!(
                help.usage.starts_with(help.name),
                "{}: {}",
                help.name,
                help.usage
            );
            assert!(
                help.description.len() > 8,
                "{} has a description",
                help.name
            );
        }
    }

    #[test]
    fn every_listed_command_is_known_to_the_parser() {
        for help in COMMAND_HELP {
            if let Err(error) = Command::parse(help.name) {
                assert!(
                    !error.0.contains("nieznana komenda"),
                    "{} is not parsed: {error}",
                    help.name
                );
            }
        }
    }

    /// The names at the start of the arms of `Command::parse` (a small scan of the source, so
    /// that a command added to the parser but not to the lists fails here).
    #[test]
    fn every_command_the_parser_accepts_is_listed() {
        // Checkouts on Windows may use CRLF line endings.
        let source = include_str!("command.rs").replace("\r\n", "\n");
        let start = source.find("pub fn parse(").expect("parse exists");
        let end = start + source[start..].find("\n    }\n").expect("parse ends");
        let mut unlisted = Vec::new();
        for line in source[start..end].lines() {
            let Some(arm) = line.strip_prefix("            \"") else {
                continue;
            };
            // The command is the first quoted word of the arm; aliases follow after `|`.
            let first = arm.split('"').next().unwrap_or("");
            if !first.is_empty() && !COMMAND_NAMES.contains(&first) {
                unlisted.push(first.to_string());
            }
        }
        assert!(unlisted.is_empty(), "commands without help: {unlisted:?}");
    }

    #[test]
    fn the_commands_page_of_the_docs_is_up_to_date() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/KOMENDY.md");
        let wanted = commands_markdown();
        if std::env::var_os("BLUEPANE_UPDATE_DOCS").is_some() {
            std::fs::write(&path, &wanted).unwrap();
        }
        let current = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert_eq!(
            current, wanted,
            "docs/KOMENDY.md is out of date: run `BLUEPANE_UPDATE_DOCS=1 cargo test -p bluepane-engine command_help`"
        );
    }
}
