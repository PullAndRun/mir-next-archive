#!/usr/bin/env python3
"""《传奇-Next》并发压测（stress）—— 只依赖 Python 标准库。

模拟 N 个真实 TCP 连接：每个连接走完整的「版本 → 注册/登录 → 建角/选角 → 进世界」
握手，然后按固定频率走动 / 攻击 / 发聊天，同时每秒抓一次服务端 `/metrics`
（Prometheus 文本）与进程 RSS/CPU，最后把结论写成 JSON 并打印表格。

用法（**服务端必须用 `tools/dev/serve.ps1` 隐藏起，别在前台弹窗**）：

    # 1) 起服务端（隐藏窗口，日志 target/server-7000.log）
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 -Port 7000
    # 2) 压测（需要 /metrics：起服务端前设置 MIR2_METRICS_LISTEN）
    py tools/stress/stress.py --clients 100 --duration 60

`MIR2_METRICS_LISTEN` 没配也能跑，只是 tick/DB 指标会是 `null`（连接/丢包统计仍然可用）。

输出的 JSON：`target/stress-<clients>.json`，字段见 `report()`。
"""

from __future__ import annotations

import argparse
import ctypes
import importlib.util
import json
import os
import random
import socket
import statistics
import struct
import sys
import threading
import time
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def load_playtest():
    """复用 playtest 的协议实现（Reader/Writer/包号），避免两处协议口径漂移。"""
    path = os.path.join(ROOT, "tools", "playtest", "playtest.py")
    spec = importlib.util.spec_from_file_location("mir_playtest", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


pt = load_playtest()

#: 走动方向（MirDirection：0=Up,1=UpRight,2=Right,3=DownRight,4=Down,5=DownLeft,6=Left,7=UpLeft）
WALK_DIRECTIONS = (0, 2, 4, 6, 1, 3, 5, 7)


# ---------------------------------------------------------------------------
# 进程指标（Windows：psapi + kernel32，纯 ctypes，不引第三方库）
# ---------------------------------------------------------------------------
class ProcessProbe:
    """读服务端进程的 RSS 与 CPU 时间（拿不到就返回 None，不影响压测）。"""

    def __init__(self, pid: int | None):
        self.pid = pid
        self._handle = None
        if pid and os.name == "nt":
            try:
                kernel32 = ctypes.windll.kernel32
                # PROCESS_QUERY_INFORMATION(0x0400) | PROCESS_VM_READ(0x0010)
                self._handle = kernel32.OpenProcess(0x0410, False, pid)
            except Exception:  # noqa: BLE001 - 指标拿不到不该打断压测
                self._handle = None
        self._last_cpu = None
        self._last_at = None

    def rss_bytes(self) -> int | None:
        if not self._handle:
            return None
        try:
            class PROCESS_MEMORY_COUNTERS(ctypes.Structure):
                _fields_ = [
                    ("cb", ctypes.c_ulong),
                    ("PageFaultCount", ctypes.c_ulong),
                    ("PeakWorkingSetSize", ctypes.c_size_t),
                    ("WorkingSetSize", ctypes.c_size_t),
                    ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                    ("QuotaPagedPoolUsage", ctypes.c_size_t),
                    ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                    ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                    ("PagefileUsage", ctypes.c_size_t),
                    ("PeakPagefileUsage", ctypes.c_size_t),
                ]

            counters = PROCESS_MEMORY_COUNTERS()
            counters.cb = ctypes.sizeof(counters)
            if ctypes.windll.psapi.GetProcessMemoryInfo(
                self._handle, ctypes.byref(counters), counters.cb
            ):
                return int(counters.WorkingSetSize)
        except Exception:  # noqa: BLE001
            pass
        return None

    def cpu_percent(self) -> float | None:
        """两次采样之间的 CPU 占用（单核百分比，可 >100）。"""
        if not self._handle:
            return None
        try:
            class FILETIME(ctypes.Structure):
                _fields_ = [("dwLowDateTime", ctypes.c_ulong), ("dwHighDateTime", ctypes.c_ulong)]

            creation, exit_, kernel, user = FILETIME(), FILETIME(), FILETIME(), FILETIME()
            if not ctypes.windll.kernel32.GetProcessTimes(
                self._handle,
                ctypes.byref(creation),
                ctypes.byref(exit_),
                ctypes.byref(kernel),
                ctypes.byref(user),
            ):
                return None

            def to_int(ft):
                return (ft.dwHighDateTime << 32) | ft.dwLowDateTime

            total = (to_int(kernel) + to_int(user)) / 1e7  # 100ns → 秒
            now = time.time()
            if self._last_cpu is None or self._last_at is None:
                self._last_cpu, self._last_at = total, now
                return None
            cpu = (total - self._last_cpu) / max(1e-6, now - self._last_at) * 100.0
            self._last_cpu, self._last_at = total, now
            return cpu
        except Exception:  # noqa: BLE001
            return None


def read_pid(port: int) -> int | None:
    path = os.path.join(ROOT, "target", f"server-{port}.pid")
    try:
        with open(path, encoding="utf-8") as handle:
            return int(handle.read().strip())
    except (OSError, ValueError):
        return None


# ---------------------------------------------------------------------------
# /metrics 抓取
# ---------------------------------------------------------------------------
def scrape_metrics(url: str, timeout: float = 2.0) -> dict[str, float]:
    try:
        with urllib.request.urlopen(url, timeout=timeout) as response:  # noqa: S310 - 本机回环
            text = response.read().decode("utf-8", "replace")
    except OSError:
        return {}
    out: dict[str, float] = {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        name, _, value = line.partition(" ")
        try:
            out[name] = float(value)
        except ValueError:
            continue
    return out


# ---------------------------------------------------------------------------
# 单个虚拟玩家
# ---------------------------------------------------------------------------
class Worker(threading.Thread):
    def __init__(self, index: int, server: tuple[str, int], duration: float, rate: float):
        super().__init__(daemon=True)
        self.index = index
        self.host, self.port = server
        self.duration = duration
        self.rate = rate
        self.connected = False
        self.entered = False
        self.error: str | None = None
        self.packets = 0
        self.disconnects = 0
        self.object_id: int | None = None
        self._sock: socket.socket | None = None
        self._buffer = bytearray()
        self._stop = threading.Event()

    # ---------------- 底层收发 ----------------
    def _send(self, index: int, payload: bytes) -> None:
        assert self._sock is not None
        total = 4 + len(payload)
        self._sock.sendall(struct.pack("<Hh", total, index) + payload)

    def _read_packet(self, deadline: float) -> tuple[int, bytes] | None:
        while True:
            if len(self._buffer) >= 4:
                total, index = struct.unpack_from("<Hh", self._buffer, 0)
                if len(self._buffer) >= total:
                    payload = bytes(self._buffer[4:total])
                    del self._buffer[:total]
                    return index, payload
            remaining = deadline - time.time()
            if remaining <= 0:
                return None
            assert self._sock is not None
            self._sock.settimeout(min(0.2, remaining))
            try:
                chunk = self._sock.recv(65536)
            except socket.timeout:
                continue
            if not chunk:
                raise ConnectionError("服务端关闭了连接")
            self._buffer += chunk

    def _drain(self, seconds: float) -> None:
        deadline = time.time() + seconds
        while True:
            packet = self._read_packet(deadline)
            if packet is None:
                return
            self.packets += 1

    def _wait_for(self, index: int, timeout: float) -> bytes | None:
        deadline = time.time() + timeout
        while True:
            packet = self._read_packet(deadline)
            if packet is None:
                return None
            self.packets += 1
            if packet[0] == index:
                return packet[1]

    # ---------------- 握手 ----------------
    def _handshake(self) -> None:
        account = f"st{self.index:03d}{random.randint(1000, 9999)}"
        password = "abc12345"
        char_name = f"ST{self.index:03d}{random.randint(100, 999)}"

        self._sock = socket.create_connection((self.host, self.port), timeout=10)
        self.connected = True
        self._sock.settimeout(10)
        if self._wait_for(pt.S_CONNECTED, 10) is None:
            raise ConnectionError("没有 S.Connected")

        writer = pt.Writer()
        writer.bytes_prefixed(bytes(16))
        self._send(pt.C_CLIENT_VERSION, writer.raw())
        if self._wait_for(pt.S_CLIENT_VERSION, 10) is None:
            raise ConnectionError("没有 S.ClientVersion")

        writer = pt.Writer()
        writer.string(account)
        writer.string(password)
        writer.i64(0)
        writer.string("")
        writer.string("")
        writer.string("")
        writer.string("")
        self._send(pt.C_NEW_ACCOUNT, writer.raw())
        self._wait_for(pt.S_NEW_ACCOUNT, 10)

        writer = pt.Writer()
        writer.string(account)
        writer.string(password)
        self._send(pt.C_LOGIN, writer.raw())
        payload = self._wait_for(pt.S_LOGIN_SUCCESS, 10)
        if payload is None:
            raise ConnectionError("登录失败（没收到 S.LoginSuccess）")
        characters = pt.Reader(payload).i32()
        char_index = None
        if characters > 0:
            char_index = pt.Reader(payload).i32()

        if char_index is None:
            writer = pt.Writer()
            writer.string(char_name)
            writer.u8(0)  # MirGender.Male
            writer.u8(0)  # MirClass.Warrior
            self._send(pt.C_NEW_CHARACTER, writer.raw())
            payload = self._wait_for(pt.S_NEW_CHARACTER_SUCCESS, 10)
            if payload is None:
                raise ConnectionError("建角失败")
            char_index = pt.Reader(payload).i32()

        writer = pt.Writer()
        writer.i32(char_index)
        self._send(pt.C_START_GAME, writer.raw())
        payload = self._wait_for(pt.S_START_GAME, 15)
        if payload is None:
            raise ConnectionError("进游戏超时（没有 S.StartGame）")
        result = pt.Reader(payload).u8()
        if result != 4:
            raise ConnectionError(f"S.StartGame.Result={result}（期望 4）")
        # 等 UserInformation（进世界）
        deadline = time.time() + 15
        while time.time() < deadline:
            payload = self._wait_for(pt.S_USER_INFORMATION, max(0.1, deadline - time.time()))
            if payload is None:
                break
            self.object_id = pt.Reader(payload).u32()
            self.entered = True
            break
        if not self.entered:
            raise ConnectionError("进世界超时（没有 S.UserInformation）")

    # ---------------- 负载 ----------------
    def run(self) -> None:
        try:
            self._handshake()
        except (OSError, ConnectionError) as error:
            self.error = str(error)
            self._close()
            return
        interval = 1.0 / max(0.1, self.rate)
        end = time.time() + self.duration
        step = 0
        try:
            while time.time() < end and not self._stop.is_set():
                # 走动 / 攻击 / 聊天：1:1:0.5 的混合比例
                kind = step % 5
                try:
                    if kind in (0, 1):
                        writer = pt.Writer()
                        writer.u8(WALK_DIRECTIONS[step % len(WALK_DIRECTIONS)])
                        self._send(pt.C_WALK, writer.raw())
                    elif kind in (2, 3):
                        writer = pt.Writer()
                        writer.u8(WALK_DIRECTIONS[step % len(WALK_DIRECTIONS)])
                        writer.u8(0)  # Spell.None
                        self._send(pt.C_ATTACK, writer.raw())
                    else:
                        writer = pt.Writer()
                        writer.string(f"stress-{self.index}-{step}")
                        writer.i32(0)
                        self._send(pt.C_CHAT, writer.raw())
                except OSError as error:
                    self.disconnects += 1
                    self.error = f"发送失败：{error}"
                    break
                self._drain(interval)
                step += 1
        finally:
            self._close()

    def _close(self) -> None:
        if self._sock is not None:
            try:
                self._sock.close()
            except OSError:
                pass
            self._sock = None

    def stop(self) -> None:
        self._stop.set()


# ---------------------------------------------------------------------------
# 主流程
# ---------------------------------------------------------------------------
def percentile(values: list[float], q: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, int(round(q * (len(ordered) - 1)))))
    return ordered[index]


def run(args) -> dict:
    host, _, port_text = args.server.partition(":")
    port = int(port_text or "7000")
    metrics_url = args.metrics or f"http://{host}:{args.metrics_port}/metrics"

    baseline = scrape_metrics(metrics_url)
    probe = ProcessProbe(read_pid(port))
    rss_samples: list[int] = []
    cpu_samples: list[float] = []

    workers = [
        Worker(i, (host, port), args.duration, args.rate) for i in range(args.clients)
    ]
    print(
        f"压测开始：{args.clients} 连接 × {args.duration:.0f}s，速率 {args.rate:.1f} pkt/s/连接，"
        f"服务端 {host}:{port}"
    )
    started = time.time()
    for worker in workers:
        worker.start()

    # 采样线程：每秒抓 metrics + RSS/CPU
    while time.time() - started < args.duration + args.grace:
        rss = probe.rss_bytes()
        if rss:
            rss_samples.append(rss)
        cpu = probe.cpu_percent()
        if cpu is not None:
            cpu_samples.append(cpu)
        alive = sum(1 for worker in workers if worker.is_alive())
        print(
            f"  t={time.time() - started:5.1f}s 存活={alive:3d}/{args.clients}"
            f"  RSS={(rss or 0) / 1048576:7.1f} MiB"
            + (f"  CPU={cpu:5.1f}%" if cpu is not None else ""),
            flush=True,
        )
        time.sleep(args.sample_interval)
        if alive == 0 and time.time() - started > 5:
            break

    for worker in workers:
        worker.stop()
    for worker in workers:
        worker.join(timeout=10)

    final = scrape_metrics(metrics_url)
    elapsed = time.time() - started

    def delta(name: str) -> float | None:
        if name not in final:
            return None
        if baseline and name in baseline:
            return final[name] - baseline[name]
        return final[name]

    connected = sum(1 for worker in workers if worker.connected)
    entered = sum(1 for worker in workers if worker.entered)
    errors = [worker.error for worker in workers if worker.error]
    report = {
        "clients": args.clients,
        "duration_secs": round(elapsed, 1),
        "rate_per_client": args.rate,
        "connected": connected,
        "entered_world": entered,
        "connect_success_rate": round(connected / max(1, len(workers)), 4),
        "enter_success_rate": round(entered / max(1, len(workers)), 4),
        "disconnects": sum(worker.disconnects for worker in workers),
        "packets_received": sum(worker.packets for worker in workers),
        "errors_sample": errors[:5],
        "error_kinds": sorted({error.split("：")[0] for error in errors}),
        "metrics": {
            "players_online": final.get("mir2_players_online"),
            "connections_active": final.get("mir2_connections_active"),
            "connections_total": final.get("mir2_connections_total"),
            "connections_rejected_delta": delta("mir2_connections_rejected"),
            "packets_dropped_delta": delta("mir2_packets_dropped_total"),
            "ticks_total_delta": delta("mir2_ticks_total"),
            "tick_micros_last": final.get("mir2_tick_micros_last"),
            "tick_micros_avg": final.get("mir2_tick_micros_avg"),
            "tick_micros_p50": final.get("mir2_tick_micros_p50"),
            "tick_micros_p95": final.get("mir2_tick_micros_p95"),
            "tick_micros_p99": final.get("mir2_tick_micros_p99"),
            "tick_micros_max": final.get("mir2_tick_micros_max"),
            "db_saves_delta": delta("mir2_db_save_total"),
            "db_save_micros_avg": final.get("mir2_db_save_micros_avg"),
            "db_save_micros_p95": final.get("mir2_db_save_micros_p95"),
            "db_save_failed_delta": delta("mir2_db_save_failed_total"),
            "autosaves_delta": delta("mir2_autosave_total"),
            "autosave_failures_delta": delta("mir2_autosave_failures_total"),
        },
        "process": {
            "pid": read_pid(port),
            "rss_mib_avg": round(statistics.fmean(rss_samples) / 1048576, 1) if rss_samples else None,
            "rss_mib_peak": round(max(rss_samples) / 1048576, 1) if rss_samples else None,
            "cpu_percent_avg": round(statistics.fmean(cpu_samples), 1) if cpu_samples else None,
            "cpu_percent_peak": round(max(cpu_samples), 1) if cpu_samples else None,
        },
        "baseline_metrics_url": metrics_url,
        "baseline_available": bool(baseline),
    }
    return report


def print_table(reports: list[dict]) -> None:
    header = (
        f"{'连接数':<8}{'成功率':<10}{'tick p50(µs)':<14}{'tick p99(µs)':<14}"
        f"{'tick max(µs)':<14}{'CPU%':<10}{'RSS(MiB)':<12}{'DB写':<8}{'拒绝':<8}{'丢包':<8}"
    )
    print()
    print(header)
    print("-" * len(header))
    for report in reports:
        metrics = report["metrics"]
        process = report["process"]

        def show(value, digits=0):
            if value is None:
                return "-"
            return f"{value:.{digits}f}"

        print(
            f"{report['clients']:<8}"
            f"{report['connect_success_rate'] * 100:>6.1f}%   "
            f"{show(metrics['tick_micros_p50']):<14}"
            f"{show(metrics['tick_micros_p99']):<14}"
            f"{show(metrics['tick_micros_max']):<14}"
            f"{show(process['cpu_percent_avg'], 1):<10}"
            f"{show(process['rss_mib_peak'], 1):<12}"
            f"{show(metrics['db_saves_delta']):<8}"
            f"{show(metrics['connections_rejected_delta']):<8}"
            f"{show(metrics['packets_dropped_delta']):<8}"
        )


def main() -> int:
    parser = argparse.ArgumentParser(description="传奇-Next 并发压测")
    parser.add_argument("--server", default="127.0.0.1:7000", help="服务端 host:port")
    parser.add_argument("--clients", type=int, default=100, help="并发连接数（100/300）")
    parser.add_argument("--duration", type=float, default=60.0, help="压测时长（秒）")
    parser.add_argument("--rate", type=float, default=4.0, help="每连接每秒发包数")
    parser.add_argument("--metrics", default=None, help="完整 /metrics URL（默认按 --metrics-port 拼）")
    parser.add_argument("--metrics-port", type=int, default=9100, help="MIR2_METRICS_LISTEN 的端口")
    parser.add_argument("--sample-interval", type=float, default=2.0, help="采样间隔（秒）")
    parser.add_argument("--grace", type=float, default=10.0, help="收尾宽限（秒）")
    parser.add_argument("--out", default=None, help="JSON 输出路径（默认 target/stress-<clients>.json）")
    parser.add_argument("--repeat", type=int, default=1, help="重复轮数（多轮结果打印同一张表）")
    parser.add_argument(
        "--max-tick-p99",
        type=float,
        default=100.0,
        help="tick p99 判据（毫秒，默认 100 = tick 预算；超过就算跟不上）",
    )
    parser.add_argument(
        "--warn-tick-p99",
        type=float,
        default=50.0,
        help="tick p99 告警线（毫秒，默认 50 = 半个 tick 预算，只提示不算失败）",
    )
    args = parser.parse_args()

    reports = []
    for round_index in range(args.repeat):
        if args.repeat > 1:
            print(f"\n=== 第 {round_index + 1}/{args.repeat} 轮 ===")
        report = run(args)
        reports.append(report)
        out = args.out or os.path.join(ROOT, "target", f"stress-{args.clients}.json")
        with open(out, "w", encoding="utf-8") as handle:
            json.dump(reports, handle, ensure_ascii=False, indent=2)
        print(f"结果已写入 {out}")

    print_table(reports)
    # 判据（供 CI/回执用）：连接成功 ≥ 95%，tick p99 ≤ --max-tick-p99（默认 = tick 预算 100ms），
    # 无服务器拒绝连接；tick p99 > --warn-tick-p99（默认半个预算 50ms）只告警不算失败。
    failed = []
    warned = []
    for report in reports:
        clients = report["clients"]
        if report["connect_success_rate"] < 0.95:
            failed.append(f"{clients} 连接成功率 {report['connect_success_rate']:.1%} < 95%")
        p99 = report["metrics"]["tick_micros_p99"]
        if p99 is not None and p99 > args.max_tick_p99 * 1000:
            failed.append(f"{clients} tick p99 {p99:.0f}µs > {args.max_tick_p99:.0f}ms（tick 预算跟不上）")
        elif p99 is not None and p99 > args.warn_tick_p99 * 1000:
            warned.append(f"{clients} tick p99 {p99:.0f}µs > {args.warn_tick_p99:.0f}ms（仍在预算内）")
        rejected = report["metrics"]["connections_rejected_delta"]
        if rejected:
            failed.append(f"{clients} 有 {rejected:.0f} 条连接被限流拒绝")
    print()
    for line in warned:
        print(f"[WARN] {line}")
    if failed:
        for line in failed:
            print(f"[FAIL] {line}")
        return 1
    print(
        f"[OK] 压测判据通过（成功率 ≥95%、tick p99 ≤{args.max_tick_p99:.0f}ms、无连接被拒）"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
