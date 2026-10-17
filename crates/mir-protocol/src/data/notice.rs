//! 登录公告 `Notice` 的线格式。
//!
//! 对照 Crystal `Shared/Data/Notice.cs:1`（读入 ctor `:9`，`Save` `:15`）手写。
//! 它由 `ServerPackets.UpdateNotice`（`Shared/ServerPackets.cs:6723-6741`）下发，
//! 没有任何版本参数。
//!
//! 线格式（一行一字段，括号内是宽度）：
//!
//! ```text
//! string Title                <- 7 位长度前缀 + UTF-8
//! string Message              <- 7 位长度前缀 + UTF-8
//! ```
//!
//! C# 里的 `LastUpdate`（`Notice.cs:5`）**不在线格式里**：ctor 不读、`Save` 不写，
//! 它只用于服务端判断「本次登录是否需要推送公告」
//!（`Server/MirObjects/PlayerObject.cs:1172`），属于服务端状态，不进入本模块。

use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `Notice`（`Shared/Data/Notice.cs:1`；ctor `:9`，`Save` `:15`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Notice {
    /// 公告标题。
    pub title: String,
    /// 公告正文。
    pub message: String,
}

impl Notice {
    /// 解码，见 `Shared/Data/Notice.cs:9`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        Ok(Self {
            title: r.read_string()?,
            message: r.read_string()?,
        })
    }

    /// 编码，见 `Shared/Data/Notice.cs:15`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_string(&self.title);
        w.write_string(&self.message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let value = Notice {
            title: "维护公告".to_owned(),
            message: "今晚 23:00 停机维护。".to_owned(),
        };
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = Notice::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, value);
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn bytes_match_hand_derived_layout() {
        // 按 `Shared/Data/Notice.cs:15-19` 的写出顺序推导（全部小端）：
        //   string Title   = "公告"：UTF-8 6 字节 -> 06 E5 85 AC E5 91 8A
        //                    公 = E5 85 AC，告 = E5 91 8A
        //   string Message = "更新"：UTF-8 6 字节 -> 06 E6 9B B4 E6 96 B0
        //                    更 = E6 9B B4，新 = E6 96 B0
        // 合计 1 + 6 + 1 + 6 = 14 字节。
        let value = Notice {
            title: "公告".to_owned(),
            message: "更新".to_owned(),
        };

        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let expected: Vec<u8> = vec![
            0x06, 0xE5, 0x85, 0xAC, 0xE5, 0x91, 0x8A, // Title = "公告"
            0x06, 0xE6, 0x9B, 0xB4, 0xE6, 0x96, 0xB0, // Message = "更新"
        ];
        assert_eq!(bytes.len(), 14);
        assert_eq!(bytes, expected);

        let mut reader = PacketReader::new(&bytes);
        assert_eq!(Notice::decode(&mut reader).expect("解码应当成功"), value);
        assert!(reader.is_empty());
    }

    #[test]
    fn empty_strings_still_write_length_prefixes() {
        // 空串不是「不写」：两条长度前缀 0 各占一字节（共 2 字节）。
        let value = Notice::default();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();
        assert_eq!(bytes, vec![0x00, 0x00]);

        let mut reader = PacketReader::new(&bytes);
        assert_eq!(
            Notice::decode(&mut reader).expect("解码应当成功"),
            Notice::default()
        );
    }

    #[test]
    fn truncated_payload_errors() {
        let value = Notice {
            title: "a".to_owned(),
            message: "b".to_owned(),
        };
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(Notice::decode(&mut reader).is_err());
    }
}
