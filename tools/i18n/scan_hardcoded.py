#!/usr/bin/env python3
"""扫描服务端源码里的「硬编码中文」字符串字面量，产出 i18n 收口清单。

为什么需要它：`AGENTS.md` 要求玩家可见文本一律走 `mir_core::i18n`，但历史代码里仍有
大量中文字面量直接拼进包/日志/聊天。人工登记容易漏，这个脚本给出一份**可复现**的底稿，
`docs/i18n.md` 的收口清单以它为准逐条勾掉。

判定口径（故意保守，宁可多报）：
* 只看 `crates/mir-server/src/**.rs`；
* 跳过整行注释（`//`、`///`、`//!`）与块注释里的行；
* 只看字符串字面量 `"..."` 与 `r#"..."#`，字面量里含 CJK 统一表意文字才算；
* `#[cfg(test)]` 之后的行单独标记 `test`，方便过滤（测试里的中文不算债）；
* `--player-visible` 会跳过日志宏那一行（`tracing::*!` / `debug!` / `info!` / `warn!` /
  `error!` / `trace!`），把「玩家看得见」的部分单独拎出来——那才是需要收口的债。

用法::

    py tools/i18n/scan_hardcoded.py                 # 全部，按文件分组
    py tools/i18n/scan_hardcoded.py --no-test       # 排除测试模块里的命中
    py tools/i18n/scan_hardcoded.py --player-visible  # 只看玩家可见（排除日志行）
    py tools/i18n/scan_hardcoded.py --markdown --player-visible  # 直接给 docs/i18n.md 用

退出码恒为 0（这是清点工具，不当作门禁）。
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SERVER_SRC = ROOT / "crates" / "mir-server" / "src"

# 字符串字面量：普通 "..."（允许转义）与原始 r#"..."#。
STRING_RE = re.compile(r'r#*"(?P<raw>.*?)"#*|"(?P<plain>(?:\\.|[^"\\])*)"')
CJK_RE = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff]")
TEST_START_RE = re.compile(r"^\s*#\[cfg\(test\)\]")
# 日志宏：这些行的中文是给运维看的，不是玩家可见文本。
LOG_RE = re.compile(r"\b(tracing::|debug!|info!|warn!|error!|trace!)")
# 开发者可见文本的常见形态：thiserror 的 Display、断言与 panic、启动期 expect。
# `--strict` 会把它们一起排除，剩下的才该真正归零（CI 用它当棘轮）。
DEV_RE = re.compile(r"#\[error\(|panic!|\.expect\(|assert!\(|assert_eq!\(|anyhow!|bail!\(")


def strip_comment(line: str) -> str | None:
    """去掉整行注释；不是注释返回原文。"""
    stripped = line.lstrip()
    if stripped.startswith("//"):
        return None
    return line


def scan_file(
    path: Path, include_tests: bool, player_visible: bool, strict: bool
) -> list[tuple[int, str, bool]]:
    hits: list[tuple[int, str, bool]] = []
    in_test = False
    in_block_comment = False
    log_depth = 0  # >0 表示当前行仍在某个日志宏的括号里
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if in_block_comment:
            if "*/" in line:
                in_block_comment = False
            continue
        stripped = line.lstrip()
        if stripped.startswith("/*"):
            if "*/" not in stripped:
                in_block_comment = True
            continue
        if TEST_START_RE.match(line):
            in_test = True
        body = strip_comment(line)
        if body is None:
            continue
        # 日志宏可能跨多行：从出现宏的那一行起，按括号平衡一直跳过，直到调用结束。
        if player_visible:
            if log_depth > 0 or LOG_RE.search(body):
                log_depth += body.count("(") - body.count(")")
                if log_depth < 0:
                    log_depth = 0
                continue
        if player_visible and LOG_RE.search(body):
            continue
        # `--strict`：再排除 thiserror Display / 断言 / panic / expect 这类开发者可见文本。
        if strict and DEV_RE.search(body):
            continue
        for match in STRING_RE.finditer(body):
            text = match.group("raw") if match.group("raw") is not None else match.group("plain")
            if text and CJK_RE.search(text):
                if in_test and not include_tests:
                    continue
                hits.append((number, text.strip(), in_test))
    return hits


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--no-test", action="store_true", help="排除 #[cfg(test)] 之后的行")
    parser.add_argument(
        "--player-visible",
        action="store_true",
        help="排除日志宏所在行，只留玩家可见文本",
    )
    parser.add_argument("--markdown", action="store_true", help="输出 markdown 表格")
    parser.add_argument(
        "--strict",
        action="store_true",
        help="在 --player-visible 之上再排除 thiserror Display / 断言 / panic / expect",
    )
    parser.add_argument(
        "--fail-above",
        type=int,
        default=None,
        metavar="N",
        help="命中数超过 N 时以退出码 1 结束（CI 门禁用）",
    )
    args = parser.parse_args(argv)

    files = sorted(SERVER_SRC.rglob("*.rs"))
    if not files:
        print(f"没找到源文件：{SERVER_SRC}", file=sys.stderr)
        return 0

    total = 0
    if args.markdown:
        print("| 文件:行 | 原文 | 建议 TextKey | 参数占位 |")
        print("| --- | --- | --- | --- |")
    for path in files:
        hits = scan_file(
            path,
            include_tests=not args.no_test,
            player_visible=args.player_visible,
            strict=args.strict,
        )
        if not hits:
            continue
        rel = path.relative_to(ROOT).as_posix()
        if not args.markdown:
            print(f"\n{rel}（{len(hits)} 处）")
        for number, text, in_test in hits:
            total += 1
            marker = "（test）" if in_test else ""
            if args.markdown:
                # 建议名留空，由礼部/实现者填；原文里的竖线转义，避免破坏表格。
                safe = text.replace("|", "\\|").replace("\n", " ")
                print(f"| {rel}:{number} | `{safe}` |  |  |")
            else:
                print(f"  {number}: {text}{marker}")

    print(f"\n合计 {total} 处硬编码中文（{len(files)} 个文件已扫描）")
    if args.fail_above is not None and total > args.fail_above:
        print(
            f"[FAIL] 命中 {total} 处，超过上限 {args.fail_above}；"
            "新增玩家可见文本请走 mir_core::i18n（见 docs/i18n.md）",
            file=sys.stderr,
        )
        return 1
    if args.fail_above is not None:
        print(f"[OK] 命中 {total} 处，未超过上限 {args.fail_above}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
