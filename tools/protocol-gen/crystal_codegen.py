"""Translate Crystal packet/record bodies into Rust.

Read and write bodies are translated independently and then compared through a
*wire signature* (the ordered list of values on the wire).  A packet is only
emitted when both translations agree, which catches any statement the translator
misread instead of silently shipping a broken protocol.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field

from crystal_packets import FieldDecl, iter_units
from crystal_types import CALL_TO_PRIM, READER_METHOD, WRITER_METHOD, RustType, TypeRegistry, snake_case


class TranslationError(RuntimeError):
    pass


@dataclass
class Translation:
    lines: list[str] = field(default_factory=list)
    signature: list[str] = field(default_factory=list)
    decoded_fields: list[str] = field(default_factory=list)


def strip_semicolon(unit: str) -> str:
    return unit.strip().rstrip(";").strip()


def split_blocks(unit: str) -> list[str]:
    text = unit.strip()
    if text.startswith("{"):
        text = text[1:]
    if text.endswith("}"):
        text = text[:-1]
    units, _ = iter_units(text)
    return [unit.strip() for unit in units]


_FIELD_NAME = re.compile(r"^[A-Za-z_]\w*$")
_FOR_HEADER = re.compile(
    r"^for\s*\(\s*(?:int|var)\s+(?P<idx>\w+)\s*=\s*0\s*;\s*(?P=idx)\s*<\s*(?P<limit>.+?)\s*;\s*(?P=idx)\s*\+\+\s*\)\s*(?P<body>.*)$"
)
_WRITE_CALL = re.compile(r"^writer\.Write\((?P<arg>.+)\)$")
_IF_STMT = re.compile(r"^if\s*\((?P<cond>.+?)\)\s*(?P<body>.+)$")


@dataclass
class Ctx:
    registry: TypeRegistry
    fields: dict[str, FieldDecl]
    out: str = "out"
    locals: dict[str, str] = field(default_factory=dict)
    # 曾经当作集合长度用的局部变量（Crystal 里会 `count = reader.ReadInt32();` 复用）。
    count_vars: set[str] = field(default_factory=set)

    def field_type(self, name: str) -> RustType:
        decl = self.fields.get(name)
        if decl is None:
            raise TranslationError(f"unknown field {name!r}")
        resolved = self.registry.resolve(decl.cs_type)
        if resolved is None:
            raise TranslationError(f"unsupported field type {decl.cs_type!r} for {name!r}")
        return resolved

    def field_rust(self, name: str) -> str:
        return f"{self.out}.{snake_case(name)}"


# ---------------------------------------------------------------------------
# expressions
# ---------------------------------------------------------------------------


def read_expr(expr: str, ctx: Ctx, expected: RustType | None) -> tuple[str, list[str]]:
    """Translate a read expression into a Rust expression plus wire signature."""
    expr = expr.strip()
    if expr in ctx.locals:
        return ctx.locals[expr], []
    call = re.fullmatch(r"reader\.(\w+)\((?P<args>.*)\)", expr)
    if call:
        method = call.group(1)
        args = call.group("args").strip()
        if method == "ReadString":
            return "r.read_string()?", ["string"]
        if method == "ReadBytes":
            if args.replace(" ", "") != "reader.ReadInt32()":
                raise TranslationError(f"ReadBytes args: {args!r}")
            return "r.read_bytes_prefixed()?", ["blob"]
        prim = CALL_TO_PRIM.get(method)
        if prim is None:
            raise TranslationError(f"unsupported reader call {method}")
        if args:
            raise TranslationError(f"unexpected args on {method}")
        if expected is not None and expected.kind == "enum":
            underlying = expected.enum_underlying
            if underlying != prim:
                raise TranslationError(f"enum {expected.rust} read as {prim}, expected {underlying}")
            return (
                f"{expected.rust}::from_value(r.{READER_METHOD[prim]}()?).unwrap_or_default()",
                [f"enum:{underlying}"],
            )
        if expected is not None and expected.kind == "string":
            raise TranslationError("string field read with primitive call")
        if expected is not None and expected.kind not in {"prim", "enum"}:
            raise TranslationError(f"type mismatch: read {prim} into {expected.rust}")
        if expected is not None and expected.rust != prim:
            raise TranslationError(f"type mismatch: read {prim} into {expected.rust}")
        return f"r.{READER_METHOD[prim]}()?", [f"prim:{prim}"]
    cast = re.fullmatch(r"\((\w+)\)\s*(.+)", expr)
    if cast and cast.group(1) in ctx.registry.enum_underlying:
        inner_expr, _ = read_expr(cast.group(2), ctx, None)
        underlying = ctx.registry.enum_underlying[cast.group(1)]
        return f"{cast.group(1)}::from_value({inner_expr}).unwrap_or_default()", [f"enum:{underlying}"]
    point = re.fullmatch(r"new\s+Point\(\s*(.+?)\s*,\s*(.+?)\s*\)", expr)
    if point:
        left, _ = read_expr(point.group(1), ctx, None)
        right, _ = read_expr(point.group(2), ctx, None)
        return f"mir_core::Point {{ x: {left}, y: {right} }}", ["prim:i32", "prim:i32"]
    color = re.fullmatch(r"Color\.FromArgb\(\s*(.+?)\s*\)", expr)
    if color:
        inner, _ = read_expr(color.group(1), ctx, None)
        return f"mir_core::Color({inner})", ["prim:i32"]
    datetime = re.fullmatch(r"DateTime\.FromBinary\(\s*(.+?)\s*\)", expr)
    if datetime:
        inner, _ = read_expr(datetime.group(1), ctx, None)
        return f"mir_core::DateTimeBinary({inner})", ["prim:i64"]
    new_object = re.fullmatch(r"new\s+(\w+)\(\s*reader\s*(?:,\s*version\s*,\s*customVersion\s*)?\)", expr)
    if new_object:
        struct = new_object.group(1)
        return f"{struct}::decode(r)?", [f"struct:{struct}"]
    raise TranslationError(f"unsupported read expression: {expr}")


def write_sig(rt: RustType) -> list[str]:
    if rt.kind == "prim":
        return [f"prim:{rt.rust}"]
    if rt.kind == "enum":
        return [f"enum:{rt.enum_underlying}"]
    if rt.kind == "string":
        return ["string"]
    if rt.kind == "point":
        return ["prim:i32", "prim:i32"]
    if rt.kind == "color":
        return ["prim:i32"]
    if rt.kind == "datetime":
        return ["prim:i64"]
    if rt.kind == "struct":
        return [f"struct:{rt.rust}"]
    if rt.kind == "blob":
        return ["blob"]
    raise TranslationError(f"cannot describe type {rt}")


def _element_type(rt: RustType, registry: TypeRegistry) -> RustType:
    if rt.kind == "list":
        inner = rt.cs[len("List<") : -1]
    elif rt.kind == "array":
        inner = rt.cs[:-2]
    else:
        raise TranslationError(f"{rt.cs} is not a collection")
    element = registry.resolve(inner)
    if element is None:
        raise TranslationError(f"unsupported element type {inner}")
    return element


def _emit_element_write(element: RustType, expr: str, indent: str) -> list[str]:
    # `expr` is a reference (`value` in `for value in &list`), so scalars need a deref.
    if element.kind == "prim":
        return [f"{indent}w.{WRITER_METHOD[element.rust]}(*{expr});"]
    if element.kind == "enum":
        underlying = element.enum_underlying
        return [f"{indent}w.{WRITER_METHOD[underlying]}(*{expr} as {underlying});"]
    if element.kind == "string":
        return [f"{indent}w.write_string({expr});"]
    if element.kind == "struct":
        return [f"{indent}{expr}.encode(w);"]
    if element.kind == "point":
        return [f"{indent}w.write_i32({expr}.x);", f"{indent}w.write_i32({expr}.y);"]
    if element.kind == "color":
        return [f"{indent}w.write_i32({expr}.0);"]
    if element.kind == "datetime":
        return [f"{indent}w.write_i64({expr}.0);"]
    raise TranslationError(f"unsupported element type {element.rust}")


def write_value(rt: RustType, expr: str, ctx: Ctx) -> tuple[list[str], list[str]]:
    if rt.kind == "prim":
        return [f"w.{WRITER_METHOD[rt.rust]}({expr});"], [f"prim:{rt.rust}"]
    if rt.kind == "enum":
        underlying = rt.enum_underlying
        return [f"w.{WRITER_METHOD[underlying]}({expr} as {underlying});"], [f"enum:{underlying}"]
    if rt.kind == "string":
        return [f"w.write_string(&{expr});"], ["string"]
    if rt.kind == "point":
        return [f"w.write_i32({expr}.x);", f"w.write_i32({expr}.y);"], ["prim:i32", "prim:i32"]
    if rt.kind == "color":
        return [f"w.write_i32({expr}.0);"], ["prim:i32"]
    if rt.kind == "datetime":
        return [f"w.write_i64({expr}.0);"], ["prim:i64"]
    if rt.kind == "struct":
        return [f"{expr}.encode(w);"], [f"struct:{rt.rust}"]
    if rt.kind == "blob":
        return [f"w.write_bytes_prefixed(&{expr});"], ["blob"]
    raise TranslationError(f"cannot write type {rt}")


# ---------------------------------------------------------------------------
# read bodies
# ---------------------------------------------------------------------------

_LIST_ADD = re.compile(
    r"^for\s*\(\s*(?:int|var)\s+(?P<idx>\w+)\s*=\s*0\s*;\s*(?P=idx)\s*<\s*(?P<limit>[\w\.]+)\s*;\s*(?P=idx)\s*\+\+\s*\)\s*"
    r"(?:\{\s*)?(?P<list>[\w\.]+)\s*\.\s*Add\(\s*(?P<value>.+?)\s*\)\s*;?\s*(?:\})?\s*$"
)
_ARRAY_ALLOC = re.compile(r"^(?P<target>\w+)\s*=\s*new\s+(?P<type>\w+)\[reader\.ReadInt32\(\)\]$")
_FIXED_ALLOC = re.compile(r"^(?P<target>\w+)\s*=\s*new\s+(?P<type>\w+)\[(?P<len>\d+)\]$")
_LIST_ALLOC = re.compile(r"^(?P<target>\w+)\s*=\s*new\s+List<(?P<type>\w+)>\(\)$")
_COUNT_LOCAL = re.compile(r"^(?:int|var)\s+(?P<var>\w+)\s*=\s*reader\.ReadInt32\(\)$")
_COUNT_REASSIGN = re.compile(r"^(?P<var>\w+)\s*=\s*reader\.ReadInt32\(\)$")
_LOCAL_DECL = re.compile(r"^(?P<type>\w+)\s+(?P<var>\w+)\s*=\s*(?P<value>.+)$")
_OPTIONAL_IF = re.compile(
    r"^if\s*\(\s*(?P<neg>!?)\s*reader\.ReadBoolean\(\)\s*(?:==\s*true\s*)?\)\s*(?P<body>.+)$"
)
# `if (reader.ReadBoolean()) { ... }`：整块可选载荷（数组等）。
_OPTIONAL_BLOCK = re.compile(
    r"^if\s*\(\s*!?\s*reader\.ReadBoolean\(\)\s*\)\s*\{(?P<body>.*)\}$"
)
# `X = new T[count];`：长度来自之前读出的局部计数变量。
_COUNT_ALLOC = re.compile(r"^(?P<target>\w+)\s*=\s*new\s+(?P<type>\w+)\[(?P<len>\w+)\]$")
_ASSIGN = re.compile(r"^(?P<target>\w+)\s*=\s*(?P<value>.+)$")
_CONTINUE = re.compile(r"^if\s*\(\s*(?P<neg>!?)\s*reader\.ReadBoolean\(\)\s*\)\s*continue$")
_INDEX_ASSIGN = re.compile(r"^(?P<target>\w+)\s*\[\s*\w+\s*\]\s*=\s*(?P<value>.+)$")


def translate_read(units: list[str], ctx: Ctx) -> Translation:
    translation = Translation()
    index = 0
    while index < len(units):
        unit = strip_semicolon(units[index])
        if not unit:
            index += 1
            continue

        count_match = _COUNT_LOCAL.match(unit)
        if count_match is None:
            # `count = reader.ReadInt32();`：复用上面已经当作长度用过的局部变量。
            reassigned = _COUNT_REASSIGN.match(unit)
            if reassigned is not None and reassigned.group("var") in ctx.count_vars:
                count_match = reassigned
        if count_match is not None and index + 1 < len(units):
            # `int count = reader.ReadInt32();` optionally followed by a fresh
            # allocation, then the fill loop (either `Xs.Add(...)` or `Xs[i] = ...`).
            count_var = count_match.group("var")
            ctx.count_vars.add(count_var)
            offset = 1
            skipped_alloc = strip_semicolon(units[index + 1])
            alloc_target = None
            alloc_match = (
                _COUNT_ALLOC.match(skipped_alloc)
                or _ARRAY_ALLOC.match(skipped_alloc)
                or _FIXED_ALLOC.match(skipped_alloc)
                or _LIST_ALLOC.match(skipped_alloc)
            )
            if alloc_match is not None:
                offset = 2
                alloc_target = alloc_match.group("target")
            if index + offset < len(units):
                loop_unit = strip_semicolon(units[index + offset])
                add_match = _LIST_ADD.match(loop_unit)
                if add_match and add_match.group("limit") == count_var:
                    target = add_match.group("list")
                    if "." in target:
                        raise TranslationError(f"nested list target {target!r}")
                    rt = ctx.field_type(target)
                    element = _element_type(rt, ctx.registry)
                    rust_expr, sig = read_expr(add_match.group("value"), ctx, element)
                    translation.lines.append("let count = r.read_count()?;")
                    translation.lines.append("for _ in 0..count {")
                    translation.lines.append(f"    {ctx.field_rust(target)}.push({rust_expr});")
                    translation.lines.append("}")
                    translation.signature.extend(["count", *sig, "list"])
                    translation.decoded_fields.append(target)
                    index += offset + 1
                    continue
                # `X = new T[count]; for (…) { if (reader.ReadBoolean()) X[i] = … }`
                header = _FOR_HEADER.match(loop_unit)
                if header is not None and alloc_target is not None:
                    target = alloc_target
                    rt = ctx.field_type(target)
                    if rt.kind == "array" and header.group("limit") == count_var:
                        element = _element_type(rt, ctx.registry)
                        fill_lines, optional, elem_sig = _array_fill_lines(
                            header.group("limit"),
                            target,
                            _loop_body_units(header.group("body")),
                            element,
                            ctx,
                        )
                        translation.lines.append("let count = r.read_count()?;")
                        translation.lines.extend(fill_lines)
                        if optional:
                            translation.signature.extend(
                                ["count", "flag", f"optional:{elem_sig}", "list"]
                            )
                        else:
                            translation.signature.extend(["count", elem_sig, "list"])
                        translation.decoded_fields.append(target)
                        index += offset + 1
                        continue

        array_match = _ARRAY_ALLOC.match(unit) or _FIXED_ALLOC.match(unit)
        if array_match:
            target = array_match.group("target")
            rt = ctx.field_type(target)
            if rt.kind != "array":
                raise TranslationError(f"{target!r} allocated as array but declared {rt.cs}")
            if index + 1 >= len(units):
                raise TranslationError(f"array {target!r} without fill loop")
            loop_unit = units[index + 1].strip()
            header = _FOR_HEADER.match(loop_unit)
            if header is None:
                raise TranslationError(f"unsupported array fill loop: {loop_unit[:120]}")
            body_units = _loop_body_units(header.group("body"))
            element = _element_type(rt, ctx.registry)
            prefixes = []
            if _ARRAY_ALLOC.match(unit):
                prefixes.append("let count = r.read_count()?;")
            else:
                length = int(_FIXED_ALLOC.match(unit).group("len"))
                prefixes.append(f"let count = {length}usize;")
            fill_lines, optional, elem_sig = _array_fill_lines(
                header.group("limit"), target, body_units, element, ctx
            )
            translation.lines.extend(prefixes)
            translation.lines.extend(fill_lines)
            if optional:
                translation.signature.extend(["count", "flag", f"optional:{elem_sig}", "list"])
            else:
                translation.signature.extend(["count", elem_sig, "list"])
            translation.decoded_fields.append(target)
            index += 2
            continue

        list_alloc = _LIST_ALLOC.match(unit)
        if list_alloc:
            target = list_alloc.group("target")
            rt = ctx.field_type(target)
            if rt.kind != "list":
                raise TranslationError(f"{target!r} allocated as List but declared {rt.cs}")
            # `X = new List<T>(); int count = ...; for (...) X.Add(...);`
            offset = 1
            if index + 1 < len(units) and _COUNT_LOCAL.match(strip_semicolon(units[index + 1])):
                offset = 2
            if index + offset >= len(units):
                raise TranslationError(f"list {target!r} without fill loop")
            loop_unit = strip_semicolon(units[index + offset])
            add_match = _LIST_ADD.match(loop_unit)
            if add_match is None:
                raise TranslationError(f"unsupported list fill: {loop_unit[:100]}")
            element = _element_type(rt, ctx.registry)
            rust_expr, sig = read_expr(add_match.group("value"), ctx, element)
            translation.lines.append("let count = r.read_count()?;")
            translation.lines.append("for _ in 0..count {")
            translation.lines.append(f"    {ctx.field_rust(target)}.push({rust_expr});")
            translation.lines.append("}")
            translation.signature.extend(["count", *sig, "list"])
            translation.decoded_fields.append(target)
            index += offset + 1
            continue

        # `if (reader.ReadBoolean()) { ...整块可选载荷... }`（例如 ManageHeroes 的 Heroes 数组）。
        optional_block = _OPTIONAL_BLOCK.match(unit)
        if optional_block is not None:
            inner_units, _ = iter_units(optional_block.group("body"))
            inner = translate_read(inner_units, ctx)
            translation.lines.append("if r.read_bool()? {")
            translation.lines.extend(f"    {line}" for line in inner.lines)
            translation.lines.append("}")
            translation.signature.extend(["flag", *inner.signature])
            translation.decoded_fields.extend(inner.decoded_fields)
            index += 1
            continue

        optional_match = _OPTIONAL_IF.match(unit)
        if optional_match and not optional_match.group("body").strip().startswith("continue"):
            body = strip_semicolon(optional_match.group("body"))
            assign = re.match(r"^(?P<target>\w+)\s*=\s*(?P<value>.+)$", body)
            if assign is None:
                raise TranslationError(f"unsupported optional body: {body[:100]}")
            target = assign.group("target")
            rt = ctx.field_type(target)
            if rt.kind != "struct":
                raise TranslationError(f"optional assignment into non-struct {rt.cs}")
            rust_expr, sig = read_expr(assign.group("value"), ctx, None)
            cond = "r.read_bool()?" if not optional_match.group("neg") else "!r.read_bool()?"
            translation.lines.append(f"if {cond} {{")
            translation.lines.append(f"    {ctx.field_rust(target)} = Some({rust_expr});")
            translation.lines.append("}")
            translation.signature.extend(["flag", f"optional:{sig[0]}"])
            translation.decoded_fields.append(target)
            index += 1
            continue

        local_match = _LOCAL_DECL.match(unit)
        if local_match and not unit.startswith(("if ", "for ", "while ")):
            value, _ = read_expr(local_match.group("value"), ctx, None)
            ctx.locals[local_match.group("var")] = value
            index += 1
            continue

        point_member = re.fullmatch(r"(?P<target>\w+)\.(?P<axis>X|Y)\s*=\s*(?P<value>.+)$", unit)
        if point_member:
            target = point_member.group("target")
            rt = ctx.field_type(target)
            if rt.kind != "point":
                raise TranslationError(f"member assignment on non-point {target!r}")
            rust_expr, sig = read_expr(point_member.group("value"), ctx, None)
            axis = point_member.group("axis").lower()
            translation.lines.append(f"{ctx.field_rust(target)}.{axis} = {rust_expr};")
            translation.signature.extend(sig)
            translation.decoded_fields.append(target)
            index += 1
            continue

        fixed_loop = _FOR_HEADER.match(unit)
        if fixed_loop:
            body_units = [strip_semicolon(item) for item in _loop_body_units(fixed_loop.group("body"))]
            body_units = [item for item in body_units if item]
            if len(body_units) != 1:
                raise TranslationError(f"unsupported fixed loop: {unit[:120]}")
            index_assign = _INDEX_ASSIGN.match(body_units[0])
            if index_assign is None:
                raise TranslationError(f"unsupported fixed loop body: {body_units[0][:100]}")
            target = index_assign.group("target")
            rt = ctx.field_type(target)
            if rt.kind != "array":
                raise TranslationError(f"fixed loop target {target!r} is not an array")
            limit = fixed_loop.group("limit").strip()
            if not limit.isdigit():
                raise TranslationError(f"fixed loop limit {limit!r} is not a literal")
            element = _element_type(rt, ctx.registry)
            rust_expr, sig = read_expr(index_assign.group("value"), ctx, element)
            translation.lines.append(f"for _ in 0..{limit}usize {{")
            translation.lines.append(f"    {ctx.field_rust(target)}.push({rust_expr});")
            translation.lines.append("}")
            translation.signature.extend(sig * int(limit))
            translation.decoded_fields.append(target)
            index += 1
            continue

        assign_match = _ASSIGN.match(unit)
        if assign_match:
            target = assign_match.group("target")
            rt = ctx.field_type(target)
            rust_expr, sig = read_expr(assign_match.group("value"), ctx, rt)
            translation.lines.append(f"{ctx.field_rust(target)} = {rust_expr};")
            translation.signature.extend(sig)
            translation.decoded_fields.append(target)
            index += 1
            continue

        raise TranslationError(f"unsupported read statement: {unit[:140]}")
    return translation


def _loop_body_units(body: str) -> list[str]:
    if body.strip().startswith("{"):
        return split_blocks(body)
    return [body.strip()]


def _array_fill_lines(
    limit: str, target: str, body_units: list[str], element: RustType, ctx: Ctx
) -> tuple[list[str], bool, str]:
    index_assign: str | None = None
    guard: tuple[bool, str] | None = None
    for raw in body_units:
        unit = strip_semicolon(raw)
        if not unit:
            continue
        index_match = _INDEX_ASSIGN.match(unit)
        if index_match and index_match.group("target") == target:
            index_assign = index_match.group("value")
            continue
        guard_match = _OPTIONAL_IF.match(unit)
        if guard_match:
            body = strip_semicolon(guard_match.group("body"))
            if body == "continue":
                guard = (bool(guard_match.group("neg")), "continue")
                continue
            nested = _INDEX_ASSIGN.match(body)
            if nested and nested.group("target") == target:
                guard = (bool(guard_match.group("neg")), nested.group("value"))
                continue
        raise TranslationError(f"unsupported array fill statement: {unit[:120]}")

    if guard is not None:
        negated, value = guard
        rust_expr, sig = read_expr(value, ctx, element)
        cond = "!r.read_bool()?" if negated else "r.read_bool()?"
        lines = [
            "for _ in 0..count {",
            f"    if {cond} {{",
            f"        {ctx.field_rust(target)}.push(Some({rust_expr}));",
            "    } else {",
            f"        {ctx.field_rust(target)}.push(None);",
            "    }",
            "}",
        ]
        return lines, True, sig[0]
    if index_assign is None:
        raise TranslationError(f"array {target!r} fill loop has no element assignment")
    rust_expr, sig = read_expr(index_assign, ctx, element)
    lines = ["for _ in 0..count {", f"    {ctx.field_rust(target)}.push({rust_expr});", "}"]
    return lines, False, sig[0]


# ---------------------------------------------------------------------------
# write bodies
# ---------------------------------------------------------------------------

_SAVE_CALL = re.compile(r"^[\w\.]+\.Save\(writer\)$")
_SAVE_CHAIN_CALL = re.compile(r"^[\w\.]+\?\.Save\(writer\)$")


def translate_write(units: list[str], ctx: Ctx) -> Translation:
    translation = Translation()
    index = 0
    while index < len(units):
        unit = strip_semicolon(units[index])
        if not unit:
            index += 1
            continue

        write_match = _WRITE_CALL.match(unit)
        if write_match:
            arg = write_match.group("arg").strip()
            count_member = re.fullmatch(r"([\w\.]+)\.(Count|Length)", arg)
            if count_member:
                target = count_member.group(1)
                if "." in target:
                    raise TranslationError(f"nested collection {target!r}")
                rt = ctx.field_type(target)
                if index + 1 >= len(units):
                    raise TranslationError(f"count for {target!r} without loop")
                if rt.kind == "blob":
                    nxt = strip_semicolon(units[index + 1])
                    next_write = _WRITE_CALL.match(nxt)
                    if next_write and next_write.group("arg").strip() == target:
                        translation.lines.append(f"w.write_bytes_prefixed(&{ctx.field_rust(target)});")
                        translation.signature.append("blob")
                        index += 2
                        continue
                    header = _FOR_HEADER.match(nxt)
                    if header is not None:
                        translation.lines.append(f"w.write_bytes_prefixed(&{ctx.field_rust(target)});")
                        translation.signature.append("blob")
                        index += 2
                        continue
                    raise TranslationError(f"unsupported blob write for {target!r}")
                loop_lines, sig = _collection_write_lines(units[index + 1], target, rt, ctx)
                if ctx.fields[target].fixed_len is not None:
                    translation.lines.extend(loop_lines)
                    translation.signature.extend(sig)
                else:
                    translation.lines.append(f"w.write_count({ctx.field_rust(target)}.len());")
                    translation.lines.extend(loop_lines)
                    translation.signature.extend(["count", *sig, "list"])
                index += 2
                continue
            nullable = re.fullmatch(r"([\w\.]+)\s*==\s*null", arg)
            if nullable:
                raise TranslationError("inverted nullability writes are not supported")
            nullable = re.fullmatch(r"([\w\.]+)\s*!=\s*null", arg)
            if nullable:
                target = nullable.group(1)
                if "." in target:
                    raise TranslationError(f"nested optional {target!r}")
                rt = ctx.field_type(target)
                if rt.kind == "array":
                    # `writer.Write(Xs != null); if (Xs != null) { Write(Length); for … }`
                    consume = _optional_array_write_lines(translation, target, rt, units[index + 1 :], ctx)
                    translation.signature.extend(["flag", "count", *consume[1], "list"])
                    index += 1 + consume[0]
                    continue
                if rt.kind != "struct":
                    raise TranslationError(f"optional write of non-struct {rt.cs}")
                # `writer.Write(X != null)` 是「存在标志」本身，必须写出来；
                # 只写 `if let Some` 会少一个字节（曾经真的漏过）。
                translation.lines.append(f"w.write_bool({ctx.field_rust(target)}.is_some());")
                consume = _optional_write_lines(translation, target, units[index + 1 : index + 2], ctx)
                translation.signature.extend(["flag", f"optional:struct:{rt.rust}"])
                index += 1 + consume
                continue
            rust_lines, sig = _write_expr(arg, ctx)
            translation.lines.extend(rust_lines)
            translation.signature.extend(sig)
            index += 1
            continue

        save_match = _SAVE_CALL.match(unit)
        if save_match:
            name = unit[: -len(".Save(writer)")]
            rt = ctx.field_type(name)
            if rt.kind != "struct":
                raise TranslationError(f"Save on non-struct {name!r}")
            translation.lines.append(f"{ctx.field_rust(name)}.encode(w);")
            translation.signature.append(f"struct:{rt.rust}")
            index += 1
            continue

        raise TranslationError(f"unsupported write statement: {unit[:140]}")
    return translation


def _collection_write_lines(
    loop_unit: str, target: str, rt: RustType, ctx: Ctx
) -> tuple[list[str], list[str]]:
    header = _FOR_HEADER.match(loop_unit.strip())
    if header is None:
        raise TranslationError(f"unsupported collection loop: {loop_unit[:120]}")
    limit = header.group("limit").strip()
    if limit not in {f"{target}.Length", f"{target}.Count"}:
        raise TranslationError(f"collection loop limit {limit!r} does not match {target!r}")
    body_units = [strip_semicolon(unit) for unit in _loop_body_units(header.group("body"))]
    body_units = [unit for unit in body_units if unit]
    element = _element_type(rt, ctx.registry)
    field_ref = ctx.field_rust(target)
    index_var = header.group("idx")

    alias = "v"
    if body_units:
        alias_match = re.match(
            r"^(?P<type>\w+)\s+(?P<name>\w+)\s*=\s*" + re.escape(target) + r"\[[^\]]+\]$", body_units[0]
        )
        if alias_match:
            alias = alias_match.group("name")
            body_units = body_units[1:]
        else:
            indexed_alias = re.fullmatch(
                r"(?P<name>\w+)\s*\[\s*" + re.escape(index_var) + r"\s*\]\s*\.\s*Save\(writer\)", body_units[0]
            )
            if indexed_alias and indexed_alias.group("name") == target:
                return [f"for value in &{field_ref} {{ value.encode(w); }}"], [f"struct:{element.rust}"]

    if not body_units:
        lines = [f"for value in &{field_ref} {{"]
        lines.extend(_emit_element_write(element, "value", "    "))
        lines.append("}")
        return lines, write_sig(element)

    first = body_units[0]
    nullable_write = re.fullmatch(r"writer\.Write\(\s*(?P<name>\w+)\s*!=\s*null\s*\)", first)
    if nullable_write is None:
        # 元素直接按下标访问的写法：`writer.Write(Xs[i] != null)`。
        nullable_write = re.fullmatch(
            r"writer\.Write\(\s*(?P<name>" + re.escape(target) + r")\s*\[[^\]]+\]\s*!=\s*null\s*\)",
            first,
        )
    if nullable_write:
        # 匹配到的是 `alias != null`（局部别名）或 `Xs[i] != null`（按下标访问），两者等价。
        lines = [
            f"for value in &{field_ref} {{",
            "    w.write_bool(value.is_some());",
            "    if let Some(v) = value { v.encode(w); }",
            "}",
        ]
        if len(body_units) > 1 and "Save(writer)" not in body_units[1]:
            raise TranslationError(f"unsupported optional guard body: {body_units[1][:100]}")
        return lines, ["flag", f"optional:struct:{element.rust}"]
    nullable = re.fullmatch(re.escape(alias) + r"\s*(?P<op>==|!=)\s*null", first)
    if nullable:
        inverted = nullable.group("op") == "=="
        lines = [f"for value in &{field_ref} {{"]
        lines.append("    w.write_bool(value.is_none());" if inverted else "    w.write_bool(value.is_some());")
        lines.append("    if let Some(v) = value { v.encode(w); }")
        lines.append("}")
        if len(body_units) > 1 and "Save(writer)" not in body_units[1]:
            raise TranslationError(f"unsupported optional guard body: {body_units[1][:100]}")
        return lines, ["flag", f"optional:struct:{element.rust}"]

    save_like = re.fullmatch(re.escape(alias) + r"\??\.Save\(writer\)", first)
    if save_like:
        return [f"for value in &{field_ref} {{ value.encode(w); }}"], [f"struct:{element.rust}"]
    call = _WRITE_CALL.match(first)
    if call:
        arg = call.group("arg").strip()
        if arg in {alias, f"{target}[{index_var}]"}:
            lines = [f"for value in &{field_ref} {{"]
            lines.extend(_emit_element_write(element, "value", "    "))
            lines.append("}")
            return lines, write_sig(element)
        cast_arg = re.fullmatch(r"\((?P<cast>\w+)\)\s*" + re.escape(target) + r"\[[^\]]+\]", arg)
        if cast_arg and element.kind == "enum":
            underlying = element.enum_underlying
            return (
                [f"for value in &{field_ref} {{", f"    w.{WRITER_METHOD[underlying]}(*value as {underlying});", "}"],
                [f"enum:{underlying}"],
            )
        raise TranslationError(f"unsupported element write argument: {arg!r}")
    raise TranslationError(f"unsupported collection body: {first[:120]}")


def _optional_array_write_lines(
    translation: Translation, target: str, rt: RustType, rest: list[str], ctx: Ctx
) -> tuple[int, list[str]]:
    """`writer.Write(Xs != null); if (Xs != null) { Write(Length); for … }`。

    返回 `(消耗的单元数, 元素签名)`。Crystal 用 `null` 表示「本次不下发这个数组」，
    而 Rust 的 `Vec` 没有 null；约定空数组即等价于 null（Crystal 侧不会发空的非 null 数组
    给这些包），这样 `Default` 也能稳定往返。
    """
    field_ref = ctx.field_rust(target)
    if not rest:
        raise TranslationError(f"optional array {target!r} without guarded block")
    first = strip_semicolon(rest[0])
    guard = _IF_STMT.match(first)
    if guard is None or not re.fullmatch(r"[\w\.]+\s*!=\s*null", guard.group("cond").strip()):
        raise TranslationError(f"unsupported optional array guard: {first[:100]}")
    body = guard.group("body")
    body_units = split_blocks(body) if body.strip().startswith("{") else [body]
    # body_units = [`writer.Write(Xs.Length);`, `for (…) { … }`]
    loop_unit = None
    for unit in body_units:
        if strip_semicolon(unit).startswith("for"):
            loop_unit = strip_semicolon(unit)
            break
    if loop_unit is None:
        raise TranslationError(f"optional array {target!r} has no fill loop")
    inner_lines, elem_sig = _collection_write_lines(loop_unit, target, rt, ctx)
    translation.lines.append(f"w.write_bool(!{field_ref}.is_empty());")
    translation.lines.append(f"if !{field_ref}.is_empty() {{")
    translation.lines.append(f"    w.write_count({field_ref}.len());")
    translation.lines.extend(f"    {line}" for line in inner_lines)
    translation.lines.append("}")
    return 1, elem_sig


def _optional_write_lines(translation: Translation, target: str, rest: list[str], ctx: Ctx) -> int:
    field_ref = ctx.field_rust(target)
    if not rest:
        raise TranslationError(f"optional {target!r} without guarded write")
    first = strip_semicolon(rest[0])
    if first.startswith("{"):
        units = split_blocks(first)
        first = strip_semicolon(units[0]) if units else ""
    guard = _IF_STMT.match(first)
    if guard and re.fullmatch(r"[\w\.]+\s*!=\s*null", guard.group("cond").strip()):
        body = strip_semicolon(guard.group("body"))
        if "Save(writer)" not in body:
            raise TranslationError(f"unsupported optional body {body[:100]}")
        translation.lines.append(f"if let Some(v) = &{field_ref} {{ v.encode(w); }}")
        return 1
    if _SAVE_CHAIN_CALL.match(first):
        translation.lines.append(f"if let Some(v) = &{field_ref} {{ v.encode(w); }}")
        return 1
    raise TranslationError(f"unsupported optional write form: {first[:100]}")


def _write_expr(expr: str, ctx: Ctx) -> tuple[list[str], list[str]]:
    expr = expr.strip()
    cast = re.fullmatch(
        r"\((byte|sbyte|short|ushort|int|uint|long|ulong|Single|UInt16|Int32)\)\s*(?P<inner>[\w\.]+)", expr
    )
    if cast:
        inner = cast.group("inner")
        rt = ctx.field_type(inner)
        if rt.kind == "enum":
            underlying = rt.enum_underlying
            return (
                [f"w.{WRITER_METHOD[underlying]}({ctx.field_rust(inner)} as {underlying});"],
                [f"enum:{underlying}"],
            )
        if rt.kind == "prim":
            cast_prim = {
                "byte": "u8",
                "sbyte": "i8",
                "short": "i16",
                "ushort": "u16",
                "UInt16": "u16",
                "int": "i32",
                "Int32": "i32",
                "uint": "u32",
                "long": "i64",
                "ulong": "u64",
                "Single": "f32",
            }[cast.group(1)]
            if cast_prim != rt.rust:
                raise TranslationError(f"cast {cast.group(1)} of {rt.rust} field {inner!r} changes width")
            return [f"w.{WRITER_METHOD[cast_prim]}({ctx.field_rust(inner)});"], [f"prim:{cast_prim}"]
        raise TranslationError(f"cast of non-scalar {inner!r}")
    member = re.fullmatch(r"([\w\.]+)\s*\.\s*(X|Y)", expr)
    if member:
        target = member.group(1)
        rt = ctx.field_type(target)
        if rt.kind != "point":
            raise TranslationError(f"member access on non-point {target!r}")
        axis = member.group(2).lower()
        return [f"w.write_i32({ctx.field_rust(target)}.{axis});"], ["prim:i32"]
    argb = re.fullmatch(r"([\w\.]+)\s*\.\s*ToArgb\(\)", expr)
    if argb:
        return [f"w.write_i32({ctx.field_rust(argb.group(1))}.0);"], ["prim:i32"]
    binary = re.fullmatch(r"([\w\.]+)\s*\.\s*ToBinary\(\)", expr)
    if binary:
        return [f"w.write_i64({ctx.field_rust(binary.group(1))}.0);"], ["prim:i64"]
    if _FIELD_NAME.match(expr):
        rt = ctx.field_type(expr)
        return write_value(rt, ctx.field_rust(expr), ctx)
    raise TranslationError(f"unsupported write expression: {expr}")
