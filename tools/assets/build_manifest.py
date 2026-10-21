#!/usr/bin/env python3
"""构建 `game-assets/manifest.assets.json`：把新布局资源汇总成客户端/文档可查的清单。

输入：
- `target/used-assets.json`（引用分析/拆包计划）；
- 可选 `target/layout-verify.json`（`verify_layout.py` 的结果，用于写入 verify 摘要）。

输出：
- `<game-assets>/manifest.assets.json`：分类别/总量、每个图库的 output（requested/loaded/missing/
  aliases/png/bytes）、地图/音频/字体列表、旧布局留存、计划 sha256；
- `<game-assets>/index/summary.json`：精简汇总 + “列出资源库”命令。

用法：

```powershell
py tools/assets/build_manifest.py --plan target/used-assets.json --game-assets game-assets
py tools/assets/build_manifest.py --verify-json target/layout-verify.json
```
"""

from __future__ import annotations

import argparse
import hashlib
import json
import time
from collections import Counter
from pathlib import Path


def directory_usage(directory: Path, skip: Path | None = None) -> tuple[int, int]:
    """返回 (文件数, 字节数)；`skip` 目录下的文件不计入（例如 data/ 里的 server/）。"""
    if not directory.is_dir():
        return 0, 0
    files = [
        path
        for path in directory.rglob("*")
        if path.is_file() and not (skip is not None and skip in path.parents)
    ]
    return len(files), sum(path.stat().st_size for path in files)


def top_level_legacy(game: Path, server_root: Path) -> dict:
    """旧布局（map/、wav/、ld/、server/、data/ 非 server 部分）的留存统计。"""
    legacy = {}
    old_suffixes = {".wzl", ".wzx", ".wil", ".wix"}
    for folder in ("data", "map", "wav", "ld", "server"):
        root = game / folder
        if not root.is_dir():
            continue
        files = []
        for path in root.rglob("*"):
            if not path.is_file():
                continue
            if folder == "data":
                # data/ 现在是新格式（.mnd/.json 等）的落点；只有老图库后缀算旧布局
                if server_root in path.parents or path.suffix.lower() not in old_suffixes:
                    continue
            files.append(path)
        if files:
            legacy[folder] = {"files": len(files), "bytes": sum(p.stat().st_size for p in files)}
    manifest_old = game / "manifest.json"
    if manifest_old.is_file():
        legacy["manifest.json"] = {"files": 1, "bytes": manifest_old.stat().st_size}
    return legacy


def build(plan_path: Path, game: Path, verify_path: Path | None) -> dict:
    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    plan_sha = hashlib.sha256(plan_path.read_bytes()).hexdigest()
    categories: Counter[str] = Counter()
    category_bytes: Counter[str] = Counter()
    libraries = []

    for entry in plan["libraries"]:
        files = 0
        size = 0
        outputs = []
        for output in entry["outputs"]:
            dest = output["dest"]
            directory = game / dest
            pngs = list(directory.glob("*.png")) if directory.is_dir() else []
            index_path = directory / "index.json"
            index_size = index_path.stat().st_size if index_path.is_file() else 0
            files += len(pngs) + (1 if index_path.is_file() else 0)
            size += sum(path.stat().st_size for path in pngs) + index_size
            if dest.startswith("gfx/"):
                top = dest.split("/")[1]
                categories[top] += len(pngs) + (1 if index_path.is_file() else 0)
                category_bytes[top] += sum(path.stat().st_size for path in pngs) + index_size
            outputs.append(
                {
                    "dest": dest,
                    "entity": output.get("entity"),
                    "requested": output.get("requested"),
                    "loaded": output.get("loaded_frames"),
                    "missing": output.get("missing_count"),
                    "aliases": output.get("aliases", []),
                    "png": len(pngs),
                }
            )
        libraries.append(
            {
                "lib": entry["lib"],
                "category": entry["category"],
                "source": entry["source"],
                "outputs": outputs,
                "files": files,
                "bytes": size,
            }
        )

    # 地图优先取我们自己的 .mnm；.map 只在还没转换时作为回退
    map_paths = sorted((game / "maps").glob("*.mnm"))
    if not map_paths:
        map_paths = sorted((game / "maps").glob("*.map"))
    maps = [path.stem for path in map_paths]
    map_bytes = sum(path.stat().st_size for path in map_paths)
    maps_detail = [
        {
            "id": path.stem,
            "file": f"maps/{path.name}",
            "format": path.suffix.lstrip("."),
            "bytes": path.stat().st_size,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        }
        for path in map_paths
    ]
    audio_files = sorted(path.name for path in (game / "audio").rglob("*") if path.is_file())
    audio_bytes = sum(path.stat().st_size for path in (game / "audio").rglob("*") if path.is_file())
    server_root = game / "data" / "server"
    server_files, server_bytes = directory_usage(server_root)
    data_files, data_bytes = directory_usage(game / "data", skip=server_root)
    font_files, font_bytes = directory_usage(game / "fonts")

    categories_manifest = {
        "gfx/tiles": {"files": categories["tiles"], "bytes": category_bytes["tiles"]},
        "gfx/sprites": {"files": categories["sprites"], "bytes": category_bytes["sprites"]},
        "gfx/ui": {"files": categories["ui"], "bytes": category_bytes["ui"]},
        "maps": {"files": len(maps), "bytes": map_bytes, "mapinfo_entries": len(plan["maps"]), "format": maps_detail[0]["format"] if maps_detail else None},
        "audio": {"files": len(audio_files), "bytes": audio_bytes},
        "data/server": {"files": server_files, "bytes": server_bytes},
        "data/other": {"files": data_files, "bytes": data_bytes},
        "fonts": {"files": font_files, "bytes": font_bytes},
    }
    totals = {
        "files": sum(item["files"] for item in categories_manifest.values()),
        "bytes": sum(item["bytes"] for item in categories_manifest.values()),
    }
    verify = None
    if verify_path is not None and verify_path.is_file():
        verify_payload = json.loads(verify_path.read_text(encoding="utf-8"))
        verify = {
            "outputs_checked": verify_payload.get("outputs_checked"),
            "png_total": verify_payload.get("png_total"),
            "missing_frames": verify_payload.get("missing_frame_total"),
            "issues": verify_payload.get("issue_count"),
        }

    manifest = {
        "version": 1,
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "source_plan": str(plan_path.relative_to(plan_path.parents[1])) if plan_path.is_absolute() else str(plan_path),
        "plan_sha256": plan_sha,
        "dataset": plan.get("dataset"),
        "layout": "game-assets/gfx|maps|audio|data/server|fonts（新布局）",
        "categories": categories_manifest,
        "totals": totals,
        "libraries": libraries,
        "maps": maps,
        "audio": audio_files,
        "fonts": [path.name for path in (game / "fonts").iterdir() if path.is_file()],
        "verify": verify,
        "maps_detail": maps_detail,
        "pack_hints": {
            "chunk_mb": 16,
            "category_by_prefix": {
                "gfx/tiles": "tiles",
                "gfx/sprites": "sprites",
                "gfx/ui": "ui",
                "maps": "maps",
                "audio": "audio",
                "data": "data",
            },
            "logical_name": {
                "gfx": "<dest>/<index>.png（dest = outputs[].dest）",
                "maps": "maps/<id>.mnm（.map 转换后）",
                "audio": "audio/sfx/<name>.wav 或 audio/music/<name>.wav",
                "data": "data/<name>（.mnd/.json 等新格式）",
            },
            "note": "按 category 切 <category>-<bucket>.mnp（单包 ≤16MB）；包内条目名 = 逻辑名；"
                    "地图等 .mnm 转换后单独 maps-*.mnp，逐张一条（maps_detail 已给 id/bytes/sha256）",
        },
        "old_layout_retained": top_level_legacy(game, server_root),
        "notes": [
            f"maps {len(maps)} 个（{maps_detail[0]['format'] if maps_detail else '?'}）= Server.MirDB 的 {len(plan['maps'])} 条 MapInfo 指向的唯一文件数（重复记录会合并）",
            "gfx 每个 output 目录的 index.json 含 rgb_sha256/rgba_sha256/pixels_sha256 与 empty/missing 统计",
            "旧布局（map/wav/ld/server/data 非 server）只做统计，不参与运行时",
        ],
    }
    return manifest


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="构建 game-assets manifest.assets.json")
    parser.add_argument("--plan", default="target/used-assets.json")
    parser.add_argument("--game-assets", default="game-assets")
    parser.add_argument("--verify-json", default="target/layout-verify.json")
    parser.add_argument("--out", default=None, help="默认 <game-assets>/manifest.assets.json")
    parser.add_argument("--summary-out", default=None, help="默认 <game-assets>/index/summary.json")
    args = parser.parse_args(argv)

    repo = Path(__file__).resolve().parents[2]
    plan_path = (repo / args.plan).resolve()
    game = (repo / args.game_assets).resolve()
    verify_path = (repo / args.verify_json).resolve() if args.verify_json else None
    out = (repo / args.out) if args.out else (game / "manifest.assets.json")
    summary_out = (repo / args.summary_out) if args.summary_out else (game / "index" / "summary.json")

    manifest = build(plan_path, game, verify_path)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    summary = {
        "generated_at": manifest["generated_at"],
        "plan_sha256": manifest["plan_sha256"],
        "categories": manifest["categories"],
        "totals": manifest["totals"],
        "libraries": len(manifest["libraries"]),
        "verify": manifest["verify"],
        "list_command": (
            'pwsh -NoProfile -Command "(Get-Content game-assets/manifest.assets.json -Raw | '
            'ConvertFrom-Json).categories"'
        ),
    }
    summary_out.parent.mkdir(parents=True, exist_ok=True)
    summary_out.write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    print(f"\n写出 {out} / {summary_out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
