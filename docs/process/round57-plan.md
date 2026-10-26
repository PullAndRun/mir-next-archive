# R57 第57轮方案（中书省 / gpt-6.1-sol）

主单 `JJC-20261026-004`（看板 Menxia）｜标题「第57轮：ObjectProjectile生产者与R56承接清理」｜HEAD=`fe86aaae9fbc8810c166288832eeca80e7bf6adb`=`origin/main`（R56 Done）｜规划 2026-10-26T13:26+08:00（Asia/Shanghai）

只读轮：本文件是本轮唯一写入；未改 `crates/`、未跑 `cargo`/`ci.ps1`、未 commit。R56 主单 `JJC-20261026-003` 已 Done；建单前看板 149 条、未结 0，建单后 150 条、schema v3.1 校验通过。

## 一、事实锚点（文件:行号 / 已读 vs 推断）

| # | 事实 | 依据 | 状态 |
|---|---|---|---|
| 1 | 125=`ObjectProjectile`，字段按 `byte Spell,u32 Source,u32 Destination` 编码；123/125/126/149 分别为 ObjectMagic/ObjectProjectile/RangeAttack/ObjectSpell | `github/Crystal/Shared/Enums.cs:1434` 起计数；`Shared/ServerPackets.cs:3560-3576`（Index=125） | 已读 |
| 2 | 125 唯一生产点是 FireBounce 的**二次弹跳**分支：首跳只排伤害不发 125；后续每跳先广播 125（Source=上一受害者、Destination=下一目标），再排 `dist*50ms` 伤害 | `Server/MirObjects/HumanObject.cs:5811-5836`（`:5826` 广播；`:5827` DelayedAction；`:5817` `delay=dist*50`；`:5825` 首跳 `delay+=500`） | 已读 |
| 3 | 连跳结算：重校验存活/同图/在落点 ±2；`Attacked(...,MAC,false)>0` 才 `LevelMagic`；仅 `target.Race==Monster` 才找下一目标；候选=半径 3 内、非自身、`IsAttackTarget(this)`、`target.CanFly(...)`；随机取一；`--bounce` | `HumanObject.cs:5934-5958` | 已读 |
| 4 | 总命中=level+2，125 条数=level+1（level0→2 命中/1 条 125；level3→5 命中/4 条 125） | 由 `HumanObject.cs:5825` `bounce=magic.Level+2` 与 `:5956` `--bounce` 递推 | 推断（代码推导） |
| 5 | 施法分派：`if (!FireBounce(...)) targetID=0;` 返回 false 仍算施法、仍扣蓝并广播 120/123，只是 targetID=0 | `HumanObject.cs:3765-3766` | 已读 |
| 6 | 初始目标解析：self / `FindObject(targetID,10)`，过滤非 Monster/Player/Hero、Dead、`!IsAttackTarget`、`IsFriendlyTarget` | `HumanObject.cs:3460-3474` | 已读 |
| 7 | FireBounce 默认数值：BaseCost=5、LevelCost=1、MPowerBase=6、PowerBase=10、Range=9；等级 15/18/21，修炼 2000/2700/3500 | `Server/MirEnvir/Envir.cs:1887-1907` | 已读 |
| 8 | 伤害=`(GetAttackPower(MinMC,MaxMC)+GetPower())*GetMultiplier()`；`GetPower=round(MPower/4*(Level+1)+DefPower)` | `Server/MirDatabase/MagicInfo.cs:180-217` | 已读 |
| 9 | `CanFly` 从源格按 8 方向逐格走到目标，越界或 `Cell.Valid==false` 即失败；`Cell.Valid` 仅 `Attribute==CellAttribute.Walk`（直线穿格判定，不是弹道寻路） | `Server/MirObjects/MapObject.cs:321-338`；`Server/MirEnvir/Map.cs:2480-2482` | 已读 |
| 10 | FireBounce 不产生 126(RangeAttack)/149(ObjectSpell)；全仓 FireBounce 仅 125 广播这一处发包 | `grep -rn "FireBounce" github/Crystal/Server` 命中行逐条核对 | 推断（grep） |
| 11 | 客户端 125 仅处理 FireBounce：按 Source 找对象、`CreateProjectile(410,...)`，命中给 Destination 加 Effect(570) 与命中音；其它 Spell 无分支 | `Client/MirScenes/GameScene.cs:4687-4713` | 已读 |
| 12 | 本作协议层已完备：`ObjectProjectile` 结构/Index=125/编解码顺序与 Crystal 一致；已有全包 roundtrip 与 125 边界枚举样本 | `crates/mir-protocol/src/packets/server.rs:3903-3929`、`:10700`、`:11464` | 已读 |
| 13 | 服务端零生产者：`SpellKind`/`spell_kind` 无 FireBounce，`REGISTERED_UNSUPPORTED` 也未登记；无 125 发送点 | `crates/mir-server/src/magic.rs:647-728`、`:329-338`；`grep -rn "ObjectProjectile" crates/mir-server/src` = 0 | 已读 |
| 14 | 可复用的延迟结算骨架：`PendingMagic{at,caster_id,spell,effect}`、`resolve/apply_pending_magic`、`MagicEffect::Damage` 分支已做 ±2 命中判定、MAC 伤害与 `level_magic`；可新增 bounce 链变体 | `crates/mir-server/src/magic.rs:901-910`、`:2382`、`:2400`、`:2410-2424`、`:2679-2701`、`:2723-2740` | 已读 |
| 15 | `Cell.Valid` 等价物已存在：`poison_cloud_cell_valid`（界内+`walkable`）正是 `Cell.Valid`；`walkable_on` 额外含占城阻挡，不能直接用 | `crates/mir-server/src/magic.rs:3534-3547`；`crates/mir-server/src/world.rs:5395-5407` | 已读 |
| 16 | 客户端无 125 生产消费路径：`net.rs` 只映射/处理 ObjectMagic，125 仅出现在 `round52_spells_wire` 的“解码后不移除 spell_field”负例；任务书“客户端已消费”不成立 | `crates/mir-client/src/net.rs:4498`、`:5420-5440`；`crates/mir-client/tests/round52_spells_wire.rs:740-760` | 已读 |
| 17 | R56 承接：P2 `world.rs:1378` 注释首字节 0x07(BEL)；R-1 `round56-plan.md:45` 证据路径错；R-3 `aoi.md:291` 写“待 R56”；README 无 R55/R56/R57 行、roadmap 有 R55 maps 脚注待统一；P4 门禁 meta 无候选文件 sha、P6 round56 README 受测 HEAD 写旧值 | `docs/testing/round56/round56-final-audit.md`、`docs/process/round56-shangshu-acceptance.md`、`docs/world/aoi.md:291`、`docs/process/round56-plan.md:45`、`docs/roadmap.md:143` | 已读 |
| 18 | 默认 `MagicInfo` 数据源（二进制 DB）是否含 spell=154 的记录未知；若缺，测试须显式注入 fixture，生产学习路径记 PARTIAL | `crates/mir-db/src/magic.rs:24-78` 只定义格式，无内建记录 | 推断（待 A0 实测） |

## 二、本轮可证伪目标

### G1（主功能：FireBounce 真实生产者与连跳语义）
在 `crates/mir-server/src/magic.rs` 注册 `SpellKind::FireBounce`（不再落 Unsupported）：初始目标须为可攻击的 Monster/Player；施法包序为 120 `S.Magic` + 123 `ObjectMagic`，**不发 125**；首跳伤害排期 `500 + MaxDistance(caster,target)*50` ms；命中走 MAC、`dealt>0` 才 `level_magic`；受害者仅为 Monster 时，在其半径 3 内按 Crystal 四条件选下一目标，**先广播 125（source=受害者，destination=下一目标），再排 `dist*50` ms 伤害**；总命中数上限 `level+2`；无候选/达上限即停。FireBounce 全程不得发 126/149。
- 证伪：任一测试红、或 125 出现在首跳、或命中数/包数不等于 level+2 / level+1、或 FireBounce 产生 126/149 → G1 FAIL。
- 证据：`target/round57/firebounce/attempt-1/**`。

### G2（包序与协议/客户端不回归）
125 的字段与顺序严格为 `Spell(u8),Source(u32),Destination(u32)`；同一次连跳中 125 必须早于该跳伤害生效；120→123 在施法时一次性发出；协议层既有 roundtrip 与 125 边界样本保持绿；客户端既有 125 解码负例保持绿（本轮不新增客户端渲染）。
- 证伪：包序断言失败、字段颠倒、或 `mir-protocol`/`mir-client` 既有用例红 → G2 FAIL。

### G3（R56 承接清理，B 批）
①`crates/mir-server/src/world.rs:1378` 仅删除注释中的 0x07(BEL) 字节（不改任何代码/行为，diff 仅该注释）；②`docs/world/aoi.md:291` “待 R56”→“待 R57+”；③`docs/process/round56-plan.md:45` 证据路径改为实际存在的 `target/round56/docs/attempt-1/00-evidence-greps.log`；④`README.md` 增 R55/R56/R57 状态行并给 R56 受测 HEAD 双标注，`docs/roadmap.md` 增 R56/R57 段并统一 R55 maps 脚注；⑤新建 `docs/testing/round57/README.md`（≤60 行，只写“已落盘/待 T 抽核”，**不得预写 PASS**）。
- 证伪：`git diff` 出现行为变更、链接检查红、或文档出现“R57 完整/125 已全链交付”等未证表述 → G3 FAIL。

### G4（门禁、终审与提交绑定）
Lead 唯一执行 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale` 12/12 exit 0；attempt 原件 `target/round57/gate/attempt-N.log` 首红不覆盖，meta 必须含受测 `HEAD`、`git diff` 的 sha256、候选文件逐一 sha256、日志 bytes+sha、起止时区与 `exit_code`（R56 P4 修复）；T 在门禁后只读终审并落 `round57-final-audit.md`+`final-manifest.json`（R56 R-2 修复）；代码批/文档批两次提交，提交后 blob sha 与门禁候选 sha 一致。
- 证伪：任一 attempt 缺字段、红 attempt 被覆盖、提交内容与门禁候选不一致 → G4 FAIL。

## 三、子任务 DAG（唯一写者）

| key | owner | depends_on | 写范围 | 产出/验收 |
|---|---|---|---|---|
| A | 兵部，deepseek-flash | 门下 approve（D1/D2） | `crates/mir-server/src/magic.rs`、`crates/mir-server/tests/round57_firebounce.rs`、`docs/world/magic.md`（仅 FireBounce 支持矩阵行） | G1/G2；A0 先实测默认 DB 是否有 FireBounce，缺则测试 fixture 注入并记 PARTIAL |
| B | 礼部，gpt | 门下 approve（D4） | `crates/mir-server/src/world.rs`（仅 :1378 注释）、`docs/world/aoi.md:291`、`docs/process/round56-plan.md:45`、`README.md`、`docs/roadmap.md`、`docs/testing/round57/README.md` | G3；不碰 A 的文件，不做行为变更 |
| T | 刑部，gpt | G 门禁完成 | `docs/testing/round57/round57-final-audit.md`、`docs/testing/round57/final-manifest.json` | 只读抽核三产物（G1/G2/G3 证据、门禁 meta、diff），逐条 PASS/PARTIAL |
| G | Lead | A+B 完成 | `target/round57/gate/**`（唯一） | G4；全量门禁、首红保留、meta 落盘 |

A 与 B 可并行；**A 不写 `world.rs`，B 不写 `magic.rs`**；T 在 G 后启动；任何代码/文档改动发生在门禁之后都必须重跑 attempt-N+1。

## 四、验收命令（逐条含预期与退出码）

1. `cargo test -p mir-server --test round57_firebounce -- --list` → 恰好 6 行 `: test` + `6 tests, 0 benchmarks`，exit 0；数量不符即范围失守。
2. `cargo test -p mir-server --test round57_firebounce` → `6 passed; 0 failed`，exit 0。
3. `cargo test -p mir-server --test round57_firebounce fire_bounce_emits_125_before_each_secondary_hit_and_hits_level_plus_two_targets -- --exact` → `1 passed; 0 failed; 5 filtered out`，exit 0。
4. `cargo test -p mir-server --test magic` → `0 failed`，exit 0，passed 数 ≥ A 首跑基线（写入 `00-baseline.log`，后续不得下降）。
5. `cargo test -p mir-server --test round52_poison_cloud` → `0 failed`，exit 0（复用 `cell_valid` 的回归）。
6. `cargo test -p mir-protocol --lib default_packets_roundtrip -- --exact` → `1 passed; 0 failed`，exit 0。
7. `cargo test -p mir-client --test round52_spells_wire` → `0 failed`，exit 0（125 解码负例不回归）。
8. `grep -c "ServerPacket::ObjectProjectile" crates/mir-server/src/magic.rs` → ≥1；`grep -c "SpellKind::FireBounce" crates/mir-server/src/magic.rs` → ≥2；两条 exit 0。
9. `git diff --check` → 无输出，exit 0。
10. `py -3 tools/docs/check_links.py` → 末行 `[OK]`，exit 0。
11. `wc -l < docs/process/round57-plan.md` → ≤200；`grep -c "^| R57" README.md` → 1；`grep -c "待 R56" docs/world/aoi.md` → 0；`grep -c "target/round56/aoi/attempt-1/10-wiring-greps.log" docs/process/round56-plan.md` → 0；各 exit 0。
12. `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale` → 12/12 全绿，exit 0；任一红保留该 attempt 并以新 attempt 重跑。
13. `git status --porcelain` → 提交后仅 2 个历史冻结 JSON（`docs/testing/monster-native/player-resource-freeze*.json`），无其他改动，exit 0。

测试清单（A 必须同名落盘）：`fire_bounce_cast_emits_magic_and_object_magic_without_projectile`、`fire_bounce_first_hit_lands_after_distance_delay`、`fire_bounce_emits_125_before_each_secondary_hit_and_hits_level_plus_two_targets`、`fire_bounce_secondary_target_filter_and_random_choice`、`fire_bounce_chain_stops_without_nearby_monster_or_bounce_exhausted`、`fire_bounce_does_not_emit_126_or_149_and_ignores_players_for_chain`。

## 五、证据路径（单文件 ≤1 MiB）

- `target/round57/firebounce/attempt-1/{00-baseline.log,01-list.log,02-suite.log,03-exact.log,04-magic-regression.log,05-poison-regression.log,06-protocol.log,07-client.log,08-greps.log,meta.json,files.json}`。
- `target/round57/docs/attempt-1/{00-before-greps.log,01-after-greps.log,02-links.log,03-diff-check.log,meta.json,files.json}`。
- `target/round57/gate/attempt-N.log` + `attempt-N.meta.json`（首红 N 保留，不覆盖；最终绿 attempt 与提交绑定）；`target/round57/commit/verify.log`。
- `meta.json` 必含：`head`、`git_diff_sha256`、`files[{path,sha256,bytes}]`、`log_sha256`、`log_bytes`、`started_at/finished_at`（含时区）、`exit_code`、`attempt`。超过 1 MiB 的日志用 `split -b 1M` 分片并记录分片 sha，禁止只留尾部。

## 六、提交策略（门禁绑定）

1. 门禁在**未提交工作树**上跑；G 先生成 `git diff > target/round57/commit/candidate.patch` 并记录 sha256 与每个候选文件 sha。
2. 提交一（编码批）：`crates/mir-server/src/magic.rs`、`crates/mir-server/tests/round57_firebounce.rs`、`crates/mir-server/src/world.rs`（仅 BEL 注释）、`docs/world/magic.md`。提交二（文档批）：`docs/process/round57-plan.md`、`docs/process/round56-plan.md`、`docs/world/aoi.md`、`README.md`、`docs/roadmap.md`、`docs/testing/round57/**`（含 T 审计与 manifest）。
3. 两次提交后 `git show --stat` 与 `git diff HEAD~2..HEAD` 的 blob sha 必须等于门禁候选 sha；不等则门禁作废、重跑。
4. 提交信息标注 `R57` 与主单 ID；禁 amend/rebase 已推送提交；禁把 2 个冻结 JSON 加入索引。

## 七、风险、PARTIAL 口径与待裁决

**风险（按概率×影响）**
1. 连跳递归引入 RNG/时序不确定：候选目标选择必须走世界 `MirRng` 且测试固定种子；候选排序后取样，保证可复现。
2. 默认 DB 无 FireBounce 记录：A0 首测；缺则仅测试 fixture，生产学习/掉落路径记 PARTIAL，不得伪造 DB 记录。
3. 初始目标非法时 Crystal 仍扣蓝并广播 targetID=0，本作既有「先校验后扣蓝」是全系统差异；本轮不为此改公共流程，登记继承差异。
4. 客户端 125 无生产消费：服务端+协议闭环可验收，客户端弹道/命中特效 PARTIAL，禁止写“全链交付”。
5. B 批触碰 `world.rs` 注释与“编码仅 deepseek”边界冲突（见 D4）；若门下否决，P2 顺延或交 A 在自身批次内清理。
6. 门禁后改文件导致候选 sha 失配：以 `git diff` sha 为准强制重跑，不接受口头一致。

**PARTIAL 口径（本轮结束必须逐条落审计）**
- 已实现：服务端 FireBounce 生产者、125 包序、连跳选择/上限/伤害/MAC/升级。
- PARTIAL：客户端 125 原生弹道与 410/570 特效；Crystal 无效目标“仍扣蓝”语义；默认 DB 缺记录时的生产学习路径；次级目标随机序列顺序（集合等价、非逐次同序，MINOR）。
- 不写“125 已全链完成”“R57 完整验收”。Crystal 无字段/无锚点的行为一律 PARTIAL，不发明。

**不做清单**
不改协议结构/包号；不实现 126/149；不做客户端渲染；不重构 `magic.rs` 其它技能；不新增技能；不动 2 个冻结 JSON；不跑重复全量门禁（仅 Lead 一次）；规划阶段不 commit/push。

**待门下裁决**
- **D1** FireBounce 范围：建议**全链（level+2 命中，125=level+1）**；若判单轮风险高，退为“首跳+1 次弹跳”，其余记 PARTIAL。请裁决并写入 memo。
- **D2** 随机选择口径：建议按对象 ID 排序后取 `rng % len`（可复现）；与 Crystal 链表序分布不同但候选集合/过滤条件一致，是否接受为 MINOR？
- **D3** 客户端 125 原生渲染是否本轮必须？建议不做：本轮只闭环服务端生产者+协议包序，客户端渲染登记 PARTIAL。
- **D4** B 批由礼部改 `world.rs:1378` 注释（零行为）是否放行？建议放行并归编码批提交；否则交 A 顺带清理或 P2 顺延。
