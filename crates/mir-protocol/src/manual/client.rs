//! 手写的客户端包。

use crate::data::ChatItem;

/// Crystal `ClientPackets.Chat`。
///
/// 手写原因：`LinkedItems` 是 `List<ChatItem>`，元素是未生成的嵌套数据类。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chat {
    /// 聊天内容。
    pub message: String,
    /// 内容里引用的物品链接。
    pub linked_items: Vec<ChatItem>,
}

impl Chat {
    /// 包编号。
    pub const INDEX: i16 = 13;
    /// 是否 gzip 压缩。
    pub const COMPRESSED: bool = false;

    /// 解码。
    pub fn decode(r: &mut crate::PacketReader<'_>) -> Result<Self, crate::ProtocolError> {
        let message = r.read_string()?;
        let count = r.read_count()?;
        let mut linked_items = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            linked_items.push(ChatItem::decode(r)?);
        }
        Ok(Self {
            message,
            linked_items,
        })
    }

    /// 编码。
    pub fn encode(&self, w: &mut crate::PacketWriter) {
        w.write_string(&self.message);
        w.write_count(self.linked_items.len());
        for item in &self.linked_items {
            item.encode(w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_roundtrip() {
        let packet = Chat {
            message: "收屠龙 <屠龙/7>".to_owned(),
            linked_items: vec![ChatItem {
                unique_id: 7,
                title: "屠龙".to_owned(),
                grid: mir_core::enums::MirGridType::Inventory,
            }],
        };
        let mut writer = crate::PacketWriter::new();
        packet.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = crate::PacketReader::new(&bytes);
        let decoded = Chat::decode(&mut reader).expect("decode");
        assert!(reader.is_empty());
        assert_eq!(decoded, packet);
    }
}
