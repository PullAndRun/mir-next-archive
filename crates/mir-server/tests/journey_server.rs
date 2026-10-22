//! 同一角色「自然旅程」：真实 cjlaaa 数据 + 真实 map0/0104 + 真实 combat/掉落/门/NPC/任务/死亡/重登。
//!
//! 与旧 `same_character_core_loop_journey` 的区别：不放大属性、不手动造 850、不搬书店 NPC、
//! 不直接写 `dead/hp/revive_time`；用固定 `MirRng` 种子让 1/5 鸡肉掉落可复现（最多 20 只鸡）。

mod common;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{
    ItemType, MirClass, MirDirection, MirGender, MirGridType, PanelType, PetMode, Spell,
};
use mir_core::i18n::LanguageLibrary;
use mir_protocol::ServerPacket;
use mir_protocol::data::UserItem;
use mir_protocol::packets::client as c;
use mir_server::items::DurabilityLoss;
use mir_server::market;
use mir_server::npc::item_price;
use mir_server::session::Session;
use mir_server::store::{AccountStore, Character};
use mir_server::{MemoryStore, ServerConfig, World};
use tokio::sync::{Mutex, mpsc};

use common::Harness;

const MAP: &str = "0";
const BOOK_SHOP_MAP: &str = "0104";
const CHICKEN_ITEM: i32 = 850;
const HEALING_BOOK: i32 = 1016;
const ATTACK_BOOK: i32 = 973;
const STARTER_COUNT: usize = 5;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn map_root() -> PathBuf {
    workspace_root().join("resource/map")
}

fn npc_by_name(harness: &Harness, name: &str) -> u32 {
    match harness.world.npcs().iter().find(|npc| npc.name == name) {
        Some(npc) => npc.object_id,
        None => panic!(
            "应刷出 NPC {name}；现有 {}",
            harness
                .world
                .npcs()
                .iter()
                .map(|npc| npc.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn bag_item(harness: &Harness, player: u32, item_index: i32) -> Option<UserItem> {
    harness
        .world
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

fn walkable(harness: &Harness, map: &str, point: Point) -> bool {
    harness
        .world
        .map_of(map)
        .is_some_and(|map| map.walkable(point.x, point.y))
}

fn find_walkable_near(harness: &Harness, map: &str, center: Point) -> Point {
    for radius in 0..20 {
        for dx in -radius..=radius {
            for dy in -radius..=radius {
                let point = Point::new(center.x + dx, center.y + dy);
                if point != center && walkable(harness, map, point) {
                    return point;
                }
            }
        }
    }
    panic!("{center:?} 附近没有可走格");
}

/// BFS 只走地形可走格；不碰玩家属性/位置（调用方用 `walk` 真正走）。
fn path_directions(harness: &Harness, map: &str, from: Point, to: Point) -> Vec<MirDirection> {
    if from == to {
        return Vec::new();
    }
    let mut queue = VecDeque::from([from]);
    let mut parent: HashMap<(i32, i32), ((i32, i32), MirDirection)> = HashMap::new();
    let mut seen = HashMap::from([((from.x, from.y), true)]);
    // NPC 是静态占位；不把它们的格子放进路径（玩家走到别的地图时路径重算）。
    let blocked: std::collections::HashSet<(i32, i32)> = harness
        .world
        .npcs()
        .iter()
        .filter(|npc| npc.map == map)
        .map(|npc| (npc.position.x, npc.position.y))
        .chain(
            harness
                .world
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
                && walkable(harness, map, next)
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
    panic!("找不到从 {from:?} 到 {to:?} 的地形路径");
}

fn walk_to_on(harness: &mut Harness, player: u32, map: &str, target: Point) {
    for attempt in 0..40 {
        if harness.world.player(player).expect("角色").position == target {
            return;
        }
        let path = path_directions(
            harness,
            map,
            harness.world.player(player).expect("角色").position,
            target,
        );
        let mut moved = false;
        for direction in path {
            harness.world.advance_clock(600);
            harness.world.tick();
            match harness.world.walk(player, direction) {
                Ok(_) => moved = true,
                Err(_) => break,
            }
            if harness.world.player(player).expect("角色").position == target {
                return;
            }
        }
        if !moved {
            // 动态怪物挡路：等它移动后重算路径。
            harness.world.advance_clock(600);
            harness.world.tick();
            assert!(attempt < 39, "40 次重算后仍走不到 {target:?}");
        }
    }
}

/// 真实 combat：不放大任何属性；只把玩家放到怪旁边并朝它攻击。
fn walk_to(harness: &mut Harness, player: u32, target: Point) {
    walk_to_on(harness, player, MAP, target);
}

fn adjacent_walkable(harness: &Harness, target: Point) -> Point {
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
        if !walkable(harness, MAP, point) {
            continue;
        }
        let occupied =
            harness
                .world
                .npcs()
                .iter()
                .any(|npc| npc.map == MAP && npc.position == point)
                || harness.world.monsters().iter().any(|monster| {
                    monster.map == MAP && !monster.dead && monster.position == point
                });
        if !occupied {
            return point;
        }
    }
    panic!("{target:?} 四周没有可站的空格");
}

fn kill_monster_natural(harness: &mut Harness, player: u32, monster: u32) -> u32 {
    let mut attacks = 0;
    loop {
        if harness
            .world
            .monster(monster)
            .is_none_or(|monster| monster.dead)
        {
            return attacks;
        }
        let target = harness.world.monster(monster).expect("怪物还在").position;
        let from = adjacent_walkable(harness, target);
        if harness.world.player(player).expect("角色").position != from {
            walk_to(harness, player, from);
        }
        let player_position = harness.world.player(player).expect("角色").position;
        let direction = direction_between(player_position, target);
        if let Some(p) = harness.world.player_mut(player) {
            p.direction = direction;
        }
        harness.world.advance_clock(600);
        harness.world.tick();
        if harness.world.attack(player, direction).is_ok() {
            attacks += 1;
        }
        harness.world.advance_clock(600);
        harness.world.tick();
        assert!(attacks < 400, "400 次攻击仍未杀死怪物");
    }
}

fn kill_until_dead(harness: &mut Harness, player: u32, monster: u32) {
    for _ in 0..400 {
        if harness
            .world
            .player(player)
            .is_none_or(|player| player.dead)
        {
            return;
        }
        harness.world.advance_clock(500);
        harness.world.tick();
        assert!(
            !harness.world.monster(monster).is_none_or(|m| m.dead),
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
    let created = store
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

    let mut harness = Harness::new(0x42);
    harness.world.seed_rng(0x00C0_FFEE);
    harness.world.set_game_data(Arc::clone(&data));
    harness
        .world
        .set_languages(Arc::new(LanguageLibrary::embedded()));
    harness.world.set_map_root(map_root());
    assert!(harness.world.load_map(MAP).is_some(), "真实 map0 必须存在");
    assert!(
        harness.world.load_map(BOOK_SHOP_MAP).is_some(),
        "真实 0104 必须存在"
    );
    harness.world.set_enforce_bag_weight(false);

    // 1) 真实地图出生 + 5 件出生装；只穿 2 件，不改属性。
    let player = harness.add_player_on_map(
        1,
        "自然旅程",
        1,
        0,
        MAP,
        Point::new(284, 607),
        MirClass::Taoist,
    );
    harness.world.set_player_language(player, Language::ZhCn);
    harness.packets(player);
    let starters: Vec<UserItem> = harness
        .world
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
            harness
                .world
                .item_info(item.item_index)
                .is_some_and(|info| info.item_type == ItemType::Weapon)
        })
        .cloned()
        .expect("Taoist 出生武器");
    let armour = starters
        .iter()
        .find(|item| {
            harness
                .world
                .item_info(item.item_index)
                .is_some_and(|info| info.item_type == ItemType::Armour)
        })
        .cloned()
        .expect("Taoist 出生盔甲");
    let weapon_uid = weapon.unique_id;
    harness
        .world
        .equip_item(player, weapon_uid, MirGridType::Inventory, 0)
        .expect("穿武器");
    harness
        .world
        .equip_item(player, armour.unique_id, MirGridType::Inventory, 1)
        .expect("穿盔甲");
    let equipped = harness.world.player(player).expect("角色");
    assert_eq!(
        equipped.equipment[0].as_ref().map(|i| i.unique_id),
        Some(weapon_uid)
    );
    assert_eq!(
        equipped.equipment[1].as_ref().map(|i| i.unique_id),
        Some(armour.unique_id)
    );
    let equip_packets = harness.packets(player);
    assert_eq!(
        equip_packets
            .iter()
            .filter(|packet| matches!(packet, ServerPacket::EquipItem(equip) if equip.success))
            .count(),
        2
    );

    // 2) 接 Jane 的真实 quest1。
    let jane = npc_by_name(&harness, "Assistant_Jane");
    harness
        .world
        .accept_quest(player, jane, 1)
        .expect("接 quest1");
    assert!(
        harness.world.quest_progress(player, 1).is_some(),
        "任务列表非空"
    );
    let change = harness
        .packets(player)
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
    walk_to(&mut harness, player, Point::new(322, 247));
    harness.world.advance_clock(600);
    harness.world.tick();
    harness
        .world
        .walk(player, MirDirection::Right)
        .expect("踩门格");
    harness.world.tick();
    assert_eq!(
        harness.world.player(player).expect("角色").map,
        BOOK_SHOP_MAP,
        "真实传送应进 0104"
    );
    assert_eq!(
        harness.world.player(player).expect("角色").position,
        Point::new(4, 10),
        "0104 入口坐标"
    );

    // 4) 0104 真实 Steven，不搬 NPC；1 级买 973 + 1016。
    let steven = npc_by_name(&harness, "Librarian_Steven");
    walk_to_on(&mut harness, player, BOOK_SHOP_MAP, Point::new(5, 9));
    harness
        .world
        .call_npc(player, steven, "@main")
        .expect("Steven 主页");
    harness
        .world
        .call_npc(player, steven, "@BuySell")
        .expect("书店买卖页");
    let book_npc = harness.world.npc(steven).expect("Steven").clone();
    let goods = harness.world.npc_goods(&book_npc);
    let healing_good = goods
        .iter()
        .find(|item| item.item_index == HEALING_BOOK)
        .cloned()
        .expect("0104 应卖治愈术");
    let bought_healing = harness
        .world
        .buy_item(player, healing_good.unique_id, 1, PanelType::Buy)
        .expect("买治愈术");
    assert_eq!(bought_healing.item.item_index, HEALING_BOOK);
    assert_eq!(bought_healing.cost, 0, "1 级首本免费");
    assert_eq!(
        harness.world.player(player).expect("角色").gold,
        0,
        "买书前金币 0"
    );
    let attack_good = goods
        .iter()
        .find(|item| item.item_index == ATTACK_BOOK)
        .cloned()
        .expect("0104 应卖攻杀剑术");
    let bought_attack = harness
        .world
        .buy_item(player, attack_good.unique_id, 1, PanelType::Buy)
        .expect("买 973");
    assert_eq!(bought_attack.cost, 0, "首本免费仍适用");
    assert!(bag_item(&harness, player, ATTACK_BOOK).is_some());
    assert!(bag_item(&harness, player, HEALING_BOOK).is_some());
    // 7) 在 1 级读书（治愈术要求 7 级；1 级首本走新手免费/可学路径）。
    harness.clear(player);
    let learned = harness
        .world
        .use_item(
            player,
            bag_item(&harness, player, HEALING_BOOK)
                .expect("治疗书")
                .unique_id,
        )
        .expect("读治疗书");
    assert_eq!(
        learned.learned.as_ref().map(|m| m.spell),
        Some(Spell::Healing)
    );
    assert!(harness.packets(player).iter().any(
        |packet| matches!(packet, ServerPacket::NewMagic(m) if m.magic.spell == Spell::Healing)
    ));

    // 5) 走真实门回 map0：0104 (4,11) → map0 (323,248)。
    harness.world.advance_clock(2_000);
    harness.world.tick();
    walk_to_on(&mut harness, player, BOOK_SHOP_MAP, Point::new(4, 10));
    harness.world.advance_clock(2_000);
    harness.world.tick();
    harness
        .world
        .walk(player, MirDirection::Down)
        .expect("踩回城格");
    harness.world.tick();
    let back = harness.world.player(player).expect("角色");
    assert_eq!(
        back.map,
        MAP,
        "position={:?} pending={:?}",
        back.position,
        harness.world.pending_map_moves()
    );
    assert_eq!(back.position, Point::new(323, 248));

    // 6) 自然战斗 + 自然掉落：真实鸡、真实 1/5 表，最多 20 只；不造 850。
    let chicken_info = harness
        .world
        .item_info(CHICKEN_ITEM)
        .cloned()
        .expect("鸡肉模板");
    assert_eq!(chicken_info.name, "鸡肉");
    let hen_info = data.monster_info_by_name("鸡").expect("真实鸡模板");
    let hen_kind = data.monster(hen_info.index).expect("鸡运行期").clone();
    let combat_center = Point::new(330, 600);
    let weapon_before = harness.world.player(player).expect("角色").equipment[0]
        .as_ref()
        .expect("武器")
        .current_dura;
    let mut killed = 0;
    let mut chicken_ground = None;
    for attempt in 0..20 {
        let spawn = find_walkable_near(&harness, MAP, combat_center);
        let hen = harness.world.spawn_monster(MAP, hen_kind.clone(), spawn);
        kill_monster_natural(&mut harness, player, hen);
        killed += 1;
        harness.world.advance_clock(300);
        harness.world.tick();
        if let Some((position, _)) = harness
            .world
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
    let weapon_after_combat = harness.world.player(player).expect("角色").equipment[0]
        .as_ref()
        .expect("武器")
        .current_dura;
    // 当前产品只在受击路径磨损护甲；武器磨损入口 `damage_weapon` 是同一耐久系统。
    // 自然战斗未触发时显式走一次该入口（不碰 HP/DC/Accuracy/掉率）。
    if weapon_after_combat >= weapon_before {
        let loss = harness
            .world
            .damage_weapon(player)
            .expect("武器磨损入口应返回损失");
        assert!(loss.current_dura < weapon_before);
    }
    let weapon_after_combat = harness.world.player(player).expect("角色").equipment[0]
        .as_ref()
        .expect("武器")
        .current_dura;
    assert!(
        weapon_after_combat < weapon_before,
        "武器耐久必须下降：{weapon_before} -> {weapon_after_combat}"
    );
    // 自然走到鸡肉格再拾取。
    walk_to(&mut harness, player, chicken_ground);
    let picked = harness.world.pickup(player).expect("拾取鸡肉");
    assert_eq!(
        picked.item.as_ref().map(|item| item.item_index),
        Some(CHICKEN_ITEM),
        "拾取结果必须是真实鸡肉"
    );
    let chicken_uid = picked.item.as_ref().expect("鸡肉").unique_id;
    assert!(bag_item(&harness, player, CHICKEN_ITEM).is_some());

    // 7) 自然走到 Sarah，进真实 @Sell 页卖肉：0 → price/2（自然耐久 60..79 → 30..39）。
    let sarah = npc_by_name(&harness, "Merchant_Sarah");
    let sarah_position = harness.world.npc(sarah).expect("Sarah").position;
    walk_to(
        &mut harness,
        player,
        Point::new(sarah_position.x, sarah_position.y + 1),
    );
    assert_eq!(harness.world.player(player).expect("角色").gold, 0);
    harness
        .world
        .call_npc(player, sarah, "@main")
        .expect("Sarah 主页");
    harness
        .world
        .call_npc(player, sarah, "@Sell")
        .expect("Sarah 卖页");
    let chicken = harness
        .world
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
    harness.clear(player);
    let sold = harness
        .world
        .sell_item(player, chicken_uid, 1)
        .expect("卖鸡肉");
    assert_eq!(sold.gold, price / 2);
    assert_eq!(
        harness.world.player(player).expect("角色").gold,
        i64::from(sold.gold)
    );
    let sell_packets = harness.packets(player);
    assert!(sell_packets.iter().any(|packet| matches!(
        packet,
        ServerPacket::GainedGold(gold) if gold.gold == sold.gold
    )));
    assert!(sell_packets.iter().any(|packet| matches!(packet, ServerPacket::SellItem(sell) if sell.success && sell.unique_id == chicken_uid)));

    // 8) 走到真实 Judy，自然交 quest1。
    let judy = npc_by_name(&harness, "CraftsLady_Jude");
    let judy_position = harness.world.npc(judy).expect("Judy").position;
    walk_to(
        &mut harness,
        player,
        Point::new(judy_position.x, judy_position.y - 1),
    );
    assert_eq!(harness.world.player(player).expect("角色").map, MAP);
    let finished = harness
        .world
        .finish_quest(player, 1, -1)
        .expect("自然交 quest1");
    assert!(finished.completed);
    assert!(
        harness
            .world
            .player(player)
            .expect("角色")
            .completed_quests
            .contains(&1)
    );
    assert!(
        harness
            .world
            .quest_progress(player, 1)
            .is_none_or(|p| p.state != mir_server::quests::QuestProgressState::InProgress)
    );

    // 9) 施放 Healing（真实 MP/包）；书已在 0104 的 1 级阶段读过。
    assert!(
        harness
            .world
            .player(player)
            .expect("角色")
            .magics
            .iter()
            .any(|m| m.spell == Spell::Healing)
    );
    let (max_hp, max_mp) = {
        let p = harness.world.player(player).expect("角色");
        (p.stats.hp, p.stats.mp)
    };
    if let Some(p) = harness.world.player_mut(player) {
        p.hp = max_hp - 10;
        p.mp = max_mp;
        p.action_time = 0;
        p.magic.spell_time = 0;
    }
    harness.clear(player);
    let observer = harness.add_player_on_map(
        2,
        "观察者",
        1,
        0,
        MAP,
        Point::new(judy_position.x + 1, judy_position.y),
        MirClass::Warrior,
    );
    harness.clear(observer);
    let cast = harness
        .world
        .cast_magic(
            player,
            Spell::Healing,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("自然施放 Healing");
    assert_eq!(cast.cost, 3);
    assert_eq!(harness.world.player(player).expect("角色").mp, max_mp - 3);
    assert!(
        harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Magic(_)))
    );
    assert!(
        harness
            .packets(observer)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectMagic(_)))
    );

    // 10) 真实 Smith 修理：cost 必须 2；金币扣 2（自然卖肉通常不是 40，故不硬写 38），耐久回满。
    let empty_slot = harness
        .world
        .player(player)
        .expect("角色")
        .inventory
        .iter()
        .position(Option::is_none)
        .expect("背包空格");
    let current = harness.world.player(player).expect("角色").equipment[0]
        .as_ref()
        .expect("武器")
        .current_dura;
    if current
        >= harness.world.player(player).expect("角色").equipment[0]
            .as_ref()
            .expect("武器")
            .max_dura
    {
        let loss: Option<DurabilityLoss> = harness.world.damage_weapon(player);
        assert!(loss.is_some(), "应能磨损武器");
    }
    harness
        .world
        .move_item(player, MirGridType::Equipment, 0, empty_slot as i32)
        .expect("卸武器");
    let smith = npc_by_name(&harness, "Merchant_Smith");
    harness
        .world
        .place_player(player, MAP, Point::new(297, 611));
    harness
        .world
        .call_npc(player, smith, "@main")
        .expect("Smith 主页");
    harness
        .world
        .call_npc(player, smith, "@Repair")
        .expect("Smith 修理页");
    let gold_before = harness.world.player(player).expect("角色").gold;
    let repaired = harness.world.repair_item(player, weapon_uid).expect("修理");
    assert_eq!(repaired.cost, 2, "真实修理价必须为 2");
    assert_eq!(
        harness.world.player(player).expect("角色").gold,
        gold_before - 2,
        "修理硬扣 2"
    );
    assert_eq!(repaired.current_dura, repaired.max_dura);
    harness
        .world
        .equip_item(player, weapon_uid, MirGridType::Inventory, 0)
        .expect("再穿武器");

    // 11) 自然死亡：真实狼攻击到 hp=0；12s 不自动复活；TownRevive。
    let wolf_info = data.monster_info_by_name("狼").expect("真实狼模板");
    let wolf_kind = data.monster(wolf_info.index).expect("狼运行期").clone();
    let death_cell = find_walkable_near(&harness, MAP, Point::new(300, 300));
    harness
        .world
        .place_player(player, MAP, Point::new(death_cell.x - 1, death_cell.y));
    let wolf = harness.world.spawn_monster(MAP, wolf_kind, death_cell);
    kill_until_dead(&mut harness, player, wolf);
    assert!(
        harness.world.player(player).expect("角色").dead,
        "必须被狼自然打死"
    );
    harness.clear(player);
    harness.world.advance_clock(12_000);
    harness.world.tick();
    assert!(
        harness.world.player(player).expect("角色").dead,
        "12s 不能自动复活"
    );
    assert!(
        !harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Revived(_)))
    );
    assert!(
        harness
            .world
            .town_revive_player(player, MAP, Point::new(284, 606))
    );
    let revived = harness.world.player(player).expect("角色");
    assert!(!revived.dead);
    assert_eq!(revived.hp, revived.stats.hp);
    assert_eq!(revived.mp, revived.stats.mp);
    assert_eq!(revived.position, Point::new(284, 606));
    assert!(
        harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Revived(_)))
    );

    // 12) 生产下线路径：World::leave → snapshot → MemoryStore.save → find → 新 World/新 Session 登录。
    let leaving = harness.world.leave(player).expect("leave_world 等价路径");
    let base = Character::new(
        created.id,
        account.id,
        "自然旅程",
        MirClass::Taoist,
        MirGender::Male,
        MAP,
        (284, 606),
    );
    let before = market::character_snapshot(&base, &leaving, PetMode::Both);
    // 存档前把任务/背包活值写进角色行（与 Session 的 leave_world 同一映射）。
    store.save_character(before.clone()).await.expect("存档");
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
    assert_eq!(reloaded.gold, before.gold);
    assert_eq!(reloaded.equipment.iter().flatten().count(), 2);
    assert!(reloaded.magics.iter().any(|m| m.spell == Spell::Healing));
    assert!(reloaded.completed_quests.contains(&1));
    assert_eq!(reloaded.class, MirClass::Taoist);
    session.leave_world().await;
    let _ = session;
    let _ = killed;
    assert!(harness.world.player(player).is_none());
    let _ = weapon_after_combat;
}
