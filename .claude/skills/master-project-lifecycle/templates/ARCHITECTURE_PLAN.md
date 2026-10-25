# Plan Architektury i Graf Zależności Zadań: [NAZWA PROJEKTU]

## 1. Architektura Warstwowa i Strategia Weryfikacji (Layer-Adaptive Verification)
- **Warstwa Core / Engine:** [Modele danych, algorytmy, parsery] -> **Rygor:** Pełne TDD (testy brzegowe przed kodem), `--level=strict`, zero ostrzeżeń.
- **Warstwa I/O & Integration:** [Bazy danych, protokoły sprzętowe/sieciowe, API] -> **Rygor:** Testy kontraktowe, symulacja błędów (timeout/CRC), Smoke Tests.
- **Warstwa Presentation / UI / Config:** [UI / CLI / Konfiguracja] -> **Rygor:** Kontrola typów (`--level=fast` / `strict`), weryfikacja wizualna/uruchomieniowa.

## 2. Rejestr Decyzji Architektonicznych (ADR — wymagane tylko dla decyzji architektonicznych)
| ID | Data | Status | Decyzja i Kontrakt | Niezmienniki Bazowe (Base Invariants) |
| :--- | :--- | :--- | :--- | :--- |
| ADR-01 | YYYY-MM-DD | Aktywna | [Krótki opis decyzji] | [Czego nie wolno złamać] |

## 3. Rejestr Zmian Zakresu (Change Requests / `/feature` Log)
| ID Zmiany | Data | Tryb (`/feature`) | Opis zatwierdzonej funkcji | Dotknięte zadania (`[R]`) i Zakres Re-weryfikacji | Wprowadzone zadania |
| :--- | :--- | :--- | :--- | :--- | :--- |
| CR-01 | YYYY-MM-DD | Fast-Track / Full Impact | [Opis zatwierdzonej zmiany] | [Brak lub np. T1.1 `[R]`: weryfikacja kontraktu SensorFrame] | [Np. T1.4] |

---

## 4. Mapa Kamieni Milowych i Graf Zależności (Task Dependency Graph)

Legenda statusów (jednolity kontrakt semantyczny):
- `[ ]` **Do zrobienia** — zadanie oczekuje na realizację.
- `[-]` **W trakcie** — zadanie aktualnie realizowane w bieżącej sesji.
- `[x]` **Ukończone i zweryfikowane** — spełniono wszystkie `acceptance_criteria`, wymagane kontrole `verify-gate.sh` zakończyły się wynikiem `PASS` i wpisano dowód weryfikacji.
- `[!]` **Zablokowane** — praca wstrzymana z określonej przyczyny (wymagana decyzja użytkownika lub zadziałała Reguła Stagnacji/Regresji).
- `[R]` **Wymaga ponownej weryfikacji po zmianie** — wcześniejszy status `[x]` został unieważniony przez zmianę `/feature`; wymaga wykonania określonego zakresu re-weryfikacji przed przywróceniem `[x]`.

### Milestone 1: [Nazwa Kamienia Milowego]

#### Zadanie T1.1: [Krótki tytuł zadania]
- **Status:** `[ ]` *(Gdy `[R]`, dopisz tutaj: **Zakres re-weryfikacji `[R]`:** co dokładnie należy ponownie sprawdzić)*
- **Warstwa i Poziom Bramki (`layer` & `gate_level`):** `Core/Engine` | Bramka zamknięcia: `--level=strict`
- **Cel:** [1-2 zdania określające rezultat]
- **depends_on (Gdy istnieją zależności):**
  - `BLOKUJĄCE`: `[Brak lub ID zadań, które muszą mieć status [x] i nie mogą mieć aktywnego [R]]`
  - `INFORMACYJNE`: `[Opcjonalnie: ID zadań dających kontekst]`
  - `INTERFEJSOWE`: `[Opcjonalnie: ADR-01, ścieżka/do/pliku (symbol) - zakaz zmiany bez oceny wpływu]`
- **context_refs & context_reason (Dla zadań nietrywialnych — kotwice semantyczne):**
  - `SPEC.md#Sekcja-2-Wymagania` -> Wymagania dotyczące zakresów pomiarowych i tolerancji błędów.
  - `src/core/interfaces.h (struct FrameHeader)` -> Definicja struktury nagłówka ramki danych.
- **Pola opcjonalne (dodawaj tylko, gdy wnoszą realną wartość):**
  - `context_exclusions`: `[Np. src/ui/* — pomijaj warstwę wizualizacji]`
  - `reuse`: `[Np. bufor cykliczny z src/utils/ring_buffer.h]`
- **invariants (Gdy mają zastosowanie — z analizy ryzyk i niezmienników):**
  - *[Ryzyko: Dane z zewnątrz]* -> Odporność na uszkodzone ramki i przepełnienie bufora.
  - *[Ryzyko: Zasoby]* -> Brak dynamicznej alokacji pamięci wewnątrz pętli odczytu.
  - Wybieraj tylko obszary istotne dla zadania: integralność danych, obsługa błędów, współbieżność, zgodność interfejsów, bezpieczeństwo operacji. Nie twórz scoringu ryzyka ani osobnego rejestru, jeśli nie wnosi wartości.
- **acceptance_criteria (Wymagane):**
  - [AUTO] Kryterium weryfikowane automatycznie, np. poprawne parsowanie ramki prawidłowej.
  - [AUTO] Kryterium weryfikowane automatycznie, np. bezpieczne odrzucenie ramki z błędnym CRC.
  - [MANUAL] Kryterium wymagające ręcznej próby uruchomieniowej lub zachowania widocznego dla użytkownika.
  - [INSPECTION] Kryterium potwierdzane przez inspekcję kodu/konfiguracji. Oznaczenia stosuj tam, gdzie rodzaj dowodu nie jest oczywisty; nie muszą występować przy każdym kryterium.
- **verification & dowód zamknięcia (Wymagane):**
  - Komenda weryfikacyjna: `./.claude/skills/master-project-lifecycle/scripts/verify-gate.sh --level=strict`
  - **Dowód weryfikacji (wypełniane przy nadaniu `[x]`):** `[Brak — do uzupełnienia po weryfikacji: wynik PASS + potwierdzenie kryteriów akceptacji]`
  - Kontrole `[MANUAL]` muszą być faktycznie wykonane przed `[x]`; sukces kompilacji ani testów automatycznych ich nie zastępuje. Jeśli kontrola manualna nie została wykonana, pozostaw `[-]` lub `[!]` i opisz brakujący dowód.
