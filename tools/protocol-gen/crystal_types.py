"""Type resolution between Crystal C# declarations and the Rust protocol crate.

Crystal declares packet/record fields with C# types; this module maps them onto
Rust types and remembers how each type is laid out on the wire so the emitter can
generate `read_*` / `write_*` calls without guessing.
"""

from __future__ import annotations

from dataclasses import dataclass

PRIM_TO_RUST = {
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
}

READER_METHOD = {
    "u8": "read_u8",
    "i8": "read_i8",
    "i16": "read_i16",
    "u16": "read_u16",
    "i32": "read_i32",
    "u32": "read_u32",
    "i64": "read_i64",
    "u64": "read_u64",
    "f32": "read_f32",
    "f64": "read_f64",
    "bool": "read_bool",
}

WRITER_METHOD = {
    "u8": "write_u8",
    "i8": "write_i8",
    "i16": "write_i16",
    "u16": "write_u16",
    "i32": "write_i32",
    "u32": "write_u32",
    "i64": "write_i64",
    "u64": "write_u64",
    "f32": "write_f32",
    "f64": "write_f64",
    "bool": "write_bool",
}

# C# `BinaryReader` call -> Rust primitive type.
CALL_TO_PRIM = {
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


@dataclass(frozen=True)
class RustType:
    kind: str  # prim | enum | string | blob | point | color | datetime | struct | list | array | optional
    rust: str
    cs: str = ""
    element: str | None = None
    enum_underlying: str | None = None
    element_optional: bool = False
    is_flags: bool = False

    @property
    def is_scalar(self) -> bool:
        return self.kind in {"prim", "enum", "string", "point", "color", "datetime", "struct"}


class TypeRegistry:
    def __init__(
        self,
        enum_underlying: dict[str, str],
        structs: set[str],
        deferred: set[str],
        flags_enums: set[str] | None = None,
    ):
        self.enum_underlying = enum_underlying
        self.structs = structs
        self.deferred = deferred
        self.flags_enums = flags_enums or set()

    def resolve(self, cs_type: str) -> RustType | None:
        cs_type = cs_type.strip()
        if cs_type in PRIM_TO_RUST:
            return RustType("prim", PRIM_TO_RUST[cs_type], cs_type)
        if cs_type == "string":
            return RustType("string", "String", cs_type)
        if cs_type == "byte[]":
            return RustType("blob", "Vec<u8>", cs_type)
        if cs_type == "Point":
            return RustType("point", "mir_core::Point", cs_type)
        if cs_type == "Color":
            return RustType("color", "mir_core::Color", cs_type)
        if cs_type == "DateTime":
            return RustType("datetime", "mir_core::DateTimeBinary", cs_type)
        if cs_type in self.enum_underlying:
            return RustType(
                "enum",
                cs_type,
                cs_type,
                enum_underlying=self.enum_underlying[cs_type],
                is_flags=cs_type in self.flags_enums,
            )
        array = _split_generic(cs_type, "[]")
        if array is not None:
            inner = self.resolve(array)
            if inner is None:
                return None
            return RustType("array", f"Vec<{inner.rust}>", cs_type, element=inner.rust)
        list_inner = _split_list(cs_type)
        if list_inner is not None:
            inner = self.resolve(list_inner)
            if inner is None:
                return None
            return RustType("list", f"Vec<{inner.rust}>", cs_type, element=inner.rust)
        if cs_type in self.structs:
            return RustType("struct", cs_type, cs_type)
        return None


def _split_list(cs_type: str) -> str | None:
    if cs_type.startswith("List<") and cs_type.endswith(">"):
        return cs_type[len("List<") : -1]
    return None


def _split_generic(cs_type: str, suffix: str) -> str | None:
    if cs_type.endswith(suffix):
        return cs_type[: -len(suffix)]
    return None


def snake_case(name: str) -> str:
    out: list[str] = []
    for index, ch in enumerate(name):
        if ch.isupper():
            if index > 0 and (not name[index - 1].isupper() or (index + 1 < len(name) and name[index + 1].islower())):
                out.append("_")
            out.append(ch.lower())
        else:
            out.append(ch)
    result = "".join(out)
    if result and result[0].isdigit():
        result = "f_" + result
    if result in {"type", "match", "move", "ref", "box", "loop", "fn", "as", "in", "if", "else", "async", "await", "dyn", "try"}:
        result += "_"
    return result
