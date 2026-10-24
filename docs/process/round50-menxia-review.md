# 第50轮门下省评审：R50 商城收尾方案

- 评审对象：`docs/process/round50-plan.md`（148 行 / 18,614 B，中书省 020a979d）；任务 `JJC-20261025-002`（Menxia）
- 评审方：门下省 gpt-6.1-sol（独立评审，不实现、不改 crates/证据/README/方案）
- 结论：**PASS-WITH-CHANGES（有条件通过：下列 5 条 must-fix 必须先落进尚书派发契约，否则本评审自动转 BLOCK，回 Zhongshu 重规划）**
- 事实基线：HEAD=`d5707c6`（119 files / 31,063+ / 274-）、门禁日志 `target/round49/final-gate.log`=182,941 B / sha256 `5d475f4f…`、R49 刑部终审 verdict=**PASS-WITH-PARTIAL**（P01–P08 未清）

## 一、放行边界（5 条 must-fix，缺一即 BLOCK）

1. **A 改口径**：授权从"仅追加"改为"3 行就地改 + 追加「门禁结果」节"，且 R49 verdict 必须写 PASS-WITH-PARTIAL，禁止纯 PASS。
2. **E 重写**：15 条按语义拆分（显示名→TextKey、匹配别名→config、日志英文化），写范围补齐 `config/lang/*.json` 与别名配置文件；`docs/i18n.md` 改由 H 写（E=deepseek 不得写规范文档）；阈值 15→0 仅在同一日志链实测 0 后改，否则维持 15 + PARTIAL。
3. **F 处置**：F1/F2 现命令会覆写 `docs/testing/monster-native`、`docs/testing/monster-motion` 冻结目录；要么扩权给两个测试加 `MIR_*_EVIDENCE_DIR` 环境变量并只写 `target/round50/**`，要么降级为只读哈希复核、P02/P03/P04 维持 PARTIAL。二者择一，禁"照跑现有命令"。
4. **判据去写死**：`plan:123` 的"末行 `==> 全部通过（12 项）`"改为"原始退出码 0 + 末行 N 项（N=当次脚本实际项数）"；删除 R50-G（前提被实测证伪）。
5. **B/C 越界与计数**：B4"客户端 GIndex 幂等"移入 C；B1 删"既有 15 例"（实测 `crates/mir-server/tests/gameshop.rs` 有 21 个测试属性）；`plan:147` 的 `app.rs:3060` 锚点更正。

## 二、逐条裁决（向中书省的正式答复）

### 决策1：是否授权追加写 `docs/testing/round49/README.md`

**决定：授权，但不是"纯追加"——准许对已入库冻结文件做最小就地改：`docs/testing/round49/README.md:5`、`:57` 与 `README.md:74` 三行门禁状态各改一次，再在 `docs/testing/round49/README.md` 末尾追加「## 门禁结果（R50-A）」节。**

理由：纯追加会让 :5/:57 与 README:74 继续写 `current_gate=PARTIAL/待队长门禁`，正是 R49 刑部 P06 未结项；而 R49 终审 §9.3 本就建议"最终提交前更新这两处门禁状态（不改审计结论）"。冻结规则允许的例外只到"门禁状态行 + 结果节"：不得改既有哈希、PNG 表、日志登记、manifest，不得删行；改动前后必须用 `git diff` 全文与 `git hash-object` 留证。措辞模板（三行+新节统一）：`gate=12/12 PASS；日志 target/round49/final-gate.log，182,941 B，sha256 5d475f4f…，exit 0，commit d5707c6；R49 verdict=PASS-WITH-PARTIAL（P01–P08 未清）`。禁写"全部通过/全部关闭/已完成"，禁把 `current_gate=PARTIAL` 字面量留在任何位置（A1 的 grep 必须为 0，含新节）。

对 DAG 的影响：A 无前置、可最早落地，第一次提交单独提交三文件不变；A1 的 grep 期望扩展到 `docs/roadmap.md:123`（line 123 现写"当前为 **PARTIAL**"，不在原 A1 覆盖内）；A2 的"git diff 仅 3 文件"在只改这三文件时仍成立（两个大 JSON 是 untracked，不进 diff，但会出现在 `git status --porcelain`，T1 允许清单必须列"允许排除"）。

### 决策2：i18n 15→0 口径

**决定：同意中书口径，但按语义三分，不是"15 行全部改走 TextKey"：**
- **显示名走 TextKey（可接受）**：`monster.rs` 的 8 条模板名（:956/:970/:980/:995/:1006/:1021/:1032/:1049）在**显示边界**改成 TextKey（如 `TextKey::MonsterName`），并且必须把"数据名/匹配名"与"显示名"分离：DB/输入匹配继续用数据名，显示另查词表；缺词表时回退数据名并记 PARTIAL。
- **匹配别名留 config（必须）**：`monster.rs:695` 弓箭守卫（AI 113 名字兜底）与 `market.rs:559-563` 五职业名共 6 条是**匹配别名**，不得塞进 UI 词表；搬到 `config/i18n-aliases.json`（中英+DB 名三列）并由代码加载，匹配语义（含 DB 中文名）不得改变。
- **日志英文化（必须）**：`server.rs:114` 是 `with_context` 开发者日志，按 `docs/i18n.md` §7.1 既有先例改英文，不占 TextKey。
- **阈值**：先跑 `--no-test --strict --fail-above 0`；**实测 0** 才把 `tools/dev/ci.ps1:136` 的 15 改 0，且阈值改动与 0 必须在同一日志链里；实测非 0（如别名仍需加载路径、或某模板名本质是数据键）则维持 15 并 PARTIAL，`docs/i18n.md` 只能写"目标 0，实测 N"，不得写 0（`docs/i18n.md:270,276` 原文"不能写成 0"）。

理由：`docs/i18n.md:287,671-681` 已把这 15 条逐条分类为"保留（数据匹配/日志）"；plan:45 的"上述 15 行改走 TextKey"直接与文档冲突，且 E 写范围没有 `config/lang/{zh-cn,en}.json`——加 TextKey 不补两语言包会红 `embedded_pack_matches_builtin_text`（`crates/mir-core/src/i18n.rs:84-88` 明确要求 `TextKey::ALL` + 两语言包 + `cargo test -p mir-core`）。

对 DAG 的影响：E 写范围改为 `crates/mir-core/src/i18n.rs`、`crates/mir-server/src/{market.rs,monster.rs,server.rs}`、`tools/dev/ci.ps1`、`config/lang/{zh-cn,en}.json`、`config/i18n-aliases.json`（新建）；`docs/i18n.md` 移出 E、归 H；E 仍依赖 B（同改 `market.rs`，串行）；E3 的 `cargo test -p mir-core` 与 `embedded_pack_matches_builtin_text` 必须进 E 证据。

### 决策3：客户端商城入口只做固定热键 `K`

**决定：同意。本轮只做固定热键 `K` 打开独立商城面板；Crystal 的 `KeybindOptions.GameShop` 可重绑不复制，留 R51。**

理由：`crates/mir-client/src/app.rs` 的 live 热键（`handle_live_key` 起 :2829）已用 i/e/m/b/p/q/y/t/n/g/f/o/v/d/s/r，**K 空闲**；Crystal 的重绑走 `CMain.InputKeys.GetKey(KeybindOptions.GameShop)`（`MainDialogs.cs:190`），其 `KeybindOptions` 体系本作未对齐，本轮引入会扩范围。约束：① 焦点在聊天/搜索框时 K 不得触发（复用现有文本输入早退，无则 C 补守卫）；② 只在 live 态生效，重复按下只开一次；③ 键位与提示语走 `ui::labels` 双语，`docs/client.md` 记"K=商城（R51 可重绑）"；④ R51 登记 `KeybindOptions.GameShop` 可重绑，落在 `docs/world/gameshop.md` §7.3 由 H 写一行，不改 roadmap。

对 DAG 的影响：C1–C4 不变；C 的 `app.rs` 只加 K 分发与守卫，不做设置面板入口；C 仍依赖 B（B 不发真库存包则面板只能离线）。

### 决策4：R50-G（看板 25 条停滞旧任务清理）是否纳入

**决定：不纳入，且不必"挂 R51"——该任务的前提已被实测证伪。**

理由：`list --all` 实测 **91 条：63 Done + 27 Cancelled + 1 Menxia（本任务）**，没有任何 2026-10-20~22 的非终态停滞任务。修改 63+27 条终态记录属于"改写历史事实"，违反 team-and-roles §5.3/§6"终态不可回填"。若尚书仍想做卫生，只允许**新增一条 memo** 记录"实测 91 条全终态、25 条口径不成立"，零条状态写。

对 DAG 的影响：**删除 R50-G/G1，10 条子单变 9 条（01/A/B/C/D/E/F/H/T）**；T1 的看板核对降为只读 `show`+`validate`，不写状态。

### 决策5：协议冻结优先级

**决定：确认"协议线格式冻结（INDEX 250/251 与 Crystal 已一致）优先于逐字节对齐"。若实测发现 250/251 字段/顺序/长度不符，只记 PARTIAL、不改线格式。**

理由：`crates/mir-protocol/src/manual/server.rs:1089-1091` 已按 Crystal `ServerPackets.cs:5348` 落 250、`crates/mir-protocol/src/packets/server.rs:5959-5968` 落 251，`Shared/Enums.cs:1686-1687` 编号一致；而 Crystal 自身 GIndex 读写键不一致（读 `Info.Index` :13710/:13763/:13926，写 `GIndex` :13843-13858），逐字节对齐等于复制其 bug；本作按 GIndex 统一已由 `docs/world/gameshop.md` §8.1 裁决。B2 的六断言以"真实包里服务端发的值与本作语义一致"为准，不以"与 Crystal 逐字节相同"为准。

对 DAG 的影响：B 不改协议 crate，B3 的 `cargo test -p mir-protocol game_shop` 只作回归；若 B2 发现 250/251 不符，B 记 PARTIAL 并继续（不发假包），C 只交离线面板证据，T1 按 PARTIAL 汇总。

## 三、方案问题清单（行号 / 证据 / 改法）

- **P1（高）`plan:45,59` E 与 i18n 文档冲突、写范围缺两语言包、与 H 重叠**：证据 `docs/i18n.md:287,671-681`（15 条已分类"保留"）、`crates/mir-core/src/i18n.rs:84-88`、E 与 H 同写 `docs/i18n.md`。改法：按决策2三分；E 写范围补 `config/lang/{zh-cn,en}.json` + `config/i18n-aliases.json`；`docs/i18n.md` 归 H；E 用 deepseek 合法（去掉文档后仍是代码）。
- **P2（高）`plan:46,60,96-100,116` F 命令会覆写冻结证据**：证据 `crates/mir-render/tests/monster_native_evidence.rs:153`（`repo_root().join("docs/testing/monster-native")`）、`:432`（写 `raw-manifest.json`）、`crates/mir-client/tests/monster_motion.rs:327`（`workspace.join("docs/testing/monster-motion")`），两测试无 env 覆盖。改法：方案 a）授权 F 只改两个测试的 out_dir 为 `MIR_*_EVIDENCE_DIR` 默认值不变，F 命令带环境变量写 `target/round50/**`（写范围 += 这两个测试文件；跑前跑后对冻结目录做 SHA256 清单比对，必须零变化）；方案 b）降级只读复核 4 张 PNG + `raw-manifest.json` 哈希与语义摘要，P02/P03/P04 维持 PARTIAL。二者必须择一写进派发。
- **P3（高）`plan:41,55` A 与"只追加"矛盾且 PASS 口径有假绿风险**：证据 `docs/testing/round49/README.md:5,:57`、`README.md:74`、`docs/roadmap.md:123` 均为门禁前 PARTIAL；R49 刑部 verdict=PASS-WITH-PARTIAL。改法：按决策1 授权 3 行就地改 + 追加节；措辞含 `gate=12/12 PASS` + `R49 verdict=PASS-WITH-PARTIAL（P01–P08 未清）`；A1 grep 扩展到 roadmap:123，并确保 `current_gate=PARTIAL` 归零；A 证据附冻结文件 `git diff` 全文。
- **P4（中高）`plan:123` 门禁判据写死 12 项，与 T1"不得固定 12/12"自相矛盾；`plan:47` G 前提证伪**：证据 `tools/dev/ci.ps1` 末行 `==> 全部通过（12 项）` 是当次脚本实际项数；看板实测 91 条全终态。改法：判据=原始进程退出码 0 + 末行 `N 项`（N 以当次日志为准，不预写 12）+ 无 `[FAIL]`；删 R50-G。
- **P5（中）`plan:42` B4 越界、`plan:75` 计数不准、`plan:147` 锚点错、`plan:14` 措辞不准**：证据 B 只写 `crates/mir-server/src` 无法验客户端幂等（客户端 0 命中 `GameShopInfo|GameShopStock`，`net.rs:660-667`）；`crates/mir-server/tests/gameshop.rs` 实测 21 个测试属性；`app.rs:3060` 是 `direction_from_key`，热键表在 :2829 起、`Key::Character` 在 :1125/:1554/:1575/:2811/:2867；`PlayerObject.cs` 是"读 Info.Index / 写 GIndex"而非"与读取键不一致"。改法：B4 移入 C4；B1 只写"既有测试全绿（当次记录实际数）"；锚点改 :2829 与 `ui/mod.rs`；plan:14 按 §8.1 修正措辞。

## 四、抽查记录（实读双侧，至少 5 处）

| # | 断言（方案行） | 实测位置 | 结论 |
|---|---|---|---|
| 1 | 登录后取商城 `plan:13` | `PlayerObject.cs:1203,1205`；Rust 登录链 `session.rs:1211` | 命中 |
| 2 | 库存包语义 `plan:14,58` | `PlayerObject.cs:13703-13723`、`:13861`、`:13913-13944` | 命中；Crystal 读 Info.Index 写 GIndex（§8.1 已述），plan:14 措辞需改 |
| 3 | 包结构 250/251 `plan:15,16` | `ServerPackets.cs:5348-5392`、`ClientPackets.cs:2434-2454`、`Enums.cs:1686-1687`；`manual/server.rs:1089-1091`、`packets/server.rs:5959-5968` | 命中 |
| 4 | 客户端面板行为 `plan:17,19` | `GameScene.cs:6679-6698`、`GameshopDialog.cs:26,508`、`MirGameShopCell.cs:264-283`、`MainDialogs.cs:186-196` | 命中 |
| 5 | Rust 现状"不发商城包" `plan:24` | `market.rs:96-98`；`grep GameShopInfo|GameShopStock crates/mir-client/src`=0 | 命中 |
| 6 | E 的 15 条位置 `plan:29` | `i18n.md:671-681` 与 `market.rs:558-565`、`ci.ps1:135-136` | 命中但分类为"保留"（见 P1） |
| 7 | `plan:75` 既有 15 例 | `crates/mir-server/tests/gameshop.rs` | **不符（21 个）** |
| 8 | `plan:147` 热键表 :3060 | `app.rs:2829` 起、:3060 是 `direction_from_key` | **不符** |
| 9 | HEAD / 119 files / 日志 182,941 B / sha 5d475f4f | `git show -s --format=%H%n%ci d5707c6`、`git show --stat`、`sha256sum target/round49/final-gate.log` | 命中 |
| 10 | G 的"25 条停滞任务" `plan:47` | `list --all`=91 条（63 Done+27 Cancelled+1 Menxia） | **前提证伪** |

## 五、验收口径（以队长原始日志/退出码为准，不固定 12/12）

- 任何命令必须记：argv、cwd、起止时间（+08:00）、**原始进程退出码**（不允许 `| tee` 后的管道码代替）、实际通过/失败数、工作树 `HEAD` 与 `git status --porcelain`、日志路径+字节+SHA256。
- 全量门禁：队长唯一一次 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；通过条件 = **原始退出码 0** + 末行 `==> 全部通过（N 项）`（N 为当次脚本实际项数，不预写 12）+ 日志无 `[FAIL]`/`[ERROR]`；N 与 12 不一致时以日志为准，不判假绿也不判红。
- 任一项红 → 该批 PARTIAL，重跑必须换新日志名（不覆盖、不复用旧日志）；跨工作树的日志一律无效。
- 用户可见的"绿"只由 T1 汇总：T1 必须给出原始日志路径/字节/SHA256/exit code 与逐项状态，缺一即 PARTIAL。

## 六、证据纪律

- **PARTIAL 边界**：任一命令非零、证据文件缺、哈希不符、manifest 缺项、图标缺失（占位+PARTIAL）、冻结目录哈希变化、真实链路未验证（fixture/静态包冒充）、i18n 实测非 0、B PARTIAL 导致 C 只交离线面板——全部记 PARTIAL 并写原因，不得四舍五入为 PASS。
- **>1 MiB 不入库**：`docs/testing/monster-native/player-resource-freeze{,-before}.json`（44,844,165 / 45,219,533 B）只登记路径/字节/SHA256；`target/round50/**` 单文件 ≤1 MiB 否则同样只登记。
- **审计产物门禁后写入口径**：T1 报告与 `docs/testing/round50/final-manifest.json` 在门禁后写入，**不使门禁失效**（按 R49 §10），但必须在 T1 报告中单列文件清单+哈希；门禁前不得预写终态结论。
- 证据路径统一 `target/round50/**`；写入冻结目录的文件（R49 docs/testing/round49）只允许 A 的三行+一节例外，且必须前后哈希对比。

## 七、对尚书派发的约束（9 条子单）

写范围不重叠（原 10 条减 G）：

| ID | owner | routing | write_scope（摘要） | 依赖 |
|---|---|---|---|---|
| R50-01 | 礼部 | gpt-6.1-sol | `target/round50/facts/**`（只读取证实证） | 无 |
| R50-A | 礼部 | gpt-6.1-sol | `README.md`、`docs/testing/round49/README.md`、`docs/roadmap.md` | 01（取行号） |
| R50-B | 兵部 | deepseek-flash | `crates/mir-server/src/{session.rs,market.rs}`、`tests/{gameshop,gameshop_wire,gameshop_failure}.rs` | 01 |
| R50-E | 兵部 | deepseek-flash | `crates/mir-core/src/i18n.rs`、`crates/mir-server/src/{market.rs,monster.rs,server.rs}`、`tools/dev/ci.ps1`、`config/lang/{zh-cn,en}.json`、`config/i18n-aliases.json` | **依赖 B（串行同改 market.rs）** |
| R50-C | 工部 | deepseek-flash | `crates/mir-client/src/{net.rs,app.rs,ui/gameshop.rs,ui/mod.rs}`、`tests/{gameshop_panel,round2_panels}.rs` | **B 完成**；与 D 解耦 |
| R50-D | 户部 | deepseek-flash | `tools/assets/gameshop_manifest.py`、`docs/assets/gameshop-manifest.json` | 无（不阻塞 C） |
| R50-F | 刑部 | deepseek-flash | `target/round50/monster-native/**`；若方案 a 获批 + `crates/mir-render/tests/monster_native_evidence.rs`、`crates/mir-client/tests/monster_motion.rs`（仅 out_dir 环境变量） | 无 |
| R50-H | 礼部 | gpt-6.1-sol | `docs/world/gameshop.md`、`docs/client.md`、`docs/i18n.md`、`docs/assets/licensing.md`、`docs/testing/round50/**`（索引/说明，不含 T 报告） | B、C、E、F 交付后 |
| R50-T | 刑部 | gpt-6.1-sol | `docs/testing/round50/round50-final-audit.md` | 队长全量门禁后 |

- **B 与 E 串行**：同改 `market.rs`，E 必须等 B 提交/交接完成再开工；C 与 B 可并行（不同 crate，但 C 的真实链路证据依赖 B 完成）；D 与 C 只经 `docs/assets/gameshop-manifest.json` 解耦——C 读不到 manifest 用占位图标并记 PARTIAL，不得等 D。
- **写范围去重**：`docs/i18n.md` 只归 H（E 不写）；`docs/testing/round50/**` 的 H 索引与 T 终审是不同文件，H 不得写 T 路径；`market.rs` 归 B/E 两人但强制串行，其余文件均单写者。
- **禁捷径增补**（在原 13 条黑名单上追加）：① 不得把匹配别名塞进 UI TextKey；② 不得改阈值/删测试/放宽扫描器来凑 0（阈值改动必须与实测 0 同一日志链）；③ 不得覆写 `docs/testing/monster-native`、`docs/testing/monster-motion`；④ 不得把 12 项/15 例/25 条等固定数字当判据；⑤ 不得用跨工作树或旧日志冒充本次结果；⑥ 不得用静态 fixture/抓包改包冒充真实包链；⑦ 不得把 TextKey 词表当匹配表用。

## 八、本轮必须向用户汇报的节点

1. A 落地后：三行门禁状态与追加节的实际 diff、`current_gate=PARTIAL` 归零、R49 verdict 是否写成 PASS-WITH-PARTIAL。
2. B 完成：六断言的真实包链证据（登录目录包、购买刷新、失败不发包、GIndex 同键、回滚、协议回归）与原始退出码。
3. C+E 完成：K 面板可用性 + i18n 实测值（0 则报阈值改动；非 0 则报 PARTIAL 与保留清单）。
4. 队长全量门禁：原始退出码、末行 N 项（不预先承诺 12）、日志路径/字节/SHA256。
5. F/H 交付后：P02/P03/P04 是收口还是仍 PARTIAL；`docs/testing/monster-native` 哈希是否变化。
6. 最终提交 hash 与 T1 verdict；若任一 must-fix 未吸收，报告"转 BLOCK 及理由"。
