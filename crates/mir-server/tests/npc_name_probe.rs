//! 临时探针（跑完即删）：扫描 resource 下全部 Server.MirDB，找带中文 NPC 名的数据集。

use std::path::Path;

use mir_db::MirDb;

fn has_cjk(text: &str) -> bool {
    text.chars()
        .any(|ch| ('\u{4E00}'..='\u{9FFF}').contains(&ch))
}

#[test]
fn probe_npc_name_language() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let mut dbs = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.file_name().is_some_and(|n| n == "Server.MirDB") {
                out.push(path);
            }
        }
    }
    walk(&root.join("resource"), &mut dbs);
    for path in dbs {
        let label = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .display()
            .to_string();
        match MirDb::load(&path) {
            Ok(db) => {
                let total = db.npcs.len();
                let cjk: Vec<&str> = db
                    .npcs
                    .iter()
                    .filter(|npc| has_cjk(&npc.name))
                    .take(8)
                    .map(|npc| npc.name.as_str())
                    .collect();
                let cjk_count = db.npcs.iter().filter(|npc| has_cjk(&npc.name)).count();
                let map0 = db
                    .maps
                    .iter()
                    .find(|m| m.file_name == "0")
                    .map(|m| m.title.as_str())
                    .unwrap_or("?");
                println!("{label}: npcs={total} cjk={cjk_count} map0={map0:?} sample={cjk:?}");
            }
            Err(error) => println!("{label}: load failed: {error}"),
        }
    }
}
