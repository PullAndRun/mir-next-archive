//! 客户端任务进度 `ClientQuestProgress` 的线格式。
//!
//! 对照 Crystal `Shared/Data/ClientData.cs:524`（读入 ctor `:546`，`Save` `:560`）手写。
//! ctor 与 `Save` 都不带 `version` 参数，没有版本分支。
//!
//! 线格式（一行一字段，全部小端）：
//!
//! ```text
//! i32    Id
//! i32    TaskList.Count + 每条 string
//! bool   Taken
//! bool   Completed
//! bool   New
//! ```
//!
//! **两个只在本地、不上线的成员**（这是本类最容易踩的坑）：
//!
//! * `QuestInfo`（`:528`，指向完整任务定义的引用）——`Save` 完全不写它，
//!   客户端收到进度后按 `Id` 去已缓存的 `ClientQuestInfo` 表里查。
//! * `Icon`（`:536` 的计算属性）——由 `QuestInfo.GetQuestIcon(Taken, Completed)`
//!   现算，没有落盘形态。
//!
//! 因此 Rust 结构体里没有这两个字段；需要图标时用
//! `ClientQuestInfo::quest_icon(taken, completed)` 现算。

use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `ClientQuestProgress`（`Shared/Data/ClientData.cs:524`；读入 ctor `:546`，`Save` `:560`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientQuestProgress {
    /// C# 字段 `Id`：任务编号，用来关联 `ClientQuestInfo.Index`。
    pub id: i32,
    /// C# 字段 `TaskList`：已展开的任务步骤文本。
    pub task_list: Vec<String>,
    /// C# 字段 `Taken`：是否已接取。
    pub taken: bool,
    /// C# 字段 `Completed`：是否已完成。
    pub completed: bool,
    /// C# 字段 `New`：是否为「新」任务（客户端高亮用）。
    pub new: bool,
}

impl ClientQuestProgress {
    /// 解码，见 `Shared/Data/ClientData.cs:546`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let id = r.read_i32()?;

        let count = r.read_count()?;
        // 不预分配：长度来自不可信输入。
        let mut task_list = Vec::new();
        for _ in 0..count {
            task_list.push(r.read_string()?);
        }

        let taken = r.read_bool()?;
        let completed = r.read_bool()?;
        let new = r.read_bool()?;

        Ok(Self {
            id,
            task_list,
            taken,
            completed,
            new,
        })
    }

    /// 编码，见 `Shared/Data/ClientData.cs:560`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i32(self.id);

        w.write_count(self.task_list.len());
        for task in &self.task_list {
            w.write_string(task);
        }

        w.write_bool(self.taken);
        w.write_bool(self.completed);
        w.write_bool(self.new);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 编码后再解码，断言相等且负载被正好读完。
    fn roundtrip(value: &ClientQuestProgress) {
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = ClientQuestProgress::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, *value, "解码结果应与原值相同");
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn bytes_match_hand_derived_layout() {
        // 按 `ClientData.cs:560` 的写出顺序推导（全部小端）：
        //   i32 Id                = 5            -> 05 00 00 00
        //   i32 TaskList.Count    = 2            -> 02 00 00 00
        //   string [0] = "Kill"                  -> 04 4B 69 6C 6C
        //   string [1] = "Get"                   -> 03 47 65 74
        //   bool Taken            = true         -> 01
        //   bool Completed        = false        -> 00
        //   bool New              = true         -> 01
        // 合计 4 + 4 + 5 + 4 + 1 + 1 + 1 = 20 字节。
        let value = ClientQuestProgress {
            id: 5,
            task_list: vec!["Kill".to_owned(), "Get".to_owned()],
            taken: true,
            completed: false,
            new: true,
        };

        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let expected: Vec<u8> = vec![
            0x05, 0x00, 0x00, 0x00, // Id
            0x02, 0x00, 0x00, 0x00, // TaskList.Count
            0x04, 0x4B, 0x69, 0x6C, 0x6C, // "Kill"
            0x03, 0x47, 0x65, 0x74, // "Get"
            0x01, // Taken
            0x00, // Completed
            0x01, // New
        ];

        assert_eq!(bytes.len(), 20);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn no_quest_info_on_the_wire() {
        // 回归点：`QuestInfo` 与 `Icon` 不在线上。空进度只有
        // i32 Id + i32 长度 + 3 个 bool = 11 字节，
        // 无论它关联的 `ClientQuestInfo` 有多大。
        let value = ClientQuestProgress {
            id: -1,
            ..ClientQuestProgress::default()
        };
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();
        assert_eq!(
            bytes,
            vec![
                0xFF, 0xFF, 0xFF, 0xFF, // Id = -1
                0x00, 0x00, 0x00, 0x00, // TaskList.Count = 0
                0x00, 0x00, 0x00, // Taken / Completed / New
            ]
        );
        roundtrip(&value);
    }

    #[test]
    fn roundtrip_with_chinese_tasks() {
        let value = ClientQuestProgress {
            id: 10086,
            task_list: vec![
                "击杀 10 只鸡".to_owned(),
                "把信交给老兵".to_owned(),
                String::new(),
            ],
            taken: true,
            completed: true,
            new: false,
        };
        roundtrip(&value);
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        ClientQuestProgress::default().encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(ClientQuestProgress::decode(&mut reader).is_err());
    }
}
