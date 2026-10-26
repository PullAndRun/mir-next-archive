//! Round 63 B: 125 `S.ObjectProjectile` 全链 + R63 G1/G2 端到端集成测试。
//!
//! 覆盖（对应看板子单 `JJC-20261026-010-sub-1f51f226`，R63-B）：
//!
//! * **G1 严格命中时钟**：经真实 125 命中链生成 `magic.wzl[570]` 后，按 Crystal
//!   `Effect.cs:38-48,81-96` 的严格 `NextFrame` 时钟采样：`NextFrame = Start + 60*(frame+1)`、
//!   等号不进帧、越过才 +1、逐边界走到第 9 帧（579）、越过最后一个边界（Start+600）
//!   才 `Remove`；同刻重复 `sync` 不推进。
//!   **R63 G1 supersede**：R62 里按 `elapsed` 反推帧号、满 600 ms 直接删除的断言不再
//!   语义有效（严格时钟下大跳不追补、等号不进），已由本文件与三个历史文件改写。
//! * **G2 跨代缓存**：新会话首批快照里「可见但未被 spawn/advance 引用」的对象位置必须
//!   在 `reset_for_new_generation` 之后保留（否则同代后续 125 丢事件）；旧代号见过的
//!   位置不得跨代复活。
//!
//! 全链要求：`ObjectMonster`(seed/move) / `ObjectRemove`(移除可见对象) / `ObjectProjectile`(125)
//! 全部走真实 `encode_frame` → `decode_server_packet` → `WorldSnapshot::apply_packet_at`
//! → `UiState::sync`，不直接填私有缓存（`known_positions` 不可见）。
//!
//! 本文件是独立集成测试 crate（helper 无法跨文件复用），helper 与 R62
//! `crates/mir-client/tests/round62_projectile_wire.rs` 的最小副本一致：只复制
//! `[len u16][index i16][payload]` 真实帧布局与公开包处理器调用。

use mir_client::net::WorldSnapshot;
use mir_client::ui::UiState;
use mir_client::ui::projectile::{
    HIT_EFFECT_BASE_INDEX, HIT_EFFECT_DURATION_MS, HIT_EFFECT_FRAME_COUNT, HIT_EFFECT_FRAME_MS,
};
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{MirDirection, Monster, Spell};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{
    ObjectMonster, ObjectProjectile, ObjectRemove, decode_server_packet,
};

/// 施法者 object id。
const SOURCE_ID: u32 = 7;
/// 目标 object id。
const DEST_ID: u32 = 8;
/// 首批快照里可见、但从未被 spawn/advance 引用的无关对象（G2 关键）。
const UNRELATED_ID: u32 = 9;
/// 只出现在旧会话、新会话从未出现的对象（G2 负例）。
const OLD_GENERATION_ID: u32 = 99;
/// 测试统一使用的出生墙钟毫秒。
const START_MS: i64 = 1_000;
/// 出生音（`GameScene.cs:4697`，`GreatFireBall = 34`）。
const SPAWN_CUE: &str = "M34-1.wav";
/// 命中音（`GameScene.cs:4709`）。
const HIT_CUE: &str = "M34-2.wav";

/// 新 UI 状态（中文是默认语言；本文件不断言本地化文本）。
fn ui_state() -> UiState {
    UiState::new(Language::ZhCn)
}

/// 通过真实 `ObjectMonster` 包处理器写入/移动一个对象。
fn seed_monster(snapshot: &mut WorldSnapshot, object_id: u32, location: Point) {
    snapshot.apply_packet_at(
        &ServerPacket::ObjectMonster(ObjectMonster {
            object_id,
            name: format!("monster-{object_id}"),
            location,
            image: Monster::Skeleton,
            direction: MirDirection::Down,
            dead: false,
            ..ObjectMonster::default()
        }),
        0,
    );
    assert_eq!(
        snapshot.object_position(object_id),
        Some(location),
        "seed failed: monster {object_id} is not a known snapshot object"
    );
}

/// 通过真实 `ObjectRemove` 包处理器移除一个可见对象。
fn remove_object(snapshot: &mut WorldSnapshot, object_id: u32) {
    let packet = through_wire(&ServerPacket::ObjectRemove(ObjectRemove { object_id }));
    let ServerPacket::ObjectRemove(decoded) = &packet else {
        panic!("26 must decode into ServerPacket::ObjectRemove");
    };
    assert_eq!(decoded.object_id, object_id);
    snapshot.apply_packet_at(&packet, 0);
    assert_eq!(
        snapshot.object_position(object_id),
        None,
        "remove failed: object {object_id} is still visible"
    );
}

/// 把包编码成真实服务器帧，再走公开协议入口解码（帧布局：`[len u16][index i16][payload]`）。
fn through_wire(packet: &ServerPacket) -> ServerPacket {
    let frame = packet.encode_frame();
    assert!(
        frame.len() > 4,
        "encoded frame must carry a payload: {frame:?}"
    );
    let index = i16::from_le_bytes([frame[2], frame[3]]);
    decode_server_packet(index, &frame[4..]).expect("server frame must decode into ServerPacket")
}

/// 推入一条 125（真实 wire → snapshot 路径）。
fn push_projectile(snapshot: &mut WorldSnapshot, source: u32, destination: u32, now_ms: u64) {
    let packet = through_wire(&ServerPacket::ObjectProjectile(ObjectProjectile {
        spell: Spell::FireBounce,
        source,
        destination,
    }));
    let ServerPacket::ObjectProjectile(decoded) = &packet else {
        panic!("125 must decode into ServerPacket::ObjectProjectile");
    };
    assert_eq!(decoded.spell, Spell::FireBounce);
    assert_eq!(decoded.source, source);
    assert_eq!(decoded.destination, destination);
    snapshot.apply_packet_at(&packet, now_ms);
}

/// 推进真实快照路径到 `now_ms`（UI 时钟喂给 `projectiles.sync`）。
fn sync_at(ui: &mut UiState, snapshot: &WorldSnapshot, now_ms: i64) {
    ui.time_millis = now_ms;
    ui.sync(snapshot);
}

/// 严格 `NextFrame` 下越过第 `frame` 个边界的 tick 时刻：
/// 边界 = `start + 60*frame`，等号不进，所以越过边界的最小正 tick 是 `+1`。
fn boundary_tick(start: i64, frame: u32) -> i64 {
    let offset = i64::try_from(u64::from(frame) * HIT_EFFECT_FRAME_MS).expect("offset fits i64");
    start + offset + 1
}

/// G1：真实 125 → 命中链生成 570 → 严格时钟逐边界采样、等号不进、越界才 Remove。
#[test]
fn round63_g1_strict_hit_clock_boundaries() {
    // 源/目标同格：duration 0 / count 1 / interval 0，出生当刻等号不进，下一毫秒到达。
    let mut snapshot = WorldSnapshot::new_session();
    seed_monster(&mut snapshot, SOURCE_ID, Point::new(10, 10));
    seed_monster(&mut snapshot, DEST_ID, Point::new(10, 10));
    push_projectile(&mut snapshot, SOURCE_ID, DEST_ID, 0);

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().len(), 1, "零距离投射物出生");
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);
    sync_at(&mut ui, &snapshot, START_MS + 1);
    assert_eq!(
        ui.projectiles().len(),
        0,
        "零距离投射物在下一毫秒到达并删除"
    );
    assert_eq!(ui.take_projectile_sound_cues(), vec![HIT_CUE]);
    let hit_effects = ui.projectiles().hit_effects();
    assert_eq!(hit_effects.len(), 1, "到达时生成一个 570 命中特效");
    let start = hit_effects[0].started_ms;
    assert_eq!(start, START_MS + 1, "570 的 Start 是到达时刻");
    assert_eq!(hit_effects[0].target_id, DEST_ID);
    // 第 0 帧：Start 时刻画 570。
    assert_eq!(
        ui.projectiles().hit_effects()[0].sprite_index(start),
        HIT_EFFECT_BASE_INDEX,
        "Start 时刻是 570"
    );

    // 同刻重复 sync：不推进、不重复生成。
    sync_at(&mut ui, &snapshot, start);
    sync_at(&mut ui, &snapshot, start);
    assert_eq!(ui.projectiles().hit_effects().len(), 1);
    assert_eq!(
        ui.projectiles().hit_effects()[0].sprite_index(start),
        HIT_EFFECT_BASE_INDEX,
        "同刻重复 sync 不推进时钟"
    );

    // 等号不进帧：Start+60 仍停在第 0 帧（570），旧 elapsed 语义在这里会进到 571。
    let first_boundary = start + i64::try_from(HIT_EFFECT_FRAME_MS).expect("60 fits i64");
    sync_at(&mut ui, &snapshot, first_boundary);
    assert_eq!(
        ui.projectiles().hit_effects().len(),
        1,
        "等号不进帧、也不删除"
    );
    assert_eq!(
        ui.projectiles().hit_effects()[0].sprite_index(first_boundary),
        HIT_EFFECT_BASE_INDEX,
        "now == NextFrame：保持 570（等号不进）"
    );

    // 越过边界 +1 ms 才 +1 帧；同刻重复不推进。
    let tick = boundary_tick(start, 1);
    assert_eq!(tick, first_boundary + 1);
    sync_at(&mut ui, &snapshot, tick);
    assert_eq!(
        ui.projectiles().hit_effects()[0].sprite_index(tick),
        HIT_EFFECT_BASE_INDEX + 1,
        "越过第 1 个边界 → 571"
    );
    sync_at(&mut ui, &snapshot, tick);
    assert_eq!(
        ui.projectiles().hit_effects()[0].sprite_index(tick),
        HIT_EFFECT_BASE_INDEX + 1,
        "同刻重复 sync 不推进"
    );

    // 逐边界走到最后一帧 9（579）：每个边界只需要一次合法 Process。
    for frame in 2..HIT_EFFECT_FRAME_COUNT {
        let now = boundary_tick(start, frame);
        sync_at(&mut ui, &snapshot, now);
        assert_eq!(
            ui.projectiles().hit_effects().len(),
            1,
            "第 {frame} 帧期间特效仍在"
        );
        assert_eq!(
            ui.projectiles().hit_effects()[0].sprite_index(now),
            HIT_EFFECT_BASE_INDEX + frame,
            "第 {frame} 次越过边界 → 570+{frame}"
        );
    }
    let last_frame_at = boundary_tick(start, HIT_EFFECT_FRAME_COUNT - 1);
    assert_eq!(
        ui.projectiles().hit_effects()[0].sprite_index(last_frame_at),
        HIT_EFFECT_BASE_INDEX + HIT_EFFECT_FRAME_COUNT - 1,
        "最后一帧是 579"
    );

    // 最后一个边界（Start+600）等号不进：仍显示 579，不得提前删除。
    let last_boundary = start + i64::try_from(HIT_EFFECT_DURATION_MS).expect("600 ms fits i64");
    sync_at(&mut ui, &snapshot, last_boundary);
    assert_eq!(
        ui.projectiles().hit_effects().len(),
        1,
        "等号不进：Start+600 仍显示最后一帧"
    );
    assert_eq!(
        ui.projectiles().hit_effects()[0].sprite_index(last_boundary),
        HIT_EFFECT_BASE_INDEX + HIT_EFFECT_FRAME_COUNT - 1,
        "Start+600 仍显示 579"
    );

    // 越过最后一个边界 +1 ms 才 Remove；同刻只删一次。
    sync_at(&mut ui, &snapshot, last_boundary + 1);
    assert!(
        ui.projectiles().hit_effects().is_empty(),
        "越过第 10 个边界后 570 被 Remove"
    );
    sync_at(&mut ui, &snapshot, last_boundary + 1);
    assert!(ui.projectiles().hit_effects().is_empty());
    assert!(
        ui.take_projectile_sound_cues().is_empty(),
        "删除不得补播命中音"
    );
}

/// G2（M3 精确口径）：新代首批快照里未被 spawn/advance 引用的可见对象位置必须活过
/// generation reset，供下一同代 125 在对象缺席时兜底；该投射物完成不产生 570/命中 cue；
/// 对象重现不补播旧 125。
#[test]
fn round63_g2_first_payload_positions_survive_generation_reset() {
    let source = Point::new(10, 10);
    let target = Point::new(13, 10);
    let unrelated = Point::new(16, 10);
    let mut snapshot = WorldSnapshot::new_session();
    seed_monster(&mut snapshot, SOURCE_ID, source);
    seed_monster(&mut snapshot, DEST_ID, target);
    seed_monster(&mut snapshot, UNRELATED_ID, unrelated);
    // 首条 125 只引用 7/8：9 从未进入 spawn/advance 的源目标回填，只能来自可见快照。
    push_projectile(&mut snapshot, SOURCE_ID, DEST_ID, 0);
    assert_eq!(snapshot.projectile_events_total, 1);

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().events_consumed(), 1);
    assert_eq!(ui.projectiles().len(), 1);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);
    let first_batch_9 = snapshot
        .object_position(UNRELATED_ID)
        .expect("首批快照里 9 必须可见");
    assert_eq!(first_batch_9, unrelated);

    // 下一同代快照：真实 ObjectRemove 移除 9；确认 9 在 snapshot 缺席。
    remove_object(&mut snapshot, UNRELATED_ID);
    assert_eq!(snapshot.object_position(UNRELATED_ID), None);
    push_projectile(&mut snapshot, DEST_ID, UNRELATED_ID, 0);
    assert_eq!(snapshot.projectile_events_total, 2);

    // 新 125 打 9：相对该 125 之前恰新增 1 个投射物、恰 1 条出生 cue，落点=首批 9 位置。
    let spawned_before = ui.projectiles().len();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().events_consumed(), 2);
    assert_eq!(
        ui.projectiles().len(),
        spawned_before + 1,
        "相对该 125 前恰新增 1 个投射物（9 的首批位置必须活过 reset）"
    );
    assert_eq!(
        ui.take_projectile_sound_cues(),
        vec![SPAWN_CUE],
        "新 125 恰 1 条出生 cue"
    );
    let cached_view = ui.projectiles().views()[1];
    assert_eq!(cached_view.destination_id, UNRELATED_ID);
    assert_eq!(
        cached_view.destination, first_batch_9,
        "落点 = 首批快照里 9 的位置"
    );

    // 两枚投射物都到齐（三格：duration 150 / count 5 / interval 30，5 次严格 tick）。
    for frame in 1..5 {
        let now = START_MS + i64::from(frame) * 30 + 1;
        sync_at(&mut ui, &snapshot, now);
    }
    assert_eq!(ui.projectiles().views().len(), 2, "两枚都在飞");
    let arrival = START_MS + 5 * 30 + 1;
    sync_at(&mut ui, &snapshot, arrival);
    assert!(ui.projectiles().views().is_empty(), "两枚同时到达");
    // A（目标 8，存活）留下 1 个 570 + 1 条命中 cue；B（目标 9，已缺席）零命中表现。
    assert_eq!(
        ui.take_projectile_sound_cues(),
        vec![HIT_CUE],
        "命中 cue 只来自仍存活的目标 8"
    );
    let effects = ui.projectiles().hit_effects();
    assert_eq!(effects.len(), 1, "目标 8 的 570 是唯一命中表现");
    assert_eq!(
        effects
            .iter()
            .filter(|e| e.target_id == UNRELATED_ID)
            .count(),
        0,
        "目标 9 的缺席投射物完成后不得产生 570"
    );
    assert_eq!(
        effects.iter().filter(|e| e.target_id == DEST_ID).count(),
        1,
        "目标 8 的既有结果隔离计数"
    );

    // 9 随后重现：不补播旧 125（不新增投射物 / 出生 cue / 命中效果）。
    seed_monster(&mut snapshot, UNRELATED_ID, Point::new(20, 20));
    let consumed_before = ui.projectiles().events_consumed();
    let effects_before = ui.projectiles().hit_effects().len();
    sync_at(&mut ui, &snapshot, arrival + 10);
    assert_eq!(
        ui.projectiles().events_consumed(),
        consumed_before,
        "9 重现不得重放已消费的旧 125"
    );
    assert!(
        ui.projectiles().views().is_empty(),
        "9 重现不补播旧事件投射物"
    );
    assert!(
        ui.take_projectile_sound_cues().is_empty(),
        "9 重现不补出生 cue/命中 cue"
    );
    assert_eq!(
        ui.projectiles().hit_effects().len(),
        effects_before,
        "9 重现不补命中效果"
    );
}

/// G2 负例：旧代号见过的位置不得跨代复活（reset 必须清缓存）。
#[test]
fn round63_g2_generation_reset_drops_old_positions() {
    // 旧代：99 可见，但没有任何 125 事件。
    let mut old = WorldSnapshot::new_session();
    seed_monster(&mut old, OLD_GENERATION_ID, Point::new(5, 5));
    let mut ui = ui_state();
    sync_at(&mut ui, &old, START_MS);
    assert_eq!(
        old.object_position(OLD_GENERATION_ID),
        Some(Point::new(5, 5))
    );
    assert_eq!(
        ui.projectiles().events_consumed(),
        0,
        "旧代没有事件，只有可见位置进缓存"
    );

    // 新代：99 从未出现，新 125 打 99。
    let mut new_generation = WorldSnapshot::new_session();
    assert!(
        new_generation.session_generation > old.session_generation,
        "真实发号器必须给出新代号"
    );
    seed_monster(&mut new_generation, SOURCE_ID, Point::new(10, 10));
    assert_eq!(new_generation.object_position(OLD_GENERATION_ID), None);
    push_projectile(&mut new_generation, SOURCE_ID, OLD_GENERATION_ID, 0);

    let cues_before = ui.take_projectile_sound_cues().len();
    sync_at(&mut ui, &new_generation, START_MS + 10);
    assert_eq!(
        ui.projectiles().events_consumed(),
        1,
        "新代 125 已消费（不生成也算已消费）"
    );
    assert!(
        ui.projectiles().views().is_empty(),
        "跨代旧位置不得复活：零生成"
    );
    assert_eq!(
        ui.take_projectile_sound_cues().len(),
        cues_before,
        "零生成 → 零出生 cue"
    );
    assert!(
        ui.projectiles().hit_effects().is_empty(),
        "零生成 → 零命中表现"
    );
    // 再同步一次也不得延迟复活。
    sync_at(&mut ui, &new_generation, START_MS + 20);
    assert!(ui.projectiles().views().is_empty());
    assert!(ui.take_projectile_sound_cues().is_empty());
}
