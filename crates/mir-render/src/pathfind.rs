//! 8 方向 A* 寻路（纯函数、无状态、可头less 测试）。
//!
//! # 与 Crystal 的对应关系
//!
//! Crystal 客户端的寻路在 `Client/MirObjects/PathFinder.cs`：
//!
//! * `FindPath(start, target, MaxNodes)` 用二叉堆跑 A*（`PathFinder.cs:194-239`）；
//! * 邻居是 8 方向（`:274-294`）；
//! * 代价 `GetDistance` 是「14 × 斜向步数 + 10 × 直线剩余步数」（`:258-267`）；
//! * `CompareTo` 先比 `FCost`、再比 `HCost`（`:146-154`）；
//! * `MaxNodes` 非 0 且路径点数不满足上限时返回 `null`（`:210-214`）。
//!
//! 本模块只做「算方向」，不依赖地图类型：可走性由调用方用闭包传入。
//! 这与 Crystal 的 `Node.Walkable => Map.EmptyCell(Location)`（`PathFinder.cs:120-125`）
//! 是同一层信息，只是本作把它抽成 `impl Fn(i32, i32) -> bool`，`mir-render` 不必知道
//! `mir_world::Map` 的存储细节。
//!
//! # 本作的加强
//!
//! * **斜向不切角**：走对角前要求两侧的正交格都可走（需求）。Crystal 的
//!   `GetNeighbours` 没有这个检查；在门框/墙角处 Crystal 会「贴角穿过去」，
//!   本作宁可绕路也不穿墙。
//! * **最大节点数**按「弹出并展开的节点数」计，而不是按最终路径长度计
//!   （Crystal 的 `MaxNodes` 是路径点数上限，`FindPath(..., 20)` 在
//!   `GameScene.cs:11312` 用来限制点击走路的距离）。本作把两者拆开：节点上限防止
//!   在超大地图上搜索爆炸，调用方还可以用 [`MAX_PATH_STEPS`] 限制长度。
//!
//! # 返回方向
//!
//! 返回从 `start` 的**下一格**开始、到 `goal` 为止的 [`MirDirection`] 序列；
//! `start == goal` 返回空 `Vec`；不可达/超过上限返回 `None`。
//! 序列中的每一步都满足「相邻格」且「可走」，所以调用方可以逐格发 `C.Walk`。

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use mir_core::Point;
use mir_core::enums::MirDirection;

/// 默认搜索节点上限（需求给的 2000）。
pub const DEFAULT_MAX_NODES: usize = 2_000;
/// 默认路径步数上限（Crystal 点击走路用 `FindPath(..., 20)` 限制距离，
/// `GameScene.cs:11312`；本作的点地走路也沿用 20 格）。
pub const MAX_PATH_STEPS: usize = 20;
/// 直线一步的代价（Crystal `GetDistance` 的 10，`PathFinder.cs:264-266`）。
pub const STRAIGHT_COST: u32 = 10;
/// 斜向一步的代价（Crystal `GetDistance` 的 14，`PathFinder.cs:264-266`）。
pub const DIAGONAL_COST: u32 = 14;

/// 在可走性闭包上跑 A*，返回从起点下一格到终点的方向序列。
///
/// * `walkable(x, y)` 必须对**越界坐标**返回 `false`（本模块不假设地图尺寸）；
/// * `max_nodes == 0` 时使用 [`DEFAULT_MAX_NODES`]；
/// * 起点不要求可走（角色可能暂时站在门/物品格上），但每一步的落点必须可走；
/// * 对角步要求两侧正交格都可走，避免切墙角。
#[must_use]
pub fn find_path(
    start: Point,
    goal: Point,
    walkable: impl Fn(i32, i32) -> bool,
    max_nodes: usize,
) -> Option<Vec<MirDirection>> {
    if start == goal {
        return Some(Vec::new());
    }
    if !walkable(goal.x, goal.y) {
        return None;
    }
    let limit = if max_nodes == 0 {
        DEFAULT_MAX_NODES
    } else {
        max_nodes
    };

    let mut open = BinaryHeap::new();
    let mut g_score: HashMap<Point, u32> = HashMap::new();
    let mut parents: HashMap<Point, (Point, MirDirection)> = HashMap::new();
    let mut closed: HashSet<Point> = HashSet::new();
    let mut sequence = 0u64;

    g_score.insert(start, 0);
    open.push(OpenEntry {
        point: start,
        g: 0,
        h: heuristic(start, goal),
        sequence,
    });
    sequence = sequence.saturating_add(1);

    let mut expanded = 0usize;
    while let Some(entry) = open.pop() {
        if closed.contains(&entry.point) {
            continue;
        }
        if g_score
            .get(&entry.point)
            .is_some_and(|&best| entry.g > best)
        {
            continue;
        }
        closed.insert(entry.point);
        expanded = expanded.saturating_add(1);
        if entry.point == goal {
            return Some(retrace(&parents, start, goal));
        }
        if expanded >= limit {
            return None;
        }

        for direction in MirDirection::ALL {
            let (dx, dy) = direction.delta();
            let next = Point::new(entry.point.x + dx, entry.point.y + dy);
            if !walkable(next.x, next.y) || closed.contains(&next) {
                continue;
            }
            let diagonal = dx != 0 && dy != 0;
            if diagonal {
                // 两侧正交格也必须可走（不切墙角）：(dx,0) 与 (0,dy)。
                if !walkable(entry.point.x + dx, entry.point.y)
                    || !walkable(entry.point.x, entry.point.y + dy)
                {
                    continue;
                }
            }
            let step = if diagonal {
                DIAGONAL_COST
            } else {
                STRAIGHT_COST
            };
            let candidate = entry.g.saturating_add(step);
            if g_score.get(&next).is_some_and(|&best| candidate >= best) {
                continue;
            }
            g_score.insert(next, candidate);
            parents.insert(next, (entry.point, *direction));
            open.push(OpenEntry {
                point: next,
                g: candidate,
                h: heuristic(next, goal),
                sequence,
            });
            sequence = sequence.saturating_add(1);
        }
    }
    None
}

/// 按「最大步数」过滤路径；超过上限时返回 `None`（Crystal 的 `MaxNodes` 语义，`PathFinder.cs:210-214`）。
#[must_use]
pub fn find_path_with_steps(
    start: Point,
    goal: Point,
    walkable: impl Fn(i32, i32) -> bool,
    max_nodes: usize,
    max_steps: usize,
) -> Option<Vec<MirDirection>> {
    let path = find_path(start, goal, walkable, max_nodes)?;
    (path.len() <= max_steps).then_some(path)
}

/// Crystal `GetDistance`：`14 × min(dx, dy) + 10 × |dx - dy|`（`PathFinder.cs:258-267`）。
#[must_use]
fn heuristic(from: Point, to: Point) -> u32 {
    let dx = from.x.abs_diff(to.x);
    let dy = from.y.abs_diff(to.y);
    let diagonal = dx.min(dy);
    let straight = dx.max(dy).saturating_sub(diagonal);
    diagonal
        .saturating_mul(DIAGONAL_COST)
        .saturating_add(straight.saturating_mul(STRAIGHT_COST))
}

/// 从父表回溯方向序列。
fn retrace(
    parents: &HashMap<Point, (Point, MirDirection)>,
    start: Point,
    goal: Point,
) -> Vec<MirDirection> {
    let mut path = Vec::new();
    let mut current = goal;
    while current != start {
        let Some(&(parent, direction)) = parents.get(&current) else {
            // 理论上不会发生：goal 是从 open 里弹出的，必然有父节点链。
            return Vec::new();
        };
        path.push(direction);
        current = parent;
    }
    path.reverse();
    path
}

/// A* 开表项：最小 `f`、再最小 `h`、再先进先出。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OpenEntry {
    /// 格。
    point: Point,
    /// 已走代价。
    g: u32,
    /// 启发式代价。
    h: u32,
    /// 入堆序号（稳定排序用）。
    sequence: u64,
}

impl OpenEntry {
    /// `f = g + h`。
    const fn f(self) -> u32 {
        self.g.saturating_add(self.h)
    }
}

impl Ord for OpenEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // `BinaryHeap` 是最大堆，所以这里反向比较：f 小、h 小、seq 小的先弹出。
        other
            .f()
            .cmp(&self.f())
            .then_with(|| other.h.cmp(&self.h))
            .then_with(|| other.sequence.cmp(&self.sequence))
    }
}

impl PartialOrd for OpenEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 全部可走的无限地图。
    fn open(_x: i32, _y: i32) -> bool {
        true
    }

    /// 路径的每一步都相邻，且终点正确。
    fn assert_valid_path(start: Point, goal: Point, path: &[MirDirection]) {
        let mut current = start;
        for direction in path {
            current = current.moved(*direction, 1);
        }
        assert_eq!(current, goal, "路径终点必须是目标格");
        for window in path.windows(2) {
            let (dx, dy) = window[1].delta();
            let (px, py) = window[0].delta();
            // 只检查相邻：上一步落点到下一步方向不能「跳格」，这条由构造保证；
            // 反向立即掉头会让路径抖动，A* 不应该产生。
            assert!((dx, dy) != (-px, -py), "路径不应出现立即掉头：{window:?}");
        }
    }

    #[test]
    fn straight_line_is_one_step_per_cell() {
        let start = Point::new(5, 5);
        let goal = Point::new(5, 10);
        let path = find_path(start, goal, open, DEFAULT_MAX_NODES).expect("直线可达");
        assert_eq!(path, vec![MirDirection::Down; 5]);
        assert_valid_path(start, goal, &path);

        let diagonal = find_path(Point::new(5, 5), Point::new(8, 8), open, DEFAULT_MAX_NODES)
            .expect("对角可达");
        assert_eq!(diagonal, vec![MirDirection::DownRight; 3]);
    }

    #[test]
    fn starts_and_goals_on_the_same_cell_return_an_empty_path() {
        let cell = Point::new(7, 7);
        assert_eq!(
            find_path(cell, cell, open, DEFAULT_MAX_NODES),
            Some(Vec::new())
        );
    }

    #[test]
    fn wall_detour_keeps_every_step_walkable() {
        // x = 4 的墙，y = 0..=4；从 (2,2) 到 (6,2) 必须从 y=5 的缺口绕过去。
        let start = Point::new(2, 2);
        let goal = Point::new(6, 2);
        let walkable = |x: i32, y: i32| !(x == 4 && (0..=4).contains(&y));
        let path = find_path(start, goal, walkable, DEFAULT_MAX_NODES).expect("绕墙可达");
        assert!(path.len() > 4, "绕墙路径应比直线（4 步）长：{}", path.len());
        assert_valid_path(start, goal, &path);
        let mut current = start;
        for direction in &path {
            current = current.moved(*direction, 1);
            assert!(walkable(current.x, current.y), "路径经过了墙：{current:?}");
        }
        assert!(
            path.iter()
                .any(|direction| matches!(direction, MirDirection::Down)),
            "绕墙应向下走"
        );
    }

    #[test]
    fn unreachable_goal_returns_none() {
        // 一整列墙把地图切成两半。
        let walkable = |x: i32, _y: i32| x != 5;
        assert_eq!(
            find_path(
                Point::new(1, 1),
                Point::new(8, 8),
                walkable,
                DEFAULT_MAX_NODES
            ),
            None
        );
    }

    #[test]
    fn diagonal_steps_may_not_cut_corners() {
        // 目标 (1,1) 可走，但它是唯一的出口；(1,0) 与 (0,1) 被堵住时不能斜穿过去。
        let walkable = |x: i32, y: i32| matches!((x, y), (0, 0) | (1, 1));
        assert_eq!(
            find_path(
                Point::new(0, 0),
                Point::new(1, 1),
                walkable,
                DEFAULT_MAX_NODES
            ),
            None,
            "两侧被堵时不能切角"
        );
        // 两侧都可走时，对角一步是合法的。
        let path = find_path(Point::new(0, 0), Point::new(1, 1), open, DEFAULT_MAX_NODES)
            .expect("两侧可走");
        assert_eq!(path, vec![MirDirection::DownRight]);
    }

    #[test]
    fn node_limit_stops_the_search() {
        let start = Point::new(0, 0);
        let goal = Point::new(40, 40);
        assert_eq!(
            find_path(start, goal, open, 2),
            None,
            "2 个节点不可能搜到 80 格外的目标"
        );
        assert!(
            find_path(start, goal, open, DEFAULT_MAX_NODES).is_some(),
            "默认上限应足够"
        );
    }

    #[test]
    fn step_limit_matches_crystal_click_distance() {
        let start = Point::new(0, 0);
        let near = find_path_with_steps(start, Point::new(3, 3), open, DEFAULT_MAX_NODES, 3);
        assert_eq!(near.map(|path| path.len()), Some(3));
        let far = find_path_with_steps(start, Point::new(30, 30), open, DEFAULT_MAX_NODES, 20);
        assert_eq!(far, None);
    }

    #[test]
    fn non_walkable_goal_is_rejected_before_searching() {
        let walkable = |x: i32, y: i32| !(x == 3 && y == 3);
        assert_eq!(
            find_path(
                Point::new(0, 0),
                Point::new(3, 3),
                walkable,
                DEFAULT_MAX_NODES
            ),
            None
        );
    }

    #[test]
    fn diagonal_paths_are_cheaper_than_staircases() {
        // 目标 (10,10) 在空地上：全对角 10 步（140）比「先右再下」的 20 步（200）便宜。
        let path = find_path(
            Point::new(0, 0),
            Point::new(10, 10),
            open,
            DEFAULT_MAX_NODES,
        )
        .expect("可达");
        assert_eq!(path.len(), 10);
        assert!(path.iter().all(|direction| matches!(
            direction,
            MirDirection::DownRight
                | MirDirection::DownLeft
                | MirDirection::UpRight
                | MirDirection::UpLeft
        )));
    }

    #[test]
    fn heuristic_matches_crystals_distance() {
        assert_eq!(heuristic(Point::new(0, 0), Point::new(3, 3)), 42);
        assert_eq!(heuristic(Point::new(0, 0), Point::new(5, 2)), 58);
        assert_eq!(heuristic(Point::new(5, 5), Point::new(5, 5)), 0);
    }

    #[test]
    fn result_is_deterministic() {
        let walkable = |x: i32, y: i32| !(x == 4 && (0..=4).contains(&y));
        let first = find_path(Point::new(2, 2), Point::new(6, 2), walkable, 2_000);
        let second = find_path(Point::new(2, 2), Point::new(6, 2), walkable, 2_000);
        assert_eq!(first, second);
    }
}
