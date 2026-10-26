//! Round 61 D1: `S.ObjectProjectile`(125) backlog with no loss, plus the Crystal dynamic
//! flight parameters, driven through the real packet/UI path.
//!
//! Contract sources:
//!
//! * `docs/process/round61-plan.md` G1 (A: 125 events and their cues are not lost to the
//!   old 16/32 entry caps) and G2 (B: recompute `duration`/`count` every frame before the
//!   `finished` decision);
//! * `docs/process/round61-menxia-review.md` M1 (17/33/257 through the real push path, both
//!   "backlog then consume" and "consume then burst", no replay, hit settles once, empty
//!   rebuild and manual `total = 0` compatibility) and M2 (same-instant far/near move,
//!   dead/missing target, frame and `finished` boundaries);
//! * `docs/process/round61-shangshu-dispatch.md` §二 D1.
//!
//! Wire path: `ServerPacket::ObjectProjectile` -> `encode_frame` -> `decode_server_packet`
//! -> `WorldSnapshot::apply_packet_at` -> `WorldSnapshot::projectile_events` ->
//! `UiState::sync` (absolute cursor) -> `UiState::projectiles` /
//! `UiState::take_projectile_sound_cues`.
//!
//! Crystal anchors:
//!
//! * `Client/MirObjects/Effect.cs:174-181` (`Missile.Process`): `Destination` is refreshed
//!   from the target and `Duration = MaxDistance(Source, Destination) * 50`,
//!   `Count = Duration / Interval` (with `Count == 0 -> 1`) are recomputed every frame;
//! * `Effect.cs:55-60`: `NextFrame = Start + (Duration / Count) * (CurrentFrame + 1)`
//!   (C# integer division), and `Effect.cs:91` returns while
//!   `CMain.Time <= NextFrame`: equality does not advance a frame. Therefore
//!   `frame = (elapsed - 1) / interval` and the projectile finishes only once the last
//!   boundary is strictly passed;
//! * `Client/MirScenes/GameScene.cs:4687-4715`: every accepted 125 creates one projectile
//!   and plays `M34-1.wav`; a live target gets the 570 impact and `M34-2.wav`; there is no
//!   ring, no cap and no dedup on the client.
//!
//! The tests intentionally use the production `RecordingSoundSink` (no fake sink): the
//! cue queue handed to the audio channel is part of the G1 contract, not just a test hook.
//!
//! R62 supersedes the R61 elapsed semantics: the production path now walks strict
//! `NextFrame` boundaries (`advance_clock`: one frame per tick, equality returns, no
//! catch-up after a jump) and resets the cursor exactly once on the first non-empty batch of
//! a new `WorldSnapshot::session_generation`. The seven gates rewritten in round 62 keep
//! their R61 names and every original assertion; only the clock driving changed.

use mir_client::net::{ProjectileEvent, WorldSnapshot};
use mir_client::ui::UiState;
use mir_client::ui::projectile::{HIT_EFFECT_DURATION_MS, duration_ms, frame_count};
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{MirDirection, Monster, Spell};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{ObjectMonster, ObjectProjectile, decode_server_packet};

/// Object id of the caster in the synthetic snapshot.
const SOURCE_ID: u32 = 7;
/// Object id of the target in the synthetic snapshot.
const DEST_ID: u32 = 8;
/// Wall-clock millisecond used as the spawn instant by the tests.
const START_MS: i64 = 1_000;
/// Spawn cue from `GameScene.cs:4697` (`GreatFireBall = 34`).
const SPAWN_CUE: &str = "M34-1.wav";
/// Impact cue from `GameScene.cs:4709`.
const HIT_CUE: &str = "M34-2.wav";

/// A fresh UI state (Chinese is the shipped default language; no localized text is asserted).
fn ui_state() -> UiState {
    UiState::new(Language::ZhCn)
}

/// Seed one monster object through the real `ObjectMonster` packet handler.
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

/// Move an already seeded monster through the same packet path (entity upsert).
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

/// Snapshot with a caster at `source` and a live target at `destination`.
fn seeded_snapshot(source: Point, destination: Point) -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    seed_monster(&mut snapshot, SOURCE_ID, source, false);
    seed_monster(&mut snapshot, DEST_ID, destination, false);
    snapshot
}

/// Encode a packet into a real server frame, then decode it through the public protocol
/// entry point exactly like the net thread does (frame layout: `[len u16][index i16][payload]`).
fn through_wire(packet: &ServerPacket) -> ServerPacket {
    let frame = packet.encode_frame();
    assert!(
        frame.len() > 4,
        "encoded frame must carry a payload: {frame:?}"
    );
    let index = i16::from_le_bytes([frame[2], frame[3]]);
    decode_server_packet(index, &frame[4..]).expect("server frame must decode into ServerPacket")
}

/// Push one `S.ObjectProjectile`(125) through the real wire -> snapshot path.
fn push_projectile(snapshot: &mut WorldSnapshot, source: u32, destination: u32, now_ms: u64) {
    let packet = through_wire(&ServerPacket::ObjectProjectile(ObjectProjectile {
        spell: Spell::FireBounce,
        source,
        destination,
    }));
    snapshot.apply_packet_at(&packet, now_ms);
}

/// Advance the real snapshot path to `now_ms` (the UI clock feeds `projectiles.sync`).
fn sync_at(ui: &mut UiState, snapshot: &WorldSnapshot, now_ms: i64) {
    ui.time_millis = now_ms;
    ui.sync(snapshot);
}

/// R62 C1: strict `NextFrame` clock, one frame per `sync`. Five cells (duration 250 ms /
/// count 8 / interval 31 ms) needs eight ticks: each tick passes exactly one boundary and
/// the eighth removes the view. The last tick still lands on `START_MS + 249` (the old
/// elapsed arrival instant), so the impact-window assertions below keep their numbers.
fn settle_five_cell_flight(ui: &mut UiState, snapshot: &WorldSnapshot) {
    for elapsed in [32_i64, 63, 94, 125, 156, 187, 218, 249] {
        sync_at(ui, snapshot, START_MS + elapsed);
    }
}

/// R62 C1: two cells (duration 100 ms / count 3 / interval 33 ms) needs three ticks; the
/// last tick still lands on `START_MS + 100`.
fn settle_two_cell_flight(ui: &mut UiState, snapshot: &WorldSnapshot) {
    for elapsed in [34_i64, 67, 100] {
        sync_at(ui, snapshot, START_MS + elapsed);
    }
}

/// How the event backlog reaches the UI cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BacklogMode {
    /// All packets arrive before the first UI sync (worst-case backlog).
    BacklogThenConsume,
    /// The UI consumes after every packet (no backlog, but no drop either).
    ConsumeThenBurst,
}

/// M1 gate for one `n` and one arrival mode: every event becomes exactly one view and one
/// spawn cue, later every live arrival settles exactly one hit and one impact cue.
///
/// R62 C1: the arrival is driven by strict `NextFrame` ticks (`settle_five_cell_flight`),
/// not by one elapsed jump.
fn run_projectile_backlog_gate(n: usize, mode: BacklogMode) {
    let source = Point::new(10, 10);
    // Five cells right: duration 250 ms, count 8, next-frame interval 31 ms
    // (`Effect.cs:174-181` + integer division), arrival at elapsed 249 ms.
    let target = Point::new(15, 10);
    let mut snapshot = seeded_snapshot(source, target);
    let mut ui = ui_state();

    match mode {
        BacklogMode::BacklogThenConsume => {
            for _ in 0..n {
                push_projectile(&mut snapshot, SOURCE_ID, DEST_ID, 0);
            }
            sync_at(&mut ui, &snapshot, START_MS);
        }
        BacklogMode::ConsumeThenBurst => {
            for _ in 0..n {
                push_projectile(&mut snapshot, SOURCE_ID, DEST_ID, 0);
                sync_at(&mut ui, &snapshot, START_MS);
            }
        }
    }

    // Memory accounting (not RSS): report the real Vec lengths and capacities, then only
    // require "no truncation" (capacity >= len). The exact growth is allocator policy.
    let event_capacity = snapshot.projectile_events.capacity();
    let views_capacity = ui.projectiles().views_capacity();
    let cue_capacity = ui.projectiles().sound_cue_capacity();
    println!(
        "R61 M1 {mode:?} n={n}: saved={} total={} consumed={} views={} cues={} capacity(events={event_capacity}, views={views_capacity}, cues={cue_capacity})",
        snapshot.projectile_events.len(),
        snapshot.projectile_events_total,
        ui.projectiles().events_consumed(),
        ui.projectiles().len(),
        ui.projectiles().sound_cues().len(),
    );

    // Saved = input: the snapshot table is not truncated anymore (old 16-entry ring).
    let expected_total = u64::try_from(n).expect("test event count fits u64");
    assert_eq!(
        snapshot.projectile_events.len(),
        n,
        "every pushed event must stay in the snapshot table"
    );
    assert_eq!(
        snapshot.projectile_events_total, expected_total,
        "the absolute counter must count every pushed event"
    );
    assert!(
        event_capacity >= n,
        "event Vec capacity {event_capacity} must cover {n} saved events"
    );
    assert_eq!(
        ui.projectiles().events_consumed(),
        n,
        "every saved event must be consumed exactly once"
    );
    assert_eq!(
        ui.projectiles().len(),
        n,
        "every consumed event must spawn exactly one view"
    );
    assert_eq!(
        ui.projectiles().sound_cues().len(),
        n,
        "every spawn must queue exactly one spawn cue (old 32-entry cue cap)"
    );
    assert!(
        views_capacity >= n,
        "views Vec capacity {views_capacity} must cover {n} views"
    );
    assert!(
        cue_capacity >= n,
        "cue Vec capacity {cue_capacity} must cover {n} cues"
    );
    assert!(
        ui.projectiles()
            .sound_cues()
            .iter()
            .all(|cue| *cue == SPAWN_CUE),
        "spawn cues must be M34-1.wav"
    );
    for view in ui.projectiles().views() {
        assert_eq!(view.source, source, "spawn keeps the source cell");
        assert_eq!(
            view.destination_id, DEST_ID,
            "spawn keeps the destination id"
        );
        assert_eq!(
            view.destination, target,
            "spawn resolves the destination cell"
        );
        assert_eq!(
            view.started_ms, START_MS,
            "spawn stamps the first sync time"
        );
        assert_eq!(view.frame, 0, "nothing has flown at the spawn instant");
        assert_eq!(view.duration_ms, duration_ms(source, target));
        assert_eq!(view.frame_count, frame_count(duration_ms(source, target)));
    }

    // Drain the spawn cues, then replay the same snapshot at the same instant: the cursor
    // must not re-consume events and the views must not be re-stamped.
    let spawn_cues = ui.take_projectile_sound_cues();
    assert_eq!(spawn_cues, vec![SPAWN_CUE; n], "spawn cue per event");
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(
        ui.projectiles().events_consumed(),
        n,
        "a repeated sync must not re-consume events"
    );
    assert_eq!(
        ui.projectiles().len(),
        n,
        "a repeated sync must not spawn new views"
    );
    assert!(
        ui.take_projectile_sound_cues().is_empty(),
        "a repeated sync must not replay cues"
    );

    // Arrival (R62 C1): five cells is duration 250 ms / count 8 / interval 31 ms, and
    // `Effect.cs:91` requires `Time > NextFrame`; one `sync` advances at most one frame, so
    // the eight strict boundaries are walked one tick at a time. The last tick still lands on
    // `START_MS + 249`, so the old arrival instant and the impact window are unchanged.
    let arrival = START_MS + 249;
    settle_five_cell_flight(&mut ui, &snapshot);
    assert!(
        ui.projectiles().is_empty(),
        "every projectile must finish at the recomputed arrival instant"
    );
    assert_eq!(
        ui.projectiles().hit_effects().len(),
        n,
        "every live arrival must settle exactly one impact"
    );
    let hit_cues = ui.take_projectile_sound_cues();
    assert_eq!(hit_cues, vec![HIT_CUE; n], "impact cue per settled hit");

    // Second arrival sync at the same instant: nothing may settle twice.
    sync_at(&mut ui, &snapshot, arrival);
    assert!(
        ui.take_projectile_sound_cues().is_empty(),
        "a repeated arrival sync must not replay the impact cue"
    );
    assert_eq!(
        ui.projectiles().hit_effects().len(),
        n,
        "a repeated arrival sync must not spawn a second impact"
    );

    // The impact walks 600 ms (`GameScene.cs:4708`) and then disappears.
    let impact_end = arrival + i64::try_from(HIT_EFFECT_DURATION_MS).expect("600 ms fits i64");
    sync_at(&mut ui, &snapshot, impact_end);
    assert!(
        ui.projectiles().hit_effects().is_empty(),
        "impacts must expire after {HIT_EFFECT_DURATION_MS} ms"
    );
}

// R62 supersedes R61 semantics（见 round62 manifest）.
#[test]
fn backlog_then_consume_keeps_every_projectile_event() {
    for n in [17usize, 33, 257] {
        run_projectile_backlog_gate(n, BacklogMode::BacklogThenConsume);
    }
}

// R62 supersedes R61 semantics（见 round62 manifest）.
#[test]
fn consume_then_burst_keeps_every_projectile_event() {
    for n in [17usize, 33, 257] {
        run_projectile_backlog_gate(n, BacklogMode::ConsumeThenBurst);
    }
}

#[test]
fn manually_built_event_table_with_zero_total_is_consumed() {
    // R60 compatibility: a manually built snapshot only carries `projectile_events`
    // (total stays 0). `dropped = total - len` saturates to 0, so all events are read
    // from index 0 and none of them counts as dropped.
    let mut snapshot = seeded_snapshot(Point::new(10, 10), Point::new(15, 10));
    for _ in 0..17 {
        snapshot.projectile_events.push(ProjectileEvent {
            spell: Spell::FireBounce,
            source: SOURCE_ID,
            destination: DEST_ID,
        });
    }
    assert_eq!(
        snapshot.projectile_events_total, 0,
        "a manual table must not advance the absolute counter"
    );

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().events_consumed(), 17);
    assert_eq!(ui.projectiles().len(), 17);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE; 17]);

    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().events_consumed(), 17);
    assert!(ui.take_projectile_sound_cues().is_empty());
}

// R62 supersedes R61 semantics（见 round62 manifest）：空表首批只推迟复位，
// 首批非空事件才复位并从头消费；reset 恰好发生一次。
#[test]
fn empty_rebuilt_snapshot_resets_the_cursor_without_replay() {
    let source = Point::new(10, 10);
    // Two cells right: duration 100 ms, count 3, interval 33 ms.
    let target = Point::new(12, 10);
    let mut snapshot = seeded_snapshot(source, target);
    push_projectile(&mut snapshot, SOURCE_ID, DEST_ID, 0);

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().events_consumed(), 1);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);
    settle_two_cell_flight(&mut ui, &snapshot);
    assert!(ui.projectiles().is_empty());
    assert_eq!(ui.take_projectile_sound_cues(), vec![HIT_CUE]);

    // Reconnect: a brand-new snapshot gets its own issued session generation (R62 C4) and
    // an empty event table. The first empty batch must NOT rewind the cursor.
    let mut rebuilt = WorldSnapshot::new_session();
    seed_monster(&mut rebuilt, SOURCE_ID, source, false);
    seed_monster(&mut rebuilt, DEST_ID, target, false);
    assert_eq!(rebuilt.projectile_events_total, 0);
    assert_ne!(
        rebuilt.session_generation, snapshot.session_generation,
        "the rebuilt snapshot must carry a new session generation"
    );
    sync_at(&mut ui, &rebuilt, START_MS + 1_000);
    assert_eq!(
        ui.projectiles().events_consumed(),
        1,
        "the first empty snapshot of a new generation must not reset the cursor yet"
    );
    assert!(ui.projectiles().is_empty());
    assert!(
        ui.take_projectile_sound_cues().is_empty(),
        "the rebuild must not replay the old cues"
    );

    // The first non-empty batch of the new generation resets exactly once, then its first
    // event is consumed exactly once (not replayed).
    push_projectile(&mut rebuilt, SOURCE_ID, DEST_ID, 0);
    sync_at(&mut ui, &rebuilt, START_MS + 1_000);
    assert_eq!(
        ui.projectiles().events_consumed(),
        1,
        "the first non-empty batch resets the cursor and consumes its first event"
    );
    assert_eq!(ui.projectiles().len(), 1);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);
    sync_at(&mut ui, &rebuilt, START_MS + 1_000);
    assert_eq!(ui.projectiles().events_consumed(), 1);
    assert!(ui.take_projectile_sound_cues().is_empty());
}

#[test]
fn shorter_manual_table_keeps_the_guarded_rebuild() {
    // The R60 F6 guard (`consumed > len && dropped == 0 -> consumed = 0`) still describes a
    // rebuilt snapshot that already carries events; the trigger is narrowed by M1 because a
    // healthy no-drop table always has `len >= consumed`.
    let source = Point::new(10, 10);
    let target = Point::new(15, 10);
    let mut snapshot = seeded_snapshot(source, target);
    for _ in 0..3 {
        push_projectile(&mut snapshot, SOURCE_ID, DEST_ID, 0);
    }
    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().events_consumed(), 3);
    assert_eq!(ui.projectiles().len(), 3);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE; 3]);

    // A new snapshot whose table is shorter than the old cursor, with no dropped events.
    let mut rebuilt = seeded_snapshot(source, target);
    for _ in 0..2 {
        rebuilt.projectile_events.push(ProjectileEvent {
            spell: Spell::FireBounce,
            source: SOURCE_ID,
            destination: DEST_ID,
        });
    }
    sync_at(&mut ui, &rebuilt, START_MS + 10);
    assert_eq!(
        ui.projectiles().events_consumed(),
        2,
        "the guarded rebuild re-reads the new table from zero"
    );
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE; 2]);
    sync_at(&mut ui, &rebuilt, START_MS + 10);
    assert_eq!(ui.projectiles().events_consumed(), 2);
    assert!(ui.take_projectile_sound_cues().is_empty());
}

// ---------------------------------------------------------------------------
// G2: dynamic flight parameters (`Missile.Process`, Effect.cs:174-190)
// ---------------------------------------------------------------------------

// R62 supersedes R61 semantics（见 round62 manifest）.
#[test]
fn dynamic_flight_parameters_follow_the_target_until_finished() {
    let source = Point::new(10, 10);
    // Six cells right: duration 300 ms, count 10, interval 30 ms.
    let mut snapshot = seeded_snapshot(source, Point::new(16, 10));
    push_projectile(&mut snapshot, SOURCE_ID, DEST_ID, 0);

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    let view = ui.projectiles().views()[0];
    assert_eq!(view.duration_ms, 300);
    assert_eq!(view.frame_count, 10);
    assert_eq!(view.dir16, 4, "spawn samples direction 4 (right)");
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);

    // R62 C1: the strict `NextFrame` clock (interval 30 ms) needs one tick per boundary.
    // Six ticks reach frame 6 at +181 ms; the sync at the old +200 ms instant no longer
    // advances (the next boundary is 1210 ms), so the old assertion time/value still hold.
    for elapsed in [31_i64, 61, 91, 121, 151, 181] {
        sync_at(&mut ui, &snapshot, START_MS + elapsed);
    }
    sync_at(&mut ui, &snapshot, START_MS + 200);
    let view = ui.projectiles().views()[0];
    assert_eq!(view.frame, 6);
    assert_eq!(view.duration_ms, 300);
    assert_eq!(ui.take_projectile_sound_cues(), vec![] as Vec<&str>);

    // Target walks far: nine cells gives duration 450 ms / count 15. The projectile keeps
    // flying with the same frame index and keeps its spawn-time source/start/direction.
    move_monster(&mut snapshot, DEST_ID, Point::new(19, 16), false);
    sync_at(&mut ui, &snapshot, START_MS + 200);
    let view = ui.projectiles().views()[0];
    assert_eq!(view.destination, Point::new(19, 16));
    assert_eq!(view.duration_ms, 450);
    assert_eq!(view.frame_count, 15);
    assert_eq!(view.frame, 6);
    assert_eq!(view.started_ms, START_MS);
    assert_eq!(view.dir16, 4, "direction keeps the spawn-time sample");
    assert!(!view.finished(), "the far move extends the flight");

    // The frozen-parameter deadline (300 ms) is no longer an arrival: one tick per
    // boundary reaches frame 13 at +391 ms, and the sync at the old +400 ms instant stays
    // on frame 13 (its next boundary is 1420 ms).
    for elapsed in [211_i64, 241, 271, 301, 331, 361, 391] {
        sync_at(&mut ui, &snapshot, START_MS + elapsed);
    }
    sync_at(&mut ui, &snapshot, START_MS + 400);
    let view = ui.projectiles().views()[0];
    assert_eq!(
        view.frame, 13,
        "frame follows the recomputed 450/15 interval"
    );
    assert!(
        !view.finished(),
        "the far move must not finish at the old deadline"
    );
    assert!(ui.take_projectile_sound_cues().is_empty());

    // Target walks near: three cells gives duration 150 ms / count 5. R62 C1 keeps the view
    // until the next tick passes the still-armed NextFrame (1420 ms), then it settles.
    move_monster(&mut snapshot, DEST_ID, Point::new(13, 10), false);
    sync_at(&mut ui, &snapshot, START_MS + 400);
    assert_eq!(
        ui.projectiles().len(),
        1,
        "the shrink is settled by the next tick, not by the same-instant sync"
    );
    assert!(ui.projectiles().views()[0].finished());
    sync_at(&mut ui, &snapshot, START_MS + 421);
    assert!(ui.projectiles().is_empty());
    assert_eq!(ui.take_projectile_sound_cues(), vec![HIT_CUE]);
    let effects = ui.projectiles().hit_effects();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].target_id, DEST_ID);
    assert_eq!(effects[0].position, Point::new(13, 10));

    // No second settlement on a repeated sync at the same instant.
    sync_at(&mut ui, &snapshot, START_MS + 421);
    assert!(ui.take_projectile_sound_cues().is_empty());
    assert_eq!(ui.projectiles().hit_effects().len(), 1);
}

#[test]
fn zero_distance_projectile_finishes_on_the_first_tick() {
    // `Count == 0 -> 1` with `Duration == 0`: `NextFrame == Start`, so the first sync at
    // `Start` still returns (equality) and any later tick removes it.
    let mut snapshot = seeded_snapshot(Point::new(10, 10), Point::new(10, 10));
    push_projectile(&mut snapshot, SOURCE_ID, DEST_ID, 0);

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    let view = ui.projectiles().views()[0];
    assert_eq!(view.duration_ms, 0);
    assert_eq!(view.frame_count, 1);
    assert_eq!(view.frame, 0);
    assert!(!view.finished(), "time <= NextFrame must not finish");
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);

    sync_at(&mut ui, &snapshot, START_MS + 1);
    assert!(ui.projectiles().is_empty());
    assert_eq!(ui.take_projectile_sound_cues(), vec![HIT_CUE]);
    assert_eq!(ui.projectiles().hit_effects().len(), 1);
}

#[test]
fn one_grid_flight_uses_a_fifty_millisecond_step() {
    // One cell: duration 50 ms, count 1, interval 50 ms; `Effect.cs:91` requires a strictly
    // greater clock, so 50 ms is not yet the arrival.
    let mut snapshot = seeded_snapshot(Point::new(10, 10), Point::new(11, 10));
    push_projectile(&mut snapshot, SOURCE_ID, DEST_ID, 0);

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().views()[0].duration_ms, 50);
    assert_eq!(ui.projectiles().views()[0].frame_count, 1);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);

    sync_at(&mut ui, &snapshot, START_MS + 50);
    assert_eq!(
        ui.projectiles().views()[0].frame,
        0,
        "time == NextFrame must not advance the frame"
    );
    assert_eq!(ui.take_projectile_sound_cues(), vec![] as Vec<&str>);

    sync_at(&mut ui, &snapshot, START_MS + 51);
    assert!(ui.projectiles().is_empty());
    assert_eq!(ui.take_projectile_sound_cues(), vec![HIT_CUE]);
}

// R62 supersedes R61 semantics（见 round62 manifest）：严格 NextFrame 每 tick 至多一帧，
// 31 ms 间隔的 8 个边界必须逐个 tick 走完，不能在 +155 ms 一次跳到 frame 4。
#[test]
fn five_grid_frame_boundaries_match_strict_next_frame() {
    // Five cells: duration 250 ms, count 8, interval `250 / 8 = 31` (C# integer division),
    // so the k-th boundary is `31 * (k + 1)` ms; equality returns and arrival happens only
    // after the last boundary (249 ms).
    let mut snapshot = seeded_snapshot(Point::new(10, 10), Point::new(15, 10));
    push_projectile(&mut snapshot, SOURCE_ID, DEST_ID, 0);

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    let view = ui.projectiles().views()[0];
    assert_eq!(view.duration_ms, 250);
    assert_eq!(view.frame_count, 8);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);

    sync_at(&mut ui, &snapshot, START_MS + 31);
    assert_eq!(ui.projectiles().views()[0].frame, 0, "equality returns");
    for (elapsed, frame) in [
        (32_i64, 1_u32),
        (63, 2),
        (94, 3),
        (125, 4),
        (156, 5),
        (187, 6),
        (218, 7),
    ] {
        sync_at(&mut ui, &snapshot, START_MS + elapsed);
        assert_eq!(
            ui.projectiles().views()[0].frame,
            frame,
            "the +{elapsed} ms tick crosses exactly one boundary"
        );
    }
    // NextFrame is 1248 ms: equality does not finish, arrival is only after it.
    sync_at(&mut ui, &snapshot, START_MS + 248);
    assert_eq!(ui.projectiles().views()[0].frame, 7);
    assert!(!ui.projectiles().views()[0].finished());

    sync_at(&mut ui, &snapshot, START_MS + 249);
    assert!(
        ui.projectiles().is_empty(),
        "arrival happens only after the last boundary"
    );
    assert_eq!(ui.take_projectile_sound_cues(), vec![HIT_CUE]);
    assert_eq!(ui.projectiles().hit_effects().len(), 1);
}

// R62 supersedes R61 semantics（见 round62 manifest）.
#[test]
fn arrival_settles_exactly_once_across_repeated_syncs() {
    // Two cells: duration 100 ms, count 3, interval 33 ms, arrival at elapsed 100 ms.
    let mut snapshot = seeded_snapshot(Point::new(10, 10), Point::new(12, 10));
    push_projectile(&mut snapshot, SOURCE_ID, DEST_ID, 0);

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);

    let arrival = START_MS + 100;
    settle_two_cell_flight(&mut ui, &snapshot);
    assert!(ui.projectiles().is_empty());
    assert_eq!(ui.take_projectile_sound_cues(), vec![HIT_CUE]);
    assert_eq!(ui.projectiles().hit_effects().len(), 1);

    sync_at(&mut ui, &snapshot, arrival);
    assert!(ui.take_projectile_sound_cues().is_empty());
    assert_eq!(ui.projectiles().hit_effects().len(), 1);

    // Still inside the 600 ms impact window: nothing may be re-settled.
    sync_at(&mut ui, &snapshot, arrival + 60);
    assert!(ui.take_projectile_sound_cues().is_empty());
    assert_eq!(ui.projectiles().hit_effects().len(), 1);

    sync_at(&mut ui, &snapshot, arrival + 600);
    assert!(ui.projectiles().hit_effects().is_empty());
    assert!(ui.take_projectile_sound_cues().is_empty());
}

// R62 supersedes R61 semantics（见 round62 manifest）.
#[test]
fn dead_or_missing_targets_finish_without_impact_or_hit_cue() {
    // Dead target: `GameScene.cs:4707` returns before the 570 effect and the hit cue.
    let mut snapshot = seeded_snapshot(Point::new(10, 10), Point::new(12, 10));
    move_monster(&mut snapshot, DEST_ID, Point::new(12, 10), true);
    push_projectile(&mut snapshot, SOURCE_ID, DEST_ID, 0);

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().len(), 1, "a dead target still gets a view");
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);

    settle_two_cell_flight(&mut ui, &snapshot);
    assert!(ui.projectiles().is_empty());
    assert!(ui.projectiles().hit_effects().is_empty());
    assert!(
        ui.take_projectile_sound_cues().is_empty(),
        "a dead target must not play the hit cue"
    );

    // Missing target: the projectile keeps its last known destination and settles nothing.
    let mut snapshot = seeded_snapshot(Point::new(10, 10), Point::new(12, 10));
    push_projectile(&mut snapshot, SOURCE_ID, DEST_ID, 0);
    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    snapshot.entities.remove(&DEST_ID);
    settle_two_cell_flight(&mut ui, &snapshot);
    assert!(ui.projectiles().is_empty());
    assert!(ui.projectiles().hit_effects().is_empty());
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);
}
