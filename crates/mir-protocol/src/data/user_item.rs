//! 装备实例 `UserItem` 及其附属结构的线格式。
//!
//! 全部对照 Crystal `Shared/Data/ItemData.cs`（`UserItem` 在 `:277`）与
//! `Shared/Data/Stat.cs`（`Stats` 在 `:1`）手写。
//!
//! **只实现最新版本分支。** Crystal 的读入 ctor
//! `UserItem(BinaryReader reader, int version = int.MaxValue, int customVersion = int.MaxValue)`
//! 里有一堆 `if (version <= 84)` / `if (version > 84)` / `if (version < 65) return;`
//! 分支。网络包与数据库路径都以缺省参数（`int.MaxValue`）调用，于是：
//!
//! * `version <= 84` 与 `version < N` 的分支**永不执行**；
//! * `version > 84`、`version > 85`、`version > 107` 的分支**总是执行**。
//!
//! 所以这里没有任何版本参数，字段顺序即最新格式的字段顺序。
//! 唯一保留的分支是 `if (reader.ReadBoolean())` 这类**运行期**判断——
//! 它们与版本无关，对应结构体里的 `Option<...>` 字段
//! （`ExpireInfo` / `RentalInformation` / `SealedInfo`）。
//!
//! 不在线上传输的成员（`Info`、`DuraChanged`、`BuybackExpiryDate`）不属于本模块。

use mir_core::DateTimeBinary;
use mir_core::enums::{AwakeType, BindMode, RefinedValue, Stat};

use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `Stats`（`Shared/Data/Stat.cs:1`；读入 ctor `:36`，`Save` `:50`）。
///
/// C# 用一个 `SortedDictionary<Stat, int>` 承载，索引器在赋值 0 时**删除**键，
/// 因此“值为 0”与“键不存在”是同一件事。这里用 `BTreeMap` 复现：
/// 键按 `Stat` 的判别值升序迭代，与 `SortedDictionary` 的迭代顺序一致，
/// 保证写出的顺序逐字节相同。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Stats {
    /// 内部存储。不变量：**不存在值为 0 的键**（与 C# 索引器一致）。
    values: std::collections::BTreeMap<Stat, i32>,
}

impl Stats {
    /// 新建空集合。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 读取某项；缺省为 0（对应 C# 索引器的 getter）。
    #[must_use]
    pub fn get(&self, stat: Stat) -> i32 {
        self.values.get(&stat).copied().unwrap_or_default()
    }

    /// 写入某项；写 0 等于删除（对应 C# 索引器的 setter）。
    pub fn set(&mut self, stat: Stat, value: i32) {
        if value == 0 {
            self.values.remove(&stat);
        } else {
            self.values.insert(stat, value);
        }
    }

    /// 累加某项（`add` 语义同 `stat += value`，累加后为 0 时键消失）。
    pub fn add(&mut self, stat: Stat, value: i32) {
        let merged = self.get(stat).saturating_add(value);
        self.set(stat, merged);
    }

    /// 与 C# `Stats.Count` 相同：所有项**绝对值**之和。
    ///
    /// 用 `saturating_abs` 避免 `i32::MIN.abs()` 在 debug 下 panic
    /// （C# 那边 `Math.Abs(int.MinValue)` 会抛异常，这里选择不崩）。
    #[must_use]
    pub fn count(&self) -> i32 {
        self.values.values().fold(0i32, |acc, value| {
            acc.saturating_add(value.saturating_abs())
        })
    }

    /// 是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// 项数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// 只读访问内部映射：按键（`Stat` 判别值）升序，即线格式的写出顺序。
    #[must_use]
    pub fn values(&self) -> &std::collections::BTreeMap<Stat, i32> {
        &self.values
    }

    /// 解码：`int count`，随后 `count` 组 `(byte stat, int value)`。
    ///
    /// 见 `Shared/Data/Stat.cs:36`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let count = r.read_count()?;
        let mut out = Self::default();
        for _ in 0..count {
            let stat = Stat::from_value(r.read_u8()?).unwrap_or_default();
            let value = r.read_i32()?;
            // C# 这里走索引器 `Values[(Stat)reader.ReadByte()] = ...`，
            // 值为 0 时删除键，因此也用 set()。
            out.set(stat, value);
        }
        Ok(out)
    }

    /// 编码：`int count`，随后按键升序写 `(byte stat, int value)`。
    ///
    /// 见 `Shared/Data/Stat.cs:50`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_count(self.values.len());
        for (&stat, &value) in &self.values {
            w.write_u8(stat.value());
            w.write_i32(value);
        }
    }
}

impl FromIterator<(Stat, i32)> for Stats {
    fn from_iter<T: IntoIterator<Item = (Stat, i32)>>(iter: T) -> Self {
        let mut out = Self::default();
        for (stat, value) in iter {
            out.set(stat, value);
        }
        out
    }
}

/// Crystal `Awake`（`Shared/Data/ItemData.cs:874`；读入 ctor `:893`，`Save` `:903`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Awake {
    /// C# 字段 `Type`：觉醒类型。
    pub awake_type: AwakeType,
    /// C# 字段 `listAwake`：每一级的觉醒数值（长度即觉醒等级）。
    pub values: Vec<u8>,
}

impl Awake {
    /// 解码：`byte type`，随后 `int count` + `count` 个 `byte`。
    ///
    /// 见 `Shared/Data/ItemData.cs:893`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let awake_type = AwakeType::from_value(r.read_u8()?).unwrap_or_default();
        let count = r.read_count()?;
        let mut values = Vec::new();
        for _ in 0..count {
            values.push(r.read_u8()?);
        }
        Ok(Self { awake_type, values })
    }

    /// 编码：`byte type`，随后 `int count` + 每个数值。
    ///
    /// 见 `Shared/Data/ItemData.cs:903`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_u8(self.awake_type.value());
        w.write_count(self.values.len());
        for &value in &self.values {
            w.write_u8(value);
        }
    }
}

/// Crystal `ExpireInfo`（`Shared/Data/ItemData.cs:711`；读入 ctor `:717`，`Save` `:722`）。
///
/// 只有一个 `DateTime.ToBinary()` 的 `long`。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExpireInfo {
    /// 到期时间。
    pub expiry_date: DateTimeBinary,
}

impl ExpireInfo {
    /// 解码：`long`（`DateTime.FromBinary`）。
    ///
    /// 见 `Shared/Data/ItemData.cs:717`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        Ok(Self {
            expiry_date: DateTimeBinary(r.read_i64()?),
        })
    }

    /// 编码：`long`（`DateTime.ToBinary`）。
    ///
    /// 见 `Shared/Data/ItemData.cs:722`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i64(self.expiry_date.0);
    }
}

/// Crystal `SealedInfo`（`Shared/Data/ItemData.cs:728`；读入 ctor `:735`，`Save` `:745`）。
///
/// `version > 92` 恒成立，所以 `NextSealDate` **总是**会被写出（两个 `long`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SealedInfo {
    /// 封印到期时间。
    pub expiry_date: DateTimeBinary,
    /// 下一次可封印时间。
    pub next_seal_date: DateTimeBinary,
}

impl SealedInfo {
    /// 解码：两个 `long`。
    ///
    /// 见 `Shared/Data/ItemData.cs:735`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        Ok(Self {
            expiry_date: DateTimeBinary(r.read_i64()?),
            next_seal_date: DateTimeBinary(r.read_i64()?),
        })
    }

    /// 编码：两个 `long`。
    ///
    /// 见 `Shared/Data/ItemData.cs:745`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i64(self.expiry_date.0);
        w.write_i64(self.next_seal_date.0);
    }
}

/// Crystal `RentalInformation`（`Shared/Data/ItemData.cs:752`；读入 ctor `:761`，`Save` `:769`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RentalInformation {
    /// 出租人角色名（`string`，7 位长度前缀）。
    pub owner_name: String,
    /// 绑定标志位（`short`，`BindMode`）。
    pub binding_flags: BindMode,
    /// 租期到期时间。
    pub expiry_date: DateTimeBinary,
    /// 是否锁定租借。
    pub rental_locked: bool,
}

impl RentalInformation {
    /// 解码：`string ownerName`、`short bindingFlags`、`long expiryDate`、`bool rentalLocked`。
    ///
    /// 见 `Shared/Data/ItemData.cs:761`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        Ok(Self {
            owner_name: r.read_string()?,
            binding_flags: BindMode::from_value(r.read_i16()?).unwrap_or_default(),
            expiry_date: DateTimeBinary(r.read_i64()?),
            rental_locked: r.read_bool()?,
        })
    }

    /// 编码：`string ownerName`、`short bindingFlags`、`long expiryDate`、`bool rentalLocked`。
    ///
    /// 见 `Shared/Data/ItemData.cs:769`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_string(&self.owner_name);
        w.write_i16(self.binding_flags.value());
        w.write_i64(self.expiry_date.0);
        w.write_bool(self.rental_locked);
    }
}

/// Crystal `UserItem`（`Shared/Data/ItemData.cs:277`；读入 ctor `:338`，`Save` `:460`）。
///
/// 字段顺序**完全**按 `Save`，并且只保留最新版本格式，详见模块文档。
///
/// 线格式（一行一字段，括号内是宽度）：
///
/// ```text
/// u64   UniqueID
/// i32   ItemIndex
/// u16   CurrentDura
/// u16   MaxDura
/// u16   Count                 <- version > 84 分支（旧版是 u32）
/// i32   SoulBoundId
/// u8    Bools                 <- bit0 Identified, bit1 Cursed
/// i32   Slots.Length
/// per slot: bool is_null + （非空时递归一个完整 UserItem）
/// u16   GemCount              <- version > 84 分支（旧版是 u32）
/// Stats AddedStats            <- i32 个数 + (u8 stat, i32 value)*
/// Awake Awake                 <- u8 类型 + i32 个数 + u8*
/// u8    RefinedValue
/// u8    RefineAdded
/// i32   RefineSuccessChance   <- version > 85 分支（恒成立）
/// i32   WeddingRing
/// bool  ExpireInfo 存在？     + （存在时 i64）
/// bool  RentalInformation 存在？ + （存在时 string, i16, i64, bool）
/// bool  IsShopItem
/// bool  SealedInfo 存在？     + （存在时 i64, i64）
/// bool  GMMade                <- version > 107 分支（恒成立）
/// ```
///
/// 注意 `Count`/`GemCount` 是 `ushort`、`Slots` 的空位用 `bool` 占位后**不写负载**，
/// 这两点是逐字节兼容的关键。
#[derive(Debug, Clone, PartialEq)]
pub struct UserItem {
    /// 物品实例的唯一编号。
    pub unique_id: u64,
    /// 物品模板编号（指向 `ItemInfo.Index`）。
    pub item_index: i32,
    /// 当前持久度。
    pub current_dura: u16,
    /// 最大持久度。
    pub max_dura: u16,
    /// 堆叠数量。
    pub count: u16,
    /// 绑定角色编号，`-1` 表示未绑定。
    pub soul_bound_id: i32,
    /// 是否已鉴定（`Bools` 的 bit0）。
    pub identified: bool,
    /// 是否被诅咒（`Bools` 的 bit1）。
    pub cursed: bool,
    /// 镶嵌槽；`None` 对应 C# 里的 `null` 槽（只写一个 `true` 占位）。
    pub slots: Vec<Option<UserItem>>,
    /// 宝石数量。
    pub gem_count: u16,
    /// 附加属性。
    pub added_stats: Stats,
    /// 觉醒信息。
    pub awake: Awake,
    /// 精炼属性类型。
    pub refined_value: RefinedValue,
    /// 精炼追加值。
    pub refine_added: u8,
    /// 精炼成功率。
    pub refine_success_chance: i32,
    /// 婚戒编号，`-1` 表示不是婚戒。
    pub wedding_ring: i32,
    /// 到期信息（`Save` 里以 `ExpireInfo != null` 作为存在标志）。
    pub expire_info: Option<ExpireInfo>,
    /// 租借信息（以 `RentalInformation != null` 作为存在标志）。
    pub rental_information: Option<RentalInformation>,
    /// 是否为商城道具。
    pub is_shop_item: bool,
    /// 封印信息（以 `SealedInfo != null` 作为存在标志）。
    pub sealed_info: Option<SealedInfo>,
    /// 是否为 GM 制造（`version > 107`，恒成立）。
    pub gmmade: bool,
}

impl Default for UserItem {
    /// 手写而不用 `derive`：Crystal 的字段初始值是
    /// `Count = 1`、`SoulBoundId = -1`、`WeddingRing = -1`（`ItemData.cs:284-296`），
    /// 直接派生会得到 0，导致新建物品的线格式与 Crystal 不一致。
    fn default() -> Self {
        Self {
            unique_id: 0,
            item_index: 0,
            current_dura: 0,
            max_dura: 0,
            count: 1,
            soul_bound_id: -1,
            identified: false,
            cursed: false,
            slots: Vec::new(),
            gem_count: 0,
            added_stats: Stats::default(),
            awake: Awake::default(),
            refined_value: RefinedValue::None,
            refine_added: 0,
            refine_success_chance: 0,
            wedding_ring: -1,
            expire_info: None,
            rental_information: None,
            is_shop_item: false,
            sealed_info: None,
            gmmade: false,
        }
    }
}

impl UserItem {
    /// 解码，见 `Shared/Data/ItemData.cs:338`（只走 `version = int.MaxValue` 分支）。
    ///
    /// 槽位是递归结构：理论上嵌套深度只受负载长度限制（Crystal 同样没有深度检查）。
    /// 每个非空槽至少还要消耗 40 余字节，`MAX_PACKET_LEN` 下深度有界，
    /// 但调用方仍应对不可信来源保持警惕。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let unique_id = r.read_u64()?;
        let item_index = r.read_i32()?;

        let current_dura = r.read_u16()?;
        let max_dura = r.read_u16()?;

        // `if (version <= 84) Count = (ushort)reader.ReadUInt32(); else ...`
        let count = r.read_u16()?;

        let soul_bound_id = r.read_i32()?;
        let bools = r.read_u8()?;
        let identified = bools & 0x01 != 0;
        let cursed = bools & 0x02 != 0;

        let slot_count = r.read_count()?;
        // 不预分配：长度来自不可信输入，`UserItem` 又比较大。
        let mut slots = Vec::new();
        for _ in 0..slot_count {
            if r.read_bool()? {
                // 空槽只占一个 bool。
                slots.push(None);
                continue;
            }
            slots.push(Some(Self::decode(r)?));
        }

        // `if (version <= 84) GemCount = (ushort)reader.ReadUInt32(); else ...`
        let gem_count = r.read_u16()?;

        // `if (version > 84) AddedStats = new Stats(reader, ...)`
        let added_stats = Stats::decode(r)?;

        let awake = Awake::decode(r)?;

        let refined_value = RefinedValue::from_value(r.read_u8()?).unwrap_or_default();
        let refine_added = r.read_u8()?;

        // `if (version > 85) RefineSuccessChance = reader.ReadInt32();`
        let refine_success_chance = r.read_i32()?;

        let wedding_ring = r.read_i32()?;

        // 之后是一串运行期的“存在标志”，与版本无关。
        let expire_info = if r.read_bool()? {
            Some(ExpireInfo::decode(r)?)
        } else {
            None
        };

        let rental_information = if r.read_bool()? {
            Some(RentalInformation::decode(r)?)
        } else {
            None
        };

        let is_shop_item = r.read_bool()?;

        let sealed_info = if r.read_bool()? {
            Some(SealedInfo::decode(r)?)
        } else {
            None
        };

        // `if (version > 107) GMMade = reader.ReadBoolean();`
        let gmmade = r.read_bool()?;

        Ok(Self {
            unique_id,
            item_index,
            current_dura,
            max_dura,
            count,
            soul_bound_id,
            identified,
            cursed,
            slots,
            gem_count,
            added_stats,
            awake,
            refined_value,
            refine_added,
            refine_success_chance,
            wedding_ring,
            expire_info,
            rental_information,
            is_shop_item,
            sealed_info,
            gmmade,
        })
    }

    /// 编码，见 `Shared/Data/ItemData.cs:460`。
    ///
    /// `Save` 本身没有版本分支，任何版本都会写出上面列出的全部字段。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_u64(self.unique_id);
        w.write_i32(self.item_index);

        w.write_u16(self.current_dura);
        w.write_u16(self.max_dura);

        w.write_u16(self.count);

        w.write_i32(self.soul_bound_id);
        let mut bools = 0u8;
        if self.identified {
            bools |= 0x01;
        }
        if self.cursed {
            bools |= 0x02;
        }
        w.write_u8(bools);

        w.write_count(self.slots.len());
        for slot in &self.slots {
            w.write_bool(slot.is_none());
            if let Some(item) = slot {
                item.encode(w);
            }
        }

        w.write_u16(self.gem_count);

        self.added_stats.encode(w);
        self.awake.encode(w);

        w.write_u8(self.refined_value.value());
        w.write_u8(self.refine_added);
        w.write_i32(self.refine_success_chance);

        w.write_i32(self.wedding_ring);

        w.write_bool(self.expire_info.is_some());
        if let Some(expire_info) = &self.expire_info {
            expire_info.encode(w);
        }

        w.write_bool(self.rental_information.is_some());
        if let Some(rental_information) = &self.rental_information {
            rental_information.encode(w);
        }

        w.write_bool(self.is_shop_item);

        w.write_bool(self.sealed_info.is_some());
        if let Some(sealed_info) = &self.sealed_info {
            sealed_info.encode(w);
        }

        w.write_bool(self.gmmade);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 编码后再解码，断言相等且负载被正好读完。
    fn roundtrip(item: &UserItem) {
        let mut writer = PacketWriter::new();
        item.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = UserItem::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, *item, "解码结果应与原值相同");
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn minimal_item_matches_hand_derived_bytes() {
        // 逐字段按 `Shared/Data/ItemData.cs:460` 的写出顺序推导出期望字节（全部小端）：
        //   u64 UniqueID            = 0x0A0B0C0D0E0F1011 -> 11 10 0F 0E 0D 0C 0B 0A
        //   i32 ItemIndex           = -2    -> FE FF FF FF
        //   u16 CurrentDura         = 10    -> 0A 00
        //   u16 MaxDura             = 20    -> 14 00
        //   u16 Count               = 1（默认值）-> 01 00
        //   i32 SoulBoundId         = -1（默认值）-> FF FF FF FF
        //   u8  Bools               = 0x01（仅 Identified）-> 01
        //   i32 Slots.Length        = 0     -> 00 00 00 00
        //   u16 GemCount            = 0     -> 00 00
        //   Stats AddedStats        = 空：i32 个数 0 -> 00 00 00 00
        //   Awake                   = u8 0x00 + i32 个数 0 -> 00 00 00 00 00
        //   u8  RefinedValue        = 0x00
        //   u8  RefineAdded         = 0x00
        //   i32 RefineSuccessChance = 0     -> 00 00 00 00
        //   i32 WeddingRing         = -1（默认值）-> FF FF FF FF
        //   bool ExpireInfo 存在     = 0x00（不存在，故无 i64）
        //   bool RentalInfo 存在     = 0x00（不存在）
        //   bool IsShopItem         = 0x00
        //   bool SealedInfo 存在     = 0x00（不存在）
        //   bool GMMade             = 0x00
        // 合计 8+4+2+2+2+4+1+4+2+4+5+1+1+4+4+1+1+1+1+1 = 53 字节。
        let item = UserItem {
            unique_id: 0x0A0B_0C0D_0E0F_1011,
            item_index: -2,
            current_dura: 10,
            max_dura: 20,
            identified: true,
            ..UserItem::default()
        };

        let mut writer = PacketWriter::new();
        item.encode(&mut writer);
        let bytes = writer.into_inner();

        let expected: Vec<u8> = vec![
            0x11, 0x10, 0x0F, 0x0E, 0x0D, 0x0C, 0x0B, 0x0A, // UniqueID
            0xFE, 0xFF, 0xFF, 0xFF, // ItemIndex = -2
            0x0A, 0x00, // CurrentDura
            0x14, 0x00, // MaxDura
            0x01, 0x00, // Count
            0xFF, 0xFF, 0xFF, 0xFF, // SoulBoundId
            0x01, // Bools
            0x00, 0x00, 0x00, 0x00, // Slots.Length
            0x00, 0x00, // GemCount
            0x00, 0x00, 0x00, 0x00, // AddedStats.Count
            0x00, // Awake.Type
            0x00, 0x00, 0x00, 0x00, // Awake 数值个数
            0x00, // RefinedValue
            0x00, // RefineAdded
            0x00, 0x00, 0x00, 0x00, // RefineSuccessChance
            0xFF, 0xFF, 0xFF, 0xFF, // WeddingRing
            0x00, // 无 ExpireInfo
            0x00, // 无 RentalInformation
            0x00, // IsShopItem
            0x00, // 无 SealedInfo
            0x00, // GMMade
        ];

        assert_eq!(bytes.len(), 53);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn counts_are_ushort_not_int() {
        // 关键回归点：Count / GemCount 是 ushort（2 字节），不是 int。
        let item = UserItem {
            count: 0x0102,
            gem_count: 0x0304,
            ..UserItem::default()
        };

        let mut writer = PacketWriter::new();
        item.encode(&mut writer);
        let bytes = writer.into_inner();

        // 8(UniqueID) + 4(ItemIndex) + 2(CurrentDura) + 2(MaxDura) = 16 起是 Count。
        assert_eq!(&bytes[16..18], &[0x02, 0x01]);
        // 再往后：SoulBoundId(4) + Bools(1) + Slots.Length(4) = 27 起是 GemCount。
        assert_eq!(&bytes[27..29], &[0x04, 0x03]);
        roundtrip(&item);
    }

    #[test]
    fn empty_slot_writes_only_a_bool() {
        // 3 个槽：Some / None / None；空槽只写 1 字节 true，不写任何负载。
        let item = UserItem {
            slots: vec![Some(UserItem::default()), None, None],
            ..UserItem::default()
        };
        roundtrip(&item);

        // 最小物品是 53 字节（其中 4 字节是 Slots.Length），
        // 再加：Some -> 1 字节 false + 53 字节负载，两个 None -> 各 1 字节 true。
        let mut writer = PacketWriter::new();
        item.encode(&mut writer);
        assert_eq!(writer.len(), 53 + (1 + 53) + 1 + 1);
    }

    #[test]
    fn slot_presence_flag_is_inverted_crystal_semantics() {
        // Crystal 写的是 `writer.Write(Slots[i] == null)`（ItemData.cs:476-483）：
        // 这个字节 **true 表示该槽位为空**，与背包/仓库数组的 `!= null` 语义相反。
        // 27 = u64 UniqueID + i32 ItemIndex + u16 CurrentDura + u16 MaxDura
        //      + u16 Count + i32 SoulBoundId + u8 Bools + i32 Slots.Length
        // 即第一个槽位的存在标志所在的下标。
        const FIRST_SLOT_FLAG: usize = 27;

        let mut empty_slot = PacketWriter::new();
        UserItem {
            slots: vec![None],
            ..UserItem::default()
        }
        .encode(&mut empty_slot);
        let bytes = empty_slot.into_inner();
        assert_eq!(bytes[FIRST_SLOT_FLAG], 1, "空槽必须写成 1");
        assert_eq!(&bytes[23..27], &[1, 0, 0, 0], "槽位个数是 1");
        // 空槽不再写任何负载：53 + 1 字节标志。
        assert_eq!(bytes.len(), 54);

        let mut filled_slot = PacketWriter::new();
        UserItem {
            slots: vec![Some(UserItem::default())],
            ..UserItem::default()
        }
        .encode(&mut filled_slot);
        let bytes = filled_slot.into_inner();
        assert_eq!(bytes[FIRST_SLOT_FLAG], 0, "非空槽必须写成 0");
        assert_eq!(bytes.len(), 54 + 53);

        // 解码方向：读到 1 就是空槽（None），读到 0 才继续读一个完整物品。
        let mut reader = PacketReader::new(&bytes);
        let decoded = UserItem::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded.slots, vec![Some(UserItem::default())]);
    }

    #[test]
    fn stats_keep_sorted_dictionary_order_and_drop_zero() {
        let mut stats = Stats::default();
        // 故意乱序插入。
        stats.set(Stat::Luck, -3);
        stats.set(Stat::HP, 100);
        stats.set(Stat::MaxDC, 5);
        stats.set(Stat::MinAC, 7);
        // 写 0 等于删除。
        stats.set(Stat::MinAC, 0);

        assert_eq!(stats.get(Stat::MinAC), 0);
        assert_eq!(stats.len(), 3);

        let mut writer = PacketWriter::new();
        stats.encode(&mut writer);
        let bytes = writer.into_inner();

        // 键按判别值升序：MaxDC(5) -> HP(12) -> Luck(15)。
        let expected: Vec<u8> = vec![
            0x03, 0x00, 0x00, 0x00, // count = 3
            5, 0x05, 0x00, 0x00, 0x00, // MaxDC = 5
            12, 0x64, 0x00, 0x00, 0x00, // HP = 100
            15, 0xFD, 0xFF, 0xFF, 0xFF, // Luck = -3
        ];
        assert_eq!(bytes, expected);

        let mut reader = PacketReader::new(&bytes);
        assert_eq!(Stats::decode(&mut reader).expect("解码应当成功"), stats);
    }

    #[test]
    fn stats_count_sums_absolute_values() {
        let stats: Stats = [(Stat::MaxDC, 5), (Stat::Luck, -3)].into_iter().collect();
        assert_eq!(stats.count(), 8);
        assert!(Stats::default().is_empty());
    }

    #[test]
    fn nested_item_roundtrip() {
        let child = UserItem {
            unique_id: 7,
            item_index: 4242,
            current_dura: 3,
            max_dura: 9,
            count: 2,
            soul_bound_id: 42,
            cursed: true,
            gem_count: 1,
            added_stats: [(Stat::MaxDC, 5), (Stat::Luck, -3)].into_iter().collect(),
            awake: Awake {
                awake_type: AwakeType::DC,
                values: vec![3, 1],
            },
            refined_value: RefinedValue::MC,
            refine_added: 2,
            refine_success_chance: 77,
            wedding_ring: 9,
            expire_info: Some(ExpireInfo {
                expiry_date: DateTimeBinary(0x1122_3344_5566_7788),
            }),
            rental_information: Some(RentalInformation {
                owner_name: "出租人".to_owned(),
                binding_flags: BindMode::DontTrade,
                expiry_date: DateTimeBinary(-5),
                rental_locked: true,
            }),
            is_shop_item: true,
            sealed_info: Some(SealedInfo {
                expiry_date: DateTimeBinary(1),
                next_seal_date: DateTimeBinary(2),
            }),
            gmmade: true,
            ..UserItem::default()
        };

        let root = UserItem {
            unique_id: u64::MAX,
            identified: true,
            count: 5,
            slots: vec![Some(child.clone()), None, Some(child)],
            ..UserItem::default()
        };

        roundtrip(&root);
    }

    #[test]
    fn optionals_absent_by_default() {
        let item = UserItem::default();
        assert!(item.expire_info.is_none());
        assert!(item.rental_information.is_none());
        assert!(item.sealed_info.is_none());
        roundtrip(&item);
    }

    #[test]
    fn awake_roundtrip() {
        let awake = Awake {
            awake_type: AwakeType::HPMP,
            values: vec![1, 2, 3, 4, 5],
        };
        let mut writer = PacketWriter::new();
        awake.encode(&mut writer);
        let bytes = writer.into_inner();
        // u8 类型 + i32 个数 + 5 个 u8
        assert_eq!(bytes[0], 6);
        assert_eq!(&bytes[1..5], &[5, 0, 0, 0]);
        assert_eq!(bytes.len(), 10);
        let mut reader = PacketReader::new(&bytes);
        assert_eq!(Awake::decode(&mut reader).expect("解码应当成功"), awake);
    }

    #[test]
    fn rental_information_roundtrip() {
        let rental = RentalInformation {
            owner_name: "Mir".to_owned(),
            binding_flags: BindMode::NoHero,
            expiry_date: DateTimeBinary(-1),
            rental_locked: false,
        };
        let mut writer = PacketWriter::new();
        rental.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes);
        assert_eq!(
            RentalInformation::decode(&mut reader).expect("解码应当成功"),
            rental
        );
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        UserItem::default().encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(UserItem::decode(&mut reader).is_err());
    }
}
