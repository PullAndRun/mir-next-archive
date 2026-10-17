#!/usr/bin/env python3
"""把 `resource/` 目录里的素材登记进 `mir2_dev` 索引库。

资源策略（见 AGENTS.md）：需要素材时先查 `mir2_dev` 索引，再按索引去 `resource/`
取文件；索引里没有的就联网下载、放进 `resource/` 并补索引。
本脚本负责「建立/更新索引」这一半，可以反复运行（按 `path` 幂等 upsert）。

用法：

    # 默认读 .env 里的 MIR2_DEV_DATABASE_URL / DATABASE_URL
    python tools/asset-index/index_resources.py

    # 只索引某些目录，或只打印不写库
    python tools/asset-index/index_resources.py --roots data map wav
    python tools/asset-index/index_resources.py --dry-run --limit 200

为什么按「路径 + 大小 + 修改时间」而不是内容哈希：仓库里有 230 万个文件、300 GB，
全量哈希要跑几个小时；大小+mtime 足以判断「文件换过了没有」，需要更强校验时可以
用 `--hash-ext .pak` 只对特定扩展名算 BLAKE2b。
"""

from __future__ import annotations

import argparse
import hashlib
import os
import sys
from collections.abc import Iterable, Iterator
from datetime import UTC, datetime
from pathlib import Path

try:
    import psycopg
except ImportError:  # pragma: no cover - 只在使用 --dry-run 时可能缺
    psycopg = None


SCHEMA = """
CREATE TABLE IF NOT EXISTS resources (
    id          BIGSERIAL PRIMARY KEY,
    path        TEXT        NOT NULL UNIQUE,
    top         TEXT        NOT NULL,
    name        TEXT        NOT NULL,
    ext         TEXT        NOT NULL DEFAULT '',
    size        BIGINT      NOT NULL,
    mtime       TIMESTAMPTZ,
    sha256      TEXT,
    indexed_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS resources_name_lower_idx ON resources (lower(name));
CREATE INDEX IF NOT EXISTS resources_ext_idx ON resources (ext);
CREATE INDEX IF NOT EXISTS resources_top_idx ON resources (top);

CREATE TABLE IF NOT EXISTS resource_sources (
    path        TEXT PRIMARY KEY,
    content     TEXT NOT NULL,
    indexed_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
"""


def repo_env() -> dict[str, str]:
    """读取仓库根目录的 `.env`（不覆盖已有的环境变量）。"""
    values: dict[str, str] = {}
    env_path = Path(__file__).resolve().parents[2] / ".env"
    if not env_path.exists():
        return values
    for line in env_path.read_text(encoding="utf-8", errors="replace").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        values[key.strip()] = value.strip().strip('"').strip("'")
    return values


def dsn() -> str:
    env = repo_env()
    for key in ("MIR2_DEV_DATABASE_URL", "DATABASE_URL"):
        value = os.environ.get(key) or env.get(key)
        if value:
            return value
    raise SystemExit("缺少 MIR2_DEV_DATABASE_URL / DATABASE_URL")


def _long_path(path: str) -> str:
    """Windows 上超过 260 字符的路径要加 `\\\\?\\` 前缀才能 stat/scandir。"""
    if os.name != "nt":
        return path
    if path.startswith("\\\\?\\"):
        return path
    absolute = os.path.abspath(path)
    if absolute.startswith("\\\\"):
        return "\\\\?\\UNC\\" + absolute[2:]
    return "\\\\?\\" + absolute


def walk(root: Path) -> Iterator[tuple[str, str, str, int, datetime | None, str]]:
    """遍历目录，产出 (相对 resource/ 的路径, top, name, size, mtime, ext)。

    自己维护相对路径而不是 `Path.relative_to`：社区仓库里有超过 260 字符的深层路径，
    只有 scandir 走 `\\\\?\\` 前缀才稳定，而 `entry.path` 会带上前缀。
    """
    top = root.name
    stack: list[tuple[str, str]] = [(str(root), "")]
    while stack:
        directory, prefix = stack.pop()
        try:
            entries = list(os.scandir(_long_path(directory)))
        except OSError as error:
            print(f"跳过无法读取的目录 {directory}: {error}", file=sys.stderr)
            continue
        for entry in entries:
            relative = f"{prefix}{entry.name}"
            try:
                if entry.is_dir(follow_symlinks=False):
                    stack.append((entry.path, f"{relative}/"))
                    continue
                stat = entry.stat(follow_symlinks=False)
            except OSError:
                continue
            ext = os.path.splitext(entry.name)[1].lower()
            try:
                mtime = datetime.fromtimestamp(stat.st_mtime, tz=UTC)
            except (OSError, OverflowError, ValueError):
                # 归档里有些文件的时间戳是 0 / 越界值（Windows 上 fromtimestamp 会报 EINVAL）。
                mtime = None
            yield f"{top}/{relative}", top, entry.name, stat.st_size, mtime, ext


def content_hash(path: Path, algorithm: str = "sha256") -> str:
    digest = hashlib.new(algorithm)
    with open(_long_path(str(path)), "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


STAGE = """
CREATE TEMP TABLE resources_stage (
    path   TEXT,
    top    TEXT,
    name   TEXT,
    ext    TEXT,
    size   BIGINT,
    mtime  TIMESTAMPTZ,
    sha256 TEXT
) ON COMMIT DROP;
"""

MERGE = """
INSERT INTO resources (path, top, name, ext, size, mtime, sha256)
SELECT path, top, name, ext, size, mtime, sha256 FROM resources_stage
ON CONFLICT (path) DO UPDATE SET
    top = EXCLUDED.top,
    name = EXCLUDED.name,
    ext = EXCLUDED.ext,
    size = EXCLUDED.size,
    mtime = EXCLUDED.mtime,
    sha256 = COALESCE(EXCLUDED.sha256, resources.sha256),
    indexed_at = now()
WHERE resources.size <> EXCLUDED.size
   OR resources.mtime IS DISTINCT FROM EXCLUDED.mtime
   OR resources.name <> EXCLUDED.name
   OR resources.top <> EXCLUDED.top
"""


def index(
    connection,
    resource_root: Path,
    roots: list[Path],
    hash_ext: set[str],
    limit: int | None,
    dry_run: bool,
) -> tuple[int, int]:
    """扫描文件并写进 `resources` 表。

    走「临时表 + COPY + MERGE」：230 万个文件如果逐行 `INSERT ... ON CONFLICT`，
    光网络往返就要几小时；COPY 是流式的，整体能在几分钟内跑完。
    """
    seen = 0
    written = 0
    buffer: list[tuple] = []
    cursor = None
    if not dry_run:
        cursor = connection.cursor()
        cursor.execute(STAGE)

    def flush() -> None:
        nonlocal buffer, written
        if not buffer:
            return
        written += len(buffer)
        if not dry_run:
            with cursor.copy(
                "COPY resources_stage (path, top, name, ext, size, mtime, sha256) FROM STDIN"
            ) as copy:
                for row in buffer:
                    copy.write_row(row)
        buffer = []

    for root in roots:
        if not root.exists():
            print(f"跳过不存在的目录：{root}", file=sys.stderr)
            continue
        for relative, top, name, size, mtime, ext in walk(root):
            seen += 1
            digest = None
            if ext in hash_ext:
                try:
                    digest = content_hash(resource_root / relative.split("/", 1)[1])
                except OSError:
                    digest = None
            buffer.append((relative, top, name, ext, size, mtime, digest))
            if len(buffer) >= 20000:
                flush()
                print(f"  已扫描 {seen} 个文件，暂存 {written} 行…", flush=True)
            if limit is not None and seen >= limit:
                break
        flush()
        if limit is not None and seen >= limit:
            break
    flush()

    if not dry_run:
        cursor.execute(MERGE)
        connection.commit()
        cursor.close()
    return seen, written


def index_sources(connection, resource_root: Path, dry_run: bool) -> int:
    """把各社区仓库的 `SOURCES.md` 摘要登记下来，方便知道素材出处。"""
    records: list[tuple[str, str]] = []
    for path in resource_root.glob("community/*/SOURCES.md"):
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        records.append((path.relative_to(resource_root).as_posix(), text[:20000]))
    if dry_run or not records:
        return len(records)
    with connection.cursor() as cursor:
        cursor.executemany(
            """
            INSERT INTO resource_sources (path, content) VALUES (%s, %s)
            ON CONFLICT (path) DO UPDATE SET content = EXCLUDED.content, indexed_at = now()
            """,
            records,
        )
    return len(records)


def main(argv: Iterable[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--resource-root",
        type=Path,
        default=Path(__file__).resolve().parents[2] / "resource",
        help="resource 目录（默认仓库根下的 resource/）",
    )
    parser.add_argument("--roots", nargs="*", default=None, help="只索引这些子目录")
    parser.add_argument("--limit", type=int, default=None, help="最多扫描多少个文件（调试用）")
    parser.add_argument(
        "--hash-ext",
        nargs="*",
        default=[],
        help="对这些扩展名额外计算 sha256（如 .pak .dat）",
    )
    parser.add_argument("--dry-run", action="store_true", help="只统计不写库")
    args = parser.parse_args(list(argv))

    root: Path = args.resource_root
    if not root.is_dir():
        raise SystemExit(f"找不到 resource 目录：{root}")
    roots = [root / name for name in args.roots] if args.roots else [
        path for path in sorted(root.iterdir()) if path.is_dir()
    ]
    hash_ext = {ext if ext.startswith(".") else f".{ext}" for ext in map(str.lower, args.hash_ext)}

    print(f"索引 {root} 下的 {len(roots)} 个目录：", ", ".join(path.name for path in roots))
    if args.dry_run:
        seen, written = index(None, root, roots, hash_ext, args.limit, True)
        print(f"dry-run：扫描 {seen} 个文件（未写入）")
        return 0

    if psycopg is None:
        raise SystemExit("需要 psycopg：pip install 'psycopg[binary]'")

    with psycopg.connect(dsn()) as connection:
        with connection.cursor() as cursor:
            cursor.execute(SCHEMA)
        connection.commit()
        seen, written = index(connection, root, roots, hash_ext, args.limit, False)
        sources = index_sources(connection, root, False)
        connection.commit()
    print(f"完成：扫描 {seen} 个文件，写入 {written} 行，登记 {sources} 份 SOURCES.md")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except SystemExit:
        raise
    except BaseException:  # noqa: BLE001 - 顶层兜底，打印完整栈方便定位坏文件名
        import traceback

        traceback.print_exc()
        raise SystemExit(2) from None
