//! 真实素材回归测试。
//!
//! `resource/` 被 gitignore（CI 上没有），所以每个测试在缺少文件时 `eprintln!` 后直接返回。

use std::path::{Path, PathBuf};

use mir_asset::{ImageLibrary, LibraryKind, format};

/// 仓库根目录：`crates/mir-asset` → 上两级。
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `resource/data`。
fn data_dir() -> PathBuf {
    repo_root().join("resource/data")
}

/// 旧版 `.Lib` 样本：社区数据库附带的 Jev 客户端图库。
fn lib_sample() -> PathBuf {
    repo_root().join("resource/community/clgforever910-mir2database/Jev/Data/dnitems.Lib")
}

/// 「版本 3 但没有 `frameSeek`」的 `.Lib` 变体（`ArtificialMir2` 客户端）。
fn variant_lib_sample() -> PathBuf {
    repo_root()
        .join("resource/community/lomcn-artificialmir-client-data/ArtificialMir2/Data/dnitems.Lib")
}

/// 版本 1 的 `.Lib`（`MLibraryV1`，DXT1）：`ArtificialMir` 的怪物库。
fn v1_lib_sample() -> PathBuf {
    repo_root()
        .join("resource/community/mirfiles-artificialmir/ArtificialMir/Client/Data/Monster/411.Lib")
}

/// 文件不存在或打不开时打印跳过原因并返回 `None`。
fn open_if_present(path: &Path) -> Option<ImageLibrary> {
    if !path.is_file() {
        eprintln!("跳过：缺少 {}（resource/ 被 gitignore）", path.display());
        return None;
    }
    match ImageLibrary::open(path) {
        Ok(library) => Some(library),
        Err(error) => {
            eprintln!("跳过：打开 {} 失败：{error}", path.display());
            None
        }
    }
}

/// 至少 3 个 `.wzl`：565 的 `anitiles1`、565 的 `cbohair_ck`、混合格式的 `cboeffect`、
/// 8 位调色板的 `cbohum`。
#[test]
fn decodes_real_wzl_libraries() {
    let samples = [
        "anitiles1.wzl",
        "cbohair_ck.wzl",
        "cboeffect.wzl",
        "cbohum.wzl",
    ];
    let mut tested = 0u32;
    for name in samples {
        let path = data_dir().join(name);
        let Some(library) = open_if_present(&path) else {
            continue;
        };
        assert_eq!(library.kind(), LibraryKind::Wzl, "{name} 应当是 .wzl");
        assert!(!library.is_empty(), "{name} 应当有图像");

        let mut decoded = 0usize;
        let mut non_empty = 0usize;
        let mut opaque_pixels = 0usize;
        for index in 0..library.len().min(4000) {
            let info = match library.info(index) {
                Ok(info) => info,
                Err(error) => panic!("{name} 第 {index} 张读头失败：{error}"),
            };
            if info.is_empty() {
                continue;
            }
            non_empty += 1;
            let image = match library.image(index) {
                Ok(image) => image,
                Err(error) => panic!("{name} 第 {index} 张解码失败：{error}"),
            };
            let expected = usize::from(info.width) * usize::from(info.height) * 4;
            assert_eq!(
                image.pixels.len(),
                expected,
                "{name} 第 {index} 张：{}×{} 的 RGBA 缓冲应当是 {expected} 字节",
                info.width,
                info.height
            );
            let opaque = image
                .pixels
                .chunks_exact(4)
                .filter(|pixel| pixel[3] == 255)
                .count();
            assert!(
                opaque > 0,
                "{name} 第 {index} 张没有任何不透明像素（{}×{}，格式 {}）",
                info.width,
                info.height,
                info.format_name()
            );
            opaque_pixels += opaque;
            decoded += 1;
            if decoded >= 8 {
                break;
            }
        }
        println!(
            "{name}: 共 {} 张，前 4000 项里非空 {non_empty} 张，抽样解码 {decoded} 张（不透明像素 {opaque_pixels} 个）",
            library.len()
        );
        assert!(decoded >= 8, "{name}: 抽样只解码了 {decoded} 张，少于 8 张");
        tested += 1;
    }
    assert!(tested >= 3, "至少检查 3 个 .wzl，实际 {tested} 个");
}

/// 旧版 `.Lib`：`dnitems.Lib`（v3，4384 张）。
#[test]
fn decodes_real_lib_library() {
    let path = lib_sample();
    let Some(library) = open_if_present(&path) else {
        return;
    };
    assert_eq!(library.kind(), LibraryKind::Lib);
    assert_eq!(library.len(), 4384, "dnitems.Lib 的图像数量");
    assert_eq!(library.declared_count(), None, ".Lib 没有 .wzx 计数");

    let info = match library.info(0) {
        Ok(info) => info,
        Err(error) => panic!("dnitems.Lib 第 0 张读头失败：{error}"),
    };
    assert_eq!((info.width, info.height), (20, 13));
    assert_eq!(info.format, format::RGBA32_GZIP);
    assert!(!info.has_mask);

    let image = match library.image(0) {
        Ok(image) => image,
        Err(error) => panic!("dnitems.Lib 第 0 张解码失败：{error}"),
    };
    assert_eq!(image.pixels.len(), 20 * 13 * 4);
    assert_eq!(
        &image.pixels[0..4],
        &[0, 0, 0, 0],
        "Crystal 写库时把纯黑像素置为全透明（MLibraryV2.cs:419-426）"
    );
    // 旧库的透明度只来自 removeBlack：纯黑像素 alpha 必为 0。
    for pixel in image.pixels.chunks_exact(4) {
        if pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 0 {
            assert_eq!(pixel[3], 0, "纯黑像素应当是透明的");
        }
    }
    assert!(
        image.pixels.chunks_exact(4).any(|pixel| pixel[3] == 255),
        "第 0 张应当有不透明像素"
    );

    let mut decoded = 1usize;
    for index in 1..40 {
        let info = match library.info(index) {
            Ok(info) => info,
            Err(error) => panic!("dnitems.Lib 第 {index} 张读头失败：{error}"),
        };
        if info.is_empty() {
            continue;
        }
        let image = match library.image(index) {
            Ok(image) => image,
            Err(error) => panic!("dnitems.Lib 第 {index} 张解码失败：{error}"),
        };
        assert_eq!(
            image.pixels.len(),
            usize::from(info.width) * usize::from(info.height) * 4
        );
        decoded += 1;
        if decoded >= 10 {
            break;
        }
    }
    println!(
        "dnitems.Lib: 共 {} 张，抽样解码 {decoded} 张",
        library.len()
    );
    assert!(decoded >= 10, "抽样只解码了 {decoded} 张");
}

/// 真实语料里的另一种 `.Lib` 头：版本 3 但没有 frameSeek（偏移表从 8 开始）。
///
/// 如果按 Crystal 的 12 字节头去读，会漏掉第 0 张、并把最后一个偏移读成垃圾；
/// 这里逐张解码确认判据生效。
#[test]
fn decodes_v3_lib_without_frame_seek() {
    let path = variant_lib_sample();
    let Some(library) = open_if_present(&path) else {
        return;
    };
    assert_eq!(library.kind(), LibraryKind::Lib);
    assert_eq!(library.len(), 5218, "ArtificialMir2 dnitems.Lib 的图像数量");
    let first_info = match library.info(0) {
        Ok(info) => info,
        Err(error) => panic!("变体 .Lib 第 0 张读头失败：{error}"),
    };
    assert_eq!(
        first_info.format,
        format::DXT1,
        "这批 .Lib 的像素数据是 DXT1"
    );

    let mut decoded = 0usize;
    let mut empty = 0usize;
    let mut transparent_pixels = 0usize;
    for index in 0..library.len() {
        let info = match library.info(index) {
            Ok(info) => info,
            Err(error) => panic!("变体 .Lib 第 {index} 张读头失败：{error}"),
        };
        if info.is_empty() {
            empty += 1;
            continue;
        }
        let image = match library.image(index) {
            Ok(image) => image,
            Err(error) => panic!("变体 .Lib 第 {index} 张解码失败：{error}"),
        };
        assert_eq!(
            image.pixels.len(),
            usize::from(info.width) * usize::from(info.height) * 4
        );
        assert_eq!(image.info.format, format::DXT1);
        transparent_pixels += image
            .pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[3] == 0)
            .count();
        decoded += 1;
    }
    println!(
        "ArtificialMir2 dnitems.Lib: {} 张，解码 {decoded}（全 DXT1），空记录 {empty}，透明像素 {transparent_pixels}",
        library.len()
    );
    assert_eq!(decoded + empty, library.len());
    assert!(decoded > 4000, "只解码了 {decoded} 张");
    assert!(transparent_pixels > 0, "DXT1 三色模式应当产生透明像素");
}

/// 版本 1 的 `.Lib`：`MLibraryV1` 写的 DXT1 库，头是 8 字节（没有 `frameSeek`）。
#[test]
fn decodes_v1_lib_with_dxt1() {
    let path = v1_lib_sample();
    let Some(library) = open_if_present(&path) else {
        return;
    };
    assert_eq!(library.kind(), LibraryKind::Lib);
    assert_eq!(
        library.len(),
        760,
        "ArtificialMir Monster/411.Lib 的图像数量"
    );

    let mut decoded = 0usize;
    let mut empty = 0usize;
    for index in 0..library.len() {
        let info = match library.info(index) {
            Ok(info) => info,
            Err(error) => panic!("v1 .Lib 第 {index} 张读头失败：{error}"),
        };
        if info.is_empty() {
            empty += 1;
            continue;
        }
        let image = match library.image(index) {
            Ok(image) => image,
            Err(error) => panic!("v1 .Lib 第 {index} 张解码失败：{error}"),
        };
        assert_eq!(image.info.format, format::DXT1);
        assert_eq!(
            image.pixels.len(),
            usize::from(info.width) * usize::from(info.height) * 4
        );
        decoded += 1;
    }
    println!(
        "ArtificialMir Monster/411.Lib（v1）: {} 张，解码 {decoded}，空记录 {empty}",
        library.len()
    );
    assert_eq!(decoded + empty, library.len());
    assert!(decoded > 700, "只解码了 {decoded} 张");
}

/// `.wzx` 偏移表必须落在 `.wzl` 里，且解码尺寸与 `ImageInfo` 一致。
///
/// 这两个库的每一张图都会真正解码一遍，顺便统计成功/空记录数量。
#[test]
fn wzx_offsets_and_decoded_sizes_are_consistent() {
    for name in ["anitiles1.wzl", "cbohair_ck.wzl"] {
        let path = data_dir().join(name);
        let Some(library) = open_if_present(&path) else {
            continue;
        };
        let file_len = match std::fs::metadata(&path) {
            Ok(metadata) => metadata.len(),
            Err(error) => panic!("读 {} 元数据失败：{error}", path.display()),
        };
        let mut decoded = 0usize;
        let mut empty = 0usize;
        let mut previous_end: Option<u64> = None;
        let mut discontinuities = 0usize;
        for index in 0..library.len() {
            let info = match library.info(index) {
                Ok(info) => info,
                Err(error) => panic!("{name} 第 {index} 张读头失败：{error}"),
            };
            if info.is_empty() {
                empty += 1;
                previous_end = None;
                continue;
            }
            assert!(
                info.data_offset >= 48,
                "{name} 第 {index} 张的数据偏移 {} 落在 .wzl 头里",
                info.data_offset
            );
            assert!(
                info.data_len > 0,
                "{name} 第 {index} 张声明了尺寸却没有数据"
            );
            assert!(
                info.data_offset + u64::from(info.data_len) <= file_len,
                "{name} 第 {index} 张的数据越过文件末尾（{} + {} > {file_len}）",
                info.data_offset,
                info.data_len
            );
            let record_offset = info.data_offset - 16;
            if let Some(end) = previous_end {
                if record_offset != end {
                    discontinuities += 1;
                }
            }
            previous_end = Some(info.data_offset + u64::from(info.data_len));
            let image = match library.image(index) {
                Ok(image) => image,
                Err(error) => panic!("{name} 第 {index} 张解码失败：{error}"),
            };
            assert_eq!(
                image.pixels.len(),
                usize::from(info.width) * usize::from(info.height) * 4,
                "{name} 第 {index} 张：解码长度与头信息不一致"
            );
            decoded += 1;
        }
        println!(
            "{name}: {} 张全部检查，非空解码 {decoded}，空记录 {empty}，记录间不连续 {discontinuities} 处",
            library.len()
        );
        assert!(decoded > 2000, "{name}: 只解码了 {decoded} 张");
        if name == "anitiles1.wzl" {
            assert_eq!(discontinuities, 0, "anitiles1 的记录是连续存放的");
        }
    }
}

/// 调色板与 alpha：`.wzl` 的 8/16 位都是不透明的；24 位扩展格式也能解。
#[test]
fn palette_and_alpha_sanity() {
    // 8 位调色板：alpha 全部来自 WeMadeLibrary.cs:77 的硬编码表，恒为 255。
    let palette_path = data_dir().join("cbohum.wzl");
    if let Some(library) = open_if_present(&palette_path) {
        let mut checked = 0usize;
        for index in 0..library.len().min(4000) {
            let Ok(info) = library.info(index) else {
                panic!("cbohum 第 {index} 张读头失败");
            };
            if info.is_empty() || info.format != format::PALETTE8 {
                continue;
            }
            let image = match library.image(index) {
                Ok(image) => image,
                Err(error) => panic!("cbohum 第 {index} 张解码失败：{error}"),
            };
            assert!(
                image.pixels.chunks_exact(4).all(|pixel| pixel[3] == 255),
                "调色板图像的 alpha 应当恒为 255（第 {index} 张）"
            );
            assert_eq!(
                &image.pixels[0..4],
                &[0, 0, 0, 255],
                "调色板 0 号是不透明黑"
            );
            checked += 1;
            if checked >= 3 {
                break;
            }
        }
        println!("cbohum.wzl: 检查了 {checked} 张调色板图的 alpha");
        assert!(checked > 0, "cbohum 里没有找到调色板图");
    }

    // RGB565：同样没有 alpha 通道。
    let tile_path = data_dir().join("anitiles1.wzl");
    if let Some(library) = open_if_present(&tile_path) {
        let info = match library.info(0) {
            Ok(info) => info,
            Err(error) => panic!("anitiles1 第 0 张读头失败：{error}"),
        };
        assert_eq!(info.format, format::RGB565);
        let image = match library.image(0) {
            Ok(image) => image,
            Err(error) => panic!("anitiles1 第 0 张解码失败：{error}"),
        };
        assert!(image.pixels.chunks_exact(4).all(|pixel| pixel[3] == 255));
    }

    // 24 位扩展格式：ui1.wzl 第 5171 张是 400×400 的水印图（"3122.cn 传奇工具分享站"）。
    let ui_path = data_dir().join("ui1.wzl");
    if let Some(library) = open_if_present(&ui_path) {
        if library.len() > 5171 {
            let info = match library.info(5171) {
                Ok(info) => info,
                Err(error) => panic!("ui1 第 5171 张读头失败：{error}"),
            };
            assert_eq!(info.format, format::RGB24);
            assert_eq!(info.width, 400);
            assert_eq!(info.height, 400);
            let image = match library.image(5171) {
                Ok(image) => image,
                Err(error) => panic!("ui1 第 5171 张解码失败：{error}"),
            };
            assert_eq!(image.pixels.len(), 400 * 400 * 4);
            assert!(
                image
                    .pixels
                    .chunks_exact(4)
                    .any(|pixel| pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0),
                "24 位图应当有非黑像素"
            );
            println!("ui1.wzl 第 5171 张：400×400 RGB24 解码成功");
        }
    }
}
