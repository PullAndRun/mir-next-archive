#!/usr/bin/env python3
"""在 `mir2_dev` 素材索引里查文件（AGENTS.md 的资源策略第一站）。

    # 按文件名片段找
    python tools/asset-index/find_asset.py --name Mon
    # 按扩展名 + 目录找
    python tools/asset-index/find_asset.py --ext .wzl --top data --limit 20
    # 找出处（哪个社区仓库带来的）
    python tools/asset-index/find_asset.py --name Server.MirDB --json

索引由 `tools/asset-index/index_resources.py` 建立。
"""

from __future__ import annotations

import argparse
import json
import sys
from collections.abc import Iterable

import psycopg

from index_resources import dsn


QUERY = """
SELECT path, top, name, ext, size, mtime, sha256
FROM resources
WHERE (%(name)s::text IS NULL OR name ILIKE %(name)s::text)
  AND (%(ext)s::text  IS NULL OR ext = %(ext)s::text)
  AND (%(top)s::text  IS NULL OR top = %(top)s::text)
  AND (%(repo)s::text IS NULL OR path LIKE %(repo)s::text)
ORDER BY size DESC
LIMIT %(limit)s
"""


def human(size: int) -> str:
    units = ["B", "KB", "MB", "GB", "TB"]
    value = float(size)
    for unit in units:
        if value < 1024 or unit == units[-1]:
            return f"{value:.1f} {unit}"
        value /= 1024
    return f"{size} B"


def main(argv: Iterable[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--name", help="文件名片段（ILIKE，自动加 %% 通配）")
    parser.add_argument("--ext", help="扩展名，如 .wzl")
    parser.add_argument("--top", help="顶层目录：community/data/map/wav/ld")
    parser.add_argument("--repo", help="社区仓库名片段，例如 crystal-database")
    parser.add_argument("--limit", type=int, default=30)
    parser.add_argument("--json", action="store_true", help="输出 JSON（便于脚本处理）")
    args = parser.parse_args(list(argv))

    params = {
        "name": f"%{args.name}%" if args.name else None,
        "ext": args.ext.lower() if args.ext else None,
        "top": args.top,
        "repo": f"%{args.repo}%" if args.repo else None,
        "limit": args.limit,
    }
    with psycopg.connect(dsn()) as connection:
        with connection.cursor() as cursor:
            cursor.execute(QUERY, params)
            rows = cursor.fetchall()

    if args.json:
        print(
            json.dumps(
                [
                    {
                        "path": row[0],
                        "top": row[1],
                        "name": row[2],
                        "ext": row[3],
                        "size": row[4],
                        "mtime": row[5].isoformat() if row[5] else None,
                        "sha256": row[6],
                    }
                    for row in rows
                ],
                ensure_ascii=False,
                indent=2,
            )
        )
        return 0

    if not rows:
        print("索引里没有匹配的文件。可以先跑 index_resources.py，或联网下载后补索引。")
        return 1
    for path, top, _name, _ext, size, mtime, _sha in rows:
        stamp = mtime.strftime("%Y-%m-%d") if mtime else "-"
        print(f"{human(size):>10}  {stamp}  {path}")
    print(f"\n共 {len(rows)} 条（上限 {args.limit}）")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
