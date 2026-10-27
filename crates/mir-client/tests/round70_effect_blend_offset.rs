//! Round 70 B：混合特效逐帧 signed offset 生产消费（`draw_effect_blend` + `apply_offset`）。
//!
//! 合成 `magic.wzl` + `magic.wzx`（2×2 非黑不透明 RGB565、逐帧 i16 `offset_x/offset_y`、
//! 负/大偏移、纯黑 alpha=0 像素、缺帧/越界帧），经**真实** `EntityLibraries` 读取链解码，
//! 再走公开绘制入口：
//!
//! * `EntityRenderer::draw_effect_blend(frame, kind, index, x, y, apply_offset)`（新 Add 入口）；
//! * `MapScene::draw_effect_blend(...)`（薄转发）；
//! * 旧 `draw_effect`（绝对坐标 Over）/ `draw_effect_with_offset`（偏移 Over）兼容基线。
//!
//! 期望整帧 RGBA 由本文件内**独立 oracle** 从原始夹具像素按规定公式手算：
//!
//! * Over：`round((src * a + dst * (255 - a)) / 255)`；
//! * Add：`min(255, dst + round(src * a / 255))`（对应 Crystal `DrawBlend` 的
//!   `SourceAlpha/One`，即既有 `Frame::blit_add` 语义，见 `crates/mir-render/src/frame.rs:428,468`）；
//! * `a == 0` 像素整点跳过；命中像素目标 alpha 写回 255；
//! * 落点 = `saturating_add(锚点, 当帧 offset)`（`apply_offset=false` 时直接用锚点），
//!   偏移先于裁剪；越界像素裁剪，整图屏外零落屏但返回「有图」。
//!
//! 本文件**不调用**生产 `blit/blit_add/add_channel/blend_channel`，也不复制其实现：
//! oracle 从帧缓冲文档与派发契约的公式独立展开（逐通道 u32 手算）。
//!
//! 运行（缺 feature 时 `effect_image` 返回 `None`，首条断言以诊断文本失败而非 SKIP）：
//!
//! ```text
//! cargo test -p mir-client --features mir-render/legacy --test round70_effect_blend_offset -- --test-threads=1
//! ```
//!
//! alpha=128 的半透明 Add 行为无法由 RGB565 WZL 表达（WZL 只有 0/255），按契约放在
//! `crates/mir-render/src/frame.rs` 既有单测区的 `additive_blending_uses_semi_transparent_source_alpha`。

use std::fs;
use std::path::PathBuf;

use mir_render::{
    EntityLibraries, EntityRenderer, Frame, FxKind, MapScene, TileLibraries, Viewport,
};

/// 合成帧边长（2×2）。
const FRAME_SIDE: u16 = 2;

/// 合成 `magic.wzl` 帧表：帧号 + 帧头有符号偏移 + 2×2 RGB565 四角原始值
/// （左上、右上、左下、右下）。
///
/// | 帧 | offset | 用途 |
/// |---|---|---|
/// | 0 | (+8,-8) | G1 正偏移 / 只叠一次 / 跨帧 |
/// | 1 | (-8,+8) | G1 负偏移 / 无上一帧残留 |
/// | 2 | (0,0) | G3 非黑底真加法与饱和 |
/// | 3 | (+5,+3) | G4 apply_offset false/true |
/// | 4 | (-3,-4) | G2 部分裁剪 |
/// | 5 | (0,0) | G3 纯黑 alpha=0 像素 |
/// | 6 | (+2,-1) | G5 交替 / 重复缓存 |
/// | 7 | (+1000,-1000) | G2 大偏移屏外→移入 / i32 极值 |
const MAGIC_FRAMES: [MagicFrame; 8] = [
    MagicFrame::new(0, (8, -8), [0xF800, 0x2104, 0x07E0, 0x001F]),
    MagicFrame::new(1, (-8, 8), [0x2104, 0xF800, 0x001F, 0x07E0]),
    MagicFrame::new(2, (0, 0), [0xF800, 0x2104, 0x07E0, 0x001F]),
    MagicFrame::new(3, (5, 3), [0x07E0, 0x001F, 0xF800, 0x2104]),
    MagicFrame::new(4, (-3, -4), [0xF800, 0x07E0, 0x001F, 0x2104]),
    MagicFrame::new(5, (0, 0), [0xF800, 0x0000, 0x07E0, 0x001F]),
    MagicFrame::new(6, (2, -1), [0x07E0, 0xF800, 0x2104, 0x001F]),
    MagicFrame::new(7, (1000, -1000), [0xF81F, 0x07FF, 0xFFE0, 0x8410]),
];

/// 一张合成帧：帧号、帧头 i16 有符号偏移、四角 RGB565 原始值。
#[derive(Clone, Copy)]
struct MagicFrame {
    index: u32,
    offset: (i16, i16),
    quad: [u16; 4],
}

impl MagicFrame {
    const fn new(index: u32, offset: (i16, i16), quad: [u16; 4]) -> Self {
        Self {
            index,
            offset,
            quad,
        }
    }
}

// ---------------------------------------------------------------------------
// 夹具：进程唯一临时目录 + 合成 WZL/WZX
// ---------------------------------------------------------------------------

/// 进程唯一临时目录（不读 `resource`/`game-assets`，也不与其它测试共享路径）。
///
/// 优先系统临时目录；Windows 上 `TEMP/TMP` 可能被环境策略拒绝写（`PermissionDenied`），
/// 此时回退到仓库内允许的 `target/round70/b/tmp/`（测试产物目录，不入库）。
fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let name = format!("mir-next-r70-b-{tag}-{}-{nanos}", std::process::id());
    let candidates = [
        std::env::temp_dir().join(&name),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/round70/b/tmp")
            .join(&name),
    ];
    for dir in candidates {
        if fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }
    panic!("无法创建进程唯一临时目录（系统临时目录与 target/round70/b/tmp 均不可写）");
}

/// 写合成 `magic.wzl` + `magic.wzx` 并返回目录。
fn write_magic_library(tag: &str) -> PathBuf {
    let dir = unique_dir(tag);
    let (wzl, wzx) = build_wzl(&MAGIC_FRAMES);
    fs::write(dir.join("magic.wzl"), wzl).expect("写合成 magic.wzl");
    fs::write(dir.join("magic.wzx"), wzx).expect("写合成 magic.wzx");
    dir
}

/// 打开合成图库并构造实体渲染器。
fn renderer_with_magic(tag: &str) -> (EntityRenderer, PathBuf) {
    let dir = write_magic_library(tag);
    (EntityRenderer::new(EntityLibraries::open_dir(&dir)), dir)
}

/// 按 `.wzl`/`.wzx` 二进制格式构造一对文件内容。
///
/// `.wzl`：48 字节头 + 每条记录 16 字节记录头（格式 5=RGB565、宽高、i16 偏移、
/// u32 zlib 长度）+ zlib 数据；`.wzx`：48 字节头（[44..48] 数量）+ u32 偏移表。
/// 行序按 Crystal 自下而上：文件第 0 行写图像最下面一行。
fn build_wzl(frames: &[MagicFrame]) -> (Vec<u8>, Vec<u8>) {
    let count = frames
        .iter()
        .map(|frame| frame.index)
        .max()
        .expect("合成帧表不能为空")
        + 1;
    let mut wzl = vec![0u8; 48];
    let mut table = vec![0u32; usize::try_from(count).expect("帧数必须放得进 usize")];
    for spec in frames {
        let mut raw = Vec::with_capacity(usize::from(FRAME_SIDE) * usize::from(FRAME_SIDE) * 2);
        // 自下而上：文件第 0 行 = 图像最下面一行（左下、右下），再写最上面一行。
        for value in [spec.quad[2], spec.quad[3], spec.quad[0], spec.quad[1]] {
            raw.extend_from_slice(&value.to_le_bytes());
        }
        let compressed = zlib_store(&raw);
        table[usize::try_from(spec.index).expect("帧号必须放得进 usize")] =
            u32::try_from(wzl.len()).expect(".wzl 偏移必须放得进 u32");
        wzl.push(5); // 格式 5 = RGB565
        wzl.extend_from_slice(&[0, 0, 0]); // Crystal 直接跳过的 3 个保留字节
        wzl.extend_from_slice(&FRAME_SIDE.to_le_bytes());
        wzl.extend_from_slice(&FRAME_SIDE.to_le_bytes());
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

/// 帧表查询（缺帧号直接 panic，防止夹具与断言漂移）。
fn magic_frame(index: u32) -> &'static MagicFrame {
    MAGIC_FRAMES
        .iter()
        .find(|frame| frame.index == index)
        .expect("合成 magic.wzl 帧表必须覆盖该帧号")
}

// ---------------------------------------------------------------------------
// 独立 oracle：从原始夹具像素手算整帧 RGBA（不调用生产混合函数）
// ---------------------------------------------------------------------------

/// RGB565 → RGBA 的独立展开（与 Crystal `convert16bitTo32bit` 一致：只左移，不复制低位）。
fn expand565(value: u16) -> [u8; 4] {
    let red = ((value >> 11) & 0x1F) << 3;
    let green = ((value >> 5) & 0x3F) << 2;
    let blue = (value & 0x1F) << 3;
    [red as u8, green as u8, blue as u8, 255]
}

/// 解码后逐像素 alpha：`remove_black_all`（`crates/mir-render/src/scene.rs:827`）把 RGB
/// 全 0 的像素置 alpha=0；WZL 的 RGB565 其它像素 alpha=255（无 alpha 通道）。
fn fixture_alpha(color: [u8; 4]) -> u8 {
    if color[0] == 0 && color[1] == 0 && color[2] == 0 {
        0
    } else {
        255
    }
}

/// Add 单通道（按帧缓冲文档公式手算，四舍五入、饱和 255）。
fn oracle_add_channel(destination: u8, source: u8, alpha: u8) -> u8 {
    let contribution = (u32::from(source) * u32::from(alpha) + 127) / 255;
    u8::try_from((u32::from(destination) + contribution).min(255)).unwrap_or(255)
}

/// Over 单通道（按帧缓冲文档公式手算，四舍五入）。
fn oracle_over_channel(destination: u8, source: u8, alpha: u8) -> u8 {
    let alpha = u32::from(alpha);
    let source = u32::from(source);
    let destination = u32::from(destination);
    u8::try_from((source * alpha + destination * (255 - alpha) + 127) / 255).unwrap_or(255)
}

/// oracle 的混合方式。
#[derive(Clone, Copy, Debug)]
enum OracleMode {
    Over,
    Add,
}

/// oracle 的一次绘制：帧号 + 锚点 + 是否消费当帧图像 offset + 混合方式。
#[derive(Clone, Copy, Debug)]
struct OracleOp {
    frame: u32,
    x: i32,
    y: i32,
    apply_offset: bool,
    mode: OracleMode,
}

impl OracleOp {
    const fn add(frame: u32, x: i32, y: i32, apply_offset: bool) -> Self {
        Self {
            frame,
            x,
            y,
            apply_offset,
            mode: OracleMode::Add,
        }
    }

    const fn over(frame: u32, x: i32, y: i32, apply_offset: bool) -> Self {
        Self {
            frame,
            x,
            y,
            apply_offset,
            mode: OracleMode::Over,
        }
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
        let spec = magic_frame(op.frame);
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
        for (slot, &raw) in spec.quad.iter().enumerate() {
            let source = expand565(raw);
            let alpha = fixture_alpha(source);
            if alpha == 0 {
                // alpha=0：整点跳过，不动 RGB，也不动目标 alpha 字节。
                continue;
            }
            let x = i64::from(left) + i64::try_from(slot % 2).expect("列");
            let y = i64::from(top) + i64::try_from(slot / 2).expect("行");
            if x < 0 || y < 0 || x >= i64::from(width) || y >= i64::from(height) {
                continue;
            }
            let offset =
                (usize::try_from(y).expect("y") * width_usize + usize::try_from(x).expect("x")) * 4;
            let target = &mut pixels[offset..offset + 4];
            for channel in 0..3 {
                target[channel] = match op.mode {
                    OracleMode::Over => {
                        oracle_over_channel(target[channel], source[channel], alpha)
                    }
                    OracleMode::Add => oracle_add_channel(target[channel], source[channel], alpha),
                };
            }
            target[3] = 255;
        }
    }
    pixels
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

/// 缓冲里的像素（无 Frame 包装，*不*做负坐标透明化；越界 panic）。
fn buffer_pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let offset = (usize::try_from(y).expect("y") * usize::try_from(width).expect("width")
        + usize::try_from(x).expect("x"))
        * 4;
    let mut value = [0u8; 4];
    value.copy_from_slice(&pixels[offset..offset + 4]);
    value
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

/// 删除进程唯一临时目录（失败不影响断言结论）。
fn cleanup(dir: &PathBuf) {
    let _ = fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// G1：逐帧 signed offset、每帧只叠一次、跨帧无残留
// ---------------------------------------------------------------------------

/// G1：正负 ±8 跨帧，Add 入口消费当帧 `offset_x/offset_y` 且只叠加一次（不跨帧累积）。
#[test]
fn r70_g1_add_blend_consumes_signed_offset_per_frame_once() {
    let (mut renderer, dir) = renderer_with_magic("g1");

    // 真实读取链：目录扫描 → 打开 magic.wzl → WzlReader 解码；i16 偏移带符号读回。
    for (index, expected) in [(0u32, (8i16, -8i16)), (1, (-8, 8)), (7, (1000, -1000))] {
        let info = renderer
            .effect_image(FxKind::Magic, index)
            .expect("合成 magic.wzl 必须经真实读取链返回图像；请加 --features mir-render/legacy")
            .info;
        assert_eq!(
            (info.width, info.height),
            (FRAME_SIDE, FRAME_SIDE),
            "G1：合成帧必须是 2×2"
        );
        assert_eq!(
            (info.offset_x, info.offset_y),
            expected,
            "G1：帧 {index} 的 i16 偏移必须原样读回（负号/大值不截断）"
        );
    }

    let background = [0, 0, 0, 255];

    // —— 帧 0：offset (+8,-8)，锚点 (20,20) → 整帧左上角 (28,12) ——
    let mut frame0 = frame_with(64, 64, background);
    assert!(
        renderer.draw_effect_blend(&mut frame0, FxKind::Magic, 0, 20, 20, true),
        "G1：有图必须返回 true"
    );
    assert_frame_matches(
        &frame0,
        &oracle_frame(64, 64, background, &[OracleOp::add(0, 20, 20, true)]),
        "G1：帧 0 的 (+8,-8) 必须整帧平移且只叠一次",
    );
    assert_eq!(
        pixel_at(&frame0, 28, 12),
        expand565(magic_frame(0).quad[0]),
        "G1：帧 0 左上角必须落在锚点 + 当帧 offset=(28,12)"
    );
    // 原点、单轴、双次偏移位置都不得落像素。
    for (x, y) in [(20, 20), (28, 20), (20, 12), (36, 4)] {
        assert_eq!(
            pixel_at(&frame0, x, y),
            background,
            "G1：({x},{y}) 不得落像素（offset 只能叠加一次、不得累积）"
        );
    }

    // —— 帧 1：offset (-8,+8)，锚点 (40,40) → 整帧左上角 (32,48) ——
    let mut frame1 = frame_with(64, 64, background);
    assert!(
        renderer.draw_effect_blend(&mut frame1, FxKind::Magic, 1, 40, 40, true),
        "G1：帧 1 有图必须返回 true"
    );
    assert_frame_matches(
        &frame1,
        &oracle_frame(64, 64, background, &[OracleOp::add(1, 40, 40, true)]),
        "G1：帧 1 的 (-8,+8) 必须整帧平移",
    );
    assert_eq!(
        pixel_at(&frame1, 32, 48),
        expand565(magic_frame(1).quad[0]),
        "G1：帧 1 左上角必须落在锚点 + (-8,+8)"
    );
    assert_ne!(
        frame0.pixels, frame1.pixels,
        "G1：帧 0/1 的 offset 与像素必须逐帧不同"
    );

    // —— 同一帧重复绘制：offset 不累积（位置不漂移），Add 按次数叠加 ——
    let mut twice = frame_with(64, 64, background);
    assert!(renderer.draw_effect_blend(&mut twice, FxKind::Magic, 0, 20, 20, true));
    let once = twice.pixels.clone();
    assert!(renderer.draw_effect_blend(&mut twice, FxKind::Magic, 0, 20, 20, true));
    assert_frame_matches(
        &twice,
        &oracle_frame(
            64,
            64,
            background,
            &[
                OracleOp::add(0, 20, 20, true),
                OracleOp::add(0, 20, 20, true),
            ],
        ),
        "G1：同帧二次绘制的 offset 不得累积（位置不漂移）",
    );
    assert_eq!(
        covered_positions(&twice.pixels, 64, background),
        covered_positions(&once, 64, background),
        "G1：重复绘制的覆盖位置集合必须完全相同（偏移不漂移）"
    );

    // —— 缓存 metadata / 尺寸不得被绘制入口改写 ——
    let after = renderer
        .effect_image(FxKind::Magic, 0)
        .expect("G1：缓存 meta 仍应可读")
        .info;
    assert_eq!(
        (after.offset_x, after.offset_y),
        (8, -8),
        "G1：绘制入口不得改写缓存帧头 offset"
    );
    assert_eq!(
        renderer.effect_size(FxKind::Magic, 0),
        Some((u32::from(FRAME_SIDE), u32::from(FRAME_SIDE))),
        "G1：绘制入口不得改写缓存尺寸"
    );

    cleanup(&dir);
}

/// G1：换帧（帧 0 → 帧 1）独立绘制时没有上一帧残留；同帧连续两帧按 oracle 叠加。
#[test]
fn r70_g1_add_blend_no_previous_frame_residue() {
    let (mut renderer, dir) = renderer_with_magic("g1b");
    let background = [0, 0, 0, 255];

    // 帧 0 画进 A，帧 1 画进 B（同一锚点）：B 不得出现 A 的 (+8,-8) 落点。
    let mut frame_a = frame_with(64, 64, background);
    assert!(renderer.draw_effect_blend(&mut frame_a, FxKind::Magic, 0, 20, 20, true));
    assert_frame_matches(
        &frame_a,
        &oracle_frame(64, 64, background, &[OracleOp::add(0, 20, 20, true)]),
        "G1b：帧 0 独立整帧",
    );

    let mut frame_b = frame_with(64, 64, background);
    assert!(renderer.draw_effect_blend(&mut frame_b, FxKind::Magic, 1, 20, 20, true));
    assert_frame_matches(
        &frame_b,
        &oracle_frame(64, 64, background, &[OracleOp::add(1, 20, 20, true)]),
        "G1b：帧 1 独立整帧",
    );
    // 帧 0 的位置 (28,12)…(29,13) 在帧 1 的整帧里必须是背景（无上一帧残留）。
    for (x, y) in [(28, 12), (29, 12), (28, 13), (29, 13)] {
        assert_eq!(
            pixel_at(&frame_b, x, y),
            background,
            "G1b：帧 1 不得残留帧 0 的 (+8,-8) 像素（({x},{y})）"
        );
    }
    assert_ne!(frame_a.pixels, frame_b.pixels, "G1b：跨帧整帧必须不同");

    // 同一帧连续画帧 0、帧 1：整帧 = 两个 op 的 oracle；交换顺序结果相同（无隐藏状态）。
    let mut both = frame_with(64, 64, background);
    assert!(renderer.draw_effect_blend(&mut both, FxKind::Magic, 0, 20, 20, true));
    assert!(renderer.draw_effect_blend(&mut both, FxKind::Magic, 1, 20, 20, true));
    assert_frame_matches(
        &both,
        &oracle_frame(
            64,
            64,
            background,
            &[
                OracleOp::add(0, 20, 20, true),
                OracleOp::add(1, 20, 20, true),
            ],
        ),
        "G1b：同帧连续两帧 offset 各自生效",
    );

    let mut swapped = frame_with(64, 64, background);
    assert!(renderer.draw_effect_blend(&mut swapped, FxKind::Magic, 1, 20, 20, true));
    assert!(renderer.draw_effect_blend(&mut swapped, FxKind::Magic, 0, 20, 20, true));
    assert_eq!(
        both.pixels, swapped.pixels,
        "G1b：Add 叠加顺序不得改变结果（无逐帧残留状态）"
    );

    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G2：偏移先于裁剪、屏外锚点移入、部分裁剪、负坐标、i32 极值
// ---------------------------------------------------------------------------

/// G2：大偏移把屏外锚点移入屏内、部分裁剪、负坐标与 i32 极值不 panic。
#[test]
fn r70_g2_add_blend_offsets_before_crop_and_moves_anchor_in() {
    let (mut renderer, dir) = renderer_with_magic("g2");
    let background = [0, 0, 0, 255];

    // ① 帧 7 offset (+1000,-1000)：锚点 (-998,1001) 在屏外，偏移后左上角 (2,1) 落屏。
    //    若实现「偏移前早退裁剪」（M2），这里将零落屏 → 本条断言行首败。
    let mut moved_in = frame_with(10, 10, background);
    assert!(
        renderer.draw_effect_blend(&mut moved_in, FxKind::Magic, 7, -998, 1001, true),
        "G2：屏外锚点经 offset 移入屏内仍必须有图 → true"
    );
    assert_frame_matches(
        &moved_in,
        &oracle_frame(10, 10, background, &[OracleOp::add(7, -998, 1001, true)]),
        "G2：偏移必须先于裁剪，屏外锚点经 offset 移入后 4 个像素必须出现",
    );
    assert_eq!(
        pixel_at(&moved_in, 2, 1),
        expand565(magic_frame(7).quad[0]),
        "G2：移入后的左上角必须落在 (2,1)"
    );
    assert_eq!(
        covered_positions(&moved_in.pixels, 10, background).len(),
        4,
        "G2：移入后必须恰好 4 个像素"
    );

    // ② 帧 7 大偏移 + 屏内锚点 (0,0) → 整帧屏外：有图仍 true，零落屏。
    let mut offscreen = frame_with(10, 10, background);
    assert!(
        renderer.draw_effect_blend(&mut offscreen, FxKind::Magic, 7, 0, 0, true),
        "G2：有图但完全屏外 → 仍返回 true（返回值原义是存在性）"
    );
    assert_only_background(&offscreen, background, "G2：大偏移完全屏外不得落像素");

    // ③ 部分裁剪：帧 4 offset (-3,-4)，锚点 (2,4) → 左上角 (-1,0)；
    //    2×2 图只剩右列（槽 1、3）落在 (0,0)/(0,1)。
    let mut partial = frame_with(10, 10, background);
    assert!(
        renderer.draw_effect_blend(&mut partial, FxKind::Magic, 4, 2, 4, true),
        "G2：部分越界但仍落屏 → true"
    );
    assert_frame_matches(
        &partial,
        &oracle_frame(10, 10, background, &[OracleOp::add(4, 2, 4, true)]),
        "G2：偏移后负坐标部分裁剪只能保留屏内像素",
    );
    assert_eq!(
        pixel_at(&partial, 0, 0),
        expand565(magic_frame(4).quad[1]),
        "G2：部分裁剪后 (0,0) 必须是源图右上角"
    );
    assert_eq!(
        pixel_at(&partial, 0, 1),
        expand565(magic_frame(4).quad[3]),
        "G2：部分裁剪后 (0,1) 必须是源图右下角"
    );
    assert_eq!(
        covered_positions(&partial.pixels, 10, background).len(),
        2,
        "G2：部分裁剪只能保留 2 个像素"
    );

    // ④ 负坐标整帧屏外：帧 0 offset (+8,-8)，锚点 (-20,-20) → (-12,-28)，仍 true 零落屏。
    let mut negative = frame_with(10, 10, background);
    assert!(
        renderer.draw_effect_blend(&mut negative, FxKind::Magic, 0, -20, -20, true),
        "G2：负坐标屏外有图 → true"
    );
    assert_only_background(&negative, background, "G2：负坐标屏外不得落像素");

    // ⑤ i32 极值不 panic（saturating_add 收口后交给 i64 裁剪）。
    let mut extreme = frame_with(16, 16, background);
    assert!(
        renderer.draw_effect_blend(&mut extreme, FxKind::Magic, 7, i32::MAX, i32::MIN, true),
        "G2：i32::MAX/i32::MIN + 大偏移不得 panic，有图 → true"
    );
    assert!(
        renderer.draw_effect_blend(&mut extreme, FxKind::Magic, 1, i32::MIN, i32::MAX, true),
        "G2：i32 极值 + 负偏移不得 panic，有图 → true"
    );
    assert_only_background(&extreme, background, "G2：i32 极值不得落像素");

    cleanup(&dir);
}

/// G2：缺图返回 false（存在性），有图屏外返回 true；缺图不得落像素、不得有尺寸。
#[test]
fn r70_g2_add_blend_missing_frame_false_offscreen_true() {
    let (mut renderer, dir) = renderer_with_magic("g2b");
    let background = [7, 8, 9, 255];

    // 缺帧/越界帧：三个入口一律 false，且零落屏。
    for missing in [9000u32, 8000] {
        let mut frame = frame_with(10, 10, background);
        assert!(
            !renderer.draw_effect_blend(&mut frame, FxKind::Magic, missing, 5, 5, true),
            "G2b：帧 {missing} 缺图 → draw_effect_blend 必须 false"
        );
        assert!(
            !renderer.draw_effect_with_offset(&mut frame, FxKind::Magic, missing, 5, 5),
            "G2b：帧 {missing} 缺图 → 旧 with_offset 必须 false"
        );
        assert!(
            !renderer.draw_effect(&mut frame, FxKind::Magic, missing, 5, 5),
            "G2b：帧 {missing} 缺图 → 旧 draw_effect 必须 false"
        );
        assert_only_background(&frame, background, "G2b：缺图不得落像素");
        assert_eq!(
            renderer.effect_size(FxKind::Magic, missing),
            None,
            "G2b：帧 {missing} 缺图 → 无尺寸（不画占位方块）"
        );
    }

    // 有图但完全屏外：true，零落屏。
    let mut offscreen = frame_with(10, 10, background);
    assert!(
        renderer.draw_effect_blend(&mut offscreen, FxKind::Magic, 7, 0, 0, true),
        "G2b：有图完全屏外 → true"
    );
    assert!(
        renderer.draw_effect_blend(&mut offscreen, FxKind::Magic, 0, i32::MAX, i32::MAX, true),
        "G2b：i32::MAX 锚点有图 → true"
    );
    assert_only_background(&offscreen, background, "G2b：完全屏外已有图不得落像素");

    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G3：Add 真加法（非黑底、饱和 255、alpha=0 跳过）
// ---------------------------------------------------------------------------

/// G3：非黑背景下 Add 是真加法（逐通道 `dst + round(src*a/255)` 饱和 255），与 Over 不同。
#[test]
fn r70_g3_add_blend_is_true_addition_saturating() {
    let (mut renderer, dir) = renderer_with_magic("g3");
    let background = [40, 50, 60, 255];

    // 帧 2 offset (0,0)：锚点 (3,3) 即左上角。
    let mut frame = frame_with(8, 8, background);
    assert!(
        renderer.draw_effect_blend(&mut frame, FxKind::Magic, 2, 3, 3, true),
        "G3：有图必须返回 true"
    );
    let expected_add = oracle_frame(8, 8, background, &[OracleOp::add(2, 3, 3, true)]);
    assert_frame_matches(&frame, &expected_add, "G3：Add 必须在非黑底上逐通道相加");

    // 逐通道锁定：饱和与不饱和各两例（RGB565 展开后 src=(248,0,0)/(32,32,32)/(0,252,0)/(0,0,248)）。
    assert_eq!(
        pixel_at(&frame, 3, 3),
        [255, 50, 60, 255],
        "G3：R 通道 40+248 饱和 255"
    );
    assert_eq!(
        pixel_at(&frame, 4, 3),
        [72, 82, 92, 255],
        "G3：非饱和通道 40+32 / 50+32 / 60+32"
    );
    assert_eq!(
        pixel_at(&frame, 3, 4),
        [40, 255, 60, 255],
        "G3：G 通道 50+252 饱和 255"
    );
    assert_eq!(
        pixel_at(&frame, 4, 4),
        [40, 50, 255, 255],
        "G3：B 通道 60+248 饱和 255"
    );

    // Add 必须与 Over 不同：Over(alpha=255) 是覆盖，(3,3) 应为源色 (248,0,0)。
    let expected_over = oracle_frame(8, 8, background, &[OracleOp::over(2, 3, 3, true)]);
    assert_ne!(
        frame.pixels, expected_over,
        "G3：Add 结果必须与 Over 不同（M3 把 Add 改调 Over 会在此首败）"
    );
    assert_eq!(
        buffer_pixel(&expected_over, 8, 3, 3),
        [248, 0, 0, 255],
        "G3：oracle 的 Over 应覆盖为源色（对照值）"
    );

    // 重复 Add 仍按次数准确叠加（饱和后保持 255）。
    assert!(renderer.draw_effect_blend(&mut frame, FxKind::Magic, 2, 3, 3, true));
    assert_frame_matches(
        &frame,
        &oracle_frame(
            8,
            8,
            background,
            &[OracleOp::add(2, 3, 3, true), OracleOp::add(2, 3, 3, true)],
        ),
        "G3：重复 Add 必须逐次相加并饱和 255",
    );

    cleanup(&dir);
}

/// G3：纯黑像素解码后 alpha=0，Add 必须整点跳过（不改 RGB/alpha），其它像素正常相加。
#[test]
fn r70_g3_add_blend_alpha_zero_pixel_skipped() {
    let (mut renderer, dir) = renderer_with_magic("g3b");
    let background = [40, 50, 60, 255];

    // 帧 5 offset (0,0)：左上红、右上纯黑（alpha=0）、左下绿、右下蓝。
    let mut frame = frame_with(8, 8, background);
    assert!(
        renderer.draw_effect_blend(&mut frame, FxKind::Magic, 5, 2, 2, true),
        "G3b：有图必须返回 true"
    );
    assert_frame_matches(
        &frame,
        &oracle_frame(8, 8, background, &[OracleOp::add(5, 2, 2, true)]),
        "G3b：alpha=0（纯黑）像素在 Add 下必须整点跳过",
    );
    // 右上槽（纯黑）完全不变。
    assert_eq!(
        pixel_at(&frame, 3, 2),
        background,
        "G3b：alpha=0 像素不得改变目标（Add 跳过于 a==0）"
    );
    // 其它三个槽确实相加。
    assert_eq!(
        pixel_at(&frame, 2, 2),
        [255, 50, 60, 255],
        "G3b：左上红必须相加"
    );
    assert_eq!(
        pixel_at(&frame, 2, 3),
        [40, 255, 60, 255],
        "G3b：左下绿必须相加"
    );
    assert_eq!(
        pixel_at(&frame, 3, 3),
        [40, 50, 255, 255],
        "G3b：右下蓝必须相加"
    );
    // 目标 alpha 恒 255（不透明画布）。
    assert_eq!(
        pixel_at(&frame, 3, 2)[3],
        255,
        "G3b：跳过的像素目标 alpha 仍是 255"
    );

    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G4：apply_offset 显式开关（false=绝对坐标，true=偏移）
// ---------------------------------------------------------------------------

/// G4：`apply_offset=false` 不消费帧头 offset（绝对落点），`true` 才偏移；两者不同。
#[test]
fn r70_g4_apply_offset_false_is_absolute_true_is_shifted() {
    let (mut renderer, dir) = renderer_with_magic("g4");
    let background = [0, 0, 0, 255];
    // 帧 3 offset (+5,+3)。
    let info = renderer
        .effect_image(FxKind::Magic, 3)
        .expect("G4：合成帧 3 必须可读")
        .info;
    assert_eq!(
        (info.offset_x, info.offset_y),
        (5, 3),
        "G4：帧 3 的帧头 offset 必须为 (+5,+3)"
    );

    // false：绝对坐标 (10,10)，不消费 (5,3)。
    let mut absolute = frame_with(64, 64, background);
    assert!(
        renderer.draw_effect_blend(&mut absolute, FxKind::Magic, 3, 10, 10, false),
        "G4：有图必须返回 true"
    );
    assert_frame_matches(
        &absolute,
        &oracle_frame(64, 64, background, &[OracleOp::add(3, 10, 10, false)]),
        "G4：apply_offset=false 必须绝对落点（不得偏移）",
    );
    assert_eq!(
        pixel_at(&absolute, 10, 10),
        expand565(magic_frame(3).quad[0]),
        "G4：false 时左上角必须正好在 (10,10)"
    );
    assert_eq!(
        pixel_at(&absolute, 15, 13),
        background,
        "G4：false 时不得消费 offset 落到 (15,13)"
    );

    // true：锚点 + 当帧 offset → (15,13)。
    let mut shifted = frame_with(64, 64, background);
    assert!(
        renderer.draw_effect_blend(&mut shifted, FxKind::Magic, 3, 10, 10, true),
        "G4：有图必须返回 true"
    );
    assert_frame_matches(
        &shifted,
        &oracle_frame(64, 64, background, &[OracleOp::add(3, 10, 10, true)]),
        "G4：apply_offset=true 必须消费当帧 offset",
    );
    assert_eq!(
        pixel_at(&shifted, 15, 13),
        expand565(magic_frame(3).quad[0]),
        "G4：true 时左上角必须落在 (15,13)"
    );
    assert_eq!(
        pixel_at(&shifted, 10, 10),
        background,
        "G4：true 时原点不得落像素"
    );
    assert_ne!(
        absolute.pixels, shifted.pixels,
        "G4：false 与 true 的整帧落点必须不同（M4 强制偏移会在此首败）"
    );

    // 旧入口兼容：draw_effect_with_offset = (偏移, Over)；draw_effect = (绝对, Over)。
    let mut legacy_shifted = frame_with(64, 64, background);
    assert!(renderer.draw_effect_with_offset(&mut legacy_shifted, FxKind::Magic, 3, 10, 10));
    assert_frame_matches(
        &legacy_shifted,
        &oracle_frame(64, 64, background, &[OracleOp::over(3, 10, 10, true)]),
        "G4：旧 with_offset 必须保持 (偏移, Over) 语义",
    );
    assert_eq!(
        covered_positions(&legacy_shifted.pixels, 64, background),
        covered_positions(&shifted.pixels, 64, background),
        "G4：新入口 true 与旧 with_offset 的偏移落点集合必须一致"
    );

    let mut legacy_absolute = frame_with(64, 64, background);
    assert!(renderer.draw_effect(&mut legacy_absolute, FxKind::Magic, 3, 10, 10));
    assert_frame_matches(
        &legacy_absolute,
        &oracle_frame(64, 64, background, &[OracleOp::over(3, 10, 10, false)]),
        "G4：旧 draw_effect 必须保持 (绝对, Over) 语义",
    );
    assert_eq!(
        covered_positions(&legacy_absolute.pixels, 64, background),
        covered_positions(&absolute.pixels, 64, background),
        "G4：新入口 false 与旧 draw_effect 的落点集合必须一致"
    );

    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// G5：scene 薄转发、旧 Over 基线、交替无泄漏
// ---------------------------------------------------------------------------

/// 用合成图库构造最小 `MapScene`（空地图 + 空瓦片库 + 合成实体库）。
fn scene_with_magic(tag: &str) -> (MapScene, PathBuf) {
    let dir = write_magic_library(tag);
    let map = mir_world::Map {
        width: 32,
        height: 32,
        cells: vec![mir_world::MapCell::default(); 32 * 32],
        file_name: "r70-empty".to_owned(),
        version: mir_world::MapVersion::V0,
        doors: Vec::new(),
    };
    let scene = MapScene::from_parts_with_entities(
        map,
        TileLibraries::empty("."),
        Viewport::DEFAULT,
        EntityLibraries::open_dir(&dir),
    );
    (scene, dir)
}

/// G5：`MapScene::draw_effect_blend` 薄转发与 `EntityRenderer` 同结果（新增/旧入口都转发）。
#[test]
fn r70_g5_scene_forwards_draw_effect_blend_identically() {
    let (mut scene, dir) = scene_with_magic("g5-scene");
    let mut renderer = EntityRenderer::new(EntityLibraries::open_dir(&dir));
    let background = [0, 0, 0, 255];

    // 新 Add 入口：scene 转发 vs entities 直调必须像素级相同，且等于独立 oracle。
    let mut scene_frame = frame_with(64, 64, background);
    let mut renderer_frame = frame_with(64, 64, background);
    assert!(
        scene.draw_effect_blend(&mut scene_frame, FxKind::Magic, 0, 20, 20, true),
        "G5：scene 新入口有图 → true"
    );
    assert!(
        renderer.draw_effect_blend(&mut renderer_frame, FxKind::Magic, 0, 20, 20, true),
        "G5：entities 新入口有图 → true"
    );
    assert_eq!(
        scene_frame.pixels, renderer_frame.pixels,
        "G5：MapScene 薄转发必须与 EntityRenderer 新入口同结果"
    );
    assert_frame_matches(
        &scene_frame,
        &oracle_frame(64, 64, background, &[OracleOp::add(0, 20, 20, true)]),
        "G5：scene 转发的整帧必须等于独立 oracle",
    );

    // 旧入口转发兼容：draw_effect / draw_effect_with_offset。
    let mut scene_legacy = frame_with(64, 64, background);
    let mut renderer_legacy = frame_with(64, 64, background);
    assert!(scene.draw_effect(&mut scene_legacy, FxKind::Magic, 0, 10, 10));
    assert!(renderer.draw_effect(&mut renderer_legacy, FxKind::Magic, 0, 10, 10));
    assert_eq!(
        scene_legacy.pixels, renderer_legacy.pixels,
        "G5：旧 draw_effect 经 scene 转发结果必须一致"
    );

    let mut scene_shifted = frame_with(64, 64, background);
    let mut renderer_shifted = frame_with(64, 64, background);
    assert!(scene.draw_effect_with_offset(&mut scene_shifted, FxKind::Magic, 1, 20, 20));
    assert!(renderer.draw_effect_with_offset(&mut renderer_shifted, FxKind::Magic, 1, 20, 20));
    assert_eq!(
        scene_shifted.pixels, renderer_shifted.pixels,
        "G5：旧 draw_effect_with_offset 经 scene 转发结果必须一致"
    );

    // 缺图 false 语义经 scene 转发一致。
    let mut missing = frame_with(10, 10, background);
    assert!(
        !scene.draw_effect_blend(&mut missing, FxKind::Magic, 9000, 5, 5, true),
        "G5：scene 转发缺图 → false"
    );
    assert_only_background(&missing, background, "G5：scene 转发缺图不得落像素");

    cleanup(&dir);
}

/// G5：Add / 旧 Over / 旧 `with_offset` 交替与重复缓存无泄漏；旧 Over 整帧与基线一致。
#[test]
fn r70_g5_legacy_over_and_add_alternation_no_leak() {
    let (mut renderer, dir) = renderer_with_magic("g5");
    let background = [10, 20, 30, 255];

    // 交替序列：新 Add(true) → 旧 draw_effect(绝对 Over) → 旧 with_offset(偏移 Over)
    // → 新 Add(false)。
    let ops = [
        OracleOp::add(6, 20, 20, true),
        OracleOp::over(0, 10, 10, false),
        OracleOp::over(1, 40, 10, true),
        OracleOp::add(3, 30, 30, false),
    ];
    let play = |renderer: &mut EntityRenderer, frame: &mut Frame, ops: &[OracleOp]| {
        for op in ops {
            let ok = match (op.mode, op.apply_offset) {
                (OracleMode::Add, apply_offset) => renderer.draw_effect_blend(
                    frame,
                    FxKind::Magic,
                    op.frame,
                    op.x,
                    op.y,
                    apply_offset,
                ),
                (OracleMode::Over, false) => {
                    renderer.draw_effect(frame, FxKind::Magic, op.frame, op.x, op.y)
                }
                (OracleMode::Over, true) => {
                    renderer.draw_effect_with_offset(frame, FxKind::Magic, op.frame, op.x, op.y)
                }
            };
            assert!(ok, "G5：交替序列的每次绘制都必须返回 true（有图）");
        }
    };

    // ① 单轮：整帧等于独立 oracle（覆盖旧 Over 基线 + 新 Add 叠加）。
    let mut single = frame_with(96, 96, background);
    play(&mut renderer, &mut single, &ops);
    assert_frame_matches(
        &single,
        &oracle_frame(96, 96, background, &ops),
        "G5：Add/旧 Over/旧 with_offset 交替整帧必须等于独立 oracle",
    );

    // ② 同一帧重复两轮：整帧 = 两轮 oracle（证明没有跨调用残留状态/偏移累积）。
    let mut doubled = frame_with(96, 96, background);
    play(&mut renderer, &mut doubled, &ops);
    play(&mut renderer, &mut doubled, &ops);
    let mut doubled_ops: Vec<OracleOp> = ops.to_vec();
    doubled_ops.extend_from_slice(&ops);
    assert_frame_matches(
        &doubled,
        &oracle_frame(96, 96, background, &doubled_ops),
        "G5：交替序列重复两轮不得泄漏（偏移不累积、缓存不污染）",
    );

    // ③ 重复绘制必须命中缓存：命中增加、未命中不增长（不重新解码）。
    let (hits_before, misses_before) = renderer.cache_stats();
    let mut again = frame_with(96, 96, background);
    play(&mut renderer, &mut again, &ops);
    let (hits_after, misses_after) = renderer.cache_stats();
    assert!(
        hits_after > hits_before,
        "G5：重复绘制必须命中缓存（hits {hits_before} → {hits_after}）"
    );
    assert_eq!(
        misses_after, misses_before,
        "G5：重复绘制不得重新解码（misses {misses_before} → {misses_after}）"
    );
    assert_eq!(
        again.pixels, single.pixels,
        "G5：重复一轮的整帧必须与首轮完全相同"
    );

    // ④ 旧 Over 单独整帧基线（不依赖 Add 路径）。
    let mut over_only = frame_with(96, 96, background);
    assert!(
        renderer.draw_effect(&mut over_only, FxKind::Magic, 0, 10, 10),
        "G5：旧 draw_effect 有图 → true"
    );
    assert_frame_matches(
        &over_only,
        &oracle_frame(96, 96, background, &[OracleOp::over(0, 10, 10, false)]),
        "G5：旧 Over 整帧必须与独立基线一致（兼容性）",
    );

    // ⑤ 缓存 metadata 不被任何入口改写。
    for (index, expected) in [(0u32, (8i16, -8i16)), (1, (-8, 8)), (6, (2, -1))] {
        let info = renderer
            .effect_image(FxKind::Magic, index)
            .expect("G5：缓存 meta 仍应可读")
            .info;
        assert_eq!(
            (info.offset_x, info.offset_y),
            expected,
            "G5：交替绘制不得改写帧 {index} 的缓存 offset"
        );
    }

    cleanup(&dir);
}
