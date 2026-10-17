//! 基础属性表 `BaseStats` 的线格式。
//!
//! 对照 Crystal `Shared/BaseStats.cs` 手写：
//!
//! * `BaseStats` 定义在 `:1`，读入 ctor 在 `:102`，`Save` 在 `:122`；
//! * 元素类型 `BaseStat` 定义在 `:141`，它**没有**自己的读写函数，
//!   全部由 `BaseStats` 的 ctor / `Save` 内联读写。
//!
//! 这两个方法都**不接受** `version` 参数（不像 `ItemInfo` 那样带默认版本分支），
//! 所以不存在需要按 `int.MaxValue` 展开的条件分支，字段顺序即源码顺序。
//!
//! 线上入口是 `BaseStatsInfo` / `HeroBaseStatsInfo` 两个包
//! （`Shared/ServerPackets.cs:4396` 与 `:4416`），二者都是
//! `ReadPacket` 里 `new BaseStats(reader)`、`WritePacket` 里 `Stats.Save(writer)`。
//!
//! 线格式（一行一字段，全部小端）：
//!
//! ```text
//! u8    Job                      MirClass
//! i32   Stats.Count
//! 每条 18 字节：
//!   u8  Type                     Stat
//!   u8  FormulaType              StatFormula
//!   i32 Base
//!   f32 Gain
//!   f32 GainRate
//!   i32 Max
//! Stats Caps                     i32 项数 + (u8 stat, i32 value)*
//! ```
//!
//! `Caps` 直接复用已实现的 `Stats`（对应 `Shared/Data/Stat.cs` 的 `Stats` 类），
//! 包括「值为 0 等于删键」的索引器语义，因此它的线格式与 `UserItem.AddedStats`
//! 完全一致；`BaseStats` 因此只依赖原语与已实现类型。
//!
//! **不在线格式里**的内容（都属于服务器侧游戏数据，客户端不关心）：
//!
//! * 带参 ctor `BaseStats(MirClass job)`（`BaseStats.cs:8`）里各职业的初始成长表；
//! * `Caps` 的默认上限表（`BaseStats.cs:91`-`:99`）。
//!
//! 客户端收到的 `Caps` 是服务器逐项下发的结果，所以这里不需要、也不应该
//! 在协议层复刻那两张表。
//!
//! 同样不在本模块的是 `BaseStat.Calculate`（`BaseStats.cs:155`）——那是
//! 服务器算属性的公式，与线格式无关。

use mir_core::enums::{MirClass, Stat, StatFormula};

use crate::data::Stats;
use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `BaseStat`（`Shared/BaseStats.cs:141`）。
///
/// 一条属性成长定义。C# 里它只有字段和一个 `Calculate` 公式方法，
/// 没有独立的 `Save` / `BinaryReader` ctor；读写顺序由 `BaseStats` 决定，
/// 见 `BaseStats.cs:107`-`:116`（读）与 `:127`-`:135`（写）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BaseStat {
    /// C# 字段 `Type`：属性种类。
    pub stat: Stat,
    /// C# 字段 `FormulaType`：成长公式（生命 / 魔法 / 负重 / 普通属性）。
    pub formula_type: StatFormula,
    /// C# 字段 `Base`：基础值。
    pub base: i32,
    /// C# 字段 `Gain`：每级成长系数（`Calculate` 里做除法）。
    pub gain: f32,
    /// C# 字段 `GainRate`：附加成长率。
    pub gain_rate: f32,
    /// C# 字段 `Max`：上限；`0` 表示不设上限（`Calculate` 里换成 `int.MaxValue`）。
    pub max: i32,
}

/// Crystal `BaseStats`（`Shared/BaseStats.cs:1`；读入 ctor `:102`，`Save` `:122`）。
///
/// 一个职业的完整基础属性表 + 属性上限表，随 `BaseStatsInfo` 包下发。
///
/// 注意 C# 没有无参 ctor（只有 `BaseStats(MirClass)` 与 `BaseStats(BinaryReader)`），
/// 这里的 `Default` 只是 Rust 侧的便利值：`Job = Warrior`、两张表都为空，
/// 解码出来的实例不会用到它。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BaseStats {
    /// C# 字段 `Job`：职业（线格式里是 1 字节）。
    pub job: MirClass,
    /// C# 字段 `Stats`：基础属性成长表，顺序即线格式顺序（**不是**按枚举排序）。
    pub stats: Vec<BaseStat>,
    /// C# 字段 `Caps`：属性上限表。
    pub caps: Stats,
}

impl BaseStats {
    /// 解码，见 `Shared/BaseStats.cs:102`。
    ///
    /// 逐字段顺序：`Job`、`Stats.Count`、每条属性 6 个字段、`Caps`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let job = MirClass::from_value(r.read_u8()?).unwrap_or_default();

        let count = r.read_count()?;
        // 不预分配：长度来自不可信输入。
        let mut stats = Vec::new();
        for _ in 0..count {
            // `new BaseStat((Stat)reader.ReadByte()) { FormulaType = ..., ... }`：
            // 构造参数先求值，随后才是对象初始化器的成员，所以 Type 在最前。
            stats.push(BaseStat {
                stat: Stat::from_value(r.read_u8()?).unwrap_or_default(),
                formula_type: StatFormula::from_value(r.read_u8()?).unwrap_or_default(),
                base: r.read_i32()?,
                gain: r.read_f32()?,
                gain_rate: r.read_f32()?,
                max: r.read_i32()?,
            });
        }

        // `Caps = new Stats(reader);`
        let caps = Stats::decode(r)?;

        Ok(Self { job, stats, caps })
    }

    /// 编码，见 `Shared/BaseStats.cs:122`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_u8(self.job.value());

        w.write_count(self.stats.len());
        for stat in &self.stats {
            w.write_u8(stat.stat.value());
            w.write_u8(stat.formula_type.value());
            w.write_i32(stat.base);
            w.write_f32(stat.gain);
            w.write_f32(stat.gain_rate);
            w.write_i32(stat.max);
        }

        // `Caps.Save(writer);`
        self.caps.encode(w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 编码后再解码，断言相等且负载被正好读完。
    fn roundtrip(value: &BaseStats) {
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = BaseStats::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, *value, "解码结果应与原值相同");
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn single_stat_matches_hand_derived_bytes() {
        // 按 `Shared/BaseStats.cs:122` 的写出顺序推导（全部小端）：
        //   u8  Job            = MirClass.Taoist = 2      -> 02
        //   i32 Stats.Count    = 1                        -> 01 00 00 00
        //   u8  Type           = Stat.HP = 12             -> 0C
        //   u8  FormulaType    = StatFormula.Health = 0   -> 00
        //   i32 Base           = 14                       -> 0E 00 00 00
        //   f32 Gain           = 6.0  = 0x40C00000        -> 00 00 C0 40
        //   f32 GainRate       = 2.5  = 0x40200000        -> 00 00 20 40
        //   i32 Max            = 0                        -> 00 00 00 00
        //   i32 Caps.Count     = 1                        -> 01 00 00 00
        //   u8  stat           = Stat.MagicResist = 30    -> 1E
        //   i32 value          = 2                        -> 02 00 00 00
        // 合计 1 + 4 + 18 + 4 + 1 + 4 = 32 字节。
        let mut caps = Stats::default();
        caps.set(Stat::MagicResist, 2);

        let value = BaseStats {
            job: MirClass::Taoist,
            stats: vec![BaseStat {
                stat: Stat::HP,
                formula_type: StatFormula::Health,
                base: 14,
                gain: 6.0,
                gain_rate: 2.5,
                max: 0,
            }],
            caps,
        };

        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let expected: Vec<u8> = vec![
            0x02, // Job = Taoist
            0x01, 0x00, 0x00, 0x00, // Stats.Count
            0x0C, // Type = HP
            0x00, // FormulaType = Health
            0x0E, 0x00, 0x00, 0x00, // Base = 14
            0x00, 0x00, 0xC0, 0x40, // Gain = 6.0f
            0x00, 0x00, 0x20, 0x40, // GainRate = 2.5f
            0x00, 0x00, 0x00, 0x00, // Max = 0
            0x01, 0x00, 0x00, 0x00, // Caps.Count
            0x1E, // MagicResist
            0x02, 0x00, 0x00, 0x00, // 2
        ];

        assert_eq!(bytes.len(), 32);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn roundtrip_with_multiple_stats_and_caps() {
        let mut caps = Stats::default();
        // 故意乱序插入，输出应仍按 Stat 判别值升序（与 C# SortedDictionary 一致）。
        caps.set(Stat::PoisonResist, 6);
        caps.set(Stat::MagicResist, 2);
        caps.set(Stat::CriticalRate, 18);

        let value = BaseStats {
            job: MirClass::Assassin,
            stats: vec![
                BaseStat {
                    stat: Stat::HP,
                    formula_type: StatFormula::Health,
                    base: 14,
                    gain: 4.0,
                    gain_rate: 3.25,
                    max: 0,
                },
                BaseStat {
                    stat: Stat::BagWeight,
                    formula_type: StatFormula::Weight,
                    base: 50,
                    gain: 3.5,
                    gain_rate: 0.0,
                    max: 120,
                },
                BaseStat {
                    stat: Stat::Agility,
                    formula_type: StatFormula::Stat,
                    base: 20,
                    gain: 0.0,
                    gain_rate: 0.0,
                    max: 0,
                },
            ],
            caps,
        };

        roundtrip(&value);
    }

    #[test]
    fn empty_stats_still_writes_job_and_caps() {
        let value = BaseStats::default();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();
        assert_eq!(
            bytes,
            vec![
                0x00, // Job = Warrior
                0x00, 0x00, 0x00, 0x00, // Stats.Count = 0
                0x00, 0x00, 0x00, 0x00, // Caps.Count = 0
            ]
        );
        roundtrip(&value);
    }

    #[test]
    fn unknown_enum_values_fall_back_to_default() {
        // Job 读到越界值（MirClass 只有 0-4）时退化为默认职业，
        // 与 crate 里其它数据类的 `unwrap_or_default()` 约定一致。
        let bytes = vec![
            0x7F, // Job = 127（未知）
            0x00, 0x00, 0x00, 0x00, // Stats.Count = 0
            0x00, 0x00, 0x00, 0x00, // Caps.Count = 0
        ];
        let mut reader = PacketReader::new(&bytes);
        let decoded = BaseStats::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded.job, MirClass::Warrior);
    }

    #[test]
    fn truncated_payload_errors() {
        let value = BaseStats {
            stats: vec![BaseStat {
                stat: Stat::HP,
                ..BaseStat::default()
            }],
            ..BaseStats::default()
        };
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(BaseStats::decode(&mut reader).is_err());
    }
}
