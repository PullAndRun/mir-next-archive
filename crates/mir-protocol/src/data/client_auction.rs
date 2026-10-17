//! 拍卖/寄售条目的线格式。
//!
//! 对应 Crystal `Shared/Data/ClientData.cs:207` 的 `ClientAuction`，
//! 线上出现在 `ServerPackets.NPCMarket`（`Shared/ServerPackets.cs:4223`）/
//! `NPCMarketPage`（`:4252`）的 `Listings` 列表里
//! （`tools/protocol-gen/report.md` 里这两项因 `List<ClientAuction>` 暂未生成）。

use mir_core::DateTimeBinary;
use mir_core::enums::MarketItemType;

use crate::data::UserItem;
use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `ClientAuction`（`Shared/Data/ClientData.cs:207`；读入 ctor `:218`，`Save` `:227`）。
///
/// 线格式（一行一字段）：
///
/// ```text
/// u64            AuctionID
/// UserItem       Item           <- 递归的完整物品负载，见 `data::UserItem`
/// string         Seller         <- 7 位长度前缀 + UTF-8
/// u32            Price
/// i64            ConsignmentDate <- DateTime.ToBinary()
/// u8             ItemType        <- MarketItemType
/// ```
///
/// `Item` 夹在 `AuctionID` 与 `Seller` 之间，是最容易漏写的一项
/// （`ClientData.cs:220-225`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClientAuction {
    /// 拍卖/寄售编号。
    pub auction_id: u64,
    /// 标的物品。
    pub item: UserItem,
    /// 卖家角色名（C# 初值 `string.Empty`）。
    pub seller: String,
    /// 价格。
    pub price: u32,
    /// 寄售时间（`DateTime.ToBinary()`）。C# 初值 `DateTime.MinValue`，其
    /// `ToBinary()` 为 `0`，与派生默认一致。
    pub consignment_date: DateTimeBinary,
    /// 市场条目类型（寄售 / 拍卖 / 商城）。
    pub item_type: MarketItemType,
}

impl ClientAuction {
    /// 解码，见 `Shared/Data/ClientData.cs:218`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        Ok(Self {
            auction_id: r.read_u64()?,
            item: UserItem::decode(r)?,
            seller: r.read_string()?,
            price: r.read_u32()?,
            consignment_date: DateTimeBinary(r.read_i64()?),
            item_type: MarketItemType::from_value(r.read_u8()?).unwrap_or_default(),
        })
    }

    /// 编码，见 `Shared/Data/ClientData.cs:227`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_u64(self.auction_id);
        self.item.encode(w);
        w.write_string(&self.seller);
        w.write_u32(self.price);
        w.write_i64(self.consignment_date.0);
        // `writer.Write((byte)ItemType)`（`ClientData.cs:234`）：单字节枚举。
        w.write_u8(self.item_type as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_into(auction: &ClientAuction) -> Vec<u8> {
        let mut writer = PacketWriter::new();
        auction.encode(&mut writer);
        writer.into_inner()
    }

    fn roundtrip(auction: &ClientAuction) {
        let bytes = encode_into(auction);
        let mut reader = PacketReader::new(&bytes);
        let decoded = ClientAuction::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, *auction, "解码结果应与原值相同");
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    fn sample(item_type: MarketItemType) -> ClientAuction {
        ClientAuction {
            auction_id: 0x1122_3344_5566_7788,
            item: UserItem {
                unique_id: 99,
                item_index: 4242,
                current_dura: 1000,
                max_dura: 5000,
                count: 1,
                identified: true,
                cursed: true,
                ..UserItem::default()
            },
            seller: "张三".to_owned(),
            price: 1_000_000,
            consignment_date: DateTimeBinary(0x0F0E_0D0C_0B0A_0908),
            item_type,
        }
    }

    #[test]
    fn client_auction_roundtrip() {
        for item_type in MarketItemType::ALL {
            roundtrip(&sample(*item_type));
        }
    }

    #[test]
    fn client_auction_empty_roundtrip() {
        roundtrip(&ClientAuction::default());
    }

    #[test]
    fn item_type_is_the_last_byte() {
        let mut bytes = encode_into(&sample(MarketItemType::GameShop));
        assert_eq!(
            bytes.pop(),
            Some(2),
            "MarketItemType::GameShop 的判别值是 2"
        );

        let bytes = encode_into(&sample(MarketItemType::Auction));
        assert_eq!(bytes.last().copied(), Some(1));

        // 尾部布局：... string Seller、u32 Price、i64 ConsignmentDate、u8 ItemType。
        let auction = sample(MarketItemType::Consign);
        let bytes = encode_into(&auction);
        assert_eq!(
            &bytes[bytes.len() - 9..bytes.len() - 1],
            &0x0F0E_0D0C_0B0A_0908i64.to_le_bytes(),
            "ItemType 前 8 字节是 ConsignmentDate"
        );
        assert_eq!(
            &bytes[bytes.len() - 13..bytes.len() - 9],
            &1_000_000u32.to_le_bytes(),
            "再往前 4 字节是 Price"
        );
    }

    #[test]
    fn auction_id_comes_before_the_item() {
        let auction = sample(MarketItemType::Consign);
        let mut item_only = PacketWriter::new();
        auction.item.encode(&mut item_only);
        let item_len = item_only.len();

        let bytes = encode_into(&auction);
        assert_eq!(
            &bytes[..8],
            &0x1122_3344_5566_7788u64.to_le_bytes(),
            "第一个字段是 AuctionID"
        );
        assert_eq!(
            &bytes[8..16],
            &99u64.to_le_bytes(),
            "紧接着必须是 UserItem.UniqueID"
        );
        // 物品负载 + Seller / Price / ConsignmentDate / ItemType 尾巴。
        assert!(bytes.len() > 8 + item_len);
    }

    #[test]
    fn truncated_payload_errors() {
        let bytes = encode_into(&sample(MarketItemType::Consign));
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(ClientAuction::decode(&mut reader).is_err());
    }
}
