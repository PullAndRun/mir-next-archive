//! 角色选择界面的记录 `SelectInfo` 的线格式。
//!
//! 对照 Crystal `Shared/Data/SharedData.cs:3`（读入 ctor `:14`，`Save` `:23`）手写。
//! 该结构出现在 `ServerPackets.LoginSuccess` / `LoginBanned` 等包的 `Characters` 列表里，
//! 服务器与客户端共用同一套序列化，没有版本参数。
//!
//! 线格式（一行一字段，括号内是宽度）：
//!
//! ```text
//! i32   Index        角色槽位序号
//! string Name        角色名（7 位长度前缀 + UTF-8）
//! u16   Level        等级
//! u8    Class        MirClass
//! u8    Gender       MirGender
//! i64   LastAccess   DateTime.ToBinary()
//! ```

use mir_core::DateTimeBinary;
use mir_core::enums::{MirClass, MirGender};

use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `SelectInfo`（`Shared/Data/SharedData.cs:3`；读入 ctor `:14`，`Save` `:23`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectInfo {
    /// 角色槽位序号。
    pub index: i32,
    /// 角色名。
    pub name: String,
    /// 等级（`ushort`，不是 `int`）。
    pub level: u16,
    /// 职业。
    pub class: MirClass,
    /// 性别。
    pub gender: MirGender,
    /// 最近一次登录时间。
    pub last_access: DateTimeBinary,
}

impl SelectInfo {
    /// 解码，见 `Shared/Data/SharedData.cs:14`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        Ok(Self {
            index: r.read_i32()?,
            name: r.read_string()?,
            level: r.read_u16()?,
            class: MirClass::from_value(r.read_u8()?).unwrap_or_default(),
            gender: MirGender::from_value(r.read_u8()?).unwrap_or_default(),
            last_access: DateTimeBinary(r.read_i64()?),
        })
    }

    /// 编码，见 `Shared/Data/SharedData.cs:23`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i32(self.index);
        w.write_string(&self.name);
        w.write_u16(self.level);
        w.write_u8(self.class.value());
        w.write_u8(self.gender.value());
        w.write_i64(self.last_access.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个字段取值都互不相同的样本。
    fn sample() -> SelectInfo {
        SelectInfo {
            index: 0x0102_0304,
            name: "传奇".to_owned(),
            level: 300,
            class: MirClass::Taoist,
            gender: MirGender::Female,
            last_access: DateTimeBinary(0x0A0B_0C0D_0E0F_1011),
        }
    }

    #[test]
    fn roundtrip() {
        let value = sample();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = SelectInfo::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, value);
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn bytes_match_hand_derived_layout() {
        // 按 `Shared/Data/SharedData.cs:23` 的写出顺序推导（全部小端）：
        //   i32 Index      = 0x01020304 -> 04 03 02 01
        //   string Name    = "传奇"：UTF-8 长度 6 -> 前缀 0x06，再 6 字节
        //                    传 = E4 BC A0，奇 = E5 A5 87
        //   u16 Level      = 300 -> 2C 01
        //   u8  Class      = Taoist(2) -> 02
        //   u8  Gender     = Female(1) -> 01
        //   i64 LastAccess = 0x0A0B0C0D0E0F1011 -> 11 10 0F 0E 0D 0C 0B 0A
        // 合计 4+1+6+2+1+1+8 = 23 字节。
        let mut writer = PacketWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();

        let expected: Vec<u8> = vec![
            0x04, 0x03, 0x02, 0x01, // Index
            0x06, 0xE4, 0xBC, 0xA0, 0xE5, 0xA5, 0x87, // Name = "传奇"
            0x2C, 0x01, // Level
            0x02, // Class
            0x01, // Gender
            0x11, 0x10, 0x0F, 0x0E, 0x0D, 0x0C, 0x0B, 0x0A, // LastAccess
        ];

        assert_eq!(bytes.len(), 23);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn empty_name_and_unknown_enum_value() {
        // 空名字 -> 单字节 0x00 前缀；未知的职业字节回退到默认值 Warrior。
        let bytes: Vec<u8> = vec![
            0xFF, 0xFF, 0xFF, 0xFF, // Index = -1
            0x00, // Name = ""
            0x01, 0x00, // Level = 1
            0x63, // Class = 99（未知）
            0x00, // Gender = Male
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // LastAccess = 0
        ];
        let mut reader = PacketReader::new(&bytes);
        let decoded = SelectInfo::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded.index, -1);
        assert_eq!(decoded.name, "");
        assert_eq!(decoded.level, 1);
        assert_eq!(decoded.class, MirClass::default());
        assert_eq!(decoded.gender, MirGender::Male);
        assert!(decoded.last_access.is_unset());
        assert!(reader.is_empty());
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(SelectInfo::decode(&mut reader).is_err());
    }
}
