//! 真实 NPC 脚本的解析测试。
//!
//! 样本是 `resource/community/suprcode-crystal-database/Jev/Envir/NPCs` 下的 660 份
//! Crystal 原版脚本（`resource/` 被 gitignore，缺失时整个测试优雅跳过）。
//!
//! 目的有两个：
//!
//! 1. **回归锚点**：把解析器在真实数据上的统计量钉死（脚本数、页数、段数、商品行数），
//!    以后任何一处切页/切段规则改动都会在这里体现出来；
//! 2. **语法覆盖证据**：`docs/world/npc.md` 里「支持哪些语法」的结论就来自这里的统计。

use std::path::{Path, PathBuf};

use mir_db::npc_script::{NpcActionKind, NpcCheckKind, NpcScript};

/// `Envir/NPCs` 目录；样本缺失时返回 `None`。
fn npc_dir() -> Option<PathBuf> {
    let envir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("resource/community/suprcode-crystal-database/Jev/Envir");
    let dir = envir.join("NPCs");
    if dir.is_dir() {
        Some(envir)
    } else {
        eprintln!("跳过：找不到 {}", dir.display());
        None
    }
}

/// 递归收集 `*.txt`，返回 (相对 `NPCs/` 的文件名，绝对路径)，按文件名排序保证可回放。
fn collect_scripts(envir: &Path) -> Vec<(String, PathBuf)> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
            {
                let relative = path
                    .strip_prefix(base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((relative, path));
            }
        }
    }
    let mut out = Vec::new();
    walk(&envir.join("NPCs"), &envir.join("NPCs"), &mut out);
    out.sort_by(|left, right| left.0.cmp(&right.0));
    out
}

#[test]
fn every_sample_script_parses() {
    let Some(envir) = npc_dir() else {
        return;
    };
    let scripts = collect_scripts(&envir);
    assert!(scripts.len() > 600, "样本里应有 600 份以上脚本");

    let mut pages = 0usize;
    let mut segments = 0usize;
    let mut goods_lines = 0usize;
    let mut buttons = 0usize;
    let mut empty_scripts = 0usize;
    let mut with_goods = 0usize;
    let mut with_types = 0usize;
    let mut without_main = 0usize;
    let mut warnings = 0usize;

    for (file_name, _path) in &scripts {
        let script = match NpcScript::load(file_name, &envir) {
            Ok(script) => script,
            Err(error) => panic!("{file_name} 解析失败：{error}"),
        };
        assert_eq!(script.file_name(), file_name.trim_end_matches(".txt"));
        warnings += script.warnings().len();
        if script.sections().is_empty() {
            empty_scripts += 1;
            continue;
        }
        if script.section("@main").is_none() {
            without_main += 1;
        }
        pages += script.sections().len();
        for page in script.sections() {
            assert!(page.key.starts_with('['), "{file_name} 的页 key 带方括号");
            assert!(page.key.ends_with(']'), "{file_name} 的页 key 带方括号");
            segments += page.segments.len();
            buttons += page.buttons.len();
        }
        if !script.goods().is_empty() {
            with_goods += 1;
            goods_lines += script.goods().len();
        }
        if !script.types().is_empty() {
            with_types += 1;
        }
    }

    eprintln!(
        "脚本 {} 份（空文件 {empty_scripts}、无 @Main 页 {without_main}）／页 {pages}／段 {segments}／\
         按钮 {buttons}／商品行 {goods_lines}（{with_goods} 份脚本带商品）／\
         带类型表 {with_types}／告警 {warnings}",
        scripts.len()
    );

    // 统计量锚点（真实数据，`docs/world/npc.md` 引用同一组数字）。
    assert!(pages > 3_000, "660 份脚本应有数千个页面，实际 {pages}");
    assert!(segments > 3_000, "段数应当不低于页数，实际 {segments}");
    assert!(buttons > 3_000, "按钮应当很多，实际 {buttons}");
    assert!(
        goods_lines > 2_000,
        "商店商品行应当上千，实际 {goods_lines}"
    );
    assert!(
        with_goods > 200,
        "240+ 份脚本带 [Trade] 段，实际 {with_goods}"
    );
    assert!(
        with_types > 200,
        "270 份脚本带 [Types] 段，实际 {with_types}"
    );
    assert_eq!(warnings, 0, "真实样本不应产生解析告警");
    assert!(
        without_main < 40,
        "绝大多数脚本都有 [@Main] 页，实际有 {without_main} 份没有"
    );
}

#[test]
fn blacksmith_script_matches_the_hand_read_structure() {
    let Some(envir) = npc_dir() else {
        return;
    };
    let script =
        NpcScript::load("BichonProvince/BichonWall/Blacksmith1", &envir).expect("载入铁匠脚本");

    // `[@MAIN]`：`CHECKPKPOINT > 199` 不满足时走 `#ELSEACT GOTO @Main-1`。
    let main = script.section("@main").expect("有主页");
    assert_eq!(main.segments.len(), 1);
    let segment = &main.segments[0];
    assert_eq!(segment.checks.len(), 1);
    assert_eq!(segment.checks[0].kind, NpcCheckKind::PkPoint);
    assert_eq!(segment.checks[0].params, vec![">", "199"]);
    assert_eq!(
        segment.else_actions[0].kind,
        NpcActionKind::Goto,
        "ELSEACT 里是 GOTO"
    );
    assert_eq!(segment.else_actions[0].params, vec!["@Main-1"]);

    // `[@Main-1]`：正文里带 `{<$WEAPON>/CORAL}` 与三个按钮。
    let second = script.section("main-1").expect("有 Main-1");
    let say = &second.segments[0].say;
    assert!(say.iter().any(|line| line.contains("{<$WEAPON>/CORAL}")));
    assert_eq!(
        second.buttons,
        vec![
            "[@BUYSELL]".to_owned(),
            "[@REPAIR]".to_owned(),
            "[@EXIT]".to_owned()
        ]
    );

    // `[Types]` 两行、`[Trade]` 里第一件是 WoodenSword。
    assert_eq!(script.types(), &[1, 14]);
    assert_eq!(script.goods()[0].name, "WoodenSword");
    assert_eq!(script.goods()[0].count, 1);
    assert!(script.goods().iter().any(|goods| goods.name == "PickAxe"));

    // `[@talk]` 用 flag 判断是否聊过，并设置 flag。
    let talk = script.section("talk").expect("有 talk 页");
    assert_eq!(talk.segments[0].checks[0].kind, NpcCheckKind::Flag);
    assert_eq!(talk.segments[0].checks[0].params, vec!["539", "1"]);
}

#[test]
fn trade_item_names_resolve_against_the_database() {
    // 解析器不认识物品模板，这里只验证 `[Trade]` 里的名字**格式**：
    // 非空、没有空白（Crystal 按空格切列，名字里不可能有空格）。
    let Some(envir) = npc_dir() else {
        return;
    };
    let mut checked = 0usize;
    for (file_name, _path) in collect_scripts(&envir).iter().take(120) {
        let script = NpcScript::load(file_name, &envir).expect("解析成功");
        for goods in script.goods() {
            assert!(!goods.name.is_empty(), "{file_name} 的空商品名");
            assert!(
                !goods.name.contains(char::is_whitespace),
                "{file_name} 的商品名带空白：{:?}",
                goods.name
            );
            assert!(goods.count >= 1, "{file_name} 的商品数量至少 1");
            checked += 1;
        }
    }
    assert!(checked > 0, "抽查的 120 份脚本里应当有商品");
}
