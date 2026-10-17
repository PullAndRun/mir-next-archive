//! 客户端 NPC 资料 `ClientNPCInfo` 的线格式。
//!
//! 对照 Crystal `Shared/Data/ClientData.cs:265`（读入 ctor `:283`，`Save` `:302`）手写。
//! 它由 `ServerPackets.NewNPCInfo`（`Shared/ServerPackets.cs:6810-6829`）整块推送，
//! 也作为 `ClientMapInfo.NPCs` 的元素出现在 `MapInformation` 包里。
//!
//! 线格式（一行一字段，括号内是宽度）：
//!
//! ```text
//! i32    Index
//! string FileName             <- `FileName ?? string.Empty`
//! string Name                 <- `Name ?? string.Empty`
//! i32    MapIndex
//! i32    Location.X
//! i32    Location.Y
//! u16    Image                <- 注意：Image 在 Rate 之前
//! u16    Rate
//! bool   ShowOnBigMap
//! i32    BigMapIcon
//! u32    ObjectID
//! i32    Icon
//! bool   CanTeleportTo
//! ```
//!
//! **注意 `Image` 与 `Rate` 的顺序**：C# 的字段声明是 `Rate` 在前、`Image` 在后
//!（`ClientData.cs:275-276`），但 ctor 与 `Save` 都是先 `Image` 再 `Rate`
//!（`:290-291`、`:310-311`），本结构体按线格式排列。

use mir_core::Point;

use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `ClientNPCInfo`（`Shared/Data/ClientData.cs:265`；ctor `:283`，`Save` `:302`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientNPCInfo {
    /// NPC 模板编号。
    pub index: i32,
    /// 脚本文件名（不带扩展名）。
    pub file_name: String,
    /// NPC 显示名。
    pub name: String,
    /// 所在地图编号。
    pub map_index: i32,
    /// 坐标。
    pub location: Point,
    /// 外观编号。
    pub image: u16,
    /// 出现概率（`ushort`）。
    pub rate: u16,
    /// 是否显示在大地图上。
    pub show_on_big_map: bool,
    /// 大地图图标编号。
    pub big_map_icon: i32,
    /// 运行时对象 ID。
    pub object_id: u32,
    /// 提示框图标编号。
    pub icon: i32,
    /// 是否可通过大地图直接传送过去。
    pub can_teleport_to: bool,
}

impl ClientNPCInfo {
    /// 解码，见 `Shared/Data/ClientData.cs:283`。
    ///
    /// **读入副作用**：ctor 末尾会做 `if (Icon == 0 && BigMapIcon != 0) Icon = BigMapIcon;`
    ///（`ClientData.cs:298-299`），把 `Icon` 回填成大地图图标。这里逐字保留，
    /// 因为客户端正是依赖这个回填值渲染提示框。
    ///
    /// 由此带来一个与 Crystal 完全相同的**不对称**：若线格式里 `Icon == 0` 且
    /// `BigMapIcon != 0`，解码后再编码会写出被回填的 `Icon`，与原始字节不同
    /// ——C# 里 `Save` 写的也是回填后的 `Icon`（`:315`），所以行为一致，不是移植偏差。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let index = r.read_i32()?;
        let file_name = r.read_string()?;
        let name = r.read_string()?;
        let map_index = r.read_i32()?;
        let location = Point {
            x: r.read_i32()?,
            y: r.read_i32()?,
        };
        let image = r.read_u16()?;
        let rate = r.read_u16()?;
        let show_on_big_map = r.read_bool()?;
        let big_map_icon = r.read_i32()?;
        let object_id = r.read_u32()?;
        let mut icon = r.read_i32()?;
        let can_teleport_to = r.read_bool()?;

        // `ClientData.cs:298-299`：ctor 的显示用回填，不在线格式里。
        if icon == 0 && big_map_icon != 0 {
            icon = big_map_icon;
        }

        Ok(Self {
            index,
            file_name,
            name,
            map_index,
            location,
            image,
            rate,
            show_on_big_map,
            big_map_icon,
            object_id,
            icon,
            can_teleport_to,
        })
    }

    /// 编码，见 `Shared/Data/ClientData.cs:302`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i32(self.index);
        // C# 写的是 `FileName ?? string.Empty`、`Name ?? string.Empty`；Rust 侧恒为非 null。
        w.write_string(&self.file_name);
        w.write_string(&self.name);
        w.write_i32(self.map_index);
        w.write_i32(self.location.x);
        w.write_i32(self.location.y);
        w.write_u16(self.image);
        w.write_u16(self.rate);
        w.write_bool(self.show_on_big_map);
        w.write_i32(self.big_map_icon);
        w.write_u32(self.object_id);
        w.write_i32(self.icon);
        w.write_bool(self.can_teleport_to);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ClientNPCInfo {
        ClientNPCInfo {
            index: 12,
            file_name: "比奇老兵".to_owned(),
            name: "老兵".to_owned(),
            map_index: 3,
            location: Point { x: 330, y: 270 },
            image: 45,
            rate: 10,
            show_on_big_map: true,
            big_map_icon: 6,
            object_id: 0xAABB_CCDD,
            icon: 7,
            can_teleport_to: true,
        }
    }

    #[test]
    fn roundtrip() {
        let value = sample();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = ClientNPCInfo::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, value);
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn image_precedes_rate() {
        // 按 `ClientData.cs:302-317` 推导偏移（FileName/Name 取空串）：
        //   i32 Index(4) + 两个长度前缀(2) + i32 MapIndex(4) + i32 X/Y(8) = 18
        // 于是 Image 在 18..20、Rate 在 20..22。
        let value = ClientNPCInfo {
            image: 0x0102,
            rate: 0x0304,
            ..ClientNPCInfo::default()
        };

        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        assert_eq!(&bytes[18..20], &[0x02, 0x01], "Image 在前");
        assert_eq!(&bytes[20..22], &[0x04, 0x03], "Rate 在后");
    }

    #[test]
    fn icon_falls_back_to_big_map_icon_on_decode() {
        // `ClientData.cs:298-299`：Icon 为 0 时用 BigMapIcon 回填。
        let value = ClientNPCInfo {
            icon: 0,
            big_map_icon: 42,
            ..ClientNPCInfo::default()
        };

        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = ClientNPCInfo::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded.icon, 42, "解码后 Icon 被回填");
        assert_eq!(decoded.big_map_icon, 42);

        // 再编码时写出的是回填值，与 C# `Save` 的行为一致。
        let mut writer = PacketWriter::new();
        decoded.encode(&mut writer);
        let reencoded = writer.into_inner();
        assert_ne!(bytes, reencoded);
        let mut reader = PacketReader::new(&reencoded);
        assert_eq!(
            ClientNPCInfo::decode(&mut reader).expect("解码应当成功"),
            decoded
        );
    }

    #[test]
    fn zero_icon_and_zero_big_map_icon_stay_zero() {
        let value = ClientNPCInfo::default();
        assert_eq!(value.icon, 0);
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes);
        assert_eq!(
            ClientNPCInfo::decode(&mut reader).expect("解码应当成功"),
            value
        );
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(ClientNPCInfo::decode(&mut reader).is_err());
    }
}
