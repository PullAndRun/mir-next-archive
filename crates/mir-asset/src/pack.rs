//! 自有分包格式 **MNP**（Mir Next Pack）的读写。
//!
//! 发布期把散装素材（`game-assets/gfx/**`、`maps/`、`audio/`、`data/`）切成
//! **≤ `chunk_mb` 的小包**，发布后改哪一类只推对应小包；开发期仍用散装目录。
//!
//! # 文件格式（全部小端）
//!
//! ```text
//! 0x00  magic        "MNP1"               4B
//! 0x04  version      u16 = 1              2B
//! 0x06  flags        u16                 2B   bit0 = 包尾有 sha256
//! 0x08  entry_count  u32                 4B
//! 0x0C  index_offset u64                 8B   条目表在文件里的偏移
//! 0x14  reserved     12B 全 0
//! 0x20  …           条目数据区（zlib 压缩的 RGBA 或原样字节）
//! index_offset …    条目表：每项
//!                      name_len u16 + name(UTF-8)
//!                      kind u8（0=RGBA+zlib，1=原样字节）
//!                      has_mask u8
//!                      width u16 + height u16
//!                      offset_x i16 + offset_y i16
//!                      shadow_x i16 + shadow_y i16
//!                      data_offset u64 + data_len u32 + raw_len u32
//! 文件尾 - 32       sha256(前面全部字节)
//! ```
//!
//! 读方只看 `index_offset` 就能拿到全表，不必解压像素；`image()` 才解压。

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::AssetError;
use crate::image::{Image, ImageInfo};

/// 包魔数。
pub const MAGIC: [u8; 4] = *b"MNP1";
/// 当前格式版本。
pub const VERSION: u16 = 1;
/// 包头长度。
const HEADER_LEN: u64 = 32;
/// 包尾 sha256 长度。
const TRAILER_LEN: u64 = 32;
/// 条目 kind：RGBA 像素 + zlib。
pub const KIND_RGBA_ZLIB: u8 = 0;
/// 条目 kind：原样字节（`.map`、`.wav`、`.MirDB`）。
pub const KIND_RAW: u8 = 1;

/// `manifest.packs.json`：发布产物清单（工具与加载端共用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackManifest {
    /// 格式版本。
    pub version: u32,
    /// 生成时间（本地时间字符串）。
    pub generated_at: String,
    /// 单包上限（MB）。
    pub chunk_mb: u32,
    /// 每个包的摘要。
    pub packs: Vec<PackSummary>,
}

/// 单个包的摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackSummary {
    /// 文件名（相对 packs 目录）。
    pub file: String,
    /// 类别（`tiles`/`sprites`/`ui`/`maps`/`audio`/`data`）。
    pub category: String,
    /// 文件字节数。
    pub bytes: u64,
    /// 整包 sha256（十六进制）。
    pub sha256: String,
    /// 条目数。
    pub entries: u32,
    /// 条目名的公共前缀（便于按库筛选）。
    #[serde(default)]
    pub prefix: String,
}

/// 一条目录项（打开时不读数据区）。
#[derive(Debug, Clone)]
pub struct EntryMeta {
    /// 逻辑名（`gfx/tiles/tiles0/0001.png` 这类 '/' 分隔路径）。
    pub name: String,
    /// 数据种类：0=RGBA+zlib，1=原样。
    pub kind: u8,
    /// 图像元数据（原样字节条目里全 0）。
    pub info: ImageInfo,
    /// 数据在包里的偏移。
    pub data_offset: u64,
    /// 数据在包里的长度（压缩后）。
    pub data_len: u32,
    /// 解压后的长度（原样字节时等于 `data_len`）。
    pub raw_len: u32,
}

/// 一个已打开的小包。
#[derive(Debug)]
pub struct PackFile {
    path: PathBuf,
    file: std::sync::Mutex<File>,
    entries: BTreeMap<String, EntryMeta>,
}

impl PackFile {
    /// 打开并只读条目表（不解压任何数据）。
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
        let header = read_exact_at(&mut file, &path, 0, HEADER_LEN as usize)?;
        if header[0..4] != MAGIC {
            return Err(AssetError::Corrupt {
                path: path.clone(),
                offset: 0,
                message: format!("不是 MNP 包（magic={:?}）", &header[0..4]),
            });
        }
        let version = u16::from_le_bytes([header[4], header[5]]);
        if version != VERSION {
            return Err(AssetError::Corrupt {
                path: path.clone(),
                offset: 4,
                message: format!("MNP 版本 {version} 不受支持（本实现只认 {VERSION}）"),
            });
        }
        let flags = u16::from_le_bytes([header[6], header[7]]);
        let entry_count = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
        let index_offset = u64::from_le_bytes([
            header[12], header[13], header[14], header[15], header[16], header[17], header[18],
            header[19],
        ]);
        if flags & 1 != 0 {
            if total < TRAILER_LEN {
                return Err(AssetError::Corrupt {
                    path: path.clone(),
                    offset: 0,
                    message: "包太短，缺 sha256 尾".to_owned(),
                });
            }
            let mut tail = [0u8; TRAILER_LEN as usize];
            read_exact_into(&mut file, &path, total - TRAILER_LEN, &mut tail)?;
            let actual = sha256_range(&path, total - TRAILER_LEN)?;
            if tail != actual {
                return Err(AssetError::Corrupt {
                    path: path.clone(),
                    offset: total - TRAILER_LEN,
                    message: "包尾 sha256 与内容不符（文件被改动过）".to_owned(),
                });
            }
        }
        let table_len = total
            .saturating_sub(TRAILER_LEN)
            .saturating_sub(index_offset) as usize;
        let table = read_exact_at(&mut file, &path, index_offset, table_len)?;
        let mut entries = BTreeMap::new();
        let mut cursor = 0usize;
        for _ in 0..entry_count {
            let (name, meta, consumed) = parse_entry(&table, cursor, &path)?;
            cursor += consumed;
            entries.insert(name, meta);
        }
        Ok(Self {
            path,
            file: std::sync::Mutex::new(file),
            entries,
        })
    }

    /// 包路径。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 条目数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否空包。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 全部条目名（升序）。
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// 条目元数据。
    #[must_use]
    pub fn meta(&self, name: &str) -> Option<&EntryMeta> {
        self.entries.get(name)
    }

    /// 读整条数据（图像条目会解压成 RGBA）。
    pub fn data(&self, name: &str) -> Result<Vec<u8>, AssetError> {
        let meta = self.entries.get(name).ok_or_else(|| AssetError::Corrupt {
            path: self.path.clone(),
            offset: 0,
            message: format!("包里没有条目 {name}"),
        })?;
        let mut raw = vec![0u8; meta.data_len as usize];
        let mut file = self
            .file
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        read_exact_into(&mut file, &self.path, meta.data_offset, &mut raw)?;
        if meta.kind == KIND_RGBA_ZLIB {
            let decoded = zlib_decompress(&raw, &self.path)?;
            if decoded.len() != meta.raw_len as usize {
                return Err(AssetError::Corrupt {
                    path: self.path.clone(),
                    offset: meta.data_offset,
                    message: format!(
                        "{name} 解压后 {} 字节，与目录项声明的 {} 不符",
                        decoded.len(),
                        meta.raw_len
                    ),
                });
            }
            Ok(decoded)
        } else {
            Ok(raw)
        }
    }

    /// 取一张图（RGBA 条目）。
    pub fn image(&self, name: &str) -> Result<Image, AssetError> {
        let meta = self.entries.get(name).ok_or_else(|| AssetError::Corrupt {
            path: self.path.clone(),
            offset: 0,
            message: format!("包里没有条目 {name}"),
        })?;
        if meta.kind != KIND_RGBA_ZLIB {
            return Err(AssetError::Corrupt {
                path: self.path.clone(),
                offset: meta.data_offset,
                message: format!("{name} 不是图像条目（kind={}）", meta.kind),
            });
        }
        let pixels = self.data(name)?;
        Ok(Image {
            info: meta.info,
            pixels,
        })
    }
}

fn parse_entry(
    table: &[u8],
    cursor: usize,
    path: &Path,
) -> Result<(String, EntryMeta, usize), AssetError> {
    let mut at = cursor;
    let need = |at: usize, n: usize| -> Result<(), AssetError> {
        if at + n > table.len() {
            Err(AssetError::Corrupt {
                path: path.to_path_buf(),
                offset: at as u64,
                message: "条目表被截断".to_owned(),
            })
        } else {
            Ok(())
        }
    };
    need(at, 2)?;
    let name_len = u16::from_le_bytes([table[at], table[at + 1]]) as usize;
    at += 2;
    need(at, name_len)?;
    let name = String::from_utf8(table[at..at + name_len].to_vec()).map_err(|error| {
        AssetError::Corrupt {
            path: path.to_path_buf(),
            offset: at as u64,
            message: format!("条目名不是 UTF-8：{error}"),
        }
    })?;
    at += name_len;
    need(at, 1 + 1 + 1 + 3 + 2 + 2 + 2 + 2 + 2 + 2 + 8 + 4 + 4)?;
    let kind = table[at];
    let has_mask = table[at + 1] != 0;
    let format = table[at + 2];
    let reserved = [table[at + 3], table[at + 4], table[at + 5]];
    let width = u16::from_le_bytes([table[at + 6], table[at + 7]]);
    let height = u16::from_le_bytes([table[at + 8], table[at + 9]]);
    let offset_x = i16::from_le_bytes([table[at + 10], table[at + 11]]);
    let offset_y = i16::from_le_bytes([table[at + 12], table[at + 13]]);
    let shadow_x = i16::from_le_bytes([table[at + 14], table[at + 15]]);
    let shadow_y = i16::from_le_bytes([table[at + 16], table[at + 17]]);
    let data_offset = u64::from_le_bytes([
        table[at + 18],
        table[at + 19],
        table[at + 20],
        table[at + 21],
        table[at + 22],
        table[at + 23],
        table[at + 24],
        table[at + 25],
    ]);
    let data_len = u32::from_le_bytes([
        table[at + 26],
        table[at + 27],
        table[at + 28],
        table[at + 29],
    ]);
    let raw_len = u32::from_le_bytes([
        table[at + 30],
        table[at + 31],
        table[at + 32],
        table[at + 33],
    ]);
    at += 34;
    let info = ImageInfo {
        width,
        height,
        offset_x,
        offset_y,
        format,
        shadow_x,
        shadow_y,
        reserved,
        has_mask,
        data_offset,
        data_len,
    };
    Ok((
        name,
        EntryMeta {
            name: String::new(),
            kind,
            info,
            data_offset,
            data_len,
            raw_len,
        },
        at - cursor,
    ))
}

/// 写一个包（顺序追加，最后 `finish` 写条目表与 sha256）。
pub struct PackWriter {
    path: PathBuf,
    file: File,
    entries: Vec<(String, EntryMeta)>,
    /// 是否压缩 RGBA（默认 true）。
    compress: bool,
    /// 已写入的字节数（自己记账，不依赖 `metadata()` 的可见性）。
    offset: u64,
}

impl PackWriter {
    /// 新建包文件（父目录必须已存在）。
    pub fn create(path: impl AsRef<Path>, compress: bool) -> Result<Self, AssetError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|source| AssetError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
        }
        let mut file = File::create(&path).map_err(|source| AssetError::Io {
            path: path.clone(),
            source,
        })?;
        let mut header = Vec::with_capacity(HEADER_LEN as usize);
        header.extend_from_slice(&MAGIC);
        header.extend_from_slice(&VERSION.to_le_bytes());
        header.extend_from_slice(&1u16.to_le_bytes()); // flags: 有 sha256 尾
        header.extend_from_slice(&0u32.to_le_bytes()); // entry_count 占位
        header.extend_from_slice(&0u64.to_le_bytes()); // index_offset 占位
        header.extend_from_slice(&[0u8; 12]);
        file.write_all(&header).map_err(|source| AssetError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(Self {
            path,
            file,
            entries: Vec::new(),
            compress,
            offset: HEADER_LEN,
        })
    }

    /// 当前写入的字节数（含包头）。
    pub fn current_len(&self) -> u64 {
        self.offset
    }

    /// 加一张 RGBA 图。
    pub fn add_image(
        &mut self,
        name: &str,
        info: &ImageInfo,
        pixels: &[u8],
    ) -> Result<(), AssetError> {
        let data = if self.compress {
            zlib_compress(pixels, &self.path)?
        } else {
            pixels.to_vec()
        };
        self.push_entry(name, KIND_RGBA_ZLIB, info, &data, pixels.len())?;
        Ok(())
    }

    /// 加一条原样字节（`.map`/`.wav`/`.MirDB`）。
    pub fn add_raw(&mut self, name: &str, bytes: &[u8]) -> Result<(), AssetError> {
        let empty = ImageInfo {
            width: 0,
            height: 0,
            offset_x: 0,
            offset_y: 0,
            format: 0,
            shadow_x: 0,
            shadow_y: 0,
            reserved: [0; 3],
            has_mask: false,
            data_offset: 0,
            data_len: 0,
        };
        self.push_entry(name, KIND_RAW, &empty, bytes, bytes.len())?;
        Ok(())
    }

    fn push_entry(
        &mut self,
        name: &str,
        kind: u8,
        info: &ImageInfo,
        data: &[u8],
        raw_len: usize,
    ) -> Result<(), AssetError> {
        let offset = self.offset;
        self.file.write_all(data).map_err(|source| AssetError::Io {
            path: self.path.clone(),
            source,
        })?;
        self.offset = self.offset.saturating_add(data.len() as u64);
        self.entries.push((
            name.to_owned(),
            EntryMeta {
                name: name.to_owned(),
                kind,
                info: *info,
                data_offset: offset,
                data_len: u32::try_from(data.len()).unwrap_or(u32::MAX),
                raw_len: u32::try_from(raw_len).unwrap_or(u32::MAX),
            },
        ));
        Ok(())
    }

    /// 写完条目表、回填包头、追加 sha256；返回包摘要。
    pub fn finish(mut self, category: &str) -> Result<PackSummary, AssetError> {
        let index_offset = self.offset;
        let mut table = Vec::new();
        for (name, meta) in &self.entries {
            let name_bytes = name.as_bytes();
            table.extend_from_slice(
                &u16::try_from(name_bytes.len())
                    .unwrap_or(u16::MAX)
                    .to_le_bytes(),
            );
            table.extend_from_slice(name_bytes);
            table.push(meta.kind);
            table.push(u8::from(meta.info.has_mask));
            table.push(meta.info.format);
            table.extend_from_slice(&meta.info.reserved);
            table.extend_from_slice(&meta.info.width.to_le_bytes());
            table.extend_from_slice(&meta.info.height.to_le_bytes());
            table.extend_from_slice(&meta.info.offset_x.to_le_bytes());
            table.extend_from_slice(&meta.info.offset_y.to_le_bytes());
            table.extend_from_slice(&meta.info.shadow_x.to_le_bytes());
            table.extend_from_slice(&meta.info.shadow_y.to_le_bytes());
            table.extend_from_slice(&meta.data_offset.to_le_bytes());
            table.extend_from_slice(&meta.data_len.to_le_bytes());
            table.extend_from_slice(&meta.raw_len.to_le_bytes());
        }
        self.file
            .write_all(&table)
            .map_err(|source| AssetError::Io {
                path: self.path.clone(),
                source,
            })?;
        self.offset = self.offset.saturating_add(table.len() as u64);
        // 回填 header
        self.file
            .seek(SeekFrom::Start(8))
            .map_err(|source| AssetError::Io {
                path: self.path.clone(),
                source,
            })?;
        self.file
            .write_all(
                &u32::try_from(self.entries.len())
                    .unwrap_or(u32::MAX)
                    .to_le_bytes(),
            )
            .map_err(|source| AssetError::Io {
                path: self.path.clone(),
                source,
            })?;
        self.file
            .write_all(&index_offset.to_le_bytes())
            .map_err(|source| AssetError::Io {
                path: self.path.clone(),
                source,
            })?;
        // 回填 header 后游标停在 0x14，必须回到文件末尾再写 sha256，
        // 否则会把尾摘要写进数据区（第一次实现就踩了这个坑）。
        self.file
            .seek(SeekFrom::End(0))
            .map_err(|source| AssetError::Io {
                path: self.path.clone(),
                source,
            })?;
        self.file.flush().map_err(|source| AssetError::Io {
            path: self.path.clone(),
            source,
        })?;
        let body_len = self.offset;
        let digest = sha256_range(&self.path, body_len)?;
        self.file
            .write_all(&digest)
            .map_err(|source| AssetError::Io {
                path: self.path.clone(),
                source,
            })?;
        self.file.flush().map_err(|source| AssetError::Io {
            path: self.path.clone(),
            source,
        })?;
        let bytes = self.offset.saturating_add(TRAILER_LEN);
        let prefix = common_prefix(self.entries.iter().map(|(name, _)| name.as_str()));
        Ok(PackSummary {
            file: self
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            category: category.to_owned(),
            bytes,
            sha256: to_hex(&digest),
            entries: u32::try_from(self.entries.len()).unwrap_or(u32::MAX),
            prefix,
        })
    }
}

fn common_prefix<'a>(names: impl Iterator<Item = &'a str>) -> String {
    // 要的是**目录前缀**（`gfx/tiles/tiles0/`），不是字符级公共前缀
    // （`0000.png`/`0001.png` 的字符前缀是 `000`，会把条目名截断）。
    let mut prefix: Option<Vec<&str>> = None;
    for name in names {
        let dirs: Vec<&str> = name
            .rsplit_once('/')
            .map_or_else(Vec::new, |(dir, _)| dir.split('/').collect());
        prefix = Some(match prefix {
            None => dirs,
            Some(current) => current
                .into_iter()
                .zip(dirs)
                .take_while(|(left, right)| left == right)
                .map(|(left, _)| left)
                .collect(),
        });
    }
    let parts = prefix.unwrap_or_default();
    if parts.is_empty() {
        String::new()
    } else {
        format!("{}/", parts.join("/"))
    }
}

/// 一整套包（`manifest.packs.json` + 若干 `.mnp`）。
#[derive(Debug, Clone)]
pub struct PackSet {
    /// 包清单（原样保留，便于诊断）。
    manifest: PackManifest,
    /// 已打开的包（按清单顺序）。
    packs: Vec<Arc<PackFile>>,
    /// 逻辑名 → (包下标, 条目元数据)。
    lookup: BTreeMap<String, (usize, EntryMeta)>,
}

impl PackSet {
    /// 打开 `packs/manifest.packs.json` 及其引用的所有 `.mnp`。
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, AssetError> {
        let dir = dir.as_ref();
        let manifest_path = dir.join("manifest.packs.json");
        let text = std::fs::read_to_string(&manifest_path).map_err(|source| AssetError::Io {
            path: manifest_path.clone(),
            source,
        })?;
        let manifest: PackManifest =
            serde_json::from_str(&text).map_err(|error| AssetError::Corrupt {
                path: manifest_path.clone(),
                offset: 0,
                message: format!("manifest.packs.json 解析失败：{error}"),
            })?;
        let mut packs = Vec::new();
        let mut lookup = BTreeMap::new();
        for summary in &manifest.packs {
            let path = dir.join(&summary.file);
            let pack = Arc::new(PackFile::open(&path)?);
            let index = packs.len();
            for name in pack.names() {
                if let Some(meta) = pack.meta(name) {
                    lookup.insert(name.to_owned(), (index, meta.clone()));
                }
            }
            packs.push(pack);
        }
        Ok(Self {
            manifest,
            packs,
            lookup,
        })
    }

    /// 清单。
    #[must_use]
    pub fn manifest(&self) -> &PackManifest {
        &self.manifest
    }

    /// 包数量。
    #[must_use]
    pub fn pack_count(&self) -> usize {
        self.packs.len()
    }

    /// 条目总数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.lookup.len()
    }

    /// 是否没有条目。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lookup.is_empty()
    }

    /// 全部逻辑名（升序）。
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.lookup.keys().map(String::as_str)
    }

    /// 是否有某个逻辑名。
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.lookup.contains_key(name)
    }

    /// 条目元数据。
    #[must_use]
    pub fn meta(&self, name: &str) -> Option<&EntryMeta> {
        self.lookup.get(name).map(|(_, meta)| meta)
    }

    /// 取一张图。
    pub fn image(&self, name: &str) -> Result<Image, AssetError> {
        let (index, _) = self.lookup.get(name).ok_or_else(|| AssetError::Corrupt {
            path: PathBuf::from("manifest.packs.json"),
            offset: 0,
            message: format!("包集合里没有 {name}"),
        })?;
        self.packs[*index].image(name)
    }

    /// 取一条原样字节。
    pub fn data(&self, name: &str) -> Result<Vec<u8>, AssetError> {
        let (index, _) = self.lookup.get(name).ok_or_else(|| AssetError::Corrupt {
            path: PathBuf::from("manifest.packs.json"),
            offset: 0,
            message: format!("包集合里没有 {name}"),
        })?;
        self.packs[*index].data(name)
    }

    /// 按前缀筛出子集（例如 `gfx/tiles/tiles0/` 当一个库用）。
    #[must_use]
    pub fn filter_prefix(&self, prefix: &str) -> Self {
        let lookup: BTreeMap<String, (usize, EntryMeta)> = self
            .lookup
            .iter()
            .filter(|(name, _)| name.starts_with(prefix))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        Self {
            manifest: self.manifest.clone(),
            packs: self.packs.clone(),
            lookup,
        }
    }

    /// 前缀下按名排序的第 `index` 个逻辑名。
    #[must_use]
    pub fn name_at(&self, index: usize) -> Option<&str> {
        self.lookup.keys().nth(index).map(String::as_str)
    }

    /// 前缀筛出的库：`len/info/image` 按位置索引（与 `UnpackedLibrary` 同形）。
    pub fn library_info(&self, index: usize) -> Option<ImageInfo> {
        self.name_at(index)
            .and_then(|name| self.meta(name))
            .map(|meta| meta.info)
    }

    /// 前缀筛出的库：按位置取图。
    pub fn library_image(&self, index: usize) -> Result<Image, AssetError> {
        let name = self.name_at(index).ok_or_else(|| AssetError::Corrupt {
            path: PathBuf::from("manifest.packs.json"),
            offset: 0,
            message: format!("包集合里第 {index} 个条目不存在"),
        })?;
        self.image(name)
    }
}

fn read_exact_at(
    file: &mut File,
    path: &Path,
    offset: u64,
    len: usize,
) -> Result<Vec<u8>, AssetError> {
    let mut buffer = vec![0u8; len];
    read_exact_into(file, path, offset, &mut buffer)?;
    Ok(buffer)
}

fn read_exact_into(
    file: &mut File,
    path: &Path,
    offset: u64,
    buffer: &mut [u8],
) -> Result<(), AssetError> {
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

fn zlib_compress(bytes: &[u8], path: &Path) -> Result<Vec<u8>, AssetError> {
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes).map_err(|source| AssetError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    encoder.finish().map_err(|source| AssetError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn zlib_decompress(bytes: &[u8], path: &Path) -> Result<Vec<u8>, AssetError> {
    use flate2::read::ZlibDecoder;
    let mut decoder = ZlibDecoder::new(bytes);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|error| AssetError::Compression {
            path: path.to_path_buf(),
            offset: 0,
            message: error.to_string(),
        })?;
    Ok(out)
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

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mir2-mnp-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn sample_info(width: u16, height: u16, offset_x: i16, shadow_x: i16) -> ImageInfo {
        ImageInfo {
            width,
            height,
            offset_x,
            offset_y: 3,
            format: 5,
            shadow_x,
            shadow_y: -2,
            reserved: [0x80, 0, 0],
            has_mask: true,
            data_offset: 0,
            data_len: 0,
        }
    }

    #[test]
    fn pack_round_trips_pixels_and_metadata() {
        let dir = temp_dir("roundtrip");
        let pack_path = dir.join("tiles0.mnp");
        let mut writer = PackWriter::create(&pack_path, true).expect("create");
        let mut expected = Vec::new();
        for index in 0..3 {
            let info = sample_info(4 + index as u16, 3, index as i16, 9);
            let pixels: Vec<u8> = (0..(info.width as usize * info.height as usize * 4))
                .map(|byte| u8::try_from(byte % 251).unwrap_or(0))
                .map(|byte| byte.wrapping_add(index as u8))
                .collect();
            writer
                .add_image(&format!("gfx/tiles/tiles0/{index:04}.png"), &info, &pixels)
                .expect("add");
            expected.push((info, pixels));
        }
        let summary = writer.finish("tiles").expect("finish");
        assert_eq!(summary.entries, 3);
        assert_eq!(summary.prefix, "gfx/tiles/tiles0/");

        let pack = PackFile::open(&pack_path).expect("open");
        assert_eq!(pack.len(), 3);
        for (index, (info, pixels)) in expected.iter().enumerate() {
            let name = format!("gfx/tiles/tiles0/{index:04}.png");
            let meta = pack.meta(&name).expect("meta");
            assert_eq!(meta.info.width, info.width);
            assert_eq!(meta.info.offset_x, info.offset_x);
            assert_eq!(meta.info.shadow_x, info.shadow_x);
            assert!(meta.info.has_mask);
            let image = pack.image(&name).expect("image");
            // `data_offset`/`data_len` 是**存储位置**（包里偏移与压缩后长度），
            // 不属于素材语义；其余字段必须逐字段一致。
            let mut expected_info = *info;
            expected_info.data_offset = image.info.data_offset;
            expected_info.data_len = image.info.data_len;
            assert_eq!(image.info, expected_info, "元数据必须逐字段一致");
            assert_eq!(image.pixels, *pixels, "像素必须逐字节一致");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tampered_pack_is_detected_by_sha256() {
        let dir = temp_dir("tamper");
        let pack_path = dir.join("ui.mnp");
        let mut writer = PackWriter::create(&pack_path, false).expect("create");
        let info = sample_info(2, 2, 0, 0);
        writer
            .add_image("gfx/ui/panel/0000.png", &info, &[1u8; 16])
            .expect("add");
        writer.finish("ui").expect("finish");

        // 改一个字节（在数据区）→ 打开必须报 sha256 不符
        let mut bytes = std::fs::read(&pack_path).expect("read");
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0xFF;
        std::fs::write(&pack_path, &bytes).expect("write");
        let error = PackFile::open(&pack_path).expect_err("篡改必须被发现");
        assert!(
            format!("{error}").contains("sha256"),
            "错误信息要指出 sha256 不符，实际：{error}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 端到端：真实库取 3 张 → 写散装（PNG + index.json）→ 打包 → 读包 → 与**原库**逐字节比较。
    #[test]
    fn packed_images_match_source_library_pixels() {
        let library_path = Path::new("../../resource/data/items.wzl");
        if !library_path.is_file() {
            eprintln!("skip: no resource/data/items.wzl");
            return;
        }
        let library = crate::ImageLibrary::open(library_path).expect("open library");
        let dir = temp_dir("e2e");
        let packs_dir = dir.join("packs");
        let mut writer =
            PackWriter::create(packs_dir.join("sprites-item-1.mnp"), true).expect("create");
        for index in 0..3usize {
            let image = library.image(index).expect("decode");
            assert!(!image.info.is_empty(), "样例图不能是空帧");
            writer
                .add_image(
                    &format!("gfx/sprites/item/icon-{index:04}/0000.png"),
                    &image.info,
                    &image.pixels,
                )
                .expect("add");
        }
        let summary = writer.finish("sprites").expect("finish");
        assert_eq!(summary.entries, 3);
        let manifest = PackManifest {
            version: 1,
            generated_at: "epoch+0s".to_owned(),
            chunk_mb: 16,
            packs: vec![summary],
        };
        std::fs::write(
            packs_dir.join("manifest.packs.json"),
            serde_json::to_string_pretty(&manifest).expect("json"),
        )
        .expect("write manifest");

        let set = PackSet::open(&packs_dir).expect("open set");
        assert_eq!(set.len(), 3);
        for index in 0..3usize {
            let name = format!("gfx/sprites/item/icon-{index:04}/0000.png");
            let packed = set.image(&name).expect("packed image");
            let source = library.image(index).expect("source image");
            assert_eq!(
                packed.pixels, source.pixels,
                "第 {index} 张像素必须与源库逐字节一致"
            );
            assert_eq!(packed.info.width, source.info.width);
            assert_eq!(packed.info.height, source.info.height);
            assert_eq!(packed.info.offset_x, source.info.offset_x);
            assert_eq!(packed.info.offset_y, source.info.offset_y);
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn pack_set_reads_manifest_and_filters_by_prefix() {
        let dir = temp_dir("set");
        let packs_dir = dir.join("packs");
        let mut summaries = Vec::new();
        for (index, category) in ["tiles", "sprites"].iter().enumerate() {
            let path = packs_dir.join(format!("{category}-1.mnp"));
            let mut writer = PackWriter::create(&path, true).expect("create");
            let info = sample_info(2, 2, 0, 0);
            writer
                .add_image(
                    &format!("gfx/{category}/lib{index}/0000.png"),
                    &info,
                    &[u8::try_from(index).unwrap_or(0); 16],
                )
                .expect("add");
            summaries.push(writer.finish(category).expect("finish"));
        }
        let manifest = PackManifest {
            version: 1,
            generated_at: "2026-10-22T00:00:00+08:00".to_owned(),
            chunk_mb: 16,
            packs: summaries,
        };
        std::fs::write(
            packs_dir.join("manifest.packs.json"),
            serde_json::to_string_pretty(&manifest).expect("json"),
        )
        .expect("write manifest");

        let set = PackSet::open(&packs_dir).expect("open set");
        assert_eq!(set.pack_count(), 2);
        assert_eq!(set.len(), 2);
        assert!(set.contains("gfx/tiles/lib0/0000.png"));

        let tiles = set.filter_prefix("gfx/tiles/");
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles.name_at(0), Some("gfx/tiles/lib0/0000.png"));
        let info = tiles.library_info(0).expect("info");
        assert_eq!(info.width, 2);
        let image = tiles.library_image(0).expect("image");
        assert_eq!(image.pixels, vec![0u8; 16]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
