#!/usr/bin/env bash
# R61 tree hashes. Code entries use git show, avoiding worktree newline conversion.
set -u
HEAD_REF="${1:-$(git rev-parse HEAD)}"
GATE_HEAD="ef317b040fdb6ef78a478b1fa247ae3cee775e20"
GATE_LOG="target/round61/gate/attempt-1.log"
GATE_BYTES=207593
GATE_SHA="6ff5f68dd112020e20d42ddf8d6f835be7099dab999e1a411d1b9b4cff77ad96"
OUT="${2:-docs/testing/round61/commit-tree-hashes.txt}"
mkdir -p "$(dirname "$OUT")"
CODE_PATHS=(
 crates/mir-client/src/net.rs
 crates/mir-client/src/ui/projectile.rs
 crates/mir-client/src/audio.rs
 crates/mir-client/src/app.rs
 crates/mir-client/tests/round61_projectile_queue.rs
 crates/mir-client/tests/round61_projectile_sound.rs
 crates/mir-client/tests/round58_projectile_wire.rs
 crates/mir-client/tests/round59_projectile_render.rs
)
DOC_PATHS=(docs/testing/round61/README.md docs/testing/round61/run-checks.sh docs/testing/round61/gen-tree-hashes.sh)
{
 echo "# R61 commit tree hashes"
 echo "# head=$HEAD_REF"
 echo "# gate_head=$GATE_HEAD"
 echo "# gate_log=$GATE_LOG"
 echo "# gate_log_bytes=$GATE_BYTES"
 echo "# gate_log_sha256=$GATE_SHA"
 echo "# code: sha256(git show <head>:<path>); docs: sha256(current candidate bytes)"
 echo "# commit-1 code paths: 8; commit-2 candidate docs: 4 (this list excludes itself)"
 for path in "${CODE_PATHS[@]}"; do
   hash=$(git show "$HEAD_REF:$path" | sha256sum | awk '{print $1}') || exit 1
   printf '%s  %s  [commit-1 code; git show %s:%s]\n' "$hash" "$path" "$HEAD_REF" "$path"
 done
 for path in "${DOC_PATHS[@]}"; do
   hash=$(sha256sum "$path" | awk '{print $1}') || exit 1
   printf '%s  %s  [commit-2 candidate; generated %s]\n' "$hash" "$path" "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
 done
 echo "# commit-tree-hashes.txt self-hash intentionally omitted; final-manifest records it."
} >"$OUT"
echo "generated $OUT for head=$HEAD_REF"
