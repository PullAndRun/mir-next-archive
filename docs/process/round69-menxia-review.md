# R69 门下省独立评审

- verdict：**PASS-WITH-CHANGES**；接受 C「契约+矩阵」限定轮，附下列强制执行/收口条件；不退回 A-局部。
- 审核官：gpt-6.1-sol / menxia；对象 `JJC-20261027-006`；基线 `9e8d9e828742f866f8b3a42f4bf352c21ce9667c`。
- 计划核验：`docs/process/round69-plan.md:1–78`，78行/11633B；SHA256 `d24b3df5a53696b29766bec40d7ddc419c8986fd4e183c7b77eb47f606efdd8c`。

## 六项审查

|必审项|判定|证据与结论|
|---|---|---|
|1 Crystal 锚点实读|PASS|计划:35–41；下表及 `target/round69/menxia/attempt-1/anchor-check.log`；G1–G5有源码依据，GameScene补入最终矩阵。|
|2 可失败性/真红|PARTIAL|计划:44–61,63–65,71，方案可执行但实现尚不存在，不能称已红绿；R1→G2、R2→G3、R3→G4、R4→G5均能形成语义矛盾，按条件2–4收口。|
|3 范围诚实|PASS|计划:5–7,12–15,72,75–77明确生产只读、GPU不背书、无新阈值；spellfx/全UI/真实素材设备/生产混合阴影mask保留PARTIAL。|
|4 单轮价值|PASS-WITH-CHANGES|接受一次有明确出口的契约轮；须交 actionable 缺口而非仅工具绿灯，并锁定R70生产消费切片（条件1、6）。|
|5 红绿与隔离|PASS（契约）|计划:57,59–65,71,77规定物理复制、禁止链接、同SHA、首败、独立root/out、失败保全；条件4补独立tmp及可信工具加载，尚未实测实现。|
|6 流程/CLI|PASS|state与validate均EXIT0；show确认Assigned；真实argv/输出见 `target/round69/menxia/attempt-1/runs.json`，未以计划命令冒充执行。|

## Crystal 抽查表（路径从仓库根开始）

|关项|实读证据|结论|
|---|---|---|
|G1|`github/Crystal/Client/MirGraphics/MLibrary.cs:640–673,675–752`|Point重载默认false；true时主体signed X/Y加一次后裁剪；整数坐标和section-opacity重载并不自动偏移。|
|G2|`github/Crystal/Client/MirGraphics/MLibrary.cs:692–710`|offSet默认false；699偏移先于701裁剪，R1移动顺序可被G2断言抓获。|
|G3|`github/Crystal/Client/MirGraphics/MLibrary.cs:704–709`；`github/Crystal/Client/MirGraphics/DXManager.cs:353–389`|只存bool；恢复调用默认rate=1F/mode=NORMAL；NORMAL=SourceAlpha/One，INVLIGHT=BlendFactor/InverseSourceColor；不等于完整状态保存。|
|G4|`github/Crystal/Client/MirGraphics/MLibrary.cs:778–797,880–901,919–928`|mask与主体共point/Width/Height；MaskX/Y有读取无绘制消费；Mask texture用主体尺寸且ReadBytes(Length)，不能按直觉修订为MaskLength。|
|G5|`github/Crystal/Client/MirGraphics/MLibrary.cs:862–900`；`target/round69/menxia/attempt-1/client-identifier-scan.json`|全集标识符搜索仅六行命中：2声明、4反序列化；未见shadow/mask坐标额外消费。仅限冻结Client C#，不推广至其他版本。|
|调用方|`github/Crystal/Client/MirObjects/Effect.cs:112–132,196–216`；`github/Crystal/Client/MirObjects/SpellObject.cs:323–347`|Owner/地图/运动偏移先形成锚点，绘制显式true；不把运动偏移与图像偏移混同。|
|GameScene|`github/Crystal/Client/MirScenes/GameScene.cs:10923–10935,11085–11087,11264–11273`|混合图有true和条件offset两路，普通图还有整数重载；照明另改混合状态，G3不能泛化为场景完整状态恢复。|
|否决全量A|`github/Crystal/Client/MirObjects/PlayerObject.cs:1708–1709,2956–2963`；`github/Crystal/Client/MirObjects/SpellObject.cs:49–55`|雷电Magic2施法20/3/300 Owner、落点10/5/400，火墙1630/6/120；确与现有通用事件不同。|
|Rust差距|`crates/mir-render/src/entities.rs:1816–1868,1923–1926`；`crates/mir-render/src/frame.rs:428–470`；`crates/mir-client/src/ui/spellfx.rs:248–265`|新入口逐帧signed saturating_add→frame.blit；旧入口无offset；Over/Add存在不等于DrawBlend rate/INVLIGHT/mask/shadow实现。|

## 强制附加条件（尚书纳入A/B/C/D验收，不满足不得Done）

1. A矩阵逐G输出「Crystal事实→Rust实际消费点→未实现行为→最小修复落点→可失败验收→优先级/责任方」；补GameScene条件offset、Draw各重载和照明状态边界；不只输出pass表。
2. A先由人工确认并冻结expected与来源SHA，工具仅产observed并比较；禁止由observed回填expected、把生成矩阵再读回当独立oracle。B明确矩阵缺项/重复ID的实际输入与调用路径；C独立逐段核原文和固定期望。
3. 四红分别只改变完整可解析C#有效方法内的语义；R4须在有效代码读取字段，不可注释/字符串伪造。输入SHA变化只作证据，不能早于目标语义断言拒绝；EXIT1必须来自对应G2/G3/G4/G5失败。补清计划:18的R1实际针对G2，G1测试独立断言false不偏移、true一次且先裁剪。
4. B/C每次独立物理副本、无symlink/hardlink，记录文件ID/链接数/源副本SHA及绝对root/out/tmp；可信审计器及测试从最终冻结版本加载，禁止从变异root偷换工具；同测试/工具SHA贯穿绿→四红→恢复绿，C独立重做变异并完整保全首次失败和重试。无cargo target复用需求，若产生target亦隔离。
5. ≥7测试须含五个命名语义测试，零skip；0 tests、导入/语法/编译错、EXIT2、既有失败均非有效红。C逐红核实际断言与首败原件；D仅抄真实命令/计数/EXIT/SHA，历史freeze JSON及既有证据不得变更。
6. R70至少在R69收口前选定并记录一个A-局部生产落点、来源依据与验收；优先仅渲染层消费逐帧offset，保持magic映射/时序/wire。不能连续以静态矩阵替代生产推进，也不能把本轮PASS升级为生产像素/GPU一致。

## 价值判断与R70建议

C值得一次：它纠正「全Draw自动偏移」「恢复三元状态」「MaskX/Y必被消费」三种会直接导致错误移植的假设，并提供漂移检测；但无法替代生产进展。
A-局部确实可行：可从 `crates/mir-render/src/entities.rs:1816–1868` 的旧/新入口和 `crates/mir-client/src/ui/spellfx.rs:257–265` 的消费边界选单一已证锚点；必须先决定该调用的坐标契约，不直接把全部居中入口替换。
最小判据：单一路径逐帧signed offset在裁剪前消费一次；合成非零正负/跨帧/屏外像素断言与失效变异；保持帧号/图库/时间/事件/wire；按计划:72补真实71/125回归并诚实保留素材设备限制。

## CLI 与证据回执

实际执行 `node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs state JJC-20261027-006 Assigned <附条件审核说明> --as menxia`：2026-10-27T03:22:39Z EXIT0，Menxia→Assigned；完整说明参数见 runs.json；`validate --as menxia` EXIT0，217条/schema v3.1；随后show确认Assigned。
证据：`target/round69/menxia/attempt-1/runs.json`、`anchor-check.log`、`source-manifest.json`、`client-identifier-scan.json`（97个C#文件/6行命中）；git diff --check EXIT0。未运行计划A/B/C实现测试，不宣称已真红。
如实披露：首次记录脚本在state已成功并保存后，打印✅触发GBK UnicodeEncodeError，脚本EXIT1；保全回执、不重试state，独立脚本仅补show/validate/status。历史两个未跟踪freeze JSON原已存在，未编辑。
门下审核循环退出：是（附条件准奏，交尚书执行；非R69功能验收完成）。

## Lead 附注（交付完整性）

本评审交付时「CLI 与证据回执」段留了一句模板占位句（"待执行…本句须在交付前替换"）。实际 CLI 已真实执行：`state JJC-20261027-006 Assigned --as menxia` 于 2026-10-27T03:22:39Z EXIT0（Menxia→Assigned），`validate --as menxia`（--json）显示 tasks=217 / issues=[]，`show` 复核状态 Assigned、任职尚书省。原始回执见 `target/round69/menxia/attempt-1/runs.json`；此处由 Lead 补记事实，不修改评审结论。
