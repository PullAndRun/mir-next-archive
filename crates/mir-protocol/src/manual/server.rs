//! 手写的服务端包。

use mir_core::enums::{
    HeroBehaviour, IntelligentCreatureType, LevelEffects, LightSetting, MirClass, MirDirection,
    MirGender, WeatherSetting,
};
use mir_core::{Color, DateTimeBinary, Point};

use crate::data::{ClientIntelligentCreature, ClientMagic, UserItem};

/// Crystal `ServerPackets.MapInformation`。
///
/// 手写原因：`Lightning` / `Fire` 被打包进同一个字节的位标记里（`bools & 0x01` / `& 0x02`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MapInformation {
    /// 地图编号。
    pub map_index: i32,
    /// 地图文件名。
    pub file_name: String,
    /// 地图标题。
    pub title: String,
    /// 小地图编号。
    pub mini_map: u16,
    /// 大地图编号。
    pub big_map: u16,
    /// 光照设置。
    pub lights: LightSetting,
    /// 是否雷雨。
    pub lightning: bool,
    /// 是否火焰特效。
    pub fire: bool,
    /// 地图明暗。
    pub map_dark_light: u8,
    /// 背景音乐编号。
    pub music: u16,
    /// 天气粒子。
    pub weather_particles: WeatherSetting,
}

impl MapInformation {
    /// 包编号。
    pub const INDEX: i16 = 17;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let map_index = r.read_i32()?;
        let file_name = r.read_string()?;
        let title = r.read_string()?;
        let mini_map = r.read_u16()?;
        let big_map = r.read_u16()?;
        let lights = LightSetting::from_value(r.read_u8()?).unwrap_or_default();
        let flags = r.read_u8()?;
        let map_dark_light = r.read_u8()?;
        let music = r.read_u16()?;
        let weather_particles = WeatherSetting::from_value(r.read_u16()?).unwrap_or_default();
        Ok(Self {
            map_index,
            file_name,
            title,
            mini_map,
            big_map,
            lights,
            lightning: flags & 0x01 == 0x01,
            fire: flags & 0x02 == 0x02,
            map_dark_light,
            music,
            weather_particles,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_i32(self.map_index);
        w.write_string(&self.file_name);
        w.write_string(&self.title);
        w.write_u16(self.mini_map);
        w.write_u16(self.big_map);
        w.write_u8(self.lights as u8);
        let mut flags = 0u8;
        if self.lightning {
            flags |= 0x01;
        }
        if self.fire {
            flags |= 0x02;
        }
        w.write_u8(flags);
        w.write_u8(self.map_dark_light);
        w.write_u16(self.music);
        w.write_u16(self.weather_particles as u16);
    }
}

/// Crystal `ServerPackets.UserInformation`。
///
/// 手写原因：Crystal 里该类不是 `sealed`，且包含三组「可为空的可选物品数组」与
/// 两个可变长数据类列表，超出了生成器的表达范围。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UserInformation {
    /// 对象 ID。
    pub object_id: u32,
    /// 真实角色 ID。
    pub real_id: u32,
    /// 角色名。
    pub name: String,
    /// 行会名。
    pub guild_name: String,
    /// 行会职位。
    pub guild_rank: String,
    /// 名字颜色。
    pub name_colour: Color,
    /// 职业。
    pub class: MirClass,
    /// 性别。
    pub gender: MirGender,
    /// 等级。
    pub level: u16,
    /// 坐标。
    pub location: Point,
    /// 朝向。
    pub direction: MirDirection,
    /// 发型。
    pub hair: u8,
    /// 当前生命。
    pub hp: i32,
    /// 当前魔法。
    pub mp: i32,
    /// 当前经验。
    pub experience: i64,
    /// 升级经验。
    pub max_experience: i64,
    /// 等级特效。
    pub level_effects: LevelEffects,
    /// 是否带有英雄。
    pub has_hero: bool,
    /// 英雄行为。
    pub hero_behaviour: HeroBehaviour,
    /// 背包（`None` 表示客户端应保留旧数据）。
    pub inventory: Option<Vec<Option<UserItem>>>,
    /// 装备。
    pub equipment: Option<Vec<Option<UserItem>>>,
    /// 任务物品。
    pub quest_inventory: Option<Vec<Option<UserItem>>>,
    /// 金币。
    pub gold: u32,
    /// 元宝。
    pub credit: u32,
    /// 是否开了扩展仓库。
    pub has_expanded_storage: bool,
    /// 是否设置了仓库密码。
    pub has_storage_password: bool,
    /// 是否强制要求仓库密码。
    pub require_storage_password: bool,
    /// 仓库密码设置时间。
    pub storage_password_last_set: DateTimeBinary,
    /// 扩展仓库到期时间。
    pub expanded_storage_expiry_time: DateTimeBinary,
    /// 已学技能。
    pub magics: Vec<ClientMagic>,
    /// 宠物。
    pub intelligent_creatures: Vec<ClientIntelligentCreature>,
    /// 当前召唤的宠物类型。
    pub summoned_creature_type: IntelligentCreatureType,
    /// 宠物是否已召唤。
    pub creature_summoned: bool,
    /// 是否允许观察。
    pub allow_observe: bool,
    /// 是否处于观察模式。
    pub observer: bool,
}

impl UserInformation {
    /// 包编号。
    pub const INDEX: i16 = 21;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let object_id = r.read_u32()?;
        let real_id = r.read_u32()?;
        let name = r.read_string()?;
        let guild_name = r.read_string()?;
        let guild_rank = r.read_string()?;
        let name_colour = Color(r.read_i32()?);
        let class = MirClass::from_value(r.read_u8()?).unwrap_or_default();
        let gender = MirGender::from_value(r.read_u8()?).unwrap_or_default();
        let level = r.read_u16()?;
        let location = Point {
            x: r.read_i32()?,
            y: r.read_i32()?,
        };
        let direction = MirDirection::from_value(r.read_u8()?).unwrap_or_default();
        let hair = r.read_u8()?;
        let hp = r.read_i32()?;
        let mp = r.read_i32()?;
        let experience = r.read_i64()?;
        let max_experience = r.read_i64()?;
        let level_effects = LevelEffects::from_value(r.read_u16()?).unwrap_or_default();
        let has_hero = r.read_bool()?;
        let hero_behaviour = HeroBehaviour::from_value(r.read_u8()?).unwrap_or_default();
        let inventory = decode_optional_items(r)?;
        let equipment = decode_optional_items(r)?;
        let quest_inventory = decode_optional_items(r)?;
        let gold = r.read_u32()?;
        let credit = r.read_u32()?;
        let has_expanded_storage = r.read_bool()?;
        let has_storage_password = r.read_bool()?;
        let require_storage_password = r.read_bool()?;
        let storage_password_last_set = DateTimeBinary(r.read_i64()?);
        let expanded_storage_expiry_time = DateTimeBinary(r.read_i64()?);
        let magic_count = r.read_count()?;
        let mut magics = Vec::with_capacity(magic_count.min(64));
        for _ in 0..magic_count {
            magics.push(ClientMagic::decode(r)?);
        }
        let creature_count = r.read_count()?;
        let mut intelligent_creatures = Vec::with_capacity(creature_count.min(64));
        for _ in 0..creature_count {
            intelligent_creatures.push(ClientIntelligentCreature::decode(r)?);
        }
        let summoned_creature_type =
            IntelligentCreatureType::from_value(r.read_u8()?).unwrap_or_default();
        let creature_summoned = r.read_bool()?;
        let allow_observe = r.read_bool()?;
        let observer = r.read_bool()?;
        Ok(Self {
            object_id,
            real_id,
            name,
            guild_name,
            guild_rank,
            name_colour,
            class,
            gender,
            level,
            location,
            direction,
            hair,
            hp,
            mp,
            experience,
            max_experience,
            level_effects,
            has_hero,
            hero_behaviour,
            inventory,
            equipment,
            quest_inventory,
            gold,
            credit,
            has_expanded_storage,
            has_storage_password,
            require_storage_password,
            storage_password_last_set,
            expanded_storage_expiry_time,
            magics,
            intelligent_creatures,
            summoned_creature_type,
            creature_summoned,
            allow_observe,
            observer,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_u32(self.object_id);
        w.write_u32(self.real_id);
        w.write_string(&self.name);
        w.write_string(&self.guild_name);
        w.write_string(&self.guild_rank);
        w.write_i32(self.name_colour.0);
        w.write_u8(self.class as u8);
        w.write_u8(self.gender as u8);
        w.write_u16(self.level);
        w.write_i32(self.location.x);
        w.write_i32(self.location.y);
        w.write_u8(self.direction as u8);
        w.write_u8(self.hair);
        w.write_i32(self.hp);
        w.write_i32(self.mp);
        w.write_i64(self.experience);
        w.write_i64(self.max_experience);
        w.write_u16(self.level_effects as u16);
        w.write_bool(self.has_hero);
        w.write_u8(self.hero_behaviour as u8);
        encode_optional_items(w, &self.inventory);
        encode_optional_items(w, &self.equipment);
        encode_optional_items(w, &self.quest_inventory);
        w.write_u32(self.gold);
        w.write_u32(self.credit);
        w.write_bool(self.has_expanded_storage);
        w.write_bool(self.has_storage_password);
        w.write_bool(self.require_storage_password);
        w.write_i64(self.storage_password_last_set.0);
        w.write_i64(self.expanded_storage_expiry_time.0);
        w.write_count(self.magics.len());
        for magic in &self.magics {
            magic.encode(w);
        }
        w.write_count(self.intelligent_creatures.len());
        for creature in &self.intelligent_creatures {
            creature.encode(w);
        }
        w.write_u8(self.summoned_creature_type as u8);
        w.write_bool(self.creature_summoned);
        w.write_bool(self.allow_observe);
        w.write_bool(self.observer);
    }
}

fn decode_optional_items(
    r: &mut crate::PacketReader<'_>,
) -> Result<Option<Vec<Option<UserItem>>>, crate::ProtocolError> {
    if !r.read_bool()? {
        return Ok(None);
    }
    let count = r.read_count()?;
    let mut items = Vec::with_capacity(count.min(128));
    for _ in 0..count {
        if r.read_bool()? {
            items.push(Some(UserItem::decode(r)?));
        } else {
            items.push(None);
        }
    }
    Ok(Some(items))
}

fn encode_optional_items(w: &mut crate::PacketWriter, items: &Option<Vec<Option<UserItem>>>) {
    let Some(items) = items else {
        w.write_bool(false);
        return;
    };
    w.write_bool(true);
    w.write_count(items.len());
    for item in items {
        match item {
            Some(item) => {
                w.write_bool(true);
                item.encode(w);
            }
            None => w.write_bool(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_information_bit_flags() {
        let packet = MapInformation {
            map_index: 3,
            file_name: "0".to_owned(),
            title: "比奇省".to_owned(),
            mini_map: 101,
            big_map: 102,
            lights: LightSetting::Day,
            lightning: true,
            fire: false,
            map_dark_light: 2,
            music: 7,
            weather_particles: WeatherSetting::None,
        };
        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = crate::PacketReader::new(&bytes);
        assert_eq!(MapInformation::decode(&mut reader).unwrap(), packet);
        assert!(reader.is_empty());
        // 位标记字节必须是 0x01（只有雷电）。
        assert!(bytes.contains(&0x01));
    }

    #[test]
    fn user_information_roundtrip() {
        let packet = UserInformation {
            object_id: 1,
            real_id: 2,
            name: "测试".to_owned(),
            guild_name: String::new(),
            guild_rank: String::new(),
            name_colour: Color(-1),
            class: MirClass::Warrior,
            gender: MirGender::Male,
            level: 1,
            location: Point { x: 100, y: 200 },
            direction: MirDirection::Down,
            hair: 1,
            hp: 15,
            mp: 10,
            experience: 0,
            max_experience: 100,
            level_effects: LevelEffects::None,
            has_hero: false,
            hero_behaviour: HeroBehaviour::default(),
            inventory: Some(vec![None, None]),
            equipment: None,
            quest_inventory: None,
            gold: 1000,
            credit: 0,
            has_expanded_storage: false,
            has_storage_password: false,
            require_storage_password: false,
            storage_password_last_set: DateTimeBinary(0),
            expanded_storage_expiry_time: DateTimeBinary(0),
            magics: Vec::new(),
            intelligent_creatures: Vec::new(),
            summoned_creature_type: IntelligentCreatureType::None,
            creature_summoned: false,
            allow_observe: true,
            observer: false,
        };
        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = crate::PacketReader::new(&bytes);
        let decoded = UserInformation::decode(&mut reader).expect("decode");
        assert!(reader.is_empty(), "left {} bytes", reader.remaining());
        assert_eq!(decoded, packet);
    }
}
