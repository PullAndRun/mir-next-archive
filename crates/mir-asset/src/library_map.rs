//! `library-map.json`：原库名 → 拆包目录 / 包前缀 的映射。
//!
//! 新命名把一库拆到多个目录（`hum.wzl` → `gfx/sprites/player/body-003/`…），
//! 渲染层按"库"取图时需要这张表，才能从 `<asset_root>` 找到对应的拆包目录
//! 或 `.mnp` 前缀。表由 `mir-tools assets unpack --plan` 生成，`assets pack`
//! 会把它复制进 `packs/`，所以开发期与发布期用同一份映射。
//!
//! ```json
//! {
//!   "version": 1,
//!   "libraries": {
//!     "tiles": { "lib": "tiles.wzl", "dirs": ["gfx/tiles/tiles0"], "prefix": "gfx/tiles/tiles0/" },
//!     "hum":   { "lib": "hum.wzl", "dirs": ["gfx/sprites/player/body-003", "gfx/sprites/player/body-004"] }
//!   }
//! }
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::AssetError;

/// 一张库映射表。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryMap {
    /// 格式版本（当前 1）。
    #[serde(default = "default_version")]
    pub version: u32,
    /// 生成时间（仅作记录）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    /// 库 stem（`hum.wzl` → `hum`）→ 条目。
    #[serde(default)]
    pub libraries: BTreeMap<String, LibraryEntry>,
}

/// 单个库的映射条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryEntry {
    /// 原库文件名（`hum.wzl`），只作溯源。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lib: Option<String>,
    /// 拆包目录（相对素材根，`gfx/...`，不带尾斜杠）。
    #[serde(default)]
    pub dirs: Vec<String>,
    /// 发布期包内前缀（`gfx/tiles/tiles0/`，带尾斜杠）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
}

fn default_version() -> u32 {
    1
}

impl LibraryMap {
    /// 解析 JSON 文本。
    pub fn parse(text: &str, origin: &Path) -> Result<Self, AssetError> {
        serde_json::from_str(text).map_err(|error| AssetError::UnpackedIndex {
            path: origin.to_path_buf(),
            message: format!("library-map.json 解析失败：{error}"),
        })
    }

    /// 从文件加载。
    pub fn load(path: impl AsRef<Path>) -> Result<Self, AssetError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| AssetError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text, path)
    }

    /// 库数量。
    #[must_use]
    pub fn len(&self) -> usize {
        self.libraries.len()
    }

    /// 是否为空表。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.libraries.is_empty()
    }

    /// 某个 stem 的条目。
    #[must_use]
    pub fn get(&self, stem: &str) -> Option<&LibraryEntry> {
        self.libraries.get(stem)
    }

    /// 某个 stem 的拆包目录（相对素材根）。
    #[must_use]
    pub fn dirs(&self, stem: &str) -> Option<&[String]> {
        self.libraries.get(stem).map(|entry| entry.dirs.as_slice())
    }

    /// 某个 stem 的包前缀。
    #[must_use]
    pub fn prefix(&self, stem: &str) -> Option<&str> {
        self.libraries
            .get(stem)
            .and_then(|entry| entry.prefix.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dirs_and_prefix() {
        let text = r#"{
            "version": 1,
            "libraries": {
                "tiles": {"lib": "tiles.wzl", "dirs": ["gfx/tiles/tiles0"], "prefix": "gfx/tiles/tiles0/"},
                "hum": {"lib": "hum.wzl", "dirs": ["gfx/sprites/player/body-003", "gfx/sprites/player/body-004"]}
            }
        }"#;
        let map = LibraryMap::parse(text, Path::new("memory")).expect("parse");
        assert_eq!(map.len(), 2);
        assert_eq!(map.dirs("hum").expect("hum").len(), 2);
        assert_eq!(map.prefix("tiles"), Some("gfx/tiles/tiles0/"));
        assert_eq!(map.prefix("hum"), None);
        assert!(map.get("nope").is_none());
    }

    #[test]
    fn empty_table_is_valid_so_callers_can_fall_back() {
        let map =
            LibraryMap::parse("{\"version\":1,\"libraries\":{}}", Path::new("x")).expect("parse");
        assert!(map.is_empty());
        assert!(map.dirs("tiles").is_none());
    }
}
