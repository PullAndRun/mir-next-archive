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
use mir_server::monster::{AI_BEHAVIOURS, AiSupport, MonsterKind};

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
    let (mut full_ais, mut partial_ais, mut missing_ais) = (0usize, 0usize, 0usize);
    let (mut full_templates, mut partial_templates, mut missing_templates) =
        (0usize, 0usize, 0usize);
    for (ai, kinds) in &groups {
        let status = kinds
            .first()
            .map_or(AiSupport::Stub, MonsterKind::ai_support);
        match status {
            AiSupport::Full => {
                full_ais += 1;
                full_templates += kinds.len();
            }
            AiSupport::Partial => {
                partial_ais += 1;
                partial_templates += kinds.len();
            }
            AiSupport::Stub => {
                missing_ais += 1;
                missing_templates += kinds.len();
            }
        }
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
    println!("AI 数：implemented={full_ais} partial={partial_ais} missing={missing_ais}");
    println!(
        "模板数：implemented={full_templates} partial={partial_templates} missing={missing_templates} total={total}"
    );
    #[allow(clippy::cast_precision_loss)]
    let all_percent = if total == 0 {
        0.0
    } else {
        covered as f64 * 100.0 / total as f64
    };
    #[allow(clippy::cast_precision_loss)]
    let full_ai_percent = {
        let ai_total = full_ais + missing_ais;
        if ai_total == 0 {
            0.0
        } else {
            full_ais as f64 * 100.0 / ai_total as f64
        }
    };
    println!(
        "覆盖率：全模板 {all_percent:.2}%（{covered}/{total}）；AI 号 Full {full_ai_percent:.2}%（{full_ais}/{}）",
        full_ais + missing_ais
    );
    let no_test = AI_BEHAVIOURS
        .iter()
        .filter(|entry| entry.test_name.trim().is_empty())
        .count();
    println!("Full 但无用例：{no_test}");
}
