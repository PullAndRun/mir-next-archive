//! 客户端相关的线格式数据类（对应 Crystal `Shared/Data/ClientData.cs` 与
//! `Shared/Data/IntelligentCreatureData.cs`）。
//!
//! 这些类型在包体里以「定长字段顺序」出现，没有独立包头，因此手写实现。

use mir_core::DateTimeBinary;
use mir_core::enums::{
    IntelligentCreaturePickupMode, IntelligentCreatureType, ItemGrade, MirGridType, Spell,
};

/// 聊天里引用的物品链接（Crystal `ChatItem`，`ItemData.cs:1428`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatItem {
    /// 物品唯一 ID。
    pub unique_id: u64,
    /// 显示名。
    pub title: String,
    /// 所在格子。
    pub grid: MirGridType,
}

impl ChatItem {
    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        Ok(Self {
            unique_id: r.read_u64()?,
            title: r.read_string()?,
            grid: MirGridType::from_value(r.read_u8()?).unwrap_or_default(),
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_u64(self.unique_id);
        w.write_string(&self.title);
        w.write_u8(self.grid as u8);
    }
}

/// 客户端已学技能（Crystal `ClientMagic`，`ClientData.cs`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClientMagic {
    /// 技能名。
    pub name: String,
    /// 技能编号。
    pub spell: Spell,
    /// 基础耗蓝。
    pub base_cost: u8,
    /// 每级耗蓝增量。
    pub level_cost: u8,
    /// 图标编号。
    pub icon: u8,
    /// 一级所需等级。
    pub level1: u8,
    /// 二级所需等级。
    pub level2: u8,
    /// 三级所需等级。
    pub level3: u8,
    /// 一级所需修炼值。
    pub need1: u16,
    /// 二级所需修炼值。
    pub need2: u16,
    /// 三级所需修炼值。
    pub need3: u16,
    /// 当前等级。
    pub level: u8,
    /// 绑定的快捷键。
    pub key: u8,
    /// 当前经验。
    pub experience: u16,
    /// 施法延迟。
    pub delay: i64,
    /// 施法距离。
    pub range: u8,
    /// 施法耗时。
    pub cast_time: i64,
}

impl ClientMagic {
    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        Ok(Self {
            name: r.read_string()?,
            spell: Spell::from_value(r.read_u8()?).unwrap_or_default(),
            base_cost: r.read_u8()?,
            level_cost: r.read_u8()?,
            icon: r.read_u8()?,
            level1: r.read_u8()?,
            level2: r.read_u8()?,
            level3: r.read_u8()?,
            need1: r.read_u16()?,
            need2: r.read_u16()?,
            need3: r.read_u16()?,
            level: r.read_u8()?,
            key: r.read_u8()?,
            experience: r.read_u16()?,
            delay: r.read_i64()?,
            range: r.read_u8()?,
            cast_time: r.read_i64()?,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_string(&self.name);
        w.write_u8(self.spell as u8);
        w.write_u8(self.base_cost);
        w.write_u8(self.level_cost);
        w.write_u8(self.icon);
        w.write_u8(self.level1);
        w.write_u8(self.level2);
        w.write_u8(self.level3);
        w.write_u16(self.need1);
        w.write_u16(self.need2);
        w.write_u16(self.need3);
        w.write_u8(self.level);
        w.write_u8(self.key);
        w.write_u16(self.experience);
        w.write_i64(self.delay);
        w.write_u8(self.range);
        w.write_i64(self.cast_time);
    }
}

/// 宠物拾取规则（Crystal `IntelligentCreatureRules`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IntelligentCreatureRules {
    /// 最低饱食度。
    pub minimal_fullness: i32,
    /// 是否允许鼠标拾取。
    pub mouse_pickup_enabled: bool,
    /// 鼠标拾取范围。
    pub mouse_pickup_range: i32,
    /// 是否允许自动拾取。
    pub auto_pickup_enabled: bool,
    /// 自动拾取范围。
    pub auto_pickup_range: i32,
    /// 是否允许半自动拾取。
    pub semi_auto_pickup_enabled: bool,
    /// 半自动拾取范围。
    pub semi_auto_pickup_range: i32,
    /// 是否可产出黑石。
    pub can_produce_black_stone: bool,
}

impl IntelligentCreatureRules {
    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        Ok(Self {
            minimal_fullness: r.read_i32()?,
            mouse_pickup_enabled: r.read_bool()?,
            mouse_pickup_range: r.read_i32()?,
            auto_pickup_enabled: r.read_bool()?,
            auto_pickup_range: r.read_i32()?,
            semi_auto_pickup_enabled: r.read_bool()?,
            semi_auto_pickup_range: r.read_i32()?,
            can_produce_black_stone: r.read_bool()?,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_i32(self.minimal_fullness);
        w.write_bool(self.mouse_pickup_enabled);
        w.write_i32(self.mouse_pickup_range);
        w.write_bool(self.auto_pickup_enabled);
        w.write_i32(self.auto_pickup_range);
        w.write_bool(self.semi_auto_pickup_enabled);
        w.write_i32(self.semi_auto_pickup_range);
        w.write_bool(self.can_produce_black_stone);
    }
}

/// 宠物拾取过滤（Crystal `IntelligentCreatureItemFilter`）。
#[derive(Debug, Clone, PartialEq)]
pub struct IntelligentCreatureItemFilter {
    /// 拾取全部。
    pub pet_pickup_all: bool,
    /// 拾取金币。
    pub pet_pickup_gold: bool,
    /// 拾取武器。
    pub pet_pickup_weapons: bool,
    /// 拾取衣服。
    pub pet_pickup_armours: bool,
    /// 拾取头盔。
    pub pet_pickup_helmets: bool,
    /// 拾取鞋子。
    pub pet_pickup_boots: bool,
    /// 拾取腰带。
    pub pet_pickup_belts: bool,
    /// 拾取饰品。
    pub pet_pickup_accessories: bool,
    /// 拾取其它。
    pub pet_pickup_others: bool,
    /// 拾取品级下限。
    pub pickup_grade: ItemGrade,
}

impl Default for IntelligentCreatureItemFilter {
    fn default() -> Self {
        Self {
            pet_pickup_all: true,
            pet_pickup_gold: false,
            pet_pickup_weapons: false,
            pet_pickup_armours: false,
            pet_pickup_helmets: false,
            pet_pickup_boots: false,
            pet_pickup_belts: false,
            pet_pickup_accessories: false,
            pet_pickup_others: false,
            pickup_grade: ItemGrade::None,
        }
    }
}

impl IntelligentCreatureItemFilter {
    /// 解码（9 个布尔；`PickupGrade` 在 Crystal 里已被注释掉，不参与线格式）。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        Ok(Self {
            pet_pickup_all: r.read_bool()?,
            pet_pickup_gold: r.read_bool()?,
            pet_pickup_weapons: r.read_bool()?,
            pet_pickup_armours: r.read_bool()?,
            pet_pickup_helmets: r.read_bool()?,
            pet_pickup_boots: r.read_bool()?,
            pet_pickup_belts: r.read_bool()?,
            pet_pickup_accessories: r.read_bool()?,
            pet_pickup_others: r.read_bool()?,
            pickup_grade: ItemGrade::None,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_bool(self.pet_pickup_all);
        w.write_bool(self.pet_pickup_gold);
        w.write_bool(self.pet_pickup_weapons);
        w.write_bool(self.pet_pickup_armours);
        w.write_bool(self.pet_pickup_helmets);
        w.write_bool(self.pet_pickup_boots);
        w.write_bool(self.pet_pickup_belts);
        w.write_bool(self.pet_pickup_accessories);
        w.write_bool(self.pet_pickup_others);
    }
}

/// 客户端宠物信息（Crystal `ClientIntelligentCreature`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClientIntelligentCreature {
    /// 宠物类型。
    pub pet_type: IntelligentCreatureType,
    /// 图标。
    pub icon: i32,
    /// 自定义名字。
    pub custom_name: String,
    /// 饱食度。
    pub fullness: i32,
    /// 所在格子。
    pub slot_index: i32,
    /// 到期时间。
    pub expire: DateTimeBinary,
    /// 黑石时间。
    pub blackstone_time: i64,
    /// 喂食维持时间。
    pub maintain_food_time: i64,
    /// 拾取模式。
    pub pet_mode: IntelligentCreaturePickupMode,
    /// 拾取规则。
    pub creature_rules: IntelligentCreatureRules,
    /// 拾取过滤。
    pub filter: IntelligentCreatureItemFilter,
}

impl ClientIntelligentCreature {
    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let pet_type = IntelligentCreatureType::from_value(r.read_u8()?).unwrap_or_default();
        let icon = r.read_i32()?;
        let custom_name = r.read_string()?;
        let fullness = r.read_i32()?;
        let slot_index = r.read_i32()?;
        let expire = DateTimeBinary(r.read_i64()?);
        let blackstone_time = r.read_i64()?;
        let pet_mode = IntelligentCreaturePickupMode::from_value(r.read_u8()?).unwrap_or_default();
        let creature_rules = IntelligentCreatureRules::decode(r)?;
        let mut filter = IntelligentCreatureItemFilter::decode(r)?;
        filter.pickup_grade = ItemGrade::from_value(r.read_u8()?).unwrap_or_default();
        let maintain_food_time = r.read_i64()?;
        Ok(Self {
            pet_type,
            icon,
            custom_name,
            fullness,
            slot_index,
            expire,
            blackstone_time,
            maintain_food_time,
            pet_mode,
            creature_rules,
            filter,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_u8(self.pet_type as u8);
        w.write_i32(self.icon);
        w.write_string(&self.custom_name);
        w.write_i32(self.fullness);
        w.write_i32(self.slot_index);
        w.write_i64(self.expire.0);
        w.write_i64(self.blackstone_time);
        w.write_u8(self.pet_mode as u8);
        self.creature_rules.encode(w);
        self.filter.encode(w);
        w.write_u8(self.filter.pickup_grade as u8);
        w.write_i64(self.maintain_food_time);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_into<T>(value: &T, encode: impl Fn(&T, &mut crate::PacketWriter)) -> Vec<u8> {
        let mut writer = crate::PacketWriter::new();
        encode(value, &mut writer);
        writer.into_inner()
    }

    #[test]
    fn chat_item_roundtrip() {
        let item = ChatItem {
            unique_id: 42,
            title: "屠龙".to_owned(),
            grid: MirGridType::Inventory,
        };
        let bytes = encode_into(&item, ChatItem::encode);
        let mut reader = crate::PacketReader::new(&bytes);
        let decoded = ChatItem::decode(&mut reader).expect("decode");
        assert!(
            reader.is_empty(),
            "decoder left {} bytes",
            reader.remaining()
        );
        assert_eq!(decoded, item);
    }

    #[test]
    fn client_magic_roundtrip() {
        let magic = ClientMagic {
            name: "火球术".to_owned(),
            spell: Spell::FireBall,
            base_cost: 3,
            level_cost: 1,
            icon: 2,
            level1: 1,
            level2: 2,
            level3: 3,
            need1: 100,
            need2: 200,
            need3: 300,
            level: 3,
            key: 1,
            experience: 7,
            delay: 900,
            range: 7,
            cast_time: 1800,
        };
        let bytes = encode_into(&magic, ClientMagic::encode);
        let mut reader = crate::PacketReader::new(&bytes);
        let decoded = ClientMagic::decode(&mut reader).expect("decode");
        assert!(
            reader.is_empty(),
            "decoder left {} bytes",
            reader.remaining()
        );
        assert_eq!(decoded, magic);
    }
}
