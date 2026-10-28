//! R77-A（JJC-20261028-004-sub-2a7d07c6）：Trap 素材准入合同 G1–G7。
//!
//! 本文件只读：直接解析 WZX/WZL/MLibrary 头部，调用只读审计工具
//! `tools/testing/round77_asset_admission.py` 取 zlib/gzip 解压长度与 SHA
//! （Python 只读、不写素材），并实读现行 Rust 行为与 R76/R59 冻结文本。
//!
//! 状态机：`NOT_READY` / `UNVERIFIED_SEMANTICS` / `AWAITING_BINDING` / `READY`；
//! `READY` 只能由人工 `--human-approve` 显式路径产生，审计 EXIT0 **不等于** READY。
//!
//! 本文件由 R77-A 新增，不修改任何既有文件；红/绿证据使用同一份本文件 SHA。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use serde_json::Value;

use mir_core::enums::Spell;

const TOOL_REL: &str = "tools/testing/round77_asset_admission.py";
const PYTHON_REL: &str = ".dsh/dsh-runtimes/dsh-primary-runtime/dependencies/python/python.exe";
const SPELLFX_REL: &str = "crates/mir-client/src/ui/spellfx.rs";
const CRYSTAL_SPELLOBJECT_REL: &str = "github/Crystal/Client/MirObjects/SpellObject.cs";
const R76_TEST_REL: &str = "crates/mir-client/tests/round76_trap_hexagon_mapping.rs";
const R76_DOC_REL: &str = "docs/testing/round76/round76-trap-contract.md";
const R76_MANIFEST_REL: &str = "docs/testing/round76/final-manifest.json";
const R72_TEST_REL: &str = "crates/mir-client/tests/round72_poison_cloud_mapping.rs";
const R59_TEST_REL: &str = "crates/mir-client/tests/round59_projectile_assets.rs";

const TRAP_FRAMES: [u32; 8] = [2360, 2361, 2362, 2363, 2364, 2365, 2366, 2367];

const DEFAULT_WZX_BYTES: usize = 5_968;
const DEFAULT_WZX_SHA: &str = "62559cdd1626b26c2566fe2922e8614dbc6d1896f9fb29584a516e3706addea3";
const DEFAULT_WZL_BYTES: usize = 10_290_606;
const DEFAULT_WZL_SHA: &str = "21f46eea925c7a498961dfdabe835433e41267fa5468a99011c9c018dc8a9bd1";
const DEFAULT_DECLARED: u64 = 1_460;
const DEFAULT_SLOTS: u64 = 1_480;
const DEFAULT_MAX_FRAME_INDEX: u64 = 1_479;
const DEFAULT_UNINDEXED_GAP_TOTAL: u64 = 2_637_986;
const DEFAULT_UNINDEXED_GAP_COUNT: usize = 5;
const DEFAULT_UNINDEXED_LARGEST_GAP: u64 = 2_437_042;
const CORPUS_GROUP_START: u64 = 13_316_944;
const CORPUS_GROUP_BEYOND_DEFAULT_WZL: u64 = 3_026_338;

const CORPUS_WZX_BYTES: usize = 10_368;
const CORPUS_WZX_SHA: &str = "fe07563ac2f486ad12bc149365ac7bbeba2af002f2981f5a05e6a0a7f990bd54";
const CORPUS_WZX_DECLARED: u64 = 2_580;
const CORPUS_WZL_BYTES: usize = 14_033_479;
const CORPUS_WZL_SHA: &str = "8e5edc62b761418760b6de64b99625157fe5344f8502bdb7e258a293f010f66d";
const CORPUS_WZL_UNINDEXED_GAP: u64 = 2_910_120;
const CORPUS_GROUP_END: u64 = 13_357_626;
const CORPUS_GROUP_COMPRESSED_BYTES: u64 = 40_682;

const LIB_BYTES: usize = 21_091_901;
const LIB_SHA: &str = "f449aa4c96632f13454d7746ab47678c06175be02e69a6dc8fdfd036755fc0d2";
const LIB_VERSION: u64 = 2;
const LIB_COUNT: u64 = 2_790;
const LIB_INDEX_TABLE_END: u64 = 11_168;

const R76_MANIFEST_SCHEMA: &str = "round76-final-manifest-v1";
const R76_MANIFEST_TASK: &str = "JJC-20261028-003";
const R59_F116_FINGERPRINT: &str =
    "e642330a568402b21528afb04459108f3dc596f7e37ad5c276670ef342940eff";
const R59_F106_FINGERPRINT: &str =
    "e465026a50ee6f86d6762e3959386fa0132ffabf63da9c92afeebdf3d52a1e98";

const SPELLFX_SHA: &str = "84fdb324504d0bffc0638afb35330021defd8acf9a849d8100ab1f1ca3c8e0f1";
const R76_TEST_SHA: &str = "8b680973a30193113c65e6cc936cccd6cfc8ac33941dbff43d4867419f3562d0";
const R72_TEST_SHA: &str = "3a9f42bd67a8e57e02d86fb970ad6347687d46991b313c3c7fb2316db97b4bfb";
const R59_TEST_SHA: &str = "849770b254ccadeef5e8fce05a5ffd2b01ab67e81e209bbab7457d52faf5fe2a";

/// 2025 语料八帧的独立实测（本角色从原始素材解析，非复制 B 数值）。
#[derive(Clone, Copy)]
struct CorpusFrameSpec {
    frame: u32,
    width: u16,
    height: u16,
    compressed_len: usize,
    data_offset: usize,
    decompressed_len: usize,
    decompressed_sha: &'static str,
    record_start: usize,
    record_end: usize,
    format: u8,
    offset_x: i16,
    offset_y: i16,
}

const CORPUS_FRAMES: [CorpusFrameSpec; 8] = [
    CorpusFrameSpec {
        frame: 2360,
        width: 158,
        height: 127,
        compressed_len: 5_465,
        data_offset: 13_316_960,
        decompressed_len: 40_132,
        decompressed_sha: "df8c8f7656953b7a314bcba79b607f36b29d2e8be167ba6fbd9a6dcad8bbbaeb",
        record_start: 13_316_944,
        record_end: 13_322_425,
        format: 5,
        offset_x: -85,
        offset_y: -100,
    },
    CorpusFrameSpec {
        frame: 2361,
        width: 176,
        height: 101,
        compressed_len: 5_976,
        data_offset: 13_322_441,
        decompressed_len: 35_552,
        decompressed_sha: "aa83222606c9643b5e7040422a5b43526902195f1b94bd4eeba72f46028c17b2",
        record_start: 13_322_425,
        record_end: 13_328_417,
        format: 5,
        offset_x: -82,
        offset_y: -75,
    },
    CorpusFrameSpec {
        frame: 2362,
        width: 148,
        height: 130,
        compressed_len: 6_159,
        data_offset: 13_328_433,
        decompressed_len: 38_480,
        decompressed_sha: "6811522618926aad421657e7cec2e18d13c430a9ab9166c404b44e9af57719b8",
        record_start: 13_328_417,
        record_end: 13_334_592,
        format: 5,
        offset_x: -66,
        offset_y: -108,
    },
    CorpusFrameSpec {
        frame: 2363,
        width: 150,
        height: 121,
        compressed_len: 5_792,
        data_offset: 13_334_608,
        decompressed_len: 36_300,
        decompressed_sha: "297dea5737e029b5b92cb765f787d58b8401ec8d07045485596677adceb59d2c",
        record_start: 13_334_592,
        record_end: 13_340_400,
        format: 5,
        offset_x: -69,
        offset_y: -103,
    },
    CorpusFrameSpec {
        frame: 2364,
        width: 174,
        height: 98,
        compressed_len: 5_646,
        data_offset: 13_340_416,
        decompressed_len: 34_104,
        decompressed_sha: "15eb89adae59d6087cd22626485242febbc8fa06e1bc8b57b11795992236ad64",
        record_start: 13_340_400,
        record_end: 13_346_062,
        format: 5,
        offset_x: -97,
        offset_y: -101,
    },
    CorpusFrameSpec {
        frame: 2365,
        width: 150,
        height: 149,
        compressed_len: 6_780,
        data_offset: 13_346_078,
        decompressed_len: 44_700,
        decompressed_sha: "89012bca4abf02152c17d1645f95589d46de14e2c7dcf1963b20382f60c0d6d3",
        record_start: 13_346_062,
        record_end: 13_352_858,
        format: 5,
        offset_x: -82,
        offset_y: -97,
    },
    CorpusFrameSpec {
        frame: 2366,
        width: 108,
        height: 116,
        compressed_len: 3_581,
        data_offset: 13_352_874,
        decompressed_len: 25_056,
        decompressed_sha: "21f07e2ca358846b2eaa81431de6d7d7c5fba4db0449d9fa0f418f5d5b8b490c",
        record_start: 13_352_858,
        record_end: 13_356_455,
        format: 5,
        offset_x: -78,
        offset_y: -83,
    },
    CorpusFrameSpec {
        frame: 2367,
        width: 66,
        height: 48,
        compressed_len: 1_155,
        data_offset: 13_356_471,
        decompressed_len: 6_336,
        decompressed_sha: "0e7804aaf2526f55ff7b5cfe1d549ab9f81ef9574caf55b408728ece46140de1",
        record_start: 13_356_455,
        record_end: 13_357_626,
        format: 5,
        offset_x: -38,
        offset_y: -16,
    },
];

/// Magic2.Lib 两个候选八帧的独立实测。
#[derive(Clone, Copy)]
struct LibFrameSpec {
    frame: u32,
    width: u16,
    height: u16,
    compressed_len: usize,
    data_offset: usize,
    decompressed_len: usize,
    decompressed_sha: &'static str,
    record_start: usize,
    record_end: usize,
}

const LIB_FRAMES: [LibFrameSpec; 8] = [
    LibFrameSpec {
        frame: 2360,
        width: 396,
        height: 363,
        compressed_len: 12_909,
        data_offset: 16_533_867,
        decompressed_len: 574_992,
        decompressed_sha: "961bc7acdadb53755b8cef97ca61a857eec5b53aaef51d7722b21467266e0e32",
        record_start: 16_533_850,
        record_end: 16_546_776,
    },
    LibFrameSpec {
        frame: 2361,
        width: 432,
        height: 363,
        compressed_len: 15_496,
        data_offset: 16_546_793,
        decompressed_len: 627_264,
        decompressed_sha: "ba57c1a7f9178e34249709235284433124e7b520e90afbe44930e794e6c24bc9",
        record_start: 16_546_776,
        record_end: 16_562_289,
    },
    LibFrameSpec {
        frame: 2362,
        width: 428,
        height: 363,
        compressed_len: 15_501,
        data_offset: 16_562_306,
        decompressed_len: 621_456,
        decompressed_sha: "200ffe8cb2c0aa1ab76755d781126139d766ab41fb7bcf688208296bc6c0b54c",
        record_start: 16_562_289,
        record_end: 16_577_807,
    },
    LibFrameSpec {
        frame: 2363,
        width: 424,
        height: 363,
        compressed_len: 15_477,
        data_offset: 16_577_824,
        decompressed_len: 615_648,
        decompressed_sha: "f32c49285bdc60f2089808cc6bef4260c630864b7b65f5a4c94efe10cba2f327",
        record_start: 16_577_807,
        record_end: 16_593_301,
    },
    LibFrameSpec {
        frame: 2364,
        width: 416,
        height: 363,
        compressed_len: 14_729,
        data_offset: 16_593_318,
        decompressed_len: 604_032,
        decompressed_sha: "6e62fdedd716442e436418dc970f206f4ee72d4bb29cd3454e2f2d5ff297ca82",
        record_start: 16_593_301,
        record_end: 16_608_047,
    },
    LibFrameSpec {
        frame: 2365,
        width: 412,
        height: 363,
        compressed_len: 14_198,
        data_offset: 16_608_064,
        decompressed_len: 598_224,
        decompressed_sha: "d25d6c66494a174b5da9ee8fdc0ed7c65470de0c5d20fcd65a41d00c47d49292",
        record_start: 16_608_047,
        record_end: 16_622_262,
    },
    LibFrameSpec {
        frame: 2366,
        width: 404,
        height: 363,
        compressed_len: 13_318,
        data_offset: 16_622_279,
        decompressed_len: 586_608,
        decompressed_sha: "5faf284d8b31bb2c5455f3b030665c8beb01f373cc525242fe1b5cdfe4aee6f9",
        record_start: 16_622_262,
        record_end: 16_635_597,
    },
    LibFrameSpec {
        frame: 2367,
        width: 400,
        height: 363,
        compressed_len: 12_975,
        data_offset: 16_635_614,
        decompressed_len: 580_800,
        decompressed_sha: "f2e32154631f04c9f67228506d3012a56f78e7068e5db903bf9e8d5b8dfa6aec",
        record_start: 16_635_597,
        record_end: 16_648_589,
    },
];

const LIB_NEIGHBOR_FRAMES: [u32; 5] = [2357, 2358, 2359, 2368, 2369];
const LIB_NEIGHBOR_COMPRESSED_LEN: usize = 23;
const LIB_NEIGHBOR_DECOMPRESSED_LEN: u64 = 16;

/// G5 内联差异表：字段 / 当前 Rust 行为 / Crystal 目标 / 状态。
///
/// current 列必须等于 [`rust_matrix_values`] 从 `spellfx.rs` 独立解析的结果，
/// target 列必须等于 [`crystal_matrix_values`] 从 Crystal `SpellObject.cs` 解析的结果；
/// 二者与工具 `--matrix` 输出三方互证（F1 返工，矩阵不是摆设）。
const CRYSTAL_DIFF_MATRIX: [(&str, &str, &str, &str); 7] = [
    ("library", "Magic", "Magic2", "BLOCKED_BY_ASSET_ADMISSION"),
    ("index_range", "0..9", "2360..2367", "DIFF_TARGET_REQUIRED"),
    ("frame_count", "10", "8", "DIFF_TARGET_REQUIRED"),
    ("interval_ms", "60", "100", "DIFF_TARGET_REQUIRED"),
    ("duration_ms", "700", "700", "SAME"),
    ("blend", "Over", "Blend(true, 0.8F)", "DIFF_TARGET_REQUIRED"),
    (
        "anchor",
        "centered (CELL_WIDTH/CELL_HEIGHT)",
        "DrawLocation (CurrentLocation*CellWidth/CellHeight)",
        "PARTIAL",
    ),
];

fn repo_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("crates/mir-client 的上级目录应存在")
        .to_path_buf()
}

fn read_bytes(rel: &str) -> Vec<u8> {
    let path = repo_root().join(rel);
    std::fs::read(&path).unwrap_or_else(|err| panic!("R77-A 读取失败 {}: {err}", path.display()))
}

fn read_text(rel: &str) -> String {
    let path = repo_root().join(rel);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|err| panic!("R77-A 读取失败 {}: {err}", path.display()));
    String::from_utf8(bytes).unwrap_or_else(|_| panic!("R77-A 文件非 UTF-8: {}", path.display()))
}

const fn le_u16(data: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([data[off], data[off + 1]])
}

const fn le_i16(data: &[u8], off: usize) -> i16 {
    i16::from_le_bytes([data[off], data[off + 1]])
}

const fn le_u32(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
}

const fn le_i32(data: &[u8], off: usize) -> i32 {
    i32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
}

/// 16bpp RGB565 行 4 字节对齐解压长度；**不是** `w*h*4`。
const fn rgb565_rowaligned_len(width: u16, height: u16) -> usize {
    ((((width as usize) * 16 + 31) >> 5) * 4) * (height as usize)
}

// ---------------------------------------------------------------------------
// G5 差异矩阵：从生产源码独立解析 current / target 两列（F1 返工）
// ---------------------------------------------------------------------------
const MATRIX_FIELDS: [&str; 7] = [
    "library",
    "index_range",
    "frame_count",
    "interval_ms",
    "duration_ms",
    "blend",
    "anchor",
];

/// spellfx.rs 通用（非 149）Trap 路径实测值。
#[derive(Debug)]
struct RustMatrixValues {
    library: String,
    frames: u32,
    frame_ms: u32,
    cast_ms: u32,
    blend: String,
    anchor: String,
}

/// Crystal `SpellObject.cs:132-138` 目标值。
#[derive(Debug)]
struct CrystalMatrixValues {
    library: String,
    base: u32,
    count: u32,
    interval: u32,
    blend_alpha: String,
}

/// 由解析值生成的期望矩阵行（与内联表逐字段相等）。
#[derive(Debug)]
struct MatrixRow {
    field: &'static str,
    current: String,
    target: String,
    status: &'static str,
}

fn parse_i64_const(text: &str, name: &str) -> i64 {
    let prefix = format!("pub const {name}: i64 = ");
    let start = text
        .find(&prefix)
        .unwrap_or_else(|| panic!("G5：spellfx.rs 缺常量 {name}"))
        + prefix.len();
    let end = text[start..]
        .find(';')
        .unwrap_or_else(|| panic!("G5：常量 {name} 未以分号结束"))
        + start;
    text[start..end]
        .trim()
        .replace('_', "")
        .parse()
        .unwrap_or_else(|err| panic!("G5：常量 {name} 解析失败: {err}"))
}

fn parse_u32_assign(text: &str, name: &str) -> u32 {
    let prefix = format!("{name} = ");
    let start = text
        .find(&prefix)
        .unwrap_or_else(|| panic!("G5：Crystal 缺 {name} 赋值"))
        + prefix.len();
    let end = text[start..]
        .find(';')
        .unwrap_or_else(|| panic!("G5：Crystal {name} 赋值未以分号结束"))
        + start;
    text[start..end]
        .trim()
        .replace('_', "")
        .parse()
        .unwrap_or_else(|err| panic!("G5：Crystal {name} 解析失败: {err}"))
}

fn rust_matrix_values(spellfx: &str) -> RustMatrixValues {
    let frames = u32::try_from(parse_i64_const(spellfx, "MAGIC_FX_FRAMES"))
        .expect("G5：MAGIC_FX_FRAMES 应为正数");
    let frame_ms = u32::try_from(parse_i64_const(spellfx, "MAGIC_FX_FRAME_MS"))
        .expect("G5：MAGIC_FX_FRAME_MS 应为正数");
    let cast_ms = u32::try_from(parse_i64_const(spellfx, "CAST_MS")).expect("G5：CAST_MS 应为正数");
    assert!(
        spellfx.contains("`magic.wzl` 精灵"),
        "G5：通用命中路径应使用 magic.wzl 精灵"
    );
    assert!(
        spellfx.contains("scene.draw_effect(frame, kind, frame_index, x, y);"),
        "G5：通用命中路径应走 draw_effect（Over）而非 Blend"
    );
    assert!(
        spellfx.contains("mir_render::CELL_WIDTH") && spellfx.contains("mir_render::CELL_HEIGHT"),
        "G5：通用命中路径应居中到格锚点"
    );
    RustMatrixValues {
        library: "Magic".to_owned(),
        frames,
        frame_ms,
        cast_ms,
        blend: "Over".to_owned(),
        anchor: "centered (CELL_WIDTH/CELL_HEIGHT)".to_owned(),
    }
}

fn crystal_matrix_values(crystal: &str) -> CrystalMatrixValues {
    let start = crystal
        .find("case Spell.Trap:")
        .expect("G5：Crystal SpellObject.cs 缺 case Spell.Trap:");
    let block = &crystal[start..crystal.len().min(start + 400)];
    assert!(
        block.contains("BodyLibrary = Libraries.Magic2;"),
        "G5：Crystal Trap 应绑定 Magic2"
    );
    assert!(
        block.contains("Blend = true;"),
        "G5：Crystal Trap 应 Blend = true"
    );
    let draw_start = crystal
        .find("public override void Draw()")
        .expect("G5：Crystal SpellObject.cs 缺 Draw()");
    let draw_block = &crystal[draw_start..crystal.len().min(draw_start + 800)];
    assert!(
        draw_block.contains("BodyLibrary.DrawBlend(") && draw_block.contains("0.8F"),
        "G5：Crystal Draw() 缺 DrawBlend(..., 0.8F) 调光"
    );
    assert!(
        crystal.contains("MapControl.CellWidth") && crystal.contains("DrawLocation"),
        "G5：Crystal 应使用 DrawLocation 点锚点"
    );
    CrystalMatrixValues {
        library: "Magic2".to_owned(),
        base: parse_u32_assign(block, "DrawFrame"),
        count: parse_u32_assign(block, "FrameCount"),
        interval: parse_u32_assign(block, "FrameInterval"),
        blend_alpha: "0.8F".to_owned(),
    }
}

fn derived_matrix_rows(rust: &RustMatrixValues, crystal: &CrystalMatrixValues) -> Vec<MatrixRow> {
    let last_index = crystal.base + crystal.count - 1;
    vec![
        MatrixRow {
            field: "library",
            current: rust.library.clone(),
            target: crystal.library.clone(),
            status: "BLOCKED_BY_ASSET_ADMISSION",
        },
        MatrixRow {
            field: "index_range",
            current: format!("0..{}", rust.frames - 1),
            target: format!("{}..{last_index}", crystal.base),
            status: "DIFF_TARGET_REQUIRED",
        },
        MatrixRow {
            field: "frame_count",
            current: rust.frames.to_string(),
            target: crystal.count.to_string(),
            status: "DIFF_TARGET_REQUIRED",
        },
        MatrixRow {
            field: "interval_ms",
            current: rust.frame_ms.to_string(),
            target: crystal.interval.to_string(),
            status: "DIFF_TARGET_REQUIRED",
        },
        MatrixRow {
            field: "duration_ms",
            current: rust.cast_ms.to_string(),
            target: rust.cast_ms.to_string(),
            status: "SAME",
        },
        MatrixRow {
            field: "blend",
            current: rust.blend.clone(),
            target: format!("Blend(true, {})", crystal.blend_alpha),
            status: "DIFF_TARGET_REQUIRED",
        },
        MatrixRow {
            field: "anchor",
            current: rust.anchor.clone(),
            target: "DrawLocation (CurrentLocation*CellWidth/CellHeight)".to_owned(),
            status: "PARTIAL",
        },
    ]
}

fn parse_wzx(bytes: &[u8]) -> (u32, Vec<u32>) {
    assert!(bytes.len() >= 48, "wzx 头部应不小于 48B");
    let declared = le_u32(bytes, 44);
    let slots = (bytes.len() - 48) / 4;
    let offsets = (0..slots)
        .map(|index| le_u32(bytes, 48 + 4 * index))
        .collect();
    (declared, offsets)
}

#[derive(Clone, Copy)]
struct WzlRecord {
    format: u8,
    width: u16,
    height: u16,
    offset_x: i16,
    offset_y: i16,
    compressed_len: usize,
    record_start: usize,
    data_offset: usize,
    record_end: usize,
}

fn parse_wzl_record(bytes: &[u8], off: usize) -> WzlRecord {
    assert!(off + 16 <= bytes.len(), "WZL 记录头越界: off={off}");
    let compressed_len = le_u32(bytes, off + 12) as usize;
    WzlRecord {
        format: bytes[off],
        width: le_u16(bytes, off + 4),
        height: le_u16(bytes, off + 6),
        offset_x: le_i16(bytes, off + 8),
        offset_y: le_i16(bytes, off + 10),
        compressed_len,
        record_start: off,
        data_offset: off + 16,
        record_end: off + 16 + compressed_len,
    }
}

#[derive(Clone, Copy)]
struct LibRecord {
    width: u16,
    height: u16,
    offset_x: i16,
    offset_y: i16,
    shadow_x: i16,
    shadow_y: i16,
    shadow_byte: u8,
    compressed_len: usize,
    record_start: usize,
    data_offset: usize,
    record_end: usize,
}

fn parse_lib(bytes: &[u8]) -> (i32, usize, Vec<u32>) {
    assert!(bytes.len() >= 8, "MLibrary 头部应不小于 8B");
    let version = le_i32(bytes, 0);
    let count = le_i32(bytes, 4);
    assert!(count > 0, "MLibrary count 应为正: {count}");
    let count = count as usize;
    assert!(8 + 4 * count <= bytes.len(), "MLibrary 索引表越界");
    let offsets = (0..count)
        .map(|index| le_u32(bytes, 8 + 4 * index))
        .collect();
    (version, count, offsets)
}

fn parse_lib_record(bytes: &[u8], off: usize) -> LibRecord {
    assert!(off + 17 <= bytes.len(), "MLibrary 记录头越界: off={off}");
    let compressed_len = le_u32(bytes, off + 13) as usize;
    LibRecord {
        width: le_u16(bytes, off),
        height: le_u16(bytes, off + 2),
        offset_x: le_i16(bytes, off + 4),
        offset_y: le_i16(bytes, off + 6),
        shadow_x: le_i16(bytes, off + 8),
        shadow_y: le_i16(bytes, off + 10),
        shadow_byte: bytes[off + 12],
        compressed_len,
        record_start: off,
        data_offset: off + 17,
        record_end: off + 17 + compressed_len,
    }
}

/// 可完整解析的记录区间并集之外的空洞总量/数量/最大空洞。
fn wzl_gaps(bytes: &[u8], offsets: &[u32]) -> (usize, usize, usize) {
    let mut intervals: Vec<(usize, usize)> = offsets
        .iter()
        .filter_map(|raw| {
            let off = *raw as usize;
            if off == 0 || off + 16 > bytes.len() {
                return None;
            }
            let compressed_len = le_u32(bytes, off + 12) as usize;
            let end = off + 16 + compressed_len;
            if end > bytes.len() {
                return None;
            }
            Some((off, end))
        })
        .collect();
    intervals.sort_unstable();
    let mut gaps: Vec<usize> = Vec::new();
    let mut cursor = 0_usize;
    for (start, end) in intervals {
        if start > cursor {
            gaps.push(start - cursor);
        }
        cursor = cursor.max(end);
    }
    if cursor < bytes.len() {
        gaps.push(bytes.len() - cursor);
    }
    let total = gaps.iter().sum::<usize>();
    let largest = gaps.iter().copied().max().unwrap_or(0);
    (total, gaps.len(), largest)
}

fn python() -> &'static PathBuf {
    static PYTHON: OnceLock<PathBuf> = OnceLock::new();
    PYTHON.get_or_init(|| {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(explicit) = std::env::var("R77_PYTHON") {
            candidates.push(PathBuf::from(explicit));
        }
        if let Ok(profile) = std::env::var("USERPROFILE") {
            candidates.push(PathBuf::from(profile).join(PYTHON_REL));
        }
        if let Ok(home) = std::env::var("HOME") {
            candidates.push(PathBuf::from(home).join(PYTHON_REL));
        }
        candidates.push(PathBuf::from("python3"));
        candidates.push(PathBuf::from("python"));
        for candidate in &candidates {
            if candidate.is_absolute() && !candidate.exists() {
                continue;
            }
            let probe = Command::new(candidate).arg("--version").output();
            if let Ok(output) = probe {
                if output.status.success() {
                    return candidate.clone();
                }
            }
        }
        panic!("R77-A 未找到可用 Python；请设置 R77_PYTHON 指向 3.8+ 解释器")
    })
}

fn run_tool(args: &[&str]) -> std::process::Output {
    let tool = repo_root().join(TOOL_REL);
    Command::new(python())
        .arg(&tool)
        .args(args)
        .current_dir(repo_root())
        .output()
        .unwrap_or_else(|err| panic!("R77-A 无法运行审计工具 {}: {err}", tool.display()))
}

fn facts() -> &'static Value {
    static FACTS: OnceLock<Value> = OnceLock::new();
    FACTS.get_or_init(|| {
        let output = run_tool(&["--facts"]);
        assert!(
            output.status.success(),
            "R77-A 审计工具 --facts 必须 EXIT0: {:?}",
            output.status.code()
        );
        serde_json::from_slice(&output.stdout).expect("R77-A --facts 应输出合法 JSON")
    })
}

fn nested<'a>(root: &'a Value, path: &[&str]) -> &'a Value {
    let mut node = root;
    for key in path {
        node = match key.parse::<usize>() {
            Ok(index) => node
                .get(index)
                .unwrap_or_else(|| panic!("R77-A facts 数组缺下标 {key}: {path:?}")),
            Err(_) => node
                .get(*key)
                .unwrap_or_else(|| panic!("R77-A facts 缺少字段 {key}")),
        };
    }
    node
}

fn u64_field(root: &Value, path: &[&str]) -> u64 {
    nested(root, path)
        .as_u64()
        .unwrap_or_else(|| panic!("R77-A facts 字段非 u64: {path:?}"))
}

fn bool_field(root: &Value, path: &[&str]) -> bool {
    nested(root, path)
        .as_bool()
        .unwrap_or_else(|| panic!("R77-A facts 字段非 bool: {path:?}"))
}

fn str_field<'a>(root: &'a Value, path: &[&str]) -> &'a str {
    nested(root, path)
        .as_str()
        .unwrap_or_else(|| panic!("R77-A facts 字段非 str: {path:?}"))
}

fn array_field<'a>(root: &'a Value, path: &[&str]) -> &'a Vec<Value> {
    nested(root, path)
        .as_array()
        .unwrap_or_else(|| panic!("R77-A facts 字段非数组: {path:?}"))
}

fn array_item<'a>(root: &'a Value, path: &[&str], index: usize) -> &'a Value {
    array_field(root, path)
        .get(index)
        .unwrap_or_else(|| panic!("R77-A facts 数组 {path:?} 缺下标 {index}"))
}

fn u64_pair(value: &Value) -> [u64; 2] {
    let items = value.as_array().expect("R77-A 期望二元数组");
    assert_eq!(items.len(), 2, "R77-A 期望二元数组");
    [
        items[0].as_u64().expect("R77-A 期望 u64"),
        items[1].as_u64().expect("R77-A 期望 u64"),
    ]
}

// ---------------------------------------------------------------------------
// G1：默认 Magic2 缺 2360..2367（索引表越界 + 2025 地址在 EOF 外仅作参考）
// ---------------------------------------------------------------------------
#[test]
fn g1_default_magic2_missing_trap_frames() {
    let facts = facts();
    let wzx = read_bytes("resource/data/magic2.wzx");
    let wzl = read_bytes("resource/data/magic2.wzl");
    assert_eq!(wzx.len(), DEFAULT_WZX_BYTES);
    assert_eq!(wzl.len(), DEFAULT_WZL_BYTES);
    assert_eq!(
        str_field(facts, &["default_magic2", "wzx", "sha256"]),
        DEFAULT_WZX_SHA
    );
    assert_eq!(
        str_field(facts, &["default_magic2", "wzl", "sha256"]),
        DEFAULT_WZL_SHA
    );

    // 直接解析 WZX：declared=1460 < 物理槽 1480，最大可寻址 1479。
    let (declared, offsets) = parse_wzx(&wzx);
    assert_eq!(u64::from(declared), DEFAULT_DECLARED);
    assert_eq!(offsets.len() as u64, DEFAULT_SLOTS);
    assert_eq!((offsets.len() - 1) as u64, DEFAULT_MAX_FRAME_INDEX);

    // 2360..2367 全部越出索引表；2360 需要字节 9488..9492 > 5968。
    for (index, frame) in TRAP_FRAMES.iter().enumerate() {
        let need_start = 48 + 4 * (*frame as usize);
        let need_end = need_start + 4;
        let beyond = need_end - DEFAULT_WZX_BYTES;
        assert!(need_start >= DEFAULT_WZX_BYTES, "{frame} 应在索引表外");
        assert_eq!(beyond, 3_524 + 4 * index);
        let frame_facts = array_item(facts, &["default_magic2", "frames_2360_2367"], index);
        assert_eq!(frame_facts["frame"].as_u64(), Some(u64::from(*frame)));
        assert_eq!(frame_facts["declared"].as_bool(), Some(false));
        assert_eq!(frame_facts["slot_in_table"].as_bool(), Some(false));
        assert_eq!(
            frame_facts["beyond_wzx_bytes"].as_u64(),
            Some(beyond as u64)
        );
        assert_eq!(
            u64_pair(&frame_facts["needed_table_byte_range"]),
            [need_start as u64, need_end as u64]
        );
        assert_eq!(
            frame_facts["verdict"].as_str(),
            Some("MISSING_INDEX_OUT_OF_TABLE")
        );
    }
    assert_eq!(48 + 4 * 2360, 9_488);
    assert_eq!(9_488 - DEFAULT_WZX_BYTES, 3_520);
    assert_eq!(9_492 - DEFAULT_WZX_BYTES, 3_524);

    // F2：地址边界只作参考——未索引空间 2,637,986B 远大于八帧压缩载荷合计
    // 114,603B（含 8×17B 记录头共 114,739B），尺寸比较不能证明「全库不存在」。
    let (gap_total, gap_count, largest_gap) = wzl_gaps(&wzl, &offsets);
    assert_eq!(gap_total as u64, DEFAULT_UNINDEXED_GAP_TOTAL);
    assert_eq!(gap_count, DEFAULT_UNINDEXED_GAP_COUNT);
    assert_eq!(largest_gap as u64, DEFAULT_UNINDEXED_LARGEST_GAP);
    assert_eq!(
        u64_field(facts, &["default_magic2", "wzl", "append_headroom_bytes"]),
        0
    );
    assert_eq!(
        u64_field(
            facts,
            &["default_magic2", "wzl", "unindexed_gap_total_bytes"]
        ),
        DEFAULT_UNINDEXED_GAP_TOTAL
    );
    let payload: u64 = LIB_FRAMES
        .iter()
        .map(|frame| frame.compressed_len as u64)
        .sum();
    let headers = u64::try_from(LIB_FRAMES.len()).expect("G5：八帧") * 17;
    assert_eq!(payload, 114_603);
    assert_eq!(headers, 136);
    assert!(
        u64_field(
            facts,
            &["default_magic2", "wzl", "unindexed_gap_total_bytes"]
        ) > payload + headers,
        "G1：默认库未索引空间足以容纳八帧压缩载荷，不能据此断言素材不存在"
    );
    assert!(bool_field(
        facts,
        &["default_magic2", "address_boundary_check", "reference_only"]
    ));
    assert_eq!(
        u64_field(
            facts,
            &[
                "default_magic2",
                "address_boundary_check",
                "required_payload_compressed_bytes"
            ]
        ),
        payload
    );
    assert_eq!(
        u64_field(
            facts,
            &[
                "default_magic2",
                "address_boundary_check",
                "required_payload_with_headers_bytes"
            ]
        ),
        payload + headers
    );
    assert_eq!(
        u64_field(
            facts,
            &[
                "default_magic2",
                "address_boundary_check",
                "group_start_offset_2025"
            ]
        ),
        CORPUS_GROUP_START
    );
    assert_eq!(
        u64_field(
            facts,
            &[
                "default_magic2",
                "address_boundary_check",
                "group_start_beyond_default_wzl_by"
            ]
        ),
        CORPUS_GROUP_BEYOND_DEFAULT_WZL
    );
    assert!(CORPUS_GROUP_START > DEFAULT_WZL_BYTES as u64);
    // NOT_READY 的独立依据：WZX 索引表越界（2360 需字节 9488 > 5968）。
    assert_eq!(
        array_field(facts, &["default_magic2", "reason_codes"]),
        &vec![
            Value::String("MISSING_INDEX".to_owned()),
            Value::String("OUT_OF_INDEX_TABLE".to_owned()),
            Value::String("CORPUS_ADDRESS_BEYOND_DEFAULT_WZL_EOF".to_owned()),
        ]
    );
    let facts_json = serde_json::to_string(facts).expect("G1：facts 序列化");
    for banned in [
        "physically_absent",
        "physical_boundary_insufficient",
        "physically absent",
    ] {
        assert!(
            !facts_json.to_lowercase().contains(banned),
            "G1：facts 出现 F2 已撤销措辞 {banned:?}"
        );
    }

    assert_eq!(
        str_field(facts, &["default_magic2", "admission", "state"]),
        "NOT_READY"
    );
    assert!(!bool_field(
        facts,
        &["default_magic2", "admission", "can_become_ready"]
    ));
    assert_eq!(
        str_field(facts, &["default_magic2", "verdict"]),
        "NOT_READY"
    );
}

// ---------------------------------------------------------------------------
// G2：2025 语料八帧 zlib / 物理连续 / 边界 / RGB565 行对齐
// ---------------------------------------------------------------------------
#[test]
fn g2_corpus_2025_eight_frames_decode_and_chain() {
    let facts = facts();
    let wzx = read_bytes(str_field(facts, &["corpus_2025", "wzx", "path"]));
    let wzl = read_bytes(str_field(facts, &["corpus_2025", "wzl", "path"]));
    assert_eq!(wzx.len(), CORPUS_WZX_BYTES);
    assert_eq!(wzl.len(), CORPUS_WZL_BYTES);
    assert_eq!(
        str_field(facts, &["corpus_2025", "wzx", "sha256"]),
        CORPUS_WZX_SHA
    );
    assert_eq!(
        str_field(facts, &["corpus_2025", "wzl", "sha256"]),
        CORPUS_WZL_SHA
    );

    let (declared, offsets) = parse_wzx(&wzx);
    assert_eq!(u64::from(declared), CORPUS_WZX_DECLARED);
    assert_eq!(offsets.len() as u64, CORPUS_WZX_DECLARED);
    assert_eq!(
        u64_field(facts, &["corpus_2025", "wzx", "declared_count"]),
        CORPUS_WZX_DECLARED
    );
    assert_eq!(
        u64_field(facts, &["corpus_2025", "wzx", "physical_slots"]),
        CORPUS_WZX_DECLARED
    );

    // 八帧逐帧：Rust 直接解析记录头，解压长度/SHA 由只读工具实测。
    for (index, spec) in CORPUS_FRAMES.iter().enumerate() {
        let record = parse_wzl_record(&wzl, offsets[spec.frame as usize] as usize);
        assert_eq!(u64::from(spec.frame), u64::from(TRAP_FRAMES[index]));
        assert_eq!(record.record_start, spec.record_start);
        assert_eq!(record.record_end, spec.record_end);
        assert_eq!(record.format, spec.format, "format 应为 5 (RGB565)");
        assert_eq!(record.width, spec.width);
        assert_eq!(record.height, spec.height);
        assert_eq!(record.compressed_len, spec.compressed_len);
        assert_eq!(record.offset_x, spec.offset_x);
        assert_eq!(record.offset_y, spec.offset_y);
        assert_eq!(record.data_offset, spec.data_offset);

        let frame_facts = array_item(facts, &["corpus_2025", "frames_2360_2367"], index);
        assert_eq!(frame_facts["zlib_ok"].as_bool(), Some(true));
        assert_eq!(
            frame_facts["decompressed_len"].as_u64(),
            Some(spec.decompressed_len as u64)
        );
        assert_eq!(
            frame_facts["decompressed_sha256"].as_str(),
            Some(spec.decompressed_sha)
        );
        assert_eq!(
            frame_facts["compressed_sha256"].as_str().map(str::len),
            Some(64)
        );
        // 解压长度 = RGB565 行 4 字节对齐，不是 w*h*4。
        let rowaligned = rgb565_rowaligned_len(spec.width, spec.height);
        let w_h_4 = usize::from(spec.width) * usize::from(spec.height) * 4;
        assert_eq!(spec.decompressed_len, rowaligned);
        assert_ne!(rowaligned, w_h_4);
        assert_eq!(
            frame_facts["expected_rgb565_rowaligned_bytes"].as_u64(),
            Some(rowaligned as u64)
        );
        assert_eq!(
            frame_facts["decompressed_eq_rgb565_rowaligned"].as_bool(),
            Some(true)
        );
        assert_eq!(frame_facts["decompressed_eq_w_h_4"].as_bool(), Some(false));
    }

    // 物理连续：帧 i+1 起点 == 帧 i 结束；前帧 2359 尾 == 组起点；后帧 2368 头 == 组终点。
    for pair in CORPUS_FRAMES.windows(2) {
        assert_eq!(pair[0].record_end, pair[1].record_start);
    }
    let preceding = parse_wzl_record(&wzl, offsets[2359] as usize);
    assert_eq!(preceding.record_end as u64, CORPUS_GROUP_START);
    assert_eq!(preceding.width, 130);
    assert_eq!(preceding.height, 140);
    let following = parse_wzl_record(&wzl, offsets[2368] as usize);
    assert_eq!(following.record_start as u64, CORPUS_GROUP_END);
    assert_eq!(following.width, 1);
    assert_eq!(following.height, 1);
    assert_eq!(following.compressed_len, 0);
    let group = &facts["corpus_2025"]["group_boundary"];
    assert_eq!(group["chain_contiguous"].as_bool(), Some(true));
    assert_eq!(
        group["group_start_offset"].as_u64(),
        Some(CORPUS_GROUP_START)
    );
    assert_eq!(group["group_end_offset"].as_u64(), Some(CORPUS_GROUP_END));
    assert_eq!(
        group["group_compressed_bytes"].as_u64(),
        Some(CORPUS_GROUP_COMPRESSED_BYTES)
    );
    assert_eq!(
        group["preceding_record_end_equals_group_start"].as_bool(),
        Some(true)
    );
    assert_eq!(
        group["following_record_start_equals_group_end"].as_bool(),
        Some(true)
    );
    assert_eq!(
        group["following_is_1x1_empty_placeholder"].as_bool(),
        Some(true)
    );
    assert!(bool_field(
        facts,
        &["corpus_2025", "wzl", "declared_equals_physical"]
    ));
    assert_eq!(
        u64_field(facts, &["corpus_2025", "wzl", "unindexed_gap_total_bytes"]),
        CORPUS_WZL_UNINDEXED_GAP
    );
    assert!(bool_field(
        facts,
        &[
            "corpus_2025",
            "decompressed_size_rule",
            "all_eight_eq_rgb565_rowaligned"
        ]
    ));
    assert!(!bool_field(
        facts,
        &[
            "corpus_2025",
            "decompressed_size_rule",
            "all_eight_eq_w_h_4"
        ]
    ));
}

// ---------------------------------------------------------------------------
// G3：同号替代未证明（2025 尺寸 vs Lib 尺寸不匹配 + 默认库缺帧）
// ---------------------------------------------------------------------------
#[test]
fn g3_same_number_substitution_not_proven() {
    let facts = facts();
    let wzx = read_bytes(str_field(facts, &["corpus_2025", "wzx", "path"]));
    let wzl = read_bytes(str_field(facts, &["corpus_2025", "wzl", "path"]));
    let (_, offsets) = parse_wzx(&wzx);
    let lib_path = str_field(facts, &["lib_candidates", "candidates", "0", "path"]);
    let lib_bytes = read_bytes(lib_path);
    let (_, _, lib_offsets) = parse_lib(&lib_bytes);

    for (index, frame) in TRAP_FRAMES.iter().enumerate() {
        let corpus = parse_wzl_record(&wzl, offsets[*frame as usize] as usize);
        let lib_record = parse_lib_record(&lib_bytes, lib_offsets[*frame as usize] as usize);
        assert_ne!(
            (corpus.width, corpus.height),
            (lib_record.width, lib_record.height),
            "2360..2367 同号替代要求尺寸一致，但 {frame} 不一致"
        );
        let match_row = array_item(
            facts,
            &["corpus_2025", "shape_vs_lib_candidate", "matches"],
            index,
        );
        assert_eq!(match_row["frame"].as_u64(), Some(u64::from(*frame)));
        assert_eq!(match_row["match"].as_bool(), Some(false));
        assert_eq!(match_row["candidate"].as_array().map(Vec::len), Some(2));
        assert_eq!(match_row["lib"].as_array().map(Vec::len), Some(2));
    }
    assert!(bool_field(
        facts,
        &["corpus_2025", "shape_vs_lib_candidate", "all_mismatch"]
    ));

    assert!(!bool_field(
        facts,
        &["admission", "same_number_substitution_proven"]
    ));
    assert_eq!(
        str_field(facts, &["admission", "corpus_2025_state"]),
        "UNVERIFIED_SEMANTICS"
    );
    assert_eq!(
        str_field(facts, &["admission", "default_state"]),
        "NOT_READY"
    );
    assert_eq!(
        str_field(facts, &["corpus_2025", "admission", "state"]),
        "UNVERIFIED_SEMANTICS"
    );
    assert!(!bool_field(
        facts,
        &["corpus_2025", "admission", "can_become_ready"]
    ));
    let corpus_row = array_item(facts, &["admission", "matrix"], 1);
    assert_eq!(corpus_row["state"].as_str(), Some("UNVERIFIED_SEMANTICS"));
    assert_eq!(corpus_row["can_become_ready"].as_bool(), Some(false));
    // 默认库缺帧（G1 双证）意味着也无法从默认 Magic2 取同号帧替代。
    assert_eq!(
        array_item(facts, &["admission", "matrix"], 0)["state"].as_str(),
        Some("NOT_READY")
    );
}

// ---------------------------------------------------------------------------
// G4：Magic2.Lib 双候选存在且字节一致（AWAITING_BINDING，不得 READY）
// ---------------------------------------------------------------------------
#[test]
fn g4_magic2_lib_candidates_await_binding() {
    let facts = facts();
    assert!(bool_field(facts, &["lib_candidates", "identical_sha"]));
    let candidates = array_field(facts, &["lib_candidates", "candidates"]);
    assert_eq!(candidates.len(), 2, "Magic2.Lib 应有双候选");
    let first_path = candidates[0]["path"].as_str().expect("候选 0 路径");
    let second_path = candidates[1]["path"].as_str().expect("候选 1 路径");
    assert!(first_path.ends_with("Magic2.Lib"));
    assert!(second_path.ends_with("Magic2.Lib"));
    assert_ne!(first_path, second_path);
    assert_eq!(candidates[1]["bytes"].as_u64(), Some(LIB_BYTES as u64));
    assert_eq!(candidates[1]["sha256"].as_str(), Some(LIB_SHA));

    for candidate in candidates {
        assert_eq!(candidate["bytes"].as_u64(), Some(LIB_BYTES as u64));
        assert_eq!(candidate["sha256"].as_str(), Some(LIB_SHA));
        assert_eq!(candidate["version"].as_u64(), Some(LIB_VERSION));
        assert_eq!(candidate["count"].as_u64(), Some(LIB_COUNT));
        assert_eq!(
            candidate["index_table_end"].as_u64(),
            Some(LIB_INDEX_TABLE_END)
        );
        assert_eq!(
            candidate["first_record_offset"].as_u64(),
            Some(LIB_INDEX_TABLE_END)
        );
        assert_eq!(candidate["structural_issue_count"].as_u64(), Some(0));
        assert_eq!(candidate["status"].as_str(), Some("AWAITING_BINDING"));
        assert_eq!(candidate["ready"].as_bool(), Some(false));
        assert_eq!(candidate["certified"].as_bool(), Some(false));
        assert_eq!(candidate["all_eight_gzip_ok"].as_bool(), Some(true));
        assert_eq!(candidate["all_eight_eq_w_h_4"].as_bool(), Some(true));
        assert_eq!(candidate["group_contiguous"].as_bool(), Some(true));
    }

    // Rust 直接解析两候选头部：v2 / count 2790 / 索引表结束 11168 / 偏移升序。
    let mut first_raw = Vec::new();
    for candidate in candidates {
        let path = candidate["path"].as_str().expect("候选路径");
        let data = read_bytes(path);
        first_raw.push(data.clone());
        let (version, count, offsets) = parse_lib(&data);
        assert_eq!(version as u64, LIB_VERSION);
        assert_eq!(count as u64, LIB_COUNT);
        assert_eq!((8 + 4 * count) as u64, LIB_INDEX_TABLE_END);
        assert_eq!(u64::from(offsets[0]), LIB_INDEX_TABLE_END);
        assert!(offsets.windows(2).all(|pair| pair[0] < pair[1]));
    }
    assert_eq!(first_raw[0], first_raw[1], "双候选必须字节一致");
    assert_eq!(first_raw[0].len(), LIB_BYTES);

    let data = &first_raw[0];
    let (_, _, offsets) = parse_lib(data);
    for (index, spec) in LIB_FRAMES.iter().enumerate() {
        let record = parse_lib_record(data, offsets[spec.frame as usize] as usize);
        assert_eq!(record.record_start, spec.record_start);
        assert_eq!(record.record_end, spec.record_end);
        assert_eq!(record.width, spec.width);
        assert_eq!(record.height, spec.height);
        assert_eq!(record.compressed_len, spec.compressed_len);
        assert_eq!(record.data_offset, spec.data_offset);
        assert_eq!(record.shadow_x, 0);
        assert_eq!(record.shadow_y, 0);
        assert_eq!(record.shadow_byte, 0);
        assert_eq!((record.offset_x, record.offset_y), (-296, -149));
        assert_eq!(
            &data[record.data_offset..record.data_offset + 2],
            b"\x1f\x8b"
        );
        let frame_facts = array_item(
            facts,
            &["lib_candidates", "candidates", "0", "frames_2360_2367"],
            index,
        );
        assert_eq!(
            frame_facts["decompressed_len"].as_u64(),
            Some(spec.decompressed_len as u64)
        );
        assert_eq!(
            frame_facts["decompressed_sha256"].as_str(),
            Some(spec.decompressed_sha)
        );
    }

    // 八帧 gzip 解压长度 = w*h*4（RGBA），与 2025 的 RGB565 行对齐规则不同。
    for (spec, record) in LIB_FRAMES.iter().zip(TRAP_FRAMES.iter()) {
        assert_eq!(u64::from(spec.frame), u64::from(*record));
        let rgba = u64::from(spec.width) * u64::from(spec.height) * 4;
        assert_eq!(rgba, spec.decompressed_len as u64);
    }

    // 邻居 2360-3 / 2368 / 2369 均为 4×1 空帧。
    let neighbors = array_field(&facts["lib_candidates"]["candidates"][0], &["neighbors"]);
    assert_eq!(neighbors.len(), LIB_NEIGHBOR_FRAMES.len());
    for (index, frame) in LIB_NEIGHBOR_FRAMES.iter().enumerate() {
        let record = parse_lib_record(data, offsets[*frame as usize] as usize);
        assert_eq!(record.width, 4);
        assert_eq!(record.height, 1);
        assert_eq!(record.compressed_len, LIB_NEIGHBOR_COMPRESSED_LEN);
        assert_eq!(neighbors[index]["frame"].as_u64(), Some(u64::from(*frame)));
        assert_eq!(neighbors[index]["width"].as_u64(), Some(4));
        assert_eq!(neighbors[index]["height"].as_u64(), Some(1));
        assert_eq!(
            neighbors[index]["decompressed_len"].as_u64(),
            Some(LIB_NEIGHBOR_DECOMPRESSED_LEN)
        );
        assert_eq!(neighbors[index]["is_4x1_empty"].as_bool(), Some(true));
    }

    assert_eq!(
        str_field(facts, &["admission", "lib_candidate_state"]),
        "AWAITING_BINDING"
    );
    let lib_row = array_item(facts, &["admission", "matrix"], 2);
    assert_eq!(lib_row["state"].as_str(), Some("AWAITING_BINDING"));
    assert_eq!(
        lib_row["ready_eligible_with_human_approval"].as_bool(),
        Some(true)
    );
    assert_eq!(lib_row["can_become_ready"].as_bool(), Some(false));
    assert!(!bool_field(facts, &["admission", "readiness_claimed"]));
    assert!(!bool_field(facts, &["admission", "all_can_become_ready"]));
}

// ---------------------------------------------------------------------------
// G5：现行 Rust 行为锁 + Crystal SpellObject.cs 差异矩阵
// ---------------------------------------------------------------------------
#[test]
fn g5_current_rust_behaviour_locked_with_crystal_diff_matrix() {
    let spellfx = read_text(SPELLFX_REL);
    // 默认 700ms 路径（色块/文字窗）与通用 Magic/60ms/居中 Over 分支。
    for anchor in [
        "pub const CAST_MS: i64 = 700;",
        "_ => CAST_MS,",
        "pub const AREA_MS: i64 = 4_000;",
        "if event.spell == Spell::TrapHexagon && event.from_object_spell {",
        "let phase = (age / TRAP_HEXAGON_FRAME_MS) % TRAP_HEXAGON_FRAME_COUNT;",
        "scene.draw_effect_blend(frame, mir_render::FxKind::Magic, index, x, y, true);",
        "pub const TRAP_HEXAGON_BASE_INDEX: u32 = 1_390;",
        "pub const TRAP_HEXAGON_FRAME_COUNT: i64 = 10;",
        "pub const TRAP_HEXAGON_FRAME_MS: i64 = 100;",
        "pub const TRAP_HEXAGON_SPRITE_MS: i64 = 4_000;",
        "let duration = duration_ms(event.spell);",
        "let frame_index = (age / MAGIC_FX_FRAME_MS).clamp(0, MAGIC_FX_FRAMES - 1);",
        "pub const MAGIC_FX_FRAMES: i64 = 10;",
        "pub const MAGIC_FX_FRAME_MS: i64 = 60;",
        "let kind = if matches!(event.spell, Spell::ThunderBolt | Spell::Lightning) {",
        "mir_render::FxKind::Bolt",
        "let x = x + (mir_render::CELL_WIDTH - i32::try_from(width).unwrap_or(0)) / 2;",
        "let y = y + (mir_render::CELL_HEIGHT - i32::try_from(height).unwrap_or(0)) / 2;",
        "scene.draw_effect(frame, kind, frame_index, x, y);",
    ] {
        assert!(
            spellfx.contains(anchor),
            "G5：spellfx.rs 缺少行为锚点 {anchor:?}"
        );
    }
    // 公共产品窗不变：duration_ms(TrapHexagon) 仍是 700ms（不是 4000）。
    assert_eq!(
        mir_client::ui::spellfx::duration_ms(Spell::TrapHexagon),
        700
    );
    assert_eq!(mir_client::ui::spellfx::TRAP_HEXAGON_BASE_INDEX, 1_390);
    assert_eq!(mir_client::ui::spellfx::TRAP_HEXAGON_FRAME_COUNT, 10);
    assert_eq!(mir_client::ui::spellfx::TRAP_HEXAGON_FRAME_MS, 100);
    assert_eq!(mir_client::ui::spellfx::TRAP_HEXAGON_SPRITE_MS, 4_000);
    assert!(spellfx.contains("不是把 `TrapHexagon` 并入 `AREA_MS`"));

    // Crystal SpellObject.cs Trap 分支：Magic2 / 2360 / 100ms / 8 帧 / Blend。
    let crystal = read_text(CRYSTAL_SPELLOBJECT_REL);
    let trap_start = crystal
        .find("case Spell.Trap:")
        .expect("G5：Crystal SpellObject.cs 缺 case Spell.Trap:");
    let trap_block = &crystal[trap_start..crystal.len().min(trap_start + 400)];
    for anchor in [
        "BodyLibrary = Libraries.Magic2;",
        "DrawFrame = 2360;",
        "FrameInterval = 100;",
        "FrameCount = 8;",
        "Blend = true;",
    ] {
        assert!(
            trap_block.contains(anchor),
            "G5：Crystal Trap 分支缺少 {anchor:?}"
        );
    }

    // F1：current 列从 spellfx.rs 独立解析，target 列从 Crystal 独立解析，
    // 内联矩阵三列逐字段等于解析值（改动矩阵任一数值即红）。
    let rust = rust_matrix_values(&spellfx);
    let target = crystal_matrix_values(&crystal);
    let derived = derived_matrix_rows(&rust, &target);
    assert_eq!(CRYSTAL_DIFF_MATRIX.len(), MATRIX_FIELDS.len());
    for (row, expected) in derived.iter().zip(CRYSTAL_DIFF_MATRIX.iter()) {
        assert_eq!(row.field, expected.0, "G5：矩阵字段顺序/名称漂移");
        assert_eq!(
            row.current, expected.1,
            "G5：current 列与生产源码解析不一致"
        );
        assert_eq!(row.target, expected.2, "G5：target 列与 Crystal 解析不一致");
        assert_eq!(row.status, expected.3, "G5：status 列漂移");
    }
    // 关键差异：current 60ms/10帧 vs target 100ms/8帧，不得标 SAME。
    assert_ne!(
        derived[2].current, derived[2].target,
        "G5：frame_count 必须可区分"
    );
    assert_ne!(
        derived[3].current, derived[3].target,
        "G5：interval_ms 必须可区分"
    );
    assert_eq!(derived[3].status, "DIFF_TARGET_REQUIRED");

    // 与只读工具 --matrix 输出三方互证（工具矩阵被改 => 此处唯一 G5 红）。
    let out = run_tool(&["--matrix"]);
    assert!(
        out.status.success(),
        "G5：工具 --matrix 必须 EXIT0，实际 {:?}",
        out.status.code()
    );
    let matrix_json: Value =
        serde_json::from_slice(&out.stdout).expect("G5：--matrix 应输出合法 JSON");
    assert_eq!(matrix_json["schema"], "round77-a/crystal-diff-matrix/v1");
    let rows_json = matrix_json["rows"]
        .as_array()
        .expect("G5：--matrix.rows 应为数组");
    assert_eq!(rows_json.len(), CRYSTAL_DIFF_MATRIX.len());
    for (json_row, (field, current, target, status)) in
        rows_json.iter().zip(CRYSTAL_DIFF_MATRIX.iter())
    {
        assert_eq!(json_row["field"].as_str(), Some(*field));
        assert_eq!(json_row["current"].as_str(), Some(*current));
        assert_eq!(json_row["target"].as_str(), Some(*target));
        assert_eq!(json_row["status"].as_str(), Some(*status));
    }
    let diffs = CRYSTAL_DIFF_MATRIX
        .iter()
        .filter(|row| row.3 == "DIFF_TARGET_REQUIRED")
        .count();
    let blocked = CRYSTAL_DIFF_MATRIX
        .iter()
        .filter(|row| row.3 == "BLOCKED_BY_ASSET_ADMISSION")
        .count();
    let partial = CRYSTAL_DIFF_MATRIX
        .iter()
        .filter(|row| row.3 == "PARTIAL")
        .count();
    let same = CRYSTAL_DIFF_MATRIX
        .iter()
        .filter(|row| row.3 == "SAME")
        .count();
    assert_eq!(diffs, 4);
    assert_eq!(blocked, 1);
    assert_eq!(partial, 1);
    assert_eq!(same, 1);
}

// ---------------------------------------------------------------------------
// G6：准入状态机——无人工批准不得 READY；绕行/自动升 READY 必须失败
// ---------------------------------------------------------------------------
#[test]
fn g6_admission_state_machine_never_auto_ready() {
    let facts = facts();
    let state_machine = &facts["admission"]["state_machine"];
    let states: Vec<&str> = state_machine["states"]
        .as_array()
        .expect("states 应为数组")
        .iter()
        .map(|state| state.as_str().expect("state 应为字符串"))
        .collect();
    assert_eq!(
        states,
        [
            "NOT_READY",
            "UNVERIFIED_SEMANTICS",
            "AWAITING_BINDING",
            "READY"
        ]
    );
    assert_eq!(
        state_machine["ready_requires_human_approval"].as_bool(),
        Some(true)
    );
    assert_eq!(state_machine["auto_ready_forbidden"].as_bool(), Some(true));
    assert_eq!(
        state_machine["audit_exit0_does_not_mean_ready"].as_bool(),
        Some(true)
    );
    assert!(
        state_machine["forbidden_transition"]
            .as_str()
            .expect("forbidden_transition")
            .contains("--human-approve")
    );

    let requirements = &facts["admission"]["ready_evidence_requirements"];
    assert_eq!(requirements["spell"].as_u64(), Some(98));
    assert_eq!(requirements["library"].as_str(), Some("Magic2"));
    assert_eq!(requirements["base"].as_u64(), Some(2_360));
    assert_eq!(requirements["count"].as_u64(), Some(8));
    assert_eq!(requirements["interval_ms"].as_u64(), Some(100));
    let binding_fields: Vec<&str> = requirements["binding_fields"]
        .as_array()
        .expect("binding_fields 应为数组")
        .iter()
        .map(|value| value.as_str().expect("binding field"))
        .collect();
    for expected in [
        "schema_version",
        "spell",
        "library",
        "base",
        "count",
        "interval_ms",
        "crystal_sha256",
        "wzx_sha256",
        "wzl_sha256",
        "review_evidence_path",
        "review_evidence_sha256",
    ] {
        assert!(
            binding_fields.contains(&expected),
            "G6：绑定字段缺 {expected}"
        );
    }
    assert!(!bool_field(facts, &["admission", "readiness_claimed"]));
    assert!(!bool_field(facts, &["admission", "all_can_become_ready"]));
    for row in array_field(facts, &["admission", "matrix"]) {
        assert!(!row["can_become_ready"].as_bool().expect("matrix 行"));
    }

    // 绕行负控：显式要求 READY 而不走人工批准 → 必定 EXIT2 + AUTO_READY_FORBIDDEN。
    let bypass = run_tool(&["--attempt-ready", "lib-candidate"]);
    assert_eq!(
        bypass.status.code(),
        Some(2),
        "G6：--attempt-ready 必须 EXIT2，实际 {:?}",
        bypass.status.code()
    );
    let bypass_stdout = String::from_utf8_lossy(&bypass.stdout);
    assert!(
        bypass_stdout.contains("AUTO_READY_FORBIDDEN"),
        "G6：绕行必须报 AUTO_READY_FORBIDDEN，stdout={bypass_stdout}"
    );
    assert!(bypass_stdout.contains("\"ready\": false"));

    // 人工批准路径缺绑定/证据 → 拒绝，仍不得 READY。
    let refused = run_tool(&["--human-approve-json", "{}"]);
    assert_eq!(
        refused.status.code(),
        Some(2),
        "G6：空人工批准必须被拒绝 EXIT2，实际 {:?}",
        refused.status.code()
    );
    let refused_stdout = String::from_utf8_lossy(&refused.stdout);
    assert!(
        refused_stdout.contains("READY_REFUSED"),
        "G6：{refused_stdout}"
    );
    assert!(refused_stdout.contains("\"state\": \"REFUSED\""));
    assert!(refused_stdout.contains("\"ready\": false"));

    // 冻结期望自检：--assert 必须 PASS（审计 EXIT0 只说明口径一致，不代表 READY）。
    let asserted = run_tool(&["--assert"]);
    let assert_stdout = String::from_utf8_lossy(&asserted.stdout);
    assert_eq!(
        asserted.status.code(),
        Some(0),
        "G6：工具 --assert 必须 PASS/EXIT0，实际 {:?}，stdout={assert_stdout}",
        asserted.status.code()
    );
    assert!(assert_stdout.contains("ASSERT PASS"), "G6：{assert_stdout}");
    assert!(assert_stdout.contains("failures=0"), "G6：{assert_stdout}");
    assert!(
        !assert_stdout.contains("READY"),
        "G6：审计输出不得自升 READY"
    );
}

// ---------------------------------------------------------------------------
// G7：R76 G8 旧 18 帧与 R59 116 帧冻结不变
// ---------------------------------------------------------------------------
#[test]
fn g7_r76_and_r59_frozen_sets_unchanged() {
    // R76 文档：旧 magic 为子集、Magic2 不变。
    let r76_doc = read_text(R76_DOC_REL);
    assert!(r76_doc.contains("旧magic为子集且Magic2不变"));
    assert!(r76_doc.contains(R59_F116_FINGERPRINT));
    assert!(r76_doc.contains("116"));
    assert!(r76_doc.contains("R59 supersedes"));

    // R76 测试：G8 断言 106 旧集 + 1390..=1399 新增段 + Magic2 旧 18 帧不变。
    let r76_test = read_text(R76_TEST_REL);
    for anchor in [
        "G8：旧 magic 冻结集应为 106 帧",
        "for frame in 1_390..=1_399 {",
        "MAGIC2=",
        "fn expand_ranges(ranges: &[(u32, u32)]) -> BTreeSet<u32>",
        "fn parse_ranges(",
        "let mut expected_magic2 = BTreeSet::new();",
        "for frame in 0..=2 {",
        "for frame in 10..=14 {",
        "for frame in 20..=29 {",
        "G8：Magic2 帧集合必须保持 0-2 + 10-14 + 20-29 不变（R76 素材在 Magic 库）",
    ] {
        assert!(r76_test.contains(anchor), "G7：R76 测试缺锚点 {anchor:?}");
    }
    let old_eighteen = (0..=2).count() + (10..=14).count() + (20..=29).count();
    assert_eq!(old_eighteen, 18, "R76 G8 冻结旧 Magic2 帧数应为 18");
    assert!(r76_test.contains("old_magic.len(), 106"));
    let appended = (1_390..=1_399).count();
    assert_eq!(appended, 10, "R76 追加 1390..=1399 共十帧");
    assert_eq!(106 + appended, 116);

    // R59：116 帧整集指纹、410 起始、106 旧子集指纹与 570-579 十帧不变。
    let r59_test = read_text(R59_TEST_REL);
    for anchor in [
        "const EXPECTED_PNG_COUNT: usize = 116;",
        "const MAGIC_BASE_INDEX: u32 = 410;",
        R59_F116_FINGERPRINT,
        R59_F106_FINGERPRINT,
        "共 116 PNGs；原 106 帧顺序与数据不变。",
        "570-579",
        "410-415",
    ] {
        assert!(r59_test.contains(anchor), "G7：R59 测试缺锚点 {anchor:?}");
    }

    // R76 终审 manifest：四个冻结工件的 SHA/字节数必须与现行文件一致。
    let manifest = read_text(R76_MANIFEST_REL);
    assert!(manifest.contains(R76_MANIFEST_SCHEMA));
    assert!(manifest.contains(R76_MANIFEST_TASK));
    let manifest_json: Value = serde_json::from_str(&manifest).expect("R76 manifest 应为合法 JSON");
    assert_eq!(manifest_json["schema"].as_str(), Some(R76_MANIFEST_SCHEMA));
    let artifacts = manifest_json["artifacts"]
        .as_array()
        .expect("manifest.artifacts 应为数组");
    for (path, expected_sha) in [
        (SPELLFX_REL, SPELLFX_SHA),
        (R76_TEST_REL, R76_TEST_SHA),
        (R72_TEST_REL, R72_TEST_SHA),
        (R59_TEST_REL, R59_TEST_SHA),
    ] {
        let artifact = artifacts
            .iter()
            .find(|item| item.get("path").and_then(Value::as_str) == Some(path))
            .unwrap_or_else(|| panic!("G7：R76 manifest 缺工件 {path}"));
        assert_eq!(
            artifact["sha256"].as_str(),
            Some(expected_sha),
            "G7：{path} 的冻结 SHA 变化"
        );
        let bytes = artifact["bytes"].as_u64().expect("artifact bytes");
        let current = std::fs::metadata(repo_root().join(path))
            .unwrap_or_else(|err| panic!("G7：读取 {path} 元数据失败: {err}"))
            .len();
        assert_eq!(bytes, current, "G7：{path} 字节数与 R76 冻结值不一致");
    }
}
