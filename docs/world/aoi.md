# AOI 网格、视野半径与门状态机

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：
> * `Server/MirEnvir/Map.cs`（`Cells` `:26`、`DoorIndex/Doors` `:28-29`、`AddDoor/OpenDoor`
>   `:50-71`、`CheckDoorOpen` `:646-651`、`Process` 的关门 `:653-665`、`AddObject/RemoveObject`
>   `:2361-2390`、`Broadcast` `:2437-2448`、`class Cell` `:2475-2530`）
> * `Server/MirObjects/MapObject.cs`（`Add/Remove` `:282-308`、`Spawned` `:339-357`）
> * `Server/MirObjects/PlayerObject.cs`（`GetObjects` `:1741-1768`、`Opendoor` `:7818-7826`）
> * `Server/MirObjects/HumanObject.cs`（`Walk` 的格子搬运 `:2465-2510`、`Run` `:2559-2602`、
>   `RemoveObjects/AddObjects` 条带差分 `:7864-8152`）
> * `Shared/Globals.cs:37`（`DataRange = 16`）、`Shared/Functions/Functions.cs:71-74`（`InRange`）
> * `Server/MirNetwork/MirConnection.cs:2188-2192`（`C.Opendoor` 分发）
>
> 本作实现：
> * `crates/mir-server/src/aoi.rs`（`AoiGrid` / `AoiKind` / `AoiEntry`，桶网格 + 查询 + 单元测试）
> * `crates/mir-server/src/world.rs`（`VIEW_RANGE = 16`、AOI 维护点、`visible_players` 差分、
>   `open_door` / `tick_doors` / `door_is_open`、`MoveError::DoorClosed`）
> * `crates/mir-server/src/{maps.rs,items.rs,npc.rs,monster.rs}`（视野/拾取/对话/索敌改走索引）
> * `crates/mir-world/src/map.rs`（地图文件的门表访问器 `doors` / `door_index`）
> * `crates/mir-server/tests/aoi.rs`（端到端：16 格边界、进出视野包序、门开关与自动关闭）
>
> **`mir-protocol` 没有改动**：`S.Opendoor{DoorIndex, Close}` 已存在（Crystal
> `Shared/ServerPackets.cs` 的 `Opendoor`；客户端 `GameScene.cs` 处理开门/关门）。

---

## 1. 总览

| 主题 | Crystal | 本作 |
|---|---|---|
| 空间索引 | 每张图 `Map.Cells[x, y]`，每格一个 `Cell.Objects`（`Map.cs:2475-2530`） | `World::aoi: HashMap<地图名, AoiGrid>`；每 8×8 地块一个桶 |
| 查询方式 | `PlayerObject.GetObjects` 扫 `33×33` 个格子（`PlayerObject.cs:1741-1768`） | `AoiGrid::query_radius` 扩到桶边界后精确过滤，最多 6×6 个桶 |
| 视野半径 | `Globals.DataRange = 16`（切比雪夫，`Globals.cs:37`） | `VIEW_RANGE = 16` |
| 查询排序 | `for y for x for cell.Objects` | 按 `(y, x, 对象 ID)` 排序（同序） |
| 玩家进出视野 | `AddObjects/RemoveObjects` 条带差分（`HumanObject.cs:7864-8152`） | `World::visible_players` 保存上一轮集合，双向差分 |
| 门 | `Map.Doors[i].DoorState/LastTick`（`Map.cs:61-71`） | `World::doors: HashMap<地图, HashMap<门号, DoorRuntime>>` |
| 门自动关闭 | `LastTick + 5000 < Envir.Time`（`Map.cs:659`） | `DOOR_OPEN_MS = 5_000`，`World::tick` 调 `tick_doors` |
| 门状态持久化 | **没有**（重启全关） | **没有**（重启全关） |

**为什么需要 AOI**：旧实现每次广播/索敌都遍历全服玩家与怪物（`O(全部对象)`）；视野 10 改 16 后
同屏对象约 2.5 倍，全量扫描会随在线人数线性变慢。网格把「查询附近对象」降到
`O(相邻桶 + 命中对象)`，且保持**结果与发包顺序**不变。

---

## 2. 视野半径：`DataRange = 16`

Crystal 的 `Globals.DataRange = 16`（`Shared/Globals.cs:37`，注释写着 `//Was 24`），
所有玩家可见性/广播/交易/对话都用它：

* `PlayerObject.GetObjects` 扫描 `(2 * DataRange + 1)² = 33 × 33` 格（`PlayerObject.cs:1741-1768`）；
* `Map.Broadcast(packet, location)` 对半径内玩家逐个 `Enqueue`（`Map.cs:2437-2448`）；
* 交易/仓库/市场 NPC 的距离检查也是同一个 `DataRange`（见 `docs/world/trade.md`、`storage.md`）。

本作把原来的 10 改为 `VIEW_RANGE = 16`，并让以下路径统一用它：

> **常量核对（与代码一致）**：`VIEW_RANGE: i32 = 16`（`world.rs:86`）、
> `AOI_CELL_SIZE: i32 = 8`（`aoi.rs:60`）、`DOOR_OPEN_MS: i64 = 5_000`（`world.rs:182`）。
> 门关闭条件是 `last_tick + DOOR_OPEN_MS < now`（严格小于），与 Crystal `Map.cs:659` 一致。

| 路径 | 位置 |
|---|---|
| 实体广播（走/跑/攻击/死亡/聊天…） | `World::broadcast` |
| 玩家进图时的初始视野 | `World::gather_view` / `World::enter` |
| 玩家移动后的玩家/怪物/地面物品/视野差分 | `World::sync_player_players` / `sync_player_monsters` / `sync_player_ground_items` |
| 怪物索敌 | `Monster::find_target` 走 `aoi_ids_in_radius(..., VIEW_RANGE)` |
| NPC 对话/市场距离 | `npc::NPC_TALK_RANGE = 16`（与 `DataRange` 同值，但语义独立） |

> **切比雪夫距离**：`in_range` 是 `|dx| <= r && |dy| <= r`（`Functions.cs:71-74`），
> 不是欧几里得；四正方向与四对角在 16 格处同时可见/不可见。

---

## 3. AOI 索引结构

`crates/mir-server/src/aoi.rs`：

```rust
pub const AOI_CELL_SIZE: i32 = 8;      // 一个桶 = 8×8 地块

pub enum AoiKind { Player, Monster, Npc, Item }

pub struct AoiEntry { object_id: u32, kind: AoiKind, position: Point }

pub struct AoiGrid {
    cell_size: i32,
    buckets: HashMap<(i32, i32), Vec<u32>>,   // 桶 → 对象 ID（插入顺序）
    entries: HashMap<(AoiKind, u32), AoiEntry>, // 对象 → 记录（O(1) 找旧位置）
}
```

要点：

* **对象键是 `(kind, object_id)`**：玩家/怪物/NPC/地面物品各自有独立 ID 空间，数值可能重复；
  早期版本只按 `object_id` 会在「玩家 1 与怪物 1」共存时互相覆盖，测试
  `different_kinds_with_same_id_do_not_overwrite` 锁住这一点。
* **插入/移动/删除**：`insert` / `move_to` / `remove`；同桶内移动只改记录，跨桶才搬一次。
* **查询**：
  * `query_rect(min, max)`：闭区间矩形；
  * `query_radius(center, r)`：切比雪夫半径（内部就是 `query_rect`）；
  * `query_cell(cell)`：单格（占格/拾取/对话）。
* **结果排序**：`(y, x, object_id)` 升序，等价于 Crystal 的 `for y for x for cell.Objects`
  扫描顺序，保证同 seed 下发包顺序可回放。
* **负坐标**：用 `div_euclid` 分桶，`-1` 与 `0` 不会落进同一个桶（`negative_coordinates_bucket_correctly`）。

复杂度：Crystal 的 33×33 格扫描每次访问 1089 个格子；本作 8×8 桶最多访问 6×6 = 36 个桶
（33 / 8 ≈ 4.125，跨桶边界最多 6 列），桶内只取命中对象。

---

## 4. 索引维护点（漏一个就会出现「幽灵对象」）

| 事件 | 实际调用链 | 位置 |
|---|---|---|
| 玩家进入世界 | `enter` → `aoi_insert(Player)` | `world.rs` |
| 玩家离开世界 | `leave` → `aoi_remove(Player)` | `world.rs` |
| 玩家走一步 | `walk` → `aoi_move(Player, to)` | `world.rs` |
| 玩家跑 | `run` → `aoi_move(Player, destination)` | `world.rs` |
| 玩家切图/传送 | `change_map` → 旧图 `aoi_remove(Player)` + 新图 `aoi_insert(Player)`（不是 `aoi_move`） | `maps.rs:451` |
| 怪物刷出 | `spawn_monster` → `aoi_insert(Monster)` | `world.rs` |
| 怪物移动 | `Monster` 移动路径 → `aoi_move(Monster)` | `monster.rs` |
| 怪物消失/尸体回收 | `forget_monster` → `aoi_remove(Monster)` | `world.rs` |
| NPC 刷出 | `spawn_npc` / `spawn_npcs` → `aoi_insert(Npc)` | `npc.rs` |
| 地面物品生成 | `spawn_ground_item` → `aoi_insert(Item)` | `items.rs` |
| 地面物品移除/过期 | `remove_ground_item`（含 `tick_ground_items` 调用）→ `aoi_remove(Item)` | `items.rs` |
| 测试/GM 瞬移 | `place_player` / `place_monster` → `aoi_remove(old)` + `aoi_insert(new)` | `world.rs` |
| 地图缓存淘汰 | `evict_maps` **不操作 AOI**：只移除「没有玩家/怪物/NPC/地面物品」的地图数据，AOI 条目本来就是空桶 | `maps.rs` |

`World::aoi_insert` / `aoi_remove` / `aoi_move` 是唯一的三个写入口（都在 `world.rs`）：
新增对象用 `insert`，删除对象用 `remove`，同一张图内改坐标用 `move`，跨图用
`remove` + `insert`。

`World::place_player` / `place_monster` 是**测试/GM 瞬移**入口（内部 remove → 改坐标 →
insert，跨图也安全）；正常玩家走/跑走 `aoi_move`，切图走 `change_map` 的
`remove` + `insert`，都不会调用 `place_*`。

---

## 5. 查询替换点

| 场景 | 旧实现 | 新实现 |
|---|---|---|
| 广播 | 遍历同图全部玩家 | `aoi_ids_in_radius(..., Player)` → 精确距离过滤 |
| 玩家进图视野 | 遍历同图玩家/怪物/NPC/物品 | `gather_view` 走 `aoi_in_radius`（按 `AoiKind` 分拣） |
| 玩家移动差分 | 全量重算 | 新旧视窗半径查询 + `visible_players` 集合差 |
| 聊天 | 遍历全服玩家 | `broadcast` → AOI |
| 刷怪视野 | 遍历全部怪物 | `sync_player_monsters` → `AoiKind::Monster` 半径查询 |
| 地面物品视野 | 遍历全部地面物品 | `sync_player_ground_items` → `AoiKind::Item` |
| NPC 可见性 | 遍历全部 NPC | `visible_npc_frames` → `AoiKind::Npc` |
| 怪物索敌 | 遍历全部玩家 | `Monster::find_target` → `AoiKind::Player` 半径查询 |
| 占格判定 | 遍历全部玩家/怪物/NPC | `check_cell` → `aoi_in_cell(map, cell)` |
| 地面物品拾取 | 遍历全部物品 | `aoi_in_cell` / `aoi_ids_in_radius` |

`broadcast` 的接收者来自 `aoi_ids_in_radius(..., AoiKind::Player)`：AOI 半径查询本身按
`(y, x, 对象 ID)` 排序（`aoi.rs:267`），再按调用方传入的 `exclude` 与发起者 `actor` 过滤
（`world.rs:5568-5570`）。**「空间序」不是「对象 ID 序」**：同一格有多个对象时，两者可能不同；
也不能把这里的顺序说成 Crystal 的 `Cell.Objects` 插入序。发起者本人是否收到由 `except`/`actor`
控制（例如走/跑要把 `ObjectWalk` 发给别人，`UserLocation` 单独回本人）。
开启 `broadcast_batching` 时，每个接收者收到的包按 `append` 顺序拼进同一缓冲
（`world.rs:5577-5582`），再由 `flush_broadcast_batch`（`world.rs:5652`）在 tick 内统一 flush。
因此「某接收者收到的完整序列」以该缓冲的解码序为准，**不能以跨 channel 取包的时刻推断全局发送序**；
Crystal 的 `Map.Broadcast` 对 `Players` 逆序直发（`Map.cs:2437-2448`），没有这一批次层。

---

## 6. 进出视野差分

Crystal 在玩家移动时只处理「新旧 33×33 视窗的条带」：`RemoveObjects` 先发
`S.ObjectRemove`，`AddObjects` 再发外观包（`HumanObject.cs:7864-8152`）。本作：

1. 用 `World::visible_players: HashMap<玩家 ID, BTreeSet<对象 ID>>` 保存上一轮可见玩家集合；
2. 走/跑/切图后查询新半径内的玩家集合；
3. `removals = previous − visible`，`adds = visible − previous`；
4. **先**给本人发 `removals` 的 `S.ObjectRemove`，**再**发 `adds` 的外观包；
5. 对每个被移除/新增的对方也做**双向**维护（对方各收一条 `ObjectRemove`/`S.ObjectPlayer`），
   并同步对方 `visible_players` 集合。

初始进图 `enter` 不走差分：先给新玩家下发视野内全部对象，再把自己的外观发给半径内玩家
（Crystal `MapObject.Add` → `GetInfo()`，`MapObject.cs:286-308`）。

> **顺序契约（玩家差分）**：先移除、后新增；同一批内的顺序是**对象 ID 升序**——`previous` 与
> `visible` 都是 `BTreeSet`，用 `difference` 做集合差（`world.rs:5287-5293`），所以同一批
> `ObjectRemove`/外观包按 ID 升序发出，**不按 `(y, x, id)` 空间序**。空间序只用于 AOI 半径
> 查询的结果（`gather_view`、广播、怪物/NPC/物品视野），两者不要混写。初始进图不走差分，
> 首帧按「玩家 → 怪物 → NPC → 物品」分组下发，见 §8 第 8 条。这样客户端不会先看到「新对象」
> 再收到「旧对象还在」的错误中间态。

---

## 7. 门状态机

### 7.1 数据来源

地图文件每格可以带 `door_index`（`mir_world::MapCell::door_index`）；`mir-world` 的 `Map::doors`
汇总成 `(index, location)` 列表。世界首次进入地图时 `register_map_doors` 按 Crystal
`Map.Load` 的 `AddDoor`（`Map.cs:50-59`、`:143`）登记，初始状态全部 `Closed`。

```rust
struct DoorRuntime {
    location: Point,       // 第一次出现的格子（Crystal Door.Location）
    state: DoorState,      // Closed / Open
    last_tick: i64,        // 开门的时刻；`tick_doors` 用 last_tick + 5000 < now 关
}
```

### 7.2 开 / 关 / 阻挡

* **开**：`C.Opendoor{DoorIndex}` → `World::open_door`（对照 `PlayerObject.Opendoor`，
  `PlayerObject.cs:7818-7826`）：
  * 本人收一条 `S.Opendoor{Close=false}`；
  * 同图视野内其它玩家收一条 `S.Opendoor{Close=false}`（`broadcast`，排除本人）；
  * 记录 `last_tick = now`。
* **自动关**：`World::tick` 每 tick 调 `tick_doors`；`state == Open && last_tick + 5000 < now`
  时置 `Closed` 并向门所在位置半径内广播 `S.Opendoor{Close=true}`（Crystal `Map.Process`，
  `Map.cs:653-665`）。
* **阻挡**：`check_cell` 在「边界 → 地形可走」之后检查门：
  格子的 `door_index` 存在且 `door_is_open == false` → `MoveError::DoorClosed{x,y,index}`，
  走/跑都会失败；失败时仍然回 `S.UserLocation`（与 Crystal 的「回拉」一致）。
  没登记门的格子视为可通过（`door_is_open` 返回 `true`）。

### 7.3 与 `need_hole` 的「洞」不是一回事

* **门**：`door_index` 指向 `Map.Doors`，有开关状态，挡住走/跑，5 秒后自动关；
* **洞**：`MovementInfo.need_hole`（`docs/world/maps.md`）是传送点条件，要求目标格
  「可走」，与门状态无关，也不参与 `S.Opendoor`。

两者不要合并成同一个结构，也不要让「洞」的判定读取门状态。

### 7.4 持久化

门状态**不落盘**：Crystal 的 `Map.Doors` 是运行期数据，重启后全关；本作一致。
地图文件里的 `door_index` 是静态的，随 `.map` 加载。

---

## 8. 与 Crystal 的**有意**差异

1. **桶大小**：Crystal 的 `Cell` 是 1×1 地块；本作用 8×8 桶，查询结果不变（精确距离过滤），
   只是减少了桶访问次数。
2. **对象键**：本作按 `(AoiKind, object_id)` 区分玩家/怪物/NPC/物品；Crystal 的 `Cell.Objects`
   是一个统一列表，靠 `MapObject` 类型判断。两者结果等价。
3. **玩家进出视野**：本作用 `visible_players` 集合显式保存上一轮；Crystal 用
   `RemoveObjects/AddObjects` 的条带扫描（`HumanObject.cs:7864-8152`），效果相同。
4. **门**：Crystal 的 `Map.Process` 每轮检查所有门；本作 `tick_doors` 也在 `World::tick`
   里每 tick 检查，门表按**地图名**存一份（同名地图实例共享门状态），与 Crystal
   「每个 `Map` 对象一份 `Doors`」在「一张地图只有一个实例」时等价；多实例同名地图属于
   未支持场景（本作地图缓存按名字唯一）。
5. **开门广播**：Crystal `Broadcast` 会给半径内所有玩家（可能包括本人）发；本作先给本人发一条，
   再用 `broadcast(..., except=本人)` 发给其它人，避免本人收到两条。
6. **查询排序**：本作显式按 `(y, x, id)` 排序，Crystal 依赖 `Cell.Objects` 的插入顺序；
   在「同格多对象」时可能不同，但本作保证确定性可回放。
7. **门状态不落盘**：两者一致，但本作把它写在 `World` 字段里，地图缓存淘汰时整表清掉；
   Crystal 的地图对象销毁时同样丢失。
8. **首帧类型分组**：玩家首次进图时按 **玩家 → 怪物 → NPC → 物品** 四组依次下发完整首帧
   （发送点 `world.rs:1882-1893`，收集自 `maps.rs:609` 的 `gather_view`）；组内玩家/怪物/物品
   按 AOI 查询的 `(y, x, id)` 序，NPC 组单独按 NPC 对象 ID 升序（`npc.rs:921-929`）。
   Crystal 的 `GetObjects` 是 `for y { for x { Cell.Objects } }` 逐格追加
   （`PlayerObject.cs:1741-1768`），同格多对象依赖 `Cell.Objects` 的插入序；因此跨类型的
   全局对象序列、同格多对象顺序两者都不保证一致。客户端只应依赖「四类各自完整、类型内对象
   不重复」，不要据此推断 Crystal 的格子插入序。

## 8.9 NPC 回收原语与生产接线状态

> 本节只记录 R56 的只读核查：**原语已存在，生产路径没有调用者**。原始命令与输出见
> `target/round56/docs/attempt-1/00-evidence-greps.log`（HEAD `e3b365f`）。

1. **① 容器原语存在**：`World::despawn_npc`（`crates/mir-server/src/npc.rs:804-811`；doc 段从
   `:804` 起写明「当前无生产调用者」，函数签名在 `:811`）一次完成四要素：容器移除
   （`:829` `self.npcs.retain(|npc| npc.object_id != object_id)`）、AOI 桶移除
   （`:828` `self.aoi_remove(&npc.map, AoiKind::Npc, object_id)`）、半径内广播
   `S.ObjectRemove`（`:817` `object_remove_frame`）、关系清理（`:833` `visible_npcs.remove`、
   `:835` `npc.close()`）。四要素与 Crystal `Server/MirEnvir/Map.cs:2375-2383` 对齐：
   `:2378` `NPCs.Remove((NPCObject)ob)`（容器）、`:2382` `GetCell(ob.CurrentLocation).Remove(ob)`
   （格子索引）；本作用 AOI 桶代替逐格 `Cell.Objects`。
2. **② Crystal 运行期不调用**：`github/Crystal/Server` 全仓库 `RemoveObject(` 共 **26** 处，
   其中实参传 `NPCObject` 的 **0** 处（调用者只有玩家/怪物/英雄/宠物/技能/物品/征服物）；
   `Envir.Remove(NPCInfo)`（`Server/MirEnvir/Envir.cs:4367-4371`）只做
   `NPCInfoList.Remove(info)`，仅定义、无服务端调用者。
3. **③ 本作无地图卸载生命周期**：`grep -rn "fn unload\|fn remove_map\|fn despawn_npc" crates/mir-server/src/`
   只命中 `npc.rs:811` 的 `despawn_npc` 定义；`initialize_map` 以 `initialized_maps` 幂等早退
   （`crates/mir-server/src/maps.rs:410-412`，判据在 `:411`），重复进图不会重建或回收 NPC。
4. **④ 淘汰路径排除有 NPC 的图**：`evict_maps`（`crates/mir-server/src/maps.rs:871-900`）的
   busy 集合在 `:881` 显式含 `self.npcs.iter().map(|npc| npc.map.as_str())`；有 NPC 的图不进入
   淘汰候选，找不到 victim 时在 `:889-895` `break`，不会留下悬空 NPC。
5. **⑤ 动态刷出只在测试/GM 路径**：`grep -rn "\.spawn_npc(" crates/` 只命中测试
   `crates/mir-server/tests/conquest_e2e.rs:628` 与 `crates/mir-server/tests/market_persistence.rs:118`；
   生产刷出只发生在启动/首次进图的 `spawn_npcs`（`maps.rs:434` 由 `initialize_map` 调用），
   没有「运行期新增 NPC」可与回收配对。

结论：**AOI-5 整体维持 PARTIAL；仅子项“无回收原语”已 CLOSED；生产零调用者接线待 R56**。
`despawn_npc` 原语保留，唯一调用者是 `crates/mir-server/tests/round55_npc_recycle.rs`
（`:141`、`:198`、`:208`、`:222`、`:230`），生产代码 0 调用。

## 9. 测试清单

`crates/mir-server/src/aoi.rs` 内 7 个单元测试：

| # | 用例 | 覆盖点 |
|---|---|---|
| 1 | `insert_query_remove_roundtrip` | 插入/单格查询/删除、`len`/`contains`/`position_of` |
| 2 | `insert_same_id_replaces_old_bucket` | 同 ID 重复插入替换旧桶 |
| 3 | `different_kinds_with_same_id_do_not_overwrite` | `(kind, id)` 复合键 |
| 4 | `move_across_buckets_keeps_single_entry` | 跨桶/同桶移动、未知 ID |
| 5 | `radius_query_is_chebyshev_and_orders_by_y_x_id` | 切比雪夫半径与排序 |
| 6 | `negative_coordinates_bucket_correctly` | 负坐标分桶 |
| 7 | `random_operations_match_bruteforce_scan` | 200 对象 × 1000 步随机增删移动，与暴力扫描逐步对拍 |

`crates/mir-server/tests/aoi.rs`（世界级端到端 **13** 个：8 个既有 + 5 个 R54 五路由验收例）：

| # | 用例 | 覆盖点 |
|---|---|---|
| 1 | `view_radius_is_crystal_data_range_sixteen` | 距离 16 的怪物可见、17 不可见（`Globals.cs:37`） |
| 2 | `broadcast_uses_grid_and_follows_placement` | 聊天广播随 `place_player` 改坐标立即生效：40 格→1 人、16 格→2 人、17 格→1 人 |
| 3 | `player_enter_and_leave_view_is_diffed_both_ways` | 甲走开/走回时双方各恰好一条 `ObjectRemove` / `ObjectPlayer` |
| 4 | `run_out_of_view_diffs_player_sets_both_ways` | 跑步离开视野：双方 `visible_players` 集合与包一致 |
| 5 | `run_into_view_diffs_player_sets_both_ways` | 跑步进入视野：双向 add、无幽灵条目 |
| 6 | `melee_target_is_found_through_the_grid_after_placement` | `place_player` 后近战目标经网格找到并延迟结算 |
| 7 | `monster_walk_updates_the_grid_and_player_view` | 怪物移动同步索引并经 AOI 广播 `ObjectWalk` |
| 8 | `door_state_machine_blocks_opens_and_auto_closes_with_wire_packets` | 关门阻挡 `DoorClosed` → 开门双方收 `Opendoor{false}` → 通过 → 5001ms 后 `Opendoor{true}` 并再次阻挡；未登记门号视为可通行 |
| 9 | `aoi_acceptance_enter_snapshot` | R54 进图：真实 `enter`，四类首帧精确 ID 集、自身排除、无重复、类型分组与组内 AOI 序（私有侧例 `..._private` 在 `world.rs`） |
| 10 | `aoi_acceptance_leave_cleanup` | R54 离图：双向 seen/索引清除、离图者恰好一条 remove、二次 leave 无新 remove、后续广播不可达 |
| 11 | `aoi_acceptance_change_map_sets` | R54 切图：三方 `visible_players` 精确集、旧 NPC 清空、`MapChanged` 后新首帧、旧图广播隔离 |
| 12 | `aoi_acceptance_recycle_all_kinds` | R54 回收：普通物品严格 `>` 边界/恰好一条 remove/二次 tick 零重复、金币对照、怪物尸体回收；**NPC 子项 PARTIAL**（无生产回收入口，见 §11） |
| 13 | `aoi_acceptance_broadcast_identity_order` | R54 广播：近/远/异图精确身份集、每接收者完整包序、batching 逐接收者 append 序、空间序 `[4,1,3,2]` vs ID 序 `[1,2,3,4]` |

> 全部用 `advance_clock` 注入时间，不 `sleep`；行号为当前工作树（2026-10-26）实测，格式化后
> 可能位移，以函数名 `grep` 为准。R54 五例的逐路由命令、exit 与断言见
> [`docs/testing/round54/round54-aoi-acceptance.md`](../testing/round54/round54-aoi-acceptance.md)。

---

## 10. 未做 / 明确不做

* **视野遮挡/高度差**：Crystal 的 AOI 是矩形，不做视线遮挡；本作一致。
* **多线程对象处理**：Crystal 可选 `Multithreaded`（`Settings.cs:46-47`）；本作单世界线程，
  AOI 不涉及并发。
* **AOI 持久化/跨服共享**：没有任何序列化，重启重建。
* **动态桶大小/分层网格**：固定 8×8；在线人数极大时再评估。
* **门的状态同步给新进图玩家**：Crystal 新玩家进图不会收到当前已开门的 `S.Opendoor`；
  本作一致（客户端以地图文件的门格为准，服务端只发变化）。
* **门的图形/碰撞按门型区分**：只按 `door_index` 的开/关判定，不区分门占几格。

---

## 11. 验收状态（R54，2026-10-26）

R54 以 **A 组五路由补证** 为本节提供逐路由证据；A 组只补测试、**生产代码 0 改动**，因此下面
的 PASS 仅限列出的断言范围。

| 路由 | 测试（集成 / 私有） | 结论 | 关键断言 |
|---|---|---|---|
| 进图 | `aoi_acceptance_enter_snapshot` / `..._private` | **PASS** | 真实 `enter`；四类首帧精确 ID 集、自身排除、无重复、类型分组与组内 AOI 序 |
| 离图 | `aoi_acceptance_leave_cleanup` / `..._private` | **PASS** | observer/leaver/far 的 `visible_players` 与索引精确清理；leaver 恰好一条 `ObjectRemove`；二次 `leave` 返回 `None` 且无新包；之后 chat/walk/刷怪不可达 |
| 切图 | `aoi_acceptance_change_map_sets` / `..._private` | **PASS** | mover/旧图观察者/新图观察者精确集合；旧 NPC 清空、新 NPC 可见；`MapChanged` 为首包；旧图 chat 负例、新图 chat 正例 |
| 回收 | `aoi_acceptance_recycle_all_kinds` / `..._private` | **PARTIAL** | 普通物品（`Some(UserItem)`）严格 `now > expire_time`：等值保持、+1ms 恰好一条 remove、再 tick 零重复；金币对照（`None`）+ `ObjectGold`；真实怪物死亡→尸体→tick 回收恰好一次。**NPC 无生产回收/移除入口**：生产只有 `spawn_npcs`（`npc.rs:687`），长时推进后 NPC 与其 AOI 项仍在、无 `ObjectRemove`；未手删容器冒充 |
| 广播 | `aoi_acceptance_broadcast_identity_order` / `..._private` | **PASS** | 近(16)/远(17)/异图同坐标精确接收者身份、每接收者完整包向量；actor/exclude 经真实 `open_door`；batching 时 tick flush 前无包、flush 后每接收者一条拼接缓冲按 append 序解码（不以跨 channel 时刻证全局序） |

结果：**4 PASS / 1 PARTIAL / 0 FAIL**。因此：

* **AOI 全量验收仍不可关闭**：回收路由的 NPC 子项没有生产入口，属产品缺口而非测试缺口；
  本轮明确未用「手删容器」制造回收路径。
* 回归：`cargo test -p mir-server --test aoi` 13 passed / `--lib aoi::tests` 7 passed（194 filtered）
  / `--test maps` 20 passed（缺 `Server.MirDB` 样本的 4 例提前 return，**有效 16**）；均 exit 0。
* 门禁：**R54 全量门禁尚未运行**，第 54 轮状态为 PARTIAL；不得以本表 PASS 冒充全量门禁结果。

证据：[R54 证据索引](../testing/round54/README.md) · [五路由验收](../testing/round54/round54-aoi-acceptance.md) · [承接台账](../testing/round54/round54-carryover.md)。
