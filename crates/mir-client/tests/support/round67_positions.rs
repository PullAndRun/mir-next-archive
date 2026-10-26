//! Round 67 A：位置缓存容量与保护引用契约的**测试私有淘汰提案**（仅 integration-test support）。
//!
//! 唯一引用方是 `crates/mir-client/tests/round67_position_policy.rs` 的
//! `#[path = "support/round67_positions.rs"] mod round67_positions;`。本文件
//! **不链接生产**：`crates/mir-client/src/**` 不引用它，生产也没有容量上限、淘汰或
//! 保护引用集合——`known_positions` 仍是无界 `HashMap`（计划 round67-plan.md:29–32、
//! 89–96 行：PARTIAL）。
//!
//! 语义（计划 41–48 行）：
//!
//! * `entries` 是**唯一** `(id, last_used_tick)` 集合：任意重复 id 先返回
//!   [`PlanError::DuplicateId`]，且不修改输入；`protected` 中重复/未知 id 无副作用、
//!   不增加条目（只是测试显式假设，不是生产自动收集的引用集）。
//! * 候选 = 未保护条目，按 `(tick, id)` 升序；删**最少前缀**使
//!   `remaining <= budget`，候选耗尽即停——受保护条目允许超额，绝不因限容丢弃活跃引用。
//! * `evict` 按该顺序给出 id；`remaining = entries.len() - evict.len()`；
//!   `over_budget = remaining.saturating_sub(budget)`。
//! * budget=0 仍保护全部受保护条目；budget>=len 不删；相同 tick 按 id 稳定；
//!   输入排列不影响结果。
//!
//! 这是本项目**待裁决测试提案**，不归因 Crystal（原作没有全局位置 LRU/容量/保护集），
//! 不进入玩法或生产运行时；只是把“淘汰候选”固定成可失败契约，供未来容量讨论裁定。

/// 一次淘汰提案的结果；`evict` 按候选 `(tick, id)` 升序给出被淘汰条目的 id。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct EvictionPlan {
    /// 被淘汰条目的 id，按 `(tick, id)` 升序（与 `entries` 排列无关）。
    pub evict: Vec<u32>,
    /// 淘汰后保留的条目数：`entries.len() - evict.len()`（保护超额仍保留）。
    pub remaining: usize,
    /// 保留部分相对预算的超额：`remaining.saturating_sub(budget)`（保护集允许 > 0）。
    pub over_budget: usize,
}

/// 淘汰提案的拒绝原因：`entries` 违反唯一 id 不变量。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PlanError {
    /// 任意重复 entry id（条目集合不是唯一集合）：fail-closed，不修改输入。
    DuplicateId,
}

/// 计算测试私有的淘汰提案；语义见文件头注释（计划 41–48 行）。
///
/// 返回 `Err(DuplicateId)` 当且仅当 `entries` 存在重复 id；否则总是返回一个
/// `Ok` 提案——即使受保护条目本身超过预算（`over_budget > 0`），也绝不淘汰它们。
pub(crate) fn plan_evictions(
    entries: &[(u32, u64)],
    protected: &[u32],
    budget: usize,
) -> Result<EvictionPlan, PlanError> {
    // ① 唯一 id 是输入不变量：先整集合 fail-closed（输入是只读切片，不做任何修改）。
    for (index, &(id, _)) in entries.iter().enumerate() {
        if entries[..index]
            .iter()
            .any(|&(earlier_id, _)| earlier_id == id)
        {
            return Err(PlanError::DuplicateId);
        }
    }

    // ② 候选 = 未保护条目；只收集排序键，按 (tick, id) 升序（与输入排列无关）。
    let mut candidates: Vec<(u64, u32)> = entries
        .iter()
        .filter(|&&(id, _)| !protected.contains(&id))
        .map(|&(id, tick)| (tick, id))
        .collect();
    candidates.sort_unstable();

    // ③ 删最少前缀使 remaining <= budget；候选耗尽即停（受保护条目允许超额）。
    let mut evict = Vec::new();
    let mut remaining = entries.len();
    for &(_, id) in &candidates {
        if remaining <= budget {
            break;
        }
        evict.push(id);
        remaining -= 1;
    }

    // ④ 结果三元组；`remaining` 由 evict 长度冗余重算，防止两者漂移。
    let remaining = entries.len() - evict.len();
    let over_budget = remaining.saturating_sub(budget);
    Ok(EvictionPlan {
        evict,
        remaining,
        over_budget,
    })
}
