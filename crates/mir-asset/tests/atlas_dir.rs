//! 图集（`atlas.png` + `atlas.json`）与散装 PNG 的像素必须逐字节一致；
//! 目录里同时存在 `atlas.json` 与 `index.json` 时，`UnpackedDirSet` 必须优先用图集。

use std::io::Write;
use std::path::{Path, PathBuf};

use mir_asset::{LibrarySource, UnpackedDirSet, format};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mir2-atlas-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_png(path: &Path, width: u32, height: u32, pixels: &[u8]) {
    let file = std::fs::File::create(path).expect("create png");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(pixels).expect("png data");
}

fn frame_pixels(width: u16, height: u16, seed: u8) -> Vec<u8> {
    (0..usize::from(width) * usize::from(height) * 4)
        .map(|index| u8::try_from(index % 251).unwrap_or(0).wrapping_add(seed))
        .collect()
}

#[test]
fn atlas_pixels_match_unpacked_pngs_and_are_preferred() {
    let dir = temp_dir("prefer");
    // 3 帧：2×2、3×1、1×3（尺寸不同，验证切图不串行）
    let sizes = [(2u16, 2u16), (3, 1), (1, 3)];
    let mut frames_json = Vec::new();
    let mut pngs = Vec::new();
    let mut atlas_width = 0u32;
    let max_height = 3u32;
    for (index, (width, height)) in sizes.iter().enumerate() {
        let pixels = frame_pixels(*width, *height, u8::try_from(index * 7).unwrap_or(0));
        write_png(
            &dir.join(format!("{index:04}.png")),
            u32::from(*width),
            u32::from(*height),
            &pixels,
        );
        frames_json.push(serde_json::json!({
            "index": index,
            "png": format!("{index:04}.png"),
            "width": width,
            "height": height,
            "offset_x": -3,
            "offset_y": 5,
            "shadow_x": 0,
            "shadow_y": 0,
            "has_mask": false,
            "format_code": format::RGB565,
            "empty": false,
        }));
        atlas_width += u32::from(*width);
        pngs.push((index, pixels));
    }
    // 图集：三帧横向排列，一张 atlas_width × 3 的纹理
    let mut canvas = vec![0u8; atlas_width as usize * max_height as usize * 4];
    let mut x = 0u32;
    let mut atlas_frames = Vec::new();
    for (index, pixels) in &pngs {
        let (width, height) = sizes[*index];
        for row in 0..u32::from(height) {
            let src = (row * u32::from(width) * 4) as usize;
            let dst = ((row * atlas_width + x) * 4) as usize;
            canvas[dst..dst + usize::from(width) * 4]
                .copy_from_slice(&pixels[src..src + usize::from(width) * 4]);
        }
        atlas_frames.push(serde_json::json!({
            "index": index,
            "x": x,
            "y": 0,
            "w": width,
            "h": height,
            "offset_x": -3,
            "offset_y": 5,
            "shadow_x": 0,
            "shadow_y": 0,
            "has_mask": false,
        }));
        x += u32::from(width);
    }
    write_png(&dir.join("atlas.png"), atlas_width, max_height, &canvas);
    let atlas_json = serde_json::json!({
        "version": 1,
        "lib": "items.wzl",
        "entity": "test",
        "texture": "atlas.png",
        "width": atlas_width,
        "height": max_height,
        "frames": atlas_frames,
    });
    let mut file = std::fs::File::create(dir.join("atlas.json")).expect("create atlas.json");
    file.write_all(
        serde_json::to_string_pretty(&atlas_json)
            .expect("json")
            .as_bytes(),
    )
    .expect("write atlas.json");
    std::fs::write(
        dir.join("index.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "version": 1,
            "lib": "items.wzl",
            "count": 3,
            "written_png": 3,
            "images": frames_json,
        }))
        .expect("json"),
    )
    .expect("write index.json");

    // 1) 目录同时有 atlas.json 与 index.json → 必须走图集
    let set = UnpackedDirSet::open(std::slice::from_ref(&dir)).expect("open dir set");
    assert_eq!(
        set.atlas_dir_count(),
        1,
        "有 atlas.json 的目录必须被识别为图集"
    );
    assert_eq!(set.len(), 3);
    for (index, pixels) in &pngs {
        let image = set.image(*index).expect("atlas image");
        assert_eq!(
            &image.pixels, pixels,
            "第 {index} 帧像素必须与散装 PNG 一致"
        );
        let info = set.info(*index).expect("atlas info");
        assert_eq!((info.offset_x, info.offset_y), (-3, 5));
    }

    // 2) `LibrarySource::from_dirs` 复用同一逻辑
    let source = LibrarySource::from_dirs(std::slice::from_ref(&dir)).expect("from dirs");
    assert!(source.is_unpacked_dirs());
    assert_eq!(source.len(), 3);

    std::fs::remove_dir_all(&dir).ok();
}
