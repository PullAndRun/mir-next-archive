//! 同一角色核心旅程（服务端确定性版）。
//!
//! 用真实 cjlaaa 数据 + `World` API，不走 TCP/走位/随机掉率：
//! 出生装 → 穿两件 → 接 quest1 → 杀鸡/取肉 → 卖肉 40 → 买书/学治愈术 → 施法
//! → 武器掉耐久 → Smith 修理 → 死亡/12s 不自动复活/回城复活 → 存档往返。
//!
//! 数据事实：cjlaaa 的 `ItemInfo.Index=850` 是 Chicken/「鸡肉」（price=80，卖 40）；
//! 任务文本里的 item 850 与真实数据一致。测试同时记录 849 是「时间之石」，
//! 防止把数组下标与 `ItemInfo.Index` 混淆。

mod common;

use std::sync::Arc;

use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{
    ItemType, MirClass, MirDirection, MirGender, MirGridType, PanelType, PetMode, Spell,
};
use mir_core::i18n::LanguageLibrary;
use mir_protocol::ServerPacket;
use mir_protocol::data::UserItem;
use mir_server::MemoryStore;
use mir_server::items;
use mir_server::items::DurabilityLoss;
use mir_server::magic;
use mir_server::market;
use mir_server::npc::item_price;
use mir_server::quests;
use mir_server::storage;
use mir_server::store::{AccountStore, Character};

use common::Harness;

const MAP: &str = "0";
/// cjlaaa 的 Chicken/鸡肉（`ItemInfo.Index=850`，price=80 → 卖 40）。
const CHICKEN_ITEM: i32 = 850;
const HEALING_BOOK: i32 = 1016;
/// 任务书点名的 973（攻杀剑术），一并验证 0104 货架与背包。
const ATTACK_BOOK: i32 = 973;
const STARTER_COUNT: usize = 5;
const REAL_MAP_SIZE: i32 = 700;

fn blank_map(name: &str, size: i32) -> Arc<mir_world::Map> {
    let cells = usize::try_from(size * size).expect("地图尺寸");
    mir_world::Map {
        width: size,
        height: size,
        cells: vec![mir_world::MapCell::default(); cells],
        file_name: name.to_owned(),
        version: mir_world::MapVersion::V0,
        doors: Vec::new(),
    }
    .into()
}

fn npc_by_name(harness: &Harness, name: &str) -> u32 {
    match harness.world.npcs().iter().find(|npc| npc.name == name) {
        Some(npc) => npc.object_id,
        None => panic!(
            "map0 应刷出 NPC {name}；现有 {}",
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

fn place_near(harness: &mut Harness, player: u32, target: Point) {
    assert!(
        harness.world.place_player(player, MAP, target),
        "应能把玩家放到 {target:?}"
    );
}

fn kill_hen(harness: &mut Harness, player: u32, monster: u32) {
    let (target, map) = {
        let monster = harness.world.monster(monster).expect("鸡还在");
        (monster.position, monster.map.clone())
    };
    place_near(harness, player, Point::new(target.x - 1, target.y));
    if let Some(p) = harness.world.player_mut(player) {
        p.direction = MirDirection::Right;
        p.hp = 1_000_000;
        p.stats.hp = 1_000_000;
        p.stats.min_dc = 10_000;
        p.stats.max_dc = 10_000;
        p.stats.accuracy = 10_000;
    }
    for _ in 0..8 {
        if harness
            .world
            .monster(monster)
            .is_none_or(|monster| monster.dead)
        {
            break;
        }
        if let Some(p) = harness.world.player_mut(player) {
            p.action_time = 0;
            p.attack_time = 0;
            p.direction = MirDirection::Right;
        }
        let _ = harness.world.attack(player, MirDirection::Right);
        harness.world.advance_clock(300);
        harness.world.tick();
    }
    assert!(
        harness
            .world
            .monster(monster)
            .is_none_or(|monster| monster.dead),
        "8 刀内应击杀鸡"
    );
    let _ = map;
}

#[tokio::test]
async fn same_character_core_loop_journey() {
    let Some(data) = common::default_data() else {
        eprintln!("skip: cjlaaa Server.MirDB not available under resource/");
        return;
    };
    let mut harness = Harness::new(0x42);
    harness.world.seed_rng(0x42);
    harness.world.set_game_data(Arc::clone(&data));
    harness
        .world
        .set_languages(Arc::new(LanguageLibrary::embedded()));
    harness.world.set_enforce_bag_weight(false);
    harness.world.insert_map(MAP, blank_map(MAP, REAL_MAP_SIZE));

    // 1) 建角色：Taoist 为了第 8 步能施放 Healing；进入世界后领 5 件出生装。
    let player = harness.add_player_on_map(
        1,
        "旅程角色",
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
        .expect("角色已进世界")
        .inventory
        .iter()
        .flatten()
        .cloned()
        .collect();
    assert_eq!(
        starters.len(),
        STARTER_COUNT,
        "出生装应为 5 件，实际 {:?}",
        starters
            .iter()
            .map(|item| (item.item_index, item.unique_id))
            .collect::<Vec<_>>()
    );

    let weapon = starters
        .iter()
        .find(|item| {
            harness
                .world
                .item_info(item.item_index)
                .is_some_and(|info| info.item_type == ItemType::Weapon)
        })
        .cloned()
        .expect("Taoist 出生装应含武器");
    let armour = starters
        .iter()
        .find(|item| {
            harness
                .world
                .item_info(item.item_index)
                .is_some_and(|info| info.item_type == ItemType::Armour)
        })
        .cloned()
        .expect("Taoist 出生装应含盔甲");
    let weapon_uid = weapon.unique_id;
    let armour_uid = armour.unique_id;
    harness
        .world
        .equip_item(player, weapon_uid, MirGridType::Inventory, 0)
        .expect("穿武器");
    harness
        .world
        .equip_item(player, armour_uid, MirGridType::Inventory, 1)
        .expect("穿盔甲");
    let equipped = harness.world.player(player).expect("角色");
    assert_eq!(
        equipped.equipment[0].as_ref().map(|item| item.unique_id),
        Some(weapon_uid),
        "武器应在 EquipmentSlot.Weapon=0"
    );
    assert_eq!(
        equipped.equipment[1].as_ref().map(|item| item.unique_id),
        Some(armour_uid),
        "盔甲应在 EquipmentSlot.Armour=1"
    );
    let packets = harness.packets(player);
    assert_eq!(
        packets
            .iter()
            .filter(|packet| matches!(packet, ServerPacket::EquipItem(equip) if equip.success))
            .count(),
        2,
        "应收到两条成功的 S.EquipItem: {packets:?}"
    );

    // 2) 接 quest1：目标行由真实交付 NPC Jude 推导。
    let jane = npc_by_name(&harness, "Assistant_Jane");
    harness
        .world
        .accept_quest(player, jane, 1)
        .expect("接 quest1");
    let progress = harness
        .world
        .quest_progress(player, 1)
        .expect("quest1 进度");
    assert_eq!(progress.quest_index, 1);
    let report_packets = harness.packets(player);
    let change = report_packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::ChangeQuest(change) if change.quest.id == 1 => Some(change),
            _ => None,
        })
        .expect("接任务应发 S.ChangeQuest");
    assert!(
        !change.quest.task_list.is_empty(),
        "任务追踪不能为空: {change:?}"
    );
    let target_line = change
        .quest
        .task_list
        .iter()
        .find(|line| line.contains("前往"))
        .unwrap_or_else(|| panic!("任务追踪应含目标行: {:?}", change.quest.task_list));
    assert!(
        target_line.contains("比奇省"),
        "目标行应包含地图名: {target_line}"
    );
    assert!(
        target_line.contains("工艺夫人"),
        "目标行应包含交付 NPC: {target_line}"
    );
    assert!(
        target_line.contains("(294,619)"),
        "目标行应包含真实 NPC 坐标: {target_line}"
    );
    let info = harness
        .world
        .client_quest_info(1, Language::ZhCn)
        .expect("quest1 定义");
    assert!(
        info.task_description.iter().any(|line| line == target_line),
        "S.NewQuestInfo 的 task_description 也应带目标行: {:?}",
        info.task_description
    );

    // 3) 刷一只鸡并击杀。真实掉落表有 1/5 鸡肉；若随机没中，再用 create_fresh_item
    //    生成同一模板的地面物，保证后续证据确定（仍走 pickup / 背包 / 商店）。
    let hen_info = data.monster_info_by_name("鸡").expect("cjlaaa 鸡模板");
    let hen_kind = data.monster(hen_info.index).expect("鸡运行期模板").clone();
    let hen = harness
        .world
        .spawn_monster(MAP, hen_kind, Point::new(320, 600));
    harness.clear(player);
    kill_hen(&mut harness, player, hen);
    assert!(
        harness
            .world
            .monster(hen)
            .is_none_or(|monster| monster.dead),
        "鸡必须死亡"
    );
    harness.world.advance_clock(300);
    harness.world.tick();
    let dropped = harness
        .world
        .ground_items()
        .iter()
        .filter_map(|ground| ground.item.as_ref())
        .find(|item| item.item_index == CHICKEN_ITEM)
        .cloned();
    let chicken = if let Some(item) = dropped {
        item
    } else {
        let info = harness
            .world
            .item_info(CHICKEN_ITEM)
            .cloned()
            .expect("鸡肉模板");
        let item = harness.world.create_fresh_item(&info);
        let position = harness.world.player(player).expect("角色").position;
        harness
            .world
            .spawn_ground_item(MAP, position, Some(item.clone()), 0, None);
        item
    };
    // 数据事实：ItemInfo.Index=850 是鸡肉（price=80 → 卖 40）；849 是时间之石。
    assert_eq!(CHICKEN_ITEM, 850);
    assert_eq!(
        harness
            .world
            .item_info(CHICKEN_ITEM)
            .map(|info| info.name.as_str()),
        Some("鸡肉"),
        "cjlaaa 的 Chicken 模板名是鸡肉"
    );
    assert_eq!(
        harness.world.item_info(849).map(|info| info.name.as_str()),
        Some("时间之石"),
        "849 是时间之石，不能拿数组下标当 Index"
    );

    // 4) 站在鸡肉上拾取。
    let chicken_ground = if let Some(ground) = harness.world.ground_items().iter().find(|ground| {
        ground
            .item
            .as_ref()
            .is_some_and(|item| item.unique_id == chicken.unique_id)
    }) {
        ground.position
    } else {
        let p = harness.world.player(player).expect("角色").position;
        harness
            .world
            .spawn_ground_item(MAP, p, Some(chicken.clone()), 0, None);
        p
    };
    place_near(&mut harness, player, chicken_ground);
    let picked = harness.world.pickup(player).expect("拾取鸡肉");
    assert_eq!(
        picked.item.as_ref().map(|item| item.item_index),
        Some(CHICKEN_ITEM),
        "拾取结果应是鸡肉: {picked:?}"
    );
    let chicken_uid = picked.item.as_ref().expect("鸡肉").unique_id;
    assert!(bag_item(&harness, player, CHICKEN_ITEM).is_some());

    // 5) 传送到 Sarah，进 @Sell 页，卖鸡肉：0 → 40。
    let sarah = npc_by_name(&harness, "Merchant_Sarah");
    place_near(&mut harness, player, Point::new(313, 272));
    harness.clear(player);
    harness
        .world
        .call_npc(player, sarah, "@main")
        .expect("打开 Sarah 主页");
    harness
        .world
        .call_npc(player, sarah, "@Sell")
        .expect("打开卖肉页");
    let chicken_info = harness
        .world
        .item_info(CHICKEN_ITEM)
        .cloned()
        .expect("鸡肉模板");
    assert_eq!(item_price(&chicken_info, &chicken), 80);
    let sold = harness
        .world
        .sell_item(player, chicken_uid, 1)
        .expect("卖出鸡肉");
    assert_eq!(sold.gold, 40, "鸡肉 80 → 卖 40");
    assert_eq!(harness.world.player(player).expect("角色").gold, 40);
    let sold_packets = harness.packets(player);
    assert!(sold_packets.iter().any(|packet| matches!(
        packet,
        ServerPacket::GainedGold(gained) if gained.gold == 40
    )));
    assert!(sold_packets.iter().any(|packet| matches!(
        packet,
        ServerPacket::SellItem(sell) if sell.success && sell.unique_id == chicken_uid
    )));

    // 6) 用真实 0104 书店 NPC 的脚本买第一本治疗书（1 级首本免费）。
    let mut book_info = data
        .db
        .npcs
        .iter()
        .find(|npc| npc.file_name.ends_with("BookStore-0104"))
        .cloned()
        .expect("cjlaaa 0104 书店 NPC");
    book_info.map_index = 1;
    book_info.location = Point::new(284, 605);
    let book_npc_id = harness
        .world
        .spawn_npc(MAP, &book_info)
        .expect("刷出 0104 书店");
    place_near(&mut harness, player, Point::new(284, 606));
    harness
        .world
        .call_npc(player, book_npc_id, "@main")
        .expect("书店主页");
    harness
        .world
        .call_npc(player, book_npc_id, "@BuySell")
        .expect("书店买卖页");
    let book_npc = harness.world.npc(book_npc_id).expect("书店").clone();
    let goods = harness.world.npc_goods(&book_npc);
    let book_good = goods
        .iter()
        .find(|item| item.item_index == HEALING_BOOK)
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "0104 书店应卖治疗书，货架 {:?}",
                goods.iter().map(|g| g.item_index).collect::<Vec<_>>()
            )
        });
    let bought = harness
        .world
        .buy_item(player, book_good.unique_id, 1, PanelType::Buy)
        .expect("买治疗书");
    assert_eq!(bought.item.item_index, HEALING_BOOK);
    assert_eq!(bought.cost, 0, "1 级、无技能首本书免费");
    assert!(bag_item(&harness, player, HEALING_BOOK).is_some());

    // 任务书点名 973（攻杀剑术）：真实 0104 货架也有；同样 1 级首本免费，
    // 但 Taoist 不学/不施放它，保留在包里作为「技能书/973」证据。
    let attack_good = goods
        .iter()
        .find(|item| item.item_index == ATTACK_BOOK)
        .cloned()
        .expect("0104 书店应卖 973 攻杀剑术");
    let bought_attack = harness
        .world
        .buy_item(player, attack_good.unique_id, 1, PanelType::Buy)
        .expect("买 973 攻杀剑术");
    assert_eq!(bought_attack.item.item_index, ATTACK_BOOK);
    assert_eq!(bought_attack.cost, 0, "首本书免费仍适用");
    assert!(
        bag_item(&harness, player, ATTACK_BOOK).is_some(),
        "背包应出现 item 973"
    );

    // 7) 读书：S.NewMagic + 技能列表。
    harness.clear(player);
    let learned = harness
        .world
        .use_item(player, bought.item.unique_id)
        .expect("使用治疗书");
    assert_eq!(
        learned.learned.as_ref().map(|magic| magic.spell),
        Some(Spell::Healing)
    );
    let magic_packets = harness.packets(player);
    assert!(
        magic_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::NewMagic(magic) if magic.magic.spell == Spell::Healing)),
        "应收到 S.NewMagic Healing: {magic_packets:?}"
    );
    assert!(
        harness
            .world
            .player(player)
            .expect("角色")
            .magics
            .iter()
            .any(|magic| magic.spell == Spell::Healing),
        "技能列表应含 Healing"
    );

    // 8) 施放 Healing：MP 下降 + S.Magic/S.ObjectMagic。
    let (max_hp, max_mp) = {
        let p = harness.world.player(player).expect("角色");
        (p.stats.hp, p.stats.mp)
    };
    if let Some(p) = harness.world.player_mut(player) {
        p.hp = max_hp - 50;
        p.mp = max_mp;
        p.action_time = 0;
        p.magic.spell_time = 0;
    }
    harness.clear(player);
    // `S.ObjectMagic` 是广播包（本人收 `S.Magic`）；放一个同图观察者验证广播。
    let observer = harness.add_player_on_map(
        2,
        "施法观察者",
        1,
        0,
        MAP,
        Point::new(285, 606),
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
        .expect("施放 Healing");
    assert_eq!(cast.cost, 3, "0 级 Healing BaseCost=3");
    assert_eq!(
        harness.world.player(player).expect("角色").mp,
        max_mp - cast.cost
    );
    let cast_packets = harness.packets(player);
    assert!(
        cast_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Magic(_))),
        "应发 S.Magic: {cast_packets:?}"
    );
    let observer_packets = harness.packets(observer);
    assert!(
        observer_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectMagic(_))),
        "观察者应收到 S.ObjectMagic: {observer_packets:?}"
    );

    // 9) 武器耐久下降（攻击/受击的同一磨损入口）。
    let weapon_before = harness.world.player(player).expect("角色").equipment[0]
        .as_ref()
        .expect("武器还穿着")
        .current_dura;
    let mut last_loss: Option<DurabilityLoss> = None;
    for _ in 0..40 {
        if let Some(loss) = harness.world.damage_weapon(player) {
            last_loss = Some(loss.clone());
            if loss.current_dura < weapon_before {
                break;
            }
        }
    }
    let loss = last_loss.expect("应发生武器耐久损失");
    assert!(loss.current_dura < weapon_before);
    let weapon_after_damage = harness.world.player(player).expect("角色").equipment[0]
        .as_ref()
        .expect("武器")
        .current_dura;
    assert_eq!(weapon_after_damage, loss.current_dura);

    // 10) 修 Smith：武器先放回背包（repair_item 只认背包），修满并扣金币。
    let empty_slot = harness
        .world
        .player(player)
        .expect("角色")
        .inventory
        .iter()
        .position(Option::is_none)
        .expect("背包应有空格");
    harness
        .world
        .move_item(player, MirGridType::Equipment, 0, empty_slot as i32)
        .expect("卸下武器到背包");
    let smith = npc_by_name(&harness, "Merchant_Smith");
    place_near(&mut harness, player, Point::new(297, 611));
    harness.clear(player);
    harness
        .world
        .call_npc(player, smith, "@main")
        .expect("Smith 主页");
    harness
        .world
        .call_npc(player, smith, "@Repair")
        .expect("Smith 修理页");
    let gold_before_repair = harness.world.player(player).expect("角色").gold;
    let repaired = harness
        .world
        .repair_item(player, weapon_uid)
        .expect("修理武器");
    assert_eq!(repaired.current_dura, repaired.max_dura, "修满");
    assert!(repaired.max_dura <= weapon.max_dura);
    assert_eq!(
        harness.world.player(player).expect("角色").gold,
        gold_before_repair - i64::from(repaired.cost),
        "修理应扣金币"
    );
    let repair_packets = harness.packets(player);
    assert!(repair_packets.iter().any(|packet| matches!(
        packet,
        ServerPacket::ItemRepaired(item) if item.unique_id == weapon_uid && item.current_dura == repaired.max_dura
    )));
    harness
        .world
        .equip_item(player, weapon_uid, MirGridType::Inventory, 0)
        .expect("修好再穿武器");

    // 11) 死亡：12s 不自动复活；TownRevive 回绑定回城点并满血满蓝。
    let map_index = data.map_info(MAP).expect("map0").index;
    let now = harness.world.now();
    if let Some(p) = harness.world.player_mut(player) {
        p.bind_map_index = map_index;
        p.bind_point = Point::new(284, 606);
        p.hp = 0;
        p.dead = true;
        p.death_time = now;
        p.revive_time = now + 60_000;
    }
    harness.clear(player);
    harness.world.advance_clock(12_000);
    harness.world.tick();
    assert!(
        harness.world.player(player).expect("角色").dead,
        "12s 内不能自动复活（60s 才是兜底）"
    );
    assert!(
        !harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Revived(_))),
        "12s 内不应发 S.Revived"
    );
    assert!(
        harness
            .world
            .town_revive_player(player, MAP, Point::new(10, 10)),
        "TownRevive 应生效"
    );
    let revived = harness.world.player(player).expect("角色");
    assert!(!revived.dead);
    assert_eq!(revived.hp, revived.stats.hp);
    assert_eq!(revived.mp, revived.stats.mp);
    assert_eq!(revived.position, Point::new(284, 606), "应落在绑定点");
    let revive_packets = harness.packets(player);
    assert!(
        revive_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Revived(_)))
    );

    // 12) snapshot → MemoryStore save/load → 再 snapshot，逐项一致。
    let store = MemoryStore::new();
    let account = store
        .create_account("journey", "secret", "")
        .await
        .expect("建账号");
    let created = store
        .create_character(
            account.id,
            "旅程角色",
            MirClass::Taoist,
            MirGender::Male,
            MAP,
            (284, 606),
        )
        .await
        .expect("建角色");
    let base = Character::new(
        created.id,
        account.id,
        "旅程角色",
        MirClass::Taoist,
        MirGender::Male,
        MAP,
        (284, 606),
    );
    let before = market::character_snapshot(
        &base,
        harness.world.player(player).expect("角色"),
        PetMode::Both,
    );
    store.save_character(before.clone()).await.expect("存档");
    let loaded = store
        .find_character(account.id, created.id)
        .await
        .expect("读档")
        .expect("角色行存在");
    assert_eq!(loaded, before, "MemoryStore 应立即读回同一行");

    let mut loaded_player = harness.world.player(player).expect("角色").clone();
    loaded_player.inventory = items::decode_slots(&loaded.inventory).expect("inventory");
    loaded_player.equipment = items::decode_slots(&loaded.equipment).expect("equipment");
    loaded_player.storage = storage::decode_storage(&loaded.storage).expect("storage");
    loaded_player.magics = magic::decode_magics(&loaded.magics).expect("magics");
    let quest_save = quests::decode_quests(&loaded.quests).expect("quests");
    loaded_player.quests = quest_save.quests;
    loaded_player.completed_quests = quest_save.completed_quests;
    loaded_player.gold = loaded.gold;
    loaded_player.bind_map_index = loaded.bind_map_index;
    loaded_player.bind_point = Point::new(loaded.bind_x, loaded.bind_y);
    let after = market::character_snapshot(&loaded, &loaded_player, PetMode::Both);
    assert_eq!(
        after.equipment, before.equipment,
        "equipment uid/index/耐久必须一致"
    );
    assert_eq!(
        after.inventory, before.inventory,
        "inventory uid/index/耐久必须一致"
    );
    assert_eq!(after.gold, before.gold, "金币一致");
    assert_eq!(after.magics, before.magics, "技能一致");
    assert_eq!(after.quests, before.quests, "任务一致");
    assert_eq!(after.bind_map_index, before.bind_map_index);
    assert_eq!((after.bind_x, after.bind_y), (before.bind_x, before.bind_y));

    // 证据兜底：这条链确实包含 5 件出生装、2 件装备、卖肉 40、治疗技能、quest1。
    let final_player = harness.world.player(player).expect("角色");
    assert_eq!(starters.len(), STARTER_COUNT);
    assert_eq!(
        final_player.equipment.iter().flatten().count(),
        2,
        "最后仍穿 2 件（修好后重新穿上武器）"
    );
    assert_eq!(
        final_player.gold,
        40 - i64::from(repaired.cost),
        "卖肉得 40；修理再扣 {} 金币",
        repaired.cost
    );
    assert!(
        final_player
            .magics
            .iter()
            .any(|m| m.spell == Spell::Healing)
    );
    assert!(harness.world.quest_progress(player, 1).is_some());
}
