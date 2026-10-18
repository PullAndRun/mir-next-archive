//! 技能（魔法）系统的端到端测试：学书、施法校验、伤害/治疗结算、技能经验与持久化。
//!
//! 全部用例都是**确定性的**：
//!
//! * 世界随机数固定种子（[`World::seed_rng`]）；
//! * 时间用 [`World::advance_clock`] 快进，不 sleep；
//! * 怪物定时器设成 `i64::MAX`（`tick_monsters` 的条件是 `now <= operate_time`），
//!   让测试怪既不思考也不还手；
//! * 玩家的 `regen_time` 同样推到 `i64::MAX`，避免自然回血干扰治疗断言。
//!
//! 每个断言都尽量对应 Crystal 的一处行为，注释里写了出处。

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{
    DamageType, ItemType, MirClass, MirDirection, MirGender, Monster as MonsterImage,
    RequiredClass, RequiredType, Spell,
};
use mir_db::{MagicInfo, MirDb};
use mir_protocol::ServerPacket;
use mir_protocol::data::ItemInfo;
use mir_server::MonsterKind;
use mir_server::combat::{self, CombatStats};
use mir_server::gamedata::GameData;
use mir_server::magic::{self, MagicError, UserMagic, decode_magics, encode_magics};
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
        Self::with_data(seed, game_data())
    }

    fn with_data(seed: u64, data: Arc<GameData>) -> Self {
        let mut world = World::new(999);
        world.seed_rng(seed);
        world.set_game_data(data);
        Self {
            world,
            receivers: HashMap::new(),
        }
    }

    fn add_player(&mut self, object_id: u32, class: MirClass, level: u16, position: Point) -> u32 {
        let stats = combat::player_base_stats(level, class);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.enter(Player {
            object_id,
            session_id: u64::from(object_id),
            character_id: i64::from(object_id),
            name: format!("玩家{object_id}"),
            map: MAP.to_owned(),
            position,
            direction: MirDirection::Down,
            level,
            experience: 0,
            max_experience: 0,
            class,
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
            inventory: mir_server::items::new_inventory(),
            equipment: mir_server::items::new_equipment(),
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
            sender,
        });
        // `World::enter` 会把 `RegenTime` 推到「一个回复周期之后」；测试里直接关掉，
        // 免得 10 秒的自然回血混进治疗断言。
        if let Some(player) = self.world.player_mut(object_id) {
            player.regen_time = i64::MAX;
        }
        self.receivers.insert(object_id, receiver);
        object_id
    }

    /// 刷一只用不动的测试怪（`operate_time = i64::MAX` 让 `tick_monsters` 永远跳过它）。
    fn spawn(&mut self, kind: MonsterKind, position: Point) -> u32 {
        let object_id = self.world.spawn_monster(MAP, kind, position);
        if let Some(monster) = self.world.monster_mut(object_id) {
            monster.operate_time = i64::MAX;
            monster.regen_time = i64::MAX;
        }
        object_id
    }

    fn learn(&mut self, object_id: u32, spell: Spell) {
        self.world
            .learn_magic(object_id, spell)
            .expect("学习应当成功");
    }

    /// 清掉动作/技能计时（等价于「等了足够久」），但保留技能冷却。
    fn ready(&mut self, object_id: u32) {
        if let Some(player) = self.world.player_mut(object_id) {
            player.action_time = 0;
            player.attack_time = 0;
            player.magic.spell_time = 0;
        }
    }

    /// 动作计时、技能间隔与**所有技能冷却**一起清掉，并回满魔法。
    fn ready_all(&mut self, object_id: u32) {
        let Some(player) = self.world.player_mut(object_id) else {
            return;
        };
        player.action_time = 0;
        player.attack_time = 0;
        player.magic.spell_time = 0;
        player.magic.casts.clear();
        player.mp = player.stats.mp;
    }

    fn elapse(&mut self, ms: i64) {
        self.world.advance_clock(ms);
        self.world.tick();
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

    fn drain(&mut self, object_id: u32) {
        self.packets(object_id);
    }

    fn hp(&self, monster: u32) -> i32 {
        self.world.monster(monster).expect("怪物还在").hp
    }

    fn player_hp(&self, object_id: u32) -> i32 {
        self.world.player(object_id).expect("玩家还在").hp
    }

    /// 往背包里塞一件物品（走 `Envir.CreateFreshItem` 的等价入口）。
    fn give(&mut self, object_id: u32, index: i32) -> u64 {
        let info = self.world.item_info(index).expect("物品模板").clone();
        let item = self.world.create_fresh_item(&info);
        let unique_id = item.unique_id;
        assert!(
            self.world.add_item_to_inventory(object_id, item),
            "背包应该有空位"
        );
        unique_id
    }
}

/// 技能模板：`MagicInfo` 的最小可用样本。
///
/// 数值刻意取整：`MPower = 10`、`DefPower = 4`、倍率 1.0，于是 0 级技能的威力是
/// `round((10/4)*1 + 4) = round(6.5) = 6`（银行家舍入 → 6），伤害可直接断言。
fn magic_info(spell: Spell, base_cost: u8, delay_base: u32, range: u8) -> MagicInfo {
    MagicInfo {
        name: format!("{spell:?}"),
        spell: spell.value(),
        base_cost,
        level_cost: 2,
        icon: 10,
        level1: 1,
        level2: 2,
        level3: 3,
        need1: 10,
        need2: 20,
        need3: 30,
        delay_base,
        delay_reduction: 100,
        power_base: 4,
        power_bonus: 0,
        m_power_base: 10,
        m_power_bonus: 0,
        range,
        multiplier_base: 1.0,
        multiplier_bonus: 0.0,
    }
}

fn book(index: i32, spell: Spell, class: RequiredClass, level: u8) -> ItemInfo {
    ItemInfo {
        index,
        name: format!("{spell:?}技能书"),
        item_type: ItemType::Book,
        // `UserMagic((Spell)item.Info.Shape)`（`PlayerObject.cs:6078`）。
        shape: i16::from(spell.value()),
        weight: 1,
        required_class: class,
        required_type: RequiredType::Level,
        required_amount: level,
        ..ItemInfo::default()
    }
}

fn game_data() -> Arc<GameData> {
    game_data_with(magic_info(Spell::FireBall, 5, 1_800, 8))
}

/// 用给定的火球模板构造一份完整夹具（其余技能用默认参数）。
fn game_data_with(fireball: MagicInfo) -> Arc<GameData> {
    let db = MirDb {
        magics: vec![
            fireball,
            magic_info(Spell::GreatFireBall, 6, 1_800, 8),
            magic_info(Spell::ThunderBolt, 6, 1_800, 8),
            magic_info(Spell::SoulFireBall, 6, 1_800, 8),
            magic_info(Spell::HellFire, 5, 1_800, 8),
            magic_info(Spell::FireBang, 5, 1_800, 8),
            magic_info(Spell::Healing, 3, 1_800, 8),
            magic_info(Spell::MassHealing, 5, 1_800, 8),
            // 增益技能（`SpellKind::Buff`）：`Haste` 已经实现，`Rage` 仍是「登记但无效果」。
            magic_info(Spell::Haste, 5, 1_800, 8),
            magic_info(Spell::Rage, 5, 1_800, 8),
        ],
        items: vec![
            book(1, Spell::FireBall, RequiredClass::None, 1),
            book(2, Spell::Healing, RequiredClass::None, 1),
            book(3, Spell::GreatFireBall, RequiredClass::Wizard, 10),
            ItemInfo {
                index: 4,
                name: "金创药(小量)".to_owned(),
                item_type: ItemType::Potion,
                weight: 1,
                ..ItemInfo::default()
            },
        ],
        ..MirDb::default()
    };
    Arc::new(GameData::from_db(db, None))
}

/// 测试怪物：`min_mac` 是魔法防御，`magic_resist` 是魔法闪避（`Stat.MagicResist`）。
fn test_monster(hp: i32, min_mac: i32, magic_resist: i32) -> MonsterKind {
    MonsterKind {
        name: "测试怪".to_owned(),
        image: MonsterImage::Oma,
        ai: 0,
        level: 1,
        view_range: 1,
        cool_eye: 0,
        attack_speed_ms: 2_500,
        move_speed_ms: 1_800,
        experience: 50,
        can_push: true,
        stats: CombatStats {
            hp,
            min_mac,
            max_mac: min_mac,
            magic_resist,
            ..CombatStats::default()
        },
    }
}

// ---------------------------------------------------------------------------
// 1. 学技能（Crystal `PlayerObject.UseItem` 的 `ItemType.Book` 分支）
// ---------------------------------------------------------------------------

#[test]
fn book_learning_adds_magic_sends_new_magic_and_consumes_book() {
    let mut harness = Harness::new(1);
    let player = harness.add_player(1, MirClass::Wizard, 1, Point::new(20, 20));
    let book_id = harness.give(player, 1);
    harness.drain(player);

    let learned = harness.world.use_book(player, book_id).expect("学会");
    assert_eq!(learned.magic.spell, Spell::FireBall);
    assert_eq!(learned.magic.level, 0, "新技能从 0 级开始");
    assert!(learned.book.is_none(), "只有一本时整格清空");

    // 背包里已经没有这本书了（`if (item.Count > 1) item.Count--; else Info.Inventory[index] = null;`，
    // `PlayerObject.cs:6330-6331`）。
    assert!(
        harness.world.find_inventory_item(player, book_id).is_none(),
        "书应被消耗"
    );

    let packets = harness.packets(player);
    // `SendMagicInfo` → `S.NewMagic`（`HumanObject.cs:2275-2278`）。
    let new_magic = packets.iter().find_map(|packet| match packet {
        ServerPacket::NewMagic(new_magic) => Some(new_magic),
        _ => None,
    });
    let new_magic = new_magic.expect("应收到 S.NewMagic");
    assert_eq!(new_magic.magic.spell, Spell::FireBall);
    assert_eq!(new_magic.magic.level, 0);
    assert_eq!(new_magic.magic.base_cost, 5);
    assert_eq!(new_magic.magic.delay, 1_800);
    assert!(!new_magic.hero);
    // `p.Success = true; Enqueue(p);`（`:6336-6337`）。
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::UseItem(use_item) if use_item.unique_id == book_id && use_item.success
        )),
        "应收到成功的 S.UseItem，实际 {packets:?}"
    );
}

#[test]
fn learning_the_same_book_twice_is_rejected() {
    let mut harness = Harness::new(2);
    let player = harness.add_player(1, MirClass::Wizard, 1, Point::new(20, 20));
    let first = harness.give(player, 1);
    let second = harness.give(player, 1);
    harness.drain(player);

    harness.world.use_book(player, first).expect("第一次学会");
    let error = harness
        .world
        .use_book(player, second)
        .expect_err("第二次应当被拒绝");
    assert_eq!(error, MagicError::AlreadyLearned(Spell::FireBall));
    assert_eq!(
        harness.world.player(player).unwrap().magics.len(),
        1,
        "不应出现两条同 spell 的记录"
    );

    let packets = harness.packets(player);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::UseItem(use_item) if use_item.unique_id == second && !use_item.success
        )),
        "失败应发 S.UseItem{{Success=false}}，实际 {packets:?}"
    );
    assert!(
        packets.iter().any(
            |packet| matches!(packet, ServerPacket::Chat(chat) if chat.message.contains("已经学会"))
        ),
        "应给一条系统提示，实际 {packets:?}"
    );
    assert!(
        harness.world.find_inventory_item(player, second).is_some(),
        "被拒绝的书不该被消耗"
    );
}

#[test]
fn book_class_requirement_is_enforced() {
    let mut harness = Harness::new(3);
    // 3 号书是「法师专属、需要 10 级」。战士用 → 先撞职业检查（`HumanObject.cs:1042-1079`）。
    let player = harness.add_player(1, MirClass::Warrior, 5, Point::new(20, 20));
    let book_id = harness.give(player, 3);
    harness.drain(player);

    let error = harness.world.use_book(player, book_id).unwrap_err();
    assert_eq!(
        error,
        MagicError::WrongClass {
            required: RequiredClass::Wizard,
            class: MirClass::Warrior,
        }
    );
    assert!(
        harness.packets(player).iter().any(
            |packet| matches!(packet, ServerPacket::Chat(chat) if chat.message.contains("职业"))
        ),
        "应提示职业不符"
    );
}

#[test]
fn book_level_requirement_is_enforced() {
    let mut harness = Harness::new(4);
    // 法师 5 级用同一本书：职业对了，等级不够（`RequiredType.Level`，`HumanObject.cs:1081-1089`）。
    let player = harness.add_player(1, MirClass::Wizard, 5, Point::new(20, 20));
    let book_id = harness.give(player, 3);
    harness.drain(player);

    let error = harness.world.use_book(player, book_id).unwrap_err();
    assert_eq!(
        error,
        MagicError::LowLevel {
            required: 10,
            level: 5
        }
    );
    assert!(
        harness.packets(player).iter().any(
            |packet| matches!(packet, ServerPacket::Chat(chat) if chat.message.contains("等级不够"))
        ),
        "应提示等级不够"
    );
}

#[test]
fn using_a_non_book_item_fails_without_learning() {
    let mut harness = Harness::new(5);
    let player = harness.add_player(1, MirClass::Wizard, 1, Point::new(20, 20));
    let potion = harness.give(player, 4);
    harness.drain(player);

    let error = harness.world.use_book(player, potion).unwrap_err();
    assert_eq!(error, MagicError::NotABook { item_index: 4 });
    assert!(harness.world.player(player).unwrap().magics.is_empty());
    assert!(
        harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::UseItem(use_item) if !use_item.success)),
        "失败应发 S.UseItem{{Success=false}}"
    );
}

// ---------------------------------------------------------------------------
// 2. 施法校验
// ---------------------------------------------------------------------------

#[test]
fn casting_without_mana_is_rejected_and_spends_nothing() {
    let mut harness = Harness::new(6);
    let player = harness.add_player(1, MirClass::Wizard, 1, Point::new(20, 20));
    let monster = harness.spawn(test_monster(100, 0, 0), Point::new(22, 20));
    harness.learn(player, Spell::FireBall);
    if let Some(p) = harness.world.player_mut(player) {
        p.mp = 1; // 火球耗蓝 5
    }
    harness.drain(player);

    let error = harness
        .world
        .cast_magic(
            player,
            Spell::FireBall,
            MirDirection::Right,
            monster,
            Point::new(22, 20),
        )
        .expect_err("蓝不够应当拒绝");
    assert_eq!(error, MagicError::NotEnoughMana { cost: 5, mp: 1 });
    assert_eq!(harness.world.player(player).unwrap().mp, 1, "不该扣蓝");

    // 拒绝时给本人回一条 `UserLocation`（`HumanObject.cs:3446`）。
    assert!(
        harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::UserLocation(_))),
        "被拒绝时应回 UserLocation"
    );

    // 也不该排入任何伤害：推进 1 秒后怪物满血。
    harness.elapse(1_000);
    assert_eq!(harness.hp(monster), 100);
}

#[test]
fn casting_before_action_time_ends_is_rejected() {
    let mut harness = Harness::new(7);
    let player = harness.add_player(1, MirClass::Wizard, 1, Point::new(20, 20));
    let monster = harness.spawn(test_monster(100, 0, 0), Point::new(22, 20));
    harness.learn(player, Spell::FireBall);
    harness.ready_all(player);

    harness
        .world
        .cast_magic(
            player,
            Spell::FireBall,
            MirDirection::Right,
            monster,
            Point::new(22, 20),
        )
        .expect("第一次施法");

    // 立刻再施法：`CanCast` 的 `SpellTime = now + 1800` 还没到（`HumanObject.cs:3425`）。
    let error = harness
        .world
        .cast_magic(
            player,
            Spell::FireBall,
            MirDirection::Right,
            monster,
            Point::new(22, 20),
        )
        .expect_err("应被 Busy 拒绝");
    assert!(
        matches!(error, MagicError::Busy { .. }),
        "期望 Busy，实际 {error:?}"
    );
}

#[test]
fn casting_during_cooldown_is_rejected() {
    // 3 秒冷却的夹具：比 1.8 秒的技能间隔长，才能把「冷却」单独测出来。
    let harness_data = game_data_with(magic_info(Spell::FireBall, 5, 3_000, 8));
    let mut harness = Harness::with_data(20, harness_data);
    let player = harness.add_player(1, MirClass::Wizard, 1, Point::new(20, 20));
    let monster = harness.spawn(test_monster(100, 0, 0), Point::new(22, 20));
    harness.learn(player, Spell::FireBall);
    harness.ready_all(player);

    harness
        .world
        .cast_magic(
            player,
            Spell::FireBall,
            MirDirection::Right,
            monster,
            Point::new(22, 20),
        )
        .expect("第一次施法");
    harness.ready(player); // 只清动作/技能间隔，冷却保留
    harness.elapse(1_800); // 技能间隔过了，但 3 秒冷却还没到

    let error = harness
        .world
        .cast_magic(
            player,
            Spell::FireBall,
            MirDirection::Right,
            monster,
            Point::new(22, 20),
        )
        .expect_err("冷却中应当拒绝");
    assert!(
        matches!(error, MagicError::CoolingDown { .. }),
        "期望 CoolingDown，实际 {error:?}"
    );

    // 再等 1.2 秒（总共 3 秒）就允许了。
    harness.ready(player);
    harness.elapse(1_200);
    harness
        .world
        .cast_magic(
            player,
            Spell::FireBall,
            MirDirection::Right,
            monster,
            Point::new(22, 20),
        )
        .expect("冷却结束后应能再施法");
}

#[test]
fn casting_out_of_range_is_rejected() {
    // 射程只有 2 的夹具。
    let harness_data = game_data_with(magic_info(Spell::FireBall, 5, 1_800, 2));
    let mut harness = Harness::with_data(21, harness_data);
    let player = harness.add_player(1, MirClass::Wizard, 1, Point::new(20, 20));
    let monster = harness.spawn(test_monster(100, 0, 0), Point::new(23, 20));
    harness.learn(player, Spell::FireBall);
    harness.ready_all(player);
    harness.drain(player);

    // 目标在 10 格搜索半径内（`FindObject(targetID, 10)`），但超出技能射程 3 > 2。
    let error = harness
        .world
        .cast_magic(
            player,
            Spell::FireBall,
            MirDirection::Right,
            monster,
            Point::new(23, 20),
        )
        .expect_err("超距应当拒绝");
    assert_eq!(
        error,
        MagicError::OutOfRange {
            distance: 3,
            range: 2
        }
    );
    // Crystal 在范围检查失败时什么都不发（`HumanObject.cs:3416`）。
    assert!(
        harness.packets(player).is_empty(),
        "范围检查失败不该发任何包"
    );
}

// ---------------------------------------------------------------------------
// 3. 伤害与治疗
// ---------------------------------------------------------------------------

#[test]
fn fireball_damages_monster_through_magic_defence_after_delay() {
    let mut harness = Harness::new(9);
    let player = harness.add_player(1, MirClass::Wizard, 1, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Warrior, 1, Point::new(19, 19));
    // 魔法防御 2：伤害 6 - 2 = 4。
    let monster = harness.spawn(test_monster(100, 2, 0), Point::new(22, 20));
    harness.learn(player, Spell::FireBall);
    harness.ready_all(player);
    harness.drain(player);
    harness.drain(observer);

    let report = harness
        .world
        .cast_magic(
            player,
            Spell::FireBall,
            MirDirection::Right,
            monster,
            Point::new(22, 20),
        )
        .expect("施法成功");
    assert_eq!(report.damage, Some(6), "0 级火球的威力应为 6");
    assert_eq!(report.cost, 5);
    assert_eq!(harness.world.player(player).unwrap().mp, 17 - 5);

    // 本人收 `S.Magic`，周围玩家收 `S.ObjectMagic`（`HumanObject.cs:3778-3779`）。
    let own = harness.packets(player);
    assert!(
        own.iter().any(|packet| matches!(
            packet,
            ServerPacket::Magic(magic) if magic.spell == Spell::FireBall
                && magic.cast && magic.level == 0 && magic.target_id == monster
        )),
        "本人应收到 S.Magic，实际 {own:?}"
    );
    assert!(
        !own.iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectMagic(_))),
        "S.ObjectMagic 是广播给周围玩家的，本人不该收到"
    );
    let seen = harness.packets(observer);
    assert!(
        seen.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectMagic(magic) if magic.object_id == player
                && magic.spell == Spell::FireBall && magic.cast
        )),
        "旁观者应收到 S.ObjectMagic，实际 {seen:?}"
    );

    // 距离 2 → 延迟 `2 * 50 + 500 = 600ms`（`HumanObject.cs:3945`）。
    harness.elapse(500);
    assert_eq!(harness.hp(monster), 100, "500ms 时还没落地");
    harness.elapse(100);
    assert_eq!(harness.hp(monster), 96, "600ms 后应扣 6 - 2 = 4 点");
}

#[test]
fn magic_resist_causes_the_bolt_to_miss() {
    let mut harness = Harness::new(10);
    let player = harness.add_player(1, MirClass::Wizard, 1, Point::new(20, 20));
    // 魔法闪避 10 → `Random.Next(10) < 10` 恒成立 → 必定闪避（`MapObject.cs:460-507` 的
    // `MAC` 分支）。
    let monster = harness.spawn(test_monster(100, 0, 10), Point::new(22, 20));
    harness.learn(player, Spell::FireBall);
    harness.ready_all(player);
    harness.drain(player);

    harness
        .world
        .cast_magic(
            player,
            Spell::FireBall,
            MirDirection::Right,
            monster,
            Point::new(22, 20),
        )
        .expect("施法成功");
    harness.elapse(600);
    assert_eq!(harness.hp(monster), 100, "闪避时不该扣血");

    let packets = harness.packets(player);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::DamageIndicator(indicator) if indicator.object_id == monster
                && indicator.type_ == DamageType::Miss
        )),
        "应广播 Miss 指示，实际 {packets:?}"
    );
}

#[test]
fn heal_restores_hp_in_chunks_with_health_changed() {
    let mut harness = Harness::new(11);
    let player = harness.add_player(1, MirClass::Taoist, 1, Point::new(20, 20));
    harness.learn(player, Spell::Healing);
    if let Some(p) = harness.world.player_mut(player) {
        // 道士 1 级基础道术是 0；给一组固定值让治疗量可算：
        // `health = magic.GetDamage(GetAttackPower(MinSC, MaxSC) * 2) + Level`
        // → base = 10 * 2 = 20，威力 6（模板 MPower 10 / DefPower 4、0 级），倍率 1.0
        // → 26 + 1 = 27。
        p.stats.min_sc = 10;
        p.stats.max_sc = 10;
        p.hp = 5;
    }
    harness.ready_all(player);
    harness.drain(player);

    let report = harness
        .world
        .cast_magic(
            player,
            Spell::Healing,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("治疗自己");
    assert_eq!(report.target_id, player, "没有目标时默认治疗自己");
    assert_eq!(report.heal, Some(27));

    // 500ms 后治疗量记到 `HealAmount`，但还没生效（`HumanObject.cs:4361` 的延迟 500ms）。
    harness.elapse(500);
    assert_eq!(harness.player_hp(player), 5);
    assert_eq!(harness.world.player(player).unwrap().magic.heal_amount, 27);

    // 下一个 tick 放出第一段：`incHeal = Level/10 + HealAmount/10 = 0 + 2`，
    // 每段 `5 + 2 = 7`（`HumanObject.cs:602-617`）。
    harness.elapse(1);
    assert_eq!(harness.player_hp(player), 12);
    let packets = harness.packets(player);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::HealthChanged(health) if health.hp == 12
        )),
        "治疗应发 S.HealthChanged，实际 {packets:?}"
    );
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::DamageIndicator(indicator) if indicator.object_id == player
                && indicator.damage == 7
        )),
        "治疗应广播 +7 的飘字，实际 {packets:?}"
    );

    // 再过 600ms 分段又放 7 点（6 点就满血：道士 1 级 16 血）。
    // 判定是严格大于（`Envir.Time > HealTime`，`HumanObject.cs:602`），所以多等 1ms。
    harness.elapse(601);
    assert_eq!(harness.player_hp(player), 16, "应回满");
    assert_eq!(
        harness.world.player(player).unwrap().magic.heal_amount,
        0,
        "满血后剩余治疗量清零"
    );
}

#[test]
fn mass_healing_heals_everyone_in_the_area() {
    let mut harness = Harness::new(12);
    let player = harness.add_player(1, MirClass::Taoist, 10, Point::new(20, 20));
    let friend = harness.add_player(2, MirClass::Warrior, 1, Point::new(21, 21));
    // 区域外的玩家（3×3 之外）。
    let far = harness.add_player(3, MirClass::Warrior, 1, Point::new(30, 30));
    harness.learn(player, Spell::MassHealing);
    if let Some(p) = harness.world.player_mut(player) {
        p.stats.min_sc = 10;
        p.stats.max_sc = 10;
        p.hp = 5;
    }
    if let Some(p) = harness.world.player_mut(friend) {
        p.hp = 1;
    }
    if let Some(p) = harness.world.player_mut(far) {
        p.hp = 1;
    }
    harness.ready_all(player);

    harness
        .world
        .cast_magic(
            player,
            Spell::MassHealing,
            MirDirection::Down,
            0,
            Point::new(20, 20),
        )
        .expect("群体治疗");
    harness.elapse(500);
    harness.elapse(1);
    // 治疗量 = `GetDamage(GetAttackPower(10,10))` = 10（威力 0、倍率 1.0）；
    // 第一段 = `5 + Level/10 + HealAmount/10`：
    //   * 10 级玩家：5 + 1 + 1 = 7
    //   * 1 级战士：5 + 0 + 1 = 6
    assert_eq!(harness.player_hp(player), 12);
    assert_eq!(harness.player_hp(friend), 7);
    assert_eq!(harness.player_hp(far), 1, "区域外的人不该被治疗");
}

// ---------------------------------------------------------------------------
// 4. 技能经验与升级
// ---------------------------------------------------------------------------

#[test]
fn killing_a_monster_grants_magic_experience() {
    let mut harness = Harness::new(13);
    let player = harness.add_player(1, MirClass::Wizard, 1, Point::new(20, 20));
    // 5 血、魔法防御 0：6 点伤害一击必杀。
    let monster = harness.spawn(test_monster(5, 0, 0), Point::new(22, 20));
    harness.learn(player, Spell::FireBall);
    harness.ready_all(player);
    harness.drain(player);

    harness
        .world
        .cast_magic(
            player,
            Spell::FireBall,
            MirDirection::Right,
            monster,
            Point::new(22, 20),
        )
        .expect("施法成功");
    harness.elapse(600);

    assert!(harness.world.monster(monster).is_none_or(|m| m.dead));
    let magic = harness
        .world
        .player_magic(player, Spell::FireBall)
        .copied()
        .expect("技能还在");
    assert!(magic.experience > 0, "击杀应给技能经验，实际 {magic:?}");

    let packets = harness.packets(player);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::MagicLeveled(leveled) if leveled.spell == Spell::FireBall
                && leveled.experience > 0
        )),
        "应收到带经验的 S.MagicLeveled，实际 {packets:?}"
    );
}

#[test]
fn magic_level_up_sends_magic_leveled_and_magic_delay() {
    let mut harness = Harness::new(14);
    let player = harness.add_player(1, MirClass::Wizard, 1, Point::new(20, 20));
    let monster = harness.spawn(test_monster(10_000, 0, 0), Point::new(22, 20));
    harness.learn(player, Spell::FireBall);
    harness.drain(player);

    // `need1 = 10`，每次施法随机 1..=3 点修炼值（`HumanObject.cs:6905`）；
    // 10 次施法最少也有 10 点，必然升级。
    for _ in 0..10 {
        harness.ready_all(player);
        harness
            .world
            .cast_magic(
                player,
                Spell::FireBall,
                MirDirection::Right,
                monster,
                Point::new(22, 20),
            )
            .expect("施法成功");
        harness.elapse(600);
    }

    let magic = harness
        .world
        .player_magic(player, Spell::FireBall)
        .copied()
        .expect("技能还在");
    assert_eq!(magic.level, 1, "10 点修炼值应升到 1 级");
    // 升级后角色等级 1 < `Level2 = 2`，后续施法不再涨经验（`HumanObject.cs:6945-6946`）。
    assert!(magic.experience < 3, "升级时扣掉本级需求");

    let packets = harness.packets(player);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::MagicLeveled(leveled) if leveled.spell == Spell::FireBall
                && leveled.level == 1
        )),
        "升级应发 S.MagicLeveled，实际 {packets:?}"
    );
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::MagicDelay(delay) if delay.spell == Spell::FireBall
                && delay.delay == 1_700 // 1800 - 1 * 100
        )),
        "升级应发 S.MagicDelay（缩短后的冷却），实际 {packets:?}"
    );
}

// ---------------------------------------------------------------------------
// 5. 区域技能
// ---------------------------------------------------------------------------

#[test]
fn hellfire_burns_a_line_of_cells() {
    let mut harness = Harness::new(15);
    let player = harness.add_player(1, MirClass::Wizard, 1, Point::new(20, 20));
    let near = harness.spawn(test_monster(100, 0, 0), Point::new(21, 20));
    let mid = harness.spawn(test_monster(100, 0, 0), Point::new(22, 20));
    let far = harness.spawn(test_monster(100, 0, 0), Point::new(24, 20));
    harness.learn(player, Spell::HellFire);
    harness.ready_all(player);

    harness
        .world
        .cast_magic(
            player,
            Spell::HellFire,
            MirDirection::Right,
            0,
            Point::new(0, 0),
        )
        .expect("施法成功");

    // 第一格 500ms 后，之后每 100ms 推进一格（`Map.cs:883-887`）。
    harness.elapse(500);
    assert_eq!(harness.hp(near), 94, "第一格应被烧到");
    assert_eq!(harness.hp(mid), 100, "第二格还没轮到");
    harness.elapse(100);
    assert_eq!(harness.hp(mid), 94);
    // 第三格（23,20）没有怪；第四格（24,20）在射程内，应被烧到。
    // 每一步都是「结算当前格 → 排出 100ms 后的下一格」，所以要一格一格推进。
    harness.elapse(100);
    assert_eq!(harness.hp(far), 100, "第三格没有怪，第四格还没轮到");
    harness.elapse(100);
    assert_eq!(harness.hp(far), 94, "第 4 格应被烧到");
}

#[test]
fn firebang_hits_the_whole_3x3_area() {
    let mut harness = Harness::new(16);
    let player = harness.add_player(1, MirClass::Wizard, 1, Point::new(20, 20));
    let a = harness.spawn(test_monster(100, 0, 0), Point::new(21, 20));
    let b = harness.spawn(test_monster(100, 0, 0), Point::new(22, 21));
    let c = harness.spawn(test_monster(100, 0, 0), Point::new(23, 22));
    // 区域外（距离落点 2 格以上）。
    let outside = harness.spawn(test_monster(100, 0, 0), Point::new(25, 24));
    harness.learn(player, Spell::FireBang);
    harness.ready_all(player);

    harness
        .world
        .cast_magic(
            player,
            Spell::FireBang,
            MirDirection::DownRight,
            0,
            Point::new(22, 21),
        )
        .expect("施法成功");
    harness.elapse(500);
    for target in [a, b, c] {
        assert_eq!(harness.hp(target), 94, "3×3 内的怪都应被烧到");
    }
    assert_eq!(harness.hp(outside), 100, "区域外的怪不该受伤");
}

// ---------------------------------------------------------------------------
// 6. 其他协议/数据行为
// ---------------------------------------------------------------------------

#[test]
fn unsupported_spells_are_registered_but_apply_nothing() {
    let mut harness = Harness::new(17);
    let player = harness.add_player(1, MirClass::Warrior, 1, Point::new(20, 20));
    // `Rage` 仍在 `REGISTERED_UNSUPPORTED` 里（`Haste` 已经实现，见 `tests/use_items.rs`）。
    harness.learn(player, Spell::Rage);
    harness.ready_all(player);
    let mp = harness.world.player(player).unwrap().mp;

    let error = harness
        .world
        .cast_magic(player, Spell::Rage, MirDirection::Down, 0, Point::new(0, 0))
        .expect_err("增益类技能尚未实现");
    assert_eq!(error, MagicError::UnsupportedSpell(Spell::Rage));
    assert_eq!(harness.world.player(player).unwrap().mp, mp, "不该扣蓝");

    // 但客户端仍能拿到模板（技能栏里能看到）。
    let client = harness.world.client_magics(player);
    assert_eq!(client.len(), 1);
    assert_eq!(client[0].spell, Spell::Rage);
}

#[test]
fn magic_key_rebinding_clears_the_previous_owner() {
    let mut harness = Harness::new(18);
    let player = harness.add_player(1, MirClass::Wizard, 1, Point::new(20, 20));
    harness.learn(player, Spell::FireBall);
    harness.learn(player, Spell::Healing);

    assert!(harness.world.set_magic_key(player, Spell::FireBall, 3));
    assert_eq!(
        harness
            .world
            .player_magic(player, Spell::FireBall)
            .unwrap()
            .key,
        3
    );

    // 把同一个键绑给另一个技能：旧占用者的键被清零（`MirConnection.cs:1554-1565`）。
    assert!(harness.world.set_magic_key(player, Spell::Healing, 3));
    assert_eq!(
        harness
            .world
            .player_magic(player, Spell::FireBall)
            .unwrap()
            .key,
        0
    );
    assert_eq!(
        harness
            .world
            .player_magic(player, Spell::Healing)
            .unwrap()
            .key,
        3
    );
    assert!(
        !harness.world.set_magic_key(player, Spell::ThunderBolt, 1),
        "没学过的技能不能绑定快捷键"
    );
}

#[test]
fn magic_blob_roundtrips_through_crystal_layout() {
    let magics = vec![
        UserMagic {
            spell: Spell::FireBall,
            level: 2,
            key: 4,
            experience: 321,
        },
        UserMagic {
            spell: Spell::Healing,
            level: 3,
            key: 0,
            experience: 0,
        },
        UserMagic {
            // 未知/未支持的技能也要原样往返（Crystal 读档时会丢弃，本作保留）。
            spell: Spell::Fencing,
            level: 1,
            key: 9,
            experience: 7,
        },
    ];
    let bytes = encode_magics(&magics);
    assert_eq!(bytes.len(), 4 + magics.len() * magic::MAGIC_RECORD_LEN);
    let decoded = decode_magics(&bytes).expect("解码应当成功");
    assert_eq!(decoded, magics);
    assert!(decode_magics(&[]).expect("空块").is_empty());
}

// ---------------------------------------------------------------------------
// 7. 真实数据冒烟
// ---------------------------------------------------------------------------

#[test]
fn real_database_exposes_magic_infos() {
    let Some(data) = GameData::discover().ok() else {
        eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
        return;
    };
    assert!(
        data.magic_count() >= 100,
        "样本里的技能模板应上百，实际 {}",
        data.magic_count()
    );
    let fireball = data.magic(Spell::FireBall).expect("样本里一定有火球术");
    assert_ne!(fireball.delay_base, 0, "火球的冷却不该是 0");
    assert!(
        fireball.m_power_base != 0 || fireball.power_base != 0,
        "火球的威力不该是 0：{fireball:?}"
    );
    assert_ne!(fireball.range, 0, "火球应有射程");
    // 名字能按「忽略空格/大小写」查到同一条。
    assert_eq!(
        data.magic_by_name(&fireball.name).map(|info| info.spell),
        Some(fireball.spell)
    );

    // 书本的 `Shape` 就是 `Spell`：至少有一本书能映射到已知技能
    // （`PlayerObject.cs:6078` 的 `(Spell)item.Info.Shape`）。
    let mapped = data.db.items.iter().find(|item| {
        item.item_type == ItemType::Book
            && u8::try_from(item.shape)
                .ok()
                .and_then(Spell::from_value)
                .and_then(|spell| data.magic(spell))
                .is_some()
    });
    assert!(
        mapped.is_some(),
        "样本里应至少有一本技能书能映射到 MagicInfo"
    );
}
