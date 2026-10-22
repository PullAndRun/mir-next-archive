//! 技能段：`MagicInfo`。
//!
//! 读入顺序见 `Server/MirDatabase/MagicInfo.cs:34-61`
//! （`MagicInfo(BinaryReader reader, int version, int Customversion)`），
//! 写出顺序见 `MagicInfo.cs:63-85`（`Save`）。
//!
//! 版本门：`Version > 66` 才有 `Range`（`:54-55`），`Version > 70` 才有
//! `MultiplierBase` / `MultiplierBonus`（`:56-60`）。两者的 C# 字段初始值分别是
//! `9` 与 `1.0f`（`MagicInfo.cs:21-22`），旧文件读不到时保留这些初值。
//!
//! 另有一条载入后修正：`LoadVersion <= 70` 时 `Envir.LoadDB` 会调用 `UpdateMagicInfo()`
//! 重算技能数值（`Envir.cs:2908-2909`）。那是规则层的行为，不属于字节格式，本 crate 不做。

use crate::MirDbError;
use crate::reader::{DbReader, DbWriter};

/// `MagicInfo` 的最小字节数（不含 `> 66` / `> 70` 的两个尾块）。
pub const MIN_MAGIC_LEN: usize = 30;

/// Crystal `MagicInfo`（`Server/MirDatabase/MagicInfo.cs:6`；读 `:34-61`，写 `:63-85`）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MagicInfo {
    /// 技能名（`MagicInfo.cs:36`）。
    pub name: String,
    /// 技能编号（`Spell`，`u8`，`:37`）。
    pub spell: u8,
    /// 基础消耗（`:38`）。
    pub base_cost: u8,
    /// 每级消耗增量（`:39`）。
    pub level_cost: u8,
    /// 图标（`:40`）。
    pub icon: u8,
    /// 1/2/3 级所需**角色等级**（`MagicInfo.cs:41-43`；Crystal `LevelMagic` 用
    /// `Level < magic.Info.Level1` 判断，`HumanObject.cs:6933`）。
    pub level1: u8,
    /// 见 [`Self::level1`]。
    pub level2: u8,
    /// 见 [`Self::level1`]。
    pub level3: u8,
    /// 1/2/3 级所需**修炼值**（`u16`，`:44-46`；`magic.Experience >= magic.Info.Need1`，
    /// `HumanObject.cs:6937`）。
    pub need1: u16,
    /// 见 [`Self::need1`]。
    pub need2: u16,
    /// 见 [`Self::need1`]。
    pub need3: u16,
    /// 基础冷却（毫秒，`u32`，`:47`）。C# 字段初始值 1800（`:18`）。
    pub delay_base: u32,
    /// 冷却缩减（`u32`，`:48`）。
    pub delay_reduction: u32,
    /// 基础威力（`u16`，`:49`）。
    pub power_base: u16,
    /// 威力成长（`u16`，`:50`）。
    pub power_bonus: u16,
    /// 道术/魔法基础威力（`u16`，`:51`）。
    pub m_power_base: u16,
    /// 道术/魔法威力成长（`u16`，`:52`）。
    pub m_power_bonus: u16,
    /// 施法距离（`u8`，`> 66`，`:54-55`）。C# 字段初始值 9（`:22`）。
    pub range: u8,
    /// 威力倍率基数（`f32`，`> 70`，`:56-60`）。C# 字段初始值 1.0（`:21`）。
    pub multiplier_base: f32,
    /// 威力倍率成长（`f32`，`> 70`）。
    pub multiplier_bonus: f32,
}

impl Default for MagicInfo {
    /// 手写而不用 `derive`：C# 字段初始值是 `DelayBase = 1800`、`Range = 9`、
    /// `MultiplierBase = 1.0f`（`MagicInfo.cs:18-22`），旧文件里后两项读不到时会保留。
    fn default() -> Self {
        Self {
            name: String::new(),
            spell: 0,
            base_cost: 0,
            level_cost: 0,
            icon: 0,
            level1: 0,
            level2: 0,
            level3: 0,
            need1: 0,
            need2: 0,
            need3: 0,
            delay_base: 1_800,
            delay_reduction: 0,
            power_base: 0,
            power_bonus: 0,
            m_power_base: 0,
            m_power_bonus: 0,
            range: 9,
            multiplier_base: 1.0,
            multiplier_bonus: 0.0,
        }
    }
}

impl MagicInfo {
    /// 解码，见 `MagicInfo.cs:34-61`。版本门：`> 66` 有 `range`，`> 70` 有两个 `f32`。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        let name = r.string()?;

        let mut info = Self {
            name,
            spell: r.u8()?,
            base_cost: r.u8()?,
            level_cost: r.u8()?,
            icon: r.u8()?,
            level1: r.u8()?,
            level2: r.u8()?,
            level3: r.u8()?,
            need1: r.u16()?,
            need2: r.u16()?,
            need3: r.u16()?,
            delay_base: r.u32()?,
            delay_reduction: r.u32()?,
            power_base: r.u16()?,
            power_bonus: r.u16()?,
            m_power_base: r.u16()?,
            m_power_bonus: r.u16()?,
            ..Self::default()
        };

        if r.version() > 66 {
            info.range = r.u8()?;
        }
        if r.version() > 70 {
            info.multiplier_base = r.f32()?;
            info.multiplier_bonus = r.f32()?;
        }
        Ok(info)
    }

    /// 编码，见 `MagicInfo.cs:63-85`（旧版本由 `version` 参数裁掉）。
    pub fn encode(&self, w: &mut DbWriter, version: i32) {
        w.string(&self.name);
        w.u8(self.spell);
        w.u8(self.base_cost);
        w.u8(self.level_cost);
        w.u8(self.icon);
        w.u8(self.level1);
        w.u8(self.level2);
        w.u8(self.level3);
        w.u16(self.need1);
        w.u16(self.need2);
        w.u16(self.need3);
        w.u32(self.delay_base);
        w.u32(self.delay_reduction);
        w.u16(self.power_base);
        w.u16(self.power_bonus);
        w.u16(self.m_power_base);
        w.u16(self.m_power_bonus);
        if version > 66 {
            w.u8(self.range);
        }
        if version > 70 {
            w.f32(self.multiplier_base);
            w.f32(self.multiplier_bonus);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> MagicInfo {
        MagicInfo {
            name: "基本剑术".to_owned(),
            spell: 1,
            base_cost: 0,
            level_cost: 2,
            icon: 2,
            level1: 7,
            level2: 9,
            level3: 12,
            need1: 27,
            need2: 60,
            need3: 130,
            delay_base: 1_800,
            delay_reduction: 100,
            power_base: 5,
            power_bonus: 6,
            m_power_base: 7,
            m_power_bonus: 8,
            range: 9,
            multiplier_base: 1.5,
            multiplier_bonus: 0.25,
        }
    }

    fn expected_for(version: i32) -> MagicInfo {
        let mut magic = sample();
        if version <= 70 {
            // C# 字段初始值（`MagicInfo.cs:21-22`）。
            magic.range = 9;
            magic.multiplier_base = 1.0;
            magic.multiplier_bonus = 0.0;
        }
        if version <= 66 {
            magic.range = 9;
        }
        magic
    }

    #[test]
    fn roundtrip_all_versions() {
        for version in [60, 66, 67, 70, 71, 84, 105, 117] {
            let mut writer = DbWriter::new();
            sample().encode(&mut writer, version);
            let bytes = writer.into_inner();

            let mut reader = DbReader::new(&bytes, version);
            let decoded = MagicInfo::parse(&mut reader).expect("解码应当成功");
            assert_eq!(reader.remaining(), 0, "应正好读完，版本 {version}");
            assert_eq!(decoded, expected_for(version), "版本 {version} 的解码结果");
        }
    }

    #[test]
    fn legacy_tail_costs_one_plus_eight_bytes() {
        let magic = sample();
        let mut writer = DbWriter::new();
        magic.encode(&mut writer, 66);
        let old = writer.into_inner();
        let mut writer = DbWriter::new();
        magic.encode(&mut writer, 67);
        let mid = writer.into_inner();
        let mut writer = DbWriter::new();
        magic.encode(&mut writer, 71);
        let new = writer.into_inner();

        // `MIN_MAGIC_LEN` 假设名字为空串（1 字节前缀）；样本名 `基本剑术` 是 12 字节。
        assert_eq!(old.len(), MIN_MAGIC_LEN + "基本剑术".len());
        assert_eq!(mid.len(), old.len() + 1, "Range 是一个字节");
        assert_eq!(new.len(), mid.len() + 8, "两个 f32");
    }

    #[test]
    fn truncated_magic_is_an_error() {
        let mut writer = DbWriter::new();
        sample().encode(&mut writer, 105);
        let bytes = writer.into_inner();
        let mut reader = DbReader::new(&bytes[..MIN_MAGIC_LEN], 105);
        assert!(MagicInfo::parse(&mut reader).is_err());
    }
}
