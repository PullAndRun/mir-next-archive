# R67 尚书省派发契约
主单 `JJC-20261027-004`：Doing；基线 `52d95b133d040e6d3c4ddded113be126059b2111`。
依据：[批准计划](round67-plan.md) 与 [门下审核](round67-menxia-review.md)；PASS-WITH-CHANGES 的 C1–C10、M1–M6 全部约束执行。
本次只登记派发，不启动执行者；各子单真实 CLI ID 如下，agent ID 均为“未启动”。
|节点|子单 ID|角色/模型|依赖与交付职责|
|---|---|---|---|
|A|JJC-20261027-004-sub-71e056ca|bingbu / deepseek|门下批准后；私有容量/保护引用/确定性LRU候选提案、G1.1/G1.2；为G1.3同代夹具/G1.4单测级红绿提供实现支撑|
|B|JJC-20261027-004-sub-bd2cae45|gongbu / deepseek|A有效回执后；G1.3完整/候选真实71/125→UI差分及G1.2/G1.4 UI身份/回退，落实M2/M3|
|C|JJC-20261027-004-sub-0b916504|xingbu / gpt|B有效回执后；独立只读复跑、变异红、证据SHA、范围/诚实性；缺陷退原owner|
|D|JJC-20261027-004-sub-5c265347|libu / gpt|C必改0及Lead门禁事实后，由Lead启动；README/契约/矩阵/manifest|
DAG：**A → B → C → Lead 唯一门禁 → D**；同文件单owner，依赖未满足不得启动；本次无执行agent映射。
|必改|责任映射|不可放宽的验收|
|---|---|---|
|M1|A支撑/B实施/C复核/Lead与D披露|G1.3用WorldSnapshot::default、同代generation=0，71预热；UI/快照代号均断言0，consumed=5。非wire筛历史/移除必须披露；若改new_session锚事件须重算基数|
|M2|B/C，D记录|先71把目标1从(11,10)更新(19,10)，隐藏后125回退(19,10)；隔离禁用可见缓存刷新取得真实UI红；保留移除cache.clear红。单纯交换优先可幸存，因先刷新/advance覆写，不能冒充有效红|
|M3|A支撑/B/C，D记录|仅隔离support保护判断忽略ID1，绝不改测试protected字面量；推导保留[7,4]，候选UI实值[4]≠预期[1]，必须败在真实UI身份/坐标assert|
|M4|A/B/Lead|A≤1.25h+B≤1.25h，总≤2.5 deepseek小时、硬上限3h；逐owner记UTC与实际累计；GPT审计/文档另计。当前执行累计0，超限停止扩面、保全证据报PARTIAL|
|M5|A/B/C|每红attempt记录绝对cwd、--manifest-path、CARGO_TARGET_DIR、TMP/TEMP/TMPDIR，证明编译red-copy；独立源码/target，无硬链接，工作树SHA不变|
|M6|Lead/D，C核验|最终五行矩阵/红绿索引披露非wire代号、幸存变异原因与替代红、实现变异位置和实值；预期标推导，实际通过数/EXIT只从命令填|
交付文件白名单（路径均相对仓库根，未写明文件只读）：
- A：`crates/mir-client/tests/support/round67_positions.rs`、`crates/mir-client/tests/round67_position_policy.rs`；证据 `target/round67/a/`。
- B：`crates/mir-client/tests/round67_position_boundary.rs`；只读A support；证据 `target/round67/b/`。
- C：`docs/testing/round67/round67-c-evidence.md`；证据 `target/round67/c/`；复跑及隔离副本可写，工作树实现只读。
- D：`docs/testing/round67/README.md`、`docs/testing/round67/round67-position-contract.md`、`docs/testing/round67/final-manifest.json`；证据 `target/round67/d/`。
- 尚书：本文及 `target/round67/shangshu/attempt-1/`；命令逐条JSON保留command/cwd/UTC起止/双流/exit，SHA清单另存。
验收继承计划精确接口/输入/测试命名；A/B新targets各≥2测试、failed0/ignored0；当前未运行任何cargo，不预写PASS。
G1.1/G1.2逐支数值断言：排序(tick,id)、最小前缀、排列不变、并列tick、全保护超额、budget0/≥len、未知/重复保护、重复entry错误且输入不变。
G1.3完整UI身份[1,2,3,4]/cue4，候选[1]/cue1；两者consumed5、重复sync新增view/cue0；源7可见、999从未见；坐标也逐项断言。
所有71/125走encode→decode→apply；历史筛选与实体移除是非wire夹具，非生产LRU，不称socket/Remove包闭环或真实可听。
真红：指定测试运行至指定assert、原始exit非零、保留左右实值；同一测试文件红绿SHA相同，只变异实现；每变异新attempt，首败永久保全。
禁把编译错、0 tests、skip、故意panic、既有失败或幸存变异充红；所有幸存如实报告。红/绿各独立target/tmp，禁止硬链接与复用变异二进制。
C按计划独立复跑A/B、既有projectile与R63–R66、fmt/clippy/diff检查；SHA/bytes/UTC/原始EXIT齐全，不能用静态检查替代运行。
Lead唯一可提交功能批并在确定HEAD独占运行 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；先核soak锁/tmp归属。A/B/C/D不得代跑、commit/push。
门禁后任何crates变化（含测试/注释）使旧门禁失效，Lead新功能提交/新attempt；D只在门禁后记录真实事实，不改crates。
禁区：src、旧测试、wire/server、依赖、配置与生产接入；不加ignore/allow、不改动作FIFO/音频/素材；生产位置缓存仍无界PARTIAL。
最终五行矩阵：生产无界缓存/参考候选/保护超额/未知与换代/map与ID复用，各挂证据或PARTIAL；承接R49–R66原编号与历史freeze，不清零旧缺口。
风险：保护漏项会少出生、保护超额不等硬上限；map epoch/同代ID复用/自动保护集/生产LRU未证；HEAD漂移先核影响，不扩大结论。
回滚：仅撤本轮新增测试/support/文档；失败保全退原owner。若需src修复，退门下重新裁决白名单，禁止改生产追绿；不覆盖历史freeze。
代笔说明：本文由gpt-6.1-sol以尚书派发职责整理批准输入；未代A/B实现、未代C独立核验、未代Lead门禁或D事实签收；模型标注为待启动角色安排。
CLI实录：四次delegate EXIT0；主单state Doing EXIT0并show复读；validate --as shangshu EXIT0，211条符合schema v3.1。
异常：delegate --help真实EXIT1（用法提示，无建单）；delegate传入--remark但实现不保存opts.remark，已用shangshu memo补四子单范围并recall核实（均EXIT0）。文件工具早期4次参数校验失败未写入，无CLI exit；详见attempt-1工具异常记录，不伪造成功。
