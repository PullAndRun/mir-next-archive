//! 无头交互测试：真实登录 + 注入装备/物品/buff/伤害包，验证交互与渲染。
//!
//! 流程：
//!
//! 1. 进程内起真实 `mir-server`，客户端走完「注册 → 登录 → 建角 → 进世界」；
//! 2. 用 `WorldSnapshot::apply_packet` 注入 `UserInformation`（14 个装备槽 + 背包）、
//!    `NewItemInfo`（带 `ItemInfo.stats`）、`ObjectMonster`、`AddBuff`/`RemoveBuff`、
//!    `ObjectStruck` 与 `DamageIndicator`；
//! 3. 用可注入时钟（`UiState::set_time`）在 `T+0.5s` 与 `T+2.6s` 各画一帧，断言伤害
//!    飘字存在/消失，不需要真实 sleep；
//! 4. 断言 tooltip 文本像素、双击产生 `NetCommand::UseItem`、拖拽产生
//!    `NetCommand::MoveItem{grid: Inventory, …}`、buff 栏显示剩余秒数；
//! 5. 输出 `target/client-interact.png`。

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use mir_client::geom::Rect;
use mir_client::net::{
    ConnectOptions, CreateCharacter, EQUIPMENT_SLOTS, NetClient, NetCommand, WorldSnapshot,
};
use mir_client::ui::{self, UiAction, UiState, buffs, equipment, inventory, tooltip};
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{
    BuffType, ChatType, DamageType, EquipmentSlot, ItemType, MirClass, MirDirection, MirGender,
    Monster, SpellEffect, Stat,
};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ClientBuff, ItemInfo, Stats, UserItem};
use mir_protocol::packets::server::{
    AddBuff, Chat, DamageIndicator, NewItemInfo, ObjectEffect, ObjectMonster, ObjectStruck,
    RemoveBuff, UserInformation,
};
use mir_render::{Frame, MapScene, TileLibraries, Viewport};
use mir_server::{MemoryStore, ServerConfig, serve_forever};
use mir_world::{Map, MapCell, MapVersion};

/// 登录链路等待上限。
const LOGIN_TIMEOUT: Duration = Duration::from_secs(15);
/// 注入的稻草人对象 ID。
const MONSTER_ID: u32 = 90_002;
/// 可注入时钟的起点（Unix 毫秒；用固定值让 buff 倒计时可预期）。
const NOW: i64 = 1_700_000_000_000;
/// buff 剩余时长。
const BUFF_MILLIS: i64 = 8_000;

/// 工作区根目录（`CARGO_MANIFEST_DIR` 是 `crates/mir-client`）。
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// 合成地图：不依赖 `resource/`，渲染结果确定，便于断言文字像素。
fn synthetic_map() -> Map {
    let width = 64;
    let height = 64;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "synthetic-interact".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
}

fn stats(entries: &[(Stat, i32)]) -> Stats {
    let mut stats = Stats::default();
    for (stat, value) in entries {
        stats.set(*stat, *value);
    }
    stats
}

fn user_item(unique_id: u64, item_index: i32, count: u16, dura: u16) -> UserItem {
    UserItem {
        unique_id,
        item_index,
        count,
        current_dura: dura,
        max_dura: dura,
        ..UserItem::default()
    }
}

/// 注入一套「带装备、药品、卷轴、buffs 与怪物」的界面数据。
fn inject_fake_world(snapshot: &mut WorldSnapshot) {
    let self_id = snapshot.self_player.as_ref().map_or(1, |me| me.object_id);
    let position = snapshot
        .self_player
        .as_ref()
        .map_or(Point::new(330, 270), |me| me.position);

    let mut equipment = vec![None; EQUIPMENT_SLOTS];
    equipment[EquipmentSlot::Weapon as usize] = Some(user_item(21, 10, 1, 5_000));
    equipment[EquipmentSlot::Armour as usize] = Some(user_item(22, 11, 1, 4_000));

    let mut inventory = vec![None; 46];
    inventory[0] = Some(user_item(31, 12, 7, 0)); // 金创药
    inventory[1] = Some(user_item(32, 13, 1, 0)); // 回城卷
    inventory[2] = Some(user_item(33, 10, 1, 5_000)); // 炼狱（备用武器）

    snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
        object_id: self_id,
        real_id: 1,
        name: "交互测试".to_owned(),
        location: position,
        direction: MirDirection::Down,
        class: MirClass::Warrior,
        gender: MirGender::Male,
        level: 22,
        hp: 50,
        mp: 20,
        gold: 1_234,
        inventory: Some(inventory),
        equipment: Some(equipment),
        ..UserInformation::default()
    }));

    let item_infos = [
        (
            10,
            "炼狱",
            ItemType::Weapon,
            22,
            stats(&[(Stat::MinDC, 5), (Stat::MaxDC, 12), (Stat::Accuracy, 2)]),
        ),
        (
            11,
            "重盔甲",
            ItemType::Armour,
            20,
            stats(&[(Stat::MinAC, 3), (Stat::MaxAC, 7), (Stat::Agility, 1)]),
        ),
        (12, "金创药", ItemType::Potion, 1, Stats::default()),
        (13, "回城卷", ItemType::Scroll, 1, Stats::default()),
    ];
    for (index, name, item_type, required_amount, stats) in item_infos {
        let default_dura = if matches!(item_type, ItemType::Weapon | ItemType::Armour) {
            5_000
        } else {
            0
        };
        snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
            info: ItemInfo {
                index,
                name: name.to_owned(),
                image: u16::try_from(index).unwrap_or(0),
                price: 1_200,
                stack_size: if item_type == ItemType::Potion {
                    100
                } else {
                    1
                },
                item_type,
                required_amount,
                durability: default_dura,
                stats,
                ..ItemInfo::default()
            },
        }));
    }

    snapshot.apply_packet(&ServerPacket::ObjectMonster(ObjectMonster {
        object_id: MONSTER_ID,
        name: "稻草人".to_owned(),
        image: Monster::Scarecrow,
        location: position.moved(MirDirection::Right, 2),
        direction: MirDirection::Left,
        ..ObjectMonster::default()
    }));

    snapshot.apply_packet(&ServerPacket::AddBuff(AddBuff {
        buff: ClientBuff {
            buff_type: BuffType::MagicShield,
            visible: true,
            object_id: self_id,
            expire_time: NOW + BUFF_MILLIS,
            infinite: false,
            paused: false,
            stats: Stats::default(),
            values: Vec::new(),
        },
    }));

    // 魔法盾上盾光效（UI 画一圈半透明圆环）。
    snapshot.apply_packet(&ServerPacket::ObjectEffect(ObjectEffect {
        object_id: self_id,
        effect: SpellEffect::MagicShieldUp,
        effect_type: 0,
        delay_time: 0,
        time: 600,
    }));

    snapshot.apply_packet(&ServerPacket::Chat(Chat {
        message: "交互测试：双击使用、右键菜单、拖拽换装".to_owned(),
        type_: ChatType::System,
    }));
}

/// 统计矩形里满足阈值的高亮像素（文字像素；深色背景上文字是亮色）。
fn text_pixels(frame: &Frame, rect: Rect) -> usize {
    let mut count = 0;
    let left = rect.x.max(0);
    let top = rect.y.max(0);
    let right = rect
        .right()
        .min(i32::try_from(frame.width).unwrap_or(i32::MAX));
    let bottom = rect
        .bottom()
        .min(i32::try_from(frame.height).unwrap_or(i32::MAX));
    for y in top..bottom {
        for x in left..right {
            if let Some([r, g, b, _]) =
                frame.pixel(u32::try_from(x).unwrap_or(0), u32::try_from(y).unwrap_or(0))
            {
                if r >= 150 && g >= 100 && b >= 90 {
                    count += 1;
                }
            }
        }
    }
    count
}

/// 统计矩形里「可见」的像素（伤害数字会淡出到约一半亮度，阈值要比 tooltip 宽）。
fn visible_pixels(frame: &Frame, rect: Rect) -> usize {
    let mut count = 0;
    let left = rect.x.max(0);
    let top = rect.y.max(0);
    let right = rect
        .right()
        .min(i32::try_from(frame.width).unwrap_or(i32::MAX));
    let bottom = rect
        .bottom()
        .min(i32::try_from(frame.height).unwrap_or(i32::MAX));
    for y in top..bottom {
        for x in left..right {
            if let Some([r, g, b, _]) =
                frame.pixel(u32::try_from(x).unwrap_or(0), u32::try_from(y).unwrap_or(0))
            {
                if r >= 80 && g >= 80 && b >= 80 {
                    count += 1;
                }
            }
        }
    }
    count
}

/// 统计两个帧在矩形内变化的像素数。
fn changed_pixels(before: &Frame, after: &Frame, rect: Rect) -> usize {
    let mut count = 0;
    let left = rect.x.max(0);
    let top = rect.y.max(0);
    let right = rect
        .right()
        .min(i32::try_from(after.width).unwrap_or(i32::MAX));
    let bottom = rect
        .bottom()
        .min(i32::try_from(after.height).unwrap_or(i32::MAX));
    for y in top..bottom {
        for x in left..right {
            let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
                continue;
            };
            if before.pixel(x, y) != after.pixel(x, y) {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn interaction_tooltip_use_drag_damage_and_buffs() {
    let root = workspace_root();
    let map_dir = root.join("resource/map");

    // 1. 真实服务端 + 登录链路。
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("创建 tokio runtime");
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取临时端口");
    let config = Arc::new(ServerConfig {
        listen: address,
        map_root: map_dir.display().to_string(),
        language_dir: root.join("config/lang").display().to_string(),
        load_game_data: false,
        demo_monsters: true,
        ..ServerConfig::default()
    });
    let store = Arc::new(MemoryStore::new());
    runtime.spawn(serve_forever(
        listener,
        Arc::clone(&config),
        Arc::clone(&store),
    ));

    let client = NetClient::connect(ConnectOptions {
        server: address.to_string(),
        account: "interactacct".to_owned(),
        password: "interactpass".to_owned(),
        character: "InteractHero".to_owned(),
        create: Some(CreateCharacter {
            name: "InteractHero".to_owned(),
            class: MirClass::Warrior,
            gender: MirGender::Male,
        }),
        auto_register: true,
        manual_select: false,
    })
    .expect("启动客户端网络线程");
    let mut snapshot = client
        .wait_until_in_game(LOGIN_TIMEOUT)
        .expect("客户端应走完登录链路进入世界");
    assert!(snapshot.is_ready());

    // 2. 注入假数据。
    inject_fake_world(&mut snapshot);
    assert_eq!(
        snapshot.equipment_item(0).map(|item| item.item_index),
        Some(10)
    );
    assert_eq!(snapshot.equipment.len(), EQUIPMENT_SLOTS);
    assert_eq!(snapshot.buffs.len(), 1);
    assert_eq!(
        snapshot.inventory_item(2).map(|item| item.unique_id),
        Some(33)
    );

    // 3. 合成地图 + 场景。
    let mut scene = MapScene::from_parts(
        synthetic_map(),
        TileLibraries::empty("."),
        Viewport::DEFAULT,
    );
    let center = snapshot
        .self_player
        .as_ref()
        .map_or(Point::new(330, 270), |me| me.position);
    let mut frame = Frame::new(800, 600);
    let entities = snapshot.entity_views(0);
    scene.render_with_entities(&mut frame, center, &entities);
    let world_only = frame.clone();

    let text = common::load_text();
    let mut state = UiState::new(Language::ZhCn);
    state.set_time(NOW);
    state.show_inventory = true;
    state.show_equipment = true;
    state.sync(&snapshot);
    assert_eq!(state.effects.len(), 1, "ObjectEffect 应产生光效圆环");
    let screen = Rect::screen(800, 600);
    let inventory_panel = inventory::inventory_panel_rect(screen);
    let equipment_layout = equipment::equipment_layout(screen);

    // 4. tooltip：鼠标悬停背包里的「炼狱」。
    state.mouse = inventory::inventory_slot_rect(inventory_panel, 2)
        .expect("2 号格")
        .center();
    let content = tooltip::content_for_item(
        &snapshot,
        snapshot.inventory_item(2).unwrap(),
        Language::ZhCn,
    );
    assert_eq!(content.title, "炼狱");
    let tooltip_layout = tooltip::layout(&content, &text, screen, state.mouse);
    ui::draw(
        &mut state, &mut frame, &mut scene, &snapshot, &text, None, center,
    );
    let tooltip_pixels = text_pixels(&frame, tooltip_layout.panel);
    assert!(
        tooltip_pixels > 0,
        "tooltip 应有文字像素，实际 {tooltip_pixels}"
    );
    assert!(
        changed_pixels(&world_only, &frame, inventory_panel) > 100,
        "背包面板应覆盖地图"
    );
    assert!(
        changed_pixels(&world_only, &frame, equipment_layout.panel) > 100,
        "装备面板应覆盖地图"
    );

    // 5. buff 栏：剩余 8 秒，文字像素非空；倒计时随注入时钟变化。
    let visible = snapshot.buffs.iter().filter(|buff| buff.visible).count();
    let buff_left = buffs::bar_left(state.show_equipment, state.shop_open);
    let buff_rect = buffs::entry_rects(screen, visible, buff_left)
        .first()
        .copied()
        .expect("应有 buff 条目");
    let buff_label = buffs::buff_text(&snapshot.buffs[0], NOW, Language::ZhCn);
    assert!(
        buff_label.contains('8'),
        "buff 文本应带剩余秒数：{buff_label}"
    );
    assert!(buff_label.contains("秒"), "中文 buff 文本：{buff_label}");
    assert!(
        buffs::buff_text(&snapshot.buffs[0], NOW + 3_000, Language::ZhCn).contains('5'),
        "3 秒后应剩 5 秒"
    );
    assert!(text_pixels(&frame, buff_rect) > 0, "buff 栏应有文字像素");
    // RemoveBuff 清掉图标。
    snapshot.apply_packet(&ServerPacket::RemoveBuff(RemoveBuff {
        type_: BuffType::MagicShield,
        object_id: snapshot.self_player.as_ref().unwrap().object_id,
    }));
    assert!(snapshot.buffs.is_empty(), "RemoveBuff 应移除 buff");
    // 重新加回来，供最终 PNG 使用。
    snapshot.apply_packet(&ServerPacket::AddBuff(AddBuff {
        buff: ClientBuff {
            buff_type: BuffType::MagicShield,
            visible: true,
            object_id: snapshot.self_player.as_ref().unwrap().object_id,
            expire_time: NOW + BUFF_MILLIS,
            infinite: false,
            paused: false,
            stats: Stats::default(),
            values: Vec::new(),
        },
    }));

    // 6. 伤害数字：ObjectStruck + DamageIndicator 在 +0.5s 内可见，2s 后消失。
    let self_id = snapshot.self_player.as_ref().unwrap().object_id;
    state.set_time(NOW);
    snapshot.apply_packet(&ServerPacket::ObjectStruck(ObjectStruck {
        object_id: MONSTER_ID,
        attacker_id: self_id,
        location: snapshot.entities[&MONSTER_ID].position,
        direction: MirDirection::Right,
    }));
    snapshot.apply_packet(&ServerPacket::DamageIndicator(DamageIndicator {
        damage: 37,
        type_: DamageType::Hit,
        object_id: MONSTER_ID,
    }));
    state.sync(&snapshot);
    assert_eq!(
        state.damage_numbers.len(),
        2,
        "ObjectStruck/DamageIndicator 各一条"
    );
    // 事件发生在 NOW；在 NOW+500ms 画帧（飘字已上浮 25px、仍未淡出）。
    state.set_time(NOW + 500);
    state.sync(&snapshot);
    assert_eq!(state.damage_numbers.len(), 2, "+500ms 时飘字仍存在");
    assert_eq!(state.effects.len(), 1, "+500ms 时光效仍在播放");
    ui::draw(
        &mut state, &mut frame, &mut scene, &snapshot, &text, None, center,
    );
    let monster_position = snapshot.entities[&MONSTER_ID].position;
    let (monster_x, monster_y) = scene.world_to_screen(center, monster_position);
    // 只统计怪物头顶那一条（避开右侧背包面板 x≥524 与头顶名字 y≈212）。
    let damage_rect = Rect::new(monster_x - 40, monster_y - 120, 80, 58);
    let damage_pixels = visible_pixels(&frame, damage_rect);
    assert!(
        damage_pixels > 0,
        "+500ms 伤害数字应可见，实际 {damage_pixels}"
    );

    // 7. 保存 PNG（此时 tooltip/装备/背包/buff/伤害数字同时可见）。
    let output = root.join("target/client-interact.png");
    frame
        .save_png(&output)
        .expect("写出 target/client-interact.png");
    let metadata = std::fs::metadata(&output).expect("PNG 应存在");
    assert!(metadata.len() > 4_096, "PNG 不应为空文件");
    assert!(
        frame.distinct_color_count() >= 20,
        "整帧颜色太少：{}",
        frame.distinct_color_count()
    );

    // 8. +2.6s：飘字消失。用一帧干净画面断言同一区域不再有文字像素
    // （不能在旧帧上叠加，否则会数到上一帧的飘字）。
    state.set_time(NOW + 2_600);
    state.sync(&snapshot);
    assert!(state.damage_numbers.is_empty(), "2s 后伤害数字应消失");
    assert!(state.effects.is_empty(), "光效应已结束");
    let mut clean = Frame::new(800, 600);
    // 只保留注入的怪物：服务端的演示怪物会自由走动，登录耗时不同会让它们停在
    // 不同格子，名字/占位方块有可能落进断言矩形，造成与飘字无关的亮像素（测试抖动）。
    let mut clean_snapshot = snapshot.clone();
    clean_snapshot.entities.retain(|id, _| *id == MONSTER_ID);
    let clean_entities = clean_snapshot.entity_views(0);
    scene.render_with_entities(&mut clean, center, &clean_entities);
    ui::draw(
        &mut state,
        &mut clean,
        &mut scene,
        &clean_snapshot,
        &text,
        None,
        center,
    );
    assert_eq!(
        visible_pixels(&clean, damage_rect),
        0,
        "飘字消失后同一区域不应再有文字"
    );
    // 9. 双击背包 → UseItem（并映射成 NetCommand::UseItem）。
    state.set_time(NOW + 3_000);
    state.mouse = inventory::inventory_slot_rect(inventory_panel, 0)
        .expect("0 号格")
        .center();
    assert!(
        state.on_click(&snapshot, &text, screen, None).is_none(),
        "第一次点击只选中并开始拖拽"
    );
    state.set_time(NOW + 3_120);
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("第二次点击应是双击使用");
    assert_eq!(action, UiAction::UseItem { unique_id: 31 });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::UseItem { unique_id: 31 }),
        "双击应产生 NetCommand::UseItem"
    );

    // 10. 拖拽背包 → 装备槽 → MoveItem{grid: Inventory, from, to}。
    state.set_time(NOW + 4_000);
    state.mouse = inventory::inventory_slot_rect(inventory_panel, 2)
        .expect("2 号格")
        .center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert!(state.drag.is_some(), "按住物品应开始拖拽");
    state.mouse = equipment_layout.slots[EquipmentSlot::Weapon as usize].center();
    let action = state
        .on_mouse_release(&snapshot, screen)
        .expect("拖到装备槽应产生移动");
    assert_eq!(
        action,
        UiAction::MoveItem {
            grid: mir_core::enums::MirGridType::Inventory,
            from: 2,
            to: EquipmentSlot::Weapon as i32,
            to_grid: mir_core::enums::MirGridType::Equipment,
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::MoveItem {
            grid: mir_core::enums::MirGridType::Inventory,
            from: 2,
            to: EquipmentSlot::Weapon as i32,
            to_grid: mir_core::enums::MirGridType::Equipment,
        }),
        "拖拽应产生 MoveItem{{grid: Inventory, from, to}}"
    );

    println!(
        "client-interact.png：{} · {}×{} · {} 种颜色 · 字体 {} · tooltip {} px · 伤害 {} px · buff「{}」",
        output.display(),
        frame.width,
        frame.height,
        frame.distinct_color_count(),
        text.font_path()
            .map_or_else(|| "3×5 点阵".to_owned(), |path| path.display().to_string()),
        tooltip_pixels,
        damage_pixels,
        buff_label,
    );

    drop(client);
    drop(runtime);
}
