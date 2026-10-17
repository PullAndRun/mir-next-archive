//! 公会增益（buff）的线格式。
//!
//! 对应 Crystal `Shared/Data/GuildData.cs` 里的 `GuildBuffInfo`（`:92`）与
//! `GuildBuff`（`:217`）。二者都出现在 `ServerPackets.GuildBuffList`
//! （`Shared/ServerPackets.cs:6188-6216`）：先是「已激活」的 `GuildBuff` 列表，
//! 再是「全部可选」的 `GuildBuffInfo` 列表。

use crate::data::Stats;
use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `GuildBuffInfo`（`Shared/Data/GuildData.cs:92`；读入 ctor `:109`，`Save` `:182`）。
///
/// 线格式（一行一字段）：
///
/// ```text
/// i32    Id
/// i32    Icon
/// string Name               <- 7 位长度前缀 + UTF-8
/// u8     LevelRequirement
/// u8     PointsRequirement
/// i32    TimeLimit
/// i32    ActivationCost
/// Stats  Stats              <- i32 个数 + (u8 stat, i32 value)*
/// ```
///
/// `Stats` 的写出顺序由 `SortedDictionary` 的键序决定（按 `Stat` 判别值升序），
/// 与 INI 文件里逐项赋值的顺序（`:133-150`）无关；详见 `data::Stats`。
/// INI 的读/写（`:122`、`:153`）不是线格式，未移植。
#[derive(Debug, Clone, PartialEq)]
pub struct GuildBuffInfo {
    /// 增益编号（`GuildBuff.Id` 指向它）。
    pub id: i32,
    /// 图标编号。
    pub icon: i32,
    /// 增益名。
    pub name: String,
    /// 解锁所需公会等级。
    pub level_requirement: u8,
    /// 解锁所需公会点数。
    pub points_requirement: u8,
    /// 生效时长限制（秒）。
    pub time_limit: i32,
    /// 激活消耗。
    pub activation_cost: i32,
    /// 增益提供的属性。
    pub stats: Stats,
}

impl Default for GuildBuffInfo {
    /// 手写而不用 `derive`：Crystal 的字段初始值是 `PointsRequirement = 1`
    /// （`Shared/Data/GuildData.cs:98`），派生默认值是 0。
    /// 后台新建增益时只赋值 `Id` 与 `Name`（`Server.MirForms/Systems/GuildInfoForm.cs:409-413`），
    /// 其余字段全靠初始值，所以默认成 0 会写出与 Crystal 不同的字节。
    fn default() -> Self {
        Self {
            id: 0,
            icon: 0,
            name: String::new(),
            level_requirement: 0,
            points_requirement: 1,
            time_limit: 0,
            activation_cost: 0,
            stats: Stats::default(),
        }
    }
}

impl GuildBuffInfo {
    /// 解码，见 `Shared/Data/GuildData.cs:109`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        Ok(Self {
            id: r.read_i32()?,
            icon: r.read_i32()?,
            name: r.read_string()?,
            level_requirement: r.read_u8()?,
            points_requirement: r.read_u8()?,
            time_limit: r.read_i32()?,
            activation_cost: r.read_i32()?,
            stats: Stats::decode(r)?,
        })
    }

    /// 编码，见 `Shared/Data/GuildData.cs:182`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i32(self.id);
        w.write_i32(self.icon);
        w.write_string(&self.name);
        w.write_u8(self.level_requirement);
        w.write_u8(self.points_requirement);
        w.write_i32(self.time_limit);
        w.write_i32(self.activation_cost);
        self.stats.encode(w);
    }
}

/// Crystal `GuildBuff`（`Shared/Data/GuildData.cs:217`；读入 ctor `:231`，`Save` `:237`）。
///
/// 线格式只有 3 个字段（`bool` 占 1 字节）：
///
/// ```text
/// i32  Id
/// bool Active
/// i32  ActiveTimeRemaining
/// ```
///
/// **`Info` 不在线上传输**：它是运行时按 `Id` 从 `Settings.Guild_BuffList` 查出来的
/// （`Server/MirDatabase/GuildInfo.cs:150`、`Client/MirScenes/Dialogs/GuildDialog.cs:1121`），
/// 读入后恒为 `null`。按本 crate 的约定（见 `data::user_item` 模块文档：
/// 「不在线上传输的成员不属于本模块」）它不进入结构体。
/// 同理，C# 的 `UsingGuildSkillIcon` 属性（`:224-227`）只是 `Info` 的派生计算，也不移植。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildBuff {
    /// 增益编号。
    pub id: i32,
    /// 是否已激活。
    pub active: bool,
    /// 剩余生效时间（秒）。
    pub active_time_remaining: i32,
}

impl GuildBuff {
    /// 解码，见 `Shared/Data/GuildData.cs:231`。
    ///
    /// ctor 里 `Id`/`Active`/`ActiveTimeRemaining` 的读取顺序与 `Save` 一致。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        Ok(Self {
            id: r.read_i32()?,
            active: r.read_bool()?,
            active_time_remaining: r.read_i32()?,
        })
    }

    /// 编码，见 `Shared/Data/GuildData.cs:237`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i32(self.id);
        w.write_bool(self.active);
        w.write_i32(self.active_time_remaining);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir_core::enums::Stat;

    fn encode_into<T>(value: &T, encode: impl Fn(&T, &mut crate::PacketWriter)) -> Vec<u8> {
        let mut writer = PacketWriter::new();
        encode(value, &mut writer);
        writer.into_inner()
    }

    fn sample_info() -> GuildBuffInfo {
        GuildBuffInfo {
            id: 3,
            icon: 1200,
            name: "公会之力".to_owned(),
            level_requirement: 5,
            points_requirement: 20,
            time_limit: 3600,
            activation_cost: 500,
            stats: [(Stat::MaxDC, 5), (Stat::HP, 100), (Stat::MaxAC, -2)]
                .into_iter()
                .collect(),
        }
    }

    #[test]
    fn guild_buff_roundtrip_and_absolute_bytes() {
        let buff = GuildBuff {
            id: 0x0A0B_0C0D,
            active: true,
            active_time_remaining: -2,
        };
        let bytes = encode_into(&buff, GuildBuff::encode);

        // 逐字段按 `Shared/Data/GuildData.cs:237-242` 的写出顺序推导（全部小端）：
        //   i32  Id                  = 0x0A0B0C0D -> 0D 0C 0B 0A
        //   bool Active              = true       -> 01
        //   i32  ActiveTimeRemaining = -2         -> FE FF FF FF
        // 共 4 + 1 + 4 = 9 字节；`Info` 不参与。
        let expected: Vec<u8> = vec![
            0x0D, 0x0C, 0x0B, 0x0A, // Id
            0x01, // Active
            0xFE, 0xFF, 0xFF, 0xFF, // ActiveTimeRemaining
        ];
        assert_eq!(bytes.len(), 9);
        assert_eq!(bytes, expected);

        let mut reader = PacketReader::new(&bytes);
        let decoded = GuildBuff::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, buff);
        assert!(reader.is_empty());
    }

    #[test]
    fn guild_buff_default_roundtrip() {
        let buff = GuildBuff::default();
        let bytes = encode_into(&buff, GuildBuff::encode);
        let mut reader = PacketReader::new(&bytes);
        assert_eq!(GuildBuff::decode(&mut reader).expect("解码应当成功"), buff);
        assert_eq!(bytes.len(), 9);
    }

    #[test]
    fn guild_buff_info_roundtrip() {
        let info = sample_info();
        let bytes = encode_into(&info, GuildBuffInfo::encode);
        let mut reader = PacketReader::new(&bytes);
        let decoded = GuildBuffInfo::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, info);
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn guild_buff_info_layout() {
        let info = sample_info();
        let bytes = encode_into(&info, GuildBuffInfo::encode);

        // 4(Id) + 4(Icon) + (1 + 12)(Name) + 1(Level) + 1(Points) + 4(TimeLimit)
        // + 4(ActivationCost) = 31 字节固定头，随后是 Stats。
        // 名字 "公会之力" 在 UTF-8 里 4 个汉字各 3 字节 = 12 字节。
        let header = 4 + 4 + 1 + 12 + 1 + 1 + 4 + 4;
        assert_eq!(bytes[8..9], [12], "Name 的 7 位长度前缀是 12");
        assert_eq!(bytes[9 + 12], 5, "LevelRequirement");
        assert_eq!(bytes[9 + 12 + 1], 20, "PointsRequirement");
        assert_eq!(
            u32::from_le_bytes([
                bytes[header],
                bytes[header + 1],
                bytes[header + 2],
                bytes[header + 3]
            ]),
            3,
            "Stats 个数为 3"
        );
        // Stats 按 Stat 判别值升序：MaxAC(1) -> MaxDC(5) -> HP(12)
        // （`Shared/Data/Stat.cs` 的 `Stat : byte` 判别值）。
        assert_eq!(bytes[header + 4], 1, "第一项是 MaxAC");
        assert_eq!(bytes[header + 5..header + 9], (-2i32).to_le_bytes());
        assert_eq!(bytes[header + 9], 5, "第二项是 MaxDC");
        assert_eq!(bytes[header + 14], 12, "第三项是 HP");
    }

    #[test]
    fn guild_buff_info_default_points_requirement_is_one() {
        // C# 字段初始值 `PointsRequirement = 1`（`GuildData.cs:98`）。
        assert_eq!(GuildBuffInfo::default().points_requirement, 1);
        let bytes = encode_into(&GuildBuffInfo::default(), GuildBuffInfo::encode);
        // 空名字：长度前缀 0 在 8..9，LevelRequirement 在 9，PointsRequirement 在 10。
        assert_eq!(bytes[10], 1);
        let mut reader = PacketReader::new(&bytes);
        assert_eq!(
            GuildBuffInfo::decode(&mut reader).expect("解码应当成功"),
            GuildBuffInfo::default()
        );
    }

    #[test]
    fn truncated_payload_errors() {
        let bytes = encode_into(&sample_info(), GuildBuffInfo::encode);
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(GuildBuffInfo::decode(&mut reader).is_err());
    }
}
