//! Round 62 B: 125 `S.ObjectProjectile` 全链 + R62 五项语义的端到端集成测试。
//!
//! 覆盖（对应 `docs/process/round62-plan.md` 的 B 交付 / 门下补充裁决）：
//!
//! 1. 真实 125 包 → 真实 `decode_server_packet` → `WorldSnapshot` → `UiState::sync`：
//!    出生恰一次、严格 `NextFrame` 飞行、命中/570 残影表现；
//! 2. 同会话代号（`WorldSnapshot::session_generation`）下裁剪空表不复位；新代号首批
//!    非空事件复位恰一次，且旧 view/hit/cue 全部清空、目标重现不补播旧事件；
//! 3. `TargetPoint` 无缓存不生成：未知目标零实体、零命中、wire 事件表不改；目标重现后
//!    已消费事件不补播；目标消失后可信缓存仍可兜底落点；
//! 4. 每 tick 至多推进一帧：29 s 大跳不追补，边界逐个 tick 走完；
//! 5. 每个投射物出生/命中音（`M34-1`/`M34-2`）各恰一次。
//!
//! Wire 路径与 helper 来源：本文件是独立集成测试 crate，round59/round61 的测试 helper
//! 是各文件私有函数，故按 `crates/mir-client/tests/round61_projectile_queue.rs:64-162`
//! 最小复制（`ui_state`/`seed_monster`/`move_monster`/`through_wire`/`push_projectile`/
//! `sync_at`），只把 `seeded_snapshot` 改为走真实发号的 [`WorldSnapshot::new_session`]。
//!
//! R62 manifest 依据：`docs/process/round62-plan.md` C1/C2/C3/C4；本文件的英文测试名
//! 即 manifest 引用名。

use mir_client::net::WorldSnapshot;
use mir_client::ui::UiState;
use mir_client::ui::projectile::{
    HIT_EFFECT_BASE_INDEX, HIT_EFFECT_DURATION_MS, HIT_EFFECT_FRAME_COUNT, HIT_EFFECT_FRAME_MS,
    duration_ms, frame_count,
};
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{MirDirection, Monster, Spell};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{ObjectMonster, ObjectProjectile, decode_server_packet};

/// 施法者 object id（合成快照）。
const SOURCE_ID: u32 = 7;
/// 目标 object id（合成快照）。
const DEST_ID: u32 = 8;
/// 从未在快照/缓存里出现过的目标 id（C2：无坐标不生成）。
const UNKNOWN_ID: u32 = 99;
/// 测试统一使用的出生墙钟毫秒。
const START_MS: i64 = 1_000;
/// 出生音（`GameScene.cs:4697`，`GreatFireBall = 34`）。
const SPAWN_CUE: &str = "M34-1.wav";
/// 命中音（`GameScene.cs:4709`）。
const HIT_CUE: &str = "M34-2.wav";
/// 五格飞行的到达 tick：严格边界 31 ms 的 8 个 tick，最后落在 249 ms。
const FIVE_CELL_TICKS: [i64; 8] = [32, 63, 94, 125, 156, 187, 218, 249];
/// 两格飞行的到达 tick：严格边界 33 ms 的 3 个 tick，最后落在 100 ms。
const TWO_CELL_TICKS: [i64; 3] = [34, 67, 100];

/// 新 UI 状态（中文是默认语言；本文件不断言本地化文本）。
fn ui_state() -> UiState {
    UiState::new(Language::ZhCn)
}

/// 通过真实 `ObjectMonster` 包处理器写入一个对象。
fn seed_monster(snapshot: &mut WorldSnapshot, object_id: u32, location: Point, dead: bool) {
    snapshot.apply_packet_at(
        &ServerPacket::ObjectMonster(ObjectMonster {
            object_id,
            name: format!("monster-{object_id}"),
            location,
            image: Monster::Skeleton,
            direction: MirDirection::Down,
            dead,
            ..ObjectMonster::default()
        }),
        0,
    );
    assert!(
        snapshot.object_position(object_id).is_some(),
        "seed failed: monster {object_id} is not a known snapshot object"
    );
}

/// 通过同一条包路径移动已存在的对象（实体 upsert）。
fn move_monster(snapshot: &mut WorldSnapshot, object_id: u32, location: Point, dead: bool) {
    snapshot.apply_packet_at(
        &ServerPacket::ObjectMonster(ObjectMonster {
            object_id,
            name: format!("monster-{object_id}"),
            location,
            image: Monster::Skeleton,
            direction: MirDirection::Down,
            dead,
            ..ObjectMonster::default()
        }),
        0,
    );
    assert_eq!(
        snapshot.object_position(object_id),
        Some(location),
        "move failed: monster {object_id} did not move to {location:?}"
    );
}

/// 新会话快照（真实发号器；`WorldSnapshot::default()` 的 0 只留给手工表兼容路径）。
fn seeded_snapshot(source: Point, destination: Point) -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::new_session();
    seed_monster(&mut snapshot, SOURCE_ID, source, false);
    seed_monster(&mut snapshot, DEST_ID, destination, false);
    snapshot
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
    snapshot.apply_packet_at(&packet, now_ms);
}

/// 推进真实快照路径到 `now_ms`（UI 时钟喂给 `projectiles.sync`）。
fn sync_at(ui: &mut UiState, snapshot: &WorldSnapshot, now_ms: i64) {
    ui.time_millis = now_ms;
    ui.sync(snapshot);
}

/// 五格（250 ms / count 8 / interval 31 ms）严格 `NextFrame` 飞行：每个 tick 只过一格边界。
fn settle_five_cell_flight(ui: &mut UiState, snapshot: &WorldSnapshot) {
    for elapsed in FIVE_CELL_TICKS {
        sync_at(ui, snapshot, START_MS + elapsed);
    }
}

/// 两格（100 ms / count 3 / interval 33 ms）严格 `NextFrame` 飞行。
fn settle_two_cell_flight(ui: &mut UiState, snapshot: &WorldSnapshot) {
    for elapsed in TWO_CELL_TICKS {
        sync_at(ui, snapshot, START_MS + elapsed);
    }
}

/// ① 真实 125 全链：出生恰一次 → 严格飞行 → 命中 570 残影跟随目标并播满 600 ms。
#[test]
fn real_125_spawns_once_flies_hits_and_keeps_the_impact_ghost() {
    let source = Point::new(10, 10);
    let target = Point::new(12, 10);
    let mut snapshot = seeded_snapshot(source, target);

    // 真实 wire：编码 → 索引/载荷解码，事件结构与 C# `S.ObjectProjectile` 一致。
    let packet = through_wire(&ServerPacket::ObjectProjectile(ObjectProjectile {
        spell: Spell::FireBounce,
        source: SOURCE_ID,
        destination: DEST_ID,
    }));
    let ServerPacket::ObjectProjectile(decoded) = &packet else {
        panic!("125 must decode into ServerPacket::ObjectProjectile");
    };
    assert_eq!(decoded.spell, Spell::FireBounce);
    assert_eq!(decoded.source, SOURCE_ID);
    assert_eq!(decoded.destination, DEST_ID);
    snapshot.apply_packet_at(&packet, 0);
    assert_eq!(snapshot.projectile_events.len(), 1);
    assert_eq!(snapshot.projectile_events_total, 1);

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().events_consumed(), 1, "spawn exactly once");
    assert_eq!(ui.projectiles().len(), 1);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);

    let duration = duration_ms(source, target);
    assert_eq!(duration, 100);
    assert_eq!(frame_count(duration), 3);
    let view = ui.projectiles().views()[0];
    assert_eq!(view.spell, Spell::FireBounce);
    assert_eq!(view.source, source);
    assert_eq!(view.destination_id, DEST_ID);
    assert_eq!(view.destination, target);
    assert_eq!(view.started_ms, START_MS);
    assert_eq!(view.frame, 0);
    assert_eq!(view.frame_count, 3);
    assert_eq!(view.next_frame_ms, START_MS + 33);

    // 严格 NextFrame：每个 tick 一帧，等号/同刻重复不变。
    sync_at(&mut ui, &snapshot, START_MS + 34);
    assert_eq!(ui.projectiles().views()[0].frame, 1);
    assert_eq!(ui.projectiles().views()[0].next_frame_ms, START_MS + 66);
    sync_at(&mut ui, &snapshot, START_MS + 34);
    assert_eq!(ui.projectiles().views()[0].frame, 1, "same instant no-op");
    sync_at(&mut ui, &snapshot, START_MS + 67);
    assert_eq!(ui.projectiles().views()[0].frame, 2);
    sync_at(&mut ui, &snapshot, START_MS + 99);
    assert_eq!(
        ui.projectiles().views()[0].frame,
        2,
        "next boundary is 99 ms"
    );

    // 命中：删除投射物、目标存活才有 570 残影 + 命中音（各恰一次）。
    let arrival = START_MS + 100;
    sync_at(&mut ui, &snapshot, arrival);
    assert!(ui.projectiles().is_empty());
    assert_eq!(ui.take_projectile_sound_cues(), vec![HIT_CUE]);
    let effects = ui.projectiles().hit_effects();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].target_id, DEST_ID);
    assert_eq!(effects[0].position, target);
    assert_eq!(effects[0].started_ms, arrival);
    assert_eq!(
        effects[0].sprite_index(arrival),
        HIT_EFFECT_BASE_INDEX,
        "570 is the first impact frame"
    );
    sync_at(&mut ui, &snapshot, arrival);
    assert!(
        ui.take_projectile_sound_cues().is_empty(),
        "a repeated arrival sync must not replay the hit cue"
    );

    // 残影跟随目标当前位置（`Effect.Draw` 的 Owner 语义）并播满 600 ms。
    let moved = Point::new(13, 10);
    move_monster(&mut snapshot, DEST_ID, moved, false);
    sync_at(&mut ui, &snapshot, arrival + 10);
    assert_eq!(ui.projectiles().hit_effects()[0].position, moved);
    for step in 0..HIT_EFFECT_FRAME_COUNT {
        let now = arrival
            + i64::try_from(u64::from(step) * HIT_EFFECT_FRAME_MS).expect("frame offset fits i64");
        sync_at(&mut ui, &snapshot, now);
        let index = ui.projectiles().hit_effects()[0].sprite_index(now);
        assert_eq!(
            index,
            HIT_EFFECT_BASE_INDEX + step,
            "impact frame {step} must be 570 + {step}"
        );
    }
    sync_at(&mut ui, &snapshot, arrival + 599);
    assert_eq!(
        ui.projectiles().hit_effects().len(),
        1,
        "the ghost is still visible one millisecond before 600 ms"
    );
    sync_at(
        &mut ui,
        &snapshot,
        arrival + i64::try_from(HIT_EFFECT_DURATION_MS).expect("600 ms fits i64"),
    );
    assert!(
        ui.projectiles().hit_effects().is_empty(),
        "the 570 ghost must be removed at exactly {HIT_EFFECT_DURATION_MS} ms"
    );
}

/// ② 同代号裁剪空表不复位；新代号首批非空恰复位一次，旧 view/hit/cue 清空、不补播。
#[test]
fn trimmed_empty_same_generation_keeps_state_and_new_generation_resets_once() {
    let source = Point::new(10, 10);
    let target = Point::new(12, 10);
    let mut first = seeded_snapshot(source, target);
    let mut ui = ui_state();

    // 旧会话：一条投射物飞完并留下 570 残影 + 命中音（不清空，用于观察新代清空）。
    push_projectile(&mut first, SOURCE_ID, DEST_ID, 0);
    sync_at(&mut ui, &first, START_MS);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);
    settle_two_cell_flight(&mut ui, &first);
    assert!(ui.projectiles().is_empty());
    assert_eq!(ui.projectiles().hit_effects().len(), 1);
    assert_eq!(
        ui.projectiles().sound_cues(),
        [HIT_CUE],
        "the hit cue stays queued until drained"
    );

    // 同代号裁剪空表：事件表被清空（total 保留），游标/在飞状态/残影都不得复位。
    first.projectile_events.clear();
    assert_eq!(first.projectile_events_total, 1);
    sync_at(&mut ui, &first, START_MS + 100);
    assert_eq!(
        ui.projectiles().events_consumed(),
        1,
        "a trimmed empty table of the same generation must not rewind the cursor"
    );
    assert_eq!(
        ui.projectiles().hit_effects().len(),
        1,
        "no reset in between"
    );

    // 同代号事件表重新长回来（真实 125 推入两条）：从绝对游标继续消费，旧的已消费事件不重放。
    push_projectile(&mut first, SOURCE_ID, DEST_ID, 0);
    push_projectile(&mut first, SOURCE_ID, DEST_ID, 0);
    assert_eq!(first.projectile_events_total, 3);
    sync_at(&mut ui, &first, START_MS + 101);
    assert_eq!(ui.projectiles().events_consumed(), 3);
    assert_eq!(ui.projectiles().len(), 2, "only the two new events spawn");
    assert_eq!(
        ui.projectiles().sound_cues(),
        [HIT_CUE, SPAWN_CUE, SPAWN_CUE],
        "old hit cue is kept; the two new spawns append once each"
    );

    // 新会话（真实发号器给新代号）：首批空快照只推迟复位，旧状态原样保留。
    let mut second = seeded_snapshot(source, target);
    assert!(
        second.session_generation > first.session_generation,
        "the issuer must hand out a fresh generation per session"
    );
    sync_at(&mut ui, &second, START_MS + 101);
    assert_eq!(
        ui.projectiles().events_consumed(),
        3,
        "an empty first batch of a new generation must not reset yet"
    );
    assert_eq!(ui.projectiles().len(), 2);
    assert_eq!(ui.projectiles().hit_effects().len(), 1);
    assert_eq!(ui.projectiles().sound_cues().len(), 3);

    // 新代号首批非空事件：恰复位一次（清旧 view/hit/cue、游标归零），只消费新事件。
    push_projectile(&mut second, SOURCE_ID, DEST_ID, 0);
    sync_at(&mut ui, &second, START_MS + 102);
    assert_eq!(
        ui.projectiles().events_consumed(),
        1,
        "the first non-empty batch of the new generation resets and re-consumes"
    );
    assert_eq!(ui.projectiles().len(), 1, "old in-flight views are cleared");
    assert!(
        ui.projectiles().hit_effects().is_empty(),
        "old 570 ghosts are cleared by the session reset"
    );
    assert_eq!(
        ui.take_projectile_sound_cues(),
        vec![SPAWN_CUE],
        "old cues are dropped, only the new session's spawn cue survives"
    );
    let view = ui.projectiles().views()[0];
    assert_eq!(view.source, source);
    assert_eq!(view.destination, target);
    assert_eq!(view.started_ms, START_MS + 102);

    // 复位恰好一次：继续消费同一代的第二条事件时不再复位。
    push_projectile(&mut second, SOURCE_ID, DEST_ID, 0);
    sync_at(&mut ui, &second, START_MS + 103);
    assert_eq!(ui.projectiles().events_consumed(), 2);
    assert_eq!(
        ui.projectiles().len(),
        2,
        "a second event appends; it must not clear and rebuild the first view"
    );
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);
    sync_at(&mut ui, &second, START_MS + 103);
    assert_eq!(ui.projectiles().events_consumed(), 2);
    assert!(ui.take_projectile_sound_cues().is_empty());
}

/// ③ `TargetPoint` 无缓存不生成；目标重现不补播；消失后缓存兜底落点；wire 事件表不改。
#[test]
fn unknown_target_never_spawns_and_a_reappearing_target_does_not_replay() {
    let mut snapshot = WorldSnapshot::new_session();
    let source = Point::new(10, 10);
    seed_monster(&mut snapshot, SOURCE_ID, source, false);
    let mut ui = ui_state();

    // 未知目标：125 已消费，但无坐标 → 零实体、零命中、零音效，wire 事件表原样保留。
    push_projectile(&mut snapshot, SOURCE_ID, UNKNOWN_ID, 0);
    assert_eq!(snapshot.projectile_events.len(), 1);
    assert_eq!(snapshot.projectile_events_total, 1);
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().events_consumed(), 1);
    assert_eq!(
        ui.projectiles().len(),
        0,
        "an unknown target must not create a projectile entity"
    );
    assert!(ui.projectiles().hit_effects().is_empty());
    assert!(ui.take_projectile_sound_cues().is_empty());
    assert_eq!(
        snapshot.projectile_events.len(),
        1,
        "the UI must never rewrite the wire event table"
    );

    // 目标重现：已消费的旧事件不补播（新代号/同代号都不重放）。
    let target = Point::new(12, 10);
    seed_monster(&mut snapshot, UNKNOWN_ID, target, false);
    sync_at(&mut ui, &snapshot, START_MS + 10);
    assert_eq!(ui.projectiles().events_consumed(), 1);
    assert_eq!(
        ui.projectiles().len(),
        0,
        "no replay after the target appears"
    );
    assert!(ui.take_projectile_sound_cues().is_empty());

    // 目标已知后的新 125 正常出生。
    push_projectile(&mut snapshot, SOURCE_ID, UNKNOWN_ID, 0);
    sync_at(&mut ui, &snapshot, START_MS + 20);
    assert_eq!(ui.projectiles().events_consumed(), 2);
    assert_eq!(ui.projectiles().len(), 1);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);

    // 目标从快照消失：125 落点退回可信缓存（最后一次见到的位置），仍生成实体。
    snapshot.entities.remove(&UNKNOWN_ID);
    push_projectile(&mut snapshot, SOURCE_ID, UNKNOWN_ID, 0);
    sync_at(&mut ui, &snapshot, START_MS + 30);
    assert_eq!(ui.projectiles().events_consumed(), 3);
    assert_eq!(
        ui.projectiles().len(),
        2,
        "the known_positions cache is used"
    );
    let cached = ui.projectiles().views()[1];
    assert_eq!(cached.destination, target);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);

    // 既不在快照、也不在缓存的目标：不生成、不造坐标、不加事件、不播出生音。
    push_projectile(&mut snapshot, SOURCE_ID, UNKNOWN_ID + 1, 0);
    sync_at(&mut ui, &snapshot, START_MS + 40);
    assert_eq!(ui.projectiles().events_consumed(), 4);
    assert_eq!(
        ui.projectiles().len(),
        2,
        "still no entity for a lost target"
    );
    assert!(ui.take_projectile_sound_cues().is_empty());
    assert_eq!(snapshot.projectile_events.len(), 4);
    assert_eq!(snapshot.projectile_events_total, 4);
}

/// ④ 每 tick 至多推进一帧：29 s 大跳只前进一帧，边界逐个 tick 走完。
#[test]
fn a_29_second_jump_advances_at_most_one_frame_per_tick() {
    let source = Point::new(10, 10);
    let target = Point::new(15, 10);
    let mut snapshot = seeded_snapshot(source, target);
    push_projectile(&mut snapshot, SOURCE_ID, DEST_ID, 0);

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().views()[0].frame, 0);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);

    // 29 s 大跳：只过一格边界（非追补），仍然在飞。
    let jump = i64::try_from(29_000_u64).expect("29 s fits i64");
    sync_at(&mut ui, &snapshot, START_MS + jump);
    let view = ui.projectiles().views()[0];
    assert_eq!(view.frame, 1, "one tick advances at most one frame");
    assert_eq!(view.frame_count, 8);
    assert!(!view.finished());
    assert_eq!(
        view.next_frame_ms,
        START_MS + 62,
        "the next boundary is recomputed from the current frame"
    );
    assert!(
        ui.projectiles().hit_effects().is_empty(),
        "a time jump must not settle the flight"
    );

    // 之后每 tick 恰好一帧，连续 7 个 tick 才走完剩下的 7 个边界。
    for (offset, frame) in [(1_i64, 2_u32), (2, 3), (3, 4), (4, 5), (5, 6), (6, 7)] {
        sync_at(&mut ui, &snapshot, START_MS + jump + offset);
        assert_eq!(
            ui.projectiles().views()[0].frame,
            frame,
            "the tick at jump+{offset} ms crosses exactly one boundary"
        );
    }
    assert!(!ui.projectiles().views()[0].finished());
    sync_at(&mut ui, &snapshot, START_MS + jump + 7);
    assert!(
        ui.projectiles().is_empty(),
        "arrival only after the eighth tick"
    );
    assert_eq!(ui.take_projectile_sound_cues(), vec![HIT_CUE]);
    assert_eq!(ui.projectiles().hit_effects().len(), 1);
}

/// ⑤ 每个投射物的出生/命中音各恰一次（含同刻重复 sync 不重播）。
#[test]
fn each_projectile_plays_its_spawn_and_hit_cue_exactly_once() {
    let source = Point::new(10, 10);
    let target = Point::new(15, 10);
    let mut snapshot = seeded_snapshot(source, target);
    // 同一快照两条 125：两个独立投射物。
    push_projectile(&mut snapshot, SOURCE_ID, DEST_ID, 0);
    push_projectile(&mut snapshot, SOURCE_ID, DEST_ID, 0);

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().len(), 2);
    assert_eq!(
        ui.take_projectile_sound_cues(),
        vec![SPAWN_CUE, SPAWN_CUE],
        "one spawn cue per projectile"
    );
    sync_at(&mut ui, &snapshot, START_MS);
    assert!(ui.take_projectile_sound_cues().is_empty());

    // 飞行途中两枚同步推进。
    sync_at(&mut ui, &snapshot, START_MS + 32);
    assert_eq!(ui.projectiles().views()[0].frame, 1);
    assert_eq!(ui.projectiles().views()[1].frame, 1);

    // 到达：两枚各一次命中表现与命中音。
    settle_five_cell_flight(&mut ui, &snapshot);
    assert!(ui.projectiles().is_empty());
    assert_eq!(ui.take_projectile_sound_cues(), vec![HIT_CUE, HIT_CUE]);
    assert_eq!(ui.projectiles().hit_effects().len(), 2);
    sync_at(&mut ui, &snapshot, START_MS + 249);
    assert!(
        ui.take_projectile_sound_cues().is_empty(),
        "same-instant arrival sync must not duplicate cues"
    );
    assert_eq!(ui.projectiles().hit_effects().len(), 2);

    // 残影到期后只剩音效证据，不重复生成。
    sync_at(
        &mut ui,
        &snapshot,
        START_MS + 249 + i64::try_from(HIT_EFFECT_DURATION_MS).expect("600 ms fits i64"),
    );
    assert!(ui.projectiles().hit_effects().is_empty());
    assert!(ui.take_projectile_sound_cues().is_empty());
}
