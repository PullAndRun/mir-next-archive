# R51 刑部／御史台终审（xingbu）

**verdict：BLOCK**。这是功能与证据闭环不满足派发契约的裁决，**不是否认真实的12/12门禁成功，也不指控伪造日志**。固定的两项PARTIAL可接受；另外发现FrostCrunch控制状态不生效、ShoulderDash必需语义缺失、延迟雷击地图未冻结，以及C层真实包链断言缺失，不能按“五技能完整”结项。

- 子单：`JJC-20261025-003-sub-c0e495c7`；快照：`main`，`HEAD = origin/main = 13f7c7828c162f3f3c1c293a672af4b84f0fd5e4`。
- 文档提交：`7802eb9d3e7d4431b5454a7f8a0355c7057440e9`；功能提交：`13f7c7828c162f3f3c1c293a672af4b84f0fd5e4`。
- Crystal checkout：`0e315fe327192afe52c3d7357ddd1f5b7e26c5b8`。本地未找到名为Spell.cs的文件；据实际存在的[HumanObject.cs](<../../../github/Crystal/Server/MirObjects/HumanObject.cs>)、[SpellObject.cs](<../../../github/Crystal/Server/MirObjects/SpellObject.cs>)、[Map.cs](<../../../github/Crystal/Server/MirEnvir/Map.cs>)核对，不虚构文件或行号。
- 只读实现审计；仅写本报告、[manifest](<final-manifest.json>)与[门禁摘要](<../../../target/round51/final-gate-summary.log>)；另按直接授权经edict CLI回填看板。未改代码、原始日志、冻结JSON；未commit/push、未执行全量CI或ignored。直接人类指令明确允许定向magic测试，优先于旧[派发契约:63](<../../process/round51-shangshu-dispatch.md#L63>)的“不自跑cargo”。

## A．门禁原始产物与返工链：PASS（带证据边界）

权威原件为[attempt3 meta:1–8](<../../../target/round51/final-gate3.meta.txt#L1-L8>)与[attempt3 log](<../../../target/round51/final-gate3.log>)。独立`Get-FileHash -Algorithm SHA256`：**186,888 B**，SHA256 **`0539b82fb2eff360e4c0e6315e40c9a9a506a6ab11ac956ca7730c6fb530bd48`**，与meta完全一致；meta自身236 B、SHA256 `744398b3502aec103dde738ec8d429283741d32dac2e70e81a1ed038143fdc07`。保存的原始进程记录为`exit_code=0`；UTC `2026-10-25T01:29:49Z → 01:31:57Z`，128 s；HEAD为上述已提交功能树。

原日志混合UTF-8与Windows中文代码页；未转码或覆盖。末尾汇总按原始十六进制检查，12项均`OK`，末句为“全部通过（12项）”。Code由[ci.ps1:38–43](<../../../tools/dev/ci.ps1#L38-L43>)的`$LASTEXITCODE`与`OK`条件对应；原件没有独立序列化的Code JSON列，以下不冒充原始JSON。脚本[失败exit1、成功exit0](<../../../tools/dev/ci.ps1#L171-L177>)。

| Name（原脚本名称） | Code | Seconds |
|---|---:|---:|
| cargo fmt --all --check | 0 | 2 |
| cargo clippy --workspace --all-targets -D warnings | 0 | 1 |
| cargo test -p mir-server --test duo_smoke（串行） | 0 | 12 |
| cargo test --workspace | 0 | 106 |
| 协议生成器 --check | 0 | 1 |
| 文档链接与包编号 | 0 | 0 |
| TextKey 完整性 | 0 | 0 |
| 硬编码棘轮（player-visible，阈值70） | 0 | 0 |
| 硬编码棘轮（no-test，阈值0） | 0 | 0 |
| 运行时不引用老资源格式 | 0 | 0 |
| 物品图标完整性 | 0 | 1 |
| PostgreSQL 集成测试（mir-store串行） | 0 | 2 |

耗时列合计125 s，与进程128 s相差3 s，为脚本/进程开销，不把整数秒列伪称精确墙钟和。

返工链真实保留：

1. [attempt1 meta](<../../../target/round51/final-gate.meta.txt>)：exit1；[log](<../../../target/round51/final-gate.log>)为188,766 B，SHA256 `81bb05e3d424799082b49d2b655461b318ae9479d906c921c93b3270de33568b`；UTC01:17:31–01:21:44，253 s；HEAD7802eb9，工作区4条状态。失败点96>70，no-test为0。
2. [before清单:123](<../../../target/round51/b/10-i18n-before.log#L123>)记录96；[修复脚本:14–85](<../../../target/round51/b/fix-i18n-hardcoded.py#L14-L85>)列35对字面量，[脚本:90–100](<../../../target/round51/b/fix-i18n-hardcoded.py#L90-L100>)要求逐对唯一匹配，未换断言运算符；[修后日志:4、93–100](<../../../target/round51/b/10-i18n-fix.log#L93-L100>)记录35条替换、61≤70、no-test0及两个exit0。96−35=61。本审计仅审脚本，**未重跑写代码的脚本**。
3. [attempt2 meta](<../../../target/round51/final-gate2.meta.txt>)：exit0；[log](<../../../target/round51/final-gate2.log>)为186,895 B，SHA256 `146577c4e64f944019caaacd9e8245a46d55372e68eed594b89a8293025f3398`；UTC01:25:12–01:28:55，223 s；HEAD仍7802eb9。这是提交前脏树证据，**不得替代attempt3**。
4. 功能提交时间09:29:40+08:00；attempt3从09:29:49+08:00起，时序一致。三原件SHA均独立吻合，失败未被覆盖。

未发现“门禁项SKIP仍记绿”的本轮证据；但不把12项称为仓库所有测试：既有[ci.ps1:111–118](<../../../tools/dev/ci.ps1#L111-L118>)显式使workspace使用`--skip duo_ --skip fault_pg`，duo另项串行；fault_pg与NoAssets不在本次12项里，[脚本:147–151](<../../../tools/dev/ci.ps1#L147-L151>)有既有说明。测试名里的`skipped`或i18n扫描打印的历史`skip:`字面量不等于门禁跳过。原进程退出码以保存meta为历史记录，非刑部重新观察该历史进程；刑部未重跑完整CI。

## B．五技能与Crystal：部分符合，存在阻断偏差

全部来自实际源码读取；“已对齐”和“未完整”分别判定，超过六处抽查。

| 检查点 | Rust证据 | Crystal证据 | 判定 |
|---|---|---|---|
| Blink 200ms | [magic.rs:1671–1680](<../../../crates/mir-server/src/magic.rs#L1671-L1680>)、[199/200ms测试:4473–4490](<../../../crates/mir-server/src/magic.rs#L4473-L4490>) | [HumanObject.cs:3537–3539](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L3537-L3539>) | 调度及正常落地时点相符。 |
| Blink禁传/射程/随机 | [magic.rs:2968–2995](<../../../crates/mir-server/src/magic.rs#L2968-L2995>) | [HumanObject.cs:6114–6127](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L6114-L6127>) | 有实际禁传提示、再查射程、0..3随机门与指定格传送；未覆盖低等级随机失败及占格落点等分支。 |
| TemporalFlux 30s/30% | [magic.rs:2937–2940](<../../../crates/mir-server/src/magic.rs#L2937-L2940>)、[cost:300–318](<../../../crates/mir-server/src/magic.rs#L300-L318>) | [HumanObject.cs:6090–6091](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L6090-L6091>)、[6127](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L6127>) | 内部截止时间和扣蓝顺序已接；没有真正AddBuff/客户端buff生命周期，只是内部计时器，不能称完整buff语义。 |
| StormEscape 500→750ms | [magic.rs:1648–1668](<../../../crates/mir-server/src/magic.rs#L1648-L1668>)、[测试:4556–4594](<../../../crates/mir-server/src/magic.rs#L4556-L4594>) | [HumanObject.cs:3580–3588](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L3580-L3588>)、[4264–4269](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L4264-L4269>) | 两独立效果、先雷后传；传送失败不撤已结伤害的代码路径存在，但仅测成功，未实测禁传/随机失败保留伤害。 |
| 雷击地图冻结 | [magic.rs:1655–1660](<../../../crates/mir-server/src/magic.rs#L1655-L1660>)只捕获location；[2038](<../../../crates/mir-server/src/magic.rs#L2038>)、[2172–2181](<../../../crates/mir-server/src/magic.rs#L2172-L2181>)结算时取caster当前map | [HumanObject.cs:4268–4269](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L4268-L4269>)把延迟动作挂施法时CurrentMap.ActionList | **偏离**：500ms内换图，旧坐标被用于新图；ThunderStorm与StormEscape均受影响。静态路径已证，未运行跨图探针。P05。 |
| FrostCrunch飞行延迟与MAC/±2 | [magic.rs:1309–1324](<../../../crates/mir-server/src/magic.rs#L1309-L1324>)、[1630–1644](<../../../crates/mir-server/src/magic.rs#L1630-L1644>)、[2241–2270](<../../../crates/mir-server/src/magic.rs#L2241-L2270>) | [HumanObject.cs:3939–3952](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L3939-L3952>)、[5967–5968](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L5967-L5968>) | 距离×50+500、MAC、同图±2的怪物路径真实；玩家被monster-only前置过滤，后续is_player分支不可达。 |
| FrostCrunch等级概率 | [magic.rs:3048–3099](<../../../crates/mir-server/src/magic.rs#L3048-L3099>) | [HumanObject.cs:5970–5991](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L5970-L5991>) | 怪物+10等级、20/40分母结构存在；**Frozen时长硬编码5**，误把Freezing解释为怪物字段；原作取施法者Stats[Freezing]。P03。 |
| Slow/Frozen真实行为和tick | [monster.rs:1273–1280](<../../../crates/mir-server/src/monster.rs#L1273-L1280>)、[magic.rs:3879–3888](<../../../crates/mir-server/src/magic.rs#L3879-L3888>) | [MonsterObject.cs:634–663](<../../../github/Crystal/Server/MirObjects/MonsterObject.cs#L634-L663>)、[1476–1479](<../../../github/Crystal/Server/MirObjects/MonsterObject.cs#L1476-L1479>)、[1544–1553](<../../../github/Crystal/Server/MirObjects/MonsterObject.cs#L1544-L1553>) | **仅存毒标记，不限制怪物移动/攻击，不施加Slow速度变化；tick续跳固定2000ms而非本技能1000ms，且未重置OperateTime。** P03。 |
| ThunderStorm自身5×5、非亡灵1/10 | [magic.rs:1648–1660](<../../../crates/mir-server/src/magic.rs#L1648-L1660>)、[3120–3195](<../../../crates/mir-server/src/magic.rs#L3120-L3195>) | [HumanObject.cs:4264–4269](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L4264-L4269>)、[Map.cs:1303–1334](<../../../github/Crystal/Server/MirEnvir/Map.cs#L1303-L1334>) | 保存施法瞬间中心、半径2、除10、玩家敌对筛选真实；地图冻结问题见P05；地形仅in_bounds，原作还要求cell.Valid。 |
| ShoulderDash距离/逐格/末包 | [magic.rs:3277–3292](<../../../crates/mir-server/src/magic.rs#L3277-L3292>)、[3354–3373](<../../../crates/mir-server/src/magic.rs#L3354-L3373>)、[1735–1756](<../../../crates/mir-server/src/magic.rs#L1735-L1756>) | [HumanObject.cs:4989–5004](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L4989-L5004>)、[5091–5098](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L5091-L5098>)、[5152–5156](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L5152-L5156>) | Next(2)+L+2、逐格150/151、安全区、末122、不发普通Magic真实；3级为5–6格，测试名“3_to_4”陈旧但实际断言5–6。 |
| ShoulderDash失败后计时 | [magic.rs:3375–3395](<../../../crates/mir-server/src/magic.rs#L3375-L3395>) | [HumanObject.cs:5128–5156](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L5128-L5156>) | **零步分支3390提前返回，漏无条件CellTime=now+500/StepCounter=0**。失败测试未检查非零原StepCounter与CellTime。P04。 |
| ShoulderDash玩家可推与受困 | [magic.rs:3273–3280](<../../../crates/mir-server/src/magic.rs#L3273-L3280>)、[3298–3307](<../../../crates/mir-server/src/magic.rs#L3298-L3307>) | [HumanObject.cs:4977–4979](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L4977-L4979>)、[5021–5027](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L5021-L5027>)、[2649–2724](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L2649-L2724>) | **无InTrapRock/CanWalk等价前置；所有玩家直接阻挡**。注释“基类Pushed=0”忽略HumanObject override，缺Pushed本人包及玩家推移，不能称原作完整。P04。 |

补充：Blink/StormEscape的[broadcast_spell_effect:2961](<../../../crates/mir-server/src/magic.rs#L2961>)显式排除施法者；原作这两处分支调用的是**CurrentMap.Broadcast**，其[Map.cs:2437–2447](<../../../github/Crystal/Server/MirEnvir/Map.cs#L2437-L2447>)给范围内全部玩家（含施法者）发包。将其误当MapObject.Broadcast会漏自机ObjectEffect。传送buff在效果/修炼之前被内部置时，亦未达到原作实序。P06。

## C．固定PARTIAL：PASS（拒绝，不扣蓝）

[spell_kind:573–605](<../../../crates/mir-server/src/magic.rs#L573-L605>)未增加PoisonCloud/TrapHexagon支持分支，落Unsupported；[cast_magic:1235–1239](<../../../crates/mir-server/src/magic.rs#L1235-L1239>)在扣蓝之前真实返回UnsupportedSpell。新增[测试:4890–4908](<../../../crates/mir-server/src/magic.rs#L4890-L4908>)逐技能断言分类、错误和MP不变；独立复跑18例中该例真实通过。没有免耗成功、没有删Fencing拒绝测试；原[world.rs:6801–6823](<../../../crates/mir-server/src/world.rs#L6801-L6823>)保留。

[七行矩阵:16–17](<../../../target/round51/b/07-seven-row-matrix.md#L16-L17>)这两行与代码一致；其余行不是可靠的完整性证书：普通技能被写成发MagicCast，实际该包只在dash提前返回发送；Blink列“ObjectEffect→TeleportOut/In”与实际先Out/In后效果相反；Dash列首尾各一次MagicCast，实际只末尾；Frozen时长/1000ms tick/玩家可推注释也与B表矛盾。必须修订矩阵，不能以文件存在当闭环。

## D．客户端包消费：真实实现，但C验收不足

[net.rs:5117–5151](<../../../crates/mir-client/src/net.rs#L5117-L5151>)确有39新增行：150更新自机权威坐标并入步行动作；151更新远端motion与坐标；152回纠自机；153清motion并置Standing；122进入Cast声音队列。**不是空match或伪造实现**。

但有四项边界：

- 150/151借用Walking与Walk声音，无独立dash动作；这是可见的跨动作降级，不能称Crystal冲撞帧已实现。P08。
- [net新增diff](<../../../crates/mir-client/src/net.rs#L5114>)只有生产分支，没有新增状态链测试；28个net测试为既有回归。服务端新测试止于ServerPacket::decode，**没有将真实服务端输出送入WorldSnapshot::apply_packet_at并断言客户端状态**；不满足[派发:54–55](<../../process/round51-shangshu-dispatch.md#L54-L55>)。P07。
- 对[net.rs](<../../../crates/mir-client/src/net.rs>)全文件搜索未找到Pushed/ObjectPushed消费分支；新增推怪实际会发ObjectPushed，未证明客户端受推对象落位。此包不在用户D指定五包之中，但属于B要求的真实推移反馈，P08。
- Walk28/Run29、ObjectAttack、原R46/R49辅助函数未被本diff修改；原日志net28、ui197通过，构成既有回归证据；**不等于新增dash时钟、逐格队列或R49像素帧已重验**。未跑截图/渲染探针，不借旧截图判本轮通过。

## E．证据真实性、复现与工作树：数字真实，闭环有缺口

| 原证据 | 实际结果 | 本审计复现边界 |
|---|---|---|
| [01 magic:27–30](<../../../target/round51/b/01-magic-tests.log#L27-L30>)、[11返工后magic:27–30](<../../../target/round51/b/11-magic-tests.log#L27-L30>) | 18 passed；0 failed/ignored；150 filtered；EXIT0 | **独立执行同一命令18/18、exit0**；job `bash-b13c5bc732924649`已收输出。 |
| [02 server lib:184–187](<../../../target/round51/b/02-server-lib.log#L184-L187>) | 168 passed；0 failed；EXIT0 | 核原件与提交后workspace日志，不另跑整套。 |
| [03 protocol:186–214](<../../../target/round51/b/03-protocol-tests.log#L186-L214>) | 178+4+4；doc-test0；EXIT0 | 数字准确，零doc-test不冒充新增用例。 |
| [04 client net:38–41](<../../../target/round51/b/04-client-net-tests.log#L38-L41>) | 28 passed；260 filtered；EXIT0 | 原件核对，没有新增R51包状态测试。 |
| [05 client ui:210–213](<../../../target/round51/b/05-client-ui-tests.log#L210-L213>) | 197 passed；91 filtered；EXIT0 | 原件核对，没有R51视觉运行证据。 |
| [12 fmt/clippy](<../../../target/round51/b/12-fmt-clippy.log>) | fmt0/clippy0 | 保留[99首次clippy失败](<../../../target/round51/b/99-clippy-first-attempt-fail.log>)，无隐藏失败。 |

独立执行两种i18n扫描与文档链接检查：**61≤70、no-test0、926条相对链接/2篇README，均exit0**；job `bash-1a4d6b5e93a249fa`已收输出。此链接数是审计产物写入前的结果；落盘后另重验，结果见[manifest复现记录](<final-manifest.json>)。中文stdout受宿主代码页显示影响，计数与退出码明确，未改原件。

11个新增测试全在[magic.rs:3991起cfg(test)](<../../../crates/mir-server/src/magic.rs#L3991>)内；生产没有测试fixture helper。使用[通用合成MagicInfo:4339起](<../../../crates/mir-server/src/magic.rs#L4339>)（如DelayBase1000），不是实际五技能默认资料；没有测试真实耗蓝与2500ms dash冷却、整数截断、低等级随机失败。Frost的[毒断言:4628–4634](<../../../crates/mir-server/src/magic.rs#L4628-L4634>)遍历可能为空的列表，不要求触发一次Slow/Frozen，且只验首个tick_at，**可在状态未产生/不影响AI时绿灯**。不是删测，而是假绿覆盖缺口。P03/P10。

`git ls-files target/round51`为空：兵部日志、矩阵、返工脚本是本地ignored证据，**不在功能提交中**；提交正文只说证据路径，不能据此声称已git归档。manifest登记逐文件SHA/字节；异机复核仍需保全原始target证据或另走批准的归档流程。没有发现“测试代码只在本地不入库”：11个测试与两生产文件同在功能提交。

审计前`git status --porcelain`仅两个既有未跟踪冻结JSON：[before](<../monster-native/player-resource-freeze-before.json>)、[after](<../monster-native/player-resource-freeze.json>)。两提交均未包含它们；审计产物新增后另有本轮审计目录未跟踪，这是授权产物，不混称原树脏。工作区hash不等同git blob：例如[net.rs](<../../../crates/mir-client/src/net.rs>)工作区357333 B、git blob348639 B，差8694 B恰为CRLF/LF；git未报修改，manifest分别标注。

## F．文档、M1–M7与降级裁决：PARTIAL

- M1/M2：固定毒云/困魔保留拒绝符合；耗材仍未接，未看到礼部宣称已接。[礼部技能表:36–42](<../../process/round51-libu-docs.md#L36-L42>)明确五技能本批未实现/未测、两技能固定PARTIAL。
- M3/M4：真实命令、非零例数、唯一实现写者基本符合；随机/控制行为断言和C真实状态链不够，不能用全套exit0冲抵P03/P07/P10。
- M5：终审在提交后权威全量之后进行，符合当前直接派发事实。旧契约原定T→功能提交；现用户明确要求审已提交13f7c78，**按新指令审，不臆造未提交状态**，未代Lead提交。
- M6/P12：R50 P11修复/验收状态、250/251字段、库存规则更新有源链接；旧缺陷历史与P02–P08/P10未被抹去。但[gameshop.md:38–44](<../../world/gameshop.md#L38-L44>)、[client.md:867–872](<../../client.md#L867-L872>)仍称“当前取证HEAD716db42、本批文档尚未提交”。716可保留作历史基线，但**“尚未提交”在7802eb9后已过期**；当前状态未清到13f7c78，P09。
- M7：Fencing拒绝未删；未把labels/色块当视觉实证。但[七行矩阵:21](<../../../target/round51/b/07-seven-row-matrix.md#L21>)、[摘要:40](<../../../target/round51/b/00-summary.md#L40>)与功能提交正文称五技能完整，与其自列缺口及本审发现冲突。
- 五条裁决：允许裁剪并逐项PARTIAL，不允许必需语义遗漏仍算完整；C协议子层不等于C整体/视觉PASS；文档仅作R50事实基线；原门禁入口保留；PetSave/PetExp列后续而不造R52单。**本报告采纳五条，不擅自扩功能补实现。**

## G．提交与入库卫生：范围PASS，措辞需纠正

两提交共8条唯一文件路径：文档批6文件、功能批恰2文件；`git ls-tree -rl`核每个blob，最大为客户端blob348639 B，均<1MiB；无target、冻结44/45MB JSON、环境文件或密钥文件入库。实现diff及指定密钥/捷径模式检查未发现凭据；这是所审变更范围结论，非全仓秘密泄漏认证。逐文件清单/大小/hash见[manifest审计索引](<final-manifest.json>)。

文档批实际393 insertions/11 deletions。功能批：**net +39/−0，magic +1481/−20（共1501条变动）**；材料中把magic写成“+1501/−20”不准确，应更正，不把总变动量再当新增量。标题“feat(magic)…包消费”与代码改动相符；正文“五技能完整语义”不成立，且正文引用提交前attempt2，最终接受证据须指attempt3。不为修文案改写/强推已同步历史，建议后续正常修订提交或验收更正记录。

## H．假绿／捷径总检

| 项目 | 结论与依据 |
|---|---|
| 测试helper进入生产 | 未发现；R51 helper均在cfg(test)，无生产GM造状态。 |
| 吞失败/门禁假码 | 原始失败与返工保留；Code来源LASTEXITCODE，未发现改失败为0。既有脚本选择范围见A。 |
| 随机放行/硬编码成功 | 未发现随机放行Unsupported、@revive或免费施法；随机门来自原作。但Frozen时长5是无依据硬编码替代，非完整移植。 |
| 删除/放宽断言 | 新测试11个在库，Fencing保留；返工35条只改消息。未发现删断言；Frost空列表for断言、无真实状态链是覆盖漏洞。 |
| 跨动作借帧 | **存在明确降级**：dash接Walking/Walk声音，不得据此报dash视觉完成。P08。 |
| 跨轮证据/缺资源早退 | 未拿R50结果代R51功能；本轮fixture不依赖资源早退。既有世界本地化测试有缺资源早退文案，不能把其PASS当Blink行为证据。 |
| 冒充包链 | 服务端真实输出→ServerPacket解码是真；到客户端状态断言未建立，不能改称端到端闭环。P07。 |

## PARTIAL／可执行返工清单

| ID | 等级／结论 | 缺口与返工（原owner执行，刑部不实现） |
|---|---|---|
| P01 | 固定PARTIAL／接受 | PoisonCloud必需护身符与毒未接；继续Unsupported、拒绝且不扣MP，保留测试。 |
| P02 | 固定PARTIAL／接受 | TrapHexagon必需护身符未接；同上，不得免费施放。 |
| P03 | HIGH／BLOCK | Frost：使Slow/Frozen实际约束怪物AI/速度；每种毒保留真实1000ms TickSpeed；按施法者Freezing计算Frozen；重置OperateTime；玩家路径实现或明确降级。用非空触发断言、持续行为/解除测试验证，不能仅验PoisonType。 |
| P04 | HIGH／BLOCK | Dash：补CanWalk/受困前置；零步也设置CellTime/StepCounter；按HumanObject override处理低等级敌玩家Pushed/ObjectPushed；火墙中断/失败聊天或经批准逐项PARTIAL。分别测非零StepCounter失败、受控、等/低等级敌玩家。 |
| P05 | HIGH／BLOCK | 雷击冻结施法时map，跨图后不能在新图旧坐标伤害；加500ms前移动/换图及StormEscape传送失败保留伤害测试；核实地形cell.Valid差异。 |
| P06 | MED／PARTIAL | Blink/StormEscape自机ObjectEffect、真实TemporalFlux buff生命周期及效果/修炼/buff顺序；补自机/观察者独立包序、30s到期与30%耗蓝断言。 |
| P07 | HIGH／BLOCK | 建立真实World输出→解码→WorldSnapshot状态测试，覆盖122/150/151/152/153及推移127/128，协议定义见[Pushed:3978](<../../../crates/mir-protocol/src/packets/server.rs#L3978>)／[ObjectPushed:4011](<../../../crates/mir-protocol/src/packets/server.rs#L4011>)。不只手造包回环；旧net28/ui197不充新增验收。 |
| P08 | MED／PARTIAL | dash Walking借帧、ObjectPushed未消费、无原作视觉运行证据；独立标视觉未完成，或按授权补动作/受推消费与证据，勿借跨动作帧报完整。 |
| P09 | MED／PARTIAL | 活动文档去掉“尚未提交”的当前断言；明确当前审计HEAD与历史716取证基线，保留历史。 |
| P10 | MED／PARTIAL | 修七行矩阵错误包序/时长/玩家推移说明，纠正+1481/−20；用真实MagicInfo/耗蓝/冷却、概率两侧、友方/边界等可失败断言补证；保全ignored原件，明确未git归档。 |

### 提交建议与复审条件

**不建议把13f7c78标为五技能完整或将主单推进Done。** 退回兵部修P03/P04/P05/P07，礼部修P09；矩阵同步P06/P08/P10，不改冻结失败证据。固定P01/P02继续保留。若降级范围，须有权角色明确接受各项必需缺口，仍不能把不同语义称完成；仅改报告标题为PARTIAL不能代替阻断缺陷处置。

修复后由Lead在新提交树执行唯一全量门禁，另取新日志/新meta，不覆盖attempt1–3；刑部按新HEAD复审。审计产物可独立纳入后续提交，但不由刑部commit/push。

### 本次复现命令与环境

```text
cwd=C:\Users\diana\Documents\github\mir-next
cargo test -p mir-server --lib magic -- --nocapture
py -3 tools/i18n/scan_hardcoded.py --player-visible --strict --fail-above 70
py -3 tools/i18n/scan_hardcoded.py --no-test --player-visible --strict --fail-above 0
py -3 tools/docs/check_links.py
git diff --check 716db4292ad6d475e92bf8b6cbd4f230f307684f HEAD
git rev-parse HEAD origin/main
git status --porcelain=v1
git show --numstat 13f7c78
git ls-tree -rl 7802eb9 <explicit six documentation paths>
git ls-tree -rl 13f7c78 crates/mir-client/src/net.rs crates/mir-server/src/magic.rs
```

实测工具链：rustc1.99.1 (`58cf8f9c3`,2026-10-13)；cargo1.99.1 (`5f94df478`,2026-08-27)；`1.99.1-x86_64-pc-windows-msvc`；Python3.14.7；Nodev24.18.0；Git2.56.0.windows.2；PowerShell7.6.8。原件/所审源码逐文件bytes、SHA256与证据索引见[manifest](<final-manifest.json>)。本报告与manifest不互相自引用hash；三产物最终hash由Lead外部memo登记。

### 交付前自校验

manifest可解析为JSON；初次对20个所审文件与21个证据文件共41项逐一用Get-Item/Get-FileHash比对，**大小/hash不一致0项**；三个指定产物均远小于1MiB。当时HEAD与origin/main未变、git diff --check为0，跟踪文件未修改。产物落盘后的文档链接检查也exit0。未运行的整套/ignored/视觉测试仍未宣称通过。

**并行返工边界补记（UTC 2026-10-25T01:50:04.0564953Z）**：在上述初次校验之后，工作区出现非刑部所写的3个文档修改：[gameshop.md](<../../world/gameshop.md>)（01:46:52Z）、[client.md](<../../client.md>)（01:46:59Z）、[round51-libu-docs.md](<../../process/round51-libu-docs.md>)（01:49:06Z）。重新比较41项为38项未变、3项漂移；后者新大小/hash已另记manifest，不覆盖原快照索引。当时HEAD/origin仍为13f7c78。已通知Lead；**本报告是对原已提交13f7c78的T1终审，不是对后来脏树的复审，attempt3不能套用于该返工树**。P09保留原快照发现；不因看到返工文字就宣布其修复验收通过。
