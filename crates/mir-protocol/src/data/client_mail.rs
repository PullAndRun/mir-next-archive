//! 邮件条目的线格式。
//!
//! 对应 Crystal `Shared/Data/ClientData.cs:154` 的 `ClientMail`，
//! 线上出现在 `ServerPackets.ReceiveMail` 的 `Mail` 列表里
//! （`Shared/ServerPackets.cs:5886-5909`：`i32` 个数 + 逐个条目）。

use mir_core::DateTimeBinary;

use crate::data::UserItem;
use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `ClientMail`（`Shared/Data/ClientData.cs:154`；读入 ctor `:168`，`Save` `:187`）。
///
/// 线格式（一行一字段）：
///
/// ```text
/// u64      MailID
/// string   SenderName        <- 7 位长度前缀 + UTF-8
/// string   Message
/// bool     Opened
/// bool     Locked
/// bool     CanReply
/// bool     Collected
/// i64      DateSent          <- DateTime.ToBinary()
/// u32      Gold
/// i32      Items.Count
/// per item: UserItem         <- 递归的完整物品负载，见 `data::UserItem`
/// ```
///
/// 4 个 `bool` 是各自独立的 1 字节，**不是**位域；`DateSent` 位于邮箱标志与 `Gold` 之间，
/// 顺序极易写反（`ClientData.cs:173-181`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClientMail {
    /// 邮件唯一编号。
    pub mail_id: u64,
    /// 发件人角色名。C# 侧未初始化（可能为 `null`），这里用 `String` 表示空串。
    pub sender_name: String,
    /// 正文。同样未初始化，空串表示无正文。
    pub message: String,
    /// 是否已读。
    pub opened: bool,
    /// 是否锁定（锁定后不可删除）。
    pub locked: bool,
    /// 是否可回复。
    pub can_reply: bool,
    /// 附件是否已领取。
    pub collected: bool,
    /// 发送时间（`DateTime.ToBinary()`）。C# 默认值 `DateTime.MinValue` 的
    /// `ToBinary()` 恰好是 `0`，与派生默认一致。
    pub date_sent: DateTimeBinary,
    /// 附带的金币。
    pub gold: u32,
    /// 附带的物品。
    pub items: Vec<UserItem>,
}

impl ClientMail {
    /// 解码，见 `Shared/Data/ClientData.cs:168`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let mail_id = r.read_u64()?;
        let sender_name = r.read_string()?;
        let message = r.read_string()?;
        let opened = r.read_bool()?;
        let locked = r.read_bool()?;
        let can_reply = r.read_bool()?;
        let collected = r.read_bool()?;
        let date_sent = DateTimeBinary(r.read_i64()?);
        let gold = r.read_u32()?;

        let count = r.read_count()?;
        // 与 `data::UserItem` 一致：长度来自不可信输入，不预分配。
        let mut items = Vec::new();
        for _ in 0..count {
            items.push(UserItem::decode(r)?);
        }

        Ok(Self {
            mail_id,
            sender_name,
            message,
            opened,
            locked,
            can_reply,
            collected,
            date_sent,
            gold,
            items,
        })
    }

    /// 编码，见 `Shared/Data/ClientData.cs:187`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_u64(self.mail_id);
        w.write_string(&self.sender_name);
        w.write_string(&self.message);
        w.write_bool(self.opened);
        w.write_bool(self.locked);
        w.write_bool(self.can_reply);
        w.write_bool(self.collected);
        w.write_i64(self.date_sent.0);
        w.write_u32(self.gold);
        w.write_count(self.items.len());
        for item in &self.items {
            item.encode(w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_into(mail: &ClientMail) -> Vec<u8> {
        let mut writer = PacketWriter::new();
        mail.encode(&mut writer);
        writer.into_inner()
    }

    fn roundtrip(mail: &ClientMail) {
        let bytes = encode_into(mail);
        let mut reader = PacketReader::new(&bytes);
        let decoded = ClientMail::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, *mail, "解码结果应与原值相同");
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    fn sample() -> ClientMail {
        ClientMail {
            mail_id: 0x1122_3344_5566_7788,
            sender_name: "张三".to_owned(),
            message: "你好，附件是屠龙。".to_owned(),
            opened: true,
            locked: false,
            can_reply: true,
            collected: false,
            date_sent: DateTimeBinary(-1234),
            gold: 50_000,
            items: vec![
                UserItem {
                    unique_id: 7,
                    item_index: 4242,
                    count: 2,
                    ..UserItem::default()
                },
                UserItem {
                    unique_id: 8,
                    item_index: 1,
                    identified: true,
                    ..UserItem::default()
                },
            ],
        }
    }

    #[test]
    fn client_mail_roundtrip() {
        roundtrip(&sample());
    }

    #[test]
    fn client_mail_empty_roundtrip() {
        roundtrip(&ClientMail::default());
    }

    #[test]
    fn bools_and_date_order() {
        // `ClientData.cs:173-181`：4 个 bool 之后才是 DateSent(i64)，再是 Gold(u32)。
        let mail = ClientMail {
            mail_id: 0,
            sender_name: String::new(),
            message: String::new(),
            opened: true,
            locked: false,
            can_reply: true,
            collected: true,
            date_sent: DateTimeBinary(0x0102_0304_0506_0708),
            gold: 0x0A0B_0C0D,
            items: Vec::new(),
        };
        let bytes = encode_into(&mail);
        // 8(MailID) + 1 + 1(两个空串的长度前缀) = 10 起是 4 个 bool。
        assert_eq!(&bytes[10..14], &[0x01, 0x00, 0x01, 0x01]);
        assert_eq!(
            &bytes[14..22],
            &0x0102_0304_0506_0708i64.to_le_bytes(),
            "DateSent 必须紧跟 4 个 bool"
        );
        assert_eq!(&bytes[22..26], &0x0A0B_0C0Di32.to_le_bytes(), "Gold 是 u32");
        assert_eq!(&bytes[26..30], &[0x00, 0x00, 0x00, 0x00], "物品个数 0");
        assert_eq!(bytes.len(), 30);
    }

    #[test]
    fn item_count_and_payload() {
        let mail = sample();
        let bytes = encode_into(&mail);

        let mut tail = PacketWriter::new();
        mail.items[0].encode(&mut tail);
        let first_item_len = tail.len();

        // 最后一个字段是物品列表：i32 个数 + 两个完整物品。
        let mut only_items = PacketWriter::new();
        for item in &mail.items {
            item.encode(&mut only_items);
        }
        let items_len = only_items.len();
        assert_eq!(
            &bytes[bytes.len() - items_len - 4..bytes.len() - items_len],
            &[0x02, 0x00, 0x00, 0x00],
            "物品个数是 i32 2"
        );
        assert!(first_item_len < items_len);
        roundtrip(&mail);
    }

    #[test]
    fn truncated_payload_errors() {
        let bytes = encode_into(&sample());
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(ClientMail::decode(&mut reader).is_err());
    }
}
