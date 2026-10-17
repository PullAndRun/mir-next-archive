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

use mir_asset::{Image, ImageInfo, ImageLibrary};

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
    libraries: BTreeMap<u16, ImageLibrary>,
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
        let directory = directory.as_ref().to_path_buf();
        if !directory.is_dir() {
            return Err(RenderError::DataDirectory { path: directory });
        }
        let mut libraries = BTreeMap::new();
        for (index, file_name) in LIBRARY_FILES {
            let Some(path) = find_case_insensitive(&directory, file_name) else {
                continue;
            };
            let library = ImageLibrary::open(&path)?;
            libraries.insert(*index, library);
        }
        if libraries.is_empty() {
            return Err(RenderError::NoLibraries { directory });
        }
        Ok(Self {
            directory,
            libraries,
        })
    }

    /// 空图库集合（单元测试用；正常渲染应当用 [`TileLibraries::open_dir`]）。
    #[must_use]
    pub fn empty(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            libraries: BTreeMap::new(),
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

    /// 按 `MapLibs` 编号取图库。
    #[must_use]
    pub fn library(&self, index: u16) -> Option<&ImageLibrary> {
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
        assert!(libraries.library(0).is_none());
        assert!(libraries.info(0, 0).is_none());
        assert!(libraries.image(0, 0).is_none());
        assert_eq!(libraries.indices().count(), 0);
        assert_eq!(libraries.directory(), Path::new("."));
    }
}
