# R53-A AOI 全量验收审计

> 审计基准：617e20266f0596e3f64a5117b2ad1593fd98def2；2026-10-26；刑部只读审计。仅写本文与 target/round53/aoi-audit；不改 crates、不 commit、不执行全量门禁。

结论：**BLOCK，AOI 全量验收不可关闭**。八路由为 **PASS 2 / PARTIAL 5 / BLOCK 1**。两条指定命令均 exit 0（6+1 用例通过），但 run 缺玩家双向视野差分，且进/离/切图与回收、顺序未形成完整实测闭环。测试全绿仅支持下述验收边界。

## 八路由矩阵（完成一路立即追加）

| 路由 | Crystal 行为与锚点 | Rust 实现与锚点 | 实际测试/命令 | 原始日志 | 结论 |
|---|---|---|---|---|---|
| 1 进图 | [PlayerObject.cs:1741](../../github/Crystal/Server/MirObjects/PlayerObject.cs#L1741) 扫 33×33，逐对象 Add | [world.rs:1881](../../crates/mir-server/src/world.rs#L1881) gather_view 后按玩家/怪物/NPC/物品发首帧；:1934 登记索引及双方集合；[maps.rs:609](../../crates/mir-server/src/maps.rs#L609) 精确半径查询 | C1 的 `player_enter_and_leave_view_is_diffed_both_ways` 调 enter，但 :186-187 丢弃首帧；`view_radius_is_crystal_data_range_sixteen` 是 enter 后刷怪，不验证完整进图快照 | [L1](../../target/round53/aoi-audit/run-aoi.log) 6/6 通过；进图四类集合/首帧次序没有直接断言 | PARTIAL：真实 enter 被执行，完整可见集及首帧同步未闭环 |
| 2 离图 | [PlayerObject.cs:400](../../github/Crystal/Server/MirObjects/PlayerObject.cs#L400) RemoveObject→ObjectRemove→Despawn；[Map.cs:2375](../../github/Crystal/Server/MirEnvir/Map.cs#L2375) 清 cell | [world.rs:1961](../../crates/mir-server/src/world.rs#L1961) leave 通知附近玩家、清对方 seen，:1979 清 AOI/本人 visible_players，:1997 删除玩家 | C1 `player_enter_and_leave_view_is_diffed_both_ways` 只 walk 离开视窗，从未调用 World::leave；C2 仅网格随机 remove | [L1](../../target/round53/aoi-audit/run-aoi.log)、[L2](../../target/round53/aoi-audit/run-bruteforce.log) 均通过，不能证明退出路径 | PARTIAL：缺退出后广播不可达、seen 撤销及不重复 ObjectRemove 的实测断言 |
| 3 移动/跑步 | [HumanObject.cs:2596](../../github/Crystal/Server/MirObjects/HumanObject.cs#L2596) Run 摘旧格→RemoveObjects(dir,steps)→新格→AddObjects；:7864/:8152 扫出入条带 | [world.rs:2126](../../crates/mir-server/src/world.rs#L2126) walk 搬桶且 :2140 调玩家双向差分；[world.rs:2227](../../crates/mir-server/src/world.rs#L2227) run 搬桶，但 :2239-2241 只同步怪物/法术场/物品，漏玩家差分 | C1 `player_enter_and_leave_view_is_diffed_both_ways` 证明 walk 越 16/17 格时双方各 1 条 remove/add；全部 6 测试无 run 调用 | [L1](../../target/round53/aoi-audit/run-aoi.log) 该 walk 测试通过；跑步缺陷为静态调用链确认，未伪称失败测试 | BLOCK：run 越界不撤销/重建玩家可见集合，阻止全量关闭 |
| 4 切图 | [PlayerObject.cs:4506](../../github/Crystal/Server/MirObjects/PlayerObject.cs#L4506) 旧图移除并广播；:4558-4583 换图、AddObject、MapChanged、GetObjects | [maps.rs:487](../../crates/mir-server/src/maps.rs#L487) 通知旧观察者，:506-520 清旧索引/集合并挂新图，:528-574 通知新观察者/MapChanged/refresh_own_view | C1 `broadcast_uses_grid_and_follows_placement` 仅同图 place_player；C1 未调用 change_map；C2 只单个 AoiGrid 随机对拍 | [L1](../../target/round53/aoi-audit/run-aoi.log)、[L2](../../target/round53/aoi-audit/run-bruteforce.log) 通过但没有切图闭环 | PARTIAL：缺真实切图后旧/新观察者与四类视野清空重建实测 |
| 5 刷出/回收 | [Map.cs:2361](../../github/Crystal/Server/MirEnvir/Map.cs#L2361) Add/Remove 维护 cell；[MapObject.cs:339](../../github/Crystal/Server/MirObjects/MapObject.cs#L339) Spawned 广播外观，:358 Despawn 广播移除 | [world.rs:4881](../../crates/mir-server/src/world.rs#L4881) 插入怪物并 sync_monster_view；:5118 forget_monster 撤可见集/发 remove/清网格；[monster.rs:2796](../../crates/mir-server/src/monster.rs#L2796) 移动维护索引 | C1 `view_radius_is_crystal_data_range_sixteen` 刷怪 16 有/17 无；`monster_walk_updates_the_grid_and_player_view` 移动收到 ObjectWalk；`melee_target_is_found_through_the_grid_after_placement` 只断言 dead 或 None | [L1](../../target/round53/aoi-audit/run-aoi.log) 通过；C2 remove 只证明网格层 | PARTIAL：怪物生成可验收；死亡尸体回收 ObjectRemove、AOI 消失、NPC/物品生成回收缺实际断言 |
| 6 广播接收者/顺序 | [Map.cs:2437](../../github/Crystal/Server/MirEnvir/Map.cs#L2437) 逆序 Players、DataRange 过滤；[MapObject.cs:408](../../github/Crystal/Server/MirObjects/MapObject.cs#L408) 另排除自身 | [world.rs:5554](../../crates/mir-server/src/world.rs#L5554) 按 query_radius_kind 顺序、exclude 与 actor 过滤；:5333 chat 自发后广播；[aoi.rs:267](../../crates/mir-server/src/aoi.rs#L267) 为 y/x/id 顺序 | C1 `broadcast_uses_grid_and_follows_placement` 只断言人数 1→2→1；门用例证明本人/另一观察者各 1 条开关包；双向 walk 用例只计数不验完整包序 | [L1](../../target/round53/aoi-audit/run-aoi.log) 通过；[L2](../../target/round53/aoi-audit/run-bruteforce.log) 验网格排序，不验世界发包序列 | PARTIAL：范围子项通过；缺跨图负例、多人接收者身份及发送顺序实证；Crystal 插入/逆序与 Rust 确定排序存在差异 |
| 7 视野边界/双向差分 | [Globals.cs:37](../../github/Crystal/Shared/Globals.cs#L37) 16 格；[Functions.cs:71](../../github/Crystal/Shared/Functions/Functions.cs#L71) 切比雪夫；[HumanObject.cs:7864](../../github/Crystal/Server/MirObjects/HumanObject.cs#L7864) 条带 remove/add | [aoi.rs:60](../../crates/mir-server/src/aoi.rs#L60) 8×8；:159 div_euclid；:295-331 桶候选精确过滤排序；[world.rs:5272](../../crates/mir-server/src/world.rs#L5272) 集合差双向更新 | C1 `view_radius_is_crystal_data_range_sixteen`、`player_enter_and_leave_view_is_diffed_both_ways`；C2 `aoi::tests::random_operations_match_bruteforce_scan` seed=0x5eed、200 初始对象×1000 步、四类、负/正坐标、随机半径 0..20，逐步比全条目及顺序 | [L1](../../target/round53/aoi-audit/run-aoi.log)、[L2](../../target/round53/aoi-audit/run-bruteforce.log) 均通过 | PASS：限网格随机/桶边界与步行玩家 16↔17 双向差分；跑步缺陷归第 3 路由 |
| 8 门状态 | [Map.cs:50](../../github/Crystal/Server/MirEnvir/Map.cs#L50) 开门更新状态；:646-663 关门阻挡且严格超过 5000ms 自动关闭；[PlayerObject.cs:7818](../../github/Crystal/Server/MirObjects/PlayerObject.cs#L7818) 自发+观察者同步 | [world.rs:1773](../../crates/mir-server/src/world.rs#L1773) 开门更新/self+broadcast；:1805 `last_tick + DOOR_OPEN_MS < now`；:5446 check_cell 门阻挡 | C1 `door_state_machine_blocks_opens_and_auto_closes_with_wire_packets`：关闭 walk 被阻→开门本人/观察者各 1 包→可走→5001ms 各 1 关闭包→再次阻挡；未知门不误阻挡 | [L1](../../target/round53/aoi-audit/run-aoi.log) 通过 | PASS：门状态生命周期、步行阻挡和本人/观察者开关同步；未覆盖远端观察者/精确 5000ms/跑步/同名地图实例 |

## 缺口与风险

1. **阻断 B1：跑步遗漏玩家同步（静态确认，非失败测试）。** [world.rs:2140](../../crates/mir-server/src/world.rs#L2140) walk 调 `sync_player_players`，但 [world.rs:2237](../../crates/mir-server/src/world.rs#L2237) run 在广播后只有怪物/法术场/物品同步；该函数引用检索只有 walk 调用与定义。位置已搬桶，双方 `visible_players` 却保留旧集；静止观察者不会补偿，跑出 16 格缺 ObjectRemove、跑入缺 ObjectPlayer。Crystal [HumanObject.cs:2597](../../github/Crystal/Server/MirObjects/HumanObject.cs#L2597) Run 显式执行 RemoveObjects/AddObjects。建议由实现方修复并补跑入/跑出各方向双向断言，再复审；本审计不改代码。
2. **覆盖缺口。** 补 enter 首帧四类对象/自身排除/完整包序、leave 最后清理与无幽灵广播、change_map 旧新图观察者与集合重建、死亡→尸体回收 remove/索引一致性。不能把 walk 离开视窗、同图 place_player、纯网格 remove 视为这些路由已验收。
3. **顺序契约。** Crystal 首帧是 y/x/cell 插入序，Rust 首帧按类型分组；Crystal 广播逆 Players 插入序，Rust y/x/id；玩家集合差按 BTreeSet ID 序。[aoi.md:155](../world/aoi.md#L155) 的广播 ID 序、:175 的差分空间序与实现不符，:226-243 同格差异说明不足以覆盖类型分组。须确定接受的新契约并补身份/包序断言，不能宣称逐包等价。
4. **门边界。** 只关闭门开/关/步行阻挡/本人+近邻同步子项；不外推远端范围、精确 5000ms、跑步撞门、初进图时已有开启门的快照或同名多实例。Rust 开门广播以门为中心，Crystal Player.OpenDoor 以玩家为中心，边缘接收集合差异需单列补测。
5. **随机证据边界。** C2 是 Rust 网格与同语义暴力扫描结果对拍，非 Crystal 程序运行对拍；没有覆盖世界可见集合/网络序列。没有执行额外 `--lib aoi`，故不能把文档列出的全部网格单测算作本轮实跑。

## Roadmap 关闭建议

[roadmap.md:26](../roadmap.md#L26) **不得改为 AOI 全量验收通过**。可在保留总状态 PARTIAL 的前提下，把“待刑部全量验收”细化为：“R53-A 已审：16 格/8×8 网格随机对拍、步行玩家双向边界与门生命周期通过；跑步玩家视野同步 BLOCK；进/离/切图、回收和广播顺序 PARTIAL，待修复补测复验”。仅给建议，未改 Roadmap。

## 精确实跑记录

工作目录为仓库根；两命令均 `TMP=TEMP=<repo>/target/tmp`，原始 stdout/stderr 合并完整写入 L1/L2；HEAD 均为本文基准。

| ID | 精确命令 | 起止时间（+08:00） | exit / 实际结果 | 原始日志 SHA-256 |
|---|---|---|---|---|
| C1 | `cargo test -p mir-server --test aoi -- --nocapture` | 2026-10-26T08:12:13 → 08:12:14 | 0；6 passed，0 failed/ignored/filtered | `4730c22b730c2b8d35c37f457204b0970a082a9d85b250ca70254f668869d7db` |
| C2 | `cargo test -p mir-server random_operations_match_bruteforce_scan -- --nocapture` | 2026-10-26T08:12:14 → 08:12:16 | 0；1 passed；库 195 filtered，其余目标 0 用例 | `30bbc744799dc0cd6237d6f6a7885f0ebe529c19c573e0d07949dcd53edd1915` |

C1 实际用例（仅此 6 条）：`broadcast_uses_grid_and_follows_placement`、`melee_target_is_found_through_the_grid_after_placement`、`view_radius_is_crystal_data_range_sixteen`、`door_state_machine_blocks_opens_and_auto_closes_with_wire_packets`、`monster_walk_updates_the_grid_and_player_view`、`player_enter_and_leave_view_is_diffed_both_ways`。
C2 实际用例：`aoi::tests::random_operations_match_bruteforce_scan`。其它目标的 0 tests/filtered 输出不是额外通过用例。

## 证据索引

- [L1：run-aoi.log](../../target/round53/aoi-audit/run-aoi.log)：完整 C1 stdout/stderr。
- [L2：run-bruteforce.log](../../target/round53/aoi-audit/run-bruteforce.log)：完整 C2 stdout/stderr（含所有被过滤目标）。
- [meta.txt](../../target/round53/aoi-audit/meta.txt)：命令、TMP/TEMP、HEAD、起止、exit、日志哈希。
- [evidence.json](../../target/round53/aoi-audit/evidence.json)：机器可读实际用例列表、命令结果、路由结论和覆盖边界。
- [00-summary.md](../../target/round53/aoi-audit/00-summary.md)：独立摘要与阻断、关闭建议。
- 基准说明：[round53-plan.md](round53-plan.md) §二目标4；[round53-shangshu-dispatch.md](round53-shangshu-dispatch.md) §四A；[aoi.md](../world/aoi.md) 所列范围已与实现/真实测试核对。

