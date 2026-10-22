//! 渲染错误。
//!
//! 地图读取错误直接复用 [`mir_world::MapError`]，素材错误复用
//! [`mir_asset::AssetError`]；这里只补充「素材目录不存在 / 没有可用图库 / PNG 写出」
//! 这几种渲染层特有的失败。

use std::path::PathBuf;

use thiserror::Error;

/// 加载地图场景或输出帧缓冲时可能出现的错误。
#[derive(Debug, Error)]
pub enum RenderError {
    /// 地图文件读取/解析失败（`mir-world`）。
    #[error("读取地图失败：{0}")]
    Map(#[from] mir_world::MapError),

    /// 图像库打开或解码失败（`mir-asset`）。
    #[error("读取素材失败：{0}")]
    Asset(#[from] mir_asset::AssetError),

    /// 素材目录不存在。
    #[error("素材目录不存在：{}", .path.display())]
    DataDirectory {
        /// 传入的目录。
        path: PathBuf,
    },

    /// 素材目录里一个地图图库都没找到。
    #[error(
        "素材目录 {} 里没有找到地图图像库（需要 library-map.json + gfx/** 或 packs/）",
        .directory.display()
    )]
    NoLibraries {
        /// 查找过的目录。
        directory: PathBuf,
    },

    /// 创建 PNG 文件失败。
    #[error("创建 PNG 文件 {} 失败：{source}", .path.display())]
    CreatePng {
        /// 目标路径。
        path: PathBuf,
        /// IO 错误。
        source: std::io::Error,
    },

    /// PNG 编码失败。
    #[error("编码 PNG 失败：{0}")]
    Png(#[from] png::EncodingError),
}
