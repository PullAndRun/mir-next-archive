#!/usr/bin/env python3
"""对账发布期小包（`.mnp`）与散装资源：条目级 sha256 + 覆盖率 + 包完整性。

用途：户部 `mir-tools assets pack` 打完包后，确认「包里的内容 = `game-assets/` 里的内容」，
且没有漏打/多打；`--report` 输出机器可读结果，退出码 0/1 可直接当门禁。

对账规则（与 `docs/assets/asset-pipeline.md` §8 的包格式一致）：

| 条目 kind | 含义 | 对账方式 |
|---|---|---|
| `1` | 原样字节（`.mnm`/`.mnd`/`.mna`/`index.json`） | 解包字节 sha256 == 散装文件 sha256 |
| `0` | zlib(RGBA) 图像 | 解压后 sha256 == 同目录 `index.json` 的 `pixels_sha256`；`raw_len == w*h*4` |
| `2` | zlib(8bit 索引) 图像 | 用同目录 `palette.bin`（kind=3）展开 RGB，sha256 == `index.json.rgb_sha256`（忽略 alpha）；`raw_len == w*h` |
| `3` | 每目录 768B 调色板（RGB，包内独有） | 条目存在且长度 768；不要求散装落盘 |

同时校验：
- `packs/manifest.packs.json` 里每个包的 `sha256` = 包文件 sha256、`entries` = 实际条目数；
- 反向覆盖：`--categories`（默认 `gfx,maps,data,index,audio`）下的每个文件都能在某个包里按逻辑名找到。

```powershell
py tools/assets/reconcile_packs.py
py tools/assets/reconcile_packs.py --categories maps,data,index          # 只查原样条目（秒级）
py tools/assets/reconcile_packs.py --quick                              # 只查覆盖率，不重算图像哈希
py tools/assets/reconcile_packs.py --report target/packs-reconcile.json
```
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import struct
import sys
import zlib
from collections import Counter, defaultdict
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_PACKS = REPO_ROOT / "game-assets" / "packs"
DEFAULT_GAME = REPO_ROOT / "game-assets"


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def parse_pack(path: Path) -> dict:
    """解析 `.mnp`：header（32B）+ 条目表（`index_offset`）+ 可选尾部整包 sha256。"""
    data = path.read_bytes()
    magic, version, flags, entry_count, index_offset = struct.unpack_from("<4sHHIQ", data, 0)
    if magic != b"MNP1":
        raise ValueError(f"{path} 不是 MNP1 包（magic={magic!r}）")
    entries = []
    offset = index_offset
    for _ in range(entry_count):
        (name_len,) = struct.unpack_from("<H", data, offset)
        offset += 2
        name = data[offset:offset + name_len].decode("utf-8")
        offset += name_len
        kind, has_mask, pixel_format = struct.unpack_from("<BBB", data, offset)
        offset += 6  # kind + has_mask + format + 3 字节保留
        width, height = struct.unpack_from("<HH", data, offset)
        offset += 4
        offset += 8  # 4 个 i16 偏移
        data_offset, data_len, raw_len = struct.unpack_from("<QII", data, offset)
        offset += 16
        entries.append(
            {
                "name": name,
                "kind": kind,
                "format": pixel_format,
                "has_mask": bool(has_mask),
                "width": width,
                "height": height,
                "data_offset": data_offset,
                "data_len": data_len,
                "raw_len": raw_len,
            }
        )
    trailing = data[-32:] if flags & 1 else None
    return {
        "path": path,
        "flags": flags,
        "entry_count": entry_count,
        "entries": entries,
        "data": data,
        "file_sha256": sha256_bytes(data),
        "trailing_sha256": trailing.hex() if trailing else None,
        "content_sha256": sha256_bytes(data[:-32]) if trailing else sha256_bytes(data),
    }


def payload_bytes(pack: dict, entry: dict) -> bytes:
    payload = pack["data"][entry["data_offset"]:entry["data_offset"] + entry["data_len"]]
    if entry["kind"] in (0, 2):
        return zlib.decompress(payload)
    return payload


def load_wemade_palette() -> bytes:
    """从 `crates/mir-asset/src/palette.rs` 解析 WeMade 256 色，返回 768B RGB（回退用）。"""
    palette_path = REPO_ROOT / "crates" / "mir-asset" / "src" / "palette.rs"
    text = palette_path.read_text(encoding="utf-8")
    # 只解析 `WEMADE_PALETTE: [u32; 256] = [...]` 这一段（文件里还有 0x00FF_FFFF 之类常量）
    block = text.split("WEMADE_PALETTE: [u32; 256] = [", 1)[1].split("];", 1)[0]
    values = [high + low for high, low in re.findall(r"0x([0-9A-Fa-f]{4})_([0-9A-Fa-f]{4})", block)]
    if len(values) != 256:
        raise ValueError(f"palette.rs 解析出 {len(values)} 项，应为 256")
    palette = bytearray()
    for argb in values:
        value = int(argb, 16)
        palette.extend(((value >> 16) & 0xFF, (value >> 8) & 0xFF, value & 0xFF))
    return bytes(palette)


def expand_palette8(indices: bytes, palette: bytes) -> bytes:
    """8bit 索引展开成 RGB（`index==0` 的 alpha 约定与对账无关，这里只产出 RGB）。

    用 3 张 256B 查找表 + `bytes.translate` + 步进切片，避免逐像素 Python 循环
    （15 万帧 × 百万像素级，逐像素会慢两个数量级）。
    """
    if len(palette) != 768:
        raise ValueError(f"调色板应为 768B，实际 {len(palette)}B")
    red = bytes(palette[index * 3] for index in range(256))
    green = bytes(palette[index * 3 + 1] for index in range(256))
    blue = bytes(palette[index * 3 + 2] for index in range(256))
    rgb = bytearray(len(indices) * 3)
    rgb[0::3] = indices.translate(red)
    rgb[1::3] = indices.translate(green)
    rgb[2::3] = indices.translate(blue)
    return bytes(rgb)


def load_index_hashes(directory: Path, cache: dict[Path, dict[int, dict]]) -> dict[int, dict]:
    if directory in cache:
        return cache[directory]
    index_path = directory / "index.json"
    mapping: dict[int, dict] = {}
    if index_path.is_file():
        payload = json.loads(index_path.read_text(encoding="utf-8"))
        for image in payload.get("images") or payload.get("entries") or []:
            if not image.get("empty", False):
                mapping[int(image["index"])] = {
                    "rgba": image.get("pixels_sha256") or image.get("rgba_sha256"),
                    "rgb": image.get("rgb_sha256"),
                }
    cache[directory] = mapping
    return mapping


def expected_files(game: Path, categories: list[str], include_index_json: bool) -> list[Path]:
    """散装侧应被包覆盖的文件。

    默认跳过每个图库目录的 `index.json`：它是开发期元数据，包条目自带 width/height/offset/format，
    发布包不需要它；要连它一起对账用 `--include-index-json`。
    """
    files = []
    for category in categories:
        root = game / category
        if root.is_dir():
            files.extend(
                path
                for path in root.rglob("*")
                if path.is_file() and (include_index_json or path.name != "index.json")
            )
    return sorted(files)


def reconcile(
    packs_dir: Path,
    game: Path,
    categories: list[str],
    quick: bool,
    include_index_json: bool,
    report_path: Path | None,
) -> dict:
    manifest_path = packs_dir / "manifest.packs.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8")) if manifest_path.is_file() else None
    declared = {item["file"]: item for item in (manifest or {}).get("packs", [])}

    packs = {}
    issues: list[dict] = []
    for pack_path in sorted(packs_dir.glob("*.mnp")):
        pack = parse_pack(pack_path)
        packs[pack_path.name] = pack
        info = declared.get(pack_path.name)
        if info is None:
            issues.append({"kind": "pack-not-in-manifest", "pack": pack_path.name})
            continue
        if info.get("sha256") and info["sha256"] != pack["file_sha256"]:
            issues.append(
                {
                    "kind": "pack-sha-mismatch",
                    "pack": pack_path.name,
                    "declared": info["sha256"][:16],
                    "actual": pack["file_sha256"][:16],
                }
            )
        if int(info.get("entries", -1)) != pack["entry_count"]:
            issues.append(
                {
                    "kind": "pack-entry-count-mismatch",
                    "pack": pack_path.name,
                    "declared": info.get("entries"),
                    "actual": pack["entry_count"],
                }
            )

    entries: dict[str, tuple[dict, dict]] = {}
    for name, pack in packs.items():
        for entry in pack["entries"]:
            if entry["name"] in entries:
                issues.append({"kind": "duplicate-entry", "name": entry["name"]})
            entries[entry["name"]] = (pack, entry)

    # kind=3：每目录一条 palette.bin（768B RGB），只在包里存，用于展开 kind=2 的 8bit 索引
    palettes: dict[str, bytes] = {}
    for name, (pack, entry) in entries.items():
        if entry["kind"] != 3:
            continue
        palette = payload_bytes(pack, entry)
        if len(palette) != 768:
            issues.append({"kind": "palette-size", "name": name, "bytes": len(palette)})
            continue
        palettes[str(Path(name).parent).replace("\\", "/")] = palette
    fallback_palette = (
        load_wemade_palette() if any(entry["kind"] == 2 for _, entry in entries.values()) else None
    )

    index_cache: dict[Path, dict[int, dict]] = {}
    stats: Counter[str] = Counter()
    stats["palette_entries"] = len(palettes)
    per_category = defaultdict(lambda: Counter())
    for path in expected_files(game, categories, include_index_json):
        logical = path.relative_to(game).as_posix()
        category = logical.split("/", 1)[0]
        stats["expected"] += 1
        per_category[category]["expected"] += 1
        found = entries.get(logical)
        if found is None:
            stats["missing_in_packs"] += 1
            per_category[category]["missing_in_packs"] += 1
            issues.append({"kind": "missing-in-pack", "name": logical})
            continue
        pack, entry = found
        if entry["kind"] == 1:
            actual = sha256_bytes(payload_bytes(pack, entry))
            expected = sha256_bytes(path.read_bytes())
            if actual == expected:
                stats["matched_raw"] += 1
                per_category[category]["matched"] += 1
            else:
                stats["mismatched"] += 1
                per_category[category]["mismatched"] += 1
                issues.append(
                    {"kind": "sha-mismatch", "name": logical, "expected": expected[:16], "actual": actual[:16]}
                )
        elif entry["kind"] == 0:
            if quick:
                stats["skipped_image_hash"] += 1
                continue
            expected = load_index_hashes(path.parent, index_cache).get(int(path.stem))
            if expected is None:
                stats["index-missing"] += 1
                per_category[category]["index_missing"] += 1
                issues.append({"kind": "index-entry-missing", "name": logical})
                continue
            rgba = payload_bytes(pack, entry)
            actual = sha256_bytes(rgba)
            if entry["raw_len"] != entry["width"] * entry["height"] * 4:
                stats["raw-len-mismatch"] += 1
                issues.append(
                    {
                        "kind": "raw-len-mismatch",
                        "name": logical,
                        "raw_len": entry["raw_len"],
                        "expected": entry["width"] * entry["height"] * 4,
                    }
                )
            if actual == expected["rgba"]:
                stats["matched_image"] += 1
                per_category[category]["matched"] += 1
            else:
                stats["mismatched"] += 1
                per_category[category]["mismatched"] += 1
                issues.append(
                    {
                        "kind": "image-sha-mismatch",
                        "name": logical,
                        "expected": (expected["rgba"] or "")[:16],
                        "actual": actual[:16],
                    }
                )
        elif entry["kind"] == 2:
            # palette8：zlib(索引) + 同目录 palette.bin（kind=3）展开，只比 RGB（alpha 由去黑约定决定）
            if quick:
                stats["skipped_image_hash"] += 1
                continue
            expected = load_index_hashes(path.parent, index_cache).get(int(path.stem))
            if expected is None or expected.get("rgb") is None:
                stats["index-missing"] += 1
                per_category[category]["index_missing"] += 1
                issues.append({"kind": "index-entry-missing", "name": logical})
                continue
            indices = payload_bytes(pack, entry)
            pixels = entry["width"] * entry["height"]
            if entry["raw_len"] != pixels or len(indices) != pixels:
                stats["raw-len-mismatch"] += 1
                issues.append(
                    {
                        "kind": "raw-len-mismatch",
                        "name": logical,
                        "raw_len": entry["raw_len"],
                        "expected": pixels,
                    }
                )
            palette = palettes.get(str(Path(logical).parent).replace("\\", "/"))
            if palette is None:
                palette = fallback_palette
                stats["palette-fallback"] += 1
                per_category[category]["palette_fallback"] += 1
            rgb = expand_palette8(indices, palette)
            actual = sha256_bytes(rgb)
            if actual == expected["rgb"]:
                stats["matched_palette8"] += 1
                per_category[category]["matched"] += 1
            else:
                stats["mismatched"] += 1
                per_category[category]["mismatched"] += 1
                issues.append(
                    {
                        "kind": "palette8-rgb-mismatch",
                        "name": logical,
                        "expected": expected["rgb"][:16],
                        "actual": actual[:16],
                    }
                )
        elif entry["kind"] == 3:
            # palette.bin：包内独有（散装不落盘），前面已按 768B 校验
            stats["palette_entries"] += 1
        else:
            stats["unknown-entry-kind"] += 1
            issues.append({"kind": "unknown-entry-kind", "name": logical, "entry_kind": entry["kind"]})

    expected_names = {
        path.relative_to(game).as_posix()
        for path in expected_files(game, categories, include_index_json)
    }
    extra = [
        name
        for name in entries
        if name.split("/", 1)[0] in categories
        and name not in expected_names
        and not name.endswith("/palette.bin")
    ]
    for name in extra:
        stats["extra_in_packs"] += 1
        issues.append({"kind": "extra-in-pack", "name": name})

    result = {
        "packs_dir": str(packs_dir),
        "packs": len(packs),
        "pack_bytes": sum(len(pack["data"]) for pack in packs.values()),
        "manifest": str(manifest_path) if manifest else None,
        "categories": categories,
        "entry_kinds": dict(sorted(Counter(entry["kind"] for _, entry in entries.values()).items())),
        "stats": dict(stats),
        "per_category": {name: dict(counter) for name, counter in sorted(per_category.items())},
        "issues": issues[:200],
        "issue_count": len(issues),
    }
    if report_path is not None:
        report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="对账 .mnp 包与散装资源（sha256/覆盖率/包完整性）")
    parser.add_argument("--packs", default=str(DEFAULT_PACKS))
    parser.add_argument("--game-assets", default=str(DEFAULT_GAME))
    parser.add_argument("--categories", default="gfx,maps,data,index,audio,fonts")
    parser.add_argument("--quick", action="store_true", help="跳过图像条目哈希（只查覆盖率与包完整性）")
    parser.add_argument(
        "--include-index-json",
        action="store_true",
        help="连每个图库目录的 index.json 一起对账（默认跳过：开发期元数据，包条目自带元数据）",
    )
    parser.add_argument("--report", default=str(REPO_ROOT / "target" / "packs-reconcile.json"))
    args = parser.parse_args(argv)

    packs_dir = Path(args.packs).resolve()
    game = Path(args.game_assets).resolve()
    if not packs_dir.is_dir():
        raise SystemExit(f"[错误] 包目录不存在：{packs_dir}")
    categories = [item.strip() for item in args.categories.split(",") if item.strip()]
    result = reconcile(
        packs_dir, game, categories, args.quick, args.include_index_json, Path(args.report).resolve()
    )
    print(json.dumps(result, ensure_ascii=False, indent=2))
    if result["issue_count"]:
        print(f"\n[失败] {result['issue_count']} 个问题，详见 {args.report}", file=sys.stderr)
        return 1
    print(f"\n[通过] {result['packs']} 个包 / 条目对账全绿（详见 {args.report}）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
