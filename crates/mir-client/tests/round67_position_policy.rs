//! Round 67 A：位置缓存容量与保护引用契约（G1.1/G1.2）。
//!
//! 看板子单 `JJC-20261027-004-sub-71e056ca`（R67-A）。本文件只调用 A 的测试私有参考模型
//! （`#[path = "support/round67_positions.rs"] mod round67_positions;`，**不修改**生产、
//! 不注册缓存、不改 wire）：`known_positions` 仍是无界缓存，生产限容继续 PARTIAL。
//!
//! * **G1.1 `r67_eviction_order_and_budget`**：entries=`[(10,1),(20,2),(30,3),(40,4)]`、
//!   protected=`[10]`、budget=2 → 淘汰按 `(tick, id)` 升序的最少前缀 `[20,30]`、
//!   `remaining=2、over_budget=0；同一集合反序排列结果相同；并列` tick
//!   `[(20,1),(10,1)]`/budget=1 → 只淘汰 id 更小的 `[10]`（budget=2 时两个都保留）。
//! * **G1.2 `r67_protected_overflow_zero_and_duplicates`**：固定
//!   entries=`[(10,1),(20,2),(30,3)]`；全保护/budget=2 → `不淘汰、remaining=3、over_budget=1`
//!   （保护超额；不能以限容名义丢活跃引用）；budget=0 仅保护 10 → 淘汰 `[20,30]`、
//!   `remaining=1、over_budget=1；budget>=len` 不删；protected=`[10,10,999]` 等价 `[10]`；
//!   重复 entry id → `Err(DuplicateId)` 且输入不变。每支都有数值断言。
//!
//! 红绿同一测试文件：本文件在红（隔离副本把 support 的实现变异：排序降序 / 忽略保护集合 /
//! 预算比较反转）与绿（工作树）两阶段**字节不变**；红阶段真实运行到本文件的 assert 并报出
//! 左右实值，不以 support 自身返回值失败充红。
//!
//! 边界（本文件不主张）：预算与 ticks 是显式测试输入，排序/最小前缀/保护超额是本项目提案，
//! 非 Crystal 原作行为、非生产 LRU/限容；自动保护集、map epoch/ID 复用与本轮无关。

#[path = "support/round67_positions.rs"]
mod round67_positions;

use round67_positions::{EvictionPlan, PlanError, plan_evictions};

/// 断言完整提案三元组：`evict` 顺序、`remaining`、`over_budget` 逐项数值比较。
fn assert_plan(entries: &[(u32, u64)], protected: &[u32], budget: usize, expected: &EvictionPlan) {
    let plan = plan_evictions(entries, protected, budget).expect("唯一 id 输入必须给出提案");
    assert_eq!(
        plan.evict, expected.evict,
        "entries={entries:?} protected={protected:?} budget={budget}: 淘汰顺序必须是 (tick, id) 升序的最小前缀"
    );
    assert_eq!(
        plan.remaining, expected.remaining,
        "entries={entries:?} protected={protected:?} budget={budget}: remaining = entries.len - evict.len"
    );
    assert_eq!(
        plan.over_budget, expected.over_budget,
        "entries={entries:?} protected={protected:?} budget={budget}: over_budget = remaining.saturating_sub(budget)"
    );
}

/// G1.1：淘汰顺序取 `(tick, id)` 升序的最小前缀，预算与排列不影响结果，并列 tick 按 id。
#[test]
fn r67_eviction_order_and_budget() {
    // 分支 1（基准）：保护 10、预算 2；候选 (20,2)/(30,3)/(40,4) 的最小前缀恰是 [20,30]。
    assert_plan(
        &[(10, 1), (20, 2), (30, 3), (40, 4)],
        &[10],
        2,
        &EvictionPlan {
            evict: vec![20, 30],
            remaining: 2,
            over_budget: 0,
        },
    );

    // 分支 2（排列无关）：同一集合反序输入，结果必须与分支 1 完全一致。
    assert_plan(
        &[(40, 4), (30, 3), (20, 2), (10, 1)],
        &[10],
        2,
        &EvictionPlan {
            evict: vec![20, 30],
            remaining: 2,
            over_budget: 0,
        },
    );

    // 分支 3（并列 tick）：tick 相同只能比 id；(20,1)/(10,1)、budget=1 → 只淘汰 [10]。
    assert_plan(
        &[(20, 1), (10, 1)],
        &[],
        1,
        &EvictionPlan {
            evict: vec![10],
            remaining: 1,
            over_budget: 0,
        },
    );
    // 同输入 budget=2 → 预算够，不删任何条目。
    assert_plan(
        &[(20, 1), (10, 1)],
        &[],
        2,
        &EvictionPlan {
            evict: vec![],
            remaining: 2,
            over_budget: 0,
        },
    );
}

/// G1.2：保护超额、budget=0/≥len、未知/重复保护、重复 entry id 的 fail-closed 与输入不变。
#[test]
fn r67_protected_overflow_zero_and_duplicates() {
    const ENTRIES: [(u32, u64); 3] = [(10, 1), (20, 2), (30, 3)];

    // 分支 1（全保护超额）：三个 id 全受保护 + budget=2 → 不淘汰、remaining=3、over_budget=1。
    assert_plan(
        &ENTRIES,
        &[10, 20, 30],
        2,
        &EvictionPlan {
            evict: vec![],
            remaining: 3,
            over_budget: 1,
        },
    );

    // 分支 2（budget=0 仍保护）：只保护 10、budget=0 → 淘汰全部未保护候选 [20,30]，
    // 受保护的 10 保留 → remaining=1、over_budget=1。
    assert_plan(
        &ENTRIES,
        &[10],
        0,
        &EvictionPlan {
            evict: vec![20, 30],
            remaining: 1,
            over_budget: 1,
        },
    );

    // 分支 3（budget>=len 不删）：等号、大于与 usize::MAX 三个边界都不淘汰。
    assert_plan(
        &ENTRIES,
        &[10],
        3,
        &EvictionPlan {
            evict: vec![],
            remaining: 3,
            over_budget: 0,
        },
    );
    assert_plan(
        &ENTRIES,
        &[10],
        4,
        &EvictionPlan {
            evict: vec![],
            remaining: 3,
            over_budget: 0,
        },
    );
    assert_plan(
        &ENTRIES,
        &[10],
        usize::MAX,
        &EvictionPlan {
            evict: vec![],
            remaining: 3,
            over_budget: 0,
        },
    );

    // 分支 4（未知/重复保护无副作用）：[10,10,999] 等价 [10]，结果同为 [20,30]/1/0。
    assert_plan(
        &ENTRIES,
        &[10, 10, 999],
        1,
        &EvictionPlan {
            evict: vec![20, 30],
            remaining: 1,
            over_budget: 0,
        },
    );
    assert_plan(
        &ENTRIES,
        &[10],
        1,
        &EvictionPlan {
            evict: vec![20, 30],
            remaining: 1,
            over_budget: 0,
        },
    );
    // 只有未知 id 的 protected 等价空保护集：不保护任何条目 → 淘汰 [10,20]。
    assert_plan(
        &ENTRIES,
        &[999],
        1,
        &EvictionPlan {
            evict: vec![10, 20],
            remaining: 1,
            over_budget: 0,
        },
    );

    // 分支 5（重复 entry id fail-closed）：任意重复 id 返回 Err(DuplicateId)，输入不被修改。
    let duplicated = [(10, 1), (20, 2), (10, 3)];
    let before = duplicated;
    assert_eq!(
        plan_evictions(&duplicated, &[10], 2),
        Err(PlanError::DuplicateId),
        "重复 entry id 必须返回 Err(DuplicateId)（唯一性先于保护/预算）"
    );
    assert_eq!(duplicated, before, "重复 id 错误不得修改输入");
    // 重复 id 即使落在保护集合里也照样报错。
    assert_eq!(
        plan_evictions(&[(10, 1), (10, 2)], &[10], 0),
        Err(PlanError::DuplicateId),
        "重复 id 被保护也不能豁免唯一性检查"
    );
}
