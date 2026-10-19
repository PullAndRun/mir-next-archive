//! 手写的服务端包。

use mir_core::enums::{
    HeroBehaviour, IntelligentCreatureType, LevelEffects, LightSetting, MirClass, MirDirection,
    MirGender, Spell, WeatherSetting,
};
use mir_core::{Color, DateTimeBinary, Point};

use crate::data::{
    ClientIntelligentCreature, ClientMagic, GameShopItem, GuildRank, GuildStorageItem, ItemInfo,
    ItemRentalInformation, RankCharacterInfo, UserItem,
};

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
        w.write_u16(self.level_effects.value());
        w.write_bool(self.has_hero);
        w.write_u8(self.hero_behaviour as u8);
        encode_optional_items(w, self.inventory.as_deref());
        encode_optional_items(w, self.equipment.as_deref());
        encode_optional_items(w, self.quest_inventory.as_deref());
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

/// Crystal `ServerPackets.UserSlotsRefresh`（`Shared/ServerPackets.cs:806`）。
///
/// 包编号 `ServerPacketIds.UserSlotsRefresh` = 22（`Shared/Enums.cs:1458`）。
///
/// 手写原因：「可选数组 + 每格存在标志」的两段式形状
/// （读 `ServerPackets.cs:818-836`，写 `:841-866`），生成器读不懂
/// （`tools/protocol-gen/report.md:11`）。
///
/// 线格式（背包、装备各来一遍）：
///
/// ```text
/// bool    存在？        <- false 表示「这一次不刷新」
/// i32     格数          <- 仅在上一个 bool 为 true 时出现
/// per slot: bool 有物品？ + （有则一个完整 `UserItem`）
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UserSlotsRefresh {
    /// 背包；`None` 对应 C# 的 `null` 数组，客户端应保留旧数据。
    pub inventory: Option<Vec<Option<UserItem>>>,
    /// 装备；`None` 同上。
    pub equipment: Option<Vec<Option<UserItem>>>,
}

impl UserSlotsRefresh {
    /// 包编号。
    pub const INDEX: i16 = 22;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let inventory = decode_optional_items(r)?;
        let equipment = decode_optional_items(r)?;
        Ok(Self {
            inventory,
            equipment,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        encode_optional_items(w, self.inventory.as_deref());
        encode_optional_items(w, self.equipment.as_deref());
    }
}

/// Crystal `ServerPackets.Magic`（`Shared/ServerPackets.cs:3388`）。
///
/// 包编号 `ServerPacketIds.Magic` = 120（`Shared/Enums.cs:1556`）。
///
/// 手写原因：`SecondaryTargetIDs` 的写侧是 `writer.Write(Count)` 加
/// `foreach (var targetID in SecondaryTargetIDs)`（`ServerPackets.cs:3424-3428`），
/// 生成器不支持对集合写 `foreach`（`tools/protocol-gen/report.md:12`）；
/// 读侧是 `int count` + 逐个 `u32`（`:3408-3413`）。
///
/// 线格式：
///
/// ```text
/// u8   Spell
/// u32  TargetID
/// i32  Target.X
/// i32  Target.Y
/// bool Cast
/// u8   Level
/// i32  SecondaryTargetIDs.Count
/// u32×Count  SecondaryTargetIDs
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Magic {
    /// 技能编号。
    pub spell: Spell,
    /// 主目标的对象编号。
    pub target_id: u32,
    /// 施法坐标。
    pub target: Point,
    /// 是否在施法（否则只是切换/准备）。
    pub cast: bool,
    /// 技能等级。
    pub level: u8,
    /// 溅射目标的编号列表。
    pub secondary_target_ids: Vec<u32>,
}

impl Magic {
    /// 包编号。
    pub const INDEX: i16 = 120;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let spell = Spell::from_value(r.read_u8()?).unwrap_or_default();
        let target_id = r.read_u32()?;
        let target = Point {
            x: r.read_i32()?,
            y: r.read_i32()?,
        };
        let cast = r.read_bool()?;
        let level = r.read_u8()?;
        let count = r.read_count()?;
        let mut secondary_target_ids = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            secondary_target_ids.push(r.read_u32()?);
        }
        Ok(Self {
            spell,
            target_id,
            target,
            cast,
            level,
            secondary_target_ids,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_u8(self.spell.value());
        w.write_u32(self.target_id);
        w.write_i32(self.target.x);
        w.write_i32(self.target.y);
        w.write_bool(self.cast);
        w.write_u8(self.level);
        w.write_count(self.secondary_target_ids.len());
        for &target_id in &self.secondary_target_ids {
            w.write_u32(target_id);
        }
    }
}

/// Crystal `ServerPackets.ObjectMagic`（`Shared/ServerPackets.cs:3469`）。
///
/// 包编号 `ServerPacketIds.ObjectMagic` = 123（`Shared/Enums.cs:1559`）。
///
/// 手写原因与 [`Magic`] 相同：`SecondaryTargetIDs` 用 `foreach` 写出
/// （`ServerPackets.cs:3522-3526`，报告 `tools/protocol-gen/report.md:13`）。
///
/// 线格式：
///
/// ```text
/// u32  ObjectID
/// i32  Location.X
/// i32  Location.Y
/// u8   Direction        <- `MirDirection`
/// u8   Spell
/// u32  TargetID
/// i32  Target.X
/// i32  Target.Y
/// bool Cast
/// u8   Level
/// bool SelfBroadcast
/// i32  SecondaryTargetIDs.Count
/// u32×Count  SecondaryTargetIDs
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjectMagic {
    /// 施法者的对象编号。
    pub object_id: u32,
    /// 施法者的位置。
    pub location: Point,
    /// 施法者的朝向。
    pub direction: MirDirection,
    /// 技能编号。
    pub spell: Spell,
    /// 主目标的对象编号。
    pub target_id: u32,
    /// 主目标的坐标。
    pub target: Point,
    /// 是否在施法。
    pub cast: bool,
    /// 技能等级。
    pub level: u8,
    /// 是否只广播给自己（C# 缺省 `false`）。
    pub self_broadcast: bool,
    /// 溅射目标的编号列表。
    pub secondary_target_ids: Vec<u32>,
}

impl ObjectMagic {
    /// 包编号。
    pub const INDEX: i16 = 123;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let object_id = r.read_u32()?;
        let location = Point {
            x: r.read_i32()?,
            y: r.read_i32()?,
        };
        let direction = MirDirection::from_value(r.read_u8()?).unwrap_or_default();
        let spell = Spell::from_value(r.read_u8()?).unwrap_or_default();
        let target_id = r.read_u32()?;
        let target = Point {
            x: r.read_i32()?,
            y: r.read_i32()?,
        };
        let cast = r.read_bool()?;
        let level = r.read_u8()?;
        let self_broadcast = r.read_bool()?;
        let count = r.read_count()?;
        let mut secondary_target_ids = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            secondary_target_ids.push(r.read_u32()?);
        }
        Ok(Self {
            object_id,
            location,
            direction,
            spell,
            target_id,
            target,
            cast,
            level,
            self_broadcast,
            secondary_target_ids,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_u32(self.object_id);
        w.write_i32(self.location.x);
        w.write_i32(self.location.y);
        w.write_u8(self.direction as u8);
        w.write_u8(self.spell.value());
        w.write_u32(self.target_id);
        w.write_i32(self.target.x);
        w.write_i32(self.target.y);
        w.write_bool(self.cast);
        w.write_u8(self.level);
        w.write_bool(self.self_broadcast);
        w.write_count(self.secondary_target_ids.len());
        for &target_id in &self.secondary_target_ids {
            w.write_u32(target_id);
        }
    }
}

/// Crystal `ServerPackets.UserStorage`（`Shared/ServerPackets.cs:3673`）。
///
/// 包编号 `ServerPacketIds.UserStorage` = 130（`Shared/Enums.cs:1566`）。
///
/// 手写原因：读侧 `if (!reader.ReadBoolean()) return;`、写侧
/// `if (Storage == null) return;`（`ServerPackets.cs:3681`、`:3693`）这种
/// 「早返回」形状生成器不支持（`tools/protocol-gen/report.md:14`）。
///
/// 线格式：
///
/// ```text
/// bool    存在？
/// i32     格数          <- 仅在上一个 bool 为 true 时出现
/// per slot: bool 有物品？ + （有则一个完整 `UserItem`）
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UserStorage {
    /// 仓库格子；`None` 对应 C# 的 `null` 数组（此时负载只有 1 个 `false`）。
    pub storage: Option<Vec<Option<UserItem>>>,
}

impl UserStorage {
    /// 包编号。
    pub const INDEX: i16 = 130;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        Ok(Self {
            storage: decode_optional_items(r)?,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        encode_optional_items(w, self.storage.as_deref());
    }
}

/// Crystal `ServerPackets.ChatItemStats`（`Shared/ServerPackets.cs:4452`）。
///
/// 包编号 `ServerPacketIds.ChatItemStats` = 165（`Shared/Enums.cs:1601`）。
///
/// 手写原因：C# 这个包**读写不对称**。`ReadPacket`（`ServerPackets.cs:4460`）
/// 无条件 `Stats = new UserItem(reader);`，而 `WritePacket`（`:4466`）是
/// `if (Stats != null) Stats.Save(writer);` —— 写侧**没有存在标志位**。
///
/// 本实现照抄这个缺陷：
///
/// * 解码**总是**读一个 `UserItem`，所以解出来的 `stats` 恒为 `Some`；
/// * 编码只在 `stats` 为 `Some` 时才写出物品负载。
///
/// 后果：`stats = None` 编出来的负载只有 8 字节，**读不回来**（读端会继续找
/// 一个完整 `UserItem`，报 `UnexpectedEof`）。Crystal 自己没踩到，是因为这条包
/// 目前是死代码：发送端 `Server/MirObjects/PlayerObject.cs:7807` 与接收端
/// `Client/MirScenes/GameScene.cs:5752` 都被注释掉了。
/// 将来服务端真要发这个包时，必须**永远**给 `stats` 填 `Some`。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatItemStats {
    /// 聊天里被点开的物品链接编号。
    pub chat_item_id: u64,
    /// 物品属性快照；`None` 只在编码时有「写 0 字节」的效果（见类型文档）。
    pub stats: Option<UserItem>,
}

impl ChatItemStats {
    /// 包编号。
    pub const INDEX: i16 = 165;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码：恒读一个完整 `UserItem`（C# `ServerPackets.cs:4460` 没有存在标志）。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let chat_item_id = r.read_u64()?;
        let stats = Some(UserItem::decode(r)?);
        Ok(Self {
            chat_item_id,
            stats,
        })
    }

    /// 编码：仅当 `stats` 为 `Some` 时写物品（C# `ServerPackets.cs:4466`）。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_u64(self.chat_item_id);
        if let Some(stats) = &self.stats {
            stats.encode(w);
        }
    }
}

/// Crystal `ServerPackets.GuildNoticeChange`（`Shared/ServerPackets.cs:4470`）。
///
/// 包编号 `ServerPacketIds.GuildNoticeChange` = 166（`Shared/Enums.cs:1602`）。
///
/// 手写原因：读侧的循环上界来自**先前读到的一个 `i32` 字段**（不是集合自身的
/// 长度），写侧还有 `if (update < 0)` 的早返回（`ServerPackets.cs:4480-4493`），
/// 生成器读不懂（`tools/protocol-gen/report.md:16`）。
///
/// 线格式：
///
/// ```text
/// i32  update      <- 负数：只写这 4 字节就结束
/// string ×update   <- 条数直接复用上面的 update，没有独立的个数前缀
/// ```
///
/// 语义：`update == -1` 是「公告变了，请重新拉取」的信号
/// （`Server/MirObjects/GuildObject.cs:599` 只发这个信号，
/// 客户端 `Client/MirScenes/GameScene.cs:5808` 据此刷新）；
/// 其余情况 `update` 保持 0，真正的内容在 `notice` 里
/// （`Server/MirObjects/PlayerObject.cs:10020` 只填 `notice`）。
///
/// **写侧用的是 `notice.len()` 而不是 `update`**（`ServerPackets.cs:4491`）：
/// 两者不相等时字节流与 `update` 不同步，读回来的 `update` 会变成 `notice.len()`。
/// 本实现与 C# 逐字节一致：`update < 0` 只写 `update`，否则写
/// `notice.len()` + 内容。因此 roundtrip 的前提是
/// `update == notice.len()`（或 `update < 0` 且 `notice` 为空）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildNoticeChange {
    /// C# 字段 `update`：`-1` 表示「公告已变，请刷新」，否则由读侧充当行数。
    pub update: i32,
    /// 公告的每一行。
    pub notice: Vec<String>,
}

impl GuildNoticeChange {
    /// 包编号。
    pub const INDEX: i16 = 166;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let update = r.read_i32()?;
        let mut notice = Vec::new();
        // `for (int i = 0; i < update; i++) notice.Add(reader.ReadString());`
        // 负值时循环一次都不执行（与 C# 相同）。
        for _ in 0..update {
            notice.push(r.read_string()?);
        }
        Ok(Self { update, notice })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        if self.update < 0 {
            w.write_i32(self.update);
            return;
        }
        w.write_count(self.notice.len());
        for line in &self.notice {
            w.write_string(line);
        }
    }
}

/// Crystal `ServerPackets.GuildMemberChange`（`Shared/ServerPackets.cs:4497`）。
///
/// 包编号 `ServerPacketIds.GuildMemberChange` = 167（`Shared/Enums.cs:1603`）。
///
/// 手写原因：`if (Status > 5) { int rankcount = reader.ReadInt32(); for (...) }`
/// 这种「字段值决定后面还有没有集合」的形状（`ServerPackets.cs:4512-4517`）
/// 生成器读不懂（`tools/protocol-gen/report.md:17`）。
///
/// `status` 的取值范围取自客户端 `Client/MirScenes/GameScene.cs:5813-5857`：
///
/// * `0` 下线、`1` 上线、`2` 入会、`3` 被踢、`4` 退会、`5` 职位（名）变化 ——
///   **不带** `ranks`；
/// * `6` 新职位、`7` 职位权限变化、`8` 自己的职位变化 —— 带 1 条；
/// * `255` 整份成员名单 —— 带全部。
///
/// 注意：`status <= 5` 时即使 `ranks` 非空也**不会**被写出，roundtrip 会丢数据。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildMemberChange {
    /// 目标角色名。
    pub name: String,
    /// 目标职位序号。
    pub rank_index: u8,
    /// 变动类型，见类型文档。
    pub status: u8,
    /// 职位列表；仅在 `status > 5` 时上线。
    pub ranks: Vec<GuildRank>,
}

impl GuildMemberChange {
    /// 包编号。
    pub const INDEX: i16 = 167;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let name = r.read_string()?;
        // C# 的字段声明顺序是 Name / Status / RankIndex，但读写顺序是
        // Name / RankIndex / Status（`ServerPackets.cs:4509-4511`），以读写为准。
        let rank_index = r.read_u8()?;
        let status = r.read_u8()?;
        let mut ranks = Vec::new();
        if status > 5 {
            let rank_count = r.read_count()?;
            ranks.reserve(rank_count.min(8));
            for _ in 0..rank_count {
                ranks.push(GuildRank::decode(r)?);
            }
        }
        Ok(Self {
            name,
            rank_index,
            status,
            ranks,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_string(&self.name);
        w.write_u8(self.rank_index);
        w.write_u8(self.status);
        if self.status > 5 {
            w.write_count(self.ranks.len());
            for rank in &self.ranks {
                rank.encode(w);
            }
        }
    }
}

/// Crystal `ServerPackets.GuildStorageItemChange`（`Shared/ServerPackets.cs:4648`）。
///
/// 包编号 `ServerPacketIds.GuildStorageItemChange` = 173（`Shared/Enums.cs:1609`）。
///
/// 手写原因：写侧 `writer.Write(Item != null); if (Item == null) return;`
/// 的早返回形状（`ServerPackets.cs:4675-4676`）生成器读不懂
/// （`tools/protocol-gen/report.md:18`）。
///
/// **这里的字段顺序与 `GuildStorageItem` 自己的 `Save` 相反**：Crystal 在本包
/// 先写 `UserId`、再写 `UserItem`（读 `ServerPackets.cs:4665-4666`，
/// 写 `:4677-4678`），而 `Shared/Data/GuildData.cs:50` / `:55` 里的
/// `GuildStorageItem` 是**先物品、后 `UserId`**。两端各自自洽所以能跑，
/// 但本包**不能**直接调用 `GuildStorageItem::decode` / `encode`：
/// `UserItem.UniqueID` 恰好也是 8 字节，错位后不会报错，只会静默读错。
/// 这里把顺序内联写出，顺序见 [`GuildStorageItemChange::decode`]。
///
/// 线格式：
///
/// ```text
/// u8   Type
/// i32  To
/// i32  From
/// i32  User
/// bool Item 存在？
/// i64  Item.UserId     <- 注意：在物品**之前**
/// UserItem Item.Item
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildStorageItemChange {
    /// C# 字段 `User`：受影响玩家的对象编号（服务端填 `Info.Index`，
    /// 见 `Server/MirObjects/PlayerObject.cs:10182`）。
    pub user: i32,
    /// C# 字段 `Type`：`0` 存入、`1` 取回、`2` 仓库内互换；
    /// `3 + type` 表示同一操作失败（`Server/MirObjects/PlayerObject.cs:10123`）。
    pub change_type: u8,
    /// 目标格子。
    pub to: i32,
    /// 来源格子。
    pub from: i32,
    /// 变动的物品；`None` 对应 C# 的 `null`（纯失败通知）。
    pub item: Option<GuildStorageItem>,
}

impl GuildStorageItemChange {
    /// 包编号。
    pub const INDEX: i16 = 173;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let change_type = r.read_u8()?;
        let to = r.read_i32()?;
        let from = r.read_i32()?;
        let user = r.read_i32()?;
        let item = if r.read_bool()? {
            // `new GuildStorageItem { UserId = reader.ReadInt64(), Item = new UserItem(reader) }`
            // —— 对象初始化器按书写顺序求值：先 `UserId`，后物品。
            let user_id = r.read_i64()?;
            let item = UserItem::decode(r)?;
            Some(GuildStorageItem { item, user_id })
        } else {
            None
        };
        Ok(Self {
            user,
            change_type,
            to,
            from,
            item,
        })
    }

    /// 编码：字段顺序与 [`GuildStorageItemChange::decode`] 一致。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_u8(self.change_type);
        w.write_i32(self.to);
        w.write_i32(self.from);
        w.write_i32(self.user);
        w.write_bool(self.item.is_some());
        let Some(item) = &self.item else {
            return;
        };
        w.write_i64(item.user_id);
        item.item.encode(w);
    }
}

/// Crystal `ServerPackets.GuildStorageList`（`Shared/ServerPackets.cs:4681`）。
///
/// 包编号 `ServerPacketIds.GuildStorageList` = 174（`Shared/Enums.cs:1610`）。
///
/// 手写原因：计数数组 + 每格 `if (reader.ReadBoolean() == true) Items[i] = ...`
/// 的填充语句（`ServerPackets.cs:4690-4695`），生成器读不懂
/// （`tools/protocol-gen/report.md:19`）。
///
/// 这里用的是 `GuildStorageItem` 自己的顺序（先物品、后 `UserId`），
/// 与 [`GuildStorageItemChange`] 相反。
///
/// 线格式：
///
/// ```text
/// i32  Items.Count
/// per slot: bool 有物品？ + （有则 UserItem Item + i64 UserId）
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildStorageList {
    /// 仓库格子。
    pub items: Vec<Option<GuildStorageItem>>,
}

impl GuildStorageList {
    /// 包编号。
    pub const INDEX: i16 = 174;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let count = r.read_count()?;
        let mut items = Vec::new();
        for _ in 0..count {
            if r.read_bool()? {
                items.push(Some(GuildStorageItem::decode(r)?));
            } else {
                items.push(None);
            }
        }
        Ok(Self { items })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_count(self.items.len());
        for item in &self.items {
            w.write_bool(item.is_some());
            if let Some(item) = item {
                item.encode(w);
            }
        }
    }
}

/// Crystal `ServerPackets.HeroCreateRequest`（`Shared/ServerPackets.cs:4720`）。
///
/// 包编号 `ServerPacketIds.HeroCreateRequest` = 176（`Shared/Enums.cs:1612`）。
///
/// 手写原因：`new bool[count]` 这种「先读个数再分配定长数组」的写法
/// （`ServerPackets.cs:4727-4730`）生成器读不懂
/// （`tools/protocol-gen/report.md:20`）。
///
/// 线格式：
///
/// ```text
/// i32    CanCreateClass.Length
/// bool×N CanCreateClass
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeroCreateRequest {
    /// 每个职业是否允许创建英雄（顺序对应 `MirClass` 的判别值）。
    pub can_create_class: Vec<bool>,
}

impl HeroCreateRequest {
    /// 包编号。
    pub const INDEX: i16 = 176;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let count = r.read_count()?;
        let mut can_create_class = Vec::with_capacity(count.min(16));
        for _ in 0..count {
            can_create_class.push(r.read_bool()?);
        }
        Ok(Self { can_create_class })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_count(self.can_create_class.len());
        for allowed in &self.can_create_class {
            w.write_bool(*allowed);
        }
    }
}

/// Crystal `ServerPackets.CompleteQuest`（`Shared/ServerPackets.cs:5239`）。
///
/// 包编号 `ServerPacketIds.CompleteQuest` = 202（`Shared/Enums.cs:1638`）。
///
/// 手写原因：写侧是 `writer.Write(CompletedQuests.Count)` 加
/// `foreach (int q in CompletedQuests)`（`ServerPackets.cs:5256-5258`），
/// 生成器不支持对集合写 `foreach`（`tools/protocol-gen/report.md:21`）。
///
/// 线格式：
///
/// ```text
/// i32    CompletedQuests.Count
/// i32×N  CompletedQuests
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompleteQuest {
    /// 已完成的任务编号。
    pub completed_quests: Vec<i32>,
}

impl CompleteQuest {
    /// 包编号。
    pub const INDEX: i16 = 202;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let count = r.read_count()?;
        let mut completed_quests = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            completed_quests.push(r.read_i32()?);
        }
        Ok(Self { completed_quests })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_count(self.completed_quests.len());
        for &quest in &self.completed_quests {
            w.write_i32(quest);
        }
    }
}

/// 一种觉醒材料：物品模板 + 需要数量。
///
/// 对应 C# 的两个平行数组：`ItemInfo[] Materials` 与 `byte[] MaterialsCount`
/// （`Shared/ServerPackets.cs:5813-5814`）。这里的元组只承载**非空格**的内容，
/// 空格的 `MaterialsCount[i]` 在 C# 里恒为 0、不上线，因此无需保存。
pub type AwakeningMaterial = (ItemInfo, u8);

/// Crystal `ServerPackets.AwakeningNeedMaterials`（`Shared/ServerPackets.cs:5809`）。
///
/// 包编号 `ServerPacketIds.AwakeningNeedMaterials` = 228（`Shared/Enums.cs:1664`）。
///
/// 手写原因：早返回（`if (!reader.ReadBoolean()) return;` / `if (Materials == null) return;`）
/// 加上两个平行数组交织写出（`ServerPackets.cs:5818-5844`），生成器读不懂
/// （`tools/protocol-gen/report.md:23`）。
///
/// 线格式：
///
/// ```text
/// bool     存在？
/// i32      格数            <- 仅在上一个 bool 为 true 时出现
/// per slot: bool 有材料？ + （有则 ItemInfo Materials[i] + u8 MaterialsCount[i]）
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AwakeningNeedMaterials {
    /// 每一格的 `(物品模板, 需要数量)`；`None` 对应 C# 的 `null` 数组。
    pub materials: Option<Vec<Option<AwakeningMaterial>>>,
}

impl AwakeningNeedMaterials {
    /// 包编号。
    pub const INDEX: i16 = 228;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        if !r.read_bool()? {
            return Ok(Self { materials: None });
        }
        let count = r.read_count()?;
        let mut materials = Vec::with_capacity(count.min(16));
        for _ in 0..count {
            if !r.read_bool()? {
                materials.push(None);
                continue;
            }
            let info = ItemInfo::decode(r)?;
            let need = r.read_u8()?;
            materials.push(Some((info, need)));
        }
        Ok(Self {
            materials: Some(materials),
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        let Some(materials) = self.materials.as_deref() else {
            w.write_bool(false);
            return;
        };
        w.write_bool(true);
        w.write_count(materials.len());
        for material in materials {
            w.write_bool(material.is_some());
            if let Some((info, need)) = material {
                info.encode(w);
                w.write_u8(*need);
            }
        }
    }
}

/// Crystal `ServerPackets.GameShopInfo`（`Shared/ServerPackets.cs:5348`）。
///
/// 包编号 `ServerPacketIds.GameShopInfo` = 250（`Shared/Enums.cs:1686`）。
///
/// 手写原因：`new GameShopItem(reader, true)` / `Item.Save(writer, true)`
/// （`ServerPackets.cs:5360`、`:5366`）这种带参数的构造与保存，生成器读不懂
/// （`tools/protocol-gen/report.md:22`）。
///
/// 第二个参数 `packet = true` 选中的正是 `data::GameShopItem` 的
/// [`decode`](GameShopItem::decode) / [`encode`](GameShopItem::encode)
/// （包路径：`GIndex` 之后整块写出 `Info`），而不是物品库路径的
/// `decode_db` / `encode_db` —— 见 `data/game_shop_item.rs` 的模块文档。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameShopInfo {
    /// 商品定义（含 `ItemInfo`）。
    pub item: GameShopItem,
    /// 当前库存档位。
    pub stock_level: i32,
}

impl GameShopInfo {
    /// 包编号。
    pub const INDEX: i16 = 250;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let item = GameShopItem::decode(r)?;
        let stock_level = r.read_i32()?;
        Ok(Self { item, stock_level })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        self.item.encode(w);
        w.write_i32(self.stock_level);
    }
}

/// Crystal `ServerPackets.Rankings`（`Shared/ServerPackets.cs:6294`）。
///
/// 包编号 `ServerPacketIds.Rankings` = 252（`Shared/Enums.cs:1688`）。
///
/// 手写原因：读侧用局部变量 `count` 承载两个列表的个数
/// （`ServerPackets.cs:6312`、`:6317`），生成器把它当成了未声明字段
/// （`tools/protocol-gen/report.md:24`）。
///
/// 注意末尾的 `count` 是**独立字段**（总人数），不是第三份列表的个数前缀：
/// 两个列表各带自己的 `i32` 个数，最后再写一个 `i32 Count`
/// （`ServerPackets.cs:6320-6322`、`:6331-6334`）。
///
/// 线格式：
///
/// ```text
/// u8    RankType
/// i32   MyRank
/// i32   ListingDetails.Count
/// RankCharacterInfo×N
/// i32   Listings.Count
/// i64×M Listings
/// i32   Count
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rankings {
    /// 榜单类型（客户端 `Client/MirScenes/Dialogs/RankingDialog.cs:286` 按它分栏）。
    pub rank_type: u8,
    /// 自己在该榜单的名次；`0` 表示未上榜（`RankingDialog.cs:319`）。
    pub my_rank: i32,
    /// 榜单明细。
    pub listing_details: Vec<RankCharacterInfo>,
    /// 榜单数值（与 `listing_details` 平行，长度通常相同）。
    pub listings: Vec<i64>,
    /// 该榜单的总人数，客户端据此算滚动分页（`RankingDialog.cs:291-294`）。
    pub count: i32,
}

impl Rankings {
    /// 包编号。
    pub const INDEX: i16 = 252;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let rank_type = r.read_u8()?;
        let my_rank = r.read_i32()?;
        let detail_count = r.read_count()?;
        let mut listing_details = Vec::with_capacity(detail_count.min(64));
        for _ in 0..detail_count {
            listing_details.push(RankCharacterInfo::decode(r)?);
        }
        let listing_count = r.read_count()?;
        let mut listings = Vec::with_capacity(listing_count.min(64));
        for _ in 0..listing_count {
            listings.push(r.read_i64()?);
        }
        let count = r.read_i32()?;
        Ok(Self {
            rank_type,
            my_rank,
            listing_details,
            listings,
            count,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_u8(self.rank_type);
        w.write_i32(self.my_rank);
        w.write_count(self.listing_details.len());
        for detail in &self.listing_details {
            detail.encode(w);
        }
        w.write_count(self.listings.len());
        for &listing in &self.listings {
            w.write_i64(listing);
        }
        w.write_i32(self.count);
    }
}

/// Crystal `ServerPackets.GetRentedItems`（`Shared/ServerPackets.cs:6357`）。
///
/// 包编号 `ServerPacketIds.GetRentedItems` = 254（`Shared/Enums.cs:1690`）。
///
/// 手写原因：写侧是 `foreach (var rentedItemInformation in RentedItems) ... Save`
/// （`ServerPackets.cs:6375-6378`），生成器不支持对集合写 `foreach`
/// （`tools/protocol-gen/report.md:25`）。
///
/// 线格式：
///
/// ```text
/// i32  RentedItems.Count
/// ItemRentalInformation×N
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetRentedItems {
    /// 当前租出去的物品。
    pub rented_items: Vec<ItemRentalInformation>,
}

impl GetRentedItems {
    /// 包编号。
    pub const INDEX: i16 = 254;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let count = r.read_count()?;
        let mut rented_items = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            rented_items.push(ItemRentalInformation::decode(r)?);
        }
        Ok(Self { rented_items })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_count(self.rented_items.len());
        for item in &self.rented_items {
            item.encode(w);
        }
    }
}

/// Crystal `ServerPackets.UpdateRentalItem`（`Shared/ServerPackets.cs:6492`）。
///
/// 包编号 `ServerPacketIds.UpdateRentalItem` = 260（`Shared/Enums.cs:1696`）。
///
/// 手写原因：`if (HasData) LoanItem = new UserItem(reader);`
/// （`ServerPackets.cs:6504-6507`）这种「用布尔字段当存在标志」的读法生成器
/// 读不懂（`tools/protocol-gen/report.md:26`）。
///
/// 注意 C# 的**写侧用的不是 `HasData`**，而是 `writer.Write(LoanItem != null)`
/// （`ServerPackets.cs:6512`）：存在字节完全由 `loan_item` 决定，`has_data`
/// 在编码时被忽略。服务端也从不填 `HasData`，只填 `LoanItem`
/// （`Server/MirObjects/PlayerObject.cs:14225`）—— 这个字段纯属读侧产物，
/// 解码后 `has_data == loan_item.is_some()` 恒成立。
///
/// 线格式：
///
/// ```text
/// bool     LoanItem 存在？
/// UserItem LoanItem        <- 仅在上一个 bool 为 true 时
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UpdateRentalItem {
    /// C# 字段 `HasData`：解码时记录存在字节；编码时**不参与**。
    pub has_data: bool,
    /// 出租中的物品。
    pub loan_item: Option<UserItem>,
}

impl UpdateRentalItem {
    /// 包编号。
    pub const INDEX: i16 = 260;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let has_data = r.read_bool()?;
        let loan_item = if has_data {
            Some(UserItem::decode(r)?)
        } else {
            None
        };
        Ok(Self {
            has_data,
            loan_item,
        })
    }

    /// 编码：存在字节取自 `loan_item`，与 C# `ServerPackets.cs:6512` 一致。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_bool(self.loan_item.is_some());
        if let Some(loan_item) = &self.loan_item {
            loan_item.encode(w);
        }
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

fn encode_optional_items(w: &mut crate::PacketWriter, items: Option<&[Option<UserItem>]>) {
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

/// Crystal `ServerPackets.ObjectHero`（index 25）。
///
/// 手写原因：它继承 `ObjectPlayer` 并在末尾追加 `OwnerName`（Crystal 里是
/// `class ObjectHero : ObjectPlayer`），生成器只处理直接继承 `Packet` 的类。
/// 线格式 = `ObjectPlayer` 的全部字段 + `string OwnerName`。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjectHero {
    /// 英雄对象，字段顺序与 `ObjectPlayer` 完全一致。
    pub player: crate::packets::server::ObjectPlayer,
    /// 主人名字。
    pub owner_name: String,
}

impl ObjectHero {
    /// 包编号（`ServerPacketIds.ObjectHero`）。
    pub const INDEX: i16 = 25;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let player = crate::packets::server::ObjectPlayer::decode(r)?;
        let owner_name = r.read_string()?;
        Ok(Self { player, owner_name })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        self.player.encode(w);
        w.write_string(&self.owner_name);
    }
}

/// Crystal `ServerPackets.HeroInformation`（index 178）。
///
/// 手写原因：它继承 `UserInformation` 但**完全覆盖**了 `ReadPacket`/`WritePacket`，
/// 线格式与父类不同（少了公会/坐标/仓库等，多了自动喝药设置）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeroInformation {
    /// 对象 ID。
    pub object_id: u32,
    /// 英雄名字。
    pub name: String,
    /// 职业。
    pub class: MirClass,
    /// 性别。
    pub gender: MirGender,
    /// 等级。
    pub level: u16,
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
    /// 背包（`None` 表示不下发）。
    pub inventory: Option<Vec<Option<UserItem>>>,
    /// 装备（`None` 表示不下发）。
    pub equipment: Option<Vec<Option<UserItem>>>,
    /// 已学技能。
    pub magics: Vec<ClientMagic>,
    /// 是否开启自动喝药。
    pub auto_pot: bool,
    /// 自动喝红药阈值（百分比）。
    pub auto_hp_percent: u8,
    /// 自动喝蓝药阈值（百分比）。
    pub auto_mp_percent: u8,
    /// 自动红药物品编号。
    pub hp_item_index: i32,
    /// 自动蓝药物品编号。
    pub mp_item_index: i32,
}

impl HeroInformation {
    /// 包编号（`ServerPacketIds.HeroInformation`）。
    pub const INDEX: i16 = 178;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let object_id = r.read_u32()?;
        let name = r.read_string()?;
        let class = MirClass::from_value(r.read_u8()?).unwrap_or_default();
        let gender = MirGender::from_value(r.read_u8()?).unwrap_or_default();
        let level = r.read_u16()?;
        let hair = r.read_u8()?;
        let hp = r.read_i32()?;
        let mp = r.read_i32()?;
        let experience = r.read_i64()?;
        let max_experience = r.read_i64()?;
        let inventory = decode_optional_items(r)?;
        let equipment = decode_optional_items(r)?;
        let magic_count = r.read_count()?;
        let mut magics = Vec::with_capacity(magic_count.min(64));
        for _ in 0..magic_count {
            magics.push(ClientMagic::decode(r)?);
        }
        let auto_pot = r.read_bool()?;
        let auto_hp_percent = r.read_u8()?;
        let auto_mp_percent = r.read_u8()?;
        let hp_item_index = r.read_i32()?;
        let mp_item_index = r.read_i32()?;
        Ok(Self {
            object_id,
            name,
            class,
            gender,
            level,
            hair,
            hp,
            mp,
            experience,
            max_experience,
            inventory,
            equipment,
            magics,
            auto_pot,
            auto_hp_percent,
            auto_mp_percent,
            hp_item_index,
            mp_item_index,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_u32(self.object_id);
        w.write_string(&self.name);
        w.write_u8(self.class as u8);
        w.write_u8(self.gender as u8);
        w.write_u16(self.level);
        w.write_u8(self.hair);
        w.write_i32(self.hp);
        w.write_i32(self.mp);
        w.write_i64(self.experience);
        w.write_i64(self.max_experience);
        encode_optional_items(w, self.inventory.as_deref());
        encode_optional_items(w, self.equipment.as_deref());
        w.write_count(self.magics.len());
        for magic in &self.magics {
            magic.encode(w);
        }
        w.write_bool(self.auto_pot);
        w.write_u8(self.auto_hp_percent);
        w.write_u8(self.auto_mp_percent);
        w.write_i32(self.hp_item_index);
        w.write_i32(self.mp_item_index);
    }
}

#[cfg(test)]
mod tests {
    use mir_core::enums::{ItemGrade, ItemType};

    use crate::data::guild_rank::GuildMember;

    use super::*;

    /// 编码后再解码，断言相等且负载被正好读完。
    ///
    /// 借用而不是移动 `$value`，这样同一个测试里还能继续用原值做字节级断言。
    macro_rules! roundtrip {
        ($ty:ty, $value:expr) => {{
            let value = &$value;
            let mut writer = crate::PacketWriter::new();
            value.encode(&mut writer);
            let bytes = writer.into_inner();
            let mut reader = crate::PacketReader::new(&bytes);
            let decoded = <$ty>::decode(&mut reader).expect("解码应当成功");
            assert_eq!(decoded, *value, "解码结果应与原值相同");
            assert!(
                reader.is_empty(),
                "应正好读完负载，剩余 {} 字节",
                reader.remaining()
            );
        }};
    }

    fn user_item() -> UserItem {
        UserItem {
            unique_id: 0x0102_0304_0506_0708,
            item_index: 4242,
            current_dura: 1000,
            max_dura: 5000,
            count: 3,
            soul_bound_id: 77,
            identified: true,
            cursed: true,
            gem_count: 2,
            ..UserItem::default()
        }
    }

    fn item_info() -> ItemInfo {
        ItemInfo {
            index: 4242,
            name: "黑铁头盔".to_owned(),
            item_type: ItemType::Helmet,
            grade: ItemGrade::Rare,
            shape: 12,
            ..ItemInfo::default()
        }
    }

    fn guild_storage_item(user_id: i64) -> GuildStorageItem {
        GuildStorageItem {
            item: user_item(),
            user_id,
        }
    }

    fn guild_rank() -> GuildRank {
        GuildRank {
            name: "会长".to_owned(),
            options: 0xFF,
            index: 0,
            members: vec![GuildMember {
                name: "张三".to_owned(),
                id: 7,
                last_login: DateTimeBinary(0x1122_3344_5566_7788),
                has_voted: true,
                online: true,
            }],
        }
    }

    fn rank_info(name: &str, player_id: i64) -> RankCharacterInfo {
        RankCharacterInfo {
            player_id,
            name: name.to_owned(),
            level: 42,
            class: MirClass::Taoist,
        }
    }

    fn rented_item(name: &str) -> ItemRentalInformation {
        ItemRentalInformation {
            item_id: 0x0A0B_0C0D_0E0F_1011,
            item_name: name.to_owned(),
            renting_player_name: "租客".to_owned(),
            item_return_date: DateTimeBinary(0x1122_3344_5566_7788),
        }
    }

    fn shop_item() -> GameShopItem {
        GameShopItem {
            item_index: 4242,
            g_index: 7,
            info: item_info(),
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

    #[test]
    fn user_slots_refresh_roundtrip() {
        let packet = UserSlotsRefresh {
            inventory: Some(vec![Some(user_item()), None]),
            equipment: Some(Vec::new()),
        };
        roundtrip!(UserSlotsRefresh, packet);

        let mut item_writer = crate::PacketWriter::new();
        user_item().encode(&mut item_writer);
        let item = item_writer.into_inner();

        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();

        // 背包：存在 + 2 格（第 0 格有物品、第 1 格空）；装备：存在 + 0 格。
        let mut expected = vec![0x01];
        expected.extend_from_slice(&2i32.to_le_bytes());
        expected.push(0x01); // 第 0 格有物品
        expected.extend_from_slice(&item);
        expected.push(0x00); // 第 1 格为空
        expected.push(0x01); // 装备存在
        expected.extend_from_slice(&0i32.to_le_bytes());
        assert_eq!(bytes, expected);
    }

    #[test]
    fn user_slots_refresh_none_writes_one_byte_each() {
        let packet = UserSlotsRefresh::default();
        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        assert_eq!(writer.into_inner(), vec![0x00, 0x00]);
        roundtrip!(UserSlotsRefresh, packet);
    }

    #[test]
    fn magic_roundtrip() {
        let packet = Magic {
            spell: Spell::FireBall,
            target_id: 0x0102_0304,
            target: Point { x: -1, y: 2 },
            cast: true,
            level: 5,
            secondary_target_ids: vec![7, 8],
        };
        roundtrip!(Magic, packet);

        // 字段顺序：spell(u8) / target_id(u32) / x(i32) / y(i32) / cast(bool) /
        // level(u8) / count(i32) / 两个 u32。
        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        assert_eq!(bytes.len(), 1 + 4 + 4 + 4 + 1 + 1 + 4 + 2 * 4);
        assert_eq!(bytes[0], Spell::FireBall.value());
        assert_eq!(&bytes[1..5], &packet.target_id.to_le_bytes());
        assert_eq!(&bytes[5..9], &packet.target.x.to_le_bytes());
        assert_eq!(&bytes[9..13], &packet.target.y.to_le_bytes());
        assert_eq!(bytes[13], 0x01);
        assert_eq!(bytes[14], packet.level);
        assert_eq!(&bytes[15..19], &2i32.to_le_bytes());
        assert_eq!(&bytes[19..23], &7u32.to_le_bytes());
        assert_eq!(&bytes[23..27], &8u32.to_le_bytes());
    }

    #[test]
    fn object_magic_roundtrip() {
        let packet = ObjectMagic {
            object_id: 0x1122_3344,
            location: Point { x: 300, y: 400 },
            direction: MirDirection::UpLeft,
            spell: Spell::FireBall,
            target_id: 0x5566_7788,
            target: Point { x: -9, y: -10 },
            cast: true,
            level: 3,
            self_broadcast: true,
            secondary_target_ids: vec![11, 12],
        };
        roundtrip!(ObjectMagic, packet);

        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        // 4 + 8 + 1 + 1 + 4 + 8 + 1 + 1 + 1 + 4 + 2×4 = 41
        assert_eq!(bytes.len(), 41);
        assert_eq!(&bytes[..4], &packet.object_id.to_le_bytes());
        assert_eq!(bytes[12], packet.direction as u8);
        assert_eq!(bytes[13], Spell::FireBall.value());
        assert_eq!(&bytes[14..18], &packet.target_id.to_le_bytes());
        assert_eq!(bytes[26], 0x01, "Cast");
        assert_eq!(bytes[27], packet.level);
        assert_eq!(bytes[28], 0x01, "SelfBroadcast 必须在个数之前");
        assert_eq!(&bytes[29..33], &2i32.to_le_bytes());
        assert_eq!(&bytes[33..37], &11u32.to_le_bytes());
        assert_eq!(&bytes[37..41], &12u32.to_le_bytes());
    }

    #[test]
    fn user_storage_roundtrip() {
        let packet = UserStorage {
            storage: Some(vec![Some(user_item()), None]),
        };
        roundtrip!(UserStorage, packet);

        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut item_writer = crate::PacketWriter::new();
        user_item().encode(&mut item_writer);
        let item_len = item_writer.len();
        // 1(bool) + 4(个数) + 1(第 0 格标志) + item + 1(第 1 格标志)
        assert_eq!(bytes.len(), 7 + item_len);
        assert_eq!(bytes[0], 0x01);
        assert_eq!(&bytes[1..5], &2i32.to_le_bytes());
        assert_eq!(bytes[5], 0x01);
        assert_eq!(bytes[6 + item_len], 0x00);
    }

    #[test]
    fn user_storage_none_is_a_single_false() {
        let packet = UserStorage::default();
        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        assert_eq!(writer.into_inner(), vec![0x00]);
        roundtrip!(UserStorage, packet);

        // 空数组与 `None` 不同：`Some(vec![])` 写 true + 0 个格子。
        let empty = UserStorage {
            storage: Some(Vec::new()),
        };
        let mut writer = crate::PacketWriter::new();
        empty.encode(&mut writer);
        assert_eq!(writer.into_inner(), vec![0x01, 0x00, 0x00, 0x00, 0x00]);
        roundtrip!(UserStorage, empty);
    }

    #[test]
    fn chat_item_stats_roundtrip_when_stats_present() {
        let packet = ChatItemStats {
            chat_item_id: 0x0102_0304_0506_0708,
            stats: Some(user_item()),
        };
        roundtrip!(ChatItemStats, packet);

        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut item_writer = crate::PacketWriter::new();
        user_item().encode(&mut item_writer);
        assert_eq!(bytes.len(), 8 + item_writer.len());
        assert_eq!(&bytes[..8], &packet.chat_item_id.to_le_bytes());
    }

    #[test]
    fn chat_item_stats_write_skips_absent_stats_but_read_always_expects_one() {
        // Crystal 的读写不对称（`Shared/ServerPackets.cs:4460` vs `:4466`）：
        // 有 `stats` 时字节对称；`stats = None` 时**只写 8 字节**，
        // 而读端总会再找一整个 `UserItem`，于是必然 `UnexpectedEof`。
        let absent = ChatItemStats {
            chat_item_id: 0x0102_0304_0506_0708,
            stats: None,
        };
        let mut writer = crate::PacketWriter::new();
        absent.encode(&mut writer);
        let bytes = writer.into_inner();
        assert_eq!(bytes.len(), 8, "没有存在标志位，所以只剩 ChatItemId");
        assert_eq!(bytes, absent.chat_item_id.to_le_bytes());

        let mut reader = crate::PacketReader::new(&bytes);
        assert!(matches!(
            ChatItemStats::decode(&mut reader),
            Err(crate::ProtocolError::UnexpectedEof { .. })
        ));

        // 反过来：读侧恒产出 `Some`，所以 `None` 永远不可能被解出来。
        // 同一个 `ChatItemId`，只要后面跟着一个完整 `UserItem` 就能正常解开 ——
        // 线上没有存在标志位，区别只在于物品负载在不在。
        let present = ChatItemStats {
            chat_item_id: absent.chat_item_id,
            stats: Some(user_item()),
        };
        let mut writer = crate::PacketWriter::new();
        present.encode(&mut writer);
        let present_bytes = writer.into_inner();
        let mut reader = crate::PacketReader::new(&present_bytes);
        let decoded = ChatItemStats::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, present);
        assert!(reader.is_empty());
    }

    #[test]
    fn guild_notice_change_roundtrip() {
        let packet = GuildNoticeChange {
            update: 3,
            notice: vec![
                "欢迎来到行会".to_owned(),
                "第二条".to_owned(),
                "第三条".to_owned(),
            ],
        };
        roundtrip!(GuildNoticeChange, packet);

        // `WritePacket` 用的是 `notice.Count`（`ServerPackets.cs:4491`），
        // 不是 `update`：两者一致时字节才是「i32 个数 + 3 条字符串」。
        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        assert_eq!(&bytes[..4], &3i32.to_le_bytes());
    }

    #[test]
    fn guild_notice_change_negative_update_is_a_bare_int() {
        // `update = -1` 是「公告已变，请刷新」的信号
        // （`Server/MirObjects/GuildObject.cs:599` 只发这个信号），
        // 写侧见到负值只写 4 字节就返回（`ServerPackets.cs:4486-4490`）。
        let packet = GuildNoticeChange {
            update: -1,
            notice: Vec::new(),
        };
        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        assert_eq!(bytes, (-1i32).to_le_bytes());
        roundtrip!(GuildNoticeChange, packet);

        // 长度不一致时字节流与 `update` 不同步：写 `notice.len()`，
        // 读回来的 `update` 变成 `notice.len()`。
        let mismatched = GuildNoticeChange {
            update: 5,
            notice: vec!["a".to_owned()],
        };
        let mut writer = crate::PacketWriter::new();
        mismatched.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = crate::PacketReader::new(&bytes);
        let decoded = GuildNoticeChange::decode(&mut reader).expect("解码应当成功");
        assert_eq!(
            decoded,
            GuildNoticeChange {
                update: 1,
                notice: vec!["a".to_owned()],
            }
        );
    }

    #[test]
    fn guild_member_change_roundtrip_with_ranks() {
        let packet = GuildMemberChange {
            name: "张三".to_owned(),
            rank_index: 2,
            // 255 = 整份成员名单（`Client/MirScenes/GameScene.cs:5854`）。
            status: 255,
            ranks: vec![guild_rank()],
        };
        roundtrip!(GuildMemberChange, packet);
    }

    #[test]
    fn guild_member_change_below_six_has_no_rank_block() {
        let packet = GuildMemberChange {
            name: "Li Si".to_owned(),
            rank_index: 1,
            status: 3,
            ranks: Vec::new(),
        };
        roundtrip!(GuildMemberChange, packet);

        // 读写顺序是 Name / RankIndex / Status，所以负载只有这三段。
        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        assert_eq!(bytes.len(), 1 + packet.name.len() + 1 + 1);

        // `status <= 5` 时即使 `ranks` 非空也**不写**（`ServerPackets.cs:4524`），
        // 因此这种组合 roundtrip 会丢数据 —— 这是 Crystal 的形状，不是实现漏洞。
        let ignored = GuildMemberChange {
            name: "Li Si".to_owned(),
            rank_index: 1,
            status: 5,
            ranks: vec![guild_rank()],
        };
        let mut writer = crate::PacketWriter::new();
        ignored.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = crate::PacketReader::new(&bytes);
        let decoded = GuildMemberChange::decode(&mut reader).expect("解码应当成功");
        assert!(decoded.ranks.is_empty(), "status <= 5 时职位列表不上线");
    }

    #[test]
    fn guild_storage_item_change_roundtrip() {
        let packet = GuildStorageItemChange {
            user: -7,
            change_type: 2,
            to: 3,
            from: 4,
            item: Some(guild_storage_item(0x1122_3344_5566_7788)),
        };
        roundtrip!(GuildStorageItemChange, packet);
    }

    #[test]
    fn guild_storage_item_change_writes_user_id_before_the_item() {
        // 本包的字段顺序与 `GuildStorageItem.Save` **相反**
        // （`ServerPackets.cs:4665-4666` / `:4677-4678` vs
        // `Shared/Data/GuildData.cs:50` / `:55`）。
        let packet = GuildStorageItemChange {
            user: 9,
            change_type: 0,
            to: 1,
            from: 2,
            item: Some(guild_storage_item(0x1122_3344_5566_7788)),
        };
        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();

        // Type(1) + To(4) + From(4) + User(4) + 存在标志(1) = 14 字节之后，
        // 先出现 `UserId`（i64），再出现 `UserItem`。
        assert_eq!(bytes[0], 0);
        assert_eq!(&bytes[1..5], &1i32.to_le_bytes());
        assert_eq!(&bytes[5..9], &2i32.to_le_bytes());
        assert_eq!(&bytes[9..13], &9i32.to_le_bytes());
        assert_eq!(bytes[13], 0x01);
        assert_eq!(
            &bytes[14..22],
            &0x1122_3344_5566_7788i64.to_le_bytes(),
            "先写 UserId"
        );
        assert_ne!(
            &bytes[14..22],
            &user_item().unique_id.to_le_bytes(),
            "若这里等于物品 UniqueID，说明误用了 GuildStorageItem::encode"
        );
        assert_eq!(
            &bytes[22..30],
            &user_item().unique_id.to_le_bytes(),
            "物品从 UserId 之后开始"
        );
    }

    #[test]
    fn guild_storage_item_change_without_item_is_a_short_prefix() {
        let packet = GuildStorageItemChange {
            user: 5,
            change_type: 5,
            to: 6,
            from: 7,
            item: None,
        };
        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        assert_eq!(bytes.len(), 14);
        assert_eq!(bytes[13], 0x00);
        roundtrip!(GuildStorageItemChange, packet);
    }

    #[test]
    fn guild_storage_list_roundtrip() {
        let packet = GuildStorageList {
            items: vec![
                Some(guild_storage_item(-1)),
                None,
                Some(guild_storage_item(i64::MAX)),
            ],
        };
        roundtrip!(GuildStorageList, packet);

        let mut entry_writer = crate::PacketWriter::new();
        guild_storage_item(-1).encode(&mut entry_writer);
        let entry = entry_writer.into_inner();

        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        assert_eq!(&bytes[..4], &3i32.to_le_bytes());
        assert_eq!(bytes[4], 0x01, "第 0 格有物品");
        assert_eq!(&bytes[5..5 + entry.len()], entry.as_slice());
        assert_eq!(bytes[5 + entry.len()], 0x00, "第 1 格为空");
        assert_eq!(bytes[6 + entry.len()], 0x01, "第 2 格有物品");
    }

    #[test]
    fn hero_create_request_roundtrip() {
        let packet = HeroCreateRequest {
            can_create_class: vec![true, false, true],
        };
        roundtrip!(HeroCreateRequest, packet);

        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        assert_eq!(writer.into_inner(), vec![3, 0, 0, 0, 0x01, 0x00, 0x01]);
    }

    #[test]
    fn complete_quest_roundtrip() {
        let packet = CompleteQuest {
            completed_quests: vec![-1, 0, 42],
        };
        roundtrip!(CompleteQuest, packet);

        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        assert_eq!(
            writer.into_inner(),
            vec![
                3, 0, 0, 0, // 个数
                0xFF, 0xFF, 0xFF, 0xFF, // -1
                0, 0, 0, 0, // 0
                42, 0, 0, 0, // 42
            ]
        );
    }

    #[test]
    fn awakening_need_materials_roundtrip() {
        let packet = AwakeningNeedMaterials {
            materials: Some(vec![Some((item_info(), 3)), None]),
        };
        roundtrip!(AwakeningNeedMaterials, packet);

        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut info_writer = crate::PacketWriter::new();
        item_info().encode(&mut info_writer);
        let info_len = info_writer.len();
        // 1(存在) + 4(格数) + 1(第 0 格标志) + info + 1(数量) + 1(第 1 格标志)
        assert_eq!(bytes.len(), 8 + info_len);
        assert_eq!(bytes[0], 0x01);
        assert_eq!(&bytes[1..5], &2i32.to_le_bytes());
        assert_eq!(bytes[5], 0x01);
        assert_eq!(bytes[6 + info_len], 3, "MaterialsCount[0]");
        assert_eq!(bytes[7 + info_len], 0x00, "第 1 格为空");
    }

    #[test]
    fn awakening_need_materials_none_is_a_single_false() {
        let packet = AwakeningNeedMaterials::default();
        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        assert_eq!(writer.into_inner(), vec![0x00]);
        roundtrip!(AwakeningNeedMaterials, packet);

        let empty = AwakeningNeedMaterials {
            materials: Some(Vec::new()),
        };
        let mut writer = crate::PacketWriter::new();
        empty.encode(&mut writer);
        assert_eq!(writer.into_inner(), vec![0x01, 0x00, 0x00, 0x00, 0x00]);
        roundtrip!(AwakeningNeedMaterials, empty);
    }

    #[test]
    fn game_shop_info_roundtrip() {
        let packet = GameShopInfo {
            item: shop_item(),
            stock_level: 42,
        };
        roundtrip!(GameShopInfo, packet);

        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut item_writer = crate::PacketWriter::new();
        shop_item().encode(&mut item_writer);
        let item_len = item_writer.len();
        assert_eq!(bytes.len(), item_len + 4);
        assert_eq!(&bytes[item_len..], &42i32.to_le_bytes());
    }

    #[test]
    fn rankings_roundtrip() {
        let packet = Rankings {
            rank_type: 1,
            my_rank: -5,
            listing_details: vec![rank_info("张三", 100), rank_info("Li Si", -2)],
            listings: vec![10, -20],
            count: 99,
        };
        roundtrip!(Rankings, packet);

        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut details_writer = crate::PacketWriter::new();
        for detail in &packet.listing_details {
            detail.encode(&mut details_writer);
        }
        let details_len = details_writer.len();
        assert_eq!(bytes[0], packet.rank_type);
        assert_eq!(&bytes[1..5], &packet.my_rank.to_le_bytes());
        assert_eq!(&bytes[5..9], &2i32.to_le_bytes());
        let listings_start = 9 + details_len;
        assert_eq!(
            &bytes[listings_start..listings_start + 4],
            &2i32.to_le_bytes()
        );
        assert_eq!(
            &bytes[listings_start + 4..listings_start + 12],
            &10i64.to_le_bytes()
        );
        assert_eq!(
            &bytes[listings_start + 12..listings_start + 20],
            &(-20i64).to_le_bytes()
        );
        // 末尾的 `Count` 是独立字段，不是第三份列表的个数前缀。
        assert_eq!(&bytes[listings_start + 20..], &99i32.to_le_bytes());
        assert_eq!(bytes.len(), listings_start + 20 + 4);
    }

    #[test]
    fn get_rented_items_roundtrip() {
        let packet = GetRentedItems {
            rented_items: vec![rented_item("屠龙"), rented_item("开天")],
        };
        roundtrip!(GetRentedItems, packet);

        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut first = crate::PacketWriter::new();
        rented_item("屠龙").encode(&mut first);
        assert_eq!(&bytes[..4], &2i32.to_le_bytes());
        assert_eq!(&bytes[4..4 + first.len()], first.into_inner());
    }

    #[test]
    fn update_rental_item_roundtrip() {
        let packet = UpdateRentalItem {
            has_data: true,
            loan_item: Some(user_item()),
        };
        roundtrip!(UpdateRentalItem, packet);

        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut item_writer = crate::PacketWriter::new();
        user_item().encode(&mut item_writer);
        assert_eq!(bytes.len(), 1 + item_writer.len());
        assert_eq!(bytes[0], 0x01);
    }

    #[test]
    fn update_rental_item_presence_byte_comes_from_loan_item() {
        // 写侧是 `writer.Write(LoanItem != null)`（`ServerPackets.cs:6512`），
        // `HasData` 不参与编码；解码后 `has_data == loan_item.is_some()` 恒成立。
        let inconsistent = UpdateRentalItem {
            has_data: true,
            loan_item: None,
        };
        let mut writer = crate::PacketWriter::new();
        inconsistent.encode(&mut writer);
        let bytes = writer.into_inner();
        assert_eq!(bytes, vec![0x00], "存在字节只看 LoanItem");

        let mut reader = crate::PacketReader::new(&bytes);
        let decoded = UpdateRentalItem::decode(&mut reader).expect("解码应当成功");
        assert_eq!(
            decoded,
            UpdateRentalItem {
                has_data: false,
                loan_item: None,
            }
        );

        let absent = UpdateRentalItem::default();
        let mut writer = crate::PacketWriter::new();
        absent.encode(&mut writer);
        assert_eq!(writer.into_inner(), vec![0x00]);
        roundtrip!(UpdateRentalItem, absent);
    }
}

/// 本项目扩展，Crystal 无此包：攻城区配置/状态（索引 300）。
///
/// 进入攻城区地图或攻城区状态变化（开战/停战/归属变化）时下发；客户端只读展示。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConquestInfo {
    /// 攻城区编号。
    pub index: i32,
    /// 攻城区名称。
    pub name: String,
    /// 当前城主行会 ID，`-1` = 无主。
    pub owner: i32,
    /// 是否正在开战。
    pub war_is_on: bool,
    /// 开战时刻（Unix 毫秒；0 = 未开战）。
    pub start_time: i64,
    /// 停战时刻（Unix 毫秒；0 = 未设置）。
    pub end_time: i64,
    /// 控制点归属与分数。
    pub control_points: Vec<ConquestControlPoint>,
}

impl ConquestInfo {
    /// 本项目扩展的包编号（生成包 0..=278 之外）。
    pub const INDEX: i16 = 300;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let index = r.read_i32()?;
        let name = r.read_string()?;
        let owner = r.read_i32()?;
        let war_is_on = r.read_bool()?;
        let start_time = r.read_i64()?;
        let end_time = r.read_i64()?;
        let count = r.read_count()?;
        if count > 256 {
            return Err(crate::ProtocolError::CollectionTooLong {
                length: count,
                limit: 256,
            });
        }
        let mut control_points = Vec::with_capacity(count);
        for _ in 0..count {
            control_points.push(ConquestControlPoint::decode(r)?);
        }
        Ok(Self {
            index,
            name,
            owner,
            war_is_on,
            start_time,
            end_time,
            control_points,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_i32(self.index);
        w.write_string(&self.name);
        w.write_i32(self.owner);
        w.write_bool(self.war_is_on);
        w.write_i64(self.start_time);
        w.write_i64(self.end_time);
        w.write_count(self.control_points.len());
        for point in &self.control_points {
            point.encode(w);
        }
    }
}

/// 本项目扩展：控制点归属与分数。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConquestControlPoint {
    /// 控制点编号。
    pub index: i32,
    /// 归属行会 ID，`-1` = 无主。
    pub owner: i32,
    /// 当前分数。
    pub points: i32,
}

impl ConquestControlPoint {
    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        Ok(Self {
            index: r.read_i32()?,
            owner: r.read_i32()?,
            points: r.read_i32()?,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_i32(self.index);
        w.write_i32(self.owner);
        w.write_i32(self.points);
    }
}

/// 本项目扩展：攻城器械类型（`ConquestObject.kind`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConquestObjectType {
    /// 城门。
    #[default]
    Gate,
    /// 城墙。
    Wall,
    /// 箭塔。
    Archer,
}

impl ConquestObjectType {
    /// 数值编码。
    pub const fn value(self) -> u8 {
        match self {
            Self::Gate => 0,
            Self::Wall => 1,
            Self::Archer => 2,
        }
    }

    /// 从数值解码；未知值返回 `None`。
    pub const fn from_value(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Gate),
            1 => Some(Self::Wall),
            2 => Some(Self::Archer),
            _ => None,
        }
    }
}

/// 本项目扩展，Crystal 无此包：攻城器械对象（索引 301）。
///
/// 进入视野、受伤、摧毁时下发；`hp == 0` 表示已摧毁。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConquestObject {
    /// 器械对象 ID（与玩家/怪物共用 ID 空间）。
    pub object_id: u32,
    /// 所属攻城区编号。
    pub conquest_index: i32,
    /// 器械类型。
    pub kind: ConquestObjectType,
    /// 位置。
    pub location: Point,
    /// 朝向。
    pub direction: MirDirection,
    /// 当前 HP。
    pub hp: i32,
    /// 最大 HP。
    pub max_hp: i32,
    /// 归属行会 ID，`-1` = 无主。
    pub owner: i32,
    /// 是否已摧毁。
    pub destroyed: bool,
}

impl ConquestObject {
    /// 本项目扩展的包编号（生成包 0..=278 之外）。
    pub const INDEX: i16 = 301;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let object_id = r.read_u32()?;
        let conquest_index = r.read_i32()?;
        let kind = ConquestObjectType::from_value(r.read_u8()?).unwrap_or_default();
        let location = Point {
            x: r.read_i32()?,
            y: r.read_i32()?,
        };
        let direction = MirDirection::from_value(r.read_u8()?).unwrap_or_default();
        let hp = r.read_i32()?;
        let max_hp = r.read_i32()?;
        let owner = r.read_i32()?;
        let destroyed = r.read_bool()?;
        Ok(Self {
            object_id,
            conquest_index,
            kind,
            location,
            direction,
            hp,
            max_hp,
            owner,
            destroyed,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_u32(self.object_id);
        w.write_i32(self.conquest_index);
        w.write_u8(self.kind.value());
        w.write_i32(self.location.x);
        w.write_i32(self.location.y);
        w.write_u8(self.direction as u8);
        w.write_i32(self.hp);
        w.write_i32(self.max_hp);
        w.write_i32(self.owner);
        w.write_bool(self.destroyed);
    }
}

/// 本项目扩展，Crystal 无此包：攻城结束结果（索引 302）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConquestResult {
    /// 攻城区编号。
    pub index: i32,
    /// 胜者行会 ID，`-1` = 无胜者/守城成功。
    pub winner: i32,
    /// 结束后归属行会 ID，`-1` = 无主。
    pub owner: i32,
    /// 结束后控制点归属。
    pub control_points: Vec<ConquestControlPoint>,
}

impl ConquestResult {
    /// 本项目扩展的包编号（生成包 0..=278 之外）。
    pub const INDEX: i16 = 302;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let index = r.read_i32()?;
        let winner = r.read_i32()?;
        let owner = r.read_i32()?;
        let count = r.read_count()?;
        if count > 256 {
            return Err(crate::ProtocolError::CollectionTooLong {
                length: count,
                limit: 256,
            });
        }
        let mut control_points = Vec::with_capacity(count);
        for _ in 0..count {
            control_points.push(ConquestControlPoint::decode(r)?);
        }
        Ok(Self {
            index,
            winner,
            owner,
            control_points,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_i32(self.index);
        w.write_i32(self.winner);
        w.write_i32(self.owner);
        w.write_count(self.control_points.len());
        for point in &self.control_points {
            point.encode(w);
        }
    }
}

#[cfg(test)]
mod conquest_packet_tests {
    use super::*;
    use crate::packets::server::ServerPacket;

    fn point() -> ConquestControlPoint {
        ConquestControlPoint {
            index: 2,
            owner: 7,
            points: 42,
        }
    }

    #[test]
    fn conquest_info_roundtrips_through_server_packet() {
        let packet = ConquestInfo {
            index: 3,
            name: "沙巴克".to_owned(),
            owner: 7,
            war_is_on: true,
            start_time: 1_700_000_000_000,
            end_time: 1_700_003_600_000,
            control_points: vec![point()],
        };
        let frame = ServerPacket::ConquestInfo(packet.clone()).encode_frame();
        assert_eq!(&frame[2..4], &ConquestInfo::INDEX.to_le_bytes());
        let decoded = ServerPacket::decode(ConquestInfo::INDEX, &frame[4..]).expect("解码");
        assert_eq!(decoded, ServerPacket::ConquestInfo(packet));
    }

    #[test]
    fn conquest_object_roundtrips_through_server_packet() {
        let packet = ConquestObject {
            object_id: 99,
            conquest_index: 3,
            kind: ConquestObjectType::Archer,
            location: Point { x: 120, y: 88 },
            direction: MirDirection::Left,
            hp: 300,
            max_hp: 500,
            owner: 7,
            destroyed: false,
        };
        let frame = ServerPacket::ConquestObject(packet.clone()).encode_frame();
        assert_eq!(&frame[2..4], &ConquestObject::INDEX.to_le_bytes());
        let decoded = ServerPacket::decode(ConquestObject::INDEX, &frame[4..]).expect("解码");
        assert_eq!(decoded, ServerPacket::ConquestObject(packet));
    }

    #[test]
    fn conquest_result_roundtrips_through_server_packet() {
        let packet = ConquestResult {
            index: 3,
            winner: 7,
            owner: 7,
            control_points: vec![point()],
        };
        let frame = ServerPacket::ConquestResult(packet.clone()).encode_frame();
        assert_eq!(&frame[2..4], &ConquestResult::INDEX.to_le_bytes());
        let decoded = ServerPacket::decode(ConquestResult::INDEX, &frame[4..]).expect("解码");
        assert_eq!(decoded, ServerPacket::ConquestResult(packet));
    }

    #[test]
    fn conquest_packet_indices_are_outside_generated_range() {
        assert_eq!(ConquestInfo::INDEX, 300);
        assert_eq!(ConquestObject::INDEX, 301);
        assert_eq!(ConquestResult::INDEX, 302);
    }
}
