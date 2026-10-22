#!/usr/bin/env python3
"""把 v1 素材树重组为 v2「自有语义 ID + 自有元数据」布局（方案见 docs/assets/asset-layout-v2.md）。

只做重组，不解码/不重新压缩：
- PNG 用硬链接（同盘）或复制，保证帧字节与 v1 完全一致；
- 每个资产目录写 `frames.json`（我们的 schema，不含源库字段）；
- 源库信息进 `index/provenance.json`；资产目录进 `index/catalog.json`；`library-map.json` 升到 v2。

```powershell
py tools/assets/make_asset_v2.py --source game-assets --out game-assets-v2
py tools/assets/make_asset_v2.py --hash-check sample --samples 20
```
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import shutil
import sys
from collections import Counter, defaultdict
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
FORBIDDEN_FIELDS = (
    "lib",
    "source",
    "source_bytes",
    "source_sha256",
    "data_offset",
    "data_len",
    "format_code",
    "reserved",
    "shadow_x",
    "shadow_y",
    "has_mask",
)

# 语义名映射：v1 目录名 → v2 相对路径（不含 gfx/sprites 前缀的语义分组）
EFFECT_NAMES = {"effect": "swing", "magic": "hit", "magic2": "bolt"}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def asset_id_for(v1_rel_dir: Path, index: dict) -> tuple[str, str, str]:
    """返回 (v2 id, kind, v2 相对目录)。`v1_rel_dir` 是相对 `game-assets/` 的路径。"""
    rel = v1_rel_dir.as_posix()
    parts = rel.split("/")
    if parts[:2] == ["gfx", "tiles"]:
        return f"tiles/{parts[2]}", "tile", f"gfx/tiles/{parts[2]}"
    if parts[:3] == ["gfx", "ui", "minimap"]:
        return "ui/minimaps", "minimap", "gfx/ui/minimaps"
    if parts[:3] == ["gfx", "sprites", "monster"]:
        number = parts[3].removeprefix("mon-")
        return f"monsters/{number}", "monster", f"gfx/sprites/monsters/{number}"
    if parts[:3] == ["gfx", "sprites", "npc"]:
        number = parts[3].removeprefix("npc-")
        return f"npcs/{number}", "npc", f"gfx/sprites/npcs/{number}"
    if parts[:3] == ["gfx", "sprites", "item"]:
        number = parts[3].removeprefix("icon-")
        return f"items/{number}", "item", f"gfx/sprites/items/{number}"
    if parts[:3] == ["gfx", "sprites", "player"]:
        leaf = parts[3]
        if leaf.startswith("body-"):
            number = leaf.removeprefix("body-")
            return f"players/body/{number}", "player-layer", f"gfx/sprites/players/body/{number}"
        if leaf.startswith("hair-"):
            number = leaf.removeprefix("hair-")
            return f"players/hair/{number}", "player-layer", f"gfx/sprites/players/hair/{number}"
        if leaf.startswith("weapon-"):
            number = leaf.removeprefix("weapon-")
            return f"players/weapon/{number}", "player-layer", f"gfx/sprites/players/weapon/{number}"
        if leaf.startswith("wings-"):
            number = leaf.removeprefix("wings-")
            return f"players/wings/{number}", "player-layer", f"gfx/sprites/players/wings/{number}"
        if leaf.startswith("mount-"):
            number = leaf.removeprefix("mount-")
            return f"players/mount/{number}", "player-layer", f"gfx/sprites/players/mount/{number}"
    if parts[:3] == ["gfx", "sprites", "effect"]:
        leaf = parts[3]
        semantic = EFFECT_NAMES.get(leaf, leaf)
        return f"effects/{semantic}", "effect", f"gfx/sprites/effects/{semantic}"
    slug = "-".join(parts[1:]).replace(" ", "_")
    return f"misc/{slug}", "misc", f"gfx/misc/{slug}"


def link_or_copy(source: Path, target: Path) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists():
        target.unlink()
    try:
        os.link(source, target)
    except OSError:
        shutil.copy2(source, target)


def build(source_root: Path, out_root: Path, hash_check: str, samples: int) -> dict:
    if out_root.exists():
        shutil.rmtree(out_root)
    (out_root / "index").mkdir(parents=True, exist_ok=True)
    catalog: dict[str, dict] = {}
    provenance: dict[str, dict] = {}
    libraries: dict[str, dict] = {}
    frame_total = 0
    count_issues: list[dict] = []
    bytes_issues: list[dict] = []
    forbidden_hits: list[dict] = []

    asset_dirs = sorted(
        path.parent
        for path in (source_root / "gfx").rglob("index.json")
    )
    minimap_entries: list[tuple[str, dict, Path]] = []

    for v1_dir in asset_dirs:
        index = json.loads((v1_dir / "index.json").read_text(encoding="utf-8"))
        entries = [entry for entry in index.get("images", []) if not entry.get("empty") and entry.get("png")]
        if not entries:
            continue
        asset_id, kind, v2_rel = asset_id_for(v1_dir.relative_to(source_root), index)
        source_lib = index.get("lib")
        source_file = index.get("source")
        source_sha = index.get("source_sha256")
        if asset_id == "ui/minimaps":
            # 小地图：一帧一资产
            for entry in entries:
                frame_id = f"ui/minimaps/{entry['index']:04d}"
                rel = f"gfx/ui/minimaps/{entry['index']:04d}"
                v2_dir = out_root / rel
                file_name = "0000.png"
                link_or_copy(v1_dir / entry["png"], v2_dir / file_name)
                frames_json = {
                    "id": frame_id,
                    "kind": "minimap",
                    "format": entry.get("format", "palette8"),
                    "frames": [
                        {
                            "id": 0,
                            "file": file_name,
                            "w": entry.get("width", 0),
                            "h": entry.get("height", 0),
                            "ox": entry.get("offset_x", 0),
                            "oy": entry.get("offset_y", 0),
                        }
                    ],
                }
                frames_path = v2_dir / "frames.json"
                frames_path.write_text(
                    json.dumps(frames_json, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
                )
                frame_total += 1
                minimap_entries.append((frame_id, entry, v1_dir))
                catalog[frame_id] = {
                    "kind": "minimap",
                    "format": frames_json["format"],
                    "frames": 1,
                    "bytes": (v2_dir / file_name).stat().st_size,
                    "sha256": sha256_file(frames_path),
                    "pack_prefix": f"{rel}/",
                }
                libraries.setdefault("ui/minimaps", {"kind": "minimap", "assets": []})["assets"].append(frame_id)
                provenance[frame_id] = {
                    "source_dataset": "cjlaaa",
                    "source_file": source_file,
                    "source_sha256": source_sha,
                    "frame_map": [[0, entry["index"]]],
                }
            continue

        v2_dir = out_root / v2_rel
        frames = []
        frame_map = []
        formats = Counter()
        for ordinal, entry in enumerate(entries):
            file_name = f"{ordinal:04d}.png"
            link_or_copy(v1_dir / entry["png"], v2_dir / file_name)
            frames.append(
                {
                    "id": ordinal,
                    "file": file_name,
                    "w": entry.get("width", 0),
                    "h": entry.get("height", 0),
                    "ox": entry.get("offset_x", 0),
                    "oy": entry.get("offset_y", 0),
                }
            )
            frame_map.append([ordinal, entry["index"]])
            formats[entry.get("format")] += 1
        frames_json = {
            "id": asset_id,
            "kind": kind,
            "format": formats.most_common(1)[0][0] if len(formats) == 1 else "mixed",
            "frames": frames,
        }
        frames_path = v2_dir / "frames.json"
        frames_path.write_text(json.dumps(frames_json, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        for field in FORBIDDEN_FIELDS:
            if field in json.dumps(frames_json):
                forbidden_hits.append({"asset": asset_id, "field": field})
        frame_total += len(frames)
        catalog[asset_id] = {
            "kind": kind,
            "format": frames_json["format"],
            "frames": len(frames),
            "bytes": sum((v2_dir / frame["file"]).stat().st_size for frame in frames),
            "sha256": sha256_file(frames_path),
            "pack_prefix": f"{v2_rel}/",
        }
        provenance[asset_id] = {
            "source_dataset": "cjlaaa",
            "source_file": source_file,
            "source_sha256": source_sha,
            "frame_map": frame_map,
        }
        group = "/".join(asset_id.split("/")[:2]) if asset_id.startswith("players/") else asset_id.split("/")[0]
        libraries.setdefault(group, {"kind": kind, "assets": []})["assets"].append(asset_id)

        # 等价性：帧数 + 帧字节 sha256
        if len(entries) != len(frames):
            count_issues.append({"asset": asset_id, "v1": len(entries), "v2": len(frames)})
        for ordinal, entry in enumerate(entries):
            if sha256_file(v1_dir / entry["png"]) != sha256_file(v2_dir / f"{ordinal:04d}.png"):
                bytes_issues.append({"asset": asset_id, "ordinal": ordinal})

    # 非 gfx 资产：原样硬链接（maps/audio/data/fonts/index）
    passthrough = []
    for category in ("maps", "audio", "data", "fonts", "index"):
        root = source_root / category
        if not root.is_dir():
            continue
        for path in sorted(root.rglob("*")):
            if not path.is_file():
                continue
            rel = path.relative_to(source_root)
            link_or_copy(path, out_root / rel)
            passthrough.append(str(rel).replace("\\", "/"))

    # catalog / provenance / library-map v2
    catalog_doc = {
        "version": 2,
        "generated_at": None,
        "assets": dict(sorted(catalog.items())),
        "passthrough": passthrough,
    }
    (out_root / "index" / "catalog.json").write_text(
        json.dumps(catalog_doc, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    provenance_doc = {
        "version": 2,
        "note": "源库信息只在这里出现；发布包不带本文件",
        "sources": dict(sorted(provenance.items())),
    }
    (out_root / "index" / "provenance.json").write_text(
        json.dumps(provenance_doc, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    library_doc = {
        "version": 2,
        "libraries": {
            key: {"kind": value["kind"], "assets": sorted(value["assets"])}
            for key, value in sorted(libraries.items())
        },
    }
    (out_root / "library-map.json").write_text(
        json.dumps(library_doc, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )

    # 解码级抽查
    sample_report = []
    if hash_check != "none":
        random.seed(20261022)
        assets = sorted(catalog)
        picks = assets if hash_check == "all" else random.sample(assets, min(samples, len(assets)))
        try:
            from PIL import Image
        except ImportError:
            Image = None
        lookup = {}
        for v1_dir in asset_dirs:
            index = json.loads((v1_dir / "index.json").read_text(encoding="utf-8"))
            entries = [entry for entry in index.get("images", []) if not entry.get("empty") and entry.get("png")]
            if not entries:
                continue
            asset_id, _, _ = asset_id_for(v1_dir.relative_to(source_root), index)
            if asset_id != "ui/minimaps":
                lookup[asset_id] = (v1_dir, entries)
        for asset_id in picks:
            if asset_id.startswith("ui/minimaps") or asset_id not in lookup:
                sample_report.append({"asset": asset_id, "checked": False, "reason": "非 gfx 目录或小地图"})
                continue
            v1_dir, entries = lookup[asset_id]
            rel = catalog[asset_id]["pack_prefix"]
            v2_dir = out_root / rel
            if Image is None:
                sample_report.append({"asset": asset_id, "checked": False, "reason": "没有 Pillow"})
                continue
            same = True
            for ordinal, entry in enumerate(entries):
                a = Image.open(v1_dir / entry["png"]).convert("RGBA")
                b = Image.open(v2_dir / f"{ordinal:04d}.png").convert("RGBA")
                if a.tobytes() != b.tobytes():
                    same = False
                    break
            sample_report.append({"asset": asset_id, "checked": True, "pixels_equal": same})

    report = {
        "assets": len(catalog),
        "frames": frame_total,
        "libraries": len(libraries),
        "passthrough_files": len(passthrough),
        "count_issues": count_issues[:20],
        "count_issue_total": len(count_issues),
        "bytes_issues": bytes_issues[:20],
        "bytes_issue_total": len(bytes_issues),
        "forbidden_fields": forbidden_hits,
        "sample": sample_report,
    }
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="重组 v2 自有语义素材树")
    parser.add_argument("--source", default=str(REPO_ROOT / "game-assets"))
    parser.add_argument("--out", default=str(REPO_ROOT / "game-assets-v2"))
    parser.add_argument("--hash-check", choices=["none", "sample", "all"], default="sample")
    parser.add_argument("--samples", type=int, default=20)
    parser.add_argument("--report", default=str(REPO_ROOT / "target" / "asset-v2-equivalence.json"))
    args = parser.parse_args(argv)
    report = build(Path(args.source).resolve(), Path(args.out).resolve(), args.hash_check, args.samples)
    Path(args.report).parent.mkdir(parents=True, exist_ok=True)
    Path(args.report).write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False, indent=2))
    ok = (
        not report["count_issues"]
        and not report["bytes_issues"]
        and not report["forbidden_fields"]
        and all(item.get("pixels_equal", True) for item in report["sample"])
    )
    print(f"\n[{'通过' if ok else '失败'}] 资产 {report['assets']} / 帧 {report['frames']}，报告 {args.report}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
