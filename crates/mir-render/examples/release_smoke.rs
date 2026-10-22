//! 发布前冒烟：**只读 `packs/`** 打开素材、渲染一帧，并与散装 `gfx/` 渲染逐像素对比。
//!
//! 给 `tools/dev/release-smoke.ps1` 调用（也可以手动跑）：
//!
//! ```text
//! cargo run -p mir-render --example release_smoke -- \
//!   --packs game-assets/packs --assets game-assets --expect-libraries 67
//! ```
//!
//! 退出码：0 = 通过；1 = 任一检查失败（原因写到 stderr）。

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mir_asset::{LibraryMap, PackSet};
use mir_core::Point;
use mir_core::enums::MirDirection;
use mir_render::{EntityKind, EntityView, Frame, MapScene};

struct Args {
    packs: PathBuf,
    assets: PathBuf,
    expect_libraries: usize,
    map: String,
    tile_frames: usize,
    entity_frames: usize,
}

fn parse_args() -> Result<Args, String> {
    let mut packs = None;
    let mut assets = None;
    let mut expect_libraries = 67usize;
    let mut map = "maps/0.mnm".to_owned();
    let mut tile_frames = 148usize;
    let mut entity_frames = 14usize;
    let mut iter = std::env::args().skip(1);
    while let Some(flag) = iter.next() {
        let mut value = || iter.next().ok_or_else(|| format!("{flag} 缺参数值"));
        match flag.as_str() {
            "--packs" => packs = Some(PathBuf::from(value()?)),
            "--assets" => assets = Some(PathBuf::from(value()?)),
            "--expect-libraries" => {
                expect_libraries = value()?
                    .parse()
                    .map_err(|error| format!("--expect-libraries: {error}"))?;
            }
            "--map" => map = value()?,
            "--tile-frames" => {
                tile_frames = value()?
                    .parse()
                    .map_err(|error| format!("--tile-frames: {error}"))?;
            }
            "--entity-frames" => {
                entity_frames = value()?
                    .parse()
                    .map_err(|error| format!("--entity-frames: {error}"))?;
            }
            other => return Err(format!("未知参数 {other}")),
        }
    }
    Ok(Args {
        packs: packs.ok_or("缺 --packs")?,
        assets: assets.ok_or("缺 --assets")?,
        expect_libraries,
        map,
        tile_frames,
        entity_frames,
    })
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mir2-release-smoke-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn non_black(frame: &Frame) -> usize {
    frame
        .pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0)
        .count()
}

fn differing(left: &Frame, right: &Frame) -> usize {
    left.pixels
        .chunks_exact(4)
        .zip(right.pixels.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count()
}

fn load_scene(
    map_path: &Path,
    data_dir: &Path,
    gfx_dir: Option<&Path>,
    asset_root: &Path,
    library_map: &LibraryMap,
    packs: Option<&PackSet>,
) -> Result<MapScene, String> {
    MapScene::load_with_sources(
        map_path,
        data_dir,
        gfx_dir,
        Some(asset_root),
        Some(library_map),
        packs,
        false,
    )
    .map_err(|error| format!("加载场景失败：{error}"))
}

fn run(args: &Args) -> Result<(), String> {
    let manifest_path = args.packs.join("manifest.packs.json");
    if !manifest_path.is_file() {
        return Err(format!("缺 {}", manifest_path.display()));
    }
    let manifest: mir_asset::PackManifest = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("解析 {} 失败：{error}", manifest_path.display()))?;

    let total_bytes: u64 = manifest.packs.iter().map(|pack| pack.bytes).sum();
    let total_entries: u64 = manifest
        .packs
        .iter()
        .map(|pack| u64::from(pack.entries))
        .sum();
    println!(
        "smoke: packs={} bytes={total_bytes} entries={total_entries} \
         generator={:?} deterministic={} generated_at={}",
        manifest.packs.len(),
        manifest.generator,
        manifest.deterministic,
        manifest.generated_at
    );
    if !manifest.deterministic {
        return Err("manifest.deterministic 必须为 true".to_owned());
    }

    let map_path = args.assets.join(&args.map);
    // 纯 release：data/gfx 指向空目录，只剩 packs 可用。
    let empty_data = temp_dir("data");
    let empty_root = temp_dir("root");
    let packs_map_path = args.packs.join("library-map.json");
    let packs_map = LibraryMap::load(&packs_map_path)
        .map_err(|error| format!("读 {} 失败：{error}", packs_map_path.display()))?;
    println!("smoke: library-map libraries={}", packs_map.len());
    if packs_map.len() != args.expect_libraries {
        return Err(format!(
            "library-map 库数 {} != 期望 {}",
            packs_map.len(),
            args.expect_libraries
        ));
    }

    let open_start = Instant::now();
    let packs = PackSet::open_unverified(&args.packs)
        .map_err(|error| format!("打开 packs 失败：{error}"))?;
    let open_elapsed = open_start.elapsed();

    let mut release = load_scene(
        &map_path,
        &empty_data,
        None,
        &empty_root,
        &packs_map,
        Some(&packs),
    )?;

    // tiles0 解码若干帧（首库）。
    let tiles = packs.filter_prefix("gfx/tiles/tiles0/");
    let mut tile_decoded = 0usize;
    for index in 0..args.tile_frames {
        if tiles
            .library_image(index)
            .is_ok_and(|image| !image.info.is_empty())
        {
            tile_decoded += 1;
        }
    }
    if tile_decoded == 0 {
        return Err("tiles0 一帧都没解出来".to_owned());
    }

    // mon-0130 解码若干帧。
    let monster = packs.filter_prefix("gfx/sprites/monster/mon-0130/");
    let mut monster_decoded = 0usize;
    for index in 0..args.entity_frames {
        if monster
            .library_image(index)
            .is_ok_and(|image| !image.info.is_empty())
        {
            monster_decoded += 1;
        }
    }
    if monster_decoded == 0 {
        return Err("mon-0130 一帧都没解出来".to_owned());
    }

    // release 渲染
    let mut view = EntityView::new(
        Point::new(330, 270),
        MirDirection::Down,
        EntityKind::Player,
        0,
    );
    view.armour = 0;
    view.hair = 1;
    view.weapon = 1;
    view.weapon_shape = 1;
    let views = [view];
    let center = Point::new(330, 270);
    let mut release_frame = Frame::new(800, 600);
    let render_start = Instant::now();
    release.render_with_entities_at(&mut release_frame, center, &views, 0);
    let release_render = render_start.elapsed();
    let pixels = non_black(&release_frame);
    if pixels == 0 {
        return Err("release 渲染全黑".to_owned());
    }

    // dev 对照（散装 gfx）
    let dev_map_path = args.assets.join("library-map.json");
    let dev_map = LibraryMap::load(&dev_map_path)
        .map_err(|error| format!("读 {} 失败：{error}", dev_map_path.display()))?;
    let dev_data = args.assets.join("data");
    let dev_gfx = args.assets.join("gfx");
    let mut dev = load_scene(
        &map_path,
        &dev_data,
        Some(&dev_gfx),
        &args.assets,
        &dev_map,
        None,
    )?;
    let mut dev_frame = Frame::new(800, 600);
    dev.render_with_entities_at(&mut dev_frame, center, &views, 0);
    let diff = differing(&release_frame, &dev_frame);
    if diff != 0 {
        return Err(format!("release 与 dev 渲染差异 {diff} 像素（应为 0）"));
    }

    println!(
        "smoke: open={:.0}ms render={:.0}ms pixels={pixels} diff_vs_dev={diff} \
         tiles_decoded={tile_decoded}/{} monster_decoded={monster_decoded}/{}",
        open_elapsed.as_secs_f64() * 1000.0,
        release_render.as_secs_f64() * 1000.0,
        args.tile_frames,
        args.entity_frames
    );
    std::fs::remove_dir_all(&empty_data).ok();
    std::fs::remove_dir_all(&empty_root).ok();
    println!("smoke: OK");
    Ok(())
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("release-smoke: {error}");
            return ExitCode::FAILURE;
        }
    };
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("release-smoke: FAILED：{error}");
            ExitCode::FAILURE
        }
    }
}
