#!/usr/bin/env bash
# R61 D3 定向验收：只做文档约定的测试/静态检查，不运行 ci.ps1。
set -u
EVID="${ROUND61_EVIDENCE:-target/round61/d3/attempt-1}"
TMP_ROOT="$(pwd)/target/tmp"
mkdir -p "$TMP_ROOT" || exit 2
export TMP="$TMP_ROOT" TEMP="$TMP_ROOT" TMPDIR="$TMP_ROOT"
CHECKS=(
 '01|fmt|always|cargo fmt --all --check'
 '02|clippy|always|cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings'
 '03|queue|testfile|mir-client@round61_projectile_queue@11'
 '04|sound|testfile|mir-client@round61_projectile_sound@8'
 '05|ui projectile|libfilter|mir-client@ui::projectile@13'
 '06|net|libfilter|mir-client@net::@31'
 '07|audio|libfilter|mir-client@audio::@21'
 '08|client bin|bin|mir-client@23'
 '09|R59 render|testfile|mir-client@round59_projectile_render@16'
 '10|R58 wire|testfile|mir-client@round58_projectile_wire@7'
 '11|render sound|libfilter|mir-render@sound::@9'
 '12|client lib aggregate|always|cargo test -p mir-client --lib'
)
MAX=${#CHECKS[@]}; PASS=0; FAIL=0; SKIP=0; FAIL_LIST=""; SKIP_LIST=""
list_checks() {
 echo "# R61 checks ($MAX items); --list performs no execution and writes no evidence"
 for spec in "${CHECKS[@]}"; do IFS='|' read -r id name kind payload <<<"$spec"; case "$kind" in
  testfile) p="${payload%%@*}"; r="${payload#*@}"; f="${r%%@*}"; echo "$id cargo test -p $p --test $f -- --list (min ${r##*@})";;
  libfilter) p="${payload%%@*}"; r="${payload#*@}"; f="${r%@*}"; echo "$id cargo test -p $p --lib $f -- --list (min ${payload##*@})";;
  bin) p="${payload%%@*}"; echo "$id cargo test -p $p --bin $p -- --list (min ${payload##*@})";;
  *) echo "$id $payload";; esac; done
}
case "${1:-}" in --list|-l) list_checks; exit 0;; "") ;; *) echo "unknown argument: $1" >&2; exit 2;; esac
mkdir -p "$EVID" || exit 2
has_bad_output() { grep -Eq 'running 0 tests|(^|[^0-9])0 passed|^(SKIP|PARTIAL):|(SKIP|PARTIAL): ' "$1" 2>/dev/null; }
count_found() { grep -c ': test$' "$1" 2>/dev/null || true; }
mark() { if [[ "$1" == PASS ]]; then PASS=$((PASS+1)); else FAIL=$((FAIL+1)); FAIL_LIST="$FAIL_LIST $2"; fi; }
run_test() {
 local id="$1" kind="$2" pkg="$3" target="$4" min="$5"; local ll="$EVID/${id}-list.log" rl="$EVID/${id}-run.log" lrc rrc found
 if [[ "$kind" == bin ]]; then cargo test -p "$pkg" --bin "$target" -- --list >"$ll" 2>&1; lrc=$?; else
  if [[ "$kind" == testfile ]]; then cargo test -p "$pkg" --test "$target" -- --list >"$ll" 2>&1; else cargo test -p "$pkg" --lib "$target" -- --list >"$ll" 2>&1; fi; lrc=$?; fi
 found=$(count_found "$ll"); echo "COMMAND=list; EXIT=$lrc; FOUND=$found; MIN=$min" >>"$ll"
 if [[ $lrc -ne 0 || "$found" -eq 0 || "$found" -lt "$min" ]]; then echo "$id FAIL list exit=$lrc found=$found min=$min"; mark FAIL "$id"; return; fi
 if [[ "$kind" == bin ]]; then cargo test -p "$pkg" --bin "$target" >"$rl" 2>&1; elif [[ "$kind" == testfile ]]; then cargo test -p "$pkg" --test "$target" >"$rl" 2>&1; else cargo test -p "$pkg" --lib "$target" >"$rl" 2>&1; fi; rrc=$?
 echo "COMMAND=run; EXIT=$rrc" >>"$rl"
 if [[ $rrc -ne 0 || $(has_bad_output "$rl"; echo $?) -eq 0 || $(has_bad_output "$ll"; echo $?) -eq 0 ]]; then echo "$id FAIL run exit=$rrc"; mark FAIL "$id"; else echo "$id PASS found=$found"; mark PASS "$id"; fi
}
echo "# R61 D3 checks HEAD=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
for spec in "${CHECKS[@]}"; do IFS='|' read -r id name kind payload <<<"$spec"; echo "[$id] $name"
 case "$kind" in
  always) log="$EVID/${id}-run.log"; bash -c "$payload" >"$log" 2>&1; rc=$?; echo "COMMAND=$payload; EXIT=$rc" >>"$log"; if [[ $rc -eq 0 && $(has_bad_output "$log"; echo $?) -ne 0 ]]; then echo "$id PASS"; mark PASS "$id"; else echo "$id FAIL exit=$rc"; mark FAIL "$id"; fi;;
  testfile) p="${payload%%@*}"; r="${payload#*@}"; f="${r%%@*}"; run_test "$id" testfile "$p" "$f" "${r##*@}";;
  libfilter) p="${payload%%@*}"; r="${payload#*@}"; f="${r%@*}"; run_test "$id" libfilter "$p" "$f" "${payload##*@}";;
  bin) p="${payload%%@*}"; run_test "$id" bin "$p" "$p" "${payload##*@}";;
 esac
done
echo "# SUMMARY PASS=$PASS FAIL=$FAIL SKIP=$SKIP TOTAL=$MAX"
[[ $FAIL -eq 0 && $SKIP -eq 0 ]] && { echo '# RESULT=PASS'; exit 0; }
echo "# RESULT=FAIL FAIL_LIST=$FAIL_LIST SKIP_LIST=$SKIP_LIST"; exit 1
