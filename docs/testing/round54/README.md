# 第 54 轮（R54）证据索引

**状态：🟡 进行中，有界。** AOI 五路由 **4 PASS / 1 PARTIAL / 0 FAIL**；Frost 精确 +2 正侧与
Group/Guild 真实护栏已补；**AOI 全量验收仍不可关闭**（回收路由 NPC 子项无生产回收入口）；
**R54 全量门禁未运行**，本轮状态记 **PARTIAL**，不得以定向绿冒充全量门禁结果。

- 仓库：`C:\Users\diana\Documents\github\mir-next`；HEAD `08572ae8ca2420c95a8ecf02977b52c64241f235`
  （`main` = `origin/main`）。
- 工作树：A/B 两条测试线**未提交**（`M crates/mir-server/tests/aoi.rs`、`M crates/mir-server/src/world.rs`、
  `?? crates/mir-server/tests/round54_frost_edges.rs`）；生产代码零改动；未 commit/push。
- 本索引与三份文档只记录已发生的读数与 exit；未跑的命令一律写「未运行」。

## 1. 交付面

| 文档 | 内容 |
| --- | --- |
| [`round54-aoi-acceptance.md`](round54-aoi-acceptance.md) | A 组五路由逐项：Crystal→Rust 锚点、测试名、命令、发现/执行数、原始 exit、断言与 PASS/PARTIAL 口径 |
| [`round54-carryover.md`](round54-carryover.md) | R53 十九项 PARTIAL 承接台账（逐项来源/动作/断言/本轮结论/依据） |
| [本文件](README.md) | 证据索引、候选哈希、A/B 实数、M5 矩阵、门禁状态与残余风险 |

## 2. 证据索引

| 路径 | 内容 |
| --- | --- |
| `target/round54/aoi/attempt-1/00-summary.md` | A 线摘要：10/10 执行矩阵、变异自检、fmt/clippy 机械修复 |
| `target/round54/aoi/attempt-1/m2-matrix.log` / `m2-summary.txt` / `m2-discovered.txt` | 五前缀 × 两目标 `--list` + `--nocapture`，20 条命令逐段 `START/CMD/EXIT` |
| `target/round54/aoi/attempt-1/regression-{aoi-full,lib-aoi-grid,maps}.log` / `regression-summary.txt` | 回归 13 / 7 / 20（16 有效）原始日志 |
| `target/round54/aoi/attempt-1/mutation/mutation-selfcheck.log`、`aoi.rs.baseline`、`world.rs.baseline` | 5×2 行级变异转红 → 还原 `cmp` 字节一致 |
| `target/round54/aoi/attempt-1/fix/` | fmt/clippy 机械修复、只格式化证明、post-fmt 变异/复跑 |
| `target/round54/aoi/attempt-1/private-lib-run-1.log` | 首红原件（`4 passed; 1 failed` exit 101）+ 修正后绿复跑 |
| `target/round54/aoi/attempt-1/{meta.json,evidence.json}` | HEAD / 时间 / 命令 / 哈希机器可读事实 |
| `target/round54/frost/attempt-1/00-summary.md` | B 线摘要：精确 +2、Group/Guild 护栏、自检、复用 harness |
| `target/round54/frost/attempt-1/{02-list,03-run,04-round53-frost-pvp,05-round52-regression,06-lib-magic}.log` | 3 例清单/执行与 7/8/43 回归原始日志 |
| `target/round54/frost/attempt-1/selfcheck-0{1-group-arm,2-landing-recheck}.log` | 两处生产变异转红证据（已还原） |
| `target/round54/frost/attempt-1/fix/` | fmt/clippy 机械修复与 post-fix 复跑（1185 行、sha256 见 §3） |
| `target/round54/lead-verify2/00-meta.txt`、`03-aoi.log`、`04-aoi-lib.log`、`05-frost.log`、`06-frost53.log`、`08-lib-magic.log`、`09-maps.log`、`10-i18n-with.log`、`12-summary.log` | Lead 门禁预检独立复跑原始输出（原始命令见 `lead-verify2.sh`） |

## 3. 候选文件与哈希

| 文件 | 增行 | 当前行数 | sha256 |
| --- | ---: | ---: | --- |
| `crates/mir-server/tests/aoi.rs` | +892 | 1436 | `e6b4ffc3374843e686d314766b78c40ebd6f20dda5c0e3190b9eda5cca3480f3` |
| `crates/mir-server/src/world.rs` | +922 | 8097 | `cb18f27f5f0262213c2d0a7a193910bc598a53e4ea7d90725f01c22f8438a892` |
| `crates/mir-server/tests/round54_frost_edges.rs` | +1185（新文件） | 1185 | `727718288c8c6380f98d87c6ad276049d68a69926c7f00c829165638b9a4d09b` |

## 4. A 组（AOI 五路由）命令与实数

命令形状（每条路由 4 条，共 20 条，全部 `EXIT: 0`）：

```text
cargo test -p mir-server --test aoi <prefix> -- --list
cargo test -p mir-server --test aoi <prefix> -- --nocapture
cargo test -p mir-server --lib <prefix> -- --list
cargo test -p mir-server --lib world::tests::<prefix>_private -- --nocapture
```

- 五前缀：`aoi_acceptance_{enter_snapshot,leave_cleanup,change_map_sets,recycle_all_kinds,broadcast_identity_order}`。
- 发现/执行：集成 5/5、私有 5/5，`TOTAL_DISCOVERED=10 TOTAL_EXECUTED=10 OVERALL_FAIL=0`；0 tests 不计通过。
- 回归：`--test aoi` 13 passed、`--lib aoi::tests` 7 passed（194 filtered）、`--test maps` 20 passed
  （4 例缺 `resource/Server.MirDB` 提前 return，**有效 16**），均 exit 0。
- 变异自检：集成/私有各 5 处行级变异 → `0 passed; 5 failed` exit 101；还原后各 5 passed、exit 0，
  `cmp` 证明字节一致；fmt 后当前字节重跑同样转红。
- 矩阵：**4 PASS / 1 PARTIAL / 0 FAIL**；PARTIAL 仅 `recycle_all_kinds` 的 NPC 子项
  （生产无 NPC 回收/移除入口，**未手删容器冒充**）。

## 5. B 组（Frost 正侧/护栏）命令与实数

```text
cargo test -p mir-server --test round54_frost_edges -- --list
cargo test -p mir-server --test round54_frost_edges -- --nocapture
```

- 3 例全绿、exit 0（`3 passed; 0 failed; 0 ignored; 0 filtered`）：
  `round54_frost_exact_plus_two_probability`、`round54_frost_party_real_cast_guard`、
  `round54_frost_guild_real_cast_guard`。
- 精确 +2（施法 60 / 目标 62 / 技能 3，种子 3/5/7/11 × 300 次真实 cast + 延迟 tick = **1200 次**）：
  Slow 命中 **43/1200**（miss 1157）、Frozen **58/1200**（miss 1142）；期望 4% → E=48，
  允许区间 **[24,72]**（未放宽、未换种子）；总伤害 **148800**；Slow duration 4 / tick 1000ms，
  Frozen duration 2 / tick 1000ms。
- +3 对照（目标 63）：**100 次**真实 cast 全部受伤、`target_id==2`、HP 因果成立、**零 Slow/Frozen**。
- Group：真实 `group_invite` + `group_join(1,2)` → `AttackMode::Group` 拒绝（`target_id==0`、
  `damage==None`、MP 照扣、pending=0、延迟后 HP/毒不变、0 伤害包）；非队员 3 正对照命中 **124** 伤害。
  落地重检：先成功 cast（pending=1）→ 真实入队 → 跨延迟 tick 落地被拒。
- Guild：`create_guild(1,"R54FrostAlpha")` + `guild_invite/join(1,2)`；异会 `create_guild(3,"R54FrostBeta")`；
  同会拒绝、异会 3 命中 **124**；落地重检对会外 4 成功 cast → 入会 → 落地被拒。`guild_join` 的
  `refresh_stats` 把 HP 100000→1364 已记录并重置后再断言。
- 回归：`round53_frost_pvp` 7 passed、`round52_regression` 8 passed、`--lib magic` 43 passed，均 exit 0。
- 自检：`world.rs:2521` Group 谓词改 `true` → exit 101；`magic.rs:4098` 落地重检短路 → exit 101；
  均还原、`git diff crates/mir-server/src` 为空。

## 6. 门禁状态（不得夸大）

- Lead 预检：`cargo fmt --all --check` exit 0；`cargo clippy -p mir-server --all-targets -- -D warnings`
  exit 0；i18n 扫描 **66/0** exit 0；原始输出 `target/round54/lead-verify2/**`。
- **`tools/dev/ci.ps1` 全量门禁未运行**；上述只是预检 + 定向复跑，R54 状态因此记 **PARTIAL**。
- 未跑：真实资源 `maps` 4 例（缺样本）、真实客户端窗口/真 socket、全量回归、提交后 CI。

## 7. M5 矩阵（R53 十九项 → R54）

统计：**关闭 6 项（13/14/15/17/18/19）、承接 13 项（其中 R52-4「缩窄」、AOI-5「部分关闭」）**；
逐项来源/断言/依据见 [`round54-carryover.md`](round54-carryover.md)。哈希列取 §3 工作树内容前 12 位
（`e6b4ffc33748` = `tests/aoi.rs`、`cb18f27f5f02` = `src/world.rs`、`727718288c8c` =
`tests/round54_frost_edges.rs`、`ee0d029f2ec8` = `docs/world/aoi.md`）。「原 exit」列只写
attempt-1 命令实际 exit；未执行项写「—」，不得以历史结论回填。

| # | R53 承接项 | 资源 | 动作 | 断言 | 有效数 | 原 exit | HEAD | 候选文件 sha256 | 结论 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | R52-1 125 ObjectProjectile 无生产者 | R52/R53 文档 | 未执行 | 125 生命周期闭合 | — | — | 08572ae | — | 承接 |
| 2 | R52-2 真 socket / 客户端窗口 | R52 文档 | 未执行 | 跨进程/真实窗口 | — | — | 08572ae | — | 承接 |
| 3 | R52-3 原生 dash、Walking 借帧 | R52 文档 | 未执行 | 视觉/音频/时序等价 | — | — | 08572ae | — | 承接 |
| 4 | R52-4 Frost 精确 +2 / 同会护栏 / PK 矩阵 | `tests/round54_frost_edges.rs` | `--test round54_frost_edges -- --nocapture` | 精确 +2、同队/同会拒绝、异会命中、落地重检 | 1200 次 cast + 3 组护栏 | 0 | 08572ae | `727718288c8c` | **缩窄** |
| 5 | R52-5 G1/G6 | R52 文档 | 未执行 | G1/G6 收口 | — | — | 08572ae | — | 承接 |
| 6 | R52-6 D3 离线/异图清场 | R52 文档 | 未执行 | 清场/丢 pending 契约 | — | — | 08572ae | — | 承接 |
| 7 | R52-7 D2 Trap 外围裁剪 | R52 文档 | 未执行 | 坐标剪裁安全 | — | — | 08572ae | — | 承接 |
| 8 | R52-8 FireWall26 撤销断言 | R52 文档 | 未执行 | 撤销/到期断言 | — | — | 08572ae | — | 承接 |
| 9 | R52-9 无 InTrapRock | R52 文档 | 未执行 | 无 InTrapRock 等价 | — | — | 08572ae | — | 承接 |
| 10 | R52-10 B4 pending/传送/耗蓝 | R52 文档 | 未执行 | pending/750ms/89 耗蓝 | — | — | 08572ae | — | 承接 |
| 11 | R52-11 guild 嵌套槽/EnemyGuild | R52 文档 | 未执行 | guild 槽位、EnemyGuild | — | — | 08572ae | — | 承接 |
| 12 | R52-12 首次红原件保全 | R52 文档 + 本轮日志 | 未执行（仅保全改善） | 历史首红原件 | 1 份首红 | 101→0 | 08572ae | — | 承接 |
| 13 | AOI-1 进图 | `tests/aoi.rs` + `src/world.rs` | 4 命令（`--list` + `--nocapture` ×2） | 四类首帧、自身排除、分组包序 | 2（集成+私有） | 0 | 08572ae | `e6b4ffc33748`+`cb18f27f5f02` | **关闭** |
| 14 | AOI-2 离图 | 同上 | 4 命令 | 双向 seen/索引、重复 leave、后续不可达 | 2 | 0 | 08572ae | 同上 | **关闭** |
| 15 | AOI-4 切图 | 同上 | 4 命令 | 三方集合、旧 NPC、旧图广播隔离 | 2 | 0 | 08572ae | 同上 | **关闭** |
| 16 | AOI-5 回收（物品/金币/尸体/NPC） | 同上 | 4 命令 | 严格 `>` 到期、单 remove、NPC 子项 | 2 | 0 | 08572ae | 同上 | **部分关闭**（NPC PARTIAL） |
| 17 | AOI-6 广播 | 同上 | 4 命令 | 身份集、每接收者完整包序、batching | 2 | 0 | 08572ae | 同上 | **关闭** |
| 18 | 文档顺序（`aoi.md:155/:175`） | `docs/world/aoi.md` | 本轮勘误 §5/§6/§8.8 | 空间序 ≠ ID 序、首帧分组 | — | — | 08572ae | `ee0d029f2ec8` | **关闭** |
| 19 | 路径漂移 | R53 实查路径 | 实查三路径 | 路径存在性 | 3 路径 | — | 08572ae | — | **关闭** |

## 8. 残余风险

- AOI 回收 NPC 子项是**产品缺口**：生产只有 `spawn_npcs`，无回收/移除入口；须另轮实现后重跑。
- 批处理仅压「单 tick 两帧」，未压多 tick/多接收者/跨图混合。
- 私有例耦合 `visible_*`/`aoi_*` 内部字段与合成 `MirDb`；生产重构需同步。
- A 集成首跑失败未留原始日志（证据保全缺口），同类断言由变异自检证明可失败。
- `maps` 20 reported ≠ 20 有效（4 例缺样本提前 return）。
- 全量门禁未跑、未 commit；本索引不构成「AOI 已全量验收」或「R54 完成」结论。

## 9. 相关文档

- [AOI 实现契约](../../world/aoi.md)（§5/§6 排序口径、§8 差异、§11 R54 验收状态）
- [五路由验收](round54-aoi-acceptance.md) · [R53 承接台账](round54-carryover.md)
- [R53 尚书验收](../../process/round53-shangshu-acceptance.md) · [R53 终审](../round53/round53-final-audit.md)
- [路线图（视野与广播条目）](../../roadmap.md)
