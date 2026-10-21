# Historia zmian

Format luźno wzorowany na [Keep a Changelog](https://keepachangelog.com/pl/). Wersje numerujemy
`0.MINOR.PATCH`: dopóki program jest przed 1.0, nowa funkcja podnosi środkową liczbę, poprawka
ostatnią. Numer jest w jednym miejscu: `version` w `[workspace.package]` w `Cargo.toml`.

## [Niewydane]

## [0.1.0]: pierwsze wydanie

Pierwsza wersja do instalacji z gotowych paczek (Windows i Linux, bez podpisu kodu).

### Panele i pliki
- Dwa panele w stylu Total Commandera, nawigacja i zaznaczanie z klawiatury i myszą, zakładki,
  pasek miejsc, „Skocz do” (Ctrl+P), pole Ctrl+K, paleta komend (F1), skróty konfigurowalne.
- Operacje F5–F8 w tle z postępem, kolejką i limitem prędkości, bez nadpisywania bez pytania,
  usuwanie do kosza.
- Archiwa zip, 7z i tar jak katalogi (czytanie, pakowanie, zmiany), zmiana nazw wielu plików
  z podglądem (także z EXIF i tagów), szukanie po nazwie i treści (także w archiwach i PDF,
  Word, Excel), indeks do natychmiastowego szukania, duplikaty, porównanie plików i katalogów,
  synchronizacja z podglądem planu, sumy kontrolne, mapa zajętości dysku.
- Samoodświeżanie list, kolumna i gałąź git z podstawowymi akcjami.

### Sieć
- SFTP, FTP, FTPS, udziały SMB, chmury przez rclone; witryny z hasłami w systemowym magazynie
  albo Bitwarden. Skaner sieci (hosty, porty, usługi, Wake-on-LAN).

### Podgląd i narzędzia
- Podgląd F3: tekst z kolorowaniem składni, renderowany Markdown, hex z edytorem (zapis przez
  kopię), dokumenty biurowe i PDF, szczegóły plików.
- Makra, własne przyciski, harmonogram (crontab i Harmonogram zadań), profile synchronizacji.
- Terminal systemu w katalogu panelu (Ctrl+Shift+T) i polecenia z wynikiem (`!polecenie`).
- Okno „O programie” (`about`) i `bluepane --version`.

### Znane ograniczenia
- Brak podpisu kodu: Windows SmartScreen może ostrzegać o nieznanym wydawcy.
- Program nie sprawdza aktualizacji sam; nowe wersje są w Releases na GitHubie.
- Prawdziwy rclone, SMB, Wake-on-LAN i harmonogram systemu zostały sprawdzone tylko testami
  automatycznymi i na fałszywych usługach; zgłoszenia z prawdziwych środowisk trafią do etapu T.
