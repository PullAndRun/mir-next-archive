#!/usr/bin/env python3
"""从全量 `used-assets.json` 生成**最小冒烟计划**：

- 一张地图实际用到的瓦片（默认地图 `0`）；
- `items.wzl` 的全部图标；
- 一个怪物实体（默认 `mon-0130` / `image=130`）。

用途：给拆包器/客户端做「真数据、小体积」联调（全量计划 69 库 / 18 万帧，冒烟计划 ~1 万帧）。

```powershell
& "C:\\Users\\diana\\.dsh\\dsh-runtimes\\dsh-primary-runtime\\dependencies\\python\\python.exe" `
    tools/assets/make_mini_plan.py --map 0 --monster-image 130
# → target/used-assets-mini.json
```
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))
import analyze_usage as au  # noqa: E402


def measure(entry: dict, indices: list[int]) -> tuple[int, int]:
    source = REPO_ROOT / entry["source"]
    index = au.WzlIndex(source, source.with_suffix(".wzx"))
    loaded = missing = 0
    for image_index in indices:
        if index.record(image_index) is None:
            missing += 1
        else:
            loaded += 1
    index.close()
    return loaded, missing


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="生成 map0+items+一只怪物的最小拆包计划")
    parser.add_argument("--plan", default="target/used-assets.json", help="全量计划")
    parser.add_argument("--out", default="target/used-assets-mini.json", help="输出冒烟计划")
    parser.add_argument("--map", default="0", help="地图文件名（不含 .map）")
    parser.add_argument("--monster-image", type=int, default=130, help="怪物 Image 号（默认 130）")
    parser.add_argument("--items-lib", default="items.wzl", help="物品图标库文件名")
    args = parser.parse_args(argv)

    full = json.loads((REPO_ROOT / args.plan).read_text(encoding="utf-8"))
    libraries = {entry["lib"].lower(): entry for entry in full["libraries"]}
    map_book = au.RefBook()
    map_path = au.MAP_DIR_CJLAAA / f"{args.map}.map"
    if not map_path.is_file():
        raise SystemExit(f"[错误] 地图不存在：{map_path}")
    au.scan_maps(au.MAP_DIR_CJLAAA, [args.map], map_book, f"map{args.map}")

    outputs: list[dict] = []
    tile_stats = {"libraries": 0, "frames": 0, "loaded": 0, "empty": 0}
    for lib, record in sorted(map_book.libs.items()):
        entry = libraries.get(lib)
        if entry is None or entry["category"] != "tiles":
            continue
        indices = sorted(set(entry["indices"]) & set(record["indices"]))
        if not indices:
            continue
        loaded, missing = measure(entry, indices)
        outputs.append(
            {
                "lib": entry["lib"],
                "category": entry["category"],
                "source": entry["source"],
                "entries": entry["entries"],
                "nonempty": entry["nonempty"],
                "dest": entry["dest"],
                "indices": indices,
                "reason": f"地图 {args.map} 的瓦片",
                "used_frames": len(indices),
                "loaded_frames": loaded,
                "missing_count": missing,
            }
        )
        tile_stats["libraries"] += 1
        tile_stats["frames"] += len(indices)
        tile_stats["loaded"] += loaded
        tile_stats["empty"] += missing

    items = libraries.get(args.items_lib.lower())
    if items is None:
        raise SystemExit(f"[错误] 全量计划里没有 {args.items_lib}")
    outputs.append(
        {key: items[key] for key in ("lib", "category", "source", "entries", "nonempty", "dest",
                                     "outputs", "indices", "reason", "used_frames",
                                     "loaded_frames", "missing_count")}
    )

    monster = None
    monster_entity = f"mon-{args.monster_image:04d}"
    for entry in full["libraries"]:
        if entry["category"] != "sprites" or not entry["lib"].lower().startswith("mon"):
            continue
        for output in entry.get("outputs", []):
            if output.get("entity") == monster_entity:
                monster = {
                    "lib": entry["lib"],
                    "category": entry["category"],
                    "source": entry["source"],
                    "entries": entry["entries"],
                    "nonempty": entry["nonempty"],
                    "dest": output["dest"],
                    "outputs": [output],
                    "indices": output["indices"],
                    "reason": f"怪物 image={args.monster_image}（冒烟样例）",
                    "used_frames": len(output["indices"]),
                    "loaded_frames": output.get("loaded_frames", len(output["indices"])),
                    "missing_count": output.get("missing_count", 0),
                }
                break
        if monster:
            break
    if monster is None:
        raise SystemExit(f"[错误] 全量计划里没有 {monster_entity}（可能该图像在库内无帧）")
    outputs.append(monster)

    mini = {
        "version": "1.1",
        "generated_at": full["generated_at"],
        "source_root": full["source_root"],
        "dataset": full["dataset"],
        "scope": f"mini：map{args.map} tiles + {args.items_lib} + {monster_entity}",
        "libraries": outputs,
        "maps": [args.map],
        "audio": full.get("audio", [])[:4],
        "stats": {
            "libraries": len(outputs),
            "tiles": tile_stats,
            "item_icons": items["used_frames"],
            "item_loaded": items["loaded_frames"],
            "monster_frames": len(monster["indices"]),
            "monster_loaded": monster["loaded_frames"],
        },
        "notes": [
            f"只覆盖：地图 {args.map} 用到的瓦片 + {args.items_lib} 全部图标 + {monster_entity}。",
            "全量见 target/used-assets.json；indices 为库内 0 基下标，outputs 决定落盘目录。",
        ],
    }
    out_path = REPO_ROOT / args.out
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(mini, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(
        f"写出 {args.out}：{len(outputs)} 个库 / "
        f"瓦片 {tile_stats['frames']} 帧（可解 {tile_stats['loaded']}，空 {tile_stats['empty']}）/ "
        f"物品 {items['used_frames']} / 怪物 {len(monster['indices'])} 帧"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
