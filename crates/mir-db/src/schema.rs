//! `.mnd` 的 **DB 原生表编码**（M2）。
//!
//! M1 把 `Server.MirDB` 原始字节塞进 `db` 段；M2 改成把 `mir-db` 解析后的 [`MirDb`]
//! 用 bincode（serde）序列化。这样运行时不再需要 `MirDb::from_bytes` 去解析老格式，
//! 老格式只存在于导入工具。
//!
//! * 段 tag：[`crate::mnd::TAG_DB_BIN`]；
//! * 段名：`db.bin`；
//! * `schema_version`：随字段语义变化升级（当前 2，与 [`crate::mnd::SCHEMA_VERSION`] 一致）。
//!
//! `MirDb.sections`（老文件的字节范围）不参与序列化——它是解析产物，不是数据语义。

use bincode::Options;

use crate::MirDb;

/// DB 原生编码的 schema 版本。
pub const DB_SCHEMA_VERSION: u16 = crate::mnd::SCHEMA_VERSION;

/// bincode 配置：varint 长度/整数编码，比默认 fixint 更省体积（编解码必须成对使用）。
fn options() -> impl Options {
    bincode::DefaultOptions::new().with_varint_encoding()
}

/// DB 编解码错误。
#[derive(Debug, thiserror::Error)]
pub enum DbCodecError {
    /// bincode 编码失败。
    #[error("bincode 编码 DB：{0}")]
    Encode(#[source] Box<bincode::ErrorKind>),
    /// bincode 解码失败（截断/字段不匹配/版本不符）。
    #[error("bincode 解码 DB：{0}")]
    Decode(#[source] Box<bincode::ErrorKind>),
    /// Envir bundle 格式错误。
    #[error("Envir bundle：{0}")]
    Bundle(String),
}

/// 把 [`MirDb`] 编码成原生表字节。
///
/// # Errors
/// bincode 序列化失败（通常只有超大长度字段）。
pub fn encode_db(db: &MirDb) -> Result<Vec<u8>, DbCodecError> {
    options().serialize(db).map_err(DbCodecError::Encode)
}

/// 从原生表字节解码 [`MirDb`]。
///
/// # Errors
/// 字节截断、schema 不匹配或 bincode 解码失败。
pub fn decode_db(bytes: &[u8]) -> Result<MirDb, DbCodecError> {
    options().deserialize(bytes).map_err(DbCodecError::Decode)
}

/// 把 Envir 脚本打成**一个** bundle（路径 → 原始字节）。
///
/// 布局（小端，便于加载端只建索引、不复制正文）：
/// ```text
/// u32 count
/// 重复 count 次：u32 name_len | name | u32 data_len | data
/// ```
///
/// # Errors
/// 条目/长度超出 `u32`。
pub fn encode_envir(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, DbCodecError> {
    let count = u32::try_from(files.len())
        .map_err(|_| DbCodecError::Bundle("脚本条数超过 u32".to_owned()))?;
    let mut out = Vec::with_capacity(
        4 + files
            .iter()
            .map(|(name, data)| 8 + name.len() + data.len())
            .sum::<usize>(),
    );
    out.extend_from_slice(&count.to_le_bytes());
    for (name, data) in files {
        let name_len =
            u32::try_from(name.len()).map_err(|_| DbCodecError::Bundle("脚本名过长".to_owned()))?;
        let data_len =
            u32::try_from(data.len()).map_err(|_| DbCodecError::Bundle("脚本过大".to_owned()))?;
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&data_len.to_le_bytes());
        out.extend_from_slice(data);
    }
    Ok(out)
}

/// 解码 Envir bundle（复制每份脚本正文；导入工具 `--verify` 用）。
///
/// # Errors
/// 截断/长度不合法。
pub fn decode_envir(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, DbCodecError> {
    let (blob, index) = decode_envir_index(bytes.to_vec())?;
    Ok(index
        .into_iter()
        .map(|(name, start, len)| (name, blob[start..start + len].to_vec()))
        .collect())
}

/// Envir bundle 索引：`(路径, blob 起始偏移, 长度)`。
pub type EnvirIndex = Vec<(String, usize, usize)>;

/// 解码 Envir bundle 的索引：返回 `(原始 blob, [(路径, 起始偏移, 长度)])`。
///
/// 加载端用这个版本，避免为 1,139 份脚本各分配一个 `String`。
///
/// # Errors
/// 截断/长度不合法。
pub fn decode_envir_index(blob: Vec<u8>) -> Result<(Vec<u8>, EnvirIndex), DbCodecError> {
    let mut cursor = 0usize;
    let count = read_u32(&blob, &mut cursor)?;
    let mut index = Vec::with_capacity(count);
    for _ in 0..count {
        let name_len = read_u32(&blob, &mut cursor)?;
        let name_end = cursor
            .checked_add(name_len)
            .filter(|end| *end <= blob.len())
            .ok_or_else(|| DbCodecError::Bundle("脚本名越界".to_owned()))?;
        let name = String::from_utf8(blob[cursor..name_end].to_vec())
            .map_err(|_| DbCodecError::Bundle("脚本名不是 UTF-8".to_owned()))?;
        cursor = name_end;
        let data_len = read_u32(&blob, &mut cursor)?;
        let data_end = cursor
            .checked_add(data_len)
            .filter(|end| *end <= blob.len())
            .ok_or_else(|| DbCodecError::Bundle("脚本正文越界".to_owned()))?;
        index.push((name, cursor, data_len));
        cursor = data_end;
    }
    if cursor != blob.len() {
        return Err(DbCodecError::Bundle("bundle 尾部有多余字节".to_owned()));
    }
    Ok((blob, index))
}

fn read_u32(blob: &[u8], cursor: &mut usize) -> Result<usize, DbCodecError> {
    let end = cursor
        .checked_add(4)
        .filter(|end| *end <= blob.len())
        .ok_or_else(|| DbCodecError::Bundle("bundle 截断".to_owned()))?;
    let bytes: [u8; 4] = blob[*cursor..end]
        .try_into()
        .map_err(|_| DbCodecError::Bundle("bundle 截断".to_owned()))?;
    *cursor = end;
    Ok(u32::from_le_bytes(bytes) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_default_db() {
        let db = MirDb::default();
        let bytes = encode_db(&db).expect("encode");
        let back = decode_db(&bytes).expect("decode");
        assert_eq!(back, db);
    }

    #[test]
    fn envir_bundle_roundtrip_and_index() {
        let files = vec![
            ("NPCs/A.txt".to_owned(), b"hello".to_vec()),
            ("Drops/B.txt".to_owned(), b"1/10 gold 10".to_vec()),
        ];
        let bytes = encode_envir(&files).expect("encode");
        let decoded = decode_envir(&bytes).expect("decode");
        assert_eq!(decoded, files);
        let (blob, index) = decode_envir_index(bytes).expect("index");
        assert_eq!(index.len(), 2);
        for (name, start, len) in index {
            let expected = files.iter().find(|(file, _)| file == &name).expect("name");
            assert_eq!(&blob[start..start + len], expected.1.as_slice());
        }
    }

    #[test]
    fn rejects_truncated_envir_bundle() {
        let files = vec![("A.txt".to_owned(), b"x".to_vec())];
        let mut bytes = encode_envir(&files).expect("encode");
        bytes.pop();
        assert!(decode_envir_index(bytes).is_err());
    }
}
