//! 掉落表：Crystal `Envir/Drops/*.txt` 的解析与掷骰。
//!
//! 怪物模板里只存 `MonsterInfo.DropPath`（文件名），真正的掉落行在文本文件里
//! （`Server/MirDatabase/MonsterInfo.cs:372-435` 的 `Load` / `FromLine` / `ParseGroup` /
//! `AttemptDrop`）。本模块把这些文本规则搬进 Rust，**不依赖**服务端类型，方便单测。
//!
//! # 文件格式（`MonsterInfo.cs:328-367`）
//!
//! ```text
//! ;注释与空行会被忽略
//! 1/100 物品名          ; 1/100 概率掉「物品名」
//! 1/50  Gold 1000       ; 掉金币，实际数量在 [Gold/2, Gold + Gold/2) 之间
//! 1/20  GROUP*          ; 成组掉落，* = 每组随机挑一条（^ = 只取第一条命中）
//! {
//!   1/1  物品A
//!   1/2  物品B
//! }
//! #INSERT [其它文件]     ; 把另一个文件的全部内容插进来（`ParseInsert`）
//! ```
//!
//! 概率写法是「1/N」：Crystal 直接砍掉前两个字符后解析成 N（`FromLine` 的
//! `parts[0].Substring(2)`），所以文件里必须保持 `1/` 前缀。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use mir_core::MirRng;

/// 一次掷骰的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DropReward {
    /// 掉落的物品名（可多个：成组掉落会累加）。
    pub items: Vec<String>,
    /// 与 [`DropReward::items`] 一一对应的任务物品标记（Crystal `DropInfo.QuestRequired`，
    /// 行尾 `Q`）：`true` 的物品只允许进任务系统，没人截胡时绝不落地
    /// （`MonsterObject.cs:1140`）。长度与 `items` 相同。
    pub quest_required: Vec<bool>,
    /// 掉落的金币。
    pub gold: u32,
}

impl DropReward {
    /// 是否什么都没掉。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.gold == 0
    }
}

/// 一条掉落规则。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropInfo {
    /// 概率分母：`1/chance`（Crystal `Chance`）。
    pub chance: u32,
    /// 掉落内容。
    pub kind: DropKind,
    /// 是否要求任务（行尾的 `Q`，`FromLine` 里的 `QuestRequired`）。
    pub quest_required: bool,
}

/// 掉落内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropKind {
    /// 金币，值为基准数（实际区间见 [`DropInfo::roll`]）。
    Gold(u32),
    /// 物品名。
    Item(String),
    /// 成组掉落。
    Group(GroupDrop),
}

/// 成组掉落（Crystal `GroupDropInfo`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupDrop {
    /// `GROUP*`：每组只保留随机一条（Crystal 里 `Random` 字段实际未被 `AttemptDrop` 使用，
    /// 但解析仍要认这个标记）。
    pub random: bool,
    /// `GROUP^`：命中第一条就停止。
    pub first: bool,
    /// 组内规则。
    pub children: Vec<DropInfo>,
}

/// 一个掉落表（对应一个 `.txt` 文件）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DropTable {
    /// 规则列表（保持文件顺序）。
    pub drops: Vec<DropInfo>,
    /// 解析失败的行（原文保留，方便日志/`mir-tools` 排查）。
    pub bad_lines: Vec<String>,
}

impl DropTable {
    /// 解析文本。
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut lines: Vec<String> = text.lines().map(std::borrow::ToOwned::to_owned).collect();
        // `#INSERT` 由 `load_with_dir` 处理；直接 parse 时把它们当无效行收集。
        let mut table = Self::default();
        let mut index = 0;
        while index < lines.len() {
            let line = lines[index].trim().to_owned();
            lines[index].clear();
            index += 1;
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if line.to_ascii_uppercase().starts_with("#INSERT") {
                table.bad_lines.push(line);
                continue;
            }
            match Self::parse_line(&line) {
                Some(mut drop) => {
                    if let DropKind::Group(group) = &mut drop.kind {
                        group.children =
                            Self::parse_group(&mut lines, &mut index, &mut table.bad_lines);
                    }
                    table.drops.push(drop);
                }
                None => table.bad_lines.push(line),
            }
        }
        table
    }

    /// 解析组体（`MonsterInfo.cs:437-487` 的 `ParseGroup`）。
    ///
    /// 组体在 `{` 与 `}` 之间；嵌套的 `GROUP` 也支持（Crystal 递归解析）。
    fn parse_group(
        lines: &mut [String],
        index: &mut usize,
        bad_lines: &mut Vec<String>,
    ) -> Vec<DropInfo> {
        let mut children = Vec::new();
        let mut started = false;
        let mut finished = false;
        let mut cursor = *index;
        while cursor < lines.len() {
            let line = lines[cursor].trim().to_owned();
            lines[cursor].clear();
            cursor += 1;
            if line == "{" {
                started = true;
                continue;
            }
            if line == "}" {
                finished = true;
                break;
            }
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            match Self::parse_line(&line) {
                Some(mut drop) => {
                    if let DropKind::Group(group) = &mut drop.kind {
                        group.children = Self::parse_group(lines, &mut cursor, bad_lines);
                    }
                    children.push(drop);
                }
                None => bad_lines.push(line),
            }
        }
        *index = cursor;
        if !(started && finished) {
            // Crystal 也只在 `start && finish` 时才把子规则挂到父节点（`:483-486`）。
            bad_lines.push("分组缺少 { 或 }".to_owned());
            return Vec::new();
        }
        children
    }

    /// 解析一行（`MonsterInfo.cs:328-367` 的 `FromLine`）。
    #[must_use]
    pub fn parse_line(line: &str) -> Option<DropInfo> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            return None;
        }
        // Crystal：`int.TryParse(parts[0].Substring(2), out Chance)`——前两个字符是 `1/`。
        let token = parts[0];
        if token.len() <= 2 {
            return None;
        }
        let chance: u32 = token.get(2..)?.parse().ok()?;
        if chance == 0 {
            return None;
        }
        let mut quest_required = false;
        let kind = if parts[1].eq_ignore_ascii_case("gold") {
            let gold: u32 = parts.get(2)?.parse().ok()?;
            if gold == 0 {
                return None;
            }
            DropKind::Gold(gold)
        } else if parts[1].to_ascii_uppercase().starts_with("GROUP") {
            DropKind::Group(GroupDrop {
                random: parts[1].ends_with('*'),
                first: parts[1].ends_with('^'),
                children: Vec::new(),
            })
        } else {
            if parts.len() > 2 && parts[2].eq_ignore_ascii_case("q") {
                quest_required = true;
            }
            DropKind::Item(parts[1].to_owned())
        };
        Some(DropInfo {
            chance,
            kind,
            quest_required,
        })
    }

    /// 载入文件，并展开 `#INSERT [...]`。
    ///
    /// `drop_dir` 是 `Envir/Drops/`，`#INSERT` 里的相对路径都相对于它。
    pub fn load(path: impl AsRef<Path>, drop_dir: impl AsRef<Path>) -> std::io::Result<Self> {
        let mut visited = HashSet::new();
        Self::load_inner(path.as_ref(), drop_dir.as_ref(), &mut visited)
    }

    fn load_inner(
        path: &Path,
        drop_dir: &Path,
        visited: &mut HashSet<PathBuf>,
    ) -> std::io::Result<Self> {
        if !visited.insert(path.to_path_buf()) {
            // 循环包含：直接返回空表，避免无限递归。
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)?;
        let mut table = Self::parse(&text);
        // 处理 `#INSERT [x]`：Crystal 把被包含文件的行**追加**到末尾再解析（`:437-460`）。
        let mut inserts: Vec<String> = Vec::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed
                .strip_prefix("#INSERT")
                .or_else(|| trimmed.strip_prefix("#insert"))
            {
                if let Some(name) = rest
                    .trim()
                    .strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                {
                    inserts.push(name.trim().to_owned());
                }
            }
        }
        for name in inserts {
            let child = drop_dir.join(&name);
            let Ok(child_table) = Self::load_inner(&child, drop_dir, visited) else {
                continue;
            };
            table.drops.extend(child_table.drops);
            table.bad_lines.extend(child_table.bad_lines);
        }
        Ok(table)
    }

    /// 掉率倍率（Crystal `Settings.DropRate`，默认 1）。
    ///
    /// 这里做成参数而不是常量，方便以后从配置读。
    #[must_use]
    pub fn roll(&self, rng: &mut MirRng, rate: u32) -> DropReward {
        let mut reward = DropReward::default();
        for drop in &self.drops {
            if let Some(part) = drop.roll(rng, rate, 0, 0) {
                reward.gold = reward.gold.saturating_add(part.gold);
                reward.items.extend(part.items);
                reward.quest_required.extend(part.quest_required);
            }
        }
        reward
    }
}

impl DropInfo {
    /// 是否为「必掉」规则（`1/1`，Crystal 每次 `AttemptDrop` 都先过 `Random.Next(1)` 恒 0）。
    #[must_use]
    pub const fn is_guaranteed(&self) -> bool {
        self.chance == 1
    }

    /// 单条规则的掷骰（`MonsterInfo.cs:489-560` 的 `AttemptDrop`）。
    ///
    /// `item_offset` / `gold_offset` 对应 Crystal 的「掉率百分比偏移」（会员等加成）。
    #[must_use]
    pub fn roll(
        &self,
        rng: &mut MirRng,
        rate: u32,
        item_offset: i32,
        gold_offset: i32,
    ) -> Option<DropReward> {
        let mut chance = (i64::from(self.chance) / i64::from(rate.max(1))) as i32;
        if item_offset > 0 {
            chance -= (chance * item_offset) / 100;
        }
        if chance < 1 {
            chance = 1;
        }
        // `Envir.Random.Next(rate) != 0`（Crystal 左闭右开）：`next_bounded(n)` 返回
        // `[0, n)`，与 C# 的 `Random.Next(n)` 完全同语义。
        if rng.next_bounded(chance as u32) != 0 {
            return None;
        }

        let mut reward = DropReward::default();
        match &self.kind {
            DropKind::Gold(gold) => {
                let lower = (*gold / 2) as i32;
                let mut upper = (*gold + *gold / 2) as i32;
                let mut lower = lower;
                if gold_offset > 0 {
                    lower += (lower * gold_offset) / 100;
                }
                if lower > upper {
                    lower = upper;
                }
                if upper <= lower {
                    upper = lower + 1;
                }
                // Crystal `Envir.Random.Next(lower, upper)` 是左闭右开：`[lower, upper)`。
                // 用 `next_bounded(span)` 显式表达，避免闭区间 API 的 off-by-one 歧义。
                let span = (upper - lower) as u32;
                let rolled = lower + rng.next_bounded(span) as i32;
                reward.gold = rolled.max(0) as u32;
            }
            DropKind::Item(name) => {
                reward.items.push(name.clone());
                reward.quest_required.push(self.quest_required);
            }
            DropKind::Group(group) => {
                let mut items = Vec::new();
                let mut flags = Vec::new();
                for child in &group.children {
                    if let Some(part) = child.roll(rng, rate, item_offset, gold_offset) {
                        reward.gold = reward.gold.saturating_add(part.gold);
                        items.extend(part.items);
                        flags.extend(part.quest_required);
                        if group.first {
                            break;
                        }
                    }
                }
                reward.items = items;
                reward.quest_required = flags;
            }
        }
        Some(reward)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rng() -> MirRng {
        MirRng::from_seed(0x5EED)
    }

    #[test]
    fn parses_item_gold_and_quest_lines() {
        let table = DropTable::parse(
            r"
            ;注释
            1/100 金创药(小量)
            1/50  Gold 1000
            1/20 任务物品 Q
            ",
        );
        assert_eq!(table.drops.len(), 3);
        assert!(table.bad_lines.is_empty(), "{:?}", table.bad_lines);
        assert_eq!(table.drops[0].chance, 100);
        assert_eq!(
            table.drops[0].kind,
            DropKind::Item("金创药(小量)".to_owned())
        );
        assert_eq!(table.drops[1].kind, DropKind::Gold(1000));
        assert!(table.drops[2].quest_required);
    }

    #[test]
    fn parses_group_blocks() {
        let table = DropTable::parse(
            r"
            1/10 GROUP^
            {
                1/1 物品A
                1/2 物品B
            }
            ",
        );
        assert_eq!(table.drops.len(), 1, "{:?}", table.bad_lines);
        let DropKind::Group(group) = &table.drops[0].kind else {
            panic!("应为成组掉落");
        };
        assert!(group.first);
        assert!(!group.random);
        assert_eq!(group.children.len(), 2);
    }

    #[test]
    fn rejects_malformed_lines() {
        let table = DropTable::parse(
            r"
            1/0 零概率
            1/10
            abc 物品
            1/10 Gold 0
            ",
        );
        assert!(table.drops.is_empty());
        assert_eq!(table.bad_lines.len(), 4);
    }

    #[test]
    fn guaranteed_drop_always_rolls() {
        let table = DropTable::parse("1/1 必掉物品\n");
        let mut rng = rng();
        let reward = table.roll(&mut rng, 1);
        assert_eq!(reward.items, vec!["必掉物品".to_owned()]);
    }

    #[test]
    fn gold_range_is_half_to_one_and_a_half() {
        let table = DropTable::parse("1/1 Gold 100\n");
        let mut rng = rng();
        // Crystal：lower = Gold/2 = 50，upper = Gold + Gold/2 = 150，取 [50, 149]。
        for _ in 0..50 {
            let reward = table.roll(&mut rng, 1);
            assert!((50..150).contains(&reward.gold), "金币 {}", reward.gold);
        }
    }

    #[test]
    fn group_first_stops_after_first_hit() {
        let table = DropTable::parse(
            r"
            1/1 GROUP^
            {
                1/1 A
                1/1 B
            }
            ",
        );
        let mut rng = rng();
        let reward = table.roll(&mut rng, 1);
        assert_eq!(reward.items, vec!["A".to_owned()], "^ 命中第一条就停");
    }

    #[test]
    fn rate_scales_the_denominator() {
        // 掉落率 10 倍 → 1/100 变成 1/10，多抽几次总该中。
        let table = DropTable::parse("1/100 稀有物品\n");
        let mut rng = rng();
        let mut hits = 0;
        for _ in 0..500 {
            if !table.roll(&mut rng, 10).items.is_empty() {
                hits += 1;
            }
        }
        assert!(hits > 20, "十倍掉率下 500 次至少中几次，实际 {hits}");
    }

    #[test]
    fn loads_a_real_drop_file_from_the_index() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resource/community");
        let candidate = root.join("clgforever910-mir2database/Daneo1989/Envir/Drops");
        if !candidate.is_dir() {
            eprintln!("跳过：resource/ 里没有 Drops 目录");
            return;
        }
        let Some(path) = std::fs::read_dir(&candidate)
            .expect("读取目录")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
                    && path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .is_some_and(|stem| !stem.starts_with('0'))
            })
        else {
            eprintln!("跳过：Drops 目录里没有普通掉落文件");
            return;
        };
        let table = DropTable::load(&path, &candidate).expect("解析真实掉落文件");
        assert!(!table.drops.is_empty(), "{} 应该解析出规则", path.display());
        assert!(
            table.bad_lines.is_empty(),
            "{} 有无法解析的行：{:?}",
            path.display(),
            table.bad_lines
        );
    }

    #[test]
    fn roll_keeps_quest_required_flag_aligned_with_items() {
        let table = DropTable::parse("1/1 QuestItem Q\n1/1 NormalItem\n");
        assert_eq!(table.drops.len(), 2);
        let reward = table.roll(&mut rng(), 1);
        assert_eq!(reward.items, vec!["QuestItem", "NormalItem"]);
        assert_eq!(
            reward.quest_required,
            vec![true, false],
            "`Q` 标记必须与 items 一一对应（Crystal `DropInfo.QuestRequired`）"
        );
    }

    #[test]
    fn group_children_are_independent_probabilities() {
        // Crystal `AttemptDrop` 对组内每一条**各自**掷 `Random.Next(Chance)`，
        // 不是「每 group 只掉一个」，也不是先归一化权重；`GROUP*` 的 `Random` 字段
        // Crystal 实际没使用（`MonsterInfo.cs:547-566`）。
        let table = DropTable::parse(
            r"
            1/1 GROUP*
            {
                1/2 A
                1/2 B
            }
            ",
        );
        let mut rng = MirRng::from_seed(0x00C0_FFEE);
        let (mut only_a, mut only_b, mut both, mut neither) = (0, 0, 0, 0);
        let rolls = 4_000;
        for _ in 0..rolls {
            let items = table.roll(&mut rng, 1).items;
            match (
                items.iter().any(|s| s == "A"),
                items.iter().any(|s| s == "B"),
            ) {
                (true, true) => both += 1,
                (true, false) => only_a += 1,
                (false, true) => only_b += 1,
                (false, false) => neither += 1,
            }
        }
        // 期望各 25%；二项分布 sd≈21.7，±5% 区间非常宽松且确定（固定种子）。
        for (label, count) in [
            ("A only", only_a),
            ("B only", only_b),
            ("both", both),
            ("neither", neither),
        ] {
            assert!(
                (800..=1_200).contains(&count),
                "{label} 应约为 1000/4000，实际 {count}"
            );
        }
    }

    #[test]
    fn guaranteed_rule_and_rate_use_right_open_rng() {
        let guaranteed = DropInfo {
            chance: 1,
            kind: DropKind::Item("必掉".to_owned()),
            quest_required: false,
        };
        assert!(guaranteed.is_guaranteed());
        let mut rng = MirRng::from_seed(0xDEAD_BEEF);
        for _ in 0..100 {
            assert!(guaranteed.roll(&mut rng, 1, 0, 0).is_some());
        }

        // 1/4 规则 4000 次应命中约 1000 次（左闭右开 0..4 中恰有 1/4 为 0）。
        let table = DropTable::parse("1/4 四分之一\n");
        let mut rng = MirRng::from_seed(0x1234_5678);
        let hits = (0..4_000)
            .filter(|_| !table.roll(&mut rng, 1).items.is_empty())
            .count();
        assert!(
            (850..=1_150).contains(&hits),
            "4000 次 1/4 应约 1000 次，实际 {hits}"
        );
    }
}
