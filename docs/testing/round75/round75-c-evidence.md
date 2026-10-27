# R75-C 独立核验（JJC-20261028-002-sub-195ef58c）

- verdict：**FAIL／退补证**；技术矩阵 PASS，证据必改 **2**，未满足 C6「C必改0」，当前不可提交/启动全门禁。
- 核验窗：2026-10-27T20:11Z 起（20分钟预算）；HEAD=96e46cc，origin/main=6502a90；C不改实现/测试、不commit/push、不运行全量门禁。
- 命令、cwd、UTC起止、EXIT、计数、日志bytes/SHA详见 [runs](../../../target/round75/c/attempt-1/runs.json) 与 [辅助命令](../../../target/round75/c/attempt-1/auxiliary-runs.json)；原始日志均在同根 logs。

|必做项|裁定|独立结果|
|---|---|---|
|1 范围/卫生|PASS|入场status仅新增A测试及两freeze；git diff/cached -- crates均空；无ignore/allow/历史私有helper导入。C只新增本报告与target/round75/c/attempt-1。|
|2 基线/逐exact/静态|PASS|独立archive物理副本七绿7/0/0，七次exact各1/0/0；fmt --all -- --check、clippy -p mir-client --all-targets -- -D warnings均EXIT0。|
|3 五变异|PASS|M1–M5全部独立物理副本：基线七绿→变异no-run0→指定exact断言红101→原字节SHA恢复→七绿；无编译错/0tests冒充红。|
|4 独立oracle|PASS|C新写stdlib解析器，自解四个WZL/WZX，13个整帧case、63个模型轨迹采样；125 agree/0 mismatch；未导入A/B脚本或生产helper。|
|5 时序/诚实性|FAIL/PARTIAL|17项A/B日志UTC/EXIT/count与manifest字节一致；措辞为差异合同无扩称；首次actual时序缺原始日志，B失败日志被覆盖，见必改。|
|6 门禁/提交条件|PARTIAL|生产零改；唯一全量门禁应由Lead在提交测试后运行，不SkipTests/SkipPostgres；须先解决本报告必改，C未跑该门禁。|

- 测试SHA：ecff550dfa0bcd671d32723bd18fbcd444c93c4d173c1562bfbc0f8ad185a05c（56,403B/1454行）；A/B冻结及C各副本恢复后相同。
- 两freeze仍为44,844,165B/SHA70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de；45,219,533B/SHAc7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32。
- A manifest SHA c10a84fe46c5233c9530f73dbae126d2949e0996576ee5cec76fa87ea4b73520；B manifest SHA a881667d7f2ac76fc345fb0c93ba7f3feef1503343c61378f70679c3f3e42dc1；C入场/收尾哈希一致。
- 独立复跑计数：11次全七绿（6副本基线+5恢复）=77 passed；7 exact=7 passed；合计84 passed/0 ignored；另5条有效红各1 failed；11次no-run EXIT0；fmt/clippy各EXIT0。
- 隔离：git -c core.autocrlf=false archive 96e46cc；copies/{base,m1,m4,m5,m2,m3}各1313文件、nlink=1、symlink=0，独立CARGO_TARGET_DIR及TEMP/TMP/TMPDIR；patch及SHA见 mutations/m*/summary.json。

|负控|首败测试行|actual → expected（均EXIT101、0passed/1failed/6filtered）|
|---|---|---|
|M1 模型 >=→>|940|(frame,next)=(0,1300) → (1,1420)|
|M2 模型 now+interval→next+interval|1002|(1,220) → (1,2120)|
|M3 模型Repeat额外removed=true|1057|(ended,removed)=(true,true) → (true,false)|
|M4 生产AREA_MS 4000→2000|1142|FireWall age2000 pixel(391,262)=[0,0,0,255] → [248,0,248,255]|
|M5 生产26额外删spell_events|1251|完整事件列表投影id=[2] → [1,2]（首败为事件断言）|

- 五红方向与A/B一致；M1–M3仅模型语句、assert/expected/fixture均未改；M4/M5仅副本生产代码。恢复SHA逐项相等。
- G1独立整帧SHA=8b9e6c364933c732d894316a95b8bc772d1dfa8527330bb7766bbddb0aedd2e9，与A frozen及测试嵌入三源一致；pixel(381,261)/(382,262)/(383,263)均[248,0,0,255]。
- oracle另核G4品红像素；G5事件id[1,2]/field仅7002；G6旧A/旧B/新A三op且id[1,2,3]、起点[1000,1000,1200]；G3/G7纯模型，分别7+21与31采样，不计生产寿命覆盖。
- 时序：A expected generated_utc=2026-10-27T19:31:57Z，声明首次actual=19:35:21Z，字符串顺序成立；遍历A全部.log未找到首次actual时间。现存七exact日志19:42:01Z起，不能独立证明“首次”。
- 必改C-M1（owner B，C5）：B manifest deviations明确披露失败注入attempt（Windows Store python EXIT49）被后续覆盖，原失败rows/log未单独保全；须退B补可追溯原始记录/新attempt及门下裁定，不能用本次独立绿洗掉历史缺口。
- 必改C-M2（owner A）：提供19:35:21Z首次actual原始命令/UTC/EXIT/日志SHA，或如实更正为未独立证明并交门下裁定；C不代改A证据，不伪造历史日志。
- C工具偏差：审核脚本首次GBK解码失败EXIT1后显式UTF-8修正；辅助复算重生成派生audit/oracle输出（内容断言一致，生成UTC更新），此过程见辅助日志及c-tool-deviations；未覆盖cargo日志或A/B字节。
- PARTIAL原号承接：GAP-1/2、R70-C2；GAP-5/R70-C1、R70-C3、GAP-3/4/6；R71时钟/对象、R73-T1；R72 B plan-M4/legacy lint；R71-C7永久PARTIAL/E1仅补救CLOSED；R69条件5；R68/R49–R70/R61–R67及R61 P01–P12、R62①–④、R63 P1–P7、R64①–⑥、R65 G1.2/G2、R66生命周期/水位、R67容量/保护/map epoch均不清零。
- R74历史artifacts=34口径照承；本轮不称生产寿命已修/严格等价/Blizzard已迁移；模型差异合同今后若被生产修复替换，须显式supersede。
- 提交前风险：两项证据缺口须裁定；历史PARTIAL与真实素材/设备/完整UI/跨后端/Live音频未覆盖；当前证据不能代替Lead提交后的唯一全量门禁。
