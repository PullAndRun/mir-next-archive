//! `.mnd`（Mir Next Data）容器：服务端运行时读取的自有数据格式。
//!
//! 设计目标：**运行时不再直接读 `Server.MirDB` / `Envir/*.txt`**。导入工具把老数据
//! 解析、校验后写成 `.mnd`；服务端只读 `.mnd`（缺失时才在开发期回退老格式，见
//! `mir-server` 的 `load_game_data`）。
//!
//! 容器采用 TLV 索引 + 逐段 zlib：
//!
//! ```text
//! magic "MND1" | format_version u16 | schema_version u16 | flags u16 | reserved u16
//! section_count u32 | index_bytes u32
//! index: [ tag u16 | codec u8 | flags u8 | raw_len u64 | stored_len u64 | offset u64 | name_len u16 | name ]
//! payload: 各段压缩后的字节（offset 相对 payload 起点）
//! ```
//!
//! * `codec=0` 原样，`codec=1` zlib（当前导入器统一用 1）；
//! * `tag` 见 [`TAG_DB`] / [`TAG_DB_BIN`] / [`TAG_ENVIR`]，未知 tag 读取时保留、由上层忽略；
//! * 段名：DB 固定 `db`（M1 原始字节）或 `db.bin`（M2 bincode），Envir 文件固定
//!   `Envir/<相对路径>`（正斜杠）。
//!
//! 版本：
//! * `format_version=1`：M1 容器，DB 段是 `Server.MirDB` 原始字节；
//! * `format_version=2`：M2 容器，DB 段是 [`crate::schema::encode_db`] 的 bincode 字节；
//! * 读取端同时接受 1/2（M1 只读兼容），写出端固定 2。

use std::io::Read;
use std::path::Path;

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

/// 容器魔数。
pub const MAGIC: &[u8; 4] = b"MND1";
/// 容器格式版本（当前写出 2）。
pub const FORMAT_VERSION: u16 = 2;
/// 兼容读取的旧容器版本（M1）。
pub const LEGACY_FORMAT_VERSION: u16 = 1;
/// 读取端接受的容器版本。
pub const SUPPORTED_FORMAT_VERSIONS: &[u16] = &[LEGACY_FORMAT_VERSION, FORMAT_VERSION];
/// 数据 schema 版本（DB/脚本语义变化时 +1，与容器布局解耦）。
pub const SCHEMA_VERSION: u16 = 2;
/// M1 DB 段 tag（`Server.MirDB` 原始字节，只读兼容）。
pub const TAG_DB: u16 = 0x0001;
/// Envir 文本段 tag。
pub const TAG_ENVIR: u16 = 0x0002;
/// 保留给 manifest / 调试段。
pub const TAG_MANIFEST: u16 = 0x0003;
/// M2 DB 段 tag（[`crate::schema`] 的 bincode 编码）。
pub const TAG_DB_BIN: u16 = 0x0004;

/// 段编码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    /// 原样字节。
    Raw = 0,
    /// zlib。
    Zlib = 1,
}

impl Codec {
    fn from_u8(value: u8) -> Result<Self, MndError> {
        match value {
            0 => Ok(Self::Raw),
            1 => Ok(Self::Zlib),
            other => Err(MndError::UnknownCodec(other)),
        }
    }
}

/// 一个数据段（解压后的内容）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MndSection {
    /// 段类型（[`TAG_DB`] / [`TAG_ENVIR`] / 未知）。
    pub tag: u16,
    /// 段名（`db`、`Envir/NPCs/...`）。
    pub name: String,
    /// 解压后的内容。
    pub data: Vec<u8>,
}

impl MndSection {
    /// 新建一个段。
    #[must_use]
    pub fn new(tag: u16, name: impl Into<String>, data: Vec<u8>) -> Self {
        Self {
            tag,
            name: name.into(),
            data,
        }
    }
}

/// `.mnd` 文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MndFile {
    /// 容器格式版本（读取时校验）。
    pub format_version: u16,
    /// 数据 schema 版本。
    pub schema_version: u16,
    /// 段列表（顺序即写出顺序）。
    pub sections: Vec<MndSection>,
}

impl Default for MndFile {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            schema_version: SCHEMA_VERSION,
            sections: Vec::new(),
        }
    }
}

impl MndFile {
    /// 新文件（当前 schema）。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一个段。
    pub fn push(&mut self, section: MndSection) {
        self.sections.push(section);
    }

    /// 按段名取段。
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&MndSection> {
        self.sections.iter().find(|section| section.name == name)
    }

    /// 编码为 `.mnd` 字节（每段 zlib）。
    ///
    /// # Errors
    /// 段数/段名超出 u32/u16 或压缩失败。
    pub fn to_bytes(&self) -> Result<Vec<u8>, MndError> {
        let section_count = u32::try_from(self.sections.len())
            .map_err(|_| MndError::TooManySections(self.sections.len()))?;
        let mut index = Vec::new();
        let mut payload = Vec::new();
        for section in &self.sections {
            let stored = zlib(&section.data)?;
            let name_len = u16::try_from(section.name.len())
                .map_err(|_| MndError::NameTooLong(section.name.clone()))?;
            index.extend_from_slice(&section.tag.to_le_bytes());
            index.push(Codec::Zlib as u8);
            index.push(0); // flags
            index.extend_from_slice(&(section.data.len() as u64).to_le_bytes());
            index.extend_from_slice(&(stored.len() as u64).to_le_bytes());
            index.extend_from_slice(&(payload.len() as u64).to_le_bytes());
            index.extend_from_slice(&name_len.to_le_bytes());
            index.extend_from_slice(section.name.as_bytes());
            payload.extend_from_slice(&stored);
        }
        let index_len =
            u32::try_from(index.len()).map_err(|_| MndError::IndexTooLong(index.len()))?;
        let mut out = Vec::with_capacity(20 + index.len() + payload.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&self.schema_version.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes()); // flags
        out.extend_from_slice(&0_u16.to_le_bytes()); // reserved
        out.extend_from_slice(&section_count.to_le_bytes());
        out.extend_from_slice(&index_len.to_le_bytes());
        out.extend_from_slice(&index);
        out.extend_from_slice(&payload);
        Ok(out)
    }

    /// 从 `.mnd` 字节解析。
    ///
    /// # Errors
    /// 魔数/版本/索引/偏移/压缩数据不合法。
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MndError> {
        let mut cursor = Cursor::new(bytes);
        if cursor.take(4)? != MAGIC {
            return Err(MndError::BadMagic);
        }
        let format_version = cursor.u16()?;
        if !SUPPORTED_FORMAT_VERSIONS.contains(&format_version) {
            return Err(MndError::UnsupportedVersion(format_version));
        }
        let schema_version = cursor.u16()?;
        let _flags = cursor.u16()?;
        let _reserved = cursor.u16()?;
        let section_count =
            usize::try_from(cursor.u32()?).map_err(|_| MndError::TooManySections(usize::MAX))?;
        let index_len =
            usize::try_from(cursor.u32()?).map_err(|_| MndError::IndexTooLong(usize::MAX))?;
        let index_start = cursor.position();
        let index_end = index_start
            .checked_add(index_len)
            .ok_or(MndError::IndexTooLong(index_len))?;
        if index_end > bytes.len() {
            return Err(MndError::Truncated("index"));
        }
        let payload_start = index_end;
        let payload = &bytes[payload_start..];
        let mut sections = Vec::with_capacity(section_count);
        let mut index_cursor = Cursor::new(&bytes[index_start..index_end]);
        for _ in 0..section_count {
            let tag = index_cursor.u16()?;
            let codec = Codec::from_u8(index_cursor.u8()?)?;
            let _flags = index_cursor.u8()?;
            let raw_len = usize::try_from(index_cursor.u64()?)
                .map_err(|_| MndError::TooManyBytes(usize::MAX))?;
            let stored_len = usize::try_from(index_cursor.u64()?)
                .map_err(|_| MndError::TooManyBytes(usize::MAX))?;
            let offset = usize::try_from(index_cursor.u64()?)
                .map_err(|_| MndError::TooManyBytes(usize::MAX))?;
            let name_len = usize::from(index_cursor.u16()?);
            let name = String::from_utf8(index_cursor.take(name_len)?.to_vec())
                .map_err(|_| MndError::NameNotUtf8)?;
            let end = offset
                .checked_add(stored_len)
                .ok_or(MndError::BadOffset(offset))?;
            if end > payload.len() {
                return Err(MndError::Truncated("payload"));
            }
            let stored = &payload[offset..end];
            let data = match codec {
                Codec::Raw => stored.to_vec(),
                Codec::Zlib => unzlib(stored, raw_len)?,
            };
            sections.push(MndSection { tag, name, data });
        }
        Ok(Self {
            format_version,
            schema_version,
            sections,
        })
    }

    /// 读文件。
    ///
    /// # Errors
    /// IO 或格式错误。
    pub fn read_from(path: impl AsRef<Path>) -> Result<Self, MndError> {
        let bytes = std::fs::read(path).map_err(MndError::Io)?;
        Self::from_bytes(&bytes)
    }

    /// 写文件。
    ///
    /// # Errors
    /// IO 或格式错误。
    pub fn write_to(&self, path: impl AsRef<Path>) -> Result<(), MndError> {
        let bytes = self.to_bytes()?;
        std::fs::write(path, bytes).map_err(MndError::Io)
    }
}

fn zlib(data: &[u8]) -> Result<Vec<u8>, MndError> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    std::io::Write::write_all(&mut encoder, data).map_err(MndError::Io)?;
    encoder.finish().map_err(MndError::Io)
}

fn unzlib(data: &[u8], expected: usize) -> Result<Vec<u8>, MndError> {
    let mut decoder = ZlibDecoder::new(data);
    let mut out = Vec::with_capacity(expected.min(64 * 1024 * 1024));
    decoder.read_to_end(&mut out).map_err(MndError::Io)?;
    if out.len() != expected {
        return Err(MndError::LengthMismatch {
            expected,
            actual: out.len(),
        });
    }
    Ok(out)
}

/// `.mnd` 错误。
#[derive(Debug, thiserror::Error)]
pub enum MndError {
    /// IO 错误。
    #[error("IO：{0}")]
    Io(#[source] std::io::Error),
    /// 魔数不对。
    #[error("bad magic（期望 MND1）")]
    BadMagic,
    /// 容器版本不支持。
    #[error("不支持的 .mnd 容器版本 {0}")]
    UnsupportedVersion(u16),
    /// 段编码不支持。
    #[error("未知段编码 {0}")]
    UnknownCodec(u8),
    /// 段数过多。
    #[error("段数过多：{0}")]
    TooManySections(usize),
    /// 索引过长。
    #[error("索引过长：{0} 字节")]
    IndexTooLong(usize),
    /// 段名过长。
    #[error("段名过长：{0}")]
    NameTooLong(String),
    /// 段名不是 UTF-8。
    #[error("段名不是 UTF-8")]
    NameNotUtf8,
    /// 文件被截断。
    #[error("文件被截断（{0}）")]
    Truncated(&'static str),
    /// 段偏移越界。
    #[error("段偏移越界：{0}")]
    BadOffset(usize),
    /// 单段字节数超出 usize。
    #[error("段字节数超出平台限制：{0}")]
    TooManyBytes(usize),
    /// 解压后长度与索引不符。
    #[error("解压长度不符：期望 {expected}，实际 {actual}")]
    LengthMismatch {
        /// 索引里的长度。
        expected: usize,
        /// 实际长度。
        actual: usize,
    },
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    const fn position(&self) -> usize {
        self.offset
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], MndError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(MndError::Truncated("cursor"))?;
        if end > self.bytes.len() {
            return Err(MndError::Truncated("cursor"));
        }
        let slice = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, MndError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, MndError> {
        let bytes: [u8; 2] = self.take(2)?.try_into().expect("2 bytes");
        Ok(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, MndError> {
        let bytes: [u8; 4] = self.take(4)?.try_into().expect("4 bytes");
        Ok(u32::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, MndError> {
        let bytes: [u8; 8] = self.take(8)?.try_into().expect("8 bytes");
        Ok(u64::from_le_bytes(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_sections_and_compression() {
        let mut file = MndFile::new();
        file.push(MndSection::new(TAG_DB, "db", vec![1, 2, 3, 4]));
        file.push(MndSection::new(
            TAG_ENVIR,
            "Envir/NPCs/Test.txt",
            "你好".as_bytes().to_vec(),
        ));
        let bytes = file.to_bytes().expect("encode");
        assert_eq!(&bytes[..4], MAGIC);
        let back = MndFile::from_bytes(&bytes).expect("decode");
        assert_eq!(back.sections.len(), 2);
        assert_eq!(back.get("db").expect("db").data, vec![1, 2, 3, 4]);
        assert_eq!(
            back.get("Envir/NPCs/Test.txt").expect("script").data,
            "你好".as_bytes()
        );
    }

    #[test]
    fn rejects_bad_magic_and_truncation() {
        assert!(matches!(
            MndFile::from_bytes(b"NOPE"),
            Err(MndError::BadMagic)
        ));
        let file = MndFile::new();
        let bytes = file.to_bytes().expect("encode");
        assert!(MndFile::from_bytes(&bytes[..bytes.len() - 1]).is_err());
    }
}
