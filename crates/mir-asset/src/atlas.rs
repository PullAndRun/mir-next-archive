//! 图集（atlas）：把一实体/一库的帧合到一张纹理里，替代"每帧一个 PNG"。
//!
//! 目录形态（与散装拆包同目录，便于渐进迁移）：
//!
//! ```text
//! gfx/<category>/<entity>/atlas.png    单张 RGBA8 纹理
//! gfx/<category>/<entity>/atlas.json   {"version":1,"lib":"hum.wzl","entity":"body-003",
//!                                        "texture":"atlas.png","width":1024,"height":2048,
//!                                        "frames":[{"index":0,"x":0,"y":0,"w":64,"h":80,
//!                                                   "offset_x":-13,"offset_y":-44,
//!                                                   "shadow_x":0,"shadow_y":0,"has_mask":false}]}
//! gfx/<category>/<entity>/index.json   保留（帧 → 原库索引的映射，散装仍可用）
//! ```
//!
//! 加载后 `image(i)` 直接切子矩形，不用逐帧解 PNG；像素与散装 PNG **逐字节一致**。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::error::AssetError;
use crate::image::{Image, ImageInfo};

/// 图集元数据文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtlasMeta {
    /// 格式版本。
    #[serde(default = "default_version")]
    pub version: u32,
    /// 原库文件名（溯源用）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lib: Option<String>,
    /// 实体名（`body-003`、`mon-0130`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    /// 纹理文件名（相对目录）。
    #[serde(default = "default_texture")]
    pub texture: String,
    /// 纹理宽。
    pub width: u32,
    /// 纹理高。
    pub height: u32,
    /// 帧列表。
    pub frames: Vec<AtlasFrame>,
}

/// 图集里的一帧。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AtlasFrame {
    /// 原库 0 基索引。
    pub index: usize,
    /// 纹理内左上角 x。
    pub x: u32,
    /// 纹理内左上角 y。
    pub y: u32,
    /// 宽。
    pub w: u16,
    /// 高。
    pub h: u16,
    /// 绘制偏移 x。
    #[serde(default)]
    pub offset_x: i16,
    /// 绘制偏移 y。
    #[serde(default)]
    pub offset_y: i16,
    /// 影子偏移 x。
    #[serde(default)]
    pub shadow_x: i16,
    /// 影子偏移 y。
    #[serde(default)]
    pub shadow_y: i16,
    /// 是否有遮罩。
    #[serde(default)]
    pub has_mask: bool,
}

fn default_version() -> u32 {
    1
}

fn default_texture() -> String {
    "atlas.png".to_owned()
}

/// 一个图集库（与 [`crate::UnpackedLibrary`] 同形的 `len/info/image`）。
#[derive(Debug)]
pub struct AtlasLibrary {
    dir: PathBuf,
    meta: AtlasMeta,
    texture: OnceLock<Vec<u8>>,
    by_index: BTreeMap<usize, usize>,
}

impl AtlasLibrary {
    /// 目录里有 `atlas.json` 才算图集。
    #[must_use]
    pub fn has_atlas(dir: impl AsRef<Path>) -> bool {
        dir.as_ref().join("atlas.json").is_file()
    }

    /// 打开目录里的图集（只读 `atlas.json`，纹理首次取像素时才解 PNG）。
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, AssetError> {
        let dir = dir.as_ref().to_path_buf();
        let meta_path = dir.join("atlas.json");
        let text = std::fs::read_to_string(&meta_path).map_err(|source| AssetError::Io {
            path: meta_path.clone(),
            source,
        })?;
        let meta: AtlasMeta =
            serde_json::from_str(&text).map_err(|error| AssetError::UnpackedIndex {
                path: meta_path.clone(),
                message: format!("atlas.json 解析失败：{error}"),
            })?;
        if meta.version != 1 {
            return Err(AssetError::UnpackedIndex {
                path: meta_path,
                message: format!("atlas 版本 {} 不受支持", meta.version),
            });
        }
        let mut by_index = BTreeMap::new();
        for (position, frame) in meta.frames.iter().enumerate() {
            by_index.insert(frame.index, position);
        }
        Ok(Self {
            dir,
            meta,
            texture: OnceLock::new(),
            by_index,
        })
    }

    /// 目录。
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// 元数据。
    #[must_use]
    pub fn meta(&self) -> &AtlasMeta {
        &self.meta
    }

    /// 帧数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.meta.frames.len()
    }

    /// 是否没有帧。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.meta.frames.is_empty()
    }

    /// 是否包含某个原库索引。
    #[must_use]
    pub fn contains(&self, index: usize) -> bool {
        self.by_index.contains_key(&index)
    }

    /// 帧元数据。
    pub fn info(&self, index: usize) -> Result<ImageInfo, AssetError> {
        let position = *self.by_index.get(&index).ok_or(AssetError::MissingIndex {
            path: self.dir.clone(),
            index,
        })?;
        let frame = &self.meta.frames[position];
        Ok(ImageInfo {
            width: frame.w,
            height: frame.h,
            offset_x: frame.offset_x,
            offset_y: frame.offset_y,
            format: crate::image::format::RGBA32_GZIP,
            shadow_x: frame.shadow_x,
            shadow_y: frame.shadow_y,
            reserved: [0; 3],
            has_mask: frame.has_mask,
            data_offset: 0,
            data_len: 0,
        })
    }

    /// 切出第 `index` 帧的像素。
    pub fn image(&self, index: usize) -> Result<Image, AssetError> {
        let position = *self.by_index.get(&index).ok_or(AssetError::MissingIndex {
            path: self.dir.clone(),
            index,
        })?;
        let frame = self.meta.frames[position];
        let info = self.info(index)?;
        let texture = self.texture()?;
        let atlas_width = self.meta.width as usize;
        let mut pixels = Vec::with_capacity(usize::from(frame.w) * usize::from(frame.h) * 4);
        for row in 0..usize::from(frame.h) {
            let start = ((frame.y as usize + row) * atlas_width + frame.x as usize) * 4;
            let end = start + usize::from(frame.w) * 4;
            pixels.extend_from_slice(&texture[start..end]);
        }
        Ok(Image { info, pixels })
    }

    fn texture(&self) -> Result<&Vec<u8>, AssetError> {
        if let Some(bytes) = self.texture.get() {
            return Ok(bytes);
        }
        let path = self.dir.join(&self.meta.texture);
        let bytes = crate::unpacked::read_png_rgba_pub(&path)?;
        if bytes.len() != self.meta.width as usize * self.meta.height as usize * 4 {
            return Err(AssetError::UnpackedIndex {
                path,
                message: format!(
                    "纹理大小 {} 字节，与 atlas.json 的 {}×{}×4 不符",
                    bytes.len(),
                    self.meta.width,
                    self.meta.height
                ),
            });
        }
        let _ = self.texture.set(bytes);
        Ok(self.texture.get().expect("just set"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_atlas_json_is_io_error_so_callers_can_fall_back() {
        let dir = std::env::temp_dir().join("mir2-atlas-empty");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let error = AtlasLibrary::open(&dir).expect_err("没有 atlas.json 必须报错");
        assert!(matches!(error, AssetError::Io { .. }), "{error}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
