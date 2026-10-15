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

Write-Host ''
Write-Host "Gotowe: $Dest"
Write-Host "Uruchom:  bluepane.exe `"$Dest`" `"$Dest\synchronizacja`""
