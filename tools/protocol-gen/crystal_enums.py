"""Parse C# `enum` declarations (Crystal `Shared/Enums.cs`) and emit Rust enums.

The generator is deliberately conservative: everything it cannot evaluate is
reported as an error instead of being silently skipped, because the generated
Rust is the wire contract of the game protocol.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path

CS_TYPE_TO_RUST = {
    "byte": "u8",
    "sbyte": "i8",
    "short": "i16",
    "ushort": "u16",
    "int": "i32",
    "uint": "u32",
    "long": "i64",
    "ulong": "u64",
}

RUST_KEYWORDS = {
    "as",
    "break",
    "const",
    "continue",
    "crate",
    "else",
    "enum",
    "extern",
    "false",
    "fn",
    "for",
    "if",
    "impl",
    "in",
    "let",
    "loop",
    "match",
    "mod",
    "move",
    "mut",
    "pub",
    "ref",
    "return",
    "self",
    "Self",
    "static",
    "struct",
    "super",
    "trait",
    "true",
    "type",
    "unsafe",
    "use",
    "where",
    "while",
    "async",
    "await",
    "dyn",
    "abstract",
    "become",
    "box",
    "do",
    "final",
    "macro",
    "override",
    "priv",
    "typeof",
    "unsized",
    "virtual",
    "yield",
    "try",
}


@dataclass
class EnumMember:
    name: str
    value: int
    # True when the member is a duplicate of an earlier value in the same enum;
    # Rust cannot express duplicate discriminants, so these become associated consts.
    duplicate: bool = False


@dataclass
class EnumDef:
    name: str
    underlying: str
    is_flags: bool
    members: list[EnumMember] = field(default_factory=list)


class EnumParseError(RuntimeError):
    pass


def strip_comments(text: str) -> str:
    """Remove // and /* */ comments while preserving line structure."""
    out: list[str] = []
    i = 0
    n = len(text)
    in_line = False
    in_block = False
    in_string = False
    while i < n:
        ch = text[i]
        nxt = text[i + 1] if i + 1 < n else ""
        if in_line:
            if ch == "\n":
                in_line = False
                out.append(ch)
            i += 1
            continue
        if in_block:
            if ch == "*" and nxt == "/":
                in_block = False
                i += 2
                continue
            if ch == "\n":
                out.append(ch)
            i += 1
            continue
        if in_string:
            out.append(ch)
            if ch == "\\" and i + 1 < n:
                out.append(text[i + 1])
                i += 2
                continue
            if ch == '"':
                in_string = False
            i += 1
            continue
        if ch == "/" and nxt == "/":
            in_line = True
            i += 2
            continue
        if ch == "/" and nxt == "*":
            in_block = True
            i += 2
            continue
        if ch == '"':
            in_string = True
            out.append(ch)
            i += 1
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def _find_matching_brace(text: str, open_index: int) -> int:
    depth = 0
    for i in range(open_index, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return i
    raise EnumParseError("unbalanced braces")


_ENUM_HEADER = re.compile(
    r"(?:\[(?P<attrs>[^\]]*)\]\s*)*\bpublic\s+enum\s+(?P<name>\w+)\s*(?::\s*(?P<type>\w+)\s*)?\{",
)


def parse_enums(source: str) -> list[EnumDef]:
    text = strip_comments(source)
    enums: list[EnumDef] = []
    for match in _ENUM_HEADER.finditer(text):
        attrs = match.group("attrs") or ""
        name = match.group("name")
        underlying = match.group("type") or "int"
        if underlying not in CS_TYPE_TO_RUST:
            raise EnumParseError(f"enum {name}: unsupported underlying type {underlying}")
        open_index = text.index("{", match.end() - 1)
        close_index = _find_matching_brace(text, open_index)
        body = text[open_index + 1 : close_index]
        enum = EnumDef(
            name=name,
            underlying=CS_TYPE_TO_RUST[underlying],
            is_flags="Flags" in attrs,
        )
        enum.members = _parse_members(name, body)
        enums.append(enum)
    return enums


def _parse_members(enum_name: str, body: str) -> list[EnumMember]:
    members: list[EnumMember] = []
    known: dict[str, int] = {}
    seen_values: set[int] = set()
    next_value = 0
    for raw in _split_members(body):
        item = raw.strip()
        if not item:
            continue
        if "=" in item:
            name_part, expr = item.split("=", 1)
            name = name_part.strip()
            value = _eval_expr(enum_name, expr.strip(), known)
        else:
            name = item.strip()
            value = next_value
        if not re.fullmatch(r"[A-Za-z_]\w*", name):
            raise EnumParseError(f"enum {enum_name}: bad member name {name!r}")
        duplicate = value in seen_values
        seen_values.add(value)
        known[name] = value
        members.append(EnumMember(name=name, value=value, duplicate=duplicate))
        next_value = value + 1
    if not members:
        raise EnumParseError(f"enum {enum_name}: no members")
    return members


def _split_members(body: str) -> list[str]:
    parts: list[str] = []
    depth = 0
    current: list[str] = []
    for ch in body:
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append("".join(current))
            current = []
            continue
        current.append(ch)
    tail = "".join(current)
    if tail.strip():
        parts.append(tail)
    return parts


_EXPR_TOKEN = re.compile(r"\s*(0[xX][0-9a-fA-F]+|0[bB][01]+|\d+|[A-Za-z_]\w*|[|&()<>+\-~])")


def _eval_expr(enum_name: str, expr: str, known: dict[str, int]) -> int:
    translated: list[str] = []
    position = 0
    while position < len(expr):
        match = _EXPR_TOKEN.match(expr, position)
        if not match:
            if expr[position:].strip() == "":
                break
            raise EnumParseError(f"enum {enum_name}: cannot parse expression {expr!r}")
        token = match.group(1)
        position = match.end()
        if token.startswith("0x") or token.startswith("0X"):
            translated.append(str(int(token, 16)))
        elif token.startswith("0b") or token.startswith("0B"):
            translated.append(str(int(token, 2)))
        elif token.isdigit():
            translated.append(token)
        elif token in known:
            translated.append(str(known[token]))
        elif token == "|":
            translated.append("|")
        elif token == "&":
            translated.append("&")
        elif token == "(":
            translated.append("(")
        elif token == ")":
            translated.append(")")
        elif token == "<<":
            translated.append("<<")
        elif token == ">>":
            translated.append(">>")
        elif token == "+":
            translated.append("+")
        elif token == "-":
            translated.append("-")
        elif token == "~":
            translated.append("~")
        else:
            raise EnumParseError(
                f"enum {enum_name}: unknown token {token!r} in expression {expr!r}"
            )
    code = "".join(translated)
    try:
        return int(eval(code, {"__builtins__": {}}, {}))  # noqa: S307 - generated from our own token stream
    except Exception as exc:  # pragma: no cover - defensive
        raise EnumParseError(
            f"enum {enum_name}: failed to evaluate {expr!r} -> {code!r}: {exc}"
        ) from exc


def rust_variant(name: str) -> str:
    if name in RUST_KEYWORDS:
        return f"r#{name}"
    return name


def _signed_type(rust_type: str) -> bool:
    return rust_type.startswith("i")


def emit_rust_enums(enums: list[EnumDef], header: str) -> str:
    lines: list[str] = [
        header,
        "",
        "#![allow("
        "clippy::upper_case_acronyms, clippy::enum_variant_names, "
        "non_camel_case_types, clippy::derivable_impls"
        ")]",
        "",
    ]
    for enum in enums:
        lines.extend(_emit_enum(enum))
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def _emit_enum(enum: EnumDef) -> list[str]:
    out: list[str] = []
    unique = [m for m in enum.members if not m.duplicate]
    zero = next((m for m in unique if m.value == 0), unique[0])
    out.append("/// Mirrors Crystal `Shared/Enums.cs`.")
    out.append("#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]")
    out.append("#[derive(serde::Serialize, serde::Deserialize)]")
    out.append(f"#[repr({enum.underlying})]")
    out.append(f"pub enum {enum.name} {{")
    for member in unique:
        if member is zero:
            out.append("    #[default]")
        out.append(f"    {rust_variant(member.name)} = {member.value},")
    out.append("}")
    out.append("")
    duplicates = [m for m in enum.members if m.duplicate]
    if duplicates:
        out.append(f"impl {enum.name} {{")
        for member in duplicates:
            first = next(m for m in unique if m.value == member.value)
            out.append(
                f"    /// Alias of [`{enum.name}::{first.name}`] (C# duplicate value {member.value})."
            )
            out.append(f"    pub const {member.name}: Self = Self::{first.name};")
        out.append("}")
        out.append("")
    ty = enum.underlying
    out.append(f"impl {enum.name} {{")
    out.append("    /// Every distinct declared value.")
    out.append(f"    pub const ALL: &'static [{enum.name}] = &[")
    for member in unique:
        out.append(f"        {enum.name}::{rust_variant(member.name)},")
    out.append("    ];")
    out.append("")
    out.append("    /// Raw discriminant, as written on the wire.")
    out.append("    #[must_use]")
    out.append(f"    pub const fn value(self) -> {ty} {{")
    out.append("        self as " + ty)
    out.append("    }")
    out.append("")
    out.append("    /// Converts a raw wire value, returning `None` when the value is unknown.")
    out.append("    #[must_use]")
    out.append(f"    pub const fn from_value(value: {ty}) -> Option<Self> {{")
    out.append("        match value {")
    for member in unique:
        out.append(f"            {member.value} => Some(Self::{rust_variant(member.name)}),")
    out.append("            _ => None,")
    out.append("        }")
    out.append("    }")
    out.append("}")
    out.append("")
    out.append(f"impl From<{enum.name}> for {ty} {{")
    out.append(f"    fn from(value: {enum.name}) -> Self {{")
    out.append("        value as " + ty)
    out.append("    }")
    out.append("}")
    out.append("")
    out.append(f"impl TryFrom<{ty}> for {enum.name} {{")
    out.append("    type Error = crate::UnknownEnumValue;")
    out.append("")
    out.append(f"    fn try_from(value: {ty}) -> Result<Self, Self::Error> {{")
    out.append(f"        Self::from_value(value).ok_or(crate::UnknownEnumValue {{")
    out.append(f"            type_name: \"{enum.name}\",")
    out.append("            value: i64::from(value),")
    out.append("        })")
    out.append("    }")
    out.append("}")
    return out


def parse_enum_file(path: Path) -> list[EnumDef]:
    return parse_enums(path.read_text(encoding="utf-8"))


if __name__ == "__main__":  # pragma: no cover - manual use
    import sys

    for definition in parse_enum_file(Path(sys.argv[1])):
        print(f"{definition.name}: {definition.underlying} ({len(definition.members)} members)")
