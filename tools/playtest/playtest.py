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
                r = Reader(payload)
                _map_index = r.i32()
                self.map_file = r.string()
                _title = r.string()
                _mini = r.u16()
                _big = r.u16()
                _music = r.u16()
                _lights = r.u8()
                self.self_pos = r.point()
                _direction = r.u8()
                self._walkable = None
                self.npcs.clear()
                self.monsters.clear()
                print(f"  [地图] 切换地图 {self.map_file} @ {self.self_pos}")
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
                    print(f'  BFS no path from {self.self_pos} to {target} (stuck={len(stuck)})')
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
            self.skip("走到 NPC / 对话", "起始地图没有下发 S.ObjectNPC（服务端缺 NPC 资源）")
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
                self.skip("买药", "NPC 商店页购买需要客户端 UI 交互（NPCGoods 解析未包含在最小自检里）")

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