"""长时 soak（稳定性）压测：100 连接 × ≥1800s，10s 采样，判据「相对 + 绝对」缺一不可。

设计要点
--------
1. **复用** `tools/stress/stress.py` 的虚拟玩家（`Worker`）、进程探针（`ProcessProbe`）、
   `/metrics` 抓取与百分位计算，避免两处协议口径漂移。
2. 采样来源全部是**真实观测**：服务端 `/metrics`（tick p50/p99、连接数、**DB 连接池**、
   `write_failed`/`dropped`）+ 操作系统（RSS、句柄数）。
3. `evaluate()` 是纯函数：既能评真实采样，也能评 `--self-check` 的合成坏样本，
   所以「判据会不会红」这件事本身可以被验证。
4. 判据（v3 规格，缺一不可）：
   - **绝对**：每个采样点 `p50 ≤ 30ms` 且 `p99 ≤ 80ms`，任一超限即 FAIL；
   - **相对**（预热后）：RSS 增长 ≤10%、句柄波动 ≤5、DB 连接数不增长、
     后 1/4 窗口 p99 ≤ 前 1/4 ×1.2、无连续 6 个采样点单调上升。
5. 脚本自断言：时长 ≥1800s、采样点 ≥180、连接数 == 100，任一不满足即 exit 非 0。

用法
----
```
# 真实 soak（空载机器；服务端需开 /metrics）
py tools/stress/soak.py --server 127.0.0.1:7000 --metrics-url http://127.0.0.1:9090/metrics \
    --clients 100 --duration 1800 --out-dir docs/testing/soak-data/<ts>

# 负向自测：合成坏样本必须被判 FAIL（进程以 1 退出属预期）
py tools/stress/soak.py --self-check
```

退出码：0 = verdict PASS；1 = verdict FAIL（含 `--self-check` 的预期失败）；2 = 参数/环境错误。
"""

from __future__ import annotations

import argparse
import csv
import ctypes
import importlib.util
import json
import os
import statistics
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

#: 判据（写死在这里，改动必须同步 `docs/testing/soak.md`）。
CRITERIA = {
    "min_duration_seconds": 1800,
    "min_samples": 180,
    "clients": 100,
    "warmup_seconds": 180,
    "max_rss_growth_ratio": 0.10,
    "max_handle_swing": 5,
    "max_p99_quarter_ratio": 1.2,
    "max_monotonic_run": 6,
    "abs_p50_ms": 30.0,
    "abs_p99_ms": 80.0,
}

#: 进程句柄数（Windows）；拿不到返回 None。
def resolve_server_pid(port: int, explicit: int | None) -> int | None:
    """定位服务端 PID：显式参数 > `serve.ps1` 写的 pid 文件 > `netstat -ano` 反查监听者。

    直接 `Start-Process` 起的服务端没有 pid 文件，这时必须靠 netstat，
    否则 RSS/句柄会全程采不到，soak 会以「缺少 RSS 数据」判 FAIL。
    """
    if explicit:
        return explicit
    from_file = stress.read_pid(port)
    if from_file:
        return from_file
    try:
        import subprocess

        output = subprocess.run(
            ["netstat", "-ano", "-p", "tcp"],
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        ).stdout
        for line in output.splitlines():
            parts = line.split()
            if len(parts) >= 5 and parts[1].endswith(f":{port}") and parts[3] == "LISTENING":
                return int(parts[4])
    except (OSError, ValueError, subprocess.SubprocessError):
        return None
    return None


def handle_count(pid: int | None) -> int | None:
    if not pid or os.name != "nt":
        return None
    try:
        kernel32 = ctypes.windll.kernel32
        handle = kernel32.OpenProcess(0x0400, False, pid)  # PROCESS_QUERY_INFORMATION
        if not handle:
            return None
        try:
            count = ctypes.c_ulong(0)
            if kernel32.GetProcessHandleCount(handle, ctypes.byref(count)):
                return int(count.value)
            return None
        finally:
            kernel32.CloseHandle(handle)
    except Exception:  # noqa: BLE001 - 指标拿不到不该打断 soak
        return None


def load_stress():
    """把 `stress.py` 当模块加载，复用 Worker / ProcessProbe / scrape_metrics / percentile。"""
    path = os.path.join(ROOT, "tools", "stress", "stress.py")
    spec = importlib.util.spec_from_file_location("mir_stress", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


stress = load_stress()


class QuietWorker(stress.Worker):
    """把连接异常收进 `error` 字段，不打印 100 份 traceback（soak 日志要能看）。"""

    def run(self) -> None:
        try:
            super().run()
        except Exception as error:  # noqa: BLE001 - 客户端异常不该打断采样
            self.error = f"{type(error).__name__}: {error}"
            try:
                self._close()
            except Exception:  # noqa: BLE001
                pass


# ---------------------------------------------------------------------------
# 判据（纯函数）
# ---------------------------------------------------------------------------
def _series(samples: list[dict], key: str) -> list[float]:
    out = []
    for sample in samples:
        value = sample.get(key)
        if isinstance(value, (int, float)):
            out.append(float(value))
    return out


def _quarter_mean(values: list[float]) -> float | None:
    if not values:
        return None
    quarter = max(1, len(values) // 4)
    return statistics.fmean(values[-quarter:])


def _longest_monotonic_run(values: list[float]) -> int:
    """最长「严格递增」连续段长度（相邻后一个 > 前一个）。"""
    best = 0
    run = 0
    for previous, current in zip(values, values[1:]):
        if current > previous:
            run += 1
            best = max(best, run)
        else:
            run = 0
    return best + 1 if values else 0


def evaluate(samples: list[dict], meta: dict) -> dict:
    """对采样序列下判决。返回 {verdict, checks[], stats{}}。"""
    checks: list[dict] = []

    def check(name: str, ok: bool, detail: str) -> None:
        checks.append({"name": name, "ok": bool(ok), "detail": detail})

    duration = float(meta.get("duration_seconds") or 0.0)
    clients = int(meta.get("clients") or 0)
    check(
        f"时长 ≥{CRITERIA['min_duration_seconds']}s",
        duration >= CRITERIA["min_duration_seconds"],
        f"duration_seconds={duration:.1f}",
    )
    check(
        f"采样点 ≥{CRITERIA['min_samples']}",
        len(samples) >= CRITERIA["min_samples"],
        f"samples={len(samples)}",
    )
    check(
        f"连接数 == {CRITERIA['clients']}",
        clients == CRITERIA["clients"],
        f"clients={clients}",
    )

    if not samples:
        check("采样非空", False, "没有任何采样点")
        return {"verdict": "FAIL", "checks": checks, "stats": {}}

    # ---- 绝对阈值：每个采样点都要过（不看预热窗口）----
    p50 = _series(samples, "tick_p50_ms")
    p99 = _series(samples, "tick_p99_ms")
    over_p50 = [(s.get("t", 0.0), s.get("tick_p50_ms")) for s in samples
                if isinstance(s.get("tick_p50_ms"), (int, float)) and s["tick_p50_ms"] > CRITERIA["abs_p50_ms"]]
    over_p99 = [(s.get("t", 0.0), s.get("tick_p99_ms")) for s in samples
                if isinstance(s.get("tick_p99_ms"), (int, float)) and s["tick_p99_ms"] > CRITERIA["abs_p99_ms"]]
    check(
        f"绝对：每个采样点 p50 ≤{CRITERIA['abs_p50_ms']}ms",
        bool(p50) and not over_p50,
        f"max_p50={max(p50):.2f}ms 超限点={len(over_p50)}" if p50 else "缺少 p50 数据",
    )
    check(
        f"绝对：每个采样点 p99 ≤{CRITERIA['abs_p99_ms']}ms",
        bool(p99) and not over_p99,
        f"max_p99={max(p99):.2f}ms 超限点={len(over_p99)} 首个超限={over_p99[:3]}" if p99 else "缺少 p99 数据",
    )

    # ---- 相对判据：只看预热之后的窗口 ----
    warmup = float(meta.get("warmup_seconds") or CRITERIA["warmup_seconds"])
    warm = [s for s in samples if float(s.get("t", 0.0)) >= warmup]
    check("预热后仍有采样点", bool(warm), f"warm_samples={len(warm)} (warmup={warmup:.0f}s)")
    if not warm:
        return {"verdict": "FAIL", "checks": checks, "stats": {}}

    rss = _series(warm, "rss_bytes")
    if rss and rss[0] > 0:
        growth = (max(rss) - rss[0]) / rss[0]
        check(
            f"RSS 增长 ≤{CRITERIA['max_rss_growth_ratio'] * 100:.0f}%",
            growth <= CRITERIA["max_rss_growth_ratio"],
            f"first={rss[0] / 1e6:.1f}MB max={max(rss) / 1e6:.1f}MB growth={growth * 100:.2f}%",
        )
    else:
        check("RSS 增长 ≤10%", False, "缺少 RSS 数据")

    handles = [float(v) for v in _series(warm, "handles")]
    if handles:
        swing = max(handles) - min(handles)
        check(
            f"句柄波动 ≤{CRITERIA['max_handle_swing']}",
            swing <= CRITERIA["max_handle_swing"],
            f"min={min(handles):.0f} max={max(handles):.0f} swing={swing:.0f}",
        )
    else:
        check("句柄波动 ≤5", False, "缺少句柄数据")

    pool = _series(warm, "db_pool_connections")
    if pool:
        check(
            "DB 连接数不增长",
            max(pool) <= pool[0],
            f"first={pool[0]:.0f} max={max(pool):.0f} last={pool[-1]:.0f}",
        )
    else:
        check("DB 连接数不增长", False, "缺少 DB 连接池数据（/metrics 需含 mir2_db_pool_connections）")

    warm_p99 = _series(warm, "tick_p99_ms")
    quarter = max(1, len(warm_p99) // 4)
    if len(warm_p99) >= 2 * quarter:
        head = statistics.fmean(warm_p99[:quarter])
        tail = statistics.fmean(warm_p99[-quarter:])
        ratio = tail / head if head > 0 else float("inf")
        check(
            f"后 1/4 窗口 p99 ≤ 前 1/4 ×{CRITERIA['max_p99_quarter_ratio']}",
            ratio <= CRITERIA["max_p99_quarter_ratio"],
            f"head={head:.2f}ms tail={tail:.2f}ms ratio={ratio:.3f}",
        )
    else:
        check("后 1/4 窗口 p99 ≤ 前 1/4 ×1.2", False, f"预热后 p99 采样点不足（{len(warm_p99)}）")

    run = _longest_monotonic_run(warm_p99)
    check(
        f"无连续 {CRITERIA['max_monotonic_run']} 个采样点单调上升",
        run < CRITERIA["max_monotonic_run"],
        f"最长连续上升段={run}",
    )

    stats = {
        "duration_seconds": duration,
        "samples": len(samples),
        "warm_samples": len(warm),
        "p50_ms_max": round(max(p50), 2) if p50 else None,
        "p99_ms_max": round(max(p99), 2) if p99 else None,
        "p99_ms_first_quarter_mean": round(statistics.fmean(warm_p99[:quarter]), 2) if warm_p99 else None,
        "p99_ms_last_quarter_mean": round(statistics.fmean(warm_p99[-quarter:]), 2) if warm_p99 else None,
        "rss_first_mb": round(rss[0] / 1e6, 1) if rss else None,
        "rss_max_mb": round(max(rss) / 1e6, 1) if rss else None,
        "handle_swing": (max(handles) - min(handles)) if handles else None,
        "db_pool_first": pool[0] if pool else None,
        "db_pool_max": max(pool) if pool else None,
        "write_failed_final": (samples[-1].get("write_failed_total") if samples else None),
        "dropped_final": (samples[-1].get("packets_dropped_total") if samples else None),
    }
    verdict = "PASS" if all(item["ok"] for item in checks) else "FAIL"
    return {"verdict": verdict, "checks": checks, "stats": stats}


def print_verdict(report: dict, title: str) -> None:
    print(f"\n=== {title} ===")
    for item in report["checks"]:
        flag = "PASS" if item["ok"] else "FAIL"
        print(f"  [{flag}] {item['name']}  —— {item['detail']}")
    stats = report.get("stats") or {}
    if stats:
        print("  —— 统计 ——")
        for key, value in stats.items():
            print(f"     {key} = {value}")
    print(f"  判决：{report['verdict']}")


# ---------------------------------------------------------------------------
# 真实采样
# ---------------------------------------------------------------------------
def sample_once(probe, pid: int | None, metrics_url: str, started: float) -> dict:
    metrics = stress.scrape_metrics(metrics_url)
    rss = probe.rss_bytes()
    return {
        "t": round(time.time() - started, 1),
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "rss_bytes": rss,
        "handles": handle_count(pid),
        "db_pool_connections": metrics.get("mir2_db_pool_connections"),
        "db_pool_idle": metrics.get("mir2_db_pool_idle"),
        "tick_p50_ms": (metrics["mir2_tick_micros_p50"] / 1000.0) if "mir2_tick_micros_p50" in metrics else None,
        "tick_p99_ms": (metrics["mir2_tick_micros_p99"] / 1000.0) if "mir2_tick_micros_p99" in metrics else None,
        "tick_samples": metrics.get("mir2_tick_samples"),
        "connections_active": metrics.get("mir2_connections_active"),
        "players_online": metrics.get("mir2_players_online"),
        "packets_dropped_total": metrics.get("mir2_packets_dropped_total"),
        "write_failed_total": metrics.get("mir2_write_failed_total"),
        "batch_write_failed_total": metrics.get("mir2_batch_write_failed_total"),
    }


def run_live(args) -> int:
    pid = resolve_server_pid(args.port, args.pid)
    if not pid:
        print(f"[FAIL] 定位不到监听 {args.port} 的服务端进程，RSS/句柄会全空 —— 用 --pid 指定", file=sys.stderr)
        return 2
    probe = stress.ProcessProbe(pid)
    started = time.time()
    workers = []
    for index in range(args.clients):
        worker = QuietWorker(
            index,
            (args.host, args.port),
            args.duration + 30,
            args.rate,
            tuple(int(part) for part in args.mix.split(",")),
            spread=True,
        )
        worker.start()
        workers.append(worker)
        time.sleep(args.connect_spacing)
    print(f"已拉起 {len(workers)} 个连接（间隔 {args.connect_spacing}s），开始采样（每 {args.sample_interval}s）")

    samples: list[dict] = []
    previous_rss = None
    previous_at = None
    try:
        while True:
            sample = sample_once(probe, pid, args.metrics_url, started)
            rss = sample.get("rss_bytes")
            if previous_rss and previous_at and rss:
                elapsed = max(0.001, time.time() - previous_at)
                sample["alloc_bytes_per_sec"] = round((rss - previous_rss) / elapsed, 1)
            else:
                sample["alloc_bytes_per_sec"] = None
            previous_rss, previous_at = rss, time.time()
            samples.append(sample)
            # 服务端没了（被 kill / 崩溃）就没有续航意义：立刻中止并标红，别白跑满 30 分钟。
            if sample.get("rss_bytes") is None and sample.get("tick_p99_ms") is None and sample.get("t", 0) > 30:
                print(f"  [ABORT] t=+{sample['t']:.0f}s 服务端进程/指标都读不到（PID {pid}）——中止本次 soak")
                meta_abort = {
                    "duration_seconds": round(time.time() - started, 1),
                    "clients": args.clients,
                    "warmup_seconds": args.warmup,
                    "aborted": "server_unreachable",
                    "pid": pid,
                }
                report = evaluate(samples, meta_abort)
                print_verdict(report, "soak 中止（服务端不可达）")
                return 1
            print(
                f"  t=+{sample['t']:7.1f}s p50={sample['tick_p50_ms']}ms p99={sample['tick_p99_ms']}ms "
                f"rss={None if rss is None else round(rss / 1e6, 1)}MB handles={sample['handles']} "
                f"db_pool={sample['db_pool_connections']} dropped={sample['packets_dropped_total']} "
                f"write_failed={sample['write_failed_total']}"
            )
            if time.time() - started >= args.duration:
                break
            time.sleep(args.sample_interval)
    finally:
        for worker in workers:
            worker.stop()
        for worker in workers:
            worker.join(timeout=5)

    meta = {
        "started_at": time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(started)),
        "duration_seconds": round(time.time() - started, 1),
        "clients": args.clients,
        "warmup_seconds": args.warmup,
        "sample_interval_seconds": args.sample_interval,
        "rate_per_client": args.rate,
        "mix": args.mix,
        "server": f"{args.host}:{args.port}",
        "metrics_url": args.metrics_url,
        "pid": pid,
    }
    connected = sum(1 for worker in workers if worker.entered)
    meta["clients_entered"] = connected
    print(f"\n连接建立并进入世界：{connected}/{len(workers)}")
    dropped_workers = [worker for worker in workers if worker.error]
    if dropped_workers:
        meta["client_errors"] = len(dropped_workers)
        print(f"客户端异常 {len(dropped_workers)} 个，示例：{dropped_workers[0].error}")

    out_dir = args.out_dir or os.path.join("docs", "testing", "soak-data", time.strftime("%Y%m%d-%H%M%S"))
    out_dir = out_dir if os.path.isabs(out_dir) else os.path.join(ROOT, out_dir)
    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, "samples.json"), "w", encoding="utf-8") as handle:
        json.dump({"meta": meta, "samples": samples}, handle, ensure_ascii=False, indent=2)
    keys = [
        "t", "timestamp", "rss_bytes", "handles", "alloc_bytes_per_sec", "db_pool_connections",
        "db_pool_idle", "tick_p50_ms", "tick_p99_ms", "tick_samples", "connections_active",
        "players_online", "packets_dropped_total", "write_failed_total", "batch_write_failed_total",
    ]
    with open(os.path.join(out_dir, "samples.csv"), "w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=keys, extrasaction="ignore")
        writer.writeheader()
        writer.writerows(samples)

    report = evaluate(samples, meta)
    report["meta"] = meta
    with open(os.path.join(out_dir, "verdict.json"), "w", encoding="utf-8") as handle:
        json.dump(report, handle, ensure_ascii=False, indent=2)
    print_verdict(report, f"soak 判决（{meta['duration_seconds']}s / {len(samples)} 采样）")
    print(f"\n原始数据：{os.path.relpath(out_dir, ROOT)}/samples.json, samples.csv, verdict.json")
    return 0 if report["verdict"] == "PASS" else 1


# ---------------------------------------------------------------------------
# 负向自测：合成坏样本必须 FAIL
# ---------------------------------------------------------------------------
def _baseline_series(count: int = 200) -> list[dict]:
    """一组合格样本（p50≈4ms、p99≈20ms、RSS/句柄/DB 连接平稳）。"""
    samples = []
    for index in range(count):
        samples.append({
            "t": float(index * 10 + 10),
            "rss_bytes": 120e6 + index * 1000.0,      # 200 点只涨 200KB
            "handles": 300 + (index % 3),             # 波动 2
            "db_pool_connections": 5,
            # 用「锯齿」而不是缓慢爬升：`index % 4` 每 4 个点就回落一次，
            # 保证健康样本不会因为自然的连续上升段被判红（判据是「连续 6 点单调上升」）。
            "tick_p50_ms": 3.5 + (index % 3) * 0.1,
            "tick_p99_ms": 18.0 + (index % 4) * 0.5,
            "write_failed_total": 0,
            "packets_dropped_total": 0,
        })
    return samples


def _meta(count: int = 200) -> dict:
    return {
        "duration_seconds": 1800.0,
        "clients": 100,
        "warmup_seconds": 180,
        "samples": count,
    }


def self_check() -> int:
    cases = []

    # 案例 0（对照）：健康的合成样本必须 PASS —— 否则判据是「恒红」，自证无意义。
    cases.append(("对照：健康样本", _baseline_series(), _meta(), "PASS"))

    # 案例 1：RSS 翻倍（预热后从 120MB 涨到 240MB）
    rss_double = _baseline_series()
    for index, sample in enumerate(rss_double):
        if sample["t"] >= 180:
            sample["rss_bytes"] = 120e6 * (1 + (sample["t"] - 180) / 1620)
    cases.append(("坏样本 1：RSS 翻倍", rss_double, _meta(), "FAIL"))

    # 案例 2：p99 单调上升（每个点都比前一个高）
    monotonic = _baseline_series()
    for index, sample in enumerate(monotonic):
        sample["tick_p99_ms"] = 20.0 + index * 0.2
    cases.append(("坏样本 2：p99 单调上升", monotonic, _meta(), "FAIL"))

    # 案例 3：单点 p99 = 90ms（验证**绝对阈值**真的生效，相对判据看不出来）
    spike = _baseline_series()
    spike[100]["tick_p99_ms"] = 90.0
    cases.append(("坏样本 3：单点 p99=90ms", spike, _meta(), "FAIL"))

    failures = []
    for title, samples, meta, expected in cases:
        report = evaluate(samples, meta)
        print_verdict(report, f"self-check · {title}（期望 {expected}）")
        if report["verdict"] != expected:
            failures.append(f"{title}：期望 {expected}，实际 {report['verdict']}")

    print("\n=== self-check 结论 ===")
    if failures:
        for failure in failures:
            print(f"  [BROKEN] {failure}")
        print("  判据自证失败：合成坏样本没有被判 FAIL（判据可能恒绿）")
        return 1
    print(f"  [OK] {len(cases)} 个案例判决全部符合预期（1 个健康 PASS + 3 个坏样本 FAIL）")
    print("  说明：本命令以 1 退出**属预期**——坏样本本该判 FAIL，这里把「会红」当证据。")
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description="长时 soak：100 连接 × ≥1800s，10s 采样，相对+绝对判据")
    parser.add_argument("--server", default="127.0.0.1:7000", help="服务端 host:port")
    parser.add_argument("--metrics-url", default="http://127.0.0.1:9090/metrics", help="/metrics 地址")
    parser.add_argument("--clients", type=int, default=CRITERIA["clients"])
    parser.add_argument("--duration", type=float, default=CRITERIA["min_duration_seconds"])
    parser.add_argument("--sample-interval", type=float, default=10.0)
    parser.add_argument("--warmup", type=float, default=float(CRITERIA["warmup_seconds"]))
    parser.add_argument("--rate", type=float, default=1.0, help="每个客户端每秒发包数")
    parser.add_argument("--mix", default="2,2,1", help="走动/攻击/聊天 比例")
    parser.add_argument("--connect-spacing", type=float, default=0.05, help="相邻连接间隔秒数")
    parser.add_argument("--pid", type=int, default=None, help="服务端 PID（不给则查 pid 文件/netstat）")
    parser.add_argument("--out-dir", default="", help="原始数据目录（默认 docs/testing/soak-data/<ts>）")
    parser.add_argument("--self-check", action="store_true", help="用合成坏样本自证判据会 FAIL")
    args = parser.parse_args()

    if args.self_check:
        return self_check()

    host, _, port = args.server.partition(":")
    args.host = host
    args.port = int(port or 7000)
    if args.clients != CRITERIA["clients"]:
        print(f"[FAIL] 规格要求 {CRITERIA['clients']} 连接，收到 {args.clients}：脚本自断言不通过", file=sys.stderr)
        return 1
    if args.duration < CRITERIA["min_duration_seconds"]:
        print(
            f"[WARN] 时长 {args.duration}s < 规格 {CRITERIA['min_duration_seconds']}s，本次判决必定 FAIL（不许缩时长）",
            file=sys.stderr,
        )
    if not stress.scrape_metrics(args.metrics_url):
        print(f"[FAIL] 抓不到 /metrics（{args.metrics_url}）：先给服务端设 MIR2_METRICS_LISTEN", file=sys.stderr)
        return 2
    return run_live(args)


if __name__ == "__main__":
    sys.exit(main())
