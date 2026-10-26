//! R58 S1: `FireBounce` terminal-hop RNG order (`Spell.FireBounce`, 154).
//!
//! Crystal `HumanObject.cs:5947-5956` selects the next target **before** spending the
//! hop budget:
//!
//! ```text
//! if (target.Race == ObjectType.Monster) {
//!     var targets = ((MonsterObject)target).FindAllNearby(3, target.CurrentLocation)
//!         .Where(x => x != target && x.IsAttackTarget(this) && target.CanFly(x.CurrentLocation))
//!         .ToList();
//!     if (targets.Count > 0) {
//!         var nextTarget = targets[Envir.Random.Next(targets.Count)];
//!         this.FireBounce(nextTarget, magic, target, --bounce);
//!     }
//! }
//! ```
//!
//! and `FireBounce` itself bails out at `:5813` when the decremented `bounce == 0`.
//! The final hop therefore consumes exactly one `MirRng` draw even though the drawn
//! target is discarded: no 125 broadcast and no damage is scheduled for it.
//!
//! The M4 test pins that draw with a control run. Both worlds use the same seed and are
//! asserted to share every draw up to the terminal hop, so the candidate run must produce
//! exactly one extra draw: its next value equals the control run's second next value.
//! Deleting the reorder (checking the budget first) makes the candidate run equal the
//! control run at the first value, so the shifted expectation fails deterministically.

mod common;

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, OnceLock};

use mir_core::Point;
use mir_core::enums::{
    AttackMode, MirClass, MirDirection, MirGender, Monster as MonsterImage, Spell,
};
use mir_core::rng::MirRng;
use mir_protocol::ServerPacket;
use mir_server::MonsterKind;
use mir_server::combat::{self, CombatStats};
use mir_server::gamedata::GameData;
use mir_server::world::{Player, World};
use tokio::sync::mpsc;

/// Synthetic map name: a custom name never hits safe-zone / no-PK map settings from the
/// default dataset.
const MAP: &str = "r58_rng_order";
/// Fixed world seed for every scenario.
const SEED: u64 = 20_261_026;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// Default dataset (`cjlaaa`): A0 proved `FireBounce(154)` exists in it.
fn game_data() -> Arc<GameData> {
    static DATA: OnceLock<Arc<GameData>> = OnceLock::new();
    DATA.get_or_init(|| {
        common::default_data()
            .expect("default dataset (cjlaaa) must be present: A0 proved FireBounce(154) exists")
    })
    .clone()
}

struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Harness {
    fn new(seed: u64) -> Self {
        let mut world = World::new(999);
        // Freeze the clock before the first cast: `World::now` reads the wall clock until
        // the first `advance_clock`, and a real-time delay between cast and `elapse`
        // could fire the pending hop early.
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
            name: format!("Player{object_id}"),
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

    /// Pin the `FireBounce` level (level-ups only read it; `learn_magic` accepts any level).
    fn set_magic_level(&mut self, object_id: u32, spell: Spell, level: u8) {
        let magic = self
            .world
            .player_mut(object_id)
            .expect("player")
            .magics
            .iter_mut()
            .find(|magic| magic.spell == spell)
            .expect("spell learned");
        magic.level = level;
    }

    /// Pin the caster attack roll: `MinMC == MaxMC` makes `attack_power` consume no RNG.
    fn set_magic_attack(&mut self, object_id: u32, min_mc: i32, max_mc: i32) {
        let player = self.world.player_mut(object_id).expect("player");
        player.stats.min_mc = min_mc;
        player.stats.max_mc = max_mc;
    }

    fn spawn(&mut self, kind: MonsterKind, position: Point) -> u32 {
        let object_id = self.world.spawn_monster(MAP, kind, position);
        if let Some(monster) = self.world.monster_mut(object_id) {
            // Trainer dummy (AI 56): never moves, never attacks, never draws RNG.
            monster.operate_time = -1;
            monster.regen_time = i64::MAX;
        }
        object_id
    }

    fn learn(&mut self, object_id: u32, spell: Spell) {
        self.world
            .learn_magic(object_id, spell)
            .expect("learn_magic succeeds");
    }

    /// Clear action / spell timers and refill MP (equivalent to waiting long enough).
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

    fn cast(&mut self, object_id: u32, target_id: u32, location: Point) {
        self.world
            .cast_magic(
                object_id,
                Spell::FireBounce,
                MirDirection::Right,
                target_id,
                location,
            )
            .expect("FireBounce cast");
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

    fn hp(&self, monster: u32) -> i32 {
        self.world
            .monster(monster)
            .expect("monster still exists")
            .hp
    }
}

/// One-hit test dummy: `hp` controls exactly how many hops a victim survives.
fn test_monster(hp: i32) -> MonsterKind {
    MonsterKind {
        name: "R58 RNG order dummy".to_owned(),
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

/// Collect 125: `(Spell, Source, Destination)`.
fn projectiles(packets: &[ServerPacket]) -> Vec<(Spell, u32, u32)> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectProjectile(projectile) => {
                Some((projectile.spell, projectile.source, projectile.destination))
            }
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Terminal-hop scenario
// ---------------------------------------------------------------------------

/// Position of the third monster, which only changes what the terminal hop can pick.
#[derive(Clone, Copy, Debug)]
enum TerminalCandidate {
    /// No third monster at all: `FindAllNearby(3)` returns an empty list.
    Absent,
    /// Third monster outside the radius-3 neighbourhood of both victims.
    Far,
    /// Third monster inside the radius-3 neighbourhood of the second victim only.
    Near,
}

struct TerminalRun {
    /// RNG state after the first hop settles.
    after_first_hop: MirRng,
    /// RNG state after the terminal hop settles.
    after_terminal_hop: MirRng,
    first_hop: Vec<(Spell, u32, u32)>,
    terminal_hop: Vec<(Spell, u32, u32)>,
    after_wait: Vec<(Spell, u32, u32)>,
    victim1: u32,
    victim2: u32,
    victim1_hp: i32,
    victim2_hp: i32,
    third_hp: i32,
}

/// Level 0 => bounce budget 2: first hop plus one secondary (terminal) hop.
///
/// Layout: caster (10,20), victim1 (15,20), victim2 (18,20), third monster either absent,
/// at (15,25) (`Far`: Chebyshev distance 5 from both victims) or at (20,20)
/// (`Near`: distance 2 from victim2, 5 from victim1, so only the terminal hop sees it).
fn run_terminal_hop(candidate: TerminalCandidate) -> TerminalRun {
    let mut harness = Harness::new(SEED);
    let caster = harness.add_player(1, MirClass::Wizard, 22, Point::new(10, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 22, Point::new(10, 26));
    harness.set_magic_attack(caster, 10, 10);
    harness.learn(caster, Spell::FireBounce);
    harness.set_magic_level(caster, Spell::FireBounce, 0);
    harness.ready_all(caster);
    let victim1 = harness.spawn(test_monster(1), Point::new(15, 20));
    let victim2 = harness.spawn(test_monster(1), Point::new(18, 20));
    let third = match candidate {
        TerminalCandidate::Absent => None,
        TerminalCandidate::Far => Some(harness.spawn(test_monster(1), Point::new(15, 25))),
        TerminalCandidate::Near => Some(harness.spawn(test_monster(1), Point::new(20, 20))),
    };
    // `spawn_monster` consumes RNG, so reset the stream after every spawn: from here on all
    // runs share one identical stream.
    harness.world.seed_rng(SEED);
    harness.drain(caster);
    harness.drain(observer);

    harness.cast(caster, victim1, Point::new(15, 20));
    // First hop: 500 + 5 * 50 = 750ms. Damage only; the first hop emits no 125.
    harness.elapse(750);
    let after_first_hop = harness.world.rng().clone();
    let first_hop = projectiles(&harness.packets(observer));
    assert_eq!(harness.hp(victim1), 0, "first victim is one-hit");

    // Terminal hop: distance 3 => 150ms. Its victim dies and the hop budget is exhausted,
    // so under Crystal order the next target is drawn and then discarded.
    harness.elapse(150);
    let after_terminal_hop = harness.world.rng().clone();
    let terminal_hop = projectiles(&harness.packets(observer));
    let victim2_hp = harness.hp(victim2);

    // Nothing may follow the discarded draw.
    harness.elapse(1_000);
    let after_wait = projectiles(&harness.packets(observer));
    let third_hp = third.map_or(0, |object_id| harness.hp(object_id));

    TerminalRun {
        after_first_hop,
        after_terminal_hop,
        first_hop,
        terminal_hop,
        after_wait,
        victim1,
        victim2,
        victim1_hp: harness.hp(victim1),
        victim2_hp,
        third_hp,
    }
}

// ---------------------------------------------------------------------------
// M4: terminal hop draws the next target before checking the budget
// ---------------------------------------------------------------------------

#[test]
fn terminal_hop_draws_next_target_before_checking_the_bounce_budget() {
    let control = run_terminal_hop(TerminalCandidate::Far);
    let candidate = run_terminal_hop(TerminalCandidate::Near);

    // Both runs share every draw before the terminal hop: the third monster is out of
    // range for the first hop in both, and only its position differs.
    assert_eq!(
        control.after_first_hop, candidate.after_first_hop,
        "both runs must share the same RNG state after the first hop"
    );
    assert_eq!(
        control.first_hop,
        vec![(Spell::FireBounce, control.victim1, control.victim2)],
        "the first hop emits exactly one 125 (source victim, destination next target)"
    );
    assert_eq!(
        control.first_hop, candidate.first_hop,
        "the first hop choice must not depend on the third monster"
    );
    assert_eq!(control.victim1_hp, 0, "first victim is one-hit");
    assert_eq!(candidate.victim1_hp, 0, "first victim is one-hit");

    // The terminal hop draws and discards its target: no 125, no damage.
    assert!(
        control.terminal_hop.is_empty(),
        "empty candidate list: the terminal hop emits no 125"
    );
    assert!(
        candidate.terminal_hop.is_empty(),
        "the terminal hop must not broadcast 125 for the discarded target"
    );
    assert!(
        control.after_wait.is_empty() && candidate.after_wait.is_empty(),
        "no 125 after the bounce budget is exhausted"
    );
    assert_eq!(control.victim2_hp, 0, "second victim is one-hit");
    assert_eq!(candidate.victim2_hp, 0, "second victim is one-hit");
    assert_eq!(control.third_hp, 1, "control run: third monster untouched");
    assert_eq!(
        candidate.third_hp, 1,
        "the discarded candidate must not be damaged"
    );

    // Exactly one extra RNG draw in the candidate run.
    assert_ne!(
        control.after_terminal_hop, candidate.after_terminal_hop,
        "Crystal order draws the next target even when the hop budget is exhausted"
    );

    // M4 (must be able to fail): under a fixed seed the candidate run's next value equals
    // the control run's *second* next value, i.e. the terminal hop consumed exactly one
    // extra `MirRng` draw. If the budget were checked before selecting (the pre-R58
    // order), the candidate run would never draw, both states would be equal, and this
    // assertion would compare equal values where a one-draw shift is required.
    let mut control_stream = control.after_terminal_hop.clone();
    let mut candidate_stream = candidate.after_terminal_hop.clone();
    let control_draws: Vec<u64> = (0..8).map(|_| control_stream.next_u64()).collect();
    let candidate_draws: Vec<u64> = (0..8).map(|_| candidate_stream.next_u64()).collect();
    assert_eq!(
        candidate_draws[0], control_draws[1],
        "M4: the terminal hop must consume exactly one extra MirRng draw"
    );
    assert_ne!(
        candidate_draws[0], control_draws[0],
        "M4: without the extra draw the candidate run would reuse the control value"
    );
    assert_eq!(
        &candidate_draws[..7],
        &control_draws[1..],
        "the whole RNG tail must be shifted by exactly one draw"
    );
}

#[test]
fn empty_candidate_list_consumes_no_rng_draw() {
    let absent = run_terminal_hop(TerminalCandidate::Absent);
    let far = run_terminal_hop(TerminalCandidate::Far);

    assert_eq!(
        absent.after_first_hop, far.after_first_hop,
        "removing the third monster must not change the shared prefix"
    );
    // Crystal guards the draw with `if (targets.Count > 0)`: an empty candidate list must
    // not advance the RNG, so dropping the monster entirely and placing it out of range
    // leave the stream in exactly the same state.
    assert_eq!(
        absent.after_terminal_hop, far.after_terminal_hop,
        "an empty candidate list must not consume an RNG draw"
    );
    assert_eq!(absent.victim2_hp, 0, "second victim is one-hit");
    assert_eq!(far.victim2_hp, 0, "second victim is one-hit");
}

// ---------------------------------------------------------------------------
// LOCK: level 3 => level + 2 hits and level + 1 projectiles
// ---------------------------------------------------------------------------

#[test]
fn level_three_chain_keeps_level_plus_two_hits_and_level_plus_one_projectiles() {
    let mut harness = Harness::new(SEED);
    let caster = harness.add_player(1, MirClass::Wizard, 22, Point::new(10, 20));
    let observer = harness.add_player(2, MirClass::Wizard, 22, Point::new(10, 26));
    harness.set_magic_attack(caster, 10, 10);
    harness.learn(caster, Spell::FireBounce);
    harness.set_magic_level(caster, Spell::FireBounce, 3);
    harness.ready_all(caster);

    // Five 1 HP monsters spaced 3 apart on the x axis: after each kill the previous corpse
    // is excluded by `IsAttackTarget`, so the radius-3 candidate list is a single target
    // and the chain path is fully determined.
    let victims = [
        Point::new(15, 20),
        Point::new(18, 20),
        Point::new(21, 20),
        Point::new(24, 20),
        Point::new(27, 20),
    ]
    .map(|position| harness.spawn(test_monster(1), position));
    harness.drain(caster);
    harness.drain(observer);

    harness.cast(caster, victims[0], Point::new(15, 20));

    // First hop: 500 + 5 * 50 = 750ms; damage only, then 125 (victim0 -> victim1).
    let mut chain: Vec<(Spell, u32, u32)> = Vec::new();
    harness.elapse(750);
    assert_eq!(harness.hp(victims[0]), 0, "first victim killed");
    let packets = harness.packets(observer);
    chain.extend(projectiles(&packets));
    assert_eq!(
        projectiles(&packets),
        vec![(Spell::FireBounce, victims[0], victims[1])],
        "125 source is the previous victim, destination the next target"
    );
    assert_eq!(
        harness.hp(victims[1]),
        1,
        "125 is broadcast before the next hit lands"
    );

    // Secondary hops 1..=3: each settlement kills its victim and broadcasts one 125.
    for index in 1..4 {
        harness.elapse(150);
        assert_eq!(
            harness.hp(victims[index]),
            0,
            "previous victim killed by its own hop"
        );
        let packets = harness.packets(observer);
        assert_eq!(
            projectiles(&packets),
            vec![(Spell::FireBounce, victims[index], victims[index + 1])],
            "each secondary hop emits exactly one 125 before its damage"
        );
        chain.extend(projectiles(&packets));
        assert_eq!(
            harness.hp(victims[index + 1]),
            1,
            "125 must arrive before the next hit lands"
        );
    }

    // Terminal hop (the fifth hit): budget exhausted after it, no more 125.
    harness.elapse(150);
    assert_eq!(
        harness.hp(victims[4]),
        0,
        "the fifth and last victim is hit"
    );
    assert!(
        projectiles(&harness.packets(observer)).is_empty(),
        "no 125 after the bounce budget is exhausted"
    );
    harness.elapse(1_000);
    assert!(
        projectiles(&harness.packets(observer)).is_empty(),
        "no 125 after the chain has stopped"
    );

    // LOCK numbers: level + 2 = 5 hits and level + 1 = 4 projectiles.
    let mut hits = 0_u8;
    for (index, victim) in victims.iter().enumerate() {
        assert_eq!(harness.hp(*victim), 0, "victim {index} must be hit");
        hits += 1;
    }
    assert_eq!(hits, 5, "level 3 hits level + 2 targets");
    assert_eq!(chain.len(), 4, "level 3 emits level + 1 projectiles");
}
