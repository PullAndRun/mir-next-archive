//! 英雄（英雄系统/副角色）信息 `ClientHeroInformation` 的线格式。
//!
//! 对照 Crystal `Shared/Data/ClientData.cs:635`（读入 ctor `:645`，`Save` `:654`）手写。
//! ctor 与 `Save` 都不带 `version` 参数，没有版本分支。
//!
//! 线格式（一行一字段，全部小端）：
//!
//! ```text
//! i32    Index       英雄在角色列表里的槽位
//! string Name        英雄名（7 位长度前缀 + UTF-8）
//! u16    Level       等级
//! u8     Class       MirClass
//! u8     Gender      MirGender
//! ```
//!
//! 与 `SelectInfo`（`SharedData.cs:3`）相比少了 `LastAccess`，其余字段同名同宽；
//! C# 的 `ToString`（`:663`）只是界面文本，不在线上。

use mir_core::enums::{MirClass, MirGender};

use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `ClientHeroInformation`（`Shared/Data/ClientData.cs:635`；读入 ctor `:645`，`Save` `:654`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientHeroInformation {
    /// C# 字段 `Index`：英雄槽位序号。
    pub index: i32,
    /// C# 字段 `Name`：英雄名。
    pub name: String,
    /// C# 字段 `Level`：等级（`ushort`，不是 `int`）。
    pub level: u16,
    /// C# 字段 `Class`：职业。
    pub class: MirClass,
    /// C# 字段 `Gender`：性别。
    pub gender: MirGender,
}

impl ClientHeroInformation {
    /// 解码，见 `Shared/Data/ClientData.cs:645`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        Ok(Self {
            index: r.read_i32()?,
            name: r.read_string()?,
            level: r.read_u16()?,
            class: MirClass::from_value(r.read_u8()?).unwrap_or_default(),
            gender: MirGender::from_value(r.read_u8()?).unwrap_or_default(),
        })
    }

    /// 编码，见 `Shared/Data/ClientData.cs:654`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i32(self.index);
        w.write_string(&self.name);
        w.write_u16(self.level);
        w.write_u8(self.class.value());
        w.write_u8(self.gender.value());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 编码后再解码，断言相等且负载被正好读完。
    fn roundtrip(value: &ClientHeroInformation) {
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = ClientHeroInformation::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, *value, "解码结果应与原值相同");
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn bytes_match_hand_derived_layout() {
        // 按 `ClientData.cs:654` 的写出顺序推导（全部小端）：
        //   i32    Index  = 0x0A0B0C0D       -> 0D 0C 0B 0A
        //   string Name   = "Mir"：长度前缀 3 -> 03，随后 4D 69 72
        //   u16    Level  = 0x0203           -> 03 02
        //   u8     Class  = MirClass.Taoist = 2 -> 02
        //   u8     Gender = MirGender.Female = 1 -> 01
        // 合计 4 + 1 + 3 + 2 + 1 + 1 = 12 字节。
        let value = ClientHeroInformation {
            index: 0x0A0B_0C0D,
            name: "Mir".to_owned(),
            level: 0x0203,
            class: MirClass::Taoist,
            gender: MirGender::Female,
        };

        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let expected: Vec<u8> = vec![
            0x0D, 0x0C, 0x0B, 0x0A, // Index
            0x03, 0x4D, 0x69, 0x72, // Name = "Mir"
            0x03, 0x02, // Level = 0x0203
            0x02, // Class = Taoist
            0x01, // Gender = Female
        ];

        assert_eq!(bytes.len(), 12);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn roundtrip_with_chinese_name() {
        let value = ClientHeroInformation {
            index: -1,
            name: "英雄".to_owned(),
            level: 65535,
            class: MirClass::Archer,
            gender: MirGender::Male,
        };
        roundtrip(&value);
    }

    #[test]
    fn empty_name_costs_one_byte() {
        let value = ClientHeroInformation::default();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        assert_eq!(
            writer.into_inner(),
            vec![
                0x00, 0x00, 0x00, 0x00, // Index
                0x00, // Name = ""
                0x00, 0x00, // Level
                0x00, // Class = Warrior
                0x00, // Gender = Male
            ]
        );
        roundtrip(&value);
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        ClientHeroInformation::default().encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(ClientHeroInformation::decode(&mut reader).is_err());
    }
}
