# R67 门下省独立审核
**verdict：PASS-WITH-CHANGES（条件批准测试契约；M1–M6纳入派发契约后实施）；C=10，M=6。**
任务 JJC-20261027-004；审核者 gpt-6.1-sol / menxia；UTC 起止以 initial/CLI/final 命令回执为准。
基线 HEAD=origin/main=`52d95b133d040e6d3c4ddded113be126059b2111`；计划全文111行/15737B，SHA256=`3a7972ba2072e2b7f74d9350e58699e5410ec9947cf0a8ad64f6a6b7ad2ffece`。

|必审项|PASS/FAIL|实核值/判据|
|---|---|---|
|1 目标/历史|PASS|计划111行、roadmap §十四244–251、R66计划134行及验收35行全文已读；R66看板Done；仅补位置容量提案，不重复最低水位模型或清零PARTIAL。|
|2 Crystal|PASS|下列原文件逐行打开；TargetPoint/活动引用/时钟/FIFO/wire/地图移除均不能推出容量常数或LRU。|
|3 范围|PASS|A/B新增文件仅crates/mir-client/tests/**；生产、旧测试、wire/server零行为；map/ID复用、自动保护集及运行时限容移后。|
|4 验收|FAIL（按M1–M3修订可执行）|G1.1/2算式成立；G1.3预热代号未定；G1.4优先级变异可能幸存；修改测试里的protected字面量会破坏同测试SHA。均有不改生产的具体修订。|
|5 成本/回滚|FAIL（按M4补齐）|原计划未给执行者小时；建议A≤1.25h、B≤1.25h，共≤2.5 deepseek执行者小时；C/D另计。仅撤新增测试/文档；超支移后，不扩src。|
|6 看板/工作树|PASS|show EXIT0：Menxia、owner=taizi、tags=round67/known-positions/contract/partial、todos=[]；初始仅新计划+2历史freeze，新增本审核后4个??；crates diff空，HEAD未变。|
|7 可追溯交付|PASS|先落6行骨架；仅审核文档和attempt-1证据写入；无执行者、cargo、ci.ps1、commit/push；state/validate真实回执见末行。|

Crystal 原行核验（仓库根 github/Crystal/）：
- `Client/MirObjects/PlayerObject.cs:3546–3566`：3548取TargetPoint，3550优先ob.CurrentLocation，3554建Missile，3565入Effects；无容量/LRU。
- `Client/MirObjects/MonsterObject.cs:346,528–529`：ActionFeed.Count>1为SkipFrames，取[0]/RemoveAt(0)为FIFO；计划说346不支持淘汰，核对成立。
- `Client/MirObjects/Effect.cs:38–48,81–96`：Owner/Source初始化、严格NextFrame/SkipFrames/Remove；144–194为Missiles加入/跟踪Destination/移出，均非历史缓存容量。
- `Client/MirScenes/GameScene.cs:4687–4714`：源缺失return，4697出生音，4699创建410，目标非空且未死才挂570及命中音；Rust缓存miss不出生是既有披露差异。
- `Shared/ServerPackets.cs:3564–3578`仅Spell(byte)/Source(u32)/Destination(u32)；`Server/MirEnvir/Map.cs:2375–2383`仅集合/cell移除，不是事件ack/缓存回收。

门下条件（任一不满足不得宣称该项PASS）：
- C1：A/B只写新tests/support及两个integration targets；正常生产/wire/server/依赖diff=0，严禁参考函数导出到库或运行时调用。
- C2：预算与ticks是显式测试输入；排序(tick,id)、最小前缀、保护超额为本项目提案，不归因Crystal、不称生产LRU。
- C3：G1.1/2覆盖重复entry错误、未知/重复保护、预算0/≥len、全保护超额、并列tick与排列不变；逐支有数值断言。
- C4：G1.3须完整/候选真实UI身份[1,2,3,4]对[1]、坐标、出生cue4对1、consumed5及重复0；代号前提按M1锁定。
- C5：G1.4证明当前位置更新和新代清缓存；map epoch/同代ID复用/自动保护集未证明，明确PARTIAL。
- C6：有效红必须到指定assert且非零EXIT；禁0 tests/编译错/skip/故意panic/既有失败；幸存变异据实报，不计红。
- C7：红绿测试SHA相同，变异仅实现；每个attempt独立源码与target、禁硬链接，首败双流/UTC/EXIT/补丁/源码SHA永久保全。
- C8：71/125经真实encode→decode→apply；实体移除和历史筛选是非wire夹具，cue不证明实际可听，不称socket/Remove包闭环。
- C9：A→B→C→Lead门禁→D串行、C独立复跑；唯Lead功能提交与ci.ps1；门禁后任何crates变化使旧门禁失效。
- C10：原编号PARTIAL/历史freeze保全、失败退原owner；HEAD漂移先核影响；成本按M4封顶，需src修复则重新送门下。

必改/落实项（范围均为派发契约与测试夹具细化，不要求本审核者改原计划）：
- M1（B/Lead）：G1.3显式用WorldSnapshot::default同代0历史夹具并披露非生产会话初始化，71/125仍真编解码；断言UI/快照代号0，保持consumed5。new_session仅71预热会在首125被clear，不能照搬R66夹具；若改用新会话锚事件，须另行核定基数与计数。
- M2（B/C）：撤销“单纯交换当前位置/cache优先必红”承诺；先cache_visible_entities再spawn且advance覆写（projectile.rs:737,798–800,852–860），该变异可幸存。保留cache.clear移除红，另增仅71把11更新19→隐藏→125落点19的测试，以隔离禁用可见缓存刷新取真实UI红。
- M3（B/C）：去保护1不得改测试字面量；只在隔离support中让保护判断忽略ID1，测试SHA不变；原算法应保留[7,4]，候选view从[1]变[4]，失败须在真实UI身份/坐标断言。
- M4（Lead）：派发注明A≤1.25h、B≤1.25h（总≤2.5h，硬上限3h）及实际累计；超限停止扩面、保全证据报告PARTIAL；不得用GPT审计/文档小时混充deepseek预算。
- M5（A/B/C）：每次红命令同时保存绝对cwd/manifest路径、CARGO_TARGET_DIR和TMP三变量，证明编译的是red-copy而非工作树；核源码无硬链接且工作树SHA不变，防R66硬链接假红复发。
- M6（Lead/D）：把M1非wire代号、M2幸存原因/替代红、M3实现变异位置写进最终五行矩阵与红绿索引；预期值标为推导，实际通过数/EXIT不得预填；测试targets各≥2、ignored0，保全第一失败。

三问裁决：①批准，仅测试提案符合不自创玩法边界；生产容量/LRU仍PARTIAL（C1/C2）。②批准隔离最小变异，同SHA/首败/无硬链接为硬条件，按M2/M3使用可达负控。③批准历史预热模拟与非wire移除披露，按M1固定代号；map/ID复用、生产自动保护集和真正限容继续移后。
PARTIAL承接：R49–R60/R60 P-1–P-6；R61 P01–P12；R62①–④；R63 P1–P7；R64①–⑥；R65 G1.2仅值独立/G2未做；R66生产注册/退出/租约/补发/fan-out/全局水位；事件/位置容量、服务端回收、u64::MAX、素材/设备/Live不清零。
证据根：[attempt-1](../../target/round67/menxia/attempt-1/)；命令逐条JSON保留command/UTC/双流/exit，initial/final-sha-manifest记录核验源与freeze实值；静态审查不冒充cargo运行。
CLI：UTC 2026-10-26T22:40:35Z，`state JJC-20261027-004 Assigned --as menxia` EXIT0；`validate --as menxia` EXIT0（207条/schema v3.1）；show复读Assigned/尚书省/尚书令。建议放行测试契约，尚书派发必须带入M1–M6。
