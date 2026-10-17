//! 物品出租记录 `ItemRentalInformation` 的线格式。
//!
//! 对照 Crystal `Shared/Data/ItemData.cs:1090`（读入 ctor `:1099`，`Save` `:1107`）手写。
//! 该结构出现在 `ServerPackets.GetRentedItems`
//!（`Shared/ServerPackets.cs:6357-6380`：先是 `int` 个数，再逐条 `Save`）。
//!
//! 读入 ctor 虽然带 `version` / `customVersion` 参数，但函数体里没有任何版本分支，
//! 所以不存在需要裁剪的旧格式。
//!
//! 线格式（一行一字段，括号内是宽度）：
//!
//! ```text
//! u64    ItemId
//! string ItemName             <- 7 位长度前缀 + UTF-8
//! string RentingPlayerName    <- 7 位长度前缀 + UTF-8
//! i64    ItemReturnDate       <- DateTime.ToBinary()
//! ```

use mir_core::DateTimeBinary;

use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `ItemRentalInformation`（`Shared/Data/ItemData.cs:1090`；
/// 读入 ctor `:1099`，`Save` `:1107`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemRentalInformation {
    /// 被出租物品实例的唯一编号（`ulong`，不是 int）。
    pub item_id: u64,
    /// 物品名。
    pub item_name: String,
    /// 租客角色名。
    pub renting_player_name: String,
    /// 归还时间。
    pub item_return_date: DateTimeBinary,
}

impl ItemRentalInformation {
    /// 解码，见 `Shared/Data/ItemData.cs:1099`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        Ok(Self {
            item_id: r.read_u64()?,
            item_name: r.read_string()?,
            renting_player_name: r.read_string()?,
            item_return_date: DateTimeBinary(r.read_i64()?),
        })
    }

    /// 编码，见 `Shared/Data/ItemData.cs:1107`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_u64(self.item_id);
        w.write_string(&self.item_name);
        w.write_string(&self.renting_player_name);
        w.write_i64(self.item_return_date.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ItemRentalInformation {
        ItemRentalInformation {
            item_id: 0x0102_0304_0506_0708,
            item_name: "屠龙".to_owned(),
            renting_player_name: "租客".to_owned(),
            item_return_date: DateTimeBinary(0x0A0B_0C0D_0E0F_1011),
        }
    }

    #[test]
    fn roundtrip() {
        let value = sample();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = ItemRentalInformation::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, value);
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn bytes_match_hand_derived_layout() {
        // 按 `Shared/Data/ItemData.cs:1107-1113` 的写出顺序推导（全部小端）：
        //   u64 ItemId              = 0x0102030405060708 -> 08 07 06 05 04 03 02 01
        //   string ItemName         = "屠龙"：UTF-8 6 字节 -> 06 E5 B1 A0 E9 BE 99
        //                             屠 = E5 B1 A0，龙 = E9 BE 99
        //   string RentingPlayerName = ""  -> 00（空串也要写长度前缀）
        //   i64 ItemReturnDate      = 0x0A0B0C0D0E0F1011 -> 11 10 0F 0E 0D 0C 0B 0A
        // 合计 8 + 7 + 1 + 8 = 24 字节。
        let value = ItemRentalInformation {
            item_id: 0x0102_0304_0506_0708,
            item_name: "屠龙".to_owned(),
            renting_player_name: String::new(),
            item_return_date: DateTimeBinary(0x0A0B_0C0D_0E0F_1011),
        };

        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let expected: Vec<u8> = vec![
            0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, // ItemId
            0x06, 0xE5, 0xB1, 0xA0, 0xE9, 0xBE, 0x99, // ItemName = "屠龙"
            0x00, // RentingPlayerName = ""
            0x11, 0x10, 0x0F, 0x0E, 0x0D, 0x0C, 0x0B, 0x0A, // ItemReturnDate
        ];
        assert_eq!(bytes.len(), 24);
        assert_eq!(bytes, expected);

        let mut reader = PacketReader::new(&bytes);
        assert_eq!(
            ItemRentalInformation::decode(&mut reader).expect("解码应当成功"),
            value
        );
        assert!(reader.is_empty());
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(ItemRentalInformation::decode(&mut reader).is_err());
    }
}
