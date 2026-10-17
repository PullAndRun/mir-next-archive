//! 客户端「传送点」资料 `ClientMovementInfo` 的线格式。
//!
//! 对照 Crystal `Shared/Data/ClientData.cs:238`（读入 ctor `:247`，`Save` `:255`）手写。
//! 它是 `ClientMapInfo.Movements` 的元素，随 `ServerPackets.MapInformation`
//! 一起下发（`Shared/Data/ClientData.cs:326`、`:339`），没有版本参数。
//!
//! 线格式（一行一字段，括号内是宽度）：
//!
//! ```text
//! i32    Destination
//! string Title                <- 7 位长度前缀 + UTF-8
//! i32    Location.X
//! i32    Location.Y
//! i32    Icon
//! ```

use mir_core::Point;

use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `ClientMovementInfo`（`Shared/Data/ClientData.cs:238`；ctor `:247`，`Save` `:255`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientMovementInfo {
    /// 目的地地图编号。
    pub destination: i32,
    /// 传送点的显示名。
    pub title: String,
    /// 传送点坐标（`System.Drawing.Point` 的两个 `int`）。
    pub location: Point,
    /// 图标编号。
    pub icon: i32,
}

impl ClientMovementInfo {
    /// 解码，见 `Shared/Data/ClientData.cs:247`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        Ok(Self {
            destination: r.read_i32()?,
            title: r.read_string()?,
            location: Point {
                x: r.read_i32()?,
                y: r.read_i32()?,
            },
            icon: r.read_i32()?,
        })
    }

    /// 编码，见 `Shared/Data/ClientData.cs:255`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i32(self.destination);
        w.write_string(&self.title);
        w.write_i32(self.location.x);
        w.write_i32(self.location.y);
        w.write_i32(self.icon);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let value = ClientMovementInfo {
            destination: 3,
            title: "比奇省".to_owned(),
            location: Point { x: 330, y: 270 },
            icon: 2,
        };
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = ClientMovementInfo::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, value);
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn bytes_match_hand_derived_layout() {
        // 按 `Shared/Data/ClientData.cs:255-262` 的写出顺序推导（全部小端）：
        //   i32 Destination  = 0x01020304 -> 04 03 02 01
        //   string Title     = "M"        -> 01 4D
        //   i32 Location.X   = 10         -> 0A 00 00 00
        //   i32 Location.Y   = -1         -> FF FF FF FF
        //   i32 Icon         = 7          -> 07 00 00 00
        // 合计 4 + 2 + 8 + 4 = 18 字节。
        let value = ClientMovementInfo {
            destination: 0x0102_0304,
            title: "M".to_owned(),
            location: Point { x: 10, y: -1 },
            icon: 7,
        };

        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let expected: Vec<u8> = vec![
            0x04, 0x03, 0x02, 0x01, // Destination
            0x01, 0x4D, // Title = "M"
            0x0A, 0x00, 0x00, 0x00, // Location.X
            0xFF, 0xFF, 0xFF, 0xFF, // Location.Y
            0x07, 0x00, 0x00, 0x00, // Icon
        ];
        assert_eq!(bytes.len(), 18);
        assert_eq!(bytes, expected);

        let mut reader = PacketReader::new(&bytes);
        assert_eq!(
            ClientMovementInfo::decode(&mut reader).expect("解码应当成功"),
            value
        );
        assert!(reader.is_empty());
    }

    #[test]
    fn truncated_payload_errors() {
        let value = ClientMovementInfo {
            destination: 1,
            title: "castle".to_owned(),
            location: Point { x: 1, y: 2 },
            icon: 3,
        };
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(ClientMovementInfo::decode(&mut reader).is_err());
    }
}
