//! 物品模板段与商城段。
//!
//! 两段都**复用 `mir-protocol` 的类型**，因此本模块只补版本门：
//!
//! | 段 | 复用类型 | 现代路径 |
//! | --- | --- | --- |
//! | 物品模板 | [`mir_protocol::data::ItemInfo`]（`Shared/Data/ItemData.cs:3`） | `version > 84` 时与包路径逐字节相同，直接调 `ItemInfo::decode` |
//! | 商城 | [`mir_protocol::data::GameShopItem`]（`ItemData.cs:778`） | `version > 105` 时直接调 `GameShopItem::decode_db` |
//!
//! 为什么需要自己写一份解码：包路径的 `decode` 固定按 `version = int.MaxValue` 展开
//! （见 `crates/mir-protocol/src/data/item_info.rs:6-17` 的说明），而数据库文件必须按
//! `version` 分支读。`<= 84` 的旧文件把属性写成**固定字段块**而不是字典
//! （`ItemData.cs:99-119`、`:123-174`），但值最终都落进同一个 `Stats` 容器，
//! 所以复用类型完全够用——不需要在 `mir-db` 里再造一个 `ItemInfo`。
//!
//! `GameShopItem` 的旧格式差异只有两处：`<= 84` 的 `Count` 是 `u32`（`ItemData.cs:806-813`），
//! `> 105` 才有 `CanBuyCredit` / `CanBuyGold`（`:821-825`）。

use mir_core::DateTimeBinary;
use mir_core::enums::{
    BindMode, ItemGrade, ItemSet, ItemType, RequiredClass, RequiredGender, RequiredType,
    SpecialItemMode, Stat,
};
use mir_protocol::data::{GameShopItem, ItemInfo, Stats};

use crate::MirDbError;
use crate::reader::DbReader;

/// 一个 `ItemInfo` 记录的最小字节数（现代布局：空名字、空属性表、无 tooltip 内容）。
pub const MIN_ITEM_LEN: usize = 42;

/// 一个 `GameShopItem` 记录的最小字节数（两个空分类名）。
pub const MIN_GAME_SHOP_LEN: usize = 35;

/// 解码一条物品模板，见 `Shared/Data/ItemData.cs:69-204`。
///
/// * `version > 84`：与包路径完全一致，直接复用 [`ItemInfo::decode`]（`ItemData.cs:88-197`
///   里所有 `<= 84` 分支都不执行，`> 83` / `> 84` 分支恒成立）。
/// * `version <= 84`：走 [`decode_legacy`]，把固定属性块塞进同一个 `Stats`。
///
/// 未实现的一条**载入后修正**：`version < 70` 时 C# 会给戒指补上
/// `BindMode.NoWeddingRing`（`ItemData.cs:199-203`）。那是规则层的修补而不是字节格式，
/// 且 `mir-protocol` 的 `BindMode` 只保留已声明的枚举值（组合位会退化为默认值），
/// 所以这里不做，留给服务层在导入后按版本处理。
pub fn decode_item(r: &mut DbReader<'_>) -> Result<ItemInfo, MirDbError> {
    if r.version() > 84 {
        return ItemInfo::decode(r.packet()).map_err(|error| r.truncated(error));
    }
    decode_legacy_item(r)
}

/// 解码一条商城商品，见 `Shared/Data/ItemData.cs:800-827`。
///
/// * `version > 105`：复用 [`GameShopItem::decode_db`]（与物品库路径完全一致）。
/// * `version <= 105`：`CanBuyCredit` / `CanBuyGold` 不存在，读成 `false`；
///   `<= 84` 的 `Count` 是 `u32`，C# 会截断成 `ushort`（`ItemData.cs:808`）。
///
/// `Info` 字段不落盘（库里只有 `ItemIndex` 引用），与 `decode_db` 一样保持默认值。
pub fn decode_game_shop(r: &mut DbReader<'_>) -> Result<GameShopItem, MirDbError> {
    if r.version() > 105 {
        return GameShopItem::decode_db(r.packet()).map_err(|error| r.truncated(error));
    }

    let item_index = r.i32()?;
    let g_index = r.i32()?;
    let gold_price = r.u32()?;
    let credit_price = r.u32()?;
    let count = if r.version() <= 84 {
        r.u32()? as u16
    } else {
        r.u16()?
    };
    let class = r.string()?;
    let category = r.string()?;
    let stock = r.i32()?;
    let i_stock = r.bool()?;
    let deal = r.bool()?;
    let top_item = r.bool()?;
    let date = DateTimeBinary(r.i64()?);

    Ok(GameShopItem {
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
        can_buy_credit: false,
        can_buy_gold: false,
    })
}

/// `<= 84` 的旧格式物品模板，见 `ItemData.cs:69-204` 里所有 `version <= 84` 分支。
///
/// 与新版唯一的实质差别是属性：旧版按固定顺序写 15 个带符号/无符号的窄整数，
/// 新版写「`i32` 项数 + (u8 键, i32 值)」。旧版的 15 个值最终都进同一个 `Stats`，
/// 键与新版一致（`ItemData.cs:102-118`、`:125-143`、`:161-173`）。
fn decode_legacy_item(r: &mut DbReader<'_>) -> Result<ItemInfo, MirDbError> {
    let index = r.i32()?;
    let name = r.string()?;
    let item_type = ItemType::from_value(r.u8()?).unwrap_or_default();
    let grade = ItemGrade::from_value(r.u8()?).unwrap_or_default();
    let required_type = RequiredType::from_value(r.u8()?).unwrap_or_default();
    let required_class = RequiredClass::from_value(r.u8()?).unwrap_or_default();
    let required_gender = RequiredGender::from_value(r.u8()?).unwrap_or_default();
    let set = ItemSet::from_value(r.u8()?).unwrap_or_default();

    let shape = r.i16()?;
    let weight = r.u8()?;
    let light = r.u8()?;
    let required_amount = r.u8()?;

    let image = r.u16()?;
    let durability = r.u16()?;

    // `StackSize = (ushort)reader.ReadUInt32();`（`ItemData.cs:90`）
    let stack_size = r.u32()? as u16;
    let price = r.u32()?;

    // 旧版固定属性块（`ItemData.cs:99-119`）。
    let mut stats = Stats::default();
    for stat in [
        Stat::MinAC,
        Stat::MaxAC,
        Stat::MinMAC,
        Stat::MaxMAC,
        Stat::MinDC,
        Stat::MaxDC,
        Stat::MinMC,
        Stat::MaxMC,
        Stat::MinSC,
        Stat::MaxSC,
    ] {
        stats.set(stat, i32::from(r.u8()?));
    }
    set_u16_stat(r, &mut stats, Stat::HP)?;
    set_u16_stat(r, &mut stats, Stat::MP)?;
    set_u8_stat(r, &mut stats, Stat::Accuracy)?;
    set_u8_stat(r, &mut stats, Stat::Agility)?;
    // 这两个是 **sbyte**（`ItemData.cs:117-118`）。
    stats.set(Stat::Luck, i32::from(r.i8()?));
    stats.set(Stat::AttackSpeed, i32::from(r.i8()?));

    let start_item = r.bool()?;

    // `if (version <= 84) { BagWeight; HandWeight; WearWeight; }`（`ItemData.cs:123-128`）
    set_u8_stat(r, &mut stats, Stat::BagWeight)?;
    set_u8_stat(r, &mut stats, Stat::HandWeight)?;
    set_u8_stat(r, &mut stats, Stat::WearWeight)?;

    let effect = r.u8()?;

    // 旧版属性块 2（`ItemData.cs:132-144`）。
    for stat in [
        Stat::Strong,
        Stat::MagicResist,
        Stat::PoisonResist,
        Stat::HealthRecovery,
        Stat::SpellRecovery,
        Stat::PoisonRecovery,
        Stat::HPRatePercent,
        Stat::MPRatePercent,
        Stat::CriticalRate,
        Stat::CriticalDamage,
    ] {
        set_u8_stat(r, &mut stats, stat)?;
    }

    // `byte bools = reader.ReadByte();`（`ItemData.cs:147`）
    let bools = r.u8()?;
    let need_identify = bools & 0x01 == 0x01;
    let show_group_pickup = bools & 0x02 == 0x02;
    let class_based = bools & 0x04 == 0x04;
    let level_based = bools & 0x08 == 0x08;
    let can_mine = bools & 0x10 == 0x10;
    // `if (version >= 77) GlobalDropNotify = (bools & 0x20) == 0x20;`（`ItemData.cs:154-157`）
    let global_drop_notify = r.version() >= 77 && bools & 0x20 == 0x20;

    // 旧版属性块 3（`ItemData.cs:159-166`）。
    for stat in [
        Stat::MaxACRatePercent,
        Stat::MaxMACRatePercent,
        Stat::Holy,
        Stat::Freezing,
        Stat::PoisonAttack,
    ] {
        set_u8_stat(r, &mut stats, stat)?;
    }

    let bind = BindMode::from_value(r.i16()?).unwrap_or_default();

    // 旧版属性块 4（`ItemData.cs:170-174`）。
    set_u8_stat(r, &mut stats, Stat::Reflect)?;
    set_u8_stat(r, &mut stats, Stat::HPDrainRatePercent)?;

    let unique = SpecialItemMode::from_value(r.i16()?).unwrap_or_default();
    let random_stats_id = r.u8()?;
    let can_fast_run = r.bool()?;
    let can_awakening = r.bool()?;

    // `if (version > 83) Slots = reader.ReadByte();`（`ItemData.cs:183-186`）
    let slots = if r.version() > 83 { r.u8()? } else { 0 };

    // `bool isTooltip = reader.ReadBoolean(); if (isTooltip) ToolTip = reader.ReadString();`
    // （`ItemData.cs:193-197`）
    let tooltip = if r.bool()? { Some(r.string()?) } else { None };

    Ok(ItemInfo {
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

/// 读一个 `u8` 属性并写入 `stats`（`Stat` 索引器语义：值 0 等于删除键）。
fn set_u8_stat(r: &mut DbReader<'_>, stats: &mut Stats, stat: Stat) -> Result<(), MirDbError> {
    stats.set(stat, i32::from(r.u8()?));
    Ok(())
}

/// 读一个 `u16` 属性并写入 `stats`。
fn set_u16_stat(r: &mut DbReader<'_>, stats: &mut Stats, stat: Stat) -> Result<(), MirDbError> {
    stats.set(stat, i32::from(r.u16()?));
    Ok(())
}

#[cfg(test)]
mod tests {
    use mir_protocol::{PacketWriter, data::ItemInfo as PacketItemInfo};

    use super::*;
    use crate::reader::DbWriter;

    /// 拼一条 `version <= 84` 的物品模板（`ItemData.cs:69-204` 的旧分支）。
    fn legacy_item_bytes(version: i32, slots: u8) -> Vec<u8> {
        let mut w = DbWriter::new();
        w.i32(7); // Index
        w.string("祈祷之剑"); // Name
        w.u8(1); // Type
        w.u8(2); // Grade
        w.u8(0); // RequiredType
        w.u8(31); // RequiredClass
        w.u8(3); // RequiredGender
        w.u8(0); // Set
        w.i16(13); // Shape
        w.u8(10); // Weight
        w.u8(2); // Light
        w.u8(20); // RequiredAmount
        w.u16(54); // Image
        w.u16(20_000); // Durability
        w.u32(1); // StackSize（u32）
        w.u32(30_000); // Price
        for value in [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10] {
            w.u8(value); // MinAC..MaxSC
        }
        w.u16(100); // HP
        w.u16(50); // MP
        w.u8(3); // Accuracy
        w.u8(4); // Agility
        w.i8(-2); // Luck（sbyte）
        w.i8(-1); // AttackSpeed（sbyte）
        w.bool(true); // StartItem
        w.u8(1); // BagWeight
        w.u8(2); // HandWeight
        w.u8(3); // WearWeight
        w.u8(9); // Effect
        for value in [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10] {
            w.u8(value); // Strong..CriticalDamage
        }
        w.u8(0x25); // NeedIdentify | ClassBased | GlobalDropNotify
        for value in [1u8, 2, 3, 4, 5] {
            w.u8(value); // MaxACRatePercent..PoisonAttack
        }
        w.i16(2); // Bind = DontDrop
        w.u8(6); // Reflect
        w.u8(7); // HPDrainRatePercent
        w.i16(0); // Unique
        w.u8(1); // RandomStatsId
        w.bool(false); // CanFastRun
        w.bool(true); // CanAwakening
        if version > 83 {
            w.u8(slots); // Slots
        }
        w.bool(true); // ToolTip 存在
        w.string(""); // ToolTip
        w.into_inner()
    }

    #[test]
    fn legacy_item_fixed_block_lands_in_stats() {
        // v84：读 `Slots`（`ItemData.cs:183-186`），属性仍是固定块。
        let bytes = legacy_item_bytes(84, 5);
        let mut reader = DbReader::new(&bytes, 84);
        let item = decode_item(&mut reader).expect("解码应当成功");
        assert_eq!(reader.remaining(), 0);

        assert_eq!(item.index, 7);
        assert_eq!(item.name, "祈祷之剑");
        assert_eq!(item.shape, 13, "Shape 是 i16");
        assert_eq!(item.stack_size, 1, "旧版 StackSize 是 u32");
        assert_eq!(item.slots, 5);
        assert_eq!(item.stats.get(Stat::MinAC), 1);
        assert_eq!(item.stats.get(Stat::MaxSC), 10);
        assert_eq!(item.stats.get(Stat::HP), 100);
        assert_eq!(item.stats.get(Stat::MP), 50);
        assert_eq!(item.stats.get(Stat::Luck), -2, "Luck 是 sbyte");
        assert_eq!(
            item.stats.get(Stat::AttackSpeed),
            -1,
            "AttackSpeed 是 sbyte"
        );
        assert_eq!(item.stats.get(Stat::BagWeight), 1);
        assert_eq!(item.stats.get(Stat::HPDrainRatePercent), 7);
        assert!(item.need_identify && item.class_based && item.global_drop_notify);
        assert!(!item.show_group_pickup && !item.level_based && !item.can_mine);
        assert_eq!(item.bind, BindMode::from_value(2).unwrap_or_default());
        assert_eq!(item.tooltip.as_deref(), Some(""));
    }

    #[test]
    fn legacy_slots_gate_is_84() {
        // `if (version > 83)`：只有 84 版才读 `Slots`。
        let bytes = legacy_item_bytes(83, 5);
        let mut reader = DbReader::new(&bytes, 83);
        let item = decode_item(&mut reader).expect("解码应当成功");
        assert_eq!(reader.remaining(), 0, "v83 不读 Slots，尾巴应当正好读完");
        assert_eq!(item.slots, 0);
        assert!(item.global_drop_notify, "v83 >= 77，这一位存在");
    }

    #[test]
    fn modern_item_reuses_packet_decoder() {
        // 现代布局与包路径逐字节相同，解码结果应与 `ItemInfo::decode` 一致。
        let value = PacketItemInfo {
            index: 42,
            name: "屠龙".to_owned(),
            stats: [(Stat::MaxDC, 40), (Stat::Luck, -3)].into_iter().collect(),
            tooltip: Some("极品".to_owned()),
            ..PacketItemInfo::default()
        };
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = DbReader::new(&bytes, 105);
        let decoded = decode_item(&mut reader).expect("解码应当成功");
        assert_eq!(reader.remaining(), 0);
        assert_eq!(decoded, value);
    }

    /// 拼一条 `version <= 105` 的商城商品（旧版 `Count` 可能是 `u32`）。
    fn legacy_game_shop_bytes(version: i32) -> Vec<u8> {
        let mut w = DbWriter::new();
        w.i32(1_268); // ItemIndex
        w.i32(1); // GIndex
        w.u32(82_500); // GoldPrice
        w.u32(220); // CreditPrice
        if version <= 84 {
            w.u32(1); // Count（u32）
        } else {
            w.u16(1); // Count（u16）
        }
        w.string("All"); // Class
        w.string("Potion"); // Category
        w.i32(0); // Stock
        w.bool(false); // iStock
        w.bool(false); // Deal
        w.bool(true); // TopItem
        w.i64(DateTimeBinary::default().0); // Date
        w.into_inner()
    }

    #[test]
    fn game_shop_legacy_paths() {
        for version in [83, 84, 88, 105] {
            let bytes = legacy_game_shop_bytes(version);
            let mut reader = DbReader::new(&bytes, version);
            let shop = decode_game_shop(&mut reader).expect("解码应当成功");
            assert_eq!(reader.remaining(), 0, "版本 {version}");
            assert_eq!(shop.item_index, 1_268);
            assert_eq!(shop.count, 1);
            assert_eq!(shop.class, "All");
            assert_eq!(shop.category, "Potion");
            assert!(shop.top_item);
            assert!(
                !shop.can_buy_credit && !shop.can_buy_gold,
                "<= 105 没有购买标志"
            );
            assert_eq!(shop.info, ItemInfo::default(), "Info 不落盘");
        }
    }

    #[test]
    fn game_shop_modern_path_reuses_db_decoder() {
        // `> 105` 时与 `GameShopItem::decode_db` 完全一致，包括两个购买标志。
        let value = GameShopItem {
            item_index: 3,
            g_index: 4,
            gold_price: 5,
            credit_price: 6,
            count: 7,
            class: "Weapon".to_owned(),
            category: "Sword".to_owned(),
            stock: 8,
            i_stock: true,
            deal: true,
            top_item: false,
            date: DateTimeBinary(0x0102_0304_0506_0708),
            can_buy_credit: true,
            can_buy_gold: true,
            ..GameShopItem::default()
        };
        let mut writer = PacketWriter::new();
        value.encode_db(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = DbReader::new(&bytes, 106);
        assert_eq!(decode_game_shop(&mut reader).expect("解码应当成功"), value);
        assert_eq!(reader.remaining(), 0);
    }
}
