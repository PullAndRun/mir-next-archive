//! 稳定的文本报告：`mir-tools mirdb-report` 与 `mir-db` 二进制共用。
//!
//! 输出格式刻意做成**逐行 `key=value`**，供测试与脚本直接比对：
//!
//! ```text
//! version=105
//! custom_version=0
//! map_index=389
//! ...
//! section maps count=389 start=40 end=251090
//! ...
//! leftover=0
//! map[0] index=1 file_name="0" title="比奇省" safe_zones=5 respawns=358 movements=70
//! ...
//! ```
//!
//! 每段最多列 [`SAMPLE_ROWS`] 条样本；`dragon` 是单条记录，直接一行。字符串一律加双引号
//! （内部 `"` 转义成 `\"`），空字段也照写（`""`），这样列数与键名永远稳定。

use std::fmt::Write as _;

use mir_core::enums::Stat;

use crate::MirDb;

/// 每段最多打印的样本条数。
pub const SAMPLE_ROWS: usize = 3;

/// 把 `value` 包成带转义的双引号字符串。
fn quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        if ch == '"' || ch == '\\' {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

/// 渲染整份报告（末尾带换行）。
///
/// `leftover` 由调用方从 [`MirDb::parse_with_leftover`] 取得；非 0 时报告里会多一行
/// `warning=...`，方便一眼看出文件与解析器版本不符。
#[must_use]
pub fn render_report(db: &MirDb, leftover: usize) -> String {
    let header = &db.header;
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("version={}", header.version));
    lines.push(format!("custom_version={}", header.custom_version));
    lines.push(format!("map_index={}", header.map_index));
    lines.push(format!("item_index={}", header.item_index));
    lines.push(format!("monster_index={}", header.monster_index));
    lines.push(format!("npc_index={}", header.npc_index));
    lines.push(format!("quest_index={}", header.quest_index));
    lines.push(format!(
        "gameshop_index={}",
        optional(header.gameshop_index)
    ));
    lines.push(format!(
        "conquest_index={}",
        optional(header.conquest_index)
    ));
    lines.push(format!("respawn_index={}", optional(header.respawn_index)));

    for span in &db.sections {
        lines.push(format!(
            "section {} count={} start={} end={}",
            span.name, span.count, span.range.start, span.range.end
        ));
    }

    // 各段条数汇总（与 `sections` 互补：这里固定列出全部段，缺失的段写 0）。
    lines.push(format!("count maps={}", db.maps.len()));
    lines.push(format!("count items={}", db.items.len()));
    lines.push(format!("count monsters={}", db.monsters.len()));
    lines.push(format!("count npcs={}", db.npcs.len()));
    lines.push(format!("count quests={}", db.quests.len()));
    lines.push(format!("count magics={}", db.magics.len()));
    lines.push(format!("count gameshop={}", db.gameshop.len()));
    lines.push(format!("count conquests={}", db.conquests.len()));
    lines.push(format!(
        "count respawn_timers={}",
        usize::from(db.respawn.is_some())
    ));
    lines.push(format!(
        "count gt_maps={}",
        db.gt_maps.as_ref().map_or(0, Vec::len)
    ));

    lines.push(format!("leftover={leftover}"));
    if leftover != 0 {
        lines.push(format!(
            "warning=文件尾还有 {leftover} 字节未被消费，文件版本或布局可能与本解析器不符"
        ));
    }

    for (index, map) in db.maps.iter().take(SAMPLE_ROWS).enumerate() {
        lines.push(format!(
            "map[{index}] index={} file_name={} title={} mini_map={} light={} big_map={} safe_zones={} respawns={} movements={} mine_zones={}",
            map.index,
            quoted(&map.file_name),
            quoted(&map.title),
            map.mini_map,
            map.light,
            map.big_map,
            map.safe_zones.len(),
            map.respawns.len(),
            map.movements.len(),
            map.mine_zones.len(),
        ));
    }

    for (index, item) in db.items.iter().take(SAMPLE_ROWS).enumerate() {
        lines.push(format!(
            "item[{index}] index={} name={} type={} grade={} price={} stack_size={} stats={}",
            item.index,
            quoted(&item.name),
            item.item_type.value(),
            item.grade.value(),
            item.price,
            item.stack_size,
            item.stats.len(),
        ));
    }

    for (index, monster) in db.monsters.iter().take(SAMPLE_ROWS).enumerate() {
        lines.push(format!(
            "monster[{index}] index={} name={} ai={} level={} hp={} image={} experience={}",
            monster.index,
            quoted(&monster.name),
            monster.ai,
            monster.level,
            monster.stat(Stat::HP),
            monster.image,
            monster.experience,
        ));
    }

    for (index, npc) in db.npcs.iter().take(SAMPLE_ROWS).enumerate() {
        lines.push(format!(
            "npc[{index}] index={} map_index={} file_name={} name={} x={} y={} image={} rate={}",
            npc.index,
            npc.map_index,
            quoted(&npc.file_name),
            quoted(&npc.name),
            npc.location.x,
            npc.location.y,
            npc.image,
            npc.rate,
        ));
    }

    for (index, quest) in db.quests.iter().take(SAMPLE_ROWS).enumerate() {
        lines.push(format!(
            "quest[{index}] index={} name={} group={} file_name={} required_min_level={} required_max_level={} required_class={}",
            quest.index,
            quoted(&quest.name),
            quoted(&quest.group),
            quoted(&quest.file_name),
            quest.required_min_level,
            quest.effective_required_max_level(),
            quest.required_class,
        ));
    }

    {
        let dragon = &db.dragon;
        let mut exps = String::new();
        for (index, exp) in dragon.exps.iter().enumerate() {
            if index > 0 {
                exps.push(',');
            }
            // 写进 String 不会失败。
            let _ = write!(exps, "{exp}");
        }
        lines.push(format!(
            "dragon enabled={} map_file_name={} monster_name={} body_name={} x={} y={} exps=[{}]",
            dragon.enabled,
            quoted(&dragon.map_file_name),
            quoted(&dragon.monster_name),
            quoted(&dragon.body_name),
            dragon.location.x,
            dragon.location.y,
            exps,
        ));
    }

    for (index, magic) in db.magics.iter().take(SAMPLE_ROWS).enumerate() {
        lines.push(format!(
            "magic[{index}] name={} spell={} icon={} level1={} level2={} level3={} delay_base={} range={} multiplier_base={}",
            quoted(&magic.name),
            magic.spell,
            magic.icon,
            magic.level1,
            magic.level2,
            magic.level3,
            magic.delay_base,
            magic.range,
            magic.multiplier_base,
        ));
    }

    for (index, shop) in db.gameshop.iter().take(SAMPLE_ROWS).enumerate() {
        lines.push(format!(
            "gameshop[{index}] item_index={} g_index={} gold_price={} credit_price={} count={} class={} category={}",
            shop.item_index,
            shop.g_index,
            shop.gold_price,
            shop.credit_price,
            shop.count,
            quoted(&shop.class),
            quoted(&shop.category),
        ));
    }

    for (index, conquest) in db.conquests.iter().take(SAMPLE_ROWS).enumerate() {
        lines.push(format!(
            "conquest[{index}] index={} name={} map_index={} palace_index={} size={} war_length={}",
            conquest.index,
            quoted(&conquest.name),
            conquest.map_index,
            conquest.palace_index,
            conquest.size,
            conquest.war_length,
        ));
    }

    if let Some(respawn) = &db.respawn {
        lines.push(format!(
            "respawn_timer base_spawn_rate={} current_tick_counter={} options={}",
            respawn.base_spawn_rate,
            respawn.current_tick_counter,
            respawn.options.len(),
        ));
    }

    if let Some(gt_maps) = &db.gt_maps {
        for (index, map) in gt_maps.iter().take(SAMPLE_ROWS).enumerate() {
            lines.push(format!(
                "gt_map[{index}] index={} name={} owner={} leader={} price={} days={}",
                map.index,
                quoted(&map.name),
                quoted(&map.owner),
                quoted(&map.leader),
                map.price,
                map.days,
            ));
        }
    }

    lines.push(String::new());
    lines.join("\n")
}

/// 可选计数器的显示：缺失时写 `none`，保证行结构稳定。
fn optional(value: Option<i32>) -> String {
    value.map_or_else(|| "none".to_owned(), |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_of_empty_db_is_stable() {
        let db = MirDb::default();
        let report = render_report(&db, 0);
        assert!(report.contains("version=0\n"));
        assert!(report.contains("gameshop_index=none\n"));
        assert!(report.contains("count maps=0\n"));
        assert!(report.contains("leftover=0\n"));
        assert!(report.ends_with('\n'));
        assert!(!report.contains("warning="));
    }

    #[test]
    fn leftover_warning_is_printed() {
        let report = render_report(&MirDb::default(), 7);
        assert!(report.contains("leftover=7\n"));
        assert!(report.contains("warning="), "非 0 leftover 应当有醒目提示");
    }

    #[test]
    fn quoted_escapes_quotes_and_backslashes() {
        assert_eq!(quoted("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(quoted(""), "\"\"");
        assert_eq!(quoted("比奇省"), "\"比奇省\"");
    }
}
