//! 第 2 轮新增技能：火墙 / 雷电 / 瞬息移动 / 隐身 / 召唤骷髅与神兽 / 施毒。
//!
//! 全部用例确定性：
//! * 世界随机数固定种子；
//! * 时间用 [`World::advance_clock`] 快进，不 sleep；
//! * 怪物定时器按用例需要清零（会思考）或设成 `i64::MAX`（不动）。
//!
//! 每条断言都尽量对应 Crystal 的一处行为，注释里标了出处。

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{
    BuffType, ItemType, MirClass, MirDirection, MirGender, Monster as MonsterImage, PoisonType,
    RequiredClass, RequiredType, Spell,
};
use mir_db::{MagicInfo, MapInfo, MirDb};
use mir_protocol::ServerPacket;
use mir_server::MonsterKind;
use mir_server::combat::{self, CombatStats};
use mir_server::gamedata::GameData;
use mir_server::magic::{CastReport, MagicError};
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
            sender,
        });
        if let Some(player) = self.world.player_mut(object_id) {
            player.regen_time = i64::MAX;
        }
        self.receivers.insert(object_id, receiver);
        object_id
    }

    /// 不思考的测试怪（火墙/雷电的目标）。
    fn spawn_passive(&mut self, kind: MonsterKind, position: Point) -> u32 {
        let object_id = self.world.spawn_monster(MAP, kind, position);
        if let Some(monster) = self.world.monster_mut(object_id) {
            monster.operate_time = i64::MAX;
            monster.regen_time = i64::MAX;
        }
        object_id
    }

    /// 会思考的测试怪（中毒结算需要 `tick_monsters` 真的轮到它）。
    fn spawn_active(&mut self, kind: MonsterKind, position: Point) -> u32 {
        let object_id = self.world.spawn_monster(MAP, kind, position);
        self.wake_monster(object_id);
        object_id
    }

    fn wake_monster(&mut self, object_id: u32) {
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

    fn learn(&mut self, object_id: u32, spell: Spell) {
        self.world
            .learn_magic(object_id, spell)
            .expect("学习应当成功");
    }

    /// 清掉动作/攻击/技能计时与冷却，回满魔法。
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
        player: u32,
        spell: Spell,
        direction: MirDirection,
        target_id: u32,
        location: Point,
    ) -> Result<CastReport, MagicError> {
        self.world
            .cast_magic(player, spell, direction, target_id, location)
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

    fn hp(&self, monster: u32) -> i32 {
        self.world.monster(monster).expect("怪物还在").hp
    }
}

// ---------------------------------------------------------------------------
// 夹具
// ---------------------------------------------------------------------------

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

fn game_data() -> Arc<GameData> {
    game_data_with_maps(Vec::new())
}

fn game_data_with_maps(maps: Vec<MapInfo>) -> Arc<GameData> {
    let db = MirDb {
        magics: vec![
            magic_info(Spell::FireBall, 5, 1_800, 8),
            magic_info(Spell::ThunderBolt, 6, 1_800, 8),
            magic_info(Spell::FireWall, 5, 1_800, 8),
            magic_info(Spell::Lightning, 10, 1_800, 8),
            magic_info(Spell::Teleport, 10, 1_800, 0),
            magic_info(Spell::Hiding, 5, 1_800, 0),
            magic_info(Spell::SummonSkeleton, 10, 1_800, 0),
            magic_info(Spell::SummonShinsu, 10, 1_800, 0),
            magic_info(Spell::Poisoning, 5, 1_800, 8),
        ],
        items: vec![mir_protocol::data::ItemInfo {
            index: 1,
            name: "火球术".to_owned(),
            item_type: ItemType::Book,
            shape: i16::from(Spell::FireBall.value()),
            required_class: RequiredClass::None,
            required_type: RequiredType::Level,
            required_amount: 1,
            ..mir_protocol::data::ItemInfo::default()
        }],
        maps,
        ..MirDb::default()
    };
    Arc::new(GameData::from_db(db, None))
}

fn monster_kind(name: &str, hp: i32, level: u16, cool_eye: u8) -> MonsterKind {
    MonsterKind {
        name: name.to_owned(),
        image: MonsterImage::Oma,
        ai: 0,
        level,
        view_range: 7,
        cool_eye,
        attack_speed_ms: 2_500,
        move_speed_ms: 1_800,
        experience: 0,
        can_push: true,
        stats: CombatStats {
            hp,
            min_dc: 0,
            max_dc: 0,
            min_mac: 0,
            max_mac: 0,
            ..CombatStats::default()
        },
    }
}

// ---------------------------------------------------------------------------
// 火墙（`Map.cs:1101-1183` + `SpellObject.cs:111-135`）
// ---------------------------------------------------------------------------

#[test]
fn fire_wall_spawns_five_cells_and_burns_each_cell_every_two_seconds() {
    let mut harness = Harness::new(3);
    let player = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 30, Point::new(20, 26));
    harness.learn(player, Spell::FireWall);
    harness.ready_all(player);

    // 落点 (20,23)；十字五格 = (20,23)、(20,22)、(21,23)、(20,24)、(19,23)。
    let cells = [
        Point::new(20, 23),
        Point::new(20, 22),
        Point::new(21, 23),
        Point::new(20, 24),
        Point::new(19, 23),
    ];
    let monsters: Vec<u32> = cells
        .iter()
        .map(|cell| harness.spawn_passive(monster_kind("火墙靶", 10_000, 1, 0), *cell))
        .collect();
    harness.packets(player);
    harness.packets(observer);

    let report = harness
        .cast(
            player,
            Spell::FireWall,
            MirDirection::Down,
            0,
            Point::new(20, 23),
        )
        .expect("施法成功");
    assert_eq!(report.damage, Some(report.damage.expect("火墙有伤害")));
    assert!(
        harness.world.spell_fields().is_empty(),
        "500ms 前还没有火墙"
    );

    // 500ms 后落地 + 首跳立即（`SpellObject.cs:111-112` 的 `TickTime` 初值 0）。
    harness.elapse(500);
    let fields = harness.world.spell_fields();
    assert_eq!(fields.len(), 5, "一次施法铺 5 格");
    let cast_id = fields[0].cast_id;
    assert!(fields.iter().all(|field| field.cast_id == cast_id));
    let mut centers: Vec<(i32, i32)> = fields
        .iter()
        .map(|field| (field.center.x, field.center.y))
        .collect();
    centers.sort_unstable();
    assert_eq!(
        centers,
        vec![(19, 23), (20, 22), (20, 23), (20, 24), (21, 23)]
    );
    for monster in &monsters {
        assert!(harness.hp(*monster) < 10_000, "五格里的怪都应吃到首跳伤害");
    }
    // 周围玩家收到 5 条 `S.ObjectSpell`（`Spawned()` 后 `BroadcastInfo`，`MapObject.cs:353`）。
    let spell_packets = harness
        .packets(observer)
        .into_iter()
        .filter(|packet| matches!(packet, ServerPacket::ObjectSpell(_)))
        .count();
    assert_eq!(spell_packets, 5);

    // 第二跳必须等到 +2000ms。
    let before: Vec<i32> = monsters.iter().map(|id| harness.hp(*id)).collect();
    harness.elapse(1_999);
    let same: Vec<i32> = monsters.iter().map(|id| harness.hp(*id)).collect();
    assert_eq!(before, same, "1999ms 时不应有第二跳");
    harness.elapse(1);
    let after: Vec<i32> = monsters.iter().map(|id| harness.hp(*id)).collect();
    assert!(
        after
            .iter()
            .zip(&same)
            .all(|(after, before)| after < before),
        "2000ms 时五格都应再吃一跳"
    );

    // 到期整片消失：`ExpireTime = now + (10 + Value / 2) * 1000`（`Map.cs:1133`）。
    let expire_at = harness.world.spell_fields()[0].expire_at;
    let remaining = expire_at - harness.world.now();
    assert!(remaining > 0);
    harness.elapse(remaining + 1);
    assert!(harness.world.spell_fields().is_empty(), "到期后火墙消失");
}

#[test]
fn fire_wall_without_mana_is_rejected() {
    let mut harness = Harness::new(4);
    let player = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    harness.learn(player, Spell::FireWall);
    harness.ready_all(player);
    harness.world.player_mut(player).expect("玩家").mp = 0;

    let result = harness.cast(
        player,
        Spell::FireWall,
        MirDirection::Down,
        0,
        Point::new(20, 21),
    );
    assert!(matches!(result, Err(MagicError::NotEnoughMana { .. })));
    harness.elapse(500);
    assert!(harness.world.spell_fields().is_empty());
}

// ---------------------------------------------------------------------------
// 雷电（`Map.cs:1187-1218`）
// ---------------------------------------------------------------------------

#[test]
fn lightning_pierces_six_cells_hitting_first_target_per_cell() {
    let mut harness = Harness::new(5);
    let player = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    harness.learn(player, Spell::Lightning);
    harness.ready_all(player);

    let first = harness.spawn_passive(monster_kind("一号", 5_000, 1, 0), Point::new(21, 20));
    let second_same_cell =
        harness.spawn_passive(monster_kind("二号", 5_000, 1, 0), Point::new(22, 20));
    let third_same_cell =
        harness.spawn_passive(monster_kind("三号", 5_000, 1, 0), Point::new(22, 20));
    let fourth = harness.spawn_passive(monster_kind("四号", 5_000, 1, 0), Point::new(24, 20));
    let beyond = harness.spawn_passive(monster_kind("六格外", 5_000, 1, 0), Point::new(27, 20));

    harness
        .cast(
            player,
            Spell::Lightning,
            MirDirection::Right,
            0,
            Point::new(0, 0),
        )
        .expect("施法成功");
    // 固定 500ms 延迟（`HumanObject.cs:4213`）。
    harness.elapse(400);
    assert_eq!(harness.hp(first), 5_000);
    harness.elapse(101);

    assert!(harness.hp(first) < 5_000, "第一格命中");
    assert!(harness.hp(second_same_cell) < 5_000, "第二格命中");
    assert_eq!(
        harness.hp(third_same_cell),
        5_000,
        "同一格只有第一个可攻击对象被命中（内层 break）"
    );
    assert!(harness.hp(fourth) < 5_000, "命中后继续贯穿（没有遇阻停止）");
    assert_eq!(harness.hp(beyond), 5_000, "只贯穿 6 格");
}

#[test]
fn lightning_without_mana_is_rejected() {
    let mut harness = Harness::new(6);
    let player = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    harness.learn(player, Spell::Lightning);
    harness.ready_all(player);
    harness.world.player_mut(player).expect("玩家").mp = 0;

    let result = harness.cast(
        player,
        Spell::Lightning,
        MirDirection::Right,
        0,
        Point::new(0, 0),
    );
    assert!(matches!(result, Err(MagicError::NotEnoughMana { .. })));
}

// ---------------------------------------------------------------------------
// 瞬息移动（`HumanObject.cs:6095-6109`、`PlayerObject.cs:4633-4648`）
// ---------------------------------------------------------------------------

#[test]
fn teleport_moves_the_caster_at_200ms_and_applies_mana_penalty() {
    let mut harness = Harness::new(7);
    let player = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    harness.learn(player, Spell::Teleport);
    harness.ready_all(player);
    let mp_before = harness.world.player(player).expect("玩家").mp;

    let report = harness
        .cast(
            player,
            Spell::Teleport,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施法成功");
    // 200ms 前不移动（`HumanObject.cs:3539`）。
    harness.elapse(100);
    assert_eq!(
        harness.world.player(player).expect("玩家").position,
        Point::new(20, 20)
    );
    harness.elapse(101);
    let position = harness.world.player(player).expect("玩家").position;
    assert_ne!(position, Point::new(20, 20), "应该随机传走");
    assert_eq!(
        harness.world.player(player).expect("玩家").mp,
        mp_before - report.cost
    );
    // 成功传送后 30 秒内传送耗蓝 +30%（`HumanObject.cs:6106`、`:3379-3385`）。
    harness.ready_all(player);
    let second = harness
        .cast(
            player,
            Spell::Teleport,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("第二次施法成功");
    assert_eq!(second.cost, 10 + 10 * 30 / 100, "0 级传送 10 蓝，惩罚后 13");
}

#[test]
fn teleport_without_mana_is_rejected_and_keeps_position() {
    let mut harness = Harness::new(8);
    let player = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    harness.learn(player, Spell::Teleport);
    harness.ready_all(player);
    harness.world.player_mut(player).expect("玩家").mp = 0;

    let result = harness.cast(
        player,
        Spell::Teleport,
        MirDirection::Down,
        0,
        Point::new(0, 0),
    );
    assert!(matches!(result, Err(MagicError::NotEnoughMana { .. })));
    harness.elapse(500);
    assert_eq!(
        harness.world.player(player).expect("玩家").position,
        Point::new(20, 20)
    );
}

// ---------------------------------------------------------------------------
// 隐身（`HumanObject.cs:4489-4498`、`:6133-6145`、`:2460-2463`、`:2540-2545`）
// ---------------------------------------------------------------------------

#[test]
fn hiding_hides_from_monsters_and_breaks_on_walking_only() {
    let mut harness = Harness::new(9);
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    // 观察者放在玩家视野内、但不在怪的索敌视野内（否则怪会去打观察者）。
    let observer = harness.add_player(2, MirClass::Taoist, 30, Point::new(10, 20));
    harness.learn(player, Spell::Hiding);
    harness.ready_all(player);
    let monster =
        harness.spawn_active(monster_kind("隐身测试怪", 1_000, 30, 0), Point::new(20, 25));
    harness.packets(observer);

    harness
        .cast(
            player,
            Spell::Hiding,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施法成功");
    harness.elapse(500);
    assert!(harness.world.player(player).expect("玩家").magic.hidden);
    let hidden_packet = harness
        .packets(observer)
        .into_iter()
        .find_map(|packet| match packet {
            ServerPacket::ObjectHidden(packet) => Some(packet),
            _ => None,
        })
        .expect("周围玩家应收到 S.ObjectHidden");
    assert!(hidden_packet.hidden);
    assert_eq!(hidden_packet.object_id, player);
    // 怪物不再索敌（`MonsterObject.cs:1974`）。
    harness.elapse(3_000);
    assert_eq!(harness.world.monster(monster).expect("怪").target, None);
    // 攻击不解除隐身（Crystal 只在走/跑时解除）。
    let front = harness.spawn_passive(monster_kind("前排怪", 1_000, 1, 0), Point::new(20, 21));
    harness.ready_all(player);
    harness
        .world
        .attack(player, MirDirection::Down)
        .expect("攻击");
    harness.elapse(300);
    assert!(harness.hp(front) < 1_000, "攻击应命中");
    assert!(
        harness.world.player(player).expect("玩家").magic.hidden,
        "攻击不解除隐身"
    );
    // 走路解除（`HumanObject.cs:2460-2463`）。
    harness.ready_all(player);
    harness.world.walk(player, MirDirection::Up).expect("走路");
    assert!(!harness.world.player(player).expect("玩家").magic.hidden);
    // 本人要收到 `S.RemoveBuff{Hiding}` 清客户端 buff 列表（`HumanObject.cs:452-458`）。
    assert!(
        harness.packets(player).iter().any(|packet| matches!(
            packet,
            ServerPacket::RemoveBuff(remove) if remove.type_ == BuffType::Hiding
        )),
        "解除隐身应发 S.RemoveBuff"
    );

    // 隐身到期同样解除。
    harness.ready_all(player);
    harness
        .cast(
            player,
            Spell::Hiding,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施法成功");
    harness.elapse(500);
    assert!(harness.world.player(player).expect("玩家").magic.hidden);
    let until = harness
        .world
        .player(player)
        .expect("玩家")
        .magic
        .hidden_until;
    let remaining = until - harness.world.now();
    assert!(remaining > 0);
    harness.elapse(remaining + 1);
    assert!(!harness.world.player(player).expect("玩家").magic.hidden);
}

#[test]
fn hiding_is_seen_by_cool_eye_monster_of_equal_level() {
    let mut harness = Harness::new(10);
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    harness.learn(player, Spell::Hiding);
    harness.ready_all(player);
    // `CoolEye` 且怪物等级 >= 玩家等级 → 可以看见隐身（`MonsterObject.cs:1974`）。
    let monster = harness.spawn_active(monster_kind("破隐怪", 1_000, 30, 100), Point::new(20, 25));
    harness
        .cast(
            player,
            Spell::Hiding,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施法成功");
    harness.elapse(500);
    harness.elapse(3_000);
    assert_eq!(
        harness.world.monster(monster).expect("怪").target,
        Some(player),
        "破隐怪应能锁定隐身玩家"
    );
}

#[test]
fn hiding_without_mana_is_rejected() {
    let mut harness = Harness::new(11);
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    harness.learn(player, Spell::Hiding);
    harness.ready_all(player);
    harness.world.player_mut(player).expect("玩家").mp = 0;
    let result = harness.cast(
        player,
        Spell::Hiding,
        MirDirection::Down,
        0,
        Point::new(0, 0),
    );
    assert!(matches!(result, Err(MagicError::NotEnoughMana { .. })));
    assert!(!harness.world.player(player).expect("玩家").magic.hidden);
}

// ---------------------------------------------------------------------------
// 召唤（`HumanObject.cs:4399-4488`、`Map.cs:914-933`）
// ---------------------------------------------------------------------------

#[test]
fn summon_skeleton_creates_a_pet_that_attacks_the_masters_target() {
    let mut harness = Harness::new(12);
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    harness.learn(player, Spell::SummonSkeleton);
    harness.ready_all(player);

    harness
        .cast(
            player,
            Spell::SummonSkeleton,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施法成功");
    assert!(harness.world.pets_of(player).is_empty(), "500ms 后才召唤");
    harness.elapse(500);
    let pets = harness.world.pets_of(player);
    assert_eq!(pets.len(), 1, "召唤出一只骷髅");
    let pet = pets[0];
    let pet_monster = harness.world.monster(pet).expect("召唤物").clone();
    assert_eq!(pet_monster.master, Some(player));
    assert_eq!(pet_monster.summon_spell, Some(Spell::SummonSkeleton));
    assert_eq!(pet_monster.position, Point::new(20, 21), "落在主人前方");
    assert_eq!(pet_monster.max_pet_level, 4);

    // 主人最近的目标 → 召唤物主动攻击（任务要求；Crystal 的召唤物自己索敌，
    // `MonsterObject.cs:1918-1997`）。
    let target = harness.spawn_passive(monster_kind("召唤靶", 5_000, 1, 0), Point::new(20, 22));
    harness
        .world
        .player_mut(player)
        .expect("玩家")
        .magic
        .last_target = Some(target);
    harness.wake_monster(pet);
    harness.elapse(0);
    assert_eq!(
        harness.world.monster(pet).expect("召唤物").pet_target,
        Some(target)
    );
    harness.elapse(300);
    assert!(harness.hp(target) < 5_000, "召唤物应打到主人的目标");
}

#[test]
fn summoning_the_same_pet_again_recalls_instead_of_creating_a_second() {
    let mut harness = Harness::new(13);
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    harness.learn(player, Spell::SummonSkeleton);
    harness.learn(player, Spell::SummonShinsu);
    harness.ready_all(player);

    harness
        .cast(
            player,
            Spell::SummonSkeleton,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施法成功");
    harness.elapse(500);
    let skeleton = harness.world.pets_of(player)[0];
    // 把骷髅丢远，再召唤一次同名技能：只召回、不新增（`HumanObject.cs:4408-4415`）。
    harness
        .world
        .place_monster(skeleton, MAP, Point::new(40, 40));
    harness.ready_all(player);
    harness
        .cast(
            player,
            Spell::SummonSkeleton,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施法成功");
    harness.elapse(500);
    assert_eq!(harness.world.pets_of(player).len(), 1);
    assert_eq!(
        harness.world.monster(skeleton).expect("骷髅").position,
        Point::new(20, 19),
        "召回后落在主人身后（Master.Back）"
    );

    // 两种召唤物共享上限 2（`HumanObject.cs:4417`、`:4466`）。
    harness.ready_all(player);
    harness
        .cast(
            player,
            Spell::SummonShinsu,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施法成功");
    harness.elapse(500);
    assert_eq!(harness.world.pets_of(player).len(), 2);
}

#[test]
fn pets_disappear_when_the_master_logs_out_or_dies() {
    let mut harness = Harness::new(14);
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    harness.learn(player, Spell::SummonSkeleton);
    harness.ready_all(player);
    harness
        .cast(
            player,
            Spell::SummonSkeleton,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施法成功");
    harness.elapse(500);
    let pet = harness.world.pets_of(player)[0];
    harness.world.leave(player);
    assert!(harness.world.pets_of(player).is_empty());
    assert!(harness.world.monster(pet).is_none(), "下线时召唤物移除");

    // 主人死亡：召唤物一起死（`PlayerObject.cs:629-633`）。
    let player = harness.add_player(2, MirClass::Taoist, 30, Point::new(30, 30));
    harness.learn(player, Spell::SummonSkeleton);
    harness.ready_all(player);
    harness
        .cast(
            player,
            Spell::SummonSkeleton,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施法成功");
    harness.elapse(500);
    let pet = harness.world.pets_of(player)[0];
    harness.world.player_mut(player).expect("玩家").hp = 1;
    let killer = harness.spawn_active(monster_kind("杀手", 1_000, 30, 0), Point::new(30, 29));
    if let Some(monster) = harness.world.monster_mut(killer) {
        monster.target = Some(player);
        monster.stats.min_dc = 100;
        monster.stats.max_dc = 100;
        monster.stats.accuracy = 100;
    }
    harness.elapse(300);
    harness.elapse(300);
    assert!(harness.world.player(player).expect("玩家").dead);
    assert!(
        harness
            .world
            .monster(pet)
            .is_none_or(|monster| monster.dead),
        "主人死亡时召唤物一起死"
    );
}

#[test]
fn summoned_pets_do_not_grant_experience() {
    let mut harness = Harness::new(15);
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    harness.learn(player, Spell::SummonSkeleton);
    harness.learn(player, Spell::FireBall);
    harness.ready_all(player);
    harness
        .cast(
            player,
            Spell::SummonSkeleton,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施法成功");
    harness.elapse(500);
    let pet = harness.world.pets_of(player)[0];
    harness.world.monster_mut(pet).expect("召唤物").hp = 1;
    let experience = harness.world.player(player).expect("玩家").experience;

    harness.ready_all(player);
    harness
        .cast(
            player,
            Spell::FireBall,
            MirDirection::Down,
            pet,
            Point::new(0, 0),
        )
        .expect("施法成功");
    harness.elapse(600);
    assert!(
        harness
            .world
            .monster(pet)
            .is_none_or(|monster| monster.dead),
        "召唤物被击杀"
    );
    assert_eq!(
        harness.world.player(player).expect("玩家").experience,
        experience,
        "召唤物死亡不给主人经验（Crystal `Master == null` 才 WinExp，`MonsterObject.cs:982`）"
    );
}

// ---------------------------------------------------------------------------
// 施毒（`HumanObject.cs:4365-4378`、`:6041-6077`、`MonsterObject.cs:1453-1569`）
// ---------------------------------------------------------------------------

#[test]
fn poisoning_applies_green_then_red_and_ticks_each_two_seconds() {
    let mut harness = Harness::new(16);
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    harness.learn(player, Spell::Poisoning);
    harness.ready_all(player);
    let target = harness.spawn_active(monster_kind("毒靶", 10_000, 1, 0), Point::new(25, 20));

    harness
        .cast(
            player,
            Spell::Poisoning,
            MirDirection::Right,
            target,
            Point::new(0, 0),
        )
        .expect("施法成功");
    harness.elapse(500);
    let poisons = &harness.world.monster(target).expect("毒靶").poisons;
    assert_eq!(poisons.len(), 1);
    assert_eq!(poisons[0].kind, PoisonType::Green);
    let value = poisons[0].value;
    assert!(value > 0);
    assert!(poisons[0].duration > 0);

    // 绿毒每 2000ms 扣一次血（`MonsterObject.cs:1493-1514`）。
    let hp_before = harness.hp(target);
    harness.elapse(2_000);
    assert_eq!(harness.hp(target), hp_before - value);

    // 第二次施毒：已经有绿毒 → 施红毒（本作玩法决策，Crystal 由毒药物品 Shape 决定）。
    harness.ready_all(player);
    harness
        .cast(
            player,
            Spell::Poisoning,
            MirDirection::Right,
            target,
            Point::new(0, 0),
        )
        .expect("施法成功");
    harness.elapse(500);
    let monster = harness.world.monster(target).expect("毒靶");
    assert_eq!(monster.poisons.len(), 2);
    assert!(monster.poisons.iter().any(|p| p.kind == PoisonType::Red));
    // 红毒降防：怪物 `ArmourRate -= 0.5`（`MonsterObject.cs:1536-1537`）。
    assert!((monster.armour_rate() - 0.5).abs() < f32::EPSILON);

    // 怪物口径：最后一跳不结算（`MonsterObject.cs:1476-1491`）。
    if let Some(monster) = harness.world.monster_mut(target) {
        let green = monster
            .poisons
            .iter_mut()
            .find(|poison| poison.kind == PoisonType::Green)
            .expect("绿毒");
        green.duration = green.ticks + 1;
        green.tick_at = 0;
    }
    harness.wake_monster(target);
    let hp_before = harness.hp(target);
    harness.elapse(1);
    assert_eq!(harness.hp(target), hp_before, "最后一跳不应扣血");
    assert!(
        !harness
            .world
            .monster(target)
            .expect("毒靶")
            .poisons
            .iter()
            .any(|poison| poison.kind == PoisonType::Green),
        "最后一跳后绿毒移除"
    );
}

#[test]
fn poisoning_requires_a_target_and_does_not_spend_mana() {
    let mut harness = Harness::new(17);
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    harness.learn(player, Spell::Poisoning);
    harness.ready_all(player);
    let mp = harness.world.player(player).expect("玩家").mp;
    let result = harness.cast(
        player,
        Spell::Poisoning,
        MirDirection::Right,
        0,
        Point::new(0, 0),
    );
    assert!(matches!(result, Err(MagicError::InvalidTarget)));
    assert_eq!(harness.world.player(player).expect("玩家").mp, mp);
}

// ---------------------------------------------------------------------------
// 雷电术（ThunderBolt）的不死系加成与固定 500ms 延迟
// ---------------------------------------------------------------------------

fn thunderbolt_damage(seed: u64, undead: bool) -> i32 {
    let mut harness = Harness::new(seed);
    let player = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    harness.learn(player, Spell::ThunderBolt);
    harness.ready_all(player);
    let target = harness.spawn_passive(monster_kind("不死系靶", 5_000, 1, 0), Point::new(21, 20));
    if undead {
        harness.world.monster_mut(target).expect("靶").undead = true;
    }
    let report = harness
        .cast(
            player,
            Spell::ThunderBolt,
            MirDirection::Right,
            target,
            Point::new(0, 0),
        )
        .expect("施法成功");
    // 固定 500ms（`HumanObject.cs:4128`），不是「距离 × 50 + 500」。
    harness.elapse(400);
    assert_eq!(harness.hp(target), 5_000);
    harness.elapse(101);
    assert!(harness.hp(target) < 5_000);
    report.damage.expect("有伤害")
}

#[test]
fn thunderbolt_gets_fifty_percent_bonus_against_undead() {
    let normal = thunderbolt_damage(18, false);
    let undead = thunderbolt_damage(18, true);
    assert!(normal > 0);
    assert_eq!(
        undead,
        normal * 3 / 2,
        "`if (target.Undead) damage = (int)(damage * 1.5F);`（`HumanObject.cs:4126`）"
    );
}

// ---------------------------------------------------------------------------
// 地图限制：火墙数量上限 / 禁止召唤
// ---------------------------------------------------------------------------

fn map_with(file_name: &str) -> MapInfo {
    MapInfo {
        file_name: file_name.to_owned(),
        ..MapInfo::default()
    }
}

#[test]
fn fire_wall_limit_recycles_the_oldest_cast() {
    let mut map = map_with(MAP);
    map.firewall_limit = true;
    map.firewall_count = 1;
    let mut harness = Harness::with_data(20, game_data_with_maps(vec![map]));
    let player = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    harness.learn(player, Spell::FireWall);
    harness.ready_all(player);

    harness
        .cast(
            player,
            Spell::FireWall,
            MirDirection::Down,
            0,
            Point::new(20, 23),
        )
        .expect("第一次施法");
    harness.elapse(500);
    let first: Vec<u32> = harness
        .world
        .spell_fields()
        .iter()
        .map(|field| field.cast_id)
        .collect();
    assert_eq!(first.len(), 5);

    // 第二次施法在**施法瞬间**回收最老实例（`HumanObject.cs:4151-4175`、`:4183-4207`）。
    harness.ready_all(player);
    harness
        .cast(
            player,
            Spell::FireWall,
            MirDirection::Down,
            0,
            Point::new(21, 23),
        )
        .expect("第二次施法");
    assert!(
        harness.world.spell_fields().is_empty(),
        "超过 FireWallCount 时施法瞬间回收最老一次施法的全部格子"
    );
    harness.elapse(500);
    let fields = harness.world.spell_fields();
    assert_eq!(fields.len(), 5, "新一次施法铺 5 格");
    assert!(
        fields.iter().all(|field| !first.contains(&field.cast_id)),
        "旧实例的格子不应残留"
    );
}

#[test]
fn summoning_on_a_no_pets_map_is_rejected_after_spending_mana() {
    let mut map = map_with(MAP);
    map.no_pets = true;
    let mut harness = Harness::with_data(21, game_data_with_maps(vec![map]));
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    harness.learn(player, Spell::SummonSkeleton);
    harness.ready_all(player);
    let mp = harness.world.player(player).expect("玩家").mp;

    // Crystal 在召唤分支里才检查 NoPets，蓝已经扣掉（`HumanObject.cs:4401-4405`）。
    harness
        .cast(
            player,
            Spell::SummonSkeleton,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施法动作本身成功");
    harness.elapse(500);
    assert!(harness.world.pets_of(player).is_empty());
    assert!(harness.world.player(player).expect("玩家").mp < mp);
    assert!(
        harness.packets(player).iter().any(
            |packet| matches!(packet, ServerPacket::Chat(chat) if chat.message.contains("不能召唤"))
        ),
        "应提示地图不能召唤"
    );
}

#[test]
fn undead_flag_is_bridged_from_monster_info_at_spawn() {
    // `MonsterInfo.Undead` 传到 `Monster` 实例（`HumanObject.cs:4126` 依赖它）。
    let info = mir_db::MonsterInfo {
        index: 0,
        name: "BoneFamiliar".to_owned(),
        undead: true,
        ..mir_db::MonsterInfo::default()
    };
    let data = Arc::new(GameData::from_db(
        MirDb {
            monsters: vec![info],
            ..MirDb::default()
        },
        None,
    ));
    let mut harness = Harness::with_data(23, data);
    let kind = harness
        .world
        .game_data()
        .and_then(|data| data.monster(0))
        .expect("怪物模板")
        .clone();
    let monster = harness.world.spawn_monster(MAP, kind, Point::new(5, 5));
    assert!(harness.world.monster(monster).expect("怪").undead);
}

#[test]
fn recasting_hiding_while_hidden_only_refreshes_duration() {
    let mut harness = Harness::new(22);
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Taoist, 30, Point::new(10, 20));
    harness.learn(player, Spell::Hiding);
    harness.ready_all(player);
    harness
        .cast(
            player,
            Spell::Hiding,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施法成功");
    harness.elapse(500);
    let first_until = harness
        .world
        .player(player)
        .expect("玩家")
        .magic
        .hidden_until;
    harness.packets(player);
    harness.packets(observer);

    harness.ready_all(player);
    harness
        .cast(
            player,
            Spell::Hiding,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施法成功");
    harness.elapse(500);
    assert!(
        harness
            .world
            .player(player)
            .expect("玩家")
            .magic
            .hidden_until
            > first_until,
        "重复施法重置时长"
    );
    assert!(
        !harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::AddBuff(_))),
        "已隐身时不再重发 S.AddBuff"
    );
    assert!(
        !harness
            .packets(observer)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectHidden(_))),
        "已隐身时不再重发 S.ObjectHidden"
    );
}
