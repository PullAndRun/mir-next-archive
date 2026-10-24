# R50 最终只读终审 v2

## R50-T2 复审

**最终 verdict：PASS-WITH-PARTIAL。** 已提交修复 `2ef42854a2ed3b1bd3ad719dbc64eb19c2cf20d5` 消除了 v1 唯一阻断 P11；attempt=3 在同一已提交 HEAD 上完成，P01 已消除；H2 独立实测 782 链接补齐 P09。本结论适用于 R50 已界定功能范围，剩余 PARTIAL 不阻断该范围合入，但不代表严格复刻 Crystal、逐像素一致或外部可选数据分支必然执行。

父单 `JJC-20261025-002`；子单 `JJC-20261025-002-sub-dbf6cc9a`；刑部 R50-T2。开始 2026-10-25 07:16:49 +08:00。仅阅读现有代码、git 元数据和保留日志；没有执行 ci.ps1、cargo 或 ignored 测试，没有 git 写。仅覆写本报告、[manifest](final-manifest.json)，新写[attempt=3 摘录](../../../target/round50/final-gate2-summary.log)。下方 v1 全文保留为历史引用，包含旧 BLOCK、旧 HEAD、旧 P 项状态，均不作为当前规则；本节覆盖其当前结论。

### A. attempt=3 门禁：PASS

[原始 meta](../../../target/round50/final-gate2.meta.txt) 实读 attempt=3、原始进程记录 `exit_code=0`、开始 `2026-10-24T23:12:39Z`、结束 `2026-10-24T23:15:50Z`、head 与本次 HEAD 均为 `2ef42854a2ed3b1bd3ad719dbc64eb19c2cf20d5`。meta 427 B，实算 SHA256 `91c06d8db940aaec269a009816e9778b5ebe3b169b9f63c10d6d196fa4ed99fe`。[原始日志](../../../target/round50/final-gate2.log) 186,193 B，按原始字节实算 SHA256 `47c8405c1a28c4bd651ff9abc21896ac7e4611617e573cab1f810035afeeeacb`，与 meta 一致。

逐项提取结果为 **12 OK、0 FAIL/ERROR**，末行是“全部通过（12 项）”。exit 0 的来源是 meta 对原进程的记录，本次没有运行该进程或由摘要推算退出码。混合编码日志按原始字节哈希；尾部 CP936/GB18030 解码只用于摘录。

| 门禁项 | 实际结果 | 秒数 |
|---|---|---:|
| fmt | OK | 2 |
| clippy | OK | 5 |
| duo_smoke 串行 | OK | 27 |
| workspace | OK | 150 |
| 协议生成器 | OK | 1 |
| 文档链接与包编号 | OK | 0 |
| TextKey | OK | 0 |
| player-visible 阈值70 | OK | 0 |
| no-test 阈值0 | OK | 0 |
| 旧资源引用 | OK | 0 |
| 物品图标完整性 | OK | 1 |
| PostgreSQL 串行集成 | OK | 3 |

与 attempt=2 保留的日志、meta 对照：运行命令仍为 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，12 项及阈值相同；输出路径、提交身份、版本说明、时间、耗时和日志字节随本次运行更新，不能声称两日志逐字节相同。`git diff b3be4af 2ef4285 -- tools/dev/ci.ps1` 为空。attempt=2 的 meta 是提交前 HEAD `2a7e77e`，实际工作树已含功能；不能将该提交树与本轮已提交树混淆。历史日志 SHA256 `8676fee8ae4d89ed3e50b26d010de9165b154fafac5c7cb5d8f0cdfed3d541ca` 保留。

### B. 提交树、远端与入库边界：PASS

只读 `git log --oneline -3` 得 `2ef4285 / b3be4af / 2a7e77e`。`git show --stat 2ef4285` 为 7 文件、334 增/72 删，只有客户端状态、面板、面板测试及四份产品文档。通过题目指定 gh credential helper 查询远端 main，实测等于完整 HEAD。四批提交逐文件 SHA256、字节数和最后涉及提交均在 manifest 登记。

**计数纠正**：`git diff-tree --no-commit-id --name-only -r b3be4af` 实际 28 个文件，不是输入概述的 29；返工提交实际 7 个。功能批原有服务端改动属于 R50-B/E 范围；返工提交没有服务端或协议改动。四批新增/修改路径没有 target、game-assets、resource 或 .env，也没有超过 1 MiB 的文件。初始与交付 `git status --porcelain` 仅两个既有大冻结 JSON 加两份授权审计文档，target 摘录被现有忽略规则忽略；未将冻结 JSON 入库。

### C. C2 修复与回归证据：PASS

[状态 upsert](../../../crates/mir-client/src/net.rs#L1845) 实际关键行 `let sold_out = item.stock > 0 && stock_level <= 0;`，新条目和已有条目刷新均写入此值；负剩余先归零。[apply_stock](../../../crates/mir-client/src/net.rs#L1870) 对限量商品归零置售罄、回正解除售罄；保留条目。[面板显示](../../../crates/mir-client/src/ui/gameshop.rs#L590)、[份数夹取](../../../crates/mir-client/src/ui/gameshop.rs#L643)、[check_buy](../../../crates/mir-client/src/ui/gameshop.rs#L711) 不再以 `i_stock` 或剩余零代表不限量；限量剩余零防御拦截，即使 stale `sold_out=false` 也返回 `SoldOut`。`i_stock` 只在服务端选择个人或全服已购表。

精确边界：源码显示与 apply_stock 实际为 `item.stock <= 0`，文档写 `==0`。服务端 `remaining` 用 `stock !=0` 判限量，负总量通常会被 `remaining<0` 过滤，不作为正常下发商品；此非法输入防御差异保留风险，不伪称源码逐字为 `==0`。本次修复和回归针对原始总量正数的限量耗尽、原始总量零的不限量，均成立。

[保留红绿证据](../../../target/round50/r50-c2/README.md#L71) 与[退出码记录](../../../target/round50/r50-c2/commands-and-exit-codes.txt) 逐项相符：lib 旧语义重放 **12 passed/3 failed、exit101** → **15 passed/0 failed、exit0**；面板旧实现 **8 passed/5 failed、exit101** → **13 passed/0 failed、exit0**。三项 lib 是显示、夹取、购买矩阵；三项新增面板是首包限量已售罄、首包真正不限量、已有条目目录刷新归零。不能把面板红日志的实际五项失败写成三项失败；其中三项是新增边界。

[feed_wire](../../../crates/mir-client/tests/gameshop_panel.rs#L140) 经 `encode_frame → FrameBuffer::next_frame → decode_server_packet → WorldSnapshot::apply_packet`；[首包回归](../../../crates/mir-client/tests/gameshop_panel.rs#L1377) 另断言 UI 点击不生成购买包。[真实生产路由](../../../crates/mir-client/src/net.rs#L5700) 的 250/251 分别进入 upsert/apply_stock，协议定义和生成器没有改动。3+3 的准确说法是三项 lib 边界加三项面板真实帧链边界；lib 自身不冒称网络测试。

C2 红日志保留了真实断言失败；lib 红为还原旧语义重放，面板红为修复前首跑。中途失败日志也保留，并未以中间结果替换最终绿。当前已提交树的完整门禁提供后续验证，本次只读审计没有复跑这些测试。

### D. H2 五组文档与链接：PASS-WITH-PARTIAL

逐读商城失败包说明、库存三分语义、客户端状态说明、i18n 提交时态、R50 索引与 P09/P12：业务规则与正数/零总量实现相符。失败保证现为“不发新的 GameShopStock”，允许 SystemChat/MarketFail；未再过度声称任何失败均无包。[H2 链接原日志](../../../target/round50/r50-h2/check-links.log) 实际 782 条相对链接、2 篇 README 引用、市场包编号一致、exit0，实算 SHA256 `a19a4bc15b5f6497eadc981e13470a65e233c89eb0ebfd895d9e045e5cfc1a5d`。无断链错误；不是沿用旧 756 计数。搜索 docs 与根 README 的三种指定旧措辞没有命中；原始搜索在本 v2 写入前完成。

文档状态仍有历史滞后：[商城状态](../../world/gameshop.md#L39) 写当前 HEAD=b3be4af，[商城 P11](../../world/gameshop.md#L384) 与[客户端 P11](../../client.md#L909) 仍说返工中；这些是 H2 在 C2 提交前的记录，当前事实由本 v2 和 manifest 校正为 2ef4285、P11 已修。此状态文案风险并入 P12，建议后续文档批同步，本次权限不修改产品文档。五组内容不应一概描述为已全部同步到最终提交身份。

### E. 证据真实性与冻结边界：PASS-WITH-PARTIAL

C2/H2 原始日志、README、T v1 原文均实际存在；v1 输入指纹登记 manifest 的 history 字段，v1 报告全文保留如下。facts 77/57、总108、92 hit/16 deviation、负向探针 exit1 为历史快照数。E 两 deviation 原样保留：8 条显示名走并行模板表替代 TextKey；授权扩写 world/maps 接显示边界。素材 manifest 与已知图库/发布包、冻结清单依据当前实际字节复核，详见 manifest 的 evidence_index 与 verification。

F3 原文实际 **25 帧、语义不匹配0、像素差异3帧、合计2,580像素**。未复跑图片差分算法，不写像素零差异。历史冻结 70 文件、129,219,674 B 的每条指纹按实物复核；R49 映射与缺少的运动证据继续保持原边界。

### F. 假绿、捷径和范围：PASS-WITH-PARTIAL

扫描功能与返工完整增量的 allow/ignore/skip/fixture/提前返回，结合关键路径阅读：功能批新增一条局部 `#[allow(clippy::too_many_arguments)]`（原 P 项已披露），返工没有新增 allow/ignore 或返回成功的测试短路。测试辅助在 tests 或 cfg(test) 中；生产目录仍依赖 GameData。工具 `_skip_*` 是二进制读取跳过无关段，不是跳过测试；素材 anchors 信息性 skipped 和真实样本库缺失成功返回仍按 P07 保留。新增售罄分支的早退是返回错误或不造未知条目，不能误记为测试作弊。返工没有服务端/协议改动，原功能批服务端是正常授权范围。

### G. P01–P12 状态与最终风险

| 编号 | v2 状态与保留事项 |
|---|---|
| P01 | **已消除**：attempt3 gate head==HEAD==2ef4285，日志哈希/meta/12OK吻合。历史 attempt2 身份边界保留于 v1。 |
| P02 | 保留：attempt1 首跑 clippy21/exit1 原文被覆盖，只能引用历史说明，不能独立重建。 |
| P03 | 保留 E deviation1：8 显示名用模板词表替代8 TextKey。 |
| P04 | 保留 E deviation2：授权 world/maps 显示边界扩写。 |
| P05 | 保留：F 3/25 帧、2580像素差异；C无像素回归基线。 |
| P06 | 保留：D历史826→827条、771854→772675 B 更正及现物指纹。 |
| P07 | 保留：真实库/素材检查依赖缺失可软跳过，通过计数不能证明真实分支执行。 |
| P08 | 保留：固定K/R51重绑、角色元宝、背包优先邮件兜底、GIndex修正、缺货不刷库存等Crystal差异。 |
| P09 | **已补**：H2真实782链接、2篇README、0断链/exit0日志；旧756仍仅历史。 |
| P10 | 保留：R49 !Mon.dat精确映射、坐骑/Push/Jump/Dash/Blend117/Reverse/Effect/Sorting、缺 self_moving/landed 证据。 |
| P11 | **已修**：2ef4285 首包/目录刷新售罄、i_stock不再代表无限，3 lib+3面板边界红→绿，完整门禁同HEAD通过。负总量非法输入防御差异另披露。 |
| P12 | 保留：facts历史108锚点/16deviation不是当前源行认证；产品文档仍有b3be4af/返工中状态滞后，以本v2校正。 |

以上残余不会重新触发 v1 的售罄限量商品误放购买缺陷，故不阻断 R50 范围合入。不能据此将全部 PARTIAL 清零、声称像素一致或严格逐分支 Crystal 对齐。

### 只读复现与产物校验

```bash
git status --porcelain
git log --oneline -3
git show --stat 2ef4285
git diff b3be4af 2ef4285 -- tools/dev/ci.ps1 crates/mir-server crates/mir-protocol
git -c credential.helper='!gh auth git-credential' ls-remote origin refs/heads/main
py -3 -X utf8 -c 'import json,hashlib;from pathlib import Path;m=json.loads(Path("docs/testing/round50/final-manifest.json").read_text(encoding="utf-8"));bad=[x["path"] for x in m["files"] if Path(x["path"]).stat().st_size!=x["bytes"] or hashlib.sha256(Path(x["path"]).read_bytes()).hexdigest()!=x["sha256"]];print(bad);assert not bad'
py -3 -X utf8 -c 'from pathlib import Path;import hashlib;p=Path("target/round50/final-gate2.log");b=p.read_bytes();print(len(b),hashlib.sha256(b).hexdigest());print(b"\n".join(b.splitlines()[-16:]).decode("gb18030"))'
```

本次使用 edict 与 code-review-7d 技能；用户三文件写范围优先，未创建额外HTML报告。manifest 不自哈希以避免递归；最终文件外部哈希在交付消息登记。

**交付时点补记（07:26:21 +08:00 后复核）**：父代理在本审计核验期间将两份审计文档提交为 `2e7e89b`，因此交付工作区 HEAD 已从被审功能提交 `2ef4285` 前移。只读 `git diff 2ef4285 --stat` 仅两份授权审计文档，crates/config/tools/产品文档功能树没有改变；当时 status 仅两个既有冻结 JSON。P01 的“gate head==HEAD”严格指门禁及本次审计取证时的 `2ef4285`，不声称 attempt3 曾在后续审计文档提交上运行。本补记与 manifest 指纹更新发生于该父代理提交之后，随后 status 可出现两份授权审计文档修改；本刑部执行者没有 git 写操作。

---

## v1 原文存档（仅历史引用；以下 BLOCK/P01/P09/P11 为当时结论）

# R50-T 最终只读终审

**Verdict：BLOCK。** 门禁原始证据为通过；当前客户端仍有“首次登录即耗尽的限量商品被显示为无限库存且允许发起购买”的确定边界缺陷，因此不建议把 R50 收尾认定为全部通过。服务端库存校验仍会拒绝购买，未发现因此超卖或绕过扣款。

父单 `JJC-20261025-002`；子单 `JJC-20261025-002-sub-f2318e27`；刑部 R50-T。审计对象为 HEAD `b3be4afcef9934d18b2b5bbe074e182235b31252`。开始时间 2026-10-25 06:45:27 +08:00。本报告按代码和保留日志做只读复核，没有运行 CI、cargo 测试或 ignored 测试，没有修改实现、配置和工具，没有 git 写操作。仅写入用户授权的三个终审产物；使用 edict 技能回填任务结果。

产物：[manifest](final-manifest.json)、[本报告](round50-final-audit.md)、[门禁摘录](../../../target/round50/final-gate-summary.log)。门禁摘录先于本报告落盘；manifest 记录 34 个提交涉及的去重文件及主要证据的实算字节/SHA256。manifest 不自哈希以避免递归；报告的外部哈希在交付消息登记。

## 3–5 条关键 finding

1. **中等，阻断验收：初始零库存判错。** [客户端状态](../../../crates/mir-client/src/net.rs#L1831) 的 `upsert` 新增条目无条件 `sold_out: false`；[库存显示](../../../crates/mir-client/src/ui/gameshop.rs#L587) 把 `stock_level <= 0` 当无限；[购买检查](../../../crates/mir-client/src/ui/gameshop.rs#L701) 只拦 `sold_out` 并仅对正库存限制数量。服务端对 `Stock>0`、`Stock-purchased==0` 的商品仍下发目录条目，所以新登录者看到 ∞，余额足够时可以发送购买请求。应使用商品原始 `stock` 与剩余库存共同判定，保持真正不限量商品 `Stock==0` 的语义，并增加耗尽后重新登录的边界回归。
2. **门禁真实通过，但快照身份有边界。** 原始日志 185,889 B，实算 SHA256 `8676fee8ae4d89ed3e50b26d010de9165b154fafac5c7cb5d8f0cdfed3d541ca` 与 meta 相同；逐项实读得到 12 个 OK、0 个 `[FAIL]`/`[ERROR]`，末行实际为“全部通过（12 项）”。meta 的 HEAD 是 `2a7e77e`，功能批随后提交为 `b3be4af`。当前跟踪文件干净且涉及文件 mtime 均早于门禁结束，支持同一功能工作树的解释；没有门禁时点的逐文件哈希冻结，不能把这解释升级为严格同快照证明。
3. **测试覆盖存在软跳过。** [真实库登录用例](../../../crates/mir-server/tests/gameshop_wire.rs) 找不到可选样本库时直接返回成功；素材检查工具对缺源库/缺发布包也有信息性跳过。合成 `.mnd` 测试确实经过 TCP 服务端和协议，不是假装网络链路；但测试通过数不能直接证明真实外部数据分支一定执行。当前图库和库文件实存且哈希吻合，降低了本次素材缺失风险，未补足未来 CI 的覆盖约束。
4. **F 语义通过不等于像素无差异。** [F3 记录](../../../target/round50/r50-f/f3-attack-semantics.json) 为 `SEMANTIC_IDENTICAL`，但 **25 帧中 3 帧像素不同，共 2,580 个像素**；不得写为 0 diff。独立实算冻结清单的 70 文件 / 129,219,674 B，当前 0 个哈希不符。
5. **文档需要返工。** [商城说明](../../world/gameshop.md#L39) 和 [R50 索引](README.md) 仍把已提交功能写成未提交；商城说明中“任何校验失败不发任何包”的表述比实际会话行为更宽，实际保证是失败不发新库存包。客户端文档把“初始目录耗尽也按无限处理”写为规则，掩盖 finding 1。独立 H 日志没有找到，756 链接 / 0 断链的特定计数不做独立背书。

## A. 门禁与工作树：PASS，附 P01/P02

[原始 meta](../../../target/round50/final-gate.meta.txt) 记录 attempt=2、原始进程 exit_code=0，开始 `2026-10-24T22:41:52Z`、结束 `2026-10-24T22:44:33Z`。meta 本身 SHA256 `a97a3564f048c084fc788a609ce6eef7cd186f257e015e69cc17c84c11d250de`。

从原始日志尾部提取的逐项结果如下，数量来自本次实际文件，没有预设验收常数：

| 项 | 结果 | 日志秒数 |
|---|---|---:|
| fmt | OK | 2 |
| clippy | OK | 1 |
| duo_smoke 串行 | OK | 25 |
| workspace | OK | 127 |
| 协议生成器 | OK | 1 |
| 文档链接与包编号 | OK | 0 |
| TextKey | OK | 0 |
| player-visible 棘轮 70 | OK | 0 |
| no-test 棘轮 0 | OK | 0 |
| 旧资源引用 | OK | 0 |
| 物品图标完整性 | OK | 1 |
| PostgreSQL 串行集成 | OK | 2 |

原始日志含混合 UTF-8/CP936；以 gb18030 容错解码尾部恢复中文摘要，哈希始终按原始字节计算。没有将解码后的文本当原始哈希输入。

初始 `git status --porcelain` 只有两个已知未跟踪大冻结 JSON；target 被忽略。产物落盘后最终复核状态恰为原有两个大冻结 JSON 加本次两个授权终审文档，无跟踪文件修改、无额外未跟踪项。manifest 首次校验 47 条文件记录全部哈希/字节匹配，大小 13,928 B；后续仅补充冻结 JSON 指纹与审计发现，最终大小和哈希以交付复核值为准。门禁在提交前运行，HEAD 差异与 P01 对应，不伪称最终 commit 直接运行过此命令。

第一次门禁 exit 1、clippy 21 条的事实由队长传入；首跑日志已被覆盖，无法在当前保留的原始日志里独立复现该条数。[C 的修复日志](../../../target/round50/r50-c/clippy-fix.log) 保留成功结束输出，第二次完整门禁也包含 clippy OK。两者都不能重建已经覆盖的首跑原文，见 P02。

## B. 提交合法性：PASS

只读 `git log` / `git show --numstat` 核对链为：

| 提交 | 文件数 | 增/删 |
|---|---:|---:|
| `4d4175a` process docs | 3 | +436 / -0 |
| `2a7e77e` A 门禁行 | 3 | +11 / -4 |
| `b3be4af` 功能批 | 28 | +9,900 / -191 |

三批去重涉及 34 个文件；本次实算这些工作树文件的大小和 SHA256 登记在 manifest。无一个超过 1 MiB；这三提交的新增/修改路径中没有 target、game-assets、resource 或 .env。两个超限冻结 JSON 分别 44,844,165 / 45,219,533 B，保持未跟踪，未被此批入库。

只读远端查询 `git -c credential.helper='!gh auth git-credential' ls-remote origin refs/heads/main` 得到 `b3be4afcef9934d18b2b5bbe074e182235b31252`，与当前 HEAD 一致。普通查询因当前终端无凭据交互能力失败，使用题目允许的 gh helper 后成功，没有 git 写。

## C. Crystal 对齐抽查：PASS-WITH-PARTIAL

源代码对照：[Crystal PlayerObject](../../../github/Crystal/Server/MirObjects/PlayerObject.cs#L13703)、[服务端 market](../../../crates/mir-server/src/market.rs#L300)、[session](../../../crates/mir-server/src/session.rs#L3052)。

- 登录目录：Crystal `GetGameShop` 的目录顺序、限量 `Stock-purchased`、负数省略、零剩余仍发送、不限量发送 0，在 Rust `game_shop_info_packet` / `game_shop_info_packets` 中对应；会话在 UserInformation / 物品模板之后、ChangeAMode 之前下发。
- 成功库存包：Rust `flush_gameshop_update` 完成后才发 `GameShopStock`；失败、回滚不发虚假新库存，符合本轮裁决。校验失败仍可能有 SystemChat / MarketFail，不能写成任何包都没有。
- Crystal 库存不足会发送聊天并刷新库存；Rust 统一失败分支不刷新库存。这是本作事务语义选择，不能宣称逐分支完全照抄 Crystal。
- GIndex：Crystal `GameShopStock` 用 Info.Index 查/发、购买落计数却用产品 GIndex，存在原实现口径不一致；Rust 全部采用产品 GIndex，并有中书决策注释。属于明确修正，不是完整复刻原 bug。
- class/level 校验、角色级元宝、背包优先放不下转邮件，与 Crystal 的账号元宝/全邮件不完全相同；这些已列文档差异，继续保留 PARTIAL。商品目录/价格来自 GameData / DB，没有发现以 fixture 商品列表替代生产来源。

## D. 客户端：BLOCK

[热键接线](../../../crates/mir-client/src/app.rs#L2857) 可确认文本输入处理后提前返回，K 分支位于该守卫之后；`live_toggle_gameshop` 再检查 Mode::Live。网络 `GameShopInfo` / `GameShopStock` 真实路由进入独立 `GameShopState`，按 GIndex 查找更新，未与市场页签状态混用。`GameShopStock(0)` 会保留条目并置售罄。

**未满足的是初始目录包剩余库存 0 的限量商品**：服务端合法下发，客户端 `upsert` 却把所有初始 0 视作不限量。另 [库存显示](../../../crates/mir-client/src/ui/gameshop.rs#L591) 将 `item.i_stock == true` 也视为无限库存，但 [服务端计数](../../../crates/mir-server/src/market.rs#L297) 明确 `i_stock` 只决定个人/全服计数，真正不限量条件是 `product.stock == 0`；所以个人限量商品即使还有正库存也会显示 ∞。[客户端说明](../../client.md#L905) 和 [商城零库存解释](../../world/gameshop.md#L375) 也采用该错误概括。首次登录、耗尽后重登、重复目录把已有商品更新为 0，均应作为返工验证边界。这里是可由代码连锁确定的静态发现，没有运行游戏或 cargo 用例。

C 的面板测试/截图可以证明渲染和交互部件；不能直接当作真实客户端完整登录、耗尽、重登的端到端证据。固定 K 未接入可重绑设置，属已披露 R51 事项。

## E. 证据真实性：PASS-WITH-PARTIAL

[01 实证记录](../../../target/round50/facts/evidence.md) 与其 stdout 文件保留 77 / 57、108 总条目、92 hit / 16 deviation、负向探针 exit 1。它核对的是此前工作树锚点，不是将这些数字重新解释为最终功能批全部锚点都正确。

本审计允许范围内只读复跑 i18n：no-test strict `--fail-above 0` 实际 0 命中、exit 0；含测试 strict `--fail-above 70` 实际 60 命中、exit 0。两个口径符合 [i18n 文档](../../i18n.md#L278)。保留 E 的两条 deviation，未以扫描归零替代设计遵循性。

[D manifest](../../assets/gameshop-manifest.json) 实算 81,386 B、SHA256 `fdf1e178321b68e03647d6d88d823653bd4f9ff3f9a756c391e5108d0788b67f`。逐条复算 82 个图标文件全部符合记录；源库实算 SHA256 `ace4fa6a70e18db7f8454350326d611ce6c56826b9f9a4f214797a3f8a5f69d0`，发布包 `3b28fa1b997a29e78054543642674e00c16ce7ac0884cf9e37808abdd38d886f`，均吻合。827 条来自已保留发布索引/校验记录，本审计没有重新执行素材工具；与实际包指纹绑定记录，不把旧的 826 当现状。

[F 报告](../../../target/round50/r50-f/r50-f-evidence-summary.md) 的 SAME_SNAPSHOT_PASS 7/7、F2_PASS 5/5 有对应 JSON、日志和清单。独立读取 frozen-final 的全部 70 条，按当前实物 SHA256 比对，0 不符，129,219,674 B 总量吻合。F3 保留 3/25 像素差异而非无差异；本次没有重做图片差分算法。历史二进制构建哈希只证明对应构建时点，不要求后续门禁构建永远相同。

H 的约定独立路径 `target/round50/r50-h.log` 实际不存在；检索未发现独立 check_links 计数日志。最终门禁文档项 OK 真实存在，但不凭此推断 756 这个计数。该条列 P09，供队长补路径或降低声明。

## F. 文档一致性：PARTIAL，需返工

[授权说明](../../assets/licensing.md#L103) 与 [素材文档](../../assets/game-assets.md#L157) 明文把旧 826 / 771,854 B 更正为 827 / 772,675 B，符合当前 manifest 和实物指纹。i18n 两种扫描口径相符；F 像素非确定性在 R50 索引已披露。

需修正：R50 索引和商城文档把功能标为未入库的旧状态；零剩余限量商品与无限库存的区分；“任何校验不发任何包”的过宽表述；真实样本可选跳过和证据覆盖范围。文档未把所有历史边界都写成 PASS，但“R50 已完成”的总括对 finding 1 仍然过宽。此审计不越权改这些实现/产品文档。

## G. 假绿/捷径：PARTIAL

扫描功能批增量发现新增 1 条 `#[allow(clippy::too_many_arguments)]`，用于参数多的现有 API 接线；未发现新增大范围 allow 屏蔽全工作区。不能因 clippy 通过断言所有边界正确。门禁数字来自日志实测，不沿用固定“12/12”；哈希全部由实际字节计算，没有复用输入给定值冒充计算。

真实库用例缺库 `return` 与素材工具缺源/缺发布包的信息性成功属于覆盖可空风险。固定样本数据库 SHA/anchor 出现在素材核验工具，用于已知 cjlaaa 快照溯源；生产商城路径依赖 GameData，不把这个工具的固定样本当生产协议硬编码。测试辅助在 tests 或 `#[cfg(test)]` 范围内，没有发现通过 fixture 注入生产目录。全 diff 较大，本审计对关键路径定点抽查及新增抑制/跳过扫描，未宣称穷尽所有分支。

## H. 完整 PARTIAL 清单与建议

- **P01** 门禁 meta HEAD 与最终提交不同；缺门禁时点逐文件冻结指纹，支持性证据不能替代严格证明。
- **P02** 首跑 exit 1 / clippy 21 条、修复后重跑事实保留；首跑原文已覆盖，只能引用队长说明，不能伪称原文独立复核。
- **P03** E deviation 1：8 条显示名用并行本地化模板表代替 8 条 TextKey，受扩展键限制。
- **P04** E deviation 2：授权扩写 world.rs / maps.rs，接入显示边界。
- **P05** F 语义一致但 3/25 帧像素不同，共 2,580 像素；C 无视觉回归基线。
- **P06** D 历史快照 826→827 条、771,854→772,675 B 的更正。
- **P07** 真实库测试与素材检查的可选依赖软跳过，必须区别“测试计数通过”和“真实数据分支执行”。
- **P08** 固定 K / R51 重绑、角色元宝、背包优先邮件兜底、GIndex 修正、缺货不刷库存等 Crystal 差异。
- **P09** H 独立日志缺失；756/0 数字未独立背书。
- **P10** 历史 R49 精确 !Mon.dat 映射、坐骑/Push/Jump/Dash/Blend117/Reverse/Effect/Sorting，以及缺失的 self_moving/landed 证据保持边界。
- **P11** 本次新增阻断：初始耗尽限量商品误判为不限量；客户端/文档/边界用例需同步返工。
- **P12** facts 锚点报告是历史快照，16 deviation 仍须保留，不能替代最终源行号认证。

建议：**暂不把 R50 总体验收标为 PASS，也不建议以“严格 Crystal 对齐已完成”名义合入/发布。** 当前功能批已存在远端主分支，本报告不回滚或改写 git；由队长安排小范围返工修复客户端零剩余边界、同步文档并用针对性回归确认，再更新最终审计与快照凭据。H 日志应补明确路径或去掉未留证的计数。历史像素差异和设计 deviation 继续附在通过条件中，不应因这次返工被遗忘。

## 只读复现命令

```bash
git status --porcelain
git log -3 --oneline
git show --format= --numstat b3be4af
git diff HEAD --stat
git -c credential.helper='!gh auth git-credential' ls-remote origin refs/heads/main
py -3 -X utf8 tools/i18n/scan_hardcoded.py --player-visible --no-test --strict --fail-above 0
py -3 -X utf8 tools/i18n/scan_hardcoded.py --player-visible --strict --fail-above 70
py -3 -X utf8 -c "import hashlib;from pathlib import Path;p=Path('target/round50/final-gate.log');print(len(p.read_bytes()),hashlib.sha256(p.read_bytes()).hexdigest())"
py -3 -X utf8 -c "import json,hashlib;from pathlib import Path;m=json.loads(Path('docs/testing/round50/final-manifest.json').read_text(encoding='utf-8'));bad=[x['path'] for x in m['files'] if Path(x['path']).stat().st_size!=x['bytes'] or hashlib.sha256(Path(x['path']).read_bytes()).hexdigest()!=x['sha256']];print(bad);assert not bad"
```

本报告使用 edict 与 code-review-7d 技能进行治理与代码审查；用户只允许三个文件，故未新增技能模板要求的额外 HTML。上述复现命令不包含全量门禁或 cargo 测试；补丁回归由获授权的实现/队长角色处理。
