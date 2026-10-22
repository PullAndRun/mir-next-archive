//! 玩家对玩家（PK）的端到端测试。
//!
//! 全部确定性：
//!
//! * 世界随机数固定种子（[`World::seed_rng`]）；
//! * 时间用 [`World::advance_clock`] 快进（不 sleep）；
//! * 地图是内存里合成的 60×60 空地，`MapInfo` 里放一个安全区（中心 10,10、半径 10）；
//! * 掉落按固定种子断言——`World::refresh` 的抽签顺序与 Crystal 一致，见
//!   `docs/world/pk.md`。
//!
//! 每个断言都对应 Crystal 的一处行为，注释里写了出处（`文件.cs:行号`）。
//! 关键源码：`Server/MirObjects/PlayerObject.cs`（`IsAttackTarget` / `Die` /
//! `RedDeathDrop` / `GetNameColour`）、`Server/MirObjects/HumanObject.cs`
//! （`Attacked(HumanObject)` / `DeathDrop`）、`Server/MirObjects/MapObject.cs`
//! （`PKPoints` 衰减）、`Server/MirObjects/Monsters/Guard.cs`。

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{AttackMode, BindMode, ItemType, MirClass, MirDirection, MirGender};
use mir_db::{MapInfo, MirDb, SafeZoneInfo};
use mir_protocol::ServerPacket;
use mir_protocol::data::ItemInfo;
use mir_server::MonsterKind;
use mir_server::gamedata::GameData;
use mir_server::{
    GuildSettings, NpcState, Player, World, combat,
    world::{NAME_COLOUR_BROWN, NAME_COLOUR_RED, NAME_COLOUR_WHITE, NAME_COLOUR_YELLOW},
};
use mir_world::{Map, MapCell, MapVersion};
use tokio::sync::mpsc;

/// 测试地图。
const MAP: &str = "0";
/// 地图数据库主键。
const MAP_INDEX: i32 = 10;
/// 安全区中心（半径 10）。
const SAFE_CENTER: Point = Point::new(10, 10);
/// 安全区半径。
const SAFE_SIZE: u16 = 10;
/// 单件武器模板。
const SWORD: i32 = 100;
/// 可堆叠药水模板（`StackSize = 10`）。
const POTION: i32 = 101;
/// 绑定物品模板（`DontDeathdrop`）。
const BOUND: i32 = 102;
/// 测试伤害：一击必杀（玩家 1 级只有 18 点血）。
const LETHAL_DAMAGE: i32 = 99;

/// 60×60 可走空地。
fn synthetic_map() -> Arc<Map> {
    Map {
        width: 60,
        height: 60,
        cells: vec![MapCell::default(); 3600],
        file_name: MAP.to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
    .into()
}

/// 物品模板。
fn item(index: i32, name: &str, item_type: ItemType, stack_size: u16, bind: BindMode) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        item_type,
        stack_size,
        durability: 1000,
        weight: 1,
        bind,
        ..ItemInfo::default()
    }
}

/// 最小数据库：一张带安全区的地图 + 三件物品模板。
fn base_db() -> MirDb {
    MirDb {
        maps: vec![MapInfo {
            index: MAP_INDEX,
            file_name: MAP.to_owned(),
            title: "比奇省".to_owned(),
            safe_zones: vec![SafeZoneInfo {
                location: SAFE_CENTER,
                size: SAFE_SIZE,
                start_point: true,
            }],
            ..MapInfo::default()
        }],
        items: vec![
            item(SWORD, "测试剑", ItemType::Weapon, 1, BindMode::None),
            item(POTION, "测试药", ItemType::Potion, 10, BindMode::None),
            item(
                BOUND,
                "绑定物品",
                ItemType::Weapon,
                1,
                BindMode::DontDeathdrop,
            ),
        ],
        ..MirDb::default()
    }
}

/// 行会测试配置：1 级、0 金币就能建会，人数上限 10。
fn guild_settings() -> GuildSettings {
    GuildSettings {
        required_level: 1,
        creation_gold: 0,
        exp_rate: 0.0,
        point_per_level: 0,
        experience_list: vec![100],
        member_cap_list: vec![10],
        buff_list: Vec::new(),
    }
}

/// 测试世界 + 每个玩家的收包通道。
struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Harness {
    fn new(seed: u64) -> Self {
        let mut world = World::new(999);
        world.seed_rng(seed);
        world.insert_map(MAP, synthetic_map());
        world.set_game_data(Arc::new(GameData::from_db(base_db(), None)));
        world.set_guild_settings(guild_settings());
        Self {
            world,
            receivers: HashMap::new(),
        }
    }

    /// 放一个 1 级战士进世界。
    fn add_player(&mut self, object_id: u32, name: &str, position: Point) -> u32 {
        let stats = combat::player_base_stats(1, MirClass::Warrior);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.enter(Player {
            object_id,
            session_id: u64::from(object_id),
            character_id: i64::from(object_id),
            name: name.to_owned(),
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
            // 关掉自然回血，免得 10 秒一次的回复混进血量断言。
            regen_time: i64::MAX,
            death_time: 0,
            revive_time: 0,
            cell_time: 0,
            step_counter: 0,
            run_counter: 0,
            run_time: 0,
            dead: false,
            attack_mode: AttackMode::Peace,
            pk_points: 0,
            brown_time: 0,
            pk_point_time: 0,
            last_hitter: None,
            last_hit_time: 0,
            name_colour: NAME_COLOUR_WHITE,
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
            npc: NpcState::default(),
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
        self.receivers.insert(object_id, receiver);
        object_id
    }

    /// 取走某个玩家收到的全部包。
    fn packets(&mut self, object_id: u32) -> Vec<ServerPacket> {
        let receiver = self.receivers.get_mut(&object_id).expect("玩家通道");
        let mut packets = Vec::new();
        while let Ok(frame) = receiver.try_recv() {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
        }
        packets
    }

    fn clear(&mut self, object_id: u32) {
        let _ = self.packets(object_id);
    }

    /// 清掉动作/攻击冷却。
    fn ready(&mut self, object_id: u32) {
        if let Some(player) = self.world.player_mut(object_id) {
            player.action_time = 0;
            player.attack_time = 0;
        }
    }

    /// 让一个玩家的攻击必定命中、伤害固定为 `damage`（无暴击）。
    fn set_attack(&mut self, object_id: u32, damage: i32) {
        if let Some(player) = self.world.player_mut(object_id) {
            player.stats.min_dc = damage;
            player.stats.max_dc = damage;
            player.stats.accuracy = 100;
            player.stats.critical_rate = 0;
            player.stats.critical_damage = 0;
            player.stats.luck = 0;
        }
    }

    /// 让一个玩家不会闪避、护甲为 0。
    fn set_dummy_defence(&mut self, object_id: u32) {
        if let Some(player) = self.world.player_mut(object_id) {
            player.stats.agility = 0;
            player.stats.min_ac = 0;
            player.stats.max_ac = 0;
            player.stats.min_mac = 0;
            player.stats.max_mac = 0;
            player.stats.reflect = 0;
            player.stats.energy_shield_percent = 0;
        }
    }

    /// 往背包的 `slot` 放一件物品。
    fn put_inventory(&mut self, object_id: u32, slot: usize, item_index: i32, count: u16) -> u64 {
        let info = self
            .world
            .item_info(item_index)
            .cloned()
            .expect("物品模板存在");
        let mut item = self.world.create_fresh_item(&info);
        item.count = count;
        let unique_id = item.unique_id;
        if let Some(player) = self.world.player_mut(object_id) {
            player.inventory[slot] = Some(item);
        }
        unique_id
    }

    /// 往装备槽放一件物品（不刷新属性，测试只关心掉落）。
    fn put_equipment(&mut self, object_id: u32, slot: usize, item_index: i32) -> u64 {
        let info = self
            .world
            .item_info(item_index)
            .cloned()
            .expect("物品模板存在");
        let item = self.world.create_fresh_item(&info);
        let unique_id = item.unique_id;
        if let Some(player) = self.world.player_mut(object_id) {
            player.equipment[slot] = Some(item);
        }
        unique_id
    }

    /// 刷一只怪并立即唤醒它。
    fn spawn(&mut self, kind: MonsterKind, position: Point) -> u32 {
        let object_id = self.world.spawn_monster(MAP, kind, position);
        self.wake(object_id);
        object_id
    }

    fn wake(&mut self, object_id: u32) {
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
    }
}

/// 守卫模板（AI 6 = `Guard`，`Server/MirObjects/MonsterObject.cs:30-31`）。
fn guard_kind() -> MonsterKind {
    MonsterKind {
        name: "测试守卫".to_owned(),
        image: mir_core::enums::Monster::Guard,
        ai: 6,
        level: 30,
        view_range: 7,
        cool_eye: 100,
        attack_speed_ms: 1500,
        move_speed_ms: 1800,
        experience: 0,
        can_push: false,
        stats: combat::CombatStats {
            hp: 1000,
            min_dc: 7,
            max_dc: 7,
            accuracy: 100,
            agility: 0,
            ..combat::CombatStats::default()
        },
    }
}

/// 两个玩家摆成「攻击者 (30,30) 面向右、目标 (31,30)」，并切换到 `mode`。
fn duel(harness: &mut Harness, mode: AttackMode) -> (u32, u32) {
    let attacker = harness.add_player(1, "玩家1", Point::new(30, 30));
    let target = harness.add_player(2, "玩家2", Point::new(31, 30));
    harness.set_attack(attacker, 5);
    harness.set_dummy_defence(target);
    harness.set_dummy_defence(attacker);
    harness.world.change_attack_mode(attacker, mode);
    harness.ready(attacker);
    harness.clear(attacker);
    harness.clear(target);
    (attacker, target)
}

// ---------------------------------------------------------------------------
// 攻击模式与安全区
// ---------------------------------------------------------------------------

#[test]
fn safe_zone_blocks_player_attack() {
    let mut harness = Harness::new(1);
    // 双方都在安全区（中心 10,10、半径 10）内。
    let attacker = harness.add_player(1, "玩家1", Point::new(10, 10));
    let target = harness.add_player(2, "玩家2", Point::new(11, 10));
    harness.world.change_attack_mode(attacker, AttackMode::All);
    harness.set_attack(attacker, 5);
    harness.set_dummy_defence(target);

    // `PlayerObject.IsAttackTarget`：`Dead || InSafeZone || attacker.InSafeZone … → false`
    // （`PlayerObject.cs:4654`）。
    let report = harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击动作本身应被接受");
    assert_eq!(report, None, "安全区里不能选中玩家");
    assert!(harness.world.pending_hits().is_empty(), "不应排入延迟伤害");
    assert_eq!(harness.world.player(target).unwrap().hp, 18);
}

#[test]
fn peace_mode_refuses_player_target() {
    let mut harness = Harness::new(2);
    let (attacker, target) = duel(&mut harness, AttackMode::Peace);
    let report = harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击动作本身应被接受");
    // `case AttackMode.Peace: return false;`（`PlayerObject.cs:4667-4668`）。
    assert_eq!(report, None);
    assert!(harness.world.pending_hits().is_empty());
    assert_eq!(harness.world.player(target).unwrap().hp, 18);
}

#[test]
fn group_mode_refuses_group_mate() {
    let mut harness = Harness::new(3);
    let (attacker, _target) = duel(&mut harness, AttackMode::Group);
    harness
        .world
        .group_invite(attacker, "玩家2")
        .expect("发起组队邀请");
    harness.world.group_join(2, true).expect("接受组队邀请");
    assert_eq!(
        harness.world.group_id_of(attacker),
        harness.world.group_id_of(2)
    );

    let report = harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击动作本身应被接受");
    // `case AttackMode.Group: return GroupMembers == null || !GroupMembers.Contains(attacker);`
    // （`PlayerObject.cs:4661-4662`）：目标（this）的队伍里包含攻击者 → false。
    assert_eq!(report, None, "组队模式下不能打队友");
}

#[test]
fn group_mode_allows_non_member() {
    let mut harness = Harness::new(4);
    let (attacker, _target) = duel(&mut harness, AttackMode::Group);
    // 双方都不在队伍里：`GroupMembers == null` → 放行（`PlayerObject.cs:4662`）。
    let report = harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击动作本身应被接受");
    assert!(report.is_some(), "目标没有队伍时，Group 模式应放行");
}

#[test]
fn guild_mode_refuses_same_guild() {
    let mut harness = Harness::new(5);
    let (attacker, _target) = duel(&mut harness, AttackMode::Guild);
    harness
        .world
        .create_guild(attacker, "测试行会")
        .expect("创建行会");
    harness
        .world
        .guild_invite(attacker, "玩家2")
        .expect("邀请入会");
    harness.world.guild_join(2, true).expect("接受入会");
    assert_eq!(
        harness.world.guild_ref_of(attacker).map(|g| g.guild_id),
        harness.world.guild_ref_of(2).map(|g| g.guild_id)
    );

    let report = harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击动作本身应被接受");
    // `case AttackMode.Guild: return MyGuild == null || MyGuild != attacker.MyGuild;`
    // （`PlayerObject.cs:4663-4664`）。
    assert_eq!(report, None, "行会模式下不能打同会成员");
}

#[test]
fn enemy_guild_mode_hits_nobody_without_wars() {
    let mut harness = Harness::new(6);
    let (attacker, _target) = duel(&mut harness, AttackMode::EnemyGuild);
    harness
        .world
        .create_guild(attacker, "测试行会")
        .expect("创建行会");
    harness
        .world
        .guild_invite(attacker, "玩家2")
        .expect("邀请入会");
    harness.world.guild_join(2, true).expect("接受入会");
    let report = harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击动作本身应被接受");
    // 行会战争未实现：`MyGuild.IsEnemy(...)` 恒 false（`PlayerObject.cs:4665-4666`）。
    assert_eq!(report, None);
}

#[test]
fn red_brown_mode_only_hits_red_or_brown_players() {
    let mut harness = Harness::new(7);
    let (attacker, target) = duel(&mut harness, AttackMode::RedBrown);

    let report = harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击动作本身应被接受");
    assert_eq!(report, None, "白名目标不能打");

    // 目标变棕名（`Envir.Time < BrownTime`）后可以打。
    let now = harness.world.now();
    if let Some(player) = harness.world.player_mut(target) {
        player.brown_time = now + 60_000;
    }
    harness.ready(attacker);
    let report = harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击动作本身应被接受");
    assert!(report.is_some(), "棕名目标应可攻击");
    assert_eq!(harness.world.pending_hits().len(), 1);
    // 先把这一发结算掉（伤害 5，目标还有 13 血）。
    harness.world.advance_clock(300);
    harness.world.tick();

    // 再打一次红名目标。
    if let Some(player) = harness.world.player_mut(target) {
        player.brown_time = 0;
        player.pk_points = 200;
    }
    harness.ready(attacker);
    let report = harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击动作本身应被接受");
    // `case AttackMode.RedBrown: return PKPoints >= 200 || Envir.Time < BrownTime;`
    // （`PlayerObject.cs:4669-4670`）。
    assert!(report.is_some(), "红名目标应可攻击");
}

#[test]
fn all_mode_hits_any_player() {
    let mut harness = Harness::new(8);
    let (attacker, target) = duel(&mut harness, AttackMode::All);
    let report = harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击动作本身应被接受");
    assert_eq!(report.map(|report| report.target), Some(target));
}

// ---------------------------------------------------------------------------
// 玩家受击与包序
// ---------------------------------------------------------------------------

#[test]
fn pvp_attack_delays_damage_and_broadcasts_packets() {
    let mut harness = Harness::new(9);
    let (attacker, target) = duel(&mut harness, AttackMode::All);
    // 旁观者，用来断言 ObjectAttack / ObjectStruck 是广播给周围的。
    let observer = harness.add_player(3, "玩家3", Point::new(29, 31));
    harness.clear(observer);

    let report = harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击动作本身应被接受")
        .expect("前方有玩家目标");
    assert_eq!(report.target, target);
    // 伤害 300ms 后才结算（`HumanObject.cs:3201`）。
    assert_eq!(harness.world.player(target).unwrap().hp, 18);
    assert_eq!(harness.world.player(target).unwrap().last_hitter, None);

    harness.world.advance_clock(300);
    harness.world.tick();

    assert_eq!(harness.world.player(target).unwrap().hp, 13, "应扣 5 点血");
    assert_eq!(
        harness.world.player(target).unwrap().last_hitter,
        Some(attacker),
        "受害者要记住最后攻击者（`HumanObject.cs:7194`）"
    );

    let packets = harness.packets(target);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Struck(struck) if struck.attacker_id == attacker)),
        "受害者应收到 S.Struck，实际 {packets:?}"
    );
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::DamageIndicator(indicator)
                if indicator.object_id == target && indicator.damage == -5
        )),
        "受害者应收到 DamageIndicator(-5)，实际 {packets:?}"
    );
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::HealthChanged(health) if health.hp == 13
        )),
        "受害者应收到 HealthChanged(13)，实际 {packets:?}"
    );
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectStruck(_))),
        "ObjectStruck 不发给受害者本人"
    );

    let observed = harness.packets(observer);
    assert!(
        observed.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectAttack(attack) if attack.object_id == attacker
        )),
        "旁观者应收到 ObjectAttack，实际 {observed:?}"
    );
    assert!(
        observed.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectStruck(struck) if struck.object_id == target
        )),
        "旁观者应收到 ObjectStruck，实际 {observed:?}"
    );
}

#[test]
fn pvp_critical_broadcasts_object_effect_and_indicator() {
    let mut harness = Harness::new(10);
    let (attacker, target) = duel(&mut harness, AttackMode::All);
    let observer = harness.add_player(3, "玩家3", Point::new(29, 31));
    harness.clear(observer);
    if let Some(player) = harness.world.player_mut(attacker) {
        // 10 * 5 = 50%？不：CriticalRate 20 * 权重 5 = 100 → 必定暴击
        // （`HumanObject.cs:7156`）。
        player.stats.critical_rate = 20;
        player.stats.critical_damage = 5;
    }
    harness.ready(attacker);
    harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击");
    harness.world.advance_clock(300);
    harness.world.tick();

    // 5 点伤害 + floor(5 * (5 / 50) * 10) = 5 + 5 = 10（`HumanObject.cs:7159`）。
    assert_eq!(harness.world.player(target).unwrap().hp, 8);
    let observed = harness.packets(observer);
    assert!(
        observed.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectEffect(effect)
                if effect.object_id == target && effect.effect == mir_core::enums::SpellEffect::Critical
        )),
        "暴击应广播 ObjectEffect{{Critical}}，实际 {observed:?}"
    );
    assert!(
        observed.iter().any(|packet| matches!(
            packet,
            ServerPacket::DamageIndicator(indicator)
                if indicator.object_id == target
                    && indicator.type_ == mir_core::enums::DamageType::Critical
        )),
        "暴击应广播 DamageIndicator{{Critical}}，实际 {observed:?}"
    );
}

#[test]
fn first_hit_makes_attacker_brown() {
    let mut harness = Harness::new(11);
    let (attacker, _target) = duel(&mut harness, AttackMode::All);
    harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击");
    harness.world.advance_clock(300);
    harness.world.tick();

    // `if (Envir.Time > BrownTime && PKPoints < 200 && !AtWar(attacker))
    //      attacker.BrownTime = Envir.Time + Settings.Minute;`（`HumanObject.cs:7199-7200`）。
    let now = harness.world.now();
    let attacker_player = harness.world.player(attacker).unwrap();
    assert!(attacker_player.brown_time >= now + 59_000);
    assert_eq!(world_colour(&harness.world, attacker), NAME_COLOUR_BROWN);

    // 1 分钟后棕名结束（`RefreshNameColour` 下一 tick 才发包）。
    harness.world.advance_clock(61_000);
    harness.world.tick();
    assert_eq!(world_colour(&harness.world, attacker), NAME_COLOUR_WHITE);
}

fn world_colour(world: &World, object_id: u32) -> mir_core::Color {
    world.name_colour_for(object_id)
}

// ---------------------------------------------------------------------------
// PK 值
// ---------------------------------------------------------------------------

#[test]
fn kill_adds_pk_points_and_turns_name_red() {
    let mut harness = Harness::new(12);
    let (attacker, target) = duel(&mut harness, AttackMode::All);
    let observer = harness.add_player(3, "玩家3", Point::new(29, 31));
    harness.set_attack(attacker, LETHAL_DAMAGE);
    // 攻击者已经有 100 点：这一刀后到 200 → 红名。
    let now = harness.world.now();
    if let Some(player) = harness.world.player_mut(attacker) {
        player.pk_points = 100;
        // 把衰减排到未来，避免这一 tick 的 `Process` 先扣成 99。
        player.pk_point_time = now + 12_000;
    }
    harness.clear(attacker);
    harness.clear(observer);
    harness.ready(attacker);
    harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击");
    harness.world.advance_clock(300);
    harness.world.tick();

    assert!(harness.world.player(target).unwrap().dead);
    // `hitter.PKPoints = Math.Min(int.MaxValue, LastHitter.PKPoints + 100);`（`PlayerObject.cs:611`）。
    assert_eq!(harness.world.player(attacker).unwrap().pk_points, 200);

    // `RefreshNameColour` 在下一个 Process 里广播（`HumanObject.cs:360`）。
    harness.world.tick();
    assert_eq!(world_colour(&harness.world, attacker), NAME_COLOUR_RED);

    let attacker_packets = harness.packets(attacker);
    assert!(
        attacker_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::ColourChanged(changed) if changed.name_colour == NAME_COLOUR_RED
        )),
        "本人应收到 S.ColourChanged(Red)，实际 {attacker_packets:?}"
    );
    let observed = harness.packets(observer);
    assert!(
        observed.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectColourChanged(changed)
                if changed.object_id == attacker && changed.name_colour == NAME_COLOUR_RED
        )),
        "周围玩家应收到 S.ObjectColourChanged(Red)，实际 {observed:?}"
    );
}

#[test]
fn killing_a_red_player_adds_no_pk_points() {
    let mut harness = Harness::new(13);
    let (attacker, target) = duel(&mut harness, AttackMode::All);
    harness.set_attack(attacker, LETHAL_DAMAGE);
    let now = harness.world.now();
    if let Some(player) = harness.world.player_mut(target) {
        // 目标已经是红名：`else if (Envir.Time > BrownTime && PKPoints < 200)` 不成立
        // （`PlayerObject.cs:607`——这里的 `PKPoints` 是**受害者**的）。
        player.pk_points = 200;
        // 别让这一 tick 的衰减先把它降到 199。
        player.pk_point_time = now + 12_000;
    }
    harness.ready(attacker);
    harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击");
    harness.world.advance_clock(300);
    harness.world.tick();

    assert!(harness.world.player(target).unwrap().dead);
    assert_eq!(
        harness.world.player(attacker).unwrap().pk_points,
        0,
        "杀红名不增加 PK 值"
    );
}

#[test]
fn attacking_a_red_player_adds_no_pk_and_no_brown() {
    let mut harness = Harness::new(17);
    let (attacker, target) = duel(&mut harness, AttackMode::All);
    let now = harness.world.now();
    if let Some(player) = harness.world.player_mut(target) {
        player.pk_points = 200;
        player.pk_point_time = now + 12_000;
    }
    harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击红名");
    harness.world.advance_clock(300);
    harness.world.tick();

    // 伤害落地了（5 点），但攻方既没加 PK 值，也没被染棕：
    // 棕名判定要求受害者的 `PKPoints < 200`（`HumanObject.cs:7199`）。
    assert_eq!(harness.world.player(target).unwrap().hp, 13);
    assert_eq!(harness.world.player(attacker).unwrap().pk_points, 0);
    assert_eq!(harness.world.player(attacker).unwrap().brown_time, 0);
}

#[test]
fn pk_points_decay_one_per_pk_delay() {
    let mut harness = Harness::new(14);
    let player = harness.add_player(1, "玩家1", Point::new(40, 40));
    // 冻住时钟再取 `now`，让 11999/1 的边界不受真实时间流逝影响。
    harness.world.advance_clock(0);
    let now = harness.world.now();
    if let Some(player) = harness.world.player_mut(player) {
        player.pk_points = 3;
        player.pk_point_time = now;
    }
    // `Settings.PKDelay = 12` 秒（`Settings.cs:119`）；判定是**严格大于**
    // （`MapObject.cs:212`：`Envir.Time > PKPointTime`）。
    harness.world.tick();
    assert_eq!(harness.world.player(player).unwrap().pk_points, 3);
    harness.world.advance_clock(1);
    harness.world.tick();
    assert_eq!(harness.world.player(player).unwrap().pk_points, 2);
    // 下一轮要再等满 12 秒：到 now + 12000 时仍不衰减（严格大于）。
    harness.world.advance_clock(11_999);
    harness.world.tick();
    assert_eq!(
        harness.world.player(player).unwrap().pk_points,
        2,
        "12 秒整还不衰减"
    );
    harness.world.advance_clock(2);
    harness.world.tick();
    assert_eq!(harness.world.player(player).unwrap().pk_points, 1);
}

#[test]
fn red_name_falls_back_to_yellow_after_decay() {
    let mut harness = Harness::new(15);
    let player = harness.add_player(1, "玩家1", Point::new(40, 40));
    harness.world.advance_clock(0);
    let now = harness.world.now();
    if let Some(player) = harness.world.player_mut(player) {
        player.pk_points = 201;
        player.pk_point_time = now;
    }
    // 入场时 `name_colour` 是红名。
    assert_eq!(world_colour(&harness.world, player), NAME_COLOUR_RED);
    harness.clear(player);

    harness.world.advance_clock(1);
    harness.world.tick();
    // 201 → 200：`>= 200` 仍是红名，不发颜色包。
    assert_eq!(harness.world.player(player).unwrap().pk_points, 200);
    assert_eq!(world_colour(&harness.world, player), NAME_COLOUR_RED);

    harness.world.advance_clock(12_001);
    harness.world.tick();
    // 200 → 199：低于红名阈值，但 `>= 100` → 黄名（`PlayerObject.cs:1886`）。
    assert_eq!(harness.world.player(player).unwrap().pk_points, 199);
    assert_eq!(world_colour(&harness.world, player), NAME_COLOUR_YELLOW);
    let packets = harness.packets(player);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::ColourChanged(changed) if changed.name_colour == NAME_COLOUR_YELLOW
        )),
        "应由红转黄并发 S.ColourChanged，实际 {packets:?}"
    );
}

#[test]
fn revive_keeps_pk_points() {
    let mut harness = Harness::new(16);
    let (attacker, target) = duel(&mut harness, AttackMode::All);
    harness.set_attack(attacker, LETHAL_DAMAGE);
    let now = harness.world.now();
    if let Some(player) = harness.world.player_mut(target) {
        player.pk_points = 150;
        // 避免 10 秒自动复活期间赶上 12 秒的 PK 衰减。
        player.pk_point_time = now + 12_000;
    }
    harness.ready(attacker);
    harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击");
    harness.world.advance_clock(300);
    harness.world.tick();
    assert!(harness.world.player(target).unwrap().dead);

    // 正常路径由玩家/TownRevive 主动复活；10s 自动复活已删除，这里直接走世界层复活入口。
    assert!(harness.world.revive_player(target), "主动复活应生效");
    assert!(!harness.world.player(target).unwrap().dead);
    // `PKPoints` 是 `CharacterInfo` 的存档字段（`CharacterInfo.cs:66`），死亡/复活不清零。
    assert_eq!(harness.world.player(target).unwrap().pk_points, 150);
}

// ---------------------------------------------------------------------------
// 死亡掉落
// ---------------------------------------------------------------------------

#[test]
fn normal_death_drops_equipment_and_inventory_by_probability() {
    let mut harness = Harness::new(20);
    let (attacker, target) = duel(&mut harness, AttackMode::All);
    harness.set_attack(attacker, LETHAL_DAMAGE);
    // 14 个装备槽都放单件武器：每件按 `Random.Next(30) == 0` 掉（`HumanObject.cs:1460`）。
    for slot in 0..14 {
        harness.put_equipment(target, slot, SWORD);
    }
    // 背包放 10 叠药水（每叠 10 瓶）：`RandomomRange(10, 8)` 决定每叠掉多少
    // （`HumanObject.cs:1511-1518`）。多放几叠是为了让固定种子下「至少掉一叠」稳定成立。
    for slot in 0..10 {
        harness.put_inventory(target, slot, POTION, 10);
    }
    // 绑定物品永远不掉（`BindMode.DontDeathdrop`，`:1502-1503`）。
    harness.put_inventory(target, 10, BOUND, 1);

    harness.ready(attacker);
    harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击");
    harness.world.advance_clock(300);
    harness.world.tick();

    assert!(harness.world.player(target).unwrap().dead);
    let dropped = harness.world.ground_items();
    assert!(
        !dropped.is_empty(),
        "固定种子下应至少掉一件（药水叠 + 14 个装备槽）"
    );
    // 死亡掉落的物品没有归属（`ItemObject.cs:53-82` 不设置 `Owner`）。
    assert!(
        dropped.iter().all(|ground| ground.owner.is_none()),
        "死亡掉落不应有归属"
    );
    // 掉出去的药水数量与背包剩余量之和仍是 100。
    let ground_potions: u16 = dropped
        .iter()
        .filter_map(|ground| ground.item.as_ref())
        .filter(|item| item.item_index == POTION)
        .map(|item| item.count)
        .sum();
    let remaining: u16 = harness
        .world
        .player(target)
        .unwrap()
        .inventory
        .iter()
        .flatten()
        .filter(|item| item.item_index == POTION)
        .map(|item| item.count)
        .sum();
    assert_eq!(ground_potions + remaining, 100, "药水不能凭空消失");
    assert!(
        dropped
            .iter()
            .filter_map(|ground| ground.item.as_ref())
            .any(|item| item.item_index == POTION),
        "固定种子 20 应让药水叠掉出一部分（抽签顺序见 docs/world/pk.md）"
    );
    assert!(
        dropped
            .iter()
            .filter_map(|ground| ground.item.as_ref())
            .any(|item| item.item_index == SWORD),
        "固定种子 20 应让至少一件装备按 1/30 掉出"
    );
    // 绑定物品留在身上。
    assert!(
        harness
            .world
            .player(target)
            .unwrap()
            .inventory
            .iter()
            .flatten()
            .any(|item| item.item_index == BOUND),
        "DontDeathdrop 的绑定物品不该掉"
    );
}

#[test]
fn red_death_drops_whole_inventory() {
    let mut harness = Harness::new(21);
    let (attacker, target) = duel(&mut harness, AttackMode::All);
    harness.set_attack(attacker, LETHAL_DAMAGE);
    let now = harness.world.now();
    if let Some(player) = harness.world.player_mut(target) {
        // `if (PKPoints > 200) RedDeathDrop(LastHitter);`（`PlayerObject.cs:638`）。
        player.pk_points = 201;
        player.pk_point_time = now + 12_000;
    }
    harness.put_inventory(target, 0, POTION, 3);
    harness.put_inventory(target, 1, SWORD, 1);
    harness.put_inventory(target, 2, BOUND, 1);

    harness.ready(attacker);
    harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击");
    harness.world.advance_clock(300);
    harness.world.tick();

    assert!(harness.world.player(target).unwrap().dead);
    // `RedDeathDrop` 的背包分支没有任何随机判定（`PlayerObject.cs:750-790`）：全掉。
    assert!(
        harness
            .world
            .player(target)
            .unwrap()
            .inventory
            .iter()
            .flatten()
            .all(|item| item.item_index == BOUND),
        "红名死亡时背包（除绑定物品外）应掉光"
    );
    let names: Vec<i32> = harness
        .world
        .ground_items()
        .iter()
        .filter_map(|ground| ground.item.as_ref())
        .map(|item| item.item_index)
        .collect();
    assert!(names.contains(&POTION));
    assert!(names.contains(&SWORD));
}

#[test]
fn monster_kill_inside_safe_zone_does_not_drop() {
    let mut harness = Harness::new(22);
    // 安全区里：`else if (!InSafeZone) DeathDrop(...)` 不成立（`PlayerObject.cs:640-641`）。
    let player = harness.add_player(1, "玩家1", Point::new(10, 10));
    harness.put_inventory(player, 0, POTION, 10);
    harness.put_equipment(player, 0, SWORD);
    if let Some(player) = harness.world.player_mut(player) {
        player.hp = 1;
        player.stats.agility = 0;
    }
    let monster = harness.spawn(
        MonsterKind {
            name: "测试怪".to_owned(),
            image: mir_core::enums::Monster::Oma,
            ai: 0,
            level: 1,
            view_range: 7,
            cool_eye: 0,
            attack_speed_ms: 1000,
            move_speed_ms: 1800,
            experience: 0,
            can_push: true,
            stats: combat::CombatStats {
                hp: 10,
                min_dc: LETHAL_DAMAGE,
                max_dc: LETHAL_DAMAGE,
                accuracy: 100,
                agility: 0,
                ..combat::CombatStats::default()
            },
        },
        Point::new(11, 10),
    );
    harness.world.tick();
    harness.world.advance_clock(300);
    harness.world.tick();
    assert!(harness.world.player(player).unwrap().dead);
    assert_eq!(
        harness.world.player(player).unwrap().last_hitter,
        Some(monster)
    );
    assert!(
        harness.world.ground_items().is_empty(),
        "安全区内死亡不爆（Crystal `!InSafeZone` 才 DeathDrop）"
    );
}

#[test]
fn monster_kill_outside_safe_zone_still_drops_in_crystal() {
    // Crystal 的 `pkbodydrop` 变量在 `DeathDrop` 里被初始化成 `true` 且条件恒真
    // （`HumanObject.cs:1393-1398`），所以**被怪杀死也会爆**。这是一个源码 bug，
    // 本作照抄并在 `docs/world/pk.md` 里记录。
    let mut harness = Harness::new(23);
    let player = harness.add_player(1, "玩家1", Point::new(30, 30));
    harness.put_inventory(player, 0, POTION, 10);
    if let Some(player) = harness.world.player_mut(player) {
        player.hp = 1;
        player.stats.agility = 0;
    }
    harness.spawn(
        MonsterKind {
            name: "测试怪".to_owned(),
            image: mir_core::enums::Monster::Oma,
            ai: 0,
            level: 1,
            view_range: 7,
            cool_eye: 0,
            attack_speed_ms: 1000,
            move_speed_ms: 1800,
            experience: 0,
            can_push: true,
            stats: combat::CombatStats {
                hp: 10,
                min_dc: LETHAL_DAMAGE,
                max_dc: LETHAL_DAMAGE,
                accuracy: 100,
                agility: 0,
                ..combat::CombatStats::default()
            },
        },
        Point::new(31, 30),
    );
    harness.world.tick();
    harness.world.advance_clock(300);
    harness.world.tick();
    assert!(harness.world.player(player).unwrap().dead);
    assert!(
        !harness.world.ground_items().is_empty(),
        "Crystal 默认连怪物击杀也会走 DeathDrop（pkbodydrop 恒 true）"
    );
}

#[test]
fn killer_can_pick_up_death_drops() {
    let mut harness = Harness::new(24);
    let (attacker, target) = duel(&mut harness, AttackMode::All);
    harness.set_attack(attacker, LETHAL_DAMAGE);
    let now = harness.world.now();
    if let Some(player) = harness.world.player_mut(target) {
        // 红名：背包无条件全掉，保证有东西可捡。
        player.pk_points = 201;
        player.pk_point_time = now + 12_000;
    }
    harness.put_inventory(target, 0, POTION, 3);
    harness.ready(attacker);
    harness
        .world
        .attack(attacker, MirDirection::Right)
        .expect("攻击");
    harness.world.advance_clock(300);
    harness.world.tick();

    let ground = harness
        .world
        .ground_items()
        .first()
        .expect("红名死亡必掉背包")
        .clone();
    assert_eq!(ground.owner, None, "死亡掉落没有归属保护");
    // 凶手走到掉落格上（服务端权威；`place_player` 同时同步 AOI 网格）。
    assert!(
        harness
            .world
            .place_player(attacker, &ground.map, ground.position)
    );
    let report = harness.world.pickup(attacker).expect("凶手应能拾取");
    assert_eq!(report.ground, ground.object_id);
    let dropped_uid = ground.item.as_ref().map(|item| item.unique_id);
    assert!(
        harness
            .world
            .player(attacker)
            .unwrap()
            .inventory
            .iter()
            .flatten()
            .any(|item| Some(item.unique_id) == dropped_uid),
        "死亡掉落应进入凶手背包"
    );
}

// ---------------------------------------------------------------------------
// 守卫
// ---------------------------------------------------------------------------

#[test]
fn guard_attacks_red_player_and_uses_ac_defence() {
    let mut harness = Harness::new(30);
    let red = harness.add_player(1, "红名", Point::new(34, 30));
    let now = harness.world.now();
    if let Some(player) = harness.world.player_mut(red) {
        player.pk_points = 200;
        player.pk_point_time = now + 12_000;
        player.stats.agility = 15; // 有敏捷也不该闪避：守卫用 DefenceType.AC。
        player.stats.min_ac = 0;
        player.stats.max_ac = 0;
    }
    let observer = harness.add_player(2, "旁观者", Point::new(33, 31));
    let guard = harness.spawn(guard_kind(), Point::new(30, 30));
    harness.clear(red);
    harness.clear(observer);

    harness.world.tick();
    // `PlayerObject.IsAttackTarget(MonsterObject)`：AI 6/58/113 → 只看 `PKPoints >= 200`
    // （`PlayerObject.cs:4679`）。
    assert_eq!(harness.world.monster(guard).unwrap().target, Some(red));
    assert_eq!(
        harness.world.pending_hits().len(),
        1,
        "守卫应立刻排入延迟伤害"
    );

    harness.world.advance_clock(300);
    harness.world.tick();
    // 7 点伤害、护甲 0（`Monsters/Guard.cs:79-86`）。
    assert_eq!(harness.world.player(red).unwrap().hp, 11);
    let observed = harness.packets(observer);
    assert!(
        observed.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectAttack(attack)
                // 守卫把攻击动画广播在**目标背后一格**（`Monsters/Guard.cs:70-73`）。
                if attack.object_id == guard && attack.location == Point::new(34, 29)
        )),
        "守卫的 ObjectAttack 应出现在目标背后一格，实际 {observed:?}"
    );
    let struck = harness.packets(red);
    assert!(
        struck.iter().any(|packet| matches!(
            packet,
            ServerPacket::Struck(struck) if struck.attacker_id == guard
        )),
        "红名应被守卫打中，实际 {struck:?}"
    );
}

#[test]
fn guard_ignores_white_and_brown_players_until_red() {
    let mut harness = Harness::new(31);
    let player = harness.add_player(1, "白名", Point::new(34, 30));
    let now = harness.world.now();
    if let Some(player) = harness.world.player_mut(player) {
        player.brown_time = now + 60_000;
    }
    let guard = harness.spawn(guard_kind(), Point::new(30, 30));
    harness.world.tick();
    assert_eq!(
        harness.world.monster(guard).unwrap().target,
        None,
        "棕名不是红名，守卫不该动手"
    );
    assert!(
        harness.world.pending_hits().is_empty(),
        "棕名不是红名，守卫不该动手"
    );
}

#[test]
fn guard_walks_back_home_when_idle() {
    let mut harness = Harness::new(32);
    let guard = harness.spawn(guard_kind(), Point::new(30, 30));
    // 模拟守卫沿路线走开（Crystal 的 `Route`，本作用 `home` 出生点近似）。
    // 走 `place_monster` 同步 AOI 网格，否则索敌/广播还会按旧坐标找它。
    let guard_map = harness.world.monster(guard).expect("守卫还在").map.clone();
    assert!(
        harness
            .world
            .place_monster(guard, &guard_map, Point::new(33, 30))
    );
    harness.world.tick();
    assert_eq!(
        harness.world.monster(guard).unwrap().position,
        Point::new(32, 30),
        "没有目标时守卫应朝出生点走一格"
    );
    harness.wake(guard);
    harness.world.tick();
    assert_eq!(
        harness.world.monster(guard).unwrap().position,
        Point::new(31, 30)
    );
}

// ---------------------------------------------------------------------------
// `C.ChangeAMode`
// ---------------------------------------------------------------------------

#[test]
fn change_attack_mode_echoes_to_self_only() {
    let mut harness = Harness::new(40);
    let player = harness.add_player(1, "玩家1", Point::new(30, 30));
    let other = harness.add_player(2, "玩家2", Point::new(31, 30));
    harness.clear(player);
    harness.clear(other);

    assert!(harness.world.change_attack_mode(player, AttackMode::Guild));
    assert_eq!(
        harness.world.attack_mode_of(player),
        Some(AttackMode::Guild)
    );
    let packets = harness.packets(player);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::ChangeAMode(changed) if changed.mode == AttackMode::Guild
        )),
        "本人应收到 S.ChangeAMode{{Guild}}，实际 {packets:?}"
    );
    // Crystal 只 `Enqueue` 给本人，**不广播**（`MirConnection.cs:1432-1439`）。
    let observed = harness.packets(other);
    assert!(
        !observed
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ChangeAMode(_))),
        "攻击模式不该广播给周围玩家"
    );
}

// ---------------------------------------------------------------------------
// 真实数据冒烟
// ---------------------------------------------------------------------------

#[test]
fn real_database_exposes_safe_zone_and_guard_template() {
    let Ok(data) = GameData::discover() else {
        eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
        return;
    };
    // 安全区：找第一张配置了安全区的地图，打印并断言解析成功。
    let Some((map, zone)) = data
        .db
        .maps
        .iter()
        .find_map(|map| map.safe_zones.first().map(|zone| (map, zone)))
    else {
        eprintln!("跳过：样本数据库里没有安全区配置");
        return;
    };
    assert!(!map.file_name.is_empty(), "地图文件名来自 MapInfo.FileName");
    assert!(zone.size > 0, "安全区半径应大于 0");
    println!(
        "安全区样本：map={} title={} center=({}, {}) size={} start_point={}",
        map.file_name, map.title, zone.location.x, zone.location.y, zone.size, zone.start_point
    );

    // 守卫模板：AI 6/58/113（`MonsterObject.GetMonster` 的 `Guard`/`ArcherGuard`）。
    let guard = data
        .db
        .monsters
        .iter()
        .find(|monster| matches!(monster.ai, 6 | 58 | 113));
    let Some(guard) = guard else {
        eprintln!("跳过：样本数据库里没有守卫类怪物（AI 6/58/113）");
        return;
    };
    let kind = mir_server::gamedata::kind_from_monster_info(guard);
    assert!(kind.is_guard(), "AI {} 应识别为守卫", guard.ai);
    assert!(kind.stats.hp > 0, "守卫模板应有 HP");
    assert!(!kind.name.is_empty(), "守卫模板应有名字");
    println!(
        "守卫样本：name={} ai={} level={} hp={} view={}",
        kind.name, kind.ai, kind.level, kind.stats.hp, kind.view_range
    );
}
