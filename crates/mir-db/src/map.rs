//! 地图段：`MapInfo` 及四个子记录。
//!
//! 读入顺序见 `Server/MirDatabase/MapInfo.cs:47-124`（`MapInfo(BinaryReader reader)`），
//! 写出顺序见 `MapInfo.cs:126-194`（`Save`）。子记录各自的出处见类型文档。
//!
//! **版本门一律以 `Envir.LoadVersion` 为准**（`Envir.cs:69`）；`Save` 永远按最新版写，
//! 所以 `encode` 也接受一个 `version` 参数——它的唯一用途是让测试能重新生成旧版文件、
//! 并验证 `encode(decode(bytes)) == bytes`（见 `tests/samples.rs`）。

use mir_core::Point;

use crate::MirDbError;
use crate::reader::{DbReader, DbWriter};

/// `MapInfo` 记录的最小字节数：到 `Music` 为止（`MapInfo.cs:49-95`），不含任何版本门字段。
pub const MIN_MAP_RECORD_LEN: usize = 57;

/// `SafeZoneInfo` 的最小字节数（`SafeZoneInfo.cs:16-18`）：`i32 X` + `i32 Y` + `u16 Size` + `bool StartPoint`。
pub const MIN_SAFE_ZONE_LEN: usize = 11;

/// `RespawnInfo` 的最小字节数（`RespawnInfo.cs:30-39`）：不含 `Version > 67` 的四个尾字段。
pub const MIN_RESPAWN_LEN: usize = 20;

/// `MovementInfo` 的最小字节数（`MovementInfo.cs:20-25`）：不含 `>= 69` / `>= 95` 的尾字段。
pub const MIN_MOVEMENT_LEN: usize = 22;

/// `MineZone` 的最小字节数（`MineInfo.cs:84-86`）。
pub const MIN_MINE_ZONE_LEN: usize = 11;

/// Crystal `SafeZoneInfo`（`Server/MirDatabase/SafeZoneInfo.cs:4`；读 `:14-19`，写 `:21-27`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SafeZoneInfo {
    /// 安全区中心（`Location.X` / `Location.Y`，`:16`）。
    pub location: Point,
    /// 安全区半径（`u16`，`:17`）。
    pub size: u16,
    /// 是否是出生点（`:18`；`MapInfo.CreateMap` 会把它加进 `Envir.StartPoints`，`MapInfo.cs:212-214`）。
    pub start_point: bool,
}

impl SafeZoneInfo {
    /// 解码，见 `SafeZoneInfo.cs:14-19`。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        let x = r.i32()?;
        let y = r.i32()?;
        let size = r.u16()?;
        let start_point = r.bool()?;
        Ok(Self {
            location: Point::new(x, y),
            size,
            start_point,
        })
    }

    /// 编码，见 `SafeZoneInfo.cs:21-27`。无版本分支。
    pub fn encode(&self, w: &mut DbWriter) {
        w.i32(self.location.x);
        w.i32(self.location.y);
        w.u16(self.size);
        w.bool(self.start_point);
    }
}

/// Crystal `RespawnInfo`（`Server/MirDatabase/RespawnInfo.cs:6`；读 `:27-52`，写 `:82-100`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RespawnInfo {
    /// 怪物模板编号（`MonsterIndex`，`:30`）。
    pub monster_index: i32,
    /// 刷怪点中心（`:31`）。
    pub location: Point,
    /// 同时存在的数量上限（`u16`，`:33`）。
    pub count: u16,
    /// 刷新半径（`u16`，`:34`）。
    pub spread: u16,
    /// 刷新间隔（分钟，`u16`，`:36`）。
    pub delay: u16,
    /// 朝向（`u8`，`:37`）。
    pub direction: u8,
    /// 巡逻路线文件名（`:39`）。
    pub route_path: String,
    /// 随机延迟上限（`u16`，仅 `Version > 67`，`:41-47`）。
    /// 旧文件没有这个字段，读成 0。
    pub random_delay: u16,
    /// 刷怪点全局编号（`i32`，仅 `Version > 67`）。
    ///
    /// 旧文件里它是由 `++Envir.RespawnIndex` 现分配的运行期值（`RespawnInfo.cs:50`），
    /// 不落盘，所以这里读成 0。
    pub respawn_index: i32,
    /// 是否保存下次刷新时间（仅 `Version > 67`，`:45`）。
    pub save_respawn_time: bool,
    /// 刷新计时（`u16`，仅 `Version > 67`，`:46`）。
    pub respawn_ticks: u16,
}

impl RespawnInfo {
    /// 解码，见 `RespawnInfo.cs:27-52`。版本门：`Version > 67` 才读 `random_delay` /
    /// `respawn_index` / `save_respawn_time` / `respawn_ticks`。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        let monster_index = r.i32()?;
        let x = r.i32()?;
        let y = r.i32()?;
        let count = r.u16()?;
        let spread = r.u16()?;
        let delay = r.u16()?;
        let direction = r.u8()?;
        let route_path = r.string()?;

        let mut info = Self {
            monster_index,
            location: Point::new(x, y),
            count,
            spread,
            delay,
            direction,
            route_path,
            random_delay: 0,
            respawn_index: 0,
            save_respawn_time: false,
            respawn_ticks: 0,
        };

        if r.version() > 67 {
            info.random_delay = r.u16()?;
            info.respawn_index = r.i32()?;
            info.save_respawn_time = r.bool()?;
            info.respawn_ticks = r.u16()?;
        }
        Ok(info)
    }

    /// 编码，见 `RespawnInfo.cs:82-100`。
    ///
    /// `Save` 无条件写四个尾字段；`version <= 67` 时本函数跳过它们，以便复现旧文件。
    pub fn encode(&self, w: &mut DbWriter, version: i32) {
        w.i32(self.monster_index);
        w.i32(self.location.x);
        w.i32(self.location.y);
        w.u16(self.count);
        w.u16(self.spread);
        w.u16(self.delay);
        w.u8(self.direction);
        w.string(&self.route_path);

        if version > 67 {
            w.u16(self.random_delay);
            w.i32(self.respawn_index);
            w.bool(self.save_respawn_time);
            w.u16(self.respawn_ticks);
        }
    }
}

/// Crystal `MovementInfo`（`Server/MirDatabase/MovementInfo.cs:6`；读 `:18-33`，写 `:34-46`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MovementInfo {
    /// 目的地地图编号（`MovementInfo.cs:20`）。
    pub map_index: i32,
    /// 传送门所在格（`:21`）。
    pub source: Point,
    /// 落点（`:22`）。
    pub destination: Point,
    /// 是否需要挖洞才能通过（`:24`）。
    pub need_hole: bool,
    /// 是否需要踩上去才触发（`:25`）。
    pub need_move: bool,
    /// 所属攻城区域（`>= 69`，`:27-28`）。
    pub conquest_index: i32,
    /// 是否显示在大地图（`>= 95`，`:30-31`）。
    pub show_on_big_map: bool,
    /// 大地图图标（`>= 95`，`:32`）。
    pub icon: i32,
}

impl MovementInfo {
    /// 解码，见 `MovementInfo.cs:18-33`。版本门：`>= 69` 有 `conquest_index`，`>= 95` 有
    /// `show_on_big_map` + `icon`。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        let map_index = r.i32()?;
        let src_x = r.i32()?;
        let src_y = r.i32()?;
        let dst_x = r.i32()?;
        let dst_y = r.i32()?;
        let need_hole = r.bool()?;
        let need_move = r.bool()?;

        let mut info = Self {
            map_index,
            source: Point::new(src_x, src_y),
            destination: Point::new(dst_x, dst_y),
            need_hole,
            need_move,
            conquest_index: 0,
            show_on_big_map: false,
            icon: 0,
        };

        if r.version() >= 69 {
            info.conquest_index = r.i32()?;
        }
        if r.version() >= 95 {
            info.show_on_big_map = r.bool()?;
            info.icon = r.i32()?;
        }
        Ok(info)
    }

    /// 编码，见 `MovementInfo.cs:34-46`（`Save` 无条件写全；旧版本由 `version` 参数裁掉）。
    pub fn encode(&self, w: &mut DbWriter, version: i32) {
        w.i32(self.map_index);
        w.i32(self.source.x);
        w.i32(self.source.y);
        w.i32(self.destination.x);
        w.i32(self.destination.y);
        w.bool(self.need_hole);
        w.bool(self.need_move);
        if version >= 69 {
            w.i32(self.conquest_index);
        }
        if version >= 95 {
            w.bool(self.show_on_big_map);
            w.i32(self.icon);
        }
    }
}

/// Crystal `MineZone`（`Server/MirDatabase/MineInfo.cs:72`；读 `:82-87`，写 `:89-95`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MineZone {
    /// 矿区中心（`MineInfo.cs:84`）。
    pub location: Point,
    /// 半径（`u16`，`:85`）。
    pub size: u16,
    /// 矿产类型（`u8`，`:86`）。
    pub mine: u8,
}

impl MineZone {
    /// 解码，见 `MineInfo.cs:82-87`。无版本分支。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        let x = r.i32()?;
        let y = r.i32()?;
        let size = r.u16()?;
        let mine = r.u8()?;
        Ok(Self {
            location: Point::new(x, y),
            size,
            mine,
        })
    }

    /// 编码，见 `MineInfo.cs:89-95`。
    pub fn encode(&self, w: &mut DbWriter) {
        w.i32(self.location.x);
        w.i32(self.location.y);
        w.u16(self.size);
        w.u8(self.mine);
    }
}

/// Crystal `MapInfo`（`Server/MirDatabase/MapInfo.cs:6`；读 `:47-124`，写 `:126-194`）。
///
/// 字段顺序完全按 `Save` 的写出顺序。带版本门的字段（`no_town_teleport` 起）在旧文件里
/// 不存在，读成 C# 字段初始值（`false` / `0`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MapInfo {
    /// 主键（`MapInfo.cs:49`）。
    pub index: i32,
    /// 地图文件名（`:50`），例如 `"0"`。
    pub file_name: String,
    /// 显示名（`:51`），例如 `"比奇省"`。
    pub title: String,
    /// 小地图编号（`u16`，`:52`）。
    pub mini_map: u16,
    /// 光照设置（`LightSetting`，`:53`）。这里保留原始字节以便逐字节往返，
    /// 服务层再转 `mir_core::enums::LightSetting`。
    pub light: u8,
    /// 大地图编号（`u16`，`:55`）。
    pub big_map: u16,
    /// 安全区（`:57-59`）。
    pub safe_zones: Vec<SafeZoneInfo>,
    /// 刷怪点（`:61-63`）。
    pub respawns: Vec<RespawnInfo>,
    /// 传送点（`:65-67`）。
    pub movements: Vec<MovementInfo>,
    /// 禁止瞬移（`:69`）。
    pub no_teleport: bool,
    /// 禁止断线重连（`:70`）。
    pub no_reconnect: bool,
    /// 禁止重连时被送回的地图名（`:71`）。
    pub no_reconnect_map: String,
    /// 禁止随机传送（`:73`）。
    pub no_random: bool,
    /// 禁止回城（`:74`）。
    pub no_escape: bool,
    /// 禁止招募（`:75`）。
    pub no_recall: bool,
    /// 禁止使用药品（`:76`）。
    pub no_drug: bool,
    /// 禁止查看坐标（`:77`）。
    pub no_position: bool,
    /// 禁止丢弃物品（`:78`）。
    pub no_throw_item: bool,
    /// 禁止玩家掉落（`:79`）。
    pub no_drop_player: bool,
    /// 禁止怪物掉落（`:80`）。
    pub no_drop_monster: bool,
    /// 隐藏玩家名（`:81`）。
    pub no_names: bool,
    /// 允许 PK（`:82`）。
    pub fight: bool,
    /// 允许火焰伤害（`:83`）。
    pub fire: bool,
    /// 火焰伤害值（`:84`）。
    pub fire_damage: i32,
    /// 允许雷电伤害（`:85`）。
    pub lightning: bool,
    /// 雷电伤害值（`:86`）。
    pub lightning_damage: i32,
    /// 地图基础暗度（`byte`，`:87`）。
    pub map_dark_light: u8,
    /// 矿区（`:88-90`）。
    pub mine_zones: Vec<MineZone>,
    /// 矿区编号（`byte`，`:91`）。
    pub mine_index: u8,
    /// 禁止骑乘（`:92`）。
    pub no_mount: bool,
    /// 需要马鞍（`:93`）。
    pub need_bridle: bool,
    /// 禁止战斗（`:94`）。
    pub no_fight: bool,
    /// 背景音乐编号（`u16`，`:95`）。
    pub music: u16,
    /// 禁止城镇传送（`>= 78`，`:97-98`）。
    pub no_town_teleport: bool,
    /// 禁止转生（`>= 79`，`:99-100`）。
    pub no_reincarnation: bool,
    /// 天气粒子（`WeatherSetting`，`>= 110`，`:102-105`）。
    pub weather_particles: u16,
    /// 是否 GT 城（`>= 111`，`:107-111`）。
    pub gt: bool,
    /// GT 城编号（`>= 111`）。
    pub gt_index: u8,
    /// 禁止获得经验（`>= 114`，`:112-123`）。
    pub no_experience: bool,
    /// 禁止组队（`>= 114`）。
    pub no_group: bool,
    /// 禁止召唤宠物（`>= 114`）。
    pub no_pets: bool,
    /// 禁止召唤智宠（`>= 114`）。
    pub no_intelligent_creatures: bool,
    /// 禁止英雄（`>= 114`）。
    pub no_hero: bool,
    /// 进入所需队伍人数（`i32`，`>= 114`，`:119`）。
    pub required_group_size: i32,
    /// 必须组队才能进入（`>= 114`，`:120`）。
    pub required_group: bool,
    /// 限制火墙数量（`>= 114`，`:121`）。
    pub firewall_limit: bool,
    /// 火墙数量上限（`i32`，`>= 114`，`:122`）。
    pub firewall_count: i32,
}

impl MapInfo {
    /// 解码，见 `MapInfo.cs:47-124`。
    ///
    /// 版本门（`MapInfo.cs:97-123`）：`< 78` 直接返回（`no_town_teleport` 之后没有），
    /// `< 79` 返回（`no_reincarnation` 之后没有），`>= 110` 才有 `weather_particles`，
    /// `>= 111` 才有 `gt` / `gt_index`，`>= 114` 才有一组禁制字段。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        let index = r.i32()?;
        let file_name = r.string()?;
        let title = r.string()?;
        let mini_map = r.u16()?;
        let light = r.u8()?;
        let big_map = r.u16()?;

        let safe_zones = r.list(MIN_SAFE_ZONE_LEN, SafeZoneInfo::parse)?;
        let respawns = r.list(MIN_RESPAWN_LEN, RespawnInfo::parse)?;
        let movements = r.list(MIN_MOVEMENT_LEN, MovementInfo::parse)?;

        let mut info = Self {
            index,
            file_name,
            title,
            mini_map,
            light,
            big_map,
            safe_zones,
            respawns,
            movements,
            ..Self::default()
        };

        info.no_teleport = r.bool()?;
        info.no_reconnect = r.bool()?;
        info.no_reconnect_map = r.string()?;

        info.no_random = r.bool()?;
        info.no_escape = r.bool()?;
        info.no_recall = r.bool()?;
        info.no_drug = r.bool()?;
        info.no_position = r.bool()?;
        info.no_throw_item = r.bool()?;
        info.no_drop_player = r.bool()?;
        info.no_drop_monster = r.bool()?;
        info.no_names = r.bool()?;
        info.fight = r.bool()?;
        info.fire = r.bool()?;
        info.fire_damage = r.i32()?;
        info.lightning = r.bool()?;
        info.lightning_damage = r.i32()?;
        info.map_dark_light = r.u8()?;
        info.mine_zones = r.list(MIN_MINE_ZONE_LEN, MineZone::parse)?;
        info.mine_index = r.u8()?;
        info.no_mount = r.bool()?;
        info.need_bridle = r.bool()?;
        info.no_fight = r.bool()?;
        info.music = r.u16()?;

        if r.version() < 78 {
            return Ok(info);
        }
        info.no_town_teleport = r.bool()?;
        if r.version() < 79 {
            return Ok(info);
        }
        info.no_reincarnation = r.bool()?;

        if r.version() >= 110 {
            info.weather_particles = r.u16()?;
        }
        if r.version() >= 111 {
            info.gt = r.bool()?;
            info.gt_index = r.u8()?;
        }
        if r.version() >= 114 {
            info.no_experience = r.bool()?;
            info.no_group = r.bool()?;
            info.no_pets = r.bool()?;
            info.no_intelligent_creatures = r.bool()?;
            info.no_hero = r.bool()?;
            info.required_group_size = r.i32()?;
            info.required_group = r.bool()?;
            info.firewall_limit = r.bool()?;
            info.firewall_count = r.i32()?;
        }
        Ok(info)
    }

    /// 编码，见 `MapInfo.cs:126-194`。
    ///
    /// `Save` 无条件写最新布局；`version` 用来裁掉旧版不存在的字段，
    /// 使测试能复现 105 / 88 / 83 版样本并逐字节比对。
    pub fn encode(&self, w: &mut DbWriter, version: i32) {
        w.i32(self.index);
        w.string(&self.file_name);
        w.string(&self.title);
        w.u16(self.mini_map);
        w.u8(self.light);
        w.u16(self.big_map);

        w.list(&self.safe_zones, |w, zone| zone.encode(w));
        w.list(&self.respawns, |w, respawn| respawn.encode(w, version));
        w.list(&self.movements, |w, movement| {
            movement.encode(w, version);
        });

        w.bool(self.no_teleport);
        w.bool(self.no_reconnect);
        w.string(&self.no_reconnect_map);

        w.bool(self.no_random);
        w.bool(self.no_escape);
        w.bool(self.no_recall);
        w.bool(self.no_drug);
        w.bool(self.no_position);
        w.bool(self.no_throw_item);
        w.bool(self.no_drop_player);
        w.bool(self.no_drop_monster);
        w.bool(self.no_names);
        w.bool(self.fight);
        w.bool(self.fire);
        w.i32(self.fire_damage);
        w.bool(self.lightning);
        w.i32(self.lightning_damage);
        w.u8(self.map_dark_light);
        w.list(&self.mine_zones, |w, zone| zone.encode(w));
        w.u8(self.mine_index);
        w.bool(self.no_mount);
        w.bool(self.need_bridle);
        w.bool(self.no_fight);
        w.u16(self.music);

        if version < 78 {
            return;
        }
        w.bool(self.no_town_teleport);
        if version < 79 {
            return;
        }
        w.bool(self.no_reincarnation);

        if version >= 110 {
            w.u16(self.weather_particles);
        }
        if version >= 111 {
            w.bool(self.gt);
            w.u8(self.gt_index);
        }
        if version >= 114 {
            w.bool(self.no_experience);
            w.bool(self.no_group);
            w.bool(self.no_pets);
            w.bool(self.no_intelligent_creatures);
            w.bool(self.no_hero);
            w.i32(self.required_group_size);
            w.bool(self.required_group);
            w.bool(self.firewall_limit);
            w.i32(self.firewall_count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个每个字段都区别于默认值的地图（v114 及以上才会全写）。
    fn sample() -> MapInfo {
        MapInfo {
            index: 7,
            file_name: "D2083".to_owned(),
            title: "龙之谷".to_owned(),
            mini_map: 3,
            light: 2,
            big_map: 9,
            safe_zones: vec![SafeZoneInfo {
                location: Point::new(11, 22),
                size: 5,
                start_point: true,
            }],
            respawns: vec![RespawnInfo {
                monster_index: 44,
                location: Point::new(33, 44),
                count: 6,
                spread: 7,
                delay: 8,
                direction: 3,
                route_path: "route/1".to_owned(),
                random_delay: 9,
                respawn_index: 1234,
                save_respawn_time: true,
                respawn_ticks: 321,
            }],
            movements: vec![MovementInfo {
                map_index: 8,
                source: Point::new(1, 2),
                destination: Point::new(3, 4),
                need_hole: true,
                need_move: false,
                conquest_index: 5,
                show_on_big_map: true,
                icon: 66,
            }],
            no_teleport: true,
            no_reconnect: true,
            no_reconnect_map: "0".to_owned(),
            no_random: true,
            no_escape: true,
            no_recall: true,
            no_drug: true,
            no_position: true,
            no_throw_item: true,
            no_drop_player: true,
            no_drop_monster: true,
            no_names: true,
            fight: true,
            fire: true,
            fire_damage: 10,
            lightning: true,
            lightning_damage: 20,
            map_dark_light: 4,
            mine_zones: vec![MineZone {
                location: Point::new(5, 6),
                size: 7,
                mine: 2,
            }],
            mine_index: 1,
            no_mount: true,
            need_bridle: true,
            no_fight: true,
            music: 8,
            no_town_teleport: true,
            no_reincarnation: true,
            weather_particles: 2,
            gt: true,
            gt_index: 3,
            no_experience: true,
            no_group: true,
            no_pets: true,
            no_intelligent_creatures: true,
            no_hero: true,
            required_group_size: 4,
            required_group: true,
            firewall_limit: true,
            firewall_count: 12,
        }
    }

    /// 按版本门把 `sample()` 里旧版不存在的字段清零，得到期望的解码结果。
    fn expected_for(version: i32) -> MapInfo {
        let mut map = sample();
        if version <= 67 {
            for respawn in &mut map.respawns {
                respawn.random_delay = 0;
                respawn.respawn_index = 0;
                respawn.save_respawn_time = false;
                respawn.respawn_ticks = 0;
            }
        }
        if version < 69 {
            for movement in &mut map.movements {
                movement.conquest_index = 0;
                movement.show_on_big_map = false;
                movement.icon = 0;
            }
        } else if version < 95 {
            for movement in &mut map.movements {
                movement.show_on_big_map = false;
                movement.icon = 0;
            }
        }
        if version < 114 {
            map.no_experience = false;
            map.no_group = false;
            map.no_pets = false;
            map.no_intelligent_creatures = false;
            map.no_hero = false;
            map.required_group_size = 0;
            map.required_group = false;
            map.firewall_limit = false;
            map.firewall_count = 0;
        }
        if version < 111 {
            map.gt = false;
            map.gt_index = 0;
        }
        if version < 110 {
            map.weather_particles = 0;
        }
        if version < 79 {
            map.no_reincarnation = false;
        }
        if version < 78 {
            map.no_town_teleport = false;
        }
        map
    }

    fn roundtrip(version: i32) {
        let mut writer = DbWriter::new();
        sample().encode(&mut writer, version);
        let bytes = writer.into_inner();

        let mut reader = DbReader::new(&bytes, version);
        let decoded = MapInfo::parse(&mut reader).expect("解码应当成功");
        assert_eq!(reader.remaining(), 0, "应正好读完，版本 {version}");
        assert_eq!(decoded, expected_for(version), "版本 {version} 的解码结果");
    }

    #[test]
    fn roundtrip_all_versions() {
        for version in [60, 67, 68, 77, 78, 79, 84, 95, 105, 110, 111, 114, 117] {
            roundtrip(version);
        }
    }

    #[test]
    fn respawn_extras_only_after_67() {
        // `RespawnInfo.cs:41-47`：`Version > 67` 才读四个尾字段。
        let respawn = RespawnInfo {
            monster_index: 1,
            location: Point::new(2, 3),
            count: 4,
            spread: 5,
            delay: 6,
            direction: 7,
            route_path: String::new(),
            random_delay: 8,
            respawn_index: 9,
            save_respawn_time: true,
            respawn_ticks: 10,
        };

        let mut writer = DbWriter::new();
        respawn.encode(&mut writer, 67);
        let old = writer.into_inner();
        // 4+4+4+2+2+2+1+1 = 20 字节。
        assert_eq!(old.len(), MIN_RESPAWN_LEN);
        let mut reader = DbReader::new(&old, 67);
        let decoded = RespawnInfo::parse(&mut reader).expect("解码应当成功");
        assert_eq!(decoded.respawn_index, 0, "旧版没有 respawn_index");
        assert!(!decoded.save_respawn_time);
        assert_eq!(reader.remaining(), 0);

        let mut writer = DbWriter::new();
        respawn.encode(&mut writer, 68);
        let new = writer.into_inner();
        assert_eq!(new.len(), MIN_RESPAWN_LEN + 9);
        let mut reader = DbReader::new(&new, 68);
        assert_eq!(
            RespawnInfo::parse(&mut reader).expect("解码应当成功"),
            respawn
        );
    }

    #[test]
    fn movement_gates_69_and_95() {
        let movement = MovementInfo {
            map_index: 1,
            source: Point::new(2, 3),
            destination: Point::new(4, 5),
            need_hole: true,
            need_move: true,
            conquest_index: 6,
            show_on_big_map: true,
            icon: 7,
        };
        for (version, expected_len) in
            [(68, MIN_MOVEMENT_LEN), (69, MIN_MOVEMENT_LEN + 4), (95, 31)]
        {
            let mut writer = DbWriter::new();
            movement.encode(&mut writer, version);
            assert_eq!(writer.len(), expected_len, "版本 {version} 的字节数");
            let bytes = writer.into_inner();
            let mut reader = DbReader::new(&bytes, version);
            let decoded = MovementInfo::parse(&mut reader).expect("解码应当成功");
            assert_eq!(reader.remaining(), 0);
            assert_eq!(decoded.map_index, 1);
            assert_eq!(
                decoded.conquest_index,
                if version >= 69 { 6 } else { 0 },
                "版本 {version} 的征服编号"
            );
            assert_eq!(decoded.icon, if version >= 95 { 7 } else { 0 });
        }
    }

    #[test]
    fn truncated_map_is_an_error() {
        let mut writer = DbWriter::new();
        sample().encode(&mut writer, 105);
        let bytes = writer.into_inner();
        let mut reader = DbReader::new(&bytes[..MIN_MAP_RECORD_LEN], 105);
        assert!(MapInfo::parse(&mut reader).is_err());
    }
}
