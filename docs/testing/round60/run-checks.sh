#!/usr/bin/env bash
# =============================================================================
# 《传奇-Next》R60（第 60 轮）人工验收脚本（D3 / 礼部）
#
# 用途：R60「125 音效真实通道（G1）+ 投射物事件去重（G2）」的本地人工验收，
#       固定 11 项检查：D1 定向 2 项 / D2 定向 2 项 / 回归 3 项 / 静态 2 项 /
#       锚点存在性 1 项 / M34 素材 1 项。
#
# 与 R59 旧脚本（docs/testing/round59/run-checks.sh）的差异（不复制其失败点）：
#   1. 每项测试**先** `cargo test … -- --list` 校验发现数，发现数 0 或低于门槛即 FAIL；
#   2. 执行期再防假绿：exit=0 但输出含 `running 0 tests` 或 `0 passed` 一律判 FAIL；
#   3. 输出中任何 `SKIP:` / `PARTIAL:` 字面量（如 `SKIP: missing M34-1.mna`）
#      一律记为「非 PASS」的 SKIP 状态，**SKIP≠PASS**，最终 exit 非 0；
#   4. 逐项保存「命令行 + 原始 exit + 原始输出」，不走管道截流（管道会吞退出码）；
#   5. 只开 set -u，不启用 errexit：任一项失败仍跑完，最后统一汇总 PASS/FAIL/SKIP 计数。
#
# 边界：本脚本不进 CI（CI 以 tools/dev/ci.ps1 为准），只做本轮人工验收与留证；
#       不改源码、不 commit、不跑数据库、不发网络包。
#
# 用法：
#   bash docs/testing/round60/run-checks.sh          # 执行全部 11 项并写证据
#   bash docs/testing/round60/run-checks.sh --list   # 只打印清单（零执行、零写入）
#
# 环境变量：
#   ROUND60_EVIDENCE  证据目录（缺省 target/round60/d3/attempt-1/logs）
#
# 退出码：0=11 项全 PASS；1=有 FAIL 或 SKIP；2=脚本自身错误
# =============================================================================
set -u

EVID="${ROUND60_EVIDENCE:-target/round60/d3/attempt-1/logs}"

# 环境：本机系统 TEMP 不可写（沙箱），用例建临时目录会 PermissionDenied（与 R60 被测逻辑无关）。
# 统一把 TMP/TEMP/TMPDIR 指到仓内 target/tmp（与 D1 证据同口径），让失败只反映被测逻辑。
REPO_TMP="$(pwd)/target/tmp"
mkdir -p "$REPO_TMP" || {
  echo "错误：无法创建仓内临时目录 $REPO_TMP" >&2
  exit 2
}
export TMP="$REPO_TMP" TEMP="$REPO_TMP" TMPDIR="$REPO_TMP"

# ---------------------------------------------------------------------------
# 固定清单（唯一事实来源；--list 与执行共用同一份数据）
# 字段：序号|说明|日志 slug|类型|载荷
#   bin       ：载荷 包名@最少发现数（`cargo test -p <包> --bin <包>`，先 --list）
#   testfile  ：载荷 包名@测试文件名@最少发现数（先 --list，再真跑）
#   libfilter ：载荷 包名@lib 过滤串@最少发现数（先 --list，再真跑）
#   always    ：载荷 完整命令串（不做事先发现数校验）
#   anchors   ：载荷 空格分隔的 `文件:符号` 列表（存在性校验，任一缺失即 FAIL）
#   assets    ：M34 音效素材存在性（缺失打印 `SKIP: missing …`，SKIP≠PASS）
# ---------------------------------------------------------------------------
CHECKS=(
  '01|D1 音效通道行为（--bin mir-client，含 5 个 R60 内联用例）|d1-client-bin|bin|mir-client@21'
  '02|D1 播放器降级/频控/fake-sink（--lib audio）|d1-audio-lib|libfilter|mir-client@audio@19'
  '03|D2 绝对序号消费（--lib ui::projectile）|d2-projectile-lib|libfilter|mir-client@ui::projectile@12'
  '04|D2 事件总数自增/重建归零（--lib net::）|d2-net-lib|libfilter|mir-client@net::@30'
  '05|R59 回归：投射物渲染/消费快照（16）|reg-r59-render|testfile|mir-client@round59_projectile_render@16'
  '06|R58 回归：投射物线格式（7）|reg-r58-wire|testfile|mir-client@round58_projectile_wire@7'
  '07|回归：mir-render 音效映射（9）|reg-mir-render-sound|libfilter|mir-render@sound@9'
  '08|静态：cargo fmt --all --check|fmt|always|cargo fmt --all --check'
  '09|静态：clippy 双包全目标 -D warnings|clippy|always|cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings'
  '10|锚点：R60 三个接线符号存在|anchors|anchors|crates/mir-client/src/ui/mod.rs:take_projectile_sound_cues crates/mir-client/src/app.rs:play_projectile_cues crates/mir-client/src/net.rs:projectile_events_total'
  '11|素材：M34-1/2 真实 .mna（缺失记 SKIP 非 PASS）|m34-assets|assets|-'
)

MAX_CHECKS=${#CHECKS[@]}
PASS_N=0
FAIL_N=0
SKIP_N=0
FAIL_LIST=""
SKIP_LIST=""

# ---------------------------------------------------------------------------
# 参数：--list 只打印清单，不执行、不写盘
# ---------------------------------------------------------------------------
list_commands() {
  echo "# R60 固定验收命令清单（共 $MAX_CHECKS 项；执行：bash docs/testing/round60/run-checks.sh）"
  for spec in "${CHECKS[@]}"; do
    IFS='|' read -r id name slug kind payload <<<"$spec"
    case "$kind" in
      bin)
        pkg="${payload%%@*}"
        min="${payload##*@}"
        printf '%s  cargo test -p %s --bin %s（先 -- --list 校验发现数 ≥%s）\n' "$id" "$pkg" "$pkg" "$min"
        ;;
      testfile)
        pkg="${payload%%@*}"
        rest="${payload#*@}"
        file="${rest%%@*}"
        printf '%s  cargo test -p %s --test %s（先 -- --list 校验发现数）\n' "$id" "$pkg" "$file"
        ;;
      libfilter)
        pkg="${payload%%@*}"
        rest="${payload#*@}"
        filter="${rest%@*}"
        printf '%s  cargo test -p %s --lib %s（先 -- --list 校验发现数）\n' "$id" "$pkg" "$filter"
        ;;
      anchors)
        printf '%s  grep -q <符号> <文件>（共 %d 个锚点）\n' "$id" "$(printf '%s' "$payload" | wc -w)"
        ;;
      assets)
        printf '%s  检查 game-assets/audio/sfx/M34-{1,2}.mna（缺失 = SKIP，非 PASS）\n' "$id"
        ;;
      always|*)
        printf '%s  %s\n' "$id" "$payload"
        ;;
    esac
  done
}

case "${1:-}" in
  --list|-l)
    list_commands
    exit 0
    ;;
  "")
    ;;
  *)
    echo "错误：未知参数 ${1:-}（用法：bash docs/testing/round60/run-checks.sh [--list]）" >&2
    exit 2
    ;;
esac

mkdir -p "$EVID" || {
  echo "错误：无法创建证据目录 $EVID" >&2
  exit 2
}

# ---------------------------------------------------------------------------
# 工具函数
# ---------------------------------------------------------------------------
mark_pass() {
  PASS_N=$((PASS_N + 1))
  echo "  -> PASS"
}

mark_fail() {
  FAIL_N=$((FAIL_N + 1))
  FAIL_LIST="$FAIL_LIST $1"
  echo "  -> FAIL（$2）"
}

mark_skip() {
  SKIP_N=$((SKIP_N + 1))
  SKIP_LIST="$SKIP_LIST $1"
  echo "  -> SKIP（非 PASS：$2）"
}

# 输出中的 `SKIP:` / `PARTIAL:` 字面量 → 非 PASS
has_skip_line() {
  grep -Eq '^(SKIP|PARTIAL):|(SKIP|PARTIAL): ' "$1" 2>/dev/null
}

# exit=0 但 0 用例的假绿防护
has_zero_tests() {
  grep -Eq 'running 0 tests|(^|[^0-9])0 passed' "$1" 2>/dev/null
}

# 测试发现数
count_found() {
  local n
  n=$(grep -c ': test$' "$1" 2>/dev/null || true)
  n=${n:-0}
  echo "$n"
}

# 测试类：先 --list 校验发现数，再真跑（kind=bin/testfile/libfilter）
run_test_check() {
  local id="$1" kind="$2" pkg="$3" target="$4" min="$5"
  local list_log="$EVID/${id}-list.log"
  local run_log="$EVID/${id}-run.log"
  local list_cmd run_cmd found list_rc run_rc

  if [[ "$kind" == "bin" ]]; then
    list_cmd=(cargo test -p "$pkg" --bin "$target" -- --list)
    run_cmd=(cargo test -p "$pkg" --bin "$target")
  elif [[ "$kind" == "testfile" ]]; then
    list_cmd=(cargo test -p "$pkg" --test "$target" -- --list)
    run_cmd=(cargo test -p "$pkg" --test "$target")
  else
    list_cmd=(cargo test -p "$pkg" --lib "$target" -- --list)
    run_cmd=(cargo test -p "$pkg" --lib "$target")
  fi

  {
    echo "# 命令：${list_cmd[*]}"
    echo "# 时间（本地）：$(date '+%Y-%m-%dT%H:%M:%S%z')"
    echo "---"
  } >"$list_log"
  "${list_cmd[@]}" >>"$list_log" 2>&1
  list_rc=$?
  echo "EXIT=$list_rc" >>"$list_log"

  found=$(count_found "$list_log")
  echo "  --list: found=$found min=$min list_exit=$list_rc"

  if [[ $list_rc -ne 0 || "$found" -eq 0 || "$found" -lt "$min" ]]; then
    mark_fail "$id" "发现数校验失败 found=$found min=$min list_exit=$list_rc"
    return
  fi

  {
    echo "# 命令：${run_cmd[*]}"
    echo "# 时间（本地）：$(date '+%Y-%m-%dT%H:%M:%S%z')"
    echo "---"
  } >"$run_log"
  "${run_cmd[@]}" >>"$run_log" 2>&1
  run_rc=$?
  echo "EXIT=$run_rc" >>"$run_log"
  echo "  run: exit=$run_rc（$run_log）"

  if [[ $run_rc -ne 0 ]]; then
    mark_fail "$id" "原始 exit=$run_rc"
    return
  fi
  if has_zero_tests "$run_log"; then
    mark_fail "$id" "exit=0 但输出含 running 0 tests / 0 passed（假绿）"
    return
  fi
  if has_skip_line "$run_log" || has_skip_line "$list_log"; then
    mark_skip "$id" "输出含 SKIP:/PARTIAL: 行"
    return
  fi
  mark_pass
}

# ---------------------------------------------------------------------------
# 主循环
# ---------------------------------------------------------------------------
echo "# R60 验收脚本：共 $MAX_CHECKS 项；证据目录 $EVID"
echo "# HEAD=$(git rev-parse --short HEAD 2>/dev/null || echo unknown) 时间=$(date '+%Y-%m-%dT%H:%M:%S%z')"
echo

for spec in "${CHECKS[@]}"; do
  IFS='|' read -r id name slug kind payload <<<"$spec"
  echo "[$id] $name"

  case "$kind" in
    bin)
      pkg="${payload%%@*}"
      min="${payload##*@}"
      run_test_check "$id" bin "$pkg" "$pkg" "$min"
      ;;
    testfile)
      pkg="${payload%%@*}"
      rest="${payload#*@}"
      file="${rest%%@*}"
      min="${rest##*@}"
      run_test_check "$id" testfile "$pkg" "$file" "$min"
      ;;
    libfilter)
      pkg="${payload%%@*}"
      rest="${payload#*@}"
      filter="${rest%@*}"
      min="${payload##*@}"
      run_test_check "$id" libfilter "$pkg" "$filter" "$min"
      ;;
    always)
      log="$EVID/${id}-run.log"
      {
        echo "# 命令：${payload}"
        echo "# 时间（本地）：$(date '+%Y-%m-%dT%H:%M:%S%z')"
        echo "---"
      } >"$log"
      bash -c "$payload" >>"$log" 2>&1
      rc=$?
      echo "EXIT=$rc" >>"$log"
      echo "  log: $log（exit=$rc）"
      if [[ $rc -ne 0 ]]; then mark_fail "$id" "原始 exit=$rc"
      elif has_skip_line "$log"; then mark_skip "$id" "输出含 SKIP:/PARTIAL: 行"
      else mark_pass; fi
      ;;
    anchors)
      log="$EVID/${id}-run.log"
      miss=0
      {
        echo "# 锚点存在性校验（文件:符号 双锚）"
        echo "# 时间（本地）：$(date '+%Y-%m-%dT%H:%M:%S%z')"
        echo "---"
      } >"$log"
      for pair in $payload; do
        file="${pair%%:*}"
        sym="${pair#*:}"
        if grep -q "$sym" "$file" 2>/dev/null; then
          echo "PASS $file :: $sym" >>"$log"
          echo "  PASS $file :: $sym"
        else
          echo "FAIL $file :: $sym（符号/文件缺失）" >>"$log"
          echo "  FAIL $file :: $sym"
          miss=$((miss + 1))
        fi
      done
      echo "EXIT=$((miss == 0 ? 0 : 1))" >>"$log"
      if [[ $miss -eq 0 ]]; then mark_pass; else mark_fail "$id" "缺失 $miss 个锚点"; fi
      ;;
    assets)
      log="$EVID/${id}-run.log"
      miss=0
      {
        echo "# M34 音效素材存在性（M-4：缺失记 SKIP，SKIP≠PASS）"
        echo "# 时间（本地）：$(date '+%Y-%m-%dT%H:%M:%S%z')"
        echo "---"
      } >"$log"
      for f in game-assets/audio/sfx/M34-1.mna game-assets/audio/sfx/M34-2.mna; do
        if [[ -f "$f" ]]; then
          bytes=$(wc -c <"$f")
          echo "PASS $f bytes=$bytes" >>"$log"
          echo "  PASS $f bytes=$bytes"
        else
          echo "SKIP: missing $(basename "$f")" >>"$log"
          echo "  SKIP: missing $(basename "$f")"
          miss=$((miss + 1))
        fi
      done
      echo "EXIT=$((miss == 0 ? 0 : 1))" >>"$log"
      if [[ $miss -eq 0 ]]; then mark_pass; else mark_skip "$id" "缺 $miss 个 .mna 素材"; fi
      ;;
    *)
      mark_fail "$id" "未知检查类型 $kind"
      ;;
  esac
  echo
done

# ---------------------------------------------------------------------------
# 汇总
# ---------------------------------------------------------------------------
echo "# ============ 汇总 ============"
echo "# 共 $MAX_CHECKS 项：PASS=$PASS_N FAIL=$FAIL_N SKIP=$SKIP_N"
[[ -n "$FAIL_LIST" ]] && echo "# FAIL:$FAIL_LIST"
[[ -n "$SKIP_LIST" ]] && echo "# SKIP:$SKIP_LIST"
if [[ $FAIL_N -eq 0 && $SKIP_N -eq 0 ]]; then
  echo "# 结论：全部 PASS"
  exit 0
fi
echo "# 结论：存在 FAIL/SKIP，判不通过（SKIP≠PASS）"
exit 1
