# R76 门下审议：PASS-WITH-CHANGES（准奏，附强制执行契约）

任务 JJC-20261028-003；基线 HEAD cf84e40f1de1f290357e2a73fcb5adafafdf1fc3。
实读[计划](round76-plan.md)：166行/19,473B/SHA256 a5161f8afaa5b5bf6460932cedc53f5cf97b74a56da11c44cecea834fd845138；本报告不修改计划。
三杯结论：需求杯 PASS（候选A 22/25合理）；源码杯 PASS（所引锚真实）；交付杯 PASS-WITH-CHANGES（C/M须纳入尚书派发契约，非功能已通过）。

|实读锚（仓库相对路径:行）|核验事实与裁决|
|---|---|
|github/Crystal/Client/MirObjects/SpellObject.cs:42–48|TrapHexagon=Libraries.Magic、1390、100ms、10帧、Blend=true，吻合。|
|github/Crystal/Client/MirObjects/SpellObject.cs:30–38、288–302|Location/Repeat=true；首NextMotion按100取整、>=推进、每Process一步，elapsed不等价。|
|github/Crystal/Client/MirObjects/SpellObject.cs:323–339、363–370|格投影/显示移动偏移、DrawBlend(true,0.8F)；AnimationOffset先加格再乘格尺寸。|
|github/Crystal/Client/MirGraphics/MLibrary.cs:28–29、692–709|Magic与Magic2独立；offset先叠后裁剪、SetBlend传rate，恢复旧bool。|
|github/Crystal/Client/MirGraphics/DXManager.cs:353–384|NORMAL SourceAlpha/One；写BlendFactor不能证明NORMAL src×0.8。|
|crates/mir-client/src/ui/spellfx.rs:27–61、287–388|Trap公共窗700，入口先截窗；旧通用Magic/60ms/居中/Over；309已为严格 > 注释。|
|crates/mir-render/src/entities.rs:497–523|Magic→Magic/start0，Swing→Magic2/start0，Bolt+20；本轮应Magic。|
|crates/mir-client/src/net.rs:7424–7475|events追加、fields按object_id更新；不修26移除/重入。|
|tools/assets/analyze_usage.py:327、932–933|MAGIC_FRAMES进入magic.wzl，MAGIC2_FRAMES进入magic2.wzl；前者追加十帧。|
|crates/mir-client/tests/round72_poison_cloud_mapping.rs:1381–1527|1383/1404注释、1408/1429枚举锁149 Trap旧图；1447局部magic2实际是Magic第2帧。|

① 产品窗裁决：批准149精灵0<=age<4000、100ms十帧elapsed循环，公共duration_ms/非149/文字700不变；明确PARTIAL，禁称Crystal等价。当前149 Trap也是700，4000为新产品决定。
② 历史契约裁决：批准仅R72 G5两枚举TrapHexagon→Reincarnation与两处注释的有限supersede；原expected/fixture/oracle操作保持。无需另加控制取代本迁移；保留R72原测试会与批准新行为冲突。若选择禁止改R72或扩大替换范围，必须计划修正+门下再批。
③ 素材裁决：只给MAGIC_FRAMES追加range(1390,1400)，绝非Magic2。原库resource/data/magic.wzl 6,275,819B/SHA256 3b8a2dba9a1246e90a663e712a84b488fbee924ad6ea70af3be986f00a02ab25，十帧格式3、非空、zlib/行长探针通过；现导出106帧未含1390，仍须提取/运行时可读/本地扩包复算，不入git。
④ 验收与职责裁决：八G/七M及A→B→C→Lead门禁→D→独立终审足以覆盖有限切片；批准附以下C/M，静态审议不替代未来cargo/CI原始证据。

C1（尚书/Lead）：派发逐字纳入本C/M，先登记后启动；生产只改crates/mir-client/src/ui/spellfx.rs，另仅新增R76测试、获批R72四处替换、素材单行增量与本轮文档；禁止泛化docs/process/round76-*.md白名单覆盖既有审批原件。
C2（A/C）：新分支在公共700过滤前，只准149来源、两param，负age/4000边界均continue；G3同时断言duration_ms(TrapHexagon)==700、文字700，G1分别核event.id与object_id，G2必须含80/99。
C3（A/B/C）：先保全原R72完整源码及bytes/SHA/UTC，公开四处diff与前后SHA；基线58f64ab2d30b69f3aea5c07a8975e988ca842bce45972499a6f93e120958baea/65,322B；保留原历史日志/fixture/expected，不宣称R72字节不变；C独立检查仅批准行改动与迁移后9项绿。
C4（A/C）：真实十帧逐帧记录源WZL/WZX哈希、索引/format/尺寸/signed offset、PNG/输出索引哈希与读取结果；按现有资源索引提取，再验证本地pack或运行时路径；完整argv/工具身份可复算，缺帧即退中书，禁以合成绿代替。
C5（B/C/Lead/D）：独立物理副本，冻结expected/fixture及原stdout早于actual；每M前绿→编译0→指定断言红101→恢复SHA→八绿；M7 Python0且G8断言红；case数/逐套计数/恢复与失败attempt只增保留，Lead唯一提交及全门禁，D后代码再变重闸。
M1（A/D必改口径）：执行合同/新注释明确“149 Trap从700改为4000的新产品精灵窗”；不得沿用计划71行“既有149窗”而暗示Trap原来4000，禁src×0.8推断；不扩改历史注释。
M2（D/C必改披露）：新合同和manifest显式列supersedes仅R72 G5的149 Trap旧像素锁，并公开C3要求；其余技能/历史合同不被默示关闭，M1/M2完成由C核必改0再交Lead。

历史承接：计划143–153行保留R75七组全部原号，含R72 B plan-M4、R71-C7/E1、R69条件5、R61–R67与R74 artifacts34；R75-E1/C-M1旧原字节不可恢复、R75-E2/C-M2首actual原stdout缺失仍PARTIAL，不追溯清零。
风险与R77：严格NextMotion/Repeat/26寿命、owner/AnimationOffset/rate/Mode/Light及设备视觉仍PARTIAL；R77先完成独立终审/尚书/门下，定义新寿命合同替代范围或核下一技能真实帧及旧控制冲突，spellfx.rs:309不重复立项。
CLI回执（UTC）：memo EXIT0；state Assigned 2026-10-27T21:37:17.840Z–21:37:17.907Z EXIT0（Menxia→Assigned）；validate 21:37:17.908Z–21:37:17.959Z EXIT0、252条任务；git diff --check EXIT0。素材探针首attempt编码错误EXIT1原件保留，修正UTF-8后attempt2 EXIT0。
范围声明：只写本报告与target/round76/menxia-review/**；未改计划/源码/素材，未启执行者、未commit/push、未跑cargo/CI。CLI实际回执见[证据](../../target/round76/menxia-review/runs.json)与[笔记](../../target/round76/menxia-review/notes.md)。
