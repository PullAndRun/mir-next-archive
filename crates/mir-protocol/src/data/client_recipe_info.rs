//! 客户端配方资料 `ClientRecipeInfo` 的线格式。
//!
//! 对照 Crystal `Shared/Data/ClientData.cs:71`（读入 ctor `:82`，`Save` `:102`）手写。
//! 它由 `ServerPackets.NewRecipeInfo`（`Shared/ServerPackets.cs:6611-6629`）整块下发。
//!
//! 线格式（一行一字段，括号内是宽度）：
//!
//! ```text
//! u32    Gold
//! u8     Chance
//! UserItem Item               <- 完整 `UserItem`（见 `data::UserItem`）
//! i32    Tools.Count
//! per tool: 完整 UserItem
//! i32    Ingredients.Count
//! per ingredient: 完整 UserItem
//! ```
//!
//! C# 的 `Item` 可以为 `null`，但 `Save`（`:106`）无条件解引用它，
//! 读入 ctor 也总是构造一个 `UserItem`（`:87`），所以 Rust 侧用非 `Option` 的
//! `UserItem` 建模：线格式上它**永远**存在。
//!
//! 材料列表里装的同样是 `UserItem`（不是物品模板），只是 `Count` 表示需要几个。

use crate::data::UserItem;
use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `ClientRecipeInfo`（`Shared/Data/ClientData.cs:71`；ctor `:82`，`Save` `:102`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClientRecipeInfo {
    /// 制作费用（金币）。
    pub gold: u32,
    /// 成功率（百分比）。
    pub chance: u8,
    /// 产出的物品。
    pub item: UserItem,
    /// 需要的工具。
    pub tools: Vec<UserItem>,
    /// 需要的材料。
    pub ingredients: Vec<UserItem>,
}

impl ClientRecipeInfo {
    /// 解码，见 `Shared/Data/ClientData.cs:82`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let gold = r.read_u32()?;
        let chance = r.read_u8()?;
        let item = UserItem::decode(r)?;

        let tool_count = r.read_count()?;
        // 不预分配：长度来自不可信输入，`UserItem` 本身较大。
        let mut tools = Vec::new();
        for _ in 0..tool_count {
            tools.push(UserItem::decode(r)?);
        }

        let ingredient_count = r.read_count()?;
        let mut ingredients = Vec::new();
        for _ in 0..ingredient_count {
            ingredients.push(UserItem::decode(r)?);
        }

        Ok(Self {
            gold,
            chance,
            item,
            tools,
            ingredients,
        })
    }

    /// 编码，见 `Shared/Data/ClientData.cs:102`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_u32(self.gold);
        w.write_u8(self.chance);
        self.item.encode(w);

        w.write_count(self.tools.len());
        for tool in &self.tools {
            tool.encode(w);
        }

        w.write_count(self.ingredients.len());
        for ingredient in &self.ingredients {
            ingredient.encode(w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(unique_id: u64, item_index: i32, count: u16) -> UserItem {
        UserItem {
            unique_id,
            item_index,
            count,
            ..UserItem::default()
        }
    }

    fn sample() -> ClientRecipeInfo {
        ClientRecipeInfo {
            gold: 50_000,
            chance: 65,
            item: item(1, 100, 1),
            tools: vec![item(2, 200, 1), item(3, 201, 2)],
            ingredients: vec![item(4, 300, 10)],
        }
    }

    #[test]
    fn roundtrip() {
        let value = sample();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = ClientRecipeInfo::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, value);
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn empty_lists_roundtrip() {
        let value = ClientRecipeInfo {
            item: item(0, 0, 1),
            ..ClientRecipeInfo::default()
        };
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = ClientRecipeInfo::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, value);
        assert!(reader.is_empty());
    }

    #[test]
    fn item_lists_are_counted_with_i32() {
        // 产出物品 `item(1, 100, 1)` 按 `UserItem.Save` 是 53 字节（见 `data::user_item` 测试），
        // 因此 Tools 的个数前缀从 4(Gold) + 1(Chance) + 53(Item) = 58 开始。
        const USER_ITEM_LEN: usize = 53;
        const TOOLS_COUNT_OFFSET: usize = 4 + 1 + USER_ITEM_LEN;

        let value = sample();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        assert_eq!(
            &bytes[TOOLS_COUNT_OFFSET..TOOLS_COUNT_OFFSET + 4],
            &[2, 0, 0, 0]
        );

        let ingredients_offset = TOOLS_COUNT_OFFSET + 4 + 2 * USER_ITEM_LEN;
        assert_eq!(
            &bytes[ingredients_offset..ingredients_offset + 4],
            &[1, 0, 0, 0]
        );
        assert_eq!(bytes.len(), ingredients_offset + 4 + USER_ITEM_LEN);
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(ClientRecipeInfo::decode(&mut reader).is_err());
    }
}
