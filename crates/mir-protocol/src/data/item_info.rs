//! 物品模板 `ItemInfo` 的线格式。
//!
//! 对照 Crystal `Shared/Data/ItemData.cs:3`（读入 ctor `:69`，`Save` `:208`）手写，
//! 内嵌属性表 `Stats` 见 `data::user_item`（Crystal `Shared/Data/Stat.cs:1`）。
//!
//! **只实现最新版本分支。** Crystal 的读入 ctor
//! `ItemInfo(BinaryReader reader, int version = int.MaxValue, int customVersion = int.MaxValue)`
//! 带版本参数，而包体与 `GameShopItem` 的包路径都以缺省参数（`int.MaxValue`）调用，
//! 于是所有版本判断都静态可解：
//!
//! * `if (version <= 84)` 的旧版 `StackSize`（`u32`）与旧版扁平属性表**永不执行**；
//! * `if (version >= 77)` 的 `GlobalDropNotify` 位**总是执行**；
//! * `if (version > 83)` 的 `Slots` 与 `if (version > 84)` 的 `Stats` **总是执行**；
//! * `if (version < 70)` 的戒指旧数据修补**永不执行**。
//!
//! 所以本模块没有任何版本参数。唯一保留的运行期分支是
//! `ToolTip != null`（`ItemData.cs:193-197` / `:254-256`），对应 `Option<String>`。
//!
//! 线格式（一行一字段，括号内是宽度）：
//!
//! ```text
//! i32    Index
//! string Name                 <- 7 位长度前缀 + UTF-8
//! u8     Type                 ItemType
//! u8     Grade                ItemGrade
//! u8     RequiredType         RequiredType
//! u8     RequiredClass        RequiredClass
//! u8     RequiredGender       RequiredGender
//! u8     Set                  ItemSet
//! i16    Shape
//! u8     Weight
//! u8     Light
//! u8     RequiredAmount
//! u16    Image
//! u16    Durability
//! u16    StackSize            <- version > 84 分支（旧版是 u32）
//! u32    Price
//! bool   StartItem
//! u8     Effect
//! u8     Bools                <- bit0 NeedIdentify, bit1 ShowGroupPickup, bit2 ClassBased,
//!                                bit3 LevelBased, bit4 CanMine, bit5 GlobalDropNotify
//! i16    Bind                 BindMode（`[Flags]`）
//! i16    Unique               SpecialItemMode（`[Flags]`）
//! u8     RandomStatsId
//! bool   CanFastRun
//! bool   CanAwakening
//! u8     Slots                <- version > 83 分支（恒成立）
//! Stats  Stats                <- i32 个数 + (u8 stat, i32 value)*，键按判别值升序
//! bool   ToolTip 存在？
//! string ToolTip              <- 仅当上一位为 true
//! ```
//!
//! 不在线上传输、因此不进入本结构体的成员：`RandomStats`（`RandomItemStat` 实例，
//! `Save` 只写 `RandomStatsId`）、`IsConsumable` / `IsFishingRod` / `FriendlyName`
//! （派生的只读属性）以及 `FromText` / `ToText`（物品数据库的文本导入导出，未实现）。
//!
//! `Bind` / `Unique` / `RequiredClass` 在 C# 里都是 `[Flags]` 枚举，位可以组合
//! （例如 `DontDrop | DontSell` = 6、`Warrior | Wizard` = 3）。`mir_core::enums` 的
//! `from_value` 只接受单个声明值，未声明的组合值会按项目约定退化为
//! `unwrap_or_default()`。若要逐位保真，需要先在 `mir-core` 侧补一个位集表示；
//! 现有已实现类型（`data::user_item`）同样采用这一约定，此处保持一致。

use mir_core::enums::{
    BindMode, ItemGrade, ItemSet, ItemType, RequiredClass, RequiredGender, RequiredType,
    SpecialItemMode,
};

use crate::data::Stats;
use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `ItemInfo`（`Shared/Data/ItemData.cs:3`；读入 ctor `:69`，`Save` `:208`）。
///
/// 字段顺序完全按 `Save` 的写出顺序排列，布局细节见模块文档。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ItemInfo {
    /// 物品模板编号。
    pub index: i32,
    /// 物品名。
    pub name: String,
    /// 物品类型。
    pub item_type: ItemType,
    /// 品级。
    pub grade: ItemGrade,
    /// 需求属性的种类。
    pub required_type: RequiredType,
    /// 需求职业（`[Flags]`）。
    pub required_class: RequiredClass,
    /// 需求性别。
    pub required_gender: RequiredGender,
    /// 套装编号。
    pub set: ItemSet,
    /// 外观编号。
    pub shape: i16,
    /// 重量。
    pub weight: u8,
    /// 光照范围。
    pub light: u8,
    /// 需求数值（与 `RequiredType` 搭配）。
    pub required_amount: u8,
    /// 背包图标编号。
    pub image: u16,
    /// 最大持久度。
    pub durability: u16,
    /// 堆叠上限（`ushort`，不是 `int`）。
    pub stack_size: u16,
    /// 价格（金币）。
    pub price: u32,
    /// 是否为新手初始装备。
    pub start_item: bool,
    /// 特效编号。
    pub effect: u8,
    /// 是否需要鉴定（`Bools` 的 bit0）。
    pub need_identify: bool,
    /// 是否允许组队拾取提示（bit1）。
    pub show_group_pickup: bool,
    /// 属性是否随职业变化（bit2）。
    pub class_based: bool,
    /// 属性是否随等级变化（bit3）。
    pub level_based: bool,
    /// 是否可以挖矿（bit4）。
    pub can_mine: bool,
    /// 是否全服掉落公告（bit5；`version >= 77` 恒成立）。
    pub global_drop_notify: bool,
    /// 绑定规则（`short`，`[Flags]`）。
    pub bind: BindMode,
    /// 特殊效果（`short`，`[Flags]`）。
    pub unique: SpecialItemMode,
    /// 随机属性模板编号（`RandomStats` 本身不上线）。
    pub random_stats_id: u8,
    /// 是否允许疾跑。
    pub can_fast_run: bool,
    /// 是否可觉醒。
    pub can_awakening: bool,
    /// 镶嵌槽数量。
    pub slots: u8,
    /// 基础属性表。
    pub stats: Stats,
    /// 悬浮说明；`None` 对应 C# 的 `null`（此时只写一个 `false` 标志位）。
    pub tooltip: Option<String>,
}

impl Default for ItemInfo {
    /// 手写而不用 `derive`：Crystal 的字段初始值不是全零（`ItemData.cs:5-41`）。
    ///
    /// * `RequiredType = Level`（0，与派生默认值相同）；
    /// * `RequiredClass = None`（31）与 `RequiredGender = None`（3）——
    ///   Rust 侧这两个枚举的 `#[default]` 是 `Warrior`(1) / `Male`(1)，
    ///   派生会写出与 Crystal 不同的字节；
    /// * `StackSize = 1`；
    /// * `ToolTip = string.Empty`，即**非 null 的空串**：Crystal 新建的物品模板
    ///   在没显式设置说明时，`Save` 也会写出 `true` + 空串（`:254-256`），
    ///   所以默认值取 `Some(String::new())` 而不是 `None`。
    fn default() -> Self {
        Self {
            index: 0,
            name: String::new(),
            item_type: ItemType::Nothing,
            grade: ItemGrade::None,
            required_type: RequiredType::Level,
            required_class: RequiredClass::None,
            required_gender: RequiredGender::None,
            set: ItemSet::None,
            shape: 0,
            weight: 0,
            light: 0,
            required_amount: 0,
            image: 0,
            durability: 0,
            stack_size: 1,
            price: 0,
            start_item: false,
            effect: 0,
            need_identify: false,
            show_group_pickup: false,
            class_based: false,
            level_based: false,
            can_mine: false,
            global_drop_notify: false,
            bind: BindMode::None,
            unique: SpecialItemMode::None,
            random_stats_id: 0,
            can_fast_run: false,
            can_awakening: false,
            slots: 0,
            stats: Stats::default(),
            tooltip: Some(String::new()),
        }
    }
}

impl ItemInfo {
    /// 解码，见 `Shared/Data/ItemData.cs:69`（只走 `version = int.MaxValue` 分支）。
    ///
    /// 逐字节顺序与 `Save` 完全一致：ctor 里所有 `version <= 84` 的旧字段都已删除，
    /// `version >= 77` / `version > 83` / `version > 84` 的判断恒成立，直接展开。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let index = r.read_i32()?;
        let name = r.read_string()?;
        let item_type = ItemType::from_value(r.read_u8()?).unwrap_or_default();
        let grade = ItemGrade::from_value(r.read_u8()?).unwrap_or_default();
        let required_type = RequiredType::from_value(r.read_u8()?).unwrap_or_default();
        let required_class = RequiredClass::from_value(r.read_u8()?).unwrap_or_default();
        let required_gender = RequiredGender::from_value(r.read_u8()?).unwrap_or_default();
        let set = ItemSet::from_value(r.read_u8()?).unwrap_or_default();

        let shape = r.read_i16()?;
        let weight = r.read_u8()?;
        let light = r.read_u8()?;
        let required_amount = r.read_u8()?;

        let image = r.read_u16()?;
        let durability = r.read_u16()?;

        // `if (version <= 84) StackSize = (ushort)reader.ReadUInt32(); else ...`
        let stack_size = r.read_u16()?;
        let price = r.read_u32()?;

        let start_item = r.read_bool()?;
        let effect = r.read_u8()?;

        let bools = r.read_u8()?;
        let need_identify = bools & 0x01 == 0x01;
        let show_group_pickup = bools & 0x02 == 0x02;
        let class_based = bools & 0x04 == 0x04;
        let level_based = bools & 0x08 == 0x08;
        let can_mine = bools & 0x10 == 0x10;
        // `if (version >= 77) GlobalDropNotify = (bools & 0x20) == 0x20;`
        let global_drop_notify = bools & 0x20 == 0x20;

        let bind = BindMode::from_value(r.read_i16()?).unwrap_or_default();
        let unique = SpecialItemMode::from_value(r.read_i16()?).unwrap_or_default();
        let random_stats_id = r.read_u8()?;
        let can_fast_run = r.read_bool()?;
        let can_awakening = r.read_bool()?;
        // `if (version > 83) Slots = reader.ReadByte();`
        let slots = r.read_u8()?;

        // `if (version > 84) Stats = new Stats(reader, version, customVersion);`
        let stats = Stats::decode(r)?;

        // 运行期分支：`bool isTooltip = reader.ReadBoolean();`
        let tooltip = if r.read_bool()? {
            Some(r.read_string()?)
        } else {
            None
        };

        Ok(Self {
            index,
            name,
            item_type,
            grade,
            required_type,
            required_class,
            required_gender,
            set,
            shape,
            weight,
            light,
            required_amount,
            image,
            durability,
            stack_size,
            price,
            start_item,
            effect,
            need_identify,
            show_group_pickup,
            class_based,
            level_based,
            can_mine,
            global_drop_notify,
            bind,
            unique,
            random_stats_id,
            can_fast_run,
            can_awakening,
            slots,
            stats,
            tooltip,
        })
    }

    /// 编码，见 `Shared/Data/ItemData.cs:208`。
    ///
    /// `Save` 没有任何版本分支，所以这里就是全部字段的顺序。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i32(self.index);
        w.write_string(&self.name);
        w.write_u8(self.item_type.value());
        w.write_u8(self.grade.value());
        w.write_u8(self.required_type.value());
        w.write_u8(self.required_class.value());
        w.write_u8(self.required_gender.value());
        w.write_u8(self.set.value());

        w.write_i16(self.shape);
        w.write_u8(self.weight);
        w.write_u8(self.light);
        w.write_u8(self.required_amount);

        w.write_u16(self.image);
        w.write_u16(self.durability);

        w.write_u16(self.stack_size);
        w.write_u32(self.price);

        w.write_bool(self.start_item);

        w.write_u8(self.effect);

        // `Save` 把 6 个布尔压进一个字节（`ItemData.cs:234-241`）。
        let mut bools = 0u8;
        if self.need_identify {
            bools |= 0x01;
        }
        if self.show_group_pickup {
            bools |= 0x02;
        }
        if self.class_based {
            bools |= 0x04;
        }
        if self.level_based {
            bools |= 0x08;
        }
        if self.can_mine {
            bools |= 0x10;
        }
        if self.global_drop_notify {
            bools |= 0x20;
        }
        w.write_u8(bools);

        // 两个 `[Flags]` 按 `short` 写出，不是 `byte`。
        w.write_i16(self.bind.value());
        w.write_i16(self.unique.value());

        w.write_u8(self.random_stats_id);

        w.write_bool(self.can_fast_run);
        w.write_bool(self.can_awakening);
        w.write_u8(self.slots);

        self.stats.encode(w);

        // `ToolTip != null` 才是存在标志：空串也算存在（`ItemData.cs:254-256`）。
        w.write_bool(self.tooltip.is_some());
        if let Some(tooltip) = &self.tooltip {
            w.write_string(tooltip);
        }
    }
}

#[cfg(test)]
mod tests {
    use mir_core::enums::Stat;

    use super::*;

    fn roundtrip(value: &ItemInfo) {
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = ItemInfo::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, *value, "解码结果应与原值相同");
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    /// 一个每个字段都区别于默认值的样本。
    fn sample() -> ItemInfo {
        ItemInfo {
            index: 1234,
            name: "屠龙".to_owned(),
            item_type: ItemType::Weapon,
            grade: ItemGrade::Legendary,
            required_type: RequiredType::MaxDC,
            required_class: RequiredClass::WarWizTao,
            required_gender: RequiredGender::Male,
            set: ItemSet::Spirit,
            shape: -3,
            weight: 40,
            light: 2,
            required_amount: 33,
            image: 0x1234,
            durability: 0x5678,
            stack_size: 1,
            price: 123_456,
            start_item: true,
            effect: 9,
            need_identify: true,
            show_group_pickup: true,
            class_based: true,
            level_based: true,
            can_mine: true,
            global_drop_notify: true,
            bind: BindMode::DontTrade,
            unique: SpecialItemMode::Teleport,
            random_stats_id: 7,
            can_fast_run: true,
            can_awakening: true,
            slots: 3,
            stats: [(Stat::MaxDC, 40), (Stat::Luck, -3)].into_iter().collect(),
            tooltip: Some("仅限战士".to_owned()),
        }
    }

    #[test]
    fn roundtrip_with_all_field_kinds() {
        roundtrip(&sample());
    }

    #[test]
    fn absent_tooltip_costs_one_byte() {
        // `ToolTip == null` 时 `Save` 只写一个 false（`ItemData.cs:254-256`）。
        let with_tooltip = sample();
        let mut without = sample();
        without.tooltip = None;

        let mut writer = PacketWriter::new();
        without.encode(&mut writer);
        let without_bytes = writer.into_inner();

        let mut writer = PacketWriter::new();
        with_tooltip.encode(&mut writer);
        let with_bytes = writer.into_inner();

        // 差异正好是「true + 7 位长度前缀 + UTF-8 负载」减去「false」。
        // "仅限战士" 是 4 个汉字 = 12 字节。
        assert_eq!(with_bytes.len(), without_bytes.len() + 1 + 12);
        roundtrip(&without);
        roundtrip(&with_tooltip);
    }

    #[test]
    fn bool_byte_and_flag_offsets() {
        // 前缀偏移推导（全部小端、`name` 为空串）：
        //   i32 Index              4  -> 0..4
        //   string Name（空）       1  -> 4..5
        //   6 个 u8 枚举            6  -> 5..11
        //   i16 Shape + 3 个 u8    5  -> 11..16
        //   2 个 u16               4  -> 16..20
        //   u16 StackSize          2  -> 20..22
        //   u32 Price              4  -> 22..26
        //   bool StartItem         1  -> 26..27
        //   u8 Effect              1  -> 27..28
        //   u8 Bools               1  -> 28..29
        //   i16 Bind               2  -> 29..31
        //   i16 Unique             2  -> 31..33
        let item = ItemInfo {
            need_identify: true,
            class_based: true,
            global_drop_notify: true,
            bind: BindMode::DontDrop,
            unique: SpecialItemMode::ClearRing,
            ..ItemInfo::default()
        };

        let mut writer = PacketWriter::new();
        item.encode(&mut writer);
        let bytes = writer.into_inner();

        assert_eq!(bytes[4], 0x00, "空名字只有 1 字节长度前缀");
        // bit0 NeedIdentify + bit2 ClassBased + bit5 GlobalDropNotify = 0x25。
        assert_eq!(bytes[28], 0x25, "Bools 的位布局");
        assert_eq!(&bytes[29..31], &[0x02, 0x00], "Bind = DontDrop(2) 是 i16");
        assert_eq!(
            &bytes[31..33],
            &[0x04, 0x00],
            "Unique = ClearRing(4) 是 i16"
        );
        // 默认 `StackSize = 1`、`ToolTip = Some("")`。
        assert_eq!(&bytes[20..22], &[0x01, 0x00], "StackSize 默认 1");
        assert_eq!(bytes[33], 0x00, "RandomStatsId");
        assert_eq!(bytes[34], 0x00, "CanFastRun");
        assert_eq!(bytes[35], 0x00, "CanAwakening");
        assert_eq!(bytes[36], 0x00, "Slots");
        // Stats 为空：i32 个数 0。
        assert_eq!(&bytes[37..41], &[0x00, 0x00, 0x00, 0x00]);
        // 最后是 ToolTip 存在标志（true，因为默认是空串而非 null），随后长度前缀 0。
        assert_eq!(bytes[41], 0x01);
        assert_eq!(bytes[42], 0x00);
        assert_eq!(bytes.len(), 43);
        roundtrip(&item);
    }

    #[test]
    fn default_matches_crystal_field_initializers() {
        // `ItemData.cs:5-41`：RequiredType = Level、RequiredClass = None、
        // RequiredGender = None、StackSize = 1、ToolTip = string.Empty。
        let item = ItemInfo::default();
        assert_eq!(item.required_type, RequiredType::Level);
        assert_eq!(item.required_class, RequiredClass::None);
        assert_eq!(item.required_gender, RequiredGender::None);
        assert_eq!(item.stack_size, 1);
        assert_eq!(item.tooltip, Some(String::new()));
        roundtrip(&item);
    }

    #[test]
    fn unknown_enum_bytes_fall_back_to_defaults() {
        // 一个只有长度、没有内容的极简负载：未知枚举值回退到 `from_value` 的默认。
        let bytes: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x00, // Index
            0x00, // Name = ""
            0xFE, // Type = 254（未知）
            0xFE, // Grade = 254（未知）
            0xFE, // RequiredType = 254（未知）
            0x00, // RequiredClass = 0（未声明）
            0x00, // RequiredGender = 0（未声明）
            0xFE, // Set = 254（未知）
            0x00, 0x00, // Shape
            0x00, 0x00, 0x00, // Weight/Light/RequiredAmount
            0x00, 0x00, 0x00, 0x00, // Image/Durability
            0x00, 0x00, // StackSize
            0x00, 0x00, 0x00, 0x00, // Price
            0x00, // StartItem
            0x00, // Effect
            0x00, // Bools
            0x00, 0x00, // Bind
            0x00, 0x00, // Unique
            0x00, // RandomStatsId
            0x00, 0x00, // CanFastRun/CanAwakening
            0x00, // Slots
            0x00, 0x00, 0x00, 0x00, // Stats 个数 0
            0x00, // 无 ToolTip
        ];
        let mut reader = PacketReader::new(&bytes);
        let decoded = ItemInfo::decode(&mut reader).expect("解码应当成功");
        assert!(reader.is_empty());
        assert_eq!(decoded.item_type, ItemType::default());
        assert_eq!(decoded.grade, ItemGrade::default());
        assert_eq!(decoded.required_type, RequiredType::default());
        assert_eq!(decoded.required_class, RequiredClass::default());
        assert_eq!(decoded.required_gender, RequiredGender::default());
        assert_eq!(decoded.set, ItemSet::default());
        assert!(decoded.tooltip.is_none());
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(ItemInfo::decode(&mut reader).is_err());
    }
}
