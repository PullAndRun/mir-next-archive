//! Round 65 B：125 `S.ObjectProjectile` 事件生命周期（半消费 → 本地确认回收 → 同代追加）
//! 的端到端集成测试。
//!
//! 覆盖（看板子单 `JJC-20261027-002-sub-4da2d45a`，R65-B）：
//!
//! * **G1.1 精确半消费全链**：真实 125 编解码 → `WorldSnapshot` → `UiState::sync`。同代先
//!   发布 `S2 = (reclaimed,len,total) = (0,2,2)`（只含 e1/e2），UI 绝对水位 = 2；权威再发布
//!   `S4 = (0,4,4)`（e1..e4），`ack 2` 后为 `(2,2,4)`，e3/e4 保留；再推 e5 为 `(2,3,5)`。
//!   UI 只新增 e3/e4/e5，重复 sync/drain 新增 0；`ack 5` 后 `(5,0,5)`。事件身份编码在
//!   `ObjectProjectile.destination`（eN ↔ destination N）：逐条断言顺序 e1→e2→e3→e4→e5、
//!   出生 cue 累计恰 5（冻结时钟隔离命中 cue/570）、`total == len + reclaimed`、
//!   `events[i].destination == reclaimed()+i+1`（1-based 水位含本条；0-based 绝对序号 =
//!   `reclaimed()+i`）。
//! * **G1.2 裁前 clone 独立**：`WorldSnapshot` 是值类型（`#[derive(Clone)]`）。裁前 clone 在
//!   权威 `ack 2` 后仍是 `(0,4,4)` 且内容逐项不变；clone 不注册消费者、不钉住回收（没有
//!   消费者注册表，单消费者）。独立 UI 读旧 clone 仅作**离线对照**：它能整表消费旧前缀，
//!   恰好说明生产路径不能用滞后 clone 喂 UI；本文件**不证明生产多消费者**（PARTIAL）。
//! * **G1.3 组合与边界**：同 UI「消费半 → 权威 ack 半 → 再 push e5」无丢失无重放；重复/
//!   后退/越尾 ack 的 no-op 与夹尾；旧代 ack 不裁新代事件；新代空首批延迟复位、首批非空
//!   仅复位一次（views/570/cue/位置缓存清理与绝对水位重计，行为断言）；
//!   `projectile_events_total` 饱和不回绕。
//!
//! ## wire 路径与披露（看板子单要求逐条区分）
//!
//! * **wire**：`ObjectMonster`(71) 与 `ObjectProjectile`(125) 的种子/事件全部走真实
//!   `ServerPacket::encode_frame` → `[len u16][index i16][payload]` → 公开
//!   `decode_server_packet` → `WorldSnapshot::apply_packet_at`。本文件的实体位置、事件
//!   顺序与事件身份都来自这条真实编解码链，不直接构造 `ProjectileEvent` 入表。
//! * **非 wire（逐条披露）**：① 会话代号来自公开真实发号器
//!   `WorldSnapshot::new_session`（125 流没有会话代号字段，与 R62–R64 测试同口径）；
//!   ② G1.3 饱和场景直接写 `projectile_events_total = u64::MAX`（wire 无法把累计数推到
//!   饱和；只验证簿记 saturating 不溢出，不声称 wire）；③ clone/离线对照是值拷贝 +
//!   独立 UI，不是生产多消费者。
//! * **PARTIAL**：只覆盖单消费者（`app.rs` 单 UI）的进程内本地确认水位；多消费者/全局
//!   最低水位/有界容量均未实现也未由本文件证明；也不宣称 125 全链路已闭（渲染/音频
//!   不在本文件范围）。
//!
//! 本文件在红（`trim_projectile_events_through` no-op）/绿（实现）两个阶段**字节不变**：
//! 红阶段由同一批 `assert_eq!` 真实失败，绿阶段全部通过。

use mir_client::net::{ProjectileEvent, WorldSnapshot};
use mir_client::ui::UiState;
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{MirDirection, Monster, Spell};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{ObjectMonster, ObjectProjectile, decode_server_packet};

/// 施法者 object id（快照里必须可见，125 的未知 source 会被接收路径丢弃）。
const SOURCE_ID: u32 = 7;
/// 测试统一使用的冻结墙钟毫秒。
const START_MS: i64 = 1_000;
/// 出生音（`GameScene.cs:4697`，`GreatFireBall = 34`）。
const SPAWN_CUE: &str = "M34-1.wav";
/// 命中音（`GameScene.cs:4709`）。
const HIT_CUE: &str = "M34-2.wav";

/// 新 UI 状态（中文是默认语言；本文件不断言本地化文本）。
fn ui_state() -> UiState {
    UiState::new(Language::ZhCn)
}

/// 把包编码成真实服务器帧，再走公开协议入口解码（帧布局：`[len u16][index i16][payload]`）。
fn through_wire(packet: &ServerPacket) -> ServerPacket {
    let frame = packet.encode_frame();
    assert!(
        frame.len() > 4,
        "encoded frame must carry a payload: {frame:?}"
    );
    let index = i16::from_le_bytes([frame[2], frame[3]]);
    decode_server_packet(index, &frame[4..]).expect("server frame must decode into ServerPacket")
}

/// 通过真实 `ObjectMonster`(71) wire 路径种入可见实体（`object_position` 可解析）。
fn seed_monster(snapshot: &mut WorldSnapshot, object_id: u32, location: Point) {
    let packet = through_wire(&ServerPacket::ObjectMonster(ObjectMonster {
        object_id,
        name: format!("monster-{object_id}"),
        location,
        image: Monster::Skeleton,
        direction: MirDirection::Down,
        ..ObjectMonster::default()
    }));
    let ServerPacket::ObjectMonster(decoded) = &packet else {
        panic!("71 must decode into ServerPacket::ObjectMonster");
    };
    assert_eq!(decoded.object_id, object_id);
    assert_eq!(decoded.location, location, "wire 必须保留实体坐标");
    assert!(!decoded.dead, "种子目标必须存活（否则命中表现被抑制）");
    snapshot.apply_packet_at(&packet, 0);
    assert_eq!(
        snapshot.object_position(object_id),
        Some(location),
        "seed failed: monster {object_id} is not a known snapshot object"
    );
}

/// 按真实接收路径追加一条 125 事件：真实 wire → `apply_packet_at`（表 append + 累计 +1）。
fn push_projectile(snapshot: &mut WorldSnapshot, destination: u32) {
    let packet = through_wire(&ServerPacket::ObjectProjectile(ObjectProjectile {
        spell: Spell::FireBounce,
        source: SOURCE_ID,
        destination,
    }));
    let ServerPacket::ObjectProjectile(decoded) = &packet else {
        panic!("125 must decode into ServerPacket::ObjectProjectile");
    };
    assert_eq!(decoded.spell, Spell::FireBounce);
    assert_eq!(decoded.source, SOURCE_ID);
    assert_eq!(
        decoded.destination, destination,
        "wire 必须保留事件身份（目标编号）"
    );
    let expected = ProjectileEvent {
        spell: Spell::FireBounce,
        source: SOURCE_ID,
        destination,
    };
    let total_before = snapshot.projectile_events_total;
    snapshot.apply_packet_at(&packet, 0);
    assert_eq!(
        snapshot.projectile_events.last().copied(),
        Some(expected),
        "接收路径只追加一条事件，不改写已有前缀"
    );
    assert_eq!(
        snapshot.projectile_events_total,
        total_before.saturating_add(1),
        "累计收到数 +1（饱和，不回绕）"
    );
}

/// 推进真实快照路径到 `now_ms`（UI 时钟喂给 `projectiles.sync`）。
fn sync_at(ui: &mut UiState, snapshot: &WorldSnapshot, now_ms: i64) {
    ui.set_time(now_ms);
    ui.sync(snapshot);
}

/// UI 自己的（会话代号, 已消费绝对序号）水位；不得拿网络快照代号去配。
fn watermark(ui: &UiState) -> (u64, u64) {
    (
        ui.projectiles().watermark_generation(),
        ui.projectiles().consumed_abs_seq(),
    )
}

/// `(reclaimed, len, total)` 三元组与 `total == len + reclaimed` 不变量。
fn assert_invariants(snapshot: &WorldSnapshot, reclaimed: u64, len: usize, total: u64) {
    assert_eq!(
        (
            snapshot.reclaimed(),
            snapshot.projectile_events.len(),
            snapshot.projectile_events_total,
        ),
        (reclaimed, len, total),
        "(reclaimed,len,total) 三元组"
    );
    assert_eq!(
        snapshot.projectile_events_total,
        u64::try_from(len).expect("event count fits u64") + reclaimed,
        "不变量 total == len + reclaimed"
    );
}

/// 事件身份 / 绝对水位不变量：eN 的身份编码在 `destination == N`，因此可直接检查顺序。
///
/// 1-based 水位含本条：`events[i]` 的绝对序号 = `reclaimed() + i`，水位 = `reclaimed()+i+1`。
fn assert_events_identity(snapshot: &WorldSnapshot, expected_destinations: &[u32]) {
    assert_eq!(
        snapshot.projectile_events.len(),
        expected_destinations.len(),
        "剩余事件条数"
    );
    assert_eq!(
        snapshot.projectile_events_total,
        u64::try_from(expected_destinations.len()).expect("event count fits u64")
            + snapshot.reclaimed(),
        "不变量 total == len + reclaimed"
    );
    for (index, expected_destination) in expected_destinations.iter().enumerate() {
        let event = snapshot.projectile_events[index];
        assert_eq!(event.spell, Spell::FireBounce);
        assert_eq!(event.source, SOURCE_ID);
        assert_eq!(
            event.destination, *expected_destination,
            "事件身份顺序必须为 e1→e2→e3→e4→e5"
        );
        assert_eq!(
            u64::from(event.destination),
            snapshot.reclaimed() + u64::try_from(index).expect("index fits u64") + 1,
            "1-based 水位（含本条）= reclaimed()+i+1；0-based events[i] 绝对序号 = reclaimed()+i"
        );
    }
}

/// 在飞视图的身份顺序（与事件顺序 e1→eN 一一对应，且每条恰生成一次）。
fn assert_view_order(ui: &UiState, expected_destinations: &[u32]) {
    let views = ui.projectiles().views();
    assert_eq!(views.len(), expected_destinations.len(), "在飞视图条数");
    for (index, expected_destination) in expected_destinations.iter().enumerate() {
        assert_eq!(
            views[index].destination_id, *expected_destination,
            "在飞视图顺序必须与事件身份顺序一致"
        );
    }
}

/// G1.1：真实 125 全链上的精确半消费 —— S2(0,2,2) → S4(0,4,4) → ack 2=(2,2,4) →
/// push e5=(2,3,5) → UI 只新增 e3/e4/e5 → ack 5=(5,0,5)。
#[test]
fn r65_g1_1_half_consumed_prefix_through_real_125_wire() {
    // 施法者 7 与五个可区别目标 1..=5：目标距施法者 20 格 → 飞行 1000 ms（33 帧）。
    // 时钟全程冻结在 START_MS：本测试只观察「事件消费」，命中 cue/570 被时钟隔离。
    let source = Point::new(10, 10);
    let far = Point::new(30, 10);
    let mut authority = WorldSnapshot::new_session();
    seed_monster(&mut authority, SOURCE_ID, source);
    for destination in 1..=5 {
        seed_monster(&mut authority, destination, far);
    }
    let generation = authority.session_generation;

    // S2：同代首批只发布 e1/e2（真实 125 wire → 快照追加）。
    push_projectile(&mut authority, 1);
    push_projectile(&mut authority, 2);
    let published_s2 = authority.clone();
    assert_invariants(&published_s2, 0, 2, 2);
    assert_events_identity(&published_s2, &[1, 2]);

    let mut ui = ui_state();
    sync_at(&mut ui, &published_s2, START_MS);
    assert_eq!(
        watermark(&ui),
        (generation, 2),
        "S2 后 UI 绝对水位 = 2（e1/e2 已消费）"
    );
    assert_view_order(&ui, &[1, 2]);
    let mut cumulative_cues = ui.take_projectile_sound_cues();
    assert_eq!(cumulative_cues, vec![SPAWN_CUE; 2]);

    // 权威再收到 e3/e4 → S4(0,4,4)：同代号、未裁剪。
    push_projectile(&mut authority, 3);
    push_projectile(&mut authority, 4);
    assert_invariants(&authority, 0, 4, 4);
    assert_events_identity(&authority, &[1, 2, 3, 4]);
    assert_eq!(
        ui.projectiles().len(),
        2,
        "权威发布快照不改 UI（UI 还没 sync）"
    );

    // ack 2 交权威：只回收已确认前缀 e1/e2 → (2,2,4)，e3/e4 保留。
    assert_eq!(
        authority.apply_projectile_ack(generation, 2),
        2,
        "ack 2 必须回收 2 条已确认前缀"
    );
    assert_invariants(&authority, 2, 2, 4);
    assert_events_identity(&authority, &[3, 4]);

    // 再 push e5（真实 wire）→ (2,3,5)，未确认后缀 = e3/e4/e5。
    push_projectile(&mut authority, 5);
    assert_invariants(&authority, 2, 3, 5);
    assert_events_identity(&authority, &[3, 4, 5]);

    // UI 首次看到半消费后的 (2,3,5)：绝对游标从 2 继续，只新增 e3/e4/e5。
    sync_at(&mut ui, &authority, START_MS);
    assert_eq!(watermark(&ui), (generation, 5), "半消费后缀消费后排到 5");
    assert_view_order(&ui, &[1, 2, 3, 4, 5]);
    cumulative_cues.extend(ui.take_projectile_sound_cues());
    assert_eq!(
        cumulative_cues,
        vec![SPAWN_CUE; 5],
        "出生 cue 累计恰 5（e3/e4/e5 各新增一次，无重播）"
    );
    assert!(
        ui.projectiles().hit_effects().is_empty(),
        "冻结时钟不产生 570 命中表现"
    );
    for destination in 1..=5 {
        assert_eq!(
            ui.projectiles()
                .views()
                .iter()
                .filter(|view| view.destination_id == destination)
                .count(),
            1,
            "e{destination} 恰生成一次"
        );
    }

    // 重复 sync / 重复 drain：新增 0、不重播。
    sync_at(&mut ui, &authority, START_MS);
    assert_eq!(watermark(&ui), (generation, 5));
    assert_eq!(ui.projectiles().len(), 5);
    assert!(
        ui.take_projectile_sound_cues().is_empty(),
        "重复 sync 不得重播出生 cue"
    );
    assert!(ui.projectiles().sound_cues().is_empty());

    // ack 5：后缀全部确认 → (5,0,5)；UI 水位不回退、不重放、不清在飞视图。
    assert_eq!(authority.apply_projectile_ack(generation, 5), 3);
    assert_invariants(&authority, 5, 0, 5);
    assert_events_identity(&authority, &[]);
    sync_at(&mut ui, &authority, START_MS);
    assert_eq!(watermark(&ui), (generation, 5), "回收空表不得复位 UI 水位");
    assert_eq!(ui.projectiles().len(), 5, "回收不得清在飞视图");
    assert!(ui.take_projectile_sound_cues().is_empty());
    assert!(ui.projectiles().hit_effects().is_empty());
}

/// G1.2：裁前 clone 独立（值语义）——权威 ack 2/ack 5 后旧 clone 仍 (0,4,4) 且内容不变。
#[test]
fn r65_g1_2_pre_trim_clone_is_value_independent() {
    let mut authority = WorldSnapshot::new_session();
    seed_monster(&mut authority, SOURCE_ID, Point::new(10, 10));
    for destination in 1..=4 {
        seed_monster(&mut authority, destination, Point::new(30, 10));
    }
    let generation = authority.session_generation;
    for destination in 1..=4 {
        push_projectile(&mut authority, destination);
    }
    assert_invariants(&authority, 0, 4, 4);
    assert_events_identity(&authority, &[1, 2, 3, 4]);

    // 裁前 clone S4：内容逐项留档。
    let published_content: Vec<ProjectileEvent> = authority.projectile_events.clone();
    let pre_trim_clone = authority.clone();

    // 权威 ack 2：只回收权威副本的已确认前缀。
    assert_eq!(authority.apply_projectile_ack(generation, 2), 2);
    assert_invariants(&authority, 2, 2, 4);
    assert_events_identity(&authority, &[3, 4]);

    // 旧 clone 仍 (0,4,4)，e1..e4 逐项不变。
    assert_invariants(&pre_trim_clone, 0, 4, 4);
    assert_eq!(
        pre_trim_clone.projectile_events, published_content,
        "裁前 clone 内容逐项不变（e1..e4）"
    );
    assert_eq!(pre_trim_clone.projectile_events_total, 4);
    assert_eq!(pre_trim_clone.reclaimed(), 0);
    assert_eq!(pre_trim_clone.session_generation, generation);

    // 权威继续 push e5 与 ack 5：clone 仍 (0,4,4)，不被后续回收影响。
    push_projectile(&mut authority, 5);
    assert_invariants(&authority, 2, 3, 5);
    assert_eq!(authority.apply_projectile_ack(generation, 5), 3);
    assert_invariants(&authority, 5, 0, 5);
    assert_invariants(&pre_trim_clone, 0, 4, 4);
    assert_eq!(pre_trim_clone.projectile_events, published_content);

    // clone 不注册消费者、不钉住回收：没有消费者注册 API，权威回收只由自身水位决定。
    // 下面用独立 UI 读旧 clone 仅作离线对照（PARTIAL：不证明生产多消费者）。
    let mut offline = ui_state();
    sync_at(&mut offline, &pre_trim_clone, START_MS);
    assert_eq!(
        watermark(&offline),
        (generation, 4),
        "离线 UI 读旧 clone 能看到整表 4 条（已回收前缀仍在 clone 里）"
    );
    assert_view_order(&offline, &[1, 2, 3, 4]);
    assert_eq!(
        offline.take_projectile_sound_cues(),
        vec![SPAWN_CUE; 4],
        "旧 clone 会整表重播；生产路径不得用滞后 clone 喂 UI"
    );

    // 同一 UI 重复同步旧 clone：不重播、不新增 cue。
    sync_at(&mut offline, &pre_trim_clone, START_MS);
    assert_eq!(watermark(&offline), (generation, 4));
    assert_eq!(offline.projectiles().len(), 4);
    assert!(offline.take_projectile_sound_cues().is_empty());
    assert!(offline.projectiles().hit_effects().is_empty());
}

/// G1.3（组合）：同 UI「消费半 → 权威 ack 半 → 再 push e5」无丢失、无重放。
#[test]
fn r65_g1_3_consume_half_ack_half_then_push_more_keeps_suffix() {
    let mut authority = WorldSnapshot::new_session();
    seed_monster(&mut authority, SOURCE_ID, Point::new(10, 10));
    for destination in 1..=5 {
        seed_monster(&mut authority, destination, Point::new(30, 10));
    }
    let generation = authority.session_generation;

    // 同 UI 已消费 e1/e2（水位 2）。
    push_projectile(&mut authority, 1);
    push_projectile(&mut authority, 2);
    let published_s2 = authority.clone();
    let mut ui = ui_state();
    sync_at(&mut ui, &published_s2, START_MS);
    assert_eq!(watermark(&ui), (generation, 2));
    assert_view_order(&ui, &[1, 2]);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE; 2]);

    // 权威发布 S4 并 ack 半：UI 首次看到的 S4 已是 (2,2,4)，只能消费 e3/e4。
    push_projectile(&mut authority, 3);
    push_projectile(&mut authority, 4);
    assert_invariants(&authority, 0, 4, 4);
    assert_events_identity(&authority, &[1, 2, 3, 4]);
    assert_eq!(authority.apply_projectile_ack(generation, 2), 2);
    assert_invariants(&authority, 2, 2, 4);
    assert_events_identity(&authority, &[3, 4]);

    sync_at(&mut ui, &authority, START_MS);
    assert_eq!(
        watermark(&ui),
        (generation, 4),
        "已确认前缀不重放，后缀 e3/e4 各消费一次"
    );
    assert_view_order(&ui, &[1, 2, 3, 4]);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE; 2]);

    // 重复 sync：不重播。
    sync_at(&mut ui, &authority, START_MS);
    assert_eq!(watermark(&ui), (generation, 4));
    assert!(ui.take_projectile_sound_cues().is_empty());

    // 再 push e5 → (2,3,5)：UI 只新增 e5。
    push_projectile(&mut authority, 5);
    assert_invariants(&authority, 2, 3, 5);
    assert_events_identity(&authority, &[3, 4, 5]);
    sync_at(&mut ui, &authority, START_MS);
    assert_eq!(watermark(&ui), (generation, 5));
    assert_view_order(&ui, &[1, 2, 3, 4, 5]);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);
    assert_eq!(
        ui.projectiles().consumed_abs_seq(),
        authority.projectile_events_total,
        "绝对水位跟上累计收到数（无丢失）"
    );
    for destination in 1..=5 {
        assert_eq!(
            ui.projectiles()
                .views()
                .iter()
                .filter(|view| view.destination_id == destination)
                .count(),
            1,
            "e{destination} 恰生成一次（无丢失/无丢失性重放）"
        );
    }

    // 无重放：重复 sync + drain。
    sync_at(&mut ui, &authority, START_MS);
    assert_eq!(watermark(&ui), (generation, 5));
    assert!(ui.take_projectile_sound_cues().is_empty());
    assert!(ui.projectiles().hit_effects().is_empty());
}

/// G1.3（边界）：重复/后退/越尾 ack、旧代 ack、total 饱和。
#[test]
fn r65_g1_3_repeat_backward_tail_and_old_generation_acks_are_safe() {
    let mut authority = WorldSnapshot::new_session();
    seed_monster(&mut authority, SOURCE_ID, Point::new(10, 10));
    for destination in 1..=4 {
        seed_monster(&mut authority, destination, Point::new(30, 10));
    }
    let generation = authority.session_generation;
    for destination in 1..=4 {
        push_projectile(&mut authority, destination);
    }
    assert_invariants(&authority, 0, 4, 4);
    assert_events_identity(&authority, &[1, 2, 3, 4]);

    assert_eq!(authority.apply_projectile_ack(generation, 2), 2);
    assert_invariants(&authority, 2, 2, 4);
    assert_events_identity(&authority, &[3, 4]);

    // 重复 ack 2 / 后退 ack 1 / ack 0：全部 no-op，不误裁未确认后缀。
    for through in [2_u64, 1, 0] {
        assert_eq!(
            authority.apply_projectile_ack(generation, through),
            0,
            "through={through} 的滞后/重复确认必须 no-op"
        );
        assert_invariants(&authority, 2, 2, 4);
        assert_events_identity(&authority, &[3, 4]);
    }

    // 越尾 ack 夹到尾：只裁剩余 2 条，total 不减少。
    assert_eq!(authority.apply_projectile_ack(generation, u64::MAX), 2);
    assert_invariants(&authority, 4, 0, 4);
    assert_events_identity(&authority, &[]);

    // 旧代 ack 不裁新代事件（真实发号器换代）。
    let mut fresh = WorldSnapshot::new_session();
    seed_monster(&mut fresh, SOURCE_ID, Point::new(10, 10));
    for destination in 1..=2 {
        seed_monster(&mut fresh, destination, Point::new(30, 10));
    }
    let fresh_generation = fresh.session_generation;
    assert_ne!(fresh_generation, generation, "真实发号器必须换代");
    push_projectile(&mut fresh, 1);
    push_projectile(&mut fresh, 2);
    assert_eq!(
        fresh.apply_projectile_ack(generation, 2),
        0,
        "旧代 ack 必须被忽略"
    );
    assert_invariants(&fresh, 0, 2, 2);
    assert_events_identity(&fresh, &[1, 2]);
    assert_eq!(fresh.apply_projectile_ack(fresh_generation, 2), 2);
    assert_invariants(&fresh, 2, 0, 2);

    // total 饱和：直接写 total=u64::MAX（非 wire，见模块文档披露）。真实 125 追加仍饱和
    // 不回绕，ack 只按前缀裁剪且不改变 total。
    let mut saturated = WorldSnapshot::new_session();
    seed_monster(&mut saturated, SOURCE_ID, Point::new(10, 10));
    seed_monster(&mut saturated, 1, Point::new(30, 10));
    seed_monster(&mut saturated, 2, Point::new(30, 10));
    push_projectile(&mut saturated, 1);
    saturated.projectile_events_total = u64::MAX;
    assert_eq!(
        saturated.reclaimed(),
        u64::MAX - 1,
        "len=1 时饱和 rec = MAX-1"
    );
    push_projectile(&mut saturated, 2);
    assert_eq!(
        saturated.projectile_events_total,
        u64::MAX,
        "饱和后真实 125 追加不回绕"
    );
    assert_eq!(saturated.projectile_events.len(), 2);
    assert_eq!(saturated.reclaimed(), u64::MAX - 2);
    assert_eq!(
        saturated.apply_projectile_ack(saturated.session_generation, 1),
        0,
        "已回收前缀之下的 ack 必须 no-op"
    );
    assert_eq!(saturated.projectile_events.len(), 2);
    assert_eq!(
        saturated.apply_projectile_ack(saturated.session_generation, u64::MAX),
        2
    );
    assert_invariants(&saturated, u64::MAX, 0, u64::MAX);
    assert_events_identity(&saturated, &[]);
}

/// G1.3（新代）：空首批延迟复位；首批非空仅复位一次，清 views/570/cue/位置缓存并重计水位。
#[test]
fn r65_g1_3_new_generation_first_payload_resets_once_and_clears_state() {
    // 旧会话：同格目标 1 在 START+1 到达（570 + 命中 cue），远距目标 2 留一架在飞。
    let mut old = WorldSnapshot::new_session();
    seed_monster(&mut old, SOURCE_ID, Point::new(10, 10));
    seed_monster(&mut old, 1, Point::new(10, 10));
    seed_monster(&mut old, 2, Point::new(30, 10));
    let old_generation = old.session_generation;
    push_projectile(&mut old, 1);
    push_projectile(&mut old, 2);

    let mut ui = ui_state();
    sync_at(&mut ui, &old, START_MS);
    assert_eq!(watermark(&ui), (old_generation, 2));
    assert_eq!(ui.projectiles().len(), 2);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE; 2]);

    sync_at(&mut ui, &old, START_MS + 1);
    assert_eq!(ui.projectiles().len(), 1, "同格 e1 到达、远距 e2 仍在飞");
    assert_eq!(ui.projectiles().views()[0].destination_id, 2);
    let effects = ui.projectiles().hit_effects();
    assert_eq!(effects.len(), 1, "e1 命中存活目标 1，留下 570 残影");
    assert_eq!(effects[0].target_id, 1);
    assert_eq!(effects[0].started_ms, START_MS + 1);
    assert_eq!(
        ui.projectiles().sound_cues(),
        [HIT_CUE],
        "命中 cue 留在队列里，等待跨代丢弃断言"
    );
    assert_eq!(watermark(&ui), (old_generation, 2));

    // 新代号首个空快照：只延迟复位，旧 views/570/cue/水位原样保留。
    let mut new_session = WorldSnapshot::new_session();
    let new_generation = new_session.session_generation;
    assert!(new_generation > old_generation, "真实发号器必须换代");
    sync_at(&mut ui, &new_session, START_MS + 2);
    assert_eq!(
        watermark(&ui),
        (old_generation, 2),
        "新代空首批不得提前复位水位"
    );
    assert_eq!(ui.projectiles().len(), 1, "空首批不得清在飞视图");
    assert_eq!(
        ui.projectiles().hit_effects().len(),
        1,
        "空首批不得清旧 570"
    );
    assert_eq!(
        ui.projectiles().sound_cues(),
        [HIT_CUE],
        "空首批不得丢旧 cue"
    );

    // 新代首批非空：恰复位一次（清 views/570/cue/位置缓存，水位归零后重计）。
    seed_monster(&mut new_session, SOURCE_ID, Point::new(40, 40));
    push_projectile(&mut new_session, 99);
    sync_at(&mut ui, &new_session, START_MS + 3);
    assert_eq!(watermark(&ui), (new_generation, 1), "新代水位从 0 重计到 1");
    assert!(ui.projectiles().is_empty(), "旧代在飞视图被清");
    assert!(ui.projectiles().hit_effects().is_empty(), "旧代 570 被清");
    assert!(
        ui.take_projectile_sound_cues().is_empty(),
        "旧代命中 cue 被跨代丢弃"
    );

    // 位置缓存跨代已清：旧代见过的目标 1 不得在新代复活（新代快照无对象 1）。
    push_projectile(&mut new_session, 1);
    sync_at(&mut ui, &new_session, START_MS + 4);
    assert_eq!(watermark(&ui), (new_generation, 2));
    assert!(
        ui.projectiles().is_empty(),
        "旧代位置缓存不得跨代复活（否则 125 会生成实体）"
    );
    assert!(ui.take_projectile_sound_cues().is_empty());

    // 首批非空仅复位一次：新代已知目标 5 正常生成，绝对水位连续推进。
    seed_monster(&mut new_session, 5, Point::new(60, 40));
    push_projectile(&mut new_session, 5);
    sync_at(&mut ui, &new_session, START_MS + 5);
    assert_eq!(watermark(&ui), (new_generation, 3));
    assert_view_order(&ui, &[5]);
    let first_started = ui.projectiles().views()[0].started_ms;
    assert_eq!(first_started, START_MS + 5);
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);
    assert!(ui.projectiles().hit_effects().is_empty());

    // 再来一条新代事件：只追加，不二次复位（在飞视图与绝对水位都保留）。
    seed_monster(&mut new_session, 6, Point::new(70, 40));
    push_projectile(&mut new_session, 6);
    sync_at(&mut ui, &new_session, START_MS + 6);
    assert_eq!(watermark(&ui), (new_generation, 4));
    assert_view_order(&ui, &[5, 6]);
    assert_eq!(
        ui.projectiles().views()[0].started_ms,
        first_started,
        "首批非空复位恰一次，后续同代事件不得重建视图"
    );
    assert_eq!(ui.take_projectile_sound_cues(), vec![SPAWN_CUE]);

    // 幂等：重复 sync 不重播。
    sync_at(&mut ui, &new_session, START_MS + 6);
    assert_eq!(watermark(&ui), (new_generation, 4));
    assert_eq!(ui.projectiles().len(), 2);
    assert!(ui.take_projectile_sound_cues().is_empty());
}
