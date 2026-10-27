#!/usr/bin/env python3
"""Round 69 render contract auditor (read-only static source audit).

Scope: R69-A G1-G5 render contract between the Crystal reference client
(github/Crystal/Client/MirGraphics, MirObjects, MirScenes) and the Rust
port (crates/mir-render, crates/mir-client, crates/mir-asset).

The human-frozen expected matrix lives at
docs/testing/round69/render-contract.json.  This tool only produces
`observed` values and compares them field by field against that matrix.
It never writes to sources, never touches the network and writes nothing
except the optional --out JSON report.

Exit codes:
    0  every one of the five checks passes (observed == expected)
    1  at least one contract check fails (semantic drift detected)
    2  input/source error: missing source file, missing/ambiguous method,
       unreadable or structurally invalid expected matrix

Usage:
    py -3 tools/testing/round69_render_contract.py --root . --out out.json
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

TOOL_NAME = "round69_render_contract.py"
TOOL_VERSION = 1
MATRIX_REL = "docs/testing/round69/render-contract.json"

MLIBRARY = "github/Crystal/Client/MirGraphics/MLibrary.cs"
DXMANAGER = "github/Crystal/Client/MirGraphics/DXManager.cs"
EFFECT = "github/Crystal/Client/MirObjects/Effect.cs"
SPELLOBJECT = "github/Crystal/Client/MirObjects/SpellObject.cs"
GAMESCENE = "github/Crystal/Client/MirScenes/GameScene.cs"
CLIENT_TREE = "github/Crystal/Client"

REQUIRED_SOURCES = [MLIBRARY, DXMANAGER, EFFECT, SPELLOBJECT, GAMESCENE]

OPTIONAL_SOURCES = [
    "crates/mir-render/src/entities.rs",
    "crates/mir-render/src/frame.rs",
    "crates/mir-render/src/scene.rs",
    "crates/mir-client/src/ui/spellfx.rs",
    "crates/mir-client/src/ui/projectile.rs",
    "crates/mir-asset/src/image.rs",
]

# Canonical declarations located by whitespace-insensitive matching.
SIG_DRAW = "public void Draw(int index, Point point, Color colour, bool offSet = false)"
SIG_DRAW_INT = "public void Draw(int index, int x, int y)"
SIG_DRAW_OPACITY = (
    "public void Draw(int index, Point point, Color colour, bool offSet, float opacity)"
)
SIG_DRAW_SECTION_OFFSET = (
    "public void Draw(int index, Rectangle section, Point point, Color colour, bool offSet)"
)
SIG_DRAW_SECTION_OPACITY = (
    "public void Draw(int index, Rectangle section, Point point, Color colour, float opacity)"
)
SIG_DRAW_SIZE = "public void Draw(int index, Point point, Size size, Color colour)"
SIG_DRAWBLEND = (
    "public void DrawBlend(int index, Point point, Color colour,"
    " bool offSet = false, float rate = 1)"
)
SIG_DRAWTINTED = (
    "public void DrawTinted(int index, Point point, Color colour,"
    " Color Tint, bool offSet = false)"
)
SIG_SETBLEND = (
    "public static void SetBlend(bool value, float rate = 1F,"
    " BlendMode mode = BlendMode.NORMAL)"
)
SIG_MIMAGE_CTOR = "public MImage(BinaryReader reader)"
SIG_CREATETEXTURE = "public unsafe void CreateTexture(BinaryReader reader)"

OVERLOAD_SIGS = [
    SIG_DRAW_INT,
    SIG_DRAW_OPACITY,
    SIG_DRAW_SECTION_OFFSET,
    SIG_DRAW_SECTION_OPACITY,
    SIG_DRAW_SIZE,
]

IDENTIFIER_RE = re.compile(r"\b(ShadowX|ShadowY|MaskX|MaskY)\b")
MASK_METADATA_IDS = ["MaskHeight", "MaskWidth", "MaskX", "MaskY"]
SHADOW_IDS = ["ShadowX", "ShadowY"]

REQUIRED_MATRIX_KEYS = [
    "schema_version",
    "round",
    "task",
    "baseline_head",
    "freeze",
    "checks",
    "gaps",
    "r70_candidates",
]
REQUIRED_CHECK_KEYS = [
    "id",
    "title",
    "crystal_facts",
    "rust_consumption",
    "unimplemented",
    "minimal_fix",
    "fail_acceptance",
    "priority",
    "owner",
    "expected",
]
REQUIRED_GAP_KEYS = [
    "id",
    "title",
    "priority",
    "owner",
    "crystal_fact",
    "rust_consumption",
    "unimplemented",
    "minimal_fix",
    "fail_acceptance",
]
REQUIRED_CANDIDATE_KEYS = [
    "id",
    "title",
    "kind",
    "priority",
    "owner",
    "landing",
    "criteria",
    "non_goals",
]
CHECK_ORDER = ["G1", "G2", "G3", "G4", "G5"]


class AuditInputError(Exception):
    """Bad tool input: arguments, matrix file or matrix structure."""


class SourceError(Exception):
    """Required source file or method is missing/ambiguous/unreadable."""


# ---------------------------------------------------------------------------
# text helpers
# ---------------------------------------------------------------------------


def norm(value: str) -> str:
    """Collapse all whitespace runs to single spaces and strip."""
    return " ".join(value.split())


def decode_source(data: bytes) -> str:
    for encoding in ("utf-8-sig", "gbk"):
        try:
            return data.decode(encoding)
        except UnicodeDecodeError:
            continue
    return data.decode("latin-1")


def mask_code(text: str) -> str:
    """Blank comments/strings/char literals while preserving newlines.

    The masked text is used for structural work (method bodies, brace
    balance, call parsing, token scans) so pseudo-code inside comments or
    string literals can never look like a real declaration or read.
    """
    out = []
    i = 0
    n = len(text)
    while i < n:
        ch = text[i]
        nxt = text[i + 1] if i + 1 < n else ""
        if ch == "/" and nxt == "/":
            end = text.find("\n", i)
            if end < 0:
                end = n
            out.append(" " * (end - i))
            i = end
            continue
        if ch == "/" and nxt == "*":
            end = text.find("*/", i + 2)
            end = n if end < 0 else end + 2
            out.append("".join("\n" if c == "\n" else " " for c in text[i:end]))
            i = end
            continue
        if ch == "@" and nxt == '"':
            end = i + 2
            while end < n:
                if text[end] == '"':
                    if end + 1 < n and text[end + 1] == '"':
                        end += 2
                        continue
                    end += 1
                    break
                end += 1
            out.append("".join("\n" if c == "\n" else " " for c in text[i:end]))
            i = end
            continue
        if ch == '"':
            end = i + 1
            while end < n:
                c = text[end]
                if c == "\\":
                    end += 2
                    continue
                if c == '"':
                    end += 1
                    break
                if c == "\n":
                    break
                end += 1
            out.append("".join("\n" if c == "\n" else " " for c in text[i:end]))
            i = end
            continue
        if ch == "'":
            end = i + 1
            closed = False
            while end < n:
                c = text[end]
                if c == "\\":
                    end += 2
                    continue
                if c == "'":
                    end += 1
                    closed = True
                    break
                if c == "\n":
                    break
                end += 1
            if closed:
                out.append("".join("\n" if c == "\n" else " " for c in text[i:end]))
                i = end
            else:
                out.append(" ")
                i += 1
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def line_of(text: str, index: int) -> int:
    return text.count("\n", 0, index) + 1


def matching_paren(text: str, open_index: int) -> int:
    """Return index of the ')' matching text[open_index] == '('."""
    depth = 0
    i = open_index
    n = len(text)
    while i < n:
        ch = text[i]
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    raise SourceError("unbalanced parentheses while parsing source")


def matching_brace(text: str, open_index: int) -> int:
    """Return index of the '}' matching text[open_index] == '{'."""
    depth = 0
    i = open_index
    n = len(text)
    while i < n:
        ch = text[i]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    raise SourceError("unbalanced braces while parsing source")


def split_top_level(text: str) -> list[str]:
    """Split a masked argument list on top-level commas."""
    parts = []
    depth = 0
    start = 0
    for i, ch in enumerate(text):
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        elif ch == "," and depth == 0:
            parts.append(text[start:i])
            start = i + 1
    tail = text[start:]
    if tail.strip() or parts:
        parts.append(tail)
    return parts


def call_args(masked: str, open_index: int) -> tuple[list[str], int]:
    """Parse the argument list for a call whose '(' starts at open_index."""
    close = matching_paren(masked, open_index)
    inner = masked[open_index + 1 : close]
    args = [norm(a) for a in split_top_level(inner)]
    return args, close


def calls_in(masked: str, start: int = 0, end: int | None = None, names=("DrawBlend", "Draw")):
    """Yield (callee, method_name, args, start_index, end_index) for calls.

    Only simple dotted/bracketed callees are considered:
        Library.Draw( ... )
        Libraries.MapLibs[fileIndex].DrawBlend( ... )
        DXManager.SetBlend( ... )
    """
    if end is None:
        end = len(masked)
    window = masked[start:end]
    name_pattern = "|".join(re.escape(name) for name in names)
    pattern = re.compile(
        r"(?<![\w.])([A-Za-z_][\w.]*(?:\[[^\]\n]*\])?)\.(" + name_pattern + r")\s*\("
    )
    for match in pattern.finditer(window):
        callee = match.group(1)
        name = match.group(2)
        open_index = start + match.end() - 1
        try:
            args, close = call_args(masked, open_index)
        except SourceError:
            continue
        call_text = callee + "." + name + "(" + ", ".join(args) + ")"
        yield {
            "callee": callee,
            "name": name,
            "args": args,
            "call": call_text,
            "start": start + match.start(),
            "end": close,
        }


def statements_set_render_state(masked: str, start: int):
    """Return {state: value} for SetRenderState calls after `start`."""
    pattern = re.compile(r"SetRenderState\(\s*RenderState\.(BlendOperation|SourceBlend|DestinationBlend)\s*,")
    result = {}
    for match in pattern.finditer(masked, start):
        open_index = masked.index("(", match.start())
        args, _close = call_args(masked, open_index)
        if len(args) >= 2:
            result.setdefault(match.group(1), args[1])
    return result


# ---------------------------------------------------------------------------
# source context
# ---------------------------------------------------------------------------


class Context:
    def __init__(self, root: Path):
        self.root = Path(root)
        self._raw: dict[str, str] = {}
        self._masked: dict[str, str] = {}
        self._sha: dict[str, str] = {}
        self._bytes: dict[str, bytes] = {}
        self._methods: dict[tuple[str, str], dict] = {}
        self.located: list[dict] = []

    def path(self, rel: str) -> Path:
        return self.root / rel

    def exists(self, rel: str) -> bool:
        return self.path(rel).is_file()

    def read_bytes(self, rel: str) -> bytes:
        if rel not in self._bytes:
            target = self.path(rel)
            if not target.is_file():
                raise SourceError("missing source file: %s" % rel)
            self._bytes[rel] = target.read_bytes()
        return self._bytes[rel]

    def raw(self, rel: str) -> str:
        if rel not in self._raw:
            self._raw[rel] = decode_source(self.read_bytes(rel))
        return self._raw[rel]

    def masked(self, rel: str) -> str:
        if rel not in self._masked:
            self._masked[rel] = mask_code(self.raw(rel))
        return self._masked[rel]

    def sha(self, rel: str) -> str:
        if rel not in self._sha:
            self._sha[rel] = hashlib.sha256(self.read_bytes(rel)).hexdigest()
        return self._sha[rel]

    def line_count(self, rel: str) -> int:
        return self.read_bytes(rel).count(b"\n")

    def method(self, rel: str, signature: str) -> dict:
        key = (rel, signature)
        if key in self._methods:
            return self._methods[key]
        masked = self.masked(rel)
        pattern = re.compile(r"\s+".join(re.escape(tok) for tok in signature.split()))
        matches = list(pattern.finditer(masked))
        if not matches:
            raise SourceError("method not found: %s in %s" % (signature, rel))
        if len(matches) > 1:
            raise SourceError(
                "method not unique (%d matches): %s in %s"
                % (len(matches), signature, rel)
            )
        match = matches[0]
        open_brace = masked.find("{", match.end())
        if open_brace < 0:
            raise SourceError("method body not found: %s in %s" % (signature, rel))
        close_brace = matching_brace(masked, open_brace)
        info = {
            "path": rel,
            "signature": norm(masked[match.start() : match.end()]),
            "start_line": line_of(masked, match.start()),
            "end_line": line_of(masked, close_brace),
            "body": masked[open_brace + 1 : close_brace],
            "body_open": open_brace,
            "body_close": close_brace,
        }
        self._methods[key] = info
        self.located.append(
            {
                "path": rel,
                "signature": info["signature"],
                "start_line": info["start_line"],
                "end_line": info["end_line"],
            }
        )
        return info


# ---------------------------------------------------------------------------
# signature helpers
# ---------------------------------------------------------------------------


def has_off_set_param(signature: str) -> bool:
    return re.search(r"\bbool\s+offSet\b", signature) is not None


def off_set_default(signature: str):
    match = re.search(r"\bbool\s+offSet\s*=\s*(true|false)\b", signature)
    if match is None:
        return None
    return match.group(1) == "true"


def param_default(signature: str, type_name: str, param: str):
    match = re.search(
        r"\b%s\s+%s\s*=\s*([^,)]+)" % (re.escape(type_name), re.escape(param),),
        signature,
    )
    return None if match is None else norm(match.group(1))


def offset_call_count(body: str) -> int:
    return len(re.findall(r"\.Offset\s*\(", body))


def offset_guard(body: str):
    match = re.search(r"\bif\s*\(\s*([A-Za-z_]\w*)\s*\)", body)
    if match is None:
        return None, None
    condition = match.group(1)
    pos = match.end()
    while pos < len(body) and body[pos].isspace():
        pos += 1
    if pos < len(body) and body[pos] == "{":
        pos += 1
        while pos < len(body) and body[pos].isspace():
            pos += 1
    end = body.find(";", pos)
    if end < 0:
        return condition, None
    return condition, norm(body[pos:end])


def first_if_with(body: str, needle: str):
    for match in re.finditer(r"\bif\s*\(", body):
        open_index = match.end() - 1
        try:
            close = matching_paren(body, open_index)
        except SourceError:
            continue
        condition = body[open_index + 1 : close]
        if needle in condition:
            return {
                "index": match.start(),
                "condition": norm(condition),
                "parts": [norm(part) for part in condition.split("||")],
            }
    return None


def first_call(body: str, callee: str, name: str, start: int = 0):
    for call in calls_in(body, start):
        if call["callee"] == callee and call["name"] == name:
            return call
    return None


def collect_consumers(ctx: Context):
    """Library/BodyLibrary draw consumers with an explicit offSet=true arg."""
    results = []
    for rel in (EFFECT, SPELLOBJECT):
        masked = ctx.masked(rel)
        for call in calls_in(masked):
            if call["callee"] not in ("Library", "BodyLibrary"):
                continue
            args = call["args"]
            if call["name"] == "Draw":
                keep = len(args) == 4 and args[3] == "true"
            else:
                keep = len(args) == 5 and args[3] == "true"
            if not keep:
                continue
            results.append(
                {
                    "path": rel,
                    "call": call["call"],
                    "line": line_of(masked, call["start"]),
                }
            )
    results.sort(key=lambda item: (item["path"], item["call"]))
    return results


# ---------------------------------------------------------------------------
# check extractors
# ---------------------------------------------------------------------------


def extract_g1(ctx: Context):
    method = ctx.method(MLIBRARY, SIG_DRAW)
    body = method["body"]
    guard_condition, offset_stmt = offset_guard(body)
    crop = first_if_with(body, "Settings.ScreenWidth")
    draw_call = first_call(body, "DXManager", "Draw")
    integer_overload = ctx.method(MLIBRARY, SIG_DRAW_INT)

    overloads = []
    overload_methods = []
    for signature in OVERLOAD_SIGS:
        info = ctx.method(MLIBRARY, signature)
        overload_methods.append(info)
        overloads.append(
            {
                "signature": info["signature"],
                "has_off_set_param": has_off_set_param(info["signature"]),
                "default_offSet": off_set_default(info["signature"]),
                "offset_calls": offset_call_count(info["body"]),
            }
        )
    overloads.sort(key=lambda item: 0)  # keep source order via OVERLOAD_SIGS list

    consumers = collect_consumers(ctx)
    observed = {
        "method": {"path": method["path"], "signature": method["signature"]},
        "default_offSet": off_set_default(method["signature"]),
        "offset_calls": offset_call_count(body),
        "offset_guard_condition": guard_condition,
        "offset_call": offset_stmt,
        "offset_before_crop": bool(
            guard_condition is not None
            and crop is not None
            and body.find("if (offSet)") < crop["index"]
        ),
        "crop_before_draw": bool(
            crop is not None and draw_call is not None and crop["index"] < draw_call["start"]
        ),
        "crop_conditions": crop["parts"] if crop is not None else None,
        "draw_call": {
            "callee": "DXManager.Draw",
            "image": draw_call["args"][0] if draw_call and len(draw_call["args"]) >= 1 else None,
            "rect": draw_call["args"][1] if draw_call and len(draw_call["args"]) >= 2 else None,
            "position": draw_call["args"][2] if draw_call and len(draw_call["args"]) >= 3 else None,
        },
        "integer_overload": {
            "signature": integer_overload["signature"],
            "has_off_set_param": has_off_set_param(integer_overload["signature"]),
            "offset_calls": offset_call_count(integer_overload["body"]),
        },
        "overloads": overloads,
        "consumers": [{"path": c["path"], "call": c["call"]} for c in consumers if "Draw(" in c["call"]],
    }
    anchors = [
        _anchor(ctx, MLIBRARY, method, "base Draw method body"),
        _anchor(ctx, MLIBRARY, integer_overload, "integer overload (no offSet)"),
    ]
    for rel in (EFFECT, SPELLOBJECT):
        lines = [str(c["line"]) for c in consumers if c["path"] == rel and "Draw(" in c["call"]]
        if lines:
            anchors.append(
                {
                    "path": rel,
                    "lines": ",".join(lines),
                    "sha256": ctx.sha(rel),
                    "note": "Draw(...,true) consumer calls",
                }
            )
    return observed, anchors


def extract_g2(ctx: Context):
    method = ctx.method(MLIBRARY, SIG_DRAWBLEND)
    body = method["body"]
    guard_condition, offset_stmt = offset_guard(body)
    crop = first_if_with(body, "Settings.ScreenWidth")
    draw_call = first_call(body, "DXManager", "Draw")
    set_blend_calls = []
    for call in calls_in(body, names=("SetBlend",)):
        if call["callee"] == "DXManager" and call["name"] == "SetBlend":
            set_blend_calls.append(call)
    first_set_blend = set_blend_calls[0] if set_blend_calls else None
    last_set_blend = set_blend_calls[-1] if set_blend_calls else None

    consumers = collect_consumers(ctx)
    conditional = []
    masked_scene = ctx.masked(GAMESCENE)
    for call in calls_in(masked_scene):
        if call["name"] != "DrawBlend":
            continue
        if not call["callee"].startswith("Libraries.MapLibs["):
            continue
        args = call["args"]
        if len(args) < 4:
            continue
        if args[3] in ("true", "false"):
            continue
        conditional.append(
            {
                "call": call["call"],
                "off_set_arg": args[3],
                "line": line_of(masked_scene, call["start"]),
            }
        )

    observed = {
        "method": {"path": method["path"], "signature": method["signature"]},
        "default_offSet": off_set_default(method["signature"]),
        "default_rate": param_default(method["signature"], "float", "rate"),
        "offset_calls": offset_call_count(body),
        "offset_guard_condition": guard_condition,
        "offset_call": offset_stmt,
        "offset_before_crop": bool(
            guard_condition is not None
            and crop is not None
            and body.find("if (offSet)") < crop["index"]
        ),
        "crop_before_set_blend": bool(
            crop is not None and first_set_blend is not None and crop["index"] < first_set_blend["start"]
        ),
        "set_blend_before_draw": bool(
            first_set_blend is not None
            and draw_call is not None
            and first_set_blend["start"] < draw_call["start"]
        ),
        "draw_before_restore": bool(
            draw_call is not None
            and last_set_blend is not None
            and draw_call["start"] < last_set_blend["start"]
        ),
        "set_blend_args": first_set_blend["args"] if first_set_blend is not None else None,
        "restore_args": last_set_blend["args"] if last_set_blend is not None else None,
        "draw_call": {
            "callee": "DXManager.Draw",
            "image": draw_call["args"][0] if draw_call and len(draw_call["args"]) >= 1 else None,
            "rect": draw_call["args"][1] if draw_call and len(draw_call["args"]) >= 2 else None,
            "position": draw_call["args"][2] if draw_call and len(draw_call["args"]) >= 3 else None,
        },
        "consumers": [
            {"path": c["path"], "call": c["call"]} for c in consumers if "DrawBlend(" in c["call"]
        ],
        "game_scene_conditional_offset": (
            {
                "call": conditional[0]["call"],
                "off_set_arg": conditional[0]["off_set_arg"],
            }
            if len(conditional) == 1
            else {"count": len(conditional), "calls": [c["call"] for c in conditional]}
        ),
    }
    anchors = [
        _anchor(ctx, MLIBRARY, method, "DrawBlend method body"),
    ]
    for rel in (EFFECT, SPELLOBJECT):
        lines = [str(c["line"]) for c in consumers if c["path"] == rel and "DrawBlend(" in c["call"]]
        if lines:
            anchors.append(
                {
                    "path": rel,
                    "lines": ",".join(lines),
                    "sha256": ctx.sha(rel),
                    "note": "DrawBlend(...,true,...) consumer calls",
                }
            )
    if conditional:
        anchors.append(
            {
                "path": GAMESCENE,
                "lines": ",".join(str(c["line"]) for c in conditional),
                "sha256": ctx.sha(GAMESCENE),
                "note": "conditional offSet argument in GameScene map draw",
            }
        )
    return observed, anchors


def extract_g3(ctx: Context):
    method = ctx.method(MLIBRARY, SIG_DRAWBLEND)
    body = method["body"]
    save_match = re.search(r"\bbool\s+([A-Za-z_]\w*)\s*=\s*DXManager\.Blending\s*;", body)
    save = (
        {
            "symbol": save_match.group(1),
            "type": "bool",
            "source": "DXManager.Blending",
        }
        if save_match is not None
        else None
    )
    set_blend_calls = [
        call
        for call in calls_in(body, names=("SetBlend",))
        if call["callee"] == "DXManager" and call["name"] == "SetBlend"
    ]
    restore = set_blend_calls[-1] if set_blend_calls else None
    restore_call = (
        {"callee": "DXManager.SetBlend", "argc": len(restore["args"]), "args": restore["args"]}
        if restore is not None
        else None
    )
    restore_uses_saved_bool_only = bool(
        save is not None
        and save["type"] == "bool"
        and restore_call is not None
        and restore_call["argc"] == 1
        and restore_call["args"] == [save["symbol"]]
    )
    saves_rate_or_mode = bool(
        re.search(r"\bBlendingRate\b|\bBlendingMode\b", ctx.masked(MLIBRARY))
    )

    set_blend_method = ctx.method(DXMANAGER, SIG_SETBLEND)
    set_blend_body = set_blend_method["body"]
    invlight_match = re.search(r"\bcase\s+BlendMode\.INVLIGHT\s*:", set_blend_body)
    default_match = re.search(r"\bdefault\s*:", set_blend_body)
    invlight_state = None
    normal_state = None
    if invlight_match is not None and default_match is not None and default_match.start() > invlight_match.start():
        invlight_region = set_blend_body[invlight_match.end() : default_match.start()]
        normal_end = set_blend_body.find("}", default_match.end())
        if normal_end < 0:
            normal_end = len(set_blend_body)
        normal_region = set_blend_body[default_match.end() : normal_end]
        raw_invlight = statements_set_render_state(invlight_region, 0)
        raw_normal = statements_set_render_state(normal_region, 0)
        invlight_state = {
            "blend_operation": raw_invlight.get("BlendOperation"),
            "source_blend": raw_invlight.get("SourceBlend"),
            "destination_blend": raw_invlight.get("DestinationBlend"),
        }
        normal_state = {
            "source_blend": raw_normal.get("SourceBlend"),
            "destination_blend": raw_normal.get("DestinationBlend"),
        }

    masked_scene = ctx.masked(GAMESCENE)
    enable_match = re.search(r"DXManager\.SetBlend\(\s*true\s*\)", masked_scene)
    disable_match = re.search(r"DXManager\.SetBlend\(\s*false\s*\)", masked_scene)
    enable_raw = (
        statements_set_render_state(masked_scene, enable_match.end())
        if enable_match is not None
        else {}
    )
    disable_raw = (
        statements_set_render_state(masked_scene, disable_match.end())
        if disable_match is not None
        else {}
    )
    observed = {
        "drawblend_save": save,
        "restore_call": restore_call,
        "restore_uses_saved_bool_only": restore_uses_saved_bool_only,
        "mlibrary_saves_blending_rate_or_mode": saves_rate_or_mode,
        "setblend": {
            "path": DXMANAGER,
            "signature": set_blend_method["signature"],
            "default_rate": param_default(set_blend_method["signature"], "float", "rate"),
            "default_mode": param_default(
                set_blend_method["signature"], "BlendMode", "mode"
            ),
        },
        "normal_state": normal_state,
        "invlight_state": invlight_state,
        "game_scene_lighting": {
            "enable_raw_blend": {
                "source_blend": enable_raw.get("SourceBlend"),
                "destination_blend": enable_raw.get("DestinationBlend"),
            },
            "restore_raw_blend": {
                "source_blend": disable_raw.get("SourceBlend"),
                "destination_blend": disable_raw.get("DestinationBlend"),
            },
        },
    }
    anchors = [
        _anchor(ctx, MLIBRARY, method, "DrawBlend save/restore shape"),
        _anchor(ctx, DXMANAGER, set_blend_method, "SetBlend NORMAL/INVLIGHT render states"),
    ]
    if enable_match is not None and disable_match is not None:
        anchors.append(
            {
                "path": GAMESCENE,
                "lines": "%d,%d"
                % (line_of(masked_scene, enable_match.start()), line_of(masked_scene, disable_match.start())),
                "sha256": ctx.sha(GAMESCENE),
                "note": "scene lighting raw render state boundary",
            }
        )
    return observed, anchors


def extract_g4(ctx: Context):
    method = ctx.method(MLIBRARY, SIG_DRAWTINTED)
    body = method["body"]
    guard_condition, _offset_stmt = offset_guard(body)
    crop = first_if_with(body, "Settings.ScreenWidth")
    main_draw = first_call(body, "DXManager", "Draw")

    mask_guard_match = re.search(r"\bif\s*\(\s*mi\.HasMask\s*\)", body)
    mask_region = None
    mask_guard_condition = None
    if mask_guard_match is not None:
        mask_guard_condition = "mi.HasMask"
        pos = mask_guard_match.end()
        while pos < len(body) and body[pos].isspace():
            pos += 1
        if pos < len(body) and body[pos] == "{":
            close = matching_brace(body, pos)
            mask_region = body[pos + 1 : close]
        else:
            end = body.find(";", pos)
            mask_region = body[pos:end]
    mask_draw = first_call(mask_region, "DXManager", "Draw") if mask_region is not None else None

    main_image = main_draw["args"][0] if main_draw and len(main_draw["args"]) >= 1 else None
    main_rect = main_draw["args"][1] if main_draw and len(main_draw["args"]) >= 2 else None
    main_pos = main_draw["args"][2] if main_draw and len(main_draw["args"]) >= 3 else None
    mask_image = mask_draw["args"][0] if mask_draw and len(mask_draw["args"]) >= 1 else None
    mask_rect = mask_draw["args"][1] if mask_draw and len(mask_draw["args"]) >= 2 else None
    mask_pos = mask_draw["args"][2] if mask_draw and len(mask_draw["args"]) >= 3 else None
    mask_colour = mask_draw["args"][3] if mask_draw and len(mask_draw["args"]) >= 4 else None

    body_mask_ids = sorted(
        {ident for ident in MASK_METADATA_IDS if re.search(r"\b%s\b" % ident, body)}
    )

    ctor = ctx.method(MLIBRARY, SIG_MIMAGE_CTOR)
    ctor_body = ctor["body"]
    mask_reads = [
        match.group(1)
        for match in re.finditer(
            r"\b(MaskWidth|MaskHeight|MaskX|MaskY)\s*=\s*reader\.ReadInt16\(\)", ctor_body
        )
    ]
    has_mask_match = re.search(r"\bHasMask\s*=\s*([^;]+);", ctor_body)
    has_mask_derivation = norm(has_mask_match.group(1)) if has_mask_match is not None else None

    texture = ctx.method(MLIBRARY, SIG_CREATETEXTURE)
    texture_body = texture["body"]
    texture_mask_match = re.search(r"\bif\s*\(\s*HasMask\s*\)", texture_body)
    mask_texture = {"signature": texture["signature"]}
    if texture_mask_match is not None:
        pos = texture_mask_match.end()
        while pos < len(texture_body) and texture_body[pos].isspace():
            pos += 1
        if pos < len(texture_body) and texture_body[pos] == "{":
            close = matching_brace(texture_body, pos)
            region = texture_body[pos + 1 : close]
        else:
            region = texture_body[pos:]
        skip = re.search(r"reader\.ReadBytes\(\s*(\d+)\s*\)", region)
        width = re.search(r"\bw\s*=\s*(\w+)", region)
        height = re.search(r"\bh\s*=\s*(\w+)", region)
        data = re.search(r"DecompressImage\(\s*reader\.ReadBytes\(\s*(\w+)", region)
        mask_texture.update(
            {
                "skip_bytes": skip.group(1) if skip is not None else None,
                "width_from": width.group(1) if width is not None else None,
                "height_from": height.group(1) if height is not None else None,
                "data_bytes_from": data.group(1) if data is not None else None,
            }
        )

    observed = {
        "method": {"path": method["path"], "signature": method["signature"]},
        "default_offSet": off_set_default(method["signature"]),
        "offset_calls": offset_call_count(body),
        "offset_before_crop": bool(
            guard_condition is not None
            and crop is not None
            and body.find("if (offSet)") < crop["index"]
        ),
        "crop_before_draw": bool(
            crop is not None and main_draw is not None and crop["index"] < main_draw["start"]
        ),
        "main_draw": {"image": main_image, "rect": main_rect, "position": main_pos},
        "mask_guard_condition": mask_guard_condition,
        "mask_draw": {
            "image": mask_image,
            "rect": mask_rect,
            "position": mask_pos,
            "colour": mask_colour,
        },
        "mask_point_equals_main": bool(mask_pos is not None and mask_pos == main_pos),
        "mask_rect_equals_main": bool(mask_rect is not None and mask_rect == main_rect),
        "mask_identifiers_in_body": body_mask_ids,
        "mimage_mask_reads": mask_reads,
        "has_mask_derivation": has_mask_derivation,
        "mask_texture": mask_texture,
    }
    anchors = [
        _anchor(ctx, MLIBRARY, method, "DrawTinted main + mask draw"),
        _anchor(ctx, MLIBRARY, ctor, "MImage mask metadata reads"),
        _anchor(ctx, MLIBRARY, texture, "CreateTexture mask layer size/data source"),
    ]
    return observed, anchors


def extract_g5(ctx: Context):
    client_dir = ctx.path(CLIENT_TREE)
    if not client_dir.is_dir():
        raise SourceError("missing source tree: %s" % CLIENT_TREE)
    files = sorted(
        (path for path in client_dir.rglob("*.cs") if path.is_file()),
        key=lambda path: path.relative_to(ctx.root).as_posix(),
    )
    hits = []
    classification_counts = {
        "declaration": 0,
        "deserialize_read": 0,
        "other_read": 0,
        "write": 0,
    }
    identifier_hits = 0
    shadow_reads = set()
    for path in files:
        rel = path.relative_to(ctx.root).as_posix()
        masked = ctx.masked(rel)
        for line_no, line in enumerate(masked.splitlines(), start=1):
            matches = IDENTIFIER_RE.findall(line)
            if not matches:
                continue
            identifier_hits += len(matches)
            identifiers = sorted(set(matches))
            if re.search(r"\b(?:ShadowX|ShadowY|MaskX|MaskY)\s*=\s*reader\.Read(?:Int16|Byte|Int32)\(\)", line):
                classification = "deserialize_read"
                if identifiers and set(identifiers) <= set(SHADOW_IDS):
                    shadow_reads.update(identifiers)
            elif re.match(
                r"^\s*(?:public|internal|protected|private)\s+(?:short|byte|int|long|bool)\b",
                line,
            ):
                classification = "declaration"
            elif re.search(r"\b(?:ShadowX|ShadowY|MaskX|MaskY)\b\s*=[^=]", line):
                classification = "write"
            else:
                classification = "other_read"
            classification_counts[classification] += 1
            hits.append(
                {
                    "path": rel,
                    "classification": classification,
                    "identifiers": identifiers,
                    "_line": line_no,
                }
            )
    for hit in hits:
        hit.pop("_line", None)
    hits.sort(key=lambda item: (item["path"], item["classification"], ",".join(item["identifiers"])))
    observed = {
        "scope": "github/Crystal/Client/**/*.cs",
        "files_scanned": len(files),
        "identifiers": ["MaskX", "MaskY", "ShadowX", "ShadowY"],
        "hit_lines": len(hits),
        "identifier_hits": identifier_hits,
        "classification_counts": classification_counts,
        "hits": hits,
        "shadow_metadata_reads": sorted(shadow_reads),
    }
    anchors = [
        {
            "path": "github/Crystal/Client/**/*.cs",
            "lines": "%d files" % len(files),
            "sha256": None,
            "note": "masked full-tree identifier scan; per-file SHA in source_files",
        }
    ]
    if hits:
        anchors.append(
            {
                "path": hits[0]["path"],
                "lines": "see source_files",
                "sha256": ctx.sha(hits[0]["path"]),
                "note": "first identifier hit is declaration+deserialize block in MLibrary.cs",
            }
        )
    return observed, anchors


EXTRACTORS = {
    "G1": extract_g1,
    "G2": extract_g2,
    "G3": extract_g3,
    "G4": extract_g4,
    "G5": extract_g5,
}


def _anchor(ctx: Context, rel: str, method: dict, note: str) -> dict:
    return {
        "path": rel,
        "lines": "%d-%d" % (method["start_line"], method["end_line"]),
        "sha256": ctx.sha(rel),
        "note": note,
    }


# ---------------------------------------------------------------------------
# expected matrix validation
# ---------------------------------------------------------------------------


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise AuditInputError("matrix invalid: " + message)


def validate_matrix(doc) -> None:
    _require(isinstance(doc, dict), "root must be a JSON object")
    for key in REQUIRED_MATRIX_KEYS:
        _require(key in doc, "missing top-level key %r" % key)
    _require(doc["schema_version"] == 1, "schema_version must be 1")
    freeze = doc["freeze"]
    _require(isinstance(freeze, dict), "freeze must be an object")
    _require(
        isinstance(freeze.get("source_files"), list) and freeze["source_files"],
        "freeze.source_files must be a non-empty list (source SHA record)",
    )
    for index, entry in enumerate(freeze["source_files"]):
        _require(isinstance(entry, dict), "freeze.source_files[%d] must be an object" % index)
        _require(
            isinstance(entry.get("path"), str) and entry["path"],
            "freeze.source_files[%d].path missing" % index,
        )
        _require(
            isinstance(entry.get("sha256"), str)
            and re.fullmatch(r"[0-9a-f]{64}", entry["sha256"]) is not None,
            "freeze.source_files[%d].sha256 must be a 64-char lowercase hex digest" % index,
        )
    checks = doc["checks"]
    _require(isinstance(checks, list), "checks must be a list")
    seen = []
    for index, check in enumerate(checks):
        _require(isinstance(check, dict), "checks[%d] must be an object" % index)
        for key in REQUIRED_CHECK_KEYS:
            _require(key in check, "checks[%d] missing key %r" % (index, key))
        check_id = check["id"]
        _require(isinstance(check_id, str), "checks[%d].id must be a string" % index)
        _require(check_id not in seen, "duplicate check id %r" % check_id)
        seen.append(check_id)
        _require(isinstance(check["expected"], dict), "checks[%d].expected must be an object" % index)
        _require(
            isinstance(check["crystal_facts"], list) and check["crystal_facts"],
            "checks[%d].crystal_facts must be a non-empty list" % index,
        )
        _require(
            isinstance(check["rust_consumption"], list) and check["rust_consumption"],
            "checks[%d].rust_consumption must be a non-empty list" % index,
        )
    _require(
        sorted(seen) == sorted(CHECK_ORDER),
        "check ids must be exactly %s, got %s" % (CHECK_ORDER, seen),
    )
    gaps = doc["gaps"]
    _require(isinstance(gaps, list) and gaps, "gaps must be a non-empty list")
    for index, gap in enumerate(gaps):
        _require(isinstance(gap, dict), "gaps[%d] must be an object" % index)
        for key in REQUIRED_GAP_KEYS:
            _require(key in gap, "gaps[%d] missing key %r" % (index, key))
    candidates = doc["r70_candidates"]
    _require(isinstance(candidates, list) and len(candidates) >= 2, "r70_candidates must list at least 2 candidates")
    for index, candidate in enumerate(candidates):
        _require(isinstance(candidate, dict), "r70_candidates[%d] must be an object" % index)
        for key in REQUIRED_CANDIDATE_KEYS:
            _require(key in candidate, "r70_candidates[%d] missing key %r" % (index, key))


def load_matrix(root: Path, matrix_arg: str | None) -> tuple[dict, str, str]:
    if matrix_arg is None:
        matrix_path = root / MATRIX_REL
    else:
        matrix_path = Path(matrix_arg)
    if not matrix_path.is_file():
        raise AuditInputError("expected matrix not readable: %s" % matrix_path)
    data = matrix_path.read_bytes()
    try:
        document = json.loads(data.decode("utf-8-sig"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise AuditInputError("expected matrix is not valid UTF-8 JSON: %s" % exc)
    validate_matrix(document)
    try:
        display = matrix_path.resolve().relative_to(root).as_posix()
    except ValueError:
        display = matrix_path.resolve().as_posix()
    return document, hashlib.sha256(data).hexdigest(), display


# ---------------------------------------------------------------------------
# observed vs expected comparison
# ---------------------------------------------------------------------------


def diff_values(expected, observed, path: str = "", out: list | None = None) -> list:
    if out is None:
        out = []
    if isinstance(expected, dict):
        if not isinstance(observed, dict):
            out.append({"path": path, "expected": expected, "observed": observed})
            return out
        for key in sorted(expected):
            child = path + "." + key if path else key
            if key not in observed:
                out.append({"path": child, "expected": expected[key], "observed": None, "reason": "missing"})
            else:
                diff_values(expected[key], observed[key], child, out)
        for key in sorted(observed):
            if key not in expected:
                child = path + "." + key if path else key
                out.append({"path": child, "expected": None, "observed": observed[key], "reason": "unexpected"})
        return out
    if isinstance(expected, list):
        if not isinstance(observed, list):
            out.append({"path": path, "expected": expected, "observed": observed})
            return out
        if len(expected) != len(observed):
            out.append(
                {
                    "path": path,
                    "expected": expected,
                    "observed": observed,
                    "reason": "list length %d != %d" % (len(expected), len(observed)),
                }
            )
            return out
        for index, (exp_item, obs_item) in enumerate(zip(expected, observed)):
            diff_values(exp_item, obs_item, "%s[%d]" % (path, index), out)
        return out
    type_mismatch = isinstance(expected, bool) != isinstance(observed, bool)
    if expected != observed or type_mismatch:
        out.append({"path": path, "expected": expected, "observed": observed})
    return out


# ---------------------------------------------------------------------------
# audit
# ---------------------------------------------------------------------------


def audit(root=".", matrix_arg: str | None = None) -> dict:
    root_path = Path(root).resolve()
    if not root_path.is_dir():
        raise AuditInputError("--root is not a directory: %s" % root_path)
    document, matrix_sha, matrix_display = load_matrix(root_path, matrix_arg)
    specs = {check["id"]: check for check in document["checks"]}

    missing_required = [rel for rel in REQUIRED_SOURCES if not (root_path / rel).is_file()]
    if not (root_path / MATRIX_REL).is_file() and matrix_arg is None:
        missing_required.append(MATRIX_REL)
    if not (root_path / CLIENT_TREE).is_dir():
        missing_required.append(CLIENT_TREE + "/")
    if missing_required:
        raise SourceError("missing required sources: %s" % ", ".join(sorted(missing_required)))

    ctx = Context(root_path)
    checks = []
    for check_id in CHECK_ORDER:
        spec = specs[check_id]
        observed, anchors = EXTRACTORS[check_id](ctx)
        expected = spec["expected"]
        mismatches = diff_values(expected, observed)
        checks.append(
            {
                "id": check_id,
                "title": spec["title"],
                "pass": not mismatches,
                "observed": observed,
                "expected": expected,
                "mismatches": mismatches,
                "anchors": anchors,
            }
        )

    source_files = []
    client_dir = root_path / CLIENT_TREE
    for path in sorted(
        (p for p in client_dir.rglob("*.cs") if p.is_file()),
        key=lambda p: p.relative_to(root_path).as_posix(),
    ):
        rel = path.relative_to(root_path).as_posix()
        source_files.append(
            {
                "path": rel,
                "sha256": ctx.sha(rel),
                "bytes": len(ctx.read_bytes(rel)),
                "lines": ctx.line_count(rel),
            }
        )
    missing_optional = []
    for rel in OPTIONAL_SOURCES:
        if not (root_path / rel).is_file():
            missing_optional.append(rel)
            continue
        source_files.append(
            {
                "path": rel,
                "sha256": ctx.sha(rel),
                "bytes": len(ctx.read_bytes(rel)),
                "lines": ctx.line_count(rel),
            }
        )

    passed = sum(1 for check in checks if check["pass"])
    verdict = "PASS" if passed == len(checks) else "FAIL"
    located = sorted(
        ctx.located,
        key=lambda item: (item["path"], item["start_line"], item["signature"]),
    )
    return {
        "schema_version": 1,
        "tool": {"name": TOOL_NAME, "version": TOOL_VERSION},
        "round": 69,
        "task": "JJC-20261027-006-sub-4757d48a",
        "root": root_path.as_posix(),
        "baseline_head": document.get("baseline_head"),
        "matrix": {"path": matrix_display, "sha256": matrix_sha},
        "source_files": source_files,
        "checks": checks,
        "coverage": {
            "required_sources": list(REQUIRED_SOURCES),
            "missing_required": [],
            "optional_sources": list(OPTIONAL_SOURCES),
            "missing_optional": missing_optional,
            "client_files_scanned": sum(
                1 for entry in source_files if entry["path"].startswith(CLIENT_TREE + "/")
            ),
            "identifiers_scanned": list(observed["identifiers"]) if observed else [],
            "methods_located": located,
            "checks_total": len(checks),
            "checks_passed": passed,
        },
        "summary": {
            "checks_total": len(checks),
            "checks_passed": passed,
            "missing_required": [],
            "missing_optional": missing_optional,
        },
        "limitations": [
            "static source-text audit only; it does not render pixels, load assets or touch the GPU",
            "crop checks assert ordering + the four Crystal boundary conditions, not D3D pixel clipping",
            "G5 scans masked source (comments/strings blanked) so pseudo mentions never count as consumption",
            "expected values are human-frozen in the matrix; this tool never writes them back",
        ],
        "verdict": verdict,
    }


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        prog=TOOL_NAME,
        description="Round 69 Crystal->Rust render contract auditor (read-only).",
    )
    parser.add_argument("--root", default=".", help="repository root to audit (default: .)")
    parser.add_argument("--out", default=None, help="optional path for the observed JSON report")
    parser.add_argument(
        "--matrix",
        default=None,
        help="optional expected matrix path (default: <root>/%s)" % MATRIX_REL,
    )
    args = parser.parse_args(argv)

    try:
        report = audit(args.root, args.matrix)
    except AuditInputError as exc:
        print("INPUT ERROR: %s" % exc, file=sys.stderr)
        return 2
    except SourceError as exc:
        print("SOURCE ERROR: %s" % exc, file=sys.stderr)
        return 2

    payload = json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    if args.out:
        out_path = Path(args.out)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(payload, encoding="utf-8", newline="\n")

    print("round69 render contract audit")
    print("root: %s" % report["root"])
    print("matrix: %s sha256=%s" % (report["matrix"]["path"], report["matrix"]["sha256"]))
    for check in report["checks"]:
        print("[%s] %s" % (check["id"], "PASS" if check["pass"] else "FAIL"))
        for mismatch in check["mismatches"]:
            print(
                "  mismatch %s: expected=%r observed=%r"
                % (mismatch["path"], mismatch.get("expected"), mismatch.get("observed"))
            )
    summary = report["summary"]
    print(
        "verdict: %s checks=%d/%d missing_required=%d missing_optional=%d"
        % (
            report["verdict"],
            summary["checks_passed"],
            summary["checks_total"],
            len(summary["missing_required"]),
            len(summary["missing_optional"]),
        )
    )
    if args.out:
        print("wrote: %s" % Path(args.out).as_posix())
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
