# R57 门下省评审（D）｜JJC-20261026-004

- **受审 HEAD**：`fe86aaae9fbc8810c166288832eeca80e7bf6adb` = `origin/main`（R56 Done，实测 `git rev-parse`）
- **受审方案**：`docs/process/round57-plan.md` 114 行 / 15,833 B / sha256 `f5e91513135a1fbd70927f26998504c8a15b3fbdf9274d4234f8c109cf348e0a`（实测复算全等）
- **评审角色**：menxia / D（gpt-6.1-sol，只读；不写 `crates/`、不跑 `cargo`/`ci.ps1`、不 commit）
- **评审时间**：2026-10-26T13:33–13:52+08:00
- **结论：PASS-WITH-CHANGES**（必改 M1–M5；0 项 BLOCK；0 项方案级不可行）

## 一、锚点抽查（18 组：Crystal 8 / Rust 10）

判定口径：“实测”= 读数与方案字面一致；“偏差”= 语义成立但行号/区间不准；“不符”= 语义与 Crystal 不一致。

| # | 方案主张 | 实测依据 | 判定 |
|---|---|---|---|
| C1 | 125=`ObjectProjectile`，字段 `byte Spell,u32 Source,u32 Destination`；123/125/126/149 对应 ObjectMagic/ObjectProjectile/RangeAttack/ObjectSpell | `Shared/Enums.cs:1434` 起枚举，`ObjectMagic`:1559、`ObjectProjectile`:1561、`RangeAttack`:1562、`ObjectSpell`:1585，减 1436 得 123/125/126/149；`Shared/ServerPackets.cs:3560-3580` 字段顺序一致 | **实测** |
| C2 | 首跳不发 125、后续跳先广播再排 `dist*50`；`delay+=500` 仅首跳 | `Server/MirObjects/HumanObject.cs:5811-5833`：`:5817` `delay=MaxDistance*50`、`:5819-5823` `bounce=Level+2; delay+=500`、`:5826` 广播、`:5829` DelayedAction | **实测（子行号偏差）**：方案写 `:5827` DelayedAction（实为 `:5829`）、`:5825` `delay+=500`（实为 `:5822`） |
| C3 | 连跳结算 M1–M6 全部语义 | `HumanObject.cs:5942` 重校验/±2、`:5943` `Attacked(...,MAC,false)>0` 才 `LevelMagic`、`:5945` 仅 Monster 续跳、`:5947` `FindAllNearby(3)`+三过滤、`:5951` 随机、`:5953` `--bounce` | **实测** |
| C4 | `CanFly` 8 方向逐格、越界或 `Cell.Valid==false` 失败；`Valid` 仅 Walk | `MapObject.cs:321-337`（`:331` 界内、`:333` `GetCell.Valid`）；`Map.cs:2480-2483` `Attribute==CellAttribute.Walk` | **实测** |
| C5 | 初始目标 self/`FindObject(targetID,10)`，过滤非 Monster/Player/Hero、Dead、`!IsAttackTarget`、`IsFriendlyTarget` | `HumanObject.cs:3460-3477`：`:3462-3468` self/FindObject、`:3471-3474` 种族过滤、**`:3476`** Dead/IsAttackTarget/IsFriendlyTarget | **偏差**：区间应到 `:3477`；且 Crystal 允许 **Hero**，方案 G1 只写 Monster/Player |
| C6 | 连跳候选 = 半径 3 内、非自身、`IsAttackTarget`、`target.CanFly`，随机取一 | `HumanObject.cs:5947-5951` 过滤器本身**实测**；但 `MonsterObject.cs:2347-2383` 的 `FindAllNearby` 显式收 `Monster/Player/Hero`（`:2368-2376`） | **不符**：方案测试名 `..._ignores_players_for_chain` 会钉死“忽略玩家为候选”，比 Crystal 窄 |
| C7 | 客户端 125 仅 FireBounce：`CreateProjectile(410,...)`、命中 Effect(570)+音效 | `Client/MirScenes/GameScene.cs:4687-4713`（`:4698` 410、`:4706` 570） | **实测** |
| C8 | FireBounce 默认值 BaseCost5/LevelCost1/MPowerBase6/PowerBase10/Range9、15/18/21、2000/2700/3500；伤害/GetPower 公式 | `Server/MirEnvir/Envir.cs:1887-1904` 逐字段一致；`Server/MirDatabase/MagicInfo.cs:180-193` `GetDamage`/`GetPower` 一致 | **实测** |
| R1 | `SpellKind`/`spell_kind` 无 FireBounce，`REGISTERED_UNSUPPORTED` 未登记；server 无 125 发送点 | 实测 `grep -rn "FireBounce\|ObjectProjectile" crates/mir-server/src` = **0/0**；`magic.rs:331-338` 未登记；`magic.rs:649-728` 无分支 | **实测** |
| R2 | （方案未引）协议层已具备 `Spell::FireBounce` | `crates/mir-core/src/enums/generated.rs:5650` `FireBounce = 154`、`:5927` from_value，**无需改协议** | **实测（补强）** |
| R3 | `ObjectProjectile` 结构/Index=125/编解码同 Crystal；已有 roundtrip 与 125 边界样本 | `crates/mir-protocol/src/packets/server.rs:3903-3932`（INDEX 125、u8/u32/u32）、`:10700` roundtrip、`:11464-11471` `Spell` 边界样本 | **实测** |
| R4 | 客户端无 125 生产消费路径 | `grep -n "ObjectProjectile" crates/mir-client/src/net.rs` = **0**；`:4498` 为 `ObjectMagic` 名映射、`:5420-5440` 为 ObjectMagic 处理 | **实测** |
| R5 | `poison_cloud_cell_valid` 即 `Cell.Valid`（界内+walkable） | `magic.rs:3533-3544`（doc 注释 3533-3537、函数体 3538-3544） | **实测（区间略宽）** |
| R6 | `walkable_on` 额外含占城阻挡，不能直接当 `Cell.Valid` | `world.rs:5395-5405`，`:5400` `conquest_object_blocking` | **实测** |
| R7 | R56 P2：`world.rs:1378` 注释首字节 0x07(BEL) | 实测 `:1378` 为 `// 冻结时钟：<0x07>dvance_clock…`（`\u0007` 已复核） | **实测** |
| R8 | 125 解码负例保持绿 | `crates/mir-client/tests/round52_spells_wire.rs:740-757`（“服务端本作没有 125 生产者”注释 + 125 不移除场断言） | **实测** |
| R9 | 验收命令 6：`--lib default_packets_roundtrip -- --exact` | 同 lib 内 **两个**同名测试：`packets/client.rs:5600`、`packets/server.rs:10700`；`--exact` 匹配全路径，短名过滤后 **0 个运行**、exit 0 | **不符（假绿）** |
| R10 | 门禁 meta 需候选文件 sha（P4） | 方案 G4/§五已列 `files[{path,sha256,bytes}]`；R56 实际 `attempt-1.meta.txt` 无此字段 | **实测（已修）** |

**抽核合计 18 组：命中 14、偏差 2（C2/C5）、不符 2（C6/R9）。**

## 二、G1–G4 可行性

- **G1 可做**：`Spell::FireBounce` 已存在（R2），`PendingMagic`/`resolve_pending_magic`/`apply_pending_magic`/`MagicEffect::Damage` 骨架已在（`magic.rs:900-913`、`:2382-2399`、`:2400-2424`），`magic_target_hittable` 已做 ±2+M1/M2 判定。**但 G1 的候选种族口径必须按 C6 修正**，否则测试会发明“忽略玩家”规则。
- **G2 可做**：125 包序断言可在测试内按 `ServerPacket::ObjectProjectile` 出现时序与伤害 tick 交错断言；协议与客户端负例均已具备。
- **G3 部分覆盖**：②③④实测锚点正确（`aoi.md:291` 确写“待 R56”、`round56-plan.md:45` 路径确不存在且 `target/round56/docs/attempt-1/00-evidence-greps.log` 存在、`roadmap.md:143` R55 maps 脚注在、根 README 现无 R55/R56/R57 行）。**缺 P3/P5/P6 处置（见六）**。
- **G4 有内部矛盾**：§六.2 把 T 审计与 manifest 放入提交二，而 T 必在门禁后产出；§六.3 又要求 `HEAD~2..HEAD` blob sha == 门禁候选 sha → T 产物不在 candidate 内，规则自我证伪（R56 R-2 即此）。必须显式限定“候选 sha 只比 A/B 文件，T 产物走第三次提交”。

## 三、D1–D4 裁决

- **D1 FireBounce 范围：判全链**（level+2 命中 / level+1 条 125）。理由：单链最大 5 跳、全走既有延迟队列，无递归爆炸；降级才违背“严格对齐 Crystal”，且 125 生产者正是本轮主目标。
- **D2 随机口径：接受，记 MINOR/PARTIAL**。Crystal 用进程级 `Envir.Random`（受伤害/掉落等共同推进），逐次同序本不可复现；候选集合与四条件一致即可。**约束**：必须用世界级 `MirRng`（禁 `thread_rng`/`rand::random`）、候选按对象 ID 稳定排序后 `rng % len`、测试固定种子；审计与 `docs/world/magic.md` 明写“集合/分布等价，非逐次同序”。
- **D3 客户端 125 渲染：本轮不做，记 PARTIAL**。服务端+协议可验收；`net.rs` 无 125 分支是事实（R4）。**红线**：任何产出不得出现“125 已全链交付”。
- **D4 B 批改 `world.rs:1378`：内容放行、执行改派 A**。理由：用户硬约束“deepseek 只编码、gpt 决策/审核/文档”是制度红线；R56 的 T 也以“未改 src/”自证边界，B 改 `.rs` 会坏先例。改派后仍满足单一写者：A=magic.rs/tests/world.rs 注释，B=纯文档，文件集不相交。A 须先落 `sed -n 1378p | cat -v` 的 before/after 证据，证明仅删 0x07、行数不变。

## 四、M1–M5 必改清单（尚书原样纳入派发）

**M1（高）范围与表述红线**
- A/B/T 三份产物禁止出现“客户端全链交付/125 已全链/R57 完整验收”；T 的终审必须逐条写“服务端生产者=已实现、客户端渲染=PARTIAL”。
- `docs/world/magic.md` 支持矩阵新增行只允许写“FireBounce 服务端生产者（R57）+ 客户端 125 渲染 PARTIAL”；不得写“已接线/PASS”。
- 派生：根 README 的 R52 行仍写“125 无服务端生产者”（`:48`），B 必须改写为“R57 起服务端有生产者，客户端渲染仍 PARTIAL”，否则文档自相矛盾（并入 M5⑤）。

**M2（高）可证伪测试与 LOCK 口径**
- LOCK 口径必须写成条件式：**受控场景（候选充足且均为 Monster）**下总命中=level+2、125 条数=level+1；一般场景只保证 `≤ level+2`。断言失败例至少 3 条：①`bounce` 用尽（level0 时 1 条 125 后停）；②无候选（半径 3 内空）即停；③非 Monster 受害者不续跳（但**可以被选为**次级命中目标，见 M2-c）。
- 125 时序断言必须写在同一次 tick 内：125 包在该跳伤害结算**之前**（先取包列表、再 `advance_clock` 到 `dist*50`），并用固定种子复跑 2 次同结果。
- **修正 C6**：候选种族按 Crystal 收 `Monster/Player/Hero`（`MonsterObject.cs:2368-2376`），测试名与断言不得写“ignores players”；若本作不生成 Hero 对象，则登记“Hero 无实例”而非改语义。
- 命令 6 改为 `cargo test -p mir-protocol --lib packets::server::tests::default_packets_roundtrip -- --exact`（全路径），断言 `1 passed; 0 failed`；其余 12 条按现状可用，`--list`/`--exact` 均须复核实际运行数（禁 0 命中）。
- 证据：`target/round57/firebounce/attempt-1/` 下 11 件齐全，`00-baseline.log` 记录 `cargo test -p mir-server --test magic` 首跑 passed 数。

**M3（高）A0 数据实测**
- A0 第一动作：实测默认 DB 是否含 `spell=154` 的 `MagicInfo`（读库并 grep，原始输出落 `target/round57/firebounce/attempt-1/00-db-firebounce.log`）。
- 缺则测试显式注入 fixture（不改生产 DB、不伪造记录），并把“生产学习/掉落路径”记 PARTIAL；`grep` 证据与 `exit_code` 入 meta。
- 禁止为让测试绿而改 `mir-db` 解析或默认数据。

**M4（高）门禁与提交绑定（修 G4 自相矛盾）**
- candidate 定义：`git diff > target/round57/commit/candidate.patch` 只取 **A/B 候选文件**；`git_diff_sha256` 与逐文件 sha 只比这批。
- T 产物（`round57-final-audit.md`/`final-manifest.json`）走**第三次提交**（文档/审计批，post-gate），不参与候选 sha 等式；§六.3 措辞改为“候选文件 blob 逐一相等”。
- 门禁 `attempt-N.meta.json` 必含 `head`、`git_diff_sha256`、`files[{path,sha256,bytes}]`、`log_sha256`、`log_bytes`、`started_at/finished_at`(+时区)、`exit_code`、`attempt`；首红 attempt 原样保留、不覆盖。
- 命令 13 的预期清单须补 `docs/process/round57-{menxia-review,shangshu-dispatch,shangshu-acceptance,menxia-confirm}.md`（今日我这份已落盘、未提交），否则提交后 `git status` 仍有未跟踪件、命令 13 必红。

**M5（中）R56 承接缺口 + 红线 grep**
- **P6（现行方案漏了目标文件）**：P6 的错值在 `docs/testing/round56/README.md:5`（“受测 HEAD `e3b365f`”），该文件不在任何子单写范围。派发必须把该文件加入 B 的 G3 范围，改为“代码基线 `e3b365f` / 门禁 HEAD `b5369d7`”。
- **P3**：`docs/world/aoi.md` 全文仍有 7 处既有 PASS（`:349`、`:353-355`、`:357`、`:359`、`:365`，R54/R55 状态表）。B 须二选一并落证：清理到 0，或把 M4 红线 grep 明确限定为“`git diff -U0` 新增行”，附行号清单与理由；禁整文件 grep 冒充。
- **P5**：统一命名为 `meta.json`（证据目录）/`attempt-N.meta.json`（门禁）/`final-manifest.json`（T）；`docs/testing/round57/README.md` 索引名必须与 T 实物同名。
- B 的红线自查落 `target/round57/docs/attempt-1/04-grep-redline.log`（新增行 `PASS|已实现|已接线|全链` 计数=0，逐条带 exit）。
- B 的 `docs/testing/round57/README.md` 只写“已落盘/待 T 抽核”，不得预写 PASS；B 不碰任何 `.rs`（D4 已把 BEL 改派 A）。

## 五、风险与 PARTIAL 口径（同意方案，追加 2 条）

- 继承方案 6 条风险；追加：**R1** 链式候选若被写成“仅 Monster”将产生 Crystal 无据的新规则（M2-c）；**R2** T 产物与候选 sha 混绑会重演 R56 R-2（M4）。
- PARTIAL 终态：客户端 125 渲染/410+570 特效；Hero 无实例；Crystal“无效目标仍扣蓝”差异（本作先校验后扣蓝）；默认 DB 缺记录时的生产学习路径；次级目标随机序列（集合等价、MINOR）。

## 六、看板回执（全部 CLI 原样 exit）

| # | 命令 | exit | 回执 |
|---|---|---|---|
| 1 | `memo JJC-20261026-004 menxia "<结论摘要>" --as menxia` | **0** | `📝 JJC-20261026-004 任务记忆：menxia` |
| 2 | `state JJC-20261026-004 Assigned --as menxia "<评审结论>"` | **0** | `✅ 状态更新：Menxia → Assigned` |
| 3 | `delegate JJC-20261026-004 menxia shangshu "按评审派发 A/B/T/G 子单" --as menxia` | **0** | **SUB_TASK_ID=`JJC-20261026-004-sub-c4685932`**（depth=1） |
| 4 | `validate --as menxia` | **0** | `✅ 看板校验通过：151 条任务符合 schema v3.1`（评审前 150、委派后 +1） |

## 七、只读声明

本轮仅新增本文件；未改 `crates/`、未跑 `cargo`/`ci.ps1`、未 commit/push、未 spawn 执行者。
