# Pomiary wydajności (CR-BP-OPT, OPT-00)

Powtarzalne pomiary stanu bazowego przed zmianami z CR-BP-OPT. Nie są częścią CI (wyniki zależą od
maszyny), wszystkie testy mają `#[ignore]` i nic nie sprawdzają asercją. Dane powstają wyłącznie w
nowym katalogu tymczasowym i znikają razem z nim; nic nie skanuje prawdziwych dysków.

## Jak uruchomić

```
cargo test -p bluepane-engine --release --test bench -- --ignored --nocapture --test-threads=1
```

Bez `--release` liczby nie mają sensu (program ostrzega w nagłówku). Zmienne środowiskowe:

| Zmienna | Znaczenie | Domyślnie |
|---|---|---|
| `BLUEPANE_BENCH_RUNS` | liczba mierzonych powtórzeń | 7 |
| `BLUEPANE_BENCH_LARGE_MIB` | rozmiar dużego pliku (MiB) | 256 (SFTP: najwyżej 64) |
| `BLUEPANE_BENCH_DIR` | katalog nadrzędny dla danych (żeby zmierzyć konkretny dysk) | katalog tymczasowy systemu |
| `BLUEPANE_BENCH_NOTE` | opis maszyny i nośnika do nagłówka | brak |

Metoda: jedno uruchomienie `first`, jedno rozgrzewkowe (pomijane), potem N pomiarów; podawane są
mediana, p95 (najbliższa ranga; przy n=7 równe maksimum), min i max. Dane wejściowe są budowane
przed każdym pomiarem i nie wchodzą do czasu.

**Zimny cache.** Test nie może wyczyścić pamięci podręcznej systemu (wymaga administratora).
`first` to pierwsze uruchomienie po wygenerowaniu danych, nie gwarantowanie zimny cache. Prawdziwy
zimny pomiar: operator uruchamia na Linuksie `sync; echo 3 | sudo tee /proc/sys/vm/drop_caches`
tuż przed testem (nie robi tego program). Na Windows: nie zmierzono.

## Scenariusze

Listowanie `vfs::list_dir`, `Panel::new`, `Panel::reload`, `Panel::set_sort` (nazwa, rozmiar, data),
`Panel::set_filter` dla 10 tys. i 100 tys. plików (polskie nazwy), przeładowanie dwóch paneli po
kolei (to robi `Session::finish_job` po każdym zadaniu), kopiowanie 10 tys. plików po 1 KiB i
jednego dużego pliku (`ops::copy`), SFTP przez pętlę zwrotną (serwer w procesie testu): wysłanie i
pobranie dużego pliku, wysłanie 500 plików po 1 KiB.

Nie ma: opóźnienia ani utraty pakietów w SFTP (serwer testowy ich nie symuluje), SMB, FTP,
przerwania transferu w trakcie pomiaru (to scenariusz testów poprawności z OPT-01), czasu klatki
GUI (wymaga instrumentacji, do OPT-02). Pamięć: `peak_rss` i `rss` z `/proc/self/status` (tylko
Linux; na Windows `n/a`), liczone dla całego procesu testowego, więc szczyt nie jest przypisany do
jednego scenariusza.

## Stan bazowy: 2026-11-03, commit `0042f5b` (main)

Warunki: Intel Celeron J4105 1,5 GHz, 7,6 GiB RAM, dysk WD Blue SA510 M.2 SATA SSD, ext4
(`/dev/sda2`), dane w `/tmp` (ten sam system plików), Linux 7.0.0-42, rustc 1.99.0, `--release`,
7 powtórzeń po rozgrzewce, pojedynczy wątek testów, w tym czasie mogły działać inne procesy
(np. inna sesja programistyczna), więc odchylenia w kopiowaniu dużych plików są realne.

| Scenariusz | first (ms) | mediana (ms) | p95 (ms) | min–max (ms) |
|---|---|---|---|---|
| `vfs::list_dir`, 10 tys. | 88,7 | 84,4 | 84,6 | 83,7–84,6 |
| `Panel::new`, 10 tys. | 97,3 | 92,1 | 94,5 | 91,1–94,5 |
| `Panel::reload`, 10 tys. | 105,9 | 99,9 | 105,7 | 96,9–105,7 |
| `set_sort` nazwa / rozmiar / data, 10 tys. | 9,4 / 8,1 / 16,3 | 9,5 / 8,0 / 16,6 | 12,8 / 8,3 / 16,8 | |
| `set_filter`, 10 tys. | 7,7 | 7,5 | 7,5 | 7,3–7,5 |
| przeładowanie 2 paneli, 10 tys. każdy | 201,6 | 195,2 | 202,4 | 193,1–202,4 |
| `vfs::list_dir`, 100 tys. | 954,8 | 989,4 | 997,2 | 987,8–997,2 |
| `Panel::new`, 100 tys. | 1107,9 | 1063,3 | 1073,0 | 1061,2–1073,0 |
| `Panel::reload`, 100 tys. | 1212,5 | 1149,4 | 1159,6 | 1142,6–1159,6 |
| `set_sort` nazwa / rozmiar / data, 100 tys. | 115,5 / 101,9 / 191,3 | 112,5 / 102,0 / 190,9 | 138,4 / 103,2 / 192,2 | |
| `set_filter`, 100 tys. | 86,6 | 86,3 | 93,2 | 84,9–93,2 |
| przeładowanie 2 paneli, 100 tys. każdy | 2361,5 | 2298,9 | 2327,8 | 2274,6–2327,8 |
| `ops::copy`, 10 tys. × 1 KiB | 961,2 | 919,6 | 921,4 | 914,6–921,4 |
| `ops::copy`, 1 plik 256 MiB | 333,2 | 543,8 | 895,9 | 189,7–895,9 |
| SFTP (pętla zwrotna) wysłanie 64 MiB | 609,6 | 912,1 | 1177,5 | 643,6–1177,5 |
| SFTP (pętla zwrotna) pobranie 64 MiB | 465,6 | 462,4 | 521,3 | 426,3–521,3 |
| SFTP (pętla zwrotna) wysłanie 500 × 1 KiB | 1429,3 | 1177,3 | 1679,5 | 927,8–1679,5 |

Pamięć po całym przebiegu: proces testowy ok. 170 MiB RSS po scenariuszach z dwoma panelami po
100 tys. wpisów (wartości dla każdego scenariusza są w surowym wyjściu, nie przypisane do jednego).

**Wnioski z liczb (fakty, nie oceny):** listowanie 100 tys. plików zajmuje ok. 1 s, a przeładowanie
obu paneli po zadaniu ok. 2,3 s, w wątku interfejsu; `Panel::new` kosztuje o ok. 74 ms więcej niż samo
`vfs::list_dir` (sortowanie w `Panel::sort_listing` i przebudowa widoku, czyli ok. 7% z 1063 ms), więc
podwójne sortowanie nie jest główną częścią kosztu; resztę stanowi czytanie katalogu i metadane
na wpis (to hipoteza do potwierdzenia pomiarem w OPT-02, nie wniosek). Wyniki kopiowania dużego pliku
mają duży rozrzut (pamięć podręczna systemu i zapis w tle), więc nie nadają się jako próg
regresji bez powtórzenia na spokojnej maszynie.

## Do uzupełnienia ręcznie ([MANUAL])

- Pomiar na Windows (NTFS) i na wolnym nośniku (HDD / USB), z zimnym cache.
- SFTP z opóźnieniem sieciowym i prawdziwy serwer testowy (nie produkcyjny NAS), SMB.

## Kryteria porównań dla kolejnych etapów

Pogorszenie mediany o więcej niż 10% w tych samych warunkach (ta sama maszyna, `--release`, ≥5
powtórzeń) wymaga uzasadnienia i zgody właściciela. Dla scenariuszy z rozrzutem większym niż 10%
(kopiowanie dużych plików) porównuj min oraz powtórz na spokojnej maszynie.
