# R54 承接台账：R53 十九项 PARTIAL / 待补项

**来源**：`docs/process/round53-shangshu-acceptance.md:29-47`（R53 尚书验收「PARTIAL 逐条承接 R54」
十九项，R52 十二项编号不重排 + AOI 五路由 + 文档顺序 + 路径漂移）。上游精确出处见各条。
**判定口径**：只有本轮有**具名测试 + 可复现命令 + 原始 exit** 的项才可写「关闭」；只补了部分
子项的写「缩窄/部分关闭」；未执行本轮验证的一律「承接」，不以历史绿灯回填。**PARTIAL 不写 PASS**。
**本轮统计：6 项关闭（13/14/15/17/18/19）、13 项承接**（其中 R52-4 缩窄、AOI-5 仅 NPC 子项承接）。

| # | 承接项（R53 编号） | R54 结论 | 本轮证据 / 原始 exit |
|---|---|---|---|
| 1 | R52-1 125 ObjectProjectile 无服务端生产者 | 承接 | 本轮未执行 125 相关验证 |
| 2 | R52-2 真 socket / 真实客户端窗口 | 承接 | 本轮未执行（无客户端 harness） |
| 3 | R52-3 原生 dash 视觉/音频/时序、Walking 借帧 | 承接 | 本轮未执行（无客户端窗口） |
| 4 | R52-4 Frost 精确 +2 正侧、同会护栏、完整 PK 矩阵 | **缩窄** | B 组 `tests/round54_frost_edges.rs` 3 passed / exit 0；PK 矩阵、G1/G6、EnemyGuild 仍开放 |
| 5 | R52-5 G1/G6 | 承接 | 本轮未执行；R51 B4 补证保留 |
| 6 | R52-6 D3 离线/异图清场/丢 pending | 承接 | 本轮未执行 |
| 7 | R52-7 D2 Trap 外围坐标剪裁 | 承接 | 本轮未执行 |
| 8 | R52-8 FireWall26 B3 撤销断言 | 承接 | 本轮未执行 |
| 9 | R52-9 无 InTrapRock | 承接 | 本轮未执行 |
| 10 | R52-10 B4 pending/传送/耗蓝 | 承接 | 本轮未执行 |
| 11 | R52-11 guild 嵌套槽/旧技能耗材、EnemyGuild | 承接 | 本轮未执行；B 组明确不扩展 EnemyGuild |
| 12 | R52-12 首次红原件保全 | 承接 | 本轮新增首红保全（`private-lib-run-1.log`、B 两份 selfcheck 101→0）；R52 历史原件仍缺 |
| 13 | AOI-1 进图 | **关闭** | `aoi_acceptance_enter_snapshot` + 私有例，exit 0（10/10 执行矩阵） |
| 14 | AOI-2 离图 | **关闭** | `aoi_acceptance_leave_cleanup` + 私有例，exit 0 |
| 15 | AOI-4 切图 | **关闭** | `aoi_acceptance_change_map_sets` + 私有例，exit 0 |
| 16 | AOI-5 回收（含 NPC） | **部分关闭** | 物品/金币/尸体子项 PASS；NPC 子项 PARTIAL（生产无入口，未手删容器） |
| 17 | AOI-6 广播 | **关闭** | `aoi_acceptance_broadcast_identity_order` + 私有例，exit 0 |
| 18 | 文档顺序（`aoi.md:155/:175`） | **关闭** | 本轮勘误：空间序/ID 序/batching 口径 + §8 第 8 条首帧分组 |
| 19 | 路径漂移 | **关闭** | 实查 `target/round53/frost-pvp/00-meta.md`、`target/round53/final-gate-summary.log`（11001 B）、`docs/testing/round53/final-manifest.json` 均存在 |

---

## 1. R52-1 125 ObjectProjectile 无服务端生产者

- **来源**：`docs/process/round53-shangshu-acceptance.md:29`；R52 侧 `docs/testing/round52/README.md`。
- **R53 原结论**：125 无服务端生产者；手造负向包不闭合生命周期；149/26 子项不外推。
- **R54 动作/证据**：无（本轮 A/B 均不涉 125）。
- **状态**：**承接**；不以 R53 的 125 负向包结论替代生命周期闭合，也不外推 149/26。
- **剩余缺口**：需要服务端 125 生产者路径与完整生命周期证据。

## 2. R52-2 真 socket / 真实客户端窗口

- **来源**：`round53-shangshu-acceptance.md:30`；上游 R52 终审。
- **R53 原结论**：进程内 `World → 编解码 → 快照` 已证；真 socket 与真实客户端窗口仍待验收。
- **R54 动作/证据**：无（本轮无客户端/网络证据）。
- **状态**：**承接**。

## 3. R52-3 原生 dash 视觉/音频/时序、Walking 借帧

- **来源**：`round53-shangshu-acceptance.md:31`。
- **R53 原结论**：原生 dash 视觉/音频/时序与 Walking 借帧等价性未闭合。
- **R54 动作/证据**：无（需真实窗口/音频证据）。
- **状态**：**承接**。

## 4. R52-4 Frost 玩家链（缩窄）

- **来源**：`round53-shangshu-acceptance.md:32`；`docs/process/round53-menxia-confirm.md:20`。
- **R53 原结论**：真实 PvP Frost 的 HP/毒/拒攻恢复已证；**精确等级 +2 正侧、同会真实施法护栏、
  完整 PK 矩阵仍有覆盖缺口**。
- **R54 动作/证据**（`target/round54/frost/attempt-1/`，B 组新文件
  `crates/mir-server/tests/round54_frost_edges.rs`，3 tests，`3 passed; 0 failed;
  0 ignored; 0 measured; 0 filtered out`，exit 0）：
  * 精确 +2：施法者 60 / 目标 62 / FrostCrunch 3；种子 3/5/7/11 各 300 次真实 cast + 延迟 tick
    （`FROST_FLIGHT_MS=700`）共 1200 次。Slow 命中 9/11/12/11 = **43/1200**（miss 1157）、
    Frozen 命中 14/21/14/9 = **58/1200**（miss 1142）；期望 `(level+1)% = 4% → 48`，允许区间
    `[24,72]`（未放宽、未换种子）；总伤害 148800（每种 37200）；Slow duration 4 / tick 1000ms、
    Frozen duration 2 / tick 1000ms。
  * +3 对照（目标 63）：100 次全部受伤、`target_id == 2`、HP 因果、始终无 Slow/Frozen。
  * Group：真实 `group_invite` + `group_join(1,2)` 后 `AttackMode::Group` 拒绝（`target_id==0`、
    `damage==None`、MP 照扣、pending=0、延迟后 HP/毒不变、0 Struck/DamageIndicator）；非队员 3
    正对照命中 124 伤害（含 Struck + DamageIndicator）；落地重检：先成功 cast（pending=1）→
    真实 invite/join(1,3) 入队 → 跨延迟 tick 落地被拒。
  * Guild：`create_guild(1,"R54FrostAlpha")` + `guild_invite/join(1,2)`；`create_guild(3,"R54FrostBeta")`
    为异会；同会拒绝、异会 3 命中 124；落地重检对会外 4 成功 cast → invite/join → 落地被拒。
    `guild_join` 的 `refresh_stats` 把 HP 100000→1364 已记录并重置后再断言。
  * 自检：`world.rs:2521` Group 分支改 `true` → exit 101（party 红，tests:667）；
    `magic.rs:4098` `magic_target_hittable` 短路 → exit 101（guild 红 tests:1162 left 99876/right
    100000、party 红 tests:850）；均还原、`git diff crates` 为空。
  * 回归：`round53_frost_pvp` 7 passed / exit 0、`round52_regression` 8 passed / exit 0、
    `--lib magic` 43 passed; 0 failed; 153 filtered / exit 0。
- **状态**：**缩窄**（原缺口的三项里，精确 +2 正侧与同会/同队真实施法护栏已补）。
- **剩余缺口**：完整 PK 矩阵、G1/G6、EnemyGuild（恒 false）/行会战仍开放；B 组未改生产代码。

## 5. R52-5 G1/G6

- **来源**：`round53-shangshu-acceptance.md:33`；`docs/testing/round52/README.md` G1–G7 表。
- **R53 原结论**：G1/G6 总体 open；根装备耗材/存档/快照仅子项；R51 B4 补证保留。
- **R54 动作/证据**：无。
- **状态**：**承接**。

## 6. R52-6 D3 离线/异图清场/丢 pending

- **来源**：`round53-shangshu-acceptance.md:34`。
- **R53 原结论**：D3 离线/异图安全清场、丢 pending，与 Crystal 留场停伤/保留 ActionList 的差异继续追踪。
- **R54 动作/证据**：无。
- **状态**：**承接**。

## 7. R52-7 D2 Trap 外围坐标剪裁

- **来源**：`round53-shangshu-acceptance.md:35`。
- **状态**：**承接**（安全剪裁差异保留）。

## 8. R52-8 FireWall26 B3 撤销断言

- **来源**：`round53-shangshu-acceptance.md:36`。
- **R53 原结论**：B3 具体 ID 替换/到期已证；C 仅创建/新入场 0 补发，缺撤销断言。
- **R54 动作/证据**：无。
- **状态**：**承接**。

## 9. R52-9 无 InTrapRock

- **来源**：`round53-shangshu-acceptance.md:37`。
- **状态**：**承接**（FireWall 逐对象 ProcessSpell/InTrapRock 延后）。

## 10. R52-10 B4 pending/传送/耗蓝

- **来源**：`round53-shangshu-acceptance.md:38`。
- **R53 原结论**：B4 先施法后 `change_map` 丢 pending、750ms 不传送；反序落当前图；
  89 耗蓝 = 65 + 8×3，TemporalFlux 30% 另分支。
- **R54 动作/证据**：无。
- **状态**：**承接**。

## 11. R52-11 guild 嵌套槽/旧技能耗材、EnemyGuild

- **来源**：`round53-shangshu-acceptance.md:39`。
- **R53 原结论**：guild、1965–1966 嵌套槽来源、旧技能耗材顺延；EnemyGuild 恒 false / 行会战不支持仍 PARTIAL。
- **R54 动作/证据**：无（B 组只补 Group/Guild 的**攻击目标护栏**，不扩展 EnemyGuild/行会战）。
- **状态**：**承接**。

## 12. R52-12 首次红原件保全

- **来源**：`round53-shangshu-acceptance.md:40`；`docs/process/round53-menxia-confirm.md:28`。
- **R53 原结论**：C 首次 lifecycle/fmt 红原件未保全，F1 为反事实重建；B4 首次 fmt/clippy 红日志
  被覆盖，当前绿不可恢复历史。
- **R54 动作/证据**：A 组保留首红 `private-lib-run-1.log`（首跑 `4 passed; 1 failed; 196 filtered`
  exit 101，panic `world.rs:7397` `player AOI order must be (y, x, id)` left `[100, 101]` right
  `[101]`；修正期望后 `5 passed` exit 0）；B 组自检两份红日志 exit 101 → 还原后 exit 0。
  **但 R52 的历史原件仍不可恢复。**
- **状态**：**承接**（保全机制改善，历史缺口不闭合）。

## 13. AOI-1 进图（关闭）

- **来源**：`round53-shangshu-acceptance.md:41`；缺口表 `docs/process/round53-aoi-acceptance-v2.md:59-63`；
  R53 终审 `docs/testing/round53/round53-final-audit.md:26-33`。
- **R53 原结论**：四类完整首帧、自身排除、分组/包序未证；先补具名断言再跑，禁止 0 tests 关闭。
- **R54 动作/证据**：`cargo test -p mir-server --test aoi aoi_acceptance_enter_snapshot -- --nocapture`
  与私有全名执行，各 `1 passed; 0 failed`（12 / 200 filtered），exit 0；真实 `enter`、四类首帧
  精确 ID 集、自身排除、无重复、玩家→怪物→NPC→物品分组与组内 AOI 序。清单与读数见
  [`round54-aoi-acceptance.md`](round54-aoi-acceptance.md) §2。
- **状态**：**关闭**（范围限该断言集）。

## 14. AOI-2 离图（关闭）

- **来源**：`round53-shangshu-acceptance.md:42`；v2 缺口表 `:59-63`。
- **R53 原结论**：双方 seen/索引精确清理、后续广播不可达、重复离图无重复 remove 未证。
- **R54 动作/证据**：`aoi_acceptance_leave_cleanup` + 私有例，exit 0；三方 `visible_players`
  精确集、leaver 恰好一条 `ObjectRemove`、二次 `leave` 返回 `None` 无新包、之后 chat/walk/刷怪不可达
  （[`round54-aoi-acceptance.md`](round54-aoi-acceptance.md) §3）。
- **状态**：**关闭**。

## 15. AOI-4 切图（关闭）

- **来源**：`round53-shangshu-acceptance.md:43`；v2 缺口表 `:59-63`。
- **R53 原结论**：三方 `visible_players` 精确集、旧 NPC 清空、旧图广播隔离未证；补真实资源四例。
- **R54 动作/证据**：`aoi_acceptance_change_map_sets` + 私有例，exit 0；mover/旧/新观察者精确集、
  旧 NPC 清空、`MapChanged` 首包、旧图 chat 负例/新图正例（[`round54-aoi-acceptance.md`](round54-aoi-acceptance.md) §4）。
  真实资源四例仍因缺 `Server.MirDB` 样本提前 return，**未下载素材**，该子项仍见 `maps` 回归边界。
- **状态**：**关闭**（仅限合成地图断言；真实资源数据子项另计入 §8/§9 的缺口说明，不冒充通过）。

## 16. AOI-5 回收（部分关闭）

- **来源**：`round53-shangshu-acceptance.md:44`；v2 `:59-63`；R53 终审 findings。
- **R53 原结论**：普通物品/NPC 回收、尸体回收后精确 AOI/seen 撤销未证；C9 的 `None,1000` 实为金币，
  不算普通物品。
- **R54 动作/证据**：`aoi_acceptance_recycle_all_kinds` + 私有例，exit 0；
  * 普通物品 `Some(UserItem)` 严格 `now > expire_time`：等值保持、+1ms 恰好一条 remove、再 tick
    零重复、`ObjectItem` 帧；
  * 金币对照（`None`、`is_gold`、`ObjectGold`）单列，不冒作普通物品；
  * 真实怪物死亡 → 尸体 → tick 回收，恰好一条 remove、二次 tick 零重复；
  * **NPC 子项 FAIL 于产品缺口**：生产只有 `spawn_npcs`（`npc.rs:687`），无回收/移除入口；
    私有测 `advance_clock(1_000_000)` + 3 ticks 后 NPC 与 AOI 项仍在、无 `ObjectRemove`；
    **未手删容器冒充回收路径**。
- **状态**：**部分关闭**（物品/金币/怪物关闭；NPC 承接）。
- **剩余缺口**：NPC 生产回收/移除入口（产品代码），需另轮实现。

## 17. AOI-6 广播（关闭）

- **来源**：`round53-shangshu-acceptance.md:45`；v2 缺口表 `:59-63`。
- **R53 原结论**：跨图负例、精确接收者身份、完整包序与 Crystal 差异契约未证。
- **R54 动作/证据**：`aoi_acceptance_broadcast_identity_order` + 私有例，exit 0；近(16)/远(17)/
  异图同坐标精确身份集、每接收者完整包向量、`exclude`/`actor` 经真实 `open_door`、batching 逐接收者
  append 序、空间序 `[4,1,3,2]` vs ID 序 `[1,2,3,4]`（[`round54-aoi-acceptance.md`](round54-aoi-acceptance.md) §6）。
- **状态**：**关闭**（单 tick 两帧 batching 的压力边界另见 [`README.md`](README.md) §8，不改写为全量验收结论）。

## 18. 文档顺序（关闭）

- **来源**：`round53-shangshu-acceptance.md:46`；R53 终审 findings 第 3 条（`round53-final-audit.md:69-75`）。
- **R53 原结论**：`docs/world/aoi.md:155` 写「广播按对象 ID 稳定排序」，`:175` 写「同一批内按
  `(y, x, id)` 排序」，与查询空间序/集合差实际不同；首帧分组与 Crystal 插入序也非等价。
- **R54 动作/证据**：本轮勘误 `docs/world/aoi.md` §5（`:155` 起）为「AOI 半径查询按 `(y, x, id)`，
  `exclude`/`actor` 过滤，空间序 ≠ ID 序；batching 逐接收者 append 序、tick flush，不以取包时刻
  推断全局发送序」；§6（`:175` 起）为「先移除后新增、批内对象 ID 升序（`BTreeSet::difference`），
  空间序只用于半径查询」；§8 新增第 8 条「首帧类型分组（玩家→怪物→NPC→物品）」。
- **状态**：**关闭**（文档口径已改，不再声称空间序）。

## 19. 路径漂移（关闭）

- **来源**：`round53-shangshu-acceptance.md:47`；R53 终审 findings 第 5 条。
- **R53 原结论**：F 证据实际在 `target/round53/frost-pvp/`（无 `f/`）；T 清单实际为
  `docs/testing/round53/final-manifest.json`、摘要为 `target/round53/final-gate-summary.log`。
- **R54 动作/证据**：C 实查三路径均存在（`00-meta.md`、11001 B 的 `final-gate-summary.log`、
  `final-manifest.json`）；R54 自己的证据路径统一登记在
  [`README.md`](README.md)，不另造原件。
- **状态**：**关闭**。

---

## 附：本轮未纳入的开放项（按 R53 终审保留）

R49 P05（Blend117/Reverse/Effect/Sorting、4 个 v2、`>800` 缺源，
`docs/testing/round49/round49-xingbu-final-audit.md:113`）、R49 P08（`remote_motion_live` 的
`#[ignore]`，同文 `:116`）、R51 遗留、门远端/精确 5000ms/跑步撞门/初始已开门/同名实例
（`docs/testing/round53/round53-final-audit.md` 末尾）均不在 R54 A/B 范围，状态不变。
