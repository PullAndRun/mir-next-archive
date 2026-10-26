//! Round 54-B: `FrostCrunch` exact +2 probability and real party/guild cast guards.
//!
//! These tests complement the frozen Round 53 suite (`tests/round53_frost_pvp.rs`)
//! without modifying it. Every sample is a real `World::cast_magic` call followed
//! by a real delayed `World::tick`; no `PoisonState` is ever injected. Party and
//! guild relationships are created through the production invite/join path
//! (`World::group_invite` / `World::group_join` and
//! `World::create_guild` / `World::guild_invite` / `World::guild_join`), never by
//! writing `group_id` / `guild` directly. The guild helpers mirror
//! `tests/common/mod.rs::Harness::create_guild` / `join`, which are the same real
//! calls `tests/guild.rs` uses.
//!
//! `FrostCrunch` landing rules (`crates/mir-server/src/magic.rs:4089-4167`,
//! Crystal `HumanObject.cs:5974` / `:5986`):
//! * the landing recheck predicate is `is_attack_target_player`
//!   (`crates/mir-server/src/world.rs:2503-2529`; Group mode rejects a real
//!   group mate, Guild mode rejects a real guild mate);
//! * the level gate is `caster_level + 2 >= target_level` for a player target
//!   (monsters use +10); above that gate both poison rolls are skipped while the
//!   direct damage is still applied;
//! * Slow rolls `rng(0..=99) <= magic_level` and lasts 4 ticks for players and
//!   Frozen rolls the same way and lasts 2 ticks; both tick at 1000 ms.

mod common;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{AttackMode, MirClass, MirDirection, MirGender, PoisonType, Spell};
use mir_db::{MapInfo, MirDb};
use mir_protocol::ServerPacket;
use mir_server::combat;
use mir_server::gamedata::GameData;
use mir_server::magic::{
    CastReport, FROST_CRUNCH_BASE_DELAY_MS, FROST_CRUNCH_POISON_TICK_MS, FROST_CRUNCH_STEP_MS,
    PoisonState,
};
use mir_server::world::{Player, World};
use tokio::sync::mpsc;

/// Map file name used by every scenario; the rounds are placed within two cells.
const MAP: &str = "0";
/// Victim HP restored before every sample so HP causality is exact.
const MAX_HP: i32 = 100_000;
/// `FrostCrunch` skill level 3 => 4% chance for each poison roll.
const FROST_LEVEL: u8 = 3;
/// Caster level used by the exact +2 test and the guard tests.
const CASTER_LEVEL: u16 = 60;
/// Delayed landing deadline for a target two cells away (2 * 50 + 500 = 600 ms).
const FROST_FLIGHT_MS: i64 = FROST_CRUNCH_STEP_MS * 2 + FROST_CRUNCH_BASE_DELAY_MS + 100;
/// Frozen seeds mandated by the Round 54 dispatch.
const SAMPLE_SEEDS: [u64; 4] = [3, 5, 7, 11];
/// Real casts per seed (4 * 300 = 1200 total).
const SAMPLES_PER_SEED: u32 = 300;
/// Samples for the "+3 target" control, where no poison may be applied.
const PLUS_THREE_SAMPLES: u32 = 100;
/// Total real casts in the probability test.
const TOTAL_SAMPLES: u32 = SAMPLES_PER_SEED * SAMPLE_SEEDS.len() as u32;
/// Admissible hit count: expected / 2 through expected * 3 / 2 (24..=72 for 48).
const POISON_HIT_MIN: u32 = 24;
const POISON_HIT_MAX: u32 = 72;

fn player_name(object_id: u32) -> String {
    format!("Player{object_id}")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root above crates/mir-server")
        .to_path_buf()
}

fn real_game_data() -> Arc<GameData> {
    let path = repo_root()
        .join("game-assets")
        .join("data")
        .join("game.mnd");
    assert!(
        path.is_file(),
        "missing real game data at {}",
        path.display()
    );
    Arc::new(GameData::load_mnd(&path).expect("load game-assets/data/game.mnd"))
}

/// Game data with the real `FrostCrunch` record and the round's map only.
fn test_game_data() -> Arc<GameData> {
    let base = real_game_data();
    let frost = base
        .magic(Spell::FrostCrunch)
        .expect("FrostCrunch record in the real magic table")
        .clone();
    let db = MirDb {
        maps: vec![MapInfo {
            index: 1,
            file_name: MAP.to_owned(),
            ..Default::default()
        }],
        magics: vec![frost],
        ..MirDb::default()
    };
    Arc::new(GameData::from_db(db, None))
}

fn strike_packets(packets: &[ServerPacket]) -> usize {
    packets
        .iter()
        .filter(|packet| {
            matches!(
                packet,
                ServerPacket::Struck(_) | ServerPacket::DamageIndicator(_)
            )
        })
        .count()
}

struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Harness {
    /// Fresh world with a frozen wall clock (no real time leaks into `now`).
    fn with_data(seed: u64, data: Arc<GameData>) -> Self {
        let mut world = World::new(999);
        world.advance_clock(0);
        world.seed_rng(seed);
        world.insert_map(MAP, common::synthetic_map(MAP));
        world.set_game_data(data);
        world.set_guild_settings(common::guild_settings());
        Self {
            world,
            receivers: HashMap::new(),
        }
    }

    fn add_player(
        &mut self,
        object_id: u32,
        class: MirClass,
        level: u16,
        gold: i64,
        position: Point,
    ) -> u32 {
        let stats = combat::player_base_stats(level, class);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.enter(Player {
            object_id,
            session_id: u64::from(object_id),
            character_id: i64::from(object_id),
            name: player_name(object_id),
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
            magic: mir_server::MagicState::default(),
            gold,
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
        self.receivers.insert(object_id, receiver);
        object_id
    }

    fn learn_frost(&mut self, caster: u32) {
        self.world
            .learn_magic(caster, Spell::FrostCrunch)
            .expect("learning FrostCrunch must succeed");
        let player = self.world.player_mut(caster).expect("caster exists");
        let magic = player
            .magics
            .iter_mut()
            .find(|magic| magic.spell == Spell::FrostCrunch)
            .expect("FrostCrunch appears in the spell book");
        magic.level = FROST_LEVEL;
    }

    /// Clear everything that could carry state (cooldown, mana, RNG draw knobs)
    /// between samples so a hit can never come from the previous sample.
    fn ready_caster(&mut self, caster: u32) {
        let player = self.world.player_mut(caster).expect("caster exists");
        player.action_time = 0;
        player.attack_time = 0;
        player.magic.spell_time = 0;
        player.magic.casts.clear();
        player.stats.mp = 10_000;
        player.mp = 10_000;
        player.stats.min_mc = 100;
        player.stats.max_mc = 100;
        player.stats.critical_rate = 0;
        player.stats.luck = 0;
    }

    /// Full HP and empty poison list, so the victim starts from a known state.
    fn ready_victim(&mut self, victim: u32) {
        let player = self.world.player_mut(victim).expect("victim exists");
        player.hp = MAX_HP;
        player.stats.hp = MAX_HP;
        player.dead = false;
        player.magic.poisons.clear();
        player.stats.min_ac = 0;
        player.stats.max_ac = 0;
        player.stats.min_mac = 0;
        player.stats.max_mac = 0;
        player.stats.magic_resist = 0;
        player.stats.energy_shield_percent = 0;
        player.stats.energy_shield_hp_gain = 0;
        player.stats.damage_reduction_percent = 0;
        player.stats.reflect = 0;
        player.stats.agility = 0;
    }

    fn elapse(&mut self, milliseconds: i64) {
        self.world.advance_clock(milliseconds);
        self.world.tick();
    }

    fn cast_frost(&mut self, caster: u32, target: u32) -> CastReport {
        let location = self.world.player(target).expect("target exists").position;
        self.world
            .cast_magic(
                caster,
                Spell::FrostCrunch,
                MirDirection::Right,
                target,
                location,
            )
            .expect("the world must accept the FrostCrunch cast request")
    }

    fn set_attack_mode(&mut self, object_id: u32, mode: AttackMode) {
        assert!(
            self.world.change_attack_mode(object_id, mode),
            "changing the attack mode of {object_id} must succeed"
        );
    }

    fn hp(&self, object_id: u32) -> i32 {
        self.world.player(object_id).expect("player exists").hp
    }

    fn poison(&self, object_id: u32, kind: PoisonType) -> Option<PoisonState> {
        self.world
            .player(object_id)
            .expect("player exists")
            .magic
            .poisons
            .iter()
            .find(|poison| poison.kind == kind)
            .copied()
    }

    fn no_poisons(&self, object_id: u32) -> bool {
        self.world
            .player(object_id)
            .expect("player exists")
            .magic
            .poisons
            .is_empty()
    }

    fn packets(&mut self, object_id: u32) -> Vec<ServerPacket> {
        let receiver = self.receivers.get_mut(&object_id).expect("player channel");
        let mut packets = Vec::new();
        while let Ok(frame) = receiver.try_recv() {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            packets.push(ServerPacket::decode(index, &frame[4..]).expect("decode a server packet"));
        }
        packets
    }

    fn drain(&mut self, object_id: u32) {
        let _ = self.packets(object_id);
    }

    /// Real party path: invite by name then accept (`group.rs:219-227` style).
    fn join_group(&mut self, inviter: u32, member: u32) {
        let name = player_name(member);
        self.world
            .group_invite(inviter, &name)
            .unwrap_or_else(|error| panic!("group invite of {name} must succeed: {error}"));
        self.world
            .group_join(member, true)
            .unwrap_or_else(|error| panic!("{name} must accept the group invite: {error}"));
    }

    /// Real guild path; mirrors `tests/common/mod.rs:378-393`.
    fn create_guild(&mut self, leader: u32, name: &str) -> i32 {
        self.world
            .create_guild(leader, name)
            .unwrap_or_else(|error| panic!("guild creation {name} must succeed: {error}"))
    }

    fn join_guild(&mut self, inviter: u32, member: u32) {
        let name = player_name(member);
        self.world
            .guild_invite(inviter, &name)
            .unwrap_or_else(|error| panic!("guild invite of {name} must succeed: {error}"));
        self.world
            .guild_join(member, true)
            .unwrap_or_else(|error| panic!("{name} must accept the guild invite: {error}"));
    }

    fn guild_id(&self, object_id: u32) -> Option<i32> {
        self.world
            .guild_ref_of(object_id)
            .map(|guild_ref| guild_ref.guild_id)
    }

    fn pending_count(&self) -> usize {
        self.world.pending_magic().len()
    }
}

/// Caster level 60 against a player of `target_level`, two cells away.
fn frost_pair(seed: u64, data: &Arc<GameData>, target_level: u16) -> Harness {
    let mut harness = Harness::with_data(seed, Arc::clone(data));
    harness.add_player(1, MirClass::Taoist, CASTER_LEVEL, 0, Point::new(10, 10));
    harness.add_player(2, MirClass::Warrior, target_level, 0, Point::new(12, 10));
    harness.learn_frost(1);
    harness.set_attack_mode(1, AttackMode::All);
    harness
}

/// R54-B-1: 1200 real casts, caster 60 / target 62 / skill 3 -> exactly +2 bound.
#[test]
fn round54_frost_exact_plus_two_probability() {
    let data = test_game_data();
    let expected = TOTAL_SAMPLES * (u32::from(FROST_LEVEL) + 1) / 100;
    assert_eq!(
        expected, 48,
        "skill level 3 must be a 4% poison chance over {TOTAL_SAMPLES} real casts"
    );
    assert_eq!(
        (POISON_HIT_MIN, POISON_HIT_MAX),
        (expected / 2, expected + expected / 2),
        "admissible interval must be half to one and a half times the expected count"
    );

    let mut slow_hits = 0_u32;
    let mut slow_misses = 0_u32;
    let mut frozen_hits = 0_u32;
    let mut frozen_misses = 0_u32;
    let mut total_damage = 0_i64;

    for seed in SAMPLE_SEEDS {
        let mut harness = frost_pair(seed, &data, 62);
        let mut seed_slow_hits = 0_u32;
        let mut seed_slow_misses = 0_u32;
        let mut seed_frozen_hits = 0_u32;
        let mut seed_frozen_misses = 0_u32;
        let mut seed_damage = 0_i64;

        for sample in 0..SAMPLES_PER_SEED {
            harness.ready_caster(1);
            harness.ready_victim(2);
            // Clean slate: a hit may only come from this sample's own real cast.
            assert_eq!(
                harness.hp(2),
                MAX_HP,
                "seed {seed} sample {sample}: victim HP must be reset before the sample"
            );
            assert!(
                harness.no_poisons(2),
                "seed {seed} sample {sample}: victim poisons must be cleared before the sample"
            );
            assert_eq!(
                harness
                    .world
                    .player(1)
                    .expect("caster exists")
                    .magic
                    .spell_time,
                0,
                "seed {seed} sample {sample}: caster cooldown must be reset before the sample"
            );

            let hp_before = harness.hp(2);
            let report = harness.cast_frost(1, 2);
            assert_eq!(
                report.spell,
                Spell::FrostCrunch,
                "seed {seed} sample {sample}: unexpected spell"
            );
            assert_eq!(
                report.level, FROST_LEVEL,
                "seed {seed} sample {sample}: skill level must stay 3"
            );
            assert_eq!(
                report.target_id, 2,
                "seed {seed} sample {sample}: caster 60 + 2 vs target 62 must keep the player target"
            );
            assert!(
                report.cost > 0,
                "seed {seed} sample {sample}: a real cast must pay mana, got cost {}",
                report.cost
            );
            let dealt = report
                .damage
                .expect("accepted FrostCrunch cast must roll damage");
            assert!(
                dealt > 0,
                "seed {seed} sample {sample}: FrostCrunch damage must be positive, got {dealt}"
            );
            assert_eq!(
                harness.pending_count(),
                1,
                "seed {seed} sample {sample}: a real cast must queue exactly one delayed landing"
            );
            assert_eq!(
                harness.hp(2),
                hp_before,
                "seed {seed} sample {sample}: HP must not move before the flight delay"
            );
            assert!(
                harness.no_poisons(2),
                "seed {seed} sample {sample}: no poison may exist before the flight delay"
            );

            harness.elapse(FROST_FLIGHT_MS);
            assert!(
                harness.world.pending_magic().is_empty(),
                "seed {seed} sample {sample}: the delayed landing must be resolved by the tick"
            );
            assert_eq!(
                harness.hp(2),
                hp_before - dealt,
                "seed {seed} sample {sample}: HP causality must hold ({hp_before} - {dealt})"
            );

            if let Some(poison) = harness.poison(2, PoisonType::Slow) {
                slow_hits += 1;
                seed_slow_hits += 1;
                assert_eq!(
                    poison.duration, 4,
                    "seed {seed} sample {sample}: player Slow lasts 4 ticks"
                );
                assert_eq!(
                    poison.tick_speed_ms, FROST_CRUNCH_POISON_TICK_MS,
                    "seed {seed} sample {sample}: player Slow ticks every 1000 ms"
                );
            } else {
                slow_misses += 1;
                seed_slow_misses += 1;
            }
            if let Some(poison) = harness.poison(2, PoisonType::Frozen) {
                frozen_hits += 1;
                seed_frozen_hits += 1;
                assert_eq!(
                    poison.duration, 2,
                    "seed {seed} sample {sample}: player Frozen lasts 2 ticks"
                );
                assert_eq!(
                    poison.tick_speed_ms, FROST_CRUNCH_POISON_TICK_MS,
                    "seed {seed} sample {sample}: player Frozen ticks every 1000 ms"
                );
            } else {
                frozen_misses += 1;
                seed_frozen_misses += 1;
            }

            seed_damage += i64::from(dealt);
            total_damage += i64::from(dealt);
        }

        assert_eq!(
            seed_slow_hits + seed_slow_misses,
            SAMPLES_PER_SEED,
            "seed {seed}: every real cast must yield a Slow hit or miss"
        );
        assert_eq!(
            seed_frozen_hits + seed_frozen_misses,
            SAMPLES_PER_SEED,
            "seed {seed}: every real cast must yield a Frozen hit or miss"
        );
        println!(
            "exact+2 seed {seed}: slow {seed_slow_hits}/{SAMPLES_PER_SEED} hit, \
             frozen {seed_frozen_hits}/{SAMPLES_PER_SEED} hit, damage {seed_damage}"
        );
    }

    println!(
        "exact+2 total: slow {slow_hits}/{TOTAL_SAMPLES} hit ({slow_misses} miss), \
         frozen {frozen_hits}/{TOTAL_SAMPLES} hit ({frozen_misses} miss), \
         expected {expected}, admissible [{POISON_HIT_MIN}, {POISON_HIT_MAX}], \
         damage {total_damage}"
    );
    assert_eq!(
        slow_hits + slow_misses,
        TOTAL_SAMPLES,
        "Slow hit and miss counts must add up to {TOTAL_SAMPLES}"
    );
    assert_eq!(
        frozen_hits + frozen_misses,
        TOTAL_SAMPLES,
        "Frozen hit and miss counts must add up to {TOTAL_SAMPLES}"
    );
    assert!(
        slow_hits > 0 && slow_misses > 0,
        "Slow needs both hits and misses over {TOTAL_SAMPLES} casts"
    );
    assert!(
        frozen_hits > 0 && frozen_misses > 0,
        "Frozen needs both hits and misses over {TOTAL_SAMPLES} casts"
    );
    assert!(
        (POISON_HIT_MIN..=POISON_HIT_MAX).contains(&slow_hits),
        "Slow hits {slow_hits} outside the admissible range [{POISON_HIT_MIN}, {POISON_HIT_MAX}] (expected {expected})"
    );
    assert!(
        (POISON_HIT_MIN..=POISON_HIT_MAX).contains(&frozen_hits),
        "Frozen hits {frozen_hits} outside the admissible range [{POISON_HIT_MIN}, {POISON_HIT_MAX}] (expected {expected})"
    );

    // +3 control: caster 60 + 2 < target 63, so damage still lands but both
    // poison rolls (and therefore both statuses) must be skipped.
    let mut control = frost_pair(101, &data, CASTER_LEVEL + 3);
    for sample in 0..PLUS_THREE_SAMPLES {
        control.ready_caster(1);
        control.ready_victim(2);
        let hp_before = control.hp(2);
        let report = control.cast_frost(1, 2);
        assert_eq!(
            report.target_id, 2,
            "plus-three sample {sample}: an out-of-level player target is still a cast target"
        );
        let dealt = report.damage.expect("plus-three cast must roll damage");
        assert!(
            dealt > 0,
            "plus-three sample {sample}: target 63 must still take FrostCrunch damage, got {dealt}"
        );
        control.elapse(FROST_FLIGHT_MS);
        assert_eq!(
            control.hp(2),
            hp_before - dealt,
            "plus-three sample {sample}: HP causality must hold for target 63"
        );
        assert!(
            control.no_poisons(2),
            "plus-three sample {sample}: caster 60 + 2 < 63 must skip both poison rolls"
        );
    }
    println!(
        "plus-three control: {PLUS_THREE_SAMPLES} casts vs level 63 dealt damage with zero poisons"
    );
}

/// R54-B-2: a real party formed through invite/join must never be a `FrostCrunch` target.
#[test]
fn round54_frost_party_real_cast_guard() {
    let data = test_game_data();
    let mut harness = Harness::with_data(201, data);
    let caster = harness.add_player(1, MirClass::Taoist, CASTER_LEVEL, 0, Point::new(10, 10));
    let mate = harness.add_player(2, MirClass::Warrior, CASTER_LEVEL, 0, Point::new(12, 10));
    let stranger = harness.add_player(3, MirClass::Warrior, CASTER_LEVEL, 0, Point::new(10, 12));
    harness.learn_frost(caster);

    // Real group membership through the production invite/join path.
    harness.join_group(caster, mate);
    assert_eq!(
        harness.world.group_id_of(caster),
        harness.world.group_id_of(mate),
        "real invite/join must put both players in one group"
    );
    assert!(
        harness.world.group_id_of(caster).is_some(),
        "the group must have a real id"
    );
    assert!(
        harness.world.in_group(caster) && harness.world.in_group(mate),
        "both players must report as grouped"
    );
    assert_eq!(
        harness.world.group_id_of(stranger),
        None,
        "the control player must stay out of the group"
    );

    // Scenario A: Group mode + real party member -> cast-time rejection.
    harness.ready_caster(caster);
    harness.ready_victim(mate);
    harness.set_attack_mode(caster, AttackMode::Group);
    assert_eq!(
        harness
            .world
            .player(caster)
            .expect("caster exists")
            .attack_mode,
        AttackMode::Group,
        "the caster must be in Group attack mode"
    );
    assert!(
        !harness.world.player(caster).expect("caster exists").dead
            && !harness.world.player(mate).expect("mate exists").dead,
        "both players must be alive"
    );
    assert_eq!(
        harness.world.player(caster).expect("caster exists").map,
        harness.world.player(mate).expect("mate exists").map,
        "both players must be on the same map"
    );
    assert!(
        !harness.world.in_safe_zone(caster) && !harness.world.in_safe_zone(mate),
        "neither player may stand in a safe zone"
    );
    assert!(
        !harness.world.map_no_fight(MAP),
        "the map must not be a no-fight map"
    );
    // Reason: the Group arm of the attack-target predicate is the failing check.
    assert!(
        !harness.world.is_attack_target_player(caster, mate),
        "Group mode must refuse a real party member as a target"
    );
    // Control flip: the same pair is accepted under Guild mode, which proves that
    // group membership alone is the rejection cause.
    harness.set_attack_mode(caster, AttackMode::Guild);
    assert!(
        harness.world.is_attack_target_player(caster, mate),
        "the same pair must be attackable under Guild mode, so the Group predicate is the cause"
    );
    harness.set_attack_mode(caster, AttackMode::Group);

    let hp_before = harness.hp(mate);
    let mp_before = harness.world.player(caster).expect("caster exists").mp;
    harness.drain(mate);
    let report = harness.cast_frost(caster, mate);
    assert_eq!(
        report.target_id, 0,
        "a rejected cast must not report a target"
    );
    assert!(
        report.damage.is_none(),
        "a rejected cast must not roll damage"
    );
    assert!(
        harness.world.player(caster).expect("caster exists").mp < mp_before,
        "the client still pays the mana cost of a rejected cast"
    );
    assert_eq!(
        harness.pending_count(),
        0,
        "a rejected cast must not queue a delayed landing"
    );
    assert_eq!(
        harness.hp(mate),
        hp_before,
        "a rejected cast must not change the party member's HP"
    );
    harness.elapse(FROST_FLIGHT_MS);
    assert_eq!(
        harness.hp(mate),
        hp_before,
        "the delayed tick must not damage a party member"
    );
    assert!(
        harness.no_poisons(mate),
        "the delayed tick must not apply Slow/Frozen to a party member"
    );
    let rejected_packets = harness.packets(mate);
    assert_eq!(
        strike_packets(&rejected_packets),
        0,
        "a party member must receive no Struck/DamageIndicator packets (got {} packets)",
        rejected_packets.len()
    );
    assert_eq!(
        harness.pending_count(),
        0,
        "the delayed queue must stay empty after the refusal"
    );
    println!("party A: Group mode refused a real party member with no HP/poison/packet change");

    // Scenario B: same conditions, non-member control must still be hit.
    harness.ready_caster(caster);
    harness.ready_victim(stranger);
    harness.set_attack_mode(caster, AttackMode::Group);
    assert!(
        !harness.world.in_group(stranger),
        "the control target must not be grouped"
    );
    assert!(
        harness.world.is_attack_target_player(caster, stranger),
        "a non-member must stay attackable under Group mode"
    );
    let stranger_hp_before = harness.hp(stranger);
    harness.drain(stranger);
    let report = harness.cast_frost(caster, stranger);
    assert_eq!(
        report.target_id, stranger,
        "the non-member must be resolved as the cast target"
    );
    let dealt = report
        .damage
        .expect("accepted non-member cast must roll damage");
    assert!(dealt > 0, "the non-member hit must deal positive damage");
    assert_eq!(
        harness.hp(stranger),
        stranger_hp_before,
        "HP must not move before the flight delay"
    );
    harness.elapse(FROST_FLIGHT_MS);
    assert_eq!(
        harness.hp(stranger),
        stranger_hp_before - dealt,
        "positive control: the non-member must take the delayed FrostCrunch damage"
    );
    let hit_packets = harness.packets(stranger);
    assert!(
        hit_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Struck(_))),
        "positive control expected S.Struck for the non-member"
    );
    assert!(
        hit_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::DamageIndicator(_))),
        "positive control expected S.DamageIndicator for the non-member"
    );
    println!("party B: non-member control took {dealt} damage with the expected hit packets");

    // Scenario C: successful cast + pending landing, then a real mid-flight
    // invite/join, then the delayed tick must refuse at impact.
    harness.elapse(700); // move past the 2000 ms group-invite cooldown
    assert!(
        harness.world.now() >= 2_000,
        "the join below must be outside the group-invite cooldown, now {}",
        harness.world.now()
    );
    harness.ready_caster(caster);
    harness.ready_victim(stranger);
    harness.set_attack_mode(caster, AttackMode::Group);
    assert!(
        !harness.world.in_group(stranger),
        "the causal target must start as a non-member"
    );
    assert!(
        harness.world.is_attack_target_player(caster, stranger),
        "before the join the stranger must be a valid target"
    );
    let causal_hp_before = harness.hp(stranger);
    harness.drain(stranger);
    let report = harness.cast_frost(caster, stranger);
    assert_eq!(
        report.target_id, stranger,
        "the cast must be accepted and locked onto the target"
    );
    assert!(
        report.damage.is_some_and(|damage| damage > 0),
        "the causal cast must roll positive damage"
    );
    assert_eq!(
        harness.pending_count(),
        1,
        "a successful cast must queue exactly one delayed landing"
    );
    let pending_at = harness.world.pending_magic()[0].at;
    assert!(
        pending_at > harness.world.now(),
        "the delayed landing must be scheduled in the future (at {pending_at}, now {})",
        harness.world.now()
    );
    assert_eq!(
        harness.hp(stranger),
        causal_hp_before,
        "no damage before the delayed landing"
    );

    harness.join_group(caster, stranger);
    assert_eq!(
        harness.world.group_id_of(caster),
        harness.world.group_id_of(stranger),
        "the mid-flight invite/join must form one real group"
    );
    assert!(
        harness.world.in_group(stranger),
        "the causal target must now be grouped"
    );
    assert!(
        !harness.world.is_attack_target_player(caster, stranger),
        "the landing recheck predicate must refuse the new party member"
    );

    harness.elapse(FROST_FLIGHT_MS);
    assert!(
        harness.world.now() >= pending_at,
        "the tick must cross the delayed landing time"
    );
    assert!(
        harness.world.pending_magic().is_empty(),
        "the delayed landing must be resolved by the tick"
    );
    assert_eq!(
        harness.hp(stranger),
        causal_hp_before,
        "the landing recheck must refuse the damage after the real mid-flight group join"
    );
    assert!(
        harness.no_poisons(stranger),
        "a refused landing must not apply Slow/Frozen"
    );
    let causal_packets = harness.packets(stranger);
    assert_eq!(
        strike_packets(&causal_packets),
        0,
        "a refused landing must not send Struck/DamageIndicator (got {} packets)",
        causal_packets.len()
    );
    println!(
        "party C: real mid-flight join turned a queued landing into a refused landing with no HP/poison/packet change"
    );
}

/// R54-B-3: a real guild formed through create/invite/join must never be a
/// `FrostCrunch` target, while a different guild and a guildless player stay
/// attackable until they really join.
#[test]
fn round54_frost_guild_real_cast_guard() {
    let data = test_game_data();
    let mut harness = Harness::with_data(301, data);
    let caster = harness.add_player(
        1,
        MirClass::Taoist,
        CASTER_LEVEL,
        2_000_000,
        Point::new(10, 10),
    );
    let mate = harness.add_player(2, MirClass::Warrior, CASTER_LEVEL, 0, Point::new(12, 10));
    let outsider = harness.add_player(
        3,
        MirClass::Warrior,
        CASTER_LEVEL,
        2_000_000,
        Point::new(10, 12),
    );
    let recruit = harness.add_player(4, MirClass::Warrior, CASTER_LEVEL, 0, Point::new(11, 11));
    harness.learn_frost(caster);

    // Two real guilds through the production create/invite/join path.
    let alpha = harness.create_guild(caster, "R54FrostAlpha");
    harness.join_guild(caster, mate);
    assert_eq!(
        harness.guild_id(caster),
        Some(alpha),
        "the caster must be in the first real guild"
    );
    assert_eq!(
        harness.guild_id(mate),
        Some(alpha),
        "the mate must be in the first real guild"
    );
    let beta = harness.create_guild(outsider, "R54FrostBeta");
    assert_ne!(alpha, beta, "the two guilds must be distinct entities");
    assert_eq!(
        harness.guild_id(outsider),
        Some(beta),
        "the outsider must be in the second real guild"
    );
    assert!(
        harness.guild_id(recruit).is_none(),
        "the recruit must start without a guild"
    );
    // Guild guards must be tested on their own: no party may be involved.
    assert!(
        !harness.world.in_group(caster)
            && !harness.world.in_group(mate)
            && !harness.world.in_group(outsider)
            && !harness.world.in_group(recruit),
        "no party may exist during the guild guard scenarios"
    );

    // Scenario A: Guild mode + real guild mate -> cast-time rejection.
    harness.ready_caster(caster);
    harness.ready_victim(mate);
    harness.set_attack_mode(caster, AttackMode::Guild);
    assert_eq!(
        harness
            .world
            .player(caster)
            .expect("caster exists")
            .attack_mode,
        AttackMode::Guild,
        "the caster must be in Guild attack mode"
    );
    assert_eq!(
        harness.guild_id(caster),
        harness.guild_id(mate),
        "the pair must share one real guild"
    );
    assert!(
        !harness.world.player(caster).expect("caster exists").dead
            && !harness.world.player(mate).expect("mate exists").dead,
        "both players must be alive"
    );
    assert_eq!(
        harness.world.player(caster).expect("caster exists").map,
        harness.world.player(mate).expect("mate exists").map,
        "both players must be on the same map"
    );
    assert!(
        !harness.world.in_safe_zone(caster) && !harness.world.in_safe_zone(mate),
        "neither player may stand in a safe zone"
    );
    assert!(
        !harness.world.map_no_fight(MAP),
        "the map must not be a no-fight map"
    );
    // Reason: the Guild arm of the attack-target predicate is the failing check.
    assert!(
        !harness.world.is_attack_target_player(caster, mate),
        "Guild mode must refuse a real guild mate as a target"
    );
    // Control flip: under Group mode the same pair is not grouped, so the
    // rejection can only come from the guild relationship.
    harness.set_attack_mode(caster, AttackMode::Group);
    assert!(
        harness.world.is_attack_target_player(caster, mate),
        "the same pair must be attackable under Group mode, so the Guild predicate is the cause"
    );
    harness.set_attack_mode(caster, AttackMode::Guild);

    let hp_before = harness.hp(mate);
    let mp_before = harness.world.player(caster).expect("caster exists").mp;
    harness.drain(mate);
    let report = harness.cast_frost(caster, mate);
    assert_eq!(
        report.target_id, 0,
        "a rejected guild cast must not report a target"
    );
    assert!(
        report.damage.is_none(),
        "a rejected guild cast must not roll damage"
    );
    assert!(
        harness.world.player(caster).expect("caster exists").mp < mp_before,
        "the client still pays the mana cost of a rejected guild cast"
    );
    assert_eq!(
        harness.pending_count(),
        0,
        "a rejected guild cast must not queue a delayed landing"
    );
    assert_eq!(
        harness.hp(mate),
        hp_before,
        "a rejected guild cast must not change the guild mate's HP"
    );
    harness.elapse(FROST_FLIGHT_MS);
    assert_eq!(
        harness.hp(mate),
        hp_before,
        "the delayed tick must not damage a guild mate"
    );
    assert!(
        harness.no_poisons(mate),
        "the delayed tick must not apply Slow/Frozen to a guild mate"
    );
    let rejected_packets = harness.packets(mate);
    assert_eq!(
        strike_packets(&rejected_packets),
        0,
        "a guild mate must receive no Struck/DamageIndicator packets (got {} packets)",
        rejected_packets.len()
    );
    assert_eq!(
        harness.pending_count(),
        0,
        "the delayed queue must stay empty after the guild refusal"
    );
    println!("guild A: Guild mode refused a real guild mate with no HP/poison/packet change");

    // Scenario B: a different guild is still attackable (positive control).
    harness.ready_caster(caster);
    harness.ready_victim(outsider);
    harness.set_attack_mode(caster, AttackMode::Guild);
    assert_ne!(
        harness.guild_id(caster),
        harness.guild_id(outsider),
        "the control target must belong to a different guild"
    );
    assert!(
        harness.world.is_attack_target_player(caster, outsider),
        "a member of a different guild must stay attackable under Guild mode"
    );
    let outsider_hp_before = harness.hp(outsider);
    harness.drain(outsider);
    let report = harness.cast_frost(caster, outsider);
    assert_eq!(
        report.target_id, outsider,
        "the different-guild member must be resolved as the cast target"
    );
    let dealt = report
        .damage
        .expect("accepted different-guild cast must roll damage");
    assert!(
        dealt > 0,
        "the different-guild hit must deal positive damage"
    );
    assert_eq!(
        harness.hp(outsider),
        outsider_hp_before,
        "HP must not move before the flight delay"
    );
    harness.elapse(FROST_FLIGHT_MS);
    assert_eq!(
        harness.hp(outsider),
        outsider_hp_before - dealt,
        "positive control: the different-guild member must take the delayed FrostCrunch damage"
    );
    let hit_packets = harness.packets(outsider);
    assert!(
        hit_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Struck(_))),
        "positive control expected S.Struck for the different-guild member"
    );
    assert!(
        hit_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::DamageIndicator(_))),
        "positive control expected S.DamageIndicator for the different-guild member"
    );
    println!("guild B: different-guild control took {dealt} damage with the expected hit packets");

    // Scenario C: successful cast + pending landing, then a real mid-flight
    // guild join, then the delayed tick must refuse at impact.
    harness.ready_caster(caster);
    harness.ready_victim(recruit);
    harness.set_attack_mode(caster, AttackMode::Guild);
    assert!(
        harness.guild_id(recruit).is_none(),
        "the causal target must start without a guild"
    );
    assert!(
        harness.world.is_attack_target_player(caster, recruit),
        "before the guild join the recruit must be a valid target"
    );
    let causal_hp_before = harness.hp(recruit);
    harness.drain(recruit);
    let report = harness.cast_frost(caster, recruit);
    assert_eq!(
        report.target_id, recruit,
        "the guild-join cast must be accepted and locked onto the target"
    );
    assert!(
        report.damage.is_some_and(|damage| damage > 0),
        "the causal guild cast must roll positive damage"
    );
    assert_eq!(
        harness.pending_count(),
        1,
        "a successful guild cast must queue exactly one delayed landing"
    );
    let pending_at = harness.world.pending_magic()[0].at;
    assert!(
        pending_at > harness.world.now(),
        "the delayed guild landing must be scheduled in the future (at {pending_at}, now {})",
        harness.world.now()
    );
    assert_eq!(
        harness.hp(recruit),
        causal_hp_before,
        "no damage before the delayed guild landing"
    );

    harness.join_guild(caster, recruit);
    assert_eq!(
        harness.guild_id(recruit),
        Some(alpha),
        "the mid-flight invite/join must put the recruit in the real guild"
    );
    assert_eq!(
        harness.guild_id(caster),
        harness.guild_id(recruit),
        "the mid-flight join must leave both players in one real guild"
    );
    assert!(
        !harness.world.is_attack_target_player(caster, recruit),
        "the landing recheck predicate must refuse the new guild mate"
    );
    // A real guild join refreshes the new member's stats, which legitimately
    // clamps the artificially boosted test HP to the level-60 base pool. That
    // is a stat refresh, not FrostCrunch damage, and the landing has not ticked
    // yet (it is still queued below).
    let hp_after_join = harness.hp(recruit);
    println!(
        "guild C: hp {causal_hp_before} -> {hp_after_join} after the real guild join (stat refresh), landing still queued"
    );
    assert_eq!(
        harness.pending_count(),
        1,
        "the delayed landing must still be queued after the guild join"
    );
    // Reset the victim pool after the stat refresh so the landing measurement
    // cannot inherit the clamp, then let the delayed landing tick.
    harness.ready_victim(recruit);

    harness.elapse(FROST_FLIGHT_MS);
    assert!(
        harness.world.now() >= pending_at,
        "the tick must cross the delayed guild landing time"
    );
    assert!(
        harness.world.pending_magic().is_empty(),
        "the delayed guild landing must be resolved by the tick"
    );
    assert_eq!(
        harness.hp(recruit),
        MAX_HP,
        "the landing recheck must refuse the damage after the real mid-flight guild join"
    );
    assert!(
        harness.no_poisons(recruit),
        "a refused guild landing must not apply Slow/Frozen"
    );
    let causal_packets = harness.packets(recruit);
    assert_eq!(
        strike_packets(&causal_packets),
        0,
        "a refused guild landing must not send Struck/DamageIndicator (got {} packets)",
        causal_packets.len()
    );
    println!(
        "guild C: real mid-flight join turned a queued landing into a refused landing with no HP/poison/packet change"
    );
}
