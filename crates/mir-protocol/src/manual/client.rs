//! 手写的客户端包。

use mir_core::enums::{ItemType, MarketPanelType};

use crate::data::ChatItem;

/// Crystal `ClientPackets.Chat`。
///
/// 手写原因：`LinkedItems` 是 `List<ChatItem>`，元素是未生成的嵌套数据类。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chat {
    /// 聊天内容。
    pub message: String,
    /// 内容里引用的物品链接。
    pub linked_items: Vec<ChatItem>,
}

impl Chat {
    /// 包编号。
    pub const INDEX: i16 = 13;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let message = r.read_string()?;
        let count = r.read_count()?;
        let mut linked_items = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            linked_items.push(ChatItem::decode(r)?);
        }
        Ok(Self {
            message,
            linked_items,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_string(&self.message);
        w.write_count(self.linked_items.len());
        for item in &self.linked_items {
            item.encode(w);
        }
    }
}

/// Crystal `ClientPackets.MarketSearch`（`Shared/ClientPackets.cs:1564`）。
///
/// 包编号 `ClientPacketIds.MarketSearch` = 71（`Shared/Enums.cs:1790`）。
///
/// 手写原因：`MaxShape` 与 `MinShape` 被声明在**同一行**
/// （`ClientPackets.cs:1571`：`public short MinShape = 0, MaxShape = 5000;`），
/// 生成器只认「一行一个字段」，于是报 `unknown field 'MaxShape'`
/// （`tools/protocol-gen/report.md:5`）。
///
/// 线格式按 `ReadPacket`（`ClientPackets.cs:1574`）/ `WritePacket`（`:1583`）
/// 原样照搬：
///
/// ```text
/// string Match        <- 7 位长度前缀 + UTF-8
/// u8     Type         <- `ItemType`
/// bool   Usermode
/// i16    MinShape
/// i16    MaxShape     <- C# 缺省值 5000
/// u8     MarketType   <- `MarketPanelType`
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarketSearch {
    /// 搜索关键字。
    pub match_text: String,
    /// 物品类型过滤。
    pub item_type: ItemType,
    /// 是否为「按玩家名搜索」模式。
    pub usermode: bool,
    /// 外观编号下限。
    pub min_shape: i16,
    /// 外观编号上限。
    pub max_shape: i16,
    /// 面板类型（市场 / 寄售 / 拍卖 / 商城）。
    pub market_type: MarketPanelType,
}

impl MarketSearch {
    /// 包编号。
    pub const INDEX: i16 = 71;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let match_text = r.read_string()?;
        let item_type = ItemType::from_value(r.read_u8()?).unwrap_or_default();
        let usermode = r.read_bool()?;
        let min_shape = r.read_i16()?;
        let max_shape = r.read_i16()?;
        let market_type = MarketPanelType::from_value(r.read_u8()?).unwrap_or_default();
        Ok(Self {
            match_text,
            item_type,
            usermode,
            min_shape,
            max_shape,
            market_type,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_string(&self.match_text);
        w.write_u8(self.item_type.value());
        w.write_bool(self.usermode);
        w.write_i16(self.min_shape);
        w.write_i16(self.max_shape);
        w.write_u8(self.market_type.value());
    }
}

/// Crystal `ClientPackets.SendMail`（`Shared/ClientPackets.cs:2122`）。
///
/// 包编号 `ClientPacketIds.SendMail` = 117（`Shared/Enums.cs:1839`）。
///
/// 手写原因：`ItemsIdx` 是**固定 5 项的 `ulong[5]`**
/// （`ClientPackets.cs:2129`），读写都是 `for (int i = 0; i < 5; i++)`
/// （`:2138`、`:2151` 的 `writer.Write(ItemsIdx[i]);`），
/// **没有任何个数前缀**；生成器只认带前缀的可变长集合
/// （`tools/protocol-gen/report.md:6`）。
///
/// 线格式：
///
/// ```text
/// string Name
/// string Message
/// u32    Gold
/// u64×5  ItemsIdx     <- 恒定 5 项，没有个数前缀
/// bool   Stamped
/// ```
///
/// `ItemsIdx` 里放的是背包物品的 `UniqueID`（客户端在
/// `Client/MirControls/MirItemCell.cs:1803` 写入），`0` 表示该格为空
/// （`Client/MirScenes/Dialogs/MailDialogs.cs:893` 的复位写法）。
///
/// 服务端只认前 `Stamped ? 5 : 1` 格（`Server/MirObjects/PlayerObject.cs:11790`），
/// 也就是说没贴邮票时后 4 格即使填了 `UniqueID` 也会被静默忽略。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SendMail {
    /// 收件人角色名。
    pub name: String,
    /// 邮件正文。
    pub message: String,
    /// 附带的金币。
    pub gold: u32,
    /// 附带物品的实例编号，固定 5 项，`0` 表示该格为空。
    pub items_idx: [u64; 5],
    /// 是否贴邮票（贴票后免邮费，并且可以附带 5 件物品）。
    pub stamped: bool,
}

impl SendMail {
    /// 包编号。
    pub const INDEX: i16 = 117;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let name = r.read_string()?;
        let message = r.read_string()?;
        let gold = r.read_u32()?;
        // `for (int i = 0; i < 5; i++) ItemsIdx[i] = reader.ReadUInt64();`
        // —— 固定 5 项，先读满再读 `Stamped`。
        let mut items_idx = [0u64; 5];
        for slot in &mut items_idx {
            *slot = r.read_u64()?;
        }
        let stamped = r.read_bool()?;
        Ok(Self {
            name,
            message,
            gold,
            items_idx,
            stamped,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_string(&self.name);
        w.write_string(&self.message);
        w.write_u32(self.gold);
        for unique_id in self.items_idx {
            w.write_u64(unique_id);
        }
        w.write_bool(self.stamped);
    }
}

/// Crystal `ClientPackets.MailCost`（`Shared/ClientPackets.cs:2246`）。
///
/// 包编号 `ClientPacketIds.MailCost` = 123（`Shared/Enums.cs:1845`）。
///
/// 手写原因与 [`SendMail`] 相同：`ItemsIdx` 是固定 5 项的 `ulong[5]`
/// （`ClientPackets.cs:2251`），读写都是 `for (int i = 0; i < 5; i++)`
/// （`:2258`、`:2269`），没有个数前缀（`tools/protocol-gen/report.md:7`）。
///
/// 线格式：
///
/// ```text
/// u32    Gold
/// u64×5  ItemsIdx     <- 恒定 5 项，没有个数前缀
/// bool   Stamped
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MailCost {
    /// 附带的金币。
    pub gold: u32,
    /// 附带物品的实例编号，固定 5 项，`0` 表示该格为空。
    pub items_idx: [u64; 5],
    /// 是否贴邮票。
    pub stamped: bool,
}

impl MailCost {
    /// 包编号。
    pub const INDEX: i16 = 123;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let gold = r.read_u32()?;
        let mut items_idx = [0u64; 5];
        for slot in &mut items_idx {
            *slot = r.read_u64()?;
        }
        let stamped = r.read_bool()?;
        Ok(Self {
            gold,
            items_idx,
            stamped,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_u32(self.gold);
        for unique_id in self.items_idx {
            w.write_u64(unique_id);
        }
        w.write_bool(self.stamped);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_roundtrip() {
        let packet = Chat {
            message: "收屠龙 <屠龙/7>".to_owned(),
            linked_items: vec![ChatItem {
                unique_id: 7,
                title: "屠龙".to_owned(),
                grid: mir_core::enums::MirGridType::Inventory,
            }],
        };
        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = crate::PacketReader::new(&bytes);
        let decoded = Chat::decode(&mut reader).expect("decode");
        assert!(reader.is_empty());
        assert_eq!(decoded, packet);
    }

    #[test]
    fn market_search_roundtrip_and_field_order() {
        let packet = MarketSearch {
            match_text: "刀".to_owned(),
            item_type: ItemType::Weapon,
            usermode: true,
            min_shape: 0x0102,
            max_shape: 0x0304,
            market_type: MarketPanelType::Auction,
        };

        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();

        // 按 `WritePacket`（`ClientPackets.cs:1583-1591`）逐字节核对：
        //   string "刀"        -> 03 E5 88 80（UTF-8 3 字节 + 1 字节长度前缀）
        //   u8     ItemType    -> 01（Weapon）
        //   bool   Usermode    -> 01
        //   i16    MinShape    -> 02 01
        //   i16    MaxShape    -> 04 03
        //   u8     MarketType  -> 02（Auction）
        let expected: Vec<u8> = vec![
            0x03, 0xE5, 0x88, 0x80, // Match = "刀"
            0x01, // Type = Weapon
            0x01, // Usermode
            0x02, 0x01, // MinShape = 0x0102
            0x04, 0x03, // MaxShape = 0x0304
            0x02, // MarketType = Auction
        ];
        assert_eq!(bytes, expected);

        let mut reader = crate::PacketReader::new(&bytes);
        let decoded = MarketSearch::decode(&mut reader).expect("decode");
        assert!(reader.is_empty(), "left {} bytes", reader.remaining());
        assert_eq!(decoded, packet);
    }

    #[test]
    fn send_mail_roundtrip_has_five_ids_without_count() {
        let packet = SendMail {
            name: "收件人".to_owned(),
            message: "附件".to_owned(),
            gold: 0x0A0B_0C0D,
            // 0 表示空位；这里故意让下标 1、3 两格为 0。
            items_idx: [0x0102_0304_0506_0708, 0, 7, 0, u64::MAX],
            stamped: true,
        };

        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();

        // 关键回归点：`ItemsIdx` 前**没有** `int` 个数前缀，
        // 所以总长 = 两个字符串 + 4 字节金币 + 固定 5×8 字节 + 1 字节标志。
        let head = 1 + packet.name.len() + 1 + packet.message.len() + 4;
        assert_eq!(bytes.len(), head + 5 * 8 + 1);

        // 5 个 u64 都按小端连续写出。
        let ids = &bytes[head..head + 5 * 8];
        for (index, unique_id) in packet.items_idx.iter().enumerate() {
            assert_eq!(
                &ids[index * 8..(index + 1) * 8],
                &unique_id.to_le_bytes(),
                "第 {index} 格必须是原始 u64"
            );
        }
        assert_eq!(bytes[head + 5 * 8], 0x01, "最后 1 字节是 Stamped");

        let mut reader = crate::PacketReader::new(&bytes);
        let decoded = SendMail::decode(&mut reader).expect("decode");
        assert!(reader.is_empty(), "left {} bytes", reader.remaining());
        assert_eq!(decoded, packet);
    }

    #[test]
    fn mail_cost_roundtrip_has_five_ids_without_count() {
        let packet = MailCost {
            gold: 5000,
            items_idx: [9, 8, 0, 0, 1],
            stamped: false,
        };

        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();

        // 4 字节金币 + 固定 5×8 字节 + 1 字节标志，没有个数前缀。
        assert_eq!(bytes.len(), 4 + 5 * 8 + 1);
        assert_eq!(&bytes[..4], &packet.gold.to_le_bytes());
        assert_eq!(&bytes[4..12], &packet.items_idx[0].to_le_bytes());
        assert_eq!(bytes[4 + 5 * 8], 0x00, "Stamped = false");

        let mut reader = crate::PacketReader::new(&bytes);
        let decoded = MailCost::decode(&mut reader).expect("decode");
        assert!(reader.is_empty(), "left {} bytes", reader.remaining());
        assert_eq!(decoded, packet);
    }
}
