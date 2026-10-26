//! Round 64 A: 投射物事件「本地确认水位 + 安全前缀回收」的公开契约测试。
//!
//! 本文件只测 [`WorldSnapshot`] 的 `pub` 接口（`reclaimed` /
//! `trim_projectile_events_through` / `apply_projectile_ack`）。需要私有
//! `handle_command` 与真实 `TcpStream` 才能证明的「ack 命令对 stream 写出 0 字节」
//! 放在 `src/net.rs` 的 `net::tests::r64_ack_command_emits_no_client_packet`
//! （单元测试），因为独立集成测试 crate 看不到私有函数。
//!
//! 本文件在红（桩）/绿（实现）两个阶段**字节不变**（计划 C1 要求同一测试文件 SHA
//! 相等）；红阶段断言失败、绿阶段全部通过。

use mir_client::net::{ProjectileEvent, WorldSnapshot};
use mir_core::enums::Spell;

fn snapshot_with_projectiles(count: u32) -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::new_session();
    for destination in 1..=count {
        push(&mut snapshot, destination);
    }
    snapshot
}

fn push(snapshot: &mut WorldSnapshot, destination: u32) {
    snapshot.projectile_events_total = snapshot.projectile_events_total.saturating_add(1);
    snapshot.projectile_events.push(ProjectileEvent {
        spell: Spell::FireBounce,
        source: 1,
        destination,
    });
}

#[test]
fn r64_trim_acked_prefix_keeps_absolute_total() {
    let mut snapshot = snapshot_with_projectiles(5);
    assert_eq!(snapshot.projectile_events_total, 5);
    assert_eq!(snapshot.reclaimed(), 0, "未回收时 reclaimed 为 0");
    let removed = snapshot.trim_projectile_events_through(3);
    assert_eq!(removed, 3, "只移除绝对序号 ≤ through 的已确认前缀");
    assert_eq!(snapshot.projectile_events.len(), 2, "未确认后缀必须保留");
    assert_eq!(
        snapshot.projectile_events_total, 5,
        "total 是累计收到数，回收后仍不减少"
    );
    assert_eq!(snapshot.reclaimed(), 3);
    assert_eq!(
        snapshot.reclaimed() + u64::try_from(snapshot.projectile_events.len()).expect("长度"),
        snapshot.projectile_events_total,
        "不变量：reclaimed == total - len"
    );
    assert_eq!(
        snapshot.projectile_events[0].destination, 4,
        "events[i] 的绝对序号 = reclaimed()+i（首条剩余是第 4 条 push）"
    );
    assert_eq!(snapshot.projectile_events[1].destination, 5);
}

#[test]
fn r64_trim_noop_below_base_and_clamps_above_total() {
    let mut snapshot = snapshot_with_projectiles(4);
    assert_eq!(snapshot.trim_projectile_events_through(2), 2);
    assert_eq!(snapshot.projectile_events.len(), 2);
    assert_eq!(
        snapshot.trim_projectile_events_through(1),
        0,
        "through < reclaimed 必须 no-op（滞后/重复确认不误裁）"
    );
    assert_eq!(snapshot.projectile_events.len(), 2);
    assert_eq!(
        snapshot.trim_projectile_events_through(u64::MAX),
        2,
        "through ≥ total 夹取到表尾（不 panic）"
    );
    assert_eq!(snapshot.projectile_events.len(), 0);
    assert_eq!(
        snapshot.projectile_events_total, 4,
        "越界夹取后 total 仍不减"
    );
    assert_eq!(snapshot.reclaimed(), 4);
    // 空表：`total == 0 → reclaimed == 0`，ack 是 no-op，不 panic。
    let mut empty = WorldSnapshot::default();
    assert_eq!(empty.reclaimed(), 0);
    assert_eq!(empty.trim_projectile_events_through(9), 0);
    assert_eq!(empty.projectile_events_total, 0);
}

#[test]
fn r64_stale_generation_ack_is_ignored() {
    let mut snapshot = snapshot_with_projectiles(3);
    let generation = snapshot.session_generation;
    let stale = generation.wrapping_add(1);
    assert_eq!(
        snapshot.apply_projectile_ack(stale, 2),
        0,
        "旧代/陌生代 ack 必须被忽略：不得裁掉当前代事件"
    );
    assert_eq!(snapshot.projectile_events.len(), 3);
    assert_eq!(snapshot.projectile_events_total, 3);
    assert_eq!(snapshot.reclaimed(), 0);
    assert_eq!(
        snapshot.apply_projectile_ack(generation, 2),
        2,
        "同代 ack 才回收已确认前缀"
    );
    assert_eq!(snapshot.projectile_events.len(), 1);
    assert_eq!(snapshot.projectile_events_total, 3);
    assert_eq!(snapshot.reclaimed(), 2);
}

#[test]
fn r64_push_after_trim_keeps_cursor_arithmetic() {
    let mut snapshot = snapshot_with_projectiles(3);
    assert_eq!(snapshot.trim_projectile_events_through(2), 2);
    push(&mut snapshot, 4);
    push(&mut snapshot, 5);
    assert_eq!(snapshot.projectile_events_total, 5);
    assert_eq!(snapshot.projectile_events.len(), 3);
    assert_eq!(snapshot.reclaimed(), 2);
    for (index, event) in snapshot.projectile_events.iter().enumerate() {
        let abs_seq = snapshot.reclaimed() + u64::try_from(index).expect("事件下标");
        assert_eq!(
            event.destination,
            u32::try_from(abs_seq + 1).expect("绝对序号"),
            "events[i] 的绝对序号必须是 reclaimed()+i"
        );
    }
}
