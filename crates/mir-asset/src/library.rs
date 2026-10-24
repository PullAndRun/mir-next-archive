//! 容器识别与统一入口 [`ImageLibrary`]。
//!
//! 识别顺序：
//!
//! 1. 扩展名 `.lib` → 旧版 `.Lib`（头里的版本号必须存在且为 2/3，否则打开失败）。
//! 2. 扩展名 `.wzl` / `.wzx` → `.wzl` + `.wzx`，缺哪个都会报
//!    [`AssetError::MissingCompanion`]（`WeMadeLibrary.cs:63-64` 要求两个文件同时存在）。
//! 3. 其它扩展名：先按 `.Lib` 的文件头嗅探（版本 2/3 且偏移表放得下），再看有没有同名
//!    `.wzx`，都不像就报 [`AssetError::BadMagic`]。
//!
//! 大小写不敏感地找配套文件：Windows 上 `.WZX` 与 `.wzx` 是同一个文件，Linux 上不是，
//! 所以先试精确路径，再扫一遍目录做不区分大小写的匹配。

use std::path::{Path, PathBuf};

use crate::AssetError;
use crate::image::{Image, ImageInfo};
use crate::lib_container::LibReader;
use crate::wil::WilReader;
use crate::wzl::WzlReader;

/// 素材库容器类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LibraryKind {
    /// 旧版单文件 `.Lib`（`Client/MirGraphics/MLibrary.cs`）。
    Lib,
    /// `.wzl` 数据文件 + `.wzx` 索引文件（`LibraryEditor/Graphics/WeMadeLibrary.cs`）。
    Wzl,
    /// `.wil` 数据文件 + `.wix` 索引文件（WEMADE 旧版 16 位 RGB565 库）。
    Wil,
}

impl LibraryKind {
    /// 稳定的小写名称，CLI 输出用。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lib => "lib",
            Self::Wzl => "wzl",
            Self::Wil => "wil",
        }
    }
}

/// 具体容器实现。
#[derive(Debug)]
enum Backend {
    Lib(LibReader),
    Wzl(WzlReader),
    Wil(WilReader),
}

/// 打开的图像库。
///
/// 打开时只读文件头/索引，图像数据在 [`ImageLibrary::info`] / [`ImageLibrary::image`]
/// 调用时才按需读取，所以打开 300 MB 的 `.wzl` 也是毫秒级。
///
/// ```no_run
/// use mir_asset::ImageLibrary;
///
/// let library = ImageLibrary::open("resource/data/anitiles1.wzl")?;
/// let image = library.image(0)?;
/// assert_eq!(
///     image.pixels.len(),
///     usize::from(image.info.width) * usize::from(image.info.height) * 4
/// );
/// # Ok::<(), mir_asset::AssetError>(())
/// ```
#[derive(Debug)]
pub struct ImageLibrary {
    /// 实际打开的数据文件（`.wzl` 时是 `.wzl` 的路径，不是 `.wzx`）。
    path: PathBuf,
    /// 容器类型。
    kind: LibraryKind,
    /// 容器实现。
    backend: Backend,
}

impl ImageLibrary {
    /// 打开 `.Lib`，或 `.wzl`（自动加载同目录的 `.wzx`）。
    ///
    /// 传入 `.wzx` 也可以，会自动找同名的 `.wzl`。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AssetError> {
        let path = path.as_ref();
        let kind = detect_kind(path)?;
        match kind {
            LibraryKind::Lib => {
                let reader = LibReader::open(path)?;
                Ok(Self {
                    path: path.to_path_buf(),
                    kind,
                    backend: Backend::Lib(reader),
                })
            }
            LibraryKind::Wzl => {
                let (wzl_path, wzx_path) = resolve_wzl(path)?;
                let reader = WzlReader::open(&wzl_path, &wzx_path)?;
                Ok(Self {
                    path: wzl_path,
                    kind,
                    backend: Backend::Wzl(reader),
                })
            }
            LibraryKind::Wil => {
                let (wil_path, wix_path) = resolve_wil(path)?;
                let reader = WilReader::open(&wil_path, &wix_path)?;
                Ok(Self {
                    path: wil_path,
                    kind,
                    backend: Backend::Wil(reader),
                })
            }
        }
    }

    /// 容器类型。
    #[must_use]
    pub const fn kind(&self) -> LibraryKind {
        self.kind
    }

    /// 实际打开的数据文件路径（`.wzl` 打开时返回 `.wzl`，而不是 `.wzx`）。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 图像数量。
    ///
    /// * `.Lib`：头里的 `count`（`MLibrary.cs:520`）。
    /// * `.wzl`：偏移表项数 `(wzx 长度 - 48) / 4`，与 Crystal 跳过 48 字节后逐 4 字节读取的
    ///   做法一致（`WeMadeLibrary.cs:140-147`），**不**用头里的数量。
    #[must_use]
    pub fn len(&self) -> usize {
        match &self.backend {
            Backend::Lib(reader) => reader.len(),
            Backend::Wzl(reader) => reader.len(),
            Backend::Wil(reader) => reader.len(),
        }
    }

    /// 是否一张图都没有。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `.wzx` 头里偏移 44 声明的数量；`.Lib` 没有这个字段，返回 `None`。
    ///
    /// 语料里有 40 个 `.wzx` 的这个值与偏移表项数对不上（详见 crate 文档）。
    #[must_use]
    pub fn declared_count(&self) -> Option<u32> {
        match &self.backend {
            Backend::Lib(_) | Backend::Wil(_) => None,
            Backend::Wzl(reader) => Some(reader.declared_count()),
        }
    }

    /// `.wzl` 配套 `.wzx` 的实际路径；`.Lib` 返回 `None`。
    #[must_use]
    pub fn index_path(&self) -> Option<&Path> {
        match &self.backend {
            Backend::Lib(_) => None,
            Backend::Wzl(reader) => Some(reader.index_path()),
            Backend::Wil(reader) => Some(reader.index_path()),
        }
    }

    /// 只读单张图的头信息，不解码像素。
    ///
    /// `.wzl` 里偏移为 0 的项表示「没有这张图」，返回宽高都为 0 的头。
    pub fn info(&self, index: usize) -> Result<ImageInfo, AssetError> {
        match &self.backend {
            Backend::Lib(reader) => reader.info(index),
            Backend::Wzl(reader) => reader.info(index),
            Backend::Wil(reader) => reader.info(index),
        }
    }

    /// 解码单张图为 RGBA8（行优先、自上而下，长度 `宽 × 高 × 4`）。
    ///
    /// 空图返回空 `pixels`，不报错。
    pub fn image(&self, index: usize) -> Result<Image, AssetError> {
        match &self.backend {
            Backend::Lib(reader) => reader.image(index),
            Backend::Wzl(reader) => reader.image(index),
            Backend::Wil(reader) => reader.image(index),
        }
    }

    /// 旧版 `.Lib` 的可选原生帧表；v1/v2、无 `frameSeek` 的 v3 与非 `.Lib` 容器返回 `None`。
    ///
    /// v4 会在这里明确报错，不静默当无表。
    pub fn frames(&self) -> Result<Option<crate::FrameSet>, AssetError> {
        match self.kind {
            LibraryKind::Lib => crate::FrameSet::from_file(&self.path),
            LibraryKind::Wzl | LibraryKind::Wil => Ok(None),
        }
    }
}

/// 取小写扩展名。
fn extension_of(path: &Path) -> Option<String> {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
}

/// 按扩展名/文件头判断容器类型。
fn detect_kind(path: &Path) -> Result<LibraryKind, AssetError> {
    match extension_of(path).as_deref() {
        Some("lib") => Ok(LibraryKind::Lib),
        Some("wzl" | "wzx") => Ok(LibraryKind::Wzl),
        Some("wil" | "wix") => Ok(LibraryKind::Wil),
        _ => sniff_kind(path),
    }
}

/// 没有可识别扩展名时的嗅探。
fn sniff_kind(path: &Path) -> Result<LibraryKind, AssetError> {
    let mut file = crate::source::open_file(path)?;
    let len = crate::source::file_len(path, &file)?;
    let mut head = [0u8; 12];
    let available = usize::try_from(len.min(12)).unwrap_or(0);
    crate::source::read_at(&mut file, path, 0, &mut head[..available])?;
    let version = i32::from_le_bytes([head[0], head[1], head[2], head[3]]);
    let count = i32::from_le_bytes([head[4], head[5], head[6], head[7]]);
    // WIL/WIX 有固定 ASCII 头 `#ILIB` / `#INDX`，优先按签名识别（扩展名可能是大写）。
    if crate::wil::is_wil_signature(&head) {
        return Ok(LibraryKind::Wil);
    }
    let header_len: u64 = match version {
        3 => {
            // 与 `lib_container::parse_header` 同一判据：第三个 i32 等于偏移表末尾时
            // 是「无 frameSeek」的 v3 变体，偏移表从 8 开始。
            if available >= 12 {
                let third = u32::from_le_bytes([head[8], head[9], head[10], head[11]]);
                if count >= 0 && u64::from(third) == 8 + u64::from(count as u32) * 4 {
                    8
                } else {
                    12
                }
            } else {
                0
            }
        }
        1 | 2 => 8,
        _ => 0,
    };
    if header_len > 0 && count >= 0 && header_len + u64::from(count as u32) * 4 <= len {
        return Ok(LibraryKind::Lib);
    }
    if find_companion(path, "wzx").is_some() {
        return Ok(LibraryKind::Wzl);
    }
    Err(AssetError::BadMagic {
        path: path.to_path_buf(),
        offset: 0,
        expected: "旧版 .Lib（版本 2/3）或带同名 .wzx 的 .wzl",
        found: u32::from_le_bytes([head[0], head[1], head[2], head[3]]),
    })
}

/// `.wzl` 入口：确定数据文件与索引文件。
fn resolve_wzl(path: &Path) -> Result<(PathBuf, PathBuf), AssetError> {
    if extension_of(path).as_deref() == Some("wzx") {
        let wzl = find_companion(path, "wzl").ok_or(AssetError::MissingCompanion {
            path: path.to_path_buf(),
            extension: "wzl",
        })?;
        return Ok((wzl, path.to_path_buf()));
    }
    let wzx = find_companion(path, "wzx").ok_or(AssetError::MissingCompanion {
        path: path.to_path_buf(),
        extension: "wzx",
    })?;
    Ok((path.to_path_buf(), wzx))
}

/// `.wil` 入口：确定数据文件与索引文件。
fn resolve_wil(path: &Path) -> Result<(PathBuf, PathBuf), AssetError> {
    if extension_of(path).as_deref() == Some("wix") {
        let wil = find_companion(path, "wil").ok_or(AssetError::MissingCompanion {
            path: path.to_path_buf(),
            extension: "wil",
        })?;
        return Ok((wil, path.to_path_buf()));
    }
    let wix = find_companion(path, "wix").ok_or(AssetError::MissingCompanion {
        path: path.to_path_buf(),
        extension: "wix",
    })?;
    Ok((path.to_path_buf(), wix))
}

/// 找同名、指定扩展名的配套文件：先试精确路径，再按大小写不敏感扫目录。
fn find_companion(path: &Path, extension: &str) -> Option<PathBuf> {
    let direct = path.with_extension(extension);
    if direct.is_file() {
        return Some(direct);
    }
    let directory = path.parent()?;
    let stem = path.file_stem()?.to_string_lossy().to_ascii_lowercase();
    std::fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|candidate| {
            candidate.is_file()
                && candidate
                    .file_stem()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(&stem))
                && candidate
                    .extension()
                    .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case(extension))
        })
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::atomic::{AtomicU32, Ordering};

    use flate2::Compression;
    use flate2::write::{GzEncoder, ZlibEncoder};

    use super::*;
    use crate::image::format;

    /// 自动清理的临时目录。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let index = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "mir-asset-test-{}-{tag}-{index}",
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
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn gzip(data: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).expect("写 gzip");
        encoder.finish().expect("收尾 gzip")
    }

    fn zlib(data: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).expect("写 zlib");
        encoder.finish().expect("收尾 zlib")
    }

    /// 手工拼一个 v3 `.Lib`：第 0 张是 2×2 不透明 RGBA，第 1 张是空记录。
    fn synthetic_lib() -> Vec<u8> {
        let pixels: [u8; 16] = [
            255, 0, 0, 255, // 左上：红
            0, 255, 0, 255, // 右上：绿
            0, 0, 255, 255, // 左下：蓝
            9, 9, 9, 255, // 右下：灰
        ];
        let compressed = gzip(&pixels);
        let mut body = Vec::new();
        body.extend_from_slice(&2i16.to_le_bytes());
        body.extend_from_slice(&2i16.to_le_bytes());
        body.extend_from_slice(&1i16.to_le_bytes());
        body.extend_from_slice(&(-2i16).to_le_bytes());
        body.extend_from_slice(&0i16.to_le_bytes());
        body.extend_from_slice(&0i16.to_le_bytes());
        body.push(0);
        body.extend_from_slice(&(compressed.len() as i32).to_le_bytes());
        body.extend_from_slice(&compressed);

        let first_offset: u32 = 12 + 2 * 4;
        let second_offset = first_offset + body.len() as u32;
        body.extend_from_slice(&[0u8; 17]);

        let mut file = Vec::new();
        file.extend_from_slice(&3i32.to_le_bytes());
        file.extend_from_slice(&2i32.to_le_bytes());
        file.extend_from_slice(&0i32.to_le_bytes());
        file.extend_from_slice(&first_offset.to_le_bytes());
        file.extend_from_slice(&second_offset.to_le_bytes());
        file.extend_from_slice(&body);
        file
    }

    fn wzl_record(
        format: u8,
        width: u16,
        height: u16,
        offset_x: i16,
        offset_y: i16,
        payload: &[u8],
    ) -> Vec<u8> {
        let compressed = zlib(payload);
        let mut record = vec![format, 1, 0, 0];
        record.extend_from_slice(&width.to_le_bytes());
        record.extend_from_slice(&height.to_le_bytes());
        record.extend_from_slice(&offset_x.to_le_bytes());
        record.extend_from_slice(&offset_y.to_le_bytes());
        record.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        record.extend_from_slice(&compressed);
        record
    }

    /// 手工拼一对 `.wzl`/`.wzx`：565、8 位调色板、24 位各一张。
    fn synthetic_wzl() -> (Vec<u8>, Vec<u8>) {
        // 565 2×2：文件第 0 行（底部）红、第 1 行（顶部）绿，每行 2px×2B = 4 字节。
        let record_565 = wzl_record(
            format::RGB565,
            2,
            2,
            -1,
            3,
            &[0x00, 0xF8, 0x00, 0xF8, 0xE0, 0x07, 0xE0, 0x07],
        );
        // 8 位 2×1：索引 0（不透明黑）与索引 3（0xFF808000，橄榄色），行宽 4。
        let record_palette = wzl_record(format::PALETTE8, 2, 1, 0, 0, &[0, 3, 0, 0]);
        // 24 位 1×1，行宽 4。
        let record_rgb24 = wzl_record(format::RGB24, 1, 1, 5, 6, &[1, 2, 3, 0]);

        let mut wzl = vec![0u8; 48];
        let first = 48u32;
        wzl.extend_from_slice(&record_565);
        let second = first + record_565.len() as u32;
        wzl.extend_from_slice(&record_palette);
        let third = second + record_palette.len() as u32;
        wzl.extend_from_slice(&record_rgb24);

        let offsets = [first, second, third];
        let mut wzx = vec![0u8; 48];
        wzx[44..48].copy_from_slice(&(offsets.len() as u32).to_le_bytes());
        for offset in offsets {
            wzx.extend_from_slice(&offset.to_le_bytes());
        }
        (wzl, wzx)
    }

    fn expect_ok<T>(result: Result<T, AssetError>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("应当成功，实际失败：{error}"),
        }
    }

    #[test]
    fn opens_synthetic_lib_and_decodes() {
        let dir = TempDir::new("lib");
        let path = dir.join("data.Lib");
        std::fs::write(&path, synthetic_lib()).expect("写 .Lib");
        let library = expect_ok(ImageLibrary::open(&path));
        assert_eq!(library.kind(), LibraryKind::Lib);
        assert_eq!(library.len(), 2);
        assert!(!library.is_empty());
        assert_eq!(library.declared_count(), None);

        let info = expect_ok(library.info(0));
        assert_eq!((info.width, info.height), (2, 2));
        assert_eq!((info.offset_x, info.offset_y), (1, -2));
        assert_eq!(info.format, format::RGBA32_GZIP);
        assert_eq!(info.format_name(), "rgba32-gzip");
        assert!(!info.has_mask);

        let image = expect_ok(library.image(0));
        assert_eq!(image.pixels.len(), 2 * 2 * 4);
        assert_eq!(&image.pixels[0..4], &[255, 0, 0, 255]);
        assert_eq!(&image.pixels[12..16], &[9, 9, 9, 255]);

        let empty = expect_ok(library.info(1));
        assert!(empty.is_empty());
        assert!(expect_ok(library.image(1)).pixels.is_empty());
    }

    #[test]
    fn sniff_synthetic_lib_without_extension() {
        let dir = TempDir::new("sniff");
        let path = dir.join("data.bin");
        std::fs::write(&path, synthetic_lib()).expect("写 .bin");
        let library = expect_ok(ImageLibrary::open(&path));
        assert_eq!(library.kind(), LibraryKind::Lib);
        assert_eq!(library.len(), 2);
    }

    #[test]
    fn opens_synthetic_wzl_and_decodes_all_formats() {
        let dir = TempDir::new("wzl");
        let (wzl, wzx) = synthetic_wzl();
        std::fs::write(dir.join("tiles.wzl"), wzl).expect("写 .wzl");
        std::fs::write(dir.join("tiles.wzx"), wzx).expect("写 .wzx");

        let library = expect_ok(ImageLibrary::open(dir.join("tiles.wzl")));
        assert_eq!(library.kind(), LibraryKind::Wzl);
        assert_eq!(library.len(), 3);
        assert_eq!(library.declared_count(), Some(3));
        assert!(library.index_path().is_some());

        let info_565 = expect_ok(library.info(0));
        assert_eq!(info_565.format, format::RGB565);
        assert_eq!((info_565.width, info_565.height), (2, 2));
        assert_eq!((info_565.offset_x, info_565.offset_y), (-1, 3));
        assert_eq!(info_565.reserved, [1, 0, 0]);
        let image_565 = expect_ok(library.image(0));
        assert_eq!(image_565.pixels.len(), 16);
        // 顶部来自文件第 1 行：绿色 0x07E0 → (0, 252, 0)
        assert_eq!(&image_565.pixels[0..4], &[0, 252, 0, 255]);
        // 底部来自文件第 0 行：红色 0xF800 → (248, 0, 0)
        assert_eq!(&image_565.pixels[8..12], &[248, 0, 0, 255]);

        let info_palette = expect_ok(library.info(1));
        assert_eq!(info_palette.format, format::PALETTE8);
        let image_palette = expect_ok(library.image(1));
        assert_eq!(image_palette.pixels.len(), 8);
        assert_eq!(&image_palette.pixels[0..4], &[0, 0, 0, 255]);
        assert_eq!(&image_palette.pixels[4..8], &[128, 128, 0, 255]);

        let info_rgb24 = expect_ok(library.info(2));
        assert_eq!(info_rgb24.format, format::RGB24);
        let image_rgb24 = expect_ok(library.image(2));
        assert_eq!(image_rgb24.pixels, vec![1, 2, 3, 255]);
    }

    #[test]
    fn opens_wzl_from_wzx_path() {
        let dir = TempDir::new("wzx");
        let (wzl, wzx) = synthetic_wzl();
        std::fs::write(dir.join("tiles.wzl"), wzl).expect("写 .wzl");
        std::fs::write(dir.join("tiles.wzx"), wzx).expect("写 .wzx");
        let library = expect_ok(ImageLibrary::open(dir.join("tiles.wzx")));
        assert_eq!(library.kind(), LibraryKind::Wzl);
        assert_eq!(
            library.path().file_name().and_then(|name| name.to_str()),
            Some("tiles.wzl")
        );
    }

    #[test]
    fn wzl_without_index_reports_missing_companion() {
        let dir = TempDir::new("missing");
        let (wzl, _) = synthetic_wzl();
        let path = dir.join("tiles.wzl");
        std::fs::write(&path, wzl).expect("写 .wzl");
        let error = ImageLibrary::open(&path).expect_err("缺 .wzx 应当报错");
        assert!(
            matches!(
                error,
                AssetError::MissingCompanion {
                    extension: "wzx",
                    ..
                }
            ),
            "实际错误：{error}"
        );
    }

    #[test]
    fn index_out_of_range_is_reported() {
        let dir = TempDir::new("range");
        let path = dir.join("data.Lib");
        std::fs::write(&path, synthetic_lib()).expect("写 .Lib");
        let library = expect_ok(ImageLibrary::open(&path));
        let error = library.info(7).expect_err("越界应当报错");
        assert!(
            matches!(error, AssetError::IndexOutOfRange { index: 7, count: 2 }),
            "实际错误：{error}"
        );
    }

    #[test]
    fn unknown_format_is_rejected() {
        let dir = TempDir::new("badfmt");
        let record = wzl_record(7, 1, 1, 0, 0, &[0, 0, 0, 0]);
        let mut wzl = vec![0u8; 48];
        wzl.extend_from_slice(&record);
        std::fs::write(dir.join("bad.wzl"), wzl).expect("写 .wzl");
        let mut wzx = vec![0u8; 48];
        wzx[44..48].copy_from_slice(&1u32.to_le_bytes());
        wzx.extend_from_slice(&48u32.to_le_bytes());
        std::fs::write(dir.join("bad.wzx"), wzx).expect("写 .wzx");

        let library = expect_ok(ImageLibrary::open(dir.join("bad.wzl")));
        let error = library.info(0).expect_err("格式 7 应当报错");
        assert!(
            matches!(
                error,
                AssetError::UnsupportedFormat {
                    format: 7,
                    offset: 48,
                    ..
                }
            ),
            "实际错误：{error}"
        );
    }

    #[test]
    fn bad_magic_for_unknown_file() {
        let dir = TempDir::new("magic");
        let path = dir.join("random.bin");
        std::fs::write(&path, b"not a library at all").expect("写文件");
        let error = ImageLibrary::open(&path).expect_err("应当报 BadMagic");
        assert!(matches!(error, AssetError::BadMagic { offset: 0, .. }));
    }

    #[test]
    fn truncation_is_reported() {
        let dir = TempDir::new("trunc");
        let mut lib = synthetic_lib();
        lib.truncate(2);
        let path = dir.join("short.Lib");
        std::fs::write(&path, lib).expect("写 .Lib");
        let error = ImageLibrary::open(&path).expect_err("过短应当报错");
        assert!(
            matches!(
                error,
                AssetError::Truncated {
                    offset: 0,
                    needed: 8,
                    actual: 2,
                    ..
                }
            ),
            "实际错误：{error}"
        );
    }

    #[test]
    fn v3_header_shorter_than_index_offset_is_reported() {
        // v3 需要第 3 个 i32 才能判断布局；只有 10 字节时应当报截断而不是下溢。
        let dir = TempDir::new("shortv3");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&3i32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&[0u8, 0]);
        let path = dir.join("tiny.Lib");
        std::fs::write(&path, bytes).expect("写 .Lib");
        let error = ImageLibrary::open(&path).expect_err("头部不完整应当报错");
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
    fn opens_v3_lib_without_frame_seek() {
        // 真实语料里大量 .Lib 是「版本 3 + v2 布局」：头 8 字节 + 偏移表，没有 frameSeek。
        let pixels: [u8; 4] = [1, 2, 3, 255];
        let compressed = gzip(&pixels);
        let mut body = Vec::new();
        body.extend_from_slice(&1i16.to_le_bytes());
        body.extend_from_slice(&1i16.to_le_bytes());
        body.extend_from_slice(&0i16.to_le_bytes());
        body.extend_from_slice(&0i16.to_le_bytes());
        body.extend_from_slice(&0i16.to_le_bytes());
        body.extend_from_slice(&0i16.to_le_bytes());
        body.push(0);
        body.extend_from_slice(&(compressed.len() as i32).to_le_bytes());
        body.extend_from_slice(&compressed);

        let first_offset: u32 = 8 + 2 * 4;
        let second_offset = first_offset + body.len() as u32;
        body.extend_from_slice(&[0u8; 17]);

        let mut file = Vec::new();
        file.extend_from_slice(&3i32.to_le_bytes());
        file.extend_from_slice(&2i32.to_le_bytes());
        // 关键：第三个 i32 不是 frameSeek，而是第一个偏移（= 偏移表末尾）。
        file.extend_from_slice(&first_offset.to_le_bytes());
        file.extend_from_slice(&second_offset.to_le_bytes());
        file.extend_from_slice(&body);

        let dir = TempDir::new("v3v2");
        let path = dir.join("variant.Lib");
        std::fs::write(&path, file).expect("写 .Lib");
        let library = expect_ok(ImageLibrary::open(&path));
        assert_eq!(library.len(), 2);
        let image = expect_ok(library.image(0));
        assert_eq!(image.pixels, vec![1, 2, 3, 255]);
        assert!(expect_ok(library.image(1)).pixels.is_empty());
    }
}
