//! Round 64 B：UI 本地确认水位 → 网络线程安全前缀回收的公开契约测试。
//!
//! 只使用 `pub` 接口（[`WorldSnapshot::reclaimed`] /
//! [`WorldSnapshot::trim_projectile_events_through`] /
//! [`WorldSnapshot::apply_projectile_ack`]，以及 `ui::projectile::Projectiles` 的
//! `consumed_abs_seq` / `watermark_generation`）。`app.rs` 里「只在 UI 水位前进时发
//! `NetCommand::AckProjectileEvents`」的决定由 `app.rs` 测试宿主
//! （`r64_ack_sent_only_when_watermark_advances`）覆盖，独立 crate 看不到该私有 seam。
//!
//! 语义提醒：ack 是**进程内本地确认水位**，不是 wire ack；事件表**没有**容量上限，
//! 回收只裁「已确认前缀」，慢消费者未确认后缀仍会增长。多消费者/全局最低水位未实现
//! （PARTIAL），本文件只覆盖单消费者。
//!
//! 本文件在红（no-op 桩）/绿（实现）两个阶段**字节不变**（计划 C1）；红阶段断言失败、
//! 绿阶段全部通过。

use mir_client::net::{ProjectileEvent, WorldSnapshot};
use mir_client::ui::UiState;
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{MirDirection, Monster, Spell};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::ObjectMonster;

/// 投射物源对象编号（与 `app.rs` 测试宿主同形，但独立定义避免共享私有常量）。
const SOURCE_ID: u32 = 7;
/// 投射物目标对象编号。
const DEST_ID: u32 = 8;

/// 通过真实 `ObjectMonster` 包路径把对象种进快照（`object_position` 可解析）。
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
        "seed failed: object {object_id} 不在快照里"
    );
}

/// 按真实接收路径追加一条 125 事件：表 append + 累计 `total` +1。
fn push_projectile(snapshot: &mut WorldSnapshot) {
    snapshot.projectile_events.push(ProjectileEvent {
        spell: Spell::FireBounce,
        source: SOURCE_ID,
        destination: DEST_ID,
    });
    snapshot.projectile_events_total = snapshot.projectile_events_total.saturating_add(1);
}

/// UI 自己的（会话代号, 已消费绝对序号）水位；不得拿网络快照代号去配。
fn watermark(ui: &UiState) -> (u64, u64) {
    (
        ui.projectiles().watermark_generation(),
        ui.projectiles().consumed_abs_seq(),
    )
}

fn seeded_snapshot() -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::new_session();
    seed_monster(&mut snapshot, SOURCE_ID, Point::new(10, 10));
    seed_monster(&mut snapshot, DEST_ID, Point::new(12, 10));
    snapshot
}

#[test]
fn r64_acked_prefix_reclaimed_without_losing_unacked() {
    let mut snapshot = seeded_snapshot();
    push_projectile(&mut snapshot);
    push_projectile(&mut snapshot);
    let mut ui = UiState::new(Language::ZhCn);
    ui.set_time(1_000);
    ui.sync(&snapshot);
    let (generation, through) = watermark(&ui);
    assert_eq!(
        generation, snapshot.session_generation,
        "UI 水位代号跟随快照会话代号"
    );
    assert_eq!(through, 2, "已消费绝对水位 = 2");
    assert_eq!(ui.projectiles().events_consumed(), 2);
    assert_eq!(ui.projectiles().len(), 2);

    // 网络线程应用 UI 水位：只裁已确认前缀，total 不减。
    assert_eq!(snapshot.apply_projectile_ack(generation, through), 2);
    assert_eq!(snapshot.reclaimed(), 2, "reclaimed");
    assert_eq!(snapshot.projectile_events.len(), 0, "未确认 len");
    assert_eq!(snapshot.projectile_events_total, 2, "total 永不回减");

    // 未确认的新事件在回收后继续到达：绝对序号 = reclaimed + i，UI 只消费新增。
    push_projectile(&mut snapshot);
    ui.sync(&snapshot);
    assert_eq!(
        ui.projectiles().consumed_abs_seq(),
        3,
        "回收后新事件从绝对序号 3 继续，不得按相对下标重放"
    );
    assert_eq!(ui.projectiles().events_consumed(), 3);
    assert_eq!(
        ui.projectiles().len(),
        3,
        "只新增一条视图，已回收前缀不重放"
    );

    // 再确认到 3：剩余前缀清空；total 仍为 3（累计收到数）。
    assert_eq!(snapshot.apply_projectile_ack(generation, 3), 1);
    assert_eq!(snapshot.reclaimed(), 3);
    assert_eq!(snapshot.projectile_events.len(), 0);
    assert_eq!(snapshot.projectile_events_total, 3);

    // 滞后确认（through < reclaimed）无操作，不误裁新到的未确认事件。
    push_projectile(&mut snapshot);
    assert_eq!(snapshot.apply_projectile_ack(generation, 1), 0);
    assert_eq!(
        snapshot.projectile_events.len(),
        1,
        "滞后确认不得裁掉未确认后缀"
    );
    assert_eq!(snapshot.projectile_events_total, 4);
}

#[test]
fn r64_reconnect_generation_reset_after_trim() {
    let mut snapshot = seeded_snapshot();
    for _ in 0..3 {
        push_projectile(&mut snapshot);
    }
    let mut ui = UiState::new(Language::ZhCn);
    ui.set_time(1_000);
    ui.sync(&snapshot);
    let (old_generation, old_through) = watermark(&ui);
    assert_eq!(old_through, 3);
    assert_eq!(
        snapshot.apply_projectile_ack(old_generation, old_through),
        3
    );
    assert_eq!(snapshot.reclaimed(), 3);

    // 重连：新会话快照（新代号、表归零重建）；旧代 ack 必须被网络侧忽略。
    let mut fresh = seeded_snapshot();
    assert_ne!(fresh.session_generation, old_generation, "新会话必须换代号");
    push_projectile(&mut fresh);
    push_projectile(&mut fresh);
    assert_eq!(
        fresh.apply_projectile_ack(old_generation, old_through),
        0,
        "旧代 ack 不得裁新代事件"
    );
    assert_eq!(fresh.projectile_events.len(), 2);
    assert_eq!(fresh.reclaimed(), 0);

    // 重连后首个空快照：UI 水位仍配对旧代，不提前复位（C5：空快照不得清游标）。
    let mut empty = WorldSnapshot::new_session();
    ui.sync(&empty);
    assert_eq!(
        watermark(&ui),
        (old_generation, old_through),
        "新代空快照不得把 UI 水位提前清零/换代号"
    );
    assert_eq!(
        empty.apply_projectile_ack(old_generation, old_through),
        0,
        "拿旧代游标配新代快照也必须被忽略"
    );

    // 新代首批非空事件到达：UI 成对复位并只消费新代事件，不灾难性回退。
    ui.sync(&fresh);
    let (new_generation, new_through) = watermark(&ui);
    assert_eq!(new_generation, fresh.session_generation);
    assert_eq!(new_through, 2, "新代水位从 0 重新计，恰好为 2");
    assert_eq!(ui.projectiles().events_consumed(), 2);
    assert_eq!(ui.projectiles().len(), 2, "旧代视图被清、只留新代事件");

    assert_eq!(fresh.apply_projectile_ack(new_generation, new_through), 2);
    assert_eq!(fresh.reclaimed(), 2);
    assert_eq!(fresh.projectile_events.len(), 0);
    assert_eq!(fresh.projectile_events_total, 2);
}

#[test]
fn r64_trimmed_snapshot_does_not_replay() {
    let mut snapshot = seeded_snapshot();
    push_projectile(&mut snapshot);
    push_projectile(&mut snapshot);
    let mut ui = UiState::new(Language::ZhCn);
    ui.set_time(1_000);
    ui.sync(&snapshot);
    let (generation, through) = watermark(&ui);
    assert_eq!(through, 2);
    assert_eq!(snapshot.apply_projectile_ack(generation, through), 2);
    assert_eq!(snapshot.reclaimed(), 2);
    assert_eq!(snapshot.projectile_events.len(), 0);

    // 已回收的空表（total>0、len==0）：不得复位游标、不得清在飞视图、不重放。
    ui.sync(&snapshot);
    assert_eq!(
        ui.projectiles().consumed_abs_seq(),
        2,
        "回收空表不得复位游标"
    );
    assert_eq!(ui.projectiles().events_consumed(), 2);
    assert_eq!(ui.projectiles().len(), 2, "回收空表不得清在飞视图");
    ui.sync(&snapshot);
    assert_eq!(ui.projectiles().consumed_abs_seq(), 2);
    assert_eq!(ui.projectiles().len(), 2);

    // 中途新建的消费者（C5 限制，单消费者内注册）：consumed=0 遇 reclaimed>0 时
    // 经 dropped 路径跳过已回收前缀，不重放。
    let mut late = UiState::new(Language::ZhCn);
    late.set_time(1_000);
    late.sync(&snapshot);
    assert_eq!(
        late.projectiles().events_consumed(),
        2,
        "中途消费者跳过已回收前缀"
    );
    assert_eq!(late.projectiles().len(), 0, "不重放已回收事件");
    // 后续新事件仍按绝对序号继续到达，不被已回收前缀干扰。
    push_projectile(&mut snapshot);
    late.sync(&snapshot);
    assert_eq!(late.projectiles().consumed_abs_seq(), 3);
    assert_eq!(late.projectiles().events_consumed(), 3);
    assert_eq!(late.projectiles().len(), 1, "只生成新增的一条");
}
