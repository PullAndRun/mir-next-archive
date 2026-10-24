#!/usr/bin/env python3
"""生成 Crystal classic 原生玩家库导入计划（C/A/AR 的 armour/hair/weapon）。

用法：
    py tools/assets/make_native_player_plan.py \
        --data-root resource/community/lomcn-artificialmir-client-data/ArtificialMir2/Data \
        --out target/native-layout/native-player-plan.json

键名规则：域前缀 + 规范化 stem（小写、空格转 `-`）。例如
`AWeapon/00 L.Lib -> aweapon-00-l`，既保留域隔离，也保留左右手。
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

DOMAINS = {
    "CArmour": "carmour",
    "CHair": "chair",
    "CWeapon": "cweapon",
    "AArmour": "aarmour",
    "AHair": "ahair",
    "AWeapon": "aweapon",
    "ARArmour": "ararmour",
    "ARHair": "arhair",
    "ARWeapon": "arweapon",
}


def key_for(prefix: str, path: Path) -> str:
    stem = path.stem.lower()
    stem = re.sub(r"[^a-z0-9]+", "-", stem).strip("-")
    return f"{prefix}-{stem}"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--data-root",
        default="resource/community/lomcn-artificialmir-client-data/ArtificialMir2/Data",
    )
    parser.add_argument("--out", default="target/native-layout/native-player-plan.json")
    args = parser.parse_args()

    root = Path(args.data_root)
    libraries = []
    seen: dict[str, Path] = {}
    for directory, prefix in DOMAINS.items():
        for lib in sorted((root / directory).glob("*.Lib"), key=lambda p: p.name.lower()):
            key = key_for(prefix, lib)
            if key in seen:
                raise SystemExit(f"重复 library_key={key}: {seen[key]} 与 {lib}")
            seen[key] = lib
            libraries.append(
                {
                    "lib": f"{directory}/{lib.name}",
                    "library_key": key,
                    "dest": f"gfx/sprites/player-native/{key}",
                    "reason": f"classic native player {directory}",
                }
            )

    if len(libraries) < 250:
        raise SystemExit(f"原生玩家库数量异常: {len(libraries)} < 250")

    plan = {
        "version": 1,
        "source_root": str(root).replace("\\", "/"),
        "note": "Crystal classic 原生玩家库：C/A/AR 的 armour+hair+weapon，域前缀隔离撞名。",
        "libraries": libraries,
    }
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(plan, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {out} libraries={len(libraries)}")


if __name__ == "__main__":
    main()
