# R62 中书规划：飞行时钟与回收契约对齐

## 一、决策与边界

本轮在 A 音频完整性、B 飞行时钟完整性、C 会话/素材边界三候选中选 **B**。

决策理由：Crystal `Client/MirObjects/Effect.cs:81-100` 的 `NextFrame` 是状态式严格时钟，`:185-187` 每帧以当前目标重算 `Duration/Count`；现有 `crates/mir-client/src/ui/projectile.rs:125-140` 仅以 elapsed 算帧号，且 `:37-40` 明确把状态机、`SkipFrames` 记为 PARTIAL。现有 `:567-616` 已有目标刷新与到达删除，因此 R62 能在单轮内补齐语义而不重做渲染。`crates/mir-client/src/net.rs:2759-2776` 的 append-only 表和 `projectile_events_total` 以及 `projectile.rs:483-529` 的消费游标形成可测的回收边界。B 不依赖设备、音频后端或素材齐备，所有主验收可用确定性 Rust 测试证伪。

不选 A：真实设备/Live 受环境限制，削波与 105 sfx 全量验证会留下不可避免的 PARTIAL，且 `HiddenSpanSource` 涉及跨 crate 播放链路，单轮风险过大。不选 C：范围可控但用户可感知弱，且 R60 P-1..P-6 会扩散至渲染/效果完整性，不如 B 直接收敛 R59–R61 时钟边界。

范围仅限投射物时钟、目标点/像素增量、事件游标回收与客户端接线/证据；不扩展到音频设备、全局音频源、会话协议改造或其它特效。

## 二、事实锚点

- `docs/testing/round61/README.md:17`：Crystal 锚点为 `Effect.cs:81-100,185-187`；`:24` 明确完整 NextFrame/SkipFrames、TargetPoint、像素 offset 仍 PARTIAL；`:38-40` 明确 append-only 回收、会话游标歧义仍待后续。
- `docs/testing/round61/round61-final-audit.md:43-55`：P01–P04 分别为回收、完整状态式时钟、TargetPoint、像素 offset 增量缺口。
- `docs/process/round61-shangshu-acceptance.md:36-48`：R62 承接清单 P01–P04；`:50` 说明本轮不应假称代码/门禁已重新验证。
- `crates/mir-client/src/ui/projectile.rs:5-17`：当前事件消费、duration/count、offset 的意图与公式；`:31-42` 登记状态机和 ImageInfo offset 偏离；`:46-66` 登记 append-only、重建游标和音频边界。
- `crates/mir-client/src/ui/projectile.rs:393-529`：`Projectiles` 的 `consumed`、cue 队列、`sync`、`consume`、重建/丢弃兼容逻辑；`:567-616` 当前动态目标与到达删除流程。
- `crates/mir-client/src/net.rs:2759-2776`：`WorldSnapshot::projectile_events` 与 `projectile_events_total` 的生产契约；`:5399-5414`：`S.ObjectProjectile` 仅接受 `FireBounce` 且源对象必须存在。
- Crystal `github/Crystal/Client/MirScenes/GameScene.cs:4697-4709`：FireBounce 出生、目标为空/死亡分支、570 命中特效与音效时机；`github/Crystal/Client/MirObjects/Effect.cs:81-100,185-187`：状态式帧推进。

## 三、目标与可测点

### 目标

- G1：实现与 Crystal 对齐的严格 `NextFrame`/`SkipFrames` 时钟，保留动态目标每帧重算。
- G2：实现 `TargetPoint` 兜底和目标消失/重建边界，不伪造目标实体，不重复命中表现。
- G3：实现像素 offset 的增量运动语义，明确格坐标、像素坐标和终点夹取，保持渲染无跳变。
- G4：建立事件/cue 的确认、裁剪、会话复位契约，并以客户端接线证据证明 125 不丢、不重放、可回收。

### M 表

| ID | 可测断言 | 失败即否决 |
|---|---|---|
| M1 | 同一 duration/count 下，`now == NextFrame` 保持旧帧，`now > NextFrame` 恰进下一帧；SkipFrames 只跨越已过边界 | 是 |
| M2 | 目标远移/近移/重合时先取 TargetPoint，再重算 Duration/Count；零距离只产生一次到达 | 是 |
| M3 | 有目标时使用当前目标点；目标未知时使用事件携带的 TargetPoint，命中效果仅在目标仍存活时生成 | 是 |
| M4 | 每个采样时刻像素 offset 单调、终点精确归零/到达，不出现一帧回退或越界 | 是 |
| M5 | append-only 队列在确认消费后可安全裁剪；裁剪边界前后绝对序号单调，未确认事件不丢 | 是 |
| M6 | 空快照/会话代号变化只复位一次；重建快照中已确认事件不重放，新增事件恰消费一次 | 是 |
| M7 | `S.ObjectProjectile` 生产接线与视图消费链保留 FireBounce/source gate；未知目标不 panic、不伪造实体 | 是 |
| M8 | 新增/更新文档、证据和脚本可被链接、差异、大小及中文/i18n 检查复核 | 是 |

## 四、子任务 DAG

A（兵部）：时钟与回收语义。范围：`crates/mir-client/src/ui/projectile.rs` 及必要的 `net.rs` 数据契约；先写红测，再实现 NextFrame/SkipFrames、TargetPoint、增量 offset、确认裁剪/复位。不得改 `mir-render` 或音频后端。产出代码、单测、偏离说明。依赖：无。

B（兵部）：客户端接线。范围：`crates/mir-client/src/net.rs`、`crates/mir-client/src/ui/mod.rs` 相关最小接线；核对 125 FireBounce、WorldSnapshot 游标、消费与绘制调用，补集成断言。依赖 A 的数据契约；与 A 串行，避免同文件冲突。

C（刑部）：测试与证据。范围：新增/维护 `target/round62/` 证据、红→绿测试记录、Crystal 对照表、PARTIAL 诚实性检查；只读审阅 A/B 代码，不改实现文件。依赖 A、B。

D（礼部）：文档与脚本。范围：R62 README/验收材料、必要的测试脚本与链接清单；保留 A 方向未做项和环境 SKIP 口径，不改历史 R61 证据。依赖 C 的事实结果。

尚书登记后执行顺序：A → B → C → D；A/B 不并行写同一 crate 文件。每个子任务须有真实 CLI delegate、todo、delegate-result 和可复查证据。

## 五、验收命令（红→绿）

1. 红证据：仅运行与投射物相关的定向测试列表/测试命令，先记录缺失断言或失败输出；不得把红证据当门禁通过。
2. 绿证据：由刑部按 A/B 产出运行定向 `cargo test`（具体包/过滤器以实现后的测试名为准），记录 stdout、stderr、退出码、提交前后差异；本规划阶段不运行 cargo。
3. 文档证据：`py -3 tools/docs/check_links.py`；`git diff --check`；检查新增文件 ≤1 MiB，且不含 `target/`、`resource/`、`.env` 或大冻结 JSON。
4. 门禁由 Lead 按既定流程执行：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，必须完整捕获 stdout/stderr 与 exit；R62 计划阶段不代跑。
5. 验收不得将真实设备音频、105 sfx 全量、HiddenSpanSource 全局化、CWD/素材边界或 R60 P-1..P-6 写成已解决；这些属于 R62 不做/后续 PARTIAL。

## 六、提交、门禁与回滚

尚书仅在所有子任务结果、定向测试、文档证据齐全后进入 Review；门下独立复核 Crystal 对标、M1–M8 和风险，再由指定 confirm_by 完成确认。提交按现有 R62 约定拆分代码、测试证据、文档；不得在本规划阶段 commit/push。任何测试/门禁失败保留原始日志，退回对应子任务修复；不回滚用户已有历史冻结 JSON，不覆盖 R61 证据。

## 七、风险与不做清单

风险：TargetPoint 需要协议字段或事件扩展，若服务端包没有点坐标则只能保留明确的客户端兼容回退；回收裁剪若无稳定确认边界会导致重复/丢失，必须以绝对序号和会话代号（或等价 generation）证明；动态目标在边界时可能改变 count，需固定采样表；像素 offset 需避免与现有 camera/actor offset 双重叠加。

不做：A 的真实设备/Live 音频、33 cue peak 治理、105 sfx 全量、HiddenSpanSource 全局化；C 的 CWD/素材来源边界、无会话代号历史游标语义、缺素材 SKIP→PARTIAL runner 重构、R60 P-1..P-6 渲染/效果完整性；不引入 Crystal 未有的网络协议、随机补点、占位素材或新特效体系。

## 八、计划状态

本文件是 R62 中书规划，事实锚点来自 R61 已审材料与当前源码行号。CLI create/state 原始输出位于 `target/round62/zhongshu/01-create.stdout.txt`、`02-state-zhongshu.stdout.txt`；后续 validate 输出另存同目录。