# R54 AOI 五路由验收（A 组证据）

**执行**：R54 A 组（只写测试）｜**文档**：R54 礼部 C（只写文档，不跑 cargo/门禁；下列命令、读数、
exit 全部转录 A 原件，不臆造未运行的命令）。
**基准**：HEAD `08572ae8ca2420c95a8ecf02977b52c64241f235`，branch `main`（origin/main == HEAD）；
A 证据时间戳 `2026-10-26T10:24–10:28+08:00`（见各日志 `START/END`）。
**证据根**：`target/round54/aoi/attempt-1/`（`00-summary.md`、`m2-matrix.log`、`m2-summary.txt`、
`regression-aoi-full.log`、`regression-lib-aoi-grid.log`、`regression-maps.log`、`mutation/`、
`private-lib-run-1.log`）。

**结论：4 PASS / 1 PARTIAL / 0 FAIL。AOI 全量验收仍不可关闭**（回收路由的 NPC 子项没有生产入口）。
本轮全量门禁**未运行**，第 54 轮状态为 **PARTIAL**；本文件不把定向绿写成「AOI 已全量验收」。

## 1. 执行矩阵

五前缀 × 两目标（集成 `crates/mir-server/tests/aoi.rs`、私有 `crates/mir-server/src/world.rs`
末尾 `#[cfg(test)]` 模块）= **10 个具名用例**。每个前缀先 `--list` 再 `--nocapture`，共 **20 条命令**，
`m2-matrix.log` 逐段以 `EXIT:` 收尾，**20 条全部 `EXIT: 0`**。汇总行：

```
TOTAL_DISCOVERED=10
TOTAL_EXECUTED=10
OVERALL_FAIL=0
```

（`m2-summary.txt` 末段；`m2-matrix.log` 20 段 `START/CMD/…/EXIT`。）

集成侧执行读数统一为 `1 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out`（`aoi.rs` 共
13 例）；私有侧统一为 `1 passed; 0 failed; 0 ignored; 0 measured; 200 filtered out`（库内共 201 例，
即 196 filtered + 5 个 `aoi_acceptance_*_private` 全跑时的口径）。

命令形状（每条路由四条，`<prefix>` 取五前缀）：

```text
cargo test -p mir-server --test aoi <prefix> -- --list
cargo test -p mir-server --test aoi <prefix> -- --nocapture
cargo test -p mir-server --lib <prefix> -- --list
cargo test -p mir-server --lib world::tests::<prefix>_private -- --nocapture
```

## 2. 路由 1：进图（PASS）

**Crystal 锚点**：`PlayerObject.cs:1741-1768` `GetObjects()`（`:1743` 遍历 y、`:1748` 遍历 x、
`:1758` 追加 `Cell.Objects`）；`MapObject.cs:286-308` `Add` → `GetInfo()`。
**Rust 锚点**：`world.rs:1832` `enter`；`:1882-1893` 四类分组发送；`:1934-1941` `aoi_insert` +
`visible_players` 双向登记；`maps.rs:609` `gather_view`；`npc.rs:921` `visible_npc_frames`。
**测试**：`aoi_acceptance_enter_snapshot`（`tests/aoi.rs:685`）＋ `aoi_acceptance_enter_snapshot_private`
（`src/world.rs:7277`）。

**命令与读数**（`m2-matrix.log` 第 1–4 段）：

| 命令 | 读数 | exit |
|---|---|---|
| `cargo test -p mir-server --test aoi aoi_acceptance_enter_snapshot -- --list` | `1 test, 0 benchmarks` | 0 |
| `cargo test -p mir-server --test aoi aoi_acceptance_enter_snapshot -- --nocapture` | `1 passed; 0 failed; 12 filtered` | 0 |
| `cargo test -p mir-server --lib aoi_acceptance_enter_snapshot -- --list` | `world::tests::…_private: test` | 0 |
| `cargo test -p mir-server --lib world::tests::aoi_acceptance_enter_snapshot_private -- --nocapture` | `1 passed; 0 failed; 200 filtered` | 0 |

**断言**：走真实 `World::enter`；NPC 首帧由生产入口 `initialize_map → spawn_npcs` 产生（不是
`spawn_npc` 测试/GM 入口）；四类（玩家/怪物/NPC/物品）首帧对象 ID 精确集合；自身不在自己的
玩家首帧内；无重复；完整首帧向量按 **玩家 → 怪物 → NPC → 物品** 分组、组内按 AOI 序
（NPC 组按 NPC 对象 ID 升序，`npc.rs:922-929`）。私有侧另核对 `gather_view` 与
`visible_*`/`visible_players` 的内部一致性。
**结论**：**PASS**（范围限上述断言）。

## 3. 路由 2：离图（PASS）

**Crystal 锚点**：`PlayerObject.cs:400-407` `StopGame`；`Map.cs:2375` `RemoveObject`。
**Rust 锚点**：`world.rs:1961` `leave`；`:1976` `seen.remove`；`:1980` `visible_players.remove`；
`:1997` `players.remove`（重复 `leave` 由 `?` 短路返回 `None`）。
**测试**：`aoi_acceptance_leave_cleanup`（`tests/aoi.rs:813`）＋ `..._private`（`src/world.rs:7451`）。

**命令与读数**（`m2-matrix.log` 第 5–8 段）：四条命令形状同上（prefix =
`aoi_acceptance_leave_cleanup`），`--list` = 1 test/0 benchmarks，两条 `--nocapture` 均
`1 passed; 0 failed`（12 / 200 filtered），全部 exit 0。

**断言**：真实 `World::leave`；observer/leaver/far 三方 `visible_players` 精确集合；离图者恰好
收到一条 `ObjectRemove`，observer/far 不收到；二次 `leave` 返回 `None` 且不产生新 remove；离图后
chat/走跑/刷怪对离图者不可达。
**结论**：**PASS**。

## 4. 路由 3：切图（PASS）

**Crystal 锚点**：`PlayerObject.cs:4506-4509` 旧图 `RemoveObject`；`:4558-4583` 新图 `Add` /
`S.MapChanged` / `GetObjects`。
**Rust 锚点**：`maps.rs:459` `change_map`；`:487-520` 旧图移除 + 对方 `visible_players` 删除 +
`visible_monsters/npcs/items` 清空 + `aoi_insert`（`:519`）；`:528-574` 新图通知与 refresh；
`:551-565` `MapChanged`。
**测试**：`aoi_acceptance_change_map_sets`（`tests/aoi.rs:928`）＋ `..._private`（`src/world.rs:7588`）。

**命令与读数**（`m2-matrix.log` 第 9–12 段）：四条命令形状同上（prefix =
`aoi_acceptance_change_map_sets`），`--list` 1 test、`--nocapture` 均 `1 passed; 0 failed`
（12 / 200 filtered），全部 exit 0。

**断言**：真实 `change_map`；mover、旧图观察者、新图观察者三方 `visible_players` 精确集合；
旧图 NPC 从 mover 可见集合清空、新图 NPC 可见；mover 通道首包为 `MapChanged`，随后是新图首帧；
旧图观察者恰好一条 `ObjectRemove`；旧图 chat 负例、新图 chat 正例。私有侧核对 AOI 地图归属与
per-map NPC cell。
**结论**：**PASS**。

## 5. 路由 4：回收（PARTIAL）

**Crystal 锚点**：`MapObject.cs:339-375`（`Spawn`/`Despawn`，尸体在 `:363` 移除、`:375`
`Node = null`）；`ItemObject.cs:129-135`（`:131` `if (Envir.Time > ExpireTime)`，严格 `>`）。
**Rust 锚点**：`items.rs:1593` `forget_ground_item`、`:1626` `tick_ground_items`、`:1629`
`if now > ground.expire_time`（严格 `>`）；`world.rs:5121` `forget_monster`（先广播 remove 再
`aoi_remove` Monster）；`npc.rs:687` `spawn_npcs`（生产 NPC 入口）、`:762` `spawn_npc`（测试/GM）。
**测试**：`aoi_acceptance_recycle_all_kinds`（`tests/aoi.rs:1104`）＋ `..._private`（`src/world.rs:7752`）。

**命令与读数**（`m2-matrix.log` 第 13–16 段）：四条命令形状同上（prefix =
`aoi_acceptance_recycle_all_kinds`），`--list` 1 test、`--nocapture` 均 `1 passed; 0 failed`
（12 / 200 filtered），全部 exit 0。

**已证子项（PASS）**：

* 普通物品（`Some(UserItem)`）：`now == expire_time` 时仍在，`now = expire_time + 1ms` 时恰好一条
  `ObjectRemove`，再 tick 零重复；到期前收到 `ObjectItem` 帧。
* 金币对照：金币堆 `None`（`is_gold()`、`ObjectGold`），不作为「普通物品」证据（R53 C9 的
  `None,1000` 实为金币，本路由单列）。
* 真实怪物死亡 → 尸体 → 延迟边界 → tick 回收：恰好一条 remove，二次 tick 零重复。

**NPC 子项（PARTIAL，原文口径）**：`production has no NPC recycle/removal entry; the private test
documents that a production-spawned NPC and its AOI entry persist after advance_clock(1_000_000) and
3 ticks and no ObjectRemove is sent for it. Containers were not hand-deleted to fake a recycle path.`
即：**生产无 NPC 回收/移除入口**，私有测推进 `1,000,000ms` + 3 tick 后 NPC 与其 AOI 项仍在、
无 `ObjectRemove`；**未手删容器冒充回收路径**。生产代码检索也只在 `maps.rs:513` 命中
`player.visible_npcs.clear()`（玩家可见集合），NPC 存储本身没有 remove/retain/clear 调用。

**结论**：**PARTIAL**（物品/金币/怪物子项 PASS；NPC 子项无生产入口，属产品缺口）。

## 6. 路由 5：广播（PASS）

**Crystal 锚点**：`Map.cs:2437-2448` `Broadcast`（`:2441` 对 `Players` 逆序）；`MapObject.cs:408`
排除自身。
**Rust 锚点**：`world.rs:5557` `broadcast`；`:5568-5570` `exclude`/`actor` 过滤；
`:5577-5582` batching 拼缓冲；`:5652` `flush_broadcast_batch`（tick 内 flush，`:2565-2566`）；
`aoi.rs:267` 查询按 `(y, x, id)` 排序。
**测试**：`aoi_acceptance_broadcast_identity_order`（`tests/aoi.rs:1292`）＋ `..._private`
（`src/world.rs:7948`）。

**命令与读数**（`m2-matrix.log` 第 17–20 段）：四条命令形状同上（prefix =
`aoi_acceptance_broadcast_identity_order`），`--list` 1 test、`--nocapture` 均
`1 passed; 0 failed`（12 / 200 filtered），全部 exit 0。

**断言**：同图近（16 格）/远（17 格）/异图同坐标的接收者精确身份集；每个接收者收到完整包向量；
发起者/`exclude` 经真实 `open_door` 生产路径；私有测网格返回序为空间序 `[4,1,3,2]`、与 ID 排序
`[1,2,3,4]` 不同；默认直接 send 与 batching 两种模式：开启 batching 后 tick flush 前无消息，
flush 后每接收者一条拼接缓冲、按 append 序解码。**不以跨 channel 取包时刻证明全局发送序**。
**结论**：**PASS**（范围限上述断言；单 tick 两帧的 batching 压力另见 §9）。

## 7. M4 口径核对（已入文档）

1. **物品到期严格 `>`**：Crystal `github/Crystal/Server/MirObjects/ItemObject.cs:129-135` 为
   `public override void Process() { … if (Envir.Time > ExpireTime) { CurrentMap.RemoveObject(this);
   Despawn(); } }`；Rust `items.rs:1629` 同样是 `now > ground.expire_time`，测试断言等值保持、
   +1ms 才移除。
2. **batching 口径**：`world.rs:5577-5582` 每接收者 `extend_from_slice(packet)` 拼一条缓冲，
   `flush_broadcast_batch`（`:5652`）在 tick 内 flush；因此「每接收者完整序列」以缓冲解码序为准，
   不能把跨线程/channel 的取包时刻当作服务端全局发送序；Crystal 无批次层（`Map.cs:2437-2448` 直发）。
3. **排序口径**：AOI 半径查询按 `(y, x, object_id)`（`aoi.rs:267`），玩家差分批内按对象 ID 升序
   （`world.rs:5287-5293` 的 `BTreeSet::difference`），两者不同；首帧类型分组见
   `world.rs:1882-1893` 与 `world/aoi.md` §8 第 8 条。

## 8. 回归与安全网（全部转录 A 原件）

| 命令 | 读数 | exit | 日志 |
|---|---|---|---|
| `cargo test -p mir-server --test aoi -- --nocapture` | `13 passed; 0 failed` | 0 | `regression-aoi-full.log` |
| `cargo test -p mir-server --lib aoi::tests` | `7 passed; 0 failed; 194 filtered` | 0 | `regression-lib-aoi-grid.log` |
| `cargo test -p mir-server --test maps -- --nocapture` | `20 passed; 0 failed`（4 例因缺 `resource/` 的 `Server.MirDB` 样本提前 return → **有效 16**） | 0 | `regression-maps.log` |
| `cargo test -p mir-server --lib aoi_acceptance -- --nocapture`（首红） | `4 passed; 1 failed; 196 filtered`，panic `world.rs:7397`：`player AOI order must be (y, x, id)` left `[100, 101]` right `[101]` | 101 | `private-lib-run-1.log` |
| 同上（修正期望后复跑） | `5 passed; 0 failed; 196 filtered` | 0 | `private-lib-run-1.log` 后半 |

`maps` 的 4 个资源依赖例：`real_database_has_movement_points_that_resolve_to_maps`、
`real_map_files_load_and_change_map_moves_between_them`、
`bichon_has_a_walkable_movement_point_near_the_plaza`、
`player_can_walk_from_the_plaza_into_a_natural_teleport_point`；日志 4 次
`跳过：resource/ 里没有 Server.MirDB 样本`。未下载素材、未改 DB。

**变异自检**（`mutation/mutation-selfcheck.log`，证明回收入口断言可失败）：Round A 注入后
`0 passed; 5 failed; 8 filtered` exit 101 → 还原 `5 passed` exit 0；Round B
`0 passed; 5 failed; 196 filtered` exit 101 → 还原 `5 passed` exit 0；末行
`MUTATION_SELFCHECK_FAIL=0`。

## 9. 证据缺口与风险

* **集成首跑失败未留日志**：进图集成例首跑曾因「远玩家 16 格仍可见」失败，但未保留该次日志；
  由变异自检证明同类断言可失败，属**证据保全缺口**，不得当作无失败。
* **私有测试耦合内部字段**（私有 `visible_*`/`gather_view`、合成 `MirDb`），且不是生产路径全覆盖。
* **batching 只压单 tick 两帧**，多 tick、多接收者、跨图混合未压。
* **NPC 回收缺口是产品问题**：生产只有 `spawn_npcs`，无回收/移除入口；不能用「删容器」伪造。
* **maps 资源 4 例提前 return**，资源就绪后应重跑；20 reported 不等于 20 有效。
* A/B 证据后工作树发生格式化（`tests/aoi.rs` 10:37、`round54_frost_edges.rs` 10:42），
  行为等价的格式变化不影响上述读数。Lead 已于格式化后的受测树独立定向复跑（fmt/clippy 与 aoi 13 /
  lib aoi::tests 7 / frost 3 / frost53 7 / reg52 8 / magic 43 / maps 20，全 exit0，见
  `target/round54/lead-verify2/**`）；**剩余仅 R54 全量门禁未运行**。

## 10. 结论

| 路由 | 集成例 | 私有例 | 有效用例数 | 原始 exit | 结论 |
|---|---|---|---|---|---|
| 进图 | `aoi_acceptance_enter_snapshot` | `world::tests::…_private` | 2 | 0 | PASS |
| 离图 | `aoi_acceptance_leave_cleanup` | `world::tests::…_private` | 2 | 0 | PASS |
| 切图 | `aoi_acceptance_change_map_sets` | `world::tests::…_private` | 2 | 0 | PASS |
| 回收 | `aoi_acceptance_recycle_all_kinds` | `world::tests::…_private` | 2 | 0 | **PARTIAL**（NPC 子项） |
| 广播 | `aoi_acceptance_broadcast_identity_order` | `world::tests::…_private` | 2 | 0 | PASS |

合计 **10 个具名用例 / 20 条命令 / 全 exit 0**，结论 **4 PASS / 1 PARTIAL / 0 FAIL**。
**AOI 全量验收仍不可关闭**；R54 全量门禁未运行，状态 PARTIAL。证据索引见
[`README.md`](README.md)，承接台账见 [`round54-carryover.md`](round54-carryover.md)。
