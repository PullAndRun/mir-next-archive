#!/usr/bin/env python3
"""**运行时禁止引用老资源格式**的守卫（用户规则）。

规则（`docs/assets/asset-formats.md` §0）：

* 运行时 crate（`mir-client` / `mir-render` / `mir-world` / `mir-server`）**不得**出现
  `resource/` 路径字面量，也不得按老格式扩展名（`.wzl/.wix/.wil/.Lib/.wav/Server.MirDB`）
  直接打开文件——运行时只读我们自己的资源库（`game-assets/`）。
* 老格式只允许出现在**导入/构建**代码里：`mir-asset` 的 `legacy` feature、`mir-tools`、
  `tools/**` 脚本。

守卫形态：**棘轮**。历史遗留项写在 `tools/dev/legacy-runtime-allowlist.txt`
（`<相对路径>:<内容片段>`），只许减少不许增加；修掉一条就从 allowlist 删一条。

用法：

    python tools/dev/check_legacy_refs.py            # 检查（CI 用，超基线 exit 1）
    python tools/dev/check_legacy_refs.py --list     # 只打印命中
    python tools/dev/check_legacy_refs.py --update   # 把当前命中写成新基线（仅修完代码后用）
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ALLOWLIST = ROOT / "tools" / "dev" / "legacy-runtime-allowlist.txt"

RUNTIME_CRATES = ("mir-client", "mir-render", "mir-world", "mir-server")

# 老格式的文件名/路径特征（只查运行时 crate 的源码）。
PATTERNS = [
    re.compile(r'"resource/'),
    re.compile(r"join\(\"resource"),
    re.compile(r"\.wz[lx]\b"),
    re.compile(r"\.wil\b"),
    re.compile(r"\.Lib\b"),
    re.compile(r"Server\.MirDB"),
]

# 源码里提到这些词的注释/文档不算违规（只查代码里的字符串/路径用法）。
def is_comment_or_doc(line: str) -> bool:
    stripped = line.lstrip()
    return stripped.startswith("//") or stripped.startswith("///") or stripped.startswith("//!")


def hits() -> list[tuple[str, str]]:
    found: list[tuple[str, str]] = []
    for crate in RUNTIME_CRATES:
        src = ROOT / "crates" / crate / "src"
        if not src.is_dir():
            continue
        for path in sorted(src.rglob("*.rs")):
            rel = path.relative_to(ROOT).as_posix()
            for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                if is_comment_or_doc(line):
                    continue
                if any(pattern.search(line) for pattern in PATTERNS):
                    found.append((rel, f"{number}:{line.strip()}"))
    return found


def load_allowlist() -> list[tuple[str, str]]:
    if not ALLOWLIST.exists():
        return []
    entries: list[tuple[str, str]] = []
    for line in ALLOWLIST.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        path, _, snippet = line.partition("|")
        entries.append((path.strip(), snippet.strip()))
    return entries


def snippet_of(entry: tuple[str, str]) -> str:
    """allowlist 的片段是 `行号:内容`，比对时只用内容（行号会漂）。"""
    _, text = entry
    return text.split(":", 1)[1].strip() if ":" in text else text


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--list", action="store_true", help="只打印命中")
    parser.add_argument("--update", action="store_true", help="把当前命中写成新基线")
    parser.add_argument("--fail-above", type=int, default=None, help="允许的上限（默认=基线条数）")
    args = parser.parse_args()

    found = hits()
    if args.update:
        lines = ["# 运行时老资源格式引用基线（棘轮：只许减少）。", "# 格式：<相对路径>|<行号>:<内容>", ""]
        lines += [f"{path}|{text}" for path, text in found]
        ALLOWLIST.write_text("\n".join(lines) + "\n", encoding="utf-8")
        print(f"[update] 基线已写入 {ALLOWLIST.relative_to(ROOT)}：{len(found)} 条")
        return 0

    if args.list:
        for path, text in found:
            print(f"{path}:{text}")
        print(f"合计 {len(found)} 条")
        return 0

    baseline = load_allowlist()
    baseline_snippets = {(path, snippet_of((path, text))) for path, text in baseline}
    current = {(path, text.split(":", 1)[1].strip()) for path, text in found}
    new = sorted(current - baseline_snippets)
    fixed = baseline_snippets - current

    limit = args.fail_above if args.fail_above is not None else len(baseline)
    print(f"运行时老格式引用：当前 {len(current)} 条 / 基线 {len(baseline)} 条（上限 {limit}）")
    if fixed:
        print(f"[提示] 已修好 {len(fixed)} 条，可从 allowlist 删除：")
        for path, text in sorted(fixed):
            print(f"  - {path}: {text}")
    if new:
        print(f"[失败] 新增 {len(new)} 条（运行时不得引用老资源格式）：")
        for path, text in new:
            print(f"  + {path}: {text}")
        return 1
    if len(current) > limit:
        print(f"[失败] 当前 {len(current)} 条超过上限 {limit}")
        return 1
    print("[OK] 运行时未新增老格式引用")
    return 0


if __name__ == "__main__":
    sys.exit(main())
