#!/usr/bin/env python3
"""按 `used-assets.json` 分片并行调用 `mir-tools assets unpack`（拆包编排）。

为什么分片：单进程实测约 1,456 文件/分钟，全量 15 万 PNG 要 ~100 分钟；拆包是 CPU 密集（zlib+PNG），
按库帧数均衡切成 N 片并行可把 32 核机器压到 ~10 分钟。分片只按图库切（同一库不拆），输出目录互不重叠。

```powershell
# 默认 6 片并行，写完 target/parts/plan-part*.json，日志 target/unpack-part*.log
py tools/assets/split_extract.py --plan target/used-assets.json --out-root game-assets --parts 6

# 只分片不跑 / 跑完再全量 verify
py tools/assets/split_extract.py --dry-run
py tools/assets/split_extract.py --verify
```

`--mir-tools` 默认依次找：环境变量 `MIR_TOOLS`、`target/hubu-verify/debug/mir-tools.exe`、
`target/debug/mir-tools.exe`、`target/release/mir-tools.exe`、PATH 里的 `mir-tools`。
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_PLAN = REPO_ROOT / "target" / "used-assets.json"
DEFAULT_OUT = REPO_ROOT / "game-assets"


def find_mir_tools(explicit: str | None) -> Path:
    candidates = []
    if explicit:
        candidates.append(Path(explicit))
    if os.environ.get("MIR_TOOLS"):
        candidates.append(Path(os.environ["MIR_TOOLS"]))
    candidates.extend(
        REPO_ROOT / relative
        for relative in (
            "target/hubu-verify/debug/mir-tools.exe",
            "target/debug/mir-tools.exe",
            "target/release/mir-tools.exe",
            "target/hubu-verify/release/mir-tools.exe",
        )
    )
    for candidate in candidates:
        if candidate.is_file():
            return candidate.resolve()
    on_path = shutil.which("mir-tools")
    if on_path:
        return Path(on_path)
    raise SystemExit(
        "[错误] 找不到 mir-tools：用 --mir-tools 指定，或设置 MIR_TOOLS，或先 cargo build -p mir-tools"
    )


def split_plan(plan: dict, parts: int, work_dir: Path) -> list[tuple[Path, int, int]]:
    """按 used_frames 贪心装箱；返回 [(part 路径, 库数, 帧数)]。"""
    work_dir.mkdir(parents=True, exist_ok=True)
    bins = [0] * parts
    groups: list[list[dict]] = [[] for _ in range(parts)]
    for entry in sorted(plan["libraries"], key=lambda item: -len(item["indices"])):
        index = min(range(parts), key=lambda position: bins[position])
        groups[index].append(entry)
        bins[index] += len(entry["indices"])
    results = []
    for index, group in enumerate(groups, start=1):
        chunk = dict(plan)
        chunk["libraries"] = group
        chunk["scope"] = f"split_extract part {index}/{parts}"
        path = work_dir / f"plan-part{index}.json"
        path.write_text(json.dumps(chunk, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        results.append((path, len(group), bins[index - 1]))
    return results


def run_part(tool: Path, part: Path, out_root: Path, log_path: Path) -> tuple[Path, int, float]:
    log_path.parent.mkdir(parents=True, exist_ok=True)
    started = time.time()
    with log_path.open("wb") as log:
        process = subprocess.Popen(
            [str(tool), "assets", "unpack", "--plan", str(part), "--out-root", str(out_root)],
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        code = process.wait()
    return part, code, time.time() - started


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="分片并行拆包（mir-tools assets unpack）")
    parser.add_argument("--plan", default=str(DEFAULT_PLAN))
    parser.add_argument("--out-root", default=str(DEFAULT_OUT))
    parser.add_argument("--parts", type=int, default=6)
    parser.add_argument("--mir-tools", default=None)
    parser.add_argument("--work-dir", default=str(REPO_ROOT / "target" / "parts"))
    parser.add_argument("--log-dir", default=str(REPO_ROOT / "target"))
    parser.add_argument("--dry-run", action="store_true", help="只分片并打印计划，不启动拆包")
    parser.add_argument("--verify", action="store_true", help="全部完成后用全量计划跑 --verify")
    parser.add_argument("--verify-log", default=str(REPO_ROOT / "target" / "verify-hubu-full.log"))
    args = parser.parse_args(argv)

    plan_path = Path(args.plan).resolve()
    if not plan_path.is_file():
        raise SystemExit(f"[错误] 计划不存在：{plan_path}")
    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    parts = max(1, args.parts)
    out_root = Path(args.out_root).resolve()
    tool = find_mir_tools(args.mir_tools)
    work_dir = Path(args.work_dir).resolve()
    log_dir = Path(args.log_dir).resolve()

    splits = split_plan(plan, parts, work_dir)
    total_frames = sum(item[2] for item in splits)
    total_outputs = sum(len(entry["outputs"]) for entry in plan["libraries"])
    print(f"工具：{tool}")
    print(f"计划：{plan_path}（{len(plan['libraries'])} 库 / {total_outputs} outputs / {total_frames:,} 帧）")
    for index, (path, libs, frames) in enumerate(splits, start=1):
        print(f"  part{index}: {libs:>2} 库 / {frames:>7,} 帧 → {path.relative_to(REPO_ROOT)}")
    if args.dry_run:
        print("dry-run：不启动拆包")
        return 0

    started = time.time()
    results = []
    for index, (path, _libs, _frames) in enumerate(splits, start=1):
        log_path = log_dir / f"unpack-part{index}.log"
        results.append(run_part(tool, path, out_root, log_path))
    elapsed = time.time() - started
    failed = [item for item in results if item[1] != 0]
    for part, code, seconds in results:
        print(f"  {part.name}: exit={code} {seconds:.1f}s")
    print(f"并行拆包完成：{len(splits)} 片 / {elapsed:.1f}s，失败 {len(failed)}")
    if failed:
        return 1

    if args.verify:
        verify_log = Path(args.verify_log)
        verify_log.parent.mkdir(parents=True, exist_ok=True)
        print(f"全量校验：{tool} assets unpack --plan {plan_path} --out-root {out_root} --verify")
        with verify_log.open("wb") as log:
            process = subprocess.Popen(
                [str(tool), "assets", "unpack", "--plan", str(plan_path), "--out-root", str(out_root), "--verify"],
                stdout=log,
                stderr=subprocess.STDOUT,
            )
            code = process.wait()
        print(f"verify exit={code}，日志 {verify_log}")
        if code != 0:
            return code
    return 0


if __name__ == "__main__":
    sys.exit(main())
