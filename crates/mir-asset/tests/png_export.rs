//! PNG 导出冒烟测试。
//!
//! `mir-tools image-dump` 用的是同样的编码参数（RGBA8、8 位深），这里把解码结果写进
//! 仓库的 `target/png-export/`，再用 `png` 解码回来逐字节比较，确保导出的文件可被外部工具打开。

use std::fs::File;
use std::path::{Path, PathBuf};

use mir_asset::{Image, ImageInfo, ImageLibrary, format};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn output_dir() -> PathBuf {
    repo_root().join("target/png-export")
}

/// 没有真实素材时用的 2×2 RGBA（左上不透明红、右下半透明白）。
fn synthetic_image() -> Image {
    Image {
        info: ImageInfo {
            width: 2,
            height: 2,
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
        pixels: vec![
            255, 0, 0, 255, 0, 255, 0, 255, // 第一行：红、绿
            0, 0, 255, 255, 255, 255, 255, 0, // 第二行：蓝、全透明白
        ],
    }
}

/// 优先用真实素材解码一张图，失败时退回合成图。
fn sample_image() -> Image {
    let candidates = [
        (repo_root().join("resource/data/anitiles1.wzl"), 0usize),
        (repo_root().join("resource/data/cbohair_ck.wzl"), 0usize),
        (
            repo_root().join("resource/community/clgforever910-mir2database/Jev/Data/dnitems.Lib"),
            0usize,
        ),
    ];
    for (path, index) in candidates {
        let Ok(library) = ImageLibrary::open(&path) else {
            continue;
        };
        let Ok(image) = library.image(index) else {
            continue;
        };
        if !image.is_empty() {
            println!("用 {} 第 {index} 张导出", path.display());
            return image;
        }
    }
    eprintln!("没有可用的真实素材，改用 2×2 合成图");
    synthetic_image()
}

/// 与 `mir-tools image-dump` 相同的写法：RGBA8、8 位深。
fn encode_png(path: &Path, image: &Image) {
    let file = File::create(path).expect("创建 PNG 文件");
    let mut encoder = png::Encoder::new(
        file,
        u32::from(image.info.width),
        u32::from(image.info.height),
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("写 PNG 头");
    writer.write_image_data(&image.pixels).expect("写 PNG 像素");
}

#[test]
fn png_export_smoke() {
    let directory = output_dir();
    std::fs::create_dir_all(&directory).expect("创建 target/png-export");
    let path = directory.join("mir-asset-smoke.png");

    let image = sample_image();
    encode_png(&path, &image);

    let file = File::open(&path).expect("打开刚写的 PNG");
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().expect("读 PNG 头");
    let buffer_size = reader.output_buffer_size().expect("PNG 输出缓冲大小");
    let mut buffer = vec![0u8; buffer_size];
    let frame = reader.next_frame(&mut buffer).expect("读 PNG 像素");
    assert_eq!(frame.width, u32::from(image.info.width));
    assert_eq!(frame.height, u32::from(image.info.height));
    assert_eq!(frame.color_type, png::ColorType::Rgba);
    assert_eq!(frame.bit_depth, png::BitDepth::Eight);

    let expected = usize::from(image.info.width) * usize::from(image.info.height) * 4;
    assert_eq!(expected, image.pixels.len());
    assert_eq!(&buffer[..expected], image.pixels.as_slice());
    println!(
        "PNG 冒烟测试通过：{}（{}×{}，{} 字节）",
        path.display(),
        image.info.width,
        image.info.height,
        std::fs::metadata(&path).map_or(0, |metadata| metadata.len())
    );
}
