# R70 门下省独立审议

- verdict：**PASS-WITH-CHANGES**（附执行/收口条件准奏）；主单 `JJC-20261027-007`；审核官 gpt-6.1-sol / menxia；非实现验收通过。
- 基线 HEAD=origin/main=`72d9313e225dbb825cece84ffd5753e0a1e25612`；[计划](round70-plan.md)115行/12765B，SHA256 `27a6e38875fdd5c73e0b47a7251c994896f66831351afcb718bbf1af34566194`，UTF-8无BOM/LF，26本地链接存在。
- 抽核于2026-10-27T06:10–06:13Z亲自用 `ops_read_files` 打开下表源码；机器复核命令 `node target/round70/menxia-attempt-1/review-evidence.cjs pre`，06:12:13Z EXIT0；[源清单/校验](../../target/round70/menxia-attempt-1/source-checks.json)记录SHA、UTC、链接及看板。

|抽核项|判定|证据与裁决|
|---|---|---|
|1 Crystal实读|PASS|下表；计划19–22、25正确：offset先于裁剪，NORMAL=SourceAlpha/One；没有One/One或GPU等价承诺。|
|2 Rust实读/最小diff|PASS|entities:1816–1870/1896–1927、scene:433–443、frame:275–300/428–470；共享私有实现可借缓存Image分派blit/blit_add，无需扩大公开API或改frame算法；旧blit_cached继续Over。|
|3 G1–G7与红绿|PARTIAL|计划65–82、88覆盖全清单且可执行；实现/最终测试尚不存在，不能认定实际红绿已通过；按C2–C4核收。|
|4 范围/合规|PASS|计划30–39、92–109；接受生产渲染API+测试消费者限定交付，无UI消费者不构成退案；完整C2/GAP-1、历史条件5及旧PARTIAL不得消账。|
|5 门禁|PASS|计划75、86–87明确Rust生产变更仅Lead执行最终唯一全量，不借R69通过、不留降级参数；本审议未运行cargo/ci。|
|6 CLI真实性|PASS|[规划回执](../../target/round70/zhongshu/cli-receipts.json):96–138与本次show-before一致：Menxia、4项not-started；本次Assigned/validate/show均EXIT0，222条，见末段。|

|Crystal亲读位置|源码事实|与计划核对|
|---|---|---|
|[MLibrary:692–711](../../github/Crystal/Client/MirGraphics/MLibrary.cs#L692)|默认offSet=false/rate=1；699叠当帧X/Y→701屏外早退→705设blend→707绘制→709仅以旧bool恢复。|19正确；不保存完整rate/mode。|
|[MLibrary:778–797](../../github/Crystal/Client/MirGraphics/MLibrary.cs#L778)|785主图偏移→787裁剪；792–794 mask共用主point/width/height，无MaskX/Y消费。|22正确；本轮不实现mask。|
|[DXManager:353–389](../../github/Crystal/Client/MirGraphics/DXManager.cs#L353)|378–379 NORMAL=SourceAlpha/One；373–375 INVLIGHT=BlendFactor/InverseSourceColor；383–384设置255×rate。|20正确；清单中的MirScenes路径不存在，实际为MirGraphics，计划链接无误。|
|[Effect:201–216](../../github/Crystal/Client/MirObjects/Effect.cs#L201)|201逐帧index、205/211运动位移；214普通/216混合均显式true，混合传Rate。|21正确；运动与图像offset分层，不动时序。|

- Rust公式亲读：[frame:468](../../crates/mir-render/src/frame.rs#L468) `contribution=(src*alpha+127)/255`，RGB=min(255,dst+contribution)；428–454 alpha=0不变，否则目标alpha=255。Over快速路径另有既有截断规则（309–351），不得借共同实现统一舍入而改变旧像素。
- 私有实现按计划32–35分派：旧draw_effect=(false,false)、旧with_offset=(true,false)、新blend=(apply_offset,true)；offset仅取当帧metadata，saturating_add后交i64裁剪；有图屏外true、缺图false。

强制条件（尚书纳入验收，不满足不得Done）：

1. **C1 / A范围**：仅entities/scene生产改动，直接借缓存不克隆/新增逐帧分配；保留旧Over与存在性语义；不改wire/server/magic、图库帧号/时序、不迁移UI、不改frame算法或历史freeze。
2. **C2 / B测试与oracle**：先独立从原始夹具/预定像素计算期望，再交A实现；不得调用生产blit/blit_add/add_channel或复制实现生成期望。≥7命名测试、0 ignored/0 filtered；覆盖±8跨帧、单次偏移、屏外移入/部分裁剪/i32极值、缺图/屏外返回、scene转发、false与旧Over/交替缓存；半透明补测锁alpha=0/128/255、非黑背景、饱和及alpha行为，两套T68全绿。
3. **C3 / B+C四变异**：在已编译最终API的有效生产代码中，M1关闭offset→G1位置断言；M2偏移前裁剪早退→G2移入像素断言；M3 Add改Over→G3非黑底通道断言；M4忽略false→G4绝对落点断言。四者都须同最终测试/工具SHA的green-before/red/green-after；编译错、0tests、import失败、skip/ignore或未知基线失败均不算红。
4. **C4 / 独立保全**：B/C独立物理副本，记录源/副本SHA、文件ID/链接数证明非symlink/junction/hardlink；各自cwd/out/TEMP/TMP/TMPDIR/CARGO_TARGET_DIR；C独立重做T70/T68及四变异，不代改B；保全首败原件、命令/UTC/EXIT/计数/断言/diff和恢复字节，测试/工具改动后重做四红与C。
5. **C5 / D诚实收口**：逐G只录真实结果，文档UTF-8无BOM/LF与本地链接；GAP-1–6、R70-C1–C3、R68与R49–R67沿原编号承接，条件5永久PARTIAL；完整DrawBlend/rate/INVLIGHT、公共API全面收敛、spellfx/GameScene条件、shadow/mask、全UI/真实素材/GPU设备均不能升级为完成。
6. **C6 / Lead门禁**：C/D完成后，仅Lead执行 `pwsh -NoProfile -File tools/dev/ci.ps1`，最终树唯一全量EXIT0且逐项成功；无SkipTests/SkipPostgres等参数；门禁后生产再改必须重跑，失败/超时追加attempt，不降级。

必改 M1–Mn：**无规划阻断项**；上文M1–M4专指既定测试变异，不是退案修订号。以上条件补强交付证据，不把尚未出现的功能测试当已完成。

- 真实执行 `node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs state JJC-20261027-007 Assigned --as menxia`：2026-10-27T06:12:42.488Z EXIT0，Menxia→Assigned；[原始回执](../../target/round70/menxia-attempt-1/state-assigned.json)。
- 真实执行 `node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs validate --as menxia`：2026-10-27T06:12:42.565Z EXIT0，222条/schema v3.1；[原始回执](../../target/round70/menxia-attempt-1/validate.json)；[show-after](../../target/round70/menxia-attempt-1/show-after.json)于06:12:42.623Z EXIT0确认Assigned、四todo未启动。
- 审议仅写本报告及指定attempt目录；未改crates/计划/既有证据，未commit/push、未跑ci.ps1、未启动执行者；[复核脚本](../../target/round70/menxia-attempt-1/review-evidence.cjs)保存实际命令与机器校验。审议循环退出：是，附条件准奏交尚书。
