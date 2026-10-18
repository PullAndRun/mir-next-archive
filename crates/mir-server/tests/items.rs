//! 物品系统的端到端测试：模板查找、掉落落地、拾取/丢弃、背包交换、装备穿戴、持久化编解码。
//!
//! 全部确定性：世界随机数固定种子（[`World::seed_rng`]）、时间用 [`World::advance_clock`]
//! 快进（不 sleep）、掉落表写进临时目录（每个用例一个专属目录，互不干扰）。
//! 每个断言都尽量对应 Crystal 的一处行为，注释里写了出处。

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{
    ItemGrade, ItemType, MirClass, MirDirection, MirGender, MirGridType, Monster as MonsterImage,
    RequiredClass, RequiredType, Stat,
};
use mir_db::{MirDb, MonsterInfo};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ItemInfo, Stats, UserItem};
use mir_server::MonsterKind;
use mir_server::combat::{self, CombatStats};
use mir_server::gamedata::GameData;
use mir_server::items::{
    self, GROUND_ITEM_OWNER_MS, GroundItem, INVENTORY_SIZE, ITEM_EXPIRE_MS, ItemError,
    ItemUidGenerator,
};
use mir_server::world::{Player, World};
use tokio::sync::mpsc;

const MAP: &str = "0";

// ---------------------------------------------------------------------------
// 测试脚手架
// ---------------------------------------------------------------------------

struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Harness {
    fn new(seed: u64) -> Self {
        let mut world = World::new(999);
        world.seed_rng(seed);
        Self {
            world,
            receivers: HashMap::new(),
        }
    }

    fn add_player(&mut self, object_id: u32, position: Point) -> u32 {
        let stats = combat::player_base_stats(1, MirClass::Warrior);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.enter(Player {
            object_id,
            session_id: u64::from(object_id),
            character_id: i64::from(object_id),
            name: format!("玩家{object_id}"),
            map: MAP.to_owned(),
            position,
            direction: MirDirection::Down,
            level: 1,
            experience: 0,
            max_experience: 0,
            class: MirClass::Warrior,
            gender: MirGender::Male,
            hair: 1,
            hp: stats.hp,
            mp: stats.mp,
            stats,
            action_time: 0,
            attack_time: 0,
            struck_time: 0,
            regen_time: 0,
            death_time: 0,
            revive_time: 0,
            cell_time: 0,
            step_counter: 0,
            run_counter: 0,
            run_time: 0,
            dead: false,
            visible_monsters: BTreeSet::new(),
            visible_npcs: BTreeSet::new(),
            movement_time: 0,
            inventory: items::new_inventory(),
            equipment: items::new_equipment(),
            magics: Vec::new(),
            magic: mir_server::MagicState::default(),
            gold: 0,
            credit: 0,
            visible_items: BTreeSet::new(),
            quests: Vec::new(),
            completed_quests: BTreeSet::new(),
            buffs: Vec::new(),
            pot_health_amount: 0,
            pot_mana_amount: 0,
            pot_time: 0,
            unlock_curse: false,
            npc: mir_server::NpcState::default(),
            group_id: None,
            group_invitation: None,
            group_invite_time: 0,
            allow_group: true,
            guild: None,
            sender,
        });
        self.receivers.insert(object_id, receiver);
        object_id
    }

    fn packets(&mut self, object_id: u32) -> Vec<ServerPacket> {
        let receiver = self.receivers.get_mut(&object_id).expect("玩家通道");
        let mut packets = Vec::new();
        while let Ok(frame) = receiver.try_recv() {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
        }
        packets
    }

    fn has(&mut self, object_id: u32, matcher: impl Fn(&ServerPacket) -> bool) -> bool {
        self.packets(object_id).iter().any(matcher)
    }

    fn ready_player(&mut self, object_id: u32) {
        if let Some(player) = self.world.player_mut(object_id) {
            player.action_time = 0;
            player.attack_time = 0;
        }
    }

    /// 给玩家背包里塞一件**满耐久**的物品（走 `Envir.CreateFreshItem` 的等价入口）。
    fn give(&mut self, object_id: u32, info: &ItemInfo) -> UserItem {
        let item = self.world.create_fresh_item(info);
        assert!(
            self.world.add_item_to_inventory(object_id, item.clone()),
            "背包应该有空位"
        );
        item
    }
}

/// 物品模板。
fn item(
    index: i32,
    name: &str,
    item_type: ItemType,
    weight: u8,
    stats: &[(Stat, i32)],
) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        item_type,
        grade: ItemGrade::None,
        weight,
        image: u16::try_from(index).unwrap_or(0),
        durability: 5_000,
        stats: Stats::from_iter(stats.iter().copied()),
        ..ItemInfo::default()
    }
}

/// 一次测试用的完整数据：几件物品 + 一只会掉东西的怪。
///
/// 掉落文件写在临时目录的 `Envir/Drops/` 下，与 [`GameData::load`] 的目录推导规则一致。
fn game_data(tag: &str, drop_text: &str) -> Arc<GameData> {
    let drops = temp_drop_dir(tag);
    std::fs::write(drops.join("测试怪.txt"), drop_text).expect("写掉落表");
    let db = MirDb {
        items: vec![
            ItemInfo {
                stack_size: 20,
                ..item(1, "金创药(小量)", ItemType::Potion, 1, &[(Stat::HP, 30)])
            },
            item(
                2,
                "测试之剑",
                ItemType::Weapon,
                10,
                &[(Stat::MinDC, 1), (Stat::MaxDC, 5)],
            ),
            item(3, "重甲", ItemType::Armour, 60, &[(Stat::MaxAC, 3)]),
            ItemInfo {
                required_type: RequiredType::Level,
                required_amount: 10,
                ..item(4, "高级剑", ItemType::Weapon, 10, &[(Stat::MaxDC, 9)])
            },
            ItemInfo {
                required_class: RequiredClass::Wizard,
                ..item(5, "法师杖", ItemType::Weapon, 10, &[(Stat::MaxMC, 9)])
            },
        ],
        monsters: vec![MonsterInfo {
            index: 1,
            name: "测试怪".to_owned(),
            drop_path: String::new(),
            ..MonsterInfo::default()
        }],
        ..MirDb::default()
    };
    Arc::new(GameData::from_db(db, Some(drops)))
}

fn temp_drop_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mir-next-items-{tag}-{}", std::process::id()));
    let drops = dir.join("Envir").join("Drops");
    std::fs::create_dir_all(&drops).expect("建临时掉落目录");
    drops
}

/// 一只 1 血、不会动的测试怪（一碰就死）。
fn test_monster() -> MonsterKind {
    MonsterKind {
        name: "测试怪".to_owned(),
        image: MonsterImage::Oma,
        ai: 0,
        level: 1,
        view_range: 7,
        cool_eye: 0,
        attack_speed_ms: 2_500,
        move_speed_ms: 1_800,
        experience: 0,
        can_push: true,
        stats: CombatStats {
            hp: 1,
            accuracy: 100,
            ..CombatStats::default()
        },
    }
}

/// 把玩家挪到怪左边、一击必杀（伤害 300ms 后落地，与 Crystal 的延迟结算一致）。
fn kill_monster(harness: &mut Harness, player: u32, monster: u32) {
    let target = harness.world.monster(monster).expect("怪物还在").position;
    let from = Point::new(target.x - 1, target.y);
    if let Some(p) = harness.world.player_mut(player) {
        p.position = from;
        p.direction = MirDirection::Right;
        p.stats.min_dc = 100;
        p.stats.max_dc = 100;
        p.stats.accuracy = 100;
    }
    harness.ready_player(player);
    harness
        .world
        .attack(player, MirDirection::Right)
        .expect("攻击应被接受")
        .expect("前方有怪");
    harness.world.advance_clock(300);
    harness.world.tick();
    assert!(
        harness.world.monster(monster).is_none_or(|m| m.dead),
        "一击 100 点应打死 1 血测试怪"
    );
}

// ---------------------------------------------------------------------------
// 1. 模板查找（Crystal `Envir.GetItemInfo`）
// ---------------------------------------------------------------------------

#[test]
fn item_lookup_accepts_index_and_ignores_case_and_spaces_in_names() {
    let data = game_data("lookup", "1/1 Gold 10\n");
    // 按编号（`Envir.GetItemInfo(int)`，`Envir.cs:4734-4743`）。
    assert_eq!(
        data.item(2).map(|info| info.name.as_str()),
        Some("测试之剑")
    );
    assert!(data.item(999).is_none());
    assert_eq!(data.item_count(), 5);
    // 按名字（`Envir.GetItemInfo(string)`，`Envir.cs:4745-4754`）：忽略空格。
    assert!(data.item_by_name("金创药(小量)").is_some());
    assert!(data.item_by_name("金创药 (小量)").is_some());
    // 大小写不敏感（`StringComparison.OrdinalIgnoreCase`）。
    assert!(
        data.item_by_name("TEST-SWORD").is_none(),
        "中文名没有大小写"
    );
    assert!(data.item_by_name("测试之剑").is_some());
    assert!(data.item_by_name("不存在的物品").is_none());
}

// ---------------------------------------------------------------------------
// 2. 掉落（Crystal `MonsterObject.Drop` + `Envir.CreateDropItem`）
// ---------------------------------------------------------------------------

#[test]
fn monster_death_places_drop_items_and_gold_around_the_corpse() {
    let mut harness = Harness::new(1);
    let data = game_data("drop", ";金币\n1/1 Gold 100\n;药水\n1/1 金创药(小量)\n");
    harness.world.set_game_data(Arc::clone(&data));
    let player = harness.add_player(1, Point::new(20, 20));
    let monster = harness
        .world
        .spawn_monster(MAP, test_monster(), Point::new(22, 20));
    harness.packets(player);

    kill_monster(&mut harness, player, monster);

    // 掉落物 2 件：一堆金币 + 一件药水（`MonsterObject.cs:1104-1143`）。
    let ground: Vec<GroundItem> = harness.world.ground_items().to_vec();
    assert_eq!(ground.len(), 2, "应有金币与药水各一堆，实际 {ground:?}");
    let gold = ground.iter().find(|item| item.is_gold()).expect("金币堆");
    // `AttemptDrop` 的金币区间是 `[Gold/2, Gold + Gold/2)`，即 [50, 150)。
    assert!(
        (50..150).contains(&gold.gold),
        "金币数量应落在 [50,150)，实际 {}",
        gold.gold
    );
    let potion = ground.iter().find(|item| !item.is_gold()).expect("药水堆");
    let potion_item = potion.item.clone().expect("药水");
    assert_eq!(potion_item.item_index, 1);
    // 归属：`Owner = EXPOwner`、`OwnerTime = Envir.Time + Settings.Minute`
    // （`MonsterObject.cs:1152-1156`）。
    assert_eq!(gold.owner, Some(player));
    assert_eq!(potion.owner, Some(player));
    assert!(gold.owner_time > harness.world.now());
    // 落点在尸体周围 `Settings.DropRange = 4` 格内（`ItemObject.Drop`，`ItemObject.cs:198`）。
    for item in &ground {
        assert!(
            item.position.in_range(Point::new(22, 20), 4),
            "落点 {0} 超出 DropRange",
            item.position
        );
        assert_eq!(item.map, MAP);
    }
    // 掉落物立刻对视野内玩家可见（`ItemObject.Drop` → `Spawned()` → `Broadcast(GetInfo())`）。
    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::ObjectItem(_) | ServerPacket::ObjectGold(_)
        )),
        "玩家应看到掉落物的 ObjectItem/ObjectGold"
    );
    // 药水模板 `NeedIdentify = false` → 掉落时直接鉴定（`Envir.cs:4422`）。
    assert!(potion_item.identified);
    // 药水的模板耐久是 5000：`CreateDropItem` 会把当前耐久抽到
    // `min(Durability, Random.Next(Durability) + 1000)`（`Envir.cs:4415`）。
    let dura = potion_item.current_dura;
    assert!(
        (1_000..=5_000).contains(&dura),
        "当前耐久应在 1000..=5000，实际 {dura}"
    );
}

#[test]
fn monster_without_killer_or_drop_table_drops_nothing() {
    let mut harness = Harness::new(2);
    let data = game_data("no-drop", "1/1 Gold 100\n");
    harness.world.set_game_data(Arc::clone(&data));
    let player = harness.add_player(1, Point::new(20, 20));
    // 没有击杀者的怪直接 die（`EXPOwner == null` → 不掉，`MonsterObject.cs:997-998`）。
    let mut kind = test_monster();
    kind.name = "没有掉落表的怪".to_owned();
    let monster = harness.world.spawn_monster(MAP, kind, Point::new(22, 20));
    kill_monster(&mut harness, player, monster);
    assert!(
        harness.world.ground_items().is_empty(),
        "数据库里没有这只怪 → 不掉东西"
    );
}

// ---------------------------------------------------------------------------
// 3. 地面物品的视野同步与过期（Crystal `ItemObject.Process`）
// ---------------------------------------------------------------------------

#[test]
fn ground_item_enters_and_leaves_view_following_player_movement() {
    let mut harness = Harness::new(3);
    let data = game_data("view", "1/1 Gold 10\n");
    harness.world.set_game_data(data);
    let player = harness.add_player(1, Point::new(20, 20));
    harness.packets(player);

    // 放在视野内（距离 2 ≤ VIEW_RANGE 10）。
    harness
        .world
        .spawn_ground_item(MAP, Point::new(22, 20), None, 500, None);
    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::ObjectGold(gold) if gold.gold == 500
        )),
        "视野内应收到 ObjectGold"
    );

    // 走出视野（10 格切比雪夫距离）：坐标 20,20 → 9,20 之后距离 13。
    for _ in 0..11 {
        harness.ready_player(player);
        harness.world.walk(player, MirDirection::Left).expect("走");
    }
    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::ObjectRemove(remove) if remove.object_id != 0
        )),
        "走出视野应收到 ObjectRemove"
    );
}

#[test]
fn ground_item_expires_after_the_configured_timeout_and_broadcasts_remove() {
    let mut harness = Harness::new(4);
    harness.world.set_item_expire_ms(ITEM_EXPIRE_MS);
    let player = harness.add_player(1, Point::new(20, 20));
    let ground = harness
        .world
        .spawn_ground_item(MAP, Point::new(20, 20), None, 1_000, None);
    harness.packets(player);

    // `if (Envir.Time > ExpireTime)`（`ItemObject.cs:131`）是**严格大于**。
    // 用 `expire_time - now` 精确快进（不依赖真实时钟停在原地）。
    let expire_time = harness
        .world
        .ground_item(ground)
        .expect("物品还在")
        .expire_time;
    let delta = expire_time - harness.world.now();
    assert!(
        delta > 0 && delta <= ITEM_EXPIRE_MS,
        "存活时间应约为 ITEM_EXPIRE_MS，实际 {delta}"
    );
    harness.world.advance_clock(delta);
    harness.world.tick();
    assert!(
        harness.world.ground_item(ground).is_some(),
        "正好到点还不该消失"
    );

    harness.world.advance_clock(1);
    harness.world.tick();
    assert!(harness.world.ground_item(ground).is_none(), "过 1ms 后消失");
    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::ObjectRemove(remove) if remove.object_id == ground
        )),
        "消失应广播 ObjectRemove"
    );
}

// ---------------------------------------------------------------------------
// 4. 拾取（Crystal `PlayerObject.PickUp`）
// ---------------------------------------------------------------------------

#[test]
fn pickup_moves_item_into_inventory_and_sends_gained_item() {
    let mut harness = Harness::new(5);
    let data = game_data("pickup", "1/1 Gold 10\n");
    harness.world.set_game_data(Arc::clone(&data));
    let player = harness.add_player(1, Point::new(20, 20));
    let observer = harness.add_player(2, Point::new(18, 20));
    let info = data.item(2).expect("测试之剑").clone();
    let item = harness.world.create_fresh_item(&info);
    let ground =
        harness
            .world
            .spawn_ground_item(MAP, Point::new(20, 20), Some(item.clone()), 0, None);
    harness.packets(player);
    harness.packets(observer);

    let report = harness.world.pickup(player).expect("站在物品上应该能捡");
    assert_eq!(report.ground, ground);
    assert_eq!(
        report.item.as_ref().map(|item| item.unique_id),
        Some(item.unique_id)
    );
    assert!(
        harness.world.ground_item(ground).is_none(),
        "地面物品应消失"
    );
    // 武器不是药水，从 6 号格开始找空位（`HumanObject.AddItem`，`HumanObject.cs:1614-1622`）。
    let inventory = &harness.world.player(player).unwrap().inventory;
    assert_eq!(
        inventory[items::BELT_SIZE].as_ref().map(|i| i.unique_id),
        Some(item.unique_id)
    );
    // 本人收 `S.GainedItem`（`HumanObject.GainItem`，`:7831`）。
    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::GainedItem(gained) if gained.item.unique_id == item.unique_id
        )),
        "拾取者应收到 GainedItem"
    );
    // 周围人只看到 `ObjectRemove`。
    assert!(
        harness.has(observer, |packet| matches!(
            packet,
            ServerPacket::ObjectRemove(remove) if remove.object_id == ground
        )),
        "旁观者应收到 ObjectRemove"
    );
}

#[test]
fn pickup_only_reaches_the_players_own_cell() {
    let mut harness = Harness::new(6);
    let data = game_data("range", "1/1 Gold 10\n");
    harness.world.set_game_data(Arc::clone(&data));
    let player = harness.add_player(1, Point::new(20, 20));
    let info = data.item(2).expect("测试之剑").clone();
    let item = harness.world.create_fresh_item(&info);
    harness
        .world
        .spawn_ground_item(MAP, Point::new(21, 20), Some(item), 0, None);

    // Crystal 只扫 `CurrentMap.GetCell(CurrentLocation)`（`PlayerObject.cs:7491-7497`），
    // 相邻格**不会**被顺手捡起。
    assert_eq!(harness.world.pickup(player), Err(ItemError::NoGroundItem));
}

#[test]
fn pickup_fails_when_inventory_is_full() {
    let mut harness = Harness::new(7);
    let data = game_data("full", "1/1 Gold 10\n");
    harness.world.set_game_data(Arc::clone(&data));
    let player = harness.add_player(1, Point::new(20, 20));
    let info = data.item(2).expect("测试之剑").clone();
    let filler = harness.world.create_fresh_item(&info);
    if let Some(p) = harness.world.player_mut(player) {
        p.inventory = vec![Some(filler); INVENTORY_SIZE];
    }
    harness.world.spawn_ground_item(
        MAP,
        Point::new(20, 20),
        Some(harness.world.create_fresh_item(&info)),
        0,
        None,
    );

    // `CanGainItem` 返回假（`HumanObject.cs:7586-7628`）→ 捡不起来。
    assert_eq!(harness.world.pickup(player), Err(ItemError::InventoryFull));
    assert_eq!(harness.world.ground_items().len(), 1, "地面物品还在");
}

#[test]
fn pickup_rejects_overweight_but_can_be_disabled() {
    let mut harness = Harness::new(8);
    let data = game_data("weight", "1/1 Gold 10\n");
    harness.world.set_game_data(Arc::clone(&data));
    let player = harness.add_player(1, Point::new(20, 20));
    let heavy = data.item(3).expect("重甲").clone();
    // 1 级战士的背包负重是 50（`Shared/BaseStats.cs:18` 的 `StatFormula.Weight`），
    // 重甲 60 > 50。
    assert_eq!(harness.world.player(player).unwrap().stats.bag_weight, 50);
    harness.world.spawn_ground_item(
        MAP,
        Point::new(20, 20),
        Some(harness.world.create_fresh_item(&heavy)),
        0,
        None,
    );

    assert_eq!(
        harness.world.pickup(player),
        Err(ItemError::TooHeavy {
            weight: 60,
            limit: 50
        }),
        "本作扩展：拾取也查负重"
    );
    assert_eq!(harness.world.ground_items().len(), 1);

    // 关掉负重检查（Crystal 的原始行为）后能捡。
    harness.world.set_enforce_bag_weight(false);
    let report = harness.world.pickup(player).expect("关掉负重检查后可以捡");
    assert!(report.item.is_some());
}

#[test]
fn pickup_respects_owner_protection_until_it_expires() {
    let mut harness = Harness::new(9);
    let data = game_data("owner", "1/1 Gold 10\n");
    harness.world.set_game_data(Arc::clone(&data));
    let player = harness.add_player(1, Point::new(20, 20));
    let owner = harness.add_player(2, Point::new(60, 60));
    let info = data.item(2).expect("测试之剑").clone();
    harness.world.spawn_ground_item(
        MAP,
        Point::new(20, 20),
        Some(harness.world.create_fresh_item(&info)),
        0,
        Some(owner),
    );

    // `if (ob.Owner != null && ob.Owner != this && !IsGroupMember(ob.Owner))`（`:7501-7505`）。
    assert_eq!(
        harness.world.pickup(player),
        Err(ItemError::NotOwner { owner })
    );
    assert_eq!(harness.world.ground_items().len(), 1, "归属保护期内捡不走");

    // `OwnerTime` 到点后 `Owner` 被清空（`ItemObject.cs:138-139`）。
    harness.world.advance_clock(GROUND_ITEM_OWNER_MS + 1);
    harness.world.tick();
    assert!(harness.world.ground_items()[0].owner.is_none());
    assert!(harness.world.pickup(player).is_ok(), "保护期过后可以捡");
}

#[test]
fn picking_up_gold_credits_the_player_and_sends_gained_gold() {
    let mut harness = Harness::new(10);
    let player = harness.add_player(1, Point::new(20, 20));
    harness
        .world
        .spawn_ground_item(MAP, Point::new(20, 20), None, 1_234, None);
    harness.packets(player);

    let report = harness.world.pickup(player).expect("捡金币");
    assert_eq!(report.gold, 1_234);
    assert!(report.item.is_none());
    assert_eq!(harness.world.player(player).unwrap().gold, 1_234);
    // `Enqueue(new S.GainedGold { Gold = gold });`（`PlayerObject.cs:7682`）。
    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::GainedGold(gold) if gold.gold == 1_234
        )),
        "本人应收到 GainedGold"
    );
    assert!(harness.world.ground_items().is_empty());
}

// ---------------------------------------------------------------------------
// 5. 丢弃（Crystal `PlayerObject.DropItem`）
// ---------------------------------------------------------------------------

#[test]
fn drop_item_removes_it_from_the_bag_and_creates_a_ground_item() {
    let mut harness = Harness::new(11);
    let data = game_data("drop-item", "1/1 Gold 10\n");
    harness.world.set_game_data(Arc::clone(&data));
    let player = harness.add_player(1, Point::new(20, 20));
    let observer = harness.add_player(2, Point::new(18, 20));
    let info = data.item(2).expect("测试之剑").clone();
    let item = harness.give(player, &info);
    harness.packets(player);
    harness.packets(observer);

    let ground = harness
        .world
        .drop_item(player, item.unique_id, 0)
        .expect("应该能丢");
    let dropped = harness.world.ground_item(ground).expect("地面物品存在");
    assert_eq!(
        dropped.item.as_ref().map(|i| i.unique_id),
        Some(item.unique_id)
    );
    assert!(dropped.owner.is_none(), "玩家丢的东西没有归属保护");
    // 背包里那一格应清空。
    assert!(
        harness
            .world
            .player(player)
            .unwrap()
            .inventory
            .iter()
            .flatten()
            .all(|slot| slot.unique_id != item.unique_id),
        "背包里不该再有这件物品"
    );
    // 本人收 `S.DropItem{Success = true}`（`:7459-7460`）。
    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::DropItem(drop) if drop.success && drop.unique_id == item.unique_id
        )),
        "本人应收到 DropItem 成功包"
    );
    // 周围人看到 `ObjectItem`（`Spawned()` 广播 `GetInfo()`）。
    assert!(
        harness.has(observer, |packet| matches!(
            packet,
            ServerPacket::ObjectItem(object) if object.object_id == ground
        )),
        "旁观者应收到 ObjectItem"
    );

    // 再丢一次不存在的 UUID：失败包 + 错误。
    assert_eq!(
        harness.world.drop_item(player, item.unique_id, 1),
        Err(ItemError::ItemNotFound {
            unique_id: item.unique_id
        })
    );
}

#[test]
fn dropping_part_of_a_stack_keeps_the_rest_in_the_bag() {
    let mut harness = Harness::new(12);
    let data = game_data("split", "1/1 Gold 10\n");
    harness.world.set_game_data(Arc::clone(&data));
    let player = harness.add_player(1, Point::new(20, 20));
    let info = data.item(1).expect("金创药").clone();
    let mut potion = harness.give(player, &info);
    potion.count = 5;
    // 统一放到 6 号格，避免「药水腰带区」同一件物品出现在两格里（UUID 会重复）。
    if let Some(p) = harness.world.player_mut(player) {
        p.inventory = items::new_inventory();
        p.inventory[items::BELT_SIZE] = Some(potion.clone());
    }

    let ground = harness
        .world
        .drop_item(player, potion.unique_id, 2)
        .expect("拆一半丢");
    let dropped = harness
        .world
        .ground_item(ground)
        .and_then(|item| item.item.clone())
        .expect("地面物品");
    assert_eq!(dropped.count, 2, "地上是拆出来的 2 个");
    assert_ne!(
        dropped.unique_id, potion.unique_id,
        "拆出来的新物品用新 UUID（`Envir.CreateFreshItem`，`:7449-7450`）"
    );
    let left = harness.world.player(player).unwrap().inventory[items::BELT_SIZE]
        .as_ref()
        .expect("背包里还有剩余");
    assert_eq!(left.count, 3, "背包里剩 3 个");
}

// ---------------------------------------------------------------------------
// 6. 背包交换（Crystal `PlayerObject.MoveItem`）
// ---------------------------------------------------------------------------

#[test]
fn moving_inside_the_inventory_swaps_two_slots() {
    let mut harness = Harness::new(13);
    let data = game_data("move", "1/1 Gold 10\n");
    harness.world.set_game_data(Arc::clone(&data));
    let player = harness.add_player(1, Point::new(20, 20));
    let sword = data.item(2).expect("测试之剑").clone();
    let armour = data.item(3).expect("重甲").clone();
    let first = harness.world.create_fresh_item(&sword);
    let second = harness.world.create_fresh_item(&armour);
    if let Some(p) = harness.world.player_mut(player) {
        p.inventory[0] = Some(first.clone());
        p.inventory[1] = Some(second.clone());
    }
    harness.packets(player);

    harness
        .world
        .move_item(player, MirGridType::Inventory, 0, 1)
        .expect("交换");
    let inventory = &harness.world.player(player).unwrap().inventory;
    assert_eq!(
        inventory[0].as_ref().map(|i| i.unique_id),
        Some(second.unique_id)
    );
    assert_eq!(
        inventory[1].as_ref().map(|i| i.unique_id),
        Some(first.unique_id)
    );
    // 成功本人收 `S.MoveItem{Success = true}`（`:5369-5370`）。
    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::MoveItem(moved) if moved.success && moved.from == 0 && moved.to == 1
        )),
        "本人应收到 MoveItem 成功包"
    );

    // `from` 是空格 → 失败包 + 错误（`:5352-5358`）。
    assert_eq!(
        harness
            .world
            .move_item(player, MirGridType::Inventory, 40, 0),
        Err(ItemError::EmptySlot {
            grid: MirGridType::Inventory,
            index: 40
        })
    );
    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::MoveItem(moved) if !moved.success
        )),
        "失败也要回一条 MoveItem"
    );

    // 越界。
    assert!(matches!(
        harness
            .world
            .move_item(player, MirGridType::Inventory, 0, 999),
        Err(ItemError::SlotOutOfRange { .. })
    ));
}

// ---------------------------------------------------------------------------
// 7. 装备穿戴（Crystal `EquipItem` / `CanEquipItem` / `RefreshEquipmentStats`）
// ---------------------------------------------------------------------------

#[test]
fn equipping_a_weapon_applies_its_stats_and_broadcasts_player_update() {
    let mut harness = Harness::new(14);
    let data = game_data("equip", "1/1 Gold 10\n");
    harness.world.set_game_data(Arc::clone(&data));
    let player = harness.add_player(1, Point::new(20, 20));
    let observer = harness.add_player(2, Point::new(18, 20));
    let info = data.item(2).expect("测试之剑").clone();
    let sword = harness.give(player, &info);
    harness.packets(player);
    harness.packets(observer);

    let before = harness.world.player(player).unwrap().stats;
    assert_eq!((before.min_dc, before.max_dc), (0, 0), "1 级战士裸装 DC 0");

    // 背包 6 号格 → 武器槽 0（Crystal `EquipmentSlot.Weapon = 0`，`Shared/Enums.cs:956`）。
    harness
        .world
        .move_item(player, MirGridType::Equipment, items::BELT_SIZE as i32, 0)
        .expect("穿上武器");

    let p = harness.world.player(player).unwrap();
    assert_eq!(
        p.equipment[0].as_ref().map(|item| item.unique_id),
        Some(sword.unique_id)
    );
    assert!(p.inventory[items::BELT_SIZE].is_none(), "背包格被腾空");
    assert_eq!(
        (p.stats.min_dc, p.stats.max_dc),
        (1, 5),
        "`Stats.Add(realItem.Stats)`（`HumanObject.cs:1858`）"
    );
    // 本人收 `S.EquipItem{Grid = Inventory, Success = true}`（`PlayerObject.cs:5682-5683`）。
    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::EquipItem(equip)
                if equip.success && equip.unique_id == sword.unique_id && equip.to == 0
        )),
        "本人应收到 EquipItem 成功包"
    );
    // 外观变化广播 `S.PlayerUpdate`（`HumanObject.UpdateLooks`，`:7058-7061`）。
    assert!(
        harness.has(observer, |packet| matches!(
            packet,
            ServerPacket::PlayerUpdate(update)
                if update.object_id == player && update.weapon == info.shape
        )),
        "旁观者应收到 PlayerUpdate，且武器外观来自 Shape"
    );
}

#[test]
fn equipping_checks_slot_type_class_and_required_level() {
    let mut harness = Harness::new(15);
    let data = game_data("equip-checks", "1/1 Gold 10\n");
    harness.world.set_game_data(Arc::clone(&data));
    let player = harness.add_player(1, Point::new(20, 20));

    // 1) 盔甲不能塞进武器槽（`HumanObject.cs:7663-7721`）。
    let armour = data.item(3).expect("重甲").clone();
    let item = harness.give(player, &armour);
    let slot = harness
        .world
        .find_inventory_item(player, item.unique_id)
        .expect("背包里能找到")
        .0 as i32;
    assert_eq!(
        harness
            .world
            .move_item(player, MirGridType::Equipment, slot, 0),
        Err(ItemError::CannotEquip {
            item_index: item.item_index,
            slot: 0
        })
    );

    // 2) 需要 10 级（`RequiredType.Level`，`:7756-7761`）。
    let high = data.item(4).expect("高级剑").clone();
    let item = harness.give(player, &high);
    let slot = harness
        .world
        .find_inventory_item(player, item.unique_id)
        .expect("背包里能找到")
        .0 as i32;
    assert!(
        matches!(
            harness
                .world
                .move_item(player, MirGridType::Equipment, slot, 0),
            Err(ItemError::CannotEquip { .. })
        ),
        "1 级不该能穿 10 级武器"
    );

    // 3) 法师专用（`RequiredClass.Wizard`，`:7736-7749`）。
    let staff = data.item(5).expect("法师杖").clone();
    let item = harness.give(player, &staff);
    let slot = harness
        .world
        .find_inventory_item(player, item.unique_id)
        .expect("背包里能找到")
        .0 as i32;
    assert!(
        matches!(
            harness
                .world
                .move_item(player, MirGridType::Equipment, slot, 0),
            Err(ItemError::CannotEquip { .. })
        ),
        "战士不该能穿法师专用武器"
    );
}

#[test]
fn unequipping_requires_an_empty_inventory_slot() {
    let mut harness = Harness::new(16);
    let data = game_data("unequip", "1/1 Gold 10\n");
    harness.world.set_game_data(Arc::clone(&data));
    let player = harness.add_player(1, Point::new(20, 20));
    let info = data.item(2).expect("测试之剑").clone();
    let sword = harness.give(player, &info);
    harness
        .world
        .move_item(player, MirGridType::Equipment, items::BELT_SIZE as i32, 0)
        .expect("穿上");
    assert_eq!(
        harness.world.player(player).unwrap().stats.max_dc,
        5,
        "穿上后属性生效"
    );

    // 目标背包格有东西 → Crystal `if (toArray[to] == null)` 不成立（`:5118`）。
    let filler = harness.world.create_fresh_item(&info);
    if let Some(p) = harness.world.player_mut(player) {
        p.inventory[20] = Some(filler);
    }
    assert_eq!(
        harness
            .world
            .move_item(player, MirGridType::Equipment, 0, 20),
        Err(ItemError::SlotOccupied {
            grid: MirGridType::Inventory,
            index: 20
        })
    );
    assert!(
        harness.world.player(player).unwrap().equipment[0].is_some(),
        "失败时装备要留在身上"
    );

    // 换到空格 → 脱下，属性回落（`RefreshStats`）。
    if let Some(p) = harness.world.player_mut(player) {
        p.inventory[20] = None;
    }
    harness.packets(player);
    harness
        .world
        .move_item(player, MirGridType::Equipment, 0, 20)
        .expect("脱下");
    let p = harness.world.player(player).unwrap();
    assert!(p.equipment[0].is_none());
    assert_eq!(
        p.inventory[20].as_ref().map(|item| item.unique_id),
        Some(sword.unique_id)
    );
    assert_eq!(p.stats.max_dc, 0, "脱下后装备加成消失");
    // 本人收 `S.RemoveItem{Grid = Equipment, Success = true}`（`:5123-5124`）。
    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::RemoveItem(remove) if remove.success && remove.grid == MirGridType::Equipment
        )),
        "本人应收到 RemoveItem 成功包"
    );
}

// ---------------------------------------------------------------------------
// 8. 持久化编解码（Crystal `CharacterInfo.Save` 的 `UserItem[]` 整块）
// ---------------------------------------------------------------------------

#[test]
fn inventory_blob_roundtrips_through_crystal_layout() {
    let mut nested = UserItem {
        unique_id: 77,
        item_index: 1,
        slots: vec![None],
        ..UserItem::default()
    };
    nested.slots[0] = Some(UserItem {
        unique_id: 78,
        item_index: 1,
        ..UserItem::default()
    });
    let slots = vec![
        Some(UserItem {
            unique_id: 11,
            item_index: 2,
            count: 3,
            current_dura: 1_234,
            max_dura: 5_000,
            ..UserItem::default()
        }),
        None,
        Some(nested),
    ];

    let bytes = items::encode_slots(&slots);
    // 头部是 `int32 count`（`CharacterInfo.cs:431`）。
    assert_eq!(&bytes[..4], &(3i32).to_le_bytes());
    let decoded = items::decode_slots(&bytes).expect("解码");
    assert_eq!(decoded.len(), 3);
    assert_eq!(decoded, slots, "整块往返必须逐字段一致（含嵌套槽位）");

    // 空字节块 = 新角色 → 空背包/空装备栏。
    assert_eq!(items::decode_inventory(&[]).unwrap().len(), INVENTORY_SIZE);
    assert_eq!(
        items::decode_equipment(&[]).unwrap().len(),
        items::EQUIPMENT_SLOTS
    );
    // 存档里槽位比本作支持的多：截断到 46。
    let mut long = slots.clone();
    long.resize(MAX_BOUND_FOR_TEST, None);
    let bytes = items::encode_slots(&long);
    assert_eq!(
        items::decode_inventory(&bytes).unwrap().len(),
        INVENTORY_SIZE,
        "多出来的槽位丢掉"
    );

    // 非法计数（超过 86）直接拒绝，避免按不可信长度分配内存。
    let bogus = (1_000i32).to_le_bytes();
    assert!(matches!(
        items::decode_slots(&bogus),
        Err(items::ItemCodecError::TooManySlots { .. })
    ));
    // 截断的字节块报协议错误。
    assert!(items::decode_slots(&[0xFF]).is_err());
}

/// 「比 46 长」的存档长度（Crystal 扩容背包的上限是 86，`CharacterInfo.cs:596`）。
const MAX_BOUND_FOR_TEST: usize = items::MAX_INVENTORY_SIZE;

// ---------------------------------------------------------------------------
// 9. 物品 UUID（Crystal `Envir.NextUserItemID`）
// ---------------------------------------------------------------------------

#[test]
fn item_uid_generator_is_monotonic_and_restores_from_a_database_maximum() {
    // Crystal 是 `++NextUserItemID`，从 0 起（`Envir.cs:4394`），所以第一个 ID 是 1。
    let generator = ItemUidGenerator::new(0);
    assert_eq!(generator.next(), 1);
    assert_eq!(generator.next(), 2);
    assert_eq!(generator.peek(), 2);

    // 读档发现数据库里已经有 100 号：发号必须从 101 开始（本作的「从最大值恢复」）。
    generator.observe(100);
    assert_eq!(generator.peek(), 100);
    assert_eq!(generator.next(), 101);

    // 时间戳起点：远大于任何 1、2、3… 的紧凑序列，进程重启也不会撞号。
    let timed = ItemUidGenerator::from_timestamp();
    assert!(timed.next() > 1_000_000, "时间戳起点应该很大");
    assert!(timed.next() > timed.peek() - 1, "严格递增");
}

#[test]
fn newly_created_items_get_unique_ids_from_the_generator() {
    let mut harness = Harness::new(17);
    let data = game_data("uuid", "1/1 Gold 10\n");
    harness.world.set_game_data(Arc::clone(&data));
    let info = data.item(2).expect("测试之剑").clone();
    let first = harness.world.create_fresh_item(&info);
    let second = harness.world.create_drop_item(&info);
    assert_ne!(first.unique_id, second.unique_id, "UUID 不能撞号");
    assert!(second.unique_id > first.unique_id, "单调递增");
}
