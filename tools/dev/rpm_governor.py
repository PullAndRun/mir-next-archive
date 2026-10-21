#!/usr/bin/env python3
"""动态 RPM 调速器（所有模型合计）。

背景：模型服务端有 **全部模型合计** 的每分钟请求上限；服务端故障时该上限会骤降。
目标：出现限流错误 → **下调** RPM；一段时间无错 → **自动回升**；把状态落盘，供队长在唤醒成员前查询。

设计（AIMD，加性增 / 乘性减）：
* 初始 `rpm = start`（默认 20）
* `--on-error`：`rpm = max(floor, rpm // 2)`（乘性减，floor 默认 2）
* `--tick`：若距上次错误 ≥ `recover_seconds`（默认 120s）且距上次提升 ≥ `raise_cooldown`
  （默认 60s），则 `rpm = min(ceiling, rpm + step)`（加性增，step 默认 2）
* `--wait`：按当前 rpm 做 **60 秒滑动窗口** 的令牌桶，必要时 sleep 到下一个可用槽位
* `--show` / `--reset`

状态文件：`target/rpm-governor.json`（不进 git）。

用法示例：
    py tools/dev/rpm_governor.py --show
    py tools/dev/rpm_governor.py --wait            # 唤醒成员前调用
    py tools/dev/rpm_governor.py --on-error        # 收到限流错误后
    py tools/dev/rpm_governor.py --tick            # 任务顺利完成后
    py tools/dev/rpm_governor.py --self-check
"""

from __future__ import annotations

import argparse
import json
import math
import sys
import time
from pathlib import Path

STATE_PATH = Path("target/rpm-governor.json")

DEFAULTS = {
    "rpm": 20,
    "start": 20,
    "floor": 2,
    "ceiling": 60,
    "step": 2,
    "recover_seconds": 120.0,
    "raise_cooldown": 60.0,
    "last_error_at": None,
    "last_raise_at": None,
    "window": [],  # 最近 60s 内已发出的请求时刻（unix 秒）
    "errors_total": 0,
    "decreases_total": 0,
    "increases_total": 0,
}


def load(path: Path = STATE_PATH) -> dict:
    if path.exists():
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            data = {}
    else:
        data = {}
    merged = dict(DEFAULTS)
    merged.update({k: v for k, v in data.items() if k in DEFAULTS})
    return merged


def save(state: dict, path: Path = STATE_PATH) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    trimmed = dict(state)
    trimmed["window"] = trimmed["window"][-400:]
    path.write_text(json.dumps(trimmed, ensure_ascii=False, indent=2), encoding="utf-8")


def on_error(state: dict, now: float) -> dict:
    state["rpm"] = max(state["floor"], state["rpm"] // 2)
    state["last_error_at"] = now
    state["errors_total"] += 1
    state["decreases_total"] += 1
    return state


def tick(state: dict, now: float) -> dict:
    last_error = state.get("last_error_at")
    if last_error is not None and now - last_error < state["recover_seconds"]:
        return state
    last_raise = state.get("last_raise_at") or 0.0
    if now - last_raise < state["raise_cooldown"]:
        return state
    if state["rpm"] >= state["ceiling"]:
        return state
    state["rpm"] = min(state["ceiling"], state["rpm"] + state["step"])
    state["last_raise_at"] = now
    state["increases_total"] += 1
    return state


def slot_delay(state: dict, now: float) -> float:
    """返回距离下一个可用请求槽还有多少秒（0 表示现在就可以发）。"""
    window = [t for t in state["window"] if now - t < 60.0]
    state["window"] = window
    limit = max(1, int(state["rpm"]))
    if len(window) < limit:
        state["window"].append(now)
        return 0.0
    # 窗口已满：等到最早那次请求滑出 60s 窗口
    oldest = min(window)
    return max(0.0, 60.0 - (now - oldest)) + 0.05


def describe(state: dict, now: float) -> str:
    last_error = state.get("last_error_at")
    since = "无" if last_error is None else f"{now - last_error:.0f}s 前"
    return (
        f"rpm={state['rpm']} (floor={state['floor']} ceiling={state['ceiling']} step={state['step']})\n"
        f"最近限流错误：{since}；窗口内请求={len([t for t in state['window'] if now - t < 60])}/{state['rpm']}\n"
        f"累计：errors={state['errors_total']} 降={state['decreases_total']} 升={state['increases_total']}"
    )


def self_check() -> int:
    """纯函数自检：AIMD 行为、恢复窗口、滑动窗口配额。返回 0 表示全部符合预期。"""
    failures = []

    def check(name, got, want):
        if got != want:
            failures.append(f"{name}: got={got} want={want}")

    # 1) 乘性减：20 → 10 → 5 → 2(floor)
    s = dict(DEFAULTS)
    for _ in range(5):
        on_error(s, 1000.0)
    check("floor", s["rpm"], 2)

    # 2) 恢复窗口内不回升
    s = dict(DEFAULTS, rpm=10, last_error_at=1000.0, last_raise_at=0.0)
    tick(s, 1000.0 + 60)  # 60s < recover_seconds(120)
    check("no raise within recover", s["rpm"], 10)

    # 3) 超过恢复窗口 + 冷却后加性增
    tick(s, 1000.0 + 200)
    check("raise after recover", s["rpm"], 12)

    # 4) 冷却期内不连续增
    tick(s, 1000.0 + 205)
    check("cooldown", s["rpm"], 12)

    # 5) 不超 ceiling
    s = dict(DEFAULTS, rpm=59, last_error_at=None, last_raise_at=0.0)
    tick(s, 10_000.0)
    check("ceiling", s["rpm"], 60)

    # 6) 滑动窗口：rpm=3 时前 3 次立即放行，第 4 次需要等到最早一格滑出 60s 窗口
    s = dict(DEFAULTS, rpm=3)
    delays = [slot_delay(s, 100.0) for _ in range(4)]
    check("slot first three", delays[:3], [0.0, 0.0, 0.0])
    # 同一时刻第 4 次：最早一格在 t=100，需等到 t=160 → 约 60s
    if not (59.9 <= delays[3] <= 60.2):
        failures.append(f"slot fourth: got={delays[3]} want≈60")
    # 6b) 若时间已经推进 3s（100→103），等待应相应缩短到约 57s
    s = dict(DEFAULTS, rpm=3)
    for offset in (100.0, 101.0, 102.0):
        slot_delay(s, offset)
    late = slot_delay(s, 103.0)
    if not (56.9 <= late <= 57.2):
        failures.append(f"slot late: got={late} want≈57")

    # 7) 滑出窗口后重新可用
    s = dict(DEFAULTS, rpm=1, window=[1000.0])
    check("slot after expiry", slot_delay(s, 1061.0), 0.0)

    # 8) on_error 记录次数
    s = dict(DEFAULTS)
    on_error(s, 5.0)
    check("errors_total", s["errors_total"], 1)

    # 9) tick 在 rpm 已达 ceiling 时不再增加
    s = dict(DEFAULTS, rpm=60, last_error_at=None, last_raise_at=0.0)
    before = s["increases_total"]
    tick(s, 10.0)
    check("no raise at ceiling", s["increases_total"], before)

    # 10) 状态序列化往返
    s = dict(DEFAULTS, rpm=7)
    tmp = Path("target/rpm-governor.selfcheck.json")
    save(s, tmp)
    check("roundtrip rpm", load(tmp)["rpm"], 7)
    tmp.unlink(missing_ok=True)

    if failures:
        print("[FAIL] 自检失败：")
        for item in failures:
            print("  -", item)
        return 1
    print("[OK] 10 项自检全部符合预期")
    return 0


def scan_logs(log_dir: Path, minutes: float, now: float) -> int:
    """扫描 DSH 会话日志，统计最近 N 分钟内的限流错误条数（不做守护进程）。"""
    if not log_dir.exists():
        return 0
    cutoff = now - minutes * 60.0
    markers = ("rate limit", "rate_limit", "rpm", "429", "too many requests")
    hits = 0
    for path in sorted(log_dir.glob("*.jsonl"))[-2:]:
        try:
            with path.open("r", encoding="utf-8", errors="replace") as handle:
                for line in handle:
                    lowered = line.lower()
                    if not any(marker in lowered for marker in markers):
                        continue
                    stamp = _line_time(line)
                    if stamp is None or stamp < cutoff:
                        continue
                    hits += 1
        except OSError:
            continue
    return hits


def _line_time(line: str) -> float | None:
    """从日志行里取 `"time":"<ISO8601>"` 并转成 unix 秒；取不到返回 None。"""
    key = '"time":"'
    start = line.find(key)
    if start < 0:
        return None
    start += len(key)
    end = line.find('"', start)
    if end < 0:
        return None
    raw = line[start:end]
    try:
        from datetime import datetime

        return datetime.fromisoformat(raw.replace("Z", "+00:00")).timestamp()
    except ValueError:
        return None


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="动态 RPM 调速器（全部模型合计）")
    parser.add_argument("--show", action="store_true", help="打印当前状态")
    parser.add_argument("--wait", action="store_true", help="按当前 rpm 等待到可用槽位")
    parser.add_argument("--on-error", action="store_true", help="记录一次限流错误并下调 rpm")
    parser.add_argument("--tick", action="store_true", help="任务顺利完成后尝试回升 rpm")
    parser.add_argument("--reset", action="store_true", help="恢复默认状态")
    parser.add_argument("--self-check", action="store_true", help="跑纯函数自检")
    parser.add_argument("--scan-log", type=str, default=None,
                        help="扫描 DSH 日志目录中的限流错误（命中则自动下调）")
    parser.add_argument("--ceiling", type=int, default=None, help="覆盖上限（例如临时降到 10）")
    args = parser.parse_args(argv)

    if args.self_check:
        return self_check()

    if args.reset:
        save(dict(DEFAULTS))
        print("[OK] 已重置")
        return 0

    state = load()
    if args.ceiling is not None:
        state["ceiling"] = max(state["floor"], args.ceiling)
        state["rpm"] = min(state["rpm"], state["ceiling"])
    now = time.time()

    if args.scan_log:
        state = load()
        now = time.time()
        hits = scan_logs(Path(args.scan_log), 15.0, now)
        if hits > 0:
            on_error(state, now)
            save(state)
            print(f"[LIMIT] 日志命中 {hits} 条限流痕迹 → rpm={state['rpm']}")
        else:
            print("[OK] 日志中未见限流痕迹")
        return 0

    if args.on_error:
        on_error(state, now)
        save(state)
        print(f"[LIMIT] 下调 → rpm={state['rpm']}")
        return 0

    if args.tick:
        before = state["rpm"]
        tick(state, now)
        save(state)
        print(f"[OK] rpm={state['rpm']}（本次{'提升' if state['rpm'] > before else '不变'}）")
        return 0

    if args.wait:
        delay = slot_delay(state, now)
        save(state)
        if delay > 0:
            print(f"[WAIT] 配额已满（rpm={state['rpm']}），等待 {delay:.1f}s")
            time.sleep(delay)
        return 0

    print(describe(state, now))
    save(state)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
