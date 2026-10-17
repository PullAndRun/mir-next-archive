//! `Server.MirDB` 的二进制读写原语。
//!
//! 文件里只有三种东西：小端定宽整数、.NET `BinaryWriter.Write(string)` 的
//! 7 位变长长度前缀 + UTF-8、以及 1 字节布尔。
//!
//! 读取侧包了一层 [`mir_protocol::PacketReader`]——那套原语已经在 `mir-protocol`
//! 里按 Crystal 的 `BinaryReader` 逐字节对齐（见 `crates/mir-protocol/src/lib.rs`
//! 的 `string_roundtrip_matches_dotnet_prefix` 等测试），不必重写。本模块在它之上补三件事：
//!
//! * **上下文**：记住当前段名与文件版本，报错时带绝对偏移（
//!   `docs/store/mirdb-format.md` §6.2 要求「读到非法 bool / 离谱 count 时立即带偏移量报错」）；
//! * **严格布尔**：值必须 ∈ {0, 1}，否则视为错位（.NET 的 `ReadBoolean` 是「非 0 即真」，
//!   会静默吞掉错位）；
//! * **长度上限**：`i32` 计数先查负数与上限，再用「每条记录至少 `min_record_size` 字节」
//!   与剩余字节数交叉校验，避免一个坏计数直接触发巨额分配。
//!
//! 写入侧 [`DbWriter`] 是镜像实现，供 `encode`（以及未来 `SaveDB`）使用。

use mir_protocol::{PacketReader, ProtocolError};

use crate::MirDbError;

/// 单个列表的长度上限：超过它几乎肯定是错位而不是真数据。
pub const MAX_LIST_LEN: usize = 1 << 20;

/// 带段名与版本上下文的读取器。
#[derive(Debug)]
pub struct DbReader<'a> {
    /// 完整文件字节，用于取剩余切片（`PacketReader` 只暴露位置）。
    bytes: &'a [u8],
    /// 底层原语读取器。
    inner: PacketReader<'a>,
    /// 文件头里的 `Version`（`Envir.cs:2833`），读到之前先用 0 占位。
    version: i32,
    /// 当前所在段名，仅用于报错。
    section: &'static str,
}

impl<'a> DbReader<'a> {
    /// 在给定字节上创建读取器。
    ///
    /// `version` 用于选择版本分支；读文件头之前还不知道版本，构造后可用
    /// [`DbReader::set_version`] 补上。
    #[must_use]
    pub const fn new(bytes: &'a [u8], version: i32) -> Self {
        Self {
            bytes,
            inner: PacketReader::new(bytes),
            version,
            section: "未知段",
        }
    }

    /// 已消耗字节数。
    #[must_use]
    pub const fn position(&self) -> usize {
        self.inner.position()
    }

    /// 剩余字节数。
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.inner.remaining()
    }

    /// 当前文件版本（`LoadVersion`，`Envir.cs:2833`）。
    #[must_use]
    pub const fn version(&self) -> i32 {
        self.version
    }

    /// 设置文件版本（读完文件头后调用）。
    pub const fn set_version(&mut self, version: i32) {
        self.version = version;
    }

    /// 设置当前段名（影响后续报错文本）。
    pub const fn set_section(&mut self, section: &'static str) {
        self.section = section;
    }

    /// 尚未读到的字节切片。
    #[must_use]
    pub fn rest(&self) -> &'a [u8] {
        &self.bytes[self.inner.position()..]
    }

    /// 丢弃剩余全部字节。
    pub fn consume_rest(&mut self) {
        let _ = self.inner.read_rest();
    }

    /// 底层读取器：复用 `mir-protocol` 里的解码实现（`Stats` / `ItemInfo` / `GameShopItem`）时使用。
    pub fn packet(&mut self) -> &mut PacketReader<'a> {
        &mut self.inner
    }

    /// 把底层错误包成带上下文的 [`MirDbError::Truncated`]。
    pub(crate) fn truncated(&self, source: ProtocolError) -> MirDbError {
        MirDbError::Truncated {
            section: self.section,
            offset: self.inner.position(),
            version: self.version,
            source,
        }
    }

    /// 读取 `u8`。
    pub fn u8(&mut self) -> Result<u8, MirDbError> {
        self.inner.read_u8().map_err(|error| self.truncated(error))
    }

    /// 读取 `i8`（C# 的 `ReadSByte`，用于旧版属性表里的 `Luck` / `AttackSpeed`）。
    pub fn i8(&mut self) -> Result<i8, MirDbError> {
        self.inner.read_i8().map_err(|error| self.truncated(error))
    }

    /// 读取严格布尔：只接受 0 / 1。
    ///
    /// C# 的 `BinaryReader.ReadBoolean` 把任何非 0 字节当 `true`，错位到随机数据时
    /// 会一路读下去；`docs/store/mirdb-format.md` §6.2 建议这里直接失败。
    pub fn bool(&mut self) -> Result<bool, MirDbError> {
        let offset = self.inner.position();
        let value = self.u8()?;
        match value {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(MirDbError::InvalidBool {
                section: self.section,
                offset,
                version: self.version,
                value: other,
            }),
        }
    }

    /// 读取 `u16`。
    pub fn u16(&mut self) -> Result<u16, MirDbError> {
        self.inner.read_u16().map_err(|error| self.truncated(error))
    }

    /// 读取 `i16`。
    pub fn i16(&mut self) -> Result<i16, MirDbError> {
        self.inner.read_i16().map_err(|error| self.truncated(error))
    }

    /// 读取 `u32`。
    pub fn u32(&mut self) -> Result<u32, MirDbError> {
        self.inner.read_u32().map_err(|error| self.truncated(error))
    }

    /// 读取 `i32`。
    pub fn i32(&mut self) -> Result<i32, MirDbError> {
        self.inner.read_i32().map_err(|error| self.truncated(error))
    }

    /// 读取 `u64`。
    pub fn u64(&mut self) -> Result<u64, MirDbError> {
        self.inner.read_u64().map_err(|error| self.truncated(error))
    }

    /// 读取 `i64`（`DateTime.ToBinary()` 与龙系统经验都走这里）。
    pub fn i64(&mut self) -> Result<i64, MirDbError> {
        self.inner.read_i64().map_err(|error| self.truncated(error))
    }

    /// 读取 `f32`。
    pub fn f32(&mut self) -> Result<f32, MirDbError> {
        self.inner.read_f32().map_err(|error| self.truncated(error))
    }

    /// 读取 `f64`。
    pub fn f64(&mut self) -> Result<f64, MirDbError> {
        self.inner.read_f64().map_err(|error| self.truncated(error))
    }

    /// 读取 7 位长度前缀的 UTF-8 字符串（`BinaryWriter.Write(string)`）。
    pub fn string(&mut self) -> Result<String, MirDbError> {
        self.inner
            .read_string()
            .map_err(|error| self.truncated(error))
    }

    /// 读取列表长度（`i32`），并校验非负与 [`MAX_LIST_LEN`] 上限。
    pub fn count(&mut self) -> Result<usize, MirDbError> {
        let offset = self.inner.position();
        let raw = self.i32()?;
        match usize::try_from(raw) {
            Ok(value) if value <= MAX_LIST_LEN => Ok(value),
            _ => Err(MirDbError::CountOutOfRange {
                section: self.section,
                offset,
                version: self.version,
                value: i64::from(raw),
                limit: MAX_LIST_LEN,
            }),
        }
    }

    /// 读 `i32` 计数 + `count` 条记录。
    ///
    /// `min_record_size` 是该类型记录的最小可能字节数（用于「计数不可能大于剩余字节数」的前置校验）。
    /// 闭包只拿到读取器本身，需要版本时用 [`DbReader::version`]。
    pub fn list<T>(
        &mut self,
        min_record_size: usize,
        mut read: impl FnMut(&mut Self) -> Result<T, MirDbError>,
    ) -> Result<Vec<T>, MirDbError> {
        let offset = self.inner.position();
        let count = self.count()?;
        let needed = count.saturating_mul(min_record_size);
        if needed > self.remaining() {
            return Err(MirDbError::ListOverrunsFile {
                section: self.section,
                offset,
                version: self.version,
                count,
                needed,
                remaining: self.remaining(),
            });
        }
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            out.push(read(self)?);
        }
        Ok(out)
    }
}

/// 按 Crystal 的写出顺序拼装字节。
#[derive(Debug, Default)]
pub struct DbWriter {
    /// 输出缓冲。
    buffer: Vec<u8>,
}

impl DbWriter {
    /// 新建写入器。
    #[must_use]
    pub const fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// 预分配容量。
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
        }
    }

    /// 取出内部缓冲。
    #[must_use]
    pub fn into_inner(self) -> Vec<u8> {
        self.buffer
    }

    /// 当前长度。
    #[must_use]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// 是否还没写任何字节。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// 追加原始字节。
    pub fn raw(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// 写入 `u8`。
    pub fn u8(&mut self, value: u8) {
        self.buffer.push(value);
    }

    /// 写入 `i8`。
    pub fn i8(&mut self, value: i8) {
        self.buffer.push(value as u8);
    }

    /// 写入布尔（0 / 1）。
    pub fn bool(&mut self, value: bool) {
        self.buffer.push(u8::from(value));
    }

    /// 写入 `u16`。
    pub fn u16(&mut self, value: u16) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// 写入 `i16`。
    pub fn i16(&mut self, value: i16) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// 写入 `u32`。
    pub fn u32(&mut self, value: u32) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// 写入 `i32`。
    pub fn i32(&mut self, value: i32) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// 写入 `u64`。
    pub fn u64(&mut self, value: u64) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// 写入 `i64`。
    pub fn i64(&mut self, value: i64) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// 写入 `f32`。
    pub fn f32(&mut self, value: f32) {
        self.buffer
            .extend_from_slice(&value.to_bits().to_le_bytes());
    }

    /// 写入 `f64`。
    pub fn f64(&mut self, value: f64) {
        self.buffer
            .extend_from_slice(&value.to_bits().to_le_bytes());
    }

    /// 写入 7 位长度前缀 + UTF-8。
    pub fn string(&mut self, value: &str) {
        let mut length = value.len();
        loop {
            let mut byte = (length & 0x7F) as u8;
            length >>= 7;
            if length != 0 {
                byte |= 0x80;
            }
            self.buffer.push(byte);
            if length == 0 {
                break;
            }
        }
        self.buffer.extend_from_slice(value.as_bytes());
    }

    /// 写入列表长度（`i32`）。
    pub fn count(&mut self, value: usize) {
        self.i32(value as i32);
    }

    /// 写 `i32` 计数 + 逐条回调。
    pub fn list<T>(&mut self, items: &[T], mut write: impl FnMut(&mut Self, &T)) {
        self.count(items.len());
        for item in items {
            write(self, item);
        }
    }

    /// 写入属性表：`i32` 项数 + 逐项 `(u8 stat, i32 value)`。
    ///
    /// 见 `Shared/Data/Stat.cs:50-59`（`Stats.Save`）。`Stats` 用 `BTreeMap` 存储，
    /// 迭代顺序即键的判别值升序，与 C# `SortedDictionary` 一致（`crates/mir-protocol/src/data/user_item.rs:26-36`）。
    pub fn stats(&mut self, stats: &mir_protocol::data::Stats) {
        self.count(stats.len());
        for (&stat, &value) in stats.values() {
            self.u8(stat.value());
            self.i32(value);
        }
    }
}
