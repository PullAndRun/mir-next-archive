//! 多地图：地形缓存、传送点（`MovementInfo`）、切图与 `S.MapChanged`。
//!
//! Crystal 的服务器一开始就把**所有**地图建好放进 `Envir.MapList`
//! （`Server/MirDatabase/MapInfo.cs:197-215` 的 `CreateMap` → `Map.Load`，
//! `Server/MirEnvir/Map.cs:432-514`），玩家对象上只有 `CurrentMap` / `CurrentMapIndex`。
//! 本作把「建图」（读 `.map` + 刷 NPC + 注册刷怪点）推迟到**第一次有人进入这张图**：
//!
//! | 步骤 | 本作 | Crystal |
//! |---|---|---|
//! | 读地形 | [`World::load_map`] | `Map.Load`（`Map.cs:432-514`） |
//! | 刷怪点 | [`World::initialize_map`] → [`World::register_spawn_group`] | `Map.Load` 里的 `MapRespawn` 循环（`Map.cs:473-488`） |
//! | NPC | [`World::initialize_map`] → [`World::spawn_npcs`] | `Map.Load` 里的 `NPCObject` 循环（`Map.cs:490-496`） |
//!
//! # 传送点（`MovementInfo`）
//!
//! 规则抄 `Server/MirObjects/PlayerObject.cs:4459-4514` 的 `CheckMovement`：
//!
//! 1. `Envir.Time < MovementTime` 直接返回（`:4461`）——切图后 [`MOVEMENT_DELAY_MS`] 内不再触发；
//! 2. 遍历 `CurrentMap.Info.Movements`，只看 `Source == location` 的条目（`:4474-4478`）；
//! 3. `NeedHole` 要求同格有「挖开的洞」（`:4480-4487`，Crystal 检查同格的法术对象
//!    `Spell.DigOutZombie` / `Spell.DigOutArmadillo`；本作没有挖洞技能，用 [`World::set_hole`] 的集合代替）；
//! 4. `ConquestIndex > 0` 要求**行会拥有**对应的攻城区（`:4489-4493`），见 [`crate::conquest`]；
//! 5. `NeedMove` 只把目的地记进 `NPCData`（`:4495-4500`），真正切图由 NPC 动作 `ENTERMAP`
//!    （`Server/MirObjects/NPC/NPCSegment.cs:4065-4074`）执行；
//! 6. 其余情况：目标图存在且落点合法才传送（`:4502-4510`）。
//!
//! **与任务描述/本仓库 Crystal 分支的差异**（都写进了 `docs/world/maps.md`）：
//!
//! * 任务约定 `MapIndex == 0` 表示「同图传送到 `Destination`」；本仓库的 Crystal 分支是
//!   `Envir.GetMap(info.MapIndex)`（`Envir.cs:4587-4590`），`0` 就是 0 号地图。本作按任务
//!   约定实现（0 = 同图），> 0 才去 `db.maps` 里按主键找。
//! * 落点越界/不可走时**不**放弃传送，而是夹进地图边界并就近找一个可走格
//!   （Crystal 是 `ValidPoint` 不通过就 `continue`，`:4504`）；`MapChangeReport::fallback_from`
//!   会记录原始请求点，测试也断言这一行为。
//! * 执行时机：Crystal 上游在 `HumanObject.Process` 里每 tick 调 `MapTime()`（下一 tick 生效），
//!   本仓库的 Crystal 分支把 `CheckMovement` 内联进 `Walk`/`Run`/`Turn` 并**同步**执行
//!   （`HumanObject.cs:2466`、`:2584`、`PlayerObject.cs:4273`）。本作把「命中」与「切图」
//!   拆成两步：走/跑/转身时**排队**，`World::tick` 开头统一结算，等价于上游的「下一 tick」。
//!
//! # 切图包序
//!
//! 逐条对应 `PlayerObject.CompleteMapMovement`（`PlayerObject.cs:4515-4618`）：
//!
//! | # | 收件人 | 包 | Crystal |
//! |---|---|---|---|
//! | 1 | 旧图视野内玩家 | `ObjectRemove` | `:4506-4507` |
//! | 2 | 新图视野内玩家 | `ObjectPlayer`（自己的外观） | `CurrentMap.AddObject(this)`，`:4562` |
//! | 3 | 本人 | `MapChanged` | `:4566-4579` |
//! | 4 | 本人 | `ObjectPlayer` / `ObjectMonster` / `ObjectNPC` / `ObjectItem`/`ObjectGold` | `GetObjects()`，`:4583` → `:1741-1768` |
//!
//! 第 4 步的顺序与 [`World::enter`] 完全一致（玩家 → 怪物 → NPC → 地面物品），
//! 因为两者共用 [`World::gather_view`]。

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{LightSetting, MirDirection, WeatherSetting};
use mir_db::MovementInfo;
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::MapChanged;

use crate::aoi::AoiKind;
use crate::world::{Player, VIEW_RANGE, World, object_remove_frame};

/// 传送点切图后的移动冷却：Crystal `PlayerObject.MovementDelay = 2000`
/// （`Server/MirObjects/PlayerObject.cs:113`），`CompleteMapMovement` 里
/// `MovementTime = Envir.Time + MovementDelay`（`:4564`），
/// `CheckMovement` 第一行 `if (Envir.Time < MovementTime) return false;`（`:4461`）。
pub const MOVEMENT_DELAY_MS: i64 = 2_000;

/// 地图缓存上限的环境变量（见 [`World::set_map_cache_limit`]）。
pub const MAP_CACHE_ENV: &str = "MIR2_MAP_CACHE";

/// 地图缓存默认上限（张）：`0.map` 解析后大约 6 MB，8 张约 50 MB。
pub const DEFAULT_MAP_CACHE_LIMIT: usize = 8;

/// 从环境变量读地图缓存上限（未设置或非法时用 [`DEFAULT_MAP_CACHE_LIMIT`]）。
///
/// `0` 表示不限制。
#[must_use]
pub fn map_cache_limit_from_env() -> usize {
    match std::env::var(MAP_CACHE_ENV) {
        Ok(value) => match value.trim().parse::<usize>() {
            Ok(limit) => limit,
            Err(error) => {
                tracing::warn!(%value, %error, "MIR2_MAP_CACHE 不是整数，使用默认上限");
                DEFAULT_MAP_CACHE_LIMIT
            }
        },
        Err(_) => DEFAULT_MAP_CACHE_LIMIT,
    }
}

/// 一张地图「首次进入」时补做了哪些事（Crystal `Map.Load`，`Map.cs:432-514`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MapInitReport {
    /// 本次是否真的初始化了（`false` = 之前已经初始化过，或地形加载失败）。
    pub initialized: bool,
    /// 刷出的 NPC 数量（`Map.cs:490-496`）。
    pub npcs: usize,
    /// 注册的刷怪组数量（`Map.cs:473-488`）。
    pub respawn_groups: usize,
    /// 立刻刷出的怪物数量（Crystal 等 `ProcessRespawns`，本作登记时铺满）。
    pub spawned: usize,
}

/// 视野内补齐的对象数量（[`MapChangeReport`] 的一部分，给日志/测试用）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewCounts {
    /// 其它玩家。
    pub players: usize,
    /// 怪物。
    pub monsters: usize,
    /// NPC。
    pub npcs: usize,
    /// 地面物品/金币堆。
    pub items: usize,
}

impl ViewCounts {
    /// 视野内对象总数。
    #[must_use]
    pub const fn total(self) -> usize {
        self.players + self.monsters + self.npcs + self.items
    }
}

/// 切图结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapChangeReport {
    /// 玩家对象 ID。
    pub player_id: u32,
    /// 原地图文件名。
    pub from_map: String,
    /// 新地图文件名。
    pub to_map: String,
    /// 新地图主键（`MapInfo.Index`；数据库里没有这张图时为 0）。
    pub map_index: i32,
    /// 实际落点（已经被 [`World::change_map`] 校正过）。
    pub position: Point,
    /// 朝向。
    pub direction: MirDirection,
    /// 是否换了地图（同图传送为 `false`，但仍然会发 `MapChanged`）。
    pub map_changed: bool,
    /// 本次是否首次进入该图（顺带刷了 NPC/刷怪点）。
    pub first_entry: bool,
    /// 原始请求点被回退时记录它（越界或落在墙里）。
    pub fallback_from: Option<Point>,
    /// 新图视野内补齐的对象数量。
    pub visible: ViewCounts,
}

/// 切图失败的原因。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MapChangeError {
    /// 玩家不在世界里。
    #[error("object {0} is not in the world")]
    NotInWorld(u32),
    /// 目标地图的地形加载失败（文件缺失/损坏，Crystal `Envir.GetMap` 返回 `null`）。
    #[error("map {map} does not exist or cannot be loaded")]
    MapNotFound {
        /// 地图文件名。
        map: String,
    },
}

/// 排队中的传送点触发（走/跑/转身命中后由 `World::tick` 结算）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMapMove {
    /// 玩家对象 ID。
    pub player_id: u32,
    /// 目标地图文件名。
    pub map_file: String,
    /// 目标格（未经落点校正）。
    pub point: Point,
    /// `S.MapChanged.Direction` 用的朝向（Crystal 用当前朝向，`PlayerObject.cs:4576`）。
    pub direction: MirDirection,
}

/// 视野内要下发的帧（Crystal `GetObjects`，`PlayerObject.cs:1741-1768`）。
///
/// 每项是 `(已编码的包, 对象 ID)`，顺序固定为 玩家 → 怪物 → NPC → 地面物品，
/// 与 [`World::enter`] 一致。
#[derive(Debug, Default, Clone)]
pub(crate) struct ViewFrames {
    /// 其它玩家的 `S.ObjectPlayer`。
    pub players: Vec<(Vec<u8>, u32)>,
    /// `S.ObjectMonster`。
    pub monsters: Vec<(Vec<u8>, u32)>,
    /// `S.ObjectNPC`。
    pub npcs: Vec<(Vec<u8>, u32)>,
    /// `S.ObjectItem` / `S.ObjectGold`。
    pub items: Vec<(Vec<u8>, u32)>,
}

impl ViewFrames {
    /// 其它玩家对象 ID 集合（写回 `World::visible_players`）。
    pub(crate) fn player_ids(&self) -> BTreeSet<u32> {
        self.players.iter().map(|(_, id)| *id).collect()
    }

    /// 怪物对象 ID 集合（写回 `Player::visible_monsters`）。
    pub(crate) fn monster_ids(&self) -> BTreeSet<u32> {
        self.monsters.iter().map(|(_, id)| *id).collect()
    }

    /// NPC 对象 ID 集合（写回 `Player::visible_npcs`）。
    pub(crate) fn npc_ids(&self) -> BTreeSet<u32> {
        self.npcs.iter().map(|(_, id)| *id).collect()
    }

    /// 地面物品对象 ID 集合（写回 `Player::visible_items`）。
    pub(crate) fn item_ids(&self) -> BTreeSet<u32> {
        self.items.iter().map(|(_, id)| *id).collect()
    }

    /// 各类对象的数量。
    fn counts(&self) -> ViewCounts {
        ViewCounts {
            players: self.players.len(),
            monsters: self.monsters.len(),
            npcs: self.npcs.len(),
            items: self.items.len(),
        }
    }
}

impl World {
    /// 设置地图文件目录（Crystal `Settings.MapPath`，`Server/Settings.cs`）。
    ///
    /// 目录里应放 `<地图文件名>.map`（例如 `resource/map/0.map`）。
    pub fn set_map_root(&mut self, root: impl AsRef<Path>) {
        self.map_root = Some(root.as_ref().to_path_buf());
        // 换了目录以后之前的「加载失败」缓存不再可信。
        self.missing_maps.clear();
    }

    /// 地图文件目录。
    #[must_use]
    pub fn map_root(&self) -> Option<&Path> {
        self.map_root.as_deref()
    }

    /// 设置地图缓存上限（`0` = 不限制）。
    ///
    /// 超出上限时优先淘汰**没有对象**的地图；有玩家/怪物/NPC/地面物品的图不会被淘汰，
    /// 因此实际缓存可能暂时超过上限（记一条 `debug`）。
    pub fn set_map_cache_limit(&mut self, limit: usize) {
        self.map_cache_limit = limit;
        self.evict_maps();
    }

    /// 当前地图缓存上限。
    #[must_use]
    pub const fn map_cache_limit(&self) -> usize {
        self.map_cache_limit
    }

    /// 已缓存的地图文件名（调试/测试用）。
    #[must_use]
    pub fn cached_maps(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.maps.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    /// 直接放进一张已经读好的地图（测试/工具用；生产路径走 [`World::load_map`]）。
    pub fn insert_map(&mut self, map_file: impl Into<String>, map: Arc<mir_world::Map>) {
        let map_file = map_file.into();
        self.missing_maps.remove(&map_file);
        self.maps.insert(map_file.clone(), map);
        // 门表随地形一起就位（`register_map_doors` 是幂等 `or_insert`），
        // 避免缓存淘汰后重插地图时丢掉门的运行时状态。
        self.register_map_doors(&map_file);
        self.evict_maps();
    }

    /// 按需从 `{map_root}/{map_file}.map` 读一张图并缓存（Crystal `Envir.GetMap`，
    /// `Envir.cs:4587-4590` + `MapInfo.CreateMap`，`MapInfo.cs:197-215`）。
    ///
    /// 读失败的地图名会被记住，避免反复读盘；返回 `None` 表示这张图不可用。
    pub fn load_map(&mut self, map_file: &str) -> Option<Arc<mir_world::Map>> {
        if let Some(map) = self.maps.get(map_file) {
            return Some(Arc::clone(map));
        }
        if self.missing_maps.contains(map_file) {
            return None;
        }
        let root = self.map_root.clone()?;
        let path = root.join(format!("{map_file}.map"));
        // `Map::load` 会优先读同目录同名 `.mnm`；日志按**实际源**打印，避免审计误判成回退。
        let mnm_path = path.with_extension("mnm");
        let (source_path, source) = if mnm_path.is_file() {
            (mnm_path, "mnm")
        } else {
            (path.clone(), "map")
        };
        match mir_world::Map::load(&path) {
            Ok(map) => {
                tracing::info!(
                    map = %map_file,
                    path = %source_path.display(),
                    source,
                    width = map.width,
                    height = map.height,
                    "已加载地图"
                );
                let map = Arc::new(map);
                self.insert_map(map_file, Arc::clone(&map));
                Some(map)
            }
            Err(error) => {
                tracing::warn!(
                    map = %map_file,
                    path = %path.display(),
                    %error,
                    "地图加载失败，这张图暂时不可进入"
                );
                self.missing_maps.insert(map_file.to_owned());
                None
            }
        }
    }

    /// 取一张已加载的地图（Crystal `Envir.GetMap` 的运行期对应物）。
    ///
    /// 世界没有任何**按名字**加载的地图时，返回 [`World::with_map`] 设置的兜底地图
    /// （单地图世界），否则返回 `None`。
    #[must_use]
    pub fn map_of(&self, map_file: &str) -> Option<&Arc<mir_world::Map>> {
        if let Some(map) = self.maps.get(map_file) {
            return Some(map);
        }
        if self.maps.is_empty() {
            return self.fallback_map.as_ref();
        }
        None
    }

    /// 某张图允许的坐标上界 `(max_x, max_y)`（Crystal `Map.ValidPoint`，
    /// `Server/MirEnvir/Map.cs:637-644`：`x < Width && y < Height`）。
    ///
    /// 没有地形时退回构造 [`World`] 时的 `bound`（两条轴相同）。
    #[must_use]
    pub fn map_bounds(&self, map_file: &str) -> (i32, i32) {
        match self.map_of(map_file) {
            Some(map) => (map.width.saturating_sub(1), map.height.saturating_sub(1)),
            None => (self.bound, self.bound),
        }
    }

    /// 某张图的传送点表（Crystal `MapInfo.Movements`）。
    ///
    /// 表在 [`World::initialize_map`] 里从 `Server.MirDB` 灌入，也可以由
    /// [`World::set_map_movements`] 直接设置。
    #[must_use]
    pub fn movements_on(&self, map_file: &str) -> &[MovementInfo] {
        match self.movements.get(map_file) {
            Some(list) => list,
            None => &[],
        }
    }

    /// 直接设置某张图的传送点表（测试/工具用）。
    pub fn set_map_movements(&mut self, map_file: impl Into<String>, movements: Vec<MovementInfo>) {
        self.movements.insert(map_file.into(), Arc::new(movements));
    }

    /// 挖开/填上某个格子的洞（`need_hole` 传送点的通行条件）。
    ///
    /// Crystal 通过同格的 `Spell.DigOutZombie` / `Spell.DigOutArmadillo` 法术对象判断
    /// （`Server/MirObjects/PlayerObject.cs:4480-4487`）；本作没有挖洞技能，用这个集合表示。
    pub fn set_hole(&mut self, map_file: impl Into<String>, cell: Point, open: bool) {
        let key = (map_file.into(), cell);
        if open {
            self.holes.insert(key);
        } else {
            self.holes.remove(&key);
        }
    }

    /// 某个格子是不是已经挖开的洞。
    #[must_use]
    pub fn has_hole(&self, map_file: &str, cell: Point) -> bool {
        self.holes.contains(&(map_file.to_owned(), cell))
    }

    /// 地图主键（Crystal `MapInfo.Index`）；数据库里没有这张图时返回 0。
    #[must_use]
    pub fn map_index_of(&self, map_file: &str) -> i32 {
        self.game_data
            .as_ref()
            .and_then(|data| data.map_info(map_file))
            .map_or(0, |info| info.index)
    }

    /// 排队中的传送点触发（测试/运维观察用）。
    #[must_use]
    pub fn pending_map_moves(&self) -> &[PendingMapMove] {
        &self.pending_map_moves
    }

    /// 初始化一张地图：读地形、灌传送点表、刷 NPC、注册刷怪点。
    ///
    /// 对应 Crystal 建图时一次做完的三件事（`Server/MirEnvir/Map.cs:432-514`）：
    /// `Map.Load` 读地形 → `MapRespawn` 循环（`:473-488`）→ `NPCObject` 循环（`:490-496`）。
    /// 本作每张图只做一次；地形加载失败时返回 `initialized = false`，下次进入还会重试。
    pub fn initialize_map(&mut self, map_file: &str) -> MapInitReport {
        if self.initialized_maps.contains(map_file) {
            return MapInitReport::default();
        }
        // Crystal `MapInfo.CreateMap` 在 `map.Load()` 失败时直接返回（`MapInfo.cs:206-208`），
        // 这张图不会出现在 `Envir.MapList` 里。
        if self.load_map(map_file).is_none() {
            return MapInitReport::default();
        }
        self.initialized_maps.insert(map_file.to_owned());
        // 门（Crystal `Map.Load` 里 `AddDoor`，`Map.cs:50-59`、`:143`）。
        self.register_map_doors(map_file);
        // 传送点（`MapInfo.Movements`，`Server/MirDatabase/MovementInfo.cs`）。
        // 已经由测试/工具用 `set_map_movements` 设过的表优先，数据库不覆盖。
        if let Some(info) = self
            .game_data
            .as_ref()
            .and_then(|data| data.map_info(map_file))
        {
            self.movements
                .entry(map_file.to_owned())
                .or_insert_with(|| Arc::new(info.movements.clone()));
        }
        // NPC（Crystal `Map.cs:490-496`）。
        let npcs = self.spawn_npcs(map_file).len();
        // 刷怪点（Crystal `Map.cs:473-488`）。
        let mut respawn_groups = 0;
        let mut spawned = 0;
        if let Some(data) = self.game_data.clone() {
            for group in data.spawn_groups(map_file) {
                respawn_groups += 1;
                spawned += self.register_spawn_group(group).len();
            }
        }
        MapInitReport {
            initialized: true,
            npcs,
            respawn_groups,
            spawned,
        }
    }

    /// 把玩家切到另一张图（同图也允许，用于传送点同图传送）。
    ///
    /// 移植 `PlayerObject.CompleteMapMovement`（`Server/MirObjects/PlayerObject.cs:4515-4618`）：
    /// 旧图广播 `ObjectRemove` → 换图 → 新图广播外观 → 本人 `S.MapChanged` → 补齐视野。
    ///
    /// # Errors
    /// 玩家不在世界里、或目标图的地形加载失败时返回 [`MapChangeError`]（玩家保持原样）。
    pub fn change_map(
        &mut self,
        player_id: u32,
        map_file: &str,
        point: Point,
        direction: MirDirection,
    ) -> Result<MapChangeReport, MapChangeError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(MapChangeError::NotInWorld(player_id));
        };
        let from_map = player.map.clone();
        let from_position = player.position;
        let now = self.now();
        let map_changed = from_map != map_file;

        // 1) 目标地形：跨图时必须有地形（Crystal `Envir.GetMap`，`Envir.cs:4587-4590`；
        //    它返回 null 时 `CheckMovement` 会跳过这条传送点，`PlayerObject.cs:4502-4504`；
        //    本作直接把失败交给调用方：传送点路径记日志并留在原地，NPC 动作让脚本继续）。
        if map_changed && self.load_map(map_file).is_none() {
            return Err(MapChangeError::MapNotFound {
                map: map_file.to_owned(),
            });
        }
        // 2) 首次进入这张图：补 NPC 与刷怪点（Crystal 建图时做，`Map.cs:473-496`）。
        let first_entry = self.initialize_map(map_file).initialized;
        // 3) 落点校正（见模块文档「与任务描述/本仓库 Crystal 分支的差异」）。
        let (landing, fallback_from) = self.resolve_landing(map_file, point);

        // 4) 旧图视野内的玩家收到 `ObjectRemove`（`PlayerObject.cs:4506-4507`）。
        //    用旧图的 AOI 网格取视野内玩家，并同步把 mover 从对方的「已看见」集合里删掉。
        let removal = object_remove_frame(player_id);
        for other_id in
            self.aoi_ids_in_radius(&from_map, from_position, VIEW_RANGE, AoiKind::Player)
        {
            if other_id == player_id {
                continue;
            }
            if let Some(other) = self.players.get(&other_id) {
                let _ = other.sender.send(removal.clone());
            }
            if let Some(seen) = self.visible_players.get_mut(&other_id) {
                seen.remove(&player_id);
            }
        }

        // 5) 换图 + 冷却 + 清空旧图视野（`PlayerObject.cs:4558-4564`）。
        //    AOI 也要跟着搬图：先摘旧图，再按落点挂新图（同图传送等于 move）。
        self.aoi_remove(&from_map, AoiKind::Player, player_id);
        if let Some(player) = self.players.get_mut(&player_id) {
            map_file.clone_into(&mut player.map);
            player.position = landing;
            player.direction = direction;
            player.movement_time = now + MOVEMENT_DELAY_MS;
            player.visible_monsters.clear();
            player.visible_npcs.clear();
            player.visible_items.clear();
            // 换图等于离开当前 NPC；客户端收到 `MapChanged` 也会把对话/商店收起来
            // （`Client/MirScenes/GameScene.cs:3970-4009`）。
            player.npc.close();
        }
        self.aoi_insert(map_file, player_id, AoiKind::Player, landing);
        self.visible_players.remove(&player_id);
        // 6) 丢弃以该玩家为发起者/目标的延迟伤害与延迟魔法：Crystal 的 `ActionList`
        //    挂在地图/对象上，切图不会丢；本作队列里只记对象 ID 与坐标，留着会在新图上错位结算。
        self.pending_hits
            .retain(|hit| hit.attacker_id != player_id && hit.target_id != player_id);
        self.pending_magic
            .retain(|magic| magic.caster_id != player_id);

        // 7) 新图视野内的玩家收到自己的外观（Crystal `CurrentMap.AddObject(this)`，`:4562`）。
        let appearance = self
            .players
            .get(&player_id)
            .map(|player| self.appearance_packet(player));
        if let Some(appearance) = appearance {
            // 新图视野内的玩家收到自己的外观；他们同时把 mover 记进「已看见」集合
            // （Crystal `CurrentMap.AddObject(this)` → `MapObject.Add(HumanObject)`，
            // `PlayerObject.cs:4562`、`HumanObject.cs:8447-8457`）。
            for other_id in self.aoi_ids_in_radius(map_file, landing, VIEW_RANGE, AoiKind::Player) {
                if other_id == player_id {
                    continue;
                }
                if let Some(other) = self.players.get(&other_id) {
                    let _ = other.sender.send(appearance.clone());
                }
                self.visible_players
                    .entry(other_id)
                    .or_default()
                    .insert(player_id);
            }
        }

        // 8) 本人收到 `S.MapChanged`（`PlayerObject.cs:4566-4579`；
        //    字段顺序见 `Shared/ServerPackets.cs:2979-3025`）。
        let header = self.map_header(map_file);
        let map_index = header.map_index;
        self.send_to_player(
            player_id,
            &ServerPacket::MapChanged(MapChanged {
                map_index,
                file_name: map_file.to_owned(),
                title: header.title,
                mini_map: header.mini_map,
                big_map: header.big_map,
                music: header.music,
                lights: header.lights,
                location: landing,
                direction,
                map_dark_light: header.map_dark_light,
                weather: header.weather,
            })
            .encode_frame(),
        );

        // 9) 补齐新图视野（Crystal `GetObjects()`，`PlayerObject.cs:4583` → `:1741-1768`）。
        let visible = self.refresh_own_view(player_id);

        // 9.5) `if (TradePartner != null) TradeCancel();`（`PlayerObject.cs:1477`，在
        //      `Teleport` → `base.Teleport` 之后）：切图/同图传送都中止交易，物品退回背包。
        self.trade_cancel(player_id);

        // 10) 组队：切图后把「我在哪张图」通知队友（Crystal `GroupMemberMapNameChanged`，
        //     `PlayerObject.cs:1568`），再把坐标同步一遍（`GetPlayerLocation`，`:4601`）。
        //     队伍**保留**：Crystal 只有进入 `NoGroup` 地图才会解散队伍
        //     （`PlayerObject.cs:1509-1513`，属于「地图进入规则」，本作未实现，见 `docs/world/group.md`）。
        self.group_member_map_name_changed(player_id);
        self.group_player_location(player_id);
        // 11) 攻城：走进皇宫就算占领（`CapturePalace` 玩法）。
        //     Crystal 在 `CompleteMapMovement` 末尾调 `CheckConquest(true)`（`PlayerObject.cs:4617`），
        //     见 `crate::conquest`。
        self.check_conquest_after_map_change(player_id);

        Ok(MapChangeReport {
            player_id,
            from_map,
            to_map: map_file.to_owned(),
            map_index,
            position: landing,
            direction,
            map_changed,
            first_entry,
            fallback_from,
            visible,
        })
    }

    /// 收集某个玩家视野内的对象帧（Crystal `PlayerObject.GetObjects`，`PlayerObject.cs:1741-1768`）。
    ///
    /// 顺序固定：其它玩家 → 怪物 → NPC → 地面物品；同类里按 `(y, x, 对象 ID)` 排序，
    /// 与 Crystal 逐格扫描（`for y { for x { } }`，`:1743-1767`）的结果一致。
    pub(crate) fn gather_view(&self, viewer: &Player) -> ViewFrames {
        let map = viewer.map.as_str();
        let position = viewer.position;

        // AOI 网格一次查出视野内全部对象（Crystal `GetObjects` 的 33×33 逐格扫描，
        // `PlayerObject.cs:1741-1768`；网格返回顺序已按 `(y, x, 对象 ID)` 排好）。
        let mut players = Vec::new();
        let mut monsters = Vec::new();
        let mut items = Vec::new();
        for entry in self.aoi_in_radius(map, position, VIEW_RANGE) {
            match entry.kind {
                AoiKind::Player => {
                    if entry.object_id == viewer.object_id {
                        continue;
                    }
                    if let Some(other) = self.players.get(&entry.object_id) {
                        players.push((self.appearance_packet(other), other.object_id));
                    }
                }
                AoiKind::Monster => {
                    if let Some(monster) = self.monster(entry.object_id) {
                        monsters
                            .push((crate::world::monster_info_frame(monster), monster.object_id));
                    }
                }
                AoiKind::Item => {
                    if let Some(ground) = self.ground_item(entry.object_id) {
                        items.push((self.ground_item_frame(ground), ground.object_id));
                    }
                }
                // NPC 的可见性还受等级/职业/标记位过滤（`NPCObject.CheckVisible`，
                // `NPCObject.cs:424-462`），交给 `visible_npc_frames` 统一处理。
                AoiKind::Npc => {}
            }
        }

        let npcs = self.visible_npc_frames(viewer);

        ViewFrames {
            players,
            monsters,
            npcs,
            items,
        }
    }

    /// 给某个玩家补齐视野并把可见集合写回（切图、进图共用）。
    fn refresh_own_view(&mut self, player_id: u32) -> ViewCounts {
        let Some(player) = self.players.get(&player_id) else {
            return ViewCounts::default();
        };
        let view = self.gather_view(player);
        for (frame, _) in view
            .players
            .iter()
            .chain(view.monsters.iter())
            .chain(view.npcs.iter())
            .chain(view.items.iter())
        {
            let _ = player.sender.send(frame.clone());
        }
        let counts = view.counts();
        let player_ids = view.player_ids();
        let Some(player) = self.players.get_mut(&player_id) else {
            return counts;
        };
        player.visible_monsters = view.monster_ids();
        player.visible_npcs = view.npc_ids();
        player.visible_items = view.item_ids();
        self.visible_players.insert(player_id, player_ids);
        counts
    }

    /// 走/跑/转身命中传送点时排队切图；返回 `true` 表示**本次移动作废**。
    ///
    /// 对应 `PlayerObject.CheckMovement`（`Server/MirObjects/PlayerObject.cs:4459-4514`）：
    /// 只有真正会切图的条目才返回 `true`；`NeedMove` 条目只记录目的地后继续检查
    /// （Crystal 的 `continue`，`:4495-4500`），走路照常完成。
    pub(crate) fn queue_movement_trigger(&mut self, player_id: u32, cell: Point) -> bool {
        let now = self.now();
        let Some(player) = self.players.get(&player_id) else {
            return false;
        };
        // `if (Envir.Time < MovementTime) return false;`（`:4461`）。
        if now < player.movement_time {
            return false;
        }
        // 已经排队等待结算的传送：重复的走/转身直接作废，避免同一 tick 内切图两次
        // （Crystal 是同步执行，不可能出现「排队两次」）。
        if self
            .pending_map_moves
            .iter()
            .any(|pending| pending.player_id == player_id)
        {
            return true;
        }
        let map = player.map.clone();
        let direction = player.direction;
        // 同格可能有多条传送点（Crystal 逐条判断并在不满足条件时 `continue`，`:4474-4511`）。
        let candidates: Vec<MovementInfo> = self
            .movements_on(&map)
            .iter()
            .filter(|info| info.source == cell)
            .cloned()
            .collect();
        for info in candidates {
            // `NeedHole`（`:4480-4487`）。
            if info.need_hole && !self.has_hole(&map, cell) {
                continue;
            }
            // `ConquestIndex > 0`（`PlayerObject.cs:4489-4493`）：只有**拥有这座城**的行会
            // 成员能过。
            //
            // ```csharp
            // if (info.ConquestIndex > 0) {
            //     if (MyGuild == null || MyGuild.Conquest == null) continue;
            //     if (MyGuild.Conquest.Info.Index != info.ConquestIndex) continue;
            // }
            // ```
            if info.conquest_index > 0 {
                let owner = self
                    .players
                    .get(&player_id)
                    .and_then(|player| player.guild)
                    .and_then(|reference| self.guild_conquest(reference.guild_id))
                    .map(|conquest| conquest.info.index);
                if owner != Some(info.conquest_index) {
                    continue;
                }
            }
            let Some(target_map) = self.resolve_movement_target(&map, &info) else {
                continue;
            };
            // `NeedMove`：只记目的地，交给 NPC 的 `ENTERMAP` 动作（`:4495-4500`、
            // `NPCSegment.cs:4065-4074`）。
            if info.need_move {
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.npc.move_map = Some(target_map);
                    player.npc.move_coord = Some(info.destination);
                }
                continue;
            }
            self.pending_map_moves.push(PendingMapMove {
                player_id,
                map_file: target_map,
                point: info.destination,
                direction,
            });
            return true;
        }
        false
    }

    /// 结算排队中的传送点（`World::tick` 开头调用）。
    pub(crate) fn resolve_pending_map_moves(&mut self) {
        if self.pending_map_moves.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending_map_moves);
        for request in pending {
            if !self.players.contains_key(&request.player_id) {
                // 排队期间下线了。
                continue;
            }
            match self.change_map(
                request.player_id,
                &request.map_file,
                request.point,
                request.direction,
            ) {
                Ok(report) => tracing::debug!(
                    player = report.player_id,
                    from = %report.from_map,
                    to = %report.to_map,
                    x = report.position.x,
                    y = report.position.y,
                    "传送点切图"
                ),
                Err(error) => {
                    tracing::warn!(player = request.player_id, %error, "传送点切图失败");
                }
            }
        }
    }

    /// 传送点目标地图：`MapIndex == 0` 是**同图**传送，其余按主键在 `db.maps` 里找。
    ///
    /// 简化：本仓库的 Crystal 分支用 `Envir.GetMap(info.MapIndex)`（`Envir.cs:4587-4590`），
    /// `0` 就是 0 号地图；本作按任务约定把 0 当作「同图」，见模块文档。
    fn resolve_movement_target(&self, from_map: &str, info: &MovementInfo) -> Option<String> {
        if info.map_index == 0 {
            return Some(from_map.to_owned());
        }
        let data = self.game_data.as_ref()?;
        let target = data.map_info_by_index(info.map_index)?;
        Some(target.file_name.clone())
    }

    /// 校正落点：越界/墙里时夹到地图内并就近找可走格。
    ///
    /// Crystal 在 `!temp.ValidPoint(info.Destination)` 时直接跳过这条传送点
    /// （`Server/MirObjects/PlayerObject.cs:4504`）；本作改成「夹进地图 + 找最近可走格」，
    /// 原因是要满足「越界目标点回退」的需求，见模块文档。
    fn resolve_landing(&self, map_file: &str, point: Point) -> (Point, Option<Point>) {
        let (max_x, max_y) = self.map_bounds(map_file);
        let clamped = Point::new(
            point.x.clamp(0, max_x.max(0)),
            point.y.clamp(0, max_y.max(0)),
        );
        if clamped == point && self.walkable_on(map_file, clamped) {
            return (clamped, None);
        }
        (self.nearest_walkable(map_file, clamped), Some(point))
    }

    /// 从 `center` 开始按半径 1..=8 找第一个可走的格子（找不到就返回 `center` 本身）。
    fn nearest_walkable(&self, map_file: &str, center: Point) -> Point {
        if self.walkable_on(map_file, center) {
            return center;
        }
        for radius in 1i32..=8 {
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs() != radius && dy.abs() != radius {
                        continue;
                    }
                    let cell = Point::new(center.x + dx, center.y + dy);
                    if self.walkable_on(map_file, cell) {
                        return cell;
                    }
                }
            }
        }
        center
    }

    /// `S.MapChanged` 需要的 `MapInfo` 字段（Crystal `PlayerObject.cs:4566-4579`）。
    fn map_header(&self, map_file: &str) -> MapHeader {
        let Some(info) = self
            .game_data
            .as_ref()
            .and_then(|data| data.map_info(map_file))
        else {
            return MapHeader::default();
        };
        MapHeader {
            map_index: info.index,
            title: info.title.clone(),
            mini_map: info.mini_map,
            big_map: info.big_map,
            music: info.music,
            lights: LightSetting::from_value(info.light).unwrap_or_default(),
            map_dark_light: info.map_dark_light,
            weather: WeatherSetting::from_value(info.weather_particles).unwrap_or_default(),
        }
    }

    /// 缓存超限时淘汰「没有对象」的地图（[`World::set_map_cache_limit`]）。
    fn evict_maps(&mut self) {
        if self.map_cache_limit == 0 {
            return;
        }
        while self.maps.len() > self.map_cache_limit {
            let busy: BTreeSet<&str> = self
                .players
                .values()
                .map(|player| player.map.as_str())
                .chain(self.monsters.iter().map(|monster| monster.map.as_str()))
                .chain(self.npcs.iter().map(|npc| npc.map.as_str()))
                .chain(self.ground_items.iter().map(|item| item.map.as_str()))
                .collect();
            let victim = self
                .maps
                .keys()
                .find(|map| !busy.contains(map.as_str()))
                .cloned();
            let Some(victim) = victim else {
                tracing::debug!(
                    maps = self.maps.len(),
                    limit = self.map_cache_limit,
                    "地图缓存全部在用，暂时超过上限"
                );
                break;
            };
            self.maps.remove(&victim);
            tracing::debug!(map = %victim, "地图缓存已满，淘汰");
        }
    }
}

/// `S.MapChanged` 的 `MapInfo` 字段（没有数据库时的默认值全是 0/默认枚举）。
#[derive(Debug, Clone, Default)]
struct MapHeader {
    /// `MapInfo.Index`（`:4568`）。
    map_index: i32,
    /// `MapInfo.Title`（`:4570`）。
    title: String,
    /// `MapInfo.MiniMap`（`:4572`）。
    mini_map: u16,
    /// `MapInfo.BigMap`（`:4573`）。
    big_map: u16,
    /// `MapInfo.Music`（`:4578`）。
    music: u16,
    /// `MapInfo.Light`（`:4574`）。
    lights: LightSetting,
    /// `MapInfo.MapDarkLight`（`:4577`）。
    map_dark_light: u8,
    /// `MapInfo.WeatherParticles`（`:4571`）。
    weather: WeatherSetting,
}
