# R75 门下省独立审议

- 主单：JJC-20261028-002；审议者：menxia（gpt-6.1-sol），独立于中书作者；裁决：**PASS-WITH-CHANGES（附条件准奏）**，必改0。
- 接受候选C“软件差异合同、生产零改、完整寿命仍PARTIAL”；本裁决批准执行计划，不表示未来G/M已实跑通过。
- 基线实算HEAD=origin/main=`6502a90f83a977562c20a1b5b901a5672b0410d2`；[计划](round75-plan.md)136行/16247B/SHA256=`0975b0c4c80b31e9a5e8975bff62e6e43b1ae333aadbfcff5eea922a9e3f9011`，输入一致。

|六项抽核|裁定|独立证据与判断|
|---|---|---|
|1 Crystal实读|PASS|下表原文；`>=`、初始floor100、每Process一步；Effect严格`>`且起点排期，不能复用其比较/排期规则。|
|2 Rust/协议|PASS|[net:5322](../../crates/mir-client/src/net.rs#L5322)26仅撤fields；5624双写；7424追加events/7465更新fields。[spellfx:287](../../crates/mir-client/src/ui/spellfx.rs#L287)只读events，age<0或>=4000不画；700/4000/1200，未见2000寿命。|
|3 验收可证伪|PASS|G1–G7各一条exact命令/固定断言；绿恰1 passed，红编译成功后EXIT101断言失败；M1–M5拒绝0tests/编译错。G1/2/4/5/6画整帧，G3/7纯模型；未重复R74普通NORMAL锁。|
|4 范围/DAG|PASS|仅新增一个tests文件，所有生产src零改（含注释）；wire/server/freeze禁止改；A→B→C→Lead提交+唯一全门禁→D→终审/验收/确认完整。|
|5 PARTIAL/历史|PASS|差异合同不称严格等价/生产修复；R74清单实算artifacts=34、stage=post-audit、gate=c222fe6/EXIT0/12OK；保留顶层历史限定与原文件。|
|6 缺陷/条件|PASS|无阻断性缺陷、M必改为空；下列C1–C6作为执行和终审条件，建议纠正路径/措辞但不改本计划原件。|

|Crystal实读锚点|原文/语义|
|---|---|
|[SpellObject:288–302](../../github/Crystal/Client/MirObjects/SpellObject.cs#L288)|`NextMotion = CMain.Time + FrameInterval; NextMotion -= NextMotion % 100;`；294 `if (CMain.Time >= NextMotion)`，296前增frame，302按now重排；无追赶while。|
|[SpellObject:296–300](../../github/Crystal/Client/MirObjects/SpellObject.cs#L296)|`FrameIndex = 0; Ended = true;`，Repeat回卷不移除；[328–339](../../github/Crystal/Client/MirObjects/SpellObject.cs#L328)非Repeat到count只隐藏。|
|[Effect:38–61](../../github/Crystal/Client/MirObjects/Effect.cs#L38)|46/60 `NextFrame = Start + (Duration / Count) * (CurrentFrame + 1);`。|
|[Effect:81–110](../../github/Crystal/Client/MirObjects/Effect.cs#L81)|`if (CMain.Time <= NextFrame) return;`，83 SkipFrames、87 RepeatUntil、94 Remove；严格`>`与SpellObject不同。|
|[SpellObject:30–79](../../github/Crystal/Client/MirObjects/SpellObject.cs#L30)|AddObject、Repeat=true；FireWall1630/6×120、PoisonCloud1650/20×120、Blizzard30×100/非Repeat；计划摘句行号正确。|
|[GameScene:5412](../../github/Crystal/Client/MirScenes/GameScene.cs#L5412)/[2289](../../github/Crystal/Client/MirScenes/GameScene.cs#L2289)|149 new+Load；26按ID查对象Remove（用户对象有提前return；本轮法术对象不受此例外影响）。|

- C1（A/B/C）：每轮红绿冻结同一断言/expected/夹具SHA；模型语句变异可改测试文件，必须分别记原文件SHA、patch及恢复SHA，不能强称变异版整文件SHA不变。
- C2（B/C）：物理副本nlink=1、禁硬链接/junction/symlink；独立target与TEMP/TMP/TMPDIR；只改副本，保留no-run EXIT0、指定exact断言红101及恢复七绿。
- C3（A/B/C）：oracle独立于生产helper和actual；先冻expected字节/SHA/UTC，独立解析实际fixture；G1/2/4/5/6全部采样整帧、G3/7模型轨迹分别计数。G4负龄/到期空帧可黑，非空采样必须有可见且异色offset像素。
- C4（全体）：禁止称严格等价实现/生产寿命修复/新Blizzard映射；不可把“不同状态机”扩大成不能共用底层时间来源，禁止把2000ms或Effect严格比较套入SpellObject。
- C5（全体）：attempt证据只增不改；R71–R74证据/两freeze原字节保全；仅Lead提交及运行唯一全量门禁，不SkipTests/SkipPostgres；D后只有crates/tools再改才重闸。
- C6（Lead/D）：G1–G7/M1–M5全矩阵和两类覆盖数齐备、C必改0才门禁；沿原号保留PARTIAL，R74按34项披露，不把旧题面35当事实。
- M必改：**无（0项）**；执行中任一C不满足即退补证，不得用本次准奏代替验收。
- 非阻断建议：计划112“35口径待Lead核定”后续文档直接写实数34；用户给的协议路径应定位到[packets/server.rs](../../crates/mir-protocol/src/packets/server.rs#L4633)（149:4633/26:762/125:3903），无须创建错误路径；load“正时戳”与G2 start0应明确零为合成时钟合法输入。
- CLI：`node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs state JJC-20261028-002 Assigned --as menxia`；UTC `2026-10-27T19:13:47.424Z–19:13:47.503Z`，EXIT0，原stdout：`✅ JJC-20261028-002 状态更新：Menxia → Assigned`，stderr空。
- 随后`validate --as menxia`：UTC `2026-10-27T19:13:47.504Z–19:13:47.566Z`，EXIT0，原stdout：`✅ 看板校验通过：247 条任务符合 schema v3.1`，stderr空。
- 证据：[notes](../../target/round75/menxia-review/notes.md)、[checks](../../target/round75/menxia-review/checks.json)、[原始日志](../../target/round75/menxia-review/logs/)；本次未运行cargo/ci.ps1，未commit/push，未改计划/crates/历史文档。
