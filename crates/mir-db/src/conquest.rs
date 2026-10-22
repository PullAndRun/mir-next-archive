//! 攻城段：`ConquestInfo` 及其子记录。
//!
//! 读入顺序见 `Server/MirDatabase/ConquestInfo.cs:53-141`
//! （`ConquestInfo(BinaryReader reader)`），写出顺序见 `ConquestInfo.cs:143-218`（`Save`）。
//!
//! 版本门：`> 73` 才有 `FullMap`（`:57-60`）；`> 72` 才有 `FlagIndex` 与旗子列表
//! （`:72-75`、`:107-114`）；`> 74` 才有控制点（`:132-140`）。
//!
//! 子记录的 `RepairCost` 有一个**字节数不变**的版本门：
//! `<= 84` 时 C# 写的是 `reader.ReadUInt32()` 再 `(int)` 强转（`ConquestInfo.cs:243-250` 等），
//! `> 84` 时直接 `ReadInt32()`。两种读法对同样的 4 字节产生同样的位模式，
//! 所以这里统一按 `i32` 读，不影响逐字节往返。

use mir_core::Point;

use crate::MirDbError;
use crate::reader::{DbReader, DbWriter};

/// `ConquestInfo` 的最小字节数（不含任何版本门字段与子列表）。
///
/// 逐项相加：`i32 Index` 4 + `Point Location` 8 + `u16 Size` 2 + 空 `Name` 1 +
/// `MapIndex..SiegeIndex` 6×4 + 五个空列表的计数 5×4 + `StartHour` 1 +
/// `WarLength` 4 + `Type`/`Game` 2 + 七个星期布尔 7 + `Point KingLocation` 8 +
/// `u16 KingSize` 2 = 83。
pub const MIN_CONQUEST_LEN: usize = 83;

/// 箭塔子记录的最小字节数（`ConquestInfo.cs:365-372`）。
pub const MIN_ARCHER_LEN: usize = 21;

/// 城门 / 城墙 / 器械子记录的最小字节数（`ConquestInfo.cs:236-251` 等）。
pub const MIN_STRUCTURE_LEN: usize = 21;

/// 旗子子记录的最小字节数（`ConquestInfo.cs:399-405`）。
pub const MIN_FLAG_LEN: usize = 14;

/// Crystal `ConquestArcherInfo`（`ConquestInfo.cs:355`；读 `:365-372`，写 `:374-382`）。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConquestArcherInfo {
    /// 主键（`:367`）。
    pub index: i32,
    /// 坐标（`:368`）。
    pub location: Point,
    /// 怪物模板编号（`:369`）。
    pub mob_index: i32,
    /// 名字（`:370`）。
    pub name: String,
    /// 修理费（`u32`，`:371`）。
    pub repair_cost: u32,
}

impl ConquestArcherInfo {
    /// 解码，见 `ConquestInfo.cs:365-372`。无版本分支（`RepairCost` 恒为 `u32`）。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        Ok(Self {
            index: r.i32()?,
            location: Point::new(r.i32()?, r.i32()?),
            mob_index: r.i32()?,
            name: r.string()?,
            repair_cost: r.u32()?,
        })
    }

    /// 编码，见 `ConquestInfo.cs:374-382`。
    pub fn encode(&self, w: &mut DbWriter) {
        w.i32(self.index);
        w.i32(self.location.x);
        w.i32(self.location.y);
        w.i32(self.mob_index);
        w.string(&self.name);
        w.u32(self.repair_cost);
    }
}

/// Crystal `ConquestGateInfo` / `ConquestWallInfo` / `ConquestSiegeInfo`
/// （`ConquestInfo.cs:312` / `:269` / `:226`）。
///
/// 三个类在磁盘上的字段完全一致（读 `:236-251` / `:279-294` / `:322-337`），
/// 只有 `RepairCost` 的 C# 类型不同（`int`，但 `<= 84` 时按 `u32` 读入再强转，
/// 字节数相同）。这里合并成一个类型，由 `ConquestInfo` 的三个 `Vec` 复用。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConquestStructure {
    /// 主键。
    pub index: i32,
    /// 坐标。
    pub location: Point,
    /// 怪物模板编号。
    pub mob_index: i32,
    /// 名字。
    pub name: String,
    /// 修理费。`<= 84` 的文件按 `u32` 写入、`> 84` 按 `i32`，位模式相同。
    pub repair_cost: i32,
}

impl ConquestStructure {
    /// 解码，见 `ConquestInfo.cs:236-251`（城门；城墙 `:279-294`、器械 `:322-337` 完全相同）。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        Ok(Self {
            index: r.i32()?,
            location: Point::new(r.i32()?, r.i32()?),
            mob_index: r.i32()?,
            name: r.string()?,
            repair_cost: r.i32()?,
        })
    }

    /// 编码，见 `ConquestInfo.cs:253-261`（城门；城墙 / 器械相同）。
    pub fn encode(&self, w: &mut DbWriter) {
        w.i32(self.index);
        w.i32(self.location.x);
        w.i32(self.location.y);
        w.i32(self.mob_index);
        w.string(&self.name);
        w.i32(self.repair_cost);
    }
}

/// Crystal `ConquestFlagInfo`（`ConquestInfo.cs:390`；读 `:399-405`，写 `:407-414`）。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConquestFlagInfo {
    /// 主键。
    pub index: i32,
    /// 坐标。
    pub location: Point,
    /// 名字。
    pub name: String,
    /// 旗子贴图文件名。
    pub file_name: String,
}

impl ConquestFlagInfo {
    /// 解码，见 `ConquestInfo.cs:399-405`。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        Ok(Self {
            index: r.i32()?,
            location: Point::new(r.i32()?, r.i32()?),
            name: r.string()?,
            file_name: r.string()?,
        })
    }

    /// 编码，见 `ConquestInfo.cs:407-414`。
    pub fn encode(&self, w: &mut DbWriter) {
        w.i32(self.index);
        w.i32(self.location.x);
        w.i32(self.location.y);
        w.string(&self.name);
        w.string(&self.file_name);
    }
}

/// Crystal `ConquestInfo`（`Server/MirDatabase/ConquestInfo.cs:6`；读 `:53-141`，写 `:143-218`）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConquestInfo {
    /// 主键（`ConquestInfo.cs:55`）。
    pub index: i32,
    /// 是否整张地图都是攻城区（`> 73`，`:57-60`）。
    pub full_map: bool,
    /// 中心坐标（`:62`）。
    pub location: Point,
    /// 半径（`u16`，`:63`）。
    pub size: u16,
    /// 攻城区名（`:64`）。
    pub name: String,
    /// 所在地图（`:65`）。
    pub map_index: i32,
    /// 皇宫所在地图（`:66`）。
    pub palace_index: i32,
    /// 守卫（箭塔）编号（`:67`）。
    pub guard_index: i32,
    /// 城门编号（`:68`）。
    pub gate_index: i32,
    /// 城墙编号（`:69`）。
    pub wall_index: i32,
    /// 器械编号（`:70`）。
    pub siege_index: i32,
    /// 旗子编号（`> 72`，`:72-75`）。
    pub flag_index: i32,
    /// 箭塔列表（`:77-81`）。
    pub guards: Vec<ConquestArcherInfo>,
    /// 附加地图（`:83-87`）。
    pub extra_maps: Vec<i32>,
    /// 城门列表（`:89-93`）。
    pub gates: Vec<ConquestStructure>,
    /// 城墙列表（`:95-99`）。
    pub walls: Vec<ConquestStructure>,
    /// 器械列表（`:101-105`）。
    pub sieges: Vec<ConquestStructure>,
    /// 旗子列表（`> 72`，`:107-114`）。
    pub flags: Vec<ConquestFlagInfo>,
    /// 开战小时（`u8`，`:116`）。C# 字段初始值 0（`:29`）。
    pub start_hour: u8,
    /// 持续分钟数（`i32`，`:117`）。C# 字段初始值 60（`:30`）。
    pub war_length: i32,
    /// 攻城类型（`ConquestType`，`u8`，`:118`）。
    pub conquest_type: u8,
    /// 攻城玩法（`ConquestGame`，`u8`，`:119`）。
    pub game: u8,
    /// 周一开战（`:121`）……周日（`:127`）。
    pub monday: bool,
    /// 见 [`Self::monday`]。
    pub tuesday: bool,
    /// 见 [`Self::monday`]。
    pub wednesday: bool,
    /// 见 [`Self::monday`]。
    pub thursday: bool,
    /// 见 [`Self::monday`]。
    pub friday: bool,
    /// 见 [`Self::monday`]。
    pub saturday: bool,
    /// 见 [`Self::monday`]。
    pub sunday: bool,
    /// 王座坐标（`:129`）。
    pub king_location: Point,
    /// 王座半径（`u16`，`:130`）。
    pub king_size: u16,
    /// 控制点编号（`> 74`，`:132-134`）。
    pub control_point_index: i32,
    /// 控制点列表（`> 74`，`:135-139`）。
    pub control_points: Vec<ConquestFlagInfo>,
}

impl Default for ConquestInfo {
    /// 手写而不用 `derive`：C# 字段初始值是 `WarLength = 60`、`Type = Request`、
    /// `Game = CapturePalace`（`ConquestInfo.cs:29-33`）。三者都会被实际字节覆盖，
    /// 保留初值只是为了与新建对象的语义一致。
    fn default() -> Self {
        Self {
            index: 0,
            full_map: false,
            location: Point::default(),
            size: 0,
            name: String::new(),
            map_index: 0,
            palace_index: 0,
            guard_index: 0,
            gate_index: 0,
            wall_index: 0,
            siege_index: 0,
            flag_index: 0,
            guards: Vec::new(),
            extra_maps: Vec::new(),
            gates: Vec::new(),
            walls: Vec::new(),
            sieges: Vec::new(),
            flags: Vec::new(),
            start_hour: 0,
            war_length: 60,
            conquest_type: 0,
            game: 0,
            monday: false,
            tuesday: false,
            wednesday: false,
            thursday: false,
            friday: false,
            saturday: false,
            sunday: false,
            king_location: Point::default(),
            king_size: 0,
            control_point_index: 0,
            control_points: Vec::new(),
        }
    }
}

impl ConquestInfo {
    /// 解码，见 `ConquestInfo.cs:53-141`。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        let index = r.i32()?;
        let mut info = Self {
            index,
            ..Self::default()
        };

        if r.version() > 73 {
            info.full_map = r.bool()?;
        }
        info.location = Point::new(r.i32()?, r.i32()?);
        info.size = r.u16()?;
        info.name = r.string()?;
        info.map_index = r.i32()?;
        info.palace_index = r.i32()?;
        info.guard_index = r.i32()?;
        info.gate_index = r.i32()?;
        info.wall_index = r.i32()?;
        info.siege_index = r.i32()?;
        if r.version() > 72 {
            info.flag_index = r.i32()?;
        }

        info.guards = r.list(MIN_ARCHER_LEN, ConquestArcherInfo::parse)?;
        info.extra_maps = r.list(4, DbReader::i32)?;
        info.gates = r.list(MIN_STRUCTURE_LEN, ConquestStructure::parse)?;
        info.walls = r.list(MIN_STRUCTURE_LEN, ConquestStructure::parse)?;
        info.sieges = r.list(MIN_STRUCTURE_LEN, ConquestStructure::parse)?;
        if r.version() > 72 {
            info.flags = r.list(MIN_FLAG_LEN, ConquestFlagInfo::parse)?;
        }

        info.start_hour = r.u8()?;
        info.war_length = r.i32()?;
        info.conquest_type = r.u8()?;
        info.game = r.u8()?;

        info.monday = r.bool()?;
        info.tuesday = r.bool()?;
        info.wednesday = r.bool()?;
        info.thursday = r.bool()?;
        info.friday = r.bool()?;
        info.saturday = r.bool()?;
        info.sunday = r.bool()?;

        info.king_location = Point::new(r.i32()?, r.i32()?);
        info.king_size = r.u16()?;

        if r.version() > 74 {
            info.control_point_index = r.i32()?;
            info.control_points = r.list(MIN_FLAG_LEN, ConquestFlagInfo::parse)?;
        }
        Ok(info)
    }

    /// 编码，见 `ConquestInfo.cs:143-218`（旧版本由 `version` 参数裁掉）。
    pub fn encode(&self, w: &mut DbWriter, version: i32) {
        w.i32(self.index);
        if version > 73 {
            w.bool(self.full_map);
        }
        w.i32(self.location.x);
        w.i32(self.location.y);
        w.u16(self.size);
        w.string(&self.name);
        w.i32(self.map_index);
        w.i32(self.palace_index);
        w.i32(self.guard_index);
        w.i32(self.gate_index);
        w.i32(self.wall_index);
        w.i32(self.siege_index);
        if version > 72 {
            w.i32(self.flag_index);
        }

        w.list(&self.guards, |w, guard| guard.encode(w));
        w.list(&self.extra_maps, |w, &map| w.i32(map));
        w.list(&self.gates, |w, gate| gate.encode(w));
        w.list(&self.walls, |w, wall| wall.encode(w));
        w.list(&self.sieges, |w, siege| siege.encode(w));
        if version > 72 {
            w.list(&self.flags, |w, flag| flag.encode(w));
        }

        w.u8(self.start_hour);
        w.i32(self.war_length);
        w.u8(self.conquest_type);
        w.u8(self.game);

        w.bool(self.monday);
        w.bool(self.tuesday);
        w.bool(self.wednesday);
        w.bool(self.thursday);
        w.bool(self.friday);
        w.bool(self.saturday);
        w.bool(self.sunday);

        w.i32(self.king_location.x);
        w.i32(self.king_location.y);
        w.u16(self.king_size);

        if version > 74 {
            w.i32(self.control_point_index);
            w.list(&self.control_points, |w, flag| flag.encode(w));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ConquestInfo {
        ConquestInfo {
            index: 1,
            full_map: true,
            location: Point::new(330, 330),
            size: 40,
            name: "沙巴克".to_owned(),
            map_index: 3,
            palace_index: 4,
            guard_index: 10,
            gate_index: 11,
            wall_index: 12,
            siege_index: 13,
            flag_index: 14,
            guards: vec![ConquestArcherInfo {
                index: 20,
                location: Point::new(1, 2),
                mob_index: 30,
                name: "弓箭手".to_owned(),
                repair_cost: 5_000,
            }],
            extra_maps: vec![7, 8],
            gates: vec![ConquestStructure {
                index: 21,
                location: Point::new(3, 4),
                mob_index: 31,
                name: "城门".to_owned(),
                repair_cost: 4_000,
            }],
            walls: vec![ConquestStructure {
                index: 22,
                location: Point::new(5, 6),
                mob_index: 32,
                name: "城墙".to_owned(),
                repair_cost: 3_000,
            }],
            sieges: vec![ConquestStructure {
                index: 23,
                location: Point::new(7, 8),
                mob_index: 33,
                name: "器械".to_owned(),
                repair_cost: 2_000,
            }],
            flags: vec![ConquestFlagInfo {
                index: 24,
                location: Point::new(9, 10),
                name: "旗子".to_owned(),
                file_name: "flag".to_owned(),
            }],
            start_hour: 20,
            war_length: 90,
            conquest_type: 1,
            game: 2,
            monday: true,
            tuesday: false,
            wednesday: true,
            thursday: false,
            friday: true,
            saturday: false,
            sunday: true,
            king_location: Point::new(11, 12),
            king_size: 6,
            control_point_index: 25,
            control_points: vec![ConquestFlagInfo {
                index: 26,
                location: Point::new(13, 14),
                name: "控制点".to_owned(),
                file_name: "cp".to_owned(),
            }],
        }
    }

    fn expected_for(version: i32) -> ConquestInfo {
        let mut conquest = sample();
        if version <= 74 {
            conquest.control_point_index = 0;
            conquest.control_points.clear();
        }
        if version <= 73 {
            conquest.full_map = false;
        }
        if version <= 72 {
            conquest.flag_index = 0;
            conquest.flags.clear();
        }
        conquest
    }

    #[test]
    fn roundtrip_all_versions() {
        for version in [60, 66, 67, 72, 73, 74, 75, 84, 85, 105, 117] {
            let mut writer = DbWriter::new();
            sample().encode(&mut writer, version);
            let bytes = writer.into_inner();

            let mut reader = DbReader::new(&bytes, version);
            let decoded = ConquestInfo::parse(&mut reader).expect("解码应当成功");
            assert_eq!(reader.remaining(), 0, "应正好读完，版本 {version}");
            assert_eq!(decoded, expected_for(version), "版本 {version} 的解码结果");
        }
    }

    #[test]
    fn legacy_repair_cost_has_the_same_width() {
        // `ConquestInfo.cs:243-250`：`<= 84` 按 `u32` 读再强转，`> 84` 按 `i32` 读，
        // 字节数相同，位模式也相同。用 -1 验证这条等价性。
        let structure = ConquestStructure {
            repair_cost: -1,
            ..ConquestStructure::default()
        };
        let mut writer = DbWriter::new();
        structure.encode(&mut writer);
        let bytes = writer.into_inner();
        assert_eq!(
            &bytes[bytes.len() - 4..],
            &[0xFF, 0xFF, 0xFF, 0xFF],
            "u32::MAX 与 -1 的位模式一致"
        );
        for version in [84, 85] {
            let mut reader = DbReader::new(&bytes, version);
            assert_eq!(
                ConquestStructure::parse(&mut reader)
                    .expect("解码应当成功")
                    .repair_cost,
                -1
            );
        }
    }

    #[test]
    fn truncated_conquest_is_an_error() {
        let mut writer = DbWriter::new();
        sample().encode(&mut writer, 105);
        let bytes = writer.into_inner();
        let mut reader = DbReader::new(&bytes[..MIN_CONQUEST_LEN], 105);
        assert!(ConquestInfo::parse(&mut reader).is_err());
    }
}
