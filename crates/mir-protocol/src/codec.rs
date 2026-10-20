//! 组帧、拆帧与 gzip 压缩，行为对齐 Crystal `Shared/Packet.cs`。

use std::borrow::Cow;
use std::io::{Read, Write};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;

use crate::{MAX_PACKET_LEN, ProtocolError};

/// 包头长度：`u16` 长度 + `i16` 编号。
pub const HEADER_LEN: usize = 4;

/// 把负载封装成完整的一帧（含包头，必要时先 gzip）。
///
/// `Crystal` 的长度字段是**整帧**（含 4 字节头）的长度。
#[must_use]
pub fn frame(index: i16, compressed: bool, body: Vec<u8>) -> Vec<u8> {
    let payload = if compressed { gzip(&body) } else { body };
    let total = HEADER_LEN + payload.len();
    let mut out = Vec::with_capacity(total);
    // `u16` 长度：超过上限时截断到最大值由 `FrameBuffer` 侧报错；
    // 这里的包都是小包，调用方在发送前会做长度检查。
    out.extend_from_slice(&(total as u16).to_le_bytes());
    out.extend_from_slice(&index.to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

/// 帧是否超过 `u16` 上限。
#[must_use]
pub fn exceeds_limit(body_len: usize) -> bool {
    HEADER_LEN + body_len > MAX_PACKET_LEN
}

/// 按需要解压负载：只有标了 `Compressed` 的包才走 gzip。
pub fn decompress_if(compressed: bool, payload: &[u8]) -> Result<Cow<'_, [u8]>, ProtocolError> {
    if compressed {
        return Ok(Cow::Owned(gunzip(payload)?));
    }
    Ok(Cow::Borrowed(payload))
}

/// gzip 压缩（与 Crystal `Functions.CompressBytes` 一致）。
#[must_use]
pub fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    // 写入 `Vec` 不会失败；失败时退化为原文，避免在无错路径上引入 panic。
    if encoder.write_all(data).is_err() {
        return data.to_vec();
    }
    encoder.finish().unwrap_or_else(|_| data.to_vec())
}

/// gzip 解压（与 Crystal `Functions.DecompressBytes` 一致）。
///
/// Crystal 读压缩包时切的是 `rawBytes[4..(length - 4)]`（左闭右开），
/// 于是末尾 4 字节的 gzip 尾部（ISIZE）被丢掉了。为了既能解自己发的包，
/// 又能解 Crystal 客户端发来的包，这里先按完整 gzip 解一次，
/// 失败再按 Crystal 的截断方式重试。
pub fn gunzip(data: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    if let Ok(out) = try_gunzip(data) {
        return Ok(out);
    }
    if data.len() > 8 {
        let truncated = &data[..data.len() - 4];
        if let Ok(out) = try_gunzip(truncated) {
            return Ok(out);
        }
    }
    Err(ProtocolError::Decompress("gzip 数据损坏".to_owned()))
}

fn try_gunzip(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut decoder = GzDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

/// 把 TCP 收到的字节流切成一个个 `(编号, 负载)`。
#[derive(Debug, Default)]
pub struct FrameBuffer {
    buffer: Vec<u8>,
}

/// 校验帧长度字段（含 4 字节头）；供 `next_frame` 与边界测试复用。
pub fn validate_frame_length(length: usize) -> Result<(), ProtocolError> {
    if !(HEADER_LEN..=MAX_PACKET_LEN).contains(&length) {
        return Err(ProtocolError::InvalidFrameLength {
            length,
            limit: MAX_PACKET_LEN,
        });
    }
    Ok(())
}

impl FrameBuffer {
    /// 新建空缓冲。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加收到的字节。
    pub fn push(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// 当前缓冲字节数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// 缓冲是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// 取出下一帧；数据不足时返回 `Ok(None)`。
    pub fn next_frame(&mut self) -> Result<Option<(i16, Vec<u8>)>, ProtocolError> {
        if self.buffer.len() < HEADER_LEN {
            return Ok(None);
        }
        let length = usize::from(u16::from_le_bytes([self.buffer[0], self.buffer[1]]));
        validate_frame_length(length)?;
        if self.buffer.len() < length {
            return Ok(None);
        }
        let index = i16::from_le_bytes([self.buffer[2], self.buffer[3]]);
        let payload = self.buffer[HEADER_LEN..length].to_vec();
        self.buffer.drain(..length);
        Ok(Some((index, payload)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_layout_matches_crystal() {
        let bytes = frame(5, false, vec![0xAA, 0xBB]);
        // 长度 = 4 头 + 2 负载
        assert_eq!(bytes[0..2], 6u16.to_le_bytes());
        assert_eq!(bytes[2..4], 5i16.to_le_bytes());
        assert_eq!(&bytes[4..], &[0xAA, 0xBB]);
    }

    #[test]
    fn gzip_roundtrip() {
        let data = b"hello mir".to_vec();
        let compressed = gzip(&data);
        assert_ne!(compressed, data);
        assert_eq!(gunzip(&compressed).unwrap(), data);
    }

    #[test]
    fn frame_buffer_handles_partial_reads() {
        let bytes = frame(7, false, vec![1, 2, 3, 4]);
        let mut buffer = FrameBuffer::new();
        buffer.push(&bytes[..3]);
        assert!(buffer.next_frame().unwrap().is_none());
        buffer.push(&bytes[3..]);
        let (index, payload) = buffer.next_frame().unwrap().unwrap();
        assert_eq!(index, 7);
        assert_eq!(payload, vec![1, 2, 3, 4]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn frame_buffer_rejects_short_length() {
        let mut buffer = FrameBuffer::new();
        buffer.push(&[1, 0, 5, 0]);
        assert!(buffer.next_frame().is_err());
    }

    #[test]
    fn multiple_frames_in_one_push() {
        let mut bytes = frame(1, false, vec![9]);
        bytes.extend_from_slice(&frame(2, false, vec![8, 7]));
        let mut buffer = FrameBuffer::new();
        buffer.push(&bytes);
        assert_eq!(buffer.next_frame().unwrap().unwrap(), (1, vec![9]));
        assert_eq!(buffer.next_frame().unwrap().unwrap(), (2, vec![8, 7]));
        assert!(buffer.next_frame().unwrap().is_none());
    }
}
