#!/usr/bin/env python3
"""文档一致性检查（仅标准库）。

校验三件事：

1. `README.md` 与 `docs/**/*.md` 里的**相对链接**都指向真实存在的文件/目录；
   外链（http/https/mailto/...）、纯锚点与代码块里的示例会跳过。
2. README 的「文档」表确实引用了指定文档（默认 `docs/world/market.md`、
   `docs/world/trade.md`，可用 `--require` 追加）。
3. `docs/world/market.md` 里写的市场包编号与 `mir-protocol` 的 `INDEX` 常量一致
   （客户端 70-76、服务端 154-159）。

用法：

```powershell
py tools/docs/check_links.py
py tools/docs/check_links.py --require docs/world/market.md,docs/world/trade.md,docs/world/aoi.md
```

退出码 0 = 全部通过，1 = 有缺失或编号不一致。
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
README = ROOT / "README.md"
DOCS = ROOT / "docs"

# Markdown 链接/图片：[label](target) / ![alt](target)；允许 <target> 与可选 title。
LINK_RE = re.compile(r"!?\[[^\]]*\]\(\s*(<[^>]+>|[^)\s]+)(?:\s+\"[^\"]*\")?\s*\)")
FENCE_RE = re.compile(r"```.*?```", re.DOTALL)
SKIP_PREFIXES = (
    "http://",
    "https://",
    "mailto:",
    "tel:",
    "data:",
    "#",
)

# market.md 必须与 mir-protocol 的 INDEX 常量一致（Crystal 的包编号）。


def strip_code(text: str) -> str:
    """去掉围栏代码块，避免把示例里的链接当真实链接。"""
    return FENCE_RE.sub("", text)


def iter_markdown_files() -> list[Path]:
    files = [README]
    if DOCS.is_dir():
        files.extend(sorted(DOCS.rglob("*.md")))
    return [path for path in files if path.is_file()]


def check_relative_links(errors: list[str]) -> int:
    checked = 0
    for path in iter_markdown_files():
        text = strip_code(path.read_text(encoding="utf-8"))
        for match in LINK_RE.finditer(text):
            raw = match.group(1)
            if raw.startswith(SKIP_PREFIXES):
                continue
            target = raw.strip("<>").split("#", 1)[0].strip()
            if not target or target.startswith(SKIP_PREFIXES):
                continue
            resolved = (path.parent / target).resolve()
            checked += 1
            if not resolved.exists():
                rel = path.relative_to(ROOT)
                errors.append(f"断链：{rel} -> {raw}")
    return checked


def check_readme_docs(required: list[str], errors: list[str]) -> None:
    text = README.read_text(encoding="utf-8")
    for target in required:
        expected = f"]({target})"
        if expected not in text:
            errors.append(f"README 文档表缺少：{target}")


def parse_packet_indices(packet_file: Path) -> dict[str, int]:
    """从生成的包文件里解析 `pub struct X {}` + `impl X { pub const INDEX }`。"""
    text = packet_file.read_text(encoding="utf-8")
    pattern = re.compile(
        r"pub struct (\w+)\s*\{[^{}]*\}\s*\n\s*impl \1\s*\{.*?pub const INDEX: i16 = (\d+);",
        re.DOTALL,
    )
    indices: dict[str, int] = {}
    for match in pattern.finditer(text):
        indices[match.group(1)] = int(match.group(2))
    return indices


def check_market_packets(errors: list[str]) -> None:
    client = parse_packet_indices(
        ROOT / "crates/mir-protocol/src/packets/client.rs"
    )
    manual = parse_packet_indices(
        ROOT / "crates/mir-protocol/src/manual/client.rs"
    )
    server = parse_packet_indices(
        ROOT / "crates/mir-protocol/src/packets/server.rs"
    )
    client.update(manual)  # MarketSearch 是手写包

    market_md = (ROOT / "docs/world/market.md").read_text(encoding="utf-8")
    expected = {
        "ConsignItem": (client.get("ConsignItem"), 70, "客户端"),
        "MarketSearch": (client.get("MarketSearch"), 71, "客户端"),
        "MarketRefresh": (client.get("MarketRefresh"), 72, "客户端"),
        "MarketPage": (client.get("MarketPage"), 73, "客户端"),
        "MarketBuy": (client.get("MarketBuy"), 74, "客户端"),
        "MarketGetBack": (client.get("MarketGetBack"), 75, "客户端"),
        "MarketSellNow": (client.get("MarketSellNow"), 76, "客户端"),
        "NPCConsign": (server.get("NPCConsign"), 154, "服务端"),
        "NPCMarket": (server.get("NPCMarket"), 155, "服务端"),
        "NPCMarketPage": (server.get("NPCMarketPage"), 156, "服务端"),
        "MarketFail": (server.get("MarketFail"), 158, "服务端"),
        "MarketSuccess": (server.get("MarketSuccess"), 159, "服务端"),
    }
    for name, (actual, wanted, side) in expected.items():
        if actual != wanted:
            errors.append(
                f"协议 {side}包 {name} 的 INDEX={actual}，预期 {wanted}"
            )
            continue
        if not re.search(
            rf"{re.escape(name)}[^\n]{{0,180}}[（(]{wanted}[)）]", market_md
        ):
            errors.append(
                f"docs/world/market.md 未把 {side}包 {name} 标成 ({wanted})"
            )

    # 服务端 ConsignItem（157）与客户端同名，单独检查一次。
    actual = server.get("ConsignItem")
    if actual != 157:
        errors.append(f"协议服务端包 ConsignItem 的 INDEX={actual}，预期 157")
    elif not re.search(r"S\.ConsignItem[^\n]{0,180}[（(]157[)）]", market_md):
        errors.append("docs/world/market.md 未把服务端 ConsignItem 标成 (157)")


def main() -> int:
    parser = argparse.ArgumentParser(description="校验文档链接与市场包编号")
    parser.add_argument(
        "--require",
        default="docs/world/market.md,docs/world/trade.md",
        help="README 文档表必须引用的文档，逗号分隔",
    )
    args = parser.parse_args()
    required = [item.strip() for item in args.require.split(",") if item.strip()]

    errors: list[str] = []
    checked = check_relative_links(errors)
    check_readme_docs(required, errors)
    if (ROOT / "docs/world/market.md").is_file():
        check_market_packets(errors)

    if errors:
        print(f"[FAIL] {len(errors)} 个问题（已检查 {checked} 条相对链接）")
        for item in errors:
            print(f"  - {item}")
        return 1
    print(f"[OK] {checked} 条相对链接、{len(required)} 篇 README 文档引用、市场包编号全部一致")
    return 0


if __name__ == "__main__":
    sys.exit(main())
