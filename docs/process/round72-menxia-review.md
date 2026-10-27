# R72 门下审核
verdict: **PASS-WITH-CHANGES**；C1–C8 共8项强制条件，按下述裁量执行。
范围：仅149 PoisonCloud逐格映射，现有入口可复用；未见需扩大框架的阻碍。

|抽查锚点|实读结论|
|---|---|
|[SpellObject.cs:57–64](../../github/Crystal/Client/MirObjects/SpellObject.cs#L57)|Magic2/1650/20帧/120ms/Light3/Blend=true；全文件未命中4000，不能据此证明毒云寿命。|
|[MLibrary.cs:692–709](../../github/Crystal/Client/MirGraphics/MLibrary.cs#L692)|699先加当帧offset，701后裁剪；704–709仅保全/恢复bool，非完整rate状态。|
|[DXManager.cs:353–384](../../github/Crystal/Client/MirGraphics/DXManager.cs#L353)|rate默认1，实际SpellObject传0.8；NORMAL=SourceAlpha/One，另写BlendFactor，不能推导CPU乘0.8等价。|
|[GameScene.cs:323–339](../../github/Crystal/Client/MirScenes/GameScene.cs#L323)→[SpellObject.cs:323–339](../../github/Crystal/Client/MirObjects/SpellObject.cs#L323)|前者为UI初始化，非LOD/绘制；实际323–325格坐标+偏移，335–339 DrawBlend(...true,0.8F)。计划第34行已正确，无需修正计划；应修正抽查请求中的文件名。|

复用实读：[spellfx.rs:272–278](../../crates/mir-client/src/ui/spellfx.rs#L272)已有149火墙特判、格锚点与blend调用；[entities.rs:509–522](../../crates/mir-render/src/entities.rs#L509)确认Swing→Magic2/start0、Bolt→Magic2/start20，限定毒云分支复用可行。

## C1–C8 强制条件

- C1：4000ms仅既有产品瞬态事件窗；inNonNextMotion（elapsed）/严格NextMotion、100ms取整、持久场与26移除继续PARTIAL，不宣称Crystal寿命等价。
- C2：rate0.8近似未实施，保留rate1 Add；Light3/DrawColour/INVLIGHT/视觉等价继续PARTIAL。
- C3：仅闭合149 PoisonCloud有限子项；其他26技能沿GAP-5原号保留，历史PARTIAL不得清零。
- C4：每角色attempt证据只增不改，原日志/原字节/差异/bytes/SHA齐备；失败另建attempt，不重写R71历史。
- C5：红绿源码物理隔离、nlink=1，隔离target及临时目录；前绿/红/后绿同测试/夹具/oracle SHA；禁止0tests/skip/ignored及编译错充当红。
- C6：B独立实跑M1–M7全部七变异；每项编译成功后九项测试以指定断言红，再恢复九项绿；独立Python oracle不得调用生产helper。
- C7：C同最终测试/夹具/oracle SHA以独立物理副本复现九项与七变异，三源比对RGBA/SHA并列必改项；必改0方可交Lead。
- C8：Lead独占功能提交与最终HEAD全量门禁（EXIT0、步骤全OK）；门禁后crates/工具变更必须新attempt；D文档HEAD与门禁HEAD分别披露。

## 三裁量

1. 接受Swing零起点入口；生产注释须明确其仅为149 PoisonCloud的Magic2寻址复用，不改变Swing公共语义。
2. 4000ms Crystal出处不足：补出处，或明确标为产品瞬态窗；现有看板原旨已允许保留该窗口，采用后者即可，不新增协议/寿命实现。
3. 接受elapsed有限映射；NextMotion/每Process一步/初始取整、rate、对象寿命与真实视觉见PARTIAL，不作整体等价验收。

## CLI 实录

- show JJC-20261027-009 --as menxia：EXIT0，采样时Menxia；原旨明确保留rate1与4000ms事件窗。
- state JJC-20261027-009 Assigned --as menxia：EXIT0，Menxia → Assigned；validate --as menxia：EXIT0，232 tasks符合schema v3.1。
- 本审核仅写本页与证据目录；未改crates/计划，未运行cargo/CI，未commit/push。
