# R72-* 尚书派发
主单 `JJC-20261027-009`：Assigned→Doing；依据[门下C1–C8](round72-menxia-review.md#L14) PASS-WITH-CHANGES与[中书计划](round72-plan.md)。
本次仅登记与派发；以下A/B/C/Lead/D为后续执行契约，不表示已实现或验收通过。

|标识/统一标题|真实ID|部门|登记状态/EXIT|
|---|---|---|---|
|主单|JJC-20261027-009|尚书协调|Doing / state 0|
|R72-A 149毒云实现与9测试隔离红绿|JJC-20261027-009-sub-3a8ae80c|bingbu|Doing / delegate 0|
|R72-B 七变异+独立oracle|JJC-20261027-009-sub-892eb72e|gongbu|Doing / delegate 0|
|R72-C 刑部独立核验|JJC-20261027-009-sub-da472dd2|xingbu|Doing / delegate 0|
|R72-D 文档与清单|JJC-20261027-009-sub-fbfcf27c|libu（礼部）|Doing / delegate 0|

DAG：门下附条件批准 → A → B → C（必改0）→ Lead功能提交/唯一全量门禁 → D → 尚书验收 → 指定门下确认。
Doing是CLI默认登记状态，不代表依赖已满足；本次未启动执行agent，ID↔agent映射由Lead启动时登记，禁止并行越过依赖。

|条件|责任与可审交付|
|---|---|
|C1|A实现、C核验、D披露：20×120ms elapsed；4000ms仅产品瞬态事件窗；严格NextMotion/100ms取整/每Process一步/持久场/26移除继续PARTIAL。|
|C2|A保留rate1 Add；B/C独立像素核对；D保持rate0.8、Light3、DrawColour、INVLIGHT、真实视觉PARTIAL。|
|C3|A仅149 PoisonCloud；C查范围；D沿GAP-5原号保留其他26技能与历史PARTIAL，不清零。|
|C4|A/B/C/Lead/D每角色attempt只增不改；失败另建attempt，保存原日志/原字节/diff/bytes/SHA256，不改R71历史。|
|C5|A/B/C源码物理隔离、nlink=1，独立target及TEMP/TMP/TMPDIR；前绿/红/后绿同测试/夹具/oracle SHA；禁止0tests/skip/ignored或编译错算红。|
|C6|B独立实跑M1–M7全部七变异：每项先编译成功、九测指定断言红、恢复九测绿；独立Python oracle不调用生产helper。|
|C7|C同最终测试/夹具/oracle SHA，以独立物理副本只读核验九测及七变异；三源RGBA/SHA比对，列必改项，必改0交Lead。|
|C8|Lead独占功能提交及最终HEAD全量门禁，EXIT0且实际步骤全OK；门禁后crates/工具变化须新attempt；D分别披露文档HEAD与门禁HEAD。|

A交付：仅149毒云→Magic2[1650..1669]，phase=(elapsed/120)%20，0≤elapsed<4000；格锚点+当帧offset各一次、每事件一张、rate1 Add；复用Swing注释明确仅149寻址，不改公共语义。
A先测试后实现：[计划九个命名测试](round72-plan.md#L67)全部保留，隔离功能红绿、定向回归；写范围限计划指定生产单文件、新测试及自身attempt，禁止自行提交。
B七变异：M1误Magic、M2误Bolt、M3冻结不回卷、M4居中、M5关offset、M6保offset改Over、M7去来源限制；逐项前绿9/0→指定assert红→后绿9/0，保留恢复SHA。
C只读共享实现/测试，复现限自有物理副本；独立Python expected、Rust expected、生产actual三源并列，缺材料或不一致即阻断，不代A修共享源码。
D待Lead门禁通过后更新README/contract/manifest/client §5.37/roadmap二十；manifest不自引用、外置摘要；文档静态核验，不跑Cargo/全量。
门禁边界：唯一全量由Lead运行 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，禁SkipTests/SkipPostgres与复用旧绿；本派发者不改crates、不commit/push/cargo/CI。

CLI均明确 `--as shangshu`：四delegate EXIT0；`state JJC-20261027-009 Doing` EXIT0；`validate --json` EXIT0，schema v3.1、tasks=236、issues=[]；父/四子show均EXIT0并核实Doing。
CLI原始stdout/stderr、完整参数与EXIT：[登记实录](../../target/round72/shangshu/cli-records.json)。四条实际契约均≤60字符，精确数值/完整交付以本页及计划展开。
偏差：CLI自动给子标题加 `[委派]`，指令标题统一R72-*；未使用错误拼写shangbu，实际派发方shangshu、D承接方libu。
代笔披露：本页由受Lead委派的尚书协调agent撰写；未冒充A/B/C/D执行产出或独立验收；subagent深度已满，本次未另开agent。
失败披露：初始包装脚本在A/B/C成功后、D调用前因D契约74>60字符抛错，脚本EXIT1；此非edict CLI失败，D未创建。核读原实录后缩短D契约，仅续跑D/state/validate/show，无重复建单。
另有首次写脚本工具因空justification被拒，未落盘；随后成功写入。所有已实际执行的edict CLI均EXIT0。
