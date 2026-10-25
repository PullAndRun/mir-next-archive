//! R52-B2：毒雾（`Spell.PoisonCloud`）端到端测试。
//!
//! 全部断言对照 Crystal（参考根目录 `github\Crystal`）：
//!
//! | 行为 | Crystal |
//! |---|---|
//! | 耗材：护身符 5 + 绿毒（shape 1）5；缺任一项 `cast = false` | `HumanObject.cs:4543-4567` |
//! | 材料失败仍保留已扣 MP / `ActionTime` / `AttackTime` / `SpellTime` / `Direction`，不写 `CastTime`、不排效果 | `HumanObject.cs:3773-3779` |
//! | 落地延迟 `MaxDistance × 50 + 500`、3×3、同格同类不叠、首个实际生成 `Show = true`、落地即训练 | `Map.cs:1434-1486`、`:2356` |
//! | **有效格 = 地图格属性 `Walk`**（`cell.Valid => Attribute == CellAttribute.Walk`），墙格/越界格跳过且不影响 `Show` 归属 | `Map.cs:2480-2482`、`:1442-1444`、`:1455-1466` |
//! | **附伤 `BonusDmg = Random.Next(PoisonAttack)`**：装备模板 `Stats` + `AddedStats` + 生效增益聚合；为 0 时不耗 RNG | `HumanObject.cs:4555`、`:1858-1859`、`:2332` |
//! | 6000ms 内每 1000ms 一跳（含首跳），到期用严格 `>` | `SpellObject.cs:51-122` |
//! | 每跳先 `DefenceType.MAC` 伤害，目标存活再附 12 跳绿毒；毒量 = 当时 `(MinSC+MaxSC)/2 + BonusDmg` | `SpellObject.cs:150-174` |
//! | 安全区/PK 规则走 `MapObject.IsAttackTarget`（双方任一在安全区即不可攻击，攻守均以 `AttackMode::All` 隔离验证） | `MapObject.cs:428-460` |
//! | 施法者下线/换图：本作清场（D3 PARTIAL，Crystal 会留场停伤至到期） | `SpellObject.cs:47-51`、`:66`、`:78-86` |
//!
//! 所有用例确定性：固定 RNG 种子、`with_data` 里先 `advance_clock(0)` 冻结时钟、怪物
//! `operate_time = -1`（训练木桩 AI 56）。

mod common;

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{
    AttackMode, ItemType, MirClass, MirDirection, MirGender, Monster as MonsterImage, PoisonType,
    Spell, Stat,
};
use mir_db::{MagicInfo, MapInfo, MirDb, SafeZoneInfo};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ItemInfo, Stats};
use mir_server::MonsterKind;
use mir_server::combat::{self, CombatStats};
use mir_server::gamedata::GameData;
use mir_server::magic::{CastReport, MagicError};
use mir_server::world::{Player, World};
use mir_server::{ActiveBuff, BuffKind};
use mir_world::Map;
use tokio::sync::mpsc;

const MAP: &str = "0";
const MAP_INDEX: i32 = 990;

/// 护身符模板（`GetAmulet(5)` 认 `ItemType.Amulet` 且形状 0）。
const AMULET: i32 = 101;
/// 绿毒模板（`GetPoison(5, 1)`：`ItemType.Amulet` + `Shape = 1`）。
const GREEN_POISON: i32 = 102;
/// 红毒模板（`Shape = 2`）：用来证明「形状必须精确匹配 1」。
const RED_POISON: i32 = 103;
/// 普通药水：用来证明「类型必须是护身符」。
const POTION: i32 = 104;
/// 武器模板 A（真实装备：`MinSC/MaxSC` 由 `ItemInfo.stats` 提供，经
/// [`World::refresh_stats`] 刷新进 `player.stats`，用来证明毒量读的是有效 SC）。
const SC_WEAPON_A: i32 = 105;
/// 武器模板 B：换装 + 刷新后 SC 变大，下一跳毒量必须跟着变。
const SC_WEAPON_B: i32 = 106;
/// 毒刃：模板自带 `PoisonAttack = 5`，附加属性再加 3、增益再加 2。
const POISON_WEAPON: i32 = 107;

/// 装备槽：B1 单测用的是 6/9，这里沿用，顺带覆盖「不是最近槽位也能找到」。
const AMULET_SLOT: usize = 6;
const POISON_SLOT: usize = 9;
/// 武器槽（`ItemType.Weapon`）。
const WEAPON_SLOT: usize = 0;

// ---------------------------------------------------------------------------
// 测试脚手架
// ---------------------------------------------------------------------------

struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Harness {
    fn new(seed: u64) -> Self {
        Self::with_data(seed, game_data(Vec::new()))
    }

    fn with_data(seed: u64, data: Arc<GameData>) -> Self {
        let mut world = World::new(999);
        // 先冻结时钟：`World::now` 在首次 `advance_clock` 前取真实墙钟，若等到第一次
        // `elapse` 才冻结，`cast_magic` 与 `elapse(599)` 之间真实耗时就可能把落点提前
        // 推进 1ms 以上（评审 B2-review 用 exit 101 复现过；同 `world.rs:6444-6455`）。
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

    /// 直接往根装备槽里放一件物品（`find_spell_material` 只看装备，不搜背包）。
    fn equip(&mut self, object_id: u32, slot: usize, index: i32, count: u16) {
        let info = self.world.item_info(index).expect("物品模板").clone();
        let mut item = self.world.create_fresh_item(&info);
        item.count = count;
        let player = self.world.player_mut(object_id).expect("玩家在世界上");
        player.equipment[slot] = Some(item);
    }

    fn equip_durability(&mut self, object_id: u32, slot: usize, index: i32, count: u16, dura: u16) {
        let info = self.world.item_info(index).expect("物品模板").clone();
        let mut item = self.world.create_fresh_item(&info);
        item.count = count;
        item.current_dura = dura;
        item.max_dura = dura;
        let player = self.world.player_mut(object_id).expect("玩家在世界上");
        player.equipment[slot] = Some(item);
    }

    fn equipment(&self, object_id: u32, slot: usize) -> Option<(i32, u16)> {
        self.world.player(object_id).and_then(|player| {
            player
                .equipment
                .get(slot)
                .and_then(Option::as_ref)
                .map(|item| (item.item_index, item.count))
        })
    }

    fn equipment_uid(&self, object_id: u32, slot: usize) -> Option<u64> {
        self.world.player(object_id).and_then(|player| {
            player
                .equipment
                .get(slot)
                .and_then(Option::as_ref)
                .map(|item| item.unique_id)
        })
    }

    /// 放一件同时带 `added_stats` 的根装备（Crystal `temp.AddedStats`，`HumanObject.cs:1858-1859`）。
    fn equip_added(
        &mut self,
        object_id: u32,
        slot: usize,
        index: i32,
        count: u16,
        stat: Stat,
        value: i32,
    ) -> u64 {
        let info = self.world.item_info(index).expect("物品模板").clone();
        let mut item = self.world.create_fresh_item(&info);
        item.count = count;
        item.added_stats.set(stat, value);
        let uid = item.unique_id;
        let player = self.world.player_mut(object_id).expect("玩家在世界上");
        player.equipment[slot] = Some(item);
        uid
    }

    /// 由真实装备/增益重算有效属性（`items.rs:1363-1399`，Crystal `RefreshStats`）。
    fn refresh_stats(&mut self, object_id: u32) {
        self.world.refresh_stats(object_id);
    }

    fn set_attack_mode(&mut self, object_id: u32, mode: AttackMode) {
        self.world.player_mut(object_id).expect("玩家").attack_mode = mode;
    }

    fn set_map(&mut self, object_id: u32, map: &str) {
        map.clone_into(&mut self.world.player_mut(object_id).expect("玩家").map);
    }

    /// 用带墙地图替换默认空地（墙格 = `CellAttribute::Wall`，不可走）。
    fn insert_walls(&mut self, walls: &[Point]) {
        self.world.insert_map(MAP, synthetic_map_with_walls(walls));
    }

    fn give_buff(&mut self, object_id: u32, stats: Stats) {
        self.give_buff_until(object_id, stats, i64::MAX);
    }

    /// 带到期时刻的增益：用于证明「生效 buff 计入 PoisonAttack，到期移除后回 0」。
    fn give_buff_until(&mut self, object_id: u32, stats: Stats, expire_at: i64) {
        let buff = ActiveBuff::new(BuffKind::Haste, expire_at, stats, object_id, Vec::new());
        self.world
            .player_mut(object_id)
            .expect("玩家")
            .buffs
            .push(buff);
    }

    fn inventory_snapshot(&self, object_id: u32) -> Vec<(u64, i32, u16)> {
        let player = self.world.player(object_id).expect("玩家还在");
        player
            .inventory
            .iter()
            .flatten()
            .map(|item| (item.unique_id, item.item_index, item.count))
            .collect()
    }

    /// 当前 RNG 的克隆：用于比对「本次施法到底抽了什么」。
    fn rng_state(&mut self) -> mir_core::MirRng {
        self.world.rng().clone()
    }

    fn spawn(&mut self, kind: MonsterKind, position: Point) -> u32 {
        let object_id = self.world.spawn_monster(MAP, kind, position);
        if let Some(monster) = self.world.monster_mut(object_id) {
            // 训练木桩（AI 56）不走动、不攻击，但 `process_monster` 里的毒 tick 仍要跑；
            // `operate_time = -1` 让它在每个 world tick 都被调度（Crystal 对象轮转）。
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

/// 技能模板：`PoisonCloud` 的 `MagicInfo`（数值同 `tests/magic.rs` 的 `magic_info`）。
fn poison_cloud_info() -> MagicInfo {
    MagicInfo {
        name: "毒雾".to_owned(),
        spell: Spell::PoisonCloud.value(),
        // `MagicCost = BaseCost + LevelCost * Level`（`HumanObject.cs:3375-3398`）。
        base_cost: 5,
        level_cost: 2,
        icon: 10,
        level1: 1,
        level2: 2,
        level3: 3,
        need1: 10,
        need2: 20,
        need3: 30,
        // R52-B4 记录：等级 3 时 `GetDelay = DelayBase - 3 * DelayReduction = 12000`
        // （`MagicInfo.cs:219-222`）；0 级时 12000，本文件每个用例只施法一次。
        delay_base: 12_000,
        delay_reduction: 100,
        power_base: 4,
        power_bonus: 0,
        m_power_base: 10,
        m_power_bonus: 0,
        // `HumanObject.cs:3416` 的射程预检：落点必须 <= 8 格。
        range: 8,
        multiplier_base: 1.0,
        multiplier_bonus: 0.0,
    }
}

fn amulet(index: i32, shape: i16, name: &str) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        item_type: ItemType::Amulet,
        shape,
        weight: 1,
        durability: 100,
        ..ItemInfo::default()
    }
}

/// 真实武器模板：属性写在 `ItemInfo.stats` 里，装备后由 [`World::refresh_stats`]
/// 汇总进 `player.stats`（Crystal `Stats[Stat.MinSC/MaxSC/PoisonAttack]` 的来源）。
fn weapon(index: i32, name: &str, min_sc: i32, max_sc: i32, poison_attack: i32) -> ItemInfo {
    let mut stats = Stats::new();
    if min_sc != 0 {
        stats.set(Stat::MinSC, min_sc);
    }
    if max_sc != 0 {
        stats.set(Stat::MaxSC, max_sc);
    }
    if poison_attack != 0 {
        stats.set(Stat::PoisonAttack, poison_attack);
    }
    ItemInfo {
        index,
        name: name.to_owned(),
        item_type: ItemType::Weapon,
        weight: 1,
        durability: 100,
        stats,
        ..ItemInfo::default()
    }
}

fn game_data(safe_zones: Vec<SafeZoneInfo>) -> Arc<GameData> {
    let db = MirDb {
        magics: vec![poison_cloud_info()],
        items: vec![
            amulet(AMULET, 0, "护身符"),
            amulet(GREEN_POISON, 1, "绿毒"),
            amulet(RED_POISON, 2, "红毒"),
            ItemInfo {
                index: POTION,
                name: "金创药(小量)".to_owned(),
                item_type: ItemType::Potion,
                weight: 1,
                ..ItemInfo::default()
            },
            // 真实装备模板：证明毒量读的是刷新后的有效 SC（不是随手改 stats）。
            weapon(SC_WEAPON_A, "木剑A", 7, 7, 0),
            weapon(SC_WEAPON_B, "木剑B", 17, 17, 0),
            weapon(POISON_WEAPON, "毒刃", 0, 0, 5),
        ],
        maps: vec![MapInfo {
            index: MAP_INDEX,
            file_name: MAP.to_owned(),
            title: "毒雾测试场".to_owned(),
            safe_zones,
            ..MapInfo::default()
        }],
        ..MirDb::default()
    };
    Arc::new(GameData::from_db(db, None))
}

/// 测试怪物：`min_mac` 是魔法防御（毒雾是 `DefenceType.MAC`）。
///
/// `ai = 56` 是 Crystal 的 Trainer（训练木桩）：`CanMove/CanAttack = false`，
/// 毒雾/毒的结算照常走 `process_monster` 的 `tick_monster_poison`（`MonsterObject.cs:1225-1229`）。
fn test_monster(hp: i32, min_mac: i32) -> MonsterKind {
    MonsterKind {
        name: "毒雾测试怪".to_owned(),
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
            min_mac,
            max_mac: min_mac,
            ..CombatStats::default()
        },
    }
}

/// 统计某个玩家的某类包。
fn count_spell_packets(packets: &[ServerPacket]) -> usize {
    packets
        .iter()
        .filter(|packet| matches!(packet, ServerPacket::ObjectSpell(_)))
        .count()
}

fn object_remove_ids(packets: &[ServerPacket]) -> Vec<u32> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectRemove(packet) => Some(packet.object_id),
            _ => None,
        })
        .collect()
}

/// `S.ObjectSpell` 的 `(object_id, location, spell)`：首跳 Show 的断言都基于它。
fn object_spells(packets: &[ServerPacket]) -> Vec<(u32, Point, Spell)> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectSpell(packet) => {
                Some((packet.object_id, packet.location, packet.spell))
            }
            _ => None,
        })
        .collect()
}

/// 收 `S.ObjectMagic` 的 `(spell, cast)`：材料失败时观察者也必须看到 `cast = false`。
fn object_magic_casts(packets: &[ServerPacket]) -> Vec<(Spell, bool)> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectMagic(packet) => Some((packet.spell, packet.cast)),
            _ => None,
        })
        .collect()
}

/// 60×60 可走地形（v0 地图格式的合成字节）；`walls` 里的格子设为高墙。
///
/// `Map` 的 `attribute` 是私有枚举，集成测试无法直接构造 `MapCell`；这里走公开的
/// [`Map::from_bytes_named`]（v0：52 字节头 + 每格 12 字节，列优先
/// `offset = 52 + (x * Height + y) * 12`，`o+0` 的 bit15 = 高墙，同
/// `crates/mir-server/tests/skills.rs:248` 的做法）。
fn synthetic_map_with_walls(walls: &[Point]) -> Arc<Map> {
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
// 1. 耗材矩阵（D2）：公共检查之后才查材料，失败不回滚公共副作用
// ---------------------------------------------------------------------------

/// 正例：符 5 + 绿毒 5 都齐 → 成功落地；`S.Magic`/`S.ObjectMagic` 的 `cast = true`；
/// 两个 `S.DeleteItem{Count: 5}` 各扣一截，扣完的槽位被摘掉。
#[test]
fn round52_poison_cloud_consumes_both_after_common_cast_checks() {
    let mut harness = Harness::new(1);
    let player = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 30, Point::new(20, 26));
    harness.learn(player, Spell::PoisonCloud);
    // 护身符多放 1 个，验证「只扣 5、不是整堆」。
    harness.equip(player, AMULET_SLOT, AMULET, 6);
    harness.equip(player, POISON_SLOT, GREEN_POISON, 5);
    harness.ready_all(player);
    harness.drain(player);
    harness.drain(observer);

    let mp_before = harness.world.player(player).expect("玩家").mp;
    let now = harness.world.now();
    let amulet_uid = harness.equipment_uid(player, AMULET_SLOT).expect("护身符");
    let poison_uid = harness.equipment_uid(player, POISON_SLOT).expect("绿毒");
    // 落点 (22,20)：距离 2 → 延迟 2 * 50 + 500 = 600ms。
    let report = harness
        .cast(
            player,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            Point::new(22, 20),
        )
        .expect("材料齐备时施法成功");
    assert_eq!(report.spell, Spell::PoisonCloud);
    assert_eq!(report.cost, 5, "0 级毒雾基础耗蓝 5");
    assert!(report.damage.is_some(), "施法时已抽好 MAC 伤害");
    assert_eq!(report.location, Point::new(22, 20));

    // 公共副作用（MP / 计时器）照常发生；本技能 CastTime 也写入了。
    let caster = harness.world.player(player).expect("玩家");
    assert_eq!(caster.mp, mp_before - report.cost, "成功施法扣蓝");
    assert_eq!(
        caster.action_time,
        now + 600,
        "ActionTime = now + MoveDelay（`HumanObject.cs:3429`）"
    );
    assert_eq!(
        caster.magic.spell_time,
        now + 1_800,
        "SpellTime = now + 1800（`HumanObject.cs:3425`）"
    );
    assert_eq!(
        caster.magic.casts.get(&Spell::PoisonCloud).copied(),
        Some(now),
        "成功施法写本技能 CastTime（`HumanObject.cs:3773-3776`）"
    );

    // 材料扣减：护身符 6 → 1；绿毒 5 → 槽位摘除。
    assert_eq!(
        harness.equipment(player, AMULET_SLOT),
        Some((AMULET, 1)),
        "护身符只扣 5 个"
    );
    assert_eq!(
        harness.equipment(player, POISON_SLOT),
        None,
        "绿毒整堆 5 个扣完，槽位被摘掉"
    );

    // 两条 `S.DeleteItem`：护身符 unique_id + 5，绿毒 unique_id + 5（B1 原语按 UUID 发包）。
    let packets = harness.packets(player);
    let deletes: Vec<(u64, u16)> = packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::DeleteItem(packet) => Some((packet.unique_id, packet.count)),
            _ => None,
        })
        .collect();
    assert_eq!(
        deletes,
        vec![(amulet_uid, 5), (poison_uid, 5)],
        "两种材料各发一条 DeleteItem，带各自的 unique_id 与数量 5"
    );
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::Magic(packet) if packet.cast && packet.spell == Spell::PoisonCloud
        )),
        "本人收到 S.Magic{{Cast: true}}"
    );
    let observer_packets = harness.packets(observer);
    assert!(
        observer_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectMagic(packet) if packet.cast && packet.spell == Spell::PoisonCloud
        )),
        "周围玩家收到 S.ObjectMagic{{Cast: true}}"
    );

    // 600ms 前不落地。
    harness.elapse(599);
    assert!(
        harness.world.spell_fields().is_empty(),
        "距离 2 格时 600ms 才落地（`HumanObject.cs:4546`）"
    );
    harness.elapse(1);
    assert_eq!(harness.world.spell_fields().len(), 9, "落点 3×3 共 9 格");
}

/// 反例矩阵（每个子用例独立世界）：
/// 缺符 / 缺毒 / 错类型 / 错形状 / 单堆不足 / 只在背包没装备 / 跨堆不合并。
///
/// 共同断言（D2）：返回 `Ok`（不是 `Err`）、`cast = false`；MP 已扣、`ActionTime`/
/// `SpellTime` 已写，但 `casts` 为空、不排效果、材料一个都不动。
#[test]
fn round52_poison_cloud_material_failure_matrix_keeps_common_effects() {
    /// 一个独立的「材料失败」用例：返回（扣了多少 MP，是否扣材料）。
    struct Case {
        name: &'static str,
        setup: fn(&mut Harness, u32),
    }

    let cases = [
        Case {
            name: "缺护身符",
            setup: |harness, player| {
                harness.equip(player, POISON_SLOT, GREEN_POISON, 5);
            },
        },
        Case {
            name: "缺绿毒",
            setup: |harness, player| {
                harness.equip(player, AMULET_SLOT, AMULET, 5);
            },
        },
        Case {
            name: "错类型（药水当护身符）",
            setup: |harness, player| {
                harness.equip(player, AMULET_SLOT, POTION, 5);
                harness.equip(player, POISON_SLOT, GREEN_POISON, 5);
            },
        },
        Case {
            name: "错形状（红毒 shape 2 不能当绿毒 shape 1）",
            setup: |harness, player| {
                harness.equip(player, AMULET_SLOT, AMULET, 5);
                harness.equip(player, POISON_SLOT, RED_POISON, 5);
            },
        },
        Case {
            name: "单堆不足（护身符只有 4 个）",
            setup: |harness, player| {
                harness.equip(player, AMULET_SLOT, AMULET, 4);
                harness.equip(player, POISON_SLOT, GREEN_POISON, 5);
            },
        },
        Case {
            name: "单堆不足（绿毒只有 4 个）",
            setup: |harness, player| {
                harness.equip(player, AMULET_SLOT, AMULET, 5);
                harness.equip(player, POISON_SLOT, GREEN_POISON, 4);
            },
        },
        Case {
            name: "只在背包、没有装备",
            setup: |harness, player| {
                // B2c R1：显式给足 Count（>4）。默认 `create_fresh_item` 的 Count=1，
                // 若只放默认数量，「错误地从背包找材料」的变异会因 1 < 5 照样全绿，
                // 反例前提就不成立。
                let info = harness.world.item_info(AMULET).expect("模板").clone();
                let mut item = harness.world.create_fresh_item(&info);
                item.count = 5;
                assert!(harness.world.add_item_to_inventory(player, item));
                let info = harness.world.item_info(GREEN_POISON).expect("模板").clone();
                let mut item = harness.world.create_fresh_item(&info);
                item.count = 7;
                assert!(harness.world.add_item_to_inventory(player, item));
            },
        },
        Case {
            name: "跨堆不合并（两个 3 个的护身符）",
            setup: |harness, player| {
                harness.equip(player, AMULET_SLOT, AMULET, 3);
                harness.equip(player, AMULET_SLOT + 1, AMULET, 3);
                harness.equip(player, POISON_SLOT, GREEN_POISON, 5);
            },
        },
    ];

    for case in cases {
        let mut harness = Harness::new(2);
        let player = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
        let observer = harness.add_player(2, MirClass::Wizard, 30, Point::new(20, 26));
        harness.learn(player, Spell::PoisonCloud);
        (case.setup)(&mut harness, player);
        if case.name == "只在背包、没有装备" {
            // B2c R1：先证反例前提——背包里确实有足够材料（UUID/index/count），
            // 且装备栏里没有同 Type/Shape 的材料；否则「错误搜背包」的变异仍会失败。
            let backpack = harness.inventory_snapshot(player);
            assert!(
                backpack
                    .iter()
                    .any(|(uid, index, count)| *uid != 0 && *index == AMULET && *count == 5),
                "反例前提：背包里应有 5 个护身符，实际 {backpack:?}"
            );
            assert!(
                backpack
                    .iter()
                    .any(|(uid, index, count)| *uid != 0 && *index == GREEN_POISON && *count == 7),
                "反例前提：背包里应有 7 个绿毒，实际 {backpack:?}"
            );
            let equipment: Vec<(usize, i32, u16)> = harness
                .world
                .player(player)
                .expect("玩家在世界上")
                .equipment
                .iter()
                .enumerate()
                .filter_map(|(slot, item)| {
                    item.as_ref()
                        .map(|item| (slot, item.item_index, item.count))
                })
                .collect();
            assert!(
                equipment.is_empty(),
                "反例前提：装备栏应为空（材料只在背包），实际 {equipment:?}"
            );
            for (slot, index, _) in &equipment {
                let info = harness.world.item_info(*index).expect("物品模板");
                assert!(
                    !(info.item_type == ItemType::Amulet && matches!(info.shape, 0 | 1)),
                    "反例前提：装备栏 {slot} 不得有同 Type/Shape 的材料"
                );
            }
        }
        harness.ready_all(player);
        harness.drain(player);
        harness.drain(observer);

        let mp_before = harness.world.player(player).expect("玩家").mp;
        let now = harness.world.now();
        let inventory_before = harness.inventory_snapshot(player);
        let report = harness
            .cast(
                player,
                Spell::PoisonCloud,
                // 预置一个非默认方向：材料失败也必须保留公共 `Direction`（D2）。
                MirDirection::Up,
                0,
                Point::new(21, 20),
            )
            .unwrap_or_else(|error| panic!("{}：材料失败不返回 Err，实际 {error:?}", case.name));

        // D2：cast = false → 不写 CastTime、不排效果；公共副作用保留。
        assert!(report.damage.is_none(), "{}：材料失败时没有伤害", case.name);
        let caster = harness.world.player(player).expect("玩家");
        assert_eq!(
            caster.mp,
            mp_before - report.cost,
            "{}：材料失败仍扣公共 MP（D2）",
            case.name
        );
        assert_eq!(
            caster.action_time,
            now + 600,
            "{}：材料失败保留动作时间（D2）",
            case.name
        );
        assert_eq!(
            caster.attack_time,
            now + 600,
            "{}：材料失败保留 AttackTime（D2）",
            case.name
        );
        assert_eq!(
            caster.magic.spell_time,
            now + 1_800,
            "{}：材料失败保留 SpellTime（D2）",
            case.name
        );
        assert_eq!(
            caster.direction,
            MirDirection::Up,
            "{}：材料失败保留公共 Direction（D2）",
            case.name
        );
        assert!(
            !caster.magic.casts.contains_key(&Spell::PoisonCloud),
            "{}：材料失败不写本技能 CastTime（D2）",
            case.name
        );
        assert_eq!(
            harness.inventory_snapshot(player),
            inventory_before,
            "{}：材料失败连背包内容也不动（含「只在背包」子例）",
            case.name
        );

        // 材料一个都不动（含「缺第二项不得先扣第一项」）。
        let amulet_after = harness.equipment(player, AMULET_SLOT);
        let amulet_after_7 = harness.equipment(player, AMULET_SLOT + 1);
        let poison_after = harness.equipment(player, POISON_SLOT);
        match case.name {
            "缺护身符" => {
                assert_eq!(amulet_after, None);
                assert_eq!(poison_after, Some((GREEN_POISON, 5)));
            }
            "缺绿毒" => {
                assert_eq!(amulet_after, Some((AMULET, 5)), "缺第二项不得先扣第一项");
                assert_eq!(poison_after, None);
            }
            "错类型（药水当护身符）" => {
                assert_eq!(amulet_after, Some((POTION, 5)), "非护身符不被扣");
                assert_eq!(poison_after, Some((GREEN_POISON, 5)));
            }
            "错形状（红毒 shape 2 不能当绿毒 shape 1）" => {
                assert_eq!(amulet_after, Some((AMULET, 5)));
                assert_eq!(poison_after, Some((RED_POISON, 5)), "红毒不被当绿毒扣");
            }
            "单堆不足（护身符只有 4 个）" => {
                assert_eq!(amulet_after, Some((AMULET, 4)), "不足 5 个不扣");
                assert_eq!(poison_after, Some((GREEN_POISON, 5)));
            }
            "单堆不足（绿毒只有 4 个）" => {
                assert_eq!(amulet_after, Some((AMULET, 5)));
                assert_eq!(poison_after, Some((GREEN_POISON, 4)));
            }
            "只在背包、没有装备" => {
                assert_eq!(amulet_after, None);
                assert_eq!(poison_after, None);
            }
            "跨堆不合并（两个 3 个的护身符）" => {
                assert_eq!(amulet_after, Some((AMULET, 3)), "3 + 3 不合并成 6");
                assert_eq!(amulet_after_7, Some((AMULET, 3)));
                assert_eq!(poison_after, Some((GREEN_POISON, 5)));
            }
            _ => unreachable!(),
        }

        // 600ms 后没有落地、没有 `S.ObjectSpell`、没有 `S.DeleteItem`。
        harness.elapse(600);
        assert!(
            harness.world.spell_fields().is_empty(),
            "{}：材料失败不排效果",
            case.name
        );
        let packets = harness.packets(player);
        assert_eq!(count_spell_packets(&packets), 0);
        assert!(
            packets
                .iter()
                .all(|packet| !matches!(packet, ServerPacket::DeleteItem(_))),
            "{}：材料失败不发 DeleteItem",
            case.name
        );
        assert!(
            packets.iter().any(|packet| matches!(
                packet,
                ServerPacket::Magic(packet) if !packet.cast
            )),
            "{}：材料失败仍回显 S.Magic{{Cast: false}}（`HumanObject.cs:3778`）",
            case.name
        );
        // 观察者也要看到 `S.ObjectMagic{Cast: false}`（Crystal 无条件 Broadcast，`HumanObject.cs:3779`）。
        let observer_packets = harness.packets(observer);
        assert!(
            object_magic_casts(&observer_packets).contains(&(Spell::PoisonCloud, false)),
            "{}：材料失败时观察者收到 S.ObjectMagic{{Cast: false}}，实际 {:?}",
            case.name,
            object_magic_casts(&observer_packets)
        );
        assert!(
            object_magic_casts(&observer_packets)
                .iter()
                .all(|(_, cast)| !cast),
            "{}：观察者不应看到 Cast: true",
            case.name
        );
    }
}

/// 0 耐久的护身符照样能烧（Crystal `GetAmulet` 不看耐久，`HumanObject.cs:6859`）。
#[test]
fn round52_poison_cloud_zero_durability_material_is_consumed() {
    let mut harness = Harness::new(3);
    let player = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    harness.learn(player, Spell::PoisonCloud);
    harness.equip_durability(player, AMULET_SLOT, AMULET, 5, 0);
    harness.equip(player, POISON_SLOT, GREEN_POISON, 5);
    harness.ready_all(player);
    harness.drain(player);

    let report = harness
        .cast(
            player,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            Point::new(20, 21),
        )
        .expect("0 耐久护身符不影响施法");
    assert!(report.damage.is_some());
    assert_eq!(harness.equipment(player, AMULET_SLOT), None);
    assert_eq!(harness.equipment(player, POISON_SLOT), None);
    harness.elapse(550);
    assert_eq!(
        harness.world.spell_fields().len(),
        9,
        "距离 1 → 550ms 落地，3×3 齐全"
    );
}

// ---------------------------------------------------------------------------
// 2. 落地/网格/跳毒/到期
// ---------------------------------------------------------------------------

#[test]
fn round52_poison_cloud_grid_tick_poison_and_expiry() {
    let mut harness = Harness::new(4);
    let caster = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 30, Point::new(20, 26));
    harness.learn(caster, Spell::PoisonCloud);
    harness.equip(caster, AMULET_SLOT, AMULET, 10);
    harness.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    harness.ready_all(caster);
    harness.drain(caster);
    harness.drain(observer);

    // 3×3 内：中心 (22,20)、邻居 (21,21)；3×3 外：(23,22)。
    let center = Point::new(22, 20);
    let inside = Point::new(21, 21);
    let outside = Point::new(23, 22);
    let center_monster = harness.spawn(test_monster(100_000, 0), center);
    let inside_monster = harness.spawn(test_monster(100_000, 0), inside);
    let outside_monster = harness.spawn(test_monster(100_000, 0), outside);

    // 有效 SC 必须来自真实装备 + RefreshStats（不是直接改 stats）：先记录基础 SC，
    // 换上武器 A 再刷，断言确实变了。
    let base_sc = {
        let stats = harness.world.player(caster).expect("玩家").stats;
        (stats.min_sc + stats.max_sc) / 2
    };
    harness.equip(caster, WEAPON_SLOT, SC_WEAPON_A, 1);
    harness.refresh_stats(caster);
    let sc = {
        let stats = harness.world.player(caster).expect("玩家").stats;
        (stats.min_sc + stats.max_sc) / 2
    };
    assert_ne!(sc, base_sc, "装备 + RefreshStats 必须刷新有效 SC");
    let hp_before = (
        harness.hp(center_monster),
        harness.hp(inside_monster),
        harness.hp(outside_monster),
    );

    let report = harness
        .cast(caster, Spell::PoisonCloud, MirDirection::Down, 0, center)
        .expect("施法成功");
    let damage = report.damage.expect("毒雾有伤害");
    let landed_at = harness.world.now() + 600;
    harness.elapse(599);
    assert!(harness.world.spell_fields().is_empty(), "600ms 前不落地");

    // 落地那一拍（`SpellObject.cs:111-112` 的 `TickTime = 0` → 首跳立即）。
    harness.elapse(1);
    assert_eq!(harness.world.now(), landed_at);
    let fields: Vec<_> = harness.world.spell_fields().to_vec();
    assert_eq!(fields.len(), 9, "落点 3×3 共 9 格");
    let mut centers: Vec<(i32, i32)> = fields
        .iter()
        .map(|field| (field.center.x, field.center.y))
        .collect();
    centers.sort_unstable();
    let mut expected = Vec::new();
    for dy in -1..=1 {
        for dx in -1..=1 {
            expected.push((center.x + dx, center.y + dy));
        }
    }
    expected.sort_unstable();
    assert_eq!(centers, expected);
    assert!(
        fields.iter().all(|field| field.spell == Spell::PoisonCloud),
        "全部登记成毒雾场"
    );
    assert!(
        fields.iter().all(|field| field.damage == damage),
        "9 格共享施法时抽到的 Value（`Map.cs:1448`）"
    );
    assert!(
        fields.iter().all(|field| field.bonus_damage == 0),
        "没有 PoisonAttack 装备/增益时 BonusDmg = 0（0 时不抽 RNG，见 _bonus_damage 用例）"
    );
    assert!(
        fields.iter().all(|field| field.cast_location == center),
        "CastLocation 是整次施法的落点"
    );
    assert!(
        fields.iter().all(|field| field.tick_ms == 1_000),
        "TickSpeed = 1000（`Map.cs:1451`）"
    );
    assert!(
        fields
            .iter()
            .all(|field| field.expire_at == landed_at + 6_000),
        "ExpireTime = 落地 + 6000（`Map.cs:1450`）"
    );
    // `Show` 语义：9 格里只有首个实际生成的对象为 true。
    assert_eq!(
        fields.iter().filter(|field| field.show).count(),
        1,
        "只有首个实际生成的对象 Show = true（`Map.cs:1452-1462`）"
    );
    // 客户端只看到一条 `S.ObjectSpell`（`SpellObject.GetInfo` 对 `!Show` 返回 null）。
    let observer_packets = harness.packets(observer);
    let spell_packets: Vec<_> = observer_packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectSpell(packet) => Some(packet),
            _ => None,
        })
        .collect();
    assert_eq!(spell_packets.len(), 1, "整片毒雾只广播一次");
    assert_eq!(spell_packets[0].spell, Spell::PoisonCloud);
    assert_eq!(spell_packets[0].location, center, "广播的是 CastLocation");

    // 首跳：中心格和 3×3 内的怪都掉血（MAC），3×3 外的怪不动。
    assert!(harness.hp(center_monster) < hp_before.0, "中心格吃首跳");
    assert!(harness.hp(inside_monster) < hp_before.1, "3×3 内吃首跳");
    assert_eq!(harness.hp(outside_monster), hp_before.2, "3×3 外不吃");

    // 存活目标被附 12 跳绿毒：毒量按当时的 (MinSC + MaxSC) / 2 + BonusDmg。
    let poisons = harness
        .world
        .monster(center_monster)
        .expect("怪")
        .poisons
        .clone();
    assert_eq!(poisons.len(), 1, "绿毒只有一份（同 kind 合并）");
    let green = &poisons[0];
    assert_eq!(green.kind, PoisonType::Green);
    assert_eq!(green.value, sc, "毒量 = (MinSC + MaxSC) / 2 + 0");
    assert_eq!(green.duration, 12, "Duration = 12 跳");
    assert_eq!(green.tick_speed_ms, 1_000, "毒与毒雾同 TickSpeed");
    assert_eq!(green.owner_id, caster);

    // 首跳后先把绿毒清掉：本段只验证毒雾的网格/跳时/到期，避免毒的自主结算干扰 HP 断言；
    // 毒雾每跳重新附毒与 12 跳毒由下面的观察段单独覆盖。
    harness
        .world
        .monster_mut(center_monster)
        .expect("怪")
        .poisons
        .clear();
    let hp_before_second_tick = harness.hp(center_monster);

    // 毒雾第二跳在落地 +1000ms；+999ms 时既无伤害也不能掉场。
    harness.elapse(999);
    assert_eq!(harness.world.now(), landed_at + 999);
    assert_eq!(
        harness.world.spell_fields().len(),
        9,
        "落地 +999ms 时毒雾还在（还没到第二跳）"
    );
    assert_eq!(
        harness.hp(center_monster),
        hp_before_second_tick,
        "落地 +999ms 没有毒雾第二跳伤害"
    );

    // 换装（真实装备 B）+ RefreshStats：毒雾下一跳重新附毒时必须用**当时**的有效 SC
    // （`SpellObject.cs:167-171` 每跳现算 `(MinSC + MaxSC) / 2`）。
    harness.equip(caster, WEAPON_SLOT, SC_WEAPON_B, 1);
    harness.refresh_stats(caster);
    let sc_after = {
        let stats = harness.world.player(caster).expect("玩家").stats;
        (stats.min_sc + stats.max_sc) / 2
    };
    assert_ne!(sc_after, sc, "换武器 B 后有效 SC 必须变化");
    harness.elapse(1);
    assert!(
        harness.hp(center_monster) < hp_before_second_tick,
        "落地 +1000ms 毒雾第二跳（`SpellObject.cs:111-112`）"
    );
    let green = harness.world.monster(center_monster).expect("怪").poisons[0];
    assert_eq!(
        green.value, sc_after,
        "每跳按当时的 (MinSC + MaxSC) / 2 重新计算毒量"
    );
    assert_eq!(green.ticks, 0, "毒雾每跳重新附毒，12 跳从这一拍重新计时");

    // 可选增强（B2c）：落地 +6000ms 的最后一跳仍只结算一次 Value；先清毒避免毒伤叠加。
    harness
        .world
        .monster_mut(center_monster)
        .expect("怪")
        .poisons
        .clear();
    let hp_before_last_tick = harness.hp(center_monster);
    // 到期边界：6000ms 这一刻仍结算（Crystal 用严格 `Envir.Time > ExpireTime`）。
    let now = harness.world.now();
    harness.elapse(landed_at + 6_000 - now);
    assert_eq!(harness.world.now(), landed_at + 6_000);
    assert_eq!(
        hp_before_last_tick - harness.hp(center_monster),
        damage,
        "落地 +6000ms 的最后一跳照常结算一次 Value（严格 `>` 到期，`SpellObject.cs:66`）"
    );
    assert_eq!(
        harness.world.spell_fields().len(),
        9,
        "落地 +6000ms 这一刻还没到期（`SpellObject.cs:66` 严格大于）"
    );
    harness.elapse(100);
    assert!(
        harness.world.spell_fields().is_empty(),
        "落地 +6000ms 之后的下一个 world tick 整片消失"
    );
    let obs = harness.packets(observer);
    let removed = object_remove_ids(&obs);
    assert_eq!(removed.len(), 9, "9 格各广播一条 S.ObjectRemove");

    // 12 跳绿毒：毒雾到期后毒继续按 1000ms 结算；怪物口径最后一跳不结算
    // （`MonsterObject.cs:1476-1491`），跳满 12 跳后整条移除。
    let hp_before_poison = harness.hp(center_monster);
    let poison_value = harness.world.monster(center_monster).expect("怪").poisons[0].value;
    assert_eq!(poison_value, sc_after);
    let mut observed = 0;
    let mut last_ticks = harness.world.monster(center_monster).expect("怪").poisons[0].ticks;
    let mut iterations = 0;
    loop {
        harness.elapse(100);
        iterations += 1;
        assert!(iterations <= 400, "绿毒应该在 12 跳内移除");
        let Some(poison) = harness
            .world
            .monster(center_monster)
            .expect("怪")
            .poisons
            .first()
            .copied()
        else {
            break;
        };
        assert!(poison.ticks <= 12, "毒 tick 不会超过 Duration");
        if poison.ticks > last_ticks {
            observed += poison.ticks - last_ticks;
            last_ticks = poison.ticks;
        }
    }
    assert!(
        observed >= 10,
        "毒雾到期后毒继续结算（观察到 {observed} 跳）"
    );
    assert_eq!(
        hp_before_poison - harness.hp(center_monster),
        i32::try_from(observed).expect("跳数小") * poison_value,
        "每一跳扣血恰为 poison.value"
    );
}

/// 同格同类的毒雾不叠加（`Map.cs:1455-1466`）：第二次落在同一格 3×3 上不会新增场。
#[test]
fn round52_poison_cloud_does_not_stack_on_same_cells() {
    let mut harness = Harness::new(5);
    let caster = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 30, Point::new(20, 26));
    harness.learn(caster, Spell::PoisonCloud);
    harness.equip(caster, AMULET_SLOT, AMULET, 10);
    harness.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    harness.ready_all(caster);
    harness.drain(caster);
    harness.drain(observer);
    let location = Point::new(21, 20);

    harness
        .cast(caster, Spell::PoisonCloud, MirDirection::Down, 0, location)
        .expect("第一次施法");
    harness.elapse(550);
    assert_eq!(harness.world.spell_fields().len(), 9);
    assert_eq!(
        count_spell_packets(&harness.packets(observer)),
        1,
        "第一次真正生成对象时广播一次 S.ObjectSpell"
    );

    let first_landing_experience = {
        let magic = harness
            .world
            .player(caster)
            .expect("玩家")
            .magics
            .iter()
            .find(|magic| magic.spell == Spell::PoisonCloud)
            .expect("已学毒雾");
        magic.experience
    };
    assert!(
        first_landing_experience > 0,
        "第一次落地即训练（`Map.cs:2356` 的 if (train)）"
    );

    // 第二次施法（清冷却），同一落点：9 格都已有同类场 → 一格都不新增。
    harness.ready_all(caster);
    harness
        .cast(caster, Spell::PoisonCloud, MirDirection::Down, 0, location)
        .expect("第二次施法");
    harness.elapse(550);
    assert_eq!(
        harness.world.spell_fields().len(),
        9,
        "同格同类不叠（`Map.cs:1455-1466`）"
    );
    let spell_packets = count_spell_packets(&harness.packets(observer));
    assert_eq!(
        spell_packets, 0,
        "第二次没有生成任何对象 → 没有新的 S.ObjectSpell（`Show` 只在首个生成时广播）"
    );

    // 训练：每次落地都 `LevelMagic`（`Map.cs:2356` 的 `if (train)`），与是否命中无关；
    // 即使第二次一格都没生成（同格不叠），`train` 仍为 true → 经验必须继续增长。
    let second_landing_experience = {
        let magic = harness
            .world
            .player(caster)
            .expect("玩家")
            .magics
            .iter()
            .find(|magic| magic.spell == Spell::PoisonCloud)
            .expect("已学毒雾");
        magic.experience
    };
    assert!(
        second_landing_experience > first_landing_experience,
        "第二次落地继续加经验（{first_landing_experience} → {second_landing_experience}）"
    );
}

// ---------------------------------------------------------------------------
// 3. PK / 安全区 / 施法者与目标生命周期
// ---------------------------------------------------------------------------

/// 安全区三例共用：攻守双方都设 `AttackMode::All`，把「不可攻击」唯一归因到安全区规则
/// （评审：旧用例 victim 默认 `Peace`，无法证明是安全区挡的）。
///
/// 返回 `(hp_before, hp_after, victim 毒数, 场格数)`。
fn safe_zone_case(
    seed: u64,
    zone: SafeZoneInfo,
    caster_pos: Point,
    victim_pos: Point,
    cast_location: Point,
) -> (i32, i32, usize, usize) {
    let mut harness = Harness::with_data(seed, game_data(vec![zone]));
    let caster = harness.add_player(1, MirClass::Wizard, 30, caster_pos);
    let victim = harness.add_player(2, MirClass::Wizard, 30, victim_pos);
    harness.learn(caster, Spell::PoisonCloud);
    harness.equip(caster, AMULET_SLOT, AMULET, 5);
    harness.equip(caster, POISON_SLOT, GREEN_POISON, 5);
    harness.set_attack_mode(caster, AttackMode::All);
    harness.set_attack_mode(victim, AttackMode::All);
    harness.ready_all(caster);
    harness.drain(caster);
    harness.drain(victim);
    let hp_before = harness.player_hp(victim);
    harness
        .cast(
            caster,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            cast_location,
        )
        .expect("施法成功");
    let delay = i64::from(caster_pos.max_distance(cast_location)) * 50 + 500;
    harness.elapse(delay);
    let hp_after = harness.player_hp(victim);
    let poison_count = harness
        .world
        .player(victim)
        .expect("玩家")
        .magic
        .poisons
        .len();
    (
        hp_before,
        hp_after,
        poison_count,
        harness.world.spell_fields().len(),
    )
}

#[test]
fn round52_poison_cloud_pk_and_caster_lifecycle() {
    // --- 安全区三例：双方都 `AttackMode::All`，隔离攻方/受方在区（`MapObject.cs:445-450`） ---
    let zone = || SafeZoneInfo {
        location: Point::new(22, 20),
        size: 1,
        start_point: false,
    };
    // A：只有受方在安全区 → 不可打。
    let (before, after, poison, fields) = safe_zone_case(
        6,
        zone(),
        Point::new(18, 20),
        Point::new(22, 20),
        Point::new(22, 20),
    );
    assert_eq!(fields, 9, "安全区不影响毒雾生成（Crystal 只挡伤害）");
    assert_eq!(after, before, "受方在安全区 → IsAttackTarget 拒绝");
    assert_eq!(poison, 0, "不被伤害就不附毒（`SpellObject.cs:163-174`）");
    // B：只有攻方在安全区 → 同样不可打。
    let (before, after, poison, fields) = safe_zone_case(
        12,
        zone(),
        Point::new(22, 20),
        Point::new(18, 20),
        Point::new(18, 20),
    );
    assert_eq!(fields, 9, "攻方在安全区也不影响毒雾生成");
    assert_eq!(after, before, "攻方在安全区 → IsAttackTarget 拒绝");
    assert_eq!(poison, 0);
    // C：双方都不在安全区 → 成功对照（证明确实是安全区规则挡的，不是用例本身失效）。
    let (before, after, poison, fields) = safe_zone_case(
        13,
        zone(),
        Point::new(18, 20),
        Point::new(16, 20),
        Point::new(16, 20),
    );
    assert_eq!(fields, 9);
    assert!(after < before, "区外成功对照：玩家目标吃 MAC 伤害");
    assert_eq!(poison, 1, "区外成功对照：存活玩家被附绿毒");

    // --- PK：都不在安全区、攻守双方都 All → 正常伤害 + 绿毒 ---
    let mut pk = Harness::new(7);
    let caster = pk.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    let victim = pk.add_player(2, MirClass::Wizard, 30, Point::new(22, 20));
    pk.learn(caster, Spell::PoisonCloud);
    pk.equip(caster, AMULET_SLOT, AMULET, 5);
    pk.equip(caster, POISON_SLOT, GREEN_POISON, 5);
    pk.set_attack_mode(caster, AttackMode::All);
    pk.set_attack_mode(victim, AttackMode::All);
    pk.ready_all(caster);
    pk.drain(caster);
    pk.drain(victim);
    let victim_hp = pk.player_hp(victim);
    let report = pk
        .cast(
            caster,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            Point::new(22, 20),
        )
        .expect("施法成功");
    let damage = report.damage.expect("毒雾伤害");
    pk.elapse(600);
    assert!(
        pk.player_hp(victim) < victim_hp,
        "PK 模式（All）下玩家目标吃 MAC 伤害（`MapObject.cs:428-460`）"
    );
    let poison = pk.world.player(victim).expect("玩家").magic.poisons.clone();
    assert_eq!(poison.len(), 1, "存活玩家被附绿毒");
    assert_eq!(poison[0].kind, PoisonType::Green);
    assert!(damage > 0, "毒雾 Core 伤害应为正数（实际 {damage}）");

    // --- 施法者死亡：Crystal 不删场，只是 Caster 变死对象（IsAttackTarget 不看攻方 Dead） ---
    let monster = pk.spawn(test_monster(100_000, 0), Point::new(22, 20));
    if let Some(player) = pk.world.player_mut(caster) {
        player.dead = true;
        player.hp = 0;
    }
    // 等毒雾的下一跳（`next_tick`），施法者死亡不应阻止结算。
    let wait = pk.world.spell_fields()[0].next_tick - pk.world.now();
    assert!(wait > 0, "毒雾下一跳应在未来");
    let hp_before = pk.hp(monster);
    pk.elapse(wait);
    assert!(
        pk.hp(monster) < hp_before,
        "施法者死亡不立即清场，毒雾照常结算（`SpellObject.cs:66` 的清场名单不含 PoisonCloud）"
    );

    // --- 已铺场：施法者下线 → 本作清场（D3 PARTIAL） ---
    let mut offline = Harness::new(8);
    let caster = offline.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    let observer = offline.add_player(2, MirClass::Wizard, 30, Point::new(20, 26));
    offline.learn(caster, Spell::PoisonCloud);
    offline.equip(caster, AMULET_SLOT, AMULET, 10);
    offline.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    offline.ready_all(caster);
    offline.drain(observer);
    offline
        .cast(
            caster,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            Point::new(21, 20),
        )
        .expect("施法成功");
    offline.elapse(550);
    assert_eq!(offline.world.spell_fields().len(), 9);
    let field_ids: Vec<u32> = offline
        .world
        .spell_fields()
        .iter()
        .map(|field| field.object_id)
        .collect();
    offline.drain(observer);
    assert!(offline.world.leave(caster).is_some(), "施法者下线");
    offline.elapse(1);
    assert!(
        offline.world.spell_fields().is_empty(),
        "已铺场阶段：施法者下线即清场（D3 已接受偏差）"
    );
    let removed = object_remove_ids(&offline.packets(observer));
    for id in field_ids {
        assert!(removed.contains(&id), "每格都广播 S.ObjectRemove（{id}）");
    }

    // --- pending 阶段：施法者下线 → 不生成任何场 ---
    let mut pending = Harness::new(9);
    let caster = pending.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    let observer = pending.add_player(2, MirClass::Wizard, 30, Point::new(20, 26));
    pending.learn(caster, Spell::PoisonCloud);
    pending.equip(caster, AMULET_SLOT, AMULET, 10);
    pending.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    pending.ready_all(caster);
    pending
        .cast(
            caster,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            Point::new(21, 20),
        )
        .expect("施法成功");
    pending.drain(observer);
    assert!(pending.world.leave(caster).is_some(), "延迟期间下线");
    pending.elapse(1_000);
    assert!(
        pending.world.spell_fields().is_empty(),
        "pending 阶段：施法者下线直接丢弃（D3 偏差）"
    );
    assert_eq!(
        count_spell_packets(&pending.packets(observer)),
        0,
        "不生成对象就不广播 S.ObjectSpell"
    );

    // --- pending 阶段：施法者换图 → 同样丢弃（D3 偏差；Crystal 延迟动作仍挂在旧图） ---
    let mut pending_moved = Harness::new(14);
    let caster = pending_moved.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    let observer = pending_moved.add_player(2, MirClass::Wizard, 30, Point::new(20, 26));
    pending_moved.learn(caster, Spell::PoisonCloud);
    pending_moved.equip(caster, AMULET_SLOT, AMULET, 10);
    pending_moved.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    pending_moved.ready_all(caster);
    pending_moved.drain(observer);
    pending_moved
        .cast(
            caster,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            Point::new(21, 20),
        )
        .expect("施法成功");
    pending_moved.set_map(caster, "其他地图");
    pending_moved.elapse(1_000);
    assert!(
        pending_moved.world.spell_fields().is_empty(),
        "pending 阶段施法者换图：本作直接丢弃（D3 已接受偏差）"
    );
    // B2c R2：不生成场就不该广播 S.ObjectSpell——用观察者实收包断言（而非只断言字段空）。
    assert_eq!(
        count_spell_packets(&pending_moved.packets(observer)),
        0,
        "pending 阶段换图丢弃时观察者不应收到 S.ObjectSpell(149)"
    );

    // --- 已铺场：施法者换图 → 本作清场（Crystal 只对 FireWall 强清，D3 PARTIAL） ---
    let mut moved = Harness::new(10);
    let caster = moved.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    let observer = moved.add_player(2, MirClass::Wizard, 30, Point::new(20, 26));
    moved.learn(caster, Spell::PoisonCloud);
    moved.equip(caster, AMULET_SLOT, AMULET, 10);
    moved.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    moved.ready_all(caster);
    moved.drain(observer);
    moved
        .cast(
            caster,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            Point::new(21, 20),
        )
        .expect("施法成功");
    moved.elapse(550);
    assert_eq!(moved.world.spell_fields().len(), 9);
    let field_ids: Vec<u32> = moved
        .world
        .spell_fields()
        .iter()
        .map(|field| field.object_id)
        .collect();
    moved.drain(observer);
    moved.set_map(caster, "其他地图");
    moved.elapse(1);
    assert!(
        moved.world.spell_fields().is_empty(),
        "施法者换图即清场（D3 已接受偏差；Crystal 毒雾会留到 ExpireTime）"
    );
    // 异图撤销也要发给观察者：逐格 `S.ObjectRemove(26)`。
    let removed = object_remove_ids(&moved.packets(observer));
    for id in field_ids {
        assert!(
            removed.contains(&id),
            "换图清场时每格都广播 S.ObjectRemove（{id}），实际 {removed:?}"
        );
    }

    // --- 已铺场：目标换图 → 不再吃伤害（`position == center` 且同图才结算） ---
    let mut target_moved = Harness::new(15);
    let caster = target_moved.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    let victim = target_moved.add_player(2, MirClass::Wizard, 30, Point::new(22, 20));
    target_moved.learn(caster, Spell::PoisonCloud);
    target_moved.equip(caster, AMULET_SLOT, AMULET, 10);
    target_moved.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    target_moved.set_attack_mode(caster, AttackMode::All);
    target_moved.set_attack_mode(victim, AttackMode::All);
    target_moved.ready_all(caster);
    target_moved
        .cast(
            caster,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            Point::new(22, 20),
        )
        .expect("施法成功");
    target_moved.elapse(600);
    assert_eq!(
        target_moved
            .world
            .player(victim)
            .expect("玩家")
            .magic
            .poisons
            .len(),
        1,
        "首跳命中后玩家被附绿毒（用于确认这片毒雾本来能伤到人）"
    );
    // 清掉已中的毒，切图后再等一整跳：不再掉血、也不再附毒。
    if let Some(player) = target_moved.world.player_mut(victim) {
        player.magic.poisons.clear();
        player.map = "其他地图".to_owned();
    }
    let hp_before = target_moved.player_hp(victim);
    let wait = target_moved.world.spell_fields()[0].next_tick - target_moved.world.now();
    target_moved.elapse(wait.max(1));
    assert_eq!(
        target_moved.player_hp(victim),
        hp_before,
        "目标换图后毒雾不再结算伤害"
    );
    assert!(
        target_moved
            .world
            .player(victim)
            .expect("玩家")
            .magic
            .poisons
            .is_empty(),
        "目标换图后不再被附毒"
    );

    // --- 已铺场：目标下线 → 对象不在场，跳过（下线期间不结算） ---
    let mut target_offline = Harness::new(16);
    let caster = target_offline.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    let victim = target_offline.add_player(2, MirClass::Wizard, 30, Point::new(22, 20));
    target_offline.learn(caster, Spell::PoisonCloud);
    target_offline.equip(caster, AMULET_SLOT, AMULET, 10);
    target_offline.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    target_offline.set_attack_mode(caster, AttackMode::All);
    target_offline.set_attack_mode(victim, AttackMode::All);
    target_offline.ready_all(caster);
    target_offline
        .cast(
            caster,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            Point::new(22, 20),
        )
        .expect("施法成功");
    target_offline.elapse(600);
    assert_eq!(
        target_offline
            .world
            .player(victim)
            .expect("玩家")
            .magic
            .poisons
            .len(),
        1,
        "首跳命中后玩家被附绿毒"
    );
    // 清掉已中的毒，避免离场后重登时毒的自主结算干扰本段。
    if let Some(player) = target_offline.world.player_mut(victim) {
        player.magic.poisons.clear();
    }
    target_offline.drain(victim);
    let player = target_offline.world.leave(victim).expect("目标下线");
    let hp_at_leave = player.hp;
    assert_eq!(
        target_offline.world.spell_fields().len(),
        9,
        "目标下线不影响毒雾场"
    );
    // B2c R2：真实跨跳证明——目标离场期间仍推进一个活跃毒雾 tick（确认没有陈旧伤害被
    // 排队），再推进到毒雾严格到期之后重登，从 World 读取 HP/毒/包序列。
    // 注意：活跃毒雾在场时同格重入仍会被下一跳命中，本用例只证明离场期间的跳不会被补结算。
    let next_tick = target_offline.world.spell_fields()[0].next_tick;
    let wait = next_tick - target_offline.world.now();
    assert!(wait > 0, "离场后应能等到至少一个毒雾 tick");
    target_offline.elapse(wait);
    assert!(
        target_offline.world.player(victim).is_none(),
        "离场期间 World 内没有该对象"
    );
    assert_eq!(
        target_offline.world.spell_fields().len(),
        9,
        "目标离场不影响毒雾场"
    );
    let offline_packets = target_offline.packets(victim);
    assert!(
        offline_packets.is_empty(),
        "离线目标不该收到任何包（含伤害/毒雾），实际 {offline_packets:?}"
    );
    let expire_at = target_offline.world.spell_fields()[0].expire_at;
    let wait = expire_at + 100 - target_offline.world.now();
    target_offline.elapse(wait.max(1));
    assert!(
        target_offline.world.spell_fields().is_empty(),
        "毒雾在目标离场期间到期清场"
    );
    target_offline.world.enter(player);
    target_offline.elapse(100);
    let reentered = target_offline.world.player(victim).expect("目标重登");
    assert_eq!(
        reentered.hp, hp_at_leave,
        "离场期间的毒雾 tick 没有把伤害排到离场对象上（重登后推进一个 tick 仍不掉血）"
    );
    assert!(reentered.magic.poisons.is_empty(), "离场期间没有毒被补附");

    // --- 伤害致死：本跳 MAC 打死目标 → 不附毒（`SpellObject.cs:163-174` 的 `if (!ob.Dead)`） ---
    let mut kill = Harness::new(17);
    let caster = kill.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    kill.learn(caster, Spell::PoisonCloud);
    kill.equip(caster, AMULET_SLOT, AMULET, 10);
    kill.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    kill.ready_all(caster);
    let fragile = kill.spawn(test_monster(1, 0), Point::new(21, 20));
    kill.cast(
        caster,
        Spell::PoisonCloud,
        MirDirection::Down,
        0,
        Point::new(21, 20),
    )
    .expect("施法成功");
    kill.elapse(550);
    let monster = kill
        .world
        .monster(fragile)
        .expect("怪物对象还在（尸体保留）");
    assert!(monster.dead, "首跳 MAC 伤害应当打死 1 血怪");
    assert!(
        monster.poisons.is_empty(),
        "本跳致死不得附毒（`SpellObject.cs:163` 先 Attacked 再判存活）"
    );

    // --- 非零 MAC 反例：同一片毒雾里，防御 0 的怪掉血多于防御 500 的怪 ---
    let mut mac = Harness::new(18);
    let caster = mac.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    mac.learn(caster, Spell::PoisonCloud);
    mac.equip(caster, AMULET_SLOT, AMULET, 10);
    mac.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    mac.ready_all(caster);
    let soft = mac.spawn(test_monster(100_000, 0), Point::new(21, 20));
    let hard = mac.spawn(test_monster(100_000, 500), Point::new(22, 20));
    mac.cast(
        caster,
        Spell::PoisonCloud,
        MirDirection::Down,
        0,
        Point::new(21, 20),
    )
    .expect("施法成功");
    mac.elapse(550);
    let soft_damage = 100_000 - mac.hp(soft);
    let hard_damage = 100_000 - mac.hp(hard);
    assert!(
        soft_damage > hard_damage,
        "毒雾走 DefenceType.MAC：0 防怪掉血 {soft_damage} 应多于 500 防怪 {hard_damage}"
    );

    // --- 目标死亡 ---
    let mut dead_target = Harness::new(11);
    let caster = dead_target.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    dead_target.learn(caster, Spell::PoisonCloud);
    dead_target.equip(caster, AMULET_SLOT, AMULET, 10);
    dead_target.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    dead_target.ready_all(caster);
    let monster = dead_target.spawn(test_monster(100_000, 0), Point::new(21, 20));
    if let Some(monster) = dead_target.world.monster_mut(monster) {
        monster.dead = true;
        // 保留一点血量：如果毒雾错误地打死亡目标，`hp` 会变，断言能抓住。
        monster.hp = 5_000;
        // 别让尸体在 550ms 内被清掉，否则断言无从下手。
        monster.dead_time = i64::MAX;
    }
    dead_target
        .cast(
            caster,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            Point::new(21, 20),
        )
        .expect("施法成功");
    dead_target.elapse(550);
    assert_eq!(dead_target.world.spell_fields().len(), 9);
    let monster = dead_target.world.monster(monster).expect("怪物对象还在");
    assert!(
        monster.poisons.is_empty(),
        "死亡目标被跳过（`SpellObject.cs:152-153` 先判 `ob.Dead`）"
    );
    assert_eq!(monster.hp, 5_000, "死亡目标不再吃伤害");
}

// ---------------------------------------------------------------------------
// 4. 附伤：BonusDmg = Random.Next(Stats[PoisonAttack])（`HumanObject.cs:4555`）
// ---------------------------------------------------------------------------

/// 复算一次毒雾施法在「伤害 + 附伤」上的 RNG 消耗，与 `magic.rs` 的效果臂同序：
/// `attack_power(MinSC, MaxSC)` → `magic.damage` → `next_range(0, PoisonAttack-1)`。
///
/// 返回 `(伤害, 附伤, 抽完后的 RNG)`；`poison_attack <= 0` 时附伤分支不抽签。
fn expected_draws(
    harness: &mut Harness,
    caster: u32,
    poison_attack: i32,
) -> (i32, i32, mir_core::MirRng) {
    let stats = harness.world.player(caster).expect("玩家").stats;
    let magic = *harness
        .world
        .player_magic(caster, Spell::PoisonCloud)
        .expect("已学毒雾");
    let info = harness
        .world
        .magic_info(Spell::PoisonCloud)
        .expect("技能模板")
        .clone();
    let mut probe = harness.rng_state();
    let base = stats.attack_power(stats.min_sc, stats.max_sc, &mut probe);
    let damage = magic.damage(&info, base, &mut probe);
    let bonus = if poison_attack > 0 {
        probe.next_range(0, poison_attack - 1)
    } else {
        0
    };
    (damage, bonus, probe)
}

/// 非零 PoisonAttack：装备模板 5 + `AddedStats` 3 = 8；再加增益 2 = 10。
/// 每次都与「按同一 RNG 复算的序列」逐字比对，并真实断言 0 附伤不耗 RNG。
#[test]
fn round52_poison_cloud_bonus_damage_from_poison_attack_and_rng() {
    // --- 第 1 次：根装备 5 + 附加属性 3 = 8 ---
    let mut harness = Harness::new(20);
    let caster = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    harness.learn(caster, Spell::PoisonCloud);
    harness.equip(caster, AMULET_SLOT, AMULET, 10);
    harness.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    harness.equip_added(caster, WEAPON_SLOT, POISON_WEAPON, 1, Stat::PoisonAttack, 3);
    harness.refresh_stats(caster);
    harness.ready_all(caster);
    let monster = harness.spawn(test_monster(100_000, 0), Point::new(21, 20));
    let sc = {
        let stats = harness.world.player(caster).expect("玩家").stats;
        (stats.min_sc + stats.max_sc) / 2
    };

    // 0 假设（不抽附伤）与 8 假设的 RNG 事后状态必须不同：证明附伤确实抽了一次。
    let (_, zero_bonus, probe_zero) = expected_draws(&mut harness, caster, 0);
    assert_eq!(zero_bonus, 0);
    let (expected_damage, expected_bonus, probe_eight) = expected_draws(&mut harness, caster, 8);
    assert_ne!(
        probe_eight, probe_zero,
        "PoisonAttack = 8 时附伤必须消耗一次 RNG"
    );
    harness.drain(caster);
    let report = harness
        .cast(
            caster,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            Point::new(21, 20),
        )
        .expect("施法成功");
    assert_eq!(report.damage, Some(expected_damage), "伤害按同一 RNG 复算");
    assert_eq!(
        harness.rng_state(),
        probe_eight,
        "施法后的 RNG 状态必须与「PoisonAttack = 8 的复算序列」完全一致"
    );
    harness.elapse(550);
    let fields: Vec<_> = harness.world.spell_fields().to_vec();
    assert_eq!(fields.len(), 9);
    assert!(
        fields
            .iter()
            .all(|field| field.bonus_damage == expected_bonus),
        "BonusDmg = Random.Next(8) 的复算值 {expected_bonus}"
    );
    let poison = harness.world.monster(monster).expect("怪").poisons[0];
    assert_eq!(
        poison.value,
        sc + expected_bonus,
        "毒量 = (MinSC + MaxSC) / 2 + BonusDmg"
    );

    // --- 第 2 次：再加一个 PoisonAttack = 2 的生效增益 → 总数 10 ---
    harness.ready_all(caster);
    harness.equip(caster, AMULET_SLOT, AMULET, 5);
    harness.equip(caster, POISON_SLOT, GREEN_POISON, 5);
    let mut buff_stats = Stats::new();
    buff_stats.set(Stat::PoisonAttack, 2);
    harness.give_buff(caster, buff_stats);
    let (_, bonus_eight, _) = expected_draws(&mut harness, caster, 8);
    let (expected_damage_two, expected_bonus_ten, probe_ten) =
        expected_draws(&mut harness, caster, 10);
    assert_ne!(
        expected_bonus_ten, bonus_eight,
        "该种子下 8 与 10 的抽取结果不同，断言才能区分聚合值"
    );
    let other = Point::new(18, 20);
    let report = harness
        .cast(caster, Spell::PoisonCloud, MirDirection::Down, 0, other)
        .expect("施法成功");
    assert_eq!(report.damage, Some(expected_damage_two));
    assert_eq!(
        harness.rng_state(),
        probe_ten,
        "施法后的 RNG 状态必须与「PoisonAttack = 10（5 根 + 3 附加 + 2 增益）」一致"
    );
    harness.elapse(600);
    let field = harness
        .world
        .spell_fields()
        .iter()
        .find(|field| field.center == other)
        .expect("第二次毒雾落在新位置");
    assert_eq!(field.bonus_damage, expected_bonus_ten);

    // --- 0 附伤：没有 PoisonAttack 时完全不多耗 RNG，BonusDmg 恒 0 ---
    let mut plain = Harness::new(22);
    let caster = plain.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    plain.learn(caster, Spell::PoisonCloud);
    plain.equip(caster, AMULET_SLOT, AMULET, 10);
    plain.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    plain.ready_all(caster);
    let (expected_damage_plain, zero_bonus, probe_plain) = expected_draws(&mut plain, caster, 0);
    assert_eq!(zero_bonus, 0);
    let report = plain
        .cast(
            caster,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            Point::new(21, 20),
        )
        .expect("施法成功");
    assert_eq!(report.damage, Some(expected_damage_plain));
    assert_eq!(
        plain.rng_state(),
        probe_plain,
        "PoisonAttack = 0 时附伤不抽签：RNG 状态必须与只抽伤害时完全一致"
    );
    plain.elapse(550);
    assert!(
        plain
            .world
            .spell_fields()
            .iter()
            .all(|field| field.bonus_damage == 0),
        "没有 PoisonAttack 装备/增益 → BonusDmg = 0"
    );

    // --- 失效来源 1：0 耐久装备的 PoisonAttack（模板 + AddedStats）不贡献 ---
    // （`items.rs:912-922` 的 `current_dura == 0 && info.durability > 0` 跳过，
    // `caster_poison_attack` 与 `caster_freezing` 同口径。）
    let mut broken = Harness::new(23);
    let caster = broken.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    broken.learn(caster, Spell::PoisonCloud);
    broken.equip(caster, AMULET_SLOT, AMULET, 10);
    broken.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    {
        let info = broken.world.item_info(POISON_WEAPON).expect("模板").clone();
        let mut item = broken.world.create_fresh_item(&info);
        item.count = 1;
        item.current_dura = 0;
        item.max_dura = 0;
        item.added_stats.set(Stat::PoisonAttack, 3);
        broken.world.player_mut(caster).expect("玩家").equipment[WEAPON_SLOT] = Some(item);
    }
    // 有效 PoisonAttack = 0 → 与「没有来源」同一条 RNG 序列，附伤 0。
    let (expected_damage_broken, zero_bonus, probe_broken) = expected_draws(&mut broken, caster, 0);
    assert_eq!(zero_bonus, 0);
    let report = broken
        .cast(
            caster,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            Point::new(21, 20),
        )
        .expect("施法成功");
    assert_eq!(report.damage, Some(expected_damage_broken));
    assert_eq!(
        broken.rng_state(),
        probe_broken,
        "0 耐久装备即使模板/附加带 PoisonAttack 也不得贡献（否则会多抽一次附伤 RNG）"
    );
    broken.elapse(550);
    assert!(
        broken
            .world
            .spell_fields()
            .iter()
            .all(|field| field.bonus_damage == 0),
        "0 耐久装备的 PoisonAttack 不计入 BonusDmg"
    );

    // --- 失效来源 2：生效 buff 贡献；到期移除后回 0 且不抽附伤 RNG ---
    // （`buffs.rs:454-474` 在 `now >= expire_at` 时移除，Crystal `HumanObject.cs:2332`。）
    let mut buffed = Harness::new(24);
    let caster = buffed.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    buffed.learn(caster, Spell::PoisonCloud);
    buffed.equip(caster, AMULET_SLOT, AMULET, 10);
    buffed.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    let mut buff_stats = Stats::new();
    buff_stats.set(Stat::PoisonAttack, 2);
    let expire_at = buffed.world.now() + 100;
    buffed.give_buff_until(caster, buff_stats, expire_at);
    buffed.ready_all(caster);
    let (expected_damage_buff, expected_bonus_two, probe_two) =
        expected_draws(&mut buffed, caster, 2);
    let report = buffed
        .cast(
            caster,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            Point::new(21, 20),
        )
        .expect("施法成功");
    assert_eq!(report.damage, Some(expected_damage_buff));
    assert_eq!(
        buffed.rng_state(),
        probe_two,
        "生效 buff 的 PoisonAttack 必须计入抽取"
    );
    buffed.elapse(600);
    assert_eq!(
        buffed.world.player(caster).expect("玩家").buffs.len(),
        0,
        "短时增益应已到期移除（`buffs.rs:454-474`）"
    );
    assert_eq!(
        buffed.world.spell_fields()[0].bonus_damage,
        expected_bonus_two,
        "buff 生效时抽到的 BonusDmg 落在场上"
    );
    // 重新备料、清冷却后再施一次：PoisonAttack 已回 0，不得多抽附伤 RNG，BonusDmg 回 0。
    buffed.ready_all(caster);
    buffed.equip(caster, AMULET_SLOT, AMULET, 5);
    buffed.equip(caster, POISON_SLOT, GREEN_POISON, 5);
    let (expected_damage_after, zero_bonus, probe_after) = expected_draws(&mut buffed, caster, 0);
    assert_eq!(zero_bonus, 0);
    let other = Point::new(18, 20);
    let report = buffed
        .cast(caster, Spell::PoisonCloud, MirDirection::Down, 0, other)
        .expect("施法成功");
    assert_eq!(report.damage, Some(expected_damage_after));
    assert_eq!(
        buffed.rng_state(),
        probe_after,
        "buff 到期后 PoisonAttack 回 0：附伤不抽签，RNG 必须与只抽伤害一致"
    );
    buffed.elapse(600);
    let second = buffed
        .world
        .spell_fields()
        .iter()
        .find(|field| field.center == other)
        .expect("第二次毒雾落地");
    assert_eq!(second.bonus_damage, 0, "buff 到期后 BonusDmg 回 0");
}

// ---------------------------------------------------------------------------
// 5. 有效地形格：Crystal `cell.Valid => Attribute == CellAttribute.Walk`
//    （`Map.cs:2480-2482`，`Map.cs:1455` 跳过不可走格）
// ---------------------------------------------------------------------------

#[test]
fn round52_poison_cloud_valid_terrain_and_first_show() {
    // --- 墙格：扫描顺序是 y 外层 + x 内层（`Map.cs:1455`），先遇到两格墙 ---
    let mut harness = Harness::new(30);
    let caster = harness.add_player(1, MirClass::Wizard, 30, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 30, Point::new(20, 26));
    harness.learn(caster, Spell::PoisonCloud);
    harness.equip(caster, AMULET_SLOT, AMULET, 10);
    harness.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    harness.ready_all(caster);
    harness.drain(observer);
    // 落点 (22,20) 的 3×3 扫描顺序：先 y=19 行的 (21,19)、(22,19)、(23,19)，再 y=20 行……
    harness.insert_walls(&[Point::new(21, 19), Point::new(22, 19)]);
    assert!(
        !harness
            .world
            .map_of(MAP)
            .expect("地图已插入")
            .walkable(21, 19),
        "合成地图的墙格必须不可走（否则用例没意义）"
    );
    harness
        .cast(
            caster,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            Point::new(22, 20),
        )
        .expect("施法成功");
    harness.elapse(600);
    let fields: Vec<_> = harness.world.spell_fields().to_vec();
    assert_eq!(fields.len(), 7, "9 格里有 2 格是墙 → 只生成 7 格");
    for field in &fields {
        assert!(
            field.center != Point::new(21, 19) && field.center != Point::new(22, 19),
            "墙格不得生成毒雾：{:?}",
            field.center
        );
    }
    // 首个实际生成的对象仍必须是扫描顺序里的第一格有效格 → `Show = true` 且只广播一条。
    let shown: Vec<_> = fields.iter().filter(|field| field.show).collect();
    assert_eq!(shown.len(), 1, "只有首个实际生成的对象 Show = true");
    assert_eq!(
        shown[0].center,
        Point::new(23, 19),
        "首个有效格是扫描到的第一格"
    );
    let spell_packets = object_spells(&harness.packets(observer));
    assert_eq!(spell_packets.len(), 1, "墙格被跳过也不影响 Show 只广播一次");
    assert_eq!(
        spell_packets[0].0, shown[0].object_id,
        "广播的 S.ObjectSpell 属于那个唯一 Show 的对象"
    );

    // --- 地图角落：落点 (0,0) 只有 4 格在界内，且没有越界广播 ---
    let mut corner = Harness::new(31);
    let caster = corner.add_player(1, MirClass::Wizard, 30, Point::new(0, 0));
    let observer = corner.add_player(2, MirClass::Wizard, 30, Point::new(3, 3));
    corner.learn(caster, Spell::PoisonCloud);
    corner.equip(caster, AMULET_SLOT, AMULET, 10);
    corner.equip(caster, POISON_SLOT, GREEN_POISON, 10);
    corner.ready_all(caster);
    corner.drain(observer);
    corner
        .cast(
            caster,
            Spell::PoisonCloud,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("角落施法成功");
    corner.elapse(500);
    let fields: Vec<_> = corner.world.spell_fields().to_vec();
    assert_eq!(fields.len(), 4, "地图角落只有 4 格在界内");
    let mut centers: Vec<(i32, i32)> = fields
        .iter()
        .map(|field| (field.center.x, field.center.y))
        .collect();
    centers.sort_unstable();
    assert_eq!(centers, vec![(0, 0), (0, 1), (1, 0), (1, 1)]);
    assert!(
        fields.iter().filter(|field| field.show).count() == 1,
        "角落也只有一个 Show"
    );
    assert_eq!(
        object_spells(&corner.packets(observer)).len(),
        1,
        "角落也只广播一条 S.ObjectSpell"
    );
}
