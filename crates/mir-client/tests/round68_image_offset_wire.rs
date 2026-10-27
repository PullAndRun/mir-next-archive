//! Round 68 B：合成 WZL/WZX 帧的 `ImageInfo` 有符号偏移读取链 + 新/旧绘制入口语义。
//!
//! 覆盖（`docs/process/round68-plan.md` 的 B 交付 / 门下 C1/C5）：
//!
//! 1. `r68_g1_synthetic_wzl_signed_offsets_pixels_are_consumed`：
//!    进程唯一临时目录里合成 `magic.wzl` + `magic.wzx`（2×2 非黑不透明 RGB565 帧，
//!    帧头 offset 依次 `(-3,5)/(4,-2)/(-2,-3)/(3,4)`），经**真实** `EntityLibraries`
//!    → `ImageLibrary`/`WzlReader` 读取链解码，再走公开绘制入口
//!    [`EntityRenderer::draw_effect_with_offset`]；用测试内独立 oracle 断言整帧 RGBA：
//!    负偏移正确平移、450/451 与 570/571 逐帧不同 offset、原点/单轴位置不误绘。
//! 2. `r68_g2_legacy_draw_effect_ignores_offset_and_crop_semantics`：
//!    旧 [`EntityRenderer::draw_effect`] 带非零 offset 仍画在调用者坐标、返回值语义
//!    （有图 `true` / 缺帧 `false`）、裁剪/完全屏外/极端 `i32`/屏外锚点经 offset 移入
//!    屏内、缓存 meta 不变、交替新旧入口重复绘制不累积。
//!
//! 注意：合成素材是原始 `.wzl/.wzx`，`EntityLibraries::open_dir` 的 WZL 扫描在
//! `legacy` feature 下才编译。运行本测试必须：
//!
//! ```text
//! cargo test -p mir-client --features mir-render/legacy --test round68_image_offset_wire
//! ```
//!
//! 缺 feature 时 `effect_image` 返回 `None`，第一条断言会以诊断文本失败（而不是静默 SKIP）。
//! 本文件不断言中文 UI 文案；中文只作为失败诊断。

use std::fs;
use std::path::PathBuf;

use mir_render::{EntityLibraries, EntityRenderer, Frame, FxKind};

/// 合成帧边长（2×2）。
const FRAME_SIDE: u16 = 2;

/// 合成 `magic.wzl` 帧表：帧号 + 帧头有符号偏移 + 2×2 RGB565 四角原始值
/// （左上、右上、左下、右下）。
///
/// 450/451/570/571 的 offset 由门下 C1 钉死为 `(-3,5)/(4,-2)/(-2,-3)/(3,4)`；
/// 其余帧保持互不相同，保证「逐帧 offset 各生效」可区分。
const MAGIC_FRAMES: [MagicFrame; 16] = [
    MagicFrame::new(450, (-3, 5), [0xF800, 0xFFE0, 0x07E0, 0x001F]),
    MagicFrame::new(451, (4, -2), [0x001F, 0xF800, 0xFFFF, 0xFFE0]),
    MagicFrame::new(452, (-2, -3), [0x07E0, 0x001F, 0xF800, 0xFFFF]),
    MagicFrame::new(453, (3, 4), [0xFFFF, 0x07E0, 0xFFE0, 0xF800]),
    MagicFrame::new(454, (1, -1), [0xFFE0, 0x07FF, 0x8410, 0xF81F]),
    MagicFrame::new(455, (-4, 2), [0x8410, 0xF81F, 0x07FF, 0xFFE0]),
    MagicFrame::new(570, (-2, -3), [0x07E0, 0x001F, 0xF800, 0xFFFF]),
    MagicFrame::new(571, (3, 4), [0xFFFF, 0x07E0, 0xFFE0, 0xF800]),
    MagicFrame::new(572, (-4, 1), [0xF800, 0x07FF, 0x001F, 0xFFE0]),
    MagicFrame::new(573, (2, -5), [0x07FF, 0xFFE0, 0xF800, 0x001F]),
    MagicFrame::new(574, (-3, -2), [0xFFE0, 0xF81F, 0x07E0, 0xFFFF]),
    MagicFrame::new(575, (0, 0), [0x8410, 0x07E0, 0x001F, 0xF800]),
    MagicFrame::new(576, (1, 3), [0x07E0, 0x8410, 0xFFFF, 0xF800]),
    MagicFrame::new(577, (-5, 2), [0xF81F, 0xF800, 0x07E0, 0x001F]),
    MagicFrame::new(578, (4, -4), [0x001F, 0xFFE0, 0x8410, 0x07FF]),
    MagicFrame::new(579, (2, 2), [0xFFFF, 0x001F, 0x8410, 0x07E0]),
];

/// 一张合成帧：帧号、帧头有符号偏移、四角 RGB565 原始值。
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

/// 进程唯一临时目录（不读 `resource` / `game-assets`，也不与其它测试共享路径）。
///
/// 优先系统临时目录；Windows 上 `TEMP/TMP` 可能被环境策略拒绝写（`PermissionDenied`），
/// 此时回退到仓库内允许的 `target/round68/b/tmp/`（测试产物目录，不入库）。
fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let name = format!("mir-next-r68-b-{tag}-{}-{nanos}", std::process::id());
    let candidates = [
        std::env::temp_dir().join(&name),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/round68/b/tmp")
            .join(&name),
    ];
    for dir in candidates {
        if fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }
    panic!("无法创建进程唯一临时目录（系统临时目录与 target/round68/b/tmp 均不可写）");
}

/// 写合成 `magic.wzl` + `magic.wzx` 并返回目录。
fn write_magic_library(tag: &str) -> PathBuf {
    let dir = unique_dir(tag);
    let (wzl, wzx) = build_wzl(&MAGIC_FRAMES);
    fs::write(dir.join("magic.wzl"), wzl).expect("写合成 magic.wzl");
    fs::write(dir.join("magic.wzx"), wzx).expect("写合成 magic.wzx");
    dir
}

/// 按 `.wzl`/`.wzx` 二进制格式构造一对文件内容。
///
/// `.wzl`：48 字节头 + 每条记录 16 字节记录头（格式5=RGB565、宽高、i16 偏移、
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

/// 构造 zlib 流：只用 DEFLATE 存储块（BTYPE=0），不依赖 `flate2` 依赖。
///
/// `mir-client` 的 dev-dependencies 里没有 `flate2`；测试自己写 zlib 头/存储块/
/// adler32，既保持「真实 `.wzl` 字节格式」又不扩大依赖。
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

/// 帧表查询（缺帧号直接 panic，防止 fixture 与断言漂移）。
fn magic_frame(index: u32) -> &'static MagicFrame {
    MAGIC_FRAMES
        .iter()
        .find(|frame| frame.index == index)
        .expect("合成 magic.wzl 帧表必须覆盖该帧号")
}

/// RGB565 → RGBA 的独立 oracle（与 Crystal `convert16bitTo32bit` 一致：只左移，不复制低位）。
fn expand565(value: u16) -> [u8; 4] {
    let red = ((value >> 11) & 0x1F) << 3;
    let green = ((value >> 5) & 0x3F) << 2;
    let blue = (value & 0x1F) << 3;
    [red as u8, green as u8, blue as u8, 255]
}

/// 把 2×2 四角放到 `top_left`（左上 → 右上 → 左下 → 右下）后得到「已落屏像素」列表。
fn place_quad(top_left: (i32, i32), quad: [u16; 4]) -> Vec<(i32, i32, [u8; 4])> {
    let mut placed = Vec::with_capacity(quad.len());
    for (slot, value) in quad.iter().enumerate() {
        let column = i32::try_from(slot % 2).expect("列号");
        let row = i32::try_from(slot / 2).expect("行号");
        placed.push((top_left.0 + column, top_left.1 + row, expand565(*value)));
    }
    placed
}

/// 整帧期望缓冲（其余像素保持 `Frame::new` 的透明黑 0）。
fn expected_frame(width: u32, height: u32, placed: &[(i32, i32, [u8; 4])]) -> Vec<u8> {
    let mut pixels =
        vec![0u8; usize::try_from(width).expect("宽") * usize::try_from(height).expect("高") * 4];
    for &(x, y, color) in placed {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            continue;
        };
        let offset = (y * usize::try_from(width).expect("宽") + x) * 4;
        let Some(target) = pixels.get_mut(offset..offset + 4) else {
            continue;
        };
        target.copy_from_slice(&color);
    }
    pixels
}

/// 取帧上像素（负坐标视为透明，便于断言「不该画」的位置）。
fn pixel_at(frame: &Frame, x: i32, y: i32) -> [u8; 4] {
    let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
        return [0, 0, 0, 0];
    };
    frame.pixel(x, y).unwrap_or([0, 0, 0, 0])
}

/// G1：合成 WZL/WZX 的有符号偏移经真实读取链读到，并由新入口按整帧平移消费。
#[test]
fn r68_g1_synthetic_wzl_signed_offsets_pixels_are_consumed() {
    let dir = write_magic_library("g1");
    let mut renderer = EntityRenderer::new(EntityLibraries::open_dir(&dir));

    // 真实读取链：目录扫描 → 按需打开 magic.wzl → WzlReader 解码。
    // 缺 feature 时这里返回 None（诊断文本直接说明缺什么），不允许静默 SKIP。
    let info = renderer
        .effect_image(FxKind::Magic, 450)
        .expect("合成 magic.wzl[450] 必须经真实读取链返回图像；请加 --features mir-render/legacy")
        .info;
    assert_eq!(
        (info.width, info.height),
        (FRAME_SIDE, FRAME_SIDE),
        "G1：合成帧必须是 2×2"
    );
    for (index, expected) in [
        (450u32, (-3i16, 5i16)),
        (451, (4, -2)),
        (570, (-2, -3)),
        (571, (3, 4)),
    ] {
        let info = renderer
            .effect_image(FxKind::Magic, index)
            .expect("合成帧必须存在")
            .info;
        assert_eq!(
            (info.offset_x, info.offset_y),
            expected,
            "G1：{index} 的帧头有符号偏移必须原样读回，不能丢符号/截断"
        );
    }

    // —— 450：offset (-3,5)，锚点 (20,20) → 整帧左上角 (17,25) ——
    let mut frame = Frame::new(80, 80);
    let base = (20, 20);
    assert!(
        renderer.draw_effect_with_offset(&mut frame, FxKind::Magic, 450, base.0, base.1),
        "G1：有图必须返回 true"
    );
    let spec = magic_frame(450);
    let top_left = (
        base.0 + i32::from(spec.offset.0),
        base.1 + i32::from(spec.offset.1),
    );
    assert_eq!(
        frame.pixels,
        expected_frame(80, 80, &place_quad(top_left, spec.quad)),
        "G1：450 的 (-3,5) 必须整帧平移 4 个像素，且不得残留/多画"
    );
    // 原点与「只叠一个轴」的位置不得误绘。
    for (x, y) in [
        base,
        (base.0 + i32::from(spec.offset.0), base.1),
        (base.0, base.1 + i32::from(spec.offset.1)),
    ] {
        assert_eq!(
            pixel_at(&frame, x, y),
            [0, 0, 0, 0],
            "G1：不应在 ({x},{y}) 误绘（偏移只能按帧头叠加一次）"
        );
    }

    // —— 451：offset (4,-2)，锚点 (40,40) → 整帧左上角 (44,38)，与 450 不同 ——
    let mut frame_451 = Frame::new(80, 80);
    assert!(
        renderer.draw_effect_with_offset(&mut frame_451, FxKind::Magic, 451, 40, 40),
        "G1：451 有图必须返回 true"
    );
    let spec_451 = magic_frame(451);
    let top_left_451 = (
        40 + i32::from(spec_451.offset.0),
        40 + i32::from(spec_451.offset.1),
    );
    assert_eq!(
        frame_451.pixels,
        expected_frame(80, 80, &place_quad(top_left_451, spec_451.quad)),
        "G1：451 的 (4,-2) 必须整帧平移"
    );
    assert_ne!(
        frame_451.pixels, frame.pixels,
        "G1：450 与 451 的 offset/像素必须逐帧不同"
    );

    // —— 570/571：命中帧同样逐帧消费 offset（(-2,-3) / (3,4)） ——
    let mut frame_570 = Frame::new(80, 80);
    assert!(
        renderer.draw_effect_with_offset(&mut frame_570, FxKind::Magic, 570, 60, 20),
        "G1：570 有图必须返回 true"
    );
    let spec_570 = magic_frame(570);
    let top_left_570 = (
        60 + i32::from(spec_570.offset.0),
        20 + i32::from(spec_570.offset.1),
    );
    assert_eq!(
        frame_570.pixels,
        expected_frame(80, 80, &place_quad(top_left_570, spec_570.quad)),
        "G1：570 的 (-2,-3) 必须整帧平移"
    );

    let mut frame_571 = Frame::new(80, 80);
    assert!(
        renderer.draw_effect_with_offset(&mut frame_571, FxKind::Magic, 571, 60, 50),
        "G1：571 有图必须返回 true"
    );
    let spec_571 = magic_frame(571);
    let top_left_571 = (
        60 + i32::from(spec_571.offset.0),
        50 + i32::from(spec_571.offset.1),
    );
    assert_eq!(
        frame_571.pixels,
        expected_frame(80, 80, &place_quad(top_left_571, spec_571.quad)),
        "G1：571 的 (3,4) 必须整帧平移"
    );
    assert_ne!(
        frame_571.pixels, frame_570.pixels,
        "G1：570 与 571 的 offset/像素必须逐帧不同"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// G2：旧入口语义不变（忽略 offset、返回值原义、裁剪正确），新入口补上屏外锚点落屏，
/// 两者交替重复绘制不累积，缓存 meta 不被改写。
#[test]
fn r68_g2_legacy_draw_effect_ignores_offset_and_crop_semantics() {
    let dir = write_magic_library("g2");
    let mut renderer = EntityRenderer::new(EntityLibraries::open_dir(&dir));

    let meta_before = renderer
        .effect_image(FxKind::Magic, 450)
        .expect("合成 magic.wzl[450] 必须可读；请加 --features mir-render/legacy")
        .info;
    let size_before = renderer
        .effect_size(FxKind::Magic, 450)
        .expect("G2：合成帧尺寸必须可读");
    assert_eq!(
        (size_before.0, size_before.1),
        (u32::from(FRAME_SIDE), u32::from(FRAME_SIDE)),
        "G2：合成帧尺寸"
    );

    // ① 旧入口：带非零 offset 的 450 仍画在调用者坐标 (20,20)，不得平移。
    let spec_450 = magic_frame(450);
    let mut legacy = Frame::new(80, 80);
    assert!(
        renderer.draw_effect(&mut legacy, FxKind::Magic, 450, 20, 20),
        "G2：旧入口返回值原义 = 图是否存在（有图 → true）"
    );
    assert_eq!(
        legacy.pixels,
        expected_frame(80, 80, &place_quad((20, 20), spec_450.quad)),
        "G2：旧 draw_effect 必须忽略 ImageInfo.offset，仍画在调用者坐标"
    );

    // ② 旧入口零 offset（575）与旧入口负偏移：画在原点。
    let mut zero_offset = Frame::new(80, 80);
    let spec_575 = magic_frame(575);
    assert!(
        renderer.draw_effect(&mut zero_offset, FxKind::Magic, 575, 30, 30),
        "G2：零 offset 旧入口必须返回 true"
    );
    assert_eq!(
        zero_offset.pixels,
        expected_frame(80, 80, &place_quad((30, 30), spec_575.quad)),
        "G2：零 offset 旧入口按调用者坐标画"
    );

    // ②b 新入口零 offset（575，帧头 offset=(0,0)）：直接调新入口，返回 true 且画在 (30,30)。
    let mut new_zero = Frame::new(80, 80);
    assert!(
        renderer.draw_effect_with_offset(&mut new_zero, FxKind::Magic, 575, 30, 30),
        "G2：新入口零 offset 必须返回 true"
    );
    assert_eq!(
        new_zero.pixels,
        expected_frame(80, 80, &place_quad((30, 30), spec_575.quad)),
        "G2：新入口零 offset（来源 magic.wzl[575] 帧头 (0,0)）按调用者坐标画，不得额外平移"
    );

    // ②c 新入口负偏移后部分裁剪：450 帧头 (-3,5)，锚点 (2,-6) → 期望左上角 (-1,-1)，
    // 仅四角中的右下 (0,0) 落在 10×10 帧内（先叠加 offset，再按 Frame blit 裁剪）。
    let mut new_clipped = Frame::new(10, 10);
    assert!(
        renderer.draw_effect_with_offset(&mut new_clipped, FxKind::Magic, 450, 2, -6),
        "G2：新入口部分越界但仍落屏必须返回 true"
    );
    let new_clipped_top_left = (
        2 + i32::from(spec_450.offset.0),
        -6 + i32::from(spec_450.offset.1),
    );
    assert_eq!(
        new_clipped_top_left,
        (-1, -1),
        "G2：新入口期望 helper 的锚点 = 调用者坐标 + 帧头 offset（来源 magic.wzl[450]）"
    );
    assert_eq!(
        new_clipped.pixels,
        expected_frame(10, 10, &[(0, 0, expand565(spec_450.quad[3]))]),
        "G2：新入口叠加 offset 后负坐标裁剪与旧入口同源，只能保留右下角 (0,0)"
    );

    // ②d 新入口完全屏外：仍返回 true（存在性），且零落屏。
    let mut new_offscreen = Frame::new(20, 20);
    assert!(
        renderer.draw_effect_with_offset(&mut new_offscreen, FxKind::Magic, 450, -100, 300),
        "G2：新入口完全屏外但有图 → 仍返回 true（返回值原义是存在性）"
    );
    assert!(
        new_offscreen.pixels.iter().all(|&byte| byte == 0),
        "G2：新入口完全屏外不得留下像素"
    );

    // ③ 部分负坐标裁剪：旧入口 (-1,-1) 只有图像右下角 (1,1) 落在帧 (0,0)。
    let mut clipped = Frame::new(10, 10);
    assert!(
        renderer.draw_effect(&mut clipped, FxKind::Magic, 575, -1, -1),
        "G2：部分越界但仍落屏必须返回 true"
    );
    assert_eq!(
        clipped.pixels,
        expected_frame(10, 10, &[(0, 0, expand565(spec_575.quad[3]))]),
        "G2：负坐标裁剪只能保留落在帧内的右下角像素"
    );

    // ④ 完全屏外：旧入口仍返回 true（存在性），不产生任何像素。
    let mut offscreen = Frame::new(20, 20);
    assert!(
        renderer.draw_effect(&mut offscreen, FxKind::Magic, 450, -100, 300),
        "G2：完全屏外但有图 → 仍返回 true（返回值原义是存在性）"
    );
    assert!(
        offscreen.pixels.iter().all(|&byte| byte == 0),
        "G2：完全屏外不得留下像素"
    );

    // ⑤ 缺帧：旧入口/新入口都必须返回 false，且不画任何像素。
    let mut missing = Frame::new(20, 20);
    assert!(
        !renderer.draw_effect(&mut missing, FxKind::Magic, 9_000, 10, 10),
        "G2：缺帧 → false（不画占位方块）"
    );
    assert!(
        !renderer.draw_effect_with_offset(&mut missing, FxKind::Magic, 9_000, 10, 10),
        "G2：新入口缺帧 → false"
    );
    assert!(
        missing.pixels.iter().all(|&byte| byte == 0),
        "G2：缺帧不得留下像素"
    );

    // ⑥ 屏外锚点经 offset 移入屏内：571 的 (3,4) 让锚点 (-2,-3) → 左上角 (1,1)。
    let mut moved_in = Frame::new(10, 10);
    let spec_571 = magic_frame(571);
    assert!(
        renderer.draw_effect_with_offset(&mut moved_in, FxKind::Magic, 571, -2, -3),
        "G2：屏外锚点经 offset 移入屏内仍必须返回 true"
    );
    let moved_top_left = (
        -2 + i32::from(spec_571.offset.0),
        -3 + i32::from(spec_571.offset.1),
    );
    assert_eq!(
        moved_in.pixels,
        expected_frame(10, 10, &place_quad(moved_top_left, spec_571.quad)),
        "G2：锚点屏外、offset 移入屏内后像素必须出现"
    );
    // 同一锚点走旧入口则整帧在屏外：仍返回 true 但零像素。
    let mut legacy_moved = Frame::new(10, 10);
    assert!(
        renderer.draw_effect(&mut legacy_moved, FxKind::Magic, 571, -2, -3),
        "G2：旧入口屏外锚点仍有图 → true"
    );
    assert!(
        legacy_moved.pixels.iter().all(|&byte| byte == 0),
        "G2：旧入口忽略 offset，屏外锚点必须零落屏"
    );

    // ⑦ 极端 i32 坐标不得 panic（saturating 语义 + 裁剪）。
    let mut extreme = Frame::new(16, 16);
    assert!(
        renderer.draw_effect(&mut extreme, FxKind::Magic, 450, i32::MAX, i32::MIN),
        "G2：极端坐标旧入口不得 panic，有图 → true"
    );
    assert!(
        renderer.draw_effect_with_offset(&mut extreme, FxKind::Magic, 571, i32::MAX, i32::MIN),
        "G2：极端坐标新入口不得 panic，有图 → true"
    );
    assert!(
        extreme.pixels.iter().all(|&byte| byte == 0),
        "G2：极端坐标不得落屏"
    );

    // ⑧ 交替新旧入口重复绘制不累积：新入口 (17,25) + 旧入口 (20,20)，重复两轮不变。
    let mut alternating = Frame::new(80, 80);
    assert!(renderer.draw_effect_with_offset(&mut alternating, FxKind::Magic, 450, 20, 20));
    assert!(renderer.draw_effect(&mut alternating, FxKind::Magic, 450, 20, 20));
    let once = alternating.pixels.clone();
    let lit_once = alternating
        .pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] == 255)
        .count();
    assert_eq!(
        lit_once, 8,
        "G2：新入口 4 像素 + 旧入口 4 像素，不得互相覆盖"
    );
    assert!(renderer.draw_effect_with_offset(&mut alternating, FxKind::Magic, 450, 20, 20));
    assert!(renderer.draw_effect(&mut alternating, FxKind::Magic, 450, 20, 20));
    assert_eq!(
        alternating.pixels, once,
        "G2：交替重复绘制不得累积/漂移（偏移只叠加一次）"
    );

    // ⑨ 旧入口不得改写缓存 meta / 尺寸（调用者坐标不变是入口契约）。
    let meta_after = renderer
        .effect_image(FxKind::Magic, 450)
        .expect("G2：缓存 meta 仍应可读")
        .info;
    assert_eq!(
        (meta_after.offset_x, meta_after.offset_y),
        (meta_before.offset_x, meta_before.offset_y),
        "G2：旧入口不得改写缓存帧头 offset"
    );
    assert_eq!(
        renderer.effect_size(FxKind::Magic, 450),
        Some(size_before),
        "G2：旧入口不得改写缓存尺寸"
    );

    let _ = fs::remove_dir_all(&dir);
}
