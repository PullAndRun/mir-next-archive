//! 公会仓库格子的线格式。
//!
//! 对应 Crystal `Shared/Data/GuildData.cs:44` 的 `GuildStorageItem`。

use crate::data::UserItem;
use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `GuildStorageItem`（`Shared/Data/GuildData.cs:44`；读入 ctor `:50`，`Save` `:55`）。
///
/// 线格式：**先物品、后 `UserId`**（全部小端）：
///
/// ```text
/// UserItem Item      <- 递归的完整物品负载，见 `data::UserItem`
/// i64      UserId    <- 存放者的角色编号
/// ```
///
/// 存在标志（`bool`）**不属于**本结构：`ServerPackets.GuildStorageList`
/// （`Shared/ServerPackets.cs:4693`、`:4702`）在数组元素前单独写一个 `bool`，
/// 由包的编解码器负责。
///
/// 注意 Crystal 有一处**自相矛盾**的写法：`ServerPackets.GuildStorageItemChange`
/// （`Shared/ServerPackets.cs:4663-4667` / `:4677-4678`）先写 `UserId` 再写 `UserItem`，
/// 与这里的 `Save` 顺序相反。两端都是自洽的，所以能跑，但将来的手写包**不能**
/// 直接复用本类型的 `decode` / `encode`，否则会读错前 8 字节
/// （`UserItem.UniqueID` 是 `u64`，恰好也是 8 字节，错位后不报错但内容全错）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildStorageItem {
    /// 存放的物品。C# 侧可以为 `null`（`Save` 会空引用），这里用默认值占位。
    pub item: UserItem,
    /// 物品所有者的角色编号。
    pub user_id: i64,
}

impl GuildStorageItem {
    /// 解码：`UserItem Item`，然后 `i64 UserId`。
    ///
    /// 见 `Shared/Data/GuildData.cs:50`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let item = UserItem::decode(r)?;
        let user_id = r.read_i64()?;
        Ok(Self { item, user_id })
    }

    /// 编码：`UserItem Item`，然后 `i64 UserId`。
    ///
    /// 见 `Shared/Data/GuildData.cs:55`。
    pub fn encode(&self, w: &mut PacketWriter) {
        self.item.encode(w);
        w.write_i64(self.user_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_into(item: &GuildStorageItem) -> Vec<u8> {
        let mut writer = PacketWriter::new();
        item.encode(&mut writer);
        writer.into_inner()
    }

    fn sample() -> GuildStorageItem {
        GuildStorageItem {
            item: UserItem {
                unique_id: 0x0A0B_0C0D_0E0F_1011,
                item_index: 4242,
                current_dura: 1000,
                max_dura: 5000,
                count: 2,
                identified: true,
                ..UserItem::default()
            },
            user_id: -7,
        }
    }

    #[test]
    fn guild_storage_item_roundtrip() {
        let value = sample();
        let bytes = encode_into(&value);
        let mut reader = PacketReader::new(&bytes);
        let decoded = GuildStorageItem::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, value);
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn user_id_is_written_after_the_item() {
        let value = sample();

        let mut item_only = PacketWriter::new();
        value.item.encode(&mut item_only);
        let item_len = item_only.len();

        let bytes = encode_into(&value);
        assert_eq!(bytes.len(), item_len + 8, "物品之后只剩 8 字节的 UserId");
        // 物品负载在前：开头是 UserItem 的 UniqueID（u64 小端）。
        assert_eq!(
            &bytes[..8],
            &0x0A0B_0C0D_0E0F_1011u64.to_le_bytes(),
            "第一个字段必须是 UserItem.UniqueID"
        );
        assert_eq!(
            &bytes[item_len..],
            &(-7i64).to_le_bytes(),
            "UserId 必须紧跟在完整物品之后"
        );
    }

    #[test]
    fn truncated_payload_errors() {
        let bytes = encode_into(&sample());
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(GuildStorageItem::decode(&mut reader).is_err());
    }
}
