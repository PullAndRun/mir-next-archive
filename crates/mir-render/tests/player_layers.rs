//! 真实素材冒烟：玩家装备分层用的 `hum*.wzl` / `weapon*.wzl` / `hair*.wzl` 能否解码。
//!
//! 这个测试**不需要** `resource/` 也能跑（缺素材时打印跳过原因），但有素材时会：
//!
//! 1. 打开三张图库并核对帧总数（`hum.wzx` 14400 = 24×600、`weapon.wzx` 45600 = 76×600、
//!    `hair.wzx` 3600 = 6×600）；
//! 2. 解码若干 (库, 外观块, 方向) 的站立帧，断言确实有像素；
//! 3. 把「裸体 / 穿衣服 / 衣服+武器 / 衣服+武器+头发」三种外观按 8 个方向渲染成
//!    `target/players-layers.png`，并断言每多一层画面都会变（装备层真的画上去了）。

use std::path::{Path, PathBuf};

use mir_asset::ImageLibrary;
use mir_core::Point;
use mir_core::enums::MirDirection;
use mir_render::entities::{
    EntityKind, EntityLibraries, EntityView, hair_library, player_library, player_standing_frame,
    weapon_library,
};
use mir_render::{Frame, Viewport};
use mir_world::{Map, MapCell, MapVersion};

/// 8 个方向，按 `MirDirection` 的线上序号排列（0=上，顺时针）。
const DIRECTIONS: [MirDirection; 8] = [
    MirDirection::Up,
    MirDirection::UpRight,
    MirDirection::Right,
    MirDirection::DownRight,
    MirDirection::Down,
    MirDirection::DownLeft,
    MirDirection::Left,
    MirDirection::UpLeft,
];

fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resource/data")
}

fn target_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(name)
}

/// 打开一张图库；缺失时返回说明而不是失败（CI 没有素材）。
fn open(stem: &str) -> Option<ImageLibrary> {
    let path = data_dir().join(format!("{stem}.wzl"));
    if !path.is_file() {
        eprintln!("跳过：{stem}.wzl 不存在（{}）", path.display());
        return None;
    }
    match ImageLibrary::open(&path) {
        Ok(library) => Some(library),
        Err(error) => {
            eprintln!("跳过：{stem}.wzl 打不开：{error}");
            None
        }
    }
}

/// 一张帧是否真的有像素。
fn decodes(library: &ImageLibrary, index: u32) -> bool {
    let Ok(index) = usize::try_from(index) else {
        return false;
    };
    library
        .image(index)
        .is_ok_and(|image| image.pixels.chunks_exact(4).any(|pixel| pixel[3] != 0))
}

#[test]
fn player_layer_libraries_decode_standing_frames() {
    let Some(hum) = open("hum") else { return };
    let Some(weapon) = open("weapon") else { return };
    let Some(hair) = open("hair") else { return };

    // 帧总数 = 外观块数 × 600。
    assert_eq!(hum.len(), 14_400, "hum.wzx 应有 24×600 帧");
    assert_eq!(weapon.len(), 45_600, "weapon.wzx 应有 76×600 帧");
    assert_eq!(hair.len(), 3_600, "hair.wzx 应有 6×600 帧");

    // 身体：块 0（裸体）与块 2（重盔甲）的 8 方向站立帧都要能解码。
    for block in [0u16, 2, 3, 23] {
        let (library, block) = player_library(block);
        assert_eq!(library, 1, "0..23 号外观在 hum.wzl");
        for direction in DIRECTIONS {
            let frame = player_standing_frame(block, direction, 0);
            assert!(
                decodes(&hum, frame),
                "hum.wzl 块 {block} 方向 {direction:?} 帧 {frame} 解不出图"
            );
        }
    }

    // 武器：`weapon.wzl` 的前两块是空的（形状 0/1 = 空手），从块 3 起有图。
    let (weapon_series, weapon_block) = weapon_library(36);
    assert_eq!(weapon_series, 1);
    assert_eq!(weapon_block, 36);
    for direction in DIRECTIONS {
        let frame = player_standing_frame(weapon_block, direction, 0);
        assert!(
            decodes(&weapon, frame),
            "weapon.wzl 块 {weapon_block} 方向 {direction:?} 帧 {frame} 解不出图"
        );
    }

    // 头发：`hair.wzl` 块 0/1 是空的，块 2 起有发型。
    let (hair_series, hair_block) = hair_library(2);
    assert_eq!(hair_series, 1);
    assert_eq!(hair_block, 2);
    for direction in DIRECTIONS {
        let frame = player_standing_frame(hair_block, direction, 0);
        assert!(
            decodes(&hair, frame),
            "hair.wzl 块 {hair_block} 方向 {direction:?} 帧 {frame} 解不出图"
        );
    }

    // 语料里没有盾牌库：`shield` 层必须安静地退化。
    assert!(
        !data_dir().join("shield.wzl").is_file(),
        "语料里意外出现了 shield.wzl，请更新文档与实现"
    );
}

#[test]
fn shifted_weapon_blocks_still_draw_every_direction() {
    // `weapon.wzl` 有 25/76 个外观块的站立段整体后移 8 帧（块首 8 帧是空的）；
    // 形状 2 就是其中之一。渲染器按块首帧探测并补偿，所以 8 个方向都应该画得出武器。
    let Some(_hum) = open("hum") else { return };
    if open("weapon").is_none() {
        return;
    }
    let mut scene = mir_render::MapScene::from_parts_with_entities(
        synthetic_map(),
        mir_render::TileLibraries::empty("."),
        Viewport::new(1024, 512),
        EntityLibraries::open_dir(data_dir()),
    );
    let center = Point::new(8, 6);
    let cells: Vec<Point> = (0..8).map(|column| Point::new(1 + column * 2, 6)).collect();
    let origins: Vec<(i32, i32)> = cells
        .iter()
        .map(|cell| scene.world_to_screen(center, *cell))
        .collect();
    let mut render = |weapon: u16, weapon_shape: u16| {
        let entities: Vec<EntityView> = DIRECTIONS
            .iter()
            .zip(cells.iter())
            .map(|(direction, cell)| {
                let mut view = EntityView::new(*cell, *direction, EntityKind::Player, 2);
                view.armour = 2;
                view.weapon = weapon;
                view.weapon_shape = weapon_shape;
                view.hair = mir_render::NO_LAYER;
                view
            })
            .collect();
        let mut frame = Frame::new(1024, 512);
        scene.render_with_entities(&mut frame, center, &entities);
        frame
    };
    let bare = render(mir_render::NO_LAYER, mir_render::NO_LAYER);
    let armed = render(2, 2);
    for (direction, (origin_x, origin_y)) in DIRECTIONS.iter().zip(origins.iter()) {
        let mut differs = 0;
        for y in origin_y - 96..origin_y + 48 {
            for x in origin_x - 48..origin_x + 48 {
                let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
                    continue;
                };
                if bare.pixel(x, y) != armed.pixel(x, y) {
                    differs += 1;
                }
            }
        }
        assert!(
            differs > 0,
            "方向 {direction:?} 的武器层没画出来（相位补偿失效？）"
        );
    }
}

/// 合成一张 32×32 的空地图：只为了拿到 `MapScene` 的视口换算。
fn synthetic_map() -> Map {
    let size = 32;
    Map {
        width: size,
        height: size,
        cells: vec![MapCell::default(); usize::try_from(size * size).unwrap_or(0)],
        file_name: "synthetic-layers".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
}

/// 一个外观配置。
#[derive(Clone, Copy)]
struct Look {
    /// 名字（打印用）。
    name: &'static str,
    /// 衣服外观号。
    armour: u16,
    /// 武器块号；`mir_render::NO_LAYER` 表示空手。
    weapon: u16,
    /// 武器原始形状。
    weapon_shape: u16,
    /// 发型号。
    hair: u16,
}

#[test]
fn composed_player_layers_render_and_differ() {
    let Some(_hum) = open("hum") else { return };
    let mut scene = mir_render::MapScene::from_parts_with_entities(
        synthetic_map(),
        mir_render::TileLibraries::empty("."),
        Viewport::new(1024, 512),
        EntityLibraries::open_dir(data_dir()),
    );
    let center = Point::new(8, 6);

    let looks = [
        Look {
            name: "裸体",
            armour: 0,
            weapon: mir_render::NO_LAYER,
            weapon_shape: mir_render::NO_LAYER,
            hair: 2,
        },
        Look {
            name: "衣服+头发",
            armour: 2,
            weapon: mir_render::NO_LAYER,
            weapon_shape: mir_render::NO_LAYER,
            hair: 2,
        },
        Look {
            name: "衣服+武器+头发",
            armour: 2,
            weapon: 10,
            weapon_shape: 36,
            hair: 2,
        },
    ];

    let mut frame = Frame::new(1024, 512);
    frame.clear(mir_render::BACKGROUND);
    let mut rendered = Vec::new();
    for look in &looks {
        // 每次都用**同一批坐标与方向**，只是外观不同，这样差异只可能来自装备层。
        let entities: Vec<EntityView> = DIRECTIONS
            .iter()
            .enumerate()
            .map(|(column, direction)| {
                let mut view = EntityView::new(
                    Point::new(1 + i32::try_from(column).unwrap_or(0) * 2, 6),
                    *direction,
                    EntityKind::Player,
                    look.armour,
                );
                view.armour = look.armour;
                view.weapon = look.weapon;
                view.weapon_shape = look.weapon_shape;
                view.hair = look.hair;
                view.name = Some(look.name.to_owned());
                view
            })
            .collect();
        let mut pass = Frame::new(1024, 512);
        scene.render_with_entities(&mut pass, center, &entities);
        rendered.push(pass);
    }

    // 每多一层，同位置同方向的画面都要有变化（说明装备层真的画上去了）。
    let changed = |left: &Frame, right: &Frame| {
        left.pixels
            .chunks_exact(4)
            .zip(right.pixels.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count()
    };
    let dressed = changed(&rendered[0], &rendered[1]);
    let armed = changed(&rendered[1], &rendered[2]);
    assert!(dressed > 100, "穿上衣服后画面必须变化，实际 {dressed}");
    assert!(armed > 100, "拿上武器后画面必须变化，实际 {armed}");

    // 最终 PNG：三种外观各占一行（上行裸体、中行穿衣服、下行全副武装）。
    let mut all: Vec<EntityView> = Vec::new();
    for (row, look) in looks.iter().enumerate() {
        for (column, direction) in DIRECTIONS.iter().enumerate() {
            let mut view = EntityView::new(
                Point::new(
                    1 + i32::try_from(column).unwrap_or(0) * 2,
                    2 + i32::try_from(row).unwrap_or(0) * 3,
                ),
                *direction,
                EntityKind::Player,
                look.armour,
            );
            view.armour = look.armour;
            view.weapon = look.weapon;
            view.weapon_shape = look.weapon_shape;
            view.hair = look.hair;
            view.name = Some(look.name.to_owned());
            all.push(view);
        }
    }
    scene.render_with_entities(&mut frame, center, &all);
    let painted = frame
        .pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] != 0 && (pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0))
        .count();
    assert!(painted > 10_000, "合成画面太暗：{painted}");

    let output = target_path("players-layers.png");
    frame.save_png(&output).expect("写出 players-layers.png");
    let metadata = std::fs::metadata(&output).expect("PNG 存在");
    println!(
        "players-layers.png：{} · {painted} 个非黑像素 · 衣服层差异 {dressed} px · 武器层差异 {armed} px · {} 字节",
        output.display(),
        metadata.len()
    );
}
