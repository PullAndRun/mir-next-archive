#!/usr/bin/env python3
"""从 `target/used-assets.json` 导出一份**最小可验证样例**到 `target/game-assets-mini/`。

用途（第 26 轮）：

- 在 Rust 拆包器（`mir-tools assets unpack`）全量落地前，先用独立实现验证
  「used 索引 → PNG + index.json」的目录结构与解码结果；
- 给客户端 `UnpackedLibrary` 一个可直接加载的小样例；
- 与户部的 Rust 拆包结果**互相校验**（本脚本不依赖 `mir-asset`，是第二实现）。

目录结构与户部约定一致：

```
target/game-assets-mini/
  gfx/<category>/<stem>/
    index.json            {"lib","source","count","reason","entries":[...]}
    0000.png …            RGBA8（黑底转透明，与 mir-render 的 remove_black 一致）
  maps/0.map              样例地图（可选，--copy-maps）
  audio/1.wav …           样例音频（可选，--copy-audio）
  manifest.json           本次样例的清单（文件数/字节数/来源）
```

用法：

```powershell
& "C:\\Users\\diana\\.dsh\\dsh-runtimes\\dsh-primary-runtime\\dependencies\\python\\python.exe" `
    tools/assets/export_asset_sample.py --plan target/used-assets.json --out target/game-assets-mini
```
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import struct
import sys
import time
import zlib
from io import BytesIO
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))
import analyze_usage as au  # noqa: E402

AUDIO_ROOTS = au.AUDIO_ROOTS

# 每类图库在样例里最多导出多少帧（默认值；`--all` 可取消限制）。
LIMITS = {
    "tiles": 64,
    "sprites": 96,
    "items": 128,
    "ui": 8,
    "effects": 256,
    "dress": 32,
    "weapons": 32,
    "hair": 32,
}


def load_palette() -> list[tuple[int, int, int, int]]:
    """从 `crates/mir-asset/src/palette.rs` 解析 WeMade 256 色调色板。"""
    text = (REPO_ROOT / "crates" / "mir-asset" / "src" / "palette.rs").read_text(encoding="utf-8")
    values = re.findall(r"0x([0-9A-Fa-f]{2})([0-9A-Fa-f]{2})_([0-9A-Fa-f]{4})", text)
    if len(values) != 256:
        raise SystemExit(f"[错误] palette.rs 解析出 {len(values)} 项，应为 256")
    palette = []
    for alpha, high, low in values:
        value = int(high + low, 16)
        palette.append((value >> 16 & 0xFF, value >> 8 & 0xFF, value & 0xFF, int(alpha, 16)))
    return palette


def decode_rgba(handle, info: dict, palette) -> bytes:
    """按 `.wzl` 记录头解码成 RGBA8（行优先、自上而下）。"""
    width, height = info["width"], info["height"]
    handle.seek(info["data_offset"])
    data = zlib.decompress(handle.read(info["length"]))
    pixel_format = info["format"]
    bits = 16 if pixel_format == 5 else 24 if pixel_format == 6 else 8
    row_bytes = (((width * bits) + 31) >> 5) * 4
    out = bytearray(width * height * 4)
    for y in range(height):
        source = (height - 1 - y) * row_bytes  # Crystal 自下而上写行
        if pixel_format == 5:  # RGB565
            for x in range(width):
                value = data[source + x * 2] | (data[source + x * 2 + 1] << 8)
                r = (value >> 11) & 0x1F
                g = (value >> 5) & 0x3F
                b = value & 0x1F
                offset = (y * width + x) * 4
                out[offset] = r << 3
                out[offset + 1] = g << 2
                out[offset + 2] = b << 3
                out[offset + 3] = 255
        elif pixel_format == 6:  # RGB24
            for x in range(width):
                offset = (y * width + x) * 4
                out[offset] = data[source + x * 3]
                out[offset + 1] = data[source + x * 3 + 1]
                out[offset + 2] = data[source + x * 3 + 2]
                out[offset + 3] = 255
        else:  # 8 位调色板（格式 0 与 3 都按调色板）
            for x in range(width):
                r, g, b, a = palette[data[source + x]]
                offset = (y * width + x) * 4
                if r == 0 and g == 0 and b == 0:
                    a = 0  # 与 mir-render::scene::remove_black 一致
                out[offset] = r
                out[offset + 1] = g
                out[offset + 2] = b
                out[offset + 3] = a
    return bytes(out)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sample_indices(entry: dict, indices: list[int], map0: au.RefBook, limit: int) -> list[int]:
    """挑出样例帧：瓦片优先取 map0 实际用到的，其余按 used 顺序截断。"""
    if entry["category"] == "tiles":
        map_record = map0.libs.get(entry["lib"].lower()) or map0.libs.get(entry["lib"])
        if map_record:
            preferred = sorted(set(indices) & set(map_record["indices"]))
            if preferred:
                return preferred[:limit]
    return indices[:limit]


def export_output(
    entry: dict,
    output: dict,
    out_root: Path,
    palette,
    map0: au.RefBook,
    limit: int,
    force: bool,
    handles: dict,
) -> dict:
    """导出一个 output（一个目标目录）。同一 lib 的句柄在 `handles` 里复用。"""
    source = REPO_ROOT / entry["source"]
    destination = out_root / output["dest"]
    destination.mkdir(parents=True, exist_ok=True)
    wzx = source.with_suffix(".wzx")
    if entry["lib"].lower() not in handles:
        handles[entry["lib"].lower()] = (au.WzlIndex(source, wzx), source.open("rb"))
    index, handle = handles[entry["lib"].lower()]
    wanted = sample_indices(entry, output["indices"], map0, limit)
    try:
        from PIL import Image
    except ImportError as error:  # pragma: no cover - 只影响本机环境
        raise SystemExit(
            "[错误] 需要 Pillow：请用带依赖的 Python 运行"
            "（C:\\Users\\diana\\.dsh\\dsh-runtimes\\dsh-primary-runtime\\dependencies\\python\\python.exe，"
            "见 docs/assets/asset-pipeline.md §5）"
        ) from error
    records = []
    exported = 0
    skipped = 0
    for image_index in wanted:
        info = index.record(image_index)
        if info is None:
            skipped += 1
            records.append({"index": image_index, "empty": True})
            continue
        rgba = decode_rgba(handle, info, palette)
        rgb = bytes(
            channel
            for pixel in range(0, len(rgba), 4)
            for channel in (rgba[pixel], rgba[pixel + 1], rgba[pixel + 2])
        )
        rgba_sha = sha256_bytes(rgba)
        image = Image.frombytes("RGBA", (info["width"], info["height"]), rgba)
        buffer = BytesIO()
        image.save(buffer, format="PNG", optimize=False)
        png = buffer.getvalue()
        filename = f"{image_index:04d}.png"
        target = destination / filename
        if force or not target.is_file() or target.read_bytes() != png:
            target.write_bytes(png)
        exported += 1
        records.append(
            {
                "index": image_index,
                "width": info["width"],
                "height": info["height"],
                "offset_x": info["offset_x"],
                "offset_y": info["offset_y"],
                "format": info["format"],
                "png": filename,
                "bytes": len(png),
                "raw_bytes": len(rgba),
                "rgb_sha256": sha256_bytes(rgb),
                "rgba_sha256": rgba_sha,
                "pixels_sha256": rgba_sha,
                "png_sha256": sha256_bytes(png),
                "empty": False,
            }
        )
    index_json = {
        "lib": entry["lib"],
        "source": entry["source"],
        "category": entry["category"],
        "entity": output.get("entity"),
        "reason": output.get("reason") or entry["reason"],
        "count": exported,
        "requested": len(wanted),
        "skipped_empty": skipped,
        "sample": True,
        "alpha": "palette8 的 (0,0,0) 已按 remove_black 规则置 alpha=0；rgb_sha256 用于跨实现解码对账",
        "entries": records,
    }
    (destination / "index.json").write_text(
        json.dumps(index_json, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    return {
        "lib": entry["lib"],
        "dest": output["dest"],
        "entity": output.get("entity"),
        "exported": exported,
        "skipped": skipped,
        "png_bytes": sum(record["bytes"] for record in records if not record["empty"]),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="导出 game-assets-mini 最小样例")
    parser.add_argument("--plan", default="target/used-assets.json")
    parser.add_argument("--out", default="target/game-assets-mini")
    parser.add_argument("--map", default="0", help="瓦片样例优先取哪张地图用到的（默认 0）")
    parser.add_argument("--per-lib", type=int, default=None, help="每库固定上限（覆盖分类默认值）")
    parser.add_argument("--all", action="store_true", help="不截断（全量导出；可能很大）")
    parser.add_argument("--copy-maps", action="store_true", default=True, help="复制样例地图（默认开）")
    parser.add_argument("--no-copy-maps", action="store_false", dest="copy_maps")
    parser.add_argument("--copy-audio", action="store_true", default=True, help="复制样例音频（默认开）")
    parser.add_argument("--no-copy-audio", action="store_false", dest="copy_audio")
    parser.add_argument("--force", action="store_true", help="强制重写 PNG")
    args = parser.parse_args(argv)

    started = time.time()
    plan = json.loads((REPO_ROOT / args.plan).read_text(encoding="utf-8"))
    out_root = REPO_ROOT / args.out
    out_root.mkdir(parents=True, exist_ok=True)
    palette = load_palette()
    map0 = au.RefBook()
    if (au.MAP_DIR_CJLAAA / f"{args.map}.map").is_file():
        au.scan_maps(au.MAP_DIR_CJLAAA, [args.map], map0, f"map{args.map}")

    results = []
    handles: dict[str, tuple] = {}
    for entry in plan["libraries"]:
        source = REPO_ROOT / entry["source"]
        if not source.is_file():
            print(f"  [跳过] {entry['lib']}：源文件不存在（{entry['source']}）", flush=True)
            continue
        outputs = entry.get("outputs") or [
            {"dest": entry["dest"], "indices": entry["indices"], "entity": None}
        ]
        for output in outputs:
            if not output["indices"]:
                continue
            limit = args.per_lib or LIMITS.get(entry["category"], 64)
            if args.all:
                limit = len(output["indices"])
            result = export_output(
                entry, output, out_root, palette, map0, limit, args.force, handles
            )
            results.append(result)
            print(
                f"  {result['dest']:<34} 导出 {result['exported']:>4} 帧"
                f" / 空 {result['skipped']:>3}（{result['png_bytes']/1e6:.1f} MB）",
                flush=True,
            )
    for index, handle in handles.values():
        index.close()
        handle.close()

    copied = {"maps": [], "audio": []}
    if args.copy_maps:
        map_path = au.MAP_DIR_CJLAAA / f"{args.map}.map"
        if map_path.is_file():
            (out_root / "maps").mkdir(exist_ok=True)
            shutil.copy2(map_path, out_root / "maps" / map_path.name)
            copied["maps"].append({"file": map_path.name, "bytes": map_path.stat().st_size})
    if args.copy_audio:
        audio_dir = out_root / "audio"
        audio_dir.mkdir(exist_ok=True)
        for name in plan.get("audio", [])[:4]:
            for root in AUDIO_ROOTS:
                candidate = root / name
                if candidate.is_file():
                    shutil.copy2(candidate, audio_dir / name)
                    copied["audio"].append({"file": name, "bytes": candidate.stat().st_size})
                    break

    manifest = {
        "kind": "game-assets-mini（最小可验证样例）",
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "source_plan": args.plan,
        "dataset": plan.get("dataset"),
        "sample_limit": "all" if args.all else (args.per_lib or LIMITS),
        "libraries": results,
        "copied": copied,
        "totals": {
            "libraries": len(results),
            "png_files": sum(result["exported"] for result in results),
            "png_bytes": sum(result["png_bytes"] for result in results)
            + sum(item["bytes"] for item in copied["maps"] + copied["audio"]),
        },
        "note": "样例只含每库前 N 帧；全量请用 mir-tools assets unpack --plan target/used-assets.json",
    }
    (out_root / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(
        f"\n== 样例完成 == {out_root}：{len(results)} 库 / {manifest['totals']['png_files']} PNG / "
        f"{manifest['totals']['png_bytes']/1e6:.1f} MB，{time.time() - started:.1f}s"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
