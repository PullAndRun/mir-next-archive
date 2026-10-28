//! Round 76 A：149 `S.ObjectSpell` 来源的 `Spell::TrapHexagon` 十帧 `Magic` 库映射。
//!
//! 本文件只验收 [`docs/process/round76-plan.md`] 的有限切片（**不是**完整 Crystal
//! `Reincarnation` 迁移，也不得与 R72 毒云/ R73 雷电作为同一迁移声称等价）：
//!
//! * 仅 149 `ObjectSpell` 来源（`SpellEvent::from_object_spell == true`）的
//!   `TrapHexagon` 走 Crystal `SpellObject` 的 `Libraries.Magic` 帧 `1390..=1399`、
//!   100ms/帧、10 帧 elapsed 循环：`phase = (age / 100) % 10`、`index = 1390 + phase`；
//! * 经 `FxKind::Magic`（库 `magic.wzl`、`start_frame() = 0`）寻址；
//! * 落点是格锚点 `scene.world_to_screen(center, event.location)`（`from_object_spell`
//!   为真时取事件 location，**不**取施法者位置；`TargetPoint` 不取 caster）；
//! * 帧头 i16 `offset_x/offset_y` 由 `MapScene::draw_effect_blend(..., apply_offset = true)`
//!   只叠一次（先 offset 后裁剪）；混合方式 = `Frame::blit_add`。
//!   Crystal 的 `rate = 0.8`、`src * 0.8 * alpha` 调光、`Light3`/`DrawColour` **未实施**
//!   （PARTIAL），本文件不推定两者等价；
//! * 时钟写实：Crystal `SpellObject` 的严格 `NextMotion`、初始 100ms 取整、每 `Process`
//!   至多一步**未实施**（PARTIAL），本切片只验证 elapsed 帧循环；4000ms 是产品瞬态窗
//!   （R76 分支常量），不是 Crystal 对象寿命；
//! * **公共 `duration_ms(TrapHexagon) = 700` 不变**（既有产品行为，不是 4000）；新分支
//!   位于公共 700ms/`duration_ms` 过滤**之前**，边界 `age == 0` 与 `age >= 4000` 用
//!   `continue` 处理，不播既有通用路径；
//! * 非 149 来源（`S.Magic`(120) / `S.ObjectMagic`(123)）的 `TrapHexagon` 与其它技能
//!   保持旧通用路径（60ms/帧、clamp 0..9、48×32 居中 + Over 整帧）；其他 26 技能映射
//!   仍 GAP-5。
//!
//! 变异自检（注释引用，不执行）：M1 换 `FxKind::Swing` → 画 `magic2[1390..]`（G1/G2/G4
//! 的 `magic2` 同索引诱饵帧 + 整帧 oracle 变红）；M2 索引错 `0` → G7 旧路径金样与 G1 变红；
//! M3 换 `draw_effect`/不加 offset → G4 帧左上偏转；M4 `apply_offset=false` → G4 变红；
//! M5 去掉 `from_object_spell` 门 → G7 的 149 火墙/毒云被 R76 分支劫持（帧颜色变红）；
//! M6 `duration_ms(TrapHexagon)` 改 4000 → G3 首行断言变红；M7 锚点换 `snapshot` 施法者
//!   位置 → G4 居中/改 centre 用例变红。
//!
//! 独立 oracle：本文件内合成 `magic.wzl`/`magic.wzx`、`magic2.wzl`/`magic2.wzx`，
//! 从原始 RGB565 像素按 `Frame::blit`/`Frame::blit_add` 的文档公式手算整帧 RGBA，
//! **不调用**生产 `blit/blit_add/add_channel/blend_channel`。
//!
//! 运行（缺 `legacy` feature 时合成图库不可读，首条夹具断言以诊断文本失败而非 SKIP）：
//!
//! ```text
//! cargo test -p mir-client --features mir-render/legacy --test round76_trap_hexagon_mapping -- --test-threads=1
//! ```

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use mir_client::net::{SpellEvent, WorldSnapshot};
use mir_client::ui::spellfx;
use mir_client::ui::{self, UiState, UiText};
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
/// 合成帧边长：5×2（`1391`，非方形，证明绘制不按尺寸居中）。
const SIZE_5X2: (u16, u16) = (5, 2);
/// 合成帧边长：2×3（`1396`）。
const SIZE_2X3: (u16, u16) = (2, 3);
/// 合成帧边长：3×2（`1397`）。
const SIZE_3X2: (u16, u16) = (3, 2);

/// R76 困魔咒 149 分支基准帧号（`magic.wzl` 的 `1390 + phase`）。
///
/// 由本文件独立写死（不读生产 `spellfx::*` 常量），与 R71 `FireWall` 的 1630 /
/// R72 `PoisonCloud` 的 1650 段刻意不重叠。
const TRAP_HEXAGON_BASE_INDEX: u32 = 1_390;
/// 帧数（`phase = (age / 100) % 10`）。
const TRAP_HEXAGON_FRAME_COUNT: i64 = 10;
/// 每帧毫秒。
const TRAP_HEXAGON_FRAME_MS: i64 = 100;
/// 产品瞬态窗（4000ms）；**不是** `duration_ms(TrapHexagon)` 的 700ms。
const TRAP_HEXAGON_SPRITE_MS: i64 = 4_000;

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

/// 合成 `magic.wzl` 帧表（`FxKind::Magic` → 库 `magic`，起始帧 0；R76 只新增 1390..=1399）。
const MAIN_MAGIC: [MagicFrame; 28] = [
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
    MagicFrame::new(1_390, SIZE_3X3, (-3, 5), [0x7C3F; 4]),
    MagicFrame::new(1_391, SIZE_5X2, (4, -2), [0x7C1F; 4]),
    MagicFrame::new(1_392, SIZE_2X2, (-1, -1), [0x0010; 4]),
    MagicFrame::new(1_393, SIZE_2X2, (50, -30), [0xF810; 4]),
    MagicFrame::new(1_394, SIZE_2X2, (7, 6), [0x03E0; 4]),
    MagicFrame::new(1_395, SIZE_2X2, (-5, 4), [0x0018; 4]),
    MagicFrame::new(1_396, SIZE_2X3, (2, 3), [0xFD20; 4]),
    MagicFrame::new(1_397, SIZE_3X2, (-2, -3), [0x7820; 4]),
    MagicFrame::new(1_398, SIZE_2X2, (2, 0), [0x3C1F; 4]),
    MagicFrame::new(1_399, SIZE_2X2, (1, -1), [0x7C00; 4]),
];

/// 合成 `magic2.wzl` 帧表（`FxKind::Swing`/`Bolt` → 库 `magic2`）。
///
/// 1390..=1399 是「错用 `FxKind::Swing` 时才会画到」的同索引诱饵；10..=14 是 R73 雷电命中
/// 旧行为金样；1650..=1669 是 R72 毒云旧行为金样；0/20..=29/1670 是 R72 冻结的起始帧诱饵。
const MAIN_MAGIC2: [MagicFrame; 48] = [
    MagicFrame::new(0, SIZE_2X2, (0, 0), [0x2222; 4]),
    MagicFrame::new(1_410, SIZE_2X2, (0, 0), [0x7E0F; 4]),
    MagicFrame::new(10, SIZE_2X2, (0, 0), [0xE71C; 4]),
    MagicFrame::new(11, SIZE_2X2, (0, 0), [0x1CE7; 4]),
    MagicFrame::new(12, SIZE_2X2, (0, 0), [0x39E7; 4]),
    MagicFrame::new(13, SIZE_2X2, (0, 0), [0x6739; 4]),
    MagicFrame::new(14, SIZE_2X2, (0, 0), [0x6B4D; 4]),
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
    MagicFrame::new(1_390, SIZE_2X2, (0, 0), [0x2223; 4]),
    MagicFrame::new(1_391, SIZE_2X2, (0, 0), [0x2444; 4]),
    MagicFrame::new(1_392, SIZE_2X2, (0, 0), [0x2888; 4]),
    MagicFrame::new(1_393, SIZE_2X2, (0, 0), [0x2CCC; 4]),
    MagicFrame::new(1_394, SIZE_2X2, (0, 0), [0x3111; 4]),
    MagicFrame::new(1_395, SIZE_2X2, (0, 0), [0x3555; 4]),
    MagicFrame::new(1_396, SIZE_2X2, (0, 0), [0x3999; 4]),
    MagicFrame::new(1_397, SIZE_2X2, (0, 0), [0x3DDD; 4]),
    MagicFrame::new(1_398, SIZE_2X2, (0, 0), [0x4211; 4]),
    MagicFrame::new(1_399, SIZE_2X2, (0, 0), [0x4655; 4]),
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
/// 优先系统临时目录；被环境策略拒绝写时回退到 `target/round76/a/tmp/`（测试产物目录）。
fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let name = format!("mir-next-r76-a-{tag}-{}-{nanos}", std::process::id());
    let candidates = [
        std::env::temp_dir().join(&name),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/round76/a/tmp")
            .join(&name),
    ];
    for dir in candidates {
        if fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }
    panic!("无法创建进程唯一临时目录（系统临时目录与 target/round76/a/tmp 均不可写）");
}

/// 合成帧的 RGB565 像素值：3×3 平铺帧全用 `quad[0]`，2×2 帧按四角取 `quad`。
fn frame_value(spec: &MagicFrame, column: u16, row: u16) -> u16 {
    if spec.size != SIZE_2X2 {
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
        file_name: "r76-empty".to_owned(),
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

/// 默认夹具可读性前置断言（缺 feature 时以诊断文本失败，不是 SKIP）。
fn assert_default_fixture_readable(scene: &mut MapScene) {
    assert_eq!(
        scene.effect_size(FxKind::Magic, 0),
        Some((2, 2)),
        "合成 magic.wzl[0] 必须可读（请加 --features mir-render/legacy）"
    );
    assert_eq!(
        scene.effect_size(FxKind::Magic, 1_390),
        Some((3, 3)),
        "合成 magic.wzl[1390] 必须是 3×3（R76 phase 0）"
    );
    assert_eq!(
        scene.effect_size(FxKind::Magic, 1_391),
        Some((5, 2)),
        "合成 magic.wzl[1391] 必须是 5×2（R76 非方形帧）"
    );
    for index in [1_392u32, 1_393, 1_394, 1_395, 1_398, 1_399] {
        assert_eq!(
            scene.effect_size(FxKind::Magic, index),
            Some((2, 2)),
            "合成 magic.wzl[{index}] 必须是 2×2"
        );
    }
    assert_eq!(
        scene.effect_size(FxKind::Magic, 1_396),
        Some((2, 3)),
        "合成 magic.wzl[1396] 必须是 2×3"
    );
    assert_eq!(
        scene.effect_size(FxKind::Magic, 1_397),
        Some((3, 2)),
        "合成 magic.wzl[1397] 必须是 3×2"
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
    for index in 10u32..=14 {
        assert_eq!(
            scene.effect_size(FxKind::Swing, index),
            Some((2, 2)),
            "合成 magic2.wzl[{index}] 雷电命中旧帧必须可读（R73 金样）"
        );
    }
    assert_eq!(
        scene.effect_size(FxKind::Swing, 1_390),
        Some((2, 2)),
        "合成 magic2.wzl[1390] 必须是 2×2（错用 Swing 的同索引诱饵）"
    );
    for index in 1_391u32..=1_399 {
        assert_eq!(
            scene.effect_size(FxKind::Swing, index),
            Some((2, 2)),
            "合成 magic2.wzl[{index}] 必须是 2×2（错用 Swing 的同索引诱饵）"
        );
    }
    assert_eq!(
        scene.effect_size(FxKind::Bolt, 1_390),
        Some((2, 2)),
        "合成 magic2.wzl[1410]（Bolt start 20 的 1390）必须是 2×2"
    );
    assert_eq!(
        scene.effect_size(FxKind::Swing, 1_650),
        Some((3, 3)),
        "合成 magic2.wzl[1650] 必须是 3×3（R72 金样）"
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
///
/// 生产 `Frame::blit`/`blit_add` 对非 2×2 帧（3×3 / 5×2 / 2×3 / 3×2）整帧平铺 `quad[0]`
/// （R71 `1630` 的 3×3 单色帧即为此契约）；2×2 帧按四角取 `quad`。
fn frame_pixels(spec: &MagicFrame) -> Vec<(i32, i32, u16)> {
    if spec.size == SIZE_2X2 {
        spec.quad
            .iter()
            .enumerate()
            .map(|(slot, &value)| ((slot % 2) as i32, (slot / 2) as i32, value))
            .collect()
    } else {
        let width = i32::from(spec.size.0);
        let height = i32::from(spec.size.1);
        (0..width * height)
            .map(|slot| (slot % width, slot / width, spec.quad[0]))
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

/// 诱饵颜色：R76 分支 `1390..=1399` 十帧之外的所有合成帧像素。
///
/// 新分支若错画旧通用帧（`0..=9`）、`1620` 起手帧、`1630..=1635`（R71 火墙金样）、
/// `magic[1650]`、任何 `magic2` 像素（含错用 `FxKind::Swing` 时的同索引 `1390..=1399`
/// 与 R72/R73 旧帧），都会命中这些颜色。
fn decoy_colors() -> Vec<[u8; 4]> {
    let mut colors = Vec::new();
    for spec in MAIN_MAGIC.iter().filter(|spec| {
        !(TRAP_HEXAGON_BASE_INDEX..=TRAP_HEXAGON_BASE_INDEX + 9).contains(&spec.index)
    }) {
        for &raw in &spec.quad {
            colors.push(expand565(raw));
        }
    }
    for spec in MAIN_MAGIC2 {
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
// 独立 age → 帧号映射（oracle 用，不读生产实现）
// ---------------------------------------------------------------------------

/// 独立映射：`age < 0` 或 `age >= 4000` → `None`；否则 `1390 + (age / 100) % 10`。
fn expected_frame(age: i64) -> Option<u32> {
    if !(0..TRAP_HEXAGON_SPRITE_MS).contains(&age) {
        return None;
    }
    let phase = (age / TRAP_HEXAGON_FRAME_MS) % TRAP_HEXAGON_FRAME_COUNT;
    Some(TRAP_HEXAGON_BASE_INDEX + u32::try_from(phase).expect("phase"))
}

/// 造一个已推进到 `now_ms` 的独立 `UiState`（每次绘制都用全新状态避免跨调用串味）。
fn ui_state_with_time(now_ms: i64) -> UiState {
    let mut ui = ui_state();
    ui.set_time(now_ms);
    ui
}

/// 从整帧 oracle 结果取像素（测试独立，不读生产 `Frame`）。
fn expected_pixel(expected: &[u8], x: i32, y: i32) -> [u8; 4] {
    let width = usize::try_from(FRAME_WIDTH).expect("宽");
    let offset = (usize::try_from(y).expect("y") * width + usize::try_from(x).expect("x")) * 4;
    let mut value = [0u8; 4];
    value.copy_from_slice(&expected[offset..offset + 4]);
    value
}

// ---------------------------------------------------------------------------
// G1：149 真实 wire 链 → 两条独立对象事件 → t=0 画 magic.wzl[1390]
// ---------------------------------------------------------------------------

/// G1：149（`S.ObjectSpell`）来源的 `TrapHexagon` 经真实
/// `encode_frame → decode_server_packet → apply_packet_at` 后，在 `t=0` 必须各画一张
/// `magic.wzl[1390]`（`FxKind::Magic` 零起点 + 格锚点 + 当帧 offset），不得画
/// `magic[0]` 旧通用帧、`magic2[1390]`（`FxKind::Swing` 错库同索引诱饵）或
/// `magic2[1410]`（`FxKind::Bolt` start 20 同索引诱饵）。
#[test]
fn r76_g1_wire_mapping() {
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
            Spell::TrapHexagon,
            *cell,
            0,
        );
    }
    assert_eq!(snapshot.spell_events.len(), 2, "两条 149 必须写两条事件");
    for (slot, event) in snapshot.spell_events.iter().enumerate() {
        assert_eq!(event.object_id, 9_001 + u32::try_from(slot).expect("槽位"));
        assert_eq!(
            event.spell,
            Spell::TrapHexagon,
            "149 载荷必须解回 TrapHexagon"
        );
        assert_eq!(event.location, cells[slot], "149 逐格落点必须原样保存");
        assert!(
            event.from_object_spell,
            "149 来源必须标记 from_object_spell"
        );
        assert_eq!(event.direction, MirDirection::Down);
        assert!(!event.param);
        assert_eq!(event.started_ms, 0);
    }
    assert_eq!(
        Spell::TrapHexagon as u8,
        73,
        "R76 走 `S.ObjectSpell` 的真实技能值必须是 73（不是别的 149 号）"
    );

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, 0);
    // 4000ms 是 R76 精灵瞬态窗；`duration_ms(TrapHexagon)` 仍是既有 700ms。
    assert_eq!(
        spellfx::duration_ms(Spell::TrapHexagon),
        700,
        "G1：公共 duration_ms(TrapHexagon) 必须保持 700ms（不是 4000）"
    );

    let mut ops = Vec::with_capacity(cells.len());
    for cell in cells {
        let (x, y) = cell_origin(center, cell);
        ops.push(OracleOp::add(
            SpriteSource::Magic,
            TRAP_HEXAGON_BASE_INDEX,
            x,
            y,
            true,
        ));
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
        "G1：t=0 两条 149 各画 magic.wzl[1390]（3×3，格原点 + 当帧 offset (-3,5) 只叠一次 + Add）",
    );

    let spec = frame_spec(SpriteSource::Magic, TRAP_HEXAGON_BASE_INDEX);
    let legacy = frame_spec(SpriteSource::Magic, 0);
    let wrong_lib = frame_spec(SpriteSource::Magic2, TRAP_HEXAGON_BASE_INDEX);
    for cell in cells {
        let (x, y) = frame_top_left(center, cell, spec);
        for (column, row, value) in frame_pixels(spec) {
            assert_eq!(
                pixel_at(&frame, x + column, y + row),
                expand565(value),
                "G1：格 {cell:?} 的 1390 像素 ({},{}) 颜色不符",
                x + column,
                y + row
            );
        }
        // 旧通用闪光（Magic 0 居中）不得出现。
        let (legacy_x, legacy_y) = legacy_top_left(center, cell, legacy);
        for (column, row, _value) in frame_pixels(legacy) {
            assert_eq!(
                pixel_at(&frame, legacy_x + column, legacy_y + row),
                background,
                "G1：旧 60ms 居中位置不得出现 magic.wzl[0]（未迁移的通用路径）"
            );
        }
        // `FxKind::Swing` 错库的同索引诱饵（magic2[1390]，2×2 offset 0）不得出现在格原点。
        let (origin_x, origin_y) = cell_origin(center, cell);
        for (column, row, _value) in frame_pixels(wrong_lib) {
            assert_eq!(
                pixel_at(&frame, origin_x + column, origin_y + row),
                background,
                "G1：格原点不得出现 magic2.wzl[1390]（`FxKind::Swing` 错库同索引诱饵）"
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

    // 第二层：`param = true` 不改变 149 困魔咒分支（无 param 门，落点仍取事件 location）。
    let param_cell = Point::new(center.x + 2, center.y);
    let mut param_snapshot = WorldSnapshot::new_session();
    push_object_spell_full_via_wire(
        &mut param_snapshot,
        9_100,
        Spell::TrapHexagon,
        param_cell,
        MirDirection::Left,
        true,
        0,
    );
    assert_eq!(param_snapshot.spell_events.len(), 1);
    assert!(
        param_snapshot.spell_events[0].param,
        "G1：param=true 必须保留在事件上"
    );
    let (param_x, param_y) = cell_origin(center, param_cell);
    let param_ops = [OracleOp::add(
        SpriteSource::Magic,
        TRAP_HEXAGON_BASE_INDEX,
        param_x,
        param_y,
        true,
    )];
    let mut param_frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut param_frame,
        &mut scene,
        &param_snapshot.spell_events,
        center,
        0,
    );
    assert_frame_matches(
        &param_frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &param_ops),
        "G1：param=true 的 149 困魔咒仍画 magic[1390]（无 param 门）",
    );
}

// ---------------------------------------------------------------------------
// G2：帧边界（100ms/帧、10 帧循环）与 4000ms 窗口
// ---------------------------------------------------------------------------

/// G2：`phase = (age / 100) % 10` 的每个边界都必须映射到 `1390 + phase`，
/// 覆盖 10 帧 × 首末边界、1000ms 回卷与 3999ms 尾窗，每时点独立清帧。
#[test]
fn r76_g2_frames_and_wrap() {
    let (mut scene, dir) = scene_with_default("g2");
    assert_default_fixture_readable(&mut scene);

    assert_eq!(TRAP_HEXAGON_FRAME_COUNT, 10, "phase 周期是 10 帧");
    assert_eq!(TRAP_HEXAGON_FRAME_MS, 100, "每帧 100ms");
    assert_eq!(TRAP_HEXAGON_SPRITE_MS, 4_000, "产品瞬态窗 4000ms");

    let center = center_point();
    let background = [0, 0, 0, 255];
    let started = 1_000i64;
    let cell = center;
    let mut cases: Vec<(i64, i64)> = Vec::new();
    for phase in 0..TRAP_HEXAGON_FRAME_COUNT {
        cases.push((TRAP_HEXAGON_FRAME_MS * phase, phase));
        cases.push((
            TRAP_HEXAGON_FRAME_MS * phase + (TRAP_HEXAGON_FRAME_MS - 1),
            phase,
        ));
    }
    cases.push((1_000, 0));
    cases.push((1_099, 0));
    cases.push((1_100, 1));
    cases.push((3_999, 9));

    let mut frame_colors = Vec::new();
    for (age, phase) in cases {
        let index = TRAP_HEXAGON_BASE_INDEX + u32::try_from(phase).expect("phase");
        assert_eq!(
            expected_frame(age),
            Some(index),
            "G2：独立映射 age={age} 必须 → 帧 {index}"
        );
        let events = [spell_event(
            1,
            Spell::TrapHexagon,
            cell,
            started - age,
            true,
        )];
        let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(&mut frame, &mut scene, &events, center, started);
        let spec = frame_spec(SpriteSource::Magic, index);
        let (x, y) = frame_top_left(center, cell, spec);
        let (origin_x, origin_y) = cell_origin(center, cell);
        let ops = [OracleOp::add(
            SpriteSource::Magic,
            index,
            origin_x,
            origin_y,
            true,
        )];
        assert_frame_matches(
            &frame,
            &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
            &format!("G2：age={age} 必须整帧画 magic[{index}]"),
        );
        for (column, row, value) in frame_pixels(spec) {
            assert_eq!(
                pixel_at(&frame, x + column, y + row),
                expand565(value),
                "G2：age={age} 帧 {index} 像素 ({},{})",
                x + column,
                y + row
            );
        }
        let raster = u32::from(spec.size.0) * u32::from(spec.size.1);
        assert_eq!(
            covered_count(&frame, background),
            usize::try_from(raster).expect("像素数"),
            "G2：age={age} 帧 {index} 的覆盖像素数"
        );
        assert_no_decoy_colors(&frame, background, &format!("G2：age={age}"));
        frame_colors.push(expand565(spec.quad[0]));
    }
    let unique: BTreeSet<[u8; 4]> = frame_colors.iter().copied().collect();
    assert_eq!(unique.len(), 10, "G2：十帧必须异色（每帧独立可辨）");
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G3：公共 700ms / duration_ms 过滤与新分支 4000ms 窗口的边界
// ---------------------------------------------------------------------------

/// G3：R76 分支必须位于公共 `duration_ms`（TrapHexagon=700）过滤之前：
/// `age==0` 画 1390、`age=699` 画 1396、`age>=700` 仍按新窗口画 1397..=1399、
/// `age>=4000` 才 `continue`；负 age / i64 极值不 panic 且零落屏；120/123 非 149
/// 来源的 `TrapHexagon` 保持旧 60ms 通用路径与 700ms 截止。
#[test]
fn r76_g3_window_and_source() {
    let (mut scene, dir) = scene_with_default("g3");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let background = [0, 0, 0, 255];
    let started = 1_000i64;
    assert_eq!(
        spellfx::duration_ms(Spell::TrapHexagon),
        700,
        "G3：TrapHexagon 既有公共窗是 700ms（不是 4000；4000 只是 R76 精灵窗）"
    );
    assert_eq!(spellfx::duration_ms(Spell::FireWall), 4_000);

    let boundaries = [
        (0i64, Some(0u32)),
        (1, Some(0)),
        (99, Some(0)),
        (100, Some(1)),
        (699, Some(6)),
        (700, Some(7)),
        (999, Some(9)),
        (1_000, Some(0)),
        (3_999, Some(9)),
        (4_000, None),
        (4_001, None),
    ];
    for (age, expected_phase) in boundaries {
        let events = [spell_event(
            1,
            Spell::TrapHexagon,
            center,
            started - age,
            true,
        )];
        let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(&mut frame, &mut scene, &events, center, started);
        match expected_phase {
            Some(phase) => {
                let index = TRAP_HEXAGON_BASE_INDEX + phase;
                let spec = frame_spec(SpriteSource::Magic, index);
                let (x, y) = frame_top_left(center, center, spec);
                let (origin_x, origin_y) = cell_origin(center, center);
                let ops = [OracleOp::add(
                    SpriteSource::Magic,
                    index,
                    origin_x,
                    origin_y,
                    true,
                )];
                assert_frame_matches(
                    &frame,
                    &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
                    &format!("G3：age={age} 必须画 magic[{index}]（新分支在公共 700ms 过滤之前）"),
                );
                assert_eq!(
                    pixel_at(&frame, x, y),
                    expand565(spec.quad[0]),
                    "G3：age={age} 帧 {index} 首像素"
                );
                assert_no_decoy_colors(&frame, background, &format!("G3：age={age}"));
            }
            None => {
                assert_only_background(
                    &frame,
                    background,
                    &format!("G3：age={age} 必须零落屏（不得回退旧通用路径）"),
                );
            }
        }
    }

    // 负 age / i64 极值：saturating 差值不得 panic，必须零落屏。
    for edge_started in [started + 1, i64::MAX, i64::MIN] {
        let events = [spell_event(
            1,
            Spell::TrapHexagon,
            center,
            edge_started,
            true,
        )];
        let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(&mut frame, &mut scene, &events, center, started);
        assert_only_background(
            &frame,
            background,
            &format!("G3：started_ms={edge_started} 必须零落屏（saturating age 边界不 panic）"),
        );
    }

    // 非 149 来源 120（`S.Magic`）：仍走旧通用 60ms/帧 clamp 0..=9 + 48×32 居中 + Over。
    let mut snapshot = WorldSnapshot::new_session();
    push_magic_via_wire(&mut snapshot, Spell::TrapHexagon, center, 0);
    assert_eq!(snapshot.spell_events.len(), 1);
    assert!(
        !snapshot.spell_events[0].from_object_spell,
        "G3：120 来源不得带 from_object_spell"
    );
    let spec9 = frame_spec(SpriteSource::Magic, 9);
    let (legacy_x, legacy_y) = legacy_top_left(center, center, spec9);
    let ops = [OracleOp::over(
        SpriteSource::Magic,
        9,
        legacy_x,
        legacy_y,
        false,
    )];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &snapshot.spell_events, center, 699);
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G3：120 非 149 旧 TrapHexagon(699ms) 仍画旧通用 magic[9] 居中 + Over",
    );
    assert_eq!(covered_count(&frame, background), 4, "G3：旧通用 2×2");
    let mut frame_700 = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut frame_700,
        &mut scene,
        &snapshot.spell_events,
        center,
        700,
    );
    assert_only_background(
        &frame_700,
        background,
        "G3：旧 TrapHexagon 公共 700ms 截止（既有旧行为，不得被 4000 新窗改写）",
    );

    // 非 149 来源 123（`S.ObjectMagic`）：同上。
    let object_cell = Point::new(center.x + 2, center.y);
    let mut object_snapshot = WorldSnapshot::new_session();
    push_object_magic_via_wire(
        &mut object_snapshot,
        9_001,
        Spell::TrapHexagon,
        object_cell,
        0,
    );
    assert_eq!(object_snapshot.spell_events.len(), 1);
    assert!(!object_snapshot.spell_events[0].from_object_spell);
    let (legacy_x, legacy_y) = legacy_top_left(center, object_cell, spec9);
    let ops_123 = [OracleOp::over(
        SpriteSource::Magic,
        9,
        legacy_x,
        legacy_y,
        false,
    )];
    let mut frame_123 = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut frame_123,
        &mut scene,
        &object_snapshot.spell_events,
        center,
        699,
    );
    assert_frame_matches(
        &frame_123,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops_123),
        "G3：123 非 149 旧 TrapHexagon(699ms) 仍画旧通用 magic[9] 居中 + Over",
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G4：格锚点 + 当帧 offset 一次（不按尺寸居中）、相机中心、屏外移入与裁剪
// ---------------------------------------------------------------------------

/// G4：R76 锚点 = `world_to_screen(center, event.location)`（格锚点，不取 caster），
/// 帧头 i16 offset 由 `draw_effect_blend(..., apply_offset=true)` 恰叠一次、先 offset
/// 后裁剪；覆盖 3×3 / 5×2 非方形帧、正/负 offset、屏外经 offset 移入、左缘裁剪、
/// 相机中心改变与两次绘制一致性。
#[test]
fn r76_g4_anchor_offset_crop() {
    let (mut scene, dir) = scene_with_default("g4");
    assert_default_fixture_readable(&mut scene);

    let background = [0, 0, 0, 255];
    let started = 1_000i64;
    let center = center_point();
    let cells = [center, Point::new(center.x + 1, center.y)];

    // 1) age=0 帧 1390（3×3，offset (-3,5)）：绝对左上 = 格原点 + offset，绝不按尺寸居中。
    let events: Vec<SpellEvent> = cells
        .iter()
        .enumerate()
        .map(|(slot, cell)| {
            spell_event(
                7_000 + u64::try_from(slot).expect("slot"),
                Spell::TrapHexagon,
                *cell,
                started,
                true,
            )
        })
        .collect();
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, center, started);
    let spec = frame_spec(SpriteSource::Magic, TRAP_HEXAGON_BASE_INDEX);
    let ops: Vec<OracleOp> = cells
        .iter()
        .map(|cell| {
            let (x, y) = cell_origin(center, *cell);
            OracleOp::add(SpriteSource::Magic, TRAP_HEXAGON_BASE_INDEX, x, y, true)
        })
        .collect();
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G4：age=0 两格 magic[1390] 3×3 = 格原点 + 当帧 offset (-3,5) 恰一次",
    );
    for cell in cells {
        let (x, y) = frame_top_left(center, cell, spec);
        for (column, row, value) in frame_pixels(spec) {
            assert_eq!(
                pixel_at(&frame, x + column, y + row),
                expand565(value),
                "G4：格 {cell:?} 的 1390 像素"
            );
        }
    }
    assert_eq!(covered_count(&frame, background), 2 * 9);

    // 2) 非方形 5×2 与 offset (50,-30)：尺寸不参与居中，仍格原点 + offset。
    let five_by_two = frame_spec(SpriteSource::Magic, 1_391);
    let off_50 = frame_spec(SpriteSource::Magic, 1_393);
    let events2 = [
        spell_event(1, Spell::TrapHexagon, cells[0], started - 100, true),
        spell_event(2, Spell::TrapHexagon, cells[1], started - 300, true),
    ];
    let mut frame2 = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame2, &mut scene, &events2, center, started);
    let mut ops2 = Vec::new();
    for (cell, index) in [(cells[0], 1_391u32), (cells[1], 1_393u32)] {
        let (x, y) = cell_origin(center, cell);
        ops2.push(OracleOp::add(SpriteSource::Magic, index, x, y, true));
    }
    assert_frame_matches(
        &frame2,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops2),
        "G4：非方形 5×2 与 offset (50,-30) 均按格锚点 + 当帧 offset（不按尺寸居中）",
    );
    assert_eq!(covered_count(&frame2, background), 10 + 4);
    for (cell, spec) in [(cells[0], five_by_two), (cells[1], off_50)] {
        let (x, y) = frame_top_left(center, cell, spec);
        for (column, row, value) in frame_pixels(spec) {
            assert_eq!(
                pixel_at(&frame2, x + column, y + row),
                expand565(value),
                "G4：尺寸帧像素"
            );
        }
    }

    // 3) 相机中心改变：同一格事件的绝对位置随 world_to_screen 变化，仍锚在格原点 + offset。
    let center_moved = Point::new(12, 9);
    let moved_cell = Point::new(11, 10);
    let events3 = [spell_event(
        3,
        Spell::TrapHexagon,
        moved_cell,
        started,
        true,
    )];
    let mut frame3 = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame3, &mut scene, &events3, center_moved, started);
    let (moved_origin_x, moved_origin_y) = cell_origin(center_moved, moved_cell);
    let ops3 = [OracleOp::add(
        SpriteSource::Magic,
        TRAP_HEXAGON_BASE_INDEX,
        moved_origin_x,
        moved_origin_y,
        true,
    )];
    assert_frame_matches(
        &frame3,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops3),
        "G4：相机中心 (12,9) 时锚点仍 = 格原点 + offset（不读快照施法者位置）",
    );
    let (x3, y3) = frame_top_left(center_moved, moved_cell, spec);
    assert_eq!(pixel_at(&frame3, x3, y3), expand565(spec.quad[0]));

    // 4) 屏外经格偏移移入：格 (1,11) 原点 (-48,288)，1393 offset (50,-30) → (2,258) 全可见。
    let outside_cell = Point::new(1, 11);
    let (outside_origin_x, outside_origin_y) = cell_origin(center, outside_cell);
    assert_eq!(
        (outside_origin_x, outside_origin_y),
        (-48, 288),
        "G4：移入用例的格原点"
    );
    let events4 = [spell_event(
        4,
        Spell::TrapHexagon,
        outside_cell,
        started - 300,
        true,
    )];
    let mut frame4 = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame4, &mut scene, &events4, center, started);
    let (moved_at_x, moved_at_y) = frame_top_left(center, outside_cell, off_50);
    assert_eq!(
        (moved_at_x, moved_at_y),
        (2, 258),
        "G4：屏外格经当帧 offset 移入视口"
    );
    let ops4 = [OracleOp::add(
        SpriteSource::Magic,
        1_393,
        outside_origin_x,
        outside_origin_y,
        true,
    )];
    assert_frame_matches(
        &frame4,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops4),
        "G4：屏外格 + offset 移入后整帧金样",
    );
    assert_eq!(
        covered_count(&frame4, background),
        4,
        "G4：移入后 4 像素可见"
    );

    // 5) 部分裁剪：格 (2,10) 原点 (0,256)，1392（age=200，2×2）offset (-1,-1)
    //    → 落点 (-1,255)：格偏移先叠加、再按视口裁剪，只剩 2 像素可见。
    let crop_cell = Point::new(2, 10);
    let (crop_origin_x, crop_origin_y) = cell_origin(center, crop_cell);
    assert_eq!(
        (crop_origin_x, crop_origin_y),
        (0, 256),
        "G4：裁剪用例的格原点"
    );
    let events5 = [spell_event(
        5,
        Spell::TrapHexagon,
        crop_cell,
        started - 200,
        true,
    )];
    let mut frame5 = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame5, &mut scene, &events5, center, started);
    let crop_spec = frame_spec(SpriteSource::Magic, 1_392);
    assert_eq!(
        frame_top_left(center, crop_cell, crop_spec),
        (-1, 255),
        "G4：裁剪前落点 = 格原点 + 当帧 offset"
    );
    let ops5 = [OracleOp::add(
        SpriteSource::Magic,
        1_392,
        crop_origin_x,
        crop_origin_y,
        true,
    )];
    assert_frame_matches(
        &frame5,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops5),
        "G4：offset 先于裁剪：2×2 帧左上角出界",
    );
    assert_eq!(
        covered_count(&frame5, background),
        2,
        "G4：2×2 经 offset 后裁掉 1 列 1 行 = 2 像素"
    );
    for (x, y) in [(0, 255), (0, 256)] {
        assert_eq!(
            pixel_at(&frame5, x, y),
            expand565(crop_spec.quad[0]),
            "G4：裁剪后可见像素 ({x},{y})"
        );
    }

    // 6) 同输入两次独立绘制必须逐像素一致（无隐藏状态）。
    let mut frame6 = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame6, &mut scene, &events5, center, started);
    assert_eq!(
        frame5.pixels, frame6.pixels,
        "G4：同一事件两次独立绘制必须一致"
    );

    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G5：Add 混合、透明黑帧、缺帧跳过、同格双 149 叠加
// ---------------------------------------------------------------------------

/// G5：R76 分支必须 `Frame::blit_add`（非 Over）；全黑帧（alpha=0）零落屏且不吞后续
/// 149 旧分支；缺失目标帧必须跳过（不回退 `magic[0]` 且不影响下一 phase）；同格两条
/// 149 必须各自 Add（clamp 到 255）。
#[test]
fn r76_g5_add_and_missing() {
    let (mut scene, dir) = scene_with_default("g5");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    // 非黑背景才能区分 Add 与 Over。
    let background = [10, 20, 30, 255];
    let started = 1_000i64;

    // 1) Add 而非 Over：1391（绿）加到 (10,20,30)。
    let cell = center;
    let events = [spell_event(
        1,
        Spell::TrapHexagon,
        cell,
        started - 100,
        true,
    )];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, center, started);
    let spec = frame_spec(SpriteSource::Magic, 1_391);
    let (x, y) = frame_top_left(center, cell, spec);
    let (origin_x, origin_y) = cell_origin(center, cell);
    let ops = [OracleOp::add(
        SpriteSource::Magic,
        1_391,
        origin_x,
        origin_y,
        true,
    )];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G5：R76 分支必须用 Add（不是 Over）",
    );
    let color = expand565(spec.quad[0]);
    let added = [
        oracle_add_channel(background[0], color[0], 255),
        oracle_add_channel(background[1], color[1], 255),
        oracle_add_channel(background[2], color[2], 255),
        255,
    ];
    assert_eq!(
        pixel_at(&frame, x, y),
        added,
        "G5：1391 绿必须加到非黑背景上（Over 会替换成 (0,252,0)，Add 得 (10,255,30)）"
    );
    assert_eq!(
        pixel_at(&frame, x + 4, y + 1),
        added,
        "G5：5×2 5×2 最右像素也必须 Add"
    );

    // 2) 全黑透明帧：1392 的 quad 置黑（alpha=0）→ 零像素；同帧 149 FireWall 仍按旧分支照画。
    let black_dir = unique_dir("g5-black");
    let mut black_table = MAIN_MAGIC.to_vec();
    for spec in &mut black_table {
        if spec.index == 1_392 {
            spec.quad = [0; 4];
        }
    }
    write_library(&black_dir, "magic", &black_table);
    write_library(&black_dir, "magic2", &MAIN_MAGIC2);
    let mut black_scene = scene_in_dir(&black_dir);
    let fire_cell = Point::new(center.x + 1, center.y);
    let black_events = [
        spell_event(2, Spell::TrapHexagon, center, started - 200, true),
        spell_event(3, Spell::FireWall, fire_cell, started - 120, true),
    ];
    let mut black_frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut black_frame,
        &mut black_scene,
        &black_events,
        center,
        started,
    );
    let (fire_origin_x, fire_origin_y) = cell_origin(center, fire_cell);
    let black_ops = [OracleOp::add(
        SpriteSource::Magic,
        1_631,
        fire_origin_x,
        fire_origin_y,
        true,
    )];
    assert_frame_matches(
        &black_frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &black_ops),
        "G5：全黑透明帧 1392 不落屏，后续 149 FireWall 仍按旧分支照画",
    );
    assert_eq!(
        covered_count(&black_frame, background),
        4,
        "G5：只剩 FireWall 4 像素"
    );
    cleanup(&black_dir);

    // 3) 缺帧 1391：跳过（不回退 magic[0]）；下一 phase 1392 仍可读。
    let missing_dir = unique_dir("g5-missing");
    let missing_table: Vec<MagicFrame> = MAIN_MAGIC
        .iter()
        .copied()
        .filter(|spec| spec.index != 1_391)
        .collect();
    write_library(&missing_dir, "magic", &missing_table);
    write_library(&missing_dir, "magic2", &MAIN_MAGIC2);
    let mut missing_scene = scene_in_dir(&missing_dir);
    assert_eq!(
        missing_scene.effect_size(FxKind::Magic, 1_391),
        None,
        "G5：缺帧场景必须缺 magic[1391]"
    );
    let missing_events = [spell_event(
        4,
        Spell::TrapHexagon,
        center,
        started - 150,
        true,
    )];
    let mut missing_frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut missing_frame,
        &mut missing_scene,
        &missing_events,
        center,
        started,
    );
    assert_only_background(
        &missing_frame,
        background,
        "G5：缺帧 1391 时零落屏（不得回退 magic[0] 旧通用帧）",
    );
    assert_no_decoy_colors(&missing_frame, background, "G5：缺帧");
    let next_events = [spell_event(
        5,
        Spell::TrapHexagon,
        center,
        started - 250,
        true,
    )];
    let mut next_frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut next_frame,
        &mut missing_scene,
        &next_events,
        center,
        started,
    );
    let (next_x, next_y) = cell_origin(center, center);
    let next_ops = [OracleOp::add(
        SpriteSource::Magic,
        1_392,
        next_x,
        next_y,
        true,
    )];
    assert_frame_matches(
        &next_frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &next_ops),
        "G5：缺 1391 不影响下一 phase 1392",
    );

    // 4) 同格两条 149：各自 Add 一次（clamp 到 255）。
    let stack_events = [
        spell_event(6, Spell::TrapHexagon, center, started, true),
        spell_event(7, Spell::TrapHexagon, center, started, true),
    ];
    let mut stack = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut stack,
        &mut missing_scene,
        &stack_events,
        center,
        started,
    );
    let (stack_x, stack_y) = cell_origin(center, center);
    let stack_ops = [
        OracleOp::add(
            SpriteSource::Magic,
            TRAP_HEXAGON_BASE_INDEX,
            stack_x,
            stack_y,
            true,
        ),
        OracleOp::add(
            SpriteSource::Magic,
            TRAP_HEXAGON_BASE_INDEX,
            stack_x,
            stack_y,
            true,
        ),
    ];
    assert_frame_matches(
        &stack,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &stack_ops),
        "G5：同格两条 149 Add 两次（clamp 到 255）",
    );
    let stack_spec = frame_spec(SpriteSource::Magic, 1_390);
    let stack_color = expand565(stack_spec.quad[0]);
    let stack_expected = [
        oracle_add_channel(
            oracle_add_channel(background[0], stack_color[0], 255),
            stack_color[0],
            255,
        ),
        oracle_add_channel(
            oracle_add_channel(background[1], stack_color[1], 255),
            stack_color[1],
            255,
        ),
        oracle_add_channel(
            oracle_add_channel(background[2], stack_color[2], 255),
            stack_color[2],
            255,
        ),
        255,
    ];
    let (stack_left, stack_top) = frame_top_left(center, center, stack_spec);
    assert_eq!(
        pixel_at(&stack, stack_left, stack_top),
        stack_expected,
        "G5：同色两次 Add 在非黑背景上逐通道 clamp 到 255（Over 会得源色）"
    );
    assert_eq!(covered_count(&stack, background), 9);
    cleanup(&missing_dir);

    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G6：公开 ui::draw 叠加 R76 帧（含缺帧场景）
// ---------------------------------------------------------------------------

/// G6：同一份真实 149 wire 快照，用公开 `ui::draw` 画到 `Frame`：
/// `scene_full`（含 `magic[1390]`）与 `scene_missing`（缺 `1390..=1399`）两套独立图库；
/// 期望 = 「同场景同快照去掉事件」的基线 + 帧 Add 手算（独立 oracle）；`ui::draw` 与
/// `spellfx::draw_sprites` 的 ROI 逐像素一致（接线证明）；缺帧场景不得回退旧通用帧。
#[test]
fn r76_g6_ui_entry() {
    let center = center_point();
    let background = [0, 0, 0, 255];
    let target = center;
    let started = 1_000i64;
    let spec = frame_spec(SpriteSource::Magic, TRAP_HEXAGON_BASE_INDEX);
    let (roi_x, roi_y) = frame_top_left(center, target, spec);
    let roi = (roi_x, roi_y, u32::from(spec.size.0), u32::from(spec.size.1));
    assert_eq!(
        roi,
        (381, 261, 3, 3),
        "G6：magic[1390] ROI = 格原点 + offset (-3,5)"
    );

    let (mut scene_full, dir_full) = scene_with_default("g6-full");
    assert_default_fixture_readable(&mut scene_full);
    let missing_dir = unique_dir("g6-missing");
    let missing_table: Vec<MagicFrame> = MAIN_MAGIC
        .iter()
        .copied()
        .filter(|spec| {
            !(TRAP_HEXAGON_BASE_INDEX..=TRAP_HEXAGON_BASE_INDEX + 9).contains(&spec.index)
        })
        .collect();
    write_library(&missing_dir, "magic", &missing_table);
    write_library(&missing_dir, "magic2", &MAIN_MAGIC2);
    let mut scene_missing = scene_in_dir(&missing_dir);
    assert_eq!(
        scene_missing.effect_size(FxKind::Magic, TRAP_HEXAGON_BASE_INDEX),
        None,
        "G6：missing 场景必须缺 magic[1390]"
    );

    for (tag, scene) in [("full", &mut scene_full), ("missing", &mut scene_missing)] {
        let plain = WorldSnapshot::new_session();
        let mut with_event = WorldSnapshot::new_session();
        push_object_spell_via_wire(
            &mut with_event,
            9_001,
            Spell::TrapHexagon,
            target,
            u64::try_from(started).expect("started"),
        );
        assert_eq!(with_event.spell_events.len(), 1);
        assert_eq!(with_event.spell_events[0].started_ms, started);

        let text = UiText::fallback();
        let mut base = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        ui::draw(
            &mut ui_state_with_time(started),
            &mut base,
            scene,
            &plain,
            &text,
            None,
            center,
        );
        let mut drawn = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        ui::draw(
            &mut ui_state_with_time(started),
            &mut drawn,
            scene,
            &with_event,
            &text,
            None,
            center,
        );
        let mut direct = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(
            &mut direct,
            scene,
            &with_event.spell_events,
            center,
            started,
        );

        // ROI 基线必须是纯背景（文本/色块/HUD 不得侵入命中帧足迹）。
        for row in 0..roi.3 {
            for column in 0..roi.2 {
                let x = roi.0 + i32::try_from(column).expect("列");
                let y = roi.1 + i32::try_from(row).expect("行");
                assert_eq!(
                    pixel_at(&base, x, y),
                    background,
                    "G6（{tag}）：ROI ({x},{y}) 基线必须是背景（HUD/文字/色块不得侵入）"
                );
            }
        }

        if tag == "missing" {
            assert_only_background(
                &direct,
                background,
                "G6（missing）：缺 magic[1390] 时精灵层不得画任何回退帧",
            );
            for row in 0..roi.3 {
                for column in 0..roi.2 {
                    let x = roi.0 + i32::try_from(column).expect("列");
                    let y = roi.1 + i32::try_from(row).expect("行");
                    assert_eq!(
                        pixel_at(&drawn, x, y),
                        background,
                        "G6（missing）：缺帧时命中 ROI 必须保持基线"
                    );
                }
            }
            continue;
        }

        let (origin_x, origin_y) = cell_origin(center, target);
        let ops = [OracleOp::add(
            SpriteSource::Magic,
            TRAP_HEXAGON_BASE_INDEX,
            origin_x,
            origin_y,
            true,
        )];
        let expected = oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops);
        assert_frame_matches(
            &direct,
            &expected,
            "G6：draw_sprites 整帧 = magic[1390]（格锚点 + offset + Add）",
        );
        // `ui::draw` 的命中 ROI = 基线 + 1390 Add（独立 oracle）。
        for row in 0..roi.3 {
            for column in 0..roi.2 {
                let x = roi.0 + i32::try_from(column).expect("列");
                let y = roi.1 + i32::try_from(row).expect("行");
                assert_eq!(
                    pixel_at(&drawn, x, y),
                    expected_pixel(&expected, x, y),
                    "G6（{tag}）：ui::draw 命中帧 ROI 像素 ({x},{y})"
                );
                assert_eq!(
                    pixel_at(&drawn, x, y),
                    pixel_at(&direct, x, y),
                    "G6（{tag}）：ui::draw 与 draw_sprites 的命中 ROI 必须一致（接线）"
                );
            }
        }
    }
    cleanup(&dir_full);
    cleanup(&missing_dir);
}

// ---------------------------------------------------------------------------
// G7：非 149 旧路径与 149 旧分支（R71/R72/R73 金样）不变
// ---------------------------------------------------------------------------

/// G7：`Blizzard` / `Reincarnation`（通用 `magic[2]` 居中 Over）、149 `FireWall`
/// （`magic[1631]`）、149 `PoisonCloud`（`magic2[1651]`）、`ThunderBolt` 命中旧窗口
/// （`magic2[11]`）整帧金样不变；R76 分支不得劫持任何旧路径；事件字段不被绘制改写；
/// 两次绘制一致。
#[test]
fn r76_g7_legacy_and_query() {
    let (mut scene, dir) = scene_with_default("g7");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let background = [0, 0, 0, 255];
    let now = 10_000i64;
    let cell_blizzard = Point::new(center.x, center.y);
    let cell_reincarnation = Point::new(center.x + 1, center.y);
    let cell_fire_wall = Point::new(center.x + 2, center.y);
    let cell_poison = Point::new(center.x, center.y + 1);
    let cell_thunder = Point::new(center.x + 1, center.y + 1);

    let mut thunder = spell_event(5, Spell::ThunderBolt, cell_thunder, now - 80, false);
    thunder.param = true;
    let events = [
        spell_event(1, Spell::Blizzard, cell_blizzard, now - 120, true),
        spell_event(2, Spell::Reincarnation, cell_reincarnation, now - 120, true),
        spell_event(3, Spell::FireWall, cell_fire_wall, now - 120, true),
        spell_event(4, Spell::PoisonCloud, cell_poison, now - 120, true),
        thunder,
    ];
    let before: Vec<(u64, Spell, Point, i64)> = events
        .iter()
        .map(|event| (event.id, event.spell, event.location, event.started_ms))
        .collect();

    let generic = frame_spec(SpriteSource::Magic, 2);
    let mut ops = Vec::new();
    for cell in [cell_blizzard, cell_reincarnation] {
        let (x, y) = legacy_top_left(center, cell, generic);
        ops.push(OracleOp::over(SpriteSource::Magic, 2, x, y, false));
    }
    let (fire_x, fire_y) = cell_origin(center, cell_fire_wall);
    ops.push(OracleOp::add(
        SpriteSource::Magic,
        1_631,
        fire_x,
        fire_y,
        true,
    ));
    let (poison_x, poison_y) = cell_origin(center, cell_poison);
    ops.push(OracleOp::add(
        SpriteSource::Magic2,
        1_651,
        poison_x,
        poison_y,
        true,
    ));
    let (thunder_x, thunder_y) = cell_origin(center, cell_thunder);
    ops.push(OracleOp::add(
        SpriteSource::Magic2,
        11,
        thunder_x,
        thunder_y,
        true,
    ));

    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, center, now);
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G7：非 149 通用 magic[2] 居中 Over + 149 FireWall/PoisonCloud 旧分支 + ThunderBolt 命中旧窗口 金样不变",
    );
    assert_eq!(
        covered_count(&frame, background),
        4 + 4 + 4 + 4 + 4,
        "G7：旧路径覆盖像素数"
    );

    // 旧路径帧不得出现任何 R76 分支帧颜色（分支不劫持旧路径）。
    let branch_colors: Vec<[u8; 4]> = MAIN_MAGIC
        .iter()
        .filter(|spec| {
            (TRAP_HEXAGON_BASE_INDEX..=TRAP_HEXAGON_BASE_INDEX + 9).contains(&spec.index)
        })
        .flat_map(|spec| spec.quad.iter().map(|&raw| expand565(raw)))
        .collect();
    for (index, pixel) in frame.pixels.chunks_exact(4).enumerate() {
        let mut value = [0u8; 4];
        value.copy_from_slice(pixel);
        if value == background {
            continue;
        }
        assert!(
            !branch_colors.contains(&value),
            "G7：旧路径帧出现 R76 分支帧颜色 {value:?}（像素索引 {index}）：分支劫持了旧路径"
        );
    }

    // 绘制不得改写事件输入。
    for (event, (id, spell, location, started_ms)) in events.iter().zip(before.iter()) {
        assert_eq!(event.id, *id);
        assert_eq!(event.spell, *spell);
        assert_eq!(event.location, *location);
        assert_eq!(event.started_ms, *started_ms);
    }

    // 同输入两次独立绘制一致（旧路径不受 R76 分支隐藏状态影响）。
    let mut again = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut again, &mut scene, &events, center, now);
    assert_eq!(frame.pixels, again.pixels, "G7：旧路径两次绘制必须一致");
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G8：素材计划帧段（tools/assets/analyze_usage.py MAGIC_FRAMES）与删段敏感性
// ---------------------------------------------------------------------------

/// Python 解释器解析：优先 `MIR_R76_PYTHON`，其次仓库约定的 bundled numpy Python，
/// 再退回 PATH 上的 `python3` / `python`；全部不可用则带诊断文本 panic（不 SKIP）。
fn python_interpreter() -> PathBuf {
    if let Ok(path) = std::env::var("MIR_R76_PYTHON") {
        let candidate = PathBuf::from(path);
        if python_works(&candidate) {
            return candidate;
        }
        panic!(
            "G8：MIR_R76_PYTHON={} 不可执行（`--version` 失败）",
            candidate.display()
        );
    }
    let candidates = [
        PathBuf::from(
            "C:/Users/diana/.dsh/dsh-runtimes/dsh-primary-runtime/dependencies/python/python.exe",
        ),
        PathBuf::from("python3"),
        PathBuf::from("python"),
    ];
    for candidate in candidates {
        if python_works(&candidate) {
            return candidate;
        }
    }
    panic!(
        "G8 需要 Python 3 运行 tools/assets/analyze_usage.py（numpy）：已尝试 MIR_R76_PYTHON、bundled dsh runtime、python3、python"
    );
}

/// 解释器能否执行 `--version`。
fn python_works(candidate: &Path) -> bool {
    Command::new(candidate)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// 解析 probe 输出的 `a-b,c-d` 区间行。
fn parse_ranges(stdout: &str, key: &str) -> Vec<(u32, u32)> {
    let line = stdout
        .lines()
        .find(|line| line.starts_with(key))
        .unwrap_or_else(|| panic!("G8：probe 输出缺少 {key} 行，stdout={stdout}"));
    let payload = line[key.len()..].trim();
    if payload.is_empty() {
        return Vec::new();
    }
    payload
        .split(',')
        .map(|part| {
            let part = part.trim();
            let mut items = part.split('-');
            let start = items
                .next()
                .expect("区间起点")
                .trim()
                .parse::<u32>()
                .unwrap_or_else(|_| panic!("G8：区间起点不是整数：{part}"));
            let end = items
                .next()
                .unwrap_or(part)
                .trim()
                .parse::<u32>()
                .unwrap_or_else(|_| panic!("G8：区间终点不是整数：{part}"));
            assert!(start <= end, "G8：区间倒置 {part}");
            (start, end)
        })
        .collect()
}

/// 区间 → 帧集合。
fn expand_ranges(ranges: &[(u32, u32)]) -> BTreeSet<u32> {
    let mut frames = BTreeSet::new();
    for &(start, end) in ranges {
        for frame in start..=end {
            frames.insert(frame);
        }
    }
    frames
}

/// G8 探针（plan 第 90 行）：`python -B asset-plan-probe.py analyze_usage.py`，
/// `runpy.run_path` 加载真实模块（不触发 `__main__` 的 DB/rustc/地图扫描）。
const G8_PROBE: &str = r#"
import runpy, sys

module = runpy.run_path(sys.argv[1])
book = module["RefBook"]()
db = {"monsters": [], "npcs": [], "items": [], "maps": []}
module["collect_db_refs"](db, book, False, [])

def ranges(indices):
    ordered = sorted(indices)
    out = []
    for value in ordered:
        if out and value == out[-1][1] + 1:
            out[-1][1] = value
        else:
            out.append([value, value])
    return out

print("MAGIC=" + ",".join("%d-%d" % (start, end) for start, end in ranges(book.libs["magic.wzl"]["indices"])))
print("MAGIC2=" + ",".join("%d-%d" % (start, end) for start, end in ranges(book.libs["magic2.wzl"]["indices"])))
"#;

/// G8：`analyze_usage.py` 的 `MAGIC_FRAMES` 必须新增 `range(1390, 1400)`（Magic 库，
/// **不是** Magic2），Magic2 帧集合保持旧冻结集不变；旧 magic 帧集合必须原样保留为子集；
/// 删除该段后脚本仍可运行但集合回到旧集（证明「删段 → 断言变红」而不是语法错红）。
#[test]
fn r76_g8_asset_plan() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = repo_root.join("tools/assets/analyze_usage.py");
    assert!(
        script.is_file(),
        "G8：素材分析脚本必须存在：{}",
        script.display()
    );
    let source = fs::read_to_string(&script).expect("G8：读取 analyze_usage.py");
    let needle = "range(1390, 1400)";
    assert!(
        source.contains(needle),
        "G8：MAGIC_FRAMES 必须包含 {needle}（R76 唯一素材改动，Magic 库不是 Magic2）"
    );

    // plan 第 90 行：固定 `python -B <probe> <analyze_usage.py>`；probe 用 `runpy.run_path`
    // 加载真实模块（非 `__main__`），只经真实 `RefBook`/`collect_db_refs` 取引用集合。
    let probe_path = repo_root.join("target/round76/a/asset-plan-probe.py");
    if let Some(parent) = probe_path.parent() {
        fs::create_dir_all(parent).expect("G8：创建 asset-plan-probe.py 目录");
    }
    fs::write(&probe_path, G8_PROBE).expect("G8：写 asset-plan-probe.py");
    let python = python_interpreter();
    let output = Command::new(&python)
        .arg("-B")
        .arg(&probe_path)
        .arg(script.as_os_str())
        .output()
        .unwrap_or_else(|error| panic!("G8：无法启动 Python（{}）：{error}", python.display()));
    assert!(
        output.status.success(),
        "G8：probe 退出码 {:?}，stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

    // 旧 magic 帧集合独立冻结：16 方向 × 6 帧（410-415/420-425/…/560-565）+ 570-579。
    let mut old_magic = BTreeSet::new();
    for direction in 0..16u32 {
        let base = 410 + direction * 10;
        for frame in base..base + 6 {
            old_magic.insert(frame);
        }
    }
    for frame in 570..=579 {
        old_magic.insert(frame);
    }
    assert_eq!(old_magic.len(), 106, "G8：旧 magic 冻结集应为 106 帧");
    let mut expected_magic = old_magic.clone();
    for frame in 1_390..=1_399 {
        expected_magic.insert(frame);
    }
    let magic_frames = expand_ranges(&parse_ranges(&stdout, "MAGIC="));
    assert_eq!(
        magic_frames, expected_magic,
        "G8：magic 帧集合必须 = 旧冻结集 ∪ 1390..=1399（MAGIC_FRAMES 新增 range(1390, 1400)）"
    );

    // Magic2 帧集合必须不变：0-2 + 10-14 + 20-29。
    let mut expected_magic2 = BTreeSet::new();
    for frame in 0..=2 {
        expected_magic2.insert(frame);
    }
    for frame in 10..=14 {
        expected_magic2.insert(frame);
    }
    for frame in 20..=29 {
        expected_magic2.insert(frame);
    }
    let magic2_frames = expand_ranges(&parse_ranges(&stdout, "MAGIC2="));
    assert_eq!(
        magic2_frames, expected_magic2,
        "G8：Magic2 帧集合必须保持 0-2 + 10-14 + 20-29 不变（R76 素材在 Magic 库）"
    );

    // 删段敏感性：去掉新增段后脚本仍可运行（不是语法错误），集合回到旧集 —— 即上一条断言会变红。
    let temp = unique_dir("g8-mutation");
    let mutated_path = temp.join("analyze_usage_mutated.py");
    let mutated = source.replacen(needle, "range(0, 0)", 1);
    assert_ne!(mutated, source, "G8：删段替换必须生效");
    fs::write(&mutated_path, mutated).expect("G8：写临时变异脚本");
    let mutated_output = Command::new(&python)
        .arg("-B")
        .arg(&probe_path)
        .arg(mutated_path.as_os_str())
        .output()
        .unwrap_or_else(|error| panic!("G8：无法启动 Python（变异脚本）：{error}"));
    assert!(
        mutated_output.status.success(),
        "G8：删段后脚本仍须可运行（不得语法错）：stderr={}",
        String::from_utf8_lossy(&mutated_output.stderr)
    );
    let mutated_stdout = String::from_utf8_lossy(&mutated_output.stdout).into_owned();
    let mutated_magic = expand_ranges(&parse_ranges(&mutated_stdout, "MAGIC="));
    assert_eq!(
        mutated_magic, old_magic,
        "G8：删除新增段后 magic 集合必须回到旧集（证明 G8 首条集合断言「删段必红」）"
    );
    cleanup(&temp);
}
