# R62 门下省独立审核
verdict：**PASS-WITH-CHANGES**。仅附条件准奏；尚书须在启动 A 前核清 C1–C5，未满足不得把对应 M 项判绿。
审核对象：[中书计划](round62-plan.md)，主任务 `JJC-20261026-009`；本审不改实现、不运行 cargo/CI、不 commit/push。

## 六项审核
| 项 | 结论 | 原件核验与限制 |
|---|---|---|
| 1 Crystal 锚点 | FAIL | 行号真实，但 M1 的 SkipFrames 描述不等价，须 C1；其余出生/空目标/死亡/570 分支相符。 |
| 2 M1–M8 可证伪 | FAIL | M1/M3/M4 的判据须收紧；M2/M5/M6/M7 可构造真实行为测试，M8 属静态验收而非 Rust 红绿。 |
| 3 依赖与写范围 | PASS | 计划明确 A→B→C→D，A/B 同文件强制串行；C 只读实现，新增测试权限须 C4 明确。 |
| 4 验收可执行性 | FAIL | 包/入口存在，命令可落地，但测试过滤器和非零红证据未锁定，须 C4/C5；全量门禁仅 Lead。 |
| 5 风险/回滚/授权 | PASS | 仅客户端、保留失败日志/旧证据；无删除/协议破坏授权；执行需落实 C2/C3 的回退与复位边界。 |
| 6 Crystal 对照边界 | PASS | 不改服务端/协议/渲染后端；绝对序号/generation 只是客户端运输去重，不得宣称 Crystal 新机制。 |

## 原件抽核
- [Effect.cs:81–100](../../github/Crystal/Client/MirObjects/Effect.cs#L81)：`Time <= NextFrame` 返回；到期先按 `Owner.SkipFrames` 额外加一，再 `++CurrentFrame`；不是按 elapsed 追赶所有已过边界。
- [Effect.cs:176–189](../../github/Crystal/Client/MirObjects/Effect.cs#L176)：先取目标位置，再逐次重算 `Duration=MaxDistance*50`、`Count=Duration/Interval`、零值归一，最后调用基类。
- [GameScene.cs:4697–4710](../../github/Crystal/Client/MirScenes/GameScene.cs#L4697)：出生音先于 CreateProjectile；只有非空 Target 挂 Complete，死亡早退，570/命中音同分支。
- [PlayerObject.cs:3546–3556](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L3546)：TargetPoint 来自源对象缓存；存在目标则覆盖，不是 125 包带点。
- [ServerPackets.cs:3564–3578](../../github/Crystal/Shared/ServerPackets.cs#L3564)：125 仅 Spell/Source/Destination；[net.rs:5399–5414](../../crates/mir-client/src/net.rs#L5399) 保留 FireBounce/source gate。
- [projectile.rs:500–529](../../crates/mir-client/src/ui/projectile.rs#L500) 当前空表/长度重建猜测会与裁剪混淆；[ui/mod.rs:1573](../../crates/mir-client/src/ui/mod.rs#L1573) 为 sync 接线，cue 由 take 路径取走。
- [R61 终审:45–56](../testing/round61/round61-final-audit.md#L45) 的 P01–P12 真读；本轮不将音频设备、105 sfx、HiddenSpanSource、R60 P-1..P-6 自动清零。

## M 表逐条结论
| 项 | 可证伪判断与复核标准 |
|---|---|
| M1 FAIL | C1：以输入时刻/SkipFrames 和输出帧序列对照 Crystal；等号不进，普通到期 +1、skip 到期 +2；含大时间跳跃、同刻重复调用及真实 Owner 条件。 |
| M2 PASS | 远/近/重合固定输入表，观测位置、移除、570 与 cue 数；零距离出生时刻不重复到达，越界后最多一次，重复 sync 不新增命中。不得只断言内部字段。 |
| M3 FAIL | C2：已知目标用当前点；未知目标有可信客户端缓存才回退，无点则不生成、不造实体/坐标；未知或死目标零命中，目标重现不得给旧事件补播。 |
| M4 FAIL | C3：静态目标限定每轴朝终点单调、无越界，残差终点为零；负向/斜向/零距均测。动态目标不保证全局单调，须明确 retarget 样本和允许变化。 |
| M5 PASS | 输入序号跨裁剪边界、消费落后/领先/重复确认/积压，外部生成与 cue 顺序恰一次；未确认保留，不得用丢最旧冒充回收。 |
| M6 PASS | 同 generation 空队列（已裁剪）不得复位；新 generation 即使首次非空且 total≥旧游标也只复位一次；旧 view/hit/cue 清理，新事件恰消费一次，无重播。 |
| M7 PASS | 真实 packet→snapshot→sync 断言 FireBounce/source gate、未知目标与非 FireBounce，不能仅测手工事件构造。 |
| M8 PASS | 文档链接、diff、逐文件大小和入库禁区由静态工具判定；中文/i18n 由现有门禁复核，不伪称 Rust 行为测试。 |

## 条件、责任与交接
- C1（中书修订、执行者 A 落实）：修改 M1 的时间追赶措辞，补 SkipFrames 来源/普通与 skip 对照；尚书核对固定预期表后开工，不能为满足测试发明新时钟。
- C2（中书、A/B）：删除“事件携带 TargetPoint”的协议暗示，指定客户端缓存来源和缺失策略；新字段仅限本地事件，禁止改 wire 包；复核 packet 不带点且未知目标无伪造。
- C3（中书、A/B）：明确像素 offset 是运动量，不是 ImageInfo offset 缺口修复；核对屏幕坐标=格投影+运动偏移，camera/actor 偏移仅一次；补动态目标、裁剪空表和 generation 测试表。
- C4（尚书、A/B/C）：C 默认仅运行/审阅 A/B 测试；需新测试时先登记精确测试路径/所有者并移交，不能借 C 写实现；D 仍等 C 事实。锁定 `cargo test -p mir-client --lib ui::projectile::tests:: -- --nocapture` 及具体 net/集成过滤器，禁止 0 tests。
- C5（中书、执行者、Lead）：红证据必须有实际断言失败/非零退出并保留 stdout/stderr；缺测试说明不能代红。绿测试必须同断言且有正例计数。链接命令 `py -3 tools/docs/check_links.py`、`git diff --check` 可用；新增每文件 ≤1MiB，禁止把 target/resource/.env/大冻结 JSON 入库。
- 全量门禁仅 Lead 执行 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，完整捕获双流/退出码；失败按 A/B 原子补丁返工，保留原始日志，不覆盖用户改动/R61/冻结 JSON，不作破坏性回滚。

## 身份与复核记录
- 计划实测 8157 B；SHA256 `dc8e62ee6c76a73513321c273df66d11ed11f9da2ce2a393637044859ca1152f`，与委派一致；76 个 LF、77 逻辑行（末行无换行），不是内容篡改。
- 初始 CLI show：ID 匹配、状态 Menxia、标题实际“第62轮：飞行时钟与回收契约对齐”，与委派简称不同；create 原件确认同 ID。181→182 数量仅按 CLI stats/已有创建证据核验，不推断历史总数。
- 本文件先落盘，再执行 memo/state/show/validate；原始 stdout/stderr 与 exit 见本地 `target/round62/menxia/01-cli.log`。未执行的 Rust/门禁验证不宣称通过。
