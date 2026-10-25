#!/usr/bin/env bash
# Adaptacyjna bramka jakości dla projektów Node.js, Rust, Python, CMake i PlatformIO.
# Status kontroli (PASS/FAIL/NOT RUN/SKIP) jest oddzielony od jej obowiązkowości.
set -uo pipefail

usage() {
  cat <<'USAGE'
Użycie:
  verify-gate.sh [--level=fast|--level=strict|--level=release] [moduł]
  verify-gate.sh --pre-compact
  verify-gate.sh --check-regression
Opcje:
  --workdir=PATH       Katalog projektu lub podprojektu (monorepo)
  --help               Pomoc

Pominięcia w release wymagają jawnego powodu:
  VERIFY_SKIP_BUILD=1 VERIFY_SKIP_BUILD_REASON='...' 
  VERIFY_SKIP_LINT=1 VERIFY_SKIP_LINT_REASON='...'
  VERIFY_SKIP_TYPECHECK=1 VERIFY_SKIP_TYPECHECK_REASON='...'
USAGE
}

LEVEL=strict
TARGET_MODULE=""
WORKDIR="${VERIFY_WORKDIR:-$PWD}"
LEVEL_SET=0
POSITIONALS=()
for arg in "$@"; do
  case "$arg" in
    --help|-h) usage; exit 0 ;;
    --pre-compact) [[ $LEVEL_SET -eq 0 ]] || { echo "BŁĄD: --pre-compact nie łączy się z innym poziomem." >&2; exit 2; }; LEVEL=pre-compact; LEVEL_SET=1 ;;
    --check-regression|--level=release|release) [[ $LEVEL_SET -eq 0 ]] || { echo "BŁĄD: Podano więcej niż jeden poziom." >&2; exit 2; }; LEVEL=release; LEVEL_SET=1 ;;
    --level=fast|fast) [[ $LEVEL_SET -eq 0 ]] || { echo "BŁĄD: Podano więcej niż jeden poziom." >&2; exit 2; }; LEVEL=fast; LEVEL_SET=1 ;;
    --level=strict|strict|gate) [[ $LEVEL_SET -eq 0 ]] || { echo "BŁĄD: Podano więcej niż jeden poziom." >&2; exit 2; }; LEVEL=strict; LEVEL_SET=1 ;;
    --workdir=*) WORKDIR="${arg#*=}"; [[ -n "$WORKDIR" ]] || { echo "BŁĄD: --workdir wymaga ścieżki." >&2; exit 2; } ;;
    --*) echo "BŁĄD: Nieznana opcja: $arg" >&2; usage >&2; exit 2 ;;
    *) POSITIONALS+=("$arg") ;;
  esac
done
if [[ ${#POSITIONALS[@]} -gt 1 ]]; then echo "BŁĄD: Dozwolony jest maksymalnie jeden argument pozycyjny (moduł w trybie fast)." >&2; exit 2; fi
if [[ ${#POSITIONALS[@]} -eq 1 ]]; then TARGET_MODULE="${POSITIONALS[0]}"; fi
if [[ -n "$TARGET_MODULE" && "$LEVEL" != fast ]]; then echo "BŁĄD: Argument modułu jest dozwolony wyłącznie w trybie fast." >&2; exit 2; fi
if [[ ! -d "$WORKDIR" ]]; then echo "BŁĄD: Katalog roboczy nie istnieje: $WORKDIR" >&2; exit 2; fi
WORKDIR="$(cd "$WORKDIR" 2>/dev/null && pwd -P)" || { echo "BŁĄD: Nie można wejść do katalogu roboczego." >&2; exit 2; }
cd "$WORKDIR" || exit 2

# Raporty nie mogą ukrywać braku Git ani błędów odczytu.
if [[ "$LEVEL" == pre-compact ]]; then
  HANDOFF_FILE="tasks/SESSION_HANDOFF.md"
  echo "=== PRE-COMPACT: $WORKDIR ==="
  if [[ ! -s "$HANDOFF_FILE" ]]; then echo "[FAIL] Brak lub pusty plik $HANDOFF_FILE" >&2; exit 1; fi
  errors=0
  required_headers=("## 1. Stan Repozytorium" "## 2. Wynik Ostatniej Weryfikacji" "## 3. Ustalenia Sesji" "### Potwierdzone Fakty" "### Hipotezy i Założenia" "## 4. Nierozwiązane Problemy i Wymagane Decyzje Użytkownika" "## 5. Minimalny Pakiet Kontekstu")
  for h in "${required_headers[@]}"; do
    if ! grep -Fq "$h" "$HANDOFF_FILE"; then echo "[FAIL] Brak sekcji: $h"; errors=$((errors+1)); fi
  done
  required_fields=("Aktywne zadanie:" "Status zadania:" "Gałąź (Branch):" "Ostatni commit:" "Zgoda użytkownika na commit:" "Uruchomiona komenda:" "Rzeczywisty wynik:" "Następne zadanie / krok do wykonania:")
  for f in "${required_fields[@]}"; do
    line="$(grep -m1 -F "$f" "$HANDOFF_FILE" || true)"
    value="${line#*"$f"}"
    value="${value#\*\*}"
    value="$(printf '%s' "$value" | sed 's/^[[:space:]`]*//;s/[[:space:]`]*$//')"
    if [[ -z "$line" || -z "$value" || "$value" == *"<WYPEŁNIJ"* ]]; then echo "[FAIL] Puste lub nieuzupełnione pole: $f"; errors=$((errors+1)); fi
  done
  if grep -nE '<WYPEŁNIJ|<ID_ZADANIA>|TODO_HANDOFF|YYYY-MM-DD HH:MM|TX\.Y' "$HANDOFF_FILE"; then echo "[FAIL] Pozostały znaczniki szablonu."; errors=$((errors+1)); fi
  if ! grep -Eq '^[-*] \*\*Rzeczywisty wynik:\*\* *(PASS|FAIL|NOT RUN)( |\(|$)' "$HANDOFF_FILE"; then echo "[FAIL] Rzeczywisty wynik musi zaczynać się od PASS, FAIL lub NOT RUN."; errors=$((errors+1)); fi
  # Sekcje mogą jawnie zawierać Brak/Nie dotyczy zamiast fikcyjnych wpisów.
  for marker in '### Potwierdzone Fakty' '### Hipotezy i Założenia' '## 4. Nierozwiązane Problemy i Wymagane Decyzje Użytkownika'; do
    section="$(awk -v m="$marker" 'index($0,m){on=1;next} /^#{1,3} /&&on{exit} on{print}' "$HANDOFF_FILE")"
    if ! printf '%s\n' "$section" | grep -Eqi '^[[:space:]]*([-*] )?.*(Brak|Nie dotyczy|Nie dotyczy|\[.+\])'; then echo "[WARN] Sprawdź ręcznie, czy sekcja ma wpis albo jawne 'Brak/Nie dotyczy': $marker"; fi
  done
  if grep -A8 '### Potwierdzone Fakty' "$HANDOFF_FILE" | grep -Eqi '\[HIPOTEZA\]|prawdopodobnie|przypuszczalnie'; then echo "[WARN] Możliwe hipotezy w sekcji faktów — sprawdź ręcznie."; fi

  if ! command -v git >/dev/null 2>&1; then
    if grep -Fqi 'Brak repozytorium Git' "$HANDOFF_FILE"; then echo "[WARN] Brak binarium Git; handoff jawnie deklaruje brak repozytorium."; else echo "[FAIL] Brak polecenia git, a handoff nie deklaruje braku repozytorium."; errors=$((errors+1)); fi
  elif git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    branch="$(git branch --show-current 2>/dev/null)" || { echo "[FAIL] Nie można odczytać gałęzi Git."; errors=$((errors+1)); branch=""; }
    commit="$(git log -1 --format='%h %s' 2>/dev/null)"; gitrc=$?
    if [[ $gitrc -ne 0 ]]; then echo "[FAIL] Błąd odczytu historii Git."; errors=$((errors+1)); fi
    status="$(git status --porcelain=v1 2>/dev/null)"; gitrc=$?
    if [[ $gitrc -ne 0 ]]; then echo "[FAIL] Błąd git status."; errors=$((errors+1)); fi
    echo "Git branch: ${branch:-[brak gałęzi]}"
    echo "Git commit: ${commit:-[brak commitów]}"
    echo "Git status --porcelain (staged/unstaged/untracked):"
    if [[ -n "$status" ]]; then
      printf '%s\n' "$status"
      printf '%s\n' "$status" | awk 'substr($0,1,2) ~ /[AMDRC]/ {print "  staged/indeks: " $0}'
      printf '%s\n' "$status" | awk 'substr($0,2,1) ~ /[AMDRC]/ {print "  unstaged: " $0}'
      printf '%s\n' "$status" | awk 'substr($0,1,2) == "??" {print "  untracked: " $0}'
    else echo "(czyste drzewo robocze)"; fi
    branch_line="$(grep -m1 -F 'Gałąź (Branch):' "$HANDOFF_FILE" || true)"
    handoff_branch="${branch_line#*Gałąź (Branch):}"
    handoff_branch="${handoff_branch#\*\*}"
    handoff_branch="$(printf '%s' "$handoff_branch" | sed 's/^[[:space:]`]*//;s/[[:space:]`]*$//')"
    if [[ -n "$branch" && -n "$handoff_branch" && "$handoff_branch" != "$branch" && "$handoff_branch" != *"Brak repozytorium"* ]]; then echo "[FAIL] Rozbieżność gałęzi: handoff='$handoff_branch', Git='$branch'."; errors=$((errors+1)); fi
    commit_line="$(grep -m1 -F 'Ostatni commit:' "$HANDOFF_FILE" || true)"
    handoff_commit="${commit_line#*Ostatni commit:}"
    handoff_commit="${handoff_commit#\*\*}"
    handoff_commit="$(printf '%s' "$handoff_commit" | sed 's/^[[:space:]`]*//;s/[[:space:]`]*$//')"
    actual_hash="${commit%% *}"
    handoff_hash="${handoff_commit%% *}"
    if [[ -n "$actual_hash" && -n "$handoff_hash" && "$handoff_hash" != "$actual_hash" && "$handoff_hash" != "Brak" && "$handoff_hash" != *"Brak repozytorium"* ]]; then echo "[FAIL] Rozbieżność ostatniego commita: handoff='$handoff_hash', Git='$actual_hash'."; errors=$((errors+1)); fi
    if grep -Fqi 'Czyste drzewo robocze' "$HANDOFF_FILE" && [[ -n "$status" ]]; then echo "[FAIL] Handoff deklaruje czyste drzewo, ale Git wykazuje zmiany."; errors=$((errors+1)); fi
    if grep -Fqi 'Brak repozytorium Git' "$HANDOFF_FILE"; then echo "[FAIL] Handoff deklaruje brak repozytorium, ale katalog jest repozytorium Git."; errors=$((errors+1)); fi
  else
    gitrc=$?
    if [[ $gitrc -eq 128 ]]; then
      if grep -Fqi 'Brak repozytorium Git' "$HANDOFF_FILE"; then echo "[WARN] Katalog nie jest repozytorium Git; handoff jawnie to deklaruje."; else echo "[FAIL] Katalog nie jest repozytorium Git, a handoff tego nie wyjaśnia."; errors=$((errors+1)); fi
    else
      echo "[FAIL] git rev-parse zakończył się kodem $gitrc — to błąd wykonania, nie potwierdzony brak repozytorium."; errors=$((errors+1))
    fi
  fi
  if [[ -f PLAN.md ]]; then echo "--- Zadania wymagające uwagi w PLAN.md ---"; grep -E '\[-\]|\[!\]|\[R\]' PLAN.md || echo "Brak zadań w toku, blokad i flag [R]."; else echo "[INFO] Brak PLAN.md."; fi
  if [[ $errors -gt 0 ]]; then echo "[FAIL] Pre-compact: $errors błąd/błędy. Handoff pozostaje zapisany; popraw wskazane rozbieżności."; exit 1; fi
  echo "[PASS] Walidacja struktury handoffu i dostępnych danych zakończona bez błędów. Pamiętaj: stan Git sam w sobie nie dowodzi aktualności testów."; exit 0
fi

# Liczniki i raport bramki kodowej.
DETECTED_STACKS=0; PASS_COUNT=0; FAIL_COUNT=0; NOT_RUN_COUNT=0; SKIP_COUNT=0; REQUIRED_PASS=0; REQUIRED_NOT_PASS=0
SUMMARY_LOG=()
record_result() {
  local status="$1" kind="$2" label="$3" detail="${4:-}"
  SUMMARY_LOG+=("[$status] ($kind) $label${detail:+ — $detail}")
  case "$status" in
    PASS) PASS_COUNT=$((PASS_COUNT+1)); [[ "$kind" == REQUIRED ]] && REQUIRED_PASS=$((REQUIRED_PASS+1)) ;;
    FAIL) FAIL_COUNT=$((FAIL_COUNT+1)); [[ "$kind" == REQUIRED ]] && REQUIRED_NOT_PASS=$((REQUIRED_NOT_PASS+1)) ;;
    "NOT RUN") NOT_RUN_COUNT=$((NOT_RUN_COUNT+1)); [[ "$kind" == REQUIRED ]] && REQUIRED_NOT_PASS=$((REQUIRED_NOT_PASS+1)) ;;
    SKIP) SKIP_COUNT=$((SKIP_COUNT+1)); [[ "$kind" == REQUIRED ]] && REQUIRED_NOT_PASS=$((REQUIRED_NOT_PASS+1)) ;;
  esac
}
run_check() {
  local kind="$1" label="$2"; shift 2
  printf -- '-> %s: %s\n' "$kind" "$label"
  if "$@"; then record_result PASS "$kind" "$label"; else local rc=$?; record_result FAIL "$kind" "$label" "kod wyjścia $rc"; fi
}
skip_check() {
  local envflag="$1" reasonvar="$2" label="$3"; shift 3
  local value="${!envflag:-0}" reason="${!reasonvar:-}"
  if [[ "$value" == 1 ]]; then
    if [[ -n "$reason" ]]; then record_result SKIP OPTIONAL "$label" "jawne pominięcie: $reason"; else record_result FAIL REQUIRED "$label" "$envflag=1 wymaga podania $reasonvar"; fi
    return 0
  fi
  return 1
}

# Jawne ostrzeżenie, jeśli użytkownik uruchomił skrypt z katalogu skilla lub nadkatalogu monorepo.
manifest_count=0
for f in package.json Cargo.toml pyproject.toml requirements.txt setup.py CMakeLists.txt platformio.ini; do [[ -f "$f" ]] && manifest_count=$((manifest_count+1)); done
if [[ $manifest_count -eq 0 ]]; then
  echo "[INFO] Brak manifestu projektu w $WORKDIR."
  if [[ "$WORKDIR" == */.claude/skills/* || "$WORKDIR" == */.claude/skills/*/* ]]; then echo "Wskazówka: uruchom skrypt z katalogu projektu lub użyj --workdir=/ścieżka/do/modułu."; fi
  for f in ../package.json ../Cargo.toml ../pyproject.toml ../requirements.txt ../CMakeLists.txt ../platformio.ini; do [[ -f "$f" ]] && { echo "[INFO] Manifest w katalogu nadrzędnym: $f. Użyj --workdir, jeśli to katalog projektu."; break; }; done
  for f in */package.json */Cargo.toml */pyproject.toml */requirements.txt */CMakeLists.txt */platformio.ini; do [[ -f "$f" ]] && { echo "[INFO] Wykryto manifest w podkatalogu: $f. Dla monorepo użyj --workdir=ścieżka/podprojektu."; break; }; done
fi

# Node.js / TypeScript
if [[ -f package.json ]]; then
  DETECTED_STACKS=$((DETECTED_STACKS+1))
  if ! command -v npm >/dev/null 2>&1; then record_result FAIL REQUIRED "Node.js: npm" "brak npm w PATH"; else
    has_script() {
      if command -v node >/dev/null 2>&1; then
        node -e 'const p=require("./package.json");process.exit(typeof (p.scripts||{})[process.argv[1]]==="string" && p.scripts[process.argv[1]].trim()?0:1)' "$1" >/dev/null 2>&1
      elif command -v python3 >/dev/null 2>&1; then
        python3 -c 'import json,sys; p=json.load(open("package.json",encoding="utf-8")); s=p.get("scripts",{}).get(sys.argv[0],""); sys.exit(0 if isinstance(s,str) and s.strip() else 1)' "$1" >/dev/null 2>&1
      else
        grep -q '"'"$1"'"[[:space:]]*:' package.json 2>/dev/null
      fi
    }
    if [[ "$LEVEL" == release ]]; then
      if skip_check VERIFY_SKIP_TYPECHECK VERIFY_SKIP_TYPECHECK_REASON "Node.js typecheck"; then :; elif has_script typecheck; then run_check REQUIRED "Node.js: npm run typecheck" npm run typecheck; else record_result FAIL REQUIRED "Node.js: typecheck" "brak skryptu typecheck; skonfiguruj go lub jawnie uzasadnij pominięcie"; fi
      if skip_check VERIFY_SKIP_LINT VERIFY_SKIP_LINT_REASON "Node.js lint"; then :; elif has_script lint; then run_check REQUIRED "Node.js: npm run lint" npm run lint; else record_result FAIL REQUIRED "Node.js: lint" "brak skryptu lint; skonfiguruj go lub jawnie uzasadnij pominięcie"; fi
    else
      if has_script typecheck; then run_check OPTIONAL "Node.js: npm run typecheck" npm run typecheck; else record_result "NOT RUN" OPTIONAL "Node.js typecheck" "brak skryptu typecheck"; fi
      if [[ "$LEVEL" == fast ]]; then record_result "NOT RUN" OPTIONAL "Node.js lint" "pominięto w trybie fast"; elif has_script lint; then run_check REQUIRED "Node.js: npm run lint" npm run lint; else record_result "NOT RUN" OPTIONAL "Node.js lint" "brak skryptu lint"; fi
    fi
    if [[ "$LEVEL" == fast && -n "$TARGET_MODULE" ]]; then
      if has_script test; then run_check REQUIRED "Node.js test (zakres: $TARGET_MODULE)" npm test -- "$TARGET_MODULE"; else record_result FAIL REQUIRED "Node.js tests" "brak skryptu test"; fi
    elif has_script test; then run_check REQUIRED "Node.js: npm test (pełny projekt)" npm test; else record_result FAIL REQUIRED "Node.js tests" "brak skryptu test"; fi
    if [[ "$LEVEL" == release ]]; then
      if skip_check VERIFY_SKIP_BUILD VERIFY_SKIP_BUILD_REASON "Node.js build"; then :; elif has_script build; then run_check REQUIRED "Node.js: npm run build" npm run build; else record_result FAIL REQUIRED "Node.js build" "brak skryptu build; dodaj go lub jawnie uzasadnij pominięcie"; fi
    fi
  fi
fi

# Rust
if [[ -f Cargo.toml ]]; then
  DETECTED_STACKS=$((DETECTED_STACKS+1))
  if ! command -v cargo >/dev/null 2>&1; then record_result FAIL REQUIRED "Rust: cargo" "brak cargo w PATH"; else
    if [[ "$LEVEL" == fast ]]; then run_check REQUIRED "Rust: cargo check" cargo check --workspace --all-targets; if [[ -n "$TARGET_MODULE" ]]; then run_check REQUIRED "Rust tests (zakres: $TARGET_MODULE)" cargo test --workspace "$TARGET_MODULE"; else run_check REQUIRED "Rust: cargo test (pełny workspace)" cargo test --workspace --all-targets; fi
    else
      if cargo fmt --version >/dev/null 2>&1; then run_check REQUIRED "Rust: cargo fmt --check" cargo fmt --all -- --check; else record_result FAIL REQUIRED "Rust rustfmt" "brak komponentu rustfmt"; fi
      if [[ "$LEVEL" == release ]] || command -v cargo-clippy >/dev/null 2>&1 || cargo clippy --version >/dev/null 2>&1; then if skip_check VERIFY_SKIP_LINT VERIFY_SKIP_LINT_REASON "Rust clippy"; then :; elif cargo clippy --version >/dev/null 2>&1; then run_check REQUIRED "Rust: cargo clippy" cargo clippy --workspace --all-targets --all-features -- -D warnings; else record_result FAIL REQUIRED "Rust clippy" "brak komponentu clippy"; fi
      elif command -v cargo-clippy >/dev/null 2>&1; then run_check OPTIONAL "Rust: cargo clippy" cargo clippy --workspace --all-targets -- -D warnings; else record_result "NOT RUN" OPTIONAL "Rust clippy" "brak clippy w strict"; fi
      run_check REQUIRED "Rust: cargo test" cargo test --workspace --all-targets --all-features
      if [[ "$LEVEL" == release ]]; then if skip_check VERIFY_SKIP_BUILD VERIFY_SKIP_BUILD_REASON "Rust release build"; then :; else run_check REQUIRED "Rust: cargo build --release" cargo build --workspace --release; fi; fi
    fi
  fi
fi

# Python
if [[ -f pyproject.toml || -f requirements.txt || -f setup.py ]]; then
  DETECTED_STACKS=$((DETECTED_STACKS+1))
  if [[ "$LEVEL" == release ]]; then
    if skip_check VERIFY_SKIP_LINT VERIFY_SKIP_LINT_REASON "Python lint"; then :; elif command -v ruff >/dev/null 2>&1; then run_check REQUIRED "Python: ruff check" ruff check .; else record_result FAIL REQUIRED "Python lint" "brak ruff; zainstaluj/skonfiguruj lub jawnie uzasadnij pominięcie"; fi
    if skip_check VERIFY_SKIP_TYPECHECK VERIFY_SKIP_TYPECHECK_REASON "Python typecheck"; then :; elif command -v mypy >/dev/null 2>&1; then run_check REQUIRED "Python: mypy" mypy .; else record_result FAIL REQUIRED "Python typecheck" "brak mypy; zainstaluj/skonfiguruj lub jawnie uzasadnij pominięcie"; fi
  elif [[ "$LEVEL" != fast ]]; then
    if command -v ruff >/dev/null 2>&1; then run_check REQUIRED "Python: ruff check" ruff check .; else record_result "NOT RUN" OPTIONAL "Python lint" "brak ruff"; fi
    if command -v mypy >/dev/null 2>&1; then run_check OPTIONAL "Python: mypy" mypy .; else record_result "NOT RUN" OPTIONAL "Python typecheck" "brak mypy"; fi
  else record_result "NOT RUN" OPTIONAL "Python lint/typecheck" "pominięto w trybie fast"; fi
  if [[ "$LEVEL" == fast && -n "$TARGET_MODULE" ]]; then
    if [[ -e "$TARGET_MODULE" ]]; then
      if command -v pytest >/dev/null 2>&1; then run_check REQUIRED "Python pytest (ścieżka: $TARGET_MODULE)" pytest "$TARGET_MODULE"
      elif python3 -c 'import pytest' >/dev/null 2>&1; then run_check REQUIRED "Python pytest (ścieżka: $TARGET_MODULE)" python3 -m pytest "$TARGET_MODULE"
      elif [[ -d "$TARGET_MODULE" ]]; then run_check REQUIRED "Python unittest discover (katalog: $TARGET_MODULE)" python3 -m unittest discover -s "$TARGET_MODULE"
      elif [[ -f "$TARGET_MODULE" && "$TARGET_MODULE" == *.py ]]; then run_check REQUIRED "Python unittest discover (plik: $TARGET_MODULE)" python3 -m unittest discover -s "$(dirname "$TARGET_MODULE")" -p "$(basename "$TARGET_MODULE")"
      else record_result FAIL REQUIRED "Python tests" "ścieżka wskazana, ale brak pytest i nie można uruchomić jej przez unittest"; fi
    else
      if command -v pytest >/dev/null 2>&1; then run_check REQUIRED "Python pytest (-k: $TARGET_MODULE)" pytest -k "$TARGET_MODULE"
      elif python3 -c 'import pytest' >/dev/null 2>&1; then run_check REQUIRED "Python pytest (-k: $TARGET_MODULE)" python3 -m pytest -k "$TARGET_MODULE"
      else record_result FAIL REQUIRED "Python tests" "filtr nazwy wymaga pytest; unittest nie obsługuje filtra -k"; fi
    fi
  elif command -v pytest >/dev/null 2>&1; then run_check REQUIRED "Python: pytest (pełny projekt)" pytest
  elif python3 -c 'import pytest' >/dev/null 2>&1; then run_check REQUIRED "Python: python3 -m pytest (pełny projekt)" python3 -m pytest
  elif [[ -d tests ]]; then run_check REQUIRED "Python: unittest discover (tests)" python3 -m unittest discover -s tests
  else record_result FAIL REQUIRED "Python test runner" "brak pytest i katalogu tests dla unittest"; fi
  if [[ "$LEVEL" == release ]]; then
    if skip_check VERIFY_SKIP_BUILD VERIFY_SKIP_BUILD_REASON "Python syntax compilation"; then :; else
      pycache="$(mktemp -d 2>/dev/null)" || { record_result FAIL REQUIRED "Python compileall" "nie można utworzyć katalogu tymczasowego"; pycache=""; }
      if [[ -n "$pycache" ]]; then PYTHONPYCACHEPREFIX="$pycache"; export PYTHONPYCACHEPREFIX; run_check REQUIRED "Python: compileall (cache tymczasowy)" python3 -m compileall -q .; unset PYTHONPYCACHEPREFIX; rm -rf "$pycache"; fi
    fi
  fi
fi

# C/C++ CMake
if [[ -f CMakeLists.txt ]]; then
  DETECTED_STACKS=$((DETECTED_STACKS+1))
  if ! command -v cmake >/dev/null 2>&1 || ! command -v ctest >/dev/null 2>&1; then record_result FAIL REQUIRED "CMake toolchain" "brak cmake lub ctest"; else
    build_type=Debug; [[ "$LEVEL" == release ]] && build_type=Release
    run_check REQUIRED "CMake configure ($build_type)" cmake -S . -B build -DCMAKE_BUILD_TYPE="$build_type"
    run_check REQUIRED "CMake build" cmake --build build
    run_check REQUIRED "CMake tests" ctest --test-dir build --output-on-failure
  fi
fi

# PlatformIO
if [[ -f platformio.ini ]]; then
  DETECTED_STACKS=$((DETECTED_STACKS+1))
  if ! command -v pio >/dev/null 2>&1; then record_result FAIL REQUIRED "PlatformIO pio" "brak pio w PATH"; else
    if [[ "$LEVEL" == release || "$LEVEL" == strict ]]; then if skip_check VERIFY_SKIP_LINT VERIFY_SKIP_LINT_REASON "PlatformIO static check"; then :; else run_check REQUIRED "PlatformIO: pio check" pio check --skip-packages; fi; else record_result "NOT RUN" OPTIONAL "PlatformIO: pio check" "pominięto w fast"; fi
    run_check REQUIRED "PlatformIO: pio test" pio test
    if [[ "$LEVEL" == release ]]; then if skip_check VERIFY_SKIP_BUILD VERIFY_SKIP_BUILD_REASON "PlatformIO build"; then :; else run_check REQUIRED "PlatformIO: pio run" pio run; fi; fi
  fi
fi

echo; echo "=== PODSUMOWANIE BRAMKI [${LEVEL^^}] ==="
echo "Katalog projektu : $WORKDIR"
echo "Zakres docelowy  : $([[ -n "$TARGET_MODULE" ]] && printf '%s (sposób dopasowania zależny od stosu)' "$TARGET_MODULE" || printf 'pełny projekt/workspace')"
printf '%s\n' "${SUMMARY_LOG[@]}"
echo "Stosy=$DETECTED_STACKS | PASS=$PASS_COUNT | FAIL=$FAIL_COUNT | NOT RUN=$NOT_RUN_COUNT | SKIP=$SKIP_COUNT | wymagane PASS=$REQUIRED_PASS | wymagane niezaliczone=$REQUIRED_NOT_PASS"
if [[ $DETECTED_STACKS -eq 0 ]]; then echo "[FAIL] Nie wykryto obsługiwanego stosu w $WORKDIR." >&2; exit 1; fi
if [[ $FAIL_COUNT -gt 0 || $REQUIRED_NOT_PASS -gt 0 || $REQUIRED_PASS -eq 0 ]]; then echo "[FAIL] Bramka niezaliczona. Sprawdź FAIL/NOT RUN/SKIP kontroli wymaganych." >&2; exit 1; fi
echo "[PASS] Wszystkie kontrole wymagane dla poziomu $LEVEL zakończyły się powodzeniem."; exit 0
