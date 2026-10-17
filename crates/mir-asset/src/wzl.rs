//! `.wzl` + `.wzx` 容器（Crystal `LibraryEditor` 的 Shanda 图库格式，
//! `LibraryEditor/Graphics/WeMadeLibrary.cs` 里 `_nType == 1` 的那一套）。
//!
//! # 两个文件的分工
//!
//! * `.wzx`：索引。前 48 字节是头，其中偏移 44 的 `u32` 是图像数量；紧接着是
//!   `count × u32` 的偏移表，指向 `.wzl` 里每条记录的开头。Crystal 的
//!   `LoadIndexFile` 直接跳过 48 字节、把剩下的字节每 4 个读成一个偏移，并不检查头里的数量
//!   （`WeMadeLibrary.cs:140-147`），本实现跟它一致：图像数量 = `(文件长度 - 48) / 4`。
//! * `.wzl`：数据。每条记录 16 字节头 + zlib 压缩的像素数据。
//!
//! # 记录头（`WeMadeLibrary.cs:313-331`）
//!
//! | 记录内偏移 | 大小 | 含义 |
//! |---|---|---|
//! | +0 | `u8` | 格式：`5` = RGB565，其余按 8 位调色板（`WeMadeLibrary.cs:319`） |
//! | +1 | 3 字节 | Crystal 直接 `ReadBytes(3)` 跳过（`WeMadeLibrary.cs:320`），本实现原样保留 |
//! | +4 | `u16` | 宽 |
//! | +6 | `u16` | 高 |
//! | +8 | `i16` | 绘制偏移 X |
//! | +10 | `i16` | 绘制偏移 Y |
//! | +12 | `u32` | zlib 压缩数据长度 |
//! | +16 | `length` | zlib 数据（`WeMadeLibrary.cs:386-394`） |
//!
//! # 像素解码要点
//!
//! * **行按 4 字节对齐**：每行字节数是 `(((宽 × 位数) + 31) >> 5) * 4`
//!   （`WidthBytes`，`WeMadeLibrary.cs:231-234`；补齐在 `WeMadeLibrary.cs:469-470`）。
//! * **行序自下而上**：Crystal 的循环是 `for (y = Height - 1; y >= 0; y--)`，数据第 0 行
//!   写进位图最下面一行（`WeMadeLibrary.cs:453-468`），所以这里要上下翻转才能得到
//!   自上而下的 RGBA 缓冲。
//! * 8 位索引查硬编码的 `WeMade` 调色板（见 [`crate::palette`]）；RGB565 的展开只做移位，
//!   不做低位复制（`WeMadeLibrary.cs:223-229`），因此 `0xF800` 得到 248 而不是 255。
//! * 长度为 0 的记录 Crystal 会把宽强制当成 0（`WeMadeLibrary.cs:362`），本实现照做。

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use flate2::read::ZlibDecoder;

use crate::image::{self, Image, ImageInfo, format};
use crate::{AssetError, MAX_IMAGE_BYTES, MAX_IMAGES, source};

/// `.wzx` 头长度：前 44 字节恒为 0，偏移 44 起是图像数量（`WeMadeLibrary.cs:140-147`）。
const WZX_HEADER_LEN: usize = 48;

/// `.wzl` 单条记录的固定头长度（`WeMadeLibrary.cs:37` 的 `ImageStructureSize[1] == 16`）。
const RECORD_HEADER_LEN: u64 = 16;

/// 已解析的 `.wzx` 索引。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WzxIndex {
    /// 头里偏移 44 声明的数量（Crystal 不读它，只用于诊断）。
    pub(crate) declared_count: u32,
    /// 偏移表；0 表示该下标没有图像（语料里大量存在）。
    pub(crate) offsets: Vec<u32>,
}

/// 解析 `.wzx` 索引。
///
/// 尾部不足 4 字节的残余会被忽略，与 Crystal 的
/// `while (position <= length - 4)` 循环一致（`WeMadeLibrary.cs:146-147`）。
pub(crate) fn parse_index(bytes: &[u8], path: &Path) -> Result<WzxIndex, AssetError> {
    if bytes.len() < WZX_HEADER_LEN {
        return Err(AssetError::Truncated {
            path: path.to_path_buf(),
            offset: 0,
            needed: WZX_HEADER_LEN as u64,
            actual: bytes.len() as u64,
        });
    }
    let declared_count = u32::from_le_bytes([bytes[44], bytes[45], bytes[46], bytes[47]]);
    let table = &bytes[WZX_HEADER_LEN..];
    let count = table.len() / 4;
    if count as u64 > MAX_IMAGES {
        return Err(AssetError::Corrupt {
            path: path.to_path_buf(),
            offset: WZX_HEADER_LEN as u64,
            message: format!("偏移表有 {count} 项，超过上限 {MAX_IMAGES}"),
        });
    }
    let offsets = table
        .chunks_exact(4)
        .map(|entry| u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]))
        .collect();
    Ok(WzxIndex {
        declared_count,
        offsets,
    })
}

/// `.wzl` 单条记录头（16 字节，`WeMadeLibrary.cs:313-331`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WzlRecord {
    /// 格式字节。
    pub(crate) format: u8,
    /// 格式字节之后的 3 个保留字节。
    pub(crate) reserved: [u8; 3],
    /// 宽。
    pub(crate) width: u16,
    /// 高。
    pub(crate) height: u16,
    /// 绘制偏移 X。
    pub(crate) offset_x: i16,
    /// 绘制偏移 Y。
    pub(crate) offset_y: i16,
    /// zlib 压缩数据长度。
    pub(crate) length: u32,
}

impl WzlRecord {
    /// 偏移表里的 0 表示「没有这张图」：返回全 0 的空记录。
    pub(crate) const fn empty() -> Self {
        Self {
            format: format::EMPTY,
            reserved: [0; 3],
            width: 0,
            height: 0,
            offset_x: 0,
            offset_y: 0,
            length: 0,
        }
    }

    /// 转成与容器无关的头信息。
    pub(crate) const fn info(&self, data_offset: u64) -> ImageInfo {
        ImageInfo {
            width: self.width,
            height: self.height,
            offset_x: self.offset_x,
            offset_y: self.offset_y,
            format: self.format,
            shadow_x: 0,
            shadow_y: 0,
            reserved: self.reserved,
            has_mask: false,
            data_offset,
            data_len: self.length,
        }
    }
}

/// 解析 `.wzl` 单条记录头。
pub(crate) fn parse_record(
    header: &[u8],
    path: &Path,
    offset: u64,
) -> Result<WzlRecord, AssetError> {
    if header.len() < RECORD_HEADER_LEN as usize {
        return Err(AssetError::Truncated {
            path: path.to_path_buf(),
            offset,
            needed: RECORD_HEADER_LEN,
            actual: header.len() as u64,
        });
    }
    let format = header[0];
    let reserved = [header[1], header[2], header[3]];
    let mut width = u16::from_le_bytes([header[4], header[5]]);
    let height = u16::from_le_bytes([header[6], header[7]]);
    let offset_x = i16::from_le_bytes([header[8], header[9]]);
    let offset_y = i16::from_le_bytes([header[10], header[11]]);
    let length = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);
    if !matches!(
        format,
        format::EMPTY | format::PALETTE8 | format::RGB565 | format::RGB24
    ) {
        return Err(AssetError::UnsupportedFormat {
            path: path.to_path_buf(),
            format,
            offset,
        });
    }
    if length == 0 {
        // WeMadeLibrary.cs:362：`Width = (nSize == 0) ? (short)0 : Width;`
        width = 0;
    }
    Ok(WzlRecord {
        format,
        reserved,
        width,
        height,
        offset_x,
        offset_y,
        length,
    })
}

/// 已经打开索引的 `.wzl` 读取器。
#[derive(Debug)]
pub(crate) struct WzlReader {
    /// `.wzl` 路径。
    wzl_path: PathBuf,
    /// `.wzx` 路径（只用于诊断，索引已经在内存里）。
    wzx_path: PathBuf,
    /// 复用的 `.wzl` 文件句柄。
    file: Mutex<File>,
    /// `.wzl` 文件长度，用于边界检查。
    file_len: u64,
    /// 偏移表。
    offsets: Vec<u32>,
    /// 头里声明的数量（`WeMadeLibrary.cs:140-147`，Crystal 不用它）。
    declared_count: u32,
}

impl WzlReader {
    /// 打开 `.wzl` 与配套 `.wzx`：只读索引，不读图像数据。
    pub(crate) fn open(wzl_path: &Path, wzx_path: &Path) -> Result<Self, AssetError> {
        let file = source::open_file(wzl_path)?;
        let file_len = source::file_len(wzl_path, &file)?;
        let index_bytes =
            std::fs::read(wzx_path).map_err(|source| source::io_error(wzx_path, source))?;
        let index = parse_index(&index_bytes, wzx_path)?;
        Ok(Self {
            wzl_path: wzl_path.to_path_buf(),
            wzx_path: wzx_path.to_path_buf(),
            file: Mutex::new(file),
            file_len,
            offsets: index.offsets,
            declared_count: index.declared_count,
        })
    }

    /// 图像数量 = 偏移表项数（Crystal 的 `_indexList.Count`）。
    pub(crate) fn len(&self) -> usize {
        self.offsets.len()
    }

    /// `.wzx` 头里声明的数量。
    pub(crate) fn declared_count(&self) -> u32 {
        self.declared_count
    }

    /// `.wzx` 路径。
    pub(crate) fn index_path(&self) -> &Path {
        &self.wzx_path
    }

    /// 只读头信息，不解码像素。
    pub(crate) fn info(&self, index: usize) -> Result<ImageInfo, AssetError> {
        let (record, data_offset) = self.record(index)?;
        Ok(record.info(data_offset))
    }

    /// 解码一张图：读 zlib 数据、解压、按格式展开成 RGBA8。
    pub(crate) fn image(&self, index: usize) -> Result<Image, AssetError> {
        let (record, data_offset) = self.record(index)?;
        let info = record.info(data_offset);
        if info.is_empty() {
            return Ok(Image {
                info,
                pixels: Vec::new(),
            });
        }
        let bits = image::pixel_bits(record.format);
        let row = image::row_bytes(bits, record.width);
        let expected = row * u64::from(record.height);
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
        let row_size = usize::try_from(row).map_err(|_| AssetError::ImageTooLarge {
            width: info.width,
            height: info.height,
            needed: expected,
            limit: MAX_IMAGE_BYTES,
        })?;
        let mut compressed = vec![0u8; usize::try_from(record.length).unwrap_or(0)];
        {
            let mut file = self.lock();
            source::read_at(&mut file, &self.wzl_path, data_offset, &mut compressed)?;
        }
        let raw = decompress_zlib(&compressed, expected_size, &self.wzl_path, data_offset)?;
        Ok(Image {
            info,
            pixels: expand(&raw, info, row_size),
        })
    }

    /// 读下标对应的记录头，并返回像素数据在 `.wzl` 里的偏移。
    fn record(&self, index: usize) -> Result<(WzlRecord, u64), AssetError> {
        let count = self.offsets.len();
        let stored = *self
            .offsets
            .get(index)
            .ok_or(AssetError::IndexOutOfRange { index, count })?;
        if stored == 0 {
            return Ok((WzlRecord::empty(), 0));
        }
        let offset = u64::from(stored);
        if offset.saturating_add(RECORD_HEADER_LEN) > self.file_len {
            return Err(AssetError::Truncated {
                path: self.wzl_path.clone(),
                offset,
                needed: RECORD_HEADER_LEN,
                actual: self.file_len.saturating_sub(offset),
            });
        }
        let mut header = [0u8; RECORD_HEADER_LEN as usize];
        {
            let mut file = self.lock();
            source::read_at(&mut file, &self.wzl_path, offset, &mut header)?;
        }
        let record = parse_record(&header, &self.wzl_path, offset)?;
        let data_offset = offset + RECORD_HEADER_LEN;
        if data_offset + u64::from(record.length) > self.file_len {
            return Err(AssetError::Truncated {
                path: self.wzl_path.clone(),
                offset: data_offset,
                needed: u64::from(record.length),
                actual: self.file_len - data_offset,
            });
        }
        Ok((record, data_offset))
    }

    /// 拿文件锁；中毒时取回内部值继续用。
    fn lock(&self) -> MutexGuard<'_, File> {
        self.file.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// 解压 zlib 数据，返回长度为 `expected_size` 的缓冲。
fn decompress_zlib(
    data: &[u8],
    expected_size: usize,
    path: &Path,
    offset: u64,
) -> Result<Vec<u8>, AssetError> {
    let decoder = ZlibDecoder::new(data);
    let mut raw = Vec::new();
    decoder
        .take(expected_size as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|error| AssetError::Compression {
            path: path.to_path_buf(),
            offset,
            message: error.to_string(),
        })?;
    if raw.len() < expected_size {
        return Err(AssetError::Corrupt {
            path: path.to_path_buf(),
            offset,
            message: format!(
                "zlib 只解压出 {} 字节，少于按行对齐后的 {expected_size} 字节",
                raw.len()
            ),
        });
    }
    // Crystal 会把多出来的字节一起拷进贴图（越界抛异常）；这里只取前 expected_size 字节。
    raw.truncate(expected_size);
    Ok(raw)
}

/// 把按行对齐、自下而上的原始像素展开成自上而下的 RGBA8。
fn expand(raw: &[u8], info: ImageInfo, row: usize) -> Vec<u8> {
    let width = usize::from(info.width);
    let height = usize::from(info.height);
    let mut pixels = vec![0u8; width * height * 4];
    for (file_row, source_row) in raw.chunks_exact(row).enumerate() {
        let flipped = height - 1 - file_row;
        let destination = &mut pixels[flipped * width * 4..(flipped + 1) * width * 4];
        match info.format {
            format::RGB565 => {
                for (column, pair) in source_row.chunks_exact(2).take(width).enumerate() {
                    let value = u16::from_le_bytes([pair[0], pair[1]]);
                    destination[column * 4..column * 4 + 4]
                        .copy_from_slice(&image::expand_rgb565(value));
                }
            }
            format::RGB24 => {
                for (column, triple) in source_row.chunks_exact(3).take(width).enumerate() {
                    destination[column * 4..column * 4 + 4]
                        .copy_from_slice(&[triple[0], triple[1], triple[2], 0xFF]);
                }
            }
            _ => {
                for (column, &index) in source_row.iter().take(width).enumerate() {
                    destination[column * 4..column * 4 + 4]
                        .copy_from_slice(&image::palette_rgba(index));
                }
            }
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_path() -> &'static Path {
        Path::new("<memory>")
    }

    fn index_bytes(declared: u32, offsets: &[u32]) -> Vec<u8> {
        let mut bytes = vec![0u8; WZX_HEADER_LEN];
        bytes[44..48].copy_from_slice(&declared.to_le_bytes());
        for offset in offsets {
            bytes.extend_from_slice(&offset.to_le_bytes());
        }
        bytes
    }

    fn record_bytes(
        format: u8,
        width: u16,
        height: u16,
        offset_x: i16,
        offset_y: i16,
        length: u32,
    ) -> Vec<u8> {
        let mut bytes = vec![format, 1, 0, 0];
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.extend_from_slice(&offset_x.to_le_bytes());
        bytes.extend_from_slice(&offset_y.to_le_bytes());
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes
    }

    #[test]
    fn index_parses_header_count_and_offsets() {
        let index = parse_index(&index_bytes(2, &[64, 0]), memory_path()).expect("索引应当合法");
        assert_eq!(index.declared_count, 2);
        assert_eq!(index.offsets, vec![64, 0]);
    }

    #[test]
    fn index_ignores_trailing_bytes() {
        let mut bytes = index_bytes(1, &[64]);
        bytes.extend_from_slice(&[0xAA, 0xBB, 0xCC]);
        let index = parse_index(&bytes, memory_path()).expect("尾部残余应当被忽略");
        assert_eq!(index.offsets, vec![64]);
    }

    #[test]
    fn index_rejects_short_header() {
        let error = parse_index(&[0u8; 47], memory_path()).expect_err("47 字节不够");
        assert!(
            matches!(
                error,
                AssetError::Truncated {
                    offset: 0,
                    needed: 48,
                    actual: 47,
                    ..
                }
            ),
            "实际错误：{error}"
        );
    }

    #[test]
    fn record_parses_565_fields() {
        let header = record_bytes(format::RGB565, 48, 32, -7, 110, 113);
        let record = parse_record(&header, memory_path(), 64).expect("记录应当合法");
        assert_eq!(record.format, format::RGB565);
        assert_eq!(record.reserved, [1, 0, 0]);
        assert_eq!(record.width, 48);
        assert_eq!(record.height, 32);
        assert_eq!(record.offset_x, -7);
        assert_eq!(record.offset_y, 110);
        assert_eq!(record.length, 113);
        let info = record.info(80);
        assert_eq!(info.data_offset, 80);
        assert_eq!(info.data_len, 113);
        assert_eq!(info.format_name(), "rgb565");
        assert!(!info.has_mask);
    }

    #[test]
    fn zero_length_forces_width_zero() {
        let header = record_bytes(format::PALETTE8, 20, 13, 0, 0, 0);
        let record = parse_record(&header, memory_path(), 48).expect("空记录应当合法");
        assert_eq!(record.width, 0);
        assert_eq!(record.height, 13);
        assert!(record.info(64).is_empty());
    }

    #[test]
    fn record_rejects_unknown_format() {
        let header = record_bytes(7, 4, 4, 0, 0, 10);
        let error = parse_record(&header, memory_path(), 100).expect_err("格式 7 不应当被接受");
        assert!(
            matches!(
                error,
                AssetError::UnsupportedFormat {
                    format: 7,
                    offset: 100,
                    ..
                }
            ),
            "实际错误：{error}"
        );
    }

    #[test]
    fn record_rejects_short_header() {
        let error = parse_record(&[5u8; 15], memory_path(), 64).expect_err("15 字节不够");
        assert!(
            matches!(
                error,
                AssetError::Truncated {
                    offset: 64,
                    needed: 16,
                    actual: 15,
                    ..
                }
            ),
            "实际错误：{error}"
        );
    }

    #[test]
    fn empty_record_is_all_zero() {
        let record = WzlRecord::empty();
        let info = record.info(0);
        assert_eq!(info.format, format::EMPTY);
        assert!(info.is_empty());
        assert_eq!(info.data_len, 0);
    }

    #[test]
    fn expand_palette_and_flip() {
        // 2×2、8 位调色板：每行按 4 字节对齐，所以文件里是 2 行 × 4 字节；
        // 文件第 0 行是图像最下面一行。
        let raw = [0u8, 3u8, 0u8, 0u8, 7u8, 1u8, 0u8, 0u8];
        let info = ImageInfo {
            width: 2,
            height: 2,
            offset_x: 0,
            offset_y: 0,
            format: format::PALETTE8,
            shadow_x: 0,
            shadow_y: 0,
            reserved: [0; 3],
            has_mask: false,
            data_offset: 0,
            data_len: 4,
        };
        let pixels = expand(&raw, info, 4);
        assert_eq!(pixels.len(), 2 * 2 * 4);
        // 输出第 0 行（顶部）来自文件第 1 行：索引 7、1
        assert_eq!(&pixels[0..4], &image::palette_rgba(7));
        assert_eq!(&pixels[4..8], &image::palette_rgba(1));
        // 输出第 1 行（底部）来自文件第 0 行：索引 0、3
        assert_eq!(&pixels[8..12], &image::palette_rgba(0));
        assert_eq!(&pixels[12..16], &image::palette_rgba(3));
    }

    #[test]
    fn expand_24_bit_rows_are_flipped() {
        // 1×2、RGB24：每行 3 字节，4 字节对齐后行宽 4。
        let raw = [10u8, 20, 30, 0, 40, 50, 60, 0];
        let info = ImageInfo {
            width: 1,
            height: 2,
            offset_x: 0,
            offset_y: 0,
            format: format::RGB24,
            shadow_x: 0,
            shadow_y: 0,
            reserved: [0; 3],
            has_mask: false,
            data_offset: 0,
            data_len: 8,
        };
        let pixels = expand(&raw, info, 4);
        assert_eq!(pixels, vec![40, 50, 60, 255, 10, 20, 30, 255]);
    }
}
