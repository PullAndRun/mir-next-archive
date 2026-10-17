//! 大地图图标 `WorldMapIcon` 的线格式。
//!
//! 对照 Crystal `Shared/Data/SharedData.cs:118`（读入 ctor `:125`，`Save` `:132`）手写。
//! 它只作为 `WorldMapSetup.Icons` 的元素出现，没有独立包头；服务器与客户端共用同一套
//! 读写代码，ctor / `Save` 都不带 `version` 参数，因此不存在需要按默认版本展开的分支。
//!
//! 线格式（一行一字段，全部小端）：
//!
//! ```text
//! i32    ImageIndex   图标图片序号
//! string Title       图标标题（7 位长度前缀 + UTF-8）
//! i32    MapIndex    图标指向的地图序号
//! ```

use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `WorldMapIcon`（`Shared/Data/SharedData.cs:118`；读入 ctor `:125`，`Save` `:132`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorldMapIcon {
    /// 图标图片序号。
    pub image_index: i32,
    /// 图标标题。
    pub title: String,
    /// 图标指向的地图序号。
    pub map_index: i32,
}

impl WorldMapIcon {
    /// 解码，见 `Shared/Data/SharedData.cs:125`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        Ok(Self {
            image_index: r.read_i32()?,
            title: r.read_string()?,
            map_index: r.read_i32()?,
        })
    }

    /// 编码，见 `Shared/Data/SharedData.cs:132`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i32(self.image_index);
        w.write_string(&self.title);
        w.write_i32(self.map_index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个字段取值都互不相同的样本。
    fn sample() -> WorldMapIcon {
        WorldMapIcon {
            image_index: 0x0102_0304,
            title: "传奇地图".to_owned(),
            map_index: 0x0A0B_0C0D,
        }
    }

    #[test]
    fn roundtrip() {
        let value = sample();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = WorldMapIcon::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, value);
        assert!(reader.is_empty(), "应正好读完负载");
    }

    #[test]
    fn bytes_match_hand_derived_layout() {
        // 按 `Shared/Data/SharedData.cs:132` 的写出顺序推导（全部小端）：
        //   i32 ImageIndex = 0x01020304 -> 04 03 02 01
        //   string Title   = "传奇地图"：UTF-8 每个汉字 3 字节，共 12 字节
        //                    传 = E4 BC A0，奇 = E5 A5 87，地 = E5 9C B0，图 = E5 9B BE
        //                    长度前缀 12 -> 0x0C
        //   i32 MapIndex   = 0x0A0B0C0D -> 0D 0C 0B 0A
        // 合计 4 + 1 + 12 + 4 = 21 字节。
        let mut writer = PacketWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();

        let expected: Vec<u8> = vec![
            0x04, 0x03, 0x02, 0x01, // ImageIndex
            0x0C, // Title 的 7 位长度前缀 = 12
            0xE4, 0xBC, 0xA0, // 传
            0xE5, 0xA5, 0x87, // 奇
            0xE5, 0x9C, 0xB0, // 地
            0xE5, 0x9B, 0xBE, // 图
            0x0D, 0x0C, 0x0B, 0x0A, // MapIndex
        ];

        assert_eq!(bytes.len(), 21);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn empty_title_costs_one_byte() {
        // C# 的 `Title` 没有初值，可能是 null；但 `BinaryWriter.Write(null)` 会抛异常，
        // 所以线上数据里只会出现非 null 字符串。空串在 Rust 侧写成单个 0x00 前缀。
        let value = WorldMapIcon {
            image_index: -1,
            title: String::new(),
            map_index: 7,
        };
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        assert_eq!(
            bytes,
            vec![
                0xFF, 0xFF, 0xFF, 0xFF, // ImageIndex = -1
                0x00, // Title = ""
                0x07, 0x00, 0x00, 0x00, // MapIndex = 7
            ]
        );

        let mut reader = PacketReader::new(&bytes);
        assert_eq!(
            WorldMapIcon::decode(&mut reader).expect("解码应当成功"),
            value
        );
        assert!(reader.is_empty());
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();
        // 去掉最后一个字节，MapIndex 读不满。
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(WorldMapIcon::decode(&mut reader).is_err());
    }
}
