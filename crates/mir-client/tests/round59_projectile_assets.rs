//! Round 59 D4: `magic.wzl` projectile/impact frame gate (real assets, never skipped).
//!
//! The frame set is produced by `tools/assets/analyze_usage.py` (`MAGIC_FRAMES`,
//! dispatch M1): 16 directions x 6 frames starting at 410 (`410-415 / 420-425 / ... /
//! 560-565`) plus the 10 impact frames `570-579`（R59 原集 106 PNGs；R76 追加见下段）。
//! `416-419` and `566-569` are 0x0 empty frames and must not appear.
//!
//! R76 有限 supersede（R76 Lead 裁定 5）：R76 的 `MAGIC_FRAMES` 追加 `range(1390, 1400)`
//! 十帧（`magic` 库困魔咒 / 149 TrapHexagon），本清单按规范序追加 `1390..=1399` 段，
//! 共 116 PNGs；原 106 帧顺序与数据不变。
//!
//! These gates deliberately never skip. Round 59 is executed against real assets, so a
//! missing `game-assets/` tree or a missing `resource/` corpus must fail loudly instead
//! of turning into a green run (dispatch C4: logic gates must not SKIP).
//!
//! Expected values were measured on the frozen R59 asset tree recorded in
//! `target/round59/d3/attempt-1/` (`frameset_sha256`（106 帧子集，R76 后实测不变）
//! `e465026a50ee6f86d6762e3959386fa0132ffabf63da9c92afeebdf3d52a1e98`）。
//! R76 有限 supersede（R76 Lead 裁定 5）后整集 116 帧指纹为
//! `e642330a568402b21528afb04459108f3dc596f7e37ad5c276670ef342940eff`。

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// `magic.wzl` base frame of the first direction strip (`Effect.cs:196` base index 410).
const MAGIC_BASE_INDEX: u32 = 410;
/// Frames per direction (`CreateProjectile(..., 6, ...)`).
const FRAMES_PER_DIRECTION: u32 = 6;
/// Direction strips in the file (16-way `Direction16`).
const DIRECTIONS: u32 = 16;
/// `magic.wzl` impact frame base (`GameScene.cs:4708`, `new Effect(..., 570, 10, ...)`).
const IMPACT_BASE_INDEX: u32 = 570;
/// Impact frames (`570..=579`).
const IMPACT_FRAMES: u32 = 10;
/// R76 追加段基址：`magic` 库困魔咒（149 TrapHexagon）十帧（R76 Lead 裁定 5）。
const TRAP_BASE_INDEX: u32 = 1390;
/// R76 追加段帧数（`1390..=1399`）。
const TRAP_FRAMES: u32 = 10;
/// Total PNGs in the R59 frame set（R76 supersede：106 + `1390..=1399` 十帧 = 116）。
const EXPECTED_PNG_COUNT: usize = 116;
/// sha256 of the whole 116-frame set (sorted `<name> <size> <sha256>` lines joined by `\n`).
/// R76 有限 supersede（R76 Lead 裁定 5）：106 帧子集旧值
/// `e465026a50ee6f86d6762e3959386fa0132ffabf63da9c92afeebdf3d52a1e98` 实测不变；
/// 整集（追加 `1390..=1399`）新值为下方 `e642330a…`。
const FROZEN_FRAMESET_SHA256: &str =
    "e642330a568402b21528afb04459108f3dc596f7e37ad5c276670ef342940eff";
/// sha256 of `resource/data/magic.wzl` recorded in `index.json.source_sha256`.
const FROZEN_WZL_SHA256: &str = "3b8a2dba9a1246e90a663e712a84b488fbee924ad6ea70af3be986f00a02ab25";
/// Byte size of `resource/data/magic.wzl` recorded in `index.json.source_bytes`.
const FROZEN_WZL_BYTES: u64 = 6_275_819;

/// Frozen per-frame expectations on the R59 asset tree:
/// `(frame index, png sha256, width, height)`.
const SAMPLE_FRAMES: [(u32, &str, u32, u32); 4] = [
    (
        410,
        "8a25595d5a0feaefbcc70a3ec32bf01d8978ea603d7a79aea59bfd31372d2157",
        84,
        82,
    ),
    (
        415,
        "31501732775ed776cb99ca6deaf2d7af42ee03fd4a2e33c3ae68d4ded521d027",
        72,
        79,
    ),
    (
        570,
        "1a1b9f47dd51cf34c3aad9d56dd881d49331fd1c8fb2d9019d09913a4300c9a9",
        80,
        77,
    ),
    (
        579,
        "cac99fdb6a392909ddbfca2a97d86f09951eaabc7fa2eb3ec72c27e56edf922d",
        92,
        119,
    ),
];

/// One decoded `index.json.images[]` entry (only the fields asserted by D4).
struct IndexedFrame<'a> {
    index: u32,
    png: &'a str,
    png_sha256: &'a str,
    width: u32,
    height: u32,
    empty: bool,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn magic_dir() -> PathBuf {
    repo_root().join("game-assets/gfx/sprites/effect/magic")
}

/// Read a file or panic: the R59 gate is asset-mandatory and must never skip.
fn read_required(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| {
        panic!(
            "R59 requires real magic assets, but {} is unreadable: {error}. \
             The asset gate fails instead of skipping (dispatch C4).",
            path.display()
        )
    })
}

fn object<'a>(value: &'a Value, key: &str) -> &'a Value {
    value
        .get(key)
        .unwrap_or_else(|| panic!("index.json is missing the `{key}` field"))
}

fn u64_field(value: &Value, key: &str) -> u64 {
    object(value, key)
        .as_u64()
        .unwrap_or_else(|| panic!("index.json field `{key}` must be an unsigned integer"))
}

fn str_field<'a>(value: &'a Value, key: &str) -> &'a str {
    object(value, key)
        .as_str()
        .unwrap_or_else(|| panic!("index.json field `{key}` must be a string"))
}

fn array_field<'a>(value: &'a Value, key: &str) -> &'a Vec<Value> {
    object(value, key)
        .as_array()
        .unwrap_or_else(|| panic!("index.json field `{key}` must be an array"))
}

fn index_json() -> Value {
    let path = magic_dir().join("index.json");
    let bytes = read_required(&path);
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("cannot parse {} as JSON: {error}", path.display()))
}

fn indexed_frames(index: &Value) -> Vec<IndexedFrame<'_>> {
    array_field(index, "images")
        .iter()
        .map(|image| IndexedFrame {
            index: u32::try_from(u64_field(image, "index")).expect("frame index fits u32"),
            png: str_field(image, "png"),
            png_sha256: str_field(image, "png_sha256"),
            width: u32::try_from(u64_field(image, "width")).expect("frame width fits u32"),
            height: u32::try_from(u64_field(image, "height")).expect("frame height fits u32"),
            empty: object(image, "empty")
                .as_bool()
                .unwrap_or_else(|| panic!("index.json field `empty` must be a boolean")),
        })
        .collect()
}

/// The frozen R59 frame set: `410 + direction * 10 + frame` for 16 directions, then
/// `570..=579`, then R76 追加段 `1390..=1399`（R76 Lead 裁定 5）。Returns 116 indices in
/// canonical order（410 段、570 段、1390 段）。
fn expected_frame_indices() -> Vec<u32> {
    let mut frames = Vec::with_capacity(EXPECTED_PNG_COUNT);
    for direction in 0..DIRECTIONS {
        for frame in 0..FRAMES_PER_DIRECTION {
            frames.push(MAGIC_BASE_INDEX + direction * 10 + frame);
        }
    }
    frames.extend(IMPACT_BASE_INDEX..IMPACT_BASE_INDEX + IMPACT_FRAMES);
    frames.extend(TRAP_BASE_INDEX..TRAP_BASE_INDEX + TRAP_FRAMES);
    frames
}

/// Real PNG dimensions from the IHDR chunk (independent of what `index.json` claims).
fn png_dimensions(bytes: &[u8]) -> (u32, u32) {
    assert!(
        bytes.len() > 24,
        "PNG is shorter than the 8-byte signature plus the 16-byte IHDR header: {} bytes",
        bytes.len()
    );
    assert!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "file does not start with the PNG signature"
    );
    let chunk = bytes.get(12..16).expect("PNG has at least four IHDR bytes");
    assert_eq!(chunk, b"IHDR".as_slice(), "first PNG chunk must be IHDR");
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    (width, height)
}

#[test]
fn index_json_lists_exactly_the_frozen_round59_magic_frame_set() {
    let index = index_json();
    let dir = magic_dir();

    assert_eq!(
        u64_field(&index, "count"),
        u64::try_from(EXPECTED_PNG_COUNT).expect("count fits u64"),
        "index.json `count` must be the frozen 116-frame set"
    );
    assert_eq!(
        str_field(&index, "lib"),
        "magic.wzl",
        "the frame set must come from magic.wzl"
    );
    assert_eq!(
        str_field(&index, "format"),
        "png",
        "index.json must describe PNG outputs"
    );
    assert!(
        array_field(&index, "missing").is_empty(),
        "index.json `missing` must be empty, got {:?}",
        array_field(&index, "missing")
    );
    assert_eq!(
        u64_field(&index, "skipped_empty"),
        0,
        "empty 0x0 frames must not be counted"
    );
    assert_eq!(
        u64_field(&index, "indices_out_of_range"),
        0,
        "no requested index may be out of range"
    );
    assert_eq!(
        u64_field(&index, "written_png"),
        u64::try_from(EXPECTED_PNG_COUNT).expect("count fits u64"),
        "the unpacker must write every requested frame"
    );

    let frames = indexed_frames(&index);
    assert_eq!(
        frames.len(),
        EXPECTED_PNG_COUNT,
        "index.json `images` must list 116 frames"
    );
    let actual: Vec<u32> = frames.iter().map(|frame| frame.index).collect();
    let expected = expected_frame_indices();
    assert_eq!(actual, expected, "frame index sequence must be frozen");
    let unique: BTreeSet<u32> = actual.iter().copied().collect();
    assert_eq!(
        unique.len(),
        EXPECTED_PNG_COUNT,
        "frame indices must not repeat"
    );
    for gap in [416, 417, 418, 419, 566, 567, 568, 569] {
        assert!(
            !unique.contains(&gap),
            "0x0 empty frame {gap} must not appear in index.json"
        );
    }

    let mut file_names: BTreeSet<String> = BTreeSet::new();
    let entries = std::fs::read_dir(&dir).unwrap_or_else(|error| {
        panic!(
            "R59 requires the generated magic frame directory {}, but it is unreadable: {error}. \
             The asset gate fails instead of skipping (dispatch C4).",
            dir.display()
        )
    });
    for entry in entries {
        let path = entry
            .expect("magic frame directory entries must be readable")
            .path();
        let name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .expect("generated asset names must be UTF-8")
            .to_owned();
        file_names.insert(name);
    }
    assert!(
        file_names.remove("index.json"),
        "magic frame directory must contain index.json"
    );
    let expected_names: BTreeSet<String> = expected
        .iter()
        .map(|index| format!("{index:04}.png"))
        .collect();
    assert_eq!(
        file_names.len(),
        EXPECTED_PNG_COUNT,
        "magic frame directory must contain exactly 116 PNGs (plus index.json)"
    );
    assert_eq!(
        file_names, expected_names,
        "PNG file names must be exactly the frozen frame set"
    );
}

#[test]
fn every_indexed_png_matches_its_frozen_sha_and_has_a_real_size() {
    let index = index_json();
    let frames = indexed_frames(&index);
    let dir = magic_dir();
    let mut checked = 0usize;
    for frame in &frames {
        assert!(
            !frame.empty,
            "frame {} is marked empty in index.json but R59 requires it",
            frame.index
        );
        assert!(
            frame.width > 0 && frame.height > 0,
            "frame {} is recorded as 0x0, which R59 forbids",
            frame.index
        );
        assert_eq!(
            frame.png,
            format!("{:04}.png", frame.index),
            "png file name must encode the frame index"
        );
        let bytes = read_required(&dir.join(frame.png));
        assert_eq!(
            sha256_hex(&bytes),
            frame.png_sha256,
            "frame {} on-disk sha256 must equal index.json png_sha256",
            frame.index
        );
        let (width, height) = png_dimensions(&bytes);
        assert!(
            width > 0 && height > 0,
            "frame {} PNG must not be 0x0",
            frame.index
        );
        assert_eq!(
            (width, height),
            (frame.width, frame.height),
            "frame {} PNG dimensions must equal index.json",
            frame.index
        );
        checked += 1;
    }
    assert_eq!(
        checked, EXPECTED_PNG_COUNT,
        "every one of the 116 frames must be verified"
    );
}

#[test]
fn sampled_frames_410_415_570_579_match_the_recorded_disk_hashes() {
    let index = index_json();
    let frames = indexed_frames(&index);
    let dir = magic_dir();
    for (frame_index, expected_sha, expected_width, expected_height) in SAMPLE_FRAMES {
        let frame = frames
            .iter()
            .find(|frame| frame.index == frame_index)
            .unwrap_or_else(|| panic!("sample frame {frame_index} is missing from index.json"));
        assert_eq!(
            frame.png_sha256, expected_sha,
            "sample frame {frame_index} index.json sha256"
        );
        assert_eq!(
            (frame.width, frame.height),
            (expected_width, expected_height),
            "sample frame {frame_index} index.json dimensions"
        );
        let bytes = read_required(&dir.join(format!("{frame_index:04}.png")));
        assert_eq!(
            sha256_hex(&bytes),
            expected_sha,
            "sample frame {frame_index} on-disk sha256 must match the frozen value"
        );
        assert_eq!(
            png_dimensions(&bytes),
            (expected_width, expected_height),
            "sample frame {frame_index} on-disk dimensions must match the frozen value"
        );
    }
}

#[test]
fn frameset_fingerprint_matches_the_frozen_round59_value() {
    let index = index_json();
    let dir = magic_dir();
    let mut lines = Vec::with_capacity(EXPECTED_PNG_COUNT);
    for frame in indexed_frames(&index) {
        let bytes = read_required(&dir.join(frame.png));
        lines.push(format!(
            "{} {} {}",
            frame.png,
            bytes.len(),
            sha256_hex(&bytes)
        ));
    }
    assert_eq!(
        lines.len(),
        EXPECTED_PNG_COUNT,
        "the fingerprint must cover every frame"
    );
    lines.sort();
    let fingerprint = sha256_hex(lines.join("\n").as_bytes());
    assert_eq!(
        fingerprint, FROZEN_FRAMESET_SHA256,
        "the sorted <name> <size> <sha256> fingerprint of the 116 PNGs must be frozen"
    );
}

#[test]
fn resource_magic_wzl_matches_the_index_source_hash() {
    let resource_dir = repo_root().join("resource");
    assert!(
        resource_dir.is_dir(),
        "R59 requires the local `resource/` corpus at {}, but it is missing. \
         Configure the corpus or run on a prepared machine; this gate fails instead of \
         skipping (dispatch C4).",
        resource_dir.display()
    );
    let index = index_json();
    let source = str_field(&index, "source").replace('\\', "/");
    assert_eq!(
        source, "resource/data/magic.wzl",
        "index.json `source` must point at the extracted magic.wzl"
    );
    let bytes = read_required(&repo_root().join(&source));
    assert_eq!(
        u64::try_from(bytes.len()).expect("file size fits u64"),
        FROZEN_WZL_BYTES,
        "magic.wzl byte size must be frozen"
    );
    assert_eq!(
        u64_field(&index, "source_bytes"),
        FROZEN_WZL_BYTES,
        "index.json `source_bytes` must be frozen"
    );
    assert_eq!(
        sha256_hex(&bytes),
        FROZEN_WZL_SHA256,
        "magic.wzl on-disk sha256 must equal the frozen value"
    );
    assert_eq!(
        str_field(&index, "source_sha256"),
        FROZEN_WZL_SHA256,
        "index.json `source_sha256` must be frozen"
    );
}

#[test]
fn embedded_sha256_matches_fips_180_4_vectors() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "empty-input vector"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "single-block vector"
    );
    assert_eq!(
        sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        "two-block vector"
    );
}

/// SHA-256 (FIPS 180-4) so the asset gate can verify on-disk hashes without adding a
/// dependency (R59 procurement scope forbids `Cargo.toml` changes).
fn sha256_hex(data: &[u8]) -> String {
    const ROUND_CONSTANTS: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];
    let mut state: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let bit_length = u64::try_from(data.len()).expect("input length fits u64") * 8;
    let mut message = Vec::with_capacity(data.len() + 72);
    message.extend_from_slice(data);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());
    for chunk in message.chunks_exact(64) {
        let mut schedule = [0u32; 64];
        for (word_index, word) in chunk.chunks_exact(4).enumerate() {
            schedule[word_index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for word_index in 16..64 {
            let sigma0 = schedule[word_index - 15].rotate_right(7)
                ^ schedule[word_index - 15].rotate_right(18)
                ^ (schedule[word_index - 15] >> 3);
            let sigma1 = schedule[word_index - 2].rotate_right(17)
                ^ schedule[word_index - 2].rotate_right(19)
                ^ (schedule[word_index - 2] >> 10);
            schedule[word_index] = schedule[word_index - 16]
                .wrapping_add(sigma0)
                .wrapping_add(schedule[word_index - 7])
                .wrapping_add(sigma1);
        }
        let mut working = state;
        for (round, constant) in ROUND_CONSTANTS.iter().enumerate() {
            let choose = (working[4] & working[5]) ^ (!working[4] & working[6]);
            let majority =
                (working[0] & working[1]) ^ (working[0] & working[2]) ^ (working[1] & working[2]);
            let sum0 = working[0].rotate_right(2)
                ^ working[0].rotate_right(13)
                ^ working[0].rotate_right(22);
            let sum1 = working[4].rotate_right(6)
                ^ working[4].rotate_right(11)
                ^ working[4].rotate_right(25);
            let temp1 = working[7]
                .wrapping_add(sum1)
                .wrapping_add(choose)
                .wrapping_add(*constant)
                .wrapping_add(schedule[round]);
            let temp2 = sum0.wrapping_add(majority);
            working[7] = working[6];
            working[6] = working[5];
            working[5] = working[4];
            working[4] = working[3].wrapping_add(temp1);
            working[3] = working[2];
            working[2] = working[1];
            working[1] = working[0];
            working[0] = temp1.wrapping_add(temp2);
        }
        for (slot, value) in state.iter_mut().zip(working) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut digest = String::with_capacity(64);
    for value in state {
        write!(digest, "{value:08x}").expect("writing a hex digest into a String cannot fail");
    }
    digest
}
