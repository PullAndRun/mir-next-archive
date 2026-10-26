//! Round 59 D4: projectile render-path gates driven through the real `UiState` snapshot path.
//!
//! These gates exercise the public client path instead of the in-crate unit tests of
//! `crates/mir-client/src/ui/projectile.rs`:
//!
//! * G1 event consumption: `WorldSnapshot::projectile_events` -> [`UiState::sync`] ->
//!   `UiState::projectiles()`, cursor based, one event spawns at most one view and the
//!   cursor never replays an event (D-R59-1 drops unknown endpoints without inventing
//!   coordinates);
//! * G2 interpolation: `duration = MaxDistance * 50`, `count = max(1, duration / 30)`,
//!   `frame_at` boundaries, `sprite_index = 410 + frame % 6 + dir16 * 10` inside
//!   `magic.wzl[410..=565]`, `Direction16` sampled against Crystal
//!   `GameScene.cs:12044-12065`, `draw_offset` lerp from source to destination;
//! * G3 impact: when the destination object is still alive at arrival, removing the view
//!   produces the `magic.wzl[570..=579]` ten-frame sequence (60 ms per frame, 600 ms);
//!   a dead destination produces no impact and no hit sound;
//! * G4 sound: `projectile_sound` uses the `GreatFireBall = 34` base, so the spawn cue is
//!   `M34-1.wav` and the impact cue is `M34-2.wav` (never `M154-*`), played once each
//!   through the injectable [`ProjectileSoundSink`].
//!
//! No gate is skipped: the projectiles module is always compiled into `mir-client`, so a
//! missing feature must fail loudly instead of silently passing.

use mir_client::net::{ProjectileEvent, WorldSnapshot};
use mir_client::ui::UiState;
use mir_client::ui::projectile::{
    BASE_INDEX, DIRECTIONS, FRAME_COUNT, HIT_EFFECT_BASE_INDEX, HIT_EFFECT_DURATION_MS,
    HIT_EFFECT_FRAME_COUNT, HIT_EFFECT_FRAME_MS, ProjectileSoundSink, direction16, draw_offset,
    duration_ms, frame_at, frame_count, frame_interval_ms, hit_effect_index, max_distance,
    sprite_index,
};
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{MirDirection, Monster, Spell};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::ObjectMonster;
use mir_render::sound::{
    PROJECTILE_SOUND_BASE, PROJECTILE_SOUND_HIT_INDEX, PROJECTILE_SOUND_SPAWN_INDEX,
    ProjectileSoundPhase, projectile_sound,
};

/// Object id of the caster in the synthetic snapshot.
const SOURCE_ID: u32 = 7;
/// Object id of the target in the synthetic snapshot.
const DEST_ID: u32 = 8;
/// Wall-clock millisecond used as the spawn instant by the tests.
const START_MS: i64 = 1_000;

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
        "seed failed: monster {object_id} did not move to {location:?}"
    );
}

/// Snapshot with a caster at `source` and a live target at `destination`.
fn seeded_snapshot(source: Point, destination: Point) -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    seed_monster(&mut snapshot, SOURCE_ID, source, false);
    seed_monster(&mut snapshot, DEST_ID, destination, false);
    snapshot
}

/// One `S.ObjectProjectile`(125) event carrying `Spell::FireBounce`.
fn fire_bounce(source: u32, destination: u32) -> ProjectileEvent {
    ProjectileEvent {
        spell: Spell::FireBounce,
        source,
        destination,
    }
}

/// Advance the real snapshot path to `now_ms` (the UI clock feeds `projectiles.sync`).
fn sync_at(ui: &mut UiState, snapshot: &WorldSnapshot, now_ms: i64) {
    ui.time_millis = now_ms;
    ui.sync(snapshot);
}

/// Fake sink injected through `Projectiles::sync_with_sink`; records literal file names.
#[derive(Debug, Default)]
struct FakeSink {
    played: Vec<&'static str>,
}

impl ProjectileSoundSink for FakeSink {
    fn play(&mut self, file: &'static str) {
        self.played.push(file);
    }
}

// ---------------------------------------------------------------------------
// G1: event consumption
// ---------------------------------------------------------------------------

#[test]
fn known_event_spawns_exactly_one_view_once() {
    let source = Point::new(10, 10);
    let target = Point::new(15, 10);
    let mut snapshot = seeded_snapshot(source, target);
    snapshot
        .projectile_events
        .push(fire_bounce(SOURCE_ID, DEST_ID));

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);

    assert_eq!(ui.projectiles().events_consumed(), 1);
    assert_eq!(ui.projectiles().len(), 1);
    let views = ui.projectiles().views();
    assert_eq!(views.len(), 1);
    let view = views[0];
    assert_eq!(view.spell, Spell::FireBounce);
    assert_eq!(view.source, source);
    assert_eq!(view.destination_id, DEST_ID);
    assert_eq!(view.destination, target);
    assert_eq!(view.started_ms, START_MS);
    assert_eq!(view.frame, 0);

    // Same snapshot and same clock: the cursor must not replay the event.
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().events_consumed(), 1);
    assert_eq!(ui.projectiles().len(), 1);
    assert_eq!(ui.projectiles().views()[0], view);
}

#[test]
fn two_events_in_one_snapshot_spawn_two_views_in_arrival_order() {
    let mut snapshot = seeded_snapshot(Point::new(10, 10), Point::new(15, 10));
    seed_monster(&mut snapshot, 9, Point::new(10, 12), false);
    snapshot
        .projectile_events
        .push(fire_bounce(SOURCE_ID, DEST_ID));
    snapshot.projectile_events.push(fire_bounce(SOURCE_ID, 9));

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);

    assert_eq!(ui.projectiles().events_consumed(), 2);
    assert_eq!(ui.projectiles().len(), 2);
    let views = ui.projectiles().views();
    assert_eq!(views[0].destination_id, DEST_ID);
    assert_eq!(views[1].destination_id, 9);
    assert_eq!(
        views[1].dir16,
        direction16(Point::new(10, 10), Point::new(10, 12))
    );
}

#[test]
fn unknown_source_event_is_consumed_without_spawning_a_view() {
    let mut snapshot = seeded_snapshot(Point::new(10, 10), Point::new(15, 10));
    snapshot.projectile_events.push(fire_bounce(999, DEST_ID));

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);

    assert_eq!(ui.projectiles().events_consumed(), 1);
    assert!(
        ui.projectiles().is_empty(),
        "unknown source must not spawn a view"
    );
    assert!(ui.projectiles().sound_cues().is_empty());
}

#[test]
fn unknown_destination_event_is_consumed_without_spawning_a_view() {
    let mut snapshot = seeded_snapshot(Point::new(10, 10), Point::new(15, 10));
    snapshot.projectile_events.push(fire_bounce(SOURCE_ID, 999));

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);

    // Registered deviation D-R59-1: no destination coordinates means no projectile, and
    // Crystal parity is honestly broken here instead of faking a landing point.
    assert_eq!(ui.projectiles().events_consumed(), 1);
    assert!(
        ui.projectiles().is_empty(),
        "unknown destination must not spawn a view"
    );
    assert!(
        ui.projectiles().sound_cues().is_empty(),
        "the spawn cue belongs to the projectile, so it must not play without one"
    );
}

#[test]
fn cursor_resets_when_the_snapshot_event_table_is_replaced() {
    let mut first = seeded_snapshot(Point::new(10, 10), Point::new(15, 10));
    first
        .projectile_events
        .push(fire_bounce(SOURCE_ID, DEST_ID));
    let mut ui = ui_state();
    sync_at(&mut ui, &first, START_MS);
    assert_eq!(ui.projectiles().events_consumed(), 1);

    // A rebuilt snapshot starts with an empty event table: the cursor rewinds to zero.
    let mut second = seeded_snapshot(Point::new(10, 10), Point::new(15, 10));
    sync_at(&mut ui, &second, START_MS + 10);
    assert_eq!(ui.projectiles().events_consumed(), 0);

    second
        .projectile_events
        .push(fire_bounce(SOURCE_ID, DEST_ID));
    sync_at(&mut ui, &second, START_MS + 20);
    assert_eq!(ui.projectiles().events_consumed(), 1);
    assert_eq!(ui.projectiles().len(), 2);
}

// ---------------------------------------------------------------------------
// G2: interpolation formulas and direction sampling
// ---------------------------------------------------------------------------

#[test]
fn flight_duration_count_and_frame_boundaries_match_effect_cs() {
    let source = Point::new(10, 10);
    let target = Point::new(15, 10);
    assert_eq!(max_distance(source, target), 5);
    assert_eq!(duration_ms(source, target), 250);
    assert_eq!(frame_count(250), 8);
    assert_eq!(frame_count(0), 1);

    // R61 M2: strict `CMain.Time <= NextFrame` (Effect.cs:91) with the integer frame
    // interval `Duration / Count`, so the k-th boundary passes only once elapsed > k * 31.
    assert_eq!(frame_interval_ms(250, 8), 31);
    assert_eq!(frame_at(0, 250, 8), 0);
    assert_eq!(frame_at(31, 250, 8), 0, "equality at a boundary returns");
    assert_eq!(frame_at(32, 250, 8), 1);
    assert_eq!(frame_at(150, 250, 8), 4, "(150 - 1) / 31");
    assert_eq!(frame_at(248, 250, 8), 7);
    assert_eq!(frame_at(249, 250, 8), 8, "arrival after the last boundary");
    assert_eq!(frame_at(30_000, 250, 8), 8, "frame clamps to count");
    // Zero distance: `Count == 0 -> 1` (Effect.cs:178-181), interval 0, first tick.
    assert_eq!(frame_interval_ms(0, 1), 0);
    assert_eq!(frame_at(0, 0, 1), 0);
    assert_eq!(frame_at(1, 0, 1), 1);

    // The real snapshot path spawns with the initial destination and recomputes the
    // dynamic parameters on every sync (R61 M2).
    let mut snapshot = seeded_snapshot(source, target);
    snapshot
        .projectile_events
        .push(fire_bounce(SOURCE_ID, DEST_ID));
    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    let view = ui.projectiles().views()[0];
    assert_eq!(view.duration_ms, duration_ms(source, target));
    assert_eq!(view.frame_count, frame_count(view.duration_ms));

    sync_at(&mut ui, &snapshot, START_MS + 150);
    let view = ui.projectiles().views()[0];
    assert_eq!(view.frame, 4);
    assert_eq!(view.sprite_index(), BASE_INDEX + 4 + 4 * 10);

    // Arrival deletes the view (frame == count) after the last strict boundary at +249 ms.
    sync_at(&mut ui, &snapshot, START_MS + 248);
    assert_eq!(ui.projectiles().len(), 1, "frame 7 < count 8: still flying");
    sync_at(&mut ui, &snapshot, START_MS + 249);
    assert!(
        ui.projectiles().is_empty(),
        "arrived projectile must be removed"
    );
}

#[test]
fn sprite_index_stays_inside_the_magic_projectile_frames() {
    for dir16 in 0..DIRECTIONS {
        for frame in 0..FRAME_COUNT {
            let index = sprite_index(frame, dir16);
            assert!(
                (BASE_INDEX..=565).contains(&index),
                "index {index} outside magic.wzl projectile range"
            );
            assert!(
                !(416..=419).contains(&index),
                "index {index} is one of the empty 416-419 frames"
            );
            assert!(
                !(566..=569).contains(&index),
                "index {index} is one of the empty 566-569 frames"
            );
        }
        assert_eq!(
            sprite_index(FRAME_COUNT, dir16),
            BASE_INDEX + u32::from(dir16) * 10
        );
    }
    assert_eq!(sprite_index(0, 0), BASE_INDEX);
    assert_eq!(sprite_index(5, 15), 410 + 5 + 15 * 10);
}

#[test]
fn direction16_matches_crystal_sixteen_way_samples() {
    // Crystal `GameScene.cs:12044-12065`: 0 = up, 4 = right, 8 = down, 12 = left.
    let origin = Point::new(10, 20);
    assert_eq!(direction16(origin, Point::new(10, 15)), 0, "up");
    assert_eq!(direction16(origin, Point::new(15, 15)), 2, "up-right");
    assert_eq!(direction16(origin, Point::new(15, 20)), 4, "right");
    assert_eq!(direction16(origin, Point::new(15, 25)), 6, "down-right");
    assert_eq!(direction16(origin, Point::new(10, 25)), 8, "down");
    assert_eq!(direction16(origin, Point::new(5, 25)), 10, "down-left");
    assert_eq!(direction16(origin, Point::new(5, 20)), 12, "left");
    assert_eq!(direction16(origin, Point::new(5, 15)), 14, "up-left");
    assert_eq!(
        direction16(origin, origin),
        0,
        "zero distance uses direction 0"
    );
    for dir16 in direction16(origin, Point::new(20, 20))..=direction16(origin, Point::new(20, 18)) {
        assert!(dir16 < DIRECTIONS, "direction {dir16} out of range");
    }
}

#[test]
fn draw_offset_interpolates_from_source_to_destination() {
    let source = Point::new(10, 10);
    let target = Point::new(10, 14);
    let duration = duration_ms(source, target);
    assert_eq!(duration, 200);
    assert_eq!(frame_count(duration), 6);
    assert_eq!(draw_offset(source, target, 0, 6), (0, 0));
    assert_eq!(draw_offset(source, target, 3, 6), (0, 64));
    assert_eq!(draw_offset(source, target, 6, 6), (0, 128));
    assert_eq!(draw_offset(source, target, 4, 6), (0, 128 * 4 / 6));

    let mut snapshot = seeded_snapshot(source, target);
    snapshot
        .projectile_events
        .push(fire_bounce(SOURCE_ID, DEST_ID));
    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().views()[0].offset(), (0, 0));
    sync_at(&mut ui, &snapshot, START_MS + 60);
    let view = ui.projectiles().views()[0];
    assert_eq!(view.frame, 1, "4-grid interval is 200 / 6 = 33");
    assert_eq!(view.offset(), draw_offset(source, target, 1, 6));
    assert_eq!(view.offset(), (0, 128 / 6));
}

#[test]
fn views_follow_the_target_current_location_with_dynamic_parameters() {
    let source = Point::new(10, 10);
    let initial = Point::new(12, 10);
    let mut snapshot = seeded_snapshot(source, initial);
    snapshot
        .projectile_events
        .push(fire_bounce(SOURCE_ID, DEST_ID));
    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    let view = ui.projectiles().views()[0];
    assert_eq!(view.destination, initial);
    assert_eq!(view.dir16, 4);
    assert_eq!(view.duration_ms, 100);

    // 目标向下走：每次同步先更新落点，再按新距离重算动态参数（Crystal Effect.cs:185-187）。
    move_monster(&mut snapshot, DEST_ID, Point::new(10, 14), false);
    sync_at(&mut ui, &snapshot, START_MS + 30);
    let view = ui.projectiles().views()[0];
    assert_eq!(view.destination, Point::new(10, 14));
    assert_eq!(view.dir16, 4, "direction stays a spawn-time property");
    assert_eq!(view.duration_ms, 200, "4-grid duration is recomputed");
    assert_eq!(view.frame_count, 6);
    assert_eq!(
        view.frame, 0,
        "29 < 33: the first boundary is not crossed yet"
    );
    assert_eq!(view.offset(), (0, 0));
}

// ---------------------------------------------------------------------------
// G3: impact 570..=579
// ---------------------------------------------------------------------------

#[test]
fn hit_effect_plays_all_ten_magic_frames_when_the_target_survives() {
    // Six extra frames are never used by the flight, so the distance is kept at two cells
    // (duration 100 ms, count 3, strict arrival at +100 ms) to make the hit instant easy
    // to pin.
    let source = Point::new(10, 10);
    let target = Point::new(12, 10);
    let mut snapshot = seeded_snapshot(source, target);
    snapshot
        .projectile_events
        .push(fire_bounce(SOURCE_ID, DEST_ID));
    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().len(), 1);

    let arrival = START_MS + 100;
    sync_at(&mut ui, &snapshot, arrival);
    assert!(
        ui.projectiles().is_empty(),
        "arrived projectile must be removed"
    );
    let effects = ui.projectiles().hit_effects();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].target_id, DEST_ID);
    assert_eq!(effects[0].position, target);
    assert_eq!(effects[0].started_ms, arrival);

    let frame_ms = i64::try_from(HIT_EFFECT_FRAME_MS).expect("hit frame duration fits i64");
    let mut observed = Vec::new();
    for step in 0..HIT_EFFECT_FRAME_COUNT {
        let now = arrival + i64::from(step) * frame_ms;
        sync_at(&mut ui, &snapshot, now);
        let effects = ui.projectiles().hit_effects();
        assert_eq!(
            effects.len(),
            1,
            "impact must stay alive during frame {step}"
        );
        let index = effects[0].sprite_index(now);
        assert_eq!(
            index,
            hit_effect_index(u64::from(step) * HIT_EFFECT_FRAME_MS)
        );
        observed.push(index);
    }
    let expected: Vec<u32> = (HIT_EFFECT_BASE_INDEX..HIT_EFFECT_BASE_INDEX + 10).collect();
    assert_eq!(observed, expected, "impact must walk magic.wzl[570..=579]");
    assert_eq!(observed.first().copied(), Some(570));
    assert_eq!(observed.last().copied(), Some(579));

    // `Effect.Process` removes the impact once the full 600 ms elapsed.
    sync_at(
        &mut ui,
        &snapshot,
        arrival + i64::try_from(HIT_EFFECT_DURATION_MS).unwrap(),
    );
    assert!(
        ui.projectiles().hit_effects().is_empty(),
        "impact must be removed after {HIT_EFFECT_DURATION_MS} ms"
    );
}

#[test]
fn hit_effect_follows_the_surviving_target() {
    let mut snapshot = seeded_snapshot(Point::new(10, 10), Point::new(12, 10));
    snapshot
        .projectile_events
        .push(fire_bounce(SOURCE_ID, DEST_ID));
    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    sync_at(&mut ui, &snapshot, START_MS + 100);
    assert_eq!(
        ui.projectiles().hit_effects()[0].position,
        Point::new(12, 10)
    );

    move_monster(&mut snapshot, DEST_ID, Point::new(13, 12), false);
    sync_at(&mut ui, &snapshot, START_MS + 100 + 60);
    assert_eq!(
        ui.projectiles().hit_effects()[0].position,
        Point::new(13, 12),
        "impact is attached to the target (Effect.cs:117-119), so it follows it"
    );
}

#[test]
fn dead_target_skips_impact_and_hit_sound() {
    let mut snapshot = seeded_snapshot(Point::new(10, 10), Point::new(12, 10));
    move_monster(&mut snapshot, DEST_ID, Point::new(12, 10), true);
    snapshot
        .projectile_events
        .push(fire_bounce(SOURCE_ID, DEST_ID));

    let mut ui = ui_state();
    let mut sink = FakeSink::default();
    ui.projectiles
        .sync_with_sink(&snapshot, START_MS, &mut sink);
    assert_eq!(
        ui.projectiles().len(),
        1,
        "a known dead target still gets a projectile"
    );
    assert_eq!(sink.played, vec!["M34-1.wav"]);

    ui.projectiles
        .sync_with_sink(&snapshot, START_MS + 100, &mut sink);
    assert!(
        ui.projectiles().hit_effects().is_empty(),
        "dead target (GameScene.cs:4707) must not start the 570 impact"
    );
    assert_eq!(
        sink.played,
        vec!["M34-1.wav"],
        "dead target must not play the M34-2 impact cue"
    );
}

// ---------------------------------------------------------------------------
// G4: sound mapping
// ---------------------------------------------------------------------------

#[test]
fn spawn_and_hit_sounds_use_m34_once_each() {
    let mut snapshot = seeded_snapshot(Point::new(10, 10), Point::new(12, 10));
    snapshot
        .projectile_events
        .push(fire_bounce(SOURCE_ID, DEST_ID));

    let mut ui = ui_state();
    let mut sink = FakeSink::default();
    ui.projectiles
        .sync_with_sink(&snapshot, START_MS, &mut sink);
    assert_eq!(
        sink.played,
        vec!["M34-1.wav"],
        "spawn cue must be M34-1.wav"
    );

    ui.projectiles
        .sync_with_sink(&snapshot, START_MS + 100, &mut sink);
    assert_eq!(
        sink.played,
        vec!["M34-1.wav", "M34-2.wav"],
        "impact cue must be M34-2.wav"
    );

    // Neither the snapshot replay nor the impact lifetime may replay a cue.
    ui.projectiles
        .sync_with_sink(&snapshot, START_MS + 100, &mut sink);
    assert_eq!(sink.played.len(), 2);
    let spawn_count = sink
        .played
        .iter()
        .filter(|file| **file == "M34-1.wav")
        .count();
    let hit_count = sink
        .played
        .iter()
        .filter(|file| **file == "M34-2.wav")
        .count();
    assert_eq!(spawn_count, 1, "spawn cue plays exactly once");
    assert_eq!(hit_count, 1, "impact cue plays exactly once");
}

#[test]
fn projectile_sound_uses_the_great_fire_ball_base_m34() {
    // Dispatch M2: the cue index is built from `Spell.GreatFireBall = 34`
    // (`GameScene.cs:4697/4709`), never from `Spell.FireBounce = 154`.
    assert_eq!(PROJECTILE_SOUND_BASE, 34);
    assert_eq!(Spell::GreatFireBall as u32, PROJECTILE_SOUND_BASE);
    assert_eq!(
        PROJECTILE_SOUND_SPAWN_INDEX,
        20_000 + PROJECTILE_SOUND_BASE * 10 + 1
    );
    assert_eq!(
        PROJECTILE_SOUND_HIT_INDEX,
        20_000 + PROJECTILE_SOUND_BASE * 10 + 2
    );
    assert_eq!(PROJECTILE_SOUND_SPAWN_INDEX, 20_341);
    assert_eq!(PROJECTILE_SOUND_HIT_INDEX, 20_342);

    assert_eq!(
        projectile_sound(Spell::FireBounce, ProjectileSoundPhase::Spawn),
        Some("M34-1.wav")
    );
    assert_eq!(
        projectile_sound(Spell::FireBounce, ProjectileSoundPhase::Hit),
        Some("M34-2.wav")
    );
    assert_eq!(ProjectileSoundPhase::Spawn.file(), "M34-1.wav");
    assert_eq!(ProjectileSoundPhase::Hit.file(), "M34-2.wav");
    assert_ne!(
        projectile_sound(Spell::FireBounce, ProjectileSoundPhase::Spawn),
        Some("M154-1.wav")
    );
    assert_eq!(
        projectile_sound(Spell::FireBall, ProjectileSoundPhase::Spawn),
        None,
        "only the projectile spell carries a projectile cue"
    );
}

#[test]
fn production_sound_cues_are_drained_once_per_projectile() {
    let mut snapshot = seeded_snapshot(Point::new(10, 10), Point::new(12, 10));
    snapshot
        .projectile_events
        .push(fire_bounce(SOURCE_ID, DEST_ID));
    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().sound_cues(), ["M34-1.wav"]);

    let drained = ui.projectiles.take_sound_cues();
    assert_eq!(drained, ["M34-1.wav"]);
    assert!(
        ui.projectiles().sound_cues().is_empty(),
        "draining the cue queue must empty it"
    );

    sync_at(&mut ui, &snapshot, START_MS + 100);
    assert_eq!(ui.projectiles().sound_cues(), ["M34-2.wav"]);
    assert_eq!(ui.projectiles.take_sound_cues(), ["M34-2.wav"]);
    assert!(ui.projectiles().sound_cues().is_empty());
}
