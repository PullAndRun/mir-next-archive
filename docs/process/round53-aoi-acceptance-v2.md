# R53 AOI 验收复审 v2（A2）

**Verdict：PASS-WITH-PARTIAL。** F2 跑步阻断已解除；八路由由 v1 **PASS 2 / PARTIAL 5 / BLOCK 1** 变为 **PASS 3 / PARTIAL 5 / BLOCK 0**，AOI 全量验收仍不可关闭。
基准 HEAD：`48aaec0ea85af0cf44c0c0ed9e7af5ff050f1439`；2026-10-26；报告骨架先落盘后补证据；只读 crates，不修改 v1，不 commit，不执行全量门禁。
沿用 [v1](round53-aoi-acceptance.md) 的八路由定义（7=视野边界/双向差分，8=门状态），所有 PASS 均限表中覆盖范围。

## 八路由矩阵

下表 C1–C14 的完整命令、实际用例数见下一节；日志路径均在 `target/round53/a2/`，Crystal 锚点沿用 v1 并对照本地源码。

| 路由 | Crystal 锚点 | Rust 锚点 | 测试名 + 命令编号 | exit | 日志路径 | 结论 |
|---|---|---|---|---|---|---|
| 1 进图 | [PlayerObject.cs:1741](../../github/Crystal/Server/MirObjects/PlayerObject.cs#L1741) GetObjects | [world.rs:1881](../../crates/mir-server/src/world.rs#L1881) gather_view；:1934 登记 | `entering_world_notifies_both_directions` C6；`entering_world_sends_object_npc_and_npcs_block_movement` C8；`player_entering_world_receives_nearby_monsters` C13 | 0/0/0 | [06](../../target/round53/a2/06-enter.log)、[08](../../target/round53/a2/08-npc-enter.log)、[13](../../target/round53/a2/13-monster-enter.log) | PARTIAL：NPC/怪物子项通过；玩家库测试仅检查有包，缺四类完整首帧/自身排除/包序 |
| 2 离图 | [PlayerObject.cs:400](../../github/Crystal/Server/MirObjects/PlayerObject.cs#L400) StopGame；[Map.cs:2375](../../github/Crystal/Server/MirEnvir/Map.cs#L2375) RemoveObject | [world.rs:1961](../../crates/mir-server/src/world.rs#L1961) leave；:1979 清索引 | `leaving_notifies_observers` C11；`logging_out_removes_the_member_from_the_group` C12，真实 leave + 指定 ID ObjectRemove | 0/0 | [11](../../target/round53/a2/11-leave.log)、[12](../../target/round53/a2/12-group-leave.log) | PARTIAL：已证离图通知；缺双方 seen/网格清理、后续广播不可达及恰好一次 |
| 3 移动/跑步 | [HumanObject.cs:2596](../../github/Crystal/Server/MirObjects/HumanObject.cs#L2596) Run 摘旧格/remove/add | [world.rs:2140](../../crates/mir-server/src/world.rs#L2140) walk；[world.rs:2241](../../crates/mir-server/src/world.rs#L2241) run 玩家同步 | `player_enter_and_leave_view_is_diffed_both_ways`、`run_out_of_view_diffs_player_sets_both_ways`、`run_into_view_diffs_player_sets_both_ways` C1 | 0（8/8） | [01](../../target/round53/a2/01-aoi.log) | PASS：真实 walk/run 双向 remove/add，run 无幽灵且集合一致；原 BLOCK 解除 |
| 4 切图 | [PlayerObject.cs:4506](../../github/Crystal/Server/MirObjects/PlayerObject.cs#L4506)、:4558–4583 | [maps.rs:487](../../crates/mir-server/src/maps.rs#L487) 旧图撤销；:506 清空；:528 新图通知；:574 refresh | `cross_map_change_removes_from_old_map_and_reveals_the_new_map`、`ground_items_and_npcs_are_visible_after_map_change`、`map_change_clears_the_old_visibility_sets`、`two_map_changes_in_a_row_do_not_leak_monsters` C5 | 0 | [05](../../target/round53/a2/05-maps.log) | PARTIAL：真实跨图、旧新观察者/怪物/NPC/金币/部分集合通过；未直接断言三方 visible_players 精确集、旧 NPC 清除及切图后广播隔离 |
| 5 刷出/回收 | [Map.cs:2361](../../github/Crystal/Server/MirEnvir/Map.cs#L2361) Add/Remove；[MapObject.cs:339](../../github/Crystal/Server/MirObjects/MapObject.cs#L339) Spawned/:358 Despawn | [world.rs:4884](../../crates/mir-server/src/world.rs#L4884) 刷怪；:5121 forget_monster；[items.rs:1512](../../crates/mir-server/src/items.rs#L1512) | `player_kills_monster_and_broadcasts_object_died_then_remove` C7；`ground_item_expires_after_the_configured_timeout_and_broadcasts_remove` C9；`gold_pile_expires_and_broadcasts_object_remove` C10；`spawning_monster_broadcasts_object_monster_to_players_in_range` C14 | 均 0 | [07](../../target/round53/a2/07-monster-remove.log)、[09](../../target/round53/a2/09-item-remove.log)、[10](../../target/round53/a2/10-gold-remove.log)、[14](../../target/round53/a2/14-monster-spawn.log) | PARTIAL：尸体延迟回收/金币过期通知及近远刷怪通过；C9 实际也生成金币，非普通物品；缺 NPC 回收、普通物品回收和精确索引/seen 撤销 |
| 6 广播接收者/顺序 | [Map.cs:2437](../../github/Crystal/Server/MirEnvir/Map.cs#L2437) Players 逆序；[MapObject.cs:408](../../github/Crystal/Server/MirObjects/MapObject.cs#L408) 排自身 | [world.rs:5557](../../crates/mir-server/src/world.rs#L5557) query→exclude/actor；[aoi.rs:267](../../crates/mir-server/src/aoi.rs#L267) y/x/id | `broadcast_uses_grid_and_follows_placement` C1；`radius_query_is_chebyshev_and_orders_by_y_x_id` C2；近远生成 C14 | 均 0 | [01](../../target/round53/a2/01-aoi.log)、[02](../../target/round53/a2/02-lib-aoi.log)、[14](../../target/round53/a2/14-monster-spawn.log) | PARTIAL：范围/网格顺序通过，缺跨图负例、多人精确接收者与世界完整发包顺序 |
| 7 视野边界/双向差分 | [Globals.cs:37](../../github/Crystal/Shared/Globals.cs#L37) 16；[Functions.cs:71](../../github/Crystal/Shared/Functions/Functions.cs#L71) Chebyshev | [aoi.rs:159](../../crates/mir-server/src/aoi.rs#L159) 负坐标；:295 精确过滤；[world.rs:5275](../../crates/mir-server/src/world.rs#L5275) 双向差分 | `view_radius_is_crystal_data_range_sixteen`、walk/run 两向 C1；7 个 `aoi::tests` C2；`random_operations_match_bruteforce_scan` C3 | 0/0/0 | [01](../../target/round53/a2/01-aoi.log)、[02](../../target/round53/a2/02-lib-aoi.log)、[03](../../target/round53/a2/03-bruteforce.log) | PASS：维持 v1，并补齐 run 与全部网格单测；随机对拍为 Rust 暴力基准，非运行 Crystal |
| 8 门状态 | [Map.cs:50](../../github/Crystal/Server/MirEnvir/Map.cs#L50)、:646；[PlayerObject.cs:7818](../../github/Crystal/Server/MirObjects/PlayerObject.cs#L7818) | [world.rs:1773](../../crates/mir-server/src/world.rs#L1773) 开门；:1805 严格超时；:5449 阻挡 | `door_state_machine_blocks_opens_and_auto_closes_with_wire_packets` C1 | 0 | [01](../../target/round53/a2/01-aoi.log) | PASS：维持 v1 的关闭/打开/步行阻挡/5001ms 关闭/双方开关包范围；不外推精确 5000ms、远端与同名实例 |

## 命令与实跑范围

仓库根执行；每条测试均设置 `TMP=TEMP=<repo>/target/tmp`；完整 stdout/stderr 原样保存。L2 限定回归，不能用静态检查替代；实测起止/秒数/HEAD/exit/SHA 见 [meta](../../target/round53/a2/meta.txt) 和 [evidence.json](../../target/round53/a2/evidence.json)。未执行任何全量门禁。

| ID | 完整命令（均以前缀 `cargo test -p mir-server` 执行） | 核实结果 |
|---|---|---|
| C1 | `cargo test -p mir-server --test aoi -- --nocapture` | 8 passed / 0 failed |
| C2 | `cargo test -p mir-server --lib aoi::tests` | 7 passed / 0 failed |
| C3 | `cargo test -p mir-server --lib random_operations_match_bruteforce_scan` | 1 passed / 0 failed；是 C2 中同一用例的复跑 |
| C4 | `cargo test -p mir-server --lib -- --list` | 196 tests 清单；仅发现，不计通过数 |
| C5 | `cargo test -p mir-server --test maps -- --nocapture` | 20 reported passed；4 条资源依赖测试提前 return，实际有效 16 条合成测试 |
| C6 | `cargo test -p mir-server --lib entering_world_notifies_both_directions -- --nocapture` | 1 passed |
| C7 | `cargo test -p mir-server --test monster_combat player_kills_monster_and_broadcasts_object_died_then_remove -- --nocapture` | 1 passed |
| C8 | `cargo test -p mir-server --test npc entering_world_sends_object_npc_and_npcs_block_movement -- --nocapture` | 1 passed |
| C9 | `cargo test -p mir-server --test items ground_item_expires_after_the_configured_timeout_and_broadcasts_remove -- --nocapture` | 1 passed；源码参数 None,1000 实为金币 |
| C10 | `cargo test -p mir-server --test items gold_pile_expires_and_broadcasts_object_remove -- --nocapture` | 1 passed |
| C11 | `cargo test -p mir-server --lib leaving_notifies_observers -- --nocapture` | 1 passed；仅有包+玩家总数断言 |
| C12 | `cargo test -p mir-server --test group logging_out_removes_the_member_from_the_group -- --nocapture` | 1 passed；有指定 ID 移除包，但注释的“先于”未用位置比较断言 |
| C13 | `cargo test -p mir-server --test monster_combat player_entering_world_receives_nearby_monsters -- --nocapture` | 1 passed |
| C14 | `cargo test -p mir-server --lib spawning_monster_broadcasts_object_monster_to_players_in_range -- --nocapture` | 1 passed |

C5 提前返回：`real_database_has_movement_points_that_resolve_to_maps`、`real_map_files_load_and_change_map_moves_between_them`、`bichon_has_a_walkable_movement_point_near_the_plaza`、`player_can_walk_from_the_plaza_into_a_natural_teleport_point`；日志四次明确“跳过：resource/ 里没有 Server.MirDB 样本”。不把这些或任何 0 tests 当作通过证据。

## F2 核验

- [world.rs:2237](../../crates/mir-server/src/world.rs#L2237)：broadcast → send_user_location → sync_player_players(:2241) → monsters → spell_fields → ground_items，与 [walk:2140](../../crates/mir-server/src/world.rs#L2140) 同序。
- [aoi.rs:227](../../crates/mir-server/tests/aoi.rs#L227) 跑出：先 walk 积累计数，推进 900ms，真实 `World::run` 返回 (19,20)；两边恰好各 1 remove、0 ghost ObjectPlayer，可见玩家集合均严格等于空集。
- [aoi.rs:305](../../crates/mir-server/tests/aoi.rs#L305) 跑入：真实 run 返回 (21,20)；两边各 1 ObjectPlayer、0 remove，可见集合严格为 `{2}` / `{1}`。900ms 在动作锁之后、idle reset 之前，未绕开 run 路径。
- [F2 变异日志](../../target/round53/f2/09-mutation-without-sync.log)：去掉同步后两测试都失败，:262/:345 的计数 left=0/right=1；0 passed / 2 failed，断言可失败。本轮只读引用/哈希并留存副本，未再次修改 crates 做变异；F2 的运行前 HEAD 617e202 与本轮提交后 48aaec0 分开记录。
- C1/C2/C3 本轮 exit 全 0，实际非零测试数 8/7/1；因此接受 F2 修复真实性与路由 3 的限定 PASS。

## 剩余 PARTIAL 与 R54 建议

下列 `aoi_acceptance_*` 是**建议新增**的用例名，目前不存在、未执行；必须先实现断言，再 `--list` 核实非零匹配，不能以过滤后 0 tests 关闭缺口。以下命令均使用同一 TMP/TEMP 前缀。

| 路由 | 缺口与影响 | R54 建议命令（补测试后） |
|---|---|---|
| 1 | 四类完整初始集合、自身排除、首帧分组/顺序；缺失或重复外观仍可能漏检 | `cargo test -p mir-server --test aoi aoi_acceptance_enter_snapshot -- --nocapture` |
| 2 | 离图后两侧 seen/索引精确清理、后续广播不可达、重复离图无重复 remove；可能残留幽灵 | `cargo test -p mir-server --test aoi aoi_acceptance_leave_cleanup -- --nocapture` |
| 4 | 三方 visible_players 精确集、旧 NPC 清空、旧图广播负例；现有四类子项不等于完整重建 | `cargo test -p mir-server --test aoi aoi_acceptance_change_map_sets -- --nocapture`；现有 C5 可先复跑 |
| 5 | 普通物品与 NPC 回收、尸体回收后 AOI/seen 精确撤销；可能 stale 索引或重复移除 | `cargo test -p mir-server --test aoi aoi_acceptance_recycle_all_kinds -- --nocapture`；先复跑 C7/C9/C10 |
| 6 | 跨图负例、接收者身份、完整包序与 Crystal 差异契约；人数相同仍可能发错对象 | `cargo test -p mir-server --test aoi aoi_acceptance_broadcast_identity_order -- --nocapture` |

门的远端/5000ms/跑步撞门/同名地图边界仍沿用 v1 未覆盖声明；不因此扩大第 8 路由 PASS 的范围。资源依赖四例需准备真实样本后复跑 C5。

## Roadmap / AOI 文档措辞建议（仅建议，未修改）

- [roadmap.md:26](../roadmap.md#L26) 保持 🟡，建议写：“R53 A2：网格 7 单测、随机对拍、步行/跑步双向玩家边界与门基础生命周期通过；F2 跑步漏同步已修复复验；八路由 PASS 3/PARTIAL 5/BLOCK 0，进/离/切图完整集合、全类型回收、广播身份和顺序待 R54 补测。”
- [aoi.md:155](../world/aoi.md#L155) 将“广播按对象 ID”改为“按 AOI 查询 `(y,x,id)` 顺序遍历，并过滤 exclude 与 actor”；不声称已有跨接收者时序实测。
- [aoi.md:175](../world/aoi.md#L175) 将“同批 `(y,x,id)`”改为“玩家差分先 remove 后 add，批内由 BTreeSet 差集按玩家 ID”；[world.rs:5288](../../crates/mir-server/src/world.rs#L5288) 是直接依据。
- [aoi.md:226](../world/aoi.md#L226) 差异节补充：enter 按玩家/怪物/NPC/物品分组；Crystal cell 插入序/Players 逆序不等同 Rust 查询顺序；门广播中心差异也需单列。避免笼统“效果相同/逐包等价”。

证据入口：[00-summary](../../target/round53/a2/00-summary.md)、[meta](../../target/round53/a2/meta.txt)、[evidence.json](../../target/round53/a2/evidence.json)；源码与 v1 的 SHA 同入清单，原始日志可独立复核。
