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

use crate::MirDb;

/// DB 原生编码的 schema 版本。
pub const DB_SCHEMA_VERSION: u16 = crate::mnd::SCHEMA_VERSION;

/// DB 编解码错误。
#[derive(Debug, thiserror::Error)]
pub enum DbCodecError {
    /// bincode 编码失败。
    #[error("bincode 编码 DB：{0}")]
    Encode(#[source] Box<bincode::ErrorKind>),
    /// bincode 解码失败（截断/字段不匹配/版本不符）。
    #[error("bincode 解码 DB：{0}")]
    Decode(#[source] Box<bincode::ErrorKind>),
}

/// 把 [`MirDb`] 编码成原生表字节。
///
/// # Errors
/// bincode 序列化失败（通常只有超大长度字段）。
pub fn encode_db(db: &MirDb) -> Result<Vec<u8>, DbCodecError> {
    bincode::serialize(db).map_err(DbCodecError::Encode)
}

/// 从原生表字节解码 [`MirDb`]。
///
/// # Errors
/// 字节截断、schema 不匹配或 bincode 解码失败。
pub fn decode_db(bytes: &[u8]) -> Result<MirDb, DbCodecError> {
    bincode::deserialize(bytes).map_err(DbCodecError::Decode)
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
}
