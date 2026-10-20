//! 新手引导任务链：表驱动（`config/newbie/chain.toml`），客户端只做提示/追踪，不改协议。
//!
//! 步骤顺序：接任务 → 自然刷 3 只鸡 → 交任务 → 学技能 → 买第一件装备。
//! 数据缺失/损坏时退回内置默认链，不 panic（与 `client_config` 的容错口径一致）。

use crate::net::WorldSnapshot;

/// 步骤目标类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewbieObjective {
    /// 接任务（`S.ChangeQuest{Add}` 后 `quests` 非空）。
    QuestAccept,
    /// 击杀指定怪物 `count` 只（进度文本里出现 `count/count`）。
    Kill { target: String, count: u32 },
    /// 交任务（`QuestView::completed` 为真）。
    QuestFinish,
    /// 学第一个技能（`magics` 非空）。
    LearnSpell,
    /// 买第一件装备（装备栏至少 2 件，出生装 1 件 + 新买 1 件）。
    BuyEquipment,
}

/// 一步。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewbieStep {
    /// 稳定 id（配置用）。
    pub id: String,
    /// 中文标题。
    pub zh: String,
    /// 英文标题。
    pub en: String,
    /// 中文提示。
    pub hint_zh: String,
    /// 英文提示。
    pub hint_en: String,
    /// 目标。
    pub objective: NewbieObjective,
}

impl NewbieStep {
    /// 当前语言的标题。
    #[must_use]
    pub fn title(&self, english: bool) -> &str {
        if english { &self.en } else { &self.zh }
    }

    /// 当前语言的提示。
    #[must_use]
    pub fn hint(&self, english: bool) -> &str {
        if english {
            &self.hint_en
        } else {
            &self.hint_zh
        }
    }
}

/// 新手链。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewbieChain {
    /// 步骤（按顺序）。
    pub steps: Vec<NewbieStep>,
}

impl Default for NewbieChain {
    fn default() -> Self {
        Self {
            steps: vec![
                NewbieStep {
                    id: "accept".to_owned(),
                    zh: "找新手向导接任务".to_owned(),
                    en: "Accept the newbie quest".to_owned(),
                    hint_zh: "与比奇城的新手向导对话，接下「初入江湖」。".to_owned(),
                    hint_en: "Talk to the newbie guide in Bichon and accept the quest.".to_owned(),
                    objective: NewbieObjective::QuestAccept,
                },
                NewbieStep {
                    id: "hunt".to_owned(),
                    zh: "自然刷 3 只鸡".to_owned(),
                    en: "Hunt 3 hens".to_owned(),
                    hint_zh: "在城外打 3 只鸡；任务进度到 3/3 即可。".to_owned(),
                    hint_en: "Kill 3 hens outside town until quest progress is 3/3.".to_owned(),
                    objective: NewbieObjective::Kill {
                        target: "鸡".to_owned(),
                        count: 3,
                    },
                },
                NewbieStep {
                    id: "deliver".to_owned(),
                    zh: "回去交任务".to_owned(),
                    en: "Turn in the quest".to_owned(),
                    hint_zh: "回到新手向导处交任务，确认金币/物品到账。".to_owned(),
                    hint_en: "Return to the guide and turn in; verify the reward.".to_owned(),
                    objective: NewbieObjective::QuestFinish,
                },
                NewbieStep {
                    id: "learn".to_owned(),
                    zh: "学第一个技能".to_owned(),
                    en: "Learn your first spell".to_owned(),
                    hint_zh: "到书店买第一本技能书并学习。".to_owned(),
                    hint_en: "Buy and learn your first skill book.".to_owned(),
                    objective: NewbieObjective::LearnSpell,
                },
                NewbieStep {
                    id: "buy".to_owned(),
                    zh: "买第一件装备".to_owned(),
                    en: "Buy your first equipment".to_owned(),
                    hint_zh: "到装备店买一件可穿的装备，确认扣金币并入包。".to_owned(),
                    hint_en: "Buy wearable equipment; verify gold spent and item in bag."
                        .to_owned(),
                    objective: NewbieObjective::BuyEquipment,
                },
            ],
        }
    }
}

impl NewbieChain {
    /// 从 `config/newbie/chain.toml` 读；缺失/坏文件回落默认并返回 `true` 表示用了默认。
    #[must_use]
    pub fn load(path: impl AsRef<std::path::Path>) -> (Self, bool) {
        let Ok(text) = std::fs::read_to_string(path) else {
            return (Self::default(), true);
        };
        match Self::parse(&text) {
            Ok(chain) if !chain.steps.is_empty() => (chain, false),
            _ => (Self::default(), true),
        }
    }

    /// 解析 TOML 子集（`[[steps]]` + 字符串/整数）。
    ///
    /// # Errors
    /// 缺字段、未知 objective、类型错误时返回错误。
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut steps: Vec<NewbieStep> = Vec::new();
        let mut current: Option<NewbieStep> = None;
        for (line_number, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "[[steps]]" {
                if let Some(step) = current.take() {
                    steps.push(step);
                }
                current = Some(NewbieStep {
                    id: String::new(),
                    zh: String::new(),
                    en: String::new(),
                    hint_zh: String::new(),
                    hint_en: String::new(),
                    objective: NewbieObjective::QuestAccept,
                });
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("第 {} 行缺少 '='", line_number + 1));
            };
            let step = current
                .as_mut()
                .ok_or_else(|| format!("第 {} 行不在 [[steps]] 内", line_number + 1))?;
            let key = key.trim();
            let value = value.trim();
            match key {
                "id" => step.id = parse_string(value)?,
                "zh" => step.zh = parse_string(value)?,
                "en" => step.en = parse_string(value)?,
                "hint_zh" => step.hint_zh = parse_string(value)?,
                "hint_en" => step.hint_en = parse_string(value)?,
                "objective" => {
                    step.objective = match parse_string(value)?.as_str() {
                        "quest_accept" => NewbieObjective::QuestAccept,
                        "kill" => NewbieObjective::Kill {
                            target: String::new(),
                            count: 0,
                        },
                        "quest_finish" => NewbieObjective::QuestFinish,
                        "learn_spell" => NewbieObjective::LearnSpell,
                        "buy_item" => NewbieObjective::BuyEquipment,
                        other => return Err(format!("未知 objective：{other}")),
                    };
                }
                "target" => {
                    if let NewbieObjective::Kill { target, .. } = &mut step.objective {
                        *target = parse_string(value)?;
                    }
                }
                "count" => {
                    if let NewbieObjective::Kill { count, .. } = &mut step.objective {
                        *count = value
                            .parse()
                            .map_err(|_| format!("count 非整数：{value}"))?;
                    }
                }
                other => return Err(format!("未知键：{other}")),
            }
        }
        if let Some(step) = current.take() {
            steps.push(step);
        }
        if steps.iter().any(|step| step.id.is_empty()) {
            return Err("步骤缺 id".to_owned());
        }
        Ok(Self { steps })
    }

    /// 当前应做的步骤下标；全部完成时返回 `steps.len()`。
    #[must_use]
    pub fn current_step(&self, snapshot: &WorldSnapshot) -> usize {
        let done = self.done_flags(snapshot);
        done.iter()
            .position(|done| !done)
            .unwrap_or(self.steps.len())
    }

    /// 每步是否已完成。
    #[must_use]
    pub fn done_flags(&self, snapshot: &WorldSnapshot) -> Vec<bool> {
        self.steps
            .iter()
            .map(|step| step_done(&step.objective, snapshot))
            .collect()
    }
}

fn step_done(objective: &NewbieObjective, snapshot: &WorldSnapshot) -> bool {
    match objective {
        NewbieObjective::QuestAccept => !snapshot.quests.is_empty(),
        NewbieObjective::Kill { target, count } => snapshot.quests.iter().any(|quest| {
            quest.completed
                || quest.task_list.iter().any(|line| {
                    line.contains(target.as_str())
                        && (line.contains(&format!("{count}/{count}"))
                            || line.contains(&format!("{count} / {count}")))
                })
        }),
        NewbieObjective::QuestFinish => snapshot.quests.iter().any(|quest| quest.completed),
        NewbieObjective::LearnSpell => !snapshot.magics.is_empty(),
        NewbieObjective::BuyEquipment => snapshot.equipment.iter().flatten().count() >= 2,
    }
}

fn parse_string(value: &str) -> Result<String, String> {
    let Some(inner) = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        return Err(format!("需要字符串：{value}"));
    };
    Ok(inner.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_have_five_steps() {
        let chain = NewbieChain::default();
        assert_eq!(chain.steps.len(), 5);
        assert_eq!(chain.steps[0].id, "accept");
        assert_eq!(chain.steps[4].id, "buy");
    }

    #[test]
    fn parses_chain_file() {
        let text = r#"
[[steps]]
id = "accept"
zh = "接"
en = "Accept"
hint_zh = "去接"
hint_en = "go"
objective = "quest_accept"

[[steps]]
id = "hunt"
zh = "打"
en = "Hunt"
hint_zh = "打 3"
hint_en = "kill 3"
objective = "kill"
target = "鸡"
count = 3
"#;
        let chain = NewbieChain::parse(text).expect("解析");
        assert_eq!(chain.steps.len(), 2);
        assert_eq!(
            chain.steps[1].objective,
            NewbieObjective::Kill {
                target: "鸡".to_owned(),
                count: 3
            }
        );
    }

    #[test]
    fn bad_file_falls_back_without_panic() {
        let (chain, fallback) = NewbieChain::load("definitely-missing-newbie-chain.toml");
        assert!(fallback);
        assert_eq!(chain.steps.len(), 5);
        assert!(NewbieChain::parse("not toml").is_err());
    }

    #[test]
    fn current_step_advances_with_snapshot() {
        let mut snapshot = WorldSnapshot::default();
        let chain = NewbieChain::default();
        assert_eq!(chain.current_step(&snapshot), 0);
        let mut quest = crate::net::QuestView {
            id: 1,
            taken: true,
            ..crate::net::QuestView::default()
        };
        snapshot.quests.push(quest.clone());
        assert_eq!(chain.current_step(&snapshot), 1);
        quest.task_list.push("击杀 鸡 3/3".to_owned());
        quest.completed = true;
        snapshot.quests[0] = quest;
        assert_eq!(chain.current_step(&snapshot), 3);
        snapshot.magics.push(crate::net::MagicView {
            name: "火球术".to_owned(),
            spell: mir_core::enums::Spell::FireBall,
            level: 1,
            key: 0,
            base_cost: 0,
            icon: 0,
            range: 0,
        });
        assert_eq!(chain.current_step(&snapshot), 4);
    }
}
