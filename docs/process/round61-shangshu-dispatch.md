# R61 尚书省执行派发

## 一、依据、职责与执行波次
- 主单 JJC-20261026-008；尚书子单 JJC-20261026-008-sub-16fb7ccb；基线 04a8983ced75a290f8878ad66d478e630d36ed01。
- 已实读 round61-plan.md §三/四/五及 round61-menxia-review.md M1–M6；门下 PASS-WITH-CHANGES 仅准实施，不代表实现通过。
- 波次①：D1兵部/deepseek（net/projectile/测试）与D2工部/deepseek（audio/app/测试）可并行编辑；cargo及fmt/clippy由Lead统一排队串行执行，六部不得自行抢跑。
- 波次②：Lead审查D1/D2和定向证据→Lead代码提交①→Lead在该代码HEAD执行唯一全量门禁；首败保全后修因另attempt。
- 波次③：门禁完成且可交付后D3礼部/gpt写文档、检查脚本与树哈希→D4刑部/gpt只读审计实现并写审计/manifest。
- 波次④：尚书/gpt逐项验收→独立门下确认→Lead文档提交②→Lead静态回验已提交blob；提交②后不复跑全量。
- 单文件单owner；模型边界：deepseek只编码/测试，gpt只文档/审计/验收；Lead独占提交与门禁。本派发者只写本文件和看板，不启动更深代理；Lead保存子单↔执行agent映射。

## 二、四个执行子单契约
| 子单/依赖 | 唯一写范围 | 验收及证据 | 禁止与PARTIAL边界 |
|---|---|---|---|
| D1 兵部/deepseek；无依赖 | crates/mir-client/src/net.rs、crates/mir-client/src/ui/projectile.rs、crates/mir-client/tests/round61_projectile_fidelity.rs；必要时仅调整crates/mir-client/tests/round58_projectile_wire.rs、crates/mir-client/tests/round59_projectile_render.rs的旧政策断言 | T1及M1/M2；target/round61/d1.log、d1-contract.txt；先写可失败断言，由Lead保全首次结果后修复；每条旧断言替换解释理由 | 不删测降覆盖、不改其它ring/协议/生产阈值；长期增长、无会话代号、完整时钟/TargetPoint/image offset继续PARTIAL |
| D2 工部/deepseek；无依赖，沿用Vec cue接口 | crates/mir-client/src/audio.rs、crates/mir-client/src/app.rs及各自内联测试；G3/G4与MF-1 | T2及M3/M4/M6；target/round61/d2.log、cue-samples.txt；先断言再修复；必须包含Projectiles真实出生/命中到离线mixer链 | 不加依赖/配置、不导出生产测试API；仅测试注入替代设备输出；缺任一M34素材为PARTIAL，真听/设备/Live仍PARTIAL |
| D3 礼部/gpt；依赖D1+D2、提交①和门禁 | docs/client.md、docs/roadmap.md、docs/testing/round61/README.md、docs/testing/round61/run-checks.sh、docs/testing/round61/commit-tree-hashes.txt | T3及M1/M2/M5/M6；新增§5.28/§十，修MF-2；target/round61/checks.log；实际发现测试名/计数并绑定代码HEAD/gate_head | 只文档/脚本，不写crates、不运行全量；不把参数修正称完整时钟等价、不沿用历史pass总数；保留既有PARTIAL |
| D4 刑部/gpt；依赖D3 | crates只读；仅docs/testing/round61/round61-final-audit.md、docs/testing/round61/final-manifest.json | T4及M4/M5/M6；target/round61/audit.log；独立抽核公式、计数、样本、哈希、PARTIAL及逐项M证据 | 不代写实现、不自审自批、不代跑全量；缺证据退回对应owner，不以计数器或口头成功代验收 |
- target/round61/**仅各owner对应证据文件可写，门禁目录由Lead独占；证据不进入提交。
- 四执行者先读CLI recall/角色简报，登记progress/todo；每单最多一个in-progress；实际交付和定向验证齐备才delegate-result，未验证须明记。

## 三、M1–M6逐条落点与拒收条件
| 必改 | 写范围/owner | 可证伪验收与证据 |
|---|---|---|
| M1 high | D1 net/projectile/R61测试；D3 README | 17/33/257分别真实push→sync/drain，先积压再消费、先消费再突增两路径；输入/保存/view/出生cue/存活命中cue=N；重复sync/drain新增0，空表/total=0兼容。d1-contract.txt逐组给len/capacity、占用估算方法并注明非RSS。README写历史Vec/snapshot clone增长且未回收，后续契约含确认游标、安全裁剪边界、会话复位、落后消费者不丢不重；257非上限/非稳态证据。 |
| M2 med | D1 projectile/R61测试；D3 client/roadmap/README | 更新目标后、finished前每帧重算，source/started_ms/dir16不变；6→9格450/15、3格150/5、重合0/1，逐项frame/finished；同刻frame=6远移不完成、近移命中仅一次，dead/失踪独立测。d1.log/d1-contract.txt；文档给1格50ms、5格31ms反例，禁止elapsed/30等同Crystal。 |
| M3 high | D2 audio/app及内联测试 | app实际调用125专用入口；同文件同刻2/33次均提交；普通play/play_event/play_spell仍100ms，0/99/100ms边界；125与普通M34交错验证125不污染普通频控。disabled、sound/master零、无输出、缺文件、坏解码均未提交。d2.log/cue-samples.txt须可核实际提交，非文件名全局豁免。 |
| M4 high | D2 audio/app测试；D4审计/manifest | 至少一条Projectiles出生/命中→take cues→生产策略→共享SoundLibrary resolve/真实decoder/amplify/真实离线rodio mixer。M34-1/2各记路径、格式/通道/采样率、输入/提交/采样数、峰值/能量/gain；同格式同起点新mixer有限拉取，非空/有限/非零；明确数值容差并断言0.25/1.0及N叠加/单cue线性，移除amplify或少cue应失败；关闭/零gain无提交。cue-samples.txt及audit.log；禁止fake counter/替代音源证明。 |
| M5 med | 本派发/后续尚书验收；Lead提交/门禁；D3脚本/树清单、D4 manifest | 按四波次/模型/owner分离；代码SHA=gate_head；锁/TMP/首败/attempt原件按§五。脚本发现数>0且0tests/0passed失败、SKIP/PARTIAL单列不算PASS。d1/d2/checks/audit/gate证据及提交后blob回验全部可复查；不得机械沿用R60的12/11。 |
| M6 low | D2 app测试注释；D3 client §5.27/README；D4重定位复核 | MF-1按符号重定位当前3794注释，只改措辞避免忽略属性字面量，实际忽略属性新增0；MF-2当前1304历史引用改P-4/R-5。历史时点保留，R61补现行覆盖说明；逐项diff证据，不改R59/R60冻结结果/计数/哈希。 |

## 四、验收命令与交付判定
- 以下为交付契约，派发阶段均未运行；T1/T2 cargo由Lead持有执行时隙并保存真实退出码/原件，D1/D2提供测试与解释，不并发构建。
```bash
# T1
cargo test -p mir-client --test round61_projectile_fidelity
cargo test -p mir-client --lib ui::projectile -- --nocapture
cargo test -p mir-client --lib net::
cargo test -p mir-client --test round59_projectile_render
cargo test -p mir-client --test round58_projectile_wire
# T2（audio在lib/bin均编译，不可只filter app::）
cargo test -p mir-client --lib audio:: -- --nocapture
cargo test -p mir-client --bin mir-client -- --nocapture
cargo test -p mir-render --lib sound
cargo fmt --all --check
cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings
# T3：--list须零执行/零写入
bash docs/testing/round61/run-checks.sh --list
bash docs/testing/round61/run-checks.sh
py -3 tools/docs/check_links.py
git diff --check
# Lead唯一全量（锁核准、提交①之后）
pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale
```
- T4：D4只读git diff/show/ls-tree及文件/日志哈希，按符号核源码，逐项核M1–M6/manifest/清单；不执行cargo/CI，不替代T1/T2或全量。
- 尚书拒收空结果、0tests/0passed、缺日志/退出码/缺SHA及未解释SKIP；准奏、数学探针、历史存在均非本轮实现PASS。素材须本轮验证存在和解码。
- 后续尚书验收写docs/process/round61-shangshu-acceptance.md；核父/子真实show、todos全completed、有效delegation_result和证据齐备，含尚书子单收交付；done进Review后验收，请求Done进入PendingConfirm，由真实confirm_by=menxia独立确认，禁止代确认。

## 五、门禁、两提交与证据卫生
- 提交①仅D1/D2 crates代码/测试；Lead先定向验证/交付审查，再记完整代码SHA；唯一全量须在该HEAD，代码SHA=gate_head。
- Lead先查target/soak.lock；有锁先协调持有人，锁未获准不得KillStale。TMP/TEMP/TMPDIR全部设仓库target/tmp，缺目录由执行者创建。
- target/round61/gate/attempt-1.log原件及原始退出码、实际12项结果、bytes、sha256、gate_head保全；首败不覆盖，修因后独立attempt-2，禁第三人复跑替代首败。
- 额外代码修因提交须先显式登记两提交偏离及原因并交门下核准；重试绑定实际HEAD，禁止篡改原件使旧门禁看似覆盖新代码。
- 提交②仅本轮docs文档/脚本/流程记录；target证据仅引用；历史未跟踪JSON、game-assets/resource全部排除；不动R59/R60冻结证据。
- crates按提交①树核；docs按提交②暂存blob核并记方法、范围；commit-tree-hashes.txt自身SHA仅入manifest，manifest自身不列清单，避免循环。
- manifest分别记代码HEAD、gate_head、文档审计范围、每attempt与SKIP/PARTIAL；Lead提交②后回验已提交blob，仅静态校验，不复跑全量。

## 六、黑名单、残余风险与交接
- 禁止改服务端/125线格式、其它事件ring、Hero/socket/攻城/天气/行会、启发式去重/会话协议；不移植完整Effect/Missile、NextFrame/SkipFrames、TargetPoint/image offset/光照。
- 禁止新依赖、运行配置/面板、新素材、普通音效频控变化、按文件名豁免、voice stealing/限流/归一化；不改ci.ps1，不删测降覆盖，不让gpt写crates。
- 长历史Vec及snapshot clone成本、无回收/会话歧义继续PARTIAL；同帧叠音峰值接受，离线样本可超1，真设备削波/主观音量/Live继续PARTIAL。
- 素材以CARGO_MANIFEST_DIR定位并记resolve路径；缺任一M34只可SKIP→PARTIAL，禁用M34-0替代；CWD可移植性、素材来源、R60首败仍保留历史意义，不自动清零P-1..P-6。
- 登记结果：D1=JJC-20261026-008-sub-7e992ec6（bingbu）；D2=JJC-20261026-008-sub-c7281e94（gongbu）；D3=JJC-20261026-008-sub-d0668f24（libu）；D4=JJC-20261026-008-sub-840540ca（xingbu），均depth=1、CLI exit=0。
- memo已记录；主单Assigned→Doing exit=0；validate --as shangshu exit=0，181条任务符合schema v3.1。由Lead在可用深度启动执行者并记录映射，D3/D4必须等依赖满足。
