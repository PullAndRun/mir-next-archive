//! 客户端渲染性能实测（t84）。
//!
//! 复现：
//! ```text
//! cargo run --release -p mir-render --example perf -- \
//!     --map resource/map/0.map --data resource/data --out target/perf-render.json
//! ```
//!
//! 输出每个视口的首帧、100 帧平均/最差、瓦片解码缓存命中率；第二个场景测
//! 100 个玩家同屏的实体排序 + 绘制耗时。数字是**单线程、未锁帧**的纯渲染时间，
//! 不含窗口/GPU 上传。

#![allow(clippy::cast_precision_loss)]

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use mir_core::Point;
use mir_core::enums::MirDirection;
use mir_render::{EntityKind, EntityView, Frame, FxKind, MapScene, Viewport};

const SIZES: [(u32, u32); 3] = [(800, 600), (1280, 720), (1920, 1080)];
const FRAMES: usize = 100;
const EFFECTS: u32 = 200;

struct Args {
    map: PathBuf,
    data: PathBuf,
    out: PathBuf,
}

fn parse_args() -> Args {
    let mut map = PathBuf::from("resource/map/0.map");
    let mut data = PathBuf::from("resource/data");
    let mut out = PathBuf::from("target/perf-render.json");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--map" => map = args.next().map(PathBuf::from).unwrap_or(map),
            "--data" => data = args.next().map(PathBuf::from).unwrap_or(data),
            "--out" => out = args.next().map(PathBuf::from).unwrap_or(out),
            other => eprintln!("忽略未知参数：{other}"),
        }
    }
    Args { map, data, out }
}

fn percentile(sorted: &[f64], percent: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f64 * percent).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

fn stats(mut samples: Vec<f64>) -> (f64, f64, f64) {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sum: f64 = samples.iter().sum();
    let avg = if samples.is_empty() {
        0.0
    } else {
        sum / samples.len() as f64
    };
    let worst = samples.last().copied().unwrap_or(0.0);
    let p95 = percentile(&samples, 0.95);
    (avg, worst, p95)
}

fn load_scene(args: &Args) -> MapScene {
    MapScene::load(&args.map, &args.data).unwrap_or_else(|error| {
        panic!(
            "加载地图失败 {} / {}：{error}（先确认 resource 存在）",
            args.map.display(),
            args.data.display()
        )
    })
}

fn center_for(scene: &MapScene) -> Point {
    Point::new(scene.map.width / 2, scene.map.height / 2)
}

fn entities_around(center: Point) -> Vec<EntityView> {
    let mut entities = Vec::with_capacity(100);
    for index in 0..100 {
        let dx = i32::from((index % 10) as i16) - 5;
        let dy = i32::from(((index / 10) % 10) as i16) - 5;
        let mut view = EntityView::new(
            Point::new(center.x + dx, center.y + dy),
            MirDirection::Down,
            EntityKind::Player,
            0,
        );
        view.armour = u16::try_from(index % 20).unwrap_or(0);
        view.weapon = u16::try_from(index % 15 + 1).unwrap_or(1);
        view.weapon_shape = view.weapon;
        view.hair = u16::try_from(index % 8).unwrap_or(0);
        entities.push(view);
    }
    entities
}

fn main() {
    let args = parse_args();
    let center = {
        let scene = load_scene(&args);
        center_for(&scene)
    };
    let mut sections = String::new();
    let mut terrain_rows = Vec::new();

    for (width, height) in SIZES {
        let mut scene = load_scene(&args);
        let mut frame = Frame::new(width, height);
        let started = Instant::now();
        scene.render(&mut frame, center);
        let first_ms = started.elapsed().as_secs_f64() * 1000.0;

        let mut samples = Vec::with_capacity(FRAMES);
        for index in 0..FRAMES {
            // 每次横移 1 格，模拟走路；保持在同一屏内。
            let shifted = Point::new(
                center.x + i32::try_from(index % 8).unwrap_or(0) - 4,
                center.y,
            );
            let started = Instant::now();
            scene.render(&mut frame, shifted);
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        let (avg, worst, p95) = stats(samples);
        let (tile_hits, tile_misses) = scene.tile_cache_stats();
        let hit_rate = if tile_hits + tile_misses == 0 {
            0.0
        } else {
            tile_hits as f64 * 100.0 / (tile_hits + tile_misses) as f64
        };
        terrain_rows.push((width, height, first_ms, avg, worst, p95, hit_rate));

        let _ = writeln!(
            sections,
            "  \"{}x{}\": {{\"first_frame_ms\": {first_ms:.3}, \"avg_ms\": {avg:.3}, \"p95_ms\": {p95:.3}, \"worst_ms\": {worst:.3}, \"cache_hit_percent\": {hit_rate:.2}, \"cached_images\": {}}},\n",
            width,
            height,
            scene.image_cache_len()
        );
    }

    // 100 个玩家同屏：地形 + 实体，比较有无实体层的耗时差。
    let mut scene = load_scene(&args);
    let entities = entities_around(center);
    let mut frame = Frame::new(1280, 720);
    scene.render_with_entities(&mut frame, center, &entities);
    let mut entity_samples = Vec::with_capacity(FRAMES);
    for index in 0..FRAMES {
        let shifted = Point::new(
            center.x + i32::try_from(index % 8).unwrap_or(0) - 4,
            center.y,
        );
        let started = Instant::now();
        scene.render_with_entities(&mut frame, shifted, &entities);
        entity_samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let (entity_avg, entity_worst, entity_p95) = stats(entity_samples);
    let (entity_hits, entity_misses) = scene.entity_cache_stats();
    let entity_hit_rate = if entity_hits + entity_misses == 0 {
        0.0
    } else {
        entity_hits as f64 * 100.0 / (entity_hits + entity_misses) as f64
    };
    let baseline = terrain_rows
        .iter()
        .find(|row| row.0 == 1280 && row.1 == 720)
        .map_or(0.0, |row| row.3);
    let entity_delta = (entity_avg - baseline).max(0.0);

    // 特效：200 个/帧 × 100 帧；对比「每次克隆 Image」与「直接读缓存绘制」的分配次数。
    let mut old_scene = load_scene(&args);
    let mut old_frame = Frame::new(1280, 720);
    let old_start = Instant::now();
    let mut old_allocations = 0usize;
    for frame_index in 0..FRAMES {
        for index in 0..EFFECTS {
            let kind = if index % 5 == 0 {
                FxKind::Bolt
            } else {
                FxKind::Magic
            };
            let image_index = (frame_index as u32 + index) % 8;
            if let Some(image) = old_scene.effect_image(kind, image_index) {
                old_allocations += 1;
                old_frame.blit(&image, 8, 8);
            }
        }
    }
    let old_effect_ms = old_start.elapsed().as_secs_f64() * 1000.0;

    let mut new_scene = load_scene(&args);
    let mut new_frame = Frame::new(1280, 720);
    let misses_before = new_scene.entity_cache_stats().1;
    let new_start = Instant::now();
    for frame_index in 0..FRAMES {
        for index in 0..EFFECTS {
            let kind = if index % 5 == 0 {
                FxKind::Bolt
            } else {
                FxKind::Magic
            };
            let image_index = (frame_index as u32 + index) % 8;
            new_scene.draw_effect(&mut new_frame, kind, image_index, 8, 8);
        }
    }
    let new_effect_ms = new_start.elapsed().as_secs_f64() * 1000.0;
    let new_allocations =
        usize::try_from(new_scene.entity_cache_stats().1 - misses_before).unwrap_or(0);

    let json = format!(
        "{{\n  \"generated_by\": \"cargo run --release -p mir-render --example perf -- --map {} --data {} --out {}\",\n  \"map\": \"{}\",\n  \"terrain\": {{\n{}  }},\n  \"entities_100_players_1280x720\": {{\"avg_ms\": {entity_avg:.3}, \"p95_ms\": {entity_p95:.3}, \"worst_ms\": {entity_worst:.3}, \"entity_delta_ms\": {entity_delta:.3}, \"cache_hit_percent\": {entity_hit_rate:.2}}},\n  \"effects_200x100\": {{\"legacy_allocations\": {old_allocations}, \"pooled_allocations\": {new_allocations}, \"legacy_ms\": {old_effect_ms:.3}, \"pooled_ms\": {new_effect_ms:.3}}},\n  \"optimization\": \"tile blit uses precomputed opaque spans; effects draw directly from the entity decode cache\",\n  \"viewport\": {:?}\n}}\n",
        args.map.display(),
        args.data.display(),
        args.out.display(),
        args.map.display(),
        sections,
        Viewport::DEFAULT
    );
    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent).expect("创建输出目录");
    }
    std::fs::write(&args.out, json).expect("写性能 JSON");

    println!("== 地形渲染（{FRAMES} 帧/视口） ==");
    for (width, height, first, avg, worst, p95, hit) in &terrain_rows {
        println!(
            "{width}x{height}: 首帧 {first:.2}ms, 平均 {avg:.2}ms, p95 {p95:.2}ms, 最差 {worst:.2}ms, 瓦片缓存命中 {hit:.1}%"
        );
    }
    println!(
        "100 玩家 1280x720: 平均 {entity_avg:.2}ms, p95 {entity_p95:.2}ms, 最差 {entity_worst:.2}ms（实体层约 {entity_delta:.2}ms）, 实体缓存命中 {entity_hit_rate:.1}%"
    );
    println!(
        "特效 200×100: 旧路径分配 {old_allocations} 次 / {old_effect_ms:.1}ms；池化后分配 {new_allocations} 次 / {new_effect_ms:.1}ms"
    );
    println!("已写 {}", args.out.display());
}
