//! Round 66 B：真实快照消费边界反例（G1.2/G1.3）。
//!
//! 看板子单 `JJC-20261027-003-sub-29fa573c`（R66-B）。本文件只读 A 的测试私有参考模型
//! （`#[path = "support/round66_watermark.rs"] mod round66_watermark;`，**不修改**）并调用
//! 现有公开裁剪接口（`reclaimed` / `apply_projectile_ack` / `trim_projectile_events_through`），
//! 不注册消费者、不接生产、不改 wire（PARTIAL）。
//!
//! * **G1.2 `r66_minimum_preserves_slow_consumer_suffix`**：同一条真实 125 流上，慢 UI 只读
//!   `S2`（水位 2）、快 UI 读 `S4`（水位 4）；参考模型
//!   `minimum_confirmed(g, 0, 4, &[(g, 2), (g, 4)]) == 2`，把这个 2 交给**真实**
//!   `apply_projectile_ack`：只裁 e1/e2 → `(reclaimed, len, total) = (2, 2, 4)`，e3/e4 保留；
//!   再收 e5 → `(2, 3, 5)`；慢 UI 只按身份顺序新增 `[3, 4, 5]`、出生 cue 恰 3，快 UI 只新增
//!   e5、无重复；重复 sync 新增 0。
//! * **G1.3 `r66_single_ack_exposes_late_reader_gap`**：同流两条独立分支。现状反例分支按
//!   当前单 UI 语义直接 `ack 4` 裁空权威表，再收 e5 → `(4, 1, 5)`；慢 UI 从旧 `S2` 读
//!   **新表**只新增 e5、cue 1，绝对水位却跳到 5——**水位 5 不等于慢者看过 1..5**（e3/e4 被
//!   跳过，缺口 2）。安全分支用模型最低水位 2 裁剪后慢 UI 新增 `[3, 4, 5]`、cue 3。两分支
//!   差分逐项断言；旧 `S4` clone 仍含 e1..e4（旧值独立，不挽回权威历史）。反例测试本身
//!   **绿**并明确记录“现状缺口”，不把“慢者必须完整”写成永久失败。
//!
//! ## wire 与非 wire 披露
//!
//! 71 种子（源 7、目标 1..=5）与 125 事件全部经真实 `ServerPacket::encode_frame` →
//! `[len u16][index i16][payload]` → 公开 `decode_server_packet` → `WorldSnapshot::apply_packet_at`
//! 进入快照；事件身份编码在 `ObjectProjectile.destination`（eN ↔ destination N）。会话代号来自
//! 公开真实发号器 `WorldSnapshot::new_session()`（125 流没有会话代号字段，与 R62–R65 同口径）。
//! 本文件只复制 `round65_projectile_lifecycle.rs:63–145` 的最小 fixture 写法，不依赖也不修改
//! 另一个测试 target 的私有 helper。
//!
//! UI 全程冻结 `now_ms = 1000`：初读 S2/S4 后先 drain 初始 cue 并记录 view 基数，后续只比较
//! **新增** view 的 destination 顺序与**新增**出生 cue，防止旧 e1/e2 混计或推进命中时钟。
//!
//! ## 红绿同一测试文件
//!
//! 本文件在红（隔离副本把 support 的参考函数 `min` 变异为 `max`）与绿（工作树）两阶段**字节
//! 不变**：红阶段同一批真实 suffix / 事件身份 / cue 断言先失败（EXIT101），不以参考函数
//! 自身返回值失败充红。

#[path = "support/round66_watermark.rs"]
mod round66_watermark;

use mir_client::net::{ProjectileEvent, WorldSnapshot};
use mir_client::ui::UiState;
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{MirDirection, Monster, Spell};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{ObjectMonster, ObjectProjectile, decode_server_packet};
use round66_watermark::minimum_confirmed;

/// 施法者 object id（快照里必须可见，125 的未知 source 会被接收路径丢弃）。
const SOURCE_ID: u32 = 7;
/// 测试统一使用的冻结墙钟毫秒；UI 命中/飞行时钟全程不推进。
const FROZEN_MS: i64 = 1_000;
/// 出生音（`GameScene.cs:4697`，`GreatFireBall = 34`）。
const SPAWN_CUE: &str = "M34-1.wav";

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

/// 通过真实 `ObjectMonster`(71) wire 路径种入可见实体（事件 source/目的地需可解析）。
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
    let len_before = snapshot.projectile_events.len();
    snapshot.apply_packet_at(&packet, 0);
    assert_eq!(
        snapshot.projectile_events.last().copied(),
        Some(expected),
        "接收路径只追加一条事件，不改写已有前缀"
    );
    assert_eq!(
        snapshot.projectile_events.len(),
        len_before + 1,
        "接收路径只 append 一条"
    );
    assert_eq!(
        snapshot.projectile_events_total,
        total_before.saturating_add(1),
        "累计收到数 +1（饱和，不回绕）"
    );
}

/// 真实 71/125 夹具：施法者 7 与目标 1..=5 全部种入，再推入前 `events` 条 125 事件。
///
/// 事件身份为 `destination 1..=events`（eN ↔ destination N），源/目标都在快照里可见。
fn real_stream(events: u32) -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::new_session();
    seed_monster(&mut snapshot, SOURCE_ID, Point::new(10, 10));
    for destination in 1..=5 {
        seed_monster(&mut snapshot, destination, Point::new(30, 10));
    }
    for destination in 1..=events {
        push_projectile(&mut snapshot, destination);
    }
    snapshot
}

/// 推进真实快照路径到冻结时钟 `FROZEN_MS`（UI 时钟喂给 `sync`）。
fn sync_at(ui: &mut UiState, snapshot: &WorldSnapshot) {
    ui.set_time(FROZEN_MS);
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

/// 事件身份 / 绝对水位不变量：eN 的身份编码在 `destination == N`。
///
/// 1-based 水位含本条：`events[i]` 的绝对序号 = `reclaimed() + i`，水位 = `reclaimed()+i+1`。
fn assert_events_identity(snapshot: &WorldSnapshot, expected_destinations: &[u32]) {
    assert_eq!(
        snapshot.projectile_events.len(),
        expected_destinations.len(),
        "剩余事件条数（suffix 保留量）"
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
            "事件身份顺序被破坏（保留的后缀必须仍是 e3→e4→e5 原序）"
        );
        assert_eq!(
            u64::from(event.destination),
            snapshot.reclaimed() + u64::try_from(index).expect("index fits u64") + 1,
            "1-based 水位（含本条）= reclaimed()+i+1；0-based events[i] 绝对序号 = reclaimed()+i"
        );
    }
}

/// 在飞视图自 `base` 之后新增部分的身份顺序（冻结时钟下旧视图不清空，只比较新增）。
fn views_since(ui: &UiState, base: usize) -> Vec<u32> {
    let views = ui.projectiles().views();
    assert!(
        views.len() >= base,
        "冻结时钟下在飞视图不应减少：{} < {base}",
        views.len()
    );
    views[base..]
        .iter()
        .map(|view| view.destination_id)
        .collect()
}

/// 冻结时钟下的初始读：drain 初始出生 cue 并返回 view 基数，后续只比较新增。
fn initial_read(ui: &mut UiState, snapshot: &WorldSnapshot) -> usize {
    sync_at(ui, snapshot);
    let base = ui.projectiles().views().len();
    let cues = ui.take_projectile_sound_cues();
    assert_eq!(
        cues.len(),
        base,
        "初始读的出生 cue 必须与初始在飞视图一一对应"
    );
    assert!(
        ui.projectiles().hit_effects().is_empty(),
        "冻结时钟不得产生命中表现（570）"
    );
    base
}

/// G1.2：参考最低水位保护慢消费者后缀 —— 真实 S2/S4 上 min2 交真实裁剪，慢 UI 仍见 e3/e4/e5。
#[test]
fn r66_minimum_preserves_slow_consumer_suffix() {
    // 真实 S2：同代首批只发布 e1/e2（真实 125 wire → 快照追加）。
    let mut authority = real_stream(2);
    let generation = authority.session_generation;
    let published_s2 = authority.clone();
    assert_invariants(&published_s2, 0, 2, 2);
    assert_events_identity(&published_s2, &[1, 2]);

    // 慢 UI 只读 S2：水位 2。先 drain 初始 cue、记录 view 基数，后续只比较新增。
    let mut slow = ui_state();
    let slow_base = initial_read(&mut slow, &published_s2);
    assert_eq!(watermark(&slow), (generation, 2), "慢 UI 只消费 e1/e2");
    assert_eq!(slow_base, 2, "慢 UI 初始在飞视图 = e1/e2");

    // 权威再收 e3/e4 → 真实 S4(0,4,4)；快 UI 读 S4 水位 4。
    push_projectile(&mut authority, 3);
    push_projectile(&mut authority, 4);
    let published_s4 = authority.clone();
    assert_invariants(&published_s4, 0, 4, 4);
    assert_events_identity(&published_s4, &[1, 2, 3, 4]);
    let mut fast = ui_state();
    let fast_base = initial_read(&mut fast, &published_s4);
    assert_eq!(watermark(&fast), (generation, 4), "快 UI 已消费 e1..e4");
    assert_eq!(fast_base, 4, "快 UI 初始在飞视图 = e1..e4");

    // 参考模型：成员 (g,2)/(g,4)，floor0、tail4 → 最低水位 2（不能用快成员的 4）。
    let consumers = [(generation, 2), (generation, 4)];
    let through = minimum_confirmed(
        generation,
        authority.reclaimed(),
        authority.projectile_events_total,
        &consumers,
    )
    .expect("同代成员水位均在 [reclaimed,total] 内，参考模型必须 Ok");
    // 把该水位交给真实裁剪。**先断言真实 suffix/事件身份**（红变异 min→max 必须在这里先
    // 失败），参考函数自身的取值断言放在最后，避免“仅 helper 返回值失败”充红。
    let removed = authority.apply_projectile_ack(generation, through);
    assert_events_identity(&authority, &[3, 4]);
    assert_invariants(&authority, 2, 2, 4);
    assert_eq!(removed, 2, "只裁已确认前缀 e1/e2，未确认 e3/e4 必须保留");
    assert_eq!(through, 2, "参考模型最低水位 = 慢成员 2");

    // 再收 e5（真实 wire）→ (2,3,5)：未确认后缀 = e3/e4/e5。
    push_projectile(&mut authority, 5);
    assert_invariants(&authority, 2, 3, 5);
    assert_events_identity(&authority, &[3, 4, 5]);

    // 慢 UI 读半消费后的权威表：只新增 e3/e4/e5，旧 e1/e2 仍在飞视图。
    sync_at(&mut slow, &authority);
    assert_eq!(watermark(&slow), (generation, 5), "慢 UI 消费后缀后排到 5");
    assert_eq!(
        views_since(&slow, slow_base),
        vec![3, 4, 5],
        "慢 UI 新增事件身份顺序必须为 [3,4,5]（e3/e4 未被丢弃）"
    );
    assert_eq!(
        slow.take_projectile_sound_cues(),
        vec![SPAWN_CUE; 3],
        "慢 UI 新增出生 cue 恰 3（e3/e4/e5 各一次，无重播）"
    );
    assert_eq!(slow.projectiles().views().len(), 5, "旧 e1/e2 视图仍在飞");
    assert!(
        slow.projectiles().hit_effects().is_empty(),
        "冻结时钟不产生 570 命中表现"
    );

    // 快 UI 读同一权威表：只新增 e5，不重复已消费的 e1..e4。
    sync_at(&mut fast, &authority);
    assert_eq!(watermark(&fast), (generation, 5));
    assert_eq!(views_since(&fast, fast_base), vec![5], "快 UI 只新增 e5");
    assert_eq!(
        fast.take_projectile_sound_cues(),
        vec![SPAWN_CUE; 1],
        "快 UI 新增出生 cue 恰 1"
    );
    for destination in 1..=5 {
        assert_eq!(
            fast.projectiles()
                .views()
                .iter()
                .filter(|view| view.destination_id == destination)
                .count(),
            1,
            "快 UI 的 e{destination} 恰生成一次（无重复）"
        );
    }

    // 重复 sync：两个 UI 都不新增 view / cue，水位不回退。
    for ui in [&mut slow, &mut fast] {
        let base = ui.projectiles().views().len();
        sync_at(ui, &authority);
        assert_eq!(
            ui.projectiles().views().len(),
            base,
            "重复 sync 不新增 view"
        );
        assert!(
            ui.take_projectile_sound_cues().is_empty(),
            "重复 sync 不得重播出生 cue"
        );
        assert_eq!(watermark(ui), (generation, 5));
    }
}

/// G1.3：现状单 ack 反例暴露晚读者缺口；同流模型 min2 安全分支修复差分。
#[test]
fn r66_single_ack_exposes_late_reader_gap() {
    // 同一真实 125 流：e1,e2 → 发布 S2；再 e3,e4 → 发布 S4（同代、未裁剪）。
    let mut authority = WorldSnapshot::new_session();
    seed_monster(&mut authority, SOURCE_ID, Point::new(10, 10));
    for destination in 1..=5 {
        seed_monster(&mut authority, destination, Point::new(30, 10));
    }
    let generation = authority.session_generation;
    push_projectile(&mut authority, 1);
    push_projectile(&mut authority, 2);
    let published_s2 = authority.clone();
    assert_invariants(&published_s2, 0, 2, 2);
    assert_events_identity(&published_s2, &[1, 2]);
    push_projectile(&mut authority, 3);
    push_projectile(&mut authority, 4);
    let published_s4 = authority.clone();
    assert_invariants(&published_s4, 0, 4, 4);
    assert_events_identity(&published_s4, &[1, 2, 3, 4]);
    assert_eq!(
        published_s4.session_generation, generation,
        "同一流的两份发布快照必须同代"
    );
    let consumers = [(generation, 2), (generation, 4)];

    // ---- 现状反例分支：当前单 UI 语义 = 快 UI 已消费 e1..e4，直接 ack 4 裁空权威表 ----
    let mut counter = published_s4.clone();
    let mut counter_reader = ui_state();
    let counter_base = initial_read(&mut counter_reader, &published_s2);
    assert_eq!(
        watermark(&counter_reader),
        (generation, 2),
        "反例分支的慢 UI 只读过旧 S2"
    );
    assert_eq!(counter_base, 2);
    assert_eq!(
        counter.apply_projectile_ack(generation, 4),
        4,
        "当前单 UI 水位语义：快 UI 消费 4 条即 ack 4，裁空权威表"
    );
    assert_invariants(&counter, 4, 0, 4);
    assert_events_identity(&counter, &[]);
    push_projectile(&mut counter, 5);
    assert_invariants(&counter, 4, 1, 5);
    assert_events_identity(&counter, &[5]);

    // 慢 UI 从旧 S2（游标 2）读裁剪后的新表：只新增 e5、cue 1，绝对水位却跳到 5。
    sync_at(&mut counter_reader, &counter);
    assert_eq!(
        watermark(&counter_reader),
        (generation, 5),
        "现状：水位到 5（权威累计 5 条），不代表慢者看过 1..5"
    );
    let counter_new = views_since(&counter_reader, counter_base);
    assert_eq!(
        counter_new,
        vec![5],
        "现状缺口：慢 UI 只新增 e5；e3/e4 已被单 UI ack 裁掉，永不可见"
    );
    assert_eq!(
        counter_reader.take_projectile_sound_cues(),
        vec![SPAWN_CUE; 1],
        "现状：慢 UI 只新增 1 次出生 cue（e3/e4 的 cue 已随事件丢失）"
    );
    let counter_skipped = (5u64 - 2) - u64::try_from(counter_new.len()).expect("新增条数");
    assert_eq!(
        counter_skipped, 2,
        "水位前进 3 条却只新增 1 条：跳过 e3/e4 共 2 条（水位 5 ≠ 慢者看过 1..5）"
    );

    // ---- 安全分支：同流用模型最低水位 2 裁剪 → 慢 UI 完整看到后缀 ----
    let mut safe = published_s4.clone();
    let mut safe_reader = ui_state();
    let safe_base = initial_read(&mut safe_reader, &published_s2);
    assert_eq!(watermark(&safe_reader), (generation, 2));
    assert_eq!(safe_base, 2);
    let through = minimum_confirmed(
        generation,
        safe.reclaimed(),
        safe.projectile_events_total,
        &consumers,
    )
    .expect("同代成员水位均在界内，参考模型必须 Ok");
    assert_eq!(
        safe.apply_projectile_ack(generation, through),
        2,
        "min2 只裁 e1/e2"
    );
    assert_events_identity(&safe, &[3, 4]);
    assert_invariants(&safe, 2, 2, 4);
    push_projectile(&mut safe, 5);
    assert_invariants(&safe, 2, 3, 5);
    assert_events_identity(&safe, &[3, 4, 5]);
    sync_at(&mut safe_reader, &safe);
    assert_eq!(watermark(&safe_reader), (generation, 5));
    let safe_new = views_since(&safe_reader, safe_base);
    assert_eq!(
        safe_new,
        vec![3, 4, 5],
        "安全分支：慢 UI 新增事件身份顺序 [3,4,5]"
    );
    assert_eq!(
        safe_reader.take_projectile_sound_cues(),
        vec![SPAWN_CUE; 3],
        "安全分支：慢 UI 新增出生 cue 恰 3"
    );
    assert_eq!(through, 2, "安全分支的参考最低水位 = 2");

    // ---- 两分支差分（同一真流、同一水位 5，可见历史不同）----
    assert_eq!(
        (
            counter.reclaimed(),
            counter.projectile_events.len(),
            counter.projectile_events_total
        ),
        (4, 1, 5),
        "现状反例权威表"
    );
    assert_eq!(
        (
            safe.reclaimed(),
            safe.projectile_events.len(),
            safe.projectile_events_total
        ),
        (2, 3, 5),
        "安全分支权威表"
    );
    assert_eq!(
        (counter_new.len(), safe_new.len()),
        (1, 3),
        "差分：现状慢 UI 新增 1 条，安全分支新增 3 条"
    );
    assert_eq!(
        (
            counter_reader.projectiles().len(),
            safe_reader.projectiles().len()
        ),
        (3, 5),
        "差分：现状慢 UI 在飞视图 3 条（e1/e2/e5），安全分支 5 条"
    );

    // 旧 S4 clone 仍是 (0,4,4)、e1..e4 逐项不变：旧值独立，不挽回权威历史。
    assert_invariants(&published_s4, 0, 4, 4);
    assert_events_identity(&published_s4, &[1, 2, 3, 4]);
    assert_invariants(&published_s2, 0, 2, 2);
    assert_events_identity(&published_s2, &[1, 2]);
}
