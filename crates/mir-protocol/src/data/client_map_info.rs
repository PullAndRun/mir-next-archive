//! 地图资料 `ClientMapInfo` 的线格式。
//!
//! 对照 Crystal `Shared/Data/ClientData.cs:320`（读入 ctor `:331`，`Save` `:345`）手写。
//! 它是服务器包 `NewMapInfo`（`Shared/ServerPackets.cs:519`）的负载主体：
//! 该包先写 `i32 MapIndex`（`:531`/`:537`），紧接着就是本结构（`:532`/`:538`）。
//! ctor / `Save` 都不带 `version` 参数，因此不存在需要按默认版本展开的分支。
//!
//! 线格式（全部小端）：
//!
//! ```text
//! string              Title                 7 位长度前缀 + UTF-8
//! i32                 Width
//! i32                 Height
//! i32                 BigMap                大地图图片序号
//! i32                 Movements.Count
//! ClientMovementInfo  Movements[i]          i32 Destination + string Title + i32 X + i32 Y + i32 Icon
//! i32                 NPCs.Count
//! ClientNPCInfo       NPCs[i]               13 个字段，见 `client_npc_info.rs`
//! ```
//!
//! **注意 `Title` 在最前**：C# 的字段声明是 `Width, Height, BigMap, Title, ...`（`:322-325`），
//! 但 ctor（`:333`）与 `Save`（`:347`）都是先写 `Title`，本结构体按线格式排列。

use crate::data::{ClientMovementInfo, ClientNPCInfo};
use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `ClientMapInfo`（`Shared/Data/ClientData.cs:320`；读入 ctor `:331`，`Save` `:345`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientMapInfo {
    /// 地图名。
    pub title: String,
    /// 地图宽度（格数）。
    pub width: i32,
    /// 地图高度（格数）。
    pub height: i32,
    /// 大地图图片序号。
    pub big_map: i32,
    /// 地图上的传送点。
    pub movements: Vec<ClientMovementInfo>,
    /// 地图上的 NPC。
    pub npcs: Vec<ClientNPCInfo>,
}

impl ClientMapInfo {
    /// 解码，见 `Shared/Data/ClientData.cs:331`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let title = r.read_string()?;
        let width = r.read_i32()?;
        let height = r.read_i32()?;
        let big_map = r.read_i32()?;

        let movement_count = r.read_count()?;
        // 不预分配：数量来自不可信输入，逐项解析到 EOF 会自然报错。
        let mut movements = Vec::new();
        for _ in 0..movement_count {
            movements.push(ClientMovementInfo::decode(r)?);
        }

        let npc_count = r.read_count()?;
        let mut npcs = Vec::new();
        for _ in 0..npc_count {
            npcs.push(ClientNPCInfo::decode(r)?);
        }

        Ok(Self {
            title,
            width,
            height,
            big_map,
            movements,
            npcs,
        })
    }

    /// 编码，见 `Shared/Data/ClientData.cs:345`。
    ///
    /// `NPCs` 元素沿用 `ClientNPCInfo` 的 `Icon` 回填语义（`ClientData.cs:298-299`）：
    /// C# 的 `Save` 写的也是回填后的 `Icon`（`:315`），行为与解码侧一致。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_string(&self.title);
        w.write_i32(self.width);
        w.write_i32(self.height);
        w.write_i32(self.big_map);

        w.write_count(self.movements.len());
        for movement in &self.movements {
            movement.encode(w);
        }

        w.write_count(self.npcs.len());
        for npc in &self.npcs {
            npc.encode(w);
        }
    }
}

#[cfg(test)]
mod tests {
    use mir_core::Point;

    use super::*;

    /// 一个传送点样本（18 字节）。
    fn movement() -> ClientMovementInfo {
        ClientMovementInfo {
            destination: 0x0102_0304,
            title: "M".to_owned(),
            location: Point { x: 10, y: -1 },
            icon: 7,
        }
    }

    /// 一个 NPC 样本（36 字节，两个字符串取空串以便手算偏移）。
    ///
    /// `icon` / `big_map_icon` 取 5 / 0，刻意**不触发** `Icon = BigMapIcon` 回填，
    /// 这样「编码 → 解码」的字节等价才成立。
    fn npc() -> ClientNPCInfo {
        ClientNPCInfo {
            index: 12,
            file_name: String::new(),
            name: String::new(),
            map_index: 3,
            location: Point { x: 330, y: 270 },
            image: 45,
            rate: 10,
            show_on_big_map: true,
            big_map_icon: 0,
            object_id: 0xAABB_CCDD,
            icon: 5,
            can_teleport_to: false,
        }
    }

    /// 字段取值都互不相同、且不触发 NPC 回填的样本。
    fn sample() -> ClientMapInfo {
        ClientMapInfo {
            title: "比奇省".to_owned(),
            width: 1000,
            height: 600,
            big_map: 3,
            movements: vec![movement()],
            npcs: vec![npc()],
        }
    }

    #[test]
    fn roundtrip() {
        let value = sample();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = ClientMapInfo::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, value);
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn bytes_match_hand_derived_layout() {
        // 按 `Shared/Data/ClientData.cs:345-357` 的写出顺序推导（全部小端）：
        //   string Title = "比奇省"：UTF-8 9 字节 -> 前缀 0x09
        //                  比 = E6 AF 94，奇 = E5 A5 87，省 = E7 9C 81       (10 字节)
        //   i32 Width     = 1000 = 0x3E8 -> E8 03 00 00                       (4)
        //   i32 Height    = 600  = 0x258 -> 58 02 00 00                       (4)
        //   i32 BigMap    = 3            -> 03 00 00 00                       (4)
        //   i32 Mov.Count = 1            -> 01 00 00 00                       (4)
        //   传送点（`ClientData.cs:255`，见 client_movement_info.rs）：       (18)
        //     i32 Destination = 0x01020304 -> 04 03 02 01
        //     string Title    = "M"        -> 01 4D
        //     i32 X = 10                   -> 0A 00 00 00
        //     i32 Y = -1                   -> FF FF FF FF
        //     i32 Icon = 7                 -> 07 00 00 00
        //   i32 NPC.Count = 1            -> 01 00 00 00                       (4)
        //   NPC（`ClientData.cs:302`，见 client_npc_info.rs）：               (36)
        //     i32 Index = 12 -> 0C 00 00 00
        //     string FileName = "" -> 00
        //     string Name = ""     -> 00
        //     i32 MapIndex = 3 -> 03 00 00 00
        //     i32 X = 330 -> 4A 01 00 00
        //     i32 Y = 270 -> 0E 01 00 00
        //     u16 Image = 45 -> 2D 00
        //     u16 Rate = 10  -> 0A 00
        //     bool ShowOnBigMap = true -> 01
        //     i32 BigMapIcon = 0 -> 00 00 00 00
        //     u32 ObjectID = 0xAABBCCDD -> DD CC BB AA
        //     i32 Icon = 5 -> 05 00 00 00
        //     bool CanTeleportTo = false -> 00
        // 合计 10 + 4 + 4 + 4 + 4 + 18 + 4 + 36 = 84 字节。
        let mut writer = PacketWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();

        let expected: Vec<u8> = vec![
            0x09, 0xE6, 0xAF, 0x94, 0xE5, 0xA5, 0x87, 0xE7, 0x9C, 0x81, // Title = "比奇省"
            0xE8, 0x03, 0x00, 0x00, // Width = 1000
            0x58, 0x02, 0x00, 0x00, // Height = 600
            0x03, 0x00, 0x00, 0x00, // BigMap = 3
            0x01, 0x00, 0x00, 0x00, // Movements.Count = 1
            0x04, 0x03, 0x02, 0x01, //   Destination
            0x01, 0x4D, //   Title = "M"
            0x0A, 0x00, 0x00, 0x00, //   Location.X = 10
            0xFF, 0xFF, 0xFF, 0xFF, //   Location.Y = -1
            0x07, 0x00, 0x00, 0x00, //   Icon = 7
            0x01, 0x00, 0x00, 0x00, // NPCs.Count = 1
            0x0C, 0x00, 0x00, 0x00, //   Index = 12
            0x00, //   FileName = ""
            0x00, //   Name = ""
            0x03, 0x00, 0x00, 0x00, //   MapIndex = 3
            0x4A, 0x01, 0x00, 0x00, //   Location.X = 330
            0x0E, 0x01, 0x00, 0x00, //   Location.Y = 270
            0x2D, 0x00, //   Image = 45
            0x0A, 0x00, //   Rate = 10
            0x01, //   ShowOnBigMap = true
            0x00, 0x00, 0x00, 0x00, //   BigMapIcon = 0
            0xDD, 0xCC, 0xBB, 0xAA, //   ObjectID = 0xAABBCCDD
            0x05, 0x00, 0x00, 0x00, //   Icon = 5
            0x00, //   CanTeleportTo = false
        ];

        assert_eq!(bytes.len(), 84);
        assert_eq!(bytes, expected);

        let mut reader = PacketReader::new(&bytes);
        assert_eq!(
            ClientMapInfo::decode(&mut reader).expect("解码应当成功"),
            sample()
        );
        assert!(reader.is_empty());
    }

    #[test]
    fn title_is_written_before_dimensions() {
        // C# 声明顺序是 Width/Height/BigMap/Title，但 `Save`（`:347-350`）先写 Title。
        // 这里用最小样本把这一「声明顺序 ≠ 线序」的差异钉死：
        //   01 4D          -> Title = "M"
        //   04 03 02 01    -> Width = 0x01020304
        //   00 00 00 00    -> Height = 0
        //   00 00 00 00    -> BigMap = 0
        //   00 00 00 00    -> Movements.Count = 0
        //   00 00 00 00    -> NPCs.Count = 0
        // 合计 2 + 4 × 5 = 22 字节。
        let value = ClientMapInfo {
            title: "M".to_owned(),
            width: 0x0102_0304,
            ..ClientMapInfo::default()
        };
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        assert_eq!(
            bytes,
            vec![
                0x01, 0x4D, // Title
                0x04, 0x03, 0x02, 0x01, // Width
                0x00, 0x00, 0x00, 0x00, // Height
                0x00, 0x00, 0x00, 0x00, // BigMap
                0x00, 0x00, 0x00, 0x00, // Movements.Count
                0x00, 0x00, 0x00, 0x00, // NPCs.Count
            ]
        );

        let mut reader = PacketReader::new(&bytes);
        assert_eq!(
            ClientMapInfo::decode(&mut reader).expect("解码应当成功"),
            value
        );
        assert!(reader.is_empty());
    }

    #[test]
    fn empty_map_info_is_twenty_one_bytes() {
        // 默认值：空 Title 占 1 字节 + 3 个 i32 尺寸 + 两个列表计数，共 21 字节。
        let value = ClientMapInfo::default();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        assert_eq!(bytes.len(), 21);
        assert_eq!(bytes, vec![0x00; 21]);

        let mut reader = PacketReader::new(&bytes);
        assert_eq!(
            ClientMapInfo::decode(&mut reader).expect("解码应当成功"),
            value
        );
        assert!(reader.is_empty());
    }

    #[test]
    fn npc_icon_normalization_is_kept() {
        // `ClientData.cs:298-299` 的回填发生在 `ClientNPCInfo` 解码阶段，
        // `ClientMapInfo` 只是把结果原样装进列表：线上 Icon = 0、BigMapIcon = 42 时，
        // 解出来的 Icon 变成 42，因此「原字节 → 解码 → 编码」不再等于原字节
        // （C# 的 `Save` 同样写回填后的值，故行为一致，不是移植偏差）。
        let value = ClientMapInfo {
            title: "Dungeon".to_owned(),
            npcs: vec![ClientNPCInfo {
                icon: 0,
                big_map_icon: 42,
                ..ClientNPCInfo::default()
            }],
            ..ClientMapInfo::default()
        };

        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = ClientMapInfo::decode(&mut reader).expect("解码应当成功");
        assert!(reader.is_empty());
        assert_eq!(decoded.npcs.len(), 1);
        assert_eq!(decoded.npcs[0].icon, 42, "解码后 Icon 被 BigMapIcon 回填");
        assert_ne!(decoded, value, "回填使解码结果与原值不同");

        // 回填后的值再编码 → 再解码是稳定的。
        let mut writer = PacketWriter::new();
        decoded.encode(&mut writer);
        let reencoded = writer.into_inner();
        assert_ne!(bytes, reencoded, "再编码写出的是回填后的 Icon");
        let mut reader = PacketReader::new(&reencoded);
        assert_eq!(
            ClientMapInfo::decode(&mut reader).expect("解码应当成功"),
            decoded
        );
    }

    #[test]
    fn negative_counts_error() {
        // 两个列表计数都是 i32；负数在 Crystal 里会被 `for` 循环静默跳过，
        // Rust 侧视为损坏数据。先构造 Movements.Count = -1 的负载。
        let mut bytes = vec![0x00]; // Title = ""
        bytes.extend_from_slice(&0i32.to_le_bytes()); // Width
        bytes.extend_from_slice(&0i32.to_le_bytes()); // Height
        bytes.extend_from_slice(&0i32.to_le_bytes()); // BigMap
        bytes.extend_from_slice(&(-1i32).to_le_bytes()); // Movements.Count = -1
        let mut reader = PacketReader::new(&bytes);
        assert!(ClientMapInfo::decode(&mut reader).is_err());

        // NPCs.Count = -1 同样报错。
        let mut bytes = vec![0x00];
        bytes.extend_from_slice(&0i32.to_le_bytes()); // Width
        bytes.extend_from_slice(&0i32.to_le_bytes()); // Height
        bytes.extend_from_slice(&0i32.to_le_bytes()); // BigMap
        bytes.extend_from_slice(&0i32.to_le_bytes()); // Movements.Count = 0
        bytes.extend_from_slice(&(-1i32).to_le_bytes()); // NPCs.Count = -1
        let mut reader = PacketReader::new(&bytes);
        assert!(ClientMapInfo::decode(&mut reader).is_err());
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();
        // 去掉最后一个字节，NPC 的 CanTeleportTo 读不满。
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(ClientMapInfo::decode(&mut reader).is_err());
    }
}
