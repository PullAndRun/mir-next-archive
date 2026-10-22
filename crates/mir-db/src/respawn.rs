//! 刷怪计时器段与 GT 城尾段。
//!
//! * `RespawnTimer`：`Server/MirEnvir/RespawnTimer.cs:51`，读 `:72-85`，写 `:86-93`。
//!   整条记录**没有** `count` 前缀，只有里面的选项列表带计数。`> 67` 才有这一段（`Envir.cs:2935-2936`）。
//! * `GtMap`：`Server/MirDatabase/GTMap.cs:10`，读 `:27-38`，写 `:40-51`。
//!   **写进去但从不读**——`SaveDB` 末尾无条件写（`Envir.cs:2503-2505`），`LoadDB` 却到此为止。
//!   本 crate 的做法见 [`crate::MirDb::parse_with_leftover`]：能整齐解析成 GT 列表就收下，
//!   否则原样算进 leftover。

use crate::MirDbError;
use crate::reader::{DbReader, DbWriter};

/// `RespawnTimer` 头部的固定字节数：`u8` + `u64` + `i32 count`（`RespawnTimer.cs:88-90`）。
pub const MIN_RESPAWN_TIMER_LEN: usize = 13;

/// 单个刷怪计时选项的字节数：`i32 UserCount` + `f64 DelayLoss`（`RespawnTimer.cs:33-43`）。
pub const RESPAWN_TICK_OPTION_LEN: usize = 12;

/// `GtMap` 的最小字节数（四个空串，`GTMap.cs:29-37`）。
pub const MIN_GT_MAP_LEN: usize = 24;

/// Crystal `RespawnTickOption`（`Server/MirEnvir/RespawnTimer.cs:26`；读 `:33-37`，写 `:39-43`）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RespawnTickOption {
    /// 玩家数阈值（`i32`，`RespawnTimer.cs:28`）。
    pub user_count: i32,
    /// 该阈值下的刷新速度系数（`f64`，`:29`）。
    pub delay_loss: f64,
}

impl Default for RespawnTickOption {
    /// C# 初始值是 `UserCount = 1`、`DelayLoss = 1.0`（`RespawnTimer.cs:28-29`）。
    fn default() -> Self {
        Self {
            user_count: 1,
            delay_loss: 1.0,
        }
    }
}

impl RespawnTickOption {
    /// 解码，见 `RespawnTimer.cs:33-37`。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        Ok(Self {
            user_count: r.i32()?,
            delay_loss: r.f64()?,
        })
    }

    /// 编码，见 `RespawnTimer.cs:39-43`。
    pub fn encode(&self, w: &mut DbWriter) {
        w.i32(self.user_count);
        w.f64(self.delay_loss);
    }
}

/// Crystal `RespawnTimer`（`Server/MirEnvir/RespawnTimer.cs:51`；读 `:72-85`，写 `:86-93`）。
///
/// `LastTick` / `LastUsercount` / `CurrentDelay` 都是运行期派生值（`:60-62`），不落盘。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RespawnTimer {
    /// 无人时的刷新间隔（分钟，`u8`，`RespawnTimer.cs:58`）。
    pub base_spawn_rate: u8,
    /// 全局刷新计时（`u64`，`:59`）。
    pub current_tick_counter: u64,
    /// 不同玩家数下的刷新速度选项（`:63`）。
    pub options: Vec<RespawnTickOption>,
}

impl Default for RespawnTimer {
    /// C# 构造时预置一个 `(0, 1.0)` 选项（`RespawnTimer.cs:65-70`）。
    fn default() -> Self {
        Self {
            base_spawn_rate: 20,
            current_tick_counter: 0,
            options: vec![RespawnTickOption {
                user_count: 0,
                delay_loss: 1.0,
            }],
        }
    }
}

impl RespawnTimer {
    /// 解码，见 `RespawnTimer.cs:72-85`。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        let base_spawn_rate = r.u8()?;
        let current_tick_counter = r.u64()?;
        let options = r.list(RESPAWN_TICK_OPTION_LEN, RespawnTickOption::parse)?;
        Ok(Self {
            base_spawn_rate,
            current_tick_counter,
            options,
        })
    }

    /// 编码，见 `RespawnTimer.cs:86-93`。
    pub fn encode(&self, w: &mut DbWriter) {
        w.u8(self.base_spawn_rate);
        w.u64(self.current_tick_counter);
        w.list(&self.options, |w, option| option.encode(w));
    }
}

/// Crystal `GTMap`（`Server/MirDatabase/GTMap.cs:10`；读 `:27-38`，写 `:40-51`）。
///
/// 这是 `SaveDB` 写在文件最末尾、`LoadDB` 从不读取的一段（见模块文档）。
/// `Maps` 字段不落盘（`GTMap.cs:14`）。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GtMap {
    /// 主键（`GTMap.cs:29`）。
    pub index: i32,
    /// 租借凭据（`:30`）。
    pub key: i32,
    /// 地图名（`:31`）。
    pub name: String,
    /// 城主（`:32`）。
    pub owner: String,
    /// 会长（`:33`）。
    pub leader: String,
    /// 副会长（`:34`）。
    pub leader2: String,
    /// 价格（`:35`）。
    pub price: i32,
    /// 租期（天，`:36`）。
    pub days: i32,
    /// 开始时间（`:37`，`int` 时间戳）。
    pub begin: i32,
}

impl GtMap {
    /// 解码，见 `GTMap.cs:27-38`。
    pub fn parse(r: &mut DbReader<'_>) -> Result<Self, MirDbError> {
        Ok(Self {
            index: r.i32()?,
            key: r.i32()?,
            name: r.string()?,
            owner: r.string()?,
            leader: r.string()?,
            leader2: r.string()?,
            price: r.i32()?,
            days: r.i32()?,
            begin: r.i32()?,
        })
    }

    /// 编码，见 `GTMap.cs:40-51`。
    pub fn encode(&self, w: &mut DbWriter) {
        w.i32(self.index);
        w.i32(self.key);
        w.string(&self.name);
        w.string(&self.owner);
        w.string(&self.leader);
        w.string(&self.leader2);
        w.i32(self.price);
        w.i32(self.days);
        w.i32(self.begin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respawn_timer_matches_sample_layout() {
        // 样本里的 `RespawnTimer`：`{base_spawn_rate: 20, current_tick_counter: 71,
        // options: [(0, 1.0)]}`，共 25 字节（`docs/store/mirdb-format.md` §4.10）。
        let timer = RespawnTimer {
            current_tick_counter: 71,
            ..RespawnTimer::default()
        };
        let mut writer = DbWriter::new();
        timer.encode(&mut writer);
        let bytes = writer.into_inner();
        assert_eq!(bytes.len(), MIN_RESPAWN_TIMER_LEN + RESPAWN_TICK_OPTION_LEN);
        assert_eq!(bytes.len(), 25);
        // `20` + `71`(u64) + `1`(i32) + `0`(i32) + `1.0`(f64)
        assert_eq!(bytes[0], 20);
        assert_eq!(&bytes[1..9], &71u64.to_le_bytes());
        assert_eq!(&bytes[9..13], &1i32.to_le_bytes());
        assert_eq!(&bytes[13..17], &0i32.to_le_bytes());
        assert_eq!(&bytes[17..25], &1.0f64.to_le_bytes());

        let mut reader = DbReader::new(&bytes, 105);
        let decoded = RespawnTimer::parse(&mut reader).expect("解码应当成功");
        assert_eq!(reader.remaining(), 0);
        assert_eq!(decoded, timer);
    }

    #[test]
    fn gt_map_roundtrip() {
        let value = GtMap {
            index: 1,
            key: 2,
            name: "GT地图".to_owned(),
            owner: "城主".to_owned(),
            leader: "会长".to_owned(),
            leader2: String::new(),
            price: 100_000,
            days: 7,
            begin: 1_700_000_000,
        };
        let mut writer = DbWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = DbReader::new(&bytes, 117);
        assert_eq!(GtMap::parse(&mut reader).expect("解码应当成功"), value);
        assert_eq!(reader.remaining(), 0);
    }
}
