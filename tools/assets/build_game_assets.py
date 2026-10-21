#!/usr/bin/env python3
"""把游戏**真正用到的素材**抽取成一个独立资源目录 `game-assets/`。

设计要点（第 25 轮需求："不要直接在 resource 文件夹引用，游戏要支持手动指定资源文件夹"）：

- 目标结构（运行期只认这一份）：

      game-assets/
        data/     ← resource/data      （客户端图库 .wzl/.wzx/.wil、UI、minimap、books）
        map/      ← cjlaaa/Maps        （默认；服务端 Server.MirDB 引用的地图）或 resource/map
        wav/      ← resource/wav       （音效）
        fonts/    ← resource/fonts
        ld/       ← resource/ld        （图库索引）
        server/   ← cjlaaa/Server.MirDB + cjlaaa/Envir/
        manifest.json                  （每个子树：来源/文件数/字节数/生成时间 + 指纹）

- **幂等**：manifest 与来源一致 → 跳过；不一致 → 打印差异并按 `size+mtime` 增量复制。
- **verify**：只比对目标与 manifest（文件数/字节数/抽样 sha256），不碰来源，退出码 0/1。
- **smoke**：`--smoke N` 每个子树只取前 N 个文件，产物 manifest 标 `"smoke": true`，
  用于在不复制 23GB 的前提下验证流程（测试/CI 也用它）。
- 不复制可执行体（`.exe/.dll/.pdb/.so/...`），对齐仓库既有采集纪律。

用法：

    py tools/assets/build_game_assets.py --verify                  # 校验已有产物
    py tools/assets/build_game_assets.py --smoke 20 --target target/game-assets-smoke
    py tools/assets/build_game_assets.py                           # 全量（约 23GB，建议后台跑）
    py tools/assets/build_game_assets.py --map-source resource      # 用 resource/map 而不是 cjlaaa
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import sys
import time
from dataclasses import dataclass, field

MANIFEST_NAME = "manifest.json"
MANIFEST_VERSION = 1

# 不复制可执行体/构建产物（对齐采集纪律）
EXCLUDE_SUFFIXES = {
    ".exe", ".dll", ".pdb", ".so", ".dylib", ".lib", ".obj", ".ilk",
    ".bat", ".cmd", ".ps1", ".msi", ".zip", ".7z", ".rar",
}

DEFAULT_CJCLAA_DIR = os.path.join(
    "resource", "community", "suprcode-crystal-database", "cjlaaa"
)


@dataclass
class Subtree:
    """一个要抽取的子树。"""

    name: str                     # 目标目录名（data/map/wav/...）
    source: str                   # 来源目录（绝对或相对仓库根）
    description: str
    files: list[tuple[str, str, int]] = field(default_factory=list)  # (相对路径, 绝对路径, 字节)

    @property
    def total_bytes(self) -> int:
        return sum(size for _, _, size in self.files)


def human(num_bytes: float) -> str:
    for unit in ("B", "KB", "MB", "GB", "TB"):
        if abs(num_bytes) < 1024.0:
            return f"{num_bytes:.2f}{unit}"
        num_bytes /= 1024.0
    return f"{num_bytes:.2f}PB"


def scan_source(root: str, description: str, name: str, *, exclude_executables: bool, smoke: int | None) -> Subtree:
    """列出来源目录下的文件（保留相对路径）。"""
    tree = Subtree(name=name, source=root, description=description)
    if not os.path.isdir(root):
        return tree
    for current, _dirs, files in os.walk(root):
        for filename in sorted(files):
            absolute = os.path.join(current, filename)
            suffix = os.path.splitext(filename)[1].lower()
            if exclude_executables and suffix in EXCLUDE_SUFFIXES:
                continue
            relative = os.path.relpath(absolute, root).replace("\\", "/")
            try:
                size = os.path.getsize(absolute)
            except OSError:
                continue
            tree.files.append((relative, absolute, size))
    tree.files.sort(key=lambda item: item[0])
    if smoke is not None:
        tree.files = tree.files[:smoke]
    return tree


def scan_single_file(path: str, name: str, description: str) -> Subtree:
    """单文件来源（Server.MirDB）。"""
    tree = Subtree(name=name, source=os.path.dirname(path), description=description)
    if os.path.isfile(path):
        tree.files.append((os.path.basename(path), path, os.path.getsize(path)))
    return tree


def sha256_of(path: str, chunk: int = 1 << 20) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        while True:
            block = handle.read(chunk)
            if not block:
                break
            digest.update(block)
    return digest.hexdigest()


def load_manifest(target: str) -> dict | None:
    path = os.path.join(target, MANIFEST_NAME)
    if not os.path.isfile(path):
        return None
    try:
        with open(path, encoding="utf-8") as handle:
            return json.load(handle)
    except (OSError, json.JSONDecodeError):
        return None


def manifest_summary(manifest: dict | None) -> dict[str, tuple[int, int]]:
    """{子树: (文件数, 字节数)}"""
    if not manifest:
        return {}
    return {
        name: (int(info.get("files", 0)), int(info.get("bytes", 0)))
        for name, info in (manifest.get("subtrees") or {}).items()
    }


def plan_diff(old: dict[str, tuple[int, int]], new: dict[str, tuple[int, int]]) -> list[str]:
    """打印用：来源清单与旧 manifest 的差异。"""
    lines = []
    for name in sorted(set(old) | set(new)):
        old_files, old_bytes = old.get(name, (0, 0))
        new_files, new_bytes = new.get(name, (0, 0))
        if (old_files, old_bytes) == (new_files, new_bytes):
            continue
        lines.append(
            f"  {name:<8} {old_files} 文件/{human(old_bytes)} → {new_files} 文件/{human(new_bytes)}"
        )
    return lines


def copy_tree(tree: Subtree, target_root: str, *, dry_run: bool, progress_every: int = 500) -> int:
    """按 size+mtime 增量复制，返回复制的文件数。"""
    copied = 0
    started = time.time()
    destination_root = os.path.join(target_root, tree.name)
    for index, (relative, absolute, size) in enumerate(tree.files, start=1):
        destination = os.path.join(destination_root, relative)
        need_copy = True
        if os.path.isfile(destination):
            try:
                same_size = os.path.getsize(destination) == size
                same_mtime = int(os.path.getmtime(destination)) == int(os.path.getmtime(absolute))
                need_copy = not (same_size and same_mtime)
            except OSError:
                need_copy = True
        if not need_copy:
            continue
        if dry_run:
            copied += 1
            continue
        os.makedirs(os.path.dirname(destination), exist_ok=True)
        shutil.copy2(absolute, destination)
        copied += 1
        if progress_every and index % progress_every == 0:
            elapsed = time.time() - started
            rate = index / elapsed if elapsed > 0 else 0
            print(
                f"  [{tree.name}] {index}/{len(tree.files)} 文件 "
                f"（{rate:.0f} 文件/秒，已复制 {copied}）",
                flush=True,
            )
    return copied


def prune_extra(tree: Subtree, target_root: str) -> list[str]:
    """删掉目标里来源没有的文件（幂等重建的一部分）。"""
    destination_root = os.path.join(target_root, tree.name)
    if not os.path.isdir(destination_root):
        return []
    expected = {os.path.normcase(os.path.join(destination_root, rel)) for rel, _, _ in tree.files}
    removed = []
    for current, _dirs, files in os.walk(destination_root):
        for filename in files:
            absolute = os.path.join(current, filename)
            if os.path.normcase(absolute) not in expected:
                removed.append(os.path.relpath(absolute, target_root).replace("\\", "/"))
                try:
                    os.remove(absolute)
                except OSError:
                    continue
    return removed


def build(args: argparse.Namespace) -> int:
    repository_root = os.path.abspath(os.getcwd())
    source_root = os.path.abspath(args.source_root)
    target_root = os.path.abspath(args.target)
    cjlaaa_root = os.path.abspath(args.cjlaaa_root)

    if not os.path.isdir(source_root):
        print(f"[错误] --source-root 不存在：{source_root}", file=sys.stderr)
        return 2

    subtitle = "（smoke）" if args.smoke else ""
    print(f"== 资源抽取 {subtitle}==")
    print(f"  仓库根    {repository_root}")
    print(f"  来源根    {os.path.relpath(source_root, repository_root)}")
    print(f"  目标      {os.path.relpath(target_root, repository_root)}")
    print(f"  地图来源  {args.map_source}")
    print(f"  可执行体  {'不复制' if not args.keep_executables else '复制'}", flush=True)

    subtrees: list[Subtree] = []
    subtrees.append(
        scan_source(
            os.path.join(source_root, "data"),
            "客户端图库/UI/minimap/books",
            "data",
            exclude_executables=not args.keep_executables,
            smoke=args.smoke,
        )
    )
    map_source = (
        os.path.join(cjlaaa_root, "Maps")
        if args.map_source == "cjlaaa"
        else os.path.join(source_root, "map")
    )
    subtrees.append(
        scan_source(
            map_source,
            f"地图（{args.map_source}）",
            "map",
            exclude_executables=not args.keep_executables,
            smoke=args.smoke,
        )
    )
    for name, description in (
        ("wav", "音效"),
        ("fonts", "字体"),
        ("ld", "图库索引"),
    ):
        subtrees.append(
            scan_source(
                os.path.join(source_root, name),
                description,
                name,
                exclude_executables=not args.keep_executables,
                smoke=args.smoke,
            )
        )

    # server/ = Server.MirDB + Envir/
    server_files: list[tuple[str, str, int]] = []
    mirror = os.path.join(cjlaaa_root, "Server.MirDB")
    if os.path.isfile(mirror):
        server_files.append(("Server.MirDB", mirror, os.path.getsize(mirror)))
    envir = scan_source(
        os.path.join(cjlaaa_root, "Envir"),
        "服务端脚本/数据",
        "server/Envir",
        exclude_executables=not args.keep_executables,
        smoke=args.smoke,
    )
    for relative, absolute, size in envir.files:
        server_files.append((f"Envir/{relative}", absolute, size))
    server_tree = Subtree(
        name="server",
        source=cjlaaa_root,
        description="服务端 Server.MirDB + Envir",
    )
    server_tree.files = sorted(server_files, key=lambda item: item[0])
    subtrees.append(server_tree)

    for tree in subtrees:
        state = "缺失" if not tree.files else f"{len(tree.files)} 文件 / {human(tree.total_bytes)}"
        print(f"  {tree.name:<8} {state:<28} ← {os.path.relpath(tree.source, repository_root)}")

    manifest = load_manifest(target_root)
    previous = manifest_summary(manifest)
    current = {tree.name: (len(tree.files), tree.total_bytes) for tree in subtrees}

    if args.verify:
        return verify(target_root, manifest, subtrees)

    differences = plan_diff(previous, current)
    if manifest and not differences and not args.force:
        print("\n[幂等] manifest 与来源一致 → 跳过复制（确实要重拷请加 --force）")
        return 0
    if manifest is None:
        print("\n[首次] 目标目录没有 manifest → 全量抽取")
    elif differences:
        print("\n[差异] 来源与现有 manifest 不一致：")
        for line in differences:
            print(line)

    if args.dry_run:
        print("\n[dry-run] 不写文件。")
        for tree in subtrees:
            print(f"  {tree.name:<8} 将复制 {len(tree.files)} 文件 / {human(tree.total_bytes)}")
        return 0

    os.makedirs(target_root, exist_ok=True)
    total_copied = 0
    for tree in subtrees:
        print(f"\n== 复制 {tree.name}（{len(tree.files)} 文件 / {human(tree.total_bytes)}）==", flush=True)
        started = time.time()
        copied = copy_tree(tree, target_root, dry_run=False)
        total_copied += copied
        removed = []
        if args.prune and not args.prune_skip:
            removed = prune_extra(tree, target_root)
        print(
            f"  完成：复制 {copied}，删除 {len(removed)}，耗时 {time.time() - started:.1f}s",
            flush=True,
        )

    # manifest：子树叶子的抽样 sha256（大文件只算一次，取样 ≤ 8 个/子树）
    subtrees_manifest: dict[str, dict] = {}
    for tree in subtrees:
        sample_hashes = {}
        sample = tree.files[:8]
        for relative, absolute, _size in sample:
            try:
                if os.path.getsize(absolute) <= 64 * 1024 * 1024:
                    sample_hashes[relative] = sha256_of(absolute)
            except OSError:
                continue
        subtrees_manifest[tree.name] = {
            "source": os.path.relpath(tree.source, repository_root).replace("\\", "/"),
            "description": tree.description,
            "files": len(tree.files),
            "bytes": tree.total_bytes,
            "sample_sha256": sample_hashes,
        }

    manifest = {
        "version": MANIFEST_VERSION,
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "smoke": bool(args.smoke),
        "smoke_limit": args.smoke,
        "map_source": args.map_source,
        "source_root": os.path.relpath(source_root, repository_root).replace("\\", "/"),
        "target_root": os.path.relpath(target_root, repository_root).replace("\\", "/"),
        "subtrees": subtrees_manifest,
        "totals": {
            "files": sum(len(tree.files) for tree in subtrees),
            "bytes": sum(tree.total_bytes for tree in subtrees),
        },
        "prune": {"enabled": bool(args.prune and not args.prune_skip), "note": prune_note()},
    }
    body = json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True)
    manifest["manifest_sha256"] = hashlib.sha256(body.encode("utf-8")).hexdigest()
    manifest_path = os.path.join(target_root, MANIFEST_NAME)
    with open(manifest_path, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True))
    print(
        f"\n== 完成 ==  总复制 {total_copied} 文件，清单 {os.path.relpath(manifest_path, repository_root)}"
        f"（{manifest['totals']['files']} 文件 / {human(manifest['totals']['bytes'])}）"
    )
    return 0


def prune_note() -> str:
    return (
        "TODO(第 25 轮)：data/ 21GB 目前是整份复制；后续按 `resource/data` 的图库引用（"
        "客户端 LibraryFile/Prguse 等实际打开的 .wzl/.wzx）生成白名单，再 `--prune` 只留被引用文件。"
    )


def verify(target_root: str, manifest: dict | None, subtrees: list[Subtree]) -> int:
    """只读校验：目标 vs manifest vs 来源。"""
    print("\n== 校验 ==")
    if not manifest:
        print(f"[失败] {os.path.join(target_root, MANIFEST_NAME)} 不存在（先跑一次抽取）", file=sys.stderr)
        return 1
    recorded = manifest.get("subtrees") or {}
    problems: list[str] = []
    for tree in subtrees:
        info = recorded.get(tree.name)
        if info is None:
            problems.append(f"{tree.name}: manifest 里没有这个子树")
            continue
        on_disk_files = 0
        on_disk_bytes = 0
        destination_root = os.path.join(target_root, tree.name)
        for current_dir, _dirs, files in os.walk(destination_root):
            for filename in files:
                absolute = os.path.join(current_dir, filename)
                on_disk_files += 1
                try:
                    on_disk_bytes += os.path.getsize(absolute)
                except OSError:
                    pass
        if on_disk_files != int(info["files"]) or on_disk_bytes != int(info["bytes"]):
            problems.append(
                f"{tree.name}: 目标 {on_disk_files} 文件/{on_disk_bytes} 字节（{human(on_disk_bytes)}） "
                f"≠ manifest {info['files']} 文件/{int(info['bytes'])} 字节（{human(int(info['bytes']))}）"
                f" 差 {on_disk_bytes - int(info['bytes']):+d} 字节"
            )
            continue
        # 抽样 sha256：抽查 manifest 里记过的样本，确认字节内容未变
        for relative, expected_hash in (info.get("sample_sha256") or {}).items():
            candidate = os.path.join(destination_root, relative)
            if not os.path.isfile(candidate):
                problems.append(f"{tree.name}/{relative}: 抽样文件缺失")
                continue
            actual = sha256_of(candidate)
            if actual != expected_hash:
                problems.append(f"{tree.name}/{relative}: sha256 不匹配（{actual[:12]} ≠ {expected_hash[:12]}）")
        source_state = f"来源现为 {len(tree.files)} 文件/{human(tree.total_bytes)}"
        status = "OK" if not problems else "见下"
        print(f"  {tree.name:<8} {info['files']:>6} 文件 / {human(int(info['bytes'])):>10}  {status}（{source_state}）")
    if problems:
        print("\n[失败] 发现不一致：", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    print(f"\n[通过] manifest 与目标一致（生成于 {manifest.get('generated_at')}，"
          f"smoke={manifest.get('smoke')}，manifest_sha256={manifest.get('manifest_sha256', '')[:16]}…）")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="把游戏用到的素材抽取到独立资源目录 game-assets/")
    parser.add_argument("--source-root", default="resource", help="素材来源根（默认 resource）")
    parser.add_argument("--target", default="game-assets", help="目标目录（默认 game-assets）")
    parser.add_argument(
        "--cjlaaa-root",
        default=DEFAULT_CJCLAA_DIR,
        help=f"cjlaaa 数据集目录（默认 {DEFAULT_CJCLAA_DIR}）",
    )
    parser.add_argument(
        "--map-source",
        choices=("cjlaaa", "resource"),
        default="cjlaaa",
        help="地图来源（默认 cjlaaa：Server.MirDB 引用的地图）",
    )
    parser.add_argument("--prune", action="store_true", help="删除目标里来源没有的文件（默认关）")
    parser.add_argument("--prune-skip", action="store_true", help="显式跳过裁剪（smoke 时用）")
    parser.add_argument("--verify", action="store_true", help="只校验目标与 manifest，不复制")
    parser.add_argument("--smoke", type=int, default=None, metavar="N", help="每子树只取前 N 个文件（冒烟）")
    parser.add_argument("--dry-run", action="store_true", help="只打印计划，不复制")
    parser.add_argument("--force", action="store_true", help="manifest 一致也强制重拷")
    parser.add_argument("--keep-executables", action="store_true", help="连可执行体一起复制（默认跳过）")
    args = parser.parse_args(argv)
    return build(args)


if __name__ == "__main__":
    sys.exit(main())
