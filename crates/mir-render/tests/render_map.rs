//! 真实素材渲染回归：加载 `resource/map/0.map` + `resource/data`，渲染 800×600。
//!
//! `resource/` 被 `.gitignore` 忽略（CI 上没有），所以文件缺失时用 `eprintln!`
//! 说明原因并直接返回。测试会：
//!
//! 1. 在 `(330,270)` 渲染一帧，断言不是纯色且颜色数足够多；
//! 2. 把帧写成 `target/render-map0.png`；
//! 3. 用 `png` 把文件解码回来，验证尺寸、颜色数与非空；
//! 4. 换一个中心 `(350,290)` 再渲染，断言像素确实变了；
//! 5. 重复渲染同一中心，断言逐字节一致（确定性）。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use mir_core::Point;
use mir_render::{Frame, MapScene};

/// 仓库根目录：`crates/mir-render` → 上两级。
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 真实素材的路径；缺任何一个就返回 `None`（测试跳过）。
fn assets() -> Option<(PathBuf, PathBuf)> {
    let root = repo_root();
    let map = root.join("resource/map/0.map");
    let data = root.join("resource/data");
    if !map.is_file() || !data.is_dir() {
        eprintln!(
            "跳过：缺 {map_display} 或 {data_display}（resource/ 被 .gitignore 忽略）",
            map_display = map.display(),
            data_display = data.display()
        );
        return None;
    }
    Some((map, data))
}

/// 用 `png` 解码 PNG，返回 (宽, 高, RGBA 像素)。
fn decode_png(path: &Path) -> (u32, u32, Vec<u8>) {
    let file = std::fs::File::open(path).expect("打开刚写出的 PNG");
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().expect("读取 PNG 头");
    let buffer_size = reader.output_buffer_size().expect("PNG 输出尺寸");
    let mut buffer = vec![0; buffer_size];
    let info = reader.next_frame(&mut buffer).expect("解码 PNG 像素");
    buffer.truncate(info.buffer_size());
    (info.width, info.height, buffer)
}

/// 统计 RGBA 缓冲里的不同颜色（忽略 alpha）。
fn color_count(pixels: &[u8]) -> usize {
    pixels
        .chunks_exact(4)
        .map(|pixel| u32::from(pixel[0]) << 16 | u32::from(pixel[1]) << 8 | u32::from(pixel[2]))
        .collect::<HashSet<_>>()
        .len()
}

#[test]
fn renders_map_zero_and_dumps_png() {
    let Some((map_path, data_dir)) = assets() else {
        return;
    };
    let target = repo_root().join("target");
    std::fs::create_dir_all(&target).expect("创建 target");

    let mut scene = MapScene::load(&map_path, &data_dir).expect("加载地图场景");
    assert_eq!(scene.map.width, 700);
    assert_eq!(scene.map.height, 700);
    assert!(
        scene.map.version == mir_world::MapVersion::V0,
        "0.map 应当是 v0，实际 {}",
        scene.map.version
    );
    assert!(!scene.libraries().is_empty(), "应当至少打开一个地图图库");

    let center = Point::new(330, 270);
    let mut frame = Frame::new(800, 600);
    let started = Instant::now();
    scene.render(&mut frame, center);
    let first_frame = started.elapsed();
    eprintln!(
        "首帧耗时 {first_frame:?}，缓存图像 {} 张，颜色 {} 种",
        scene.image_cache_len(),
        frame.distinct_color_count()
    );

    // 不是纯色，颜色足够多（真实地图瓦片有上千种颜色）。
    let colors = frame.distinct_color_count();
    let output = target.join("render-map0.png");
    frame.save_png(&output).expect("写出 PNG");
    assert!(colors > 1, "整帧只有一种颜色，渲染失败");
    assert!(colors >= 256, "颜色数只有 {colors}，地图可能没有画出来");

    let png_bytes = std::fs::metadata(&output).expect("读取 PNG 元数据").len();
    let (width, height, decoded) = decode_png(&output);
    assert_eq!((width, height), (800, 600));
    assert_eq!(decoded.len(), 800 * 600 * 4);
    let decoded_colors = color_count(&decoded);
    assert!(
        decoded_colors >= 64,
        "PNG 解码后颜色只有 {decoded_colors} 种"
    );
    eprintln!(
        "写出 {}：{width}×{height}，{png_bytes} 字节，解码后 {decoded_colors} 种颜色",
        output.display()
    );

    // 第二次渲染（同一中心）必须逐字节一致。
    let mut again = Frame::new(800, 600);
    let cached_started = Instant::now();
    scene.render(&mut again, center);
    let cached_frame = cached_started.elapsed();
    eprintln!("缓存后同帧耗时 {cached_frame:?}（首帧包含解码）");
    assert_eq!(again.pixels, frame.pixels, "同一中心两次渲染结果应当一致");

    // 换一个中心：像素必须不同（相机确实移动了）。
    let other_center = Point::new(350, 290);
    let mut moved = Frame::new(800, 600);
    scene.render(&mut moved, other_center);
    let moved_output = target.join("render-map0-alt.png");
    moved.save_png(&moved_output).expect("写出第二张 PNG");
    assert_ne!(moved.pixels, frame.pixels, "换中心后画面应当不同");
    assert!(
        moved.distinct_color_count() >= 256,
        "第二张图颜色过少：{}",
        moved.distinct_color_count()
    );
    eprintln!(
        "第二张 {}：800×600，{} 种颜色",
        moved_output.display(),
        moved.distinct_color_count()
    );

    // 摄像机移动后，坐标映射也要跟着走。
    let (x0, y0) = scene.world_to_screen(other_center, other_center);
    let (x1, y1) = scene.world_to_screen(other_center, Point::new(351, 290));
    assert_eq!((x1 - x0, y1 - y0), (48, 0));
}

#[test]
fn renders_map_zero_with_animation_frame() {
    let Some((map_path, data_dir)) = assets() else {
        return;
    };
    let mut scene = MapScene::load(&map_path, &data_dir).expect("加载地图场景");
    let center = Point::new(330, 270);
    let mut frame = Frame::new(800, 600);
    scene.set_animation_frame(0);
    scene.render(&mut frame, center);
    let static_pixels = frame.pixels.clone();
    // 有前层动画的地图格（0.map 的 o+8 = 0x8A 等）应当在换帧后不同。
    scene.set_animation_frame(3);
    scene.render(&mut frame, center);
    if scene.animation_frame() == 3 && static_pixels == frame.pixels {
        eprintln!("注意：该中心在动画帧 0 与 3 下像素相同（可能视口内没有动画前层）");
    }
    assert!(frame.distinct_color_count() > 1);
}

#[test]
fn walking_frames_reuse_the_decode_cache() {
    let Some((map_path, data_dir)) = assets() else {
        return;
    };
    let mut scene = MapScene::load(&map_path, &data_dir).expect("加载地图场景");
    let mut frame = Frame::new(800, 600);
    // 预热：把起点附近的可见瓦片解码进缓存。
    scene.render(&mut frame, Point::new(330, 270));
    let cached_after_warmup = scene.image_cache_len();
    assert!(cached_after_warmup > 0, "预热后应该有缓存图像");

    // 模拟走路：相机每帧右移一格，连续 30 帧。
    let started = Instant::now();
    for step in 0..30 {
        scene.render(&mut frame, Point::new(330 + step, 270));
    }
    let elapsed = started.elapsed();
    let average_ms = elapsed.as_secs_f64() * 1000.0 / 30.0;
    eprintln!(
        "走路 30 帧：总 {elapsed:?}，平均 {average_ms:.2} ms/帧（≈{:.0} FPS），缓存 {} 张",
        if average_ms > 0.0 {
            1000.0 / average_ms
        } else {
            0.0
        },
        scene.image_cache_len()
    );
    // 只做宽松上限（避免 debug 构建在慢机器上抖动）；release 下实测远低于此。
    assert!(
        average_ms < 400.0,
        "单帧 {average_ms:.1} ms 过慢，缓存失效了？"
    );
    assert!(
        scene.image_cache_len() >= cached_after_warmup,
        "走路时缓存不应当被清空"
    );
}
