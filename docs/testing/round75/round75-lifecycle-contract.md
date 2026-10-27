# R75 149 生命周期差异合同

阶段：**D 文档批、待独立终审**；`stage=pre-audit`；[范围/证据/七项PARTIAL](README.md)。仅新增测试锁，生产零改；编号以已提交测试原名为准。

|G / 测试后缀|精确固定预期与边界|
|---|---|
|G1 initial_quantization_and_equality|模型start1234、interval120→next1300；1299帧0，1300帧1/next1420；真实149生产age66画1630，模型1631；各自expected，不要求相等。|
|G2 sparse_process_is_one_step|模型start0合法合成输入→next100；process2000仅帧1/next2120，再2120帧2/next2240；生产age2000画1634；无while追赶。|
|G3 repeat_wrap_is_not_removal|纯模型FireWall tick100/220/340/460/580/700回卷0、Ended=true、removed=false、visible=true；820帧1且Ended仍true。PoisonCloud 20次tick同样回卷。|
|G4 product_window_is_4000_not_2000|真实149 FireWall/PoisonCloud，age -1/4000/4001空；0/1999/2000/3999可见；3999索引1633/1663；signed offset一次、整帧金样。4000为AREA_MS产品窗，2000仅内部采样。|
|G5 remove_field_leaves_transient_event|真实encode→decode→apply：149(A=7001/B=7002)@1000→26(A)@1100；fields只剩B，events仍[id1,id2]、原object_id/started_ms不变；draw@1120仍两处1631。参考remove(A)仅A不可见。|
|G6 draw_is_query_and_reentry_appends|接G5线序再149(A)@1200换格；fields为[B,新A]共2，events旧A/B/新A共3，事件id[1,2,3]、起点[1000,1000,1200]；draw@1240 append三op=1632/1632/1630；两次整帧相等，两表逐字段不变。|
|G7 nonrepeat_hides_without_remove|纯模型Blizzard count30/interval100/nonrepeat，100..3000→frame30隐藏未删，3100→31仍未删；显式remove才removed=true；Effect next80时80不进、81进，SpellObject等号进。|

- G1/G2/G4/G5/G6调用生产draw_sprites；G3/G7纯参考模型，不计生产寿命覆盖。149→26→149线序、对象ID与事件ID由G5/G6联合覆盖。
- Crystal [SpellObject:288–302](../../../github/Crystal/Client/MirObjects/SpellObject.cs#L288)：初始100ms向下取整、`>=`、每Process一步、`next=now+interval`；[Effect:81](../../../github/Crystal/Client/MirObjects/Effect.cs#L81)严格`>`不可混用。
- Rust [net:5322–5333](../../../crates/mir-client/src/net.rs#L5322) 26只删fields不清events；[5624–5647](../../../crates/mir-client/src/net.rs#L5624)149同写两表；[spellfx:287–389](../../../crates/mir-client/src/ui/spellfx.rs#L287)消费events而不读fields。

|变异|隔离变更 / 指定断言红|
|---|---|
|M1（模型）|`now>=next`→`now>next`；G1 (frame,next)=(0,1300)，预期(1,1420)。|
|M2（模型）|`next=now+interval`→`next+=interval`；G2 (1,220)，预期(1,2120)。|
|M3（模型）|Repeat回卷额外removed=true；G3 (ended,removed)=(true,true)，预期(true,false)。|
|M4（生产副本）|AREA_MS 4000→2000；G4 age2000 pixel(391,262)=[0,0,0,255]，预期[248,0,248,255]。|
|M5（生产副本）|26额外清events；G5事件[id2]，预期[id1,id2]；不评价未来修复设计。|

- 各变异编译EXIT0→指定exact断言红101→原字节SHA恢复→七绿；M1–M3模型变体文件SHA另记，断言/expected/fixture不改；M4/M5测试SHA不变。详见[A](../../../target/round75/a/attempt-1/manifest.json)、[B](../../../target/round75/b/attempt-1/manifest.json)、[C](round75-c-evidence.md)。
- 覆盖分别披露：B oracle82/82 agree、12个frame samples；C13个整帧case+63个模型采样、125 agree/0 mismatch。C3静态55 PASS/0 FAIL不冒充重跑矩阵；[三次复核链](README.md#门禁与证据复核链)。

七项PARTIAL台账（详细原号见[索引](README.md#七项-partial-与历史原号)）：

1. GAP-1/2、R70-C2完整DrawBlend/rate/Mode/DrawColour；`src×0.8×alpha`近似未实施。
2. GAP-5/R70-C1其余149技能/1620/起手，R70-C3及GAP-3/4/6 mask/tint/shadow/Light/AnimationOffset。
3. R71时钟/对象、R73-T1完整寿命；历史反向注释子项修正不关闭生产缺口。
4. R72 B plan-M4/legacy lint、R71-C7永久PARTIAL/历史E1仅补救CLOSED、R69条件5永久缺口。
5. R68/R49–R70/R61–R67原号；P01–P12/①–④/P1–P7/①–⑥/G1.2/G2/R66生命周期水位/R67容量保护map epoch不清零；R74 artifacts=34照承。
6. R75-E1/C-M1：B aborted-attempt原字节不可恢复，CLOSED-AS-PARTIAL流程项。
7. R75-E2/C-M2：A首次actual原stdout缺失，冻结早于首次actual仅声明级PARTIAL；C2任务书误引中间快照的身份冲突已由C3 CLOSED。

禁称：生产寿命修复、Crystal严格等价、Blizzard生产迁移、2000ms为当前寿命、模型为生产能力、Effect与SpellObject同一推进规则、Rust完全没有对象状态。真实素材/GPU/设备/全UI/跨后端/Live音频未覆盖；未来改生产须显式supersede现状合同并另列验收。
