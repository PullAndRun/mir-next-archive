# 仓库体积与产物管理（约定自 2026-10-22 起）

> 用户要求：**控制项目体积，只保留"当前正在用或以后要用"的产物；后续每轮都遵守。**

## 1. 哪些东西允许留在仓库（会进版本库）

| 类别 | 示例 | 说明 |
|---|---|---|
| 源码 | `crates/**/*.rs` | 唯一真源 |
| 数据/配置 | `config/**`、`crates/mir-store/migrations/**` | 运行时与迁移 |
| 文档 | `docs/**`、`README.md` | 含验收证据（很小） |
| 小体积证据 | `docs/testing/evidence/**`（单文件 KB 级） | gate 关键日志，便于追溯 |
| 工具脚本 | `tools/**`（*.py/*.ps1） | 不含生成物 |

**允许入库的单个文件上限：1 MB**（超过就要换一种保存方式，例如摘要 + 复现命令）。

## 2. 哪些东西**不要**留在仓库

| 类别 | 示例 | 处置 |
|---|---|---|
| 编译产物 | `target/**`、`dist/**/mir-*.exe` | `.gitignore`；用 `tools/dev/package.ps1` 现场生成 |
| 成员一次性验证副本 | `target/*-verify/`、`target/ci-sim/`、`target/t*-verify/` | `tools/dev/clean.ps1` 删除 |
| 运行日志 | `target/*.log`、`*.err.log` | 超过 3 天自动清；关键证据复制到 `docs/testing/evidence/` |
| 生成缓存 | `tools/playtest/data/mapgrid-*.bin`、`__pycache__/`、`*.pyc` | `.gitignore` |
| 备份/临时包 | `*.bundle`、`*.bak`、`target/tmp/` | `clean.ps1` 删除 |

## 3. 每轮收尾动作（队长/户部）

```powershell
pwsh -NoProfile -File tools/dev/clean.ps1 -DryRun   # 先看会删什么
pwsh -NoProfile -File tools/dev/clean.ps1           # 清临时产物（不触发重建）
pwsh -NoProfile -File tools/dev/clean.ps1 -Deep     # 只在磁盘告急时（默认 <120GB 会拒绝，需 -Force）
```

`clean.ps1` 的安全闸：检测到 `cargo`/`rustc`/`mir-server` 在运行时**直接退出（exit 2）**，不会误删正在用的东西。

## 3.1 清理频率（不要频繁清）

| 动作 | 频率 | 说明 |
|---|---|---|
| 默认 `clean.ps1`（日志/临时 exe/`*-verify`/pycache） | 每轮收尾 | **不触发重编译**，写盘几乎为零 |
| `-Deep`（`cargo clean` 整个 target） | **仅磁盘告急（<120GB）或产物损坏时** | 全量重建会额外写 5–10 GB；磁盘充足时脚本会拒绝（需 `-Force`） |
| 删除/复制 `target` 做出独立验证目录 | **禁止** | 这是上次 247 GB 的主因；用共享 `CARGO_TARGET_DIR` 或直接在主干跑 |

## 3.2 对 SSD 寿命的影响（结论：正确用几乎无影响）

- **删除文件**只改元数据 + TRIM，几乎不产生写入；真正磨损来自**重新编译的写入**。
- 一次全量重建约写 **5–10 GB**；本机 C: 是 2TB NVMe（Solidigm），典型 TBW 数百 TB，
  即使每天全量重建（≈300 GB/月）也远低于寿命阈值——但**没必要**，因为干净清理不重建。
- 真正的浪费是**复制 target 做独立验证**（一次 15–61 GB 写入），已在制度里禁止。
- 建议：把 `clean.ps1` 的默认动作挂在每轮收尾；`-Deep` 只在 `C:` 可用 < 120 GB 时执行一次。

## 4. 历史教训（为什么要立这条）

2026-10-22 实测：`target/` 达到 **247 GB**，其中
- `target/debug` **142 GB**（incremental）
- `target/hubu-verify` **61 GB**、`target/ci-sim` **15 GB**、`target/t72|t77|t80-verify` **26 GB**
  —— 都是成员为了"独立验证"把整份 target 复制出去留下的
- 单个 gate 日志 **109 MB**（死亡后无限重算路径导致的重复诊断）

清理后回收约 **220 GB**。结论：
1. **不要复制 target 做"独立验证"**：用 `CARGO_TARGET_DIR` 指向一个共享目录，或直接在主干上跑（门禁本来就是串行的）；
2. **日志必须限频/限大小**（gate 单次目标 < 2 MB）；
3. 每轮结束跑一次 `clean.ps1`，别把体积留到磁盘告急。

## 5. 与门禁的关系

清理**不影响**门禁语义：`clean.ps1` 不碰源码、不碰 `config/`、不碰 `resource/`、不碰 `github/`。
`-Deep` 之后第一次 `ci.ps1` 会重新编译（项目约 60 个 crate，release 2–4 分钟、全量测试 5–8 分钟），属预期。
