//! 小地图回归：合成 `mmap.wzl` 图库 + 真实素材冒烟。
//!
//! 两个测试：
//!
//! 1. [`synthetic_mmap_library_renders_player_dot`]：现场拼一个最小的 `.wzl` + `.wzx`
//!    （`mir-asset` 只读不写，所以这里自己按 `WeMadeLibrary.cs` 的记录格式写），
//!    用 [`MinimapScene::open_dir`] 打开，画到 `Frame` 上，断言底图颜色与黄色玩家点都在。
//! 2. [`real_mmap_decodes_at_least_one_frame`]：真实 `resource/data/mmap.wzl`（3273 帧），
//!    解至少一张图并打印尺寸；`resource/` 被 `.gitignore` 忽略，CI 上缺文件时打印跳过。
//!
//! `.wzl` 记录格式见 `crates/mir-asset/src/wzl.rs` 的模块文档
//! （`WeMadeLibrary.cs:37,140-147,313-331`）：16 字节记录头 + zlib 数据，
//! 像素行按 4 字节对齐、自下而上。

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::ZlibEncoder;
use mir_core::Point;
use mir_render::minimap::{MarkerKind, MinimapMarker, MinimapScene, MinimapView, PLAYER_COLOR};
use mir_render::{Frame, Image};

/// 仓库根目录：`crates/mir-render` → 上两级。
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 合成 `.wzl` 的记录头长度（`WeMadeLibrary.cs:37` 的 `ImageStructureSize[1]`）。
const RECORD_HEADER_LEN: usize = 16;

/// 把一张纯色 RGB565 图写成 `.wzl` 记录（16 字节头 + zlib 像素）。
fn solid_record(width: u16, height: u16, rgb565: u16) -> Vec<u8> {
    // `WidthBytes = (((w * 16) + 31) >> 5) * 4`（`WeMadeLibrary.cs:231-234`）。
    let row_bytes = ((usize::from(width) * 16 + 31) >> 5) * 4;
    let mut raw = Vec::with_capacity(row_bytes * usize::from(height));
    for _ in 0..height {
        let mut row = Vec::with_capacity(row_bytes);
        for _ in 0..width {
            row.extend_from_slice(&rgb565.to_le_bytes());
        }
        row.resize(row_bytes, 0);
        raw.extend_from_slice(&row);
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&raw).expect("压缩像素");
    let compressed = encoder.finish().expect("结束压缩");

    let mut record = Vec::with_capacity(RECORD_HEADER_LEN + compressed.len());
    record.push(5); // format = 5 → RGB565（`WeMadeLibrary.cs:319`）
    record.extend_from_slice(&[0, 0, 0]); // 保留 3 字节（`:320`）
    record.extend_from_slice(&width.to_le_bytes());
    record.extend_from_slice(&height.to_le_bytes());
    record.extend_from_slice(&0i16.to_le_bytes()); // offset x
    record.extend_from_slice(&0i16.to_le_bytes()); // offset y
    record.extend_from_slice(
        &u32::try_from(compressed.len())
            .expect("压缩长度放得进 u32")
            .to_le_bytes(),
    );
    record.extend_from_slice(&compressed);
    record
}

/// 在 `directory` 里写一对 `mmap.wzl` + `mmap.wzx`。
///
/// `frames` 里值为 `None` 的帧写偏移 0（`mir-asset` 把 0 当作「没有这张图」，
/// `wzl.rs` 的 `record()`），所以 `.wzl` 开头先垫一段空白，保证真实记录不会落在 0 上。
fn write_library(directory: &Path, frames: &[Option<(u16, u16, u16)>]) {
    std::fs::create_dir_all(directory).expect("创建临时素材目录");
    // 偏移 0 = 没有图像，真实语料的第一条记录也不会在 0（头之前有 0 字节，但索引里用 0 表示空）。
    let mut data = vec![0u8; RECORD_HEADER_LEN];
    let mut offsets = Vec::with_capacity(frames.len());
    for frame in frames {
        match frame {
            Some((width, height, color)) => {
                offsets.push(u32::try_from(data.len()).expect("偏移放得进 u32"));
                data.extend_from_slice(&solid_record(*width, *height, *color));
            }
            None => offsets.push(0),
        }
    }
    std::fs::write(directory.join("mmap.wzl"), &data).expect("写 wzl");

    // `.wzx`：48 字节头（偏移 44 是图像数量）+ 每帧一个 u32 偏移。
    let mut index = vec![0u8; 48];
    let count = u32::try_from(frames.len()).expect("帧数放得进 u32");
    index[44..48].copy_from_slice(&count.to_le_bytes());
    for offset in offsets {
        index.extend_from_slice(&offset.to_le_bytes());
    }
    std::fs::write(directory.join("mmap.wzx"), &index).expect("写 wzx");
}

/// 合成素材：3 号帧是 160×120 的纯绿，0 号帧是空图。
fn synthetic_directory() -> PathBuf {
    let directory = std::env::temp_dir().join(format!("mir-render-minimap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    // RGB565 0x07E0：R = 0、G = (0x3F << 2) = 252、B = 0（`expand_rgb565`，`WeMadeLibrary.cs:223-229`）。
    write_library(&directory, &[None, None, None, Some((160, 120, 0x07E0))]);
    directory
}

#[test]
fn synthetic_mmap_library_renders_player_dot() {
    let directory = synthetic_directory();
    let (mut scene, reason) = MinimapScene::open_dir(&directory);
    assert!(reason.is_none(), "合成图库应当能打开：{reason:?}");
    assert!(scene.is_available());
    let expected_path = directory.join("mmap.wzl");
    assert_eq!(scene.path(), Some(expected_path.as_path()));

    // 空帧与越界帧都没有图像。
    assert!(scene.image_size(0).is_none(), "偏移 0 的帧应视为没有图像");
    assert!(scene.image_size(99).is_none(), "越界帧号应返回 None");
    assert_eq!(
        scene.image_size(3),
        Some((160, 120)),
        "3 号帧应当是 160×120"
    );

    // 地图 400×300 格、图像 160×120 → 每格 0.4 像素；玩家 (200,150) 落在视窗正中。
    let view = MinimapView {
        image_index: 3,
        map_width: 400,
        map_height: 300,
        player: Point::new(200, 150),
        markers: vec![MinimapMarker {
            cell: Point::new(250, 150),
            kind: MarkerKind::Monster,
        }],
    };
    let mut frame = Frame::new(200, 200);
    frame.clear([0, 0, 0, 255]);
    assert!(scene.draw(&mut frame, 10, 10, &view), "应当画出底图");

    // 底图颜色：纯绿（RGB565 展开不做低位复制，见 `expand_rgb565`）。
    assert_eq!(frame.pixel(10 + 2, 10 + 2), Some([0, 252, 0, 255]));
    // 玩家点：2×2 黄色，正好在视窗中心（60, 54）。
    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        assert_eq!(
            frame.pixel(10 + 60 + dx, 10 + 54 + dy),
            Some(PLAYER_COLOR),
            "玩家点像素（{dx},{dy}）应当是黄色"
        );
    }
    // 怪物点：玩家右 50 格 = 20 像素 → (80, 54)。
    assert_eq!(
        frame.pixel(10 + 80, 10 + 54),
        Some([255, 0, 0, 255]),
        "怪物点应当是红色（MainDialogs.cs:1959）"
    );

    // 同一张图重复画必须逐字节一致（解码已缓存）。
    let mut again = Frame::new(200, 200);
    again.clear([0, 0, 0, 255]);
    assert!(scene.draw(&mut again, 10, 10, &view));
    assert_eq!(again.pixels, frame.pixels);

    std::fs::remove_dir_all(&directory).ok();
}

#[test]
fn from_images_is_usable_without_a_library_file() {
    // 没有 `.wzl` 时的合成路径：直接把解码好的图像塞进场景（无头测试用）。
    let mut frames = BTreeMap::new();
    frames.insert(7, {
        let mut pixels = Vec::new();
        for _ in 0..(8 * 8) {
            pixels.extend_from_slice(&[10, 20, 30, 255]);
        }
        Image {
            info: mir_asset::ImageInfo {
                width: 8,
                height: 8,
                offset_x: 0,
                offset_y: 0,
                format: 0,
                shadow_x: 0,
                shadow_y: 0,
                reserved: [0; 3],
                has_mask: false,
                data_offset: 0,
                data_len: 0,
            },
            pixels,
        }
    });
    let mut scene = MinimapScene::from_images(frames);
    assert!(scene.is_available());
    assert_eq!(scene.image_size(7), Some((8, 8)));
    assert!(scene.image_size(8).is_none());
}

#[test]
fn real_mmap_decodes_at_least_one_frame() {
    let data = repo_root().join("resource/data");
    if !data.join("mmap.wzl").is_file() {
        eprintln!(
            "跳过：缺 {}（resource/ 被 .gitignore 忽略）",
            data.display()
        );
        return;
    }
    let (mut scene, reason) = MinimapScene::open_dir(&data);
    assert!(reason.is_none(), "打开真实 mmap.wzl 失败：{reason:?}");
    assert!(scene.is_available());

    // 真实图库有 3273 帧；至少解出第一张非空图并打印尺寸。
    let mut decoded = 0;
    let mut first_size = None;
    for index in 0..64u16 {
        if let Some((width, height)) = scene.image_size(index) {
            if decoded == 0 {
                first_size = Some((index, width, height));
            }
            decoded += 1;
        }
    }
    assert!(decoded > 0, "mmap.wzl 前 64 帧里一张图都解不出来");
    let (index, width, height) = first_size.expect("至少有一帧");
    eprintln!("mmap.wzl 前 64 帧解出 {decoded} 张图；第一张是 {index} 号 {width}×{height}");

    // 用真实底图画一帧小地图，证明投影公式在真实尺寸下也能落进视窗。
    let view = MinimapView {
        image_index: index,
        map_width: 700,
        map_height: 700,
        player: Point::new(330, 270),
        markers: vec![MinimapMarker {
            cell: Point::new(340, 270),
            kind: MarkerKind::Npc,
        }],
    };
    let mut frame = Frame::new(160, 160);
    frame.clear([0, 0, 0, 255]);
    let drew = scene.draw(&mut frame, 10, 10, &view);
    assert!(drew, "真实素材应当画得出小地图底图");
    let non_black = frame
        .pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0)
        .count();
    assert!(non_black > 1_000, "小地图非黑像素太少：{non_black}");
    let output = repo_root().join("target/minimap-real.png");
    std::fs::create_dir_all(repo_root().join("target")).ok();
    frame
        .save_png(&output)
        .expect("写出 target/minimap-real.png");
    eprintln!("写出 {}", output.display());
}
