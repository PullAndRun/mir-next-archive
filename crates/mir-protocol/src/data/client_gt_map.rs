//! 攻城/行会属地地图 `ClientGTMap` 的线格式。
//!
//! 对照 Crystal `Shared/Data/SharedData.cs:139`（读入 ctor `:155`，`Save` `:167`）手写。
//! 它出现在服务器包 `ClientGTMap`（城堡属地列表）里，ctor / `Save` 都不带 `version`
//! 参数，因此不存在需要按默认版本展开的分支。
//!
//! 线格式（一行一字段，全部小端）：
//!
//! ```text
//! i32    index     地图序号
//! string Name      地图名（7 位长度前缀 + UTF-8）
//! string Owner     当前占领行会名
//! string Leader    会长名
//! string Leader2   副会长名
//! i32    price     竞拍所需金币
//! i32    days      剩余天数
//! i32    begin     起拍时间（C# 里是 `DateTime`，线上按原始 `int` 传输）
//! ```

use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `ClientGTMap`（`Shared/Data/SharedData.cs:139`；读入 ctor `:155`，`Save` `:167`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientGTMap {
    /// 地图序号。
    pub index: i32,
    /// 地图名。
    pub name: String,
    /// 当前占领行会名。
    pub owner: String,
    /// 会长名。
    pub leader: String,
    /// 副会长名。
    pub leader2: String,
    /// 竞拍所需金币。
    pub price: i32,
    /// 剩余天数。
    pub days: i32,
    /// 起拍时间（按原始 `i32` 保真传输）。
    pub begin: i32,
}

impl ClientGTMap {
    /// 解码，见 `Shared/Data/SharedData.cs:155`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        Ok(Self {
            index: r.read_i32()?,
            name: r.read_string()?,
            owner: r.read_string()?,
            leader: r.read_string()?,
            leader2: r.read_string()?,
            price: r.read_i32()?,
            days: r.read_i32()?,
            begin: r.read_i32()?,
        })
    }

    /// 编码，见 `Shared/Data/SharedData.cs:167`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i32(self.index);
        w.write_string(&self.name);
        w.write_string(&self.owner);
        w.write_string(&self.leader);
        w.write_string(&self.leader2);
        w.write_i32(self.price);
        w.write_i32(self.days);
        w.write_i32(self.begin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个字段取值都互不相同的样本。
    fn sample() -> ClientGTMap {
        ClientGTMap {
            index: 7,
            name: "沙巴克".to_owned(),
            owner: "Guild".to_owned(),
            leader: "L1".to_owned(),
            leader2: String::new(),
            price: 1000,
            days: 30,
            begin: -1,
        }
    }

    #[test]
    fn roundtrip() {
        let value = sample();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = ClientGTMap::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, value);
        assert!(reader.is_empty(), "应正好读完负载");
    }

    #[test]
    fn bytes_match_hand_derived_layout() {
        // 按 `Shared/Data/SharedData.cs:167` 的写出顺序推导（全部小端）：
        //   i32 index   = 7 -> 07 00 00 00
        //   string Name = "沙巴克"：UTF-8 9 字节 -> 前缀 0x09
        //                 沙 = E6 B2 99，巴 = E5 B7 B4，克 = E5 85 8B
        //   string Owner   = "Guild" -> 前缀 0x05 + 47 75 69 6C 64
        //   string Leader  = "L1"    -> 前缀 0x02 + 4C 31
        //   string Leader2 = ""      -> 前缀 0x00
        //   i32 price = 1000     -> E8 03 00 00
        //   i32 days  = 30       -> 1E 00 00 00
        //   i32 begin = -1       -> FF FF FF FF
        // 合计 4 + 10 + 6 + 3 + 1 + 4 + 4 + 4 = 36 字节。
        let mut writer = PacketWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();

        let expected: Vec<u8> = vec![
            0x07, 0x00, 0x00, 0x00, // index
            0x09, 0xE6, 0xB2, 0x99, 0xE5, 0xB7, 0xB4, 0xE5, 0x85, 0x8B, // Name = "沙巴克"
            0x05, 0x47, 0x75, 0x69, 0x6C, 0x64, // Owner = "Guild"
            0x02, 0x4C, 0x31, // Leader = "L1"
            0x00, // Leader2 = ""
            0xE8, 0x03, 0x00, 0x00, // price = 1000
            0x1E, 0x00, 0x00, 0x00, // days = 30
            0xFF, 0xFF, 0xFF, 0xFF, // begin = -1
        ];

        assert_eq!(bytes.len(), 36);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn all_strings_empty() {
        // 四个字符串全空时各占 1 字节，整体 = 4 + 4×1 + 4×3 = 20 字节。
        let value = ClientGTMap::default();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        assert_eq!(bytes.len(), 20);
        // 四个空串的长度前缀紧接着 index。
        assert_eq!(&bytes[4..8], &[0x00, 0x00, 0x00, 0x00]);

        let mut reader = PacketReader::new(&bytes);
        assert_eq!(
            ClientGTMap::decode(&mut reader).expect("解码应当成功"),
            value
        );
        assert!(reader.is_empty());
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();
        // 去掉最后一个字节，begin 读不满。
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(ClientGTMap::decode(&mut reader).is_err());
    }
}
