#!/usr/bin/env python3
"""《传奇-Next》可玩性自检（playtest）——只依赖 Python 标准库。

连接真实服务端（默认 127.0.0.1:7000），按「新玩家 10 分钟能玩到的东西」逐项验证并
打印 [OK]/[FAIL]/[SKIP]；任一项失败时退出码为 1，全部通过/跳过为 0。

用法：
    python tools/playtest/playtest.py
    python tools/playtest/playtest.py --server 127.0.0.1:7000 --timeout 8

设计要点：
* 所有等待都有超时（默认 8s），超时时打印「期望收到什么包」的诊断；
* 不发 KeepAlive、不 sleep 死等：每次等待都是「收包直到目标包或超时」；
* 服务端缺资源（例如起始地图没有 NPC/鸡鹿）时该项打印 [SKIP] 并说明原因。

协议：帧 = u16 总长（含 4 字节头）+ i16 包号 + 负载；字符串为 7-bit 变长长度 + UTF-8；
标了 Compressed 的包（NPCGoods 等）负载是 gzip。
"""

from __future__ import annotations

import argparse
import collections
import gzip
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
C_CALL_NPC = 50
C_CHAT = 13
C_MAGIC = 58
C_BUY_ITEM = 51

#: 比奇药店最便宜的初级红药（`Server.MirDB` 里 `(HP)DrugSmall` 的模板号）。
POTION_ITEM_INDEX = 658

# --- 服务端包号（server.rs / manual/server.rs） ---
S_CONNECTED = 0
S_CLIENT_VERSION = 1
S_NEW_ACCOUNT = 4
S_LOGIN = 7
S_LOGIN_SUCCESS = 9
S_NEW_CHARACTER_SUCCESS = 11
S_START_GAME = 14
S_MAP_INFORMATION = 17
S_USER_INFORMATION = 21
S_USER_LOCATION = 23
S_OBJECT_WALK = 28
S_CHAT = 30
S_OBJECT_MONSTER = 71
S_OBJECT_DIED = 81
S_GAIN_EXPERIENCE = 85
S_NEW_MAGIC = 117
S_OBJECT_MAGIC = 120
S_OBJECT_NPC = 92
S_NPC_RESPONSE = 93
S_NEW_MAP_INFO = 18
S_GAINED_ITEM = 66
S_GAINED_GOLD = 67
S_LOSE_GOLD = 68
S_MAP_CHANGED = 98
S_NPC_GOODS = 102

DIRECTIONS = ["Up", "UpRight", "Right", "DownRight", "Down", "DownLeft", "Left", "UpLeft"]


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
    def __init__(self, server: str, timeout: float, gm: str | None = None):
        host, _, port = server.partition(":")
        self.host = host
        self.port = int(port or "7000")
        self.timeout = timeout
        self.gm = gm
        self.sock: socket.socket | None = None
        self.buffer = bytearray()
        self.self_object_id: int | None = None
        self.self_pos: tuple[int, int] | None = None
        self.npcs: list[dict] = []
        self.monsters: list[dict] = []
        self.recent_packets: collections.deque[int] = collections.deque(maxlen=20)
        self.health_history: list[tuple[float, int]] = []
        self.last_userinfo_hp: int | None = None
        self.self_hp: int | None = None
        self.chats: list[str] = []
        self.learned_spells: set[int] = set()
        self.self_mp: int | None = None
        self.map_file: str | None = None
        self._walkable: list[bytearray] | None = None
        self._map_size: tuple[int, int] | None = None
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
        self.map_index: int | None = None
        self.map_size: tuple[int, int] | None = None

    # ---------------- 结果输出 ----------------
    def ok(self, step: str, detail: str = "") -> None:
        self.results.append(("OK", step, detail))
        print(f"[OK]   {step}" + (f" —— {detail}" if detail else ""))

    def skip(self, step: str, reason: str) -> None:
        self.results.append(("SKIP", step, reason))
        print(f"[SKIP] {step} —— {reason}")

    def fail(self, step: str, detail: str) -> None:
        self.results.append(("FAIL", step, detail))
        print(f"[FAIL] {step} —— {detail}")

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

    def next_packet(self, deadline: float) -> tuple[int, bytes] | None:
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
    def _observe(self, index: int, payload: bytes) -> None:
        self.recent_packets.append(index)
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
                self.npcs.append({"id": object_id, "name": name, "pos": location})
            elif index == S_OBJECT_MONSTER:
                r = Reader(payload)
                object_id = r.u32()
                name = r.string()
                r.i32()  # name colour
                location = r.point()
                if not any(m["id"] == object_id for m in self.monsters):
                    self.monsters.append({"id": object_id, "name": name, "pos": location, "dead": False})
            elif index == S_OBJECT_DIED:
                r = Reader(payload)
                object_id = r.u32()
                for monster in self.monsters:
                    if monster["id"] == object_id:
                        monster["dead"] = True
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
            elif index == 98:  # S.MapChanged
                # 字段顺序照 `Shared/ServerPackets.cs:2979-3025`：map_index, file_name,
                # title, mini_map(u16), big_map(u16), **lights(u8)**, location, direction,
                # map_dark_light, music(u16), weather(u16)。
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
                self.npcs.clear()
                self.monsters.clear()
                print(f"  [地图] 切换地图 {self.map_file} @ {self.self_pos}")
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
                self.movements = movements
                self.map_size = (width, height)
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
            elif index == S_GAINED_ITEM:  # 66：到手物品
                r = Reader(payload)
                self.last_gained_items.append(r.user_item())
            elif index == S_LOSE_GOLD:  # 68：金币减少
                r = Reader(payload)
                gold = r.u32()
                self.gold_delta -= gold
                print(f"  [金币] -{gold}（累计 {self.gold_delta:+d}）")
            elif index == S_GAINED_GOLD:  # 67：金币增加
                r = Reader(payload)
                gold = r.u32()
                self.gold_delta += gold
                print(f"  [金币] +{gold}（累计 {self.gold_delta:+d}）")
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
                        monster["pos"] = location
        except (ProtocolError, struct.error):
            pass

    # ---------------- 动作 ----------------
    def wait_for(self, wanted: set[int] | int, step: str, timeout: float | None = None):
        return self.expect(wanted, step, timeout)

    def diagnostics(self, reason: str) -> None:
        print(f"  [诊断] {reason}")
        print(f"  [诊断] 最近包索引: {list(self.recent_packets)}")
        print(f"  [诊断] UserInformation.hp={self.last_userinfo_hp} 当前 hp={self.self_hp}")
        print(f"  [诊断] HealthChanged 序列: {self.health_history[-10:]}")

    def _load_map(self) -> None:
        if self._walkable is not None or not self.map_file:
            return
        root = pathlib.Path(__file__).resolve().parents[2]
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

    def goto(self, target: tuple[int, int], max_steps: int = 160) -> bool:
        """走到目标旁一格：近处用 BFS（`walk_towards`，绕开建筑），远处先贪心再 BFS 兜底。"""
        if self._distance(target) <= 1:
            return True
        if self._distance(target) <= 40:
            return self.walk_towards(target, max_steps=max_steps)
        if self.walk_to(target, max_steps=max_steps // 2):
            return True
        return self.walk_towards(target, max_steps=max_steps)

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
        return self.goto(npc["pos"], max_steps=160)

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
        # ① 自然传送点：优先，但只在步行预算内尝试。
        nearest = sorted(
            self.movements,
            key=lambda mv: self._distance(mv["pos"]),
        )
        if nearest and self._distance(nearest[0]["pos"]) <= self.NATURAL_MOVEMENT_BUDGET:
            movement = nearest[0]
            if self._take_movement(movement, start_map, start_pos):
                return True
        # ② 城镇里的传送 NPC（比奇有 Teleport_Jason/Harrison/Kyle…，脚本用 `MOVE` 切图）。
        if self._teleport_by_npc(start_map, start_pos):
            return True
        reason = "最近的自然传送点 " + (
            f"{nearest[0]['pos']} 距起点 {self._distance(nearest[0]['pos'])} 格（超过 "
            f"{self.NATURAL_MOVEMENT_BUDGET} 格预算）" if nearest else "缺失（服务端没下发 movements）"
        )
        # ③ 兜底：GM `@map`。
        return self._cross_map_via_gm(reason)

    def _take_movement(self, movement: dict, start_map, start_pos) -> bool:
        """走到传送点格子上 → 断言 `S.MapChanged` / 落点正确。"""
        source = movement["pos"]
        if not self.goto(source, max_steps=self.NATURAL_MOVEMENT_BUDGET + 10):
            self.diagnostics(f"走不到传送点 {source}")
            return False
        dx = source[0] - self.self_pos[0]
        dy = source[1] - self.self_pos[1]
        if (dx, dy) != (0, 0):
            writer = Writer()
            writer.u8(self._direction(dx, dy))
            self.send(C_WALK, writer.raw())
        deadline = time.time() + self.timeout
        while time.time() < deadline:
            packet = self.next_packet(deadline)
            if packet is None:
                break
            index, payload = packet
            self._observe(index, payload)
            if index == S_MAP_CHANGED:
                if self.map_index != movement["destination"]:
                    self.fail(
                        "跨图传送",
                        f"目标地图 #{movement['destination']}，实际 #{self.map_index}",
                    )
                    return False
                if not self._walkable_at(self.self_pos):
                    self.fail("跨图传送", f"落点 {self.self_pos} 不可站立（{self.map_file}）")
                    return False
                self.ok(
                    "跨图传送",
                    f"自然传送点 {start_map} {source} → {self.map_file} @ {self.self_pos}",
                )
                return True
        self.diagnostics(f"踩传送点 {source} 后 {self.timeout:.0f}s 内没收到 S.MapChanged")
        return False

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

    def walk_towards(
        self,
        target: tuple[int, int],
        max_steps: int = 200,
        blocked: set[tuple[int, int]] | None = None,
    ) -> bool:
        blocked = blocked or set()
        if self.self_pos is None:
            return False
        entity_cells = {tuple(n["pos"]) for n in self.npcs}
        entity_cells |= {tuple(m["pos"]) for m in self.monsters if not m["dead"] and tuple(m["pos"]) != target}
        local_blocked = set(entity_cells) | blocked
        for _ in range(max_steps):
            if max(abs(self.self_pos[0] - target[0]), abs(self.self_pos[1] - target[1])) <= 1:
                return True
            path = self._bfs_path(target, local_blocked)
            if path is None:
                if __import__('os').environ.get('PT_DEBUG'):
                    print(f'  BFS no path from {self.self_pos} to {target}')
                self.diagnostics(f"BFS 无路 {self.self_pos}->{target}")
                return False
            before = self.self_pos
            nxt = path[0]
            if __import__('os').environ.get('PT_DEBUG'):
                print(f'  step {before}->{nxt} target={target} pathlen={len(path)}')
            direction = self._direction(nxt[0] - before[0], nxt[1] - before[1])
            moved = False
            for attempt in range(3):
                writer = Writer()
                writer.u8(direction)
                self.send(C_WALK, writer.raw())
                deadline = time.time() + 2.5
                while True:
                    packet = self.next_packet(deadline)
                    if packet is None:
                        break
                    index, payload = packet
                    self._observe(index, payload)
                    if index == S_USER_LOCATION:
                        if self.self_pos != before:
                            moved = True
                            break
                        break
                    if index == S_OBJECT_WALK:
                        reader = Reader(payload)
                        object_id = reader.u32()
                        location = reader.point()
                        if object_id == self.self_object_id:
                            self.self_pos = location
                            moved = True
                            break
                if moved:
                    break
                time.sleep(0.3)
            if not moved:
                local_blocked.add(nxt)
                continue
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

    # ---------------- GM 闭环 ----------------
    def send_chat(self, message: str) -> None:
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
            writer.u8(2)  # MirClass.Wizard
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
        self.ok("进入世界", f"object_id={self.self_object_id} pos={self.self_pos}")

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
                # 不在城里长距离寻路（会踩到传送点换图）；视野内没有鸡/鹿就先跳过，
                # 服务端战斗已由 tests/skills.rs / duo_smoke 覆盖。
                self.skip("打死一只鸡/鹿", "起始视野内没有鸡/鹿类怪物（不跨图寻路，避免踩传送点）")
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

            self.skip("使用技能", "新建角色未学习技能（技能书在背包，需客户端 UI 学习）")
        self.ok("开背包/关闭", "客户端本地 UI（协议无背包开关包）")

        # 跨图传送：优先自然传送点（S.NewMapInfo 下发的 movements），GM @map 只作兜底。
        self.cross_map()

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


def main() -> int:
    parser = argparse.ArgumentParser(description="传奇-Next 可玩性自检")
    parser.add_argument("--server", default="127.0.0.1:7000", help="服务端地址 host:port")
    parser.add_argument("--timeout", type=float, default=8.0, help="每步等待超时（秒）")
    parser.add_argument("--gm", default=None, help="GM 账号（服务端 MIR2_GM_ACCOUNTS 需包含它）；提供时走 @level/@mob/@learn 闭环")
    args = parser.parse_args()

    test = Playtest(args.server, args.timeout, args.gm)
    try:
        code = test.run()
    except (OSError, ProtocolError) as error:
        test.fail("连接/协议", str(error))
        code = 1
    finally:
        test.close()
    print()
    if code == 0:
        print("结果：全部通过（含跳过项）")
    else:
        print("结果：存在失败项")
    return code


if __name__ == "__main__":
    sys.exit(main())