//! R53-F: `PvP` `FrostCrunch` against real player targets.
//!
//! Crystal reference (`github/Crystal/Server/MirObjects/HumanObject.cs`):
//!
//! * `:5928` — the delayed action only lands when the target is still within
//!   `Functions.InRange(target.CurrentLocation, targetLocation, 2)`.
//! * `:5929` — `target.Attacked(this, value, DefenceType.MAC, false) > 0` gates the poison rolls.
//! * `:5961-5996` — player targets use `Level + 2 >= target.Level`, a `Random.Next(100)`
//!   denominator, Slow `Duration = 4`, Frozen `Duration = 2`, `TickSpeed = 1000`, `OperateTime = 0`.
//! * `:7092-7221` — `HumanObject.Attacked(HumanObject)` returns `damage - armour` and calls `ChangeHP`.
//!
//! Every case below goes through the real `World::cast_magic` entry point followed by real
//! `World::tick` slices; no `PoisonState` is injected to fake a hit.

mod common;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{AttackMode, MirClass, MirDirection, MirGender, PoisonType, Spell};
use mir_db::{MapInfo, MirDb, SafeZoneInfo};
use mir_protocol::ServerPacket;
use mir_server::MonsterKind;
use mir_server::combat::{self, CombatStats};
use mir_server::gamedata::GameData;
use mir_server::magic::{CastReport, PoisonState};
use mir_server::world::{CombatError, Player, World};
use tokio::sync::mpsc;

const MAP: &str = "0";
const SAFE_MAP: &str = "safe";
const NO_FIGHT_MAP: &str = "nofight";

/// Victim HP used by every player-target case (high enough to survive a hit).
const MAX_HP: i32 = 100_000;
/// `FrostCrunch` flight delay is `MaxDistance * 50 + 500`; distance 2 lands after 600ms.
const FROST_FLIGHT_MS: i64 = 700;
/// `PlayerObject.IsAttackTarget` safe-zone cell used by the mid-flight scenario.
const SAFE_CELL: Point = Point::new(14, 10);

const SAMPLE_SEEDS: [u64; 4] = [3, 5, 7, 11];
/// 1200 real casts in total: with the player denominator `Random.Next(100)` and magic level 3
/// the expected hit count is `1200 * (3 + 1) / 100 = 48` per poison. The admissible window in
/// the probability case separates that rate from the monster denominators (`Next(20)` = 240
/// hits, `Next(40)` = 120 hits).
const SAMPLES_PER_SEED: u32 = 300;

// ---------------------------------------------------------------------------
// Game data (real default `game.mnd` magic record + custom synthetic maps)
// ---------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn default_game_data() -> Arc<GameData> {
    let path = repo_root().join("game-assets/data/game.mnd");
    assert!(
        path.is_file(),
        "default game.mnd is required for the real FrostCrunch record: {}",
        path.display()
    );
    Arc::new(GameData::load_mnd(&path).expect("default game.mnd should load"))
}

/// Real default magic record plus three synthetic maps: normal, safe zone, no-fight.
fn test_game_data() -> Arc<GameData> {
    let base = default_game_data();
    let magic = base
        .magic(Spell::FrostCrunch)
        .expect("FrostCrunch magic info must exist in game.mnd")
        .clone();
    let normal = MapInfo {
        index: 1,
        file_name: MAP.to_owned(),
        title: "Frost Field".to_owned(),
        ..MapInfo::default()
    };
    let mut safe = MapInfo {
        index: 2,
        file_name: SAFE_MAP.to_owned(),
        title: "Frost Safe Zone".to_owned(),
        ..MapInfo::default()
    };
    safe.safe_zones.push(SafeZoneInfo {
        location: SAFE_CELL,
        size: 0,
        start_point: false,
    });
    let mut no_fight = MapInfo {
        index: 3,
        file_name: NO_FIGHT_MAP.to_owned(),
        title: "Frost No Fight".to_owned(),
        ..MapInfo::default()
    };
    no_fight.no_fight = true;
    let db = MirDb {
        maps: vec![normal, safe, no_fight],
        magics: vec![magic],
        ..MirDb::default()
    };
    Arc::new(GameData::from_db(db, None))
}

/// Training dummy (AI 56): never moves, never attacks, keeps the frozen player's front cell busy.
fn dummy_kind() -> MonsterKind {
    MonsterKind {
        name: "R53 Frost Dummy".to_owned(),
        image: mir_core::enums::Monster::Oma,
        ai: 56,
        level: 1,
        view_range: 1,
        cool_eye: 0,
        attack_speed_ms: 2_500,
        move_speed_ms: 1_800,
        experience: 0,
        can_push: true,
        stats: CombatStats {
            hp: 10_000,
            min_mac: 0,
            max_mac: 0,
            ..CombatStats::default()
        },
    }
}

// ---------------------------------------------------------------------------
// Test harness (real World entry points, deterministic clock + RNG)
// ---------------------------------------------------------------------------

struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Harness {
    fn with_data(seed: u64, data: Arc<GameData>) -> Self {
        let mut world = World::new(999);
        // Freeze the wall clock before any `now()` call, exactly like round52.
        world.advance_clock(0);
        world.seed_rng(seed);
        world.set_game_data(data);
        for map in [MAP, SAFE_MAP, NO_FIGHT_MAP] {
            world.insert_map(map, common::synthetic_map(map));
        }
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
        map: &str,
        position: Point,
    ) -> u32 {
        let stats = combat::player_base_stats(level, class);
        let (sender, receiver) = mpsc::unbounded_channel();
        let name = format!("Player{object_id}");
        self.world.enter(Player {
            object_id,
            session_id: u64::from(object_id),
            character_id: i64::from(object_id),
            name,
            map: map.to_owned(),
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

    /// Learn `FrostCrunch` and pin it to level 3 (so the probability rolls have a real level).
    fn learn_frost(&mut self, object_id: u32) {
        self.world
            .learn_magic(object_id, Spell::FrostCrunch)
            .expect("FrostCrunch should be learnable");
        let Some(player) = self.world.player_mut(object_id) else {
            panic!("caster {object_id} must exist");
        };
        let Some(magic) = player
            .magics
            .iter_mut()
            .find(|magic| magic.spell == Spell::FrostCrunch)
        else {
            panic!("FrostCrunch must be known after learning it");
        };
        magic.level = 3;
        magic.experience = 0;
    }

    /// Reset caster timers/cooldowns and give it a deterministic magic attack.
    fn ready_caster(&mut self, object_id: u32) {
        let Some(player) = self.world.player_mut(object_id) else {
            panic!("caster {object_id} must exist");
        };
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

    /// Reset victim HP/poison/defence so every sample starts from the same observable state.
    fn ready_victim(&mut self, object_id: u32) {
        let Some(player) = self.world.player_mut(object_id) else {
            panic!("victim {object_id} must exist");
        };
        player.hp = MAX_HP;
        player.stats.hp = MAX_HP;
        player.dead = false;
        player.magic.poisons.clear();
        player.stats.min_mac = 0;
        player.stats.max_mac = 0;
        player.stats.min_ac = 0;
        player.stats.max_ac = 0;
        player.stats.magic_resist = 0;
        player.stats.energy_shield_percent = 0;
        player.stats.energy_shield_hp_gain = 0;
        player.stats.damage_reduction_percent = 0;
        player.stats.reflect = 0;
        player.stats.agility = 0;
    }

    fn set_attack_mode(&mut self, object_id: u32, mode: AttackMode) {
        let Some(player) = self.world.player_mut(object_id) else {
            panic!("player {object_id} must exist");
        };
        player.attack_mode = mode;
    }

    fn spawn_dummy(&mut self, position: Point) -> u32 {
        let object_id = self.world.spawn_monster(MAP, dummy_kind(), position);
        if let Some(monster) = self.world.monster_mut(object_id) {
            monster.operate_time = -1;
            monster.regen_time = i64::MAX;
        }
        object_id
    }

    fn elapse(&mut self, ms: i64) {
        self.world.advance_clock(ms);
        self.world.tick();
    }

    /// Real cast: target is resolved by the world, the packet location is the target's cell.
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
            .expect("frost crunch cast should be accepted")
    }

    fn cast_frost_on_self(&mut self, caster: u32) -> CastReport {
        let position = self.world.player(caster).expect("caster exists").position;
        self.world
            .cast_magic(
                caster,
                Spell::FrostCrunch,
                MirDirection::Right,
                caster,
                position,
            )
            .expect("self-target frost crunch cast should still be accepted")
    }

    /// Real casts only: retry fixed-seed `FrostCrunch` until the player target receives `kind`.
    fn cast_until_poison(&mut self, caster: u32, victim: u32, kind: PoisonType) -> PoisonState {
        for _ in 0..600 {
            self.ready_caster(caster);
            self.ready_victim(victim);
            let report = self.cast_frost(caster, victim);
            assert!(
                report.damage.is_some(),
                "real FrostCrunch must damage a PK-eligible player target"
            );
            self.elapse(FROST_FLIGHT_MS);
            if let Some(poison) = self.poison(victim, kind) {
                return poison;
            }
        }
        panic!("FrostCrunch did not apply {kind:?} within 600 real casts");
    }

    fn hp(&self, object_id: u32) -> i32 {
        self.world.player(object_id).expect("player exists").hp
    }

    fn monster_hp(&self, object_id: u32) -> i32 {
        self.world.monster(object_id).expect("monster exists").hp
    }

    fn poison(&self, object_id: u32, kind: PoisonType) -> Option<PoisonState> {
        self.world.player(object_id).and_then(|player| {
            player
                .magic
                .poisons
                .iter()
                .find(|poison| poison.kind == kind)
                .copied()
        })
    }

    fn has_poison(&self, object_id: u32, kind: PoisonType) -> bool {
        self.world.player(object_id).is_some_and(|player| {
            player
                .magic
                .poisons
                .iter()
                .any(|poison| poison.kind == kind)
        })
    }

    fn no_poisons(&self, object_id: u32) -> bool {
        self.world
            .player(object_id)
            .is_some_and(|player| player.magic.poisons.is_empty())
    }

    fn packets(&mut self, object_id: u32) -> Vec<ServerPacket> {
        let receiver = self.receivers.get_mut(&object_id).expect("player channel");
        let mut packets = Vec::new();
        while let Ok(frame) = receiver.try_recv() {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            packets.push(ServerPacket::decode(index, &frame[4..]).expect("decode server packet"));
        }
        packets
    }

    fn drain(&mut self, object_id: u32) {
        self.packets(object_id);
    }
}

/// Two PK-eligible players on a synthetic map: caster at (10,10), victim at (12,10).
fn pk_harness(
    seed: u64,
    data: &Arc<GameData>,
    caster_level: u16,
    victim_level: u16,
    map: &str,
) -> Harness {
    let mut h = Harness::with_data(seed, Arc::clone(data));
    h.add_player(1, MirClass::Taoist, caster_level, map, Point::new(10, 10));
    h.add_player(2, MirClass::Warrior, victim_level, map, Point::new(12, 10));
    h.learn_frost(1);
    h.ready_caster(1);
    h.ready_victim(2);
    h.set_attack_mode(1, AttackMode::All);
    h
}

// ---------------------------------------------------------------------------
// 1) A PK-eligible player is a real FrostCrunch target: MAC damage lands after the flight delay.
// ---------------------------------------------------------------------------

#[test]
fn round53_frost_crunch_hits_pk_eligible_player() {
    let data = test_game_data();
    let mut h = pk_harness(7, &data, 60, 60, MAP);
    let hp_before = h.hp(2);
    h.drain(2);

    let report = h.cast_frost(1, 2);
    assert_eq!(report.spell, Spell::FrostCrunch);
    assert_eq!(
        report.target_id, 2,
        "a PK-eligible player must be a real target"
    );
    let dealt = report
        .damage
        .expect("frost crunch must report real damage for a player target");
    assert!(
        dealt > 0,
        "reported player damage must be positive: {dealt}"
    );
    assert_eq!(
        h.hp(2),
        hp_before,
        "HP must not change before the delayed impact is resolved"
    );
    assert!(h.no_poisons(2), "no poison may be applied at cast time");

    h.elapse(FROST_FLIGHT_MS);
    assert_eq!(
        h.hp(2),
        hp_before - dealt,
        "player target must lose exactly the reported MAC damage"
    );
    let packets = h.packets(2);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Struck(_))),
        "victim must receive S.Struck for a real player hit"
    );
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::DamageIndicator(_))),
        "victim must receive an S.DamageIndicator for a real player hit"
    );
}

// ---------------------------------------------------------------------------
// 2) The target is re-validated at impact: PK mode, group membership and safe zones are refreshed.
// ---------------------------------------------------------------------------

#[test]
fn round53_frost_crunch_rechecks_pk_at_impact() {
    let data = test_game_data();

    // 2a) Caster leaves PK mode mid-flight.
    let mut h = pk_harness(11, &data, 60, 60, MAP);
    assert_eq!(h.cast_frost(1, 2).target_id, 2);
    let hp_before = h.hp(2);
    h.set_attack_mode(1, AttackMode::Peace);
    h.elapse(FROST_FLIGHT_MS);
    assert_eq!(
        h.hp(2),
        hp_before,
        "target must not be damaged once the caster leaves PK mode before impact"
    );
    assert!(h.no_poisons(2), "refused impact must not apply Slow/Frozen");
    assert!(!h.has_poison(2, PoisonType::Frozen));

    // 2b) Caster and victim join the same group mid-flight.
    let mut h = pk_harness(12, &data, 60, 60, MAP);
    h.set_attack_mode(1, AttackMode::Group);
    assert_eq!(h.cast_frost(1, 2).target_id, 2);
    let hp_before = h.hp(2);
    h.world
        .group_invite(1, "Player2")
        .expect("group invite should be accepted");
    h.world
        .group_join(2, true)
        .expect("group join should be accepted");
    assert_eq!(
        h.world.group_id_of(1),
        h.world.group_id_of(2),
        "both players must share a group before impact"
    );
    assert!(
        h.world.group_id_of(1).is_some(),
        "group must exist before the impact check"
    );
    h.elapse(FROST_FLIGHT_MS);
    assert_eq!(h.hp(2), hp_before, "same-group members must not be hit");
    assert!(h.no_poisons(2));

    // 2c) Victim steps into a safe-zone cell mid-flight (still within the 2-cell tolerance).
    let mut h = pk_harness(13, &data, 60, 60, SAFE_MAP);
    assert_eq!(h.cast_frost(1, 2).target_id, 2);
    let hp_before = h.hp(2);
    assert!(
        h.world.place_player(2, SAFE_MAP, SAFE_CELL),
        "victim must be teleported into the safe-zone cell"
    );
    assert!(
        h.world.in_safe_zone(2),
        "victim must be inside the safe zone before impact"
    );
    h.elapse(FROST_FLIGHT_MS);
    assert_eq!(
        h.hp(2),
        hp_before,
        "a target that entered a safe zone must not take delayed magic damage"
    );
    assert!(h.no_poisons(2), "safe-zone refusal must not apply poison");
}

// ---------------------------------------------------------------------------
// 3) Player target level gate (+2) and the real `Random.Next(100)` probability rolls.
// ---------------------------------------------------------------------------

#[test]
fn round53_frost_crunch_player_level_and_probability_edges() {
    let data = test_game_data();

    // 3a) Level gate: caster 60 + 2 < victim 63 -> damage lands, but no poison is rolled/applied.
    let mut h = pk_harness(21, &data, 60, 63, MAP);
    let hp_before = h.hp(2);
    let report = h.cast_frost(1, 2);
    let dealt = report
        .damage
        .expect("damage must still land on a higher-level player target");
    h.elapse(FROST_FLIGHT_MS);
    assert_eq!(
        h.hp(2),
        hp_before - dealt,
        "level gate must not block the MAC damage itself"
    );
    assert!(
        h.no_poisons(2),
        "caster level 60 + 2 < victim level 63 must skip both poison rolls"
    );

    // 3b) Probability edges over fixed seeds. The player branch rolls `Random.Next(100)` for both
    // poisons (`HumanObject.cs:5971,5982`), so with magic level 3 each cast hits with probability
    // `(3 + 1) / 100 = 4%`. A monster denominator (`Next(20)` = 20%, `Next(40)` = 10%) would push
    // the observed hit count far above `1.5 * expected` and must fail the admissibility check.
    let total = SAMPLES_PER_SEED * u32::try_from(SAMPLE_SEEDS.len()).expect("seed count fits");
    let expected_hits = total * 4 / 100; // 48 hits out of 1200 at 4%
    let admissible = expected_hits / 2..=expected_hits * 3 / 2; // [24, 72], about 3.5 sigma each side
    let mut slow_hits = 0_u32;
    let mut slow_misses = 0_u32;
    let mut frozen_hits = 0_u32;
    let mut frozen_misses = 0_u32;
    for seed in SAMPLE_SEEDS {
        let mut h = pk_harness(seed, &data, 60, 60, MAP);
        let mut seed_slow_hits = 0_u32;
        let mut seed_frozen_hits = 0_u32;
        for _ in 0..SAMPLES_PER_SEED {
            h.ready_caster(1);
            h.ready_victim(2);
            let report = h.cast_frost(1, 2);
            assert!(
                report.damage.is_some(),
                "real FrostCrunch must damage a PK-eligible player target"
            );
            h.elapse(FROST_FLIGHT_MS);
            if let Some(poison) = h.poison(2, PoisonType::Slow) {
                slow_hits += 1;
                seed_slow_hits += 1;
                assert_eq!(
                    poison.duration, 4,
                    "player Slow duration must be 4 ticks (4s at 1000ms)"
                );
                assert_eq!(poison.tick_speed_ms, 1_000);
            } else {
                slow_misses += 1;
            }
            if let Some(poison) = h.poison(2, PoisonType::Frozen) {
                frozen_hits += 1;
                seed_frozen_hits += 1;
                assert_eq!(
                    poison.duration, 2,
                    "player Frozen duration must be 2 ticks (2s at 1000ms)"
                );
                assert_eq!(poison.tick_speed_ms, 1_000);
            } else {
                frozen_misses += 1;
            }
        }
        println!(
            "frost crunch player-target seed {seed}: slow {seed_slow_hits}/{SAMPLES_PER_SEED} hit, frozen {seed_frozen_hits}/{SAMPLES_PER_SEED} hit"
        );
    }
    println!(
        "frost crunch player-target totals over {total} real casts: slow {slow_hits} hit / {slow_misses} miss, frozen {frozen_hits} hit / {frozen_misses} miss; expected {expected_hits} hits at the player 4% rate, admissible {admissible:?}"
    );
    assert_eq!(
        slow_hits + slow_misses,
        total,
        "every sample must be counted"
    );
    assert_eq!(
        frozen_hits + frozen_misses,
        total,
        "every sample must be counted"
    );
    assert!(slow_hits > 0, "Slow success side must be observed");
    assert!(slow_misses > 0, "Slow failure side must be observed");
    assert!(frozen_hits > 0, "Frozen success side must be observed");
    assert!(frozen_misses > 0, "Frozen failure side must be observed");
    assert!(
        admissible.contains(&slow_hits),
        "Slow hit count must follow the player Random.Next(100) denominator: observed {slow_hits} of {total} casts, expected {expected_hits}, admissible {admissible:?} (the monster Next(20) denominator would give about 240 hits)"
    );
    assert!(
        admissible.contains(&frozen_hits),
        "Frozen hit count must follow the player Random.Next(100) denominator: observed {frozen_hits} of {total} casts, expected {expected_hits}, admissible {admissible:?} (the monster Next(40) denominator would give about 120 hits)"
    );
}

// ---------------------------------------------------------------------------
// 4) Frozen from a real player-target FrostCrunch blocks a real `World::attack` until it expires.
// ---------------------------------------------------------------------------

#[test]
fn round53_frozen_by_frost_blocks_real_player_attack() {
    let data = test_game_data();
    let mut h = pk_harness(31, &data, 60, 60, MAP);
    let dummy = h.spawn_dummy(Point::new(13, 10));

    let frozen = h.cast_until_poison(1, 2, PoisonType::Frozen);
    assert_eq!(frozen.kind, PoisonType::Frozen);
    assert_eq!(frozen.duration, 2, "player Frozen duration must be 2s");
    assert_eq!(frozen.tick_speed_ms, 1_000);
    assert!(h.has_poison(2, PoisonType::Frozen));

    let now = h.world.now();
    assert!(
        !h.world.player(2).expect("victim").can_attack(now),
        "a frozen player must not be allowed to attack"
    );
    let error = h
        .world
        .attack(2, MirDirection::Right)
        .expect_err("World::attack must reject a frozen player");
    assert!(
        matches!(error, CombatError::Frozen { object_id: 2 }),
        "frozen attack rejection must be CombatError::Frozen, got {error:?}"
    );
    assert_eq!(
        h.world.player(2).expect("victim").action_time,
        0,
        "a rejected frozen attack must not consume the action timer"
    );

    // Frozen ticks once per 1000ms: still frozen after one tick, gone after the second.
    h.elapse(1_001);
    assert!(h.has_poison(2, PoisonType::Frozen), "frozen must last 2s");
    h.elapse(1_001);
    assert!(
        !h.has_poison(2, PoisonType::Frozen),
        "Frozen must expire after two 1000ms ticks (2s)"
    );

    h.ready_caster(2);
    let hp_before = h.monster_hp(dummy);
    let attack = h
        .world
        .attack(2, MirDirection::Right)
        .expect("attacking after thaw must be accepted")
        .expect("the dummy in front must be a target");
    assert_eq!(attack.target, dummy);
    h.elapse(350);
    assert_eq!(
        h.monster_hp(dummy),
        hp_before - attack.damage,
        "thawed player attack must deal its reported damage to the dummy"
    );
}

// ---------------------------------------------------------------------------
// 5) Cast-time rejections never leave a delayed action or poison behind.
// ---------------------------------------------------------------------------

#[test]
fn round53_frost_crunch_cast_time_rejections_without_effect() {
    let data = test_game_data();
    let mut cases: Vec<(&str, Harness)> = Vec::new();

    // 5a) No-fight map (`MapInfo::no_fight`).
    let no_fight = pk_harness(51, &data, 60, 60, NO_FIGHT_MAP);
    assert!(no_fight.world.map_no_fight(NO_FIGHT_MAP));
    cases.push(("no-fight map", no_fight));

    // 5b) Attacker inside a safe zone.
    let mut safe_zone = pk_harness(52, &data, 60, 60, SAFE_MAP);
    assert!(safe_zone.world.place_player(1, SAFE_MAP, SAFE_CELL));
    assert!(
        safe_zone
            .world
            .place_player(2, SAFE_MAP, Point::new(16, 10))
    );
    assert!(safe_zone.world.in_safe_zone(1));
    cases.push(("attacker in safe zone", safe_zone));

    // 5c) Peace attack mode (default).
    let mut peace = pk_harness(53, &data, 60, 60, MAP);
    peace.set_attack_mode(1, AttackMode::Peace);
    cases.push(("peace mode", peace));

    // 5d) Both already in the same group.
    let mut grouped = pk_harness(54, &data, 60, 60, MAP);
    grouped.set_attack_mode(1, AttackMode::Group);
    grouped
        .world
        .group_invite(1, "Player2")
        .expect("group invite should be accepted");
    grouped
        .world
        .group_join(2, true)
        .expect("group join should be accepted");
    assert_eq!(
        grouped.world.group_id_of(1),
        grouped.world.group_id_of(2),
        "players must be grouped for the cast-time rejection case"
    );
    cases.push(("same group", grouped));

    for (label, h) in &mut cases {
        let hp_before = h.hp(2);
        let mp_before = h.world.player(1).expect("caster").mp;
        let report = h.cast_frost(1, 2);
        assert_eq!(
            report.target_id, 0,
            "{label}: the player target must be rejected at cast time"
        );
        assert!(
            report.damage.is_none(),
            "{label}: a rejected cast must not queue damage"
        );
        assert!(
            h.world.player(1).expect("caster").mp < mp_before,
            "{label}: Crystal order still pays the mana cost for an unattackable target"
        );
        h.elapse(FROST_FLIGHT_MS);
        assert_eq!(
            h.hp(2),
            hp_before,
            "{label}: rejected cast must not damage the player later"
        );
        assert!(
            h.no_poisons(2),
            "{label}: rejected cast must not apply Slow/Frozen"
        );
    }

    // 5e) Self target is never a valid FrostCrunch target.
    let mut self_cast = pk_harness(55, &data, 60, 60, MAP);
    let report = self_cast.cast_frost_on_self(1);
    assert_eq!(
        report.target_id, 0,
        "self target must be rejected at cast time"
    );
    let hp_before = self_cast.hp(1);
    self_cast.elapse(FROST_FLIGHT_MS);
    assert_eq!(self_cast.hp(1), hp_before);
    assert!(self_cast.no_poisons(1));
}

// ---------------------------------------------------------------------------
// 6) Impact-time rejections: map change, death and leaving the 2-cell tolerance.
// ---------------------------------------------------------------------------

#[test]
fn round53_frost_crunch_impact_rejects_changed_map_death_and_shift() {
    let data = test_game_data();

    // 6a) Victim changes map mid-flight.
    let mut h = pk_harness(61, &data, 60, 60, MAP);
    assert_eq!(h.cast_frost(1, 2).target_id, 2);
    let hp_before = h.hp(2);
    assert!(h.world.place_player(2, SAFE_MAP, Point::new(12, 10)));
    h.elapse(FROST_FLIGHT_MS);
    assert_eq!(
        h.hp(2),
        hp_before,
        "a target that changed map must not take delayed magic damage"
    );
    assert!(h.no_poisons(2));

    // 6b) Victim dies before the delayed impact.
    let mut h = pk_harness(62, &data, 60, 60, MAP);
    assert_eq!(h.cast_frost(1, 2).target_id, 2);
    let now = h.world.now();
    {
        let victim = h.world.player_mut(2).expect("victim");
        victim.dead = true;
        victim.hp = 0;
        victim.death_time = now;
    }
    assert!(
        !h.world.is_attack_target_player(1, 2),
        "a dead player must not be an attack target"
    );
    h.elapse(FROST_FLIGHT_MS);
    assert_eq!(
        h.hp(2),
        0,
        "a dead target must not take delayed magic damage"
    );
    assert!(h.no_poisons(2), "a dead target must not receive poison");

    // 6c) Victim moves 2 -> 3 cells away from the cast location: outside `InRange(..., 2)`.
    let mut h = pk_harness(63, &data, 60, 60, MAP);
    assert_eq!(h.cast_frost(1, 2).target_id, 2);
    let hp_before = h.hp(2);
    assert!(h.world.place_player(2, MAP, Point::new(15, 10)));
    h.elapse(FROST_FLIGHT_MS);
    assert_eq!(
        h.hp(2),
        hp_before,
        "moving to 3 cells from the cast location must dodge the delayed hit"
    );
    assert!(h.no_poisons(2));

    // 6d) Boundary: moving to exactly 2 cells from the cast location still hits.
    let mut h = pk_harness(64, &data, 60, 60, MAP);
    let report = h.cast_frost(1, 2);
    let dealt = report.damage.expect("boundary cast must report damage");
    let hp_before = h.hp(2);
    assert!(h.world.place_player(2, MAP, Point::new(14, 10)));
    h.elapse(FROST_FLIGHT_MS);
    assert_eq!(
        h.hp(2),
        hp_before - dealt,
        "moving to exactly 2 cells from the cast location must still hit"
    );
}

// ---------------------------------------------------------------------------
// 7) Real player Slow/Frozen durations and expiry on the tick clock.
// ---------------------------------------------------------------------------

#[test]
fn round53_frost_poison_durations_and_expiry() {
    let data = test_game_data();

    // Slow: 4 ticks of 1000ms -> still slowed after 3 ticks, gone on the 4th.
    let mut h = pk_harness(41, &data, 60, 60, MAP);
    let slow = h.cast_until_poison(1, 2, PoisonType::Slow);
    assert_eq!(slow.kind, PoisonType::Slow);
    assert_eq!(
        slow.duration, 4,
        "player Slow duration must be 4 ticks (4s)"
    );
    assert_eq!(slow.tick_speed_ms, 1_000);
    assert!(
        h.has_poison(2, PoisonType::Slow),
        "Slow must be active on hit"
    );
    for tick in 1..=3 {
        h.elapse(1_001);
        assert!(
            h.has_poison(2, PoisonType::Slow),
            "Slow must still be active after tick {tick} of 4"
        );
    }
    h.elapse(1_001);
    assert!(
        !h.has_poison(2, PoisonType::Slow),
        "Slow must expire after four 1000ms ticks (4s)"
    );

    // Frozen: 2 ticks of 1000ms -> still frozen after 1 tick, gone on the 2nd.
    let mut h = pk_harness(43, &data, 60, 60, MAP);
    let frozen = h.cast_until_poison(1, 2, PoisonType::Frozen);
    assert_eq!(frozen.kind, PoisonType::Frozen);
    assert_eq!(
        frozen.duration, 2,
        "player Frozen duration must be 2 ticks (2s)"
    );
    assert_eq!(frozen.tick_speed_ms, 1_000);
    h.elapse(1_001);
    assert!(
        h.has_poison(2, PoisonType::Frozen),
        "Frozen must still be active after tick 1 of 2"
    );
    h.elapse(1_001);
    assert!(
        !h.has_poison(2, PoisonType::Frozen),
        "Frozen must expire after two 1000ms ticks (2s)"
    );
}
