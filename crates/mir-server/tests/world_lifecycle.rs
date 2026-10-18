//! 世界生命周期四件套的端到端测试：**刷怪重生 / 经验升级 / 死亡复活 / 自然回血**。
//!
//! 全部确定性：世界随机数固定种子、时间用 [`World::advance_clock`] 快进（不 sleep）、
//! 怪物定时器用 [`Harness::wake`] 归零。每个断言都对应 Crystal 的一处行为，注释写了出处。

use std::collections::{BTreeSet, HashMap};

use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, Monster as MonsterImage};
use mir_protocol::ServerPacket;
use mir_server::combat::{self, CombatStats};
use mir_server::world::{PLAYER_REGEN_DELAY_MS, PLAYER_REVIVE_DELAY_MS};
use mir_server::{MonsterKind, Player, SpawnGroup, World};
use tokio::sync::mpsc;

const MAP: &str = "0";
/// 1 级战士的生命上限（`combat::player_base_stats(1, Warrior)`）。
const WARRIOR_HP: i32 = 18;
/// 1 级战士的魔法上限。
const WARRIOR_MP: i32 = 14;

/// 测试世界 + 每个玩家的收包通道。
struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Harness {
    fn new(seed: u64) -> Self {
        let mut world = World::new(999);
        world.seed_rng(seed);
        // 先把时钟冻结在「世界创建瞬间」：`World::enter` 里 `regen_time = now + 10s`，
        // 而 `advance_clock(9999)` 会先读一次实时推进过几毫秒的 `now()`；不冻结的话
        // 「入场后立刻快进 9999ms」在繁忙的 CI/并行测试里可能刚好越过 10s 触发回血
        // （断言「未到点不回血」偶发失败）。冻结后 `now()` 只由偏移决定，完全可复现。
        world.advance_clock(0);
        Self {
            world,
            receivers: HashMap::new(),
        }
    }

    /// 放一个满血满蓝的 1 级战士进世界，返回对象 ID。
    ///
    /// 满血是刻意的：让「自然回血」只影响专门测它的用例。
    fn add_player(&mut self, object_id: u32, position: Point) -> u32 {
        let stats = combat::player_base_stats(1, MirClass::Warrior);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.enter(Player {
            object_id,
            session_id: u64::from(object_id),
            character_id: i64::from(object_id),
            name: format!("玩家{object_id}"),
            map: MAP.to_owned(),
            position,
            direction: MirDirection::Down,
            level: 1,
            experience: 0,
            max_experience: 0,
            class: MirClass::Warrior,
            gender: mir_core::enums::MirGender::Male,
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
            visible_monsters: BTreeSet::new(),
            visible_npcs: BTreeSet::new(),
            movement_time: 0,
            inventory: mir_server::items::new_inventory(),
            equipment: mir_server::items::new_equipment(),
            magics: Vec::new(),
            magic: mir_server::MagicState::default(),
            gold: 0,
            credit: 0,
            visible_items: BTreeSet::new(),
            quests: Vec::new(),
            completed_quests: BTreeSet::new(),
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
            sender,
        });
        self.receivers.insert(object_id, receiver);
        object_id
    }

    /// 刷一只怪并「立即唤醒」它（所有定时器归零，下一次 tick 就会思考）。
    fn spawn(&mut self, kind: MonsterKind, position: Point) -> u32 {
        let object_id = self.world.spawn_monster(MAP, kind, position);
        self.wake(object_id);
        object_id
    }

    /// 把所有定时器清零（理由同 `tests/monster_combat.rs`：调度条件是严格大于）。
    fn wake(&mut self, object_id: u32) {
        if let Some(monster) = self.world.monster_mut(object_id) {
            monster.operate_time = -1;
            monster.action_time = -1;
            monster.move_time = -1;
            monster.attack_time = -1;
            monster.search_time = -1;
            monster.roam_time = -1;
            monster.alone_time = -1;
            monster.regen_time = i64::MAX;
        }
    }

    fn ready_player(&mut self, object_id: u32) {
        if let Some(player) = self.world.player_mut(object_id) {
            player.action_time = 0;
            player.attack_time = 0;
        }
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

    fn has(&mut self, object_id: u32, matcher: impl Fn(&ServerPacket) -> bool) -> bool {
        self.packets(object_id).iter().any(matcher)
    }

    fn alive_monsters(&self) -> usize {
        self.world
            .monsters()
            .iter()
            .filter(|monster| !monster.dead)
            .count()
    }

    /// 把玩家挪到怪物左边一格，返回攻击方向。
    fn stand_next_to(&mut self, player: u32, monster: u32) -> MirDirection {
        let target = self.world.monster(monster).expect("怪物还在").position;
        let from = Point::new(target.x - 1, target.y);
        if let Some(p) = self.world.player_mut(player) {
            p.position = from;
            p.direction = MirDirection::Right;
        }
        from.direction_to(target)
    }

    /// 走到怪物旁边、一击必杀（伤害 300ms 后落地，与 Crystal 的延迟结算一致）。
    fn kill_monster(&mut self, player: u32, monster: u32) {
        let direction = self.stand_next_to(player, monster);
        if let Some(p) = self.world.player_mut(player) {
            p.stats.min_dc = 100;
            p.stats.max_dc = 100;
            p.stats.accuracy = 100;
        }
        self.ready_player(player);
        self.world
            .attack(player, direction)
            .expect("攻击应被接受")
            .expect("前方有怪");
        self.world.advance_clock(300);
        self.world.tick();
        assert!(
            self.world.monster(monster).is_none_or(|m| m.dead),
            "一击 100 点应打死测试怪"
        );
    }
}

/// 测试怪物：准确 100（必中）、敏捷 0、攻击力由参数决定、经验由参数决定。
fn test_monster(hp: i32, min_dc: i32, max_dc: i32, experience: u32) -> MonsterKind {
    MonsterKind {
        name: "测试怪".to_owned(),
        image: MonsterImage::Oma,
        ai: 0,
        level: 1,
        view_range: 7,
        cool_eye: 0,
        attack_speed_ms: 2_500,
        move_speed_ms: 1_800,
        experience,
        can_push: true,
        stats: CombatStats {
            hp,
            min_dc,
            max_dc,
            max_ac: 0,
            accuracy: 100,
            agility: 0,
            ..CombatStats::default()
        },
    }
}

// ---------------------------------------------------------------------------
// 1. 刷怪点重生（Crystal `MapRespawn` / `RespawnTimer`）
// ---------------------------------------------------------------------------

#[test]
fn register_spawn_group_fills_count_and_reports_state() {
    let mut harness = Harness::new(21);
    let player = harness.add_player(1, Point::new(10, 10));
    let spawned = harness.world.register_spawn_group(
        SpawnGroup::new(MAP, test_monster(10, 0, 0, 10), Point::new(40, 40), 3, 4).with_delay(1, 0),
    );
    assert_eq!(spawned.len(), 3, "登记时立刻铺满 count");

    let state = harness.world.respawn_state();
    assert_eq!(state.len(), 1);
    assert_eq!(state[0].count, 3);
    assert_eq!(state[0].alive, 3);
    assert_eq!(state[0].next_spawn_at, None, "不缺怪就没有排程");
    assert_eq!(state[0].center, Point::new(40, 40));
    assert_eq!(state[0].monster, "测试怪");
    // 刷出来的怪都记得自己属于哪个刷怪组（Crystal `MonsterObject.Respawn`）。
    assert!(
        harness
            .world
            .monsters()
            .iter()
            .all(|monster| monster.respawn_group == Some(0)),
        "刷怪组刷出来的怪必须带上组下标"
    );
    let _ = player;
}

#[test]
fn killing_one_monster_respawns_one_after_delay_and_never_exceeds_count() {
    let mut harness = Harness::new(22);
    let player = harness.add_player(1, Point::new(10, 10));
    let ids = harness.world.register_spawn_group(
        SpawnGroup::new(MAP, test_monster(10, 0, 0, 0), Point::new(40, 40), 2, 4).with_delay(1, 0),
    );
    assert_eq!(harness.alive_monsters(), 2);
    let victim = ids[0];
    let other = ids[1];
    harness.packets(player);

    harness.kill_monster(player, victim);
    let state = &harness.world.respawn_state()[0];
    assert_eq!(state.alive, 1, "死 1 只 → 存活 1 只");
    let delay = state.next_spawn_at.expect("应排出下一次刷新") - harness.world.now();
    assert_eq!(delay, 60_000, "间隔 1 分钟（Crystal `Settings.Minute`）");

    // 差 1ms：还没到点。
    harness.world.advance_clock(59_999);
    harness.world.tick();
    assert_eq!(harness.world.respawn_state()[0].alive, 1, "未到点不补怪");
    assert_eq!(harness.alive_monsters(), 1, "世界里的活怪还是 1 只");
    assert!(
        harness.world.respawn_state()[0].next_spawn_at.is_some(),
        "没补满就继续排程"
    );

    // 到点：补 1 只。
    harness.world.advance_clock(1);
    harness.world.tick();
    let state = &harness.world.respawn_state()[0];
    assert_eq!(state.alive, 2, "到点补 1 只，回到 count");
    assert_eq!(state.next_spawn_at, None, "补满就不再排程");
    assert_eq!(harness.alive_monsters(), 2);

    // 再等很久也不会超过 count（Crystal 的 `respawn.Count < Info.Count * SpawnMultiplier` 判定）。
    harness.world.advance_clock(600_000);
    harness.world.tick();
    assert_eq!(harness.world.respawn_state()[0].alive, 2, "不会超过 count");
    assert_eq!(harness.alive_monsters(), 2);
    assert!(harness.world.monster(other).is_some(), "没被牵连的那只还在");
}

#[test]
fn random_delay_stays_inside_crystals_window() {
    // Crystal：`delay = max(1, Delay - RandomDelay + Random.Next(RandomDelay * 2))`（`Map.cs:759`）
    // → 间隔落在 `[Delay - RandomDelay, Delay + RandomDelay - 1]` 分钟。
    let mut harness = Harness::new(23);
    let player = harness.add_player(1, Point::new(10, 10));
    let ids = harness.world.register_spawn_group(
        SpawnGroup::new(MAP, test_monster(10, 0, 0, 0), Point::new(40, 40), 1, 1).with_delay(3, 2),
    );
    harness.kill_monster(player, ids[0]);

    let state = &harness.world.respawn_state()[0];
    let minutes = (state.next_spawn_at.expect("有排程") - harness.world.now()) / 60_000;
    assert!(
        (1..=4).contains(&minutes),
        "Delay 3 ± RandomDelay 2 → 1..=4 分钟，实际 {minutes}"
    );

    // `RandomDelay == 0` 时公式退化成固定 Delay（`.NET` 的 `Next(0)` 不抽签）。
    let mut strict = Harness::new(24);
    strict.add_player(1, Point::new(10, 10));
    let ids = strict.world.register_spawn_group(
        SpawnGroup::new(MAP, test_monster(10, 0, 0, 0), Point::new(40, 40), 1, 1).with_delay(2, 0),
    );
    strict.kill_monster(1, ids[0]);
    let state = &strict.world.respawn_state()[0];
    assert_eq!(
        state.next_spawn_at.expect("有排程") - strict.world.now(),
        120_000,
        "RandomDelay 0 → 固定 2 分钟"
    );
}

#[test]
fn respawn_tick_scale_stretches_the_interval() {
    let mut harness = Harness::new(25);
    let player = harness.add_player(1, Point::new(10, 10));
    harness.world.set_respawn_tick_scale(2.0);
    assert!((harness.world.respawn_tick_scale() - 2.0).abs() < f64::EPSILON);
    let ids = harness.world.register_spawn_group(
        SpawnGroup::new(MAP, test_monster(10, 0, 0, 0), Point::new(40, 40), 1, 1).with_delay(1, 0),
    );
    harness.kill_monster(player, ids[0]);

    harness.world.advance_clock(60_000);
    harness.world.tick();
    assert_eq!(harness.alive_monsters(), 0, "倍率 2 倍时 1 分钟还不够");
    harness.world.advance_clock(60_000);
    harness.world.tick();
    assert_eq!(harness.alive_monsters(), 1, "2 分钟才补");
}

// ---------------------------------------------------------------------------
// 2. 经验与升级（Crystal `PlayerObject.GainExp` / `LevelUp`）
// ---------------------------------------------------------------------------

#[test]
fn killing_a_monster_grants_experience_and_notifies_only_the_killer() {
    let mut harness = Harness::new(31);
    let killer = harness.add_player(1, Point::new(20, 20));
    let bystander = harness.add_player(2, Point::new(18, 20));
    let monster = harness.spawn(test_monster(10, 0, 0, 50), Point::new(21, 20));
    harness.packets(killer);
    harness.packets(bystander);

    harness.kill_monster(killer, monster);

    let player = harness.world.player(killer).unwrap();
    // level 1 打 level 1 的怪：没有等级差衰减（`ReduceExp`，`HumanObject.cs:2369`）。
    assert_eq!(player.experience, 50);
    assert_eq!(player.level, 1, "50 < 100 不升级");
    assert_eq!(player.max_experience, 100, "兜底经验表 1 级 = 1^3 * 100");

    let packets = harness.packets(killer);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::GainExperience(gain) if gain.amount == 50
        )),
        "击杀者应收到 S.GainExperience{{50}}（`PlayerObject.cs:893`），实际 {packets:?}"
    );
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::LevelChanged(_))),
        "没升级就不该有 LevelChanged，实际 {packets:?}"
    );
    let observed = harness.packets(bystander);
    assert!(
        !observed
            .iter()
            .any(|packet| matches!(packet, ServerPacket::GainExperience(_))),
        "经验包只给本人（Enqueue，不是 Broadcast），实际 {observed:?}"
    );
}

#[test]
fn high_level_killer_gets_reduced_experience_from_a_weak_monster() {
    // `ReduceExp`（`HumanObject.cs:2365-2381`）：level 20 打 level 1：
    // step = max(150/15, 1) = 10；over = 20 - (1 + 10) = 9；penalty = round(90) = 90 → 150-90 = 60。
    let mut harness = Harness::new(32);
    let killer = harness.add_player(1, Point::new(20, 20));
    if let Some(player) = harness.world.player_mut(killer) {
        player.level = 20;
        player.stats = combat::player_base_stats(20, MirClass::Warrior);
    }
    let monster = harness.spawn(test_monster(10, 0, 0, 150), Point::new(21, 20));
    harness.packets(killer);

    harness.kill_monster(killer, monster);

    assert_eq!(harness.world.player(killer).unwrap().experience, 60);
    assert!(harness.has(killer, |packet| matches!(
        packet,
        ServerPacket::GainExperience(gain) if gain.amount == 60
    )));
}

#[test]
fn enough_experience_levels_up_recalculates_stats_and_sends_packets() {
    let mut harness = Harness::new(33);
    let killer = harness.add_player(1, Point::new(20, 20));
    let observer = harness.add_player(2, Point::new(18, 20));
    let monster = harness.spawn(test_monster(10, 0, 0, 120), Point::new(21, 20));
    harness.packets(killer);
    harness.packets(observer);

    harness.kill_monster(killer, monster);

    let player = harness.world.player(killer).unwrap();
    assert_eq!(player.level, 2, "120 ≥ 100 → 升到 2 级");
    assert_eq!(
        player.experience, 20,
        "溢出经验保留（`PlayerObject.cs:921`）"
    );
    assert_eq!(player.max_experience, 800, "2 级的兜底需求 2^3 * 100");
    let expected = combat::player_base_stats(2, MirClass::Warrior);
    assert_eq!(player.stats, expected, "升级后按等级曲线重算属性");
    assert_eq!(player.hp, expected.hp, "`SetHP(Stats[HP])` 补满");
    assert_eq!(player.mp, expected.mp, "`SetMP(Stats[MP])` 补满");

    let packets = harness.packets(killer);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::LevelChanged(level)
                if level.level == 2 && level.experience == 20 && level.max_experience == 800
        )),
        "本人应收到 S.LevelChanged（`PlayerObject.cs:938`），实际 {packets:?}"
    );
    let health: Vec<&ServerPacket> = packets
        .iter()
        .filter(|packet| matches!(packet, ServerPacket::HealthChanged(_)))
        .collect();
    assert!(
        health.len() >= 2,
        "SetHP 与 SetMP 各发一条 HealthChanged（`HumanObject.cs:848-849`），实际 {} 条",
        health.len()
    );
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectLeveled(_))),
        "ObjectLeveled 是广播给周围玩家的，本人不该收到，实际 {packets:?}"
    );
    let observed = harness.packets(observer);
    assert!(
        observed.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectLeveled(leveled) if leveled.object_id == killer
        )),
        "旁观者应收到 S.ObjectLeveled（`HumanObject.cs:851`），实际 {observed:?}"
    );
}

#[test]
fn master_experience_table_comes_from_the_index_sample_when_present() {
    // 生产路径用 `ExpList.ini` 的真表；测试里显式确认「有文件时首级需求是 100」。
    use mir_server::exp::ExpTable;
    let table = ExpTable::discover();
    assert_eq!(
        table.exp_for_level(1),
        100,
        "Crystal 样本与兜底曲线首级都是 100"
    );
    let mut world = World::new(999);
    world.set_exp_table(table);
    assert_eq!(world.exp_table().exp_for_level(1), 100);
}

// ---------------------------------------------------------------------------
// 3. 死亡与复活（Crystal `PlayerObject.Die` / `HumanObject.Revive`）
// ---------------------------------------------------------------------------

#[test]
fn player_dies_then_revives_in_place_and_can_walk_again() {
    let mut harness = Harness::new(41);
    let victim = harness.add_player(1, Point::new(20, 20));
    let observer = harness.add_player(2, Point::new(18, 20));
    // 一击 99 点，玩家只有 18 血。
    harness.spawn(test_monster(10, 99, 99, 0), Point::new(21, 20));
    harness.packets(victim);
    harness.packets(observer);

    harness.world.tick();
    harness.world.advance_clock(300);
    harness.world.tick();

    let player = harness.world.player(victim).unwrap();
    assert!(player.dead, "应该被打死");
    assert_eq!(player.hp, 0);
    let death_time = player.death_time;
    assert!(death_time > 0);
    assert_eq!(
        player.revive_time,
        death_time + PLAYER_REVIVE_DELAY_MS,
        "复活时刻 = 死亡时刻 + PLAYER_REVIVE_DELAY_MS"
    );
    let position = player.position;

    // 死亡期间不能走（`MoveError::Dead`）。
    harness.ready_player(victim);
    assert!(harness.world.walk(victim, MirDirection::Up).is_err());

    // 差 1ms：还不复活。
    harness.world.advance_clock(PLAYER_REVIVE_DELAY_MS - 1);
    harness.world.tick();
    assert!(harness.world.player(victim).unwrap().dead, "未到点不复活");

    // 到点：原地满血满蓝复活。
    harness.world.advance_clock(1);
    harness.world.tick();
    let player = harness.world.player(victim).unwrap();
    assert!(!player.dead, "到点自动复活");
    assert_eq!(
        player.position, position,
        "原地复活（`HumanObject.cs:1703-1704`）"
    );
    assert_eq!(player.hp, player.stats.hp, "HP 补满");
    assert_eq!(player.mp, player.stats.mp, "MP 补满");

    let packets = harness.packets(victim);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Revived(_))),
        "本人应收到 S.Revived（`HumanObject.cs:1719`），实际 {packets:?}"
    );
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::HealthChanged(health) if health.hp == WARRIOR_HP
        )),
        "复活时 SetHP 会发 HealthChanged，实际 {packets:?}"
    );
    let observed = harness.packets(observer);
    assert!(
        observed.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectRemove(remove) if remove.object_id == victim
        )),
        "旁观者应收到 ObjectRemove（`HumanObject.cs:1697-1698`），实际 {observed:?}"
    );
    assert!(
        observed.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectRevived(revived) if revived.object_id == victim
        )),
        "旁观者应收到 ObjectRevived（`HumanObject.cs:1721`），实际 {observed:?}"
    );
    assert!(
        observed.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectPlayer(player) if player.object_id == victim && !player.dead
        )),
        "旁证者应收到复活后的外观包，实际 {observed:?}"
    );

    // 复活后能正常走一步。
    harness.ready_player(victim);
    let moved = harness
        .world
        .walk(victim, MirDirection::Left)
        .expect("复活后可以走");
    assert_eq!(moved, Point::new(position.x - 1, position.y));
}

// ---------------------------------------------------------------------------
// 4. 自然回血回蓝（Crystal `HumanObject.ProcessRegen`）
// ---------------------------------------------------------------------------

#[test]
fn wounded_idle_player_regenerates_after_the_delay_and_receives_health_changed() {
    let mut harness = Harness::new(51);
    let player = harness.add_player(1, Point::new(20, 20));
    if let Some(p) = harness.world.player_mut(player) {
        p.hp = 10; // 上限 18
        p.mp = 0; // 上限 14
    }
    harness.packets(player);

    // 入场时排的第一个周期还没到。
    harness.world.advance_clock(PLAYER_REGEN_DELAY_MS - 1);
    harness.world.tick();
    assert_eq!(harness.world.player(player).unwrap().hp, 10, "未到点不回血");

    harness.world.advance_clock(1);
    harness.world.tick();
    let p = harness.world.player(player).unwrap();
    // `(int)(18 * 0.03F) + 1 = 1`（`HumanObject.cs:562`）。
    assert_eq!(p.hp, 11, "回复 floor(HP上限 * 3%) + 1");
    assert_eq!(p.mp, 1, "魔法同式（`HumanObject.cs:568`）");
    assert_eq!(
        p.regen_time,
        harness.world.now() + PLAYER_REGEN_DELAY_MS,
        "回完再排下一个 10s"
    );

    let packets = harness.packets(player);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::HealthChanged(health) if health.hp == 11)),
        "回血后应收到 HealthChanged（Crystal 没有 MPChanged 包），实际 {packets:?}"
    );
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::DamageIndicator(indicator)
                if indicator.damage == 1 && indicator.object_id == player
        )),
        "回血还会广播 +1 飘字（`HumanObject.cs:638`），实际 {packets:?}"
    );
}

#[test]
fn full_health_player_resets_the_timer_without_healing() {
    let mut harness = Harness::new(52);
    let player = harness.add_player(1, Point::new(20, 20));
    harness.packets(player);
    let before = harness.world.player(player).unwrap().regen_time;

    harness.world.advance_clock(PLAYER_REGEN_DELAY_MS);
    harness.world.tick();

    let p = harness.world.player(player).unwrap();
    assert_eq!(p.hp, WARRIOR_HP);
    assert_eq!(p.mp, WARRIOR_MP);
    assert!(
        p.regen_time > before,
        "即使满血，`RegenTime` 也会被推进（`HumanObject.cs:556-558`）"
    );
    assert!(
        harness
            .packets(player)
            .iter()
            .all(|packet| !matches!(packet, ServerPacket::HealthChanged(_))),
        "满血不该发 HealthChanged"
    );
}

#[test]
fn attacking_postpones_regen_but_walking_does_not() {
    let mut harness = Harness::new(53);
    let player = harness.add_player(1, Point::new(20, 20));
    if let Some(p) = harness.world.player_mut(player) {
        p.hp = 10;
    }
    // 攻击（挥空也算）会把 `RegenTime` 推后一个完整周期（`HumanObject.cs:2976`）。
    harness.ready_player(player);
    harness
        .world
        .attack(player, MirDirection::Up)
        .expect("挥空");
    let after_attack = harness.world.player(player).unwrap().regen_time;
    // 容差 5ms：`now()` 读的是真实时钟，`attack()` 与断言之间可能跨过一毫秒
    // （并行跑全量测试时尤其明显），这里只验证「被推后了一个完整周期」。
    let expected = harness.world.now() + PLAYER_REGEN_DELAY_MS;
    assert!(
        (after_attack - expected).abs() <= 5,
        "出手后重置回复计时：{after_attack} vs {expected}"
    );

    // 走路**不会**重置回复计时——这是 Crystal 的原样行为：
    // `RegenTime` 只在攻击/施法/受击时被推后（`HumanObject.cs:2976`、`:3450`、`:7196-7367`），
    // `Walk`/`Run` 里没有这一行。所以「移动中不回血」在本作里只体现为「移动本身不回血」。
    harness.ready_player(player);
    harness.world.walk(player, MirDirection::Left).expect("走");
    assert_eq!(
        harness.world.player(player).unwrap().regen_time,
        after_attack,
        "走路不改变回复计时（与 Crystal 一致）"
    );

    // 移动 5 秒：这一步一步都不会回血。
    for _ in 0..5 {
        harness.world.advance_clock(1_000);
        harness.world.tick();
        harness.ready_player(player);
        let _ = harness.world.walk(player, MirDirection::Left);
    }
    assert!(
        harness.world.player(player).unwrap().hp <= 10,
        "走路期间没有额外回血"
    );
}
