#!/usr/bin/env bash
# =============================================================================
# 《传奇-Next》R58（第 58 轮）人工验收脚本
#
# 用途：本轮三项写入（S1 FireBounce 终局 RNG 对齐 / S2 客户端 125 接线 /
#       S3 协议 roundtrip 路径修正）的本地人工验收，固定 9 条命令。
# 写法要点：
#   - 逐条执行并保存「命令 + 原始 exit 码 + 原始输出」，不做管道截流（管道会丢失退出码）；
#   - 只开 set -u（未定义变量即报错），不启用 errexit 语义：任一命令失败仍要继续
#     跑完其余命令，最后统一汇总；
#   - 测试类命令做「0 命中假绿」防护：exit=0 但输出 running 0 tests / 0 passed
#     时判 FAIL（R57 曾出现过滤串不匹配仍 exit=0 的假绿，见 R57 审计 M3）；
#   - 用例文件尚未落盘（S1/S2 未产出）时记 SKIP 并写明缺失路径，不当作通过。
#
# 边界：本脚本**不进 CI**（CI 以 scripts/ci.ps1 为准），仅供本轮人工验收与留证；
#       不改源码、不 commit、不跑数据库。证据默认写入 target/round58/s3/attempt-1/。
#
# 用法：
#   bash docs/testing/round58/run-checks.sh          # 执行全部命令并写证据
#   bash docs/testing/round58/run-checks.sh --list   # 只打印固定命令清单（零执行、零写入）
#
# 环境变量：
#   ROUND58_EVIDENCE  证据目录（缺省 target/round58/s3/attempt-1）
#
# 退出码：0=无 FAIL（SKIP 不致命，但会显式提示人工确认）；1=有 FAIL；2=脚本自身错误
# =============================================================================
set -u

# ---------------------------------------------------------------------------
# 固定命令清单（唯一事实来源；--list 与执行共用同一份数据）
# 字段：序号|说明|日志 slug|类型|载荷
#   类型 always   ：载荷为完整命令串
#   类型 proto    ：载荷为 mir-protocol lib 用例过滤串（--exact，先经 --list 校验存在）
#   类型 testfile ：载荷为 包名:测试文件名（先校验 crates/<包>/tests/<文件>.rs 是否存在）
#   类型 libfilter：载荷为 包名:lib 用例过滤串
# 注意：协议用例的真实模块段是 roundtrip（不是 R57 旧脚本里误写的那个模块段）。
# ---------------------------------------------------------------------------
CHECKS=(
  '01|格式检查（fmt）|fmt|always|cargo fmt --all --check'
  '02|Clippy 双包全目标（-D warnings）|clippy|always|cargo clippy -p mir-server -p mir-client --all-targets -- -D warnings'
  '03|协议 roundtrip（服务端包）|protocol-server-roundtrip|proto|packets::server::roundtrip::default_packets_roundtrip'
  '04|协议 roundtrip（客户端包）|protocol-client-roundtrip|proto|packets::client::roundtrip::default_packets_roundtrip'
  '05|服务端 RNG 抽签顺序（round58）|round58-rng-order|testfile|mir-server:round58_rng_order'
  '06|火球弹射回归（round57）|round57-firebounce|testfile|mir-server:round57_firebounce'
  '07|magic 单元测试|server-magic-lib|libfilter|mir-server:magic'
  '08|客户端 125 接线（round58）|round58-projectile-wire|testfile|mir-client:round58_projectile_wire'
  '09|客户端 net 单元测试|client-net-lib|libfilter|mir-client:net::'
)

# 根据「类型 + 载荷」构造命令行与展示串，结果写入全局 CMD_DISPLAY / CMD[]
build_command() {
  local kind="$1" payload="$2" pkg file
  CMD_DISPLAY=""
  CMD=()
  case "$kind" in
    always)
      CMD_DISPLAY="$payload"
      CMD=(bash -c "$payload")
      ;;
    proto)
      CMD_DISPLAY="cargo test -p mir-protocol --lib $payload -- --exact"
      CMD=(cargo test -p mir-protocol --lib "$payload" -- --exact)
      ;;
    testfile)
      pkg="${payload%%:*}"
      file="${payload#*:}"
      CMD_DISPLAY="cargo test -p $pkg --test $file"
      CMD=(cargo test -p "$pkg" --test "$file")
      ;;
    libfilter)
      pkg="${payload%%:*}"
      file="${payload#*:}"
      CMD_DISPLAY="cargo test -p $pkg --lib $file"
      CMD=(cargo test -p "$pkg" --lib "$file")
      ;;
    *)
      echo "内部错误：未知命令类型 $kind" >&2
      exit 2
      ;;
  esac
}

# ---------------------------------------------------------------------------
# 参数：--list 只列清单，不执行、不写盘
# ---------------------------------------------------------------------------
case "${1:-}" in
  --list|-l)
    echo "# R58 固定验收命令清单（共 ${#CHECKS[@]} 条；执行：bash docs/testing/round58/run-checks.sh）"
    for spec in "${CHECKS[@]}"; do
      IFS='|' read -r id name slug kind payload <<<"$spec"
      build_command "$kind" "$payload"
      printf '%s  %s\n' "$id" "$CMD_DISPLAY"
    done
    exit 0
    ;;
  "")
    ;;
  *)
    echo "错误：未知参数 ${1:-}（用法：bash docs/testing/round58/run-checks.sh [--list]）" >&2
    exit 2
    ;;
esac

# ---------------------------------------------------------------------------
# 环境准备
# ---------------------------------------------------------------------------
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || {
  echo "错误：当前目录不是 git 仓库，无法定位仓库根" >&2
  exit 2
}
cd "$REPO_ROOT" || exit 2
export TMP="$REPO_ROOT/target/tmp"
export TEMP="$TMP"
E="${ROUND58_EVIDENCE:-target/round58/s3/attempt-1}"
mkdir -p "$TMP" "$E/logs" || exit 2

echo "仓库根：$REPO_ROOT"
echo "HEAD  ：$(git rev-parse --short HEAD 2>/dev/null || echo 未知)"
echo "证据  ：$E"
echo

# 单条命令执行：输出写 logs/<序号>-<slug>.log，原始 exit 记入 rc
run_logged() {
  local log="$1" display="$2"
  shift 2
  local rc
  {
    printf '$ %s\n\n' "$display"
    "$@"
    rc=$?
    printf '\nEXIT=%d\n' "$rc"
  } >"$log" 2>&1
  return "$rc"
}

SUMMARY="$E/03-exit-summary.tsv"
printf '序号\t状态\t原始exit\t备注\t日志\t命令\n' >"$SUMMARY"

# ---------------------------------------------------------------------------
# 前置检查：拉一次协议用例清单，用于校验 proto 过滤串真实存在（防假绿）
# ---------------------------------------------------------------------------
PROTOCOL_LIST_LOG="$E/logs/00-protocol-list.log"
{
  printf '$ cargo test -p mir-protocol --lib -- --list\n\n'
  cargo test -p mir-protocol --lib -- --list
  proto_list_rc=$?
  printf '\nEXIT=%d\n' "$proto_list_rc"
} >"$PROTOCOL_LIST_LOG" 2>&1

echo "前置：协议用例清单 -> logs/00-protocol-list.log（exit=$proto_list_rc）"

# ---------------------------------------------------------------------------
# 主循环
# ---------------------------------------------------------------------------
pass=0
fail=0
skip=0
total=0

for spec in "${CHECKS[@]}"; do
  IFS='|' read -r id name slug kind payload <<<"$spec"
  build_command "$kind" "$payload"
  display="$CMD_DISPLAY"
  log="$E/logs/${id}-${slug}.log"
  status=""
  note=""
  rc=""

  # --- 前置跳过判定 ---
  case "$kind" in
    testfile)
      pkg="${payload%%:*}"
      file="${payload#*:}"
      if [ ! -f "crates/$pkg/tests/$file.rs" ]; then
        status="SKIP"
        note="用例文件未落盘：crates/$pkg/tests/$file.rs（S1/S2 可能尚未产出）"
        {
          printf '$ %s\n\n' "$display"
          printf 'SKIP: %s\n' "$note"
        } >"$log" 2>&1
        rc="SKIP"
      fi
      ;;
    proto)
      if [ "$proto_list_rc" -ne 0 ]; then
        status="FAIL"
        note="协议用例清单命令失败（exit=$proto_list_rc），无法校验过滤串"
        {
          printf '$ %s\n\n' "$display"
          printf 'FAIL: %s\n' "$note"
        } >"$log" 2>&1
        rc="N/A"
      elif ! grep -qF "$payload: test" "$PROTOCOL_LIST_LOG"; then
        status="SKIP"
        note="过滤串未出现在 cargo test --list：$payload（模块可能改名，按 SKIP 待人工确认）"
        {
          printf '$ %s\n\n' "$display"
          printf 'SKIP: %s\n' "$note"
          printf '(前置清单见 logs/00-protocol-list.log)\n'
        } >"$log" 2>&1
        rc="SKIP"
      fi
      ;;
  esac

  # --- 实际执行 ---
  if [ -z "$status" ]; then
    run_logged "$log" "$display" "${CMD[@]}"
    rc=$?
    if [ "$rc" -eq 0 ]; then
      # 0 命中假绿防护：exit=0 但一条用例都没跑，不算通过
      if [ "$kind" != "always" ] && grep -qE 'running 0 tests|test result: ok\. 0 passed' "$log"; then
        status="FAIL"
        note="0 个用例命中，exit=0 属假绿（过滤器/路径不匹配）"
      else
        status="PASS"
      fi
    else
      status="FAIL"
      note="命令非 0 退出"
    fi
  fi

  # --- 记账与输出 ---
  total=$((total + 1))
  case "$status" in
    PASS) pass=$((pass + 1)) ;;
    FAIL) fail=$((fail + 1)) ;;
    SKIP) skip=$((skip + 1)) ;;
  esac
  printf '%-2s %-4s exit=%-4s %-46s %s\n' "$id" "$status" "$rc" "logs/${id}-${slug}.log" "$name"
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$status" "$rc" "$note" "logs/${id}-${slug}.log" "$display" >>"$SUMMARY"
done

# ---------------------------------------------------------------------------
# 汇总
# ---------------------------------------------------------------------------
# 任一 FAIL 则非 0；SKIP 不致命（用显式警告表达），避免把「子任务未落盘」误报成命令失败。

echo
echo "================= R58 验收汇总 ================="
printf '总计 %d 条：PASS=%d FAIL=%d SKIP=%d\n' "$total" "$pass" "$fail" "$skip"
printf '明细表：%s\n' "$SUMMARY"
printf '日志目录：%s/logs/\n' "$E"
if [ "$skip" -gt 0 ]; then
  echo '注意：存在 SKIP（对应子任务用例尚未落盘或过滤串未匹配），须人工确认后再定门禁。'
fi
if [ "$fail" -gt 0 ]; then
  printf '总 EXIT=1（%d 条 FAIL，见上表与日志）\n' "$fail"
  exit 1
fi
echo '总 EXIT=0（无 FAIL）'
exit 0
