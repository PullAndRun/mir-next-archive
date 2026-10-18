//! 任务脚本（`Envir/Quests/<file>.txt`）解析：**纯数据**，不依赖服务端。
//!
//! 逐条对照 Crystal `Server/MirDatabase/QuestInfo.cs` 的文本解析部分：
//!
//! | 本模块 | Crystal | 说明 |
//! |---|---|---|
//! | [`QuestScript::load`] | `QuestInfo.LoadInfo`（`QuestInfo.cs:130-146`） | 读 `Envir/Quests/{FileName}.txt` |
//! | [`QuestScript::parse`] | `QuestInfo.ParseFile`（`:161-257`） | `[@SECTION]` 分段 |
//! | [`parse_reward_line`] | `QuestInfo.ParseReward`（`:259-282`） | `物品名 [数量]` |
//! | [`parse_kill_line`] / `parse_item_line` | `QuestInfo.ParseKill`（`:284-302`）/ `ParseItem`（`:304-326`） | `名字 [数量] ["消息"]` |
//! | [`parse_flag_line`] | `QuestInfo.ParseFlag`（`:328-348`） | `标记号 ["消息"]` |
//!
//! # 文件到底有哪些段
//!
//! 数据库里**只有头部**（`QuestInfo.cs:85-110`，见 [`mir_db::quest::QuestInfo`]），
//! 描述、目标、奖励全在这里。Crystal 认的段就是 `QuestInfo.cs:163-176` 的那 13 个常量：
//!
//! ```text
//! [@DESCRIPTION] [@TASKDESCRIPTION] [@RETURNDESCRIPTION] [@COMPLETION]
//! [@CARRYITEMS] [@KILLTASKS] [@ITEMTASKS] [@FLAGTASKS]
//! [@FIXEDREWARDS] [@SELECTREWARDS] [@EXPREWARD] [@GOLDREWARD] [@CREDITREWARD]
//! ```
//!
//! 注意：任务书里写的 `[@GOTO]` 段**在 Crystal 里并不存在**——`headers` 列表
//! （`QuestInfo.cs:178-183`）里没有它，「去某某处交任务」的文案是数据库头部的
//! `GotoMessage`（`QuestInfo.cs:42`），由 `QuestManager.UpdateGotoTask` 在任务完成后再追加一行
//! （`QuestProgressInfo.cs:527-532`）。本模块照实只认上面 13 个段，遇到 `[@GOTO]` 记一条告警。
//!
//! # 与 Crystal 的差异（都是「更宽容」，不影响正确数据）
//!
//! * **切列用 `split_whitespace`**：Crystal 用 `line.Split(' ')`（`QuestInfo.cs:263`、`:288`），
//!   行内连写两个空格时会切出空字段，`TryParse("")` 把数量写成 0。本作按空白切分，
//!   数量缺省 1；数字写错时记告警并按 Crystal 的 `TryParse` 语义取 0。
//! * **注释**：任务书要求「`#` / `;` 注释与空行忽略」。Crystal 只跳过空行（`:200`），
//!   `#`/`;` 行会被当成正文或数据行。本作把 `;`（行首、未缩进）与 `#`（行首）当注释丢掉。
//! * **未知段**：Crystal 直接忽略（`headers` 里没有的行永远不会被读到）。本作记一条告警，
//!   便于发现「新版本加了段而解析器没跟上」。
//! * **坏行**：Crystal 静默丢弃（`ParseItem` 返回 `null`，`:325`）。本作记录到
//!   [`QuestScript::warnings`] 并丢弃该行。
//! * **模板校验**：Crystal 解析时就拿 `Envir.GetMonsterInfo` / `GetItemInfo` 过滤不存在的
//!   怪物/物品（`:292`、`:312`、`:268`）。本 crate 没有模板表，所以只解析名字，
//!   校验交给 `mir-server` 的 `GameData`（见 `crates/mir-server/src/quests.rs`）。

use std::path::{Path, PathBuf};

/// 一段任务脚本（一份 `Envir/Quests/<file>.txt` 的解析结果）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestScript {
    /// 脚本文件名（不带目录与 `.txt`，即 `QuestInfo.file_name`）。
    pub file_name: String,
    /// `[@DESCRIPTION]`：任务简介。
    pub description: Vec<String>,
    /// `[@TASKDESCRIPTION]`：任务目标描述。
    pub task_description: Vec<String>,
    /// `[@RETURNDESCRIPTION]`：交任务时的描述（Crystal `ReturnDescription`，`QuestInfo.cs:56`）。
    pub return_description: Vec<String>,
    /// `[@COMPLETION]`：完成后的描述。
    pub completion_description: Vec<String>,
    /// `[@CARRYITEMS]`：接取时发给玩家的物品（`QuestItemTask`）。
    pub carry_items: Vec<QuestItemTask>,
    /// `[@KILLTASKS]`：击杀目标。
    pub kill_tasks: Vec<QuestKillTask>,
    /// `[@ITEMTASKS]`：收集目标。
    pub item_tasks: Vec<QuestItemTask>,
    /// `[@FLAGTASKS]`：标记位目标。
    pub flag_tasks: Vec<QuestFlagTask>,
    /// `[@FIXEDREWARDS]`：必定发放的物品。
    pub fixed_rewards: Vec<QuestRewardLine>,
    /// `[@SELECTREWARDS]`：任选其一的物品。
    pub select_rewards: Vec<QuestRewardLine>,
    /// `[@EXPREWARD]`：奖励经验。
    pub exp_reward: u32,
    /// `[@GOLDREWARD]`：奖励金币。
    pub gold_reward: u32,
    /// `[@CREDITREWARD]`：奖励元宝。
    pub credit_reward: u32,
    /// 解析告警（坏行、未知段、数字解析失败）。不参与逻辑，只给日志与测试看。
    pub warnings: Vec<String>,
}

/// `[@KILLTASKS]` 的一行：`怪物名 [数量] ["消息"]`（Crystal `QuestKillTask`，`QuestInfo.cs:492-497`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestKillTask {
    /// 怪物模板名（`MonsterInfo.Name` 的写法，比较时忽略大小写与空格）。
    pub monster: String,
    /// 需要击杀的数量。
    pub count: i32,
    /// 任务列表里的提示文案（行内双引号包起来的部分，可以为空）。
    pub message: String,
}

/// `[@ITEMTASKS]` / `[@CARRYITEMS]` 的一行：`物品名 [数量] ["消息"]`
/// （Crystal `QuestItemTask`，`QuestInfo.cs:499-504`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestItemTask {
    /// 物品模板名（`ItemInfo.Name` 的写法，比较时忽略大小写与空格）。
    pub item: String,
    /// 需要收集/携带的数量。
    pub count: u16,
    /// 任务列表里的提示文案。
    pub message: String,
}

/// `[@FLAGTASKS]` 的一行：`标记号 ["消息"]`（Crystal `QuestFlagTask`，`QuestInfo.cs:506-510`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestFlagTask {
    /// 标记位下标（`0..=999`，见 [`MAX_FLAG_TASK_NUMBER`]）。
    pub number: i32,
    /// 任务列表里的提示文案。
    pub message: String,
}

/// `[@FIXEDREWARDS]` / `[@SELECTREWARDS]` 的一行：`物品名 [数量]`
/// （Crystal `QuestItemReward`，`Shared/Data/QuestItemReward.cs`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestRewardLine {
    /// 物品模板名。
    pub item: String,
    /// 发放数量。
    pub count: u16,
}

/// 标记位任务号的上限：Crystal `number > Globals.FlagIndexCount - 1000` 直接丢弃
/// （`QuestInfo.cs:339`），`Globals.FlagIndexCount = 1999`（`Shared/Globals.cs:31`）。
pub const MAX_FLAG_TASK_NUMBER: i32 = 999;

/// 读脚本失败（文件不存在、编码问题等）。
#[derive(Debug, thiserror::Error)]
pub enum QuestScriptError {
    /// 读文件失败。
    #[error("读取任务脚本 {path} 失败：{source}")]
    Io {
        /// 出错的路径。
        path: String,
        /// 底层 IO 错误。
        source: std::io::Error,
    },
}

impl QuestScript {
    /// 载入一份任务脚本。
    ///
    /// `file_name` 是数据库里的 `QuestInfo.file_name`（例如 `BichonProvince/BichonWall/1`，
    /// 可以带 `.txt`，也可以是绝对路径）；`envir_root` 是 `Envir/` 目录，
    /// 用于拼 `Envir/Quests/…`（Crystal `Settings.QuestPath`，`Server/Settings.cs`）。
    pub fn load(
        file_name: impl AsRef<Path>,
        envir_root: impl AsRef<Path>,
    ) -> Result<Self, QuestScriptError> {
        let path = resolve_script_path(file_name.as_ref(), envir_root.as_ref());
        let lines = read_lines(&path)?;
        let name = file_name.as_ref().to_string_lossy().replace('\\', "/");
        Ok(Self::parse(name, &lines))
    }

    /// 直接从内存行解析。
    #[must_use]
    pub fn parse(file_name: impl AsRef<str>, lines: &[String]) -> Self {
        let mut script = Self {
            file_name: normalize_file_name(file_name.as_ref()),
            ..Self::default()
        };
        parse_sections(&mut script, lines);
        script
    }

    /// 脚本文件名（不带目录与扩展名）。
    #[must_use]
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// 解析告警。
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// 是否有任何「可完成」的目标（击杀 / 收集 / 标记）。
    ///
    /// Crystal 的 `QuestProgressInfo.CheckCompleted`（`QuestProgressInfo.cs:321-362`）在三个列表
    /// 全都为空时直接判定「可以完成」——所以没有目标的纯剧情任务也能交。
    #[must_use]
    pub fn has_tasks(&self) -> bool {
        !self.kill_tasks.is_empty() || !self.item_tasks.is_empty() || !self.flag_tasks.is_empty()
    }
}

/// 按 `QuestInfo.file_name` 拼出脚本真实路径（Crystal `Path.Combine(Settings.QuestPath, FileName + ".txt")`，
/// `QuestInfo.cs:136`）。
///
/// 绝对路径原样返回；相对路径拼 `envir_root/Quests/`，并补 `.txt`；反斜杠统一成 `/`
/// （数据库里的 `FileName` 两种分隔符都出现过）。
#[must_use]
pub fn resolve_script_path(file_name: &Path, envir_root: &Path) -> PathBuf {
    if file_name.is_absolute() {
        return file_name.to_path_buf();
    }
    let normalized = file_name.to_string_lossy().replace('\\', "/");
    let mut path = envir_root.join("Quests").join(normalized);
    if path.extension().is_none() {
        path.set_extension("txt");
    }
    path
}

/// 归一化脚本名：统一分隔符、去掉 `.txt`。
fn normalize_file_name(file_name: &str) -> String {
    let unified = file_name.replace('\\', "/");
    unified.strip_suffix(".txt").unwrap_or(&unified).to_owned()
}

/// 读文本行：按 `\n` 切、去掉行尾 `\r`、去掉 UTF-8 BOM（与 `NpcScript` 同口径）。
fn read_lines(path: &Path) -> Result<Vec<String>, QuestScriptError> {
    let bytes = std::fs::read(path).map_err(|source| QuestScriptError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    Ok(text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
        .collect())
}

/// 一个任务段的种类（与 Crystal `QuestInfo.cs:163-176` 的 13 个常量一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    /// `[@DESCRIPTION]`
    Description,
    /// `[@TASKDESCRIPTION]`
    TaskDescription,
    /// `[@RETURNDESCRIPTION]`
    ReturnDescription,
    /// `[@COMPLETION]`
    Completion,
    /// `[@CARRYITEMS]`
    CarryItems,
    /// `[@KILLTASKS]`
    KillTasks,
    /// `[@ITEMTASKS]`
    ItemTasks,
    /// `[@FLAGTASKS]`
    FlagTasks,
    /// `[@FIXEDREWARDS]`
    FixedRewards,
    /// `[@SELECTREWARDS]`
    SelectRewards,
    /// `[@EXPREWARD]`
    ExpReward,
    /// `[@GOLDREWARD]`
    GoldReward,
    /// `[@CREDITREWARD]`
    CreditReward,
}

/// `headers` 列表的照抄（顺序同 `QuestInfo.cs:178-183`），只影响告警顺序。
const HEADERS: &[(&str, Section)] = &[
    ("[@DESCRIPTION]", Section::Description),
    ("[@TASKDESCRIPTION]", Section::TaskDescription),
    ("[@COMPLETION]", Section::Completion),
    ("[@CARRYITEMS]", Section::CarryItems),
    ("[@KILLTASKS]", Section::KillTasks),
    ("[@ITEMTASKS]", Section::ItemTasks),
    ("[@FLAGTASKS]", Section::FlagTasks),
    ("[@FIXEDREWARDS]", Section::FixedRewards),
    ("[@SELECTREWARDS]", Section::SelectRewards),
    ("[@EXPREWARD]", Section::ExpReward),
    ("[@GOLDREWARD]", Section::GoldReward),
    ("[@CREDITREWARD]", Section::CreditReward),
    ("[@RETURNDESCRIPTION]", Section::ReturnDescription),
];

/// 把整份文件按 `[@SECTION]` 切开并逐段解析（Crystal `ParseFile`，`QuestInfo.cs:161-257`）。
///
/// Crystal 是「对每个段头扫全文件」，所以**段可以出现多次、顺序随意**；同一个段出现两次时
/// 两次的内容都会累加（`ParseFile` 没有 `break`）。这里保持同样的语义。
fn parse_sections(script: &mut QuestScript, lines: &[String]) {
    let upper: Vec<String> = lines
        .iter()
        .map(|line| line.trim().to_uppercase())
        .collect();
    for &(header, section) in HEADERS {
        let mut index = 0;
        while index < lines.len() {
            if upper[index] != header {
                index += 1;
                continue;
            }
            index += 1;
            // 段正文：到下一个 `[…]` 或 `//` 行为止（`QuestInfo.cs:199`）。
            while index < lines.len() {
                let raw = &lines[index];
                let trimmed = raw.trim();
                if raw.starts_with('[') || raw.starts_with("//") || trimmed.starts_with('[') {
                    break;
                }
                index += 1;
                if trimmed.is_empty() {
                    continue;
                }
                // Crystal 只跳空行；`;`/`#` 注释是本作扩展（见模块文档）。
                if raw.starts_with(';') || trimmed.starts_with('#') || trimmed.starts_with(';') {
                    continue;
                }
                // 正文按**原始行**收（Crystal `Description.Add(innerLine)`，`QuestInfo.cs:205`），
                // 数据行按去掉首尾空白的行解析。
                apply_line(script, section, raw, trimmed);
            }
        }
    }
    warn_unknown_sections(script, &upper);
}

/// 记录没被任何 `HEADERS` 认领的 `[…]` 段（Crystal 会静默忽略）。
fn warn_unknown_sections(script: &mut QuestScript, upper: &[String]) {
    for line in upper {
        if !line.starts_with('[') || !line.ends_with(']') {
            continue;
        }
        if HEADERS.iter().any(|(header, _)| header == line) {
            continue;
        }
        script
            .warnings
            .push(format!("忽略未知的任务段 {line}（Crystal 的段表里没有它）"));
    }
}

/// 把一段正文按段的种类分派（Crystal `ParseFile` 内层 `switch`，`QuestInfo.cs:202-251`）。
///
/// `raw` 是原始行（正文用），`line` 是去掉首尾空白的行（数据行用）。
fn apply_line(script: &mut QuestScript, section: Section, raw: &str, line: &str) {
    match section {
        Section::Description => script.description.push(raw.to_owned()),
        Section::TaskDescription => script.task_description.push(raw.to_owned()),
        Section::ReturnDescription => script.return_description.push(raw.to_owned()),
        Section::Completion => script.completion_description.push(raw.to_owned()),
        Section::CarryItems => match parse_item_line(line) {
            Some(task) => script.carry_items.push(task),
            None => warn(script, line, "CarryItem"),
        },
        Section::KillTasks => match parse_kill_line(line) {
            Some(task) => script.kill_tasks.push(task),
            None => warn(script, line, "KillTask"),
        },
        Section::ItemTasks => match parse_item_line(line) {
            Some(task) => script.item_tasks.push(task),
            None => warn(script, line, "ItemTask"),
        },
        Section::FlagTasks => match parse_flag_line(line) {
            Some(task) => script.flag_tasks.push(task),
            None => warn(script, line, "FlagTask"),
        },
        Section::FixedRewards => match parse_reward_line(line) {
            Some(reward) => script.fixed_rewards.push(reward),
            None => warn(script, line, "FixedReward"),
        },
        Section::SelectRewards => match parse_reward_line(line) {
            Some(reward) => script.select_rewards.push(reward),
            None => warn(script, line, "SelectReward"),
        },
        // Crystal：`uint.TryParse(innerLine, out ExpReward)`（`QuestInfo.cs:243`）——
        // 解析失败会把值写成 0（`TryParse` 的语义），且后出现的段覆盖前一次。
        Section::ExpReward => script.exp_reward = parse_uint(script, line, "ExpReward"),
        Section::GoldReward => script.gold_reward = parse_uint(script, line, "GoldReward"),
        Section::CreditReward => script.credit_reward = parse_uint(script, line, "CreditReward"),
    }
}

/// 记一条「丢弃了这一行」的告警。
fn warn(script: &mut QuestScript, line: &str, kind: &str) {
    script
        .warnings
        .push(format!("{kind} 行无法解析，已忽略：{line}"));
}

/// 解析一个 `u32` 奖励；写坏时按 Crystal 的 `TryParse` 语义取 0 并记告警。
fn parse_uint(script: &mut QuestScript, line: &str, kind: &str) -> u32 {
    if let Ok(value) = line.parse::<u32>() {
        return value;
    }
    script
        .warnings
        .push(format!("{kind} 的数值无法解析（Crystal 会写成 0）：{line}"));
    0
}

/// 取行内第一对双引号之间的内容（Crystal 正则 `"([^"]*)"`，`QuestInfo.cs:80`、`:295`）。
fn quoted_message(line: &str) -> String {
    let Some(start) = line.find('"') else {
        return String::new();
    };
    let after = &line[start + 1..];
    let Some(end) = after.find('"') else {
        return String::new();
    };
    after[..end].to_owned()
}

/// 解析 `[@KILLTASKS]` 的一行（Crystal `ParseKill`，`QuestInfo.cs:284-302`）。
#[must_use]
pub fn parse_kill_line(line: &str) -> Option<QuestKillTask> {
    let mut parts = line.split_whitespace();
    let monster = parts.next()?.to_owned();
    if monster.is_empty() || monster.starts_with('"') {
        return None;
    }
    // Crystal：`if (split.Length > 1) int.TryParse(split[1], out count);`（缺省 1）。
    let count = match parts.next() {
        Some(text) if text.starts_with('"') => 1,
        Some(text) => text.parse::<i32>().unwrap_or(0),
        None => 1,
    };
    Some(QuestKillTask {
        monster,
        count,
        message: quoted_message(line),
    })
}

/// 解析 `[@ITEMTASKS]` / `[@CARRYITEMS]` 的一行（Crystal `ParseItem`，`QuestInfo.cs:304-326`）。
#[must_use]
pub fn parse_item_line(line: &str) -> Option<QuestItemTask> {
    let mut parts = line.split_whitespace();
    let item = parts.next()?.to_owned();
    if item.is_empty() || item.starts_with('"') {
        return None;
    }
    let count = match parts.next() {
        Some(text) if text.starts_with('"') => 1,
        Some(text) => text.parse::<u16>().unwrap_or(0),
        None => 1,
    };
    Some(QuestItemTask {
        item,
        count,
        message: quoted_message(line),
    })
}

/// 解析 `[@FLAGTASKS]` 的一行（Crystal `ParseFlag`，`QuestInfo.cs:328-348`）。
///
/// 越界（`< 0` 或 `> 999`）时 Crystal 返回 `null`（`:339`），这里同样返回 `None`
/// （调用方记告警）。
#[must_use]
pub fn parse_flag_line(line: &str) -> Option<QuestFlagTask> {
    let number = line.split_whitespace().next()?.parse::<i32>().ok()?;
    if !(0..=MAX_FLAG_TASK_NUMBER).contains(&number) {
        return None;
    }
    Some(QuestFlagTask {
        number,
        message: quoted_message(line),
    })
}

/// 解析 `[@FIXEDREWARDS]` / `[@SELECTREWARDS]` 的一行（Crystal `ParseReward`，`QuestInfo.cs:259-282`）。
///
/// 简化：Crystal 在模板名不存在时会去找 `名字(M)` / `名字(F)` 两个性别版本（`:272-276`），
/// 那是「按性别挑外观」的行为；本 crate 没有模板表，交给服务端在解析奖励时处理。
#[must_use]
pub fn parse_reward_line(line: &str) -> Option<QuestRewardLine> {
    let mut parts = line.split_whitespace();
    let item = parts.next()?.to_owned();
    if item.is_empty() || item.starts_with('"') {
        return None;
    }
    // Crystal：`ushort count = 1; if (split.Length > 1) ushort.TryParse(split[1], out count);`
    // ——第二个字段存在但写坏时，`TryParse` 会把 count 写成 0。
    let count = match parts.next() {
        Some(text) => text.parse::<u16>().unwrap_or(0),
        None => 1,
    };
    Some(QuestRewardLine { item, count })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(text: &str) -> QuestScript {
        let lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
        QuestScript::parse("Test/1", &lines)
    }

    #[test]
    fn parses_every_section_of_a_real_shaped_file() {
        let quest = script(
            "[@Description]\n\
             Go kill {the/Crimson} yaks.\n\
             \n\
             [@TaskDescription]\n\
             Eliminate 5 yaks.\n\
             \n\
             [@Completion]\n\
             Well done.\n\
             \n\
             [@KillTasks]\n\
             ForestYeti 5\n\
             \n\
             [@ItemTasks]\n\
             BatTeeth 3 \"Collect bat teeth\"\n\
             \n\
             [@FlagTasks]\n\
             527 \"Speak with the TrainerTaoist\"\n\
             \n\
             [@CarryItems]\n\
             RepairOil 2\n\
             \n\
             [@FixedRewards]\n\
             PrecisionPendant\n\
             \n\
             [@SelectRewards]\n\
             BronzeShortSword\n\
             ElkWoodenBow 2\n\
             \n\
             [@ExpReward]\n\
             200\n\
             \n\
             [@GoldReward]\n\
             50\n\
             \n\
             [@CreditReward]\n\
             7\n",
        );
        assert_eq!(quest.file_name(), "Test/1");
        assert_eq!(quest.description, vec!["Go kill {the/Crimson} yaks."]);
        assert_eq!(quest.task_description, vec!["Eliminate 5 yaks."]);
        assert_eq!(quest.completion_description, vec!["Well done."]);
        assert_eq!(
            quest.kill_tasks,
            vec![QuestKillTask {
                monster: "ForestYeti".to_owned(),
                count: 5,
                message: String::new(),
            }]
        );
        assert_eq!(
            quest.item_tasks,
            vec![QuestItemTask {
                item: "BatTeeth".to_owned(),
                count: 3,
                message: "Collect bat teeth".to_owned(),
            }]
        );
        assert_eq!(
            quest.flag_tasks,
            vec![QuestFlagTask {
                number: 527,
                message: "Speak with the TrainerTaoist".to_owned(),
            }]
        );
        assert_eq!(quest.carry_items.len(), 1);
        assert_eq!(quest.carry_items[0].count, 2);
        assert_eq!(
            quest.fixed_rewards,
            vec![QuestRewardLine {
                item: "PrecisionPendant".to_owned(),
                count: 1,
            }]
        );
        assert_eq!(
            quest.select_rewards,
            vec![
                QuestRewardLine {
                    item: "BronzeShortSword".to_owned(),
                    count: 1,
                },
                QuestRewardLine {
                    item: "ElkWoodenBow".to_owned(),
                    count: 2,
                },
            ]
        );
        assert_eq!(quest.exp_reward, 200);
        assert_eq!(quest.gold_reward, 50);
        assert_eq!(quest.credit_reward, 7);
        assert!(quest.warnings.is_empty(), "{:?}", quest.warnings);
    }

    #[test]
    fn section_names_are_case_insensitive_and_order_independent() {
        let quest = script("[@killtasks]\nHen 2\n[@GoldReward]\n5\n");
        assert_eq!(quest.kill_tasks.len(), 1);
        assert_eq!(quest.kill_tasks[0].monster, "Hen");
        assert_eq!(quest.gold_reward, 5);
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let quest = script(
            "[@Description]\n\
             ;这是注释\n\
             \n\
             real text\n\
             // Crystal 的另一种注释写法\n\
             # 也是注释\n\
             [@GoldReward]\n\
             10\n",
        );
        assert_eq!(quest.description, vec!["real text"]);
        assert_eq!(quest.gold_reward, 10);
        assert!(quest.warnings.is_empty(), "{:?}", quest.warnings);
    }

    #[test]
    fn repeated_sections_accumulate() {
        // `ParseFile` 对每个段头扫全文件且不 `break`（`QuestInfo.cs:187-257`）。
        let quest = script("[@KillTasks]\nHen 1\n[@KillTasks]\nDeer 2\n");
        assert_eq!(quest.kill_tasks.len(), 2);
        assert_eq!(quest.kill_tasks[1].monster, "Deer");
    }

    #[test]
    fn bad_lines_become_warnings_not_failures() {
        // 旗标越界（Crystal `:339` 返回 null）、击杀行没有名字、经验写成非数字。
        let quest = script(
            "[@FlagTasks]\n\
             5000 \"way out of range\"\n\
             not-a-number\n\
             [@KillTasks]\n\
             \"only a message\"\n\
             [@ExpReward]\n\
             lots\n",
        );
        assert!(quest.flag_tasks.is_empty());
        // 坏行被丢弃，但整个脚本仍然解析成功。
        assert!(quest.kill_tasks.is_empty());
        assert_eq!(quest.exp_reward, 0, "Crystal 的 TryParse 语义：写坏取 0");
        let warnings = quest.warnings.join("\n");
        assert!(warnings.contains("FlagTask"), "{warnings}");
        assert!(warnings.contains("KillTask"), "{warnings}");
        assert!(warnings.contains("ExpReward"), "{warnings}");
    }

    #[test]
    fn unknown_sections_are_reported() {
        let quest = script("[@GOTO]\nsomewhere\n[@MAIN]\nhi\n");
        assert!(quest.description.is_empty());
        let warnings = quest.warnings.join("\n");
        assert!(warnings.contains("[@GOTO]"), "{warnings}");
        assert!(warnings.contains("[@MAIN]"), "{warnings}");
    }

    #[test]
    fn quoted_message_is_extracted() {
        assert_eq!(
            quoted_message("533 \"Enquire at the Library.\""),
            "Enquire at the Library."
        );
        assert_eq!(quoted_message("533"), "");
        assert_eq!(quoted_message("\"unterminated"), "");
    }

    #[test]
    fn counts_default_to_one_and_parse_errors_to_zero() {
        // Crystal：`split.Length > 1` 才解析，解析失败 `TryParse` 写 0。
        assert_eq!(parse_item_line("Sword").unwrap().count, 1);
        assert_eq!(parse_item_line("Sword abc").unwrap().count, 0);
        assert_eq!(parse_kill_line("Hen").unwrap().count, 1);
        assert_eq!(parse_kill_line("Hen 9").unwrap().count, 9);
        assert_eq!(parse_kill_line("Hen \"five\"").unwrap().count, 1);
        assert!(parse_kill_line("").is_none());
    }

    #[test]
    fn flag_bounds_follow_globals() {
        assert!(parse_flag_line("0").is_some());
        assert!(parse_flag_line("999").is_some());
        assert!(parse_flag_line("1000").is_none());
        assert!(parse_flag_line("-1").is_none());
    }

    #[test]
    fn resolve_path_joins_the_quest_directory() {
        let path = resolve_script_path(
            Path::new("BichonProvince\\BichonWall\\1"),
            Path::new("C:/mir/Envir"),
        );
        assert_eq!(
            path.to_string_lossy().replace('\\', "/"),
            "C:/mir/Envir/Quests/BichonProvince/BichonWall/1.txt"
        );
        // 绝对路径要原样返回：Windows 用 `C:/x/y.txt`，Unix 上它不算绝对路径，用 `/x/y.txt`。
        let (absolute_input, absolute_expected) = if cfg!(windows) {
            ("C:/x/y.txt", "C:/x/y.txt")
        } else {
            ("/x/y.txt", "/x/y.txt")
        };
        let absolute = resolve_script_path(Path::new(absolute_input), Path::new("C:/mir/Envir"));
        assert_eq!(
            absolute.to_string_lossy().replace('\\', "/"),
            absolute_expected
        );
        assert!(absolute.is_absolute(), "绝对路径应保持绝对：{absolute:?}");
    }
}
