#!/usr/bin/env bash
# Samowystarczalne testy regresyjne verify-gate.sh. Bez sieci i bez zmian w repo użytkownika.
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GATE="$SCRIPT_DIR/verify-gate.sh"
TMP="$(mktemp -d)" || exit 1
trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok() { printf '[PASS] %s\n' "$1"; PASS=$((PASS+1)); }
bad() { printf '[FAIL] %s\n' "$1" >&2; FAIL=$((FAIL+1)); }
expect_rc() { local name="$1" expected="$2"; shift 2; local out rc; out="$("$@" 2>&1)"; rc=$?; if [[ $rc -eq $expected ]]; then ok "$name"; else bad "$name (kod=$rc oczekiwano=$expected; wyjście: $out)"; fi; }
new_project() { mkdir -p "$TMP/$1"; }
mock_bin() { mkdir -p "$1"; }

# 1. CLI help and 2–3. Invalid argument contracts.
expect_rc 'help' 0 "$GATE" --help
expect_rc 'nieznana opcja odrzucona' 2 "$GATE" --wat
expect_rc 'nadmiarowe argumenty odrzucone' 2 "$GATE" --level=fast a b
expect_rc 'moduł poza fast odrzucony' 2 "$GATE" --level=strict module

# 5. Unsupported stack fails with a useful diagnostic.
new_project empty
expect_rc 'brak obsługiwanego stosu kończy się FAIL' 1 "$GATE" --workdir="$TMP/empty" --level=strict

# Mock pytest / npm / git; all mocks are isolated in the temporary directory.
mock_bin "$TMP/bin"
cat > "$TMP/bin/pytest" <<'MOCK'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$MOCK_LOG"
exit "${MOCK_PYTEST_RC:-0}"
MOCK
cat > "$TMP/bin/npm" <<'MOCK'
#!/usr/bin/env bash
printf 'npm %s\n' "$*" >> "$MOCK_LOG"
exit "${MOCK_NPM_RC:-0}"
MOCK
cat > "$TMP/bin/git" <<'MOCK'
#!/usr/bin/env bash
case "$1" in
  rev-parse) echo true ;;
  branch) echo main ;;
  log) echo 'abc123 baseline commit' ;;
  status) exit 0 ;;
  diff) exit 0 ;;
  *) exit 1 ;;
esac
MOCK
chmod +x "$TMP/bin/pytest" "$TMP/bin/npm" "$TMP/bin/git"
export PATH="$TMP/bin:$PATH" MOCK_LOG="$TMP/mock.log"

# 6. A minimal Python project passes when a test runner is available.
new_project pyok; touch "$TMP/pyok/pyproject.toml"; mkdir -p "$TMP/pyok/tests"; cat > "$TMP/pyok/tests/test_sample.py" <<'PYTEST'
import unittest
class SampleTest(unittest.TestCase):
    def test_ok(self):
        self.assertTrue(True)
PYTEST
expect_rc 'Python strict przechodzi przy udanym pytest' 0 "$GATE" --workdir="$TMP/pyok" --level=strict

# 7. Python module path must be passed as a path, not -k expression.
: > "$MOCK_LOG"
expect_rc 'Python fast z istniejącą ścieżką przechodzi' 0 "$GATE" --workdir="$TMP/pyok" --level=fast tests/test_sample.py
if grep -Fxq 'tests/test_sample.py' "$MOCK_LOG"; then ok 'Python fast przekazuje ścieżkę bezpośrednio do pytest'; else bad 'Python fast nie przekazał ścieżki do pytest'; fi

# 8. Failing tests must fail the gate.
export MOCK_PYTEST_RC=7
expect_rc 'nieudane testy blokują bramkę' 1 "$GATE" --workdir="$TMP/pyok" --level=strict
unset MOCK_PYTEST_RC

# 9. Node release without build declaration fails; explicit justified skip is reported.
new_project node_no_build
cat > "$TMP/node_no_build/package.json" <<'JSON'
{"scripts":{"test":"echo test","lint":"echo lint","typecheck":"echo typecheck"}}
JSON
expect_rc 'release bez build i bez deklaracji kończy się FAIL' 1 "$GATE" --workdir="$TMP/node_no_build" --level=release
: > "$MOCK_LOG"
skip_out="$(VERIFY_SKIP_BUILD=1 VERIFY_SKIP_BUILD_REASON='Projekt jest skryptowy i nie generuje artefaktu build' "$GATE" --workdir="$TMP/node_no_build" --level=release 2>&1)"; skip_rc=$?
if [[ $skip_rc -eq 0 ]] && printf '%s\n' "$skip_out" | grep -q '\[SKIP\].*Projekt jest skryptowy'; then ok 'release akceptuje i raportuje uzasadnione SKIP build'; else bad "release nie raportuje poprawnie SKIP build (kod=$skip_rc): $skip_out"; fi

# 10. Skip without a reason must fail.
VERIFY_SKIP_BUILD=1 VERIFY_SKIP_BUILD_REASON='' expect_rc 'SKIP bez uzasadnienia blokuje release' 1 "$GATE" --workdir="$TMP/node_no_build" --level=release

# 11. Pre-compact rejects placeholders/incomplete handoff.
new_project handoff_bad; mkdir -p "$TMP/handoff_bad/tasks"; cp "$SCRIPT_DIR/../templates/SESSION_HANDOFF.md" "$TMP/handoff_bad/tasks/SESSION_HANDOFF.md"
expect_rc 'czysty szablon handoff jest odrzucany' 1 "$GATE" --workdir="$TMP/handoff_bad" --pre-compact

# 12. Pre-compact validates filled handoff and checks Git consistency.
new_project handoff_ok; mkdir -p "$TMP/handoff_ok/tasks"
cat > "$TMP/handoff_ok/tasks/SESSION_HANDOFF.md" <<'HANDOFF'
- **Znacznik czasu:** 2026-10-24 20:00
- **Powód zapisu:** Przygotowanie do compact
- **Aktywne zadanie:** Brak - między zadaniami
- **Warstwa i rygor zadania:** Core/Engine — Poziom bramki: strict
- **Status zadania:** [x] Ukończone
## 1. Stan Repozytorium i Niezatwierdzone Zmiany (Git State)
- **Gałąź (Branch):** main
- **Ostatni commit:** abc123 baseline commit
- **Zgoda użytkownika na commit:** BRAK
- **Zmodyfikowane / nowe pliki:** Czyste drzewo robocze
## 2. Wynik Ostatniej Weryfikacji (Verification Status)
- **Uruchomiona komenda:** pytest
- **Ocena postępu:** W normie
- **Rzeczywisty wynik:** PASS
- **Kluczowy wycinek błędu:** Brak błędów
## 3. Ustalenia Sesji: Fakty vs. Hipotezy
### Potwierdzone Fakty
- Brak dodatkowych faktów.
### Hipotezy i Założenia
- Brak otwartych hipotez.
## 4. Nierozwiązane Problemy i Wymagane Decyzje Użytkownika
- Brak otwartych decyzji blokujących.
## 5. Minimalny Pakiet Kontekstu dla Następnego Kroku (Kotwice Semantyczne)
- **Następne zadanie / krok do wykonania:** Przejrzeć PLAN.md
- **Przeczytaj wyłącznie:** Brak
- **Nie wczytuj:** Nie dotyczy
HANDOFF
expect_rc 'poprawny handoff przechodzi z mockowanym Git' 0 "$GATE" --workdir="$TMP/handoff_ok" --pre-compact


# Additional isolated PATHs: required tool missing and unittest fallback.
mkdir -p "$TMP/bin_no_npm"
expect_rc 'brak wymaganego npm kończy się FAIL' 1 env PATH="$TMP/bin_no_npm:/usr/bin:/bin" "$GATE" --workdir="$TMP/node_no_build" --level=strict

mkdir -p "$TMP/bin_unittest"
cat > "$TMP/bin_unittest/python3" <<'MOCK'
#!/usr/bin/env bash
if [[ "${1:-}" == -c && "${2:-}" == 'import pytest' ]]; then exit 1; fi
if [[ "${1:-}" == -m && "${2:-}" == unittest ]]; then printf 'unittest %s\n' "${*:3}" >> "$MOCK_LOG"; fi
exec /usr/bin/python3 "$@"
MOCK
chmod +x "$TMP/bin_unittest/python3"
: > "$MOCK_LOG"
expect_rc 'Python unittest obsługuje ścieżkę pliku w fast' 0 env PATH="$TMP/bin_unittest:/usr/bin:/bin" MOCK_LOG="$MOCK_LOG" "$GATE" --workdir="$TMP/pyok" --level=fast tests/test_sample.py
if grep -q 'unittest discover -s tests -p test_sample.py' "$MOCK_LOG"; then ok 'Python unittest otrzymał konkretny plik docelowy'; else bad 'Python unittest nie otrzymał konkretnego pliku'; fi

new_project node_no_quality
cat > "$TMP/node_no_quality/package.json" <<'JSON'
{"scripts":{"test":"echo test","build":"echo build"}}
JSON
expect_rc 'release bez lint/typecheck nie przechodzi po cichu' 1 "$GATE" --workdir="$TMP/node_no_quality" --level=release

# A real Git/handoff mismatch is detected.
sed -i 's/Gałąź (Branch):\*\* main/Gałąź (Branch):** wrong-branch/' "$TMP/handoff_ok/tasks/SESSION_HANDOFF.md"
expect_rc 'rozbieżność gałęzi Git blokuje pre-compact' 1 "$GATE" --workdir="$TMP/handoff_ok" --pre-compact
cp "$TMP/handoff_ok/tasks/SESSION_HANDOFF.md" "$TMP/handoff_branch_mismatch.md"
cp "$TMP/handoff_branch_mismatch.md" "$TMP/handoff_ok/tasks/SESSION_HANDOFF.md"
sed -i 's/Gałąź (Branch):\*\* wrong-branch/Gałąź (Branch):** main/' "$TMP/handoff_ok/tasks/SESSION_HANDOFF.md"
sed -i 's/Ostatni commit:\*\* abc123/Ostatni commit:** deadbeef/' "$TMP/handoff_ok/tasks/SESSION_HANDOFF.md"
expect_rc 'rozbieżność ostatniego commita blokuje pre-compact' 1 "$GATE" --workdir="$TMP/handoff_ok" --pre-compact

# 14. --workdir supports monorepo subprojects without guessing.
new_project mono; mkdir -p "$TMP/mono/frontend"; cp "$TMP/node_no_build/package.json" "$TMP/mono/frontend/package.json"
expect_rc '--workdir wskazuje jawny podprojekt' 0 "$GATE" --workdir="$TMP/mono/frontend" --level=strict

printf '\nWynik testów: PASS=%d FAIL=%d\n' "$PASS" "$FAIL"
[[ $FAIL -eq 0 ]]
