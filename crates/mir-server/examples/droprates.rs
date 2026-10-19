//! 掉落率校验：固定种子跑 N 次该怪的掉落表，对比配置概率与实测概率。
//!
//! 用法：
//! ```text
//! cargo run -p mir-server --example droprates -- Hen 100000
//! ```
//!
//! 输出每行：`配置 1/50 (2.00%)` vs `实测 x/N (y.yy%)`，用于发现「配置说 10% 实测 1%」
//! 这类解析/倍率错误。只统计顶层规则；`GROUP` 规则会列出子项配置并统计整组命中率。

use std::collections::HashMap;
use std::env;

use mir_core::MirRng;
use mir_db::drops::DropKind;
use mir_server::gamedata::GameData;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(monster) = args.first() else {
        eprintln!("用法：droprates <怪物名> [次数，默认 100000]");
        std::process::exit(2);
    };
    let rolls: u32 = args
        .get(1)
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(100_000);

    let data = match GameData::discover() {
        Ok(data) => data,
        Err(error) => {
            eprintln!("加载真实数据失败：{error}");
            std::process::exit(1);
        }
    };
    let Some(table) = data.monster_drop_table(monster) else {
        eprintln!("怪物 {monster:?} 没有掉落表（名字需与 MonsterInfo.Name 一致）");
        std::process::exit(1);
    };

    let mut rng = MirRng::from_seed(0xD0_0D_1E);
    let mut item_hits: HashMap<String, u32> = HashMap::new();
    let mut gold_hits = 0_u32;
    for _ in 0..rolls {
        let reward = table.roll(&mut rng, 1);
        if reward.gold > 0 {
            gold_hits += 1;
        }
        for item in reward.items {
            *item_hits.entry(item).or_default() += 1;
        }
    }

    println!("怪物 {monster}：{rolls} 次，固定种子 0xD00D1E");
    for rule in &table.drops {
        let configured = 1.0 / f64::from(rule.chance.max(1));
        let (label, hits) = match &rule.kind {
            DropKind::Item(name) => (name.clone(), *item_hits.get(name).unwrap_or(&0)),
            DropKind::Gold(_) => ("Gold（任意金额）".to_owned(), gold_hits),
            DropKind::Group(group) => {
                let mut hits = 0_u32;
                for child in &group.children {
                    if let DropKind::Item(name) = &child.kind {
                        hits += *item_hits.get(name).unwrap_or(&0);
                    }
                }
                (format!("GROUP({} children)", group.children.len()), hits)
            }
        };
        let measured = f64::from(hits) / f64::from(rolls);
        println!(
            "  {label:<28} 配置 1/{:<6} ({:>6.2}%)  实测 {hits:>7}/{rolls} ({:>6.2}%)  {}",
            rule.chance,
            configured * 100.0,
            measured * 100.0,
            if rule.quest_required { "Q" } else { "" }
        );
    }
}
