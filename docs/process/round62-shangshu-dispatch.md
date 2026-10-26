# R62 尚书派发：飞行时钟与回收契约对齐

主任务：`JJC-20261026-009`「第62轮：飞行时钟与回收契约对齐」（Doing，尚书省）。
依据：`docs/process/round62-plan.md`（76 LF/77 逻辑行，8,157B，sha256 `dc8e62ee…115d2f`）与门下审阅 `docs/process/round62-menxia-review.md`（47 行，6,561B，sha256 `584be23b…4da3`，PASS-WITH-CHANGES）。派发与登记 CLI 原始回执见 `target/round62/shangshu/01-delegate.log`。

## 一、子任务登记表（CLI delegate 已落，186 条看板校验通过）

| 子任务 ID | 部门 | 内容 | 依赖 | 写范围 |
|---|---|---|---|---|
| `sub-e4499be7` | 兵部 | A 飞行时钟/回收语义（NextFrame/SkipFrames、TargetPoint 缓存回退、运动 offset、确认裁剪/会话复位） | 无 | `crates/mir-client/src/ui/projectile.rs`（+必要时 `net.rs` 数据契约最小改动）、定向单测 |
| `sub-2ee8a17b` | 兵部 | B 客户端接线（125 packet→snapshot→sync/cue 全链最小接线与集成断言） | A | `crates/mir-client/src/net.rs`、`ui/mod.rs` 最小接线、集成测试 |
| `sub-7c491d88` | 刑部 | C 测试与证据（真实红→绿、Crystal 对照表、PARTIAL 诚实性） | A、B | `target/round62/**`、`docs/testing/round62/**`（只读实现，需新测试先登记路径） |
| `sub-c9de34ed` | 礼部 | D 文档与脚本（R62 README/run-checks/roadmap/client.md 指针） | C | `docs/testing/round62/**`、`docs/roadmap.md`、`docs/client.md` |

映射约定：子任务 ID ↔ 启动的 deepseek 团队成员在 Lead 启动时登记到 `target/round62/lead-mapping.md`；每个成员只写自己范围，A→B→C→D 串行，同文件不得并行写。

## 二、C1–C5 落实口径（门下条件，逐条绑定责任）

- **C1（A 落实，尚书核表）**：`SkipFrames` 语义 = `Time > NextFrame` 到期先按 `Owner.SkipFrames` 额外 `++` 再普通 `++CurrentFrame`（Crystal `Effect.cs:81-100`），**不是 elapsed 追赶**。M1 固定预期表：`now == NextFrame` 保持旧帧；普通到期恰 +1；skip 到期 +2；大时间跳跃不逐帧追补；同刻重复调用不变。
- **C2（A/B 落实）**：125 包 `ServerPackets.cs:3564-3578` 只有 Spell/Source/Destination，**无点坐标**；`TargetPoint` 仅取可信客户端缓存（`PlayerObject.cs:3546-3556` 同类语义），无缓存不生成、不造实体/坐标、不改 wire 包；未知/死亡目标零命中，目标重现不补播旧事件。
- **C3（A/B 落实）**：像素 offset 是**运动量**：屏幕坐标 = 格投影 + 运动 offset（只叠加一次 camera/actor 偏移）；静态目标每轴朝终点单调、无越界、终点残差零；动态 retarget 固定样本表并声明允许变化；同 generation 裁剪空表不重置，新 generation 清旧 view/hit/cue。
- **C4（尚书+全体）**：锁定测试过滤器（`cargo test -p mir-client --lib ui::projectile::tests:: -- --nocapture` 与具体 net/集成过滤器），禁止 0 tests 假绿；C 默认只读实现，需新增测试须先登记精确路径与所有者并移交。
- **C5（全体+Lead）**：红证据必须实际断言失败/非零退出并保留双流；绿测试同断言且有正例计数；缺测试说明不能代红；全量门禁仅 Lead 执行 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale` 并完整捕获双流/退出码。

## 三、交付契约（每个执行成员）

1. `recall`/`progress` 子任务，最多一个 todo in-progress；CLI 路径 `node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs`。
2. 代码/测试/证据落盘后运行定向 `cargo test`（拒绝 0 tests/`running 0 tests`），原始 stdout/stderr/exit 存 `target/round62/<子任务>/attempt-1/`。
3. 完成后 `delegate-result <SUB_ID> "<产出+证据+风险>" --as <bingbu|xingbu|libu>`；未完成/阻塞用 `block` 上报，不调 `state`、不代父任务验收。
4. 禁 commit/push、禁跑 `ci.ps1`、禁改历史 R61 证据与两个历史冻结 JSON、禁入 `target/`、`resource/`、`.env`、大 JSON；禁新增 `#[allow]`/`#[ignore]` 旁路。
5. 文档/证据新增文件 ≤1MiB；中文断言按 i18n 棘轮（strict 60/0）口径。

## 四、执行顺序与门禁

A → B → C → D 串行（A/B 同 crate 文件，强制先 A 后 B；C 只读 A/B 后写证据；D 依赖 C 事实）。
全部完成后：Lead 唯一全量门禁（完整捕获）→ 尚书验收 → 门下确认 → 提交拆分（代码/证据/文档）→ push → 汇报。
任何失败保留原始日志并退回对应子任务；不回滚用户已有历史冻结 JSON，不覆盖 R61 证据。

## 五、风险清单

- wire 不得新增 125 字段；TargetPoint 无缓存即不生成，避免伪造目标。
- offset 与 camera/actor 偏移不得双重叠加；动态目标不得用静态单调断言误杀。
- 裁剪必须以绝对序号 + 会话/generation 证明「不丢、不重放」，禁止丢最旧冒充回收。
- 0 tests / 跳过测试即 PARTIAL，不得计绿；门禁失败保留原始双流与退出码。

## 六、代笔说明（如实留痕）

gpt-6.1-sol 尚书派发 subagent 连续两次（a89fe8fb、6d3168f6）超时/失败无产物，仅留 1,045B 骨架。派发 CLI（4 条 delegate + state Doing + show + validate，全部 exit 0）与本文由队长按尚书角色代笔执行，原始回执见 `target/round62/shangshu/01-delegate.log`；未改代码/证据/计划/审阅文档，未 commit/push。

## 七、执行中 Lead 补充裁决（2026-10-26T13:4x）

- A（`sub-e4499be7`）已完成：`projectile.rs` 1676 行（+657/−184）+ `net.rs` +20 行（`WorldSnapshot.session_generation` 非 wire + `next_session_generation()`），C1–C4 落实，红→绿证据 `target/round62/r62-a/attempt-1/{red-baseline.log,green-unit.log 18/0,green-lib.log 312/0}`。
- **裁决：`crates/mir-client/tests/round59_projectile_render.rs` 属 R59/R61 历史集成测试，其 6 个用例按旧 elapsed 语义驱动；R62 语义更替合法，由 B 改为按 R62 语义驱动并在 R62 manifest/证据中登记旧/新 SHA 与更替理由（R61 evidence 仍为 R61 提交时点的历史事实，不改）。** 不得 `#[ignore]` 或删除断言；`cursor_resets_when_the_snapshot_event_table_is_replaced` 按 C4 改为「同代号裁剪空表不复位 + 新代号首批复位」。
- A 遗留 PARTIAL 承接 R63：SkipFrames 生产来源缺口（无 ActionFeed 等价字段）、事件表物理回收无服务端协议。
- **追加裁决（B 上报 R61 冻结测试冲突）**：`crates/mir-client/tests/round61_projectile_queue.rs`（R61 manifest SHA `91389124…`）4 passed/7 failed，与 R59 同为旧 elapsed 语义冲突 → 授权 B 按 R62 语义最小改写（保留断言/测试名、禁 `#[ignore]`），并在 R62 证据中登记旧/新 SHA 与 supersede 关系；R61 证据文档不改。批准 B 在 `net.rs` 增加 `WorldSnapshot::new_session()` 公共发号构造（复用 A 的 `next_session_generation`，`initial_world_snapshot` 走它；非 wire、无新字段）。
