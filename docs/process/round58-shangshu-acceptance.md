# R58 尚书省验收报告（JJC-20261026-005）

- 验收官：shangshu（gpt-6.1-sol）｜2026-10-26T16:15+08:00｜只读验收：仅写本文件与 `target/round58/shangshu-acceptance/*`
- 验收对象：HEAD=origin/main=`2f3ab9f`（`2f3ab9f98f53059e3a60deb825047f1b354bc7c7`）；主单状态 Doing；边界父提交 `1a9aeb3`

## §1 输入事实（真读复算）

- 三提交：`f119a20`（5 文件 +1174/−7，功能批）→`d404ed1`（7 文件 +399/−2，文档批）→`2f3ab9f`（1 文件 +14/−7，M-1/M-2）。
- 门禁 attempt-1：head `d404ed1`，201,795 B sha `114891f01851…`，exit0，末行「==> 全部通过（12 项）」｜`target/round58/gate/attempt-1.log`（meta bytes/sha 与实测 LOG_MATCH）。
- 门禁 attempt-2：head `2f3ab9f`，201,813 B sha `faed4916c019…`，exit0，末行「==> 全部通过（12 项）」；start 08:03:34Z/end 08:05:38Z/124s。
- T 终审（S5）：**PASS-WITH-PARTIAL**，2 必改 MINOR（M-1/M-2）已由 `2f3ab9f` 闭合；产物 final-manifest.json 26,063 B sha `740e781f…`、round58-final-audit.md 17,292 B sha `15c47128…`、audit/summary.log 13,599 B sha `4e06e2f4…`。
- 两次门禁原因：attempt-1 覆盖功能+文档批（d404ed1）；T 终审判 M-1/M-2 文档必改→`2f3ab9f` 修正后重跑 attempt-2，使门禁绑定最终树，属必要重跑而非重复劳动。

## §2 A–H 核验表

| 项 | 结论 | 证据（文件:行/sha/exit） |
| --- | --- | --- |
| A | **PASS** | 两 log 字节/sha 与各自 meta 逐项相等；`^OK` 13 行=12 汇总项+1 前置项「所有保留物品模板都有图标」，末行汇总一致；exit0；仅 attempt-1/2 无多余 attempt；head 分别绑定 d404ed1/2f3ab9f；FAILED/error/panicked=0；R58 测试名各出现 2 次。见 `02-gate-verify.log` |
| B | **PASS** | f119a20∩d404ed1=∅；d404ed1∩2f3ab9f 仅 `docs/testing/round58/README.md`（M-1/M-2 定向修正）；无 `target/`、`.env`、44/45MB 冻结 JSON 入库；三提交全部 blob <1 MiB（逐 blobs 检查无 BIG）。见 `01-input-sha.log`+`git show --stat` |
| C | **PASS（记账注）** | `02-tree-hashes-at-head.txt` 13 行=12 路径+README 终版；逐条 `git show 2f3ab9f:<path>\|sha256sum`：12/13 严格 MATCH；唯一不匹配行是 README 修正前哈希 `b198e1e4…`，与 `d404ed1:<path>` 完全一致，由末行终版 `550376962a…`（MATCH HEAD）取代，属有意 supersede。见 `03-candidates.log` |
| D | **PASS** | M-1：`grep -c 在途 docs/testing/round58/README.md`=**0**（5 处改「已落盘」，2f3ab9f +14/−7）；M-2：README:91 §4.9 实存，`EFFECT_EVENT_LIMIT: usize = 16` 在 `net.rs:166`、`remove(0)` 在 `net.rs:7257-7259`、含「R59 消费方须接受可能丢最旧/届时评估扩容」约定与测试锚点。见 `04-doc-closure.log` |
| E | **PASS** | S1 `round58_rng_order` 3 passed exit0，M4 红→绿 exit101/0（`s1/attempt-1/10-meta.txt`）；S2 `round58_projectile_wire` 7 passed exit0，仅 FireBounce 白名单、unknown source 丢弃不 panic、无渲染（`net.rs` 新注释明示渲染延后 R59）；S3 `run-checks.sh` 9/9 PASS FAIL=0 SKIP=0，server/client roundtrip 各 `1 passed; 177 filtered out`。 |
| F | **PASS** | T 审计 §四 P1–P7 七项逐条诚实登记：P1 渲染→R59、P2 Hero、P3 默认 DB 学习/掉落、P4 `:1536` 偏差、P5 125 不回显、P6 次级随机序、P7 ring 语义；新增行无「125 全链/R58 完整」正向宣称（4 处「全链」均为禁令语境）。见 `04-doc-closure.log` |
| G | **PASS** | 本批新增 `#[allow]`=0（net.rs 3 处为存量）、`#[ignore]`=0、删除断言=0、新增测试非 ASCII=0；无手改 DB；SKIP 未冒充 PASS（SKIP=0，防护自测显式标注）；门禁两 log FAILED/error=0；i18n 三闸（TextKey/两条硬编码棘轮）均 OK。 |
| H | **PARTIAL** | `README.md:3` 仍写 HEAD `1a9aeb3`（T 观察项，非阻断）；roadmap 无 R58 条目=无矛盾；magic.md:687/774/791-796 已同步 125 口径与 `:1536` 偏差。另见 O-1：T 产物 final-manifest/final-audit 仍 untracked，monster-native 44.8MB/45.2MB JSON untracked（未入库，但有误提交风险）。 |

## §3 必改闭合核对

- **M-1 闭合**：README 五处「在途（S1/S3）」已改「已落盘」；`grep -c 在途`=0；`已落盘` 计数 20；不动代码、不改判定。
- **M-2 闭合**：新增 §4.9（README:91）含三要素——`EFFECT_EVENT_LIMIT=16`（net.rs:166）、超限 `remove(0)` 丢最旧（net.rs:7257-7259）、R59 消费方须接受或评估扩容；并挂 `projectile_ring_reuses_measured_effect_event_limit` 与 wire 测试 `:250-270` 证据锚点。均与源码实读一致。

## §4 R59 承接清单（8 项）

1. 客户端 125 渲染基座：Missile 410 实体/飞行插值/帧动画（P1，R58 只解码登记）。
2. 命中表现 Effect 570(`10,600`)（P1）。
3. 音效 `20000+GreatFireBall*10+1/+2`（P1）。
4. Hero 实例与候选等价（P2；当前 Hero 无实例，仅注释保留分支）。
5. 默认 DB FireBounce 学习书/掉落生产路径（P3，A0 仅证 MagicInfo 存在）。
6. `:1536` 攻城期 `can_fly` 偏差修复，动前自带「攻城期阻挡格 FireBall 目标判定」回归（P4）。
7. 事件 ring 语义消费/扩容评估（P7；16 条环形、丢最旧、需接受高频丢事件）。
8. `README.md:3` HEAD `1a9aeb3` 滞留更新（H）；并收口 O-1：T 产物入库、44/45MB 未跟踪 JSON 清理或忽略。

## §5 结论

**ACCEPT-WITH-PARTIAL**：A–G 全 PASS，H PARTIAL（README:3 滞留，非阻断）；M-1/M-2 已闭合。可进场下确认，条件为 R59 承接上述 8 项。

**不得对外宣称「125 全链」「R58 完整」**：客户端 125 仅解码登记、渲染/命中/音效全为 PARTIAL；服务端 125 生产已实现不等于全链贯通。

CLI 回执（详见 `05-cli.log`）：`memo` exit0；`done JJC-20261026-005 --as shangshu` **exit3**——board 子任务 #5「待 Lead 复核」未完成（4/5），readiness 拦截，未绕过；`show` exit0，state **Doing**（进度 4/5）；`validate --as shangshu` exit0（161 条符合 schema v3.1）。

## §6 证据复算命令与结果

- `sha256sum target/round58/gate/attempt-{1,2}.log` → `114891f0…`/`faed4916…`，bytes=201795/201813，与 meta LOG_MATCH。
- `git show 2f3ab9f:<path> | sha256sum` × 13 → 12 MATCH + README 终版 `550376962a…` MATCH；旧行 `b198e1e4…`=d404ed1。
- `grep -c 在途 docs/testing/round58/README.md` → 0；`sed -n '166p;7257,7259p' crates/mir-client/src/net.rs` → 16/remove(0)。
- S1 `10-meta.txt` exit 101（M4 红）→0（绿，3 passed）；S3 `02-run.log` 9/9 PASS、roundtrip 各 1 passed/177 filtered。
- 证据目录：`target/round58/shangshu-acceptance/{01-input-sha,02-gate-verify,03-candidates,04-doc-closure,05-cli}.log`。
