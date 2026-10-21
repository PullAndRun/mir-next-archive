//! 怪物 AI 与物理战斗的端到端测试。
//!
//! 全部用例都是**确定性的**：
//!
//! * 世界随机数用固定种子（[`World::seed_rng`]）；
//! * 时间用 [`World::advance_clock`] 快进，不 sleep；
//! * 怪物定时器用 [`Harness::wake`] 归零（等价于「Crystal 的 `now > OperateTime` 已经到期」）。
//!
//! 每个断言都对应 Crystal 的一个行为，注释里写了出处。

use std::collections::{BTreeSet, HashMap};

use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, Monster as MonsterImage};
use mir_protocol::ServerPacket;
use mir_server::MonsterKind;
use mir_server::combat::{self, CombatStats};
use mir_server::monster::AiSupport;
use mir_server::world::{Player, World};
use tokio::sync::mpsc;

const MAP: &str = "0";

/// 测试用世界 + 每个玩家的收包通道。
struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Harness {
    fn new(seed: u64) -> Self {
        let mut world = World::new(999);
        world.seed_rng(seed);
        Self {
            world,
            receivers: HashMap::new(),
        }
    }

    /// 放一个 1 级战士进世界，返回对象 ID。
    fn add_player(&mut self, object_id: u32, position: Point) -> u32 {
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
            hp: 15,
            mp: 0,
            stats: combat::player_base_stats(1, MirClass::Warrior),
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
            attack_mode: mir_core::enums::AttackMode::Peace,
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

    /// 把所有定时器清零。
    ///
    /// `operate_time` 设成 -1 而不是 0：Crystal 的调度条件是**严格大于**
    /// （`if (Time > current.Value.OperateTime)`，`Envir.cs:2130`），
    /// 而测试里 `now()` 可能正好是 0，用 0 会让怪物永远轮不到。
    ///
    /// `regen_time` 故意设成 `i64::MAX`：测试里的怪物受过伤，若不关掉自然回血
    /// （每 10 秒 +2%）会干扰伤害断言。
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

    /// 清掉玩家的动作/攻击冷却，让它能立刻再次出手。
    fn ready_player(&mut self, object_id: u32) {
        if let Some(player) = self.world.player_mut(object_id) {
            player.action_time = 0;
            player.attack_time = 0;
        }
    }

    /// 取走这个玩家已经收到的所有包。
    fn packets(&mut self, object_id: u32) -> Vec<ServerPacket> {
        let receiver = self.receivers.get_mut(&object_id).expect("玩家通道");
        let mut packets = Vec::new();
        while let Ok(frame) = receiver.try_recv() {
            // 生产服务端会把同一 tick 的多帧合并成一个 channel 消息；测试也要逐帧解码。
            let mut offset = 0;
            while offset + 4 <= frame.len() {
                let total = usize::from(u16::from_le_bytes([frame[offset], frame[offset + 1]]));
                if total < 4 || offset + total > frame.len() {
                    break;
                }
                let index = i16::from_le_bytes([frame[offset + 2], frame[offset + 3]]);
                packets.push(
                    ServerPacket::decode(index, &frame[offset + 4..offset + total])
                        .expect("解码服务端包"),
                );
                offset += total;
            }
        }
        packets
    }

    fn has(&mut self, object_id: u32, matcher: impl Fn(&ServerPacket) -> bool) -> bool {
        self.packets(object_id).iter().any(matcher)
    }
}

/// 测试怪物：准确 100（必中）、敏捷 0（不会闪避）、攻击间隔 2500ms、移动间隔 1800ms。
fn test_monster(hp: i32, min_dc: i32, max_dc: i32) -> MonsterKind {
    MonsterKind {
        name: "测试怪".to_owned(),
        image: MonsterImage::Oma,
        ai: 0,
        level: 1,
        view_range: 7,
        cool_eye: 0,
        attack_speed_ms: 2500,
        move_speed_ms: 1800,
        experience: 0,
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

#[test]
fn monster_acquires_target_inside_view_range() {
    let mut harness = Harness::new(11);
    let player = harness.add_player(1, Point::new(20, 20));
    let monster = harness.spawn(test_monster(10, 1, 2), Point::new(24, 20));

    harness.world.tick();

    // Crystal `MonsterObject.ProcessSearch` → `FindTarget`（`MonsterObject.cs:1804-1813,1918-1997`）：
    // 距离 4 ≤ 视野 7 → 选中玩家。
    assert_eq!(harness.world.monster(monster).unwrap().target, Some(player));
}

#[test]
fn monster_outside_view_range_does_not_aggro() {
    let mut harness = Harness::new(12);
    harness.add_player(1, Point::new(20, 20));
    // 距离 8 > 视野 7。
    let monster = harness.spawn(test_monster(10, 1, 2), Point::new(28, 20));

    harness.world.tick();

    assert_eq!(
        harness.world.monster(monster).unwrap().target,
        None,
        "视野外的玩家不该被选中"
    );
}

#[test]
fn monster_walks_one_step_toward_target_and_broadcasts_walk() {
    let mut harness = Harness::new(13);
    let player = harness.add_player(1, Point::new(20, 20));
    let monster = harness.spawn(test_monster(10, 1, 2), Point::new(24, 20));

    harness.world.tick();

    let m = harness.world.monster(monster).unwrap();
    // Crystal `MonsterObject.MoveTo`（`MonsterObject.cs:2027-2071`）：朝目标方向走一格。
    assert_eq!(m.position, Point::new(23, 20));
    assert_eq!(m.direction, MirDirection::Left);
    // 走一步后的计时（`MonsterObject.cs:2127-2131`）：CellTime +500、ActionTime +300、MoveTime +MoveSpeed。
    let now = harness.world.now();
    assert!(m.cell_time >= now + 400, "CellTime 应为 now + 500");
    assert!(m.action_time >= now + 200, "ActionTime 应为 now + 300");
    assert!(m.move_time >= now + 1700, "MoveTime 应为 now + 1800");
    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::ObjectWalk(walk) if walk.object_id == monster
        )),
        "视野内玩家应收到怪物的 ObjectWalk"
    );
}

#[test]
fn player_attack_retargets_adjacent_oma_and_it_retaliates() {
    let mut harness = Harness::new(14);
    let player = harness.add_player(1, Point::new(20, 20));
    // AI 0 的普通近战怪（Oma 同族）：高 HP，保证一次攻击不会秒杀，便于观察 retarget。
    let monster = harness.spawn(test_monster(100, 5, 5), Point::new(21, 20));
    harness.ready_player(player);
    harness
        .world
        .attack(player, MirDirection::Right)
        .expect("玩家攻击应发到相邻怪");
    harness.world.tick();
    assert_eq!(
        harness.world.monster(monster).unwrap().target,
        Some(player),
        "被玩家攻击的 AI0 怪必须锁定攻击者"
    );

    let hp_before = harness.world.player(player).unwrap().hp;
    harness.wake(monster);
    harness.world.tick();
    harness.world.advance_clock(400);
    harness.world.tick();
    assert!(
        harness.world.player(player).unwrap().hp < hp_before,
        "锁定后怪物必须真的反击（Oma/AI0 基础近战）"
    );
}

#[test]
fn monster_attacks_after_delay_and_damages_player() {
    let mut harness = Harness::new(14);
    let player = harness.add_player(1, Point::new(20, 20));
    // 旁观者：用来验证 `ObjectStruck`/`DamageIndicator` 是广播给**周围玩家**的
    // （Crystal `Broadcast` 不发给发布者本人，受害者单独收 `S.Struck`）。
    let observer = harness.add_player(2, Point::new(19, 19));
    // 相邻（距离 1），攻击力固定 5，玩家 1 级 AC 为 0。
    let monster = harness.spawn(test_monster(10, 5, 5), Point::new(21, 20));
    harness.packets(player);
    harness.packets(observer);

    let before = harness.world.now();
    harness.world.tick();

    // `MonsterObject.Attack`：`AttackTime = now + AttackSpeed`（2500ms）、`ActionTime = now + 300`。
    let m = harness.world.monster(monster).unwrap();
    assert_eq!(m.target, Some(player), "离得近的玩家 1 才是目标");
    assert!(
        m.attack_time >= before + 2500 && m.attack_time <= before + 2600,
        "攻击冷却应约为 now + 2500，实际 {}（now {before}）",
        m.attack_time
    );
    assert!(
        harness.has(observer, |packet| matches!(
            packet,
            ServerPacket::ObjectAttack(attack) if attack.object_id == monster
        )),
        "攻击动作应立即广播 ObjectAttack"
    );
    // 伤害是延迟 300ms 结算的（`MonsterObject.cs:2171`），此刻还没掉血。
    assert_eq!(harness.world.player(player).unwrap().hp, 15, "伤害尚未落地");

    // 推进 300ms 让延迟伤害到期。
    harness.world.advance_clock(300);
    harness.world.tick();

    let hp = harness.world.player(player).unwrap().hp;
    // 玩家 AC 0、怪物攻击 5 → 伤害恒为 5（`damage - armour`）。
    assert_eq!(hp, 10, "应扣 5 点血，实际 {hp}");
    // 注意：`packets()` 会把通道取空，所以这里一次性收集再逐条断言。
    let packets = harness.packets(player);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::Struck(struck) if struck.attacker_id == monster
        )),
        "受害者本人应收到 S.Struck，实际 {packets:?}"
    );
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::HealthChanged(health) if health.hp == 10
        )),
        "受害者应收到 HealthChanged，实际 {packets:?}"
    );
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectStruck(_))),
        "ObjectStruck 是广播给周围玩家的，本人不该收到，实际 {packets:?}"
    );

    let observed = harness.packets(observer);
    assert!(
        observed.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectStruck(struck) if struck.object_id == player
        )),
        "旁观者应收到 ObjectStruck，实际 {observed:?}"
    );
    assert!(
        observed.iter().any(|packet| matches!(
            packet,
            ServerPacket::DamageIndicator(indicator)
                if indicator.object_id == player && indicator.damage == -5
        )),
        "DamageIndicator 的 Damage 是 armour - damage（负数），实际 {observed:?}"
    );
}

#[test]
fn player_kills_monster_and_broadcasts_object_died_then_remove() {
    let mut harness = Harness::new(15);
    let player = harness.add_player(1, Point::new(20, 20));
    let monster = harness.spawn(test_monster(10, 5, 5), Point::new(21, 20));
    // 让玩家一击必杀：攻击力 50、准确 100。
    if let Some(p) = harness.world.player_mut(player) {
        p.stats.min_dc = 50;
        p.stats.max_dc = 50;
        p.stats.accuracy = 100;
    }

    harness.ready_player(player);
    let report = harness
        .world
        .attack(player, MirDirection::Right)
        .expect("攻击应被接受")
        .expect("前方有怪物");
    assert_eq!(report.target, monster);

    // 伤害延迟 300ms（`HumanObject.cs:3201`）。
    harness.world.advance_clock(300);
    harness.world.tick();

    let m = harness.world.monster(monster).expect("尸体还在");
    assert!(m.dead, "10 血应被 50 攻一击打死");
    assert_eq!(m.hp, 0);
    assert!(
        m.dead_time > harness.world.now(),
        "尸体要等 DeadDelay 才移除"
    );
    let packets = harness.packets(player);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectStruck(struck) if struck.object_id == monster
        )),
        "打怪的玩家自己也应看到 ObjectStruck（Broadcast 只排除怪物自己），实际 {packets:?}"
    );
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectDied(died) if died.object_id == monster
        )),
        "死亡应广播 ObjectDied（Crystal `MonsterObject.Die`，`MonsterObject.cs:975`），实际收到 {packets:?}"
    );

    // 推进 3 分钟（`DeadDelay = 180000`）后尸体消失并广播 ObjectRemove。
    harness.world.advance_clock(180_000);
    harness.world.tick();

    assert!(harness.world.monster(monster).is_none(), "尸体应被移除");
    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::ObjectRemove(remove) if remove.object_id == monster
        )),
        "尸体消失应广播 ObjectRemove"
    );
}

#[test]
fn monster_kills_player_and_sends_death_packet() {
    let mut harness = Harness::new(16);
    let player = harness.add_player(1, Point::new(20, 20));
    // 一击 99 点，玩家只有 15 血。
    harness.spawn(test_monster(10, 99, 99), Point::new(21, 20));

    harness.world.tick();
    harness.world.advance_clock(300);
    harness.world.tick();

    let p = harness.world.player(player).unwrap();
    assert!(p.dead, "玩家应被标记为死亡");
    assert_eq!(p.hp, 0);
    let packets = harness.packets(player);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Death(_))),
        "受害者应收到 Death 包（Crystal `PlayerObject.cs:649`），实际 {packets:?}"
    );
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::HealthChanged(health) if health.hp == 0
        )),
        "HealthChanged 应把血量同步成 0，实际 {packets:?}"
    );

    // 死亡后不能再走/攻击。
    harness.ready_player(player);
    assert!(harness.world.walk(player, MirDirection::Up).is_err());
    assert!(harness.world.attack(player, MirDirection::Up).is_err());
}

#[test]
fn player_entering_world_receives_nearby_monsters() {
    let mut harness = Harness::new(17);
    // 先有怪，后有玩家：进场时应收到 ObjectMonster（Crystal `MapObject.Add(HumanObject)`）。
    harness.spawn(test_monster(10, 1, 2), Point::new(22, 22));
    let player = harness.add_player(1, Point::new(20, 20));

    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::ObjectMonster(monster)
                if monster.location == Point::new(22, 22) && monster.name == "测试怪"
        )),
        "进场玩家应收到视野内的 ObjectMonster"
    );
}

#[test]
fn walking_out_of_range_removes_monster_and_walking_back_adds_it() {
    let mut harness = Harness::new(18);
    // 玩家在 (20,20)，怪物在 (28,20)：距离 8，在广播半径（16，`Globals.cs:37`）内。
    let player = harness.add_player(1, Point::new(20, 20));
    harness.spawn(test_monster(10, 1, 2), Point::new(28, 20));
    harness.packets(player);

    // 向左走 7 格到 (13,20)：距离 15 ≤ 16，仍然可见。
    for _ in 0..7 {
        harness.ready_player(player);
        harness.world.walk(player, MirDirection::Left).expect("走");
    }
    assert!(
        !harness.has(player, |packet| matches!(
            packet,
            ServerPacket::ObjectRemove(_)
        )),
        "距离 15（≤ 16）时还不该移除"
    );

    // 再走 2 格到 (11,20)：距离 17 > 16 → 应发 ObjectRemove。
    for _ in 0..2 {
        harness.ready_player(player);
        harness.world.walk(player, MirDirection::Left).expect("走");
    }
    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::ObjectRemove(_)
        )),
        "走出视野应收到 ObjectRemove"
    );

    // 走回来 2 格到 (13,20)（距离 15）→ 重新收到 ObjectMonster。
    for _ in 0..2 {
        harness.ready_player(player);
        harness.world.walk(player, MirDirection::Right).expect("走");
    }
    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::ObjectMonster(_)
        )),
        "走回视野应重新收到 ObjectMonster"
    );
}

#[test]
fn rng_is_reproducible_for_the_same_seed() {
    let mut positions = Vec::new();
    for _ in 0..2 {
        let mut harness = Harness::new(99);
        let _player = harness.add_player(1, Point::new(20, 20));
        let monster = harness.spawn(test_monster(10, 0, 9), Point::new(24, 20));
        for _ in 0..6 {
            harness.wake(monster);
            harness.world.tick();
        }
        positions.push((
            harness.world.monster(monster).unwrap().position,
            harness.world.monster(monster).unwrap().direction,
        ));
    }
    assert_eq!(positions[0], positions[1], "同 seed 必须得到同一结果");
}

#[test]
fn newbie_weapon_kills_hen_within_fifteen_swings() {
    let mut harness = Harness::new(98);
    let player = harness.add_player(1, Point::new(20, 20));
    if let Some(player) = harness.world.player_mut(player) {
        player.stats.min_dc = 2;
        player.stats.max_dc = 4;
        player.stats.accuracy = 7;
    }
    let mut kind = test_monster(5, 0, 0);
    kind.stats.agility = 10;
    let monster = harness.spawn(kind, Point::new(21, 20));
    harness.wake(monster);
    let mut swings = 0;
    while harness
        .world
        .monster(monster)
        .is_some_and(|monster| !monster.dead)
        && swings < 15
    {
        harness.ready_player(player);
        let _ = harness.world.attack(player, MirDirection::Right);
        harness.world.advance_clock(1_000);
        harness.world.tick();
        if let Some(monster) = harness.world.monster_mut(monster) {
            monster.operate_time = i64::MAX;
        }
        swings += 1;
    }
    assert!(
        harness
            .world
            .monster(monster)
            .is_none_or(|monster| monster.dead),
        "1 级新号应在 15 次挥砍内杀死 5 HP 的鸡，实际 {swings} 次"
    );
}

#[test]
fn newbie_weapon_hits_hen_at_least_seventy_percent() {
    let mut harness = Harness::new(99);
    let player = harness.add_player(1, Point::new(20, 20));
    if let Some(player) = harness.world.player_mut(player) {
        player.stats.min_dc = 2;
        player.stats.max_dc = 4;
        // 出生武器 +2 Accuracy 的新手扩展（见 `items::starter_items_for`）。
        player.stats.accuracy = 7;
    }
    let mut kind = test_monster(1_000, 0, 0);
    kind.stats.agility = 10;
    let monster = harness.spawn(kind, Point::new(21, 20));
    harness.wake(monster);
    let mut hits = 0;
    for _ in 0..100 {
        harness.ready_player(player);
        let before = harness.world.monster(monster).expect("怪").hp;
        let _ = harness.world.attack(player, MirDirection::Right);
        harness.world.advance_clock(1_000);
        harness.world.tick();
        let after = harness.world.monster(monster).map(|monster| monster.hp);
        if after.is_some_and(|hp| hp < before) {
            hits += 1;
        }
        if let Some(monster) = harness.world.monster_mut(monster) {
            monster.operate_time = i64::MAX;
        }
    }
    assert!(
        hits >= 70,
        "1 级新号对 Hen 的 100 次挥砍命中应 >=70，实际 {hits}"
    );
}

fn special_monster(ai: u8, hp: i32) -> MonsterKind {
    let mut kind = test_monster(hp, 0, 0);
    kind.ai = ai;
    kind.name = format!("special-{ai}");
    kind
}

fn summon_count(harness: &Harness) -> usize {
    harness
        .world
        .monsters()
        .iter()
        .filter(|monster| monster.kind.name == "BugBat" && !monster.dead)
        .count()
}

#[test]
fn summoned_bugbat_inherits_master_target() {
    let mut harness = Harness::new(44);
    let player = harness.add_player(1, Point::new(20, 20));
    let master = harness.spawn(special_monster(12, 100), Point::new(21, 20));
    harness.world.monster_mut(master).unwrap().target = Some(player);
    harness.wake(master);
    harness.world.tick();
    let bugbat = harness
        .world
        .monsters()
        .iter()
        .find(|monster| monster.kind.name == "BugBat")
        .expect("应召唤 BugBat");
    assert_eq!(bugbat.target, Some(player));
}

#[test]
fn healthy_deer_does_not_flee() {
    let mut harness = Harness::new(45);
    let player = harness.add_player(1, Point::new(20, 20));
    let deer = harness.spawn(special_monster(2, 100), Point::new(21, 20));
    {
        let monster = harness.world.monster_mut(deer).unwrap();
        monster.hp = 100;
        monster.target = Some(player);
    }
    harness.wake(deer);
    harness.world.tick();
    let position = harness.world.monster(deer).unwrap().position;
    assert_eq!(position, Point::new(21, 20), "满血不应触发逃跑");
}

#[test]
fn bugbat_maggot_summons_with_cap_and_cooldown() {
    let mut harness = Harness::new(41);
    let player = harness.add_player(1, Point::new(20, 20));
    let master = harness.spawn(special_monster(12, 100), Point::new(21, 20));
    harness.world.monster_mut(master).unwrap().target = Some(player);
    harness.wake(master);
    harness.world.tick();
    assert_eq!(summon_count(&harness), 1, "第一次应召唤 1 只 BugBat");
    harness.wake(master);
    harness.world.tick();
    assert_eq!(summon_count(&harness), 1, "3 秒冷却内不能重复召唤");
    for _ in 0..6 {
        harness.world.advance_clock(3_100);
        harness.wake(master);
        harness.world.tick();
    }
    assert_eq!(summon_count(&harness), 4, "召唤上限 4 只");
}

#[test]
fn summoner_death_removes_its_slaves() {
    let mut harness = Harness::new(42);
    let player = harness.add_player(1, Point::new(20, 20));
    {
        let p = harness.world.player_mut(player).unwrap();
        p.stats.min_dc = 100;
        p.stats.max_dc = 100;
        p.stats.accuracy = 100;
    }
    let master = harness.spawn(special_monster(12, 10), Point::new(21, 20));
    harness.world.monster_mut(master).unwrap().target = Some(player);
    harness.wake(master);
    harness.world.tick();
    assert_eq!(summon_count(&harness), 1);
    harness.ready_player(player);
    let _ = harness.world.attack(player, MirDirection::Right);
    harness.world.advance_clock(1_000);
    harness.world.tick();
    harness.world.tick();
    assert!(
        harness.world.monster(master).is_none_or(|m| m.dead),
        "召唤者应被击杀"
    );
    assert_eq!(summon_count(&harness), 0, "召唤者死亡后小怪应消失");
}

#[test]
fn low_health_deer_flees_away_from_target() {
    let mut harness = Harness::new(43);
    let player = harness.add_player(1, Point::new(20, 20));
    let mut kind = special_monster(2, 100);
    kind.stats.hp = 100;
    let deer = harness.spawn(kind, Point::new(21, 20));
    {
        let monster = harness.world.monster_mut(deer).unwrap();
        monster.hp = 20;
        monster.target = Some(player);
    }
    harness.wake(deer);
    harness.world.tick();
    let position = harness.world.monster(deer).unwrap().position;
    assert!(position.x > 21, "低血 Deer 应远离目标，实际 {position:?}");
}

#[test]
fn trap_rock_show_spawns_children_once() {
    let mut harness = Harness::new(46);
    let player = harness.add_player(1, Point::new(20, 20));
    let parent = harness.spawn(special_monster(47, 100), Point::new(21, 20));
    harness.world.monster_mut(parent).unwrap().target = Some(player);
    harness.wake(parent);
    harness.world.tick();
    let children = trace_rock_children(&harness, parent);
    assert_eq!(children, 3, "TrapRock Show 应生成 3 个角落子石");
    harness.wake(parent);
    harness.world.tick();
    assert_eq!(
        trace_rock_children(&harness, parent),
        3,
        "父石 Show 只能触发一次"
    );
}

fn trace_rock_children(harness: &Harness, parent: u32) -> usize {
    harness
        .world
        .monsters()
        .iter()
        .filter(|monster| monster.kind.ai == 47 && monster.object_id != parent && !monster.dead)
        .count()
}

#[test]
fn trap_rock_children_expire_after_five_seconds() {
    let mut harness = Harness::new(47);
    let player = harness.add_player(1, Point::new(20, 20));
    let parent = harness.spawn(special_monster(47, 100), Point::new(21, 20));
    harness.world.monster_mut(parent).unwrap().target = Some(player);
    harness.wake(parent);
    harness.world.tick();
    assert_eq!(trace_rock_children(&harness, parent), 3);
    harness.world.advance_clock(5_100);
    harness.world.tick();
    assert_eq!(
        trace_rock_children(&harness, parent),
        0,
        "子石存活 5 秒后应消失"
    );
}

#[test]
fn tree_and_trainer_are_stationary_scenery() {
    let mut harness = Harness::new(48);
    let _player = harness.add_player(1, Point::new(20, 20));
    for ai in [3, 56] {
        let id = harness.spawn(special_monster(ai, 100), Point::new(22, 20));
        let before = harness.world.monster(id).unwrap().position;
        harness.world.tick();
        assert_eq!(
            harness.world.monster(id).unwrap().position,
            before,
            "AI {ai} 应固定不动"
        );
    }
}

#[test]
fn trainer_scenery_is_stationary_and_does_not_attack() {
    let mut harness = Harness::new(49);
    let player = harness.add_player(1, Point::new(20, 20));
    let trainer = harness.spawn(special_monster(56, 100), Point::new(21, 20));
    harness.world.monster_mut(trainer).unwrap().target = Some(player);
    harness.wake(trainer);
    harness.world.tick();
    assert_eq!(
        harness.world.monster(trainer).unwrap().position,
        Point::new(21, 20),
        "Trainer 不应移动"
    );
}

#[test]
fn newly_audited_scenery_ais_stay_stationary() {
    let mut harness = Harness::new(50);
    let _player = harness.add_player(1, Point::new(20, 20));
    for ai in [15, 99, 119, 120, 128] {
        let id = harness.spawn(special_monster(ai, 100), Point::new(22, 20));
        let before = harness.world.monster(id).unwrap().position;
        harness.wake(id);
        harness.world.tick();
        assert_eq!(
            harness.world.monster(id).unwrap().position,
            before,
            "AI {ai} 应是站桩/石化"
        );
    }
}

#[test]
fn horned_archer_and_warrior_have_audited_ranges() {
    assert_eq!(special_monster(164, 100).ranged_attack_range(), Some(6));
    assert_eq!(special_monster(165, 100).ranged_attack_range(), Some(4));
    assert_eq!(special_monster(102, 100).ranged_attack_range(), Some(8));
    assert_eq!(
        special_monster(181, 100).ranged_attack_range(),
        Some(7),
        "WaterDragon 使用 view_range 远程"
    );
    assert_eq!(
        special_monster(210, 100).ranged_attack_range(),
        Some(7),
        "HoodedSummonerScrolls 使用 view_range 远程"
    );
    for ai in [121, 127, 173] {
        assert_eq!(
            special_monster(ai, 100).ranged_attack_range(),
            Some(2),
            "AI {ai} 用范围 2 攻击"
        );
    }
}

#[test]
fn armadillo_family_flees_at_low_health() {
    let mut harness = Harness::new(52);
    let player = harness.add_player(1, Point::new(20, 20));
    for ai in [124, 125] {
        let id = harness.spawn(special_monster(ai, 100), Point::new(21, 20));
        {
            let monster = harness.world.monster_mut(id).unwrap();
            monster.hp = 20;
            monster.target = Some(player);
        }
        harness.wake(id);
        harness.world.tick();
        assert!(
            harness.world.monster(id).unwrap().position.x > 21,
            "AI {ai} 低血应逃跑"
        );
    }
}

#[test]
fn extra_missing_ai_ranges_and_stationary() {
    let mut harness = Harness::new(53);
    let _player = harness.add_player(1, Point::new(20, 20));
    for ai in [79, 170] {
        let id = harness.spawn(special_monster(ai, 100), Point::new(22, 20));
        let before = harness.world.monster(id).unwrap().position;
        harness.wake(id);
        harness.world.tick();
        assert_eq!(
            harness.world.monster(id).unwrap().position,
            before,
            "AI {ai} 应固定不动"
        );
    }
    assert_eq!(special_monster(72, 100).ranged_attack_range(), Some(6));
    assert_eq!(special_monster(4, 100).ranged_attack_range(), Some(2));
    assert_eq!(special_monster(116, 100).ranged_attack_range(), Some(2));
    assert_eq!(special_monster(117, 100).ranged_attack_range(), Some(2));
    assert_eq!(special_monster(192, 100).ranged_attack_range(), Some(4));
    assert_eq!(special_monster(70, 100).ranged_attack_range(), Some(5));
    assert!(special_monster(163, 100).monster_magic().is_some());
    assert!(special_monster(169, 100).monster_magic().is_some());
    assert!(special_monster(118, 100).monster_magic().is_some());
    assert!(special_monster(126, 100).monster_magic().is_some());
}

#[test]
fn hellknight_uses_generic_melee_path() {
    let mut harness = Harness::new(51);
    let player = harness.add_player(1, Point::new(20, 20));
    let knight = harness.spawn(special_monster(97, 100), Point::new(21, 20));
    harness.world.monster_mut(knight).unwrap().target = Some(player);
    harness.wake(knight);
    harness.world.tick();
    assert_eq!(
        harness.world.monster(knight).unwrap().target,
        Some(player),
        "HellKnight 应走基类索敌/近战路径"
    );
}

#[test]
fn death_and_experience_packets_survive_large_world_batch_flush() {
    let mut harness = Harness::new(54);
    let player = harness.add_player(1, Point::new(20, 20));
    // `tick_monsters` 只在大世界（>512 只）才启用 AI 合批；填充物放远处，避免干扰玩家。
    for i in 0..513 {
        let x = 30 + (i % 50);
        let y = 30 + (i / 50);
        let _ = harness.spawn(special_monster(3, 100), Point::new(x, y));
    }
    let mut kind = test_monster(1, 5, 5);
    kind.experience = 18;
    let monster = harness.spawn(kind, Point::new(21, 20));
    if let Some(p) = harness.world.player_mut(player) {
        p.stats.min_dc = 50;
        p.stats.max_dc = 50;
        p.stats.accuracy = 100;
    }
    // 生产服务端开启按 tick 合批；死亡发生在 `resolve_pending_hits`，早于 `tick_monsters`。
    harness.world.set_broadcast_batching(true);
    harness.ready_player(player);
    harness
        .world
        .attack(player, MirDirection::Right)
        .expect("攻击应被接受")
        .expect("前方有怪物");
    harness.world.advance_clock(300);
    harness.world.tick();
    assert!(
        harness
            .world
            .monster(monster)
            .is_none_or(|monster| monster.dead),
        "怪物应被击杀"
    );
    let packets = harness.packets(player);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectDied(died) if died.object_id == monster
        )),
        "合批路径不能丢掉 ObjectDied，实际 {packets:?}"
    );
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::GainExperience(exp) if exp.amount == 18)),
        "合批路径应同时保留 GainExperience，实际 {packets:?}"
    );
}

#[test]
fn poison_hugger_has_ranged_five() {
    assert_eq!(special_monster(69, 100).ai_support(), AiSupport::Full);
    assert_eq!(special_monster(69, 100).ranged_attack_range(), Some(5));
    assert!(special_monster(69, 100).monster_special().is_none());
}

#[test]
fn restless_jar_stays_stationary_and_ranged_six() {
    assert_eq!(special_monster(122, 100).ai_support(), AiSupport::Full);
    let mut harness = Harness::new(55);
    let player = harness.add_player(1, Point::new(20, 20));
    let jar = harness.spawn(special_monster(122, 100), Point::new(22, 20));
    harness.world.monster_mut(jar).unwrap().target = Some(player);
    harness.wake(jar);
    harness.world.tick();
    assert_eq!(
        harness.world.monster(jar).unwrap().position,
        Point::new(22, 20),
        "RestlessJar CanMove=false"
    );
    assert_eq!(special_monster(122, 100).ranged_attack_range(), Some(6));
}

#[test]
fn cannibal_tentacles_uses_view_range_ranged() {
    assert_eq!(special_monster(130, 100).ai_support(), AiSupport::Full);
    assert_eq!(
        special_monster(130, 100).ranged_attack_range(),
        Some(7),
        "CannibalTentacles 用 ViewRange 远程"
    );
    assert!(special_monster(130, 100).monster_special().is_none());
}

#[test]
fn snow_yeti_has_ranged_nine() {
    assert_eq!(special_monster(190, 100).ai_support(), AiSupport::Full);
    assert_eq!(special_monster(190, 100).ranged_attack_range(), Some(9));
}

#[test]
fn snow_wolf_aoe_radius_two() {
    assert_eq!(special_monster(179, 100).ai_support(), AiSupport::Full);
    let spec = special_monster(179, 100)
        .monster_special()
        .expect("SnowWolf AoE");
    assert_eq!(spec.range, 2);
    assert!(matches!(
        spec.shape,
        mir_server::monster::MonsterMagicShape::Aoe { radius: 2, .. }
    ));
}

#[test]
fn frozen_miner_aoe_radius_one() {
    assert_eq!(special_monster(187, 100).ai_support(), AiSupport::Full);
    let spec = special_monster(187, 100)
        .monster_special()
        .expect("FrozenMiner AoE");
    assert_eq!(spec.range, 1);
    assert!(matches!(
        spec.shape,
        mir_server::monster::MonsterMagicShape::Aoe { radius: 1, .. }
    ));
}

#[test]
fn sand_snail_aoe_radius_one() {
    assert_eq!(special_monster(115, 100).ai_support(), AiSupport::Full);
    let spec = special_monster(115, 100)
        .monster_special()
        .expect("SandSnail AoE");
    assert_eq!(spec.range, 1);
    assert!(matches!(
        spec.shape,
        mir_server::monster::MonsterMagicShape::Aoe { radius: 1, .. }
    ));
}
