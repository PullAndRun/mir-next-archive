#!/usr/bin/env python3
"""核对 `docs/i18n.md` §7.2 的表格与 `TextKey` 实现、两份语言包是否一一对应。

为什么需要它：第 3 轮把服务端玩家可见文本收口到 `mir_core::i18n`，§7.2 是唯一输入清单。
收口是否真实、有没有「漏键 / 孤儿键 / 中英模板漂移」，靠人工逐条比对既慢又不可复跑。
这个脚本把「文档表格 → Rust 实现 → 语言包」三方对起来，给刑部一条 `exit 1` 就能拦住的门禁。

核对口径（只读，不改任何源码）：
* 解析 `docs/i18n.md` §7.2 的每一行：文件:行、场景、原文、建议 TextKey、参数占位；
* 解析 `crates/mir-core/src/i18n.rs`：枚举变体、`TextKey::ALL`、`key()`、`args()`、
  `text_zh()`、`text_en()` 六处必须与 §7.2 闭合（键名 → CamelCase 变体）；
* 解析 `config/lang/zh-cn.json` / `en.json` 的 `server` 段，必须与内置模板逐字一致；
* §7.2 的「原文」把旧占位（`{}` / `{0}` / `{CONST}`）按「参数占位」列改成 `{snake_case}` 后，
  必须与 zh 模板**逐字相同**；英文模板的占位符集合必须与 `args()` 相同；
* 第 1 轮的 11 个无参键不在 §7.2 内，单列核对；另有 4 个 `guild_not_allowed_*`
  扩展键（行会权限的具体提示，替换会泄漏给玩家的内部标识符）以及后续扩展键（当前共 32 个），
  走白名单单列核对，不进 §7.2；除这些键之外的键都必须是 §7.2 的键（无孤儿键、无漏登记）。

用法::

    py tools/i18n/verify_text_keys.py            # 只校验 + 统计（CI 门禁：0=通过 1=有差异）
    py tools/i18n/verify_text_keys.py --table    # 额外打印「§7.2 行 → 键名 → 变体 → zh → en」表
    py tools/i18n/verify_text_keys.py --report tools/i18n/textkeys-report.md  # 生成对照表文件

退出码：0 = 全部一致；1 = 有差异（差异逐条打印）；2 = 解析失败（文档/Rust 结构变了）。
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DOC = ROOT / "docs" / "i18n.md"
I18N_RS = ROOT / "crates" / "mir-core" / "src" / "i18n.rs"
ZH_JSON = ROOT / "config" / "lang" / "zh-cn.json"
EN_JSON = ROOT / "config" / "lang" / "en.json"

#: 第 1 轮验收过的 11 个无参键（不在 §7.2 里，文案冻结）。
LEGACY_KEYS = (
    "welcome",
    "account_banned",
    "wrong_password",
    "too_many_characters",
    "language_changed",
    "unknown_language",
    "server_full",
    "unsupported_command",
    "character_name_taken",
    "character_name_invalid",
    "login_success",
)

#: 第 2 轮在 §7.2 之外追加的扩展键：把 `GuildError::Permission` 里会泄漏给玩家的
#: 内部标识符（`CanRecruit` 等）换成具体提示（Crystal `ServerTextKeys.NotAllowed*`）。
#: 口径：§7.2 是**冻结的扫描清单**（192 行 / 185 键），这些扩展键不进 §7.2 表格，
#: 由下面的白名单单独核对（对照表里也单列一节）；§7.2 的 `guild_no_permission`
#: 仍保留，作未知权限兜底。
EXTENSION_KEYS = (
    "guild_not_allowed_recruit",
    "guild_not_allowed_kick",
    "guild_not_allowed_change_rank",
    "guild_not_allowed_change_notice",
    "quest_item_found",
    # 第 26 轮：TP-1 任务导航行。
    "quest_navigation_go_to",
    "quest_navigation_none",
    # 第 24 轮：C.AcceptQuest 失败反馈（Crystal AcceptQuest 的 ReceiveChat）。
    "quest_accept_failed",
    "quest_too_many",
    # 第 25 轮：新手链路径上的拾取/购买/交付失败反馈。
    "quest_finish_failed",
    "pickup_failed",
    "buy_failed",
    "ally_usage",
    "ally_guild_not_found",
    "ally_requested",
    "ally_accepted",
    "ally_removed",
    "ally_failed",
    "newbie_guide",
    "spouse_dead",
    "spouse_not_married",
    "spouse_offline",
    "spouse_recall_needs_ring",
    "spouse_teleported",
    "lover_recall_allowed",
    "lover_recall_blocked",
    "spouse_recall_denied",
    "spouse_recall_partner_ring",
    "reboot_countdown",
    "spouse_ring_given",
    # 第 51 轮 R51-B2：ShoulderDash 一格未走时的系统聊天（Crystal
    # `HumanObject.cs:5140-5143` 的 NoPushingInSafezone / NotEnoughPushingPower）。
    "no_pushing_in_safezone",
    "not_enough_pushing_power",
)

#: 允许带参数的扩展键（Crystal `ServerTextKeys.YouFound` 是 `You found {0}`）。
EXTENSION_KEYS_WITH_ARGS = (
    "quest_item_found",
    "quest_navigation_go_to",
    "spouse_teleported",
    "reboot_countdown",
)

#: §7.2 预期的行数（第 2 轮扫描口径，冻结；文档改口径时同步改这里）。
EXPECTED_ROWS = 207

#: §7.2 唯一的键数（192 行里有 7 行是重复键）。
EXPECTED_UNIQUE_KEYS = 200

PLACEHOLDER_RE = re.compile(r"\{([^{}]*)\}")
SNAKE_RE = re.compile(r"^[a-z][a-z0-9_]*$")
BACKTICKED_SNAKE_RE = re.compile(r"`([a-z][a-z0-9_]*)`")
LOCATION_RE = re.compile(r"([a-z_]+\.rs):(\d+)")
VARIANT_RE = re.compile(r"^ {4}([A-Z][A-Za-z0-9]*),$")
ALL_ENTRY_RE = re.compile(r"Self::([A-Z][A-Za-z0-9]*),")
KEY_ARM_RE = re.compile(r'Self::([A-Z][A-Za-z0-9]*) => "([a-z][a-z0-9_]*)",')
ARGS_ARM_RE = re.compile(r"Self::([A-Z][A-Za-z0-9]*) => &\[([^\]]*)\],")
TEMPLATE_ARM_RE = re.compile(
    r'TextKey::([A-Z][A-Za-z0-9]*)\s*=>\s*(?:\{\s*)?"((?:\\.|[^"\\])*)"'
)


class ParseError(RuntimeError):
    """文档或 Rust 源码结构与预期不符。"""


@dataclass
class DocRow:
    """§7.2 表格的一行。"""

    location: str  # 例如 `conquest.rs:334`
    scene: str
    original: str
    key: str
    params: list[str]
    template: str = ""  # 原文按参数列改写后的命名占位模板


@dataclass
class RustTables:
    """`i18n.rs` 里与键相关的六张表。"""

    variants: list[str] = field(default_factory=list)
    all_entries: list[str] = field(default_factory=list)
    key_names: dict[str, str] = field(default_factory=dict)
    args: dict[str, list[str]] = field(default_factory=dict)
    zh: dict[str, str] = field(default_factory=dict)
    en: dict[str, str] = field(default_factory=dict)


def camel_case(key: str) -> str:
    """`guild_not_in_world` → `GuildNotInWorld`（与 `TextKey` 变体命名规则一致）。"""
    return "".join(part.capitalize() for part in key.split("_"))


def decode_rust_literal(literal: str) -> str:
    """解码 Rust 字符串字面量里的转义（够用即可：n/r/t/引号/反斜杠）。"""
    escapes = {"n": "\n", "r": "\r", "t": "\t", '"': '"', "\\": "\\", "'": "'", "0": "\0"}
    out: list[str] = []
    index = 0
    while index < len(literal):
        character = literal[index]
        if character == "\\" and index + 1 < len(literal):
            out.append(escapes.get(literal[index + 1], literal[index + 1]))
            index += 2
        else:
            out.append(character)
            index += 1
    return "".join(out)


def normalize_original(original: str, params: list[str]) -> str:
    """把 §7.2「原文」里的旧占位（`{}`/`{0}`/`{CONST}`/旧英文名）改成 `{参数名}`。"""
    text = original.strip()
    if text.startswith("`") and text.endswith("`"):
        text = text[1:-1]
    # 文档里写的是转义形式 `\r\n`，模板里是真实的 CRLF。
    text = text.replace("\\r\\n", "\r\n")

    remaining = list(params)
    used: set[str] = set()

    def replace(match: re.Match[str]) -> str:
        name = match.group(1)
        if name in params and name not in used:
            used.add(name)
            remaining.remove(name)
            return "{" + name + "}"
        if not remaining:
            raise ValueError(f"占位符 {match.group(0)!r} 没有可用的参数名（参数列：{params}）")
        chosen = remaining.pop(0)
        used.add(chosen)
        return "{" + chosen + "}"

    template = PLACEHOLDER_RE.sub(replace, text)
    leftovers = PLACEHOLDER_RE.findall(template)
    if sorted(leftovers) != sorted(params):
        raise ValueError(f"模板占位 {leftovers} 与参数列 {params} 不一致：{template!r}")
    return template


def placeholder_sequence(template: str) -> list[str]:
    """模板里的命名占位符（按出现顺序；`{{`/`}}` 不算）。"""
    found: list[str] = []
    rest = template
    while "{" in rest:
        open_index = rest.find("{")
        if rest[open_index:].startswith("{{"):
            rest = rest[open_index + 2:]
            continue
        after = rest[open_index + 1:]
        close = after.find("}")
        if close < 0:
            break
        name = after[:close]
        if name and "{" not in name:
            found.append(name)
        rest = after[close + 1:]
    return found


def parse_doc() -> list[DocRow]:
    """解析 §7.2 的表格（容忍礼部后续加勾选列，只要「原文/键名/参数」三列仍在原位）。"""
    lines = DOC.read_text(encoding="utf-8").splitlines()
    try:
        start = next(i for i, line in enumerate(lines) if line.startswith("### 7.2"))
        end = next(i for i, line in enumerate(lines) if line.startswith("### 7.3"))
    except StopIteration as error:
        raise ParseError("docs/i18n.md 里找不到 §7.2 或 §7.3 的标题") from error

    rows: list[DocRow] = []
    for line in lines[start:end]:
        stripped = line.strip()
        if not stripped.startswith("| `") or stripped.count("|") < 5:
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        key_index = next(
            (
                index
                for index, cell in enumerate(cells)
                if (match := BACKTICKED_SNAKE_RE.fullmatch(cell)) is not None
            ),
            None,
        )
        if key_index is None or key_index < 3:
            continue
        key = BACKTICKED_SNAKE_RE.fullmatch(cells[key_index]).group(1)  # type: ignore[union-attr]
        location_cell = cells[key_index - 3]
        location_match = LOCATION_RE.search(location_cell)
        if location_match is None:
            raise ParseError(f"§7.2 行缺少「文件:行」：{stripped}")
        params_cell = cells[key_index + 1] if key_index + 1 < len(cells) else "—"
        params = [
            name.strip().strip("{}").strip()
            for name in re.findall(r"\{([^{}]*)\}", params_cell)
            if name.strip()
        ]
        original_cell = cells[key_index - 1]
        row = DocRow(
            location=f"{location_match.group(1)}:{location_match.group(2)}",
            scene=cells[key_index - 2].strip("`"),
            original=original_cell,
            key=key,
            params=params,
        )
        row.template = normalize_original(row.original, params)
        rows.append(row)
    if not rows:
        raise ParseError("§7.2 里一行表格都没解析到")
    return rows


def parse_rust() -> RustTables:
    """解析 `i18n.rs` 的枚举、ALL、key()、args()、text_zh/text_en。"""
    source = I18N_RS.read_text(encoding="utf-8")
    tables = RustTables()

    enum_start = source.find("pub enum TextKey {")
    if enum_start < 0:
        raise ParseError("i18n.rs 里找不到 `pub enum TextKey {`")
    enum_source = source[enum_start:]
    enum_end = enum_source.find("\n}")
    if enum_end < 0:
        raise ParseError("TextKey 枚举没有闭合")
    enum_body = enum_source[:enum_end]
    tables.variants = [
        match.group(1)
        for line in enum_body.splitlines()
        if (match := VARIANT_RE.match(line.rstrip())) is not None
    ]

    all_start = source.find("pub const ALL: &'static [Self] = &[", enum_start)
    if all_start < 0:
        raise ParseError("i18n.rs 里找不到 `TextKey::ALL`")
    all_end = source.find("];", all_start)
    if all_end < 0:
        raise ParseError("TextKey::ALL 没有闭合")
    all_body = source[all_start:all_end]
    tables.all_entries = ALL_ENTRY_RE.findall(all_body)

    key_start = source.find("pub const fn key(self)")
    args_start = source.find("pub const fn args(self)")
    if key_start < 0 or args_start < 0:
        raise ParseError("i18n.rs 里找不到 `TextKey::key()` / `TextKey::args()`")
    key_body = source[key_start : source.find("\n    }", key_start)]
    args_body = source[args_start : source.find("\n    }", args_start)]
    tables.key_names = dict(KEY_ARM_RE.findall(key_body))
    for variant, raw_args in ARGS_ARM_RE.findall(args_body):
        names = re.findall(r'"([^"]*)"', raw_args)
        tables.args[variant] = names

    zh_start = source.find("const fn text_zh")
    en_start = source.find("const fn text_en")
    if zh_start < 0 or en_start < 0 or en_start <= zh_start:
        raise ParseError("i18n.rs 里找不到 `text_zh` / `text_en`")
    zh_body = source[zh_start:en_start]
    en_end = source.find("pub enum LanguagePackError", en_start)
    en_body = source[en_start : en_end if en_end > en_start else len(source)]
    for variant, literal in TEMPLATE_ARM_RE.findall(zh_body):
        tables.zh[variant] = decode_rust_literal(literal)
    for variant, literal in TEMPLATE_ARM_RE.findall(en_body):
        tables.en[variant] = decode_rust_literal(literal)
    if not tables.zh or not tables.en:
        raise ParseError("text_zh/text_en 里一条模板都没解析到")
    return tables


def load_pack(path: Path) -> dict[str, str]:
    """读语言包的 `server` 段（模板文本）。"""
    pack = json.loads(path.read_text(encoding="utf-8"))
    server = pack.get("server")
    if not isinstance(server, dict):
        raise ParseError(f"{path.name} 缺少 server 段")
    return {str(name): str(value) for name, value in server.items()}


def escape_cell(text: str) -> str:
    """markdown 表格单元：竖线转义、换行转成可见的 `\\n`。"""
    return (
        text.replace("|", "\\|")
        .replace("\r", "\\r")
        .replace("\n", "\\n")
    )


def build_markdown(
    rows: list[DocRow],
    tables: RustTables,
    zh_pack: dict[str, str],
    en_pack: dict[str, str],
    duplicates: dict[str, list[str]],
    passed: bool,
) -> str:
    """生成「§7.2 行 → 键名 → 变体 → zh 模板 → en 模板」对照表。"""
    parameterized = sum(1 for key in tables.key_names.values() if tables.args[camel_case(key)])
    lines = [
        "# `TextKey` 对照表（§7.2 → 键名 → 变体 → 中英模板）",
        "",
        "> 本文件由 `py tools/i18n/verify_text_keys.py --report tools/i18n/textkeys-report.md` 生成，请勿手改。",
        "> 核对对象：`docs/i18n.md` §7.2（收口清单）、`crates/mir-core/src/i18n.rs`、`config/lang/{zh-cn,en}.json`。",
        "",
        "## 统计",
        "",
        "| 项 | 数量 |",
        "| --- | --- |",
        f"| §7.2 表格行 | {len(rows)} |",
        f"| §7.2 唯一键 | {len({row.key for row in rows})}（{len(rows) - len({row.key for row in rows})} 行是重复出现的键） |",
        f"| 第 1 轮无参键 | {len(LEGACY_KEYS)} |",
        f"| 扩展键（不在 §7.2 内） | {len(EXTENSION_KEYS)} |",
        f"| `TextKey` 变体 / `ALL` | {len(tables.variants)} / {len(tables.all_entries)} |",
        f"| 带参数的键 | {parameterized} |",
        f"| 语言包 server 键（zh / en） | {len(zh_pack)} / {len(en_pack)} |",
        "",
        (
            f"核对结果：**PASS**（§7.2 {len(rows)} 行 → {len({row.key for row in rows})} 键；"
            f"加 {len(LEGACY_KEYS)} 个第 1 轮键与 {len(EXTENSION_KEYS)} 个扩展键共 "
            f"{len(tables.variants)} 个 `TextKey`；无漏键、无孤儿键、中英占位一致）。"
            if passed
            else "核对结果：**FAIL**（发现不一致，详见 `py tools/i18n/verify_text_keys.py` 的 stderr 输出）。"
        ),
        "",
        f"## 重复出现的键（{len(rows)} 行 → {len({row.key for row in rows})} 键）",
        "",
        "同一键名在 §7.2 里出现多次（同一句提示在多个分支复用），实现里合并为一个变体。",
        "",
        "| 键名 | 出现次数 | §7.2 行 |",
        "| --- | --- | --- |",
    ]
    for key, locations in duplicates.items():
        lines.append(f"| `{key}` | {len(locations)} | {'、'.join(f'`{item}`' for item in locations)} |")

    lines += [
        "",
        f"## §7.2 逐行对照（{len(rows)} 行）",
        "",
        "| §7.2 行 | 键名 | 变体 | zh 模板 | en 模板 |",
        "| --- | --- | --- | --- | --- |",
    ]
    for row in rows:
        variant = camel_case(row.key)
        lines.append(
            f"| `{row.location}` | `{row.key}` | `{variant}` | `{escape_cell(tables.zh[variant])}` "
            f"| `{escape_cell(tables.en[variant])}` |"
        )

    lines += [
        "",
        f"## 扩展键（{len(EXTENSION_KEYS)} 个，不属于 §7.2）",
        "",
        "用于替换 `GuildError::Permission` 会泄漏给玩家的内部标识符（Crystal `ServerTextKeys.NotAllowed*`）；"
        "§7.2 的 `guild_no_permission` 仍保留，作未知权限兜底。",
        "",
        "| 键名 | 变体 | zh 模板 | en 模板 |",
        "| --- | --- | --- | --- |",
    ]
    for key in EXTENSION_KEYS:
        variant = camel_case(key)
        lines.append(
            f"| `{key}` | `{variant}` | `{escape_cell(tables.zh[variant])}` "
            f"| `{escape_cell(tables.en[variant])}` |"
        )

    lines += [
        "",
        "## 第 1 轮 11 个无参键（不在 §7.2 内，文案冻结）",
        "",
        "| 键名 | 变体 | zh 模板 | en 模板 |",
        "| --- | --- | --- | --- |",
    ]
    for key in LEGACY_KEYS:
        variant = camel_case(key)
        lines.append(
            f"| `{key}` | `{variant}` | `{escape_cell(tables.zh[variant])}` "
            f"| `{escape_cell(tables.en[variant])}` |"
        )
    lines.append("")
    return "\n".join(lines)


def verify() -> tuple[list[str], list[DocRow], RustTables, dict[str, str], dict[str, str]]:
    """跑全部核对，返回 (差异列表, 行, Rust 表, zh 包, en 包)。"""
    rows = parse_doc()
    tables = parse_rust()
    zh_pack = load_pack(ZH_JSON)
    en_pack = load_pack(EN_JSON)
    problems: list[str] = []

    def check(condition: bool, message: str) -> None:
        if not condition:
            problems.append(message)

    # ---- 1) 文档行数与重复键 ----
    check(len(rows) == EXPECTED_ROWS, f"§7.2 行数应为 {EXPECTED_ROWS}，实际 {len(rows)}")
    unique_keys = {row.key for row in rows}
    check(
        len(unique_keys) == EXPECTED_UNIQUE_KEYS,
        f"§7.2 唯一键应为 {EXPECTED_UNIQUE_KEYS}，实际 {len(unique_keys)}",
    )

    by_key: dict[str, list[DocRow]] = {}
    for row in rows:
        by_key.setdefault(row.key, []).append(row)
    for key, group in by_key.items():
        if len(group) > 1:
            templates = {row.template for row in group}
            check(len(templates) == 1, f"重复键 {key} 的各行原文不一致：{templates}")
            params = {tuple(row.params) for row in group}
            check(len(params) == 1, f"重复键 {key} 的各行参数不一致：{params}")

    # ---- 2) 六个集合闭合：§7.2 键 / 枚举 / ALL / key() / 模板 / 语言包 ----
    doc_keys = set(by_key)
    doc_variants = {camel_case(key) for key in doc_keys}
    legacy_variants = {camel_case(key) for key in LEGACY_KEYS}
    all_variants = set(tables.variants)
    key_by_variant = tables.key_names
    variant_by_key = {key: variant for variant, key in key_by_variant.items()}

    check(
        len(tables.variants) == len(all_variants),
        "TextKey 枚举里有重复变体",
    )
    check(
        set(tables.all_entries) == all_variants and len(tables.all_entries) == len(all_variants),
        "TextKey::ALL 与枚举变体不是一一对应（有漏登记或重复）",
    )
    check(
        set(key_by_variant) == all_variants,
        "key() 的变体集合与枚举不一致",
    )
    check(
        set(tables.args) == all_variants,
        "args() 的变体集合与枚举不一致",
    )
    check(set(tables.zh) == all_variants, "text_zh() 的变体集合与枚举不一致")
    check(set(tables.en) == all_variants, "text_en() 的变体集合与枚举不一致")
    check(
        set(zh_pack) == set(en_pack),
        "两份语言包的 server 键集合不一致",
    )
    check(
        set(zh_pack) == set(key_by_variant.values()),
        "语言包键集合与 TextKey::key() 不一致（有孤儿键或漏键）",
    )
    check(
        set(zh_pack) - doc_keys == set(LEGACY_KEYS) | set(EXTENSION_KEYS),
        f"除第 1 轮 {len(LEGACY_KEYS)} 个键与 {len(EXTENSION_KEYS)} 个扩展键外还有不在 §7.2 里的键："
        f"{sorted(set(zh_pack) - doc_keys - set(LEGACY_KEYS) - set(EXTENSION_KEYS))}",
    )
    check(
        doc_keys <= set(zh_pack),
        f"§7.2 的键在语言包里缺失：{sorted(doc_keys - set(zh_pack))}",
    )
    check(
        doc_variants <= all_variants,
        f"§7.2 的键在 TextKey 里缺失：{sorted(doc_variants - all_variants)}",
    )
    check(
        legacy_variants <= all_variants,
        f"第 1 轮的键在 TextKey 里缺失：{sorted(legacy_variants - all_variants)}",
    )
    for key in doc_keys:
        variant = camel_case(key)
        if key in variant_by_key and variant_by_key[key] != variant:
            problems.append(f"键名 {key} 的变体名不是 {variant}，而是 {variant_by_key[key]}")

    # ---- 3) 逐行核对：zh 模板 == 原文（旧占位改名后）；en/args 占位一致 ----
    for row in rows:
        variant = camel_case(row.key)
        if variant not in all_variants:
            continue
        zh = tables.zh.get(variant, "")
        en = tables.en.get(variant, "")
        if zh != row.template:
            problems.append(
                f"{row.location} {row.key}：zh 模板与原文不一致\n"
                f"    期望（原文）：{row.template!r}\n"
                f"    实际（zh）  ：{zh!r}"
            )
        if zh_pack.get(row.key) != zh:
            problems.append(f"{row.location} {row.key}：zh-cn.json 与内置 zh 模板不一致")
        if en_pack.get(row.key) != en:
            problems.append(f"{row.location} {row.key}：en.json 与内置 en 模板不一致")
        declared = tables.args.get(variant, [])
        zh_placeholders = placeholder_sequence(zh)
        en_placeholders = placeholder_sequence(en)
        if sorted(zh_placeholders) != sorted(declared):
            problems.append(
                f"{row.location} {row.key}：zh 模板占位 {zh_placeholders} 与 args() {declared} 不一致"
            )
        if sorted(en_placeholders) != sorted(declared):
            problems.append(
                f"{row.location} {row.key}：en 模板占位 {en_placeholders} 与 args() {declared} 不一致"
            )
        if not en:
            problems.append(f"{row.location} {row.key}：缺少英文模板")

    # ---- 4) 第 1 轮 11 个键：无参、且不属于 §7.2 ----
    for key in LEGACY_KEYS:
        variant = camel_case(key)
        if variant not in all_variants:
            continue
        if tables.args.get(variant):
            problems.append(f"第 1 轮的键 {key} 不应有参数：{tables.args[variant]}")
        if key in doc_keys:
            problems.append(f"第 1 轮的键 {key} 不该出现在 §7.2 里")
        if zh_pack.get(key) != tables.zh.get(variant):
            problems.append(f"第 1 轮的键 {key}：zh-cn.json 与内置模板不一致")
        if en_pack.get(key) != tables.en.get(variant):
            problems.append(f"第 1 轮的键 {key}：en.json 与内置模板不一致")

    # ---- 5) 扩展键（不属于 §7.2）：无参、且两份语言包与内置模板一致 ----
    for key in EXTENSION_KEYS:
        variant = camel_case(key)
        check(
            variant in all_variants,
            f"扩展键 {key} 没有对应的 TextKey 变体 {variant}",
        )
        if variant not in all_variants:
            continue
        if tables.args.get(variant) and key not in EXTENSION_KEYS_WITH_ARGS:
            problems.append(f"扩展键 {key} 不应有参数：{tables.args[variant]}")
        if key in doc_keys:
            problems.append(
                f"扩展键 {key} 不该出现在 §7.2 里（§7.2 是冻结的扫描清单，扩展键单列）"
            )
        if zh_pack.get(key) != tables.zh.get(variant):
            problems.append(f"扩展键 {key}：zh-cn.json 与内置模板不一致")
        if en_pack.get(key) != tables.en.get(variant):
            problems.append(f"扩展键 {key}：en.json 与内置模板不一致")

    return problems, rows, tables, zh_pack, en_pack


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--table", action="store_true", help="打印「§7.2 行 → 键名 → 变体 → zh → en」表")
    parser.add_argument(
        "--report",
        type=Path,
        default=None,
        metavar="PATH",
        help="把完整对照表写入 markdown 文件（UTF-8）",
    )
    args = parser.parse_args(argv)

    try:
        problems, rows, tables, zh_pack, en_pack = verify()
    except (ParseError, ValueError) as error:
        print(f"[FAIL] 解析失败：{error}", file=sys.stderr)
        return 2

    duplicates: dict[str, list[str]] = {}
    for row in rows:
        duplicates.setdefault(row.key, []).append(row.location)
    duplicates = {key: value for key, value in duplicates.items() if len(value) > 1}

    parameterized = sum(1 for key in tables.key_names.values() if tables.args[camel_case(key)])
    print(f"§7.2 行数            : {len(rows)}（预期 {EXPECTED_ROWS}）")
    print(f"§7.2 唯一键          : {len({row.key for row in rows})}")
    print(f"重复出现的键         : {len(duplicates)}（{'、'.join(duplicates)}）")
    print(f"第 1 轮无参键        : {len(LEGACY_KEYS)}")
    print(f"扩展键（不在 §7.2）  : {len(EXTENSION_KEYS)}")
    print(f"TextKey 变体 / ALL   : {len(tables.variants)} / {len(tables.all_entries)}")
    print(f"带参数的键           : {parameterized}")
    print(f"语言包 server 键     : zh={len(zh_pack)} en={len(en_pack)}")
    print(f"核对差异             : {len(problems)}")

    if args.table:
        print()
        print("| §7.2 行 | 键名 | 变体 | zh 模板 | en 模板 |")
        print("| --- | --- | --- | --- | --- |")
        for row in rows:
            variant = camel_case(row.key)
            print(
                f"| `{row.location}` | `{row.key}` | `{variant}` | "
                f"`{escape_cell(tables.zh[variant])}` | `{escape_cell(tables.en[variant])}` |"
            )

    if args.report is not None:
        markdown = build_markdown(rows, tables, zh_pack, en_pack, duplicates, not problems)
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(markdown, encoding="utf-8", newline="\n")
        print(f"对照表已写入         : {args.report.as_posix()}（{len(markdown)} 字符）")

    if problems:
        print("\n[FAIL] 发现以下不一致：", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    print(
        f"\n[OK] §7.2 {len(rows)} 行 → {len({row.key for row in rows})} 键；"
        f"加 {len(LEGACY_KEYS)} 个第 1 轮键与 {len(EXTENSION_KEYS)} 个扩展键共 "
        f"{len(tables.variants)} 个 TextKey 全部对应，无漏键、无孤儿键、中英占位一致"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))



