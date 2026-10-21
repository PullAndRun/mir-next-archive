#!/usr/bin/env python3
"""校验拆包产物（新布局）是否与 `used-assets.json` 计划和独立解码样例一致。

检查项：
1. 每个 output 目录都有 `index.json`，PNG 数与 index 非空条目数一致；
2. PNG 数 = 计划 `loaded_frames`，`loaded + missing = requested`；
3. 分类体积（gfx/tiles、gfx/sprites、gfx/ui、maps、audio、data/server、fonts）；
4. 抽样对账：新布局 `index.json.images[].rgb_sha256/rgba_sha256` 与
   `--sample-dir`（独立 Python 解码产物）逐 index 比较；
5. 旧布局留存统计（只报告，不判失败）。

产物：`--out` JSON + 结论；`--strict` 时有 issue 退出码 1。

```powershell
py tools/assets/verify_layout.py --plan target/used-assets.json --game-assets game-assets
py tools/assets/verify_layout.py --samples gfx/tiles/tiles0,gfx/sprites/monster/mon-0130
```
"""

from __future__ import annotations

import argparse
import json
from collections import Counter
from pathlib import Path

DEFAULT_SAMPLES = "gfx/tiles/tiles0,gfx/sprites/monster/mon-0130,gfx/sprites/item/icon-0054"


def images_of(index_data: dict) -> list[dict]:
    return index_data.get("images") or index_data.get("entries") or []


def directory_usage(directory: Path, skip: Path | None = None) -> tuple[int, int]:
    if not directory.is_dir():
        return 0, 0
    files = [
        path
        for path in directory.rglob("*")
        if path.is_file() and not (skip is not None and skip in path.parents)
    ]
    return len(files), sum(path.stat().st_size for path in files)


def sample_compare(game: Path, sample_dir: Path | None, dest: str) -> dict:
    if sample_dir is None:
        return {"dest": dest, "status": "skip（未给 --sample-dir）"}
    ours_path = sample_dir / dest / "index.json"
    new_path = game / dest / "index.json"
    if not ours_path.is_file() or not new_path.is_file():
        return {"dest": dest, "status": "skip（一侧缺 index.json）"}
    ours = {
        entry["index"]: entry
        for entry in json.loads(ours_path.read_text(encoding="utf-8")).get("entries", [])
        if not entry.get("empty")
    }
    new = {
        entry["index"]: entry
        for entry in images_of(json.loads(new_path.read_text(encoding="utf-8")))
        if not entry.get("empty")
    }
    common = sorted(set(ours) & set(new))
    return {
        "dest": dest,
        "common": len(common),
        "rgb_same": sum(1 for index in common if ours[index].get("rgb_sha256") == new[index].get("rgb_sha256")),
        "rgba_same": sum(1 for index in common if ours[index].get("rgba_sha256") == new[index].get("rgba_sha256")),
        "missing_rgb": [
            index for index in common if ours[index].get("rgb_sha256") != new[index].get("rgb_sha256")
        ][:5],
    }


def verify(plan_path: Path, game: Path, sample_dir: Path | None, samples: list[str]) -> dict:
    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    issues: list[str] = []
    category: Counter[str] = Counter()
    category_bytes: Counter[str] = Counter()
    outputs_checked = 0
    png_total = 0
    missing_total = 0
    empty_total = 0

    for entry in plan["libraries"]:
        for output in entry["outputs"]:
            outputs_checked += 1
            dest = output["dest"]
            directory = game / dest
            index_path = directory / "index.json"
            if not index_path.is_file():
                issues.append(f"缺 index.json：{dest}")
                continue
            data = json.loads(index_path.read_text(encoding="utf-8"))
            images = images_of(data)
            written = [image for image in images if not image.get("empty", False)]
            pngs = list(directory.glob("*.png"))
            if len(pngs) != len(written):
                issues.append(f"{dest}: PNG {len(pngs)} ≠ index 非空 {len(written)}")
            expected_loaded = int(output.get("loaded_frames", len(written)))
            if len(written) != expected_loaded:
                issues.append(f"{dest}: index 非空 {len(written)} ≠ 计划 loaded {expected_loaded}")
            requested = int(output.get("requested", len(output.get("indices", []))))
            if len(written) + len(images) - len(written) != requested:
                issues.append(f"{dest}: index 条目 {len(images)} ≠ 计划 requested {requested}")
            empty_total += len(images) - len(written)
            missing_total += int(data.get("skipped_empty", len(images) - len(written)))
            png_total += len(pngs)
            if dest.startswith("gfx/"):
                top = dest.split("/")[1]
                category[top] += len(pngs) + 1
                category_bytes[top] += sum(path.stat().st_size for path in pngs) + index_path.stat().st_size

    categories = {
        "gfx/tiles": {"files": category["tiles"], "bytes": category_bytes["tiles"]},
        "gfx/sprites": {"files": category["sprites"], "bytes": category_bytes["sprites"]},
        "gfx/ui": {"files": category["ui"], "bytes": category_bytes["ui"]},
    }
    for folder, label in (("maps", "maps"), ("audio", "audio"), ("fonts", "fonts")):
        files, size = directory_usage(game / folder)
        categories[label] = {"files": files, "bytes": size}
    server_root = game / "data" / "server"
    files, size = directory_usage(server_root)
    categories["data/server"] = {"files": files, "bytes": size}
    files, size = directory_usage(game / "data", skip=server_root)
    categories["data/other"] = {"files": files, "bytes": size}

    old_layout = {}
    old_suffixes = {".wzl", ".wzx", ".wil", ".wix"}
    for folder in ("data", "map", "wav", "ld", "server"):
        root = game / folder
        if not root.is_dir():
            continue
        if folder == "data":
            files = [
                path for path in root.rglob("*")
                if path.is_file() and server_root not in path.parents
                and path.suffix.lower() in old_suffixes
            ]
            size = sum(path.stat().st_size for path in files)
        else:
            files, size = directory_usage(root)
        if files:
            old_layout[folder] = {"files": len(files) if isinstance(files, list) else files, "bytes": size}

    sample_report = [sample_compare(game, sample_dir, dest) for dest in samples]
    return {
        "outputs_checked": outputs_checked,
        "png_total": png_total,
        "missing_frame_total": missing_total,
        "index_empty_total": empty_total,
        "categories": categories,
        "samples": sample_report,
        "old_layout": old_layout,
        "issues": issues[:100],
        "issue_count": len(issues),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="校验 game-assets 新布局与拆包计划")
    parser.add_argument("--plan", default="target/used-assets.json")
    parser.add_argument("--game-assets", default="game-assets")
    parser.add_argument("--sample-dir", default="target/game-assets-mini", help="独立解码样例目录（可空）")
    parser.add_argument("--samples", default=DEFAULT_SAMPLES, help="逗号分隔的抽样 dest")
    parser.add_argument("--out", default="target/layout-verify.json")
    parser.add_argument("--strict", action="store_true", help="有 issue 时退出码 1")
    args = parser.parse_args(argv)

    repo = Path(__file__).resolve().parents[2]
    plan_path = (repo / args.plan).resolve()
    game = (repo / args.game_assets).resolve()
    sample_dir = (repo / args.sample_dir).resolve() if args.sample_dir else None
    samples = [item.strip() for item in args.samples.split(",") if item.strip()]
    result = verify(plan_path, game, sample_dir, samples)
    out = (repo / args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, ensure_ascii=False, indent=2))
    print(f"\n写出 {out}")
    return 1 if (args.strict and result["issue_count"]) else 0


if __name__ == "__main__":
    raise SystemExit(main())
