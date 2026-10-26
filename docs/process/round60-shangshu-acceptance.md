# 第 60 轮（R60）尚书省验收

- 受审 HEAD：`220c5f6`（审计提交）；代码 `bb53321`，文档 `e053905`。
- **verdict：ACCEPT-WITH-PARTIAL**；采纳 Lead 核验及 D4b `PASS-WITH-PARTIAL`，并独立抽读下列原始佐证。
- 范围：只读指定方案、门下评审、派发、验收索引、manifest、终审与门禁 meta；补读接线、游标、树清单和红绿日志。仅写本文，未跑 cargo/ci、未 commit、未碰看板。

## A 门禁真实性

- 唯一有效门禁为 `target/round60/gate/attempt-1.log`：205,317 B，exit 0，12/12，`gate_head=bb53321`。
- SHA256：`193b6fe4f8a0a46397a0026c2e8a2d66f0e041c9fa33f0ad6ca3a17d1f3678dd`。
- 独立读 `target/round60/gate/attempt-1.meta.txt:1`：START `2026-10-26T10:29:53Z`、END `10:32:26Z`，BYTES/SHA 与 README、manifest 一致。
- 采纳 D4b 对日志的核验：6 条 R60 用例实际执行；13 行 `OK` 含一条程序输出，门禁汇总仍为 12 项。本文未复跑门禁。

## B 提交边界与树哈希

- 提交① `bb53321`：feat(client)，4 文件 +593/−12；提交② `e053905`：docs，10 文件 +925/−7；审计提交 `220c5f6`。
- 采纳 D4b 边界核验：`bb53321..e053905` 的 `crates/**` 差异为空，代码门禁与文档批分离。
- 实际冻结文件为 `docs/testing/round60/commit-tree-hashes.txt`，2209 B；SHA256 `520379f025184edc5ca127e4db70b84d3d0ceebb6edd676622e7d27766abf450`。
- 独立读清单头：`head=gate_head=bb53321`，门禁 bytes/SHA 同 A；13 条覆盖代码及 D3/流程文档，D4b 逐条复算 13/13。
- 清单自身 SHA 仅由 manifest 记录；该冻结不冒充后续审计/本文的全树签名。

## C 目标闭合

- D1：独立读 `crates/mir-client/src/app.rs:2753-2759`，`live.ui.sync` 后同帧取走 cue 并调用 `play_projectile_cues`；取走数与已播数分开记录。
- D2：独立读 `crates/mir-client/src/ui/projectile.rs:461-495`，由 `total−len` 求 dropped，绝对 cursor 跳过已丢历史，以 `cursor−dropped` 取新事件；重建条件收紧，避免 ring 满时误复位。
- 独立读 `target/round60/d2/attempt-1/04-red-before.log:15-28`：第 17 事件断言 left 16/right 17，exit 101；`05-green-after.log:8-13` 同用例 1 passed，exit 0。
- 采纳终审：`projectile_events_total` 自增/归零、ring 16 与 `ProjectileEvent` 保持约束，协议零改；只修游标吞掉第 17+ 事件，不保证 ring 不丢。

## D M-1..M-7 落点

| 项 | 验收落点与口径 |
|---|---|
| M-1 | 方案 rev2、README §4 更正 F6：旧缺陷是第 17+ 事件丢失，期望新增 1。 |
| M-2 | D2 原始红→绿日志 101→0，已独立抽读，禁以旧实现重放 16 为前提。 |
| M-3 | app 注入式核心及 crate 内用例；静音/零增益降级环境无关，终审记录通过。 |
| M-4 | M34-1/2 的 `.mna` 与解码佐证；缺素材 SKIP 记 PARTIAL，素材数量完整性未扩称。 |
| M-5 | net 单调总数与 projectile 绝对游标；真重建归零，ring 满不误复位。 |
| M-6 | 11 项脚本有发现数与 0-tests 守卫，SKIP/PARTIAL 非 PASS；采纳终审实跑记录。 |
| M-7 | 清单头绑定代码/门禁 HEAD，清单自哈希排除并入 manifest，已抽读头部。 |

## E 假绿红线

- 采纳 D4b：新增 ignore 属性 0；注释字面量命中不等于跳测。不得把缺素材、0 tests、SKIP 或频控丢弃的 cue 算作已播。
- i18n 门禁口径带 `--player-visible`；缺该参数的 356 处/exit 1 已登记为命令口径差异，不掩盖成通过。
- 门禁 12/12、脚本 11/11 分属两套检查；可听性与 Live 集成仍未证，不据此宣称 G1–G4 全绿。

## F 卫生

- 采纳终审：未跟踪目录 `target/`、`resource/`、`game-assets/` 及 `.env` 未入库；两份历史大冻结 JSON 保持原状。
- 终审记录文档引用检查 exit 0、差异空白检查 exit 0；本文为单一小型中文 Markdown，不更改代码或既有证据。

## G PARTIAL 与 R61 承接

- MF-1（low，非阻断）：`crates/mir-client/src/app.rs:249` 注释字面 `#[ignore]` 触发朴素 grep；R61 改写措辞，实际新增属性 0。
- MF-2（low，非阻断）：`docs/client.md:1304` 频控交叉引用 P-6→P-4（保留 R-5），R61 修正文档。
- P-1：可听性/无 Live 窗口集成，R61 真机手听留证并补可注入帧集成测试。
- P-2：`from_repo_root` 根路径依赖 CWD，换机缺 M34 按 SKIP 非 PASS；R61 收口根路径。
- P-3：ring 16 仍丢最旧，R61 评估容量/队列，不能承诺不丢。
- P-4：同文件 100ms 频控保留，与 Crystal 每发都响有差异；R61 评估并发上限。
- P-5：M34 源自 R59 导入的 community Crystal wav，素材目录不入库；R61 脚本化导入与来源复现。
- P-6：脚本首跑 TEMP 不可写导致两项 FAIL，改指 `target/tmp` 后 11/11 PASS；保留首败日志，R61 固化环境前置。

**结论：ACCEPT-WITH-PARTIAL。125 音效通道打通与事件去重，PARTIAL 诚实登记，禁止称全链/完整/真实可听已证。**
