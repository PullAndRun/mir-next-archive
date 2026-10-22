#!/usr/bin/env python3
"""素材引用分析：算出「哪些图库的哪些帧真的会被游戏用到」，产出拆包计划。

设计目标（第 26 轮用户要求）：

> 把原版素材包拆开，只用正在用的素材，分类存放到 `game-assets/`。

本脚本只做**只读分析**：扫描默认数据集（`Server.MirDB`）、地图文件、客户端/渲染层代码里的
图库引用，输出：

1. `target/used-assets.json` —— 给 Rust 拆包器（`mir-tools assets unpack --plan`）吃的中间格式；
2. `target/asset-usage-stats.json` —— 每个 `.wzl` 的 entries / used / unused / 体积统计；
3. `target/asset-usage-report.md` —— 人读的汇总表（`docs/assets/asset-pipeline.md` 引用它）。

判定口径与证据见 `docs/assets/asset-pipeline.md`。核心规则：

- **地图瓦片**：解析 `Server.MirDB` 里 `MapInfo` 引用的 `.map`，按 Crystal 客户端
  `MapCode.LoadMapType1/0/2/3/4` 的字段布局取出 Back/Middle/Front 图像号；
  后层 → `tiles.wzl`、中层 → `smtiles.wzl`、前层 → `objects*.wzl`（`front_index + 2`）。
- **怪物/NPC**：模板的 `Image` → `mon{Image/10+1}.wzl` 槽位 / `npc{Image/100+1}.wzl` 槽位，
  只取 `mir-render/src/animation.rs` 里**动作帧表实际会播到的帧**（不是整块 430 帧）。
- **玩家外观**：物品模板 `Shape` → `hum*/weapon*` 块；发型号 0..8 → `hair*`；
  盔甲 `Effect` → `wingeffect_x.wzl`。
- **物品图标**：`items.wzl`，模板 `Image` 即 0 基下标。
- **小地图/大地图**：`mmap.wzl`，取 `MapInfo.MiniMap` / `MapInfo.BigMap` 非 0 帧。
- **特效**：`effect.wzl` / `magic.wzl` / `magic2.wzl` 的固定帧段。
- **音频**：`mir-render/src/sound.rs` 的事件映射 + 技能施法音 + 背景音乐兜底。
- **其余图库一律不拆**（`ui*.wzl`、`hum_ef/weapon_ef/cbo*`、`tiles2..`、`smtiles2..`、
  `objects28+`、`*.wil` 等）：当前客户端代码没有任何引用路径。

用法：

```powershell
# 推荐：用带 numpy 的 Python 跑（389 张 v1 地图约 5~10s；纯标准库要几分钟）
& "C:\\Users\\diana\\.dsh\\dsh-runtimes\\dsh-primary-runtime\\dependencies\\python\\python.exe" `
    tools/assets/analyze_usage.py

# 只重算地图瓦片 / 只看统计
py tools/assets/analyze_usage.py --skip-audio
```

首次运行需要 `target/asset-probe/db.json`：脚本会自动用 `rustc` 编译
`target/asset-probe/db_probe.rs` 并链接 `target/debug/deps/libmir_db-*.rlib`（先 `cargo build -p mir-db`）。
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import time
from collections import Counter, defaultdict
from pathlib import Path

DB_PROBE_SOURCE = r"""//! 临时 Rust 探针（由 `tools/assets/analyze_usage.py` 按需编译）：
//! 把 `Server.MirDB` 里「素材引用分析」需要的字段导出成 JSON。
//!
//! 用法：
//!
//! ```text
//! rustc --edition 2024 -O target/asset-probe/db_probe.rs -o target/asset-probe/db_probe.exe \
//!       -L dependency=target/debug/deps --extern mir_db=<libmir_db-*.rlib>
//! target/asset-probe/db_probe.exe <Server.MirDB> <输出 json>
//! ```
//!
//! 只读 `MirDb` 的公开字段，不写库、不改游戏代码。脚本会自动找最新的
//! `target/debug/deps/libmir_db-*.rlib`（需先 `cargo build -p mir-db`）。
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::Write as _;

fn esc(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("用法：db_probe <Server.MirDB> <输出 json>");
    let out_path = args.next().expect("缺少输出 JSON 路径");
    let db = mir_db::MirDb::load(&path).expect("解析 Server.MirDB 失败");

    // 哪些怪物真的会被刷出来（地图 RespawnInfo 引用了它的模板号）。
    let mut spawned: BTreeSet<i32> = BTreeSet::new();
    for map in &db.maps {
        for respawn in &map.respawns {
            spawned.insert(respawn.monster_index);
        }
    }

    let mut out = String::with_capacity(1 << 20);
    out.push_str("{\n\"maps\":[");
    for (i, map) in db.maps.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "\n{{\"index\":{},\"file\":{},\"title\":{},\"mini_map\":{},\"big_map\":{},\"music\":{}}}",
            map.index,
            esc(&map.file_name),
            esc(&map.title),
            map.mini_map,
            map.big_map,
            map.music,
        );
    }
    out.push_str("\n],\n\"monsters\":[");
    for (i, monster) in db.monsters.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "\n{{\"index\":{},\"name\":{},\"image\":{},\"ai\":{},\"spawned\":{}}}",
            monster.index,
            esc(&monster.name),
            monster.image,
            monster.ai,
            spawned.contains(&monster.index),
        );
    }
    out.push_str("\n],\n\"npcs\":[");
    for (i, npc) in db.npcs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "\n{{\"index\":{},\"name\":{},\"image\":{},\"map_index\":{}}}",
            npc.index,
            esc(&npc.name),
            npc.image,
            npc.map_index,
        );
    }
    out.push_str("\n],\n\"items\":[");
    for (i, item) in db.items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "\n{{\"index\":{},\"name\":{},\"type\":{},\"image\":{},\"shape\":{},\"effect\":{}}}",
            item.index,
            esc(&item.name),
            item.item_type.value(),
            item.image,
            item.shape,
            item.effect,
        );
    }
    out.push_str("\n],\n\"magics\":[");
    for (i, magic) in db.magics.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "\n{{\"name\":{},\"spell\":{},\"icon\":{}}}",
            esc(&magic.name),
            magic.spell,
            magic.icon,
        );
    }
    out.push_str("\n]\n}\n");

    let mut file = std::fs::File::create(&out_path).expect("创建输出文件失败");
    file.write_all(out.as_bytes()).expect("写输出文件失败");
    println!(
        "maps={} monsters={} npcs={} items={} magics={} spawned={} -> {}",
        db.maps.len(),
        db.monsters.len(),
        db.npcs.len(),
        db.items.len(),
        db.magics.len(),
        spawned.len(),
        out_path
    );
}
"""


REPO_ROOT = Path(__file__).resolve().parents[2]
DATA_DIR = REPO_ROOT / "resource" / "data"
MAP_DIR_CJLAAA = REPO_ROOT / "resource" / "community" / "suprcode-crystal-database" / "cjlaaa" / "Maps"
DATASET_DIR = REPO_ROOT / "resource" / "community" / "suprcode-crystal-database" / "cjlaaa"
MAP_DIR_RESOURCE = REPO_ROOT / "resource" / "map"
PROBE_DIR = REPO_ROOT / "target" / "asset-probe"
DB_JSON = PROBE_DIR / "db.json"
AUDIO_ROOTS = [
    REPO_ROOT / "resource" / "wav",
    REPO_ROOT / "resource" / "community" / "mirfiles-crystal-client" / "Client" / "Sound",
    REPO_ROOT / "resource" / "community" / "gqf2008-crystal" / "Sound",
]
FONTS_DIR = REPO_ROOT / "resource" / "fonts"

# Crystal `MapLibs` 编号 → `resource/data` 文件名（与 `crates/mir-render/src/tiles.rs` 的
# `LIBRARY_FILES` 逐项一致；改那边要同步改这里，脚本自测会比对）。
LIBRARY_FILES: dict[int, str] = {
    0: "tiles.wzl",
    1: "smtiles.wzl",
    2: "objects.wzl",
    3: "objects2.wzl",
    4: "objects3.wzl",
    5: "objects4.wzl",
    6: "objects5.wzl",
    7: "objects6.wzl",
    8: "objects7.wzl",
    9: "objects8.wzl",
    10: "objects9.wzl",
    11: "objects10.wzl",
    12: "objects11.wzl",
    13: "objects12.wzl",
    14: "objects13.wzl",
    15: "objects14.wzl",
    16: "objects15.wzl",
    17: "objects16.wzl",
    18: "objects17.wzl",
    19: "objects18.wzl",
    20: "objects19.wzl",
    21: "objects20.wzl",
    22: "objects21.wzl",
    23: "objects22.wzl",
    24: "objects23.wzl",
    25: "objects24.wzl",
    26: "objects25.wzl",
    27: "objects26.wzl",
    28: "objects27.wzl",
    90: "objects_32bit.wzl",
    190: "anitiles1.wzl",
}

# 帧布局常量（`crates/mir-render/src/entities.rs` / `animation.rs`）：
PLAYER_FRAMES_PER_APPEARANCE = 600
MONSTER_FRAMES_PER_IMAGE = 430
NPC_FRAMES_PER_IMAGE = 60
MOUNT_FRAMES_PER_APPEARANCE = 640
WING_FRAMES_PER_APPEARANCE = 600
HUM_BLOCKS = [24, 30, 46]
HAIR_BLOCKS = [6, 22, 5, 152]
HORSE_X_BLOCKS = 3
WING_BLOCKS = 2


def sequence(start: int, count: int, skip: int) -> list[int]:
    """`FrameSequence` 在 8 个方向上的全部帧号（不含外观块基址）。"""
    stride = count + skip
    return [start + direction * stride + local for direction in range(8) for local in range(count)]


def union_sequences(*sequences: tuple[int, int, int]) -> list[int]:
    """多个动作帧表的并集（升序、去重）。"""
    frames: set[int] = set()
    for start, count, skip in sequences:
        frames.update(sequence(start, count, skip))
    return sorted(frames)


# 人类（hum/weapon/hair 共用）：站立 + 走 + 跑 + 攻击1 + 受击 + 死亡 + 尸体。
HUMAN_FRAMES = union_sequences(
    (0, 4, 4),    # HUMAN_STANDING
    (64, 6, 2),   # HUMAN_WALKING
    (128, 6, 2),  # HUMAN_RUNNING
    (200, 6, 2),  # HUMAN_ATTACK1
    (472, 3, 5),  # HUMAN_STRUCK
    (536, 4, 4),  # HUMAN_DYING
    (539, 1, 7),  # HUMAN_DEAD
)
# 怪物：站立 + 走（跑复用走）+ 攻击1 + 受击 + 死亡 + 尸体。
MONSTER_FRAMES = union_sequences(
    (0, 4, 4),    # MONSTER_STANDING
    (64, 6, 2),   # MONSTER_WALKING
    (128, 6, 2),  # MONSTER_ATTACK1
    (280, 2, 6),  # MONSTER_STRUCK（mon1 图像 0 实测布局）
    (344, 6, 2),  # MONSTER_DYING
    (349, 1, 7),  # MONSTER_DEAD
)
# NPC：站立帧表跨到下一图像（`npc.wzl` 布局），保守保留整块 60 帧。
NPC_FRAMES = list(range(NPC_FRAMES_PER_IMAGE))
# 翅膀：`WING_*` 帧表并集。
WING_FRAMES = union_sequences(
    (0, 8, 0),    # WING_STANDING
    (64, 8, 0),   # WING_WALKING
    (112, 8, 0),  # WING_RUNNING
    (168, 8, 0),  # WING_ATTACKING
    (392, 3, 0),  # WING_STRUCK
    (416, 4, 0),  # WING_DYING
    (419, 1, 3),  # WING_DEAD
)
# 坐骑：当前服务端未实现骑乘（`riding_mount` 恒 false），默认不拆，`--include-reserved` 才收。
MOUNT_FRAMES = union_sequences(
    (0, 4, 4),
    (64, 6, 2),
    (128, 6, 2),
    (192, 3, 5),
    (384, 6, 2),
    (512, 3, 5),
    (576, 1, 7),
)
# 特效帧段（`crates/mir-render/src/entities.rs` FxKind + `mir-client/src/ui/spellfx.rs`）。
EFFECT_FRAMES = list(range(0, 3))          # effect.wzl：挥砍 3 帧
MAGIC_FRAMES = list(range(0, 10))          # magic.wzl：技能命中 10 帧
MAGIC2_FRAMES = list(range(0, 3)) + list(range(20, 30))  # magic2.wzl：挥砍 3 + 雷电束 10


def human_category(lib: str) -> str | None:
    """图库 → 分类（`gfx/<category>/<stem>/`）。"""
    name = lib.lower()
    if name == "tiles.wzl" or name == "smtiles.wzl" or name == "anitiles1.wzl":
        return "tiles"
    if re.fullmatch(r"objects(_32bit)?\d*\.wzl", name) or name == "objects_32bit.wzl":
        return "tiles"
    if re.fullmatch(r"mon\d+\.wzl", name) or re.fullmatch(r"npc\d*\.wzl", name):
        return "sprites"
    if re.fullmatch(r"hum\d*\.wzl", name):
        return "dress"
    if re.fullmatch(r"weapon\d*\.wzl", name):
        return "weapons"
    if re.fullmatch(r"hair\d*\.wzl", name):
        return "hair"
    if name == "items.wzl":
        return "items"
    if name == "mmap.wzl":
        return "ui"
    if re.fullmatch(r"(effect|magic|magic2)\.wzl", name):
        return "effects"
    if name == "wingeffect_x.wzl":
        return "effects"
    if name == "horse_x.wzl":
        return "sprites"
    return None


def lib_stem(lib: str) -> str:
    return Path(lib).stem


def tileset_name(lib: str) -> str:
    """图库 → 我们自己的 tileset 名（小写，不含源库编号歧义）。

    `tiles.wzl → tiles0`、`smtiles.wzl → tiles1`、`objects.wzl → objects0`、
    `objects2.wzl → objects1` …（= Crystal `MapLibs` 编号 - 2）。
    """
    name = lib.lower()
    if name == "tiles.wzl":
        return "tiles0"
    if name == "smtiles.wzl":
        return "tiles1"
    if name == "objects.wzl":
        return "objects0"
    match = re.fullmatch(r"objects(\d+)\.wzl", name)
    if match:
        return f"objects{int(match.group(1)) - 1}"
    if name == "objects_32bit.wzl":
        return "objects32"
    if name == "anitiles1.wzl":
        return "anitiles1"
    return lib_stem(lib).lower()


def default_dest(lib: str) -> str:
    """整库一个目录时的输出路径（tiles/ui/effects）。"""
    category = human_category(lib) or "unused"
    name = lib.lower()
    if category == "tiles":
        return f"gfx/tiles/{tileset_name(lib)}"
    if category == "ui":
        return "gfx/ui/minimap"
    if category == "effects":
        return f"gfx/sprites/effect/{lib_stem(name)}"
    return f"gfx/{category}/{lib_stem(name)}"


def entity_dest(lib: str, entity: str) -> str:
    """同类实体的输出目录：`gfx/sprites/<kind>/<entity>/`。"""
    name = lib.lower()
    if name.startswith("mon"):
        return f"gfx/sprites/monster/{entity}"
    if name.startswith("npc"):
        return f"gfx/sprites/npc/{entity}"
    if name.startswith(("hum", "weapon", "hair", "horse", "wing")):
        return f"gfx/sprites/player/{entity}"
    if name == "items.wzl":
        return f"gfx/sprites/item/{entity}"
    return f"{default_dest(lib)}/{entity}"


def find_type(data: bytes) -> int:
    """Crystal `Map.cs:73-110` 的 `FindType`（v0..v100）。"""
    if len(data) < 52:
        return -1
    if data[2] == 0x43 and data[3] == 0x23:
        return 100
    if data[0] == 0:
        return 5
    if data[0] == 0x0F and data[5] == 0x53 and data[14] == 0x33:
        return 6
    if data[0] == 0x15 and data[4] == 0x32 and data[6] == 0x41 and data[19] == 0x31:
        return 4
    if data[0] == 0x10 and data[2] == 0x61 and data[7] == 0x31 and data[14] == 0x31:
        return 1
    if data[4] == 0x0F or (data[4] == 0x03 and data[18] == 0x0D and data[19] == 0x0A):
        width, height = struct.unpack_from("<HH", data, 0)
        return 3 if len(data) > 52 + width * height * 14 else 2
    if data[0] == 0x0D and data[1] == 0x4C and data[7] == 0x20 and data[11] == 0x6D:
        return 7
    return 0


class RefBook:
    """`lib 文件名 → {indices, reasons, evidence, groups}`。

    `groups` 是「同一图库里按实体拆出去的输出目录」：怪物/NPC/玩家外观/物品图标
    一个实体一个目录（见 `docs/assets/asset-pipeline.md` 的命名规则）；瓦片/UI/特效
    没有 groups，整库一个目标目录。
    """

    def __init__(self) -> None:
        self.libs: dict[str, dict] = {}

    def _record(self, lib: str) -> dict:
        return self.libs.setdefault(
            lib,
            {"indices": set(), "reasons": Counter(), "evidence": Counter(), "groups": {}},
        )

    def add_many(self, lib: str, indices, reason: str, evidence: str | None = None) -> None:
        record = self._record(lib)
        record["indices"].update(indices)
        record["reasons"][reason] += 1
        if evidence:
            record["evidence"][evidence] += 1

    def add_group(
        self,
        lib: str,
        indices,
        reason: str,
        dest: str,
        entity: str,
        evidence: str | None = None,
        aliases: list[int] | None = None,
    ) -> None:
        """按实体分组：同一 lib 的帧将来落到 `dest/`。

        `aliases` 是共用同一套美术、但不用单独落盘的实体号（目前只有翅膀：
        `Effect=2` 与 `Effect=100` 都映射到块 1）。
        """
        record = self._record(lib)
        group = record["groups"].setdefault(
            dest, {"indices": set(), "reason": reason, "entity": entity, "aliases": []}
        )
        group["indices"].update(indices)
        if aliases:
            group["aliases"] = sorted(set(group["aliases"]) | set(aliases))
        self.add_many(lib, indices, reason, evidence)


def load_numpy():
    try:
        import numpy  # noqa: PLC0415

        return numpy
    except ImportError:
        return None


def scan_map_with_numpy(np, raw: bytes, version: int, book: RefBook, stats: dict) -> None:
    """numpy 快速路径：一次把整张地图的格点向量化。"""
    arr = np.frombuffer(raw, dtype=np.uint8)
    if version == 1:
        width = int(arr[21]) | (int(arr[22]) << 8)
        xor = int(arr[23]) | (int(arr[24]) << 8)
        height = int(arr[25]) | (int(arr[26]) << 8)
        width ^= xor
        height ^= xor
        start, stride = 54, 15
    elif version in (0, 2, 3):
        width, height = struct.unpack_from("<HH", raw, 0)
        start, stride = 52, {0: 12, 2: 14, 3: 36}[version]
    elif version == 4:
        width_word, xor, height_word = struct.unpack_from("<HHH", raw, 31)
        width = width_word ^ xor
        height = height_word ^ xor
        start, stride = 64, 12
    else:
        stats["unsupported_versions"][version] = stats["unsupported_versions"].get(version, 0) + 1
        return
    if width <= 0 or height <= 0:
        stats["bad_dimensions"] += 1
        return
    total = width * height
    needed = start + total * stride
    if len(raw) < needed:
        stats["truncated"] += 1
        return
    stats["cells"] += total
    cells = np.lib.stride_tricks.as_strided(
        arr[start:needed], shape=(total, stride), strides=(stride, 1)
    )
    stats["width_height"][f"{width}x{height}"] = stats["width_height"].get(f"{width}x{height}", 0) + 1

    def word(offset: int, width_bytes: int = 2):
        view = cells[:, offset:offset + width_bytes].copy().view("<u4" if width_bytes == 4 else "<u2")
        return view[:, 0]

    back = word(0, 4 if version == 1 else 2)
    if version == 1:
        back = back ^ np.uint32(0xAA38AA38)
    elif version == 4:
        back = back ^ np.uint16(xor)
    if version in (0, 1, 4):
        back_index = back & 0x7FFF
        used = np.unique(back_index[back_index > 0] - 1)
        book.add_many("tiles.wzl", used.tolist(), f"地图后层瓦片（v{version}）")

        middle_offset = 4 if version == 1 else 2
        front_offset = 6 if version == 1 else 4
        middle = word(middle_offset, 2)
        if version in (1, 4):
            middle = middle ^ np.uint16(xor)
        middle = middle & 0x7FFF
        used = np.unique(middle[middle > 0] - 1)
        book.add_many("smtiles.wzl", used.tolist(), f"地图中层瓦片（v{version}）")

        front = word(front_offset, 2)
        if version in (1, 4):
            front = front ^ np.uint16(xor)
        front = front & 0x7FFF
        front_index = cells[:, 10 if version != 1 else 12].astype(np.uint16)
        if version == 1:
            front_index = front_index + 2
            front_index = np.where(front_index == 102, 90, front_index)
            front_index = np.where(front_index >= 255, 0, front_index)
        else:
            front_index = front_index + 2
        mask = front > 0
        pairs = np.stack([front_index[mask], front[mask] - 1], axis=1)
        unique, counts = np.unique(pairs, axis=0, return_counts=True)
        for (library, index), count in zip(unique.tolist(), counts.tolist()):
            lib = LIBRARY_FILES.get(library)
            if lib is None:
                stats["unsupported_front_libs"][library] = (
                    stats["unsupported_front_libs"].get(library, 0) + count
                )
                continue
            book.add_many(lib, [index], f"地图前层瓦片（v{version}）")
        # 前层动画（门）：动画帧号 1..anim 都是实际会画到的帧。
        animation = (cells[:, 8 if version != 1 else 10].astype(np.uint16)) & 0x7F
        animated = mask & (animation > 0)
        for library, base, frames in zip(
            front_index[animated].tolist(),
            (front[animated] - 1).tolist(),
            animation[animated].tolist(),
        ):
            lib = LIBRARY_FILES.get(library)
            if lib is None or frames == 0:
                continue
            book.add_many(lib, range(base, base + frames + 1), f"地图门/前层动画（v{version}）")
    elif version in (2, 3):
        # v2/v3 的后层/中层/前层库号是 Mir3 编号（BackIndex=+100、Middle=+110、Front=+120），
        # 本项目 `LIBRARY_FILES` 没有这些库，只统计「有多少格有前层图」。
        front = word(4, 2) & 0x7FFF
        stats["v2v3_front_tiles"] += int((front > 0).sum())


def scan_map_pure(raw: bytes, version: int, book: RefBook, stats: dict) -> None:
    """纯标准库回退路径（慢，但无 numpy 也能跑）。"""
    if version == 1:
        width = (raw[21] | raw[22] << 8) ^ (raw[23] | raw[24] << 8)
        height = (raw[25] | raw[26] << 8) ^ (raw[23] | raw[24] << 8)
        xor = raw[23] | raw[24] << 8
        start, stride, fmt = 54, 15, "<IhhBBBBBBB"
    elif version == 0:
        width, height = struct.unpack_from("<HH", raw, 0)
        start, stride, fmt = 52, 12, "<HHHBBBBBB"
        xor = 0
    elif version == 4:
        width_word, xor, height_word = struct.unpack_from("<HHH", raw, 31)
        width, height = width_word ^ xor, height_word ^ xor
        start, stride, fmt = 64, 12, "<HHHBBBBBB"
    elif version in (2, 3):
        width, height = struct.unpack_from("<HH", raw, 0)
        stats["v2v3_front_tiles"] += 1
        return
    else:
        stats["unsupported_versions"][version] = stats["unsupported_versions"].get(version, 0) + 1
        return
    total = width * height
    if width <= 0 or height <= 0 or len(raw) < start + total * stride:
        stats["bad_dimensions"] += 1
        return
    stats["cells"] += total
    stats["width_height"][f"{width}x{height}"] = stats["width_height"].get(f"{width}x{height}", 0) + 1
    back_set: set[int] = set()
    middle_set: set[int] = set()
    front_sets: dict[int, set[int]] = defaultdict(set)
    animated: list[tuple[int, int, int]] = []
    for fields in struct.iter_unpack(fmt, raw[start:start + total * stride]):
        if version == 1:
            back = (fields[0] ^ 0xAA38AA38) & 0x7FFF
            middle = ((fields[1] & 0xFFFF) ^ xor) & 0x7FFF
            front = ((fields[2] & 0xFFFF) ^ xor) & 0x7FFF
            front_index = fields[7] + 2
            if front_index == 102:
                front_index = 90
            if front_index >= 255:
                front_index = 0
            animation = fields[5] & 0x7F
        elif version == 0:
            back = fields[0] & 0x7FFF
            middle = fields[1] & 0x7FFF
            front = fields[2] & 0x7FFF
            front_index = fields[7] + 2
            animation = fields[5] & 0x7F
        else:  # version 4
            back = (fields[0] ^ xor) & 0x7FFF
            middle = (fields[1] ^ xor) & 0x7FFF
            front = (fields[2] ^ xor) & 0x7FFF
            front_index = fields[7] + 2
            animation = fields[5] & 0x7F
        if back:
            back_set.add(back - 1)
        if middle:
            middle_set.add(middle - 1)
        if front:
            library = LIBRARY_FILES.get(front_index)
            if library is None:
                stats["unsupported_front_libs"][front_index] = (
                    stats["unsupported_front_libs"].get(front_index, 0) + 1
                )
            else:
                front_sets[front_index].add(front - 1)
                if animation:
                    animated.append((front_index, front - 1, animation))
    book.add_many("tiles.wzl", back_set, f"地图后层瓦片（v{version}）")
    book.add_many("smtiles.wzl", middle_set, f"地图中层瓦片（v{version}）")
    for library, indices in front_sets.items():
        book.add_many(LIBRARY_FILES[library], indices, f"地图前层瓦片（v{version}）")
    for library, base, frames in animated:
        book.add_many(LIBRARY_FILES[library], range(base, base + frames + 1), f"地图门/前层动画（v{version}）")


def scan_maps(map_dir: Path, names: list[str], book: RefBook, label: str) -> dict:
    """扫描一组地图，把瓦片引用写进 `book`，返回统计。"""
    np = load_numpy()
    stats = {
        "label": label,
        "map_dir": rel(map_dir),
        "maps": 0,
        "missing": [],
        "versions": Counter(),
        "cells": 0,
        "width_height": Counter(),
        "unsupported_versions": Counter(),
        "unsupported_front_libs": Counter(),
        "v2v3_front_tiles": 0,
        "bad_dimensions": 0,
        "truncated": 0,
        "bytes": 0,
        "files": [],
    }
    started = time.time()
    for order, name in enumerate(names, 1):
        path = map_dir / f"{name}.map"
        if not path.is_file():
            stats["missing"].append(name)
            continue
        raw = path.read_bytes()
        version = find_type(raw)
        stats["maps"] += 1
        stats["bytes"] += len(raw)
        stats["versions"][version] += 1
        stats["files"].append({"file": name, "version": version, "bytes": len(raw)})
        if np is not None:
            scan_map_with_numpy(np, raw, version, book, stats)
        else:
            scan_map_pure(raw, version, book, stats)
        if order % 100 == 0:
            print(f"  [{label}] {order}/{len(names)} 张地图（{time.time() - started:.1f}s）", flush=True)
    stats["elapsed_s"] = round(time.time() - started, 2)
    for key in ("versions", "width_height", "unsupported_versions", "unsupported_front_libs"):
        stats[key] = dict(sorted(stats[key].items()))
    stats["width_height"] = dict(
        sorted(stats["width_height"].items(), key=lambda item: -item[1])[:10]
    )
    stats["missing_count"] = len(stats["missing"])
    stats["files"] = stats["files"]
    print(
        f"  [{label}] {stats['maps']} 张地图 / {stats['cells']} 格，版本 {stats['versions']}，"
        f"缺 {stats['missing_count']} 张，{stats['elapsed_s']}s",
        flush=True,
    )
    return stats


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(REPO_ROOT).as_posix()
    except ValueError:
        return str(path)


def ensure_db_json(dataset: Path) -> dict:
    """确保有 DB JSON；没有就用 rustc 编译探针（只读 MirDB 的公开字段）。"""
    if DB_JSON.is_file() and DB_JSON.stat().st_mtime >= (dataset / "Server.MirDB").stat().st_mtime:
        return json.loads(DB_JSON.read_text(encoding="utf-8"))
    PROJECT = REPO_ROOT / "target" / "asset-probe"
    PROJECT.mkdir(parents=True, exist_ok=True)
    deps = REPO_ROOT / "target" / "debug" / "deps"
    rlibs = sorted(deps.glob("libmir_db-*.rlib"), key=lambda path: path.stat().st_mtime, reverse=True)
    if not rlibs:
        raise SystemExit(
            "[错误] 找不到 target/debug/deps/libmir_db-*.rlib。先跑 `cargo build -p mir-db`，"
            "或手工生成 target/asset-probe/db.json。"
        )
    exe = PROJECT / "db_probe.exe"
    rustc = shutil.which("rustc")
    if rustc is None:
        raise SystemExit("[错误] PATH 里没有 rustc，无法生成 DB JSON。")
    probe_source = PROJECT / "db_probe.rs"
    probe_source.write_text(DB_PROBE_SOURCE, encoding="utf-8")
    command = [
        rustc,
        "--edition",
        "2024",
        "-O",
        str(probe_source),
        "-o",
        str(exe),
        "-L",
        f"dependency={deps}",
        "--extern",
        f"mir_db={rlibs[0]}",
    ]
    print("  [db] 编译探针：" + " ".join(command[:4]) + " …", flush=True)
    subprocess.run(command, check=True)
    subprocess.run([str(exe), str(dataset / "Server.MirDB"), str(DB_JSON)], check=True)
    return json.loads(DB_JSON.read_text(encoding="utf-8"))


def player_library(armour: int) -> tuple[int, int]:
    remaining = armour
    for index, count in enumerate(HUM_BLOCKS):
        if remaining < count:
            return index + 1, remaining
        remaining -= count
    return len(HUM_BLOCKS), HUM_BLOCKS[-1] - 1


def hair_library(hair: int) -> tuple[int, int]:
    remaining = hair
    for index, count in enumerate(HAIR_BLOCKS):
        if remaining < count:
            return index + 1, remaining
        remaining -= count
    return len(HAIR_BLOCKS), HAIR_BLOCKS[-1] - 1


def weapon_library(shape: int) -> tuple[int, int]:
    series = min(shape // 100, 2)
    return series + 1, shape % 100


def library_filename(stem: str, number: int) -> str:
    return f"{stem}.wzl" if number == 1 else f"{stem}{number}.wzl"


def collect_db_refs(db: dict, book: RefBook, include_reserved: bool, notes: list[str]) -> dict:
    """物品/怪物/NPC/玩家外观/小地图/特效引用。"""
    stats = {
        "monster_templates": 0,
        "monster_images": 0,
        "monster_spawned_images": 0,
        "monster_unmapped": [],
        "npc_templates": 0,
        "npc_images": 0,
        "item_icons": 0,
        "item_templates": 0,
        "armour_shapes": 0,
        "weapon_shapes": 0,
        "hair_shapes": 0,
        "wing_blocks": [],
        "mmap_frames": 0,
    }
    # ---- 怪物：Image → mon{Image/10+1}.wzl，槽位 = Image % 10 ----
    images = {int(monster["image"]) for monster in db["monsters"]}
    spawned_images = {int(monster["image"]) for monster in db["monsters"] if monster["spawned"]}
    stats["monster_templates"] = len(db["monsters"])
    stats["monster_images"] = len(images)
    stats["monster_spawned_images"] = len(spawned_images)
    for image in sorted(images):
        library = image // 10 + 1
        if image >= 540:  # Crystal 特殊段：900.. 攻城/城门、10000.. 宠物，扁平语料没有对应库
            stats["monster_unmapped"].append(
                {"image": image, "reason": "特殊段（>=540）：Crystal 用 Siege/Gates/Pets 库，本项目没有"}
            )
            continue
        lib = f"mon{library}.wzl"
        slot = image % 10
        frames = [slot * MONSTER_FRAMES_PER_IMAGE + frame for frame in MONSTER_FRAMES]
        spawned = "已刷出" if image in spawned_images else "仅模板"
        entity = f"mon-{image:04d}"
        book.add_group(
            lib, frames, f"怪物图像 image={image}（槽 {slot}，{spawned}）",
            entity_dest(lib, entity), entity, f"image={image}",
        )
    if stats["monster_unmapped"]:
        notes.append(
            f"有 {len(stats['monster_unmapped'])} 个怪物 Image 落在 Crystal 特殊段（>=540），"
            "本项目渲染层映射到不存在的 mon 库，未纳入拆包"
        )
    # ---- NPC：Image → npc{Image/100+1}.wzl，槽位 = Image % 100 ----
    npc_images = {int(npc["image"]) for npc in db["npcs"]}
    stats["npc_templates"] = len(db["npcs"])
    stats["npc_images"] = len(npc_images)
    for image in sorted(npc_images):
        library = min(max(image // 100 + 1, 1), 4)
        lib = library_filename("npc", library)
        slot = image % 100
        frames = [slot * NPC_FRAMES_PER_IMAGE + frame for frame in NPC_FRAMES]
        entity = f"npc-{image:04d}"
        book.add_group(
            lib, frames, f"NPC 图像 image={image}（槽 {slot}）",
            entity_dest(lib, entity), entity, f"image={image}",
        )
    # ---- 物品图标：items.wzl，Image 即 0 基下标 ----
    item_images = {int(item["image"]) for item in db["items"]}
    stats["item_icons"] = len(item_images)
    stats["item_templates"] = len(db["items"])
    for image in sorted(item_images):
        entity = f"icon-{image:04d}"
        book.add_group(
            "items.wzl", [image], f"物品模板图标 image={image}",
            entity_dest("items.wzl", entity), entity, f"image={image}",
        )
    # ---- 玩家外观：衣服/武器/头发/翅膀 ----
    armour_shapes = {0} | {int(item["shape"]) for item in db["items"] if item["type"] == 2}
    weapon_shapes = {int(item["shape"]) for item in db["items"] if item["type"] == 1}
    armour_effects = {int(item["effect"]) for item in db["items"] if item["type"] == 2}
    stats["armour_shapes"] = len(armour_shapes)
    stats["weapon_shapes"] = len(weapon_shapes)
    for shape in sorted(armour_shapes):
        library, block = player_library(shape)
        lib = library_filename("hum", library)
        frames = [block * PLAYER_FRAMES_PER_APPEARANCE + frame for frame in HUMAN_FRAMES]
        entity = f"body-{shape:03d}"
        book.add_group(
            lib, frames, f"衣服外观 shape={shape}（块 {block}）",
            entity_dest(lib, entity), entity, f"shape={shape}",
        )
    for shape in sorted(weapon_shapes):
        library, block = weapon_library(shape)
        lib = library_filename("weapon", library)
        frames = [block * PLAYER_FRAMES_PER_APPEARANCE + frame for frame in HUMAN_FRAMES]
        entity = f"weapon-{shape:03d}"
        book.add_group(
            lib, frames, f"武器外观 shape={shape}（块 {block}）",
            entity_dest(lib, entity), entity, f"shape={shape}",
        )
    # 发型号：服务端创建角色时 `hair = character.id % 9`（session.rs:899），0..8。
    hairs = set(range(9))
    stats["hair_shapes"] = len(hairs)
    for hair in sorted(hairs):
        library, block = hair_library(hair)
        lib = library_filename("hair", library)
        frames = [block * PLAYER_FRAMES_PER_APPEARANCE + frame for frame in HUMAN_FRAMES]
        entity = f"hair-{hair:02d}"
        book.add_group(
            lib, frames, f"发型号 {hair}（块 {block}）",
            entity_dest(lib, entity), entity, f"hair={hair}",
        )
    # 翅膀：盔甲 Effect → wing_effect，客户端 wing_layer = effect，外观块 = (effect - 1) % 2。
    # 语料只有 2 个翅膀块，所以按「块」落盘（effects 1 / 2 是规范号），其余 Effect 记别名：
    # 本数据集里 Effect=100（破凰天魔衣）与 Effect=2（天龙不死衣）共用块 1，不重复拆一份 PNG。
    wings = sorted(effect for effect in armour_effects if effect > 0)
    stats["wing_blocks"] = wings
    wings_by_block: dict[int, list[int]] = {}
    for wing in wings:
        wings_by_block.setdefault((wing - 1) % WING_BLOCKS, []).append(wing)
    for block, effects in sorted(wings_by_block.items()):
        frames = [block * WING_FRAMES_PER_APPEARANCE + frame for frame in WING_FRAMES]
        canonical = block + 1
        entity = f"wings-{canonical:02d}"
        book.add_group(
            "wingeffect_x.wzl",
            frames,
            f"翅膀块 {block}（Effect={', '.join(str(effect) for effect in effects)}）",
            entity_dest("wingeffect_x.wzl", entity),
            entity,
            aliases=[effect for effect in effects if effect != canonical],
        )
    # ---- 小地图 / 大地图：mmap.wzl ----
    mmap = set()
    for map_info in db["maps"]:
        for key in ("mini_map", "big_map"):
            value = int(map_info[key])
            if value:
                mmap.add(value)
    stats["mmap_frames"] = len(mmap)
    book.add_many("mmap.wzl", sorted(mmap), "MapInfo.MiniMap/BigMap 帧")
    # ---- 固定特效帧段 ----
    book.add_many("effect.wzl", EFFECT_FRAMES, "挥砍光效（FxKind::Swing）")
    book.add_many("magic.wzl", MAGIC_FRAMES, "技能命中闪光（FxKind::Magic）")
    book.add_many("magic2.wzl", MAGIC2_FRAMES, "挥砍 + 雷电束（FxKind::Swing/Bolt）")
    # ---- 坐骑：渲染层恒定必需（`PlayerLayer::Mount`，stem `horse_x`），与 DB 是否发骑乘无关 ----
    # 渲染层按「块号 × 640 + 动作表偏移」取帧（`entities.rs::mount_frames`），所以按块落盘，
    # 每块目录里是块内绝对索引；块号 `mount-1..mount-3` 与 `library-map.json` 的 dirs 顺序一致。
    for block in range(HORSE_X_BLOCKS):
        frames = [block * MOUNT_FRAMES_PER_APPEARANCE + frame for frame in MOUNT_FRAMES]
        entity = f"mount-{block + 1}"
        book.add_group(
            "horse_x.wzl",
            frames,
            f"坐骑外观块 {block}（渲染层必需：PlayerLayer::Mount）",
            entity_dest("horse_x.wzl", entity),
            entity,
        )
    stats["mount_blocks"] = HORSE_X_BLOCKS
    notes.append("坐骑 horse_x.wzl 已按渲染层必需库收录（3 个外观块，块号 mount-1..3）")
    return stats


def collect_audio(db: dict, notes: list[str]) -> dict:
    """音频引用：事件音 + 施法音 + 背景音乐。"""
    fixed = {
        "1.wav": "走路（SoundId::Walk）",
        "3.wav": "跑步（SoundId::Run）",
        "50.wav": "挥空（SoundId::Swing）",
        "70.wav": "命中（SoundId::Hit）",
        "138.wav": "男受击（SoundId::Struck）",
        "139.wav": "女受击（sound_for_sex）",
        "144.wav": "男死亡（SoundId::Die）",
        "145.wav": "女死亡（sound_for_sex）",
        "levelup.wav": "升级（SoundId::LevelUp）",
        "pet_pickup.wav": "拾取（SoundId::Pickup）",
        "106.wav": "金币（SoundId::Gold）",
        "108.wav": "喝药（SoundId::Drink）",
        "103.wav": "NPC 对话（SoundId::NpcDialog）",
        "100.wav": "登录音（LOGIN_SOUND_FILE）",
        "Select2.wav": "切图音（MAP_CHANGE_SOUND_FILE）",
        "30001.wav": "背景音乐：比奇/默认（music_for_map）",
        "30002.wav": "背景音乐：盟重（music_for_map）",
    }
    spells = sorted({int(magic["spell"]) for magic in db["magics"] if int(magic["spell"]) > 0})
    spell_files = {f"M{spell}-0.wav": f"施法音（Spell={spell}）" for spell in spells}
    wanted = dict(fixed)
    wanted.update(spell_files)
    files = []
    missing = []
    for name, reason in sorted(wanted.items()):
        found = None
        for root in AUDIO_ROOTS:
            candidate = root / name
            if candidate.is_file():
                found = candidate
                break
        if found is None:
            missing.append(name)
            continue
        files.append(
            {
                "file": name,
                "reason": reason,
                "source": rel(found),
                "bytes": found.stat().st_size,
            }
        )
    # 背景音乐实际用哪几首：这份数据的 MapInfo.Music 全是 0，客户端按地图名兜底。
    used_music = {entry["file"] for entry in files if entry["file"].startswith(("300", "310"))}
    notes.append(
        "音频解析顺序：resource/wav → mirfiles-crystal-client/Client/Sound → gqf2008-crystal/Sound；"
        f"本数据集 MapInfo.Music 全为 0，背景音乐按客户端兜底取 {sorted(used_music)}"
    )
    if missing:
        notes.append(f"有 {len(missing)} 个引用音频在三处音效目录都找不到：{missing[:8]}")
    return {"files": files, "missing": missing, "spell_sounds": spells}


class WzlIndex:
    """.wzx 索引 + 按需读取 .wzl 记录头（不解码像素）。

    语料的空槽统一指向 `.wzl` 偏移 48 处的一条全零 16 字节记录（`nonempty` 会把它排除）。
    """

    def __init__(self, wzl: Path, wzx: Path) -> None:
        self.wzl = wzl
        self.size = wzl.stat().st_size
        table = wzx.read_bytes()[48:]
        count = len(table) // 4
        self.offsets = struct.unpack_from(f"<{count}I", table, 0) if count else ()
        self._handle = None
        self.placeholder = self._detect_placeholder()

    def _read_header(self, offset: int) -> dict | None:
        if offset <= 0:
            return None
        if self._handle is None:
            self._handle = self.wzl.open("rb")
        self._handle.seek(offset)
        header = self._handle.read(16)
        if len(header) < 16:
            return None
        pixel_format, width, height, offset_x, offset_y, length = struct.unpack_from(
            "<B3xHHhhI", header, 0
        )
        if width == 0 or height == 0 or length == 0:
            return None
        return {
            "format": pixel_format,
            "width": width,
            "height": height,
            "offset_x": offset_x,
            "offset_y": offset_y,
            "length": length,
            "data_offset": offset + 16,
        }

    def _detect_placeholder(self) -> int | None:
        """偏移 48 是不是共享的空记录占位符。"""
        if len(self.offsets) > 0 and 48 in self.offsets and self._read_header(48) is None:
            return 48
        return None

    def record(self, index: int) -> dict | None:
        if index < 0 or index >= len(self.offsets):
            return None
        offset = self.offsets[index]
        if offset == 0 or offset == self.placeholder:
            return None
        return self._read_header(offset)

    def count_nonempty(self) -> int:
        return sum(
            1 for offset in self.offsets if offset and offset != self.placeholder
        )

    def close(self) -> None:
        if self._handle is not None:
            self._handle.close()
            self._handle = None


def data_files_by_lower_name() -> dict[str, Path]:
    return {path.name.lower(): path for path in DATA_DIR.iterdir() if path.is_file()}


def compute_library_stats(book: RefBook, include_reserved: bool) -> dict:
    """对 `resource/data` 里的每个 `.wzl` 算 used/unused 与体积。"""
    files = data_files_by_lower_name()
    wzl_names = sorted(name for name in files if name.endswith(".wzl"))
    libraries = []
    referenced = set(book.libs)
    total_entries = 0
    total_nonempty = 0
    total_file_bytes = 0
    used_compressed = 0
    used_pixels = 0
    used_frames = 0
    used_wzl_files = 0
    missing_libs = []
    started = time.time()
    for name in wzl_names:
        path = files[name]
        index_path = files.get(name[:-4] + ".wzx")
        total_file_bytes += path.stat().st_size
        if index_path is None:
            # 没有 .wzx 的 .wzl 打不开，按未使用统计。
            libraries.append(
                {
                    "lib": path.name,
                    "category": human_category(name) or "unused",
                    "entries": 0,
                    "nonempty": 0,
                    "used": 0,
                    "missing_indices": [],
                    "file_bytes": path.stat().st_size,
                    "error": "缺 .wzx 索引",
                }
            )
            continue
        wzl = WzlIndex(path, index_path)
        entries = len(wzl.offsets)
        nonempty = wzl.count_nonempty()
        total_entries += entries
        total_nonempty += nonempty
        record = book.libs.get(name)
        used = sorted(record["indices"]) if record else []
        used_ok = []
        missing = []
        out_of_range = []
        pixels = 0
        compressed = 0
        for index in used:
            if index >= entries:
                out_of_range.append(index)
                continue
            info = wzl.record(index)
            if info is None:
                missing.append(index)
                continue
            used_ok.append(index)
            pixels += info["width"] * info["height"]
            compressed += info["length"]
        wzl.close()
        category = human_category(name) or ("unused" if not record else "unused")
        if record is not None and human_category(name) is None:
            category = "unused"
        used_pixels += pixels
        used_compressed += compressed
        used_frames += len(used_ok)
        if record is not None:
            used_wzl_files += 1
        libraries.append(
            {
                "lib": path.name,
                "category": category,
                "entries": entries,
                "nonempty": nonempty,
                "used": len(used_ok),
                "used_declared": len(used),
                "missing_indices": missing[:200],
                "missing_count": len(missing),
                "out_of_range_indices": out_of_range[:100],
                "out_of_range_count": len(out_of_range),
                "unused_nonempty": max(0, nonempty - len(used_ok)),
                "file_bytes": path.stat().st_size,
                "wzx_bytes": index_path.stat().st_size,
                "used_pixels": pixels,
                "raw_rgba_bytes": pixels * 4,
                "used_compressed_bytes": compressed,
                "unused_wzl_bytes": max(0, path.stat().st_size - compressed),
            }
        )
    for lib in sorted(referenced - {entry["lib"].lower() for entry in libraries}):
        missing_libs.append(lib)
    frames_total = sum(entry["used"] for entry in libraries)
    print(
        f"  [wzl] {len(libraries)} 个库 / {total_entries} 项（非空 {total_nonempty}），"
        f"用到 {used_wzl_files} 个库、{frames_total} 帧、{used_pixels / 1e6:.1f}MP，"
        f"{time.time() - started:.1f}s",
        flush=True,
    )
    return {
        "libraries": libraries,
        "totals": {
            "libraries": len(libraries),
            "entries": total_entries,
            "nonempty": total_nonempty,
            "used_libraries": used_wzl_files,
            "used_frames": frames_total,
            "used_pixels": used_pixels,
            "used_raw_rgba_bytes": used_pixels * 4,
            "used_compressed_bytes": used_compressed,
            "wzl_total_bytes": total_file_bytes,
            "unused_wzl_bytes_approx": max(0, total_file_bytes - used_compressed),
        },
        "missing_libraries": missing_libs,
    }


def asset_sizes() -> dict:
    """maps / audio / fonts / server / ld 的 used vs unused 统计。"""
    maps = []
    maps_dir = MAP_DIR_CJLAAA
    for path in sorted(maps_dir.glob("*.map")):
        maps.append({"file": path.stem, "bytes": path.stat().st_size})
    fonts = [
        {"file": path.name, "bytes": path.stat().st_size}
        for path in sorted(FONTS_DIR.glob("*"))
        if path.is_file()
    ]
    ld_dir = REPO_ROOT / "resource" / "ld"
    ld = [
        {"file": path.name, "bytes": path.stat().st_size}
        for path in sorted(ld_dir.iterdir())
        if path.is_file()
    ] if ld_dir.is_dir() else []
    envir = DATASET_DIR / "Envir"
    server_files = [{"file": "Server.MirDB", "bytes": (DATASET_DIR / "Server.MirDB").stat().st_size}]
    if envir.is_dir():
        for path in sorted(envir.rglob("*")):
            if path.is_file():
                server_files.append({"file": "Envir/" + path.relative_to(envir).as_posix(), "bytes": path.stat().st_size})
    return {"maps": maps, "fonts": fonts, "ld": ld, "server": server_files}


def build_used_assets(book: RefBook, maps_used: list[str], audio: dict, stats_libs: dict, args) -> dict:
    """组装 `target/used-assets.json`（hubu 拆包器吃的 v1 格式）。"""
    entries = {entry["lib"].lower(): entry for entry in stats_libs["libraries"]}
    disk_files = data_files_by_lower_name()
    libraries = []
    skipped_entities: list[tuple[str, int, int]] = []
    for lib, record in sorted(book.libs.items()):
        stats = entries.get(lib.lower(), {})
        actual = disk_files.get(lib.lower())
        display_lib = actual.name if actual is not None else lib
        limit = int(stats.get("entries", 0))
        declared = sorted(record["indices"])
        # 越界（>= 库长）的图号是地图里的哨兵/空洞（例如 0x7FFF），拆不出来，不进计划。
        indices = [index for index in declared if limit == 0 or index < limit]
        out_of_range_dropped = len(declared) - len(indices)
        reason_counts = record["reasons"]
        top_reasons = "；".join(
            f"{reason} ×{count}" if count > 1 else reason
            for reason, count in reason_counts.most_common(3)
        )
        evidence = [name for name, _ in record["evidence"].most_common(6)]
        category = human_category(lib) or "unused"
        missing_count = int(stats.get("missing_count", 0))
        missing_sample = list(stats.get("missing_indices") or [])[:20]
        out_of_range_count = int(stats.get("out_of_range_count", 0))
        groups = record.get("groups") or {}
        dropped_outputs = []
        if groups:
            outputs = []
            for group_dest, group in sorted(groups.items()):
                group_indices = sorted(index for index in group["indices"] if limit == 0 or index < limit)
                if not group_indices:
                    dropped_outputs.append((group.get("entity"), len(group["indices"])))
                    continue
                outputs.append(
                    {
                        "dest": group_dest,
                        "entity": group.get("entity"),
                        "indices": group_indices,
                        "requested": len(group_indices),
                        "aliases": sorted(group.get("aliases") or []),
                        "reason": group.get("reason", top_reasons),
                    }
                )
            if not outputs:
                skipped_entities.append((display_lib, len(groups), len(indices)))
                continue
            dest = outputs[0]["dest"]
        else:
            if not indices:
                skipped_entities.append((display_lib, 0, 0))
                continue
            outputs = [
                {
                    "dest": default_dest(lib),
                    "indices": indices,
                    "requested": len(indices),
                    "reason": top_reasons,
                }
            ]
            dest = outputs[0]["dest"]
        if actual is None:
            skipped_entities.append((display_lib, -1, len(indices)))
            continue
        wzx_actual = disk_files.get(actual.with_suffix(".wzx").name.lower())
        loaded_frames = 0
        if wzx_actual is not None:
            wzl = WzlIndex(actual, wzx_actual)
            for output in outputs:
                output_loaded = sum(
                    1 for index in output["indices"] if wzl.record(index) is not None
                )
                output["loaded_frames"] = output_loaded
                output["missing_count"] = len(output["indices"]) - output_loaded
                loaded_frames += output_loaded
            wzl.close()
        entry = {
            "lib": display_lib,
            "indices": indices,
            "category": category,
            "dest": dest,
            "outputs": outputs,
            "dropped_outputs": [
                {"entity": entity, "declared_frames": count} for entity, count in dropped_outputs
            ],
            "reason": top_reasons,
            "used_frames": len(indices),
            "loaded_frames": loaded_frames,
            "dropped_out_of_range": out_of_range_dropped,
            "entries": stats.get("entries", 0),
            "nonempty": stats.get("nonempty", 0),
            "source": rel(actual) if actual is not None else rel(DATA_DIR / lib),
            "source_bytes": stats.get("file_bytes", 0),
            "used_pixels": stats.get("used_pixels", 0),
            "used_compressed_bytes": stats.get("used_compressed_bytes", 0),
            "estimated_png_bytes": int(stats.get("used_pixels", 0) * 1.5),
            "missing_indices": missing_sample,
            "missing_count": missing_count,
            "out_of_range_indices": list(stats.get("out_of_range_indices") or [])[:20],
            "out_of_range_count": out_of_range_count,
        }
        if evidence:
            entry["evidence"] = evidence
        libraries.append(entry)
    library_bytes = sum(entry["estimated_png_bytes"] for entry in libraries)
    missing_libraries = stats_libs.get("missing_libraries") or []
    if missing_libraries:
        notes_extra = [
            f"引用了但 resource/data 里没有文件的图库（{len(missing_libraries)}）："
            + "、".join(missing_libraries[:10])
        ]
    else:
        notes_extra = []
    return {
        "version": 1,
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "source_root": "resource/data",
        "dataset": args.dataset,
        "map_source": args.map_source,
        "libraries": libraries,
        "maps": maps_used,
        "audio": [entry["file"] for entry in audio["files"]],
        "fonts": [entry["file"] for entry in asset_sizes()["fonts"]],
        "notes": [
            "indices 是库内 0 基下标（与 mir_asset::ImageLibrary::image(i) 一致）；全部为显式帧级集合，"
            "没有整库保留项。",
            "帧级裁剪依据 crates/mir-render/src/animation.rs 的动作帧表；空帧（.wzx 偏移 0 / 宽高 0）"
            "由拆包器跳过并计入 missing。",
            "estimated_png_bytes = used_pixels × 1.5（经验中值，实际 PNG 约 1~3 字节/像素；"
            "拆包后以落地文件为准）。",
            *notes_extra,
            *(
                [
                    f"有 {len(skipped_entities)} 个图库的实体在库内完全没有可落地帧（已跳过）："
                    + "、".join(lib for lib, _, _ in skipped_entities[:12])
                ]
                if skipped_entities
                else []
            ),
        ],
        "stats": {
            "libraries": len(libraries),
            "skipped_entities": [lib for lib, _, _ in skipped_entities],
            "missing_libraries": missing_libraries,
            "used_frames": sum(entry["used_frames"] for entry in libraries),
            "used_pixels": sum(entry["used_pixels"] for entry in libraries),
            "used_raw_rgba_bytes": sum(entry["used_pixels"] * 4 for entry in libraries),
            "estimated_png_bytes": library_bytes,
            "maps": len(maps_used),
            "audio_files": len(audio["files"]),
        },
    }


def write_report(path: Path, used: dict, stats_libs: dict, map_stats: dict, compare: dict | None, audio: dict, sizes: dict, elapsed: float) -> None:
    entries = sorted(
        (entry for entry in stats_libs["libraries"] if entry["used"] > 0),
        key=lambda entry: -entry.get("estimated_png_bytes", 0),
    )
    lines = [
        "# 素材使用统计（自动生成）",
        "",
        f"- 生成时间：{used['generated_at']}",
        f"- 生成脚本：`tools/assets/analyze_usage.py`（耗时 {elapsed:.1f}s）",
        f"- 数据集：`{used['dataset']}`；地图来源：`{used['map_source']}`",
        f"- 结论：保留 **{used['stats']['libraries']}** 个图库 / **{used['stats']['used_frames']}** 帧"
        f"（{used['stats']['used_pixels']/1e6:.1f} MP，PNG 估计 {used['stats']['estimated_png_bytes']/1e6:.0f} MB）",
        "",
        "## 1) 地图",
        "",
        f"- 使用地图：**{map_stats['maps']}** 张（{map_stats['bytes']/1e6:.1f} MB），"
        f"版本分布 `{map_stats['versions']}`，格数 {map_stats['cells']:,}",
        f"- 数据集合计 {len(sizes['maps'])} 张 / {sum(item['bytes'] for item in sizes['maps'])/1e6:.1f} MB；"
        f"未引用 **{len(sizes['maps']) - map_stats['maps']}** 张"
        f"（{(sum(item['bytes'] for item in sizes['maps']) - map_stats['bytes'])/1e6:.1f} MB）",
    ]
    if map_stats["missing_count"]:
        lines.append(f"- 缺文件 {map_stats['missing_count']} 张（例如 `{map_stats['missing'][:5]}`）")
    if map_stats["unsupported_versions"]:
        lines.append(f"- 不支持的版本：`{map_stats['unsupported_versions']}`（不产生瓦片引用）")
    if map_stats["unsupported_front_libs"]:
        lines.append(
            f"- 前层库号不在 `LIBRARY_FILES` 的格数：{sum(map_stats['unsupported_front_libs'].values()):,}"
            f"（库号 `{sorted(map_stats['unsupported_front_libs'])[:12]}` …）"
        )
    if compare:
        lines += [
            "",
            "### 与 `resource/map`（v0）对比",
            "",
            f"- `resource/map` 可用 {compare['stats']['maps']}/{compare['requested']} 张；"
            f"版本 `{compare['stats']['versions']}`",
            f"- 两边瓦片引用集合：`tiles.wzl` 交集 {compare['overlap'].get('tiles.wzl', 0)}、"
            f"并集 {compare['union'].get('tiles.wzl', 0)}；`smtiles.wzl` 交集 {compare['overlap'].get('smtiles.wzl', 0)}",
            "- 结论：默认数据集（cjlaaa v1）与 v0 地图瓦片集合高度重合；瓦片最终以 cjlaaa 的 v1 引用为准。",
        ]
    lines += [
        "",
        "## 2) 保留的图库（按 PNG 估计体积降序）",
        "",
        "| 图库 | 分类 | 保留帧 | 落地帧 | 库内非空 | 压缩字节 | PNG 估计 | 空帧 | 越界 |",
        "|---|---|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for entry in entries:
        png_bytes = entry.get("estimated_png_bytes", entry["used_pixels"] * 1.5)
        declared = entry["used"] + entry["missing_count"] + entry["out_of_range_count"]
        lines.append(
            f"| `{entry['lib']}` | {entry['category']} | {declared:,} | "
            f"{entry['used']:,} | "
            f"{entry['nonempty']:,} | {entry['used_compressed_bytes']/1e6:.1f} MB | "
            f"{png_bytes/1e6:.1f} MB | {entry['missing_count']:,} | {entry['out_of_range_count']:,} |"
        )
    total = stats_libs["totals"]
    lines += [
        "",
        f"- 图库总量：{total['libraries']} 个 / {total['entries']:,} 项（非空 {total['nonempty']:,}）",
        f"- 未使用 `.wzl` 体积（近似）：**{total['unused_wzl_bytes_approx']/1e9:.2f} GB**"
        f"（占总 {total['wzl_total_bytes']/1e9:.2f} GB 的 "
        f"{total['unused_wzl_bytes_approx']/max(1, total['wzl_total_bytes'])*100:.1f}%）",
        "",
        "## 3) 音频 / 字体 / 服务端",
        "",
        f"- 音频：{len(audio['files'])} 个文件，"
        f"{sum(item['bytes'] for item in audio['files'])/1e6:.1f} MB；缺失 {len(audio['missing'])}",
        f"- 字体：{sum(item['bytes'] for item in sizes['fonts'])/1e6:.1f} MB（全部保留）",
        f"- 服务端：{len(sizes['server'])} 个文件 / {sum(item['bytes'] for item in sizes['server'])/1e6:.2f} MB（全部保留）",
        f"- `resource/ld`：{len(sizes['ld'])} 个文件——**无代码引用，未纳入**",
        "",
        "## 4) 未使用的图库族（体积最大的 20 个族）",
        "",
        "| 族 | 库数 | 未用字节 |",
        "|---|---:|---:|",
    ]
    families: dict[str, dict] = {}
    for entry in stats_libs["libraries"]:
        if entry["used"] > 0:
            continue
        family = re.sub(r"\d+", "#", entry["lib"].lower())
        bucket = families.setdefault(family, {"count": 0, "bytes": 0})
        bucket["count"] += 1
        bucket["bytes"] += entry["file_bytes"]
    for family, bucket in sorted(families.items(), key=lambda item: -item[1]["bytes"])[:20]:
        lines.append(f"| `{family}` | {bucket['count']} | {bucket['bytes']/1e6:.1f} MB |")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    global DB_JSON
    parser = argparse.ArgumentParser(description="素材引用分析：生成 used-assets.json 与统计报告")
    parser.add_argument("--dataset", default="cjlaaa", help="数据集名（默认 cjlaaa）")
    parser.add_argument("--db-json", default=str(DB_JSON), help="DB 探针 JSON（默认 target/asset-probe/db.json）")
    parser.add_argument(
        "--map-source",
        choices=("cjlaaa", "resource"),
        default="cjlaaa",
        help="地图来源（默认 cjlaaa：Server.MirDB 实际引用的 389 张 v1 地图）",
    )
    parser.add_argument("--out", default="target/used-assets.json", help="拆包计划 JSON")
    parser.add_argument("--stats", default="target/asset-usage-stats.json", help="详细统计 JSON")
    parser.add_argument("--report", default="target/asset-usage-report.md", help="Markdown 报告")
    parser.add_argument("--compare-maps", action="store_true", help="额外分析 resource/map（v0）做对比")
    parser.add_argument("--include-reserved", action="store_true", help="收坐骑等预留图库（默认不收）")
    parser.add_argument("--skip-audio", action="store_true", help="跳过音频解析（调试用）")
    args = parser.parse_args(argv)

    started = time.time()
    DB_JSON = Path(args.db_json)
    print("== 素材引用分析 ==", flush=True)
    if not DATA_DIR.is_dir():
        print(f"[错误] 缺素材目录 {DATA_DIR}", file=sys.stderr)
        return 2
    db = ensure_db_json(DATASET_DIR)
    notes: list[str] = []
    book = RefBook()

    # ---- 地图瓦片 ----
    map_names = [item["file"] for item in db["maps"]]
    map_dir = MAP_DIR_CJLAAA if args.map_source == "cjlaaa" else MAP_DIR_RESOURCE
    print(f"[1/4] 扫描地图：{len(map_names)} 张（{rel(map_dir)}）", flush=True)
    map_stats = scan_maps(map_dir, map_names, book, args.map_source)
    compare = None
    if args.compare_maps:
        print("[1b] 对比 resource/map（v0）", flush=True)
        resource_book = RefBook()
        resource_stats = scan_maps(MAP_DIR_RESOURCE, map_names, resource_book, "resource")
        overlap = {}
        union = {}
        for lib in set(book.libs) | set(resource_book.libs):
            left = book.libs.get(lib, {}).get("indices", set())
            right = resource_book.libs.get(lib, {}).get("indices", set())
            overlap[lib] = len(left & right)
            union[lib] = len(left | right)
        compare = {
            "requested": len(map_names),
            "stats": resource_stats,
            "overlap": overlap,
            "union": union,
        }

    # ---- DB 实体 ----
    print("[2/4] 扫描 Server.MirDB 模板引用", flush=True)
    db_stats = collect_db_refs(db, book, args.include_reserved, notes)

    # ---- 音频 ----
    audio = {"files": [], "missing": [], "spell_sounds": []}
    if not args.skip_audio:
        print("[3/4] 扫描音频引用", flush=True)
        audio = collect_audio(db, notes)

    # ---- 图库统计 ----
    print("[4/4] 统计 .wzl 索引与帧体积", flush=True)
    stats_libs = compute_library_stats(book, args.include_reserved)
    sizes = asset_sizes()

    maps_used = [item["file"] for item in map_stats["files"]]
    used = build_used_assets(book, maps_used, audio, stats_libs, args)
    used["notes"].extend(notes)
    used["stats"]["maps_unused"] = len(sizes["maps"]) - len(maps_used)
    used["stats"]["maps_total_bytes"] = sum(item["bytes"] for item in sizes["maps"])
    used["stats"]["maps_used_bytes"] = map_stats["bytes"]
    used["stats"]["audio_bytes"] = sum(item["bytes"] for item in audio["files"])
    used["stats"]["fonts_bytes"] = sum(item["bytes"] for item in sizes["fonts"])
    used["stats"]["server_bytes"] = sum(item["bytes"] for item in sizes["server"])
    used["stats"]["wzl_total_bytes"] = stats_libs["totals"]["wzl_total_bytes"]
    used["stats"]["unused_wzl_bytes_approx"] = stats_libs["totals"]["unused_wzl_bytes_approx"]
    used["stats"]["map_versions"] = map_stats["versions"]
    used["stats"]["map_cells"] = map_stats["cells"]
    used["db"] = db_stats
    used["audio_files"] = audio["files"]

    out_path = REPO_ROOT / args.out
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(used, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    stats = {
        "generated_at": used["generated_at"],
        "dataset": args.dataset,
        "map_source": args.map_source,
        "map_stats": map_stats,
        "compare_resource_maps": compare,
        "db_refs": db_stats,
        "audio": audio,
        "libraries": stats_libs,
        "sizes": {
            section: {
                "files": len(items),
                "bytes": sum(item["bytes"] for item in items),
            }
            for section, items in sizes.items()
        },
    }
    stats_path = REPO_ROOT / args.stats
    stats_path.parent.mkdir(parents=True, exist_ok=True)
    stats_path.write_text(json.dumps(stats, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    report_path = REPO_ROOT / args.report
    report_path.parent.mkdir(parents=True, exist_ok=True)
    write_report(report_path, used, stats_libs, map_stats, compare, audio, sizes, time.time() - started)

    total_wzl = stats_libs["totals"]["wzl_total_bytes"]
    used_wzl = stats_libs["totals"]["used_compressed_bytes"]
    print("\n== 完成 ==")
    print(f"  图库：保留 {used['stats']['libraries']} 个 / {used['stats']['used_frames']:,} 帧，"
          f"PNG 估计 {used['stats']['estimated_png_bytes']/1e6:.0f} MB")
    print(f"  瓦片：{sum(1 for entry in used['libraries'] if entry['category'] == 'tiles')} 个图库；"
          f"地图 {map_stats['maps']} 张 / {map_stats['bytes']/1e6:.0f} MB")
    print(f"  怪物：{db_stats['monster_images']} 个图像（已刷出 {db_stats['monster_spawned_images']}），"
          f"未映射 {len(db_stats['monster_unmapped'])}")
    print(f"  音频：{len(audio['files'])} 个 / {used['stats']['audio_bytes']/1e6:.1f} MB，"
          f"缺失 {len(audio['missing'])}")
    print(f"  图库总量 {total_wzl/1e9:.2f} GB，其中未使用约 "
          f"{stats_libs['totals']['unused_wzl_bytes_approx']/1e9:.2f} GB")
    print(f"  产物：{rel(out_path)}、{rel(stats_path)}、{rel(report_path)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
