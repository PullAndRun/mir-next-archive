# 第66轮：最低水位契约与消费边界反例 Implementation Plan

> **For agentic workers:** 门下 PASS 后由尚书登记委派；逐项执行 A→B→C→D，使用 subagent-driven-development 或 executing-plans 技能。本次仅规划，未启动实现。

**Goal:** 以测试私有最低水位参考函数、真实快照/UI 裁剪反例及兼容矩阵，界定未来多消费者接入前置条件。
**Architecture:** 参考函数只存在于 integration-test support，不链接生产。对同一真实 125 事件流分别施加参考最低水位与现有单 UI 水位，断言慢读取者可见差异；保持生产逻辑、wire 与服务端零变更。
**Tech Stack:** Rust integration tests、WorldSnapshot、UiState、既有 71/125 编解码；Markdown 与证据 JSON。
**Spec:** [roadmap §十三/十四](../roadmap.md)、[R65 契约](../testing/round65/round65-contract.md)、[门下建议](round65-menxia-confirm.md)。

## Global Constraints 与前置依赖

- 主单 `JJC-20261027-003`，标题“第66轮：最低水位契约与消费边界反例”；太子建单 exit0，Taizi→Zhongshu exit0；本计划送 Menxia，不表示获准实施。
- 规划基线 `f14a81dc4a1f141dd37d7229d3f623a07e9a0df5`；R65 主单 `JJC-20261027-002` 实读 Done，门下确认时间 `2026-10-26T20:32:59Z`。
- R65 README/contract 及 roadmap 的 Doing/待终审是历史快照，不能覆盖看板终态；本轮不回写冻结材料，不伪改其历史意见。
- 已存未跟踪 `docs/testing/monster-native/player-resource-freeze-before.json`、`docs/testing/monster-native/player-resource-freeze.json` 必须保全，不暂存、不覆盖。
- 本次只写本文、`target/round66/plan/**` 及 CLI 看板；不改 crates、不 commit/push、不运行 cargo 或 ci.ps1；子单尚未建立。
- 执行前尚书核 HEAD/status、精确 target/owner/白名单及依赖；HEAD 漂移先比对影响，不把旧测试结果当新树结果。
- 默认中文、可切语言；不新增本地化文案断言；新增 `#[ignore]`/`#[allow]` 为 0；不引依赖、不发明 Crystal 容量常数。

## 候选裁决（只选一个主目标）

|候选|单轮裁决|核验依据与理由|
|---|---|---|
|A 多消费者/全局最低水位契约|选；限定参考模型＋边界反例＋兼容矩阵|R65 已证单 UI、clone 值独立和 waiter 不 ack；缺的是慢读取者在当前权威裁剪后失去历史的差分证据。公开 apply_projectile_ack/UI seam 足够，新增可失败测试，无需生产注册或 wire。|
|B 事件表/位置缓存容量|移出整项|事件表 append/单 UI trim；known_positions 只 insert，均无已定容量。只注入任意常数或写“不丢最旧”会自证假政策；真正限容需先裁决保护引用、超额、背压/丢弃与失效，不能夹带进 A。|
|C 服务端回收协议|移出、独立跨轮设计|Crystal 125 只有 Spell/Source/Destination；当前 mir-server magic.rs:2803–2809 直接 encode/broadcast，没有本地消费者水位。Map.RemoveObject 不等于事件确认；需协议评审、身份/重连/兼容及可失败 wire 设计，超出本轮零 wire。|
|D §十四其他项|本轮不选，非断言不可单轮|Player/Monster ActionFeed 有 FIFO/SkipFrames 原作链；550/索敌需另选具体场景与原作入口，现轮没有完整场景证据；掉落表/护身符属于独立服务端机制。素材需索引/来源，音频需真实设备/Live；均无法由本地水位测试验收。|

选 A 的增量是“最小安全裁剪候选”与“当前生产缺口”的可执行差分，不重复包装 R65 半消费测试。模型策略是本项目待实施设计，绝不归因于 Crystal 已有全局水位。

## 已逐条打开核实的原作及当前实现锚点

|事实源|核对事实与限制|
|---|---|
|[Effect.cs:144](../../github/Crystal/Client/MirObjects/Effect.cs#L144)、[155](../../github/Crystal/Client/MirObjects/Effect.cs#L155)、[191–195](../../github/Crystal/Client/MirObjects/Effect.cs#L191)|Missiles 是活动列表；构造加入，Remove 删除；不是历史保留、注册或全局 ack。|
|[GameScene.cs:4687–4714](../../github/Crystal/Client/MirScenes/GameScene.cs#L4687)|源不存在 return；FireBounce 出生音、410 飞行；仅存在目标且未死亡才产生 570 与命中音。测试冻结时钟，只计出生 cue，不证明实际可听。|
|[PlayerObject.cs:750](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L750)、[966–967](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L966)|SkipFrames = this != User && ActionFeed.Count > 0；FIFO 取 [0] 后 RemoveAt(0)。不扩生产动作链。|
|[PlayerObject.cs:3546–3556](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L3546)|TargetPoint 回退、对象当前位置优先；不是 known_positions 容量/LRU 依据。|
|[MonsterObject.cs:346](../../github/Crystal/Client/MirObjects/MonsterObject.cs#L346)、[528–529](../../github/Crystal/Client/MirObjects/MonsterObject.cs#L528)|SkipFrames = ActionFeed.Count > 1；FIFO 出队。不是消费者注册表。|
|[ServerPackets.cs:3560–3579](../../github/Crystal/Shared/ServerPackets.cs#L3560)|ObjectProjectile 序列化仅 byte Spell、uint Source、uint Destination；没有 ack/事件序号字段。|
|[Map.cs:2375–2383](../../github/Crystal/Server/MirEnvir/Map.cs#L2375)|RemoveObject 从种类集合/地图 cell 移除对象，不能证明投射物事件表服务端回收。|
|[net.rs:3030–3074](../../crates/mir-client/src/net.rs#L3030)、[app.rs:210–229](../../crates/mir-client/src/app.rs#L210)|reclaimed=total−len；apply_projectile_ack 校代后裁前缀；app 只上报非零严格前进的单 UI 水位，没有取多消费者 min。|
|[net.rs:3853–3895](../../crates/mir-client/src/net.rs#L3853)、[projectile.rs:715–761](../../crates/mir-client/src/ui/projectile.rs#L715)|try_recv/wait_for_snapshot 使用同一 Receiver；waiter 不 ack。UI 游标会跨过已裁前缀，因此水位前进不证明旧事件均被该 UI 看到。|
|[R65 wire 测试:63–145](../../crates/mir-client/tests/round65_projectile_lifecycle.rs#L63)|可参考真实 71/125 frame encode→decode→apply_packet_at 与公开 UI watermark；只复制最小 fixture，不修改/依赖另一个测试 target 的私有 helper。|

## G1 必做：参考裁剪契约与兼容矩阵

- 仅新增测试 support：`crates/mir-client/tests/support/round66_watermark.rs`，不导出库 API、不建注册服务、不接 app、不改变 NetClient。
- 精确接口：`enum ReferenceError { NoConsumers, GenerationMismatch, BelowReclaimed, BeyondTail, InvalidBounds }`，derive Debug/PartialEq/Eq。
- 精确接口：`fn minimum_confirmed(generation: u64, reclaimed: u64, total: u64, consumers: &[(u64, u64)]) -> Result<u64, ReferenceError>`；供测试模块使用，签名可标 pub(crate)。
- 顺序：reclaimed>total→InvalidBounds；空成员→NoConsumers；任一代不匹配→GenerationMismatch；任一水位<reclaimed→BelowReclaimed；任一水位>total→BeyondTail；否则返回所有成员水位最小值。
- Err 必须不调用真实裁剪；空成员 fail-closed，不默认取尾；0 在 reclaimed=0 时是合法慢成员，阻止回收。重复成员不改变 min。
- consumers 是测试显式提供的假设集合，非生产注册。新加入者仅能从 >=reclaimed 的显式水位开始；低于已回收位置拒绝，历史补发/追赶/注销/租约另案。
- 测试夹具计数固定小值且不饱和；`reclaimed+i+1` 为 1-based 含本条水位。u64::MAX 恢复继续 PARTIAL，不设计回绕。
- UI 全程冻结 `now_ms=1000`；初读 S2/S4 后先 drain 初始 cue 并记录 view 基数，后续只比较新增 view 的 destination 顺序与新增出生 cue，防止把旧 e1/e2 混计或推进命中时钟。
- 可移出：性能测量、长跑内存观测、消费者动态注册/退出设计扩展；均不得替代下列必做。不设第二主目标/G2。

|兼容对象|现状与本轮可证|接入条件/继续 PARTIAL|
|---|---|---|
|单 app UI|维持现有本地 ack；单成员参考结果等于原水位|生产路径不调用参考函数；既有生命周期测试绿不等于多消费者绿。|
|已存在的 clone|权威裁后旧值独立，可离线读取|clone 不注册、不钉住权威表；晚到 UI 读裁后表会缺历史。|
|同步等待器|同一 Receiver 竞争取事件；predicate 观测、不发 ack|不是广播订阅者，不列入模型成员；并发/fan-out 未实现。|
|未来显式消费者|模型固定成员快4/慢2返回2；慢0阻止裁剪|仅提案；注册原子性、成员退出、历史补发、背压/容量与生产接线未实现。|

## 可失败验收（G1 全部必做）

|ID/测试名|固定输入与断言|红证据要求|
|---|---|---|
|G1.1 `r66_reference_minimum_and_validation`|gen=g, floor0, tail4；[4,2]→2，[4,0]→0，单[4]→4，重复[2,2]→2；表驱动覆盖五种 Err；Err 时真实快照事件身份/total 不变。|隔离副本 min→max，至少 [4,2] 真实 assert 4≠2；空集→tail 另红，不能只测 helper 自己返回常数。|
|G1.2 `r66_minimum_preserves_slow_consumer_suffix`|真实71种子源7/目标1..5；真实125 e1..e4，慢UI只读S2水位2，快UI读S4水位4；model min2交真实apply ack→(reclaimed,len,total)=(2,2,4)，保留e3/e4；push e5→(2,3,5)，慢UI只新增e3/e4/e5，身份顺序[3,4,5]、出生cue3，重复sync新增0。|隔离副本 min→max，同一测试必须在真实suffix/事件身份或cue断言失败；不能仅参考函数返回值失败。|
|G1.3 `r66_single_ack_exposes_late_reader_gap`|独立同流分支直接用当前快UI ack4，裁空再push5；慢UI从S2读新表，只新增e5/cue1，水位却到5；与安全分支新增[3,4,5]/cue3差分。旧S4 clone仍含1..4，旧值独立不挽回权威历史。|反例测试本身应绿且明确“现状缺口”；隔离副本错误裁剪保持len却删错前缀，G1.2身份顺序断言红。不得把“慢者必须完整”写成永久失败并标已完成。|
|G1.4 `r66_generation_and_late_join_fail_closed`|floor2/tail4，新成员0→BelowReclaimed；旧代成员→GenerationMismatch；空成员→NoConsumers；各分支裁剪调用计数0、真实表不变；同代合法2/4→2。|隔离副本忽略 generation 或接受低水位须真实assert红；不伪称动态注册系统已测。|
|G1.5 `r66_waiter_uses_shared_receiver_without_ack`|net.rs cfg(test)宿主：预先send S1/S2；wait predicate total>=2 返回S2；随后try_recv=None，command_rx=Empty，证明早期与命中快照均被同一receiver取走；无并发线程/睡眠。|隔离副本等待器返回时注入本地ack，Empty断言红；复用R65 predicate正对照，错误返回S1也须失败。|
|G1.6 兼容/零行为矩阵|四行矩阵逐项指向测试/现有源码；正常编译生产逻辑/wire/server diff为空；R64/R65定向回归通过。|静态检查若出现非test生产变更、漏行/虚假PASS即失败；此项以范围检查为准，不伪造运行时红。|

红/绿记录：同一测试文件字节 SHA；基线 HEAD、UTC 起止、命令、原始 EXIT、stdout/stderr、失败测试名/左右实值、变异补丁 SHA、源码 SHA、字节数；0 tests/编译错误/SKIP/既有失败不算红。每项变异单独 attempt，首败永久保全。
门下对本计划 PASS 即包含在 `target/round66/<owner>/red-copy/**` 隔离副本做上述最小变异的评审；不改工作树，不提交变异，不恢复覆盖原件；如门下排除此项，G1 红证据不达标须退回裁决，不能豁免为 PASS。

## 子任务 DAG 与精确 owner（尚未建子单）

`A → B → C → D`；Lead 唯一功能提交/门禁位于 C 通过之后、D 记录门禁之前。全程同文件单 owner，依赖串行；deepseek 编码，gpt 独立核验/文档。

|节点|owner/depends_on|产出、白名单与交付|
|---|---|---|
|A|兵部 deepseek；依门下 PASS|新建 crates/mir-client/tests/support/round66_watermark.rs 与 crates/mir-client/tests/round66_watermark_contract.rs；只写此二文件及 target/round66/a/**。实现参考函数、G1.1/G1.4及其红绿；后续 B/C 只读 support。|
|B|工部 deepseek；依 A|新建 crates/mir-client/tests/round66_consumer_boundary.rs，#[path="support/round66_watermark.rs"] 引用 A；仅另改 crates/mir-client/src/net.rs 的 cfg(test) 区新增 G1.5。产出 G1.2/G1.3/G1.5与 target/round66/b/** 红绿。不得改 app/ui 或 net 生产逻辑/旧测试。|
|C|刑部 gpt-6.1-sol；依 B|只读实现/历史证据；独立 target 复跑与负控，写 docs/testing/round66/round66-c-evidence.md、target/round66/c/**；核 G1.1–G1.6、计数/实值/SHA、无新增ignore/allow、必改0才交Lead。发现缺陷退给原owner，不自修。|
|D|礼部 gpt-6.1-sol；依 C 及 Lead 门禁事实|只写 docs/testing/round66/README.md、docs/testing/round66/round66-contract.md、docs/testing/round66/final-manifest.json、target/round66/d/**；记录四行矩阵、原编号PARTIAL、红绿与门禁索引、阶段。既有 roadmap/client/R49–R65 冻结原件只读。|

- [ ] A：先写断言与最小参考函数，在隔离变异取有效红，功能树绿；不得以缺函数编译错误充红。
- [ ] B：真实71/125串行双分支与等待器宿主；先核事件身份与cue差分，再保全各变异红绿，交接 C。
- [ ] C：独立复跑相同命令/变异，核 same-test SHA、发现数>0且无skip；审正常构建差异；必改问题退回并换attempt。
- [ ] D：只在实际门禁后填写实数/HEAD/SHA；校链接/编码/manifest，交尚书验收与独立门下确认。

## 将来执行命令与证据根（规划阶段未运行）

- A：`cargo test -p mir-client --test round66_watermark_contract -- --nocapture`。
- B：`cargo test -p mir-client --test round66_consumer_boundary -- --nocapture`；`cargo test -p mir-client --lib r66_ -- --nocapture`。
- C：独立复跑 A/B；`cargo test -p mir-client --test round65_projectile_lifecycle`；`cargo test -p mir-client --test round64_projectile_ack`；`cargo test -p mir-client --test round64_event_reclaim`；`cargo test -p mir-client --lib r65_`；`cargo test -p mir-client --bin mir-client r65_`；`cargo test -p mir-client --lib ui::projectile::tests::`；`cargo fmt --all -- --check`；`cargo clippy -p mir-client --all-targets -- -D warnings`。
- 每个 owner 使用 CARGO_TARGET_DIR=`target/round66/<owner>/cargo-target`，TMP/TEMP/TMPDIR=`target/round66/<owner>/tmp`（绝对化）；attempt-N 不复用。测试不得依赖素材/声卡；cue只断言文件标识与次数。
- D：`py -3 tools/docs/check_links.py`、`git diff --check`、JSON parse；逐文件核 UTF-8/LF、lines/bytes/SHA，manifest自身SHA另记避免循环。

## Review Focus 与 C1–C8 风险/封驳条件

|条件|风险与处置/判据|
|---|---|
|C1 范围越界|发现生产注册/全局min接线、容量/LRU、server/wire变化即封驳；退回测试私有设计，另案评审。C/G1.6负责。|
|C2 参考模型冒充生产|模型绿不证明并发/fan-out；必须同时有G1.3现状反例与四行矩阵，否则退回。模型不是 Crystal 原作机制。|
|C3 成员/代号边界|0慢成员、空集、旧代、晚加入低于floor、越尾、非法bounds均须可失败；不得自动注销/超时推进floor。A/G1.1/G1.4负责。|
|C4 水位掩盖漏读|不能只断言consumed=5；G1.2/G1.3须事件身份、顺序、cue3对cue1和旧clone不变；缺一退回B。|
|C5 Receiver误称广播|G1.5必须同receiver顺序取走S1/S2且commands空；不宣称等待器为第二消费者/并发安全。无不稳定sleep。|
|C6 红证据与本地化|有效红必须运行到目标assert，same-test SHA/原始exit/实值齐；中文诊断允许，禁止依赖中文UI文案断言；0 tests/skip不计PASS。|
|C7 历史与容量饱和|不能清零整项PARTIAL；默认backlog/clone/cache仍可增长，u64::MAX不是恢复或唯一序号证明；容量/素材音频不混入G1。|
|C8 门禁/证据漂移|Lead独占门禁；crates任何门禁后变更均失效，另功能提交与新attempt；禁止覆盖首败、伪填HEAD/EXIT或暂存冻结JSON。|

## 历史 PARTIAL 承接、禁称与回滚

- R49–R60 所有既存 PARTIAL 按原轮/原编号继续承接；本轮不重编、不整项清零，不以水位模型推导场景/掉落/护身符/素材/音频闭合。R60 P-1–P-6 仍由原台账定义。
- R61 P01：仅补最低水位提案与反例；生产多消费者/长期内存仍在。P02生产SkipFrames/ActionFeed；P03全局TargetPoint/容量/失效；P04 ImageInfo/渲染扩面继续在。
- R61 P05–P06设备/Live与33路peak=22.844788削波；P07 105 sfx逐项；P08 HiddenSpanSource全局化；P09 CWD/来源；P10饱和恢复；P11缺素材SKIP；P12/R60 P-1–P-6，均仍PARTIAL。
- R62原四项：①服务端物理回收②known_positions容量/LRU③生产owner_skip_frames=false④R63局部570时钟不外推素材/全局，均保留。
- R63 P1生产动作来源；P2事件限容/服务端/全局回收；P3位置容量；P4真实素材可听/渲染；P5局部严格时钟/R61交接；P6历史阶段≠当前终态；P7旧gate meta缺EXIT/HEAD，按原编号延续。
- R64原六项：①多消费者/clone最低水位②事件容量③位置容量/LRU④服务端回收⑤定向≠唯一门禁⑥历史原编号延续；R65 G1.2仅值独立、G2未做、全局最低水位未实现，均不由本轮参考模型清零。
- 禁称：“多消费者已支持”“全局回收已实现”“已限容/有界”“服务端ack已闭环”“全链已闭”“真实可听已证”。可称“最低水位参考契约/现状反例已由测试补证”，且必须伴PARTIAL。
- 测试不满足预期时保全首败，退原owner；最小回滚仅撤回本轮新增测试/文档与cfg(test)增量，不删除旧证据，不覆盖用户文件，不以改生产行为追绿。

## 提交、唯一门禁与收口边界

1. 仅 Lead 在尚书确认白名单后提交功能批；A/B/C/D 不 commit/push，不运行 tools/dev/ci.ps1。规划者也不提交。
2. C 独立核验必改0后，Lead核soak锁与TMP归属，在确定功能HEAD独占 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`。
3. gate/attempt-N保留完整双流、UTC、原始EXIT、完整HEAD、operator、log bytes/SHA；首次失败不可覆盖，不预填未来PASS。
4. 门禁后 `git diff <gate_head> -- crates` 必须为空，包括测试/注释；任何变化使旧门禁失效，由Lead新提交另attempt。D据实出清单，不倒填。
5. D文档校验后，刑部/尚书/独立门下按真实看板责任链验收、确认；Done前只报当前阶段，模型/回归/门禁各自的PASS不替代最终验收。
6. 本次主单只推进至Menxia并 `validate --as zhongshu` exit0；子任务等待门下PASS后由尚书派发，禁止预建或预填执行结论。
