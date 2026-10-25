# R51 T4b 独立只读终审：PASS-WITH-PARTIAL

- **verdict=PASS-WITH-PARTIAL**；**P03 closed，P10 closed**（本轮有效装备来源、实际Frozen延长、五包存在性与证据纠偏范围）。七技能完整实现与主单Done不在本结论内；PARTIAL须由有权角色接受。
- 主单 `JJC-20261025-003`，子单 `JJC-20261025-003-sub-1c3b4c52`；刑部/御史台独立T4b，gpt-6.1-sol。审计开始2026-10-25T19:42:08+08:00。
- 仓库 `C:/Users/diana/Documents/github/mir-next`，main，**HEAD=origin/main=8cbe71332ff3dc2037b9bea4ab564f6fb3ff0e75**；起始无tracked修改，仅两份大冻结JSON未跟踪。本次只新增本报告、[v4 manifest](final-manifest-v4.json)、[gate8机读摘要](../../../target/round51/final-gate8-summary.log)，未改代码/旧文档/原日志/Git，未跑全量门禁。
- Git实际父子链：**f500269 → 1fe36dd → 42a49e0 → 0d27e5e → edad41b → 8cbe713**。交接把42a49e0和1fe36dd先后写反；42a49e0为B3后的T2文档提交，T3 v3入库在edad41b。

## A．attempt8原件：PASS

独立读取[原日志](../../../target/round51/final-gate8.log)原字节，**188937 B**、SHA256 **817f531c078679bb8d4519576b1a155f2119aae3b92e7fcfec502fda6ff28510**，与[meta](../../../target/round51/final-gate8.meta.txt#L1)一致。meta实有attempt8、exit_code=0、完整HEAD/main、命令 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`、cwd `/c/Users/diana/Documents/github/mir-next`、UTC **11:39:18–11:41:48**、150秒。末尾3215–3226行实况如下，3227行为全部通过12项。门禁中文按GB18030解码，Rust段含UTF-8，哈希始终按原字节。

| 原日志逐项名称 | 状态 | 秒 |
|---|---|---:|
| cargo fmt --all --check | OK | 3 |
| cargo clippy --workspace --all-targets -D warnings | OK | 5 |
| cargo test -p mir-server --test duo_smoke（串行） | OK | 17 |
| cargo test --workspace | OK | 119 |
| 协议生成器 --check | OK | 1 |
| 文档链接与包编号 | OK | 0 |
| TextKey 完整性 | OK | 0 |
| 硬编码棘轮（player-visible，阈值70） | OK | 0 |
| 硬编码棘轮（no-test，阈值0） | OK | 0 |
| 运行时不引用老资源格式 | OK | 0 |
| 物品图标完整性 | OK | 1 |
| PostgreSQL集成测试（mir-store串行） | OK | 3 |

整数秒合计149，墙钟150；逐项数字exit未单独留存，不把OK分类冒充逐进程独立采集。门禁[Invoke-Gate](../../../tools/dev/ci.ps1#L33)按真实LASTEXITCODE分类。workspace、duo、PG均实际执行，未以SkipTests/SkipPostgres取代。

attempt1原件名为final-gate.log。attempt1–5的log/meta重算SHA与[v3清单](final-manifest-v3.json#L147)逐一一致；attempt6原日志与v3一致；attempt7为189032 B / f734bc55c78e5e718c7ebf5f463af2093ed6ff1124193dc965fd94f366f17477，与meta和校正说明一致。历史失败attempt1/4的exit1与旧HEAD保留。全部指纹见manifest；本次未覆盖历史原件。

## B．P03：closed

[生产汇总](../../../crates/mir-server/src/magic.rs#L3123)取得模板后，在模板/附加求和之前执行 `if item.current_dura == 0 && info.durability > 0 { continue; }`；损坏装备两种来源一起排除，buff求和在装备循环外独立进行。对应Crystal [1836耐久判断](../../../github/Crystal/Server/MirObjects/HumanObject.cs#L1836)、[1858/1859普通来源](../../../github/Crystal/Server/MirObjects/HumanObject.cs#L1858)与Rust [apply_equipment_stats](../../../crates/mir-server/src/items.rs#L912)。durability==0不会被过度过滤。[freeze_roll](../../../crates/mir-server/src/magic.rs#L3156)正值next_range(0,freezing-1)，非正直接0。

B4恰新增五例、无改名，31→36：

| 用例 | 可失败断言与准确边界 |
|---|---|
| [damaged_equipment_does_not_contribute_freezing](../../../crates/mir-server/src/magic.rs#L5986) | 模板7+附加5、durability10000/current0→汇总0、真实Frozen=5；恢复current10000→12，roll出现正值。删过滤或只过滤一个来源会失败。 |
| [no_durability_equipment_still_contributes_freezing](../../../crates/mir-server/src/magic.rs#L6061) | durability0/current0、模板Freezing5→汇总5、roll出现正值；删durability>0条件会失败。该例未设置非零附加，不能声称单独运行覆盖此组合。 |
| [buff_freezing_alone_extends_the_frozen_roll](../../../crates/mir-server/src/magic.rs#L6105) | 无装备，buff3→3，roll出现正值；remove后0，重新添加3，到期tick后0。漏buff、移除/到期无效均会失败。真实Frozen范围5..7单独仍容许恒5，由下一例补非退化证明。 |
| [frost_crunch_real_cast_uses_the_freezing_roll](../../../crates/mir-server/src/magic.rs#L6160) | 固定种子0..23经真实cast/延迟结算，必须至少一次duration>5；实际施毒恒5必失败。范围上限12比Freezing7理论上限11宽一格，不能据此称精确上界已测。 |
| [frost_crunch_without_freezing_always_lasts_five_ticks](../../../crates/mir-server/src/magic.rs#L6212) | 两种固定种子无来源，真实非空Frozen均精确5。 |

原有[三来源测试](../../../crates/mir-server/src/magic.rs#L5906)精确7→12→15；[附加独有](../../../crates/mir-server/src/magic.rs#L5821)精确17，B4改为存在的无Freezing模板。真实Frozen [helper](../../../crates/mir-server/src/magic.rs#L5860)执行cast_magic、推进时钟、resolve_pending_magic，400次无Frozen则panic，没有直接注入Frozen。重置MP/冷却/血量仅为来源fixture，不证明真实耗蓝冷却。

**本次独立唯一 `TMP=TEMP=<repo>/target/tmp cargo test -p mir-server --lib magic`：36 passed / 0 failed / 0 ignored / 150 filtered，exit0**，cargo构建0.66s、测试0.01s；host job bash-a4e08c07662547e0已收取。阶段时间不是外部墙钟。未实施源码变异红灯运行，所述“删除则失败”为断言逻辑核验。

零来源不调用RNG仍只有生产分支静态证据，未做相同种子后续RNG状态比较；坏装备+有效buff、无耐久装备+非零附加未独立运行。结构可静态说明其正确性，本报告不冒称已跑；列作补证，不把已修复的生产耐久缺陷重新判open。

## C．P10：closed

[Blink本人](../../../crates/mir-server/src/magic.rs#L4887)Effect/Buff两项is_some、[观察者](../../../crates/mir-server/src/magic.rs#L4930)Out/In/Effect三项is_some，均先于各自排序执行；缺任一所需包必先panic，T3的None<Some反例不再能绿。**排序语法仍为Option比较**，未改为expect后的整数变量；但前置断言支配比较、保证全部Some，其排序与内部整数索引完全一致，不误称已改写整数语法。

[零步测试](../../../crates/mir-server/src/magic.rs#L5522)施法前step_counter=3、后断言0，删清零会失败；真实非空Frozen见B。两项均在本次36例运行。[新矩阵](../../../target/round51/b4/07-seven-row-matrix.md#L22)和[摘要更正记录](../../../target/round51/b4/00-summary.md#L41)纠正时间、29+2=31/改名计数、两crate clippy、1965插槽、零RNG静态证据、恒5风险、buff生命周期、原Blink假绿；映射由附录纠正。未见“全部关闭/未决无”复活；StormEscape严格包序继续R52，未称七技能均有运行包序。

## D．PARTIAL/R52诚实性：PASS

[B4摘要](../../../target/round51/b4/00-summary.md#L66)与[矩阵](../../../target/round51/b4/07-seven-row-matrix.md#L33)保留P01/P02 Unsupported、Frost玩家目标、火墙简化/InTrapRock、P07 152/153、P08 Walking/Walk借用。R52必须保留：

1. PoisonCloud/TrapHexagon耗材及完整语义；Frost玩家目标PvP。
2. 真实默认MagicInfo逐技能耗蓝冷却，包括ShoulderDash2500ms。
3. 真实非空Slow、Slow/Frozen概率两侧与玩家/怪边界。
4. StormEscape buff→LevelMagic严格包序、专门跨图运行例。
5. 玩家Frozen实际拒攻。
6. 152/153错误预测格+已有motion的回纠/Standing/清队列不可删证明。
7. 火墙逐对象ProcessSpell、InTrapRock系统边界。
8. 原生dash帧/视觉/声音/时序。
9. guild Freezing来源映射、真socket端到端；当前为World→编码→decode。
10. 多槽durability>0装备叠加及Crystal [1965/1966插槽属性](../../../github/Crystal/Server/MirObjects/HumanObject.cs#L1965)。
11. 补强零RNG状态对照、坏装备+有效buff、无耐久非零附加；收紧Freezing7时长上界。

## E．可复现：PASS

B4 [magic原日志](../../../target/round51/b4/01-magic-tests.log#L1)含命令/cwd/基线完整HEAD、时区、11:56:27–11:56:34+08、raw exit0、36例；[fmt/clippy](../../../target/round51/b4/02-fmt-clippy.log#L1)含11:56:34–11:56:51+08及各exit0。[i18n](../../../target/round51/b4/04-i18n-ratchet.log#L99)实有67≤70、no-test0、两项exit0及命令/cwd/时间。它们是B4尚未提交的42a49e0基线快照；当前已提交blob与工作树一致，attempt8与本次复跑覆盖新树，未用旧树冒充。

client两例未另跑cargo，attempt8原日志802–809行的[wire测试](../../../crates/mir-client/tests/round51_spells_wire.rs)两例均ok。独立 `py -3 tools/docs/check_links.py` 已exit0、1164条相对链接/2篇README（写入新报告之前实况）；最终检查结果另存manifest。未重复全量/clippy/i18n，遵循本次直接授权的成本限制。

## F．文档一致与追溯：PASS，轻微残留明确披露

[权威校正说明](round51-b4-traceability-note.md#L6)、[README](README.md#L3)、[B4摘要附录](../../../target/round51/b4/00-summary.md#L80)、[矩阵附录](../../../target/round51/b4/07-seven-row-matrix.md#L63)、[派发](../../process/round51-shangshu-rework-dispatch-b4.md#L21)一致：**B4=sub-e2cb39cb，B3=sub-3b310e70，T3=sub-d0ea04bd，T4=sub-1c3b4c52；sub-28585983为B2**。原头部错误已有显式附录纠正，不再算未修BLOCK。

T3 §F十一类偏差逐条处置：①B3提前补测范围偏差由[续派](../../process/round51-shangshu-rework-dispatch-b4.md#L11)保留交Lead裁决，不倒填授权；②身份附录纠正；③时间纠正；④29+2=31，改名不计新增；⑤buff独有/失效补齐；⑥零RNG降为静态、真实延长补证；⑦五包存在支配排序；⑧普通1858/1859、插槽1965/1966生产doc与新矩阵纠正；⑨两crate自测如实、workspace归attempt8；⑩新meta补命令/cwd，旧脚本13项注释不覆盖实际12项；⑪旧HEAD/未跟踪清单是历史快照，不覆盖。

剩余不一致：

- README/校正说明截至attempt7；attempt8在8cbe713提交后运行，由本报告追加。索引不再写B4待提交，不要求同一被审提交预记其后门禁。
- 当前旧测试注释[5823](../../../crates/mir-server/src/magic.rs#L5823)、[5908](../../../crates/mir-server/src/magic.rs#L5908)仍把主装备附加错引1965；[4899](../../../crates/mir-server/src/magic.rs#L4899)与矩阵写“四包”实际五个位置。这不是历史文件快照，是当前轻微引用/计数文字残留，后续应修文；不影响实际来源/五项assert，不假称已全改。
- 派发要求workspace clippy/--nocapture/RNG状态对照，B4实录两crate/无nocapture/静态RNG；已如实区分，workspace另由attempt8覆盖，缺边界列D。本次直接任务允许一次magic，优先于历史T4“不cargo”条款。
- B4摘要把42a49e0说成含T3文档、交接链前两项倒置，以本次Git父子核验为准。旧审计BLOCK与旧HEAD清单为合法历史，不构成当前生产树失败。

未发现仍改变验收范围、证据归属或生产行为的实质矛盾；现行轻微文字错误不隐去，建议Lead后续文档提交处理。

## G．入库卫生：PASS（有界）

`git diff 716db42..HEAD --name-only`为31路径；对应HEAD blob最大client/net350031 B、world307947 B、magic272611 B；**无本轮新增/改动blob>1MiB，无target/.env/密钥路径入库**。全HEAD已有1228554 B的attack3资产计划JSON，同blob已存在716db42，不误报本轮新增。新增行对私钥头、AWS AccessKey、GitHub token格式有界扫描0命中，不外推全历史无秘密。

两份冻结JSON继续未跟踪排除，原字节：before **44844165 B / 70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de**；after **45219533 B / c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32**。三份新产物各≤1MiB；未暂存/提交。

## H．假绿/捷径：PASS

已读0d27e5e与8cbe713完整diff。B4仅生产有效装备过滤、补五例及五包存在断言，附加测试换存在模板；未新增allow/ignore/skip，未删或放宽既有断言；8cbe713只改索引和新增追溯说明。真实Frozen helper无手工注毒，手工消费测试在矩阵明确区分；未改生产包序凑绿。

`git diff 716db42..HEAD -- crates/mir-protocol tools/dev/ci.ps1`为空，协议250/251与门禁无本轮diff。工作树magic的git hash-object与HEAD blob均为84a88d804c9cf105b76fbc2cb6dfb6c4ff5a2c2b；HEAD/origin/meta/原字节sha吻合，未见伪造。断言可失败性为静态逻辑分析加正向运行，未冒充源码变异红灯实验。

## P01–P10及建议

| 编号 | 状态 | 准确范围 |
|---|---|---|
| P01 | PARTIAL | PoisonCloud Unsupported，耗材未实现 |
| P02 | PARTIAL | TrapHexagon同上 |
| P03 | closed | 三来源/耐久/实际延长阻断闭合；玩家目标另PARTIAL，边界补证见D |
| P04 | PARTIAL | StepCounter3→0已补；火墙逐对象/InTrapRock保留 |
| P05 | closed | 雷击施法图冻结保持；StormEscape专门跨图R52 |
| P06 | closed | Blink五包存在/包序有效；StormEscape严格包序R52 |
| P07 | PARTIAL | 真实wire两例绿；152/153敏感回纠缺口 |
| P08 | PARTIAL | Walking/Walk借用，原生dash未验 |
| P09 | closed | 主时态/追溯纠正；轻微文字残留见F |
| P10 | closed | 旧假绿反例被前置断言堵住，声明边界纠正，无全部关闭复活 |

建议Lead保全ignored原日志/B4附录，提交本报告与v4 manifest，不含冻结JSON或target；尚书/门下按权限接受明确PARTIAL后处理后续状态，**不直接宣布主单Done**。报告hash入manifest，manifest自身hash外部消息回报，避免自引用。最新直接任务只允许三份产物，优先于通用review技能额外HTML/成本表要求。
