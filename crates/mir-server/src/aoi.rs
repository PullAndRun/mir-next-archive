//! AOI（Area of Interest）网格索引：把「每次广播/索敌都遍历全部对象」换成按格子查询。
//!
//! # Crystal 对照
//!
//! 本仓库的 Crystal 分支没有名为 `CellGrid` 的类；它的空间索引就是
//! `Map.Cells[x, y]` 上每个格子一份 `Cell.Objects` 列表
//! （`Server/MirEnvir/Map.cs:2475-2518`）：
//!
//! ```csharp
//! public class Cell {
//!     public List<MapObject> Objects = new List<MapObject>();   // Map.cs:2485
//!     public void Add(MapObject mapObject) { Objects.Add(mapObject); }        // :2489-2504
//!     public void Remove(MapObject mapObject) { Objects.Remove(mapObject); }  // :2505-2518
//! }
//! ```
//!
//! 对象生成/移动/消失时同步维护这份索引：`MapObject.Spawned` 把对象挂进
//! `Envir.Objects`（`MapObject.cs:339-357`），`HumanObject.Walk` 在落格前后
//! 先 `GetCell(CurrentLocation).Remove(this)` 再 `GetCell(CurrentLocation).Add(this)`
//! （`HumanObject.cs:2465-2476`、跑动见 `:2594-2602`）。
//!
//! 可见性查询是**矩形逐格扫描**：`PlayerObject.GetObjects` 扫
//! `(2 * Globals.DataRange + 1)² = 33 × 33` 个格子（`PlayerObject.cs:1741-1768`），
//! 判定用切比雪夫距离 `Functions.InRange`（`Shared/Functions/Functions.cs:71-74`）。
//! 移动时只处理进出视野的条带（`HumanObject.cs:7864-8440` 的 `RemoveObjects/AddObjects`），
//! 等价于「新旧视窗矩形的差集」。
//!
//! # 本实现
//!
//! Crystal 的格子是 1×1 地块；直接照搬会让每次广播访问 33×33 = 1089 个格子。
//! 本作把 [`AOI_CELL_SIZE`] × [`AOI_CELL_SIZE`] 个地块合并成一个**桶**：
//!
//! * `insert` / `move_to` / `remove` 是 O(桶内对象数)，移动只在跨桶时搬一次；
//! * `query_radius`（切比雪夫视野）先把正方形扩到桶边界，再对候选做**精确距离过滤**，
//!   所以结果与 Crystal 的 33×33 逐格扫描完全一致，单次最多访问 6×6 = 36 个桶；
//! * `query_rect` / `query_cell` 供占格判定、拾取、对话范围等使用。
//!
//! 查询结果按 `(y, x, 对象 ID)` 升序返回，等价于 Crystal
//! `for (y) for (x) for (cell.Objects)` 的扫描顺序，保证发包顺序可回放。
//!
//! # 键为什么是 `(类别, ID)`
//!
//! Crystal 的玩家/怪物/NPC/物品共用一个 `ObjectID` 空间；本作的测试会直接构造
//! 对象 ID（不经过 `World::allocate_object_id`），不同类别的 ID 可能撞车
//! （例如测试里手写 `object_id = 1` 的玩家与数据库刷出的 1 号 NPC）。
//! 既有可见性集合本来就是按类别分开的（`visible_monsters` / `visible_items` / …），
//! 所以索引键取 `(AoiKind, u32)`，与既有语义一致，也不会让一类别覆盖另一类别。
//!
//! 负坐标用 `div_euclid` 分桶，不会出现「-1 和 0 落到同一个桶」的错位。

use std::collections::HashMap;

use mir_core::Point;

/// 一个桶覆盖的地块边长。
///
/// Crystal 的 `Cell` 是 1×1（`Map.cs:2475-2518`）；本作取 8×8，把 33×33 格的
/// 视野查询从 1089 次格子访问降到最多 36 次桶访问（33 / 8 ≈ 5，跨桶时最多 6 列）。
/// 查询仍用切比雪夫距离精确过滤，**结果不变**。
pub const AOI_CELL_SIZE: i32 = 8;

/// AOI 里的对象类别（决定查询方拿到的是玩家、怪物、NPC 还是地面物品）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AoiKind {
    /// 玩家（`Player`）。
    Player,
    /// 怪物（`Monster`）。
    Monster,
    /// NPC（`Npc`）。
    Npc,
    /// 地面物品/金币堆（`GroundItem`）。
    Item,
}

/// 索引键：`(类别, 对象 ID)`（理由见模块文档）。
pub type AoiKey = (AoiKind, u32);

/// 索引里的一条对象记录。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AoiEntry {
    /// 对象 ID（与 Crystal `MapObject.ObjectID` 一样，玩家/怪物/NPC/物品共用一个 ID 空间）。
    pub object_id: u32,
    /// 对象类别。
    pub kind: AoiKind,
    /// 当前坐标（索引维护时的值；查询结果里的坐标就是它）。
    pub position: Point,
}

impl AoiEntry {
    /// 本记录在索引里的键。
    #[must_use]
    pub const fn key(&self) -> AoiKey {
        (self.kind, self.object_id)
    }
}

/// 一张地图的 AOI 网格。
///
/// 一个 `World` 按地图名各持一份（与 Crystal 每张 `Map` 各有一份 `Cells` 对应）。
#[derive(Debug, Clone)]
pub struct AoiGrid {
    /// 桶边长（地块数）。
    cell_size: i32,
    /// 桶 → 对象键列表（桶内顺序是插入顺序，查询时统一排序）。
    buckets: HashMap<(i32, i32), Vec<AoiKey>>,
    /// 对象键 → 记录（移动/删除时 O(1) 找到旧位置）。
    entries: HashMap<AoiKey, AoiEntry>,
}

impl Default for AoiGrid {
    fn default() -> Self {
        Self::new(AOI_CELL_SIZE)
    }
}

impl AoiGrid {
    /// 用指定桶边长构造（`cell_size <= 0` 时按 1 处理，避免除零）。
    #[must_use]
    pub fn new(cell_size: i32) -> Self {
        Self {
            cell_size: cell_size.max(1),
            buckets: HashMap::new(),
            entries: HashMap::new(),
        }
    }

    /// 索引里的对象数量。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 某个对象是否在索引里。
    #[must_use]
    pub fn contains(&self, kind: AoiKind, object_id: u32) -> bool {
        self.entries.contains_key(&(kind, object_id))
    }

    /// 某个对象在索引里的位置。
    #[must_use]
    pub fn position_of(&self, kind: AoiKind, object_id: u32) -> Option<Point> {
        self.entries
            .get(&(kind, object_id))
            .map(|entry| entry.position)
    }

    /// 某个对象的类别（存在时就是传入的类别）。
    #[must_use]
    pub fn kind_of(&self, kind: AoiKind, object_id: u32) -> Option<AoiKind> {
        self.entries.get(&(kind, object_id)).map(|entry| entry.kind)
    }

    /// 桶坐标（负坐标按欧几里得除法，-1 与 0 不会混桶）。
    fn bucket_key(&self, position: Point) -> (i32, i32) {
        (
            position.x.div_euclid(self.cell_size),
            position.y.div_euclid(self.cell_size),
        )
    }

    /// 把对象挂进桶里。
    fn attach(&mut self, entry: AoiEntry) {
        self.buckets
            .entry(self.bucket_key(entry.position))
            .or_default()
            .push(entry.key());
    }

    /// 从桶里摘掉对象（同一键只会出现一次）。
    fn detach(&mut self, entry: AoiEntry) {
        let key = self.bucket_key(entry.position);
        let Some(bucket) = self.buckets.get_mut(&key) else {
            return;
        };
        if let Some(index) = bucket.iter().position(|item| *item == entry.key()) {
            bucket.swap_remove(index);
        }
        if bucket.is_empty() {
            self.buckets.remove(&key);
        }
    }

    /// 插入/替换一个对象，返回被替换掉的旧记录。
    pub fn insert(&mut self, entry: AoiEntry) -> Option<AoiEntry> {
        let old = self.entries.insert(entry.key(), entry);
        if let Some(old) = old {
            self.detach(old);
        }
        self.attach(entry);
        old
    }

    /// 删除一个对象，返回它原来的记录。
    pub fn remove(&mut self, kind: AoiKind, object_id: u32) -> Option<AoiEntry> {
        let entry = self.entries.remove(&(kind, object_id))?;
        self.detach(entry);
        Some(entry)
    }

    /// 把对象移动到新坐标；对象不在索引里时返回 `false`。
    ///
    /// 同一桶内的移动只改记录，不搬桶。
    pub fn move_to(&mut self, kind: AoiKind, object_id: u32, position: Point) -> bool {
        let key = (kind, object_id);
        let Some(entry) = self.entries.get(&key).copied() else {
            return false;
        };
        if entry.position == position {
            return true;
        }
        let new_entry = AoiEntry { position, ..entry };
        if self.bucket_key(entry.position) == self.bucket_key(position) {
            self.entries.insert(key, new_entry);
            return true;
        }
        self.detach(entry);
        self.entries.insert(key, new_entry);
        self.attach(new_entry);
        true
    }

    /// 查询闭区间矩形内**某一类**对象（按 `(y, x, 对象 ID)` 排序）。
    ///
    /// 广播/AOI 同步只需要玩家或只需要怪物时用它：直接跳过无关类别，
    /// 避免把整张视野里的怪物/地面物品都先收集再过滤。
    #[must_use]
    pub fn query_rect_kind(&self, min: Point, max: Point, kind: AoiKind) -> Vec<AoiEntry> {
        let left = min.x.min(max.x);
        let right = min.x.max(max.x);
        let top = min.y.min(max.y);
        let bottom = min.y.max(max.y);
        let mut found = Vec::new();
        if self.entries.is_empty() {
            return found;
        }
        let start = self.bucket_key(Point::new(left, top));
        let end = self.bucket_key(Point::new(right, bottom));
        for bucket_x in start.0..=end.0 {
            for bucket_y in start.1..=end.1 {
                let Some(bucket) = self.buckets.get(&(bucket_x, bucket_y)) else {
                    continue;
                };
                for key in bucket {
                    if key.0 != kind {
                        continue;
                    }
                    let Some(entry) = self.entries.get(key) else {
                        continue;
                    };
                    if entry.position.x < left
                        || entry.position.x > right
                        || entry.position.y < top
                        || entry.position.y > bottom
                    {
                        continue;
                    }
                    found.push(*entry);
                }
            }
        }
        found.sort_by_key(|entry| (entry.position.y, entry.position.x, entry.object_id));
        found
    }

    /// 查询切比雪夫半径内某一类对象（广播按玩家类别查询时用）。
    #[must_use]
    pub fn query_radius_kind(&self, center: Point, radius: i32, kind: AoiKind) -> Vec<AoiEntry> {
        let radius = radius.max(0);
        self.query_rect_kind(
            Point::new(center.x - radius, center.y - radius),
            Point::new(center.x + radius, center.y + radius),
            kind,
        )
    }

    /// 查询闭区间矩形 `[min, max]` 内的对象（按 `(y, x, 对象 ID)` 排序）。
    ///
    /// 参数顺序反了也能工作：内部会取 `min/max` 的分量最小值/最大值。
    #[must_use]
    pub fn query_rect(&self, min: Point, max: Point) -> Vec<AoiEntry> {
        let left = min.x.min(max.x);
        let right = min.x.max(max.x);
        let top = min.y.min(max.y);
        let bottom = min.y.max(max.y);
        let mut found = Vec::new();
        if self.entries.is_empty() {
            return found;
        }
        let start = self.bucket_key(Point::new(left, top));
        let end = self.bucket_key(Point::new(right, bottom));
        for bucket_x in start.0..=end.0 {
            for bucket_y in start.1..=end.1 {
                let Some(bucket) = self.buckets.get(&(bucket_x, bucket_y)) else {
                    continue;
                };
                for key in bucket {
                    let Some(entry) = self.entries.get(key) else {
                        continue;
                    };
                    if entry.position.x < left
                        || entry.position.x > right
                        || entry.position.y < top
                        || entry.position.y > bottom
                    {
                        continue;
                    }
                    found.push(*entry);
                }
            }
        }
        found.sort_by_key(|entry| (entry.position.y, entry.position.x, entry.object_id));
        found
    }

    /// 查询切比雪夫半径 `radius` 内的对象（`Functions.InRange`，`Functions.cs:71-74`）。
    ///
    /// 与 Crystal 的 `for (y in ±DataRange) for (x in ±DataRange)` 结果完全一致。
    /// `radius < 0` 视为 0。
    #[must_use]
    pub fn query_radius(&self, center: Point, radius: i32) -> Vec<AoiEntry> {
        let radius = radius.max(0);
        self.query_rect(
            Point::new(center.x - radius, center.y - radius),
            Point::new(center.x + radius, center.y + radius),
        )
    }

    /// 查询某一格上的对象（占格/拾取/对话范围用）。
    #[must_use]
    pub fn query_cell(&self, cell: Point) -> Vec<AoiEntry> {
        self.query_rect(cell, cell)
    }

    /// 清空整张图（世界销毁/地图缓存淘汰时用）。
    pub fn clear(&mut self) {
        self.buckets.clear();
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 确定性伪随机（避免给库引入 rand 依赖）。
    struct Lcg(u64);

    impl Lcg {
        fn new(seed: u64) -> Self {
            Self(seed)
        }

        fn next(&mut self) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            (self.0 >> 33) as u32
        }

        fn range(&mut self, low: i32, high: i32) -> i32 {
            low + (self.next() % u32::try_from(high - low + 1).unwrap_or(1)) as i32
        }
    }

    fn entry(id: u32, x: i32, y: i32) -> AoiEntry {
        AoiEntry {
            object_id: id,
            kind: AoiKind::Player,
            position: Point::new(x, y),
        }
    }

    #[test]
    fn insert_query_remove_roundtrip() {
        let mut grid = AoiGrid::new(8);
        assert!(grid.is_empty());
        assert_eq!(grid.insert(entry(1, 3, 4)), None);
        assert_eq!(grid.len(), 1);
        assert_eq!(grid.position_of(AoiKind::Player, 1), Some(Point::new(3, 4)));
        assert_eq!(grid.kind_of(AoiKind::Player, 1), Some(AoiKind::Player));
        assert!(grid.contains(AoiKind::Player, 1));

        let found = grid.query_cell(Point::new(3, 4));
        assert_eq!(found, vec![entry(1, 3, 4)]);
        assert!(grid.query_cell(Point::new(3, 5)).is_empty());

        assert_eq!(grid.remove(AoiKind::Player, 1), Some(entry(1, 3, 4)));
        assert!(grid.is_empty());
        assert_eq!(grid.remove(AoiKind::Player, 1), None);
    }

    #[test]
    fn insert_same_id_replaces_old_bucket() {
        let mut grid = AoiGrid::new(8);
        grid.insert(entry(7, 1, 1));
        let old = grid.insert(entry(7, 40, 40));
        assert_eq!(old, Some(entry(7, 1, 1)));
        assert!(grid.query_cell(Point::new(1, 1)).is_empty());
        assert_eq!(grid.query_cell(Point::new(40, 40)), vec![entry(7, 40, 40)]);
        assert_eq!(grid.len(), 1);
    }

    #[test]
    fn different_kinds_with_same_id_do_not_overwrite() {
        let mut grid = AoiGrid::new(8);
        grid.insert(entry(1, 3, 3));
        grid.insert(AoiEntry {
            object_id: 1,
            kind: AoiKind::Npc,
            position: Point::new(4, 4),
        });
        assert_eq!(grid.len(), 2);
        assert_eq!(grid.query_cell(Point::new(3, 3)).len(), 1);
        assert_eq!(grid.query_cell(Point::new(4, 4)).len(), 1);
        assert!(grid.remove(AoiKind::Player, 1).is_some());
        assert!(grid.contains(AoiKind::Npc, 1));
    }

    #[test]
    fn move_across_buckets_keeps_single_entry() {
        let mut grid = AoiGrid::new(8);
        grid.insert(entry(2, 0, 0));
        assert!(grid.move_to(AoiKind::Player, 2, Point::new(9, 9)));
        assert_eq!(grid.position_of(AoiKind::Player, 2), Some(Point::new(9, 9)));
        assert!(grid.query_cell(Point::new(0, 0)).is_empty());
        assert_eq!(grid.query_cell(Point::new(9, 9)), vec![entry(2, 9, 9)]);
        assert_eq!(grid.len(), 1);

        // 同桶内移动不搬桶。
        assert!(grid.move_to(AoiKind::Player, 2, Point::new(10, 10)));
        assert_eq!(
            grid.position_of(AoiKind::Player, 2),
            Some(Point::new(10, 10))
        );
        assert_eq!(grid.query_cell(Point::new(9, 9)), Vec::new());
        assert_eq!(grid.query_cell(Point::new(10, 10)), vec![entry(2, 10, 10)]);
        assert!(!grid.move_to(AoiKind::Player, 99, Point::new(1, 1)));
    }

    #[test]
    fn radius_query_is_chebyshev_and_orders_by_y_x_id() {
        let mut grid = AoiGrid::new(8);
        grid.insert(entry(3, 2, 0));
        grid.insert(entry(2, 0, 2));
        grid.insert(entry(1, 1, 1));
        grid.insert(entry(4, 3, 3));
        grid.insert(entry(5, 4, 4));
        let found = grid.query_radius(Point::new(1, 1), 2);
        let ids: Vec<u32> = found.iter().map(|entry| entry.object_id).collect();
        assert_eq!(ids, vec![3, 1, 2, 4], "切比雪夫 2 且按 (y,x,id) 排序");
        let far = grid.query_radius(Point::new(1, 1), 3);
        assert_eq!(far.len(), 5);
    }

    #[test]
    fn negative_coordinates_bucket_correctly() {
        let mut grid = AoiGrid::new(8);
        grid.insert(entry(1, -1, -1));
        grid.insert(entry(2, 0, 0));
        assert_eq!(grid.query_cell(Point::new(-1, -1)).len(), 1);
        assert_eq!(grid.query_cell(Point::new(0, 0)).len(), 1);
        assert_eq!(grid.query_radius(Point::new(-1, -1), 1).len(), 2);
    }

    /// 需求里的随机对拍：200 个对象、1000 步随机增删移动，每一步都与暴力扫描比对。
    #[test]
    fn random_operations_match_bruteforce_scan() {
        let mut rng = Lcg::new(0x5eed);
        let mut grid = AoiGrid::new(AOI_CELL_SIZE);
        let mut mirror: HashMap<AoiKey, AoiEntry> = HashMap::new();
        let kinds = [
            AoiKind::Player,
            AoiKind::Monster,
            AoiKind::Npc,
            AoiKind::Item,
        ];

        for id in 1..=200 {
            let found = AoiEntry {
                object_id: id,
                kind: kinds[(id as usize) % kinds.len()],
                position: Point::new(rng.range(-30, 60), rng.range(-30, 60)),
            };
            mirror.insert(found.key(), found);
            grid.insert(found);
        }
        assert_eq!(grid.len(), mirror.len());

        for _ in 0..1000 {
            match rng.next() % 3 {
                0 => {
                    // 移动一个已有对象。
                    let id = rng.range(1, 200) as u32;
                    let kind = kinds[(id as usize) % kinds.len()];
                    if let Some(entry) = mirror.get_mut(&(kind, id)) {
                        entry.position = Point::new(rng.range(-40, 80), rng.range(-40, 80));
                        assert!(grid.move_to(kind, id, entry.position));
                    }
                }
                1 => {
                    // 删除一个对象。
                    let id = rng.range(1, 220) as u32;
                    let kind = kinds[(id as usize) % kinds.len()];
                    let expected = mirror.remove(&(kind, id));
                    assert_eq!(grid.remove(kind, id), expected);
                }
                _ => {
                    // 重新插入（可能替换）。
                    let id = rng.range(1, 220) as u32;
                    let kind = kinds[(id as usize) % kinds.len()];
                    let found = AoiEntry {
                        object_id: id,
                        kind,
                        position: Point::new(rng.range(-40, 80), rng.range(-40, 80)),
                    };
                    let expected = mirror.insert(found.key(), found);
                    assert_eq!(grid.insert(found), expected);
                }
            }

            let center = Point::new(rng.range(-40, 80), rng.range(-40, 80));
            let radius = rng.range(0, 20);
            let mut expected: Vec<AoiEntry> = mirror
                .values()
                .copied()
                .filter(|entry| entry.position.in_range(center, radius))
                .collect();
            expected.sort_by_key(|entry| (entry.position.y, entry.position.x, entry.object_id));
            assert_eq!(
                grid.query_radius(center, radius),
                expected,
                "center={center:?} radius={radius}"
            );
        }
    }
}
