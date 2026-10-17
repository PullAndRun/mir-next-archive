//! 怪物段：`MonsterInfo`。
//!
//! 读入顺序见 `Server/MirDatabase/MonsterInfo.cs:53-148`（`MonsterInfo(BinaryReader reader)`），
//! 写出顺序见 `MonsterInfo.cs:196-226`（`Save`）。
//!
//! 这里有两个大的版本断层（`MonsterInfo.cs:62-119`）：
//!
//! * `Version < 62`：`Level` 是 `u8`，且十个基础属性各占 **`u8`**；
//! * `62 <= Version <= 84`：`Level` 是 `u16`，十个基础属性各占 **`u16`**，
//!   另有独立的 `Accuracy` / `Agility` 两个 `u8`；
//! * `Version > 84`：属性表改成「`i32` 项数 + (u8 键, i32 值)」的字典格式。
//!
//! 三种布局共用一个 [`MonsterInfo`]（C# 侧也是同一个 `Stats` 容器）。

use mir_core::enums::Stat;
use mir_protocol::data::Stats;

use crate::MirDbError;
use crate::reader::{DbReader, DbWriter};

/// `MonsterInfo` 的最小字节数：`i32 Index` + 空 `Name` + 定宽字段 + 空属性表，
/// 连 `< 18` 才缺席的 `AutoRev` / `Undead` 都不算。
///
/// 实际可读到的版本（`>= 60`）至少还要多两个布尔（= 29 字节），所以这里取的 27
/// 对任何真实文件都是安全下界。
pub const MIN_MONSTER_LEN: usize = 27;

/// Crystal `MonsterInfo`（`Server/MirDatabase/MonsterInfo.cs:6`；读 `:53-148`，写 `:196-226`）。
#[derive(Debug, Clone, PartialEq)]
pub struct MonsterInfo {
    /// 主键（`MonsterInfo.cs:55`）。
    pub index: i32,
    /// 怪物名（`:56`）。
    pub name: String,
    /// 外观编号（`u16`，`:58`）。
    ///
    /// C# 是 `Monster` 枚举，但其底层类型就是 `ushort`，而数据库允许出现枚举里没有的编号
    /// （`MonsterInfo.cs:58` 直接强转），所以这里保留原始值。
    pub image: u16,
    /// AI 类型（`u8`，`:59`）。
    pub ai: u8,
    /// 特效类型（`u8`，`:60`）。
    pub effect: u8,
    /// 等级（`:62-69`；`Version < 62` 时是 `u8`）。
    pub level: u16,
    /// 视野（`u8`，`:71`）。
    pub view_range: u8,
    /// 反隐（`u8`，`:72`）。
    pub cool_eye: u8,
    /// 属性表（`Stats`，`:74-119`）。旧版的十项固定属性也会被塞进这张表，
    /// 键与字典格式完全一致（`Stat::MinAC` … `Stat::MaxSC`、`Stat::HP`、`Stat::Accuracy`、`Stat::Agility`）。
    pub stats: Stats,
    /// 自带光照（`u8`，`:121`）。
    pub light: u8,
    /// 攻击间隔（`u16`，`:123`）。
    pub attack_speed: u16,
    /// 移动间隔（`u16`，`:124`）。
    pub move_speed: u16,
    /// 经验（`u32`，`:126`）。
    pub experience: u32,
    /// 是否可被推动（`:128`）。
    pub can_push: bool,
    /// 是否可被驯服（`:129`）。
    pub can_tame: bool,
    /// 死后自动复活（`>= 18`，`:132`）。
    pub auto_rev: bool,
    /// 是否为亡灵（`>= 18`，`:133`）。
    pub undead: bool,
    /// 掉落表文件名（`>= 89`，`:137`）。旧文件没有，读成空串；
    /// 掉落内容永远来自 `Envir/Drops/*.txt`（`MonsterInfo.cs:372-435`），不落盘。
    pub drop_path: String,
    /// 是否可被招募（`>= 115`，`:141`）。
    pub can_recall: bool,
    /// 是否为 Boss（`>= 116`，`:146`）。
    pub is_boss: bool,
}

impl Default for MonsterInfo {
    /// 手写而不用 `derive`：C# 的 `CanTame` / `CanPush` / `AutoRev` 初值都是 `true`
    /// （`MonsterInfo.cs:39`），`ViewRange` 是 7（`:27`），攻/移速是 2500 / 1800（`:32`）。
    /// 这些字段在能读到的版本里总会被实际字节覆盖，只有构造新对象时才看得出来。
    fn default() -> Self {
        Self {
            index: 0,
            name: String::new(),
            image: 0,
            ai: 0,
            effect: 0,
            level: 0,
            view_range: 7,
            cool_eye: 0,
            stats: Stats::default(),
            light: 0,
            attack_speed: 2_500,
            move_speed: 1_800,
            experience: 0,
            can_push: true,
            can_tame: true,
            auto_rev: true,
            undead: false,
            drop_path: String::new(),
            can_recall: false,
            is_boss: false,
        }
    }
}

impl MonsterInfo {
    /// 解码，见 `MonsterInfo.cs:53-148`。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        let index = r.i32()?;
        let name = r.string()?;
        let image = r.u16()?;
        let ai = r.u8()?;
        let effect = r.u8()?;

        // `if (Envir.LoadVersion < 62) Level = (ushort)reader.ReadByte(); else Level = reader.ReadUInt16();`
        let level = if r.version() < 62 {
            u16::from(r.u8()?)
        } else {
            r.u16()?
        };

        let view_range = r.u8()?;
        let cool_eye = r.u8()?;

        let stats = parse_stats(r)?;

        let mut info = Self {
            index,
            name,
            image,
            ai,
            effect,
            level,
            view_range,
            cool_eye,
            stats,
            light: r.u8()?,
            attack_speed: r.u16()?,
            move_speed: r.u16()?,
            experience: r.u32()?,
            can_push: r.bool()?,
            can_tame: r.bool()?,
            auto_rev: false,
            undead: false,
            drop_path: String::new(),
            can_recall: false,
            is_boss: false,
        };

        if r.version() < 18 {
            return Ok(info);
        }
        info.auto_rev = r.bool()?;
        info.undead = r.bool()?;

        if r.version() < 89 {
            return Ok(info);
        }
        info.drop_path = r.string()?;

        if r.version() >= 115 {
            info.can_recall = r.bool()?;
        }
        if r.version() >= 116 {
            info.is_boss = r.bool()?;
        }
        Ok(info)
    }

    /// 编码，见 `MonsterInfo.cs:196-226`。
    ///
    /// `Save` 无条件写最新布局；`version` 用于裁掉旧版不存在的部分
    /// （`Level` 的宽度、属性表格式、`DropPath` / `CanRecall` / `IsBoss`）。
    pub fn encode(&self, w: &mut DbWriter, version: i32) {
        w.i32(self.index);
        w.string(&self.name);
        w.u16(self.image);
        w.u8(self.ai);
        w.u8(self.effect);
        if version < 62 {
            w.u8(self.level as u8);
        } else {
            w.u16(self.level);
        }
        w.u8(self.view_range);
        w.u8(self.cool_eye);

        encode_stats(w, &self.stats, version);

        w.u8(self.light);
        w.u16(self.attack_speed);
        w.u16(self.move_speed);
        w.u32(self.experience);
        w.bool(self.can_push);
        w.bool(self.can_tame);

        if version < 18 {
            return;
        }
        w.bool(self.auto_rev);
        w.bool(self.undead);

        if version < 89 {
            return;
        }
        w.string(&self.drop_path);

        if version >= 115 {
            w.bool(self.can_recall);
        }
        if version >= 116 {
            w.bool(self.is_boss);
        }
    }

    /// 取某项属性（缺省 0），等价于 C# 的 `Stats[Stat.X]` 索引器。
    #[must_use]
    pub fn stat(&self, stat: Stat) -> i32 {
        self.stats.get(stat)
    }
}

/// 旧版十项固定属性的键顺序（`MonsterInfo.cs:87-96` / `:102-111`）。
const LEGACY_STATS: [Stat; 10] = [
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
];

/// 按版本读取属性表（`MonsterInfo.cs:74-119`）。
fn parse_stats(r: &mut DbReader<'_>) -> Result<Stats, MirDbError> {
    if r.version() > 84 {
        // `Stats = new Stats(reader)`（`MonsterInfo.cs:76`），见 `Shared/Data/Stat.cs:36-42`。
        return Stats::decode(r.packet()).map_err(|error| r.truncated(error));
    }

    let mut stats = Stats::default();
    // `Stats[Stat.HP] = (int)reader.ReadUInt32();`（`MonsterInfo.cs:82`）
    stats.set(Stat::HP, r.u32()? as i32);

    if r.version() < 62 {
        // 十项各 `u8`（`MonsterInfo.cs:87-96`）。
        for stat in LEGACY_STATS {
            stats.set(stat, i32::from(r.u8()?));
        }
    } else {
        // 十项各 `u16`（`MonsterInfo.cs:102-111`）。
        for stat in LEGACY_STATS {
            stats.set(stat, i32::from(r.u16()?));
        }
    }

    // `Stats[Stat.Accuracy]` / `Stats[Stat.Agility]` 两个 `u8`（`MonsterInfo.cs:117-118`）。
    stats.set(Stat::Accuracy, i32::from(r.u8()?));
    stats.set(Stat::Agility, i32::from(r.u8()?));
    Ok(stats)
}

/// 按版本写出属性表（`MonsterInfo.cs:208` 的 `Stats.Save` 与旧版固定块）。
fn encode_stats(w: &mut DbWriter, stats: &Stats, version: i32) {
    if version > 84 {
        w.stats(stats);
        return;
    }

    w.u32(stats.get(Stat::HP) as u32);
    if version < 62 {
        for stat in LEGACY_STATS {
            w.u8(stats.get(stat) as u8);
        }
    } else {
        for stat in LEGACY_STATS {
            w.u16(stats.get(stat) as u16);
        }
    }
    w.u8(stats.get(Stat::Accuracy) as u8);
    w.u8(stats.get(Stat::Agility) as u8);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> MonsterInfo {
        MonsterInfo {
            index: 12,
            name: "邪恶巨人".to_owned(),
            image: 139,
            ai: 57,
            effect: 2,
            level: 66,
            view_range: 7,
            cool_eye: 1,
            stats: Stats::from_iter([
                (Stat::HP, 9_999),
                (Stat::MinAC, 30),
                (Stat::MaxAC, 45),
                (Stat::Accuracy, 17),
                (Stat::Agility, 12),
            ]),
            light: 20,
            attack_speed: 2_000,
            move_speed: 1_800,
            experience: 1_500,
            can_push: false,
            can_tame: false,
            auto_rev: true,
            undead: false,
            drop_path: "Drops/EvilMir".to_owned(),
            can_recall: true,
            is_boss: true,
        }
    }

    #[test]
    fn roundtrip_modern_and_legacy() {
        for version in [17, 18, 61, 62, 84, 85, 89, 105, 115, 116, 117] {
            let mut writer = DbWriter::new();
            sample().encode(&mut writer, version);
            let bytes = writer.into_inner();

            let mut reader = DbReader::new(&bytes, version);
            let decoded = MonsterInfo::parse(&mut reader).expect("解码应当成功");
            assert_eq!(reader.remaining(), 0, "应正好读完，版本 {version}");

            // 旧版里不存在的字段读成默认值，其余保持一致。
            let mut expected = sample();
            if version < 18 {
                expected.auto_rev = false;
                expected.undead = false;
            }
            if version < 89 {
                expected.drop_path = String::new();
            }
            if version < 115 {
                expected.can_recall = false;
            }
            if version < 116 {
                expected.is_boss = false;
            }
            expected.stats = match version {
                // 旧版属性表只有 HP + 十项基础 + 命中/敏捷。
                0..=84 => Stats::from_iter([
                    (Stat::HP, 9_999),
                    (Stat::MinAC, 30),
                    (Stat::MaxAC, 45),
                    (Stat::Accuracy, 17),
                    (Stat::Agility, 12),
                ]),
                _ => sample().stats,
            };
            assert_eq!(decoded, expected, "版本 {version} 的解码结果");
        }
    }

    #[test]
    fn legacy_level_is_one_byte_before_62() {
        // 只比较「等级字段」附近：`MonsterInfo.cs:62-69`。
        // 槽位（空名字）：Index 0..4、Name 长度前缀 4、Name 无内容、Image 5..7、
        // AI 7、Effect 8，于是 Level 从偏移 9 开始。
        let monster = MonsterInfo {
            level: 0x0102,
            view_range: 4,
            ..MonsterInfo::default()
        };
        let mut writer = DbWriter::new();
        monster.encode(&mut writer, 61);
        let old = writer.into_inner();
        assert_eq!(old[9], 0x02, "v61 的 Level 只有一个字节");
        assert_eq!(old[10], 4, "v61 的 ViewRange 紧跟其后");

        let mut writer = DbWriter::new();
        monster.encode(&mut writer, 62);
        let new = writer.into_inner();
        assert_eq!(&new[9..11], &[0x02, 0x01], "v62 的 Level 是两个字节");
        assert_eq!(new[11], 4);
    }

    #[test]
    fn legacy_fixed_block_reads_signed_bytes() {
        // 手动拼一个 v60 的怪物记录：`Level` 是 `u8`，十项属性各 `u8`
        // （`MonsterInfo.cs:62-96`）。
        let mut w = DbWriter::new();
        w.i32(3); // Index
        w.string("鸡"); // Name
        w.u16(7); // Image
        w.u8(1); // AI
        w.u8(0); // Effect
        w.u8(9); // Level（u8）
        w.u8(4); // ViewRange
        w.u8(0); // CoolEye
        w.u32(15); // HP（u32）
        for value in [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10] {
            w.u8(value); // MinAC..MaxSC
        }
        w.u8(11); // Accuracy
        w.u8(12); // Agility
        w.u8(1); // Light
        w.u16(1_500); // AttackSpeed
        w.u16(1_800); // MoveSpeed
        w.u32(10); // Experience
        w.bool(true); // CanPush
        w.bool(true); // CanTame
        w.bool(false); // AutoRev
        w.bool(false); // Undead
        let bytes = w.into_inner();

        let mut reader = DbReader::new(&bytes, 60);
        let decoded = MonsterInfo::parse(&mut reader).expect("解码应当成功");
        assert_eq!(reader.remaining(), 0);
        assert_eq!(decoded.level, 9);
        assert_eq!(decoded.stats.get(Stat::HP), 15);
        assert_eq!(decoded.stats.get(Stat::MinAC), 1);
        assert_eq!(decoded.stats.get(Stat::MaxSC), 10);
        assert_eq!(decoded.stats.get(Stat::Accuracy), 11);
        assert_eq!(decoded.stats.get(Stat::Agility), 12);
        assert!(decoded.drop_path.is_empty(), "v60 没有 DropPath");
        assert!(!decoded.is_boss);
    }

    #[test]
    fn truncated_monster_is_an_error() {
        let mut writer = DbWriter::new();
        sample().encode(&mut writer, 105);
        let bytes = writer.into_inner();
        let mut reader = DbReader::new(&bytes[..MIN_MONSTER_LEN], 105);
        assert!(MonsterInfo::parse(&mut reader).is_err());
    }
}
