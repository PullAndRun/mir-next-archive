#!/usr/bin/env bash
# R63 D: targeted re-run + evidence/hash checks. No ci.ps1 invocation.
# Run from any directory inside this repository. The only CI gate stays Lead's ci.ps1.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 2
commands=(
  'cargo test -p mir-client --lib ui::projectile::tests'
  'cargo test -p mir-client --test round63_projectile_wire'
  'cargo test -p mir-client --test round59_projectile_render'
  'cargo test -p mir-client --test round61_projectile_queue'
  'cargo test -p mir-client --test round62_projectile_wire'
  'cargo fmt --all -- --check'
  'cargo clippy -p mir-client --all-targets -- -D warnings'
  'for f in target/round63/gate/attempt-1.log target/round63/gate/attempt-1.meta.txt target/round63/c/round63-c-evidence.md target/round63/a/attempt-1/summary.md target/round63/b/attempt-1/summary.md target/round63/b/attempt-1/baseline-red-final.log target/round63/b/attempt-1/test-files.final.sha256; do test -s "$f" || { echo "missing evidence: $f"; exit 1; }; done; echo evidence-files-ok:7'
  'test "$(wc -c < target/round63/gate/attempt-1.log | tr -d "[:space:]")" = 190207 && test "$(sha256sum target/round63/gate/attempt-1.log | cut -d" " -f1)" = 15adfa307592ef1ab3ea4cec4df248a852c62c814974c8d5043c8f52e2ac1656 && echo gate-log-ok'
  'bash docs/testing/round63/gen-tree-hashes.sh | diff -q - docs/testing/round63/commit-tree-hashes.txt && echo tree-hashes-reproducible'
  'git cat-file -e 1fcf4fccaf4177475af971dc50db69ffc3a3d1d9^{commit} && for p in crates/mir-client/src/ui/projectile.rs crates/mir-client/tests/round59_projectile_render.rs crates/mir-client/tests/round61_projectile_queue.rs crates/mir-client/tests/round62_projectile_wire.rs crates/mir-client/tests/round63_projectile_wire.rs; do git cat-file -e "1fcf4fc:$p" || exit 1; done; echo git-tree-blobs-ok:5'
  'py -3 -c "import json;d=json.load(open(\"docs/testing/round63/final-manifest.json\",encoding=\"utf-8\"));assert len(d[\"artifacts\"])>0;print(\"manifest-json-ok\",len(d[\"artifacts\"]))"'
  'bash -n docs/testing/round63/run-checks.sh && bash -n docs/testing/round63/gen-tree-hashes.sh && echo scripts-syntax-ok'
  '! grep -RnE "R63[[:space:]]*通过|终审[[:space:]]*PASS|验收[[:space:]]*PASS|生产[[:space:]]*SkipFrames[[:space:]]*已接|缓存[[:space:]]*已限容|125[[:space:]]*全链已证|真实可听已证|R63[[:space:]]*完整" docs/testing/round63 docs/client.md docs/roadmap.md | grep -vE "不得|禁止|尚未|待办|不预写|不宣称|不写|不.{0,4}等于" | grep -q .'
  'py -3 tools/docs/check_links.py'
  'git diff --check'
)
if [[ ${1:-} == --list ]]; then printf '%s\n' "${commands[@]}"; exit 0; fi
if [[ $# -gt 0 ]]; then echo 'Usage: run-checks.sh [--list]' >&2; exit 2; fi
log_dir=${R63_LOG_DIR:-target/round63/d/attempt-1}
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
  tool=${cmd%% *}; tool=${tool#! }
  printf 'RUN %s: %s\n' "$number" "$cmd"
  if ! command -v "$tool" >/dev/null 2>&1; then
    status=SKIP; rc=127; skip=$((skip+1)); printf 'Missing executable: %s\n' "$tool" > "$log"
  else
    bash -c "$cmd" > "$log" 2>&1; rc=$?
    if [[ $rc -ne 0 ]] || grep -Eq '(^|[[:space:]])(SKIP|PARTIAL)(:|[[:space:]])' "$log"; then
      status=FAIL; fail=$((fail+1))
    elif [[ $number -le 5 ]] && ! grep -Eq 'test result: ok\. [1-9][0-9]* passed;' "$log"; then
      status=FAIL; fail=$((fail+1))
    else status=PASS; pass=$((pass+1)); fi
  fi
  elapsed=$((SECONDS-start))
  printf '%s\texit=%s\tseconds=%s\t%s\t%s\n' "$status" "$rc" "$elapsed" "$cmd" "$log" | tee -a "$log_dir/summary.tsv"
  if [[ $status != PASS ]]; then tail -30 "$log"; fi
done
printf 'SUMMARY PASS=%s FAIL=%s SKIP=%s TOTAL=%s seconds=%s\n' "$pass" "$fail" "$skip" "${#commands[@]}" "$((SECONDS-total_start))" | tee -a "$log_dir/summary.tsv"
[[ $fail -eq 0 && $skip -eq 0 ]]
