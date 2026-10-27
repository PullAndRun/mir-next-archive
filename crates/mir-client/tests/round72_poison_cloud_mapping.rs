//! Round 72 A：149 `S.ObjectSpell` 来源的 `Spell::PoisonCloud` 二十帧 elapsed 循环映射。
//!
//! 本文件只验收 [`docs/process/round72-plan.md`] 的有限切片（**不是**完整 Crystal
//! `PoisonCloud` 迁移）：
//!
//! * 仅 149 `ObjectSpell` 来源（`SpellEvent::from_object_spell == true`）的 `PoisonCloud`
//!   走 Crystal `SpellObject.cs:57-64` 的 `Magic2` 帧 `1650..=1669`、120ms/帧、20 帧
//!   elapsed 循环：`phase = (age / 120) % 20`、`index = 1650 + phase`，经
//!   `FxKind::Swing`（`Magic2`、`start_frame() = 0`）零起点寻址复用；
//! * 落点是格锚点 `world_to_screen(center, location)`（`SpellObject.cs:323-339`），
//!   帧头 i16 `offset_x/offset_y` 由 `MapScene::draw_effect_blend(..., apply_offset = true)`
//!   按 `MLibrary.cs:692-709`「当帧 signed offset 恰一次、先 offset 后裁剪」只叠一次；
//! * 混合方式 = `Frame::blit_add`（Crystal `DrawBlend(true, 0.8F)` 的 CPU 子集）。
//!   Crystal 的 `rate = 0.8`、`src * 0.8 * alpha` 调光、`Light3`/`DrawColour` **未实施**
//!   （PARTIAL），本文件不推定两者等价；
//! * 时钟写实：Crystal `SpellObject.cs:288-302` 的严格 `NextMotion`、初始 100ms 取整、
//!   每 `Process` 至多一步**未实施**（PARTIAL），本切片只验证 elapsed 帧循环；4000ms 是
//!   产品瞬态窗（`spellfx::AREA_MS`），不是 Crystal 对象寿命；
//! * `FxKind::Swing` 的零起点入口只作为 `Magic2` 帧号寻址复用，**仅限本 149 毒云分支**，
//!   不改变 `Swing` 的公共语义，也不迁移 `1620` 起手帧；
//! * 非 149 来源（`S.Magic`(120) / `S.ObjectMagic`(123)）的 `PoisonCloud` 与其它技能
//!   保持旧通用路径（60ms/帧、clamp 0..9、48×32 居中 + Over 整帧）；其他 26 技能映射
//!   仍 GAP-5。
//!
//! 独立 oracle：本文件内合成 `magic.wzl`/`magic.wzx`、`magic2.wzl`/`magic2.wzx`，
//! 从原始 RGB565 像素按 `Frame::blit`/`Frame::blit_add` 的文档公式手算整帧 RGBA，
//! **不调用**生产 `blit/blit_add/add_channel/blend_channel`。
//!
//! 运行（缺 `legacy` feature 时合成图库不可读，首条夹具断言以诊断文本失败而非 SKIP）：
//!
//! ```text
//! cargo test -p mir-client --features mir-render/legacy --test round72_poison_cloud_mapping -- --test-threads=1
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use mir_client::net::{SpellEvent, WorldSnapshot};
use mir_client::ui::UiState;
use mir_client::ui::spellfx;
use mir_core::enums::{MirDirection, Spell};
use mir_core::{Language, Point};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{Magic, ObjectMagic, ObjectSpell, decode_server_packet};
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
/// | 0..=9 | 2×2 | (0,0) | 旧通用闪光诱饵（毒云新分支不得画） |
/// | 1620 | 2×2 | (0,0) | Crystal FireWall 起手帧诱饵（本切片不迁移起手） |
/// | 1630 | 3×3 | (-3,5) | R71 FireWall phase 0 金样（G5 保持旧行为） |
/// | 1631 | 2×2 | (4,-2) | R71 FireWall phase 1 金样 |
/// | 1632 | 2×2 | (-1,-1) | R71 FireWall 负坐标裁剪 |
/// | 1633 | 2×2 | (50,-30) | R71 FireWall 屏外移入 |
/// | 1634 | 2×2 | (7,6) | R71 边界 age=599 |
/// | 1635 | 2×2 | (-5,4) | R71 边界 age=600/719 |
/// | 1650 | 2×2 | (0,0) | `FxKind::Magic` 同索引诱饵：新分支错用 `Magic` 会画到它 |
static MAIN_MAGIC: [MagicFrame; 18] = [
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
    MagicFrame::new(1_650, SIZE_2X2, (0, 0), [0x1234; 4]),
];

/// 合成 `magic2.wzl` 帧表（`FxKind::Swing` / `FxKind::Bolt` → 库 `magic2`）。
///
/// | 帧 | 边长 | offset | 用途 |
/// |---|---|---|---|
/// | 0 | 2×2 | (0,0) | 零起点诱饵：错把 20 帧 phase 当索引会画到它 |
/// | 20..=29 | 2×2 | (0,0) | 旧 Bolt 路径（`ThunderBolt`/`Lightning`）金样帧 |
/// | 1650 | 3×3 | (-3,5) | 毒云 phase 0；3×3 用于证明不按尺寸居中 |
/// | 1651 | 2×2 | (4,-2) | 毒云 phase 1；尺寸与 1650 不同 |
/// | 1652 | 2×2 | (-1,-1) | 毒云负坐标部分裁剪 |
/// | 1653 | 2×2 | (50,-30) | 屏外格锚点被当帧 offset 移入 |
/// | 1654..=1669 | 2×2 | 小偏移 | 毒云 phase 4..=19，逐帧不同色/offset |
/// | 1670 | 2×2 | (0,0) | `FxKind::Bolt` 同索引诱饵：错用 `Bolt` 会画 `1650 + 20` |
static MAIN_MAGIC2: [MagicFrame; 32] = [
    MagicFrame::new(0, SIZE_2X2, (0, 0), [0x2222; 4]),
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
    MagicFrame::new(1_650, SIZE_3X3, (-3, 5), [0xF820; 4]),
    MagicFrame::new(1_651, SIZE_2X2, (4, -2), [0x07F0; 4]),
    MagicFrame::new(1_652, SIZE_2X2, (-1, -1), [0x001F; 4]),
    MagicFrame::new(1_653, SIZE_2X2, (50, -30), [0xFFE0; 4]),
    MagicFrame::new(1_654, SIZE_2X2, (0, 0), [0x4010; 4]),
    MagicFrame::new(1_655, SIZE_2X2, (1, 1), [0x4011; 4]),
    MagicFrame::new(1_656, SIZE_2X2, (-2, 0), [0x4020; 4]),
    MagicFrame::new(1_657, SIZE_2X2, (0, -2), [0x4021; 4]),
    MagicFrame::new(1_658, SIZE_2X2, (2, -1), [0x4040; 4]),
    MagicFrame::new(1_659, SIZE_2X2, (-1, 2), [0x4041; 4]),
    MagicFrame::new(1_660, SIZE_2X2, (1, -2), [0x4080; 4]),
    MagicFrame::new(1_661, SIZE_2X2, (-2, 1), [0x4081; 4]),
    MagicFrame::new(1_662, SIZE_2X2, (2, 2), [0x4100; 4]),
    MagicFrame::new(1_663, SIZE_2X2, (-1, -2), [0x4101; 4]),
    MagicFrame::new(1_664, SIZE_2X2, (3, 0), [0x4200; 4]),
    MagicFrame::new(1_665, SIZE_2X2, (0, 3), [0x4201; 4]),
    MagicFrame::new(1_666, SIZE_2X2, (-3, 0), [0x4400; 4]),
    MagicFrame::new(1_667, SIZE_2X2, (0, -3), [0x4401; 4]),
    MagicFrame::new(1_668, SIZE_2X2, (3, -3), [0x4800; 4]),
    MagicFrame::new(1_669, SIZE_2X2, (-3, 3), [0x4801; 4]),
    MagicFrame::new(1_670, SIZE_2X2, (0, 0), [0x3456; 4]),
];

// ---------------------------------------------------------------------------
// 夹具：进程唯一临时目录 + 合成 WZL/WZX（真实 `EntityLibraries` 读取链）
// ---------------------------------------------------------------------------

/// 进程唯一临时目录（不读 `resource`/`game-assets`，也不与其它测试共享路径）。
///
/// 优先系统临时目录；被环境策略拒绝写时回退到 `target/round72/a/tmp/`（测试产物目录）。
fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let name = format!("mir-next-r72-a-{tag}-{}-{nanos}", std::process::id());
    let candidates = [
        std::env::temp_dir().join(&name),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/round72/a/tmp")
            .join(&name),
    ];
    for dir in candidates {
        if fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }
    panic!("无法创建进程唯一临时目录（系统临时目录与 target/round72/a/tmp 均不可写）");
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
        file_name: "r72-empty".to_owned(),
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

/// 去掉指定帧号的合成 magic2 帧表（`.wzx` 表项留 0 = 缺帧）。
fn magic2_without(missing: u32) -> Vec<MagicFrame> {
    MAIN_MAGIC2
        .iter()
        .copied()
        .filter(|spec| spec.index != missing)
        .collect()
}

/// 只写 `magic.wzl`、不写 `magic2.wzl` 的场景（缺 `Magic2` 图库）。
fn scene_with_magic_only(tag: &str) -> (MapScene, PathBuf) {
    let dir = unique_dir(tag);
    write_library(&dir, "magic", &MAIN_MAGIC);
    (scene_in_dir(&dir), dir)
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
        "合成 magic.wzl[1620] FireWall 起手诱饵必须可读"
    );
    assert_eq!(
        scene.effect_size(FxKind::Magic, 1_630),
        Some((3, 3)),
        "合成 magic.wzl[1630] 必须是 3×3（R71 金样）"
    );
    for index in 1_631u32..=1_635 {
        assert_eq!(
            scene.effect_size(FxKind::Magic, index),
            Some((2, 2)),
            "合成 magic.wzl[{index}] 必须是 2×2"
        );
    }
    assert_eq!(
        scene.effect_size(FxKind::Magic, 1_650),
        Some((2, 2)),
        "合成 magic.wzl[1650] `FxKind::Magic` 同索引诱饵必须可读"
    );
    assert_eq!(
        scene.effect_size(FxKind::Bolt, 0),
        Some((2, 2)),
        "合成 magic2.wzl[20]（Bolt 起始帧 20）必须可读"
    );
    assert_eq!(
        scene.effect_size(FxKind::Swing, 0),
        Some((2, 2)),
        "合成 magic2.wzl[0] 零起点诱饵必须可读"
    );
    assert_eq!(
        scene.effect_size(FxKind::Swing, 1_650),
        Some((3, 3)),
        "合成 magic2.wzl[1650] 必须是 3×3"
    );
    for index in 1_651u32..=1_669 {
        assert_eq!(
            scene.effect_size(FxKind::Swing, index),
            Some((2, 2)),
            "合成 magic2.wzl[{index}] 必须是 2×2"
        );
    }
    assert_eq!(
        scene.effect_size(FxKind::Bolt, 1_630),
        Some((3, 3)),
        "合成 magic2.wzl[1650] 经 Bolt 可见"
    );
    assert_eq!(
        scene.effect_size(FxKind::Bolt, 1_650),
        Some((2, 2)),
        "合成 magic2.wzl[1670] `FxKind::Bolt` 同索引诱饵必须可读"
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

/// 诱饵颜色：毒云 `1650..=1669` 二十帧之外的所有合成帧像素。
///
/// 新分支若错画旧通用帧（`0..=9`）、`1620` 起手帧、`magic[1650]`
/// （`FxKind::Magic` 同索引）或 `magic2[1670]`（`FxKind::Bolt` 同索引），会命中这些颜色。
/// R71 `FireWall` 六帧（`1630..=1635`）是 G5 允许的旧行为金样，不算诱饵。
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
    for spec in MAIN_MAGIC2
        .iter()
        .filter(|spec| !(1_650..=1_669).contains(&spec.index))
    {
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
fn push_object_spell_full_via_wire(
    snapshot: &mut WorldSnapshot,
    object_id: u32,
    spell: Spell,
    location: Point,
    direction: MirDirection,
    param: bool,
    now_ms: u64,
) {
    let packet = through_wire(&ServerPacket::ObjectSpell(ObjectSpell {
        object_id,
        location,
        spell,
        direction,
        param,
    }));
    snapshot.apply_packet_at(&packet, now_ms);
}

/// 149 `ObjectSpell` 默认朝向 `Down` / `param = false` 的便捷入口。
fn push_object_spell_via_wire(
    snapshot: &mut WorldSnapshot,
    object_id: u32,
    spell: Spell,
    location: Point,
    now_ms: u64,
) {
    push_object_spell_full_via_wire(
        snapshot,
        object_id,
        spell,
        location,
        MirDirection::Down,
        false,
        now_ms,
    );
}

/// 构造真实 `S.Magic`（包 120）并经 wire 链写入快照（非 149 旧来源）。
fn push_magic_via_wire(snapshot: &mut WorldSnapshot, spell: Spell, target: Point, now_ms: u64) {
    let packet = through_wire(&ServerPacket::Magic(Magic {
        spell,
        target_id: 0,
        target,
        cast: true,
        level: 3,
        secondary_target_ids: Vec::new(),
    }));
    snapshot.apply_packet_at(&packet, now_ms);
}

/// 构造真实 `S.ObjectMagic`（包 123）并经 wire 链写入快照（非 149 旧来源）。
fn push_object_magic_via_wire(
    snapshot: &mut WorldSnapshot,
    object_id: u32,
    spell: Spell,
    target: Point,
    now_ms: u64,
) {
    let packet = through_wire(&ServerPacket::ObjectMagic(ObjectMagic {
        object_id,
        location: target,
        direction: MirDirection::Down,
        spell,
        target_id: 0,
        target,
        cast: true,
        level: 3,
        self_broadcast: false,
        secondary_target_ids: Vec::new(),
    }));
    snapshot.apply_packet_at(&packet, now_ms);
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
// G1：149 真实 wire 链 → 两条独立对象事件 → t=0 画 magic2.wzl[1650]
// ---------------------------------------------------------------------------

/// G1：149（`S.ObjectSpell`）来源的 `PoisonCloud` 经真实
/// `encode_frame → decode_server_packet → apply_packet_at` 后，在 `t=0` 必须各画一张
/// `magic2.wzl[1650]`（`FxKind::Swing` 零起点 + 格锚点 + 当帧 offset），不得画
/// `magic[0]` 旧通用帧、`magic[1650]`（`FxKind::Magic` 同索引诱饵）或
/// `magic2[1670]`（`FxKind::Bolt` start 20 诱饵）。
#[test]
fn r72_g1_wire_mapping() {
    let (mut scene, dir) = scene_with_default("g1");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let background = [0, 0, 0, 255];
    let cells = [center, Point::new(center.x + 1, center.y)];
    let mut snapshot = WorldSnapshot::new_session();
    for (slot, cell) in cells.iter().enumerate() {
        push_object_spell_via_wire(
            &mut snapshot,
            9_001 + u32::try_from(slot).expect("事件槽位"),
            Spell::PoisonCloud,
            *cell,
            0,
        );
    }
    assert_eq!(snapshot.spell_events.len(), 2, "两条 149 必须写两条事件");
    for (slot, event) in snapshot.spell_events.iter().enumerate() {
        assert_eq!(event.object_id, 9_001 + u32::try_from(slot).expect("槽位"));
        assert_eq!(event.spell, Spell::PoisonCloud);
        assert_eq!(event.location, cells[slot], "149 逐格落点必须原样保存");
        assert!(
            event.from_object_spell,
            "149 来源必须标记 from_object_spell"
        );
        assert_eq!(event.direction, MirDirection::Down);
        assert!(!event.param);
        assert_eq!(event.started_ms, 0);
    }

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, 0);
    // 4000ms 是产品瞬态窗（`spellfx::AREA_MS`），不是 Crystal `SpellObject` 对象寿命。
    assert_eq!(spellfx::duration_ms(Spell::PoisonCloud), 4_000);

    let mut ops = Vec::with_capacity(cells.len());
    for cell in cells {
        let (x, y) = cell_origin(center, cell);
        ops.push(OracleOp::add(SpriteSource::Magic2, 1_650, x, y, true));
    }
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut frame,
        &mut scene,
        &snapshot.spell_events,
        center,
        ui.time_millis,
    );
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G1：t=0 两条 149 各画 magic2.wzl[1650]（3×3，格原点 + 当帧 offset (-3,5) 只叠一次）",
    );

    let spec = frame_spec(SpriteSource::Magic2, 1_650);
    let magic0 = frame_spec(SpriteSource::Magic, 0);
    let magic_decoy = frame_spec(SpriteSource::Magic, 1_650);
    for cell in cells {
        let (x, y) = frame_top_left(center, cell, spec);
        for (column, row, value) in frame_pixels(spec) {
            assert_eq!(
                pixel_at(&frame, x + column, y + row),
                expand565(value),
                "G1：格 {cell:?} 的 1650 像素 ({},{}) 颜色不符",
                x + column,
                y + row
            );
        }
        // 旧通用闪光（Magic 0 居中）不得出现。
        let (legacy_x, legacy_y) = legacy_top_left(center, cell, magic0);
        for (column, row, _value) in frame_pixels(magic0) {
            assert_eq!(
                pixel_at(&frame, legacy_x + column, legacy_y + row),
                background,
                "G1：旧 60ms 居中位置不得出现 magic.wzl[0]（未迁移的通用路径）"
            );
        }
        // `FxKind::Magic` 同索引诱饵（magic[1650]，offset 0）不得出现在格原点。
        let (origin_x, origin_y) = cell_origin(center, cell);
        for (column, row, _value) in frame_pixels(magic_decoy) {
            assert_eq!(
                pixel_at(&frame, origin_x + column, origin_y + row),
                background,
                "G1：格原点不得出现 magic.wzl[1650]（`FxKind::Magic` 同索引诱饵）"
            );
        }
    }
    assert_eq!(
        covered_count(&frame, background),
        2 * 9,
        "G1：两条各 9 像素"
    );
    assert_no_decoy_colors(&frame, background, "G1");
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G2：帧边界（120ms/帧、20 帧循环）与事件窗口
// ---------------------------------------------------------------------------

/// G2：`phase = (age / 120) % 20` 的每个边界都必须映射到 `1650 + phase`，
/// 覆盖 20 帧 × 2 边界、2400ms 回卷与 3999ms 尾窗，每时点独立清帧。
#[test]
fn r72_g2_frame_boundaries() {
    let (mut scene, dir) = scene_with_default("g2-frame");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let background = [0, 0, 0, 255];
    let mut snapshot = WorldSnapshot::new_session();
    push_object_spell_via_wire(&mut snapshot, 9_201, Spell::PoisonCloud, center, 0);
    let mut ui = ui_state();

    let mut cases = Vec::new();
    for phase in 0..20u32 {
        let index = 1_650 + phase;
        let age = i64::from(phase) * 120;
        cases.push((age, index));
        cases.push((age + 119, index));
    }
    cases.push((2_399, 1_669));
    cases.push((2_400, 1_650));
    cases.push((2_520, 1_651));
    cases.push((3_999, 1_663));
    assert_eq!(cases.len(), 44, "40 个逐帧边界 + 4 个回卷/尾窗边界");

    let (origin_x, origin_y) = cell_origin(center, center);
    for (age, expected_index) in cases {
        sync_at(&mut ui, &snapshot, age);
        let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(
            &mut frame,
            &mut scene,
            &snapshot.spell_events,
            center,
            ui.time_millis,
        );
        let ops = [OracleOp::add(
            SpriteSource::Magic2,
            expected_index,
            origin_x,
            origin_y,
            true,
        )];
        assert_frame_matches(
            &frame,
            &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
            &format!("G2：age={age} 必须映射 magic2.wzl[{expected_index}]"),
        );
        let spec = frame_spec(SpriteSource::Magic2, expected_index);
        assert_eq!(
            covered_count(&frame, background),
            usize::from(spec.size.0) * usize::from(spec.size.1),
            "G2：age={age} 覆盖像素数必须等于帧面积"
        );
        let (x, y) = frame_top_left(center, center, spec);
        assert_eq!(
            pixel_at(&frame, x, y),
            expand565(frame_value(spec, 0, 0)),
            "G2：age={age} 首像素颜色"
        );
    }
    cleanup(&dir);
}

/// G2：4000ms 产品瞬态窗（`spellfx::AREA_MS`）——`age=3999` 仍画 1663，
/// `age=4000` 与负 age / i64 极值零落屏；这不是 Crystal 对象寿命。
#[test]
fn r72_g2_age_window() {
    let (mut scene, dir) = scene_with_default("g2-window");
    assert_default_fixture_readable(&mut scene);
    assert_eq!(spellfx::duration_ms(Spell::PoisonCloud), 4_000);

    let center = center_point();
    let background = [0, 0, 0, 255];
    let started_ms = 1_000;
    let events = [spell_event(1, Spell::PoisonCloud, center, started_ms, true)];

    for now_ms in [999, 5_000, i64::MIN, i64::MAX] {
        let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(&mut frame, &mut scene, &events, center, now_ms);
        assert_only_background(
            &frame,
            background,
            &format!("G2：age 越窗 now={now_ms} 必须零落屏（4000ms 产品瞬态窗）"),
        );
    }

    let (origin_x, origin_y) = cell_origin(center, center);
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, center, started_ms + 3_999);
    let spec = frame_spec(SpriteSource::Magic2, 1_663);
    let ops = [OracleOp::add(
        SpriteSource::Magic2,
        1_663,
        origin_x,
        origin_y,
        true,
    )];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G2：age=3999 仍画 1663（产品窗内尾帧）",
    );
    assert_eq!(
        covered_count(&frame, background),
        usize::from(spec.size.0) * usize::from(spec.size.1)
    );

    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, center, started_ms + 4_000);
    assert_only_background(&frame, background, "G2：age=4000 必须停止绘制");
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G3：格锚点 + 当帧 signed offset（恰一次、先 offset 后裁剪）
// ---------------------------------------------------------------------------

/// G3：落点 = 格原点 + **当帧** i16 offset（只叠一次、不跨帧累积、不按尺寸居中），
/// 且随相机中心平移；混合事件各按自己的 `started_ms` 取帧。
#[test]
fn r72_g3_anchor_offsets() {
    let (mut scene, dir) = scene_with_default("g3-anchor");
    assert_default_fixture_readable(&mut scene);
    let center = center_point();
    let background = [0, 0, 0, 255];
    let events = [spell_event(1, Spell::PoisonCloud, center, 0, true)];

    // age=0 → 1650（3×3，offset (-3,5)）：格原点 (384,256) + 当帧 offset = (381,261)。
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, center, 0);
    let spec = frame_spec(SpriteSource::Magic2, 1_650);
    assert_eq!(
        frame_top_left(center, center, spec),
        (381, 261),
        "G3：格锚点 (384,256) + (-3,5)"
    );
    let ops = [OracleOp::add(SpriteSource::Magic2, 1_650, 384, 256, true)];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G3：age=0 画 1650 且当帧 offset 恰一次",
    );
    for (column, row, value) in frame_pixels(spec) {
        assert_eq!(
            pixel_at(&frame, 381 + column, 261 + row),
            expand565(value),
            "G3：1650 像素 ({},{})",
            381 + column,
            261 + row
        );
    }
    // 只叠一次：把 (-3,5) 叠两次会落在 (378,266)。
    assert_eq!(
        pixel_at(&frame, 378, 266),
        background,
        "G3：offset 不得叠加两次"
    );
    // 不按尺寸居中：旧 60ms 通用路径居中位置不得出图。
    let magic0 = frame_spec(SpriteSource::Magic, 0);
    let (legacy_x, legacy_y) = legacy_top_left(center, center, magic0);
    assert_eq!(
        pixel_at(&frame, legacy_x, legacy_y),
        background,
        "G3：不得退回 60ms 尺寸居中"
    );
    // `FxKind::Magic` 同索引诱饵（格原点）不得出图。
    assert_eq!(
        pixel_at(&frame, 384, 256),
        background,
        "G3：不得用 FxKind::Magic 取 magic[1650]"
    );
    assert_eq!(covered_count(&frame, background), 9);

    // age=120 → 1651（2×2，offset (4,-2)）：跨帧只换当帧 offset，不累积上一帧。
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, center, 120);
    let spec = frame_spec(SpriteSource::Magic2, 1_651);
    assert_eq!(
        frame_top_left(center, center, spec),
        (388, 254),
        "G3：格锚点 + (4,-2)"
    );
    let ops = [OracleOp::add(SpriteSource::Magic2, 1_651, 384, 256, true)];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G3：age=120 画 1651",
    );
    assert_eq!(
        pixel_at(&frame, 381, 261),
        background,
        "G3：上一帧 offset 不得残留"
    );
    assert_eq!(covered_count(&frame, background), 4);

    // 重复 draw 同一事件到新帧：结果与单次一致（无跨调用状态）。
    let mut repeat = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut repeat, &mut scene, &events, center, 120);
    assert_frame_matches(
        &repeat,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G3：重复 draw 无累积",
    );

    // 混合事件：各自按 started_ms 取帧，不同格互不覆盖。
    let mixed = [
        spell_event(2, Spell::PoisonCloud, center, 0, true),
        spell_event(
            3,
            Spell::PoisonCloud,
            Point::new(center.x + 1, center.y),
            120,
            true,
        ),
    ];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &mixed, center, 120);
    let (other_x, other_y) = cell_origin(center, Point::new(center.x + 1, center.y));
    let ops = [
        OracleOp::add(SpriteSource::Magic2, 1_651, 384, 256, true),
        OracleOp::add(SpriteSource::Magic2, 1_650, other_x, other_y, true),
    ];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G3：混合 age 各取各帧",
    );

    // 换相机中心：center=(11,10) 时格 (10,10) 原点 (336,256)，+(-3,5) → (333,261)。
    let other_center = Point::new(center.x + 1, center.y);
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, other_center, 0);
    let spec = frame_spec(SpriteSource::Magic2, 1_650);
    assert_eq!(
        frame_top_left(other_center, center, spec),
        (333, 261),
        "G3：换中心后仍按格锚点"
    );
    let ops = [OracleOp::add(SpriteSource::Magic2, 1_650, 336, 256, true)];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G3：换相机中心整帧",
    );
    cleanup(&dir);
}

/// G3：先 offset 后裁剪——屏外格锚点被当帧 offset 移入、负坐标只裁掉出屏部分、
/// 完全屏外零像素且不 panic。
#[test]
fn r72_g3_offscreen_crop() {
    let (mut scene, dir) = scene_with_default("g3-crop");
    assert_default_fixture_readable(&mut scene);
    let center = center_point();
    let background = [0, 0, 0, 255];

    // age=360 → 1653（offset (50,-30)）；格 (1,9) 原点 (-48,224) 经 offset 移入 → (2,194)。
    let cell = Point::new(1, 9);
    let events = [spell_event(1, Spell::PoisonCloud, cell, 0, true)];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, center, 360);
    assert_eq!(cell_origin(center, cell), (-48, 224));
    let spec = frame_spec(SpriteSource::Magic2, 1_653);
    assert_eq!(
        frame_top_left(center, cell, spec),
        (2, 194),
        "G3：屏外格锚点经当帧 offset 移入"
    );
    let ops = [OracleOp::add(SpriteSource::Magic2, 1_653, -48, 224, true)];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G3：offset 移入整帧",
    );
    assert_eq!(covered_count(&frame, background), 4);

    // age=240 → 1652（offset (-1,-1)）；格 (2,2) 原点 (0,0) → 仅右下 (0,0) 可见。
    let cell = Point::new(2, 2);
    let events = [spell_event(1, Spell::PoisonCloud, cell, 0, true)];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, center, 240);
    let spec = frame_spec(SpriteSource::Magic2, 1_652);
    assert_eq!(frame_top_left(center, cell, spec), (-1, -1));
    let ops = [OracleOp::add(SpriteSource::Magic2, 1_652, 0, 0, true)];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G3：负坐标裁剪整帧",
    );
    assert_eq!(
        pixel_at(&frame, 0, 0),
        expand565(spec.quad[3]),
        "G3：仅右下角像素可见"
    );
    assert_eq!(covered_count(&frame, background), 1);

    // 单边裁剪：格 (2,3) 原点 (0,32) → (-1,31)，可见右列 (0,31)/(0,32)。
    let cell = Point::new(2, 3);
    let events = [spell_event(1, Spell::PoisonCloud, cell, 0, true)];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, center, 240);
    let ops = [OracleOp::add(SpriteSource::Magic2, 1_652, 0, 32, true)];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G3：单边裁剪整帧",
    );
    assert_eq!(pixel_at(&frame, 0, 31), expand565(spec.quad[1]));
    assert_eq!(pixel_at(&frame, 0, 32), expand565(spec.quad[3]));
    assert_eq!(covered_count(&frame, background), 2);

    // 完全屏外：格 (0,0) 原点 (-96,-64) → (-97,-65)，零像素、不 panic。
    let cell = Point::new(0, 0);
    let events = [spell_event(1, Spell::PoisonCloud, cell, 0, true)];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, center, 240);
    assert_eq!(
        frame_top_left(center, cell, frame_spec(SpriteSource::Magic2, 1_652)),
        (-97, -65)
    );
    assert_only_background(&frame, background, "G3：完全屏外 anchor 不 panic 且零落屏");
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G4：Add 混合语义（非黑底、饱和、同格叠加、旁邻空白）
// ---------------------------------------------------------------------------

/// G4：149 毒云复用 R70 的 `blit_add` 入口——逐 RGB
/// `min(255, dst + (src * alpha + 127) / 255)`、目标 alpha 沿用既有 `Frame` 规则
/// （不透明）；非黑背景、通道饱和、同格两事件叠加、旁邻格空白。
#[test]
fn r72_g4_add_per_cell() {
    let (mut scene, dir) = scene_with_default("g4-add");
    assert_default_fixture_readable(&mut scene);
    let center = center_point();
    let background = [40, 50, 60, 255];
    let events = [
        spell_event(1, Spell::PoisonCloud, center, 0, true),
        spell_event(2, Spell::PoisonCloud, center, 0, true),
    ];

    let (origin_x, origin_y) = cell_origin(center, center);
    let ops = [
        OracleOp::add(SpriteSource::Magic2, 1_650, origin_x, origin_y, true),
        OracleOp::add(SpriteSource::Magic2, 1_650, origin_x, origin_y, true),
    ];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, center, 0);
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G4：同格两次 Add 整帧",
    );

    let spec = frame_spec(SpriteSource::Magic2, 1_650);
    let color = expand565(0xF820);
    assert_eq!(color, [248, 4, 0, 255], "RGB565 0xF820 展开（oracle 基线）");
    let (x, y) = frame_top_left(center, center, spec);
    // 手算：第一次 (40+248→255, 50+4=54, 60+0=60)；第二次 (255, 58, 60)。
    assert_eq!(
        pixel_at(&frame, x, y),
        [255, 58, 60, 255],
        "G4：Add 公式 min(255, dst + (src * a + 127) / 255) 逐通道"
    );
    assert_eq!(
        pixel_at(&frame, x, y)[3],
        255,
        "G4：目标 alpha 沿用既有 Frame 规则"
    );
    // 旁邻格（未发事件）保持背景。
    let neighbor = cell_origin(center, Point::new(center.x + 1, center.y));
    assert_eq!(
        pixel_at(&frame, neighbor.0, neighbor.1),
        background,
        "G4：旁邻格必须空白"
    );
    // 同格两事件只覆盖 3×3 并集。
    assert_eq!(covered_count(&frame, background), 9);
    // Over 结果 (248,0,0) 不得出现（隔离混合因子）。
    assert_ne!(pixel_at(&frame, x, y), [248, 0, 0, 255]);
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G5：旧来源分流、缺帧/缺库、诊断字段
// ---------------------------------------------------------------------------

/// G5：非 149 来源的 `PoisonCloud`（真实 `S.Magic`(120) / `S.ObjectMagic`(123) wire）
/// 必须继续走旧通用路径（`magic[0]` 2×2 整帧 Over 居中），不得复用 1650 映射；
/// 同时 149 `FireWall`（1630）、`Blizzard`/`TrapHexagon`（旧通用 `magic[2]`）与
/// `ThunderBolt` 金样（`magic2[20]`）保持 R71/旧行为不变。
#[test]
fn r72_g5_legacy_sources() {
    let (mut scene, dir) = scene_with_default("g5-legacy");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let background = [0, 0, 0, 255];
    let cells = [
        center,
        Point::new(center.x + 1, center.y),
        Point::new(center.x + 2, center.y),
        Point::new(center.x, center.y - 1),
        Point::new(center.x - 1, center.y),
        Point::new(center.x, center.y + 1),
    ];
    let mut snapshot = WorldSnapshot::new_session();
    // 旧来源（非 149）：真实 S.Magic(120) / S.ObjectMagic(123)，落点取 Target。
    push_magic_via_wire(&mut snapshot, Spell::PoisonCloud, cells[3], 120);
    push_object_magic_via_wire(&mut snapshot, 7_777, Spell::PoisonCloud, cells[4], 120);
    // 149：FireWall 在 t=120 生效（age=0 → magic[1630]）；Blizzard/TrapHexagon 在
    // t=0 生效（age=120 → 旧通用 magic[2]）。
    push_object_spell_via_wire(&mut snapshot, 9_101, Spell::FireWall, cells[0], 120);
    push_object_spell_via_wire(&mut snapshot, 9_102, Spell::Blizzard, cells[1], 0);
    push_object_spell_via_wire(&mut snapshot, 9_103, Spell::TrapHexagon, cells[2], 0);
    assert_eq!(snapshot.spell_events.len(), 5);

    assert_eq!(snapshot.spell_events[0].spell, Spell::PoisonCloud);
    assert!(
        !snapshot.spell_events[0].from_object_spell,
        "S.Magic 不得置位 from_object_spell"
    );
    assert_eq!(snapshot.spell_events[0].object_id, 0, "S.Magic 无对象编号");
    assert_eq!(snapshot.spell_events[0].location, cells[3]);
    assert_eq!(snapshot.spell_events[0].started_ms, 120);
    assert_eq!(snapshot.spell_events[1].spell, Spell::PoisonCloud);
    assert!(
        !snapshot.spell_events[1].from_object_spell,
        "S.ObjectMagic 不得置位 from_object_spell"
    );
    assert_eq!(snapshot.spell_events[1].object_id, 7_777);
    assert_eq!(snapshot.spell_events[1].location, cells[4]);
    for (slot, spell, cell) in [
        (2usize, Spell::FireWall, cells[0]),
        (3, Spell::Blizzard, cells[1]),
        (4, Spell::TrapHexagon, cells[2]),
    ] {
        let event = &snapshot.spell_events[slot];
        assert_eq!(event.spell, spell);
        assert!(
            event.from_object_spell,
            "149 来源必须置位 from_object_spell"
        );
        assert_eq!(event.location, cell);
    }

    let mut events = snapshot.spell_events.clone();
    events.push(spell_event(99, Spell::ThunderBolt, cells[5], 120, false));
    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, 120);
    assert_eq!(ui.time_millis, 120);

    let magic0 = frame_spec(SpriteSource::Magic, 0);
    let magic2 = frame_spec(SpriteSource::Magic, 2);
    let fire_spec = frame_spec(SpriteSource::Magic, 1_630);
    let bolt = frame_spec(SpriteSource::Magic2, 20);
    let poison_magic_top = legacy_top_left(center, cells[3], magic0);
    let poison_object_top = legacy_top_left(center, cells[4], magic0);
    let fire_top = frame_top_left(center, cells[0], fire_spec);
    let blizzard_top = legacy_top_left(center, cells[1], magic2);
    let trap_top = legacy_top_left(center, cells[2], magic2);
    let bolt_top = legacy_top_left(center, cells[5], bolt);
    let fire_cell_origin = cell_origin(center, cells[0]);
    let ops = [
        OracleOp::over(
            SpriteSource::Magic,
            0,
            poison_magic_top.0,
            poison_magic_top.1,
            false,
        ),
        OracleOp::over(
            SpriteSource::Magic,
            0,
            poison_object_top.0,
            poison_object_top.1,
            false,
        ),
        OracleOp::add(
            SpriteSource::Magic,
            1_630,
            fire_cell_origin.0,
            fire_cell_origin.1,
            true,
        ),
        OracleOp::over(
            SpriteSource::Magic,
            2,
            blizzard_top.0,
            blizzard_top.1,
            false,
        ),
        OracleOp::over(SpriteSource::Magic, 2, trap_top.0, trap_top.1, false),
        OracleOp::over(SpriteSource::Magic2, 20, bolt_top.0, bolt_top.1, false),
    ];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, center, 120);
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G5：旧来源分流整帧",
    );

    // 非 149 毒云仍画旧 magic[0]（Over、居中）。
    assert_eq!(
        pixel_at(&frame, poison_magic_top.0, poison_magic_top.1),
        expand565(0x8410),
        "G5：S.Magic 毒云仍走旧通用路径"
    );
    assert_eq!(
        pixel_at(&frame, poison_object_top.0, poison_object_top.1),
        expand565(0x8410),
        "G5：S.ObjectMagic 毒云仍走旧通用路径"
    );
    // 非 149 毒云不得落 1650 目标帧位置（错用 149 分支会画 magic2[1650]）。
    let poison_origin = cell_origin(center, cells[3]);
    assert_eq!(
        pixel_at(&frame, poison_origin.0 - 3, poison_origin.1 + 5),
        background,
        "G5：非 149 毒云不得画 1650 帧"
    );
    // FireWall 1630 与 Bolt 金样保持。
    assert_eq!(
        pixel_at(&frame, fire_top.0, fire_top.1),
        expand565(0xF800),
        "G5：149 FireWall 仍画 magic[1630]"
    );
    assert_eq!(
        pixel_at(&frame, bolt_top.0, bolt_top.1),
        expand565(0x8410),
        "G5：ThunderBolt 仍画 magic2[20]"
    );
    cleanup(&dir);
}

/// G5：缺帧/缺图库——缺 `magic2[1651]` 时该事件不画（不退 1650/1652/0、不 panic），
/// 同帧后续 149 事件仍绘制；完全空图库安全跳过。
#[test]
fn r72_g5_missing_frame() {
    let magic2 = magic2_without(1_651);
    let (mut scene, dir) = scene_with_libraries("g5-missing", &MAIN_MAGIC, &magic2);
    assert_eq!(
        scene.effect_size(FxKind::Swing, 1_651),
        None,
        "缺帧必须报告 None（不 panic）"
    );
    assert_eq!(scene.effect_size(FxKind::Swing, 1_650), Some((3, 3)));
    assert_eq!(scene.effect_size(FxKind::Swing, 1_652), Some((2, 2)));

    let center = center_point();
    let background = [0, 0, 0, 255];
    let mut snapshot = WorldSnapshot::new_session();
    push_object_spell_via_wire(&mut snapshot, 9_500, Spell::PoisonCloud, center, 0);
    push_object_spell_via_wire(
        &mut snapshot,
        9_501,
        Spell::FireWall,
        Point::new(center.x + 1, center.y),
        0,
    );
    assert_eq!(snapshot.spell_events.len(), 2);

    // age=120 → 毒云请求缺帧 1651（不画）；FireWall age=120 → magic[1631] 仍画。
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &snapshot.spell_events, center, 120);
    let fire_origin = cell_origin(center, Point::new(center.x + 1, center.y));
    let ops = [OracleOp::add(
        SpriteSource::Magic,
        1_631,
        fire_origin.0,
        fire_origin.1,
        true,
    )];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G5：缺 1651 的毒云不画，后续 149 仍绘制",
    );
    assert_eq!(covered_count(&frame, background), 4);

    // 只发缺帧毒云：零落屏且不 panic（不退 1650/1652/旧 0）。
    let poison_only = [spell_event(1, Spell::PoisonCloud, center, 0, true)];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &poison_only, center, 120);
    assert_only_background(&frame, background, "G5：缺 1651 不得回退到 1650/1652/0");

    // 缺 `magic2` 图库：安全跳过，不 panic。
    let (mut magic_only_scene, magic_only_dir) = scene_with_magic_only("g5-magic-only");
    assert_eq!(magic_only_scene.effect_size(FxKind::Swing, 1_650), None);
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut magic_only_scene, &poison_only, center, 0);
    assert_only_background(&frame, background, "G5：缺 magic2 图库安全跳过");

    // 完全空实体图库：同样安全。
    let (mut empty_scene, empty_dir) = scene_without_entities("g5-empty");
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut empty_scene, &poison_only, center, 0);
    assert_only_background(&frame, background, "G5：空实体库安全跳过");

    cleanup(&dir);
    cleanup(&magic_only_dir);
    cleanup(&empty_dir);
}

/// G5：`direction`/`param` 只是诊断字段——不改变 149 毒云的帧号与落点
/// （Crystal `SpellObject` 不带朝向参数；`Light3`/`DrawColour` 未迁移，维持 PARTIAL）。
#[test]
fn r72_g5_param_direction() {
    let (mut scene, dir) = scene_with_default("g5-param");
    assert_default_fixture_readable(&mut scene);
    let center = center_point();
    let background = [0, 0, 0, 255];
    let mut snapshot = WorldSnapshot::new_session();
    push_object_spell_full_via_wire(
        &mut snapshot,
        9_301,
        Spell::PoisonCloud,
        center,
        MirDirection::Up,
        true,
        0,
    );
    push_object_spell_full_via_wire(
        &mut snapshot,
        9_302,
        Spell::PoisonCloud,
        center,
        MirDirection::Down,
        false,
        0,
    );
    assert_eq!(snapshot.spell_events.len(), 2);
    assert_eq!(snapshot.spell_events[0].direction, MirDirection::Up);
    assert!(snapshot.spell_events[0].param);
    assert_eq!(snapshot.spell_events[1].direction, MirDirection::Down);
    assert!(!snapshot.spell_events[1].param);

    let (origin_x, origin_y) = cell_origin(center, center);
    let ops = [
        OracleOp::add(SpriteSource::Magic2, 1_650, origin_x, origin_y, true),
        OracleOp::add(SpriteSource::Magic2, 1_650, origin_x, origin_y, true),
    ];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &snapshot.spell_events, center, 0);
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G5：direction/param 不影响帧号与落点",
    );

    let baseline = [
        spell_event(1, Spell::PoisonCloud, center, 0, true),
        spell_event(2, Spell::PoisonCloud, center, 0, true),
    ];
    let mut baseline_frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut baseline_frame, &mut scene, &baseline, center, 0);
    assert_eq!(
        frame.pixels, baseline_frame.pixels,
        "G5：诊断字段不改变像素"
    );
    assert_eq!(
        covered_count(&frame, background),
        9,
        "G5：同格叠加仍是 3×3 并集"
    );
    cleanup(&dir);
}
