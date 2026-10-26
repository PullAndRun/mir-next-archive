//! Round 66 A：多消费者「最低水位」参考模型（**仅 integration-test support**）。
//!
//! 唯一引用方是 `crates/mir-client/tests/round66_watermark_contract.rs` 的
//! `#[path = "support/round66_watermark.rs"] mod round66_watermark;`。本文件
//! **不链接生产**：`crates/mir-client/src/**` 不引用它，生产也没有消费者注册表、
//! 全局水位或裁剪接线（计划 round66-plan.md 48–54 行；PARTIAL）。
//!
//! 语义（计划 49–53 行）：
//!
//! * `generation` 是权威快照的会话代号；`consumers` 是测试显式假设的
//!   `(代号, 1-based 已消费绝对水位)` 集合，**不是**生产注册集合；
//! * 校验是**全集合分阶段**的固定优先级（结果与成员排列顺序无关）：界非法
//!   `reclaimed > total` → `InvalidBounds`；空成员 → `NoConsumers`；阶段 1 任一
//!   成员代号不匹配 → `GenerationMismatch`；阶段 2 任一水位 `< reclaimed` →
//!   `BelowReclaimed`；阶段 3 任一水位 `> total` → `BeyondTail`；全部通过才返回
//!   所有成员水位的**最小值**；
//! * 空成员 fail-closed，绝不默认取表尾；`watermark == reclaimed` 合法（成员刚好
//!   停在权威已裁边界）；`0` 在 `reclaimed == 0` 时是合法慢成员，会阻止任何裁剪；
//! * 重复成员不改变最小值。新加入者只能从 `>= reclaimed` 的显式水位开始；历史
//!   补发/追赶/注销/租约均不在本参考模型内。

/// 参考最低水位的拒绝原因。五个变体与校验顺序一一对应。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReferenceError {
    /// 成员集合为空：没有消费者可确认，fail-closed（不得默认取表尾）。
    NoConsumers,
    /// 成员代号与权威代号不一致：旧代/陌生代成员不能确认当前代事件。
    GenerationMismatch,
    /// 成员水位低于权威已回收边界：要求补发已裁历史（历史补发另案）。
    BelowReclaimed,
    /// 成员水位超过累计收到数：越尾，超出权威已知事件范围。
    BeyondTail,
    /// 界非法：`reclaimed > total`（快照自身不变量保证合法快照不会出现）。
    InvalidBounds,
}

/// 参考最低确认水位（1-based 绝对序号，含该条）：安全前缀裁剪最多只能到该值。
///
/// 返回 `Ok(watermark)` 当且仅当所有成员同代、水位落在 `[reclaimed, total]` 内；
/// 否则按「全集合分阶段」优先级返回唯一的拒绝原因，调用方必须 fail-closed、不得裁剪。
pub(crate) fn minimum_confirmed(
    generation: u64,
    reclaimed: u64,
    total: u64,
    consumers: &[(u64, u64)],
) -> Result<u64, ReferenceError> {
    if reclaimed > total {
        return Err(ReferenceError::InvalidBounds);
    }
    if consumers.is_empty() {
        return Err(ReferenceError::NoConsumers);
    }
    // 阶段 1：整集合先看代号——任一旧代/陌生代成员都优先于水位越界/越尾。
    if consumers
        .iter()
        .any(|&(consumer_generation, _)| consumer_generation != generation)
    {
        return Err(ReferenceError::GenerationMismatch);
    }
    // 阶段 2：整集合再看下界——任一低于已回收边界优先于任一越尾。
    if consumers
        .iter()
        .any(|&(_, watermark)| watermark < reclaimed)
    {
        return Err(ReferenceError::BelowReclaimed);
    }
    // 阶段 3：整集合最后看上界。
    if consumers.iter().any(|&(_, watermark)| watermark > total) {
        return Err(ReferenceError::BeyondTail);
    }
    // 全集合通过：取所有成员水位最小值（`total` 是合法上界，可作 fold 初值）。
    Ok(consumers
        .iter()
        .fold(total, |acc, &(_, watermark)| acc.min(watermark)))
}
