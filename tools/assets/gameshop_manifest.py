#!/usr/bin/env python3
"""R50-D 商城素材授权与哈希 manifest 生成/校验工具（户部）。

为 `Server.MirDB` 商城段（`GameShopItem`）的**每个 GIndex** 生成可复核的素材
manifest：`GIndex -> 图标/名称/价格/页签分组`，每条附 `source_path / bytes /
sha256 / license`。素材只取 `resource/` 现有图库经户部导入链产出的散装图标
（`game-assets/gfx/sprites/item/icon-<image:04>/<image:04>.png`），不新造、
不下载、不改 `resource/` 源文件。

授权口径：`docs/assets/licensing.md` §1（2026-10-20 项目所有者裁决）：
`resource/` 目录中的资源无视资源授权、全部可以使用；仅覆盖 `resource/` 内资源，
仓库源码/文档仍 GPL-3.0-or-later。manifest 的 `license` 记录该政策 id；
无法核验/缺失的条目只允许 PARTIAL 占位并写明 `partial_reason`、`license:"unverified"`，
**禁止把缺失写成齐全**。

命令（均可复跑、确定性输出；同输入 → 同字节）：
  py tools/assets/gameshop_manifest.py --write           # 生成 docs/assets/gameshop-manifest.json
  py tools/assets/gameshop_manifest.py --check           # 校验 manifest（缺项打印 partial: k 退出 1）
  py tools/assets/gameshop_manifest.py --verify-icons     # 逐图标核 存在+bytes+sha256
  py tools/assets/gameshop_manifest.py --verify-release  # 可选：核 MNP 发布包内逐图标条目
  py tools/assets/gameshop_manifest.py --dump <path>     # 可选：导出解析源数据（审计证据）

`--check` 与 `--verify-icons` 支持 `--allow-partial`（PARTIAL 项不导致非零退出），
但**不会**把 PARTIAL 说成 ok；`--check` 一定打印 `partial: k`。

数据来源与口径（全部可复核）：
  * `Server.MirDB` 商城段 = `GameShopItem`（`docs/store/mirdb-format.md` §4.8）：
    item_index / gindex / gold_price / credit_price / count / class / category /
    stock / istock / deal / top_item / date。`credit` = 元宝单价（CreditPrice）、
    `gold` = 金币单价（GoldPrice），均为 1 件单价，购买总额 = 单价 × 数量
    （`docs/world/gameshop.md` §2/§5）。
  * 物品模板（名称/图标 image 索引）来自 `Server.MirDB` items 段（同一文件，
    `ItemInfo`，`docs/store/mirdb-format.md` §4.2）；服务端启动时按 ItemIndex
    绑定模板，模板不存在即丢弃该 GIndex（`Envir.cs:4550-4561`）。
  * 图标 PNG 为 `resource/data/items.wzl` 经户部导入链的散装产物，客户端按
    `image` 索引取图标（`crates/mir-client/src/ui/mod.rs` `load_icon`，
    `crates/mir-render/src/entities.rs:1793 item_icon`）。

实现说明：本工具用 Python 按 `mirdb-format.md` 的逐字段布局独立解析
`Server.MirDB`（v105 样本；同时支持 85..117 的版本门），并用
`mir-tools mirdb-report` 的 10 段边界做锚点自检（样本 sha256 固定时逐段校验，
不一致立即失败），因此不依赖 Rust 侧改动，也不信任任何手抄清单。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
from pathlib import Path

SCHEMA_VERSION = 1
MANIFEST_KIND = "gameshop-manifest"
GENERATED_BY = "tools/assets/gameshop_manifest.py --write"

DEFAULT_DB = "resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB"
DEFAULT_MANIFEST = "docs/assets/gameshop-manifest.json"
ICON_ROOT = "game-assets/gfx/sprites/item"
ICON_PATH_PATTERN = ICON_ROOT + "/icon-{image:04}/{image:04}.png"
ICON_LOGICAL_PATTERN = "gfx/sprites/item/icon-{image:04}/{image:04}.png"
RELEASE_PACK_DIR = "game-assets/packs"
RELEASE_PACK_MANIFEST = RELEASE_PACK_DIR + "/manifest.packs.json"
CLEAN_REPORT = "game-assets/data/clean-report.json"
GAME_DATA_MANIFEST = "game-assets/data/manifest.json"

# 授权政策（docs/assets/licensing.md §1，2026-10-20 所有者裁决）。
LICENSE_POLICY_ID = "resource-policy-2026-10-20"
LICENSE_UNVERIFIED = "unverified"

# 已知样本（cjlaaa/Server.MirDB v105）的锚点：用于证明 Python 解析器逐段消费正确。
# 数值来自 `mir-tools mirdb-report` 实测输出（R50-D 证据 target/round50/r50-d/）。
PINNED_DB_SHA256 = "ace4fa6a70e18db7f8454350326d611ce6c56826b9f9a4f214797a3f8a5f69d0"
PINNED_DB_BYTES = 452106
PINNED_ANCHORS = {
    "maps": (40, 251090, 389),
    "items": (251090, 355442, 1349),
    "monsters": (355442, 398505, 544),
    "npcs": (398505, 429110, 309),
    "quests": (429110, 443035, 157),
    "dragon": (443035, 443174, 1),
    "magics": (443174, 448441, 107),
    "gameshop": (448441, 452077, 82),
    "conquests": (452077, 452081, 0),
    "respawn_timer": (452081, 452106, 1),
}

CAVEATS = [
    "图标像素来自 resource/data/items.wzl 经户部导入链的散装 PNG；"
    "docs/assets/game-assets.md §5.1.1 记录 172/830 空帧中 168 个按同索引由其它 "
    "resource/ 库补齐（韩服 items.Lib 104 / hbacc DX 37 / Arcadia 27），是**汇总级**"
    "溯源，逐图标归属未记录；本 manifest 逐条登记 sha256，不伪造逐图标出处。",
    "clean-report 中 3 个无图标模板（index 533/589/590）与 1 个派生占位图标模板"
    "（index 403）均未被本轮商城 82 条引用，故商城条目无缺图标；若后续商城引用它们，"
    "本工具会按 PARTIAL 输出。",
    "game-assets/gfx 与 game-assets/packs 均被 .gitignore 排除（本地构建产物）；"
    "source_path 是开发树相对路径，bytes/sha256 是生成时快照，重跑 --write 会按当前树刷新。",
    "release 段来自本地 game-assets/packs/manifest.packs.json 与 MNP 条目表（存在时）；"
    "发布包链只消费本 manifest 指向的 .mnp 条目（docs/world/gameshop.md §12.1）。",
    "价格口径：credit=元宝单价（CreditPrice）、gold=金币单价（GoldPrice），均按 1 件计；"
    "购买总额 = 单价 × 数量（docs/world/gameshop.md §5）。本工具只读，不改任何源文件。",
]


class ManifestError(RuntimeError):
    """解析/校验失败。"""


# --------------------------------------------------------------------------- #
# Server.MirDB 读取
# --------------------------------------------------------------------------- #


class Reader:
    """小端读取器（Server.MirDB 是 .NET BinaryWriter 输出：LEB128 长度 + UTF-8）。"""

    def __init__(self, buf: bytes, pos: int = 0):
        self.buf = buf
        self.pos = pos

    def take(self, n: int) -> bytes:
        if self.pos + n > len(self.buf):
            raise ManifestError(f"文件截断：offset {self.pos} 需要 {n} 字节，总长 {len(self.buf)}")
        raw = self.buf[self.pos : self.pos + n]
        self.pos += n
        return raw

    def i8(self) -> int:
        return struct.unpack("<b", self.take(1))[0]

    def u8(self) -> int:
        return self.take(1)[0]

    def i16(self) -> int:
        return struct.unpack("<h", self.take(2))[0]

    def u16(self) -> int:
        return struct.unpack("<H", self.take(2))[0]

    def i32(self) -> int:
        return struct.unpack("<i", self.take(4))[0]

    def u32(self) -> int:
        return struct.unpack("<I", self.take(4))[0]

    def i64(self) -> int:
        return struct.unpack("<q", self.take(8))[0]

    def u64(self) -> int:
        return struct.unpack("<Q", self.take(8))[0]

    def f32(self) -> float:
        return struct.unpack("<f", self.take(4))[0]

    def f64(self) -> float:
        return struct.unpack("<d", self.take(8))[0]

    def bool(self) -> bool:
        return self.u8() != 0

    def string(self) -> str:
        length = 0
        shift = 0
        while True:
            b = self.u8()
            length |= (b & 0x7F) << shift
            shift += 7
            if not b & 0x80:
                break
        return self.take(length).decode("utf-8")

    def i32_list(self) -> list[int]:
        return [self.i32() for _ in range(self.i32())]

    def stats(self) -> dict[int, int]:
        return {self.u8(): self.i32() for _ in range(self.i32())}


def _skip_map_info(r: Reader, version: int) -> None:
    r.i32()  # Index
    r.string()  # FileName
    r.string()  # Title
    r.u16()  # MiniMap
    r.u8()  # Light
    r.u16()  # BigMap
    for _ in range(r.i32()):  # SafeZones
        r.i32(), r.i32(), r.u16(), r.bool()
    for _ in range(r.i32()):  # Respawns
        r.i32(), r.i32(), r.i32(), r.u16(), r.u16(), r.u16(), r.u8()
        r.string()  # RoutePath
        if version > 67:
            r.u16(), r.i32(), r.bool(), r.u16()
    for _ in range(r.i32()):  # Movements
        r.i32(), r.i32(), r.i32(), r.i32(), r.i32(), r.bool(), r.bool()
        if version >= 69:
            r.i32()
        if version >= 95:
            r.bool()
        r.i32()  # Icon
    r.bool(), r.bool()  # NoTeleport, NoReconnect
    r.string()  # NoReconnectMap
    for _ in range(11):  # NoRandom..Fire
        r.bool()
    r.i32()  # FireDamage
    r.bool()  # Lightning
    r.i32()  # LightningDamage
    r.u8()  # MapDarkLight
    for _ in range(r.i32()):  # MineZones
        r.i32(), r.i32(), r.u16(), r.u8()
    r.u8()  # MineIndex
    r.bool(), r.bool(), r.bool()  # NoMount, NeedBridle, NoFight
    r.u16()  # Music
    if version >= 78:
        r.bool()  # NoTownTeleport
    if version >= 79:
        r.bool()  # NoReincarnation
    if version >= 110:
        r.u16()  # WeatherParticles
    if version >= 111:
        r.bool(), r.u8()  # GT, GTIndex
    if version >= 114:
        for _ in range(5):
            r.bool()
        r.i32()
        r.bool(), r.bool()
        r.i32()


def _read_item_infos(r: Reader, version: int) -> list[dict]:
    if version <= 84:
        raise ManifestError(
            f"不支持 Version={version} 的旧版固定字段编码（<=84）；"
            "本工具按 mirdb-format.md §4.2 的 ≥85 布局解析"
        )
    items = []
    for _ in range(r.i32()):
        item = {
            "index": r.i32(),
            "name": r.string(),
            "type": r.u8(),
            "grade": r.u8(),
            "required_type": r.u8(),
            "required_class": r.u8(),
            "required_gender": r.u8(),
            "set": r.u8(),
            "shape": r.i16(),
            "weight": r.u8(),
            "light": r.u8(),
            "required_amount": r.u8(),
            "image": r.u16(),
            "durability": r.u16(),
            "stack_size": r.u16(),
            "price": r.u32(),
            "start_item": r.bool(),
            "effect": r.u8(),
            "bools": r.u8(),
            "bind": r.i16(),
            "unique": r.i16(),
            "random_stats_id": r.u8(),
            "can_fast_run": r.bool(),
            "can_awakening": r.bool(),
            "slots": r.u8(),
            "stats": r.stats(),
            "tooltip": r.string() if r.bool() else None,
        }
        items.append(item)
    return items


def _skip_monster_infos(r: Reader, version: int) -> int:
    if version <= 84:
        raise ManifestError(f"不支持 Version={version} 的旧版怪物编码（<=84）")
    count = r.i32()
    for _ in range(count):
        r.i32()  # Index
        r.string()  # Name
        r.u16()  # Image
        r.u8(), r.u8()  # AI, Effect
        r.u16() if version >= 62 else r.u8()  # Level
        r.u8(), r.u8()  # ViewRange, CoolEye
        r.stats()  # Stats（>84）
        r.u8()  # Light
        r.u16(), r.u16()  # AttackSpeed, MoveSpeed
        r.u32()  # Experience
        r.bool(), r.bool()  # CanPush, CanTame
        if version >= 18:
            r.bool(), r.bool()  # AutoRev, Undead
        if version >= 89:
            r.string()  # DropPath
        if version >= 115:
            r.bool()  # CanRecall
        if version >= 116:
            r.bool()  # IsBoss
    return count


def _skip_npc_infos(r: Reader, version: int) -> int:
    count = r.i32()
    for _ in range(count):
        r.i32()  # Index
        r.i32()  # MapIndex
        r.i32_list()  # CollectQuestIndexes
        r.i32_list()  # FinishQuestIndexes
        r.string()  # FileName
        r.string()  # Name
        r.i32(), r.i32()  # X, Y
        r.u16() if version >= 72 else r.u8()  # Image
        r.u16()  # Rate
        if version >= 64:
            r.bool()  # TimeVisible
            r.u8(), r.u8(), r.u8(), r.u8()  # Hour/Minute Start/End
            r.i16(), r.i16()  # MinLev, MaxLev
            r.string(), r.string()  # DayofWeek, ClassRequired
        if version >= 66:
            r.i32()  # Conquest
        else:
            r.bool()  # Sabuk
        if version >= 64:
            r.i32()  # FlagNeeded
        if version > 95:
            r.bool(), r.i32()  # ShowOnBigMap, BigMapIcon
        if version > 96:
            r.bool()  # CanTeleportTo
        if version >= 107:
            r.bool()  # ConquestVisible
    return count


def _skip_quest_infos(r: Reader, version: int) -> int:
    count = r.i32()
    for _ in range(count):
        r.i32()  # Index
        r.string(), r.string(), r.string()  # Name, Group, FileName
        r.i32(), r.i32(), r.i32()  # RequiredMin/MaxLevel, RequiredQuest
        r.u8(), r.u8()  # RequiredClass, Type
        r.string(), r.string(), r.string(), r.string()  # 4 条消息
        if version > 90:
            r.i32()  # TimeLimitInSeconds
    return count


def _skip_dragon_info(r: Reader) -> None:
    r.bool()  # Enabled
    r.string(), r.string(), r.string()  # MapFileName, MonsterName, BodyName
    r.i32(), r.i32()  # Location X, Y
    r.i32(), r.i32()  # DropAreaTop X, Y
    r.i32(), r.i32()  # DropAreaBottom X, Y
    for _ in range(12):  # Exps（MaxDragonLevel - 1）
        r.i64()


def _skip_magic_infos(r: Reader, version: int) -> int:
    count = r.i32()
    for _ in range(count):
        r.string()  # Name
        r.u8()  # Spell
        r.u8(), r.u8(), r.u8()  # BaseCost, LevelCost, Icon
        r.u8(), r.u8(), r.u8()  # Level1..3
        r.u16(), r.u16(), r.u16()  # Need1..3
        r.u32(), r.u32()  # DelayBase, DelayReduction
        r.u16(), r.u16(), r.u16(), r.u16()  # PowerBase/Bonus, MPowerBase/Bonus
        if version > 66:
            r.u8()  # Range
        if version > 70:
            r.f32(), r.f32()  # MultiplierBase/Bonus
    return count


def _read_gameshop(r: Reader, version: int) -> list[dict]:
    rows = []
    for _ in range(r.i32()):
        row = {
            "item_index": r.i32(),
            "gindex": r.i32(),
            "gold_price": r.u32(),
            "credit_price": r.u32(),
            "count": r.u16() if version > 84 else r.u32(),
            "class": r.string(),
            "category": r.string(),
            "stock": r.i32(),
            "istock": r.bool(),
            "deal": r.bool(),
            "top_item": r.bool(),
            "date": r.i64(),
        }
        if version > 105:
            row["can_buy_credit"] = r.bool()
            row["can_buy_gold"] = r.bool()
        rows.append(row)
    return rows


def parse_mir_db(path: Path) -> dict:
    """独立解析 Server.MirDB，返回 header/sections/items/gameshop。

    解析器按 `docs/store/mirdb-format.md` 的逐字段布局实现；对已知样本
    （sha256 固定）额外逐段校验边界，证明「逐字节消费到文件尾」。
    """

    data = path.read_bytes()
    sha = hashlib.sha256(data).hexdigest()
    r = Reader(data)
    version = r.i32()
    custom_version = r.i32()
    counters = {
        "map": r.i32(),
        "item": r.i32(),
        "monster": r.i32(),
        "npc": r.i32(),
        "quest": r.i32(),
    }
    if version >= 63:
        counters["gameshop"] = r.i32()
    if version >= 66:
        counters["conquest"] = r.i32()
    if version >= 68:
        counters["respawn"] = r.i32()

    sections: dict[str, dict] = {}

    def record(name: str, start: int, count: int) -> None:
        sections[name] = {"start": start, "end": r.pos, "count": count}

    start = r.pos
    maps_count = r.i32()
    for _ in range(maps_count):
        _skip_map_info(r, version)
    record("maps", start, maps_count)

    start = r.pos
    items = _read_item_infos(r, version)
    record("items", start, len(items))

    start = r.pos
    count = _skip_monster_infos(r, version)
    record("monsters", start, count)

    start = r.pos
    count = _skip_npc_infos(r, version)
    record("npcs", start, count)

    start = r.pos
    count = _skip_quest_infos(r, version)
    record("quests", start, count)

    start = r.pos
    _skip_dragon_info(r)
    record("dragon", start, 1)

    start = r.pos
    count = _skip_magic_infos(r, version)
    record("magics", start, count)

    start = r.pos
    gameshop = _read_gameshop(r, version)
    record("gameshop", start, len(gameshop))

    start = r.pos
    conquests = r.i32() if r.pos + 4 <= len(data) else -1
    if conquests > 0:
        raise ManifestError(
            f"样本含 {conquests} 条攻城记录；本工具只为商城 manifest 解析商城段，"
            "不支持带攻城段的样本（已知样本为 0 条）"
        )
    if conquests == 0:
        record("conquests", start, 0)

    if version > 67 and r.pos + 13 <= len(data):
        start = r.pos
        r.u8()  # BaseSpawnRate
        r.u64()  # CurrentTickcounter
        for _ in range(r.i32()):
            r.i32()  # UserCount
            r.f64()  # DelayLoss
        record("respawn_timer", start, 1)

    leftover = len(data) - r.pos

    return {
        "path": path,
        "bytes": len(data),
        "sha256": sha,
        "version": version,
        "custom_version": custom_version,
        "counters": counters,
        "sections": sections,
        "items": items,
        "gameshop": gameshop,
        "leftover": leftover,
    }


def verify_pinned_anchors(db: dict, strict: bool = False) -> list[str]:
    """对已知样本逐段校验边界；返回人类可读的检查行。"""

    if db["sha256"] != PINNED_DB_SHA256:
        message = (
            f"源 DB 不是固定核验样本：sha256={db['sha256']}（期望 {PINNED_DB_SHA256}，"
            f"bytes 期望 {PINNED_DB_BYTES} 实为 {db['bytes']}）"
        )
        if strict:
            raise ManifestError(
                message + "；拒绝生成 manifest（如需核验新数据源，须先评审并更新 PINNED_* 常量）"
            )
        return [f"anchors: skipped（{message}）"]
    problems = []
    for name, (start_expected, end_expected, count_expected) in PINNED_ANCHORS.items():
        section = db["sections"].get(name)
        if section is None:
            problems.append(f"{name}: 缺段")
            continue
        if (section["start"], section["end"]) != (start_expected, end_expected):
            problems.append(
                f"{name}: 段应为 [{start_expected},{end_expected}) 实为 [{section['start']},{section['end']})"
            )
        if section["count"] != count_expected:
            problems.append(f"{name}: 条数应为 {count_expected} 实为 {section['count']}")
    if db["bytes"] != PINNED_DB_BYTES:
        problems.append(f"bytes 应为 {PINNED_DB_BYTES} 实为 {db['bytes']}")
    if db["leftover"] != 0:
        problems.append(f"leftover 应为 0 实为 {db['leftover']}")
    if problems:
        raise ManifestError("锚点校验失败（解析器与 mirdb-format 不一致）：\n  " + "\n  ".join(problems))
    return [f"anchors ok: {len(PINNED_ANCHORS)} 段逐字节消费到文件尾（leftover=0）"]


# --------------------------------------------------------------------------- #
# 图标 / 发布包 / 清洗报告等本地素材事实
# --------------------------------------------------------------------------- #


def sha256_file(path: Path) -> tuple[int, str]:
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            size += len(chunk)
            digest.update(chunk)
    return size, digest.hexdigest()


def read_clean_report(path: Path) -> dict | None:
    if not path.is_file():
        return None
    raw = path.read_bytes()
    report = json.loads(raw.decode("utf-8"))
    return {
        "path": path,
        "bytes": len(raw),
        "sha256": hashlib.sha256(raw).hexdigest(),
        "counts": report.get("counts", {}),
        "mnd_sha256": report.get("mnd_sha256"),
        "kept": {str(entry["index"]): entry for entry in report.get("kept", [])},
        "pruned": {str(entry["index"]): entry for entry in report.get("pruned", [])},
        "derived": {str(entry["index"]): entry for entry in report.get("warn", []) if entry.get("icon_filled")},
    }


def read_release_pack(root: Path) -> dict | None:
    """读 game-assets/packs 的发布包清单与 sprites-item 包条目表（存在时）。"""

    manifest_path = root / RELEASE_PACK_MANIFEST
    if not manifest_path.is_file():
        return None
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    pack = None
    for summary in manifest.get("packs", []):
        if str(summary.get("prefix", "")).startswith("gfx/sprites/item/"):
            pack = summary
            break
    if pack is None:
        return None
    pack_path = root / RELEASE_PACK_DIR / pack["file"]
    result = {
        "file": pack["file"],
        "bytes": pack["bytes"],
        "sha256": pack["sha256"],
        "entries": pack["entries"],
        "prefix": pack.get("prefix", ""),
        "present": pack_path.is_file(),
        "pack_sha256_actual": None,
        "names": None,
    }
    if pack_path.is_file():
        size, digest = sha256_file(pack_path)
        result["pack_bytes_actual"] = size
        result["pack_sha256_actual"] = digest
        try:
            result["names"] = pack_entry_names(pack_path.read_bytes())
        except ManifestError:
            result["names"] = None
    return result


def pack_entry_names(data: bytes) -> set[str]:
    """解析 MNP 条目表（crates/mir-asset/src/pack.rs 的格式文档）。"""

    if len(data) < 64 or data[:4] != b"MNP1":
        raise ManifestError("不是 MNP1 包")
    (version,) = struct.unpack_from("<H", data, 4)
    (_flags,) = struct.unpack_from("<H", data, 6)
    (entry_count,) = struct.unpack_from("<I", data, 8)
    (index_offset,) = struct.unpack_from("<Q", data, 12)
    if version != 1:
        raise ManifestError(f"未知 MNP version={version}")
    pos = index_offset
    limit = len(data) - 32  # 尾部 sha256
    names: set[str] = set()
    for _ in range(entry_count):
        if pos + 2 > limit:
            raise ManifestError("MNP 条目表截断")
        (name_len,) = struct.unpack_from("<H", data, pos)
        pos += 2
        names.add(data[pos : pos + name_len].decode("utf-8"))
        pos += name_len
        # pack.rs::parse_entry：kind u8 + has_mask u8 + format u8 + reserved 3B
        # + width/height u16×2 + offset/shadow i16×4 + data_offset u64
        # + data_len u32 + raw_len u32 = 固定 34 字节。
        pos += 34
    return names


# --------------------------------------------------------------------------- #
# manifest 生成
# --------------------------------------------------------------------------- #


def icon_rel_path(image: int) -> str:
    return ICON_PATH_PATTERN.format(image=image)


def build_manifest(root: Path, db_path: Path) -> tuple[dict, list[str]]:
    """按 DB + 本地散装图标生成 manifest；返回 (manifest, 生成日志行)。"""

    log: list[str] = []
    db = parse_mir_db(db_path)
    anchors = verify_pinned_anchors(db, strict=True)
    log.extend(anchors)
    log.append(
        f"source: {db_path.relative_to(root).as_posix()} bytes={db['bytes']} sha256={db['sha256']} "
        f"version={db['version']} items={len(db['items'])} gameshop={len(db['gameshop'])} "
        f"leftover={db['leftover']}"
    )

    clean = read_clean_report(root / CLEAN_REPORT)
    if clean:
        log.append(
            f"catalog: {CLEAN_REPORT} sha256={clean['sha256']} kept={len(clean['kept'])} "
            f"pruned={len(clean['pruned'])} warn={len(clean['derived'])} "
            f"mnd_sha256={clean['mnd_sha256']}"
        )

    release = read_release_pack(root)
    if release:
        status = "ok" if release["present"] else "missing"
        names_count = "?" if release["names"] is None else len(release["names"])
        log.append(
            f"release: {RELEASE_PACK_DIR}/{release['file']} entries={release['entries']} "
            f"names={names_count} bytes={release['bytes']} status={status}"
        )
    else:
        log.append(f"release: {RELEASE_PACK_DIR} 不可用（仅登记开发树散装图标）")

    items_by_index = {item["index"]: item for item in db["items"]}
    entries: list[dict] = []
    partials: list[dict] = []
    group_counts: dict[str, int] = {}

    for row in sorted(db["gameshop"], key=lambda value: value["gindex"]):
        gindex = row["gindex"]
        item_index = row["item_index"]
        item = items_by_index.get(item_index)

        entry: dict = {
            "gindex": gindex,
            "item_index": item_index,
            "name": None,
            "category": row.get("category") or "",
            "class": row.get("class") or "",
            "count": row["count"],
            "credit": row["credit_price"],
            "credit_source": "Server.MirDB gameshop.credit_price（GameShopItem.CreditPrice，元宝单价）",
            "gold": row["gold_price"],
            "gold_source": "Server.MirDB gameshop.gold_price（GameShopItem.GoldPrice，金币单价）",
            "price_unit": "1 件单价；购买总额 = 单价 × 数量（docs/world/gameshop.md §5）",
            "stock": row["stock"],
            "icon": None,
            "icon_image": None,
            "icon_pack": None,
            "source_path": None,
            "bytes": None,
            "sha256": None,
            "license": LICENSE_UNVERIFIED,
            "partial": True,
            "partial_reason": None,
        }

        reasons: list[str] = []
        if item is None:
            reasons.append(
                f"物品模板缺失（ItemIndex={item_index} 不在 Server.MirDB items 段；服务端启动会丢弃该 GIndex）"
            )
        else:
            entry["name"] = item["name"]
            image = item["image"]
            entry["icon_image"] = image
            logical = ICON_LOGICAL_PATTERN.format(image=image)
            entry["icon"] = logical
            png = root / icon_rel_path(image)
            entry["source_path"] = icon_rel_path(image)

            derived = clean["derived"].get(str(item_index)) if clean else None
            pruned = clean["pruned"].get(str(item_index)) if clean else None
            if pruned is not None:
                reasons.append(
                    f"图标缺失：clean-report 记录 index={item_index}/image={image} "
                    f"reason={pruned.get('reason')}"
                )
            elif png.is_file():
                size, digest = sha256_file(png)
                entry["bytes"] = size
                entry["sha256"] = digest
                if derived is not None:
                    reasons.append(
                        f"派生占位图标：非该物品原始帧，derived_from={derived.get('icon_derived_from')}"
                        f"（clean-report icon_origin=derived）"
                    )
            else:
                reasons.append(f"图标文件缺失：{icon_rel_path(image)}")

            if release is not None and release["names"] is not None:
                if logical in release["names"]:
                    entry["icon_pack"] = release["file"]
                else:
                    reasons.append(f"发布包 {release['file']} 不含条目 {logical}")

        if not entry["category"]:
            reasons.append("category 为空（页签分组缺失）")
        else:
            group_counts[entry["category"]] = group_counts.get(entry["category"], 0) + 1

        if reasons:
            entry["partial"] = True
            entry["partial_reason"] = "；".join(reasons)
            partials.append({"gindex": gindex, "item_index": item_index, "reason": entry["partial_reason"]})
        else:
            entry["partial"] = False
            entry["license"] = LICENSE_POLICY_ID

        entries.append(entry)

    counts = {
        "entries": len(entries),
        "ok": len(entries) - len(partials),
        "partial": len(partials),
    }

    source: dict = {
        "database": {
            "path": db_path.relative_to(root).as_posix(),
            "bytes": db["bytes"],
            "sha256": db["sha256"],
            "db_version": db["version"],
            "custom_version": db["custom_version"],
            "counters": db["counters"],
            "sections": db["sections"],
            "leftover": db["leftover"],
        },
    }
    if clean:
        source["item_catalog"] = {
            "path": CLEAN_REPORT,
            "sha256": clean["sha256"],
            "counts": clean["counts"],
            "mnd_sha256": clean["mnd_sha256"],
        }
    game_data_manifest = root / GAME_DATA_MANIFEST
    if game_data_manifest.is_file():
        raw = game_data_manifest.read_bytes()
        data_manifest = json.loads(raw.decode("utf-8"))
        source["game_data"] = {
            "path": GAME_DATA_MANIFEST,
            "sha256": hashlib.sha256(raw).hexdigest(),
            "db_sha256": data_manifest.get("sha256"),
            "db_bytes": data_manifest.get("db_bytes"),
            "source_db": data_manifest.get("source", {}).get("db"),
        }

    icon_block: dict = {
        "root": ICON_ROOT,
        "dev_path_pattern": ICON_PATH_PATTERN,
        "logical_pattern": ICON_LOGICAL_PATTERN,
        "client_lookup": (
            "crates/mir-render/src/entities.rs item_icon(image) -> SpriteLibrary::Item 帧 image；"
            "crates/mir-client/src/ui/mod.rs load_icon(scene, icons, image)"
        ),
        "release": None,
    }
    if release is not None:
        release_summary = {
            "file": release["file"],
            "bytes": release["bytes"],
            "sha256": release["sha256"],
            "entries": release["entries"],
            "prefix": release["prefix"],
            "present": release["present"],
            "pack_sha256_actual": release["pack_sha256_actual"],
            "manifest_packs_json": RELEASE_PACK_MANIFEST,
        }
        if release["names"] is not None:
            release_summary["index_entries_parsed"] = len(release["names"])
        icon_block["release"] = release_summary

    manifest = {
        "schema_version": SCHEMA_VERSION,
        "kind": MANIFEST_KIND,
        "round": "R50",
        "generated_by": GENERATED_BY,
        "license": {
            "policy_id": LICENSE_POLICY_ID,
            "policy": (
                "docs/assets/licensing.md §1（2026-10-20 项目所有者裁决）：resource/ 目录中的资源"
                "无视资源授权、全部可以使用；仅覆盖 resource/ 内资源，仓库源码/文档仍 GPL-3.0-or-later"
            ),
            "source_doc": "docs/assets/licensing.md",
            "unverified_marker": LICENSE_UNVERIFIED,
            "notes": "图标为 resource/data/items.wzl 派生 PNG；manifest 不改动 resource/ 源文件，不新造图标",
        },
        "source": source,
        "icon": icon_block,
        "price": {
            "credit": "元宝单价（GameShopItem.CreditPrice）",
            "gold": "金币单价（GameShopItem.GoldPrice）",
            "unit_note": "单价按 1 件计；购买总额 = 单价 × 数量（docs/world/gameshop.md §5）",
        },
        "counts": counts,
        "groups": dict(sorted(group_counts.items())),
        "entries": entries,
        "partials": partials,
        "caveats": CAVEATS,
    }
    log.append(f"built: entries={counts['entries']} ok={counts['ok']} partial={counts['partial']}")
    return manifest, log


# --------------------------------------------------------------------------- #
# 校验
# --------------------------------------------------------------------------- #


REQUIRED_FIELDS = (
    "gindex",
    "icon",
    "name",
    "credit",
    "gold",
    "category",
    "source_path",
    "bytes",
    "sha256",
    "license",
)


def load_manifest(path: Path) -> dict:
    if not path.is_file():
        raise ManifestError(f"manifest 不存在：{path}（先跑 --write）")
    return json.loads(path.read_text(encoding="utf-8"))


def validate_manifest_shape(manifest: dict, manifest_path: Path) -> tuple[list[str], list[str]]:
    """校验结构与必填字段；返回 (ok 行, 问题行)。"""

    problems: list[str] = []
    info: list[str] = []
    if manifest.get("schema_version") != SCHEMA_VERSION or manifest.get("kind") != MANIFEST_KIND:
        problems.append(f"schema/kind 不符：schema_version={manifest.get('schema_version')} kind={manifest.get('kind')}")
    entries = manifest.get("entries")
    if not isinstance(entries, list) or not entries:
        problems.append("entries 为空")
        return info, problems

    source_db = (manifest.get("source") or {}).get("database") or {}
    if source_db.get("sha256") != PINNED_DB_SHA256 or source_db.get("bytes") != PINNED_DB_BYTES:
        problems.append(
            f"manifest.source.database 不是固定核验样本：sha256={source_db.get('sha256')} "
            f"bytes={source_db.get('bytes')}（期望 {PINNED_DB_SHA256}/{PINNED_DB_BYTES}）"
        )
    else:
        sections = source_db.get("sections") or {}
        for name, (start_expected, end_expected, count_expected) in PINNED_ANCHORS.items():
            section = sections.get(name)
            if section is None:
                problems.append(f"manifest.source.database.sections 缺 {name}")
                continue
            if (section.get("start"), section.get("end")) != (start_expected, end_expected):
                problems.append(
                    f"manifest 段 {name} 边界不符：[{section.get('start')},{section.get('end')}) "
                    f"!= [{start_expected},{end_expected})"
                )
            if section.get("count") != count_expected:
                problems.append(f"manifest 段 {name} 条数不符：{section.get('count')} != {count_expected}")

    gindexes = [entry.get("gindex") for entry in entries]
    if gindexes != sorted(gindexes):
        problems.append("entries 未按 gindex 升序排列（确定性要求）")
    if len(set(gindexes)) != len(gindexes):
        problems.append("gindex 重复")
    for entry in entries:
        missing = [key for key in REQUIRED_FIELDS if key not in entry]
        if missing:
            problems.append(f"gindex={entry.get('gindex')} 缺字段 {missing}")
            continue
        if entry.get("partial"):
            if not entry.get("partial_reason"):
                problems.append(f"gindex={entry.get('gindex')} partial 但缺 partial_reason")
            if entry.get("license") != LICENSE_UNVERIFIED:
                problems.append(f"gindex={entry.get('gindex')} partial 但 license 不是 unverified")
        else:
            for key in ("icon", "name", "source_path", "bytes", "sha256"):
                if entry.get(key) is None:
                    problems.append(f"gindex={entry.get('gindex')} 非 partial 但 {key} 为空")
            if entry.get("license") != LICENSE_POLICY_ID:
                problems.append(f"gindex={entry.get('gindex')} 非 partial 但 license 不是政策 id")
            if not isinstance(entry.get("credit"), int) or not isinstance(entry.get("gold"), int):
                problems.append(f"gindex={entry.get('gindex')} credit/gold 不是整数")

    counts = manifest.get("counts") or {}
    partial_entries = [entry for entry in entries if entry.get("partial")]
    expected = {
        "entries": len(entries),
        "ok": len(entries) - len(partial_entries),
        "partial": len(partial_entries),
    }
    if counts != expected:
        problems.append(f"counts 与 entries 不符：manifest={counts} 实际={expected}")

    groups = manifest.get("groups") or {}
    if sum(groups.values()) != len(entries):
        problems.append(f"groups 计数之和 {sum(groups.values())} != entries {len(entries)}")

    partials = manifest.get("partials")
    if not isinstance(partials, list) or len(partials) != len(partial_entries):
        problems.append("partials 列表与 partial entries 数量不一致")

    info.append(f"entries: {len(entries)}（gindex {min(gindexes)}..{max(gindexes)}，去重 {len(set(gindexes))}）")
    info.append(f"counts: ok={expected['ok']} partial={expected['partial']}")
    return info, problems


def cross_check_source(manifest: dict, db_path: Path) -> tuple[list[str], list[str]]:
    """manifest 与源 DB 交叉核对（源不存在时跳过，仅记录）。"""

    info: list[str] = []
    problems: list[str] = []
    source = manifest.get("source", {}).get("database", {})
    if not db_path.is_file():
        info.append(f"source: missing（{db_path} 不存在，跳过覆盖交叉核对）")
        return info, problems
    db = parse_mir_db(db_path)
    if db["sha256"] != source.get("sha256"):
        problems.append(
            f"源 DB 已变化：manifest.sha256={source.get('sha256')} 当前={db['sha256']}（需重跑 --write）"
        )
        return info, problems
    expected = {row["gindex"]: row["item_index"] for row in db["gameshop"]}
    actual = {entry["gindex"]: entry.get("item_index") for entry in manifest.get("entries", [])}
    if expected != actual:
        missing = sorted(set(expected) - set(actual))
        extra = sorted(set(actual) - set(expected))
        changed = sorted(g for g in set(expected) & set(actual) if expected[g] != actual[g])
        problems.append(f"GIndex 覆盖与源不符：缺 {missing} 多 {extra} item_index 变化 {changed}")
    else:
        info.append(f"source: 覆盖一致（{len(expected)} 个 GIndex 与源商城段一一对应）")
    return info, problems


def cmd_check(args: argparse.Namespace) -> int:
    root = Path(args.root)
    manifest_path = root / args.manifest
    try:
        manifest = load_manifest(manifest_path)
        info, problems = validate_manifest_shape(manifest, manifest_path)
    except ManifestError as error:
        print(f"check failed: {error}")
        return 2
    info2, problems2 = cross_check_source(manifest, root / args.db)

    raw = manifest_path.read_bytes()
    print(f"manifest: {manifest_path.relative_to(root).as_posix()} bytes={len(raw)} sha256={hashlib.sha256(raw).hexdigest()}")
    for line in info + info2:
        print(line)
    partial = [entry for entry in manifest.get("entries", []) if entry.get("partial")]
    for entry in partial:
        print(f"partial: gindex={entry.get('gindex')} item_index={entry.get('item_index')} reason={entry.get('partial_reason')}")
    if partial:
        print(f"partial: {len(partial)}")
    for line in problems + problems2:
        print(f"problem: {line}")
    if problems or problems2:
        return 1
    print(f"manifest ok: {len(manifest['entries'])}")
    if partial:
        return 0 if args.allow_partial else 1
    return 0


def cmd_verify_icons(args: argparse.Namespace) -> int:
    root = Path(args.root)
    manifest_path = root / args.manifest
    try:
        manifest = load_manifest(manifest_path)
    except ManifestError as error:
        print(f"verify-icons failed: {error}")
        return 2
    bad: list[str] = []
    ok = 0
    partial_icons = 0
    for entry in manifest.get("entries", []):
        gindex = entry.get("gindex")
        source_path = entry.get("source_path")
        if entry.get("partial") and source_path is None:
            partial_icons += 1
            continue
        if source_path is None:
            bad.append(f"gindex={gindex} source_path 为空")
            continue
        path = root / source_path
        expected_path = None
        if entry.get("icon_image") is not None:
            expected_path = icon_rel_path(entry["icon_image"])
        if expected_path is not None and source_path != expected_path:
            bad.append(f"gindex={gindex} source_path 与 icon_image 不符：{source_path} != {expected_path}")
            continue
        if not path.is_file():
            bad.append(f"gindex={gindex} 图标缺失：{source_path}")
            continue
        size, digest = sha256_file(path)
        if size != entry.get("bytes") or digest != entry.get("sha256"):
            bad.append(
                f"gindex={gindex} 哈希不符：manifest bytes={entry.get('bytes')} sha256={entry.get('sha256')} "
                f"实际 bytes={size} sha256={digest}"
            )
            continue
        ok += 1

    print(f"icons ok: {ok}")
    if partial_icons:
        print(f"icons partial: {partial_icons}（缺失图标条目，已列入 manifest.partials）")
    if bad:
        for line in bad:
            print(f"icons bad: {line}")
        print(f"icons bad: {len(bad)}")
        return 0 if args.allow_partial else 1
    if partial_icons and not args.allow_partial:
        # fail-closed：manifest 里有无法核验的缺失图标时，不得以「全绿」退出。
        return 1
    return 0


def cmd_verify_release(args: argparse.Namespace) -> int:
    root = Path(args.root)
    release = read_release_pack(root)
    if release is None:
        print("release: unavailable（game-assets/packs/manifest.packs.json 不存在或无 sprites-item 包）")
        return 0
    if not release["present"]:
        print(f"release bad: {RELEASE_PACK_DIR}/{release['file']} 不存在")
        return 1
    problems: list[str] = []
    if release["pack_sha256_actual"] != release["sha256"]:
        problems.append(
            f"整包 sha256 不符：manifest={release['sha256']} 实际={release['pack_sha256_actual']}"
        )
    if release["names"] is None:
        problems.append("MNP 条目表不可解析")
    manifest = load_manifest(root / args.manifest)
    ok = 0
    missing: list[str] = []
    for entry in manifest.get("entries", []):
        if entry.get("icon_image") is None:
            continue
        logical = ICON_LOGICAL_PATTERN.format(image=entry["icon_image"])
        if release["names"] is not None and logical in release["names"]:
            ok += 1
        else:
            missing.append(f"gindex={entry.get('gindex')} {logical}")
    if missing:
        problems.extend(missing)
    print(f"release: {RELEASE_PACK_DIR}/{release['file']} entries={release['entries']} icons ok: {ok}")
    for line in problems:
        print(f"release bad: {line}")
    if problems:
        return 1
    return 0


def cmd_write(args: argparse.Namespace) -> int:
    root = Path(args.root)
    db_path = root / args.db
    if not db_path.is_file():
        print(f"write failed: 源 DB 不存在：{db_path}")
        return 2
    try:
        manifest, log = build_manifest(root, db_path)
    except ManifestError as error:
        print(f"write failed: {error}")
        return 2
    for line in log:
        print(line)
    payload = (json.dumps(manifest, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    target = root / args.manifest
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(payload)
    print(
        f"wrote: {target.relative_to(root).as_posix()} bytes={len(payload)} "
        f"sha256={hashlib.sha256(payload).hexdigest()} entries={manifest['counts']['entries']} "
        f"ok={manifest['counts']['ok']} partial={manifest['counts']['partial']}"
    )
    return 0


def cmd_dump(args: argparse.Namespace) -> int:
    root = Path(args.root)
    db_path = root / args.db
    try:
        db = parse_mir_db(db_path)
        anchors = verify_pinned_anchors(db)
    except ManifestError as error:
        print(f"dump failed: {error}")
        return 2
    items_by_index = {item["index"]: item for item in db["items"]}
    gameshop = sorted(db["gameshop"], key=lambda value: value["gindex"])
    referenced: dict[int, dict] = {}
    for row in gameshop:
        item = items_by_index.get(row["item_index"])
        if item is not None:
            referenced[row["item_index"]] = {
                "index": item["index"],
                "name": item["name"],
                "type": item["type"],
                "image": item["image"],
                "shape": item["shape"],
                "price": item["price"],
                "stack_size": item["stack_size"],
            }
    clean = read_clean_report(root / CLEAN_REPORT)
    release = read_release_pack(root)
    release_dump = None
    if release is not None:
        release_dump = {key: value for key, value in release.items() if key != "names"}
        release_dump["names_parsed"] = None if release["names"] is None else len(release["names"])
    dump = {
        "tool": GENERATED_BY.replace(" --write", " --dump"),
        "anchors": anchors,
        "database": {
            "path": args.relative_db,
            "bytes": db["bytes"],
            "sha256": db["sha256"],
            "version": db["version"],
            "custom_version": db["custom_version"],
            "counters": db["counters"],
            "sections": db["sections"],
            "leftover": db["leftover"],
        },
        "clean_report": {
            "path": CLEAN_REPORT,
            "sha256": clean["sha256"] if clean else None,
            "counts": clean["counts"] if clean else None,
            "pruned": clean["pruned"] if clean else None,
            "derived": clean["derived"] if clean else None,
        },
        "release_pack": release_dump,
        "gameshop": gameshop,
        "referenced_items": [referenced[key] for key in sorted(referenced)],
    }
    out = Path(args.dump)
    if not out.is_absolute():
        out = root / out
    payload = (json.dumps(dump, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(payload)
    print(
        f"dumped: {out.as_posix()} bytes={len(payload)} sha256={hashlib.sha256(payload).hexdigest()} "
        f"gameshop={len(gameshop)} referenced_items={len(referenced)}"
    )
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="R50-D 商城素材授权与哈希 manifest 生成/校验（户部）。",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true", help="生成 docs/assets/gameshop-manifest.json")
    mode.add_argument("--check", action="store_true", help="校验 manifest（缺项打印 partial: k 退出 1）")
    mode.add_argument("--verify-icons", action="store_true", help="逐图标核 存在+bytes+sha256")
    mode.add_argument("--verify-release", action="store_true", help="核 MNP 发布包内逐图标条目")
    mode.add_argument("--dump", metavar="PATH", help="导出解析出的源数据（审计证据，JSON）")
    parser.add_argument("--allow-partial", action="store_true", help="PARTIAL 项不导致非零退出（仍打印 partial）")
    parser.add_argument("--root", default=None, help="仓库根目录（默认 = 本脚本上两级）")
    parser.add_argument("--db", default=DEFAULT_DB, help=f"Server.MirDB 相对路径（默认 {DEFAULT_DB}）")
    parser.add_argument("--manifest", default=DEFAULT_MANIFEST, help=f"manifest 相对路径（默认 {DEFAULT_MANIFEST}）")
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_intermixed_args(argv)
    if args.root is None:
        args.root = str(Path(__file__).resolve().parents[2])
    args.relative_db = args.db
    if args.write:
        return cmd_write(args)
    if args.check:
        return cmd_check(args)
    if args.verify_icons:
        return cmd_verify_icons(args)
    if args.verify_release:
        return cmd_verify_release(args)
    if args.dump:
        return cmd_dump(args)
    parser.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
