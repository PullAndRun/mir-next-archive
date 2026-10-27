# R71 门下省独立审议

- 主单：JJC-20261027-008；verdict：**PASS-WITH-CHANGES（附条件准奏 / approve）**。
- 审议基线：HEAD `6e0f7876a07dbb8af4e2cbf34ee0409a2653ea8f`；实读计划149行及下列Crystal原码。
- 本结论仅放行受条件约束的执行；功能、测试、变异、全量与视觉均仍待执行，不声称通过。

## 三条裁决

1. **rate / DrawColour：接受显式PARTIAL，禁止Crystal等价表述。**
   `DXManager.cs:353–389` 默认SourceAlpha/One；BlendFactor写入不代表默认源因子使用它。
   CPU `src×0.8×α`只能作为另行声明的近似契约，不能从0.8实参推出DX等价；DrawColour调制亦未闭合。
   本轮按计划继续R70 rate=1 Add，不新增0.8路径/API；“接受近似为PARTIAL”不等于本轮已实现近似。
2. **调度：接受复用rate=1与4000ms窗口，不要求A同时实现严格Crystal时钟。**
   允许 `(age / 120) % 6`；严格NextMotion、初始100ms取整、逐Process步进及对象存续/移除仍PARTIAL。
   实读 `SpellObject.cs:288–302`：初始为Time+FrameInterval再减余数；每Process最多一步，
   第302行实际为 `NextMotion = CMain.Time + FrameInterval;`，并非 `NextMotion += Duration`。
   elapsed跳帧/720ms回卷及4000ms截止是本轮约定，不能标成Crystal严格时序或对象生命周期已迁移。
3. **范围：接受仅149来源ObjectSpell FireWall，保留非149旧行为，1620起手不迁。**
   `SpellObject.cs:37,49–56`支持Repeat、Magic1630–1635、6帧、120ms；323–339支持格原点与DrawBlend(true,0.8)。
   `PlayerObject.cs:1802–1804`的1620属于施法者起手，3057–3059完成仅声音，不能混入地面火墙。
   当帧signed offset仅加一次后裁剪；一条149一图，不再客户端铺十字；Light3/真实素材视觉仍PARTIAL。

## C：执行条件

- C1 尚书先登记A→B→C→D子单及agent映射，按依赖串行；审核者独立于规划/实现者。
- C2 A先写8项命名测试和独立RGBA oracle；旧代码须成功编译且在映射/坐标断言失败才算功能红，再实现绿。
- C3 B/C各用独立真实物理副本；禁hardlink/symlink/junction共享可变源码，记录绝对cwd、源root、nlink=1及独立target/tmp/out。
- C4 测试/夹具/执行器同SHA冻结；五变异逐个前绿→预期断言红→原字节恢复后绿；保存生产diff与恢复SHA。
- C5 红必须编译成功、实际running8、预期断言失败且EXIT非0；编译错、0tests、导入错、环境/权限错误、SKIP/ignore均不得冒充红。
- C6 G1–G5八测试、真实149公开生产链及R回归执行齐全；C以同SHA独立复现五变异，不能转录B结果冒充复验。
- C7 原始命令/UTC/HEAD/cwd/环境/EXIT/统计/首败expected-actual、逐文件bytes/SHA及外置摘要只增不改；失败attempt、历史freeze/R69原件全保留。
- C8 仅Lead跑计划唯一全量与提交/push；各条EXIT分别保全；改crates/工具后须新attempt重跑，不能借旧绿或降级参数。
- C9 本切片验收只结对应子项；rate/DrawColour/Light、严格时钟、持久场/26移除、其他调用点及真实视觉继续列PARTIAL。

## M：必改与复核点

- M1 尚书在A任务契约、D最终台账明确：本轮实际rate=1；src×0.8×α为可接受但未实施的显式PARTIAL近似，默认BlendFactor非等价证明。
- M2 所有执行契约/时钟来源说明按第302行的Time+FrameInterval写实；不得传播“NextMotion+=Duration”或将elapsed循环称严格迁移。
- M3 A任务标题/注释/验收措辞限定“149 FireWall六帧elapsed循环与格锚点接线”；不得称完整Crystal FireWall迁移，4000ms不是Crystal对象寿命。
- M4 B/C报告逐项列八测试计数及五变异前绿/断言红/后绿与SHA；必要新增测试须重新冻结并全重跑，不减少八项。
- M1–M3须在派发A时落实；M4为B/C交付硬门禁；任一条件不满足应BLOCK/返工，不得继续收口。

## CLI与审议证据

- `state JJC-20261027-008 Assigned --as menxia`：EXIT 0；show复核Assigned；`validate --as menxia`：EXIT 0（227条符合schema v3.1）。
- 原始stdout/stderr及UTC/cwd/完整命令/EXIT回执：`target/round71/menxia/attempt-1/`；流程校验不代表功能验收。
- 本审议未写crates，未运行cargo/CI，未commit/push。
