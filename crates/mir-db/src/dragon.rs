//! 龙系统段：`DragonInfo`（整份数据库里只有一条，**没有** `count` 前缀）。
//!
//! 读入顺序见 `Server/MirDatabase/DragonInfo.cs:49-69`（`DragonInfo(BinaryReader reader)`），
//! 写出顺序见 `DragonInfo.cs:70-86`（`Save`）。
//!
//! `Exps` 的长度是 `Globals.MaxDragonLevel - 1`（`DragonInfo.cs:22`），
//! 而 `Shared/Globals.cs:27` 里 `MaxDragonLevel = 13`，所以是固定的 12 个 `i64`。
//! 掉落表不落盘（`DragonInfo.cs:88-143` 从 `Envir/Drops/DragonItem.txt` 读）。

use mir_core::Point;

use crate::MirDbError;
use crate::reader::{DbReader, DbWriter};

/// 龙等级上限（`Shared/Globals.cs:27`：`MaxDragonLevel = 13`）。
pub const MAX_DRAGON_LEVEL: usize = 13;

/// `Exps` 的项数：`MaxDragonLevel - 1`（`DragonInfo.cs:22`）。
pub const DRAGON_EXP_LEVELS: usize = MAX_DRAGON_LEVEL - 1;

/// `DragonInfo` 的最小字节数：`bool` + 3 个空串 + 6×`i32` + 12×`i64`。
pub const MIN_DRAGON_LEN: usize = 1 + 3 + 6 * 4 + DRAGON_EXP_LEVELS * 8;

/// Crystal `DragonInfo`（`Server/MirDatabase/DragonInfo.cs:6`；读 `:49-69`，写 `:70-86`）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DragonInfo {
    /// 是否启用龙系统（`DragonInfo.cs:51`）。
    pub enabled: bool,
    /// 龙所在地图（`:52`）。
    pub map_file_name: String,
    /// 龙对应的怪物名（`:53`）。
    pub monster_name: String,
    /// 身体对象名（`:54`）。
    pub body_name: String,
    /// 出生坐标（`:55`）。
    pub location: Point,
    /// 掉落区域左上角（`:56`）。
    pub drop_area_top: Point,
    /// 掉落区域右下角（`:57`）。
    pub drop_area_bottom: Point,
    /// 每一级的升级经验（`12 × i64`，`:61-64`）。
    pub exps: [i64; DRAGON_EXP_LEVELS],
}

impl Default for DragonInfo {
    /// 手写而不用 `derive`：C# 的字段初始值不是全零（`DragonInfo.cs:29-48`），
    /// 且 `Exps[i] = (i + 1) * 10000` 是等差初值。旧文件（`< 60` 不可能存在）
    /// 读不到这些字段时就会保留它们。
    fn default() -> Self {
        let mut exps = [0i64; DRAGON_EXP_LEVELS];
        for (i, exp) in exps.iter_mut().enumerate() {
            *exp = (i as i64 + 1) * 10_000;
        }
        Self {
            enabled: false,
            map_file_name: "D2083".to_owned(),
            monster_name: "Evil Mir".to_owned(),
            body_name: "00".to_owned(),
            location: Point::new(82, 44),
            drop_area_top: Point::new(75, 45),
            drop_area_bottom: Point::new(86, 57),
            exps,
        }
    }
}

impl DragonInfo {
    /// 解码，见 `DragonInfo.cs:49-69`。无版本门。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        let enabled = r.bool()?;
        let map_file_name = r.string()?;
        let monster_name = r.string()?;
        let body_name = r.string()?;
        let location = Point::new(r.i32()?, r.i32()?);
        let drop_area_top = Point::new(r.i32()?, r.i32()?);
        let drop_area_bottom = Point::new(r.i32()?, r.i32()?);

        let mut exps = [0i64; DRAGON_EXP_LEVELS];
        for exp in &mut exps {
            *exp = r.i64()?;
        }

        Ok(Self {
            enabled,
            map_file_name,
            monster_name,
            body_name,
            location,
            drop_area_top,
            drop_area_bottom,
            exps,
        })
    }

    /// 编码，见 `DragonInfo.cs:70-86`。无版本门。
    pub fn encode(&self, w: &mut DbWriter) {
        w.bool(self.enabled);
        w.string(&self.map_file_name);
        w.string(&self.monster_name);
        w.string(&self.body_name);
        w.i32(self.location.x);
        w.i32(self.location.y);
        w.i32(self.drop_area_top.x);
        w.i32(self.drop_area_top.y);
        w.i32(self.drop_area_bottom.x);
        w.i32(self.drop_area_bottom.y);
        for exp in self.exps {
            w.i64(exp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DragonInfo {
        DragonInfo {
            enabled: true,
            map_file_name: "D2083".to_owned(),
            monster_name: "Evil Mir".to_owned(),
            body_name: "00".to_owned(),
            location: Point::new(82, 44),
            drop_area_top: Point::new(75, 45),
            drop_area_bottom: Point::new(86, 57),
            exps: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
        }
    }

    #[test]
    fn roundtrip() {
        let value = sample();
        let mut writer = DbWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();
        // `"D2083"`(6) + `"Evil Mir"`(9) + `"00"`(3) = 18 字节的字符串，
        // 于是 124 - 3 + 18 = 139，与 `docs/store/mirdb-format.md` §8.2 记录的
        // 「`DragonInfo` 段 139 字节」一致。
        assert_eq!(bytes.len(), MIN_DRAGON_LEN - 3 + 18);

        let mut reader = DbReader::new(&bytes, 117);
        assert_eq!(DragonInfo::parse(&mut reader).expect("解码应当成功"), value);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn default_matches_crystal_initializers() {
        // `DragonInfo.cs:29-48`：默认关闭，默认经验是 10000 的等差数列。
        let info = DragonInfo::default();
        assert!(!info.enabled);
        assert_eq!(info.map_file_name, "D2083");
        assert_eq!(info.exps[0], 10_000);
        assert_eq!(info.exps[DRAGON_EXP_LEVELS - 1], 120_000);
        assert_eq!(info.exps.len(), 12);
    }

    #[test]
    fn truncated_dragon_is_an_error() {
        let mut writer = DbWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = DbReader::new(&bytes[..MIN_DRAGON_LEN - 1], 117);
        assert!(DragonInfo::parse(&mut reader).is_err());
    }
}
