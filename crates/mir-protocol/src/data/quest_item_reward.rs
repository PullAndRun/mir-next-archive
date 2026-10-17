//! 任务物品奖励 `QuestItemReward` 的线格式。
//!
//! 对照 Crystal `Shared/Data/SharedData.cs:75`（读入 ctor `:82`，`Save` `:88`）手写。
//!
//! **顺序以源码为准：先整个 `Item`，最后才是 `ushort Count`。**
//!
//! ```csharp
//! public void Save(BinaryWriter writer)   // SharedData.cs:88
//! {
//!     Item.Save(writer);      // :90  先写物品模板
//!     writer.Write(Count);    // :91  再写数量
//! }
//! ```
//!
//! 读入 ctor（`:82`）同序：`Item = new ItemInfo(reader)` → `Count = reader.ReadUInt16()`。
//! 顺序写反的话客户端会把 `Count` 当成 `ItemInfo` 的第一个 `int Index`，整个包错位，
//! 且 `ItemInfo` 是变长结构、没有任何长度前缀可以重新同步。
//!
//! 线格式（一行一字段，全部小端）：
//!
//! ```text
//! ItemInfo Item      变长，布局见 `data::item_info`（不属于本模块）
//! u16      Count     数量（`ushort`，不是 `int`）
//! ```
//!
//! 版本分支已按 `int.MaxValue` 展开：C# 这里是 `new ItemInfo(reader)`，用的是 ctor 的
//! 默认参数 `version = int.MaxValue, customVersion = int.MaxValue`，等价于本 crate 的
//! `ItemInfo::decode`（只走最新版本分支）。
//!
//! 注意两个 C# 侧陷阱：
//!
//! * `Item` 是无参 ctor 不会初始化的引用字段（`:80` 只有 `public QuestItemReward() { }`），
//!   为 `null` 时 `Save` 会在 `Item.Save(writer)` 上空引用崩溃。线上数据里它必然存在，
//!   所以 Rust 侧用具体的 `ItemInfo` 字段而不是 `Option`。
//! * `Count` 是值类型，初值 **1** 而不是 0（`:78`），见下面的手写 `Default`。

use crate::data::ItemInfo;
use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `QuestItemReward`（`Shared/Data/SharedData.cs:75`；读入 ctor `:82`，`Save` `:88`）。
#[derive(Debug, Clone, PartialEq)]
pub struct QuestItemReward {
    /// C# 字段 `Item`：奖励的物品模板（变长）。
    pub item: ItemInfo,
    /// C# 字段 `Count`：数量，**`ushort`**；C# 缺省值是 1（`SharedData.cs:78`）。
    pub count: u16,
}

impl Default for QuestItemReward {
    /// 手写而不用 `derive`：C# 的 `Count` 初值是 `1`（`SharedData.cs:78`），
    /// 派生会得到 0，新建奖励的线格式就与 Crystal 不一致了。
    fn default() -> Self {
        Self {
            item: ItemInfo::default(),
            count: 1,
        }
    }
}

impl QuestItemReward {
    /// 解码，见 `Shared/Data/SharedData.cs:82`。
    ///
    /// 顺序：先 `ItemInfo`，再 `u16 Count`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let item = ItemInfo::decode(r)?;
        let count = r.read_u16()?;
        Ok(Self { item, count })
    }

    /// 编码，见 `Shared/Data/SharedData.cs:88`。
    ///
    /// 顺序：先 `ItemInfo`，再 `u16 Count`（与 ctor 严格对称）。
    pub fn encode(&self, w: &mut PacketWriter) {
        self.item.encode(w);
        w.write_u16(self.count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 编码后再解码，断言相等且负载被正好读完。
    fn roundtrip(value: &QuestItemReward) {
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = QuestItemReward::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, *value, "解码结果应与原值相同");
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    /// 一个最小但非默认的物品模板。
    fn sample_item() -> ItemInfo {
        ItemInfo {
            index: 0x0102_0304,
            name: "屠龙".to_owned(),
            ..ItemInfo::default()
        }
    }

    #[test]
    fn item_comes_before_count() {
        // 关键回归点：`Item.Save(writer); writer.Write(Count);`（`SharedData.cs:88`-`:91`）。
        //
        // 推导：先写出 `ItemInfo::default()`（长度由 `item_info` 模块决定，记作 N），
        // 其前 4 字节必定是 `i32 Index`；再写 `u16 Count`。于是：
        //   总长 = N + 2
        //   bytes[0..4] = Index 的小端编码
        //   bytes[N..N+2] = Count 的小端编码
        let mut item_writer = PacketWriter::new();
        sample_item().encode(&mut item_writer);
        let item_bytes = item_writer.into_inner();
        let n = item_bytes.len();

        let value = QuestItemReward {
            item: sample_item(),
            count: 0x0A0B,
        };

        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        assert_eq!(bytes.len(), n + 2, "末尾必须只多出 2 字节的 Count");
        assert_eq!(
            &bytes[..n],
            item_bytes.as_slice(),
            "前缀必须正好是 ItemInfo"
        );
        assert_eq!(
            &bytes[n..],
            &[0x0B, 0x0A],
            "Count 必须是 u16 小端，且写在最末"
        );
        // 若顺序写反，末尾这两字节会落在 ItemInfo 的 Index 位置上。
        assert_eq!(&bytes[..4], &[0x04, 0x03, 0x02, 0x01]);

        // 解码方向同样验证顺序。
        let mut reader = PacketReader::new(&bytes);
        let decoded = QuestItemReward::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded.count, 0x0A0B);
        assert_eq!(decoded.item.index, 0x0102_0304);
        assert!(reader.is_empty());
    }

    #[test]
    fn count_is_ushort_not_int() {
        // 回归点：`writer.Write(Count)` 写的是 `ushort`（2 字节）。
        // 最小奖励 = ItemInfo 长度 + 2。
        let mut item_writer = PacketWriter::new();
        ItemInfo::default().encode(&mut item_writer);
        let n = item_writer.into_inner().len();

        let mut writer = PacketWriter::new();
        QuestItemReward {
            item: ItemInfo::default(),
            count: 1,
        }
        .encode(&mut writer);
        assert_eq!(writer.len(), n + 2, "Count 必须是 2 字节的 ushort");
    }

    #[test]
    fn default_count_is_one() {
        // C# `public ushort Count = 1;`（`SharedData.cs:78`）。
        assert_eq!(QuestItemReward::default().count, 1);
        roundtrip(&QuestItemReward::default());
    }

    #[test]
    fn roundtrip_with_nondefault_item() {
        let value = QuestItemReward {
            item: sample_item(),
            count: u16::MAX,
        };
        roundtrip(&value);
    }

    #[test]
    fn truncated_payload_errors() {
        // 去掉 Count 的最后一个字节：`read_u16` 应当报负载不完整。
        let mut writer = PacketWriter::new();
        QuestItemReward::default().encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(QuestItemReward::decode(&mut reader).is_err());
    }
}
