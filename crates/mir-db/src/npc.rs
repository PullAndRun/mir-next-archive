//! NPC 段：`NPCInfo`（Rust 侧命名为 `NpcInfo` 以满足 clippy 的 `upper_case_acronyms`）。
//!
//! 读入顺序见 `Server/MirDatabase/NPCInfo.cs:44-103`（`NPCInfo(BinaryReader reader)`），
//! 写出顺序见 `NPCInfo.cs:104-141`（`Save`）。
//!
//! 版本门比较碎（`NPCInfo.cs:62-102`）：
//!
//! * `Image` 在 `Version < 72` 时是 `u8`（`:62-69`）；
//! * `>= 64` 才有「可见时段 / 等级限制 / 星期 / 职业限制 / 攻城 / FlagNeeded」这一整块（`:73-89`）；
//! * 攻城字段在 `>= 66` 是 `i32 Conquest`，在此之前是 `bool Sabuk`（`:84-87`）；
//! * `> 95` 有 `ShowOnBigMap` + `BigMapIcon`，`> 96` 有 `CanTeleportTo`，`>= 107` 有 `ConquestVisible`（`:91-102`）。

use mir_core::Point;

use crate::MirDbError;
use crate::reader::{DbReader, DbWriter};

/// `NpcInfo` 的最小字节数：不含 `>= 64` 的整块。
pub const MIN_NPC_LEN: usize = 29;

/// Crystal `NPCInfo`（`Server/MirDatabase/NPCInfo.cs:6`；读 `:44-103`，写 `:104-141`）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NpcInfo {
    /// 主键（`NPCInfo.cs:46`）。
    pub index: i32,
    /// 所在地图（`MapIndex`，`:47`）。
    pub map_index: i32,
    /// 可接取的收集类任务（`:49-51`）。
    pub collect_quest_indexes: Vec<i32>,
    /// 可交付的任务（`:53-55`）。
    pub finish_quest_indexes: Vec<i32>,
    /// 脚本文件名（`:57`）。
    pub file_name: String,
    /// NPC 名（`:58`）。
    pub name: String,
    /// 坐标（`:60`）。
    pub location: Point,
    /// 外观编号（`:62-69`；`Version < 72` 时是 `u8`）。
    pub image: u16,
    /// 出现概率（`u16`，`:71`）。
    pub rate: u16,
    /// 是否只在特定时段出现（`>= 64`，`:75`）。
    pub time_visible: bool,
    /// 出现时段：起始小时（`>= 64`，`:76`）。
    pub hour_start: u8,
    /// 出现时段：起始分钟（`>= 64`，`:77`）。
    pub minute_start: u8,
    /// 出现时段：结束小时（`>= 64`，`:78`）。
    pub hour_end: u8,
    /// 出现时段：结束分钟（`>= 64`，`:79`）。C# 字段初始值是 1（`:27`）。
    pub minute_end: u8,
    /// 可见所需最低等级（`i16`，`>= 64`，`:80`）。
    pub min_lev: i16,
    /// 可见所需最高等级（`i16`，`>= 64`，`:81`）。
    pub max_lev: i16,
    /// 可见星期（`>= 64`，`:82`）。
    pub day_of_week: String,
    /// 可见职业（`>= 64`，`:83`）。
    pub class_required: String,
    /// 所属攻城区域编号（`i32`，`>= 66`，`:84-85`）。
    pub conquest: i32,
    /// 旧版的「是否沙巴克 NPC」（`bool`，`< 66`，`:86-87`）。
    ///
    /// 与 [`Self::conquest`] 是**互斥**的两种编码：同一个字段位置，`>= 66` 读 `i32`，
    /// `< 66` 读 `bool`（C# 侧也是两个不同的字段，`NPCInfo.cs:32,34`）。
    pub sabuk: bool,
    /// 进入所需 Flag（`i32`，`>= 64`，`:88`）。
    pub flag_needed: i32,
    /// 是否显示在大地图（`> 95`，`:91-95`）。
    pub show_on_big_map: bool,
    /// 大地图图标（`i32`，`> 95`）。
    pub big_map_icon: i32,
    /// 是否可被传送（`> 96`，`:96-97`）。
    pub can_teleport_to: bool,
    /// 攻城区内是否可见（`>= 107`，`:99-102`）。C# 字段初始值是 `true`（`:38`）。
    pub conquest_visible: bool,
}

impl Default for NpcInfo {
    /// 手写而不用 `derive`：C# 的字段初始值不是全零（`NPCInfo.cs:19-38`）。
    ///
    /// 只有旧文件里**读不到**的字段才受影响（`MinuteEnd = 1`、`ConquestVisible = true`），
    /// 其余初值（`Rate = 100` 等）总会被实际字节覆盖。
    fn default() -> Self {
        Self {
            index: 0,
            map_index: 0,
            collect_quest_indexes: Vec::new(),
            finish_quest_indexes: Vec::new(),
            file_name: String::new(),
            name: String::new(),
            location: Point::default(),
            image: 0,
            rate: 100,
            time_visible: false,
            hour_start: 0,
            minute_start: 0,
            hour_end: 0,
            minute_end: 1,
            min_lev: 0,
            max_lev: 0,
            day_of_week: String::new(),
            class_required: String::new(),
            conquest: 0,
            sabuk: false,
            flag_needed: 0,
            show_on_big_map: false,
            big_map_icon: 0,
            can_teleport_to: false,
            conquest_visible: true,
        }
    }
}

impl NpcInfo {
    /// 解码，见 `NPCInfo.cs:44-103`。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        let index = r.i32()?;
        let map_index = r.i32()?;

        let mut info = Self {
            index,
            map_index,
            ..Self::default()
        };

        info.collect_quest_indexes = r.list(4, DbReader::i32)?;
        info.finish_quest_indexes = r.list(4, DbReader::i32)?;

        info.file_name = r.string()?;
        info.name = r.string()?;
        info.location = Point::new(r.i32()?, r.i32()?);

        // `if (Envir.LoadVersion >= 72) Image = reader.ReadUInt16(); else Image = reader.ReadByte();`
        info.image = if r.version() >= 72 {
            r.u16()?
        } else {
            u16::from(r.u8()?)
        };

        info.rate = r.u16()?;

        if r.version() >= 64 {
            info.time_visible = r.bool()?;
            info.hour_start = r.u8()?;
            info.minute_start = r.u8()?;
            info.hour_end = r.u8()?;
            info.minute_end = r.u8()?;
            info.min_lev = r.i16()?;
            info.max_lev = r.i16()?;
            info.day_of_week = r.string()?;
            info.class_required = r.string()?;
            if r.version() >= 66 {
                info.conquest = r.i32()?;
            } else {
                info.sabuk = r.bool()?;
            }
            info.flag_needed = r.i32()?;
        }

        if r.version() > 95 {
            info.show_on_big_map = r.bool()?;
            info.big_map_icon = r.i32()?;
        }
        if r.version() > 96 {
            info.can_teleport_to = r.bool()?;
        }
        if r.version() >= 107 {
            info.conquest_visible = r.bool()?;
        }
        Ok(info)
    }

    /// 编码，见 `NPCInfo.cs:104-141`（旧版本由 `version` 参数裁掉）。
    pub fn encode(&self, w: &mut DbWriter, version: i32) {
        w.i32(self.index);
        w.i32(self.map_index);

        w.list(&self.collect_quest_indexes, |w, &quest| w.i32(quest));
        w.list(&self.finish_quest_indexes, |w, &quest| w.i32(quest));

        w.string(&self.file_name);
        w.string(&self.name);

        w.i32(self.location.x);
        w.i32(self.location.y);
        if version >= 72 {
            w.u16(self.image);
        } else {
            w.u8(self.image as u8);
        }
        w.u16(self.rate);

        if version >= 64 {
            w.bool(self.time_visible);
            w.u8(self.hour_start);
            w.u8(self.minute_start);
            w.u8(self.hour_end);
            w.u8(self.minute_end);
            w.i16(self.min_lev);
            w.i16(self.max_lev);
            w.string(&self.day_of_week);
            w.string(&self.class_required);
            if version >= 66 {
                w.i32(self.conquest);
            } else {
                w.bool(self.sabuk);
            }
            w.i32(self.flag_needed);
        }

        if version > 95 {
            w.bool(self.show_on_big_map);
            w.i32(self.big_map_icon);
        }
        if version > 96 {
            w.bool(self.can_teleport_to);
        }
        if version >= 107 {
            w.bool(self.conquest_visible);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> NpcInfo {
        NpcInfo {
            index: 5,
            map_index: 6,
            collect_quest_indexes: vec![11, 12],
            finish_quest_indexes: vec![13],
            file_name: "BichonProvince/BorderVillage/Border_Transport-0".to_owned(),
            name: "Teleport_Gilbert".to_owned(),
            location: Point::new(287, 615),
            image: 15,
            rate: 100,
            time_visible: true,
            hour_start: 1,
            minute_start: 2,
            hour_end: 3,
            minute_end: 4,
            min_lev: 10,
            max_lev: 40,
            day_of_week: "1,2,3".to_owned(),
            class_required: "Warrior".to_owned(),
            conquest: 9,
            sabuk: true,
            flag_needed: 7,
            show_on_big_map: true,
            big_map_icon: 3,
            can_teleport_to: true,
            conquest_visible: false,
        }
    }

    fn expected_for(version: i32) -> NpcInfo {
        let mut npc = sample();
        if version < 107 {
            // C# 字段初始值 `ConquestVisible = true`（`NPCInfo.cs:38`）。
            npc.conquest_visible = true;
        }
        if version <= 96 {
            npc.can_teleport_to = false;
        }
        if version <= 95 {
            npc.show_on_big_map = false;
            npc.big_map_icon = 0;
        }
        if version < 66 {
            npc.conquest = 0;
        } else {
            npc.sabuk = false;
        }
        if version < 64 {
            npc.sabuk = false;
            npc.flag_needed = 0;
            npc.time_visible = false;
            npc.hour_start = 0;
            npc.minute_start = 0;
            npc.hour_end = 0;
            npc.minute_end = 1;
            npc.min_lev = 0;
            npc.max_lev = 0;
            npc.day_of_week = String::new();
            npc.class_required = String::new();
        }
        npc
    }

    #[test]
    fn roundtrip_all_versions() {
        for version in [
            60, 63, 64, 65, 66, 70, 71, 72, 84, 95, 96, 97, 106, 107, 117,
        ] {
            let mut writer = DbWriter::new();
            sample().encode(&mut writer, version);
            let bytes = writer.into_inner();

            let mut reader = DbReader::new(&bytes, version);
            let decoded = NpcInfo::parse(&mut reader).expect("解码应当成功");
            assert_eq!(reader.remaining(), 0, "应正好读完，版本 {version}");
            assert_eq!(decoded, expected_for(version), "版本 {version} 的解码结果");
        }
    }

    #[test]
    fn conquest_and_sabuk_share_one_slot() {
        // `NPCInfo.cs:84-87`：`>= 66` 读 `i32 Conquest`，`< 66` 读 `bool Sabuk`。
        let npc = NpcInfo {
            conquest: 0x0102_0304,
            sabuk: true,
            ..NpcInfo::default()
        };
        let mut writer = DbWriter::new();
        npc.encode(&mut writer, 66);
        let new = writer.into_inner();

        let mut writer = DbWriter::new();
        npc.encode(&mut writer, 65);
        let old = writer.into_inner();
        assert_eq!(old.len() + 3, new.len(), "i32 比 bool 多三个字节");

        // 同一个偏移处，`< 66` 文件的这个字节是 1（`Sabuk = true`）。
        assert_eq!(old[old.len() - 5], 1);
    }

    #[test]
    fn legacy_image_is_one_byte_before_72() {
        let npc = NpcInfo {
            image: 0x0102,
            ..NpcInfo::default()
        };
        let mut writer = DbWriter::new();
        npc.encode(&mut writer, 71);
        let old = writer.into_inner();
        let mut writer = DbWriter::new();
        npc.encode(&mut writer, 72);
        let new = writer.into_inner();
        assert_eq!(old.len() + 1, new.len());
    }

    #[test]
    fn truncated_npc_is_an_error() {
        let mut writer = DbWriter::new();
        sample().encode(&mut writer, 105);
        let bytes = writer.into_inner();
        let mut reader = DbReader::new(&bytes[..MIN_NPC_LEN], 105);
        assert!(NpcInfo::parse(&mut reader).is_err());
    }
}
