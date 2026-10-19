//! 等级经验表（Crystal `Settings.ExperienceList`）。
//!
//! Crystal 把「升到下一级需要多少经验」放在 `Configs/ExpList.ini` 里：
//!
//! ```ini
//! [Exp]
//! Level1=100
//! Level2=200
//! Level3=300
//! ...
//! ```
//!
//! 读取逻辑逐条对应 `Server/Settings.cs:839-848`：
//!
//! ```csharp
//! long exp = 100;                                                        // Settings.cs:841
//! InIReader reader = new InIReader(Path.Combine(ConfigPath, "ExpList.ini")); // :842
//! for (int i = 1; i <= 500; i++)
//! {
//!     exp = reader.ReadInt64("Exp", "Level" + i, exp);  // 缺失项沿用上一项（:846）
//!     ExperienceList.Add(exp);                          // :847
//! }
//! ```
//!
//! 而「当前等级的需求」是这样取的（`Server/MirObjects/PlayerObject.cs:9627-9630`）：
//!
//! ```csharp
//! MaxExperience = Level < Settings.ExperienceList.Count ? Settings.ExperienceList[Level - 1] : 0;
//! ```
//!
//! 也就是说 **`entries[level - 1]` 是「从 level 升到 level+1」的经验**，
//! 升级时 `Experience` 会把这份需求扣掉（`PlayerObject.cs:911-921`），
//! 所以 Crystal 的 `Experience` 是「当前等级内已积累的经验」，不是总经验。
//!
//! # 为什么不在代码里写死整张表
//!
//! 经验曲线是运营参数（私服各改各的），Crystal 也从 `.ini` 读。所以这里实现
//! [`ExpTable`]：优先读 `config/ExpList.ini`，其次读素材索引里的
//! `resource/community/*/Configs/ExpList.ini`；全部不可用时退回
//! `exp = level^3 * 100` 的近似曲线并 `warn!`（**只在加载时告警一次**，不刷屏）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Crystal `Settings.LoadEXP` 固定读 500 级（`Server/Settings.cs:844` 的 `i <= 500`）。
pub const MAX_LEVEL: u16 = 500;

/// 兜底曲线系数：`exp = level^3 * 100`。
///
/// 选 100 是为了让 1 级的需求（100）与 Crystal 表首项一致；高等级会明显偏陡，
/// 所以它只是「文件缺失时游戏还能玩」的近似，日志里会明确告警。
pub const FALLBACK_EXP_FACTOR: i64 = 100;

/// 经验表候选路径（按优先级）。
///
/// 前两条是本作自己的配置（`config/ExpList.ini`，注意 Crystal 的文件名是中缀大写的
/// `ExpList.ini`；Windows 上大小写不敏感，这里仍然把两种写法都列上，兼容大小写敏感的文件系统）；
/// 其余直接复用 `gamedata::DB_CANDIDATES` 里那几个社区数据库自带的 `Configs/ExpList.ini`。
pub const EXP_CANDIDATES: &[&str] = &[
    "config/ExpList.ini",
    "config/Explist.ini",
    "resource/community/suprcode-crystal-database/cjlaaa/Configs/ExpList.ini",
    "resource/community/suprcode-crystal-database/Jev/Configs/ExpList.ini",
    "resource/community/clgforever910-mir2database/Daneo1989/Configs/ExpList.ini",
    "resource/community/suprcode-crystal-database/cjlaaa/Configs/Explist.ini",
];

/// 表的来源（日志与测试用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpSource {
    /// 从文件加载。
    File(PathBuf),
    /// 从内存里的 ini 文本解析（测试用）。
    Inline,
    /// 兜底近似曲线。
    Fallback,
}

/// 等级经验表。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpTable {
    /// `entries[level - 1]` = 从 `level` 升到 `level + 1` 所需经验
    /// （Crystal `ExperienceList[Level - 1]`，`PlayerObject.cs:9629`）。
    entries: Vec<i64>,
    /// 来源。
    source: ExpSource,
}

impl Default for ExpTable {
    fn default() -> Self {
        Self::fallback()
    }
}

impl ExpTable {
    /// 兜底近似曲线 `exp = level^3 * 100`（1 级 = 100）。
    #[must_use]
    pub fn fallback() -> Self {
        let mut entries = Vec::with_capacity(usize::from(MAX_LEVEL));
        for level in 1..=MAX_LEVEL {
            let level = i64::from(level);
            entries.push(level * level * level * FALLBACK_EXP_FACTOR);
        }
        Self {
            entries,
            source: ExpSource::Fallback,
        }
    }

    /// 直接用一串「每级需求」构造。
    ///
    /// 不足 [`MAX_LEVEL`] 项时用最后一项补齐（Crystal 的 `ReadInt64(..., exp)` 默认值语义，
    /// `Settings.cs:846`）；空表退回 [`ExpTable::fallback`]。
    #[must_use]
    pub fn from_entries(entries: Vec<i64>) -> Self {
        if entries.is_empty() {
            return Self::fallback();
        }
        let mut entries = entries;
        let last = entries[entries.len() - 1];
        while entries.len() < usize::from(MAX_LEVEL) {
            entries.push(last);
        }
        entries.truncate(usize::from(MAX_LEVEL));
        Self {
            entries,
            source: ExpSource::Inline,
        }
    }

    /// 解析 `ExpList.ini` 的文本。
    ///
    /// 只认 `Level<数字>=<整数>`（大小写不敏感，允许任意 `[节]` 名），
    /// 解析失败或一条都没读到就返回 `None`（调用方决定是否退回近似曲线）。
    #[must_use]
    pub fn parse_ini(text: &str) -> Option<Vec<i64>> {
        let mut raw: HashMap<u16, i64> = HashMap::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty()
                || line.starts_with('[')
                || line.starts_with(';')
                || line.starts_with('#')
            {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let Some(digits) = key
                .strip_prefix("Level")
                .or_else(|| key.strip_prefix("level"))
            else {
                continue;
            };
            let (Ok(level), Ok(exp)) = (digits.trim().parse::<u16>(), value.trim().parse::<i64>())
            else {
                continue;
            };
            if level == 0 {
                continue;
            }
            raw.insert(level, exp);
        }
        if raw.is_empty() {
            return None;
        }

        // 与 `Settings.cs:841-848` 一致：缺项沿用上一项，起点默认 100。
        let mut entries = Vec::with_capacity(usize::from(MAX_LEVEL));
        let mut exp = 100_i64;
        for level in 1..=MAX_LEVEL {
            if let Some(value) = raw.get(&level) {
                exp = *value;
            }
            entries.push(exp);
        }
        Some(entries)
    }

    /// 从 ini 文本构造（来源记为 [`ExpSource::Inline`]）。
    #[must_use]
    pub fn from_ini(text: &str) -> Option<Self> {
        Self::parse_ini(text).map(Self::from_entries)
    }

    /// 从文件加载；读不到或解析失败返回 `None`。
    #[must_use]
    pub fn load(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).ok()?;
        let entries = Self::parse_ini(&text)?;
        let mut table = Self::from_entries(entries);
        table.source = ExpSource::File(path.to_path_buf());
        Some(table)
    }

    /// 按候选路径依次尝试；全部失败时退回近似曲线并 `warn!`。
    #[must_use]
    pub fn discover() -> Self {
        for candidate in candidate_paths() {
            if !candidate.is_file() {
                continue;
            }
            match Self::load(&candidate) {
                Some(table) => {
                    tracing::info!(
                        path = %candidate.display(),
                        levels = table.len(),
                        "已加载等级经验表"
                    );
                    return table;
                }
                None => {
                    tracing::warn!(
                        path = %candidate.display(),
                        "等级经验表解析失败（没有读到任何 LevelNN=…），尝试下一个候选"
                    );
                }
            }
        }
        tracing::warn!(
            factor = FALLBACK_EXP_FACTOR,
            "没有可用的 ExpList.ini，退回近似曲线 exp = level^3 * {FALLBACK_EXP_FACTOR}"
        );
        Self::fallback()
    }

    /// 来源。
    #[must_use]
    pub fn source(&self) -> &ExpSource {
        &self.source
    }

    /// 表里有几级。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 表是否为空（永远为 false：至少 [`MAX_LEVEL`] 项）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 「从 `level` 升到 `level + 1`」所需经验（Crystal `MaxExperience`）。
    ///
    /// * `level == 0`（新建角色，进世界前）按 1 级处理——Crystal 的
    ///   `ExperienceList[Level - 1]` 在 `Level == 0` 时会数组越界（`PlayerObject.cs:9629`），
    ///   这是它自己不会走到的分支（建角后首次进世界就补成 1 级）。
    /// * 超过表长时返回 0；Crystal 同样返回 0，但那样 `while (experience >= MaxExperience)`
    ///   会一路升到 65535 级（见 `docs/world/combat.md` 的「Crystal 异常行为」），
    ///   本作的升级循环遇到 0 会停下。
    #[must_use]
    pub fn exp_for_level(&self, level: u16) -> i64 {
        let index = usize::from(level.saturating_sub(1));
        self.entries.get(index).copied().unwrap_or(0)
    }

    /// 从 1 级升到 `level` 所需的**总**经验（各等级需求之和）。
    ///
    /// 纯累计口径，方便 GM 命令与测试；Crystal 自己不做这个换算
    /// （它的 `Experience` 是当前等级内的进度）。
    #[must_use]
    pub fn total_exp_for_level(&self, level: u16) -> i64 {
        let count = usize::from(level.saturating_sub(1)).min(self.entries.len());
        self.entries[..count]
            .iter()
            .fold(0_i64, |sum, exp| sum.saturating_add(*exp))
    }

    /// 按「总经验」反查等级（`total_exp_for_level` 的逆运算）。
    ///
    /// 恰好卡在阈值上时算升到该级（`remaining < entry` 才停），
    /// 等级上限 [`MAX_LEVEL`]。
    #[must_use]
    pub fn level_for_total_exp(&self, total: i64) -> u16 {
        let mut remaining = total.max(0);
        let mut level = 1_u16;
        for entry in &self.entries {
            if level >= MAX_LEVEL || remaining < *entry {
                break;
            }
            remaining -= *entry;
            level += 1;
        }
        level
    }
}

/// 候选路径：先按**仓库根**展开（`CARGO_MANIFEST_DIR/../..`），再按当前工作目录。
///
/// 与 `gamedata::candidate_paths` 同一套规则：`cargo test` 的工作目录是 crate 目录，
/// 而生产是从仓库根启动的，两种都要能找到。
#[must_use]
pub fn candidate_paths() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let mut paths = Vec::with_capacity(EXP_CANDIDATES.len() * 2);
    for candidate in EXP_CANDIDATES {
        paths.push(crate::gamedata::normalize(&root.join(candidate)));
    }
    for candidate in EXP_CANDIDATES {
        paths.push(PathBuf::from(candidate));
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_crystal_exp_list() {
        let text = "[Exp]\nLevel1=100\nLevel2=200\nLevel3=300\n";
        let table = ExpTable::from_ini(text).expect("解析");
        assert_eq!(table.exp_for_level(1), 100);
        assert_eq!(table.exp_for_level(2), 200);
        assert_eq!(table.exp_for_level(3), 300);
        // 缺项沿用上一项（`Settings.cs:846` 的默认值语义）。
        assert_eq!(table.exp_for_level(4), 300);
        assert_eq!(table.len(), usize::from(MAX_LEVEL));
    }

    #[test]
    fn rejects_garbage_instead_of_guessing() {
        assert!(ExpTable::parse_ini("").is_none());
        assert!(ExpTable::parse_ini("[Exp]\nhello=world\n").is_none());
        assert!(ExpTable::parse_ini("LevelX=1\n").is_none());
        // 大小写不敏感：Crystal 的键名是 `LevelN`，但配置是人手写的。
        assert!(ExpTable::parse_ini("level7=42\n").is_some());
    }

    #[test]
    fn fallback_curve_is_cubic_and_starts_at_crystal_first_entry() {
        let table = ExpTable::fallback();
        assert_eq!(table.source(), &ExpSource::Fallback);
        assert_eq!(table.exp_for_level(1), 100, "1 级与 Crystal 表首项一致");
        assert_eq!(table.exp_for_level(2), 800, "2^3 * 100");
        assert_eq!(table.exp_for_level(10), 100_000);
        assert_eq!(table.exp_for_level(0), 100, "0 级按 1 级处理");
    }

    #[test]
    fn total_exp_and_level_lookup_are_inverse() {
        let table = ExpTable::from_entries(vec![100, 200, 300]);
        assert_eq!(table.total_exp_for_level(1), 0);
        assert_eq!(table.total_exp_for_level(2), 100);
        assert_eq!(table.total_exp_for_level(3), 300);
        assert_eq!(table.total_exp_for_level(4), 600);

        assert_eq!(table.level_for_total_exp(0), 1);
        assert_eq!(table.level_for_total_exp(99), 1);
        assert_eq!(table.level_for_total_exp(100), 2, "卡在阈值上算升级");
        assert_eq!(table.level_for_total_exp(299), 2);
        assert_eq!(table.level_for_total_exp(300), 3);
        assert_eq!(table.level_for_total_exp(-5), 1, "负数按 0 处理");
        assert_eq!(table.level_for_total_exp(i64::MAX), MAX_LEVEL, "封顶");
    }

    #[test]
    fn discovers_the_index_sample_when_available() {
        let mut found = false;
        for candidate in candidate_paths() {
            if !candidate.is_file() {
                continue;
            }
            let Some(table) = ExpTable::load(&candidate) else {
                continue;
            };
            found = true;
            // Crystal 样本的首项是 100（见 `resource/.../Configs/ExpList.ini`）。
            assert_eq!(table.exp_for_level(1), 100);
            assert!(
                table.exp_for_level(10) > table.exp_for_level(9),
                "experience must be monotonic"
            );
            assert!(
                matches!(table.source(), ExpSource::File(_)),
                "从文件加载时记录来源路径"
            );
            break;
        }
        if !found {
            eprintln!("跳过：resource/ 里没有 ExpList.ini 样本");
        }
    }
}
