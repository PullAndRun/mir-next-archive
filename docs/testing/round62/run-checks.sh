#!/usr/bin/env bash
# Run from any directory inside this repository. No ci.ps1 invocation.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 2
commands=(
  'cargo test -p mir-client --tests'
  'cargo test -p mir-client --lib'
  'cargo test -p mir-client --lib ui::projectile::tests::'
  'cargo clippy -p mir-client --all-targets -- -D warnings'
  'cargo fmt --check'
  'py -3 tools/docs/check_links.py'
  'git diff --check'
)
if [[ ${1:-} == --list ]]; then printf '%s\n' "${commands[@]}"; exit 0; fi
if [[ $# -gt 0 ]]; then echo 'Usage: run-checks.sh [--list]' >&2; exit 2; fi
log_dir=${R62_LOG_DIR:-target/round62/r62-d/attempt-1/checks}
mkdir -p "$log_dir" target/tmp || exit 2
# Native Rust file tests need a writable native Windows TEMP path.
if command -v cygpath >/dev/null 2>&1; then
  export TEMP="$(cygpath -m "$PWD/target/tmp")" TMP="$(cygpath -m "$PWD/target/tmp")"
else
  export TEMP="$PWD/target/tmp" TMP="$PWD/target/tmp"
fi
pass=0; fail=0; skip=0; total_start=$SECONDS
: > "$log_dir/summary.tsv"
for i in "${!commands[@]}"; do
  cmd=${commands[$i]}; number=$((i+1)); log="$log_dir/$number.log"; start=$SECONDS
  tool=${cmd%% *}
  printf 'RUN %s: %s\n' "$number" "$cmd"
  if ! command -v "$tool" >/dev/null 2>&1; then
    status=SKIP; rc=127; skip=$((skip+1)); printf 'Missing executable: %s\n' "$tool" > "$log"
  else
    bash -c "$cmd" > "$log" 2>&1; rc=$?
    if [[ $rc -ne 0 ]] || grep -Eq '(^|[[:space:]])(SKIP|PARTIAL)(:|[[:space:]])' "$log"; then
      status=FAIL; fail=$((fail+1))
    elif [[ $number -le 3 ]] && ! grep -Eq 'test result: ok\. [1-9][0-9]* passed;' "$log"; then
      status=FAIL; fail=$((fail+1))
    else status=PASS; pass=$((pass+1)); fi
  fi
  elapsed=$((SECONDS-start))
  printf '%s\texit=%s\tseconds=%s\t%s\t%s\n' "$status" "$rc" "$elapsed" "$cmd" "$log" | tee -a "$log_dir/summary.tsv"
  if [[ $status != PASS ]]; then tail -30 "$log"; fi
 done
printf 'SUMMARY PASS=%s FAIL=%s SKIP=%s TOTAL=7 seconds=%s\n' "$pass" "$fail" "$skip" "$((SECONDS-total_start))" | tee -a "$log_dir/summary.tsv"
[[ $fail -eq 0 && $skip -eq 0 ]]
