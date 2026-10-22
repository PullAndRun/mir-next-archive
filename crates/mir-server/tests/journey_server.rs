//! 同一角色「自然旅程」：真实 cjlaaa 数据 + 真实 map0/0104 + 真实 combat/掉落/门/NPC/任务/死亡/重登。
//!
//! 与旧 `same_character_core_loop_journey` 的区别：不放大属性、不手动造 850、不搬书店 NPC、
//! 不直接写 `dead/hp/revive_time`；用固定 `MirRng` 种子让 1/5 鸡肉掉落可复现（最多 20 只鸡）。

mod common;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{
    ItemType, MirClass, MirDirection, MirGender, MirGridType, PanelType, PetMode, Spell,
};
use mir_core::i18n::LanguageLibrary;
use mir_protocol::ServerPacket;
use mir_protocol::data::UserItem;
use mir_protocol::packets::client as c;
use mir_server::combat::CombatStats;
use mir_server::items::apply_equipment_stats;
use mir_server::market;
use mir_server::npc::item_price;
use mir_server::session::Session;
use mir_server::store::{AccountStore, Character};
use mir_server::{MemoryStore, ServerConfig, World};
use tokio::sync::{Mutex, mpsc};

const MAP: &str = "0";
const BOOK_SHOP_MAP: &str = "0104";
const CHICKEN_ITEM: i32 = 850;
const HEALING_BOOK: i32 = 1016;
const ATTACK_BOOK: i32 = 973;
const STARTER_COUNT: usize = 5;

fn take_packets(rx: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<ServerPacket> {
    let mut packets = Vec::new();
    while let Ok(frame) = rx.try_recv() {
        let index = i16::from_le_bytes([frame[2], frame[3]]);
        packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
    }
    packets
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn map_root() -> PathBuf {
    workspace_root().join("resource/map")
}

fn npc_by_name(world: &World, name: &str) -> u32 {
    match world.npcs().iter().find(|npc| npc.name == name) {
        Some(npc) => npc.object_id,
        None => panic!(
            "应刷出 NPC {name}；现有 {}",
            world
                .npcs()
                .iter()
                .map(|npc| npc.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn bag_item(world: &World, player: u32, item_index: i32) -> Option<UserItem> {
    world
        .player(player)?
        .inventory
        .iter()
        .flatten()
        .find(|item| item.item_index == item_index)
        .cloned()
}

fn direction_between(from: Point, to: Point) -> MirDirection {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    match (dx.signum(), dy.signum()) {
        (0, 1) => MirDirection::Down,
        (0, -1) => MirDirection::Up,
        (1, 0) => MirDirection::Right,
        (-1, 0) => MirDirection::Left,
        (1, 1) => MirDirection::DownRight,
        (-1, 1) => MirDirection::DownLeft,
        (1, -1) => MirDirection::UpRight,
        (-1, -1) => MirDirection::UpLeft,
        _ => MirDirection::Down,
    }
}

fn walkable(world: &World, map: &str, point: Point) -> bool {
    world
        .map_of(map)
        .is_some_and(|map| map.walkable(point.x, point.y))
}

fn find_walkable_near(world: &World, map: &str, center: Point) -> Point {
    for radius in 0..20 {
        for dx in -radius..=radius {
            for dy in -radius..=radius {
                let point = Point::new(center.x + dx, center.y + dy);
                if point != center && walkable(world, map, point) {
                    return point;
                }
            }
        }
    }
    panic!("{center:?} 附近没有可走格");
}

/// BFS 只走地形可走格；不碰玩家属性/位置（调用方用 `walk` 真正走）。
fn path_directions(world: &World, map: &str, from: Point, to: Point) -> Vec<MirDirection> {
    if from == to {
        return Vec::new();
    }
    let mut queue = VecDeque::from([from]);
    let mut parent: HashMap<(i32, i32), ((i32, i32), MirDirection)> = HashMap::new();
    let mut seen = HashMap::from([((from.x, from.y), true)]);
    // NPC 是静态占位；不把它们的格子放进路径（玩家走到别的地图时路径重算）。
    let blocked: std::collections::HashSet<(i32, i32)> = world
        .npcs()
        .iter()
        .filter(|npc| npc.map == map)
        .map(|npc| (npc.position.x, npc.position.y))
        .chain(
            world
                .monsters()
                .iter()
                .filter(|monster| monster.map == map && !monster.dead)
                .map(|monster| (monster.position.x, monster.position.y)),
        )
        .collect();
    while let Some(current) = queue.pop_front() {
        for direction in [
            MirDirection::Up,
            MirDirection::UpRight,
            MirDirection::Right,
            MirDirection::DownRight,
            MirDirection::Down,
            MirDirection::DownLeft,
            MirDirection::Left,
            MirDirection::UpLeft,
        ] {
            let (dx, dy) = match direction {
                MirDirection::Up => (0, -1),
                MirDirection::UpRight => (1, -1),
                MirDirection::Right => (1, 0),
                MirDirection::DownRight => (1, 1),
                MirDirection::Down => (0, 1),
                MirDirection::DownLeft => (-1, 1),
                MirDirection::Left => (-1, 0),
                MirDirection::UpLeft => (-1, -1),
            };
            let next = Point::new(current.x + dx, current.y + dy);
            if seen.insert((next.x, next.y), true).is_none()
                && walkable(world, map, next)
                && !blocked.contains(&(next.x, next.y))
            {
                parent.insert((next.x, next.y), ((current.x, current.y), direction));
                if next == to {
                    let mut path = Vec::new();
                    let mut cursor = (to.x, to.y);
                    while let Some((previous, step)) = parent.get(&cursor).copied() {
                        path.push(step);
                        cursor = previous;
                        if cursor == (from.x, from.y) {
                            break;
                        }
                    }
                    path.reverse();
                    return path;
                }
                queue.push_back(next);
            }
        }
    }
    Vec::new()
}

/// 真实 combat：不放大任何属性；用 `walk_to` 走到怪物相邻格再攻击。
fn try_walk_to_on(world: &mut World, player: u32, map: &str, target: Point) -> bool {
    for attempt in 0..40 {
        if world.player(player).expect("角色").position == target {
            return true;
        }
        let path = path_directions(
            world,
            map,
            world.player(player).expect("角色").position,
            target,
        );
        if path.is_empty() {
            return false;
        }
        let mut moved = false;
        for direction in path {
            world.advance_clock(600);
            world.tick();
            match world.walk(player, direction) {
                Ok(_) => moved = true,
                Err(_) => break,
            }
            if world.player(player).expect("角色").position == target {
                return true;
            }
        }
        if !moved {
            world.advance_clock(600);
            world.tick();
            if attempt >= 39 {
                return false;
            }
        }
    }
    false
}

fn walk_to_on(world: &mut World, player: u32, map: &str, target: Point) {
    assert!(
        try_walk_to_on(world, player, map, target),
        "走不到 {target:?}"
    );
}

fn walk_to(world: &mut World, player: u32, target: Point) {
    walk_to_on(world, player, MAP, target);
}

fn adjacent_walkable(world: &World, target: Point) -> Point {
    for direction in [
        MirDirection::Up,
        MirDirection::UpRight,
        MirDirection::Right,
        MirDirection::DownRight,
        MirDirection::Down,
        MirDirection::DownLeft,
        MirDirection::Left,
        MirDirection::UpLeft,
    ] {
        let (dx, dy) = match direction {
            MirDirection::Up => (0, -1),
            MirDirection::UpRight => (1, -1),
            MirDirection::Right => (1, 0),
            MirDirection::DownRight => (1, 1),
            MirDirection::Down => (0, 1),
            MirDirection::DownLeft => (-1, 1),
            MirDirection::Left => (-1, 0),
            MirDirection::UpLeft => (-1, -1),
        };
        let point = Point::new(target.x + dx, target.y + dy);
        if !walkable(world, MAP, point) {
            continue;
        }
        let occupied = world
            .npcs()
            .iter()
            .any(|npc| npc.map == MAP && npc.position == point)
            || world
                .monsters()
                .iter()
                .any(|monster| monster.map == MAP && !monster.dead && monster.position == point);
        if !occupied {
            return point;
        }
    }
    panic!("{target:?} 四周没有可站的空格");
}

fn kill_monster_natural(world: &mut World, player: u32, monster: u32) -> u32 {
    let mut attacks = 0;
    loop {
        if world.monster(monster).is_none_or(|monster| monster.dead) {
            return attacks;
        }
        let target = world.monster(monster).expect("怪物还在").position;
        let from = adjacent_walkable(world, target);
        if world.player(player).expect("角色").position != from
            && !try_walk_to_on(world, player, MAP, from)
        {
            world.advance_clock(600);
            world.tick();
            continue;
        }
        let player_position = world.player(player).expect("角色").position;
        let direction = direction_between(player_position, target);
        if let Some(p) = world.player_mut(player) {
            p.direction = direction;
        }
        world.advance_clock(600);
        world.tick();
        if world.attack(player, direction).is_ok() {
            attacks += 1;
        }
        world.advance_clock(600);
        world.tick();
        assert!(attacks < 400, "400 次攻击仍未杀死怪物");
    }
}

fn kill_until_dead(world: &mut World, player: u32, monster: u32) {
    for _ in 0..400 {
        if world.player(player).is_none_or(|player| player.dead) {
            return;
        }
        world.advance_clock(500);
        world.tick();
        assert!(
            !world.monster(monster).is_none_or(|m| m.dead),
            "怪物提前死亡，不能用于自然死亡"
        );
    }
    panic!("玩家在 200 秒内没有被怪物打死");
}

#[tokio::test]
async fn journey_natural() {
    let Some(data) = common::default_data() else {
        eprintln!("skip: cjlaaa Server.MirDB not available under resource/");
        return;
    };
    let store = Arc::new(MemoryStore::new());
    let account = store
        .create_account("journeynatural", "secret", "")
        .await
        .expect("建账号");
    let mut created = store
        .create_character(
            account.id,
            "自然旅程",
            MirClass::Taoist,
            MirGender::Male,
            MAP,
            (284, 607),
        )
        .await
        .expect("建角色");

    let mut initial_world = World::new(0x42);
    initial_world.seed_rng(0x00C0_FFEE);
    initial_world.set_game_data(Arc::clone(&data));
    initial_world.set_languages(Arc::new(LanguageLibrary::embedded()));
    initial_world.set_map_root(map_root());
    assert!(initial_world.load_map(MAP).is_some(), "真实 map0 必须存在");
    assert!(
        initial_world.load_map(BOOK_SHOP_MAP).is_some(),
        "真实 0104 必须存在"
    );
    initial_world.set_enforce_bag_weight(false);

    // 角色的真实绑定回城点来自 DB 行；先写行再交给 Session 登录装载。
    let bind_index = data.map_info(MAP).expect("map0").index;
    created.bind_map_index = bind_index;
    created.bind_x = 284;
    created.bind_y = 606;
    store
        .save_character(created.clone())
        .await
        .expect("写入真实 bind 行");

    let world = Arc::new(Mutex::new(initial_world));
    let (out_tx, mut out_rx) = mpsc::unbounded_channel();
    let config = Arc::new(ServerConfig {
        start_map: MAP.to_owned(),
        map_root: map_root().display().to_string(),
        ..ServerConfig::default()
    });
    let mut session = Session::new(
        11,
        "journey-natural".to_owned(),
        Arc::clone(&config),
        Arc::clone(&store),
        Arc::clone(&world),
        out_tx,
        Arc::new(LanguageLibrary::embedded()),
    );
    session
        .handle(mir_protocol::ClientPacket::ClientVersion(
            c::ClientVersion {
                version_hash: Vec::new(),
            },
        ))
        .await
        .expect("ClientVersion");
    session
        .handle(mir_protocol::ClientPacket::Login(c::Login {
            account_id: "journeynatural".to_owned(),
            password: "secret".to_owned(),
        }))
        .await
        .expect("Login");
    session
        .handle(mir_protocol::ClientPacket::StartGame(c::StartGame {
            character_index: i32::try_from(created.id).expect("char id"),
        }))
        .await
        .expect("StartGame");
    let mut world_guard = world.lock().await;
    let world = &mut *world_guard;
    let player = world
        .player_object_id_by_name("自然旅程")
        .expect("Session 应已进世界");
    let _ = take_packets(&mut out_rx);
    let starters: Vec<UserItem> = world
        .player(player)
        .expect("角色")
        .inventory
        .iter()
        .flatten()
        .cloned()
        .collect();
    assert_eq!(starters.len(), STARTER_COUNT, "出生装 5 件");
    let weapon = starters
        .iter()
        .find(|item| {
            world
                .item_info(item.item_index)
                .is_some_and(|info| info.item_type == ItemType::Weapon)
        })
        .cloned()
        .expect("Taoist 出生武器");
    let armour = starters
        .iter()
        .find(|item| {
            world
                .item_info(item.item_index)
                .is_some_and(|info| info.item_type == ItemType::Armour)
        })
        .cloned()
        .expect("Taoist 出生盔甲");
    let weapon_uid = weapon.unique_id;
    world
        .equip_item(player, weapon_uid, MirGridType::Inventory, 0)
        .expect("穿武器");
    world
        .equip_item(player, armour.unique_id, MirGridType::Inventory, 1)
        .expect("穿盔甲");
    let equipped = world.player(player).expect("角色");
    assert_eq!(
        equipped.equipment[0].as_ref().map(|i| i.unique_id),
        Some(weapon_uid)
    );
    assert_eq!(
        equipped.equipment[1].as_ref().map(|i| i.unique_id),
        Some(armour.unique_id)
    );
    let equip_packets = take_packets(&mut out_rx);
    assert_eq!(
        equip_packets
            .iter()
            .filter(|packet| matches!(packet, ServerPacket::EquipItem(equip) if equip.success))
            .count(),
        2
    );

    // 2) 接 Jane 的真实 quest1。
    let jane = npc_by_name(world, "Assistant_Jane");
    world.accept_quest(player, jane, 1).expect("接 quest1");
    assert!(world.quest_progress(player, 1).is_some(), "任务列表非空");
    let change = take_packets(&mut out_rx)
        .into_iter()
        .find_map(|packet| match packet {
            ServerPacket::ChangeQuest(change) if change.quest.id == 1 => Some(change),
            _ => None,
        })
        .expect("接任务 S.ChangeQuest");
    let target_line = change
        .quest
        .task_list
        .iter()
        .find(|line| line.contains("前往"))
        .expect("目标行");
    assert!(target_line.contains("比奇省"));
    assert!(target_line.contains("工艺夫人"));
    assert!(target_line.contains("(294,619)"));

    // 3) 走真实门：map0 (322,247) → 门格 (323,247) → 0104。
    walk_to(world, player, Point::new(322, 247));
    world.advance_clock(600);
    world.tick();
    world.walk(player, MirDirection::Right).expect("踩门格");
    world.tick();
    assert_eq!(
        world.player(player).expect("角色").map,
        BOOK_SHOP_MAP,
        "真实传送应进 0104"
    );
    assert_eq!(
        world.player(player).expect("角色").position,
        Point::new(4, 10),
        "0104 入口坐标"
    );

    // 4) 0104 真实 Steven，不搬 NPC；1 级买 973 + 1016。
    let steven = npc_by_name(world, "Librarian_Steven");
    walk_to_on(world, player, BOOK_SHOP_MAP, Point::new(5, 9));
    world
        .call_npc(player, steven, "@main")
        .expect("Steven 主页");
    world
        .call_npc(player, steven, "@BuySell")
        .expect("书店买卖页");
    let book_npc = world.npc(steven).expect("Steven").clone();
    let goods = world.npc_goods(&book_npc);
    let healing_good = goods
        .iter()
        .find(|item| item.item_index == HEALING_BOOK)
        .cloned()
        .expect("0104 应卖治愈术");
    let bought_healing = world
        .buy_item(player, healing_good.unique_id, 1, PanelType::Buy)
        .expect("买治愈术");
    assert_eq!(bought_healing.item.item_index, HEALING_BOOK);
    assert_eq!(bought_healing.cost, 0, "1 级首本免费");
    assert_eq!(world.player(player).expect("角色").gold, 0, "买书前金币 0");
    let attack_good = goods
        .iter()
        .find(|item| item.item_index == ATTACK_BOOK)
        .cloned()
        .expect("0104 应卖攻杀剑术");
    let bought_attack = world
        .buy_item(player, attack_good.unique_id, 1, PanelType::Buy)
        .expect("买 973");
    assert_eq!(bought_attack.cost, 0, "首本免费仍适用");
    assert!(bag_item(world, player, ATTACK_BOOK).is_some());
    assert!(bag_item(world, player, HEALING_BOOK).is_some());
    // 7) 在 1 级读书（治愈术要求 7 级；1 级首本走新手免费/可学路径）。
    let _ = take_packets(&mut out_rx);
    let learned = world
        .use_item(
            player,
            bag_item(world, player, HEALING_BOOK)
                .expect("治疗书")
                .unique_id,
        )
        .expect("读治疗书");
    assert_eq!(
        learned.learned.as_ref().map(|m| m.spell),
        Some(Spell::Healing)
    );
    assert!(take_packets(&mut out_rx).iter().any(
        |packet| matches!(packet, ServerPacket::NewMagic(m) if m.magic.spell == Spell::Healing)
    ));

    // 5) 走真实门回 map0：0104 (4,11) → map0 (323,248)。
    world.advance_clock(2_000);
    world.tick();
    walk_to_on(world, player, BOOK_SHOP_MAP, Point::new(4, 10));
    world.advance_clock(2_000);
    world.tick();
    world.walk(player, MirDirection::Down).expect("踩回城格");
    world.tick();
    let back = world.player(player).expect("角色");
    assert_eq!(
        back.map,
        MAP,
        "position={:?} pending={:?}",
        back.position,
        world.pending_map_moves()
    );
    assert_eq!(back.position, Point::new(323, 248));

    // 6) 自然战斗 + 自然掉落：真实鸡、真实 1/5 表，最多 20 只；不造 850。
    let chicken_info = world.item_info(CHICKEN_ITEM).cloned().expect("鸡肉模板");
    assert_eq!(chicken_info.name, "鸡肉");
    let hen_info = data.monster_info_by_name("鸡").expect("真实鸡模板");
    let hen_kind = data.monster(hen_info.index).expect("鸡运行期").clone();
    let combat_center = Point::new(330, 600);
    let weapon_before = world.player(player).expect("角色").equipment[0]
        .as_ref()
        .expect("武器")
        .current_dura;
    let mut killed = 0;
    let mut total_attacks = 0;
    let mut chicken_ground = None;
    for attempt in 0..20 {
        let spawn = find_walkable_near(world, MAP, combat_center);
        let hen = world.spawn_monster(MAP, hen_kind.clone(), spawn);
        total_attacks += kill_monster_natural(world, player, hen);
        killed += 1;
        world.advance_clock(300);
        world.tick();
        if let Some((position, _)) = world
            .ground_items()
            .iter()
            .find(|ground| {
                ground
                    .item
                    .as_ref()
                    .is_some_and(|item| item.item_index == CHICKEN_ITEM)
            })
            .map(|ground| (ground.position, ground.object_id))
        {
            chicken_ground = Some(position);
            break;
        }
        assert!(attempt < 19, "20 只鸡都没有掉鸡肉");
    }
    let chicken_ground = chicken_ground.expect("真实掉落鸡肉");
    let weapon_after_combat = world.player(player).expect("角色").equipment[0]
        .as_ref()
        .expect("武器")
        .current_dura;
    let durability_delta = weapon_before - weapon_after_combat;
    eprintln!(
        "T132 combat: durability {weapon_before} -> {weapon_after_combat}, delta={durability_delta}, attacks={total_attacks}"
    );
    assert!(
        durability_delta > 0,
        "自然命中应磨损武器：{weapon_before} -> {weapon_after_combat}"
    );
    assert!(
        u32::from(durability_delta) <= 4 * total_attacks.max(1),
        "累计磨损 {durability_delta} 超过 {total_attacks} 次命中 * 1..=4"
    );
    // 自然走到鸡肉格再拾取。
    walk_to(world, player, chicken_ground);
    let picked = world.pickup(player).expect("拾取鸡肉");
    assert_eq!(
        picked.item.as_ref().map(|item| item.item_index),
        Some(CHICKEN_ITEM),
        "拾取结果必须是真实鸡肉"
    );
    let chicken_uid = picked.item.as_ref().expect("鸡肉").unique_id;
    assert!(bag_item(world, player, CHICKEN_ITEM).is_some());

    // 7) 自然走到 Sarah，进真实 @Sell 页卖肉：0 → price/2（自然耐久 60..79 → 30..39）。
    let sarah = npc_by_name(world, "Merchant_Sarah");
    let sarah_position = world.npc(sarah).expect("Sarah").position;
    walk_to(
        world,
        player,
        Point::new(sarah_position.x, sarah_position.y + 1),
    );
    assert_eq!(world.player(player).expect("角色").gold, 0);
    world.call_npc(player, sarah, "@main").expect("Sarah 主页");
    world.call_npc(player, sarah, "@Sell").expect("Sarah 卖页");
    let chicken = world
        .player(player)
        .expect("角色")
        .inventory
        .iter()
        .flatten()
        .find(|item| item.item_index == CHICKEN_ITEM)
        .cloned()
        .expect("背包鸡肉");
    let price = item_price(&chicken_info, &chicken);
    // 自然掉落会随机当前耐久，鸡肉价格 60..=79；卖出价 = price/2（最高 39）。
    // 40 只能来自 create_fresh_item 的满耐久（本测试禁止手动造 850）。
    assert!((60..=79).contains(&price), "natural chicken price={price}");
    let _ = take_packets(&mut out_rx);
    let sold = world.sell_item(player, chicken_uid, 1).expect("卖鸡肉");
    eprintln!(
        "T132 sale: price={price} gold={} killed={killed}",
        sold.gold
    );
    assert_eq!(sold.gold, price / 2);
    assert_eq!(
        world.player(player).expect("角色").gold,
        i64::from(sold.gold)
    );
    let sell_packets = take_packets(&mut out_rx);
    assert!(sell_packets.iter().any(|packet| matches!(
        packet,
        ServerPacket::GainedGold(gold) if gold.gold == sold.gold
    )));
    assert!(sell_packets.iter().any(|packet| matches!(packet, ServerPacket::SellItem(sell) if sell.success && sell.unique_id == chicken_uid)));

    // 8) 走到真实 Judy，自然交 quest1。
    let judy = npc_by_name(world, "CraftsLady_Jude");
    let judy_position = world.npc(judy).expect("Judy").position;
    walk_to(
        world,
        player,
        Point::new(judy_position.x, judy_position.y - 1),
    );
    assert_eq!(world.player(player).expect("角色").map, MAP);
    let finished = world.finish_quest(player, 1, -1).expect("自然交 quest1");
    assert!(finished.completed);
    assert!(
        world
            .player(player)
            .expect("角色")
            .completed_quests
            .contains(&1)
    );
    assert!(
        world
            .quest_progress(player, 1)
            .is_none_or(|p| p.state != mir_server::quests::QuestProgressState::InProgress)
    );

    // 9) 施放 Healing（真实 MP/包）；书已在 0104 的 1 级阶段读过。
    assert!(
        world
            .player(player)
            .expect("角色")
            .magics
            .iter()
            .any(|m| m.spell == Spell::Healing)
    );
    let mp_before = world.player(player).expect("角色").mp;
    // 不手改 HP/MP/冷却；自然等完行走/攻击的动作冷却。
    for _ in 0..20 {
        if world.player(player).expect("角色").mp >= 3 {
            break;
        }
        world.advance_clock(1_000);
        world.tick();
    }
    world.advance_clock(2_000);
    world.tick();
    let _ = take_packets(&mut out_rx);
    let cast = world
        .cast_magic(
            player,
            Spell::Healing,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("自然施放 Healing");
    let mp_after = world.player(player).expect("角色").mp;
    eprintln!(
        "T132 heal: mp {mp_before} -> {mp_after}, cost={}",
        cast.cost
    );
    assert_eq!(cast.cost, 3);
    assert_eq!(mp_after, mp_before - 3);
    assert!(
        take_packets(&mut out_rx)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Magic(_)))
    );

    // 10) 真实 Smith 修理：cost 必须 2；金币扣 2（自然卖肉通常不是 40，故不硬写 38），耐久回满。
    let empty_slot = world
        .player(player)
        .expect("角色")
        .inventory
        .iter()
        .position(Option::is_none)
        .expect("背包空格");
    world
        .move_item(player, MirGridType::Equipment, 0, empty_slot as i32)
        .expect("卸武器");
    let smith = npc_by_name(world, "Merchant_Smith");
    let smith_position = world.npc(smith).expect("Smith").position;
    let smith_adjacent = adjacent_walkable(world, smith_position);
    walk_to(world, player, smith_adjacent);
    world.call_npc(player, smith, "@main").expect("Smith 主页");
    world
        .call_npc(player, smith, "@Repair")
        .expect("Smith 修理页");
    let gold_before = world.player(player).expect("角色").gold;
    let repaired = world.repair_item(player, weapon_uid).expect("修理");
    eprintln!(
        "T132 repair: durability -> {}/{}, cost={repaired_cost}, gold {gold_before} -> {}",
        repaired.current_dura,
        repaired.max_dura,
        world.player(player).expect("角色").gold,
        repaired_cost = repaired.cost
    );
    assert_eq!(repaired.cost, 2, "真实修理价必须为 2");
    assert_eq!(
        world.player(player).expect("角色").gold,
        gold_before - 2,
        "修理硬扣 2"
    );
    assert_eq!(repaired.current_dura, repaired.max_dura);
    world
        .equip_item(player, weapon_uid, MirGridType::Inventory, 0)
        .expect("再穿武器");

    // 10b) 修好后再战一次：真实命中应再次磨损，且攻击/伤害仍生效。
    let repaired_dura = world.player(player).expect("角色").equipment[0]
        .as_ref()
        .expect("武器")
        .current_dura;
    let target_info = data.monster_info_by_name("鸡").expect("真实鸡模板");
    let target_kind = data.monster(target_info.index).expect("鸡运行期").clone();
    let player_position = world.player(player).expect("角色").position;
    let target_cell = adjacent_walkable(world, player_position);
    let target = world.spawn_monster(MAP, target_kind, target_cell);
    let target_position = world.monster(target).expect("鸡").position;
    let direction = direction_between(player_position, target_position);
    if let Some(p) = world.player_mut(player) {
        p.direction = direction;
    }
    world.advance_clock(2_000);
    world.tick();
    let report = world.attack(player, direction).expect("修好后攻击");
    assert!(report.is_some(), "修好后攻击应有命中结算");
    world.advance_clock(600);
    world.tick();
    let after_attack = world.player(player).expect("角色").equipment[0]
        .as_ref()
        .expect("武器")
        .current_dura;
    assert!(
        after_attack < repaired_dura,
        "修好后命中也必须磨损：{repaired_dura} -> {after_attack}"
    );
    assert!(
        (1..=4).contains(&(repaired_dura - after_attack)),
        "单次命中磨损应 1..=4：{repaired_dura} -> {after_attack}"
    );
    eprintln!(
        "T132 post-repair attack: durability {repaired_dura} -> {after_attack}, damage={:?}",
        report.map(|r| r.damage)
    );
    // 耐久=0 的武器不得贡献任何 DC/AC（既有单测 items.rs:3440 的旅程内复核）。
    let weapon_info = world.item_info(weapon.item_index).expect("武器模板");
    let mut broken_weapon = weapon.clone();
    broken_weapon.current_dura = 0;
    let mut broken_stats = CombatStats::default();
    apply_equipment_stats(&mut broken_stats, weapon_info, &broken_weapon);
    assert_eq!(
        broken_stats,
        CombatStats::default(),
        "耐久 0 武器不得给基础伤害加成"
    );
    eprintln!("T132 zero-durability weapon: no DC/AC contribution");

    // 11) 自然死亡：真实狼攻击到 hp=0；12s 不自动复活；TownRevive。
    let wolf_info = data.monster_info_by_name("狼").expect("真实狼模板");
    let wolf_kind = data.monster(wolf_info.index).expect("狼运行期").clone();
    let player_position = world.player(player).expect("角色").position;
    let wolf_cell = adjacent_walkable(world, player_position);
    let wolf = world.spawn_monster(MAP, wolf_kind, wolf_cell);
    kill_until_dead(world, player, wolf);
    assert!(world.player(player).expect("角色").dead, "必须被狼自然打死");
    let _ = take_packets(&mut out_rx);
    world.advance_clock(12_000);
    world.tick();
    assert!(world.player(player).expect("角色").dead, "12s 不能自动复活");
    assert!(
        !take_packets(&mut out_rx)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Revived(_)))
    );
    assert!(world.town_revive_player(player, MAP, Point::new(10, 10)));
    let revived = world.player(player).expect("角色");
    assert!(!revived.dead);
    assert_eq!(revived.hp, revived.stats.hp);
    assert_eq!(revived.mp, revived.stats.mp);
    assert_eq!(
        revived.position,
        Point::new(created.bind_x, created.bind_y),
        "必须回角色真实 bind，不是 fallback"
    );
    assert!(
        take_packets(&mut out_rx)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Revived(_)))
    );

    // 12) 生产下线：Session::leave_world 的保存路径（不是手工 snapshot/save）。
    let base = Character::new(
        created.id,
        account.id,
        "自然旅程",
        MirClass::Taoist,
        MirGender::Male,
        MAP,
        (284, 606),
    );
    let before =
        market::character_snapshot(&base, world.player(player).expect("角色"), PetMode::Both);
    let expected_inventory = world.player(player).expect("角色").inventory.clone();
    let expected_equipment = world.player(player).expect("角色").equipment.clone();
    let expected_gold = world.player(player).expect("角色").gold;
    let expected_magics = world.player(player).expect("角色").magics.clone();
    let expected_completed = world.player(player).expect("角色").completed_quests.clone();
    let expected_bind_map = world.player(player).expect("角色").bind_map_index;
    let expected_bind_point = world.player(player).expect("角色").bind_point;
    drop(world_guard);
    session.leave_world().await;
    let loaded = store
        .find_character(account.id, created.id)
        .await
        .expect("读档")
        .expect("角色行");
    assert_eq!(loaded.inventory, before.inventory);
    assert_eq!(loaded.equipment, before.equipment);
    assert_eq!(loaded.gold, before.gold);
    assert_eq!(loaded.magics, before.magics);
    assert_eq!(loaded.quests, before.quests);
    assert_eq!(loaded.bind_map_index, before.bind_map_index);
    assert_eq!(
        (loaded.bind_x, loaded.bind_y),
        (before.bind_x, before.bind_y)
    );

    let new_world = Arc::new(Mutex::new({
        let mut world = World::new(0x43);
        world.set_game_data(Arc::clone(&data));
        world.set_languages(Arc::new(LanguageLibrary::embedded()));
        world.set_map_root(map_root());
        world.load_map(MAP).expect("新 World 加载 map0");
        world.load_map(BOOK_SHOP_MAP).expect("新 World 加载 0104");
        world
    }));
    let (out, _out_rx) = mpsc::unbounded_channel();
    let config = Arc::new(ServerConfig {
        start_map: MAP.to_owned(),
        map_root: map_root().display().to_string(),
        ..ServerConfig::default()
    });
    let mut session = Session::new(
        7,
        "test".to_owned(),
        Arc::clone(&config),
        Arc::clone(&store),
        Arc::clone(&new_world),
        out,
        Arc::new(LanguageLibrary::embedded()),
    );
    session
        .handle(mir_protocol::ClientPacket::ClientVersion(
            c::ClientVersion {
                version_hash: Vec::new(),
            },
        ))
        .await
        .expect("ClientVersion");
    session
        .handle(mir_protocol::ClientPacket::Login(c::Login {
            account_id: "journeynatural".to_owned(),
            password: "secret".to_owned(),
        }))
        .await
        .expect("Login");
    session
        .handle(mir_protocol::ClientPacket::StartGame(c::StartGame {
            character_index: i32::try_from(created.id).expect("char id"),
        }))
        .await
        .expect("StartGame");
    let (reloaded_id, reloaded) = {
        let world = new_world.lock().await;
        let id = world
            .player_object_id_by_name("自然旅程")
            .expect("重登玩家");
        let player = world.player(id).expect("重登玩家").clone();
        (id, player)
    };
    assert!(reloaded_id > 0);
    assert_eq!(reloaded.gold, expected_gold);
    assert_eq!(
        reloaded.equipment, expected_equipment,
        "重登后 equipment 的 uid/index/耐久必须全等"
    );
    assert_eq!(
        reloaded.inventory, expected_inventory,
        "重登后 inventory 的 uid/index/耐久必须全等"
    );
    assert_eq!(reloaded.magics, expected_magics, "技能必须全等");
    assert_eq!(
        reloaded.completed_quests, expected_completed,
        "完成列表必须全等"
    );
    assert_eq!(reloaded.bind_map_index, expected_bind_map);
    assert_eq!(reloaded.bind_point, expected_bind_point);
    assert!(reloaded.magics.iter().any(|m| m.spell == Spell::Healing));
    assert!(reloaded.completed_quests.contains(&1));
    assert_eq!(reloaded.class, MirClass::Taoist);
    session.leave_world().await;
    let _ = session;
    let _ = killed;
}
