#!/usr/bin/env bash
# R62: source bytes from git show; pending docs from the working tree.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
head=${R62_HEAD:-a7362fc}
output=${1:-}
if [[ $# -gt 1 ]]; then echo 'Usage: gen-tree-hashes.sh [output-file]' >&2; exit 2; fi
emit() {
  printf '# head=%s\n' "$head"
  printf '# gate_log_sha256=81e8ef05131460635adaee4bf02066d4e2187a2a852573a8ae7191a23adfd9ab\n'
  printf '# source=git-show: exact committed blob bytes; docs=worktree: pending documentation bytes\n'
  printf '# kind\tbytes\tsha256\tlines\tpath\n'
  local path bytes sha lines
  for path in crates/mir-client/src/app.rs crates/mir-client/src/net.rs crates/mir-client/src/ui/projectile.rs crates/mir-client/tests/round59_projectile_render.rs crates/mir-client/tests/round61_projectile_queue.rs crates/mir-client/tests/round62_projectile_wire.rs; do
    sha=$(git show "$head:$path" | sha256sum); sha=${sha%% *}
    bytes=$(git show "$head:$path" | wc -c); bytes=${bytes//[[:space:]]/}
    lines=$(git show "$head:$path" | wc -l); lines=${lines//[[:space:]]/}
    printf 'git-show\t%s\t%s\t%s\t%s\n' "$bytes" "$sha" "$lines" "$path"
  done
  for path in docs/testing/round62/README.md docs/testing/round62/final-manifest.json docs/testing/round62/run-checks.sh docs/testing/round62/gen-tree-hashes.sh; do
    sha=$(sha256sum "$path"); sha=${sha%% *}
    bytes=$(wc -c < "$path"); bytes=${bytes//[[:space:]]/}
    lines=$(wc -l < "$path"); lines=${lines//[[:space:]]/}
    printf 'worktree\t%s\t%s\t%s\t%s\n' "$bytes" "$sha" "$lines" "$path"
  done
}
if [[ -n "$output" ]]; then emit > "$output"; else emit; fi
