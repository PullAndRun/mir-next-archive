# R61 门下省独立评审

- 结论：**PASS-WITH-CHANGES / approve**；允许尚书按本裁决登记执行，M1–M6 是验收必改条件，尚未证明实现通过。
- 主任务：JJC-20261026-008；审核者：独立门下省 gpt-6.1-sol；日期：2026-10-26。
- 受审 HEAD：`04a8983ced75a290f8878ad66d478e630d36ed01`；起始工作区仅方案与两历史冻结 JSON 未跟踪。
- 受审方案：[round61-plan.md](round61-plan.md)（109 行）；先实测 SHA256=`c79a36c8a88eca5d55473b368d7148ffb76ce2bfae41607306f56d64c90814da`，与报称一致。
- 范围：只读 Crystal/crates；仅写本评审与 edict；未运行 cargo/CI，未 commit，未改方案或 crates。

## 一、事实锚点抽查（原文件真读，非转述方案）

| 组 | 真读锚点 | 事实与判定 |
|---|---|---|
| A1 | [GameScene.cs:4687](../../github/Crystal/Client/MirScenes/GameScene.cs#L4687)，至 4714 | source 不存在返回；FireBounce 每包先播出生音再 CreateProjectile；Target 非空才挂 Complete，Dead 跳过命中。方案准确。 |
| A2 | [Effect.cs:144](../../github/Crystal/Client/MirObjects/Effect.cs#L144)，至 195 | Missiles 为 List，构造 Add；185–187 才是现行 Duration/Count 重算；176 更新目标，174–181 不是完整公式。191–195 Remove 会清除 Missile；不能将活动 List 当永久历史依据。 |
| A3 | [Effect.cs:73](../../github/Crystal/Client/MirObjects/Effect.cs#L73)，至 109 | Time<=NextFrame 返回；常规一次 ++CurrentFrame，SkipFrames 可额外推进；NextFrame=Start+(Duration/Count)*(CurrentFrame+1)。elapsed/30 不等价。 |
| A4 | [PlayerObject.cs:3539](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L3539)，至 3567 | TargetPoint 回退；初始 duration=MaxDistance*50；Interval/count/Skip 构造参数可核。方案准确。 |
| A5 | [SoundManager.cs:75](../../github/Crystal/Client/MirSounds/SoundManager.cs#L75)，至 118、177–192 | 实际目录为 MirSounds；缓存命中后每次 new OneShotProvider 并 AddMixerInput，后者处理通道数；该调用链无同文件 100ms 限流。 |
| A6 | [net.rs:166](../../crates/mir-client/src/net.rs#L166)、[net.rs:7256](../../crates/mir-client/src/net.rs#L7256)，至 7273 | EFFECT_EVENT_LIMIT=16；total 饱和自增，push 后超限 remove(0)。绝对序号不能恢复已被删除的未消费事件。 |
| A7 | [projectile.rs:94](../../crates/mir-client/src/ui/projectile.rs#L94)、[projectile.rs:342](../../crates/mir-client/src/ui/projectile.rs#L342)、[projectile.rs:461](../../crates/mir-client/src/ui/projectile.rs#L461)，至 495 | cue32 丢最旧与事件16是独立瓶颈；consume 的 dropped/cursor 只防重及跳过丢失，方案准确。 |
| A8 | [projectile.rs:104](../../crates/mir-client/src/ui/projectile.rs#L104)、[projectile.rs:515](../../crates/mir-client/src/ui/projectile.rs#L515)，至 550 | duration/frame_count 仅 spawn 冻结；advance 更新 destination 后以 elapsed/30 求 frame。G2 可局部修正。 |
| A9 | [sound.rs:230](../../crates/mir-render/src/sound.rs#L230)，至 280 | FireBounce 仍取 GreatFireBall=34，Spawn/Hit 对应 M34-1/2；其它 spell 返回 None。 |
| A10 | [audio.rs:79](../../crates/mir-client/src/audio.rs#L79)、[audio.rs:589](../../crates/mir-client/src/audio.rs#L589)，至 624 | 普通 play 检查开关/gain/输出→100ms→resolve→decoder/amplify→mixer.add；当前 test_sink 在 add 前计数返回，不能证明样本。 |
| A11 | [app.rs:194](../../crates/mir-client/src/app.rs#L194)，至 219、[app.rs:3794](../../crates/mir-client/src/app.rs#L3794) | 生产 cue 仍调用普通 play；闭包只计数。MF-1 当前在3794测试文档注释，非249属性；不得误删测试。 |
| A12 | [client.md:1304](../client.md#L1304)、[R60 manifest:114](../testing/round60/final-manifest.json#L114) | MF-2 当前确为 P-6/R-5，正确引用应 P-4/R-5；manifest 的249是历史位置，保留冻结证据。 |

参照已读：[roadmap §九](../roadmap.md#L180)、[client §5.26/5.27](../client.md#L1178)、[R59 README](../testing/round59/README.md)、[R60 README](../testing/round60/README.md)、[R60 manifest](../testing/round60/final-manifest.json)。
R60 manifest 记 PASS-WITH-PARTIAL、代码 bb53321、文档 e053905；12项门禁与11项脚本分别统计。以上为历史证据记载，本评审未重跑历史门禁或复算其日志。

## 二、轻量实验（只读算术探针，非 Rust/Crystal 运行证据）

- Node 探针 exit=0；代入已读容量政策：N=17/33/257，事件保留16/16/16，cue保留17/32/32；去上限后的数学保存数为N。它不验证实际补丁或内存稳态。
- 代入现行公式：距离0/1/3/5/6/9 → duration=0/50/150/250/300/450，count=1/1/5/8/10/15。
- Crystal 首 NextFrame 相对 Start 为0/50/30/31/30/30ms，且相等时仍返回；Rust frame1阈值恒30ms。1格及5格反例直接证伪“elapsed/30完整等价”。
- 未加载声卡、未跑音频或游戏进程；本阶段样本、压力及定向回归结果均待实现者实证。

## 三、§七五项逐条裁决（D1–D5指待裁决项，不同于方案子任务编号）

| 项 | 裁决 | 附带条件及理由 |
|---|---|---|
| D1 无上限历史 Vec | **准奏，长期增长仍 PARTIAL** | 同时取消125事件与cue丢最旧，禁固定扩容冒充不丢；不扩大到其它事件ring。本轮不强制新增回收接口，但必须执行M1并写出消费确认回收的后续设计契约。Crystal List是活动集合且完成删除，不能证明历史无限保留合理。 |
| D2 仅参数修正 | **准奏** | 每帧更新目标后、finished前重算duration/count，出生source/started_ms/dir16不变。只允许“动态参数已修”；NextFrame/SkipFrames完整调度、TargetPoint、image offset等继续PARTIAL，执行M2。 |
| D3 125专用频控策略 | **准奏，峰值风险接受** | 仅125入口绕过100ms；普通play/play_event/play_spell保持原契约；不按M34文件名全局豁免。Crystal可重叠，禁止无依据新增限流、voice stealing或归一化，执行M3。 |
| D4 离线真实mixer样本 | **准奏** | 共享生产resolve/decoder/amplify/提交逻辑，注入仅替代输出；fake counter/自造替代音源不满足完成线。缺任一M34为PARTIAL；真设备削波、真人可听与Live仍PARTIAL，执行M4。 |
| D5 两提交/门禁/MF | **准奏** | Lead独占提交与唯一全量门禁；锁保护、首败不覆盖、代码/gate/docs范围及提交后blob复核按计划执行；MF按现行锚点修，不改R60冻结manifest，执行M5/M6。 |

## 四、必改 M1–M6（尚书纳入派发与验收；不要求本评审者越权写实现）

| 编号/级别 | owner及写范围 | 可执行修复与可证伪验收 |
|---|---|---|
| M1 high | 兵部/deepseek：方案D1已有net/projectile/测试范围；礼部/gpt：R61 README | 对17/33/257分别走真实push及sync/drain，覆盖先积压再消费、先消费再突增；分别核输入/保存/view/出生cue/存活命中cue=N，重复sync/drain=0新增，保留空表/total=0兼容。记录len/capacity及占用估算口径，注明非进程RSS。README须明列历史Vec与snapshot clone随总量增长、尚无回收；后续契约至少写确认游标/安全裁剪边界/会话复位/落后消费者不丢不重，禁把257当生产上限或声称长期稳态。 |
| M2 med | 兵部/deepseek：projectile及R61测试；礼部/gpt：client/roadmap/R61 README | 远移450/15、近移150/5、重合0/1，按frame与finished逐项断言；同刻远移不完成、近移只命中一次，dead/失踪继续独立覆盖。文档补1格50ms与5格31ms调度反例；禁止“每30ms等同Crystal”及将G2 PASS扩大为时钟等价。 |
| M3 high | 工部/deepseek：audio/app及内联测试 | app生产入口必须实际选择125专用策略；同文件同刻2/33次全部提交。普通入口0/99/100ms边界保持；交错125与普通M34调用验证入口策略而非文件豁免，125不污染普通频控状态。disabled、sound/master零、无输出、缺文件、坏解码返回未提交。 |
| M4 high | 工部/deepseek：audio/app内联测试；刑部/gpt只读复核及R61审计/manifest | 至少一条真实Projectiles出生/命中→take cues→生产播放策略→共享decoder/amplify→真实离线rodio mixer证据链，不仅手造文件列表。两M34各给路径、格式/通道/采样率、输入/提交/采样数、峰值/能量/gain；同格式同起点新mixer有限拉取，非空/有限/非零。明确容差值并断言0.25对1.0比例、N重叠对单cue线性关系；移除amplify或少提交一cue应失败。关闭/零gain无提交；禁止仅返回测试counter成功。 |
| M5 med | 尚书/gpt：派发/验收；Lead：提交与门禁；礼部/刑部：R61脚本/清单/manifest各自范围 | 派发保留deepseek编码、gpt文档/独立审计、Lead唯一全量分工；单文件单owner。提交①后核soak锁及TMP/TEMP/TMPDIR，锁在先协调不KillStale。首败原退出码/原日志/sha保全，修因另attempt；不许第三人替跑。代码SHA=gate_head，提交②仅docs，blob清单排除自身循环并提交后回验；额外代码提交先登记偏离。脚本按实际测试发现数>0、0tests/0passed失败、SKIP/PARTIAL非PASS，不复用历史总数。 |
| M6 low | 工部/deepseek：app测试注释；礼部/gpt：client §5.27/R61 README | MF-1只改3794注释措辞避免忽略属性字面量，实际忽略属性新增仍0；MF-2把1304历史引用改P-4/R-5。旧段以历史时点保留，R61新增现行覆盖说明；不改R59/R60冻结结果/计数/哈希。审计按符号重定位，交付diff应可逐项核。 |

## 五、残余风险与放行边界

- 已接受：R-1历史增长/clone成本、无会话代号歧义；本轮不靠删除未确认事件解决内存问题。
- 保留：完整NextFrame/SkipFrames、目标缺失/TargetPoint、图像offset、Hero/socket及素材来源/运行CWD可移植性；不得自动清零R60 P-1..P-6。
- 接受同帧叠音峰值；离线线性样本可能超1，不得为“峰值好看”夹带归一化；设备削波及主观音量仍PARTIAL。
- 缺素材、0tests或未实跑均不得计PASS。M项缺实证时不得收口；本结论只批准实施方案，不替代尚书验收/独立门下终确认。

## 六、看板交接

- memo已记录本裁决/M1–M6与PARTIAL；主任务已由Menxia→Assigned（CLI exit=0）。
- menxia→shangshu已委派：`SUB_TASK_ID=JJC-20261026-008-sub-16fb7ccb`（depth=1，exit=0）。
- `validate --as menxia`：exit=0，**177条任务符合schema v3.1**；受审时176条，新增本次尚书子任务1条。
- 本阶段没有登记实现通过或触发门禁；Lead接手尚书执行与后续独立验收。
