# R58 刑部终审报告（JJC-20261026-005）

- 审计对象：`HEAD = origin/main = d404ed1`（`d404ed11c612dae29293f84e4090eca24276c01a`），边界父提交 `1a9aeb3`（`1a9aeb316c288150b32bb19859fc35905c510272`）。
- 子单：`JJC-20261026-005-sub-fa652fa3`；审计角色：刑部（只读终审），模型 gpt-6.1-sol。
- 只读边界：未修改 `crates/` 任何文件，未 commit/push，未跑全量门禁或 ignored 测试；仅复跑单条只读测试、`check_links`、`bash -n`、`--list` 与 sha 校验。
- 审计时间：2026-10-26 15:49–15:53（Asia/Shanghai）。

## 一、结论

**PASS-WITH-PARTIAL**。功能批与文档批的真实性、提交树绑定、S1–S4 判据与“不宣称完整”口径均成立；客户端 125 渲染等按 C2/C3 为声明式 PARTIAL（R59 承接），不属本轮未完成。

必改项 **2 条（均 MINOR，文档/台账行级，不阻断功能判定）**：

| 编号 | 文件:行 | 问题 | 要求 |
| --- | --- | --- | --- |
| **M-1** | `docs/testing/round58/README.md:19`、`:28`、`:34`、`:76`、`:98` | S1/S3 在 `d404ed1` 前已落盘，README 状态列仍写「在途（S1）」「在途（S3）」，与实现/脚本/门禁结论不一致 | 5 处改为「已落盘」，或同处注明「截至 d404ed1 已落盘」；不改代码、不改判定 |
| **M-2** | `docs/testing/round58/README.md:78-82`（§4.6） | 事件 ring 语义（`EFFECT_EVENT_LIMIT=16`、超限 `remove(0)` 丢最旧、高频 FireBounce 可能一帧内丢事件）只在 `target/` 的 S2 证据摘要与测试注释中，未进入入库 PARTIAL 台账（G/B4 不完整） | 补 1 行容量/丢最旧/消费方须接受可能丢最旧（或届时评估扩容），并入 PARTIAL/已知限制表 |

**建议：可进尚书验收**。验收前由 Lead 一次性完成 M-1/M-2 两处文档修正（不触碰实现），验收时以 `final-manifest.json` 的 sha 与复现命令复核；对外口径禁止「125 全链」「R58 完整」。

## 二、A–H 必核结论

### A. 门禁真实性 —— PASS

- `target/round58/gate/attempt-1.log` = **201,795 B**，sha256 `114891f01851107bfd2b5c51c705088fd50f13221c6905b6b7cffecb3ab7e7a1`；`attempt-1.meta.txt` = 638 B，sha256 `1cd9c5689d1ccb0ee64f9ed83624183f41343cc0f302ecf3b3caebc3daad8686`，meta 内 `log_bytes/log_sha256` 与实测**完全一致**。
- meta：`attempt=1`、`head=d404ed1`、`branch=main`、start `07:45:16Z`、end `07:48:43Z`、`duration=207s`、`exit=0`、`capture=full(*>)`、`soak_lock=absent`、`residual=none`；命令为 `pwsh -NoProfile -Command "& ./tools/dev/ci.ps1 -KillStale *> target/round58/gate/attempt-1.log"`，与本轮 HEAD 对齐。
- 日志末行 `==> 全部通过（12 项）`；12 条 `OK` 汇总行。`^OK ` 共 13 行 = 12 汇总 + 1 条检查内文「OK 所有保留物品模板都有图标」；`^==> ` 共 13 行 = 12 检查横幅 + 1 汇总行，**不存在多算/少算**。
- 日志全文 0 条 `FAILED`、0 条 `error:`、0 条 `warning:`，`test result:` 行 148 个且均为 ok；含 R58 独有测试名：`round58_rng_order` 3 passed、`round58_projectile_wire` 7 passed。
- 仅 `attempt-1`，无 `attempt-2`，无旧日志复用。
- 双件/编码：源日志本身已是 UTF-8（CRLF，iconv 全文件校验通过），`attempt-1.utf8.log` 与原件**逐字节相同**（201,795 B / 同 sha），故无 GBK 原件需求；`encoding.log`（218 B，sha256 `fe9a8fdc9ed4d2d5a96fd0067e43ca0e133e6da5e7d92fe64bb3dc1719bde19f`）记 `raw_sha == utf8_sha`。其 `diff_chars=183432` 与同 sha 自相矛盾，见观察 O-1（不影响字节级结论）。

### B. 提交树绑定与卫生 —— PASS

- 对 12 条候选路径逐条执行 `git show d404ed1:<path> | sha256sum`，与 `target/round58/commit-verify/01-tree-hashes.txt`（1205 B，sha256 `dfbe6d84b8ccd3a1640a0a1baae11e0beb4cbd136e0bb5e39632d047358903f7`）比对：**12/12 MATCH**（清单见 manifest `candidate_files`）。
- `f119a20`（5 文件，+1174/−7）与 `d404ed1`（7 文件，+399/−2）变更路径集合**交集为空**；无 `target/`、无 `.env`、无 44/45MB 冻结 JSON、无 >1MiB 新增文件；`world.rs` 仅 `:1378` 注释差。
- 过程观察 O-5：派单建议 fix/feat/docs 三粒度，实际把 S1+S2+S3 合为 `f119a20` 单提交；提交说明与 diff 仍逐项可追溯，不影响内容判定。

### C. S1 语义（终局 RNG 改序） —— PASS

- `crates/mir-server/src/magic.rs` 现序与 Crystal `HumanObject.cs:5947-5956` 一致：先取候选（helper 候选空则 `return None`，**不抽**），非空才 `self.rng.next_u32() % candidates.len()`；随后 `remaining = bounce.saturating_sub(1)`，`remaining == 0` 直接 `return`——即“候选非空先抽 1 次 → 再扣 bounce → 终局抽中即废”。
- `round58_rng_order.rs` 的 `level=3` 用例断言命中数 `5 = level+2`、`chain.len() = 4 = level+1`，顺序 source→destination，与 Crystal 广播/排队次序一致。
- M4 可失败且红→绿：旧序红日志（1198 B，sha256 `8f2e97fa403a0ea08c5ba5ca7306cc85cb7c6c76b5ca791c1b5751ce76476125`）在 `round58_rng_order.rs:419:5` `assert_ne!` 失败，`test result: FAILED`、`0 passed; 1 failed`；`10-meta.txt`（1217 B，sha256 `b87c3313f808bacecd51ab21c093b24cd2de1c0421b95a7948d992b746862833`）记该条原始 **exit 101**。新序绿日志 595 B（sha256 `0d9818b9dba3a11f9ff0e204a4db87337d9dcacc8a96d71ea855a126c3845aa3`），3 passed。
- 既有用例零改动：`f119a20` 对本轮既有文件的唯一修改是 `magic.rs` 的 +13/−7 与新增两个测试文件；回归 magic 43、frost 7、round52 8、round57 6 全 0 failed。

### D. S2 语义（125 客户端接线） —— PASS

- `crates/mir-client/src/net.rs` 新增 `pub struct ProjectileEvent { pub spell: Spell, pub source: u32, pub destination: u32 }`、`WorldSnapshot.projectile_events`、`packet_name`；125 解码为 `ProjectileEvent`。
- FireBounce 白名单：仅 `projectile.spell == Spell::FireBounce && snapshot.object_position(projectile.source).is_some()` 才 `push_projectile_event`；source 未知**丢弃**、destination 未知**保留**、非 FireBounce 丢弃——三组均有测试与 Crystal `GameScene.cs:4691/4693` 对应。
- `push_projectile_event` 复用既有 `const EFFECT_EVENT_LIMIT: usize = 16`（提交后 `net.rs:166`），超限 `remove(0)` 丢最旧；无渲染、无资源、无音效实现（模块文档明确 rendering deferred R59）。
- M2 负例存在且绿：`non_firebounce_spell_is_dropped`；`round58_projectile_wire` 共 7 passed / 0 failed，含 wire 顺序断言。

### E. S3 脚本（run-checks.sh） —— PASS

- 入树脚本 `docs/testing/round58/run-checks.sh`：253 行 / 9835 B / sha256 `1ee97c251391539d24de063fcf6cf13f78794ccebb63f7829b2e59b9c6cf4fb3`；`bash -n` exit 0。
- 真实协议路径：`--list` 共 9 条，含 `packets::server::roundtrip::default_packets_roundtrip` 与 client 两条；`grep -c 'packets::tests'` = **0**（无 0 命中假绿）。
- 正式跑 PASS=9 / FAIL=0 / SKIP=0，`SCRIPT_EXIT=0`；逐条 `logs/*.log` 末尾原始 `EXIT=0`；协议两包各 `1 passed; 177 filtered`，证明用例真跑而非 0 tests 假绿。
- 防假绿自测：构造 `running 0 tests / 0 passed` 判 FAIL、用例文件缺失判 SKIP；注入自测 `PASS=7 FAIL=1 SKIP=1` 证明防护生效。
- Lead 独立复跑 `target/round58/lead-verify/run-checks.log`（1272 B，sha256 `9b1136a4a278295bd1e1c490bcc92ef7de4e21690a81ff5a22281d614585cab6`）：HEAD `1a9aeb3`、9/9 PASS、总 EXIT=0。

### F. S4 文档 —— PASS（行级口径见 O-4）

- M5 核心句「服务端 125 生产者已实现；客户端 125 仅解码登记，投射物渲染 PARTIAL 待 R59」在 `docs/world/magic.md:687`、`docs/testing/round57/README.md:51`、`docs/testing/round58/README.md:7` 各 `grep -F` = 1，**核心句逐字一致**；三行前缀不同、magic.md 在 R59 后追加「（R57 门禁/终审见 …）」，非整行逐字（O-4，按核心句口径满足）。`magic.md:687` 已去掉 ✅。
- 「125 全链」「R58 完整」宣称：新增行中「125 全链」共 4 处，全在「不写/禁」禁止性语境（plan §7.1、menxia M5、dispatch §三/§六）；「A2 已通」2 处同为禁令语境；「R58 完整」= 0。**无宣称性语句**（O-3 指出 S4 `03-redline.log` 的 `added_125_chain=0` 计数口径不准，但 H 项实质满足）。
- G4 十项承接表 `docs/testing/round58/README.md §1 ①–⑩` 可逐项追溯；§2 十二项与 `round58-plan.md §六` 对应。
- `py -3 tools/docs/check_links.py` 复跑 exit 0（1716 相对链接；控制台因代码页显示 mojibake，退出码为准）。
- `crates/mir-server/src/world.rs:1378` 确为注释差（`dvance_clock`→`advance_clock`），`dvance_clock` 残留 0。

### G. 假绿/捷径/诚实性 —— PASS（PARTIAL 台账需补 M-2）

- 无新增 `#[allow]`；无 CJK 断言；无删除/放宽既有断言；无手改 DB；无 SKIP 冒充 PASS；无 0 tests 记 PASS。
- PARTIAL 台账总体诚实：客户端渲染、Hero、默认 DB 学习/掉落、`:1536` 攻城期偏差、125 不回显施法者、次级目标随机序均单列；M2 RNG 已闭合、can_fly 不合并已登记。
- 缺口仅 P7 事件 ring 语义未入 README 台账（M-2）；`target/round58/s2/attempt-1/00-summary.md §8.2` 与 `round58_projectile_wire.rs:250-270` 有实现/测试依据。

### H. 入库卫生与文档一致性 —— PASS（M-1 为 stale 状态）

- README/roadmap/magic.md 状态与实现一致：magic.md 已同步 125 口径；roadmap 无 R58 变更（无矛盾）；README §3/§4 的 PARTIAL 与 R59 承接关系明确。
- 无「AOI/125/R58 完整」宣称；本轮仅新增“已实现/已登记/PARTIAL”表述。
- 唯一不一致：README 5 处「在途（S1）/（S3）」为提交前状态残留（**偏保守，非虚假宣称**），需按 M-1 修正。
- 提交卫生：两提交路径不重叠、无禁用文件、单文件最大 565 行；`docs/testing/round58/final-manifest.json` 等审计产物未入库（按只读边界，不提交）。

## 三、假绿/捷径逐项核对表

| 风险点 | 结论 | 证据 |
| --- | --- | --- |
| 门禁日志复用/旧 attempt | 通过 | 仅 attempt-1；meta head=d404ed1；无 attempt-2 文件 |
| 门禁数字造假/多算 | 通过 | 201,795 B；12 汇总 OK；13 `==>` / 13 `^OK` 已解释；0 FAILED/error/warning |
| 只跑 0 tests 假绿 | 通过 | 148 个 `test result:` 均 ok；R58 两测试 3/7 passed；脚本有 0 tests→FAIL 防护 |
| 协议路径非真实 | 通过 | `packets::{server,client}::roundtrip::…` 各 1 passed/177 filtered；`packets::tests`=0 |
| SKIP 冒充 PASS | 通过 | 正式跑 SKIP=0；脚本缺文件判 SKIP 并与 PASS 分列 |
| 删除/放宽断言 | 通过 | 既有测试零改动；仅 magic.rs 改序 +13/−7 |
| 新增 `#[allow]` | 通过 | 新增 diff 中 0 处 |
| CJK/本机路径断言 | 通过 | 新增测试断言均为协议/数值/ID |
| 手改 DB | 通过 | 无 DB 文件变更；S4 仅文档 |
| 文档宣称“完整” | 通过 | 「125 全链」4 处均禁令语境；「R58 完整」0 |
| 链接失效 | 通过 | check_links exit 0（1716） |
| M4 断言不可失败 | 通过 | 旧序红 `assert_ne!` 失败 + exit 101；新序绿 3 passed |
| M5 三处口径 | 通过（核心句） | grep -F 各 1；行级前缀差异见 O-4 |

## 四、PARTIAL 判定

| 编号 | 事项 | 判定 | 依据/承接 |
| --- | --- | --- | --- |
| P1 | 客户端 125 渲染（Missile 410 / Effect 570(10,600) / 音效 +1/+2） | PARTIAL → R59 | README §4.8、magic.md:687、round57 README:51 |
| P2 | Hero 无实例 | PARTIAL | README §4.7 |
| P3 | 默认 DB FireBounce 学习书/掉落生产路径未证 | PARTIAL | README §4.2/§2 第 8 项 |
| P4 | `:1536` FireBall/GreatFireBall/FrostCrunch 攻城期 `can_fly` 偏差 | 已登记，R59 单列；动前须自带攻城期回归 | README §3、magic.md:791-796 |
| P5 | 125 不回显施法者（`World::broadcast` 不含本人） | PARTIAL/已知偏差 | README §4.6、magic.md:687 |
| P6 | 次级目标随机序列（稳定排序、集合等价） | MINOR | README §4.5/§2 第 9 项 |
| P7 | 事件 ring 语义（16/丢最旧/高频丢事件） | 已实现并测试；台账缺失 → M-2 | net.rs:166/7237-7260、wire 测试 :250-270、S2 §8.2 |

已闭合项：M2 终局 RNG 差 1 次抽签、`can_fly` 不合并登记、`dvance_clock` 错字、脚本协议路径、UTF-8 双件、M1 常量复用、M2 白名单负例、M4 可失败断言。

## 五、Crystal 对照

- `github/Crystal/Server/MirObjects/HumanObject.cs:5811-5833`：`:5813` 守卫（target null / 非攻击目标 / 非 CanFly / bounce==0 直接 return）；`:5826` 广播 125；`:5829` 才排队（delay=MaxDistance*50，bounce==-1 加 500）。
- 同文件 `:5947-5956`：`:5949` 找 3 格内可飞行攻击目标；`:5951` `Count>0` 才抽；`:5953` `Random.Next` **先抽**；`:5955` `FireBounce(..., --bounce)`——终局抽中即废，本作已对齐。
- `github/Crystal/Client/MirScenes/GameScene.cs:4687-4716`：`:4691` source 未知丢弃；`:4693` 仅 FireBounce、无 default；`:4697` Missile 410；`:4703` Effect 570(10,600)；`:4699/:4708` 两段音效（未实现，P1）。
- 协议锚点：`crates/mir-protocol/src/packets/server.rs:3903-3929`（`ObjectProjectile{spell,source,destination}`，INDEX=125，COMPRESSED=false）；`:10696/:10700` server roundtrip；`crates/mir-protocol/src/packets/client.rs:5596/:5600` client roundtrip。
- 偏差锚点：`magic.rs:4029` `can_fly` 走 `walkable_on`（含攻城阻挡）；`magic.rs:4047` `can_fly_cell_valid` 仅 `Cell.Valid`；`:1536` 三法术用前者、`:1555` FireBounce 用后者——不合并，P4 已登记。

## 六、复现命令

```bash
git rev-parse HEAD origin/main
sha256sum target/round58/gate/attempt-1.log target/round58/gate/attempt-1.meta.txt target/round58/gate/attempt-1.utf8.log target/round58/gate/encoding.log
cat target/round58/gate/attempt-1.meta.txt
grep -n '^==> ' target/round58/gate/attempt-1.log
grep -c '^OK ' target/round58/gate/attempt-1.log
while read -r h p; do p=${p#\*}; git show d404ed1:"$p" | sha256sum; done < target/round58/commit-verify/01-tree-hashes.txt
cargo test -p mir-server --test round58_rng_order
cargo test -p mir-client --test round58_projectile_wire
bash -n docs/testing/round58/run-checks.sh
bash docs/testing/round58/run-checks.sh --list
bash docs/testing/round58/run-checks.sh   # 只读复跑；正式门禁仍以 Lead attempt-1 为准
py -3 tools/docs/check_links.py
```

原始输出摘要见 `target/round58/audit/summary.log`；机器可读清单见 `docs/testing/round58/final-manifest.json`（26,063 B，sha256 `740e781f016148666a0ae30ea96ace57a1a9644029cf8dc01d3c50ee1d76601b`）。

## 七、观察项（不影响判定，建议下轮顺手修）

- **O-1** `encoding.log` 的 `diff_chars=183432` 与 `raw_sha==utf8_sha` 矛盾，疑为字符总数误标；建议改 `diff_bytes=0`。
- **O-2** S1 红日志无显式 `EXIT=101` 行，exit 由 `10-meta.txt` 承载；建议单条日志末尾记原始 exit。
- **O-3** S4 `03-redline.log` 称 `added_125_chain=0`，实为 4 处禁令语境；建议判据改为「宣称性表述=0」并附语境。
- **O-4** M5 三处非整行逐字（前缀/后缀差异）；按核心句口径满足，若验收要求整行需统一。
- **O-5** `f119a20` 合并 S1+S2+S3 单提交；建议下轮按派单粒度拆分。
- **O-6** `EFFECT_EVENT_LIMIT` 行号 165→166 漂移导致测试注释与实测不一致；建议注释只写常量值。

## 八、审计期间工作树变动（16:01，未提交）

- 审计起始（15:49）工作树仅 2 个冻结 JSON 未跟踪；16:01:17 `docs/testing/round58/README.md` 被外部修改（`git status` 显示 `M`，mtime 16:01:17），内容恰好覆盖 M-1/M-2：
  - 5 处「在途（S1）/（S3）」全部改为「已落盘（S1/S3）」，并从「口径」允许值中删除「在途」；
  - 新增 `§4.9 事件 ring 语义（P7，已知限制）`：`EFFECT_EVENT_LIMIT=16`、`net.rs:7257-7259` 的 `remove(0)` 丢最旧、R59 消费方须接受可能丢最旧、证据指 S2 §8.2 与 wire 测试 `:250-270`；
  - §5 脚本与 S1–S3 证据状态改「已落盘」。
- 本报告结论以 **d404ed1 提交版** 为准：提交版 README sha `b198e1e4…` 仍含 5 处「在途」，故 M-1/M-2 在提交版成立；工作树修正内容正确但**未提交**，也未重跑 `check_links`/门禁。审计未代改、未 commit。

## 九、验收建议

1. **可进尚书验收**：先由 Lead 提交上述工作树 README 修正（M-1/M-2 已按建议改好，只差 commit），并复跑 `check_links`（新增 §4.9 无外链变更，预期 1716 不变）；若尚书以 `d404ed1` 为验收对象，则须先修再验，M-1/M-2 记为「提交版必改、工作树已修待提交」。
2. 尚书验收以 `final-manifest.json` 的 12 文件 sha、门禁 log/meta sha 与本报告复现命令为准；复跑脚本应保留逐条原始 exit。
3. 对外表述严格限定为「服务端 125 生产已实现、客户端仅解码登记、渲染 PARTIAL 待 R59」，禁止「125 全链」「R58 完整」。
4. 下轮 R59 开工前先补齐 P4 的攻城期回归用例，再评估 `:1536` 是否统一到 `can_fly_cell_valid`。
