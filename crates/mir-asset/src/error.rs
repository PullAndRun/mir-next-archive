//! 素材库错误。
//!
//! 约定：所有「文件内容不合规」的错误都带 `offset`（相对出错文件的字节偏移），
//! 可以直接拿去十六进制查看器里定位；`Truncated` 的 `actual` 是**剩余**字节数，
//! 其余变体的 `actual` 含义见字段文档。

use std::path::PathBuf;

use thiserror::Error;

/// 打开或解码素材库时可能出现的错误。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AssetError {
    /// 拆包目录的 `index.json` 不是合法 JSON / 缺字段。
    #[error("{path} 不是合法的拆包索引：{message}")]
    UnpackedIndex {
        /// 出错的 `index.json`。
        path: PathBuf,
        /// 解析器给出的原因。
        message: String,
    },

    /// 拆包目录里没有这个索引（裁剪时没被引用）——调用方应回退到原始 `.wzl`。
    #[error("{path} 里没有原库索引 {index}（裁剪时未收录）")]
    MissingIndex {
        /// 拆包目录的 `index.json`。
        path: PathBuf,
        /// 原库内的 0 基索引。
        index: usize,
    },

    /// 打开、定位或读取文件失败。
    #[error("读取素材文件 {path} 失败：{source}")]
    Io {
        /// 出错的文件（`.Lib`、`.wzl` 或 `.wzx`）。
        path: PathBuf,
        /// 底层 I/O 错误。
        #[source]
        source: std::io::Error,
    },

    /// 文件头不是本实现认识的素材库。
    ///
    /// * 旧 `.Lib`：头 4 字节是 `i32` 版本号，Crystal 只接受 `>= 2`（`MLibrary.cs:513-519`），
    ///   本实现只接受 2 和 3。
    /// * 无扩展名的文件：既不像 `.Lib`，也没有同名的 `.wzx`。
    #[error(
        "{path} 偏移 {offset} 处的文件头无法识别：期望 {expected}，读到的第一个 u32 是 {found}"
    )]
    BadMagic {
        /// 被打开的文件。
        path: PathBuf,
        /// 头部字节偏移，恒为 0。
        offset: u64,
        /// 期望的格式说明。
        expected: &'static str,
        /// 实际读到的头 4 字节（小端 `u32`）。
        found: u32,
    },

    /// `.wzl` 缺少配套的 `.wzx`（`WeMadeLibrary.cs:63-64` 要求两个文件都在）。
    #[error("找不到 {path} 配套的 .{extension} 文件")]
    MissingCompanion {
        /// 已给出的文件。
        path: PathBuf,
        /// 缺少的扩展名（`wzx` 或 `wzl`）。
        extension: &'static str,
    },

    /// 读取越过文件末尾。
    #[error("{path} 偏移 {offset} 处需要 {needed} 字节，但只剩 {actual} 字节")]
    Truncated {
        /// 被读取的文件。
        path: PathBuf,
        /// 越界读取的起始偏移。
        offset: u64,
        /// 需要读取的字节数。
        needed: u64,
        /// 该偏移处实际剩余（或文件总计）的字节数。
        actual: u64,
    },

    /// 记录在语义上损坏（负宽高、负长度、解压后像素不足等）。
    #[error("{path} 偏移 {offset} 处的记录损坏：{message}")]
    Corrupt {
        /// 被读取的文件。
        path: PathBuf,
        /// 记录（或其中字段）的字节偏移。
        offset: u64,
        /// 具体原因。
        message: String,
    },

    /// 压缩数据解不开：`.Lib` 的 gzip 像素数据（`MLibrary.cs:914`），
    /// 或 `.wzl` 的 zlib 像素数据（`WeMadeLibrary.cs:388-394`）。
    #[error("{path} 偏移 {offset} 处的压缩数据解压失败：{message}")]
    Compression {
        /// 被读取的文件。
        path: PathBuf,
        /// 压缩数据在文件里的偏移。
        offset: u64,
        /// 解压器给出的原因。
        message: String,
    },

    /// `.wzl` 记录的像素格式字节不在本实现支持的集合里。
    ///
    /// 语料里实际出现过 `0`/`3`（8 位调色板）、`5`（RGB565）、`6`（RGB24），
    /// 见 `WeMadeLibrary.cs:319`、`WeMadeLibrary.cs:464-467`。
    #[error("{path} 偏移 {offset} 处的图像格式 {format} 不受支持")]
    UnsupportedFormat {
        /// 被读取的文件。
        path: PathBuf,
        /// 记录头的第一个字节。
        format: u8,
        /// 记录偏移。
        offset: u64,
    },

    /// 旧 `.Lib` 的像素数据既不是 gzip 流，也不是该尺寸的 DXT1。
    ///
    /// 语料里的 `.Lib` 只有两种存储：Crystal `MLibraryV2` 的 gzip RGBA32
    /// （`LibraryEditor/Graphics/MLibraryV2.cs:510-546`）与 `MLibraryV1` 的 DXT1
    /// （`LibraryEditor/Graphics/MLibraryV1.cs:433-442`）。
    #[error(
        "{path} 偏移 {offset} 处的像素数据无法识别：{length} 字节，既不是 gzip，也不是 {width}×{height} 的 DXT1"
    )]
    UnsupportedPixels {
        /// 被读取的文件。
        path: PathBuf,
        /// 像素数据的偏移。
        offset: u64,
        /// 记录头里的宽。
        width: u16,
        /// 记录头里的高。
        height: u16,
        /// 记录头里的数据长度。
        length: u32,
    },

    /// 图像下标越界。
    #[error("图像下标 {index} 越界（共 {count} 张）")]
    IndexOutOfRange {
        /// 调用方请求的下标。
        index: usize,
        /// 素材库里的图像数量。
        count: usize,
    },

    /// 声明的尺寸大到不合理，拒绝分配（防止解压炸弹）。
    #[error("{width}×{height} 的图像需要 {needed} 字节，超过单张图上限 {limit} 字节")]
    ImageTooLarge {
        /// 记录头里的宽。
        width: u16,
        /// 记录头里的高。
        height: u16,
        /// 按像素格式算出的解压后字节数。
        needed: u64,
        /// 上限。
        limit: u64,
    },
}
