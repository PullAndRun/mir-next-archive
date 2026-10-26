//! Round 66 A：多消费者「最低水位」参考模型契约（G1.1/G1.4）。
//!
//! 看板子单 `JJC-20261027-003-sub-bd16c8c5`（R66-A）。本文件只测**测试私有**参考
//! 模型 `tests/support/round66_watermark.rs` 与现有 [`WorldSnapshot`] 公开裁剪接口
//! （`reclaimed` / `apply_projectile_ack` / `trim_projectile_events_through`）：
//! 不改生产代码、不注册消费者、不把模型冒充生产接线（PARTIAL）。
//!
//! * **G1.1 `r66_reference_minimum_and_validation`**：固定 `gen=g`、`floor0`、
//!   `tail4`：`[4,2]→2`、`[4,0]→0`、单成员 `[4]→4`、重复 `[2,2]→2`；表驱动覆盖
//!   五种 `ReferenceError`（含“界先于空集”优先级）；每个 Err 分支后真实 125 事件
//!   身份序列、`total`、`reclaimed()`、`len()` 全不变且裁剪调用计数为 0；正例
//!   `[4,2]→2` 交真实 ack 只裁 e1/e2。
//! * **G1.4 `r66_generation_and_late_join_fail_closed`**：`floor2`/`tail4` 的真实
//!   快照 `(2,2,4)` 上：新成员 `(g,0)`→`BelowReclaimed`、旧代 `(g-1,4)`→
//!   `GenerationMismatch`、空成员→`NoConsumers`；各 Err 分支裁剪调用 0、真实表身份/
//!   `total`/`reclaimed()`/`len()` 不变；同代合法 `[(g,2),(g,4)]→2`（慢成员停在
//!   已裁边界，ack 是 no-op，不动权威表）。
//! * **`r66_cross_member_priority_is_per_phase`**：交叉优先级——同一集合混合旧代/
//!   低于 floor/越尾成员时，按计划 `round66-plan.md:51` 的“全集合分阶段”判定
//!   （代号阶段 → 下界阶段 → 上界阶段），结果与成员排列顺序无关；含 3 组双向交叉
//!   与一组 below/beyond 混合，并在真实 125 快照上复核 Err 不裁剪。
//!
//! ## wire 与非 wire 披露
//!
//! 事件全部经真实 `ServerPacket::encode_frame` → `[len u16][index i16][payload]` →
//! 公开 `decode_server_packet` → `WorldSnapshot::apply_packet_at` 进入快照，事件身份
//! 编码在 `ObjectProjectile.destination`（eN ↔ destination N）。会话代号来自公开真实
//! 发号器 `WorldSnapshot::new_session()`（125 流没有会话代号字段，与 R62–R65 同口径）。
//! 本文件只复制 `round65_projectile_lifecycle.rs:63–145` 的**最小 fixture 写法**，不依赖
//! 也不修改另一个测试 target 的私有 helper。
//!
//! ## 红绿不变的同一测试文件
//!
//! 本文件在红（隔离副本里变异 **support**：min→max、空集→tail、忽略 generation）
//! 与绿（工作树实现）两阶段**字节不变**：红阶段由同一批 `assert_eq!` 真实运行到
//! 目标断言失败（EXIT101），不以缺函数编译错误、0 tests 或跳过充红。

#[path = "support/round66_watermark.rs"]
mod round66_watermark;

use mir_client::net::{ProjectileEvent, WorldSnapshot};
use mir_core::Point;
use mir_core::enums::{MirDirection, Monster, Spell};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{ObjectMonster, ObjectProjectile, decode_server_packet};
use round66_watermark::{ReferenceError, minimum_confirmed};

/// 施法者 object id（快照里必须可见，125 的未知 source 会被接收路径丢弃）。
const SOURCE_ID: u32 = 7;

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
    assert!(!decoded.dead, "种子目标必须存活");
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

/// 真实 125 事件夹具：`(0,0,events)` 的快照，`events[i].destination == i+1`。
fn snapshot_with_real_events(events: u32) -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::new_session();
    seed_monster(&mut snapshot, SOURCE_ID, Point::new(10, 10));
    for destination in 1..=events {
        seed_monster(&mut snapshot, destination, Point::new(30, 10));
        push_projectile(&mut snapshot, destination);
    }
    snapshot
}

/// 快照里与参考水位相关的可观测状态：事件身份序列 + `total` / `reclaimed()` / `len()`。
#[derive(Debug, PartialEq, Eq)]
struct TableState {
    /// 事件身份序列（`destination` 即 eN）。
    destinations: Vec<u32>,
    /// 累计收到数（回收后不减少）。
    total: u64,
    /// 已被确认回收的前缀条数。
    reclaimed: u64,
    /// 当前表长。
    len: usize,
}

fn table_state(snapshot: &WorldSnapshot) -> TableState {
    TableState {
        destinations: snapshot
            .projectile_events
            .iter()
            .map(|event| event.destination)
            .collect(),
        total: snapshot.projectile_events_total,
        reclaimed: snapshot.reclaimed(),
        len: snapshot.projectile_events.len(),
    }
}

/// 测试侧「Err 即不裁剪」接线：参考模型返回 Err 时**直接返回**，绝不对真实快照调用
/// `apply_projectile_ack`；只有 Ok 才推进一次真实裁剪，并用本地计数记录调用次数。
/// 这是测试内的计数包装，不修改生产代码，也不新增生产 seam。
#[derive(Default)]
struct AckProbe {
    /// 真实 `apply_projectile_ack`（内部 `trim_projectile_events_through`）调用次数。
    trim_calls: usize,
}

impl AckProbe {
    fn apply(
        &mut self,
        snapshot: &mut WorldSnapshot,
        generation: u64,
        reclaimed: u64,
        total: u64,
        consumers: &[(u64, u64)],
    ) -> Result<u64, ReferenceError> {
        let watermark = minimum_confirmed(generation, reclaimed, total, consumers)?;
        let before_reclaimed = snapshot.reclaimed();
        let before_len = u64::try_from(snapshot.projectile_events.len()).expect("事件条数可转 u64");
        self.trim_calls += 1;
        let removed = u64::try_from(snapshot.apply_projectile_ack(generation, watermark))
            .expect("移除条数可转 u64");
        let expected = watermark.saturating_sub(before_reclaimed).min(before_len);
        assert_eq!(removed, expected, "Ok 路径只裁已确认前缀，且不动 total");
        Ok(watermark)
    }
}

/// G1.1 的 Err 行：模型假设参数（真快照不变量不会给出非法界）+ 期望拒绝原因。
struct ErrCase {
    /// 用例名（失败信息用）。
    label: &'static str,
    /// 权威代号。
    generation: u64,
    /// 权威已回收边界。
    reclaimed: u64,
    /// 权威累计收到数。
    total: u64,
    /// 测试显式假设的成员集合。
    consumers: &'static [(u64, u64)],
    /// 期望拒绝原因。
    expected: ReferenceError,
}

/// G1.1：固定 `gen = 7`、`floor0`、`tail4` 的参考函数取值与五 Err 表。
const GEN: u64 = 7;
static G11_ERR_CASES: [ErrCase; 5] = [
    ErrCase {
        label: "InvalidBounds(reclaimed>total，且界先于空集检查)",
        generation: GEN,
        reclaimed: 5,
        total: 4,
        consumers: &[],
        expected: ReferenceError::InvalidBounds,
    },
    ErrCase {
        label: "NoConsumers(空成员 fail-closed，不默认取 tail)",
        generation: GEN,
        reclaimed: 0,
        total: 4,
        consumers: &[],
        expected: ReferenceError::NoConsumers,
    },
    ErrCase {
        label: "GenerationMismatch(旧代成员)",
        generation: GEN,
        reclaimed: 0,
        total: 4,
        consumers: &[(GEN.wrapping_sub(1), 4)],
        expected: ReferenceError::GenerationMismatch,
    },
    ErrCase {
        label: "BelowReclaimed(成员水位低于已裁边界)",
        generation: GEN,
        reclaimed: 2,
        total: 4,
        consumers: &[(GEN, 1)],
        expected: ReferenceError::BelowReclaimed,
    },
    ErrCase {
        label: "BeyondTail(成员水位越过累计收到数)",
        generation: GEN,
        reclaimed: 0,
        total: 4,
        consumers: &[(GEN, 5)],
        expected: ReferenceError::BeyondTail,
    },
];

#[test]
fn r66_reference_minimum_and_validation() {
    // 参考函数取值：floor0 / tail4，min 取所有成员水位最小值。
    assert_eq!(
        minimum_confirmed(GEN, 0, 4, &[(GEN, 4), (GEN, 2)]),
        Ok(2),
        "[4,2] → 2（快成员 4 不能越过慢成员 2 裁剪）"
    );
    assert_eq!(
        minimum_confirmed(GEN, 0, 4, &[(GEN, 4), (GEN, 0)]),
        Ok(0),
        "[4,0] → 0（floor=0 时 0 是合法慢成员，阻止任何回收）"
    );
    assert_eq!(
        minimum_confirmed(GEN, 0, 4, &[(GEN, 4)]),
        Ok(4),
        "单成员 [4] → 4"
    );
    assert_eq!(
        minimum_confirmed(GEN, 0, 4, &[(GEN, 2), (GEN, 2)]),
        Ok(2),
        "重复成员 [2,2] 不改变 min（不是 max/sum）"
    );

    // 五种 Err 表：每个分支都作用在真实 125 快照上，Err 后表身份/total/reclaimed/len
    // 全不变，且真实裁剪调用计数为 0。
    for case in &G11_ERR_CASES {
        let mut snapshot = snapshot_with_real_events(4);
        // 计划固定 `gen = g`：代号是进程内簿记（非 wire 字段），这里固定成表里的 g，
        // 让 Err 分支若被错误放行时会在真实 ack 上暴露，而不是被代号校验兜底掩盖。
        snapshot.session_generation = case.generation;
        let before = table_state(&snapshot);
        assert_eq!(
            before,
            TableState {
                destinations: vec![1, 2, 3, 4],
                total: 4,
                reclaimed: 0,
                len: 4,
            },
            "{}: 前置真实 125 事件身份必须是 e1..e4",
            case.label
        );
        let mut probe = AckProbe::default();
        let result = probe.apply(
            &mut snapshot,
            case.generation,
            case.reclaimed,
            case.total,
            case.consumers,
        );
        assert_eq!(
            result.err().as_ref(),
            Some(&case.expected),
            "{}: 拒绝原因按固定优先级返回",
            case.label
        );
        assert_eq!(
            probe.trim_calls, 0,
            "{}: Err 必须 fail-closed，真实裁剪调用 0 次",
            case.label
        );
        assert_eq!(
            table_state(&snapshot),
            before,
            "{}: Err 后真实 125 表事件身份序列/total/reclaimed()/len() 必须全不变",
            case.label
        );
    }

    // 正例接线：真实快照上 floor0/tail4 的 [4,2]→2 交真实 ack，只裁 e1/e2。
    let mut snapshot = snapshot_with_real_events(4);
    let generation = snapshot.session_generation;
    let mut probe = AckProbe::default();
    let watermark = probe
        .apply(
            &mut snapshot,
            generation,
            0,
            4,
            &[(generation, 4), (generation, 2)],
        )
        .expect("同代、floor0、tail4 的 [4,2] 必须返回 Ok(2)");
    assert_eq!(watermark, 2, "参考最低水位 = 慢成员水位 2");
    assert_eq!(probe.trim_calls, 1, "Ok 路径推进一次真实裁剪");
    assert_eq!(
        table_state(&snapshot),
        TableState {
            destinations: vec![3, 4],
            total: 4,
            reclaimed: 2,
            len: 2,
        },
        "真实 ack 只回收 e1/e2，e3/e4 与 total 不变"
    );
}

/// 交叉优先级用例行：混合成员集合 + 期望拒绝原因。
struct CrossCase<'a> {
    /// 用例名（失败信息用）。
    label: &'static str,
    /// 测试显式假设的成员集合。
    consumers: &'a [(u64, u64)],
    /// 期望拒绝原因。
    expected: ReferenceError,
}

/// 交叉优先级：计划 `round66-plan.md:51` 的“全集合分阶段”语义——先整集合查代号，
/// 再整集合查下界，最后整集合查上界；同一组混合输入无论成员排列如何都必须给出
/// **同一个**拒绝原因，而不是“先命中先返回”。
#[test]
fn r66_cross_member_priority_is_per_phase() {
    const STALE: u64 = GEN.wrapping_sub(1);
    const BELOW_THEN_STALE: &[(u64, u64)] = &[(GEN, 0), (STALE, 4)];
    const STALE_THEN_BELOW: &[(u64, u64)] = &[(STALE, 4), (GEN, 0)];
    const BEYOND_THEN_STALE: &[(u64, u64)] = &[(GEN, 5), (STALE, 2)];
    const STALE_THEN_BEYOND: &[(u64, u64)] = &[(STALE, 2), (GEN, 5)];
    const BEYOND_THEN_BELOW: &[(u64, u64)] = &[(GEN, 5), (GEN, 0)];
    const BELOW_THEN_BEYOND: &[(u64, u64)] = &[(GEN, 0), (GEN, 5)];
    const MIXED: &[(u64, u64)] = &[(GEN, 3), (GEN, 5), (GEN, 1)];
    let cases: [CrossCase<'_>; 7] = [
        CrossCase {
            label: "旧代成员在后：代号阶段（阶段 1）先于下界阶段（阶段 2）",
            consumers: BELOW_THEN_STALE,
            expected: ReferenceError::GenerationMismatch,
        },
        CrossCase {
            label: "旧代成员在前：结果与排列无关",
            consumers: STALE_THEN_BELOW,
            expected: ReferenceError::GenerationMismatch,
        },
        CrossCase {
            label: "旧代成员在后 + 越尾：代号阶段先于上界阶段",
            consumers: BEYOND_THEN_STALE,
            expected: ReferenceError::GenerationMismatch,
        },
        CrossCase {
            label: "旧代成员在前 + 越尾：结果与排列无关",
            consumers: STALE_THEN_BEYOND,
            expected: ReferenceError::GenerationMismatch,
        },
        CrossCase {
            label: "越尾在前 + 低于 floor：下界阶段（阶段 2）先于上界阶段（阶段 3）",
            consumers: BEYOND_THEN_BELOW,
            expected: ReferenceError::BelowReclaimed,
        },
        CrossCase {
            label: "低于 floor 在前 + 越尾：结果与排列无关",
            consumers: BELOW_THEN_BEYOND,
            expected: ReferenceError::BelowReclaimed,
        },
        CrossCase {
            label: "合法成员夹在 below/beyond 之间：整集合按阶段判",
            consumers: MIXED,
            expected: ReferenceError::BelowReclaimed,
        },
    ];
    for case in &cases {
        assert_eq!(
            minimum_confirmed(GEN, 2, 4, case.consumers).as_ref().err(),
            Some(&case.expected),
            "{}: 交叉输入必须按全集合分阶段优先级判定，与成员排列顺序无关",
            case.label
        );
    }

    // 真实 125 快照上同样 fail-closed：below/beyond 混合的交叉 Err 不裁剪真实表。
    let mut snapshot = snapshot_with_real_events(4);
    snapshot.session_generation = GEN;
    let before = table_state(&snapshot);
    let mut probe = AckProbe::default();
    let result = probe.apply(&mut snapshot, GEN, 2, 4, BEYOND_THEN_BELOW);
    assert_eq!(
        result.err().as_ref(),
        Some(&ReferenceError::BelowReclaimed),
        "混合 [(g,5),(g,0)]：阶段 2 的 below 先于阶段 3 的 beyond（plan:51 顺序）"
    );
    assert_eq!(
        probe.trim_calls, 0,
        "交叉优先级 Err 必须 fail-closed，真实裁剪调用 0 次"
    );
    assert_eq!(
        table_state(&snapshot),
        before,
        "交叉优先级 Err 后真实 125 表事件身份/total/reclaimed()/len() 必须全不变"
    );
}

/// G1.4 的 Err 行：权威代号/floor/tail + 测试显式假设的成员集合 + 期望拒绝原因。
struct LateJoinCase<'a> {
    /// 用例名（失败信息用）。
    label: &'static str,
    /// 权威代号。
    generation: u64,
    /// 权威已回收边界。
    reclaimed: u64,
    /// 权威累计收到数。
    total: u64,
    /// 测试显式假设的成员集合。
    consumers: &'a [(u64, u64)],
    /// 期望拒绝原因。
    expected: ReferenceError,
}

#[test]
fn r66_generation_and_late_join_fail_closed() {
    // floor2 / tail4 的真实快照 (reclaimed,len,total)=(2,2,4)：e3/e4 未确认。
    let mut snapshot = snapshot_with_real_events(4);
    let generation = snapshot.session_generation;
    assert_eq!(
        snapshot.trim_projectile_events_through(2),
        2,
        "floor2 由真实前缀裁剪构造（e1/e2 已回收）"
    );
    let before = table_state(&snapshot);
    assert_eq!(
        before,
        TableState {
            destinations: vec![3, 4],
            total: 4,
            reclaimed: 2,
            len: 2,
        },
        "前置真实快照必须为 (reclaimed,len,total)=(2,2,4)，剩余 e3/e4"
    );

    // 三个 Err：新成员低于 floor、旧代成员、空成员。全部 fail-closed。
    let below = [(generation, 0u64)];
    let stale = [(generation.wrapping_sub(1), 4u64)];
    let cases: [LateJoinCase<'_>; 3] = [
        LateJoinCase {
            label: "BelowReclaimed(新成员从 0 加入，低于已回收边界 2)",
            generation,
            reclaimed: 2,
            total: before.total,
            consumers: &below,
            expected: ReferenceError::BelowReclaimed,
        },
        LateJoinCase {
            label: "GenerationMismatch(旧代成员 g-1)",
            generation,
            reclaimed: 2,
            total: before.total,
            consumers: &stale,
            expected: ReferenceError::GenerationMismatch,
        },
        LateJoinCase {
            label: "NoConsumers(空成员 fail-closed)",
            generation,
            reclaimed: 2,
            total: before.total,
            consumers: &[],
            expected: ReferenceError::NoConsumers,
        },
    ];
    for case in &cases {
        let mut probe = AckProbe::default();
        let result = probe.apply(
            &mut snapshot,
            case.generation,
            case.reclaimed,
            case.total,
            case.consumers,
        );
        assert_eq!(
            result.err().as_ref(),
            Some(&case.expected),
            "{}: 拒绝原因按固定优先级返回",
            case.label
        );
        assert_eq!(
            probe.trim_calls, 0,
            "{}: Err 必须 fail-closed，真实裁剪调用 0 次",
            case.label
        );
        assert_eq!(
            table_state(&snapshot),
            before,
            "{}: Err 后真实表 e3/e4 身份/total/reclaimed()/len() 必须全不变",
            case.label
        );
    }

    // 同代合法 [(g,2),(g,4)] → 2：慢成员停在已裁边界，合法但不得推进权威回收。
    assert_eq!(
        minimum_confirmed(generation, 2, 4, &[(generation, 2), (generation, 4)]),
        Ok(2),
        "同代合法 [(g,2),(g,4)] → 2（min 取慢成员 2，且 2 == floor 合法）"
    );
    let mut probe = AckProbe::default();
    let watermark = probe
        .apply(
            &mut snapshot,
            generation,
            2,
            before.total,
            &[(generation, 2), (generation, 4)],
        )
        .expect("同代合法成员必须返回 Ok(2)");
    assert_eq!(watermark, 2, "参考最低水位 = 慢成员已裁边界 2");
    assert_eq!(probe.trim_calls, 1, "合法 Ok 路径推进一次真实裁剪");
    assert_eq!(
        table_state(&snapshot),
        before,
        "ack 到已裁边界是 no-op：真实表 e3/e4 与 total 不变"
    );
}
