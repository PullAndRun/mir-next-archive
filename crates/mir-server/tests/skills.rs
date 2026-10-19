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
    AttackMode, BuffType, ItemType, MirClass, MirDirection, MirGender, Monster as MonsterImage,
    PetMode, PoisonType, RequiredClass, RequiredType, Spell,
};
use mir_core::{Language, TextKey};
use mir_db::{MagicInfo, MapInfo, MirDb};
use mir_protocol::ServerPacket;
use mir_server::MonsterKind;
use mir_server::combat::{self, CombatStats};
use mir_server::gamedata::GameData;
use mir_server::magic::{CastReport, MagicError};
use mir_server::world::{Player, World};
use mir_world::Map;
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

    /// 召唤一只骷髅并返回它的对象 ID（500ms 延迟已经走完）。
    fn summon_skeleton(&mut self, player: u32) -> u32 {
        self.learn(player, Spell::SummonSkeleton);
        self.ready_all(player);
        self.cast(
            player,
            Spell::SummonSkeleton,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("施法动作成功");
        self.elapse(500);
        self.world
            .pets_of(player)
            .first()
            .copied()
            .expect("应召唤出一只骷髅")
    }

    /// 把 `target` 的发包通道接到 `source` 的接收端上。
    ///
    /// 用于断言**跨收包方**的包序：`S.Magic` 只发给施法者、`S.ObjectMagic` 只广播给
    /// 周围玩家；让两者进入同一个 mpsc 队列后，就能验证发送顺序。
    fn share_channel(&mut self, source: u32, target: u32) {
        let sender = self.world.player(source).expect("源玩家").sender.clone();
        self.world.player_mut(target).expect("目标玩家").sender = sender;
    }
}

/// 60×60 可走地形（v0 地图格式的合成字节）；`walls` 里的格子设为高墙。
///
/// `Map` 的 `attribute` 字段类型是私有枚举，集成测试无法直接构造 `MapCell`；
/// 这里走公开的 [`Map::from_bytes_named`]（v0：52 字节头 + 每格 12 字节，
/// 列优先 `offset = 52 + (x * Height + y) * 12`，`o+0` 的 bit15 = 高墙，
/// 见 `crates/mir-world/src/map.rs` 的 v0 解析）。
fn synthetic_map(walls: &[Point]) -> Arc<Map> {
    const V0_HEADER: usize = 52;
    const V0_STRIDE: usize = 12;
    let width = 60_usize;
    let height = 60_usize;
    let mut data = vec![0_u8; V0_HEADER + V0_STRIDE * width * height];
    data[0..2].copy_from_slice(&u16::try_from(width).unwrap_or(0).to_le_bytes());
    data[2..4].copy_from_slice(&u16::try_from(height).unwrap_or(0).to_le_bytes());
    for wall in walls {
        if wall.x < 0 || wall.y < 0 || wall.x >= 60 || wall.y >= 60 {
            continue;
        }
        let index =
            usize::try_from(wall.x).unwrap_or(0) * height + usize::try_from(wall.y).unwrap_or(0);
        let offset = V0_HEADER + index * V0_STRIDE;
        // `o+0` 的 bit15 → `HighWall`（`WallRule::OverrideWords`）。
        data[offset..offset + 2].copy_from_slice(&0x8000_u16.to_le_bytes());
    }
    Arc::new(Map::from_bytes_named(&data, MAP).expect("合成 v0 地图应能解析"))
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
            magic_info(Spell::GreatFireBall, 10, 1_800, 8),
            magic_info(Spell::SoulFireBall, 10, 1_800, 8),
            magic_info(Spell::FireBang, 10, 1_800, 8),
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

/// 魔法怪：给足 MC/DC/准确，保证命中与伤害可断言。
fn magic_kind(name: &str, ai: u8) -> MonsterKind {
    let mut kind = monster_kind(name, 5_000, 10, 0);
    kind.ai = ai;
    kind.stats.min_mc = 80;
    kind.stats.max_mc = 80;
    kind.stats.min_dc = 80;
    kind.stats.max_dc = 80;
    kind.stats.accuracy = 100;
    kind
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
    let damage = report.damage.expect("火墙有伤害");
    assert!(damage > 0, "火墙伤害应为正数，实际 {damage}");
    assert!(
        harness.world.spell_fields().is_empty(),
        "500ms 前还没有火墙"
    );

    // 500ms 后落地 + 首跳立即（`SpellObject.cs:111-112` 的 `TickTime` 初值 0）。
    harness.elapse(500);
    let fields = harness.world.spell_fields();
    assert_eq!(fields.len(), 5, "一次施法铺 5 格");
    assert!(
        fields.iter().all(|field| field.damage == damage),
        "五格共享施法时抽到的 Value（`Map.cs:1129-1139`）"
    );
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
    // 路径第 3 格 (23,20) 是高墙：Crystal `Map.ValidPoint` 失败只是 `continue`，
    // 下一格从墙格继续推进（`Map.cs:1194-1198`），不会中断贯穿。
    harness
        .world
        .insert_map(MAP, synthetic_map(&[Point::new(23, 20)]));
    assert!(
        !harness
            .world
            .map_of(MAP)
            .expect("合成地图")
            .walkable(23, 20),
        "合成地图的 (23,20) 应是不可走格，否则本用例覆盖不到 continue"
    );
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
    assert!(
        harness.hp(fourth) < 5_000,
        "第 3 格是高墙 → continue 后第 4 格仍被命中（没有遇阻停止）"
    );
    assert_eq!(harness.hp(beyond), 5_000, "只贯穿 6 格");
}

#[test]
fn lightning_sends_magic_to_caster_before_object_magic_broadcast() {
    let mut harness = Harness::new(24);
    let player = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 30, Point::new(20, 24));
    // 把观察者的发包通道接到施法者的接收端，合并两个收包方以断言发送先后。
    harness.share_channel(player, observer);
    harness.learn(player, Spell::Lightning);
    harness.ready_all(player);
    harness.packets(player);

    harness
        .cast(
            player,
            Spell::Lightning,
            MirDirection::Right,
            0,
            Point::new(0, 0),
        )
        .expect("施法成功");

    // 全部在 `cast_magic` 内同步发出（伤害要等 500ms，与包序无关）。
    let packets = harness.packets(player);
    let magic_index = packets.iter().position(
        |packet| matches!(packet, ServerPacket::Magic(magic) if magic.spell == Spell::Lightning),
    );
    let object_magic_index = packets.iter().position(|packet| {
        matches!(
            packet,
            ServerPacket::ObjectMagic(object)
                if object.spell == Spell::Lightning && object.object_id == player
        )
    });
    let magic_index = magic_index.expect("施法者应收到 S.Magic");
    let object_magic_index = object_magic_index.expect("旁观者应收到 S.ObjectMagic");
    assert!(
        magic_index < object_magic_index,
        "包序必须是 S.Magic → S.ObjectMagic，实际 {packets:?}"
    );
    // 同一次施法只发一条 `S.Magic`，且 `cast = true`（`HumanObject.cs:3778-3779`）。
    assert_eq!(
        packets
            .iter()
            .filter(|packet| matches!(packet, ServerPacket::Magic(_)))
            .count(),
        1
    );
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

    // 主人打自己的宠物需要 All 模式（Crystal `MonsterObject.cs:2440`）。
    harness.world.player_mut(player).expect("玩家").attack_mode = AttackMode::All;
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
    let expected = mir_core::i18n::text(Language::ZhCn, TextKey::MagicSummonForbidden);
    assert!(
        harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Chat(chat) if chat.message == expected)),
        "应提示地图不能召唤（按中文语言包取值 {expected}）"
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

// ---------------------------------------------------------------------------
// i18n：World 语言接入口（旁置映射，不新增 Player 字段）
// ---------------------------------------------------------------------------

#[test]
fn system_text_uses_the_players_language() {
    let mut map = map_with(MAP);
    map.no_pets = true;
    let mut harness = Harness::with_data(25, game_data_with_maps(vec![map]));
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    harness.world.set_player_language(player, Language::En);
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
        .expect("施法动作本身成功");
    harness.elapse(500);

    let expected = mir_core::i18n::text(Language::En, TextKey::MagicSummonForbidden);
    assert!(
        harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Chat(chat) if chat.message == expected)),
        "英文玩家应收到英文提示（{expected}）"
    );
}

#[test]
fn broadcast_text_renders_per_recipient_language() {
    let mut harness = Harness::new(26);
    let chinese = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    let english = harness.add_player(2, MirClass::Wizard, 30, Point::new(21, 20));
    harness.world.set_player_language(chinese, Language::ZhCn);
    harness.world.set_player_language(english, Language::En);
    harness.packets(chinese);
    harness.packets(english);

    harness
        .world
        .broadcast_text(MAP, TextKey::MagicFirewallLimit, &[]);

    let zh = mir_core::i18n::text(Language::ZhCn, TextKey::MagicFirewallLimit);
    let en = mir_core::i18n::text(Language::En, TextKey::MagicFirewallLimit);
    assert!(
        harness
            .packets(chinese)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Chat(chat) if chat.message == zh)),
        "中文玩家收中文广播"
    );
    assert!(
        harness
            .packets(english)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Chat(chat) if chat.message == en)),
        "英文玩家收英文广播（逐收件人编码）"
    );
}

// ---------------------------------------------------------------------------
// 宠物 ↔ 野生怪（Crystal `MonsterObject.FindTarget` / `Attacked`）
// ---------------------------------------------------------------------------

#[test]
fn wild_monster_targets_and_attacks_a_pet_then_pet_retaliates() {
    let mut harness = Harness::new(31);
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    let pet = harness.summon_skeleton(player);
    let pet_hp = harness.hp(pet);

    let mut kind = monster_kind("野生恶狼", 5_000, 1, 0);
    kind.stats.min_dc = 3;
    kind.stats.max_dc = 3;
    kind.stats.accuracy = 100;
    // 宠物在 (20,21)，玩家在 (20,20)：野怪离宠物更近，应优先锁宠物。
    let wild = harness.spawn_active(kind, Point::new(20, 24));

    harness.elapse(0);
    assert_eq!(
        harness.world.monster(wild).expect("野生怪").target,
        Some(pet),
        "野生怪应优先选中更近的召唤物（Crystal `MonsterObject.cs:1940-1963`、`:2481-2484`）"
    );

    for _ in 0..8 {
        harness.elapse(1_000);
    }
    assert_eq!(
        harness.world.monster(pet).expect("宠物").pet_target,
        Some(wild),
        "宠物被咬后应把攻击者设回 pet_target（Crystal `:2601-2610`）"
    );
    assert!(harness.hp(pet) < pet_hp, "宠物应被野生怪咬到");
    assert!(harness.hp(wild) < 5_000, "宠物应反击野生怪");
}

#[test]
fn wild_monster_switches_target_to_the_pet_that_bites_it() {
    let mut harness = Harness::new(39);
    let master = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    let pet = harness.summon_skeleton(master);
    let kind = monster_kind("被咬的野怪", 5_000, 1, 0);
    // 宠物 (20,21)，野怪 (20,22)：先手动把野怪目标设成主人，模拟「已经锁玩家」。
    let wild = harness.spawn_active(kind, Point::new(20, 22));
    harness.world.monster_mut(wild).expect("野生怪").target = Some(master);
    harness.wake_monster(pet);

    harness.elapse(0);
    harness.elapse(300);
    assert_eq!(
        harness.world.monster(wild).expect("野生怪").target,
        Some(pet),
        "被宠物咬后应无条件改打宠物（Crystal `MonsterObject.cs:2601-2610`）"
    );
}

#[test]
fn pet_death_to_a_wild_monster_does_not_panic_the_master() {
    let mut harness = Harness::new(32);
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    let pet = harness.summon_skeleton(player);
    let max_hp = harness.world.player(player).expect("玩家").stats.hp;

    let mut kind = monster_kind("斩宠者", 5_000, 10, 0);
    kind.stats.min_dc = 100;
    kind.stats.max_dc = 100;
    kind.stats.accuracy = 100;
    let wild = harness.spawn_active(kind, Point::new(20, 22));
    harness.world.monster_mut(wild).expect("野生怪").target = Some(pet);
    harness.world.monster_mut(pet).expect("宠物").hp = 10;

    harness.elapse(0); // 野怪攻击入队（伤害 300ms 后结算）
    harness.elapse(300);
    assert!(
        harness.world.pets_of(player).is_empty(),
        "宠物被打死后应从主人宠物表注销"
    );
    assert!(
        !harness.world.player(player).expect("玩家").dead,
        "主人不应跟着死"
    );
    assert_eq!(harness.world.player(player).expect("玩家").hp, max_hp);

    // 目标死亡后的清理：再跑一小段，野生怪不能把主人打崩。
    harness.elapse(1_000);
    assert!(!harness.world.player(player).expect("玩家").dead);
    assert_ne!(
        harness.world.monster(wild).expect("野生怪").target,
        Some(pet),
        "宠物死亡后野生怪应脱战/换目标"
    );
}

#[test]
fn guards_ignore_pets() {
    let mut harness = Harness::new(33);
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    let pet = harness.summon_skeleton(player);
    let pet_hp = harness.hp(pet);

    let mut guard_kind = monster_kind("守卫", 5_000, 10, 0);
    guard_kind.ai = 6; // Crystal Guard：只打红名主人的宠物
    guard_kind.stats.min_dc = 10;
    guard_kind.stats.max_dc = 10;
    let guard = harness.spawn_active(guard_kind, Point::new(20, 23));

    harness.elapse(0);
    harness.elapse(2_000);
    assert_eq!(
        harness.world.monster(guard).expect("守卫").target,
        None,
        "守卫不应锁定普通玩家的宠物（Crystal `MonsterObject.cs:2471-2475`）"
    );
    assert_eq!(harness.hp(pet), pet_hp);
}

#[test]
fn peaceful_masters_pet_does_not_attack_players() {
    let mut harness = Harness::new(35);
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    let pet = harness.summon_skeleton(player);

    // 和平模式：主人把 last_target 指向另一个玩家，宠物也不出手。
    let other = harness.add_player(2, MirClass::Warrior, 30, Point::new(20, 22));
    let other_hp = harness.world.player(other).expect("另一个玩家").hp;
    {
        let master = harness.world.player_mut(player).expect("玩家");
        master.attack_mode = AttackMode::Peace;
        master.magic.last_target = Some(other);
    }
    harness.world.monster_mut(pet).expect("宠物").pet_target = Some(other);
    harness.wake_monster(pet);
    harness.elapse(0);
    assert_ne!(
        harness.world.monster(pet).expect("宠物").pet_target,
        Some(other),
        "和平模式主人的宠物不应攻击玩家"
    );
    harness.elapse(1_000);
    assert_eq!(
        harness.world.player(other).expect("另一个玩家").hp,
        other_hp
    );
}

#[test]
fn pet_never_targets_its_own_master() {
    let mut harness = Harness::new(34);
    let player = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    let pet = harness.summon_skeleton(player);
    let max_hp = harness.world.player(player).expect("玩家").stats.hp;

    harness
        .world
        .player_mut(player)
        .expect("玩家")
        .magic
        .last_target = Some(player);
    harness.world.monster_mut(pet).expect("宠物").pet_target = Some(player);
    harness.wake_monster(pet);
    harness.elapse(0);
    assert_ne!(
        harness.world.monster(pet).expect("宠物").pet_target,
        Some(player),
        "召唤物永远不能把主人当目标"
    );
    assert_eq!(
        harness.world.monster(pet).expect("宠物").pet_target,
        None,
        "没有其他目标时应回到主人身后"
    );
    assert_eq!(harness.world.player(player).expect("玩家").hp, max_hp);
}

#[test]
fn pet_retaliates_when_attacked_by_a_player() {
    let mut harness = Harness::new(36);
    let master = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    let pet = harness.summon_skeleton(master);
    {
        // 主人 All 模式：宠物才被允许反击玩家（Peace 下 `pet_target_valid` 会拒绝）。
        let player = harness.world.player_mut(master).expect("主人");
        player.attack_mode = AttackMode::All;
        player.attack_time = 0;
        player.action_time = 0;
    }
    let attacker = harness.add_player(3, MirClass::Warrior, 10, Point::new(20, 22));
    {
        let player = harness.world.player_mut(attacker).expect("攻击者");
        player.attack_mode = AttackMode::All;
        player.direction = MirDirection::Up;
        player.attack_time = 0;
        player.action_time = 0;
        player.stats.accuracy = 100;
        player.stats.min_dc = 20;
        player.stats.max_dc = 20;
        player.stats.agility = 0;
        player.stats.min_ac = 0;
        player.stats.max_ac = 0;
    }
    let report = harness
        .world
        .attack(attacker, MirDirection::Up)
        .expect("攻击动作");
    assert!(report.is_some(), "All 模式玩家可以攻击宠物");
    harness.elapse(300);
    assert_eq!(
        harness.world.monster(pet).expect("宠物").pet_target,
        Some(attacker),
        "宠物被玩家攻击后应把 pet_target 设回该玩家（Crystal `MonsterObject.cs:2571-2574`、`:2601-2610`）"
    );
    harness.elapse(5_000);
    assert_eq!(
        harness.world.monster(pet).expect("宠物").pet_target,
        Some(attacker),
        "宠物应持续锁定攻击它的玩家"
    );
    assert!(
        harness.packets(master).iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectAttack(attack) if attack.object_id == pet
        )),
        "宠物应真的出手反击攻击者（S.ObjectAttack）"
    );
}

#[test]
fn peaceful_master_pet_does_not_retaliate_against_players() {
    let mut harness = Harness::new(37);
    let master = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    let pet = harness.summon_skeleton(master);
    harness.world.monster_mut(pet).expect("宠物").hp = 10_000;
    let attacker = harness.add_player(3, MirClass::Warrior, 10, Point::new(20, 22));
    {
        let player = harness.world.player_mut(attacker).expect("攻击者");
        player.attack_mode = AttackMode::All;
        player.direction = MirDirection::Up;
        player.attack_time = 0;
        player.action_time = 0;
        player.stats.accuracy = 100;
        player.stats.min_dc = 20;
        player.stats.max_dc = 20;
        player.stats.agility = 0;
        player.stats.min_ac = 0;
        player.stats.max_ac = 0;
    }
    let attacker_hp = harness.world.player(attacker).expect("攻击者").hp;

    // 主人默认 Peace：受击也不反击玩家，但攻击本身仍然能命中。
    harness
        .world
        .attack(attacker, MirDirection::Up)
        .expect("攻击动作");
    harness.elapse(300);
    assert_eq!(
        harness.world.monster(pet).expect("宠物").pet_target,
        None,
        "和平模式主人的宠物不反击玩家"
    );
    harness.elapse(1_000);
    assert_eq!(
        harness.world.player(attacker).expect("攻击者").hp,
        attacker_hp
    );
}

#[test]
fn players_cannot_attack_pets_without_all_mode() {
    let mut harness = Harness::new(38);
    let master = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    let pet = harness.summon_skeleton(master);
    let pet_hp = harness.hp(pet);

    // 主人的 Group 模式：不能打自己的宠物（Crystal `:2440`）。
    {
        let player = harness.world.player_mut(master).expect("主人");
        player.attack_mode = AttackMode::Group;
        player.direction = MirDirection::Down;
        player.attack_time = 0;
        player.action_time = 0;
    }
    let report = harness
        .world
        .attack(master, MirDirection::Down)
        .expect("攻击动作");
    assert!(report.is_none(), "Group 模式不能选中自己的宠物");
    harness.elapse(300);
    assert_eq!(harness.hp(pet), pet_hp);

    // 主人的 All 模式：可以打自己的宠物，但宠物不会把主人当目标。
    {
        let player = harness.world.player_mut(master).expect("主人");
        player.attack_mode = AttackMode::All;
        player.attack_time = 0;
        player.action_time = 0;
        player.stats.min_dc = 20;
        player.stats.max_dc = 20;
    }
    let report = harness
        .world
        .attack(master, MirDirection::Down)
        .expect("攻击动作");
    assert!(report.is_some(), "All 模式可以选中自己的宠物");
    harness.elapse(300);
    assert!(harness.hp(pet) < pet_hp);
    assert_ne!(
        harness.world.monster(pet).expect("宠物").pet_target,
        Some(master)
    );
}

// ---------------------------------------------------------------------------
// 宠物命令（C.ChangePMode）与宠物互不攻击
// ---------------------------------------------------------------------------

#[test]
fn move_only_pet_follows_without_attacking() {
    let mut harness = Harness::new(40);
    let master = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    let pet = harness.summon_skeleton(master);
    let target = harness.spawn_passive(monster_kind("训练木桩", 5_000, 1, 0), Point::new(20, 22));
    harness.world.set_pet_mode(master, PetMode::MoveOnly);
    harness.wake_monster(pet);

    harness.elapse(600);
    assert_eq!(
        harness.world.monster(pet).expect("宠物").pet_target,
        None,
        "MoveOnly 不索敌"
    );
    assert_eq!(harness.hp(target), 5_000, "MoveOnly 不攻击");
}

#[test]
fn both_mode_pet_attacks_nearest_wild() {
    let mut harness = Harness::new(41);
    let master = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    let pet = harness.summon_skeleton(master);
    let target = harness.spawn_passive(monster_kind("训练木桩", 5_000, 1, 0), Point::new(20, 22));
    // 默认 Both：跟随 + 攻击。
    assert_eq!(harness.world.pet_mode(master), PetMode::Both);
    harness.wake_monster(pet);

    harness.elapse(1_000);
    harness.elapse(400);
    assert!(
        harness.hp(target) < 5_000,
        "Both 模式宠物应攻击视野内最近的野生怪"
    );
}

#[test]
fn attack_only_pet_locks_the_masters_target() {
    let mut harness = Harness::new(42);
    let master = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    let pet = harness.summon_skeleton(master);
    // 远处是主人目标，近处是干扰目标：必须锁主人目标。
    let far = harness.spawn_passive(monster_kind("主人目标", 5_000, 1, 0), Point::new(20, 25));
    let near = harness.spawn_passive(monster_kind("干扰目标", 5_000, 1, 0), Point::new(20, 22));
    harness
        .world
        .player_mut(master)
        .expect("玩家")
        .magic
        .last_target = Some(far);
    harness.world.set_pet_mode(master, PetMode::AttackOnly);
    harness.wake_monster(pet);

    harness.elapse(0);
    assert_eq!(
        harness.world.monster(pet).expect("宠物").pet_target,
        Some(far),
        "AttackOnly 应锁主人当前目标"
    );
    harness.elapse(600);
    assert_eq!(harness.hp(near), 5_000, "不应改打更近的干扰目标");
}

#[test]
fn rest_mode_pet_does_not_search_or_follow() {
    let mut harness = Harness::new(43);
    let master = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    let pet = harness.summon_skeleton(master);
    let target = harness.spawn_passive(monster_kind("训练木桩", 5_000, 1, 0), Point::new(20, 22));
    let rest_position = harness.world.monster(pet).expect("宠物").position;
    harness.world.set_pet_mode(master, PetMode::None);
    harness.wake_monster(pet);

    // 主人走开，休息态宠物不跟、不索敌。
    {
        let player = harness.world.player_mut(master).expect("玩家");
        player.action_time = 0;
        player.attack_time = 0;
        player.movement_time = 0;
    }
    harness
        .world
        .walk(master, MirDirection::Left)
        .expect("主人走一步");
    harness.elapse(3_000);
    assert_eq!(
        harness.world.monster(pet).expect("宠物").pet_target,
        None,
        "休息态不索敌"
    );
    assert_eq!(
        harness.world.monster(pet).expect("宠物").position,
        rest_position,
        "休息态不跟随"
    );
    assert_eq!(harness.hp(target), 5_000);
}

#[test]
fn pets_do_not_attack_each_other() {
    let mut harness = Harness::new(44);
    let first = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    let first_pet = harness.summon_skeleton(first);
    let second = harness.add_player(3, MirClass::Taoist, 30, Point::new(20, 23));
    harness
        .world
        .player_mut(second)
        .expect("第二个玩家")
        .direction = MirDirection::Up;
    let second_pet = harness.summon_skeleton(second);
    // 两只宠物相邻；双方主人 All 模式（即使敌对，宠物也不互攻）。
    for master in [first, second] {
        harness.world.player_mut(master).expect("主人").attack_mode = AttackMode::All;
    }
    let first_hp = harness.hp(first_pet);
    let second_hp = harness.hp(second_pet);
    harness
        .world
        .monster_mut(first_pet)
        .expect("宠物1")
        .pet_target = Some(second_pet);
    harness
        .world
        .monster_mut(second_pet)
        .expect("宠物2")
        .pet_target = Some(first_pet);
    harness.wake_monster(first_pet);
    harness.wake_monster(second_pet);

    harness.elapse(1_000);
    assert_eq!(harness.hp(first_pet), first_hp, "宠物之间不互相攻击");
    assert_eq!(harness.hp(second_pet), second_hp);
    assert_ne!(
        harness.world.monster(first_pet).expect("宠物1").pet_target,
        Some(second_pet)
    );
    assert_ne!(
        harness.world.monster(second_pet).expect("宠物2").pet_target,
        Some(first_pet)
    );
}

#[test]
fn pet_mode_resets_on_recall_and_pet_death() {
    let mut harness = Harness::new(45);
    let master = harness.add_player(1, MirClass::Taoist, 30, Point::new(20, 20));
    let pet = harness.summon_skeleton(master);
    harness.world.set_pet_mode(master, PetMode::AttackOnly);
    assert_eq!(harness.world.pet_mode(master), PetMode::AttackOnly);

    // 同名召唤 = 召回：命令重置为 Both。
    harness.ready_all(master);
    harness
        .cast(
            master,
            Spell::SummonSkeleton,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("召回施法");
    harness.elapse(500);
    assert_eq!(
        harness.world.pet_mode(master),
        PetMode::Both,
        "重召唤后命令重置"
    );

    // 宠物死亡：命令同样重置。
    harness.world.set_pet_mode(master, PetMode::None);
    let mut killer_kind = monster_kind("斩宠者", 5_000, 10, 0);
    killer_kind.stats.min_dc = 100;
    killer_kind.stats.max_dc = 100;
    killer_kind.stats.accuracy = 100;
    let killer = harness.spawn_active(killer_kind, Point::new(20, 18));
    harness.world.monster_mut(killer).expect("杀手").target = Some(pet);
    harness.world.monster_mut(pet).expect("宠物").hp = 10;
    harness.elapse(0);
    harness.elapse(300);
    assert!(harness.world.pets_of(master).is_empty(), "宠物应死亡");
    assert_eq!(
        harness.world.pet_mode(master),
        PetMode::Both,
        "宠物死亡后命令重置"
    );
}

// ---------------------------------------------------------------------------
// 怪物 AI 深化：远程攻击 / 目标丢失回位
// ---------------------------------------------------------------------------

#[test]
fn ranged_ai_ranges_cover_the_data_archer_templates() {
    let mut kind = monster_kind("ArcherGuard", 100, 1, 0);
    kind.view_range = 9;
    for (ai, expected) in [
        (8, 6),
        (16, 9),
        (164, 6),
        (175, 6),
        (199, 6),
        (31, 8),
        (32, 8),
        (57, 10),
        (80, 10),
        (113, 9),
    ] {
        kind.ai = ai;
        assert_eq!(
            kind.ranged_attack_range(),
            Some(expected),
            "AI {ai} 的远程射程"
        );
    }
    kind.ai = 31;
    assert!(kind.melee_when_adjacent(), "RightGuard 贴身改近战");
    kind.ai = 16;
    assert!(kind.melee_when_adjacent(), "RedThunderZuma 贴身改近战");
    kind.ai = 8;
    assert!(!kind.melee_when_adjacent(), "AxeSkeleton 贴身仍投射");
    kind.ai = 57;
    assert!(kind.is_town_archer(), "TownArcher 站桩");
    kind.ai = 113;
    assert!(kind.is_archer_guard(), "AI 113 + ArcherGuard 名字");
    kind.name = "Armadillo".to_owned();
    assert_eq!(
        kind.ranged_attack_range(),
        None,
        "Jev 库 AI 113 是 Armadillo，不能当弓手"
    );
    kind.name = "弓手".to_owned();
    kind.ai = 10; // FlamingWooma 是近战物理（MACAgility），不在远程覆盖内
    assert_eq!(kind.ranged_attack_range(), None);
}

#[test]
fn town_archer_does_not_chase_without_a_route() {
    let mut harness = Harness::new(52);
    let player = harness.add_player(1, MirClass::Warrior, 30, Point::new(20, 20));
    let mut kind = monster_kind("弓箭守卫", 5_000, 10, 0);
    kind.ai = 57;
    let archer = harness.spawn_active(kind, Point::new(20, 32));
    harness.world.monster_mut(archer).expect("弓手").target = Some(player);
    harness.wake_monster(archer);
    let position = harness.world.monster(archer).expect("弓手").position;

    harness.elapse(2_000);
    assert_eq!(
        harness.world.monster(archer).expect("弓手").position,
        position,
        "TownArcher 无路线表应站桩，不追击"
    );
}

#[test]
fn town_archer_only_shoots_red_name_at_the_pk_threshold() {
    let mut harness = Harness::new(78);
    let player = harness.add_player(1, MirClass::Warrior, 30, Point::new(20, 20));
    let mut kind = monster_kind("城镇弓手", 5_000, 10, 0);
    kind.ai = 57;
    kind.stats.min_dc = 50;
    kind.stats.max_dc = 50;
    kind.stats.accuracy = 100;
    let archer = harness.spawn_active(kind, Point::new(20, 24));

    // 白名（PK=0）：不锁目标、不广播、不受伤。
    harness.elapse(0);
    assert!(
        !harness.packets(player).iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectRangeAttack(attack) if attack.object_id == archer
        )),
        "白名玩家不应被 TownArcher 射击"
    );
    assert_eq!(
        harness.world.player(player).expect("玩家").hp,
        harness.world.player(player).expect("玩家").stats.hp
    );

    // 边界 199：仍不是红名（Crystal `PKPoints >= 200`）。
    if let Some(player) = harness.world.player_mut(player) {
        player.pk_points = 199;
    }
    harness.wake_monster(archer);
    harness.elapse(0);
    assert!(
        !harness.packets(player).iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectRangeAttack(attack) if attack.object_id == archer
        )),
        "PK=199 仍不应被射击"
    );

    // 200 起是红名：应锁定并射击。
    if let Some(player) = harness.world.player_mut(player) {
        player.pk_points = 200;
    }
    harness.wake_monster(archer);
    harness.elapse(0);
    let packets = harness.packets(player);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectRangeAttack(attack)
                if attack.object_id == archer && attack.target_id == player
        )),
        "PK=200 应被 TownArcher 射击：{packets:?}"
    );
    let hp = harness.world.player(player).expect("玩家").hp;
    harness.elapse(800);
    assert!(
        harness.world.player(player).expect("玩家").hp < hp,
        "红名应真的掉血"
    );
}

#[test]
fn ranged_monster_shoots_from_six_cells() {
    let mut harness = Harness::new(50);
    let player = harness.add_player(1, MirClass::Warrior, 30, Point::new(20, 20));
    let max_hp = harness.world.player(player).expect("玩家").stats.hp;
    let mut kind = monster_kind("掷斧骷髅", 5_000, 5, 0);
    kind.ai = 8; // Crystal AxeSkeleton：射程 6
    kind.stats.min_dc = 5;
    kind.stats.max_dc = 5;
    kind.stats.accuracy = 100;
    let shooter = harness.spawn_active(kind, Point::new(20, 24));
    harness.wake_monster(shooter);

    harness.elapse(0);
    assert!(
        harness.packets(player).iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectRangeAttack(attack)
                if attack.object_id == shooter && attack.target_id == player
        )),
        "远程怪应广播 S.ObjectRangeAttack（Crystal `AxeSkeleton.cs:39`）"
    );
    // `ProjectileAttack`：distance(4) * 50 + 500 = 700ms。
    harness.elapse(800);
    assert!(
        harness.world.player(player).expect("玩家").hp < max_hp,
        "远程伤害应在弹道延迟后落地"
    );
}

#[test]
fn target_lost_monster_returns_home() {
    let mut harness = Harness::new(51);
    let _player = harness.add_player(1, MirClass::Warrior, 30, Point::new(20, 20));
    let monster = harness.spawn_active(monster_kind("游荡怪", 5_000, 5, 0), Point::new(30, 20));
    harness.world.monster_mut(monster).expect("怪物").home = Point::new(20, 20);
    // 模拟目标死亡/跑出 DataRange 后 `process_monster` 的清理结果。
    harness.world.monster_mut(monster).expect("怪物").target = None;
    harness.wake_monster(monster);

    harness.elapse(2_000);
    let position = harness.world.monster(monster).expect("怪物").position;
    assert_eq!(harness.world.monster(monster).expect("怪物").target, None);
    assert!(
        position.max_distance(Point::new(20, 20)) < 10,
        "目标丢失后应朝出生点回位，实际 {position:?}"
    );
}

// ---------------------------------------------------------------------------
// 怪物魔法攻击（MonsterMagic）
// ---------------------------------------------------------------------------

#[test]
fn magic_monster_does_not_cast_out_of_range() {
    let mut harness = Harness::new(60);
    let player = harness.add_player(1, MirClass::Warrior, 30, Point::new(20, 20));
    let monster = harness.spawn_active(magic_kind("远程法师", 55), Point::new(20, 28));
    // 手动锁目标，避免视野半径掩盖射程判定。
    harness.world.monster_mut(monster).expect("法师").target = Some(player);
    harness.wake_monster(monster);

    harness.elapse(0);
    assert!(
        !harness.packets(player).iter().any(
            |packet| matches!(packet, ServerPacket::ObjectMagic(magic) if magic.object_id == monster)
        ),
        "射程 8 > 6，不应施法"
    );
}

#[test]
fn magic_monster_casts_and_damages_the_target() {
    let mut harness = Harness::new(61);
    let player = harness.add_player(1, MirClass::Warrior, 30, Point::new(20, 20));
    let max_hp = harness.world.player(player).expect("玩家").stats.hp;
    let monster = harness.spawn_active(magic_kind("远程法师", 55), Point::new(20, 24));
    // 跳过 HumanWizard 的首轮 5 秒 FearTime 走位（该行为由 kite 用例单独覆盖）。
    harness.world.monster_mut(monster).expect("法师").fear_time = -1;
    harness.wake_monster(monster);

    harness.elapse(0);
    assert!(
        harness.packets(player).iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectMagic(magic)
                if magic.object_id == monster && magic.spell == Spell::ThunderBolt
        )),
        "应广播 S.ObjectMagic（ThunderBolt）"
    );
    harness.elapse(600);
    assert!(
        harness.world.player(player).expect("玩家").hp < max_hp,
        "魔法伤害应在 500ms 延迟后落地"
    );
}

#[test]
fn magic_monster_respects_attack_speed_cooldown() {
    let mut harness = Harness::new(62);
    let player = harness.add_player(1, MirClass::Warrior, 30, Point::new(20, 20));
    let monster = harness.spawn_active(magic_kind("远程法师", 55), Point::new(20, 24));
    // 跳过 HumanWizard 的首轮 5 秒 FearTime 走位（该行为由 kite 用例单独覆盖）。
    harness.world.monster_mut(monster).expect("法师").fear_time = -1;
    harness.wake_monster(monster);

    harness.elapse(0);
    let first = harness
        .packets(player)
        .iter()
        .filter(|packet| matches!(packet, ServerPacket::ObjectMagic(_)))
        .count();
    assert_eq!(first, 1, "首轮施法一次");
    harness.elapse(0);
    let second = harness
        .packets(player)
        .iter()
        .filter(|packet| matches!(packet, ServerPacket::ObjectMagic(_)))
        .count();
    assert_eq!(second, 0, "冷却内不应再次施法");
    harness.elapse(3_000);
    let third = harness
        .packets(player)
        .iter()
        .filter(|packet| matches!(packet, ServerPacket::ObjectMagic(_)))
        .count();
    assert_eq!(third, 1, "冷却结束后再次施法");
}

#[test]
fn magic_line_attack_penetrates_two_players() {
    let mut harness = Harness::new(63);
    let first = harness.add_player(1, MirClass::Warrior, 30, Point::new(20, 21));
    let second = harness.add_player(2, MirClass::Warrior, 30, Point::new(20, 22));
    let first_hp = harness.world.player(first).expect("玩家1").hp;
    let second_hp = harness.world.player(second).expect("玩家2").hp;
    let monster = harness.spawn_active(magic_kind("骷髅长枪兵", 29), Point::new(20, 20));
    harness.wake_monster(monster);

    harness.elapse(0);
    harness.elapse(600);
    assert!(harness.world.player(first).expect("玩家1").hp < first_hp);
    assert!(
        harness.world.player(second).expect("玩家2").hp < second_hp,
        "直线技能应穿透到第二格"
    );
}

#[test]
fn magic_aoe_hits_multiple_targets_but_not_outside_radius() {
    let mut harness = Harness::new(64);
    let near1 = harness.add_player(1, MirClass::Warrior, 30, Point::new(19, 20));
    let near2 = harness.add_player(2, MirClass::Warrior, 30, Point::new(21, 20));
    let far = harness.add_player(3, MirClass::Warrior, 30, Point::new(28, 20));
    let near1_hp = harness.world.player(near1).expect("近1").hp;
    let near2_hp = harness.world.player(near2).expect("近2").hp;
    let far_hp = harness.world.player(far).expect("远").hp;
    let monster = harness.spawn_active(magic_kind("赤月恶魔", 13), Point::new(20, 20));
    harness.wake_monster(monster);

    harness.elapse(0);
    harness.elapse(600);
    assert!(harness.world.player(near1).expect("近1").hp < near1_hp);
    assert!(harness.world.player(near2).expect("近2").hp < near2_hp);
    assert_eq!(
        harness.world.player(far).expect("远").hp,
        far_hp,
        "AOE 视野半径 7 以外不应受伤"
    );
}

#[test]
fn magic_cast_is_interrupted_when_target_dies() {
    let mut harness = Harness::new(65);
    let player = harness.add_player(1, MirClass::Warrior, 30, Point::new(20, 20));
    let monster = harness.spawn_active(magic_kind("远程法师", 55), Point::new(20, 24));
    // 跳过 HumanWizard 的首轮 5 秒 FearTime 走位（该行为由 kite 用例单独覆盖）。
    harness.world.monster_mut(monster).expect("法师").fear_time = -1;
    harness.wake_monster(monster);
    harness.elapse(0); // 施法入队，500ms 后结算
    {
        let target = harness.world.player_mut(player).expect("玩家");
        target.dead = true;
        target.hp = 0;
    }
    harness.elapse(600);
    assert!(
        harness
            .world
            .player(player)
            .is_some_and(|target| target.dead),
        "目标死亡不应崩，延迟伤害直接丢弃"
    );
    assert!(harness.world.monster(monster).is_some());
}

#[test]
fn bone_spearman_line_damage_is_delayed_by_distance() {
    let mut harness = Harness::new(67);
    let first = harness.add_player(1, MirClass::Warrior, 30, Point::new(20, 21));
    let second = harness.add_player(2, MirClass::Warrior, 30, Point::new(20, 22));
    let first_hp = harness.world.player(first).expect("第一格").hp;
    let second_hp = harness.world.player(second).expect("第二格").hp;
    let monster = harness.spawn_active(magic_kind("骷髅长枪兵", 29), Point::new(20, 20));
    harness.wake_monster(monster);

    harness.elapse(0);
    // 第一格：250 + 1*50 = 300ms；第二格：250 + 2*50 = 350ms。
    harness.elapse(310);
    assert!(harness.world.player(first).expect("第一格").hp < first_hp);
    assert_eq!(
        harness.world.player(second).expect("第二格").hp,
        second_hp,
        "第二格伤害应晚 50ms"
    );
    harness.elapse(60);
    assert!(harness.world.player(second).expect("第二格").hp < second_hp);
}

#[test]
fn aoe_does_not_hit_players_on_no_fight_maps() {
    let mut map = MapInfo {
        file_name: MAP.to_owned(),
        title: "安全地图".to_owned(),
        no_fight: true,
        ..MapInfo::default()
    };
    map.file_name = MAP.to_owned();
    let mut harness = Harness::with_data(68, game_data_with_maps(vec![map]));
    let player = harness.add_player(1, MirClass::Warrior, 30, Point::new(20, 20));
    let hp = harness.world.player(player).expect("玩家").hp;
    let monster = harness.spawn_active(magic_kind("赤月恶魔", 13), Point::new(20, 22));
    harness.wake_monster(monster);

    harness.elapse(0);
    harness.elapse(600);
    assert_eq!(
        harness.world.player(player).expect("玩家").hp,
        hp,
        "NoFight 地图上怪物 AOE 不应造成伤害"
    );
}

#[test]
fn magic_monster_kites_when_adjacent() {
    let mut harness = Harness::new(66);
    let player = harness.add_player(1, MirClass::Warrior, 30, Point::new(20, 20));
    let monster = harness.spawn_active(magic_kind("远程法师", 55), Point::new(20, 21));
    let start = harness.world.monster(monster).expect("法师").position;
    harness.wake_monster(monster);

    harness.elapse(0);
    let moved = harness.world.monster(monster).expect("法师").position;
    assert_ne!(moved, start, "HumanWizard 贴身应拉开距离（kite）");
    assert!(moved.max_distance(Point::new(20, 20)) > 1);
    assert!(harness.world.player(player).expect("玩家").hp > 0);
}

// ---------------------------------------------------------------------------
// F3：BoneSpearman（AI 29）直线遇高墙 continue 的回归
// ---------------------------------------------------------------------------

#[test]
fn bone_spearman_line_continues_past_high_wall() {
    let mut harness = Harness::new(72);
    // 直线第一格 (21,20) 是高墙，目标在墙后的 (22,20)。
    harness
        .world
        .insert_map(MAP, synthetic_map(&[Point::new(21, 20)]));
    assert!(
        !harness
            .world
            .map_of(MAP)
            .expect("合成地图")
            .walkable(21, 20),
        "合成地图的 (21,20) 应是不可走高墙"
    );
    let player = harness.add_player(1, MirClass::Warrior, 30, Point::new(22, 20));
    let hp = harness.world.player(player).expect("玩家").hp;
    let monster = harness.spawn_active(magic_kind("骷髅长枪兵", 29), Point::new(20, 20));
    harness.wake_monster(monster);

    harness.elapse(0);
    harness.elapse(600);
    assert!(
        harness.world.player(player).expect("玩家").hp < hp,
        "高墙后的目标仍应被直线命中（遇墙 continue，不是 break）"
    );
}

#[test]
fn bone_spearman_line_hits_target_before_high_wall() {
    let mut harness = Harness::new(73);
    // 直线第二格 (22,20) 是墙，目标在墙前 (21,20)；证明墙前目标正常命中。
    harness
        .world
        .insert_map(MAP, synthetic_map(&[Point::new(22, 20)]));
    assert!(
        !harness
            .world
            .map_of(MAP)
            .expect("合成地图")
            .walkable(22, 20)
    );
    let player = harness.add_player(1, MirClass::Warrior, 30, Point::new(21, 20));
    let hp = harness.world.player(player).expect("玩家").hp;
    let monster = harness.spawn_active(magic_kind("骷髅长枪兵", 29), Point::new(20, 20));
    harness.wake_monster(monster);

    harness.elapse(0);
    harness.elapse(600);
    assert!(
        harness.world.player(player).expect("玩家").hp < hp,
        "墙前目标应正常被命中"
    );
}

#[test]
fn bone_spearman_line_wall_without_target_after_does_not_panic() {
    let mut harness = Harness::new(74);
    // 目标在墙前，墙后没有目标；continue 扫到墙后空格不应 panic。
    harness
        .world
        .insert_map(MAP, synthetic_map(&[Point::new(22, 20)]));
    let player = harness.add_player(1, MirClass::Warrior, 30, Point::new(21, 20));
    let monster = harness.spawn_active(magic_kind("骷髅长枪兵", 29), Point::new(20, 20));
    harness.wake_monster(monster);

    harness.elapse(0);
    harness.elapse(600);
    assert!(harness.world.monster(monster).is_some());
    assert!(harness.world.player(player).is_some());
}

// ---------------------------------------------------------------------------
// t70：AI 14/50/100 物理特殊攻击（勿写成魔法）
// ---------------------------------------------------------------------------

/// 特殊 AI 的测试怪：低 DC（避免测试中途打死玩家）、高准确、SC=0（绿毒 0 伤害）。
fn special_kind(name: &str, ai: u8) -> MonsterKind {
    let mut kind = monster_kind(name, 5_000, 10, 0);
    kind.ai = ai;
    kind.stats.min_dc = 1;
    kind.stats.max_dc = 1;
    kind.stats.accuracy = 100;
    kind.stats.min_sc = 0;
    kind.stats.max_sc = 0;
    kind
}

/// 反复挨打直到目标出现指定毒（Crystal `PoisonTarget` 是概率触发）。
fn wait_for_poison(harness: &mut Harness, player: u32, kind: PoisonType, cycles: usize) -> bool {
    for _ in 0..cycles {
        if harness
            .world
            .player(player)
            .is_some_and(|player| player.magic.poisons.iter().any(|p| p.kind == kind))
        {
            return true;
        }
        harness.elapse(2_600);
    }
    harness
        .world
        .player(player)
        .is_some_and(|player| player.magic.poisons.iter().any(|p| p.kind == kind))
}

#[test]
fn evil_centipede_is_physical_aoe_with_green_and_paralysis() {
    let mut harness = Harness::new(80);
    let tank = harness.add_player(1, MirClass::Warrior, 40, Point::new(20, 21));
    let far = harness.add_player(2, MirClass::Warrior, 40, Point::new(20, 27));
    let outside = harness.add_player(3, MirClass::Warrior, 40, Point::new(20, 28));
    let tank_hp = harness.world.player(tank).expect("目标").hp;
    let far_hp = harness.world.player(far).expect("半径 7 内").hp;
    let outside_hp = harness.world.player(outside).expect("半径 7 外").hp;
    let monster = harness.spawn_active(special_kind("邪恶蜈蚣", 14), Point::new(20, 20));
    harness.wake_monster(monster);

    harness.elapse(0);
    harness.elapse(600);
    assert!(harness.world.player(tank).expect("目标").hp < tank_hp);
    assert!(
        harness.world.player(far).expect("半径 7 内").hp < far_hp,
        "半径 7 内的第二个玩家也应被 AOE 命中"
    );
    assert_eq!(
        harness.world.player(outside).expect("半径 7 外").hp,
        outside_hp,
        "半径 7 外的玩家不应被误伤"
    );
    let packets = harness.packets(tank);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectAttack(_))),
        "AI 14 是 S.ObjectAttack 物理 AOE"
    );
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectMagic(_))),
        "AI 14 不是 S.ObjectMagic"
    );
    assert!(
        wait_for_poison(&mut harness, tank, PoisonType::Green, 80),
        "命中后应挂绿毒（5/15/2000）"
    );
    assert!(
        wait_for_poison(&mut harness, tank, PoisonType::Paralysis, 80),
        "命中后应挂麻痹（15/5/2000）"
    );
}

#[test]
fn great_fox_spirit_close_range_uses_radius_two() {
    let mut harness = Harness::new(81);
    let tank = harness.add_player(1, MirClass::Warrior, 40, Point::new(20, 22));
    let outside = harness.add_player(2, MirClass::Warrior, 40, Point::new(20, 24));
    let tank_hp = harness.world.player(tank).expect("目标").hp;
    let outside_hp = harness.world.player(outside).expect("半径 2 外").hp;
    let monster = harness.spawn_active(special_kind("大狐灵", 50), Point::new(20, 20));
    harness.wake_monster(monster);

    harness.elapse(0);
    harness.elapse(500);
    assert!(harness.world.player(tank).expect("目标").hp < tank_hp);
    assert_eq!(
        harness.world.player(outside).expect("半径 2 外").hp,
        outside_hp,
        "贴身形态半径 2，4 格外的玩家不应被误伤"
    );
    assert!(
        wait_for_poison(&mut harness, tank, PoisonType::Slow, 80),
        "命中后应挂 Slow（5/15/1000）"
    );
}

#[test]
fn great_fox_spirit_ranged_form_uses_radius_seven_and_range_attack() {
    let mut harness = Harness::new(82);
    let tank = harness.add_player(1, MirClass::Warrior, 40, Point::new(20, 25));
    let near = harness.add_player(2, MirClass::Warrior, 40, Point::new(20, 26));
    let outside = harness.add_player(3, MirClass::Warrior, 40, Point::new(20, 29));
    let tank_hp = harness.world.player(tank).expect("目标").hp;
    let near_hp = harness.world.player(near).expect("半径 7 内").hp;
    let outside_hp = harness.world.player(outside).expect("半径 7 外").hp;
    let monster = harness.spawn_active(special_kind("大狐灵", 50), Point::new(20, 20));
    harness.wake_monster(monster);

    harness.elapse(0);
    harness.elapse(500);
    assert!(harness.world.player(tank).expect("目标").hp < tank_hp);
    assert!(
        harness.world.player(near).expect("半径 7 内").hp < near_hp,
        "远程形态半径 7"
    );
    assert_eq!(
        harness.world.player(outside).expect("半径 7 外").hp,
        outside_hp,
        "半径 7 外的玩家不应被误伤"
    );
    let packets = harness.packets(tank);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectRangeAttack(_))),
        "距离 >2 时 Crystal 广播 S.ObjectRangeAttack"
    );
    assert!(
        wait_for_poison(&mut harness, tank, PoisonType::Paralysis, 80),
        "命中后应挂 Paralysis（5/5/1000）"
    );
}

#[test]
fn venom_spider_line_hits_two_cells_and_poisons() {
    let mut harness = Harness::new(83);
    let front = harness.add_player(1, MirClass::Warrior, 40, Point::new(20, 21));
    let behind = harness.add_player(2, MirClass::Warrior, 40, Point::new(20, 22));
    let side = harness.add_player(3, MirClass::Warrior, 40, Point::new(22, 22));
    let front_hp = harness.world.player(front).expect("第一格").hp;
    let behind_hp = harness.world.player(behind).expect("第二格").hp;
    let side_hp = harness.world.player(side).expect("直线外").hp;
    let monster = harness.spawn_active(special_kind("毒蜘蛛", 100), Point::new(20, 20));
    harness.wake_monster(monster);

    harness.elapse(0);
    harness.elapse(500);
    assert!(harness.world.player(front).expect("第一格").hp < front_hp);
    assert!(
        harness.world.player(behind).expect("第二格").hp < behind_hp,
        "LineAttack(2) 第二格也应命中"
    );
    assert_eq!(
        harness.world.player(side).expect("直线外").hp,
        side_hp,
        "直线外玩家不应被误伤"
    );
    let packets = harness.packets(front);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectAttack(_))),
        "AI 100 是 S.ObjectAttack 物理直线"
    );
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectMagic(_))),
        "AI 100 不是 S.ObjectMagic"
    );
    assert!(
        wait_for_poison(&mut harness, front, PoisonType::Green, 80),
        "命中后应挂绿毒（8/5/1000）"
    );
}

#[test]
fn evil_centipede_aoe_does_not_fire_on_no_fight_maps() {
    let mut map = MapInfo {
        file_name: MAP.to_owned(),
        title: "安全地图".to_owned(),
        no_fight: true,
        ..MapInfo::default()
    };
    map.file_name = MAP.to_owned();
    let mut harness = Harness::with_data(84, game_data_with_maps(vec![map]));
    let player = harness.add_player(1, MirClass::Warrior, 40, Point::new(20, 21));
    let hp = harness.world.player(player).expect("玩家").hp;
    let monster = harness.spawn_active(special_kind("邪恶蜈蚣", 14), Point::new(20, 20));
    harness.wake_monster(monster);

    harness.elapse(0);
    harness.elapse(600);
    assert_eq!(
        harness.world.player(player).expect("玩家").hp,
        hp,
        "NoFight 地图上 AI 14 不应造成伤害"
    );
    assert!(
        harness
            .world
            .player(player)
            .expect("玩家")
            .magic
            .poisons
            .is_empty(),
        "NoFight 地图上 AI 14 不应挂毒（绿毒/麻痹）"
    );
}

#[test]
fn great_fox_spirit_aoe_does_not_fire_on_no_fight_maps() {
    let mut map = MapInfo {
        file_name: MAP.to_owned(),
        title: "安全地图".to_owned(),
        no_fight: true,
        ..MapInfo::default()
    };
    map.file_name = MAP.to_owned();
    let mut harness = Harness::with_data(85, game_data_with_maps(vec![map]));
    let player = harness.add_player(1, MirClass::Warrior, 40, Point::new(20, 22));
    let hp = harness.world.player(player).expect("玩家").hp;
    let monster = harness.spawn_active(special_kind("大狐灵", 50), Point::new(20, 20));
    harness.wake_monster(monster);

    harness.elapse(0);
    harness.elapse(500);
    assert_eq!(
        harness.world.player(player).expect("玩家").hp,
        hp,
        "NoFight 地图上 AI 50 不应造成伤害"
    );
    assert!(
        harness
            .world
            .player(player)
            .expect("玩家")
            .magic
            .poisons
            .is_empty(),
        "NoFight 地图上 AI 50 不应挂毒"
    );
}

#[test]
fn venom_spider_line_does_not_fire_on_no_fight_maps() {
    let mut map = MapInfo {
        file_name: MAP.to_owned(),
        title: "安全地图".to_owned(),
        no_fight: true,
        ..MapInfo::default()
    };
    map.file_name = MAP.to_owned();
    let mut harness = Harness::with_data(86, game_data_with_maps(vec![map]));
    // 直线两格都站人：若 NoFight 失效，两格都会被 LineAttack 命中。
    let front = harness.add_player(1, MirClass::Warrior, 40, Point::new(20, 21));
    let behind = harness.add_player(2, MirClass::Warrior, 40, Point::new(20, 22));
    let front_hp = harness.world.player(front).expect("第一格").hp;
    let behind_hp = harness.world.player(behind).expect("第二格").hp;
    let monster = harness.spawn_active(special_kind("毒蜘蛛", 100), Point::new(20, 20));
    harness.wake_monster(monster);

    harness.elapse(0);
    harness.elapse(600);
    assert_eq!(
        harness.world.player(front).expect("第一格").hp,
        front_hp,
        "NoFight 地图上 AI 100 直线不应命中第一格"
    );
    assert_eq!(
        harness.world.player(behind).expect("第二格").hp,
        behind_hp,
        "NoFight 地图上 AI 100 直线不应命中第二格"
    );
    for player in [front, behind] {
        assert!(
            harness
                .world
                .player(player)
                .expect("玩家")
                .magic
                .poisons
                .is_empty(),
            "NoFight 地图上 AI 100 不应挂绿毒"
        );
    }
}

// ---------------------------------------------------------------------------
// t75：Crystal 参考魔法系（AI 215/216/220）
// ---------------------------------------------------------------------------

fn s_object_magic_count(harness: &mut Harness, player: u32) -> usize {
    harness
        .packets(player)
        .iter()
        .filter(|packet| matches!(packet, ServerPacket::ObjectMagic(_)))
        .count()
}

#[test]
fn sep_wizard_casts_great_fire_ball_and_deals_damage() {
    let mut harness = Harness::new(90);
    let player = harness.add_player(1, MirClass::Warrior, 40, Point::new(20, 23));
    let hp = harness.world.player(player).expect("玩家").hp;
    let monster = harness.spawn_active(magic_kind("分身法师", 215), Point::new(20, 20));
    harness.wake_monster(monster);

    harness.elapse(0);
    assert_eq!(
        s_object_magic_count(&mut harness, player),
        1,
        "应广播 S.ObjectMagic"
    );
    assert_eq!(
        harness.world.player(player).expect("玩家").hp,
        hp,
        "伤害延迟 500ms 后才落地"
    );
    harness.elapse(600);
    assert!(
        harness.world.player(player).expect("玩家").hp < hp,
        "GreatFireBall 应造成伤害"
    );
}

#[test]
fn monster_magic_does_not_cast_out_of_range() {
    let mut harness = Harness::new(91);
    let player = harness.add_player(1, MirClass::Warrior, 40, Point::new(20, 27));
    let monster = harness.spawn_active(magic_kind("分身法师", 215), Point::new(20, 20));
    let start = harness.world.monster(monster).expect("法师").position;
    harness.wake_monster(monster);

    harness.elapse(0);
    assert_eq!(
        s_object_magic_count(&mut harness, player),
        0,
        "射程 6 外不应施法"
    );
    let moved = harness.world.monster(monster).expect("法师").position;
    assert_ne!(moved, start, "射程外应靠近而不是原地空放");
}

#[test]
fn monster_magic_respects_cooldown() {
    let mut harness = Harness::new(92);
    let player = harness.add_player(1, MirClass::Warrior, 40, Point::new(20, 23));
    let monster = harness.spawn_active(magic_kind("分身法师", 215), Point::new(20, 20));
    harness.wake_monster(monster);

    harness.elapse(0);
    assert_eq!(s_object_magic_count(&mut harness, player), 1);
    harness.elapse(100);
    assert_eq!(
        s_object_magic_count(&mut harness, player),
        0,
        "冷却 2500ms 内不应再次施法"
    );
    harness.elapse(2_500);
    assert!(
        s_object_magic_count(&mut harness, player) >= 1,
        "冷却结束后应再次施法"
    );
}

#[test]
fn monster_magic_stops_when_target_is_dead() {
    let mut harness = Harness::new(93);
    let player = harness.add_player(1, MirClass::Warrior, 40, Point::new(20, 23));
    let monster = harness.spawn_active(magic_kind("分身法师", 215), Point::new(20, 20));
    harness.wake_monster(monster);
    if let Some(state) = harness.world.player_mut(player) {
        state.dead = true;
    }
    let hp = harness.world.player(player).expect("玩家").hp;

    harness.elapse(0);
    harness.elapse(600);
    assert_eq!(
        s_object_magic_count(&mut harness, player),
        0,
        "死亡目标不再施法"
    );
    assert_eq!(harness.world.player(player).expect("玩家").hp, hp);
}

#[test]
fn sep_high_wizard_fire_bang_aoe_hits_multiple_targets() {
    let mut harness = Harness::new(94);
    let target = harness.add_player(1, MirClass::Warrior, 40, Point::new(20, 23));
    let nearby = harness.add_player(2, MirClass::Warrior, 40, Point::new(20, 24));
    let outside = harness.add_player(3, MirClass::Warrior, 40, Point::new(20, 28));
    let target_hp = harness.world.player(target).expect("目标").hp;
    let nearby_hp = harness.world.player(nearby).expect("AOE 内").hp;
    let outside_hp = harness.world.player(outside).expect("AOE 外").hp;
    let monster = harness.spawn_active(magic_kind("高阶分身法师", 220), Point::new(20, 20));
    harness.wake_monster(monster);

    harness.elapse(0);
    harness.elapse(600);
    assert!(harness.world.player(target).expect("目标").hp < target_hp);
    assert!(
        harness.world.player(nearby).expect("AOE 内").hp < nearby_hp,
        "FireBang 半径 1 应命中目标相邻格"
    );
    assert_eq!(
        harness.world.player(outside).expect("AOE 外").hp,
        outside_hp,
        "AOE 外不应被误伤"
    );
}
