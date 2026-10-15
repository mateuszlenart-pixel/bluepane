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
   - Linux: `bluepane-linux` (w środku `bluepane-linux.tar.gz`),
   - Windows: `bluepane-windows` (w środku `bluepane-windows.zip`).
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

```
Expand-Archive bluepane-windows.zip -DestinationPath .
cd bluepane
.\bluepane.exe             # albo:  .\bluepane.exe C:\Users\ja\a D:\
```

Windows może pokazać ekran „System Windows chronił ten komputer" (SmartScreen), bo plik nie
jest podpisany: *Więcej informacji* → *Uruchom mimo to*. Za oknem programu otwiera się
też okno konsoli: **zostaw je**, tam trafiają komunikaty o błędach.

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
`duze`, `rozne`.

Przykładowe uruchomienie: `bluepane ~/bluepane-test ~/bluepane-test/synchronizacja`.

## 3. Jak zgłaszać błędy

Dla każdego problemu zapisz: system i wersję (Linux Mint 22 / Windows 11 itd.), zawartość
`WERSJA.txt`, numer punktu z listy poniżej, **co kliknięto lub wpisano**, co się stało, co
miało się stać, zrzut ekranu i komunikat z terminala/konsoli. Jeśli program się zamknął,
najważniejszy jest ostatni komunikat w terminalu (Linux) albo w oknie konsoli (Windows).
Wygląd (czytelność, kolory, rozmiary) też zgłaszaj, nawet jeśli „działa".

## 4. Lista kontrolna

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
- [ ] Komenda `theme` w wierszu poleceń pokazuje motyw `dark`; `theme jasny` daje czytelny
      błąd po polsku.
- [ ] **W:** czy okno wygląda dobrze przy skalowaniu ekranu 125% i 150%?

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

- [ ] Okno konsoli za programem nie przeszkadza; zamknięcie okna programu kończy proces.
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

## 5. Wyniki

Wypełnij po teście (skopiuj wiersze do wiadomości, jeśli wolisz).

| Obszar | Linux | Windows | Uwagi / numery błędów |
| --- | --- | --- | --- |
| 4.1 Start i wygląd | | | |
| 4.2 Nawigacja i zaznaczanie | | | |
| 4.3 Operacje na plikach | | | |
| 4.4 Kolejka i limit | | | |
| 4.5 Archiwa | | | |
| 4.6 Szukanie | | | |
| 4.7 Zmiana nazw | | | |
| 4.8 Synchronizacja | | | |
| 4.9 Podgląd F3 i strony | | | |
| 4.10 Quick View | | | |
| 4.11 Schowek | | | |
| 4.12 Dok i układ | | | |
| 4.13 Skaner sieci | | | |
| 4.14 Serwery | | | |
| 4.15 Tylko Windows | — | | |
| 4.16 Tylko Linux | | — | |

Skróty, które chcesz zmienić, i pomysły zapisuj osobno (lista „na później"): skróty są w
`crates/engine/src/keymap.rs` i da się je zmienić bez przebudowy logiki.
