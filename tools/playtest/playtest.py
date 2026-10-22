#!/usr/bin/env python3
"""《传奇-Next》可玩性自检（playtest）——只依赖 Python 标准库。

连接真实服务端（默认 127.0.0.1:7000），按「新玩家 10 分钟能玩到的东西」逐项验证并
打印 [OK]/[FAIL]/[SKIP]；任一项失败时退出码为 1，全部通过/跳过为 0。
末行按实际计数输出「全部通过（0 跳过）」或「全部通过（含 N 个跳过项）」并列出跳过项。

用法：
    python tools/playtest/playtest.py
    python tools/playtest/playtest.py --server 127.0.0.1:7000 --timeout 8

设计要点：
* 所有等待都有超时（默认 8s），超时时打印「期望收到什么包」的诊断；
* 不发 KeepAlive、不 sleep 死等：每次等待都是「收包直到目标包或超时」；
* 服务端缺资源（例如起始地图没有 NPC/鸡鹿）时该项打印 [SKIP] 并说明原因；
* **幂等**：同一服务端可以连续跑多次——GM 模式下进世界后先 `@map` 回起始图并校验落点
  （跨图传送那一步会把角色留在目标图），非 GM 模式每次注册新账号新角色。
* `--tp1`（同图导航 gate）的已知限制：脚本用 BFS 走到 `S.NewMapInfo.movements` 门格，
  不驱动客户端大地图 UI 点击；当前数据没有 1–15 级跨图任务；quest 5 的 NPC 不发布该任务，
  因此用 quest 1（Jane）。详见 `README.md`「已知限制（第 26/27 轮 TP-1 gate）」。

协议：帧 = u16 总长（含 4 字节头）+ i16 包号 + 负载；字符串为 7-bit 变长长度 + UTF-8；
标了 Compressed 的包（NPCGoods 等）负载是 gzip。
"""

from __future__ import annotations

import argparse
import collections
import gzip
import json
import pathlib
import random
import re
import socket
import struct
import sys
import time

# --- 客户端包号（crates/mir-protocol/src/packets/client.rs） ---
C_CLIENT_VERSION = 0
C_NEW_ACCOUNT = 3
C_LOGIN = 5
C_NEW_CHARACTER = 6
C_START_GAME = 8
C_LOGOUT = 9
C_WALK = 11
C_TURN = 10
C_ATTACK = 47
C_EQUIP_ITEM = 18
C_PICKUP = 35
C_USE_ITEM = 22
C_CALL_NPC = 50
C_CHAT = 13
C_MAGIC = 58
C_BUY_ITEM = 51
C_SELL_ITEM = 52
C_ACCEPT_QUEST = 104
C_FINISH_QUEST = 105
C_OPENDOOR = 136
C_TOWNREVIVE = 68
S_DEATH = 80
S_REVIVED = 136
S_OBJECT_STRUCK = 74
S_DAMAGE_INDICATOR = 75
S_OBJECT_ATTACK = 72

#: 比奇药店最便宜的初级红药（`Server.MirDB` 里 `(HP)DrugSmall` 的模板号）。
POTION_ITEM_INDEX = 658

#: 起始图（比奇省）的候选标识：`@map 0 330 270` 里的 "0" 是**地图文件名**，
#: 但有些数据集里 `map_index 0` 不存在（本仓库的 `Server.MirDB` 里比奇的 index 是 1、
#: 文件名才是 "0"），`@map` 先按 index 解析、失败才按文件名 → 这里两个都试一遍。
DEFAULT_START_MAPS = ("0", "1")
#: 起始坐标（`ServerConfig.start_point` 默认值）。
DEFAULT_START_POINT = (330, 270)

# --- 服务端包号（server.rs / manual/server.rs） ---
S_CONNECTED = 0
C_KEEP_ALIVE = 2
S_CLIENT_VERSION = 1
S_NEW_ACCOUNT = 4
S_LOGIN = 7
S_LOGIN_SUCCESS = 9
S_LOGOUT_SUCCESS = 58
S_NEW_CHARACTER_SUCCESS = 11
S_START_GAME = 14
S_MAP_INFORMATION = 17
S_USER_INFORMATION = 21
S_USER_LOCATION = 23
S_OBJECT_WALK = 28
S_CHAT = 30
S_OBJECT_MONSTER = 71
S_OBJECT_ITEM = 64
S_OBJECT_GOLD = 65
S_OBJECT_DIED = 81
S_GAIN_EXPERIENCE = 85
S_NEW_MAGIC = 117
S_OBJECT_MAGIC = 120
S_OBJECT_NPC = 92
S_NPC_RESPONSE = 93
S_NEW_MAP_INFO = 18
S_GAINED_ITEM = 66
S_GAINED_QUEST_ITEM = 205
S_DURA_CHANGED = 76
S_ITEM_REPAIRED = 114
C_REPAIR_ITEM = 54
S_REPAIR_ITEM = 56
C_MOVE_ITEM = 14
S_EQUIP_ITEM = 38
S_USE_ITEM = 52
S_GAINED_GOLD = 67
S_LOSE_GOLD = 68
S_CHANGE_QUEST = 201
S_COMPLETE_QUEST = 202
S_MAP_CHANGED = 98
S_NPC_GOODS = 102
S_SELL_ITEM = 111
S_LOG_OUT_SUCCESS = 58

DIRECTIONS = ["Up", "UpRight", "Right", "DownRight", "Down", "DownLeft", "Left", "UpLeft"]

#: 新手链 BFS 走位时已知的“门未开”格（从 `resource/map/0.map` v0 的 `o+8 & 0x7F` 解析）。
#: 只用于 gate 脚本；产品侧的门交互是第 24 轮候选。
def _load_doors() -> dict[tuple[int, int], int]:
    path = pathlib.Path(__file__).with_name("doors.json")
    if not path.exists():
        return {}
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    doors = {}
    for key, value in raw.items():
        if key.startswith("_") or "," not in key:
            continue
        x, y = key.split(",", 1)
        doors[(int(x), int(y))] = int(value)
    return doors


NEWBIE_DOORS = _load_doors()


class ProtocolError(Exception):
    pass


class Reader:
    def __init__(self, data: bytes):
        self.data = data
        self.pos = 0

    def take(self, n: int) -> bytes:
        if self.pos + n > len(self.data):
            raise ProtocolError(f"负载不足：需要 {n} 字节，剩余 {len(self.data) - self.pos}")
        out = self.data[self.pos : self.pos + n]
        self.pos += n
        return out

    def u8(self) -> int:
        return self.take(1)[0]

    def u16(self) -> int:
        return struct.unpack("<H", self.take(2))[0]

    def i32(self) -> int:
        return struct.unpack("<i", self.take(4))[0]

    def u32(self) -> int:
        return struct.unpack("<I", self.take(4))[0]

    def i16(self) -> int:
        return struct.unpack("<h", self.take(2))[0]

    def i64(self) -> int:
        return struct.unpack("<q", self.take(8))[0]

    def u64(self) -> int:
        return struct.unpack("<Q", self.take(8))[0]

    def f32(self) -> float:
        return struct.unpack("<f", self.take(4))[0]

    def boolean(self) -> bool:
        return self.u8() != 0

    def string(self) -> str:
        length = 0
        shift = 0
        while True:
            byte = self.u8()
            length |= (byte & 0x7F) << shift
            if byte & 0x80 == 0:
                break
            shift += 7
            if shift >= 35:
                raise ProtocolError("非法字符串长度")
        return self.take(length).decode("utf-8", "replace")

    def point(self) -> tuple[int, int]:
        return self.i32(), self.i32()

    def stats(self) -> dict[int, int]:
        """`Stats`：`i32` 条数 → (`u8` stat, `i32` value)。"""
        out = {}
        for _ in range(self.i32()):
            stat = self.u8()
            out[stat] = self.i32()
        return out

    def awake(self) -> tuple[int, list[int]]:
        """`Awake`：`u8` 类型 → `i32` 条数 → 每项 `u8`。"""
        kind = self.u8()
        return kind, [self.u8() for _ in range(self.i32())]

    def user_item(self) -> dict:
        """整条 `UserItem`（`Shared/Data/ItemData.cs:338` 的 version=MaxValue 分支）。

        商店列表是 `Vec<UserItem>`，要逐条读完整条才能走到下一条，所以这里按
        服务端 `UserItem::decode` 的字段顺序原样复刻（返回值只留自检需要的字段）。
        """
        unique_id = struct.unpack("<Q", self.take(8))[0]
        item_index = self.i32()
        current_dura = self.u16()
        max_dura = self.u16()
        count = self.u16()
        self.i32()  # soul_bound_id
        self.u8()  # identified / cursed 位标记
        for _ in range(self.i32()):  # slots
            if not self.boolean():
                self.user_item()
        self.u16()  # gem_count
        self.stats()
        self.awake()
        self.u8()  # refined_value
        self.u8()  # refine_added
        self.i32()  # refine_success_chance
        self.i32()  # wedding_ring
        if self.boolean():  # expire_info
            self.i64()
        if self.boolean():  # rental_information
            self.string()
            self.i16()  # binding flags
            self.i64()
        self.boolean()  # is_shop_item
        if self.boolean():  # sealed_info
            self.i64()
            self.i64()
        self.boolean()  # gmmade
        return {
            "unique_id": unique_id,
            "item_index": item_index,
            "count": count,
            "current_dura": current_dura,
            "max_dura": max_dura,
        }


class Writer:
    def __init__(self):
        self.buf = bytearray()

    def u8(self, value: int) -> None:
        self.buf += struct.pack("<B", value & 0xFF)

    def u16(self, value: int) -> None:
        self.buf += struct.pack("<H", value & 0xFFFF)

    def i32(self, value: int) -> None:
        self.buf += struct.pack("<i", value)

    def u32(self, value: int) -> None:
        self.buf += struct.pack("<I", value)

    def u64(self, value: int) -> None:
        self.buf += struct.pack("<Q", value)

    def i64(self, value: int) -> None:
        self.buf += struct.pack("<q", value)

    def string(self, value: str) -> None:
        raw = value.encode("utf-8")
        length = len(raw)
        while True:
            byte = length & 0x7F
            length >>= 7
            if length:
                self.u8(byte | 0x80)
            else:
                self.u8(byte)
                break
        self.buf += raw

    def bytes_prefixed(self, value: bytes) -> None:
        self.i32(len(value))
        self.buf += value

    def raw(self) -> bytes:
        return bytes(self.buf)


class Playtest:
    def __init__(
        self,
        server: str,
        timeout: float,
        gm: str | None = None,
        newbie: bool = False,
        start_maps: tuple[str, ...] = DEFAULT_START_MAPS,
        start_point: tuple[int, int] = DEFAULT_START_POINT,
    ):
        host, _, port = server.partition(":")
        self.host = host
        self.port = int(port or "7000")
        self.timeout = timeout
        self.gm = gm
        self.newbie = newbie
        self.dr1 = False
        self.ps1 = False
        self.tp1 = False
        self.quest5 = False
        self.quest1 = False
        self.expected_quest1_exp = 50
        self.eq1 = False
        self.sell1 = False
        self.dr1_keep_moving = False
        self.dr1_taken_hits: list[tuple[float, int, int]] = []
        self.dr1_counts: collections.Counter[str] = collections.Counter()
        self.sock: socket.socket | None = None
        self.last_send = time.time()
        self.buffer = bytearray()
        self.self_object_id: int | None = None
        self.self_pos: tuple[int, int] | None = None
        self.self_dead = False
        self.death_at: float | None = None
        self.revived_at: float | None = None
        self.npcs: list[dict] = []
        self.monsters: list[dict] = []
        self.recent_packets: collections.deque[int] = collections.deque(maxlen=20)
        # DR-1 gate：完整包序 + 时间戳（证明“先发后收”）。
        self.packet_log: list[tuple[float, int]] = []
        self.dr1_hit_target: int | None = None
        self.health_history: list[tuple[float, int]] = []
        self.last_userinfo_hp: int | None = None
        self.self_hp: int | None = None
        self.chats: list[str] = []
        self.learned_spells: set[int] = set()
        self.self_mp: int | None = None
        self.self_level: int | None = None
        self.self_exp: int | None = None
        self.self_gold: int | None = None
        self.inventory_items: list[dict] = []
        self.equipment_items: list[dict] = []
        self.ground_items: list[dict] = []
        self.ground_gold: list[dict] = []
        # 被服务端拒绝的“活物/临时占位”格：cell -> 解除时间（monotonic）。
        # 静态 NPC/建筑走 blocked 永久集；怪物/玩家短 TTL，避免把会走的对象永久拉黑。
        self._dynamic_blocked: dict[tuple[int, int], float] = {}
        # 诊断限频：同一 reason 5s 内最多一条，避免死循环把日志撑到 100MB。
        self._diag_last: dict[str, float] = {}
        self._diag_suppressed: dict[str, int] = {}
        self.quest_items: list[dict] = []
        self.equipped_uids: list[int] = []
        self.equipped_slots: list[int] = []
        self.equipped_items: list[dict] = []
        self.dura_events: list[tuple[float, int, int]] = []
        self.repair_events: list[tuple[float, int, int, int]] = []
        self.gates: set[tuple[int, int]] = {
            (344, 309),
            (355, 319),
            (343, 310),
            (354, 320),
            (323, 247),
        }
        self.unexpected_map: tuple[str, tuple[int, int], str] | None = None
        self.gate_map = self._load_gates()
        self.map_file: str | None = None
        self._walkable: list[bytearray] | None = None
        self._map_size: tuple[int, int] | None = None
        self.move_ready_at = 0.0
        self.results: list[tuple[str, str, str]] = []
        # 商店：最近一次 `S.NPCGoods` 与最近一页 `S.NPCResponse` 的正文（抠按钮用）。
        self.last_goods: dict | None = None
        self.goods_seq = 0
        self.last_npc_page: list[str] = []
        # 金币变化与到手物品（买药断言用）。
        self.gold_delta = 0
        self.last_gained_items: list[dict] = []
        # `S.NewMapInfo` 里的传送点（跨图用）。
        self.movements: list[dict] = []
        self.map_npcs: list[dict] = []
        self.newmapinfo_npc_count = 0
        self.map_index: int | None = None
        self.map_size: tuple[int, int] | None = None
        # 幂等（同一服务端可重复跑）：进世界后先回起始图，起点由 CLI 覆盖。
        self.start_maps = start_maps
        self.start_point = start_point
        # 进世界时角色实际所在的图/坐标（用于诊断「上一轮把角色留在别处」）。
        self.entry_map: str | None = None
        self.entry_pos: tuple[int, int] | None = None
        self.run_started_at = time.time()
        self.gm_commands: list[str] = []
        self.kill_count = 0
        # 只统计 Hen 的击杀，防止把 Deer 混进 quest 7 的 3 只计数。
        self.hen_kill_count = 0
        # S.CompleteQuest(202) 是登录时服务端下发的“已完成任务”全集；清洁初态必须不含 7。
        self.completed_quests: set[int] = set()
        self.completed_quests_seen = False

    # ---------------- 结果输出 ----------------
    def _stamp(self) -> str:
        elapsed = int(time.time() - getattr(self, "run_started_at", time.time()))
        return f"t=+{elapsed // 60:02d}:{elapsed % 60:02d}"

    def ok(self, step: str, detail: str = "") -> None:
        self.results.append(("OK", step, detail))
        print(f"[{self._stamp()}] [OK]   {step}" + (f" —— {detail}" if detail else ""))

    def skip(self, step: str, reason: str) -> None:
        self.results.append(("SKIP", step, reason))
        print(f"[{self._stamp()}] [SKIP] {step} —— {reason}")

    def fail(self, step: str, detail: str) -> None:
        self.results.append(("FAIL", step, detail))
        print(f"[{self._stamp()}] [FAIL] {step} —— {detail}")

    # ---------------- 网络 ----------------
    def connect(self) -> None:
        self.sock = socket.create_connection((self.host, self.port), timeout=self.timeout)
        self.sock.settimeout(0.05)

    def close(self) -> None:
        if self.sock is not None:
            try:
                self.sock.close()
            finally:
                self.sock = None

    def send(self, index: int, payload: bytes) -> None:
        assert self.sock is not None
        self.last_send = time.time()
        total = 4 + len(payload)
        if total > 0xFFFF:
            raise ProtocolError(f"包 {index} 过长：{total}")
        self.sock.sendall(struct.pack("<Hh", total, index) + payload)

    def _read_more(self, deadline: float) -> bool:
        assert self.sock is not None
        if time.time() >= deadline:
            return False
        try:
            chunk = self.sock.recv(65536)
        except socket.timeout:
            return True
        if not chunk:
            raise ProtocolError("服务端关闭了连接")
        self.buffer += chunk
        return True

    def keepalive(self) -> None:
        writer = Writer()
        writer.i64(int(time.time() * 1000))
        self.send(C_KEEP_ALIVE, writer.raw())

    def next_packet(self, deadline: float) -> tuple[int, bytes] | None:
        if time.time() - self.last_send > 20:
            self.keepalive()
        while True:
            if len(self.buffer) >= 4:
                total, index = struct.unpack_from("<Hh", self.buffer, 0)
                if total < 4:
                    raise ProtocolError(f"非法帧长度 {total}")
                if len(self.buffer) >= total:
                    payload = bytes(self.buffer[4:total])
                    del self.buffer[:total]
                    if payload.startswith(b"\x1f\x8b"):
                        payload = gzip.decompress(payload)
                    return index, payload
            if not self._read_more(deadline):
                return None

    def expect(self, wanted: set[int] | int, step: str, timeout: float | None = None) -> tuple[int, bytes]:
        wanted_set = {wanted} if isinstance(wanted, int) else wanted
        deadline = time.time() + (timeout if timeout is not None else self.timeout)
        seen: list[int] = []
        while True:
            packet = self.next_packet(deadline)
            if packet is None:
                names = ", ".join(
                    {0: "S.Connected", 1: "S.ClientVersion", 4: "S.NewAccount", 7: "S.Login",
                     9: "S.LoginSuccess", 11: "S.NewCharacterSuccess", 14: "S.StartGame",
                     21: "S.UserInformation", 92: "S.ObjectNPC", 71: "S.ObjectMonster",
                     93: "S.NPCResponse"}.get(i, f"包 {i}") for i in wanted_set)
                raise ProtocolError(f"超时 {timeout or self.timeout:.0f}s：期望收到 {names}；已收到 {seen[-10:]}")
            index, payload = packet
            seen.append(index)
            self._last_index = index
            self._observe(index, payload)
            if index in wanted_set:
                return index, payload

    # ---------------- 收包旁路（记录实体） ----------------
    def _merge_npc_sources(self) -> None:
        """合并 NewMapInfo.npcs 与可见 S.ObjectNPC；不依赖 ShowOnBigMap 标志。"""
        merged: dict[int, dict] = {}
        for npc in self.map_npcs:
            key = npc.get("object_id", npc.get("id", 0))
            merged[int(key)] = npc
        for npc in self.npcs:
            key = npc.get("id", 0)
            if int(key) not in merged:
                merged[int(key)] = npc
        self.map_npcs = list(merged.values())

    def _observe(self, index: int, payload: bytes) -> None:
        self.recent_packets.append(index)
        self.packet_log.append((time.time(), index))
        try:
            if index == S_MAP_INFORMATION:
                r = Reader(payload)
                _map_index = r.i32()
                self.map_file = r.string()
            elif index == S_USER_INFORMATION:
                r = Reader(payload)
                object_id = r.u32()
                _real_id = r.u32()
                name = r.string()
                r.string()
                r.string()
                r.i32()  # name colour
                _class = r.u8()
                _gender = r.u8()
                _level = r.u16()
                location = r.point()
                _direction = r.u8()
                _hair = r.u8()
                hp = r.i32()
                mp = r.i32()
                self.self_hp = hp
                self.self_mp = mp
                self.last_userinfo_hp = hp
                self.self_level = _level
                self.self_exp = r.i64()
                _max_experience = r.i64()
                _level_effects = r.u16()
                _has_hero = r.boolean()
                _hero_behaviour = r.u8()

                def optional_items():
                    if not r.boolean():
                        return []
                    count = r.i32()
                    return [r.user_item() if r.boolean() else None for _ in range(count)]

                inventory = [item for item in optional_items() if item]
                if inventory:
                    self.inventory_items = inventory
                equipment = [item for item in optional_items() if item]
                if equipment:
                    self.equipment_items = equipment
                _quest_items = optional_items()
                self.self_gold = r.u32()
                _credit = r.u32()
                _expanded = r.boolean()
                _has_storage_password = r.boolean()
                _require_storage_password = r.boolean()
                _storage_password_last_set = r.i64()
                _expanded_storage_expiry = r.i64()
                magic_count = r.i32()
                for _ in range(magic_count):
                    _magic_name = r.string()
                    spell = r.u8()
                    r.u8()  # base_cost
                    r.u8()  # level_cost
                    r.u8()  # icon
                    r.u8()  # level1
                    r.u8()  # level2
                    r.u8()  # level3
                    r.u16()  # need1
                    r.u16()  # need2
                    r.u16()  # need3
                    r.u8()  # level
                    r.u8()  # key
                    r.u16()  # experience
                    self.learned_spells.add(spell)
                if self.self_object_id is None:
                    self.self_object_id = object_id
                    self.self_pos = location
                    self._player_name = name
            elif index == S_OBJECT_NPC:
                r = Reader(payload)
                object_id = r.u32()
                name = r.string()
                r.i32()  # name colour
                r.u16()  # image
                r.i32()  # colour
                location = r.point()
                r.u8()  # direction
                quest_count = r.i32()
                quest_ids = [r.i32() for _ in range(max(0, min(quest_count, 64)))]
                self.npcs.append(
                    {"id": object_id, "name": name, "pos": location, "quest_ids": quest_ids}
                )
                self._merge_npc_sources()
            elif index == S_OBJECT_MONSTER:
                r = Reader(payload)
                object_id = r.u32()
                name = r.string()
                r.i32()  # name colour
                location = r.point()
                if not any(m["id"] == object_id for m in self.monsters):
                    self.monsters.append(
                        {
                            "id": object_id,
                            "name": name,
                            "pos": location,
                            "last_pos": location,
                            "updated_at": time.time(),
                            "dead": False,
                        }
                    )
                    if __import__("os").environ.get("PT_DEBUG"):
                        print(f"  [怪] {name} #{object_id} @ {location}")
            elif index == S_GAIN_EXPERIENCE:
                amount = Reader(payload).u32()
                self.self_exp = (self.self_exp or 0) + amount
            elif index == S_OBJECT_GOLD:  # 65：地面金币
                r = Reader(payload)
                object_id = r.u32()
                gold = r.u32()
                location = r.point()
                self.ground_gold = [g for g in self.ground_gold if g["id"] != object_id]
                self.ground_gold.append({"id": object_id, "gold": gold, "pos": location})
                print(f"  [地面金币] id={object_id} amount={gold} pos={location}")
            elif index == S_OBJECT_ITEM:  # 64：地面物品
                r = Reader(payload)
                object_id = r.u32()
                name = r.string()
                r.i32()  # name colour
                location = r.point()
                image = r.u16()
                grade = r.u8()
                self.ground_items = [item for item in self.ground_items if item["id"] != object_id]
                self.ground_items.append(
                    {
                        "id": object_id,
                        "name": name,
                        "pos": location,
                        "image": image,
                        "grade": grade,
                    }
                )
            elif index == 26:  # S.ObjectRemove
                r = Reader(payload)
                object_id = r.u32()
                self.ground_items = [item for item in self.ground_items if item["id"] != object_id]
                self.monsters = [m for m in self.monsters if m["id"] != object_id]
            elif index == S_OBJECT_DIED:
                r = Reader(payload)
                object_id = r.u32()
                for monster in self.monsters:
                    if monster["id"] == object_id:
                        monster["dead"] = True
                        if any(key in monster["name"].lower() for key in self.PREY_KEYWORDS):
                            self.kill_count += 1
                        if "hen" in monster["name"].lower():
                            self.hen_kill_count += 1
            elif index == S_COMPLETE_QUEST:
                r = Reader(payload)
                count = r.i32()
                self.completed_quests = {r.i32() for _ in range(max(0, min(count, 256)))}
                self.completed_quests_seen = True
            elif index == S_CHAT:
                r = Reader(payload)
                message = r.string()
                self.chats.append(message)
                print(f"  [聊天] {message}")
                # GM `@map` 的回复带了落点（`Teleported to <map> (x, y).`）：
                # 同图传送时服务端不一定发 `S.MapChanged`，直接按回复同步坐标最可靠。
                match = re.match(r"Teleported to (\S+) \((-?\d+), (-?\d+)\)\.", message)
                if match:
                    self.map_file = match.group(1)
                    self.self_pos = (int(match.group(2)), int(match.group(3)))
            elif index == S_NEW_MAGIC:
                r = Reader(payload)
                _name = r.string()
                spell = r.u8()
                self.learned_spells.add(spell)
                print(f"  [技能] 学会 {_name} (#{spell})")
            elif index == 77:  # S.HealthChanged
                r = Reader(payload)
                hp = r.i32()
                mp = r.i32()
                self.self_hp = hp
                self.self_mp = mp
                self.health_history.append((time.time(), hp))
            elif index == S_OBJECT_ATTACK:
                r = Reader(payload)
                object_id = r.u32()
                r.point()
                r.u8()  # direction
                r.u8()  # spell
                r.u8()  # level
                r.u8()  # type
                if object_id == self.self_object_id:
                    self.dr1_counts["self_attack"] += 1
                else:
                    self.dr1_counts["monster_attack"] += 1
                    if object_id == 344:
                        self.dr1_counts["oma344_attack"] += 1
            elif index == S_OBJECT_STRUCK:
                r = Reader(payload)
                object_id = r.u32()
                attacker_id = r.u32()
                r.point()
                r.u8()  # direction
                if attacker_id == self.self_object_id:
                    print(f"  [DR-1] 命中 target={object_id}（S.ObjectStruck 74）")
                    self.dr1_hit_target = object_id
                if object_id == self.self_object_id:
                    print(f"  [DR-1] 我被命中 attacker={attacker_id}（S.ObjectStruck 74）")
                    self.dr1_taken_hits.append((time.time(), attacker_id, 0))
            elif index == S_DAMAGE_INDICATOR:
                r = Reader(payload)
                damage = r.i32()
                _type = r.u8()
                object_id = r.u32()
                if object_id != self.self_object_id:
                    print(f"  [DR-1] 伤害数字 target={object_id} dmg={damage}（75）")
                    if damage > 0:
                        self.dr1_hit_target = object_id
            elif index == S_DEATH:
                self.self_dead = True
                self.death_at = time.time()
                print(f"  [DR-1] 收到 S.Death @ {self.self_pos} t={self.death_at:.3f} hp={self.self_hp}")
            elif index == S_REVIVED:
                self.self_dead = False
                self.revived_at = time.time()
                print(f"  [DR-1] 收到 S.Revived @ {self.self_pos} t={self.revived_at:.3f} hp={self.self_hp}")
            elif index == 98:  # S.MapChanged
                # 字段顺序照 `Shared/ServerPackets.cs:2979-3025`：map_index, file_name,
                # title, mini_map(u16), big_map(u16), **lights(u8)**, location, direction,
                # map_dark_light, music(u16), weather(u16)。
                old_pos = self.self_pos
                old_map = self.map_file
                r = Reader(payload)
                map_index = r.i32()
                self.map_file = r.string()
                _title = r.string()
                _mini = r.u16()
                _big = r.u16()
                _lights = r.u8()
                self.self_pos = r.point()
                _direction = r.u8()
                self.map_index = map_index
                self._walkable = None
                self.move_ready_at = time.time() + 2.1
                self.npcs.clear()
                self.monsters.clear()
                print(f"  [地图] 切换地图 {old_map}@{old_pos} -> {self.map_file}@{self.self_pos}")
                if old_map == "0" and self.map_file != "0" and old_pos is not None:
                    self.gates.add(old_pos)
                    self.unexpected_map = (old_map, old_pos, self.map_file)
            elif index == S_NEW_MAP_INFO:  # 18：进图详情（含传送点）
                r = Reader(payload)
                map_index = r.i32()
                r.string()  # title
                width = r.i32()
                height = r.i32()
                r.i32()  # big_map
                count = r.i32()
                movements = []
                for _ in range(count):
                    destination = r.i32()
                    title = r.string()
                    location = r.point()
                    icon = r.i32()
                    movements.append(
                        {
                            "destination": destination,
                            "title": title,
                            "pos": location,
                            "icon": icon,
                        }
                    )
                npc_count = r.i32()
                map_npcs = []
                for _ in range(npc_count):
                    index = r.i32()
                    file_name = r.string()
                    name = r.string()
                    npc_map_index = r.i32()
                    location = r.point()
                    image = r.u16()
                    rate = r.u16()
                    show_on_big_map = r.boolean()
                    big_map_icon = r.i32()
                    object_id = r.u32()
                    icon = r.i32()
                    can_teleport_to = r.boolean()
                    map_npcs.append(
                        {
                            "object_id": object_id,
                            "name": name,
                            "pos": location,
                            "file": file_name,
                            "index": index,
                            "npc_map_index": npc_map_index,
                            "image": image,
                            "rate": rate,
                            "show_on_big_map": show_on_big_map,
                            "big_map_icon": big_map_icon,
                            "icon": icon,
                            "can_teleport_to": can_teleport_to,
                        }
                    )
                self.map_npcs = map_npcs
                self.newmapinfo_npc_count = npc_count
                self.movements = movements
                self.map_size = (width, height)
                self._merge_npc_sources()
                print(
                    f"  [地图] 进图详情 map#{map_index} {width}x{height}，"
                    f"NPC {npc_count} 个，传送点 {count} 个"
                )
                if __import__("os").environ.get("PT_DEBUG"):
                    for npc in map_npcs[:60]:
                        print(f"    [NPC] #{npc['index']} {npc['name']} @ {npc['pos']}")
                    for movement in movements:
                        print(f"    [传送点] -> #{movement['destination']} @ {movement['pos']}")
            elif index == S_NPC_RESPONSE:  # 93：对话正文（下面用它抠按钮）
                r = Reader(payload)
                count = r.i32()
                self.last_npc_page = [r.string() for _ in range(count)]
            elif index == S_NPC_GOODS:  # 102：商店货架
                r = Reader(payload)
                count = r.i32()
                goods = [r.user_item() for _ in range(count)]
                rate = r.f32()
                panel = r.u8()
                hide = r.boolean()
                self.goods_seq += 1
                self.last_goods = {
                    "items": goods,
                    "rate": rate,
                    "panel": panel,
                    "hide_added_stats": hide,
                }
                listed = ", ".join(
                    f"#{item['item_index']}x{item['count']}" for item in goods[:6]
                )
                print(f"  [商店] 货架 {count} 项（{listed}），价格倍率 {rate:.2f}")
            elif index == S_DURA_CHANGED:  # 76：耐久下降
                r = Reader(payload)
                unique_id = r.u64()
                current = r.u16()
                self.dura_events.append((time.time(), unique_id, current))
                for equipped in self.equipped_items:
                    if equipped["unique_id"] == unique_id:
                        equipped["current_dura"] = current
                for inv in self.inventory_items:
                    if inv["unique_id"] == unique_id:
                        inv["current_dura"] = current
                print(f"  [耐久] unique_id={unique_id} current={current}（S.DuraChanged 76）")
            elif index == S_REPAIR_ITEM:  # 56：C.RepairItem 的先行回显
                r = Reader(payload)
                unique_id = r.u64()
                print(f"  [修理] S.RepairItem 回显 unique_id={unique_id}（56）")
            elif index == S_ITEM_REPAIRED:  # 114：修理完成
                r = Reader(payload)
                unique_id = r.u64()
                max_dura = r.u16()
                current = r.u16()
                self.repair_events.append((time.time(), unique_id, max_dura, current))
                print(f"  [修理] unique_id={unique_id} max={max_dura} current={current}（S.ItemRepaired 114）")
            elif index == S_GAINED_ITEM:  # 66：到手物品
                r = Reader(payload)
                self.last_gained_items.append(r.user_item())
            elif index == S_GAINED_QUEST_ITEM:  # 205：任务物品栏（Crystal GainQuestItem）
                r = Reader(payload)
                item = r.user_item()
                self.quest_items.append(item)
                print(f"  [任务物品] #{item['item_index']}x{item.get('count')} unique_id={item['unique_id']}")
            elif index == S_LOSE_GOLD:  # 68：金币减少
                r = Reader(payload)
                gold = r.u32()
                self.gold_delta -= gold
                print(f"  [金币] -{gold}（累计 {self.gold_delta:+d}）")
            elif index == S_GAINED_GOLD:  # 67：金币增加
                r = Reader(payload)
                gold = r.u32()
                self.gold_delta += gold
                self.self_gold = (self.self_gold or 0) + gold
                print(f"  [金币] +{gold}（累计 {self.gold_delta:+d}，余额 {self.self_gold}）")
            elif index == S_USER_LOCATION:
                r = Reader(payload)
                self.self_pos = r.point()
                _direction = r.u8()
            elif index == S_OBJECT_WALK:
                r = Reader(payload)
                object_id = r.u32()
                location = r.point()
                direction = r.u8()
                _ = direction
                # 自己移动时服务端只发 S.UserLocation，ObjectWalk 是广播给别人看的。
                for monster in self.monsters:
                    if monster["id"] == object_id:
                        monster["last_pos"] = monster.get("pos")
                        monster["pos"] = location
                        monster["updated_at"] = time.time()
        except (ProtocolError, struct.error):
            pass

    # ---------------- 动作 ----------------
    def wait_for(self, wanted: set[int] | int, step: str, timeout: float | None = None):
        return self.expect(wanted, step, timeout)

    def diagnostics(self, reason: str) -> None:
        now = time.time()
        last = self._diag_last.get(reason)
        if last is not None and now - last < 5.0:
            self._diag_suppressed[reason] = self._diag_suppressed.get(reason, 0) + 1
            return
        suppressed = self._diag_suppressed.pop(reason, 0)
        self._diag_last[reason] = now
        if suppressed:
            print(f"  [诊断] （上一条相同诊断已省略 {suppressed} 次）")
        print(f"  [诊断] {reason}")
        print(f"  [诊断] 最近包索引: {list(self.recent_packets)}")
        print(f"  [诊断] UserInformation.hp={self.last_userinfo_hp} 当前 hp={self.self_hp}")
        print(f"  [诊断] HealthChanged 序列: {self.health_history[-10:]}")

    def _load_map(self) -> None:
        if self._walkable is not None or not self.map_file:
            return
        root = pathlib.Path(__file__).resolve().parents[2]
        # 优先用服务端同源导出的位图（width u32 + height u32 + 列优先位图，1=可走）。
        bin_path = root / "target" / f"mapgrid-{self.map_file}.bin"
        if bin_path.exists():
            raw = bin_path.read_bytes()
            width = int.from_bytes(raw[0:4], "little")
            height = int.from_bytes(raw[4:8], "little")
            bits = raw[8:]
            grid = [bytearray(width) for _ in range(height)]
            for x in range(width):
                for y in range(height):
                    bit = x * height + y
                    if bit // 8 < len(bits) and bits[bit // 8] & (1 << (bit % 8)):
                        grid[y][x] = 1
            self._walkable = grid
            self._map_size = (width, height)
            print(f"  [地图] 已加载 {bin_path.name} {width}x{height}")
            return
        path = root / "resource" / "map" / f"{self.map_file}.map"
        if not path.exists():
            return
        data = path.read_bytes()
        width = int.from_bytes(data[0:2], "little")
        height = int.from_bytes(data[2:4], "little")
        header, stride = 52, 12
        grid = [bytearray(width) for _ in range(height)]
        for y in range(height):
            row = grid[y]
            for x in range(width):
                offset = header + (x * height + y) * stride
                if offset + 6 > len(data):
                    continue
                w0 = int.from_bytes(data[offset:offset + 2], "little")
                w2 = int.from_bytes(data[offset + 2:offset + 4], "little")
                w4 = int.from_bytes(data[offset + 4:offset + 6], "little")
                if not (w0 & 0x8000 or w2 & 0x8000 or w4 & 0x8000):
                    row[x] = 1
        self._walkable = grid
        self._map_size = (width, height)
        print(f"  [地图] 已加载 {path.name} {width}x{height}")

    def _is_walkable(self, point: tuple[int, int]) -> bool:
        if self._walkable is None:
            self._load_map()
        if self._walkable is None:
            return True
        x, y = point
        if y < 0 or x < 0 or y >= len(self._walkable):
            return False
        row = self._walkable[y]
        return x < len(row) and row[x] == 1

    def _bfs_path(
        self, target: tuple[int, int], blocked: set[tuple[int, int]] | None = None
    ) -> list[tuple[int, int]] | None:
        self._load_map()
        if self._walkable is None or self.self_pos is None:
            return None
        start = self.self_pos
        goals = {
            (target[0] + dx, target[1] + dy)
            for dx, dy in ((0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1))
        }
        blocked = blocked or set()
        goals = {cell for cell in goals if self._is_walkable(cell) and cell not in blocked}
        if not goals:
            if __import__("os").environ.get("PT_DEBUG"):
                print(f"  [BFS] 无可行邻格 target={target} self={start} blocked={len(blocked)}")
            return None
        from collections import deque

        queue = deque([start])
        previous: dict[tuple[int, int], tuple[int, int] | None] = {start: None}
        found = None
        while queue:
            current = queue.popleft()
            if current in goals:
                found = current
                break
            for dx, dy in ((0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)):
                nxt = (current[0] + dx, current[1] + dy)
                if nxt in previous or not self._is_walkable(nxt) or nxt in blocked:
                    continue
                # 斜向不能穿墙角（服务端会拒绝并回拉）。
                if dx != 0 and dy != 0:
                    if not self._is_walkable((current[0] + dx, current[1])) or not self._is_walkable(
                        (current[0], current[1] + dy)
                    ):
                        continue
                previous[nxt] = current
                queue.append(nxt)
        if found is None:
            if __import__("os").environ.get("PT_DEBUG"):
                print(f"  [BFS] 搜不到 target={target} self={start} blocked={len(blocked)} goals={sorted(goals)[:8]}")
            return None
        path = []
        node = found
        while node is not None and node != start:
            path.append(node)
            node = previous[node]
        path.reverse()
        return path

    def chat(self, text: str) -> None:
        """发一条聊天（`C.Chat{Message, LinkedItems}`；`LinkedItems` 是 count 前缀的数组）。"""
        writer = Writer()
        writer.string(text)
        writer.i32(0)  # linked_items 为空
        self.send(C_CHAT, writer.raw())

    def goto(
        self,
        target: tuple[int, int],
        max_steps: int = 160,
        blocked: set[tuple[int, int]] | None = None,
    ) -> bool:
        """走到目标旁一格：近处用 BFS（`walk_towards`，绕开建筑），远处先贪心再 BFS 兜底。"""
        if self._distance(target) <= 1:
            return True
        if blocked is None:
            blocked = self._walker_blocked()
        if self._distance(target) <= 40:
            return self.walk_towards(target, max_steps=max_steps, blocked=blocked)
        if self.walk_to(target, max_steps=max_steps // 2):
            return True
        return self.walk_towards(target, max_steps=max_steps, blocked=blocked)

    def _stand_next_to(self, npc: dict) -> bool:
        """站到 NPC 相邻格：有 `--gm` 用 `@map` 定位（脚本步行器在比奇建筑群里不稳），否则走路。"""
        if self._distance(npc["pos"]) <= 1:
            return True
        if self.gm:
            # `@map` 的解析顺序是「先按地图编号、再按文件名」，而比奇在库里的编号是
            # `S.NewMapInfo.map_index`（本数据集是 1、文件名是 "0"），所以这里用编号。
            map_id = self.map_index if self.map_index is not None else 1
            for dx, dy in ((0, 1), (1, 0), (0, -1), (-1, 0)):
                cell = (npc["pos"][0] + dx, npc["pos"][1] + dy)
                self.chat(f"@map {map_id} {cell[0]} {cell[1]}")
                self._drain(0.8)
                if self._distance(npc["pos"]) <= 1:
                    return True
            self.diagnostics(f"GM @map 也没能站到 {npc['name']} 旁边")
            return False
        return self.walk_towards(npc["pos"], max_steps=160, blocked=self._walker_blocked())

    # ---------------- 幂等：回到起始图 ----------------
    def return_to_start(self, char_index: int) -> bool:
        """把角色拉回起始图/起点，保证脚本可以在**同一服务端上重复运行**。

        为什么需要：跨图传送那一步会把角色留在目标图（本数据集是 `D011`），下一次运行
        进世界后视野里一个 NPC 都没有 → `[FAIL] 走到 NPC`。

        为什么还要「下线再进」：GM `@map` 走的是 `World::place_player`，它只改坐标、
        **不发** `S.MapChanged`/视野对象（服务端在 GM 注释里写明是测试助手路径），
        所以挪完位置后必须重新 `StartGame` 一遍，才能拿到新图的
        `MapInformation`/`NewMapInfo`/`S.ObjectNPC`。非 GM 模式每次跑都是新账号新角色，
        天然幂等，直接返回。
        """
        if not self.gm:
            return True
        x, y = self.start_point
        attempts = []
        landed = False
        for map_name in self.start_maps:
            before = len(self.chats)
            self.chat(f"@map {map_name} {x} {y}")
            receipt = self._wait_gm_reply(before)
            attempts.append(f"@map {map_name} {x} {y} → {receipt}")
            if self.self_pos == (x, y):
                landed = True
                break
        if not landed:
            self.fail(
                "回到起始图",
                f"GM `@map` 没能把角色放回起点 {(x, y)}；尝试记录：{attempts}；"
                f"当前 {self.map_file} @ {self.self_pos}"
                "（用 `--start-map/--start-point` 指定本数据集的起始图）",
            )
            return False
        # 下线再进：重拿地图信息与视野内的 NPC/怪物（`@map` 自己不发这些包）。
        writer = Writer()
        self.send(C_LOGOUT, writer.raw())
        try:
            self.wait_for(S_LOG_OUT_SUCCESS, "下线", 5)
        except ProtocolError:
            self.diagnostics("下线没收到 S.LogOutSuccess")
        self.self_object_id = None
        self.self_pos = None
        self.npcs.clear()
        self.monsters.clear()
        self.map_npcs.clear()
        self.movements.clear()
        writer = Writer()
        writer.i32(char_index)
        self.send(C_START_GAME, writer.raw())
        try:
            _, payload = self.wait_for(S_START_GAME, "重新进入游戏")
        except ProtocolError as error:
            self.fail("回到起始图", f"重新进入游戏失败：{error}")
            return False
        result = Reader(payload).u8()
        if result != 4:
            self.fail("回到起始图", f"重新 StartGame 被拒：Result={result}（期望 4）")
            return False
        # 收集 Login 流（UserInformation + 至少一条 NPC/怪物）。
        deadline = time.time() + self.timeout
        while time.time() < deadline and (self.self_object_id is None or not (self.npcs or self.monsters)):
            packet = self.next_packet(deadline)
            if packet is None:
                break
            self._observe(*packet)
        if self.self_object_id is None:
            self.fail(
                "回到起始图",
                f"重新进世界没拿到 S.UserInformation；最近包 {list(self.recent_packets)}",
            )
            return False
        self.ok(
            "回到起始图",
            f"{self.map_file} @ {self.self_pos}，视野 NPC {len(self.npcs)} 个"
            f"（本图 NPC {len(self.map_npcs)} 个）",
        )
        return True

    #: GM `@map` 的三种回执前缀（`Teleported to` / `Map not found` / `Usage:` / `Cannot enter map`）。
    GM_REPLY_PREFIXES = ("Teleported to", "Map not found", "Usage:", "Cannot enter map")

    def _wait_gm_reply(self, since: int, timeout: float | None = None) -> str:
        """等到**GM 回执**为止（登录欢迎语之类的普通聊天不算），返回回执文本。"""
        deadline = time.time() + (timeout if timeout is not None else self.timeout)
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            self._observe(*packet)
            for text in self.chats[since:]:
                if text.startswith(self.GM_REPLY_PREFIXES):
                    return text
        return "（无回执）"

    def _walkable_at(self, pos: tuple[int, int] | None) -> bool:
        """落点是否在本地地形图里可站立（没有地形文件时不判定）。"""
        if pos is None:
            return False
        self._load_map()
        if self._walkable is None:
            return True
        x, y = pos
        if y < 0 or y >= len(self._walkable):
            return False
        row = self._walkable[y]
        if x < 0 or x >= len(row):
            return False
        return row[x] != 0

    def walk_to(self, target: tuple[int, int], max_steps: int = 200) -> bool:
        """朝目标贪心走位（八方向按「更接近目标」排序，撞墙就换方向）。

        不走本地地形图也不跑全图 BFS：**服务端才是权威**，这里直接试走，被拒就把那一格
        记进本次的 `blocked`，因此对「地形文件与服务端口径不一致」也免疫。
        """
        blocked: set[tuple[int, int]] = set()
        blocked |= {tuple(n["pos"]) for n in self.npcs}
        blocked |= {tuple(m["pos"]) for m in self.monsters if not m["dead"]}
        directions = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)]
        for _ in range(max_steps):
            if self.self_pos is None:
                return False
            if self._distance(target) <= 1:
                return True
            options = []
            for direction, (dx, dy) in enumerate(directions):
                cell = (self.self_pos[0] + dx, self.self_pos[1] + dy)
                if cell in blocked:
                    continue
                options.append((max(abs(cell[0] - target[0]), abs(cell[1] - target[1])), direction, cell))
            if not options:
                self.diagnostics(f"贪心走位无路可试 self={self.self_pos} target={target}")
                return False
            options.sort()
            before = self.self_pos
            moved = False
            for _, direction, cell in options:
                writer = Writer()
                writer.u8(direction)
                self.send(C_WALK, writer.raw())
                deadline = time.time() + 1.2
                while time.time() < deadline:
                    packet = self.next_packet(deadline)
                    if packet is None:
                        break
                    index, payload = packet
                    self._observe(index, payload)
                    if index == S_MAP_CHANGED:
                        return True
                    if index == S_USER_LOCATION:
                        moved = self.self_pos != before
                        break
                if moved:
                    break
                # 这一格走不进去：本次行走内不再尝试。
                blocked.add(cell)
            if not moved:
                if __import__("os").environ.get("PT_DEBUG"):
                    print(f"  [调试] 八个方向都被拒 self={self.self_pos} target={target}")
                return False
        return self._distance(target) <= 1

    # ---------------- 商店买药（去掉 SKIP） ----------------
    @staticmethod
    def _button_keys(page: list[str]) -> list[str]:
        """从 `#SAY` 渲染文本里抠出按钮 key：`<买药/@BuySell>` → `@BuySell`。"""
        keys: list[str] = []
        for line in page:
            for match in re.finditer(r"/@([A-Za-z0-9_]+)", line):
                key = f"@{match.group(1)}"
                if key.lower() != "@exit" and key not in keys:
                    keys.append(key)
        return keys

    def call_npc(self, npc: dict, key: str, timeout: float = 3.0) -> int | None:
        """点 NPC / 点按钮：发 `C.CallNPC`，返回期间收到的第一个关键包索引。"""
        writer = Writer()
        writer.u32(npc["id"])
        writer.string(key)
        if __import__("os").environ.get("PT_DEBUG"):
            print(f"  [调试] C.CallNPC id={npc['id']} key={key} pos={npc['pos']} self={self.self_pos}")
        self.send(C_CALL_NPC, writer.raw())
        deadline = time.time() + timeout
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            index, payload = packet
            self._observe(index, payload)
            if index in (S_NPC_GOODS, S_NPC_RESPONSE):
                return index
        return None

    #: 名字里带这些关键字的就是「会开商店页」的 NPC（比奇：Alchemist_Samuel / Merchant_* / InnKeeper_*）。
    SHOP_NPC_KEYWORDS = (
        "alchemist",
        "merchant",
        "grocer",
        "drug",
        "potionshop",
        "innkeeper",
        "store",
        "杂货",
        "药",
    )

    def _shop_candidates(self) -> list[dict]:
        """会开商店的 NPC：先按名字关键字筛，找不到再退回「全部 NPC 按距离」。"""
        pool = self.map_npcs or self.npcs
        candidates = [
            npc
            for npc in pool
            if any(word in npc["name"].lower() for word in self.SHOP_NPC_KEYWORDS)
        ]
        if not candidates:
            candidates = pool
        return sorted(candidates, key=lambda npc: self._distance(npc["pos"]))

    def buy_potion(self) -> bool:
        """走到商店 NPC → 开商店页 → 买 1 个药 → 断言 `S.GainedItem` + 金币减少。

        货架（`S.NPCGoods` 102）只带 `UserItem`（模板号/数量/耐久），**没有价格字段**
        （Crystal 的 `ClientGoodsInfo.Price` 本作未建模），所以「最便宜」按已知的初级药水
        模板号优先（`POTION_ITEM_INDEX`），其余货架取第一项；实际成交价用 `S.LoseGold` 反查。
        """
        candidates = self._shop_candidates()
        if not candidates:
            self.fail("买药", "服务端没下发任何 NPC（`S.NewMapInfo`/`S.ObjectNPC` 都为空）")
            return False
        for npc in candidates[:6]:
            if not self._stand_next_to(npc):
                self.diagnostics(f"走不到商店 NPC {npc['name']} @ {npc['pos']}")
                continue
            spawn = dict(npc, id=npc.get("object_id", npc.get("id")))
            dx = npc["pos"][0] - self.self_pos[0]
            dy = npc["pos"][1] - self.self_pos[1]
            writer = Writer()
            writer.u8(self._direction(dx, dy))
            self.send(C_TURN, writer.raw())
            time.sleep(0.05)
            self.last_goods = None
            if self.call_npc(spawn, "Main", timeout=4) is None:
                self.diagnostics(f"{npc['name']} 的 @Main 没回应")
                continue
            if self.gm and self.gold_delta == 0:
                # 新角色 0 金币：GM 先补一点，买东西才走得到 `S.LoseGold` 那一步。
                self.chat("@gold 5000")
                self._drain(0.5)
            # 在对话页里做一次广度优先搜索（最多 3 层、12 次点击）找商店页：
            # 交易类 NPC 的商店经常藏在 `@trade` → `@BUYSELL` 这样的二级页面后面。
            queue: list[tuple[str, int]] = [("Main", 0)]
            seen: set[str] = set()
            clicks = 0
            while queue and clicks < 12:
                key, depth = queue.pop(0)
                if key in seen:
                    continue
                seen.add(key)
                clicks += 1
                response = self.call_npc(spawn, key, timeout=3)
                # `S.NPCResponse` 之后紧跟的 `S.NPCGoods`（商店页）会被 `call_npc` 留在缓冲里，
                # 这里补收一小段，否则会以为「没进商店」。
                self._drain(0.35)
                if __import__("os").environ.get("PT_DEBUG"):
                    print(
                        f"  [调试] {npc['name']} 点 {key} → 包 {response}；"
                        f"货架 {'有' if self.last_goods else '无'}"
                    )
                if self.last_goods is None:
                    if response == S_NPC_RESPONSE and depth < 3:
                        for nxt in self._button_keys(self.last_npc_page):
                            if nxt not in seen:
                                queue.append((nxt, depth + 1))
                    continue
                goods = self.last_goods
                if not goods or not goods["items"]:
                    continue
                items = goods["items"]
                pick = next(
                    (item for item in items if item["item_index"] == POTION_ITEM_INDEX),
                    items[0],
                )
                gold_before = self.gold_delta
                self.last_gained_items.clear()
                writer = Writer()
                writer.u64(pick["unique_id"])
                writer.u16(1)
                writer.u8(0)  # PanelType.Buy
                self.send(C_BUY_ITEM, writer.raw())
                deadline = time.time() + self.timeout
                bought = False
                while time.time() < deadline:
                    packet = self.next_packet(deadline)
                    if packet is None:
                        break
                    index, payload = packet
                    self._observe(index, payload)
                    if index == S_GAINED_ITEM:
                        bought = True
                        break
                spent = gold_before - self.gold_delta
                if not bought:
                    self.fail(
                        "买药",
                        f"{npc['name']} 的 {key} 页买模板 #{pick['item_index']}，"
                        f"{self.timeout:.0f}s 内没收到 S.GainedItem（66）；"
                        f"最近包 {list(self.recent_packets)}",
                    )
                    return False
                if spent <= 0:
                    self.fail(
                        "买药",
                        f"收到 S.GainedItem 但金币没减少（delta={self.gold_delta:+d}）",
                    )
                    return False
                got = self.last_gained_items[-1] if self.last_gained_items else pick
                self.ok(
                    "买药",
                    f"{npc['name']}（{key} 页）买模板 #{got['item_index']} x{got['count']}，"
                    f"花费 {spent} 金币，货架倍率 {goods['rate']:.2f}",
                )
                return True
        self.fail(
            "买药",
            f"试了 {min(len(candidates), 6)} 个商店 NPC 都没开出商店页"
            f"（最近包 {list(self.recent_packets)}）",
        )
        return False

    # ---------------- 自然传送点（来自 GM `@mapinfo`） ----------------
    #: `@mapinfo` 里传送点行的格式（见 `crates/mir-server/src/gm.rs::movement_lines`）：
    #: `    (326,288) -> 0108(#4) (8,12) dist=18 [bigmap]`
    MOVEMENT_LINE = re.compile(
        r"^\(\s*(?P<x>-?\d+)\s*,\s*(?P<y>-?\d+)\s*\)\s*->\s*(?P<map>\S+?)\(#(?P<index>-?\d+)\)\s*"
        r"\((?P<tx>-?\d+)\s*,\s*(?P<ty>-?\d+)\)\s*dist=(?P<dist>\d+)(?P<marks>.*)$"
    )

    def nearby_movements(self) -> list[dict]:
        """问 GM `@mapinfo` 要「附近传送点」列表（含 `ShowOnBigMap = false` 的那些）。

        为什么要走 GM：Crystal 的 `S.NewMapInfo` 只带 `ShowOnBigMap` 的条目
        （`Server/MirObjects/PlayerObject.cs:1009-1056`），脚本按客户端协议说话就看不到
        「走进城门就换图」的格子。`@mapinfo` 是开发者旁路：**只观察数据**，换图仍然靠
        玩家自己走进去触发（服务端 `maps.rs::check_movement`），不改变客户端语义。
        """
        before = len(self.chats)
        self.chat("@mapinfo")
        deadline = time.time() + self.timeout
        lines: list[str] = []
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            self._observe(*packet)
            lines = self.chats[before:]
            if any("movements:" in line for line in lines) and len(lines) > 0:
                # 名单是逐条发的，等最后一行的下一小段安静期再收尾。
                self._drain(0.25)
                lines = self.chats[before:]
                break
        movements = []
        for line in lines:
            match = self.MOVEMENT_LINE.match(line.strip())
            if not match:
                continue
            marks = match.group("marks")
            movements.append(
                {
                    "pos": (int(match.group("x")), int(match.group("y"))),
                    "map": match.group("map"),
                    "index": int(match.group("index")),
                    "target": (int(match.group("tx")), int(match.group("ty"))),
                    "dist": int(match.group("dist")),
                    # 只有「走进去就换图」的点能当自然传送用。
                    "walkable": not ("[hole]" in marks or "[move]" in marks),
                    "big_map": "[bigmap]" in marks,
                }
            )
        movements.sort(key=lambda movement: movement["dist"])
        return movements

    # ---------------- 跨图传送（去掉 SKIP） ----------------
    #: 自然传送点（`S.NewMapInfo.movements`）的步行预算：超了就不硬走（700×700 的比奇太小、
    #: 传送点又在角落，硬走要 2 分钟以上）。
    NATURAL_MOVEMENT_BUDGET = 60

    def cross_map(self) -> bool:
        """跨图传送：① 自然传送点 → ② 传送 NPC → ③ GM `@map` 兜底（并在输出里说明原因）。"""
        if self.self_pos is None:
            self.fail("跨图传送", "还没进世界")
            return False
        start_map, start_pos = self.map_file, self.self_pos
        # ① 自然传送点：GM `@mapinfo` 给出的**全部**传送点（含非 ShowOnBigMap），
        #    挑最近的可走点走过去；这一步失败才降级。
        natural = [movement for movement in self.nearby_movements() if movement["walkable"]]
        if natural:
            print(
                f"  [传送点] @mapinfo 给了 {len(natural)} 个可走传送点，"
                f"最近 {natural[0]['pos']} → {natural[0]['map']} {natural[0]['target']}"
                f"（{natural[0]['dist']} 格）"
            )
        for movement in natural[:3]:
            if self._distance(movement["pos"]) > self.NATURAL_MOVEMENT_BUDGET:
                continue
            if self._take_movement(movement, start_map, start_pos):
                return True
        # 客户端协议里能看到的大地图传送点（`S.NewMapInfo.movements`）作为次选。
        nearest = sorted(
            (
                {
                    "pos": mv["pos"],
                    "index": mv["destination"],
                    "map": None,
                    "target": None,
                    "dist": self._distance(mv["pos"]),
                    "walkable": True,
                }
                for mv in self.movements
            ),
            key=lambda mv: mv["dist"],
        )
        if nearest and nearest[0]["dist"] <= self.NATURAL_MOVEMENT_BUDGET:
            if self._take_movement(nearest[0], start_map, start_pos):
                return True
        # ② 城镇里的传送 NPC（比奇有 Teleport_Jason/Harrison/Kyle…，脚本用 `MOVE` 切图）。
        if self._teleport_by_npc(start_map, start_pos):
            return True
        if natural:
            reason = (
                f"@mapinfo 找到的最近可走传送点 {natural[0]['pos']} 距起点 "
                f"{self._distance(natural[0]['pos'])} 格（超过 {self.NATURAL_MOVEMENT_BUDGET} 格预算）"
            )
        else:
            reason = "缺失（`@mapinfo` 没给出可走传送点，服务端也没下发 movements）"
        # ③ 兜底：GM `@map`。
        return self._cross_map_via_gm(reason)

    def _take_movement(self, movement: dict, start_map, start_pos) -> bool:
        """走到传送点格子上 → 断言 `S.MapChanged` / 落点正确。

        `movement` 统一形状：`pos`（源格）、`index`（目标图编号）、`map`/`target`（目标图文件名
        与落点，`@mapinfo` 才有）、`dist`（距玩家的距离）。

        踩上格子那一步可能被「目标格被占」（城门上常有 NPC/怪物/别的玩家）挡下来，
        所以最后一步要重试几次；服务端把换图排到下一个 tick 结算。
        """
        source = movement["pos"]
        self._load_map()
        if not self.goto(source, max_steps=self.NATURAL_MOVEMENT_BUDGET + 10):
            self.diagnostics(f"走不到传送点 {source}")
            return False
        for attempt in range(8):
            if self.self_pos == source:
                break
            dx = source[0] - self.self_pos[0]
            dy = source[1] - self.self_pos[1]
            writer = Writer()
            writer.u8(self._direction(dx, dy))
            self.send(C_WALK, writer.raw())
            deadline = time.time() + 1.5
            while time.time() < deadline:
                packet = self.next_packet(deadline)
                if packet is None:
                    break
                index, payload = packet
                self._observe(index, payload)
                if index == S_MAP_CHANGED:
                    return self._confirm_map_change(movement)
                if index == S_USER_LOCATION:
                    break
            time.sleep(0.3)
        if self.self_pos != source:
            self.diagnostics(
                f"踩不上传送点 {source}（当前位置 {self.self_pos}，"
                f"格子被 NPC/怪物/玩家占住？）"
            )
            return False
        # 人已经站在传送点格子上：换图由服务端的 tick 结算，等 `S.MapChanged`。
        deadline = time.time() + self.timeout
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            index, payload = packet
            self._observe(index, payload)
            if index == S_MAP_CHANGED:
                return self._confirm_map_change(movement)
        self.diagnostics(f"踩传送点 {source} 后 {self.timeout:.0f}s 内没收到 S.MapChanged")
        return False

    def _confirm_map_change(self, movement: dict) -> bool:
        """换图后的断言：目标图编号/落点可站，并输出「自然传送点 …」那一行。"""
        if movement.get("index") is not None and self.map_index != movement["index"]:
            self.fail("跨图传送", f"目标地图 #{movement['index']}，实际 #{self.map_index}")
            return False
        if not self._walkable_at(self.self_pos):
            self.fail("跨图传送", f"落点 {self.self_pos} 不可站立（{self.map_file}）")
            return False
        target = movement.get("target")
        target_text = (
            f"{movement['map']} {target}" if target and movement.get("map") else self.map_file
        )
        self.ok(
            "跨图传送",
            f"自然传送点 {movement['pos']} → {target_text}"
            f"（{movement.get('dist', 0)} 格）",
        )
        return True

    def _teleport_by_npc(self, start_map, start_pos) -> bool:
        """走到传送 NPC（名字以 `Teleport` 开头）→ `@Main` → 点 `@tele`/`@move` 页 → 换图。"""
        teleporters = [npc for npc in self.map_npcs if "teleport" in npc["name"].lower()]
        if not teleporters:
            return False
        teleporters.sort(key=lambda npc: self._distance(npc["pos"]))
        for npc in teleporters[:2]:
            if not self._stand_next_to(npc):
                self.diagnostics(f"走不到传送 NPC {npc['name']} @ {npc['pos']}")
                continue
            spawn = dict(npc, id=npc["object_id"])
            if self.gm:
                # 传送页有 `CHECKGOLD > 500`，GM 先补点钱（等价于玩家自己带钱）。
                self.chat("@gold 100000")
                time.sleep(0.3)
                self._drain(0.3)
            if self.call_npc(spawn, "Main", timeout=4) is None:
                self.diagnostics(f"{npc['name']} 的 @Main 没回应")
                continue
            tele_key = next(
                (key for key in self._button_keys(self.last_npc_page) if "tele" in key.lower()),
                None,
            ) or (self._button_keys(self.last_npc_page) or [None])[0]
            if tele_key is None:
                continue
            if self.call_npc(spawn, tele_key, timeout=4) is None:
                continue
            move_keys = [key for key in self._button_keys(self.last_npc_page) if "move" in key.lower()]
            for key in move_keys[:3]:
                before = self.self_pos
                if self.call_npc(spawn, key, timeout=4) is None and self.self_pos == before:
                    continue
                deadline = time.time() + self.timeout
                while time.time() < deadline:
                    packet = self.next_packet(deadline)
                    if packet is None:
                        break
                    index, payload = packet
                    self._observe(index, payload)
                    jumped = before is not None and self.self_pos is not None and (
                        max(abs(self.self_pos[0] - before[0]), abs(self.self_pos[1] - before[1])) > 20
                        or self.map_file != start_map
                    )
                    if index == S_MAP_CHANGED or jumped:
                        self.ok(
                            "跨图传送",
                            f"传送 NPC {npc['name']} {tele_key} → {key}："
                            f"{start_map} {start_pos} → {self.map_file} @ {self.self_pos}",
                        )
                        return True
        return False

    def _cross_map_via_gm(self, reason: str) -> bool:
        """兜底：GM `@map <地图> <x> <y>`（等价于 Crystal `@move`）。"""
        if not self.gm:
            self.fail("跨图传送", f"{reason}；且未开 --gm，无法走 GM @map 兜底")
            return False
        start_map, start_pos = self.map_file, self.self_pos
        destinations = [mv["destination"] for mv in self.movements] or [34, 47, 64]
        for map_id in destinations[:4]:
            self.chat(f"@map {map_id} 100 100")
            self._drain(1.2)
            if self.map_file and self.map_file != start_map and self.self_pos is not None:
                self.ok(
                    "跨图传送",
                    f"{start_map} {start_pos} → map#{map_id} {self.map_file} @ {self.self_pos}"
                    f"（GM @map 兜底：{reason}）",
                )
                return True
        self.fail("跨图传送", f"{reason}；GM @map 兜底也失败（最近包 {list(self.recent_packets)}）")
        return False

    def _drain(self, seconds: float) -> None:
        deadline = time.time() + seconds
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            self._observe(*packet)

    @staticmethod
    def _load_gates() -> dict[str, set[tuple[int, int]]]:
        path = pathlib.Path(__file__).resolve().parent / "gates.json"
        try:
            import json

            raw = json.loads(path.read_text(encoding="utf-8-sig"))
        except (OSError, ValueError):
            return {}
        gates: dict[str, set[tuple[int, int]]] = {}
        for entry in raw:
            gates.setdefault(str(entry.get("map")), set()).add(
                (int(entry["x"]), int(entry["y"]))
            )
        return gates

    def _map_gates(self, target: tuple[int, int]) -> set[tuple[int, int]]:
        gates = set(self.gate_map.get(self.map_file or "", set()))
        gates.discard(target)
        return gates

    def _static_npc_cells(self) -> set[tuple[int, int]]:
        """S.ObjectNPC/可见 NPC 的静态格：walker 的永久障碍。"""
        return {tuple(n["pos"]) for n in self.map_npcs if n.get("pos")} | {
            tuple(n["pos"]) for n in self.npcs if n.get("pos")
        }

    def _dynamic_blocked_cells(self) -> set[tuple[int, int]]:
        """怪物等会移动的临时障碍，TTL 到期自动解除。"""
        now = time.monotonic()
        expired = [cell for cell, until in self._dynamic_blocked.items() if until <= now]
        for cell in expired:
            del self._dynamic_blocked[cell]
        return set(self._dynamic_blocked)

    def _monster_cells(self) -> set[tuple[int, int]]:
        """当前可见活怪格：作为**软**障碍加入规划，避免主动走进怪堆。"""
        return {tuple(m["pos"]) for m in self.monsters if not m.get("dead")}

    def _walker_blocked(self) -> set[tuple[int, int]]:
        return (
            self._static_npc_cells()
            | self._dynamic_blocked_cells()
            | self._monster_cells()
        )

    def _obstacle_kind(self, cell: tuple[int, int]) -> str:
        if cell in self._static_npc_cells():
            return "static_npc"
        if any(tuple(m["pos"]) == cell and not m.get("dead") for m in self.monsters):
            return "monster"
        if cell in NEWBIE_DOORS:
            return "door"
        return "unknown"

    def _mark_dynamic_blocked(self, cell: tuple[int, int], kind: str) -> None:
        # 怪物会走：5s TTL；玩家/未知占位也会走开，但轨迹更慢，给 30s TTL。
        # 这样“成功一步就清 local_blocked”不会让同一格立刻被重新选中。
        if kind == "monster":
            self._dynamic_blocked[cell] = time.monotonic() + 5.0
        elif kind == "unknown":
            self._dynamic_blocked[cell] = time.monotonic() + 30.0

    def walk_towards(
        self,
        target: tuple[int, int],
        max_steps: int = 200,
        blocked: set[tuple[int, int]] | None = None,
    ) -> bool:
        """服务端确认制逐步移动：发一步 → 等 S.UserLocation 确认落在**目标格** → 下一步。

        确认失败（没有 S.UserLocation / 落在别的格）不重试同一步掩盖问题：把该格加入
        本次局部障碍后重新 BFS；连续 3 步无确认直接带诊断返回 False。
        """
        blocked = set(blocked or set()) | self._map_gates(target) | self._walker_blocked()
        if self.self_pos is None:
            return False
        base_blocked = set(blocked)
        local_blocked = set(blocked)
        door_sent: set[tuple[int, int]] = set()
        rejected = 0
        for _ in range(max_steps):
            if max(abs(self.self_pos[0] - target[0]), abs(self.self_pos[1] - target[1])) <= 1:
                return True
            local_blocked |= self._dynamic_blocked_cells()
            path = self._bfs_path(target, local_blocked)
            if path is None and local_blocked != base_blocked:
                local_blocked = set(base_blocked)
                path = self._bfs_path(target, local_blocked)
            if path is None:
                self.diagnostics(f"BFS 无路 {self.self_pos}->{target}（blocked={len(local_blocked)}）")
                return False
            before = self.self_pos
            nxt = path[0]
            if time.time() < self.move_ready_at:
                time.sleep(self.move_ready_at - time.time())
            direction = self._direction(nxt[0] - before[0], nxt[1] - before[1])
            writer = Writer()
            writer.u8(direction)
            self.send(C_WALK, writer.raw())
            confirmed = False
            deadline = time.time() + 1.2
            while time.time() < deadline:
                packet = self.next_packet(deadline)
                if packet is None:
                    break
                index, payload = packet
                self._observe(index, payload)
                if index == S_USER_LOCATION:
                    # `_observe` 已把 self_pos 更新成服务端实际位置。
                    confirmed = self.self_pos == nxt
                    break
            if confirmed:
                rejected = 0
                local_blocked = set(base_blocked)
                # Crystal `PlayerObject.MovementDelay`（600ms）：等动作冷却再发下一步。
                self.move_ready_at = time.time() + 0.68
                continue
            if self.self_pos is not None and self.self_pos != before:
                # 服务端确认了另一格（位移修正）：以服务端为准重算，不重复发同一步。
                self.diagnostics(f"移动确认偏离：期望 {nxt}，服务端 {self.self_pos}（从 {before} 出发）")
                local_blocked = set(base_blocked)
                self.move_ready_at = time.time() + 0.68
                continue
            rejected += 1
            kind = self._obstacle_kind(nxt)
            self.diagnostics(
                f"第 {rejected} 次无移动确认：cell={nxt} reason={kind} 目标={target} "
                f"当前位置={before} 地图={self.map_file}；避开该格并重规划"
            )
            if kind == "door" and nxt not in door_sent:
                door_sent.add(nxt)
                door = NEWBIE_DOORS[nxt]
                self.diagnostics(f"门未开，发 C.Opendoor{{index={door}}} 后重试 {nxt}")
                writer = Writer()
                writer.u8(door)
                self.send(C_OPENDOOR, writer.raw())
                self._drain(0.2)
                time.sleep(0.35)
                continue
            self._mark_dynamic_blocked(nxt, kind)
            local_blocked.add(nxt)
            if rejected >= 8:
                self.diagnostics("连续 8 步服务端未确认，停止并交由 service/data owner 修")
                return False
            time.sleep(0.35)
        return False

    @staticmethod
    def _step_cell(pos: tuple[int, int], direction: int) -> tuple[int, int]:
        dx, dy = {
            0: (0, -1), 1: (1, -1), 2: (1, 0), 3: (1, 1),
            4: (0, 1), 5: (-1, 1), 6: (-1, 0), 7: (-1, -1),
        }[direction]
        return pos[0] + dx, pos[1] + dy

    @staticmethod
    def _direction(dx: int, dy: int) -> int:
        # Crystal MirDirection：0=Up, 1=UpRight, 2=Right, 3=DownRight, 4=Down,
        # 5=DownLeft, 6=Left, 7=UpLeft
        if dx > 0 and dy < 0:
            return 1
        if dx > 0 and dy == 0:
            return 2
        if dx > 0 and dy > 0:
            return 3
        if dx == 0 and dy > 0:
            return 4
        if dx < 0 and dy > 0:
            return 5
        if dx < 0 and dy == 0:
            return 6
        if dx < 0 and dy < 0:
            return 7
        return 0

    # ---------------- 新手 10 分钟（非 GM） ----------------
    PREY_KEYWORDS = ("hen", "deer", "鸡", "鹿")

    def _live_prey(self) -> list[dict]:
        gates = self.gate_map.get(self.map_file or "", set())
        return [
            m
            for m in self.monsters
            if not m.get("dead")
            and any(key in m["name"].lower() for key in self.PREY_KEYWORDS)
            # 别把怪杀在传送格上：掉落会落在门上，踩上去会换图，永远捡不到。
            and tuple(m["pos"]) not in gates
        ]

    def _attack_until_dead(self, prey: dict, timeout: float = 60.0) -> bool:
        deadline = time.time() + timeout
        while time.time() < deadline:
            latest = next((m for m in self.monsters if m["id"] == prey["id"]), prey)
            if latest.get("dead"):
                return True
            prey = latest
            distance = max(
                abs(self.self_pos[0] - prey["pos"][0]),
                abs(self.self_pos[1] - prey["pos"][1]),
            )
            if distance > 1:
                self.walk_towards(prey["pos"], max_steps=12)
                continue
            dx = prey["pos"][0] - self.self_pos[0]
            dy = prey["pos"][1] - self.self_pos[1]
            writer = Writer()
            writer.u8(self._direction(dx, dy))
            writer.u8(0)
            self.send(C_ATTACK, writer.raw())
            wait = time.time() + 0.8
            while time.time() < wait:
                packet = self.next_packet(wait)
                if packet is None:
                    break
                index, payload = packet
                self._observe(index, payload)
                if index == S_OBJECT_DIED and Reader(payload).u32() == prey["id"]:
                    return True
        return False

    def newbie_equip(self) -> int:
        equipped = 0
        self.equipped_uids = []
        self.equipped_slots = []
        self.equipped_items = []
        for item in list(self.inventory_items):
            for slot in range(14):
                writer = Writer()
                writer.u8(1)  # MirGridType.Inventory
                writer.u64(item["unique_id"])
                writer.i32(slot)
                self.send(C_EQUIP_ITEM, writer.raw())
                deadline = time.time() + 0.2
                success = False
                while time.time() < deadline:
                    packet = self.next_packet(deadline)
                    if packet is None:
                        break
                    index, payload = packet
                    self._observe(index, payload)
                    if index == S_EQUIP_ITEM:
                        success = Reader(payload).boolean()
                        break
                if success:
                    equipped += 1
                    self.equipped_uids.append(item["unique_id"])
                    self.equipped_slots.append(slot)
                    self.equipped_items.append(
                        dict(
                            item,
                            slot=slot,
                            dura_before=int(item.get("current_dura", 0)),
                            current_dura=int(item.get("current_dura", 0)),
                        )
                    )
                    break
        return equipped

    def step_onto(self, cell: tuple[int, int], timeout: float = 5.0) -> bool:
        if self.self_pos is None:
            return False
        direction = self._direction(cell[0] - self.self_pos[0], cell[1] - self.self_pos[1])
        old_map = self.map_file
        writer = Writer()
        writer.u8(direction)
        self.send(C_WALK, writer.raw())
        deadline = time.time() + timeout
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            self._observe(packet[0], packet[1])
            if self.map_file != old_map:
                return True
            if self.self_pos == cell:
                return True
        return False

    def recover_from_map(self) -> bool:
        """被城镇门带到 0103/0104 后，走回该图的回程传送格并校验回到 0。"""
        for _ in range(3):
            if self.map_file == "0":
                return True
            backs = {
                "0103": [(16, 25), (17, 24), (4, 13), (5, 13)],
                "0104": [(4, 11)],
                "0107": [(10, 13)],
            }.get(self.map_file)
            if not backs:
                self.diagnostics(f"误入未知地图 {self.map_file} @ {self.self_pos}")
                return False
            moved = False
            for back in backs:
                print(f"  [恢复] {self.map_file} @ {self.self_pos} -> 试回程点 {back}")
                if not self.walk_towards(back, max_steps=80):
                    continue
                if self.step_onto(back) and self.map_file == "0":
                    moved = True
                    break
            if not moved:
                self.diagnostics(f"回程点全部失败（当前 {self.map_file}@{self.self_pos}）")
                return False
            self._drain(0.5)
        return self.map_file == "0"

    def newbie_hunt(self, count: int) -> bool:
        """自然打猎：只打 Hen，逐只计数 1→2→3；用服务端确认的 ObjectDied + 经验断言。"""
        exp_before = self.self_exp or 0
        kills_before = self.hen_kill_count
        deadline = time.time() + 900
        anchors = [(342, 312), (280, 260)]
        anchor_index = 0
        empty_waits = 0
        while self.hen_kill_count - kills_before < count and time.time() < deadline:
            preys = [m for m in self._live_prey() if "hen" in m["name"].lower()]
            if not preys:
                anchor = anchors[anchor_index % len(anchors)]
                if not self.walk_towards(anchor, max_steps=160):
                    self.diagnostics(f"无法走到 Hen 群锚点 {anchor}，当前位置 {self.self_pos}")
                    anchor_index += 1
                    continue
                self._drain(1.5)
                empty_waits += 1
                print(f"  [打猎 t=+{int(time.time()-self.run_started_at)//60:02d}:{int(time.time()-self.run_started_at)%60:02d}] 锚点 {anchor} 无 Hen（monsters={len(self.monsters)}，等待 {empty_waits}）")
                if empty_waits >= 3:
                    empty_waits = 0
                    anchor_index += 1
                time.sleep(1.0)
                continue
            prey = min(preys, key=lambda m: self._distance(m["pos"]))
            print(f"  [打猎 t=+{int(time.time()-self.run_started_at)//60:02d}:{int(time.time()-self.run_started_at)%60:02d}] Hen {self.hen_kill_count - kills_before + 1}/{count} pos={self.self_pos} 目标={prey['name']}@{prey['pos']} 距离={self._distance(prey['pos'])}")
            if not self._attack_until_dead(prey, timeout=45):
                self.diagnostics(f"打 {prey['name']}#{prey['id']}@{prey['pos']} 45s 未击杀，换目标")
            self._drain(0.4)
        kills = self.hen_kill_count - kills_before
        exp_after = self.self_exp or 0
        if kills < count:
            self.fail("计数", f"阶段=计数：只击杀 Hen {kills}/{count}（经验 {exp_before}->{exp_after}；Deer 不计入）")
            return False
        if exp_after <= exp_before:
            self.fail("计数", f"阶段=计数：Hen 击杀 {kills} 只但经验未增长（{exp_before}->{exp_after}）")
            return False
        self.ok("计数", f"Hen 逐只 1→{kills}，经验 {exp_before}->{exp_after}（ObjectDied+GainExperience；Deer 不计入）")
        return True

    def newbie_level_to(self, target_level: int = 4) -> bool:
        """清洁角色自然打怪升到 target_level（quest 7 的 MinLevel=4）。"""
        start_level = self.self_level or 1
        start_exp = self.self_exp or 0
        start_kills = self.kill_count
        min_hp = self.self_hp
        deadline = time.time() + 900
        while (self.self_level or 1) < target_level and time.time() < deadline:
            before_level = self.self_level
            before_exp = self.self_exp or 0
            if not self.newbie_hunt(1):
                self.fail(
                    "升级",
                    f"阶段=升级：打怪升级失败（level={self.self_level} exp={self.self_exp}）",
                )
                return False
            if self.self_hp is not None:
                min_hp = self.self_hp if min_hp is None else min(min_hp, self.self_hp)
            if self.self_level != before_level:
                print(
                    f"  [升级] {before_level}->{self.self_level}；经验 {before_exp}->{self.self_exp}；"
                    f"累计击杀 {self.kill_count - start_kills}；HP {self.self_hp}/{self.self_hp}"
                )
        if (self.self_level or 1) < target_level:
            self.fail(
                "升级",
                f"阶段=升级：900s 内未到 {target_level} 级（level={self.self_level} exp={self.self_exp}，"
                f"击杀 {self.kill_count - start_kills}）",
            )
            return False
        self.ok(
            "升级",
            f"{start_level}->{self.self_level}；经验 {start_exp}->{self.self_exp}；"
            f"击杀 {self.kill_count - start_kills}（Hen={self.hen_kill_count}）；最低 HP={min_hp}",
        )
        return True

    def newbie_pickup(self) -> bool:
        for item in sorted(self.ground_items, key=lambda g: self._distance(g["pos"])):
            if not self.walk_towards(item["pos"], max_steps=60):
                self.diagnostics(f"拾取走不到 {item['pos']}（{item['name']}），当前位置 {self.self_pos} 地图 {self.map_file}")
                continue
            # `C.PickUp` 要求与物品同格（服务端只看当前格 AOI），先确认制踩上去。
            if self.self_pos != tuple(item["pos"]) and not self.step_onto(tuple(item["pos"])):
                self.diagnostics(f"踩不上物品格 {item['pos']}，当前位置 {self.self_pos}")
                continue
            self.last_gained_items.clear()
            self.send(C_PICKUP, b"")
            deadline = time.time() + self.timeout
            picked = None
            while time.time() < deadline:
                packet = self.next_packet(deadline)
                if packet is None:
                    break
                index, payload = packet
                self._observe(index, payload)
                if index == S_GAINED_ITEM:
                    picked = self.last_gained_items[-1]
                    break
                if index == 67:  # S.GainedGold：鸡也可能只掉金币
                    break
            if picked is not None:
                self.ok("捡起掉落", f"{picked.get('item_index')} x{picked.get('count')}")
                return True
        self.fail("捡起掉落", f"附近没有可拾取的地面物品（ground={len(self.ground_items)}）")
        return False

    def _shop_pages(self, npc: dict) -> list[dict]:
        spawn = dict(npc, id=npc.get("object_id", npc.get("id")))
        queue: list[tuple[str, int]] = [("Main", 0)]
        seen: set[str] = set()
        pages: list[dict] = []
        clicks = 0
        while queue and clicks < 16:
            key, depth = queue.pop(0)
            if key in seen:
                continue
            seen.add(key)
            clicks += 1
            self.last_goods = None
            response = self.call_npc(spawn, key, timeout=3)
            self._drain(0.35)
            if self.last_goods is not None:
                pages.append(self.last_goods)
                continue
            if response == S_NPC_RESPONSE and depth < 3:
                for nxt in self._button_keys(self.last_npc_page):
                    if nxt not in seen:
                        queue.append((nxt, depth + 1))
        return pages

    def newbie_go_bookshop(self) -> bool:
        """从 map 0 走自然传送点 (323,247) → 0104(4,10)，再站到书商旁边。"""
        old_map = self.map_file
        target = (323, 247)
        others = {tuple(m["pos"]) for m in self.movements} - {target}
        reached = False
        for dx, dy in ((0, 1), (1, 0), (0, -1), (-1, 0), (1, 1), (-1, 1), (1, -1), (-1, -1)):
            approach = (target[0] + dx, target[1] + dy)
            if not self._is_walkable(approach):
                continue
            if not self.walk_towards(approach, max_steps=240, blocked=others):
                continue
            self.step_onto(target)
            if self.map_file != old_map:
                reached = True
                break
        if not reached:
            self.fail("去书店", f"踩传送点 {target} 后没有换图（仍是 {old_map}，位置 {self.self_pos}）")
            return False
        print(f"  [传送] {old_map} -> {self.map_file} @ {self.self_pos}")
        deadline = time.time() + self.timeout
        while time.time() < deadline and not self.npcs:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            self._observe(packet[0], packet[1])
        book_npcs = [
            npc
            for npc in self.npcs
            if any(key in npc["name"].lower() for key in ("book", "librarian", "书", "技能"))
        ]
        if not book_npcs:
            self.fail("去书店", f"已到 {self.map_file}，但视野里没有书商 NPC（NPC={[n['name'] for n in self.npcs]}）")
            return False
        if not self._stand_next_to(book_npcs[0]):
            self.fail("去书店", f"走不到书商 {book_npcs[0]['name']} @ {book_npcs[0]['pos']}，位置 {self.self_pos}")
            return False
        self.ok("去书店", f"{self.map_file} 的 {book_npcs[0]['name']} @ {book_npcs[0]['pos']}")
        return True

    def newbie_book_and_cast(self) -> bool:
        book_npcs = [
            npc
            for npc in self.npcs
            if any(
                key in npc["name"].lower()
                for key in ("book", "librarian", "书", "书店", "技能", "skill")
            )
        ]
        if not book_npcs:
            self.fail("读书学技能", "当前地图没有书商 NPC")
            return False
        for npc in book_npcs:
            if not self._stand_next_to(npc):
                continue
            pages = self._shop_pages(npc)
            for goods in pages:
                for item in goods["items"]:
                    self.last_gained_items.clear()
                    self.gold_delta = 0
                    writer = Writer()
                    writer.u64(item["unique_id"])
                    writer.u16(1)
                    writer.u8(0)
                    self.send(C_BUY_ITEM, writer.raw())
                    deadline = time.time() + 3
                    bought = None
                    while time.time() < deadline:
                        packet = self.next_packet(deadline)
                        if packet is None:
                            break
                        index, payload = packet
                        self._observe(index, payload)
                        if index == S_GAINED_ITEM:
                            bought = self.last_gained_items[-1]
                            break
                    if bought is None:
                        continue
                    # 读书：UseItem 成功 + S.NewMagic 才算学会。
                    before = set(self.learned_spells)
                    writer = Writer()
                    writer.u64(bought["unique_id"])
                    writer.u8(1)  # MirGridType.Inventory
                    self.send(C_USE_ITEM, writer.raw())
                    deadline = time.time() + 4
                    learned = False
                    while time.time() < deadline:
                        packet = self.next_packet(deadline)
                        if packet is None:
                            break
                        index, payload = packet
                        self._observe(index, payload)
                        if index == S_NEW_MAGIC and self.learned_spells - before:
                            learned = True
                            break
                    if not learned:
                        continue
                    spell = next(iter(self.learned_spells - before))
                    self.ok("读书学技能", f"{bought.get('item_index')} -> spell #{spell}")
                    # 施放一次：对最近的活怪；没有目标就对自身方向空放（服务端仍回 ObjectMagic/失败包）。
                    targets = self._live_prey()
                    target = min(targets, key=lambda m: self._distance(m["pos"])) if targets else None
                    if target is None and spell != 61:
                        self.fail("使用技能", f"已学会 spell #{spell}，但附近没有可施法目标")
                        return False
                    if target is not None:
                        dx = target["pos"][0] - self.self_pos[0]
                        dy = target["pos"][1] - self.self_pos[1]
                    else:
                        # Healing(#61) 可以对自身施放：target_id=self、location=self。
                        target = {"id": self.self_object_id, "pos": self.self_pos}
                        dx, dy = 0, 1
                    writer = Writer()
                    writer.u32(self.self_object_id or 0)
                    writer.u8(spell)
                    writer.u8(self._direction(dx, dy))
                    writer.u32(target["id"] if target else 0)
                    writer.i32(target["pos"][0] if target else 0)
                    writer.i32(target["pos"][1] if target else 0)
                    writer.u8(0)
                    self.send(C_MAGIC, writer.raw())
                    deadline = time.time() + 4
                    casted = False
                    while time.time() < deadline:
                        packet = self.next_packet(deadline)
                        if packet is None:
                            break
                        index, payload = packet
                        self._observe(index, payload)
                        if index == 120:  # S.ObjectMagic
                            casted = True
                            break
                    if casted:
                        self.ok("使用技能", f"spell #{spell} 收到 S.ObjectMagic(120)")
                        return True
                    self.fail("使用技能", f"spell #{spell} 施放后未收到 S.ObjectMagic(120)")
                    return False
        self.fail("读书学技能", "书店货架没有可买/可学的技能书")
        return False

    # ---- 新手链 quest 7：经队长临时授权新增（勿改其它逻辑）----
    def _newbie_walk_bfs(self, npc: dict) -> bool:
        """按 `mapgrid-*.bin` 的 BFS 路径逐格走（绕开建筑；不依赖贪心 walker）。"""
        if self._distance(npc["pos"]) <= 1:
            return True
        dynamic_blocked: set[tuple[int, int]] = set()
        for walked in range(1200):
            if self._distance(npc["pos"]) <= 1:
                return True
            blocked = {tuple(npc["pos"])}
            blocked |= dynamic_blocked
            blocked |= self._walker_blocked()
            # 优先不走门（42 个门格当软障碍）；实在绕不开才允许穿门并在卡住时开门。
            path = self._bfs_path(tuple(npc["pos"]), blocked=blocked | set(NEWBIE_DOORS))
            if not path:
                path = self._bfs_path(tuple(npc["pos"]), blocked=blocked)
            if not path:
                # 动态障碍可能把玩家围在中间；清空本轮的临时障碍再试一次，
                # 否则 5s/30s TTL 还没到期就会被误判为“无路”。
                if dynamic_blocked or self._dynamic_blocked:
                    print(
                        f"  [新手链] BFS 无路径（动态障碍围死）：当前 {self.self_pos} -> "
                        f"{npc.get('name')} @ {npc.get('pos')}；清空临时障碍后重试"
                    )
                    dynamic_blocked.clear()
                    self._dynamic_blocked.clear()
                    path = self._bfs_path(
                        tuple(npc["pos"]),
                        blocked={tuple(npc["pos"])}
                        | self._static_npc_cells()
                        | self._monster_cells()
                        | set(NEWBIE_DOORS),
                    )
            if not path:
                # 怪物也会移动：先尝试打掉一只邻格怪（玩家在包围里会做的选择），
                # 仍不行再等一个移动 tick 重算，避免硬穿怪堆被连续攻击。
                nearby_monsters = [
                    m
                    for m in self.monsters
                    if not m.get("dead") and self._distance(m["pos"]) <= 1
                ]
                nearby = sorted(tuple(m["pos"]) for m in nearby_monsters)
                print(
                    f"  [新手链] BFS 无路径：当前 {self.self_pos} -> {npc.get('name')} @ "
                    f"{npc.get('pos')}；被 {len(nearby)} 只邻格怪包围 {nearby[:8]}，"
                    f"选择攻击 {nearby_monsters[0]['name'] if nearby_monsters else 'none'}"
                )
                if nearby_monsters:
                    if self._attack_until_dead(nearby_monsters[0], timeout=25):
                        continue
                    print("  [新手链] 攻击未清出通路，等待 1s 重算")
                time.sleep(1.0)
                path = self._bfs_path(
                    tuple(npc["pos"]),
                    blocked={tuple(npc["pos"])}
                    | self._static_npc_cells()
                    | self._monster_cells()
                    | set(NEWBIE_DOORS),
                )
            if not path:
                # 最后降级：怪物格只是软障碍（会移动），不能因为一只怪占住走廊就判定无路；
                # 静态 NPC/门仍保留，服务端拒绝时由 step 重试/换锚点处理。
                print(
                    f"  [新手链] BFS 仍无路，忽略软障碍（怪物格）重算："
                    f"{self.self_pos} -> {npc.get('name')} @ {npc.get('pos')}"
                )
                path = self._bfs_path(
                    tuple(npc["pos"]),
                    blocked={tuple(npc["pos"])}
                    | self._static_npc_cells()
                    | set(NEWBIE_DOORS),
                )
            if not path:
                print(f"  [新手链] BFS 无路径：当前 {self.self_pos} -> {npc.get('name')} @ {npc.get('pos')}")
                return False
            cell = path[0]
            if abs(cell[0] - self.self_pos[0]) > 1 or abs(cell[1] - self.self_pos[1]) > 1:
                # 服务端回拉/纠正后路径会过期，下一轮用新位置重算。
                continue
            moved = False
            stale = False
            for attempt in range(1, 2):
                if abs(cell[0] - self.self_pos[0]) > 1 or abs(cell[1] - self.self_pos[1]) > 1:
                    stale = True
                    break
                if time.time() < self.move_ready_at:
                    time.sleep(self.move_ready_at - time.time())
                if self.step_onto(cell):
                    self.move_ready_at = time.time() + 0.68
                    moved = True
                    break
                self.move_ready_at = time.time() + 0.68
                print(
                    f"  [新手链] 移动被拒 第 {attempt}/1 次：当前 {self.self_pos} -> 目标 {cell}，"
                    f"原因={self._obstacle_kind(cell)}，地图 {self.map_file}；立即重规划"
                )
            if stale:
                continue
            if not moved:
                door = NEWBIE_DOORS.get(cell)
                if door is not None:
                    print(f"  [新手链] 门未开？door_index={door} @ {cell}，尝试 C.Opendoor")
                    writer = Writer()
                    writer.u8(door)
                    self.send(C_OPENDOOR, writer.raw())
                    self._drain(0.2)
                    if self.step_onto(cell):
                        moved = True
                if not moved:
                    # 动态障碍：记下并重新规划整条路径，而不是一直硬走同一格。
                    kind = self._obstacle_kind(cell)
                    self._mark_dynamic_blocked(cell, kind)
                    dynamic_blocked.add(cell)
                    print(
                        f"  [新手链] 该格拒绝 reason={kind}，加入动态障碍并重算路径：{cell}；"
                        f"目标 NPC {npc.get('name')} @ {npc.get('pos')}"
                    )
                    continue
        print(f"  [新手链] BFS 行走超过 500 步：当前 {self.self_pos}，目标 {npc.get('pos')}")
        return False

    def newbie_quest_accept(self) -> bool:
        """走到 Assistant_Jane 接 quest 3（1 级可接的纯对话任务），断言 S.ChangeQuest{3}。"""
        npc = next(
            (n for n in self.map_npcs if "jane" in n["name"].lower()),
            None,
        )
        if npc is None:
            self.fail("接取", f"map0 没有 Assistant_Jane：{ [n['name'] for n in self.map_npcs[:12]] }")
            return False
        # 登录时 S.CompleteQuest(202) 可能和新手聊天前后脚到达；先等到它再断言清洁初态。
        deadline = time.time() + 1.5
        while not self.completed_quests_seen and time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            self._observe(packet[0], packet[1])
        self.ok(
            "清洁初态",
            f"角色 {self.account}/{self.character}；金币 {self.self_gold}，经验 {self.self_exp}，"
            f"completed_quests={sorted(self.completed_quests)}（不含 3={3 not in self.completed_quests}）",
        )
        if not self.completed_quests_seen:
            self.fail("清洁初态", "阶段=接取：未收到 S.CompleteQuest(202)，无法证明 completed_quests 不含 3")
            return False
        if 3 in self.completed_quests:
            self.fail("清洁初态", f"阶段=接取：新角色 completed_quests 已含 3：{sorted(self.completed_quests)}")
            return False
        if not self._newbie_walk_bfs(npc) and not self._stand_next_to(npc):
            self.fail("接取", f"走不到 Assistant_Jane @ {npc['pos']}，阶段=接取")
            return False
        visible = next((n for n in self.npcs if n["id"] == npc.get("object_id")), None)
        if visible is not None:
            print(
                f"  [诊断] Jane NPC: object_id={visible['id']} "
                f"quest_ids={visible.get('quest_ids')} file={npc.get('file')}"
            )
        else:
            print(
                f"  [诊断] Jane NPC: object_id={npc.get('object_id')} "
                f"quest_ids=None（不在 S.ObjectNPC 可见集） file={npc.get('file')}"
            )
        writer = Writer()
        writer.u32(npc.get("object_id", npc.get("id", 0)))
        writer.i32(3)
        chats_before = len(self.chats)
        self.send(C_ACCEPT_QUEST, writer.raw())
        deadline = time.time() + self.timeout
        accepted = False
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            index, payload = packet
            self._observe(index, payload)
            if index == S_CHANGE_QUEST:
                quest_id = Reader(payload).i32()
                if quest_id == 3:
                    accepted = True
                    break
        if not accepted:
            self.fail(
                "接取",
                f"阶段=接取：发 C.AcceptQuest{{3}} 后未收到 S.ChangeQuest{{3}}；"
                f"Jane quest_ids={visible.get('quest_ids') if visible else None}；"
                f"失败聊天={self.chats[chats_before:]}",
            )
            return False
        self.ok("接取", f"Assistant_Jane @ {npc['pos']} -> S.ChangeQuest{{3}}")
        return True

    def newbie_quest_finish(self) -> bool:
        """走到 Merchant_John 交 quest 3：断言 CompleteQuest{3}、经验 +10、金币前后不变。"""
        npc = next(
            (n for n in self.map_npcs if "merchant_john" in n["name"].lower() or "merchant john" in n["name"].lower()),
            None,
        )
        if npc is None:
            self.fail("交付", f"阶段=交付：map0 没有 Merchant_John：{ [n['name'] for n in self.map_npcs[:12]] }")
            return False
        if not self._stand_next_to(npc) and not self._newbie_walk_bfs(npc):
            self.fail("交付", f"阶段=交付：走不到 Merchant_John @ {npc['pos']}")
            return False
        gold_before = self.self_gold
        exp_before = self.self_exp or 0
        self.gold_delta = 0
        writer = Writer()
        writer.i32(3)
        writer.i32(-1)
        self.send(C_FINISH_QUEST, writer.raw())
        deadline = time.time() + self.timeout
        completed = False
        while time.time() < deadline and not completed:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            index, payload = packet
            self._observe(index, payload)
            if index == S_COMPLETE_QUEST:
                reader = Reader(payload)
                count = reader.i32()
                ids = [reader.i32() for _ in range(max(0, min(count, 64)))]
                completed = 3 in ids
        if not completed or 3 not in self.completed_quests:
            self.fail(
                "交付",
                f"阶段=交付：发 C.FinishQuest{{3}} 后未收到含 3 的 S.CompleteQuest{{3}}；"
                f"本地 completed_quests={sorted(self.completed_quests)}",
            )
            return False
        exp_after = self.self_exp or 0
        gold_after = (gold_before or 0) + self.gold_delta
        if self.gold_delta != 0:
            self.fail("交付", f"阶段=交付奖励：S.GainedGold 累计 {self.gold_delta:+d}，期望 +0")
            return False
        if exp_after - exp_before != 10:
            self.fail("交付", f"阶段=交付奖励：经验 {exp_before}->{exp_after}，期望 +10")
            return False
        self.ok(
            "交付",
            f"completed_quests 含 3；金币 {gold_before}->{gold_after}（+{self.gold_delta}），"
            f"经验 {exp_before}->{exp_after}（+{exp_after - exp_before}）",
        )
        self.self_gold = gold_after
        return True

    def newbie_sell_item(self) -> bool:
        """在 Merchant_John 卖一件出生装/背包物品：断言金币增加、背包少一件。"""
        npc = next(
            (
                n
                for n in self.map_npcs
                if "merchant_john" in n["name"].lower()
                or "merchant john" in n["name"].lower()
                or "约翰" in n["name"]
            ),
            None,
        )
        if npc is None:
            self.fail("卖出", f"阶段=卖出：map0 没有 Merchant_John：{ [n['name'] for n in self.map_npcs[:12]] }")
            return False
        if not self._stand_next_to(npc) and not self._newbie_walk_bfs(npc):
            self.fail("卖出", f"阶段=卖出：走不到 Merchant_John @ {npc['pos']}")
            return False
        spawn = dict(npc, id=npc.get("object_id", npc.get("id")))
        for key in ("@BuySell", "@sell", "@Sell"):
            self.call_npc(spawn, key, timeout=3)
            self._drain(0.2)
        if not self.inventory_items:
            self.fail("卖出", "阶段=卖出：背包为空，无法卖")
            return False
        item = self.inventory_items[0]
        gold_before = self.self_gold or 0
        bag_before = len(self.inventory_items)
        self.gold_delta = 0
        self.last_gained_gold = 0
        writer = Writer()
        writer.u64(item["unique_id"])
        writer.u16(item["count"])
        self.send(C_SELL_ITEM, writer.raw())
        sold = False
        deadline = time.time() + self.timeout
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            index, payload = packet
            self._observe(index, payload)
            if index == S_SELL_ITEM:
                seller = Reader(payload)
                _uid = seller.u64()
                _count = seller.u16()
                sold = seller.boolean()
            if sold and index == S_GAINED_GOLD:
                break
        if not sold:
            self.fail(
                "卖出",
                f"阶段=卖出：C.SellItem #{item['item_index']} unique_id={item['unique_id']} 未成功；"
                f"最近包={list(self.recent_packets)}",
            )
            return False
        # 服务端 SellItem 成功包只带 unique_id/count；这里同步本地背包与金币。
        self.inventory_items = [
            entry for entry in self.inventory_items if entry["unique_id"] != item["unique_id"]
        ]
        self.self_gold = gold_before + self.gold_delta
        bag_after = len(self.inventory_items)
        gold_after = self.self_gold or 0
        if gold_after <= gold_before:
            self.fail("卖出", f"阶段=卖出：金币未增加 {gold_before}->{gold_after}")
            return False
        if bag_after != bag_before - 1:
            self.fail("卖出", f"阶段=卖出：背包 {bag_before}->{bag_after}，期望 -1")
            return False
        self.ok(
            "卖出",
            f"卖出 #{item['item_index']} unique_id={item['unique_id']}；"
            f"金币 {gold_before}->{gold_after}；背包 {bag_before}->{bag_after}",
        )
        return True

    def newbie_buy_equipment(self) -> bool:
        """买第一件装备：断言 S.LoseGold 金额、S.GainedItem 模板/数量、背包 +1。"""
        keywords = ("smith", "armour", "armor", "weapon", "装备", "铁匠", "武器", "衣服")
        candidates = [
            npc for npc in self.map_npcs if any(word in npc["name"].lower() for word in keywords)
        ]
        if not candidates:
            self.fail("买装备", f"map0 没有装备店 NPC：{ [n['name'] for n in self.map_npcs[:12]] }")
            return False
        for npc in candidates:
            if not self._stand_next_to(npc):
                continue
            pages = self._shop_pages(npc)
            for goods in pages:
                items = goods["items"]
                pick = next((item for item in items if item["item_index"] != POTION_ITEM_INDEX), None)
                if pick is None:
                    continue
                gold_before = self.self_gold
                bag_before = len(self.inventory_items)
                self.gold_delta = 0
                self.last_gained_items.clear()
                writer = Writer()
                writer.u64(pick["unique_id"])
                writer.u16(1)
                writer.u8(0)
                self.send(C_BUY_ITEM, writer.raw())
                deadline = time.time() + self.timeout
                got = None
                spent = 0
                while time.time() < deadline:
                    packet = self.next_packet(deadline)
                    if packet is None:
                        break
                    index, payload = packet
                    self._observe(index, payload)
                    if index == S_GAINED_ITEM:
                        got = self.last_gained_items[-1]
                    if index == S_LOSE_GOLD and spent == 0:
                        spent = Reader(payload).u32()
                    if got is not None and spent > 0:
                        break
                if got is None or spent == 0:
                    continue
                if got["item_index"] != pick["item_index"] or got["count"] != 1:
                    self.fail(
                        "购买",
                        f"阶段=购买：S.GainedItem #{got['item_index']}x{got['count']} 与货架 #{pick['item_index']} 不一致",
                    )
                    return False
                if got.get("unique_id", 0) <= 0 or not any(
                    item.get("unique_id") == got["unique_id"] for item in self.inventory_items
                ):
                    self.fail(
                        "购买",
                        f"阶段=购买：装备 unique_id={got.get('unique_id')} 未进入背包（inventory={len(self.inventory_items)}）",
                    )
                    return False
                bag_after = len(self.inventory_items)
                gold_after = (gold_before or 0) - spent
                if bag_after != bag_before + 1:
                    self.fail("买装备", f"背包槽 {bag_before}->{bag_after}，期望 +1")
                    return False
                self.ok(
                    "购买",
                    f"{npc['name']} 货架 #{pick['item_index']}，扣金币 {spent}（余额 {gold_before}->{gold_after}），"
                    f"背包 {bag_before}->{bag_after}，unique_id={got.get('unique_id')}",
                )
                self.self_gold = gold_after
                return True
        self.fail("买装备", f"装备店没有可买货架：{ [n['name'] for n in candidates] }")
        return False

    def newbie_flow(self, char_index: int) -> int:
        self.ok("进入世界", f"object_id={self.self_object_id} pos={self.self_pos}")
        deadline = time.time() + self.timeout
        while time.time() < deadline and not any("新手" in chat for chat in self.chats):
            packet = self.next_packet(deadline)
            if packet is None:
                break
            self._observe(packet[0], packet[1])
        if not any("新手" in chat for chat in self.chats):
            self.fail("新手引导", f"第一条聊天不是新手提示：{self.chats[:1]}")
            return 1
        self.ok("新手引导", next(chat for chat in self.chats if "新手" in chat).splitlines()[0])
        if not self.inventory_items:
            self.fail("出生装", "S.UserInformation 背包为空（期望 StartItem 出生装）")
            return 1
        indices = [item["item_index"] for item in self.inventory_items]
        self.ok("出生装", f"背包 {len(indices)} 件：{indices[:8]}")

        equipped = self.newbie_equip()
        if equipped:
            self.ok("穿上出生装", f"{equipped} 件")
        # quest 3 的 MinLevel=1（Crystal 数据事实）；quest 7 需 4 级，留给长跑 gate。
        if not self.newbie_quest_accept():
            return 1
        if not self.newbie_quest_finish():
            return 1
        if not self.newbie_sell_item():
            return 1
        if not self.newbie_buy_equipment():
            return 1
        if self.gm_commands:
            self.fail("新号禁 GM", f"检测到 @ 命令：{self.gm_commands}")
            return 1
        # 重登持久化：等级/经验/背包/技能都还在。
        level_before = self.self_level
        exp_before = self.self_exp
        inv_before = len(self.inventory_items)
        spells_before = set(self.learned_spells)
        writer = Writer()
        self.send(C_LOGOUT, writer.raw())
        try:
            self.expect(S_LOGOUT_SUCCESS, "下线", 5)
        except ProtocolError:
            self.fail("重登", "LogOut 未收到 S.LogOutSuccess")
            return 1
        self.close()
        self.connect()
        self.expect(S_CONNECTED, "重连")
        writer = Writer()
        writer.bytes_prefixed(bytes(16))
        self.send(C_CLIENT_VERSION, writer.raw())
        self.expect(S_CLIENT_VERSION, "版本校验")
        writer = Writer()
        writer.string(self.account)
        writer.string(self.password)
        self.send(C_LOGIN, writer.raw())
        _, payload = self.expect(S_LOGIN_SUCCESS, "重登")
        if Reader(payload).i32() <= 0:
            self.fail("重登", "重登后角色列表为空")
            return 1
        self.self_object_id = None
        self.self_pos = None
        self.inventory_items = []
        self.equipment_items = []
        self.learned_spells.clear()
        writer = Writer()
        writer.i32(char_index)
        self.send(C_START_GAME, writer.raw())
        self.expect(S_START_GAME, "重登进游戏")
        deadline = time.time() + self.timeout
        while time.time() < deadline and self.self_object_id is None:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            self._observe(packet[0], packet[1])
        ok = (
            self.self_level == level_before
            and self.self_exp == exp_before
            and len(self.inventory_items) >= inv_before
            and spells_before <= set(self.learned_spells)
        )
        detail = (
            f"等级 {level_before}->{self.self_level}，经验 {exp_before}->{self.self_exp}，"
            f"背包 {inv_before}->{len(self.inventory_items)}，技能 {sorted(spells_before)}"
        )
        if not ok:
            self.fail("重登持久化", detail)
            return 1
        self.ok("重登持久化", detail)
        elapsed = int(time.time() - self.run_started_at)
        if elapsed > 30 * 60:
            self.fail("墙钟 ≤30:00", f"实际 {elapsed // 60:02d}:{elapsed % 60:02d}")
            return 1
        self.ok("墙钟 ≤30:00", f"{elapsed // 60:02d}:{elapsed % 60:02d}")
        if self.gm_commands:
            self.fail("新号禁 GM", f"检测到 @ 命令：{self.gm_commands}")
            return 1
        self.ok("新号禁 GM", "全程未发送 @ 命令")
        return 0
    # ---------------- GM 闭环 ----------------
    def send_chat(self, message: str) -> None:
        if message.lstrip().startswith("@"):
            self.gm_commands.append(message)
        writer = Writer()
        writer.string(message)
        writer.i32(0)  # LinkedItems
        self.send(C_CHAT, writer.raw())

    def wait_for_chat(self, needle: str, timeout: float = 5.0) -> bool:
        deadline = time.time() + timeout
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            self._observe(packet[0], packet[1])
            if any(needle.lower() in chat.lower() for chat in self.chats):
                return True
        return any(needle.lower() in chat.lower() for chat in self.chats)

    def wait_for_monster(self, name: str, near: tuple[int, int], radius: int = 2, timeout: float = 5.0):
        deadline = time.time() + timeout
        while time.time() < deadline:
            for monster in self.monsters:
                if not monster["dead"] and name.lower() in monster["name"].lower():
                    if max(abs(monster["pos"][0] - near[0]), abs(monster["pos"][1] - near[1])) <= radius:
                        return monster
            packet = self.next_packet(deadline)
            if packet is None:
                break
            self._observe(packet[0], packet[1])
        return None

    def gm_combat_flow(self) -> None:
        assert self.self_pos is not None
        self._gain_exp_count = 0
        self.send_chat("@level 30")
        if not self.wait_for_chat("Level set", 5):
            self.fail("GM @level", "未收到 'Level set' 聊天回复")
            return
        self.send_chat("@heal")
        self.wait_for_chat("Healed", 5)
        self.send_chat("@mob Hen 5")
        self.wait_for_chat("Spawned", 5)
        prey = None
        for _ in range(8):
            candidates = [
                m
                for m in self.monsters
                if not m["dead"] and "hen" in m["name"].lower()
            ]
            adjacent = [
                m
                for m in candidates
                if max(abs(m["pos"][0] - self.self_pos[0]), abs(m["pos"][1] - self.self_pos[1])) == 1
            ]
            if adjacent:
                prey = adjacent[0]
                break
            # 第一只 @mob 会叠在脚下（距离 0），先挪一格再找相邻目标。
            if candidates:
                self.walk_towards((self.self_pos[0] + 1, self.self_pos[1]), max_steps=2)
            else:
                packet = self.next_packet(time.time() + 2)
                if packet is None:
                    break
                self._observe(packet[0], packet[1])
        if prey is None:
            self.diagnostics("@mob Hen 5 后没有相邻 Hen")
            self.fail("GM 刷怪", "未收到相邻格 Hen 的 S.ObjectMonster(71)")
            return
        killed = False
        hen_ids = {m["id"] for m in self.monsters if not m["dead"] and "hen" in m["name"].lower()}
        for _ in range(400):
            live = [
                m
                for m in self.monsters
                if m["id"] in hen_ids and not m["dead"]
            ]
            if not live:
                killed = True
                break
            adjacent = [
                m
                for m in live
                if max(abs(m["pos"][0] - self.self_pos[0]), abs(m["pos"][1] - self.self_pos[1])) == 1
            ]
            if not adjacent:
                nearest = min(
                    live,
                    key=lambda m: max(
                        abs(m["pos"][0] - self.self_pos[0]), abs(m["pos"][1] - self.self_pos[1])
                    ),
                )
                self.walk_towards(nearest["pos"], max_steps=3)
                continue
            prey = adjacent[0]
            direction = self._direction(prey["pos"][0] - self.self_pos[0], prey["pos"][1] - self.self_pos[1])
            writer = Writer()
            writer.u8(direction)
            writer.u8(0)
            self.send(C_ATTACK, writer.raw())
            deadline = time.time() + 0.65
            while True:
                packet = self.next_packet(deadline)
                if packet is None:
                    break
                self._observe(packet[0], packet[1])
                if packet[0] == S_GAIN_EXPERIENCE:
                    self._gain_exp_count += 1
                if packet[0] == S_OBJECT_DIED:
                    died_id = Reader(packet[1]).u32()
                    if died_id in hen_ids:
                        killed = True
            if killed:
                break
        deadline = time.time() + 3
        while time.time() < deadline and self._gain_exp_count == 0:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            self._observe(packet[0], packet[1])
            if packet[0] == S_GAIN_EXPERIENCE:
                self._gain_exp_count += 1
        self.ok(
            "打死一只鸡/鹿",
            f"GM @mob Hen（ObjectDied 81 + GainExperience 85 x{self._gain_exp_count}）",
        )

        self.send_chat("@learn FireBall")
        deadline = time.time() + 5
        while time.time() < deadline and 31 not in self.learned_spells:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            self._observe(packet[0], packet[1])
        if 31 not in self.learned_spells and not any(
            "already know" in chat.lower() for chat in self.chats
        ):
            self.diagnostics("@learn FireBall 后未收到 NewMagic(117)")
            self.fail("使用技能", "未学会 FireBall（期望 S.NewMagic 117，spell=31）")
            return
        self.send_chat("@mob Hen 1")
        self.wait_for_chat("Spawned", 5)
        target = self.wait_for_monster("Hen", self.self_pos, 2)
        if target is None:
            self.fail("使用技能", "没有可施法的 Hen 目标")
            return
        before_mp = self.self_mp
        got_magic = False
        for _ in range(8):
            self.send_chat("@mob Hen 1")
            if not self.wait_for_chat("Spawned", 4):
                continue
            spawn_chat = self.chats[-1]
            match = re.search(r"object ids: \[([0-9,\s]+)\]", spawn_chat)
            if not match:
                continue
            target_id = int(match.group(1).split(",")[0])
            deadline = time.time() + 1
            while time.time() < deadline:
                packet = self.next_packet(deadline)
                if packet is None:
                    break
                self._observe(packet[0], packet[1])
            writer = Writer()
            writer.u32(self.self_object_id or 0)
            writer.u8(31)  # Spell.FireBall
            writer.u8(0)
            writer.u32(target_id)
            writer.i32(0)
            writer.i32(0)
            writer.u8(0)
            self.send(C_MAGIC, writer.raw())
            deadline = time.time() + 1.5
            while time.time() < deadline:
                packet = self.next_packet(deadline)
                if packet is None:
                    break
                self._observe(packet[0], packet[1])
                if packet[0] == S_OBJECT_MAGIC:
                    got_magic = True
                    break
            if got_magic:
                break
        if not got_magic:
            self.diagnostics("FireBall 施法后未收到 ObjectMagic(120)")
            self.fail("使用技能", "未收到 S.ObjectMagic(120)")
            return
        self.ok("使用技能", f"FireBall 命中（MP {before_mp} -> {self.self_mp}）")

    # ---------------- 主流程 ----------------
    def sell1_flow(self) -> int:
        """卖肉链：杀 Hen → 捡 Chicken → Sarah 卖出 → 金币 → eq1 修理。"""
        self.newbie_equip()
        chicken = None
        kills = 0
        deadline = time.time() + 1500
        for attempt in range(1, 41):
            if kills >= 20 or time.time() >= deadline:
                break
            preys = [
                m
                for m in self.monsters
                if not m.get("dead")
                and ("hen" in m.get("name", "").lower() or "鸡" in m.get("name", ""))
            ]
            if not preys:
                anchor = ((342, 312), (280, 260), (330, 530), (200, 400), (420, 95))[
                    attempt % 5
                ]
                self.walk_towards(anchor, max_steps=160)
                self._drain(1.0)
                continue
            # 优先 d<=3 且周围 3 格怪最少的鸡；同分取近的。
            def score(monster: dict) -> tuple[int, int]:
                crowd = sum(
                    1
                    for other in self.monsters
                    if other is not monster
                    and not other.get("dead")
                    and max(
                        abs(monster["pos"][0] - other["pos"][0]),
                        abs(monster["pos"][1] - other["pos"][1]),
                    )
                    <= 3
                )
                return (crowd, self._distance(monster["pos"]))

            prey = min(preys, key=score)
            print(
                f"  [sell1] attempt={attempt} kills={kills}/20 目标={prey.get('name')} id={prey.get('id')}@{prey['pos']} "
                f"d={self._distance(prey['pos'])} score={score(prey)}"
            )
            if not self._attack_until_dead(prey, timeout=60):
                self.diagnostics(f"卖肉第 {attempt} 次：Hen 未击杀")
                continue
            kills += 1
            chicken_ground = [
                item for item in self.ground_items if "鸡肉" in item.get("name", "")
            ]
            self.last_gained_items.clear()
            self.newbie_pickup()
            picked = list(self.last_gained_items)
            print(
                f"  [sell1] attempt={attempt} kills={kills} ground={[(g.get('name'), g.get('pos')) for g in self.ground_items[:8]]}"
            )
            if not chicken_ground:
                continue
            if picked:
                chicken = picked[-1]
                self.ok(
                    "sell1 捡鸡肉",
                    f"attempt={attempt} 杀鸡={kills} item_index={chicken['item_index']} "
                    f"count={chicken.get('count')} unique_id={chicken['unique_id']}",
                )
                break
        if chicken is None:
            self.fail("sell1 捡鸡肉", f"已击杀 {kills} 只 Hen，未掉鸡肉（cjlaaa 单只约 26.7%）")
            return 1

        sarah = next(
            (
                n
                for n in self.map_npcs
                if "sarah" in n.get("name", "").lower() or "莎拉" in n.get("name", "")
            ),
            None,
        )
        if sarah is None:
            self.fail("sell1 找商人", f"找不到 Merchant_Sarah：{[n.get('name') for n in self.map_npcs]}")
            return 1
        if not self._newbie_walk_bfs(sarah) and not self._stand_next_to(sarah):
            self.fail("sell1 找商人", f"走不到 Sarah @ {sarah['pos']}")
            return 1
        spawn = dict(sarah, id=sarah.get("object_id", sarah.get("id")))
        self.call_npc(spawn, "@main", timeout=2)
        self._drain(0.4)
        print(f"  [sell1] Sarah 主对话页={self.last_npc_page}")
        sell_key = next(
            (
                k
                for k in self._button_keys(self.last_npc_page)
                if "BUY" in k.upper() or "SELL" in k.upper()
            ),
            "@BuySell",
        )
        print(f"  [sell1] 解析商店 key={sell_key}")
        self.call_npc(spawn, sell_key, timeout=2)
        self._drain(0.4)
        print(f"  [sell1] 商店页={self.last_npc_page}")
        gold_before = self.self_gold or 0
        writer = Writer()
        writer.u64(chicken["unique_id"])
        writer.u16(int(chicken.get("count", 1)))
        self.send(C_SELL_ITEM, writer.raw())
        deadline = time.time() + self.timeout
        sold = False
        gained = 0
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            index, payload = packet
            self._observe(index, payload)
            if index == S_SELL_ITEM:
                r = Reader(payload)
                uid = r.u64()
                count = r.u16()
                ok = r.boolean()
                print(f"  [sell1] S.SellItem uid={uid} count={count} success={ok}")
                if ok and uid == chicken["unique_id"]:
                    sold = True
            if index == S_GAINED_GOLD:
                gained += 1
            if sold and gained:
                break
        gold_after = self.self_gold or 0
        if not sold:
            self.fail("sell1 卖出", f"Sarah 拒卖 Chicken uid={chicken['unique_id']}；gold={gold_before}->{gold_after}")
            return 1
        if gold_after - gold_before <= 0:
            self.fail("sell1 金币", f"卖出未加金币：{gold_before}->{gold_after}")
            return 1
        self.ok("sell1 卖出", f"Chicken 卖出，金币 {gold_before}->{gold_after}（+{gold_after - gold_before}）")

        # 金币已到手，接 EQ-1 修理流程（B 会重新打耐久、C 用这笔金币修理）。
        return self.eq1_flow()

    def eq1_flow(self) -> int:
        """EQ-1 现实版：穿出生装打怪掉耐久 → Smith 修理 → 扣款（0 金币也可验）。"""
        max_by_uid = {
            item["unique_id"]: int(item.get("max_dura", 0)) for item in self.inventory_items
        }
        equipped = self.newbie_equip()
        self.ok("EQ-1 装备", f"穿上出生装 {equipped} 件")

        # 打怪 20–30 刀，等待 S.DuraChanged(76)。
        deadline = time.time() + 300
        hits = 0
        while hits < 30 and time.time() < deadline and not self.dura_events:
            if self.self_dead or (self.self_hp or 1) <= 0:
                self.fail("EQ-1 耐久下降", f"攻击中死亡，hits={hits}")
                return 1
            preys = [
                m
                for m in self.monsters
                if not m.get("dead")
                and any(
                    key in m.get("name", "").lower()
                    for key in ("deer", "hen", "scarecrow", "oma", "rakingcat", "yeti", "wolf", "鹿", "鸡", "稻草人", "奥玛", "猫", "雪人", "狼", "卫士", "护卫")
                )
            ]
            if not preys:
                self.walk_towards((self.self_pos[0], self.self_pos[1] + 80), max_steps=80)
                self._drain(0.5)
                continue
            prey = min(preys, key=lambda m: self._distance(m["pos"]))
            if self._distance(prey["pos"]) > 1:
                self.walk_towards(prey["pos"], blocked={tuple(prey["pos"])}, max_steps=20)
                continue
            dx = prey["pos"][0] - self.self_pos[0]
            dy = prey["pos"][1] - self.self_pos[1]
            writer = Writer()
            writer.u8(self._direction(dx, dy))
            writer.u8(0)
            self.send(C_ATTACK, writer.raw())
            hits += 1
            self._drain(0.35)
        # 兵部新增：DuraDelay=10s 周期 flush 脏装备的 S.DuraChanged(76)，等 11s 再断言。
        if hits:
            self._drain(11.0)
        # B：扫描全部已装备物品，找 delta>=1 的目标（武器优先）。
        rows = []
        for equipped in self.equipped_items:
            before = int(equipped.get("dura_before", 0))
            after = int(equipped.get("current_dura", before))
            rows.append((equipped.get("slot", -1), equipped["unique_id"], equipped.get("item_index", -1), before, after, after - before))
        print("  [EQ-1] 装备耐久表：slot | unique_id | item_index | before -> after | delta")
        for slot, uid, index, before, after, delta in rows:
            print(f"    {slot} | {uid} | {index} | {before} -> {after} | {delta}")
        damaged = [
            row for row in rows if row[3] - row[4] >= 1
        ]
        # 武器槽 0 优先，其次按掉耐久从多到少。
        damaged.sort(key=lambda row: (0 if row[0] == 0 else 1, -(row[3] - row[4])))
        if not damaged:
            self.skip("EQ-1 耐久下降", "本轮无装备掉耐久（等待时间/受击次数不足）")
            return 0
        slot, damaged_uid, damaged_index, before_dura, damaged_current, damage_delta = damaged[0]
        damaged_max = max_by_uid.get(damaged_uid, before_dura)
        self.ok(
            "EQ-1 耐久下降",
            f"slot={slot} uid={damaged_uid} index={damaged_index} {before_dura}->{damaged_current}（掉 {-damage_delta}）",
        )

        # 打怪攒修理费：捡 S.ObjectGold(65)，直到至少 2 金币（自然掉落，不用 GM）。
        gold_deadline = time.time() + 90
        while (self.self_gold or 0) < 2 and time.time() < gold_deadline:
            if self.ground_gold:
                gold = min(self.ground_gold, key=lambda g: self._distance(g["pos"]))
                self.walk_towards(gold["pos"], blocked={tuple(gold["pos"])}, max_steps=20)
                self.step_onto(gold["pos"])
                self.send(C_PICKUP, b"")
                self._drain(0.8)
                continue
            preys = [
                m
                for m in self.monsters
                if not m.get("dead")
                and any(
                    key in m.get("name", "").lower()
                    for key in ("deer", "hen", "scarecrow", "oma", "rakingcat", "yeti", "wolf", "鹿", "鸡", "稻草人", "奥玛", "猫", "雪人", "狼", "卫士", "护卫")
                )
            ]
            if not preys:
                self.walk_towards((self.self_pos[0], self.self_pos[1] + 80), max_steps=80)
                self._drain(0.8)
                continue
            self._attack_until_dead(min(preys, key=lambda m: self._distance(m["pos"])), timeout=20)
        self.ok("EQ-1 打怪攒钱", f"金币={self.self_gold or 0}")

        smith = next(
            (
                n
                for n in self.map_npcs
                if n.get("name", "").lower() == "blacksmith_smith"
                or "史密斯" in n.get("name", "")
                or "铁匠" in n.get("name", "")
            ),
            None,
        )
        if smith is None:
            self.fail("EQ-1 修理", "找不到 Blacksmith_Smith")
            return 1
        if not self._newbie_walk_bfs(smith) and not self._stand_next_to(smith):
            self.fail("EQ-1 修理", f"走不到 Smith @ {smith['pos']}")
            return 1
        if self.equipped_slots and damaged_uid in self.equipped_uids:
            # 先把武器脱到背包空格 40（Crystal RemoveItem：from 装备槽、to 背包格）。
            writer = Writer()
            writer.u8(2)  # MirGridType.Equipment
            writer.i32(self.equipped_slots[0])
            writer.i32(40)
            self.send(C_MOVE_ITEM, writer.raw())
            self._drain(0.6)
        before_current = next(
            (
                int(item.get("current_dura", 0))
                for item in self.inventory_items
                if item["unique_id"] == damaged_uid
            ),
            0,
        )
        spawn = dict(smith, id=smith.get("object_id", smith.get("id")))
        # 先 @main 打开主对话，再点 [@REPAIR]；直接点页会被拒（not in dialogue）。
        self.call_npc(spawn, "@main", timeout=2)
        print(f"  [EQ-1] @main 页面原文={self.last_npc_page}")
        repair_key = next(
            (k for k in self._button_keys(self.last_npc_page) if "REPAIR" in k.upper()),
            "@REPAIR",
        )
        print(f"  [EQ-1] 从页面解析修理 key={repair_key}")
        self.call_npc(spawn, repair_key, timeout=2)
        print(f"  [EQ-1] @REPAIR 页面原文={self.last_npc_page}")
        self._drain(0.2)
        writer = Writer()
        writer.u64(damaged_uid)
        self.send(C_REPAIR_ITEM, writer.raw())
        deadline = time.time() + self.timeout
        repaired = None
        repair_ack = None
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            index, payload = packet
            self._observe(index, payload)
            if index == S_REPAIR_ITEM:
                repair_ack = Reader(payload).u64()
            if index == S_ITEM_REPAIRED:
                r = Reader(payload)
                uid = r.u64()
                max_dura = r.u16()
                current = r.u16()
                if uid == damaged_uid:
                    repaired = (max_dura, current)
                    break
        if repair_ack != damaged_uid:
            self.fail(
                "EQ-1 修理",
                f"未收到 S.RepairItem(56) 回显 uid={damaged_uid}（ack={repair_ack}）",
            )
            return 1
        if repaired is None:
            self.fail("EQ-1 修理", f"发 C.RepairItem uid={damaged_uid} 后未收到 S.ItemRepaired(114)")
            return 1
        max_dura, current = repaired
        gold_after = self.self_gold or 0
        if current != max_dura:
            self.fail("EQ-1 修理", f"耐久未回满：{current}/{max_dura}")
            return 1
        if gold_after > gold_before:
            self.fail("EQ-1 扣款", f"金币反而增加：{gold_before}->{gold_after}")
            return 1
        fee = gold_before - gold_after
        self.ok(
            "EQ-1 修理/扣款",
            f"uid={damaged_uid} 耐久 {before_current}->{current}/{max_dura}，"
            f"金币 {gold_before}->{gold_after}（修理价 {fee}；0 有效）",
        )
        return 0

    def quest1_flow(self) -> int:
        """quest 1 完整生命周期：Jane 接取 → 任务物品栏 5 叶 → CraftsLady 交付 → 经验+10。"""
        jane = next(
            (
                n
                for n in self.map_npcs
                if "jane" in n.get("name", "").lower()
                or "简" in n.get("name", "")
                or 1 in (n.get("quest_ids") or [])
            ),
            None,
        )
        crafts = next(
            (
                n
                for n in self.map_npcs
                if "craftslady" in n.get("name", "").lower()
                or "craft" in n.get("name", "").lower()
                or "工匠" in n.get("name", "")
                or "朱迪" in n.get("name", "")
            ),
            None,
        )
        print(
            f"  [NPC] newmapinfo={self.newmapinfo_npc_count} visible={len(self.npcs)} "
            f"merged={len(self.map_npcs)}"
        )
        if jane is None or crafts is None:
            self.fail(
                "quest1 找NPC",
                f"Jane={jane} CraftsLady={crafts}；map_npcs={[n.get('name') for n in self.map_npcs]}",
            )
            return 1
        print(f"  [quest1] 怪物名样本={ [m.get('name') for m in self.monsters[:6]] }")
        # 第二只 NPC 取样放在 Jane 之前：Sarah 就在出生点附近，避免取样后横穿地图。
        sample = next(
            (n for n in self.map_npcs if "sarah" in n.get("name", "").lower()),
            None,
        )
        if sample is not None:
            self.newbie_equip()
            if self._distance(sample["pos"]) > 1:
                self._newbie_walk_bfs(sample)
            spawn_sample = dict(sample, id=sample.get("object_id", sample.get("id")))
            self.call_npc(spawn_sample, "@main", timeout=2)
            self._drain(0.4)
            sample_page = list(self.last_npc_page)
            sample_cjk = any(
                any('\u4e00' <= ch <= '\u9fff' for ch in line) for line in sample_page
            )
            print(
                f"  [NPC对话] {sample.get('name')} @ {sample['pos']} 页行数={len(sample_page)}"
            )
            if sample_page:
                print(f"  [NPC对话] 首行='{sample_page[0]}'")
            print(f"  [NPC对话] 中文={sample_cjk}")
            if not sample_page or not sample_cjk:
                self.fail("quest1 NPC对话2", f"{sample.get('name')} 对话页为空或非中文：{sample_page}")
                return 1
            self.ok("quest1 NPC对话2", f"{sample.get('name')} 中文页 {len(sample_page)} 行")
        self.newbie_equip()
        if not self._newbie_walk_bfs(jane) and not self._stand_next_to(jane):
            self.fail("quest1 接取", f"走不到 Jane @ {jane['pos']}")
            return 1
        spawn_jane = dict(jane, id=jane.get("object_id", jane.get("id")))
        self.call_npc(spawn_jane, "@main", timeout=2)
        # GOTO @Main-1 时会先发一个空页，再发真正正文；等 0.4s 取最后一页。
        self._drain(0.4)
        jane_page = list(self.last_npc_page)
        print(f"  [NPC对话] Jane @ {jane['pos']} 页行数={len(jane_page)}")
        if jane_page:
            print(f"  [NPC对话] 首行='{jane_page[0]}'")
        jane_cjk = any(
            any('\u4e00' <= ch <= '\u9fff' for ch in line) for line in jane_page
        )
        print(f"  [NPC对话] 中文={jane_cjk}")
        if not jane_page or not jane_cjk:
            self.fail("quest1 NPC对话", f"Jane 对话页为空或非中文：{jane_page}")
            return 1
        self.ok("quest1 NPC对话", f"Jane 中文页 {len(jane_page)} 行：{jane_page[0]!r}")
        writer = Writer()
        writer.u32(jane.get("object_id", jane.get("id", 0)))
        writer.i32(1)
        self.send(C_ACCEPT_QUEST, writer.raw())
        deadline = time.time() + self.timeout
        task_lines = None
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            index, payload = packet
            self._observe(index, payload)
            if index == S_CHANGE_QUEST:
                r = Reader(payload)
                quest_id = r.i32()
                count = r.i32()
                lines = [r.string() for _ in range(max(0, min(count, 64)))]
                if quest_id == 1:
                    task_lines = lines
                    self._drain(0.5)
                    break
        if task_lines is None:
            self.fail("quest1 接取", "发 C.AcceptQuest{1} 后未收到 S.ChangeQuest{1}")
            return 1
        self.ok("quest1 接取", f"Jane @ {jane['pos']}")
        print(f"  [quest1] task_list={task_lines}")
        if "前往 比奇省(0)" not in task_lines:
            self.fail("quest1 导航行", f"缺少「前往 比奇省(0)」：{task_lines}")
            return 1
        for line in task_lines:
            if "前往 " in line:
                self.ok("quest1 导航行", line)
        print(f"  [quest1] 任务物品栏={[(i['item_index'], i.get('count')) for i in self.quest_items]}")
        leaf_count = sum(int(i.get("count", 0)) for i in self.quest_items)
        if leaf_count < 5:
            self.fail("quest1 任务物品栏", f"期望 5 个 CannibalLeaves，实际 {leaf_count}：{self.quest_items}")
            return 1
        if not self._newbie_walk_bfs(crafts) and not self._stand_next_to(crafts):
            self.fail("quest1 交付", f"走不到 CraftsLady @ {crafts['pos']}")
            return 1
        exp_before = self.self_exp or 0
        gold_before = self.self_gold or 0
        writer = Writer()
        writer.i32(1)
        writer.i32(-1)
        self.send(C_FINISH_QUEST, writer.raw())
        deadline = time.time() + self.timeout
        completed = False
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            index, payload = packet
            self._observe(index, payload)
            if index == S_COMPLETE_QUEST:
                r = Reader(payload)
                count = r.i32()
                ids = {r.i32() for _ in range(max(0, min(count, 256)))}
                if 1 in ids:
                    completed = True
                    break
        if not completed:
            self.fail("quest1 交付", f"发 C.FinishQuest{{1}} 后未收到含 1 的 S.CompleteQuest")
            return 1
        # 服务端先发 S.GainExperience(85)，再发 S.CompleteQuest(202)；等 0.5s 收完再断言。
        self._drain(0.5)
        exp_after = self.self_exp or 0
        gold_after = self.self_gold or 0
        if exp_after - exp_before != self.expected_quest1_exp:
            self.fail("quest1 奖励", f"经验 {exp_before}->{exp_after}（cjlaaa 数据期望 +50）")
            return 1
        self.ok(
            "quest1 交付/奖励",
            f"completed_quests 含 1；经验 {exp_before}->{exp_after}（+50，cjlaaa），金币 {gold_before}->{gold_after}",
        )
        return 0

    def quest5_flow(self) -> int:
        """quest 5 完整任务：Smith 接取 → 自然杀 10 Deer + 10 Scarecrow → 交付奖励。"""
        smith = next(
            (
                n
                for n in self.map_npcs
                if n.get("name", "").lower() == "blacksmith_smith"
                or "史密斯" in n.get("name", "")
                or "铁匠" in n.get("name", "")
            ),
            None,
        )
        if smith is None:
            self.fail("quest5 找NPC", f"按名字找不到 Blacksmith_Smith：{ [n['name'] for n in self.map_npcs[:20]] }")
            return 1
        if not self._newbie_walk_bfs(smith) and not self._stand_next_to(smith):
            self.fail("quest5 找NPC", f"走不到 Smith @ {smith['pos']}")
            return 1
        writer = Writer()
        writer.u32(smith.get("object_id", smith.get("id", 0)))
        writer.i32(5)
        self.send(C_ACCEPT_QUEST, writer.raw())
        deadline = time.time() + self.timeout
        task_lines = None
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            index, payload = packet
            self._observe(index, payload)
            if index == S_CHANGE_QUEST:
                r = Reader(payload)
                quest_id = r.i32()
                count = r.i32()
                lines = [r.string() for _ in range(max(0, min(count, 64)))]
                if quest_id == 5:
                    task_lines = lines
                    break
        if task_lines is None:
            self.fail("quest5 接取", "发 C.AcceptQuest{5} 后未收到 S.ChangeQuest{5}")
            return 1
        self.ok("quest5 接取", f"Smith @ {smith['pos']}")
        print(f"  [quest5] task_list={task_lines}")
        for line in task_lines:
            if "前往 " in line:
                self.ok("quest5 导航行", line)

        kills = {"deer": 0, "scarecrow": 0}
        # 穿上出生装减伤；背包里的初级红药作为保命手段（不使用任何 GM）。
        equipped = self.newbie_equip()
        self.ok("quest5 装备", f"穿上出生装 {equipped} 件")
        deadline = time.time() + 1200
        anchor_index = 0
        deaths = 0
        while sum(kills.values()) < 20 and time.time() < deadline:
            if self.self_dead or (self.self_hp or 1) <= 0:
                deaths += 1
                if deaths > 3:
                    self.fail("quest5 击杀", f"死亡 {deaths} 次，完成度 {kills}")
                    return 1
                self.ok("quest5 被击杀", f"等待 60s 兜底复活（第 {deaths} 次）")
                revive_end = time.time() + 70
                while time.time() < revive_end and ((self.self_hp or 0) <= 0 or self.self_dead):
                    packet = self.next_packet(time.time() + 0.25)
                    if packet:
                        self._observe(*packet)
                self._dynamic_blocked.clear()
                continue
            missing = [key for key, need in (("deer", 10), ("scarecrow", 10)) if kills[key] < need]
            preys = [
                m
                for m in self.monsters
                if not m.get("dead")
                and any(key in m.get("name", "").lower() for key in missing)
            ]
            if not preys:
                offsets = ((0, 80), (-80, 0), (80, 0), (0, -80), (-80, 80), (80, -80))
                ox, oy = offsets[anchor_index % len(offsets)]
                anchor_index += 1
                if anchor_index % len(offsets) == 0:
                    self._dynamic_blocked.clear()
                self.walk_towards(
                    (self.self_pos[0] + ox, self.self_pos[1] + oy), max_steps=100
                )
                self._drain(0.8)
                continue
            prey = min(preys, key=lambda m: self._distance(m["pos"]))
            if (self.self_hp or 0) <= 12:
                potion = next(
                    (i for i in self.inventory_items if i.get("item_index") == 658),
                    None,
                )
                if potion is not None:
                    writer = Writer()
                    writer.u64(potion["unique_id"])
                    writer.u8(1)
                    self.send(C_USE_ITEM, writer.raw())
                    self._drain(0.4)
            name = prey.get("name", "").lower()
            key = "deer" if "deer" in name else "scarecrow"
            print(
                f"  [quest5] 目标 {key} {kills[key] + 1}/10 "
                f"{prey.get('name')}@{prey['pos']} 距离={self._distance(prey['pos'])}"
            )
            if self._quest5_attack(prey, timeout=45):
                kills[key] += 1
                self.ok("quest5 击杀", f"{key} {kills[key]}/10")
        if kills["deer"] < 10 or kills["scarecrow"] < 10:
            self.fail(
                "quest5 击杀",
                f"超时：deer={kills['deer']}/10 scarecrow={kills['scarecrow']}/10",
            )
            return 1

        if not self._newbie_walk_bfs(smith) and not self._stand_next_to(smith):
            self.fail("quest5 交付", f"走不回 Smith @ {smith['pos']}")
            return 1
        gold_before = self.self_gold or 0
        exp_before = self.self_exp or 0
        self.gold_delta = 0
        writer = Writer()
        writer.i32(5)
        writer.i32(-1)
        self.send(C_FINISH_QUEST, writer.raw())
        deadline = time.time() + self.timeout
        completed = False
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            index, payload = packet
            self._observe(index, payload)
            if index == S_COMPLETE_QUEST:
                r = Reader(payload)
                count = r.i32()
                ids = {r.i32() for _ in range(max(0, min(count, 256)))}
                if 5 in ids:
                    completed = True
                    break
        if not completed:
            self.fail("quest5 交付", f"发 C.FinishQuest{{5}} 后未收到含 5 的 S.CompleteQuest；gold={self.self_gold}")
            return 1
        gold_after = self.self_gold or 0
        exp_after = self.self_exp or 0
        if exp_after - exp_before != 120:
            self.fail("quest5 奖励", f"经验 {exp_before}->{exp_after}（期望 +120）")
            return 1
        if gold_after - gold_before != 30:
            self.fail("quest5 奖励", f"金币 {gold_before}->{gold_after}（期望 +30）")
            return 1
        self.ok(
            "quest5 交付/奖励",
            f"completed_quests 含 5；经验 {exp_before}->{exp_after}（+120），金币 {gold_before}->{gold_after}（+30）",
        )
        return 0

    def _quest5_attack(self, prey: dict, timeout: float = 45.0) -> bool:
        """只打服务端相邻且活着的怪；连续 3 次无正伤害就换目标。"""
        deadline = time.time() + timeout
        misses = 0
        while time.time() < deadline:
            latest = next((m for m in self.monsters if m["id"] == prey["id"]), prey)
            if latest.get("dead"):
                return True
            if self.self_dead or (self.self_hp or 1) <= 0:
                return False
            if self._distance(latest["pos"]) > 1:
                self.walk_towards(latest["pos"], blocked={tuple(latest["pos"])}, max_steps=20)
                continue
            dx = latest["pos"][0] - self.self_pos[0]
            dy = latest["pos"][1] - self.self_pos[1]
            self.dr1_hit_target = None
            writer = Writer()
            writer.u8(self._direction(dx, dy))
            writer.u8(0)
            self.send(C_ATTACK, writer.raw())
            wait = time.time() + 1.2
            while time.time() < wait:
                packet = self.next_packet(time.time() + 0.1)
                if packet:
                    self._observe(*packet)
            if self.dr1_hit_target == latest["id"]:
                misses = 0
            else:
                misses += 1
                print(f"  [quest5] 未命中/0伤害 {latest.get('name')} id={latest['id']} ({misses}/3)")
                if misses >= 3:
                    return False
        return False

    def tp1_flow(self) -> int:
        """TP-1 同图导航：接 quest 5 → 逐字断言导航行 → 走到 movement 门格。"""
        smith = next(
            (
                n
                for n in self.map_npcs
                if "jane" in n.get("name", "").lower()
                or "助理" in n.get("name", "")
                or "简" in n.get("name", "")
                or 1 in (n.get("quest_ids") or [])
            ),
            None,
        )
        if smith is None:
            self.fail("TP-1 接取", f"map0 没有 Assistant_Jane：{ [n['name'] for n in self.map_npcs[:12]] }")
            return 1
        if not self._newbie_walk_bfs(smith) and not self._stand_next_to(smith):
            self.fail("TP-1 接取", f"走不到 Jane @ {smith['pos']}")
            return 1
        writer = Writer()
        writer.u32(smith.get("object_id", smith.get("id", 0)))
        writer.i32(1)
        self.send(C_ACCEPT_QUEST, writer.raw())
        deadline = time.time() + self.timeout
        task_lines = None
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            index, payload = packet
            self._observe(index, payload)
            if index == S_CHANGE_QUEST:
                r = Reader(payload)
                quest_id = r.i32()
                count = r.i32()
                lines = [r.string() for _ in range(max(0, min(count, 64)))]
                if quest_id == 1:
                    task_lines = lines
                    break
        if task_lines is None:
            self.fail("TP-1 接取", "发 C.AcceptQuest{1} 后未收到 S.ChangeQuest{1}")
            return 1
        self.ok("TP-1 接取", f"Jane @ {smith['pos']} -> quest 1")
        print(f"  [TP-1] task_list={task_lines}")
        expected = "前往 比奇省(0)"
        if expected not in task_lines:
            self.fail("TP-1 导航行", f"task_list 未逐字包含 {expected!r}：{task_lines}")
            return 1
        self.ok("TP-1 导航行", expected)

        # 找一个 S.NewMapInfo.movements 门格；不读配置、不硬编码。
        if not self.movements:
            self.fail("TP-1 大地图点击", "S.NewMapInfo.movements 为空")
            return 1
        gate = self.movements[0]
        target = tuple(gate["pos"])
        print(f"  [TP-1] 门格 title={gate['title']} pos={target}")
        fake = {"name": "gate", "pos": target}
        if not self._newbie_walk_bfs(fake) and not self._stand_next_to(fake):
            self.fail("TP-1 寻路", f"走不到门格 {target}（当前位置 {self.self_pos}）")
            return 1
        old_map = self.map_file
        arrived = self.step_onto(target)
        deadline = time.time() + 2.0
        while time.time() < deadline and self.map_file == old_map:
            packet = self.next_packet(time.time() + 0.2)
            if packet is None:
                continue
            self._observe(*packet)
        if self.map_file == old_map:
            self.fail("TP-1 到达", f"踩到门格 {target} 后未换图（仍 {old_map}，位置 {self.self_pos}）")
            return 1
        self.ok("TP-1 到达", f"{old_map} -> {self.map_file} @ {self.self_pos}（gate={gate['title']}）")
        return 0

    def ps1_flow(self, char_index: int) -> int:
        """PS-1：进程级重登后逐字段比对金币/装备/技能/已完成任务。"""
        self.ok("PS-1 清洁角色", f"object_id={self.self_object_id} hp={self.self_hp}")
        # 不穿出生装：保持全部 unique_id 在背包里，重登前后可直接逐项比对，
        # 也避免 EquipItem 不回显装备槽导致的本地缓存差。
        self.ok("PS-1 出生装", f"背包 {len(self.inventory_items)} 件（保留）")

        # 主判据用服务器分配的 unique_id（无需金币）：金币 0/0 只作中性信息。
        self.ok("PS-1 状态", "金币 0（中性，不作主判据）；以背包/装备 unique_id 为主")

        def snapshot_state() -> tuple:
            equipment = sorted(
                (item["unique_id"], item["item_index"], item["current_dura"], item["max_dura"])
                for item in self.equipment_items
            )
            inventory = sorted(
                (item["unique_id"], item["item_index"], item["current_dura"], item["max_dura"])
                for item in self.inventory_items
            )
            return (
                self.self_gold,
                equipment,
                inventory,
                set(self.completed_quests),
                set(self.learned_spells),
            )

        before = snapshot_state()
        print(
            f"  [PS-1] 重登前 gold={before[0]} equipment={before[1]} inventory={before[2]} "
            f"completed={sorted(before[3])} spells={sorted(before[4])}"
        )
        if before[0] is None:
            self.fail("PS-1 获得状态", "没收到金币字段")
            return 1

        # 正常退出（不是强杀）。
        self.send(C_LOGOUT, b"")
        try:
            self.expect(S_LOGOUT_SUCCESS, "PS-1 退出", 5)
        except ProtocolError:
            self.fail("PS-1 退出", "C.LogOut 未收到 S.LogOutSuccess")
            return 1
        self.ok("PS-1 退出", "S.LogOutSuccess")
        self.close()

        # 新 TCP 连接重新登录同一角色。
        self.connect()
        self.expect(S_CONNECTED, "PS-1 重连")
        writer = Writer()
        writer.bytes_prefixed(bytes(16))
        self.send(C_CLIENT_VERSION, writer.raw())
        self.expect(S_CLIENT_VERSION, "PS-1 重连版本")
        writer = Writer()
        writer.string(self.account)
        writer.string(self.password)
        self.send(C_LOGIN, writer.raw())
        _, payload = self.expect(S_LOGIN_SUCCESS, "PS-1 重登")
        if Reader(payload).i32() <= 0:
            self.fail("PS-1 重登", "重登后角色列表为空")
            return 1
        self.self_object_id = None
        self.self_pos = None
        self.inventory_items = []
        self.equipment_items = []
        self.learned_spells.clear()
        self.completed_quests = set()
        writer = Writer()
        writer.i32(char_index)
        self.send(C_START_GAME, writer.raw())
        self.expect(S_START_GAME, "PS-1 重登进游戏")
        deadline = time.time() + self.timeout
        while time.time() < deadline and self.self_object_id is None:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            self._observe(packet[0], packet[1])
        # 等 S.CompleteQuest/背包装备下发完。
        self._drain(0.5)
        if self.self_object_id is None:
            self.fail("PS-1 重登", "重登后未收到 S.UserInformation")
            return 1
        after = snapshot_state()
        print(
            f"  [PS-1] 重登后 gold={after[0]} equipment={after[1]} inventory={after[2]} "
            f"completed={sorted(after[3])} spells={sorted(after[4])}"
        )
        self.ok("PS-1 重登", f"object_id={self.self_object_id} pos={self.self_pos}")

        if before != after:
            self.fail(
                "PS-1 逐字段比对",
                f"before={before} after={after}",
            )
            return 1
        self.ok(
            "PS-1 逐字段比对",
            f"gold={before[0]} equipment={len(before[1])} completed={sorted(before[3])} spells={sorted(before[4])}",
        )

        # 恢复后可玩：移动一次、攻击一次。
        start = self.self_pos
        moved = False
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            target = (start[0] + dx, start[1] + dy)
            if self._walkable_at(target) and self.step_onto(target):
                moved = True
                break
        if not moved:
            self.fail("PS-1 恢复后可移动", f"从 {start} 未能移动")
            return 1
        writer = Writer()
        writer.u8(0)
        writer.u8(0)
        self.send(C_ATTACK, writer.raw())
        self._drain(0.5)
        self.ok("PS-1 恢复后可玩", f"移动 {start}->{self.self_pos}，已发 C.Attack")
        return 0

    def dr1_flow(self) -> int:
        """DR-1 gate：自然死亡 → 12s 无自动复活 → C.TownRevive → 复活/移动/攻击。"""
        if self.gm:
            self.fail("DR-1 清洁角色", "该 gate 不允许 --gm（捷径自检）")
            return 1
        if not self.monsters:
            self.fail("DR-1 自然死亡", "当前地图没有 S.ObjectMonster，无法自然战斗")
            return 1
        self.ok("DR-1 清洁角色", f"object_id={self.self_object_id} map={self.map_file} hp={self.self_hp}")
        # 清洁角色出生装还在背包里；先穿上，否则 1 级空手攻击伤害 0，怪不会反击。
        equipped = self.newbie_equip()
        self.ok("DR-1 穿上出生装", f"{equipped} 件")
        # 城里/安全区没有会反击的怪：按“相对当前位置”的八方向逐圈探索野外，不做坐标传送。
        aggressive_keys = ("oma",)

        def aggressive_here() -> list[dict]:
            return [
                m
                for m in self.monsters
                if any(key in m.get("name", "").lower() for key in aggressive_keys)
            ]

        hunt = aggressive_here()
        if not hunt:
            # 相对当前位置的几条刷怪带路线；不写绝对坐标、不传送。
            for ox, oy in ((90, -175), (0, 280), (-120, 200), (180, 140), (0, -200)):
                target = (self.self_pos[0] + ox, self.self_pos[1] + oy)
                if self.walk_towards(target, max_steps=220):
                    self._drain(1.0)
                hunt = aggressive_here()
                if not hunt:
                    hunt = [m for m in self.monsters if not m.get("dead")]
                if hunt:
                    break
        if not hunt:
            self.fail("DR-1 自然死亡", "几条野外路线均未发现活怪")
            return 1
        print("  [DR-1] 附近怪物：")
        for monster in self.monsters[:12]:
            print(f"    {monster.get('name')} id={monster.get('id')} pos={monster.get('pos')} d={self._distance(monster['pos'])}")

        # 1) 一次拉 5–6 只 Oma：对每只各打一下触发 retarget，然后站着挨打。
        aggressive = [m for m in self.monsters if "oma" in m.get("name", "").lower()]
        targets = sorted(aggressive or self.monsters, key=lambda m: self._distance(m["pos"]))[:6]
        for target in targets:
            latest = next((m for m in self.monsters if m["id"] == target["id"]), target)
            if latest.get("dead"):
                continue
            if self._distance(latest["pos"]) > 1:
                self.walk_towards(latest["pos"], blocked={tuple(latest["pos"])}, max_steps=40)
                self._drain(0.2)
                latest = next((m for m in self.monsters if m["id"] == target["id"]), latest)
            if self._distance(latest["pos"]) <= 1:
                dx = latest["pos"][0] - self.self_pos[0]
                dy = latest["pos"][1] - self.self_pos[1]
                print(f"  [DR-1] 拉怪攻击 {latest.get('name')} id={latest.get('id')} d=1")
                writer = Writer()
                writer.u8(self._direction(dx, dy))
                writer.u8(0)
                self.send(C_ATTACK, writer.raw())
                self._drain(0.45)
        initial_hp = self.self_hp or 0
        min_hp = initial_hp
        deadline = time.time() + 180
        last_attack = 0.0
        last_move = 0.0
        last_report = 0.0
        saw_damage = False
        while time.time() < deadline and not self.self_dead:
            now = time.time()
            alive = [
                m
                for m in self.monsters
                if "oma" in m.get("name", "").lower() and not m.get("dead")
            ]
            if not alive:
                alive = [m for m in self.monsters if not m.get("dead")]
            if not alive:
                self.fail("DR-1 自然死亡", "附近已没有活怪")
                return 1
            prey = min(alive, key=lambda m: self._distance(m["pos"]))
            d = self._distance(prey["pos"])
            age = now - prey.get("updated_at", 0.0)
            moving = age < 0.8 and prey.get("last_pos") != prey.get("pos")
            if now - last_report >= 1.0:
                print(
                    f"  [DR-1] 追怪：self={self.self_pos} prey_server={tuple(prey['pos'])} "
                    f"id={prey.get('id')} d={d} moving={moving} stale_ms={int(age * 1000)} hp={self.self_hp}"
                )
                last_report = now
            if d > 1:
                self.walk_towards(prey["pos"], blocked={tuple(prey["pos"])}, max_steps=30)
            elif now - last_attack >= 2.0:
                dx = prey["pos"][0] - self.self_pos[0]
                dy = prey["pos"][1] - self.self_pos[1]
                print(f"  [DR-1] 近身攻击 id={prey.get('id')} d=1 moving={moving}")
                writer = Writer()
                writer.u8(self._direction(dx, dy))
                writer.u8(0)
                self.send(C_ATTACK, writer.raw())
                last_attack = now
            if self.dr1_keep_moving and now - last_move >= 1.2:
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    target = (self.self_pos[0] + dx, self.self_pos[1] + dy)
                    if self._walkable_at(target) and self.step_onto(target):
                        print(f"  [DR-1] 走位挨打 {self.self_pos} hp={self.self_hp}")
                        break
                last_move = now
            if self.self_hp is not None:
                min_hp = min(min_hp, self.self_hp)
                if self.self_hp < initial_hp:
                    saw_damage = True
            pkt = self.next_packet(
                time.time() + min(0.2, max(0.01, deadline - time.time()))
            )
            if pkt:
                self._observe(*pkt)
        if not self.self_dead:
            print(f"  [DR-1] 包计数：{dict(self.dr1_counts)}")
            self.fail(
                "DR-1 自然死亡",
                f"180s 内未死亡（初始 hp={initial_hp} 最低 hp={min_hp} 受伤={saw_damage}，"
                f"怪攻击包={self.dr1_counts.get('monster_attack', 0)}，"
                f"Oma344攻击包={self.dr1_counts.get('oma344_attack', 0)}，"
                f"最近包={list(self.recent_packets)}）",
            )
            return 1
        death_at = self.death_at or time.time()
        self.ok("DR-1 自然死亡", f"hp=0 t={death_at:.3f} pos={self.self_pos}")

        # 2) 12s 等待：持续读包，不得出现 S.Revived（证明不是 10s 自动复活）。
        wait_end = death_at + 12.0
        early = None
        while time.time() < wait_end:
            pkt = self.next_packet(
                time.time() + min(0.25, max(0.01, wait_end - time.time()))
            )
            if pkt:
                index, payload = pkt
                self._observe(index, payload)
                if index == S_REVIVED and early is None:
                    early = time.time()
        if early is not None:
            self.fail("DR-1 12s 无自动复活", f"死亡后 {early - death_at:.3f}s 收到 S.Revived（期望 >12s 或只由客户端触发）")
            return 1
        self.ok("DR-1 12s 无自动复活", f"t={time.time():.3f}，仍未 S.Revived")

        # 3) 发 C.TownRevive，1s 内必须收到 S.Revived；记录先发后收。
        send_at = time.time()
        self.send(C_TOWNREVIVE, b"")
        revive_at = None
        deadline = send_at + 5.0
        while time.time() < deadline:
            pkt = self.next_packet(
                time.time() + min(0.1, max(0.01, deadline - time.time()))
            )
            if pkt:
                index, payload = pkt
                print(f"  [DR-1] 等待136 frame index={index} t={time.time():.3f}")
                self._observe(index, payload)
                if index == S_REVIVED:
                    revive_at = time.time()
                    break
        if revive_at is None:
            self.fail("DR-1 C.TownRevive", f"发送后 5s 未收到 S.Revived（最近包={list(self.recent_packets)}）")
            return 1
        self.ok("DR-1 C.TownRevive", f"发 t={send_at:.3f} → 收 t={revive_at:.3f}（{revive_at - send_at:.3f}s）")
        # S.Revived 只清状态；落点由随后的 S.UserLocation/MapChanged 驱动，再 drain 2s。
        revived_pos = self.self_pos
        pos_end = time.time() + 2.0
        while time.time() < pos_end:
            pkt = self.next_packet(time.time() + 0.1)
            if pkt:
                self._observe(*pkt)
            if self.self_pos != revived_pos:
                break

        # 4) 复活点/HP/可移动/可攻击。
        if self.self_hp is None or self.self_hp <= 0:
            self.fail("DR-1 复活 HP", f"复活后 HP={self.self_hp}")
            return 1
        # 服务端 S.Revived 不带落点；客户端位置要等下一次 S.UserLocation 才刷新。
        revived_pos = self.self_pos
        self.ok("DR-1 复活点/HP", f"pos(缓存)={revived_pos} hp={self.self_hp}")

        start_pos = self.self_pos
        moved = False
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1)):
            target = (start_pos[0] + dx, start_pos[1] + dy)
            self.step_onto(target)
            if self.self_pos != start_pos:
                moved = True
                break
        if not moved or self.self_pos == start_pos:
            self.fail("DR-1 复活后可移动", f"从 {start_pos} 未能移动（当前 {self.self_pos}）")
            return 1
        self.ok("DR-1 复活后可移动", f"{start_pos} -> {self.self_pos}")
        # 移动确认后服务端会回真实落点；此时核对回城/出生点（容差 3 格）。
        if self.entry_pos is not None and self._distance(self.entry_pos) > 3:
            self.fail(
                "DR-1 复活点",
                f"移动后 pos={self.self_pos}，距出生点 {self.entry_pos} >3 格",
            )
            return 1

        writer = Writer()
        writer.u8(0)
        writer.u8(0)
        self.send(C_ATTACK, writer.raw())
        deadline = time.time() + 0.5
        while time.time() < deadline:
            pkt = self.next_packet(max(0.01, deadline - time.time()))
            if pkt:
                self._observe(*pkt)
        self.ok("DR-1 复活后可攻击", "C.Attack 已发送，连接保持")

        print("  [DR-1] 包序（最近 20 条，时间/索引）：")
        for stamp, index in self.packet_log[-20:]:
            print(f"    t={stamp:.3f} index={index}")
        return 0

    def run(self) -> int:
        suffix = random.randint(100000, 999999)
        if self.gm:
            account = self.gm
            password = "gm12345"
            character = f"GM{suffix}"
        else:
            account = f"pt{suffix}"
            password = "abc12345"
            character = f"PT{suffix}"

        self.account = account
        self.character = character
        self.password = password
        self.connect()
        self.expect(S_CONNECTED, "连接")
        self.ok("连接服务端", f"{self.host}:{self.port}")

        writer = Writer()
        writer.bytes_prefixed(bytes(16))
        self.send(C_CLIENT_VERSION, writer.raw())
        _, payload = self.expect(S_CLIENT_VERSION, "版本校验")
        result = Reader(payload).u8()
        if result != 1:
            self.fail("版本校验", f"S.ClientVersion.Result={result}（期望 1）")
            return 1
        self.ok("版本校验")

        def try_login() -> tuple[int, bytes]:
            writer = Writer()
            writer.string(account)
            writer.string(password)
            self.send(C_LOGIN, writer.raw())
            return self.expect({S_LOGIN_SUCCESS, S_LOGIN}, "登录")

        login_index, payload = try_login()
        if login_index == S_LOGIN:
            # 账号不存在：注册后重登
            writer = Writer()
            writer.string(account)
            writer.string(password)
            writer.i64(0)
            writer.string("")
            writer.string("")
            writer.string("")
            writer.string("")
            self.send(C_NEW_ACCOUNT, writer.raw())
            _, payload = self.expect(S_NEW_ACCOUNT, "注册")
            result = Reader(payload).u8()
            if result != 8:
                self.fail("注册", f"S.NewAccount.Result={result}（期望 8）")
                return 1
            self.ok("注册账号", account)
            login_index, payload = try_login()
            if login_index == S_LOGIN:
                self.fail("登录", f"S.Login.Result={Reader(payload).u8()}")
                return 1
        self.ok("登录")

        reader = Reader(payload)
        characters = reader.i32()
        char_index = None
        if characters > 0:
            char_index = reader.i32()
        if characters <= 0:
            writer = Writer()
            writer.string(character)
            writer.u8(0)  # MirGender.Male
            writer.u8(0 if self.newbie else 2)  # 新手链走战士（quest 7 required=1）；其余仍 Wizard
            self.send(C_NEW_CHARACTER, writer.raw())
            index, created = self.expect({S_NEW_CHARACTER_SUCCESS, S_LOGIN_SUCCESS}, "创建角色")
            char_index = Reader(created).i32()
            self.ok("创建角色", character)
        else:
            self.ok("角色列表", f"{characters} 个角色")

        writer = Writer()
        writer.i32(char_index if char_index is not None else 0)
        self.send(C_START_GAME, writer.raw())
        _, payload = self.expect(S_START_GAME, "进入游戏")
        result = Reader(payload).u8()
        if result != 4:
            self.fail("进入游戏", f"S.StartGame.Result={result}（期望 4）")
            return 1
        self.ok("进入游戏")

        # 等实体列表（UserInformation + 至少一条 NPC/怪物）
        deadline = time.time() + self.timeout
        seen_ids = []
        while time.time() < deadline and (self.self_object_id is None or not (self.npcs or self.monsters)):
            packet = self.next_packet(deadline)
            if packet is None:
                break
            self._observe(packet[0], packet[1])
            seen_ids.append(packet[0])
        if self.self_object_id is None:
            self.fail("进入世界", f"未收到 S.UserInformation（期望 21）；已收到 {seen_ids[-15:]}")
            return 1
        if self.newbie:
            return self.newbie_flow(char_index if char_index is not None else 0)
        self.ok("进入世界", f"object_id={self.self_object_id} pos={self.self_pos}")
        self.entry_map, self.entry_pos = self.map_file, self.self_pos
        if self.sell1:
            return self.sell1_flow()
        if self.eq1:
            return self.eq1_flow()
        if self.quest1:
            return self.quest1_flow()
        if self.quest5:
            return self.quest5_flow()
        if self.tp1:
            return self.tp1_flow()
        if self.ps1:
            return self.ps1_flow(char_index if char_index is not None else 0)
        if self.dr1:
            return self.dr1_flow()
        # 幂等：上一轮的跨图传送会把角色留在别的图（NPC=0），每次运行先回起点。
        if not self.return_to_start(char_index if char_index is not None else 0):
            return 1

        # 走到 NPC 并对话
        if not self.npcs:
            self.fail("走到 NPC / 对话", "起始地图没有下发 S.ObjectNPC（服务端缺 NPC 资源）")
        else:
            npc = min(self.npcs, key=lambda n: self._distance(n["pos"]))
            reached = self.walk_towards(npc["pos"], blocked={npc["pos"]})
            if not reached:
                self.fail("走到 NPC", f"超时：未能走到 {npc['name']} @ {npc['pos']}")
            else:
                dx = npc["pos"][0] - self.self_pos[0]
                dy = npc["pos"][1] - self.self_pos[1]
                writer = Writer()
                writer.u8(self._direction(dx, dy))
                self.send(C_TURN, writer.raw())
                time.sleep(0.05)
                writer = Writer()
                writer.u32(npc["id"])
                writer.string("Main")
                self.send(C_CALL_NPC, writer.raw())
                try:
                    self.wait_for(S_NPC_RESPONSE, "NPC 对话", 5)
                    self.ok("走到 NPC / 对话", npc["name"])
                except ProtocolError as error:
                    self.fail("对话", f"{error}；期望 S.NPCResponse（93）")
        # 买药：走真实商店页（`S.NPCGoods` 102）→ `C.BuyItem`（51）→ `S.GainedItem`/`S.LoseGold`
        self.buy_potion()

        # 跨图传送：① 自然传送点（`@mapinfo` 拿到全部传送点、挑最近的走过去）→ ② 传送 NPC
        # → ③ GM `@map` 兜底。放在打怪之前：后面的 `@mob` 会刷出一群怪把周围格子占满
        # （服务端拒绝移动：`target cell is occupied`），那时再想自然走进城门就没路了。
        self.cross_map()

        if self.gm:
            self.gm_combat_flow()

        # 打一只鸡/鹿
        if not self.gm:
            def find_prey():
                return next(
                    (
                        m
                        for m in self.monsters
                        if not m["dead"] and any(k in m["name"] for k in ("鸡", "Hen", "鹿", "Deer"))
                    ),
                    None,
                )

            prey = find_prey()
            if prey is None:
                self.fail("打死一只鸡/鹿", f"起始视野内没有鸡/鹿类怪物（当前 {self.map_file}@{self.self_pos}，monsters={len(self.monsters)}）")
            if prey is not None:
                exp_before = self._gain_exp_count = 0
                reached = False
                for _ in range(20):
                    latest = next((m for m in self.monsters if m["id"] == prey["id"]), prey)
                    if latest.get("dead"):
                        break
                    prey = latest
                    distance = max(
                        abs(self.self_pos[0] - prey["pos"][0]),
                        abs(self.self_pos[1] - prey["pos"][1]),
                    )
                    if __import__('os').environ.get('PT_DEBUG'):
                        print(f'  chase self={self.self_pos} prey={prey["pos"]} dist={distance}')
                    if distance <= 1:
                        reached = True
                        break
                    if self.walk_towards(prey["pos"], max_steps=30):
                        reached = True
                        break
                if not reached:
                    self.fail("接近怪物", f"超时：未能走到 {prey['name']} @ {prey['pos']}")
                else:
                    killed = False
                    for _ in range(120):
                        latest = next((m for m in self.monsters if m["id"] == prey["id"]), prey)
                        if latest.get("dead"):
                            killed = True
                            break
                        prey = latest
                        distance = max(
                            abs(self.self_pos[0] - prey["pos"][0]),
                            abs(self.self_pos[1] - prey["pos"][1]),
                        )
                        if distance > 1:
                            self.walk_towards(prey["pos"], max_steps=3)
                            continue
                        dx = prey["pos"][0] - self.self_pos[0]
                        dy = prey["pos"][1] - self.self_pos[1]
                        writer = Writer()
                        writer.u8(self._direction(dx, dy))
                        writer.u8(0)  # Spell.None
                        self.send(C_ATTACK, writer.raw())
                        deadline_attack = time.time() + 1.2
                        while True:
                            packet = self.next_packet(deadline_attack)
                            if packet is None:
                                break
                            index, payload = packet
                            self._observe(index, payload)
                            if index == S_GAIN_EXPERIENCE:
                                self._gain_exp_count += 1
                            if index == S_OBJECT_DIED:
                                died_id = Reader(payload).u32()
                                if died_id == prey["id"]:
                                    killed = True
                                    break
                        if killed:
                            break
                    if killed or self._gain_exp_count > 0:
                        self.ok("打死一只鸡/鹿", f"{prey['name']}（收到经验包 {self._gain_exp_count} 次）")
                    else:
                        self.fail("打死一只鸡/鹿", "40 次攻击后未收到 S.ObjectDied（81）/S.GainExperience（85）")

            self.fail("使用技能", "新建角色未学习技能（技能书仍在背包时需先 C.UseItem 读书）")
        self.ok("开背包/关闭", "客户端本地 UI（协议无背包开关包）")

        writer = Writer()
        self.send(C_LOGOUT, writer.raw())
        try:
            self.expect({S_CONNECTED, 2, 7}, "下线", 3)
        except ProtocolError:
            pass
        self.ok("下线")
        return 0 if all(kind != "FAIL" for kind, _, _ in self.results) else 1

    def _distance(self, pos: tuple[int, int]) -> int:
        if self.self_pos is None:
            return 10**9
        return max(abs(self.self_pos[0] - pos[0]), abs(self.self_pos[1] - pos[1]))


def self_check() -> int:
    """脚本自检：`@mapinfo` 的传送点行解析 + 结果文案（不连服务端）。"""
    samples = [
        ("    (326,288) -> 0108(#4) (10,12) dist=18", (326, 288), "0108", 4, (10, 12), 18, ""),
        ("    (400,225) -> 0120(#20) (11,15) dist=74 [bigmap]", (400, 225), "0120", 20, (11, 15), 74, "bigmap"),
        ("    (335,299) -> 0103(#10) (5,12) dist=29 [hole]", (335, 299), "0103", 10, (5, 12), 29, "hole"),
        ("    (317,301) -> 0106(#12) (20,12) dist=31 [bigmap] [move]", (317, 301), "0106", 12, (20, 12), 31, "bigmap+move"),
    ]
    failures = []
    for line, pos, map_file, index, target, dist, marks in samples:
        match = Playtest.MOVEMENT_LINE.match(line.strip())
        if not match:
            failures.append(f"解析失败：{line}")
            continue
        got = {
            "pos": (int(match.group("x")), int(match.group("y"))),
            "map": match.group("map"),
            "index": int(match.group("index")),
            "target": (int(match.group("tx")), int(match.group("ty"))),
            "dist": int(match.group("dist")),
        }
        expected = {"pos": pos, "map": map_file, "index": index, "target": target, "dist": dist}
        if got != expected:
            failures.append(f"{line} → {got}，期望 {expected}")
    # 结果文案三条分支
    if summarize([("OK", "连接", "")], 0) != ["结果：全部通过（0 跳过）"]:
        failures.append("0 跳过文案不对")
    skipped = summarize([("OK", "连接", ""), ("SKIP", "打怪", "无怪")], 0)
    if skipped[0] != "结果：全部通过（含 1 个跳过项）" or "打怪" not in skipped[1]:
        failures.append(f"含跳过项文案不对：{skipped}")
    failed = summarize([("OK", "连接", ""), ("FAIL", "买药", "没开商店页")], 1)
    if failed[0] != "结果：存在失败项（OK 1 / SKIP 0 / FAIL 1）" or "买药" not in failed[1]:
        failures.append(f"失败文案不对：{failed}")
    if failures:
        for line in failures:
            print(f"[FAIL] {line}")
        return 1
    print("[OK] 自检通过：@mapinfo 传送点行解析 4/4、结果文案 3/3")
    return 0


def summarize(results: list[tuple[str, str, str]], code: int) -> list[str]:
    """结果汇总文案（纯函数，便于单测文案）。

    * 0 跳过 → `结果：全部通过（0 跳过）`
    * N 跳过 → `结果：全部通过（含 N 个跳过项）` + 逐条列出
    * 有失败 → `结果：存在失败项（OK x / SKIP y / FAIL z）` + 逐条列出
    """
    counts: dict[str, int] = collections.Counter(kind for kind, _, _ in results)
    skipped = [(step, detail) for kind, step, detail in results if kind == "SKIP"]
    failed = [(step, detail) for kind, step, detail in results if kind == "FAIL"]
    lines: list[str] = []
    if code == 0:
        if skipped:
            lines.append(f"结果：全部通过（含 {len(skipped)} 个跳过项）")
            lines.extend(f"  - [SKIP] {step} —— {reason}" for step, reason in skipped)
        else:
            lines.append("结果：全部通过（0 跳过）")
    else:
        lines.append(
            "结果：存在失败项"
            f"（OK {counts.get('OK', 0)} / SKIP {len(skipped)} / FAIL {len(failed)}）"
        )
        lines.extend(f"  - [FAIL] {step} —— {reason}" for step, reason in failed)
    return lines


def main() -> int:
    parser = argparse.ArgumentParser(description="传奇-Next 可玩性自检")
    parser.add_argument("--server", default="127.0.0.1:7000", help="服务端地址 host:port")
    parser.add_argument("--timeout", type=float, default=8.0, help="每步等待超时（秒）")
    parser.add_argument("--gm", default=None, help="GM 账号（服务端 MIR2_GM_ACCOUNTS 需包含它）；提供时走 @level/@mob/@learn 闭环")
    parser.add_argument("--newbie", action="store_true", help="非 GM「新手 10 分钟」完整链路（出生装/打 3 只/买书读书/捡物/重登）")
    parser.add_argument("--dr1", action="store_true", help="DR-1 死亡→12s 无自动复活→C.TownRevive→复活/可移动/可攻击")
    parser.add_argument("--ps1", action="store_true", help="PS-1 进程级重登持久化（金币/装备 unique_id+耐久/技能/任务）")
    parser.add_argument("--tp1", action="store_true", help="TP-1 同图导航：接 quest 5 → 断言导航行 → 走到 movement 门格")
    parser.add_argument("--quest5", action="store_true", help="quest 5 完整任务：Smith 接取→杀 10 Deer+10 Scarecrow→交付→奖励")
    parser.add_argument("--quest1", action="store_true", help="quest 1 完整任务：Jane 接取→任务物品栏→CraftsLady 交付→经验+50(cjlaaa)；--expected-exp 可改")
    parser.add_argument("--expected-exp", type=int, default=50, help="quest1 期望经验（cjlaaa=50，Jev=10）")
    parser.add_argument("--eq1", action="store_true", help="EQ-1 耐久：卖物→穿武器打怪掉耐久→Smith 修理→扣款")
    parser.add_argument("--sell1", action="store_true", help="卖肉链：杀 Hen→捡 Chicken→Sarah 卖→金币→eq1 修理")
    parser.add_argument("--dr1-keep-moving", action="store_true", help="DR-1 等待死亡时持续小范围走位（验证“移动才挨打”假设）")
    parser.add_argument(
        "--self-check",
        action="store_true",
        help="只跑脚本内自检（@mapinfo 传送点行解析 + 结果文案），不连服务端",
    )
    parser.add_argument(
        "--start-map",
        default=",".join(DEFAULT_START_MAPS),
        help="起始图标识候选（逗号分隔，`@map` 依次尝试；默认 '0,1' = 比奇的 map_index 与文件名）",
    )
    parser.add_argument(
        "--start-point",
        default=f"{DEFAULT_START_POINT[0]},{DEFAULT_START_POINT[1]}",
        help="起始坐标 x,y（默认 330,270 = ServerConfig.start_point）",
    )
    args = parser.parse_args()
    if args.self_check:
        return self_check()

    try:
        start_point = tuple(int(part) for part in args.start_point.split(","))
        if len(start_point) != 2:
            raise ValueError
    except ValueError:
        parser.error("--start-point 需要形如 330,270")
    test = Playtest(
        args.server,
        args.timeout,
        args.gm,
        args.newbie,
        tuple(part.strip() for part in args.start_map.split(",") if part.strip()),
        start_point,
    )
    test.dr1 = args.dr1
    test.ps1 = args.ps1
    test.tp1 = args.tp1
    test.quest5 = args.quest5
    test.quest1 = args.quest1
    test.expected_quest1_exp = args.expected_exp
    test.eq1 = args.eq1
    test.sell1 = args.sell1
    test.dr1_keep_moving = args.dr1_keep_moving
    try:
        code = test.run()
    except (OSError, ProtocolError) as error:
        test.fail("连接/协议", str(error))
        code = 1
    finally:
        test.close()
    if test.newbie:
        if any(kind == "SKIP" for kind, _, _ in test.results):
            code = 1
        if time.time() - test.run_started_at > 30 * 60:
            code = 1
    print()
    for line in summarize(test.results, code):
        print(line)
    return code


if __name__ == "__main__":
    sys.exit(main())


