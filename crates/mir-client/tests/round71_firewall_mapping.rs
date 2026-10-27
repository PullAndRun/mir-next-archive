//! Round 71 A：149 `S.ObjectSpell` 来源的 `Spell::FireWall` 六帧 elapsed 循环映射。
//!
//! 本文件只验收 [`docs/process/round71-plan.md`] 的有限切片（**不是**完整 Crystal
//! `FireWall` 迁移）：
//!
//! * 仅 149 `ObjectSpell` 来源（`SpellEvent::from_object_spell == true`）的 `FireWall`
//!   走 Crystal `SpellObject.cs:49` 的 `1630..=1635`、120ms/帧、6 帧 elapsed 循环：
//!   `phase = (age / 120) % 6`、`index = 1630 + phase`；
//! * 落点是格锚点 `world_to_screen(center, location)`，帧头 i16 `offset_x/offset_y`
//!   由 `MapScene::draw_effect_blend(..., apply_offset = true)` **只叠一次**；
//! * 混合方式 = `Frame::blit_add`（Crystal `DrawBlend` 的 CPU 子集）。Crystal 的
//!   `rate = 1` 与 `src * 0.8 * alpha` 调光**未实施**（PARTIAL），本文件不推定两者等价；
//! * 时钟写实：Crystal `SpellObject.cs:302` 的 `NextMotion = CMain.Time + FrameInterval`；
//!   本切片只验证 elapsed 帧循环，严格 `NextMotion` 累积、`Light`、`DrawColour` 与对象
//!   寿命均未迁移；
//! * 非 149 来源的 `FireWall` 与其它技能保持旧通用路径（48×32 居中 + Over 整帧）。
//!
//! 独立 oracle：本文件内合成 `magic.wzl`/`magic.wzx`、`magic2.wzl`/`magic2.wzx`，
//! 从原始 RGB565 像素按 `Frame::blit`/`Frame::blit_add` 的文档公式手算整帧 RGBA，
//! **不调用**生产 `blit/blit_add/add_channel/blend_channel`。
//!
//! 运行（缺 `legacy` feature 时合成图库不可读，首条夹具断言以诊断文本失败而非 SKIP）：
//!
//! ```text
//! cargo test -p mir-client --features mir-render/legacy --test round71_firewall_mapping -- --test-threads=1
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use mir_client::net::{SpellEvent, WorldSnapshot};
use mir_client::ui::UiState;
use mir_client::ui::spellfx;
use mir_core::enums::{MirDirection, Spell};
use mir_core::{Language, Point};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{ObjectSpell, decode_server_packet};
use mir_render::{EntityLibraries, Frame, FxKind, MapScene, TileLibraries, Viewport};
use mir_world::{Map, MapCell, MapVersion};

// ---------------------------------------------------------------------------
// 独立几何常量：800×600 视口（Crystal `MapControl` 的 CellWidth/CellHeight 语义）
// ---------------------------------------------------------------------------

/// 测试帧宽：与 `Viewport::DEFAULT` 一致，但由本文件独立写死（oracle 不读生产常量）。
const FRAME_WIDTH: u32 = 800;
/// 测试帧高。
const FRAME_HEIGHT: u32 = 600;
/// 格原点：`OffSetX = 800 / 2 / 48 = 8`、`OffSetY = 600 / 2 / 32 - 1 = 8`，
/// 故中心格原点 `(8 * 48, 8 * 32) = (384, 256)`。
const VIEWPORT_ORIGIN: (i32, i32) = (384, 256);
/// 格宽（Crystal `MapControl.CellWidth`）。
const CELL_WIDTH: i32 = 48;
/// 格高（Crystal `MapControl.CellHeight`）。
const CELL_HEIGHT: i32 = 32;

/// 合成帧边长：2×2（通用帧/诱饵帧）。
const SIZE_2X2: (u16, u16) = (2, 2);
/// 合成帧边长：3×3（`1630`，用于证明不按尺寸居中）。
const SIZE_3X3: (u16, u16) = (3, 3);

// ---------------------------------------------------------------------------
// 合成帧表
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

/// 合成 `magic.wzl` 帧表（`FxKind::Magic` → 库 `magic`，起始帧 0）。
///
/// | 帧 | 边长 | offset | 用途 |
/// |---|---|---|---|
/// | 0..=9 | 2×2 | (0,0) | 旧通用闪光诱饵（四角异色，新分支不得画） |
/// | 1620 | 2×2 | (0,0) | Crystal FireWall 起手帧诱饵（本切片不迁移起手） |
/// | 1630 | 3×3 | (-3,5) | phase 0；3×3 用于证明不按尺寸居中 |
/// | 1631 | 2×2 | (4,-2) | phase 1；尺寸与 1630 不同 |
/// | 1632 | 2×2 | (-1,-1) | 负坐标部分裁剪 |
/// | 1633 | 2×2 | (50,-30) | 屏外格锚点被当帧 offset 移入 |
/// | 1634 | 2×2 | (7,6) | 边界 age=599 映射 |
/// | 1635 | 2×2 | (-5,4) | 边界 age=600/719 映射 |
static MAIN_MAGIC: [MagicFrame; 17] = [
    MagicFrame::new(0, SIZE_2X2, (0, 0), [0x8410, 0x2104, 0x7BEF, 0xFFFF]),
    MagicFrame::new(1, SIZE_2X2, (0, 0), [0x2104, 0x8410, 0xFFFF, 0x7BEF]),
    MagicFrame::new(2, SIZE_2X2, (0, 0), [0x7BEF, 0xFFFF, 0x8410, 0x2104]),
    MagicFrame::new(3, SIZE_2X2, (0, 0), [0xFFFF, 0x7BEF, 0x2104, 0x8410]),
    MagicFrame::new(4, SIZE_2X2, (0, 0), [0x2104, 0x7BEF, 0xFFFF, 0x8410]),
    MagicFrame::new(5, SIZE_2X2, (0, 0), [0xFC00, 0x07C0, 0xF804, 0x52AA]),
    MagicFrame::new(6, SIZE_2X2, (0, 0), [0x07C0, 0xFC00, 0x52AA, 0xF804]),
    MagicFrame::new(7, SIZE_2X2, (0, 0), [0xF804, 0x52AA, 0xFC00, 0x07C0]),
    MagicFrame::new(8, SIZE_2X2, (0, 0), [0x52AA, 0xF804, 0x07C0, 0xFC00]),
    MagicFrame::new(9, SIZE_2X2, (0, 0), [0xFC00, 0x52AA, 0xF804, 0x07C0]),
    MagicFrame::new(1_620, SIZE_2X2, (0, 0), [0xFC10; 4]),
    MagicFrame::new(1_630, SIZE_3X3, (-3, 5), [0xF800; 4]),
    MagicFrame::new(1_631, SIZE_2X2, (4, -2), [0x07E0; 4]),
    MagicFrame::new(1_632, SIZE_2X2, (-1, -1), [0x001F; 4]),
    MagicFrame::new(1_633, SIZE_2X2, (50, -30), [0xFFE0; 4]),
    MagicFrame::new(1_634, SIZE_2X2, (7, 6), [0xF81F; 4]),
    MagicFrame::new(1_635, SIZE_2X2, (-5, 4), [0x07FF; 4]),
];

/// 合成 `magic2.wzl` 帧表（`FxKind::Bolt` → 库 `magic2`，起始帧 20）。
///
/// | 帧 | 用途 |
/// |---|---|
/// | 20..=29 | 旧 Bolt 路径（`ThunderBolt`/`Lightning`）金样帧 |
/// | 1650 | 同索引诱饵：`1630 + start_frame(20)`，若新分支错用 `FxKind::Bolt` 会画到它 |
static MAIN_MAGIC2: [MagicFrame; 11] = [
    MagicFrame::new(20, SIZE_2X2, (0, 0), [0x8410, 0xFFFF, 0x7BEF, 0x2104]),
    MagicFrame::new(21, SIZE_2X2, (0, 0), [0x7BEF, 0x8410, 0x2104, 0xFFFF]),
    MagicFrame::new(22, SIZE_2X2, (0, 0), [0xFFFF, 0x2104, 0x8410, 0x7BEF]),
    MagicFrame::new(23, SIZE_2X2, (0, 0), [0x2104, 0x7BEF, 0xFFFF, 0x8410]),
    MagicFrame::new(24, SIZE_2X2, (0, 0), [0xFC00, 0x07C0, 0x52AA, 0xFC10]),
    MagicFrame::new(25, SIZE_2X2, (0, 0), [0x07C0, 0xFC00, 0xFC10, 0x52AA]),
    MagicFrame::new(26, SIZE_2X2, (0, 0), [0x52AA, 0xFC10, 0xFC00, 0x07C0]),
    MagicFrame::new(27, SIZE_2X2, (0, 0), [0xFC10, 0x52AA, 0x07C0, 0xFC00]),
    MagicFrame::new(28, SIZE_2X2, (0, 0), [0xF804, 0xFC10, 0x52AA, 0x07C0]),
    MagicFrame::new(29, SIZE_2X2, (0, 0), [0xFC10, 0xF804, 0x07C0, 0x52AA]),
    MagicFrame::new(1_650, SIZE_2X2, (0, 0), [0x8410; 4]),
];

// ---------------------------------------------------------------------------
// 夹具：进程唯一临时目录 + 合成 WZL/WZX（真实 `EntityLibraries` 读取链）
// ---------------------------------------------------------------------------

/// 进程唯一临时目录（不读 `resource`/`game-assets`，也不与其它测试共享路径）。
///
/// 优先系统临时目录；被环境策略拒绝写时回退到 `target/round71/a/tmp/`（测试产物目录）。
fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let name = format!("mir-next-r71-a-{tag}-{}-{nanos}", std::process::id());
    let candidates = [
        std::env::temp_dir().join(&name),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/round71/a/tmp")
            .join(&name),
    ];
    for dir in candidates {
        if fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }
    panic!("无法创建进程唯一临时目录（系统临时目录与 target/round71/a/tmp 均不可写）");
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
/// 每行按 4 字节对齐（`WidthBytes = ((宽 × 16 + 31) >> 5) * 4`，RGB565 下 3×3 每行补 2 字节）。
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
        file_name: "r71-empty".to_owned(),
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

/// 写 `magic` + `magic2` 两个合成图库并构造场景。
fn scene_with_libraries(
    tag: &str,
    magic: &[MagicFrame],
    magic2: &[MagicFrame],
) -> (MapScene, PathBuf) {
    let dir = unique_dir(tag);
    write_library(&dir, "magic", magic);
    write_library(&dir, "magic2", magic2);
    (scene_in_dir(&dir), dir)
}

/// 默认夹具（完整 `MAIN_MAGIC` + `MAIN_MAGIC2`）。
fn scene_with_default(tag: &str) -> (MapScene, PathBuf) {
    scene_with_libraries(tag, &MAIN_MAGIC, &MAIN_MAGIC2)
}

/// 空实体图库场景（目录里没有任何 `.wzl`，即「无 Magic」）。
fn scene_without_entities(tag: &str) -> (MapScene, PathBuf) {
    let dir = unique_dir(tag);
    (scene_in_dir(&dir), dir)
}

/// 去掉指定帧号的合成 magic 帧表（`.wzx` 表项留 0 = 缺帧）。
fn magic_without(missing: u32) -> Vec<MagicFrame> {
    MAIN_MAGIC
        .iter()
        .copied()
        .filter(|spec| spec.index != missing)
        .collect()
}

/// 默认夹具可读性前置断言（缺 feature 时以诊断文本失败，不是 SKIP）。
fn assert_default_fixture_readable(scene: &mut MapScene) {
    assert_eq!(
        scene.effect_size(FxKind::Magic, 0),
        Some((2, 2)),
        "合成 magic.wzl[0] 必须可读（请加 --features mir-render/legacy）"
    );
    assert_eq!(
        scene.effect_size(FxKind::Magic, 1_620),
        Some((2, 2)),
        "合成 magic.wzl[1620] 诱饵必须可读"
    );
    assert_eq!(
        scene.effect_size(FxKind::Magic, 1_630),
        Some((3, 3)),
        "合成 magic.wzl[1630] 必须是 3×3"
    );
    for index in 1_631u32..=1_635 {
        assert_eq!(
            scene.effect_size(FxKind::Magic, index),
            Some((2, 2)),
            "合成 magic.wzl[{index}] 必须是 2×2"
        );
    }
    assert_eq!(
        scene.effect_size(FxKind::Bolt, 0),
        Some((2, 2)),
        "合成 magic2.wzl[20]（Bolt 起始帧 20）必须可读"
    );
    assert_eq!(
        scene.effect_size(FxKind::Bolt, 1_630),
        Some((2, 2)),
        "合成 magic2.wzl[1650] 同索引诱饵必须可读"
    );
}

// ---------------------------------------------------------------------------
// 独立 oracle：从原始夹具像素手算整帧 RGBA（不调用生产混合函数）
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

/// Add 单通道（`Frame::blit_add` 文档公式：`min(255, dst + round(src * a / 255))`）。
fn oracle_add_channel(destination: u8, source: u8, alpha: u8) -> u8 {
    let contribution = (u32::from(source) * u32::from(alpha) + 127) / 255;
    u8::try_from((u32::from(destination) + contribution).min(255)).unwrap_or(255)
}

/// Over 单通道（`round((src * a + dst * (255 - a)) / 255)`）。
fn oracle_over_channel(destination: u8, source: u8, alpha: u8) -> u8 {
    let alpha = u32::from(alpha);
    let source = u32::from(source);
    let destination = u32::from(destination);
    u8::try_from((source * alpha + destination * (255 - alpha) + 127) / 255).unwrap_or(255)
}

/// oracle 帧来自哪个合成图库（决定 `start_frame` 语义与帧号空间）。
#[derive(Clone, Copy, Debug)]
enum SpriteSource {
    /// `magic.wzl`（`FxKind::Magic`，起始帧 0）。
    Magic,
    /// `magic2.wzl`（`FxKind::Bolt`/`Swing`，起始帧 20）。
    Magic2,
}

/// oracle 的混合方式。
#[derive(Clone, Copy, Debug)]
enum BlendKind {
    Over,
    Add,
}

/// oracle 的一次绘制：图库 + 帧号 + 锚点 + 是否消费当帧 offset + 混合方式。
#[derive(Clone, Copy, Debug)]
struct OracleOp {
    source: SpriteSource,
    frame: u32,
    x: i32,
    y: i32,
    apply_offset: bool,
    blend: BlendKind,
}

impl OracleOp {
    const fn add(source: SpriteSource, frame: u32, x: i32, y: i32, apply_offset: bool) -> Self {
        Self {
            source,
            frame,
            x,
            y,
            apply_offset,
            blend: BlendKind::Add,
        }
    }

    const fn over(source: SpriteSource, frame: u32, x: i32, y: i32, apply_offset: bool) -> Self {
        Self {
            source,
            frame,
            x,
            y,
            apply_offset,
            blend: BlendKind::Over,
        }
    }
}

/// 帧表查询（缺帧号直接 panic，防止夹具与断言漂移）。
fn frame_spec(source: SpriteSource, index: u32) -> &'static MagicFrame {
    let table: &'static [MagicFrame] = match source {
        SpriteSource::Magic => &MAIN_MAGIC,
        SpriteSource::Magic2 => &MAIN_MAGIC2,
    };
    table
        .iter()
        .find(|spec| spec.index == index)
        .expect("合成帧表必须覆盖该帧号")
}

/// 一帧的逐像素来源：`(列, 行, RGB565)`。
fn frame_pixels(spec: &MagicFrame) -> Vec<(i32, i32, u16)> {
    if spec.size == SIZE_3X3 {
        (0..9)
            .map(|slot| (slot % 3, slot / 3, spec.quad[0]))
            .collect()
    } else {
        spec.quad
            .iter()
            .enumerate()
            .map(|(slot, &value)| ((slot % 2) as i32, (slot / 2) as i32, value))
            .collect()
    }
}

/// 独立整帧 oracle：从 `background` 起按 `ops` 顺序逐像素手算。
fn oracle_frame(width: u32, height: u32, background: [u8; 4], ops: &[OracleOp]) -> Vec<u8> {
    let width_usize = usize::try_from(width).expect("oracle 宽");
    let height_usize = usize::try_from(height).expect("oracle 高");
    let mut pixels = vec![0u8; width_usize * height_usize * 4];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&background);
    }
    for op in ops {
        let spec = frame_spec(op.source, op.frame);
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
        for (column, row, raw) in frame_pixels(spec) {
            let source = expand565(raw);
            let alpha = fixture_alpha(source);
            if alpha == 0 {
                // alpha=0：整点跳过，不动 RGB，也不动目标 alpha 字节。
                continue;
            }
            let x = i64::from(left) + i64::from(column);
            let y = i64::from(top) + i64::from(row);
            if x < 0 || y < 0 || x >= i64::from(width) || y >= i64::from(height) {
                continue;
            }
            let offset =
                (usize::try_from(y).expect("y") * width_usize + usize::try_from(x).expect("x")) * 4;
            let target = &mut pixels[offset..offset + 4];
            for (channel, value) in target[..3].iter_mut().enumerate() {
                *value = match op.blend {
                    BlendKind::Over => oracle_over_channel(*value, source[channel], alpha),
                    BlendKind::Add => oracle_add_channel(*value, source[channel], alpha),
                };
            }
            target[3] = 255;
        }
    }
    pixels
}

/// 诱饵颜色：所有非 `FireWall` 六帧（`1630..=1635`）的合成像素展开值。
///
/// 新分支若错画旧通用帧（0..=9）、`1620` 起手帧或 `magic2` 同索引帧，会命中这些颜色。
fn decoy_colors() -> Vec<[u8; 4]> {
    let mut colors = Vec::new();
    for spec in MAIN_MAGIC
        .iter()
        .filter(|spec| !(1_630..=1_635).contains(&spec.index))
    {
        for &raw in &spec.quad {
            colors.push(expand565(raw));
        }
    }
    for spec in &MAIN_MAGIC2 {
        for &raw in &spec.quad {
            colors.push(expand565(raw));
        }
    }
    colors
}

// ---------------------------------------------------------------------------
// 帧 / 位置 / 断言辅助
// ---------------------------------------------------------------------------

/// 新建并用指定背景清屏（背景 alpha 必须 255，与生产帧缓冲「不透明画布」一致）。
fn frame_with(width: u32, height: u32, background: [u8; 4]) -> Frame {
    assert_eq!(background[3], 255, "测试背景必须是不透明画布");
    let mut frame = Frame::new(width, height);
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

/// 覆盖位置集合：与背景不同的像素 `(x, y)`。
fn covered_positions(pixels: &[u8], width: u32, background: [u8; 4]) -> Vec<(u32, u32)> {
    let width_usize = usize::try_from(width).expect("width");
    let mut positions = Vec::new();
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        if pixel != background {
            positions.push((
                u32::try_from(index % width_usize).expect("x"),
                u32::try_from(index / width_usize).expect("y"),
            ));
        }
    }
    positions
}

/// 非背景像素数量。
fn covered_count(frame: &Frame, background: [u8; 4]) -> usize {
    covered_positions(&frame.pixels, frame.width, background).len()
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
fn assert_only_background(frame: &Frame, background: [u8; 4], label: &str) {
    for (index, pixel) in frame.pixels.chunks_exact(4).enumerate() {
        if pixel != background {
            let x = u32::try_from(index % usize::try_from(frame.width).expect("width")).expect("x");
            let y = u32::try_from(index / usize::try_from(frame.width).expect("width")).expect("y");
            let mut value = [0u8; 4];
            value.copy_from_slice(pixel);
            panic!("{label}：({x},{y}) 出现意外像素 actual={value:?} expected=背景{background:?}");
        }
    }
}

/// 断言整帧没有任何诱饵帧颜色（0..=9 / 1620 / magic2 同索引帧）。
#[track_caller]
fn assert_no_decoy_colors(frame: &Frame, background: [u8; 4], label: &str) {
    let decoys = decoy_colors();
    for (index, pixel) in frame.pixels.chunks_exact(4).enumerate() {
        if pixel == background {
            continue;
        }
        let mut value = [0u8; 4];
        value.copy_from_slice(pixel);
        if decoys.contains(&value) {
            let x = u32::try_from(index % usize::try_from(frame.width).expect("width")).expect("x");
            let y = u32::try_from(index / usize::try_from(frame.width).expect("width")).expect("y");
            panic!(
                "{label}：({x},{y}) 出现诱饵帧颜色 {value:?}；149 六帧分支不得画旧通用帧/起手帧/magic2 同索引帧"
            );
        }
    }
}

/// 删除进程唯一临时目录（失败不影响断言结论）。
fn cleanup(directory: &Path) {
    let _ = fs::remove_dir_all(directory);
}

/// 格原点（视口中心格原点 + 格偏移），独立于生产 `world_to_screen` 实现。
fn cell_origin(center: Point, cell: Point) -> (i32, i32) {
    (
        VIEWPORT_ORIGIN.0 + (cell.x - center.x) * CELL_WIDTH,
        VIEWPORT_ORIGIN.1 + (cell.y - center.y) * CELL_HEIGHT,
    )
}

/// 149 分支落点：格原点 + 当帧 i16 offset（只叠一次，先于裁剪）。
fn frame_top_left(center: Point, cell: Point, spec: &MagicFrame) -> (i32, i32) {
    let (x, y) = cell_origin(center, cell);
    (
        x.saturating_add(i32::from(spec.offset.0)),
        y.saturating_add(i32::from(spec.offset.1)),
    )
}

/// 旧通用路径落点：`(48 - w) / 2`、`(32 - h) / 2` 居中，不消费帧 offset。
fn legacy_top_left(center: Point, cell: Point, spec: &MagicFrame) -> (i32, i32) {
    let (x, y) = cell_origin(center, cell);
    (
        x + (CELL_WIDTH - i32::from(spec.size.0)) / 2,
        y + (CELL_HEIGHT - i32::from(spec.size.1)) / 2,
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

/// `FireWall` 五格十字：中心 + 上下左右（149 `ObjectSpell` 逐格独立事件）。
fn fire_wall_cross(center: Point) -> [Point; 5] {
    [
        center,
        Point::new(center.x, center.y - 1),
        Point::new(center.x, center.y + 1),
        Point::new(center.x - 1, center.y),
        Point::new(center.x + 1, center.y),
    ]
}

/// 手造事件（旧路径 / 非 wire 场景用；`from_object_spell` 决定走哪条分支）。
fn spell_event(
    id: u64,
    spell: Spell,
    location: Point,
    started_ms: i64,
    from_object_spell: bool,
) -> SpellEvent {
    SpellEvent {
        id,
        object_id: 0,
        spell,
        location,
        direction: MirDirection::Down,
        param: false,
        from_object_spell,
        started_ms,
    }
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
// G1：149 真实 wire 链 → 五格十字 → t=0 画 magic.wzl[1630]
// ---------------------------------------------------------------------------

/// G1：149 五格十字（真实 `encode_frame → decode_server_packet → apply_packet_at`）
/// 在 `t=0` 必须各画一张 `magic.wzl[1630]`（格锚点 + 当帧 offset），不得画旧 0/20/1620 帧。
#[test]
fn r71_g1_wire_mapping() {
    let (mut scene, dir) = scene_with_default("g1");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let background = [0, 0, 0, 255];
    let cross = fire_wall_cross(center);
    let mut snapshot = WorldSnapshot::new_session();
    for (slot, cell) in cross.iter().enumerate() {
        let object_id = 9_001 + u32::try_from(slot).expect("十字槽位");
        push_object_spell_via_wire(&mut snapshot, object_id, Spell::FireWall, *cell, 0);
    }
    assert_eq!(
        snapshot.spell_events.len(),
        5,
        "149 五格十字必须写入 5 条事件"
    );
    for (slot, event) in snapshot.spell_events.iter().enumerate() {
        let object_id = 9_001 + u32::try_from(slot).expect("十字槽位");
        assert!(
            event.from_object_spell,
            "149 来源必须标记 from_object_spell=true"
        );
        assert_eq!(event.spell, Spell::FireWall);
        assert_eq!(event.location, cross[slot], "149 逐格落点必须原样保存");
        assert_eq!(event.object_id, object_id, "149 每格一个独立 object_id");
        assert_eq!(event.started_ms, 0);
    }

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, 0);
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut frame,
        &mut scene,
        &snapshot.spell_events,
        center,
        ui.time_millis,
    );

    let mut ops = Vec::with_capacity(cross.len());
    for cell in cross {
        let (x, y) = cell_origin(center, cell);
        ops.push(OracleOp::add(SpriteSource::Magic, 1_630, x, y, true));
    }
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G1：t=0 五格十字每格必须画 magic.wzl[1630]（3×3，格原点 + 当帧 offset (-3,5) 只叠一次）",
    );

    // 显式逐像素：1630 是 3×3 纯红，左上 = 格原点 + (-3,5)。
    let spec = frame_spec(SpriteSource::Magic, 1_630);
    let red = expand565(0xF800);
    assert_eq!(
        red,
        [248, 0, 0, 255],
        "RGB565 0xF800 展开为纯红（oracle 基线）"
    );
    for cell in fire_wall_cross(center) {
        let (x, y) = frame_top_left(center, cell, spec);
        for (column, row, value) in frame_pixels(spec) {
            assert_eq!(
                pixel_at(&frame, x + column, y + row),
                expand565(value),
                "G1：格 {cell:?} 的 1630 像素 ({},{}) 颜色不符",
                x + column,
                y + row
            );
        }
    }
    assert_eq!(
        covered_count(&frame, background),
        5 * 9,
        "G1：五格各 9 像素"
    );

    // 旧通用闪光（Magic 0 / Bolt 20）按 48×32 居中：新映射不得在那里出图。
    let magic0 = frame_spec(SpriteSource::Magic, 0);
    for cell in fire_wall_cross(center) {
        let (legacy_x, legacy_y) = legacy_top_left(center, cell, magic0);
        for (column, row, _value) in frame_pixels(magic0) {
            assert_eq!(
                pixel_at(&frame, legacy_x + column, legacy_y + row),
                background,
                "G1：旧居中位置不得出现 magic.wzl[0]（未迁移的起手闪光）"
            );
        }
        let (origin_x, origin_y) = cell_origin(center, cell);
        assert_eq!(
            pixel_at(&frame, origin_x, origin_y),
            background,
            "G1：格原点不得出现 1620 起手帧"
        );
    }
    assert_no_decoy_colors(&frame, background, "G1");
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G2：帧边界（120ms/帧、6 帧循环）与事件窗口
// ---------------------------------------------------------------------------

/// G2：`phase = (age / 120) % 6` 的每个边界都必须映射到 `1630 + phase`，每时点独立清帧。
#[test]
fn r71_g2_frame_boundaries() {
    let (mut scene, dir) = scene_with_default("g2-frame");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let cell = center;
    let background = [0, 0, 0, 255];
    let mut snapshot = WorldSnapshot::new_session();
    push_object_spell_via_wire(&mut snapshot, 9_201, Spell::FireWall, cell, 0);
    let mut ui = ui_state();

    let boundaries = [
        (0i64, 1_630u32),
        (119, 1_630),
        (120, 1_631),
        (599, 1_634),
        (600, 1_635),
        (719, 1_635),
        (720, 1_630),
        (840, 1_631),
        (3_999, 1_633),
    ];
    for (age, expected_index) in boundaries {
        sync_at(&mut ui, &snapshot, age);
        let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(
            &mut frame,
            &mut scene,
            &snapshot.spell_events,
            center,
            ui.time_millis,
        );

        let spec = frame_spec(SpriteSource::Magic, expected_index);
        let (origin_x, origin_y) = cell_origin(center, cell);
        let label = format!(
            "G2 boundaries：age={age} 必须映射 phase=(age/120)%6 → magic.wzl[{expected_index}]"
        );
        assert_frame_matches(
            &frame,
            &oracle_frame(
                FRAME_WIDTH,
                FRAME_HEIGHT,
                background,
                &[OracleOp::add(
                    SpriteSource::Magic,
                    expected_index,
                    origin_x,
                    origin_y,
                    true,
                )],
            ),
            &label,
        );

        let (x, y) = frame_top_left(center, cell, spec);
        assert_eq!(
            pixel_at(&frame, x, y),
            expand565(spec.quad[0]),
            "G2 boundaries：age={age} 左上角颜色必须是 magic.wzl[{expected_index}]"
        );
        assert_eq!(
            covered_count(&frame, background),
            frame_pixels(spec).len(),
            "G2 boundaries：age={age} 只能画一帧（独立清帧、无上一帧残留）"
        );
    }
    cleanup(&dir);
}

/// G2：`age < 0` 与 `age >= 4000` 必须无图且不 panic（含 i64 边界）。
#[test]
fn r71_g2_age_window() {
    let (mut scene, dir) = scene_with_default("g2-window");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let background = [12, 34, 56, 255];
    let mut snapshot = WorldSnapshot::new_session();
    push_object_spell_via_wire(&mut snapshot, 9_202, Spell::FireWall, center, 0);
    let mut ui = ui_state();

    for now_ms in [-1i64, 4_000, i64::MIN, i64::MAX] {
        sync_at(&mut ui, &snapshot, now_ms);
        let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(
            &mut frame,
            &mut scene,
            &snapshot.spell_events,
            center,
            ui.time_millis,
        );
        let age = ui.time_millis.saturating_sub(0);
        let label =
            format!("G2 age_window：age={age} 必须被 0 <= age < 4000 过滤（无图、不 panic）");
        assert_only_background(&frame, background, &label);
    }
    assert_eq!(
        spellfx::duration_ms(Spell::FireWall),
        4_000,
        "G2 age_window：149 FireWall 事件窗口仍是 4000ms（Crystal SpellObject Duration）"
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G3：格锚点 + 当帧 signed offset 一次；屏外/部分裁剪
// ---------------------------------------------------------------------------

/// G3：不同帧尺寸必须都落在格原点 + 当帧 offset（不做 `effect_size` 居中），
/// offset 只叠一次、不跨帧残留；换相机中心时锚点随 `world_to_screen` 移动。
#[test]
fn r71_g3_anchor_offsets() {
    let (mut scene, dir) = scene_with_default("g3-anchor");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let cell = center;
    let background = [0, 0, 0, 255];

    // 帧尺寸必须不同：1630 是 3×3、1631 是 2×2（旧实现按尺寸居中，新实现不居中）。
    assert_eq!(scene.effect_size(FxKind::Magic, 1_630), Some((3, 3)));
    assert_eq!(scene.effect_size(FxKind::Magic, 1_631), Some((2, 2)));

    let mut snapshot = WorldSnapshot::new_session();
    push_object_spell_via_wire(&mut snapshot, 9_301, Spell::FireWall, cell, 0);
    let mut ui = ui_state();
    let (origin_x, origin_y) = cell_origin(center, cell);

    // —— age=0 → 1630，offset (-3,5)：左上 = 格原点 + (-3,5) ——
    sync_at(&mut ui, &snapshot, 0);
    let mut frame0 = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut frame0,
        &mut scene,
        &snapshot.spell_events,
        center,
        ui.time_millis,
    );
    let spec0 = frame_spec(SpriteSource::Magic, 1_630);
    assert_frame_matches(
        &frame0,
        &oracle_frame(
            FRAME_WIDTH,
            FRAME_HEIGHT,
            background,
            &[OracleOp::add(
                SpriteSource::Magic,
                1_630,
                origin_x,
                origin_y,
                true,
            )],
        ),
        "G3 anchor：age=0 → 1630，锚点=格原点，offset (-3,5) 只叠一次",
    );
    let (x0, y0) = frame_top_left(center, cell, spec0);
    assert_eq!((x0, y0), (origin_x - 3, origin_y + 5));
    assert_eq!(pixel_at(&frame0, x0, y0), expand565(spec0.quad[0]));
    assert_eq!(
        covered_count(&frame0, background),
        9,
        "G3 anchor：1630 是 3×3"
    );
    let (legacy_centered_x, legacy_centered_y) = legacy_top_left(center, cell, spec0);
    assert_eq!(
        pixel_at(&frame0, legacy_centered_x, legacy_centered_y),
        background,
        "G3 anchor：149 分支不做 effect_size 居中（旧居中点必须空白）"
    );

    // —— age=120 → 1631，offset (4,-2)：新位置出图，且帧 1630 位置无残留 ——
    sync_at(&mut ui, &snapshot, 120);
    let mut frame1 = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut frame1,
        &mut scene,
        &snapshot.spell_events,
        center,
        ui.time_millis,
    );
    let spec1 = frame_spec(SpriteSource::Magic, 1_631);
    assert_frame_matches(
        &frame1,
        &oracle_frame(
            FRAME_WIDTH,
            FRAME_HEIGHT,
            background,
            &[OracleOp::add(
                SpriteSource::Magic,
                1_631,
                origin_x,
                origin_y,
                true,
            )],
        ),
        "G3 anchor：age=120 → 1631，offset (4,-2) 只叠一次",
    );
    let (x1, y1) = frame_top_left(center, cell, spec1);
    assert_eq!((x1, y1), (origin_x + 4, origin_y - 2));
    assert_eq!(pixel_at(&frame1, x1, y1), expand565(spec1.quad[0]));
    assert_eq!(
        covered_count(&frame1, background),
        4,
        "G3 anchor：1631 是 2×2"
    );
    assert_eq!(
        pixel_at(&frame1, x0, y0),
        background,
        "G3 anchor：1631 不得残留 1630 的位置（offset 不跨帧累积）"
    );

    // —— 同帧重复绘制：Add 叠两次（248 + 248 饱和到 255）——
    let repeated_events = [
        spell_event(9_311, Spell::FireWall, cell, 0, true),
        spell_event(9_312, Spell::FireWall, cell, 0, true),
    ];
    let mut repeated_frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut repeated_frame, &mut scene, &repeated_events, center, 0);
    assert_frame_matches(
        &repeated_frame,
        &oracle_frame(
            FRAME_WIDTH,
            FRAME_HEIGHT,
            background,
            &[
                OracleOp::add(SpriteSource::Magic, 1_630, origin_x, origin_y, true),
                OracleOp::add(SpriteSource::Magic, 1_630, origin_x, origin_y, true),
            ],
        ),
        "G3 anchor：同一帧同锚点重复绘制必须逐次 Add（offset 每次只叠一次，位置不漂移）",
    );
    assert_eq!(
        pixel_at(&repeated_frame, x0, y0),
        [255, 0, 0, 255],
        "G3 anchor：Add 两次纯红 248+248 饱和到 255"
    );

    // —— 跨帧：同一帧上 age=0（1630）与 age=120（1631）——
    let mixed_events = [
        spell_event(9_321, Spell::FireWall, cell, 0, true),
        spell_event(9_322, Spell::FireWall, cell, 120, true),
    ];
    let mut mixed_frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut mixed_frame, &mut scene, &mixed_events, center, 120);
    assert_frame_matches(
        &mixed_frame,
        &oracle_frame(
            FRAME_WIDTH,
            FRAME_HEIGHT,
            background,
            &[
                OracleOp::add(SpriteSource::Magic, 1_631, origin_x, origin_y, true),
                OracleOp::add(SpriteSource::Magic, 1_630, origin_x, origin_y, true),
            ],
        ),
        "G3 anchor：跨帧事件各按自己 age 取帧（1630/1631 互不干扰）",
    );

    // —— 换相机中心：事件格不变，格原点随 world_to_screen 移动 ——
    let other_center = Point::new(11, 10);
    let mut shifted_frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut shifted_frame,
        &mut scene,
        &[spell_event(9_331, Spell::FireWall, cell, 0, true)],
        other_center,
        0,
    );
    let (shifted_origin_x, shifted_origin_y) = cell_origin(other_center, cell);
    assert_frame_matches(
        &shifted_frame,
        &oracle_frame(
            FRAME_WIDTH,
            FRAME_HEIGHT,
            background,
            &[OracleOp::add(
                SpriteSource::Magic,
                1_630,
                shifted_origin_x,
                shifted_origin_y,
                true,
            )],
        ),
        "G3 anchor：换相机中心后锚点必须随 world_to_screen 移动（不写死屏幕坐标）",
    );
    let (shifted_x, shifted_y) = frame_top_left(other_center, cell, spec0);
    assert_eq!((shifted_x, shifted_y), (333, 261));
    assert_eq!(
        pixel_at(&shifted_frame, shifted_x, shifted_y),
        expand565(spec0.quad[0])
    );
    cleanup(&dir);
}

/// G3：屏外格锚点先按当帧 offset 移动再裁剪；负坐标像素部分裁剪。
#[test]
fn r71_g3_offscreen_crop() {
    let (mut scene, dir) = scene_with_default("g3-crop");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let background = [0, 0, 0, 255];

    // ① 屏外格锚点被当帧 offset 移入：age=360 → 1633，offset (50,-30)；
    //    格 (1,9) 原点 (-48,224) → 帧左上 (2,194)，4 像素全部落屏。
    let offscreen_cell = Point::new(1, 9);
    let mut snapshot = WorldSnapshot::new_session();
    push_object_spell_via_wire(&mut snapshot, 9_401, Spell::FireWall, offscreen_cell, 0);
    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, 360);
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut frame,
        &mut scene,
        &snapshot.spell_events,
        center,
        ui.time_millis,
    );
    let (offscreen_origin_x, offscreen_origin_y) = cell_origin(center, offscreen_cell);
    assert_eq!((offscreen_origin_x, offscreen_origin_y), (-48, 224));
    let spec = frame_spec(SpriteSource::Magic, 1_633);
    assert_frame_matches(
        &frame,
        &oracle_frame(
            FRAME_WIDTH,
            FRAME_HEIGHT,
            background,
            &[OracleOp::add(
                SpriteSource::Magic,
                1_633,
                offscreen_origin_x,
                offscreen_origin_y,
                true,
            )],
        ),
        "G3 offscreen：屏外格锚点必须先按当帧 offset 移动再裁剪（offset 先于裁剪）",
    );
    let (x, y) = frame_top_left(center, offscreen_cell, spec);
    assert_eq!((x, y), (2, 194));
    assert_eq!(pixel_at(&frame, x, y), expand565(spec.quad[0]));
    assert_eq!(covered_count(&frame, background), 4);

    // ② 负坐标部分裁剪：age=240 → 1632，offset (-1,-1)；格 (2,2) 原点 (0,0)
    //    → 帧左上 (-1,-1)，2×2 只剩右下角 (0,0)。
    let partial_cell = Point::new(2, 2);
    let mut partial_snapshot = WorldSnapshot::new_session();
    push_object_spell_via_wire(
        &mut partial_snapshot,
        9_402,
        Spell::FireWall,
        partial_cell,
        0,
    );
    let mut partial_ui = ui_state();
    sync_at(&mut partial_ui, &partial_snapshot, 240);
    let mut partial_frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut partial_frame,
        &mut scene,
        &partial_snapshot.spell_events,
        center,
        partial_ui.time_millis,
    );
    let (partial_origin_x, partial_origin_y) = cell_origin(center, partial_cell);
    assert_eq!((partial_origin_x, partial_origin_y), (0, 0));
    let partial_spec = frame_spec(SpriteSource::Magic, 1_632);
    assert_frame_matches(
        &partial_frame,
        &oracle_frame(
            FRAME_WIDTH,
            FRAME_HEIGHT,
            background,
            &[OracleOp::add(
                SpriteSource::Magic,
                1_632,
                partial_origin_x,
                partial_origin_y,
                true,
            )],
        ),
        "G3 offscreen：偏移后负坐标像素必须被裁剪，只保留屏内部分",
    );
    let expected_positions = [(0u32, 0u32)];
    assert_eq!(
        covered_positions(&partial_frame.pixels, partial_frame.width, background),
        expected_positions
    );
    assert_eq!(
        pixel_at(&partial_frame, 0, 0),
        expand565(partial_spec.quad[3]),
        "G3 offscreen：可见像素必须是右下角（左上角被裁掉）"
    );
    assert_eq!(pixel_at(&partial_frame, -1, 0), [0, 0, 0, 0]);

    // ③ 只截一侧：格 (2,3) 原点 (0,32) → 帧左上 (-1,31)，可见 (0,31)、(0,32)。
    let edge_cell = Point::new(2, 3);
    let mut edge_snapshot = WorldSnapshot::new_session();
    push_object_spell_via_wire(&mut edge_snapshot, 9_403, Spell::FireWall, edge_cell, 0);
    let mut edge_ui = ui_state();
    sync_at(&mut edge_ui, &edge_snapshot, 240);
    let mut edge_frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut edge_frame,
        &mut scene,
        &edge_snapshot.spell_events,
        center,
        edge_ui.time_millis,
    );
    let (edge_origin_x, edge_origin_y) = cell_origin(center, edge_cell);
    assert_eq!((edge_origin_x, edge_origin_y), (0, 32));
    assert_frame_matches(
        &edge_frame,
        &oracle_frame(
            FRAME_WIDTH,
            FRAME_HEIGHT,
            background,
            &[OracleOp::add(
                SpriteSource::Magic,
                1_632,
                edge_origin_x,
                edge_origin_y,
                true,
            )],
        ),
        "G3 offscreen：帧锚点 (-1,31) 时只保留右列可见像素",
    );
    let expected_edge_positions = [(0u32, 31u32), (0u32, 32u32)];
    assert_eq!(
        covered_positions(&edge_frame.pixels, edge_frame.width, background),
        expected_edge_positions
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G4：非黑背景上的逐格 Add 叠加
// ---------------------------------------------------------------------------

/// G4：五格十字在非黑背景上必须逐通道 `min(255, dst + round(src * a / 255))` 叠加
/// （不是 Over 覆盖），每格独立一张 3×3 帧、格间空隙保持背景。
#[test]
fn r71_g4_add_per_cell() {
    let (mut scene, dir) = scene_with_default("g4");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    // 非黑背景：Add 与 Over 在这里结果不同，能证明 149 分支走 `draw_effect_blend`。
    let background = [40, 50, 60, 255];
    let cross = fire_wall_cross(center);
    let mut snapshot = WorldSnapshot::new_session();
    for (slot, cell) in cross.iter().enumerate() {
        let object_id = 9_501 + u32::try_from(slot).expect("十字槽位");
        push_object_spell_via_wire(&mut snapshot, object_id, Spell::FireWall, *cell, 0);
    }
    assert_eq!(snapshot.spell_events.len(), 5);
    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, 0);
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut frame,
        &mut scene,
        &snapshot.spell_events,
        center,
        ui.time_millis,
    );

    let mut ops = Vec::with_capacity(cross.len());
    for cell in cross {
        let (x, y) = cell_origin(center, cell);
        ops.push(OracleOp::add(SpriteSource::Magic, 1_630, x, y, true));
    }
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G4：五格十字 Add 叠加必须逐通道 dst + round(src*a/255) 饱和（不是 Over）",
    );

    // 逐格逐像素：dst=(40,50,60) + src=(248,0,0) → (255,50,60)。
    let spec = frame_spec(SpriteSource::Magic, 1_630);
    let expected = [
        oracle_add_channel(40, 248, 255),
        oracle_add_channel(50, 0, 255),
        oracle_add_channel(60, 0, 255),
        255,
    ];
    assert_eq!(expected, [255, 50, 60, 255]);
    for cell in fire_wall_cross(center) {
        let (x, y) = frame_top_left(center, cell, spec);
        for (column, row, _value) in frame_pixels(spec) {
            assert_eq!(
                pixel_at(&frame, x + column, y + row),
                expected,
                "G4：格 {cell:?} 像素 ({},{}) 的 Add 结果不符",
                x + column,
                y + row
            );
        }
    }
    assert_eq!(
        covered_count(&frame, background),
        45,
        "G4：五格各 9 像素，互不重叠"
    );

    // 格间空隙保持背景（没有错误居中/双叠污染）。
    let (center_x, center_y) = cell_origin(center, center);
    for (x, y) in [
        (center_x + 16, center_y + 2),
        (center_x - 16, center_y + 4),
        (center_x + 2, center_y - 16),
        (center_x + 2, center_y + 32),
    ] {
        assert_eq!(
            pixel_at(&frame, x, y),
            background,
            "G4：格间空隙 ({x},{y}) 必须保持背景"
        );
    }
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G5：旧路径金样 + 缺帧显式无图
// ---------------------------------------------------------------------------

/// G5：非 149 来源的 `FireWall`、`ThunderBolt`/`Lightning`、`PoisonCloud` 必须与
/// 旧通用路径（48×32 居中 + Over 整帧、不消费帧 offset）完全一致。
#[test]
fn r71_g5_legacy_sources() {
    let (mut scene, dir) = scene_with_default("g5-legacy");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let background = [0, 0, 0, 255];
    let fire_wall_cell = Point::new(5, 5);
    let thunder_cell = Point::new(7, 5);
    let lightning_cell = Point::new(9, 5);
    let poison_cell = Point::new(11, 5);
    // 事件时钟：now=120；FireWall/ThunderBolt/Lightning age=0，PoisonCloud age=120。
    let events = [
        spell_event(9_601, Spell::FireWall, fire_wall_cell, 120, false),
        spell_event(9_602, Spell::ThunderBolt, thunder_cell, 120, false),
        spell_event(9_603, Spell::Lightning, lightning_cell, 120, false),
        spell_event(9_604, Spell::PoisonCloud, poison_cell, 0, false),
    ];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, center, 120);

    let mut ops = Vec::new();
    for (source, index, cell) in [
        (SpriteSource::Magic, 0u32, fire_wall_cell),
        (SpriteSource::Magic2, 20, thunder_cell),
        (SpriteSource::Magic2, 20, lightning_cell),
        (SpriteSource::Magic, 2, poison_cell),
    ] {
        let spec = frame_spec(source, index);
        let (x, y) = legacy_top_left(center, cell, spec);
        ops.push(OracleOp::over(source, index, x, y, false));
    }
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G5 legacy：非 149 来源必须保持旧居中 + Over 整帧金样（不得走 FireWall 六帧映射）",
    );

    // 旧路径不消费帧头 offset：位置严格是 (48-w)/2、(32-h)/2。
    let magic0 = frame_spec(SpriteSource::Magic, 0);
    let (fire_x, fire_y) = legacy_top_left(center, fire_wall_cell, magic0);
    let (fire_origin_x, fire_origin_y) = cell_origin(center, fire_wall_cell);
    assert_eq!(
        (fire_x - fire_origin_x, fire_y - fire_origin_y),
        (23, 15),
        "G5 legacy：旧通用闪光按 48×32 居中（(48-2)/2=23、(32-2)/2=15）"
    );
    assert_eq!(pixel_at(&frame, fire_x, fire_y), expand565(magic0.quad[0]));
    assert_eq!(
        pixel_at(&frame, fire_origin_x - 3, fire_origin_y + 5),
        background,
        "G5 legacy：非 149 来源不得画到 1630 六帧位置"
    );

    // Bolt 旧路径用 magic2 的起始帧 20。
    let bolt_frame = frame_spec(SpriteSource::Magic2, 20);
    let (thunder_x, thunder_y) = legacy_top_left(center, thunder_cell, bolt_frame);
    assert_eq!(
        pixel_at(&frame, thunder_x, thunder_y),
        expand565(bolt_frame.quad[0]),
        "G5 legacy：ThunderBolt 必须画 magic2[20]（Bolt start_frame=20）"
    );

    // PoisonCloud age=120 仍按旧 60ms/帧 → magic.wzl[2]。
    let poison_frame = frame_spec(SpriteSource::Magic, 2);
    let (poison_x, poison_y) = legacy_top_left(center, poison_cell, poison_frame);
    assert_eq!(
        pixel_at(&frame, poison_x, poison_y),
        expand565(poison_frame.quad[0]),
        "G5 legacy：PoisonCloud 必须画 magic.wzl[2]（旧 60ms/帧）"
    );
    cleanup(&dir);
}

/// G5：缺 `1631`/无 `magic` 时显式无图（不得退回 0 帧），混合列表的后续事件仍能绘制。
#[test]
fn r71_g5_missing_frame() {
    let frames_without_1631 = magic_without(1_631);
    let (mut scene, dir) = scene_with_libraries("g5-missing", &frames_without_1631, &MAIN_MAGIC2);
    // 缺 1631 但 0 帧存在：若实现「缺帧退 0 帧」会被本用例抓到。
    assert_eq!(
        scene.effect_size(FxKind::Magic, 1_631),
        None,
        "夹具必须缺 magic.wzl[1631]（.wzx 偏移 0 = 没有这张图）"
    );
    assert!(
        scene.effect_size(FxKind::Magic, 0).is_some(),
        "0 帧必须存在，否则无法区分「缺帧无图」与「无素材」"
    );
    assert!(scene.effect_size(FxKind::Magic, 1_630).is_some());

    let center = center_point();
    let background = [10, 20, 30, 255];

    // ① 149 FireWall age=120 → 1631 缺图：显式期望无图（不画占位方块、不退回 0 帧）。
    let mut snapshot = WorldSnapshot::new_session();
    push_object_spell_via_wire(&mut snapshot, 9_701, Spell::FireWall, center, 0);
    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, 120);
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut frame,
        &mut scene,
        &snapshot.spell_events,
        center,
        ui.time_millis,
    );
    assert_only_background(
        &frame,
        background,
        "G5 missing：缺 1631 帧必须显式无图（不得退回 0 帧）",
    );

    // ② 混合列表：缺图事件在前，后续事件（age=0 → 1630）仍必须绘制（不得整体中断）。
    let neighbor = Point::new(11, 10);
    let mut mixed = WorldSnapshot::new_session();
    push_object_spell_via_wire(&mut mixed, 9_711, Spell::FireWall, center, 0);
    push_object_spell_via_wire(&mut mixed, 9_712, Spell::FireWall, neighbor, 120);
    let mut mixed_ui = ui_state();
    sync_at(&mut mixed_ui, &mixed, 120);
    let mut mixed_frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut mixed_frame,
        &mut scene,
        &mixed.spell_events,
        center,
        mixed_ui.time_millis,
    );
    let (neighbor_x, neighbor_y) = cell_origin(center, neighbor);
    assert_frame_matches(
        &mixed_frame,
        &oracle_frame(
            FRAME_WIDTH,
            FRAME_HEIGHT,
            background,
            &[OracleOp::add(
                SpriteSource::Magic,
                1_630,
                neighbor_x,
                neighbor_y,
                true,
            )],
        ),
        "G5 missing：缺图事件不得中断后续事件的绘制",
    );
    let neighbor_spec = frame_spec(SpriteSource::Magic, 1_630);
    let (neighbor_left, neighbor_top) = frame_top_left(center, neighbor, neighbor_spec);
    // 非黑背景上的 Add 结果不是原始色：dst + round(src * a / 255)。
    let red = expand565(neighbor_spec.quad[0]);
    let alpha = fixture_alpha(red);
    let expected_add = [
        oracle_add_channel(background[0], red[0], alpha),
        oracle_add_channel(background[1], red[1], alpha),
        oracle_add_channel(background[2], red[2], alpha),
        255,
    ];
    assert_eq!(
        expected_add,
        [255, 20, 30, 255],
        "G5 missing：背景 (10,20,30) 上 Add 纯红的 oracle 基线"
    );
    assert_eq!(
        pixel_at(&mixed_frame, neighbor_left, neighbor_top),
        expected_add,
        "G5 missing：后续事件必须按 Add 叠在非黑背景上（不是 Over 覆盖）"
    );

    // ③ 「无 Magic」：空实体图库（没有 magic.wzl/magic2.wzl）→ 所有帧缺图，
    //    静默跳过、不 panic、零落屏。
    let (mut empty_scene, empty_dir) = scene_without_entities("g5-missing-empty");
    assert_eq!(
        empty_scene.effect_size(FxKind::Magic, 0),
        None,
        "空实体图库不得有任何 magic 帧"
    );
    let mut empty_frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut empty_frame,
        &mut empty_scene,
        &mixed.spell_events,
        center,
        mixed_ui.time_millis,
    );
    assert_only_background(
        &empty_frame,
        background,
        "G5 missing：无 magic.wzl 时必须静默跳过、不得 panic",
    );

    cleanup(&dir);
    cleanup(&empty_dir);
}
