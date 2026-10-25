# Kanoniczny Stan Przekazania Sesji (Session Handoff)

> **INSTRUKCJA DLA AGENTA:** Zastąp wszystkie znaczniki `<WYPEŁNIJ:...>` danymi z bieżącej sesji. Dla pustych list wpisz jawnie `Brak` lub `Nie dotyczy`; nie wymyślaj faktów. `verify-gate.sh --pre-compact` sprawdza strukturę, wymagane pola, placeholdery i podstawową zgodność Git. Możesz zapisać `FAIL` lub `NOT RUN` jako rzeczywisty wynik weryfikacji — handoff ma przekazywać prawdziwy stan, a nie wymuszać zielone testy.

- **Znacznik czasu:** `<WYPEŁNIJ: YYYY-MM-DD HH:MM>`
- **Powód zapisu:** `<WYPEŁNIJ: Przygotowanie do /compact | Koniec sesji | Blokada decyzyjna | Reguła Stagnacji/Regresji | Zawieszenie przed /feature>`
- **Aktywne zadanie:** `<WYPEŁNIJ: ID_ZADANIA oraz nazwa zadania lub 'Brak - między zadaniami'>`
- **Warstwa i rygor zadania:** `<WYPEŁNIJ: Core/Engine | I/O | UI/Config — Poziom bramki: fast | strict | release>`
- **Status zadania:** `<WYPEŁNIJ: [-] W trakcie | [!] Zablokowane | [x] Ukończone | [R] Do re-weryfikacji>`

---

## 1. Stan Repozytorium i Niezatwierdzone Zmiany (Git State)
- **Gałąź (Branch):** `<WYPEŁNIJ: nazwa gałęzi albo 'Brak repozytorium Git'>`
- **Ostatni commit:** `<WYPEŁNIJ: hash i tytuł albo 'Brak commitów' / 'Brak repozytorium Git'>`
- **Zgoda użytkownika na commit:** `<WYPEŁNIJ: BRAK (Zmiany pozostawione w working tree) | AUTORYZOWANO>`
- **Zmodyfikowane / nowe pliki (`git status --short`):**
  - `<WYPEŁNIJ: lista plików ze statusem M/A/?? i krótkim opisem albo 'Czyste drzewo robocze' / 'Brak repozytorium Git'>`

## 2. Wynik Ostatniej Weryfikacji (Verification Status)
- **Uruchomiona komenda:** `<WYPEŁNIJ: dokładna komenda lub 'NOT RUN'>`
- **Ocena postępu (Stagnation & Regression Rule):** `<WYPEŁNIJ: W normie | STAGNACJA (2x ten sam błąd) | REGRESJA (zepsuto inny test)>`
- **Rzeczywisty wynik:** `<WYPEŁNIJ: PASS | FAIL (X failed, Y passed) | NOT RUN (powód)>`
  - Sam identyczny stan Git nie dowodzi aktualności testów. Zapisz, czy zmienił się kod, konfiguracja, zależności lub środowisko od ostatniej weryfikacji.
- **Kluczowy wycinek błędu (jeśli FAIL):**
  > `<WYPEŁNIJ: krótki wycinek komunikatu błędu ze wskazaniem pliku i funkcji lub 'Brak błędów'>`

## 3. Ustalenia Sesji: Fakty vs. Hipotezy
### Potwierdzone Fakty (Zweryfikowane w kodzie/testach):
- `<WYPEŁNIJ: konkretne ustalenia potwierdzone w tej sesji albo jawne Brak>`

### Hipotezy i Założenia (Wymagają weryfikacji w kodzie — NIE traktować jako faktów!):
- `<WYPEŁNIJ: przypuszczenia dotyczące przyczyny błędu albo jawne Brak otwartych hipotez>`

## 4. Nierozwiązane Problemy i Wymagane Decyzje Użytkownika
- `<WYPEŁNIJ: otwarte pytania blokujące dalszą pracę albo jawne Brak otwartych decyzji blokujących>`

## 5. Minimalny Pakiet Kontekstu dla Następnego Kroku (Kotwice Semantyczne)
- **Następne zadanie / krok do wykonania:** `<WYPEŁNIJ: dokładny opis pierwszej czynności po wznowieniu sesji>`
- **Przeczytaj wyłącznie (Context Refs — używaj nagłówków sekcji i nazw symboli/funkcji, NIE numerów linii):**
  1. `<WYPEŁNIJ: np. PLAN.md (sekcja: #### Zadanie TX.Y)>`
  2. `<WYPEŁNIJ: np. ścieżka/do/pliku (funkcja/struktura: nazwa_symbolu)>`
- **Nie wczytuj (Context Exclusions):**
  - `<WYPEŁNIJ: moduły/katalogi niepowiązane z bieżącym zadaniem>`
