//! AI 覆盖率巡检：按真实 `Server.MirDB` 的 `MonsterInfo.AI` 分组，输出
//! 「AI 编号 → 怪物名 → 本作实现状态」以及按怪物模板数计算的覆盖率。
//!
//! 用法：
//! ```text
//! cargo run -p mir-server --example ai_audit
//! ```
//!
//! 状态由 [`mir_server::monster::MonsterKind::ai_support`] 给出：`已实现` 表示有
//! 专属行为，`部分` 表示只有基类索敌/近战/漫游，`未实现` 表示没有任何分支。

use std::collections::BTreeMap;

use mir_server::gamedata::GameData;
use mir_server::monster::{AiSupport, MonsterKind};

fn main() {
    let missing_only = std::env::args().any(|arg| arg == "--missing");
    let data = match GameData::discover() {
        Ok(data) => data,
        Err(error) => {
            eprintln!("加载真实 Server.MirDB 失败：{error}");
            std::process::exit(1);
        }
    };

    let mut groups: BTreeMap<u8, Vec<MonsterKind>> = BTreeMap::new();
    for index in 0..data.monster_count() {
        let Ok(index) = i32::try_from(index) else {
            continue;
        };
        if let Some(kind) = data.monster(index) {
            groups.entry(kind.ai).or_default().push(kind.clone());
        }
    }

    println!("AI  实现状态  怪物数  怪物名");
    println!("--  ---------  ------  ----------------------------------------");
    let mut covered = 0usize;
    let mut total = 0usize;
    for (ai, kinds) in &groups {
        let status = kinds
            .first()
            .map_or(AiSupport::Stub, MonsterKind::ai_support);
        if status != AiSupport::Stub {
            covered += kinds.len();
        }
        total += kinds.len();
        if missing_only && status != AiSupport::Stub {
            continue;
        }
        let mut names: Vec<&str> = kinds.iter().map(|kind| kind.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        println!(
            "{ai:<3} {:<10} {:<7} {}",
            status.label(),
            kinds.len(),
            names.join(", ")
        );
    }
    println!();
    println!(
        "已实现 AI 覆盖：{covered}/{total} 个怪物模板（{:.1}%）",
        if total == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let percent = covered as f64 * 100.0 / total as f64;
            percent
        }
    );
}
