# bluepane

Dwupanelowy menedżer plików w stylu Total Commandera. Rust + egui, natywny, sterowany
klawiaturą, działa na **Windows i Linuksie**. Projekt we wczesnej fazie (szkielet, etap 1 z 9).

## Uruchomienie

Wymagany Rust 1.88 lub nowszy (`rustup`).

```
cargo run --release -p bluepane              # oba panele w bieżącym katalogu
cargo run --release -p bluepane -- ~/a ~/b   # lewy i prawy katalog
```

Linux: do zbudowania okna potrzebne są biblioteki systemowe (przykład dla Debiana/Ubuntu):

```
sudo apt install libxkbcommon-dev libwayland-dev libx11-dev libxcb1-dev libgtk-3-dev
```

## Klawisze

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

Wiersz poleceń na dole przyjmuje te same komendy: `cd <ścieżka>`, `back`, `forward`,
`volume <numer|nazwa>`, `tab <new|open|close [n]|next|prev|n>`, `filter <tekst>`, `mkdir <nazwa>`, `copy`, `move`, `delete`,
`selmask <maska>`, `unselmask <maska>`, `selectall`, `unselectall`, `invert`,
`sort <name|ext|size|date> [asc|desc]`, `pack`, `unpack`, `testarchive`, `select <nazwa>`, `cursor <n>`, `rename [nazwa]`, `confirm` / `cancel` (odpowiedź na pytanie), `stop` (przerwij zadanie),
`sync`, `syncrun`, `syncset`, `syncclose`, `queue`, `queuepause`, `queueresume`, `queuedel`, `queueretry`, `queueclear`, `queueclose`, `editsend`, `sites`, `siteset`, `sitedel`, `siteclose`, `connect`, `disconnect`, `find`, `findset`, `findrun`, `findstop`, `findgo`, `findview`, `findclose`, `multirename`, `mrset`, `mrname`, `mrrun`, `mrundo`, `mrclose`,
`overwrite`, `newer` (nadpisz starsze), `skip`, `keepboth` (z końcówką `-all` = dla wszystkich;
odpowiedzi na „plik już istnieje”), `refresh`.

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

**Dokumenty biurowe:** F3 na pliku Word (`.docx`, `.docm`), OpenDocument (`.odt`, `.ods`), Excel (`.xlsx`) lub `.pdf` pokazuje **tekst dokumentu**:
akapity, nagłówki (`#`), listy (`•`), tabele i arkusze jako wiersze `a | b | c`, a PDF strona po stronie. To podgląd do czytania, nie renderer: bez układu strony,
obrazków i czcionek. Działa także dla plików w archiwum i na serwerze. Limity: 64 MB (PDF 32 MB); skan PDF bez tekstu i dokumenty chronione hasłem
dają czytelny komunikat. **F4** otwiera taki plik w programie systemu (Word, LibreOffice, przeglądarka PDF) albo w `BLUEPANE_EDITOR`; bluepane nie edytuje dokumentów samodzielnie.


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

## Kolejka zadań (Ctrl+J)

Kopiowanie, przenoszenie, pakowanie, transfery z serwerów i inne długie zadania idą jedno po drugim. Okno kolejki pokazuje bieżące zadanie z postępem,
oczekujące (każde można **usunąć z kolejki**, zanim ruszy) i ostatnie zakończone (każde można **ponowić**, np. po błędzie). **Wstrzymaj kolejkę** pozwala
bieżącemu zadaniu się dokończyć, a następne czekają, dopóki ich nie wznowisz; odświeżanie list serwerów i logowanie nie czekają. `stop` przerywa bieżące
i czyści oczekujące.

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
- **Praca w panelu:** Enter / Backspace / `cd` / F9 przeglądają serwer; F5 pobiera (z drugiego panelu) lub wysyła (do panelu serwera), także całe katalogi;
  konflikty pytają jak przy kopiowaniu (nadpisz / starsze / pomiń / zachowaj oba); F7 nowy katalog, F2 zmiana nazwy, F8 usuwa **trwale** (serwer nie ma kosza, pytanie to mówi wprost).
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

## Bez okna

```
cargo run -p bluepane-cli -- commands
cargo run -p bluepane-cli -- ls <katalog>
cargo run -p bluepane-cli -- run <lewy> <prawy> "select a.txt" "copy"
cargo run -p bluepane-cli -- run <lewy> <prawy> "select a.txt" "delete" "confirm"
```

## Dla developerów

```
cargo xtask ci    # fmt, clippy, testy, reguły warstw (to samo robi GitHub Actions)
```

Układ crate'ów, zasady i sposób pracy z Claude Code: [CLAUDE.md](CLAUDE.md). Plan i stan prac:
[ROADMAP.md](ROADMAP.md).

## Licencja

MIT lub Apache-2.0, do wyboru. Nazwa, kod i zasoby są niezależne od Total Commandera.
