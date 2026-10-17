//! 任务定义 `ClientQuestInfo` 的线格式。
//!
//! 对照 Crystal `Shared/Data/ClientData.cs:360`（读入 ctor `:395`，`Save` `:441`）手写。
//! ctor 与 `Save` 都不带 `version` 参数，没有版本分支。
//!
//! 线格式（一行一字段，全部小端）：
//!
//! ```text
//! i32    Index
//! u32    NPCIndex
//! string Name
//! string Group
//! i32    Description.Count          + 每条 string
//! i32    TaskDescription.Count      + 每条 string
//! i32    ReturnDescription.Count    + 每条 string
//! i32    CompletionDescription.Count + 每条 string
//! i32    MinLevelNeeded
//! i32    MaxLevelNeeded
//! i32    QuestNeeded
//! u8     ClassNeeded                RequiredClass（`[Flags]`，按位或）
//! u8     Type                       QuestType
//! i32    TimeLimitInSeconds
//! u32    RewardGold
//! u32    RewardExp
//! u32    RewardCredit
//! i32    RewardsFixedItem.Count     + 每个 QuestItemReward
//! i32    RewardsSelectItem.Count    + 每个 QuestItemReward
//! u32    FinishNPCIndex
//! ```
//!
//! 关键点：
//!
//! * 四段 `List<string>` 各带自己的 `int` 长度，**顺序固定**为
//!   `Description` → `TaskDescription` → `ReturnDescription` → `CompletionDescription`；
//!   与类里的声明顺序（`:367`-`:370`）一致。
//! * `RewardGold` / `RewardExp` / `RewardCredit` / `NPCIndex` / `FinishNPCIndex` 都是
//!   **`uint`**，不是 `int`；数量字段才是 `int`。
//! * `ClassNeeded` 是 `[Flags]` 枚举 `RequiredClass`，底层 `byte`（如 `Warrior|Wizard|Taoist` = 7）。
//!   注意它的取值里**没有 0**：读到 0 时 `from_value` 返回 `None`，这里退化为默认值
//!   `RequiredClass::Warrior`（crate 统一约定）。
//! * `SameFinishNPC`（`:388`）是**计算属性**，不上线——对应 [`ClientQuestInfo::same_finish_npc`]。
//! * `GetQuestIcon`（`:487`）是纯本地计算，也不上线——对应
//!   [`ClientQuestInfo::quest_icon`]。

use mir_core::enums::{QuestIcon, QuestType, RequiredClass};

use crate::data::QuestItemReward;
use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `ClientQuestInfo`（`Shared/Data/ClientData.cs:360`；读入 ctor `:395`，`Save` `:441`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClientQuestInfo {
    /// C# 字段 `Index`：任务编号。
    pub index: i32,
    /// C# 字段 `NPCIndex`：发布任务的 NPC（`uint`）。
    pub npc_index: u32,
    /// C# 字段 `Name`：任务名。
    pub name: String,
    /// C# 字段 `Group`：任务分组（同组任务互斥/串联）。
    pub group: String,
    /// C# 字段 `Description`：任务简介（多段）。
    pub description: Vec<String>,
    /// C# 字段 `TaskDescription`：任务目标描述（多段）。
    pub task_description: Vec<String>,
    /// C# 字段 `ReturnDescription`：交任务时的描述（多段）。
    pub return_description: Vec<String>,
    /// C# 字段 `CompletionDescription`：完成后的描述（多段）。
    pub completion_description: Vec<String>,
    /// C# 字段 `MinLevelNeeded`：接取等级下限。
    pub min_level_needed: i32,
    /// C# 字段 `MaxLevelNeeded`：接取等级上限。
    pub max_level_needed: i32,
    /// C# 字段 `QuestNeeded`：前置任务编号（`0` 表示无）。
    pub quest_needed: i32,
    /// C# 字段 `ClassNeeded`：可接职业（`RequiredClass`，按位或）。
    pub class_needed: RequiredClass,
    /// C# 字段 `Type`：任务类型。
    pub quest_type: QuestType,
    /// C# 字段 `TimeLimitInSeconds`：限时秒数，`0` 表示不限时。
    pub time_limit_in_seconds: i32,
    /// C# 字段 `RewardGold`：奖励金币（`uint`）。
    pub reward_gold: u32,
    /// C# 字段 `RewardExp`：奖励经验（`uint`）。
    pub reward_exp: u32,
    /// C# 字段 `RewardCredit`：奖励元宝/点券（`uint`）。
    pub reward_credit: u32,
    /// C# 字段 `RewardsFixedItem`：必定获得的物品。
    pub rewards_fixed_item: Vec<QuestItemReward>,
    /// C# 字段 `RewardsSelectItem`：任选其一的物品。
    pub rewards_select_item: Vec<QuestItemReward>,
    /// C# 字段 `FinishNPCIndex`：交任务的 NPC（`uint`）。
    pub finish_npc_index: u32,
}

impl ClientQuestInfo {
    /// 解码，见 `Shared/Data/ClientData.cs:395`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let index = r.read_i32()?;
        let npc_index = r.read_u32()?;
        let name = r.read_string()?;
        let group = r.read_string()?;

        let description = read_string_list(r)?;
        let task_description = read_string_list(r)?;
        let return_description = read_string_list(r)?;
        let completion_description = read_string_list(r)?;

        let min_level_needed = r.read_i32()?;
        let max_level_needed = r.read_i32()?;
        let quest_needed = r.read_i32()?;
        let class_needed = RequiredClass::from_value(r.read_u8()?).unwrap_or_default();
        let quest_type = QuestType::from_value(r.read_u8()?).unwrap_or_default();
        let time_limit_in_seconds = r.read_i32()?;
        let reward_gold = r.read_u32()?;
        let reward_exp = r.read_u32()?;
        let reward_credit = r.read_u32()?;

        let rewards_fixed_item = read_reward_list(r)?;
        let rewards_select_item = read_reward_list(r)?;

        let finish_npc_index = r.read_u32()?;

        Ok(Self {
            index,
            npc_index,
            name,
            group,
            description,
            task_description,
            return_description,
            completion_description,
            min_level_needed,
            max_level_needed,
            quest_needed,
            class_needed,
            quest_type,
            time_limit_in_seconds,
            reward_gold,
            reward_exp,
            reward_credit,
            rewards_fixed_item,
            rewards_select_item,
            finish_npc_index,
        })
    }

    /// 编码，见 `Shared/Data/ClientData.cs:441`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i32(self.index);
        w.write_u32(self.npc_index);
        w.write_string(&self.name);
        w.write_string(&self.group);

        write_string_list(w, &self.description);
        write_string_list(w, &self.task_description);
        write_string_list(w, &self.return_description);
        write_string_list(w, &self.completion_description);

        w.write_i32(self.min_level_needed);
        w.write_i32(self.max_level_needed);
        w.write_i32(self.quest_needed);
        w.write_u8(self.class_needed.value());
        w.write_u8(self.quest_type.value());
        w.write_i32(self.time_limit_in_seconds);
        w.write_u32(self.reward_gold);
        w.write_u32(self.reward_exp);
        w.write_u32(self.reward_credit);

        w.write_count(self.rewards_fixed_item.len());
        for reward in &self.rewards_fixed_item {
            reward.encode(w);
        }

        w.write_count(self.rewards_select_item.len());
        for reward in &self.rewards_select_item {
            reward.encode(w);
        }

        w.write_u32(self.finish_npc_index);
    }

    /// C# 计算属性 `SameFinishNPC`（`ClientData.cs:388`）：发布与交付是否为同一 NPC。
    ///
    /// 只是本地比较，不占线格式字节。
    #[must_use]
    pub fn same_finish_npc(&self) -> bool {
        self.npc_index == self.finish_npc_index
    }

    /// C# 方法 `GetQuestIcon(taken, completed)`（`ClientData.cs:487`-`:521`）。
    ///
    /// 纯本地计算，由任务类型 + 接取/完成状态决定 NPC 头顶的图标；
    /// `ClientQuestProgress.Icon`（`:536`）就是拿它的两个标志调这个方法。
    #[must_use]
    pub fn quest_icon(&self, taken: bool, completed: bool) -> QuestIcon {
        match self.quest_type {
            // `case QuestType.General:` 与 `case QuestType.Repeatable:` 共用一套黄色图标。
            QuestType::General | QuestType::Repeatable => {
                if completed {
                    QuestIcon::QuestionYellow
                } else if taken {
                    QuestIcon::QuestionWhite
                } else {
                    QuestIcon::ExclamationYellow
                }
            }
            QuestType::Daily => {
                if completed {
                    QuestIcon::QuestionBlue
                } else if taken {
                    QuestIcon::QuestionWhite
                } else {
                    QuestIcon::ExclamationBlue
                }
            }
            QuestType::Story => {
                if completed {
                    QuestIcon::QuestionGreen
                } else if taken {
                    QuestIcon::QuestionWhite
                } else {
                    QuestIcon::ExclamationGreen
                }
            }
        }
    }
}

/// 读一段 `int count + string[count]`（C# 里反复出现的 `List<string>` 读法）。
fn read_string_list(r: &mut PacketReader<'_>) -> Result<Vec<String>, ProtocolError> {
    let count = r.read_count()?;
    // 不预分配：长度来自不可信输入。
    let mut out = Vec::new();
    for _ in 0..count {
        out.push(r.read_string()?);
    }
    Ok(out)
}

/// 写一段 `int count + string[count]`。
fn write_string_list(w: &mut PacketWriter, values: &[String]) {
    w.write_count(values.len());
    for value in values {
        w.write_string(value);
    }
}

/// 读一段 `int count + QuestItemReward[count]`。
fn read_reward_list(r: &mut PacketReader<'_>) -> Result<Vec<QuestItemReward>, ProtocolError> {
    let count = r.read_count()?;
    // 不预分配：长度来自不可信输入，且每条奖励内嵌一个变长的 `ItemInfo`。
    let mut out = Vec::new();
    for _ in 0..count {
        out.push(QuestItemReward::decode(r)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::ItemInfo;

    /// 编码后再解码，断言相等且负载被正好读完。
    fn roundtrip(value: &ClientQuestInfo) {
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = ClientQuestInfo::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, *value, "解码结果应与原值相同");
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn empty_lists_match_hand_derived_bytes() {
        // 按 `ClientData.cs:441` 的写出顺序逐字段推导（全部小端）：
        //   i32 Index                  = 1            -> 01 00 00 00
        //   u32 NPCIndex               = 2            -> 02 00 00 00
        //   string Name                = "Q"          -> 01 51
        //   string Group               = "G"          -> 01 47
        //   i32 Description.Count      = 0            -> 00 00 00 00
        //   i32 TaskDescription.Count  = 0            -> 00 00 00 00
        //   i32 ReturnDescription.Count= 0            -> 00 00 00 00
        //   i32 CompletionDesc.Count   = 0            -> 00 00 00 00
        //   i32 MinLevelNeeded         = 1            -> 01 00 00 00
        //   i32 MaxLevelNeeded         = 99           -> 63 00 00 00
        //   i32 QuestNeeded            = 0            -> 00 00 00 00
        //   u8  ClassNeeded  = RequiredClass.Warrior = 1 -> 01
        //   u8  Type         = QuestType.Daily = 1       -> 01
        //   i32 TimeLimitInSeconds     = 0            -> 00 00 00 00
        //   u32 RewardGold             = 100          -> 64 00 00 00
        //   u32 RewardExp              = 200          -> C8 00 00 00
        //   u32 RewardCredit           = 300          -> 2C 01 00 00
        //   i32 RewardsFixedItem.Count = 0            -> 00 00 00 00
        //   i32 RewardsSelectItem.Count= 0            -> 00 00 00 00
        //   u32 FinishNPCIndex         = 2            -> 02 00 00 00
        // 合计 4+4+2+2+16+4+4+4+1+1+4+4+4+4+4+4+4+4 = 70 字节。
        let value = ClientQuestInfo {
            index: 1,
            npc_index: 2,
            name: "Q".to_owned(),
            group: "G".to_owned(),
            min_level_needed: 1,
            max_level_needed: 99,
            class_needed: RequiredClass::Warrior,
            quest_type: QuestType::Daily,
            reward_gold: 100,
            reward_exp: 200,
            reward_credit: 300,
            finish_npc_index: 2,
            ..ClientQuestInfo::default()
        };

        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let expected: Vec<u8> = vec![
            0x01, 0x00, 0x00, 0x00, // Index
            0x02, 0x00, 0x00, 0x00, // NPCIndex
            0x01, 0x51, // Name = "Q"
            0x01, 0x47, // Group = "G"
            0x00, 0x00, 0x00, 0x00, // Description.Count
            0x00, 0x00, 0x00, 0x00, // TaskDescription.Count
            0x00, 0x00, 0x00, 0x00, // ReturnDescription.Count
            0x00, 0x00, 0x00, 0x00, // CompletionDescription.Count
            0x01, 0x00, 0x00, 0x00, // MinLevelNeeded
            0x63, 0x00, 0x00, 0x00, // MaxLevelNeeded = 99
            0x00, 0x00, 0x00, 0x00, // QuestNeeded
            0x01, // ClassNeeded = Warrior
            0x01, // Type = Daily
            0x00, 0x00, 0x00, 0x00, // TimeLimitInSeconds
            0x64, 0x00, 0x00, 0x00, // RewardGold = 100
            0xC8, 0x00, 0x00, 0x00, // RewardExp = 200
            0x2C, 0x01, 0x00, 0x00, // RewardCredit = 300
            0x00, 0x00, 0x00, 0x00, // RewardsFixedItem.Count
            0x00, 0x00, 0x00, 0x00, // RewardsSelectItem.Count
            0x02, 0x00, 0x00, 0x00, // FinishNPCIndex
        ];

        assert_eq!(bytes.len(), 70);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn roundtrip_with_descriptions_and_rewards() {
        let fixed_item = QuestItemReward {
            item: ItemInfo {
                index: 42,
                name: "屠龙".to_owned(),
                ..ItemInfo::default()
            },
            count: 1,
        };
        let select_item = QuestItemReward {
            item: ItemInfo {
                index: 43,
                name: "开天".to_owned(),
                ..ItemInfo::default()
            },
            count: 3,
        };

        let value = ClientQuestInfo {
            index: 100,
            npc_index: 200,
            name: "鸡飞狗跳".to_owned(),
            group: "新手村".to_owned(),
            description: vec!["村长有请。".to_owned(), "去村口看看。".to_owned()],
            task_description: vec!["击杀 10 只鸡".to_owned()],
            return_description: vec!["回去复命".to_owned()],
            completion_description: vec!["干得漂亮".to_owned(), String::new()],
            min_level_needed: 1,
            max_level_needed: 7,
            quest_needed: -1,
            class_needed: RequiredClass::WarWizTao,
            quest_type: QuestType::Repeatable,
            time_limit_in_seconds: 3600,
            reward_gold: 1_000,
            reward_exp: 20_000,
            reward_credit: 5,
            rewards_fixed_item: vec![fixed_item],
            rewards_select_item: vec![select_item],
            finish_npc_index: 201,
        };

        roundtrip(&value);
    }

    #[test]
    fn same_finish_npc_is_computed_not_written() {
        // `SameFinishNPC`（`ClientData.cs:388`）不在线上：两个 NPC 相同与否，
        // 编码字节数完全一样。
        let same = ClientQuestInfo {
            npc_index: 7,
            finish_npc_index: 7,
            ..ClientQuestInfo::default()
        };
        let different = ClientQuestInfo {
            npc_index: 7,
            finish_npc_index: 8,
            ..ClientQuestInfo::default()
        };

        assert!(same.same_finish_npc());
        assert!(!different.same_finish_npc());

        let mut a = PacketWriter::new();
        same.encode(&mut a);
        let mut b = PacketWriter::new();
        different.encode(&mut b);
        assert_eq!(a.len(), b.len(), "计算属性不应影响线格式长度");
    }

    #[test]
    fn quest_icon_matches_crystal_switch() {
        // `GetQuestIcon`（`ClientData.cs:487`-`:521`）。
        let quest = |quest_type| ClientQuestInfo {
            quest_type,
            ..ClientQuestInfo::default()
        };

        for quest_type in [QuestType::General, QuestType::Repeatable] {
            let info = quest(quest_type);
            assert_eq!(info.quest_icon(false, false), QuestIcon::ExclamationYellow);
            assert_eq!(info.quest_icon(true, false), QuestIcon::QuestionWhite);
            assert_eq!(info.quest_icon(true, true), QuestIcon::QuestionYellow);
        }

        let daily = quest(QuestType::Daily);
        assert_eq!(daily.quest_icon(false, false), QuestIcon::ExclamationBlue);
        assert_eq!(daily.quest_icon(true, false), QuestIcon::QuestionWhite);
        assert_eq!(daily.quest_icon(true, true), QuestIcon::QuestionBlue);

        let story = quest(QuestType::Story);
        assert_eq!(story.quest_icon(false, false), QuestIcon::ExclamationGreen);
        assert_eq!(story.quest_icon(true, false), QuestIcon::QuestionWhite);
        assert_eq!(story.quest_icon(true, true), QuestIcon::QuestionGreen);
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        ClientQuestInfo::default().encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(ClientQuestInfo::decode(&mut reader).is_err());
    }
}
