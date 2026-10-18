//! 真实素材冒烟：`hum.wzl` / `mon1.wzl` 的站立/走/跑/攻击/受击/死亡帧能否解码。
//!
//! 缺 `resource/` 时打印跳过原因并返回。有素材时：
//!
//! 1. 按 [`mir_render::animation`] 的帧表逐个方向、逐帧解码；
//! 2. 打印每个动作的帧数、方向步长与起止帧号（人类用块 0，怪物用 `mon1.wzl` 图像 0）；
//! 3. 断言所有帧都真的非空——帧表/起点写错时这里会立刻失败。

use std::path::{Path, PathBuf};

use mir_asset::ImageLibrary;
use mir_core::enums::MirDirection;
use mir_render::animation::{
    ActorAction, HUMAN_ATTACK1, HUMAN_ATTACK2, HUMAN_ATTACK3, HUMAN_DEAD, HUMAN_DYING,
    HUMAN_RUNNING, HUMAN_STANDING, HUMAN_STRUCK, HUMAN_WALKING, MONSTER_ATTACK1, MONSTER_DEAD,
    MONSTER_DYING, MONSTER_STANDING, MONSTER_STRUCK, MONSTER_WALKING,
};

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

fn open(stem: &str) -> Option<ImageLibrary> {
    let path = data_dir().join(format!("{stem}.wzl"));
    if !path.is_file() {
        eprintln!("跳过：{} 不存在", path.display());
        return None;
    }
    match ImageLibrary::open(&path) {
        Ok(library) => Some(library),
        Err(error) => {
            eprintln!("跳过：{} 打不开：{error}", path.display());
            None
        }
    }
}

/// 一帧是否真的有像素。
fn decodes(library: &ImageLibrary, index: u32) -> bool {
    let Ok(index) = usize::try_from(index) else {
        return false;
    };
    library
        .image(index)
        .is_ok_and(|image| image.pixels.chunks_exact(4).any(|pixel| pixel[3] != 0))
}

#[test]
fn human_action_frames_decode_for_every_direction() {
    let Some(hum) = open("hum") else { return };
    // 裸体男（块 0）：一个外观块 600 帧。
    let block = 0u32;
    let actions = [
        ("站立", ActorAction::Standing, HUMAN_STANDING),
        ("行走", ActorAction::Walking, HUMAN_WALKING),
        ("跑步", ActorAction::Running, HUMAN_RUNNING),
        ("攻击1", ActorAction::Attacking, HUMAN_ATTACK1),
        ("攻击2", ActorAction::Attacking, HUMAN_ATTACK2),
        ("攻击3", ActorAction::Attacking, HUMAN_ATTACK3),
        ("受击", ActorAction::Struck, HUMAN_STRUCK),
        ("死亡", ActorAction::Dying, HUMAN_DYING),
        ("尸体", ActorAction::Dead, HUMAN_DEAD),
    ];
    for (name, _, frames) in actions {
        let first = block * mir_render::PLAYER_FRAMES_PER_APPEARANCE
            + u32::from(frames.index_of(MirDirection::Up, 0));
        let last = block * mir_render::PLAYER_FRAMES_PER_APPEARANCE
            + u32::from(frames.index_of(MirDirection::UpLeft, frames.count - 1));
        println!(
            "人类 {name}：{} 帧/方向 · 步长 {} · 方向0 起 {first} · 方向7 止 {last} · 间隔 {} ms",
            frames.count,
            frames.stride(),
            frames.interval_ms
        );
        for direction in DIRECTIONS {
            for local in 0..frames.count {
                let index = block * mir_render::PLAYER_FRAMES_PER_APPEARANCE
                    + u32::from(frames.index_of(direction, local));
                assert!(
                    decodes(&hum, index),
                    "hum.wzl 块 {block} {name} 方向 {direction:?} 第 {local} 帧 {index} 解不出图"
                );
            }
        }
    }
    println!("人类动作帧全部可解码（hum.wzl 块 {block}）");
}

#[test]
fn monster_action_frames_decode_for_every_direction() {
    let Some(mon) = open("mon1") else { return };
    // `mon1.wzl` 第 0 个图像（`Monster::Guard` / image 0），一个图像 430 帧。
    let base = 0u32;
    let actions = [
        ("站立", MONSTER_STANDING),
        ("行走", MONSTER_WALKING),
        ("攻击", MONSTER_ATTACK1),
        ("受击", MONSTER_STRUCK),
        ("死亡", MONSTER_DYING),
        ("尸体", MONSTER_DEAD),
    ];
    for (name, frames) in actions {
        let first = base + u32::from(frames.index_of(MirDirection::Up, 0));
        let last = base + u32::from(frames.index_of(MirDirection::UpLeft, frames.count - 1));
        println!(
            "怪物 {name}：{} 帧/方向 · 步长 {} · 方向0 起 {first} · 方向7 止 {last} · 间隔 {} ms",
            frames.count,
            frames.stride(),
            frames.interval_ms
        );
        for direction in DIRECTIONS {
            for local in 0..frames.count {
                let index = base + u32::from(frames.index_of(direction, local));
                assert!(
                    decodes(&mon, index),
                    "mon1.wzl 图像 0 {name} 方向 {direction:?} 第 {local} 帧 {index} 解不出图"
                );
            }
        }
    }
    println!("怪物动作帧全部可解码（mon1.wzl 图像 0）");
}
