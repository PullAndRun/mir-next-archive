# R69 混合、阴影、遮罩与 offset 契约矩阵 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use subagent-driven-development or executing-plans to implement this plan task-by-task. Steps use checkbox syntax.

**Goal:** 交付 Crystal DrawBlend/shadow/mask 与 offset 的有来源矩阵、可失败源码契约审计及独立核验；本轮限定候选 C 的契约层，不改生产渲染。
**Architecture:** 从本地 Crystal 的完整方法体提取坐标、裁剪、混合状态和遮罩事实，和 Rust 现状逐行对照。审计工具验证已确认源码契约与矩阵完整性；不把静态 PASS 当 GPU 像素一致。
**Tech Stack:** Python 3 标准库、JSON、Markdown；Crystal C# 与 Rust 源码只读。
**Spec:** 本轮直接任务要求候选 C；R68 边界见 [契约](../testing/round68/round68-offset.md)、[终核](../testing/round68/round68-c-final.md)。
**Task:** `JJC-20261027-006`；基线 `9e8d9e828742f866f8b3a42f4bf352c21ce9667c`，origin/main 同步；规划官只写本计划与 `target/round69/zhongshu/**`。

## Global Constraints
- 规划阶段禁 cargo/ci.ps1、commit/push；执行阶段也不为文档契约轮启动全量 Rust 门禁；任何生产改动须退中书另行审批范围。
- UTF-8 无 BOM、LF；不读/输出 .env 密钥；不改资源、历史证据、freeze、协议、服务端、crates 或既有测试。
- 尚书先登记 A/B/C/D 子单及真实 agent 映射，再启动；严格 DAG A→B→C→D，独立 C 不兼任 A/B。
- 本轮后续执行白名单：`tools/testing/round69_render_contract.py`、`docs/testing/round69/**`、本计划、`docs/client.md`、`docs/roadmap.md`、`target/round69/**`；单文件单 owner。

## Review Focus
- offSet=false 默认值与 true 路径必须分开；不能把所有 Draw 都解释成自动偏移（A G1、B R1）。
- signed X/Y 在裁剪前加一次；不得把宽高居中、运动偏移或 mask 偏移混为同一语义（A G1/G2、B R1/R3）。
- DrawBlend 只保存 oldBlend，恢复调用默认 rate/mode；不得描述成保存三元状态（A G3、B R2）。
- HasMask 绘制复用主体 point/Width/Height；MaskX/Y 已读取不等于被绘制消费（A G4、B R3）。
- shadow 字段存在不证明已有 shadow 绘制路径；负向扫描只对冻结的 Client 源码范围负责（A G5、B R4）。

## 背景、选择与边界事实
- R68 看板 `JJC-20261027-005` 已 Done；[旧索引:9–12](../testing/round68/README.md#L9) 的“待终审”是历史文档阶段，不覆盖看板事实。
- [client §5.33](../client.md#L1391)、[roadmap §十六](../roadmap.md#L263)：R68 仅 125/570 signed offset；旧 draw_effect 与 spellfx 居中未迁移；混合/阴影/mask 保留 PARTIAL。
- R68 C 最终为 12 passed/0 failed/0 ignored，RGBA 直接比较；门禁 attempt-2 EXIT0，不能把历史结果当本轮实跑。
- R67 `target/round67/d/attempt-1/checks.json:3–19` 披露 D 失败后 Lead 补齐及链接复跑；R68 `target/round68/c/final/static-audit.json:45–57` 绑定最终测试 SHA；新证据同样必须披露作者、重试与字节来源。
- 否决 A 本轮全量迁移：spellfx:248–265 为通用 10×60ms、Magic0/Bolt20 起并按尺寸居中；Crystal 雷电施法20/3/300且Owner，落点10/5/400；火墙1630/6/120。只换入口不能称严格对标。
- B UI 全矩阵跨文字/物品/血条坐标体系，尚无足够单一迁移点依据；本轮不扩大。D 需真实素材来源和设备签核，本次未验证具备，不能承诺；E 仅兜底，C 已有直接源码缺口与可失败交付，不选 E。

## Crystal 对照表（路径均从仓库根开始）
|关项|Crystal 文件:行号与事实|Rust 文件:行号与本轮判定|
|---|---|---|
|G1 基础偏移|`github/Crystal/Client/MirGraphics/MLibrary.cs:658–670` 默认 offSet=false，true 时先 point.Offset(mi.X, mi.Y)，再裁剪|`crates/mir-render/src/entities.rs:1816–1868` 旧入口不加；新入口 signed saturating_add；保留现状|
|G2 混合偏移|`github/Crystal/Client/MirGraphics/MLibrary.cs:692–710` DrawBlend 同样在裁剪前加 offset|`crates/mir-render/src/entities.rs:1923–1926` blit_cached 走 frame.blit；非 DrawBlend 完整实现|
|G3 状态|`github/Crystal/Client/MirGraphics/DXManager.cs:353–387` NORMAL SrcAlpha/One，INVLIGHT BlendFactor/InverseSourceColor；MLibrary:704–709 只保存 bool、恢复 SetBlend(oldBlend)|`crates/mir-render/src/frame.rs:428–470` Over/Add CPU 通道算法存在；不据此宣称 rate/mode/GPU 完全一致|
|G4 mask|`github/Crystal/Client/MirGraphics/MLibrary.cs:778–795` DrawTinted 先加主体 X/Y；HasMask 在同一点、同主体宽高绘制；:880–901 读取 signed 主体/shadow/mask 元数据；:919–928 mask texture 仍用主体宽高|本轮仅核实元数据/渲染差距，不新增独立 MaskX/Y 定位规则|
|G5 shadow|`github/Crystal/Client/MirGraphics/MLibrary.cs:862–889` 声明并读取 ShadowX/Y；本地 `Client/**/*.cs` 点成员消费初扫无命中|不得自行规定 shadow offset 加主体 offset；输出完整扫描清单和限定性阴性结论|
|消费锚点|`github/Crystal/Client/MirObjects/Effect.cs:112–132,201–216` offset=true；`github/Crystal/Client/MirObjects/SpellObject.cs:323–346` 地图锚点后绘制|`crates/mir-client/src/ui/projectile.rs:927–932` 仅两路 R68；spellfx:257–265 旧居中留存|
|否决A依据|`github/Crystal/Client/MirObjects/PlayerObject.cs:1708–1709,2956–2963`；`github/Crystal/Client/MirObjects/SpellObject.cs:49–55`|不同帧段/Owner/生命周期，另轮完整建模|

## 交付接口与可失败标准
- A 新建 `tools/testing/round69_render_contract.py`，仅标准库；接口 `audit(root: Path) -> dict`，CLI `--root <repo> --out <json>`；EXIT0=G1–G5 全确认，EXIT1=契约矛盾/缺项，EXIT2=输入/源码无法读取或方法无法唯一定位。
- 解析按准确方法签名+成对花括号提取完整方法体，排除注释/字符串中的伪 token；不能只按固定行号或全文件关键词存在即通过；各证据输出路径、起止行、SHA256和原文短片段。
- 结果 JSON 固定含 `schema_version=1`、`source_files`、`checks`（G1–G5各一次）、`coverage`、`limitations`；每 check 含 id/observed/expected/pass/anchors；缺项与重复 id 拒绝。
- G1/G2 验证 offset 条件、次数、先于裁剪；G3 验证保存与恢复调用形态及 NORMAL/INVLIGHT 精确因子；G4 验证主体与 mask 坐标同源，禁止误写 MaskX/Y 已消费。
- G5 对所有 Client C# 文件按标识符使用扫描 ShadowX/Y/MaskX/Y，区分声明、反序列化、读取、注释；列每个命中与文件全集 SHA，发现额外消费则失败并退回修矩阵，绝不删除命中。
- 工具测试在 B 新建 `docs/testing/round69/contract-tests.py`，用标准库 unittest，不依赖真实素材；测试审计器对完整源码快照的观测及输入损坏拒绝，不模拟 GPU。

## A→B→C→D 子任务 DAG
### A 兵部：实现源码契约审计与事实矩阵（depends_on=[]）
- [ ] 只写工具、`docs/testing/round69/render-contract.json` 和 `target/round69/a/**`；矩阵列完整方法签名、默认参数、偏移来源、裁剪顺序、混合因子/恢复、mask/shadow、Rust差距和证据层级。
- [ ] 先冻结 G1–G5 与源码 SHA，运行未实现工具的失败只算开发过程；完成工具后生成矩阵，逐项读原方法确认 expected，禁止工具自生成 expected 再自证。
- [ ] 验收：`py -3 tools/testing/round69_render_contract.py --root . --out target/round69/a/audit.json`，EXIT0、checks=5、全 pass、零缺源，矩阵与观测逐项一致；不自动改源码。
### B 刑部集成测试：真实源码输入＋物理隔离变异（depends_on=[A]）
- [ ] 只写测试、`target/round69/b/**`；从冻结仓库物理复制必要目录，无 symlink/hardlink；记录绝对 root、文件ID/链接数和源/副本SHA，禁止在主工作树变异。
- [ ] 5 个命名测试 `g1_draw_offset_order`、`g2_blend_offset_order`、`g3_restore_and_factors`、`g4_mask_coordinate`、`g5_shadow_usage_inventory`，加载真实源码并调用 audit；另测缺文件和重复方法 EXIT2，矩阵缺项拒绝。
- [ ] 变异 R1 把 DrawBlend 的 offset 移到裁剪后；R2 恢复调用改为 `SetBlend(oldBlend, rate)`；R3 mask point 改为叠 MaskX/Y；R4 在有效代码加入 ShadowX/Y 读取。逐个独立副本运行，不叠变异。
- [ ] 验收：`py -3 docs/testing/round69/contract-tests.py --root . --out target/round69/b/green.json` EXIT0，命名测试≥7且零skip；四变异用相同命令仅换 `--root`/`--out`，各 EXIT1 且目标 G2/G3/G4/G5 断言首败；恢复后绿。
- [ ] 红日志含期望/实际、方法、行号、目标断言布尔值；审计器SHA和测试SHA必须与最终绿相同。改测试后此前红全部失效，必须新 attempt 重跑。
### C 独立核验：事实、真红与声明边界（depends_on=[B]）
- [ ] 只写 `target/round69/c/**` 与 `docs/testing/round69/round69-c-evidence.md`；读取A/B原件、独立物理副本，复跑上述审计命令与测试命令（out换C目录），四红逐一复现。
- [ ] 核对G5全集扫描而非仅点成员搜索；核对G3“只恢复bool”而非完整恢复；确认crates/既有测试/资源/历史证据零变更。
- [ ] 验收：绿EXIT0、≥7执行/0skip，四个指定断言红EXIT1；`git diff --check` EXIT0；A/B/C manifest逐文件SHA一致且无自引用。剩余必改须0方可交D。
### D 礼部：契约索引与PARTIAL台账（depends_on=[C]）
- [ ] 只写 `docs/testing/round69/README.md`、`docs/client.md`、`docs/roadmap.md`、`target/round69/d/**`；记录实际文件/命令/EXIT/计数/SHA与C结论，不把计划命令充作执行记录。
- [ ] 验收：`py -3 tools/docs/check_links.py` 与 `git diff --check` EXIT0；UTF-8无BOM/LF探针EXIT0；文档逐项覆盖G1–G5及下述保留边界。

## 全局红规则、风险与回滚
- 所有有效红必须先有同输入未变异绿基线、目标测试实际执行、可读取/可解析源码、目标语义断言首败；0 tests、编译错、语法/导入错、既有失败、skip均不计红。
- 本轮红证明“源码契约漂移能被审计抓到”，不证明 Rust 生产像素已修复；若后续扩大生产行为，必须另补合成素材整帧RGBA差分及真实71/125回归后才可验收。
- 风险：源码提取器误定位、阴性扫描漏裸标识符、mask历史实现怪异、D3D状态默认值误推导；缓解为唯一完整方法体、注释剔除、全集token清单与C逐段人工重读。
- 覆盖清单：G1基础Draw；G2 DrawBlend；G3状态/因子；G4 DrawTinted与mask读取/创建；G5 shadow元数据/消费；Effect/SpellObject调用点；Rust Over/Add差距。
- 不做清单：新混合算法、rate/INVLIGHT模拟、shadow生成或新mask定位、spellfx帧/居中迁移、全UI矩阵、服务器/wire、下载/素材改写、真实设备视觉签核、历史证据修补。
- PARTIAL：R68真实素材/设备、全UI、spellfx、DrawBlend/shadow/mask生产实现仍保留；本轮仅闭合其“有证据契约矩阵”子项；R49–R67原编号、R68旧措辞/共享日志限制全部承接。
- 回滚：不触碰主仓库生产文件；失效时仅撤回本轮新增工具/文档以及本轮client/roadmap对应文本，保存全部失败证据；不执行reset/clean、不回写Crystal，不覆盖旧attempt。
- 待裁决项：无；门下只需核本限定范围与可执行验收。通过后尚书按DAG登记委派；本规划官不代门下批准。
