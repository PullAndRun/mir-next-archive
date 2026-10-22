#!/usr/bin/env python3
"""物品图标完整性门禁（第 42 轮数据清洗配套）。

读取 data_import 写出的 `clean-report.json`（随发布包在 game-assets/data/），
对每个仍留在 `.mnd` 的物品检查 `game-assets/gfx/sprites/item/icon-{image:04}/`：
- 目录不存在、没有 PNG，或 index.json 把所有帧标成 empty=true → 违规；
- 违规项必须在 `target/item-icon-allowlist.txt` 里显式列出（初始为空）。

退出码 0 = 全绿灯；1 = 存在未白名单的无图标模板。
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def icon_available(root: Path, image: int) -> tuple[bool, str]:
    directory = root / f"icon-{image:04}"
    if not directory.is_dir():
        return False, "目录不存在"
    if not any(path.suffix.lower() == ".png" for path in directory.iterdir() if path.is_file()):
        return False, "无 PNG"
    index_path = directory / "index.json"
    if not index_path.is_file():
        return True, ""
    try:
        data = json.loads(index_path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return True, ""
    images = data.get("images")
    if isinstance(images, list) and (
        not images or all(bool(item.get("empty")) for item in images if isinstance(item, dict))
    ):
        return False, "index.json 全部 empty=true"
    return True, ""


def load_allowlist(path: Path) -> tuple[set[int], set[str]]:
    indexes: set[int] = set()
    names: set[str] = set()
    if not path.is_file():
        return indexes, names
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        # 兼容 `403`、`403 HardenedRing`、`name:HardenedRing`
        token = line.split("#", 1)[0].strip()
        if token.isdigit():
            indexes.add(int(token))
            continue
        if token.lower().startswith("name:"):
            names.add(token.split(":", 1)[1].strip().lower())
            continue
        parts = token.split(None, 1)
        if parts and parts[0].isdigit():
            indexes.add(int(parts[0]))
            if len(parts) > 1:
                names.add(parts[1].strip().lower())
        else:
            names.add(token.lower())
    return indexes, names


def main() -> int:
    parser = argparse.ArgumentParser(description="验证 .mnd 中每个物品模板都有图标")
    parser.add_argument("--report", type=Path, default=ROOT / "game-assets/data/clean-report.json")
    parser.add_argument("--icons", type=Path, default=ROOT / "game-assets/gfx/sprites/item")
    parser.add_argument("--allowlist", type=Path, default=ROOT / "target/item-icon-allowlist.txt")
    args = parser.parse_args()

    if not args.report.is_file():
        print(f"FAIL 找不到清洗报告 {args.report}；先运行 data_import", file=sys.stderr)
        return 1
    if not args.icons.is_dir():
        print(f"FAIL 找不到图标目录 {args.icons}", file=sys.stderr)
        return 1
    try:
        report = json.loads(args.report.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        print(f"FAIL 读取清洗报告失败：{error}", file=sys.stderr)
        return 1

    kept = report.get("kept")
    if not isinstance(kept, list) or not kept:
        print("FAIL 清洗报告没有 kept 物品列表", file=sys.stderr)
        return 1

    # 报告必须对应当前 .mnd：manifest 的 items 数量与整包 sha256 都要能对上。
    manifest_path = args.report.parent / "manifest.json"
    if manifest_path.is_file():
        try:
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        except (OSError, ValueError) as error:
            print(f"FAIL 读取 manifest 失败：{error}", file=sys.stderr)
            return 1
        manifest_items = manifest.get("counts", {}).get("items")
        report_items = report.get("counts", {}).get("items_after")
        if manifest_items != report_items:
            print(
                f"FAIL clean-report items_after={report_items} "
                f"与 manifest counts.items={manifest_items} 不一致（报告过期，请重跑 data_import）",
                file=sys.stderr,
            )
            return 1
        report_sha = report.get("mnd_sha256")
        manifest_sha = manifest.get("sha256")
        if report_sha and manifest_sha and report_sha != manifest_sha:
            print("FAIL clean-report 的 mnd_sha256 与 manifest sha256 不一致", file=sys.stderr)
            return 1

    allow_indexes, allow_names = load_allowlist(args.allowlist)
    violations: list[tuple[int, str, int, str]] = []
    for item in kept:
        if not isinstance(item, dict):
            continue
        index = int(item.get("index", 0))
        name = str(item.get("name", ""))
        image = int(item.get("image", 0))
        available, reason = icon_available(args.icons, image)
        if available:
            continue
        if index in allow_indexes or name.lower() in allow_names:
            print(f"ALLOW #{index} {name} image={image}（白名单）")
            continue
        violations.append((index, name, image, reason))

    counts = report.get("counts", {})
    print(
        "物品图标检查：kept={}，违规={}，报告 before={} after={} pruned={} warn={}".format(
            len(kept),
            len(violations),
            counts.get("items_before"),
            counts.get("items_after"),
            counts.get("pruned"),
            counts.get("warn"),
        )
    )
    for index, name, image, reason in violations:
        print(f"FAIL #{index} {name!r} image={image}: {reason}", file=sys.stderr)
    if violations:
        print(
            f"共 {len(violations)} 个无图标模板；请补图、删引用后重导，"
            f"或在 {args.allowlist} 显式白名单（需说明原因）",
            file=sys.stderr,
        )
        return 1
    print("OK 所有保留物品模板都有图标")
    return 0


if __name__ == "__main__":
    sys.exit(main())
