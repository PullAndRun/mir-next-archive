#!/usr/bin/env bash
# R63: committed tree bytes come from `git show 1fcf4fc:<path>`; pending docs from the worktree.
# NOTE: `git show --name-only 1fcf4fc` lists exactly 5 task files (1 src + 4 tests);
#       the task brief's "6" does not match the actual commit and the commit is authoritative.
# The manifest is intentionally NOT hashed here: it records this file's SHA, and hashing it
# would make the pair self-referential. Pin the manifest only after regenerating this file.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
head=${R63_HEAD:-1fcf4fc}
output=${1:-}
if [[ $# -gt 1 ]]; then echo 'Usage: gen-tree-hashes.sh [output-file]' >&2; exit 2; fi
emit() {
  printf '# head=%s\n' "$head"
  printf '# gate_log_sha256=15adfa307592ef1ab3ea4cec4df248a852c62c814974c8d5043c8f52e2ac1656\n'
  printf '# source=git-show: exact committed blob bytes; docs=worktree: pending documentation bytes\n'
  printf '# tree_files=5; pending_docs=3; manifest=not-hashed (recorded by itself)\n'
  printf '# kind\tbytes\tsha256\tlines\tpath\n'
  local path bytes sha lines
  for path in crates/mir-client/src/ui/projectile.rs crates/mir-client/tests/round59_projectile_render.rs crates/mir-client/tests/round61_projectile_queue.rs crates/mir-client/tests/round62_projectile_wire.rs crates/mir-client/tests/round63_projectile_wire.rs; do
    sha=$(git show "$head:$path" | sha256sum); sha=${sha%% *}
    bytes=$(git show "$head:$path" | wc -c); bytes=${bytes//[[:space:]]/}
    lines=$(git show "$head:$path" | wc -l); lines=${lines//[[:space:]]/}
    printf 'git-show\t%s\t%s\t%s\t%s\n' "$bytes" "$sha" "$lines" "$path"
  done
  for path in docs/testing/round63/README.md docs/testing/round63/run-checks.sh docs/testing/round63/gen-tree-hashes.sh; do
    sha=$(sha256sum "$path"); sha=${sha%% *}
    bytes=$(wc -c < "$path"); bytes=${bytes//[[:space:]]/}
    lines=$(wc -l < "$path"); lines=${lines//[[:space:]]/}
    printf 'worktree\t%s\t%s\t%s\t%s\n' "$bytes" "$sha" "$lines" "$path"
  done
}
if [[ -n "$output" ]]; then emit > "$output"; else emit; fi
