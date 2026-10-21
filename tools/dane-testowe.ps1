# Przygotowuje katalog do testów ręcznych bluepane (Windows, PowerShell).
# Użycie:  powershell -ExecutionPolicy Bypass -File tools\dane-testowe.ps1 [katalog docelowy]
# (domyślnie %USERPROFILE%\bluepane-test)
param([string]$Dest = (Join-Path $env:USERPROFILE 'bluepane-test'))
$ErrorActionPreference = 'Stop'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$src = Join-Path $here '..\testdata'
if (-not (Test-Path $src)) { $src = Join-Path $here 'testdata' }   # wersja rozpakowana z artefaktu

if ((Test-Path $Dest) -and (Get-ChildItem $Dest -Force | Select-Object -First 1)) {
    Write-Error "Katalog $Dest już istnieje i nie jest pusty. Podaj inny albo usuń go ręcznie."
}
New-Item -ItemType Directory -Force -Path $Dest | Out-Null
Copy-Item -Path (Join-Path $src '*') -Destination $Dest -Recurse -Force

function Put($path, $text) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
    [System.IO.File]::WriteAllText($path, $text, (New-Object System.Text.UTF8Encoding($false)))
}
function Stamp($path, $when) { (Get-Item $path).LastWriteTime = [datetime]$when }

# --- nazwy: do zmiany nazw i sortowania ---
1..12 | ForEach-Object { Put (Join-Path $Dest ("nazwy\IMG_{0:D4}.JPG" -f $_)) "zdjecie $_" }
Put (Join-Path $Dest 'nazwy\Zażółć gęślą.txt') 'x'
Put (Join-Path $Dest 'nazwy\plik ze spacjami.txt') 'x'
Put (Join-Path $Dest 'nazwy\WIELKIE.TXT') 'x'
Put (Join-Path $Dest 'nazwy\a.txt') 'x'
Put (Join-Path $Dest 'nazwy\b.txt') 'x'

# --- synchronizacja ---
$L = Join-Path $Dest 'synchronizacja\lewy'; $R = Join-Path $Dest 'synchronizacja\prawy'
Put "$L\takie-same.txt" 'to samo';            Put "$R\takie-same.txt" 'to samo'
Stamp "$L\takie-same.txt" '2026-10-12 08:00'; Stamp "$R\takie-same.txt" '2026-10-12 08:00'
Put "$L\tylko-lewy.txt" 'tylko po lewej'
Put "$R\tylko-prawy.txt" 'tylko po prawej'
Put "$L\nowszy-w-lewym.txt" 'nowsza wersja';   Put "$R\nowszy-w-lewym.txt" 'stara wersja'
Stamp "$L\nowszy-w-lewym.txt" '2026-10-14 12:00'; Stamp "$R\nowszy-w-lewym.txt" '2026-10-10 12:00'
Put "$L\nowszy-w-prawym.txt" 'stara wersja';  Put "$R\nowszy-w-prawym.txt" 'nowsza wersja'
Stamp "$L\nowszy-w-prawym.txt" '2026-10-10 12:00'; Stamp "$R\nowszy-w-prawym.txt" '2026-10-14 12:00'
Put "$L\inny-rozmiar.txt" 'krótki';            Put "$R\inny-rozmiar.txt" 'znacznie dłuższa zawartość tego pliku'
Put "$L\podkatalog\a.txt" 'w podkatalogu';    Put "$R\podkatalog\a.txt" 'w podkatalogu, inaczej'
Stamp "$L\podkatalog\a.txt" '2026-10-12 08:00'; Stamp "$R\podkatalog\a.txt" '2026-10-12 08:00'

# --- dużo plików i jeden duży ---
1..1500 | ForEach-Object { Put (Join-Path $Dest ("duzo-plikow\plik-{0:D4}.txt" -f $_)) "plik $_" }
$big = Join-Path $Dest 'duze'
New-Item -ItemType Directory -Force -Path $big | Out-Null
function RandomFile($path, $megabytes) {
    Write-Host "Tworzę $path ($megabytes MB)..."
    $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    $buffer = New-Object byte[] (1MB)
    $stream = [System.IO.File]::Create($path)
    try { 1..$megabytes | ForEach-Object { $rng.GetBytes($buffer); $stream.Write($buffer, 0, $buffer.Length) } }
    finally { $stream.Dispose() }
}
RandomFile (Join-Path $big 'duzy-300MB.bin') 300
RandomFile (Join-Path $big 'sredni-40MB.bin') 40

# --- inne przypadki ---
$misc = Join-Path $Dest 'rozne'
Put "$misc\ukryty.txt" 'ukryty'
(Get-Item "$misc\ukryty.txt").Attributes = 'Hidden'
Put "$misc\tylko-do-odczytu.txt" 'tylko do odczytu'
(Get-Item "$misc\tylko-do-odczytu.txt").IsReadOnly = $true
Put "$misc\cel.txt" 'cel dowiązania'
try { New-Item -ItemType SymbolicLink -Path "$misc\dowiazanie.txt" -Target "$misc\cel.txt" | Out-Null }
catch { Write-Host 'Dowiązanie symboliczne pominięte (wymaga uprawnień administratora lub trybu deweloperskiego).' }
New-Item -ItemType Directory -Force -Path "$misc\pusty-katalog" | Out-Null

# --- bardzo długa ścieżka (powyżej 260 znaków) ---
$deep = $Dest
1..12 | ForEach-Object { $deep = Join-Path $deep ('katalog-o-dlugiej-nazwie-numer-{0:D2}' -f $_) }
if ($deep.Length -gt 270) {
    $long = '\\?\' + $deep
    New-Item -ItemType Directory -Force -Path $long | Out-Null
    [System.IO.File]::WriteAllText("$long\plik-w-bardzo-dlugiej-sciezce.txt", 'ok')
}

# --- kod i Markdown: kolorowanie składni, renderowany Markdown (F3) ---
$kod = Join-Path $Dest 'kod'
Put "$kod\przyklad.rs" "//! Przykład do kolorowania składni.`n/* komentarz blokowy`n   przez kilka linii */`nfn main() {`n    let tekst = `"Zażółć gęślą jaźń`"; // komentarz`n    let liczba = 42;`n    println!(`"{tekst} {liczba}`");`n}`n"
Put "$kod\skrypt.py" "# komentarz`ndef powitanie(imie):`n    if imie:`n        return `"Cześć, `" + imie  # zażółć`n    return None`n"
Put "$kod\dane.json" "{`"nazwa`": `"Zażółć`", `"liczby`": [1, 2.5], `"aktywny`": true, `"brak`": null}`n"
Put "$kod\ustawienia.toml" "# ustawienia`n[sekcja]`nklucz = `"wartość`"`nliczba = 7`nflaga = true`n"
Put "$kod\zapytanie.sql" "-- zapytanie`nSELECT imie, COUNT(*) FROM osoby WHERE wiek > 18 GROUP BY imie;`n"
Put "$kod\strona.html" "<!-- strona -->`n<html><body><p class=`"a`">Zażółć gęślą</p></body></html>`n"
$readme = @'
# Nagłówek pierwszego poziomu

Akapit ze **zdaniem mocnym**, *kursywą*, `kodem w tekście` i [odnośnikiem](https://example.org).
Drugi wiersz tego samego akapitu zawija się do szerokości okna, jeśli jest dostatecznie długi, żeby sprawdzić zawijanie.

## Lista

- pierwszy punkt
- drugi punkt z **mocnym** słowem
  - zagnieżdżony punkt

1. numer jeden
2. numer dwa

> Cytat o zażółć gęślą jaźń
> w dwóch liniach.

---

```rust
let x = 1;
```

| tabela | zostaje | tak jak napisano |
| --- | --- | --- |
'@
Put "$kod\README.md" $readme

# --- hex: edytor hex w podglądzie ---
New-Item -ItemType Directory -Force -Path (Join-Path $Dest 'hex') | Out-Null
[System.IO.File]::WriteAllBytes((Join-Path $Dest 'hex\maly.bin'), [byte[]](0..255))
Put (Join-Path $Dest 'hex\tekst.txt') "zwykły tekst do przełączenia na hex`n"

# --- duplikaty: te same treści w różnych katalogach ---
$dup = Join-Path $Dest 'duplikaty'
Put "$dup\a\zdjecie.txt" "ta sama treść pliku, powtórzona trzy razy`n"
Put "$dup\b\zdjecie-kopia.txt" "ta sama treść pliku, powtórzona trzy razy`n"
Put "$dup\c\Zażółć kopia.txt" "ta sama treść pliku, powtórzona trzy razy`n"
Stamp "$dup\a\zdjecie.txt" '2026-01-10 10:00'
Stamp "$dup\b\zdjecie-kopia.txt" '2026-05-20 10:00'
Stamp "$dup\c\Zażółć kopia.txt" '2026-09-30 10:00'
Put "$dup\a\unikalny.txt" "unikalny plik`n"
Put "$dup\a\ten-sam-rozmiar.txt" "ABCDEFGHIJKLMNOPQRSTUVWXYZ-1`n"
Put "$dup\b\ten-sam-rozmiar.txt" "ABCDEFGHIJKLMNOPQRSTUVWXYZ-2`n"

# --- porównanie plików i sumy kontrolne ---
$cmp = Join-Path $Dest 'porownanie'
Put "$cmp\lewy.txt" "pierwszy wiersz`ndrugi wiersz`ntrzeci wiersz zażółć`nczwarty wiersz`npiąty wiersz`n"
Put "$cmp\prawy.txt" "pierwszy wiersz`ndrugi wiersz zmieniony`ntrzeci wiersz zażółć`nnowy wiersz`nczwarty wiersz`npiąty wiersz`n"
[System.IO.File]::WriteAllBytes("$cmp\bin1.bin", (New-Object byte[] 4096))
$bin2 = New-Object byte[] 4096; $bin2[2000] = 255
[System.IO.File]::WriteAllBytes("$cmp\bin2.bin", $bin2)

# --- indeks do szukania: teksty z polskimi znakami i różne kodowania ---
$idx = Join-Path $Dest 'indeks'
Put "$idx\umowa.txt" "Umowa o dzieło: żółw, gęś i jaźń. Termin płatności: 30 dni.`n"
Put "$idx\notatki.md" "# Notatki`n`nZakupy: mleko, chleb, masło.`n"
Put "$idx\głęboko\Zażółć gęślą.txt" "Plik w głębokim katalogu o słowie wyjątkowe.`n"
[System.IO.File]::WriteAllBytes("$idx\stary-cp1250.txt", [byte[]](0xA3, 0xF3, 0x64, 0x9F, 0x20, 0x6E, 0x6F, 0x63, 0xB9, 0x0A))

# --- zdjęcia i utwory: zmiana nazw z EXIF i tagów (kopie z media) ---
$media = Join-Path $Dest 'media'
New-Item -ItemType Directory -Force -Path (Join-Path $Dest 'zdjecia'), (Join-Path $Dest 'muzyka') | Out-Null
Copy-Item "$media\zdjecie-exif.jpg" (Join-Path $Dest 'zdjecia\IMG_0001.jpg')
Copy-Item "$media\zdjecie-exif-2.jpg" (Join-Path $Dest 'zdjecia\IMG_0002.jpg')
Copy-Item "$media\zdjecie-bez-exif.jpg" (Join-Path $Dest 'zdjecia\IMG_0003.jpg')
Copy-Item "$media\utwor-tagi.wav" (Join-Path $Dest 'muzyka\01.wav')
Copy-Item "$media\utwor-2.wav" (Join-Path $Dest 'muzyka\02.wav')

# --- repozytorium git: kolumna git, gałąź, git add/commit/diff/log ---
if (Get-Command git -ErrorAction SilentlyContinue) {
    $g = Join-Path $Dest 'repozytorium-git'
    New-Item -ItemType Directory -Force -Path "$g\src" | Out-Null
    git -C $g init -q -b main
    git -C $g config user.email 'test@example.com'
    git -C $g config user.name 'Tester'
    git -C $g config commit.gpgsign false
    Put "$g\README.md" "Projekt testowy`n"
    Put "$g\src\main.rs" "fn main() { println!(`"cześć`"); }`n"
    Put "$g\stale.txt" "stały plik`n"
    git -C $g add .
    git -C $g commit -q -m 'Pierwszy commit'
    [System.IO.File]::AppendAllText("$g\README.md", "dopisek`n")      # zmieniony (M)
    Put "$g\Zażółć nowy.txt" "nowy plik`n"                           # nieśledzony (?)
    Remove-Item "$g\stale.txt"                                        # usunięty (D)
    Put "$g\src\dodany.rs" "kolejny`n"                               # dodany (A)
    git -C $g add "$g\src\dodany.rs"
} else {
    Write-Host 'Brak programu git: pomijam repozytorium-git (testy gita pominąć).'
}

Write-Host ''
Write-Host "Gotowe: $Dest"
Write-Host "Uruchom:  bluepane.exe `"$Dest`" `"$Dest\synchronizacja`""
