//! R52-B3：困魔咒（`Spell.TrapHexagon`）端到端测试。
//!
//! 全部断言对照 Crystal（参考根目录 `github\Crystal`）：
//!
//! | 行为 | Crystal |
//! |---|---|
//! | 预检范围 **3×2**（`x-1..=x+1`、`y-1..=y`），格必须可走；`IsAttackTarget` + `Level <= caster.Level+2` | `HumanObject.cs:4596-4629` |
//! | 无目标直接 `return`（`cast=false`，不查耗材） | `HumanObject.cs:4621-4629` |
//! | 耗材：护身符 1；`GetAmulet(1)` 为空 → `cast=false`，公共副作用（MP/ActionTime/SpellTime）已发生 | `HumanObject.cs:4630-4642` |
//! | 施法成功即 `LevelMagic`；延迟 500ms 落地；`Duration = (Level*5+10)*1000` | `HumanObject.cs:4631-4638` |
//! | 落地重检 **3×3**，中心格怪无条件当中心、其余怪需可攻击且等级 ≤ +2；`ShockTime = now+Duration; Target = null` | `Map.cs:1752-1796` |
//! | 无中心怪 → 不生成场；八外围场 = `(±1,∓2)/(±2,±1)`，场绑定中心怪 | `Map.cs:1797-1829` |
//! | 场到期/中心怪 `ShockTime == 0` 时撤销；施法者下线不撤（本作照抄） | `SpellObject.cs:51-70` |
//! | Shock 只否决 `CanMove`（不进 `CanAttack`）；够不到目标 → 清目标；攻击入口无条件 `ShockTime = 0` | `MonsterObject.cs:604`、`:642`、`:650-665`、`:1858-1862`、`:2150-2155` |
//! | 受击只有**有效伤害**才清 Shock；护甲吃满（`armour >= damage`）直接 `return 0` 不清 | `MonsterObject.cs:2569-2625` |
//! | `S.ObjectMonster.ShockTime` = 剩余毫秒 | `MonsterObject.cs:2874` |
//! | 外围场越界：Crystal 原始判断会跳过 `x=0` 且允许 `x == Width`；本作按合法地图坐标安全剪裁（D2 已批准） | `Map.cs:1810-1813` |
//!
//! 返工（review BLOCK R1–R4）追加的断言：
//! - **R1** 真实专属攻击入口：守卫真实施法拒收（`Guard.cs:54-57` 的 `IsAttackTarget => false`）、
//!   可攻击宠物的独立 `pet_target` 清理与贴身攻击复现、`AI55` `HumanWizard` 魔法入口
//!   （`HumanWizard.cs:26-55`）、`AI13` `RedMoonEvil` 特殊 AOE 入口（`RedMoonEvil.cs:31-52`）、
//!   `AI12` `BugBagMaggot` 召唤入口（`BugBagMaggot.cs:30-53`）清 Shock。
//! - **R2** 视野：`Show` 对象按 `CastLocation` 补发、视野内移动不重复、进入/离开/回收三态一致
//!   （`SpellObject.cs:490-530`），Trap 与 `PoisonCloud` 各覆盖。
//! - **R3** 材料失败矩阵 6 例 + 0 耐久材料；目标死亡/移位/换图；纯 +3 拒绝；右下边界；毒雾 `Show` 语义。
//! - **R4** `refused_cast_report` 改结构体参数后不再需要 `#[allow(clippy::too_many_arguments)]`。
//!
//! 所有用例确定性：固定 RNG 种子、`with_data` 里先 `advance_clock(0)` 冻结时钟、
//! 怪物 `operate_time = -1`（训练木桩 AI 56 不思考；普通怪 AI 1 用于真实 AI 断言）。

mod common;

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{
    AttackMode, ItemType, MirClass, MirDirection, MirGender, Monster as MonsterImage, PetMode,
    Spell,
};
use mir_db::{MagicInfo, MapInfo, MirDb};
use mir_protocol::ServerPacket;
use mir_protocol::data::ItemInfo;
use mir_server::MonsterKind;
use mir_server::combat::{self, CombatStats};
use mir_server::gamedata::GameData;
use mir_server::magic::{CastReport, MAGIC_CAST_ACTION_MS, MAGIC_SPELL_TIME_MS, MagicError};
use mir_server::world::{Player, World};
use tokio::sync::mpsc;

const MAP: &str = "0";
const MAP_INDEX: i32 = 990;

/// 护身符模板（`GetAmulet(1)` 认 `ItemType.Amulet` + 形状 0）。
const AMULET: i32 = 201;
/// 装备槽：B1 单测用 6，沿用。
const AMULET_SLOT: usize = 6;
/// 形状 1 的护身符：不是困魔咒要的形状 0，用来验证「错形状」不算材料。
const WRONG_SHAPE_AMULET: i32 = 202;
/// 药水（`ItemType.Potion`）：验证「错类型」不算护身符。
const POTION: i32 = 203;
/// 绿毒（B2 毒雾耗材）：R2 里对毒雾 `Show` 语义做视野补发回归。
const GREEN_POISON: i32 = 204;
/// 毒雾耗材装备槽。
const POISON_SLOT: usize = 9;

/// 困魔咒落地延迟（`HumanObject.cs:4637` 的 `Envir.Time + 500`）。
const FIELD_DELAY_MS: i64 = 500;

/// 3×3 落地重检后八外围场相对落点的坐标（`Map.cs:1797-1829` 的展开）。
const FIELD_OFFSETS: [(i32, i32); 8] = [
    (1, -2),
    (-1, -2),
    (2, -1),
    (2, 1),
    (1, 2),
    (-1, 2),
    (-2, -1),
    (-2, 1),
];

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
        // 先冻结时钟：`World::now` 在首次 `advance_clock` 前取真实墙钟，若等到第一次
        // `elapse` 才冻结，`cast_magic` 与 `elapse(499)` 之间真实耗时就可能把落点提前
        // 推进 1ms 以上（评审 B2-review 用 exit 101 复现过；同 `world.rs:6444-6455`）。
        world.advance_clock(0);
        world.seed_rng(seed);
        world.set_game_data(game_data());
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
        self.equip_durability(object_id, slot, index, count, 100);
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

    /// 背包快照：材料失败矩阵要证明「一件都不动」（含只在背包的子例）。
    fn inventory_snapshot(&self, object_id: u32) -> Vec<(u64, i32, u16)> {
        let player = self.world.player(object_id).expect("玩家还在");
        player
            .inventory
            .iter()
            .flatten()
            .map(|item| (item.unique_id, item.item_index, item.count))
            .collect()
    }

    /// 往背包里放 `count` 个物品（`find_spell_material` 只看装备，不搜背包）。
    fn backpack(&mut self, object_id: u32, index: i32, count: u16) {
        let info = self.world.item_info(index).expect("物品模板").clone();
        let mut item = self.world.create_fresh_item(&info);
        item.count = count;
        assert!(
            self.world.add_item_to_inventory(object_id, item),
            "背包应有空位"
        );
    }

    /// 直接改怪物字段（AI/Shock/目标类断言用）。
    fn monster_edit(&mut self, object_id: u32, edit: impl FnOnce(&mut mir_server::Monster)) {
        edit(self.world.monster_mut(object_id).expect("怪还在"));
    }

    fn spawn(&mut self, kind: MonsterKind, position: Point) -> u32 {
        let object_id = self.world.spawn_monster(MAP, kind, position);
        if let Some(monster) = self.world.monster_mut(object_id) {
            // 默认 `operate_time = -1` 让每个世界 tick 都调度它（Crystal 对象轮转）；
            // 需要冻结 AI 时测试自己改成 `i64::MAX`。
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

    /// 直接改技能等级（Crystal `UserMagic.Level`，`MagicInfo.cs:88-101`）；
    /// 新学技能默认 0 级（`UserMagic::new`），困魔咒持续时间依赖它。
    fn set_magic_level(&mut self, object_id: u32, spell: Spell, level: u8) {
        let player = self.world.player_mut(object_id).expect("玩家还在");
        let magic = player
            .magics
            .iter_mut()
            .find(|magic| magic.spell == spell)
            .expect("已学技能");
        magic.level = level;
    }

    fn magic_level(&self, object_id: u32) -> u8 {
        self.world
            .player(object_id)
            .expect("玩家还在")
            .magics
            .iter()
            .find(|magic| magic.spell == Spell::TrapHexagon)
            .expect("已学困魔咒")
            .level
    }

    fn set_position(&mut self, object_id: u32, position: Point) {
        self.world.player_mut(object_id).expect("玩家还在").position = position;
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
        direction: MirDirection,
        location: Point,
    ) -> Result<CastReport, MagicError> {
        self.world
            .cast_magic(object_id, Spell::TrapHexagon, direction, 0, location)
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
}

// ---------------------------------------------------------------------------
// 技能模板 / 测试怪 / 包辅助
// ---------------------------------------------------------------------------

/// 困魔咒 `MagicInfo`：耗蓝 5、无冷却（重复施法用例用 `ready_all` 清，但延迟取 0 便于观察）。
fn trap_hexagon_info() -> MagicInfo {
    MagicInfo {
        name: "困魔咒".to_owned(),
        spell: Spell::TrapHexagon.value(),
        base_cost: 5,
        level_cost: 2,
        icon: 10,
        level1: 1,
        level2: 2,
        level3: 3,
        need1: 10,
        need2: 20,
        need3: 30,
        delay_base: 0,
        delay_reduction: 0,
        power_base: 0,
        power_bonus: 0,
        m_power_base: 0,
        m_power_bonus: 0,
        // `HumanObject.cs:3416` 的射程预检：落点必须 <= 8 格。
        range: 8,
        multiplier_base: 1.0,
        multiplier_bonus: 0.0,
    }
}

/// 火墙 `MagicInfo`：F1 反例用——火墙走 B2 的逐格直接广播，不在 `field_views`
/// 记账链里，所以「生成后视野内移动」与「回收/替换后移动」都不能重复补发 149/26。
fn fire_wall_info() -> MagicInfo {
    MagicInfo {
        name: "火墙".to_owned(),
        spell: Spell::FireWall.value(),
        base_cost: 5,
        level_cost: 2,
        icon: 10,
        level1: 1,
        level2: 2,
        level3: 3,
        need1: 10,
        need2: 20,
        need3: 30,
        delay_base: 0,
        delay_reduction: 0,
        power_base: 4,
        power_bonus: 0,
        m_power_base: 0,
        m_power_bonus: 0,
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

fn game_data() -> Arc<GameData> {
    let db = MirDb {
        magics: vec![
            trap_hexagon_info(),
            thunder_bolt_info(),
            poison_cloud_info(),
            fire_wall_info(),
        ],
        items: vec![
            amulet(AMULET, 0, "护身符"),
            amulet(WRONG_SHAPE_AMULET, 1, "形状 1 的护身符"),
            amulet(GREEN_POISON, 1, "绿毒"),
            ItemInfo {
                index: POTION,
                name: "金创药(小量)".to_owned(),
                item_type: ItemType::Potion,
                weight: 1,
                ..ItemInfo::default()
            },
        ],
        maps: vec![MapInfo {
            index: MAP_INDEX,
            file_name: MAP.to_owned(),
            title: "困魔咒测试场".to_owned(),
            // F1 火墙反例：上限 1 次施法实例，第二次施法会走「回收最老一次」替换路径。
            firewall_limit: true,
            firewall_count: 1,
            ..MapInfo::default()
        }],
        ..MirDb::default()
    };
    Arc::new(GameData::from_db(db, None))
}

/// 怪物魔法模板（`AI55` `HumanWizard` 用 `ThunderBolt`，`monster.rs:753`）。
fn thunder_bolt_info() -> MagicInfo {
    MagicInfo {
        name: "雷电术".to_owned(),
        spell: Spell::ThunderBolt.value(),
        base_cost: 5,
        level_cost: 2,
        icon: 10,
        level1: 1,
        level2: 2,
        level3: 3,
        need1: 10,
        need2: 20,
        need3: 30,
        delay_base: 0,
        delay_reduction: 0,
        power_base: 4,
        power_bonus: 0,
        m_power_base: 10,
        m_power_bonus: 0,
        range: 6,
        multiplier_base: 1.0,
        multiplier_bonus: 0.0,
    }
}

/// B2 毒雾模板（R2 视野补发回归用；与 `tests/round52_poison_cloud.rs` 同口径）。
fn poison_cloud_info() -> MagicInfo {
    MagicInfo {
        name: "毒雾".to_owned(),
        spell: Spell::PoisonCloud.value(),
        base_cost: 5,
        level_cost: 2,
        icon: 10,
        level1: 1,
        level2: 2,
        level3: 3,
        need1: 10,
        need2: 20,
        need3: 30,
        delay_base: 12_000,
        delay_reduction: 100,
        power_base: 4,
        power_bonus: 0,
        m_power_base: 10,
        m_power_bonus: 0,
        range: 8,
        multiplier_base: 1.0,
        multiplier_bonus: 0.0,
    }
}

/// 测试怪：`ai` 决定 AI 行为（56 = 训练木桩不思考；1 = 普通野生怪走真实 AI）。
fn monster_kind(level: u16, hp: i32, min_mac: i32, ai: u8) -> MonsterKind {
    MonsterKind {
        name: "困魔咒测试怪".to_owned(),
        image: MonsterImage::Oma,
        ai,
        level,
        view_range: 7,
        cool_eye: 0,
        attack_speed_ms: 2_500,
        move_speed_ms: 1_800,
        experience: 0,
        can_push: true,
        stats: CombatStats {
            hp,
            min_dc: 5,
            max_dc: 5,
            min_mac,
            max_mac: min_mac,
            ..CombatStats::default()
        },
    }
}

fn trainer(level: u16, hp: i32) -> MonsterKind {
    monster_kind(level, hp, 0, 56)
}

fn field_centers(world: &World) -> BTreeSet<(i32, i32)> {
    world
        .spell_fields()
        .iter()
        .map(|field| (field.center.x, field.center.y))
        .collect()
}

fn field_ids(world: &World) -> BTreeSet<u32> {
    world
        .spell_fields()
        .iter()
        .map(|field| field.object_id)
        .collect()
}

fn expected_centers(location: Point) -> BTreeSet<(i32, i32)> {
    FIELD_OFFSETS
        .iter()
        .map(|(dx, dy)| (location.x + dx, location.y + dy))
        .collect()
}

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

/// 收 `S.ObjectMagic` 的 `(spell, cast)`：目标/材料失败时观察者也要看到 `cast = false`。
fn object_magic_casts(packets: &[ServerPacket]) -> Vec<(Spell, bool)> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectMagic(packet) => Some((packet.spell, packet.cast)),
            _ => None,
        })
        .collect()
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

/// 收 `S.ObjectMonster` 的 `(object_id, shock_time 剩余毫秒)`。
fn object_monsters(packets: &[ServerPacket]) -> Vec<(u32, i64)> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectMonster(packet) => Some((packet.object_id, packet.shock_time)),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. 预检 3×2 / 落地重检 3×3 / 八场生成与绑定 / 入视野同步
// ---------------------------------------------------------------------------

#[test]
fn round52_trap_hexagon_precheck_3x2_resolve_3x3() {
    let mut harness = Harness::new(7);
    let player = harness.add_player(1, MirClass::Wizard, 10, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 10, Point::new(20, 26));
    harness.learn(player, Spell::TrapHexagon);
    harness.equip(player, AMULET_SLOT, AMULET, 3);
    harness.ready_all(player);
    harness.drain(player);
    harness.drain(observer);

    // --- A) 目标只在 y+1（预检 3×2 之外）→ cast=false：不耗材、不排效果、不写 CastTime ---
    let below = harness.spawn(trainer(10, 500), Point::new(20, 21));
    let mp_before = harness.world.player(player).expect("玩家").mp;
    let now = harness.world.now();
    let report = harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("目标不足走 cast=false，但 cast_magic 仍返回 Ok");
    assert_eq!(report.spell, Spell::TrapHexagon);
    assert!(report.damage.is_none(), "困魔咒没有直接伤害");
    assert!(
        harness.world.player(player).expect("玩家").mp < mp_before,
        "公共检查已扣 MP（D2：材料/目标失败不回滚）"
    );
    assert!(
        harness.world.player(player).expect("玩家").action_time > now,
        "动作时间已锁（保留公共副作用）"
    );
    assert_eq!(
        harness.equipment(player, AMULET_SLOT),
        Some((AMULET, 3)),
        "预检不命中 → 不扣护身符"
    );
    assert!(
        harness.world.pending_magic().is_empty(),
        "cast=false 不排落地效果"
    );
    assert!(
        harness.world.spell_fields().is_empty(),
        "cast=false 不生成场"
    );
    assert!(
        !harness
            .world
            .player(player)
            .expect("玩家")
            .magic
            .casts
            .contains_key(&Spell::TrapHexagon),
        "cast=false 不写本技能 CastTime"
    );
    assert!(
        object_magic_casts(&harness.packets(observer)).contains(&(Spell::TrapHexagon, false)),
        "观察者也看到 cast=false"
    );

    // --- B) 预检内目标（x-1, y-1）→ 成功；500ms 后落地 3×3，中心怪被 Shock + 八外围场 ---
    let inside = harness.spawn(trainer(10, 500), Point::new(19, 19));
    harness.ready_all(player);
    harness.drain(observer);
    let cast_now = harness.world.now();
    harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("预检命中应施法成功");
    assert_eq!(
        harness.equipment(player, AMULET_SLOT),
        Some((AMULET, 2)),
        "护身符扣 1（`GetAmulet(1)`）"
    );
    assert_eq!(
        harness.world.pending_magic().len(),
        1,
        "500ms 后落地：效果在延迟队列"
    );
    assert!(
        harness
            .world
            .player(player)
            .expect("玩家")
            .magic
            .casts
            .contains_key(&Spell::TrapHexagon),
        "cast=true 才写本技能 CastTime"
    );
    assert!(
        harness.world.spell_fields().is_empty(),
        "尚未到 500ms，不生成场"
    );
    harness.elapse(FIELD_DELAY_MS - 1);
    assert!(harness.world.spell_fields().is_empty(), "499ms 时仍未落地");
    assert_eq!(
        harness.world.monster(below).expect("怪还在").shock_time,
        0,
        "499ms 时尚未 Shock"
    );
    harness.elapse(1);
    let land_now = harness.world.now();
    assert_eq!(land_now, cast_now + FIELD_DELAY_MS, "落地延迟恰好 500ms");
    assert!(
        harness.world.pending_magic().is_empty(),
        "落地消耗掉延迟动作"
    );
    assert_eq!(
        harness.world.spell_fields().len(),
        8,
        "八外围场（`Map.cs:1797-1829`）"
    );
    let duration = (i64::from(harness.magic_level(player)) * 5 + 10) * 1_000;
    assert_eq!(
        harness.world.spell_fields()[0].expire_at,
        land_now + duration,
        "持续时间 = (Level*5+10)*1000"
    );
    assert_eq!(
        harness.world.monster(inside).expect("怪还在").shock_time,
        land_now + duration,
        "落地即给中心怪写 Shock"
    );
    assert_eq!(
        harness.world.monster(below).expect("怪还在").shock_time,
        land_now + duration,
        "落地 3×3 重检连 y+1 的怪一起 Shock"
    );
    let centers = expected_centers(Point::new(20, 20));
    assert_eq!(field_centers(&harness.world), centers, "八场坐标");
    assert!(
        harness
            .world
            .spell_fields()
            .iter()
            .all(|field| field.target_monster == inside
                && field.spell == Spell::TrapHexagon
                && field.caster_id == player),
        "八场全部绑定中心怪"
    );

    // --- R2：视野差分。观察者创建时就在视野内 → 生成广播恰好一次；移动不重复补发 ---
    let spells = object_spells(&harness.packets(observer));
    assert_eq!(
        spells.len(),
        8,
        "生成广播：视野内观察者收到八条 ObjectSpell"
    );
    assert!(
        spells
            .iter()
            .all(|(_, _, spell)| *spell == Spell::TrapHexagon),
        "生成广播的场类型都是困魔咒"
    );
    assert_eq!(
        spells
            .iter()
            .map(|(_, location, _)| (location.x, location.y))
            .collect::<BTreeSet<_>>(),
        centers,
        "生成广播的场坐标（每格自己的位置，`SpellObject.cs:553-560`）"
    );
    // 仍在视野内移动：`field_views` 已登记 → 不再补发（旧实现会重复发 8 条）。
    harness.ready_all(observer);
    harness
        .world
        .walk(observer, MirDirection::Down)
        .expect("观察者可移动");
    assert!(
        object_spells(&harness.packets(observer)).is_empty(),
        "视野内移动不重复补发 ObjectSpell"
    );
    // 走出视野：8 条 S.ObjectRemove（INDEX 26）。
    harness.ready_all(observer);
    harness.set_position(observer, Point::new(20, 40));
    harness
        .world
        .walk(observer, MirDirection::Down)
        .expect("观察者可移动");
    let field_id_set = field_ids(&harness.world);
    let removed: Vec<u32> = object_remove_ids(&harness.packets(observer))
        .into_iter()
        .filter(|object_id| field_id_set.contains(object_id))
        .collect();
    // 多重集断言：每个场 ID 必须**恰好一次**。旧的 `BTreeSet` 写法会把重复撤销折叠掉，
    // 漏掉「离开视野时同一对象被撤销两次」的回归。
    let mut removed_sorted = removed;
    removed_sorted.sort_unstable();
    let mut expected_ids: Vec<u32> = field_id_set.iter().copied().collect();
    expected_ids.sort_unstable();
    assert_eq!(
        removed_sorted, expected_ids,
        "离开视野撤销八条场对象（同批 ObjectRemove 里还有离开视野的怪，按场 ID 过滤），每个 ID 恰好一次"
    );
    // 再进入视野：补发一次。
    harness.ready_all(observer);
    harness.set_position(observer, Point::new(20, 30));
    harness
        .world
        .walk(observer, MirDirection::Up)
        .expect("观察者可移动");
    let reentered = object_spells(&harness.packets(observer));
    assert_eq!(reentered.len(), 8, "重新进入视野补发一次八条 ObjectSpell");
    assert_eq!(
        reentered
            .iter()
            .map(|(_, location, _)| (location.x, location.y))
            .collect::<BTreeSet<_>>(),
        centers,
        "补发坐标"
    );

    // --- C) 落地重检扫 3×3 第三行：预检命中后目标死亡 → 落地改选 y+1 的怪 ---
    harness.world.monster_mut(inside).expect("怪还在").dead = true;
    // 施法时需要一个活着的预检目标（(x+1, y-1) 在 3×2 内）。
    let bait = harness.spawn(trainer(10, 500), Point::new(21, 19));
    harness.ready_all(player);
    harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("预检命中应施法成功");
    // 预检已过：把它也弄死，只剩 y+1 的 below 能在 3×3 里当中心。
    harness.world.monster_mut(bait).expect("怪还在").dead = true;
    harness.elapse(FIELD_DELAY_MS);
    let land2 = harness.world.now();
    assert_eq!(
        harness.world.spell_fields().len(),
        8,
        "旧八场（中心怪已死）被提前撤销，新八场重新生成"
    );
    assert!(
        harness
            .world
            .spell_fields()
            .iter()
            .all(|field| field.target_monster == below),
        "落地 3×3 重检选中 y+1 的怪（若不重检就会选中死怪/无场）"
    );
    assert_eq!(
        harness.world.monster(below).expect("怪还在").shock_time,
        land2 + duration,
        "重检后的中心怪被 Shock"
    );

    let _ = player;
}

// ---------------------------------------------------------------------------
// 2. Shock：限制移动 / 远距清目标 / 攻击入口解除 / 有效受击解除 / 护甲早退不解除 / 八场回收
// ---------------------------------------------------------------------------

#[test]
fn round52_trap_hexagon_shock_ai_release_and_eight_fields() {
    let mut harness = Harness::new(11);
    let player = harness.add_player(1, MirClass::Wizard, 10, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 10, Point::new(20, 26));
    harness.learn(player, Spell::TrapHexagon);
    harness.equip(player, AMULET_SLOT, AMULET, 4);
    harness.ready_all(player);

    // 普通 AI（非树/训练木桩）：真实移动与目标 AI 入口。
    let mob = harness.spawn(monster_kind(10, 500, 0, 1), Point::new(20, 19));
    let cast_now = harness.world.now();
    harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("施法成功");
    harness.elapse(FIELD_DELAY_MS);
    let land_now = harness.world.now();
    assert_eq!(land_now, cast_now + FIELD_DELAY_MS);
    let duration = (i64::from(harness.magic_level(player)) * 5 + 10) * 1_000;
    assert_eq!(
        harness.world.monster(mob).expect("怪还在").shock_time,
        land_now + duration
    );
    assert!(
        !harness
            .world
            .monster(mob)
            .expect("怪还在")
            .can_move(land_now),
        "Shock 限制移动（`MonsterObject.cs:642`）"
    );
    assert!(
        harness
            .world
            .monster(mob)
            .expect("怪还在")
            .poisons
            .is_empty(),
        "Shock 是独立世界时钟状态，不复用 Frozen 毒"
    );

    // 玩家不被拘束：施法者照常走。
    harness.ready_all(player);
    harness
        .world
        .walk(player, MirDirection::Down)
        .expect("困魔咒不拘束玩家移动");

    // 真实移动入口连续 5 个 tick：Shock 中位置不变。
    harness.set_position(player, Point::new(20, 24));
    for _ in 0..5 {
        {
            let monster = harness.world.monster_mut(mob).expect("怪还在");
            monster.operate_time = -1;
            monster.action_time = 0;
            monster.move_time = 0;
        }
        harness.elapse(100);
    }
    assert_eq!(
        harness.world.monster(mob).expect("怪还在").position,
        Point::new(20, 19),
        "Shock 期间 AI 不移动"
    );
    assert!(
        harness.world.monster(mob).expect("怪还在").shock_time > harness.world.now(),
        "移动受限期间 Shock 未过期"
    );

    // 远距清目标：AI 够不到目标 → `Target = null`（`MonsterObject.cs:1858-1862`）。
    {
        let monster = harness.world.monster_mut(mob).expect("怪还在");
        monster.target = Some(player);
        monster.operate_time = -1;
        monster.action_time = 0;
        monster.attack_time = 0;
        monster.search_time = 0;
        monster.roam_time = 0;
    }
    harness.elapse(100);
    assert_eq!(
        harness.world.monster(mob).expect("怪还在").target,
        None,
        "Shock 中够不到目标 → 清目标原地不动"
    );
    assert!(
        harness.world.monster(mob).expect("怪还在").shock_time > harness.world.now(),
        "清目标不解除 Shock"
    );

    // 贴身照常攻击：Shock **不进** CanAttack，攻击入口无条件 `ShockTime = 0`。
    harness.set_position(player, Point::new(20, 20));
    harness.ready_all(player);
    let shock_until = harness.world.now() + 5_000;
    {
        let monster = harness.world.monster_mut(mob).expect("怪还在");
        monster.target = Some(player);
        monster.operate_time = -1;
        monster.action_time = 0;
        monster.attack_time = 0;
        monster.search_time = 0;
        monster.roam_time = 0;
        monster.shock_time = shock_until;
    }
    harness.elapse(100);
    assert_eq!(
        harness.world.monster(mob).expect("怪还在").shock_time,
        0,
        "攻击动作入口无条件清 Shock（`MonsterObject.cs:2150-2155`）"
    );
    assert!(
        !harness.world.pending_hits().is_empty(),
        "贴身仍能攻击：已排入 300ms 延迟伤害"
    );

    // 有效受击解除：玩家物理攻击命中 → 有效伤害清 Shock。
    harness.ready_all(player);
    let hit_shock_until = harness.world.now() + 5_000;
    {
        let monster = harness.world.monster_mut(mob).expect("怪还在");
        monster.hp = 500;
        monster.shock_time = hit_shock_until;
        monster.target = None;
        monster.operate_time = i64::MAX; // 冻结怪物 AI
        monster.attack_time = 0;
        monster.action_time = 0;
    }
    harness
        .world
        .attack(player, MirDirection::Up)
        .expect("玩家可攻击前方的被困怪");
    harness.elapse(400);
    assert_eq!(
        harness.world.monster(mob).expect("怪还在").shock_time,
        0,
        "有效受击解除 Shock（`MonsterObject.cs:2620-2625`）"
    );
    assert!(
        harness.world.monster(mob).expect("怪还在").hp < 500,
        "确实造成了有效伤害"
    );

    // 护甲早退不解除：物理防御（AC）吃满伤害时 Crystal 直接 `return 0`，不写 `ShockTime = 0`。
    let mut tank_kind = trainer(10, 500);
    tank_kind.stats.min_ac = 1_000;
    tank_kind.stats.max_ac = 1_000;
    let tank = harness.spawn(tank_kind, Point::new(20, 23));
    harness.set_position(player, Point::new(20, 24));
    harness.ready_all(player);
    let shield = harness.world.now() + 5_000;
    harness.world.monster_mut(tank).expect("怪还在").shock_time = shield;
    harness
        .world
        .attack(player, MirDirection::Up)
        .expect("攻向高护甲怪");
    harness.elapse(400);
    assert_eq!(
        harness.world.monster(tank).expect("怪还在").hp,
        500,
        "护甲吃掉全部伤害（miss）"
    );
    assert_eq!(
        harness.world.monster(tank).expect("怪还在").shock_time,
        shield,
        "护甲早退不解除 Shock（`MonsterObject.cs:2569-2585`）"
    );

    // 到期撤销八场 + ObjectRemove。
    assert!(
        harness.world.spell_fields().is_empty(),
        "Shock 被攻击解除后场已提前撤销"
    );
    harness.set_position(player, Point::new(20, 22));
    harness.ready_all(player);
    harness
        .world
        .player_mut(observer)
        .expect("观察者")
        .visible_monsters
        .clear();
    harness.ready_all(observer);
    harness.drain(observer);
    harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("再次施法成功");
    harness.elapse(FIELD_DELAY_MS);
    let land2 = harness.world.now();
    assert_eq!(harness.world.spell_fields().len(), 8);
    let ids = field_ids(&harness.world);
    let expire = harness.world.spell_fields()[0].expire_at;
    assert_eq!(expire, land2 + duration);
    harness.elapse(expire + 1 - harness.world.now());
    assert!(
        harness.world.spell_fields().is_empty(),
        "到期撤销全部八场（`SpellObject.cs:51-70`）"
    );
    // 多重集：每个场 ID 恰好一次 ObjectRemove（重复撤销会被折叠掉，旧写法漏回归）。
    let removed = object_remove_ids(&harness.packets(observer));
    let mut removed_sorted = removed
        .into_iter()
        .filter(|object_id| ids.contains(object_id))
        .collect::<Vec<u32>>();
    removed_sorted.sort_unstable();
    let mut expected_ids: Vec<u32> = ids.iter().copied().collect();
    expected_ids.sort_unstable();
    assert_eq!(
        removed_sorted, expected_ids,
        "每条场恰好广播一次 S.ObjectRemove"
    );
    // 到期移除后再跨视野边界：既不能补发 149（场已不存在），也不能再撤销 26。
    harness.ready_all(observer);
    harness.set_position(observer, Point::new(20, 40));
    harness
        .world
        .walk(observer, MirDirection::Down)
        .expect("观察者可移动");
    assert!(
        object_spells(&harness.packets(observer)).is_empty(),
        "到期移除后跨边界不应再收到 ObjectSpell"
    );
    assert!(
        object_remove_ids(&harness.packets(observer))
            .into_iter()
            .all(|object_id| !ids.contains(&object_id)),
        "到期移除后跨边界不应再收到这些场对象的 ObjectRemove"
    );
}

// ---------------------------------------------------------------------------
// 3. 等级边界 +2/+3、持续时间、地图角剪裁、重复施法、施法者换图
// ---------------------------------------------------------------------------

#[test]
fn round52_trap_hexagon_level_edges_and_recast() {
    let mut harness = Harness::new(13);
    let player = harness.add_player(1, MirClass::Wizard, 10, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 10, Point::new(20, 26));
    harness.learn(player, Spell::TrapHexagon);
    // 技能 2 级：Duration = (2*5+10)*1000 = 20000ms。
    harness.set_magic_level(player, Spell::TrapHexagon, 2);
    harness.equip(player, AMULET_SLOT, AMULET, 3);
    harness.ready_all(player);

    // 等级边界：施法者 10 级 → 12 级（+2）可，13 级（+3）不可。
    let plus2 = harness.spawn(trainer(12, 500), Point::new(19, 19));
    let plus3 = harness.spawn(trainer(13, 500), Point::new(21, 19));
    let cast_now = harness.world.now();
    harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("施法成功");
    harness.elapse(FIELD_DELAY_MS);
    let land_now = harness.world.now();
    assert_eq!(land_now, cast_now + FIELD_DELAY_MS, "落地延迟 500ms");
    let duration = (i64::from(harness.magic_level(player)) * 5 + 10) * 1_000;
    assert_eq!(duration, 20_000, "2 级技能持续时间");
    assert_eq!(
        harness.world.spell_fields()[0].expire_at,
        land_now + duration
    );
    assert_eq!(
        harness.world.monster(plus2).expect("+2 怪").shock_time,
        land_now + duration,
        "等级 <= 施法者+2 的怪被 Shock"
    );
    assert_eq!(
        harness.world.monster(plus3).expect("+3 怪").shock_time,
        0,
        "等级 > 施法者+2 的怪不受影响（`Map.cs:1782`）"
    );
    assert_eq!(harness.world.spell_fields().len(), 8);

    // 进视野同步：ObjectMonster.shock_time 是剩余毫秒（`MonsterObject.cs:2874`）。
    harness
        .world
        .player_mut(observer)
        .expect("观察者")
        .visible_monsters
        .clear();
    harness.ready_all(observer);
    harness.drain(observer);
    harness
        .world
        .walk(observer, MirDirection::Down)
        .expect("观察者可移动");
    let monsters = object_monsters(&harness.packets(observer));
    let (_, remaining) = monsters
        .iter()
        .find(|(object_id, _)| *object_id == plus2)
        .expect("观察者收到中心怪的 ObjectMonster");
    assert!(
        *remaining > 0,
        "ObjectMonster.shock_time 是剩余毫秒（此刻 = {remaining}）"
    );
    assert_eq!(
        *remaining,
        land_now + duration - harness.world.now(),
        "剩余时间 = ShockTime - now"
    );

    // 重复施法：清冷却后再施一次 → 8 + 8 = 16 场（同格不合并），Shock 刷新。
    let first_ids = field_ids(&harness.world);
    harness.ready_all(player);
    harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("重复施法成功");
    harness.elapse(FIELD_DELAY_MS);
    let land2 = harness.world.now();
    assert_eq!(harness.world.spell_fields().len(), 16, "重复施法两批八场");
    assert_eq!(
        harness.world.monster(plus2).expect("+2 怪").shock_time,
        land2 + duration,
        "重复施法刷新 Shock"
    );

    // 第一批到期：只剩第二批 8 场，且第一批的 id 都发了 ObjectRemove。
    harness
        .world
        .player_mut(observer)
        .expect("观察者")
        .visible_monsters
        .clear();
    harness.ready_all(observer);
    harness.drain(observer);
    harness.elapse(land_now + duration + 1 - harness.world.now());
    assert_eq!(
        harness.world.spell_fields().len(),
        8,
        "第一批八场先到期撤销"
    );
    let removed = object_remove_ids(&harness.packets(observer));
    assert!(
        first_ids.is_subset(&removed.into_iter().collect()),
        "第一批的每条场都发了 ObjectRemove"
    );
    harness.elapse(land2 + duration + 1 - harness.world.now());
    assert!(
        harness.world.spell_fields().is_empty(),
        "第二批到期后全部撤销"
    );

    // 地图角：落点 (1,1)，八外围点只有 4 个合法 → 安全剪裁（D2 已批准；
    // Crystal 原判定 `X <= 0` 会跳掉 x=0 且允许 `X == Width`，故有偏差）。
    harness.equip(player, AMULET_SLOT, AMULET, 1);
    let corner_mob = harness.spawn(trainer(10, 500), Point::new(1, 0));
    harness.set_position(player, Point::new(1, 1));
    harness.ready_all(player);
    harness
        .cast(player, MirDirection::Up, Point::new(1, 1))
        .expect("角落施法成功");
    harness.elapse(FIELD_DELAY_MS);
    let land3 = harness.world.now();
    let corner_expected: BTreeSet<(i32, i32)> =
        [(3, 0), (3, 2), (2, 3), (0, 3)].into_iter().collect();
    assert_eq!(
        field_centers(&harness.world),
        corner_expected,
        "地图角安全剪裁：只剩 4 个合法外围点"
    );
    assert_eq!(harness.world.spell_fields().len(), 4, "越界外围场被剪裁");
    assert_eq!(
        harness
            .world
            .monster(corner_mob)
            .expect("怪还在")
            .shock_time,
        land3 + duration,
        "角落中心怪照常 Shock"
    );

    // 施法后换图：落地检查施法者仍在原图（D3 PARTIAL，Crystal 会在旧图生成）。
    harness.ready_all(player);
    harness
        .cast(player, MirDirection::Up, Point::new(1, 1))
        .expect("施法成功");
    harness.world.player_mut(player).expect("玩家").map = "1".to_owned();
    harness.elapse(FIELD_DELAY_MS);
    assert_eq!(
        harness.world.spell_fields().len(),
        4,
        "换图后丢弃落地：场数量不增"
    );
}

// ---------------------------------------------------------------------------
// 4. 材料失败矩阵：每个反例都必须保留公共副作用、不写 CastTime、不排效果
// ---------------------------------------------------------------------------

#[test]
fn round52_trap_hexagon_material_failure_matrix_keeps_common_effects() {
    struct Case {
        name: &'static str,
        setup: fn(&mut Harness, u32),
    }

    let cases = [
        Case {
            name: "缺护身符",
            setup: |_harness, _player| {},
        },
        Case {
            name: "错类型（药水当护身符）",
            setup: |harness, player| harness.equip(player, AMULET_SLOT, POTION, 1),
        },
        Case {
            name: "错形状（护身符 shape 1）",
            setup: |harness, player| harness.equip(player, AMULET_SLOT, WRONG_SHAPE_AMULET, 1),
        },
        Case {
            name: "只在背包、没有装备",
            setup: |harness, player| harness.backpack(player, AMULET, 2),
        },
        Case {
            name: "数量不足（0 个）",
            setup: |harness, player| harness.equip(player, AMULET_SLOT, AMULET, 0),
        },
        Case {
            name: "两件都不是护身符",
            setup: |harness, player| {
                harness.equip(player, AMULET_SLOT, POTION, 1);
                harness.equip(player, AMULET_SLOT + 1, WRONG_SHAPE_AMULET, 1);
            },
        },
    ];

    for case in cases {
        let mut harness = Harness::new(31);
        let player = harness.add_player(1, MirClass::Wizard, 10, Point::new(20, 20));
        let observer = harness.add_player(2, MirClass::Wizard, 10, Point::new(20, 26));
        harness.learn(player, Spell::TrapHexagon);
        // 材料失败发生在「预检命中之后」：必须有一个合法目标，否则走的是目标失败分支。
        let target = harness.spawn(trainer(10, 500), Point::new(19, 19));
        (case.setup)(&mut harness, player);
        if case.name == "只在背包、没有装备" {
            // 反例前提：背包里确有护身符，装备栏为空（否则「错误地搜背包」的变异没有反例）。
            assert!(
                harness
                    .inventory_snapshot(player)
                    .iter()
                    .any(|(_, index, count)| *index == AMULET && *count == 2),
                "反例前提：背包里有 2 个护身符"
            );
            assert_eq!(harness.equipment(player, AMULET_SLOT), None);
        }
        harness.ready_all(player);
        harness.drain(player);
        harness.drain(observer);

        let mp_before = harness.world.player(player).expect("玩家").mp;
        let now = harness.world.now();
        let inventory_before = harness.inventory_snapshot(player);
        let report = harness
            .cast(player, MirDirection::Up, Point::new(20, 20))
            .unwrap_or_else(|error| panic!("{}：材料失败不返回 Err，实际 {error:?}", case.name));

        assert_eq!(report.spell, Spell::TrapHexagon, "{}", case.name);
        assert!(report.damage.is_none(), "{}：没有直接伤害", case.name);
        let caster = harness.world.player(player).expect("玩家");
        assert_eq!(
            caster.mp,
            mp_before - report.cost,
            "{}：材料失败仍扣公共 MP（D2）",
            case.name
        );
        assert_eq!(
            caster.attack_time,
            now + MAGIC_CAST_ACTION_MS,
            "{}：材料失败保留 AttackTime",
            case.name
        );
        assert_eq!(
            caster.magic.spell_time,
            now + MAGIC_SPELL_TIME_MS,
            "{}：材料失败保留 SpellTime",
            case.name
        );
        assert_eq!(
            caster.direction,
            MirDirection::Up,
            "{}：材料失败保留公共 Direction",
            case.name
        );
        assert!(
            !caster.magic.casts.contains_key(&Spell::TrapHexagon),
            "{}：材料失败不写本技能 CastTime",
            case.name
        );
        assert_eq!(
            harness.inventory_snapshot(player),
            inventory_before,
            "{}：材料失败连背包内容也不动",
            case.name
        );
        match case.name {
            "缺护身符" | "只在背包、没有装备" => {
                assert_eq!(harness.equipment(player, AMULET_SLOT), None);
            }
            "错类型（药水当护身符）" => {
                assert_eq!(
                    harness.equipment(player, AMULET_SLOT),
                    Some((POTION, 1)),
                    "非护身符不被扣"
                );
            }
            "错形状（护身符 shape 1）" => {
                assert_eq!(
                    harness.equipment(player, AMULET_SLOT),
                    Some((WRONG_SHAPE_AMULET, 1)),
                    "错形状不被当材料扣"
                );
            }
            "数量不足（0 个）" => {
                assert_eq!(
                    harness.equipment(player, AMULET_SLOT),
                    Some((AMULET, 0)),
                    "0 个不扣"
                );
            }
            "两件都不是护身符" => {
                assert_eq!(harness.equipment(player, AMULET_SLOT), Some((POTION, 1)));
                assert_eq!(
                    harness.equipment(player, AMULET_SLOT + 1),
                    Some((WRONG_SHAPE_AMULET, 1)),
                    "缺材料时不得先扣错误物品"
                );
            }
            _ => unreachable!(),
        }
        assert_eq!(
            harness.world.monster(target).expect("怪还在").shock_time,
            0,
            "{}：cast=false 不 Shock 任何怪",
            case.name
        );
        assert!(
            harness.world.pending_magic().is_empty(),
            "{}：cast=false 不排落地效果",
            case.name
        );
        assert!(
            harness.world.spell_fields().is_empty(),
            "{}：cast=false 不生成场",
            case.name
        );

        // 500ms 后仍然没有任何落地物；玩家收到 `S.Magic{cast:false}`、观察者收到
        // `S.ObjectMagic{cast:false}`，且没有 `S.DeleteItem`。
        harness.elapse(FIELD_DELAY_MS);
        assert!(
            harness.world.spell_fields().is_empty(),
            "{}：材料失败 500ms 后仍无场",
            case.name
        );
        let packets = harness.packets(player);
        assert_eq!(
            object_spells(&packets).len(),
            0,
            "{}：材料失败不发 ObjectSpell",
            case.name
        );
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
            "{}：材料失败仍回显 S.Magic{{Cast: false}}",
            case.name
        );
        let observer_packets = harness.packets(observer);
        assert!(
            object_magic_casts(&observer_packets).contains(&(Spell::TrapHexagon, false)),
            "{}：观察者收到 S.ObjectMagic{{Cast: false}}，实际 {:?}",
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

    // 0 耐久的护身符照样可以作材料（Crystal `GetAmulet` 不看耐久，
    // `HumanObject.cs:6859`；B2 毒雾同口径）。
    let mut harness = Harness::new(32);
    let player = harness.add_player(1, MirClass::Wizard, 10, Point::new(20, 20));
    harness.learn(player, Spell::TrapHexagon);
    harness.equip_durability(player, AMULET_SLOT, AMULET, 1, 0);
    harness.spawn(trainer(10, 500), Point::new(19, 19));
    harness.ready_all(player);
    harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("0 耐久护身符仍是合法材料");
    assert_eq!(
        harness.equipment(player, AMULET_SLOT),
        None,
        "0 耐久护身符被扣掉"
    );
    assert_eq!(harness.world.pending_magic().len(), 1, "效果已排队");
    harness.elapse(FIELD_DELAY_MS);
    assert_eq!(harness.world.spell_fields().len(), 8, "正常落地");
}

// ---------------------------------------------------------------------------
// 5. 目标生命周期 + 纯 +3 拒绝 + 右/下边界 + PoisonCloud Show/视野语义
// ---------------------------------------------------------------------------

#[test]
fn round52_trap_hexagon_target_lifecycle_and_edges() {
    let mut harness = Harness::new(41);
    let player = harness.add_player(1, MirClass::Wizard, 10, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 10, Point::new(20, 26));
    harness.learn(player, Spell::TrapHexagon);
    harness.equip(player, AMULET_SLOT, AMULET, 5);
    harness.ready_all(player);
    harness.drain(observer);

    // A) 预检通过之后、落地之前目标死亡 → 落地重检无中心怪，不生成场。
    let doomed = harness.spawn(trainer(10, 500), Point::new(19, 19));
    harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("预检命中");
    harness.monster_edit(doomed, |monster| monster.dead = true);
    harness.elapse(FIELD_DELAY_MS);
    assert!(
        harness.world.spell_fields().is_empty(),
        "目标死亡：落地无中心怪、不生成场"
    );
    assert!(
        harness
            .world
            .monster(doomed)
            .is_none_or(|monster| monster.shock_time == 0),
        "死亡目标（已移出世界）不应被 Shock"
    );
    assert_eq!(
        object_spells(&harness.packets(observer)).len(),
        0,
        "不生成场就不该有 ObjectSpell"
    );

    // B) 预检通过之后目标移出 3×3 → 落地无中心怪。
    harness.equip(player, AMULET_SLOT, AMULET, 5);
    let mover = harness.spawn(trainer(10, 500), Point::new(19, 19));
    harness.ready_all(player);
    harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("预检命中");
    // 用世界移动辅助（`World::place_monster`：改坐标并同步 AOI 网格），而不是直接改字段。
    assert!(
        harness.world.place_monster(mover, MAP, Point::new(25, 25)),
        "世界移动辅助成功"
    );
    harness.drain(observer);
    harness.elapse(FIELD_DELAY_MS);
    assert!(
        harness.world.spell_fields().is_empty(),
        "目标移位：不生成场"
    );
    assert!(
        object_spells(&harness.packets(observer)).is_empty(),
        "目标移位：不生成场就不该有错误的新 ObjectSpell"
    );
    assert_eq!(harness.world.monster(mover).expect("怪还在").shock_time, 0);

    // C) 预检通过之后目标换图（保持**同坐标**）→ 落地无中心怪。
    harness.equip(player, AMULET_SLOT, AMULET, 5);
    let traveler = harness.spawn(trainer(10, 500), Point::new(19, 19));
    harness.ready_all(player);
    harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("预检命中");
    // 经世界移动辅助换到 map "1" 的**同一坐标**并维护 AOI：这样若落地只用坐标、
    // 不校验目标是否还在施法地图上，就会把异图怪物当成中心怪。直接改字段再挪走坐标
    // 会让「超 3×3」而不是「异图」成为拒绝原因，测不出地图校验。
    assert!(
        harness
            .world
            .place_monster(traveler, "1", Point::new(19, 19)),
        "世界移动辅助成功"
    );
    harness.drain(observer);
    harness.elapse(FIELD_DELAY_MS);
    assert!(
        harness.world.spell_fields().is_empty(),
        "目标换图（同坐标）：不生成场"
    );
    assert!(
        object_spells(&harness.packets(observer)).is_empty(),
        "目标换图：不应有错误的新 ObjectSpell"
    );

    // D) 纯 +3 目标：预检直接拒绝，不耗材。
    harness.equip(player, AMULET_SLOT, AMULET, 2);
    let plus3 = harness.spawn(trainer(13, 500), Point::new(19, 19));
    harness.ready_all(player);
    let mp_before = harness.world.player(player).expect("玩家").mp;
    let report = harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("目标等级超限仍返回 Ok(cast=false)");
    assert!(report.damage.is_none());
    assert_eq!(
        harness.equipment(player, AMULET_SLOT),
        Some((AMULET, 2)),
        "纯 +3 目标：不扣护身符"
    );
    assert_eq!(
        harness.world.player(player).expect("玩家").mp,
        mp_before - report.cost,
        "公共 MP 仍扣"
    );
    assert!(harness.world.pending_magic().is_empty());
    assert!(harness.world.spell_fields().is_empty());
    assert_eq!(harness.world.monster(plus3).expect("怪还在").shock_time, 0);

    // E) 右/下边界：落点 (58,58)、目标 (57,58)。八外围点里只有 4 个在 60×60 内，
    // 其余按合法地图坐标安全剪裁（D2 已批准；Crystal 原判定有 x==Width 越界 bug）。
    harness.equip(player, AMULET_SLOT, AMULET, 2);
    let edge = harness.spawn(trainer(10, 500), Point::new(57, 58));
    harness.set_position(player, Point::new(58, 58));
    // 持续时间按「施法时的技能等级」计算（`magic.rs` 在 cast 时取 `magic.level`）；
    // 施法/落地都会训练技能（Crystal `LevelMagic`），所以要在施法前记等级。
    let level_at_cast = harness.magic_level(player);
    harness.ready_all(player);
    harness
        .cast(player, MirDirection::Up, Point::new(58, 58))
        .expect("边界施法成功");
    harness.elapse(FIELD_DELAY_MS);
    let land = harness.world.now();
    let expected: BTreeSet<(i32, i32)> = [(59, 56), (57, 56), (56, 57), (56, 59)]
        .into_iter()
        .collect();
    assert_eq!(
        field_centers(&harness.world),
        expected,
        "右下边界只保留 4 个合法外围点"
    );
    assert_eq!(harness.world.spell_fields().len(), 4);
    let duration = (i64::from(level_at_cast) * 5 + 10) * 1_000;
    assert_eq!(
        harness.world.monster(edge).expect("怪还在").shock_time,
        land + duration
    );

    // F) PoisonCloud `Show` + 视野四态（R2）：3×3 只有首个 `Show` 对象对客户端可见，
    // 位置是 `CastLocation`（`SpellObject.cs:490-530`），其余 8 格只做服务端伤害场。
    // 覆盖：生成时就在视野内 / 视野内移动 / 离开（场仍存活）/ 重进 / 到期时在视野内。
    let mut harness = Harness::new(43);
    let player = harness.add_player(1, MirClass::Wizard, 10, Point::new(40, 40));
    // 观察者起始在落点 (41,40) 正下方 4 格：生成广播时就已经在视野内。
    let observer = harness.add_player(2, MirClass::Wizard, 10, Point::new(41, 44));
    harness.learn(player, Spell::PoisonCloud);
    harness.equip(player, AMULET_SLOT, AMULET, 5);
    harness.equip(player, POISON_SLOT, GREEN_POISON, 5);
    harness.ready_all(player);
    harness.drain(player);
    harness.drain(observer);
    harness
        .world
        .cast_magic(
            player,
            Spell::PoisonCloud,
            MirDirection::Right,
            0,
            Point::new(41, 40),
        )
        .expect("毒雾施法成功");
    harness.elapse(550);
    assert_eq!(harness.world.spell_fields().len(), 9, "毒雾 3×3 九场");
    let visible_id = harness
        .world
        .spell_fields()
        .iter()
        .find(|field| field.show)
        .expect("首个 Show 场")
        .object_id;
    assert_eq!(
        harness
            .world
            .spell_fields()
            .iter()
            .filter(|field| field.show)
            .count(),
        1,
        "只有首个对象 Show=true"
    );
    // ① 生成时就在视野内：恰好 1 条 ObjectSpell，位置是 CastLocation。
    let spells = object_spells(&harness.packets(observer));
    assert_eq!(spells.len(), 1, "生成时在视野内只收 1 条 ObjectSpell");
    assert_eq!(spells[0].0, visible_id, "补发的是 Show 对象");
    assert_eq!(spells[0].1, Point::new(41, 40), "位置是 CastLocation");
    assert_eq!(spells[0].2, Spell::PoisonCloud);
    // ② 仍在视野内移动：0 条重复补发。
    harness.ready_all(observer);
    harness
        .world
        .walk(observer, MirDirection::Down)
        .expect("观察者可移动");
    assert!(
        object_spells(&harness.packets(observer)).is_empty(),
        "视野内移动不重复补发"
    );
    // ③ 离开视野（场仍存活）：恰好 1 条 ObjectRemove（同批还有玩家离开 AOI 的
    // ObjectRemove，按场 ID 过滤后再断言）。
    harness.ready_all(observer);
    harness.set_position(observer, Point::new(20, 40));
    harness
        .world
        .walk(observer, MirDirection::Down)
        .expect("观察者可移动");
    let field_id_set = field_ids(&harness.world);
    let removed: Vec<u32> = object_remove_ids(&harness.packets(observer))
        .into_iter()
        .filter(|object_id| field_id_set.contains(object_id))
        .collect();
    assert_eq!(
        removed,
        vec![visible_id],
        "离开视野只撤销那 1 个 Show 对象（8 个隐藏内部场不发撤销，同批玩家对象已过滤）"
    );
    // ④ 场还活着时重进：恰好 1 条补发。
    harness.ready_all(observer);
    harness.set_position(observer, Point::new(30, 40));
    harness
        .world
        .walk(observer, MirDirection::Down)
        .expect("观察者可移动");
    let reentered = object_spells(&harness.packets(observer));
    assert_eq!(reentered.len(), 1, "场存活时重进补发 1 条");
    assert_eq!(reentered[0].1, Point::new(41, 40));
    // ⑤ 到期时观察者在视野内：只撤销 1 个可见对象，8 个隐藏内部场不得发未知 ID 撤销。
    let all_field_ids: BTreeSet<u32> = harness
        .world
        .spell_fields()
        .iter()
        .map(|field| field.object_id)
        .collect();
    harness.drain(observer);
    harness.elapse(6_001);
    assert!(harness.world.spell_fields().is_empty(), "毒雾到期回收");
    let removed_at_expiry = object_remove_ids(&harness.packets(observer));
    assert_eq!(
        removed_at_expiry,
        vec![visible_id],
        "到期只撤销 1 个 Show 对象"
    );
    // 回收后跨边界重进：无补发、无重复撤销（同批玩家 ObjectRemove 按已知场 ID 过滤）。
    harness.ready_all(observer);
    harness.set_position(observer, Point::new(20, 40));
    harness
        .world
        .walk(observer, MirDirection::Down)
        .expect("观察者可移动");
    harness.ready_all(observer);
    harness.set_position(observer, Point::new(30, 40));
    harness
        .world
        .walk(observer, MirDirection::Down)
        .expect("观察者可移动");
    let packets = harness.packets(observer);
    assert_eq!(
        object_spells(&packets)
            .iter()
            .filter(|(object_id, _, _)| all_field_ids.contains(object_id))
            .count(),
        0,
        "回收后不再补发"
    );
    assert_eq!(
        object_remove_ids(&packets)
            .iter()
            .filter(|object_id| all_field_ids.contains(object_id))
            .count(),
        0,
        "回收后不再重复撤销"
    );
}

// ---------------------------------------------------------------------------
// 6. 真实专属攻击入口（R1）：守卫拒绝受术 / 宠物独立目标 / AI55 魔法 / AI13 特殊
// ---------------------------------------------------------------------------

#[test]
fn round52_trap_hexagon_real_ai_entries_release_shock() {
    // 1) 守卫：Crystal `Guard.IsAttackTarget` 恒 false（`Monsters/Guard.cs:54-57`）→
    //    真实施法必须无合法目标、拒绝施法，守卫也不会被注 Shock。
    let mut harness = Harness::new(51);
    let player = harness.add_player(1, MirClass::Wizard, 10, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 10, Point::new(20, 24));
    harness.learn(player, Spell::TrapHexagon);
    harness.equip(player, AMULET_SLOT, AMULET, 2);
    let guard = harness.spawn(monster_kind(10, 500, 0, 58), Point::new(19, 19));
    harness.ready_all(player);
    harness.drain(player);
    harness.drain(observer);
    let mp_before = harness.world.player(player).expect("玩家").mp;
    let report = harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("无合法目标仍返回 Ok(cast=false)");
    assert!(report.damage.is_none());
    // 实际拒绝包：施法者收到 `Magic{cast:false}`，观察者收到 `ObjectMagic{cast:false}`；
    // 若实现把守卫当合法目标会走成功路径发 cast=true，这两种断言必须能区分
    //（`damage.is_none()` 对成功施法的 `CastReport` 同样成立，不能单独证明拒绝）。
    let caster_packets = harness.packets(player);
    assert!(
        caster_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::Magic(magic)
                if magic.spell == Spell::TrapHexagon && !magic.cast
        )),
        "守卫拒绝：施法者收到 Magic{{cast:false}}"
    );
    assert!(
        !caster_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::Magic(magic)
                if magic.spell == Spell::TrapHexagon && magic.cast
        )),
        "守卫拒绝：施法者不得收到 Magic{{cast:true}}"
    );
    let observer_casts = object_magic_casts(&harness.packets(observer));
    assert!(
        observer_casts
            .iter()
            .any(|(spell, cast)| *spell == Spell::TrapHexagon && !*cast),
        "守卫拒绝：观察者收到 ObjectMagic{{cast:false}}"
    );
    assert!(
        !observer_casts
            .iter()
            .any(|(spell, cast)| *spell == Spell::TrapHexagon && *cast),
        "守卫拒绝：观察者不得收到 ObjectMagic{{cast:true}}"
    );
    assert_eq!(
        harness.world.monster(guard).expect("守卫还在").shock_time,
        0,
        "守卫不是合法受术对象，不被困"
    );
    assert_eq!(
        harness.equipment(player, AMULET_SLOT),
        Some((AMULET, 2)),
        "无合法目标不耗材"
    );
    assert_eq!(
        harness.world.player(player).expect("玩家").mp,
        mp_before - report.cost
    );
    assert!(harness.world.pending_magic().is_empty());
    assert!(harness.world.spell_fields().is_empty());

    // 2) 可攻击宠物（另一个玩家的召唤物）：落地清 `pet_target`；AI 早退路径也清。
    let mut harness = Harness::new(52);
    let player = harness.add_player(1, MirClass::Wizard, 10, Point::new(20, 20));
    let master = harness.add_player(2, MirClass::Wizard, 10, Point::new(20, 24));
    for object_id in [player, master] {
        harness
            .world
            .player_mut(object_id)
            .expect("玩家")
            .attack_mode = AttackMode::All;
    }
    harness.learn(player, Spell::TrapHexagon);
    harness.equip(player, AMULET_SLOT, AMULET, 2);
    let pet = harness.spawn(monster_kind(10, 500, 0, 1), Point::new(19, 19));
    harness.monster_edit(pet, |monster| monster.master = Some(master));
    assert!(harness.world.set_pet_mode(master, PetMode::Both));
    // 前置：宠物必须先有独立目标（`pet_target`）——否则「落地清 pet_target」这条断言
    // 把实现删掉也会通过（本来就是 None）。
    harness.monster_edit(pet, |monster| {
        monster.pet_target = Some(player);
        monster.target = Some(player);
    });
    assert_eq!(
        harness.world.monster(pet).expect("宠物还在").pet_target,
        Some(player),
        "前置：宠物独立目标已设置"
    );
    harness.ready_all(player);
    harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("宠物是可攻击目标");
    harness.elapse(FIELD_DELAY_MS);
    let land = harness.world.now();
    let duration = (i64::from(harness.magic_level(player)) * 5 + 10) * 1_000;
    assert_eq!(
        harness.world.monster(pet).expect("宠物还在").shock_time,
        land + duration,
        "可攻击宠物被 Shock"
    );
    assert_eq!(
        harness.world.monster(pet).expect("宠物还在").pet_target,
        None,
        "落地清宠物独立目标（`Map.cs:1785-1786` 的 `Target = null` 对应）"
    );
    // 主人缺失（`process_pet_ai` 早退）时，目标仍按可达范围清掉。
    harness.set_position(player, Point::new(20, 30));
    let now = harness.world.now();
    harness.monster_edit(pet, |monster| {
        monster.master = Some(9_999);
        monster.pet_target = Some(player);
        monster.target = Some(player);
        monster.operate_time = -1;
        monster.action_time = 0;
        monster.attack_time = 0;
    });
    harness.elapse(100);
    assert_eq!(
        harness.world.monster(pet).expect("宠物还在").pet_target,
        None,
        "AI 早退路径（主人缺失）也清不可达的 pet_target"
    );
    assert_eq!(
        harness.world.monster(pet).expect("宠物还在").target,
        None,
        "不可达的 target 同样被清"
    );
    assert!(
        harness.world.monster(pet).expect("宠物还在").shock_time > now,
        "清目标不解除 Shock"
    );
    // 贴身攻击仍正常：攻击入口无条件清 Shock。
    harness.set_position(player, Point::new(19, 20));
    harness.monster_edit(pet, |monster| {
        monster.master = Some(master);
        monster.pet_target = Some(player);
        monster.action_time = 0;
        monster.attack_time = 0;
        monster.move_time = 0;
        monster.operate_time = -1;
    });
    harness.elapse(100);
    assert_eq!(
        harness.world.monster(pet).expect("宠物还在").shock_time,
        0,
        "宠物贴身攻击入口清 Shock（`MonsterObject.cs:2150-2155`）"
    );
    assert!(
        !harness.world.pending_hits().is_empty(),
        "宠物确实攻击了（伤害已排队）"
    );

    // 3) AI55 HumanWizard：真实 `process_magic_target` → `cast_monster_magic` 入口清 Shock。
    let mut harness = Harness::new(53);
    let player = harness.add_player(1, MirClass::Wizard, 10, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 10, Point::new(20, 26));
    harness.learn(player, Spell::TrapHexagon);
    harness.equip(player, AMULET_SLOT, AMULET, 2);
    // AI55 雷电术伤害走 `MinMC/MaxMC`（`monster.rs` 的 `use_magic_stats` 分支）；
    // 默认模板魔法攻击 0 会让 `cast_monster_magic` 在 `base == 0` 时提前返回。
    let mut wizard_kind = monster_kind(10, 500, 0, 55);
    wizard_kind.stats.min_mc = 5;
    wizard_kind.stats.max_mc = 5;
    let wizard = harness.spawn(wizard_kind, Point::new(19, 19));
    harness.ready_all(player);
    harness.drain(observer);
    harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("魔法怪可作中心目标");
    harness.elapse(FIELD_DELAY_MS);
    assert!(
        harness.world.monster(wizard).expect("怪还在").shock_time > harness.world.now(),
        "前置：AI55 已被 Shock"
    );
    harness.set_position(player, Point::new(22, 19));
    let now = harness.world.now();
    harness.monster_edit(wizard, |monster| {
        monster.target = Some(player);
        monster.attack_time = 0;
        monster.action_time = 0;
        monster.move_time = 0;
        monster.operate_time = -1;
        // 非 0 且已过：跳过 HumanWizard 的 kite 走位，直接进施法决策。
        monster.fear_time = now;
    });
    harness.elapse(100);
    assert_eq!(
        harness.world.monster(wizard).expect("怪还在").shock_time,
        0,
        "AI55 魔法攻击入口清 Shock（`Monsters/HumanWizard.cs:26-55`）"
    );
    assert!(!harness.world.pending_hits().is_empty(), "雷电术伤害已排队");
    let observer_magic: Vec<(Spell, bool)> = harness
        .packets(observer)
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectMagic(packet) => Some((packet.spell, packet.cast)),
            _ => None,
        })
        .collect();
    assert!(
        observer_magic.contains(&(Spell::ThunderBolt, true)),
        "观察者看到怪物雷电术，实际 {observer_magic:?}"
    );

    // 4) AI13 RedMoonEvil：特殊 AOE 真实入口清 Shock（`Monsters/RedMoonEvil.cs:31-52`）。
    let mut harness = Harness::new(54);
    let player = harness.add_player(1, MirClass::Wizard, 10, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 10, Point::new(20, 26));
    harness.learn(player, Spell::TrapHexagon);
    harness.equip(player, AMULET_SLOT, AMULET, 2);
    let aoe = harness.spawn(monster_kind(10, 500, 0, 13), Point::new(19, 19));
    harness.ready_all(player);
    harness.drain(observer);
    harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("AOE 怪可作中心目标");
    harness.elapse(FIELD_DELAY_MS);
    assert!(
        harness.world.monster(aoe).expect("怪还在").shock_time > harness.world.now(),
        "前置：AI13 已被 Shock"
    );
    harness.set_position(player, Point::new(19, 21));
    harness.monster_edit(aoe, |monster| {
        monster.target = Some(player);
        monster.attack_time = 0;
        monster.action_time = 0;
        monster.move_time = 0;
        monster.operate_time = -1;
    });
    harness.elapse(100);
    assert_eq!(
        harness.world.monster(aoe).expect("怪还在").shock_time,
        0,
        "AI13 特殊 AOE 攻击入口清 Shock（`Monsters/RedMoonEvil.cs:31-52`）"
    );
    assert!(
        !harness.world.pending_hits().is_empty(),
        "特殊 AOE 伤害已排队"
    );

    // 5) AI12 BugBagMaggot：召唤动作本身就是攻击入口
    //    （`Monsters/BugBagMaggot.cs:30-53` 第一行 `ShockTime = 0;`）。
    //    这里用真实召唤结果（新召唤实体 + 目标归属 + 攻击时钟）证明走的是召唤入口，
    //    而不是仅在分流处人为清零。
    let mut harness = Harness::new(55);
    let player = harness.add_player(1, MirClass::Wizard, 10, Point::new(20, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 10, Point::new(20, 24));
    harness.learn(player, Spell::TrapHexagon);
    harness.equip(player, AMULET_SLOT, AMULET, 2);
    let maggot = harness.spawn(monster_kind(10, 500, 0, 12), Point::new(19, 19));
    harness.ready_all(player);
    harness.drain(observer);
    let monsters_before = harness.world.monsters().len();
    harness
        .cast(player, MirDirection::Up, Point::new(20, 20))
        .expect("召唤怪可作中心目标");
    harness.elapse(FIELD_DELAY_MS);
    assert!(
        harness.world.monster(maggot).expect("怪还在").shock_time > harness.world.now(),
        "前置：AI12 已被 Shock"
    );
    harness.set_position(player, Point::new(19, 20));
    let now = harness.world.now();
    harness.monster_edit(maggot, |monster| {
        monster.target = Some(player);
        monster.attack_time = 0;
        monster.action_time = 0;
        monster.move_time = 0;
        monster.operate_time = -1;
    });
    harness.drain(observer);
    harness.elapse(100);
    assert_eq!(
        harness.world.monster(maggot).expect("怪还在").shock_time,
        0,
        "AI12 召唤入口清 Shock（`Monsters/BugBagMaggot.cs:30-53`）"
    );
    // 真实召唤发生：怪物总数 +1，召唤物以玩家为目标，Maggot 攻击时钟被推后，观察者
    // 收到新召唤物的 ObjectMonster（都是 `process_bugbat_summon` 的真实副作用）。
    assert_eq!(
        harness.world.monsters().len(),
        monsters_before + 1,
        "AI12 真的召唤了 BugBat"
    );
    let summons: Vec<u32> = harness
        .world
        .monsters()
        .iter()
        .filter(|monster| monster.object_id != maggot)
        .map(|monster| monster.object_id)
        .collect();
    assert_eq!(summons.len(), 1, "恰好一个召唤物");
    let slave = summons[0];
    assert_eq!(
        harness.world.monster(slave).expect("召唤物").target,
        Some(player),
        "召唤物带攻击目标（真实召唤动作的副作用）"
    );
    let slave_view = harness.world.monster(slave).expect("召唤物");
    assert_eq!(
        slave_view.kind.image,
        MonsterImage::BugBat,
        "召唤物是 BugBat（真实召唤动作的产物）"
    );
    assert!(
        harness.world.monster(maggot).expect("怪还在").attack_time > now,
        "Maggot 攻击时钟被推后（真实召唤动作）"
    );
    let summoned_frames: Vec<u32> = harness
        .packets(observer)
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectMonster(packet) if packet.object_id == slave => Some(slave),
            _ => None,
        })
        .collect();
    assert_eq!(summoned_frames, vec![slave], "观察者看到召唤物进入视野");
}

// ---------------------------------------------------------------------------
// 7. F1（复审 v2 / R2）：火墙没有接 `field_views` 记账链，
//    生成/替换/到期后的视野内移动都不得重复补发 149 或重复撤销 26。
// ---------------------------------------------------------------------------

#[test]
fn round52_firewall_view_sync_has_no_duplicates() {
    let mut harness = Harness::new(61);
    let player = harness.add_player(1, MirClass::Wizard, 10, Point::new(20, 20));
    // 观察者就在落点旁，生成广播与后续移动都在视野内。
    let observer = harness.add_player(2, MirClass::Wizard, 10, Point::new(20, 23));
    harness.learn(player, Spell::FireWall);
    harness.ready_all(player);
    harness.drain(observer);

    // 第一次施法：落点 (21,22) → 5 格火墙（落点 + 上右下左）。
    harness
        .world
        .cast_magic(
            player,
            Spell::FireWall,
            MirDirection::Up,
            0,
            Point::new(21, 22),
        )
        .expect("火墙施法成功");
    harness.elapse(500);
    let first_ids = field_ids(&harness.world);
    assert_eq!(first_ids.len(), 5, "火墙 5 格");
    let generated = object_spells(&harness.packets(observer));
    assert_eq!(generated.len(), 5, "生成时逐格广播 5 条 ObjectSpell");
    assert!(
        generated
            .iter()
            .all(|(_, _, spell)| *spell == Spell::FireWall),
        "生成广播的场类型都是火墙"
    );
    // ① 火墙生成后视野内移动：0 条重复 149。
    harness.ready_all(observer);
    harness
        .world
        .walk(observer, MirDirection::Down)
        .expect("观察者可移动");
    assert!(
        object_spells(&harness.packets(observer)).is_empty(),
        "火墙生成后视野内移动不得重复补发 ObjectSpell（F1）"
    );

    // 第二次施法：地图上限 1 次施法实例 → 回收最老一次（5 条 26）后重新铺 5 格。
    harness.ready_all(player);
    harness.drain(observer);
    harness
        .world
        .cast_magic(
            player,
            Spell::FireWall,
            MirDirection::Up,
            0,
            Point::new(21, 22),
        )
        .expect("第二次火墙施法成功");
    harness.elapse(500);
    let second_ids = field_ids(&harness.world);
    assert_eq!(second_ids.len(), 5, "替换后仍是 5 格火墙");
    assert!(first_ids.is_disjoint(&second_ids), "替换生成新对象 ID");
    let packets = harness.packets(observer);
    let mut removed_by_replace: Vec<u32> = object_remove_ids(&packets)
        .into_iter()
        .filter(|object_id| first_ids.contains(object_id))
        .collect();
    removed_by_replace.sort_unstable();
    let mut expected_removed: Vec<u32> = first_ids.iter().copied().collect();
    expected_removed.sort_unstable();
    assert_eq!(
        removed_by_replace, expected_removed,
        "替换回收：每个旧场 ID 恰好一条 ObjectRemove"
    );
    assert_eq!(
        object_spells(&packets).len(),
        5,
        "替换后重新广播 5 条新 ObjectSpell"
    );
    // ② 火墙替换/回收后视野内移动：0 条重复 26、0 条重复 149。
    harness.ready_all(observer);
    harness
        .world
        .walk(observer, MirDirection::Down)
        .expect("观察者可移动");
    let packets = harness.packets(observer);
    assert!(
        object_spells(&packets).is_empty(),
        "替换后移动不得重复补发 ObjectSpell"
    );
    assert!(
        object_remove_ids(&packets)
            .iter()
            .all(|object_id| !first_ids.contains(object_id)),
        "替换后移动不得重复撤销旧场 ObjectRemove（F1）"
    );

    // ③ 到期回收：逐格 26 各恰好一次；之后跨边界不再补发/撤销。
    let expire = harness.world.spell_fields()[0].expire_at;
    harness.drain(observer);
    harness.elapse(expire + 1 - harness.world.now());
    assert!(harness.world.spell_fields().is_empty(), "火墙到期回收");
    let packets = harness.packets(observer);
    let mut removed_at_expiry: Vec<u32> = object_remove_ids(&packets)
        .into_iter()
        .filter(|object_id| second_ids.contains(object_id))
        .collect();
    removed_at_expiry.sort_unstable();
    let mut expected_removed: Vec<u32> = second_ids.iter().copied().collect();
    expected_removed.sort_unstable();
    assert_eq!(
        removed_at_expiry, expected_removed,
        "到期时每个场 ID 恰好一次 ObjectRemove"
    );
    harness.ready_all(observer);
    harness.set_position(observer, Point::new(20, 40));
    harness
        .world
        .walk(observer, MirDirection::Down)
        .expect("观察者可移动");
    harness.ready_all(observer);
    harness.set_position(observer, Point::new(20, 23));
    harness
        .world
        .walk(observer, MirDirection::Up)
        .expect("观察者可移动");
    let packets = harness.packets(observer);
    assert!(
        object_spells(&packets).is_empty(),
        "到期后跨边界不再补发 ObjectSpell"
    );
    assert!(
        object_remove_ids(&packets)
            .iter()
            .all(|object_id| !second_ids.contains(object_id)),
        "到期后跨边界不再重复撤销 ObjectRemove"
    );
}
