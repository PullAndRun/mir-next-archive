//! 拆包目录（`index.json` + 每图一张 PNG）的只读素材库。
//!
//! `mir-tools assets unpack` 把 `.wzl/.wil/.Lib` 拆成：
//!
//! ```text
//! <dir>/
//!   index.json    每图一条：index/宽高/偏移/阴影/是否有 mask/像素 sha256/png 文件名
//!   0000.png …    RGBA8（透明保留，不做预乘）
//! ```
//!
//! 本模块把这种目录读回成与 [`ImageLibrary`](crate::ImageLibrary) **同形**的库
//! （`len` / `info` / `image`），让渲染层无感切换；库里**没有**的索引会返回错误，
//! 调用方据此回退到原始 `.wzl`（保证没拆包/没装素材时仍能跑）。
//!
//! 索引语义：`info(index)` / `image(index)` 的 `index` 是**原库内的 0 基索引**，
//! 不是拆包目录里的数组下标——这样裁剪（只拆一部分）之后调用方的编号不用换算。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::AssetError;
use crate::image::{Image, ImageInfo};

/// `index.json` 顶层。
#[derive(Debug, Deserialize)]
struct IndexFile {
    /// 原库文件名（`.wzl` / `.Lib` / `.wil`）。
    #[serde(default)]
    lib: Option<String>,
    /// 原库路径（拆包时的来源，仅记录）。
    #[serde(default)]
    source: Option<String>,
    /// 为什么收这个库（引用分析给的说明，仅记录）。
    #[serde(default)]
    reason: Option<String>,
    /// 每图一条。
    images: Vec<Entry>,
}

/// `index.json` 里的一条图。
#[derive(Debug, Deserialize)]
struct Entry {
    /// 原库内的 0 基索引。
    index: u32,
    /// PNG 文件名；空图没有文件。
    #[serde(default)]
    png: Option<String>,
    width: u16,
    height: u16,
    offset_x: i16,
    offset_y: i16,
    #[serde(default)]
    shadow_x: i16,
    #[serde(default)]
    shadow_y: i16,
    #[serde(default)]
    has_mask: bool,
    #[serde(default)]
    format_code: u8,
    #[serde(default)]
    reserved: [u8; 3],
    #[serde(default)]
    data_offset: u64,
    #[serde(default)]
    data_len: u32,
    #[serde(default)]
    empty: bool,
}

impl Entry {
    fn to_info(&self) -> ImageInfo {
        ImageInfo {
            width: self.width,
            height: self.height,
            offset_x: self.offset_x,
            offset_y: self.offset_y,
            format: self.format_code,
            shadow_x: self.shadow_x,
            shadow_y: self.shadow_y,
            reserved: self.reserved,
            has_mask: self.has_mask,
            data_offset: self.data_offset,
            data_len: self.data_len,
        }
    }
}

/// 拆包目录作为一个只读素材库。
#[derive(Debug)]
pub struct UnpackedLibrary {
    dir: PathBuf,
    /// 原库索引 → 条目（稀疏；只包含拆出来的那些图）。
    entries: BTreeMap<usize, Entry>,
    /// 原库文件名（日志/错误信息用）。
    lib: Option<String>,
    /// 拆包时的来源路径（`index.json` 里记的，仅诊断用）。
    source: Option<String>,
    /// 引用分析给出的收录理由（仅诊断用）。
    reason: Option<String>,
}

impl UnpackedLibrary {
    /// 打开一个拆包目录（必须有 `index.json`）。
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, AssetError> {
        let dir = dir.as_ref();
        let index_path = dir.join("index.json");
        let text = std::fs::read_to_string(&index_path).map_err(|source| AssetError::Io {
            path: index_path.clone(),
            source,
        })?;
        let parsed: IndexFile =
            serde_json::from_str(&text).map_err(|error| AssetError::UnpackedIndex {
                path: index_path.clone(),
                message: error.to_string(),
            })?;
        let mut entries = BTreeMap::new();
        for entry in parsed.images {
            entries.insert(entry.index as usize, entry);
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            entries,
            lib: parsed.lib,
            source: parsed.source,
            reason: parsed.reason,
        })
    }

    /// 拆包目录路径。
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// 原库文件名（`items.wzl`）。
    #[must_use]
    pub fn lib_name(&self) -> Option<&str> {
        self.lib.as_deref()
    }

    /// 拆包时的来源路径（`index.json` 记录，仅诊断用）。
    #[must_use]
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// 引用分析给出的收录理由（`index.json` 记录，仅诊断用）。
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    /// 拆出来的图数量（可能小于原库，因为只拆了一部分）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否一张图都没拆。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 原库的索引是否在本目录里。
    #[must_use]
    pub fn contains(&self, index: usize) -> bool {
        self.entries.contains_key(&index)
    }

    /// 已拆出来的原库索引（升序）。
    pub fn indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.entries.keys().copied()
    }

    /// 单张图的头信息（不解码像素）。
    pub fn info(&self, index: usize) -> Result<ImageInfo, AssetError> {
        let entry = self.entry(index)?;
        Ok(entry.to_info())
    }

    /// 解码单张图：读 PNG 得到 RGBA8 像素。
    pub fn image(&self, index: usize) -> Result<Image, AssetError> {
        let entry = self.entry(index)?;
        if entry.empty || entry.png.is_none() {
            return Ok(Image {
                info: entry.to_info(),
                pixels: Vec::new(),
            });
        }
        let png_name = entry.png.as_deref().unwrap_or_default();
        let path = self.dir.join(png_name);
        let pixels = read_png_rgba(&path)?;
        let expected = usize::from(entry.width) * usize::from(entry.height) * 4;
        if pixels.len() != expected {
            return Err(AssetError::Corrupt {
                path,
                offset: 0,
                message: format!(
                    "PNG 解码后 {} 字节，与 index.json 的 {}×{}×4={} 不符",
                    pixels.len(),
                    entry.width,
                    entry.height,
                    expected
                ),
            });
        }
        Ok(Image {
            info: entry.to_info(),
            pixels,
        })
    }

    fn entry(&self, index: usize) -> Result<&Entry, AssetError> {
        self.entries
            .get(&index)
            .ok_or_else(|| AssetError::MissingIndex {
                path: self.dir.join("index.json"),
                index,
            })
    }
}

/// 一个图像库的来源：原始容器（`.wzl/.wil/.Lib`）或拆包目录。
///
/// 渲染层用它在两者之间**无感切换**：优先 `<gfx>/<stem>/index.json`（拆包目录），
/// 没有就退回原始 `.wzl`。这样不装素材/没拆包的机器仍能跑测试与演示。
#[derive(Debug)]
pub enum LibrarySource {
    /// 原始容器文件。
    Packed(crate::ImageLibrary),
    /// 拆包目录（`index.json` + PNG）。
    Unpacked(UnpackedLibrary),
    /// 发布期小包（`.mnp`，已按某个前缀筛成一个库）。
    Pack(crate::PackSet),
}

impl LibrarySource {
    /// 打开一个库：`gfx_dir` 下有同名拆包目录就优先用它，否则回退原始文件。
    ///
    /// 拆包目录打不开（`index.json` 坏了）时打一条告警并回退，**不让渲染失败**。
    pub fn open_with_gfx(path: &Path, gfx_dir: Option<&Path>) -> Result<Self, AssetError> {
        if let Some(gfx_dir) = gfx_dir {
            if let Some(stem) = path.file_stem() {
                let unpacked_dir = gfx_dir.join(stem);
                if unpacked_dir.join("index.json").is_file() {
                    match UnpackedLibrary::open(&unpacked_dir) {
                        Ok(library) => return Ok(Self::Unpacked(library)),
                        Err(error) => {
                            eprintln!(
                                "警告：拆包目录 {} 打不开（{error}），回退原始库 {}",
                                unpacked_dir.display(),
                                path.display()
                            );
                        }
                    }
                }
            }
        }
        Ok(Self::Packed(crate::ImageLibrary::open(path)?))
    }

    /// 从一整套小包里筛出某个库（`prefix` 形如 `gfx/tiles/tiles0/`）。
    #[must_use]
    pub fn from_packs(set: &crate::PackSet, prefix: &str) -> Self {
        Self::Pack(set.filter_prefix(prefix))
    }

    /// 是否来自拆包目录。
    #[must_use]
    pub fn is_unpacked(&self) -> bool {
        matches!(self, Self::Unpacked(_))
    }

    /// 是否来自发布期小包。
    #[must_use]
    pub fn is_pack(&self) -> bool {
        matches!(self, Self::Pack(_))
    }

    /// 图像数量（拆包目录返回**已拆出**的图数）。
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Packed(library) => library.len(),
            Self::Unpacked(library) => library.len(),
            Self::Pack(set) => set.len(),
        }
    }

    /// 是否一张图都没有。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 单张图的头信息。
    pub fn info(&self, index: usize) -> Result<ImageInfo, AssetError> {
        match self {
            Self::Packed(library) => library.info(index),
            Self::Unpacked(library) => library.info(index),
            Self::Pack(set) => set
                .library_info(index)
                .ok_or_else(|| AssetError::MissingIndex {
                    path: PathBuf::from("manifest.packs.json"),
                    index,
                }),
        }
    }

    /// 解码单张图。
    pub fn image(&self, index: usize) -> Result<Image, AssetError> {
        match self {
            Self::Packed(library) => library.image(index),
            Self::Unpacked(library) => library.image(index),
            Self::Pack(set) => set.library_image(index),
        }
    }
}

/// 读 PNG 成 RGBA8（拆包器写的就是 RGBA8，这里严格一点，别的形态直接报错）。
fn read_png_rgba(path: &Path) -> Result<Vec<u8>, AssetError> {
    let file = std::fs::File::open(path).map_err(|source| AssetError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().map_err(|error| AssetError::Corrupt {
        path: path.to_path_buf(),
        offset: 0,
        message: format!("PNG 头解码失败：{error}"),
    })?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| AssetError::Corrupt {
            path: path.to_path_buf(),
            offset: 0,
            message: "PNG 输出缓冲大小未知".to_owned(),
        })?;
    let mut buffer = vec![0u8; size];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| AssetError::Corrupt {
            path: path.to_path_buf(),
            offset: 0,
            message: format!("PNG 像素解码失败：{error}"),
        })?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err(AssetError::Corrupt {
            path: path.to_path_buf(),
            offset: 0,
            message: format!(
                "PNG 是 {:?}/{:?}，拆包目录只接受 RGBA8",
                info.color_type, info.bit_depth
            ),
        });
    }
    buffer.truncate(info.buffer_size());
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ImageLibrary;

    /// 仓库里的样例库（CI 没有素材时跳过）。
    fn sample_library() -> Option<PathBuf> {
        let path = Path::new("../../resource/data/items.wzl");
        path.is_file().then(|| path.to_path_buf())
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mir2-unpacked-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_png(path: &Path, width: u16, height: u16, pixels: &[u8]) {
        let file = std::fs::File::create(path).expect("create png");
        let mut encoder = png::Encoder::new(file, u32::from(width), u32::from(height));
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("png header");
        writer.write_image_data(pixels).expect("png data");
    }

    fn write_index(dir: &Path, images: &[(usize, ImageInfo, Option<&str>)]) {
        let entries: Vec<serde_json::Value> = images
            .iter()
            .map(|(index, info, png)| {
                serde_json::json!({
                    "index": index,
                    "png": png,
                    "width": info.width,
                    "height": info.height,
                    "offset_x": info.offset_x,
                    "offset_y": info.offset_y,
                    "shadow_x": info.shadow_x,
                    "shadow_y": info.shadow_y,
                    "has_mask": info.has_mask,
                    "format_code": info.format,
                    "reserved": info.reserved,
                    "data_offset": info.data_offset,
                    "data_len": info.data_len,
                    "empty": png.is_none(),
                })
            })
            .collect();
        let index = serde_json::json!({
            "version": 1,
            "lib": "items.wzl",
            "count": entries.len(),
            "images": entries,
        });
        std::fs::write(
            dir.join("index.json"),
            serde_json::to_string_pretty(&index).expect("json"),
        )
        .expect("write index");
    }

    #[test]
    fn unpacked_pixels_match_source_library() {
        let Some(library_path) = sample_library() else {
            eprintln!("skip: 没有 resource/data/items.wzl");
            return;
        };
        let library = ImageLibrary::open(&library_path).expect("open library");
        let dir = temp_dir("roundtrip");
        let mut written = Vec::new();
        for index in 0..3 {
            let image = library.image(index).expect("decode source");
            let png_name = format!("{index:04}.png");
            write_png(
                &dir.join(&png_name),
                image.info.width,
                image.info.height,
                &image.pixels,
            );
            written.push((index, image.info, Some(png_name)));
        }
        let borrowed: Vec<(usize, ImageInfo, Option<&str>)> = written
            .iter()
            .map(|(index, info, png)| (*index, *info, png.as_deref()))
            .collect();
        write_index(&dir, &borrowed);

        let unpacked = UnpackedLibrary::open(&dir).expect("open unpacked");
        assert_eq!(unpacked.len(), 3);
        assert_eq!(unpacked.indices().collect::<Vec<_>>(), vec![0, 1, 2]);
        for index in 0..3 {
            let source = library.image(index).expect("source");
            let restored = unpacked.image(index).expect("unpacked");
            assert_eq!(
                restored.info, source.info,
                "第 {index} 张的 ImageInfo 必须一致"
            );
            assert_eq!(
                restored.pixels, source.pixels,
                "第 {index} 张的像素必须逐字节相等"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unpacked_round_trips_shadow_metadata() {
        let dir = temp_dir("shadow");
        let info = ImageInfo {
            width: 4,
            height: 3,
            offset_x: 2,
            offset_y: -5,
            format: crate::image::format::RGB24,
            shadow_x: 9,
            shadow_y: -7,
            reserved: [0x80, 0, 0],
            has_mask: true,
            data_offset: 1234,
            data_len: 56,
        };
        let pixels = vec![0u8; info.width as usize * info.height as usize * 4];
        let png_name = "0000.png";
        write_png(&dir.join(png_name), info.width, info.height, &pixels);
        write_index(&dir, &[(0, info, Some(png_name))]);

        let unpacked = UnpackedLibrary::open(&dir).expect("open");
        let restored = unpacked.info(0).expect("info");
        assert_eq!(restored, info, "shadow/mask/偏移必须原样还原");
        assert!(restored.has_mask);
        assert_eq!(restored.shadow_x, 9);
        assert_eq!(restored.shadow_y, -7);

        // ImageInfo 是 Copy：改副本不影响已还原值，避免"只比较了同一个变量"的假绿
        let mut changed = restored;
        changed.shadow_x = 0;
        assert_eq!(restored.shadow_x, 9);
        assert_eq!(changed.shadow_x, 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn library_source_can_read_from_mnp_packs() {
        let dir = temp_dir("pack-source");
        let packs_dir = dir.join("packs");
        let mut writer = crate::PackWriter::create(packs_dir.join("tiles-tiles0-1.mnp"), true)
            .expect("create pack");
        for index in 0..2u16 {
            let info = ImageInfo {
                width: 2,
                height: 2,
                offset_x: 0,
                offset_y: 0,
                format: crate::image::format::RGB565,
                shadow_x: 0,
                shadow_y: 0,
                reserved: [0; 3],
                has_mask: false,
                data_offset: 0,
                data_len: 0,
            };
            let pixels = vec![u8::try_from(index).unwrap_or(0); 16];
            writer
                .add_image(&format!("gfx/tiles/tiles0/{index:04}.png"), &info, &pixels)
                .expect("add");
        }
        let summary = writer.finish("tiles").expect("finish");
        std::fs::write(
            packs_dir.join("manifest.packs.json"),
            serde_json::to_string_pretty(&crate::PackManifest {
                version: 1,
                generated_at: "epoch+0s".to_owned(),
                chunk_mb: 16,
                packs: vec![summary],
            })
            .expect("json"),
        )
        .expect("manifest");

        let set = crate::PackSet::open(&packs_dir).expect("open set");
        let library = LibrarySource::from_packs(&set, "gfx/tiles/tiles0/");
        assert!(library.is_pack(), "from_packs 必须产出 Pack 变体");
        assert_eq!(library.len(), 2);
        let info = library.info(0).expect("info");
        assert_eq!((info.width, info.height), (2, 2));
        let image = library.image(1).expect("image");
        assert_eq!(image.pixels, vec![1u8; 16]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn library_source_prefers_unpacked_and_falls_back() {
        let Some(library_path) = sample_library() else {
            eprintln!("skip: no resource/data/items.wzl");
            return;
        };
        let library = ImageLibrary::open(&library_path).expect("open library");
        let root = temp_dir("switch");
        let gfx = root.join("gfx");
        let unpacked_dir = gfx.join("items");
        std::fs::create_dir_all(&unpacked_dir).expect("mkdir");
        let mut written = Vec::new();
        for index in 0..3 {
            let image = library.image(index).expect("decode");
            let png_name = format!("{index:04}.png");
            write_png(
                &unpacked_dir.join(&png_name),
                image.info.width,
                image.info.height,
                &image.pixels,
            );
            written.push((index, image.info, png_name));
        }
        let borrowed: Vec<(usize, ImageInfo, Option<&str>)> = written
            .iter()
            .map(|(index, info, png)| (*index, *info, Some(png.as_str())))
            .collect();
        write_index(&unpacked_dir, &borrowed);

        // 有拆包目录 → 走拆包
        let preferred =
            LibrarySource::open_with_gfx(&library_path, Some(gfx.as_path())).expect("open");
        assert!(
            preferred.is_unpacked(),
            "有 gfx/<stem>/index.json 时应优先拆包"
        );
        for index in 0..3 {
            assert_eq!(
                preferred.image(index).expect("unpacked").pixels,
                library.image(index).expect("packed").pixels,
                "第 {index} 张像素必须一致"
            );
        }

        // 没有拆包目录 → 回退原始 .wzl
        let fallback = LibrarySource::open_with_gfx(&library_path, None).expect("open");
        assert!(!fallback.is_unpacked(), "没有 gfx 时必须回退 .wzl");
        assert_eq!(fallback.len(), library.len());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_index_is_an_error_so_callers_can_fall_back() {
        let dir = temp_dir("partial");
        write_index(&dir, &[]);
        let unpacked = UnpackedLibrary::open(&dir).expect("open");
        assert!(!unpacked.contains(7));
        let error = unpacked.image(7).expect_err("缺席的索引必须报错");
        assert!(
            matches!(error, AssetError::MissingIndex { index: 7, .. }),
            "错误类型要能区分 『缺席』 与 『损坏』，实际 {error:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
