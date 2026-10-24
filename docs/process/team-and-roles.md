# 团队分工与职责（2026-10-25 修订）

> 用途：写死“谁做、用什么模型做、谁签字、谁不能做”。流程细节以三省六部典章与 CLI 为准，本文件不新增状态或命令。
>
> 所有者指令（历史原文）：
> 1. “deepseek模型只负责编码工作，gpt-6.1-sol模型负责编码外的其他工作。工作中的决定和分歧由gpt负责，不要问我。”（路由依据：§1、§2）
> 2. “去除三省六部最大成员数限制，按项目需要变动项目成员。”（规模依据：§3）
> 3. “要用三省六部。不要自己一个人做。”（流程依据：§4）

**一句话**：deepseek 写代码，gpt 做判断与编码外工作，Lead 编排并守住门禁/提交边界；授权范围内的决定和分歧由 gpt 裁决并留痕，不向所有者提问。

## 1. 硬路由表（按任务产出，不按部门名）

| 任务类型 | 典型产出 | 模型 | 启动方式 | 备注 |
|---|---|---|---|---|
| 编码实现 | `crates/**`、`tools/**`、迁移、配置、素材处理 | `deepseek-flash` | `spawn_teammate` | 成员 provider 继承 runtime spawn（当前默认 `shapi-deepseek`/`deepseek-flash`）；**不手填 `sol-relay`** |
| 测试实现 | 单测、集成测试、`--ignored` 证据、复现脚本 | `deepseek-flash` | `spawn_teammate` | 只写测试，不判“通过” |
| 批量重构 | 跨 crate 重命名、机械迁移 | `deepseek-flash` | `spawn_teammate` | 边界与验收先由 gpt 给 |
| 规划/方案/拆单 | 方案、DAG、子单拆分 | `gpt-6.1-sol` | `subagent`/`workflow` | 必须显式 `provider:"sol-relay", model:"gpt-6.1-sol"` |
| 评审/裁决/判据 | 门下评审、风险裁决、验收标准 | `gpt-6.1-sol` | `subagent`/`workflow` | 同上；不写实现 |
| 证据审计/只读终审 | 哈希/门禁/报告一致性结论 | `gpt-6.1-sol` | `subagent`/`workflow` | 只读；不代修 |
| 文档/i18n 规范 | `docs/**`、术语表、文案规范 | `gpt-6.1-sol` | `subagent`/`workflow` | 纯文档任务不派给礼部 deepseek |
| 看板治理 | 状态迁移、派发、验收、confirm | `gpt-6.1-sol` | `subagent`/`workflow`（gpt 治理会话） | 真实 `--as <角色>`；Lead 只做只读核对 |
| 编排/门禁/git | 启动执行者、唯一全量门禁、提交 | Lead（`deepseek`） | 主会话 | 不做治理判断、不代 confirm、不替 gpt 签字 |

**执行细则**

1. **先定产出再定模型**：产出是代码/测试/脚本/配置/数据变更 → deepseek；产出是判断/方案/评审/判据/文档/规范/证据结论 → gpt。部门名不改变路由；同一子单不得混两种模型，判断与实现拆单。
2. **deepseek 执行者**：统一用 `spawn_teammate` 启动；成员 provider 继承 runtime spawn，不手填 `sol-relay`。启动前先拿到 `SUB_TASK_ID`，把父/子 ID、允许写范围、依赖、交付与验收写进任务包。
3. **gpt 调用**：`subagent`/`workflow` 显式 `provider:"sol-relay", model:"gpt-6.1-sol"`；AgentTeams 成员同样显式（`add_member(..., provider:"sol-relay", model:"gpt-6.1-sol")`）。漏填会回落到默认 deepseek，等同路由失效；回执必须写实际 provider/model。
4. **Lead 边界**：Lead 只做编排与守门——准备任务包、启动执行者、跑唯一全量门禁、执行唯一 git 提交、只读核对看板；不建单/不改状态/不代 confirm/不替 gpt 签字。建单与状态推进由太子/中书/尚书/门下按 CLI 权限执行。`git add/commit` 只由 Lead 执行，其他成员一律不碰。

## 2. 角色-模型分配表（默认值，§1 优先）

| 角色 | 官署/职能 | 默认模型 | 主要承接 | 边界/例外 |
|---|---|---|---|---|
| `taizi` | 太子 | gpt-6.1-sol | 分拣、收旨、最终回报 | 不做技术裁决；不写实现 |
| `zhongshu` | 中书省 | gpt-6.1-sol | 方案、拆单、设计/流程文档、i18n 规范 | 不写实现；编码交六部 |
| `menxia` | 门下省 | gpt-6.1-sol | 独立评审、封驳、对口确认 | 不实现；封驳须给具体问题、`must_fix` 与依据 |
| `shangshu` | 尚书省 | gpt-6.1-sol | 派发、汇总、验收、看板治理 | 不跑全量门禁（Lead）；只按 `confirm_by` 做自己那一类确认，不代门下 `confirm` `Review → Done` |
| `yushitai` | 御史台（职能，非 CLI 角色） | gpt-6.1-sol | 判据、风险裁决、证据审计 | 禁止 `--as yushitai`；看板写入走 `qintianjian`/`xingbu` |
| `bingbu` | 兵部 | deepseek-flash | 服务端、协议、核心逻辑 | 治理判断上报尚书 |
| `gongbu` | 工部 | deepseek-flash | 客户端、渲染、交互 | 同上 |
| `hubu` | 户部 | deepseek-flash | 门禁/工具/性能/持久化 | 同上 |
| `libu` | 礼部 | deepseek-flash | 素材索引脚本、词条替换、UI 文案接入、素材落地 | 文档/i18n 规范/文案裁量 → gpt（同一礼部角色，换模型） |
| `xingbu` | 刑部 | deepseek-flash | 测试实现、复现脚本、只读核对执行 | 只读终审/验收判断 → gpt（同一刑部角色，换模型） |
| Lead | 队长（主会话） | deepseek-flash | 编排、启动执行者、唯一全量门禁、唯一 git、看板核对 | 不做治理判断、不代 confirm、不替 gpt 签字 |

**关键说明**

1. **§1 优先于本表**：表内是默认值；同一部门承接不同任务时按 §1 换模型。礼部写文档、刑部做终审都必须走 gpt，角色名不变；`libu_hr`（吏部，Skill/Agent 管理与评估）属编码外治理，按 §1 走 gpt，不计入 deepseek 实现六部。
2. **御史台不是 CLI 角色**：CLI 没有 `yushitai`，不能靠冒名取得权限。需要看板留痕时：测试/审查/只读终审挂 `xingbu` 子单（尚书可派）；度量/性能用 `qintianjian` 角色留痕（其唯一出边是 `qintianjian → shangshu`，尚书不能反向派活；需要新建任务时由太子/中书按 `create` 权限立单）。父单的正式 `confirm` 由门下/尚书按 `confirm_by` 执行；御史台判断以 `xingbu`/`qintianjian` 的 memo/报告留痕。
3. **文档/i18n 路由**：编码外工作统一 gpt。常见落点：中书省（方案/设计/流程文档）、礼部（文档/素材规范，gpt 承接）、御史台/门下（审计/评审报告）。礼部 deepseek 只做已定文案/素材的代码与数据落地。
4. **六部不给自己签字**：六部可以定向自测，但不能判自己“通过”；六部用 `delegate-result` 回填，不 `state`、不代父任务验收。写测试=deepseek、只读终审=gpt，同一子单只能选一种，终审发现缺陷只报告、不代修。验收由尚书组织，结论由 gpt 门下/御史台独立判断。

## 3. 团队规模与成员管理

- **取消固定上限**：不再设 16 人上限，按批次、并行写范围、依赖和证据工作量增减成员；不为了凑人数开成员，也不为了省成本让一个成员串行覆盖本可并行的独立写范围。
- **配置事实**：`~/.dsh/profiles/desktop/cordis.patch.yml` 的 `agent-team` 已设 `maxMembers: 256`、`maxTasks: 256`。256 是当前配置上限（高于实际需求），不是固定编制或目标人数；需要更多成员时按项目需要调整该配置。

| 场景 | 动作 |
|---|---|
| 多个互不重叠的写范围 | 各建子单、各配执行者；编辑可并行，唤醒串行 |
| 同一文件/manifest/冻结目录 | 不并行；指定唯一写者，其余只读或等待交接 |
| 需要独立判断 | 加 gpt 治理成员；判断完即释放 |
| 子单完成/长期空闲 | 释放执行者，保留证据、映射与 memo；空闲成员移除，避免上下文/RPM 空耗 |

- **Lead 维护映射**：每次 `delegate` 后保存 `SUB_TASK_ID ↔ agent id`；Lead 启动执行者后把映射交给尚书登记（memo/派发文档），并保留核对清单。执行者退出、失败或换人时同步更新。
- **gpt 会话深度**：治理会话有 subagent 深度限制（典章上限 ≤3，且可能已被上层消耗）。执行者一律由 Lead 启动，不要求 gpt 治理者再拉执行者链；gpt 需要实现时，出判据/子单要求，交 Lead 启动。
- **唤醒节流与限流**：所有模型共享 RPM；一次只唤醒一个成员，收到回执再派下一个，不并行开多个 `subagent`/`workflow`。出现 `Upstream rate limit exceeded` 时用 `tools/dev/rpm_governor.py` 降速/等待；成员报 `INVALID_REQUEST` 或长时间无响应时，重启其单轮任务，不反复追发。
- **成本纪律**：不精算 sol 成本，但别浪费；gpt 单任务闭环，输入裁剪到判断所需（方案摘要、关键 diff、实测数字），输出 `verdict + ≤3–5 条 findings + 一行总结`。

## 4. 标准节奏（edict，不另造流程）

节奏：太子分拣 → 中书规划 → 门下评审 → 尚书派发 → 六部执行 → 尚书验收 → 对口确认 → Lead 提交。

主链：`Taizi → Zhongshu → Menxia → Assigned → Doing → Review → PendingConfirm → Done`（另有 `Pending`/`Next`/`Blocked`/`Cancelled`，以 `node "...edict.mjs" states` 和 `protocol.md` 为准）。开朝由太子（gpt）按 CLI `create` 建单（初始 `Taizi`），Lead 准备任务包；已建单先 `show`/`recall` 接续。

| # | 步骤 | 产出 | 负责 | 门禁/备注 |
|---|---|---|---|---|
| 1 | 事实 | 现状、路径、行号、实测数据 | Lead 汇总 | 只列可复核事实；需要独立取证时由 gpt 只读复核，不让 gpt 做长工具循环 |
| 2 | 判据 | 验收标准、禁止捷径 | gpt 御史台/门下 | 可判定、fail-closed；阈值高于噪声 |
| 3 | 方案 | 技术方案 + DAG | gpt 中书 | 不写实现；需要编码交六部 |
| 4 | 评审 | 通过/封驳 | gpt 门下 | 最多三轮；致命问题必拒；封驳给 `must_fix` |
| 5 | 派发 | 子单 + 写范围 + 依赖 | gpt 尚书 | gpt 尚书 `delegate` 拿 `SUB_TASK_ID`；Lead 再启动执行者；写范围不重叠才并行 |
| 6 | 实现 | 代码/测试/证据 | deepseek 六部 | Lead 启动执行者；一子单一人；不得越界 |
| 7 | 定向自测 | 本子单测试 | deepseek 执行者 | 只证明本范围，不等于全量绿 |
| 8 | 全量门禁 | 本次 gate 日志 + 原始退出码 | Lead | 唯一执行者；`ci.ps1`（`target/soak.lock` 存在时禁 `-KillStale`）；一人一树；不并行 ignored/soak |
| 9 | 验收 + 只读终审 | 验收结论、审计报告 | gpt 尚书；gpt 刑部/御史台 | 尚书核对 todos/委派后请求 Done；终审只读，不代修 |
| 10 | 确认 + 提交 | `confirm` 记录 + commit | gpt 门下（`Review → Done` 的 `confirm_by`）；Lead（git） | Lead 先核对 confirm 已生效、门禁证据齐备；不代 confirm |
| 11 | 留痕 | memo + 文档 + 证据路径 | 尚书/Lead | 终态不可回填；续做新建关联任务 |

**派活矩阵（CLI 唯一白名单）**

| 派发方 | 可派给 |
|---|---|
| `taizi` | `zhongshu` |
| `zhongshu` | `menxia` / `shangshu` |
| `menxia` | `shangshu` / `zhongshu`（封驳） |
| `shangshu` | `zhongshu` / `menxia` / 六部（含 `libu_hr`）；当前未开放 `qintianjian` |
| 六部 / `qintianjian` | `shangshu`；`zaochao` 不可派活 |

**对口确认（`confirm_by`）**

| 请求 | 中间状态 | `confirm_by` |
|---|---|---|
| `Review → Done` | `PendingConfirm` | `menxia` |
| `Doing → Cancelled` | `PendingConfirm` | `shangshu` |
| `Menxia → Cancelled` | `PendingConfirm` | `zhongshu` |

**不可绕过的规则**

- 终态 `Done`/`Cancelled` 冻结，续做新建关联任务。`Doing` 禁止直接 `state Done`，先 `done` 进 `Review`；`done`/`Review` 要核对 todos 与全部委派结果。
- `PendingConfirm` 只能由 `confirm_by` 执行 `confirm`；Lead/`main` 无通用例外，尚书只做 `Doing → Cancelled` 这类指定确认。封驳/裁决写 `memo`，不塞 `state` 备注（会截断）。
- 执行者可用新证据申请复核，但不能绕过 gpt 改判；gpt 改判也要留痕。
- 只经 CLI 写看板，不手改 JSON；`exit 3/4` 先补前提/权限，不原样重试或换身份规避。
- 六部与 gpt 都不执行 `git add/commit`；提交只由 Lead 执行。

## 5. 证据纪律（不可妥协）

### 5.1 判绿红线

- 不许假绿：未运行、失败、缺素材/PNG、缺 ignored 输出、哈希不符 → `PARTIAL`；旧日志/旧截图不算本次证据。
- 不固定“12/12”“13/13”：读本次 `tools/dev/ci.ps1` 实际产物、总数/通过数和原始退出码；缺一项 fail-closed。
- 阈值高于噪声（RSS 抖动 ±7–9 MB 时，10% 判据在 40 MB 基线下低于噪声）；否则加噪声地板或改判据。
- 证因果链，不证终态；“恰好满足”不能排除残留状态。
- 玩家可感知失败必须有反馈（i18n），不能只写日志。
- 随机性允许、作弊不允许；测试辅助代码不进生产（`#[doc(hidden)]` ≠ `#[cfg(test)]`）。

### 5.2 每次交付的证据包

| 证据项 | 最低要求 |
|---|---|
| 命令/时间 | 原文 + 工作目录 + 起止时间（时区） |
| 退出码 | 原始进程退出码；`Tee`/管道末端不算 |
| 日志/产物 | 路径 + SHA256 + 字节数 + 实际通过/总数 |
| 工作树 | `git rev-parse HEAD` + `git status --porcelain` |
| 边界 | 风险、未决项、`PARTIAL` 范围 |

### 5.3 写入、冻结与体积

- manifest 同一时刻只有一个写者；交接时记录基线哈希与变化原因，不静默覆盖前一阶段结果。
- 冻结目录只读（如 `docs/testing/round49/**`）；新批次写 `target/round49/**` 或新目录；证据生成器不得写已入库冻结目录，旧证据原样保留。
- 单文件 >1 MiB 不入库：只登记路径、字节数、SHA256 和本地可复核引用；不复制、不提交、不删除原文件。
- 全量门禁一次只一人；`target/soak.lock` 存在时禁止 `-KillStale`；不并行 ignored/soak/全量门禁。
- 只读终审只写允许的审计产物，不代修、不补跑 ignored 测试、不改实现。

### 5.4 捷径黑名单

| 类别 | 禁止项 |
|---|---|
| GM/传送 | `@revive`、`@repair`、`@give`、`@teleport`、`@map`、`@mapinfo` |
| 数据 | 改 DB、伪造库存、静态 fixture 当真实链路 |
| 进程/测试 | 强杀冒充正常退出、`SKIP`、缺素材 `return Ok`、硬编码坐标 |
| 证据 | 抓包改包、旧日志/旧截图当本次绿、把社区/仿制素材冒充 Crystal 原作 |

出现任一即 `PARTIAL` 或封驳；不得以“测试需要”为由豁免。

## 6. 本轮教训 → 流程约束

| # | 教训/现象 | 现在必须 |
|---|---|---|
| 1 | 子单进入 `Done`/`Cancelled` 后回填被拒（终态不可写） | 派活前 `show` 核对状态可写；终态续做新建关联任务，不改旧单 |
| 2 | 同一子单多个执行者会撞状态/进度 | 一子单一执行者；并行任务各建子单，写范围不重叠 |
| 3 | gpt 路由漏传 `provider`/`model` 会回落到 deepseek | 所有 gpt 调用显式传 `sol-relay`/`gpt-6.1-sol`；回执记录实际路由 |
| 4 | `--ignored` live 证据测试缺素材时假绿 | 缺 PNG/素材必须 assert 失败或 `PARTIAL`；不得 `return Ok` 早退 |
| 5 | 证据生成器写已入库冻结目录，污染冻结哈希 | 新批次写 `target/` 或 round49 新目录；冻结目录只读 |

## 7. 调整留痕

| 日期 | 依据 | 变更点 |
|---|---|---|
| 2026-10-21 | 所有者：引入 sol；去掉上限 | 新增御史台；sol 定验收/deepseek 实现；团队上限 8 → 16（**已被 2026-10-25 规则取代**） |
| 2026-10-25 | 所有者原文 1 | deepseek 只编码；gpt 负责编码外全部工作与决定；文档/i18n/评审/判据/证据审计改 gpt；Lead 不做治理判断、不代 confirm |
| 2026-10-25 | 所有者原文 2 | 取消固定成员上限；`agent-team.maxMembers=256`；按需增减 |
| 2026-10-25 | 所有者原文 3 | 标准节奏回到三省六部；保留状态机/派活矩阵/confirm 门禁 |
| 2026-10-25 | [R49 门下评审](round49-menxia-review.md)、[R49 尚书派发](round49-shangshu-dispatch.md) + 本轮教训 | 固定“12/12”改为实际门禁 fail-closed；manifest 串行；冻结目录只读；>1 MiB 不入库；ignored 缺素材不得假绿；§6 五条约束 |

后续任何制度变更，先在本节加一行（日期/依据/变更点）再执行；无记录的口头变更无效。
