//! 第 52 轮 C 批：**真实服务端 World → 线上编码帧 → 客户端解码 → 快照状态**。
//!
//! 门下 M01/M04/M08 要求以真实产包（而不是手造成功包/仅背包快照）证明三处客户端
//! 消费链修复：
//!
//! 1. **装备耗材 `S.DeleteItem`(79) 真实链**（M01）：B1 的 `consume_spell_material`
//!    对**装备 UUID** 发 `DeleteItem`，客户端必须同时命中装备与背包：部分扣数只减
//!    `count`、整堆耗光摘槽、错误 UUID 不动、背包同模板不误扣；每件耗材一条精确
//!    `Count` 的包。Crystal `HumanObject.cs:1346`（`ConsumeItem`）→
//!    `S.DeleteItem`（`Client/MirScenes/GameScene.cs:3213-3230`）。
//! 2. **法术场生命周期**（M04/E1/E3）：服务端对毒雾/困魔咒有视野差分
//!    （`world.rs` 的 `sync_player_spell_fields`）——生成/进场补发是
//!    `S.ObjectSpell`(149)、离场/到期/清场是 `S.ObjectRemove`(26)。客户端按对象 ID
//!    登记持久场，149 不再只是一条瞬态事件；125 是 `S.ObjectProjectile`，**不是撤销**
//!    （§9 勘误 E1），这里用协议负向断言钉死。
//! 3. **冲刺失败回纠**（152/153）：`S.UserDashFail`(152) 与 `S.ObjectDashFail`(153)
//!    的语义是「撤销本地预测」，前置必须是**错误预测格 + 非空既有动作/motion**：
//!    152 把自机钉回权威格、清掉预测动作（不按 1 格 = Walking 重启动画），并让生产
//!    渲染步骤 `camera::prepare_frame` 清空自机 `SelfMotion`；153 清远端 motion 并把
//!    实体置 Standing。Crystal `GameScene.cs:3540-3556`（`UserDashFail`）、
//!    `:3527-3539`（`ObjectDashFail`）。
//!
//! 真实链：进程内起真实 [`World`]、真学技能/真施法/真走路，从玩家收包通道取服务端
//! 已经编码好的帧，再用 `ServerPacket::decode` + [`WorldSnapshot::apply_packet_at`]
//! 驱动客户端快照。装备/背包初值从**服务端当前状态**复制（登录态摆位，不是成功包），
//! 之后的所有状态变化都来自真实产包。
//!
//! 变异反证（隔离副本，见 `target/round52/c/00-summary.md`）：删除本文件覆盖的
//! 4 处消费分支后 `round52_` 定向测试必红，证明断言可失败。

use std::collections::BTreeSet;
use std::sync::Arc;

use mir_client::camera;
use mir_client::net::{InventoryItem, WorldSnapshot};
use mir_core::Point;
use mir_core::enums::{
    AttackMode, ItemType, MirClass, MirDirection, MirGender, Monster as MonsterImage, Spell,
};
use mir_db::{MagicInfo, MapInfo, MirDb};
use mir_protocol::ServerPacket;
use mir_protocol::data::ItemInfo;
use mir_protocol::packets::server::{ObjectProjectile, UserInformation, UserLocation};
use mir_render::animation::ActorAction;
use mir_server::combat::{self, CombatStats};
use mir_server::gamedata::GameData;
use mir_server::magic::SpellField;
use mir_server::{MagicState, MailState, MonsterKind, NpcState, Player, TradeState, World};
use tokio::sync::mpsc;

/// 测试地图文件名（`synthetic_map` 的合成 30×30 空地）。
const MAP: &str = "0";
/// 地图主键（`MapInfo.index`，与文件名分开是为了覆盖 `map_index` 查找）。
const MAP_INDEX: i32 = 990;

/// 护身符模板（`GetAmulet` 认 `ItemType::Amulet` + 形状 0）。
const AMULET: i32 = 101;
/// 绿毒模板（`GetPoison(5, 1)` 认 `ItemType::Amulet` + 形状 1）。
const GREEN_POISON: i32 = 102;

/// 装备槽：B1 单测用 6/9，这里沿用（不是最近的槽位也能按 UUID 命中）。
const AMULET_SLOT: usize = 6;
/// 见 [`AMULET_SLOT`]。
const POISON_SLOT: usize = 9;

const AMULET_UID: u64 = 70_001;
const POISON_UID: u64 = 70_002;
/// 背包里与装备同模板的绿毒：UUID 不同，耗材扣装备时**不得**被误扣。
const BAG_POISON_UID: u64 = 70_003;
/// 困魔咒护身符：与毒雾护身符区分 UUID，便于逐件断言。
const TRAP_AMULET_UID: u64 = 70_004;

// ---------------------------------------------------------------------------
// 客户端快照辅助
// ---------------------------------------------------------------------------

/// 给快照补一条「已进入世界」的 `S.UserInformation`（与 `round51_spells_wire.rs` 同）。
fn seed_self(
    snapshot: &mut WorldSnapshot,
    object_id: u32,
    name: &str,
    position: Point,
    level: u16,
) {
    let info = UserInformation {
        object_id,
        real_id: object_id,
        name: name.to_owned(),
        class: MirClass::Warrior,
        gender: MirGender::Male,
        level,
        location: position,
        direction: MirDirection::Down,
        hair: 1,
        hp: 100,
        mp: 100,
        ..UserInformation::default()
    };
    snapshot.apply_packet_at(&ServerPacket::UserInformation(info), 0);
}

/// 从**服务端当前状态**复制背包/装备初值到客户端快照。
///
/// 这是登录态的摆位（`S.UserInformation.Inventory/Equipment` 解码后的形状），
/// 不是「手造成功包」：之后断言的状态变化全部来自真实 `DeleteItem`/`ObjectSpell` 等包。
fn seed_items(snapshot: &mut WorldSnapshot, world: &World, object_id: u32) {
    let player = world.player(object_id).expect("玩家在世界里");
    snapshot.inventory = player
        .inventory
        .iter()
        .map(|item| item.as_ref().map(InventoryItem::from_user_item))
        .collect();
    snapshot.equipment = player
        .equipment
        .iter()
        .map(|item| item.as_ref().map(InventoryItem::from_user_item))
        .collect();
}

/// 背包/装备里按唯一编号取数量（null 表示该格为空）。
fn snapshot_count(items: &[Option<InventoryItem>], unique_id: u64) -> Option<(i32, u16)> {
    items
        .iter()
        .flatten()
        .find(|item| item.unique_id == unique_id)
        .map(|item| (item.item_index, item.count))
}

/// 把一帧服务端原始帧解码成 [`ServerPacket`]（与网络线程同一入口）。
fn decode(raw: Vec<Vec<u8>>) -> Vec<ServerPacket> {
    raw.into_iter()
        .map(|frame| {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            ServerPacket::decode(index, &frame[4..]).expect("服务端帧必须能被客户端解码")
        })
        .collect()
}

/// 按顺序把包喂给快照（`apply_packet_at` 是网络线程与无头 UI 共用入口）。
fn apply_packets(snapshot: &mut WorldSnapshot, packets: &[ServerPacket]) {
    for (index, packet) in packets.iter().enumerate() {
        let now_ms = 1_000 + u64::try_from(index).unwrap_or(0) * 10;
        snapshot.apply_packet_at(packet, now_ms);
    }
}

/// 一条 `S.ObjectSpell` 的关键字段：`(对象 ID, 位置, 技能, param)`。
type SpellWire = (u32, Point, Spell, bool);

fn object_spells(packets: &[ServerPacket]) -> Vec<SpellWire> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectSpell(packet) => Some((
                packet.object_id,
                packet.location,
                packet.spell,
                packet.param,
            )),
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

fn delete_items(packets: &[ServerPacket]) -> Vec<(u64, u16)> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::DeleteItem(packet) => Some((packet.unique_id, packet.count)),
            _ => None,
        })
        .collect()
}

fn user_dash_fail(packets: &[ServerPacket]) -> Option<(Point, MirDirection)> {
    packets.iter().find_map(|packet| match packet {
        ServerPacket::UserDashFail(packet) => Some((packet.location, packet.direction)),
        _ => None,
    })
}

fn object_dash_fail(packets: &[ServerPacket]) -> Option<(u32, Point, MirDirection)> {
    packets.iter().find_map(|packet| match packet {
        ServerPacket::ObjectDashFail(packet) => {
            Some((packet.object_id, packet.location, packet.direction))
        }
        _ => None,
    })
}

/// 客户端快照里的场编号集合（诊断/差分断言用）。
fn snapshot_field_ids(snapshot: &WorldSnapshot) -> BTreeSet<u32> {
    snapshot
        .spell_fields
        .iter()
        .map(|field| field.object_id)
        .collect()
}

/// 服务端仍在场上的可见场（`show` 为 true 才会进 149/26 差分链）。
fn server_visible_fields(world: &World) -> Vec<&SpellField> {
    world
        .spell_fields()
        .iter()
        .filter(|field| field.show)
        .collect()
}

// ---------------------------------------------------------------------------
// 服务端真实 World 辅助（与 `round51_spells_wire.rs` / server round52 测试同构）
// ---------------------------------------------------------------------------

/// 30×30 全可走地图（与 `crates/mir-server/tests/common/mod.rs` 的合成图一致）。
fn synthetic_map(name: &str) -> Arc<mir_world::Map> {
    mir_world::Map {
        width: 30,
        height: 30,
        cells: vec![mir_world::MapCell::default(); 900],
        file_name: name.to_owned(),
        version: mir_world::MapVersion::V0,
        doors: Vec::new(),
    }
    .into()
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

/// 毒雾 `MagicInfo`：耗蓝 5、落地延迟 `距离 × 50 + 500`、射程 8（同 `round52_poison_cloud`）。
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

/// 困魔咒 `MagicInfo`：耗蓝 5、无冷却、落地延迟 500ms、射程 8（同 `round52_trap_hexagon`）。
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
        range: 8,
        multiplier_base: 1.0,
        multiplier_bonus: 0.0,
    }
}

/// 火墙 `MagicInfo`：走逐格直接广播，不进 `field_views` 差分链（E3 负例）。
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

/// 冲撞 `MagicInfo`（与 `round51_spells_wire.rs` 的 `magic_info` 一致）。
fn magic_info(spell: Spell) -> MagicInfo {
    MagicInfo {
        name: format!("{spell:?}"),
        spell: spell.value(),
        base_cost: 1,
        level_cost: 1,
        icon: 0,
        level1: 1,
        level2: 2,
        level3: 3,
        need1: 0,
        need2: 0,
        need3: 0,
        delay_base: 1_000,
        delay_reduction: 0,
        power_base: 4,
        power_bonus: 0,
        m_power_base: 10,
        m_power_bonus: 0,
        range: 0,
        multiplier_base: 1.0,
        multiplier_bonus: 0.0,
    }
}

/// 真实世界：最小配表 + 合成图 + 时钟冻结到 0（`with_data` 先 `advance_clock(0)`）。
fn wire_world() -> World {
    let db = MirDb {
        magics: vec![
            poison_cloud_info(),
            trap_hexagon_info(),
            fire_wall_info(),
            magic_info(Spell::ShoulderDash),
        ],
        items: vec![amulet(AMULET, 0, "护身符"), amulet(GREEN_POISON, 1, "绿毒")],
        maps: vec![MapInfo {
            index: MAP_INDEX,
            file_name: MAP.to_owned(),
            title: "R52-C 测试场".to_owned(),
            ..MapInfo::default()
        }],
        ..MirDb::default()
    };
    let mut world = World::new(999);
    // 先冻结时钟再建数据：`World::now` 在首次 `advance_clock` 前取真实墙钟，
    // 会让「延迟 600ms 落地」这类断言随机器快慢抖动（B2-review 复现过）。
    world.advance_clock(0);
    world.seed_rng(52);
    world.set_game_data(Arc::new(GameData::from_db(db, None)));
    world.insert_map(MAP, synthetic_map(MAP));
    world
}

/// 真玩家 + 收包通道（字段与 `crates/mir-server/tests/common/mod.rs` 同步）。
fn add_player(
    world: &mut World,
    object_id: u32,
    level: u16,
    class: MirClass,
    position: Point,
) -> mpsc::UnboundedReceiver<Vec<u8>> {
    let stats = combat::player_base_stats(level, class);
    let (sender, receiver) = mpsc::unbounded_channel();
    world.enter(Player {
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
        name_colour: mir_core::Color(-1),
        visible_monsters: BTreeSet::new(),
        visible_npcs: BTreeSet::new(),
        movement_time: 0,
        inventory: mir_server::items::new_inventory(),
        equipment: mir_server::items::new_equipment(),
        storage: mir_server::storage::new_storage(),
        magics: Vec::new(),
        magic: MagicState::default(),
        gold: 0,
        credit: 0,
        visible_items: BTreeSet::new(),
        quests: Vec::new(),
        completed_quests: BTreeSet::new(),
        mail: MailState::default(),
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
        trade: TradeState::default(),
        trade_invitation: None,
        next_trade_time: 0,
        allow_trade: true,
        bind_map_index: 0,
        bind_point: Point::default(),
        sender,
    });
    receiver
}

fn learn(world: &mut World, object_id: u32, spell: Spell, level: u8) {
    world.learn_magic(object_id, spell).expect("学技能应成功");
    if let Some(player) = world.player_mut(object_id) {
        if let Some(entry) = player.magics.iter_mut().find(|m| m.spell == spell) {
            entry.level = level;
        }
    }
}

/// 清动作/技能计时与冷却、回满魔法（等价于「等了足够久」，B2 测试的 `ready_all`）。
fn ready(world: &mut World, object_id: u32) {
    let Some(player) = world.player_mut(object_id) else {
        return;
    };
    player.action_time = 0;
    player.attack_time = 0;
    player.magic.spell_time = 0;
    player.magic.casts.clear();
    player.mp = player.stats.mp;
}

/// 真实 tick 推进（先快进时钟再结算延迟效果）。
fn elapse(world: &mut World, ms: i64) {
    world.advance_clock(ms);
    world.tick();
}

fn drain(receiver: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    while let Ok(frame) = receiver.try_recv() {
        out.push(frame);
    }
    out
}

fn frames(receiver: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<Vec<u8>> {
    drain(receiver)
}

/// 用生产 `create_fresh_item` 造一件真实物品并钉 UUID/数量，放进装备槽。
fn give_equipment(
    world: &mut World,
    object_id: u32,
    slot: usize,
    index: i32,
    count: u16,
    uid: u64,
) {
    let info = world.item_info(index).expect("物品模板").clone();
    let mut item = world.create_fresh_item(&info);
    item.unique_id = uid;
    item.count = count;
    world.player_mut(object_id).expect("玩家在世界上").equipment[slot] = Some(item);
}

/// 同 [`give_equipment`]，放进背包第一格。
fn give_inventory(
    world: &mut World,
    object_id: u32,
    slot: usize,
    index: i32,
    count: u16,
    uid: u64,
) {
    let info = world.item_info(index).expect("物品模板").clone();
    let mut item = world.create_fresh_item(&info);
    item.unique_id = uid;
    item.count = count;
    world.player_mut(object_id).expect("玩家在世界上").inventory[slot] = Some(item);
}

fn equipment_count(world: &World, object_id: u32, slot: usize) -> Option<(i32, u16)> {
    world.player(object_id).and_then(|player| {
        player
            .equipment
            .get(slot)
            .and_then(Option::as_ref)
            .map(|item| (item.item_index, item.count))
    })
}

fn inventory_count(world: &World, object_id: u32, slot: usize) -> Option<(i32, u16)> {
    world.player(object_id).and_then(|player| {
        player
            .inventory
            .get(slot)
            .and_then(Option::as_ref)
            .map(|item| (item.item_index, item.count))
    })
}

/// 训练木桩（AI 56）：不会走动/攻击，但结算链照常跑（同 `round52_poison_cloud`）。
fn trainer(hp: i32) -> MonsterKind {
    MonsterKind {
        name: "困魔咒靶子".to_owned(),
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

/// 困魔咒八外围场相对落点中心的偏移（Crystal `Map.cs:1797-1829`）。
const TRAP_FIELD_OFFSETS: [(i32, i32); 8] = [
    (1, -2),
    (-1, -2),
    (2, -1),
    (2, 1),
    (1, 2),
    (-1, 2),
    (-2, 1),
    (-2, -1),
];

fn expected_trap_centers(center: Point) -> BTreeSet<(i32, i32)> {
    TRAP_FIELD_OFFSETS
        .iter()
        .map(|(dx, dy)| (center.x + dx, center.y + dy))
        .collect()
}

// ---------------------------------------------------------------------------
// 1. 装备耗材 DeleteItem 真实链 + 法术场生命周期（149/26 + 进场补发 + 125 负例）
// ---------------------------------------------------------------------------

#[test]
fn round52_wire_equipped_material_count_and_field_lifecycle() {
    let mut world = wire_world();
    let mut caster_rx = add_player(&mut world, 1, 20, MirClass::Wizard, Point::new(20, 20));

    // 装备：槽 6 护身符 6 枚、槽 9 绿毒 5 枚；背包放同模板绿毒 5 枚（UUID 不同）。
    give_equipment(&mut world, 1, AMULET_SLOT, AMULET, 6, AMULET_UID);
    give_equipment(&mut world, 1, POISON_SLOT, GREEN_POISON, 5, POISON_UID);
    give_inventory(&mut world, 1, 0, GREEN_POISON, 5, BAG_POISON_UID);
    learn(&mut world, 1, Spell::PoisonCloud, 1);
    ready(&mut world, 1);
    drain(&mut caster_rx); // 丢弃入场包：自机状态由 seed_self 注入（入场包另有 round51 覆盖）。

    // 客户端快照 = 服务端登录态（不是成功包）。
    let mut caster_snapshot = WorldSnapshot::default();
    seed_self(&mut caster_snapshot, 1, "玩家1", Point::new(20, 20), 20);
    seed_items(&mut caster_snapshot, &world, 1);
    assert_eq!(
        snapshot_count(&caster_snapshot.equipment, AMULET_UID),
        Some((AMULET, 6)),
        "登录态：护身符 6 枚"
    );

    // 真施法：落点 (22,20)，距离 2 → 落地延迟 600ms。
    world
        .cast_magic(
            1,
            Spell::PoisonCloud,
            MirDirection::Right,
            0,
            Point::new(22, 20),
        )
        .expect("材料齐全时毒雾施法成功");
    let cast_packets = decode(frames(&mut caster_rx));

    // ---- M01：装备 UUID 的 DeleteItem 精确 Count ----
    let deletes = delete_items(&cast_packets);
    assert_eq!(
        deletes.len(),
        2,
        "两件耗材各一条 DeleteItem（逐物品单包、不合并/不重复），实际={deletes:?}"
    );
    assert!(
        deletes.contains(&(AMULET_UID, 5)),
        "护身符按装备 UUID 扣 5：{deletes:?}"
    );
    assert!(
        deletes.contains(&(POISON_UID, 5)),
        "绿毒按装备 UUID 扣 5：{deletes:?}"
    );
    assert!(
        !deletes.iter().any(|(uid, _)| *uid == BAG_POISON_UID),
        "背包里同模板的绿毒不得被扣"
    );
    assert_eq!(
        equipment_count(&world, 1, AMULET_SLOT),
        Some((AMULET, 1)),
        "服务端：护身符 6→1 留在槽 6"
    );
    assert_eq!(
        equipment_count(&world, 1, POISON_SLOT),
        None,
        "服务端：绿毒 5→0 整堆耗光摘槽"
    );
    assert_eq!(
        inventory_count(&world, 1, 0),
        Some((GREEN_POISON, 5)),
        "服务端：背包同模板绿毒原样"
    );

    // 真实产包 → encode → decode → WorldSnapshot：装备部分扣数。
    apply_packets(&mut caster_snapshot, &cast_packets);
    assert_eq!(
        snapshot_count(&caster_snapshot.equipment, AMULET_UID),
        Some((AMULET, 1)),
        "客户端：装备护身符 6→1"
    );
    assert_eq!(
        snapshot_count(&caster_snapshot.equipment, POISON_UID),
        None,
        "客户端：装备绿毒耗尽摘槽"
    );
    assert_eq!(
        snapshot_count(&caster_snapshot.inventory, BAG_POISON_UID),
        Some((GREEN_POISON, 5)),
        "客户端：背包同模板绿毒不被误扣"
    );

    // 错误 UUID：把**同一批真实 DeleteItem 包**喂给 UUID 全错的快照，状态必须不变。
    let mut wrong_snapshot = WorldSnapshot::default();
    seed_self(&mut wrong_snapshot, 1, "玩家1", Point::new(20, 20), 20);
    seed_items(&mut wrong_snapshot, &world, 1); // 此时服务端已是 1/None/5 的**消耗后**状态
    if let Some(item) = wrong_snapshot
        .equipment
        .get_mut(AMULET_SLOT)
        .and_then(Option::as_mut)
    {
        item.unique_id = AMULET_UID + 1_000;
    }
    if let Some(item) = wrong_snapshot.inventory.get_mut(0).and_then(Option::as_mut) {
        item.unique_id = BAG_POISON_UID + 1_000;
    }
    let before_equipment = wrong_snapshot.equipment.clone();
    apply_packets(&mut wrong_snapshot, &cast_packets);
    assert_eq!(
        wrong_snapshot.equipment, before_equipment,
        "错误 UUID 的 DeleteItem 不得改动任何装备格"
    );

    // ---- M04/E1：落地只发一条 149，位置 = CastLocation（show-only） ----
    elapse(&mut world, 600);
    let landed = decode(frames(&mut caster_rx));
    let spells = object_spells(&landed);
    assert_eq!(
        spells.len(),
        1,
        "毒雾 3×3 只广播首个实际生成对象的一条 ObjectSpell，实际={spells:?}"
    );
    let (field_id, field_location, field_spell, param) = spells[0];
    assert_eq!(field_spell, Spell::PoisonCloud);
    assert_eq!(
        field_location,
        Point::new(22, 20),
        "149 的位置是 CastLocation（不是首格中心）"
    );
    assert!(!param, "服务端 param 恒 false");
    let visible = server_visible_fields(&world);
    assert_eq!(visible.len(), 1, "服务端只有首个场 show = true");
    assert_eq!(visible[0].object_id, field_id);
    assert_eq!(
        visible[0].cast_location,
        Point::new(22, 20),
        "服务端 cast_location 与 149 位置一致"
    );

    apply_packets(&mut caster_snapshot, &landed);
    assert_eq!(
        snapshot_field_ids(&caster_snapshot),
        [field_id].into_iter().collect::<BTreeSet<u32>>(),
        "客户端按对象 ID 登记持久场"
    );
    let registered = caster_snapshot.spell_field(field_id).expect("场已登记");
    assert_eq!(registered.spell, Spell::PoisonCloud);
    assert_eq!(registered.location, Point::new(22, 20));

    // ---- E1 负例：125 是 ObjectProjectile，不是撤销 ----
    // 服务端本作没有 125 生产者（见 00-summary.md 的 PARTIAL 记录），这里用协议负向
    // 断言钉死「收到 125 不得移除已登记场」；它**不是**场生命周期的放行证据。
    let projectile = ServerPacket::ObjectProjectile(ObjectProjectile {
        spell: Spell::PoisonCloud,
        source: 1,
        destination: 2,
    });
    let wire = projectile.encode_frame();
    let decoded = decode(vec![wire]);
    assert!(
        matches!(decoded.as_slice(), [ServerPacket::ObjectProjectile(_)]),
        "125 解码回 ObjectProjectile"
    );
    apply_packets(&mut caster_snapshot, &decoded);
    assert_eq!(
        snapshot_field_ids(&caster_snapshot),
        [field_id].into_iter().collect::<BTreeSet<u32>>(),
        "125 不是撤销：已登记场必须保留"
    );

    // ---- E3：玩家进入已有毒雾视野 → 服务端补发 149（可失败断言） ----
    let observer_id = world.allocate_object_id();
    let mut observer_rx = add_player(
        &mut world,
        observer_id,
        20,
        MirClass::Wizard,
        Point::new(24, 20),
    );
    let observer_entry = decode(frames(&mut observer_rx));
    let resends = object_spells(&observer_entry);
    assert_eq!(
        resends.len(),
        1,
        "新玩家进入已有毒雾视野必须补发一条 149，实际={resends:?}"
    );
    assert_eq!(resends[0].0, field_id, "补发的是同一个场对象 ID");
    assert_eq!(resends[0].1, Point::new(22, 20));
    let mut observer_snapshot = WorldSnapshot::default();
    seed_self(
        &mut observer_snapshot,
        observer_id,
        "玩家2",
        Point::new(24, 20),
        20,
    );
    apply_packets(&mut observer_snapshot, &observer_entry);
    assert_eq!(
        snapshot_field_ids(&observer_snapshot),
        [field_id].into_iter().collect::<BTreeSet<u32>>(),
        "客户端：进场补发的 149 也登记成同一块场"
    );

    // 视野内移动不重复补发（`field_views` 已记过该玩家）。
    world
        .walk(observer_id, MirDirection::Left)
        .expect("观察者可移动");
    let observer_walk = decode(frames(&mut observer_rx));
    assert!(
        object_spells(&observer_walk).is_empty(),
        "已见过的场不得在视野内移动时重复补发 149"
    );
    apply_packets(&mut observer_snapshot, &observer_walk);

    // ---- 26：到期撤销（严格 now > expire_at） ----
    elapse(&mut world, 6_001);
    let caster_removes = decode(frames(&mut caster_rx));
    assert_eq!(
        object_remove_ids(&caster_removes),
        vec![field_id],
        "毒雾到期只撤销 show 的那个场对象"
    );
    apply_packets(&mut caster_snapshot, &caster_removes);
    assert!(
        caster_snapshot.spell_fields.is_empty(),
        "客户端：收到 26 后同一对象 ID 的场被摘掉"
    );
    // 撤销对象 ID 与生成时一致：观察者同样按对象 ID 摘场。
    let observer_removes = decode(frames(&mut observer_rx));
    assert_eq!(object_remove_ids(&observer_removes), vec![field_id]);
    apply_packets(&mut observer_snapshot, &observer_removes);
    assert!(observer_snapshot.spell_fields.is_empty());

    // ---- 困魔咒：8 个外围场，同一套 149/26 生命周期 ----
    ready(&mut world, 1);
    learn(&mut world, 1, Spell::TrapHexagon, 1);
    give_equipment(&mut world, 1, AMULET_SLOT, AMULET, 2, TRAP_AMULET_UID);
    // 预检 3×2 内放训练木桩：中心格 (22,20)。
    let monster = world.spawn_monster(MAP, trainer(5_000), Point::new(22, 20));
    if let Some(monster) = world.monster_mut(monster) {
        monster.operate_time = -1;
        monster.regen_time = i64::MAX;
    }
    drain(&mut caster_rx);
    world
        .cast_magic(
            1,
            Spell::TrapHexagon,
            MirDirection::Right,
            0,
            Point::new(22, 20),
        )
        .expect("困魔咒施法成功（预检内确有目标）");
    let trap_cast = decode(frames(&mut caster_rx));
    assert_eq!(
        delete_items(&trap_cast),
        vec![(TRAP_AMULET_UID, 1)],
        "困魔咒只耗 1 枚护身符，一条精确 Count"
    );
    assert_eq!(
        equipment_count(&world, 1, AMULET_SLOT),
        Some((AMULET, 1)),
        "服务端：护身符 2→1"
    );
    elapse(&mut world, 500);
    let trap_landed = decode(frames(&mut caster_rx));
    let trap_spells = object_spells(&trap_landed);
    assert_eq!(trap_spells.len(), 8, "困魔咒 3×3 不含中心，共 8 个外围场");
    let trap_ids: BTreeSet<u32> = trap_spells.iter().map(|(id, _, _, _)| *id).collect();
    assert_eq!(trap_ids.len(), 8, "8 个场对象 ID 互不相同");
    let trap_locations: BTreeSet<(i32, i32)> = trap_spells
        .iter()
        .map(|(_, location, _, _)| (location.x, location.y))
        .collect();
    assert_eq!(
        trap_locations,
        expected_trap_centers(Point::new(22, 20)),
        "8 个外围格的坐标"
    );
    assert!(
        trap_spells
            .iter()
            .all(|(_, _, spell, _)| *spell == Spell::TrapHexagon),
        "全部是困魔咒场"
    );
    apply_packets(&mut caster_snapshot, &trap_landed);
    assert_eq!(
        snapshot_field_ids(&caster_snapshot),
        trap_ids,
        "客户端：8 个外围场全部登记"
    );
    // 到期撤销：Duration = (Level×5 + 10) × 1000 = 15000ms（落地时刻起算）。
    elapse(&mut world, 15_001);
    let trap_removes = decode(frames(&mut caster_rx));
    assert_eq!(
        object_remove_ids(&trap_removes).len(),
        8,
        "8 个场到期各撤销一条 26，实际={:?}",
        object_remove_ids(&trap_removes)
    );
    assert_eq!(
        object_remove_ids(&trap_removes)
            .into_iter()
            .collect::<BTreeSet<_>>(),
        trap_ids,
        "撤销的是同一批场对象 ID"
    );
    apply_packets(&mut caster_snapshot, &trap_removes);
    assert!(
        caster_snapshot.spell_fields.is_empty(),
        "客户端：困魔咒撤完清空"
    );

    // ---- 火墙负例：不走 field_views 差分链，进场不得补发 149 ----
    ready(&mut world, 1);
    learn(&mut world, 1, Spell::FireWall, 1);
    drain(&mut caster_rx);
    world
        .cast_magic(1, Spell::FireWall, MirDirection::Up, 0, Point::new(21, 22))
        .expect("火墙施法成功");
    drain(&mut caster_rx); // S.Magic 等
    elapse(&mut world, 500);
    let firewall_landed = decode(frames(&mut caster_rx));
    let firewall_spells = object_spells(&firewall_landed);
    assert_eq!(firewall_spells.len(), 5, "火墙 5 格逐格广播 149");
    assert!(
        firewall_spells
            .iter()
            .all(|(_, _, spell, _)| *spell == Spell::FireWall),
        "全部是火墙场"
    );
    apply_packets(&mut caster_snapshot, &firewall_landed);
    assert_eq!(caster_snapshot.spell_fields.len(), 5);
    let fire_observer_id = world.allocate_object_id();
    let mut fire_observer_rx = add_player(
        &mut world,
        fire_observer_id,
        20,
        MirClass::Wizard,
        Point::new(24, 22),
    );
    let fire_entry = decode(frames(&mut fire_observer_rx));
    assert!(
        object_spells(&fire_entry).is_empty(),
        "火墙不在 149/26 差分链里：新玩家进场不得补发（E3 负例）"
    );
    let mut fire_snapshot = WorldSnapshot::default();
    seed_self(
        &mut fire_snapshot,
        fire_observer_id,
        "玩家3",
        Point::new(24, 22),
        20,
    );
    apply_packets(&mut fire_snapshot, &fire_entry);
    assert!(
        fire_snapshot.spell_fields.is_empty(),
        "客户端：火墙补发为 0"
    );
}

// ---------------------------------------------------------------------------
// 2. 冲刺失败回纠：152 自机 / 153 远端，前置均为「错误预测格 + 非空既有动作/motion」
// ---------------------------------------------------------------------------

#[test]
fn round52_dash_fail_wrong_prediction_with_existing_motion() {
    let mut world = wire_world();
    let mut caster_rx = add_player(&mut world, 1, 20, MirClass::Warrior, Point::new(12, 29));
    let mut observer_rx = add_player(&mut world, 2, 20, MirClass::Warrior, Point::new(14, 29));
    learn(&mut world, 1, Spell::ShoulderDash, 3);
    ready(&mut world, 1);
    drain(&mut caster_rx);
    let observer_entry = decode(frames(&mut observer_rx));

    let mut caster_snapshot = WorldSnapshot::default();
    seed_self(&mut caster_snapshot, 1, "玩家1", Point::new(12, 29), 20);
    let mut observer_snapshot = WorldSnapshot::default();
    seed_self(&mut observer_snapshot, 2, "玩家2", Point::new(14, 29), 20);
    apply_packets(&mut observer_snapshot, &observer_entry);
    assert!(
        observer_snapshot.entities.contains_key(&1),
        "观察者快照里有施法者实体（真实 ObjectPlayer）"
    );

    // ---- 152：零步冲刺（面向 Down 已到地图下边），服务端只给本人发 UserDashFail ----
    world
        .cast_magic(
            1,
            Spell::ShoulderDash,
            MirDirection::Down,
            0,
            Point::default(),
        )
        .expect("零步冲刺仍返回 Ok（服务端发失败包）");
    let fail_packets = decode(frames(&mut caster_rx));
    let fail = user_dash_fail(&fail_packets).expect("服务端必须发 UserDashFail(152)");
    assert_eq!(fail.0, Point::new(12, 29), "152 回拉服务端权威格（原地）");
    assert_eq!(fail.1, MirDirection::Down);

    // 前置（生产入口，F2）：用真实 `S.UserLocation`(23) 权威包把自机从 (12,29) 走到
    // 错误预测格 (11,29)——`net::apply_self_location` 会把自机动作置 Walking 并给非 0
    // 起始时刻；再用生产判定 `camera::motion_after_authoritative_step` 得到非空
    // `SelfMotion`（不手塞字段）。
    let walk_packet = ServerPacket::UserLocation(UserLocation {
        location: Point::new(11, 29),
        direction: MirDirection::Left,
    });
    caster_snapshot.apply_packet_at(&walk_packet, 1_200);
    let mut self_motion = match camera::motion_after_authoritative_step(
        &caster_snapshot,
        Point::new(12, 29),
        Point::new(11, 29),
    ) {
        camera::StepMotion::Started(motion) => Some(motion),
        other => panic!("生产 camera 判定必须为权威一步产生 SelfMotion，实际={other:?}"),
    };
    assert!(self_motion.is_some(), "前置：自机 motion 非空");
    {
        let me = caster_snapshot.self_player.as_mut().expect("自机已注入");
        assert_eq!(me.position, Point::new(11, 29), "权威包把自机落到预测格");
        assert_eq!(me.action, ActorAction::Walking, "前置：既有非空移动动作");
        assert_ne!(me.action_started_ms, 0, "前置：动作有时钟基准");
    }
    let user_dash_fail_packet = fail_packets
        .iter()
        .find(|packet| matches!(packet, ServerPacket::UserDashFail(_)))
        .expect("UserDashFail 包");
    caster_snapshot.apply_packet_at(user_dash_fail_packet, 1_500);
    let me = caster_snapshot.self_player.as_ref().expect("自机");
    assert_eq!(
        me.position,
        Point::new(12, 29),
        "152 把错误预测格钉回权威格"
    );
    assert_eq!(me.direction, MirDirection::Down);
    assert_eq!(
        me.action,
        ActorAction::Standing,
        "152 必须清掉旧预测动作（不得按 1 格 = Walking 重启动画）"
    );
    assert_eq!(
        me.action_started_ms, 1_500,
        "Standing 的起始时刻 = 应用该包的时刻"
    );
    assert_eq!(
        caster_snapshot.wall_bump_id, 0,
        "152 不是撞墙回包，不得计撞墙"
    );

    // F2：生产渲染步骤 `camera::prepare_frame`。motion 起始 1200、本步 1500（仍在
    // 600ms 内），但自机动作已被 152 切成 Standing，所以必须走「显式打断」分支清掉
    // 自机 motion（Crystal 收到 `UserDashFail` 后清 `NextAction` 与动作队列）。
    let prepared = camera::prepare_frame(
        &mut caster_snapshot,
        &mut self_motion,
        Point::new(12, 29),
        1_500,
    );
    assert!(
        self_motion.is_none(),
        "152 后生产 camera 步骤必须清空自机 motion（Standing 显式打断插值）"
    );
    assert!(
        matches!(prepared.phase, camera::MotionPhase::Interrupted),
        "prepare_frame 的 motion 处置必须是 Interrupted，实际={:?}",
        prepared.phase
    );

    // ---- 153：远端观察者看到施法者先走一步，再零步冲刺失败 ----
    elapse(&mut world, 1_200);
    let walked = world.walk(1, MirDirection::Right).expect("施法者向右一步");
    assert_eq!(walked, Point::new(13, 29));
    let walk_packets = decode(frames(&mut observer_rx));
    apply_packets(&mut observer_snapshot, &walk_packets);
    assert!(
        observer_snapshot.remote_motions.contains_key(&1),
        "前置：观察者已有非空远端 motion（真实 ObjectWalk 产生）"
    );
    {
        let entity = observer_snapshot.entities.get(&1).expect("施法者实体");
        assert_eq!(entity.position, Point::new(13, 29));
        assert_eq!(entity.action, ActorAction::Walking);
    }

    elapse(&mut world, 1_200);
    world
        .cast_magic(
            1,
            Spell::ShoulderDash,
            MirDirection::Down,
            0,
            Point::default(),
        )
        .expect("第二次零步冲刺仍返回 Ok");
    let fail2_packets = decode(frames(&mut observer_rx));
    let dash_fail = object_dash_fail(&fail2_packets).expect("观察者必须收到 ObjectDashFail(153)");
    assert_eq!(dash_fail.0, 1, "153 带对象 ID");
    assert_eq!(dash_fail.1, Point::new(13, 29), "153 回拉服务端权威格");
    assert_eq!(dash_fail.2, MirDirection::Down);

    // 前置：把远端实体错画到 (13,28)（错误预测格），motion 仍在。
    {
        let entity = observer_snapshot.entities.get_mut(&1).expect("施法者实体");
        entity.position = Point::new(13, 28);
    }
    assert!(observer_snapshot.remote_motions.contains_key(&1));
    let object_dash_fail_packet = fail2_packets
        .iter()
        .find(|packet| matches!(packet, ServerPacket::ObjectDashFail(_)))
        .expect("ObjectDashFail 包");
    observer_snapshot.apply_packet_at(object_dash_fail_packet, 2_000);
    let entity = observer_snapshot
        .entities
        .get(&1)
        .expect("施法者实体仍存在");
    assert_eq!(
        entity.position,
        Point::new(13, 29),
        "153 把错误预测格钉回权威格"
    );
    assert_eq!(entity.direction, MirDirection::Down);
    assert_eq!(
        entity.action,
        ActorAction::Standing,
        "153 清掉 Walking 动作"
    );
    assert_eq!(entity.action_started_ms, 2_000);
    assert!(
        !observer_snapshot.remote_motions.contains_key(&1),
        "153 清掉远端 motion 队列"
    );
    assert!(
        !observer_snapshot.monster_motions.contains_key(&1),
        "153 也清怪物 motion 表里的同 ID 项（虽然该 ID 是玩家）"
    );
    assert_eq!(
        world.player(1).expect("施法者仍在").position,
        Point::new(13, 29),
        "权威位置与客户端一致"
    );
}
