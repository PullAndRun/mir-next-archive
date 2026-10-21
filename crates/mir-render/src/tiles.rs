//! 地图图库集合：把 Crystal 的 `MapLibs[0..400]` 编号映射到平铺的 `resource/data/*.wzl`。
//!
//! Crystal 的图库按目录组织（`MLibrary.cs:124-187`），例如
//! `Map/WemadeMir2/Tiles.Lib`、`Map/WemadeMir2/Objects3.Lib`。本项目的
//! `resource/data` 是平铺的 `.wzl`/`.wzx`，所以这里用一张固定表
//! [`LIBRARY_FILES`] 把编号映射成文件名：
//!
//! | `MapLibs` 编号 | Crystal 名称 | 本项目文件 |
//! |---|---|---|
//! | 0 | `Tiles` | `tiles.wzl` |
//! | 1 | `SmTiles` | `smtiles.wzl` |
//! | 2 | `Objects` | `objects.wzl` |
//! | 3…28 | `Objects2`…`Objects27` | `objects2.wzl`…`objects27.wzl` |
//! | 90 | `Objects_32bit` | `objects_32bit.wzl` |
//! | 190 | `AniTiles1` | `anitiles1.wzl` |
//!
//! Mir3 的 200–399 号（`Tilesc`/`Smtilesc`/…）没有收录：v0 地图（`resource/map` 里
//! 848 个文件中的绝大多数，含 `0.map`）完全不使用它们，而平铺语料里也无法把
//! `tiles2.wzl` 之类的文件可靠地区分成 Wemade/Shanda 目录。
//!
//! 打开时只读 `.wzx` 索引，不碰像素；缺失的文件直接跳过，存在但损坏的文件返回错误
//! （避免静默画出半张地图）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mir_asset::{Image, ImageInfo, LibrarySource};

use crate::RenderError;

/// Crystal `MapLibs` 编号 → `resource/data` 文件名。
///
/// 来源：`MLibrary.cs:124-131`（Wemade Mir2 部分）与 `MLibrary.cs:149`（`AniTiles1`）。
pub const LIBRARY_FILES: &[(u16, &str)] = &[
    (0, "tiles.wzl"),
    (1, "smtiles.wzl"),
    (2, "objects.wzl"),
    (3, "objects2.wzl"),
    (4, "objects3.wzl"),
    (5, "objects4.wzl"),
    (6, "objects5.wzl"),
    (7, "objects6.wzl"),
    (8, "objects7.wzl"),
    (9, "objects8.wzl"),
    (10, "objects9.wzl"),
    (11, "objects10.wzl"),
    (12, "objects11.wzl"),
    (13, "objects12.wzl"),
    (14, "objects13.wzl"),
    (15, "objects14.wzl"),
    (16, "objects15.wzl"),
    (17, "objects16.wzl"),
    (18, "objects17.wzl"),
    (19, "objects18.wzl"),
    (20, "objects19.wzl"),
    (21, "objects20.wzl"),
    (22, "objects21.wzl"),
    (23, "objects22.wzl"),
    (24, "objects23.wzl"),
    (25, "objects24.wzl"),
    (26, "objects25.wzl"),
    (27, "objects26.wzl"),
    (28, "objects27.wzl"),
    (90, "objects_32bit.wzl"),
    (190, "anitiles1.wzl"),
];

/// 已打开的地图图库。
#[derive(Debug)]
pub struct TileLibraries {
    /// 素材目录（诊断用）。
    directory: PathBuf,
    /// 按 `MapLibs` 编号排序的图库。
    libraries: BTreeMap<u16, LibrarySource>,
    /// 目录里额外找到的 `.wil`/`.wix` 图库（WEMADE 旧版 16 位库）。
    ///
    /// 平铺语料没有 Crystal 的 Mir3 目录编号（`Tiles110.wil`/`Objects81.wil` 的
    /// 数字是素材分组，不是 `MapLibs` 下标），且当前 `resource/map` 的 v0/v1 地图
    /// 只引用 `tiles`/`smtiles`/`objects*` 这些 `.wzl` 库，所以这里只**打开索引**
    /// 并计数（启动日志/诊断用），不参与 `library(index)` 的选择。
    wil_libraries: Vec<(String, LibrarySource)>,
}

impl TileLibraries {
    /// 打开目录里所有存在的地图图库（只读索引，不解码像素）。
    ///
    /// 文件名比较不区分大小写（Windows 上 `.WZL` 与 `.wzl` 是同一个文件，
    /// 其它平台则不是）。
    ///
    /// # Errors
    /// * 目录不存在 → [`RenderError::DataDirectory`]；
    /// * 一个图库都没找到 → [`RenderError::NoLibraries`]；
    /// * 某个文件存在但打不开 → [`RenderError::Asset`]。
    pub fn open_dir(directory: impl AsRef<Path>) -> Result<Self, RenderError> {
        Self::open_dir_with_gfx(directory, None)
    }

    /// 同 [`Self::open_dir`]，但优先用 `<gfx_dir>/<stem>/index.json` 的拆包目录，
    /// 缺失时回退原始 `.wzl`（素材粒度最细的加载路径）。
    ///
    /// # Errors
    /// 与 [`Self::open_dir`] 相同。
    pub fn open_dir_with_gfx(
        directory: impl AsRef<Path>,
        gfx_dir: Option<&Path>,
    ) -> Result<Self, RenderError> {
        Self::open_with_sources(directory, gfx_dir, None, None, None, true)
    }

    /// 完整加载：**散装目录（`library-map.json`）→ 小包（`.mnp`）→ 原始 `.wzl`**。
    ///
    /// * `gfx_dir`：老的平铺拆包根（`<root>/gfx`，按 `<stem>/index.json` 找），可为 `None`；
    /// * `asset_root`：素材根（`library-map.json` 里的 `dirs` 相对它解析）；
    /// * `map`：`library-map.json`（`stem → dirs/prefix`）；
    /// * `packs`：发布期小包集合。
    ///
    /// # Errors
    /// 目录不存在且没有任何散装/包来源 → [`RenderError::DataDirectory`]；
    /// 一个图库都找不到 → [`RenderError::NoLibraries`]。
    pub fn open_with_sources(
        directory: impl AsRef<Path>,
        gfx_dir: Option<&Path>,
        asset_root: Option<&Path>,
        map: Option<&mir_asset::LibraryMap>,
        packs: Option<&mir_asset::PackSet>,
        allow_legacy: bool,
    ) -> Result<Self, RenderError> {
        let directory = directory.as_ref().to_path_buf();
        let has_alternatives = map.is_some_and(|map| !map.is_empty()) || packs.is_some();
        if !directory.is_dir() && !has_alternatives {
            return Err(RenderError::DataDirectory { path: directory });
        }
        let mut libraries = BTreeMap::new();
        for (index, file_name) in LIBRARY_FILES {
            let Some(library) = open_library(
                file_name,
                &directory,
                gfx_dir,
                asset_root,
                map,
                packs,
                allow_legacy,
            )?
            else {
                continue;
            };
            libraries.insert(*index, library);
        }
        if libraries.is_empty() {
            return Err(RenderError::NoLibraries { directory });
        }
        // `.wil`/`.wix`：打开索引（不参与 `library(index)`，见字段文档）。
        let mut wil_libraries = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_wil = path
                    .extension()
                    .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("wil"));
                if !is_wil {
                    continue;
                }
                match LibrarySource::open_with_gfx(&path, gfx_dir) {
                    Ok(library) => {
                        let name = path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        wil_libraries.push((name, library));
                    }
                    Err(error) => {
                        eprintln!(
                            "警告：打开 WIL 图库 {} 失败（{error}），跳过",
                            path.display()
                        );
                    }
                }
            }
        }
        wil_libraries.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Self {
            directory,
            libraries,
            wil_libraries,
        })
    }

    /// 空图库集合（单元测试用；正常渲染应当用 [`TileLibraries::open_dir`]）。
    #[must_use]
    pub fn empty(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            libraries: BTreeMap::new(),
            wil_libraries: Vec::new(),
        }
    }

    /// 打开时使用的素材目录。
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// 实际打开的图库数量。
    #[must_use]
    pub fn len(&self) -> usize {
        self.libraries.len()
    }

    /// 是否一个图库都没打开。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.libraries.is_empty()
    }

    /// 额外打开的 `.wil` 图库数量（WEMADE 旧版 16 位库，不参与瓦片选择）。
    #[must_use]
    pub fn wil_len(&self) -> usize {
        self.wil_libraries.len()
    }

    /// 额外打开的 `.wil` 文件名（升序）。
    pub fn wil_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.wil_libraries.iter().map(|(name, _)| name.as_str())
    }

    /// 按 `MapLibs` 编号取图库。
    #[must_use]
    pub fn library(&self, index: u16) -> Option<&LibrarySource> {
        self.libraries.get(&index)
    }

    /// 已打开的编号（升序）。
    pub fn indices(&self) -> impl Iterator<Item = u16> + '_ {
        self.libraries.keys().copied()
    }

    /// 取单张图的头信息；库不存在、下标越界或空图返回 `None`。
    #[must_use]
    pub fn info(&self, library: u16, image: u32) -> Option<ImageInfo> {
        let library = self.libraries.get(&library)?;
        let image = usize::try_from(image).ok()?;
        library.info(image).ok().filter(|info| !info.is_empty())
    }

    /// 解码单张图；库不存在、下标越界、空图或格式不支持返回 `None`。
    #[must_use]
    pub fn image(&self, library: u16, image: u32) -> Option<Image> {
        let library = self.libraries.get(&library)?;
        let image = usize::try_from(image).ok()?;
        library.image(image).ok().filter(|image| !image.is_empty())
    }
}

/// 在目录里找名字不区分大小写的文件。
/// 按 **散装目录 → 小包 → `.wzl`** 的顺序打开一个库。
///
/// 返回 `Ok(None)` 表示三种来源都没有这个库（调用方跳过即可）。
fn open_library(
    file_name: &str,
    directory: &Path,
    gfx_dir: Option<&Path>,
    asset_root: Option<&Path>,
    map: Option<&mir_asset::LibraryMap>,
    packs: Option<&mir_asset::PackSet>,
    allow_legacy: bool,
) -> Result<Option<LibrarySource>, RenderError> {
    let stem = Path::new(file_name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    // 1) 散装（library-map 里的多目录）
    if let (Some(map), Some(root)) = (map, asset_root) {
        if let Some(dirs) = map.dirs(&stem) {
            let paths: Vec<PathBuf> = dirs
                .iter()
                .map(|dir| root.join(dir))
                .filter(|path| path.join("index.json").is_file())
                .collect();
            if !paths.is_empty() {
                return Ok(Some(LibrarySource::from_dirs(&paths)?));
            }
        }
        // 2) 发布期小包
        if let (Some(prefix), Some(packs)) = (map.prefix(&stem), packs) {
            let set = packs.filter_prefix(prefix);
            if !set.is_empty() {
                return Ok(Some(LibrarySource::Pack(set)));
            }
        }
    }

    // 3) 原始容器（含老式平铺拆包目录 `<gfx>/<stem>/`）——**默认禁止**，
    //    只有显式 `asset_mode = legacy` 才允许（用户要求：运行时只读自有格式）。
    if !allow_legacy {
        return Ok(None);
    }
    let Some(path) = find_case_insensitive(directory, file_name) else {
        return Ok(None);
    };
    Ok(Some(LibrarySource::open_with_gfx(&path, gfx_dir)?))
}

fn find_case_insensitive(directory: &Path, file_name: &str) -> Option<PathBuf> {
    let direct = directory.join(file_name);
    if direct.is_file() {
        return Some(direct);
    }
    std::fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|candidate| {
            candidate.is_file()
                && candidate
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(file_name))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_table_matches_crystal_wemade_layout() {
        assert_eq!(LIBRARY_FILES.len(), 31);
        assert_eq!(LIBRARY_FILES[0], (0, "tiles.wzl"));
        assert_eq!(LIBRARY_FILES[1], (1, "smtiles.wzl"));
        assert_eq!(LIBRARY_FILES[2], (2, "objects.wzl"));
        // MapLibs[3] = Objects2（MLibrary.cs:127-130 的循环从 i = 2 开始）。
        assert_eq!(LIBRARY_FILES[3], (3, "objects2.wzl"));
        assert_eq!(LIBRARY_FILES[28], (28, "objects27.wzl"));
        assert!(LIBRARY_FILES.contains(&(190, "anitiles1.wzl")));
        // 编号严格递增，方便二分/调试。
        assert!(LIBRARY_FILES.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[test]
    fn missing_directory_is_reported() {
        let directory =
            std::env::temp_dir().join(format!("mir-render-missing-{}", std::process::id()));
        let error = TileLibraries::open_dir(&directory).expect_err("目录不存在应当报错");
        assert!(
            matches!(error, RenderError::DataDirectory { .. }),
            "实际错误：{error}"
        );
    }

    #[test]
    fn empty_directory_reports_no_libraries() {
        let directory =
            std::env::temp_dir().join(format!("mir-render-empty-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("创建临时目录");
        let error = TileLibraries::open_dir(&directory).expect_err("空目录应当报错");
        assert!(
            matches!(error, RenderError::NoLibraries { .. }),
            "实际错误：{error}"
        );
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn empty_libraries_are_usable_but_return_nothing() {
        let libraries = TileLibraries::empty(".");
        assert!(libraries.is_empty());
        assert_eq!(libraries.len(), 0);
        assert_eq!(libraries.wil_len(), 0);
        assert!(libraries.library(0).is_none());
        assert!(libraries.info(0, 0).is_none());
        assert!(libraries.image(0, 0).is_none());
        assert_eq!(libraries.indices().count(), 0);
        assert_eq!(libraries.directory(), Path::new("."));
    }

    #[test]
    fn real_data_dir_opens_wzl_and_wil_libraries() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resource/data");
        if !directory.is_dir() {
            eprintln!("跳过：缺 {}", directory.display());
            return;
        }
        let libraries = TileLibraries::open_dir(&directory).expect("打开真实素材目录");
        assert!(libraries.len() >= 3, "至少 tiles/smtiles/objects");
        assert!(
            libraries.wil_len() >= 4,
            "resource/data 里的 .wil/.wix 也应当打开：{}",
            libraries.wil_len()
        );
        // 0.map 的铺地层用 `tiles.wzl`：解一张真实瓦片，必须有像素。
        let info = libraries.info(0, 0).expect("tiles.wzl 第 0 张的头");
        assert!(info.width > 0 && info.height > 0);
        let image = libraries.image(0, 0).expect("tiles.wzl 第 0 张");
        assert!(!image.pixels.is_empty());
        println!(
            "图库 {} 个（+WIL {} 个），tiles 第 0 张 {}×{}",
            libraries.len(),
            libraries.wil_len(),
            info.width,
            info.height
        );
    }
}
