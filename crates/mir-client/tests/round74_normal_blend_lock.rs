//! Round 74 A：普通 NORMAL（CPU `SourceAlpha/One`）跨图库 blend 锁（G1–G5）。
//!
//! 本文件只锁**现有**公开 CPU 行为，不修生产、不新增 rate/Mode API：
//!
//! * `FxKind::Magic` → `magic.wzl` 起始帧 0；`FxKind::Swing` → `magic2.wzl` 起始帧 0；
//!   `FxKind::Bolt` → `magic2.wzl` 起始帧 20（`crates/mir-render/src/entities.rs:497`
//!   区域的 `library`/`start_frame` 映射）；
//! * `EntityRenderer::draw_effect_blend` / `MapScene::draw_effect_blend` 的
//!   `apply_offset` 只叠加当帧 `ImageInfo` 的 i16 signed offset 一次，先 offset 后裁剪；
//!   返回值是「该帧是否存在」（缺帧 `false`，有图但完全屏外仍 `true`）；
//! * Add 混合 = `Frame::blit_add`：`out = min(255, dst + round(src * a / 255))`；
//!   旧入口 `draw_effect`（绝对坐标 Over）/ `draw_effect_with_offset`（偏移 Over）
//!   行为保持不变；
//! * G4 用公开 [`Frame::blit_add`] 直接喂 RGBA8 源像素（WZL 只有 alpha 0/255，
//!   无法表达半透明），锁 CPU rate1 的**未缩放**舍入，不伪造 rate0.8 调用。
//!
//! 独立 oracle：本文件内合成 `magic.wzl`/`magic.wzx` + `magic2.wzl`/`magic2.wzx`
//! （同索引异色、Magic2 帧 0/20/21 设诱饵、帧 27 哨兵造出 25 号缺帧洞），从原始
//! RGB565 像素按帧缓冲文档公式逐通道手算整帧 RGBA，**不调用**生产
//! `blit/blit_add/add_channel/blend_channel`，也不拿 `effect_image` 输出当期望。
//!
//! 运行（缺 `legacy` feature 时合成图库不可读，首条夹具断言以诊断文本失败而非 SKIP）：
//!
//! ```text
//! cargo test -p mir-client --features mir-render/legacy --test round74_normal_blend_lock -- --test-threads=1
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use mir_asset::format;
use mir_render::{
    EntityLibraries, EntityRenderer, Frame, FxKind, Image, ImageInfo, MapScene, TileLibraries,
    Viewport,
};

// ---------------------------------------------------------------------------
// 合成夹具帧表（RGB565；Magic 与 Magic2 同索引异色）
// ---------------------------------------------------------------------------

/// RGB565 原始值（解码口径：`(v >> 11) & 0x1F) << 3` 等，只左移不复制低位）。
const RGB_RED: u16 = 0xF800; // → [248, 0, 0, 255]
const RGB_GREEN: u16 = 0x07E0; // → [0, 252, 0, 255]
const RGB_BLUE: u16 = 0x001F; // → [0, 0, 248, 255]
const RGB_CYAN: u16 = 0x07FF; // → [0, 252, 248, 255]
const RGB_MAGENTA: u16 = 0xF81F; // → [248, 0, 248, 255]
const RGB_YELLOW: u16 = 0xFFE0; // → [248, 252, 0, 255]
const RGB_WHITEISH: u16 = 0xFFFF; // → [248, 252, 248, 255]
const RGB_BLACK: u16 = 0x0000; // 纯黑 → `remove_black_all` 置 alpha=0

/// 手算 RGBA（RGB565 展开后 `remove_black_all` 只对纯黑改 alpha）。
const RGBA_RED: [u8; 4] = [248, 0, 0, 255];
const RGBA_GREEN: [u8; 4] = [0, 252, 0, 255];
const RGBA_BLUE: [u8; 4] = [0, 0, 248, 255];

/// 2×2 四角原始值（行优先：左上、右上、左下、右下）。
static QUAD_RED_GREEN_BLUE_BLACK: [u16; 4] = [RGB_RED, RGB_GREEN, RGB_BLUE, RGB_BLACK];
static QUAD_GREEN_BLUE_RED_BLACK: [u16; 4] = [RGB_GREEN, RGB_BLUE, RGB_RED, RGB_BLACK];
static QUAD_BLUE_RED_GREEN_BLACK: [u16; 4] = [RGB_BLUE, RGB_RED, RGB_GREEN, RGB_BLACK];
static QUAD_BLUE_BLACK_GREEN_RED: [u16; 4] = [RGB_BLUE, RGB_BLACK, RGB_GREEN, RGB_RED];
static QUAD_CYAN_MAGENTA_YELLOW_WHITE: [u16; 4] = [RGB_CYAN, RGB_MAGENTA, RGB_YELLOW, RGB_WHITEISH];
static TRIPLE_RED_GREEN_BLUE: [u16; 3] = [RGB_RED, RGB_GREEN, RGB_BLUE];
static SINGLE_WHITEISH: [u16; 1] = [RGB_WHITEISH];

/// 一张合成帧：帧号、宽高、帧头 i16 signed offset、行优先 RGB565 像素。
#[derive(Clone, Copy)]
struct FxFrame {
    index: u32,
    width: u16,
    height: u16,
    offset: (i16, i16),
    pixels: &'static [u16],
}

impl FxFrame {
    const fn new(
        index: u32,
        width: u16,
        height: u16,
        offset: (i16, i16),
        pixels: &'static [u16],
    ) -> Self {
        Self {
            index,
            width,
            height,
            offset,
            pixels,
        }
    }
}

/// `magic.wzl`：G1/G3/G5 用帧 0；G2 用带 (-3,+2) 偏移的帧 1；G5 旧 Over 金样用帧 3。
const MAGIC_FRAMES: [FxFrame; 4] = [
    FxFrame::new(0, 2, 2, (0, 0), &QUAD_RED_GREEN_BLUE_BLACK),
    FxFrame::new(1, 2, 2, (-3, 2), &QUAD_BLUE_RED_GREEN_BLACK),
    FxFrame::new(2, 2, 2, (0, 0), &QUAD_RED_GREEN_BLUE_BLACK),
    FxFrame::new(3, 2, 2, (0, 0), &QUAD_CYAN_MAGENTA_YELLOW_WHITE),
];

/// `magic2.wzl`：帧 0（Swing 起点）/20（Bolt 起点，3×1 异尺寸）/21（Bolt 1）为诱饵；
/// 帧 27 是哨兵，把偏移表撑到 28 项，留下 25 号「表项 0 = 缺帧」洞（Bolt(5)）。
const MAGIC2_FRAMES: [FxFrame; 4] = [
    FxFrame::new(0, 2, 2, (0, 0), &QUAD_GREEN_BLUE_RED_BLACK),
    FxFrame::new(20, 3, 1, (4, -1), &TRIPLE_RED_GREEN_BLUE),
    FxFrame::new(21, 2, 2, (1, -2), &QUAD_BLUE_BLACK_GREEN_RED),
    FxFrame::new(27, 1, 1, (0, 0), &SINGLE_WHITEISH),
];

// ---------------------------------------------------------------------------
// 独立期望模型：kind → (图库帧, 落点) 的映射与整帧手算
// ---------------------------------------------------------------------------

/// 测试侧独立特效引用（不经过生产 `FxKind::library/start_frame`）。
#[derive(Clone, Copy, Debug)]
enum Fx {
    Magic(u32),
    Swing(u32),
    Bolt(u32),
}

/// 独立映射：Magic→`magic` 同号帧；Swing→`magic2` 同号帧；Bolt→`magic2` 第 `index + 20` 帧。
fn fx_frame(fx: Fx) -> &'static FxFrame {
    match fx {
        Fx::Magic(index) => find_frame(&MAGIC_FRAMES, index),
        Fx::Swing(index) => find_frame(&MAGIC2_FRAMES, index),
        Fx::Bolt(index) => find_frame(&MAGIC2_FRAMES, index + 20),
    }
}

/// 生产入口参数（与独立期望模型一一对应，供两侧同时驱动）。
fn fx_kind(fx: Fx) -> (FxKind, u32) {
    match fx {
        Fx::Magic(index) => (FxKind::Magic, index),
        Fx::Swing(index) => (FxKind::Swing, index),
        Fx::Bolt(index) => (FxKind::Bolt, index),
    }
}

/// 帧表查询（缺帧号直接 panic，防止夹具与断言漂移）。
fn find_frame(frames: &'static [FxFrame], index: u32) -> &'static FxFrame {
    frames
        .iter()
        .find(|frame| frame.index == index)
        .unwrap_or_else(|| panic!("合成图库必须覆盖帧号 {index}"))
}

/// RGB565 → RGBA 的独立展开（Crystal `convert16bitTo32bit`：只左移，不复制低位）。
fn expand565(value: u16) -> [u8; 4] {
    let red = ((value >> 11) & 0x1F) << 3;
    let green = ((value >> 5) & 0x3F) << 2;
    let blue = (value & 0x1F) << 3;
    [red as u8, green as u8, blue as u8, 255]
}

/// 解码后逐像素 alpha：`remove_black_all` 把 RGB 全 0 置 alpha=0，其它 255（WZL 无 alpha）。
fn fixture_alpha(color: [u8; 4]) -> u8 {
    if color[0] == 0 && color[1] == 0 && color[2] == 0 {
        0
    } else {
        255
    }
}

/// Add 单通道：`min(255, dst + (src * a + 127) / 255)`（四舍五入、饱和）。
fn oracle_add_channel(destination: u8, source: u8, alpha: u8) -> u8 {
    let contribution = (u32::from(source) * u32::from(alpha) + 127) / 255;
    u8::try_from((u32::from(destination) + contribution).min(255)).unwrap_or(255)
}

/// Over 单通道：`(src * a + dst * (255 - a) + 127) / 255`。
fn oracle_over_channel(destination: u8, source: u8, alpha: u8) -> u8 {
    let alpha = u32::from(alpha);
    let source = u32::from(source);
    let destination = u32::from(destination);
    u8::try_from((source * alpha + destination * (255 - alpha) + 127) / 255).unwrap_or(255)
}

/// 混合方式。
#[derive(Clone, Copy, Debug)]
enum Mode {
    Add,
    Over,
}

/// 一次绘制：独立特效引用 + 锚点 + 是否消费当帧 offset + 混合方式。
#[derive(Clone, Copy, Debug)]
struct Op {
    fx: Fx,
    x: i32,
    y: i32,
    apply_offset: bool,
    mode: Mode,
}

impl Op {
    const fn add(fx: Fx, x: i32, y: i32, apply_offset: bool) -> Self {
        Self {
            fx,
            x,
            y,
            apply_offset,
            mode: Mode::Add,
        }
    }

    const fn over(fx: Fx, x: i32, y: i32, apply_offset: bool) -> Self {
        Self {
            fx,
            x,
            y,
            apply_offset,
            mode: Mode::Over,
        }
    }
}

/// 独立整帧 oracle：从 `background` 起按 `ops` 顺序逐像素手算整帧 RGBA。
fn oracle_frame(width: u32, height: u32, background: [u8; 4], ops: &[Op]) -> Vec<u8> {
    let width_usize = usize::try_from(width).expect("oracle 宽");
    let height_usize = usize::try_from(height).expect("oracle 高");
    let mut pixels = vec![0u8; width_usize * height_usize * 4];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&background);
    }
    for op in ops {
        let spec = fx_frame(op.fx);
        // 偏移先于裁剪；`saturating_add` 与入口收口语义一致。
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
        let spec_width = usize::from(spec.width);
        for (slot, &raw) in spec.pixels.iter().enumerate() {
            let source = expand565(raw);
            let alpha = fixture_alpha(source);
            if alpha == 0 {
                // alpha=0：整点跳过，不动 RGB，也不动目标 alpha 字节。
                continue;
            }
            let x = i64::from(left) + i64::try_from(slot % spec_width).expect("列");
            let y = i64::from(top) + i64::try_from(slot / spec_width).expect("行");
            if x < 0 || y < 0 || x >= i64::from(width) || y >= i64::from(height) {
                continue;
            }
            let offset =
                (usize::try_from(y).expect("y") * width_usize + usize::try_from(x).expect("x")) * 4;
            let target = &mut pixels[offset..offset + 4];
            for channel in 0..3 {
                target[channel] = match op.mode {
                    Mode::Over => oracle_over_channel(target[channel], source[channel], alpha),
                    Mode::Add => oracle_add_channel(target[channel], source[channel], alpha),
                };
            }
            target[3] = 255;
        }
    }
    pixels
}

/// RGBA8 源图（G4 用；不经过 WZL 链，直接走公开 `Frame::blit_add`）。
fn rgba_image(width: u16, height: u16, color: [u8; 4]) -> Image {
    let pixels =
        [color[0], color[1], color[2], color[3]].repeat(usize::from(width) * usize::from(height));
    Image {
        info: ImageInfo {
            width,
            height,
            offset_x: 0,
            offset_y: 0,
            format: format::RGBA32_GZIP,
            shadow_x: 0,
            shadow_y: 0,
            reserved: [0; 3],
            has_mask: false,
            data_offset: 0,
            data_len: 0,
        },
        pixels,
    }
}

/// G4 独立 oracle：把 RGBA8 源按 Add 公式（四舍五入、饱和、a=0 跳过）贴到背景。
fn oracle_add_rgba_frame(
    width: u32,
    height: u32,
    background: [u8; 4],
    image: &Image,
    x: i32,
    y: i32,
) -> Vec<u8> {
    let width_usize = usize::try_from(width).expect("oracle 宽");
    let height_usize = usize::try_from(height).expect("oracle 高");
    let mut pixels = vec![0u8; width_usize * height_usize * 4];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&background);
    }
    let source_width = u32::from(image.info.width);
    for (slot, source) in image.pixels.chunks_exact(4).enumerate() {
        let alpha = source[3];
        if alpha == 0 {
            continue;
        }
        let offset_x = i64::try_from(slot).expect("slot") % i64::from(source_width);
        let offset_y = i64::try_from(slot).expect("slot") / i64::from(source_width);
        let x = i64::from(x) + offset_x;
        let y = i64::from(y) + offset_y;
        if x < 0 || y < 0 || x >= i64::from(width) || y >= i64::from(height) {
            continue;
        }
        let offset =
            (usize::try_from(y).expect("y") * width_usize + usize::try_from(x).expect("x")) * 4;
        let target = &mut pixels[offset..offset + 4];
        for channel in 0..3 {
            target[channel] = oracle_add_channel(target[channel], source[channel], alpha);
        }
        target[3] = 255;
    }
    pixels
}

/// 帧表逐帧的独立期望像素（供显式常量断言）。
fn expected_source(fx: Fx, col: usize, row: usize) -> [u8; 4] {
    let spec = fx_frame(fx);
    let width = usize::from(spec.width);
    let raw = spec.pixels[row * width + col];
    let source = expand565(raw);
    let alpha = fixture_alpha(source);
    [source[0], source[1], source[2], alpha]
}

/// Add 命中像素的独立期望：源 alpha=0 时保留背景，否则
/// `bg + round(src * a / 255)`（饱和），目标 alpha 写回 255。
fn expected_add(background: [u8; 4], source: [u8; 4]) -> [u8; 4] {
    let alpha = source[3];
    if alpha == 0 {
        return background;
    }
    [
        oracle_add_channel(background[0], source[0], alpha),
        oracle_add_channel(background[1], source[1], alpha),
        oracle_add_channel(background[2], source[2], alpha),
        255,
    ]
}

// ---------------------------------------------------------------------------
// 夹具：进程唯一临时目录 + 合成 WZL/WZX
// ---------------------------------------------------------------------------

/// 进程唯一临时目录（不读 `resource`/`game-assets`，也不与其它测试共享路径）。
fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let name = format!("mir-next-r74-a-{tag}-{}-{nanos}", std::process::id());
    let candidates = [
        std::env::temp_dir().join(&name),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/round74/a/tmp")
            .join(&name),
    ];
    for dir in candidates {
        if fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }
    panic!("无法创建进程唯一临时目录（系统临时目录与 target/round74/a/tmp 均不可写）");
}

/// 写一个合成图库（`{stem}.wzl` + `{stem}.wzx`）。
fn write_library(directory: &Path, stem: &str, frames: &[FxFrame]) {
    let (wzl, wzx) = build_wzl(frames);
    fs::write(directory.join(format!("{stem}.wzl")), wzl).expect("写合成 wzl");
    fs::write(directory.join(format!("{stem}.wzx")), wzx).expect("写合成 wzx");
}

/// 写 `magic` + `magic2` 两个合成图库并返回目录。
fn write_libraries(tag: &str) -> PathBuf {
    let dir = unique_dir(tag);
    write_library(&dir, "magic", &MAGIC_FRAMES);
    write_library(&dir, "magic2", &MAGIC2_FRAMES);
    dir
}

/// 用目录内实体图库构造 `EntityRenderer`。
fn renderer_in_dir(dir: &Path) -> EntityRenderer {
    EntityRenderer::new(EntityLibraries::open_dir(dir))
}

/// 用空 32×32 地图 + 空瓦片库 + 目录内实体图库构造 `MapScene`（`Viewport::DEFAULT`）。
fn scene_in_dir(dir: &Path) -> MapScene {
    let map = mir_world::Map {
        width: 32,
        height: 32,
        cells: vec![mir_world::MapCell::default(); 32 * 32],
        file_name: "r74-empty".to_owned(),
        version: mir_world::MapVersion::V0,
        doors: Vec::new(),
    };
    MapScene::from_parts_with_entities(
        map,
        TileLibraries::empty("."),
        Viewport::DEFAULT,
        EntityLibraries::open_dir(dir),
    )
}

/// 按 `.wzl`/`.wzx` 二进制格式构造一对文件内容。
///
/// `.wzl`：48 字节头 + 每条记录 16 字节记录头（格式 5=RGB565、宽高、i16 偏移、
/// u32 zlib 长度）+ zlib 数据；`.wzx`：48 字节头（[44..48] 数量）+ u32 偏移表。
/// 行序按 Crystal 自下而上：文件第 0 行写图像最下面一行。
fn build_wzl(frames: &[FxFrame]) -> (Vec<u8>, Vec<u8>) {
    let count = frames
        .iter()
        .map(|frame| frame.index)
        .max()
        .expect("合成帧表不能为空")
        + 1;
    let mut wzl = vec![0u8; 48];
    let mut table = vec![0u32; usize::try_from(count).expect("帧数必须放得进 usize")];
    for spec in frames {
        let width = usize::from(spec.width);
        let height = usize::from(spec.height);
        assert_eq!(spec.pixels.len(), width * height, "夹具像素数必须等于宽×高");
        let mut raw = Vec::with_capacity(width * height * 2);
        // 自下而上：文件第 0 行 = 图像最下面一行；每行按 4 字节对齐
        //（`WidthBytes = (((width * bits) + 31) >> 5) * 4`，补齐字节为 0）。
        let row_bytes = (width * 2).div_ceil(4) * 4;
        for row in (0..height).rev() {
            let start = row * width;
            let before = raw.len();
            for &value in &spec.pixels[start..start + width] {
                raw.extend_from_slice(&value.to_le_bytes());
            }
            raw.resize(before + row_bytes, 0);
        }
        let compressed = zlib_store(&raw);
        table[usize::try_from(spec.index).expect("帧号必须放得进 usize")] =
            u32::try_from(wzl.len()).expect(".wzl 偏移必须放得进 u32");
        wzl.push(5); // 格式 5 = RGB565
        wzl.extend_from_slice(&[0, 0, 0]); // Crystal 直接跳过的 3 个保留字节
        wzl.extend_from_slice(&spec.width.to_le_bytes());
        wzl.extend_from_slice(&spec.height.to_le_bytes());
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

/// 删除进程唯一临时目录（失败不影响断言结论）。
fn cleanup(dir: &Path) {
    let _ = fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// 帧 / 断言辅助
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

/// 整帧断言：失败时报告首个差异像素的坐标/实际值/期望值（`#[track_caller]` 让首败
/// 行号落在调用测试行，作为变异红的「首个失败断言」证据）。
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

/// 用生产 `EntityRenderer` 播放一次独立 op。
fn play(renderer: &mut EntityRenderer, frame: &mut Frame, op: Op) -> bool {
    let (kind, index) = fx_kind(op.fx);
    match (op.mode, op.apply_offset) {
        (Mode::Add, apply_offset) => {
            renderer.draw_effect_blend(frame, kind, index, op.x, op.y, apply_offset)
        }
        (Mode::Over, false) => renderer.draw_effect(frame, kind, index, op.x, op.y),
        (Mode::Over, true) => renderer.draw_effect_with_offset(frame, kind, index, op.x, op.y),
    }
}

/// 用生产 `MapScene` 播放一次独立 op（薄转发）。
fn play_scene(scene: &mut MapScene, frame: &mut Frame, op: Op) -> bool {
    let (kind, index) = fx_kind(op.fx);
    match (op.mode, op.apply_offset) {
        (Mode::Add, apply_offset) => {
            scene.draw_effect_blend(frame, kind, index, op.x, op.y, apply_offset)
        }
        (Mode::Over, false) => scene.draw_effect(frame, kind, index, op.x, op.y),
        (Mode::Over, true) => scene.draw_effect_with_offset(frame, kind, index, op.x, op.y),
    }
}

// ---------------------------------------------------------------------------
// G1：跨图库起点映射（Magic0 / Magic2[0] / Magic2[20] / Magic2[21]）
// ---------------------------------------------------------------------------

/// G1：四个 kind/index 读到正确图库与起始帧；同 scene 与分离 renderer 逐像素相等，
/// 整帧等于独立 oracle（错误图库/错误起点在首败像素暴露）。
#[test]
fn r74_g1_cross_library_start_indices() {
    let dir = write_libraries("g1");
    let mut scene = scene_in_dir(&dir);
    let mut renderer = renderer_in_dir(&dir);
    let background = [5, 5, 5, 255];

    // (独立期望, kind, index, 锚点)：锚点各不相同，覆盖 2×2、3×1、带偏移 2×2。
    let cases = [
        (Fx::Magic(0), 2, 3),
        (Fx::Swing(0), 7, 3),
        (Fx::Bolt(0), 12, 3),
        (Fx::Bolt(1), 18, 3),
    ];
    for (frame_index, (fx, x, y)) in cases.into_iter().enumerate() {
        let spec = fx_frame(fx);
        let expected_size = (u32::from(spec.width), u32::from(spec.height));
        let (kind, index) = fx_kind(fx);

        // 读取链先自证：尺寸来自正确图库/帧（错库/错起点会先在这里红）。
        assert_eq!(
            scene.effect_size(kind, index),
            Some(expected_size),
            "G1：{fx:?} 尺寸必须是 {expected_size:?}"
        );
        assert_eq!(
            renderer.effect_size(kind, index),
            Some(expected_size),
            "G1：renderer 视角 {fx:?} 尺寸必须是 {expected_size:?}"
        );

        let mut scene_frame = frame_with(24, 8, background);
        let mut renderer_frame = frame_with(24, 8, background);
        assert!(
            play_scene(&mut scene, &mut scene_frame, Op::add(fx, x, y, true)),
            "G1：scene 有图必须返回 true（{fx:?}）"
        );
        assert!(
            play(&mut renderer, &mut renderer_frame, Op::add(fx, x, y, true)),
            "G1：renderer 有图必须返回 true（{fx:?}）"
        );
        assert_eq!(
            scene_frame.pixels, renderer_frame.pixels,
            "G1：MapScene 薄转发必须与分离 EntityRenderer 逐像素相等（{fx:?}）"
        );
        let expected = oracle_frame(24, 8, background, &[Op::add(fx, x, y, true)]);
        assert_frame_matches(
            &scene_frame,
            &expected,
            &format!("G1：{fx:?} 整帧必须等于独立 oracle（帧序 {frame_index}）"),
        );

        // 手算首批像素：起点错位/错库都会在不同颜色上暴露。
        let left = x + i32::from(spec.offset.0);
        let top = y + i32::from(spec.offset.1);
        assert_eq!(
            pixel_at(&scene_frame, left, top),
            expected_add(background, expected_source(fx, 0, 0)),
            "G1：{fx:?} 左上角像素"
        );
        assert_eq!(
            pixel_at(&scene_frame, left + 1, top),
            expected_add(background, expected_source(fx, 1, 0)),
            "G1：{fx:?} 第二列像素"
        );
    }

    // 显式颜色常量：锁定「magic0=红，magic2[0]=绿，magic2[20]=红(3×1)，magic2[21]=蓝」。
    // Add 落地值 = 背景 + 源（alpha=255），所以颜色断言都按 expected_add 展开。
    let added = |source: [u8; 4]| expected_add(background, source);
    let mut checks = frame_with(24, 8, background);
    assert!(play_scene(
        &mut scene,
        &mut checks,
        Op::add(Fx::Magic(0), 2, 3, true)
    ));
    assert_eq!(
        pixel_at(&checks, 2, 3),
        added(RGBA_RED),
        "G1：Magic(0) 左上必须是 magic[0] 红"
    );
    assert_eq!(
        pixel_at(&checks, 3, 3),
        added(RGBA_GREEN),
        "G1：Magic(0) 右上必须是绿"
    );
    assert_eq!(
        pixel_at(&checks, 2, 4),
        added(RGBA_BLUE),
        "G1：Magic(0) 左下必须是蓝"
    );
    assert_eq!(
        pixel_at(&checks, 3, 4),
        background,
        "G1：Magic(0) 右下纯黑像素 alpha=0 不得落屏"
    );

    let mut swing = frame_with(24, 8, background);
    assert!(play_scene(
        &mut scene,
        &mut swing,
        Op::add(Fx::Swing(0), 7, 3, true)
    ));
    assert_eq!(
        pixel_at(&swing, 7, 3),
        added(RGBA_GREEN),
        "G1：Swing(0) 必须读 magic2[0] 绿"
    );
    assert_eq!(
        pixel_at(&swing, 8, 3),
        added(RGBA_BLUE),
        "G1：Swing(0) 右上必须是蓝"
    );
    assert_eq!(
        pixel_at(&swing, 7, 4),
        added(RGBA_RED),
        "G1：Swing(0) 左下必须是红"
    );

    let mut bolt = frame_with(24, 8, background);
    assert!(play_scene(
        &mut scene,
        &mut bolt,
        Op::add(Fx::Bolt(0), 12, 3, true)
    ));
    assert_eq!(
        pixel_at(&bolt, 16, 2),
        added(RGBA_RED),
        "G1：Bolt(0) 必须读 magic2[20] 红"
    );
    assert_eq!(
        pixel_at(&bolt, 17, 2),
        added(RGBA_GREEN),
        "G1：Bolt(0) 中间必须是绿"
    );
    assert_eq!(
        pixel_at(&bolt, 18, 2),
        added(RGBA_BLUE),
        "G1：Bolt(0) 右端必须是蓝"
    );
    assert_eq!(
        pixel_at(&bolt, 16, 3),
        background,
        "G1：Bolt(0) 3×1 高度不得溢出"
    );

    let mut bolt1 = frame_with(24, 8, background);
    assert!(play_scene(
        &mut scene,
        &mut bolt1,
        Op::add(Fx::Bolt(1), 18, 3, true)
    ));
    assert_eq!(
        pixel_at(&bolt1, 19, 1),
        added(RGBA_BLUE),
        "G1：Bolt(1) 必须读 magic2[21] 蓝"
    );
    assert_eq!(
        pixel_at(&bolt1, 19, 2),
        added(RGBA_GREEN),
        "G1：Bolt(1) 左下必须是绿"
    );
    assert_eq!(
        pixel_at(&bolt1, 20, 2),
        added(RGBA_RED),
        "G1：Bolt(1) 右下必须是红"
    );
    assert_eq!(
        pixel_at(&bolt1, 20, 1),
        background,
        "G1：Bolt(1) 右上黑像素不得落屏"
    );

    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G2：跨图库 signed offset、offset 先于裁剪、缺帧 false
// ---------------------------------------------------------------------------

/// G2：两库同帧不同尺寸、offset(-3,2)/(4,-1) 的 true/false 语义；屏外锚点经 offset
/// 移入、完全屏外仍 true、缺帧 false；落点手算整帧。
#[test]
fn r74_g2_cross_library_signed_offsets() {
    let dir = write_libraries("g2");
    let mut renderer = renderer_in_dir(&dir);
    let background = [5, 6, 7, 255];

    // ① 同帧两库：Magic(1) 2×2 offset(-3,+2) 与 Bolt(0) 3×1 offset(+4,-1) 各画一次。
    let combined_ops = [
        Op::add(Fx::Magic(1), 10, 10, true),
        Op::add(Fx::Bolt(0), 10, 10, true),
    ];
    let mut combined = frame_with(32, 32, background);
    for op in combined_ops {
        assert!(
            play(&mut renderer, &mut combined, op),
            "G2：两库绘制都必须返回 true"
        );
    }
    assert_frame_matches(
        &combined,
        &oracle_frame(32, 32, background, &combined_ops),
        "G2：两库同帧不同尺寸 + 各自 signed offset 整帧",
    );
    // magic[1] offset(-3,2) → 左上 (7,12)；magic2[20] offset(4,-1) → 左上 (14,9)。
    let add = |source: [u8; 4]| expected_add(background, source);
    assert_eq!(
        pixel_at(&combined, 7, 12),
        add(RGBA_BLUE),
        "G2：Magic(1) 左上必须是蓝"
    );
    assert_eq!(
        pixel_at(&combined, 8, 12),
        add(RGBA_RED),
        "G2：Magic(1) 右上必须是红"
    );
    assert_eq!(
        pixel_at(&combined, 7, 13),
        add(RGBA_GREEN),
        "G2：Magic(1) 左下必须是绿"
    );
    assert_eq!(
        pixel_at(&combined, 8, 13),
        background,
        "G2：Magic(1) 右下黑像素不得落屏"
    );
    assert_eq!(
        pixel_at(&combined, 14, 9),
        add(RGBA_RED),
        "G2：Bolt(0) 左上必须是红"
    );
    assert_eq!(
        pixel_at(&combined, 15, 9),
        add(RGBA_GREEN),
        "G2：Bolt(0) 中间必须是绿"
    );
    assert_eq!(
        pixel_at(&combined, 16, 9),
        add(RGBA_BLUE),
        "G2：Bolt(0) 右端必须是蓝"
    );

    // ② apply_offset=false：绝对左上角，不消费帧头 offset。
    let absolute_ops = [
        Op::add(Fx::Magic(1), 10, 10, false),
        Op::add(Fx::Bolt(0), 10, 10, false),
    ];
    let mut absolute = frame_with(32, 32, background);
    for op in absolute_ops {
        assert!(
            play(&mut renderer, &mut absolute, op),
            "G2：绝对落点绘制必须返回 true"
        );
    }
    assert_frame_matches(
        &absolute,
        &oracle_frame(32, 32, background, &absolute_ops),
        "G2：apply_offset=false 必须绝对落点整帧",
    );
    // false 时 magic[1] 与 magic2[20] 都从 (10,10) 起：Bolt(0) 最后覆盖 (10,10)，
    // 所以该像素是两次 Add 叠加（先蓝后红）。
    assert_eq!(
        pixel_at(&absolute, 10, 10),
        expected_add(expected_add(background, RGBA_BLUE), RGBA_RED),
        "G2：false 时 Magic(1) 左上在原点且被 Bolt(0) 叠加"
    );
    assert_eq!(
        pixel_at(&absolute, 7, 12),
        background,
        "G2：false 时不得消费 (-3,2)"
    );
    assert_ne!(
        combined.pixels, absolute.pixels,
        "G2：true 与 false 的整帧落点必须不同（负 offset 被忽略会在此首败）"
    );

    // ③ 屏外锚点经 offset 移入：Magic(1) 锚点 (5,-2) 在屏外，offset(-3,+2) 后左上 (2,0)。
    let mut moved_in = frame_with(32, 32, background);
    assert!(
        play(
            &mut renderer,
            &mut moved_in,
            Op::add(Fx::Magic(1), 5, -2, true)
        ),
        "G2：屏外锚点经 offset 移入仍必须有图 → true"
    );
    assert_frame_matches(
        &moved_in,
        &oracle_frame(32, 32, background, &[Op::add(Fx::Magic(1), 5, -2, true)]),
        "G2：offset 必须先于裁剪（屏外锚点移入）",
    );
    assert_eq!(
        pixel_at(&moved_in, 2, 0),
        add(RGBA_BLUE),
        "G2：移入后左上必须落在 (2,0)"
    );
    assert_eq!(
        pixel_at(&moved_in, 3, 0),
        add(RGBA_RED),
        "G2：移入后右上必须落在 (3,0)"
    );

    // ④ 完全屏外：有图仍 true，零落屏。
    let mut offscreen = frame_with(32, 32, background);
    assert!(
        play(
            &mut renderer,
            &mut offscreen,
            Op::add(Fx::Magic(1), -100, -100, true)
        ),
        "G2：有图但完全屏外 → 仍返回 true（存在性语义）"
    );
    assert_only_background(&offscreen, background, "G2：完全屏外不得落像素");

    // ⑤ i32 极值不 panic，零落屏。
    let mut extreme = frame_with(32, 32, background);
    assert!(
        play(
            &mut renderer,
            &mut extreme,
            Op::add(Fx::Magic(1), i32::MAX, i32::MIN, true)
        ),
        "G2：i32 极值 + offset 不得 panic，有图 → true"
    );
    assert!(
        play(
            &mut renderer,
            &mut extreme,
            Op::add(Fx::Bolt(0), i32::MIN, i32::MAX, true)
        ),
        "G2：负向 i32 极值不得 panic，有图 → true"
    );
    assert_only_background(&extreme, background, "G2：i32 极值不得落像素");

    // ⑥ 缺帧 false：Bolt(5) → magic2[25] 偏移表项 0（洞）；Magic(900) 越界。
    let mut missing = frame_with(32, 32, background);
    assert!(
        !renderer.draw_effect_blend(&mut missing, FxKind::Bolt, 5, 3, 3, true),
        "G2：magic2[25] 缺帧（表项 0）→ false"
    );
    assert!(
        !renderer.draw_effect_with_offset(&mut missing, FxKind::Bolt, 5, 3, 3),
        "G2：缺帧经旧 with_offset → false"
    );
    assert!(
        !renderer.draw_effect(&mut missing, FxKind::Magic, 900, 3, 3),
        "G2：magic[900] 越界缺图 → false"
    );
    assert_only_background(&missing, background, "G2：缺帧不得画占位方块/回退");
    assert_eq!(
        renderer.effect_size(FxKind::Bolt, 5),
        None,
        "G2：magic2[25] 缺帧 → 无尺寸"
    );

    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G3：跨图库 Add/Over 同像素交替序列
// ---------------------------------------------------------------------------

/// G3：同像素 Magic Add→Magic2 Add→Magic Over→Magic2 Add，每步整帧对独立 oracle；
/// 重复绘制不污染缓存 offset。
#[test]
fn r74_g3_cross_library_add_over_sequence() {
    let dir = write_libraries("g3");
    let mut renderer = renderer_in_dir(&dir);
    let background = [40, 60, 80, 255];
    let ops = [
        Op::add(Fx::Magic(2), 1, 1, true),
        Op::add(Fx::Swing(0), 1, 1, true),
        Op::over(Fx::Magic(2), 1, 1, false),
        Op::add(Fx::Swing(0), 1, 1, true),
    ];

    let mut frame = frame_with(32, 32, background);
    for (step, op) in ops.iter().enumerate() {
        assert!(
            play(&mut renderer, &mut frame, *op),
            "G3：第 {step} 步有图必须返回 true"
        );
        assert_frame_matches(
            &frame,
            &oracle_frame(32, 32, background, &ops[..=step]),
            &format!("G3：第 {step} 步整帧必须等于独立 oracle"),
        );
    }

    // 逐步显式常量（同一 2×2 像素上的 RGB565 红/绿/蓝/透明黑）。
    let mut step1 = frame_with(32, 32, background);
    assert!(play(
        &mut renderer,
        &mut step1,
        Op::add(Fx::Magic(2), 1, 1, true)
    ));
    assert_eq!(
        pixel_at(&step1, 1, 1),
        [255, 60, 80, 255],
        "G3：Add 红通道饱和 255"
    );
    assert_eq!(
        pixel_at(&step1, 2, 1),
        [40, 255, 80, 255],
        "G3：Add 绿通道饱和 255"
    );
    assert_eq!(
        pixel_at(&step1, 1, 2),
        [40, 60, 255, 255],
        "G3：Add 蓝通道饱和 255"
    );
    assert_eq!(
        pixel_at(&step1, 2, 2),
        background,
        "G3：透明黑像素 Add 跳过"
    );

    let mut step2 = frame_with(32, 32, background);
    for op in &ops[..2] {
        assert!(play(&mut renderer, &mut step2, *op));
    }
    assert_eq!(
        pixel_at(&step2, 1, 1),
        [255, 255, 80, 255],
        "G3：跨库 Add 第二次叠加"
    );
    assert_eq!(
        pixel_at(&step2, 2, 1),
        [40, 255, 255, 255],
        "G3：跨库 Add 蓝绿饱和"
    );
    assert_eq!(
        pixel_at(&step2, 1, 2),
        [255, 60, 255, 255],
        "G3：跨库 Add 红蓝饱和"
    );

    let mut step3 = frame_with(32, 32, background);
    for op in &ops[..3] {
        assert!(play(&mut renderer, &mut step3, *op));
    }
    assert_eq!(
        pixel_at(&step3, 1, 1),
        [248, 0, 0, 255],
        "G3：Over(alpha=255) 覆盖为源红"
    );
    assert_eq!(
        pixel_at(&step3, 2, 1),
        [0, 252, 0, 255],
        "G3：Over 覆盖为源绿"
    );
    assert_eq!(
        pixel_at(&step3, 1, 2),
        [0, 0, 248, 255],
        "G3：Over 覆盖为源蓝"
    );
    assert_eq!(
        pixel_at(&step3, 2, 2),
        background,
        "G3：Over 下透明黑像素仍跳过"
    );
    assert_ne!(
        step1.pixels, step3.pixels,
        "G3：Add→Over 的整帧必须不同（Add 被改成 Over 在 step1 首败）"
    );

    let mut step4 = frame_with(32, 32, background);
    for op in &ops[0..4] {
        assert!(play(&mut renderer, &mut step4, *op));
    }
    assert_eq!(
        pixel_at(&step4, 1, 1),
        [248, 252, 0, 255],
        "G3：Over 后再 Add 绿"
    );
    assert_eq!(
        pixel_at(&step4, 2, 1),
        [0, 252, 248, 255],
        "G3：Over 后再 Add 蓝"
    );
    assert_eq!(
        pixel_at(&step4, 1, 2),
        [248, 0, 248, 255],
        "G3：Over 后再 Add 红"
    );

    // 重复两轮 + 之后带 offset 的 Magic(1)：整帧仍等于 oracle，证明 offset 不被污染。
    let mut twice = frame_with(32, 32, background);
    for _ in 0..2 {
        for op in &ops {
            assert!(play(&mut renderer, &mut twice, *op));
        }
    }
    let twice_with_offset = twice.pixels.clone();
    let mut doubled_ops: Vec<Op> = ops.to_vec();
    doubled_ops.extend_from_slice(&ops);
    assert_frame_matches(
        &twice,
        &oracle_frame(32, 32, background, &doubled_ops),
        "G3：同帧重复两轮 Add/Over 序列整帧",
    );
    assert!(play(
        &mut renderer,
        &mut twice,
        Op::add(Fx::Magic(1), 10, 10, true)
    ));
    doubled_ops.push(Op::add(Fx::Magic(1), 10, 10, true));
    assert_frame_matches(
        &twice,
        &oracle_frame(32, 32, background, &doubled_ops),
        "G3：重复绘制后 Magic(1) 的 (-3,+2) offset 仍须照常生效",
    );
    assert_eq!(
        pixel_at(&twice, 7, 12),
        expected_add(background, RGBA_BLUE),
        "G3：重复序列后 offset(-3,+2) 仍落在 (7,12)"
    );

    // 缓存 metadata 不被任何入口改写（帧头 signed offset 原样保留）。
    for (fx, expected) in [
        (Fx::Magic(1), (-3i16, 2i16)),
        (Fx::Bolt(0), (4, -1)),
        (Fx::Bolt(1), (1, -2)),
    ] {
        let (kind, index) = fx_kind(fx);
        let info = renderer
            .effect_image(kind, index)
            .expect("G3：缓存 meta 仍应可读")
            .info;
        assert_eq!(
            (info.offset_x, info.offset_y),
            expected,
            "G3：重复绘制不得改写缓存帧头 offset（{fx:?}）"
        );
    }
    assert_ne!(
        twice.pixels, twice_with_offset,
        "G3：追加一帧后整帧必须变化（证明偏移帧确实落屏）"
    );

    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G4：CPU rate1 的未缩放 alpha 舍入（RGBA8 源，非黑/饱和底）
// ---------------------------------------------------------------------------

/// G4：`Frame::blit_add` 对 RGBA 源 `[101,53,7,a]` 按 rate=1 未缩放公式混合；
/// a=128 时底 `[10,20,30,255]` → `[61,47,34,255]`，并与 src×0.8 模型不同。
#[test]
fn r74_g4_cpu_alpha_rounding_is_unscaled() {
    // (alpha, 非黑底 [10,20,30] 上的手算期望)
    let cases: [(u8, [u8; 4]); 6] = [
        (0, [10, 20, 30, 255]),
        (1, [10, 20, 30, 255]),
        (127, [60, 46, 33, 255]),
        (128, [61, 47, 34, 255]),
        (254, [111, 73, 37, 255]),
        (255, [111, 73, 37, 255]),
    ];
    for (alpha, expected) in cases {
        let image = rgba_image(2, 2, [101, 53, 7, alpha]);
        let mut frame = frame_with(16, 16, [10, 20, 30, 255]);
        frame.blit_add(&image, 3, 4);
        assert_frame_matches(
            &frame,
            &oracle_add_rgba_frame(16, 16, [10, 20, 30, 255], &image, 3, 4),
            &format!("G4：alpha={alpha} 的 Add 整帧必须等于独立 oracle"),
        );
        for (x, y) in [(3, 4), (4, 4), (3, 5), (4, 5)] {
            assert_eq!(
                pixel_at(&frame, x, y),
                expected,
                "G4：alpha={alpha} 的非黑底期望（四舍五入未缩放）"
            );
        }
        assert_eq!(
            pixel_at(&frame, 0, 0),
            [10, 20, 30, 255],
            "G4：alpha={alpha} 未覆盖像素必须保持背景"
        );
    }

    // a=128 显式常量 + 与 src×0.8 模型（rate0.8）对照：rate1 是未缩放。
    let image = rgba_image(2, 2, [101, 53, 7, 128]);
    let mut frame = frame_with(16, 16, [10, 20, 30, 255]);
    frame.blit_add(&image, 3, 4);
    assert_eq!(
        pixel_at(&frame, 3, 4),
        [61, 47, 34, 255],
        "G4：a=128 底 [10,20,30] 必须是 [61,47,34]（rate1 未缩放）"
    );
    // src×0.8 模型：round(101*0.8)=81、round(53*0.8)=42、round(7*0.8)=6 →
    // [10+41, 20+21, 30+3] = [51,41,33]，与 rate1 明显不同。
    let rate08_model = [51u8, 41, 33, 255];
    assert_ne!(
        pixel_at(&frame, 3, 4),
        rate08_model,
        "G4：CPU Add 不得乘 rate0.8（否则首败于此）"
    );
    let mut full = frame_with(16, 16, [10, 20, 30, 255]);
    full.blit_add(&rgba_image(2, 2, [101, 53, 7, 255]), 3, 4);
    assert_eq!(
        pixel_at(&full, 3, 4),
        [111, 73, 37, 255],
        "G4：a=255 必须是完整未缩放相加 [111,73,37]"
    );
    assert_ne!(
        pixel_at(&full, 3, 4),
        [91u8, 62, 36, 255],
        "G4：a=255 也不得乘 rate0.8（模型 [91,62,36]）"
    );

    // 饱和底：饱和到 255，且不产生溢出回绕。
    let saturated = [250u8, 240, 230, 255];
    let mut saturated_frame = frame_with(16, 16, saturated);
    saturated_frame.blit_add(&rgba_image(2, 2, [101, 53, 7, 128]), 3, 4);
    assert_eq!(
        pixel_at(&saturated_frame, 3, 4),
        [255, 255, 234, 255],
        "G4：饱和底 [250,240,230] + a=128 → [255,255,234]"
    );
    assert_eq!(
        pixel_at(&saturated_frame, 3, 4),
        [255, 255, 234, 255],
        "G4：饱和底不得回绕"
    );
    let mut saturated_full = frame_with(16, 16, saturated);
    saturated_full.blit_add(&rgba_image(2, 2, [101, 53, 7, 255]), 3, 4);
    assert_eq!(
        pixel_at(&saturated_full, 3, 4),
        [255, 255, 237, 255],
        "G4：饱和底 + a=255 → [255,255,237]"
    );

    // a=0 整点跳过：目标 RGB/alpha 都不动；其它 alpha 目标 alpha 写回 255。
    let mut zero = frame_with(16, 16, [10, 20, 30, 255]);
    zero.blit_add(&rgba_image(2, 2, [101, 53, 7, 0]), 3, 4);
    assert_only_background(&zero, [10, 20, 30, 255], "G4：a=0 不得改变目标像素");
    let mut opaque = frame_with(16, 16, [10, 20, 30, 255]);
    opaque.blit_add(&rgba_image(2, 2, [101, 53, 7, 128]), 3, 4);
    assert_eq!(
        pixel_at(&opaque, 0, 0)[3],
        255,
        "G4：未覆盖像素目标 alpha 仍是 255"
    );
    assert_eq!(
        pixel_at(&opaque, 3, 4)[3],
        255,
        "G4：Add 后目标 alpha 必须写回 255"
    );
}

// ---------------------------------------------------------------------------
// G5：缺帧 false 后同 scene 继续、Bolt20 与旧 Over 金样
// ---------------------------------------------------------------------------

/// G5：缺 Magic2 帧返回 false 后同 scene 继续 Magic 成功；再 Bolt20 与旧 Over 各对金样；
/// 缺帧不 fallback、不吞后续绘制。
#[test]
fn r74_g5_missing_and_legacy_continuation() {
    let dir = write_libraries("g5");
    let mut scene = scene_in_dir(&dir);
    let background = [12, 34, 56, 255];
    let add = |source: [u8; 4]| expected_add(background, source);

    // ① 缺帧：Bolt(5) → magic2[25] 偏移表项 0 → false，零落屏、无尺寸。
    let mut frame = frame_with(32, 32, background);
    assert!(
        !scene.draw_effect_blend(&mut frame, FxKind::Bolt, 5, 3, 3, true),
        "G5：magic2[25] 缺帧 → false"
    );
    assert_only_background(&frame, background, "G5：缺帧不得 fallback 到 0 帧/其它图库");
    assert_eq!(
        scene.effect_size(FxKind::Bolt, 5),
        None,
        "G5：缺帧不得产生尺寸（不画占位方块）"
    );

    // ② 同一 scene 继续 Magic(0)：成功，且与全新金样逐像素相同（缺帧不污染缓存）。
    let mut golden_magic = frame_with(32, 32, background);
    assert!(
        scene.draw_effect_blend(&mut golden_magic, FxKind::Magic, 0, 3, 3, true),
        "G5：金样 Magic(0) 必须成功"
    );
    assert!(
        scene.draw_effect_blend(&mut frame, FxKind::Magic, 0, 3, 3, true),
        "G5：缺帧后同 scene 继续 Magic(0) 必须成功"
    );
    assert_eq!(
        frame.pixels, golden_magic.pixels,
        "G5：缺帧后的 Magic(0) 必须与未经历缺帧的金样完全相同"
    );
    assert_frame_matches(
        &frame,
        &oracle_frame(32, 32, background, &[Op::add(Fx::Magic(0), 3, 3, true)]),
        "G5：缺帧后继续 Magic(0) 整帧",
    );

    // ③ 继续 Bolt(0)（magic2[20]，3×1）与旧 Over 金样，逐帧追加。
    let continuation_ops = [
        Op::add(Fx::Magic(0), 3, 3, true),
        Op::add(Fx::Bolt(0), 16, 2, true),
    ];
    assert!(
        scene.draw_effect_blend(&mut frame, FxKind::Bolt, 0, 16, 2, true),
        "G5：缺帧后继续 Bolt(0) 必须成功"
    );
    assert_frame_matches(
        &frame,
        &oracle_frame(32, 32, background, &continuation_ops),
        "G5：缺帧后继续 Bolt(0) 整帧（Bolt20 金样）",
    );
    assert_eq!(
        pixel_at(&frame, 20, 1),
        add(RGBA_RED),
        "G5：Bolt(0) 必须从 magic2[20] 起，左上落在 (20,1)"
    );
    assert_eq!(
        pixel_at(&frame, 21, 1),
        add(RGBA_GREEN),
        "G5：Bolt(0) 中间必须是绿"
    );
    assert_eq!(
        pixel_at(&frame, 22, 1),
        add(RGBA_BLUE),
        "G5：Bolt(0) 右端必须是蓝"
    );

    // ④ 旧 Over：draw_effect（绝对）与 draw_effect_with_offset（偏移）在金样帧上追加。
    let mut over_ops = continuation_ops.to_vec();
    over_ops.push(Op::over(Fx::Magic(3), 5, 5, false));
    assert!(
        scene.draw_effect(&mut frame, FxKind::Magic, 3, 5, 5),
        "G5：旧 draw_effect(Over) 有图必须 true"
    );
    assert_frame_matches(
        &frame,
        &oracle_frame(32, 32, background, &over_ops),
        "G5：旧 Over 金样整帧",
    );
    assert_eq!(
        pixel_at(&frame, 5, 5),
        [0, 252, 248, 255],
        "G5：旧 Over 左上源青"
    );

    over_ops.push(Op::over(Fx::Magic(1), 10, 10, true));
    assert!(
        scene.draw_effect_with_offset(&mut frame, FxKind::Magic, 1, 10, 10),
        "G5：旧 draw_effect_with_offset 有图必须 true"
    );
    assert_frame_matches(
        &frame,
        &oracle_frame(32, 32, background, &over_ops),
        "G5：旧 Over(偏移) 金样整帧",
    );
    assert_eq!(
        pixel_at(&frame, 7, 12),
        RGBA_BLUE,
        "G5：旧 with_offset 必须把 (-3,+2) 消费到 (7,12)"
    );

    // ⑤ 缺帧不吞后续：Bolt(5) 再次 false 后仍可继续 Magic(0)。
    let mut after = frame_with(32, 32, background);
    assert!(!scene.draw_effect_blend(&mut after, FxKind::Bolt, 5, 1, 1, true));
    assert!(
        scene.draw_effect_blend(&mut after, FxKind::Magic, 0, 1, 1, true),
        "G5：二次缺帧后 Magic(0) 仍必须成功"
    );
    assert_frame_matches(
        &after,
        &oracle_frame(32, 32, background, &[Op::add(Fx::Magic(0), 1, 1, true)]),
        "G5：二次缺帧后继续绘制整帧",
    );

    cleanup(&dir);
}
