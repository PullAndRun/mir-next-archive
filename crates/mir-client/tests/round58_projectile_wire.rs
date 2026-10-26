//! Round 58 S2: client `ObjectProjectile`(125) decode wiring (no rendering).
//!
//! The tests drive the real public wire path:
//! `ServerPacket::ObjectProjectile` -> `encode_frame` -> `decode_server_packet`
//! -> `WorldSnapshot::apply_packet_at` -> `WorldSnapshot::projectile_events`.
//!
//! Crystal parity is pinned to `Client/MirScenes/GameScene.cs:4687-4716`: an unknown
//! `Source` is dropped without panicking, only `Spell.FireBounce` produces a projectile,
//! and an unknown `Destination` is tolerated (Crystal only skips the impact effect when
//! `missile.Target == null`). Round 58 creates no rendering/flight state (C2 PARTIAL,
//! the visual is deferred to round 59).

use mir_client::net::{ProjectileEvent, WorldSnapshot};
use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender, Monster, Spell};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{
    ObjectMonster, ObjectProjectile, UserInformation, decode_server_packet,
};

/// Self player object id.
const SELF_ID: u32 = 1;
/// Source monster object id used by the positive cases.
const SOURCE_ID: u32 = 7;
/// Destination monster object id used by the positive cases.
const DEST_ID: u32 = 8;

fn seed_self(snapshot: &mut WorldSnapshot) {
    let info = UserInformation {
        object_id: SELF_ID,
        real_id: SELF_ID,
        name: "wire-test".to_owned(),
        class: MirClass::Wizard,
        gender: MirGender::Male,
        level: 20,
        location: Point::new(20, 20),
        direction: MirDirection::Down,
        hair: 1,
        hp: 100,
        mp: 100,
        ..UserInformation::default()
    };
    snapshot.apply_packet_at(&ServerPacket::UserInformation(info), 0);
}

fn seed_monster(snapshot: &mut WorldSnapshot, object_id: u32, location: Point) {
    snapshot.apply_packet_at(
        &ServerPacket::ObjectMonster(ObjectMonster {
            object_id,
            name: format!("monster-{object_id}"),
            location,
            image: Monster::Skeleton,
            direction: MirDirection::Down,
            ..ObjectMonster::default()
        }),
        0,
    );
    assert!(
        snapshot.object_position(object_id).is_some(),
        "seed failed: monster {object_id} is not a known snapshot object"
    );
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

fn decoded_projectile(packet: &ServerPacket) -> ObjectProjectile {
    match through_wire(packet) {
        ServerPacket::ObjectProjectile(projectile) => projectile,
        other => panic!("expected ObjectProjectile after wire round-trip, got {other:?}"),
    }
}

fn firebounce(source: u32, destination: u32) -> ServerPacket {
    ServerPacket::ObjectProjectile(ObjectProjectile {
        spell: Spell::FireBounce,
        source,
        destination,
    })
}

#[test]
fn firebounce_packet_becomes_projectile_event() {
    let mut snapshot = WorldSnapshot::default();
    seed_self(&mut snapshot);
    seed_monster(&mut snapshot, SOURCE_ID, Point::new(21, 20));
    seed_monster(&mut snapshot, DEST_ID, Point::new(23, 20));

    let decoded = decoded_projectile(&firebounce(SOURCE_ID, DEST_ID));
    assert_eq!(decoded.spell, Spell::FireBounce, "decoded spell field");
    assert_eq!(decoded.source, SOURCE_ID, "decoded source field");
    assert_eq!(decoded.destination, DEST_ID, "decoded destination field");

    snapshot.apply_packet_at(&ServerPacket::ObjectProjectile(decoded), 1_000);
    assert_eq!(
        snapshot.packet_count("ObjectProjectile"),
        1,
        "packet counter counts the wired packet"
    );
    assert_eq!(snapshot.projectile_events.len(), 1, "one event per packet");
    assert_eq!(
        snapshot.projectile_events[0],
        ProjectileEvent {
            spell: Spell::FireBounce,
            source: SOURCE_ID,
            destination: DEST_ID,
        },
        "event fields must mirror the wire payload"
    );

    // Consume the ring exactly like the future UI would.
    let consumed: Vec<ProjectileEvent> = std::mem::take(&mut snapshot.projectile_events);
    assert_eq!(consumed.len(), 1, "drained projectile events");
    assert_eq!(consumed[0].spell, Spell::FireBounce, "consumed spell");
    assert_eq!(consumed[0].source, SOURCE_ID, "consumed source");
    assert_eq!(consumed[0].destination, DEST_ID, "consumed destination");
    assert!(
        snapshot.projectile_events.is_empty(),
        "drained ring is empty"
    );
}

#[test]
fn self_object_is_a_known_source() {
    let mut snapshot = WorldSnapshot::default();
    seed_self(&mut snapshot);

    snapshot.apply_packet_at(&firebounce(SELF_ID, DEST_ID), 1_000);

    assert_eq!(
        snapshot.projectile_events.len(),
        1,
        "the local player is part of the known object set"
    );
    assert_eq!(
        snapshot.projectile_events[0].source, SELF_ID,
        "self source id"
    );
}

#[test]
fn unknown_source_is_dropped_without_panic() {
    let mut snapshot = WorldSnapshot::default();
    seed_self(&mut snapshot);
    seed_monster(&mut snapshot, DEST_ID, Point::new(23, 20));

    snapshot.apply_packet_at(&firebounce(999, DEST_ID), 1_000);

    assert!(
        snapshot.projectile_events.is_empty(),
        "unknown source must be dropped"
    );
    assert_eq!(
        snapshot.packet_count("ObjectProjectile"),
        1,
        "dropped packets are still counted"
    );
}

#[test]
fn unknown_destination_does_not_panic_or_drop() {
    let mut snapshot = WorldSnapshot::default();
    seed_self(&mut snapshot);
    seed_monster(&mut snapshot, SOURCE_ID, Point::new(21, 20));

    snapshot.apply_packet_at(&firebounce(SOURCE_ID, 999), 1_000);

    assert_eq!(
        snapshot.projectile_events.len(),
        1,
        "unknown destination is tolerated"
    );
    assert_eq!(
        snapshot.projectile_events[0].destination, 999,
        "destination is kept verbatim"
    );
}

#[test]
fn non_firebounce_spell_is_dropped() {
    let mut snapshot = WorldSnapshot::default();
    seed_self(&mut snapshot);
    seed_monster(&mut snapshot, SOURCE_ID, Point::new(21, 20));
    seed_monster(&mut snapshot, DEST_ID, Point::new(23, 20));

    let packet = ServerPacket::ObjectProjectile(ObjectProjectile {
        spell: Spell::PoisonCloud,
        source: SOURCE_ID,
        destination: DEST_ID,
    });
    // The wire itself still decodes any spell byte; the drop happens in apply.
    assert_eq!(
        decoded_projectile(&packet).spell,
        Spell::PoisonCloud,
        "wire decode keeps the raw spell"
    );

    snapshot.apply_packet_at(&packet, 1_000);

    assert!(
        snapshot.projectile_events.is_empty(),
        "only FireBounce produces a projectile event"
    );
    assert_eq!(
        snapshot.packet_count("ObjectProjectile"),
        1,
        "dropped packets are still counted"
    );
}

#[test]
fn consecutive_projectiles_keep_wire_order() {
    let mut snapshot = WorldSnapshot::default();
    seed_self(&mut snapshot);
    seed_monster(&mut snapshot, SOURCE_ID, Point::new(21, 20));
    seed_monster(&mut snapshot, DEST_ID, Point::new(23, 20));

    let first = through_wire(&firebounce(SOURCE_ID, DEST_ID));
    let second = through_wire(&firebounce(DEST_ID, SOURCE_ID));
    snapshot.apply_packet_at(&first, 1_000);
    snapshot.apply_packet_at(&second, 1_000);

    let order: Vec<(u32, u32)> = snapshot
        .projectile_events
        .iter()
        .map(|event| (event.source, event.destination))
        .collect();
    assert_eq!(
        order,
        vec![(SOURCE_ID, DEST_ID), (DEST_ID, SOURCE_ID)],
        "same-tick projectiles must keep packet order"
    );
}

#[test]
fn projectile_ring_reuses_measured_effect_event_limit() {
    let mut snapshot = WorldSnapshot::default();
    seed_self(&mut snapshot);
    seed_monster(&mut snapshot, SOURCE_ID, Point::new(21, 20));

    for offset in 0..20u32 {
        snapshot.apply_packet_at(&firebounce(SOURCE_ID, 100 + offset), 1_000);
    }

    // Probe result recorded in target/round58/s2/attempt-1/01-probe.log:
    // EFFECT_EVENT_LIMIT = 16 (crates/mir-client/src/net.rs:165), reused by the
    // projectile event ring instead of inventing a new cap.
    assert_eq!(
        snapshot.projectile_events.len(),
        16,
        "projectile ring cap must be the measured EFFECT_EVENT_LIMIT"
    );
    assert_eq!(
        snapshot
            .projectile_events
            .first()
            .map(|event| event.destination),
        Some(104),
        "oldest four events dropped"
    );
    assert_eq!(
        snapshot
            .projectile_events
            .last()
            .map(|event| event.destination),
        Some(119),
        "newest event is last"
    );
}
