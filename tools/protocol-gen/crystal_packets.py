"""Parse C# packet classes (Crystal `Shared/ClientPackets.cs` / `ServerPackets.cs`)
and emit Rust equivalents.

The parser understands the small statement vocabulary Crystal actually uses in
`ReadPacket` / `WritePacket`:

* `Field = reader.ReadInt32();`                           (primitive read)
* `Field = (SomeEnum)reader.ReadByte();`                  (enum read)
* `Field = reader.ReadString();`                          (7-bit length UTF-8)
* `Field = reader.ReadBytes(reader.ReadInt32());`         (count-prefixed blob)
* `int count = reader.ReadInt32();`
  `for (int i = 0; i < count; i++) List.Add(new T(reader));`   (count-prefixed list)

Read and write bodies are cross-checked field-by-field; anything that does not
fit the vocabulary is reported as *unsupported* so it can be implemented by hand
instead of being guessed at.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from pathlib import Path

from crystal_enums import strip_comments

CS_PRIMITIVES = {
    "bool": "bool",
    "byte": "u8",
    "sbyte": "i8",
    "short": "i16",
    "ushort": "u16",
    "int": "i32",
    "uint": "u32",
    "long": "i64",
    "ulong": "u64",
    "float": "f32",
    "double": "f64",
    "string": "String",
}

READER_CALLS = {
    "ReadBoolean": "bool",
    "ReadByte": "u8",
    "ReadSByte": "i8",
    "ReadInt16": "i16",
    "ReadUInt16": "u16",
    "ReadInt32": "i32",
    "ReadUInt32": "u32",
    "ReadInt64": "i64",
    "ReadUInt64": "u64",
    "ReadSingle": "f32",
    "ReadDouble": "f64",
}


@dataclass
class FieldDecl:
    name: str
    cs_type: str
    rust_type: str
    is_list: bool = False
    element_type: str | None = None
    fixed_len: int | None = None


@dataclass
class Op:
    """One decoded/encoded value."""

    kind: str  # "prim" | "enum" | "string" | "blob" | "object" | "list"
    field_name: str
    cs_type: str
    rust_type: str
    element_type: str | None = None


@dataclass
class PacketDef:
    name: str
    index_name: str
    index_value: int
    compressed: bool
    direction: str
    fields: list[FieldDecl] = field(default_factory=list)
    ops: list[Op] = field(default_factory=list)
    unsupported: str | None = None
    read_body: str = ""
    write_body: str = ""


class PacketParseError(RuntimeError):
    pass


_CLASS_HEADER = re.compile(r"public\s+(?:sealed\s+)?class\s+(?P<name>\w+)\s*:\s*Packet\s*\{")


def _find_matching_brace(text: str, open_index: int) -> int:
    depth = 0
    for i in range(open_index, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return i
    raise PacketParseError("unbalanced braces")


def _read_balanced(text: str, open_index: int) -> int:
    """Index just past the balanced group that starts at `open_index`."""
    pairs = {"(": ")", "[": "]", "{": "}"}
    opener = text[open_index]
    closer = pairs[opener]
    depth = 0
    for i in range(open_index, len(text)):
        if text[i] == opener:
            depth += 1
        elif text[i] == closer:
            depth -= 1
            if depth == 0:
                return i + 1
    raise PacketParseError("unbalanced group")


_CONTROL = re.compile(r"(for|foreach|while|if|else|switch|do|try|using|lock)\b")


def iter_units(body: str, start: int = 0, end: int | None = None) -> tuple[list[str], int]:
    """Split a method body into logical units.

    A unit is either a simple statement (`...;`) or a control-flow header
    together with the unit it governs.  Returns `(units, consumed_length)`.
    """
    text = " ".join(line.strip() for line in body.splitlines() if line.strip())
    stop = len(text) if end is None else end
    units: list[str] = []
    i = start
    while i < stop:
        if text[i].isspace():
            i += 1
            continue
        if text[i] == "{":
            i = _read_balanced(text, i)
            continue
        if text[i] == ";":
            i += 1
            continue
        unit_start = i
        control = _CONTROL.match(text, i)
        if control:
            keyword = control.group(1)
            i = control.end()
            while i < stop and text[i].isspace():
                i += 1
            if keyword == "else":
                sub_units, sub_end = iter_units(text, i, stop)
                units.append(text[unit_start:i] + " " + (sub_units[0] if sub_units else ""))
                i = sub_end
                continue
            if i < stop and text[i] == "(":
                i = _read_balanced(text, i)
            while i < stop and text[i].isspace():
                i += 1
            if i < stop and text[i] == "{":
                close = _read_balanced(text, i)
                units.append(text[unit_start:close])
                i = close
            else:
                sub_units, sub_end = iter_units(text, i, stop)
                units.append(text[unit_start:i] + " " + (sub_units[0] if sub_units else ""))
                i = sub_end
            continue
        depth = 0
        while i < stop:
            ch = text[i]
            if ch in "([":
                depth += 1
            elif ch in ")]":
                depth -= 1
            elif ch == ";" and depth == 0:
                i += 1
                break
            i += 1
        units.append(text[unit_start:i].strip())
    return units, i


_FIELD_DECL = re.compile(
    r"\bpublic\s+(?P<type>[\w\.<>\[\]]+)\s+(?P<decls>[^;=]+?)\s*(?:=\s*(?P<init>[^;]+))?;"
)

FIELD_SKIP_TYPES = {"override", "class", "sealed", "static", "const"}

_FIXED_LEN = re.compile(r"new\s+\w+\[(?P<len>\d+)\]")


def parse_fields(body: str) -> dict[str, FieldDecl]:
    fields: dict[str, FieldDecl] = {}
    for match in _FIELD_DECL.finditer(body):
        cs_type = match.group("type")
        if cs_type in FIELD_SKIP_TYPES:
            continue
        names = [name.strip() for name in match.group("decls").split(",")]
        init = match.group("init")
        fixed_len = None
        if init is not None:
            fixed = _FIXED_LEN.search(init)
            if fixed:
                fixed_len = int(fixed.group("len"))
        for name in names:
            if not re.fullmatch(r"\w+", name) or name == "Index":
                continue
            decl = FieldDecl(name=name, cs_type=cs_type, rust_type=cs_type, fixed_len=fixed_len)
            list_match = re.fullmatch(r"List<(.+)>", cs_type)
            if list_match:
                decl.is_list = True
                decl.element_type = list_match.group(1)
                decl.rust_type = f"Vec<{list_match.group(1)}>"
            elif cs_type == "byte[]":
                decl.rust_type = "Vec<u8>"
            elif cs_type in CS_PRIMITIVES:
                decl.rust_type = CS_PRIMITIVES[cs_type]
            fields[name] = decl
    return fields


_READ_ASSIGN = re.compile(
    r"^(?P<target>[\w\.]+)\s*=\s*(?:\((?P<cast>\w+)\)\s*)?reader\.(?P<call>\w+)\((?P<args>.*)\)\s*;?$"
)
_OBJECT_NEW = re.compile(r"^(?P<target>[\w\.]+)\s*=\s*new\s+(?P<type>\w+)\(\s*reader\s*\)\s*;?$")
_COUNT_READ = re.compile(r"^(?P<var>\w+)\s*=\s*reader\.ReadInt32\(\)\s*;?$")
_FOR_LIST_ADD = re.compile(
    r"^for\s*\(\s*int\s+(?P<idx>\w+)\s*=\s*0\s*;\s*(?P=idx)\s*<\s*(?P<limit>[\w\.]+)\s*;\s*(?P=idx)\s*\+\+\s*\)\s*"
    r"(?:\{\s*)?(?P<list>[\w\.]+)\s*\.\s*Add\s*\(\s*new\s+(?P<elem>\w+)\s*\(\s*reader\s*\)\s*\)\s*;?\s*(?:\})?\s*;?$"
)


def parse_read_ops(body: str, fields: dict[str, FieldDecl]) -> tuple[list[Op], str | None]:
    units, _ = iter_units(body)
    ops: list[Op] = []
    i = 0
    while i < len(units):
        unit = units[i].rstrip(";").strip()
        count_match = _COUNT_READ.match(unit)
        if count_match and i + 1 < len(units):
            loop_match = _FOR_LIST_ADD.match(units[i + 1].strip())
            if loop_match and loop_match.group("limit") == count_match.group("var"):
                list_name = loop_match.group("list")
                decl = fields.get(list_name)
                if decl is None or not decl.is_list:
                    return ops, f"list field {list_name!r} not declared as List<>"
                ops.append(
                    Op(
                        kind="list",
                        field_name=list_name,
                        cs_type=decl.cs_type,
                        rust_type=decl.rust_type,
                        element_type=loop_match.group("elem"),
                    )
                )
                i += 2
                continue
        if _FOR_LIST_ADD.match(unit):
            return ops, f"unbounded list loop: {unit[:120]}"
        object_match = _OBJECT_NEW.match(unit)
        if object_match:
            target = object_match.group("target")
            decl = fields.get(target)
            ops.append(
                Op(
                    kind="object",
                    field_name=target,
                    cs_type=decl.cs_type if decl else object_match.group("type"),
                    rust_type=decl.rust_type if decl else object_match.group("type"),
                )
            )
            i += 1
            continue
        read_match = _READ_ASSIGN.match(unit)
        if read_match:
            target = read_match.group("target")
            cast = read_match.group("cast")
            call = read_match.group("call")
            args = read_match.group("args").strip()
            decl = fields.get(target)
            if call == "ReadString":
                if args:
                    return ops, f"ReadString with args {args!r}"
                ops.append(Op("string", target, "string", "String"))
                i += 1
                continue
            if call == "ReadBytes":
                if args.replace(" ", "") != "reader.ReadInt32()":
                    return ops, f"ReadBytes with args {args!r}"
                ops.append(Op("blob", target, "byte[]", "Vec<u8>"))
                i += 1
                continue
            prim = READER_CALLS.get(call)
            if prim is None:
                return ops, f"unsupported reader call {call}"
            if args:
                return ops, f"unexpected args on {call}: {args!r}"
            if cast:
                ops.append(Op("enum", target, cast, cast))
            else:
                declared = decl.rust_type if decl else prim
                if declared not in set(CS_PRIMITIVES.values()):
                    declared = prim
                ops.append(Op("prim", target, prim, declared))
            i += 1
            continue
        return ops, f"unsupported statement: {unit[:140]}"
    return ops, None


_WRITE_SIMPLE = re.compile(r"^writer\.Write\((?P<arg>.+)\)\s*;?$")
_FOR_LIST_SAVE = re.compile(
    r"^for\s*\(\s*int\s+(?P<idx>\w+)\s*=\s*0\s*;\s*(?P=idx)\s*<\s*(?P<list>[\w\.]+)\s*\.\s*Count\s*;\s*(?P=idx)\s*\+\+\s*\)\s*"
    r"(?:\{\s*)?(?P<item>[\w\.]+)\s*\[\s*(?P=idx)\s*\]\s*\.\s*Save\(\s*writer\s*\)\s*;?\s*(?:\})?\s*;?$"
)


def parse_write_ops(body: str) -> tuple[list[str], str | None]:
    units, _ = iter_units(body)
    ops: list[str] = []
    i = 0
    while i < len(units):
        unit = units[i].strip()
        write_match = _WRITE_SIMPLE.match(unit)
        if write_match:
            arg = write_match.group("arg").strip()
            count_match = re.fullmatch(r"([\w\.]+)\.Count", arg)
            if count_match and i + 1 < len(units):
                loop_match = _FOR_LIST_SAVE.match(units[i + 1].strip())
                if loop_match and loop_match.group("list") == count_match.group(1):
                    ops.append(f"list:{count_match.group(1)}")
                    i += 2
                    continue
            length_match = re.fullmatch(r"([\w\.]+)\.Length", arg)
            if length_match and i + 1 < len(units):
                nxt = _WRITE_SIMPLE.match(units[i + 1].strip())
                if nxt and nxt.group("arg").strip() == length_match.group(1):
                    ops.append(f"blob:{length_match.group(1)}")
                    i += 2
                    continue
            cast = re.fullmatch(r"\((?P<cast>\w+)\)\s*(?P<expr>[\w\.]+)", arg)
            if cast:
                ops.append(cast.group("expr"))
                i += 1
                continue
            if re.fullmatch(r"[\w\.]+", arg):
                ops.append(arg)
                i += 1
                continue
            return ops, f"unsupported write expression: {arg[:140]}"
        return ops, f"unsupported write statement: {unit[:140]}"
    return ops, None


def _extract_body(text: str, method_signature: str) -> str | None:
    index = text.find(method_signature)
    if index < 0:
        return None
    brace = text.index("{", index + len(method_signature) - 1)
    close = _find_matching_brace(text, brace)
    return text[brace + 1 : close]


def cross_check(packet: PacketDef, write_ops: list[str]) -> str | None:
    expected: list[str] = []
    for op in packet.ops:
        if op.kind in {"list", "blob"}:
            expected.append(f"{op.kind}:{op.field_name}")
        else:
            expected.append(op.field_name)
    if expected != write_ops:
        return f"read order {expected} != write order {write_ops}"
    return None


def parse_packet_file(path: Path, direction: str, index_map: dict[str, int]) -> list[PacketDef]:
    text = strip_comments(path.read_text(encoding="utf-8"))
    packets: list[PacketDef] = []
    for match in _CLASS_HEADER.finditer(text):
        name = match.group("name")
        open_index = text.index("{", match.end() - 1)
        close_index = _find_matching_brace(text, open_index)
        body = text[open_index + 1 : close_index]
        index_match = re.search(
            r"Index\s*\{?\s*get\s*\{?\s*return\s*\(short\)\w+\.(\w+)\s*;", body
        ) or re.search(r"Index\s*=>\s*\(short\)\w+\.(\w+)", body)
        if index_match is None:
            raise PacketParseError(f"{name}: cannot find Index property")
        index_name = index_match.group(1)
        if index_name not in index_map:
            raise PacketParseError(f"{name}: unknown packet id {index_name}")
        compressed = bool(re.search(r"Compressed\s*=>\s*true", body))
        fields = parse_fields(body)
        read_body = _extract_body(body, "protected override void ReadPacket(BinaryReader reader)")
        write_body = _extract_body(body, "protected override void WritePacket(BinaryWriter writer)")
        if read_body is None or write_body is None:
            raise PacketParseError(f"{name}: missing ReadPacket/WritePacket")
        ops, problem = parse_read_ops(read_body, fields)
        packet = PacketDef(
            name=name,
            index_name=index_name,
            index_value=index_map[index_name],
            compressed=compressed,
            direction=direction,
            fields=list(fields.values()),
            ops=ops,
            read_body=read_body,
            write_body=write_body,
        )
        if problem is None:
            write_ops, write_problem = parse_write_ops(write_body)
            problem = write_problem if write_problem else cross_check(packet, write_ops)
        packet.unsupported = problem
        packets.append(packet)
    return packets


def load_packets(crystal: Path) -> tuple[list[PacketDef], list[PacketDef]]:
    from crystal_enums import parse_enum_file

    enums = {e.name: e for e in parse_enum_file(crystal / "Shared" / "Enums.cs")}
    client_ids = {m.name: m.value for m in enums["ClientPacketIds"].members}
    server_ids = {m.name: m.value for m in enums["ServerPacketIds"].members}
    client = parse_packet_file(crystal / "Shared" / "ClientPackets.cs", "client", client_ids)
    server = parse_packet_file(crystal / "Shared" / "ServerPackets.cs", "server", server_ids)
    return client, server


def main() -> None:  # pragma: no cover - manual use
    import sys

    crystal = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("C:/Users/diana/ref/Crystal")
    client, server = load_packets(crystal)
    report = {
        "client": {
            "total": len(client),
            "unsupported": [{"name": p.name, "reason": p.unsupported} for p in client if p.unsupported],
        },
        "server": {
            "total": len(server),
            "unsupported": [{"name": p.name, "reason": p.unsupported} for p in server if p.unsupported],
        },
    }
    out = json.dumps(report, indent=2, ensure_ascii=False)
    print(out[:40000])
    print(f"\nclient unsupported: {len(report['client']['unsupported'])}/{len(client)}")
    print(f"server unsupported: {len(report['server']['unsupported'])}/{len(server)}")


if __name__ == "__main__":  # pragma: no cover
    main()

