# 第49轮尚书省派发说明

- 派发角色：尚书省（尚书令，`shangshu`）。
- 日期：2026-10-25，时区 Asia/Shanghai。
- 治理父单：`JJC-20261025-001`，已由 `Assigned` 转为 `Doing`。
- 正式实现主线：`JJC-20261024-004`，继续 `Doing`；既有工部子单 `JJC-20261024-004-sub-f9748d4c` 继续执行，本次没有修改其状态。
- 依据：[门下最终评审](round49-menxia-review.md)的 PASS-WITH-CHANGES 优先于[中书原方案](round49-plan.md)中被修订的双主线、商城和固定“12/12”口径。本文件承接重规划后的拆分契约，不改写历史方案或旧日志。

## 一、范围决定与只读事实

004 仅收口现有怪物原生帧表、走/跑插值及对应真实素材证据；001 只负责冻结、审计、文档/i18n 一致性和拆分治理。004 已有的工部实现线继续使用，不登记第二条怪物实现线。独立商城原 R49-03/R49-04 整体移至 R50，本轮没有建立商城实现子单。

001 的历史标题仍由 CLI 保留。当前 CLI 没有通用标题编辑命令，尚书不手改看板 JSON；本轮实际范围由门下评审、001 的治理 memo、Doing 说明及本派发说明共同限定。尚书无父任务 `create` 权限，R50 关联事项已用 001 memo 登记，后续交太子/中书走建单和审议流程。

按要求依次读取角色职责、典章、门下评审、163 行原方案后，已运行三个指定 `show`、`list --all`、`git status --porcelain` 和 `git diff --stat`，均退出 0。当前只读采样结果为：

- `git diff --stat`：`46 files changed, 1951 insertions(+), 230 deletions(-)`。原方案的 45 个文件是历史快照；Lead 新增 CLI 帧表装载涉及 `assets.rs` 等，R49-01 必须清点实际全部文件，解释 45→实际值的差异，不能固定 45 或漏掉新增项。
- 未跟踪项包含原生帧表模块、真实素材测试、CLI 装载单点测试、两份流程文档和多份证据目录；目录条目不等于逐文件清单，R49-01 必须展开。
- Git 提示部分文本将在下次 Git 写入时由 LF 转为 CRLF；冻结必须同时区分工作树原始字节 SHA256、Git 规范化 blob SHA 和行数，不能把两种哈希混用。
- Lead 已补的 CLI 装载、`load_monster_frames`、ignored 单点测试和缺素材 assert 修复均属于现有 WIP；本次只登记验收，不改实现。
- 本次尚书未运行 Rust 测试、i18n、链接检查或全量门禁，未声明 R49 通过，未执行 `git add` / `git commit`。

## 二、三个子单与执行顺序

| 轮次项 | 真实 SUB_TASK_ID | 承接角色 / 模型 | 依赖与启动条件 |
|---|---|---|---|
| R49-01 WIP 冻结事实 | `JJC-20261025-001-sub-aa73c714` | 礼部 `libu` / `gpt-6.1-sol` | 无新增治理任务前置；队长先协调 004 现有写入者完成稳定快照交接；先于 R49-05 写入 |
| R49-05 文档与 i18n 一致性收口 | `JJC-20261025-001-sub-fd937482` | 礼部 `libu` / `gpt-6.1-sol` | R49-01 已交付并释放共享目录，且 004 的真实素材证据稳定后启动 |
| R49-06 最终只读终审 | `JJC-20261025-001-sub-6282c186` | 刑部 `xingbu` / `gpt-6.1-sol` | R49-01、R49-05、004 ignored 证据全部齐备，且队长唯一执行最终全量门禁后启动 |

三个子单由 CLI 默认登记为 `Doing`，这只表示委派已建立，不能据此跳过依赖启动。当前尚书会话的子代理深度已用尽，三个执行者尚未在本会话启动；子单 ID ↔ agent ID 映射应由队长启动执行者时补记。这里只做已授权的登记和完整启动契约交接，不把未启动写成完成。

建议顺序：R49-01 → 004 既有实现/证据交接完成 → R49-05 → 队长最终完整门禁 → R49-06。R49-01/05 都写 `docs/testing/round49/manifest.json`，必须串行；R49-05 与工部涉及的 README/元数据也必须交接写入权。冻结只读核对期间可以与不写相同文件的只读审阅并行；采样期间文件变化必须重采，不能接受混合时刻快照。

## 三、共同执行契约

看板唯一入口为：

```text
node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs <命令> --as <真实角色>
```

角色简报分别为 `C:/Users/diana/.dsh/skills/edict/references/roles/libu.md` 与 `C:/Users/diana/.dsh/skills/edict/references/roles/xingbu.md`。每名承接者启动后先 `recall <父ID>`、`recall <自身子ID>`、`progress <自身子ID> ...`，再维护自身 todo；每个子单至多一个 `in-progress`。命令中礼部用 `--as libu`、刑部用 `--as xingbu`，不冒用协调角色。

每项交付必须同时提供命令原文、工作目录、起止时间、原始退出码、实际计数、工作树指纹、证据路径及 SHA256、风险/未决项。验证命令非 0、缺素材、缺 PNG、缺 ignored 真实证据、哈希不符、范围越界一律记录 `PARTIAL`。资料读取/统计的成功不代表功能通过；不能固定“12/12”，不能用 `SKIP`、缺素材 `return Ok`、旧日志或改数字制造通过。

三名执行者均不得改 `crates/**`、测试实现、数据库或提交内容，不得 `git add` / `git commit`；旧日志、旧 PNG/CSV 和历史审计原文必须保全。发现实现缺陷交回队长及 004 既有工部线处理。全量门禁只由队长运行，礼部/刑部不得运行 `ci.ps1` 或 `-KillStale`。

验收不允许 GM/改 DB、静态商城 fixture、抓包改包、伪造库存、跨动作借帧、硬编码坐标、吞掉事务失败、强杀冒充正常退出，亦不得把现有市场页签当独立商城真实包链。

## 四、R49-01：WIP 冻结事实

**允许写入：**仅 `docs/testing/round49/wip-freeze.json` 和 `docs/testing/round49/manifest.json`。两者各自不得超过 1 MiB（1,048,576 字节）；同目录已有审计探针及报告只读保全。不得改原方案、代码、其他证据目录或旧日志。

执行命令至少包括：

```text
git rev-parse HEAD
git status --porcelain
git diff --stat
git diff --check
git ls-files --modified
git ls-files --others --exclude-standard
```

对每个实际 M 文件及每个展开的未跟踪文件记录路径、Git 状态、字节数、类型、Git blob SHA（区分 HEAD/index/worktree）、原始字节 SHA256 和文本行数；二进制 PNG 等的行数明确记为“不适用”，不得把二进制按文本解码。逐文件命令示例：

```text
git ls-files --stage -- <文件>
git hash-object --path <文件> <文件>
Get-FileHash -LiteralPath <文件> -Algorithm SHA256
(Get-Item -LiteralPath <文件>).Length
(Get-Content -LiteralPath <文本文件> | Measure-Object -Line).Lines
```

最后一条必须注明所用行数口径；空行也应被准确覆盖，必要时使用按原始字节换行符统计的等价实现。没有执行外部程序的统计记录操作是否成功及错误信息，外部程序记录原始进程退出码。不要对超大文件无界读取，也不要读取或输出凭据值。

四个证据目录必须逐文件列出 SHA256、大小和来源轮次：`docs/testing/monster-native/`、`docs/testing/monster-motion/`、`docs/testing/native-monster/`、`docs/testing/attack-variants/`。记录相对 HEAD 的真实变化，不修改 R47/R48 原始日志来消除差异。

交付清点结论必须区分“可进入后续待审提交清单”和“必须排除”。重点核实 `docs/testing/monster-native/player-resource-freeze*.json` 约 44/45 MB 的大 JSON；超过单文件 1 MiB 上限的内容仅以路径、字节数、SHA256 和本地可复核引用登记，不复制入新的入库 JSON、不提交、不删除原文件。可提交只代表满足清单/大小条件，仍须最终门禁、尚书验收和门下复核。

`manifest.json` 必须区分历史门禁和当前待运行门禁，登记真实日志、退出码来源、通过数/总数及采样工作树。若当前最终门禁尚未运行，写“待队长最终运行 / PARTIAL”，不能沿用旧日志的绿色结论。冻结开始/结束的状态、文件大小和哈希应一致；出现并发变化必须协调交接后重采。

验收：两份 JSON 能解析、各≤1 MiB；当前全部实际 M/未跟踪项及四目录均有覆盖；45→实际数差异、旧日志时效、超限排除项和回退边界可追溯；命令退出码和证据路径齐备。完成冻结并不表示 R49 已通过。

## 五、R49-05：文档与 i18n 一致性收口

**允许写入：**[客户端文档](../client.md)、[路线图](../roadmap.md)、[商城说明](../world/gameshop.md)、必要的 [i18n 文档](../i18n.md)，本轮新增 `docs/testing/round49/README.md`，以及 `docs/testing/{monster-native,monster-motion,native-monster}/README.md`、`docs/testing/monster-native/freeze.json`、`docs/testing/round49/i18n.log` 和 `docs/testing/round49/manifest.json`。README/freeze 元数据须先与 004 证据写入者交接；不得改 `wip-freeze.json`、审计报告、旧日志、PNG/CSV 或代码。

先核对客户端 §6/§8、路线图和商城说明：仅有本轮真实证据支撑的怪物原生帧/走跑 WIP 可以写“已收口”；精确 `!Mon.dat` 映射、坐骑与 Push/Jump/Dash 保持未完成，独立商城明确“未完成，移至 R50”。R49 不能提前计入未来商城实现或 i18n 成果；新增玩家可见文字依旧必须使用 `TextKey`/`Label` 参数化 API。

对本轮证据 README 和原生怪物 `freeze.json`，按实际产物核对 batch、client_binary 哈希及 attack-variants“0 diff”过期断言。只能修订结论与引用，原始 R47/R48 日志/图片保留；存在变化就如实记录差异与 `PARTIAL`，不能更新哈希后仍声称与旧版本完全相同。

必须逐项执行并保存到新 `i18n.log`，每项独立记录原始退出码和摘要：

```text
py tools/i18n/scan_hardcoded.py --player-visible --strict --fail-above 70
py tools/i18n/scan_hardcoded.py --player-visible --no-test --strict
py tools/docs/check_links.py
git diff --check
git status --porcelain
```

对上述两条 i18n 棘轮准确记录计数，并解释历史 `189/22/69/15` 基线及 no-test strict 的 15 处开发者/数据文案边界；不能把“玩家可见归零”扩大为所有文本归零。链接检查若因缺 PNG 失败，反馈真实缺口，不补造图片、不抹掉失败日志、不写成功。

交付包括更新后的文档/README/元数据、`i18n.log` 和继承 R49-01 冻结记录的 `manifest.json`。manifest 必须保留初始冻结，另列本阶段允许变化后的候选文件/证据哈希和原因；不把正常串行文档变化静默覆盖成初始状态。全部移交队长后才进入最终全量门禁。

验收：四条检查有真实退出码/计数/路径，已做与未做口径一致，过期断言得到可复核解释；失败或缺证据仍 `PARTIAL`，不跨范围修实现。

## 六、R49-06：队长最终门禁后的只读终审

**允许写入：**`target/round49/**` 的新终审产物、`docs/testing/round49/final-manifest.json`、`docs/testing/round49/round49-xingbu-final-audit.md`。其他文件全部只读，包括初始冻结、普通 manifest、README 和旧日志。不得自行运行全量门禁或补跑 ignored 测试。

由队长在 004 ignored 证据与 R49-05 文档稳定后，唯一运行：

```text
pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale
```

刑部必须取得本次原始输出、原始进程退出码、起止时间和所用候选工作树；不能把 `Tee` 等管道末端的成功码当门禁进程退出码。不得与其他 ignored/soak/全量门禁并行 `-KillStale`。

尚书已只读检查[全量门禁入口](../../tools/dev/ci.ps1)：当前脚本将实际 Name/Code/Seconds 汇总后输出总数，并不原生生成 gate JSON manifest。因此刑部应根据队长本次日志和退出码构建 `final-manifest.json`，保存实际逐项结果、通过数/总数及原始日志 SHA256；不能依赖不存在的 gate manifest，更不能固定 12/12 或 13/13。历史 `target/gate-r49-d.log` 仅可引用为过期历史。

刑部只运行只读核对命令：

```text
git status --porcelain
git diff --stat
git diff --check
git ls-files --modified
git ls-files --others --exclude-standard
git hash-object --path <文件> <文件>
Get-FileHash -LiteralPath <证据文件> -Algorithm SHA256
```

再核对门下评审 §五规定的全部命令是否已有本次真实输出和原始退出码。以下为读取证据清单，不是让刑部再运行：

```text
cargo test -p mir-asset --test real_framesets
cargo test -p mir-render --test monster_native -- --ignored --nocapture
cargo test -p mir-render --test monster_native_evidence -- --ignored --nocapture
cargo test -p mir-client --lib camera::monster_motion_tests
cargo test -p mir-client --test monster_motion -- --ignored --nocapture
cargo test -p mir-tools --test monster_native_import -- --ignored --nocapture
cargo test -p mir-client --test cli_monster_frames -- --ignored --nocapture
py tools/i18n/scan_hardcoded.py --player-visible --strict --fail-above 70
py tools/i18n/scan_hardcoded.py --player-visible --no-test --strict
py tools/docs/check_links.py
git diff --check
pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale
```

除门下原清单外，必须复核 Lead 新增 CLI 原生帧表装载单点 ignored 测试真实执行证据。必须确认 `self_moving.png`、`landed.png` 及其他必需 PNG 存在、哈希与清单一致；真实素材测试不能以缺素材早退视为通过。复核负 Skip/stride=0、Count×Interval 生命周期、每怪帧表接入、OffSetMove 符号及缺库/有表缺动作的回退说明没有被证据包装掩盖。

最终交付：

- `target/round49/final-gate.log`：本次门禁证据摘要及原始日志引用；若队长已在该路径保存原始输出，则原样保留，摘要另存 `target/round49/final-gate-summary.log`，在最终 manifest 中同时登记两者。
- `docs/testing/round49/final-manifest.json`：最终候选工作树、由 R49-01/R49-05 合法演进而来的哈希、每条命令退出码、门禁实际总数/通过数、证据哈希、待审提交清单和必须排除清单。
- `docs/testing/round49/round49-xingbu-final-audit.md`：只读终审结论、逐条证据、PARTIAL/回退边界、捷径黑名单核对、未决项及交回建议。

新增终审文件本身应单列为终审产物，不能把写出审计报告误判为之前冻结的生产代码发生变化。反之，任何未被允许演进覆盖的源文件/证据变化、门禁失败、缺 PNG/ignored 输出、哈希不符、拟入库超 1 MiB 大文件均必须 `PARTIAL`，交队长安排恢复证据链，不由刑部代修。R49 通过仍需尚书验收及门下独立复核，刑部完成审阅不等于允许提交。

## 七、R50 商城移出事项与前置条件

001 的 R50 memo 已明确登记原 R49-03/04 为移出项，当前不建立实现子单。后续 R50 至少先满足：

1. 资产 manifest 列明路径、来源、SHA256、授权/可用范围、目标尺寸和稳定缺图占位；来源未确认的社区/仿制素材不得冒充 Crystal 原作，缺图截图和报告保留 `PARTIAL`。
2. 冻结真实 GameData/DB 计数和真实 `S.GameShopInfo` / `S.GameShopStock` 包链。登录每个可用 GIndex 只发一次目录信息，个人/全服与零库存按 Crystal 处理，客户端按 GIndex 幂等消费迟到/重复包。
3. 事务成功提交后按条件刷新库存；库存不足按 Crystal 负例刷新；DB、邮件、角色落盘失败不得发送虚假库存，内存扣款/发货必须回滚，并保存独立的失败包序证据。

沿用 `lock_game_shop`、`gameshop_buy`、`flush_gameshop_update`、`rollback_gameshop_purchase` 边界。已有协议结构在线格式无差异时不重写；服务端/客户端商城边界重新审议，未来客户端实现与 004 的 `mir-client` 文件串行交接。

## 八、看板操作与最终 show 摘要

本轮尚书所有看板读写均带 `--as shangshu`，所有已执行 CLI 操作退出 0：

1. 004 `memo`：记录主线撞车处置、保留既有工部线、001 转治理、商城移 R50，引用门下评审。
2. 001 `memo`：以 R49 冻结/审计/拆分治理限定实际执行范围，并记下当前 46 文件的事实差异和串行依赖。
3. 001 `state Doing`：说明为“门下评审通过，进入 R49 冻结/审计/治理执行”；实际返回 `Assigned → Doing`。
4. 001 三次 `delegate ... shangshu libu/libu/xingbu ...`：分别返回本说明所列的三个真实子单 ID。
5. 001 `memo`：登记 R50 移出项、素材授权/哈希 manifest、真实包链及事务失败包序前置条件，并记录尚书不具父任务 create 权限。
6. 各项 `show` 和 `validate`：实际核对如下；文件落盘后通过 001 的 `progress` / `flow` 回交派发说明及启动依赖，不代任何执行者运行 `delegate-result`。

| 看板对象 | 实测状态 | owner / 进度 | 核对结论 |
|---|---|---|---|
| `JJC-20261024-004` | Doing | `main` / 3/4 | 正式实现主线保留；仅新增撞车 memo |
| `JJC-20261024-004-sub-f9748d4c` | Doing | `gongbu` / 0/0 | 既有工部线未改状态，继续执行 |
| `JJC-20261025-001` | Doing | `taizi` / 0/0 | 当前说明已转冻结/审计/治理执行；owner 字段为原收旨责任，本次未冒名重写 |
| `JJC-20261025-001-sub-aa73c714` | Doing | `libu` / 0/0 | R49-01 登记成功，等待启动与稳定采样交接 |
| `JJC-20261025-001-sub-fd937482` | Doing | `libu` / 0/0 | R49-05 登记成功，依赖 R49-01/004 证据交接 |
| `JJC-20261025-001-sub-6282c186` | Doing | `xingbu` / 0/0 | R49-06 登记成功，依赖最终门禁完成 |
| `JJC-20261025-001-sub-03e00608` | Doing | `shangshu` / 0/0 | 门下已有的协调委派保留，不代执行层交结果 |

`validate --as shangshu` 实际返回：“看板校验通过：90 条任务符合 schema v3.1”，退出 0。该结果只证明看板字段/状态合法，不代表运行门禁、真实素材或功能验收已通过。

派发无看板或权限阻塞。执行阶段存在明确前置依赖：稳定冻结、004 证据收口、文档一致性和队长最终门禁；缺任一项维持 `PARTIAL`。本次不将 001 或三个子单推进 Review/Done，不提前完成尚未启动的执行工作。
