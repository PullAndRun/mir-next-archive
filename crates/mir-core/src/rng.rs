//! 可复现的伪随机数发生器。
//!
//! 战斗、掉落与刷怪都需要「可回放」的随机性：同一 seed 必须得到同一序列，
//! 因此这里实现一个确定性的 xorshift128+，不依赖平台 `rand` 实现。

use std::time::{SystemTime, UNIX_EPOCH};

/// xorshift128+ 发生器。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirRng {
    state0: u64,
    state1: u64,
}

impl MirRng {
    /// 用给定种子构造。
    #[must_use]
    pub const fn from_seed(seed: u64) -> Self {
        // 用 splitmix64 展开种子，避免全零状态。
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        let state0 = z ^ (z >> 31);
        let mut z2 = state0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z2 = (z2 ^ (z2 >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z2 = (z2 ^ (z2 >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        let state1 = z2 ^ (z2 >> 31);
        Self {
            state0: if state0 == 0 { 1 } else { state0 },
            state1: if state1 == 0 { 2 } else { state1 },
        }
    }

    /// 用系统时间构造种子。
    #[must_use]
    pub fn from_entropy() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos() as u64);
        Self::from_seed(nanos ^ u64::from(std::process::id()))
    }

    /// 下一个 64 位随机数。
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state0;
        let y = self.state1;
        self.state0 = y;
        x ^= x << 23;
        self.state1 = x ^ y ^ (x >> 17) ^ (y >> 26);
        self.state1.wrapping_add(y)
    }

    /// 下一个 32 位随机数。
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// `[0, bound)` 区间内的随机整数；`bound == 0` 时返回 0。
    pub fn next_bounded(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            return 0;
        }
        // 拒绝采样消除取模偏差。
        let zone = u64::from(u32::MAX) - (u64::from(u32::MAX) % u64::from(bound));
        loop {
            let value = u64::from(self.next_u32());
            if value < zone {
                return (value % u64::from(bound)) as u32;
            }
        }
    }

    /// 闭区间 `[min, max]` 的随机整数。
    pub fn next_range(&mut self, min: i32, max: i32) -> i32 {
        if max <= min {
            return min;
        }
        let span = (i64::from(max) - i64::from(min) + 1) as u32;
        min + self.next_bounded(span) as i32
    }

    /// 以 `percent`（0..=100）的概率返回 `true`。
    pub fn chance(&mut self, percent: u32) -> bool {
        if percent == 0 {
            return false;
        }
        if percent >= 100 {
            return true;
        }
        self.next_bounded(100) < percent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = MirRng::from_seed(42);
        let mut b = MirRng::from_seed(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_differ() {
        let mut a = MirRng::from_seed(1);
        let mut b = MirRng::from_seed(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn bounded_stays_in_range() {
        let mut rng = MirRng::from_seed(7);
        for _ in 0..1000 {
            let value = rng.next_bounded(6);
            assert!(value < 6);
            let ranged = rng.next_range(-3, 3);
            assert!((-3..=3).contains(&ranged));
        }
    }

    #[test]
    fn chance_edges() {
        let mut rng = MirRng::from_seed(9);
        assert!(!rng.chance(0));
        assert!(rng.chance(100));
        assert!(MirRng::from_seed(0).next_u64() < u64::MAX);
    }

    #[test]
    fn zero_range_is_degenerate() {
        let mut rng = MirRng::from_seed(3);
        assert_eq!(rng.next_bounded(0), 0);
        assert_eq!(rng.next_range(5, 5), 5);
        assert_eq!(rng.next_range(5, 1), 5);
    }
}
