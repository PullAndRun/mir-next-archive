# R65 尚书派发：单UI消费生命周期与clone快照契约

- 主任务 `JJC-20261027-002`：门下 PASS-WITH-CHANGES，现 `Doing`；基线 `2deacee47efc9e505d034fe87e58692dfeb50187`；本件仅派发，不是实现PASS。
- 准入依据：[计划](round65-plan.md)108行/13792B/SHA256 `d5906954567a9fc3012258ab1b5a0d85b009886ad91bfea17876322155841fe5`；[门下](round65-menxia-review.md)49行/7525B/`23ef001944a902e20efcb27eecc525a381a973643d15530d51aeb80bbeb9f243`；C逐条核销C1–C8、M1–M6。
- DAG严格串行：A→B→C→Lead功能提交及唯一门禁→D；CLI无depends_on参数，依赖存于instruction/return_spec及本件；子单默认Doing不代表依赖满足，不得并行越过前项。

|项/任务ID|部门/唯一owner建议|启动依赖|精确write scope白名单|
|---|---|---|---|
|主 `JJC-20261027-002`|尚书gpt派发；Lead协调|门下准入|本尚书仅 `docs/process/round65-shangshu-dispatch.md`、`target/round65/shangshu/**`；看板只经CLI|
|A `JJC-20261027-002-sub-d8948266`|兵部 bingbu / deepseek编码，唯一net/app owner|门下已准入，Lead启动|仅 `crates/mir-client/src/net.rs`、`crates/mir-client/src/app.rs` 的测试/注释及最小本地契约；`target/round65/a/**`|
|B `JJC-20261027-002-sub-4da2d45a`|工部 gongbu / deepseek编码|A有效回执|仅新 `crates/mir-client/tests/round65_projectile_lifecycle.rs`；`target/round65/b/**`|
|C `JJC-20261027-002-sub-8c780054`|刑部 xingbu / gpt独立核验|A、B有效回执|仅 `docs/testing/round65/round65-c-evidence.md`、`target/round65/c/**`；源码/旧证据只读|
|D `JJC-20261027-002-sub-ce003bd2`|礼部 libu / gpt文档|C必改0＋Lead门禁事实|仅 `docs/testing/round65/README.md`、`docs/testing/round65/round65-contract.md`、`docs/testing/round65/final-manifest.json`|

- M1：一文件一owner；A默认不改生产逻辑，发现真实缺陷先停报门下审批最小修复；B不改旧测试/ui/协议；C不修实现、不代跑门禁；D不扩文档白名单。执行成员均不commit/push。
- 代笔边界：deepseek只编码，gpt决策/文档/独立审查；本尚书未替A/B实现、未启动成员（当前委派深度无余量），由Lead按DAG启动并记录子单ID↔真实agent ID；未启动时不得伪填映射或回执。
- 每成员先recall/progress，todo同刻最多1个in-progress；仅交可复查产出/证据/风险，再由真实承接角色delegate-result；尚书不代造结果、不代门下confirm。

- C1/G1.1：同代e1/e2发布S2(total2,len2)，权威继续e3/e4到S4；UI只sync已发布S2（水位2），ack2交S4得(reclaimed,len,total)=(2,2,4)，push e5得(2,3,5)，后缀消费e3/e4/e5后ack5得(5,0,5)。
- B必须真实125编码/解码→apply_packet_at→snapshot→UiState::sync；用五个可区别事件身份，断言e1→e2→e3→e4→e5顺序、累计出生cue恰5、重复sync/drain新增0；冻结时刻/目标距离，隔离命中cue；不得把单次sync表虚称半消费。
- C3/G1.2：裁前clone S4四事件逐项不变；权威ack2后clone仍len4/total4，内容/顺序不变；clone不注册消费者、不钉住权威回收；独立UI读旧clone仅离线对照，同UI重复不重播，不证明共享历史安全重放。
- C3/M2：水位是已消费条数（1-based，含本条），0-based events[i]对应事件序号 `reclaimed+i+1`；`reclaimed+i`只可称零基偏移。total饱和只证不溢出，不证明恢复/唯一序号/全局安全。
- C7/G1.3：重复ack/后退ack=no-op/no-op，越尾夹尾，total不减；旧代不裁新代；新代空首批延迟复位、首批非空仅复位一次；views/570/cues/cache清理和绝对水位须行为断言，不只字段。
- C4/G1.4 app：快照同步后仅严格前进的非零水位生成ack；next_projectile_ack在app宿主至少1个r65_命中；保留真实TCP AckProjectileEvents零字节＋Turn有字节正对照；ack经InGame gate后、send前返回。
- C4独立入口边界：`crates/mir-client/src/net.rs:3881 wait_for_snapshot`与app接收同一Receiver；predicate不满足继续等、满足返回且不自动ack；不登记最低水位成员，禁称全仓仅一个Snapshot接收点。
- wire锁定零变更；B凡声称wire的125/Monster/Remove种子均须真实编解码，否则逐项披露直接构造，不能混称全wire覆盖。

- C2限定批准：A只可在 `target/round65/a/`、B只可在 `target/round65/b/` 的隔离源码副本修改 `trim_projectile_events_through` 为no-op、`next_projectile_ack` 返回None、或trim保持移除数量但删错前缀；仅上述函数体，严禁覆盖生产树，记录副本/补丁SHA及diff。
- C6有效红：no-op裁剪触发(2,2,4)预期assert失败，None触发严格前进应Some断言失败，错前缀触发身份/顺序断言；须非零原始EXIT、测试名/行号/左右值；编译错、0 tests、旧失败、任意panic/空assert不算红，负控目标仍绿即不合格。
- C6/M2提取方案：为每个net/app内嵌目标保存完整测试块（全部属性＋函数至匹配闭括号；需要时取整个cfg(test)模块）；红/绿从原始字节提取，不规范化换行，保存宿主路径/测试名/起止字节/行范围及提取脚本，C独立核完整边界及diff。
- C6身份映射：内嵌完整提取块红绿SHA必须相同，整宿主文件红/绿分别记SHA（桩改变宿主不能称全文件同SHA）；B独立测试文件红绿全字节SHA相同；不改断言迁就负控，无有效红则PARTIAL。
- C8证据：A/B主证据根分别 `target/round65/a/attempt-1/`、`target/round65/b/attempt-1/`；A/B/C各用独立 `target/round65/<owner>/cargo-target` 与其证据根内tmp，TMP/TEMP/TMPDIR显式设同一owner可写tmp，记录实际绝对路径。
- 每次执行保存完整命令、基线/实际完整HEAD、UTC起止、stdout/stderr、原始EXIT、输出bytes/SHA、源码/测试/补丁SHA及发现/通过/失败/ignored计数；过滤发现并通过≥1，SKIP/缺素材/0 tests不计PASS；新增ignore/allow=0。
- M3已静态核宿主：`src/lib.rs`导出net，`src/main.rs`声明app；A定向 `cargo test -p mir-client --lib r65_ -- --nocapture`、`cargo test -p mir-client --bin mir-client r65_ -- --nocapture`，并各以r64_回归；首次开工仍核实际命中数。
- B定向 `cargo test -p mir-client --test round65_projectile_lifecycle -- --nocapture`；获批隔离负控与功能树相同测试取红/绿；A、B只交目标证据，不外推全量门禁。
- C按[计划89行](round65-plan.md#L89)独立复跑A/B、projectile单测、R64 reclaim/ack、R63 wire、fmt/clippy批准清单；复核Crystal锚点、C1–C8/M1–M6、计数/SHA/禁称词，不替Lead门禁。

- M6/C8门禁：C必改0后仅Lead提交功能批并在功能HEAD跑 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，先核soak锁及独立tmp；本尚书与A/B/C/D均不得代跑。
- Lead独占首次 `target/round65/gate/attempt-1.log` 与配套meta：完整双流；当时落盘UTC起止、原始EXIT、完整HEAD、operator、日志bytes/SHA256；不预写通过或未来SHA；失败原件保全，修因另attempt，禁覆盖首败/倒填原meta。
- 门禁后 `git diff <gate_head> -- crates` 必须零差异；任何crates测试/注释变化亦失效，Lead新功能提交再另attempt；D只据真实功能HEAD/日志/meta收文，提交后核blob/manifest，自身SHA外记避免循环。
- D检查链接、diff空白、JSON解析、逐文件SHA/bytes/UTF-8-LF，命令见[计划90行](round65-plan.md#L90)；不得借检查扩大写范围。
- M4：R61–R64计划/门下/README/终审/冻结JSON及roadmap/client一律只读；R61 P01–P12、R62四项、R63 P1–P7、R64六项按原编号逐项承接，历史阶段BLOCK不冒充当前终态。
- M5/G2本轮整项移出，D明确未做；容量/known_positions LRU/全局最低水位/服务端回收继续PARTIAL。默认无上限增长、未确认backlog不丢最旧；未来配置/淘汰/换图/ID复用仅待评审契约，不猜容量常数。
- C5禁区：server语义/回收协议、wire字段/编解码修改、运行时容量/LRU、生产SkipFrames/动作FIFO、掉落/护身符、素材/设备/Live、570/550扩面；禁称“已限容”“有界”“多消费者已支持”“全链已闭”。
- 风险：未确认积压/clone仍增长，慢消费者无共享历史保证，同步等待器可能取快照不ack，饱和恢复未证；已有功能只补回归，不包装新增策略；全局/素材/音频/渲染历史PARTIAL不清零。

- CLI完整回执：[01-delegate.log](../../target/round65/shangshu/01-delegate.log)，每命令均有UTC/COMMAND/EXIT；四次delegate皆EXIT=0，SUB_TASK_ID依上表；创建是delegate产生子单，不额外create重复任务。
- state回执：`✅ JJC-20261027-002 状态更新：Assigned → Doing`，EXIT=0；validate回执：`✅ 看板校验通过：201 条任务符合 schema v3.1`，EXIT=0；随后父/四子show均EXIT=0，parent_task及部门owner吻合。
- 异常保全：首登记脚本在A成功EXIT=0后，中文日志字节偏移误作字符串偏移，提取ID失败导致脚本EXIT=1；已只读核原日志，修解析并保留A，续建B/C/D，无重复delegate，CLI本身无参数异常；恢复记录在原日志。
- 尚书本次未改crates/计划/门下/R64锁定材料，未运行cargo/ci.ps1，未commit/push；仅文档/证据写入与获准CLI登记。派发完成不代表子单实现完成；最终收口仍经真实回执、独立核验与门下确认。
