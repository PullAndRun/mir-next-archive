# R58 门下省收口确认（menxia，gpt-6.1-sol，只读复核）

## §1 输入与 HEAD

- 主单 `JJC-20261026-005` 状态 **Review**，5/5 todos，updatedAt `2026-10-26T08:18:35Z`（`show` 实读）。
- 交付 HEAD = `origin/main` = `2f3ab9f98f53059e3a60deb825047f1b354bc7c7`；链 `f119a20`(功能批5文件)→`d404ed1`(文档批7文件)→`2f3ab9f`(M-1/M-2 README +14/−7)；边界父 `1a9aeb3`。
- 被审对象：T 终审以 `d404ed1` 为对象（15:49–15:53），M-1/M-2 由 `2f3ab9f` 闭合，attempt-2 门禁 head=`2f3ab9f`；**交付 HEAD 与门禁最终 head 一致**，审计/验收均为门禁后产物（允许口径）。
- 输入（真读）：`round58-plan.md` 141 行、`round58-menxia-review.md` 64 行、`round58-shangshu-dispatch.md` 80 行、`round58-shangshu-acceptance.md` 57 行/6,791 B sha `1cf2342d3d14…`、T `round58-final-audit.md` 17,292 B sha `15c47128…`、`final-manifest.json` 26,063 B sha `740e781f…`、`docs/testing/round58/README.md`。

## §2 门禁 / 提交 / 候选核验表

| 核验项 | 实测（真读/真复算） | 判定 |
|---|---|---|
| attempt-1 | 201,795 B；sha `114891f01851107bfd2b5c51c705088fd50f13221c6905b6b7cffecb3ab7e7a1`；meta head `d404ed1`、exit0、07:45:16Z→07:48:43Z/207s | ✓ 一致 |
| attempt-2 | 201,813 B；sha `faed4916c019cd15123d2966a1e6b2c279d29143cbc39ea08ea5e643749a10bb`；meta head `2f3ab9f`、exit0、08:03:34Z→08:05:38Z/124s | ✓ 一致 |
| 12 项 + 末行 | 两 log 汇总段各 12 行 `OK`，末行均「==> 全部通过（12 项）」；另有 1 条前置图标子步骤 OK 行，不入 12 项计数 | ✓ |
| 假绿扫描 | `FAILED|panicked|^error` 两 log 命中 **0**；`log_bytes/log_sha256` 与实测全等（LOG_MATCH） | ✓ 无假绿 |
| 两次门禁理由 | attempt-1 覆盖功能+文档批（`d404ed1`）；T 判 M-1/M-2 文档必改→`2f3ab9f` 修正后 attempt-2 重跑，绑定最终树 | ✓ 必要重跑 |
| 提交边界 | f1 5 文件（`net.rs`/`round58_projectile_wire.rs`/`magic.rs`/`round58_rng_order.rs`/`run-checks.sh`）；d4 7 文件（`world.rs` 仅 1 行 `dvance_clock`→`advance_clock` 注释错字）；2f 仅 README | ✓ 无跨批污染 |
| 交集 | f1∩d4=∅；d4∩2f=`docs/testing/round58/README.md`（M-1/M-2 顺序精修，2f 未回改实现） | ✓ 非污染 |
| 工作树 | 仅 `??` acceptance + `final-manifest.json` + `round58-final-audit.md` + 2 个历史 freeze JSON；`git diff --quiet -- crates` exit **0** | ✓ 符合预期 |
| 候选/产物哈希 | acceptance 57 行 6,791 B `1cf2342d…`；manifest 26,063 B `740e781f…`；audit 17,292 B `15c47128…` 均与派发所载一致 | ✓ |

## §3 M-1 / M-2 闭合

- **M-1**：`grep -c 在途 docs/testing/round58/README.md` = **0**；5 处「在途（S1/S3）」→「已落盘」；状态列允许值移除「在途」并注明原因（2f3ab9f +14/−7 实读 diff）。
- **M-2**：`README.md:91` §4.9 实存；实读源码 `crates/mir-client/src/net.rs:166` 为 `const EFFECT_EVENT_LIMIT: usize = 16;`、`net.rs:7257-7259` 为 `if snapshot.projectile_events.len() > EFFECT_EVENT_LIMIT { snapshot.projectile_events.remove(0); }`；台账含「R59 须接受可能丢最旧/届时评估扩容」与测试锚点 `round58_projectile_wire.rs:250-270`，与源码逐字一致。
- **如实保留**：`README.md:3` 仍写 HEAD `1a9aeb3`（H PARTIAL，非阻断，R59 #8 承接）。

## §4 PARTIAL 与 R59 承接 8 项

- **P1–P7 台账齐**（T 审计 §四 7 行）：P1 客户端渲染 PARTIAL→R59、P2 Hero 无实例、P3 默认 DB 学习/掉落未证、P4 `:1536` 偏差 R59、P5 125 不回显、P6 次级随机序 MINOR、P7 ring 语义（M-2 已入台账）。
- **口径诚实**：新增行「125 全链/125全链路/R58 完整」4 命中**全为禁令语境**（「禁写」「不写」），「A2 已通」2 命中同为禁令；**无正向宣称**。
- **复核 T/尚书不越权**：新增 `#[allow]`/`#[ignore]` = **0**；新测试非 ASCII = **0**；删除 `assert` = **0**；门禁无 FAILED/error（见 §2）。
- **D1 裁决 vs 实施**：C1 `magic.rs` 先抽下一目标（`fire_bounce_next_target`，空候选不抽签）再扣 `bounce`，M4 红 exit**101** 1 failed→绿 exit**0** 3 passed；C2 新增行渲染类 `Sprite|Texture|draw|render` = **0**，仅 `ProjectileEvent` 解码登记；C3 `can_fly` 未合并、`:1536` 单列 R59；C4 GBK 原件 + `attempt-1.utf8.log` 成对（byte-identical，`encode_pair.py`/`encoding.log`）；C5 `docs/testing/round58/run-checks.sh` 在树 9,835 B。
- **R59 承接 8 项**（尚书 §4，与 T 一致）：①125 渲染基座 Missile 410 插值/帧动画 ②命中 Effect 570(`10,600`) ③音效 `20000+GreatFireBall*10+1/+2` ④Hero 实例与候选等价 ⑤默认 DB 学习书/掉落生产路径 ⑥`:1536` 攻城期 `can_fly` 修复（自带回归用例）⑦事件 ring 消费/扩容评估 ⑧`README.md:3` HEAD 更新并收口 O-1。
- **非阻断观察**：尚书验收未逐字复写 M5 同文（README:3 行、T 审计各 1 次命中，acceptance 0 次命中），但 R59 #1–3 与「PARTIAL」口径等价，不构成阻断。

## §5 结论

**PASS-WITH-PARTIAL 收口成立**：两门禁 exit0 12/12 与 meta/sha 全等；三提交边界清晰、工作树符合预期；M-1/M-2 已由 `2f3ab9f` 实闭；P1–P7 台账齐、无假绿、无越权改动；R59 承接 8 项与 T/尚书一致。**非阻断**：README:3 HEAD 滞留（H）。**禁称**「125 全链」「R58 完整」「A2 已通」。

## §6 CLI 回执（`node "C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs"`，逐条原始 exit/stdout）

1. `memo … menxia "R58 收口确认：PASS-WITH-PARTIAL，M-1/M-2 闭合，R59 承接 8 项，禁称125全链"` → **exit 0**｜`📝 JJC-20261026-005 任务记忆：menxia`
2. `state JJC-20261026-005 Done --as menxia` → **exit 0**｜`⏳ JJC-20261026-005 高风险操作 Review→Done，已进入 PendingConfirm，需 门下省 执行 confirm approve`
3. `show JJC-20261026-005` → **exit 0**｜`状态 PendingConfirm`、`当前 待确认: Review→Done`、`进度 5/5`、`⏳ 待确认：menxia 申请 Review → Done，需 门下省 批准`（pending_confirm 存在）
4. `confirm JJC-20261026-005 approve "PASS-WITH-PARTIAL收口成立" --as menxia` → **exit 0**｜`✅ JJC-20261026-005 confirm approve → Done`
5. `show JJC-20261026-005` → **exit 0**｜`状态 Done 归属 完成`、`当前 PASS-WITH-PARTIAL收口成立`、流转新增 `2026-10-26T08:21:14Z PendingConfirm → Done ✅ 批准: PASS-WITH-PARTIAL收口成立`
6. `validate --as menxia` → **exit 0**｜`✅ 看板校验通过：161 条任务符合 schema v3.1`

六步全 exit 0，无绕过、未直改 JSON。
