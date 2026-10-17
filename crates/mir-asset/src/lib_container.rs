//! 旧版 `.Lib` 容器（Crystal 客户端自用的图库，`Client/MirGraphics/MLibrary.cs`）。
//!
//! # 文件布局（`MLibrary.cs:513-533`）
//!
//! | 偏移 | 大小 | 含义 |
//! |---|---|---|
//! | 0 | `i32` | 版本号。`MLibraryV1` 写 1、`MLibraryV2` 写 3（`MLibraryV1.cs:21`、`MLibraryV2.cs:21`）；Crystal 客户端只接受 `>= 2`（`MLibrary.cs:513-519`），本实现接受 1、2、3 |
//! | 4 | `i32` | 图像数量 `count` |
//! | 8 | `i32` | 只有 Crystal 写的 v3 才是 `frameSeek`（`MLibrary.cs:522-526`）；v1/v2 与「无 `frameSeek`」变体在这里直接开始偏移表，见 `parse_header` |
//! | 8 / 12 | `count × i32` | 每张图记录的文件偏移（`MLibrary.cs:531-532`）；起始位置见左行 |
//!
//! 单张图记录（`MLibrary.cs:880-890`）：
//!
//! | 记录内偏移 | 大小 | 含义 |
//! |---|---|---|
//! | +0 | `i16` | 宽 `Width` |
//! | +2 | `i16` | 高 `Height` |
//! | +4 | `i16` | 绘制偏移 `X` |
//! | +6 | `i16` | 绘制偏移 `Y` |
//! | +8 | `i16` | 阴影偏移 `ShadowX` |
//! | +10 | `i16` | 阴影偏移 `ShadowY` |
//! | +12 | `u8` | `Shadow`；最高位为 1 表示后面还有第二层（`MLibrary.cs:892-902`） |
//! | +13 | `i32` | 第一层 gzip 数据长度 `Length` |
//! | +17 | `Length` | **gzip 压缩的 RGBA32**（`MLibrary.cs:914`）或 **DXT1**（`MLibraryV1.cs:482`） |
//!
//! 两种存储都解出 `宽 × 高 × 4` 字节，行序**自上而下**：Crystal 把结果原样拷进
//! `A8R8G8B8` 贴图（`MLibrary.cs:910-917`、`MLibraryV1.cs:466-495`），没有任何翻转。
//! 存储方式没有字段标记，本模块按数据开头是不是 gzip 魔数、长度是不是正好
//! `⌈宽/4⌉ × ⌈高/4⌉ × 8` 来识别（见 [`parse_record`]）。
//!
//! 有第二层时，第一层数据之后紧跟 `i16 × 4`（MaskWidth/MaskHeight/MaskX/MaskY）
//! 与 `i32 MaskLength` + 数据（`MLibrary.cs:896-902`）。本模块只解析第二层是否存在的标志，
//! 不解码它——Crystal 客户端渲染时也只在 `DrawTinted` 里用它做蒙版（`MLibrary.cs:792-795`）。

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use flate2::read::GzDecoder;

use crate::dxt1;
use crate::image::{Image, ImageInfo, format};
use crate::{AssetError, MAX_IMAGE_BYTES, MAX_IMAGES, source};

/// `.Lib` 头至少要有版本号与数量两个 `i32`（`MLibrary.cs:513-520`）。
const MIN_HEADER_LEN: u64 = 8;

/// Crystal 的 v3 头还多一个 `frameSeek`（`MLibrary.cs:522-526`）。
const V3_HEADER_LEN: u64 = 12;

/// `.Lib` 单张图记录头：6 个 `i16` + 1 个 `u8` + 1 个 `i32`（`MLibrary.cs:880-890`）。
const RECORD_HEADER_LEN: u64 = 17;

/// gzip 魔数：`MLibraryV2` 的像素数据都以它开头（`MLibraryV2.cs:510-546`）。
const GZIP_MAGIC: [u8; 2] = [0x1F, 0x8B];

/// `.Lib` 文件头。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LibHeader {
    /// 图像数量。
    pub(crate) count: u32,
    /// 偏移表起始位置。见 [`parse_header`] 里对两种 v3 布局的说明。
    pub(crate) index_offset: u64,
}

/// 解析 `.Lib` 文件头。
///
/// # 版本与两种头
///
/// Crystal 的编辑器写过三个版本：`MLibraryV1` 是版本 1、DXT1（`MLibraryV1.cs:21`、
/// `MLibraryV1.cs:51-55`），`MLibraryV2` 是版本 3、gzip RGBA32（`MLibraryV2.cs:21`）；
/// 客户端只接受 `>= 2`（`Client/MirGraphics/MLibrary.cs:513-519`），但语料里仍有 4561 个
/// 版本 1 的 `.Lib`（`ArtificialMir` 的怪物/装备库），所以这里 1、2、3 都接受。
///
/// Crystal 的写入器把 `frameSeek` 放在 v3 头里第三个 `i32`（`Client/MirGraphics/MLibrary.cs:522-526`，
/// 写入端见 `LibraryEditor/Graphics/MLibraryV2.cs:109-133`），所以偏移表从 12 开始。
///
/// 但语料里大量真实 `.Lib`（`ArtificialMir2` 客户端、社区武器/盔甲包，共 1350 个）虽然版本号是 3，
/// 却**没有** `frameSeek` 字段：头只有 `版本 + 数量`，偏移表从 8 开始，第一个偏移正好等于
/// `8 + count × 4`（即偏移表末尾）。用这个等式区分两种 v3 布局：
///
/// * 版本 1/2：偏移表从 8 开始（`MLibraryV1.cs:56-61`、`MLibrary.cs:520-532`）。
/// * 版本 3 且 `u32@8 == 8 + count × 4` → 无 `frameSeek`，偏移表从 8 开始。
/// * 其余版本 3 → Crystal 布局，偏移表从 12 开始。
///
/// Crystal 写的 `frameSeek` 总是指向所有图像之后的帧数据（`= 12 + count × 4 + 图像字节数`），
/// 不可能等于 `8 + count × 4`，所以这个判据不会误判非空库。
pub(crate) fn parse_header(prefix: &[u8], path: &Path) -> Result<LibHeader, AssetError> {
    if prefix.len() < MIN_HEADER_LEN as usize {
        return Err(AssetError::Truncated {
            path: path.to_path_buf(),
            offset: 0,
            needed: MIN_HEADER_LEN,
            actual: prefix.len() as u64,
        });
    }
    let version = i32::from_le_bytes([prefix[0], prefix[1], prefix[2], prefix[3]]);
    let raw_count = i32::from_le_bytes([prefix[4], prefix[5], prefix[6], prefix[7]]);
    if !matches!(version, 1..=3) {
        return Err(AssetError::BadMagic {
            path: path.to_path_buf(),
            offset: 0,
            expected: "旧版 .Lib 头里的版本号 1/2/3（MLibrary.cs:513-519、MLibraryV1.cs:51-55）",
            found: u32::from_le_bytes([prefix[0], prefix[1], prefix[2], prefix[3]]),
        });
    }
    if raw_count < 0 {
        return Err(AssetError::Corrupt {
            path: path.to_path_buf(),
            offset: 4,
            message: format!("图像数量为负：{raw_count}"),
        });
    }
    let count = raw_count as u32;
    let index_offset = if version >= 3 {
        let Some(third) = prefix.get(8..12) else {
            return Err(AssetError::Truncated {
                path: path.to_path_buf(),
                offset: 8,
                needed: 4,
                actual: prefix.len() as u64 - 8,
            });
        };
        let third = u32::from_le_bytes([third[0], third[1], third[2], third[3]]);
        if u64::from(third) == MIN_HEADER_LEN + u64::from(count) * 4 {
            MIN_HEADER_LEN
        } else {
            V3_HEADER_LEN
        }
    } else {
        MIN_HEADER_LEN
    };
    Ok(LibHeader {
        count,
        index_offset,
    })
}

/// `.Lib` 单张图记录头（17 字节，`MLibrary.cs:880-890`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LibRecord {
    /// 宽。
    pub(crate) width: i16,
    /// 高。
    pub(crate) height: i16,
    /// 绘制偏移 X。
    pub(crate) offset_x: i16,
    /// 绘制偏移 Y。
    pub(crate) offset_y: i16,
    /// 阴影偏移 X。
    pub(crate) shadow_x: i16,
    /// 阴影偏移 Y。
    pub(crate) shadow_y: i16,
    /// `Shadow` 字节，最高位是「有第二层」标志（`MLibrary.cs:892-893`）。
    pub(crate) shadow: u8,
    /// 第一层数据长度。
    pub(crate) length: u32,
    /// 像素数据的存储方式：[`format::RGBA32_GZIP`] 或 [`format::DXT1`]。
    pub(crate) format: u8,
}

impl LibRecord {
    /// 转成与容器无关的头信息。
    pub(crate) fn info(&self, data_offset: u64) -> ImageInfo {
        ImageInfo {
            width: self.width as u16,
            height: self.height as u16,
            offset_x: self.offset_x,
            offset_y: self.offset_y,
            format: self.format,
            shadow_x: self.shadow_x,
            shadow_y: self.shadow_y,
            reserved: [self.shadow, 0, 0],
            has_mask: self.shadow & 0x80 != 0,
            data_offset,
            data_len: self.length,
        }
    }
}

/// 解析 `.Lib` 单张图记录头，并按数据的开头两个字节/长度判断像素存储方式。
pub(crate) fn parse_record(
    header: &[u8],
    data_magic: Option<[u8; 2]>,
    path: &Path,
    offset: u64,
) -> Result<LibRecord, AssetError> {
    if header.len() < RECORD_HEADER_LEN as usize {
        return Err(AssetError::Truncated {
            path: path.to_path_buf(),
            offset,
            needed: RECORD_HEADER_LEN,
            actual: header.len() as u64,
        });
    }
    let width = i16::from_le_bytes([header[0], header[1]]);
    let height = i16::from_le_bytes([header[2], header[3]]);
    let offset_x = i16::from_le_bytes([header[4], header[5]]);
    let offset_y = i16::from_le_bytes([header[6], header[7]]);
    let shadow_x = i16::from_le_bytes([header[8], header[9]]);
    let shadow_y = i16::from_le_bytes([header[10], header[11]]);
    let shadow = header[12];
    let length = i32::from_le_bytes([header[13], header[14], header[15], header[16]]);
    if width < 0 || height < 0 {
        return Err(AssetError::Corrupt {
            path: path.to_path_buf(),
            offset,
            message: format!("负的宽高 {width}×{height}"),
        });
    }
    if length < 0 {
        return Err(AssetError::Corrupt {
            path: path.to_path_buf(),
            offset: offset + 13,
            message: format!("像素数据长度为负：{length}"),
        });
    }
    let length = length as u32;
    let format = if width == 0 || height == 0 || length == 0 {
        // 空记录，后面不会真的解码。
        format::RGBA32_GZIP
    } else if data_magic == Some(GZIP_MAGIC) {
        // Crystal `MLibraryV2` 的像素数据是 gzip 流（`MLibraryV2.cs:510-546`）。
        format::RGBA32_GZIP
    } else if u64::from(length) == dxt1::storage_size(width as u16, height as u16) {
        // Crystal `MLibraryV1` 的像素数据是 DXT1（`MLibraryV1.cs:433-442`、`MLibraryV1.cs:466-495`）。
        format::DXT1
    } else {
        return Err(AssetError::UnsupportedPixels {
            path: path.to_path_buf(),
            offset: offset + RECORD_HEADER_LEN,
            width: width as u16,
            height: height as u16,
            length,
        });
    };
    Ok(LibRecord {
        width,
        height,
        offset_x,
        offset_y,
        shadow_x,
        shadow_y,
        shadow,
        length,
        format,
    })
}

/// 已经打开索引的 `.Lib` 读取器。
#[derive(Debug)]
pub(crate) struct LibReader {
    /// 文件路径，只用于错误信息。
    path: PathBuf,
    /// 复用的文件句柄；所有读取都在锁内 `seek` 到目标位置。
    file: Mutex<File>,
    /// 文件长度，用于边界检查。
    file_len: u64,
    /// 每张图记录的文件偏移。
    offsets: Vec<u32>,
}

impl LibReader {
    /// 打开 `.Lib`：读文件头与偏移表，不读任何图像数据。
    pub(crate) fn open(path: &Path) -> Result<Self, AssetError> {
        let mut file = source::open_file(path)?;
        let file_len = source::file_len(path, &file)?;
        if file_len < MIN_HEADER_LEN {
            return Err(AssetError::Truncated {
                path: path.to_path_buf(),
                offset: 0,
                needed: MIN_HEADER_LEN,
                actual: file_len,
            });
        }
        let mut head = [0u8; V3_HEADER_LEN as usize];
        let prefix_len = usize::try_from(file_len.min(V3_HEADER_LEN)).unwrap_or(0);
        source::read_at(&mut file, path, 0, &mut head[..prefix_len])?;
        let header = parse_header(&head[..prefix_len], path)?;
        if u64::from(header.count) > MAX_IMAGES {
            return Err(AssetError::Corrupt {
                path: path.to_path_buf(),
                offset: 4,
                message: format!("图像数量 {} 超过上限 {MAX_IMAGES}", header.count),
            });
        }
        let index_bytes = u64::from(header.count) * 4;
        if header.index_offset + index_bytes > file_len {
            return Err(AssetError::Truncated {
                path: path.to_path_buf(),
                offset: header.index_offset,
                needed: index_bytes,
                actual: file_len.saturating_sub(header.index_offset),
            });
        }
        let mut raw = vec![0u8; usize::try_from(index_bytes).unwrap_or(0)];
        source::read_at(&mut file, path, header.index_offset, &mut raw)?;
        let offsets = raw
            .chunks_exact(4)
            .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            .collect();
        Ok(Self {
            path: path.to_path_buf(),
            file: Mutex::new(file),
            file_len,
            offsets,
        })
    }

    /// 图像数量，等于头里的 `count`（`MLibrary.cs:520`）。
    pub(crate) fn len(&self) -> usize {
        self.offsets.len()
    }

    /// 只读头信息，不解码像素。
    pub(crate) fn info(&self, index: usize) -> Result<ImageInfo, AssetError> {
        let (record, data_offset) = self.record(index)?;
        Ok(record.info(data_offset))
    }

    /// 解码一张图：gzip RGBA32 直接解压，DXT1 走块解码。
    pub(crate) fn image(&self, index: usize) -> Result<Image, AssetError> {
        let (record, data_offset) = self.record(index)?;
        let info = record.info(data_offset);
        if info.is_empty() {
            // Crystal 在 `CheckImage` 里遇到 0 宽或 0 高直接返回 false（`MLibrary.cs:571-574`）。
            return Ok(Image {
                info,
                pixels: Vec::new(),
            });
        }
        let expected = u64::from(info.width) * u64::from(info.height) * 4;
        if expected > MAX_IMAGE_BYTES {
            return Err(AssetError::ImageTooLarge {
                width: info.width,
                height: info.height,
                needed: expected,
                limit: MAX_IMAGE_BYTES,
            });
        }
        let expected_size = usize::try_from(expected).map_err(|_| AssetError::ImageTooLarge {
            width: info.width,
            height: info.height,
            needed: expected,
            limit: MAX_IMAGE_BYTES,
        })?;
        let mut compressed = vec![0u8; usize::try_from(record.length).unwrap_or(0)];
        {
            let mut file = self.lock();
            source::read_at(&mut file, &self.path, data_offset, &mut compressed)?;
        }
        if info.format == format::DXT1 {
            return Ok(Image {
                info,
                pixels: dxt1::decode(&compressed, info.width, info.height),
            });
        }
        match decompress_gzip(&compressed, expected_size, &self.path, data_offset) {
            Ok(pixels) => Ok(Image { info, pixels }),
            Err(error) => {
                // 极少数 DXT1 图的首块颜色低字节恰好是 `1f 8b`，会被误判成 gzip；
                // 长度正好等于 DXT1 尺寸时按 DXT1 再试一次。
                if u64::from(record.length) == dxt1::storage_size(info.width, info.height) {
                    let mut info = info;
                    info.format = format::DXT1;
                    return Ok(Image {
                        info,
                        pixels: dxt1::decode(&compressed, info.width, info.height),
                    });
                }
                Err(error)
            }
        }
    }

    /// 读下标对应的记录头，并返回像素数据在文件里的偏移。
    fn record(&self, index: usize) -> Result<(LibRecord, u64), AssetError> {
        let count = self.offsets.len();
        let stored = *self
            .offsets
            .get(index)
            .ok_or(AssetError::IndexOutOfRange { index, count })?;
        let offset = u64::from(stored);
        if offset + RECORD_HEADER_LEN > self.file_len {
            return Err(AssetError::Truncated {
                path: self.path.clone(),
                offset,
                needed: RECORD_HEADER_LEN,
                actual: self.file_len.saturating_sub(offset),
            });
        }
        let mut head = [0u8; (RECORD_HEADER_LEN + 2) as usize];
        let to_read = usize::try_from(self.file_len.saturating_sub(offset).min(head.len() as u64))
            .unwrap_or(0);
        {
            let mut file = self.lock();
            source::read_at(&mut file, &self.path, offset, &mut head[..to_read])?;
        }
        let data_magic = if to_read >= head.len() {
            Some([
                head[RECORD_HEADER_LEN as usize],
                head[RECORD_HEADER_LEN as usize + 1],
            ])
        } else {
            None
        };
        let header_len = to_read.min(RECORD_HEADER_LEN as usize);
        let record = parse_record(&head[..header_len], data_magic, &self.path, offset)?;
        let data_offset = offset + RECORD_HEADER_LEN;
        if data_offset + u64::from(record.length) > self.file_len {
            return Err(AssetError::Truncated {
                path: self.path.clone(),
                offset: data_offset,
                needed: u64::from(record.length),
                actual: self.file_len - data_offset,
            });
        }
        Ok((record, data_offset))
    }

    /// 拿文件锁；中毒时取回内部值继续用（读取是只读的，中毒不影响数据）。
    fn lock(&self) -> MutexGuard<'_, File> {
        self.file.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// 解压 gzip 像素数据，返回长度为 `expected_size` 的缓冲。
fn decompress_gzip(
    data: &[u8],
    expected_size: usize,
    path: &Path,
    offset: u64,
) -> Result<Vec<u8>, AssetError> {
    let mut pixels = Vec::with_capacity(expected_size);
    let decoder = GzDecoder::new(data);
    // 多读 1 字节就能判断「解压结果比声明的尺寸长」。
    decoder
        .take(expected_size as u64 + 1)
        .read_to_end(&mut pixels)
        .map_err(|error| AssetError::Compression {
            path: path.to_path_buf(),
            offset,
            message: error.to_string(),
        })?;
    if pixels.len() < expected_size {
        return Err(AssetError::Corrupt {
            path: path.to_path_buf(),
            offset,
            message: format!(
                "gzip 只解压出 {} 字节，少于 {expected_size} 字节",
                pixels.len()
            ),
        });
    }
    // Crystal 多解压出来的字节会撑爆贴图；这里忽略多余字节。
    pixels.truncate(expected_size);
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_path() -> &'static Path {
        Path::new("<memory>")
    }

    fn header_bytes(version: i32, count: i32, third: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; 12];
        bytes[..4].copy_from_slice(&version.to_le_bytes());
        bytes[4..8].copy_from_slice(&count.to_le_bytes());
        bytes[8..12].copy_from_slice(&third.to_le_bytes());
        bytes
    }

    #[test]
    fn v2_and_v3_headers_differ_only_in_index_offset() {
        let v2 = parse_header(&header_bytes(2, 7, 0), memory_path()).expect("v2 头应当合法");
        assert_eq!(v2.count, 7);
        assert_eq!(v2.index_offset, 8);
        // Crystal 布局：第三个 i32 是 frameSeek（这里给 0），偏移表从 12 开始。
        let v3 = parse_header(&header_bytes(3, 4096, 0), memory_path()).expect("v3 头应当合法");
        assert_eq!(v3.count, 4096);
        assert_eq!(v3.index_offset, 12);
    }

    #[test]
    fn v3_without_frame_seek_uses_index_offset_eight() {
        // 语料里 1350 个真实 .Lib：版本 3 但没有 frameSeek，u32@8 正好是偏移表末尾。
        let v3 = parse_header(&header_bytes(3, 368, 8 + 368 * 4), memory_path())
            .expect("无 frameSeek 的 v3 应当合法");
        assert_eq!(v3.count, 368);
        assert_eq!(v3.index_offset, 8);
    }

    #[test]
    fn v3_prefix_without_third_field_is_truncated() {
        let error = parse_header(&header_bytes(3, 1, 0)[..10], memory_path())
            .expect_err("v3 需要 12 字节才能判断布局");
        assert!(
            matches!(
                error,
                AssetError::Truncated {
                    offset: 8,
                    needed: 4,
                    actual: 2,
                    ..
                }
            ),
            "实际错误：{error}"
        );
    }

    #[test]
    fn v1_and_v2_use_index_offset_eight() {
        let v1 = parse_header(&header_bytes(1, 760, 0), memory_path()).expect("v1 头应当合法");
        assert_eq!(v1.count, 760);
        assert_eq!(v1.index_offset, 8);
        let v2 = parse_header(&header_bytes(2, 7, 0), memory_path()).expect("v2 头应当合法");
        assert_eq!(v2.count, 7);
        assert_eq!(v2.index_offset, 8);
    }

    #[test]
    fn header_rejects_old_version() {
        let error =
            parse_header(&header_bytes(0, 1, 0), memory_path()).expect_err("版本 0 应当被拒");
        assert!(
            matches!(
                error,
                AssetError::BadMagic {
                    offset: 0,
                    found: 0,
                    ..
                }
            ),
            "实际错误：{error}"
        );
    }

    #[test]
    fn header_rejects_negative_count() {
        let error =
            parse_header(&header_bytes(3, -5, 0), memory_path()).expect_err("负数量应当被拒");
        assert!(matches!(error, AssetError::Corrupt { offset: 4, .. }));
    }

    #[test]
    fn header_rejects_short_prefix() {
        let error = parse_header(&[3, 0, 0], memory_path()).expect_err("3 字节不够");
        assert!(
            matches!(
                error,
                AssetError::Truncated {
                    offset: 0,
                    needed: 8,
                    actual: 3,
                    ..
                }
            ),
            "实际错误：{error}"
        );
    }

    #[test]
    fn record_parses_all_fields_and_mask_flag() {
        let mut header = [0u8; 17];
        header[0..2].copy_from_slice(&20i16.to_le_bytes());
        header[2..4].copy_from_slice(&13i16.to_le_bytes());
        header[4..6].copy_from_slice(&7i16.to_le_bytes());
        header[6..8].copy_from_slice(&(-44i16).to_le_bytes());
        header[8..10].copy_from_slice(&(-2i16).to_le_bytes());
        header[10..12].copy_from_slice(&3i16.to_le_bytes());
        header[12] = 0x80;
        header[13..17].copy_from_slice(&235i32.to_le_bytes());

        let record =
            parse_record(&header, Some(GZIP_MAGIC), memory_path(), 100).expect("记录头应当合法");
        assert_eq!(record.width, 20);
        assert_eq!(record.height, 13);
        assert_eq!(record.offset_x, 7);
        assert_eq!(record.offset_y, -44);
        assert_eq!(record.shadow_x, -2);
        assert_eq!(record.shadow_y, 3);
        assert_eq!(record.length, 235);
        assert_eq!(record.format, format::RGBA32_GZIP);

        let info = record.info(117);
        assert_eq!(info.format, format::RGBA32_GZIP);
        assert_eq!(info.format_name(), "rgba32-gzip");
        assert_eq!(info.reserved, [0x80, 0, 0]);
        assert!(info.has_mask);
        assert_eq!(info.data_offset, 117);
        assert_eq!(info.data_len, 235);
        assert_eq!(info.shadow_x, -2);
    }

    #[test]
    fn record_detects_dxt1_by_exact_storage_size() {
        // 20×16 的 DXT1 = ⌈20/4⌉ × ⌈16/4⌉ × 8 = 160 字节，与真实数据一致。
        let mut header = [0u8; 17];
        header[0..2].copy_from_slice(&20i16.to_le_bytes());
        header[2..4].copy_from_slice(&16i16.to_le_bytes());
        header[13..17].copy_from_slice(&160i32.to_le_bytes());
        let record = parse_record(&header, None, memory_path(), 100).expect("DXT1 记录应当合法");
        assert_eq!(record.format, format::DXT1);
        assert_eq!(record.info(117).format_name(), "dxt1");
    }

    #[test]
    fn record_rejects_unknown_pixel_storage() {
        let mut header = [0u8; 17];
        header[0..2].copy_from_slice(&20i16.to_le_bytes());
        header[2..4].copy_from_slice(&16i16.to_le_bytes());
        header[13..17].copy_from_slice(&100i32.to_le_bytes());
        let error = parse_record(&header, None, memory_path(), 100)
            .expect_err("100 字节既不是 gzip 也不是 DXT1");
        assert!(
            matches!(
                error,
                AssetError::UnsupportedPixels {
                    offset: 117,
                    width: 20,
                    height: 16,
                    length: 100,
                    ..
                }
            ),
            "实际错误：{error}"
        );
    }

    #[test]
    fn empty_record_needs_no_pixel_format() {
        let header = [0u8; 17];
        let record = parse_record(&header, None, memory_path(), 0).expect("空记录应当合法");
        assert!(record.info(17).is_empty());
        assert_eq!(record.format, format::RGBA32_GZIP);
    }

    #[test]
    fn record_rejects_negative_dimensions() {
        let mut header = [0u8; 17];
        header[0..2].copy_from_slice(&(-1i16).to_le_bytes());
        let error = parse_record(&header, None, memory_path(), 0).expect_err("负宽应当被拒");
        assert!(matches!(error, AssetError::Corrupt { offset: 0, .. }));
    }

    #[test]
    fn record_rejects_negative_length() {
        let mut header = [0u8; 17];
        header[13..17].copy_from_slice(&(-9i32).to_le_bytes());
        let error = parse_record(&header, None, memory_path(), 48).expect_err("负长度应当被拒");
        assert!(
            matches!(error, AssetError::Corrupt { offset: 61, .. }),
            "实际错误：{error}"
        );
    }
}
