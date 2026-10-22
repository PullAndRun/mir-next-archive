# 多地图与传送（切图）

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：`Server/MirObjects/PlayerObject.cs`（`CheckMovement` / `CompleteMapMovement`）、
> `Server/MirObjects/HumanObject.cs`（`Walk`/`Run`/`Turn` 里的传送检查、`Teleport`）、
> `Server/MirObjects/MapObject.cs`（`Teleport` 的移除/广播）、
> `Server/MirObjects/NPC/NPCSegment.cs`（`MOVE` / `ENTERMAP` 动作）、
> `Server/MirDatabase/MapInfo.cs` + `Server/MirEnvir/Map.cs`（建图）、
> `Server/MirDatabase/MovementInfo.cs`（传送点数据）、
> `Shared/ServerPackets.cs`（`S.MapChanged`）、`Client/MirScenes/GameScene.cs`（客户端切图）。
>
> 本作实现：服务端 `crates/mir-server/src/maps.rs` + `world.rs`；客户端
> `crates/mir-client/src/net.rs`、`app.rs`；渲染 `crates/mir-render/src/scene.rs`。
> 测试：`crates/mir-server/tests/maps.rs`、`crates/mir-client/tests/map_changed.rs`、
> `crates/mir-client/src/app.rs` 的单元测试。

---

## 1. 总览

| 主题 | Crystal | 本作 |
|---|---|---|
| 地图集合 | `Envir.MapList`，开局把所有 `MapInfo` 建成 `Map`（`MapInfo.cs:197-215`） | [`World`] 里的 `maps: HashMap<文件名, Arc<Map>>`，**第一次进入**某张图才建（`World::load_map` / `World::initialize_map`） |
| 建图内容 | `Map.Load`：读地形 → 注册 `MapRespawn` → 刷 `NPCObject`（`Map.cs:432-514`） | `World::initialize_map` 做同样三件事（`initialize_map` → `spawn_npcs` + `register_spawn_group`） |
| 玩家所在地图 | `PlayerObject.CurrentMap` / `CurrentMapIndex` | `Player::map`（地图文件名） |
| 传送点数据 | `MapInfo.Movements`（`MovementInfo`） | `World::movements_on` 缓存，来自 `GameData::map_info(file).movements` |
| 切图入口 | `PlayerObject.CheckMovement` / `CompleteMapMovement`、`HumanObject.Teleport` | `World::change_map`（同步原语）+ 走/跑/转身命中传送点时的延迟队列 |
| 客户端 | `S.MapChanged` → `MapControl.LoadMap()` | `NetEvent::MapChanged` → `WorldSnapshot::map_reload_request` → `MapScene::reload_map` |

[`World`]: ../../crates/mir-server/src/world.rs

---

## 2. 多地图加载与缓存

### 2.1 地图目录与按需加载

- 服务端配置 `ServerConfig::map_root`（默认 `resource/map`）经 `World::set_map_root` 交给世界；
  地图文件名按 `<map_root>/<FileName>.map` 拼路径（Crystal `Settings.MapPath` + `Info.FileName + ".map"`，
  `Server/MirEnvir/Map.cs:436`）。
- `World::load_map(map_file)`：命中缓存直接返回 `Arc<mir_world::Map>`；否则读盘并缓存。
  读失败的地图名会进 `missing_maps`，**同一张坏图只读一次**，并打一条 `warn`。
- 跨图切图必须有目标地形：`change_map` 在目标图加载失败时返回
  `MapChangeError::MapNotFound`，玩家保持原样（Crystal `Envir.GetMap` 返回 `null` 时
  `CheckMovement` 直接 `continue`，`PlayerObject.cs:4502-4504`）。
- 同图传送不要求地形（单地图兜底世界也能工作）。

### 2.2 首次进入的初始化（NPC / 刷怪点 / 传送点表）

`World::initialize_map(map_file)` 对每张图只成功执行一次，返回 `MapInitReport`：

| 步骤 | 本作 | Crystal |
|---|---|---|
| 地形 | `load_map` | `Map.Load`（`Map.cs:432-514`） |
| 传送点表 | 从 `GameData::map_info(file).movements` 灌入 `World::movements` | `MapInfo.Movements` 一直挂在 `Map.Info` 上 |
| NPC | `spawn_npcs`（按 `NpcInfo.MapIndex` 过滤，坐标必须在地图内） | `Map.cs:490-496` |
| 刷怪点 | `register_spawn_group`（登记后立刻铺满，见 `docs/world/tick-loop.md`） | `Map.cs:473-488` 建 `MapRespawn`，等 `ProcessRespawns` 补怪 |

- 出生地图在服务端启动时初始化一次（`server.rs::attach_game_data`）。
- 玩家进入一张没初始化过的图（登录到存档地图、或切图）时也会初始化。
- `spawn_npcs` 对「同一张图 + 同一个 `NpcInfo.Index`」做了去重，重复初始化不会多出 NPC。
- 传送点表优先用 `World::set_map_movements` 设置的表（测试/GM），数据库不会覆盖。

### 2.3 缓存上限

- `MIR2_MAP_CACHE` 环境变量（`maps::MAP_CACHE_ENV`）设置缓存张数，默认
  `DEFAULT_MAP_CACHE_LIMIT = 8`，`0` = 不限制；也可用 `World::set_map_cache_limit` 运行时改。
- 超过上限时优先淘汰**没有对象**的地图（没有玩家/怪物/NPC/地面物品）；
  全都在用时记一条 `debug` 并暂时超过上限（宁可不淘汰也不让地形消失）。

---

## 3. 传送点（`MovementInfo`）

### 3.1 数据

`MovementInfo`（`Server/MirDatabase/MovementInfo.cs:6-46`，本作 `mir-db::MapInfo::movements`）：

| 字段 | 含义 | 本作语义 |
|---|---|---|
| `map_index` | 目标地图**主键** | `0` = **同图传送**到 `destination`；`> 0` 才在 `db.maps` 里按 `index` 找文件（见 §6.1） |
| `source` | 传送门所在格 | 玩家走/跑/转身**命中这一格**时触发 |
| `destination` | 落点 | 越界/墙里会被夹进地图（§3.4） |
| `need_hole` | 需要洞口 | 要求 `World::has_hole(map, source)`；没开洞就**继续检查下一条** |
| `need_move` | 延迟到 NPC 动作 | 只把目的地记进 `Player::npc.move_map / move_coord`，由 NPC 动作 `ENTERMAP` 切图 |
| `conquest_index` | 攻城区域 | `> 0` 时要求**行会拥有**对应的攻城区（`PlayerObject.cs:4489-4493`，见 [`docs/world/conquest.md`](conquest.md)） |
| `show_on_big_map` / `icon` | 大地图显示 | 只影响进图下发的 `S.NewMapInfo`（`World::map_entry_info`） |

### 3.2 触发规则（`PlayerObject.CheckMovement`，`PlayerObject.cs:4459-4514`）

1. `Envir.Time < MovementTime` → 不检查（`:4461`）。`MovementTime` 在每次切图后设为
   `now + MovementDelay`，`MovementDelay = 2000`（`:113`、`:4564`），即 **2 秒冷却**。
2. 按顺序遍历 `CurrentMap.Info.Movements`，只看 `Source == location`（`:4474-4478`）。
3. `NeedHole` 不满足 → 下一条（`:4480-4487`；Crystal 检查同格的法术对象
   `Spell.DigOutZombie` / `Spell.DigOutArmadillo`，本作用 `World::set_hole` 表示「洞已挖开」）。
4. `ConquestIndex > 0` 且行会不拥有这座城 → 下一条（`:4489-4493`）。
5. `NeedMove` → 记录目的地后**下一条**，走路照常完成（`:4495-4500`）。
6. 其余：解析目标图 → 排队传送。

触发点有三处，全部走同一段逻辑（`World::queue_movement_trigger`）：

| 动作 | Crystal | 本作行为 |
|---|---|---|
| 走 | 落格前 `CheckMovement(location)`（`HumanObject.cs:2466`） | 整次走动作**作废**：不移动、不广播 `ObjectWalk`、不回 `UserLocation`，传送排队 |
| 跑 | 逐格 `CheckMovement(location)`（`HumanObject.cs:2584`） | 整次跑作废，传送排队 |
| 转身 | `CheckMovement(CurrentLocation)`（`PlayerObject.cs:4264-4273`） | 方向已改，**不广播** `ObjectTurn`、不回 `UserLocation`，传送排队 |

### 3.3 延迟执行

- Crystal 上游在 `HumanObject.Process` 里每 tick 调 `MapTime()`，所以传送表现为「下一个 tick 生效」；
  本仓库的 Crystal 分支把 `CheckMovement` 内联进 `Walk`/`Run`/`Turn` 并**同步**执行。
- 本作把「命中」与「切图」拆开：命中时 `World::pending_map_moves` 入队，
  `World::tick` **开头**统一调用 `change_map` 结算。
- 好处：`World::walk` / `World::run` / `World::turn` 不会在内部递归切图，
  包序与状态变更都集中在 `change_map` 一处；测试可以用一次 `world.tick()` 确定性驱动。
- 排队期间玩家下线：结算时跳过。排队期间又命中另一个传送点：不会发生
  （一次走/跑/停都只排队一次，且命中后本次移动作废）。

### 3.4 落点校正

- Crystal：`!temp.ValidPoint(info.Destination)` → 跳过这条传送点（`PlayerObject.cs:4504`）。
- 本作（需求：「越界目标点回退」）：把坐标夹进 `[0, width-1] × [0, height-1]`，
  如果落点是墙/被占则从中心向外按半径 1..=8 找第一个可走格，仍找不到就用夹取后的点。
  `MapChangeReport::fallback_from` 记录原始请求点。
- 简化：Crystal 里 `Teleport` 本身也会拒绝非法落点，所以「回退」是本作的有意扩展。

### 3.5 真实数据与大地图可见性（t93）

- 比奇 `MapInfo.Movements` 在真实 `Server.MirDB` 里是 **72 条**，其中 `ShowOnBigMap`
  **只有 6 条**；
- Crystal `PlayerObject.CheckMapInfo`（`PlayerObject.cs:1009-1056`）**只把 `ShowOnBigMap`
  的传送点传给客户端**（`S.NewMapInfo.Movements` 是大地图图标），本作照抄，行为一致；
  所以脚本客户端「看不到附近传送点坐标」是正常的（Crystal 也不给），playtest 用 GM `@map`
  兜底是对的；
- 广场 40 格内实际有多条可走传送点：**最近 `(326,288) → 地图 4（文件 0108）的 (8,12)`，
  距离 18 格**；
- 测试 `tests/maps.rs`：`bichon_has_a_walkable_movement_point_near_the_plaza`（比奇 ≥20 条、
  `ShowOnBigMap` 只占少数、40 格内有可走点）与
  `player_can_walk_from_the_plaza_into_a_natural_teleport_point`（真实 `0.map` + 真实传送点表，
  BFS 34 步走到传送点 → 下一 tick 换图到 `0108 @ (8,12)` 并收到 `S.MapChanged`）。
  即「玩家从比奇广场自然走进传送点换图」在服务端成立。

---

## 4. 切图流程与包序（`World::change_map`）

移植 `PlayerObject.CompleteMapMovement`（`PlayerObject.cs:4515-4618`），顺序如下：

| # | 收件人 | 包 / 动作 | Crystal |
|---|---|---|---|
| 0 | — | 校验玩家存在；跨图时加载目标地形；首次进入初始化（刷 NPC/刷怪点）；落点校正 | `:4502-4504`、`Map.cs:473-496` |
| 1 | 旧图视野内玩家 | `S.ObjectRemove`（离开旧图） | `:4506-4507` |
| 2 | — | `player.map/position/direction` 更新；`MovementTime = now + 2000`；清空 `visible_monsters/npcs/items`；关闭 NPC 对话 | `:4558-4564` |
| 3 | — | 丢弃以该玩家为发起者/目标的延迟伤害、以及该玩家施放的延迟魔法 | 见 §6.2 |
| 4 | 新图视野内玩家 | `S.ObjectPlayer`（自己的外观） | `CurrentMap.AddObject(this)`，`:4562` |
| 5 | 本人 | `S.MapChanged` | `:4566-4579` |
| 6 | 本人 | 视野内对象：其它玩家 `ObjectPlayer` → 怪物 `ObjectMonster` → NPC `ObjectNPC` → 地面物品 `ObjectItem`/`ObjectGold` | `GetObjects()`，`:4583` → `:1741-1768` |

- 第 6 步的**顺序固定**，与 `World::enter` 共用 `World::gather_view`；同类里按 `(y, x, objectID)`
  排序，对应 Crystal 逐格 `for y { for x { } }` 扫描（`:1743-1767`）。
- `S.MapChanged` 的字段顺序：`MapIndex, FileName, Title, MiniMap, BigMap, Lights, Location,
  Direction, MapDarkLight, Music, Weather`（`Shared/ServerPackets.cs:2997-3025`）；
  本作全部填自 `MapInfo`（`MapIndex/Title/MiniMap/BigMap/Light/Music/MapDarkLight/WeatherParticles`）。
- 同图传送也会发 `S.MapChanged`（Crystal `HumanObject.Teleport`，`:6985-7031`），
  客户端的行为是 `MapControl.ResetMap()` 而不是换文件（`GameScene.cs:3972-3988`）。
- 切图后角色所在的地图名会随角色一起落盘（`Session::leave_world` 回写 `character.map`），
  重登时仍在切图后的位置。

---

## 5. NPC 脚本切图动作

| 动作 | Crystal | 本作 |
|---|---|---|
| `MOVE <地图> <x> <y>` | 解析 `NPCSegment.cs:452-459`，执行 `:3075-3088` | `World::run_map_action`：同步调用 `change_map` |
| `MAPMOVE <地图> <x> <y>` | 只在 GM 聊天命令里出现（`PlayerObject.cs:2872-2913`） | **本作扩展**：`MOVE` 的别名（需求指定的写法） |
| `ENTERMAP` | 解析 `NPCSegment.cs:993-995`，执行 `:4065-4074` | 消费 `NeedMove` 传送点记下的目的地；没有记录时什么都不做 |

- 地图参数：**先按地图文件名查**（Crystal 的写法，`Envir.GetMapByNameAndInstance`，
  `Envir.cs:4607-4614`；可带 `.map` 后缀），查不到再按 `MapInfo.Index` 查（**本作扩展**，
  方便写 `MOVE 20 10 10`）。顺序不能反：Mir2 的地图文件名全是数字，按主键优先会把
  `MOVE 1 ...` 解析成错误的地图。
- 坐标缺失时本作**忽略该行**；Crystal 会改成在目标图随机传送（`NPCSegment.cs:3085-3086`），
  本作没有实现随机传送。
- `mir-db` 的动作解析只认得少数关键字（`crates/mir-db/src/npc_script.rs::parse_action`），
  其余落到 `NpcActionKind::Unsupported`；`mir-server` 在 `run_npc_action` 里再按
  `action.source` 的关键字补上这三个动作，因此**不需要改 `mir-db`**。

---

## 6. 客户端切图

### 6.1 包 → 快照 → 重载请求

- `WorldSnapshot::apply_packet` 处理 `S.MapChanged`（`net.rs`）：
  更新 `map`（`map_index/file_name/title`）、**清空 `entities`**、关闭 NPC 对话/商店、
  把 `self_player.position/direction` 设为 `Location/Direction`。
- 网络线程在**同一批包**里先发 `NetEvent::Snapshot`，再发
  `NetEvent::MapChanged { map_index, position }`，保证 UI 收到事件时快照已是新地图。
- `WorldSnapshot::map_reload_request(map_dir)` 产出 `MapReloadRequest`：
  - `file_name` 取 `S.MapChanged.FileName`；为空时退回 `map_index.to_string()`
    （「地图号 == 文件名」的老约定）；
  - `path = <map_dir>/<file_name>.map`（Crystal `Path.Combine(Settings.MapPath, FileName + ".map")`，
    `GameScene.cs:3979`）；
  - `position` 取快照里自己的坐标（相机要重置到这里）。
- `MapScene::reload_map(path)` 只重读地形并清空解码缓存，保留瓦片/实体图库
  （Crystal 切图时会 `MapControl.LoadMap()` 重建地图纹理，`GameScene.cs:3987`）。
- `app.rs::apply_map_changed`：重载成功后打印新地图并 `center = clamp(落点)`；
  **缺文件/损坏时保留旧地图**并打印警告（需求：保留旧图；黑屏占位更糟）。
- 客户端地图目录来自命令行 `--map-dir`（默认 `resource/map`）。

### 6.2 客户端没有做的事

- 没有 Crystal 的 `MapControl.InputDelay = CMain.Time + 400`（`GameScene.cs:4006`）：
  本作客户端没有动作队列，按键直接发包。
- 没有 `S.NewMapInfo` 的切图重发：NPC 列表/大地图图标只在登录时下发一次，
  切图后靠 `ObjectNPC`/`ObjectMonster` 增量补齐。

---

## 7. 与 Crystal 的差异 / Crystal 侧的异常行为

1. **`MapIndex == 0` 的语义**：本仓库的 Crystal 分支是 `Envir.GetMap(info.MapIndex)`
   （`Envir.cs:4587-4590`），`0` 就是 0 号地图（比奇省）；本作按任务约定把 `0` 当作
   「同图传送」，`> 0` 才按主键找图。
2. **`NeedMove` 的语义**：Crystal 不是「玩家正在移动」，而是「把目的地存进 NPCData，
   等 NPC 的 `ENTERMAP` 动作」（`:4495-4500` 的 `continue`）。本作照抄，并补上了
   `ENTERMAP` 动作（见 §5）。
3. **执行时机**：Crystal 上游是每 tick 的 `MapTime()`（延迟一个 tick）；本仓库的 Crystal
   分支是内联同步执行。本作采用「排队 + 下一个 `World::tick` 结算」。
4. **落点越界**：Crystal 跳过该传送点；本作夹进地图并回退到最近可走格（§3.4）。
5. **`NeedHole` 的判定**：Crystal 找同格的 `SpellObject`（`Spell.DigOutZombie` /
   `Spell.DigOutArmadillo`）；本作没有挖洞技能，用 `World::set_hole` 的集合表示。
6. **`ConquestIndex`**：Crystal 要求行会**拥有**对应攻城区；本作照抄
   （[`World::guild_conquest`]，见 `docs/world/conquest.md`）。
7. **延迟伤害/魔法**：Crystal 的 `ActionList` 挂在对象/地图上，切图不会丢；
   本作队列只记对象 ID 与坐标，所以切图时丢弃「以该玩家为发起者/目标」的延迟伤害、
   以及该玩家施放的延迟魔法，避免在新图上按旧坐标结算。
8. **`Crystal` 分支本身的特点**：`MovementDelay = 2000` 只是切图后的**冷却**，
   不是执行延迟；`MovementTime` 只在这一处被写（`PlayerObject.cs:4564`），
   `CheckMovement` 第一行读它（`:4461`）。本作照抄这个 2 秒冷却。
9. **客户端 `MapChanged` 的 `MapInformation`**：Crystal 客户端切图时不会重新收
   `S.MapInformation`，只收 `S.MapChanged`；本作客户端同样只靠 `S.MapChanged` 的
   `FileName/Title` 等信息更新 HUD。

---

## 8. 未做

- 大地图（`S.NewMapInfo`）不随切图刷新；小地图/大地图界面本身未实现。
- 随机传送（`TeleportRandom`）、`TeleportRing`、`S.Teleport` 等其它切图入口未实现；
  **回城 `C.TownRevive` 已实现**（优先角色绑定点，见 `docs/world/combat.md` §10.2）。
- 地图实例（`GetMapByNameAndInstance`）：同一文件名多实例没有实现，`MOVE` 的
  instance 参数会被忽略。
- 安全区/绑定坐标（`SafeZoneInfo`、`BindLocation`）：`characters.bind_map_index/bind_x/bind_y`
  已双向映射，回城优先绑定点、无效/未设置回退 `ServerConfig.start_map/start_point`；
  **红名 `PKTown` 未实现**（数据里没有 PKTown 配置）；切图本身不会自动更新绑定（Crystal 同）。
- 门的开关（`CheckDoorOpen`）与挖洞技能：`need_hole` 只认 `World::set_hole`。
- `RequiredGroup` 等其它地图进入条件（攻城对传送点的限制已实现，见 `docs/world/conquest.md`）。
- 跨图延迟魔法/伤害的「跟随对象」语义（见 §7.7）。
- 客户端切图时的 400ms 输入延迟与 `MapControl` 纹理重建策略（见 §6.2）。
- 地图缓存上限的 LRU（本作是「先淘汰空闲图」，不维护访问顺序）。

---

## 9. 测试

| 用例 | 覆盖 |
|---|---|
| `same_map_teleport_moves_the_player_and_sends_map_changed` | 同图传送、`S.MapChanged` 字段、`MapChangeReport` |
| `cross_map_change_removes_from_old_map_and_reveals_the_new_map` | 旧图 `ObjectRemove`、新图 `ObjectPlayer`/`ObjectMonster` |
| `walking_onto_a_movement_source_teleports_on_the_next_tick` | 走格命中、移动作废、延迟到下一 tick |
| `turning_on_a_movement_source_counts_as_movement` | 转身命中（`CheckMovement(CurrentLocation)`） |
| `need_hole_movement_point_requires_an_open_hole` | `NeedHole` 条件 + `set_hole` |
| `need_move_movement_point_records_the_destination_for_entermap` | `NeedMove` + `ENTERMAP` |
| `first_entry_registers_respawns_and_spawns_monsters_only_once` | 首次进入注册刷怪点、只注册一次 |
| `ground_items_and_npcs_are_visible_after_map_change` | 切图后 NPC/地面物品可见 |
| `map_change_clears_the_old_visibility_sets` | 旧图可见集合清理 |
| `two_map_changes_in_a_row_do_not_leak_monsters` | 两次切图怪物数量不翻倍 |
| `npc_move_actions_teleport_by_file_name_and_by_index` | NPC `MAPMOVE`/`MOVE`（文件名与主键两种写法） |
| `out_of_bounds_destination_falls_back_inside_the_map` | 越界落点回退 |
| `movement_delay_blocks_an_immediate_retrigger` | 2 秒冷却 |
| `movement_points_come_from_the_database_on_first_entry` | 数据库传送点灌入 |
| `unknown_map_change_reports_an_error_and_keeps_the_player_in_place` | 目标图缺失 |
| `map_changed_packet_carries_the_full_map_info` | `S.MapChanged` 全部字段 |
| `real_database_has_movement_points_that_resolve_to_maps` | 真实数据冒烟（Jev v88：地图 `3`（MongchonProvince）77 个传送点） |
| `real_map_files_load_and_change_map_moves_between_them` | 真实 `.map` 地形：`0`（347,188）→ `0122`（13,40），51×55 |
| 客户端 `map_changed_*` / `app::tests::map_changed_*` | 快照清空、位置/地图号更新、重载请求与缺文件回退 |
