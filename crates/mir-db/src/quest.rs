//! 任务段：`QuestInfo` 的**头部**。
//!
//! 读入顺序见 `Server/MirDatabase/QuestInfo.cs:85-110`（`QuestInfo(BinaryReader reader)`），
//! 写出顺序见 `QuestInfo.cs:112-128`（`Save`）。
//!
//! 数据库里**只有头部**：任务目标、奖励、描述全部来自
//! `Envir/Quests/{FileName}.txt`（`QuestInfo.cs:109` 的 `LoadInfo()`，解析在 `:130-146` 之后），
//! 所以这里没有 `CarryItems` / `KillTasks` / `GoldReward` 之类的字段。

use crate::MirDbError;
use crate::reader::{DbReader, DbWriter};

/// `QuestInfo` 记录的最小字节数（`QuestInfo.cs:87-102`，不含 `> 90` 的时限）。
pub const MIN_QUEST_LEN: usize = 25;

/// Crystal `QuestInfo`（`Server/MirDatabase/QuestInfo.cs:7`；读 `:85-110`，写 `:112-128`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestInfo {
    /// 主键（`QuestInfo.cs:87`）。
    pub index: i32,
    /// 任务名（`:88`）。
    pub name: String,
    /// 任务分组（`:89`），样本里是地图名，如 `比奇省`。
    pub group: String,
    /// 任务脚本文件名（`:90`），正文在 `Envir/Quests/{file_name}.txt`。
    pub file_name: String,
    /// 接取所需最低等级（`:91`）。
    pub required_min_level: i32,
    /// 接取所需最高等级（`:93`）。
    ///
    /// **注意**：C# 读到 0 时会立刻改写成 `ushort.MaxValue`（`:94`）——那是载入后的语义修正，
    /// 不是磁盘上的值。这里保留原始值，需要 Crystal 语义时用
    /// [`QuestInfo::effective_required_max_level`]。
    pub required_max_level: i32,
    /// 前置任务编号（`:96`）。
    pub required_quest: i32,
    /// 接取所需职业（`u8` 位掩码，`:97`）。
    pub required_class: u8,
    /// 任务类型（`QuestType`，`u8`，`:98`）。
    pub quest_type: u8,
    /// 前往提示（`:99`）。
    pub goto_message: String,
    /// 击杀提示（`:100`）。
    pub kill_message: String,
    /// 收集提示（`:101`）。
    pub item_message: String,
    /// Flag 提示（`:102`）。
    pub flag_message: String,
    /// 时限（秒，`> 90`，`:104-107`）。旧文件没有，读成 0（表示不限时）。
    pub time_limit_in_seconds: i32,
}

impl QuestInfo {
    /// 解码，见 `QuestInfo.cs:85-110`。版本门：`> 90` 才有 `time_limit_in_seconds`。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        let index = r.i32()?;
        let name = r.string()?;
        let group = r.string()?;
        let file_name = r.string()?;
        let required_min_level = r.i32()?;
        let required_max_level = r.i32()?;
        let required_quest = r.i32()?;
        let required_class = r.u8()?;
        let quest_type = r.u8()?;
        let goto_message = r.string()?;
        let kill_message = r.string()?;
        let item_message = r.string()?;
        let flag_message = r.string()?;

        let time_limit_in_seconds = if r.version() > 90 { r.i32()? } else { 0 };

        Ok(Self {
            index,
            name,
            group,
            file_name,
            required_min_level,
            required_max_level,
            required_quest,
            required_class,
            quest_type,
            goto_message,
            kill_message,
            item_message,
            flag_message,
            time_limit_in_seconds,
        })
    }

    /// 编码，见 `QuestInfo.cs:112-128`（`version <= 90` 时不写时限）。
    pub fn encode(&self, w: &mut DbWriter, version: i32) {
        w.i32(self.index);
        w.string(&self.name);
        w.string(&self.group);
        w.string(&self.file_name);
        w.i32(self.required_min_level);
        w.i32(self.required_max_level);
        w.i32(self.required_quest);
        w.u8(self.required_class);
        w.u8(self.quest_type);
        w.string(&self.goto_message);
        w.string(&self.kill_message);
        w.string(&self.item_message);
        w.string(&self.flag_message);
        if version > 90 {
            w.i32(self.time_limit_in_seconds);
        }
    }

    /// Crystal 语义的等级上限：`0` 表示不限制，等价于 `ushort.MaxValue`（`QuestInfo.cs:94`）。
    #[must_use]
    pub const fn effective_required_max_level(&self) -> i32 {
        if self.required_max_level == 0 {
            u16::MAX as i32
        } else {
            self.required_max_level
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> QuestInfo {
        QuestInfo {
            index: 1,
            name: "助理的要求".to_owned(),
            group: "比奇省".to_owned(),
            file_name: "1".to_owned(),
            required_min_level: 1,
            required_max_level: 255,
            required_quest: 0,
            required_class: 31,
            quest_type: 1,
            goto_message: "去比奇".to_owned(),
            kill_message: "杀鸡".to_owned(),
            item_message: "交肉".to_owned(),
            flag_message: String::new(),
            time_limit_in_seconds: 3_600,
        }
    }

    #[test]
    fn roundtrip_all_versions() {
        for version in [60, 84, 90, 91, 117] {
            let mut writer = DbWriter::new();
            sample().encode(&mut writer, version);
            let bytes = writer.into_inner();

            let mut reader = DbReader::new(&bytes, version);
            let decoded = QuestInfo::parse(&mut reader).expect("解码应当成功");
            assert_eq!(reader.remaining(), 0, "应正好读完，版本 {version}");

            let mut expected = sample();
            if version <= 90 {
                expected.time_limit_in_seconds = 0;
            }
            assert_eq!(decoded, expected, "版本 {version} 的解码结果");
        }
    }

    #[test]
    fn zero_max_level_means_unlimited() {
        // `QuestInfo.cs:94`：`if (RequiredMaxLevel == 0) RequiredMaxLevel = ushort.MaxValue;`
        let quest = QuestInfo {
            required_max_level: 0,
            ..QuestInfo::default()
        };
        assert_eq!(quest.effective_required_max_level(), 65_535);
        let quest = QuestInfo {
            required_max_level: 300,
            ..QuestInfo::default()
        };
        assert_eq!(quest.effective_required_max_level(), 300);
    }

    #[test]
    fn truncated_quest_is_an_error() {
        let mut writer = DbWriter::new();
        sample().encode(&mut writer, 105);
        let bytes = writer.into_inner();
        let mut reader = DbReader::new(&bytes[..MIN_QUEST_LEN], 105);
        assert!(QuestInfo::parse(&mut reader).is_err());
    }
}
