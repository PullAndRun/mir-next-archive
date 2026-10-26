# R66 尚书派发记录（attempt-1）

- 基线：`f14a81dc4a1f141dd37d7229d3f623a07e9a0df5`；派发前 HEAD 一致、crates diff 空、索引空；R65 主单 `JJC-20261027-002` 实读 Done。
- 输入：[计划](round66-plan.md)全134行、[门下审核](round66-menxia-review.md)全46行（PASS，必改0）、[R65契约](../testing/round65/round65-contract.md)；主单先 show 复核 Assigned。
- 本记录只证明登记与约束交接；四子单尚未启动 agent，由 Lead spawn/后台执行，Doing 不表示依赖已满足。

|节点|真实 ID|owner / 路由|实读状态|启动依赖|
|---|---|---|---|---|
|主|JJC-20261027-003|尚书协调 / Lead执行门禁|Doing|四单已登记|
|A|JJC-20261027-003-sub-bd16c8c5|兵部 bingbu / deepseek|Doing|无；Lead首启|
|B|JJC-20261027-003-sub-29fa573c|工部 gongbu / deepseek|Doing|A有效回执及support SHA|
|C|JJC-20261027-003-sub-cf73d939|刑部 xingbu / gpt-6.1-sol|Doing|B有效回执，A/B红绿证据齐|
|D|JJC-20261027-003-sub-7640317f|礼部 libu / gpt-6.1-sol|Doing|C必改0＋Lead功能提交及唯一门禁|

- DAG：A → B → C → Lead功能提交/唯一门禁 → D；计划81–93行。支持文件单owner，依赖串行，不并行写同文件。
- CLI差异：尚书无create权限；delegate才能生成带parent_task/delegation、支持delegate-result的正式子单，ID自动追加sub后缀，不支持指定00N。
- 四次delegate创建均EXIT0且原子赋Doing；A追加state Doing返回EXIT3（Doing→Doing非法，允许Review/Blocked/Cancelled），首败已保全，未借虚假Blocked中转。其余以show核实Doing；不声称四次state EXIT0。
- 主单 `state JJC-20261027-003 Doing --as shangshu` EXIT0；`validate --as shangshu` EXIT0，206条符合schema v3.1；字段校验不等于行为验证。

|节点|精确写域白名单（其余只读）|派发验收要点|
|---|---|---|
|A|`crates/mir-client/tests/support/round66_watermark.rs`；`crates/mir-client/tests/round66_watermark_contract.rs`；`target/round66/a/**`|计划48–54、69、72、85行：G1.1/G1.4，错误优先级、min/0/单成员/重复成员、五Err、不裁剪计数及真实表不变|
|B|`crates/mir-client/tests/round66_consumer_boundary.rs`；`crates/mir-client/src/net.rs`仅cfg(test)新增等待器测试；`target/round66/b/**`|计划70–73、86行：G1.2/G1.3/G1.5，真实71/125→snapshot→UI双分支，shared Receiver顺序反例；A support只读|
|C|`docs/testing/round66/round66-c-evidence.md`；`target/round66/c/**`|计划74、87、92、99行：只读独立复跑/隔离变异负控/SHA核验，正常生产零diff、新ignore/allow为0、四行矩阵；缺陷退原owner，不自修|
|D|`docs/testing/round66/README.md`；`docs/testing/round66/round66-contract.md`；`docs/testing/round66/final-manifest.json`；`target/round66/d/**`|计划88、93、101行：四行兼容矩阵、红绿/门禁索引、原编号PARTIAL台账、实际HEAD/EXIT与manifest；冻结历史材料只读|

- 交付契约：各单return_spec已写入全部输入、白名单、专项验收与共同约束；主证据目录分别为 `target/round66/a/attempt-1/`、`target/round66/b/attempt-1/`、`target/round66/c/attempt-1/`、`target/round66/d/attempt-1/`，attempt不复用。
- 各owner回传证据文件及路径/行数/bytes/SHA/原始EXIT/剩余PARTIAL清单，自身todo完成后以真实owner回填delegate-result，再send_message给Lead；口头完成/空回执不能启动下游。
- C1（门下35行；计划69–77行）：隔离副本最小变异、每变异新attempt、首败永久保全，红绿同测试SHA、源码/补丁SHA、UTC/HEAD/原始EXIT/双流/失败测试及左右实值齐。0 tests、编译失败、SKIP、旧失败、空泛assert不算红。
- G1.2 min→max须败在真实suffix/身份/cue；错误前缀保len须被身份顺序抓住；空集→tail、忽略generation/接纳低水位、等待器注ack/早返S1均须有效红（计划69–73行）。
- C2（门下36行；计划55、70–73行）：now_ms=1000冻结；初读S2/S4先drain初始cue并记view基数，只比较新增view。安全分支裁后(2,2,4)、push5后(2,3,5)，新增目标[3,4,5]/cue3；现状分支仅[5]/cue1而水位5，旧S4 clone仍[1,2,3,4]，重复sync新增0。
- 等待器有限timeout，预送S1/S2，predicate total>=2返回S2，随后try_recv=None且command_rx=Empty；无并发线程/sleep，不称第二广播消费者。
- C3（门下37行）：派发前HEAD/白名单已核；C再核生产逻辑/wire/server零diff、新增ignore/allow为0、四行矩阵，问题退原owner。
- C4（门下38行；计划129–133行）：Lead独占功能提交与ci.ps1，C必改0后才执行；门禁留完整HEAD/UTC/原始EXIT/双流/bytes/SHA。门禁后任何crates变化（测试/注释亦含）使旧门禁失效，须Lead新提交、新attempt；D不得预填PASS。
- 风险锚点（计划40–43行）：Crystal `Shared/ServerPackets.cs:3564`只有Spell/Source/Destination，无全局最低水位/acks/序号；`Server/MirEnvir/Map.cs:2375–2383`仅地图对象移除，不能当投射物确认。
- 模型是本项目待实施提案，不是Crystal已有；生产多消费者/全局min注册退出租约/fan-out、容量/LRU、服务端回收、饱和恢复均未实现且继续PARTIAL；历史R49–R65按原轮原编号承接，不清零（计划118–124行）。
- 禁称“生产多消费者已支持”“全局已实现”“125全链已证”“R66通过”“已限容/有界”“服务端ack闭环”“真实可听已证”；单项模型/回归/门禁结论不替代尚书验收与门下确认。
- 两份既有未跟踪 `docs/testing/monster-native/player-resource-freeze*.json` 按SHA保全，禁止暂存/覆盖。尚书本次不改crates、不commit/push、不跑cargo/ci.ps1。
- 本次原始命令回执、子单show、首败、派发回执与终检均在 [尚书证据目录](../../target/round66/shangshu/attempt-1/)；CLI创建方式与冗余state失败已告知Lead。
