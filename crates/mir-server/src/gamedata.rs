//! 游戏数据（`Server.MirDB`）到运行时类型的桥接。
//!
//! `mir-db` 负责把 Crystal 的数据库文件解析成结构体；这一层负责把「数据库里的怪物模板」
//! 变成 `mir-server` 战斗系统真正使用的 [`MonsterKind`]，并把地图上的 `RespawnInfo`
//! 变成可执行的 [`SpawnGroup`]。
//!
//! 数值来源与 Crystal 的对应关系：
//!
//! | Rust | Crystal |
//! | --- | --- |
//! | [`MonsterKind::name`] | `MonsterInfo.Name` |
//! | [`MonsterKind::image`] | `MonsterInfo.Image`（`Monster` 枚举，未声明值退回默认） |
//! | [`MonsterKind::stats`] | `MonsterInfo.Stats` 字典（`Stat::HP`/`MinDC`/…） |
//! | [`MonsterKind::attack_speed_ms`] | `MonsterInfo.AttackSpeed`（毫秒，0 用默认 2500） |
//! | [`MonsterKind::move_speed_ms`] | `MonsterInfo.MoveSpeed`（毫秒，0 用默认 1800） |
//! | [`SpawnGroup`] | `MapInfo.Respawns`（`RespawnInfo` 的 count/spread/location） |

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mir_core::Point;
use mir_core::enums::Monster as MonsterImage;
use mir_core::enums::Stat;
use mir_db::{MapInfo, MirDb, MirDbError, MonsterInfo};

use crate::combat::CombatStats;
use crate::monster::{DEFAULT_ATTACK_SPEED_MS, DEFAULT_MOVE_SPEED_MS, MonsterKind};
use crate::world::SpawnGroup;

/// 索引里能找到的 `Server.MirDB` 候选路径（按优先级）。
///
/// 用 `python tools/asset-index/find_asset.py --name Server.MirDB` 可以列出全部；
/// 这里只挑版本较新、内容完整的三个（v105 / v88 / v83），任何一个存在就能跑。
pub const DB_CANDIDATES: &[&str] = &[
    "resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB",
    "resource/community/suprcode-crystal-database/Jev/Server.MirDB",
    "resource/community/clgforever910-mir2database/Daneo1989/Server.MirDB",
];

/// 已加载的游戏数据。
#[derive(Debug)]
pub struct GameData {
    /// 解析后的数据库。
    pub db: MirDb,
    /// 怪物模板：`MonsterIndex` → 运行期种类。
    monsters: HashMap<i32, MonsterKind>,
    /// 数据文件路径（日志与排错用）。
    path: PathBuf,
}

impl GameData {
    /// 从文件加载。
    pub fn load(path: impl AsRef<Path>) -> Result<Self, MirDbError> {
        let path = path.as_ref().to_path_buf();
        let db = MirDb::load(&path)?;
        let monsters = db
            .monsters
            .iter()
            .map(|info| (info.index, kind_from_monster_info(info)))
            .collect();
        Ok(Self { db, monsters, path })
    }

    /// 按候选路径依次尝试加载，全部失败时返回错误（调用方决定降级）。
    ///
    /// 候选路径先按**仓库根**解析（`CARGO_MANIFEST_DIR/../..`），再退回相对当前工作目录，
    /// 这样 `cargo test`（工作目录是 crate 目录）和从仓库根启动服务器都能找到样本。
    pub fn discover() -> Result<Self, MirDbError> {
        let mut last = None;
        for candidate in candidate_paths() {
            if !candidate.exists() {
                continue;
            }
            match Self::load(&candidate) {
                Ok(data) => return Ok(data),
                Err(error) => {
                    tracing::warn!(path = %candidate.display(), %error, "MirDB 解析失败，尝试下一个候选");
                    last = Some(error);
                }
            }
        }
        Err(last.unwrap_or_else(|| MirDbError::Io {
            path: PathBuf::from("resource/community/<repo>/Server.MirDB"),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "没有找到任何 Server.MirDB（用 tools/asset-index/find_asset.py --name Server.MirDB 查询）",
            ),
        }))
    }

    /// 数据文件路径。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 按 `MonsterIndex` 取怪物种类。
    #[must_use]
    pub fn monster(&self, index: i32) -> Option<&MonsterKind> {
        self.monsters.get(&index)
    }

    /// 已加载的怪物模板数量。
    #[must_use]
    pub fn monster_count(&self) -> usize {
        self.monsters.len()
    }

    /// 某张地图的刷怪计划。
    ///
    /// `map` 是地图文件名（Crystal 的 `MapInfo.FileName`，例如 `"0"`）。
    /// 返回只包含模板确实存在的刷怪点；模板缺失时会记一条 `warn`（数据常见于
    /// 裁剪过的数据库，例如 `Jev Without Archers`）。
    #[must_use]
    pub fn spawn_groups(&self, map: &str) -> Vec<SpawnGroup> {
        let Some(info) = self.map_info(map) else {
            return Vec::new();
        };
        let mut groups = Vec::with_capacity(info.respawns.len());
        for respawn in &info.respawns {
            let Some(kind) = self.monster(respawn.monster_index) else {
                tracing::warn!(
                    map = map,
                    monster_index = respawn.monster_index,
                    "刷怪点引用了不存在的怪物模板，已跳过"
                );
                continue;
            };
            if respawn.count == 0 {
                continue;
            }
            groups.push(SpawnGroup {
                map: info.file_name.clone(),
                kind: kind.clone(),
                center: respawn.location,
                count: usize::from(respawn.count),
                spread: i32::from(respawn.spread),
            });
        }
        groups
    }

    /// 按地图文件名找 `MapInfo`。
    #[must_use]
    pub fn map_info(&self, map: &str) -> Option<&MapInfo> {
        self.db.maps.iter().find(|info| info.file_name == map)
    }

    /// 刷怪点总数（调试用）。
    #[must_use]
    pub fn respawn_count(&self, map: &str) -> usize {
        self.map_info(map).map_or(0, |info| info.respawns.len())
    }
}

/// 把数据库里的怪物模板转成运行期种类。
///
/// 属性全部来自 `Stats` 字典；缺失项按 0 处理（Crystal 的 `Stats` 也是「不写就是 0」）。
#[must_use]
pub fn kind_from_monster_info(info: &MonsterInfo) -> MonsterKind {
    MonsterKind {
        name: info.name.clone(),
        image: MonsterImage::from_value(info.image).unwrap_or_default(),
        ai: info.ai,
        level: info.level,
        view_range: i32::from(info.view_range),
        cool_eye: info.cool_eye,
        // 0 在 Crystal 里等于「每次轮到就动/打」，那是明显的坏数据；用默认值兜底。
        attack_speed_ms: if info.attack_speed == 0 {
            DEFAULT_ATTACK_SPEED_MS
        } else {
            i64::from(info.attack_speed)
        },
        move_speed_ms: if info.move_speed == 0 {
            DEFAULT_MOVE_SPEED_MS
        } else {
            i64::from(info.move_speed)
        },
        experience: info.experience,
        can_push: info.can_push,
        stats: stats_from_monster_info(info),
    }
}

/// 把 `Stats` 字典映射成 [`CombatStats`]。
#[must_use]
pub fn stats_from_monster_info(info: &MonsterInfo) -> CombatStats {
    let stats = &info.stats;
    let get = |stat: Stat| stats.get(stat);
    CombatStats {
        hp: get(Stat::HP),
        mp: get(Stat::MP),
        min_dc: get(Stat::MinDC),
        max_dc: get(Stat::MaxDC),
        min_mc: get(Stat::MinMC),
        max_mc: get(Stat::MaxMC),
        min_sc: get(Stat::MinSC),
        max_sc: get(Stat::MaxSC),
        min_ac: get(Stat::MinAC),
        max_ac: get(Stat::MaxAC),
        min_mac: get(Stat::MinMAC),
        max_mac: get(Stat::MaxMAC),
        accuracy: get(Stat::Accuracy),
        agility: get(Stat::Agility),
        luck: get(Stat::Luck),
        attack_speed: get(Stat::AttackSpeed),
        critical_rate: get(Stat::CriticalRate),
        critical_damage: get(Stat::CriticalDamage),
        magic_resist: get(Stat::MagicResist),
        reflect: get(Stat::Reflect),
    }
}

/// 把候选路径展开成真实路径：先按仓库根，再按当前工作目录。
///
/// `CARGO_MANIFEST_DIR/../..` 里带 `..`，直接进日志很难看，这里做一次词法归一化
/// （不解析符号链接，也不要求文件存在）。
#[must_use]
pub fn candidate_paths() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let mut paths = Vec::with_capacity(DB_CANDIDATES.len() * 2);
    for candidate in DB_CANDIDATES {
        paths.push(normalize(&root.join(candidate)));
    }
    for candidate in DB_CANDIDATES {
        paths.push(PathBuf::from(candidate));
    }
    paths
}

/// 纯词法归一化：消掉 `.` 与 `..`（不访问文件系统）。
#[must_use]
pub fn normalize(path: &Path) -> PathBuf {
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if parts.len() > 1 {
                    parts.pop();
                }
            }
            other => parts.push(other.as_os_str().to_owned()),
        }
    }
    parts.iter().collect()
}

/// 刷怪点中心（测试与日志用）。
#[must_use]
pub fn respawn_centers(data: &GameData, map: &str) -> Vec<Point> {
    data.map_info(map)
        .map(|info| info.respawns.iter().map(|r| r.location).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Option<GameData> {
        GameData::discover().ok()
    }

    #[test]
    fn discovers_a_database_from_the_index() {
        let Some(data) = sample() else {
            eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
            return;
        };
        assert!(data.monster_count() > 100, "怪物模板数量应上百");
        assert!(!data.db.maps.is_empty());
        assert!(
            data.map_info("0").is_some(),
            "样本里一定有比奇省（地图文件名 0）"
        );
    }

    #[test]
    fn monster_info_maps_to_runtime_stats() {
        let Some(data) = sample() else {
            eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
            return;
        };
        // 找一个有血量的模板，验证 HP / AC / DC 真的从 Stats 字典里取出来了。
        let with_hp = data
            .db
            .monsters
            .iter()
            .find(|info| info.stats.get(Stat::HP) > 0)
            .expect("样本里必然有带血量的怪物");
        let kind = kind_from_monster_info(with_hp);
        assert_eq!(kind.name, with_hp.name);
        assert_eq!(kind.stats.hp, with_hp.stats.get(Stat::HP));
        assert!(kind.stats.hp > 0);
        assert_eq!(kind.level, with_hp.level);
        assert!(
            kind.attack_speed_ms >= DEFAULT_ATTACK_SPEED_MS.min(i64::from(with_hp.attack_speed))
        );
    }

    #[test]
    fn world_filled_from_database_spawns_monsters() {
        let Some(data) = sample() else {
            eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
            return;
        };
        let mut world = crate::world::World::default();
        for group in data.spawn_groups("0") {
            world.spawn_group(&group);
        }
        assert!(
            world.monsters().len() > 10,
            "比奇省应该刷出不少怪，实际 {}",
            world.monsters().len()
        );
        // 怪物名字应该来自数据库（不是演示怪的占位名）。
        let names: std::collections::HashSet<&str> = world
            .monsters()
            .iter()
            .map(|m| m.kind.name.as_str())
            .collect();
        assert!(names.len() > 1, "至少有两种怪");
        assert!(
            names.iter().all(|name| !name.is_empty()),
            "怪物名字来自 MonsterInfo.Name"
        );
    }

    #[test]
    fn spawn_groups_cover_all_respawns_with_known_templates() {
        let Some(data) = sample() else {
            eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
            return;
        };
        let map = "0";
        let groups = data.spawn_groups(map);
        assert!(!groups.is_empty(), "比奇省应该有刷怪点");
        for group in &groups {
            assert!(group.count > 0);
            assert_eq!(group.map, map);
            assert!(!group.kind.name.is_empty());
        }
        let total: usize = groups.iter().map(|g| g.count).sum();
        assert!(total > 10, "比奇省的刷怪总量应该不少，实际 {total}");
    }
}
