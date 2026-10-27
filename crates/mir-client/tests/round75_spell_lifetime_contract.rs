//! Round 75 A：149 `S.ObjectSpell` 瞬态精灵 vs Crystal `SpellObject` 对象寿命的**差异合同**。
//!
//! 本文件是测试作者与差异合同冻结（**不是**生产寿命修复，也**不是**“严格等价 Crystal”）：
//!
//! * `CrystalSpellClock` 是测试私有的参考模型，只复读
//!   `github/Crystal/Client/MirObjects/SpellObject.cs:288–302` 的
//!   `NextMotion` 量化/`>=` 等号进/Repeat 回卷/`Ended` 语义；它不是生产能力，
//!   本轮生产 `crates/**/src/**` 零改动；
//! * 生产侧读取真实 `149 ObjectSpell → 26 ObjectRemove → 149 ObjectSpell` wire 链
//!   （`encode_frame → decode_server_packet → apply_packet_at`）后的
//!   `WorldSnapshot::spell_events` / `spell_fields`，再调用生产
//!   `spellfx::draw_sprites`（`crates/mir-client/src/ui/spellfx.rs:287–389`）实画整帧；
//! * 双方预期分别固定、不要求相等：参考模型 `frame` 与产品 `elapsed` 索引是两条独立
//!   expected，禁把差异当等价；确证的现状差异（26 只删持久登记、瞬态事件仍绘制）只作为
//!   “差异合同/残影现状”记录，**不得**称已修复寿命；
//! * 独立 oracle：本文件内合成 `magic.wzl`/`magic.wzx`、`magic2.wzl`/`magic2.wzx`，
//!   从原始 RGB565 像素按 `Frame::blit_add` 的文档公式手算整帧 RGBA，**不调用**
//!   生产 `blit/blit_add/add_channel/blend_channel/effect_image`；
//! * 整帧金样：`target/round75/a/attempt-1/frozen/expected.json` 由独立 Python oracle
//!   （bundled python，`hashlib`）在**任何 actual 运行之前**冻结（UTC
//!   2026-10-27T19:31:57Z），本文件常量逐字复制该文件；
//!
//! 运行（缺 `legacy` feature 时合成图库不可读，夹具断言以诊断文本失败而非 SKIP）：
//!
//! ```text
//! cargo test -p mir-client --features mir-render/legacy --test round75_spell_lifetime_contract -- --test-threads=1
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use mir_client::net::{SpellEvent, WorldSnapshot};
use mir_client::ui::UiState;
use mir_client::ui::spellfx;
use mir_core::enums::{MirDirection, Spell};
use mir_core::{Language, Point};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{ObjectRemove, ObjectSpell, decode_server_packet};
use mir_render::{EntityLibraries, Frame, MapScene, TileLibraries, Viewport};
use mir_world::{Map, MapCell, MapVersion};

// ---------------------------------------------------------------------------
// 冻结金样常量（复制自 target/round75/a/attempt-1/frozen/expected.json，
// 生成 UTC 2026-10-27T19:31:57Z，早于本文件任何 actual 运行）
// ---------------------------------------------------------------------------

const FROZEN_G1_SHA: &str = "8b9e6c364933c732d894316a95b8bc772d1dfa8527330bb7766bbddb0aedd2e9";
const FROZEN_G1_COVERED: usize = 9;
const FROZEN_G2_SHA: &str = "2daeabaad7a6c4c888d4987a5de6fbbfa1de087ef4fbe323555cbed6ebf34934";
const FROZEN_G2_COVERED: usize = 4;
const FROZEN_G4_FW0_SHA: &str = "8b9e6c364933c732d894316a95b8bc772d1dfa8527330bb7766bbddb0aedd2e9";
const FROZEN_G4_FW0_COVERED: usize = 9;
const FROZEN_G4_FW1999_SHA: &str =
    "2daeabaad7a6c4c888d4987a5de6fbbfa1de087ef4fbe323555cbed6ebf34934";
const FROZEN_G4_FW1999_COVERED: usize = 4;
const FROZEN_G4_FW2000_SHA: &str =
    "2daeabaad7a6c4c888d4987a5de6fbbfa1de087ef4fbe323555cbed6ebf34934";
const FROZEN_G4_FW2000_COVERED: usize = 4;
const FROZEN_G4_FW3999_SHA: &str =
    "059aee63bec3563251754e80fed16559afcfc9399689695cfdaf82b712436d99";
const FROZEN_G4_FW3999_COVERED: usize = 4;
const FROZEN_G4_PC0_SHA: &str = "7f967ae97639fce830223511832f493aa81f2639d84b495f38546e61deca94d4";
const FROZEN_G4_PC0_COVERED: usize = 4;
const FROZEN_G4_PC1999_SHA: &str =
    "f21f6dd4be9e9ae1043cf875e66d98cc930261a01a2ca1a374576386ca454344";
const FROZEN_G4_PC1999_COVERED: usize = 4;
const FROZEN_G4_PC2000_SHA: &str =
    "f21f6dd4be9e9ae1043cf875e66d98cc930261a01a2ca1a374576386ca454344";
const FROZEN_G4_PC2000_COVERED: usize = 4;
const FROZEN_G4_PC3999_SHA: &str =
    "eb938cc07921c770bef213e31906cb3aa508bb49e1127a407abc4911e217e15d";
const FROZEN_G4_PC3999_COVERED: usize = 4;
const FROZEN_G4_EMPTY_SHA: &str =
    "a7aff0f8e3f44663c369e1e75a981b90b66949586d71035771921871d67e334e";
const FROZEN_G4_EMPTY_COVERED: usize = 0;
const FROZEN_G5_SHA: &str = "455f8769f67277a62f13d3a43d52c27c6ebf27c213880c95f99c966ee329a0e7";
const FROZEN_G5_COVERED: usize = 8;
const FROZEN_G6_SHA: &str = "41be5fca415f0582991718747d9bd55cfc47e00e8f8afbd5bd12cf8fbacea1df";
const FROZEN_G6_COVERED: usize = 17;
const FROZEN_MAGIC_WZL_SHA: &str =
    "f678a3485ac907c547b5f071cc10bc8135513ecb264b409b7ca4b74c790f4820";
const FROZEN_MAGIC_WZL_BYTES: usize = 274;
const FROZEN_MAGIC_WZX_SHA: &str =
    "91f6e863ac7aaa5f27919546a090105a2faf485dcbdcae941be08834f05c29a3";
const FROZEN_MAGIC_WZX_BYTES: usize = 6592;
const FROZEN_MAGIC2_WZL_SHA: &str =
    "f2ccbe57336be8a516e48ea7706cf13805b5007279f6ab62b909f615aa968e3d";
const FROZEN_MAGIC2_WZL_BYTES: usize = 748;
const FROZEN_MAGIC2_WZX_SHA: &str =
    "60debbaa28d44c19226d5c075fd0237f00f372001f8baac2aaba6d268ae0794a";
const FROZEN_MAGIC2_WZX_BYTES: usize = 6728;

// ---------------------------------------------------------------------------
// 独立几何常量：800×600 视口（Crystal `MapControl` 的 CellWidth/CellHeight 语义）
// ---------------------------------------------------------------------------

/// 测试帧宽：与 `Viewport::DEFAULT` 一致，但由本文件独立写死（oracle 不读生产常量）。
const FRAME_WIDTH: u32 = 800;
/// 测试帧高。
const FRAME_HEIGHT: u32 = 600;
/// 帧缓冲字节数。
const FRAME_BYTES: usize = (FRAME_WIDTH as usize) * (FRAME_HEIGHT as usize) * 4;
/// 测试画布背景：不透明黑（与生产帧缓冲“不透明画布”一致）。
const BACKGROUND: [u8; 4] = [0, 0, 0, 255];
/// 格原点：`OffSetX = 800 / 2 / 48 = 8`、`OffSetY = 600 / 2 / 32 - 1 = 8`，
/// 故中心格原点 `(8 * 48, 8 * 32) = (384, 256)`。
const VIEWPORT_ORIGIN: (i32, i32) = (384, 256);
/// 格宽（Crystal `MapControl.CellWidth`）。
const CELL_WIDTH: i32 = 48;
/// 格高（Crystal `MapControl.CellHeight`）。
const CELL_HEIGHT: i32 = 32;
/// 合成帧边长：2×2（通用帧）。
const SIZE_2X2: (u16, u16) = (2, 2);
/// 合成帧边长：3×3（`1630`，用于证明不按尺寸居中）。
const SIZE_3X3: (u16, u16) = (3, 3);

// ---------------------------------------------------------------------------
// 合成帧表（与冻结 expected.json 的 tables 逐帧一致）
// ---------------------------------------------------------------------------

/// 一张合成帧：帧号、边长、帧头 i16 有符号偏移、RGB565 四角原始值
/// （左上、右上、左下、右下）。
#[derive(Clone, Copy, Debug)]
struct MagicFrame {
    index: u32,
    size: (u16, u16),
    offset: (i16, i16),
    quad: [u16; 4],
}

impl MagicFrame {
    const fn new(index: u32, size: (u16, u16), offset: (i16, i16), quad: [u16; 4]) -> Self {
        Self {
            index,
            size,
            offset,
            quad,
        }
    }
}

/// 合成 `magic.wzl` 帧表（`FxKind::Magic`，起始帧 0）：Crystal `FireWall` 六帧
/// `1630..=1635`（`SpellObject.cs:49–64`，120ms/帧）。
///
/// | 帧 | 边长 | offset | 产品采样 age |
/// |---|---|---|---|
/// | 1630 | 3×3 | (-3,5) | 0 / 66（G1/G4） |
/// | 1631 | 2×2 | (4,-2) | 120（G5/G6 边界） |
/// | 1632 | 2×2 | (-1,-1) | 240（G6） |
/// | 1633 | 2×2 | (50,-30) | 3999（G4 屏内裁剪） |
/// | 1634 | 2×2 | (7,6) | 1999 / 2000 / 2000（G2/G4 内部采样点） |
/// | 1635 | 2×2 | (-5,4) | 未被产品窗采样（六帧完整性） |
static MAGIC_FRAMES: [MagicFrame; 6] = [
    MagicFrame::new(1_630, SIZE_3X3, (-3, 5), [0xF800; 4]),
    MagicFrame::new(1_631, SIZE_2X2, (4, -2), [0x07E0; 4]),
    MagicFrame::new(1_632, SIZE_2X2, (-1, -1), [0x001F; 4]),
    MagicFrame::new(1_633, SIZE_2X2, (50, -30), [0xFFE0; 4]),
    MagicFrame::new(1_634, SIZE_2X2, (7, 6), [0xF81F; 4]),
    MagicFrame::new(1_635, SIZE_2X2, (-5, 4), [0x07FF; 4]),
];

/// 合成 `magic2.wzl` 帧表（`FxKind::Swing`，起始帧 0）：Crystal `PoisonCloud` 二十帧
/// `1650..=1669`（`SpellObject.cs:49–64` 的 `Libraries.Magic2`/1650/120/20）。
///
/// 第 k 帧单色 RGB565：`r5 = 8 + k`、`g6 = 4 + k`、`b5 = 1 + k`；`k` 不同则颜色不同且
/// 非黑，故采样帧 `1650/1663/1666` 必然异色。
fn magic2_frames() -> Vec<MagicFrame> {
    (0..20u16)
        .map(|k| {
            let raw = (((8 + k) & 0x1F) << 11) | (((4 + k) & 0x3F) << 5) | ((1 + k) & 0x1F);
            let x = i16::try_from(k % 7).expect("k 小") - 3;
            let y = i16::try_from(k % 5).expect("k 小") - 2;
            MagicFrame::new(1_650 + u32::from(k), SIZE_2X2, (x, y), [raw; 4])
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 夹具：进程唯一临时目录 + 合成 WZL/WZX（真实 `EntityLibraries` 读取链）
// ---------------------------------------------------------------------------

/// 进程唯一临时目录（不读 `resource`/`game-assets`，也不与其它测试共享路径）。
///
/// 优先系统临时目录；被环境策略拒绝写时回退到 `target/round75/a/tmp/`（测试产物目录）。
fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let name = format!("mir-next-r75-a-{tag}-{}-{nanos}", std::process::id());
    let candidates = [
        std::env::temp_dir().join(&name),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/round75/a/tmp")
            .join(&name),
    ];
    for dir in candidates {
        if fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }
    panic!("无法创建进程唯一临时目录（系统临时目录与 target/round75/a/tmp 均不可写）");
}

/// 合成帧的 RGB565 像素值：3×3 平铺帧全用 `quad[0]`，2×2 帧按四角取 `quad`。
fn frame_value(spec: &MagicFrame, column: u16, row: u16) -> u16 {
    if spec.size == SIZE_3X3 {
        return spec.quad[0];
    }
    let slot = usize::from(row) * 2 + usize::from(column);
    spec.quad[slot]
}

/// 按 Crystal WZL 的行序构造未压缩像素：文件第 0 行 = 图像最下面一行，
/// 每行按 4 字节对齐（`WidthBytes = ((宽 × 16 + 31) >> 5) * 4`）。
fn build_raw(spec: &MagicFrame) -> Vec<u8> {
    let (width, height) = spec.size;
    let row_bytes =
        usize::try_from((((u32::from(width) * 16) + 31) >> 5) * 4).expect("行字节必须放得进 usize");
    let mut raw = Vec::with_capacity(row_bytes * usize::from(height));
    for row in (0..height).rev() {
        let start = raw.len();
        for column in 0..width {
            raw.extend_from_slice(&frame_value(spec, column, row).to_le_bytes());
        }
        raw.resize(start + row_bytes, 0);
    }
    raw
}

/// 按 `.wzl`/`.wzx` 二进制格式构造一对文件内容。
///
/// `.wzl`：48 字节头 + 每条记录 16 字节记录头（格式 5=RGB565、宽高、i16 偏移、
/// u32 zlib 长度）+ zlib 数据；`.wzx`：48 字节头（`[44..48]` 数量）+ u32 偏移表，
/// 偏移为 0 表示「没有这张图」（缺帧）。
fn build_wzl(frames: &[MagicFrame]) -> (Vec<u8>, Vec<u8>) {
    let count = frames
        .iter()
        .map(|spec| spec.index)
        .max()
        .expect("合成帧表不能为空")
        + 1;
    let mut wzl = vec![0u8; 48];
    let mut table = vec![0u32; usize::try_from(count).expect("帧数必须放得进 usize")];
    for spec in frames {
        let raw = build_raw(spec);
        let compressed = zlib_store(&raw);
        let (width, height) = spec.size;
        table[usize::try_from(spec.index).expect("帧号必须放得进 usize")] =
            u32::try_from(wzl.len()).expect(".wzl 偏移必须放得进 u32");
        wzl.push(5); // 格式 5 = RGB565
        wzl.extend_from_slice(&[0, 0, 0]); // Crystal 直接跳过的 3 个保留字节
        wzl.extend_from_slice(&width.to_le_bytes());
        wzl.extend_from_slice(&height.to_le_bytes());
        wzl.extend_from_slice(&spec.offset.0.to_le_bytes());
        wzl.extend_from_slice(&spec.offset.1.to_le_bytes());
        wzl.extend_from_slice(
            &u32::try_from(compressed.len())
                .expect("压缩长度必须放得进 u32")
                .to_le_bytes(),
        );
        wzl.extend_from_slice(&compressed);
    }
    let mut wzx = vec![0u8; 48];
    wzx[44..48].copy_from_slice(&count.to_le_bytes());
    for offset in table {
        wzx.extend_from_slice(&offset.to_le_bytes());
    }
    (wzl, wzx)
}

/// 构造 zlib 流：只用 DEFLATE 存储块（BTYPE=0），不依赖 `flate2`。
fn zlib_store(payload: &[u8]) -> Vec<u8> {
    // CMF=0x78（deflate, 32K 窗口），FLG=0x01 使 (CMF<<8|FLG) % 31 == 0。
    let mut out = vec![0x78, 0x01];
    let mut chunks = payload.chunks(usize::from(u16::MAX)).peekable();
    while let Some(chunk) = chunks.next() {
        let final_block = chunks.peek().is_none();
        out.push(u8::from(final_block)); // BFINAL + BTYPE=00（存储块）
        let len = u16::try_from(chunk.len()).expect("存储块长度 <= u16::MAX");
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&adler32(payload).to_be_bytes());
    out
}

/// zlib 尾部校验：RFC 1950 的 Adler-32。
fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let (mut low, mut high) = (1u32, 0u32);
    for &byte in data {
        low = (low + u32::from(byte)) % MOD;
        high = (high + low) % MOD;
    }
    (high << 16) | low
}

/// 写一个合成图库（`{stem}.wzl` + `{stem}.wzx`）。
fn write_library(directory: &Path, stem: &str, frames: &[MagicFrame]) {
    let (wzl, wzx) = build_wzl(frames);
    fs::write(directory.join(format!("{stem}.wzl")), wzl).expect("写合成 wzl");
    fs::write(directory.join(format!("{stem}.wzx")), wzx).expect("写合成 wzx");
}

/// 用空 32×32 地图 + 目录内实体图库构造 `MapScene`（`Viewport::DEFAULT`）。
fn scene_in_dir(directory: &Path) -> MapScene {
    let map = Map {
        width: 32,
        height: 32,
        cells: vec![MapCell::default(); 32 * 32],
        file_name: "r75-empty".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    };
    MapScene::from_parts_with_entities(
        map,
        TileLibraries::empty("."),
        Viewport::DEFAULT,
        EntityLibraries::open_dir(directory),
    )
}

/// 独立 oracle 的帧来源（决定图库与帧号空间）。
#[derive(Clone, Copy, Debug)]
enum SpriteSource {
    /// `magic.wzl`（`FxKind::Magic`，起始帧 0）。
    Magic,
    /// `magic2.wzl`（`FxKind::Swing`，起始帧 0）。
    Magic2,
}

/// oracle 的一次 Add 绘制：图库 + 帧号 + 格锚点 + 是否消费当帧 offset。
#[derive(Clone, Copy, Debug)]
struct OracleOp {
    source: SpriteSource,
    frame: u32,
    x: i32,
    y: i32,
    apply_offset: bool,
}

impl OracleOp {
    const fn add(source: SpriteSource, frame: u32, x: i32, y: i32) -> Self {
        Self {
            source,
            frame,
            x,
            y,
            apply_offset: true,
        }
    }
}

/// 合成夹具：两张图库的帧表 + 落盘目录。
struct Fixture {
    magic: Vec<MagicFrame>,
    magic2: Vec<MagicFrame>,
    dir: PathBuf,
}

impl Fixture {
    /// 写夹具并用独立 Python 冻结的 SHA/字节数校验（夹具字节本身也是冻结 expected）。
    fn create(tag: &str) -> Self {
        let magic = MAGIC_FRAMES.to_vec();
        let magic2 = magic2_frames();
        let (magic_wzl, magic_wzx) = build_wzl(&magic);
        let (magic2_wzl, magic2_wzx) = build_wzl(&magic2);
        assert_eq!(
            magic_wzl.len(),
            FROZEN_MAGIC_WZL_BYTES,
            "冻结 magic.wzl 字节数"
        );
        assert_eq!(
            magic_wzx.len(),
            FROZEN_MAGIC_WZX_BYTES,
            "冻结 magic.wzx 字节数"
        );
        assert_eq!(
            magic2_wzl.len(),
            FROZEN_MAGIC2_WZL_BYTES,
            "冻结 magic2.wzl 字节数"
        );
        assert_eq!(
            magic2_wzx.len(),
            FROZEN_MAGIC2_WZX_BYTES,
            "冻结 magic2.wzx 字节数"
        );
        assert_eq!(
            sha256_hex(&magic_wzl),
            FROZEN_MAGIC_WZL_SHA,
            "冻结 magic.wzl SHA"
        );
        assert_eq!(
            sha256_hex(&magic_wzx),
            FROZEN_MAGIC_WZX_SHA,
            "冻结 magic.wzx SHA"
        );
        assert_eq!(
            sha256_hex(&magic2_wzl),
            FROZEN_MAGIC2_WZL_SHA,
            "冻结 magic2.wzl SHA"
        );
        assert_eq!(
            sha256_hex(&magic2_wzx),
            FROZEN_MAGIC2_WZX_SHA,
            "冻结 magic2.wzx SHA"
        );
        let dir = unique_dir(tag);
        write_library(&dir, "magic", &magic);
        write_library(&dir, "magic2", &magic2);
        Self { magic, magic2, dir }
    }

    /// 用夹具目录构造场景（真实 `EntityLibraries::open_dir` 读取链）。
    fn scene(&self) -> MapScene {
        scene_in_dir(&self.dir)
    }

    /// 帧表查询（缺帧号直接 panic，防止夹具与断言漂移）。
    fn spec(&self, source: SpriteSource, index: u32) -> &MagicFrame {
        let table: &[MagicFrame] = match source {
            SpriteSource::Magic => &self.magic,
            SpriteSource::Magic2 => &self.magic2,
        };
        table
            .iter()
            .find(|spec| spec.index == index)
            .expect("合成帧表必须覆盖该帧号")
    }

    /// 独立整帧 oracle：从背景起按 `ops` 顺序逐像素手算（不调用生产混合函数）。
    fn oracle(&self, ops: &[OracleOp]) -> Vec<u8> {
        let width_usize = usize::try_from(FRAME_WIDTH).expect("oracle 宽");
        let height_usize = usize::try_from(FRAME_HEIGHT).expect("oracle 高");
        let mut pixels = vec![0u8; width_usize * height_usize * 4];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&BACKGROUND);
        }
        for op in ops {
            let spec = self.spec(op.source, op.frame);
            // 偏移先于裁剪；`saturating_add` 与生产入口的收口语义一致。
            let left = if op.apply_offset {
                op.x.saturating_add(i32::from(spec.offset.0))
            } else {
                op.x
            };
            let top = if op.apply_offset {
                op.y.saturating_add(i32::from(spec.offset.1))
            } else {
                op.y
            };
            let (width, height) = spec.size;
            for row in 0..height {
                for column in 0..width {
                    let source = expand565(frame_value(spec, column, row));
                    let alpha = fixture_alpha(source);
                    if alpha == 0 {
                        // alpha=0：整点跳过，不动 RGB，也不动目标 alpha 字节。
                        continue;
                    }
                    let x = i64::from(left) + i64::from(column);
                    let y = i64::from(top) + i64::from(row);
                    if x < 0 || y < 0 || x >= i64::from(FRAME_WIDTH) || y >= i64::from(FRAME_HEIGHT)
                    {
                        continue;
                    }
                    let offset = (usize::try_from(y).expect("y") * width_usize
                        + usize::try_from(x).expect("x"))
                        * 4;
                    let target = &mut pixels[offset..offset + 4];
                    for (channel, value) in target[..3].iter_mut().enumerate() {
                        let contribution =
                            (u32::from(source[channel]) * u32::from(alpha) + 127) / 255;
                        *value = u8::try_from((u32::from(*value) + contribution).min(255))
                            .unwrap_or(255);
                    }
                    target[3] = 255;
                }
            }
        }
        pixels
    }
}

/// 删除进程唯一临时目录（失败不影响断言结论）。
fn cleanup(directory: &Path) {
    let _ = fs::remove_dir_all(directory);
}

// ---------------------------------------------------------------------------
// 独立 oracle 的像素/帧辅助
// ---------------------------------------------------------------------------

/// RGB565 → RGBA 的独立展开（只左移，不复制低位）。
fn expand565(value: u16) -> [u8; 4] {
    let red = ((value >> 11) & 0x1F) << 3;
    let green = ((value >> 5) & 0x3F) << 2;
    let blue = (value & 0x1F) << 3;
    [red as u8, green as u8, blue as u8, 255]
}

/// 解码后逐像素 alpha：`remove_black_all` 把 RGB 全 0 置 alpha=0；WZL 其余像素 255。
fn fixture_alpha(color: [u8; 4]) -> u8 {
    if color[0] == 0 && color[1] == 0 && color[2] == 0 {
        0
    } else {
        255
    }
}

/// 新建并用指定背景清屏（背景 alpha 必须 255，与生产帧缓冲“不透明画布”一致）。
fn frame_with(background: [u8; 4]) -> Frame {
    assert_eq!(background[3], 255, "测试背景必须是不透明画布");
    let mut frame = Frame::new(FRAME_WIDTH, FRAME_HEIGHT);
    frame.clear(background);
    frame
}

/// 取帧上像素（负坐标视为透明，便于断言「不该画」的位置）。
fn pixel_at(frame: &Frame, x: i32, y: i32) -> [u8; 4] {
    let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
        return [0, 0, 0, 0];
    };
    frame.pixel(x, y).unwrap_or([0, 0, 0, 0])
}

/// 非背景像素数量。
fn covered_count(frame: &Frame) -> usize {
    frame
        .pixels
        .chunks_exact(4)
        .filter(|pixel| *pixel != BACKGROUND)
        .count()
}

/// 整帧断言：失败时报告首个差异像素的坐标/实际值/期望值（`#[track_caller]` 让首败
/// 行号落在调用测试行，作为红相位「首个失败断言」证据）。
#[track_caller]
fn assert_frame_matches(frame: &Frame, expected: &[u8], label: &str) {
    assert_eq!(
        frame.pixels.len(),
        expected.len(),
        "{label}：整帧缓冲长度不一致"
    );
    let width = frame.width;
    for (index, (actual, want)) in frame
        .pixels
        .chunks_exact(4)
        .zip(expected.chunks_exact(4))
        .enumerate()
    {
        if actual != want {
            let x = u32::try_from(index % usize::try_from(width).expect("width")).expect("x");
            let y = u32::try_from(index / usize::try_from(width).expect("width")).expect("y");
            let mut actual_value = [0u8; 4];
            actual_value.copy_from_slice(actual);
            let mut expected_value = [0u8; 4];
            expected_value.copy_from_slice(want);
            panic!(
                "{label}：整帧 RGBA 首个差异像素 ({x},{y}) actual={actual_value:?} \
                 expected={expected_value:?}（独立 oracle；首败断言）"
            );
        }
    }
}

/// 断言整帧只剩背景（零落屏）。
#[track_caller]
fn assert_only_background(frame: &Frame, label: &str) {
    for (index, pixel) in frame.pixels.chunks_exact(4).enumerate() {
        if pixel != BACKGROUND {
            let x = u32::try_from(index % usize::try_from(frame.width).expect("width")).expect("x");
            let y = u32::try_from(index / usize::try_from(frame.width).expect("width")).expect("y");
            let mut value = [0u8; 4];
            value.copy_from_slice(pixel);
            panic!("{label}：({x},{y}) 出现意外像素 actual={value:?} expected=背景{BACKGROUND:?}");
        }
    }
}

/// 整帧金样断言：冻结 expected 的字节数/SHA 与覆盖数必须逐项命中。
///
/// 首败信息同时给出 actual SHA 与 expected SHA（红相位证据）。
#[track_caller]
fn assert_frozen_frame(frame: &Frame, sha: &str, covered: usize, label: &str) {
    assert_eq!(frame.pixels.len(), FRAME_BYTES, "{label}：整帧字节数");
    assert_eq!(covered_count(frame), covered, "{label}：非背景覆盖像素数");
    let actual = sha256_hex(&frame.pixels);
    assert_eq!(
        actual, sha,
        "{label}：整帧 SHA256 必须等于预冻结 expected（actual={actual}）"
    );
}

/// 格原点（视口中心格原点 + 格偏移），独立于生产 `world_to_screen` 实现。
fn cell_origin(center: Point, cell: Point) -> (i32, i32) {
    (
        VIEWPORT_ORIGIN.0 + (cell.x - center.x) * CELL_WIDTH,
        VIEWPORT_ORIGIN.1 + (cell.y - center.y) * CELL_HEIGHT,
    )
}

/// 测试中心格（`world_to_screen` 的相机中心）。
fn center_point() -> Point {
    Point::new(10, 10)
}

// ---------------------------------------------------------------------------
// 真实 wire 链与事件辅助
// ---------------------------------------------------------------------------

/// 走真实 `encode_frame → decode_server_packet` 一帧往返。
fn through_wire(packet: &ServerPacket) -> ServerPacket {
    let frame = packet.encode_frame();
    assert!(frame.len() > 4, "编码帧必须带载荷");
    let index = i16::from_le_bytes([frame[2], frame[3]]);
    decode_server_packet(index, &frame[4..]).expect("服务器帧必须能解码回 ServerPacket")
}

/// 构造 149 `ObjectSpell`（`S.ObjectSpell`）并经真实 wire 链写入快照。
fn push_object_spell_via_wire(
    snapshot: &mut WorldSnapshot,
    object_id: u32,
    spell: Spell,
    location: Point,
    now_ms: u64,
) {
    let packet = through_wire(&ServerPacket::ObjectSpell(ObjectSpell {
        object_id,
        location,
        spell,
        direction: MirDirection::Down,
        param: false,
    }));
    snapshot.apply_packet_at(&packet, now_ms);
}

/// 构造 26 `ObjectRemove` 并经真实 wire 链写入快照。
fn push_object_remove_via_wire(snapshot: &mut WorldSnapshot, object_id: u32, now_ms: u64) {
    let packet = through_wire(&ServerPacket::ObjectRemove(ObjectRemove { object_id }));
    snapshot.apply_packet_at(&packet, now_ms);
}

/// 生产同款 UI 时钟状态。
fn ui_state() -> UiState {
    UiState::new(Language::ZhCn)
}

/// 生产同款同步：`ui.time_millis = now; ui.sync(snapshot)`。
fn sync_at(ui: &mut UiState, snapshot: &WorldSnapshot, now_ms: i64) {
    ui.time_millis = now_ms;
    ui.sync(snapshot);
}

/// 调生产绘制入口 `spellfx::draw_sprites`。
fn draw_at(
    frame: &mut Frame,
    scene: &mut MapScene,
    events: &[SpellEvent],
    center: Point,
    now_ms: i64,
) {
    spellfx::draw_sprites(frame, events, scene, center, now_ms);
}

// ---------------------------------------------------------------------------
// 测试私有 CrystalSpellClock 参考模型（SpellObject.cs:288–302 的字面复读）
// ---------------------------------------------------------------------------

/// Crystal `SpellObject` 的 `NextMotion`/Repeat/Ended 参考模型（**测试私有**，非生产能力）。
///
/// * `load`：`Repeat = true; Ended = false;`，`NextMotion = start + interval;`
///   `NextMotion -= NextMotion % 100;`（`SpellObject.cs:288–302`）；
/// * `process`：`if (now >= next)` 才推进一帧（等号进）；Repeat 回卷
///   `frame = 0; ended = true;`；`next = now + interval`；不追赶、不 `SkipFrames`；
/// * `visible`：`!removed && (repeat || frame < count)`：非 Repeat 越帧只隐藏，
///   **不等于**对象删除（`SpellObject.cs:328–339`）；
/// * `remove`：仅把对象标记为移除，`ended` 不变。
#[derive(Clone, Copy, Debug)]
struct CrystalSpellClock {
    frame: u32,
    count: u32,
    interval: i64,
    next: i64,
    repeat: bool,
    ended: bool,
    removed: bool,
}

impl CrystalSpellClock {
    /// 复读 `SpellObject.Load` 的排期初值（Repeat/Ended 与 `NextMotion` 量化）。
    fn load(start: i64, count: u32, interval: i64, repeat: bool) -> Self {
        let mut next = start.saturating_add(interval);
        next -= next % 100;
        Self {
            frame: 0,
            count,
            interval,
            next,
            repeat,
            ended: false,
            removed: false,
        }
    }

    /// 复读 `SpellObject.Process`：`now >= next` 等号进，单次推进，回卷置 `ended`。
    fn process(&mut self, now: i64) {
        if now >= self.next {
            self.frame = self.frame.saturating_add(1);
            if self.frame >= self.count && self.repeat {
                self.frame = 0;
                self.ended = true;
            }
            self.next = now.saturating_add(self.interval);
        }
    }

    /// 复读 `Draw` 的可见性判定：Repeat 对象恒可见，非 Repeat 越帧只隐藏。
    fn visible(&self) -> bool {
        !self.removed && (self.repeat || self.frame < self.count)
    }

    /// 复读 26 `Remove()`：只标记移除，不改 `ended`。
    fn remove(&mut self) {
        self.removed = true;
    }
}

/// Crystal `Effect` 的排期参考（`Effect.cs:38–61/81–96`）：起点排期 + 严格 `>` 才进。
///
/// 仅用于 G7 与 `SpellObject` 的**不同比较符**对照，防止把两者当同一时钟。
#[derive(Clone, Copy, Debug)]
struct EffectClock {
    frame: i64,
    count: u32,
    duration: i64,
    start: i64,
    next: i64,
}

impl EffectClock {
    /// 起点排期：`NextFrame = Start + (Duration / Count) * (CurrentFrame + 1)`，首帧即排期。
    const fn load(start: i64, duration: i64, count: u32) -> Self {
        let step = duration / count as i64;
        Self {
            frame: 0,
            count,
            duration,
            start,
            next: start + step,
        }
    }

    /// `if (CMain.Time <= NextFrame) return;` 严格 `>` 才进入下一帧。
    fn process(&mut self, now: i64) {
        if now <= self.next {
            return;
        }
        self.frame += 1;
        let step = self.duration / i64::from(self.count);
        self.next = self.start + step * (self.frame + 1);
    }
}

// ---------------------------------------------------------------------------
// 自包含 SHA-256（FIPS 180-4，无新依赖；与冻结 expected 的 hashlib 对齐）
// ---------------------------------------------------------------------------

/// SHA-256（FIPS 180-4）十六进制摘要。
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
        use std::fmt::Write as _;
        write!(digest, "{value:08x}").expect("writing a hex digest into a String cannot fail");
    }
    digest
}

// ---------------------------------------------------------------------------
// G1–G7：差异合同（参考模型 / 生产实画 各自固定 expected，不要求相等）
// ---------------------------------------------------------------------------

/// G1：初始量化与等号进。
///
/// * 参考 `FireWall` `start=1234` → `next=1300`；`process(1299)` 帧 0；`process(1300)`
///   帧 1、`next=1420`（等号进）；
/// * 生产真实 149 wire `FireWall` `started_ms=1234`，draw at 1300 → age 66 → `Magic 1630`；
/// * 两条 expected 分别固定：参考帧 1 在 Crystal 是 `1631`，产品是 `1630`，**不要求相等**。
#[test]
fn r75_g1_initial_quantization_and_equality() {
    let fixture = Fixture::create("g1");
    let mut scene = fixture.scene();
    let center = center_point();
    let cell = Point::new(10, 10);
    let (x, y) = cell_origin(center, cell);

    // 1) 参考模型轨迹（测试私有，非生产能力）。
    let mut clock = CrystalSpellClock::load(1_234, 6, 120, true);
    assert_eq!(clock.next, 1_300, "G1：start=1234 + 120 量化到 1300");
    assert_eq!((clock.frame, clock.ended), (0, false));
    clock.process(1_299);
    assert_eq!(
        (clock.frame, clock.next),
        (0, 1_300),
        "G1：1299 未到点，帧 0、next 不动"
    );
    clock.process(1_300);
    assert_eq!(
        (clock.frame, clock.next),
        (1, 1_420),
        "G1：1300 等号进，帧 1、next=1420"
    );
    let crystal_reference_index = 1_630 + clock.frame;
    assert_eq!(
        crystal_reference_index, 1_631,
        "G1：参考帧 1 -> Crystal magic.wzl[1631]"
    );

    // 2) 生产：真实 wire 149 -> apply -> draw_sprites 整帧。
    let mut snapshot = WorldSnapshot::new_session();
    push_object_spell_via_wire(&mut snapshot, 9_001, Spell::FireWall, cell, 1_234);
    assert_eq!(snapshot.spell_events.len(), 1);
    assert_eq!(snapshot.spell_fields.len(), 1);
    assert_eq!(snapshot.spell_events[0].started_ms, 1_234);
    assert!(snapshot.spell_events[0].from_object_spell);
    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, 1_300);
    let mut frame = frame_with(BACKGROUND);
    draw_at(
        &mut frame,
        &mut scene,
        &snapshot.spell_events,
        center,
        1_300,
    );
    let expected = fixture.oracle(&[OracleOp::add(SpriteSource::Magic, 1_630, x, y)]);
    assert_frame_matches(&frame, &expected, "G1 生产 age=66 magic.wzl[1630]");
    assert_frozen_frame(&frame, FROZEN_G1_SHA, FROZEN_G1_COVERED, "G1");

    // 3) 差异合同：参考 1631 与产品 1630 各自固定、必须不同；不得写成等价。
    let reference = fixture.oracle(&[OracleOp::add(SpriteSource::Magic, 1_631, x, y)]);
    assert_ne!(
        frame.pixels, reference,
        "G1：参考 1631 与产品 1630 必须不同（差异合同，不要求相等）"
    );
    assert_ne!(FROZEN_G1_SHA, sha256_hex(&reference));
    cleanup(&fixture.dir);
}

/// G2：稀疏推进只走一帧（生产 `elapsed` 与 Crystal `NextMotion` 排期不同）。
///
/// * 参考 `start=0` → `next=100`；只 `process(2000)` → 帧 1、`next=2120`；再
///   `process(2120)` → 帧 2、`next=2240`；
/// * 生产 `age=2000` → `Magic 1634`；
/// * 参考帧 1（`1631`）与产品 `1634` 分别固定 expected，**严禁让差异必须相等**。
#[test]
fn r75_g2_sparse_process_is_one_step() {
    let fixture = Fixture::create("g2");
    let mut scene = fixture.scene();
    let center = center_point();
    let cell = Point::new(10, 10);
    let (x, y) = cell_origin(center, cell);

    let mut clock = CrystalSpellClock::load(0, 6, 120, true);
    assert_eq!(
        clock.next, 100,
        "G2：start=0 -> next=100（0 为合法合成输入）"
    );
    clock.process(2_000);
    assert_eq!(
        (clock.frame, clock.next),
        (1, 2_120),
        "G2：稀疏推进（2000）只走一帧，next=2120"
    );
    assert_eq!(1_630 + clock.frame, 1_631, "G2：参考帧 1 -> Crystal 1631");
    clock.process(2_120);
    assert_eq!(
        (clock.frame, clock.next),
        (2, 2_240),
        "G2：2120 再走一帧，next=2240"
    );

    let mut snapshot = WorldSnapshot::new_session();
    push_object_spell_via_wire(&mut snapshot, 9_002, Spell::FireWall, cell, 0);
    let mut frame = frame_with(BACKGROUND);
    draw_at(
        &mut frame,
        &mut scene,
        &snapshot.spell_events,
        center,
        2_000,
    );
    let expected = fixture.oracle(&[OracleOp::add(SpriteSource::Magic, 1_634, x, y)]);
    assert_frame_matches(&frame, &expected, "G2 生产 age=2000 magic.wzl[1634]");
    assert_frozen_frame(&frame, FROZEN_G2_SHA, FROZEN_G2_COVERED, "G2");

    let reference = fixture.oracle(&[OracleOp::add(SpriteSource::Magic, 1_631, x, y)]);
    assert_ne!(
        frame.pixels, reference,
        "G2：参考 1631 与产品 1634 必须不同（差异合同）"
    );
    cleanup(&fixture.dir);
}

/// G3：Repeat 回卷不是生命周期截止（纯参考模型合同，非生产测试）。
///
/// `FireWall` `ticks 100/220/340/460/580/700` → 帧 `[1,2,3,4,5,0]`，回卷后
/// `ended=true`、`removed=false`、`visible=true`；`PoisonCloud` `ticks 100+120*k
/// (k=0..19)` 同样回卷；继续一 tick 帧 1 且 `ended` 保持 true。
#[test]
fn r75_g3_repeat_wrap_is_not_removal() {
    let mut clock = CrystalSpellClock::load(0, 6, 120, true);
    assert_eq!(clock.next, 100);
    let mut seen = Vec::new();
    for tick in [100i64, 220, 340, 460, 580, 700] {
        clock.process(tick);
        seen.push(clock.frame);
    }
    assert_eq!(
        seen.as_slice(),
        &[1u32, 2, 3, 4, 5, 0],
        "G3：FireWall 六 tick 的帧序列（第 6 次回卷到 0）"
    );
    assert_eq!(
        (clock.ended, clock.removed),
        (true, false),
        "G3：Repeat 回卷置 ended，不是删除"
    );
    assert!(clock.visible(), "G3：回卷后 repeat 对象仍 visible");
    assert_eq!(clock.next, 820, "G3：回卷后 next 仍为 last tick + interval");
    clock.process(820);
    assert_eq!(clock.frame, 1, "G3：继续一 tick 帧 1");
    assert!(clock.ended, "G3：ended 保持 true");
    assert!(!clock.removed && clock.visible(), "G3：不是生命周期截止");

    // PoisonCloud：20 帧同样回卷。
    let mut poison = CrystalSpellClock::load(0, 20, 120, true);
    let mut frames = Vec::new();
    for k in 0..20u32 {
        poison.process(100 + i64::from(k) * 120);
        frames.push(poison.frame);
    }
    assert_eq!(frames.len(), 20, "G3：PoisonCloud 共 20 次 process");
    let first_nineteen: Vec<u32> = (1..=19).collect();
    assert_eq!(
        frames[..19],
        first_nineteen[..],
        "G3：PoisonCloud 前 19 次逐 tick 推进到 19"
    );
    assert_eq!(poison.frame, 0, "G3：PoisonCloud 第 20 次回卷到 0");
    assert!(poison.ended && !poison.removed && poison.visible());
}

/// G4：产品窗是 4000ms，不是 2000ms（2000 只是内部采样点）。
///
/// 实际 149 `FireWall` 与 `PoisonCloud` 事件：`age -1` 空、`0/1999/2000/3999` 有精灵、
/// `4000/4001` 空；`age=3999` 的 index 分别为 `1633`/`1663`；所有非空采样必须有可见
/// 像素、冻结金样互异（异色/offset）；整帧对预冻结金样。
#[test]
fn r75_g4_product_window_is_4000_not_2000() {
    let fixture = Fixture::create("g4");
    let mut scene = fixture.scene();
    let center = center_point();
    let cell = Point::new(10, 10);
    let (x, y) = cell_origin(center, cell);

    let mut firewall = WorldSnapshot::new_session();
    push_object_spell_via_wire(&mut firewall, 9_101, Spell::FireWall, cell, 0);
    let mut poison = WorldSnapshot::new_session();
    push_object_spell_via_wire(&mut poison, 9_102, Spell::PoisonCloud, cell, 0);
    assert_eq!(firewall.spell_events.len(), 1);
    assert_eq!(poison.spell_events.len(), 1);

    let firewall_samples: [(i64, Option<u32>, &str, usize); 7] = [
        (-1, None, FROZEN_G4_EMPTY_SHA, FROZEN_G4_EMPTY_COVERED),
        (0, Some(1_630), FROZEN_G4_FW0_SHA, FROZEN_G4_FW0_COVERED),
        (
            1_999,
            Some(1_634),
            FROZEN_G4_FW1999_SHA,
            FROZEN_G4_FW1999_COVERED,
        ),
        (
            2_000,
            Some(1_634),
            FROZEN_G4_FW2000_SHA,
            FROZEN_G4_FW2000_COVERED,
        ),
        (
            3_999,
            Some(1_633),
            FROZEN_G4_FW3999_SHA,
            FROZEN_G4_FW3999_COVERED,
        ),
        (4_000, None, FROZEN_G4_EMPTY_SHA, FROZEN_G4_EMPTY_COVERED),
        (4_001, None, FROZEN_G4_EMPTY_SHA, FROZEN_G4_EMPTY_COVERED),
    ];
    for (now_ms, index, sha, covered) in firewall_samples {
        let mut frame = frame_with(BACKGROUND);
        draw_at(
            &mut frame,
            &mut scene,
            &firewall.spell_events,
            center,
            now_ms,
        );
        let label = format!("G4 FireWall age={now_ms}");
        if let Some(index) = index {
            let expected = fixture.oracle(&[OracleOp::add(SpriteSource::Magic, index, x, y)]);
            assert_frame_matches(&frame, &expected, &label);
            assert_frozen_frame(&frame, sha, covered, &label);
        } else {
            assert_only_background(&frame, &label);
            assert_frozen_frame(&frame, sha, covered, &label);
        }
    }

    let poison_samples: [(i64, Option<u32>, &str, usize); 7] = [
        (-1, None, FROZEN_G4_EMPTY_SHA, FROZEN_G4_EMPTY_COVERED),
        (0, Some(1_650), FROZEN_G4_PC0_SHA, FROZEN_G4_PC0_COVERED),
        (
            1_999,
            Some(1_666),
            FROZEN_G4_PC1999_SHA,
            FROZEN_G4_PC1999_COVERED,
        ),
        (
            2_000,
            Some(1_666),
            FROZEN_G4_PC2000_SHA,
            FROZEN_G4_PC2000_COVERED,
        ),
        (
            3_999,
            Some(1_663),
            FROZEN_G4_PC3999_SHA,
            FROZEN_G4_PC3999_COVERED,
        ),
        (4_000, None, FROZEN_G4_EMPTY_SHA, FROZEN_G4_EMPTY_COVERED),
        (4_001, None, FROZEN_G4_EMPTY_SHA, FROZEN_G4_EMPTY_COVERED),
    ];
    for (now_ms, index, sha, covered) in poison_samples {
        let mut frame = frame_with(BACKGROUND);
        draw_at(&mut frame, &mut scene, &poison.spell_events, center, now_ms);
        let label = format!("G4 PoisonCloud age={now_ms}");
        if let Some(index) = index {
            let expected = fixture.oracle(&[OracleOp::add(SpriteSource::Magic2, index, x, y)]);
            assert_frame_matches(&frame, &expected, &label);
            assert_frozen_frame(&frame, sha, covered, &label);
        } else {
            assert_only_background(&frame, &label);
            assert_frozen_frame(&frame, sha, covered, &label);
        }
    }

    // 每帧异色/offset：四条非空金样必须互异（冻结 expected 之间即已互异）。
    assert_ne!(FROZEN_G4_FW0_SHA, FROZEN_G4_FW1999_SHA);
    assert_ne!(FROZEN_G4_FW1999_SHA, FROZEN_G4_FW3999_SHA);
    assert_ne!(FROZEN_G4_PC0_SHA, FROZEN_G4_PC1999_SHA);
    assert_ne!(FROZEN_G4_PC0_SHA, FROZEN_G4_PC3999_SHA);
    assert_ne!(FROZEN_G4_PC1999_SHA, FROZEN_G4_PC3999_SHA);
    assert_ne!(FROZEN_G4_FW0_SHA, FROZEN_G4_PC0_SHA);
    assert_ne!(FROZEN_G4_FW3999_SHA, FROZEN_G4_PC3999_SHA);

    // 2000ms 只是内部采样点：重画一次取运行期覆盖像素数，证明不是产品截止。
    let mut sample_2000 = frame_with(BACKGROUND);
    draw_at(
        &mut sample_2000,
        &mut scene,
        &poison.spell_events,
        center,
        2_000,
    );
    assert_eq!(
        covered_count(&sample_2000),
        FROZEN_G4_PC2000_COVERED,
        "G4：2000ms 只是内部采样点，不是产品截止"
    );
    assert_ne!(
        covered_count(&sample_2000),
        0,
        "G4：2000ms 采样仍有可见像素"
    );

    // 参考 repeat 对象未 remove 时仍 visible，不把 2000 当 Crystal 截止。
    let mut clock = CrystalSpellClock::load(0, 6, 120, true);
    clock.process(4_000);
    assert!(
        !clock.removed && clock.visible(),
        "G4：Crystal repeat 对象未 remove 时仍 visible（无 2000ms 截止）"
    );
    cleanup(&fixture.dir);
}

/// G5：26 `ObjectRemove` 只删持久登记，瞬态事件仍绘制（残影现状，非寿命实现）。
///
/// 真实 `encode → decode → apply`：A/B 两条 149 `@1000`；26(A)`@1100` 后 `fields`
/// 仅 B、`events` 仍 A/B 且 ID/`started_ms` 不变；draw`@1120` 仍两处精灵。
/// Crystal 参考 `remove(A)` 仅 A 不可见。
#[test]
fn r75_g5_remove_field_leaves_transient_event() {
    let fixture = Fixture::create("g5");
    let mut scene = fixture.scene();
    let center = center_point();
    let cell_a = Point::new(8, 10);
    let cell_b = Point::new(9, 10);

    let mut snapshot = WorldSnapshot::new_session();
    push_object_spell_via_wire(&mut snapshot, 7_001, Spell::FireWall, cell_a, 1_000);
    push_object_spell_via_wire(&mut snapshot, 7_002, Spell::FireWall, cell_b, 1_000);
    assert_eq!(snapshot.spell_events.len(), 2);
    assert_eq!(snapshot.spell_fields.len(), 2);
    let events_before = snapshot.spell_events.clone();

    push_object_remove_via_wire(&mut snapshot, 7_001, 1_100);
    assert_eq!(snapshot.spell_fields.len(), 1, "G5：26(A) 后持久场只剩 B");
    assert_eq!(snapshot.spell_fields[0].object_id, 7_002);
    assert_eq!(snapshot.spell_fields[0].started_ms, 1_000);
    assert_eq!(
        snapshot.spell_events, events_before,
        "G5：26 不删瞬态事件（现状差异合同，不是寿命实现）"
    );
    assert_eq!(snapshot.spell_events[0].id, 1);
    assert_eq!(snapshot.spell_events[1].id, 2);

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, 1_120);
    let mut frame = frame_with(BACKGROUND);
    draw_at(
        &mut frame,
        &mut scene,
        &snapshot.spell_events,
        center,
        1_120,
    );
    let (ax, ay) = cell_origin(center, cell_a);
    let (bx, by) = cell_origin(center, cell_b);
    let expected = fixture.oracle(&[
        OracleOp::add(SpriteSource::Magic, 1_631, ax, ay),
        OracleOp::add(SpriteSource::Magic, 1_631, bx, by),
    ]);
    assert_frame_matches(&frame, &expected, "G5 age=120 两处 magic.wzl[1631]");
    assert_frozen_frame(&frame, FROZEN_G5_SHA, FROZEN_G5_COVERED, "G5");
    // 1631 offset (4,-2)：A 锚点 (288,256) -> 顶左 (292,254)，非黑像素可见。
    assert_eq!(
        pixel_at(&frame, ax + 4, ay - 2),
        expand565(0x07E0),
        "G5：A 的瞬态残影像素可见"
    );
    assert_eq!(
        pixel_at(&frame, bx + 4, by - 2),
        expand565(0x07E0),
        "G5：B 的像素可见"
    );

    // Crystal 参考：remove(A) 只让 A 不可见，B 仍可见。
    let mut clock_a = CrystalSpellClock::load(1_000, 6, 120, true);
    let mut clock_b = CrystalSpellClock::load(1_000, 6, 120, true);
    clock_a.process(1_100);
    clock_b.process(1_100);
    clock_a.remove();
    assert!(!clock_a.visible(), "G5：参考 A remove 后不可见");
    assert!(clock_b.visible(), "G5：参考 B 未 remove 仍可见");
    assert_eq!(clock_a.frame, clock_b.frame, "G5：remove 不改帧");
    cleanup(&fixture.dir);
}

/// G6：`draw_sprites` 是纯查询；同 ID 再进入只追加事件、不回收旧事件。
///
/// G5 链后同 ID A 新 149`@1200` 换格：`fields` A/B 共 2，`events` 共 3
/// （旧 A 1000 / 新 A 1200）；draw`@1240` 两次整帧相等、事件/场表逐字段不变；
/// 三个事件分别手算金样，避免同 ID 去重造成的伪等价。
#[test]
fn r75_g6_draw_is_query_and_reentry_appends() {
    let fixture = Fixture::create("g6");
    let mut scene = fixture.scene();
    let center = center_point();
    let cell_a = Point::new(8, 10);
    let cell_b = Point::new(9, 10);
    let cell_c = Point::new(11, 10);

    let mut snapshot = WorldSnapshot::new_session();
    push_object_spell_via_wire(&mut snapshot, 7_001, Spell::FireWall, cell_a, 1_000);
    push_object_spell_via_wire(&mut snapshot, 7_002, Spell::FireWall, cell_b, 1_000);
    push_object_remove_via_wire(&mut snapshot, 7_001, 1_100);
    push_object_spell_via_wire(&mut snapshot, 7_001, Spell::FireWall, cell_c, 1_200);

    assert_eq!(
        snapshot.spell_fields.len(),
        2,
        "G6：149 再进入后 fields A/B 共 2"
    );
    assert_eq!(
        snapshot.spell_events.len(),
        3,
        "G6：同 ID 再进入只追加事件（旧 A + B + 新 A）"
    );
    let events_before = snapshot.spell_events.clone();
    assert_eq!(events_before[0].object_id, 7_001);
    assert_eq!(events_before[0].started_ms, 1_000);
    assert_eq!(events_before[0].location, cell_a);
    assert_eq!(events_before[1].object_id, 7_002);
    assert_eq!(events_before[1].started_ms, 1_000);
    assert_eq!(events_before[1].location, cell_b);
    assert_eq!(events_before[2].object_id, 7_001);
    assert_eq!(events_before[2].started_ms, 1_200, "G6：新 A 起点 1200");
    assert_eq!(events_before[2].location, cell_c);

    // 26(A) 已把 A 从场表移除；149 再进入是**追加**，故顺序为 [B, 新 A]。
    assert_eq!(snapshot.spell_fields[0].object_id, 7_002);
    assert_eq!(snapshot.spell_fields[0].location, cell_b);
    assert_eq!(snapshot.spell_fields[0].started_ms, 1_000);
    assert_eq!(snapshot.spell_fields[1].object_id, 7_001);
    assert_eq!(
        snapshot.spell_fields[1].location, cell_c,
        "G6：同 ID 场位置换格"
    );
    assert_eq!(snapshot.spell_fields[1].started_ms, 1_200);

    let (ax, ay) = cell_origin(center, cell_a);
    let (bx, by) = cell_origin(center, cell_b);
    let (cx, cy) = cell_origin(center, cell_c);
    let expected = fixture.oracle(&[
        OracleOp::add(SpriteSource::Magic, 1_632, ax, ay),
        OracleOp::add(SpriteSource::Magic, 1_632, bx, by),
        OracleOp::add(SpriteSource::Magic, 1_630, cx, cy),
    ]);

    let mut first = frame_with(BACKGROUND);
    draw_at(
        &mut first,
        &mut scene,
        &snapshot.spell_events,
        center,
        1_240,
    );
    let mut second = frame_with(BACKGROUND);
    draw_at(
        &mut second,
        &mut scene,
        &snapshot.spell_events,
        center,
        1_240,
    );
    assert_frame_matches(
        &first,
        &expected,
        "G6 旧A+B age=240 magic.wzl[1632]、新A age=40 magic.wzl[1630]",
    );
    assert_eq!(
        first.pixels, second.pixels,
        "G6：draw 两次整帧必须相等（绘制是纯查询）"
    );
    assert_frozen_frame(&first, FROZEN_G6_SHA, FROZEN_G6_COVERED, "G6");
    assert_eq!(
        snapshot.spell_events, events_before,
        "G6：draw 不改瞬态事件（不推进、不回收）"
    );
    assert_eq!(snapshot.spell_fields.len(), 2);
    assert_eq!(snapshot.spell_fields[1].location, cell_c);
    assert_eq!(snapshot.spell_fields[0].started_ms, 1_000);
    assert_eq!(
        covered_count(&first),
        FROZEN_G6_COVERED,
        "G6：三个事件分别落屏（4+4+9），无同 ID 去重伪等价"
    );
    cleanup(&fixture.dir);
}

/// G7：非 Repeat 只隐藏、不删除（纯参考模型合同，非生产测试）。
///
/// 测试私有 Blizzard `count=30`/`interval=100`/`repeat=false`：`ticks 100..3000` 后
/// 帧 30、`visible=false`、`removed=false`；`3100` 后帧 31 仍不删；调用 `remove()`
/// 才 `removed=true`。另以 Effect 参考 `next=80` 在 80 不进、81 才进，与
/// `SpellObject` 的 `>=` 等号进区分比较符。
#[test]
fn r75_g7_nonrepeat_hides_without_remove() {
    // 测试私有 Blizzard 参数（SpellObject.cs:71–79），不称已迁移生产 Blizzard。
    let mut blizzard = CrystalSpellClock::load(0, 30, 100, false);
    assert_eq!(blizzard.next, 100);
    let mut frames = Vec::new();
    for tick in (100..=3_000).step_by(100) {
        blizzard.process(tick);
        frames.push(blizzard.frame);
    }
    assert_eq!(frames.len(), 30, "G7：ticks 100..=3000 共 30 次");
    assert_eq!(frames.last().copied(), Some(30), "G7：30 tick 后 frame=30");
    assert_eq!(
        (blizzard.ended, blizzard.removed),
        (false, false),
        "G7：非 Repeat 不置 ended，也不删除"
    );
    assert!(!blizzard.visible(), "G7：frame>=count 且 !repeat -> 只隐藏");
    blizzard.process(3_100);
    assert_eq!(blizzard.frame, 31, "G7：3100 继续推进一帧");
    assert!(!blizzard.removed, "G7：隐藏不等于 remove");
    assert!(!blizzard.visible(), "G7：仍隐藏");
    blizzard.remove();
    assert!(
        blizzard.removed && !blizzard.visible(),
        "G7：显式 26 Remove 才 removed"
    );
    assert!(!blizzard.ended, "G7：remove 不改 ended");

    // Effect 参考：起点排期 next=80，严格 `>` 才进；与 SpellObject 的等号进不同。
    let mut effect = EffectClock::load(0, 400, 5);
    assert_eq!(effect.next, 80);
    effect.process(80);
    assert_eq!(effect.frame, 0, "G7：Effect 在 next=80 等号不进");
    effect.process(81);
    assert_eq!(effect.frame, 1, "G7：Effect 81 才进");
    assert_eq!(effect.next, 160, "G7：Effect 下一排期 160（起点排期）");

    // 对照：SpellObject 在 next=100 等号进。
    let mut spell_object = CrystalSpellClock::load(0, 6, 120, true);
    assert_eq!(spell_object.next, 100);
    spell_object.process(100);
    assert_eq!(
        spell_object.frame, 1,
        "G7：SpellObject 等号进（与 Effect 严格 > 不同，禁复用比较符）"
    );
}
