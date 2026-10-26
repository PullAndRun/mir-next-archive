#!/usr/bin/env bash
# Frozen feature-commit blobs; MISSING records a path absent in that commit.
set -euo pipefail
[[ $# == 0 ]] || { echo 'Usage: gen-tree-hashes.sh' >&2; exit 2; }
cd "$(git rev-parse --show-toplevel)"
head=063c6f609e2ee61fc4d3cd8c5d322dcc32c49ce2
paths=(crates/mir-client/src/net.rs crates/mir-client/src/app.rs crates/mir-client/src/ui/projectile.rs crates/mir-client/tests/round64_event_reclaim.rs crates/mir-client/tests/round64_projectile_ack.rs docs/client.md docs/roadmap.md docs/testing/round64/README.md)
for path in "${paths[@]}"; do
  if git cat-file -e "$head:$path" 2>/dev/null; then
    hash=$(git show "$head:$path" | sha256sum | cut -d ' ' -f1)
    printf '%s  %s\n' "$hash" "$path"
  elif [[ $path == docs/testing/round64/README.md ]]; then
    printf 'MISSING  %s\n' "$path"
  else
    printf 'Required commit blob missing: %s\n' "$path" >&2
    exit 1
  fi
done
