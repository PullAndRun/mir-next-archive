//! `asset_mode=release` 实测：**只用 `packs/`** 加载真实素材（tiles0 / mon-0130 / map0 `.mnm`），
//! 并与散装（`gfx/`）对比打开与解码耗时。
//!
//! 仓库里没有 `game-assets/` 时跳过（CI 不跑素材用例）。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mir_asset::{LibraryMap, PackSet};
use mir_render::TileLibraries;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn temp_empty_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mir2-release-load-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn millis(elapsed: Duration) -> f64 {
    elapsed.as_secs_f64() * 1000.0
}

/// 取前 `count` 张非空帧并返回 (解码张数, 耗时)。
fn decode_frames(library: &mir_asset::LibrarySource, count: usize) -> (usize, Duration) {
    let start = Instant::now();
    let mut decoded = 0usize;
    for index in 0..count {
        if let Ok(image) = library.image(index) {
            if !image.info.is_empty() {
                decoded += 1;
            }
        }
    }
    (decoded, start.elapsed())
}

#[test]
fn release_packs_load_real_assets_and_compare_with_dev() {
    let root = repo_root().join("game-assets");
    let packs_dir = root.join("packs");
    if !packs_dir.join("manifest.packs.json").is_file() {
        eprintln!(
            "跳过：缺 {}",
            packs_dir.join("manifest.packs.json").display()
        );
        return;
    }

    // ---- release：只用 packs/ ----
    let start = Instant::now();
    let release_map = LibraryMap::load(packs_dir.join("library-map.json")).expect("packs map");
    let packs = PackSet::open_unverified(&packs_dir).expect("open packs");
    let open_elapsed = start.elapsed();
    let empty_data = temp_empty_dir("release");
    // 纯 release：素材根换成空目录（没有 `gfx/**`），只剩 `packs/` 可用。
    let empty_root = temp_empty_dir("release-root");
    let release_libraries = TileLibraries::open_with_sources(
        &empty_data,
        None,
        Some(&empty_root),
        Some(&release_map),
        Some(&packs),
        false,
    )
    .expect("release 模式应当只靠 packs 打开 tiles 库");
    let release_tiles = release_libraries.library(0).expect("tiles 库");
    assert!(release_tiles.is_pack(), "release 模式必须是 Pack 变体");
    assert!(
        release_tiles.len() > 100,
        "tiles0 至少上百帧，实际 {}",
        release_tiles.len()
    );
    let (release_decoded, release_decode) = decode_frames(release_tiles, 200);

    // mon-0130：从 packs 里按前缀取一个实体
    let mon = packs.filter_prefix("gfx/sprites/monster/mon-0130/");
    assert!(!mon.is_empty(), "packs 里必须有 mon-0130");
    let (mon_decoded, mon_decode) = decode_frames(&mir_asset::LibrarySource::Pack(mon), 60);

    // map0 `.mnm`
    let map_start = Instant::now();
    let map = mir_world::Map::open_mnm(&root.join("maps/0.mnm")).expect("load map0.mnm");
    let map_elapsed = map_start.elapsed();

    // ---- dev：散装 gfx/ ----
    let dev_map = LibraryMap::load(root.join("library-map.json")).expect("dev map");
    let dev_data = root.join("data");
    let dev_gfx = root.join("gfx");
    let dev_libraries = TileLibraries::open_with_sources(
        &dev_data,
        Some(dev_gfx.as_path()),
        Some(&root),
        Some(&dev_map),
        None,
        false,
    )
    .expect("dev 模式打开 tiles 库");
    let dev_tiles = dev_libraries.library(0).expect("tiles 库");
    assert!(dev_tiles.is_unpacked_dirs(), "dev 模式必须是 UnpackedDirs");
    let (dev_decoded, dev_decode) = decode_frames(dev_tiles, 200);

    println!(
        "release: open={:.2}ms decode({release_decoded}/200)={:.2}ms | mon-0130 decode({mon_decoded}/60)={:.2}ms | map0.mnm={:.2}ms",
        millis(open_elapsed),
        millis(release_decode),
        millis(mon_decode),
        millis(map_elapsed)
    );
    println!(
        "dev    : decode({dev_decoded}/200)={:.2}ms | 相对 release 倍数={:.2}x",
        millis(dev_decode),
        millis(dev_decode) / millis(release_decode).max(0.001)
    );
    assert!(release_decoded > 0, "release 模式必须能解出 tile 帧");
    assert!(mon_decoded > 0, "release 模式必须能解出 mon-0130 帧");
    assert!(map.width > 0 && map.height > 0, "map0.mnm 必须有尺寸");
    std::fs::remove_dir_all(&empty_data).ok();
    std::fs::remove_dir_all(&empty_root).ok();
}
