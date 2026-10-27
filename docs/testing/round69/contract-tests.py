#!/usr/bin/env python3
"""Round 69-B render-contract integration test runner (physical-isolation mutants).

Board card: JJC-20261027-006-sub-9ca99e64 (R69-B, xingbu integration tests).

What this runner proves, with the *same* frozen auditor, matrix and test bytes:

  * T1  green_real_tree      the frozen auditor returns EXIT0 / verdict PASS /
                             5 of 5 checks on the real repository tree.
  * g1..g5                   five named semantic tests (plan line 58) that load
                             the real sources through the auditor and assert the
                             human-frozen Crystal facts as literals (G1..G5).
  * T2  determinism          two consecutive auditor runs are byte identical.
  * T3  input_error_missing_source
                             removing a Crystal source from a physical copy makes
                             the auditor return EXIT2 (input error, not EXIT1).
  * T4  matrix_tamper        a matrix with a missing check and one with a
                             duplicated check id both return EXIT2.
  * T5..T8                   four semantic mutants (R1..R4), each applied to its
                             own physical, link-free repository copy, inside a
                             complete parseable C# method:
                               R1 DrawBlend offset moved after the crop gate  -> G2 red
                               R2 DrawBlend restore call gains rate argument -> G3 red
                               R3 mask draw position consumes MaskX/MaskY     -> G4 red
                               R4 an effective ShadowX/ShadowY read is added  -> G5 red
  * T8b (supplement)         R4b: the ShadowX deserialize read is removed from
                             the MImage constructor -> G5 red (covers the
                             "read deleted/renamed" wording; not required by the
                             plan, kept clearly separate from R1..R4).
  * T9  no_production_change no tracked file changed, git diff --check clean,
                             HEAD/baseline match and all 13 frozen source SHAs
                             still match the frozen matrix.

Every mutant test records: patch source (absolute path), source SHA before/after,
target check, first failed assertion / first mismatch, full mutated and restored
auditor reports, EXIT codes, and the restored-copy re-run that must be green.
Each variant gets its own physical copy, its own TMP/TEMP/TMPDIR and its own
output directory; hard links, symlinks and reparse points are asserted absent and
file IDs of source vs copy are recorded in isolation.json.

The auditor and this test file are always loaded from the real repository root
(passed via --root), never from a mutated copy; their SHA256 is verified against
the frozen constants below before every auditor invocation.

Exit codes:
    0  all tests passed (and >= 7 executed, 0 skipped)
    1  at least one test failed
    2  harness input error (bad --root/--work-root/--out, missing frozen
       tool/matrix/test)

Usage (default work root):
    py -3 docs/testing/round69/contract-tests.py --root . \
        --out target/round69/b/attempt-1/test-results.json

Usage (independent root, e.g. another department or the Lead gate; the
isolated copies, tmp dirs and outputs all move under --work-root, which must
stay inside --root):
    py -3 docs/testing/round69/contract-tests.py --root . \
        --work-root target/round69/c \
        --out target/round69/c/attempt-1/test-results.json
"""

from __future__ import annotations

import argparse
import copy
import difflib
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

SCHEMA_VERSION = 1
ROUND = 69
SUITE = "round69-contract-tests"
TASK = "JJC-20261027-006-sub-9ca99e64"
BASELINE_HEAD = "9e8d9e828742f866f8b3a42f4bf352c21ce9667c"

# Frozen evidence from R69-A (task input, verified by T1/T9 before/while running).
EXPECTED_TOOL_SHA256 = "1fdf4069a7841c507c931a3b74d4f03fbf549373522d146019bd135cc1f5e729"
EXPECTED_MATRIX_SHA256 = "e513550fffdf91c9809218a52a601270741b84622e6dc4cd89796063387b0416"

MATRIX_REL = "docs/testing/round69/render-contract.json"
TOOL_REL = "tools/testing/round69_render_contract.py"
TEST_REL = "docs/testing/round69/contract-tests.py"
CLIENT_REL = "github/Crystal/Client"
MLIBRARY_REL = "github/Crystal/Client/MirGraphics/MLibrary.cs"

DEFAULT_WORK_ROOT_REL = "target/round69/b"
ATTEMPT_DIRNAME = "attempt-1"
ISOLATED_DIRNAME = "isolated"
DEFAULT_OUT_REL = DEFAULT_WORK_ROOT_REL + "/" + ATTEMPT_DIRNAME + "/test-results.json"

EXPECTED_CLIENT_CS_FILES = 97
EXPECTED_SOURCE_FILE_ENTRIES = 103  # 97 Client C# + 6 optional read-only Rust files

CHECK_ORDER = ["G1", "G2", "G3", "G4", "G5"]

OPTIONAL_SOURCES = [
    "crates/mir-render/src/entities.rs",
    "crates/mir-render/src/frame.rs",
    "crates/mir-render/src/scene.rs",
    "crates/mir-client/src/ui/spellfx.rs",
    "crates/mir-client/src/ui/projectile.rs",
    "crates/mir-asset/src/image.rs",
]

# Necessary-directory physical copy, per plan line 57: the auditor only needs the
# Crystal Client tree, its frozen matrix, the frozen tool/test bytes and the six
# optional read-only Rust files it records SHA for.  Nothing is hardlinked.
COPY_DIRS = [CLIENT_REL]
COPY_FILES = [MATRIX_REL, TOOL_REL, TEST_REL] + OPTIONAL_SOURCES

# ---------------------------------------------------------------------------
# R1..R4 semantic mutants.  Strings are LF in this file; the applier converts to
# the source file's actual newline convention (the Crystal sources are CRLF) and
# asserts the old anchor occurs exactly once before replacing it.  Each mutation
# touches one semantic point inside a complete, parseable C# method body.
# ---------------------------------------------------------------------------

R1_OLD = """            MImage mi = _images[index];

            if (offSet) point.Offset(mi.X, mi.Y);

            if (point.X >= Settings.ScreenWidth || point.Y >= Settings.ScreenHeight || point.X + mi.Width < 0 || point.Y + mi.Height < 0)
                return;

            bool oldBlend = DXManager.Blending;"""
R1_NEW = """            MImage mi = _images[index];

            if (point.X >= Settings.ScreenWidth || point.Y >= Settings.ScreenHeight || point.X + mi.Width < 0 || point.Y + mi.Height < 0)
                return;

            if (offSet) point.Offset(mi.X, mi.Y);

            bool oldBlend = DXManager.Blending;"""

R2_OLD = """            DXManager.Draw(mi.Image, new Rectangle(0, 0, mi.Width, mi.Height), new Vector3((float)point.X, (float)point.Y, 0.0F), colour);

            DXManager.SetBlend(oldBlend);
            mi.CleanTime = CMain.Time + Settings.CleanDelay;
        }
        public void Draw(int index, Rectangle section, Point point, Color colour, bool offSet)"""
R2_NEW = """            DXManager.Draw(mi.Image, new Rectangle(0, 0, mi.Width, mi.Height), new Vector3((float)point.X, (float)point.Y, 0.0F), colour);

            DXManager.SetBlend(oldBlend, rate);
            mi.CleanTime = CMain.Time + Settings.CleanDelay;
        }
        public void Draw(int index, Rectangle section, Point point, Color colour, bool offSet)"""

R3_OLD = """            if (mi.HasMask)
            {
                DXManager.Draw(mi.MaskImage, new Rectangle(0, 0, mi.Width, mi.Height), new Vector3((float)point.X, (float)point.Y, 0.0F), Tint);
            }"""
R3_NEW = """            if (mi.HasMask)
            {
                DXManager.Draw(mi.MaskImage, new Rectangle(0, 0, mi.Width, mi.Height), new Vector3((float)(point.X + mi.MaskX), (float)(point.Y + mi.MaskY), 0.0F), Tint);
            }"""

R4_OLD = """            if (point.X >= Settings.ScreenWidth || point.Y >= Settings.ScreenHeight || point.X + mi.Width < 0 || point.Y + mi.Height < 0)
                return;

            DXManager.Draw(mi.Image, new Rectangle(0, 0, mi.Width, mi.Height), new Vector3((float)point.X, (float)point.Y, 0.0F), colour);

            if (mi.HasMask)"""
R4_NEW = """            if (point.X >= Settings.ScreenWidth || point.Y >= Settings.ScreenHeight || point.X + mi.Width < 0 || point.Y + mi.Height < 0)
                return;

            if (mi.ShadowX != 0 || mi.ShadowY != 0) point.X += mi.ShadowX;

            DXManager.Draw(mi.Image, new Rectangle(0, 0, mi.Width, mi.Height), new Vector3((float)point.X, (float)point.Y, 0.0F), colour);

            if (mi.HasMask)"""

R4B_OLD = """            ShadowX = reader.ReadInt16();"""
R4B_NEW = """            ShadowX = 0;"""

MUTATION_SPECS = [
    {
        "key": "R1",
        "test": "t5_mutation_r1_g2_offset_order",
        "kind": "semantic_mutation",
        "target_check": "G2",
        "expected_red": ["G2"],
        "first_mismatch": "offset_before_crop",
        "file_rel": MLIBRARY_REL,
        "old": R1_OLD,
        "new": R1_NEW,
        "semantic_point": (
            "DrawBlend: the offSet point.Offset(mi.X, mi.Y) is moved after the "
            "screen crop gate, so it is no longer applied before clipping"
        ),
        "plan_ref": "plan line 59: R1 offset moved after crop (G2 red)",
    },
    {
        "key": "R2",
        "test": "t6_mutation_r2_g3_restore_and_factors",
        "kind": "semantic_mutation",
        "target_check": "G3",
        "expected_red": ["G2", "G3"],
        "first_mismatch": "restore_call.argc",
        "file_rel": MLIBRARY_REL,
        "old": R2_OLD,
        "new": R2_NEW,
        "semantic_point": (
            "DrawBlend: the blend restore call becomes SetBlend(oldBlend, rate), "
            "i.e. it restores more than the saved bool Blending (rate is re-applied)"
        ),
        "plan_ref": "plan line 59: R2 restore call SetBlend(oldBlend, rate) (G3 red)",
        "collateral_note": (
            "G2's frozen observed also records the restore call shape "
            "(restore_args), so this mandated mutation is red on G2 *and* G3. "
            "The target check G3 fails on restore_call.argc / restore_call.args / "
            "restore_uses_saved_bool_only."
        ),
    },
    {
        "key": "R3",
        "test": "t7_mutation_r3_g4_mask_coordinate",
        "kind": "semantic_mutation",
        "target_check": "G4",
        "expected_red": ["G4", "G5"],
        "first_mismatch": "mask_draw.position",
        "file_rel": MLIBRARY_REL,
        "old": R3_OLD,
        "new": R3_NEW,
        "semantic_point": (
            "DrawTinted: the mask draw position is shifted by the mask metadata "
            "mi.MaskX/mi.MaskY instead of reusing the body point"
        ),
        "plan_ref": "plan line 59: R3 mask point stacks MaskX/Y (G4 red)",
        "collateral_note": (
            "G5 is a whole-tree ShadowX/ShadowY/MaskX/MaskY identifier scan; the "
            "new effective MaskX/MaskY read is classified other_read and is a "
            "real drift, so this mandated mutation is red on G4 *and* G5. "
            "The target check G4 fails on mask_draw.position."
        ),
    },
    {
        "key": "R4",
        "test": "t8_mutation_r4_g5_shadow_read",
        "kind": "semantic_mutation",
        "target_check": "G5",
        "expected_red": ["G5"],
        "first_mismatch": "classification_counts.other_read",
        "file_rel": MLIBRARY_REL,
        "old": R4_OLD,
        "new": R4_NEW,
        "semantic_point": (
            "DrawTinted: a new effective read of mi.ShadowX/mi.ShadowY is added "
            "to the draw path (shadow metadata gets a consumption site)"
        ),
        "plan_ref": "plan line 59 / menxia condition 3: R4 adds an effective "
        "ShadowX/ShadowY read in valid code (G5 red)",
    },
    {
        "key": "R4b",
        "test": "t8b_mutation_r4b_g5_shadow_deserialize_removed",
        "kind": "supplementary_semantic_mutation",
        "target_check": "G5",
        "expected_red": ["G5"],
        "first_mismatch": "classification_counts.deserialize_read",
        "file_rel": MLIBRARY_REL,
        "old": R4B_OLD,
        "new": R4B_NEW,
        "semantic_point": (
            "MImage ctor: the ShadowX deserialize read is removed (field set to 0), "
            "i.e. a shadow identifier read is deleted"
        ),
        "plan_ref": "supplements the task wording 'shadow read deleted/renamed'; "
        "the four mandated mutants are R1..R4",
        "collateral_note": "no collateral red expected; only G5 observes the scan",
    },
]

# Pure-function semantic literals for g1..g5 (human-frozen matrix facts asserted
# directly, not copied back from the tool output).
G1_OVERLOAD_SIGNATURES = [
    "public void Draw(int index, int x, int y)",
    "public void Draw(int index, Point point, Color colour, bool offSet, float opacity)",
    "public void Draw(int index, Rectangle section, Point point, Color colour, bool offSet)",
    "public void Draw(int index, Rectangle section, Point point, Color colour, float opacity)",
    "public void Draw(int index, Point point, Size size, Color colour)",
]
G1_CONSUMERS = [
    {
        "path": "github/Crystal/Client/MirObjects/Effect.cs",
        "call": "Library.Draw(BaseIndex + CurrentFrame, DrawLocation, Color.White, true)",
    },
    {
        "path": "github/Crystal/Client/MirObjects/Effect.cs",
        "call": "Library.Draw(index, DrawLocation, Color.White, true)",
    },
    {
        "path": "github/Crystal/Client/MirObjects/SpellObject.cs",
        "call": "BodyLibrary.Draw(DrawFrame + FrameIndex, AnimationOffset == default ? DrawLocation : GetDrawWithOffset(), DrawColour, true)",
    },
]
G2_CONSUMERS = [
    {
        "path": "github/Crystal/Client/MirObjects/Effect.cs",
        "call": "Library.DrawBlend(BaseIndex + CurrentFrame, DrawLocation, Color.White, true, Rate)",
    },
    {
        "path": "github/Crystal/Client/MirObjects/Effect.cs",
        "call": "Library.DrawBlend(index, DrawLocation, Color.White, true, Rate)",
    },
    {
        "path": "github/Crystal/Client/MirObjects/SpellObject.cs",
        "call": "BodyLibrary.DrawBlend(DrawFrame + FrameIndex, AnimationOffset == default ? DrawLocation : GetDrawWithOffset(), DrawColour, true, 0.8F)",
    },
]
G5_HITS = [
    {"classification": "declaration", "identifiers": ["MaskX", "MaskY"], "path": "github/Crystal/Client/MirGraphics/MLibrary.cs"},
    {"classification": "declaration", "identifiers": ["ShadowX", "ShadowY"], "path": "github/Crystal/Client/MirGraphics/MLibrary.cs"},
    {"classification": "deserialize_read", "identifiers": ["MaskX"], "path": "github/Crystal/Client/MirGraphics/MLibrary.cs"},
    {"classification": "deserialize_read", "identifiers": ["MaskY"], "path": "github/Crystal/Client/MirGraphics/MLibrary.cs"},
    {"classification": "deserialize_read", "identifiers": ["ShadowX"], "path": "github/Crystal/Client/MirGraphics/MLibrary.cs"},
    {"classification": "deserialize_read", "identifiers": ["ShadowY"], "path": "github/Crystal/Client/MirGraphics/MLibrary.cs"},
]


class HarnessError(RuntimeError):
    """Bad harness input or frozen-evidence mismatch (exit 2)."""


class ContractAssertion(AssertionError):
    """One named test assertion failed."""

    def __init__(self, name, expected, actual, source):
        self.name = name
        self.expected = expected
        self.actual = actual
        self.source = source
        message = "%s failed at %s: expected=%r actual=%r" % (
            name,
            source,
            clip(expected),
            clip(actual),
        )
        super().__init__(message)


def clip(value, limit=400):
    text = repr(value) if not isinstance(value, str) else value
    return text if len(text) <= limit else text[: limit - 3] + "..."


def json_safe(value):
    try:
        json.dumps(value)
        return value
    except (TypeError, ValueError):
        return clip(value)


def utc_now():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def sha256_file(path):
    return sha256_bytes(Path(path).read_bytes())


def read_json_file(path):
    return json.loads(Path(path).read_bytes().decode("utf-8-sig"))


def write_json_artifact(path, document):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(document, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    path.write_bytes(text.encode("utf-8"))


def write_text_artifact(path, text):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(text.encode("utf-8"))


def rel_posix(root, path):
    try:
        return Path(path).resolve().relative_to(Path(root).resolve()).as_posix()
    except ValueError:
        return Path(path).resolve().as_posix()


def newline_style(data):
    return "CRLF" if b"\r\n" in data else "LF"


def build_patch_text(abs_path, before_bytes, after_bytes):
    """Informational unified diff (LF-normalised lines; CRLF/BOM preserved on disk)."""
    before = before_bytes.decode("utf-8-sig").splitlines()
    after = after_bytes.decode("utf-8-sig").splitlines()
    diff = difflib.unified_diff(
        before,
        after,
        fromfile=abs_path + " (baseline, sha256=recorded in mutation-meta.json)",
        tofile=abs_path + " (mutated)",
        lineterm="",
        n=3,
    )
    lines = list(diff)
    return ("\n".join(lines) + "\n") if lines else ""


def apply_snippet_mutation(data, old_text, new_text):
    """Replace exactly one CRLF/LF-normalised snippet; returns (new_bytes, evidence)."""
    style = newline_style(data)
    if style == "CRLF":
        old_pattern = old_text.replace("\n", "\r\n")
        new_pattern = new_text.replace("\n", "\r\n")
    else:
        old_pattern = old_text
        new_pattern = new_text
    text = data.decode("utf-8")
    occurrences = text.count(old_pattern)
    if occurrences != 1:
        raise HarnessError(
            "mutation anchor must occur exactly once, found %d: %r"
            % (occurrences, old_pattern[:80])
        )
    mutated = text.replace(old_pattern, new_pattern)
    return mutated.encode("utf-8"), {
        "newline_style": style,
        "old_anchor_occurrences": occurrences,
        "old_anchor_chars": len(old_pattern),
        "new_anchor_chars": len(new_pattern),
    }


def manifest_digest(pairs):
    """pairs: iterable of (rel_path, sha256); returns a stable tree digest."""
    lines = ["%s\t%s" % (rel, sha) for rel, sha in sorted(pairs)]
    return sha256_bytes(("\n".join(lines) + "\n").encode("utf-8"))


def is_reparse_point(path):
    st = os.stat(path)
    flags = getattr(st, "st_file_attributes", 0)
    return bool(flags & 0x400)  # FILE_ATTRIBUTE_REPARSE_POINT


def file_id(st):
    return "%s:%s" % (st.st_dev, st.st_ino)


# ---------------------------------------------------------------------------
# harness
# ---------------------------------------------------------------------------


class Harness:
    def __init__(self, root, out, work_root=None):
        self.root = Path(root).resolve()
        if not self.root.is_dir():
            raise HarnessError("--root is not a directory: %s" % self.root)
        if work_root is None:
            resolved_work_root = self.root / DEFAULT_WORK_ROOT_REL
        else:
            candidate = Path(work_root)
            if not candidate.is_absolute():
                candidate = self.root / candidate
            resolved_work_root = candidate.resolve()
        if not resolved_work_root.is_relative_to(self.root):
            raise HarnessError(
                "--work-root must stay inside --root %s/, got %s"
                % (self.root.as_posix(), resolved_work_root.as_posix())
            )
        self.work_root = resolved_work_root
        out_path = Path(out)
        if not out_path.is_absolute():
            out_path = (Path.cwd() / out_path).resolve()
        else:
            out_path = out_path.resolve()
        if not out_path.is_relative_to(self.work_root):
            raise HarnessError(
                "--out must stay inside --work-root %s/, got %s "
                "(pass e.g. --work-root %s to run the suite in another evidence directory)"
                % (
                    self.work_root.as_posix(),
                    out_path.as_posix(),
                    out_path.parent.as_posix(),
                )
            )
        self.out = out_path
        self.attempt = self.work_root / ATTEMPT_DIRNAME
        self.isolated_root = self.work_root / ISOLATED_DIRNAME
        self.evidence = self.attempt
        self.tmp = self.work_root / "tmp"
        self.logs = self.attempt / "logs"
        self.tool = self.root / TOOL_REL
        self.matrix_path = self.root / MATRIX_REL
        test_path = Path(__file__).resolve()
        if not test_path.is_relative_to(self.root):
            raise HarnessError(
                "test file %s is outside --root %s; cannot copy frozen test into isolated repos"
                % (test_path, self.root)
            )
        self.test_path = test_path
        self.runs = []
        self.tests = []
        self.test_durations = []
        self.isolation_records = []
        self.mutation_records = []
        self.green_report = None
        self.green_report_sha = None
        self.tool_sha = sha256_file(self.tool) if self.tool.is_file() else None
        self.matrix_sha = sha256_file(self.matrix_path) if self.matrix_path.is_file() else None
        self.test_sha = sha256_file(self.test_path)
        self.matrix = None
        self.freeze_sha = {}
        self.started_utc = utc_now()
        self.started_monotonic = time.monotonic()
        for directory in (self.attempt, self.tmp, self.logs):
            directory.mkdir(parents=True, exist_ok=True)

    # -- preflight ---------------------------------------------------------

    def preflight(self):
        if not self.tool.is_file():
            raise HarnessError("frozen auditor missing: %s" % self.tool)
        if self.tool_sha != EXPECTED_TOOL_SHA256:
            raise HarnessError(
                "frozen auditor SHA mismatch: %s != %s" % (self.tool_sha, EXPECTED_TOOL_SHA256)
            )
        if not self.matrix_path.is_file():
            raise HarnessError("frozen matrix missing: %s" % self.matrix_path)
        if self.matrix_sha != EXPECTED_MATRIX_SHA256:
            raise HarnessError(
                "frozen matrix SHA mismatch: %s != %s"
                % (self.matrix_sha, EXPECTED_MATRIX_SHA256)
            )
        self.matrix = read_json_file(self.matrix_path)
        for entry in self.matrix["freeze"]["source_files"]:
            self.freeze_sha[entry["path"]] = entry["sha256"]
        if self.freeze_sha.get(MLIBRARY_REL) is None:
            raise HarnessError("frozen matrix has no SHA for %s" % MLIBRARY_REL)
        for rel in [CLIENT_REL] + OPTIONAL_SOURCES + [MATRIX_REL, TOOL_REL, TEST_REL]:
            if not (self.root / rel).exists():
                raise HarnessError("required copy input missing: %s" % rel)
        mlibrary = (self.root / MLIBRARY_REL).read_bytes()
        for spec in MUTATION_SPECS:
            data = mlibrary if spec["file_rel"] == MLIBRARY_REL else (self.root / spec["file_rel"]).read_bytes()
            style = newline_style(data)
            old_pattern = spec["old"].replace("\n", "\r\n") if style == "CRLF" else spec["old"]
            count = data.decode("utf-8").count(old_pattern)
            if count != 1:
                raise HarnessError(
                    "mutation anchor for %s occurs %d times (must be 1)" % (spec["key"], count)
                )

    # -- command execution -------------------------------------------------

    def run(self, argv, label, cwd=None, env_extra=None, log_prefix=None):
        seq = len(self.runs) + 1
        prefix = log_prefix or "run"
        stdout_path = self.logs / ("%02d-%s.stdout.txt" % (seq, prefix))
        stderr_path = self.logs / ("%02d-%s.stderr.txt" % (seq, prefix))
        env = os.environ.copy()
        env.setdefault("PYTHONDONTWRITEBYTECODE", "1")
        if env_extra:
            env.update(env_extra)
        started = utc_now()
        started_mono = time.monotonic()
        proc = subprocess.run(
            [str(arg) for arg in argv],
            cwd=str(cwd or self.root),
            env=env,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        duration_ms = int(round((time.monotonic() - started_mono) * 1000))
        write_text_artifact(stdout_path, proc.stdout)
        write_text_artifact(stderr_path, proc.stderr)
        record = {
            "seq": seq,
            "label": label,
            "argv": [str(arg) for arg in argv],
            "cwd": str(cwd or self.root),
            "tmp_dir": (env_extra or {}).get("TMP"),
            "started_utc": started,
            "finished_utc": utc_now(),
            "duration_ms": duration_ms,
            "exit": proc.returncode,
            "stdout_artifact": rel_posix(self.root, stdout_path),
            "stderr_artifact": rel_posix(self.root, stderr_path),
            "stdout_sha256": sha256_bytes(proc.stdout.encode("utf-8")),
            "stderr_sha256": sha256_bytes(proc.stderr.encode("utf-8")),
        }
        self.runs.append(record)
        return {
            "exit": proc.returncode,
            "stdout": proc.stdout,
            "stderr": proc.stderr,
            "record": record,
        }

    def run_tool(self, target_root, out_path, label, tmp_dir):
        """Run the *real* frozen auditor against target_root (never a copy's tool)."""
        out_path = Path(out_path).resolve()
        if not out_path.is_relative_to(self.work_root):
            raise HarnessError(
                "tool --out outside allowed work root %s: %s"
                % (self.work_root.as_posix(), out_path)
            )
        if sha256_file(self.tool) != EXPECTED_TOOL_SHA256:
            raise HarnessError("frozen auditor changed during run; refusing to continue")
        if out_path.exists():
            out_path.unlink()
        tmp_dir = Path(tmp_dir)
        tmp_dir.mkdir(parents=True, exist_ok=True)
        env_extra = {
            "TMP": str(tmp_dir),
            "TEMP": str(tmp_dir),
            "TMPDIR": str(tmp_dir),
            "PYTHONIOENCODING": "utf-8",
        }
        result = self.run(
            [sys.executable, self.tool, "--root", target_root, "--out", out_path],
            label=label,
            cwd=self.root,
            env_extra=env_extra,
            log_prefix="tool-" + label,
        )
        report = None
        report_sha = None
        if out_path.is_file():
            report_sha = sha256_file(out_path)
            try:
                report = json.loads(out_path.read_bytes().decode("utf-8"))
            except (UnicodeDecodeError, json.JSONDecodeError) as exc:
                report = {"_unreadable": str(exc)}
        result.update(
            {
                "out_path": out_path,
                "report": report,
                "report_sha256": report_sha,
                "tmp_dir": str(tmp_dir),
            }
        )
        return result

    # -- physical isolation copies -----------------------------------------

    def make_isolated(self, variant_id, kind, purpose):
        base = self.isolated_root / variant_id
        if base.exists():
            shutil.rmtree(base)
        repo = base / "repo"
        tmp = base / "tmp"
        out = base / "out"
        for directory in (repo, tmp, out):
            directory.mkdir(parents=True, exist_ok=True)
        source_pairs = []
        copied = []
        for dir_rel in COPY_DIRS:
            src_dir = self.root / dir_rel
            for path in sorted(src_dir.rglob("*")):
                if not path.is_file():
                    continue
                rel = path.relative_to(self.root).as_posix()
                dst = repo / rel
                dst.parent.mkdir(parents=True, exist_ok=True)
                # copyfile: physical byte copy, never a hardlink/symlink.
                shutil.copyfile(path, dst)
                copied.append(rel)
                source_pairs.append((rel, sha256_file(path)))
        for rel in COPY_FILES:
            src = self.root / rel
            dst = repo / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(src, dst)
            copied.append(rel)
            source_pairs.append((rel, sha256_file(src)))
        record = self.inspect_copy(variant_id, kind, purpose, repo, copied, source_pairs)
        record["tmp_dir"] = str(tmp)
        record["out_dir"] = str(out)
        self.isolation_records.append(record)
        write_json_artifact(base / "isolation.json", record)
        return {
            "id": variant_id,
            "repo": repo,
            "tmp": tmp,
            "out": out,
            "record": record,
            "manifest": dict(source_pairs),
        }

    def inspect_copy(self, variant_id, kind, purpose, repo, copied, source_pairs):
        issues = {
            "symlink_files": [],
            "reparse_point_files": [],
            "nlink_not_one": [],
            "file_id_collisions": [],
            "sha_mismatch": [],
        }
        total_bytes = 0
        sample_pairs = []
        copy_pairs = []
        sample_rel_set = set(
            [
                MLIBRARY_REL,
                TOOL_REL,
                TEST_REL,
                MATRIX_REL,
                copied[0] if copied else MLIBRARY_REL,
            ]
        )
        for rel in copied:
            src = self.root / rel
            dst = repo / rel
            src_st = os.stat(src)
            dst_st = os.stat(dst)
            if dst.is_symlink():
                issues["symlink_files"].append(rel)
            if is_reparse_point(dst):
                issues["reparse_point_files"].append(rel)
            if dst_st.st_nlink != 1:
                issues["nlink_not_one"].append({"path": rel, "nlink": dst_st.st_nlink})
            if (src_st.st_dev, src_st.st_ino) == (dst_st.st_dev, dst_st.st_ino):
                issues["file_id_collisions"].append(rel)
            src_sha = sha256_file(src)
            dst_sha = sha256_file(dst)
            if src_sha != dst_sha:
                issues["sha_mismatch"].append(rel)
            total_bytes += dst_st.st_size
            copy_pairs.append((rel, dst_sha))
            if rel in sample_rel_set and len(sample_pairs) < 6:
                sample_pairs.append(
                    {
                        "path": rel,
                        "source": {
                            "file_id": file_id(src_st),
                            "st_dev": src_st.st_dev,
                            "st_ino": src_st.st_ino,
                            "nlink": src_st.st_nlink,
                            "stat_size": src_st.st_size,
                            "sha256": src_sha,
                        },
                        "copy": {
                            "file_id": file_id(dst_st),
                            "st_dev": dst_st.st_dev,
                            "st_ino": dst_st.st_ino,
                            "nlink": dst_st.st_nlink,
                            "stat_size": dst_st.st_size,
                            "sha256": dst_sha,
                        },
                        "same_file_id": file_id(src_st) == file_id(dst_st),
                    }
                )
        # No extra files may be smuggled into the copy.
        expected_new = set(copied)
        extra_files = [
            path.relative_to(repo).as_posix()
            for path in repo.rglob("*")
            if path.is_file()
            and path.relative_to(repo).as_posix() not in expected_new
        ]
        for path in repo.rglob("*"):
            if path.is_symlink():
                issues["symlink_files"].append(rel_posix(repo, path))
            if path.is_file() and is_reparse_point(path):
                issues["reparse_point_files"].append(rel_posix(repo, path))
        extra_files = sorted(set(extra_files))
        record = {
            "variant_id": variant_id,
            "kind": kind,
            "purpose": purpose,
            "source_repo": self.root.as_posix(),
            "baseline_head": BASELINE_HEAD,
            "copy_repo": repo.as_posix(),
            "copy_method": (
                "Python shutil.copyfile per file: physical byte-for-byte copy of the "
                "necessary directories/files; no symlink, no reparse point, no hardlink"
            ),
            "copy_scope": {"directories": list(COPY_DIRS), "files": list(COPY_FILES)},
            "source_files_with_frozen_sha_checked": len(copied),
            "file_count": len(copied),
            "bytes": total_bytes,
            "symlink_count": len(issues["symlink_files"]),
            "reparse_point_count": len(issues["reparse_point_files"]),
            "nlink_max_on_copied_files": max(
                (os.stat(repo / rel).st_nlink for rel in copied), default=0
            ),
            "issues": issues,
            "all_nlink_one": not issues["nlink_not_one"],
            "no_symlinks": not issues["symlink_files"],
            "no_reparse_points": not issues["reparse_point_files"],
            "no_file_id_collisions": not issues["file_id_collisions"],
            "no_extra_files": not extra_files,
            "extra_files": extra_files,
            "source_tree_manifest_sha256": manifest_digest(source_pairs),
            "copy_tree_manifest_sha256": manifest_digest(copy_pairs),
            "manifests_equal": manifest_digest(source_pairs) == manifest_digest(copy_pairs),
            "sample_source_copy_pairs": sample_pairs,
            "frozen_bytes_in_copy": {
                "tool_sha256": sha256_file(repo / TOOL_REL),
                "tool_sha256_expected": EXPECTED_TOOL_SHA256,
                "matrix_sha256": sha256_file(repo / MATRIX_REL),
                "matrix_sha256_expected": EXPECTED_MATRIX_SHA256,
                "test_sha256": sha256_file(repo / TEST_REL),
                "test_sha256_running": self.test_sha,
            },
            "tool_matches_frozen": sha256_file(repo / TOOL_REL) == EXPECTED_TOOL_SHA256,
            "matrix_matches_frozen": sha256_file(repo / MATRIX_REL) == EXPECTED_MATRIX_SHA256,
            "test_matches_running_file": sha256_file(repo / TEST_REL) == self.test_sha,
        }
        return record

    def verify_copy_integrity(self, variant, allowed_changed):
        """Re-hash the copied tree: exactly `allowed_changed` files may differ."""
        repo = variant["repo"]
        expected = variant["manifest"]
        changed = []
        for rel, expected_sha in expected.items():
            path = repo / rel
            if not path.is_file():
                changed.append(rel + " (missing)")
                continue
            if sha256_file(path) != expected_sha:
                changed.append(rel)
        changed.sort()
        expected_changed = sorted(allowed_changed)
        return changed, expected_changed

    # -- result assembly ---------------------------------------------------

    def execute_test(self, name, kind, function, *args):
        assertions = []
        status = "passed"
        error = None
        started = time.monotonic()
        try:
            function(self, assertions, *args)
        except ContractAssertion as exc:
            status = "failed"
            error = str(exc)
            exc.test_name = name
        except Exception as exc:  # noqa: BLE001 - record any harness failure as failed test
            status = "failed"
            error = "%s: %s" % (type(exc).__name__, exc)
        duration_ms = int(round((time.monotonic() - started) * 1000))
        entry = {
            "name": name,
            "kind": kind,
            "status": status,
            "assertions_total": len(assertions),
            "assertions_passed": sum(1 for item in assertions if item["ok"]),
            "assertions": assertions,
            "error": error,
        }
        self.test_durations.append({"name": name, "duration_ms": duration_ms})
        self.tests.append(entry)
        marker = "PASS" if status == "passed" else "FAIL"
        print("[%s] %s (%d assertions)" % (marker, name, len(assertions)))
        if error:
            print("    %s" % error.replace("\n", "\n    "))
        return entry

    def write_results(self):
        total = len(self.tests)
        passed = sum(1 for item in self.tests if item["status"] == "passed")
        failed = total - passed
        skipped = 0
        mutation_summaries = []
        for record in self.mutation_records:
            mutation_summaries.append(
                {
                    "key": record["key"],
                    "test": record["test"],
                    "kind": record["kind"],
                    "target_check": record["target_check"],
                    "expected_red": record["expected_red"],
                    "mutated_file_rel": record["mutated_file_rel"],
                    "sha256_before": record["sha256_before"],
                    "sha256_after": record["sha256_after"],
                    "sha256_restored": record["sha256_restored"],
                    "exit_mutated": record["exit_mutated"],
                    "failed_checks_mutated": record["failed_checks_mutated"],
                    "target_check_failed": record["target_check_failed"],
                    "first_failure": record["first_failure"],
                    "first_failed_check": record.get("first_failed_check"),
                    "target_mismatch": record.get("target_mismatch"),
                    "other_checks_green": record.get("other_checks_green"),
                    "restored_run": record["restored_run"],
                    "patch": record["patch_rel"],
                    "meta": record["meta_rel"],
                    "collateral_note": record.get("collateral_note"),
                }
            )
        isolation_summary = []
        for record in self.isolation_records:
            isolation_summary.append(
                {
                    "variant_id": record["variant_id"],
                    "kind": record["kind"],
                    "file_count": record["file_count"],
                    "bytes": record["bytes"],
                    "all_nlink_one": record["all_nlink_one"],
                    "no_symlinks": record["no_symlinks"],
                    "no_reparse_points": record["no_reparse_points"],
                    "no_file_id_collisions": record["no_file_id_collisions"],
                    "no_extra_files": record["no_extra_files"],
                    "manifests_equal": record["manifests_equal"],
                    "tool_matches_frozen": record["tool_matches_frozen"],
                    "matrix_matches_frozen": record["matrix_matches_frozen"],
                    "test_matches_running_file": record["test_matches_running_file"],
                }
            )
        determinism = getattr(self, "determinism", None)
        result = {
            "schema_version": SCHEMA_VERSION,
            "suite": SUITE,
            "round": ROUND,
            "task": TASK,
            "runner": {
                "path": TEST_REL,
                "sha256": self.test_sha,
                "python": sys.version.split()[0],
                "platform": sys.platform,
            },
            "root": self.root.as_posix(),
            "baseline_head": BASELINE_HEAD,
            "auditor": {
                "path": TOOL_REL,
                "sha256": self.tool_sha,
                "expected_sha256": EXPECTED_TOOL_SHA256,
                "loaded_from": "real --root (never from an isolated/mutated copy)",
            },
            "matrix": {
                "path": MATRIX_REL,
                "sha256": self.matrix_sha,
                "expected_sha256": EXPECTED_MATRIX_SHA256,
            },
            "invocation": {
                "argv_without_out": [
                    "py",
                    "-3",
                    TEST_REL,
                    "--root",
                    self.root.as_posix(),
                ],
                "note": (
                    "test-results.json is deterministic: it contains no timestamps, "
                    "durations, random ids or copy file IDs; run logs/timings live in "
                    "runs.json, timings.json and isolation.json"
                ),
            },
            "counts": {
                "total": total,
                "passed": passed,
                "failed": failed,
                "skipped": skipped,
            },
            "result": "PASS" if (failed == 0 and total >= 7 and skipped == 0) else "FAIL",
            "tests": self.tests,
            "green_baseline": (
                {
                    "exit": self.green_exit,
                    "verdict": self.green_report["verdict"],
                    "checks_passed": self.green_report["summary"]["checks_passed"],
                    "checks_total": self.green_report["summary"]["checks_total"],
                    "matrix_sha256": self.green_report["matrix"]["sha256"],
                    "report_sha256": self.green_report_sha,
                    "report_artifact": rel_posix(
                        self.root, self.attempt / "tool-green" / "report-real.json"
                    ),
                }
                if self.green_report is not None
                else None
            ),
            "isolation_summary": isolation_summary,
            "mutations": mutation_summaries,
            "determinism": determinism,
            "boundaries": [
                "static source-text contract audit only: this suite does not render pixels, "
                "load game assets or run the GPU; a PASS never claims production render or "
                "pixel/GPU equivalence with the Crystal client",
                "EXIT1 reds prove that source-contract drift is detected; they are not "
                "evidence that the Rust production renderer was fixed",
                "each mutant runs on its own physical, link-free copy of the necessary "
                "directories (Crystal Client tree + matrix/tool/test bytes + six optional "
                "read-only Rust files), never on the real working tree",
                "the auditor always runs from the real --root; the copies' tool/test/matrix "
                "bytes are SHA-checked against the frozen constants, not executed",
                "R2 and R3 have documented collateral reds (G2/G5) that come from their own "
                "frozen observed shapes; the target check in each case still fails first on "
                "the asserted semantic point",
            ],
        }
        self.result = result
        write_json_artifact(self.out, result)
        if determinism is not None:
            write_json_artifact(self.attempt / "determinism.json", determinism)
        write_json_artifact(
            self.attempt / "runs.json",
            {
                "schema_version": SCHEMA_VERSION,
                "suite": SUITE,
                "task": TASK,
                "harness_argv": sys.argv,
                "root": self.root.as_posix(),
                "out": self.out.as_posix(),
                "started_utc": self.started_utc,
                "finished_utc": utc_now(),
                "duration_ms": int(round((time.monotonic() - self.started_monotonic) * 1000)),
                "runs": self.runs,
            },
        )
        write_json_artifact(
            self.attempt / "timings.json",
            {
                "schema_version": SCHEMA_VERSION,
                "suite": SUITE,
                "task": TASK,
                "generated_utc": utc_now(),
                "total_duration_ms": int(
                    round((time.monotonic() - self.started_monotonic) * 1000)
                ),
                "tests": self.test_durations,
            },
        )
        write_json_artifact(
            self.attempt / "isolation.json",
            {
                "schema_version": SCHEMA_VERSION,
                "suite": SUITE,
                "task": TASK,
                "generated_utc": utc_now(),
                "source_repo": self.root.as_posix(),
                "baseline_head": BASELINE_HEAD,
                "method": (
                    "physical per-file copies (shutil.copyfile) of necessary paths; "
                    "no symlink / reparse point / hardlink; source and copy file IDs "
                    "(st_dev:st_ino) recorded and required to differ"
                ),
                "tmp_policy": (
                    "every auditor invocation runs with TMP=TEMP=TMPDIR set to a "
                    "variant-specific directory under %s/<id>/tmp"
                    % rel_posix(self.root, self.isolated_root)
                ),
                "tool": {"path": TOOL_REL, "sha256": self.tool_sha, "expected": EXPECTED_TOOL_SHA256},
                "matrix": {"path": MATRIX_REL, "sha256": self.matrix_sha, "expected": EXPECTED_MATRIX_SHA256},
                "test": {"path": TEST_REL, "sha256": self.test_sha, "const": "running file"},
                "variants": self.isolation_records,
            },
        )
        return result


# ---------------------------------------------------------------------------
# assertions helper
# ---------------------------------------------------------------------------


def check(assertions, name, condition, expected=None, actual=None, source=None):
    entry = {"name": name, "ok": bool(condition)}
    if not condition:
        entry["expected"] = json_safe(expected)
        entry["actual"] = json_safe(actual)
        assertions.append(entry)
        raise ContractAssertion(name, expected, actual, source or name)
    assertions.append(entry)


def check_equal(assertions, name, actual, expected, source=None):
    check(assertions, name, actual == expected, expected=expected, actual=actual, source=source)


# ---------------------------------------------------------------------------
# T1 baseline
# ---------------------------------------------------------------------------


def test_t1_green_real_tree(h, assertions):
    result = h.run_tool(
        h.root, h.attempt / "tool-green" / "report-real.json", "T1-green-real", h.tmp / "real"
    )
    h.green_exit = result["exit"]
    h.green_report = result["report"]
    h.green_report_sha = result["report_sha256"]
    check_equal(assertions, "exit_code", result["exit"], 0, "auditor on real tree")
    check(assertions, "report_written", result["report"] is not None, True, result["report"] is not None)
    report = result["report"]
    check_equal(assertions, "verdict", report.get("verdict"), "PASS")
    check_equal(assertions, "checks_total", report.get("summary", {}).get("checks_total"), 5)
    check_equal(assertions, "checks_passed", report.get("summary", {}).get("checks_passed"), 5)
    per_check = {item["id"]: item["pass"] for item in report.get("checks", [])}
    check_equal(assertions, "check_ids", sorted(per_check), sorted(CHECK_ORDER))
    check_equal(assertions, "all_checks_pass", all(per_check.values()), True)
    check_equal(assertions, "matrix_sha256", report.get("matrix", {}).get("sha256"), EXPECTED_MATRIX_SHA256)
    check_equal(assertions, "matrix_path", report.get("matrix", {}).get("path"), MATRIX_REL)
    check_equal(assertions, "baseline_head", report.get("baseline_head"), BASELINE_HEAD)
    check_equal(
        assertions,
        "missing_required",
        report.get("summary", {}).get("missing_required"),
        [],
    )
    check_equal(
        assertions, "missing_optional", report.get("summary", {}).get("missing_optional"), []
    )
    check_equal(
        assertions,
        "client_files_scanned",
        report.get("coverage", {}).get("client_files_scanned"),
        EXPECTED_CLIENT_CS_FILES,
    )
    check_equal(
        assertions,
        "source_file_entries",
        len(report.get("source_files", [])),
        EXPECTED_SOURCE_FILE_ENTRIES,
    )
    check_equal(assertions, "tool_identity", report.get("tool"), {"name": "round69_render_contract.py", "version": 1})
    check_equal(assertions, "stderr_empty", result["stderr"], "", "clean EXIT0 run has empty stderr")
    for check_id in CHECK_ORDER:
        check(
            assertions,
            "stdout_has_%s_pass" % check_id,
            ("[%s] PASS" % check_id) in result["stdout"],
            True,
            ("[%s] PASS" % check_id) in result["stdout"],
        )
    check(
        assertions,
        "stdout_verdict_line",
        "verdict: PASS checks=5/5" in result["stdout"],
        True,
        "verdict: PASS checks=5/5" in result["stdout"],
    )


# ---------------------------------------------------------------------------
# g1..g5 named semantic tests (real sources, auditor, literal frozen facts)
# ---------------------------------------------------------------------------


def _semantic_audit(h, assertions, name):
    result = h.run_tool(
        h.root, h.attempt / "tool-semantic" / (name + ".json"), name, h.tmp / "real"
    )
    check_equal(assertions, "exit_code", result["exit"], 0, name)
    report = result["report"]
    check(assertions, "report_written", report is not None, True, report is not None)
    check_equal(assertions, "verdict", report.get("verdict"), "PASS")
    checks = {item["id"]: item for item in report.get("checks", [])}
    return report, checks


def test_g1_draw_offset_order(h, assertions):
    report, checks = _semantic_audit(h, assertions, "g1_draw_offset_order")
    observed = checks["G1"]["observed"]
    check_equal(assertions, "default_offSet_false", observed["default_offSet"], False)
    check_equal(assertions, "offset_calls_once", observed["offset_calls"], 1)
    check_equal(assertions, "offset_guard", observed["offset_guard_condition"], "offSet")
    check_equal(assertions, "offset_call", observed["offset_call"], "point.Offset(mi.X, mi.Y)")
    check_equal(assertions, "offset_before_crop", observed["offset_before_crop"], True)
    check_equal(assertions, "crop_before_draw", observed["crop_before_draw"], True)
    check_equal(
        assertions,
        "crop_conditions",
        observed["crop_conditions"],
        [
            "point.X >= Settings.ScreenWidth",
            "point.Y >= Settings.ScreenHeight",
            "point.X + mi.Width < 0",
            "point.Y + mi.Height < 0",
        ],
    )
    check_equal(
        assertions,
        "integer_overload_no_offset",
        observed["integer_overload"],
        {
            "signature": "public void Draw(int index, int x, int y)",
            "has_off_set_param": False,
            "offset_calls": 0,
        },
    )
    check_equal(
        assertions,
        "overload_signatures",
        [item["signature"] for item in observed["overloads"]],
        G1_OVERLOAD_SIGNATURES,
    )
    check_equal(
        assertions,
        "overload_offset_calls",
        [item["offset_calls"] for item in observed["overloads"]],
        [0, 1, 1, 0, 0],
    )
    check_equal(
        assertions,
        "overload_has_offSet_param",
        [item["has_off_set_param"] for item in observed["overloads"]],
        [False, True, True, False, False],
    )
    check_equal(assertions, "consumers", observed["consumers"], G1_CONSUMERS)
    check_equal(
        assertions,
        "main_draw_position",
        observed["draw_call"]["position"],
        "new Vector3((float)point.X, (float)point.Y, 0.0F)",
    )


def test_g2_blend_offset_order(h, assertions):
    report, checks = _semantic_audit(h, assertions, "g2_blend_offset_order")
    observed = checks["G2"]["observed"]
    check_equal(assertions, "default_offSet_false", observed["default_offSet"], False)
    check_equal(assertions, "default_rate", observed["default_rate"], "1")
    check_equal(assertions, "offset_calls_once", observed["offset_calls"], 1)
    check_equal(assertions, "offset_guard", observed["offset_guard_condition"], "offSet")
    check_equal(assertions, "offset_before_crop", observed["offset_before_crop"], True)
    check_equal(assertions, "crop_before_set_blend", observed["crop_before_set_blend"], True)
    check_equal(assertions, "set_blend_before_draw", observed["set_blend_before_draw"], True)
    check_equal(assertions, "draw_before_restore", observed["draw_before_restore"], True)
    check_equal(assertions, "set_blend_args", observed["set_blend_args"], ["true", "rate"])
    check_equal(assertions, "restore_args", observed["restore_args"], ["oldBlend"])
    check_equal(assertions, "consumers", observed["consumers"], G2_CONSUMERS)
    check_equal(
        assertions,
        "game_scene_conditional_offset",
        observed["game_scene_conditional_offset"],
        {
            "call": "Libraries.MapLibs[fileIndex].DrawBlend(index, new Point(drawX, drawY - s.Height), Color.White, (index >= 2723 && index <= 2732))",
            "off_set_arg": "(index >= 2723 && index <= 2732)",
        },
    )


def test_g3_restore_and_factors(h, assertions):
    report, checks = _semantic_audit(h, assertions, "g3_restore_and_factors")
    observed = checks["G3"]["observed"]
    check_equal(
        assertions,
        "drawblend_save",
        observed["drawblend_save"],
        {"symbol": "oldBlend", "type": "bool", "source": "DXManager.Blending"},
    )
    check_equal(
        assertions,
        "restore_call",
        observed["restore_call"],
        {"callee": "DXManager.SetBlend", "argc": 1, "args": ["oldBlend"]},
    )
    check_equal(assertions, "restore_uses_saved_bool_only", observed["restore_uses_saved_bool_only"], True)
    check_equal(
        assertions,
        "mlibrary_saves_blending_rate_or_mode",
        observed["mlibrary_saves_blending_rate_or_mode"],
        False,
    )
    check_equal(
        assertions,
        "setblend_defaults",
        {
            "default_rate": observed["setblend"]["default_rate"],
            "default_mode": observed["setblend"]["default_mode"],
            "signature": observed["setblend"]["signature"],
        },
        {
            "default_rate": "1F",
            "default_mode": "BlendMode.NORMAL",
            "signature": "public static void SetBlend(bool value, float rate = 1F, BlendMode mode = BlendMode.NORMAL)",
        },
    )
    check_equal(
        assertions,
        "normal_state",
        observed["normal_state"],
        {"source_blend": "Blend.SourceAlpha", "destination_blend": "Blend.One"},
    )
    check_equal(
        assertions,
        "invlight_state",
        observed["invlight_state"],
        {
            "blend_operation": "BlendOperation.Add",
            "source_blend": "Blend.BlendFactor",
            "destination_blend": "Blend.InverseSourceColor",
        },
    )
    check_equal(
        assertions,
        "game_scene_lighting",
        observed["game_scene_lighting"],
        {
            "enable_raw_blend": {"source_blend": "Blend.SourceAlpha", "destination_blend": "Blend.One"},
            "restore_raw_blend": {"source_blend": "Blend.Zero", "destination_blend": "Blend.SourceColor"},
        },
    )


def test_g4_mask_coordinate(h, assertions):
    report, checks = _semantic_audit(h, assertions, "g4_mask_coordinate")
    observed = checks["G4"]["observed"]
    check_equal(assertions, "default_offSet_false", observed["default_offSet"], False)
    check_equal(assertions, "offset_calls_once", observed["offset_calls"], 1)
    check_equal(assertions, "offset_before_crop", observed["offset_before_crop"], True)
    check_equal(assertions, "crop_before_draw", observed["crop_before_draw"], True)
    check_equal(assertions, "mask_guard_condition", observed["mask_guard_condition"], "mi.HasMask")
    check_equal(assertions, "mask_point_equals_main", observed["mask_point_equals_main"], True)
    check_equal(assertions, "mask_rect_equals_main", observed["mask_rect_equals_main"], True)
    check_equal(assertions, "mask_identifiers_in_body", observed["mask_identifiers_in_body"], [])
    check_equal(
        assertions,
        "mimage_mask_reads",
        observed["mimage_mask_reads"],
        ["MaskWidth", "MaskHeight", "MaskX", "MaskY"],
    )
    check_equal(
        assertions,
        "has_mask_derivation",
        observed["has_mask_derivation"],
        "((Shadow >> 7) == 1) ? true : false",
    )
    check_equal(
        assertions,
        "mask_texture",
        observed["mask_texture"],
        {
            "signature": "public unsafe void CreateTexture(BinaryReader reader)",
            "skip_bytes": "12",
            "width_from": "Width",
            "height_from": "Height",
            "data_bytes_from": "Length",
        },
    )
    check_equal(
        assertions,
        "mask_draw",
        observed["mask_draw"],
        {
            "image": "mi.MaskImage",
            "rect": "new Rectangle(0, 0, mi.Width, mi.Height)",
            "position": "new Vector3((float)point.X, (float)point.Y, 0.0F)",
            "colour": "Tint",
        },
    )


def test_g5_shadow_usage_inventory(h, assertions):
    report, checks = _semantic_audit(h, assertions, "g5_shadow_usage_inventory")
    observed = checks["G5"]["observed"]
    check_equal(assertions, "scope", observed["scope"], "github/Crystal/Client/**/*.cs")
    check_equal(assertions, "files_scanned", observed["files_scanned"], EXPECTED_CLIENT_CS_FILES)
    check_equal(assertions, "identifiers", observed["identifiers"], ["MaskX", "MaskY", "ShadowX", "ShadowY"])
    check_equal(assertions, "hit_lines", observed["hit_lines"], 6)
    check_equal(assertions, "identifier_hits", observed["identifier_hits"], 8)
    check_equal(
        assertions,
        "classification_counts",
        observed["classification_counts"],
        {"declaration": 2, "deserialize_read": 4, "other_read": 0, "write": 0},
    )
    check_equal(assertions, "shadow_metadata_reads", observed["shadow_metadata_reads"], ["ShadowX", "ShadowY"])
    check_equal(assertions, "hits", observed["hits"], G5_HITS)
    check_equal(
        assertions,
        "no_effective_shadow_consumption",
        observed["classification_counts"]["other_read"],
        0,
    )


# ---------------------------------------------------------------------------
# T2 determinism
# ---------------------------------------------------------------------------


def test_t2_determinism(h, assertions):
    first = h.run_tool(
        h.root, h.attempt / "tool-green" / "report-repeat-a.json", "T2-determinism-a", h.tmp / "real"
    )
    second = h.run_tool(
        h.root, h.attempt / "tool-green" / "report-repeat-b.json", "T2-determinism-b", h.tmp / "real"
    )
    check_equal(assertions, "first_exit", first["exit"], 0)
    check_equal(assertions, "second_exit", second["exit"], 0)
    check_equal(assertions, "first_sha", first["report_sha256"], second["report_sha256"])
    check(assertions, "byte_identical", first["report_sha256"] == second["report_sha256"], True, first["report_sha256"] == second["report_sha256"])
    check_equal(assertions, "same_as_t1", first["report_sha256"], h.green_report_sha)
    h.determinism = {
        "runs": [
            {"label": "T2-determinism-a", "report_sha256": first["report_sha256"], "exit": first["exit"]},
            {"label": "T2-determinism-b", "report_sha256": second["report_sha256"], "exit": second["exit"]},
        ],
        "byte_identical": first["report_sha256"] == second["report_sha256"],
        "equals_t1_green_report": first["report_sha256"] == h.green_report_sha,
        "note": (
            "the auditor JSON is sorted-key deterministic; the copy roots are not part "
            "of this deterministic report comparison (T1/T2 both target the real tree)"
        ),
        "artifacts": [
            rel_posix(h.root, h.attempt / "tool-green" / "report-repeat-a.json"),
            rel_posix(h.root, h.attempt / "tool-green" / "report-repeat-b.json"),
        ],
    }
    h.determinism["byte_identical_reports"] = first["report_sha256"] == second["report_sha256"]


# ---------------------------------------------------------------------------
# T3 input error: missing Crystal source
# ---------------------------------------------------------------------------


def test_t3_input_error_missing_source(h, assertions):
    variant = h.make_isolated(
        "T3-missing-source",
        "input_error",
        "delete a required Crystal source from a physical copy: auditor must EXIT2",
    )
    record = variant["record"]
    check_equal(assertions, "copy_all_nlink_one", record["all_nlink_one"], True)
    check_equal(assertions, "copy_no_symlinks", record["no_symlinks"], True)
    check_equal(assertions, "copy_no_reparse_points", record["no_reparse_points"], True)
    check_equal(assertions, "copy_no_file_id_collisions", record["no_file_id_collisions"], True)
    check_equal(assertions, "copy_manifests_equal", record["manifests_equal"], True)
    removed = variant["repo"] / MLIBRARY_REL
    sha_before = sha256_file(removed)
    removed.unlink()
    check(assertions, "source_removed", not removed.exists(), True, removed.exists())
    result = h.run_tool(
        variant["repo"],
        variant["out"] / "missing-source.json",
        "T3-missing-source",
        variant["tmp"],
    )
    meta = {
        "schema_version": SCHEMA_VERSION,
        "variant_id": "T3-missing-source",
        "kind": "input_error",
        "removed_file_rel": MLIBRARY_REL,
        "removed_file_sha256_before": sha_before,
        "exit": result["exit"],
        "stderr": result["stderr"],
        "report_written": result["report"] is not None,
        "tmp_dir": str(variant["tmp"]),
        "out_dir": str(variant["out"]),
        "copy_repo": variant["repo"].as_posix(),
    }
    write_json_artifact(h.attempt / "input-errors" / "T3-missing-source.json", meta)
    check_equal(assertions, "exit_is_input_error_2", result["exit"], 2)
    check(assertions, "exit_not_contract_red_1", result["exit"] != 1, True, result["exit"] != 1)
    check(assertions, "exit_not_green_0", result["exit"] != 0, True, result["exit"] != 0)
    check(assertions, "no_report_written", result["report"] is None, True, result["report"] is not None)
    check(
        assertions,
        "stderr_source_error",
        "SOURCE ERROR: missing required sources" in result["stderr"],
        True,
        result["stderr"],
    )
    check_equal(
        assertions,
        "copy_tool_still_frozen",
        sha256_file(variant["repo"] / TOOL_REL),
        EXPECTED_TOOL_SHA256,
    )
    check_equal(
        assertions,
        "copy_test_still_running_sha",
        sha256_file(variant["repo"] / TEST_REL),
        h.test_sha,
    )


# ---------------------------------------------------------------------------
# T4 input error: tampered expected matrix (missing / duplicated check id)
# ---------------------------------------------------------------------------


def _tamper_case(h, assertions, case_id, mutate, expected_stderr_fragment):
    variant = h.make_isolated(
        case_id,
        "input_error",
        "tamper the frozen expected matrix inside a physical copy: auditor must EXIT2",
    )
    record = variant["record"]
    check_equal(assertions, case_id + "_copy_all_nlink_one", record["all_nlink_one"], True)
    matrix_copy = variant["repo"] / MATRIX_REL
    original_sha = sha256_file(matrix_copy)
    document = read_json_file(matrix_copy)
    mutate(document)
    matrix_copy.write_bytes(
        (json.dumps(document, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    )
    tampered_sha = sha256_file(matrix_copy)
    check_equal(assertions, case_id + "_matrix_changed", tampered_sha != original_sha, True)
    result = h.run_tool(
        variant["repo"], variant["out"] / "tampered-matrix.json", case_id, variant["tmp"]
    )
    meta = {
        "schema_version": SCHEMA_VERSION,
        "variant_id": case_id,
        "kind": "input_error",
        "matrix_rel": MATRIX_REL,
        "matrix_sha256_frozen": original_sha,
        "matrix_sha256_tampered": tampered_sha,
        "tamper": "matrix structure changed as described in the test",
        "exit": result["exit"],
        "stderr": result["stderr"],
        "report_written": result["report"] is not None,
        "copy_repo": variant["repo"].as_posix(),
        "tmp_dir": str(variant["tmp"]),
    }
    write_json_artifact(h.attempt / "input-errors" / (case_id + ".json"), meta)
    check_equal(assertions, case_id + "_exit_is_2", result["exit"], 2)
    check(assertions, case_id + "_no_report", result["report"] is None, True, result["report"] is not None)
    check(
        assertions,
        case_id + "_stderr_expected",
        expected_stderr_fragment in result["stderr"],
        True,
        result["stderr"],
    )


def _matrix_drop_check(document):
    document["checks"] = [entry for entry in document["checks"] if entry["id"] != "G3"]


def _matrix_duplicate_check(document):
    duplicate = copy.deepcopy(
        [entry for entry in document["checks"] if entry["id"] == "G1"][0]
    )
    document["checks"].append(duplicate)


def test_t4_matrix_tamper(h, assertions):
    _tamper_case(
        h,
        assertions,
        "T4a-matrix-missing-check",
        _matrix_drop_check,
        "check ids must be exactly",
    )
    _tamper_case(
        h,
        assertions,
        "T4b-matrix-duplicate-check",
        _matrix_duplicate_check,
        "duplicate check id 'G1'",
    )


# ---------------------------------------------------------------------------
# T5..T8 / T8b semantic mutants on physical copies
# ---------------------------------------------------------------------------


def _first_mismatch(report, check_id):
    for item in report.get("checks", []):
        if item["id"] == check_id and item.get("mismatches"):
            return item["mismatches"][0]
    return None


def _failed_checks(report):
    return [item["id"] for item in report.get("checks", []) if not item["pass"]]


def _target_mismatch(report, check_id, expected_path):
    for item in report.get("checks", []):
        if item["id"] != check_id:
            continue
        for mismatch in item.get("mismatches", []):
            if mismatch.get("path") == expected_path:
                return mismatch
    return None


def run_mutation_test(h, assertions, spec):
    key = spec["key"]
    variant = h.make_isolated(
        key,
        spec["kind"],
        spec["semantic_point"],
    )
    record = variant["record"]
    repo = variant["repo"]
    check_equal(assertions, "copy_all_nlink_one", record["all_nlink_one"], True, key)
    check_equal(assertions, "copy_no_symlinks", record["no_symlinks"], True, key)
    check_equal(assertions, "copy_no_reparse_points", record["no_reparse_points"], True, key)
    check_equal(assertions, "copy_no_file_id_collisions", record["no_file_id_collisions"], True, key)
    check_equal(assertions, "copy_manifests_equal", record["manifests_equal"], True, key)
    check_equal(assertions, "copy_tool_matches_frozen", record["tool_matches_frozen"], True, key)
    check_equal(assertions, "copy_matrix_matches_frozen", record["matrix_matches_frozen"], True, key)
    check_equal(assertions, "copy_test_matches_running", record["test_matches_running_file"], True, key)

    source_path = repo / spec["file_rel"]
    source_abs = source_path.resolve()
    original_bytes = source_path.read_bytes()
    sha_before = sha256_bytes(original_bytes)
    frozen_sha = h.freeze_sha[spec["file_rel"]]
    check_equal(assertions, "source_sha_matches_frozen_matrix", sha_before, frozen_sha, key)

    mutated_bytes, apply_evidence = apply_snippet_mutation(original_bytes, spec["old"], spec["new"])
    sha_after = sha256_bytes(mutated_bytes)
    check_equal(assertions, "mutation_changed_bytes", sha_after != sha_before, True, key)
    source_path.write_bytes(mutated_bytes)
    # While mutated, exactly the one targeted copy file may differ from the
    # baseline snapshot of the copy (integrity of the physical isolation).
    changed_while_mutated, expected_changed = h.verify_copy_integrity(variant, [spec["file_rel"]])
    check_equal(
        assertions,
        "only_mutated_file_changed_while_mutated",
        changed_while_mutated,
        expected_changed,
        key,
    )
    # The mutation must not excuse the copy's auditor/test/matrix bytes: they stay
    # the frozen A-tool artifact and the running test file SHA throughout.
    check_equal(
        assertions,
        "copy_tool_still_frozen_after_mutation",
        sha256_file(repo / TOOL_REL),
        EXPECTED_TOOL_SHA256,
        key,
    )
    check_equal(
        assertions,
        "copy_test_still_running_sha_after_mutation",
        sha256_file(repo / TEST_REL),
        h.test_sha,
        key,
    )
    check_equal(
        assertions,
        "copy_matrix_still_frozen_after_mutation",
        sha256_file(repo / MATRIX_REL),
        EXPECTED_MATRIX_SHA256,
        key,
    )

    patch_text = build_patch_text(source_abs.as_posix(), original_bytes, mutated_bytes)
    mutation_dir = h.attempt / "mutations" / key
    patch_path = mutation_dir / "patch.diff"
    write_text_artifact(patch_path, patch_text)
    check(assertions, "patch_non_empty", bool(patch_text.strip()), True, bool(patch_text.strip()))

    mutated = h.run_tool(repo, variant["out"] / "report-mutated.json", key + "-mutated", variant["tmp"])
    write_text_artifact(mutation_dir / "mutated.stdout.txt", mutated["stdout"])
    write_text_artifact(mutation_dir / "mutated.stderr.txt", mutated["stderr"])
    mutated_report = mutated["report"]
    if mutated["out_path"].is_file():
        shutil.copyfile(mutated["out_path"], mutation_dir / "report-mutated.json")

    meta = {
        "schema_version": SCHEMA_VERSION,
        "run_key": key,
        "test": spec["test"],
        "kind": spec["kind"],
        "target_check": spec["target_check"],
        "expected_red_checks": spec["expected_red"],
        "semantic_point": spec["semantic_point"],
        "plan_ref": spec["plan_ref"],
        "collateral_note": spec.get("collateral_note"),
        "mutated_file_rel": spec["file_rel"],
        "mutated_file_abs": source_abs.as_posix(),
        "source_file_sha256_before": sha_before,
        "source_file_sha256_after": sha_after,
        "source_file_sha256_restored": None,
        "frozen_matrix_sha256_for_file": frozen_sha,
        "source_newline_style": apply_evidence["newline_style"],
        "anchor_evidence": apply_evidence,
        "patch_abs": patch_path.resolve().as_posix(),
        "patch_sha256": sha256_file(patch_path),
        "exit_mutated": mutated["exit"],
        "verdict_mutated": (mutated_report or {}).get("verdict"),
        "failed_checks_mutated": _failed_checks(mutated_report) if mutated_report else None,
        "target_check_failed": None,
        "first_mismatch": None,
        "other_checks_green": None,
        "restored_run": None,
        "out_dir": str(variant["out"]),
        "tmp_dir": str(variant["tmp"]),
    }
    write_json_artifact(mutation_dir / "mutation-meta.json", meta)

    check_equal(assertions, "mutated_exit_is_contract_red_1", mutated["exit"], 1, key)
    check(assertions, "mutated_exit_not_input_error_2", mutated["exit"] != 2, True, mutated["exit"] != 2)
    check(assertions, "mutated_exit_not_green_0", mutated["exit"] != 0, True, mutated["exit"] != 0)
    check(assertions, "mutated_report_written", mutated_report is not None, True, mutated_report is not None)
    check_equal(assertions, "mutated_stderr_empty", mutated["stderr"], "", key)
    check_equal(assertions, "mutated_checks_total", mutated_report.get("summary", {}).get("checks_total"), 5, key)
    failed_checks = _failed_checks(mutated_report)
    check_equal(assertions, "failed_checks", failed_checks, spec["expected_red"], key)
    per_check = {item["id"]: item for item in mutated_report["checks"]}
    check_equal(
        assertions,
        "target_check_fails",
        per_check[spec["target_check"]]["pass"],
        False,
        key,
    )
    target_mismatch = _target_mismatch(mutated_report, spec["target_check"], spec["first_mismatch"])
    check(
        assertions,
        "target_first_mismatch_present",
        target_mismatch is not None,
        True,
        target_mismatch,
    )
    check_equal(
        assertions,
        "target_mismatch_path",
        target_mismatch.get("path"),
        spec["first_mismatch"],
        key + ":" + spec["target_check"],
    )
    global_first_check = failed_checks[0] if failed_checks else None
    first_mismatch = _first_mismatch(mutated_report, global_first_check) if global_first_check else None
    check(
        assertions,
        "first_failure_recorded",
        first_mismatch is not None,
        True,
        first_mismatch,
    )
    other_checks = {item["id"]: item["pass"] for item in mutated_report["checks"]}
    green_others = sorted(
        check_id
        for check_id, passed in other_checks.items()
        if passed and check_id != spec["target_check"]
    )
    check_equal(
        assertions,
        "non_red_checks_green",
        sorted(
            check_id
            for check_id, passed in other_checks.items()
            if passed
        ),
        sorted([check_id for check_id in CHECK_ORDER if check_id not in spec["expected_red"]]),
        key,
    )
    meta.update(
        {
            "target_check_failed": not per_check[spec["target_check"]]["pass"],
            "first_mismatch": first_mismatch,
            "first_failed_check": global_first_check,
            "target_mismatch": target_mismatch,
            "other_checks_green": green_others,
        }
    )
    write_json_artifact(mutation_dir / "mutation-meta.json", meta)

    # Restore the physical copy and prove the same auditor is green again.
    source_path.write_bytes(original_bytes)
    sha_restored = sha256_file(source_path)
    check_equal(assertions, "restored_sha_equals_before", sha_restored, sha_before, key)
    restored = h.run_tool(repo, variant["out"] / "report-restored.json", key + "-restored", variant["tmp"])
    write_text_artifact(mutation_dir / "restored.stdout.txt", restored["stdout"])
    write_text_artifact(mutation_dir / "restored.stderr.txt", restored["stderr"])
    if restored["out_path"].is_file():
        shutil.copyfile(restored["out_path"], mutation_dir / "report-restored.json")
    restored_report = restored["report"]
    check_equal(assertions, "restored_exit_0", restored["exit"], 0, key)
    check_equal(assertions, "restored_verdict", (restored_report or {}).get("verdict"), "PASS", key)
    check_equal(
        assertions,
        "restored_checks_passed",
        (restored_report or {}).get("summary", {}).get("checks_passed"),
        5,
        key,
    )
    check_equal(
        assertions,
        "restored_stderr_empty",
        restored["stderr"],
        "",
        key,
    )
    # Strong green-restore equality: only the report root path differs from T1.
    check_equal(
        assertions,
        "restored_checks_identical_to_real_tree",
        (restored_report or {}).get("checks"),
        h.green_report.get("checks"),
        key,
    )
    check_equal(
        assertions,
        "restored_source_files_identical_to_real_tree",
        (restored_report or {}).get("source_files"),
        h.green_report.get("source_files"),
        key,
    )
    changed, expected_changed = h.verify_copy_integrity(variant, [])
    check_equal(assertions, "copy_restored_to_baseline", changed, expected_changed, key)
    meta.update(
        {
            "source_file_sha256_restored": sha_restored,
            "restored_run": {
                "exit": restored["exit"],
                "verdict": restored_report.get("verdict"),
                "checks_passed": restored_report["summary"]["checks_passed"],
                "report_sha256": restored["report_sha256"],
            },
            "changed_files_while_mutated": changed_while_mutated,
            "changed_files_after_restore": changed,
            "artifact_abs": {
                "patch": patch_path.resolve().as_posix(),
                "mutated_report": (mutation_dir / "report-mutated.json").resolve().as_posix(),
                "restored_report": (mutation_dir / "report-restored.json").resolve().as_posix(),
            },
        }
    )
    write_json_artifact(mutation_dir / "mutation-meta.json", meta)
    h.mutation_records.append(
        {
            "key": key,
            "test": spec["test"],
            "kind": spec["kind"],
            "target_check": spec["target_check"],
            "expected_red": spec["expected_red"],
            "mutated_file_rel": spec["file_rel"],
            "sha256_before": sha_before,
            "sha256_after": sha_after,
            "sha256_restored": sha_restored,
            "exit_mutated": mutated["exit"],
            "failed_checks_mutated": failed_checks,
            "first_failed_check": global_first_check,
            "first_failure": first_mismatch,
            "target_check_failed": not per_check[spec["target_check"]]["pass"],
            "target_mismatch": target_mismatch,
            "other_checks_green": green_others,
            "restored_run": meta["restored_run"],
            "patch_rel": (mutation_dir / "patch.diff").relative_to(h.root).as_posix(),
            "meta_rel": (mutation_dir / "mutation-meta.json").relative_to(h.root).as_posix(),
            "collateral_note": spec.get("collateral_note"),
        }
    )


def test_t5_mutation_r1_g2_offset_order(h, assertions):
    run_mutation_test(h, assertions, MUTATION_SPECS[0])


def test_t6_mutation_r2_g3_restore_and_factors(h, assertions):
    run_mutation_test(h, assertions, MUTATION_SPECS[1])


def test_t7_mutation_r3_g4_mask_coordinate(h, assertions):
    run_mutation_test(h, assertions, MUTATION_SPECS[2])


def test_t8_mutation_r4_g5_shadow_read(h, assertions):
    run_mutation_test(h, assertions, MUTATION_SPECS[3])


def test_t8b_mutation_r4b_g5_shadow_deserialize_removed(h, assertions):
    run_mutation_test(h, assertions, MUTATION_SPECS[4])


# ---------------------------------------------------------------------------
# T9 no production change (static assertions)
# ---------------------------------------------------------------------------


def test_t9_no_production_change(h, assertions):
    git = shutil.which("git")
    check(assertions, "git_available", git is not None, True, git)
    head = h.run([git, "-C", str(h.root), "rev-parse", "HEAD"], "T9-git-rev-parse")
    check_equal(assertions, "rev_parse_exit", head["exit"], 0)
    # HEAD is informational only.  Governance/docs/tool commits legitimately advance
    # HEAD after delivery, so "no production change" cannot mean "HEAD == baseline";
    # it means the production code tree must be byte-identical to the frozen baseline
    # commit.  BASELINE_HEAD remains the frozen acceptance anchor and is never edited.
    head_sha = head["stdout"].strip()
    check(
        assertions,
        "head_recorded",
        len(head_sha) == 40 and all(char in "0123456789abcdef" for char in head_sha),
        True,
        {"head": head_sha, "baseline_head": BASELINE_HEAD},
    )
    crates_diff = h.run(
        [git, "-C", str(h.root), "diff", "--name-only", BASELINE_HEAD, "--", "crates"],
        "T9-git-diff-crates",
    )
    check_equal(assertions, "crates_diff_exit", crates_diff["exit"], 0)
    check_equal(
        assertions,
        "crates_unchanged_since_baseline",
        crates_diff["stdout"].splitlines(),
        [],
    )
    diff_names = h.run([git, "-C", str(h.root), "diff", "--name-only", "HEAD"], "T9-git-diff-names")
    check_equal(assertions, "git_diff_exit", diff_names["exit"], 0)
    check_equal(assertions, "no_tracked_modifications", diff_names["stdout"].strip(), "")
    diff_check = h.run([git, "-C", str(h.root), "diff", "--check"], "T9-git-diff-check")
    check_equal(assertions, "git_diff_check_exit", diff_check["exit"], 0)
    check_equal(assertions, "git_diff_check_clean", diff_check["stdout"].strip(), "")
    status = h.run([git, "-C", str(h.root), "status", "--porcelain"], "T9-git-status")
    check_equal(assertions, "git_status_exit", status["exit"], 0)
    tracked_change_lines = [
        line
        for line in status["stdout"].splitlines()
        if line[:2] not in ("??",) and line.strip()
    ]
    check_equal(assertions, "only_untracked_additions_or_ignored", tracked_change_lines, [])
    freeze_mismatches = []
    for entry in h.matrix["freeze"]["source_files"]:
        path = h.root / entry["path"]
        actual = sha256_file(path) if path.is_file() else None
        if actual != entry["sha256"]:
            freeze_mismatches.append(
                {"path": entry["path"], "expected": entry["sha256"], "actual": actual}
            )
    check_equal(assertions, "frozen_source_files_unchanged", freeze_mismatches, [])
    check_equal(assertions, "frozen_tool_sha", h.tool_sha, EXPECTED_TOOL_SHA256)
    check_equal(assertions, "frozen_matrix_sha", h.matrix_sha, EXPECTED_MATRIX_SHA256)
    check(
        assertions,
        "test_sha_recorded",
        isinstance(h.test_sha, str) and len(h.test_sha) == 64,
        True,
        h.test_sha,
    )
    check_equal(assertions, "current_test_sha", sha256_file(Path(__file__).resolve()), h.test_sha)


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------


TEST_SEQUENCE = [
    ("t1_green_real_tree", "baseline", test_t1_green_real_tree),
    ("g1_draw_offset_order", "semantic", test_g1_draw_offset_order),
    ("g2_blend_offset_order", "semantic", test_g2_blend_offset_order),
    ("g3_restore_and_factors", "semantic", test_g3_restore_and_factors),
    ("g4_mask_coordinate", "semantic", test_g4_mask_coordinate),
    ("g5_shadow_usage_inventory", "semantic", test_g5_shadow_usage_inventory),
    ("t2_determinism", "determinism", test_t2_determinism),
    ("t3_input_error_missing_source", "input_error", test_t3_input_error_missing_source),
    ("t4_matrix_tamper", "input_error", test_t4_matrix_tamper),
    ("t5_mutation_r1_g2_offset_order", "semantic_mutation", test_t5_mutation_r1_g2_offset_order),
    ("t6_mutation_r2_g3_restore_and_factors", "semantic_mutation", test_t6_mutation_r2_g3_restore_and_factors),
    ("t7_mutation_r3_g4_mask_coordinate", "semantic_mutation", test_t7_mutation_r3_g4_mask_coordinate),
    ("t8_mutation_r4_g5_shadow_read", "semantic_mutation", test_t8_mutation_r4_g5_shadow_read),
    ("t8b_mutation_r4b_g5_shadow_deserialize_removed", "supplementary_semantic_mutation", test_t8b_mutation_r4b_g5_shadow_deserialize_removed),
    ("t9_no_production_change", "static", test_t9_no_production_change),
]


def main(argv=None):
    parser = argparse.ArgumentParser(
        prog="round69-contract-tests.py",
        description="R69-B render-contract integration tests with physical-isolation mutants.",
    )
    parser.add_argument("--root", default=".", help="repository root (default: .)")
    parser.add_argument(
        "--work-root",
        default=None,
        help=(
            "evidence directory root, resolved against --root when relative; "
            "isolated copies/tmp/out all live under it (default: %s)"
            % DEFAULT_WORK_ROOT_REL
        ),
    )
    parser.add_argument(
        "--out",
        default=DEFAULT_OUT_REL,
        help=(
            "machine-readable test result JSON; must stay inside --work-root "
            "(default: %s)" % DEFAULT_OUT_REL
        ),
    )
    args = parser.parse_args(argv)

    print("round69 render-contract integration tests (R69-B)")
    print("root: %s" % Path(args.root).resolve())
    harness = None
    try:
        harness = Harness(args.root, args.out, args.work_root)
        harness.preflight()
    except HarnessError as exc:
        print("INPUT ERROR: %s" % exc, file=sys.stderr)
        if harness is not None:
            failure = {
                "schema_version": SCHEMA_VERSION,
                "suite": SUITE,
                "round": ROUND,
                "task": TASK,
                "result": "INPUT_ERROR",
                "preflight_error": str(exc),
                "counts": {"total": 0, "passed": 0, "failed": 1, "skipped": 0},
                "tests": [],
            }
            try:
                write_json_artifact(harness.out, failure)
            except OSError as write_exc:  # pragma: no cover - best effort
                print("WARN: could not write failure JSON: %s" % write_exc, file=sys.stderr)
        return 2

    try:
        for name, kind, function in TEST_SEQUENCE:
            harness.execute_test(name, kind, function)
        result = harness.write_results()
    finally:
        # runs.json is written by write_results on success; preserve a partial run
        # record for first-failure preservation if the harness itself blew up.
        if not (harness.attempt / "runs.json").is_file():
            write_json_artifact(
                harness.attempt / "runs.json",
                {
                    "schema_version": SCHEMA_VERSION,
                    "suite": SUITE,
                    "task": TASK,
                    "harness_argv": sys.argv,
                    "root": harness.root.as_posix(),
                    "partial": True,
                    "runs": harness.runs,
                },
            )

    counts = result["counts"]
    print(
        "counts: total=%d passed=%d failed=%d skipped=%d result=%s"
        % (counts["total"], counts["passed"], counts["failed"], counts["skipped"], result["result"])
    )
    print("wrote: %s" % harness.out.as_posix())
    if result["result"] == "PASS" and counts["total"] >= 7 and counts["skipped"] == 0:
        return 0
    return 1


if __name__ == "__main__":
    sys.exit(main())
