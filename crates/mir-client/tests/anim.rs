//! 注入式动作动画测试：`ObjectWalk/ObjectAttack/ObjectStruck/ObjectDied` → 快照 → 渲染。
//!
//! 不连服务器。所有时间都由测试显式给出（[`WorldSnapshot::apply_packet_at`] 与
//! [`WorldSnapshot::tick_animations`]），因此断言是确定性的：
//!
//! 1. 注入 `S.ObjectWalk` 后实体动作为 `Walking`，600 ms 内 `frame_at` 返回不同帧，
//!    600 ms 后回 `Standing`；
//! 2. 攻击 300 ms（6 帧 × 50 ms）、受击 300 ms、死亡 400 ms 后依次结束（死亡转 `Dead`）；
//! 3. 自己的走/攻击由本地命令驱动（服务端不会给自己发 `ObjectWalk`）；
//! 4. 渲染 `target/client-anim.png`：一个正在走路的玩家 + 一只正在攻击的怪物，
//!    并断言非空白、两个时间点的画面不同。

use std::path::{Path, PathBuf};

use mir_client::net::{EQUIPMENT_SLOTS, NetCommand, WorldSnapshot};
use mir_core::Point;
use mir_core::enums::{EquipmentSlot, ItemType, MirClass, MirDirection, MirGender, Monster};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ItemInfo, UserItem};
use mir_protocol::packets::server::{
    NewItemInfo, ObjectAttack, ObjectDied, ObjectMonster, ObjectPlayer, ObjectStruck, ObjectWalk,
    UserInformation,
};
use mir_render::animation::ActorAction;
use mir_render::{EntityLibraries, EntityView, Frame, MapScene, TileLibraries, Viewport};
use mir_world::{Map, MapCell, MapVersion};

/// 测试用的固定时间基准（Unix 毫秒）。
const T0: u64 = 1_000_000;
/// 自己。
const SELF_ID: u32 = 7;
/// 路人玩家（走路的那个）。
const WALKER_ID: u32 = 901;
/// 怪物（攻击的那个）。
const MONSTER_ID: u32 = 902;
/// 合成地图上的中心格。
const CENTER: Point = Point::new(10, 10);

/// 工作区根目录。
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// 无 `resource/` 时的合成空地图。
fn synthetic_map() -> Map {
    let width = 64;
    let height = 64;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "synthetic-anim".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
}

fn user_item(unique_id: u64, item_index: i32) -> UserItem {
    UserItem {
        unique_id,
        item_index,
        count: 1,
        current_dura: 5_000,
        max_dura: 5_000,
        ..UserItem::default()
    }
}

fn item_info(index: i32, name: &str, item_type: ItemType, shape: i16, image: u16) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        item_type,
        shape,
        image,
        durability: 5_000,
        price: 1_000,
        ..ItemInfo::default()
    }
}

/// 注入自己 + 一个路人玩家 + 一只怪物（全部在 `T0` 之前创建）。
fn injected_world() -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();

    for info in [
        item_info(30, "炼狱", ItemType::Weapon, 36, 30),
        item_info(31, "天魔神甲", ItemType::Armour, 2, 31),
    ] {
        snapshot.apply_packet_at(&ServerPacket::NewItemInfo(NewItemInfo { info }), 0);
    }
    let mut equipment = vec![None; EQUIPMENT_SLOTS];
    equipment[EquipmentSlot::Weapon as usize] = Some(user_item(1_001, 30));
    equipment[EquipmentSlot::Armour as usize] = Some(user_item(1_002, 31));
    snapshot.apply_packet_at(
        &ServerPacket::UserInformation(UserInformation {
            object_id: SELF_ID,
            real_id: 1,
            name: "动画测试".to_owned(),
            location: CENTER,
            direction: MirDirection::Down,
            class: MirClass::Warrior,
            gender: MirGender::Male,
            level: 22,
            hair: 2,
            hp: 88,
            mp: 30,
            gold: 1_234,
            equipment: Some(equipment),
            inventory: Some(vec![None; 46]),
            ..UserInformation::default()
        }),
        0,
    );

    snapshot.apply_packet_at(
        &ServerPacket::ObjectPlayer(ObjectPlayer {
            object_id: WALKER_ID,
            name: "走路的人".to_owned(),
            class: MirClass::Taoist,
            gender: MirGender::Female,
            level: 20,
            location: Point::new(CENTER.x + 1, CENTER.y),
            direction: MirDirection::Down,
            hair: 2,
            armour: 2,
            weapon: 36,
            ..ObjectPlayer::default()
        }),
        0,
    );

    snapshot.apply_packet_at(
        &ServerPacket::ObjectMonster(ObjectMonster {
            object_id: MONSTER_ID,
            name: "攻击的怪".to_owned(),
            location: Point::new(CENTER.x + 2, CENTER.y),
            image: Monster::Guard,
            direction: MirDirection::Down,
            ..ObjectMonster::default()
        }),
        0,
    );

    snapshot
}

/// 按名字找一个实体的渲染视图。
fn view_named(snapshot: &WorldSnapshot, now_ms: u64, name: &str) -> EntityView {
    snapshot
        .entity_views(now_ms)
        .into_iter()
        .find(|view| view.name.as_deref() == Some(name))
        .expect("实体应存在于快照")
}

#[test]
fn object_walk_drives_a_walking_animation_for_600_ms() {
    let mut snapshot = injected_world();
    snapshot.apply_packet_at(
        &ServerPacket::ObjectWalk(ObjectWalk {
            object_id: WALKER_ID,
            location: Point::new(CENTER.x + 2, CENTER.y + 1),
            direction: MirDirection::DownRight,
        }),
        T0,
    );

    let walker = &snapshot.entities[&WALKER_ID];
    assert_eq!(walker.action, ActorAction::Walking);
    assert_eq!(walker.action_started_ms, T0);
    assert_eq!(walker.direction, MirDirection::DownRight);

    // 600 ms 内：动作在走，帧号随时间变化。
    let view = view_named(&snapshot, T0 + 100, "走路的人");
    assert_eq!(view.action, ActorAction::Walking);
    let animation = view.animation();
    let first = animation.frame_at(T0);
    let middle = animation.frame_at(T0 + 300);
    assert!(!first.finished);
    assert!(!middle.finished);
    assert_ne!(first.index, middle.index, "走路动画在不同时间必须取不同帧");
    assert_eq!(animation.duration_ms(), 600, "Crystal 行走 = 6 帧 × 100 ms");

    // 599 ms 还在走；600 ms 后回 Standing。
    snapshot.tick_animations(T0 + 599);
    assert_eq!(snapshot.entities[&WALKER_ID].action, ActorAction::Walking);
    snapshot.tick_animations(T0 + 600);
    assert_eq!(snapshot.entities[&WALKER_ID].action, ActorAction::Standing);
    let view = view_named(&snapshot, T0 + 600, "走路的人");
    assert_eq!(view.action, ActorAction::Standing);
    assert_eq!(
        view.animation().frame_at(T0 + 600).index,
        mir_render::animation::HUMAN_STANDING.index_of(MirDirection::DownRight, 0),
        "走完后画站立第 0 帧"
    );
}

#[test]
fn object_run_and_turn_switch_actions() {
    let mut snapshot = injected_world();

    snapshot.apply_packet_at(
        &ServerPacket::ObjectRun(mir_protocol::packets::server::ObjectRun {
            object_id: WALKER_ID,
            location: Point::new(CENTER.x + 3, CENTER.y + 1),
            direction: MirDirection::DownRight,
        }),
        T0,
    );
    assert_eq!(snapshot.entities[&WALKER_ID].action, ActorAction::Running);
    let view = view_named(&snapshot, T0 + 200, "走路的人");
    assert_eq!(view.action, ActorAction::Running);
    assert_ne!(
        view.animation().frame_at(T0).index,
        view.animation().frame_at(T0 + 300).index
    );
    snapshot.tick_animations(T0 + 600);
    assert_eq!(snapshot.entities[&WALKER_ID].action, ActorAction::Standing);

    // 转身：立即回到 Standing，并更新朝向。
    snapshot.apply_packet_at(
        &ServerPacket::ObjectTurn(mir_protocol::packets::server::ObjectTurn {
            object_id: WALKER_ID,
            location: Point::new(CENTER.x + 3, CENTER.y + 1),
            direction: MirDirection::Up,
        }),
        T0 + 1_000,
    );
    assert_eq!(snapshot.entities[&WALKER_ID].action, ActorAction::Standing);
    assert_eq!(snapshot.entities[&WALKER_ID].direction, MirDirection::Up);
    assert_eq!(snapshot.entities[&WALKER_ID].action_started_ms, T0 + 1_000);
}

#[test]
fn self_death_packet_runs_dying_then_dead() {
    let mut snapshot = injected_world();
    snapshot.apply_packet_at(
        &ServerPacket::Death(mir_protocol::packets::server::Death {
            location: CENTER,
            direction: MirDirection::Down,
        }),
        T0,
    );
    let me = snapshot.self_player.as_ref().expect("自己");
    assert_eq!(me.action, ActorAction::Dying);
    assert!(me.dead);
    snapshot.tick_animations(T0 + 399);
    assert_eq!(
        snapshot.self_player.as_ref().expect("自己").action,
        ActorAction::Dying
    );
    snapshot.tick_animations(T0 + 400);
    let me = snapshot.self_player.as_ref().expect("自己");
    assert_eq!(me.action, ActorAction::Dead);
    assert!(me.dead);
    let view = view_named(&snapshot, T0 + 400, "动画测试");
    assert_eq!(view.action, ActorAction::Dead);
}

#[test]
fn object_attack_and_struck_and_died_follow_crystal_durations() {
    let mut snapshot = injected_world();

    // 原生怪物攻击：6 帧 × 100 ms = 600 ms（t41 固定生命期）。
    snapshot.apply_packet_at(
        &ServerPacket::ObjectAttack(ObjectAttack {
            object_id: MONSTER_ID,
            location: Point::new(CENTER.x + 2, CENTER.y),
            direction: MirDirection::Down,
            ..ObjectAttack::default()
        }),
        T0,
    );
    assert_eq!(
        snapshot.entities[&MONSTER_ID].action,
        ActorAction::Attacking
    );
    let view = view_named(&snapshot, T0 + 100, "攻击的怪");
    assert_eq!(view.action, ActorAction::Attacking);
    assert_ne!(
        view.animation().frame_at(T0).index,
        view.animation().frame_at(T0 + 150).index,
        "攻击动画在不同时间必须取不同帧"
    );
    snapshot.tick_animations(T0 + 599);
    assert_eq!(
        snapshot.entities[&MONSTER_ID].action,
        ActorAction::Attacking,
        "600 ms 之前不能提前结束"
    );
    snapshot.tick_animations(T0 + 600);
    assert_eq!(snapshot.entities[&MONSTER_ID].action, ActorAction::Standing);

    // 受击：人类 3 帧 × 100 ms = 300 ms。
    snapshot.apply_packet_at(
        &ServerPacket::ObjectStruck(ObjectStruck {
            object_id: WALKER_ID,
            attacker_id: MONSTER_ID,
            location: Point::new(CENTER.x + 1, CENTER.y),
            direction: MirDirection::Left,
        }),
        T0 + 1_000,
    );
    assert_eq!(snapshot.entities[&WALKER_ID].action, ActorAction::Struck);
    snapshot.tick_animations(T0 + 1_299);
    assert_eq!(snapshot.entities[&WALKER_ID].action, ActorAction::Struck);
    snapshot.tick_animations(T0 + 1_300);
    assert_eq!(snapshot.entities[&WALKER_ID].action, ActorAction::Standing);

    // 死亡：人类 4 帧 × 100 ms = 400 ms，之后变 Dead。
    snapshot.apply_packet_at(
        &ServerPacket::ObjectDied(ObjectDied {
            object_id: WALKER_ID,
            location: Point::new(CENTER.x + 1, CENTER.y),
            direction: MirDirection::Left,
            ..ObjectDied::default()
        }),
        T0 + 2_000,
    );
    assert_eq!(snapshot.entities[&WALKER_ID].action, ActorAction::Dying);
    assert!(snapshot.entities[&WALKER_ID].dead);
    snapshot.tick_animations(T0 + 2_399);
    assert_eq!(snapshot.entities[&WALKER_ID].action, ActorAction::Dying);
    snapshot.tick_animations(T0 + 2_400);
    assert_eq!(snapshot.entities[&WALKER_ID].action, ActorAction::Dead);
    let view = view_named(&snapshot, T0 + 2_400, "走路的人");
    assert_eq!(view.action, ActorAction::Dead);
    assert!(view.dead);
    assert_eq!(
        view.animation().frame_at(T0 + 2_400).index,
        mir_render::animation::HUMAN_DEAD.index_of(MirDirection::Left, 0),
        "尸体帧来自 Frames.cs:170 的 Dead"
    );
}

#[test]
fn local_player_actions_are_driven_without_a_server_echo() {
    let mut snapshot = injected_world();

    // 自己的走：服务端只回 UserLocation，客户端必须在发包时本地驱动。
    assert_eq!(
        NetCommand::Walk(MirDirection::Right).local_animation(),
        Some((ActorAction::Walking, MirDirection::Right, None, None))
    );
    snapshot.apply_local_action(ActorAction::Walking, MirDirection::Right, T0);
    let me = snapshot.self_player.as_ref().expect("自己");
    assert_eq!(me.action, ActorAction::Walking);
    assert_eq!(me.action_started_ms, T0);
    let view = view_named(&snapshot, T0, "动画测试");
    assert_eq!(view.action, ActorAction::Walking);
    assert_ne!(
        view.animation().frame_at(T0).index,
        view.animation().frame_at(T0 + 200).index
    );
    snapshot.tick_animations(T0 + 600);
    assert_eq!(
        snapshot.self_player.as_ref().expect("自己").action,
        ActorAction::Standing
    );

    // 自己的攻击：300 ms（6 帧 × 50 ms）。
    snapshot.apply_local_action(ActorAction::Attacking, MirDirection::Down, T0 + 1_000);
    assert_eq!(
        snapshot.self_player.as_ref().expect("自己").action,
        ActorAction::Attacking
    );
    snapshot.tick_animations(T0 + 1_299);
    assert_eq!(
        snapshot.self_player.as_ref().expect("自己").action,
        ActorAction::Attacking
    );
    snapshot.tick_animations(T0 + 1_300);
    assert_eq!(
        snapshot.self_player.as_ref().expect("自己").action,
        ActorAction::Standing
    );

    // 本地动作也会带朝向。
    snapshot.apply_local_action(ActorAction::Running, MirDirection::UpRight, T0 + 2_000);
    assert_eq!(
        snapshot.self_player.as_ref().expect("自己").direction,
        MirDirection::UpRight
    );
    assert_eq!(
        snapshot.self_player.as_ref().expect("自己").action,
        ActorAction::Running
    );
}

#[test]
fn walking_player_and_attacking_monster_render_to_a_non_blank_png() {
    let root = workspace_root();
    let data_dir = root.join("resource/data");
    let libraries = if data_dir.join("hum.wzl").is_file() {
        EntityLibraries::open_dir(&data_dir)
    } else {
        eprintln!("跳过真实素材：{} 不存在，使用占位方块", data_dir.display());
        EntityLibraries::empty(&data_dir)
    };
    let has_assets = libraries.has(mir_render::SpriteLibrary::Hum(1));

    let mut snapshot = injected_world();
    // 自己走（朝右下），怪物在 100 ms 后开始攻击。
    snapshot.apply_local_action(ActorAction::Walking, MirDirection::DownRight, T0);
    snapshot.apply_packet_at(
        &ServerPacket::ObjectAttack(ObjectAttack {
            object_id: MONSTER_ID,
            location: Point::new(CENTER.x + 2, CENTER.y),
            direction: MirDirection::Down,
            ..ObjectAttack::default()
        }),
        T0 + 100,
    );
    // 走/攻击都在 600 ms 内，取 250 ms 时两边的帧都不是第 0 帧。
    let now = T0 + 250;
    let views = snapshot.entity_views(now);
    assert_eq!(
        views
            .iter()
            .find(|view| view.name.as_deref() == Some("动画测试"))
            .expect("自己")
            .action,
        ActorAction::Walking
    );
    assert_eq!(
        views
            .iter()
            .find(|view| view.name.as_deref() == Some("攻击的怪"))
            .expect("怪物")
            .action,
        ActorAction::Attacking
    );

    let mut scene = MapScene::from_parts_with_entities(
        synthetic_map(),
        TileLibraries::empty("."),
        Viewport::new(640, 480),
        libraries,
    );
    let mut frame = Frame::new(640, 480);
    scene.render_with_entities_at(&mut frame, CENTER, &views, now);

    if !has_assets {
        // CI 上没有 `resource/`（被 gitignore）：占位方块不随帧变化，颜色数与帧间差异断言
        // 没有意义；本地有素材时才校验真实图库输出。
        println!("跳过像素断言：缺少真实素材（resource/ 被 gitignore）");
        return;
    }
    let painted = frame
        .pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] != 0 && (pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0))
        .count();
    let threshold = if has_assets { 2_000 } else { 100 };
    assert!(
        painted > threshold,
        "画面太暗（{painted} 个非黑像素，阈值 {threshold}）"
    );
    // 没有真实图库时（CI 的 resource/ 被 gitignore）只画占位色块，颜色数与帧间差异都更小，
    // 阈值随之放宽；有素材时仍要求真实图库的多色输出。
    let colors = frame.distinct_color_count();
    let color_threshold = if has_assets { 8 } else { 3 };
    assert!(
        colors > color_threshold,
        "应有玩家/怪物/地形的多种颜色，实际 {colors}（阈值 {color_threshold}）"
    );

    // 另一个时间点（走第 4 帧、攻击第 2 帧）必须与第一帧不同：动画真的在动。
    let mut later = Frame::new(640, 480);
    scene.render_with_entities_at(&mut later, CENTER, &views, now + 200);
    let changed = frame
        .pixels
        .chunks_exact(4)
        .zip(later.pixels.chunks_exact(4))
        .filter(|(left, right)| left != right)
        .count();
    let moved_threshold = if has_assets { 100 } else { 20 };
    assert!(
        changed > moved_threshold,
        "两个时间点应有明显像素差异，实际 {changed}（阈值 {moved_threshold}）"
    );

    let output = root.join("target/client-anim.png");
    frame.save_png(&output).expect("写出 client-anim.png");
    println!(
        "client-anim.png：{} · {painted} 个非黑像素 · 两帧差异 {changed} px · 素材={}",
        output.display(),
        if has_assets { "真实" } else { "占位" }
    );
    println!(
        "动作帧：玩家走 {} 帧/方向，怪物攻击 {} 帧/方向，怪物死亡 {} 帧/方向",
        mir_render::animation::HUMAN_WALKING.count,
        mir_render::animation::MONSTER_ATTACK1.count,
        mir_render::animation::MONSTER_DYING.count
    );
}
