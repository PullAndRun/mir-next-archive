//! 素材三形态加载优先级：**散装（library-map）→ 小包（`.mnp`）→ 原始 `.wzl`**。
//!
//! 对应任务：「`asset_mode = auto|dev|release`，回退链 散装 → packs → .wzl」。

use std::path::{Path, PathBuf};

use mir_asset::{ImageInfo, LibraryMap, PackManifest, PackSet, PackWriter, format};
use mir_render::TileLibraries;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mir2-asset-modes-{tag}-{}-{}",
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

fn sample_info(width: u16, height: u16) -> ImageInfo {
    ImageInfo {
        width,
        height,
        offset_x: 1,
        offset_y: 2,
        format: format::RGB565,
        shadow_x: 3,
        shadow_y: 4,
        reserved: [0; 3],
        has_mask: false,
        data_offset: 0,
        data_len: 0,
    }
}

fn frame_pixels(seed: u8) -> Vec<u8> {
    (0..4 * 4 * 4)
        .map(|index| u8::try_from(index).unwrap_or(0).wrapping_add(seed))
        .collect()
}

/// 造一个"散装"素材根：`gfx/tiles/tiles0/` 两张图 + `library-map.json`。
fn make_dev_root(root: &Path) -> Vec<Vec<u8>> {
    let tiles_dir = root.join("gfx/tiles/tiles0");
    std::fs::create_dir_all(&tiles_dir).expect("mkdir tiles");
    let mut pixels = Vec::new();
    let mut images = Vec::new();
    for (index, seed) in [0u8, 7].into_iter().enumerate() {
        let rgba = frame_pixels(seed);
        write_png(&tiles_dir.join(format!("{index:04}.png")), 4, 4, &rgba);
        images.push(serde_json::json!({
            "index": index,
            "png": format!("{index:04}.png"),
            "width": 4,
            "height": 4,
            "offset_x": 1,
            "offset_y": 2,
            "shadow_x": 3,
            "shadow_y": 4,
            "has_mask": false,
            "format_code": format::RGB565,
            "empty": false,
        }));
        pixels.push(rgba);
    }
    std::fs::write(
        tiles_dir.join("index.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "version": 1,
            "lib": "tiles.wzl",
            "count": 2,
            "written_png": 2,
            "images": images,
        }))
        .expect("json"),
    )
    .expect("write index");
    std::fs::write(
        root.join("library-map.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "version": 1,
            "libraries": {
                "tiles": {"lib": "tiles.wzl", "dirs": ["gfx/tiles/tiles0"]}
            }
        }))
        .expect("json"),
    )
    .expect("write map");
    pixels
}

/// 把散装根打成 `packs/`（一个包 + manifest + 带 prefix 的 library-map.json）。
fn make_release_root(root: &Path, pixels: &[Vec<u8>]) {
    let packs = root.join("packs");
    std::fs::create_dir_all(&packs).expect("mkdir packs");
    let mut writer = PackWriter::create(packs.join("tiles-tiles0-1.mnp"), true).expect("create");
    for (index, rgba) in pixels.iter().enumerate() {
        writer
            .add_image(
                &format!("gfx/tiles/tiles0/{index:04}.png"),
                &sample_info(4, 4),
                rgba,
            )
            .expect("add");
    }
    let summary = writer.finish("tiles").expect("finish");
    std::fs::write(
        packs.join("manifest.packs.json"),
        serde_json::to_string_pretty(&PackManifest {
            version: 1,
            generated_at: "epoch+0s".to_owned(),
            chunk_mb: 16,
            packs: vec![summary],
        })
        .expect("json"),
    )
    .expect("manifest");
    std::fs::write(
        packs.join("library-map.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "version": 1,
            "libraries": {
                "tiles": {
                    "lib": "tiles.wzl",
                    "dirs": ["gfx/tiles/tiles0"],
                    "prefix": "gfx/tiles/tiles0/"
                }
            }
        }))
        .expect("json"),
    )
    .expect("map");
}

#[test]
fn dev_layout_is_preferred_and_pixels_match() {
    let root = temp_dir("dev");
    let expected = make_dev_root(&root);
    let data_dir = root.join("data");
    std::fs::create_dir_all(&data_dir).expect("mkdir data");
    let map = LibraryMap::load(root.join("library-map.json")).expect("load map");

    let libraries = TileLibraries::open_with_sources(
        &data_dir,
        Some(&root.join("gfx")),
        Some(&root),
        Some(&map),
        None,
        false,
    )
    .expect("open dev layout");
    let library = libraries.library(0).expect("tiles 库");
    assert!(
        library.is_unpacked_dirs(),
        "有 library-map 的多目录时必须走 UnpackedDirs"
    );
    assert_eq!(library.len(), 2);
    let image = library.image(0).expect("image");
    assert_eq!(image.pixels, expected[0], "散装像素必须与写入一致");
    let info = library.info(0).expect("info");
    assert_eq!((info.offset_x, info.offset_y, info.shadow_x), (1, 2, 3));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn release_packs_are_used_when_no_dev_tree() {
    let root = temp_dir("release");
    let expected = make_dev_root(&root);
    make_release_root(&root, &expected);
    // 删掉散装目录，只剩 packs/：模拟发布形态
    std::fs::remove_dir_all(root.join("gfx")).expect("rm gfx");
    std::fs::remove_file(root.join("library-map.json")).expect("rm dev map");
    let data_dir = root.join("data");
    std::fs::create_dir_all(&data_dir).expect("mkdir data");
    let map = LibraryMap::load(root.join("packs/library-map.json")).expect("load map");
    let packs = PackSet::open(root.join("packs")).expect("open packs");

    let libraries = TileLibraries::open_with_sources(
        &data_dir,
        None,
        Some(&root),
        Some(&map),
        Some(&packs),
        false,
    )
    .expect("open release layout");
    let library = libraries.library(0).expect("tiles 库");
    assert!(library.is_pack(), "没有散装时必须走 Pack 变体");
    assert_eq!(library.len(), 2);
    assert_eq!(library.image(1).expect("image").pixels, expected[1]);
    std::fs::remove_dir_all(&root).ok();
}

/// `auto` 档：没有散装时用 packs（与 release 等价），仍然不碰 `.wzl`。
#[test]
fn auto_falls_back_to_packs_before_legacy() {
    let root = temp_dir("auto");
    let expected = make_dev_root(&root);
    make_release_root(&root, &expected);
    std::fs::remove_dir_all(root.join("gfx")).expect("rm gfx");
    let data_dir = root.join("data");
    std::fs::create_dir_all(&data_dir).expect("mkdir data");
    let map = LibraryMap::load(root.join("packs/library-map.json")).expect("load map");
    let packs = PackSet::open(root.join("packs")).expect("open packs");

    let libraries = TileLibraries::open_with_sources(
        &data_dir,
        None,
        Some(&root),
        Some(&map),
        Some(&packs),
        false,
    )
    .expect("open auto layout");
    assert!(libraries.library(0).expect("tiles").is_pack());
    std::fs::remove_dir_all(&root).ok();
}

/// 新规则：**默认（非 legacy）不许回退老格式**——目录里只有 `.wzl` 时必须报"没图库"。
#[test]
fn non_legacy_mode_refuses_original_wzl() {
    let data_dir = Path::new("../../resource/data");
    if !data_dir.is_dir() {
        eprintln!("skip: no resource/data");
        return;
    }
    let error = TileLibraries::open_with_sources(data_dir, None, None, None, None, false)
        .expect_err("非 legacy 档不许读 .wzl");
    assert!(
        matches!(error, mir_render::RenderError::NoLibraries { .. }),
        "必须是 NoLibraries（表示没有自有格式素材），实际：{error}"
    );
}

/// `legacy` 档（仅开发）：允许回退原始 `.wzl`。
#[test]
#[cfg(feature = "legacy")]
fn legacy_mode_allows_original_wzl_with_warning_path() {
    let data_dir = Path::new("../../resource/data");
    if !data_dir.is_dir() {
        eprintln!("skip: no resource/data");
        return;
    }
    let libraries = TileLibraries::open_with_sources(data_dir, None, None, None, None, true)
        .expect("legacy 档应能打开原始 .wzl");
    let library = libraries.library(0).expect("tiles 库");
    assert!(
        !library.is_unpacked_dirs() && !library.is_unpacked() && !library.is_pack(),
        "legacy 档走的是原始容器"
    );
}

/// 真实 `game-assets/`（户部二号拆包产物）能被 `auto` 档读出来。
///
/// 仓库里没有素材时跳过；有素材时必须走 `UnpackedDirs` 且 `tiles0` 目录真的在。
#[test]
fn real_game_assets_dev_tree_loads() {
    let root = Path::new("../../game-assets");
    let map_path = root.join("library-map.json");
    if !map_path.is_file() {
        eprintln!("skip: no game-assets/library-map.json");
        return;
    }
    let map = LibraryMap::load(&map_path).expect("load library map");
    assert!(map.len() >= 60, "全量拆包应有 66 个库，实际 {}", map.len());
    let data_dir = root.join("data");
    let libraries = TileLibraries::open_with_sources(
        &data_dir,
        Some(&root.join("gfx")),
        Some(root),
        Some(&map),
        None,
        false,
    )
    .expect("open real dev tree");
    let tiles = libraries.library(0).expect("tiles 库");
    assert!(
        tiles.is_unpacked_dirs(),
        "game-assets 的新布局必须走多目录 UnpackedDirs"
    );
    assert!(tiles.len() > 100, "tiles0 至少上百帧，实际 {}", tiles.len());
    let image = tiles.image(0).expect("tiles0[0]");
    assert!(!image.pixels.is_empty());
}
