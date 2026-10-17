//! 《传奇-Next》网络协议。
//!
//! 线格式与 Crystal（`Shared/Packet.cs`）逐字节对齐：
//!
//! ```text
//! +--------+--------+------------------+
//! | u16 长度 | i16 编号 | 负载（可变长）      |
//! +--------+--------+------------------+
//! ```
//!
//! * 长度**包含** 4 字节头；编号是有符号 16 位，小端。
//! * 字符串使用 .NET `BinaryWriter` 的 7 位长度前缀 + UTF-8。
//! * 标了 `Compressed` 的包，其负载是 gzip 压缩后的字节。
//! * 没有任何加密层（Crystal 在 `MirConnection` 里直接读写明文）。
//!
//! 包结构本身由 `tools/protocol-gen/generate.py` 从 Crystal 的 C# 源码生成：
//! 每个包的字段顺序、枚举底层类型与压缩标记都来自源码，不是手工抄写。

pub mod codec;
pub mod data;
pub mod manual;
pub mod packets;

pub use mir_core::enums::*;
pub use mir_core::{Color, DateTimeBinary, Point};
pub use packets::client::ClientPacket;
pub use packets::server::ServerPacket;

/// 协议层的错误。
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    /// 负载提前结束。
    #[error("负载不完整：需要 {needed} 字节，只剩 {remaining} 字节")]
    UnexpectedEof {
        /// 还需要多少字节。
        needed: usize,
        /// 实际剩余多少字节。
        remaining: usize,
    },
    /// 收到未知的包编号。
    #[error("未知的包编号 {index}")]
    UnknownPacket {
        /// 包编号。
        index: i16,
    },
    /// 长度前缀声称的集合长度超出上限。
    #[error("集合长度 {length} 超出上限 {limit}")]
    CollectionTooLong {
        /// 声称的长度。
        length: usize,
        /// 允许的上限。
        limit: usize,
    },
    /// 字符串不是合法 UTF-8。
    #[error("字符串不是合法 UTF-8")]
    InvalidString,
    /// 字符串长度前缀非法。
    #[error("字符串长度前缀非法")]
    InvalidStringLength,
    /// gzip 解压失败。
    #[error("gzip 解压失败：{0}")]
    Decompress(String),
    /// 包长度字段非法。
    #[error("包长度 {length} 非法（上限 {limit}）")]
    InvalidFrameLength {
        /// 头里声明的长度。
        length: usize,
        /// 允许的上限。
        limit: usize,
    },
}

/// 集合长度上限：防止一个损坏/恶意的长度前缀导致巨额分配。
pub const MAX_COLLECTION_LEN: usize = 1 << 20;

/// 单个包（含头）的最大长度：长度字段是 `u16`。
pub const MAX_PACKET_LEN: usize = u16::MAX as usize;

/// 从字节缓冲区读取 Crystal 的各类原语。
#[derive(Debug)]
pub struct PacketReader<'a> {
    buffer: &'a [u8],
    position: usize,
}

impl<'a> PacketReader<'a> {
    /// 在给定负载上创建读取器。
    #[must_use]
    pub const fn new(buffer: &'a [u8]) -> Self {
        Self {
            buffer,
            position: 0,
        }
    }

    /// 已消耗的字节数。
    #[must_use]
    pub const fn position(&self) -> usize {
        self.position
    }

    /// 剩余未读字节数。
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.buffer.len() - self.position
    }

    /// 是否已读完。
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], ProtocolError> {
        if self.remaining() < len {
            return Err(ProtocolError::UnexpectedEof {
                needed: len,
                remaining: self.remaining(),
            });
        }
        let slice = &self.buffer[self.position..self.position + len];
        self.position += len;
        Ok(slice)
    }

    /// 读取 `u8`。
    pub fn read_u8(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.take(1)?[0])
    }

    /// 读取 `i8`。
    pub fn read_i8(&mut self) -> Result<i8, ProtocolError> {
        Ok(self.read_u8()? as i8)
    }

    /// 读取 `bool`（1 字节，非 0 即真）。
    pub fn read_bool(&mut self) -> Result<bool, ProtocolError> {
        Ok(self.read_u8()? != 0)
    }

    /// 读取 `u16`（小端）。
    pub fn read_u16(&mut self) -> Result<u16, ProtocolError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// 读取 `i16`（小端）。
    pub fn read_i16(&mut self) -> Result<i16, ProtocolError> {
        Ok(self.read_u16()? as i16)
    }

    /// 读取 `u32`（小端）。
    pub fn read_u32(&mut self) -> Result<u32, ProtocolError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// 读取 `i32`（小端）。
    pub fn read_i32(&mut self) -> Result<i32, ProtocolError> {
        Ok(self.read_u32()? as i32)
    }

    /// 读取 `u64`（小端）。
    pub fn read_u64(&mut self) -> Result<u64, ProtocolError> {
        let bytes = self.take(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// 读取 `i64`（小端）。
    pub fn read_i64(&mut self) -> Result<i64, ProtocolError> {
        Ok(self.read_u64()? as i64)
    }

    /// 读取 `f32`（小端）。
    pub fn read_f32(&mut self) -> Result<f32, ProtocolError> {
        Ok(f32::from_bits(self.read_u32()?))
    }

    /// 读取 `f64`（小端）。
    pub fn read_f64(&mut self) -> Result<f64, ProtocolError> {
        Ok(f64::from_bits(self.read_u64()?))
    }

    /// 读取一个 7 位长度前缀的 UTF-8 字符串（.NET `BinaryWriter.Write(string)`）。
    pub fn read_string(&mut self) -> Result<String, ProtocolError> {
        let mut length: usize = 0;
        let mut shift = 0u32;
        loop {
            let byte = self.read_u8()?;
            length |= usize::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift >= 35 {
                return Err(ProtocolError::InvalidStringLength);
            }
        }
        if length > MAX_COLLECTION_LEN {
            return Err(ProtocolError::CollectionTooLong {
                length,
                limit: MAX_COLLECTION_LEN,
            });
        }
        let bytes = self.take(length)?;
        core::str::from_utf8(bytes)
            .map(ToOwned::to_owned)
            .map_err(|_| ProtocolError::InvalidString)
    }

    /// 读取 `reader.ReadBytes(reader.ReadInt32())`：i32 长度前缀 + 原始字节。
    pub fn read_bytes_prefixed(&mut self) -> Result<Vec<u8>, ProtocolError> {
        let length = self.read_count()?;
        Ok(self.take(length)?.to_vec())
    }

    /// 读取集合长度（i32），并校验非负与上限。
    pub fn read_count(&mut self) -> Result<usize, ProtocolError> {
        let raw = self.read_i32()?;
        if raw < 0 {
            return Err(ProtocolError::CollectionTooLong {
                length: 0,
                limit: MAX_COLLECTION_LEN,
            });
        }
        let length = raw as usize;
        if length > MAX_COLLECTION_LEN {
            return Err(ProtocolError::CollectionTooLong {
                length,
                limit: MAX_COLLECTION_LEN,
            });
        }
        Ok(length)
    }

    /// 读取剩余的全部字节。
    pub fn read_rest(&mut self) -> &'a [u8] {
        let rest = &self.buffer[self.position..];
        self.position = self.buffer.len();
        rest
    }
}

/// 按 Crystal 的写入顺序拼装负载。
#[derive(Debug, Default)]
pub struct PacketWriter {
    buffer: Vec<u8>,
}

impl PacketWriter {
    /// 新建写入器。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 预分配容量。
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
        }
    }

    /// 取出内部缓冲区。
    #[must_use]
    pub fn into_inner(self) -> Vec<u8> {
        self.buffer
    }

    /// 当前负载长度。
    #[must_use]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// 是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// 写入 `u8`。
    pub fn write_u8(&mut self, value: u8) {
        self.buffer.push(value);
    }

    /// 写入 `i8`。
    pub fn write_i8(&mut self, value: i8) {
        self.buffer.push(value as u8);
    }

    /// 写入 `bool`（0 或 1）。
    pub fn write_bool(&mut self, value: bool) {
        self.buffer.push(u8::from(value));
    }

    /// 写入 `u16`（小端）。
    pub fn write_u16(&mut self, value: u16) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// 写入 `i16`（小端）。
    pub fn write_i16(&mut self, value: i16) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// 写入 `u32`（小端）。
    pub fn write_u32(&mut self, value: u32) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// 写入 `i32`（小端）。
    pub fn write_i32(&mut self, value: i32) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// 写入 `u64`（小端）。
    pub fn write_u64(&mut self, value: u64) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// 写入 `i64`（小端）。
    pub fn write_i64(&mut self, value: i64) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// 写入 `f32`（小端）。
    pub fn write_f32(&mut self, value: f32) {
        self.write_u32(value.to_bits());
    }

    /// 写入 `f64`（小端）。
    pub fn write_f64(&mut self, value: f64) {
        self.write_u64(value.to_bits());
    }

    /// 写入 7 位长度前缀的 UTF-8 字符串。
    pub fn write_string(&mut self, value: &str) {
        self.write_7bit_length(value.len());
        self.buffer.extend_from_slice(value.as_bytes());
    }

    /// 写入 `i32` 长度前缀 + 原始字节。
    pub fn write_bytes_prefixed(&mut self, value: &[u8]) {
        self.write_i32(value.len() as i32);
        self.buffer.extend_from_slice(value);
    }

    /// 写入集合长度。
    pub fn write_count(&mut self, value: usize) {
        self.write_i32(value as i32);
    }

    fn write_7bit_length(&mut self, mut length: usize) {
        loop {
            let mut byte = (length & 0x7F) as u8;
            length >>= 7;
            if length != 0 {
                byte |= 0x80;
            }
            self.buffer.push(byte);
            if length == 0 {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_roundtrip_matches_dotnet_prefix() {
        let mut writer = PacketWriter::new();
        writer.write_string("Mir");
        // .NET: 单字节长度前缀，再是 UTF-8。
        assert_eq!(writer.into_inner(), vec![3, b'M', b'i', b'r']);
    }

    #[test]
    fn long_string_uses_seven_bit_prefix() {
        let text = "x".repeat(200);
        let mut writer = PacketWriter::new();
        writer.write_string(&text);
        let bytes = writer.into_inner();
        // 200 = 0b1100_1000 -> 前缀两字节 0xC8 0x01
        assert_eq!(&bytes[..2], &[0xC8, 0x01]);
        let mut reader = PacketReader::new(&bytes);
        assert_eq!(reader.read_string().unwrap(), text);
    }

    #[test]
    fn primitives_are_little_endian() {
        let mut writer = PacketWriter::new();
        writer.write_u16(0x0102);
        writer.write_u32(0x0304_0506);
        assert_eq!(
            writer.into_inner(),
            vec![0x02, 0x01, 0x06, 0x05, 0x04, 0x03]
        );
    }

    #[test]
    fn reading_past_end_errors() {
        let mut reader = PacketReader::new(&[1, 2]);
        assert!(reader.read_u32().is_err());
        assert_eq!(reader.remaining(), 2);
    }
}
