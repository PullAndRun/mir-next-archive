//! R46：其他玩家（非自机 Player）`S.ObjectWalk`/`S.ObjectRun` 移动插值。
//!
//! 覆盖门下令 M1–M4 的确定性部分：
//! - M1：固定坐标量/符号，向北/向南首帧与 600ms 终点独立数值；
//! - M2：动作/motion 共用该包接收时刻；200/600ms 延迟不把传输耗时算进动作进度；
//! - M3：8 方向 × Walk/Run × 6 帧 = 96 组合的独立二维像素期望（常量表，不由生产插值生成）；
//!   以及 0/99/100/300/500/599/600/700ms 边界；
//! - 状态：同包不重启、新步替换、打断/非法/未知/ID 复用/双远端隔离；
//! - 回归：自机静止/移动 × 远端移动的 anchor/global offset；
//! - 排序冒烟：上下行穿过站立实体，实体只出现一次，排序仍走既有 `position` 规则。
//!
//! `live_world` 双客户端证据见 `crates/mir-client/tests/remote_motion_live.rs`。

use mir_client::camera::{self, SelfMotion, StepMotion};
use mir_client::net::{PlayerLooks, SelfPlayer, WorldEntity, WorldSnapshot};
use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{
    MapChanged, ObjectAttack, ObjectDied, ObjectRemove, ObjectRun, ObjectStruck, ObjectTurn,
    ObjectWalk,
};
use mir_render::{ActorAction, EntityKind, EntityView, Viewport, geometry};

/// 测试基准时刻（Unix 毫秒，必须非 0）。
const BASE: u64 = 2_000_000;

const DIRECTIONS: [MirDirection; 8] = [
    MirDirection::Up,
    MirDirection::UpRight,
    MirDirection::Right,
    MirDirection::DownRight,
    MirDirection::Down,
    MirDirection::DownLeft,
    MirDirection::Left,
    MirDirection::UpLeft,
];

/// M3 独立二维期望：`REMOTE_ADVANCE[方向][Walk/Run][帧]` = 远端相对 `from` 的推进像素。
///
/// 数值由 Crystal 公式 `(int)((Cell*i/6.0)*(index+1))` + `%2` 手工固化，
/// **测试不从生产插值函数生成期望**。方向顺序见 [`DIRECTIONS`]。
const REMOTE_ADVANCE: [[[(i32, i32); 6]; 2]; 8] = [
    // Up
    [
        [(0, -6), (0, -10), (0, -16), (0, -22), (0, -26), (0, -32)],
        [(0, -10), (0, -22), (0, -32), (0, -42), (0, -54), (0, -64)],
    ],
    // UpRight
    [
        [
            (8, -6),
            (16, -10),
            (24, -16),
            (32, -22),
            (40, -26),
            (48, -32),
        ],
        [
            (16, -10),
            (32, -22),
            (48, -32),
            (64, -42),
            (80, -54),
            (96, -64),
        ],
    ],
    // Right
    [
        [(8, 0), (16, 0), (24, 0), (32, 0), (40, 0), (48, 0)],
        [(16, 0), (32, 0), (48, 0), (64, 0), (80, 0), (96, 0)],
    ],
    // DownRight
    [
        [(8, 6), (16, 10), (24, 16), (32, 22), (40, 26), (48, 32)],
        [(16, 10), (32, 22), (48, 32), (64, 42), (80, 54), (96, 64)],
    ],
    // Down
    [
        [(0, 6), (0, 10), (0, 16), (0, 22), (0, 26), (0, 32)],
        [(0, 10), (0, 22), (0, 32), (0, 42), (0, 54), (0, 64)],
    ],
    // DownLeft
    [
        [
            (-8, 6),
            (-16, 10),
            (-24, 16),
            (-32, 22),
            (-40, 26),
            (-48, 32),
        ],
        [
            (-16, 10),
            (-32, 22),
            (-48, 32),
            (-64, 42),
            (-80, 54),
            (-96, 64),
        ],
    ],
    // Left
    [
        [(-8, 0), (-16, 0), (-24, 0), (-32, 0), (-40, 0), (-48, 0)],
        [(-16, 0), (-32, 0), (-48, 0), (-64, 0), (-80, 0), (-96, 0)],
    ],
    // UpLeft
    [
        [
            (-8, -6),
            (-16, -10),
            (-24, -16),
            (-32, -22),
            (-40, -26),
            (-48, -32),
        ],
        [
            (-16, -10),
            (-32, -22),
            (-48, -32),
            (-64, -42),
            (-80, -54),
            (-96, -64),
        ],
    ],
];

fn snapshot_with_self(position: Point) -> WorldSnapshot {
    WorldSnapshot {
        self_player: Some(SelfPlayer {
            object_id: 1,
            real_id: 1,
            name: "SelfHero".to_owned(),
            position,
            direction: MirDirection::Right,
            hp: 100,
            hp_max: 100,
            mp: 0,
            mp_max: 0,
            level: 1,
            class: MirClass::Warrior,
            gender: MirGender::Male,
            hair: 0,
            gold: 0,
            credit: 0,
            dead: false,
            action: ActorAction::Standing,
            action_started_ms: 0,
            attack_variant: None,
            native_action: None,
            looks: PlayerLooks::default(),
        }),
        ..WorldSnapshot::default()
    }
}

fn insert_remote(
    snapshot: &mut WorldSnapshot,
    object_id: u32,
    position: Point,
    direction: MirDirection,
) {
    let mut entity = WorldEntity::new(
        object_id,
        EntityKind::Player,
        format!("Remote{object_id}"),
        position,
        direction,
        0,
    );
    entity.action = ActorAction::Standing;
    entity.action_started_ms = 0;
    snapshot.entities.insert(object_id, entity);
}

fn apply_walk(snapshot: &mut WorldSnapshot, id: u32, to: Point, direction: MirDirection, now: u64) {
    snapshot.apply_packet_at(
        &ServerPacket::ObjectWalk(ObjectWalk {
            object_id: id,
            location: to,
            direction,
        }),
        now,
    );
}

fn apply_run(snapshot: &mut WorldSnapshot, id: u32, to: Point, direction: MirDirection, now: u64) {
    snapshot.apply_packet_at(
        &ServerPacket::ObjectRun(ObjectRun {
            object_id: id,
            location: to,
            direction,
        }),
        now,
    );
}

fn remote_view(prepared: &camera::PreparedFrame, id: u32) -> &EntityView {
    prepared
        .entities
        .iter()
        .find(|view| view.name.as_deref() == Some(format!("Remote{id}").as_str()))
        .expect("远端实体应出现在实体列表")
}

fn self_view(prepared: &camera::PreparedFrame) -> &EntityView {
    prepared
        .entities
        .iter()
        .find(|view| view.name.as_deref() == Some("SelfHero"))
        .expect("自机应出现")
}

fn screen_of(prepared: &camera::PreparedFrame, view: &EntityView) -> (i32, i32) {
    let (x, y) = geometry::world_to_screen(Viewport::DEFAULT, prepared.center, view.position);
    (
        x + prepared.offset.0 + view.pixel_offset.0,
        y + prepared.offset.1 + view.pixel_offset.1,
    )
}

fn prepare(
    snapshot: &WorldSnapshot,
    self_motion: Option<SelfMotion>,
    now: u64,
) -> camera::PreparedFrame {
    let mut snapshot = snapshot.clone();
    let mut motion = self_motion;
    camera::prepare_frame(&mut snapshot, &mut motion, Point::new(0, 0), now)
}

// ---------------------------------------------------------------------------
// M1：固定坐标量/符号
// ---------------------------------------------------------------------------

#[test]
fn m1_north_south_first_frame_and_600ms_endpoint_use_from_and_advance() {
    let self_pos = Point::new(100, 100);
    let center = Viewport::DEFAULT.center_origin();
    for (direction, expected_first, expected_end, crystal_first) in [
        (
            MirDirection::Up,
            (0, -6),
            (0, -32),
            (0, 6), // Crystal OffSetMove 指向起点：Up 为 +y
        ),
        (MirDirection::Down, (0, 6), (0, 32), (0, -6)),
    ] {
        let mut snapshot = snapshot_with_self(self_pos);
        let to = self_pos.moved(direction, 1);
        insert_remote(&mut snapshot, 2, self_pos, direction);
        apply_walk(&mut snapshot, 2, to, direction, BASE);

        // 首帧：从 from 推进 1/6 格（不是相对终点的剩余 5/6）。
        let prepared = prepare(&snapshot, None, BASE);
        let remote = remote_view(&prepared, 2);
        assert_eq!(remote.position, self_pos, "绘制起点必须是 remote.from");
        assert_eq!(remote.pixel_offset, expected_first, "首帧推进量");
        assert_eq!(
            screen_of(&prepared, remote),
            (center.0 + expected_first.0, center.1 + expected_first.1),
            "screen = world_to_screen(self_center, from) + self_offset + remote 推进量"
        );
        assert_eq!(
            camera::world_offset(self_pos, to, 0),
            crystal_first,
            "Crystal OffSetMove 与 pixel_offset 反号"
        );
        assert_eq!(
            screen_of(&prepared, self_view(&prepared)),
            center,
            "自机不动时仍居中"
        );

        // 600ms 终点：状态清空、绘制落权威 to。
        let prepared_end = prepare(&snapshot, None, BASE + 600);
        let remote_end = remote_view(&prepared_end, 2);
        assert_eq!(remote_end.position, to, "600ms 后落终点");
        assert_eq!(remote_end.pixel_offset, (0, 0));
        assert_eq!(
            screen_of(&prepared_end, remote_end),
            (center.0 + expected_end.0, center.1 + expected_end.1),
            "终点 screen 用 world_to_screen(self_center, to)"
        );
        let mut snapshot_end = snapshot.clone();
        let mut none = None;
        camera::prepare_frame(&mut snapshot_end, &mut none, Point::new(0, 0), BASE + 600);
        assert!(
            snapshot_end.remote_motions.is_empty(),
            "600ms 后 remote motion 应清空"
        );
    }
}

// ---------------------------------------------------------------------------
// M2：同钟与延迟包
// ---------------------------------------------------------------------------

#[test]
fn m2_delayed_packets_do_not_count_transmission_time_as_action_progress() {
    let self_pos = Point::new(100, 100);
    let from = self_pos;
    let to = self_pos.moved(MirDirection::Right, 1);
    for delay in [200_u64, 600] {
        let send_ms = BASE;
        let receive_ms = send_ms + delay;
        let mut snapshot = snapshot_with_self(self_pos);
        insert_remote(&mut snapshot, 2, from, MirDirection::Right);
        apply_walk(&mut snapshot, 2, to, MirDirection::Right, receive_ms);

        let motion = snapshot.remote_motions[&2];
        assert_eq!(
            motion.started_ms, receive_ms,
            "motion 必须从接收/应用时刻起算"
        );
        assert_eq!(
            snapshot.entities[&2].action_started_ms, receive_ms,
            "动作与 motion 共用接收时刻"
        );
        eprintln!(
            "M2 delay={delay}ms send={send_ms} receive={receive_ms} motion_started={} action_started={}",
            motion.started_ms, snapshot.entities[&2].action_started_ms
        );

        // 采样点 = 接收时刻 + 0/100/599/600ms（不是发送时刻）。
        for (elapsed, expected_frame, expected_px) in [
            (0_u64, 0_usize, (8, 0)),
            (100, 1, (16, 0)),
            (300, 3, (32, 0)),
            (500, 5, (48, 0)),
            (599, 5, (48, 0)),
        ] {
            let sample_ms = receive_ms + elapsed;
            let prepared = prepare(&snapshot, None, sample_ms);
            let remote = remote_view(&prepared, 2);
            assert_eq!(
                remote.pixel_offset,
                expected_px,
                "delay={delay} sample={elapsed} 应取第 {} 帧",
                expected_frame + 1
            );
            assert_eq!(remote.position, from);
        }
        let prepared_end = prepare(&snapshot, None, receive_ms + 600);
        let remote_end = remote_view(&prepared_end, 2);
        assert_eq!(remote_end.position, to);
        assert_eq!(remote_end.pixel_offset, (0, 0));
        let mut snapshot_end = snapshot.clone();
        let mut none = None;
        camera::prepare_frame(
            &mut snapshot_end,
            &mut none,
            Point::new(0, 0),
            receive_ms + 600,
        );
        assert!(snapshot_end.remote_motions.is_empty());
    }
}

// ---------------------------------------------------------------------------
// M3：96 组合独立像素期望 + 边界
// ---------------------------------------------------------------------------

#[test]
fn m3_all_96_direction_action_frame_combinations_match_independent_table() {
    let self_pos = Point::new(100, 100);
    let center = Viewport::DEFAULT.center_origin();
    for (dir_index, direction) in DIRECTIONS.iter().enumerate() {
        for (action_index, action_frames) in REMOTE_ADVANCE[dir_index].iter().enumerate() {
            let running = action_index == 1;
            let steps = if running { 2 } else { 1 };
            let action = if running {
                ActorAction::Running
            } else {
                ActorAction::Walking
            };
            let to = self_pos.moved(*direction, steps);
            for (frame, expected) in action_frames.iter().enumerate() {
                let mut snapshot = snapshot_with_self(self_pos);
                insert_remote(&mut snapshot, 2, self_pos, *direction);
                if running {
                    apply_run(&mut snapshot, 2, to, *direction, BASE);
                } else {
                    apply_walk(&mut snapshot, 2, to, *direction, BASE);
                }
                let prepared = prepare(&snapshot, None, BASE + (frame as u64) * 100);
                let remote = remote_view(&prepared, 2);
                assert_eq!(
                    remote.position, self_pos,
                    "{direction:?} {action:?} 帧{frame} 绘制在 from"
                );
                assert_eq!(
                    remote.pixel_offset, *expected,
                    "{direction:?} {action:?} 帧{frame} 推进量"
                );
                assert_eq!(
                    remote.action, action,
                    "{direction:?} {action:?} 帧{frame} 视觉动作"
                );
                assert_eq!(
                    screen_of(&prepared, remote),
                    (center.0 + expected.0, center.1 + expected.1),
                    "{direction:?} {action:?} 帧{frame} 屏幕位置"
                );
                // Crystal OffSetMove = -推进量（固定数值由表反号，不调用生产插值求期望）。
                assert_eq!(
                    camera::world_offset(self_pos, to, (frame as u64) * 100),
                    (-expected.0, -expected.1),
                    "{direction:?} {action:?} 帧{frame} Crystal OffSetMove"
                );
                assert_eq!(screen_of(&prepared, self_view(&prepared)), center);
            }
        }
    }
}

#[test]
fn m3_boundary_0_99_100_300_500_599_600_700ms() {
    let self_pos = Point::new(100, 100);
    let center = Viewport::DEFAULT.center_origin();
    let from = self_pos;
    let to = self_pos.moved(MirDirection::Down, 1);
    let mut snapshot = snapshot_with_self(self_pos);
    insert_remote(&mut snapshot, 2, from, MirDirection::Down);
    apply_walk(&mut snapshot, 2, to, MirDirection::Down, BASE);

    for (elapsed, expected_px, should_have_motion) in [
        (0_u64, (0, 6), true),
        (99, (0, 6), true),
        (100, (0, 10), true),
        (300, (0, 22), true),
        (500, (0, 32), true),
        (599, (0, 32), true),
        (600, (0, 0), false),
        (700, (0, 0), false),
    ] {
        let prepared = prepare(&snapshot, None, BASE + elapsed);
        let remote = remote_view(&prepared, 2);
        assert_eq!(remote.pixel_offset, expected_px, "{elapsed}ms 的远端偏移");
        if elapsed >= 600 {
            assert_eq!(remote.position, to, "{elapsed}ms 应落终点");
            assert_eq!(remote.action, ActorAction::Standing);
            assert_eq!(
                screen_of(&prepared, remote),
                (center.0, center.1 + 32),
                "{elapsed}ms 终点屏幕"
            );
        } else {
            assert_eq!(remote.position, from);
            assert_eq!(
                screen_of(&prepared, remote),
                (center.0 + expected_px.0, center.1 + expected_px.1)
            );
        }
        if !should_have_motion {
            let mut snapshot_copy = snapshot.clone();
            let mut none = None;
            camera::prepare_frame(
                &mut snapshot_copy,
                &mut none,
                Point::new(0, 0),
                BASE + elapsed,
            );
            assert!(
                snapshot_copy.remote_motions.is_empty(),
                "{elapsed}ms 应无残留 motion"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 状态：同包不重启、新步替换、打断/非法/未知/ID 复用/双远端隔离
// ---------------------------------------------------------------------------

#[test]
fn same_coordinate_packet_does_not_restart_and_new_step_replaces() {
    let self_pos = Point::new(100, 100);
    let p1 = self_pos.moved(MirDirection::Right, 1);
    let p2 = p1.moved(MirDirection::Right, 1);
    let mut snapshot = snapshot_with_self(self_pos);
    insert_remote(&mut snapshot, 2, self_pos, MirDirection::Right);
    apply_walk(&mut snapshot, 2, p1, MirDirection::Right, BASE);
    let first = snapshot.remote_motions[&2];
    // 同坐标重复包（回拉/重复广播）：不重启动作、不延长 motion。
    apply_walk(&mut snapshot, 2, p1, MirDirection::Right, BASE + 200);
    assert_eq!(snapshot.remote_motions[&2], first);
    assert_eq!(snapshot.entities[&2].action_started_ms, BASE);
    let prepared = prepare(&snapshot, None, BASE + 599);
    assert_eq!(remote_view(&prepared, 2).pixel_offset, (48, 0));
    let prepared_end = prepare(&snapshot, None, BASE + 600);
    assert_eq!(remote_view(&prepared_end, 2).position, p1);

    // 新的合法步覆盖旧状态，从新接收时刻重新起算。
    apply_walk(&mut snapshot, 2, p2, MirDirection::Right, BASE + 600);
    let second = snapshot.remote_motions[&2];
    assert_eq!(second.from, p1);
    assert_eq!(second.to, p2);
    assert_eq!(second.started_ms, BASE + 600);
    assert_eq!(snapshot.entities[&2].position, p2);
    let prepared = prepare(&snapshot, None, BASE + 600);
    let remote = remote_view(&prepared, 2);
    assert_eq!(remote.position, p1, "新一步从 p1 开始，不继续旧 offset");
    assert_eq!(remote.pixel_offset, (8, 0));
}

#[test]
fn interruptions_invalid_unknown_and_id_reuse_clear_state() {
    let self_pos = Point::new(100, 100);
    let to = self_pos.moved(MirDirection::Right, 1);
    let start = |id: u32| {
        let mut snapshot = snapshot_with_self(self_pos);
        insert_remote(&mut snapshot, id, self_pos, MirDirection::Right);
        apply_walk(&mut snapshot, id, to, MirDirection::Right, BASE);
        assert!(snapshot.remote_motions.contains_key(&id));
        snapshot
    };
    // 转身。
    let mut snapshot = start(2);
    snapshot.apply_packet_at(
        &ServerPacket::ObjectTurn(ObjectTurn {
            object_id: 2,
            location: to,
            direction: MirDirection::Down,
        }),
        BASE + 100,
    );
    assert!(snapshot.remote_motions.is_empty());
    // 攻击。
    let mut snapshot = start(2);
    snapshot.apply_packet_at(
        &ServerPacket::ObjectAttack(ObjectAttack {
            object_id: 2,
            location: to,
            direction: MirDirection::Down,
            spell: mir_core::enums::Spell::None,
            level: 0,
            type_: 0,
        }),
        BASE + 100,
    );
    assert!(snapshot.remote_motions.is_empty());
    // 受击。
    let mut snapshot = start(2);
    snapshot.apply_packet_at(
        &ServerPacket::ObjectStruck(ObjectStruck {
            object_id: 2,
            attacker_id: 1,
            location: to,
            direction: MirDirection::Down,
        }),
        BASE + 100,
    );
    assert!(snapshot.remote_motions.is_empty());
    // 死亡。
    let mut snapshot = start(2);
    snapshot.apply_packet_at(
        &ServerPacket::ObjectDied(ObjectDied {
            object_id: 2,
            location: to,
            direction: MirDirection::Down,
            type_: 0,
        }),
        BASE + 100,
    );
    assert!(snapshot.remote_motions.is_empty());
    // 删除。
    let mut snapshot = start(2);
    snapshot.apply_packet_at(
        &ServerPacket::ObjectRemove(ObjectRemove { object_id: 2 }),
        BASE + 100,
    );
    assert!(snapshot.remote_motions.is_empty());
    assert!(!snapshot.entities.contains_key(&2));
    // 非法位移：Walk 报 2 格 → 只落点、无 motion、动作 snap 到 Standing。
    let mut snapshot = snapshot_with_self(self_pos);
    insert_remote(&mut snapshot, 2, self_pos, MirDirection::Right);
    apply_walk(
        &mut snapshot,
        2,
        self_pos.moved(MirDirection::Right, 2),
        MirDirection::Right,
        BASE,
    );
    assert!(snapshot.remote_motions.is_empty());
    assert_eq!(
        snapshot.entities[&2].position,
        self_pos.moved(MirDirection::Right, 2)
    );
    assert_eq!(
        snapshot.entities[&2].action,
        ActorAction::Walking,
        "不匹配只关插值，动作仍按包播放"
    );
    // R46-X1-04：未知对象不创建实体、不建 motion（等 ObjectPlayer/发现包）。
    let mut snapshot = snapshot_with_self(self_pos);
    apply_walk(
        &mut snapshot,
        99,
        self_pos.moved(MirDirection::Right, 1),
        MirDirection::Right,
        BASE,
    );
    apply_run(
        &mut snapshot,
        99,
        self_pos.moved(MirDirection::Right, 2),
        MirDirection::Right,
        BASE + 100,
    );
    assert!(snapshot.remote_motions.is_empty());
    assert!(
        !snapshot.entities.contains_key(&99),
        "未知对象不得被 ObjectWalk/ObjectRun 凭空创建"
    );
    // 同 ID 重建：先删除，再以同一 ID 新位置出现；旧 motion 不得残留。
    let mut snapshot = start(2);
    snapshot.apply_packet_at(
        &ServerPacket::ObjectRemove(ObjectRemove { object_id: 2 }),
        BASE + 100,
    );
    insert_remote(&mut snapshot, 2, Point::new(200, 200), MirDirection::Down);
    assert!(snapshot.remote_motions.is_empty());
    apply_walk(
        &mut snapshot,
        2,
        Point::new(200, 201),
        MirDirection::Down,
        BASE + 200,
    );
    assert_eq!(snapshot.remote_motions[&2].from, Point::new(200, 200));
    assert_eq!(snapshot.remote_motions[&2].started_ms, BASE + 200);
}

#[test]
fn two_remote_players_are_isolated() {
    let self_pos = Point::new(100, 100);
    let a_from = Point::new(110, 100);
    let b_from = Point::new(120, 100);
    let mut snapshot = snapshot_with_self(self_pos);
    insert_remote(&mut snapshot, 2, a_from, MirDirection::Right);
    insert_remote(&mut snapshot, 3, b_from, MirDirection::Right);
    apply_walk(
        &mut snapshot,
        2,
        a_from.moved(MirDirection::Right, 1),
        MirDirection::Right,
        BASE,
    );
    apply_walk(
        &mut snapshot,
        3,
        b_from.moved(MirDirection::Down, 1),
        MirDirection::Down,
        BASE + 100,
    );
    assert_eq!(snapshot.remote_motions[&2].started_ms, BASE);
    assert_eq!(snapshot.remote_motions[&3].started_ms, BASE + 100);
    let prepared = prepare(&snapshot, None, BASE + 300);
    assert_eq!(remote_view(&prepared, 2).pixel_offset, (32, 0));
    assert_eq!(remote_view(&prepared, 3).pixel_offset, (0, 16));
    // 删除 A 不影响 B。
    snapshot.apply_packet_at(
        &ServerPacket::ObjectRemove(ObjectRemove { object_id: 2 }),
        BASE + 200,
    );
    assert!(!snapshot.remote_motions.contains_key(&2));
    assert!(snapshot.remote_motions.contains_key(&3));
    // B 到期只清 B。
    let mut snapshot_end = snapshot.clone();
    let mut none = None;
    camera::prepare_frame(&mut snapshot_end, &mut none, Point::new(0, 0), BASE + 700);
    assert!(!snapshot_end.remote_motions.contains_key(&3));
}

// ---------------------------------------------------------------------------
// 回归：自机静止/移动 × 远端移动
// ---------------------------------------------------------------------------

#[test]
fn self_static_and_moving_regressions_keep_anchors_and_global_offset() {
    let self_pos = Point::new(100, 100);
    let center = Viewport::DEFAULT.center_origin();
    let remote_from = self_pos;
    let remote_to = self_pos.moved(MirDirection::Down, 1);
    let mut snapshot = snapshot_with_self(self_pos);
    insert_remote(&mut snapshot, 2, remote_from, MirDirection::Down);
    apply_walk(&mut snapshot, 2, remote_to, MirDirection::Down, BASE);

    // 自机静止：global offset 0；远端 = center + advance；自机居中。
    let prepared = prepare(&snapshot, None, BASE + 300);
    assert_eq!(prepared.offset, (0, 0));
    assert_eq!(screen_of(&prepared, self_view(&prepared)), center);
    assert_eq!(
        screen_of(&prepared, remote_view(&prepared, 2)),
        (center.0, center.1 + 22),
        "Down 走第 3 帧推进 22px（%2 后）"
    );

    // 自机同时走一步：global offset 来自自机，远端 screen 加同一 global + 自己推进量。
    let mut self_snapshot = snapshot.clone();
    self_snapshot.apply_local_action(ActorAction::Walking, MirDirection::Right, BASE);
    self_snapshot.apply_packet_at(
        &ServerPacket::UserLocation(mir_protocol::packets::server::UserLocation {
            location: self_pos.moved(MirDirection::Right, 1),
            direction: MirDirection::Right,
        }),
        BASE,
    );
    let self_motion = match camera::motion_after_authoritative_step(
        &self_snapshot,
        self_pos,
        self_pos.moved(MirDirection::Right, 1),
    ) {
        StepMotion::Started(motion) => motion,
        other => panic!("自机走一步应 Started，实际 {other:?}"),
    };
    let mut runtime = self_snapshot.clone();
    let mut self_motion_opt = Some(self_motion);
    let prepared = camera::prepare_frame(
        &mut runtime,
        &mut self_motion_opt,
        Point::new(0, 0),
        BASE + 300,
    );
    assert_eq!(prepared.offset, (-32, 0), "自机走右第 3 帧全局偏移");
    assert_eq!(screen_of(&prepared, self_view(&prepared)), center);
    assert_eq!(
        screen_of(&prepared, remote_view(&prepared, 2)),
        (center.0 - 32, center.1 + 22),
        "远端 screen = self_center + global self offset + remote advance（不叠加两次）"
    );
}

// ---------------------------------------------------------------------------
// 排序冒烟：上下行穿过站立实体，实体只出现一次，沿用既有 position 排序。
// ---------------------------------------------------------------------------

#[test]
fn sorting_smoke_moving_players_cross_a_standing_entity_once() {
    let self_pos = Point::new(100, 100);
    let standing = Point::new(100, 101);
    for direction in [MirDirection::Down, MirDirection::Up] {
        let from = if direction == MirDirection::Down {
            Point::new(100, 100)
        } else {
            Point::new(100, 102)
        };
        let to = from.moved(direction, 2);
        let mut snapshot = snapshot_with_self(self_pos);
        insert_remote(&mut snapshot, 2, from, direction);
        insert_remote(&mut snapshot, 3, standing, MirDirection::Down);
        apply_run(&mut snapshot, 2, to, direction, BASE);
        let prepared = prepare(&snapshot, None, BASE + 300);
        // 每个对象只出现一次（无重复绘制/重复 offset）。
        let remote_count = prepared
            .entities
            .iter()
            .filter(|view| view.name.as_deref() == Some("Remote2"))
            .count();
        assert_eq!(remote_count, 1, "移动远端只应有一个 EntityView");
        let standing_count = prepared
            .entities
            .iter()
            .filter(|view| view.name.as_deref() == Some("Remote3"))
            .count();
        assert_eq!(standing_count, 1);
        // 既有排序规则：按绘制坐标 (position.y, position.x) 升序；未改排序系统。
        let mut order = prepared.entities.iter().collect::<Vec<_>>();
        order.sort_by_key(|view| (view.sort_y, view.position.x));
        let remote_index = order
            .iter()
            .position(|view| view.name.as_deref() == Some("Remote2"))
            .expect("远端存在");
        let standing_index = order
            .iter()
            .position(|view| view.name.as_deref() == Some("Remote3"))
            .expect("站立对象存在");
        assert_ne!(remote_index, standing_index);
        let remote_y = order[remote_index].position.y;
        let standing_y = order[standing_index].position.y;
        if direction == MirDirection::Down {
            assert!(remote_y < standing_y, "下行时远端仍在站立实体北侧");
        } else {
            assert!(remote_y > standing_y, "上行时远端仍在站立实体南侧");
        }
        assert_eq!(
            order[remote_index].sort_y,
            from.y.max(to.y),
            "排序 Y 用 Crystal DrawY=max(from.y,to.y)"
        );
        assert!(
            order[remote_index].sort_y > order[standing_index].sort_y,
            "上下行穿过站立实体时按 DrawY 排在南侧，不重复抵消"
        );
    }
}

// ---------------------------------------------------------------------------
// M4：状态有界（重复创建/移动/删除/同 ID 重建/切图/重连后无残留）
// ---------------------------------------------------------------------------

#[test]
fn m4_state_is_bounded_and_has_no_residue() {
    let self_pos = Point::new(100, 100);
    let mut snapshot = snapshot_with_self(self_pos);
    for id in 2..=11_u32 {
        let from = Point::new(100 + i32::try_from(id).unwrap(), 100);
        insert_remote(&mut snapshot, id, from, MirDirection::Right);
        apply_walk(
            &mut snapshot,
            id,
            from.moved(MirDirection::Right, 1),
            MirDirection::Right,
            BASE + u64::from(id),
        );
    }
    assert_eq!(
        snapshot.remote_motions.len(),
        10,
        "状态数 = 符合条件的远端玩家数"
    );
    // 删除一半：状态同步减少，不残留旧时钟。
    for id in 2..=6_u32 {
        snapshot.apply_packet_at(
            &ServerPacket::ObjectRemove(ObjectRemove { object_id: id }),
            BASE + 100,
        );
    }
    assert_eq!(snapshot.remote_motions.len(), 5);
    // 全部到期后清零。
    let mut snapshot_end = snapshot.clone();
    let mut none = None;
    camera::prepare_frame(&mut snapshot_end, &mut none, Point::new(0, 0), BASE + 2000);
    assert!(snapshot_end.remote_motions.is_empty());
    // 同 ID 反复重建/删除，无残留。
    let mut churn = snapshot_with_self(self_pos);
    for _ in 0..50 {
        insert_remote(&mut churn, 2, self_pos, MirDirection::Right);
        apply_walk(
            &mut churn,
            2,
            self_pos.moved(MirDirection::Right, 1),
            MirDirection::Right,
            BASE,
        );
        churn.apply_packet_at(
            &ServerPacket::ObjectRemove(ObjectRemove { object_id: 2 }),
            BASE + 100,
        );
        assert!(!churn.remote_motions.contains_key(&2));
    }
    // 死亡/改类型：tick 后状态被清掉。
    let mut dead = snapshot_with_self(self_pos);
    insert_remote(&mut dead, 2, self_pos, MirDirection::Right);
    apply_walk(
        &mut dead,
        2,
        self_pos.moved(MirDirection::Right, 1),
        MirDirection::Right,
        BASE,
    );
    dead.entities.get_mut(&2).expect("remote").dead = true;
    let mut none = None;
    camera::prepare_frame(&mut dead, &mut none, Point::new(0, 0), BASE + 100);
    assert!(dead.remote_motions.is_empty());
    // 切图：实体与状态一起清空。
    let mut map = snapshot_with_self(self_pos);
    insert_remote(&mut map, 2, self_pos, MirDirection::Right);
    apply_walk(
        &mut map,
        2,
        self_pos.moved(MirDirection::Right, 1),
        MirDirection::Right,
        BASE,
    );
    assert!(map.remote_motions.contains_key(&2));
    map.apply_packet_at(
        &ServerPacket::MapChanged(MapChanged {
            map_index: 1,
            file_name: "1".to_owned(),
            ..MapChanged::default()
        }),
        BASE + 100,
    );
    assert!(map.remote_motions.is_empty());
    assert!(map.entities.is_empty());
    // 重连/新会话：默认快照没有旧状态。
    assert!(WorldSnapshot::default().remote_motions.is_empty());
}

// ---------------------------------------------------------------------------
// M4：性能——1/50/200 远端 × 1000 帧，基线（无远端插值）vs 新增。
// 默认 ignored；用 `cargo test --release -p mir-client --test remote_motion -- --ignored m4_perf` 跑。
// ---------------------------------------------------------------------------

#[allow(clippy::cast_precision_loss)]
fn percentile(sorted: &[u128], pct: f64) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() as f64 - 1.0) * pct).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

#[test]
#[ignore = "性能证据手动运行：先跑 85bfc21 baseline，再跑 cargo test --release -p mir-client --test remote_motion -- --ignored m4_perf"]
#[allow(clippy::cast_precision_loss)]
fn m4_perf_1_50_200_remotes_1000_frames_baseline_vs_new() {
    use std::hint::black_box;
    use std::time::Instant;

    // 测量前固定的退化判定：新增 p95 不得比 85bfc21 真实基线高 40% + 500ns。
    const MAX_P95_RATIO: f64 = 1.40;
    const MAX_P95_SLACK_NS: u128 = 500;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let baseline_path = std::env::var("R46_BASELINE_PERF").map_or_else(
        |_| root.join("target/remote-motion-baseline.json"),
        std::path::PathBuf::from,
    );
    let baseline: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&baseline_path).unwrap_or_else(|error| {
            panic!(
                "缺少 85bfc21 baseline 原始数据 {}：{error}；先按 docs/testing/remote-motion/README.md 跑 baseline worktree",
                baseline_path.display()
            )
        }),
    )
    .expect("baseline perf JSON 解析");
    let baseline_runs = baseline["runs"].as_array().expect("baseline runs");
    let mut report = Vec::new();
    for remote_count in [1_u32, 50, 200] {
        let build = || {
            let mut snapshot = snapshot_with_self(Point::new(100, 100));
            for id in 2..2 + remote_count {
                let x = 100 + i32::try_from(id % 80).unwrap();
                insert_remote(&mut snapshot, id, Point::new(x, 100), MirDirection::Right);
            }
            snapshot
        };
        let refresh = |snapshot: &mut WorldSnapshot, now: u64| {
            for id in 2..2 + remote_count {
                let from = snapshot.entities[&id].position;
                let to = from.moved(MirDirection::Right, 1);
                apply_walk(snapshot, id, to, MirDirection::Right, now);
            }
        };
        let mut snapshot = build();
        let mut times = Vec::with_capacity(1000);
        for frame in 0..1000_u64 {
            let now = BASE + frame * 16;
            if frame % 40 == 0 {
                refresh(&mut snapshot, now);
            }
            let started = Instant::now();
            let views = snapshot.entity_views(now);
            let elapsed = started.elapsed().as_nanos();
            black_box(views.len());
            times.push(elapsed);
        }
        times.sort_unstable();
        let new_med = percentile(&times, 0.50);
        let new_p95 = percentile(&times, 0.95);
        let baseline_run = baseline_runs
            .iter()
            .find(|run| run["remotes"].as_u64() == Some(u64::from(remote_count)))
            .expect("baseline 档位");
        let base_med = u128::from(baseline_run["median_ns"].as_u64().expect("median_ns"));
        let base_p95 = u128::from(baseline_run["p95_ns"].as_u64().expect("p95_ns"));
        let threshold = (base_p95 as f64 * MAX_P95_RATIO) as u128 + MAX_P95_SLACK_NS;
        let ratio = new_p95 as f64 / base_p95.max(1) as f64;
        eprintln!(
            "remote_count={remote_count} baseline(85bfc21) median={base_med}ns p95={base_p95}ns | new median={new_med}ns p95={new_p95}ns | ratio={ratio:.2} threshold={threshold}ns"
        );
        assert!(
            new_p95 <= threshold,
            "{remote_count} 远端：新增 p95 {new_p95}ns 超过固定退化判定 {threshold}ns（baseline {base_p95}ns）"
        );
        report.push(serde_json::json!({
            "remotes": remote_count,
            "frames": 1000,
            "baseline_median_ns": base_med,
            "baseline_p95_ns": base_p95,
            "new_median_ns": new_med,
            "new_p95_ns": new_p95,
            "measured_p95_ratio": ratio,
            "fixed_threshold_p95_ns": threshold,
        }));
    }
    let path = root.join("target/remote-motion-perf.json");
    std::fs::create_dir_all(path.parent().expect("target dir")).expect("创建 target");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&serde_json::json!({
            "baseline_commit": "85bfc21",
            "baseline_commit_full": "85bfc21befae1725d73982afedabefeaca09f110",
            "baseline_source": baseline_path.display().to_string(),
            "baseline_command": "cargo test --release -p mir-client --test remote_perf_baseline -- --ignored --nocapture（85bfc21 worktree）",
            "new_command": "cargo test --release -p mir-client --test remote_motion -- --ignored m4_perf --nocapture",
            "host_note": "85bfc21 独立 worktree release 构建 vs 当前 R46 release 构建；同机、同场景 1/50/200 远端 ×1000 帧，每帧 entity_views，纳秒计时",
            "threshold": {"p95_ratio": MAX_P95_RATIO, "p95_slack_ns": MAX_P95_SLACK_NS},
            "runs": report,
        }))
        .expect("perf JSON"),
    )
    .expect("写 target/remote-motion-perf.json");
    eprintln!("perf report: {}", path.display());
}
