# DSH 工具链选型（插件与技能）

本文记录《传奇-Next》开发过程中，从
[awesome-dsh-plugin](https://github.com/awesome-dsh-plugin/awesome-dsh-plugin) 里挑选并落地的
DSH 插件与技能：选了什么、为什么选、怎么配置、怎么验证、怎么回滚。

选型原则只有三条：

1. **贴着本仓库的真实痛点**：10 个 crate 的 Rust 工作区、Postgres 素材索引、严格的回合门禁、
   长会话里的大段命令输出。
2. **不重复已有的能力**：三省六部（edict）、Agent Teams、vision-router、playwright 抓取、
   工作区自带的文档体系已经在跑，不再叠同质工具。
3. **可回滚**：每个插件都能一条命令卸掉，每处配置都有备份。

---

## 1. 落地清单

| # | 名称 | 类型 | 版本 | 解决什么 | 状态 |
|---|---|---|---|---|---|
| 1 | `dsh-lsp-actions` | 插件 | 0.5.16 | 用真实 rust-analyzer 提供 `lsp_diagnostics` / `lsp_symbols` / `lsp_rename` 等，改前就能看到类型错误 | 已装，**配置需重启生效** |
| 2 | `@jayyuen66/dsh-quality-gate` | 插件 | 0.1.2 | 回合收口时对改过的代码跑 `cargo check`，失败则把错误注入下一轮，不让"没编译过"的回合结束 | 已装并已挂载 |
| 3 | `dsh-db-connector` | 插件 | 0.1.0 | `db_query` / `db_schema` / `db_exec` 直连 Postgres，用来查 `mir2_dev` 素材索引与 `mir2` 存档库 | 已装，挂载失败待重启复核（§4.3） |
| 4 | `@deepseek-ai/dsh-experimental-session-search` | 官方插件 | 0.2.1-rc.2 | 让 agent 检索本工作区历史会话（复活旧轮次的决策与证据） | 已装并已挂载 |
| 5 | `local-solution-first` | 技能 | 仓库 HEAD | 移植/实现前先在本机与参考仓库找现成方案，用证据分级（已验证/部分验证/推断/未知）记录 | 已装，已进技能目录 |
| 6 | `verify-cheap` | 技能 | 仓库 HEAD | L0–L3 验证成本阶梯 + `wait-until.mjs`（条件轮询替代 sleep）+ `measure-runtime.mjs`（实测耗时） | 已装，已进技能目录 |
| 7 | `code-review-7d` | 技能 | 仓库 HEAD | 五步审查、三级严重度、四维评分、文字/HTML 报告，用于提交前与御史台复核 | 已装，已进技能目录 |

依赖补充：为让 `db-connector` 能连 Postgres，profile 里额外装了 `pg@8.23.1`
（插件把 `pg` / `mysql2` 声明为可选 peer 依赖）；rust-analyzer 通过
`rustup component add rust-analyzer` 补齐（1.99.1）。

### 为什么没选

| 候选 | 放弃理由 |
|---|---|
| superpowers / Aegis / spec-driven 系列 | 与三省六部 + Agent Teams 的"计划—执行—复核"职责重叠，且一次性塞 14–25 个技能会让技能目录膨胀、每轮都吃 token |
| pixel-studio / gpt-image / game-material-master | 需要外部图像 API key（本机没有），或面向 Aseprite/视频工作流，与 Mir2 的 WIL/WIX 256 色图库格式不对口 |
| git worktree 系列 | 本仓库当前有多轮未提交改动，且 launcher 以单工作区为假设；并行 worktree 会先把提交纪律搞乱，暂不引入 |
| memory / 记忆类插件 | 项目本身已有 `docs/` 决策链 + 看板 + 会话检索，再叠一层记忆会分裂事实来源 |
| lsp-packs 的 Rust 包（`@dsh-lsp-packs/dsh-lsp-rust`） | 未发布到 npm；`dsh-lsp-actions` 本身就能配置 rust-analyzer，且功能面更全 |

---

## 2. 生效方式（必须重启一次）

四个插件都是 bundle 层，`plugin_manager` 报告 `restart-required`：包已进 profile
（`~/.dsh/profiles/desktop/package.json` 的依赖与 `dsh.profile.bundles` 都已登记），
但**运行中的进程仍用旧配置**。因此：

- `quality-gate` / `session-search` 已经热挂载，可直接用；
- `lsp-actions` 的工具已在，但 `servers` 还是空表 —— 重启后才读到下面的配置；
- `db-connector` 挂载失败（见 §4.3），重启后复核。

重启 DSH 桌面端即可；本会话的对话记录不丢。

---

## 3. 配置

### 3.1 rust-analyzer（`~/.dsh/profiles/desktop/cordis.patch.yml`）

`dsh-lsp-actions` 的默认 `servers` 是空表（等于不启用任何语言服务器）。
在 profile 的补丁层加一条 id 覆盖（`config` 是整块替换，所以把默认值一并重述）：

```yaml
- id: lsp-actions
  name: dsh-lsp-actions
  config:
    servers:
      rust:
        command: rust-analyzer
        args: []
        extensionToLanguage:
          .rs: rust
        projectMarkers:
          - Cargo.toml
        initializationOptions:
          cargo:
            buildScripts:
              enable: true
        idleTimeoutMs: 0        # 常驻，避免反复冷启动索引
    editor:
      enabled: false
      requestTimeoutMs: 60000
      diagnosticsCacheMaxFiles: 64
    maxDiagnostics: 200
    # …其余上限沿用默认值（见下）
```

完整默认值：`maxCompletionItems: 20`、`maxCodeActions: 50`、`maxSymbols: 100`、
`maxSignatures: 10`、`maxInlayHints: 200`、`maxResultChars: 16000`、`timeoutMs: 60000`。

### 3.2 数据库连接（不写进配置）

`db-connector` 的配置里可以预登记连接，但本机 `.env` 的变量**不在 DSH 进程环境里**，
写 `${DATABASE_URL}` 占位符会展开成空值。约定改为**运行时连接**：

```text
db_connect { "action": "connect", "name": "mir2_dev",
             "config": { "driver": "postgres",
                         "connectionString": "<从 .env 的 MIR2_DEV_DATABASE_URL 读取的值>" } }
```

连接串只出现在这一次工具调用里，不进 profile 配置、不进 git。
`db_query` 默认只读（行数上限 1000、30s 超时），写入必须显式 `allowWrite: true`，
全部审计落到 `.dsh-db/audit.jsonl`。

### 3.3 quality-gate 默认行为

- 触发条件：本回合编辑过代码/配置文件（`.md` 等非代码面豁免）。
- 动作：从被改文件向上找最近的清单根（本仓库是根 `Cargo.toml`），跑 `cargo check`。
- 预算：单次 300s，每回合最多注入 2 次修复指令；失败才注入，通过不打扰。
- 代价：每轮多一次增量 `cargo check`。本机实测（2026-10-24，target 已热）：
  `cargo check --workspace` 1.57s / 墙钟 1.7s，exit 0 —— 成本可忽略。
  若任务现场 target 是冷的，首次会到分钟级，届时可在设置页或 profile 补丁里把
  `enabled` 置 false。

---

## 4. 验证证据

| 项 | 证据 |
|---|---|
| profile 清单 | `package.json` 的 `dependencies` 与 `dsh.profile.bundles` 均含 4 个新 bundle；`node_modules` 下 4 个包版本与声明一致 |
| YAML 合法性 | 用 profile 内的 `js-yaml` 解析 `cordis.patch.yml`：14 个条目，`lsp-actions` 的 `servers.rust` 内容与预期一致 |
| rust-analyzer | `rust-analyzer --version` → `1.99.1 (58cf8f9c 2026-10-13)`，工具链由仓库 `rust-toolchain.toml` 固定 |
| Postgres | 用 `pg` 直连：`mir2` 14 张表、`mir2_dev` 16 张表，均 `OK` |
| 门禁开销 | `cargo check --workspace` 1.57s（target 热）、exit 0，quality-gate 的每轮成本可忽略 |
| 技能注册 | 会话技能目录已出现 `local-solution-first`、`verify-cheap`、`code-review-7d` |

### 4.1 插件挂载状态（`plugin_manager list_plugins`）

```text
include:lsp-actions     active
include:quality-gate    active
include:optional-tool-session-query  active
include:db-connector    failed   ← 见 §4.3
```

### 4.2 技能目录

```text
~/.dsh/skills/local-solution-first/{SKILL.md,references/…}
~/.dsh/skills/verify-cheap/{SKILL.md,references/…,scripts/…}
~/.dsh/skills/code-review-7d/{SKILL.md,references/…,templates/…}
```

`code-review-7d` 上游没有 YAML frontmatter，安装时补了 `name`/`description` 并改名为
`code-review-7d`（技能名需以字母开头）。

### 4.3 `db-connector` 未挂载

现象：`fiberPhase: failed`。已排查：

- 模块本身在 Node 24 下 `apply()` 正常，注册 5 个工具
  （`db_connect` / `db_schema` / `db_query` / `db_exec` / `db_audit`）；
- 已对 `dist/index.js` 打两处防御补丁（原文件备份为 `dist/index.js.orig`，
  属卸载/升级会被覆盖的本地补丁）：
  1. `ctx.logger` 缺失时退回 no-op logger；
  2. `/db` 命令注册失败不再拖垮整个插件（工具照常注册）；
- 仍失败，待重启后复核；若仍失败，按 §6 卸掉，改用 `pg` 直连脚本查库。

> 该失败是**隔离的**：Cordis 只标记该 fiber 失败，不影响其余插件与宿主启动。

---

## 5. 使用方式

- **改 Rust 前**：`lsp_symbols` 找定义、`lsp_diagnostics` 看现有告警，再动手；
  `lsp_rename` 做跨 crate 重命名（比全仓库字符串替换安全）。
- **回合收口**：quality-gate 自动跑 `cargo check`；失败信息会作为下一轮输入。
- **查素材索引**：`db_connect` + `db_query` 查 `mir2_dev`，不要再靠逐行读索引文件。
- **验证改动**：`verify-cheap` 技能给出 L0–L3 选档；等后台作业用
  `node ~/.dsh/skills/verify-cheap/scripts/wait-until.mjs --timeout 120 -- <命令>`，
  不要 `Start-Sleep`。
- **移植/选依赖前**：`local-solution-first` 要求先查本机与 `github/` 下的参考仓库，
  并给出证据等级，避免重复造轮子或误判兼容性。
- **提交前**：`code-review-7d` 按五步流程过一遍 diff，输出分级结论。
- **翻旧账**：会话检索工具可以搜历次会话里的决策与证据，比翻聊天窗口快。

---

## 6. 回滚

```powershell
# 1) 卸插件（任选其一）
#    a. 在 DSH 设置 → 插件里关闭/卸载；
#    b. 命令行（profile 被桌面端独占时用不了，只能在应用内操作）
# 2) 还原 profile 配置
Copy-Item "$HOME\.dsh\profiles\desktop\cordis.patch.yml.bak-20261024-lsp" `
          "$HOME\.dsh\profiles\desktop\cordis.patch.yml" -Force
# 3) 删技能
Remove-Item -Recurse -Force "$HOME\.dsh\skills\local-solution-first",
                               "$HOME\.dsh\skills\verify-cheap",
                               "$HOME\.dsh\skills\code-review-7d"
# 4) 如要连 pg 一起清掉
pnpm --dir "$HOME\.dsh\profiles\desktop" remove pg
```

`cordis.patch.yml.bak-20261024-lsp` 是加入 LSP 条目之前的原样备份；
若只想撤 LSP 配置，删掉 `id: lsp-actions` 那一段即可。

---

## 7. 风险与注意

1. **第三方插件供应链**：4 个插件中 3 个来自社区 npm 包。`db-connector` 已出现 0.2.x
   兼容问题；社区包升级前先在隔离 profile 里试装。
2. **profile 被桌面端独占**：`dsh plugin --profile desktop` 会被拒绝，只能在应用内
   通过插件管理或 `plugin_manager` 操作；手工改 `package.json`/锁文件容易与插件管理器打架。
3. **LSP 常驻内存**：`idleTimeoutMs: 0` 让 rust-analyzer 常驻，10 个 crate 的索引大约占
   1–2 GB；机器吃紧时改成 `idleTimeoutMs: 300000`。
4. **`cargo check` 门禁与并行构建抢锁**：quality-gate 与人的 `cargo build/test` 同时跑会
   排队，极端情况下 300s 预算耗尽，会注入"未核验"提示而不是静默放行。
5. **`.env` 不在进程环境**：数据库连接串每次运行时从 `.env` 读取后传入，不写进配置或提交。
