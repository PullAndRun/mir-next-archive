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
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
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
    // 玩家在 (20,20)，怪物在 (28,20)：距离 8，在广播半径（10）内。
    let player = harness.add_player(1, Point::new(20, 20));
    harness.spawn(test_monster(10, 1, 2), Point::new(28, 20));
    harness.packets(player);

    // 向左走两格到 (18,20)：距离 10，仍然可见。
    for _ in 0..2 {
        harness.ready_player(player);
        harness.world.walk(player, MirDirection::Left).expect("走");
    }
    assert!(
        !harness.has(player, |packet| matches!(
            packet,
            ServerPacket::ObjectRemove(_)
        )),
        "距离 10 时还不该移除"
    );

    // 再走两格到 (16,20)：距离 12 > 10 → 应发 ObjectRemove。
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

    // 走回来 → 重新收到 ObjectMonster。
    for _ in 0..4 {
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
