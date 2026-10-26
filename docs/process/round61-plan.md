# 第61轮计划：125积压不丢、动态飞行参数与逐cue离线混音证据

## 一、事实锚点（已真读；基于HEAD 04a8983）

- 太子→中书规划；本阶段只写本文件/edict，不改crates、不跑cargo/CI、不提交；后续须独立门下审核。
- HEAD实测04a8983，初始仅两历史冻结JSON未跟踪，禁止纳入本轮提交。
- 已读 docs/roadmap.md:150-200 §八/九（无§十）、R59/R60 README、docs/client.md §5.26/5.27、R60 final-manifest及五份round60流程文档。
- R60 PASS-WITH-PARTIAL：代码bb53321、文档e053905，后有审计/收口提交；manifest历史审计HEAD不是当前HEAD。门禁12/12与脚本11/11分开统计，P-1..P-6不自动清零。
- Crystal github/Crystal/Client/MirScenes/GameScene.cs:4687-4714 ObjectProjectile：source存在才处理；FireBounce每包调用出生音/CreateProjectile，目标非空挂回调，死亡不产生命中音/特效。
- Crystal github/Crystal/Client/MirObjects/Effect.cs:144-165 Missiles是List，每次构造Add；没有本作ring16的丢弃语义。
- Rust crates/mir-client/src/net.rs:7256-7273 push_projectile_event：total自增，Vec push后超过EFFECT_EVENT_LIMIT就remove(0)。
- Rust crates/mir-client/src/ui/projectile.rs:461-495 consume：绝对游标防重，但无法追回未消费即丢的事件；:94-96/:342-347另有SOUND_CUE_LIMIT=32丢最旧。
- Crystal github/Crystal/Client/MirObjects/PlayerObject.cs:3539-3567 CreateProjectile：TargetPoint回退、初始Duration=MaxDistance*50；github/Crystal/Client/MirObjects/Effect.cs:176-189每帧更新Destination、Duration、Count=max(1,Duration/Interval)。旧文档174-181已漂移。
- Rust crates/mir-client/src/ui/projectile.rs:104-122已有duration_ms/frame_count/frame_at；:515-528生成时冻结参数，:545-550每帧仅更新目标。
- Crystal github/Crystal/Client/MirObjects/Effect.cs:81-96 Process：Time<=NextFrame返回；每次至多推进一次（另有Owner.SkipFrames）；NextFrame=Start+(Duration/Count)*(CurrentFrame+1)。Rust固定elapsed/30，本轮B只纠偏参数，不宣称完整调度等价。
- Crystal github/Crystal/Client/MirSounds/SoundManager.cs:94-105每次PlaySound创建OneShotProvider入mixer，:177-190转换通道，无同文件100ms限流。
- Rust crates/mir-client/src/app.rs:194-219 play_projectile_cues调用SoundPlayer::play；crates/mir-client/src/audio.rs:589-624依次检查开关/gain/设备/频控/resolve/decoder/amplify/mixer.add。
- Rust crates/mir-client/src/audio.rs:288-293 test_sink仅计数；:615-619在真实mixer.add前返回，不能证明混音样本非零。
- MF-1真位置crates/mir-client/src/app.rs:3794测试注释含忽略属性字面量（非提示的249）；MF-2 docs/client.md:1304历史P-6/R-5应P-4/R-5。
- 沿用M34-1/2.mna与106个magic帧，不需新素材/图元；实现阶段须再验证存在与解码，不能把历史存在当本轮PASS。

## 二、目标G1..G6（可证伪）

- **G1/A**：125事件及cue不因16/32容量丢弃；仅移除projectile_events和RecordingSoundSink的丢最旧策略，Vec按需增长；其它事件ring原样，不设无Crystal依据的生产硬阈值。
- G1断言：同步前累计N=17/33/257个有效同源同目标事件，产生N个view/N个出生cue；同刻重复sync、重复drain不重播；后续命中N个cue；先消费后卡顿突增同样不漏。
- G1保留R60绝对序号、空表重建及手工total=0快照兼容。压力记录输入/保存/消费/cue数及Vec占用，257不是生产上限；长期历史增长仍R-1 PARTIAL。
- **G2/B**：advance在更新目标后、finished前每帧重算duration_ms/frame_count；Source/started_ms/dir16保持出生语义，不随目标改向。
- G2断言：source=(0,0)，目标(6,0)→(9,0)使300/10→450/15；移至(3,0)为150/5，重合0/1；同刻frame=6时远移仍飞、近移完成；命中次数与死亡分支独立验证。
- G2仅参数保真可PASS，完整NextFrame/SkipFrames时钟、失踪目标/TargetPoint及image offset仍PARTIAL，文档不得称完整对齐。
- **G3/C**：125入口使用明确无同文件频控策略（建议SoundPlayer::play_projectile_cue），内部共用解码/增益/提交逻辑；普通play/play_event/play_spell保留100ms，不新增配置开关。
- G3断言：同文件同刻2/33次cue均提交；普通play的0/99/100ms边界仍符合旧契约；disabled、sound gain=0、master=0、无设备、缺文件/坏解码不报已播。按入口决定策略，不全局豁免M34文件名。
- **G4/E**：无声卡逐M34-1/2 cue走SoundLibrary→真实decoder→amplify→真实离线rodio mixer；记录路径、输入/提交数、样本数、峰值/能量/gain。注入替换设备输出，复用生产提交逻辑。
- G4断言：单cue样本非空、有限且至少一非零；gain=0.25与1.0同段样本满足数值容差；同刻重复cue与单cue线性叠加；关闭/零增益不提交。缺素材SKIP→PARTIAL，不能用计数闭包或替代音源证明混音。
- **G5/D**：修MF-1注释与MF-2历史引用；新文档按符号重核行号，不改R60冻结证据/计数/哈希。
- **G6/F**：脚本发现数>0，执行0tests/0passed失败，SKIP/PARTIAL单列；README/manifest/树哈希绑定实际代码HEAD与唯一门禁。实听/Live仍PARTIAL。

## 三、子任务DAG（独立门下批准后尚书登记；模型/写范围互斥）

| key/owner | 精确范围与交付 | depends_on | 验收/证据 |
|---|---|---|---|
| D1 兵部/deepseek编码 | crates/mir-client/src/net.rs、src/ui/projectile.rs、tests/round61_projectile_fidelity.rs；必要时只修tests/round58_projectile_wire.rs、tests/round59_projectile_render.rs旧政策断言；G1/G2 | 无 | T1；target/round61/d1.log、d1-contract.txt |
| D2 工部/deepseek编码 | crates/mir-client/src/audio.rs、src/app.rs；G3/G4、MF-1；离线mixer注入限测试，无新依赖/运行配置 | 无；沿用Vec cue接口 | T2；target/round61/d2.log、cue-samples.txt |
| D3 礼部/gpt文档 | docs/client.md、docs/roadmap.md、docs/testing/round61/{README.md,run-checks.sh,commit-tree-hashes.txt}；G5/G6、MF-2，新§5.28/§十 | D1+D2、提交①、门禁 | 脚本/链接/diff；target/round61/checks.log |
| D4 刑部/gpt独立审计 | 只读crates；docs/testing/round61/{round61-final-audit.md,final-manifest.json}；公式/计数/哈希/PARTIAL抽核 | D3 | 静态审计；target/round61/audit.log |
| D5 尚书/gpt+Lead | 登记收交付、docs/process/round61-{shangshu-dispatch,shangshu-acceptance}.md；仅Lead提交/唯一门禁；独立门下另写round61-menxia-{review,confirm}.md | 下述拓扑 | §四；target/round61/gate/attempt-1.log |

- 拓扑：独立门下批准→D1∥D2→Lead提交①→唯一门禁→D3→D4→尚书验收/独立门下确认→Lead提交②。
- 单文件单owner；D1替换因政策/动态参数变化失效的旧断言须逐条解释，禁删测降覆盖；其它R58/R59/R60契约保持。
- D1/D2先写可失败断言并保全首次结果再修复。D2可在crate测试模块组合Projectiles cue与离线mixer，不向生产导出测试API。
- D3先发现实际测试名/计数，不能机械沿用历史11项或pass数；D4不代写实现，不自审自批。

## 四、提交与门禁（仅实现阶段执行；规划者不执行）

```bash
# T1：D1，新filter发现数必须>0
cargo test -p mir-client --test round61_projectile_fidelity
cargo test -p mir-client --lib ui::projectile -- --nocapture
cargo test -p mir-client --lib net::
cargo test -p mir-client --test round59_projectile_render
cargo test -p mir-client --test round58_projectile_wire
# T2：D2，audio同时编入bin，必须两端都跑，不只filter app::
cargo test -p mir-client --lib audio:: -- --nocapture
cargo test -p mir-client --bin mir-client -- --nocapture
cargo test -p mir-render --lib sound
cargo fmt --all --check
cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings
# 文档/证据：--list零执行、零写入
bash docs/testing/round61/run-checks.sh --list
bash docs/testing/round61/run-checks.sh
py -3 tools/docs/check_links.py
git diff --check
# 仅Lead在提交①HEAD跑唯一全量；先核锁
pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale
```

- **提交①**仅D1/D2的crates代码/测试；定向验证和交付审查通过后记完整SHA，Lead才跑唯一全量。
- 门禁前核target/soak.lock；有锁先协调持有人，禁止KillStale。TMP/TEMP/TMPDIR统一仓库target/tmp，缺目录由执行者创建。
- 保全原始退出码、实际12项结果、日志bytes/sha256/gate_head；首败不得覆盖，只许修因后attempt-2，禁止第三人复跑替代首败。
- **提交②**仅本轮docs文档/脚本/证据/流程记录；禁止历史未跟踪JSON、game-assets/resource及target产物入提交。
- manifest区分代码HEAD、gate_head、文档审计范围；crates按提交①树核，docs按提交②暂存blob核并记方法。清单自身SHA只进manifest，manifest自身不列清单，避免自哈希循环；提交后Lead回验已提交blob。
- 提交②后只静态回验文档/哈希，不复跑全量。修因若需额外代码提交，显式登记两提交偏离及原因交门下核准，不能藏改证据。

## 五、风险与PARTIAL降级

| 风险 | 处理及真实边界 |
|---|---|
| R-1 无上限历史Vec/snapshot clone成本 | 不造丢弃阈值；257压力计数/内存统计只是最低证据，非长期稳态。确认回收接口留后轮PARTIAL；若不接受须退回G1设计，不能固定扩容后声称不丢。 |
| R-2 动态Count改变命中 | 覆盖远移/近移/重合/死亡/失踪；NextFrame/SkipFrames完整调度仍PARTIAL。 |
| R-3 同帧叠音峰值 | Crystal多OneShot本有叠音；证明混音提交及线性样本，不新增限流/voice stealing/归一化；真实设备削波和主观可听性PARTIAL。 |
| R-4 素材/CWD依赖 | 测试以CARGO_MANIFEST_DIR定位库并记resolve路径；缺任一M34则G4素材部分PARTIAL，禁M34-0替代。运行时CWD可移植性P-2不宣称修复。 |
| R-5 无声卡上限 | 离线样本只证明解码/增益/混音，不证明设备或Live真听，延续P-1。 |
| R-6 无显式会话代号 | 保留空表复位/手工快照兼容；新非空会话与旧游标不能区分的已有缺口仍PARTIAL，不加线上字段。 |
| R-7 历史口径/假绿 | R60 P-3/P-4仅在R61新证据充分后标解决；P-5素材来源/P-6首败保留历史意义。0tests、SKIP、路径失败不算PASS。 |

## 六、不做清单

1. 不改服务端/125线格式，不做Hero/真socket/攻城/天气/行会，不加启发式去重或跨会话协议。
2. 不移植完整Effect/Missile类体系、NextFrame/Owner.SkipFrames、未知目标TargetPoint、image offset/光照，另轮保真。
3. 不做驱动/真人实听/Live端到端/新素材/配置面板，不改普通音效100ms，不做按文件名的全局豁免。
4. 不加依赖、不改ci.ps1，不改R59/R60冻结证据，不以历史计数充当本轮结果，不让文档代理写crates。
5. 本规划阶段不实施、不跑cargo/CI、不commit/push；仅完成审议计划与看板。

## 七、待门下裁决项（五项建议；未经独立批准不派发）

1. 批准A同时去掉16事件/32cue丢弃，接受长历史增长PARTIAL；或退回要求消费确认回收设计（扩大接口范围，禁固定扩容冒充不丢）。
2. 批准B仅修每帧duration/count，完整NextFrame/SkipFrames时钟仍PARTIAL；若要求本轮完整移植，先重估D1/回归预算，不能把elapsed/30称等价。
3. 批准125专用入口绕过频控，普通音效仍100ms；接受同帧叠音峰值，不自创限流/归一化。
4. 批准E完成线为共享生产路径的真实离线mixer样本；缺素材PARTIAL，设备/Live真听继续PARTIAL。
5. 批准两提交/唯一门禁/锁保护/首败保全及MF-1现行3794行、MF-2历史P-4引用修正；独立门下裁决，规划者不得自批。
