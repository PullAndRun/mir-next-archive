# 第50轮尚书省派发说明（R50 商城收尾）

- 派发：尚书省 `shangshu`（gpt-6.1-sol）；日期 2026-10-25（Asia/Shanghai）。
- 父单：`JJC-20261025-002`（Assigned → Doing）；本派发吸收[门下评审](round50-menxia-review.md) 5 条 must-fix，优先于[中书方案](round50-plan.md)被修订处；不改写方案、评审与历史日志。
- 基线：HEAD `d5707c6`（119 files）；R49 门禁日志 182,941 B / sha256 `5d475f4f…` / exit 0；R49 verdict=**PASS-WITH-PARTIAL**（P01–P08 未清）。
- 9 条子单已全部 `delegate` 登记（删 R50-G，DAG 10→9）；写范围互不重叠；`market.rs` 归 B/E 强制串行。
- 执行者一律由 Lead 启动；尚书不 spawn、不跑 cargo/门禁、不 git；六部不 `state`、不验收、不 `git add/commit`。

## 一、执行顺序与依赖

- 依赖图：`01 → A / B / D / F`；`B → C`；`B → E`（同改 `market.rs`，强制串行）；`B+C+E+F → H`；`队长唯一全量门禁 → T`。
- 第 1 波：仅 `R50-01`（产出精确行号与 GIndex 事实，是 A/B/D/F 的共同前置）。
- 第 2 波：`A`、`B`、`D`、`F` 并行（不同写范围；A 用 01 的行号，B 用 01 的登录锚点，D 用 01 的 GIndex，F 独立）。
- 第 3 波：B 交付并释放 `market.rs` 后，`C`、`E` 并行（C 与 B 不同 crate；E 必须等 B 写完 `market.rs` 再开工，不得同时改）。
- 第 4 波：`H`（B/C/E/F 全部交付后）；第 5 波：Lead 全量门禁 → `T` 只读终审。
- A 批单独先提交（§四），不与功能批混提；C 与 D 只经 `docs/assets/gameshop-manifest.json` 解耦——C 读不到 manifest 用占位图标 + PARTIAL，**不等 D**。
- H 与 T 路径不重叠：H 写 `docs/testing/round50/**` 索引/说明（不含 T 报告与 final-manifest）；T 只写终审报告。
- 说明：父单下另有门下→尚书的派发委派 `JJC-20261025-002-sub-9469ed8b`（本文件即其交付），不占 9 个执行者名额。

## 二、9 条子单逐条契约

### R50-01 实证复核 · `JJC-20261025-002-sub-b9c2de60`
- 承接：礼部 `libu` / `gpt-6.1-sol`｜依赖：无｜写范围：`target/round50/facts/**`（只读复核，不改被核对文件）。
- 任务：逐条实读复核方案/评审的 `文件:行`（Crystal 10 条 + Rust 侧），落 `facts.json`（路径/实际内容/命中或偏差）与 `verify.py`；重点核对 `README.md:74`、`docs/roadmap.md:123`、`docs/testing/round49/README.md:5/:57`、`app.rs:2829/:2864-2902`、`gameshop.rs` 21 个测试属性、`ci.ps1:136` 阈值 15、`docs/i18n.md` §7.3 的 15 条分类。
- 命令（期望 exit 0；任一锚点越界 exit 1）：`py target/round50/facts/verify.py docs/process/round50-plan.md` → `facts ok: N`（N=本文件 `路径:行` 引用数）。
- 证据：`target/round50/facts/**`（命令、cwd、起止、原始退出码、HEAD+porcelain）。
- 禁止捷径：盲信方案行号；把 grep 命中当功能通过；顺带修改任何被核对文件。
- PARTIAL：任一锚点偏差未列、facts 与原文不一致、缺原始退出码。

### R50-A 门禁行刷新 · `JJC-20261025-002-sub-dfd9ba4c`
- 承接：礼部 `libu` / `gpt-6.1-sol`｜依赖：01｜写范围：`README.md`（仅第49轮行，实测 `:74`）、`docs/testing/round49/README.md`（仅 `:5`/`:57` 各就地改一次 + 末尾追加「## 门禁结果（R50-A）」节）、`docs/roadmap.md`（仅 §六 `:123`）。
- 口径（门下决策1）：三行 + 新节统一写 `gate=12/12 PASS；日志 target/round49/final-gate.log，182,941 B，sha256 5d475f4f…，exit 0，commit d5707c6；R49 verdict=PASS-WITH-PARTIAL（P01–P08 未清）`；禁写“全部通过/全部关闭/已完成”；`current_gate=PARTIAL` 必须归零（含新节）；不改既有哈希/PNG 表/日志登记/manifest/删行。
- 命令（`git show`/`git diff` 期望 exit 0；两条零计数 grep 输出 0 时退出码 1 属预期）：`git show --stat --oneline d5707c6`（首行含 d5707c6 与 119 files）；`grep -c "current_gate=PARTIAL" README.md docs/testing/round49/README.md docs/roadmap.md`（三条计数均 0）；`grep -c 5d475f4f README.md docs/testing/round49/README.md`（≥1）；`grep -c d5707c6 README.md docs/testing/round49/README.md`（≥1）；`git diff --stat`（仅 3 文件）；`git diff --check`（exit 0）。
- 证据：`target/round50/r50-a.log`（前后 `git diff` 全文、`git hash-object`、命令/退出码）。
- 禁止捷径：越界改其他文件；写成纯 PASS 或删掉 PARTIAL 历史；用旧日志冒充本次核对。
- PARTIAL：任一行未改/越界、`current_gate=PARTIAL` 残留、R49 verdict 未写 PASS-WITH-PARTIAL。

### R50-B 服务端真实包链 · `JJC-20261025-002-sub-2b2da8e0`
- 承接：兵部 `bingbu` / `deepseek-flash`｜依赖：01｜写范围：`crates/mir-server/src/{session.rs,market.rs}`、`crates/mir-server/tests/{gameshop,gameshop_wire,gameshop_failure}.rs`（后两者新建；`gameshop.rs` 既有 **21** 个测试属性，不得删/放宽）。
- 语义：登录按 Crystal 顺序逐商品发 `S.GameShopInfo`（`Stock!=0` 发 `Stock-purchased`，为负整条不发；`Stock==0` 发 0）；购买成功**仅在事务提交后**按条件发 `S.GameShopStock`；失败/回滚零包；个人/全服计数同键 GIndex。协议 INDEX 250/251 冻结优先，不符只记 PARTIAL、不改线格式（门下决策5）。
- 命令（均期望 exit 0；B1 记录当次实际用例数）：`cargo test -p mir-server --test gameshop`；`cargo test -p mir-server --test gameshop_wire`；`cargo test -p mir-server --test gameshop_failure`；`cargo test -p mir-protocol game_shop`。
- 断言（B2 六条）：`Stock!=0` 发剩余 / 为负不发 / `Stock==0` 发 0 / 成功刷新 / 失败零包 / 回滚不刷。
- 证据：`target/round50/r50-b/**`（逐命令原始日志、退出码、用例数、HEAD+porcelain）。
- 禁止捷径：改 `mir-protocol` 线格式；静态 fixture/抓包改包冒充真实包链；失败也发库存包；吞事务错误。
- PARTIAL：250/251 实测不符、任一测试红、失败分支出现库存包、无真实链路证据。

### R50-C 客户端独立商城面板 · `JJC-20261025-002-sub-7d8f26cc`
- 承接：工部 `gongbu` / `deepseek-flash`｜依赖：**B 完成**｜写范围：`crates/mir-client/src/{net.rs,app.rs,ui/gameshop.rs,ui/mod.rs}`、`crates/mir-client/tests/{gameshop_panel,round2_panels}.rs`（`gameshop_panel.rs` 新建；`round2_panels.rs` 只增不删既有用例）。
- 语义：消费 `GameShopInfo`/`GameShopStock`，GIndex 幂等（迟到/重复包不重复入列）；目录缓存、分类/职业/新品/促销筛选、搜索、元宝/金币双币、`C.GameshopBuy`、空态/断线/缺图标/库存零可见；与市场页签共存且状态隔离。入口仅固定热键 `K`：只在 live 态、重复按下只开一次、焦点在聊天/搜索框时走 `app.rs:2864` 文本输入早退不得触发；`KeybindOptions` 重绑留 R51。
- 命令（均期望 exit 0）：`cargo test -p mir-client --lib ui::gameshop`；`cargo test -p mir-client --test gameshop_panel`；`cargo test -p mir-client --test round2_panels`；`cargo test -p mir-client --lib ui::labels`。
- C4：GIndex 幂等断言须有 B 真实包链证据；C3 标签中英齐全。
- 证据：`target/round50/r50-c/**`；缺 manifest 用占位图标并记 PARTIAL，不等 D。
- 禁止捷径：把市场页签当独立面板；K 在输入框触发；B 未完成就宣称真实链路通过。
- PARTIAL：B PARTIAL → 只交离线面板 + 空态/断线态并挂 PARTIAL；缺图标/密钥位无证据。

### R50-D 素材授权与哈希 manifest · `JJC-20261025-002-sub-d38516b3`
- 承接：户部 `hubu` / `deepseek-flash`｜依赖：01（GIndex 事实）｜写范围：`tools/assets/gameshop_manifest.py`、`docs/assets/gameshop-manifest.json`。
- 每条 GIndex 记录 icon/name/credit/gold/category + source_path/bytes/sha256/license；只允许 `resource/` 现有图库与 `mir2_dev` 索引；不新造图标、不把社区仿制素材冒充 Crystal 原作；未核验素材只许 PARTIAL 占位并明文标注。
- 命令（均期望 exit 0）：`py tools/assets/gameshop_manifest.py --check` → `manifest ok: N`（缺项打印 `partial: k` 且退出 1，除非显式 `--allow-partial`）；`py tools/assets/gameshop_manifest.py --verify-icons` → `icons ok: N`。
- 证据：`target/round50/r50-d/**`（清单、缺口清单、逐文件哈希）。
- 禁止捷径：伪造图标/来源/授权；把缺图标写成齐全；改 `resource/` 源文件。
- PARTIAL：缺图标、缺授权、缺哈希、来源不可复核，全部列明。

### R50-E i18n no-test 15→0（语义三分） · `JJC-20261025-002-sub-177decbe`
- 承接：兵部 `bingbu` / `deepseek-flash`｜依赖：**B 交付并释放 `market.rs`**（强制串行）｜写范围：`crates/mir-core/src/i18n.rs`、`crates/mir-server/src/{market.rs,monster.rs,server.rs}`、`tools/dev/ci.ps1`、`config/lang/{zh-cn,en}.json`、`config/i18n-aliases.json`（新建）；**`docs/i18n.md` 归 H，E 不得写**。
- 三分（门下决策2）：8 条 `monster.rs` 显示名在显示边界改 TextKey（显示名/数据名分离，缺词表回退数据名 + PARTIAL）；6 条匹配别名（`market.rs:559-563` 五职业 + `monster.rs:695` 弓箭守卫）搬 `config/i18n-aliases.json` 三列（中/英/DB 名）由代码加载，匹配语义不变，严禁塞进 UI 词表；`server.rs:114` 日志英文化。
- 命令（均期望 exit 0）：`py tools/i18n/scan_hardcoded.py --player-visible --no-test --strict --fail-above 0`（实测命中 0）；`py tools/i18n/scan_hardcoded.py --player-visible --strict --fail-above 70`（≤70）；`py tools/i18n/verify_text_keys.py`；`cargo test -p mir-core`；`cargo test -p mir-core embedded_pack_matches_builtin_text`。
- 阈值：仅同一日志链实测 0 才把 `ci.ps1:136` 的 15 改 0；非 0 维持 15 + PARTIAL，文档只写“目标 0，实测 N”。
- 证据：`target/round50/r50-e/**`（15 条基线、改动后计数、逐条去向）。
- 禁止捷径：别名塞 UI 词表；改阈值/删测试/放宽扫描器凑 0；写 `docs/i18n.md`。
- PARTIAL：实测非 0、E3 任一红、别名表缺列。

### R50-F 证据重跑（门下决策3方案 a） · `JJC-20261025-002-sub-5f8f99a4`
- 承接：刑部 `xingbu` / `deepseek-flash`｜依赖：01｜写范围：`target/round50/{monster-native,monster-motion}/**`、`crates/mir-render/tests/monster_native_evidence.rs`、`crates/mir-client/tests/monster_motion.rs`（**仅** out_dir 环境变量覆盖，默认值不变）。
- 冻结保护：跑前/跑后对 `docs/testing/monster-native`、`docs/testing/monster-motion` 做**全部文件**（含未跟踪的 `player-resource-freeze*.json`）SHA256 清单比对，必须零变化；这些未跟踪文件不得 add/删/改，是否入库由 Lead 在功能批决定；产物只写 `target/round50/**`。
- 命令（均期望 exit 0）：带 `MIR_NATIVE_EVIDENCE_DIR=target/round50/monster-native` 跑 `cargo test -p mir-render --test monster_native_evidence -- --ignored --nocapture`；带 `MIR_MOTION_EVIDENCE_DIR=target/round50/monster-motion` 跑 `cargo test -p mir-client --test monster_motion -- --ignored --nocapture`；`Get-FileHash` 4 张实存 PNG + 两次 attack 语义摘要比对。
- F1 三哈希同一快照（raw-manifest/client_binary/source_blobs）；F3 语义一致、像素差按帧登记，不写 0 diff。
- 证据：`target/round50/r50-f/**`（前后哈希清单、命令、退出码、摘要）。
- 禁止捷径：覆写冻结目录；缺素材 `return Ok`；旧日志/旧 PNG 冒充本次。
- PARTIAL：缺素材、命令红、三哈希不一致、冻结目录哈希变化、默认路径被改动。

### R50-H 文档终稿 · `JJC-20261025-002-sub-a78487f6`
- 承接：礼部 `libu` / `gpt-6.1-sol`｜依赖：**B、C、E、F 交付后**｜写范围：`docs/world/gameshop.md`、`docs/client.md`、`docs/i18n.md`、`docs/assets/licensing.md`、`docs/testing/round50/**`（索引/说明；**不写 T 报告与 final-manifest**）。
- 口径：只写真实证据支撑的“已实现”；i18n 写“目标 0，实测 N”（N=E 实测，非 0 不得写 0）；`docs/client.md` 记 `K=商城（R51 可重绑）` + 焦点守卫 + 仅 live 态；`gameshop.md` §7.3 记库存零可见差异与 R51 `KeybindOptions.GameShop`，§12 按实际收口/维持 PARTIAL；`docs/testing/round50` 索引登记路径/字节/SHA256，>1 MiB 只登记。
- 命令（均期望 exit 0）：`py tools/docs/check_links.py`（链接 ≥670 无断链）；`git diff --check`。
- H2：每条“已实现”断言须引 `round50-plan.md` §三 对应行与 `target/round50/**` 实际证据，零条无证据断言（T1 复核）。
- 证据：`target/round50/r50-h.log`（命令/退出码/diff）。
- 禁止捷径：写 T 路径；把 E 未达的阈值写成 0；改 R47/R49 历史日志/PNG/冻结文件。
- PARTIAL：B/C/E/F 任一 PARTIAL 未在文档标注；无证据断言；链接检查红。

### R50-T 只读终审 · `JJC-20261025-002-sub-f2318e27`
- 承接：刑部 `xingbu` / `gpt-6.1-sol`｜依赖：**队长唯一全量门禁完成**｜写范围：仅 `docs/testing/round50/round50-final-audit.md`。
- 核对：本次原始退出码、起止、日志路径/字节/SHA256、末行 `==> 全部通过（N 项）`（N=当次脚本实际项数，不预写 12）、逐项状态、HEAD+porcelain；9 子单 PARTIAL 边界、捷径黑名单、证据路径/哈希一致性、>1 MiB 只登记；不得自跑门禁、不补跑 ignored、不代修。
- 命令（只读；期望依据）：`git status --porcelain`；`git diff --stat`；`Get-FileHash target/round50/<本次门禁日志>`；读取门禁日志与各子单证据。
- T1 通过条件：原始 exit 0 + 末行 N 项 + 无 `[FAIL]`/`[ERROR]`；不得固定 12/12；跨工作树/旧日志一律无效。
- 证据：该报告 + 队长门禁日志；其余文件只读。
- 禁止捷径：自跑 `ci.ps1`/`-KillStale`；改实现或补证据；把审计报告写出当成门禁通过。
- PARTIAL：门禁非 0、N 与逐项不符、日志 SHA 不符、缺逐项证据、任一子单 PARTIAL 未汇总。

## 三、模型路由表

| 子单 | 部门 | 产出性质 | 模型 | Lead 启动方式 |
|---|---|---|---|---|
| R50-01 | 礼部 | 实证/只读结论 | gpt-6.1-sol | `subagent` 显式 `provider:"sol-relay", model:"gpt-6.1-sol"` |
| R50-A | 礼部 | 门禁状态文档 | gpt-6.1-sol | 同上 |
| R50-B | 兵部 | 服务端编码+测试 | deepseek-flash | `spawn_teammate`（provider 继承 runtime spawn） |
| R50-C | 工部 | 客户端编码+测试 | deepseek-flash | 同上 |
| R50-D | 户部 | 脚本/manifest | deepseek-flash | 同上 |
| R50-E | 兵部 | 编码+配置+阈值 | deepseek-flash | 同上 |
| R50-F | 刑部 | 测试实现+证据脚本 | deepseek-flash | 同上 |
| R50-H | 礼部 | 文档/i18n 规范 | gpt-6.1-sol | `subagent` 显式 `sol-relay`/`gpt-6.1-sol` |
| R50-T | 刑部 | 只读终审 | gpt-6.1-sol | 同上 |

- deepseek 子单由 Lead 用 `spawn_teammate` 启动，成员 provider 继承 runtime spawn，**不手填 `sol-relay`**；gpt 子单必须显式传 `provider:"sol-relay", model:"gpt-6.1-sol"`，漏填即回落 deepseek、路由失效。
- Lead 维护 `SUB_TASK_ID ↔ agent_id` 映射，启动后交尚书登记（memo/本文件）；执行者退出、失败、换人时同步更新。
- 一子单一执行者，不混模型；gpt 治理会话深度受限，不得再拉执行者链；实现缺口一律出判据交 Lead 启动。
- 唤醒节流：按 §一 波次启动，一次一个唤醒、收到回执再派下一个；遇 `Upstream rate limit exceeded` 用 `tools/dev/rpm_governor.py` 降速，不重复追发。

## 四、轮次提交与门禁口径

1. **A 批先单独提交**：仅 A 的 3 个文件，A1/A2 绿后由 Lead 提交（“R50-A 门禁行刷新”）；不与功能批混提，避免相互拖累。
2. **功能批**：B、C、D、E、F、H 成果一起提交；`docs/testing/round50/` 证据索引随批入库（>1 MiB 仅登记路径/字节/SHA256）。
3. **队长唯一全量门禁**：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；保留原始进程退出码、起止时间、日志路径/字节/SHA256。绿 = **原始退出码 0** + 末行 `==> 全部通过（N 项）`（N 为当次脚本实际项数；本树 `ci.ps1` 现有 13 处 `Invoke-Gate` 调用含条件项，R49 为 12，均不得预写）+ 无 `[FAIL]`/`[ERROR]`；不得以管道尾部成功码代门禁码。
4. **只跑一次且不并发**：提交前先跑 A2/E2/C2 子集；`-KillStale` 期间不跑 ignored/soak；`target/soak.lock` 存在时禁 `-KillStale`。
5. **审计产物门禁后写**：T 报告与 final-manifest 在门禁后写入，不使门禁失效（R49 §10 口径），但 T 必须单列文件清单+哈希；门禁前不得预写终态。
6. **回滚**：任一门禁项红只回退本批文件，不删证据、不改历史日志；重跑必须换新日志名。
7. 提交前 Lead 只读核对 confirm 已生效（门下/尚书按 `confirm_by`），Lead 不代 confirm、不代签。

## 五、风险与交付纪律

| 风险 | 预案 |
|---|---|
| A 越界 R49 冻结目录 | 只 3 行 + 1 节；前后 `git diff` 全文与 `hash-object` 留证；越界即 PARTIAL |
| E 实测到不了 0 | 维持阈值 15 + PARTIAL；`docs/i18n.md` 只写“目标 0，实测 N”，不写 0 |
| B 包序错（失败也发库存） | 事务成功后按条件发；失败零包由 `gameshop_failure` 断言；违例 PARTIAL |
| B 若 PARTIAL | C 只交离线面板 + 空态/断线态，真实链路验收挂 B |
| D 缺图标/双路径 | 占位 + PARTIAL，不伪造素材、不改授权口径 |
| F 污染冻结证据 | 方案 a：env 覆盖 + 默认值不变；跑前跑后冻结目录 SHA256 零变化 |
| H 证据通胀 | 只写有证据的“已实现”；i18n 数字用 E 实测；链接红即 PARTIAL |
| G 已删除 | `list --all` 实测 91 条全终态（63 Done+27 Cancelled+1 Menxia），25 条口径不成立；只许新增 memo，零状态写 |

- 证据统一 `target/round50/**`；单文件 ≤1 MiB，超限只登记路径/字节/SHA256。
- 全局黑名单：GM/传送命令造状态、改 DB、伪造库存、静态 fixture 当真实链路、`SKIP`/缺素材 `return Ok`、硬编码坐标、抓包改包、旧日志/旧截图冒充本次、固定 12/12 或 15 例或 25 条、跨工作树/旧日志、把 TextKey 词表当匹配表。
- 六部不 `state`/不 `confirm`/不 git；尚书不 spawn、不跑 cargo/门禁；全员不得改历史日志与冻结证据。

## 六、Lead 启动清单（按 SUB ID）

| 波次 | SUB_TASK_ID | 执行者 | 首条消息要点 |
|---|---|---|---|
| 1 | `JJC-20261025-002-sub-b9c2de60` | libu/gpt | 读方案+评审；`recall` 父/子；只写 `target/round50/facts`；先建 `verify.py` 基线再全量核对；回报偏差清单 |
| 2 | `...-sub-dfd9ba4c` | libu/gpt | 等 01 行号；只改 3 文件；用 §二 A 模板措辞；零 `current_gate=PARTIAL`；证据 `r50-a.log`；不 commit |
| 2 | `...-sub-2b2da8e0` | bingbu/deepseek | 读 `market.rs:96-98`、`session.rs:3047/:3157`、登录链 `:1179-1290`；先写 wire 失败基线再实现；协议不改；完成后通知释放 `market.rs` |
| 2 | `...-sub-d38516b3` | hubu/deepseek | 等 01 的 GIndex；只读 `resource/`+`mir2_dev`；先 `--check` 后 `--verify-icons`；缺项 PARTIAL |
| 2 | `...-sub-5f8f99a4` | xingbu/deepseek | 先加 env 覆盖（默认不变）→ 冻结节 SHA 清单 → 带 env 重跑；产物只进 `target/round50` |
| 3 | `...-sub-7d8f26cc` | gongbu/deepseek | 等 B 完成；K 键仅 live + 焦点守卫；先做 `net.rs` 消费/缓存再 UI；不写 `market.rs` |
| 3 | `...-sub-177decbe` | bingbu/deepseek | 等 B 释放 `market.rs`；先测 15 基线再三分；别名三列；非 0 不得改阈值 |
| 4 | `...-sub-a78487f6` | libu/gpt | 收齐 B/C/E/F 证据后写 4 文档 + round50 索引；i18n“目标0实测N”；每条“已实现”引证据 |
| 5 | `...-sub-f2318e27` | xingbu/gpt | 等 Lead 门禁日志；只写终审报告；核对原始退出码/N 项/SHA；不跑门禁；回报 verdict |

- 每条首条消息必须含：父/子 ID、允许写范围、依赖等待条件、验收命令与期望退出码、证据路径、禁捷径、PARTIAL 边界；要求执行者先 `recall <父ID>`、`recall <自身子ID>`、`progress`、`todo`（同刻仅 1 个 in-progress）。
- 执行者完成用 `delegate-result` 回填，不 `state`、不验收、不 `git`；Lead 启动/结束时把 `SUB ↔ agent` 映射交尚书登记。
- 本派发只登记契约与顺序，未启动任何执行者、未运行 cargo/门禁/git；9 子单登记成功即 Doing，不等于开工或通过。
