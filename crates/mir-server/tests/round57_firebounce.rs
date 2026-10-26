//! R57-G1：火焰弹射（`Spell.FireBounce`，154）的真实 `S.ObjectProjectile`（125）服务端生产者。
//!
//! 全部断言对照 Crystal（参考根目录 `github\Crystal`）：
//!
//! | 行为 | Crystal |
//! |---|---|
//! | 首跳只排伤害（`delay = MaxDistance * 50 + 500`），**不发** 125 | `HumanObject.cs:5811-5836` |
//! | 续跳先广播 125（`Source` = 本跳受害者、`Destination` = 下一目标）再排 `dist * 50` 伤害 | `HumanObject.cs:5824-5827`、`:5947-5954` |
//! | 伤害走 `DefenceType.MAC`，实际扣血 `> 0` 才 `LevelMagic` | `HumanObject.cs:5938-5942` |
//! | 只有受害者种族为 `Monster` 才继续弹跳 | `HumanObject.cs:5947` |
//! | 候选 = `FindAllNearby(3)` 内、非受害者、`IsAttackTarget`、`CanFly`、种族 Monster/Player/Hero | `MonsterObject.cs:2347-2384`、`MapObject.cs:321-337`、`Map.cs:2480-2482` |
//! | 总命中上限 `Level + 2`（`bounce` 从 `Level + 2` 递减，`--bounce` 后为 0 即停） | `HumanObject.cs:5819`、`:5953` |
//! | 全程不发 `RangeAttack`（126）/`ObjectSpell`（149） | 本文件所有用例扫描断言 |
//!
//! 已知偏差（D3 PARTIAL，实现注释里也登记了）：125 用 `World::broadcast` 广播，沿用本作
//! 「不回显施法者本人」惯例；Crystal `Map.Broadcast`（`Map.cs:2437-2448`）包含施法者。
//! 客户端 125 渲染不在 R57 范围；Hero 候选无实例（服务器不生成 Hero 对象，`AoiKind` 也没有
//! Hero），所以「接受 Player 候选」用例即种族口径的证据。
//!
//! 所有用例确定性：固定 RNG 种子、`with_data` 先 `advance_clock(0)` 冻结时钟、怪物
//! `operate_time = -1`（训练木桩 AI 56）、施法者 `MinMC = MaxMC = 10`、怪物 MAC = 0。
//! 真实 `FireBounce(154)` 的 `MagicInfo` 来自默认数据集（A0 探针已证明存在，不做 fixture
//! 注入；数据集缺失时本文件直接 panic，不 skip）。

mod common;

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, OnceLock};

use mir_core::Point;
use mir_core::enums::{
    AttackMode, MirClass, MirDirection, MirGender, Monster as MonsterImage, Spell,
};
use mir_protocol::ServerPacket;
use mir_server::MonsterKind;
use mir_server::combat::{self, CombatStats};
use mir_server::gamedata::GameData;
use mir_server::magic::{CastReport, MagicError};
use mir_server::world::{Player, World};
use tokio::sync::mpsc;

/// 合成地图名：自定义名避免命中默认数据集里的安全区/免战图配置。
const MAP: &str = "r57_fire_bounce";

// ---------------------------------------------------------------------------
// 测试脚手架
// ---------------------------------------------------------------------------

/// 默认数据集（`cjlaaa`）只加载一次：A0 已实测 `FireBounce(154)` 存在。
fn game_data() -> Arc<GameData> {
    static DATA: OnceLock<Arc<GameData>> = OnceLock::new();
    DATA.get_or_init(|| {
        common::default_data().expect(
            "default dataset (cjlaaa) must be present: A0 proved FireBounce(154) exists in it",
        )
    })
    .clone()
}

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
        // 先冻结时钟：`World::now` 在首次 `advance_clock` 前取真实墙钟，若等到第一次
        // `elapse` 才冻结，`cast_magic` 与 `elapse` 之间真实耗时就可能把排期提前推进
        // （同 `tests/round52_poison_cloud.rs:81-94`）。
        world.advance_clock(0);
        world.seed_rng(seed);
        world.set_game_data(data);
        world.insert_map(MAP, common::synthetic_map(MAP));
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
            attack_mode: AttackMode::Peace,
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
            bind_point: Point::default(),
            sender,
        });
        if let Some(player) = self.world.player_mut(object_id) {
            player.regen_time = i64::MAX;
        }
        self.receivers.insert(object_id, receiver);
        object_id
    }

    fn set_attack_mode(&mut self, object_id: u32, mode: AttackMode) {
        self.world.player_mut(object_id).expect("玩家").attack_mode = mode;
    }

    /// 把 `FireBounce` 的等级钉死（`learn_magic` 不校验角色等级，等级只在升级时读取）。
    fn set_magic_level(&mut self, object_id: u32, spell: Spell, level: u8) {
        let magic = self
            .world
            .player_mut(object_id)
            .expect("玩家")
            .magics
            .iter_mut()
            .find(|magic| magic.spell == spell)
            .expect("已学技能");
        magic.level = level;
    }

    /// 固定施法者攻击力抽签：`MinMC == MaxMC` 时 `attack_power` 不抽签（`combat.rs:173-190`）。
    fn set_magic_attack(&mut self, object_id: u32, min_mc: i32, max_mc: i32) {
        let player = self.world.player_mut(object_id).expect("玩家");
        player.stats.min_mc = min_mc;
        player.stats.max_mc = max_mc;
    }

    fn spawn(&mut self, kind: MonsterKind, position: Point) -> u32 {
        let object_id = self.world.spawn_monster(MAP, kind, position);
        if let Some(monster) = self.world.monster_mut(object_id) {
            // 训练木桩（AI 56）：不走动、不攻击。
            monster.operate_time = -1;
            monster.regen_time = i64::MAX;
        }
        object_id
    }

    fn learn(&mut self, object_id: u32, spell: Spell) {
        self.world
            .learn_magic(object_id, spell)
            .expect("学习应当成功");
    }

    /// 清动作/技能计时与冷却、回满魔法（等价于「等了足够久」）。
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

    fn cast(
        &mut self,
        object_id: u32,
        spell: Spell,
        direction: MirDirection,
        target_id: u32,
        location: Point,
    ) -> Result<CastReport, MagicError> {
        self.world
            .cast_magic(object_id, spell, direction, target_id, location)
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
}

/// 测试怪物：打不死、不还手的木桩；`hp` 用来控制「一击必杀」的受控连跳。
fn test_monster(hp: i32) -> MonsterKind {
    MonsterKind {
        name: "火焰弹射测试怪".to_owned(),
        image: MonsterImage::Oma,
        ai: 56,
        level: 1,
        view_range: 1,
        cool_eye: 0,
        attack_speed_ms: 2_500,
        move_speed_ms: 1_800,
        experience: 0,
        can_push: true,
        stats: CombatStats {
            hp,
            ..CombatStats::default()
        },
    }
}

/// 收集 125：`(Spell, Source, Destination)`。
fn projectiles(packets: &[ServerPacket]) -> Vec<(Spell, u32, u32)> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectProjectile(projectile) => {
                Some((projectile.spell, projectile.source, projectile.destination))
            }
            _ => None,
        })
        .collect()
}

/// `FireBounce` 不得伪装成近战/旧法术包（126 `RangeAttack`、149 `ObjectSpell`）。
fn assert_no_forbidden_packets(packets: &[ServerPacket]) {
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::RangeAttack(_))),
        "FireBounce must not emit RangeAttack (126)"
    );
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectSpell(_))),
        "FireBounce must not emit ObjectSpell (149)"
    );
}

// ---------------------------------------------------------------------------
// 1. 施法包：120 + 123，首跳不发 125；非法初始目标照常施法但无效果
// ---------------------------------------------------------------------------

#[test]
fn fire_bounce_cast_emits_magic_and_object_magic_without_projectile() {
    let mut harness = Harness::new(7);
    let caster = harness.add_player(1, MirClass::Wizard, 22, Point::new(10, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 22, Point::new(10, 26));
    harness.set_magic_attack(caster, 10, 10);
    harness.learn(caster, Spell::FireBounce);
    harness.set_magic_level(caster, Spell::FireBounce, 3);
    harness.ready_all(caster);

    // 非法初始目标（尸体）：Crystal `case Spell.FireBounce: if (!FireBounce(target, magic, this))
    // targetID = 0;`（`HumanObject.cs:3765-3766`）——照常施法、不报错、只把目标置 0。
    let corpse = harness.spawn(test_monster(10), Point::new(12, 20));
    if let Some(monster) = harness.world.monster_mut(corpse) {
        monster.dead = true;
        // 手动置死时给足尸体保留时间，避免 `tick_monsters` 提前清尸。
        monster.dead_time = i64::MAX;
    }
    harness.drain(caster);
    harness.drain(observer);
    let report = harness
        .cast(
            caster,
            Spell::FireBounce,
            MirDirection::Right,
            corpse,
            Point::new(12, 20),
        )
        .expect("invalid initial target still casts");
    assert_eq!(
        report.target_id, 0,
        "dead initial target must be dropped to 0"
    );
    let caster_packets = harness.packets(caster);
    assert!(
        caster_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::Magic(magic)
                if magic.spell == Spell::FireBounce && magic.cast && magic.target_id == 0
        )),
        "invalid initial target still emits S.Magic cast=true target_id=0"
    );
    assert!(
        projectiles(&caster_packets).is_empty(),
        "invalid initial target emits no 125"
    );
    harness.elapse(2_000);
    assert!(
        projectiles(&harness.packets(observer)).is_empty(),
        "invalid initial target schedules no 125"
    );
    assert_eq!(harness.hp(corpse), 10, "dead target never damaged");

    // 合法初始目标：120 给本人、123 给旁观者，125 一条都不发、伤害也不立即结算。
    let target = harness.spawn(test_monster(100), Point::new(15, 20));
    harness.ready_all(caster);
    harness.drain(caster);
    harness.drain(observer);
    let report = harness
        .cast(
            caster,
            Spell::FireBounce,
            MirDirection::Right,
            target,
            Point::new(15, 20),
        )
        .expect("cast");
    assert_eq!(report.target_id, target, "initial target accepted");
    assert_eq!(
        report.damage,
        Some(26),
        "GetDamage = GetAttackPower(10, 10) + GetPower(level 3) = 10 + 16"
    );
    assert_eq!(
        harness.hp(target),
        100,
        "first hit is delayed, never immediate"
    );
    let caster_packets = harness.packets(caster);
    assert!(
        caster_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::Magic(magic)
                if magic.spell == Spell::FireBounce && magic.cast && magic.target_id == target
        )),
        "caster receives S.Magic cast=true with the accepted target"
    );
    let observer_packets = harness.packets(observer);
    assert!(
        observer_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectMagic(magic)
                if magic.object_id == caster
                    && magic.spell == Spell::FireBounce
                    && magic.cast
                    && magic.target_id == target
        )),
        "observer receives broadcast S.ObjectMagic (123)"
    );
    assert!(
        projectiles(&caster_packets).is_empty(),
        "first hop emits no 125 for the caster"
    );
    assert!(
        projectiles(&observer_packets).is_empty(),
        "first hop emits no 125 for the observer"
    );
    assert_no_forbidden_packets(&caster_packets);
    assert_no_forbidden_packets(&observer_packets);
}

// ---------------------------------------------------------------------------
// 2. 首跳时序：`500 + MaxDistance * 50` ms（Crystal `HumanObject.cs:5817`、`:5821-5822`）
// ---------------------------------------------------------------------------

#[test]
fn fire_bounce_first_hit_lands_after_distance_delay() {
    let mut harness = Harness::new(11);
    let caster = harness.add_player(1, MirClass::Wizard, 22, Point::new(10, 20));
    harness.add_player(2, MirClass::Wizard, 22, Point::new(10, 26));
    harness.set_magic_attack(caster, 10, 10);
    harness.learn(caster, Spell::FireBounce);
    harness.set_magic_level(caster, Spell::FireBounce, 3);
    harness.ready_all(caster);
    let target = harness.spawn(test_monster(100), Point::new(15, 20));
    harness.drain(caster);

    let report = harness
        .cast(
            caster,
            Spell::FireBounce,
            MirDirection::Right,
            target,
            Point::new(15, 20),
        )
        .expect("cast");
    assert_eq!(report.damage, Some(26), "level 3 damage roll");

    // MaxDistance(caster, target) = 5 → 5 * 50 + 500 = 750ms。
    harness.elapse(749);
    assert_eq!(
        harness.hp(target),
        100,
        "no damage before 500 + distance * 50 ms"
    );
    harness.elapse(1);
    assert_eq!(
        harness.hp(target),
        74,
        "first hit lands exactly on the scheduled tick"
    );
}

// ---------------------------------------------------------------------------
// 3. 受控连跳：level + 2 次命中、level + 1 条 125，且 125 早于该跳伤害
// ---------------------------------------------------------------------------

#[test]
fn fire_bounce_emits_125_before_each_secondary_hit_and_hits_level_plus_two_targets() {
    let mut harness = Harness::new(23);
    let caster = harness.add_player(1, MirClass::Wizard, 22, Point::new(10, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 22, Point::new(10, 26));
    harness.set_magic_attack(caster, 10, 10);
    harness.learn(caster, Spell::FireBounce);
    harness.set_magic_level(caster, Spell::FireBounce, 3);
    harness.ready_all(caster);

    // 5 只 1 HP 怪沿 x 轴间距 3：每跳击杀后前一只尸体被 `IsAttackTarget` 排除，
    // 半径 3 内只剩唯一候选，连跳路径完全确定。
    let victim1 = harness.spawn(test_monster(1), Point::new(15, 20));
    let victim2 = harness.spawn(test_monster(1), Point::new(18, 20));
    let victim3 = harness.spawn(test_monster(1), Point::new(21, 20));
    let victim4 = harness.spawn(test_monster(1), Point::new(24, 20));
    let victim5 = harness.spawn(test_monster(1), Point::new(27, 20));
    harness.drain(caster);
    harness.drain(observer);

    harness
        .cast(
            caster,
            Spell::FireBounce,
            MirDirection::Right,
            victim1,
            Point::new(15, 20),
        )
        .expect("cast");

    // 首跳：500 + 5 * 50 = 750ms，只结算伤害、不发 125。
    harness.elapse(750);
    assert_eq!(harness.hp(victim1), 0, "first victim killed");
    let packets = harness.packets(observer);
    assert_eq!(
        projectiles(&packets),
        vec![(Spell::FireBounce, victim1, victim2)],
        "125 source is the previous victim, destination the next target"
    );
    assert_eq!(
        harness.hp(victim2),
        1,
        "125 is broadcast before the next hit lands"
    );

    // 后续每跳 MaxDistance = 3 → 150ms。
    for (previous, next) in [(victim2, victim3), (victim3, victim4), (victim4, victim5)] {
        harness.elapse(150);
        assert_eq!(harness.hp(previous), 0, "previous victim killed");
        let packets = harness.packets(observer);
        assert_eq!(
            projectiles(&packets),
            vec![(Spell::FireBounce, previous, next)],
            "each secondary hop emits exactly one 125 before its damage"
        );
        assert_eq!(
            harness.hp(next),
            1,
            "125 must arrive before the next hit lands"
        );
    }

    // 命中 level + 2 = 5 次后停、125 共 level + 1 = 4 条。
    harness.elapse(1_000);
    assert_eq!(harness.hp(victim5), 0, "fifth and last victim killed");
    assert!(
        projectiles(&harness.packets(observer)).is_empty(),
        "no 125 after the bounce budget is exhausted"
    );
    assert_no_forbidden_packets(&harness.packets(observer));
}

// ---------------------------------------------------------------------------
// 4. 候选过滤与随机选择：只收可攻击的半径 3 内目标，固定种子可复现
// ---------------------------------------------------------------------------

/// 场景结果：`(125 列表, 受害者, 半径内活怪, 可攻击玩家, 尸体, 半径外怪)`。
type FilterScenario = (Vec<(Spell, u32, u32)>, u32, u32, u32, u32, u32);

/// 返回 `(125 列表, 受害者, 半径内活怪, 可攻击玩家, 尸体, 半径外怪)`。
fn run_filter_scenario(seed: u64) -> FilterScenario {
    let mut harness = Harness::new(seed);
    let caster = harness.add_player(1, MirClass::Wizard, 22, Point::new(10, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 22, Point::new(15, 28));
    let player = harness.add_player(3, MirClass::Wizard, 22, Point::new(12, 20));
    harness.set_attack_mode(caster, AttackMode::All);
    harness.set_magic_attack(caster, 10, 10);
    harness.learn(caster, Spell::FireBounce);
    harness.set_magic_level(caster, Spell::FireBounce, 3);
    harness.ready_all(caster);

    let victim = harness.spawn(test_monster(100), Point::new(15, 20));
    let nearby_monster = harness.spawn(test_monster(100), Point::new(18, 20));
    let corpse = harness.spawn(test_monster(10), Point::new(16, 20));
    let far_monster = harness.spawn(test_monster(100), Point::new(15, 25));
    if let Some(monster) = harness.world.monster_mut(corpse) {
        monster.dead = true;
        monster.dead_time = i64::MAX;
    }
    harness.drain(caster);
    harness.drain(observer);

    harness
        .cast(
            caster,
            Spell::FireBounce,
            MirDirection::Right,
            victim,
            Point::new(15, 20),
        )
        .expect("cast");
    // 首跳 500 + 5 * 50 = 750ms；此 tick 只选下一跳目标并广播 125。
    harness.elapse(750);
    let packets = harness.packets(observer);
    (
        projectiles(&packets),
        victim,
        nearby_monster,
        player,
        corpse,
        far_monster,
    )
}

#[test]
fn fire_bounce_secondary_target_filter_and_random_choice() {
    let (first_run, victim, nearby_monster, player, corpse, far_monster) = run_filter_scenario(41);
    assert_eq!(first_run.len(), 1, "exactly one secondary target is chosen");
    let (spell, source, destination) = first_run[0];
    assert_eq!(spell, Spell::FireBounce, "125 carries the spell id");
    assert_eq!(source, victim, "125 source is the current victim");
    assert!(
        destination == nearby_monster || destination == player,
        "candidate pool is the attackable monster and the attackable player within radius 3"
    );
    assert_ne!(destination, corpse, "dead monsters are not candidates");
    assert_ne!(
        destination, far_monster,
        "monsters outside FindAllNearby(3) are not candidates"
    );

    // 固定种子复跑 2 次：RNG 走世界级 MirRng + 候选按对象 ID 排序，结果必须一致。
    let (second_run, second_victim, second_monster, second_player, _, _) = run_filter_scenario(41);
    assert_eq!(
        first_run, second_run,
        "same seed reproduces the same random pick"
    );
    assert_eq!(victim, second_victim);
    assert_eq!(nearby_monster, second_monster);
    assert_eq!(player, second_player);

    // 多个种子必须能命中候选池里的两种目标：证明是「从候选列表随机取」而不是固定偏好。
    let mut picked: Vec<u32> = vec![destination];
    for seed in 42..=57 {
        let (run, _, _, _, _, _) = run_filter_scenario(seed);
        assert_eq!(run.len(), 1);
        picked.push(run[0].2);
    }
    assert!(
        picked.contains(&nearby_monster),
        "the attackable monster is selectable"
    );
    assert!(
        picked.contains(&player),
        "an attackable player is a valid candidate (M2-c race rule)"
    );
    for picked_id in &picked {
        assert!(
            *picked_id == nearby_monster || *picked_id == player,
            "only filtered candidates can be chosen"
        );
    }
}

// ---------------------------------------------------------------------------
// 5. 停止条件：无候选停、弹跳预算用尽停
// ---------------------------------------------------------------------------

#[test]
fn fire_bounce_chain_stops_without_nearby_monster_or_bounce_exhausted() {
    // (a) bounce 预算用尽：level 0 → bounce = 2 → 1 条 125（level + 1）后停。
    let mut harness = Harness::new(5);
    let caster = harness.add_player(1, MirClass::Wizard, 22, Point::new(10, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 22, Point::new(10, 26));
    harness.set_magic_attack(caster, 10, 10);
    harness.learn(caster, Spell::FireBounce);
    harness.ready_all(caster);
    let victim1 = harness.spawn(test_monster(1), Point::new(15, 20));
    let victim2 = harness.spawn(test_monster(1), Point::new(18, 20));
    harness.drain(caster);
    harness.drain(observer);

    harness
        .cast(
            caster,
            Spell::FireBounce,
            MirDirection::Right,
            victim1,
            Point::new(15, 20),
        )
        .expect("cast");
    harness.elapse(750);
    assert_eq!(harness.hp(victim1), 0);
    assert_eq!(
        projectiles(&harness.packets(observer)),
        vec![(Spell::FireBounce, victim1, victim2)],
        "level 0 chain emits one 125"
    );
    assert_eq!(harness.hp(victim2), 1);
    harness.elapse(150);
    assert_eq!(harness.hp(victim2), 0);
    harness.elapse(1_000);
    assert!(
        projectiles(&harness.packets(observer)).is_empty(),
        "level 0 chain stops after level + 1 projectiles"
    );

    // (b) 无候选：受害者是怪物但半径 3 内没有可攻击对象 → 只命中一次、0 条 125。
    let mut harness = Harness::new(5);
    let caster = harness.add_player(1, MirClass::Wizard, 22, Point::new(10, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 22, Point::new(15, 28));
    harness.set_magic_attack(caster, 10, 10);
    harness.learn(caster, Spell::FireBounce);
    harness.set_magic_level(caster, Spell::FireBounce, 3);
    harness.ready_all(caster);
    let lonely = harness.spawn(test_monster(100), Point::new(15, 20));
    harness.drain(caster);
    harness.drain(observer);

    harness
        .cast(
            caster,
            Spell::FireBounce,
            MirDirection::Right,
            lonely,
            Point::new(15, 20),
        )
        .expect("cast");
    harness.elapse(750);
    assert_eq!(harness.hp(lonely), 74, "only the first hit lands");
    harness.elapse(2_000);
    assert!(
        projectiles(&harness.packets(observer)).is_empty(),
        "a monster victim with no nearby candidate stops the chain"
    );
}

// ---------------------------------------------------------------------------
// 6. 不发 126/149；Player 候选被接受，非 Monster 受害者不续跳
// ---------------------------------------------------------------------------

#[test]
fn fire_bounce_does_not_emit_126_or_149_and_chain_accepts_player_hero_candidates() {
    let mut harness = Harness::new(13);
    let caster = harness.add_player(1, MirClass::Wizard, 22, Point::new(10, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 22, Point::new(15, 28));
    let player = harness.add_player(3, MirClass::Wizard, 22, Point::new(18, 20));
    harness.set_attack_mode(caster, AttackMode::All);
    harness.set_magic_attack(caster, 10, 10);
    harness.learn(caster, Spell::FireBounce);
    harness.set_magic_level(caster, Spell::FireBounce, 3);
    harness.ready_all(caster);
    let victim = harness.spawn(test_monster(100), Point::new(15, 20));
    harness.drain(caster);
    harness.drain(observer);

    let player_hp_before = harness.player_hp(player);
    harness
        .cast(
            caster,
            Spell::FireBounce,
            MirDirection::Right,
            victim,
            Point::new(15, 20),
        )
        .expect("cast");
    harness.elapse(750);
    assert_eq!(harness.hp(victim), 74, "monster victim took the first hit");
    let first_packets = harness.packets(observer);
    assert_eq!(
        projectiles(&first_packets),
        vec![(Spell::FireBounce, victim, player)],
        "the attackable player within radius 3 is accepted as the next target"
    );

    // 下一跳 dist 3 → 150ms：玩家吃到 MAC 伤害后链路停止（非 Monster 受害者不续跳）。
    harness.elapse(150);
    assert!(
        harness.player_hp(player) < player_hp_before,
        "the player candidate takes the next hit"
    );
    let mut all_packets = first_packets;
    all_packets.extend(harness.packets(observer));
    all_packets.extend(harness.packets(caster));
    harness.elapse(2_000);
    all_packets.extend(harness.packets(observer));
    all_packets.extend(harness.packets(caster));

    assert_eq!(
        projectiles(&all_packets),
        vec![(Spell::FireBounce, victim, player)],
        "a non-monster victim stops the chain after exactly one 125"
    );
    assert_no_forbidden_packets(&all_packets);
    assert_eq!(
        harness.hp(victim),
        74,
        "the monster victim is not hit again"
    );
    assert!(
        harness.world.player(player).is_some(),
        "player victim stays in the world"
    );
}
