//! WIL/WIX（WEMADE 旧版 `#ILIB`/`#INDX` 素材库，16 位 RGB565）。
//!
//! `resource/data` 里除 `.wzl` 外还有一批 `.wil` + `.wix`（`Objects81.wil`、
//! `Tiles110.wil` …，来自旧版 Mir3/WeMade 客户端）。格式按语料实测：
//!
//! ```text
//! WIX（索引）：
//!   +0   36 字节签名 "#INDX v1.0-WEMADE Entertainment inc."
//!   +36  16 字节头（4 个 u32；语料值 219 / 15024080 / 865 / 1080，本实现不使用）
//!   +52  偏移表：每项 u32，图像数据在 WIL 里的绝对偏移（严格递增）
//!
//! WIL（数据）：
//!   +0   36 字节签名 "#ILIB v1.0-WEMADE Entertainment inc."
//!   +36  24 字节头（含 1024 字节调色板前的字段；本实现只校验签名）
//!   每张图（偏移表指向的位置）：
//!         u16 width, u16 height, i16 offset_x, i16 offset_y,
//!         然后 width×height 个 u16（RGB565 小端，0x0000 = 透明）
//! ```
//!
//! 相对 `.wzl` 的差异：**没有 zlib**、**没有逐图长度字段**（长度 = 下一张偏移 −
//! 当前偏移，最后一张 = WIL 文件长 − 偏移），像素是 RGB565 而不是 8 位调色板。
//!
//! 偏移表项数 = `(WIX 长度 − 52) / 4`（语料里 WIX 头声明的数量与实际项数不一致，
//! 与 `.wzx` 的情况类似，所以按实际长度算）。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::AssetError;
use crate::image::{Image, ImageInfo, format};

/// WIX 签名（36 字节，不带结尾 NUL）。
const WIX_SIGNATURE: &[u8] = b"#INDX v1.0-WEMADE Entertainment inc.";
/// WIL 签名。
const WIL_SIGNATURE: &[u8] = b"#ILIB v1.0-WEMADE Entertainment inc.";
/// WIX 头长度（签名 + 16 字节字段），偏移表从这里开始。
pub(crate) const WIX_HEADER_LEN: u64 = 52;
/// 每张图的头长度（宽/高/偏移 X/偏移 Y）。
const IMAGE_HEADER_LEN: u64 = 8;
/// 单张图的最大解码后字节数（防止畸形索引导致超大分配）。
const MAX_IMAGE_BYTES: u64 = crate::MAX_IMAGE_BYTES;

/// 已打开的 WIL + WIX 库。
#[derive(Debug)]
pub(crate) struct WilReader {
    /// 数据文件（`.wil`）。
    wil_path: PathBuf,
    /// 索引文件（`.wix`）。
    wix_path: PathBuf,
    /// 每张图在 `.wil` 里的绝对偏移。
    offsets: Vec<u32>,
    /// `.wil` 文件长度。
    wil_len: u64,
}

impl WilReader {
    /// 打开 `.wil` 与配套 `.wix`。
    pub(crate) fn open(wil_path: &Path, wix_path: &Path) -> Result<Self, AssetError> {
        let mut wix = File::open(wix_path).map_err(|source| AssetError::Io {
            path: wix_path.to_path_buf(),
            source,
        })?;
        let wix_len = wix
            .metadata()
            .map_err(|source| AssetError::Io {
                path: wix_path.to_path_buf(),
                source,
            })?
            .len();
        if wix_len < WIX_HEADER_LEN {
            return Err(AssetError::Truncated {
                path: wix_path.to_path_buf(),
                offset: 0,
                needed: WIX_HEADER_LEN,
                actual: wix_len,
            });
        }
        let mut header = vec![0u8; usize::try_from(WIX_HEADER_LEN).unwrap_or(52)];
        wix.read_exact(&mut header)
            .map_err(|source| AssetError::Io {
                path: wix_path.to_path_buf(),
                source,
            })?;
        if !header.starts_with(WIX_SIGNATURE) {
            return Err(AssetError::BadMagic {
                path: wix_path.to_path_buf(),
                offset: 0,
                expected: "#INDX v1.0-WEMADE 索引头",
                found: u32::from_le_bytes([header[0], header[1], header[2], header[3]]),
            });
        }
        let count = usize::try_from((wix_len - WIX_HEADER_LEN) / 4).unwrap_or(0);
        let mut raw = vec![0u8; count * 4];
        wix.read_exact(&mut raw).map_err(|source| AssetError::Io {
            path: wix_path.to_path_buf(),
            source,
        })?;
        let offsets: Vec<u32> = raw
            .chunks_exact(4)
            .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            .collect();
        // 偏移必须严格递增（长度由相邻偏移算出）。
        if offsets.windows(2).any(|pair| pair[0] >= pair[1]) {
            let index = offsets
                .windows(2)
                .position(|pair| pair[0] >= pair[1])
                .unwrap_or(0);
            return Err(AssetError::Corrupt {
                path: wix_path.to_path_buf(),
                offset: WIX_HEADER_LEN + u64::try_from(index).unwrap_or(0) * 4,
                message: "WIX 偏移表不是严格递增".to_owned(),
            });
        }
        let wil_len = File::open(wil_path)
            .map_err(|source| AssetError::Io {
                path: wil_path.to_path_buf(),
                source,
            })?
            .metadata()
            .map_err(|source| AssetError::Io {
                path: wil_path.to_path_buf(),
                source,
            })?
            .len();
        // 数据文件也校验签名（扩展名大写/小写混用时不靠文件名判断）。
        let mut wil = File::open(wil_path).map_err(|source| AssetError::Io {
            path: wil_path.to_path_buf(),
            source,
        })?;
        let mut wil_head = [0u8; 12];
        wil.read_exact(&mut wil_head)
            .map_err(|_| AssetError::Truncated {
                path: wil_path.to_path_buf(),
                offset: 0,
                needed: 12,
                actual: wil_len,
            })?;
        if !wil_head.starts_with(&WIL_SIGNATURE[..12]) {
            return Err(AssetError::BadMagic {
                path: wil_path.to_path_buf(),
                offset: 0,
                expected: "#ILIB v1.0-WEMADE 数据头",
                found: u32::from_le_bytes([wil_head[0], wil_head[1], wil_head[2], wil_head[3]]),
            });
        }
        if offsets
            .last()
            .is_some_and(|offset| u64::from(*offset) > wil_len)
        {
            return Err(AssetError::Corrupt {
                path: wix_path.to_path_buf(),
                offset: WIX_HEADER_LEN,
                message: "偏移超出 WIL 文件长度".to_owned(),
            });
        }
        Ok(Self {
            wil_path: wil_path.to_path_buf(),
            wix_path: wix_path.to_path_buf(),
            offsets,
            wil_len,
        })
    }

    /// 图像数量。
    pub(crate) fn len(&self) -> usize {
        self.offsets.len()
    }

    /// 索引文件路径。
    pub(crate) fn index_path(&self) -> &Path {
        &self.wix_path
    }

    /// 某张图的数据长度（到下一张偏移或文件尾）。
    fn image_len(&self, index: usize) -> u64 {
        let start = u64::from(self.offsets[index]);
        let end = self
            .offsets
            .get(index + 1)
            .map_or(self.wil_len, |offset| u64::from(*offset));
        end.saturating_sub(start)
    }

    /// 只读头信息。
    pub(crate) fn info(&self, index: usize) -> Result<ImageInfo, AssetError> {
        let Some(offset) = self.offsets.get(index) else {
            return Err(AssetError::IndexOutOfRange {
                index,
                count: self.offsets.len(),
            });
        };
        let mut file = self.open_wil()?;
        file.seek(SeekFrom::Start(u64::from(*offset)))
            .map_err(|source| AssetError::Io {
                path: self.wil_path.clone(),
                source,
            })?;
        let mut header = [0u8; IMAGE_HEADER_LEN as usize];
        file.read_exact(&mut header)
            .map_err(|_| AssetError::Truncated {
                path: self.wil_path.clone(),
                offset: u64::from(*offset),
                needed: IMAGE_HEADER_LEN,
                actual: self.image_len(index),
            })?;
        let mut info = Self::decode_header(header);
        info.data_offset = u64::from(*offset) + IMAGE_HEADER_LEN;
        info.data_len =
            u32::try_from(self.image_len(index).saturating_sub(IMAGE_HEADER_LEN)).unwrap_or(0);
        Ok(info)
    }

    /// 解码一张图为 RGBA8。
    pub(crate) fn image(&self, index: usize) -> Result<Image, AssetError> {
        let Some(offset) = self.offsets.get(index) else {
            return Err(AssetError::IndexOutOfRange {
                index,
                count: self.offsets.len(),
            });
        };
        let offset = u64::from(*offset);
        let available = self.image_len(index);
        if available < IMAGE_HEADER_LEN {
            return Err(AssetError::Truncated {
                path: self.wil_path.clone(),
                offset,
                needed: IMAGE_HEADER_LEN,
                actual: available,
            });
        }
        let mut file = self.open_wil()?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|source| AssetError::Io {
                path: self.wil_path.clone(),
                source,
            })?;
        let mut header = [0u8; IMAGE_HEADER_LEN as usize];
        file.read_exact(&mut header)
            .map_err(|source| AssetError::Io {
                path: self.wil_path.clone(),
                source,
            })?;
        let mut info = Self::decode_header(header);
        info.data_offset = offset + IMAGE_HEADER_LEN;
        info.data_len = u32::try_from(available.saturating_sub(IMAGE_HEADER_LEN)).unwrap_or(0);
        if info.is_empty() {
            return Ok(Image {
                info,
                pixels: Vec::new(),
            });
        }
        let pixel_bytes = u64::from(info.width) * u64::from(info.height) * 2;
        if pixel_bytes > MAX_IMAGE_BYTES {
            return Err(AssetError::ImageTooLarge {
                width: info.width,
                height: info.height,
                needed: pixel_bytes,
                limit: MAX_IMAGE_BYTES,
            });
        }
        if available < IMAGE_HEADER_LEN + pixel_bytes {
            return Err(AssetError::Truncated {
                path: self.wil_path.clone(),
                offset,
                needed: IMAGE_HEADER_LEN + pixel_bytes,
                actual: available,
            });
        }
        let mut raw = vec![0u8; usize::try_from(pixel_bytes).unwrap_or(0)];
        file.read_exact(&mut raw).map_err(|source| AssetError::Io {
            path: self.wil_path.clone(),
            source,
        })?;
        let mut pixels = Vec::with_capacity(raw.len() / 2 * 4);
        for bytes in raw.chunks_exact(2) {
            let value = u16::from_le_bytes([bytes[0], bytes[1]]);
            pixels.extend_from_slice(&expand565(value));
        }
        Ok(Image { info, pixels })
    }

    /// 打开 WIL 数据文件。
    fn open_wil(&self) -> Result<File, AssetError> {
        File::open(&self.wil_path).map_err(|source| AssetError::Io {
            path: self.wil_path.clone(),
            source,
        })
    }

    /// 解析 8 字节图像头。
    fn decode_header(header: [u8; IMAGE_HEADER_LEN as usize]) -> ImageInfo {
        let width = u16::from_le_bytes([header[0], header[1]]);
        let height = u16::from_le_bytes([header[2], header[3]]);
        let offset_x = i16::from_le_bytes([header[4], header[5]]);
        let offset_y = i16::from_le_bytes([header[6], header[7]]);
        ImageInfo {
            width,
            height,
            offset_x,
            offset_y,
            format: if width == 0 || height == 0 {
                format::EMPTY
            } else {
                format::RGB565
            },
            shadow_x: 0,
            shadow_y: 0,
            has_mask: false,
            reserved: [0; 3],
            // 调用方按当前偏移补齐（`data_offset`/`data_len` 依赖索引位置）。
            data_offset: 0,
            data_len: 0,
        }
    }
}

/// RGB565 → RGBA8；`0x0000` 为透明（语料里用 0 表示空像素）。
fn expand565(value: u16) -> [u8; 4] {
    if value == 0 {
        return [0, 0, 0, 0];
    }
    let red = (value >> 11) & 0x1F;
    let green = (value >> 5) & 0x3F;
    let blue = value & 0x1F;
    [replicate5(red), replicate6(green), replicate5(blue), 255]
}

/// 5 位色扩展到 8 位（按位复制，`0x1F → 0xFF`）。
const fn replicate5(value: u16) -> u8 {
    ((value << 3) | (value >> 2)) as u8
}

/// 6 位色扩展到 8 位。
const fn replicate6(value: u16) -> u8 {
    ((value << 2) | (value >> 4)) as u8
}

/// `#[cfg(test)]` 用：判断文件头是不是 WIL/WIX。
pub(crate) fn is_wil_signature(head: &[u8]) -> bool {
    head.starts_with(b"#ILI") || head.starts_with(b"#IND")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// 临时目录（与 `library.rs` 的测试助手同口径）。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let index = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "mir-asset-wil-{}-{tag}-{index}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("创建临时目录");
            Self(path)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    /// 构造一对最小的 WIL/WIX：一张 2×1 的图（红、透明）。
    fn synthetic_pair() -> (TempDir, PathBuf, PathBuf) {
        let dir = TempDir::new("pair");
        let wil_path = dir.join("sample.wil");
        let wix_path = dir.join("sample.wix");
        // WIL：60 字节头 + 图像（偏移 1084，模拟 1024 字节调色板）。
        let mut wil = vec![0u8; 1084];
        wil[..WIL_SIGNATURE.len()].copy_from_slice(WIL_SIGNATURE);
        let pixels: [u16; 2] = [0xF800, 0x0000]; // 纯红 + 透明
        for value in pixels {
            wil.extend_from_slice(&value.to_le_bytes());
        }
        // 图像头：宽 2 高 1，偏移 0,0 —— 写在像素前，所以重新拼一次。
        let mut image = Vec::new();
        image.extend_from_slice(&2u16.to_le_bytes());
        image.extend_from_slice(&1u16.to_le_bytes());
        image.extend_from_slice(&0i16.to_le_bytes());
        image.extend_from_slice(&0i16.to_le_bytes());
        for value in pixels {
            image.extend_from_slice(&value.to_le_bytes());
        }
        wil.truncate(1084);
        wil.extend_from_slice(&image);
        std::fs::write(&wil_path, &wil).expect("写 WIL");

        // WIX：52 字节头 + 1 个偏移。
        let mut wix = vec![0u8; WIX_HEADER_LEN as usize];
        wix[..WIX_SIGNATURE.len()].copy_from_slice(WIX_SIGNATURE);
        wix.extend_from_slice(&1084u32.to_le_bytes());
        std::fs::write(&wix_path, &wix).expect("写 WIX");
        (dir, wil_path, wix_path)
    }

    #[test]
    fn synthetic_wil_decodes_header_and_rgb565() {
        let (_dir, wil, wix) = synthetic_pair();
        let reader = WilReader::open(&wil, &wix).expect("打开 WIL");
        assert_eq!(reader.len(), 1);
        assert_eq!(reader.index_path(), wix.as_path());

        let info = reader.info(0).expect("头信息");
        assert_eq!((info.width, info.height), (2, 1));
        assert_eq!((info.offset_x, info.offset_y), (0, 0));
        assert_eq!(info.format, format::RGB565);

        let image = reader.image(0).expect("解码");
        assert_eq!(image.pixels.len(), 8);
        assert_eq!(&image.pixels[0..4], &[255, 0, 0, 255], "红色");
        assert_eq!(&image.pixels[4..8], &[0, 0, 0, 0], "0x0000 透明");
    }

    #[test]
    fn broken_signature_and_short_index_are_reported() {
        let dir = TempDir::new("bad");
        let wil = dir.join("bad.wil");
        let wix = dir.join("bad.wix");
        std::fs::write(&wil, b"not a wil file").expect("写");
        std::fs::write(&wix, b"short").expect("写");
        let error = WilReader::open(&wil, &wix).expect_err("WIX 太短应当报错");
        assert!(matches!(error, AssetError::Truncated { .. }));

        // 合法头但零偏移：可以打开，只是没有图。
        let mut wix_bytes = vec![0u8; WIX_HEADER_LEN as usize];
        wix_bytes[..WIX_SIGNATURE.len()].copy_from_slice(WIX_SIGNATURE);
        std::fs::write(&wix, &wix_bytes).expect("写");
        let mut wil_bytes = vec![0u8; 1084];
        wil_bytes[..WIL_SIGNATURE.len()].copy_from_slice(WIL_SIGNATURE);
        std::fs::write(&wil, &wil_bytes).expect("写合法 WIL 头");
        let reader = WilReader::open(&wil, &wix).expect("没有偏移的 WIX 可以打开");
        assert_eq!(reader.len(), 0);
        assert!(matches!(
            reader.image(0),
            Err(AssetError::IndexOutOfRange { .. })
        ));
    }

    #[test]
    fn real_objects81_decodes_if_assets_exist() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resource/data");
        let wil = root.join("Objects81.wil");
        let wix = root.join("Objects81.wix");
        if !wil.is_file() || !wix.is_file() {
            eprintln!("跳过：缺 {}", wil.display());
            return;
        }
        let reader = WilReader::open(&wil, &wix).expect("打开真实 WIL");
        assert!(reader.len() > 100, "Objects81 应当有几百张图");
        let image = reader.image(0).expect("解码第 0 张");
        assert!(image.info.width > 0 && image.info.height > 0);
        assert!(
            image.pixels.chunks_exact(4).any(|pixel| pixel[3] != 0),
            "第 0 张图应当有非透明像素"
        );
        println!(
            "Objects81：{} 张图，第 0 张 {}×{}，偏移 ({}, {})",
            reader.len(),
            image.info.width,
            image.info.height,
            image.info.offset_x,
            image.info.offset_y
        );
    }
}
