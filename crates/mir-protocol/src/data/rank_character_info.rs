//! 排行榜条目 `RankCharacterInfo` 的线格式。
//!
//! 对照 Crystal `Shared/Data/SharedData.cs:43`（读入 ctor `:58`，`Save` `:66`）手写。
//! ctor 与 `Save` 都不带 `version` 参数，没有版本分支。
//!
//! 线格式（一行一字段，全部小端）：
//!
//! ```text
//! i64    PlayerId     角色编号
//! string Name         角色名（7 位长度前缀 + UTF-8）
//! i32    level        等级（C# 里字段名就是小写 `level`，`:48`）
//! u8     Class        MirClass
//! ```
//!
//! **只在服务器内存里、不上线的三个字段**（`:50`-`:52`）：
//!
//! * `Experience`（`long`，`:50`）——注释写着 "clients shouldnt care about this only server"，
//!   排序用的经验值只留在服务器；客户端靠 `Envir.cs` 侧的排序结果拿到顺序。
//! * `object info`（`:51`）——声明为 `object`，注释 "again only keep this on server!"，
//!   实际存放服务器侧的 `CharacterInfo` 引用，`Save` 从不写它。
//! * `LastUpdated`（`DateTime`，`:52`）——服务器刷新排行榜缓存用，不上线。
//!
//! 因此 Rust 结构体只含 4 个线上字段；服务器侧要用经验值/时间戳时，请在
//! 排行榜模块里另外维护一张 `PlayerId -> (Experience, LastUpdated)` 表，
//! 不要塞回协议层。
//!
//! 唯一载体是 `Rankings` 包（`Shared/ServerPackets.cs:6294`）：先 `byte RankType`、
//! `int MyRank`，然后 `int count` + 逐个本结构，再 `int count` + 一组 `long`，
//! 最后 `int Count`。本结构自身没有包头。

use mir_core::enums::MirClass;

use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `RankCharacterInfo`（`Shared/Data/SharedData.cs:43`；读入 ctor `:58`，`Save` `:66`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RankCharacterInfo {
    /// C# 字段 `PlayerId`：角色编号。
    pub player_id: i64,
    /// C# 字段 `Name`：角色名。
    pub name: String,
    /// C# 字段 `level`：等级。C# 里字段名是小写（首字母未大写），这里保留原意。
    pub level: i32,
    /// C# 字段 `Class`：职业。
    pub class: MirClass,
}

impl RankCharacterInfo {
    /// 解码，见 `Shared/Data/SharedData.cs:58`。
    ///
    /// 只有 4 个字段：`PlayerId`、`Name`、`level`、`Class`。
    /// C# ctor 里没有读 `Experience` / `info` / `LastUpdated`，见模块文档。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        Ok(Self {
            player_id: r.read_i64()?,
            name: r.read_string()?,
            level: r.read_i32()?,
            class: MirClass::from_value(r.read_u8()?).unwrap_or_default(),
        })
    }

    /// 编码，见 `Shared/Data/SharedData.cs:66`。
    ///
    /// 与 ctor 严格对称，多余字段一个字节都不写。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i64(self.player_id);
        w.write_string(&self.name);
        w.write_i32(self.level);
        w.write_u8(self.class.value());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 编码后再解码，断言相等且负载被正好读完。
    fn roundtrip(value: &RankCharacterInfo) {
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = RankCharacterInfo::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, *value, "解码结果应与原值相同");
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn bytes_match_hand_derived_layout() {
        // 按 `SharedData.cs:66` 的写出顺序推导（全部小端）：
        //   i64 PlayerId = 0x0102030405060708 -> 08 07 06 05 04 03 02 01
        //   string Name  = "Mir"：前缀 3      -> 03 4D 69 72
        //   i32 level    = 42                 -> 2A 00 00 00
        //   u8  Class    = MirClass.Archer = 4 -> 04
        // 合计 8 + 4 + 4 + 1 = 17 字节。
        let value = RankCharacterInfo {
            player_id: 0x0102_0304_0506_0708,
            name: "Mir".to_owned(),
            level: 42,
            class: MirClass::Archer,
        };

        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let expected: Vec<u8> = vec![
            0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, // PlayerId
            0x03, 0x4D, 0x69, 0x72, // Name = "Mir"
            0x2A, 0x00, 0x00, 0x00, // level = 42
            0x04, // Class = Archer
        ];

        assert_eq!(bytes.len(), 17);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn server_only_fields_are_not_written() {
        // 回归点：`Experience` / `info` / `LastUpdated` 不在线上。
        // 结构体里没有它们，最小条目固定 14 字节：
        // i64(8) + 空名字的长度前缀(1) + i32(4) + u8(1) = 14。
        // 若哪天有人「顺手」把 `Experience` 也写进去，这里会立刻变成 22。
        let value = RankCharacterInfo {
            player_id: -1,
            name: String::new(),
            level: 0,
            class: MirClass::Warrior,
        };
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        assert_eq!(writer.len(), 14);
        assert_eq!(
            writer.into_inner(),
            vec![
                0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // PlayerId = -1
                0x00, // Name = ""
                0x00, 0x00, 0x00, 0x00, // level = 0
                0x00, // Class = Warrior
            ]
        );
        roundtrip(&value);
    }

    #[test]
    fn roundtrip_with_chinese_name() {
        let value = RankCharacterInfo {
            player_id: i64::MAX,
            name: "传奇老玩家".to_owned(),
            level: 65535,
            class: MirClass::Assassin,
        };
        roundtrip(&value);
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        RankCharacterInfo::default().encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(RankCharacterInfo::decode(&mut reader).is_err());
    }
}
