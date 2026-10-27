# R71 尚书省派发

- 主单 `JJC-20261027-008`：Assigned→Doing，真实 EXIT 0；依门下 PASS-WITH-CHANGES 及149行计划派发。
- CLI：`delegate <父ID> shangshu <部门> "<标题>" "<契约>" --as shangshu`；无 depends_on 参数，契约及父 memo 强制 A→B→C→D 串行。

|子单/责任署|真实 ID|登记状态/EXIT|agent 映射与启动边界|
|---|---|---|---|
|A 兵部：FireWall 149 映射与锚点实现+8测试|JJC-20261027-008-sub-96fd3df7|Doing / 0|Lead spawn deepseek teammate；agent ID待Lead回填|
|B 工部：独立副本八测试与五变异|JJC-20261027-008-sub-e4596e20|Doing / 0|Lead spawn deepseek teammate；等待A通过；agent ID待回填|
|C 刑部：独立核验|JJC-20261027-008-sub-3cd9c7fd|Doing / 0|Lead spawn独立gpt subagent；等待B通过；agent ID待回填|
|D 礼部：文档批|JJC-20261027-008-sub-8a71743a|Doing / 0|Lead spawn gpt subagent；等待C PASS且必改0；agent ID待回填|

- Doing为CLI登记默认值，不代表四代理已启动；尚书受委派深度限制，由Lead实际spawn并保全真实ID映射，禁止跨依赖并行。
- A契约（M1）：实际 rate=1；`src×0.8×α`只属可接受但未实施的显式 PARTIAL 近似，默认BlendFactor不证明等价；不新增0.8 API。
- A契约（M2）：时钟来源写实 `NextMotion = CMain.Time + FrameInterval`；初始100ms取整/逐Process一步仍PARTIAL；elapsed取模不称严格Crystal迁移。
- A标题/注释/验收限定（M3）：149 FireWall六帧elapsed循环与格锚点接线；Magic1630–1635、120ms、720ms回卷；4000ms仅本轮事件窗口，非Crystal对象寿命。
- A写入仅 `crates/mir-client/src/ui/spellfx.rs` 与新增 `crates/mir-client/tests/round71_firewall_mapping.rs`；非149旧行为保留，一条149一图，当帧signed offset一次后裁剪。
- B/C硬门禁（M4）：逐项列八测试计数、五变异前绿→预期断言红→原字节恢复后绿及SHA；必要新增测试退A，重新冻结B/C全重跑，不减少八项。

|门下条件|责任与不可省略验收|
|---|---|
|C1|尚书登记及契约/memo；Lead实际spawn回填映射；A→B→C→D，C独立于规划/实现。|
|C2|A先8项命名测试+独立RGBA oracle；旧码编译成功且映射/坐标断言失败才记红，再实现绿。|
|C3|B/C各自真实物理副本；禁hardlink/symlink/junction共享可变源码；绝对cwd、源root、nlink=1、独立target/tmp/out。|
|C4|B/C测试/夹具/执行器同SHA冻结；五变异逐个前绿/断言红/恢复绿，留生产diff与恢复SHA。|
|C5|A/B/C红须编译成功、实际running8、预期断言失败且EXIT非0；编译/导入/权限/环境错、0tests、SKIP/ignore均不算红。|
|C6|A/B/C齐备G1–G5八测试、真实149公开生产链及R回归；C同SHA独立复现五变异，不转录B冒充。|
|C7|A/B/C/D/Lead原命令、UTC、HEAD、cwd、环境、EXIT、统计、首败expected-actual、逐文件bytes/SHA及外置摘要只增不改；失败attempt/历史freeze/R69原件保留。|
|C8|仅Lead执行唯一全量及功能/证据文档分批提交push；分别保存EXIT，门禁后crates/工具变化须新attempt重跑。|
|C9|C核验、D台账、Lead收口仅结对应子项；rate/DrawColour/Light、严格时钟、持久场/26移除、其他调用点、真实视觉继续PARTIAL。|

- B只写 `target/round71/b/attempt-N/**`；C写独立证据及 `docs/testing/round71/round71-c-evidence.md`；发现缺口退A，不代改正式测试。
- D依C通过写 `docs/testing/round71/README.md`、manifest/外置sha及 `docs/client.md`、`docs/roadmap.md`相关小节，仅用实录，M1–M3复写最终台账。
- Lead唯一全量：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，不得SkipTests/SkipPostgres降级或借旧绿；EXIT0且12/12 OK及原log/meta完整才可验收。
- A/B/C/D不commit/push、不跑全量；尚书本次仅获例外提交本派发文档，消息 `docs(process): R71 尚书派发（第71轮）`，不push。
- 证据根 `target/round71/**`；尚书回执 `target/round71/shangshu/attempt-1/`，含完整命令/UTC/cwd/stdout/stderr/EXIT；功能证据尚未执行。
- 父memo/Doing/show/四子show均EXIT0；`validate --as shangshu` EXIT0：231条符合schema v3.1；流程校验不代表功能通过。
- CLI异常披露：实际delegate需派发方/承接方，无原生depends_on，登记子单默认Doing；本轮CLI无非零EXIT。任一C/M未满足BLOCK/返工，禁止继续收口。
