"""长时 soak（稳定性）压测：≥1800s 开发跑 / **4 小时长跑（14400s）** 发版前跑。

规格（第 21 轮 v2）
------------------
1. **时长自证**：`requested_duration_seconds` 来自 `--duration`（**不再硬编码 1800**），
   `meta.elapsed_seconds >= requested` 由脚本断言，verdict 里写 `duration_ok`；
   采样点断言由 `requested / sample_interval` 派生（4h @10s → ≥1440 点）。
2. **反作弊**：`--duration < 14400` 时**拒绝产出长跑 verdict**（`exit 2`），除非显式 `--allow-short`；
   此时产物一律带 `-short` 后缀、`meta.short_run = true`，不可能冒充长跑结论。
3. **chunk fail-fast**：每 `--chunk-seconds`（默认 1800s）落一份 `verdict-chunk-<n>.json`
   （区间、该区间判据、**累计斜率**）；**任一 chunk FAIL → 立即 `exit 1`** 并打印失败项，
   同时写 `verdict.json` 且 `"verdict": "FAIL_EARLY"`，**已产出的 chunk 文件全部保留**；
   全部通过才汇总 `verdict.json`（含 `chunks` 数组）。
4. **判据**（相对 + 绝对 + 趋势斜率，缺一不可）：
   - 绝对：**每个采样点** `p50 ≤ 30ms`、`p99 ≤ 80ms`；
   - 相对（预热 180s 后）：RSS 增长 ≤10%、句柄波动 ≤5、DB 连接数不增长、
     后 1/4 窗口 p99 ≤ 前 1/4 ×1.2、无连续 6 点单调上升；
   - 趋势：RSS 线性回归斜率 **≤2 MB/h**、句柄斜率 **≤2 个/h**。

采样来源全部是**真实观测**：`/metrics`（tick p50/p99、连接数、DB 连接池、`write_failed`/`dropped`）
+ 操作系统（RSS / 句柄数）。`evaluate_window()` 是纯函数，既评真实采样也评 `--self-check` 的合成样本。

用法
----
```
# 4h 长跑（发版前；空载机器；先写 target/soak.lock）
py tools/stress/soak.py --duration 14400 --clients 100 --sample-interval 10 \
    --server 127.0.0.1:7000 --metrics-url http://127.0.0.1:9090/metrics \
    --out-dir docs/testing/soak-data/<ts>

# 开发冒烟（必须显式承认短跑：产物带 -short）
py tools/stress/soak.py --duration 120 --allow-short ...

# 负向自测（1 个健康 + 5 个坏样本）
py tools/stress/soak.py --self-check
```

退出码：0 = PASS；1 = FAIL / FAIL_EARLY（含 `--self-check` 的预期失败）；2 = 参数/环境错误。
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

#: 判据（改动必须同步 `docs/testing/soak.md`）。
CRITERIA = {
    # 反作弊门限：低于它就只许产出 `-short` 产物。
    "long_run_min_seconds": 14400,
    # 中间 verdict 的区间长度。
    "chunk_seconds": 1800,
    # 相对判据的预热窗口。
    "warmup_seconds": 180,
    "max_rss_growth_ratio": 0.10,
    "max_handle_swing": 5,
    "max_p99_quarter_ratio": 1.2,
    "max_monotonic_run": 6,
    # 绝对阈值（每一点都要过）。
    "abs_p50_ms": 30.0,
    "abs_p99_ms": 80.0,
    # 趋势斜率（长跑必需）。
    # chunk 区间判据的**绝对噪声地板**：实测 chunk 内 RSS 锯齿 35.8↔49.3MB（±19%），
    # 10% 百分比在 ~40MB 基线下只有 4.7MB，**低于测量噪声**，会随机误杀（第 23 轮 chunk-2 差 0.06pp）。
    # 判据：growth > max(10% × 基线, 8MB) 才 FAIL。
    "chunk_rss_noise_floor_bytes": 8_000_000,
    "max_rss_slope_mb_per_hour": 2.0,
    "max_handle_slope_per_hour": 2.0,
    # 总斜率**跳过**前 30 分钟：那一段是地图/NPC/怪物首载 + 分配器爬坡，不是泄漏。
    "slope_skip_seconds": 1800,
}

#: 规格连接数。
REQUIRED_CLIENTS = 100


def load_stress():
    """把 `stress.py` 当模块加载，复用 Worker / ProcessProbe / scrape_metrics / read_pid。"""
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


def resolve_server_pid(port: int, explicit: int | None) -> int | None:
    """定位服务端 PID：显式参数 > `serve.ps1` 的 pid 文件 > `netstat -ano` 反查监听者。"""
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
    """进程句柄数（Windows）；拿不到返回 None。"""
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


# ---------------------------------------------------------------------------
# 纯函数：序列工具
# ---------------------------------------------------------------------------
def series(samples: list[dict], key: str) -> list[float]:
    out = []
    for sample in samples:
        value = sample.get(key)
        if isinstance(value, (int, float)):
            out.append(float(value))
    return out


def longest_monotonic_run(values: list[float]) -> int:
    """最长「严格递增」连续段长度。"""
    if not values:
        return 0
    best = 0
    run = 0
    for previous, current in zip(values, values[1:]):
        if current > previous:
            run += 1
            best = max(best, run)
        else:
            run = 0
    return best + 1


def slope_per_hour(samples: list[dict], key: str) -> float | None:
    """线性回归斜率（每小时）：`Σ((x-x̄)(y-ȳ)) / Σ((x-x̄)²)`，`x` 单位是小时。"""
    pairs = [(float(s["t"]) / 3600.0, float(s[key])) for s in samples if isinstance(s.get(key), (int, float))]
    if len(pairs) < 2:
        return None
    xs = [x for x, _ in pairs]
    ys = [y for _, y in pairs]
    mean_x = statistics.fmean(xs)
    mean_y = statistics.fmean(ys)
    denominator = sum((x - mean_x) ** 2 for x in xs)
    if denominator == 0:
        return None
    numerator = sum((x - mean_x) * (y - mean_y) for x, y in zip(xs, ys))
    return numerator / denominator


# ---------------------------------------------------------------------------
# 判据：单个窗口（整跑 / chunk 共用）
# ---------------------------------------------------------------------------
def _check(checks: list[dict], name: str, ok: bool, detail: str, **extra: object) -> None:
    entry = {"name": name, "ok": bool(ok), "detail": detail}
    entry.update(extra)
    checks.append(entry)


def evaluate_window(
    samples: list[dict],
    meta: dict,
    *,
    label: str,
    check_duration: bool = True,
    cumulative_slope: bool = False,
    include_slope: bool = True,
    chunk_gate: bool = False,
    chunk_index: int | None = None,
) -> dict:
    """对一个采样窗口下判决。`check_duration=False` 时跳过时长/样本数自证（chunk 用）。"""
    checks: list[dict] = []
    requested = float(meta.get("requested_duration_seconds") or 0.0)
    elapsed = float(meta.get("elapsed_seconds") or meta.get("duration_seconds") or 0.0)
    interval = float(meta.get("sample_interval_seconds") or 10.0)
    required_samples = int(requested / interval) if requested else 0

    if check_duration:
        _check(
            checks,
            f"时长自证：requested ≥ {int(CRITERIA['long_run_min_seconds'])}s",
            requested >= CRITERIA["long_run_min_seconds"],
            f"requested={requested:.0f}s",
        )
        _check(
            checks,
            "时长自证：elapsed ≥ requested",
            elapsed >= requested and requested > 0,
            f"elapsed={elapsed:.1f}s requested={requested:.0f}s 差额={elapsed - requested:+.1f}s",
        )
        _check(
            checks,
            f"采样点 ≥ requested/interval = {required_samples}",
            len(samples) >= required_samples,
            f"samples={len(samples)} required={required_samples}",
        )
        # 连接：请求 100 + **全部成功进入世界** + 没有被服务端拒绝 + 稳态在线 ≥95。
        # 第 21 轮实测有 1 个客户端在握手后自行退出（服务端 accepted=100、entered=99、rejected=0），
        # 所以判据不能只看「请求 100」，必须看 entered 与 active_min。
        requested_clients = int(meta.get("clients") or 0)
        entered = meta.get("clients_entered")
        _check(
            checks,
            f"clients_requested == {REQUIRED_CLIENTS}",
            requested_clients == REQUIRED_CLIENTS,
            f"clients_requested={requested_clients}",
        )
        if entered is None:
            # 早退路径可能还没汇总「进入世界」的人数：此时 clients_entered 与 active_min **一起**标未采集，
            # 绝不允许「clients_entered=0 却 active_min=100」这种自相矛盾（第 21 轮刑部 medium）。
            _check(
                checks,
                "连接字段采集完整（早退时 clients_entered / active_min 一起标未采集）",
                True,
                "未采集：早退路径未汇总 clients_entered，因此 active_min 也不判定",
            )
        else:
            _check(
                checks,
                f"clients_entered == {REQUIRED_CLIENTS}（100 个都进入世界）",
                int(entered) == REQUIRED_CLIENTS,
                f"clients_entered={entered}",
            )

    if not samples:
        _check(checks, "采样非空", False, "窗口内没有采样点")
        return {"window": label, "verdict": "FAIL", "checks": checks, "stats": {}}

    # ---- 绝对阈值：窗口内每一点 ----
    p50 = series(samples, "tick_p50_ms")
    p99 = series(samples, "tick_p99_ms")
    over_p50 = [(s.get("t"), s.get("tick_p50_ms")) for s in samples
                if isinstance(s.get("tick_p50_ms"), (int, float)) and s["tick_p50_ms"] > CRITERIA["abs_p50_ms"]]
    over_p99 = [(s.get("t"), s.get("tick_p99_ms")) for s in samples
                if isinstance(s.get("tick_p99_ms"), (int, float)) and s["tick_p99_ms"] > CRITERIA["abs_p99_ms"]]
    _check(
        checks,
        f"绝对：每点 p50 ≤{CRITERIA['abs_p50_ms']}ms",
        bool(p50) and not over_p50,
        f"max_p50={max(p50):.2f}ms 超限点={len(over_p50)}" if p50 else "缺少 p50 数据",
    )
    _check(
        checks,
        f"绝对：每点 p99 ≤{CRITERIA['abs_p99_ms']}ms",
        bool(p99) and not over_p99,
        f"max_p99={max(p99):.2f}ms 超限点={len(over_p99)} 首个={over_p99[:2]}" if p99 else "缺少 p99 数据",
    )

    active = series(samples, "connections_active")
    active_min = min(active) if active else None
    if meta.get("clients_entered") is None:
        # 与 clients_entered 同进同退：早退时不单独判 active_min（否则就是 0 vs 100 的矛盾）。
        pass
    elif active:
        _check(
            checks,
            "稳态连接数 active_min ≥ 95（≥95% 持续在线）",
            active_min is not None and active_min >= 95,
            f"active_min={active_min:.0f} active_max={max(active):.0f}",
        )
        rejected_values = series(samples, "connections_rejected")
        if rejected_values:
            _check(
                checks,
                "服务端未拒绝连接（connections_rejected == 0）",
                max(rejected_values) == 0,
                f"rejected_max={max(rejected_values):.0f}",
            )
        else:
            _check(
                checks,
                "服务端未拒绝连接（connections_rejected == 0）",
                True,
                "旧版采样没有该字段，跳过（新版会采）",
            )
    else:
        _check(checks, "稳态连接数 active_min ≥ 95", False, "缺少 connections_active 数据")

    # ---- 相对判据：只看预热窗口之后 ----
    warmup = float(meta.get("warmup_seconds") or CRITERIA["warmup_seconds"])
    warm = [s for s in samples if float(s.get("t", 0.0)) >= warmup]
    observe: list[dict] = []
    if chunk_gate:
        # chunk 门禁的**最小充分集**：绝对延迟（上面已判）+ 区间 RSS 增长。
        # 句柄波动 / DB 池 / 连续上升 / 趋势斜率只在**整跑**判：
        # 30 分钟窗口里这三项天然抖动（实测 chunk-2：句柄 373→380=7、池 2→3、
        # p99 有 7 点连续上升），拿整跑判据逐窗判会**误杀**，第 21 轮真跑已经踩到。
        if len(warm) >= 3:
            rss = series(warm, "rss_bytes")
            if rss and rss[0] > 0:
                growth = (max(rss) - rss[0]) / rss[0]
                limit = CRITERIA["max_rss_growth_ratio"]
                detail = (
                    f"区间基线={rss[0] / 1e6:.1f}MB 峰值={max(rss) / 1e6:.1f}MB "
                    f"增长={growth * 100:+.2f}%"
                )
                if chunk_index == 1:
                    # **chunk-1 是预热块**：整跑判据本来就跳过前 1800s（=chunk-1 长度），
                    # 所以这一块的 RSS 增长只观察、不参与 FAIL（第 23 轮实测 +15.64% 是启动爬坡）。
                    _check(
                        checks,
                        f"RSS 增长（区间·仅观察）≤{limit * 100:.0f}%",
                        True,
                        detail + "（chunk-1 预热块：仅观察，不参与 FAIL）",
                        observe_only=True,
                    )
                    observe.append({
                        "kind": "rss_interval_observe_only",
                        "chunk": chunk_index,
                        "growth_ratio": round(growth, 4),
                        "note": "chunk-1 预热块：RSS 区间增长仅观察；chunk≥2 与整跑仍是硬判据",
                    })
                else:
                    floor = float(CRITERIA["chunk_rss_noise_floor_bytes"])
                    growth_bytes = max(rss) - rss[0]
                    allowed = max(limit * rss[0], floor)
                    _check(
                        checks,
                        f"RSS 增长 ≤max({limit * 100:.0f}%, {floor / 1e6:.0f}MB)（区间）",
                        growth_bytes <= allowed,
                        detail + f" 允许=max({limit * rss[0] / 1e6:.2f}MB, {floor / 1e6:.0f}MB)"
                        f"={allowed / 1e6:.2f}MB 实际={growth_bytes / 1e6:+.2f}MB",
                    )
        result = _window_stats(label, checks, samples, warm, p50, p99, requested, elapsed, None, meta)
        result["observe_warnings"] = observe
        return result
    if len(warm) < 3:
        _check(checks, "预热后样本足够做相对判据", False, f"warm_samples={len(warm)}（预热 {warmup:.0f}s 后不足 3 点）")
    else:
        rss = series(warm, "rss_bytes")
        if rss and rss[0] > 0:
            growth = (max(rss) - rss[0]) / rss[0]
            _check(
                checks,
                f"RSS 增长 ≤{CRITERIA['max_rss_growth_ratio'] * 100:.0f}%",
                growth <= CRITERIA["max_rss_growth_ratio"],
                f"基线(预热后首点)={rss[0] / 1e6:.1f}MB 峰值={max(rss) / 1e6:.1f}MB 增长={growth * 100:+.2f}%",
            )
        else:
            _check(checks, "RSS 增长 ≤10%", False, "缺少 RSS 数据")

        handles = series(warm, "handles")
        if handles:
            swing = max(handles) - min(handles)
            _check(
                checks,
                f"句柄波动 ≤{CRITERIA['max_handle_swing']}",
                swing <= CRITERIA["max_handle_swing"],
                f"min={min(handles):.0f} max={max(handles):.0f} swing={swing:.0f}",
            )
        else:
            _check(checks, "句柄波动 ≤5", False, "缺少句柄数据")

        pool = series(warm, "db_pool_connections")
        if pool:
            _check(
                checks,
                "DB 连接数不增长",
                max(pool) <= pool[0],
                f"首={pool[0]:.0f} 峰={max(pool):.0f} 末={pool[-1]:.0f}",
            )
        else:
            _check(checks, "DB 连接数不增长", False, "缺少 DB 连接池数据（需 mir2_db_pool_connections）")

        warm_p99 = series(warm, "tick_p99_ms")
        quarter = max(1, len(warm_p99) // 4)
        if len(warm_p99) >= 2 * quarter:
            head = statistics.fmean(warm_p99[:quarter])
            tail = statistics.fmean(warm_p99[-quarter:])
            ratio = tail / head if head > 0 else float("inf")
            _check(
                checks,
                f"后 1/4 窗口 p99 ≤ 前 1/4 ×{CRITERIA['max_p99_quarter_ratio']}",
                ratio <= CRITERIA["max_p99_quarter_ratio"],
                f"head={head:.2f}ms tail={tail:.2f}ms ratio={ratio:.3f}",
            )
        else:
            _check(checks, "后 1/4 窗口 p99 ≤ 前 1/4 ×1.2", False, f"预热后 p99 点不足（{len(warm_p99)}）")

        run = longest_monotonic_run(warm_p99)
        _check(
            checks,
            f"无连续 {CRITERIA['max_monotonic_run']} 点单调上升",
            run < CRITERIA["max_monotonic_run"],
            f"最长连续上升段={run}",
        )

        stats: dict = {}
        # ---- 趋势斜率：**整跑判据**（chunk 只记录累计斜率，不参与 pass/fail） ----
        # 30 分钟的窗口里 RSS 还处在启动后的分配爬坡，用它判「≤2MB/h」会把正常预热判成泄漏。
        if include_slope:
            if cumulative_slope:
                window_samples = samples
            else:
                skip = float(meta.get("slope_skip_seconds") or CRITERIA["slope_skip_seconds"])
                window_samples = [s for s in samples if float(s.get("t", 0.0)) >= skip]
                if len(window_samples) < 2:
                    window_samples = warm
            stats["slope_window_seconds"] = [
                round(float(window_samples[0]["t"])) if window_samples else None,
                round(float(window_samples[-1]["t"])) if window_samples else None,
            ]
            rss_slope = slope_per_hour(window_samples, "rss_bytes")
            if rss_slope is None:
                _check(checks, "趋势：RSS 斜率 ≤2MB/h", False, "样本不足，算不出斜率")
            else:
                rss_slope_mb = rss_slope / 1e6
                _check(
                    checks,
                    f"趋势：RSS 斜率 ≤{CRITERIA['max_rss_slope_mb_per_hour']}MB/h",
                    rss_slope_mb <= CRITERIA["max_rss_slope_mb_per_hour"],
                    f"rss_slope={rss_slope_mb:+.3f}MB/h（样本 {len(window_samples)}）",
                )
            handle_slope = slope_per_hour(window_samples, "handles")
            if handle_slope is None:
                _check(checks, "趋势：句柄斜率 ≤2/h", False, "样本不足，算不出斜率")
            else:
                _check(
                    checks,
                    f"趋势：句柄斜率 ≤{CRITERIA['max_handle_slope_per_hour']}/h",
                    handle_slope <= CRITERIA["max_handle_slope_per_hour"],
                    f"handle_slope={handle_slope:+.3f}/h",
                )

    return _window_stats(label, checks, samples, warm, p50, p99, requested, elapsed, stats, meta)


def _window_stats(
    label: str,
    checks: list[dict],
    samples: list[dict],
    warm: list[dict],
    p50: list[float],
    p99: list[float],
    requested: float,
    elapsed: float,
    stats: dict | None = None,
    meta: dict | None = None,
) -> dict:
    """把 stats / verdict 收尾（整跑与 chunk 共用）。"""
    stats = dict(stats or {})
    warm_rss = series(warm, "rss_bytes")
    warm_handles = series(warm, "handles")
    warm_pool = series(warm, "db_pool_connections")
    stats.update({
        "samples": len(samples),
        "warm_samples": len(warm),
        "requested_duration_seconds": requested,
        "elapsed_seconds": elapsed,
        "p50_ms_max": round(max(p50), 3) if p50 else None,
        "p99_ms_max": round(max(p99), 3) if p99 else None,
        "rss_first_mb": round(warm_rss[0] / 1e6, 2) if warm_rss else None,
        "rss_max_mb": round(max(warm_rss) / 1e6, 2) if warm_rss else None,
        "handle_swing": (max(warm_handles) - min(warm_handles)) if warm_handles else None,
        "db_pool_first": warm_pool[0] if warm_pool else None,
        "db_pool_max": max(warm_pool) if warm_pool else None,
        "rss_slope_mb_per_hour": round(slope_per_hour(samples, "rss_bytes") / 1e6, 3)
        if slope_per_hour(samples, "rss_bytes") is not None
        else None,
        "handle_slope_per_hour": round(slope_per_hour(samples, "handles"), 3)
        if slope_per_hour(samples, "handles") is not None
        else None,
        "write_failed_final": samples[-1].get("write_failed_total"),
        "dropped_final": samples[-1].get("packets_dropped_total"),
        "clients_requested": int((meta or {}).get("clients") or 0),
        "clients_entered": (
            None if (meta or {}).get("clients_entered") is None else int((meta or {})["clients_entered"])
        ),
        # 与 clients_entered **同进同退**：早退未采集时两者都写 None（绝不写 0 / 100 并存）。
        "active_min": (
            None
            if (meta or {}).get("clients_entered") is None
            else (min(series(samples, "connections_active")) if series(samples, "connections_active") else None)
        ),
        "active_max": (
            None
            if (meta or {}).get("clients_entered") is None
            else (max(series(samples, "connections_active")) if series(samples, "connections_active") else None)
        ),
    })
    warnings = detect_rss_steps(samples)
    verdict = "PASS" if all(item["ok"] for item in checks) else "FAIL"
    return {"window": label, "verdict": verdict, "checks": checks, "stats": stats, "warnings": warnings}


def print_verdict(report: dict, title: str) -> None:
    print(f"\n=== {title} ===")
    for item in report["checks"]:
        flag = "PASS" if item["ok"] else "FAIL"
        print(f"  [{flag}] {item['name']}  —— {item['detail']}")
    warnings = report.get("warnings") or []
    for warning in warnings:
        print(f"  [WARN] {warning['kind']} @t={warning['at_t']}s 单步 {warning['step_mb']}MB"
              f"（{warning['pre_mb']} → 最低 {warning['lowest_after_mb']}MB，仍高于台阶前 "
              f"{warning['held_above_pre_mb']}MB）— {warning['note']}")
    stats = report.get("stats") or {}
    if stats:
        print("  —— 统计 ——")
        for key, value in stats.items():
            print(f"     {key} = {value}")
    print(f"  判决：{report['verdict']}")


# ---------------------------------------------------------------------------
# chunk 汇总（fail-fast）
# ---------------------------------------------------------------------------
def chunk_windows(requested: float, chunk_seconds: float) -> list[tuple[int, float, float]]:
    """按 `chunk_seconds` 切窗口，返回 [(序号, 起, 止)]；最后一段可能不满。"""
    count = max(1, int(-(-requested // chunk_seconds)))
    return [(index + 1, index * chunk_seconds, min(requested, (index + 1) * chunk_seconds))
            for index in range(count)]


def evaluate_chunks(
    samples: list[dict],
    meta: dict,
    *,
    elapsed: float | None = None,
    require_complete: bool = False,
) -> tuple[list[dict], int | None]:
    """逐个 chunk 判定；返回 (chunk 报告列表, 首个 FAIL 的 chunk 序号或 None)。

    `elapsed` 给定时**只评已经跑到的窗口**（否则未跑到的空窗口会被判「采样非空 FAIL」——
    第 21 轮真跑第一次就踩了这个坑：第 1 个 chunk 时把 chunk-8 当成了失败项）。
    chunk 判据不含趋势斜率（斜率是 4h 级判据），只把**累计斜率**记录进报告。
    """
    requested = float(meta.get("requested_duration_seconds") or 0.0)
    chunk_seconds = float(meta.get("chunk_seconds") or CRITERIA["chunk_seconds"])
    windows = chunk_windows(requested, chunk_seconds)
    reports: list[dict] = []
    first_failure: int | None = None
    covered = elapsed if elapsed is not None else requested
    for index, start, end in windows:
        if start >= covered:
            break  # 还没跑到的窗口直接跳过
        if require_complete and end > covered:
            break  # 当前这一段还没跑满：实时触发时不评它（否则会拿 1 个采样点判「预热后样本不足」）
        window = [s for s in samples if start <= float(s.get("t", 0.0)) < end]
        if index == windows[-1][0] or end > covered:
            # 最后一段 / 尚未跑满的当前段：把 >= start 的全部纳入
            window = [s for s in samples if float(s.get("t", 0.0)) >= start]
        if not window:
            continue  # 该区间还没有任何采样点（刚跨过边界时会出现）
        cumulative = [s for s in samples if float(s.get("t", 0.0)) < end]
        report = evaluate_window(
            window,
            {**meta, "elapsed_seconds": min(end, covered)},
            chunk_index=index,
            label=f"chunk-{index} [{start:.0f}s,{end:.0f}s)",
            check_duration=False,
            cumulative_slope=True,
            include_slope=False,
            chunk_gate=True,
        )
        report["index"] = index
        report["range_seconds"] = [start, end]
        report["cumulative_slope"] = {
            "rss_slope_mb_per_hour": round(slope_per_hour(cumulative, "rss_bytes") / 1e6, 3)
            if slope_per_hour(cumulative, "rss_bytes") is not None
            else None,
            "handle_slope_per_hour": round(slope_per_hour(cumulative, "handles"), 3)
            if slope_per_hour(cumulative, "handles") is not None
            else None,
            "samples": len(cumulative),
        }
        reports.append(report)
        if report["verdict"] != "PASS" and first_failure is None:
            first_failure = index
    return reports, first_failure


def chunk_verdict(window_samples: list[dict], meta: dict, *, label: str) -> dict:
    """**纯函数**：只吃「某个窗口的样本切片」，返回判决；不看时钟、不写文件、不推进状态。

    实时路径只负责「喂满一个窗口的样本」再调它 —— 这样「窗口是否已完成」由**样本数**决定，
    不会再把「还没跑到 / 还没跑满」的窗口当成失败（第 21 轮前两次事故的根因）。
    """
    return evaluate_window(
        window_samples,
        meta,
        label=label,
        check_duration=False,
        cumulative_slope=True,
        include_slope=False,
    )


def detect_rss_steps(
    samples: list[dict],
    *,
    step_mb: float = 5.0,
    hold_seconds: float = 600.0,
    hold_mb: float = 2.0,
) -> list[dict]:
    """检测「**持续性** RSS 台阶」并只产 WARN（不参与 FAIL）。

    判据（门下限要求）：某一步 `Δ > step_mb`（默认 5MB）**且**其后 `hold_seconds`（600s）内
    RSS 仍高于台阶前 `hold_mb`（2MB）→ 记一条 `warn`。
    只看单步会被 GC / 分配器归还的噪声淹没（告警疲劳），所以必须看**是否维持高位**。
    """
    warnings = []
    for index in range(1, len(samples)):
        before = samples[index - 1].get("rss_bytes")
        after = samples[index].get("rss_bytes")
        if not isinstance(before, (int, float)) or not isinstance(after, (int, float)):
            continue
        delta_mb = (after - before) / 1e6
        if delta_mb <= step_mb:
            continue
        at_t = float(samples[index].get("t", 0.0))
        window = [
            float(s["rss_bytes"])
            for s in samples[index:]
            if isinstance(s.get("rss_bytes"), (int, float))
            and float(s.get("t", 0.0)) <= at_t + hold_seconds
        ]
        if not window:
            continue
        lowest_after_mb = (min(window) - before) / 1e6
        if lowest_after_mb >= hold_mb:
            warnings.append({
                "kind": "rss_step_persistent",
                "at_t": round(at_t, 1),
                "step_mb": round(delta_mb, 2),
                "pre_mb": round(before / 1e6, 2),
                "lowest_after_mb": round(min(window) / 1e6, 2),
                "held_above_pre_mb": round(lowest_after_mb, 2),
                "hold_window_seconds": hold_seconds,
                "note": "单步 >5MB 且 600s 内仍高于台阶前 ≥2MB：WARN（仅观察，不参与 FAIL）",
            })
    return warnings


def client_fields_consistency(stats: dict) -> list[str]:
    """检查「连接字段」是否自相矛盾：**禁止** `clients_entered=0` 与 `active_min>0` 并存。

    早退路径要么把 `clients_entered` 汇总出来，要么把它与 `active_min` **一起**标 `None`（未采集）。
    """
    entered = stats.get("clients_entered")
    active_min = stats.get("active_min")
    problems = []
    if entered == 0 and isinstance(active_min, (int, float)) and active_min > 0:
        problems.append(
            f"clients_entered=0 却 active_min={active_min}：早退时两字段必须一起标未采集"
        )
    return problems


def crosscheck_chunks(realtime: dict[int, dict], recomputed: dict[int, dict]) -> dict:
    """**线上数据的事后校验**：对比「实时路径写出的 chunk」与「结束时用整份样本重算的 chunk」。

    只比 `verdict`（判据细节可能因为「实时看到的是截断窗口」而不同，但结论必须一致）。
    不一致就说明实时路径算错了 —— 这正是前两次事故里没人能当场发现的东西。
    """
    mismatches = []
    for index in sorted(set(realtime) | set(recomputed)):
        live = realtime.get(index, {}).get("verdict")
        again = recomputed.get(index, {}).get("verdict")
        if live != again:
            mismatches.append({"chunk": index, "realtime": live, "recomputed": again})
    return {
        "ok": not mismatches,
        "chunks_realtime": len(realtime),
        "chunks_recomputed": len(recomputed),
        "mismatches": mismatches,
    }


# ---------------------------------------------------------------------------
# 采样
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
        # 慢泄漏观测（第 24 轮）：内部集合长度（kind 编译期固定 5 项）
        "internal_db_save": metrics.get('mir2_internal_records{kind="db_save"}'),
        "internal_mail_writes": metrics.get('mir2_internal_records{kind="mail_writes"}'),
        "internal_pending_hits": metrics.get('mir2_internal_records{kind="pending_hits"}'),
        "internal_pending_map_moves": metrics.get('mir2_internal_records{kind="pending_map_moves"}'),
        "internal_market_searches": metrics.get('mir2_internal_records{kind="market_searches"}'),
        "connections_active": metrics.get("mir2_connections_active"),
        "connections_rejected": metrics.get("mir2_connections_rejected"),
        "connections_total": metrics.get("mir2_connections_total"),
        "players_online": metrics.get("mir2_players_online"),
        "packets_dropped_total": metrics.get("mir2_packets_dropped_total"),
        "write_failed_total": metrics.get("mir2_write_failed_total"),
        "batch_write_failed_total": metrics.get("mir2_batch_write_failed_total"),
    }


def write_samples(out_dir: str, samples: list[dict], meta: dict, suffix: str = "") -> tuple[str, str]:
    """把原始采样落盘（`samples<suffix>.json` + `.csv`）。**正常结束与早退都要调用**。

    第 22 轮的教训：`FAIL_EARLY` 直接 return，导致 `samples.json` 缺失，队长与刑部无法复算台阶。
    """
    json_path = os.path.join(out_dir, f"samples{suffix}.json")
    csv_path = os.path.join(out_dir, f"samples{suffix}.csv")
    write_json(json_path, {"meta": meta, "samples": samples})
    keys = [
        "t", "timestamp", "rss_bytes", "handles", "alloc_bytes_per_sec", "db_pool_connections",
        "db_pool_idle", "tick_p50_ms", "tick_p99_ms", "tick_samples", "connections_active",
        "connections_rejected", "connections_total", "players_online", "packets_dropped_total",
        "internal_db_save", "internal_mail_writes", "internal_pending_hits",
        "internal_pending_map_moves", "internal_market_searches",
        "write_failed_total", "batch_write_failed_total",
    ]
    with open(csv_path, "w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=keys, extrasaction="ignore")
        writer.writeheader()
        writer.writerows(samples)
    return json_path, csv_path


def write_json(path: str, payload: dict) -> None:
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, ensure_ascii=False, indent=2)


def run_live(args) -> int:
    pid = resolve_server_pid(args.port, args.pid)
    if not pid:
        print(f"[FAIL] 定位不到监听 {args.port} 的服务端进程 —— 用 --pid 指定", file=sys.stderr)
        return 2
    probe = stress.ProcessProbe(pid)
    started = time.time()
    short = args.duration < CRITERIA["long_run_min_seconds"]
    suffix = "-short" if short else ""
    out_dir = args.out_dir or os.path.join("docs", "testing", "soak-data", time.strftime("%Y%m%d-%H%M%S"))
    out_dir = out_dir if os.path.isabs(out_dir) else os.path.join(ROOT, out_dir)
    os.makedirs(out_dir, exist_ok=True)

    meta = {
        "started_at": time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(started)),
        "requested_duration_seconds": args.duration,
        "clients": args.clients,
        "clients_requested": args.clients,
        "warmup_seconds": args.warmup,
        "sample_interval_seconds": args.sample_interval,
        "chunk_seconds": args.chunk_seconds,
        "rate_per_client": args.rate,
        "mix": args.mix,
        "server": f"{args.host}:{args.port}",
        "metrics_url": args.metrics_url,
        "pid": pid,
        "short_run": short,
        "criteria": CRITERIA,
    }

    workers = []
    for index in range(args.clients):
        worker = QuietWorker(
            index,
            (args.host, args.port),
            args.duration + 120,
            args.rate,
            tuple(int(part) for part in args.mix.split(",")),
            spread=True,
        )
        worker.start()
        workers.append(worker)
        time.sleep(args.connect_spacing)
    print(f"已拉起 {len(workers)} 个连接（间隔 {args.connect_spacing}s），开始采样（每 {args.sample_interval}s）")
    print(f"requested={args.duration:.0f}s chunk={args.chunk_seconds:.0f}s 产物后缀='{suffix}'")

    samples: list[dict] = []
    realtime_chunks: dict[int, dict] = {}
    previous_rss = None
    previous_at = None
    next_chunk_at = float(args.chunk_seconds)
    try:
        while True:
            sample = sample_once(probe, pid, args.metrics_url, started)
            rss = sample.get("rss_bytes")
            if previous_rss and previous_at and rss:
                delta_seconds = max(0.001, time.time() - previous_at)
                sample["alloc_bytes_per_sec"] = round((rss - previous_rss) / delta_seconds, 1)
            else:
                sample["alloc_bytes_per_sec"] = None
            previous_rss, previous_at = rss, time.time()
            samples.append(sample)

            # 服务端没了 → 立刻中止，别白等 4 小时。
            if sample.get("rss_bytes") is None and sample.get("tick_p99_ms") is None and sample.get("t", 0) > 30:
                print(f"  [ABORT] t=+{sample['t']:.0f}s 服务端进程/指标都读不到（PID {pid}）")
                meta["elapsed_seconds"] = round(time.time() - started, 1)
                meta["aborted"] = "server_unreachable"
                meta["clients_entered"] = sum(1 for worker in workers if worker.entered)
                report = evaluate_window(samples, meta, label="aborted")
                report["verdict"] = "FAIL_EARLY"
                report["meta"] = meta
                write_json(os.path.join(out_dir, f"verdict{suffix}.json"), report)
                write_samples(out_dir, samples, meta, suffix)
                print_verdict(report, "soak 中止（服务端不可达）")
                return 1

            print(
                f"  t=+{sample['t']:8.1f}s p50={sample['tick_p50_ms']}ms p99={sample['tick_p99_ms']}ms "
                f"rss={None if rss is None else round(rss / 1e6, 1)}MB handles={sample['handles']} "
                f"db_pool={sample['db_pool_connections']} dropped={sample['packets_dropped_total']} "
                f"write_failed={sample['write_failed_total']}"
            )

            # ---- chunk fail-fast ----
            elapsed = time.time() - started
            if elapsed >= next_chunk_at and len(samples) > 1:
                window_reports, first_failure = evaluate_chunks(
                    samples, meta, elapsed=elapsed, require_complete=True
                )
                if not window_reports:
                    next_chunk_at += float(args.chunk_seconds)
                    continue
                latest = window_reports[-1]
                # 把**这一轮新完成的所有 chunk** 都落盘（不只是最后一个），并记下实时结论
                for report_item in window_reports:
                    write_json(
                        os.path.join(out_dir, f"verdict-chunk-{report_item['index']}{suffix}.json"),
                        report_item,
                    )
                    realtime_chunks[report_item["index"]] = report_item
                chunk_path = os.path.join(out_dir, f"verdict-chunk-{latest['index']}{suffix}.json")
                print(
                    f"  [chunk-{latest['index']}] verdict={latest['verdict']} "
                    f"rss_slope={latest['cumulative_slope']['rss_slope_mb_per_hour']}MB/h "
                    f"handle_slope={latest['cumulative_slope']['handle_slope_per_hour']}/h → {chunk_path}"
                )
                if first_failure is not None:
                    failed = window_reports[first_failure - 1]
                    print(f"  [FAIL_EARLY] chunk-{first_failure} 判据未通过，立即停止：")
                    print_verdict(failed, f"chunk-{first_failure} 失败明细")
                    meta["elapsed_seconds"] = round(elapsed, 1)
                    meta["failed_chunk"] = first_failure
                    # 早退也要把「已进入世界的人数」汇总出来，避免 clients_entered 与 active_min 矛盾。
                    meta["clients_entered"] = sum(1 for worker in workers if worker.entered)
                    report = evaluate_window(samples, meta, label="fail_early")
                    report["verdict"] = "FAIL_EARLY"
                    report["chunks"] = window_reports
                    report["meta"] = meta
                    write_json(os.path.join(out_dir, f"verdict{suffix}.json"), report)
                    # 早退也必须落原始采样（第 22 轮缺口）
                    write_samples(out_dir, samples, meta, suffix)
                    return 1
                next_chunk_at += float(args.chunk_seconds)

            if time.time() - started >= args.duration:
                break
            time.sleep(args.sample_interval)
    finally:
        for worker in workers:
            worker.stop()
        for worker in workers:
            worker.join(timeout=5)

    meta["elapsed_seconds"] = round(time.time() - started, 1)
    meta["duration_seconds"] = meta["elapsed_seconds"]
    meta["clients_entered"] = sum(1 for worker in workers if worker.entered)
    dropped_workers = [worker for worker in workers if worker.error]
    meta["client_errors"] = len(dropped_workers)
    if dropped_workers:
        print(f"客户端异常 {len(dropped_workers)} 个，示例：{dropped_workers[0].error}")

    # 收尾：补齐剩余 chunk（含最后一段不满的），再做整跑判决。
    final_chunks, first_failure = evaluate_chunks(samples, meta, elapsed=meta["elapsed_seconds"])
    for report in final_chunks:
        write_json(os.path.join(out_dir, f"verdict-chunk-{report['index']}{suffix}.json"), report)

    recomputed_map = {item["index"]: item for item in final_chunks}
    crosscheck = crosscheck_chunks(realtime_chunks, recomputed_map)
    write_json(os.path.join(out_dir, f"chunk-crosscheck{suffix}.json"), crosscheck)
    if not crosscheck["ok"]:
        print(f"  [WARN] 实时 chunk 与重算结果不一致：{crosscheck['mismatches']}（见 chunk-crosscheck.json）")

    report = evaluate_window(samples, meta, label="full")
    report["meta"] = meta
    report["chunks"] = final_chunks
    report["chunk_crosscheck"] = crosscheck
    report["duration_ok"] = all(
        item["ok"] for item in report["checks"] if item["name"].startswith("时长自证")
    )
    if first_failure is not None:
        report["verdict"] = "FAIL_EARLY"
        meta["failed_chunk"] = first_failure

    write_samples(out_dir, samples, meta, suffix)
    write_json(os.path.join(out_dir, f"verdict{suffix}.json"), report)

    print_verdict(
        report,
        f"soak 判决（requested {meta['requested_duration_seconds']:.0f}s / "
        f"elapsed {meta['elapsed_seconds']:.0f}s / {len(samples)} 采样 / {len(final_chunks)} chunk）",
    )
    print(
        f"\n时长自证：requested={meta['requested_duration_seconds']:.0f}s "
        f"elapsed={meta['elapsed_seconds']:.1f}s duration_ok={report['duration_ok']}"
    )
    print(f"中间 verdict：{len(final_chunks)} 份 chunk；失败 chunk={meta.get('failed_chunk')}")
    print(
        f"原始数据：{os.path.relpath(out_dir, ROOT)}/samples{suffix}.json, "
        f"samples{suffix}.csv, verdict{suffix}.json"
    )
    return 0 if report["verdict"] == "PASS" else 1


# ---------------------------------------------------------------------------
# 负向自测：6 个案例（1 健康 + 5 坏样本）
# ---------------------------------------------------------------------------
def baseline_series(count: int = 1450, duration: float = 14400.0) -> list[dict]:
    """一组合格样本：p50≈3.5ms、p99 锯齿（避免误触「连续上升」）、RSS 斜率 ~0.7MB/h。"""
    step = duration / count
    return [
        {
            "t": round((index + 1) * step, 1),
            "rss_bytes": 120e6 + index * 2000.0,
            "handles": 300 + (index % 3),
            "db_pool_connections": 5,
            "tick_p50_ms": 3.5 + (index % 3) * 0.1,
            "tick_p99_ms": 18.0 + (index % 4) * 0.5,
            "write_failed_total": 0,
            "packets_dropped_total": 0,
            "connections_active": 100,
            "connections_rejected": 0,
            "connections_total": 100,
        }
        for index in range(count)
    ]


def base_meta(duration: float = 14400.0) -> dict:
    return {
        "requested_duration_seconds": duration,
        "elapsed_seconds": duration,
        "clients": REQUIRED_CLIENTS,
        "clients_requested": REQUIRED_CLIENTS,
        "clients_entered": REQUIRED_CLIENTS,
        "warmup_seconds": 180,
        "sample_interval_seconds": 10,
        "chunk_seconds": 1800,
    }


def self_check() -> int:
    cases: list[tuple[str, list[dict], dict, str]] = []

    # 0) 对照：健康 4h 样本必须 PASS（否则判据恒红，自证无意义）
    cases.append(("对照：健康 4h 样本", baseline_series(), base_meta(), "PASS"))

    # 1) RSS 翻倍
    rss_double = baseline_series()
    for sample in rss_double:
        if sample["t"] >= 180:
            sample["rss_bytes"] = 120e6 * (1 + (sample["t"] - 180) / (14400 - 180))
    cases.append(("坏样本 1：RSS 翻倍", rss_double, base_meta(), "FAIL"))

    # 2) p99 单调上升
    monotonic = baseline_series()
    for index, sample in enumerate(monotonic):
        sample["tick_p99_ms"] = 20.0 + index * 0.02
    cases.append(("坏样本 2：p99 单调上升", monotonic, base_meta(), "FAIL"))

    # 3) 单点 p99 = 90ms（验证绝对阈值真的生效）
    spike = baseline_series()
    spike[700]["tick_p99_ms"] = 90.0
    cases.append(("坏样本 3：单点 p99=90ms", spike, base_meta(), "FAIL"))

    # 4) 时长造假：requested=14400 但 elapsed=1800
    fraud = baseline_series(count=180, duration=1800.0)
    cases.append(
        ("坏样本 4：时长造假（requested=14400 / elapsed=1800）", fraud, {**base_meta(), "elapsed_seconds": 1800.0}, "FAIL")
    )

    failures: list[str] = []
    for title, samples, meta, expected in cases:
        report = evaluate_window(samples, meta, label=title)
        print_verdict(report, f"self-check · {title}（期望 {expected}）")
        if report["verdict"] != expected:
            failures.append(f"{title}：期望 {expected}，实际 {report['verdict']}")

    # 5) 8 个 chunk 里第 3 个 FAIL → 必须 FAIL_EARLY
    chunked = baseline_series()
    for sample in chunked:
        if 3600 <= sample["t"] < 5400 and abs(sample["t"] - 4500) < 6:
            sample["tick_p99_ms"] = 90.0
    window_reports, first_failure = evaluate_chunks(chunked, base_meta(), elapsed=14400.0)
    early = evaluate_window(chunked, {**base_meta(), "elapsed_seconds": 5400.0}, label="fail_early")
    early["verdict"] = "FAIL_EARLY" if first_failure is not None else "PASS"
    early["chunks"] = window_reports
    print_verdict(early, "self-check · 坏样本 5：8 chunk 中第 3 个 FAIL（期望 FAIL_EARLY）")
    print(
        f"  首个失败 chunk = {first_failure}（期望 3）；"
        f"chunk 判决 = {[report['verdict'] for report in window_reports]}"
    )
    if first_failure != 3 or early["verdict"] != "FAIL_EARLY":
        failures.append(
            f"chunk fail-fast：期望第 3 个 chunk → FAIL_EARLY，实际首个失败={first_failure} verdict={early['verdict']}"
        )

    # 6) 增量安全：只跑了 1 个 chunk 的数据时，**不许**因为「后面的 chunk 还没跑」而误判失败
    partial = [s for s in baseline_series() if s["t"] < 1800]
    partial_meta = {**base_meta(), "elapsed_seconds": 1807.0}
    partial_reports, partial_failure = evaluate_chunks(
        partial, partial_meta, elapsed=1807.0, require_complete=True
    )
    print(f"  [增量] 只跑 1 个 chunk（{len(partial)} 点）：chunk 判决="
          f"{[r['verdict'] for r in partial_reports]} 首个失败={partial_failure}（期望 None）")
    if partial_failure is not None:
        failures.append(f"增量误判：只跑 1 个 chunk 时不该失败，实际首个失败 chunk={partial_failure}")

    # 7) 逐边界模拟实时路径：每个 chunk 边界只评「已跑满」的窗口，全程不许出现误判
    sim = baseline_series()
    sim_meta = base_meta()
    boundary_ok = True
    for boundary in range(1800, 14401, 1800):
        elapsed_at = float(boundary + 5)
        subset = [s for s in sim if s["t"] < elapsed_at]
        reports, failure = evaluate_chunks(subset, sim_meta, elapsed=elapsed_at, require_complete=True)
        expected_count = boundary // 1800
        if failure is not None or len(reports) != expected_count:
            boundary_ok = False
            print(f"  [边界模拟] t={elapsed_at:.0f}s：chunk 数={len(reports)}（期望 {expected_count}）"
                  f" 首个失败={failure}")
    print(f"  [边界模拟] 8 个边界逐个检查：{'全部通过' if boundary_ok else '出现误判'}（期望全部通过）")
    if not boundary_ok:
        failures.append("边界模拟：在某个 chunk 边界上实时判定出现误判/漏评")

    # 8) 事后校验：篡改一份「实时结论」必须被 crosscheck 抓到
    live_map = {item["index"]: item for item in window_reports}
    tampered = {index: dict(item) for index, item in live_map.items()}
    tampered[3] = {**tampered[3], "verdict": "PASS"}  # 假装实时说它 PASS
    cross = crosscheck_chunks(tampered, live_map)
    print(f"  [事后校验] 篡改 chunk-3 的实时结论 → crosscheck.ok={cross['ok']} "
          f"不一致项={cross['mismatches']}（期望 ok=False）")
    if cross["ok"]:
        failures.append("事后校验：篡改实时结论后 crosscheck 仍报 ok=True（校验失效）")

    # 9) chunk 门禁抗噪：把第 3 次真跑 chunk-2 的「噪声三连」造出来，
    #    要求 chunk 门禁 PASS（它只管绝对延迟 + 区间 RSS），而**整跑**判据仍能抓真泄漏。
    noisy = baseline_series(count=180, duration=1800.0)
    for index, sample in enumerate(noisy):
        sample["handles"] = 373 + (index % 8)            # 波动 7
        sample["db_pool_connections"] = 2 if index < 90 else 3   # 池 2→3
        if 10 <= index < 17:                              # p99 连续上升 7 点
            sample["tick_p99_ms"] = 0.80 + (index - 10) * 0.02
    noisy_report = evaluate_window(noisy, base_meta(1800.0), label="chunk-noise", check_duration=False,
                                   include_slope=False, chunk_gate=True)
    print(f"  [抗噪] 噪声三连（句柄 7 波动 / 池 2→3 / p99 连续上升 7 点）→ chunk 门禁 "
          f"{noisy_report['verdict']}（期望 PASS）")
    if noisy_report["verdict"] != "PASS":
        failures.append("chunk 门禁被 30 分钟噪声误杀（应只在绝对延迟或区间 RSS 上判）")

    # 10) 早退字段一致性：构造「chunk-2 FAIL 早退」的两种 meta，断言不出现
    #     「clients_entered=0 + active_min=100」这类矛盾（第 21 轮刑部 medium）。
    early_samples = [s for s in baseline_series() if s["t"] < 3600]
    #  (a) 早退时**已回填**：entered=100 + active_min=100 → 一致
    meta_filled = {**base_meta(), "elapsed_seconds": 3600.0, "clients_entered": 100}
    report_filled = evaluate_window(early_samples, meta_filled, label="early-filled")
    problems_filled = client_fields_consistency(report_filled["stats"])
    #  (b) 早退时**未采集**：两个字段都应写 None（而不是 0 与 100 并存）
    meta_missing = {**base_meta(), "elapsed_seconds": 3600.0}
    meta_missing.pop("clients_entered", None)
    report_missing = evaluate_window(early_samples, meta_missing, label="early-missing")
    problems_missing = client_fields_consistency(report_missing["stats"])
    #  (c) 负例：故意造出旧 bug 的数据，断言校验器**能抓到**
    problems_buggy = client_fields_consistency({"clients_entered": 0, "active_min": 100})
    print(f"  [早退字段] 已回填：entered={report_filled['stats'].get('clients_entered')} "
          f"active_min={report_filled['stats'].get('active_min')} 矛盾={problems_filled}")
    print(f"  [早退字段] 未采集：entered={report_missing['stats'].get('clients_entered')} "
          f"active_min={report_missing['stats'].get('active_min')} 矛盾={problems_missing}")
    print(f"  [早退字段] 校验器对旧 bug 数据：{problems_buggy}（期望非空）")
    if problems_filled or problems_missing:
        failures.append(f"早退字段矛盾未被消除：filled={problems_filled} missing={problems_missing}")
    if not problems_buggy:
        failures.append("校验器抓不到 clients_entered=0 + active_min=100 的旧 bug（校验失效）")

    # 10b) 早退也必须落 samples.json / .csv（第 22 轮缺口：FAIL_EARLY 跳过收尾落盘）
    import tempfile
    with tempfile.TemporaryDirectory() as temp_dir:
        early_dir = os.path.join(temp_dir, "early-exit")
        os.makedirs(early_dir, exist_ok=True)
        early_meta = {**meta_missing, "elapsed_seconds": 3600.0, "failed_chunk": 2}
        early_report = evaluate_window(early_samples, early_meta, label="fail_early")
        early_report["verdict"] = "FAIL_EARLY"
        early_report["meta"] = early_meta
        write_json(os.path.join(early_dir, "verdict.json"), early_report)
        json_path, csv_path = write_samples(early_dir, early_samples, early_meta, "")
        saved = json.load(open(json_path, encoding="utf-8"))
        saved_count = len(saved.get("samples") or [])
        csv_lines = sum(1 for _ in open(csv_path, encoding="utf-8")) - 1
        print(f"  [早退落盘] samples.json 存在={os.path.exists(json_path)} 点数={saved_count}"
              f"（期望 {len(early_samples)}）| samples.csv 行数={csv_lines}")
        if not os.path.exists(json_path) or saved_count != len(early_samples) or csv_lines != len(early_samples):
            failures.append(
                f"早退未落 samples：exists={os.path.exists(json_path)} count={saved_count} "
                f"csv={csv_lines} 期望={len(early_samples)}"
            )

    # 11) 台阶**持续**高位 → 必须 WARN（仅观察，不影响 verdict）
    step_persist = baseline_series()
    for sample in step_persist:
        # 基线压平（不带爬坡），台阶放 t=10800 之后 +5.5MB 常驻：
        # 最小二乘斜率 ≈ 1.55MB/h（<2MB/h）、增长 ≈ 4.6%（<10%），
        # 这样「WARN 出现」与「verdict 仍 PASS」可以同时被验证（WARN 只观察、不参与 FAIL）。
        sample["rss_bytes"] = 120e6 + (5.5e6 if sample["t"] >= 10800 else 0.0)
    persist_report = evaluate_window(step_persist, base_meta(), label="step-persist")
    print(f"  [台阶持续] warnings={len(persist_report.get('warnings') or [])} verdict="
          f"{persist_report['verdict']}（期望 ≥1 条 WARN 且 verdict 仍 PASS）")
    if not (persist_report.get("warnings") or []) or persist_report["verdict"] != "PASS":
        failures.append(
            f"台阶持续未被记为 WARN：warnings={len(persist_report.get('warnings') or [])} "
            f"verdict={persist_report['verdict']}"
        )

    # 12) 涨后回落（GC/分配器归还）→ 必须**不** WARN
    step_drop = baseline_series()
    for sample in step_drop:
        if 3600 <= sample["t"] < 3700:
            sample["rss_bytes"] += 8e6          # 短暂抬高
        elif sample["t"] >= 3700:
            sample["rss_bytes"] += 0            # 600s 内回落
    drop_report = evaluate_window(step_drop, base_meta(), label="step-drop")
    print(f"  [涨后回落] warnings={len(drop_report.get('warnings') or [])} verdict="
          f"{drop_report['verdict']}（期望 0 条 WARN 且 verdict 仍 PASS）")
    if drop_report.get("warnings"):
        failures.append(f"涨后回落被误记为 WARN：{drop_report['warnings']}")

    # 13) chunk-1 预热块：区间 RSS +15% 必须 **PASS**（只记 observe_only WARN）
    ramp = baseline_series(count=180, duration=1800.0)
    for sample in ramp:                      # 前 30 分钟线性爬 15%
        sample["rss_bytes"] = 44e6 * (1 + 0.15 * (sample["t"] / 1800.0))
    chunk1_reports, chunk1_failure = evaluate_chunks(
        [{**sample, "tick_p50_ms": 0.58, "tick_p99_ms": 1.0} for sample in ramp],
        base_meta(),
        elapsed=1807.0,
        require_complete=True,
    )
    chunk1 = chunk1_reports[0] if chunk1_reports else {"verdict": "MISSING", "checks": [], "observe_warnings": []}
    observe_hits = [c for c in chunk1["checks"] if c.get("observe_only")]
    print(f"  [chunk-1 预热] verdict={chunk1['verdict']}（期望 PASS）observe_only 项="
          f"{len(observe_hits)} observe_warnings={len(chunk1.get('observe_warnings') or [])}")
    if chunk1["verdict"] != "PASS" or not observe_hits:
        failures.append(
            f"chunk-1 预热块未按 observe-only 处理：verdict={chunk1['verdict']} observe_only={len(observe_hits)}"
        )

    # 14) chunk-2 同样的 +15% 必须 **FAIL**（证明只软化了 chunk-1，不是全局放水）
    ramp2 = baseline_series(count=360, duration=3600.0)
    for sample in ramp2:
        if sample["t"] < 1800:
            sample["rss_bytes"] = 44e6
        else:
            # +10MB/30min：既超 10%（4.4MB）也超 8MB 绝对地板 → 必须 FAIL
            sample["rss_bytes"] = 44e6 + 10e6 * ((sample["t"] - 1800) / 1800.0)
    chunk2_reports, _ = evaluate_chunks(ramp2, base_meta(), elapsed=3607.0, require_complete=True)
    chunk2 = next((r for r in chunk2_reports if r["index"] == 2), {"verdict": "MISSING", "checks": []})
    print(f"  [chunk-2 硬判] verdict={chunk2['verdict']}（期望 FAIL）")
    if chunk2["verdict"] != "FAIL":
        failures.append(f"chunk-2 未按硬判据 FAIL（软化被扩大到其它块）：verdict={chunk2['verdict']}")

    # 15) 锯齿样本（Windows working set trimming）：44→36→49 摆动、中位数平稳 → 必须 PASS
    saw = baseline_series(count=360, duration=3600.0)
    pattern = [44.8, 44.9, 35.8, 35.8, 38.5, 40.0, 41.5, 43.0, 44.0, 45.0, 46.0, 47.0, 47.5, 48.0, 48.5, 49.0, 49.3]
    for index, sample in enumerate(saw):
        base = 37.1 if sample["t"] < 1800 else 44.8
        sample["rss_bytes"] = (base + pattern[index % len(pattern)] - 44.8) * 1e6 if sample["t"] < 1800             else pattern[index % len(pattern)] * 1e6
    saw_report = evaluate_window(
        [x for x in saw if x["t"] >= 1800],
        {**base_meta(), "elapsed_seconds": 3600.0},
        label="sawtooth",
        check_duration=False,
        include_slope=False,
        chunk_gate=True,
        chunk_index=2,
    )
    print(f"  [锯齿抗噪] verdict={saw_report['verdict']}（期望 PASS）增长={saw_report['stats']['rss_first_mb']}→"
          f"{saw_report['stats']['rss_max_mb']}MB")
    if saw_report["verdict"] != "PASS":
        failures.append(f"锯齿样本被误判：verdict={saw_report['verdict']}")

    # 16) 单调增长（30 分钟 +20MB）→ 必须 FAIL（证明加了地板仍能抓真泄漏）
    leak = baseline_series(count=180, duration=1800.0)
    for sample in leak:
        sample["rss_bytes"] = (47.0 + 20.0 * (sample["t"] / 1800.0)) * 1e6
    leak_report = evaluate_window(
        leak,
        {**base_meta(), "elapsed_seconds": 1800.0},
        label="chunk-leak",
        check_duration=False,
        include_slope=False,
        chunk_gate=True,
        chunk_index=2,
    )
    print(f"  [真泄漏仍红] verdict={leak_report['verdict']}（期望 FAIL）")
    if leak_report["verdict"] != "FAIL":
        failures.append(f"加了噪声地板后抓不到真泄漏：verdict={leak_report['verdict']}")

    print("\n=== self-check 结论 ===")
    if failures:
        for failure in failures:
            print(f"  [BROKEN] {failure}")
        print("  判据自证失败：合成坏样本没有被判 FAIL（判据可能恒绿）")
        return 1
    print(
        f"  [OK] {len(cases) + 1} 个案例全部符合预期"
        "（1 个健康 PASS + 4 个坏样本 FAIL + 1 个 chunk fail-fast FAIL_EARLY）"
    )
    print("  说明：本命令以 1 退出**属预期** —— 坏样本本该判 FAIL，这里把「会红」当证据。")
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description="长时 soak：相对 + 绝对 + 趋势斜率，chunk fail-fast")
    parser.add_argument("--server", default="127.0.0.1:7000")
    parser.add_argument("--metrics-url", default="http://127.0.0.1:9090/metrics")
    parser.add_argument("--clients", type=int, default=REQUIRED_CLIENTS)
    parser.add_argument(
        "--duration",
        type=float,
        default=CRITERIA["long_run_min_seconds"],
        help="请求时长（秒）。< 14400 会被拒绝，除非同时给 --allow-short",
    )
    parser.add_argument("--sample-interval", type=float, default=10.0)
    parser.add_argument("--chunk-seconds", type=float, default=float(CRITERIA["chunk_seconds"]))
    parser.add_argument("--warmup", type=float, default=float(CRITERIA["warmup_seconds"]))
    parser.add_argument("--rate", type=float, default=1.0)
    parser.add_argument("--mix", default="2,2,1")
    parser.add_argument("--connect-spacing", type=float, default=0.05)
    parser.add_argument("--pid", type=int, default=None)
    parser.add_argument("--out-dir", default="")
    parser.add_argument(
        "--allow-short",
        action="store_true",
        help="显式承认这是短跑（产物带 -short 后缀），用于开发冒烟",
    )
    parser.add_argument("--self-check", action="store_true")
    parser.add_argument("--analyze", default="", help="从既有 samples.json 复算判决（不压测）")
    args = parser.parse_args()

    if args.self_check:
        return self_check()

    if args.analyze:
        path = args.analyze if os.path.isabs(args.analyze) else os.path.join(ROOT, args.analyze)
        payload = json.load(open(path, encoding="utf-8"))
        samples_in = payload.get("samples") or []
        meta_in = payload.get("meta") or {}
        report = evaluate_window(samples_in, meta_in, label="recomputed")
        chunks, first_failure = evaluate_chunks(samples_in, meta_in,
                                                elapsed=float(meta_in.get("elapsed_seconds") or 0.0))
        report["chunks"] = chunks
        report["meta"] = meta_in
        report["duration_ok"] = all(item["ok"] for item in report["checks"] if item["name"].startswith("时长自证"))
        if first_failure is not None:
            report["verdict"] = "FAIL_EARLY"
        print_verdict(report, f"复算（samples={len(samples_in)}，slope 窗口={report['stats'].get('slope_window_seconds')}）")
        out = os.path.join(os.path.dirname(path), "verdict-recomputed.json")
        write_json(out, report)
        print(f"复算结果：{out}")
        return 0 if report["verdict"] == "PASS" else 1

    host, _, port = args.server.partition(":")
    args.host = host
    args.port = int(port or 7000)
    if args.clients != REQUIRED_CLIENTS:
        print(f"[FAIL] 规格要求 {REQUIRED_CLIENTS} 连接，收到 {args.clients}", file=sys.stderr)
        return 1
    # 反作弊：短跑不许产出长跑 verdict。
    if args.duration < CRITERIA["long_run_min_seconds"] and not args.allow_short:
        print(
            f"[REFUSED] --duration {args.duration:.0f}s < 长跑门槛 {CRITERIA['long_run_min_seconds']}s："
            "拒绝产出长跑 verdict（短跑请显式加 --allow-short，产物会带 -short 后缀）",
            file=sys.stderr,
        )
        return 2
    if args.duration < CRITERIA["long_run_min_seconds"]:
        print(f"[WARN] 短跑模式：{args.duration:.0f}s（产物 -short），不可作为长跑证据", file=sys.stderr)
    if not stress.scrape_metrics(args.metrics_url):
        print(f"[FAIL] 抓不到 /metrics（{args.metrics_url}）", file=sys.stderr)
        return 2
    return run_live(args)


if __name__ == "__main__":
    sys.exit(main())
