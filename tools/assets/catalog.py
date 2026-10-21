"""素材目录数据库（`assets` schema）辅助工具。

用途：`game-assets/` 已有 15 万级文件，用文件树管理「哪些文件、什么类型、多大、重复与否、
该打进哪个 `.mnp` 包」都很笨重。本工具把这些元数据同步进 PostgreSQL 的 `assets` schema，
只存**相对资源根的 POSIX 路径**，绝不写绝对地址。

用法：
```text
py tools/assets/catalog.py sync --root game-assets [--dsn ...] [--hash] [--prune] [--dry-run]
py tools/assets/catalog.py stats [--top 30]
py tools/assets/catalog.py changed --since <run_id>
py tools/assets/catalog.py dupes [--limit 30]
py tools/assets/catalog.py pack-plan --chunk-mb 16 [--out plan.json]
```

连接串：`--dsn` > 环境变量 `MIR2_ASSET_DB` > `.env` 的 `DATABASE_URL` > 环境变量 `DATABASE_URL`。
表不存在时自动建 `assets.catalog_run` / `assets.file`（见 `docs/assets/asset-catalog.md`）。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

SCHEMA_SQL = """
CREATE SCHEMA IF NOT EXISTS assets;

CREATE TABLE IF NOT EXISTS assets.catalog_run (
  id BIGSERIAL PRIMARY KEY,
  started_at timestamptz NOT NULL DEFAULT now(),
  finished_at timestamptz,
  root_rel text NOT NULL,
  files bigint,
  bytes bigint,
  note text
);

CREATE TABLE IF NOT EXISTS assets.file (
  id BIGSERIAL PRIMARY KEY,
  path text NOT NULL UNIQUE,
  category text NOT NULL,
  kind text NOT NULL,
  bytes bigint NOT NULL,
  sha256 text,
  output_dir text,
  origin text NOT NULL,
  run_id bigint REFERENCES assets.catalog_run(id),
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS assets_file_category_idx ON assets.file (category, kind);
CREATE INDEX IF NOT EXISTS assets_file_sha_idx ON assets.file (sha256);
"""

ABS_PATTERNS = re.compile(r"^(?:[A-Za-z]:[\\/]|[\\/])")
DRIVE_PATTERN = re.compile(r"^[A-Za-z]:")

TOP_CATEGORIES = {
    "gfx.tiles": "gfx/tiles",
    "gfx.sprites": "gfx/sprites",
    "gfx.ui": "gfx/ui",
}


def load_env_file(path: Path = Path(".env")) -> dict[str, str]:
    """极简 `.env` 读取：`KEY=VALUE`，忽略空行/注释，不展开变量。"""
    values: dict[str, str] = {}
    if not path.is_file():
        return values
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.strip()] = value.strip().strip('"').strip("'")
    return values


def resolve_dsn(explicit: str | None) -> str:
    if explicit:
        return explicit
    if os.environ.get("MIR2_ASSET_DB"):
        return os.environ["MIR2_ASSET_DB"]
    env_file = load_env_file()
    dsn = env_file.get("DATABASE_URL") or os.environ.get("DATABASE_URL")
    if not dsn:
        raise SystemExit("缺少连接串：--dsn / MIR2_ASSET_DB / .env DATABASE_URL 三选一")
    return dsn


def safe_relative(path: str) -> str:
    """校验并返回相对 POSIX 路径；绝对路径、`..`、盘符、反斜杠一律拒绝。"""
    if not path or path.startswith("/") or ABS_PATTERNS.match(path) or DRIVE_PATTERN.match(path):
        raise ValueError(f"禁止绝对路径：{path!r}")
    if "\\" in path:
        raise ValueError(f"只接受 POSIX 路径（反斜杠请先转换）：{path!r}")
    if "\x00" in path:
        raise ValueError("路径包含 NUL")
    parts = path.split("/")
    if any(part in ("", ".", "..") for part in parts):
        raise ValueError(f"路径包含空段/./..：{path!r}")
    return path


def posix_rel(root: Path, file: Path) -> str:
    rel = file.relative_to(root).as_posix()
    return safe_relative(rel)


def classify(rel: str) -> str:
    parts = rel.split("/")
    top = parts[0]
    if top == "gfx":
        if len(parts) > 1 and parts[1] == "tiles":
            return "gfx.tiles"
        if len(parts) > 1 and parts[1] == "sprites":
            return "gfx.sprites"
        if len(parts) > 1 and parts[1] == "ui":
            return "gfx.ui"
        return "gfx.other"
    if top in ("maps", "map"):
        return "maps"
    if top in ("data", "server"):
        return "data"
    if top == "audio":
        return "audio"
    if top == "packs":
        return "packs"
    if top == "fonts":
        return "fonts"
    if top == "index":
        return "index"
    if "/" not in rel and rel.endswith(".json"):
        return "index"
    return "other"


def kind_of(rel: str) -> str:
    name = rel.rsplit("/", 1)[-1]
    if "." not in name:
        return "none"
    return name.rsplit(".", 1)[-1].lower()


def output_dir_of(rel: str) -> str:
    parent = rel.rsplit("/", 1)[0] if "/" in rel else "."
    return parent or "."


def origin_of(rel: str) -> str:
    top = rel.split("/", 1)[0]
    if top in ("map", "server", "fonts"):
        return "imported"
    if rel.startswith("data/server/"):
        return "imported"
    return "generated"


def sha256_file(path: Path, chunk: int = 1024 * 1024) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while True:
            block = handle.read(chunk)
            if not block:
                break
            digest.update(block)
    return digest.hexdigest()


def walk_files(root: Path):
    for current, dirs, files in os.walk(root):
        dirs.sort()
        files.sort()
        current_path = Path(current)
        for name in files:
            yield current_path / name


def collect(root: Path, want_hash: bool) -> list[dict]:
    root = root.resolve()
    rows: list[dict] = []
    for file in walk_files(root):
        rel = posix_rel(root, file)
        stat = file.stat()
        rows.append(
            {
                "path": rel,
                "category": classify(rel),
                "kind": kind_of(rel),
                "bytes": stat.st_size,
                "sha256": sha256_file(file) if want_hash else None,
                "output_dir": output_dir_of(rel),
                "origin": origin_of(rel),
            }
        )
    return rows


def summarize(rows: list[dict]) -> None:
    total_bytes = sum(row["bytes"] for row in rows)
    by_category = Counter(row["category"] for row in rows)
    by_kind = Counter(row["kind"] for row in rows)
    bytes_by_category: dict[str, int] = defaultdict(int)
    for row in rows:
        bytes_by_category[row["category"]] += row["bytes"]
    print(f"文件总数：{len(rows)}；总字节：{total_bytes}（{total_bytes / 1024 / 1024:.1f} MiB）")
    print("\n按 category：")
    print(f"{'category':<14}{'files':>10}{'bytes':>16}{'MiB':>10}")
    for category, count in sorted(by_category.items(), key=lambda item: -item[1]):
        size = bytes_by_category[category]
        print(f"{category:<14}{count:>10}{size:>16}{size / 1024 / 1024:>10.1f}")
    print("\n按 kind（前 15）：")
    for kind, count in by_kind.most_common(15):
        print(f"  {kind:<10}{count:>10}")


def connect(dsn: str):
    try:
        import psycopg  # type: ignore
    except ImportError as error:  # pragma: no cover
        raise SystemExit(f"需要 psycopg（pip install psycopg）：{error}") from error
    connection = psycopg.connect(dsn)
    connection.autocommit = False
    return connection


def ensure_schema(connection) -> None:
    with connection.cursor() as cursor:
        cursor.execute(SCHEMA_SQL)
    connection.commit()


def upsert_batch(cursor, rows: list[dict], run_id: int) -> None:
    columns = ("path", "category", "kind", "bytes", "sha256", "output_dir", "origin", "run_id")
    paths = [row["path"] for row in rows]
    categories = [row["category"] for row in rows]
    kinds = [row["kind"] for row in rows]
    sizes = [row["bytes"] for row in rows]
    hashes = [row["sha256"] for row in rows]
    dirs = [row["output_dir"] for row in rows]
    origins = [row["origin"] for row in rows]
    run_ids = [run_id] * len(rows)
    sql = """
        INSERT INTO assets.file (path, category, kind, bytes, sha256, output_dir, origin, run_id)
        SELECT * FROM unnest(
            %s::text[], %s::text[], %s::text[], %s::bigint[], %s::text[],
            %s::text[], %s::text[], %s::bigint[]
        )
        ON CONFLICT (path) DO UPDATE SET
            category = EXCLUDED.category,
            kind = EXCLUDED.kind,
            bytes = EXCLUDED.bytes,
            sha256 = COALESCE(EXCLUDED.sha256, assets.file.sha256),
            output_dir = EXCLUDED.output_dir,
            origin = EXCLUDED.origin,
            run_id = EXCLUDED.run_id,
            updated_at = now()
        WHERE assets.file.bytes <> EXCLUDED.bytes
           OR assets.file.category <> EXCLUDED.category
           OR assets.file.kind <> EXCLUDED.kind
           OR assets.file.output_dir IS DISTINCT FROM EXCLUDED.output_dir
           OR assets.file.origin <> EXCLUDED.origin
           OR (EXCLUDED.sha256 IS NOT NULL AND assets.file.sha256 IS DISTINCT FROM EXCLUDED.sha256)
    """
    cursor.execute(
        sql,
        (paths, categories, kinds, sizes, hashes, dirs, origins, run_ids),
    )


def cmd_sync(args: argparse.Namespace) -> int:
    root = Path(args.root)
    if not root.is_dir():
        raise SystemExit(f"资源根不存在：{root}")
    if root.is_absolute():
        raise SystemExit("--root 必须是相对路径（库里只存相对地址）")
    root_rel = safe_relative(root.as_posix())
    rows = collect(root, args.hash)
    print(f"扫描 {root_rel}：{len(rows)} 个文件")
    if args.dry_run:
        summarize(rows)
        print("\n[dry-run] 未写数据库。")
        return 0

    connection = connect(resolve_dsn(args.dsn))
    try:
        ensure_schema(connection)
        with connection.cursor() as cursor:
            cursor.execute(
                "INSERT INTO assets.catalog_run (root_rel, note) VALUES (%s, %s) RETURNING id",
                (root_rel, args.note),
            )
            run_id = cursor.fetchone()[0]
        total = len(rows)
        done = 0
        batch = max(200, args.batch)
        with connection.cursor() as cursor:
            for start in range(0, total, batch):
                upsert_batch(cursor, rows[start : start + batch], run_id)
                done += len(rows[start : start + batch])
                if done % 20000 < batch:
                    print(f"  已写入 {done}/{total}")
            if args.prune:
                paths = [row["path"] for row in rows]
                cursor.execute(
                    "DELETE FROM assets.file WHERE NOT (path = ANY(%s::text[]))",
                    (paths,),
                )
                print(f"  已清理库里多余的 {cursor.rowcount} 行")
            total_bytes = sum(row["bytes"] for row in rows)
            cursor.execute(
                "UPDATE assets.catalog_run SET finished_at = now(), files = %s, bytes = %s WHERE id = %s",
                (total, total_bytes, run_id),
            )
        connection.commit()
    except Exception:
        connection.rollback()
        raise
    finally:
        connection.close()
    print(f"同步完成：run_id={run_id}，{total} 行")
    return 0


def cmd_stats(args: argparse.Namespace) -> int:
    connection = connect(resolve_dsn(args.dsn))
    try:
        ensure_schema(connection)
        with connection.cursor() as cursor:
            cursor.execute("SELECT count(*), COALESCE(sum(bytes), 0) FROM assets.file")
            files, total_bytes = cursor.fetchone()
            print(f"assets.file：{files} 行，{total_bytes} 字节（{total_bytes / 1024 / 1024:.1f} MiB）")
            cursor.execute(
                """
                SELECT category, kind, count(*), COALESCE(sum(bytes), 0)
                FROM assets.file
                GROUP BY category, kind
                ORDER BY sum(bytes) DESC
                LIMIT %s
                """,
                (args.top,),
            )
            print(f"\n{'category':<14}{'kind':<10}{'files':>10}{'MiB':>10}")
            for category, kind, count, size in cursor.fetchall():
                print(f"{category:<14}{kind:<10}{count:>10}{size / 1024 / 1024:>10.1f}")
            cursor.execute(
                """
                SELECT category, count(*), COALESCE(sum(bytes), 0)
                FROM assets.file GROUP BY category ORDER BY sum(bytes) DESC
                """
            )
            print(f"\n{'category':<14}{'files':>10}{'MiB':>10}")
            for category, count, size in cursor.fetchall():
                print(f"{category:<14}{count:>10}{size / 1024 / 1024:>10.1f}")
    finally:
        connection.close()
    return 0


def cmd_changed(args: argparse.Namespace) -> int:
    connection = connect(resolve_dsn(args.dsn))
    try:
        ensure_schema(connection)
        with connection.cursor() as cursor:
            cursor.execute(
                """
                SELECT category, kind, count(*), COALESCE(sum(bytes), 0)
                FROM assets.file WHERE run_id > %s
                GROUP BY category, kind ORDER BY count(*) DESC
                """,
                (args.since,),
            )
            rows = cursor.fetchall()
            print(f"run_id > {args.since} 的文件：")
            for category, kind, count, size in rows:
                print(f"  {category:<14}{kind:<10}{count:>8}{size / 1024 / 1024:>9.1f} MiB")
            cursor.execute(
                "SELECT id, started_at, finished_at, files, bytes, root_rel FROM assets.catalog_run ORDER BY id DESC LIMIT 10"
            )
            print("\n最近批次：")
            for run in cursor.fetchall():
                print(f"  run={run[0]} started={run[1]} finished={run[2]} files={run[3]} bytes={run[4]} root={run[5]}")
    finally:
        connection.close()
    return 0


def cmd_dupes(args: argparse.Namespace) -> int:
    connection = connect(resolve_dsn(args.dsn))
    try:
        ensure_schema(connection)
        with connection.cursor() as cursor:
            cursor.execute(
                """
                SELECT sha256, count(*), sum(bytes), min(path)
                FROM assets.file
                WHERE sha256 IS NOT NULL
                GROUP BY sha256 HAVING count(*) > 1
                ORDER BY sum(bytes) DESC LIMIT %s
                """,
                (args.limit,),
            )
            rows = cursor.fetchall()
            wasted = sum(size - size // count for _, count, size, _ in rows)
            print(f"重复组：{len(rows)}（浪费约 {wasted / 1024 / 1024:.1f} MiB）")
            for sha, count, size, sample in rows:
                print(f"  x{count:<4} {size:>12} B  {sha[:12]}…  {sample}")
    finally:
        connection.close()
    return 0


def cmd_pack_plan(args: argparse.Namespace) -> int:
    connection = connect(resolve_dsn(args.dsn))
    try:
        ensure_schema(connection)
        with connection.cursor() as cursor:
            cursor.execute(
                """
                SELECT category, path, bytes FROM assets.file
                ORDER BY category, path
                """
            )
            rows = cursor.fetchall()
    finally:
        connection.close()

    chunk_bytes = args.chunk_mb * 1024 * 1024
    chunks: list[dict] = []
    counters: dict[str, int] = defaultdict(int)

    def flush(chunk: dict) -> None:
        counters[chunk["category"]] += 1
        chunk["name"] = f"{chunk['category'].replace('.', '-')}-{counters[chunk['category']]}"
        chunk.pop("paths", None)
        chunks.append(chunk)

    current: dict | None = None
    for category, path, size in rows:
        if current is None:
            current = {"category": category, "files": 0, "bytes": 0, "paths": []}
        elif current["category"] != category or (
            current["bytes"] + size > chunk_bytes and current["files"] > 0
        ):
            flush(current)
            current = {"category": category, "files": 0, "bytes": 0, "paths": []}
        current["files"] += 1
        current["bytes"] += size
        current["paths"].append(path)
    if current is not None and current["files"]:
        flush(current)
    plan = {"chunk_mb": args.chunk_mb, "chunks": chunks}
    text = json.dumps(plan, ensure_ascii=False, indent=2)
    if args.out:
        Path(args.out).write_text(text + "\n", encoding="utf-8")
        print(f"已写出 {args.out}：{len(chunks)} 个分片")
    else:
        print(text)
    return 0


def cmd_selftest(args: argparse.Namespace) -> int:
    """不连数据库的纯函数自测：路径校验、分类、kind/output_dir/origin。"""
    rejected = ["/abs/path.png", "C:/win/path.png", "C:\\win\\path.png", "../up.png", "a//b.png", "a/./b.png"]
    for bad in rejected:
        try:
            safe_relative(bad)
        except ValueError:
            continue
        raise SystemExit(f"self-test 失败：应拒绝 {bad!r}")
    assert safe_relative("gfx/tiles/tiles0/0000.png") == "gfx/tiles/tiles0/0000.png"
    cases = {
        "gfx/tiles/tiles0/0000.png": ("gfx.tiles", "png", "gfx/tiles/tiles0", "generated"),
        "gfx/sprites/monster/mon-0130/0001.png": ("gfx.sprites", "png", "gfx/sprites/monster/mon-0130", "generated"),
        "maps/0.mnm": ("maps", "mnm", "maps", "generated"),
        "map/0.map": ("maps", "map", "map", "imported"),
        "audio/sfx/1.wav": ("audio", "wav", "audio/sfx", "generated"),
        "data/game.mnd": ("data", "mnd", "data", "generated"),
        "data/server/Server.MirDB": ("data", "mirdb", "data/server", "imported"),
        "server/Envir/Notice.txt": ("data", "txt", "server/Envir", "imported"),
        "packs/data-1.mnp": ("packs", "mnp", "packs", "generated"),
        "fonts/NotoSansSC-Regular.otf": ("fonts", "otf", "fonts", "imported"),
        "index/summary.json": ("index", "json", "index", "generated"),
        "manifest.json": ("index", "json", ".", "generated"),
    }
    for path, expected in cases.items():
        actual = (classify(path), kind_of(path), output_dir_of(path), origin_of(path))
        assert actual == expected, f"{path}: {actual} != {expected}"
    print(f"self-test 通过：路径校验 {len(rejected)} 条 + 分类样例 {len(cases)} 条")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="game-assets 素材目录数据库（assets schema）")
    parser.add_argument("--dsn", default=None, help="PostgreSQL 连接串（默认取 MIR2_ASSET_DB/.env DATABASE_URL）")
    sub = parser.add_subparsers(dest="command", required=True)

    def add_dsn(target: argparse.ArgumentParser) -> None:
        # 允许把 --dsn 放在子命令后面（队长给的用法是 `sync ... [--dsn ...]`）。
        target.add_argument("--dsn", default=argparse.SUPPRESS, help="PostgreSQL 连接串")

    sync = sub.add_parser("sync", help="遍历资源库并 upsert 到 assets.file")
    sync.add_argument("--root", default="game-assets", help="相对资源根（默认 game-assets）")
    sync.add_argument("--hash", action="store_true", help="计算 sha256（3GB 级别会慢一些）")
    sync.add_argument("--prune", action="store_true", help="删除库里已不存在的行")
    sync.add_argument("--dry-run", action="store_true", help="只扫描并打印统计，不写库")
    sync.add_argument("--batch", type=int, default=2000, help="每批行数（默认 2000）")
    sync.add_argument("--note", default=None, help="写进 catalog_run.note 的备注")
    add_dsn(sync)
    sync.set_defaults(func=cmd_sync)

    stats = sub.add_parser("stats", help="按 category/kind 汇总")
    stats.add_argument("--top", type=int, default=30)
    add_dsn(stats)
    stats.set_defaults(func=cmd_stats)

    changed = sub.add_parser("changed", help="两次同步之间新增/更新的文件")
    changed.add_argument("--since", type=int, required=True, help="基准 run_id")
    add_dsn(changed)
    changed.set_defaults(func=cmd_changed)

    dupes = sub.add_parser("dupes", help="按 sha256 找重复")
    dupes.add_argument("--limit", type=int, default=30)
    add_dsn(dupes)
    dupes.set_defaults(func=cmd_dupes)

    pack = sub.add_parser("pack-plan", help="按 category 出分包清单")
    pack.add_argument("--chunk-mb", type=int, default=16)
    pack.add_argument("--out", default=None, help="写出 JSON（默认 stdout）")
    add_dsn(pack)
    pack.set_defaults(func=cmd_pack_plan)

    selftest = sub.add_parser("self-test", help="纯函数自测（不连数据库）")
    selftest.set_defaults(func=cmd_selftest)

    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
