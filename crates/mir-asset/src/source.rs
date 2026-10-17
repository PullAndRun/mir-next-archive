//! 带路径与偏移的文件读取小工具。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::AssetError;

/// 打开文件；失败时把路径一起塞进 [`AssetError::Io`]。
pub(crate) fn open_file(path: &Path) -> Result<File, AssetError> {
    File::open(path).map_err(|source| io_error(path, source))
}

/// 读文件长度。
pub(crate) fn file_len(path: &Path, file: &File) -> Result<u64, AssetError> {
    file.metadata()
        .map(|metadata| metadata.len())
        .map_err(|source| io_error(path, source))
}

/// 从 `offset` 起读满 `buf`。
///
/// 调用方应当先用文件长度做过边界检查，因此这里的 `read_exact` 失败只可能是并发截断
/// 或 I/O 错误，统一按 [`AssetError::Io`] 上报。
pub(crate) fn read_at(
    file: &mut File,
    path: &Path,
    offset: u64,
    buf: &mut [u8],
) -> Result<(), AssetError> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|source| io_error(path, source))?;
    file.read_exact(buf)
        .map_err(|source| io_error(path, source))
}

/// 构造一个带路径的 [`AssetError::Io`]。
pub(crate) fn io_error(path: &Path, source: std::io::Error) -> AssetError {
    AssetError::Io {
        path: path.to_path_buf(),
        source,
    }
}
