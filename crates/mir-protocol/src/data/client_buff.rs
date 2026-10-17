//! 客户端增益（buff 图标）`ClientBuff` 的线格式。
//!
//! 对照 Crystal `Shared/Data/ClientData.cs:575`（读入 ctor `:593`，`Save` `:616`）手写。
//! ctor 与 `Save` 都不带 `version` 参数，没有版本分支。
//!
//! 线格式（一行一字段，全部小端）：
//!
//! ```text
//! u8    Type            BuffType
//! bool  Visible
//! u32   ObjectID
//! i64   ExpireTime      Unix 毫秒时间戳
//! bool  Infinite
//! bool  Paused
//! Stats Stats           i32 项数 + (u8 stat, i32 value)*
//! i32   Values.Length + i32*
//! ```
//!
//! 三个 C# 侧陷阱：
//!
//! * `Caster`（`:578`）**不在线上**：ctor 第一行是 `Caster = null;`（`:595`），
//!   `Save` 也从不写它——它只是服务器/客户端本地的展示缓存。所以这里没有该字段。
//! * `Paused` 在类里声明于 `Stats` 之后（`:584`），但 `Save` 把它写在 `Stats`
//!   **之前**（`:623`）；读入 ctor 也按「先 `Paused` 后 `Stats`」读（`:602`-`:604`）。
//!   这里以 `Save` 的顺序为准。
//! * 无参 ctor 只初始化 `Stats`（`:590`），`Values` 仍是 `null`，此时 `Save`
//!   会在 `Values.Length` 上抛空引用异常。Rust 侧用空 `Vec` 表示，写出 `0`。

use mir_core::enums::BuffType;

use crate::data::Stats;
use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `ClientBuff`（`Shared/Data/ClientData.cs:575`；读入 ctor `:593`，`Save` `:616`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClientBuff {
    /// C# 字段 `Type`：增益类型。
    pub buff_type: BuffType,
    /// C# 字段 `Visible`：是否在客户端图标栏显示。
    pub visible: bool,
    /// C# 字段 `ObjectID`：施法者/来源对象的编号。
    pub object_id: u32,
    /// C# 字段 `ExpireTime`：到期时间（Unix 毫秒），`Infinite` 时无意义。
    pub expire_time: i64,
    /// C# 字段 `Infinite`：是否永久。
    pub infinite: bool,
    /// C# 字段 `Paused`：计时是否暂停；线上位置在 `Stats` 之前。
    pub paused: bool,
    /// C# 字段 `Stats`：该增益提供的属性加成。
    pub stats: Stats,
    /// C# 字段 `Values`：各技能的附加参数（长度随增益类型变化）。
    pub values: Vec<i32>,
}

impl ClientBuff {
    /// 解码，见 `Shared/Data/ClientData.cs:593`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        // `Caster = null;` —— 不读。
        let buff_type = BuffType::from_value(r.read_u8()?).unwrap_or_default();
        let visible = r.read_bool()?;
        let object_id = r.read_u32()?;
        let expire_time = r.read_i64()?;
        let infinite = r.read_bool()?;
        let paused = r.read_bool()?;

        let stats = Stats::decode(r)?;

        let count = r.read_count()?;
        // 不预分配：长度来自不可信输入。
        let mut values = Vec::new();
        for _ in 0..count {
            values.push(r.read_i32()?);
        }

        Ok(Self {
            buff_type,
            visible,
            object_id,
            expire_time,
            infinite,
            paused,
            stats,
            values,
        })
    }

    /// 编码，见 `Shared/Data/ClientData.cs:616`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_u8(self.buff_type.value());
        w.write_bool(self.visible);
        w.write_u32(self.object_id);
        w.write_i64(self.expire_time);
        w.write_bool(self.infinite);
        w.write_bool(self.paused);

        self.stats.encode(w);

        w.write_count(self.values.len());
        for &value in &self.values {
            w.write_i32(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir_core::enums::Stat;

    /// 编码后再解码，断言相等且负载被正好读完。
    fn roundtrip(value: &ClientBuff) {
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = ClientBuff::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, *value, "解码结果应与原值相同");
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn bytes_match_hand_derived_layout() {
        // 按 `ClientData.cs:616` 的写出顺序推导（全部小端）：
        //   u8   Type        = BuffType.Hiding = 2        -> 02
        //   bool Visible     = true                       -> 01
        //   u32  ObjectID    = 0x01020304                 -> 04 03 02 01
        //   i64  ExpireTime  = -2                         -> FE FF FF FF FF FF FF FF
        //   bool Infinite    = false                      -> 00
        //   bool Paused      = true                       -> 01
        //   Stats            = 空（i32 0）                -> 00 00 00 00
        //   i32  Values.Length = 2                        -> 02 00 00 00
        //   i32  [0]         = 7                          -> 07 00 00 00
        //   i32  [1]         = -8                         -> F8 FF FF FF
        // 合计 1+1+4+8+1+1+4+4+4+4 = 32 字节。
        let value = ClientBuff {
            buff_type: BuffType::Hiding,
            visible: true,
            object_id: 0x0102_0304,
            expire_time: -2,
            infinite: false,
            paused: true,
            stats: Stats::default(),
            values: vec![7, -8],
        };

        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let expected: Vec<u8> = vec![
            0x02, // Type = Hiding
            0x01, // Visible
            0x04, 0x03, 0x02, 0x01, // ObjectID
            0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // ExpireTime = -2
            0x00, // Infinite
            0x01, // Paused
            0x00, 0x00, 0x00, 0x00, // Stats.Count = 0
            0x02, 0x00, 0x00, 0x00, // Values.Length = 2
            0x07, 0x00, 0x00, 0x00, // Values[0]
            0xF8, 0xFF, 0xFF, 0xFF, // Values[1]
        ];

        assert_eq!(bytes.len(), 32);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn paused_is_written_before_stats() {
        // 回归点：`Paused` 在 C# 类里声明在 `Stats` 之后，但线上在 `Stats` 之前。
        // 偏移量：u8 + bool + u32 + i64 + bool = 15，第 15 字节就是 Paused。
        let mut writer = PacketWriter::new();
        ClientBuff {
            paused: true,
            ..ClientBuff::default()
        }
        .encode(&mut writer);
        let bytes = writer.into_inner();
        assert_eq!(bytes[15], 1, "第 15 字节必须是 Paused");
        roundtrip(&ClientBuff {
            paused: true,
            ..ClientBuff::default()
        });
    }

    #[test]
    fn roundtrip_with_stats_and_values() {
        let mut stats = Stats::default();
        stats.set(Stat::MaxDC, 5);
        stats.set(Stat::Luck, -3);

        let value = ClientBuff {
            buff_type: BuffType::MagicShield,
            visible: false,
            object_id: u32::MAX,
            expire_time: 1_700_000_000_000,
            infinite: true,
            paused: false,
            stats,
            values: vec![1, 2, 3, 4, 5],
        };

        roundtrip(&value);
    }

    #[test]
    fn default_buff_is_minimal() {
        let value = ClientBuff::default();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        // 1+1+4+8+1+1 + 4（空 Stats）+ 4（空 Values）= 24 字节。
        assert_eq!(writer.len(), 24);
        roundtrip(&value);
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        ClientBuff::default().encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(ClientBuff::decode(&mut reader).is_err());
    }
}
