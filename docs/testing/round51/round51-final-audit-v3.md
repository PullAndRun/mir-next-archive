# R51 T3 独立只读终审：BLOCK

- 主单 `JJC-20261025-003`；子单 `JJC-20261025-003-sub-d0ea04bd`；刑部/御史台，gpt-6.1-sol。
- 审计树：`main`，`HEAD=origin/main=42a49e0879be4d00bccd43bd7ebce8c9e4276058`；2026-10-25，Asia/Shanghai。
- **verdict：BLOCK。P03、P10 均未 closed。** B3补上模板/buff求和，但损坏装备仍被计入；观察者包序断言允许缺失TeleportOut。门禁通过与这两项缺口同时成立。
- 仅新建本报告、[v3 manifest](final-manifest-v3.json)、[gate6机读摘要](../../../target/round51/final-gate6-summary.log)。未改代码/旧文档/原日志，未commit/push，未跑全量或ignored。当前任务授权定向cargo复现，优先于历史派发“T3不得跑cargo”。

## A．attempt6 原件：PASS，meta缺执行上下文

独立读取[原始log](../../../target/round51/final-gate6.log)原字节：**188643 B**，SHA256 **`1e1ffa2f7a9cee641b4a1d9729722bd99158030c68b8630cfc062709ef4dacdb`**。[meta:1](../../../target/round51/final-gate6.meta.txt#L1)记录attempt6、exit0、完整HEAD、main；UTC **03:37:05–03:40:52**，227秒；bytes/sha均一致。日志末尾实有12条OK和“全部通过（12 项）”。摘要按GB18030解码、Rust段UTF-8，哈希按原字节。

| 原日志逐项名称 | 实况 | 秒 |
|---|---|---:|
| cargo fmt --all --check | OK | 3 |
| cargo clippy --workspace --all-targets -D warnings | OK | 5 |
| cargo test -p mir-server --test duo_smoke（串行） | OK | 17 |
| cargo test --workspace | OK | 195 |
| 协议生成器 --check | OK | 1 |
| 文档链接与包编号 | OK | 0 |
| TextKey 完整性 | OK | 0 |
| 硬编码棘轮（player-visible，阈值 70） | OK | 0 |
| 硬编码棘轮（no-test，阈值 0） | OK | 0 |
| 运行时不引用老资源格式 | OK | 0 |
| 物品图标完整性 | OK | 1 |
| PostgreSQL 集成测试（mir-store 串行） | OK | 3 |

整数秒合计225，与墙钟差2秒。原摘要只存OK/FAIL，逐项数字exit未另存；[脚本:38](../../../tools/dev/ci.ps1#L38)以LASTEXITCODE分类。meta缺派发要求的**完整调用命令、cwd**，日志也未打印完整调用，故不伪称独立记录了默认参数。实际workspace/duo/PG均出现，未见跳过。

attempt1–5原件继续保留，manifest重算指纹：1 exit1（player-visible 96>70；clippy OK），2 exit0（旧树、实现未提交），3 exit0（13f7c78），4 exit1（TextKey实际243/期望241），5 exit0（f500269）。不抹掉历史失败和树边界。

## B．P03：open，三来源已接，装备有效性仍错

[magic.rs:3122](../../../crates/mir-server/src/magic.rs#L3122)的`caster_freezing(&self, caster_id: u32) -> i32`依次加装备模板、附加、buff；[freeze_roll:3146](../../../crates/mir-server/src/magic.rs#L3146)正值抽0..Freezing−1，非正返回0。相对f500269只加added_stats，原漏源已实质修复。buff由[add/remove:315](../../../crates/mir-server/src/buffs.rs#L315)及[tick:454](../../../crates/mir-server/src/buffs.rs#L454)维护，装备后buff的顺序符合既有刷新。

**阻断B-P03-1：坏装备仍提供Freezing。** 当前[循环:3125](../../../crates/mir-server/src/magic.rs#L3125)没有耐久过滤；Crystal[HumanObject.cs:1836](../../../github/Crystal/Server/MirObjects/HumanObject.cs#L1836)在累加前跳过`CurrentDura == 0 && Info.Durability > 0`，Rust既有[apply_equipment_stats:912](../../../crates/mir-server/src/items.rs#L912)也实现该条件。反例：模板Freezing=7、附加=5、durability>0、current_dura=0、无buff，现函数返回12，正确有效总值应0，Frozen可错误延长。总来源修复必须沿用有效装备语义。此为静态确定反例，未改源码做变异运行。

[Crystal:1858](../../../github/Crystal/Server/MirObjects/HumanObject.cs#L1858)/1859才是主装备模板/附加；[1965](../../../github/Crystal/Server/MirObjects/HumanObject.cs#L1965)/1966属于RefreshSocketStats插槽模板/附加，旧报告及B3把1965标成AddedStats不准确。Rust[items.rs:909](../../../crates/mir-server/src/items.rs#L909)已明示插槽、等级/职业替换未实现，不据此要求本轮实现新系统；guild Freezing仍留R52。

- [三来源测试:5863](../../../crates/mir-server/src/magic.rs#L5863)精确断言7→12→15，删任一来源会失败；64次roll至少一次>0，能击穿恒0，确有非退化改善。但缺buff独有、移除/到期、坏装备用例，未满足派发全部边界。
- [附加独有:5789](../../../crates/mir-server/src/magic.rs#L5789)准确限定added=17，单独范围0..16仍允许恒0。
- [零来源:5929](../../../crates/mir-server/src/magic.rs#L5929)真实施法取得Frozen再断言5，非空有效；freeze_roll==0未比较后续RNG，不能称运行证明“不消耗随机数”，此结论只有生产分支静态证据。
- [混合实际施毒:5919](../../../crates/mir-server/src/magic.rs#L5919)只要求duration为5..19，实际始终写5也可能绿；非退化只单独约束freeze_roll，尚未证明实际施毒确实使用额外roll。需固定种子精确duration或实际duration>5的有效断言。

## C．P10：open，四项纠偏仍有新夸大

1. **零步已成立**：[5498](../../../crates/mir-server/src/magic.rs#L5498)预置step_counter=3，[5516](../../../crates/mir-server/src/magic.rs#L5516)断言0，删除清零将失败，B3该声明准确。
2. **本人包序有效，观察者缺Out仍可绿。** [4877](../../../crates/mir-server/src/magic.rs#L4877)已有Effect存在断言，`effect_at < buff_at`也会拒绝缺Buff。观察者[4910](../../../crates/mir-server/src/magic.rs#L4910)三个position返回Option<usize>，直接断言`out_at < in_at && in_at < observer_effect_at`；Rust `None < Some(_)`为真，独立存在断言只覆盖In/Effect。反例`None, Some(0), Some(1)`满足表达式。故[B3矩阵:14](../../../target/round51/b3/07-seven-row-matrix.md#L14)/[25](../../../target/round51/b3/07-seven-row-matrix.md#L25)严格Out<In<Effect覆盖仍不充分。应先expect三者再比较整数；未实施源码变异探针。
3. **真实非空Frozen成立**：[helper:5817](../../../crates/mir-server/src/magic.rs#L5817)最多400次cast_magic+结算，找Frozen否则panic，无手工注入Frozen。重置冷却/MP/血量是来源测试fixture，不能证明真实耗蓝冷却。旧手工毒消费测试已明确区分；真实Slow/概率两侧仍未测。
4. **全关闭声明已撤回**：[摘要:30](../../../target/round51/b3/00-summary.md#L30)、[矩阵:27](../../../target/round51/b3/07-seven-row-matrix.md#L27)保留PARTIAL及待T3。未发现“全部关闭/未决无”复活；但包序漏洞和RNG运行证明夸大须纠正，P10仍open。

## D．PARTIAL诚实性：基本PASS，补充边界

[摘要:58](../../../target/round51/b3/00-summary.md#L58)/[矩阵:37](../../../target/round51/b3/07-seven-row-matrix.md#L37)保留P01/P02 Unsupported、Frost玩家目标、火墙简化、无InTrapRock、P07的152/153敏感回纠、Walking/Walk借用及原生视觉未验。Frozen消费接线不等于Frost玩家目标完成。

[R52:48](../../../target/round51/b3/07-seven-row-matrix.md#L48)保留真MagicInfo耗蓝冷却、真实非空Slow/概率两侧、StormEscape buff→LevelMagic严格包序、玩家Frozen拒攻、独立跨图、火墙逐对象、原生dash及真socket。非零StepCounter已在B3补，原R52应转已补；152/153仍留。Frozen概率两侧、观察者Out存在性、buff失效边界也须补列。

## E．可复现：PASS，绿测试不排除缺口

[B3 magic原日志:1](../../../target/round51/b3/01-magic-tests.log#L1)含命令/cwd/旧基线HEAD，实际11:32:54–11:33:00+08，31 passed/0 failed/0 ignored、raw exit0。[fmt/clippy:6](../../../target/round51/b3/02-fmt-clippy.log#L6)各exit0，实际clippy范围server/client；hash见manifest。

T3按当前任务独立复跑，TMP=TEMP为仓库target/tmp（L2）：

- `cargo test -p mir-server --lib magic`：31 passed/0 failed/0 ignored，exit0；cargo构建0.61s/测试0.00s；job `bash-cb4111299d144809`已收取。
- `cargo test -p mir-client --test round51_spells_wire`：2 passed/0 failed/0 ignored，exit0；cargo构建3.15s/测试0.00s；job `bash-c338795b45c64d86`已收取。

时间为cargo阶段输出，非外部墙钟；无需重复L3全量。

额外L1编译探针没有创建源码或修改仓库：把`out_at: Option<usize>=None; in_at=Some(0usize); effect_at=Some(1usize); out_at<in_at && in_at<effect_at`作为函数经stdin送给`rustc --crate-type=lib -O --emit=llvm-ir -o - -`。exit0，LLVM IR为`ret i1 true`，直接确认缺Out仍满足原表达式。rustc版本1.99.1；这不是对仓库源码做变异测试。

## F．文档一致性与全部发现的偏差

[T2报告](round51-final-audit-v2.md)/[manifest](final-manifest-v2.json)审f500269，其BLOCK是合理历史记录；[README:3](README.md#L3)的待attempt6/T3为42a49e0提交时进度。本报告补后续事实，旧件不覆盖。

1. [派发:23](../../process/round51-shangshu-rework-dispatch.md#L23)禁止B3提前补StepCounter/真实施毒/严格包序，实际均补；生产写文件仍只magic。属契约范围偏差，请Lead/尚书记录裁决。
2. 派发B3子单为`sub-3b310e70`，B3摘要/矩阵为`sub-28585983`，身份映射需说明；不重试已Done单回填。
3. [B3快照:19](../../../target/round51/b3/03-summary.md#L19)写magic开始11:31:19，原日志实际11:32:54，结束同为11:33:00；以原件为准。
4. [快照:30](../../../target/round51/b3/03-summary.md#L30)写b2的28+新增3=31；实际B3新增两个test、重命名一个，b2为29，29+2=31。new/changed三项名单正确，不能把改名计新增。
5. 派发要求buff独有/失效边界，现只有模板独有、附加独有、混合、零值；不能称全部边界已验。
6. 零来源“不消耗RNG”未作运行状态比较；真实施毒5..19不证明实际延长。
7. 严格观察者包序声明超出Option比较保证（C节阻断）。
8. Crystal1965为插槽模板累加，主装备附加实际1859；B3、T2、派发引用不精确。
9. B3 clippy只server/client，派发要求workspace；attempt6的workspace通过不能冒充B3命令记录。
10. attempt6 meta缺完整命令/cwd；[ci脚本:6](../../../tools/dev/ci.ps1#L6)旧注释写13项，默认原日志实际12项；按原件判定。
11. T2 manifest所列未跟踪v2文件已在42a49e0入库，为历史快照；B3 f500269也是当时快照，无需改写为当前HEAD。

新文档落盘后执行`PYTHONIOENCODING=utf-8 py -3 tools/docs/check_links.py`：**1157条相对链接、2篇README、exit0**；manifest另记实况。本地ignored target原件需Lead另行保全，hash索引不能恢复原件。

## G．入库卫生：PASS（有界检查）

`git diff 716db42..HEAD --name-only`为27路径；对应`git ls-tree -rl HEAD`最大blob为client/net **350031 B**，magic **259559 B**。无>1MiB变更blob、无target/.env/私钥路径入库。本次相对f500269仅magic+四文档，共511增/13删；B3实现225增/10删。

开始未跟踪仅[before冻结JSON](../monster-native/player-resource-freeze-before.json) **44844165 B**与[after冻结JSON](../monster-native/player-resource-freeze.json) **45219533 B**，保留排除。T3自身只新增两份v3文档、ignored摘要。**11:50:35收尾共享工作树另出现外部对magic的修改和[B4派发新文档](../../process/round51-shangshu-rework-dispatch-b4.md)，HEAD仍42a49e0。** 本报告审计与定向复跑针对此前未修改的新提交树；manifest代码指纹固定取git HEAD blob，后续B4工作树不受attempt6结论覆盖。未代修或覆盖同期改动。秘密扫描范围/结果见manifest，不外推历史全库无密钥。

## H．假绿/捷径与250/251：未见旁路，有断言漏洞

已读f500269→1fe36dd全magic diff，未新增allow/skip/ignored，未删或放宽既有业务断言；旧Freezing测试仅改名、收窄说明并保留断言。真实Frozen helper未注毒；fixture冷却重置不证明冷却。`git diff 716db42..HEAD -- crates/mir-protocol tools/dev/ci.ps1`为空，250/251无本轮diff，门禁无本轮绕过修改。HEAD/sha吻合；无伪造迹象。

**Option包序漏洞可导致假绿；坏装备Freezing为生产缺口。** 31/31、2/2和12项OK不能替代修复。

## P01–P10状态

| 编号 | T3状态 | 理由及残余 |
|---|---|---|
| P01 | PARTIAL | PoisonCloud真Unsupported，耗材/语义未完成 |
| P02 | PARTIAL | TrapHexagon同上 |
| P03 | **open** | 三来源已接，坏装备误计；玩家目标另PARTIAL，buff/实际延长补证 |
| P04 | PARTIAL | 非零StepCounter已补；火墙逐对象/InTrapRock留缺口 |
| P05 | closed | 施法图冻结修复保持；StormEscape独立跨图R52 |
| P06 | closed | 真实TemporalFlux/自机效果保持；StormEscape严格包序R52；Blink测试漏洞归P10 |
| P07 | PARTIAL | wire两例复跑通过；152/153敏感回纠仍缺 |
| P08 | PARTIAL | Walking借用、原生dash视觉/时序未验 |
| P09 | closed | 243口径与历史时态修正保持；本次流程偏差见F |
| P10 | **open** | 四项纠偏有实质改进，严格Out包序/RNG运行声明仍超量 |

## 必补与建议

本轮先修有效装备耐久过滤与健康/损坏来源断言、buff独有/移除/到期边界、实际Frozen延长验证、包序三包先expect再比较；更正B3时间/计数/引用/覆盖范围。Lead对新提交执行唯一新attempt，不覆盖1–6。

R52：真实MagicInfo耗蓝/冷却含2500ms、Frost两概率两侧/真实非空Slow、StormEscape严格包序、152/153错误预测格+活动motion下回纠/Standing/清队列、玩家Frozen实际拒攻、StormEscape独立跨图、火墙逐对象/InTrapRock、Frost玩家目标、P01/P02耗材、原生dash视觉/声音、guild Freezing及真socket。非零StepCounter/真实非空Frozen已补，不能抹掉其余残余。

**建议保持BLOCK，精确返工后复审，不推进主单Done。** 修复并有权接受PARTIAL后才考虑PASS-WITH-PARTIAL。只向Lead提出v3产物提交建议，T3不提交；manifest记录报告最终hash，自身hash外部消息回报，避免循环自引用。
