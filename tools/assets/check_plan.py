#!/usr/bin/env python3
"""拆包计划（`used-assets.json`）自检：命名、目录唯一性、帧数与索引一致性。

给拆包器前的门禁用（不读 `.wzl` 像素，只读计划 JSON；逐帧可解性由拆包器 `--verify` 负责）。

```powershell
py tools/assets/check_plan.py target/used-assets.json
py tools/assets/check_plan.py target/used-assets-mini.json --strict-range
```
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
FORBIDDEN_DEST = re.compile(r"\.(wzl|wzx|wil|wix)$|/(tiles|smtiles|objects\d*|mon\d+|npc\d*|hum\d*|weapon\d*|hair\d*)\.(wzl|wil)$")


def fail(errors: list[str], message: str) -> None:
    errors.append(message)


def check(path: Path, strict_range: bool) -> list[str]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    errors: list[str] = []
    libraries = payload.get("libraries") or []
    if not libraries:
        fail(errors, "libraries 为空")
        return errors

    dests: list[str] = []
    frame_sets: Counter[tuple[str, tuple[int, ...]]] = Counter()
    total_outputs = 0
    total_requested = 0
    total_loaded = 0
    total_missing = 0
    for entry in libraries:
        lib = entry.get("lib", "")
        outputs = entry.get("outputs") or []
        if not outputs:
            fail(errors, f"{lib}: outputs 为空（应至少一个目标目录）")
            continue
        union = sorted(entry.get("indices") or [])
        seen_union = set(union)
        for output in outputs:
            total_outputs += 1
            dest = output.get("dest", "")
            if not dest:
                fail(errors, f"{lib}: output 缺 dest")
                continue
            if not dest.startswith(("gfx/", "maps/", "audio/", "data/", "fonts/")):
                fail(errors, f"{lib}: dest 不在允许的顶层目录：{dest}")
            if FORBIDDEN_DEST.search(dest):
                fail(errors, f"{lib}: dest 含原库文件名/扩展名：{dest}")
            dests.append(dest)
            indices = output.get("indices") or []
            if indices != sorted(set(indices)):
                fail(errors, f"{lib}/{dest}: indices 非升序去重")
            if strict_range and entry.get("entries"):
                limit = int(entry["entries"])
                bad = [index for index in indices if not 0 <= index < limit]
                if bad:
                    fail(errors, f"{lib}/{dest}: {len(bad)} 个索引越界（entries={limit}，如 {bad[:3]}）")
            requested = output.get("requested", len(indices))
            if requested != len(indices):
                fail(errors, f"{lib}/{dest}: requested={requested} ≠ len(indices)={len(indices)}")
            loaded = output.get("loaded_frames")
            missing = output.get("missing_count")
            if loaded is not None and missing is not None and loaded + missing != len(indices):
                fail(errors, f"{lib}/{dest}: loaded({loaded})+missing({missing}) ≠ requested({len(indices)})")
            total_requested += len(indices)
            total_loaded += int(loaded or 0)
            total_missing += int(missing or 0)
            frame_sets[(lib.lower(), tuple(indices))] += 1
            # 顶层 indices 必须是所有 output 的并集（允许 output 是子集，但并集必须相等）
            if not set(indices) <= seen_union:
                fail(errors, f"{lib}/{dest}: output 索引不在库级 indices 里")
        if outputs and union:
            union_outputs = sorted({index for output in outputs for index in output.get("indices") or []})
            if union_outputs != union:
                fail(errors, f"{lib}: 库级 indices（{len(union)}）≠ outputs 并集（{len(union_outputs)}）")

    duplicates = [key for key, count in frame_sets.items() if count > 1]
    for lib, indices in duplicates:
        fail(errors, f"{lib}: 同一套帧被 {frame_sets[(lib, indices)]} 个 output 重复引用（{len(indices)} 帧）")
    dest_duplicates = [dest for dest, count in Counter(dests).items() if count > 1]
    for dest in dest_duplicates:
        fail(errors, f"dest 重复：{dest}")

    print(f"计划：{path}")
    print(
        f"  libraries={len(libraries)} outputs={total_outputs} "
        f"requested={total_requested:,} loaded={total_loaded:,} missing={total_missing:,}"
    )
    print(f"  dest 唯一={len(set(dests))}/{len(dests)}，重复帧集合={len(duplicates)}")
    if errors:
        print(f"\n[失败] {len(errors)} 个问题：", file=sys.stderr)
        for message in errors[:50]:
            print(f"  - {message}", file=sys.stderr)
    else:
        print("[通过] 命名/唯一性/帧数/索引一致性检查全部通过")
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="used-assets.json 自检")
    parser.add_argument("plan", nargs="?", default="target/used-assets.json")
    parser.add_argument("--strict-range", action="store_true", help="要求每个 index < entries（越界直接失败）")
    args = parser.parse_args(argv)
    errors = check(REPO_ROOT / args.plan, args.strict_range)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
