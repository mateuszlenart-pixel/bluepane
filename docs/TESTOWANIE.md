# Testowanie ręczne bluepane (Linux i Windows)

Ten plik jest dla testera. Zawiera: jak uruchomić program, jak przygotować dane, listę rzeczy do
sprawdzenia (z oczekiwanym wynikiem) i miejsce na wyniki. CI sprawdza logikę i kompilację na
obu systemach, ale **nie widzi okna**, prawdziwej sieci, serwerów ani programów zewnętrznych:
to właśnie tu trzeba obejrzeć.

Oznaczenia: **L** = sprawdź na Linuksie, **W** = sprawdź na Windows, **L W** = na obu.
Pole `[ ]` zaznacz `[x]`, gdy działa zgodnie z opisem; gdy nie działa, dopisz po `→` co się
stało (patrz „Jak zgłaszać błędy").

## 1. Uruchomienie

### Wariant A: gotowy plik z GitHuba (bez Rusta)

1. Wejdź na repozytorium w GitHubie, zakładka **Actions**, wpis **Build testowy** (najnowszy
   udany przebieg na gałęzi `main`; można też uruchomić go ręcznie: *Run workflow*).
2. Na dole strony przebiegu, w sekcji **Artifacts**, pobierz:
   - Linux: `bluepane-linux` (pobrany `.zip` z `bluepane-linux.tar.gz` w środku),
   - Windows: `bluepane-windows` (pobrany `.zip` z `bluepane-windows.zip` w środku).
3. Rozpakuj. W środku jest katalog `bluepane` z programem, danymi testowymi (`testdata`),
   skryptami (`tools`), README i tym plikiem. Plik `WERSJA.txt` mówi, z którego commita
   pochodzi build; zapisz go przy zgłoszeniu błędu.

**Linux** (terminal):

```
tar -xzf bluepane-linux.tar.gz
cd bluepane
chmod +x bluepane bluepane-cli tools/dane-testowe.sh
./bluepane                 # albo:  ./bluepane ~/jakis/katalog ~/drugi/katalog
```

Do działania okna potrzebne są zwykłe biblioteki graficzne pulpitu (na Linux Mint są).
Jeśli program odmawia startu, uruchom go z terminala i skopiuj komunikat.

**Windows** (PowerShell albo Eksplorator):

GitHub pakuje artefakt jeszcze raz, więc pobrany plik `bluepane-windows.zip` (zewnętrzny) zawiera
drugi `bluepane-windows.zip` (wewnętrzny) o **tej samej nazwie**. Rozpakowanie obu w jednym
katalogu kończy się błędem „file already exists”. Rozpakuj więc w dwóch krokach, do osobnych katalogów:

```
Expand-Archive bluepane-windows.zip -DestinationPath .\pobrane
Expand-Archive .\pobrane\bluepane-windows.zip -DestinationPath .
cd bluepane
.\bluepane.exe             # albo:  .\bluepane.exe C:\Users\ja\a D:\
```

(W Eksploratorze: rozpakuj pobrany plik do nowego folderu, w środku jest drugi zip, rozpakuj i jego.)
Na Linuksie jest tak samo: pobrane `bluepane-linux.zip` zawiera `bluepane-linux.tar.gz`
(`unzip bluepane-linux.zip`, potem `tar -xzf bluepane-linux.tar.gz`). Wydania (zakładka
Releases, patrz README) nie mają tego dodatkowego opakowania.

Windows może pokazać ekran „System Windows chronił ten komputer" (SmartScreen), bo plik nie
jest podpisany: *Więcej informacji* → *Uruchom mimo to*. Za oknem programu otwiera się
też okno konsoli (w buildach testowych): **zostaw je**, tam trafiają komunikaty o błędach. Awarie (panic) trafiają dodatkowo do pliku `bluepane.log` w katalogu ustawień (Linux `~/.config/bluepane`, Windows `%APPDATA%\bluepane`); dołącz go do zgłoszenia błędu.

### Wariant B: budowanie ze źródeł

Wymagany Rust 1.95 lub nowszy (<https://rustup.rs>). Na Linuksie dodatkowo:

```
sudo apt install libxkbcommon-dev libwayland-dev libx11-dev libxcb1-dev libgtk-3-dev
```

```
git clone <adres repozytorium> bluepane && cd bluepane
cargo run --release -p bluepane -- ~/a ~/b        # Linux
cargo run --release -p bluepane -- C:\a D:\b      # Windows
```

Pierwsze budowanie trwa kilka minut. Gotowy plik: `target/release/bluepane` (Windows:
`target\release\bluepane.exe`).

### Czysty profil i gdzie są ustawienia

Ustawienia (witryny serwerów, motyw, układ okna `layout.json`, ulubione skanera) leżą w:
Linux `~/.config/bluepane`, Windows `%APPDATA%\bluepane`. Żeby uruchomić z czystym,
tymczasowym profilem (nic nie psuje Twoich ustawień):

```
BLUEPANE_CONFIG_DIR=/tmp/bp-profil ./bluepane                  # Linux
$env:BLUEPANE_CONFIG_DIR="$env:TEMP\bp-profil"; .\bluepane.exe # Windows (PowerShell)
```

Żeby wrócić do układu domyślnego, skasuj `layout.json` z katalogu ustawień (program musi być
zamknięty).

## 2. Dane testowe

Skrypty leżą w katalogu `tools` **wewnątrz rozpakowanego folderu `bluepane`**: najpierw `cd bluepane` (inaczej ścieżka `tools\...` nie istnieje), potem polecenia poniżej.

Skrypt kopiuje gotowe pliki (`testdata`: dokumenty, archiwa, teksty) i tworzy resztę (pary do
synchronizacji z różnymi datami, 1500 małych plików, pliki 40 MB i 300 MB, ukryty plik, plik
tylko do odczytu, dowiązanie, nazwy ze spacjami i polskimi znakami; na Windows także bardzo
długa ścieżka):

```
tools/dane-testowe.sh                     # Linux, tworzy ~/bluepane-test (ok. 350 MB)
powershell -ExecutionPolicy Bypass -File tools\dane-testowe.ps1    # Windows, %USERPROFILE%\bluepane-test
```

Skrypt nie nadpisuje: jeśli katalog istnieje i nie jest pusty, kończy się błędem. Hasła do
plików chronionych w danych testowych: **`sekret`** (PDF, ZIP, 7z).

Co gdzie jest: `dokumenty` (docx, odt, rtf, pdf, xlsx, ods, pptx, odp, PDF z hasłem),
`archiwa` (zip, tar.gz, 7z, zip i 7z z hasłem), `teksty` (UTF-8, CP1250, kod, binarny, długi
tekst 5000 wierszy), `nazwy` (do zmiany nazw), `synchronizacja/lewy` i `prawy`, `duzo-plikow`,
`duze`, `rozne`. Dla narzędzi z etapów 11–14 dochodzą: `kod` (pliki do kolorowania składni
i `README.md` do renderowania Markdown), `hex` (256 bajtów do edytora hex), `duplikaty`,
`porownanie` (pary do porównania i sum kontrolnych), `indeks` (teksty z polskimi znakami, także
w Windows-1250), `media` (zdjęcia z EXIF i bez, utwory `.wav` z tagami), `zdjecia` i `muzyka`
(kopie do zmiany nazw z EXIF i tagów) oraz `repozytorium-git` (repozytorium z jednym commitem i
zmianami M, D, ?, A; tworzone tylko, gdy jest program `git`).

Przykładowe uruchomienie (Linux): `./bluepane ~/bluepane-test ~/bluepane-test/synchronizacja`; do dymu
(4.0): `./bluepane ~/bluepane-test ~/bluepane-test/kod`. **Windows (PowerShell, z katalogu `bluepane`):**
`.\bluepane.exe "$env:USERPROFILE\bluepane-test" "$env:USERPROFILE\bluepane-test\kod"`. Kropka i ukośnik
na początku są obowiązkowe: PowerShell nie uruchamia programów z bieżącego katalogu bez ścieżki.

**Ćwicz na kopiach.** Skrypt nigdy nie nadpisuje, ale testy zmieniają dane (kosz, edytor hex,
profile synchronizacji): jeśli coś zepsujesz, usuń `bluepane-test` i uruchom skrypt od nowa.

## 3. Jak zgłaszać błędy

Dla każdego problemu zapisz: system i wersję (Linux Mint 22 / Windows 11 itd.), zawartość
`WERSJA.txt`, numer punktu z listy poniżej, **co kliknięto lub wpisano**, co się stało, co
miało się stać, zrzut ekranu i komunikat z terminala/konsoli. Jeśli program się zamknął,
najważniejszy jest ostatni komunikat w terminalu (Linux) albo w oknie konsoli (Windows).
Wygląd (czytelność, kolory, rozmiary) też zgłaszaj, nawet jeśli „działa".

## 4. Lista kontrolna

### Jak do tego podejść

Lista jest długa (ok. 32 sekcje), więc nie przechodź jej w całości naraz. Proponowana kolejność:

1. **Dym (4.0):** ok. 20 minut na każdym systemie. Jeśli tu coś nie działa, reszta ma mniejszy sens: zgłoś od razu.
2. **Sekcje według ryzyka:** tabela niżej. Zacznij od **P1**.
3. **Sekcje `[Z]`** (potrzebują czegoś z zewnątrz: serwera SMB, rclone, urządzenia do Wake-on-LAN) rób tylko wtedy, gdy to masz.
4. **Wyniki wpisuj partiami** (jedna sekcja = jedna porcja, patrz 5): błędy z jednej porcji naprawiam razem, w jednym PR.

**Priorytety:** **P1** = nigdy nie uruchomione na Windows albo na prawdziwych danych i usługach, tu najłatwiej o błąd; **P2** = wygląd i wygoda nowych okien (logika jest sprawdzona testami, ale tylko Ty widzisz, czy jest czytelnie); **P3** = starsze funkcje, sprawdzane już wcześniej.

**`[CI]`** przy punkcie oznacza, że logikę tego punktu sprawdzają testy automatyczne na obu systemach; Ty oceniasz tylko to, co widać.

| Sekcja | Priorytet | Dlaczego |
| --- | --- | --- |
| 4.21 Makra, przyciski, harmonogram | **P1** | `crontab` i Harmonogram zadań Windows, `bluepane-cli`; nigdy nie uruchomione na prawdziwym systemie |
| 4.27 Samoodświeżanie list | **P1** | inotify i ReadDirectoryChangesW; zachowanie zależy od systemu |
| 4.30 Podgląd: edytor hex | **P1** | zapis przez kopię i podmiana pliku; zachowanie plików na Windows |
| 4.28 Git | **P1** | ścieżki i program `git` na Windows |
| 4.29 Indeks | **P1** | czas i pamięć na dużym katalogu, prawdziwe PDF/Word |
| 4.24 Skaner: Wake-on-LAN i udział `[Z]` | **P1** | prawdziwa sieć i urządzenia |
| 4.25 SMB `[Z]` | **P1** | prawdziwy serwer SMB |
| 4.26 Chmury rclone `[Z]` | **P1** | prawdziwe konto w chmurze |
| 4.32 Instalacja z paczek | **P1** | nigdy nie sprawdzana na prawdziwej, czystej maszynie |
| 4.17–4.20, 4.22, 4.23, 4.30 (reszta), 4.31 | P2 | nowe okna i narzędzia: wygląd, wygoda |
| 4.1–4.16 | P2 na Windows, P3 na Linuksie | starsze funkcje; Windows nie był jeszcze sprawdzany |

### 4.0 Dym (L W, ok. 20 minut)

Przygotuj dane (`tools/dane-testowe`, patrz 2) i uruchom program z dwoma katalogami (polecenie dla Linuksa i Windows w sekcji 2). Jeśli punkt się nie powiedzie, dopisz `→` co się stało i **idź dalej** (zgłosisz po całości).

- [ ] **Start:** okno się otwiera, dwa panele, ikony i polskie znaki w nazwach (`indeks/głęboko`) wyświetlają się poprawnie. (4.1)
- [ ] **Motywy:** `theme light`, potem `theme dark`: oba czytelne (listy, okna, kolory nazw). (4.19)
- [ ] **Operacje:** F5 kopiuje mały plik do drugiego panelu, F2 zmienia nazwę w wierszu, F8 przenosi do kosza. (4.3, 4.18)
- [ ] **F1:** paleta komend; wpisz `sciez`: na liście jest `cd`. (4.17)
- [ ] **Ctrl+K:** wpisz `zaz`: na liście plik `Zażółć…`; Esc zamyka. (4.17)
- [ ] **Ctrl+P / Ctrl+B:** „Skocz do” i pasek Miejsca otwierają się i przechodzą do wybranego katalogu. (4.17)
- [ ] **Prawy klik** na pliku: menu z „Podgląd”, „Kopiuj ścieżkę”, „Właściwości”. (4.18)
- [ ] **Przeciąganie:** plik z lewego panelu na prawy kopiuje go. (4.18)
- [ ] **F3** na `kod/przyklad.rs`: kolorowa składnia, w nagłówku `tekst, Rust`; `C` wyłącza kolory. (4.30)
- [ ] **F3** na `kod/README.md`: wyrenderowany Markdown (nagłówki, lista, cytat); `M` przełącza na tekst. (4.30)
- [ ] **F3** na `hex/maly.bin`, klawisz `E`, wpisz `ff`: bajt zmienia kolor; pierwszy Esc kończy edycję, drugi próbuje zamknąć i **ostrzega** o zmianach, trzeci zamyka; plik na dysku jest bez zmian. (4.30)
- [ ] **Szczegóły:** kursor na `media/zdjecie-exif.jpg`, komenda `details`: widać aparat i datę. (4.30)
- [ ] **Samoodświeżanie:** (prawy panel to `kod`) w drugim terminalu `touch ~/bluepane-test/kod/nowy.txt` (Windows: `ni $env:USERPROFILE\bluepane-test\kod\nowy.txt`): plik pojawia się na liście bez F9. (4.27)
- [ ] **Git:** `cd ~/bluepane-test/repozytorium-git`, `columns git`: litery M, D, ?, A przy plikach i `git: main` przy ścieżce. (4.28)
- [ ] **Indeks:** `index add here` w `indeks`, po chwili `index find zolw`: lista z `umowa.txt`. (4.29)
- [ ] **Duplikaty:** w `duplikaty` komenda `dupes`: jedna grupa z trzema plikami. (4.22)
- [ ] **Porównanie:** w `porownanie` zaznacz `lewy.txt` i `prawy.txt`, komenda `compare`: różnice podświetlone. (4.22)
- [ ] **Makro:** `macro record Dym`, wpisz `sort size`, `macro stop`, `macro play Dym`: sortowanie się powtarza. (4.21)
- [ ] **Sesja:** zamknij program i uruchom go bez argumentów: wracają katalogi i zakładki. (4.20)

### 4.1 Start i wygląd (L W)

- [ ] Okno otwiera się, są dwa panele z ikonami typów plików i kolorowymi nazwami (katalogi
      niebieskie, archiwa fioletowe, dokumenty pomarańczowe, obrazy różowe, kod miętowy).
- [ ] Aktywny panel ma niebieską ramkę i jaśniejsze tło, nieaktywny jest ciemniejszy. Tab
      przełącza.
- [ ] Wszystkie ikony się wyświetlają (żadnych kwadracików i „?" zamiast ikon).
- [ ] Nad każdym panelem: lista dysków, pasek zajętości i „X wolne z Y", ścieżka jako
      klikalne segmenty. Kliknięcie segmentu przechodzi tam; kliknięcie pustego miejsca paska
      zamienia ścieżkę w pole do wpisania (Enter = przejście).
- [ ] Pod listą jest linia podsumowania („21 elementów: 10 katalogów, 11 plików (…)"; po
      zaznaczeniu „Zaznaczono: …").
- [ ] Na dole są dwa rzędy przycisków klawiszy funkcyjnych, żaden nie jest ucięty; zmniejsz
      i powiększ okno: rzędy dopasowują się.
- [ ] Polskie znaki w nazwach (`Zażółć gęślą.txt`) wyświetlają się poprawnie.
- [ ] Komenda `theme` w wierszu poleceń wymienia motywy (`dark`, `light`, `system`); `theme light`
      przełącza na jasny; nieznany motyw (`theme różowy`) daje czytelny błąd po polsku.
- [ ] **W:** czy okno wygląda dobrze przy skalowaniu ekranu 125% i 150%?
- [ ] Własne skróty: `bind ctrl+k sort name` w wierszu poleceń, Ctrl+K sortuje po nazwie; `unbind f9` wyłącza F9; `bind f9 default` przywraca; po restarcie zmiany zostają (plik `settings.conf` w katalogu ustawień; błędny wiersz w nim daje ostrzeżenie w pasku stanu, a program startuje).

### 4.2 Nawigacja i zaznaczanie (L W)

- [ ] Strzałki, PgUp/PgDn, Enter wchodzi do katalogu, Backspace wychodzi, wiersz `..` też.
- [ ] Alt+← i Alt+→ (historia katalogów) działają osobno w każdym panelu.
- [ ] Wpisanie liter zawęża listę (odznaka „Filtr: …" z ✕); Esc lub ✕ czyści; wiersz poleceń
      dostaje klawiaturę dopiero po kliknięciu lub Ctrl+L.
- [ ] Spacja/Insert zaznacza i schodzi niżej (nazwy zaznaczonych na bursztynowo); Ctrl+A,
      Ctrl+Shift+A, Ctrl+I, Ctrl+M (maska np. `*.txt`).
- [ ] Myszą: klik przesuwa kursor, **Ctrl+klik** zaznacza wiersz, **Shift+klik** zaznacza
      zakres, Ctrl+Shift+klik dodaje zakres; podwójny klik wchodzi do katalogu.
- [ ] Sortowanie: klik w nagłówek kolumny (strzałka ▲/▼) i Ctrl+F3…F6.
- [ ] Zakładki katalogów: Ctrl+T (nowa), Ctrl+W (zamknij), Ctrl+Tab, Ctrl+1…9; klik × i
      środkowy przycisk myszy zamykają.
- [ ] Alt+1…9 przeskakuje do dysku o tym numerze.
- [ ] **W:** dyski (C:, D:, dyski sieciowe, pendrive) są na liście; wejście na dysk
      bez płyty/nośnika daje czytelny komunikat, a nie zawieszenie.
- [ ] **W:** ścieżki `C:\` i okruszki: pierwszy segment to `C:\`, nie dwa osobne segmenty.

### 4.3 Operacje na plikach (L W)

Pracuj w `bluepane-test`; lewy panel `nazwy`, prawy pusty katalog (F7 = nowy).

- [ ] F5 kopiuje zaznaczone do drugiego panelu; pasek zadania na dole pokazuje postęp.
- [ ] Kopiowanie na istniejący plik pyta o decyzję (nadpisz / starsze / pomiń / oba / anuluj);
      „zastosuj do wszystkich" działa.
- [ ] F6 przenosi, F2 zmienia nazwę (nie nadpisuje istniejącego), F7 tworzy katalog,
      nazwa niedozwolona daje komunikat.
- [ ] F8 pyta i przenosi **do kosza** (sprawdź w koszu systemu, że plik tam jest).
- [ ] **W:** plik tylko do odczytu (`rozne\tylko-do-odczytu.txt`): kopiowanie i usunięcie do
      kosza działają lub dają zrozumiały komunikat.
- [ ] **W:** ścieżka dłuższa niż 260 znaków (`bluepane-test\katalog-o-dlugiej-nazwie-…`):
      wejście, kopiowanie i usunięcie działają.
- [ ] **L:** dowiązanie symboliczne (`rozne/dowiazanie.txt`) kopiuje się jako dowiązanie i
      nie psuje listy.
- [ ] Kopiowanie pliku 300 MB (`duze`): postęp, bajty, **prędkość i czas do końca** na pasku;
      „Anuluj" przerywa, a niedokończony plik nie zostaje pod prawdziwą nazwą.
- [ ] Alt+Shift+Enter (`dirsize`) na kilku katalogach: w kolumnie rozmiaru pojawiają się
      rozmiary z paskami, okno nie zwalnia; F9 je czyści.

### 4.4 Kolejka zadań i limit prędkości (L W)

- [ ] Ctrl+J otwiera kolejkę jako zakładkę; „Wstrzymaj kolejkę" wstrzymuje start następnych.
- [ ] Zakolejkuj 3 kopiowania (pauza → trzy razy F5 na różnych plikach → wznów): przyciski ▲ ▼
      „Na początek" zmieniają kolejność i zadania wykonują się w nowej kolejności.
- [ ] Zakończone zadania są na liście; „Ponów" uruchamia je jeszcze raz.
- [ ] Lista „Limit prędkości" w kolejce (albo komenda `limit 2m`): kopiowanie 300 MB idzie
      wyraźnie wolniej (pasek zadania pokazuje ok. 2 MB/s); zmiana limitu w trakcie działa od
      razu; `limit off` zdejmuje.

### 4.5 Archiwa (L W)

- [ ] Enter na `paczka.zip`, `paczka.tar.gz`, `paczka.7z` wchodzi do środka; `..` wychodzi.
- [ ] F5 z archiwum rozpakowuje do drugiego panelu; Alt+F9 (rozpakuj) też.
- [ ] Archiwa z hasłem (`zip-haslo-sekret.zip`, `7z-haslo-sekret.7z`): pole hasła jest
      ukryte; błędne hasło pyta ponownie; `sekret` działa.
- [ ] Alt+F5 (pakuj) tworzy zip i 7z z wybranymi plikami; wynik otwiera się i ma polskie nazwy.
- [ ] **W:** pakowanie i rozpakowanie plików z polskimi znakami w nazwach; nazwy
      niedozwolone w Windows w archiwum z Linuksa dają zrozumiały komunikat.

### 4.6 Szukanie (L W)

- [ ] Ctrl+F: szukanie po nazwie (`*.txt`) i po treści (`szukane-slowo-42` w `teksty`):
      wyniki pojawiają się na bieżąco, „Stop" działa.
- [ ] Wyniki to wirtualna lista w panelu; Enter przechodzi do pliku.
- [ ] Szukanie w archiwum zip oraz „wszystkie dyski" (uwaga: może trwać; sprawdź czas i
      zużycie pamięci).
- [ ] Tekst CP1250 (`cp1250-polskie.txt`) znajduje się po polskim słowie (kodowanie auto lub
      `cp1250`).

### 4.7 Zmiana nazw (Ctrl+F2) (L W)

Zaznacz wszystko w `nazwy` (Ctrl+A), Ctrl+F2.

- [ ] Maska `[N]_v2`: w kolumnie nowych nazw **tylko dodane znaki** są podświetlone na
      zielono, przy gotowych nazwach zielony znaczek.
- [ ] Licznik `[C]`, zmiana wielkości liter, szukaj i zamień (także RegWy), rozszerzenie.
- [ ] Dwie nazwy na tę samą (np. maska `stała`): konflikt oznaczony czerwono z ostrzeżeniem i
      powodem w podpowiedzi; „Start" nie wykonuje konfliktu.
- [ ] Klik w nową nazwę pozwala wpisać ją ręcznie.
- [ ] Start zmienia nazwy na dysku; „Cofnij" (Ctrl+Z) przywraca.
- [ ] **W:** zmiana `a.txt` na `A.TXT` (tylko wielkość liter) działa; nazwa niedozwolona
      (`con`, `aux`, kończąca się kropką lub z `:`) daje konflikt, nie błąd systemu.

### 4.8 Synchronizacja (Alt+S) (L W)

Lewy panel `synchronizacja/lewy`, prawy `synchronizacja/prawy`.

- [ ] Okno pokazuje 5 różnic: zielona strzałka (kopiuj w prawo), niebieska (w lewo), po dwie
      pozycje z każdej strony według dat; różnice w rozmiarze i katalogu `podkatalog`.
- [ ] Klik w wiersz zmienia kierunek (prawo → lewo → pomiń); ikony i kolory się zmieniają.
- [ ] Warianty (dwukierunkowa, lustro, aktualizacja) zmieniają listę po „Porównaj" (F9).
- [ ] „Usunięte i zastąpione pliki: do kosza / do katalogu wersji / trwale": trwałe ostrzega
      na czerwono.
- [ ] „Synchronizuj" wykonuje plan; po nim drugie porównanie nie pokazuje różnic.
- [ ] Zawartość (porównanie po zawartości) wykrywa plik o tym samym rozmiarze i dacie, ale
      innej treści (zmień jeden bajt ręcznie).
- [ ] **W:** różnica dat rzędu 1-2 sekund (FAT/NTFS): tolerancja nie pokazuje fałszywych różnic.

### 4.9 Podgląd plików F3 i strony dokumentów (L W)

- [ ] `teksty/dlugi-tekst.txt`: przewijanie strzałkami, PgDn, Home/End, kółkiem; `/` szuka
      (`szukane-slowo-42`), N dalej.
- [ ] `teksty/binarny.bin` otwiera się jako hex; `H` przełącza tekst/hex.
- [ ] `cp1250-polskie.txt` i `utf8-polskie.txt`: polskie litery poprawne (CP1250 może wymagać
      przełączenia kodowania).
- [ ] **PDF** (`raport.pdf`): strony jako obrazy, ◀ ▶ / PgDn zmieniają stronę, + − 0
      zmieniają powiększenie, przy dużym powiększeniu są paski przewijania i przesuwanie
      myszą.
- [ ] **PDF z hasłem** (`raport-haslo-sekret.pdf`): F3 pyta o hasło; błędne = ponowne pytanie
      z komunikatem; `sekret` otwiera; Esc rezygnuje.
- [ ] **Word/Excel/PowerPoint/OpenDocument/RTF**: z zainstalowanym **LibreOffice** pokazują
      się jako strony (przy pierwszym razie może to potrwać kilka sekund); bez LibreOffice
      (lub z `BLUEPANE_NO_OFFICE=1`) pokazuje się tekst dokumentu.
- [ ] Pliki dokumentów **z archiwum** (`paczka.zip` → `raport.docx`, `podkatalog/raport.pdf`):
      F3 działa jak dla lokalnych, kopia tymczasowa znika po zamknięciu podglądu.
- [ ] F4 otwiera plik w programie systemu (`BLUEPANE_EDITOR` zmienia program).
- [ ] **W:** F4 na `.docx` otwiera Worda lub program skojarzony; F4 na pliku z polską nazwą.

### 4.10 Quick View (Ctrl+Q) (L W)

- [ ] Ctrl+Q: drugi panel pokazuje plik pod kursorem aktywnego; **okno programu nie
      zamyka się** (to ważne: skrót Ctrl+Q bywa „wyjściem").
- [ ] Przejście kursorem po `teksty`, `dokumenty`, `archiwa`: tekst, hex, strona PDF
      (przyciski ◀ ▶), tekst dokumentów, notatka dla archiwum/multimediów.
- [ ] Szybkie przytrzymanie strzałki w dół po dużych plikach nie zawiesza okna.
- [ ] Kółko myszy przewija tekst; Ctrl+Q zamyka podgląd.

### 4.11 Schowek (L W)

- [ ] Ctrl+Shift+1/2/3 po zaznaczeniu plików: wklej do Notatnika/edytora: nazwy bez
      rozszerzeń / z rozszerzeniami / pełne ścieżki, po jednej w linii.

### 4.12 Dok: zakładki i układ (L W)

- [ ] Otwórz kilka narzędzi naraz (np. Ctrl+J kolejka, Alt+S synchronizacja, Ctrl+F szukanie):
      każde ma osobną zakładkę po prawej od paneli.
- [ ] Przeciągnij zakładkę na dół/lewo panelu: okno dzieli się; przeciągnij poza okno:
      zakładka staje się pływającym oknem.
- [ ] Zamknięcie zakładki × zamyka narzędzie; zamknięcie narzędzia klawiszem (Esc w oknie)
      usuwa zakładkę.
- [ ] Skróty narzędzia (np. F5/F9 w synchronizacji) działają, gdy jego zakładka ma fokus
      (kliknij w nią); po kliknięciu w panel klawisze wracają do panelu.
- [ ] Zmień układ (np. panele jeden nad drugim), zamknij program, uruchom ponownie: układ
      wraca (bez otwartych narzędzi). Uszkodzony `layout.json` nie psuje startu.

### 4.13 Skaner sieci (Ctrl+N) (L W)

- [ ] Domyślny zakres to Twoja podsieć; „Skanuj" pokazuje urządzenia, nazwy, producentów (po
      adresie MAC), porty i usługi; „Stop" działa.
- [ ] Akcje na hoście: otwarcie w przeglądarce (HTTP), SSH/SFTP (przejście do witryn), RDP.
- [ ] Udziały sieciowe: lista udziałów hosta z Windows/Samby; otwarcie udziału; błędy
      logowania mają zrozumiały komunikat (a nie surowy tekst systemu).
- [ ] Ulubione: dodaj, wybierz, usuń; ustawienia przeżywają restart.
- [ ] **L:** `smbclient` zainstalowany (apt install smbclient) pokazuje udziały; bez niego jest
      wskazówka co doinstalować.
- [ ] **W:** udziały przez `net view`; pulpit zdalny otwiera `mstsc`.

### 4.14 Serwery: SFTP, FTP, FTPS (Ctrl+S) (L W)

Potrzebny prawdziwy serwer. Najłatwiej: Twój serwer Proxmox/Home Assistant przez SSH, albo
lokalnie:

- Linux, SFTP: `sudo apt install openssh-server`, serwer na `127.0.0.1:22`.
- FTP: `pip install pyftpdlib`, potem `python3 -m pyftpdlib -p 2121 -w -d ~/bluepane-test` (anonimowo,
  z zapisem). Działa na Linuksie i Windows (jeśli jest Python).
- Windows, SFTP: *Ustawienia → Aplikacje → Funkcje opcjonalne → Serwer OpenSSH*, albo łącz się
  z Windowsa na serwer z Linuksa/Proxmoxa.

- [ ] Ctrl+S: dodaj witrynę (host, port, użytkownik), „Zapisz i połącz". Pierwsze połączenie
      pokazuje odcisk klucza i pyta o zaufanie (Enter = zaufaj); drugie łączy bez pytania.
- [ ] Panel pokazuje zawartość serwera jak katalog lokalny: wchodzenie, Backspace, F9.
- [ ] F5 pobiera/wysyła pliki (także całe katalogi), kolejka pokazuje postęp i prędkość;
      konflikty pytają jak lokalnie; F6 przenosi; F8 usuwa **trwale** i pytanie to mówi.
- [ ] Przerwij duży transfer („Anuluj") i uruchom ponownie: wznawia (plik `.bpart`).
- [ ] **chmod:** na serwerze `chmod 600` na pliku (Ctrl+Shift+P wpisuje komendę): sprawdź na
      serwerze `ls -l`. Na serwerze FTP bez `SITE CHMOD` jest czytelny błąd.
- [ ] **F3** na pliku z serwera (tekst, PDF, docx) działa; **F4** otwiera kopię w edytorze,
      po zapisie `editsend` (Ctrl+Shift+F4) wysyła zmiany.
- [ ] **FTP tryb aktywny:** w oknie witryny (FTP/FTPS) zaznacz „Tryb aktywny" i połącz się z
      serwerem w tej samej sieci: listowanie i transfer działają; za zaporą dostajesz błąd
      po ok. 15 s, nie zawieszenie.
- [ ] **FTPS** (jeśli masz serwer): jawny i niejawny, pytanie o odcisk certyfikatu.
- [ ] Hasła: „pytaj przy łączeniu", „magazyn haseł systemu" (**L:** Gnome Keyring/KWallet,
      **W:** Menedżer poświadczeń: sprawdź, że hasło nie ląduje w `sites.conf`), „Bitwarden"
      (program `bw` z odblokowanym sejfem i `BW_SESSION`).
- [ ] Rozłączenie i ponowne połączenie; zmiana hasła na serwerze daje czytelny błąd logowania.

### 4.15 Sprawy tylko dla Windows (W)

- [ ] Okno konsoli za programem nie przeszkadza; zamknięcie okna programu kończy proces. Ikona programu (dwa panele) widoczna w pasku zadań i w nagłówku okna.
- [ ] `bluepane.log` w `%APPDATA%\bluepane` ma wiersz „start bluepane …” z każdego uruchomienia.
- [ ] Ścieżki UNC (`\\serwer\udział`) w `cd` i na pasku ścieżki.
- [ ] Kosz: pliki z dysku D: i z pendrive'a (na nośnikach wymiennych kosza może nie być:
      komunikat zamiast cichego twardego usunięcia).
- [ ] Antywirus/Defender nie blokuje programu ani nie spowalnia kopiowania w sposób widoczny.
- [ ] Ukryte i systemowe pliki widać (`rozne\ukryty.txt` przygaszony).

### 4.16 Sprawy tylko dla Linuksa (L)

- [ ] Uprawnienia w kolumnach/komendzie `chmod` na plikach lokalnych (komenda działa tylko na
      serwerze: lokalnie powinna mówić dlaczego).
- [ ] Wolumeny z `/proc/mounts` (dyski, pendrive, udziały sieciowe) są na liście; martwy udział
      sieciowy nie zawiesza okna (wolne miejsce jest pytane w tle).
- [ ] Wayland i X11: okno, schowek (Ctrl+Shift+1/2/3) i skróty działają w obu sesjach (jeśli
      masz obie). Skróty zarezerwowane przez menedżer okien (np. Alt+F7) są poza naszą
      kontrolą: użyj Ctrl+F.

### 4.17 Paleta, „Skocz do”, Ctrl+K, Miejsca, omnibox (L W) · P2

`[CI]` sprawdza dopasowanie rozmyte, ranking katalogów i parsowanie adresów; tu oceniasz wygodę i wygląd.

- [ ] **F1:** wpisz `sciez` (bez polskich liter): na liście jest `cd`. Enter na komendzie z argumentem wpisuje ją do wiersza poleceń z końcową spacją. Uruchom kilka komend, zamknij program i otwórz go: ostatnio użyte są na górze.
- [ ] **Ctrl+P:** wejdź do kilku katalogów, otwórz „Skocz do”: najczęściej i najświeżej odwiedzane są wyżej; Enter przechodzi w aktywnym panelu. Katalog, który usuniesz z dysku, znika z listy.
- [ ] **Ctrl+K:** `zaz` pokazuje plik `Zażółć gęślą.txt` w bieżącym katalogu (wystarczy `zaz`), `>sort` tylko komendy, `?*.txt` uruchamia szukanie (okno `find`); Enter na pliku ustawia na nim kursor.
- [ ] **Ctrl+B (Miejsca):** dyski z paskiem zajętości (kolor od 85% i 95%), `fav` dodaje bieżący katalog do ulubionych, ✕ go usuwa, ulubione przeżywają restart, „Ostatnie” pokazuje osiem ostatnich katalogów.
- [ ] **Omnibox:** klik w puste miejsce za okruszkami zamienia je w pole. `>sort size` Enter sortuje, `?*.pdf` szuka w bieżącym katalogu, podkatalog od trzech znaków dostaje podpowiedź (klik wstawia).
- [ ] **Okruszki:** klik w `›` między segmentami pokazuje katalogi rodzeństwa; klik w katalog przechodzi do niego.
- [ ] **W:** okruszki i omnibox działają dla `C:\...` (pierwszy segment to `C:\`) i dla `\\serwer\udział`.

### 4.18 Menu kontekstowe, przeciąganie, F2 w wierszu, powiadomienia (L W) · P2

- [ ] **Prawy klik** (albo Shift+F10): na pliku (Otwórz w programie, Podgląd, Edytuj, Kopiuj/Przenieś, Zmień nazwę, Spakuj, Usuń, Kopiuj ścieżkę / nazwę, Właściwości), na katalogu (Otwórz, Otwórz w nowej zakładce, Dodaj do ulubionych), na archiwum (Otwórz archiwum, Rozpakuj do drugiego panelu, Sprawdź archiwum) i na pustym miejscu (Nowy katalog, Odśwież, Szukaj plików, Nowa zakładka...). Esc albo klik poza menu zamyka. „Właściwości” pokazuje opis w pasku stanu.
- [ ] **Przeciąganie:** plik z jednego panelu na drugi kopiuje (jak F5), z przytrzymanym **Shift** przenosi (jak F6); upuszczenie na wiersz katalogu kopiuje do niego; przy wskaźniku widać „Kopiuj: …” albo „Przenieś: …”; **Esc** w trakcie przeciągania anuluje.
- [ ] **Z systemu:** przeciągnij plik z menedżera plików systemu (Nautilus, Eksplorator) na panel albo na wiersz katalogu: zostaje skopiowany (nigdy przeniesiony).
- [ ] **F2 w wierszu:** nazwa staje się polem, zaznaczona jest część bez rozszerzenia (dla katalogu cała nazwa); Enter zatwierdza, Esc anuluje, klik w inny wiersz kończy bez zmiany. Zmiana na istniejącą nazwę jest odrzucona.
- [ ] **Powiadomienia** w prawym dolnym rogu po skończonym zadaniu: sukces znika po kilku sekundach, **błąd zostaje do ✕**; klik w powiadomienie z przyciskiem „Cofnij” cofa, gdy się da.
- [ ] **Pasek akcji** (rząd z błyskawicą nad klawiszami funkcyjnymi): na archiwum „Rozpakuj tutaj / do podkatalogu / do drugiego panelu”, na kilku zaznaczonych plikach „Spakuj” i „Zmień nazwy zbiorowo”, na dwóch różnych katalogach „Porównaj / synchronizuj”. Gdy nic nie pasuje, rzędu nie ma.
- [ ] **Ściąga:** `?` pokazuje skróty; przytrzymanie samego Ctrl (ok. 0,7 s) tylko te z Ctrl; puszczenie zamyka. Tytuł zakładki, do której idą klawisze, ma ikonę klawiatury.

### 4.19 Wygląd, kolumny, filtr, siatka miniatur (L W) · P2

- [ ] **Motywy:** `theme light`, `theme system`, `theme dark`: w każdym czytelne listy, okna dialogowe, podgląd i kolory nazw; wybór zostaje po restarcie (`settings.conf`). `theme` bez argumentu wymienia dostępne.
- [ ] **Wygląd:** `appearance textsize 130` powiększa tekst całego okna (także dialogów), `appearance rowheight 30`, `appearance accent #e0457b` zmienia kolor ramki kursora i fokusu, `appearance reset` wraca do domyślnych; `density compact` / `comfortable`.
- [ ] **Kolumny:** `columns created`, `columns perms`, `columns owner`, `columns sha256`. **L:** prawa (np. `rw-r--r--`) i właściciel pokazują wartości; **W:** te dwie kolumny są puste. Przeciągnij granicę nagłówka: szerokość się zmienia i zostaje po restarcie; dwuklik przywraca domyślną; `columns default` wraca do zestawu podstawowego.
- [ ] **SHA-256:** zaznacz kilka plików w `duze` i komenda `hash`: kolumna wypełnia się w tle, okno nie zwalnia; `hash stop` przerywa; `hash copy` kopiuje wiersze `suma  nazwa`. Zmiana pliku czyści jego sumę.
- [ ] **Filtr:** w `duzo-plikow` wpisz `plik-01`, potem `ext:txt >1k nowsze:30d`, `katalogi`: lista zawęża się, jest licznik „Widoczne N z M”, Esc czyści.
- [ ] **Siatka miniatur (Ctrl+Shift+G)** w `zdjecia`: miniatury pojawiają się w tle (obraz z niebieskim tłem i żółtym kołem), strzałki chodzą po kafelkach; F3 na zdjęciu otwiera je w zakładce „Strony”.
- [ ] **W:** skalowanie ekranu 125% i 150% razem z `appearance textsize`: nic nie jest ucięte ani nie nachodzi.

### 4.20 Obszary robocze i sesja (L W) · P2

- [ ] Ustaw po dwie zakładki w panelach, różne sortowania, w jednym panelu siatkę; `workspace save Praca`. Zmień układ, otwórz inne katalogi, potem `workspace load Praca`: wszystko wraca; `workspace list` pokazuje obszar.
- [ ] Zamknij program i uruchom go **bez argumentów**: wracają katalogi, zakładki i układ okna z poprzedniej sesji. Z katalogami w wierszu poleceń sesja nie nadpisuje tego, co podałeś.
- [ ] `workspace restore off` i restart: panele otwierają się w katalogu startowym; `workspace restore on` włącza z powrotem.
- [ ] Zapisz obszar z katalogiem, który potem usuniesz z dysku, wczytaj: brakujący jest pominięty, a pasek stanu mówi, ile ich było.
- [ ] `layout save Moj`, `layout reset`, `layout load Moj`, `layout list`: układ okna wraca do zapisanego.

### 4.21 Makra, własne przyciski, harmonogram, profile synchronizacji (L W) · **P1**

`[CI]` sprawdza zapis i odtwarzanie makr, parametry i budowanie wpisów harmonogramu; **nie** sprawdza prawdziwego `crontab` ani Harmonogramu zadań.

- [ ] **Nagrywanie:** `macro record Porzadki`, wpisz `sort size`, `columns created`, `filter txt`, `filter` (w wierszu poleceń), `macro stop`. `macro show Porzadki` wymienia te komendy, `macro list` makro. Zmień sortowanie i kolumny ręcznie, `macro play Porzadki`: stan wraca do nagranego. W trakcie nagrywania pasek stanu świeci „● Nagrywam”.
- [ ] **Parametry:** utwórz plik `Przejdz.bp` z jednym wierszem `cd %1` w katalogu `macros` w katalogu ustawień (patrz 1), `macro list` go pokaże, `macro play Przejdz ~/bluepane-test/nazwy` (**W:** `$env:USERPROFILE\bluepane-test\nazwy`): aktywny panel przechodzi do podanego katalogu.
- [ ] **Bez okna:** w terminalu `./bluepane-cli run-script ~/bluepane-test ~/bluepane-test/nazwy <plik>.bp` (plik makra leży w `macros` katalogu ustawień): komendy wykonują się, a program kończy się kodem 0; błędne polecenie w skrypcie zatrzymuje go z komunikatem.
- [ ] **Własne przyciski:** `button add Edytor = gedit %f` (**W:** `notepad %f`). Pojawia się osobny rząd przycisków pod klawiszami funkcyjnymi; klik uruchamia program na pliku pod kursorem. Plik `Zażółć gęślą.txt` (spacje i polskie znaki) trafia do programu jako **jeden** argument. Przycisk z nieistniejącym programem daje komunikat z jego nazwą. `button list`, `button delete Edytor`. Przyciski przeżywają restart.
- [ ] **Harmonogram:** utwórz plik makra `Znacznik.bp` z wierszem `mkdir znacznik-harmonogramu` (w katalogu `macros`, jak wyżej). `schedule add Znacznik; hourly`, potem `schedule list`. **L:** `crontab -l` zawiera wiersz z `# bluepane:Znacznik`, a inne Twoje wpisy są nietknięte. **W:** w Harmonogramie zadań jest folder `bluepane` z zadaniem; inne zadania nietknięte.
- [ ] **Czy naprawdę się wykonuje:** `schedule add Znacznik; daily GG:MM` z godziną za 2 minuty, zamknij bluepane i poczekaj. W katalogu **lewego panelu z chwili dodania zadania** powinien powstać `znacznik-harmonogramu` (zapisz, gdzie powstał, jeśli gdzie indziej). `schedule delete Znacznik` usuwa wpis (sprawdź `crontab -l` / Harmonogram zadań).
- [ ] **Profil synchronizacji:** otwórz Alt+S w `synchronizacja/lewy` i `prawy`, ustaw wariant i „Porównaj”, w polu „Profil” wpisz nazwę i „Zapisz profil” (albo `syncprofile save Para`). Zmień katalogi w panelach, `syncprofile run Para`: panele wracają do zapisanych katalogów i plan czeka na ekranie (nic nie kopiuje się bez „Synchronizuj”). `syncprofile list`, `syncprofile delete Para`.
- [ ] **Profil w harmonogramie:** `schedule add sync-Para; daily GG:MM` (godzina za 2 minuty): po tej porze **bez udziału okna** pliki z lewej strony pary zostały zsynchronizowane. Uwaga: profil z usuwaniem „Trwale” działa bez pytania; do próby zostaw usuwanie do kosza.

### 4.22 Mapa zajętości, duplikaty, porównanie, sumy kontrolne (L W) · P2

`[CI]` sprawdza algorytmy (grupowanie duplikatów, różnice, sumy); tu wygląd okien i wygoda.

- [ ] **Mapa (`map`)** w `bluepane-test`: zakładka z kafelkami, pole proporcjonalne do rozmiaru (`duze` największe); klik w kafelek katalogu wchodzi do niego (`map up` wraca); okno nie zwalnia w trakcie liczenia; `map close`.
- [ ] **Duplikaty (`dupes`)** w `duplikaty`: jedna grupa z trzema plikami `zdjecie*.txt` / `Zażółć kopia.txt`; plików `ten-sam-rozmiar.txt` (ten sam rozmiar, inna treść) **nie** ma w grupie. `dupes keep newest`: zaznaczone są dwa starsze; `dupes keep oldest` odwraca; „Do kosza” (`dupes trash`) przenosi zaznaczone do kosza i zostaje jeden. Zaznaczenie wszystkich kopii w grupie: komenda odmawia.
- [ ] **Porównanie (`compare`)** w `porownanie`: zaznacz `lewy.txt` i `prawy.txt`: tekst obok tekstu, zmieniony wiersz żółty, brakujący po lewej czerwony/ nowy po prawej zielony; F8 / F7 skaczą między różnicami. Dla `bin1.bin` i `bin2.bin`: komunikat o pierwszym różnym bajcie (offset 2000, `0x07d0`) z kontekstem hex.
- [ ] **Sumy:** zaznacz kilka plików w `porownanie`, `checksum create sha256`: powstaje `SHA256SUMS`; `checksum verify` na nim pokazuje „zgodny” przy każdym. Zmień jeden plik i `checksum verify` jeszcze raz: ten jeden jest „ZMIENIONY”. Drugi `checksum create sha256` nie nadpisuje istniejącego pliku.

### 4.23 Zmiana nazw z EXIF i tagów (L W) · P2

`[CI]` sprawdza maski i odczyt danych z pliku; tu oceniasz okno.

- [ ] W `zdjecia`: zaznacz wszystko, Ctrl+F2, maska `[EXIF:YMD]_[N]` (przyciski wstawiania mają EXIF i TAG): `IMG_0001.jpg` dostaje `20250714_IMG_0001`, `IMG_0002.jpg` dostaje `20241224_IMG_0002`, a `IMG_0003.jpg` (bez EXIF) dostaje **pusty fragment**, czyli `_IMG_0003` (przy kilku takich plikach podgląd pokaże konflikt zamiast po cichu nadpisać). Start zmienia nazwy, Ctrl+Z (`mrundo`) cofa.
- [ ] Maska `[EXIF:Make]` daje `Aparat Żółć` i `Inny Aparat` (znaki niedozwolone w nazwie pliku są zastępowane), `[EXIF:Model]` model.
- [ ] W `muzyka`: maska `[TAG:artist] - [TAG:title]` daje `Zespół Żółć - Gęślą jaźń.wav` i `Beta Kwartet - Łódź nocą.wav`; `[TAG:track]`, `[TAG:album]`, `[TAG:year]` też działają.
- [ ] Przy zaznaczeniu wielu plików okno nie zawiesza się podczas odczytu (dane czytane są tylko dla masek, które o nie proszą).

### 4.24 Skaner: Wake-on-LAN, udział w panelu, witryna, historia (L W) · **P1** `[Z]`

Potrzebna sieć domowa z kilkoma urządzeniami; do Wake-on-LAN urządzenie wyłączone, ze włączonym WoL w BIOS/UEFI.

- [ ] **Historia:** pierwszy skan zapisuje stan wyjściowy (nic nie jest „NOWE”). Podłącz do sieci nowe urządzenie (telefon z Wi-Fi), skanuj ponownie: nowy host ma znacznik **NOWE**, filtr `nowe` pokazuje tylko takie; karta „Historia” wymienia skany (czas, zakres, liczba hostów, nowe). `scanhistory off` wyłącza zapisywanie.
- [ ] **Witryna jednym kliknięciem:** na hoście z portem 22 przycisk „Witryna SFTP” zapisuje witrynę z jego adresem (do uzupełnienia o użytkownika); to samo dla FTP.
- [ ] **Udział w panelu:** na hoście z udziałem „W panelu” otwiera udział w aktywnym panelu (**L:** przez `gio mount`, trzeba mieć `gvfs`; **W:** `\\host\udział`). Udział wymagający logowania daje zrozumiały komunikat z podpowiedzią, co zrobić.
- [ ] **Wake-on-LAN:** wyłącz urządzenie, w skanerze wybierz je (adres MAC musi być znany z wcześniejszego skanu) i „Obudź (Wake-on-LAN)": urządzenie się uruchamia. Bez znanego MAC przycisk mówi, czemu nie działa.
- [ ] Akcje z 4.13 nadal działają (HTTP, SSH, RDP, ulubione).

### 4.25 Udziały SMB jako katalog (L W) · **P1** `[Z]`

Potrzebny serwer SMB (NAS, Windows z udostępnionym katalogiem, Samba). **L:** `sudo apt install smbclient`.

- [ ] `go smb://serwer` (bez udziału): **L** z `smbclient` panel pokazuje listę udziałów (bez `C$`, `IPC$`, drukarek); wejście w udział pokazuje jego pliki **bez montowania** w systemie.
- [ ] `go smb://użytkownik@serwer/udział/podkatalog`: pyta o hasło (pole ukryte), zapisuje witrynę `użytkownik@serwer (smb)` (Ctrl+S ją pokazuje), otwiera podkatalog. Zły login daje czytelny komunikat, a nie surowy `NT_STATUS_…`. Logowanie jako gość: `go smb://serwer` bez użytkownika.
- [ ] Na udziale: F5 pobiera i wysyła pliki (także katalogi; nazwy z polskimi znakami i spacjami), F7 nowy katalog, F2 zmiana nazwy, F8 usuwa **trwale** (pytanie to mówi). Nazwa ze średnikiem lub cudzysłowem daje komunikat, że smbclient jej nie przyjmie.
- [ ] Duży plik: postęp rośnie w trakcie pobierania; „Anuluj” przerywa i nie zostawia `.bpart` pod prawdziwą nazwą.
- [ ] **Bez `smbclient`** (Linux: odinstaluj albo zmień nazwę programu): `go smb://serwer/udział` otwiera udział starą drogą (GVFS) albo mówi, co doinstalować. **W:** bez smbclienta działa ścieżka UNC (`\\serwer\udział`).

### 4.26 Chmury przez rclone (L W) · **P1** `[Z]`

Potrzebny zainstalowany `rclone` (rclone.org) i skonfigurowana chmura (`rclone config`: Google Drive, OneDrive, WebDAV lub inna).

- [ ] Bez rclone: `cloud` daje komunikat, co zainstalować (a nie błąd systemowy).
- [ ] `cloud` wymienia skonfigurowane chmury; `cloud nazwa` otwiera chmurę w panelu i zapisuje witrynę `nazwa (rclone)` (Ctrl+S); `go rclone://nazwa/katalog` otwiera od razu katalog. Nie pyta o hasło (loginy trzyma rclone).
- [ ] Wejście w katalogi, Backspace, F9. F5 pobiera i wysyła pliki (także katalogi; polskie znaki w nazwach); wysłany plik widać w chmurze (np. w przeglądarce); F7, F2 (nie nadpisuje istniejącego), F8 usuwa **trwale** i pytanie to mówi.
- [ ] Duży plik: pobieranie pokazuje rosnący postęp, „Anuluj” zatrzymuje proces `rclone` (sprawdź w menedżerze zadań, że nie został) i nie zostawia pół pliku pod prawdziwą nazwą.
- [ ] Nieistniejąca chmura (`cloud nie-ma`) daje komunikat z listą znanych chmur.

### 4.27 Samoodświeżanie list (L W) · **P1**

`[CI]` sprawdza reakcję silnika na zdarzenia systemu na obu systemach; tu oceniasz to w oknie i na prawdziwych zmianach.

- [ ] Otwórz w panelu `kod`. W terminalu: `touch nowy.txt` (**W:** `ni nowy.txt`): plik pojawia się na liście w ułamku sekundy, **bez F9**, a kursor zostaje na swoim pliku i zaznaczenie się nie gubi.
- [ ] Usuń plik w terminalu (`rm`, **W:** `del`): znika z listy. Zmień zawartość pliku (`echo x >> kod/skrypt.py`): zmienia się rozmiar i data.
- [ ] Pobierz coś większego do otwartego katalogu (przeglądarka, `curl`): lista nie „drga” i nie zwalnia okna.
- [ ] Zmiany w **drugim** panelu też są widoczne; zmiany w podkatalogu **nie** odświeżają listy nadrzędnej (tak ma być).
- [ ] `watch` pokazuje stan, `watch off` wyłącza (zmiana poza programem nie jest już widoczna bez F9), `watch on` włącza; ustawienie zostaje po restarcie.
- [ ] Katalog na dysku sieciowym lub montowaniu sieciowym: wpisz, czy odświeża się samo (może nie; wtedy F9).
- [ ] Okno bezczynne nie obciąża procesora zauważalnie (sprawdź w menedżerze zadań: poniżej ok. 1–2%).

### 4.28 Git: gałąź, status, akcje (L W) · **P1**

Potrzebny zainstalowany `git`. Dane: `repozytorium-git` (jeden commit i zmiany: M, D, ?, A).

- [ ] Wejdź do `repozytorium-git`, `columns git`: przy ścieżce `git: main`; `README.md` ma **M**, `stale.txt` **D**, `Zażółć nowy.txt` **?**, `src` (zawiera `dodany.rs`) **A**; w `src` tylko `dodany.rs` ma **A**.
- [ ] Zaznacz `README.md`, `git add`: litera zmienia się; `git unstage` cofa; `git commit "Zmiana opisu: zażółć"` zatwierdza **tylko** to, co było w poczekalni (pozostałe zmiany zostają); `git commit` bez niczego w poczekalni daje komunikat gita, a nie zawieszenie.
- [ ] `git diff` na pliku pod kursorem i na katalogu (puste miejsce): zmiany w podglądzie, Esc zamyka; `git log`: historia z commitami, w tym polskim opisem.
- [ ] Zmień plik w edytorze lub terminalu: bez F9 litera i gałąź aktualizują się (samoodświeżanie, 4.27).
- [ ] `git checkout -b inna` w terminalu: po chwili `git: inna`. W katalogu bez repozytorium nic się nie pokazuje (kolumna pusta, brak etykiety).
- [ ] **W:** nazwy z polskimi znakami i spacjami w statusie (`Zażółć nowy.txt`) pokazują się poprawnie; repozytorium na innym dysku (D:) też działa.
- [ ] Bez zainstalowanego `git` (zmień nazwę programu): komenda `git add` mówi, co zainstalować.

### 4.29 Indeks do szukania (L W) · **P1**

`[CI]` sprawdza wyniki, aktualizację i zapisane wyszukiwania na małych danych; tu liczy się czas i pamięć na prawdziwych danych.

- [ ] **Zgoda:** po starcie programu `index` mówi „Indeks jest pusty”, a w menedżerze zadań nic nie mieli: nic nie jest indeksowane samo.
- [ ] `index add here` w `bluepane-test` (to dużo plików i dwa duże binarne): budowanie idzie w tle, okno działa, `index` pokazuje przejrzane pliki, `index stop` przerywa (poprzedni indeks zostaje). Zapisz **czas budowania** i zużycie pamięci (menedżer zadań).
- [ ] `index find zolw` znajduje `umowa.txt` (słowo „żółw” w treści) i dokumenty z `dokumenty` (jeśli mają to słowo); wyniki wpadają do panelu jako lista, Backspace wraca. `index find dzie` (początek słowa „dzieło”) też. `index find name:zazolc` znajduje `Zażółć gęślą.txt`; `index find ext:pdf` pliki PDF; `index find text:mleko` `notatki.md`.
- [ ] **Treść dokumentów:** `index find` ze słowem z `dokumenty/raport.docx`, `raport.pdf` i `arkusz.xlsx` znajduje te pliki (zapisz, które słowa i które formaty zadziałały).
- [ ] **Kodowania:** `index find lodz` znajduje `stary-cp1250.txt` (Łódź w Windows-1250) i `Zażółć gęślą.txt`.
- [ ] `index update` po zmianie jednego pliku jest **szybki** i czyta tylko zmieniony (komunikat „przeczytano 1”); nowy plik pojawia się w wynikach, usunięty znika.
- [ ] **Zapisane wyszukiwania:** `index save Umowy umowa`, `index list`, `index open Umowy`: lista jak z `find`, ale to „katalog” pytany o indeks na nowo; po restarcie wyszukiwanie zostaje.
- [ ] Restart programu: `index` pokazuje wcześniej zbudowany indeks (plik `index.dat`), bez ponownego budowania. `index clear` go usuwa.

### 4.30 Podgląd (F3): kolory, Markdown, edytor hex, szczegóły (L W) · P2, edytor hex **P1**

`[CI]` sprawdza kolorowanie, renderowanie i zapis; tu oceniasz **wygląd i wygodę**.

**Kolory składni** (pliki w `kod`)

- [ ] `przyklad.rs`, `skrypt.py`, `dane.json`, `ustawienia.toml`, `zapytanie.sql`, `strona.html`: słowa kluczowe, napisy, liczby i komentarze mają różne kolory, w nagłówku jest nazwa języka (`tekst, Rust`). Komentarz blokowy `/* … */` przez kilka linii jest cały w kolorze komentarza także po przewinięciu.
- [ ] Kolory są czytelne w **obu** motywach (`theme light` i `theme dark`): nic nie ginie (np. żółty na białym).
- [ ] `C` wyłącza i włącza kolory; `Zażółć` w napisach i komentarzach nie psuje kolorów; poziome przewijanie (strzałki) nie przekłamuje kolorów.
- [ ] Plik bez rozpoznanego języka (`teksty/utf8-polskie.txt`) jest bez kolorów, a `C` mówi, że nie ma kolorowania.

**Markdown** (`kod/README.md`)

- [ ] Otwiera się wyrenderowany: nagłówek z podkreśleniem, **mocne**, *kursywa*, `kod`, link jako `tekst (adres)`, lista z kropkami i zagnieżdżona, lista numerowana, cytat z kreską, linia pozioma, blok kodu; tabela zostaje tak, jak napisana. W nagłówku `[Markdown]` i `wiersz N z M`.
- [ ] Zmień szerokość okna: akapity zawijają się do szerokości (bez ucinania słów), kolory są zachowane. `M` przełącza na zwykły tekst i z powrotem; `/` (szukanie) przełącza na tekst.
- [ ] Plik `.md` ponad 4 MB otwiera się jako tekst (opcjonalnie: sprawdź większym plikiem).

**Edytor hex** (`hex/maly.bin`; używaj kopii, gdy chcesz ćwiczyć na własnych plikach)

- [ ] `E` włącza edycję: żółty kursor na bajcie; strzałki, PgUp/PgDn go przesuwają, widok za nim podąża; cyfry `0-9 a-f` (dwie = bajt) zmieniają bajt, kursor idzie dalej, zmienione bajty są podświetlone **w kolumnie hex i w kolumnie znaków**. Plik na dysku **nie zmienia się** (sprawdź w drugim programie).
- [ ] `Ctrl+Z` odrzuca zmiany; `Esc` kończy edycję; zamknięcie (`Esc`/`Q`) z niezapisanymi zmianami **ostrzega** (pasek stanu), dopiero drugie zamyka.
- [ ] `Ctrl+S`: powstaje `maly.bin.bpedit` i pasek pyta; `Esc` usuwa kopię (zmiany zostają w edytorze). Ponownie `Ctrl+S`, **Enter**: plik zostaje podmieniony, a oryginał jest w `maly.bin.bak`; widok pokazuje nową zawartość. Kolejny zapis daje `maly.bin.bak2` (pierwsza kopia nie jest nadpisana). **W:** podmiana działa mimo otwartego pliku w podglądzie i bez błędu o zajętym pliku.
- [ ] `viewer poke 0x10 ff 00` ustawia bajty z wiersza poleceń; poza końcem pliku daje błąd. Edycja z F3 na pliku **z archiwum** albo **dokumentu** jest odrzucona (tylko do odczytu).

**Szczegóły pliku**

- [ ] Kursor na `media/zdjecie-exif.jpg`, `details` (albo `I` w podglądzie): rozmiar, daty, oraz pola EXIF: aparat **Aparat Żółć**, model `Model gęślą 5`, data 2025:07:14 18:30:05 (polskie znaki bez ucieczek). Na `zdjecie-bez-exif.jpg` komunikat o braku danych EXIF.
- [ ] Na `media/utwor-tagi.wav`: czas trwania 0:00:01, 8000 Hz, wykonawca `Zespół Żółć`, tytuł `Gęślą jaźń`, album `Płyta testowa`, gatunek, ścieżka 7, rok 1999. Na katalogu: komunikat „to katalog”.

### 4.31 Terminal tutaj i polecenie z wynikiem (L W) · P2

`[CI]` sprawdza uruchamianie programu, przechwycenie wyjścia, kod wyjścia, przerwanie i znaczniki; **nie** otwiera prawdziwego terminala.

- [ ] **Terminal:** w `~/bluepane-test/nazwy` komenda `terminal` (albo Ctrl+Shift+T): otwiera się terminal systemu, a jego bieżący katalog to katalog aktywnego panelu (`pwd` / `cd`). **L:** sprawdź też z ustawioną zmienną `TERMINAL`. **W:** Windows Terminal albo zwykła konsola.
- [ ] **Wynik:** `!ls` (**W:** `!cmd /c dir`): w podglądzie lista plików katalogu panelu, na górze `$ …`, na dole `kod wyjścia: 0`; Esc zamyka.
- [ ] **Znaczniki i polskie znaki:** kursor na `Zażółć gęślą.txt`, `!echo %n` (**L:** `!printf "%s\n" %f`): wyjście ma pełną nazwę z polskimi literami, bez zniekształceń. W **W** sprawdź to samo z `!cmd /c echo %n` (strona kodowa konsoli może zniekształcić litery: zapisz, jak wygląda).
- [ ] **Błąd i kod wyjścia:** `!ls nie-ma-takiego` (**W:** `!cmd /c dir nie-ma-takiego`): w podglądzie sekcja „błędy (stderr)” i niezerowy kod wyjścia; pasek stanu „Kod wyjścia …”. Nieistniejący program (`!nie-ma-programu`) daje czytelny komunikat, bez podglądu.
- [ ] **Przerwanie:** `!sleep 60` (**W:** `!ping -n 60 127.0.0.1`): okno nie zamarza, pasek stanu mówi „Działa: …”; druga komenda `!ls` w tym czasie odmawia, `runstop` kończy z „Przerwano”.
- [ ] **Archiwum i serwer:** w archiwum zip albo na serwerze `terminal` i `!ls` odmawiają czytelnym komunikatem (działają tylko na dysku).

### 4.32 Instalacja z gotowych paczek na czystej maszynie (L W) · **P1**

`[CI]` sprawdza na czystych runnerach: instalację cichą instalatora Windows i jej odinstalowanie, instalację `.deb` przez `apt` z usunięciem, `--version` z AppImage. **Nie** sprawdza wyglądu menu, ikony, SmartScreena ani pierwszego uruchomienia okna. Najlepiej na świeżej maszynie wirtualnej (Windows 11, Ubuntu LTS) bez Rusta i bez bluepane. Paczki: Releases, a przed pierwszym wydaniem „Run workflow” w „Wydanie” i artefakty `paczka-linux` i `paczka-windows`.

- [ ] **Pobranie i suma:** pobierz plik i `SHA256SUMS.txt`, sprawdź sumę (README, „Instalacja”). Zgadza się.
- [ ] **W: SmartScreen:** instalator pokazuje ostrzeżenie o nieznanym wydawcy; „Więcej informacji” > „Uruchom mimo to” przechodzi dalej. Zapisz, jak wygląda komunikat.
- [ ] **W: instalator:** polski interfejs, pokazana licencja, domyślnie bez pytania o administratora; opcja skrótu na pulpicie. Po zakończeniu „Uruchom bluepane” otwiera okno.
- [ ] **W: skrót i ikona:** skrót w menu Start ma ikonę (dwa niebieskie panele); sam `bluepane.exe` w Eksploratorze ma ikonę **domyślną** (znane ograniczenie). Okno ma własną ikonę na pasku zadań.
- [ ] **W: instalacja w katalogu z polskimi znakami** (np. `C:\Users\Zażółć\bluepane`): program startuje, `about` pokazuje wersję 0.1.0.
- [ ] **W: odinstalowanie:** „Aplikacje” > bluepane > Odinstaluj usuwa program i skróty; `%APPDATA%\bluepane` zostaje.
- [ ] **W: zip przenośny:** rozpakowany gdziekolwiek uruchamia się; `bluepane-cli.exe --version` działa w terminalu.
- [ ] **L: `.deb`:** `sudo apt install ./bluepane-…-amd64.deb` dociąga zależności bez błędów; bluepane jest w menu aplikacji z ikoną; uruchomienie z menu otwiera okno; `which bluepane` wskazuje `/usr/bin/bluepane`.
- [ ] **L: usunięcie:** `sudo apt remove bluepane` usuwa program i wpis w menu; `~/.config/bluepane` zostaje.
- [ ] **L: AppImage:** `chmod +x` i dwuklik albo uruchomienie z terminala otwiera okno (na dystrybucji innej niż Ubuntu, jeśli jest); `--version` w terminalu wypisuje wersję. Brak biblioteki daje komunikat systemowy: zapisz jej nazwę.
- [ ] **L: tar.gz:** rozpakowany plik `bluepane` uruchamia się.
- [ ] **Pierwsze uruchomienie:** bez argumentów i bez ustawień okno otwiera się w katalogu domowym lub bieżącym, dwa panele, polskie nazwy czytelne; `F1` i `Ctrl+K` działają. Opcjonalne programy (`git`, `smbclient`, `rclone`) nie są wymagane: ich brak daje czytelny komunikat tylko przy użyciu funkcji.

## 5. Wyniki

Wypełnij po teście (skopiuj wiersze do wiadomości, jeśli wolisz).

| Obszar | Linux | Windows | Uwagi / numery błędów |
| --- | --- | --- | --- |
| 4.0 Dym |  |  | |
| 4.1 Start i wygląd |  |  | |
| 4.2 Nawigacja i zaznaczanie |  |  | |
| 4.3 Operacje na plikach |  |  | |
| 4.4 Kolejka i limit |  |  | |
| 4.5 Archiwa |  |  | |
| 4.6 Szukanie |  |  | |
| 4.7 Zmiana nazw |  |  | |
| 4.8 Synchronizacja |  |  | |
| 4.9 Podgląd F3 i strony |  |  | |
| 4.10 Quick View |  |  | |
| 4.11 Schowek |  |  | |
| 4.12 Dok i układ |  |  | |
| 4.13 Skaner sieci |  |  | |
| 4.14 Serwery |  |  | |
| 4.15 Tylko Windows | — |  | |
| 4.16 Tylko Linux |  | — | |
| 4.17 Paleta, Skocz do, Ctrl+K, Miejsca, omnibox |  |  | |
| 4.18 Menu, przeciąganie, F2 w wierszu, powiadomienia |  |  | |
| 4.19 Wygląd, kolumny, filtr, siatka |  |  | |
| 4.20 Obszary robocze i sesja |  |  | |
| **4.21 Makra, przyciski, harmonogram (P1)** |  |  | |
| 4.22 Mapa, duplikaty, porównanie, sumy |  |  | |
| 4.23 Zmiana nazw z EXIF i tagów |  |  | |
| **4.24 Skaner: WoL, udział, historia (P1, Z)** |  |  | |
| **4.25 SMB (P1, Z)** |  |  | |
| **4.26 rclone (P1, Z)** |  |  | |
| **4.27 Samoodświeżanie list (P1)** |  |  | |
| **4.28 Git (P1)** |  |  | |
| **4.29 Indeks (P1)** |  |  | |
| **4.30 Podgląd: kolory, Markdown, hex, szczegóły (P1 hex)** |  |  | |
| **4.31 Terminal tutaj i polecenie z wynikiem** |  |  | |
| **4.32 Instalacja z paczek na czystej maszynie (P1)** |  |  | |

Wpisuj `OK`, `część` (z `→` co nie działa), `brak` (nie sprawdzano, np. nie ma sprzętu) albo numer błędu. Wyniki możesz
wysyłać **partiami**, po jednej sekcji.

Skróty, które chcesz zmienić, i pomysły zapisuj osobno (lista „na później"): skróty są w
`crates/engine/src/keymap.rs` i da się je zmienić bez przebudowy logiki.
