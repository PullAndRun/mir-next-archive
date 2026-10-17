//! 好友列表条目的线格式。
//!
//! 对应 Crystal `Shared/Data/ClientData.cs:122` 的 `ClientFriend`，
//! 线上出现在 `ServerPackets.FriendUpdate` 的 `Friends` 列表里
//! （`Shared/ServerPackets.cs:6162-6186`：`i32` 个数 + 逐个条目）。

use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `ClientFriend`（`Shared/Data/ClientData.cs:122`；读入 ctor `:133`，`Save` `:143`）。
///
/// 线格式（一行一字段）：
///
/// ```text
/// i32    Index
/// string Name      <- 7 位长度前缀 + UTF-8
/// string Memo
/// bool   Blocked
/// bool   Online
/// ```
///
/// 两个 `bool` 是各自独立的 1 字节，**不是**位域。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClientFriend {
    /// 好友在服务器数据库里的编号。
    pub index: i32,
    /// 好友角色名。
    pub name: String,
    /// 备注（C# 字段初始值是 `""`，与派生默认一致）。
    pub memo: String,
    /// 是否已拉黑。
    pub blocked: bool,
    /// 是否在线。
    pub online: bool,
}

impl ClientFriend {
    /// 解码，见 `Shared/Data/ClientData.cs:133`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        Ok(Self {
            index: r.read_i32()?,
            name: r.read_string()?,
            memo: r.read_string()?,
            blocked: r.read_bool()?,
            online: r.read_bool()?,
        })
    }

    /// 编码，见 `Shared/Data/ClientData.cs:143`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i32(self.index);
        w.write_string(&self.name);
        w.write_string(&self.memo);
        w.write_bool(self.blocked);
        w.write_bool(self.online);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_into(friend: &ClientFriend) -> Vec<u8> {
        let mut writer = PacketWriter::new();
        friend.encode(&mut writer);
        writer.into_inner()
    }

    fn roundtrip(friend: &ClientFriend) {
        let bytes = encode_into(friend);
        let mut reader = PacketReader::new(&bytes);
        let decoded = ClientFriend::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, *friend, "解码结果应与原值相同");
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn client_friend_roundtrip() {
        roundtrip(&ClientFriend {
            index: 42,
            name: "张三".to_owned(),
            memo: "法師".to_owned(),
            blocked: true,
            online: false,
        });
    }

    #[test]
    fn client_friend_empty_roundtrip() {
        roundtrip(&ClientFriend::default());
    }

    #[test]
    fn client_friend_layout() {
        let friend = ClientFriend {
            index: 0x0102_0304,
            name: "Li".to_owned(),
            memo: String::new(),
            blocked: true,
            online: false,
        };
        let bytes = encode_into(&friend);
        // i32 Index 小端。
        assert_eq!(&bytes[..4], &[0x04, 0x03, 0x02, 0x01]);
        // 名字：7 位长度前缀 2 + "Li"。
        assert_eq!(&bytes[4..7], &[0x02, b'L', b'i']);
        // 备注：长度前缀 0（空串仍写 1 字节）。
        assert_eq!(bytes[7], 0x00);
        // 两个独立的 bool。
        assert_eq!(&bytes[8..10], &[0x01, 0x00]);
        assert_eq!(bytes.len(), 10);

        // 中文名字按 UTF-8 计长：3 个汉字 = 9 字节。
        let bytes = encode_into(&ClientFriend {
            name: "张三丰".to_owned(),
            ..ClientFriend::default()
        });
        assert_eq!(bytes[4], 9);
        assert_eq!(bytes.len(), 4 + 1 + 9 + 1 + 1 + 1);
    }

    #[test]
    fn truncated_payload_errors() {
        let bytes = encode_into(&ClientFriend {
            name: "A".to_owned(),
            ..ClientFriend::default()
        });
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(ClientFriend::decode(&mut reader).is_err());
    }
}
