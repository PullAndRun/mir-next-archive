//! 诊断工具：打印全部任务的等级/前置门槛及 Hen/Deer 经验（只读 Server.MirDB）。
//!
//! 用法：`cargo run --release -p mir-server --example quest_levels`
//! 用于确认新手链起点（例如 `quest 7` 的 `MinLevel=4`、`quest 3` 的 `required_quest=2`）。

fn main() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../resource/community/suprcode-crystal-database/Jev/Server.MirDB");
    let db = mir_db::MirDb::load(&path).expect("load Server.MirDB");
    for quest in &db.quests {
        println!(
            "{} | {} | min={} max={} required_quest={} class={} type={}",
            quest.index,
            quest.name,
            quest.required_min_level,
            quest.required_max_level,
            quest.required_quest,
            quest.required_class,
            quest.quest_type
        );
    }
    for name in ["Hen", "Deer"] {
        if let Some(monster) = db.monsters.iter().find(|monster| monster.name == name) {
            println!(
                "monster {} exp={} hp={:?}",
                name,
                monster.experience,
                monster.stats.get(mir_core::enums::Stat::HP)
            );
        }
    }
}
