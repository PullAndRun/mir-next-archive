# 第65轮中书计划：单UI消费生命周期与克隆快照契约

- 主任务：`JJC-20261027-002`；状态：`Menxia`，待独立审核；本文件不是实现/通过报告。
- create回执：`✅ 收旨 JJC-20261027-002 | 第65轮：单UI消费生命周期与克隆快照契约 | state=Taizi | org=太子`；`TASK_ID=JJC-20261027-002`；exit 0。
- 先核全局help；create命令级help返回缺标题，实际按全局help显式参数创建，无JSON输入。
- 流转回执：`✅ JJC-20261027-002 状态更新：Taizi → Zhongshu`；`✅ JJC-20261027-002 状态更新：Zhongshu → Menxia`；均exit 0。
- validate：`✅ 看板校验通过：197 条任务符合 schema v3.1`，exit 0。仅建主单，以下子单尚未创建。
- 基线：`2deacee`，与origin/main同步；R64主单`JJC-20261027-001`已Done，ACCEPT/PASS-WITH-PARTIAL，A-M1/A-M2闭环。
- R64[路线图](../roadmap.md)、[README](../testing/round64/README.md)、[终审](../testing/round64/round64-final-audit.md)的“待定/BLOCK”是历史阶段；不回写锁定原件，不由旧阶段否定最终主单状态。

## 事实与范围选择

|事实源|核对事实及用途|
|---|---|
|[AGENTS](../../AGENTS.md)|严格对照Crystal；三省六部；中文默认可切语言；本次不提交的具体授权优先于一般提交要求。|
|[roadmap §十三/十四](../roadmap.md)、[client §5.31](../client.md)|R64只覆盖单UI确认前缀；R65六候选不能整包接入。|
|[R64 C证据](../testing/round64/round64-c-evidence.md)、[终审](../testing/round64/round64-final-audit.md)|A-M3精确“半消费→确认半→再push”未实跑；C6实值2/0/2、2/1/3、3/0/3不等于半消费组合。|
|[net](../../crates/mir-client/src/net.rs#L3030)、[projectile](../../crates/mir-client/src/ui/projectile.rs#L602)、[app](../../crates/mir-client/src/app.rs#L210)|reclaimed/裁剪/代号校验、UI绝对游标、本地ack已存在；不包装成R65新增功能。|
|[net同步接收](../../crates/mir-client/src/net.rs#L3881)|wait_for_snapshot消费同一Receiver；不能称全仓只有一个Snapshot接收点，不能自动当已登记的第二UI消费者。|
|[Crystal客户端Effect](../../github/Crystal/Client/MirObjects/Effect.cs#L144)|活动Missiles加入并在191–195 Remove移除，不是永久事件历史、消费注册表或全局ack策略。|
|[Crystal 125包](../../github/Crystal/Shared/ServerPackets.cs#L3564)、[GameScene](../../github/Crystal/Client/MirScenes/GameScene.cs#L4687)|125仅Spell/Source/Destination；源缺失不生成；出生音/存活目标命中分支保持，不能增加wire字段。|
|[Crystal TargetPoint](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L3546)|当前位置优先与源缓存回退，不证明全局LRU、容量值或淘汰顺序。|
|[Crystal Player](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L750)、[Monster](../../github/Crystal/Client/MirObjects/MonsterObject.cs#L346)|SkipFrames依赖ActionFeed：Player非自机Count>0，Monster Count>1；Player:966–967、Monster:528–529 FIFO出队。补生产链涉及动作生命周期，移出本轮。|
|Crystal服务端路径核验|本地不存在候选Server/MirObjects/Effect.cs、Server/MirObjects/Map.cs；实际[MirEnvir/Map.cs](../../github/Crystal/Server/MirEnvir/Map.cs#L2375)为地图对象移除，不作为125回收ack依据。|

**G1必做：现有单UI消费生命周期的可失败回归与本地契约。** 从候选1收敛，优先补R64 A-M3及clone/接收器边界；不实施多消费者全局最低水位。交付为测试、必要本地API注释及R65契约；若发现真实缺陷，须门下明确批准最小本地修复。

|验收项|固定输入与可观察预期|证据形式|
|---|---|---|
|G1.1精确半消费|同代生产e1/e2后发布前缀S2(total=2,len=2)，再生产e3/e4得权威S4(total=4,len=4)。UI只sync S2，水位2；其本地ack交权威S4后(reclaimed,len,total)=(2,2,4)，e3/e4保留；再push e5得(2,3,5)。UI读后缀只新增e3/e4/e5，重复sync新增0；ack5后(5,0,5)。|真实125编码/解码→apply_packet_at→snapshot→UiState::sync；累计出生cue恰5，事件身份/顺序对应，重复drain=0。“消费半”指已发布前缀，不能假装一次sync只消费半表。|
|G1.2 clone独立|裁前克隆S4；权威裁至2后旧clone仍len4/total4/内容不变。clone不注册消费者、不钉住权威前缀；其独立UI可读旧内容，不证明共享历史安全重放。|逐事件标识/顺序与cue数量；同UI重复旧clone不重播；第二UI仅离线反例，生产多消费者仍PARTIAL。|
|G1.3生命周期|同代重复/后退/越尾ack为no-op/no-op/夹尾，total不减；旧代ack不裁新代；新代空首批依既有延迟复位，首批非空只复位一次；total饱和不溢出。|复用R64测试并补组合缺口；UI views/570/cues/cache清理及绝对水位断言，不只内部字段。|
|G1.4入口与wire|app快照到达后严格前进非零水位才ack；wait_for_snapshot按predicate返回，不自动确认、不成为全局最低水位成员；AckProjectileEvents在InGame gate之后、本地send之前返回。|app宿主≥1例；复用R64真实TCP ack零字节＋Turn有字节正对照；核两个接收入口，不增线程/定时器/协议。|

Crystal没有全局水位依据；注册、慢消费者退出、clone租约、超时驱逐、饱和恢复只列未决契约，**继续PARTIAL**。现有total饱和可测，不发明回绕恢复。此目标不要求新增生产策略。

**G2可选、可移出：known_positions及事件表容量政策文档。** 只整理现行默认无界和未来显式可配置策略的设计接口；不增加配置项/运行时LRU，不设魔法容量。保护活跃引用、miss不造点/少生成、换图/ID复用失效、保护项超额、未确认backlog不丢最旧均标待评审/PARTIAL。与G1争时整项移出。

## 历史PARTIAL逐项承接

依据[R61计划](round61-plan.md)/[门下](round61-menxia-review.md)/[README](../testing/round61/README.md)/[原台账](../testing/round61/round61-final-audit.md)、[R62计划](round62-plan.md)/[门下](round62-menxia-review.md)/[README](../testing/round62/README.md)、[R63计划](round63-plan.md)/[门下](round63-menxia-review.md)/[README](../testing/round63/README.md)。R49–R64的历史限定PASS/ACCEPT-WITH-PARTIAL不等于全链闭合。

|来源|R65交接状态|
|---|---|
|R61 P01|R64已补单UI确认裁剪；G1只补生命周期/组合覆盖，clone成本、最低水位/长期内存仍PARTIAL。|
|R61 P02|R62飞行/R63 570局部时钟已证；生产SkipFrames/ActionFeed仍PARTIAL。|
|R61 P03|R63 reset→cache→spawn已修首批可信位置；全局TargetPoint/容量/失效策略仍PARTIAL。|
|R61 P04|运动offset局部已证，不等于ImageInfo.offset或渲染全局闭合。|
|R61 P05/P06|设备/Live未证；33路peak=22.844788削波风险，保留。|
|R61 P07/P08/P09|105 sfx逐项、HiddenSpanSource全局化、CWD/素材来源，均保留。|
|R61 P10|本地generation/延迟复位已有证；饱和/多入口/全局恢复不外推。|
|R61 P11/P12|缺素材SKIP→PARTIAL；R60 P-1..P-6不自动清零。|
|R62四项|物理回收只闭单UI本地子项，服务端未做；known_positions容量/LRU未做；生产owner_skip_frames=false；570 elapsed由R63局部严格时钟supersede，素材/全局不外推。|
|R63 P1|生产SkipFrames/ActionFeed未接，保留。|
|R63 P2|事件物理回收只补单UI子项；事件限容/服务端及全局回收仍PARTIAL。|
|R63 P3|known_positions无容量/LRU，仍仅契约。|
|R63 P4|570素材不入库，真实渲染/可听未证。|
|R63 P5|严格NextFrame限本模块570，R61十二项继续承接。|
|R63 P6|历史终审阶段不代替对应尚书/门下最终结论；本轮不改旧凭证。|
|R63 P7|历史gate原meta缺EXIT/HEAD；后续附件不得伪称原meta，本轮不能消除此证据限制。|
|R64六项|多消费者/clone最低水位、事件限容、known_positions容量/LRU、服务端回收、定向≠全量门禁、R61/R62/R63原编号承接，全部保留；G1不宣称整项完成。|

## 建议子单DAG与write scope

`A（兵部/工部）→B（工部/刑部）→C（独立核验）→Lead功能提交与唯一门禁→D（礼部）`。门下批准后尚书建子单、锁精确白名单；同文件单owner、串行交接。deepseek只编码；gpt决策、文档、独立审核。

|建议子单|责任/依赖|write scope与验收|
|---|---|---|
|A 生命周期契约/单测|兵部/工部deepseek；门下准入后|仅crates/mir-client/src/net.rs、crates/mir-client/src/app.rs现有测试/注释与target/round65/a/**；默认不改生产逻辑。app私有seam在宿主测；真实行为修复另报门下。|
|B 半消费/clone公开链|工部deepseek写测、刑部gpt核表；依A|仅新crates/mir-client/tests/round65_projectile_lifecycle.rs与target/round65/b/**；不改ui/projectile/协议/旧测试。声称wire的Monster/Remove/125种子均须真实编解码，否则披露。|
|C 独立核验|刑部gpt；依A/B|只读源码/旧证据；仅写target/round65/c/**与新docs/testing/round65/round65-c-evidence.md；不修实现、不代Lead跑门禁；独立target复跑，同测红绿SHA/计数核对。|
|D 契约/PARTIAL|礼部gpt；依C与门禁事实|仅新docs/testing/round65/README.md、round65-contract.md、final-manifest.json；G2写在contract可移出；既有roadmap/client默认只读，扩白名单先报门下；不改R64锁定材料。|

## 真红与定向验收

已有测试初跑绿不算新红。门下批准后，A/B在各自证据根隔离源码副本临时将指定本地裁剪/ack选择函数改no-op，绝不覆盖生产树；**no-op桩红须视为修改并获批准**。无批准只报基线绿/覆盖补证，红未达标，不签G1完整验收。编译错、0 tests、旧失败、空泛assert不算红。

- no-op裁剪应令G1.1期望(2,2,4)真实assert失败；no-op ack令宿主“水位前进产生Some”失败；有非零exit、左右值、测试名。
- 另用保持len但删错前缀的负控证明身份/顺序断言有效；5条相同无区别输入不算顺序证明。
- 同一测试文件字节SHA用于红/绿；记录基线HEAD、源码/补丁SHA、命令、UTC起止、stdout/stderr、EXIT、断言；红副本不提交，不改断言迁就。
- 独立CARGO_TARGET_DIR=target/round65/<owner>/cargo-target；TMP/TEMP/TMPDIR为各自可写tmp；过滤命令发现/通过≥1；SKIP/缺素材/0 tests不计PASS，历史ignored列位置且新增0。

以下命令为将来实施验收，规划阶段未运行cargo/CI；target实际拼写在开工前核定，错误不得跳过。

|子单|验收命令（保留原始EXIT）|
|---|---|
|A|`cargo test -p mir-client --lib r65_ -- --nocapture`；`cargo test -p mir-client --bin mir-client r65_ -- --nocapture`；同两命令过滤r64_回归。|
|B|`cargo test -p mir-client --test round65_projectile_lifecycle -- --nocapture`；获批隔离no-op副本相同命令取红，再功能树取绿。|
|C|独立重跑A/B；`cargo test -p mir-client --lib ui::projectile::tests:: -- --nocapture`；`cargo test -p mir-client --test round64_event_reclaim -- --nocapture`；`cargo test -p mir-client --test round64_projectile_ack -- --nocapture`；`cargo test -p mir-client --test round63_projectile_wire -- --nocapture`；`cargo fmt --all -- --check`；`cargo clippy -p mir-client --all-targets -- -D warnings`。|
|D|`py -3 tools/docs/check_links.py`；`git diff --check`；`node -e "JSON.parse(require('fs').readFileSync('docs/testing/round65/final-manifest.json','utf8'))"`；逐文件SHA/字节/UTF-8-LF校验，清单自身SHA另记避免循环。|

## Lead唯一门禁与收口

1. 尚书核status/归属，只暂存批准白名单；历史冻结JSON、target/resource/game-assets/.env不混入。实现/测试/注释为功能批，Lead提交。
2. C独立核验且必改0后，Lead在功能HEAD独占`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，先核soak锁及TMP/TEMP/TMPDIR，冲突先协调；A/B/C/D不得代跑。
3. 首次为target/round65/gate/attempt-1.log，完整双流；meta当时落盘UTC起止、原始EXIT、完整HEAD、operator、日志bytes/sha256。失败原件保全，修因另attempt，不倒填/覆盖首败。
4. 门禁后`git diff <gate_head> -- crates`须零差异；含测试/注释任何crates变化均使门禁失效，Lead新功能提交另attempt；D只据实文档，提交后核blob/manifest。
5. 最终刑部/尚书/门下确认必改0才收口；PASS-WITH-PARTIAL带逐项台账。不预写未来提交SHA、数字、hash或PASS。规划者本次不commit/push、不改crates、不跑ci.ps1。

## 风险、不做与待裁决

- 门下①：批准G1固定预期、隔离no-op/错前缀负控修改；不批准则红证据PARTIAL，不伪称完整。
- 门下②：全局最低水位/容量缺Crystal依据，降级契约+PARTIAL；不引注册/超时/淘汰。wire任何变更另案门下批准，本计划授权为零。
- 尚书③：开工前核A/B精确测试target/宿主/owner/独立target；确认历史P1–P7逐项映射；G2可整项移出。
- 不做：server语义/回收协议、运行时容量/LRU、动作FIFO/生产SkipFrames、570/550扩面、掉落/护身符、设备/Live音频、素材、泛化渲染/多语言改造。
- 未确认积压/clone仍增长；慢消费者没有共享历史保证；同步等待器可能取快照而不ack；如实披露，禁丢最旧、猜容量或自动确认掩盖。
- 服务端回收将来先设计评审＋旧客户端/旧服务端/混合版本兼容矩阵及可失败测试骨架；本轮不写server骨架。
- 中文默认可切语言；新增`#[ignore]`/`#[allow]`为0，不发明Crystal常数；最小可审阅修复，不删除证据/覆盖用户改动。
