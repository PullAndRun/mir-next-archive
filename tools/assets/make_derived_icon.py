#!/usr/bin/env python3
"""生成"我们自绘"的衍生物品图标（`origin: derived`），用于补齐无图标的 GM/占位模板。

与 `data_import` 写出的 `index.json` 同一 schema：除尺寸/偏移外还写
`bytes` / `png_sha256` / `rgb_sha256` / `rgba_sha256` / `pixels_sha256`（= rgba 别名），
避免对账器报 image-sha-mismatch。

做法：取一张现有图标作为**形状来源**，去饱和 + 钢灰调（线性 RGB，保留 alpha），
再按 items 规则做**边框连通**去黑（只去掉与边框相连的近黑不透明像素）。

用法：

    python tools/assets/make_derived_icon.py \\
        --source game-assets/gfx/sprites/item/icon-0433/0433.png \\
        --index 3576 --out game-assets/gfx/sprites/item/icon-3576 \\
        --derived-from icon-0433 --note "我们自绘的 GM 戒指占位图（非 Crystal 原图）"

产物：`<out>/<index:04>.png` + `<out>/index.json`（`origin=derived`、`derived_from=...`）。
"""

from __future__ import annotations

import argparse
import hashlib
import json
from collections import deque
from pathlib import Path

from PIL import Image

# 钢灰调：luma → (scale, offset) 三通道，略偏蓝
STEEL = (
    (0.72, 55.0),
    (0.76, 62.0),
    (0.82, 74.0),
)


def luma(r: int, g: int, b: int) -> float:
    return 0.299 * r + 0.587 * g + 0.114 * b


def remove_border_black(image: Image.Image, threshold: int = 10) -> int:
    """从四条边泛洪，把与边框相连的近黑不透明像素设为透明；返回去掉的像素数。"""
    width, height = image.size
    pixels = image.load()

    def opaque_black(x: int, y: int) -> bool:
        r, g, b, a = pixels[x, y]
        return a > 0 and r <= threshold and g <= threshold and b <= threshold

    seen = [[False] * width for _ in range(height)]
    queue: deque[tuple[int, int]] = deque()
    for x in range(width):
        for y in (0, height - 1):
            if opaque_black(x, y) and not seen[y][x]:
                seen[y][x] = True
                queue.append((x, y))
    for y in range(height):
        for x in (0, width - 1):
            if opaque_black(x, y) and not seen[y][x]:
                seen[y][x] = True
                queue.append((x, y))

    removed = 0
    while queue:
        x, y = queue.popleft()
        pixels[x, y] = (0, 0, 0, 0)
        removed += 1
        for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)):
            if 0 <= nx < width and 0 <= ny < height and not seen[ny][nx] and opaque_black(nx, ny):
                seen[ny][nx] = True
                queue.append((nx, ny))
    return removed


def hashes(png_path: Path, image: Image.Image) -> dict[str, object]:
    rgba = bytes(channel for pixel in image.getdata() for channel in pixel)
    rgb = bytes(channel for pixel in image.getdata() for channel in pixel[:3])
    return {
        "bytes": len(rgba),
        "png_sha256": hashlib.sha256(png_path.read_bytes()).hexdigest(),
        "rgb_sha256": hashlib.sha256(rgb).hexdigest(),
        "rgba_sha256": hashlib.sha256(rgba).hexdigest(),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, help="形状来源 PNG（现有图标）")
    parser.add_argument("--index", type=int, required=True, help="原库 0 基索引（= image 编号）")
    parser.add_argument("--out", required=True, help="输出目录（icon-XXXX）")
    parser.add_argument("--derived-from", required=True, help="来源目录名，如 icon-0433")
    parser.add_argument("--note", default="我们自绘的占位图标，非 Crystal 原图")
    args = parser.parse_args()

    source_path = Path(args.source)
    if not source_path.is_file():
        raise SystemExit(f"缺来源图标 {source_path}")
    source = Image.open(source_path).convert("RGBA")
    width, height = source.size
    source_pixels = source.load()

    target = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    target_pixels = target.load()
    for y in range(height):
        for x in range(width):
            r, g, b, a = source_pixels[x, y]
            if a == 0:
                continue
            level = luma(r, g, b)
            target_pixels[x, y] = (
                min(255, max(0, round(level * STEEL[0][0] + STEEL[0][1]))),
                min(255, max(0, round(level * STEEL[1][0] + STEEL[1][1]))),
                min(255, max(0, round(level * STEEL[2][0] + STEEL[2][1]))),
                a,
            )
    removed = remove_border_black(target)

    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    png_name = f"{args.index:04}.png"
    png_path = out_dir / png_name
    target.save(png_path, format="PNG", optimize=True)
    digest = hashes(png_path, target)

    index = {
        "version": 1,
        "count": 1,
        "format": "png",
        "lib": "items.wzl",
        "origin": "derived",
        "derived_from": args.derived_from,
        "note": args.note,
        "written_png": 1,
        "images": [
            {
                "index": args.index,
                "png": png_name,
                "empty": False,
                "width": width,
                "height": height,
                "offset_x": 0,
                "offset_y": 0,
                "shadow_x": 0,
                "shadow_y": 0,
                "has_mask": False,
                "format": "rgba8",
                "format_code": 0,
                "reserved": [0, 0, 0],
                "png_sha256": digest["png_sha256"],
                "rgb_sha256": digest["rgb_sha256"],
                "rgba_sha256": digest["rgba_sha256"],
                "pixels_sha256": digest["rgba_sha256"],
                "bytes": digest["bytes"],
            }
        ],
    }
    (out_dir / "index.json").write_text(
        json.dumps(index, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    visible = sum(1 for pixel in target.getdata() if pixel[3] > 0)
    print(
        f"icon-{args.index:04}: {width}x{height} · 可见 {visible}/{width * height} 像素 · 去黑 {removed}"
    )
    print(f"png={png_path} bytes={png_path.stat().st_size} sha256={digest['png_sha256']}")
    print(
        f"rgb_sha256={digest['rgb_sha256']} rgba_sha256={digest['rgba_sha256']} "
        f"pixels_sha256={digest['rgba_sha256']} bytes={digest['bytes']}"
    )


if __name__ == "__main__":
    main()
