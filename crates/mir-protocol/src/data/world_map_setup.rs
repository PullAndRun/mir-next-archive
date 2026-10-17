//! 大地图配置 `WorldMapSetup` 的线格式。
//!
//! 对照 Crystal `Shared/Data/SharedData.cs:95`（读入 ctor `:102`，`Save` `:110`）手写。
//! 它是服务器包 `WorldMapSetup`（编号 19）的完整负载，ctor / `Save` 都不带 `version`
//! 参数，因此不存在需要按默认版本展开的分支。
//!
//! 线格式（全部小端）：
//!
//! ```text
//! bool            Enabled     是否启用（1 字节，非 0 即真）
//! i32             Icons.Count 图标数量
//! WorldMapIcon    Icons[i]    i32 ImageIndex + string Title + i32 MapIndex，逐项重复
//! ```

use crate::data::WorldMapIcon;
use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `WorldMapSetup`（`Shared/Data/SharedData.cs:95`；读入 ctor `:102`，`Save` `:110`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorldMapSetup {
    /// 是否启用大地图。
    pub enabled: bool,
    /// 大地图上的入口图标。
    pub icons: Vec<WorldMapIcon>,
}

impl WorldMapSetup {
    /// 解码，见 `Shared/Data/SharedData.cs:102`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let enabled = r.read_bool()?;
        let count = r.read_count()?;
        // 不预分配：数量来自不可信输入，逐项解析到 EOF 会自然报错。
        let mut icons = Vec::new();
        for _ in 0..count {
            icons.push(WorldMapIcon::decode(r)?);
        }
        Ok(Self { enabled, icons })
    }

    /// 编码，见 `Shared/Data/SharedData.cs:110`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_bool(self.enabled);
        w.write_count(self.icons.len());
        for icon in &self.icons {
            icon.encode(w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 两个图标，其中一个是空标题，用来覆盖字符串长度前缀的两端。
    fn sample() -> WorldMapSetup {
        WorldMapSetup {
            enabled: true,
            icons: vec![
                WorldMapIcon {
                    image_index: 1,
                    title: "比奇省".to_owned(),
                    map_index: 0,
                },
                WorldMapIcon {
                    image_index: -2,
                    title: String::new(),
                    map_index: 0x7FFF_FFFF,
                },
            ],
        }
    }

    #[test]
    fn roundtrip() {
        let value = sample();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = WorldMapSetup::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, value);
        assert!(reader.is_empty(), "应正好读完负载");
    }

    #[test]
    fn empty_icon_list_is_five_bytes() {
        // 按 `Shared/Data/SharedData.cs:110`：
        //   bool Enabled -> 01
        //   i32  Count   -> 00 00 00 00
        let value = WorldMapSetup {
            enabled: true,
            icons: Vec::new(),
        };
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        assert_eq!(bytes, vec![0x01, 0x00, 0x00, 0x00, 0x00]);

        let mut reader = PacketReader::new(&bytes);
        assert_eq!(
            WorldMapSetup::decode(&mut reader).expect("解码应当成功"),
            value
        );
        assert!(reader.is_empty());
    }

    #[test]
    fn icon_count_prefixes_the_list() {
        // 两个图标：第 1 个 4 + (1 + 9) + 4 = 18 字节，第 2 个 4 + (1 + 0) + 4 = 9 字节，
        // 加上 bool(1) + count(4)，共 32 字节。
        let mut writer = PacketWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();

        assert_eq!(bytes.len(), 32);
        assert_eq!(&bytes[..5], &[0x01, 0x02, 0x00, 0x00, 0x00]);
        // 第 1 个图标：ImageIndex = 1，Title = "比奇省"（UTF-8 9 字节），MapIndex = 0。
        assert_eq!(&bytes[5..9], &[0x01, 0x00, 0x00, 0x00]);
        assert_eq!(bytes[9], 0x09);
        assert_eq!(
            &bytes[10..19],
            &[0xE6, 0xAF, 0x94, 0xE5, 0xA5, 0x87, 0xE7, 0x9C, 0x81]
        );
        assert_eq!(&bytes[19..23], &[0x00, 0x00, 0x00, 0x00]);
        // 第 2 个图标：ImageIndex = -2，Title = ""，MapIndex = 0x7FFFFFFF。
        assert_eq!(&bytes[23..27], &[0xFE, 0xFF, 0xFF, 0xFF]);
        assert_eq!(bytes[27], 0x00);
        assert_eq!(&bytes[28..32], &[0xFF, 0xFF, 0xFF, 0x7F]);
    }

    #[test]
    fn negative_count_errors() {
        // 长度前缀是 i32，Crystal 侧 `for (int i = 0; i < count; i++)` 会直接跳过；
        // Rust 侧把负数视为损坏数据（`read_count` 报错），语义更严格。
        let bytes = vec![0x00, 0xFF, 0xFF, 0xFF, 0xFF];
        let mut reader = PacketReader::new(&bytes);
        assert!(WorldMapSetup::decode(&mut reader).is_err());
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();
        // 去掉最后一个图标的最后一字节。
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(WorldMapSetup::decode(&mut reader).is_err());
    }
}
