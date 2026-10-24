//! 旧版 `.Lib` 末尾的可选帧表（Crystal `Frames.cs:231-243`）。
//!
//! v3 且头里第三个 `i32` 不是「偏移表末尾」时，第三个字段是 `frameSeek`：
//! `frameSeek + 4` 处是 `frameCount: i32`，随后是 `frameCount × 35` 字节。
//! 每条记录为 `action: u8` + 8 个 `i32` + `Reverse: bool` + `Blend: bool`。
//! v1/v2 和「v3 无 frameSeek」的库没有帧表；v4 明确拒绝。
//!
//! 这里只解析元数据，不碰图像解码；`ImageLibrary::frames()` 在 `legacy` 下提供
//! 同一实现。动作号保留原始 `u8`，未知动作不会被猜成已知名字。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::AssetError;

/// 单条帧记录的长度：1 + 8 × 4 + 1 + 1。
pub const FRAME_RECORD_LEN: u64 = 35;

/// 防止畸形帧表让解析器分配巨量内存。
pub const MAX_FRAME_RECORDS: u32 = 100_000;

/// 一条动作的帧表记录（Crystal `Frames.cs:231-243`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    /// 原始动作号；不认识的号原样保留。
    pub action: u8,
    /// 首帧下标。Crystal 的公式是 `Start + Direction * (Count + Skip)`。
    pub start: i32,
    /// 帧数。
    pub count: i32,
    /// 方向步长附加量：`Count + Skip` 是相邻方向的帧号差，可以为 0
    /// （8 方向共享同一批帧）或负数（方向号越大起点越小）。
    pub skip: i32,
    /// 帧间隔，毫秒。
    pub interval: i32,
    /// 特效首帧。
    pub effect_start: i32,
    /// 特效帧数。
    pub effect_count: i32,
    /// 特效方向附加量。
    pub effect_skip: i32,
    /// 特效帧间隔。
    pub effect_interval: i32,
    /// 是否倒放。
    pub reverse: bool,
    /// 是否混合（Crystal `Blend`）。
    pub blend: bool,
}

impl Frame {
    /// 已知动作名；未知返回 `None`，调用方应使用 [`Frame::raw_action_name`] 或
    /// 自己保留 `action`。
    #[must_use]
    pub const fn action_name(&self) -> Option<&'static str> {
        action_name(self.action)
    }

    /// 未知动作的稳定展示名；已知动作返回 `None`。
    #[must_use]
    pub fn raw_action_name(&self) -> Option<String> {
        self.action_name()
            .is_none()
            .then(|| format!("raw:{}", self.action))
    }

    /// 帧表记录的布局是否可用；返回第一条不可用原因。
    ///
    /// 与 `.Lib` 解析共用同一套规则，避免 `frames.json` 消费端接受
    /// `.Lib` 解析端拒绝的布局。Crystal 的取帧公式是
    /// `Start + Direction * (Count + Skip)`（`Client/MirObjects/MonsterObject.cs:357`，
    /// 共 8 个方向），所以只有按该公式算出的某个方向帧区间出现负下标、
    /// 或末端越过 `i32` 上界时才算不可用：
    ///
    /// * `Count > 1 && Count + Skip == 0`：8 方向共享同一批帧（stride 0）；
    /// * `Count + Skip < 0`：方向号越大起点越小，如
    ///   `Start=7, Count=2, Skip=-3` 使 8 个方向的起点是 7..0；
    /// * `Count == 1` 时 `Skip` 也可以是 `-1` 以下的负数，如
    ///   `Start=7, Count=1, Skip=-2` 使 8 个方向的起点是 7..0。
    ///
    /// Crystal 读表时不做这些校验，把合法的负步长拒掉会让真实怪物缺动作。
    #[must_use]
    pub fn layout_error(&self) -> Option<&'static str> {
        if self.start < 0 || self.count < 0 || self.interval < 0 {
            return Some("负的 Start/Count/Interval");
        }
        // Count==0 不访问任何帧，没有“某方向的区间”可越界，保持放行。
        if self.count > 0 {
            let stride = i64::from(self.count) + i64::from(self.skip);
            for direction in 0..8i64 {
                let start = i64::from(self.start) + direction * stride;
                let end = start + i64::from(self.count);
                if start < 0 {
                    return Some("某方向的帧下标为负");
                }
                if end > i64::from(i32::MAX) {
                    return Some("帧下标溢出 i32");
                }
            }
        }
        if self.effect_count < 0 || self.effect_start < 0 || self.effect_interval < 0 {
            return Some("负的特效 Start/Count/Interval");
        }
        None
    }

    /// 该动作在某个方向上的帧下标范围；`None` 表示方向越界，或按 Crystal
    /// 公式算出的区间有负下标/越过 `i32` 上界（见 [`Frame::layout_error`]）。
    ///
    /// `Count + Skip` 为 0 或负数时同样按公式返回有效区间。
    #[must_use]
    pub fn frame_indices(&self, direction: usize) -> Option<std::ops::Range<i32>> {
        if direction > 7 || self.count < 0 {
            return None;
        }
        let stride = i64::from(self.count) + i64::from(self.skip);
        let start = i64::from(self.start) + direction as i64 * stride;
        let end = start + i64::from(self.count);
        if start < 0 || end > i64::from(i32::MAX) {
            return None;
        }
        Some(start as i32..end as i32)
    }
}

/// 一个 `.Lib` 的完整帧表。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameSet {
    /// 按文件顺序保存的记录；动作号是原始字节。
    pub frames: Vec<Frame>,
    /// 出现但不认识的动作号（去重、保持首次出现顺序）。
    pub unknown_actions: Vec<u8>,
}

impl FrameSet {
    /// 按动作号查找；重复动作在解析时已拒绝，所以最多一条。
    #[must_use]
    pub fn get(&self, action: u8) -> Option<&Frame> {
        self.frames.iter().find(|frame| frame.action == action)
    }

    /// 已知动作集合，按文件顺序。
    #[must_use]
    pub fn known_actions(&self) -> Vec<&'static str> {
        self.frames.iter().filter_map(Frame::action_name).collect()
    }

    /// 从 `.Lib` 文件读取帧表。v1/v2 和无 frameSeek 的 v3 返回 `Ok(None)`；
    /// v4、损坏的表返回 `Err`。
    pub fn from_file(path: &Path) -> Result<Option<Self>, AssetError> {
        let data = std::fs::read(path).map_err(|source| AssetError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let file_len = data.len() as u64;
        let available = data.len().min(12);
        Self::from_header(
            path,
            file_len,
            &data[..available],
            &mut |offset, buf: &mut [u8]| {
                let start = usize::try_from(offset).map_err(|_| AssetError::Truncated {
                    path: path.to_path_buf(),
                    offset,
                    needed: buf.len() as u64,
                    actual: 0,
                })?;
                let end = start
                    .checked_add(buf.len())
                    .ok_or_else(|| AssetError::Truncated {
                        path: path.to_path_buf(),
                        offset,
                        needed: buf.len() as u64,
                        actual: 0,
                    })?;
                let slice = data.get(start..end).ok_or_else(|| AssetError::Truncated {
                    path: path.to_path_buf(),
                    offset,
                    needed: buf.len() as u64,
                    actual: data.len().saturating_sub(start) as u64,
                })?;
                buf.copy_from_slice(slice);
                Ok(())
            },
        )
    }

    /// 从已读入的头部和读取回调解析；供测试与 `LibReader` 复用。
    pub(crate) fn from_header<F>(
        path: &Path,
        file_len: u64,
        prefix: &[u8],
        mut read_at: F,
    ) -> Result<Option<Self>, AssetError>
    where
        F: FnMut(u64, &mut [u8]) -> Result<(), AssetError>,
    {
        let version = read_i32(prefix, 0, path, 0)?;
        let raw_count = read_i32(prefix, 4, path, 4)?;
        if !matches!(version, 1..=3) {
            return Err(AssetError::BadMagic {
                path: path.to_path_buf(),
                offset: 0,
                expected: "旧版 .Lib 版本 1/2/3（v4 明确拒绝）",
                found: u32::from_le_bytes([prefix[0], prefix[1], prefix[2], prefix[3]]),
            });
        }
        if raw_count < 0 {
            return Err(corrupt(path, 4, format!("图像数量为负：{raw_count}")));
        }
        if version < 3 {
            return Ok(None);
        }
        let third = read_u32(prefix, 8, path, 8)?;
        let index_end = 8 + u64::from(raw_count as u32) * 4;
        if u64::from(third) == index_end {
            return Ok(None);
        }
        let frame_seek = u64::from(third);
        if frame_seek < 12 + u64::from(raw_count as u32) * 4 {
            return Err(corrupt(
                path,
                8,
                format!("frameSeek {frame_seek} 落在偏移表内"),
            ));
        }
        if frame_seek + 4 > file_len {
            return Err(AssetError::Truncated {
                path: path.to_path_buf(),
                offset: frame_seek,
                needed: 4,
                actual: file_len.saturating_sub(frame_seek),
            });
        }
        let mut count_buf = [0u8; 4];
        read_at(frame_seek, &mut count_buf)?;
        let raw_frame_count = i32::from_le_bytes(count_buf);
        if raw_frame_count < 0 {
            return Err(corrupt(
                path,
                frame_seek,
                format!("帧表数量为负：{raw_frame_count}"),
            ));
        }
        let frame_count = raw_frame_count as u32;
        if frame_count > MAX_FRAME_RECORDS {
            return Err(corrupt(
                path,
                frame_seek,
                format!("帧表数量 {frame_count} 超过上限 {MAX_FRAME_RECORDS}"),
            ));
        }
        let expected = u64::from(frame_count)
            .checked_mul(FRAME_RECORD_LEN)
            .ok_or_else(|| corrupt(path, frame_seek, "帧表长度溢出"))?;
        let bytes_start = frame_seek + 4;
        let remaining = file_len.saturating_sub(bytes_start);
        if remaining != expected {
            return Err(corrupt(
                path,
                bytes_start,
                format!("帧表剩余 {remaining} 字节，应为 {expected} 字节"),
            ));
        }
        let mut bytes = vec![0u8; usize::try_from(expected).unwrap_or(0)];
        read_at(bytes_start, &mut bytes)?;
        Self::parse_records(path, &bytes)
    }

    /// 从已定位的 35 字节记录区解析；空切片得到空集合。
    pub(crate) fn parse_records(path: &Path, bytes: &[u8]) -> Result<Option<Self>, AssetError> {
        if bytes.len() as u64 % FRAME_RECORD_LEN != 0 {
            return Err(corrupt(
                path,
                0,
                format!("帧表字节数 {} 不是 35 的倍数", bytes.len()),
            ));
        }
        let mut frames = Vec::with_capacity(bytes.len() / FRAME_RECORD_LEN as usize);
        let mut unknown = Vec::new();
        for (index, record) in bytes.chunks_exact(FRAME_RECORD_LEN as usize).enumerate() {
            let action = record[0];
            if frames.iter().any(|frame: &Frame| frame.action == action) {
                return Err(corrupt(
                    path,
                    index as u64 * FRAME_RECORD_LEN,
                    format!("重复动作号 {action}"),
                ));
            }
            let ints = |offset: usize| -> i32 {
                i32::from_le_bytes([
                    record[offset],
                    record[offset + 1],
                    record[offset + 2],
                    record[offset + 3],
                ])
            };
            let reverse = bool_byte(path, record[33], index, 33)?;
            let blend = bool_byte(path, record[34], index, 34)?;
            let frame = Frame {
                action,
                start: ints(1),
                count: ints(5),
                skip: ints(9),
                interval: ints(13),
                effect_start: ints(17),
                effect_count: ints(21),
                effect_skip: ints(25),
                effect_interval: ints(29),
                reverse,
                blend,
            };
            validate_frame(path, index as u64 * FRAME_RECORD_LEN, &frame)?;
            if frame.action_name().is_none() {
                unknown.push(action);
            }
            frames.push(frame);
        }
        Ok(Some(Self {
            frames,
            unknown_actions: unknown,
        }))
    }
}

/// Crystal `MirAction` 中本模块认识的动作号。
#[must_use]
pub const fn action_name(action: u8) -> Option<&'static str> {
    Some(match action {
        0 => "Standing",
        1 => "Walking",
        2 => "Running",
        9 => "Attack1",
        10 => "Attack2",
        11 => "Attack3",
        12 => "Attack4",
        18 => "Struck",
        21 => "Die",
        22 => "Dead",
        28 => "Revive",
        _ => return None,
    })
}

fn validate_frame(path: &Path, offset: u64, frame: &Frame) -> Result<(), AssetError> {
    if let Some(reason) = frame.layout_error() {
        let detail = match reason {
            "负的 Start/Count/Interval" => format!("负的 Start/Count/Interval：{frame:?}"),
            "某方向的帧下标为负" => format!(
                "Start={} Count={} Skip={} 使某方向的帧下标为负",
                frame.start, frame.count, frame.skip
            ),
            "帧下标溢出 i32" => format!(
                "Start={} Count={} Skip={} 使某方向的帧下标溢出 i32",
                frame.start, frame.count, frame.skip
            ),
            "负的特效 Start/Count/Interval" => {
                format!("负的特效 Start/Count/Interval：{frame:?}")
            }
            other => other.to_owned(),
        };
        return Err(corrupt(path, offset, detail));
    }
    Ok(())
}

fn bool_byte(path: &Path, value: u8, index: usize, field: usize) -> Result<bool, AssetError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(corrupt(
            path,
            index as u64 * FRAME_RECORD_LEN + field as u64,
            format!("非法布尔值 {value}（只接受 0/1）"),
        )),
    }
}

fn read_i32(prefix: &[u8], offset: usize, path: &Path, abs: u64) -> Result<i32, AssetError> {
    let bytes = prefix
        .get(offset..offset + 4)
        .ok_or_else(|| AssetError::Truncated {
            path: path.to_path_buf(),
            offset: abs,
            needed: 4,
            actual: prefix.len().saturating_sub(offset) as u64,
        })?;
    Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u32(prefix: &[u8], offset: usize, path: &Path, abs: u64) -> Result<u32, AssetError> {
    let bytes = prefix
        .get(offset..offset + 4)
        .ok_or_else(|| AssetError::Truncated {
            path: path.to_path_buf(),
            offset: abs,
            needed: 4,
            actual: prefix.len().saturating_sub(offset) as u64,
        })?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn corrupt(path: &Path, offset: u64, message: impl Into<String>) -> AssetError {
    AssetError::Corrupt {
        path: PathBuf::from(path),
        offset,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> &'static Path {
        Path::new("<memory>")
    }

    fn record(action: u8, start: i32, count: i32, skip: i32) -> Vec<u8> {
        let mut bytes = vec![0u8; 35];
        bytes[0] = action;
        for (offset, value) in [
            (1, start),
            (5, count),
            (9, skip),
            (13, 100),
            (17, 0),
            (21, 0),
            (25, 0),
            (29, 0),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    /// 直接构造一条记录，用于只测布局规则、不经过 35 字节记录的用例。
    fn frame_of(start: i32, count: i32, skip: i32) -> Frame {
        Frame {
            action: 0,
            start,
            count,
            skip,
            interval: 100,
            effect_start: 0,
            effect_count: 0,
            effect_skip: 0,
            effect_interval: 0,
            reverse: false,
            blend: false,
        }
    }

    #[test]
    fn parses_crystal_record_layout_and_unknown_action() {
        let mut bytes = record(0, 0, 4, 0);
        bytes.extend(record(99, 32, 6, 0));
        let set = FrameSet::parse_records(path(), &bytes)
            .expect("合法帧表")
            .expect("有记录");
        assert_eq!(set.frames.len(), 2);
        assert_eq!(set.frames[0].action_name(), Some("Standing"));
        assert_eq!(set.frames[0].frame_indices(2), Some(8..12));
        assert_eq!(set.frames[1].action, 99);
        assert_eq!(set.frames[1].action_name(), None);
        assert_eq!(set.unknown_actions, vec![99]);
    }

    #[test]
    fn accepts_skip_minus_one_for_single_static_frame() {
        let bytes = record(0, 18, 1, -1);
        let set = FrameSet::parse_records(path(), &bytes)
            .expect("Skip=-1 + Count=1 合法")
            .expect("有记录");
        assert_eq!(set.frames[0].frame_indices(7), Some(18..19));
    }

    #[test]
    fn rejects_negative_count_and_bad_bool() {
        let mut bytes = record(0, 0, -1, 0);
        assert!(FrameSet::parse_records(path(), &bytes).is_err());
        bytes = record(0, 0, 1, 0);
        bytes[33] = 2;
        assert!(FrameSet::parse_records(path(), &bytes).is_err());
    }

    #[test]
    fn rejects_negative_interval_negative_start_and_out_of_range_start() {
        // 负 Interval 不是合法的“负 Skip”；仍必须拒绝。
        let mut bytes = record(0, 0, 4, 0);
        bytes[13..17].copy_from_slice(&(-1i32).to_le_bytes());
        assert!(FrameSet::parse_records(path(), &bytes).is_err());
        // 负 Start。
        let bytes = record(0, -1, 4, 0);
        assert!(FrameSet::parse_records(path(), &bytes).is_err());
        // 8 方向的末端越界 i32。
        let bytes = record(0, i32::MAX - 2, 4, 0);
        assert!(FrameSet::parse_records(path(), &bytes).is_err());
    }

    #[test]
    fn rejects_duplicate_action() {
        let mut bytes = record(0, 0, 4, 0);
        bytes.extend(record(0, 32, 4, 0));
        assert!(FrameSet::parse_records(path(), &bytes).is_err());
    }

    #[test]
    fn accepts_stride_zero_and_shares_frames_across_directions() {
        let mut bytes = record(1, 64, 6, -6);
        bytes.extend(record(2, 128, 20, -20));
        bytes.extend(record(21, 640, 1, -1));
        let set = FrameSet::parse_records(path(), &bytes)
            .expect("stride=0 合法")
            .expect("有记录");
        for frame in [&set.frames[0], &set.frames[1]] {
            let first = frame.frame_indices(0);
            assert!(first.is_some());
            for direction in 0..=7 {
                assert_eq!(frame.frame_indices(direction), first);
            }
        }
        assert_eq!(set.frames[0].frame_indices(7), Some(64..70));
        assert_eq!(set.frames[1].frame_indices(7), Some(128..148));
        assert_eq!(set.frames[2].frame_indices(7), Some(640..641));
    }

    #[test]
    fn accepts_negative_stride_and_indexes_all_eight_directions() {
        // Crystal 公式：首帧 = Start + Direction * (Count + Skip)。
        // Start=7 Count=2 Skip=-3 → stride=-1，8 方向起点 7,6,…,0 全部非负。
        let frame = frame_of(7, 2, -3);
        assert_eq!(
            frame.layout_error(),
            None,
            "Crystal 可用的负 stride 不应报错"
        );
        let mut starts = Vec::new();
        for direction in 0..8usize {
            let range = frame
                .frame_indices(direction)
                .unwrap_or_else(|| panic!("方向 {direction} 应可用"));
            assert_eq!(
                range,
                (7 - direction as i32)..(9 - direction as i32),
                "方向 {direction} 的帧区间"
            );
            starts.push(range.start);
        }
        assert_eq!(starts, vec![7, 6, 5, 4, 3, 2, 1, 0]);
        assert_eq!(frame.frame_indices(8), None, "只有 8 个方向");
    }

    #[test]
    fn parses_negative_stride_record_accepted_by_crystal() {
        // 经 35 字节记录走一遍解析器，确认负 stride 不再被解析层拒绝。
        let bytes = record(1, 7, 2, -3);
        let set = FrameSet::parse_records(path(), &bytes)
            .expect("负 stride 帧表合法")
            .expect("有记录");
        assert_eq!(set.frames[0].frame_indices(7), Some(0..2));
        assert_eq!(set.frames[0].frame_indices(0), Some(7..9));
    }

    #[test]
    fn rejects_negative_stride_when_a_direction_starts_below_zero() {
        // Start=0 Count=2 Skip=-3 → stride=-1，方向 7 的起点是 -7，不可用。
        let frame = frame_of(0, 2, -3);
        assert_eq!(frame.layout_error(), Some("某方向的帧下标为负"));
        assert_eq!(frame.frame_indices(7), None);
        let error = FrameSet::parse_records(path(), &record(1, 0, 2, -3))
            .expect_err("方向 7 的帧下标为负必须拒绝");
        assert!(
            format!("{error}").contains("帧下标为负"),
            "错误信息应指出负下标：{error}"
        );
        // 旧的拒绝样例 Start=0 Count=6 Skip=-7 在新规则下仍不可用。
        assert!(FrameSet::parse_records(path(), &record(1, 0, 6, -7)).is_err());
    }

    #[test]
    fn accepts_negative_skip_for_single_frame() {
        // Start=7 Count=1 Skip=-2 → stride=-1，8 方向起点 7..0，每个方向 1 帧。
        let frame = frame_of(7, 1, -2);
        assert_eq!(frame.layout_error(), None);
        for direction in 0..8usize {
            assert_eq!(
                frame.frame_indices(direction),
                Some((7 - direction as i32)..(8 - direction as i32)),
                "方向 {direction} 的帧区间"
            );
        }
        assert_eq!(frame.frame_indices(7), Some(0..1));
    }

    #[test]
    fn rejects_negative_skip_for_single_frame_when_a_direction_goes_negative() {
        // Start=6 Count=1 Skip=-2 → 方向 7 的起点是 -1，不可用。
        let frame = frame_of(6, 1, -2);
        assert_eq!(frame.layout_error(), Some("某方向的帧下标为负"));
        assert_eq!(frame.frame_indices(7), None);
        let error = FrameSet::parse_records(path(), &record(0, 6, 1, -2))
            .expect_err("方向 7 的帧下标为负必须拒绝");
        assert!(
            format!("{error}").contains("帧下标为负"),
            "错误信息应指出负下标：{error}"
        );
    }

    #[test]
    fn rejects_old_versions_and_v4() {
        let mut head = [0u8; 12];
        head[..4].copy_from_slice(&1i32.to_le_bytes());
        assert!(
            FrameSet::from_header(path(), 12, &head, |_, _| Ok(()))
                .expect("v1")
                .is_none()
        );
        head[..4].copy_from_slice(&4i32.to_le_bytes());
        assert!(FrameSet::from_header(path(), 12, &head, |_, _| Ok(())).is_err());
    }

    #[test]
    fn v3_without_frame_seek_is_none() {
        let mut head = [0u8; 12];
        head[..4].copy_from_slice(&3i32.to_le_bytes());
        head[4..8].copy_from_slice(&2i32.to_le_bytes());
        head[8..12].copy_from_slice(&(8u32 + 2 * 4).to_le_bytes());
        assert!(
            FrameSet::from_header(path(), 100, &head, |_, _| Ok(()))
                .expect("无 seek")
                .is_none()
        );
    }

    #[test]
    fn empty_frame_table_is_some_empty() {
        let mut head = [0u8; 12];
        head[..4].copy_from_slice(&3i32.to_le_bytes());
        head[4..8].copy_from_slice(&1i32.to_le_bytes());
        head[8..12].copy_from_slice(&20u32.to_le_bytes());
        let set = FrameSet::from_header(path(), 24, &head, |offset, buf| {
            if offset == 20 {
                buf.copy_from_slice(&0i32.to_le_bytes());
            }
            Ok(())
        })
        .expect("空表合法")
        .expect("有帧表");
        assert!(set.frames.is_empty());
    }

    #[test]
    fn malformed_header_and_truncated_table_error() {
        let mut head = [0u8; 12];
        head[..4].copy_from_slice(&3i32.to_le_bytes());
        head[4..8].copy_from_slice(&1i32.to_le_bytes());
        head[8..12].copy_from_slice(&100u32.to_le_bytes());
        assert!(
            FrameSet::from_header(path(), 104, &head, |offset, buf| {
                if offset == 100 {
                    buf.copy_from_slice(&1i32.to_le_bytes());
                }
                Ok(())
            })
            .is_err()
        );
    }
}
