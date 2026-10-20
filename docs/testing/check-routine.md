# 巡检例程 check-routine（迁移演练 + 无素材沙箱 + 关键 PG 用例）

> 一句话：**发版前跑 `-Full`，日常改代码后跑 `-Quick`**；`-SelfTest` 用来证明这套巡检「真的会红」。

## 为什么要有它

`tools/dev/ci.ps1` 的 10 项门禁是**每次收口都要跑**的快检查；但有三件事慢到不能塞进门禁，却又必须在
发版前确认一遍：

1. **存档迁移演练**：`0013 → 0016`（含 `0014` 空洞）在**一次性库**上真的能跑通、失败会回滚、`mir2` 不被碰；
2. **无素材沙箱**：没有 `resource/` 的机器上（等价 CI）整套测试必须「缺素材就 skip，而不是红」；
3. **关键 PG 用例**：4 条 `postgres_*` 真的落到 PostgreSQL 上跑（不是 MemoryStore 替身）。

`check-routine.ps1` 把这三步串成一条命令，产出**带时间戳的证据目录**，出问题能直接翻日志。

## 三种模式

| 模式 | 跑什么 | 何时用 | 退出码 |
|---|---|---|---|
| `-Full` | 迁移演练 + 无素材沙箱 + 关键 PG 用例 | **发版前 / 冻结前** | 0 = 全过 |
| `-Quick` | 无素材沙箱 + 关键 PG 用例（**迁移演练 SKIPPED**，summary 里明确标注） | 日常改代码后 | 0 = 全过 |
| `-SelfTest` | 用**真实步骤失败**注入自证会红，然后再跑一次 `-Quick` 证明恢复正常 | 改过本脚本/迁移后 | 1（见下） |

```powershell
# 发版前
$env:MIR2_DRILL_DATABASE_URL = "postgres://postgres:<pw>@nas:3001/mir2_drill"
$env:MIR2_DRILL_ALLOWED_HOSTS = "nas"
pwsh -NoProfile -File tools/dev/check-routine.ps1 -Full

# 日常
pwsh -NoProfile -File tools/dev/check-routine.ps1 -Quick

# 自证会红（注入：把关键 PG 用例指向不存在的库）
pwsh -NoProfile -File tools/dev/check-routine.ps1 -SelfTest
```

## 退出码约定

| 码 | 含义 |
|---|---|
| `0` | 全部步骤 PASS |
| `1` | **至少一个步骤 FAIL**（`-SelfTest` 也是 1：被注入的那次运行确实红了，这是负向自检的预期结果） |
| `2` | 参数/环境错误：缺 `MIR2_DRILL_DATABASE_URL`（`-Full`/`-SelfTest`）、drill 自己 `exit 2`（host 不在白名单/库名前缀不对）、找不到必需的可执行文件 |

## 证据目录

```
tools/dev/evidence/check-routine/<yyyyMMdd-HHmmss>/
├── summary.md                 # 汇总表：每步 PASS/FAIL + 耗时 + 日志路径 + 结论
├── 01-migrate-drill.log       # 迁移演练（-Quick 下不生成，summary 标 SKIPPED）
├── 02-no-assets.log           # 无素材沙箱
├── 03-postgres.log            # 关键 PG 用例
└── migrate-drill.md           # 迁移演练自己产出的证据（快照/checksum/失败注入）
```

- 目录**在仓库内**（`tools/dev/evidence/`），已在 `.gitignore` 里忽略，避免污染版本库；可随时整个删掉重来。
- 每次运行一个新时间戳目录，**不会覆盖**上一次的证据（便于对比「注入前后」）。

## `-SelfTest` 注入的是**真实失败**，不是假桩

注入方式：把关键 PG 用例的 `MIR2_TEST_DATABASE_URL` 改成一个**不存在的库名**
（`mir2_check_routine_missing_<时间戳>`），其它步骤照常跑。断言四条：

1. 巡检进程 **`exit == 1`**（不是 2、不是崩溃码）；
2. `summary.md` 里**只有被注入那一步 FAIL**，其它步骤 PASS；
3. 该步日志包含数据库返回的**原始错误文本**（`database "..." does not exist`）；
4. 紧接着**再跑一次正常 `-Quick` 必须 `exit 0`**（证明巡检不是恒失败）。

四条都成立时打印 `[SELFTEST] PASS`（仍以 1 退出，因为被注入那次本来就是红的）；任一条不成立打印
`[SELFTEST] FAIL` 并在 summary 里列出不成立的断言。

## 与 `ci.ps1` 的关系

- **不并入** `ci.ps1` 的 10 项门禁：这三步里「无素材沙箱」要另建 target 目录跑一遍全套二进制（分钟级），
  迁移演练还要真 PG，塞进去会让每次收口都变慢。
- 关系是**互补**：`ci.ps1` 快而全（每次收口），`check-routine` 慢而深（发版前）。

## 无人值守

脚本里 **0 处 `Read-Host`/交互**：PG 连接串从 `.env` 的 `DATABASE_URL` 推导（换库名到 `mir2_test`），
迁移演练库从 `MIR2_DRILL_DATABASE_URL` 读。适合挂到定时任务/看板里。

## 已知边界

- `-Quick` 跳过的只有**迁移演练**；无素材沙箱与 PG 用例仍会跑（这两步是日常最容易挡回归的）。
- 迁移演练要求 `MIR2_DRILL_DATABASE_URL` **显式设置**且库名以 `mir2_drill` 开头（见
  [`../store/schema.md`](../store/schema.md) §12）；缺它时 `-Full` 直接 `exit 2`，不会「悄悄跳过」。
- 无素材沙箱会自己建 `target/ci-sim` 并编译一遍（首次约 2–4 分钟）；磁盘占用随 `cargo` 正常增长。
