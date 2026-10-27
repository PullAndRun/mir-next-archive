//! Round 73 A：`ThunderBolt` 命中精灵映射（Crystal `PlayerObject.cs:2956-2963` 有限切片）。
//!
//! 本文件只验收 [`docs/process/round73-plan.md`] 的有限切片（**不是**完整 Crystal
//! `ThunderBolt` 迁移）：
//!
//! * 仅**非 149 来源**（`S.Magic`(120) / `S.ObjectMagic`(123)，
//!   `SpellEvent::from_object_spell == false`）且**施法成功**（`cast == true`，即
//!   `SpellEvent::param == true`）的 `Spell::ThunderBolt` 走 Crystal
//!   `PlayerObject.cs:2956-2963` 的命中特效：
//!   `new Effect(Libraries.Magic2, 10, 5, 400, TargetPoint / ob)`，即 `Magic2` 帧
//!   `10..=14`、`Duration / Count = 400 / 5 = 80` 毫秒/帧；
//! * 本实现用「80 ms elapsed 帧窗 + 恰好 400 ms 截止」近似 Crystal 的 `Effect.NextFrame`
//!   时间轴：`phase = (age / 80).min(4)`、`index = 10 + phase`，`age >= 400` 后不再画精灵。
//!   **`Duration = 400` 是 Crystal 事实；80 ms elapsed 与 400 ms 产品精灵窗是产品近似**
//!   （Crystal 的严格 `NextMotion = Start + (Duration / Count) * (CurrentFrame + 1)`、
//!   `Owner.SkipFrames`、每轮 `Process` 至多一步见 `Effect.cs:38-61`、`Effect.cs:81-96`，
//!   未迁移 → PARTIAL）；
//! * 落点是**格锚点** `world_to_screen(center, target.location)`；本作无 owner，用包内
//!   `TargetPoint` 快照位置（`SpellEvent::location`），**不能**改用施法者位置
//!   （Crystal 有 `ob` 时特效随 `Owner.DrawLocation`，`Effect.cs:113-130`）；
//! * 混合方式 = `Frame::blit_add`（`DrawBlend(..., true, Rate = 1F)` 的 Add 子集），
//!   当帧 i16 signed offset 由 `MapScene::draw_effect_blend(..., apply_offset = true)`
//!   按 `MLibrary.cs:692-709`「先 offset 后裁剪、恰好一次」消费；
//! * 精灵窗结束后（`age >= 400`）只是不再画精灵；`spellfx::draw` 的旧 700 ms 文字/色块
//!   窗（`CAST_MS`）与「施法失败 / Lightning」的旧通用路径**保持原样**；
//! * PARTIAL（承 R71/R72 原号）：Crystal `Effect` 的 `Light = 6`/`LightColour` 光照、
//!   `SoundManager.PlaySound(20000 + (ushort)Spell * 10)` 施法音、严格 `NextMotion` 与
//!   对象寿命、`PlayerObject.cs:2819-2835` 的 `Cast` 帧循环 / `ActionFeed` 起算、
//!   Crystal 起手帧（`Effect.cs:81-96` 的 `CurrentFrame == 1` 回调）均未迁移。
//!   其他 26 技能映射仍 GAP-5；`S.Magic` 之外的 `Lightning`（`PlayerObject.cs:3225` 的
//!   `970 + Direction * 20` 保旧）不在本切片内。
//!
//! 独立 oracle：本文件内合成 `magic.wzl`/`magic.wzx`、`magic2.wzl`/`magic2.wzx`，
//! 从原始 RGB565 像素按 `Frame::blit`/`Frame::blit_add` 的文档公式手算整帧 RGBA，
//! **不调用**生产 `blit/blit_add/add_channel/blend_channel`。
//!
//! 运行（缺 `legacy` feature 时合成图库不可读，首条夹具断言以诊断文本失败而非 SKIP）：
//!
//! ```text
//! cargo test -p mir-client --features mir-render/legacy --test round73_thunderbolt_hit_mapping -- --test-threads=1
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use mir_client::net::{PlayerLooks, SelfPlayer, SpellEvent, WorldSnapshot};
use mir_client::ui::spellfx;
use mir_client::ui::{self, UiState, UiText};
use mir_core::enums::{MirClass, MirDirection, MirGender, Spell};
use mir_core::{Language, Point};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{Magic, ObjectMagic, ObjectSpell, decode_server_packet};
use mir_render::animation::ActorAction;
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
/// 合成帧边长：3×3（`10`/`13`，用于证明不按尺寸居中）。
const SIZE_3X3: (u16, u16) = (3, 3);
/// 合成帧边长：5×2（`11`，非方形，证明不按尺寸居中）。
const SIZE_5X2: (u16, u16) = (5, 2);

/// 命中帧起始号（Crystal `BaseIndex = 10`）。
const HIT_BASE: u32 = 10;
/// 命中帧每帧时长（`Duration / Count = 400 / 5 = 80` 毫秒）。
const HIT_FRAME_MS: i64 = 80;
/// 命中精灵窗时长（Crystal `Duration = 400` 毫秒；产品精灵窗截止）。
const HIT_DURATION_MS: i64 = 400;
/// 旧通用闪光帧数（`spellfx::MAGIC_FX_FRAMES` 的独立副本）。
const LEGACY_FRAME_COUNT: i64 = 10;
/// 旧通用闪光每帧时长（`spellfx::MAGIC_FX_FRAME_MS` 的独立副本）。
const LEGACY_FRAME_MS: i64 = 60;

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
/// | 0..=9 | 2×2 | (0,0) | 旧通用闪光（`magic.wzl[0..=9]`）金样/诱饵 |
/// | 10..=14 | 2×2 | (0,0) | `FxKind::Magic` 同索引诱饵：命中分支错用 `Magic` 会画到它 |
static MAIN_MAGIC: [MagicFrame; 15] = [
    MagicFrame::new(0, SIZE_2X2, (0, 0), [0x8410, 0x2104, 0x7BEF, 0xF804]),
    MagicFrame::new(1, SIZE_2X2, (0, 0), [0x2104, 0x8410, 0x07C0, 0x7BEF]),
    MagicFrame::new(2, SIZE_2X2, (0, 0), [0x7BEF, 0x07C0, 0x8410, 0x2104]),
    MagicFrame::new(3, SIZE_2X2, (0, 0), [0xFC00, 0x7BEF, 0x2104, 0x8410]),
    MagicFrame::new(4, SIZE_2X2, (0, 0), [0x2104, 0xFC00, 0x07C0, 0xF804]),
    MagicFrame::new(5, SIZE_2X2, (0, 0), [0xFC00, 0x07C0, 0x52AA, 0xFC10]),
    MagicFrame::new(6, SIZE_2X2, (0, 0), [0x07C0, 0xFC00, 0xFC10, 0x52AA]),
    MagicFrame::new(7, SIZE_2X2, (0, 0), [0x52AA, 0xFC10, 0xFC00, 0x07C0]),
    MagicFrame::new(8, SIZE_2X2, (0, 0), [0xFC10, 0x52AA, 0x07C0, 0xFC00]),
    MagicFrame::new(9, SIZE_2X2, (0, 0), [0x4010, 0xFC10, 0x52AA, 0x07C0]),
    MagicFrame::new(10, SIZE_2X2, (0, 0), [0x3456, 0x4011, 0x4020, 0x4021]),
    MagicFrame::new(11, SIZE_2X2, (0, 0), [0x4040, 0x4041, 0x4080, 0x4081]),
    MagicFrame::new(12, SIZE_2X2, (0, 0), [0x4100, 0x4101, 0x4200, 0x4201]),
    MagicFrame::new(13, SIZE_2X2, (0, 0), [0x4400, 0x4401, 0x4800, 0x4801]),
    MagicFrame::new(14, SIZE_2X2, (0, 0), [0x2222, 0x1234, 0x4321, 0x2468]),
];

/// 合成 `magic2.wzl` 帧表（`FxKind::Swing` 起始帧 0 / `FxKind::Bolt` 起始帧 20）。
///
/// | 帧 | 边长 | offset | 用途 |
/// |---|---|---|---|
/// | 0 | 2×2 | (0,0) | 零起点诱饵 |
/// | 10 | 3×3 | (-3,5) | 命中 phase 0（G3 非居中/当帧 offset 金样） |
/// | 11 | 5×2 | (4,-2) | 命中 phase 1（非方形，G3 非居中金样） |
/// | 12 | 2×2 | (-1,-1) | 命中 phase 2（G3 负坐标裁剪 + G4 Add 饱和白） |
/// | 13 | 3×3 | (50,-30) | 命中 phase 3（G3 屏外格原点被 offset 移入） |
/// | 14 | 2×2 | (31,-1) | 命中 phase 4（G3 右缘裁剪） |
/// | 20..=29 | 2×2 | (0,0) | 旧 `FxKind::Bolt` 路径金样（`Lightning`/`cast=false`/149） |
/// | 30 | 2×2 | (0,0) | `FxKind::Bolt` 同索引诱饵：命中分支错用 `Bolt` 会画 `10 + 20` |
static MAIN_MAGIC2: [MagicFrame; 17] = [
    MagicFrame::new(0, SIZE_2X2, (0, 0), [0x2222; 4]),
    MagicFrame::new(10, SIZE_3X3, (-3, 5), [0xF800; 4]),
    MagicFrame::new(11, SIZE_5X2, (4, -2), [0x07E0; 4]),
    MagicFrame::new(12, SIZE_2X2, (-1, -1), [0xFFFF; 4]),
    MagicFrame::new(13, SIZE_3X3, (50, -30), [0xFFE0; 4]),
    MagicFrame::new(14, SIZE_2X2, (31, -1), [0xF81F; 4]),
    MagicFrame::new(20, SIZE_2X2, (0, 0), [0x8410, 0x2104, 0x7BEF, 0xF804]),
    MagicFrame::new(21, SIZE_2X2, (0, 0), [0x7BEF, 0x8410, 0xF804, 0x2104]),
    MagicFrame::new(22, SIZE_2X2, (0, 0), [0xF804, 0x2104, 0x07C0, 0x7BEF]),
    MagicFrame::new(23, SIZE_2X2, (0, 0), [0x2104, 0x7BEF, 0x8410, 0xF804]),
    MagicFrame::new(24, SIZE_2X2, (0, 0), [0xFC00, 0x07C0, 0x52AA, 0xFC10]),
    MagicFrame::new(25, SIZE_2X2, (0, 0), [0x07C0, 0xFC00, 0xFC10, 0x52AA]),
    MagicFrame::new(26, SIZE_2X2, (0, 0), [0x52AA, 0xFC10, 0xFC00, 0x07C0]),
    MagicFrame::new(27, SIZE_2X2, (0, 0), [0xFC10, 0x52AA, 0x07C0, 0xFC00]),
    MagicFrame::new(28, SIZE_2X2, (0, 0), [0x4010, 0xFC10, 0x52AA, 0x07C0]),
    MagicFrame::new(29, SIZE_2X2, (0, 0), [0xFC10, 0x4010, 0x07C0, 0x52AA]),
    MagicFrame::new(30, SIZE_2X2, (0, 0), [0x3456; 4]),
];

// ---------------------------------------------------------------------------
// 夹具：进程唯一临时目录 + 合成 WZL/WZX（真实 `EntityLibraries` 读取链）
// ---------------------------------------------------------------------------

/// 进程唯一临时目录（不读 `resource`/`game-assets`，也不与其它测试共享路径）。
///
/// 优先系统临时目录；被环境策略拒绝写时回退到 `target/round73/a/tmp/`（测试产物目录）。
fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let name = format!("mir-next-r73-a-{tag}-{}-{nanos}", std::process::id());
    let candidates = [
        std::env::temp_dir().join(&name),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/round73/a/tmp")
            .join(&name),
    ];
    for dir in candidates {
        if fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }
    panic!("无法创建进程唯一临时目录（系统临时目录与 target/round73/a/tmp 均不可写）");
}

/// 合成帧的 RGB565 像素值：3×3/5×2 平铺帧全用 `quad[0]`，2×2 帧按四角取 `quad`。
fn frame_value(spec: &MagicFrame, column: u16, row: u16) -> u16 {
    if spec.size != SIZE_2X2 {
        return spec.quad[0];
    }
    let slot = usize::from(row) * 2 + usize::from(column);
    spec.quad[slot]
}

/// 按 Crystal WZL 的行序构造未压缩像素：文件第 0 行 = 图像最下面一行，
/// 每行按 4 字节对齐（`WidthBytes = ((宽 × 16 + 31) >> 5) * 4`，RGB565 下 5×2 每行补 2 字节）。
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
        file_name: "r73-empty".to_owned(),
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
    for index in 0..=14u32 {
        assert_eq!(
            scene.effect_size(FxKind::Magic, index),
            Some((2, 2)),
            "合成 magic.wzl[{index}] 必须是 2×2"
        );
    }
    for (frame, size) in [
        (10u32, (3u32, 3u32)),
        (11, (5, 2)),
        (12, (2, 2)),
        (13, (3, 3)),
        (14, (2, 2)),
    ] {
        assert_eq!(
            scene.effect_size(FxKind::Swing, frame),
            Some(size),
            "合成 magic2.wzl[{frame}]（命中 phase 帧）几何不符"
        );
    }
    for index in 0..=9u32 {
        assert_eq!(
            scene.effect_size(FxKind::Bolt, index),
            Some((2, 2)),
            "合成 magic2.wzl[{}]（Bolt 起始帧 20）必须是 2×2",
            20 + index
        );
    }
    assert_eq!(
        scene.effect_size(FxKind::Bolt, 10),
        Some((2, 2)),
        "合成 magic2.wzl[30]（`FxKind::Bolt` 同索引诱饵）必须可读"
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

/// oracle 的一次绘制：图库 + 图库内**最终**帧号 + 锚点 + 是否消费当帧 offset + 混合方式。
///
/// `frame` 是落在 `.wzl` 里的最终帧号：命中分支是 `Magic2[10 + phase]`（`Swing` 起始 0），
/// 旧通用路径是 `Magic2[20 + (age / 60).clamp(0, 9)]`（`Bolt` 起始 20）或
/// `Magic[(age / 60).clamp(0, 9)]`（`Magic` 起始 0）。
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

    const fn over(source: SpriteSource, frame: u32, x: i32, y: i32) -> Self {
        Self {
            source,
            frame,
            x,
            y,
            apply_offset: false,
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
    let (width, height) = spec.size;
    let mut pixels = Vec::with_capacity(usize::from(width) * usize::from(height));
    for row in 0..height {
        for column in 0..width {
            pixels.push((
                i32::from(column),
                i32::from(row),
                frame_value(spec, column, row),
            ));
        }
    }
    pixels
}

/// 独立整帧 oracle：从 `background` 起按 `ops` 顺序逐像素手算。
fn oracle_frame(width: u32, height: u32, background: [u8; 4], ops: &[OracleOp]) -> Vec<u8> {
    let width_usize = usize::try_from(width).expect("oracle 宽");
    let height_usize = usize::try_from(height).expect("oracle 高");
    let mut pixels = vec![0u8; width_usize * height_usize * 4];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&background);
    }
    apply_ops(&mut pixels, width, height, ops);
    pixels
}

/// 把 `ops` 依次叠到一段整帧 RGBA 缓冲上（oracle 的混合实现）。
fn apply_ops(pixels: &mut [u8], width: u32, height: u32, ops: &[OracleOp]) {
    let width_usize = usize::try_from(width).expect("oracle 宽");
    for op in ops {
        let spec = frame_spec(op.source, op.frame);
        // offset 先于裁剪；`saturating_add` 与生产入口的收口语义一致。
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
}

/// 诱饵颜色：命中帧 `Magic2[10..=14]` 之外的所有合成帧像素。
///
/// 命中分支若错画旧通用帧（`magic[0..=9]` / `magic2[20..=29]`）、`FxKind::Magic` 同索引
/// 帧（`magic[10..=14]`）或 `FxKind::Bolt` 起始帧 20 的 `magic2[30]`，会命中这些颜色。
fn decoy_colors() -> Vec<[u8; 4]> {
    let mut colors = Vec::new();
    for spec in &MAIN_MAGIC {
        for &raw in &spec.quad {
            colors.push(expand565(raw));
        }
    }
    for spec in MAIN_MAGIC2
        .iter()
        .filter(|spec| !(HIT_BASE..HIT_BASE + 5).contains(&spec.index))
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

/// 断言整帧没有任何诱饵帧颜色（旧通用帧 / 起手帧 / 跨库同索引帧）。
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
                "{label}：({x},{y}) 出现诱饵帧颜色 {value:?}；命中分支不得画旧通用帧/跨库同索引帧"
            );
        }
    }
}

/// 从 `base` 帧里取一块 ROI，按 `ops` 手算期望像素（用于 UI 叠层断言）。
fn expected_roi(
    base: &Frame,
    roi: (i32, i32, u32, u32),
    ops: &[OracleOp],
    label: &str,
) -> Vec<[u8; 4]> {
    let (x0, y0, width, height) = roi;
    let mut pixels = Vec::with_capacity(usize::try_from(width * height).expect("ROI 像素数"));
    for row in 0..height {
        for column in 0..width {
            let x = x0 + i32::try_from(column).expect("列");
            let y = y0 + i32::try_from(row).expect("行");
            pixels.push(pixel_at(base, x, y));
        }
    }
    // 把 ROI 拼成一张整帧再套 oracle（坐标整体平移），保持 oracle 单一实现。
    let mut scratch = vec![0u8; pixels.len() * 4];
    for (index, pixel) in pixels.iter().enumerate() {
        scratch[index * 4..index * 4 + 4].copy_from_slice(pixel);
    }
    let shifted: Vec<OracleOp> = ops
        .iter()
        .map(|op| OracleOp {
            source: op.source,
            frame: op.frame,
            x: op.x - x0,
            y: op.y - y0,
            apply_offset: op.apply_offset,
            blend: op.blend,
        })
        .collect();
    apply_ops(&mut scratch, width, height, &shifted);
    let mut expected = Vec::with_capacity(pixels.len());
    for index in 0..pixels.len() {
        expected.push([
            scratch[index * 4],
            scratch[index * 4 + 1],
            scratch[index * 4 + 2],
            scratch[index * 4 + 3],
        ]);
    }
    let _ = label;
    expected
}

/// 断言 ROI 与生产帧一致（失败报告首个差异像素）。
#[track_caller]
fn assert_roi_matches(frame: &Frame, roi: (i32, i32, u32, u32), expected: &[[u8; 4]], label: &str) {
    let (x0, y0, width, height) = roi;
    let mut index = 0usize;
    for row in 0..height {
        for column in 0..width {
            let x = x0 + i32::try_from(column).expect("列");
            let y = y0 + i32::try_from(row).expect("行");
            let actual = pixel_at(frame, x, y);
            assert!(
                actual == expected[index],
                "{label}：ROI 首个差异像素 ({x},{y}) actual={actual:?} \
                 expected={:?}（独立 oracle）",
                expected[index]
            );
            index += 1;
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

/// 命中分支落点：格原点 + **当帧** i16 offset（只叠一次，先于裁剪，不按尺寸居中）。
fn hit_top_left(center: Point, cell: Point, spec: &MagicFrame) -> (i32, i32) {
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

/// 旧通用 Bolt 路径期望：`magic2[20 + (age / 60).clamp(0, 9)]`，居中 + Over。
fn legacy_bolt_op(center: Point, cell: Point, age: i64) -> OracleOp {
    let frame_index = (age / LEGACY_FRAME_MS).clamp(0, LEGACY_FRAME_COUNT - 1);
    let frame = 20 + u32::try_from(frame_index).expect("旧通用帧号");
    let spec = frame_spec(SpriteSource::Magic2, frame);
    let (x, y) = legacy_top_left(center, cell, spec);
    OracleOp::over(SpriteSource::Magic2, frame, x, y)
}

/// 旧通用 Magic 路径期望：`magic[(age / 60).clamp(0, 9)]`，居中 + Over。
fn legacy_magic_op(center: Point, cell: Point, age: i64) -> OracleOp {
    let frame_index = (age / LEGACY_FRAME_MS).clamp(0, LEGACY_FRAME_COUNT - 1);
    let frame = u32::try_from(frame_index).expect("旧通用帧号");
    let spec = frame_spec(SpriteSource::Magic, frame);
    let (x, y) = legacy_top_left(center, cell, spec);
    OracleOp::over(SpriteSource::Magic, frame, x, y)
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

/// 构造真实 `S.Magic`（包 120）并经 wire 链写入快照（非 149 旧来源）。
fn push_magic_via_wire(
    snapshot: &mut WorldSnapshot,
    spell: Spell,
    target: Point,
    cast: bool,
    now_ms: u64,
) {
    let packet = through_wire(&ServerPacket::Magic(Magic {
        spell,
        target_id: 0,
        target,
        cast,
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
    location: Point,
    target: Point,
    cast: bool,
    now_ms: u64,
) {
    let packet = through_wire(&ServerPacket::ObjectMagic(ObjectMagic {
        object_id,
        location,
        direction: MirDirection::Down,
        spell,
        target_id: 0,
        target,
        cast,
        level: 3,
        self_broadcast: false,
        secondary_target_ids: Vec::new(),
    }));
    snapshot.apply_packet_at(&packet, now_ms);
}

/// 构造真实 149 `S.ObjectSpell` 并经 wire 链写入快照。
fn push_object_spell_via_wire(
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

/// 手造事件（`spellfx::draw_sprites` 的公开输入；命中分支要求 `param = true` 且
/// `from_object_spell = false`）。
fn spell_event(
    id: u64,
    spell: Spell,
    location: Point,
    started_ms: i64,
    param: bool,
    from_object_spell: bool,
) -> SpellEvent {
    SpellEvent {
        id,
        object_id: 0,
        spell,
        location,
        direction: MirDirection::Down,
        param,
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

/// 造一个带玩家（施法者）的会话快照；`position`/`direction` 是**施法者**格。
fn snapshot_with_self(position: Point, direction: MirDirection) -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::new_session();
    snapshot.self_player = Some(SelfPlayer {
        object_id: 1,
        real_id: 1,
        name: "R73Caster".to_owned(),
        position,
        direction,
        hp: 100,
        hp_max: 100,
        mp: 0,
        mp_max: 0,
        level: 1,
        class: MirClass::Wizard,
        gender: MirGender::Male,
        hair: 0,
        gold: 0,
        credit: 0,
        dead: false,
        action: ActorAction::Standing,
        action_started_ms: 0,
        attack_variant: None,
        native_action: None,
        looks: PlayerLooks::default(),
    });
    snapshot
}

// ---------------------------------------------------------------------------
// G1：真实 120/123 wire 链 → 快照 → UiState::sync → 命中帧 10
// ---------------------------------------------------------------------------

/// G1：`S.Magic`(120) 与 `S.ObjectMagic`(123) 的 `ThunderBolt` 成功施法（`cast = true`）
/// 经真实 `encode_frame → decode_server_packet → apply_packet_at` 后：
///
/// * 事件必须标记 `param = true`、`from_object_spell = false`、`started_ms = 收包时刻`，
///   落点取包内 `TargetPoint`（`Target`）——**不是**施法者位置；
/// * `t=0` 必须画 `magic2.wzl[10]`（`3×3`、格原点 + 当帧 offset `(-3,5)` 一次、Add），
///   不得画 SPELL 旧通用 `magic2[20]`（Bolt 起始 20、居中、Over）或 `magic[10]` 诱饵。
#[test]
fn r73_g1_wire_mapping() {
    let (mut scene, dir) = scene_with_default("g1-wire");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let background = [0, 0, 0, 255];
    let caster = Point::new(5, 5);
    // 目标格与施法者格相距 ≥ 2 格（对角 2,2；水平 3）。
    let magic_target = Point::new(7, 7);
    let object_target = Point::new(8, 5);
    assert!((magic_target.x - caster.x).abs() >= 2 && (magic_target.y - caster.y).abs() >= 2);
    assert!((object_target.x - caster.x).abs() >= 2);

    let mut snapshot = snapshot_with_self(caster, MirDirection::UpRight);
    push_magic_via_wire(&mut snapshot, Spell::ThunderBolt, magic_target, true, 0);
    push_object_magic_via_wire(
        &mut snapshot,
        9_001,
        Spell::ThunderBolt,
        caster,
        object_target,
        true,
        0,
    );

    assert_eq!(snapshot.spell_events.len(), 2, "两条成功施法必须写两条事件");
    let magic_event = &snapshot.spell_events[0];
    assert_eq!(
        magic_event.object_id, 1,
        "120 的 object_id 必须是本人对象编号"
    );
    assert_eq!(magic_event.spell, Spell::ThunderBolt);
    assert_eq!(
        magic_event.location, magic_target,
        "120 的落点必须取包内 TargetPoint，不能是施法者位置 {caster:?}"
    );
    assert_eq!(magic_event.direction, MirDirection::UpRight);
    assert!(magic_event.param, "120 的 cast=true 必须映射 param=true");
    assert!(
        !magic_event.from_object_spell,
        "120 不是 149 来源，必须 from_object_spell=false"
    );
    assert_eq!(magic_event.started_ms, 0);

    let object_event = &snapshot.spell_events[1];
    assert_eq!(object_event.object_id, 9_001);
    assert_eq!(object_event.spell, Spell::ThunderBolt);
    assert_eq!(
        object_event.location, object_target,
        "123 的落点必须取包内 TargetPoint，不是 Cast 时的施法者位置"
    );
    assert_eq!(object_event.direction, MirDirection::Down);
    assert!(object_event.param, "123 的 cast=true 必须映射 param=true");
    assert!(!object_event.from_object_spell);

    // 700ms 是产品文字/色块窗，不是命中精灵窗（精灵窗 400ms，见 G2）。
    assert_eq!(spellfx::duration_ms(Spell::ThunderBolt), 700);
    assert_eq!(spellfx::CAST_MS, 700);

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, 0);

    let spec = frame_spec(SpriteSource::Magic2, 10);
    let ops = [
        {
            let (x, y) = cell_origin(center, magic_target);
            OracleOp::add(SpriteSource::Magic2, 10, x, y, true)
        },
        {
            let (x, y) = cell_origin(center, object_target);
            OracleOp::add(SpriteSource::Magic2, 10, x, y, true)
        },
    ];
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
        "G1：真实 120/123 wire 链成功后必须在各自 TargetPoint 画 magic2.wzl[10]（格锚点 + offset 一次 + Add）",
    );

    // 逐像素：两格各自是 3×3 命中帧；施法者格与旧通用居中位置必须是背景。
    let legacy = frame_spec(SpriteSource::Magic2, 20);
    for cell in [magic_target, object_target] {
        let (x, y) = hit_top_left(center, cell, spec);
        for (column, row, raw) in frame_pixels(spec) {
            assert_eq!(
                pixel_at(&frame, x + column, y + row),
                expand565(raw),
                "G1：格 {cell:?} 的命中像素 ({},{}) 颜色不符",
                x + column,
                y + row
            );
        }
        let (legacy_x, legacy_y) = legacy_top_left(center, cell, legacy);
        for (column, row, _value) in frame_pixels(legacy) {
            assert_eq!(
                pixel_at(&frame, legacy_x + column, legacy_y + row),
                background,
                "G1：格 {cell:?} 不得出现旧通用 Bolt 帧（居中 + Over）"
            );
        }
    }
    // 施法者格与远离两处 TargetPoint 的邻格不得出现任何精灵（落点不是 caster）。
    for cast_cell in [caster, Point::new(caster.x - 2, caster.y - 2)] {
        let (x, y) = cell_origin(center, cast_cell);
        for dx in -1..=(CELL_WIDTH + 1) {
            for dy in -1..=(CELL_HEIGHT / 2 + 1) {
                assert_eq!(
                    pixel_at(&frame, x + dx, y + dy),
                    background,
                    "G1：施法者附近格 ({x},{y}) 偏移 ({dx},{dy}) 不得有精灵（落点必须取 TargetPoint）"
                );
            }
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
// G1（第二层）：公开 `ui::draw` 叠加命中帧到真实帧
// ---------------------------------------------------------------------------

/// G1（UI）：同一份真实 120 wire 快照，用公开 `ui::draw` 画到 `Frame`——
///
/// * `scene_full`（含 `magic2[10]`）与 `scene_missing`（缺 `magic2[10]`）两套**独立**临时
///   图库；缺帧场景不得回退到旧通用帧；
/// * 期望值 = 「同场景同快照去掉事件」的基线 ROI + 命中帧 Add 手算，不调用生产 helper；
/// * `ui::draw` 的命中 ROI 必须与 `spellfx::draw_sprites` 单独绘制的结果一致（接线证明）。
#[test]
fn r73_g1_ui_draw() {
    let center = center_point();
    let background = [0, 0, 0, 255];
    let caster = Point::new(6, 6);
    let target = Point::new(10, 10);
    let started = 1_000i64;
    let roi = {
        let spec = frame_spec(SpriteSource::Magic2, 10);
        let (x, y) = hit_top_left(center, target, spec);
        (x, y, u32::from(spec.size.0), u32::from(spec.size.1))
    };
    assert_eq!(
        roi,
        (381, 261, 3, 3),
        "G1 UI：命中帧 10 ROI 必须是格原点 + offset"
    );

    for (tag, dir, mut scene) in [
        {
            let (mut scene, dir) = scene_with_default("g1-ui-full");
            assert_default_fixture_readable(&mut scene);
            ("full", dir, scene)
        },
        {
            let dir = unique_dir("g1-ui-missing");
            write_library(&dir, "magic", &MAIN_MAGIC);
            write_library(&dir, "magic2", &magic2_without(10));
            let mut scene = scene_in_dir(&dir);
            assert_eq!(
                scene.effect_size(FxKind::Swing, 10),
                None,
                "G1 UI：missing 场景必须缺 magic2[10]"
            );
            assert_eq!(
                scene.effect_size(FxKind::Swing, 11),
                Some((5, 2)),
                "G1 UI：missing 场景其它命中帧仍可读"
            );
            ("missing", dir, scene)
        },
    ] {
        let mut plain = snapshot_with_self(caster, MirDirection::Down);
        plain.spell_events.clear();
        let mut with_event = snapshot_with_self(caster, MirDirection::Down);
        push_magic_via_wire(
            &mut with_event,
            Spell::ThunderBolt,
            target,
            true,
            u64::try_from(started).expect("wire 时刻"),
        );
        assert_eq!(with_event.spell_events.len(), 1);
        assert_eq!(with_event.spell_events[0].started_ms, started);

        let text = UiText::fallback();
        let mut base = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        ui::draw(
            &mut ui_state_with_time(started),
            &mut base,
            &mut scene,
            &plain,
            &text,
            None,
            center,
        );
        let mut drawn = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        ui::draw(
            &mut ui_state_with_time(started),
            &mut drawn,
            &mut scene,
            &with_event,
            &text,
            None,
            center,
        );
        // `spellfx::draw_sprites` 单独画在纯背景上（该入口只画精灵，不含文字/色块）。
        let mut direct = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(
            &mut direct,
            &mut scene,
            &with_event.spell_events,
            center,
            started,
        );

        // ROI 基线必须是纯背景（文字在落点上方、色块在格矩形内，都不得侵入命中帧足迹）。
        for row in 0..roi.3 {
            for column in 0..roi.2 {
                let x = roi.0 + i32::try_from(column).expect("列");
                let y = roi.1 + i32::try_from(row).expect("行");
                assert_eq!(
                    pixel_at(&base, x, y),
                    background,
                    "G1 UI（{tag}）：ROI ({x},{y}) 基线必须是背景（HUD/文字/色块不得侵入）"
                );
            }
        }

        let op = {
            let (x, y) = cell_origin(center, target);
            OracleOp::add(SpriteSource::Magic2, 10, x, y, true)
        };

        if tag == "missing" {
            // 缺目标帧：精灵层整帧必须零落屏（不回退旧通用路径/不 panic）。
            assert_only_background(
                &direct,
                background,
                "G1 UI（missing）：缺 magic2[10] 时 draw_sprites 不得画任何回退精灵",
            );
            // `ui::draw` 的命中 ROI 必须保持基线（精灵层没画任何东西）。
            let base_roi = expected_roi(&base, roi, &[], "G1 UI（missing）");
            assert_roi_matches(
                &drawn,
                roi,
                &base_roi,
                "G1 UI（missing）：缺帧时命中 ROI 必须保持基线",
            );
            cleanup(&dir);
            continue;
        }

        // 精灵层整帧 = 恰好一帧命中帧（格锚点 + offset 一次 + Add），没有旧通用帧/诱饵帧。
        assert_frame_matches(
            &direct,
            &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &[op]),
            "G1 UI：draw_sprites 整帧 = 命中帧 magic2[10]",
        );
        assert_no_decoy_colors(&direct, background, "G1 UI direct");
        // `ui::draw` 的命中 ROI = 基线 ROI + 命中帧 Add（独立 oracle，不调用生产 helper 自证）。
        let expected = expected_roi(&base, roi, &[op], "G1 UI");
        assert_roi_matches(&drawn, roi, &expected, "G1 UI：ui::draw 命中帧 ROI");
        // 接线证明：`ui::draw` 与 `draw_sprites` 的命中 ROI 逐像素一致。
        for row in 0..roi.3 {
            for column in 0..roi.2 {
                let x = roi.0 + i32::try_from(column).expect("列");
                let y = roi.1 + i32::try_from(row).expect("行");
                assert_eq!(
                    pixel_at(&drawn, x, y),
                    pixel_at(&direct, x, y),
                    "G1 UI：ui::draw 与 draw_sprites 的命中 ROI 必须一致（接线）"
                );
            }
        }
        cleanup(&dir);
    }
}

/// 构造一个已推进到 `now_ms` 的独立 `UiState`（每次绘制都用全新状态避免跨调用串味）。
fn ui_state_with_time(now_ms: i64) -> UiState {
    let mut ui = ui_state();
    ui.set_time(now_ms);
    ui
}

// ---------------------------------------------------------------------------
// G2：帧边界（80ms/帧、5 帧、400ms 截止）
// ---------------------------------------------------------------------------

/// G2：`phase = (age / 80).min(4)` 的每个边界都必须映射到 `10 + phase`：
/// `60/79/80`、`159/160`、`239/240`、`319/320`、`399` 各边界 + `400/401` 截止。
///
/// Crystal `Duration / Count = 400 / 5 = 80` 是事实；本实现用 elapsed 分隔，
/// 因而 `age < 400` 的每一帧都由 `10..=14` 覆盖，`age = 400` 整帧必须零落屏。
#[test]
fn r73_g2_frame_boundaries() {
    let (mut scene, dir) = scene_with_default("g2-frame");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let background = [0, 0, 0, 255];
    let started = 1_000i64;
    let target = center;
    let events = [spell_event(
        1,
        Spell::ThunderBolt,
        target,
        started,
        true,
        false,
    )];

    // Crystal `Duration = 400`、`Count = 5` → 80 ms/帧（oracle 侧独立常量一致性）。
    assert_eq!(HIT_DURATION_MS, HIT_FRAME_MS * 5);

    let cases: [(i64, Option<u32>); 13] = [
        (0, Some(10)),
        (60, Some(10)),
        (79, Some(10)),
        (80, Some(11)),
        (159, Some(11)),
        (160, Some(12)),
        (239, Some(12)),
        (240, Some(13)),
        (319, Some(13)),
        (320, Some(14)),
        (399, Some(14)),
        (400, None),
        (401, None),
    ];
    let (origin_x, origin_y) = cell_origin(center, target);
    for (age, expected_index) in cases {
        let now_ms = started + age;
        let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(&mut frame, &mut scene, &events, center, now_ms);
        let label = format!(
            "G2：age={age} 必须映射 {}",
            expected_index.map_or("零落屏".to_owned(), |index| format!(
                "magic2.wzl[{index}]"
            ))
        );
        match expected_index {
            Some(index) => {
                let spec = frame_spec(SpriteSource::Magic2, index);
                let ops = [OracleOp::add(
                    SpriteSource::Magic2,
                    index,
                    origin_x,
                    origin_y,
                    true,
                )];
                assert_frame_matches(
                    &frame,
                    &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
                    &label,
                );
                assert_eq!(
                    covered_count(&frame, background),
                    usize::from(spec.size.0) * usize::from(spec.size.1),
                    "G2：age={age} 覆盖像素数必须等于帧面积"
                );
                let (x, y) = hit_top_left(center, target, spec);
                assert_eq!(
                    pixel_at(&frame, x, y),
                    expand565(frame_value(spec, 0, 0)),
                    "G2：age={age} 首像素颜色"
                );
            }
            None => assert_only_background(&frame, background, &label),
        }
    }
    cleanup(&dir);
}

/// G2：命中精灵窗 400ms 与旧 700ms 文字/色块窗并存——
/// `age=399` 画 `magic2[14]`；`400..=699` 精灵零落屏但 `spellfx::draw` 的色块仍存在；
/// `age>=700`（`CAST_MS`）连文字/色块窗也结束；负 age 与 i64 极值零落屏。
#[test]
fn r73_g2_age_window() {
    let (mut scene, dir) = scene_with_default("g2-window");
    assert_default_fixture_readable(&mut scene);
    assert_eq!(spellfx::duration_ms(Spell::ThunderBolt), 700);
    assert_eq!(spellfx::CAST_MS, 700);

    let center = center_point();
    let background = [0, 0, 0, 255];
    let started = 1_000i64;
    let events = [spell_event(
        1,
        Spell::ThunderBolt,
        center,
        started,
        true,
        false,
    )];
    let text = UiText::fallback();
    let plain = snapshot_with_self(Point::new(5, 5), MirDirection::Down);

    for now_ms in [999, 0, i64::MIN, i64::MAX] {
        let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(&mut frame, &mut scene, &events, center, now_ms);
        assert_only_background(
            &frame,
            background,
            &format!("G2：age 越窗 now={now_ms} 必须零落屏"),
        );
    }

    // age=399 仍画 phase 4（magic2[14]，2×2）。
    {
        let ops = [{
            let (x, y) = cell_origin(center, center);
            OracleOp::add(SpriteSource::Magic2, 14, x, y, true)
        }];
        let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(&mut frame, &mut scene, &events, center, started + 399);
        assert_frame_matches(
            &frame,
            &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
            "G2：age=399 仍画 magic2[14]（精灵窗内尾帧）",
        );
    }

    // 400..=699：命中精灵不画，但旧色块/文字窗仍在（`spellfx::draw` 非空）。
    for age in [400i64, 401, 699] {
        let mut sprites = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(&mut sprites, &mut scene, &events, center, started + age);
        assert_only_background(
            &sprites,
            background,
            &format!("G2：age={age} 命中精灵必须停止绘制"),
        );

        let mut legacy_layer = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        spellfx::draw(
            &mut legacy_layer,
            &events,
            &plain,
            &scene,
            center,
            &text,
            Language::En,
            started + age,
        );
        assert!(
            covered_count(&legacy_layer, background) > 0,
            "G2：age={age} 旧 700ms 文字/色块窗必须保留（CAST_MS 未变）"
        );
    }

    // age=700（CAST_MS 截止）：旧文字/色块窗也结束。
    for age in [700i64, 701] {
        let mut legacy_layer = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        spellfx::draw(
            &mut legacy_layer,
            &events,
            &plain,
            &scene,
            center,
            &text,
            Language::En,
            started + age,
        );
        assert_only_background(
            &legacy_layer,
            background,
            &format!("G2：age={age} 产品 700ms 文字/色块窗必须结束"),
        );
    }
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G3：格锚点 + 当帧 signed offset（恰一次、先 offset 后裁剪）
// ---------------------------------------------------------------------------

/// G3：命中帧落点 = 格原点 + **当帧** i16 offset（只叠一次、不按尺寸居中），
/// 且随相机中心平移；帧 10（3×3，offset `(-3,5)`）与帧 11（5×2，offset `(4,-2)`）
/// 尺寸/偏移都不同，直接证明不是「48×32 居中」的旧路径。
#[test]
fn r73_g3_anchor_offsets() {
    let (mut scene, dir) = scene_with_default("g3-anchor");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let background = [0, 0, 0, 255];
    let target = center;
    let started = 1_000i64;
    let events = [spell_event(
        1,
        Spell::ThunderBolt,
        target,
        started,
        true,
        false,
    )];

    // phase 0 → magic2[10]（3×3，offset (-3,5)）：格原点 (384,256) + offset = (381,261)。
    let spec10 = frame_spec(SpriteSource::Magic2, 10);
    assert_eq!(hit_top_left(center, target, spec10), (381, 261));
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, center, started);
    let ops = [{
        let (x, y) = cell_origin(center, target);
        OracleOp::add(SpriteSource::Magic2, 10, x, y, true)
    }];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G3：phase 0 落点 = 格原点 + 当帧 offset (-3,5)，Add",
    );

    // phase 1 → magic2[11]（5×2，offset (4,-2)）：格原点 + (4,-2) = (388,254)。
    let spec11 = frame_spec(SpriteSource::Magic2, 11);
    assert_eq!(hit_top_left(center, target, spec11), (388, 254));
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut frame,
        &mut scene,
        &events,
        center,
        started + HIT_FRAME_MS,
    );
    let ops = [{
        let (x, y) = cell_origin(center, target);
        OracleOp::add(SpriteSource::Magic2, 11, x, y, true)
    }];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G3：phase 1 落点 = 格原点 + 当帧 offset (4,-2)，Add",
    );

    // 不按尺寸居中：旧通用路径的居中位置必须为空。
    let (legacy10_x, legacy10_y) = legacy_top_left(center, target, spec10);
    let (legacy11_x, legacy11_y) = legacy_top_left(center, target, spec11);
    for (column, row, _value) in frame_pixels(spec11) {
        assert_eq!(
            pixel_at(&frame, legacy11_x + column, legacy11_y + row),
            background,
            "G3：5×2/offset (4,-2) 帧不得出现在「48×32 居中」旧位置"
        );
    }
    let mut frame10 = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame10, &mut scene, &events, center, started);
    for (column, row, _value) in frame_pixels(spec10) {
        assert_eq!(
            pixel_at(&frame10, legacy10_x + column, legacy10_y + row),
            background,
            "G3：3×3/offset (-3,5) 帧不得出现在「48×32 居中」旧位置"
        );
    }

    // offset 恰好一次：把 offset 叠第二次的位置必须为空。
    for (column, row, _value) in frame_pixels(spec11) {
        assert_eq!(
            pixel_at(
                &frame,
                388 + i32::from(spec11.offset.0) + column,
                254 + i32::from(spec11.offset.1) + row,
            ),
            background,
            "G3：offset 不得叠第二次（({},{}) 必须为空）",
            388 + i32::from(spec11.offset.0) + column,
            254 + i32::from(spec11.offset.1) + row,
        );
    }

    // 换相机中心：同一格/同一帧仍然落在「新 center 下的格原点 + offset」。
    let center2 = Point::new(11, 12);
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut frame,
        &mut scene,
        &events,
        center2,
        started + HIT_FRAME_MS,
    );
    let (dx, dy) = cell_origin(center2, target);
    let ops = [OracleOp::add(SpriteSource::Magic2, 11, dx, dy, true)];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G3：换 center 后落点仍 = 新格原点 + 当帧 offset",
    );

    // 邻格（48/32 步进）：每格独立落点。
    for (cell, phase) in [
        (Point::new(center.x + 1, center.y), 0u32),
        (Point::new(center.x - 1, center.y - 1), 1),
        (Point::new(center.x, center.y + 2), 2),
        (Point::new(center.x + 2, center.y), 3),
        (Point::new(center.x - 2, center.y + 1), 4),
    ] {
        let events = [spell_event(
            u64::from(phase) + 10,
            Spell::ThunderBolt,
            cell,
            started,
            true,
            false,
        )];
        let spec = frame_spec(SpriteSource::Magic2, HIT_BASE + phase);
        let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(
            &mut frame,
            &mut scene,
            &events,
            center,
            started + i64::from(phase) * HIT_FRAME_MS,
        );
        let (x, y) = cell_origin(center, cell);
        let ops = [OracleOp::add(
            SpriteSource::Magic2,
            HIT_BASE + phase,
            x,
            y,
            true,
        )];
        assert_frame_matches(
            &frame,
            &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
            &format!("G3：格 {cell:?} phase {phase} 落点"),
        );
        let (top_left_x, top_left_y) = hit_top_left(center, cell, spec);
        assert_eq!(
            pixel_at(&frame, top_left_x, top_left_y),
            expand565(frame_value(spec, 0, 0)),
            "G3：格 {cell:?} phase {phase} 首像素"
        );
    }
    cleanup(&dir);
}

/// G3：屏外裁剪——负坐标部分裁剪、完全屏外零落屏、屏外格原点被当帧 offset 移入、
/// 右缘部分裁剪；全部与独立 oracle 逐像素一致（offset 先于裁剪、恰一次）。
#[test]
fn r73_g3_offscreen_crop() {
    let (mut scene, dir) = scene_with_default("g3-crop");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let background = [0, 0, 0, 255];
    let started = 1_000i64;

    // 1) 负坐标部分裁剪：phase 2（magic2[12]，2×2，offset (-1,-1)）落在格原点 (0,0)
    //    → 只有 (0,0) 一个像素可见。
    let cell = Point::new(center.x - 8, center.y - 8);
    assert_eq!(cell_origin(center, cell), (0, 0));
    let events = [spell_event(
        1,
        Spell::ThunderBolt,
        cell,
        started,
        true,
        false,
    )];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut frame,
        &mut scene,
        &events,
        center,
        started + 2 * HIT_FRAME_MS,
    );
    let spec12 = frame_spec(SpriteSource::Magic2, 12);
    let ops = [{
        let (x, y) = cell_origin(center, cell);
        OracleOp::add(SpriteSource::Magic2, 12, x, y, true)
    }];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G3：负坐标部分裁剪（offset(-1,-1)，仅 1 像素可见）",
    );
    assert_eq!(
        covered_count(&frame, background),
        1,
        "G3：2×2 帧在原点 + (-1,-1) 只剩 1 个像素"
    );
    let (x, y) = hit_top_left(center, cell, spec12);
    assert_eq!((x, y), (-1, -1));
    assert_eq!(
        pixel_at(&frame, 0, 0),
        expand565(frame_value(spec12, 0, 0)),
        "G3：裁剪后唯一像素"
    );

    // 2) 完全屏外：phase 3（magic2[13]，3×3，offset (50,-30)）落在格原点 (-48,-64)
    //    → 左 (-46)、上 (-94) 全部在屏外，零落屏。
    let cell = Point::new(center.x - 9, center.y - 10);
    assert_eq!(cell_origin(center, cell), (-48, -64));
    let events = [spell_event(
        2,
        Spell::ThunderBolt,
        cell,
        started,
        true,
        false,
    )];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut frame,
        &mut scene,
        &events,
        center,
        started + 3 * HIT_FRAME_MS,
    );
    assert_only_background(
        &frame,
        background,
        "G3：完全屏外的格原点 + 当帧 offset 仍零落屏",
    );

    // 3) 屏外格原点被当帧 offset 移入：格原点 (-48,32) + (50,-30) = (2,2)，3×3 全可见。
    let cell = Point::new(center.x - 9, center.y - 7);
    assert_eq!(cell_origin(center, cell), (-48, 32));
    let events = [spell_event(
        3,
        Spell::ThunderBolt,
        cell,
        started,
        true,
        false,
    )];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut frame,
        &mut scene,
        &events,
        center,
        started + 3 * HIT_FRAME_MS,
    );
    let ops = [{
        let (x, y) = cell_origin(center, cell);
        OracleOp::add(SpriteSource::Magic2, 13, x, y, true)
    }];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G3：屏外格原点被当帧 offset (50,-30) 移入 (2,2)",
    );
    let spec13 = frame_spec(SpriteSource::Magic2, 13);
    assert_eq!(hit_top_left(center, cell, spec13), (2, 2));
    assert_eq!(covered_count(&frame, background), 9, "G3：3×3 全部可见");

    // 4) 右缘部分裁剪：phase 4（magic2[14]，2×2，offset (31,-1)）落在格原点 (768,256)
    //    → 左 799，x=800 被裁掉，只剩左列 2 个像素。
    let cell = Point::new(center.x + 8, center.y);
    assert_eq!(cell_origin(center, cell), (768, 256));
    let events = [spell_event(
        4,
        Spell::ThunderBolt,
        cell,
        started,
        true,
        false,
    )];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut frame,
        &mut scene,
        &events,
        center,
        started + 4 * HIT_FRAME_MS,
    );
    let spec14 = frame_spec(SpriteSource::Magic2, 14);
    let ops = [{
        let (x, y) = cell_origin(center, cell);
        OracleOp::add(SpriteSource::Magic2, 14, x, y, true)
    }];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G3：右缘部分裁剪（offset(31,-1)，x=800 列被裁）",
    );
    assert_eq!(hit_top_left(center, cell, spec14), (799, 255));
    assert_eq!(
        covered_count(&frame, background),
        2,
        "G3：右缘只剩左列 2 像素"
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G4：Add 混合（逐格、饱和、非黑底两事件叠加）
// ---------------------------------------------------------------------------

/// G4：命中帧走 `blit_add`（Add），逐格独立叠加——
/// * 非黑底上 Add 的期望值按 `min(255, dst + round(src * a / 255))` 手算；
/// * 白帧（`magic2[12]`，0xFFFF 展开）在亮底上必须饱和到 255（Over 会得到 248/252/248）；
/// * 同格两条事件（phase 0 + phase 1）按事件顺序各叠一次，独立整帧 oracle。
#[test]
fn r73_g4_add_per_cell() {
    let (mut scene, dir) = scene_with_default("g4-add");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let background = [40, 80, 120, 255];
    let started = 1_000i64;
    let cell = center;
    let events = [spell_event(
        1,
        Spell::ThunderBolt,
        cell,
        started,
        true,
        false,
    )];

    // phase 0：magic2[10]（红色 0xF800）叠在非黑底上。
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &events, center, started);
    let spec10 = frame_spec(SpriteSource::Magic2, 10);
    let ops = [{
        let (x, y) = cell_origin(center, cell);
        OracleOp::add(SpriteSource::Magic2, 10, x, y, true)
    }];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G4：非黑底上的 Add（10，红）必须逐像素等于手算 Add",
    );
    // 红通道饱和：0 + 248? 底 40 + 248 = 288 → 255；若错用 Over 会得到 248。
    let (x, y) = hit_top_left(center, cell, spec10);
    assert_eq!(
        pixel_at(&frame, x, y),
        [255, 80, 120, 255],
        "G4：Add 必须饱和而不是 Over 覆盖（红 255 / 绿 80 / 蓝 120）"
    );
    assert_eq!(
        pixel_at(&frame, x, y),
        [
            oracle_add_channel(background[0], 0xF8, 255),
            oracle_add_channel(background[1], 0x00, 255),
            oracle_add_channel(background[2], 0x00, 255),
            255
        ]
    );

    // phase 2：白帧（0xFFFF）在亮底上：三通道全部饱和到 255。
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, [250, 200, 100, 255]);
    draw_at(
        &mut frame,
        &mut scene,
        &events,
        center,
        started + 2 * HIT_FRAME_MS,
    );
    let spec12 = frame_spec(SpriteSource::Magic2, 12);
    let ops = [{
        let (x, y) = cell_origin(center, cell);
        OracleOp::add(SpriteSource::Magic2, 12, x, y, true)
    }];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, [250, 200, 100, 255], &ops),
        "G4：白帧在亮底上 Add 饱和",
    );
    let (x, y) = hit_top_left(center, cell, spec12);
    assert_eq!(
        pixel_at(&frame, x, y),
        [255, 255, 255, 255],
        "G4：0xFFFF 展开 (248,252,248) + 亮底必须饱和为白（Over 不会）"
    );

    // 同格两条事件（phase 0 + phase 1）独立各叠一次：整帧 = 依次 Add 两次。
    let two = [
        spell_event(1, Spell::ThunderBolt, cell, started, true, false),
        spell_event(
            2,
            Spell::ThunderBolt,
            cell,
            started + HIT_FRAME_MS,
            true,
            false,
        ),
    ];
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &two, center, started + HIT_FRAME_MS);
    let op10 = {
        let (x, y) = cell_origin(center, cell);
        OracleOp::add(SpriteSource::Magic2, 10, x, y, true)
    };
    let op11 = {
        let (x, y) = cell_origin(center, cell);
        OracleOp::add(SpriteSource::Magic2, 11, x, y, true)
    };
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &[op10, op11]),
        "G4：同格两事件按事件顺序独立 Add（10 然后 11）",
    );
    // 两帧都覆盖的像素要叠加两次：可见像素数 = 10 的 9 像素 ∪ 11 的 10 像素。
    let mut union: Vec<(i32, i32)> = frame_pixels(spec10)
        .iter()
        .map(|(column, row, _)| {
            (
                hit_top_left(center, cell, spec10).0 + column,
                hit_top_left(center, cell, spec10).1 + row,
            )
        })
        .collect();
    for (column, row, _value) in frame_pixels(frame_spec(SpriteSource::Magic2, 11)) {
        let point = (
            hit_top_left(center, cell, frame_spec(SpriteSource::Magic2, 11)).0 + column,
            hit_top_left(center, cell, frame_spec(SpriteSource::Magic2, 11)).1 + row,
        );
        if !union.contains(&point) {
            union.push(point);
        }
    }
    assert_eq!(
        covered_count(&frame, background),
        union.len(),
        "G4：两事件并集覆盖像素数"
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G5：来源/施法矩阵与旧行为金样
// ---------------------------------------------------------------------------

/// G5 矩阵的行来源（120/123/149 三种 wire 入口）。
#[derive(Clone, Copy)]
enum Source {
    Magic(bool),
    ObjectMagic(bool),
    ObjectSpell(bool),
}

/// G5 矩阵的一行：来源 + 技能 + 来源标记 + 施法参数 + 期望路径。
struct Row {
    label: &'static str,
    source: Source,
    spell: Spell,
    from_object_spell: bool,
    param: bool,
    expect_hit: bool,
}

/// G5：来源 × 施法状态矩阵逐行——
///
/// * 120/123 + `cast = true` + `ThunderBolt` → 命中帧 `magic2[10 + phase]`（Add + offset）；
/// * 120/123 + `cast = false` → 旧通用 Bolt 路径（`magic2[20 + 60ms 帧]`，居中 + Over）；
/// * 149（`S.ObjectSpell`）× `param` true/false → 旧通用 Bolt 路径（不抢命中分支）；
/// * `Lightning`（120/123/149 全来源）→ 旧通用 Bolt 路径金样（旧行为不变）。
#[test]
fn r73_g5_source_cast_legacy() {
    let (mut scene, dir) = scene_with_default("g5-matrix");
    assert_default_fixture_readable(&mut scene);

    let center = center_point();
    let background = [0, 0, 0, 255];
    let caster = Point::new(5, 5);
    let target = Point::new(8, 8);
    let started = 1_000i64;
    // 固定 age = 120ms：命中 phase 1 → magic2[11]；旧通用帧 = 120/60 = 2 → 20 + 2 = 22。
    let age = 120i64;
    let hit_phase = age / HIT_FRAME_MS; // 1
    let legacy_frame = u32::try_from(20 + age / LEGACY_FRAME_MS).expect("旧通用帧号"); // 22
    assert_eq!(hit_phase, 1);
    assert_eq!(legacy_frame, 22);

    let rows = [
        Row {
            label: "120+cast=true ThunderBolt",
            source: Source::Magic(true),
            spell: Spell::ThunderBolt,
            from_object_spell: false,
            param: true,
            expect_hit: true,
        },
        Row {
            label: "120+cast=false ThunderBolt",
            source: Source::Magic(false),
            spell: Spell::ThunderBolt,
            from_object_spell: false,
            param: false,
            expect_hit: false,
        },
        Row {
            label: "123+cast=true ThunderBolt",
            source: Source::ObjectMagic(true),
            spell: Spell::ThunderBolt,
            from_object_spell: false,
            param: true,
            expect_hit: true,
        },
        Row {
            label: "123+cast=false ThunderBolt",
            source: Source::ObjectMagic(false),
            spell: Spell::ThunderBolt,
            from_object_spell: false,
            param: false,
            expect_hit: false,
        },
        Row {
            label: "149+param=false ThunderBolt",
            source: Source::ObjectSpell(false),
            spell: Spell::ThunderBolt,
            from_object_spell: true,
            param: false,
            expect_hit: false,
        },
        Row {
            label: "149+param=true ThunderBolt",
            source: Source::ObjectSpell(true),
            spell: Spell::ThunderBolt,
            from_object_spell: true,
            param: true,
            expect_hit: false,
        },
        Row {
            label: "120+cast=true Lightning",
            source: Source::Magic(true),
            spell: Spell::Lightning,
            from_object_spell: false,
            param: true,
            expect_hit: false,
        },
        Row {
            label: "120+cast=false Lightning",
            source: Source::Magic(false),
            spell: Spell::Lightning,
            from_object_spell: false,
            param: false,
            expect_hit: false,
        },
        Row {
            label: "123+cast=true Lightning",
            source: Source::ObjectMagic(true),
            spell: Spell::Lightning,
            from_object_spell: false,
            param: true,
            expect_hit: false,
        },
        Row {
            label: "149+param=false Lightning",
            source: Source::ObjectSpell(false),
            spell: Spell::Lightning,
            from_object_spell: true,
            param: false,
            expect_hit: false,
        },
        Row {
            label: "149+param=true Lightning",
            source: Source::ObjectSpell(true),
            spell: Spell::Lightning,
            from_object_spell: true,
            param: true,
            expect_hit: false,
        },
    ];

    for row in rows {
        let mut snapshot = snapshot_with_self(caster, MirDirection::Down);
        match row.source {
            Source::Magic(cast) => {
                push_magic_via_wire(
                    &mut snapshot,
                    row.spell,
                    target,
                    cast,
                    u64::try_from(started).expect("wire 时刻"),
                );
            }
            Source::ObjectMagic(cast) => {
                push_object_magic_via_wire(
                    &mut snapshot,
                    9_100,
                    row.spell,
                    caster,
                    target,
                    cast,
                    u64::try_from(started).expect("wire 时刻"),
                );
            }
            Source::ObjectSpell(param) => {
                push_object_spell_via_wire(
                    &mut snapshot,
                    9_200,
                    row.spell,
                    target,
                    MirDirection::Down,
                    param,
                    u64::try_from(started).expect("wire 时刻"),
                );
            }
        }
        assert_eq!(
            snapshot.spell_events.len(),
            1,
            "G5（{}）：必须恰好一条事件",
            row.label
        );
        let event = &snapshot.spell_events[0];
        assert_eq!(event.spell, row.spell, "G5（{}）：技能", row.label);
        assert_eq!(
            event.location, target,
            "G5（{}）：落点必须是包内 TargetPoint",
            row.label
        );
        assert_eq!(event.param, row.param, "G5（{}）：param", row.label);
        assert_eq!(
            event.from_object_spell, row.from_object_spell,
            "G5（{}）：from_object_spell",
            row.label
        );

        let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(
            &mut frame,
            &mut scene,
            &snapshot.spell_events,
            center,
            started + age,
        );

        if row.expect_hit {
            let (x, y) = cell_origin(center, target);
            let ops = [OracleOp::add(
                SpriteSource::Magic2,
                HIT_BASE + u32::try_from(hit_phase).expect("phase"),
                x,
                y,
                true,
            )];
            assert_frame_matches(
                &frame,
                &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
                &format!("G5（{}）：命中帧 magic2[11]，Add + offset", row.label),
            );
            let spec = frame_spec(SpriteSource::Magic2, 11);
            // 格 (8,8) 原点 (288,192) + 当帧 offset (4,-2) = (292,190)（手算，不调生产 helper）。
            assert_eq!(hit_top_left(center, target, spec), (292, 190));
        } else {
            // 旧通用 Bolt 金样：居中 + Over，不消费帧 offset。
            let ops = [legacy_bolt_op(center, target, age)];
            assert_frame_matches(
                &frame,
                &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
                &format!("G5（{}）：旧通用 Bolt 路径金样", row.label),
            );
            let spec = frame_spec(SpriteSource::Magic2, legacy_frame);
            let (x, y) = legacy_top_left(center, target, spec);
            assert_eq!(
                pixel_at(&frame, x, y),
                expand565(frame_value(spec, 0, 0)),
                "G5（{}）：旧通用 Bolt 首像素",
                row.label
            );
            // 旧路径不得画命中帧。
            let hit_spec = frame_spec(
                SpriteSource::Magic2,
                HIT_BASE + u32::try_from(hit_phase).expect("phase"),
            );
            let (hit_x, hit_y) = hit_top_left(center, target, hit_spec);
            for (column, row_index, _value) in frame_pixels(hit_spec) {
                assert_eq!(
                    pixel_at(&frame, hit_x + column, hit_y + row_index),
                    background,
                    "G5（{}）：旧路径不得出现命中帧像素",
                    row.label
                );
            }
        }
    }
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G5：缺帧/空库容错
// ---------------------------------------------------------------------------

/// G5：缺帧/空库——
/// * 缺 `magic2[11]` 时 age=80 必须零落屏（不回退旧通用路径、不 panic），
///   而 age=0（帧 10）与 age=160（帧 12）仍照常画；
/// * 整库缺失 `magic2` 时命中窗口安全不画；
/// * 同一份快照里其它技能（120 `FireBall` 的旧 `Magic` 路径）照常绘制。
#[test]
fn r73_g5_missing_frame() {
    let background = [0, 0, 0, 255];
    let center = center_point();
    let target = center;
    let started = 1_000i64;

    // 1) 缺 11 帧：10/12 可读，11 不可读。
    let dir = unique_dir("g5-missing");
    write_library(&dir, "magic", &MAIN_MAGIC);
    write_library(&dir, "magic2", &magic2_without(11));
    let mut scene = scene_in_dir(&dir);
    assert_eq!(
        scene.effect_size(FxKind::Swing, 11),
        None,
        "G5：缺 11 帧夹具必须读不到 magic2[11]"
    );
    assert_eq!(scene.effect_size(FxKind::Swing, 10), Some((3, 3)));
    assert_eq!(scene.effect_size(FxKind::Swing, 12), Some((2, 2)));

    let events = [spell_event(
        1,
        Spell::ThunderBolt,
        target,
        started,
        true,
        false,
    )];
    // age=80 → phase 1（帧 11 缺失）：零落屏。
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(
        &mut frame,
        &mut scene,
        &events,
        center,
        started + HIT_FRAME_MS,
    );
    assert_only_background(
        &frame,
        background,
        "G5：缺 11 帧时 phase 1 必须零落屏（不回退旧路径）",
    );

    // age=0（帧 10）与 age=160（帧 12）仍照常。
    for (age, index) in [(0i64, 10u32), (2 * HIT_FRAME_MS, 12)] {
        let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(&mut frame, &mut scene, &events, center, started + age);
        let ops = [{
            let (x, y) = cell_origin(center, target);
            OracleOp::add(SpriteSource::Magic2, index, x, y, true)
        }];
        assert_frame_matches(
            &frame,
            &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
            &format!("G5：缺 11 帧不影响 age={age} 的 magic2[{index}]"),
        );
    }
    cleanup(&dir);

    // 2) 整库缺 magic2：命中窗口安全不画（无 panic）。
    let (mut scene, dir) = scene_with_magic_only("g5-empty-magic2");
    for age in [
        0i64,
        HIT_FRAME_MS,
        2 * HIT_FRAME_MS,
        3 * HIT_FRAME_MS,
        4 * HIT_FRAME_MS,
    ] {
        let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
        draw_at(&mut frame, &mut scene, &events, center, started + age);
        assert_only_background(
            &frame,
            background,
            &format!("G5：缺整个 magic2 库时 age={age} 必须安全零落屏"),
        );
    }
    cleanup(&dir);

    // 3) 同快照其它技能照常：120 ThunderBolt 命中帧缺失 + 120 FireBall 旧 Magic 路径。
    let dir = unique_dir("g5-mixed");
    write_library(&dir, "magic", &MAIN_MAGIC);
    write_library(&dir, "magic2", &magic2_without(10));
    let mut scene = scene_in_dir(&dir);
    let mut snapshot = snapshot_with_self(Point::new(5, 5), MirDirection::Down);
    push_magic_via_wire(&mut snapshot, Spell::ThunderBolt, target, true, 0);
    push_magic_via_wire(&mut snapshot, Spell::FireBall, Point::new(12, 10), true, 0);
    assert_eq!(snapshot.spell_events.len(), 2);
    let mut frame = frame_with(FRAME_WIDTH, FRAME_HEIGHT, background);
    draw_at(&mut frame, &mut scene, &snapshot.spell_events, center, 0);
    let ops = [legacy_magic_op(center, Point::new(12, 10), 0)];
    assert_frame_matches(
        &frame,
        &oracle_frame(FRAME_WIDTH, FRAME_HEIGHT, background, &ops),
        "G5：ThunderBolt 命中帧缺失时同快照 FireBall 旧 Magic 路径照常绘制",
    );
    cleanup(&dir);
}
