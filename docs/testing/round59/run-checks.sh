#!/usr/bin/env bash
# =============================================================================
# 《传奇-Next》R59（第 59 轮）人工验收脚本
#
# 用途：本轮投射物（projectile）最小闭环的本地人工验收，固定 9 条命令：
#   G1 事件消费 / G2 插值 / G3 命中 570 / G4 音效映射（M34-1/M34-2）/
#   G5 magic.wzl 帧集素材 / 回归（R58 wire、mir-server magic）+ fmt/clippy。
#
# 写法要点：
#   - 逐条执行并保存「命令 + 原始 exit 码 + 原始输出」，不做管道截流（管道会丢退出码）；
#   - 只开 set -u（未定义变量即报错），不启用 errexit：任一命令失败仍继续跑完，
#     最后统一汇总；
#   - 测试类命令先做发现数校验（cargo test -- --list），发现数 0 或低于门槛即 FAIL；
#     执行期再做「0 命中假绿」防护：exit=0 但输出 running 0 tests / 0 passed 判 FAIL；
#   - 逻辑门（G1–G4、G5）一律不得 SKIP：素材/用例缺失直接 FAIL（派发件 C4）。
#
# 边界：本脚本不进 CI（CI 以 tools/dev/ci.ps1 为准），仅供本轮人工验收与留证；
#       不改源码、不 commit、不跑数据库。证据默认写入 target/round59/d4/script/。
#
# 用法：
#   bash docs/testing/round59/run-checks.sh          # 执行全部 9 条命令并写证据
#   bash docs/testing/round59/run-checks.sh --list   # 只打印固定命令清单（零执行、零写入）
#
# 环境变量：
#   ROUND59_EVIDENCE  证据目录（缺省 target/round59/d4/script）
#
# 退出码：0=9 条全部 PASS；1=有 FAIL；2=脚本自身错误
# =============================================================================
set -u

# ---------------------------------------------------------------------------
# 固定命令清单（唯一事实来源；--list 与执行共用同一份数据）
# 字段：序号|说明|日志 slug|类型|载荷
#   类型 testfile  ：载荷 包名:测试文件名:最少发现数（先 --list 校验，再真跑）
#   类型 libfilter ：载荷 包名:lib 用例过滤串:最少发现数（先 --list 校验，再真跑）
#   类型 always    ：载荷为完整命令串
#   类型 pyscript  ：载荷为内嵌 python 源码（py -3 执行素材校验，源码落证据目录）
# 说明：9 = 素材校验（py -3，等价于 Rust 侧 round59_projectile_assets 的独立复核）。
# ---------------------------------------------------------------------------
CHECKS=(
  '01|客户端投射物逻辑单测（G1/G2）|client-projectile-lib|libfilter|mir-client:projectile:8'
  '02|客户端音效单测（G4）|client-sound-lib|libfilter|mir-client:sound:8'
  '03|R59 渲染/消费快照测试（G1/G2/G3/G4）|round59-projectile-render|testfile|mir-client:round59_projectile_render:16'
  '04|R59 magic 帧集素材测试（G5）|round59-projectile-assets|testfile|mir-client:round59_projectile_assets:6'
  '05|R58 投射物接线回归|round58-projectile-wire|testfile|mir-client:round58_projectile_wire:7'
  '06|服务端 magic 单测|server-magic-lib|libfilter|mir-server:magic:43'
  '07|格式检查（fmt）|fmt|always|cargo fmt --check'
  '08|Clippy 三包全目标（-D warnings）|clippy|always|cargo clippy -p mir-client -p mir-render -p mir-server --all-targets -- -D warnings'
  '09|素材校验（game-assets magic 帧集 + resource/magic.wzl）|magic-assets|pyscript|__PY_MAGIC_ASSETS__'
)

# ---------------------------------------------------------------------------
# 参数：--list 只列清单，不执行、不写盘
# ---------------------------------------------------------------------------
list_commands() {
  echo "# R59 固定验收命令清单（共 ${#CHECKS[@]} 条；执行：bash docs/testing/round59/run-checks.sh）"
  for spec in "${CHECKS[@]}"; do
    IFS='|' read -r id name slug kind payload <<<"$spec"
    case "$kind" in
      testfile)
        pkg="${payload%%:*}"
        rest="${payload#*:}"
        file="${rest%%:*}"
        printf '%s  cargo test -p %s --test %s\n' "$id" "$pkg" "$file"
        ;;
      libfilter)
        pkg="${payload%%:*}"
        rest="${payload#*:}"
        filter="${rest%%:*}"
        printf '%s  cargo test -p %s --lib %s\n' "$id" "$pkg" "$filter"
        ;;
      pyscript)
        printf '%s  py -3 <证据目录>/logs/%s-magic-assets.py（magic 帧集与 resource/magic.wzl 校验）\n' "$id" "$id"
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
    echo "错误：未知参数 ${1:-}（用法：bash docs/testing/round59/run-checks.sh [--list]）" >&2
    exit 2
    ;;
esac

# ---------------------------------------------------------------------------
# 内嵌 python：素材校验（与 Rust round59_projectile_assets 同口径，独立复核）
# 只读；缺素材/缺 resource 直接非 0，绝不 SKIP 假绿。
# ---------------------------------------------------------------------------
PY_MAGIC_ASSETS=$(cat <<'PY'
import hashlib
import json
import pathlib
import sys

BASE = pathlib.Path("game-assets/gfx/sprites/effect/magic")
INDEX = BASE / "index.json"
WZL = pathlib.Path("resource/data/magic.wzl")
EXPECTED_COUNT = 106
EXPECTED_FRAMESET = "e465026a50ee6f86d6762e3959386fa0132ffabf63da9c92afeebdf3d52a1e98"
EXPECTED_WZL_SHA = "3b8a2dba9a1246e90a663e712a84b488fbee924ad6ea70af3be986f00a02ab25"
EXPECTED_WZL_BYTES = 6275819
EXPECTED_INDICES = sorted(
    [410 + d * 10 + f for d in range(16) for f in range(6)] + list(range(570, 580))
)

errors = []


def fail(message):
    errors.append(message)
    print("FAIL " + message)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


if not INDEX.is_file():
    fail("missing frame index: %s" % INDEX)
    print("MAGIC ASSET GATE FAILED: no %s" % INDEX)
    sys.exit(1)

index = json.loads(INDEX.read_text(encoding="utf-8"))
images = index.get("images", [])
print("PASS index.json readable: %s" % INDEX)

if len(images) == EXPECTED_COUNT:
    print("PASS frame count == %d" % EXPECTED_COUNT)
else:
    fail("frame count %d != %d" % (len(images), EXPECTED_COUNT))

if index.get("count") == EXPECTED_COUNT and index.get("written_png") == EXPECTED_COUNT:
    print("PASS index count/written_png == %d" % EXPECTED_COUNT)
else:
    fail("index count/written_png = %r/%r" % (index.get("count"), index.get("written_png")))

if index.get("missing") == []:
    print("PASS missing == []")
else:
    fail("missing frames: %r" % (index.get("missing"),))
if index.get("skipped_empty") == 0:
    print("PASS skipped_empty == 0")
else:
    fail("skipped_empty = %r" % (index.get("skipped_empty"),))
if index.get("indices_out_of_range") == 0:
    print("PASS indices_out_of_range == 0")
else:
    fail("indices_out_of_range = %r" % (index.get("indices_out_of_range"),))

actual_indices = sorted(int(image["index"]) for image in images)
if actual_indices == EXPECTED_INDICES:
    print("PASS frame index set == 410+d*10..415+d*10 (d=0..15) + 570..579")
else:
    fail("frame index set mismatch: %r" % (actual_indices,))

forbidden = set(range(416, 420)) | set(range(566, 570))
if forbidden.isdisjoint(actual_indices):
    print("PASS empty frames 416-419 / 566-569 absent")
else:
    fail("forbidden empty frames present: %r" % (sorted(forbidden & set(actual_indices)),))

on_disk = sorted(path.name for path in BASE.glob("*.png"))
expected_names = sorted("0%d.png" % frame for frame in EXPECTED_INDICES)
if on_disk == expected_names:
    print("PASS on-disk PNG list matches the %d indexed frames" % EXPECTED_COUNT)
else:
    fail("on-disk PNG list mismatch: %d files" % len(on_disk))

fingerprint_lines = []
for image in images:
    png = pathlib.Path(image["png"])
    path = BASE / png.name
    if not path.is_file():
        fail("missing PNG: %s" % path)
        continue
    data = path.read_bytes()
    digest = sha256(data)
    if digest != image["png_sha256"]:
        fail("%s sha256 %s != recorded %s" % (png.name, digest, image["png_sha256"]))
    width = int.from_bytes(data[16:20], "big") if data[:8] == b"\x89PNG\r\n\x1a\n" else 0
    height = int.from_bytes(data[20:24], "big") if data[:8] == b"\x89PNG\r\n\x1a\n" else 0
    if width == 0 or height == 0:
        fail("%s has no real size (%dx%d)" % (png.name, width, height))
    if width != int(image["width"]) or height != int(image["height"]):
        fail("%s IHDR %dx%d != recorded %dx%d"
             % (png.name, width, height, int(image["width"]), int(image["height"])))
    fingerprint_lines.append("%s %d %s" % (png.name, len(data), digest))
print("PASS per-PNG sha256 + non-zero IHDR size for %d frames" % len(fingerprint_lines))

fingerprint = sha256("\n".join(sorted(fingerprint_lines)).encode("utf-8"))
if fingerprint == EXPECTED_FRAMESET:
    print("PASS frameset_sha256 == %s" % EXPECTED_FRAMESET)
else:
    fail("frameset_sha256 %s != %s" % (fingerprint, EXPECTED_FRAMESET))

if not WZL.is_file():
    fail("missing resource magic.wzl: %s" % WZL)
else:
    wzl = WZL.read_bytes()
    if len(wzl) == EXPECTED_WZL_BYTES:
        print("PASS resource/data/magic.wzl size == %d" % EXPECTED_WZL_BYTES)
    else:
        fail("magic.wzl size %d != %d" % (len(wzl), EXPECTED_WZL_BYTES))
    digest = sha256(wzl)
    if digest == EXPECTED_WZL_SHA:
        print("PASS resource/data/magic.wzl sha256 == %s" % EXPECTED_WZL_SHA)
    else:
        fail("magic.wzl sha256 %s != %s" % (digest, EXPECTED_WZL_SHA))

if errors:
    print("MAGIC ASSET GATE FAILED: %d error(s)" % len(errors))
    sys.exit(1)
print("MAGIC ASSET GATE OK: %d magic frames verified" % EXPECTED_COUNT)
PY
)

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
E="${ROUND59_EVIDENCE:-target/round59/d4/script}"
mkdir -p "$TMP" "$E/logs" || exit 2

echo "仓库根：$REPO_ROOT"
echo "HEAD  ：$(git rev-parse --short HEAD 2>/dev/null || echo 未知)"
echo "证据  ：$E"

# 0 tests 假绿防护正则（R57 审计 M3 的教训）
ZERO_TESTS_RE='running 0 tests|test result: ok\. 0 passed'

# 单条命令执行：输出写 logs/<序号>-<slug>.log，原始 exit 记入全局 rc
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

# 发现数：cargo test ... -- --list，输出 "<count> <log>"；list 命令失败时 count=-1
discover_count() {
  local kind="$1" payload="$2" log="$3" rc count
  case "$kind" in
    testfile)
      pkg="${payload%%:*}"
      rest="${payload#*:}"
      file="${rest%%:*}"
      display="cargo test -p $pkg --test $file -- --list"
      CMD=(cargo test -p "$pkg" --test "$file" -- --list)
      ;;
    libfilter)
      pkg="${payload%%:*}"
      rest="${payload#*:}"
      filter="${rest%%:*}"
      display="cargo test -p $pkg --lib $filter -- --list"
      CMD=(cargo test -p "$pkg" --lib "$filter" -- --list)
      ;;
    *)
      printf '%s \n' "-1"
      return
      ;;
  esac
  run_logged "$log" "$display" "${CMD[@]}"
  rc=$?
  if [ "$rc" -ne 0 ]; then
    printf '%s \n' "-1"
    return
  fi
  count=$(grep -cE ': test$' "$log")
  printf '%s \n' "$count"
}

# ---------------------------------------------------------------------------
# 前置发现数校验（逻辑门不得 0 命中；发现数不足即 FAIL）
# ---------------------------------------------------------------------------
echo
echo "--- 发现数校验（cargo test -- --list）---"
DISCOVERY="$E/logs/00-discovery.tsv"
printf '序号\t类型\t最少\t实际\t结果\n' >"$DISCOVERY"
for spec in "${CHECKS[@]}"; do
  IFS='|' read -r id name slug kind payload <<<"$spec"
  case "$kind" in
    testfile|libfilter)
      min="${payload##*:}"
      got="$(discover_count "$kind" "$payload" "$E/logs/${id}-list.log" | tr -d ' ')"
      if [ "$got" = "-1" ]; then
        verdict="FAIL"
      elif [ "$got" -lt "$min" ]; then
        verdict="FAIL"
      else
        verdict="PASS"
      fi
      printf '%s\t%s\t%s\t%s\t%s\n' "$id" "$kind" "$min" "$got" "$verdict" >>"$DISCOVERY"
      printf '%-2s %-4s list=%s（门槛 %s）\n' "$id" "$verdict" "$got" "$min"
      ;;
  esac
done

# ---------------------------------------------------------------------------
# 主循环（逐条执行、逐条记原始 exit）
# ---------------------------------------------------------------------------
SUMMARY="$E/01-exit-summary.tsv"
printf '序号\t状态\t原始exit\t备注\t日志\t命令\n' >"$SUMMARY"

pass=0
fail=0
total=0

add_result() {
  local id="$1" status="$2" rc="$3" note="$4" log="$5" display="$6" name="$7"
  total=$((total + 1))
  case "$status" in
    PASS) pass=$((pass + 1)) ;;
    FAIL) fail=$((fail + 1)) ;;
    *) ;;
  esac
  printf '%-2s %-4s exit=%-4s %-42s %s\n' "$id" "$status" "$rc" "logs/${id}-$(basename "$log" .log | sed "s/^${id}-//")" "$name"
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$status" "$rc" "$note" "logs/${id}-$(basename "$log" .log | sed "s/^${id}-//")" "$display" >>"$SUMMARY"
}

for spec in "${CHECKS[@]}"; do
  IFS='|' read -r id name slug kind payload <<<"$spec"
  log="$E/logs/${id}-${slug}.log"
  rc=""
  status=""
  note=""

  case "$kind" in
    testfile)
      pkg="${payload%%:*}"
      rest="${payload#*:}"
      file="${rest%%:*}"
      if [ ! -f "crates/$pkg/tests/$file.rs" ]; then
        status="FAIL"
        note="用例文件未落盘：crates/$pkg/tests/$file.rs（逻辑门不得 SKIP）"
        { printf '$ cargo test -p %s --test %s\n\nFAIL: %s\n' "$pkg" "$file" "$note"; } >"$log" 2>&1
        rc="N/A"
      else
        display="cargo test -p $pkg --test $file"
        CMD=(cargo test -p "$pkg" --test "$file")
      fi
      ;;
    libfilter)
      pkg="${payload%%:*}"
      rest="${payload#*:}"
      filter="${rest%%:*}"
      display="cargo test -p $pkg --lib $filter"
      CMD=(cargo test -p "$pkg" --lib "$filter")
      ;;
    always)
      display="$payload"
      CMD=(bash -c "$payload")
      ;;
    pyscript)
      pyfile="$E/logs/${id}-magic-assets.py"
      printf '%s\n' "$PY_MAGIC_ASSETS" >"$pyfile" || exit 2
      display="py -3 $pyfile"
      CMD=(py -3 "$pyfile")
      ;;
    *)
      echo "内部错误：未知命令类型 $kind" >&2
      exit 2
      ;;
  esac

  if [ -z "$status" ]; then
    run_logged "$log" "$display" "${CMD[@]}"
    rc=$?
    if [ "$rc" -eq 0 ]; then
      if [ "$kind" != "always" ] && grep -qE "$ZERO_TESTS_RE" "$log"; then
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

  add_result "$id" "$status" "$rc" "$note" "$log" "$display" "$name"
done

# ---------------------------------------------------------------------------
# 汇总
# ---------------------------------------------------------------------------
echo
echo "================= R59 验收汇总 ================="
printf '总计 %d 条：PASS=%d FAIL=%d\n' "$total" "$pass" "$fail"
printf '明细表：%s\n' "$SUMMARY"
printf '发现数：%s\n' "$DISCOVERY"
printf '日志目录：%s/logs/\n' "$E"
if [ "$fail" -gt 0 ]; then
  printf '总 EXIT=1（%d 条 FAIL，见 01-exit-summary.tsv 与 logs/）\n' "$fail"
  exit 1
fi
echo '总 EXIT=0（9 条全部 PASS）'
exit 0
