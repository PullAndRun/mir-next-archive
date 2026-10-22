//! `.mna`（Mir Next Audio）容器读写。
//!
//! 发布期的音频是"压缩流 + 自描述头"，替代 `resource/wav/*.wav` 的 PCM。
//! 编码器在构建期由 `mir-tools audio import` 调 ffmpeg 完成；运行期只需要解码，
//! Vorbis 用纯 Rust 的 `lewton`（`rodio` 的 `vorbis` feature）。
//!
//! ```text
//! 0x00  magic        "MNA1"            4B
//! 0x04  version      u16 = 1           2B
//! 0x06  codec        u8（0=flac，1=vorbis）
//! 0x07  channels     u8
//! 0x08  sample_rate  u32               4B
//! 0x0C  frames       u64               8B   原始 PCM 帧数（时长对账用）
//! 0x14  data_len     u32               4B
//! 0x18  data         …                 压缩音频字节（ogg/flac 容器）
//! 文件尾 - 32        sha256(前面全部字节)
//! ```

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::error::AssetError;

/// 音频容器魔数。
pub const MAGIC: [u8; 4] = *b"MNA1";
/// 容器版本。
pub const VERSION: u16 = 1;
/// 头部长度。
pub const HEADER_LEN: u64 = 24;
/// 包尾 sha256 长度。
const TRAILER_LEN: u64 = 32;

/// 压缩编码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    /// 无损 FLAC。
    Flac,
    /// 有损 Vorbis（默认，体积约为 PCM 的 11%）。
    Vorbis,
}

impl AudioCodec {
    /// 从字节解析（未知值返回 `None`）。
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Flac),
            1 => Some(Self::Vorbis),
            _ => None,
        }
    }

    /// 写入容器的编码号。
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Flac => 0,
            Self::Vorbis => 1,
        }
    }

    /// 命令行写法。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Flac => "flac",
            Self::Vorbis => "vorbis",
        }
    }

    /// 解析命令行写法（`flac`/`vorbis`，大小写不敏感）。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "flac" => Some(Self::Flac),
            "vorbis" | "ogg" => Some(Self::Vorbis),
            _ => None,
        }
    }
}

/// 音频元数据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioMeta {
    /// 编码。
    pub codec: AudioCodec,
    /// 声道数。
    pub channels: u8,
    /// 采样率。
    pub sample_rate: u32,
    /// 原始 PCM 帧数（校验时长）。
    pub frames: u64,
}

/// 一段音频（元数据 + 压缩数据）。
#[derive(Debug, Clone)]
pub struct AudioClip {
    /// 元数据。
    pub meta: AudioMeta,
    /// 压缩后的音频字节（ogg / flac 容器）。
    pub data: Vec<u8>,
}

impl AudioClip {
    /// 读一个 `.mna`（校验包尾 sha256）。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AssetError> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path).map_err(|source| AssetError::Io {
            path: path.clone(),
            source,
        })?;
        let total = file
            .metadata()
            .map_err(|source| AssetError::Io {
                path: path.clone(),
                source,
            })?
            .len();
        if total < HEADER_LEN + TRAILER_LEN {
            return Err(AssetError::Corrupt {
                path,
                offset: 0,
                message: "文件太短，不是 .mna".to_owned(),
            });
        }
        let mut header = [0u8; HEADER_LEN as usize];
        read_at(&mut file, &path, 0, &mut header)?;
        if header[0..4] != MAGIC {
            return Err(AssetError::Corrupt {
                path: path.clone(),
                offset: 0,
                message: format!("不是 .mna（magic={:?}）", &header[0..4]),
            });
        }
        let version = u16::from_le_bytes([header[4], header[5]]);
        if version != VERSION {
            return Err(AssetError::Corrupt {
                path: path.clone(),
                offset: 4,
                message: format!(".mna 版本 {version} 不受支持（本实现只认 {VERSION}）"),
            });
        }
        let Some(codec) = AudioCodec::from_u8(header[6]) else {
            return Err(AssetError::Corrupt {
                path: path.clone(),
                offset: 6,
                message: format!("未知音频编码 {}", header[6]),
            });
        };
        let channels = header[7];
        let sample_rate = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
        let frames = u64::from_le_bytes([
            header[12], header[13], header[14], header[15], header[16], header[17], header[18],
            header[19],
        ]);
        let data_len = u32::from_le_bytes([header[20], header[21], header[22], header[23]]);
        if u64::from(data_len) + HEADER_LEN + TRAILER_LEN != total {
            return Err(AssetError::Corrupt {
                path: path.clone(),
                offset: 20,
                message: format!(
                    "data_len={data_len} 与文件大小 {total} 不一致（应为 data_len={}）",
                    total - HEADER_LEN - TRAILER_LEN
                ),
            });
        }
        let mut trailer = [0u8; TRAILER_LEN as usize];
        read_at(&mut file, &path, total - TRAILER_LEN, &mut trailer)?;
        let body_digest = sha256_range(&path, total - TRAILER_LEN)?;
        if trailer != body_digest {
            return Err(AssetError::Corrupt {
                path: path.clone(),
                offset: total - TRAILER_LEN,
                message: "包尾 sha256 与内容不符（文件被改动过）".to_owned(),
            });
        }
        let mut data = vec![0u8; data_len as usize];
        read_at(&mut file, &path, HEADER_LEN, &mut data)?;
        Ok(Self {
            meta: AudioMeta {
                codec,
                channels,
                sample_rate,
                frames,
            },
            data,
        })
    }

    /// 写一个 `.mna`（追加包尾 sha256）。
    pub fn write(path: impl AsRef<Path>, meta: AudioMeta, data: &[u8]) -> Result<(), AssetError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|source| AssetError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
        }
        let data_len = u32::try_from(data.len()).map_err(|_| AssetError::Corrupt {
            path: path.clone(),
            offset: 0,
            message: "音频数据超过 4GiB".to_owned(),
        })?;
        let mut file = File::create(&path).map_err(|source| AssetError::Io {
            path: path.clone(),
            source,
        })?;
        let mut header = Vec::with_capacity(HEADER_LEN as usize);
        header.extend_from_slice(&MAGIC);
        header.extend_from_slice(&VERSION.to_le_bytes());
        header.push(meta.codec.as_u8());
        header.push(meta.channels);
        header.extend_from_slice(&meta.sample_rate.to_le_bytes());
        header.extend_from_slice(&meta.frames.to_le_bytes());
        header.extend_from_slice(&data_len.to_le_bytes());
        file.write_all(&header).map_err(|source| AssetError::Io {
            path: path.clone(),
            source,
        })?;
        file.write_all(data).map_err(|source| AssetError::Io {
            path: path.clone(),
            source,
        })?;
        file.flush().map_err(|source| AssetError::Io {
            path: path.clone(),
            source,
        })?;
        let digest = sha256_range(&path, HEADER_LEN + u64::from(data_len))?;
        file.write_all(&digest).map_err(|source| AssetError::Io {
            path: path.clone(),
            source,
        })?;
        file.flush().map_err(|source| AssetError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(())
    }

    /// 时长（秒，仅用于展示；精度损失可接受）。
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn seconds(&self) -> f64 {
        if self.meta.sample_rate == 0 {
            return 0.0;
        }
        self.meta.frames as f64 / f64::from(self.meta.sample_rate)
    }
}

fn read_at(file: &mut File, path: &Path, offset: u64, buffer: &mut [u8]) -> Result<(), AssetError> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|source| AssetError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.read_exact(buffer).map_err(|source| AssetError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn sha256_range(path: &Path, len: u64) -> Result<[u8; 32], AssetError> {
    use sha2::Digest as _;
    let mut file = File::open(path).map_err(|source| AssetError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    let mut remaining = len;
    while remaining > 0 {
        let want = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        file.read_exact(&mut buffer[..want])
            .map_err(|source| AssetError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        hasher.update(&buffer[..want]);
        remaining -= want as u64;
    }
    Ok(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mir2-mna-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn mna_round_trips_meta_and_bytes() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("1.mna");
        let meta = AudioMeta {
            codec: AudioCodec::Vorbis,
            channels: 2,
            sample_rate: 44_100,
            frames: 22_050,
        };
        let payload = vec![0x5Au8; 4096];
        AudioClip::write(&path, meta, &payload).expect("write");
        let clip = AudioClip::open(&path).expect("open");
        assert_eq!(clip.meta, meta);
        assert_eq!(clip.data, payload);
        assert!((clip.seconds() - 0.5).abs() < 1e-9);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tampered_mna_is_rejected() {
        let dir = temp_dir("tamper");
        let path = dir.join("2.mna");
        let meta = AudioMeta {
            codec: AudioCodec::Flac,
            channels: 1,
            sample_rate: 16_000,
            frames: 800,
        };
        AudioClip::write(&path, meta, &[1u8; 1024]).expect("write");
        let mut bytes = std::fs::read(&path).expect("read");
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0xFF;
        std::fs::write(&path, &bytes).expect("write");
        let error = AudioClip::open(&path).expect_err("tampered must fail");
        assert!(format!("{error}").contains("sha256"), "{error}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
