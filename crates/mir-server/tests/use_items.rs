//! 消耗品使用（`C.UseItem`）与增益容器的端到端测试。
//!
//! 覆盖 `docs/world/items.md` §「使用物品」与 `docs/world/buffs.md` 里的每一条可观察行为：
//!
//! * 普通药水（`Shape == 0`）→ 药水池 → 每 200ms 放 `5 + Level/10`；
//! * 太阳水（`Shape == 1`）→ 立刻回血/回蓝；
//! * 职业/等级/死亡三条拒绝路径（失败包 + 不消耗）；
//! * 复活卷轴是唯一「死亡时可用」的物品；
//! * 技能书路径保持可用；
//! * 9 个增益技能：护盾加防、攻速、魔法盾减伤、可见 buff 广播、到期回血重发；
//! * `C.DropGold` 落地 + 扣钱；
//! * 真实 Jev 数据里能找到药水模板与护盾技能模板。
//!
//! 所有用例都是确定性的：固定随机种子、`advance_clock` 快进、不 `sleep`、
//! 怪物定时器设成「永不思考」（`operate_time = -1` 只在明确要它出手时才唤醒）。

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{
    ItemType, MirClass, MirDirection, MirGender, Monster as MonsterImage, RequiredClass,
    RequiredType, Spell, Stat,
};
use mir_core::i18n::{format_text, text};
use mir_core::{Language, TextKey};
use mir_db::{MagicInfo, MirDb};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ItemInfo, Stats};
use mir_server::MonsterKind;
use mir_server::buffs::BuffKind;
use mir_server::combat::{self, CombatStats};
use mir_server::gamedata::GameData;
use mir_server::items::{ItemError, UseItemReport};
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
        world.set_game_data(game_data());
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
            attack_mode: mir_core::enums::AttackMode::Peace,
            pk_points: 0,
            brown_time: 0,
            pk_point_time: 0,
            last_hitter: None,
            last_hit_time: 0,
            name_colour: mir_core::Color(-1),
            visible_monsters: BTreeSet::new(),
            visible_npcs: BTreeSet::new(),
            movement_time: 0,
            inventory: mir_server::items::new_inventory(),
            equipment: mir_server::items::new_equipment(),
            storage: mir_server::storage::new_storage(),
            magics: Vec::new(),
            magic: mir_server::MagicState::default(),
            gold: 0,
            credit: 0,
            visible_items: BTreeSet::new(),
            quests: Vec::new(),
            completed_quests: BTreeSet::new(),
            mail: mir_server::MailState::default(),
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
            trade: mir_server::TradeState::default(),
            trade_invitation: None,
            next_trade_time: 0,
            allow_trade: true,
            bind_map_index: 0,
            bind_point: mir_core::Point::default(),
            sender,
        });
        // 关掉自然回血，免得 10 秒一次的 +3% 混进药水/治疗断言。
        if let Some(player) = self.world.player_mut(object_id) {
            player.regen_time = i64::MAX;
        }
        self.receivers.insert(object_id, receiver);
        object_id
    }

    /// 往背包里塞一件物品（`count` 件），返回 UUID。
    fn give(&mut self, object_id: u32, index: i32, count: u16) -> u64 {
        let info = self.world.item_info(index).expect("物品模板").clone();
        let mut item = self.world.create_fresh_item(&info);
        item.count = count;
        let unique_id = item.unique_id;
        assert!(
            self.world.add_item_to_inventory(object_id, item),
            "背包应该有空位"
        );
        unique_id
    }

    /// 穿上一件装备（绕过 `C.EquipItem` 的 UI 路径，直接放进槽位并刷新属性）。
    fn equip(&mut self, object_id: u32, slot: usize, index: i32) {
        let info = self.world.item_info(index).expect("物品模板").clone();
        let item = self.world.create_fresh_item(&info);
        if let Some(player) = self.world.player_mut(object_id) {
            player.equipment[slot] = Some(item);
        }
        self.world.refresh_stats(object_id);
    }

    fn learn(&mut self, object_id: u32, spell: Spell) {
        self.world
            .learn_magic(object_id, spell)
            .expect("学习应当成功");
    }

    /// 清掉动作/技能计时与冷却，并回满魔法（等价于「等了足够久」）。
    fn ready(&mut self, object_id: u32) {
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

    fn hp(&self, object_id: u32) -> i32 {
        self.world.player(object_id).expect("玩家还在").hp
    }

    fn mp(&self, object_id: u32) -> i32 {
        self.world.player(object_id).expect("玩家还在").mp
    }

    fn max_hp(&self, object_id: u32) -> i32 {
        self.world.player(object_id).expect("玩家还在").stats.hp
    }

    fn gold(&self, object_id: u32) -> i64 {
        self.world.player(object_id).expect("玩家还在").gold
    }

    fn has_buff(&self, object_id: u32, kind: BuffKind) -> bool {
        self.world.has_buff(object_id, kind)
    }

    /// 刷一只「立刻唤醒」的怪（所有定时器归零，下一次 tick 就会思考）。
    fn spawn_awake(&mut self, kind: MonsterKind, position: Point) -> u32 {
        let object_id = self.world.spawn_monster(MAP, kind, position);
        if let Some(monster) = self.world.monster_mut(object_id) {
            monster.operate_time = -1;
            monster.action_time = -1;
            monster.move_time = -1;
            monster.attack_time = -1;
            monster.search_time = -1;
            monster.roam_time = -1;
            monster.alone_time = -1;
            monster.regen_time = i64::MAX;
        }
        object_id
    }
}

/// 物品模板：`index`/`name`/类型/形状 + 属性表。
fn item(
    index: i32,
    name: &str,
    item_type: ItemType,
    shape: i16,
    stats: &[(Stat, i32)],
) -> ItemInfo {
    let mut table = Stats::default();
    for &(stat, value) in stats {
        table.set(stat, value);
    }
    ItemInfo {
        index,
        name: name.to_owned(),
        item_type,
        shape,
        weight: 1,
        stack_size: 5,
        durability: 2,
        stats: table,
        ..ItemInfo::default()
    }
}

/// 技能模板：耗蓝 5、冷却 1800ms、`MPower/DefPower` 固定（不抽签），便于断言。
fn magic_info(spell: Spell, range: u8) -> MagicInfo {
    MagicInfo {
        name: format!("{spell:?}"),
        spell: spell.value(),
        base_cost: 5,
        level_cost: 0,
        icon: 10,
        level1: 1,
        level2: 2,
        level3: 3,
        need1: 10,
        need2: 20,
        need3: 30,
        delay_base: 1_800,
        delay_reduction: 0,
        power_base: 4,
        power_bonus: 0,
        m_power_base: 10,
        m_power_bonus: 0,
        range,
        multiplier_base: 1.0,
        multiplier_bonus: 0.0,
    }
}

const POTION_HP: i32 = 1; // 金创药(小量)：Shape 0，HP +30
const POTION_MP: i32 = 2; // 魔法药(小量)：Shape 0，MP +20
const SUN_POTION: i32 = 3; // 太阳水：Shape 1，HP +50
const BUFF_POTION: i32 = 4; // 战士药水：Shape 3，MaxDC +3 / HP +10（2 分钟）
const WIZARD_ONLY_POTION: i32 = 5; // 法师专属红药
const HIGH_LEVEL_POTION: i32 = 6; // 需要 10 级的红药
const RESURRECTION_SCROLL: i32 = 7; // 复活卷轴：Scroll Shape 6
const EXP_POTION: i32 = 8; // 经验药水：Shape 4
const BOOK_FIREBALL: i32 = 9; // 火球术技能书
const TELEPORT_SCROLL: i32 = 10; // 传送卷：Scroll Shape 0（未实现）
const ARMOUR: i32 = 11; // 布衣：MaxAC 10 / MinAC 4

fn game_data() -> Arc<GameData> {
    let db = MirDb {
        magics: vec![
            magic_info(Spell::ProtectionField, 0),
            magic_info(Spell::MagicShield, 0),
            magic_info(Spell::SoulShield, 6),
            magic_info(Spell::BlessedArmour, 6),
            magic_info(Spell::EnergyShield, 6),
            magic_info(Spell::Haste, 0),
            magic_info(Spell::LightBody, 0),
            magic_info(Spell::Fury, 0),
            magic_info(Spell::MagicBooster, 0),
            // 技能书用得到（`use_book` 会查模板）。
            magic_info(Spell::FireBall, 8),
        ],
        items: vec![
            item(
                POTION_HP,
                "金创药(小量)",
                ItemType::Potion,
                0,
                &[(Stat::HP, 30)],
            ),
            item(
                POTION_MP,
                "魔法药(小量)",
                ItemType::Potion,
                0,
                &[(Stat::MP, 20)],
            ),
            item(SUN_POTION, "太阳水", ItemType::Potion, 1, &[(Stat::HP, 50)]),
            item(
                BUFF_POTION,
                "战士药水",
                ItemType::Potion,
                3,
                &[(Stat::MaxDC, 3), (Stat::HP, 10)],
            ),
            ItemInfo {
                required_class: RequiredClass::Wizard,
                ..item(
                    WIZARD_ONLY_POTION,
                    "法师红药",
                    ItemType::Potion,
                    0,
                    &[(Stat::HP, 30)],
                )
            },
            ItemInfo {
                required_type: RequiredType::Level,
                required_amount: 10,
                ..item(
                    HIGH_LEVEL_POTION,
                    "高级金创药",
                    ItemType::Potion,
                    0,
                    &[(Stat::HP, 30)],
                )
            },
            item(RESURRECTION_SCROLL, "复活卷轴", ItemType::Scroll, 6, &[]),
            item(
                EXP_POTION,
                "经验药水",
                ItemType::Potion,
                4,
                &[(Stat::Luck, 5)],
            ),
            item(
                BOOK_FIREBALL,
                "火球术",
                ItemType::Book,
                i16::from(Spell::FireBall.value()),
                &[],
            ),
            item(TELEPORT_SCROLL, "回城卷", ItemType::Scroll, 0, &[]),
            item(
                ARMOUR,
                "布衣(男)",
                ItemType::Armour,
                1,
                &[(Stat::MaxAC, 10), (Stat::MinAC, 4)],
            ),
        ],
        ..MirDb::default()
    };
    Arc::new(GameData::from_db(db, None))
}

/// 攻击力固定 10、必中、攻击间隔 2500ms 的测试怪。
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
            hp: 100,
            min_dc: 10,
            max_dc: 10,
            accuracy: 100,
            agility: 0,
            ..CombatStats::default()
        },
    }
}

// ---------------------------------------------------------------------------
// 1. 药水（Shape 0 / 1）
// ---------------------------------------------------------------------------

#[test]
fn normal_potion_fills_the_pot_pool_and_heals_over_time() {
    let mut harness = Harness::new(1);
    let player = harness.add_player(1, MirClass::Warrior, 1, Point::new(20, 20));
    let potion = harness.give(player, POTION_HP, 3);
    // 掉到 20 点血（1 级战士满血 15？见下：先按上限夹一下）。
    let max_hp = harness.max_hp(player);
    if let Some(player_state) = harness.world.player_mut(player) {
        player_state.hp = max_hp - 10;
    }
    harness.drain(player);

    let report = harness.world.use_item(player, potion).expect("喝药成功");
    assert_eq!(report.item_index, POTION_HP);
    // `PotHealthAmount = min(ushort.MaxValue, PotHealthAmount + Stats[HP])`（`PlayerObject.cs:5832`）。
    assert_eq!(report.pot_health, 30, "30 点先攒进药水池");
    assert_eq!(report.hp, 0, "普通药水不是立刻回血");
    assert_eq!(harness.hp(player), max_hp - 10, "喝下瞬间血量不变");
    assert_eq!(
        harness
            .world
            .find_inventory_item(player, potion)
            .map(|(_, item)| item.count),
        Some(2),
        "堆叠应减 1"
    );

    let packets = harness.packets(player);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::UseItem(use_item) if use_item.unique_id == potion && use_item.success
        )),
        "应收到成功的 S.UseItem，实际 {packets:?}"
    );

    // 200ms 后放一批：`PerTickRegen = 5 + Level / 10`（`HumanObject.cs:577`），1 级 = 5。
    let before = harness.hp(player);
    harness.elapse(200);
    assert_eq!(harness.hp(player), before + 5, "每 200ms 回一批");
    assert!(
        harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::HealthChanged(changed) if changed.hp == before + 5)),
        "回血要发 S.HealthChanged"
    );
}

#[test]
fn normal_potion_at_full_health_is_rejected_without_consuming() {
    let mut harness = Harness::new(2);
    let player = harness.add_player(1, MirClass::Warrior, 1, Point::new(20, 20));
    let potion = harness.give(player, POTION_HP, 1);
    harness.drain(player);
    assert_eq!(harness.hp(player), harness.max_hp(player), "起始是满血");

    let error = harness
        .world
        .use_item(player, potion)
        .expect_err("满血应当被拒绝");
    assert_eq!(error, ItemError::PotionNoEffect);
    assert!(
        harness.world.find_inventory_item(player, potion).is_some(),
        "被拒绝的药水不该被消耗"
    );
    let packets = harness.packets(player);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::UseItem(use_item) if use_item.unique_id == potion && !use_item.success
        )),
        "失败要发 S.UseItem{{Success=false}}"
    );
    let expected = text(Language::ZhCn, TextKey::ItemPotionNoEffectFull);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Chat(chat) if chat.message == expected)),
        "应提示满血（按中文语言包取值 {expected}）"
    );
}

#[test]
fn mana_potion_restores_mp_through_the_pot_pool() {
    let mut harness = Harness::new(3);
    let player = harness.add_player(1, MirClass::Wizard, 1, Point::new(20, 20));
    let potion = harness.give(player, POTION_MP, 2);
    let max_mp = harness.world.player(player).unwrap().stats.mp;
    if let Some(player_state) = harness.world.player_mut(player) {
        player_state.mp = 0;
    }
    harness.drain(player);

    let report = harness.world.use_item(player, potion).expect("喝蓝药");
    assert_eq!(report.pot_mana, 20);
    assert_eq!(harness.mp(player), 0, "蓝药也走药水池，不是立即生效");
    harness.elapse(200);
    assert_eq!(harness.mp(player), 5, "每 200ms 回 5 点魔法");
    assert!(max_mp >= 5);
}

#[test]
fn sun_potion_heals_the_flat_amount_immediately() {
    let mut harness = Harness::new(4);
    let player = harness.add_player(1, MirClass::Warrior, 20, Point::new(20, 20));
    let potion = harness.give(player, SUN_POTION, 1);
    let max_hp = harness.max_hp(player);
    // 扣到上限以下 60 点，这样 +50 不会被上限截断。
    if let Some(player_state) = harness.world.player_mut(player) {
        player_state.hp = max_hp - 60;
    }
    harness.drain(player);

    let report = harness.world.use_item(player, potion).expect("喝太阳水");
    // Crystal 的太阳水是**固定值**：`ChangeHP(item.Info.Stats[Stat.HP])`（`PlayerObject.cs:5836`），
    // 不是百分比——模板里没有百分比字段，`Stats` 字典就是全部数值来源。
    assert_eq!(report.hp, 50);
    assert_eq!(harness.hp(player), max_hp - 10);
    assert!(
        harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::HealthChanged(changed) if changed.hp == max_hp - 10)),
        "立刻回血要发 S.HealthChanged"
    );
}

// ---------------------------------------------------------------------------
// 2. 拒绝路径
// ---------------------------------------------------------------------------

#[test]
fn potion_class_requirement_is_enforced() {
    let mut harness = Harness::new(5);
    let player = harness.add_player(1, MirClass::Warrior, 5, Point::new(20, 20));
    let potion = harness.give(player, WIZARD_ONLY_POTION, 1);
    harness.drain(player);

    let error = harness.world.use_item(player, potion).unwrap_err();
    assert_eq!(
        error,
        ItemError::WrongClass {
            required: RequiredClass::Wizard,
            class: MirClass::Warrior,
        }
    );
    assert!(
        harness.world.find_inventory_item(player, potion).is_some(),
        "不该消耗"
    );
    let packets = harness.packets(player);
    let expected = text(Language::ZhCn, TextKey::ItemWrongClass);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Chat(chat) if chat.message == expected)),
        "应提示职业不符（按中文语言包取值 {expected}）"
    );
    assert!(packets.iter().any(|packet| matches!(
        packet,
        ServerPacket::UseItem(use_item) if !use_item.success
    )));
}

#[test]
fn potion_level_requirement_is_enforced() {
    let mut harness = Harness::new(6);
    let player = harness.add_player(1, MirClass::Warrior, 5, Point::new(20, 20));
    let potion = harness.give(player, HIGH_LEVEL_POTION, 1);
    harness.drain(player);

    let error = harness.world.use_item(player, potion).unwrap_err();
    assert_eq!(
        error,
        ItemError::LowLevel {
            required: 10,
            level: 5
        }
    );
    let expected = format_text(
        Language::ZhCn,
        TextKey::ItemLevelTooLow,
        &[("required", "10")],
    );
    assert!(
        harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Chat(chat) if chat.message == expected)),
        "应提示等级不够（按中文语言包取值 {expected}）"
    );
}

#[test]
fn dead_player_cannot_use_a_potion_but_a_resurrection_scroll_revives() {
    let mut harness = Harness::new(7);
    let player = harness.add_player(1, MirClass::Warrior, 1, Point::new(20, 20));
    let potion = harness.give(player, POTION_HP, 1);
    let scroll = harness.give(player, RESURRECTION_SCROLL, 1);
    // 让玩家死掉：直接置位（等价于 `World::player_died` 的可观察结果）。
    if let Some(player_state) = harness.world.player_mut(player) {
        player_state.dead = true;
        player_state.hp = 0;
        player_state.revive_time = i64::MAX; // 关掉本作的自动复活
    }
    harness.drain(player);

    // `if (Dead && !(Scroll && Shape == 6)) { Enqueue(p); return; }`（`PlayerObject.cs:5820-5824`）。
    let error = harness.world.use_item(player, potion).unwrap_err();
    assert_eq!(error, ItemError::Dead { object_id: player });
    assert!(
        harness.world.find_inventory_item(player, potion).is_some(),
        "死亡时药水不该被消耗"
    );

    // 复活卷轴：`MP = Stats[MP]; Revive(MaxHealth, true);`（`:5986-5987`）。
    harness.world.use_item(player, scroll).expect("复活卷轴");
    assert!(!harness.world.player(player).unwrap().dead, "应当已复活");
    assert_eq!(harness.hp(player), harness.max_hp(player), "复活后满血");
    assert!(
        harness.world.find_inventory_item(player, scroll).is_none(),
        "卷轴应被消耗"
    );
    assert!(
        harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Revived(_))),
        "应收到 S.Revived"
    );
}

#[test]
fn unknown_item_shapes_and_scrolls_are_rejected_without_consuming() {
    let mut harness = Harness::new(8);
    let player = harness.add_player(1, MirClass::Warrior, 5, Point::new(20, 20));
    let scroll = harness.give(player, TELEPORT_SCROLL, 1);
    harness.drain(player);

    // 简化：传送卷需要传送子系统，本作拒绝并提示（Crystal 会把不认识的花色吃掉）。
    let error = harness.world.use_item(player, scroll).unwrap_err();
    assert!(matches!(error, ItemError::UnsupportedItem { .. }));
    assert!(
        harness.world.find_inventory_item(player, scroll).is_some(),
        "不该消耗"
    );
    let expected = format_text(
        Language::ZhCn,
        TextKey::ItemEffectNotImplemented,
        &[("item", "回城卷")],
    );
    assert!(
        harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Chat(chat) if chat.message == expected)),
        "应提示效果尚未实现（按中文语言包取值 {expected}）"
    );
}

// ---------------------------------------------------------------------------
// 3. 技能书路径仍然可用
// ---------------------------------------------------------------------------

#[test]
fn using_a_book_still_learns_the_spell() {
    let mut harness = Harness::new(9);
    let player = harness.add_player(1, MirClass::Wizard, 5, Point::new(20, 20));
    let book = harness.give(player, BOOK_FIREBALL, 1);
    harness.drain(player);

    let report: UseItemReport = harness.world.use_item(player, book).expect("学会火球");
    assert_eq!(
        report.learned.map(|magic| magic.spell),
        Some(Spell::FireBall)
    );
    assert!(harness.world.find_inventory_item(player, book).is_none());
    let packets = harness.packets(player);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::NewMagic(new_magic) if new_magic.magic.spell == Spell::FireBall))
    );
    assert!(packets.iter().any(|packet| matches!(
        packet,
        ServerPacket::UseItem(use_item) if use_item.unique_id == book && use_item.success
    )));
}

// ---------------------------------------------------------------------------
// 4. 增益技能
// ---------------------------------------------------------------------------

#[test]
fn protection_field_raises_defence_until_it_expires() {
    let mut harness = Harness::new(10);
    let player = harness.add_player(1, MirClass::Warrior, 5, Point::new(20, 20));
    harness.equip(player, 1, ARMOUR);
    let base_ac = harness.world.player(player).unwrap().stats.max_ac;
    assert_eq!(base_ac, 10, "布衣给 10 点 MaxAC");

    harness.learn(player, Spell::ProtectionField);
    harness.ready(player);
    harness.drain(player);
    let report = harness
        .world
        .cast_magic(
            player,
            Spell::ProtectionField,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施放护体");
    assert_eq!(report.buff, Some(BuffKind::ProtectionField));

    // `addValue = round(MaxAC * (0.2 + 0.03 * Level))`（`HumanObject.cs:4958`），0 级 = 2。
    let stats = harness.world.player(player).unwrap().stats;
    assert_eq!(stats.max_ac, base_ac + 2);
    assert_eq!(stats.min_ac, 4 + 2);
    assert!(harness.has_buff(player, BuffKind::ProtectionField));
    assert!(
        harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::AddBuff(add) if add.buff.buff_type == mir_core::enums::BuffType::ProtectionField)),
        "应收到 S.AddBuff"
    );

    // 0 级持续 45 秒（`:4957`）。到点后加成撤掉，并发 `S.RemoveBuff`。
    harness.elapse(45_001);
    assert!(!harness.has_buff(player, BuffKind::ProtectionField));
    assert_eq!(
        harness.world.player(player).unwrap().stats.max_ac,
        base_ac,
        "到期后防御回到装备值"
    );
    assert!(
        harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::RemoveBuff(remove) if remove.type_ == mir_core::enums::BuffType::ProtectionField))
    );
}

#[test]
fn haste_raises_attack_speed_and_shortens_the_attack_delay() {
    let mut harness = Harness::new(11);
    let player = harness.add_player(1, MirClass::Assassin, 20, Point::new(20, 20));
    harness.learn(player, Spell::Haste);
    harness.ready(player);
    let before = harness.world.player(player).unwrap().stats.attack_speed;
    let delay_before = combat::player_attack_delay_ms(20, before);

    harness
        .world
        .cast_magic(
            player,
            Spell::Haste,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施放疾风步");
    // `Haste` 是延迟 500ms 生效的（`HumanObject.cs:3544-3546`）。
    assert!(
        !harness.has_buff(player, BuffKind::Haste),
        "500ms 内还没生效"
    );
    harness.elapse(500);
    assert!(harness.has_buff(player, BuffKind::Haste));

    // `AttackSpeed = (magic.Level * 2) + 2`（`:6151`），0 级 = +2。
    let after = harness.world.player(player).unwrap().stats.attack_speed;
    assert_eq!(after, before + 2);
    let delay_after = combat::player_attack_delay_ms(20, after);
    assert_eq!(delay_before - delay_after, 120, "每点攻速省 60ms");
}

#[test]
fn magic_shield_reduces_incoming_monster_damage() {
    let mut harness = Harness::new(12);
    let player = harness.add_player(1, MirClass::Wizard, 20, Point::new(20, 20));
    harness.learn(player, Spell::MagicShield);
    harness.ready(player);
    let max_hp = harness.max_hp(player);
    if let Some(player_state) = harness.world.player_mut(player) {
        player_state.hp = max_hp;
    }

    harness
        .world
        .cast_magic(
            player,
            Spell::MagicShield,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施放魔法盾");
    harness.elapse(500); // `DelayedAction(..., Envir.Time + 500, ...)`（`:3591`）
    assert!(harness.has_buff(player, BuffKind::MagicShield));
    // `DamageReductionPercent = (magic.Level + 2) * 10`（`:6203`），0 级 = 20%。
    assert_eq!(
        harness
            .world
            .player(player)
            .unwrap()
            .stats
            .damage_reduction_percent,
        20
    );

    // 怪物攻击力固定 10、玩家 AC 0 → 原伤害 10；减伤 20% 后 8（`HumanObject.cs:7244-7248`）。
    harness.spawn_awake(test_monster(), Point::new(21, 20));
    harness.world.tick();
    harness.elapse(400);
    assert_eq!(
        harness.hp(player),
        max_hp - 8,
        "魔法盾把 10 点伤害压到 8 点"
    );
}

#[test]
fn visible_buffs_are_broadcast_and_magic_shield_sends_object_effects() {
    let mut harness = Harness::new(13);
    let player = harness.add_player(1, MirClass::Warrior, 5, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Warrior, 5, Point::new(21, 20));
    harness.learn(player, Spell::Fury);
    harness.learn(player, Spell::MagicShield);
    harness.ready(player);
    harness.drain(player);
    harness.drain(observer);

    // `Fury` 在 `BuffInfo.Load()` 里是 `Visible = true`（`BuffInfo.cs:23`），要被广播。
    harness
        .world
        .cast_magic(player, Spell::Fury, MirDirection::Down, 0, Point::new(0, 0))
        .expect("施放狂暴");
    harness.elapse(500);
    assert!(harness.has_buff(player, BuffKind::Fury));
    assert!(
        harness
            .packets(observer)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::AddBuff(add) if add.buff.buff_type == mir_core::enums::BuffType::Fury)),
        "可见 buff 要广播给周围玩家"
    );

    // `MagicShield` 不是可见 buff，但 `AddBuff` 之前会广播 `SpellEffect.MagicShieldUp`
    // （`HumanObject.cs:7466-7468`）。
    harness.ready(player);
    harness.drain(player);
    harness.drain(observer);
    harness
        .world
        .cast_magic(
            player,
            Spell::MagicShield,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施放魔法盾");
    harness.elapse(500);
    let observer_packets = harness.packets(observer);
    assert!(
        observer_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectEffect(effect)
                if effect.effect == mir_core::enums::SpellEffect::MagicShieldUp
        )),
        "周围玩家应收到 MagicShieldUp 特效，实际 {observer_packets:?}"
    );
    assert!(
        !observer_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::AddBuff(_))),
        "不可见的 buff 不该广播 S.AddBuff"
    );
}

#[test]
fn expiring_a_health_buff_resends_health_and_object_health() {
    let mut harness = Harness::new(14);
    let player = harness.add_player(1, MirClass::Warrior, 5, Point::new(20, 20));
    let potion = harness.give(player, BUFF_POTION, 1);
    let base_max_hp = harness.max_hp(player);
    harness.drain(player);

    // Shape 3：按属性表套 `HealthAid`（HP +10）与 `Impact`（MaxDC +3），时长 = 耐久分钟数。
    let report = harness.world.use_item(player, potion).expect("喝增益药水");
    assert_eq!(
        report.buffs,
        vec![BuffKind::Impact, BuffKind::HealthAid],
        "顺序照抄 PlayerObject.cs:5853-5866"
    );
    assert_eq!(harness.max_hp(player), base_max_hp + 10, "生命上限 +10");
    let packets = harness.packets(player);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::HealthChanged(_))),
        "上限变化要发 S.HealthChanged"
    );
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectHealth(_))),
        "上限变化要发 S.ObjectHealth"
    );

    // 模板耐久 2 分钟（`ItemInfo.Durability` 在这里当时长用，`PlayerObject.cs:5851`）。
    harness.elapse(121_000);
    assert!(!harness.has_buff(player, BuffKind::HealthAid));
    assert_eq!(harness.max_hp(player), base_max_hp, "到期后上限回退");
    let packets = harness.packets(player);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::HealthChanged(_))),
        "到期要重发血量"
    );
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectHealth(_))),
        "到期要重发 S.ObjectHealth"
    );
}

#[test]
fn magic_booster_raises_magic_power_and_mana_cost() {
    let mut harness = Harness::new(15);
    let player = harness.add_player(1, MirClass::Wizard, 20, Point::new(20, 20));
    harness.learn(player, Spell::MagicBooster);
    harness.ready(player);
    harness
        .world
        .cast_magic(
            player,
            Spell::MagicBooster,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施放魔力增幅");
    harness.elapse(500);

    // `bonus = 6 + Level * 6`，`ManaPenaltyPercent = 6 + Level`（`HumanObject.cs:4347`、`:6232-6235`）。
    // 20 级法师的基础 `MinMC/MaxMC` 都是 2（`Shared/BaseStats.cs` 的 `Stat/7` 曲线）。
    let base_mc = combat::player_base_stats(20, MirClass::Wizard).min_mc;
    let stats = harness.world.player(player).unwrap().stats;
    assert_eq!(stats.min_mc, base_mc + 6);
    assert_eq!(stats.max_mc, base_mc + 6);
    assert_eq!(stats.mana_penalty_percent, 6);
    // `MagicCost` 的惩罚：`cost + cost * 6 / 100`（`:3387-3390`）。
    let info = harness
        .world
        .magic_info(Spell::MagicBooster)
        .unwrap()
        .clone();
    let magic = *harness
        .world
        .player_magic(player, Spell::MagicBooster)
        .unwrap();
    assert_eq!(magic.cost(&info), 5);
    assert_eq!(magic.cost_with_penalty(&info, 6), 5);
    // 5 + 5*6/100 = 5（整数除法）；用 100 点基础耗蓝直观验证惩罚确实生效。
    assert_eq!(magic.cost_with_penalty(&info, 100), 10);
}

#[test]
fn area_shields_buff_nearby_players_with_level_scaled_defence() {
    let mut harness = Harness::new(16);
    let caster = harness.add_player(1, MirClass::Taoist, 14, Point::new(20, 20));
    let ally = harness.add_player(2, MirClass::Warrior, 14, Point::new(22, 20));
    harness.learn(caster, Spell::SoulShield);
    harness.ready(caster);
    harness.drain(caster);
    harness.drain(ally);

    harness
        .world
        .cast_magic(
            caster,
            Spell::SoulShield,
            MirDirection::Down,
            0,
            Point::new(22, 20),
        )
        .expect("施放幽灵盾");
    // 延迟 = 距离 × 50 + 500（`HumanObject.cs:4519`），距离 2 → 600ms。
    harness.elapse(600);

    // `MaxMAC = target.Level / 7 + 4`（`Map.cs:1082`），14 级 → 2 + 4 = 6。
    assert!(harness.has_buff(ally, BuffKind::SoulShield));
    let ally_stats = harness.world.player(ally).unwrap().stats;
    assert_eq!(ally_stats.max_mac, 6);
    assert!(
        harness
            .packets(ally)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::AddBuff(add) if add.buff.buff_type == mir_core::enums::BuffType::SoulShield))
    );
}

// ---------------------------------------------------------------------------
// 5. C.DropGold
// ---------------------------------------------------------------------------

#[test]
fn drop_gold_spawns_a_gold_pile_and_loses_gold() {
    let mut harness = Harness::new(17);
    let player = harness.add_player(1, MirClass::Warrior, 5, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Warrior, 5, Point::new(21, 20));
    if let Some(player_state) = harness.world.player_mut(player) {
        player_state.gold = 500;
    }
    harness.drain(player);
    harness.drain(observer);

    let ground = harness
        .world
        .drop_gold(player, 300)
        .expect("丢金币成功")
        .expect("应落地");
    let pile = harness.world.ground_item(ground).expect("金币堆还在");
    assert!(pile.is_gold(), "金币堆用 `GroundItem::gold` 表示");
    assert_eq!(pile.gold, 300);
    assert_eq!(pile.owner, None, "玩家丢的金币没有归属保护");
    assert_eq!(harness.gold(player), 200, "扣掉 300");
    assert!(
        pile.position.in_range(Point::new(20, 20), 5),
        "落在 `ob.Drop(5)` 的半径内"
    );

    let packets = harness.packets(player);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::LoseGold(lose) if lose.gold == 300)),
        "本人应收到 S.LoseGold"
    );
    assert!(
        harness
            .packets(observer)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectGold(gold) if gold.gold == 300)),
        "视野内玩家应看到 S.ObjectGold"
    );

    // 钱不够：静默失败（Crystal `if (Account.Gold < gold) return;`，`PlayerObject.cs:7475`）。
    let error = harness.world.drop_gold(player, 1000).unwrap_err();
    assert_eq!(
        error,
        ItemError::NotEnoughGold {
            requested: 1000,
            held: 200
        }
    );
    assert_eq!(harness.gold(player), 200);
}

// ---------------------------------------------------------------------------
// 6. 真实数据冒烟
// ---------------------------------------------------------------------------

#[test]
fn real_database_exposes_potions_and_shield_spells() {
    let Some(data) = GameData::discover().ok() else {
        eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
        return;
    };

    // 至少有一个能用（HP/MP 非 0）的普通药水模板。
    let potions: Vec<&ItemInfo> = data
        .db
        .items
        .iter()
        .filter(|item| {
            item.item_type == ItemType::Potion
                && (item.stats.get(Stat::HP) > 0 || item.stats.get(Stat::MP) > 0)
        })
        .collect();
    eprintln!("Jev 数据里带 HP/MP 的药水模板：{} 个", potions.len());
    let potion = potions.first().expect("Jev 数据里应有带 HP/MP 的药水");
    eprintln!(
        "样本：{}（HP {} / MP {}，堆叠 {}）",
        potion.name,
        potion.stats.get(Stat::HP),
        potion.stats.get(Stat::MP),
        potion.stack_size
    );
    assert!(potion.stack_size >= 1, "药水应可堆叠：{potion:?}");

    // 至少有一个本作已实现的护盾技能模板（`SpellKind::Buff`）。
    let shields = [
        Spell::ProtectionField,
        Spell::MagicShield,
        Spell::SoulShield,
        Spell::BlessedArmour,
        Spell::EnergyShield,
    ];
    let found: Vec<&MagicInfo> = shields
        .iter()
        .filter_map(|spell| {
            let info = data.magic(*spell)?;
            eprintln!(
                "护盾技能：{}（{spell:?}，耗蓝 {}+{}×L，射程 {}）",
                info.name, info.base_cost, info.level_cost, info.range
            );
            Some(info)
        })
        .collect();
    assert!(
        !found.is_empty(),
        "Jev 数据里应至少有一个护盾技能模板（ProtectionField/MagicShield/SoulShield/BlessedArmour/EnergyShield）"
    );
    for info in found {
        assert!(
            info.base_cost > 0 || info.level_cost > 0,
            "护盾技能 {} 的耗蓝不该是 0：{info:?}",
            info.name
        );
    }
}
