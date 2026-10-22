//! 商城商品 `GameShopItem` 的线格式。
//!
//! 对照 Crystal `Shared/Data/ItemData.cs:778`（包路径读入 ctor `:829`，
//! `Save(BinaryWriter writer, bool packet = false)` 在 `:848`）手写。
//!
//! Crystal 里这个类有两条互不兼容的序列化路径：
//!
//! | 路径 | 读入 ctor | 写出 | 含 `Info` |
//! | --- | --- | --- | --- |
//! | 包（`packet = true`） | `GameShopItem(reader, packet: true)` `:829` | `Save(writer, true)` `:848` | 是 |
//! | 物品库（`version = int.MaxValue`） | `GameShopItem(reader, version, customVersion)` `:800` | `Save(writer)` / `Save(writer, false)` | 否 |
//!
//! 本模块实现**两条路径**：
//!
//! * `decode` / `encode`：包路径。`ServerPackets.GameShopInfo`
//!   （`Shared/ServerPackets.cs:5348-5369`）用的正是 `Save(writer, true)` /
//!   `new GameShopItem(reader, true)`，`Info` 紧跟在 `GIndex` 之后整块写出。
//! * `decode_db` / `encode_db`：物品库路径（`Save(writer)` 的缺省参数 `packet = false`、
//!   以及带 `version` 的 ctor `:800`，按 `version = int.MaxValue` 展开）。
//!   除**不含 `Info`** 外字段顺序完全相同；服务端从 `GameShop.db` 读入后自行按
//!   `item_index` 查表补 `Info`（`Server/MirEnvir/Envir.cs:2917` 即此调用）。
//!
//! 线格式（一行一字段，括号内是宽度）：
//!
//! ```text
//! i32    ItemIndex
//! i32    GIndex
//! ItemInfo Info               <- 仅包路径；完整 `ItemInfo`（见 `data::ItemInfo`）
//! u32    GoldPrice
//! u32    CreditPrice
//! u16    Count                <- `writer.Write(Count)`，Count 是 ushort
//! string Class
//! string Category
//! i32    Stock
//! bool   iStock
//! bool   Deal
//! bool   TopItem
//! i64    Date                 <- DateTime.ToBinary()
//! bool   CanBuyCredit
//! bool   CanBuyGold
//! ```

use mir_core::DateTimeBinary;

use crate::data::ItemInfo;
use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `GameShopItem`（`Shared/Data/ItemData.cs:778`；包路径 ctor `:829`，`Save` `:848`）。
///
/// 线格式见模块文档。字段顺序按 `Save(writer, true)` 的写出顺序排列；
/// 物品库路径（`decode_db` / `encode_db`）跳过 `info` 一项，其余相同。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GameShopItem {
    /// 物品模板编号。
    pub item_index: i32,
    /// 商城商品编号（`GIndex` 是 Crystal 的原名）。
    pub g_index: i32,
    /// 物品模板；包路径整块写出，物品库路径不写。
    pub info: ItemInfo,
    /// 金币价格。
    pub gold_price: u32,
    /// 元宝价格。
    pub credit_price: u32,
    /// 每次售出的数量（`ushort`）。
    pub count: u16,
    /// 所属分类的上级分组名（Crystal 叫 `Class`，与职业无关）。
    pub class: String,
    /// 所属分类名。
    pub category: String,
    /// 库存。
    pub stock: i32,
    /// 是否无限库存。
    pub i_stock: bool,
    /// 是否促销中。
    pub deal: bool,
    /// 是否置顶。
    pub top_item: bool,
    /// 上架时间。
    pub date: DateTimeBinary,
    /// 是否可用元宝购买。
    pub can_buy_credit: bool,
    /// 是否可用金币购买。
    pub can_buy_gold: bool,
}

impl Default for GameShopItem {
    /// 手写而不用 `derive`：Crystal 的 `Count` 初始值是 1（`ItemData.cs:785`），
    /// 派生默认值会写出 0 个，与 Crystal 新建商品的语义不符。
    fn default() -> Self {
        Self {
            item_index: 0,
            g_index: 0,
            info: ItemInfo::default(),
            gold_price: 0,
            credit_price: 0,
            count: 1,
            class: String::new(),
            category: String::new(),
            stock: 0,
            i_stock: false,
            deal: false,
            top_item: false,
            date: DateTimeBinary::default(),
            can_buy_credit: false,
            can_buy_gold: false,
        }
    }
}

impl GameShopItem {
    /// 解码，见 `Shared/Data/ItemData.cs:829`（`packet = true` 路径）。
    ///
    /// 注意 `Info` 位于 `GIndex` 与 `GoldPrice` 之间（`:833`），
    /// 而两条 `Save` 路径的其它字段顺序完全相同。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let item_index = r.read_i32()?;
        let g_index = r.read_i32()?;
        let info = ItemInfo::decode(r)?;
        let gold_price = r.read_u32()?;
        let credit_price = r.read_u32()?;
        // 包路径恒为 `Count = reader.ReadUInt16()`（`:836`），没有旧版的 `uint` 分支。
        let count = r.read_u16()?;
        let class = r.read_string()?;
        let category = r.read_string()?;
        let stock = r.read_i32()?;
        let i_stock = r.read_bool()?;
        let deal = r.read_bool()?;
        let top_item = r.read_bool()?;
        let date = DateTimeBinary(r.read_i64()?);
        let can_buy_credit = r.read_bool()?;
        let can_buy_gold = r.read_bool()?;
        Ok(Self {
            item_index,
            g_index,
            info,
            gold_price,
            credit_price,
            count,
            class,
            category,
            stock,
            i_stock,
            deal,
            top_item,
            date,
            can_buy_credit,
            can_buy_gold,
        })
    }

    /// 编码，见 `Shared/Data/ItemData.cs:848`（`packet = true` 路径）。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i32(self.item_index);
        w.write_i32(self.g_index);
        self.info.encode(w);
        w.write_u32(self.gold_price);
        w.write_u32(self.credit_price);
        w.write_u16(self.count);
        w.write_string(&self.class);
        w.write_string(&self.category);
        w.write_i32(self.stock);
        w.write_bool(self.i_stock);
        w.write_bool(self.deal);
        w.write_bool(self.top_item);
        w.write_i64(self.date.0);
        w.write_bool(self.can_buy_credit);
        w.write_bool(self.can_buy_gold);
    }

    /// 解码**物品库**路径，见 `Shared/Data/ItemData.cs:800`（`version = int.MaxValue`）。
    ///
    /// 与包路径的唯一区别是 `GIndex` 之后**没有** `Info`。版本分支全部静态可解：
    /// `version <= 84` 的 `Count` 旧格式（`u32`）不执行，`version > 105` 的两个
    /// 购买标志恒执行（`:821-825`）。
    ///
    /// 读入后 `info` 保持 `ItemInfo::default()`（C# 里是 `null`）——
    /// 物品模板由调用方按 `item_index` 自行查表填充，这一点与 Crystal 相同。
    pub fn decode_db(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let item_index = r.read_i32()?;
        let g_index = r.read_i32()?;
        let gold_price = r.read_u32()?;
        let credit_price = r.read_u32()?;
        let count = r.read_u16()?;
        let class = r.read_string()?;
        let category = r.read_string()?;
        let stock = r.read_i32()?;
        let i_stock = r.read_bool()?;
        let deal = r.read_bool()?;
        let top_item = r.read_bool()?;
        let date = DateTimeBinary(r.read_i64()?);
        // `if (version > 105) { CanBuyCredit = ...; CanBuyGold = ...; }`
        let can_buy_credit = r.read_bool()?;
        let can_buy_gold = r.read_bool()?;
        Ok(Self {
            item_index,
            g_index,
            info: ItemInfo::default(),
            gold_price,
            credit_price,
            count,
            class,
            category,
            stock,
            i_stock,
            deal,
            top_item,
            date,
            can_buy_credit,
            can_buy_gold,
        })
    }

    /// 编码**物品库**路径，见 `Shared/Data/ItemData.cs:848`（`packet = false`）。
    ///
    /// 与 `encode` 的唯一区别是不写 `Info`。
    pub fn encode_db(&self, w: &mut PacketWriter) {
        w.write_i32(self.item_index);
        w.write_i32(self.g_index);
        w.write_u32(self.gold_price);
        w.write_u32(self.credit_price);
        w.write_u16(self.count);
        w.write_string(&self.class);
        w.write_string(&self.category);
        w.write_i32(self.stock);
        w.write_bool(self.i_stock);
        w.write_bool(self.deal);
        w.write_bool(self.top_item);
        w.write_i64(self.date.0);
        w.write_bool(self.can_buy_credit);
        w.write_bool(self.can_buy_gold);
    }
}

#[cfg(test)]
mod tests {
    use mir_core::enums::{BindMode, ItemGrade, ItemType};

    use super::*;

    fn sample() -> GameShopItem {
        GameShopItem {
            item_index: 4242,
            g_index: 7,
            info: ItemInfo {
                index: 4242,
                name: "传送卷".to_owned(),
                item_type: ItemType::Scroll,
                grade: ItemGrade::Rare,
                bind: BindMode::DontStore,
                ..ItemInfo::default()
            },
            gold_price: 100_000,
            credit_price: 250,
            count: 5,
            class: "消耗品".to_owned(),
            category: "卷轴".to_owned(),
            stock: 999,
            i_stock: true,
            deal: true,
            top_item: true,
            date: DateTimeBinary(0x0102_0304_0506_0708),
            can_buy_credit: true,
            can_buy_gold: false,
        }
    }

    #[test]
    fn roundtrip() {
        let value = sample();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = GameShopItem::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, value);
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn info_sits_between_gindex_and_gold_price() {
        // 按 `Save(writer, true)`（`ItemData.cs:848-865`）推导：
        //   i32 ItemIndex      -> 04 03 02 01
        //   i32 GIndex         -> 08 07 06 05
        //   ItemInfo Info      -> 紧随其后，整块
        //   u32 GoldPrice      -> 0D 0C 0B 0A
        //   u32 CreditPrice    -> 44 33 22 11
        //   u16 Count          -> 66 55（ushort，不是 int）
        //   string Class  "A"  -> 01 41
        //   string Category "" -> 00
        //   i32 Stock = -1     -> FF FF FF FF
        //   bool iStock        -> 01
        //   bool Deal          -> 00
        //   bool TopItem       -> 01
        //   i64 Date           -> 11 10 0F 0E 0D 0C 0B 0A
        //   bool CanBuyCredit  -> 01
        //   bool CanBuyGold    -> 00
        let value = GameShopItem {
            item_index: 0x0102_0304,
            g_index: 0x0506_0708,
            info: ItemInfo::default(),
            gold_price: 0x0A0B_0C0D,
            credit_price: 0x1122_3344,
            count: 0x5566,
            class: "A".to_owned(),
            category: String::new(),
            stock: -1,
            i_stock: true,
            deal: false,
            top_item: true,
            date: DateTimeBinary(0x0A0B_0C0D_0E0F_1011),
            can_buy_credit: true,
            can_buy_gold: false,
        };

        let mut writer = PacketWriter::new();
        value.info.encode(&mut writer);
        let info_bytes = writer.into_inner();

        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        // 前 8 字节是两个 int，随后是整块 `ItemInfo`。
        assert_eq!(&bytes[..4], &[0x04, 0x03, 0x02, 0x01]);
        assert_eq!(&bytes[4..8], &[0x08, 0x07, 0x06, 0x05]);
        assert_eq!(&bytes[8..8 + info_bytes.len()], &info_bytes[..]);

        let tail = &bytes[8 + info_bytes.len()..];
        let expected_tail: Vec<u8> = vec![
            0x0D, 0x0C, 0x0B, 0x0A, // GoldPrice
            0x44, 0x33, 0x22, 0x11, // CreditPrice
            0x66, 0x55, // Count（ushort）
            0x01, 0x41, // Class = "A"
            0x00, // Category = ""
            0xFF, 0xFF, 0xFF, 0xFF, // Stock = -1
            0x01, // iStock
            0x00, // Deal
            0x01, // TopItem
            0x11, 0x10, 0x0F, 0x0E, 0x0D, 0x0C, 0x0B, 0x0A, // Date
            0x01, // CanBuyCredit
            0x00, // CanBuyGold
        ];
        assert_eq!(tail, expected_tail.as_slice());

        let mut reader = PacketReader::new(&bytes);
        assert_eq!(
            GameShopItem::decode(&mut reader).expect("解码应当成功"),
            value
        );
        assert!(reader.is_empty());
    }

    #[test]
    fn default_count_is_one() {
        // `ItemData.cs:785`：`public ushort Count = 1;`
        assert_eq!(GameShopItem::default().count, 1);
        let value = GameShopItem::default();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes);
        assert_eq!(
            GameShopItem::decode(&mut reader).expect("解码应当成功"),
            value
        );
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(GameShopItem::decode(&mut reader).is_err());
    }

    #[test]
    fn db_path_has_no_info_and_roundtrips() {
        // 物品库路径（`ItemData.cs:800-827` / `:848-865`，`packet = false`）：
        // 前 8 字节之后**直接**是 GoldPrice，没有 `ItemInfo` 块。
        let value = sample();
        let mut writer = PacketWriter::new();
        value.encode_db(&mut writer);
        let db_bytes = writer.into_inner();

        // 包路径比库里多出恰好一个 `Info` 的体积。
        let mut writer = PacketWriter::new();
        value.info.encode(&mut writer);
        let info_len = writer.len();

        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let packet_bytes = writer.into_inner();
        assert_eq!(db_bytes.len() + info_len, packet_bytes.len());

        // 库里第 9 个字节就是 GoldPrice = 100000 -> A0 86 01 00。
        assert_eq!(&db_bytes[8..12], &[0xA0, 0x86, 0x01, 0x00]);

        let mut reader = PacketReader::new(&db_bytes);
        let decoded = GameShopItem::decode_db(&mut reader).expect("解码应当成功");
        assert!(reader.is_empty());

        // 库路径不带 `Info`，读回的是默认值；其余字段应与原值一致。
        let expected = GameShopItem {
            info: ItemInfo::default(),
            ..value
        };
        assert_eq!(decoded, expected);

        let mut writer = PacketWriter::new();
        decoded.encode_db(&mut writer);
        assert_eq!(writer.into_inner(), db_bytes);
    }
}
