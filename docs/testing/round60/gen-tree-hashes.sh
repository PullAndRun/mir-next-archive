#!/usr/bin/env bash
# R60 树哈希清单生成器（M-7）。只读：只写 docs/testing/round60/commit-tree-hashes.txt。
# 头部三字段：head / gate_head / gate_log_sha256 / gate_log_bytes；清单自身 sha 不入清单。
set -u
OUT=docs/testing/round60/commit-tree-hashes.txt
GATE=target/round60/gate/attempt-1.log
HEAD_SHORT=$(git rev-parse --short HEAD)
GATE_HEAD=bb53321
GATE_SHA=$(sha256sum "$GATE" | awk '{print $1}')
GATE_BYTES=$(wc -c <"$GATE")

CRATES=(
  crates/mir-client/src/app.rs
  crates/mir-client/src/ui/mod.rs
  crates/mir-client/src/net.rs
  crates/mir-client/src/ui/projectile.rs
)
DOCS_D3=(
  docs/client.md
  docs/roadmap.md
  docs/testing/round59/README.md
  docs/testing/round60/README.md
  docs/testing/round60/run-checks.sh
  docs/testing/round60/gen-tree-hashes.sh
)
DOCS_PROCESS=(
  docs/process/round60-plan.md
  docs/process/round60-menxia-review.md
  docs/process/round60-shangshu-dispatch.md
)

emit() {
  local role="$1" path="$2" bytes sha
  bytes=$(wc -c <"$path")
  sha=$(sha256sum "$path" | awk '{print $1}')
  printf '%s\t%s\t%s\t%s\n' "$role" "$path" "$bytes" "$sha"
}

{
  echo "# R60 提交树哈希清单（M-7）：TAB 分隔 role<TAB>path<TAB>bytes<TAB>sha256"
  echo "# 说明：清单自身 sha256 不写入本清单（只由 D4 的 final-manifest.json 记录），破自哈希循环。"
  echo "# head=$HEAD_SHORT"
  echo "# gate_head=$GATE_HEAD"
  echo "# gate_log_sha256=$GATE_SHA"
  echo "# gate_log_bytes=$GATE_BYTES"
  echo "# gate_log_path=$GATE"
  echo "# gen=bash docs/testing/round60/gen-tree-hashes.sh  # 等价：wc -c <路径> + sha256sum 路径"
  echo "# scope=提交①（${GATE_HEAD}）的 crates/** 4 文件 + 本轮 docs/** 候选（D3 交付 5 件 + 3 份 process 文件）"
  echo "# verify=crates 4 文件与 ${GATE_HEAD} 逐字节一致：git diff ${GATE_HEAD} -- <4 路径> 为空（见 target/round60/d3/attempt-1/06-tree-hashes.log）"
  for p in "${CRATES[@]}"; do emit crates "$p"; done
  for p in "${DOCS_D3[@]}"; do emit docs-d3 "$p"; done
  for p in "${DOCS_PROCESS[@]}"; do emit docs-process "$p"; done
} >"$OUT"

echo "wrote $OUT"
echo "lines=$(wc -l <"$OUT") bytes=$(wc -c <"$OUT")"
echo "own_sha256=$(sha256sum "$OUT" | awk '{print $1}')"
