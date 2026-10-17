//! 公会职位（`GuildRank`）与其成员（`GuildMember`）的线格式。
//!
//! 对应 Crystal `Shared/Data/GuildData.cs:1` 的 `GuildRank` 与 `:62` 的 `GuildMember`。
//!
//! 两个类在 Crystal 里都有「网络 / 数据库」两种布局，用参数切换：
//!
//! * `GuildRank(BinaryReader, bool offline = false)`（`:10`）与
//!   `Save(BinaryWriter, bool save = false)`（`:26`）：`offline` 与 `save` 同义，
//!   为 `true` 时**不写 `Index`**。网络包（`Shared/ServerPackets.cs:4516`、`:4528`）
//!   用缺省值，所以**写 `Index`**；数据库路径（`Server/MirDatabase/GuildInfo.cs:114`
//!   与 `:220`）传 `true`，不写 `Index`。
//! * `GuildMember(BinaryReader, bool offline = false)`（`:73`）：`offline` **不影响字节**，
//!   只在读完后把 `Online` 置为 `false`（`:80`）。
//!
//! 按本任务约定「版本/参数一律取包路径缺省值」，这里静态解析为 `offline = false` /
//! `save = false`：固定读写 `Index`，不提供 `offline` / `save` 变体。
//! 数据库布局若将来需要，另行添加方法即可。

use mir_core::DateTimeBinary;
use mir_core::enums::GuildRankOptions;

use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `GuildMember`（`Shared/Data/GuildData.cs:62`；读入 ctor `:73`，`Save` `:82`）。
///
/// 它**只**作为 `GuildRank.Members` 的嵌套负载出现在线上
/// （唯一使用者是 `ServerPackets.GuildMemberChange`，`Shared/ServerPackets.cs:4506`），
/// 因此随 `GuildRank` 一起放在本模块，不单独成模块、不在 `data/mod.rs` 里导出。
///
/// `offline` 参数不改线格式（见模块文档），故这里没有对应字段。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildMember {
    /// 角色名（`string`，7 位长度前缀）。
    pub name: String,
    /// 角色编号。
    pub id: i32,
    /// 最后登录时间（`DateTime.ToBinary()` 的 8 字节；见 `GuildData.cs:86`）。
    pub last_login: DateTimeBinary,
    /// 是否已投票。
    pub has_voted: bool,
    /// 是否在线。
    pub online: bool,
}

impl GuildMember {
    /// 解码：`string name`、`i32 id`、`i64 lastLogin`、`bool hasvoted`、`bool online`。
    ///
    /// 见 `Shared/Data/GuildData.cs:73`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        Ok(Self {
            name: r.read_string()?,
            id: r.read_i32()?,
            last_login: DateTimeBinary(r.read_i64()?),
            has_voted: r.read_bool()?,
            online: r.read_bool()?,
        })
    }

    /// 编码：字段顺序与 [`GuildMember::decode`] 一致。
    ///
    /// 见 `Shared/Data/GuildData.cs:82`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_string(&self.name);
        w.write_i32(self.id);
        w.write_i64(self.last_login.0);
        w.write_bool(self.has_voted);
        w.write_bool(self.online);
    }
}

/// Crystal `GuildRank`（`Shared/Data/GuildData.cs:1`；读入 ctor `:10`，`Save` `:26`）。
///
/// 线格式（一行一字段，对应 `Save(writer)` 的缺省参数）：
///
/// ```text
/// string Name                  <- 7 位长度前缀 + UTF-8
/// u8     Options               <- C# 的 [Flags] 位掩码，见 options 字段说明
/// i32    Index                 <- offline/save 为 true 时缺省，网络包恒有
/// i32    Members.Count
/// per member: GuildMember      <- Name, Id, LastLogin, hasvoted, Online
/// ```
///
/// 字段声明顺序与 `Save` 的写出顺序一致，便于逐行核对。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildRank {
    /// 职位名。
    pub name: String,
    /// 职位权限位掩码（C# `GuildRankOptions Options`）。
    ///
    /// **故意用 `u8` 而不是 `mir_core::enums::GuildRankOptions`**：
    /// C# 侧是 `[Flags] enum GuildRankOptions : byte`（`Shared/Enums.cs`），
    /// 线上几乎总是多位组合——公会会长是 `(GuildRankOptions)255`
    /// （`Server/MirDatabase/GuildInfo.cs:54`），普通职位则由服务器逐位置位累加
    /// （`Server/MirObjects/GuildObject.cs:370` 的 `Options |= (GuildRankOptions)(1 << Option)`）。
    /// 也就是说线上是**位掩码**，不能用单值枚举 roundtrip。
    /// Rust 生成的是单值枚举，`from_value(255)` / `from_value(3)` 都会返回 `None`，
    /// 落到 `unwrap_or_default()` 就变成 `1`，重新编码会改写原始字节。
    /// 逐字节保真优先，所以这里存原始字节；需要按位判断时用 [`GuildRank::has_option`]。
    pub options: u8,
    /// 职位序号（`0` 是会长职位）。数据库路径不写这个字段。
    pub index: i32,
    /// 该职位下的成员列表（线上是 `i32` 个数 + 逐个 [`GuildMember`]）。
    pub members: Vec<GuildMember>,
}

impl GuildRank {
    /// 解码，对应 `GuildRank(BinaryReader reader)` 的 `offline = false` 分支。
    ///
    /// 见 `Shared/Data/GuildData.cs:10`：`Name`、`Options`、`Index` 之后才是成员个数。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let name = r.read_string()?;
        let options = r.read_u8()?;

        // `if (!offline) Index = reader.ReadInt32();`（`GuildData.cs:15-18`），
        // 包路径 offline=false，所以一定读。
        let index = r.read_i32()?;

        let member_count = r.read_count()?;
        // 与 `data::UserItem` 一样不预分配：长度来自不可信输入。
        let mut members = Vec::new();
        for _ in 0..member_count {
            members.push(GuildMember::decode(r)?);
        }

        Ok(Self {
            name,
            options,
            index,
            members,
        })
    }

    /// 编码，对应 `Save(BinaryWriter writer)` 的 `save = false` 分支。
    ///
    /// 见 `Shared/Data/GuildData.cs:26`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_string(&self.name);
        // `writer.Write((byte)Options)`（`GuildData.cs:29`）：单字节位掩码。
        w.write_u8(self.options);

        // `if (!save) writer.Write(Index);`（`GuildData.cs:30-33`），包路径 save=false。
        w.write_i32(self.index);

        w.write_count(self.members.len());
        for member in &self.members {
            member.encode(w);
        }
    }

    /// 判断是否拥有某项权限，等价于 C# 的 `Options.HasFlag(option)`
    /// （例如 `Server/MirObjects/PlayerObject.cs:10362`）。
    pub fn has_option(&self, option: GuildRankOptions) -> bool {
        self.options & option.value() != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_into<T>(value: &T, encode: impl Fn(&T, &mut crate::PacketWriter)) -> Vec<u8> {
        let mut writer = PacketWriter::new();
        encode(value, &mut writer);
        writer.into_inner()
    }

    fn roundtrip(rank: &GuildRank) {
        let bytes = encode_into(rank, GuildRank::encode);
        let mut reader = PacketReader::new(&bytes);
        let decoded = GuildRank::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, *rank, "解码结果应与原值相同");
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    fn sample() -> GuildRank {
        GuildRank {
            name: "会长".to_owned(),
            // 255 = 全部权限，Crystal 里会长的真实取值（`GuildInfo.cs:54`）。
            options: 0xFF,
            index: 0,
            members: vec![
                GuildMember {
                    name: "张三".to_owned(),
                    id: 7,
                    last_login: DateTimeBinary(0x1122_3344_5566_7788),
                    has_voted: true,
                    online: true,
                },
                GuildMember {
                    name: "Li Si".to_owned(),
                    id: -3,
                    last_login: DateTimeBinary(-1),
                    has_voted: false,
                    online: false,
                },
            ],
        }
    }

    #[test]
    fn guild_rank_roundtrip_with_members() {
        roundtrip(&sample());
    }

    #[test]
    fn guild_rank_roundtrip_without_members() {
        roundtrip(&GuildRank {
            name: "Members".to_owned(),
            options: 0,
            index: 1,
            members: Vec::new(),
        });
    }

    #[test]
    fn options_are_a_raw_bitmask() {
        // 组合值（`CanRecruit|CanChangeRank` = 3、会长 255）必须原样往返。
        // 若改用单值枚举，3/255 会被 `unwrap_or_default()` 折成 1。
        for options in [0u8, 1, 3, 5, 0x7F, 0xFF] {
            let rank = GuildRank {
                options,
                ..GuildRank::default()
            };
            roundtrip(&rank);
        }

        let rank = sample();
        assert!(rank.has_option(GuildRankOptions::CanRecruit));
        assert!(rank.has_option(GuildRankOptions::CanActivateBuff));
        let plain = GuildRank {
            options: 0,
            ..GuildRank::default()
        };
        assert!(!plain.has_option(GuildRankOptions::CanKick));
    }

    #[test]
    fn index_sits_between_options_and_member_count() {
        // 逐字节验证布局：`Save` 的顺序是 Name、Options、Index、Members.Count。
        let rank = GuildRank {
            name: "R".to_owned(),
            options: 0x05,
            index: 0x0102_0304,
            members: Vec::new(),
        };
        let bytes = encode_into(&rank, GuildRank::encode);
        // 1 字节长度前缀 + 1 字节内容（"R"）= 2 字节，然后是 Options。
        assert_eq!(&bytes[..2], &[0x01, b'R']);
        assert_eq!(bytes[2], 0x05);
        assert_eq!(&bytes[3..7], &[0x04, 0x03, 0x02, 0x01]);
        assert_eq!(&bytes[7..11], &[0x00, 0x00, 0x00, 0x00]);
        assert_eq!(bytes.len(), 11);
    }

    #[test]
    fn truncated_members_error() {
        let bytes = encode_into(&sample(), GuildRank::encode);
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(GuildRank::decode(&mut reader).is_err());
    }
}
