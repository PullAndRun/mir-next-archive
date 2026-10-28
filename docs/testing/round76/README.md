# R76：149 困魔咒十帧映射（post-audit）

- 主单 JJC-20261028-003；D 礼部文档批 + 独立终审 **PASS-WITH-PARTIAL、必改 0**（[终审报告](round76-final-audit.md)）；尚书验收/门下确认待办；整体证据口径 PASS-WITH-PARTIAL，不称整轮通过。
- DAG：A兵部→B工部→C刑部→Lead提交/门禁→D礼部；[计划](../../process/round76-plan.md)、[门下](../../process/round76-menxia-review.md)、[派发及澄清](../../process/round76-shangshu-dispatch.md)、[裁定1–5](../../process/round76-lead-ruling.md)。
- 身份：功能 `9bf6ec6` → 裁定5 `23cb621` → R59 fix `41425b1` → C3 `02c3ea8` → D 文档批 `b743c23` → 清单 `72fb0a4`；post-audit 阶段以 `final-manifest.json` 的 `delivery_head` 为准（本页为 D 写作时 pre-audit 历史快照）。
- 功能提交实读 5 文件、4008增/1603删；素材脚本包含行尾/排版差异，不能报成约76增/6删。fix 提交仅R59、29增/14删；`9bf6ec6..41425b1` 实际含R59与裁定文档2文件。
- 有限行为见[trap合同](round76-trap-contract.md)：仅149来源困魔咒、Magic1390–1399、100ms elapsed `%10` 回卷；4000ms为从700ms新增的精灵产品窗，公共/非149/文字仍700ms。
- 素材计划只给MAGIC_FRAMES追加十帧；本地真实十帧可读、旧106帧数据不变，game-assets被gitignore，不入git；合成像素/素材探针不等于设备视觉签核。

|证据|实读结论与历史|
|---|---|
|[A attempt-1](../../../target/round76/a/attempt-1/manifest.md)|同SHA测试先红2绿/6败 EXIT101，:1051 `(381,261)` actual `[0,0,0,255]` / expected `[120,132,248,255]`；后绿8/8；R72/71/70/73/75=9/8/9/9/7。|
|[B attempt-1](../../../target/round76/b/attempt-1/manifest.md)|七M最终全部前绿→编译0→目标红101→恢复SHA→后绿；六套回归；独立oracle真实十帧及像素采样；82/82是R75历史数，R76此manifest未报该总数。|
|[C原核验](round76-c-evidence.md)、[C2](round76-c-addendum.md)|原M7未有效红；C2新副本补Python0/no-run0/G8红101/恢复8绿，C独立七M 7/7 CLOSED；整体仍PASS-WITH-PARTIAL。|
|[A R59补做](../../../target/round76/a/attempt-2/notes.md)、[B复核](../../../target/round76/b/attempt-2/manifest.md)、[C3](round76-c-supersede-addendum.md)|R59有限supersede CLOSED；6/6、R76 8/8、R72 9/9；旧106子集指纹不变。|

- R72 supersedes **仅**G5的149 Trap旧通用像素锁：原:1383/:1404注释、:1408/:1429枚举换Reincarnation，另加顶注；expected/fixture不改。原65,322B/`58f64ab2…`保全；现66,437B/`3a9f42bd…`；A旧sidecar错误由追加勘误闭合，流程PARTIAL。
- R59 supersedes仅冻结集106→116、追加1390..1399及对应常量/指纹/措辞；旧585行/19,447B/`2cfff50c…`保全，新600行/20,596B/`849770b2…`；四样例、WZL哈希、六测试语义不变。
- 门禁attempt-1：HEAD9bf6ec6、EXIT1，唯一失败R59三测试106/116冲突；[原log](../../../target/round76/gate/attempt-1.log)69,152B/`979cc747…`，未被后绿抹除。
- 门禁attempt-2：HEAD41425b1、EXIT0、12/12，UTC 2026-10-28T00:28:12–00:31:25Z；[log](../../../target/round76/gate/attempt-2.log)215,013B/SHA `85bdd3e6b5b725dd52d32ec9d0e43f038318f09c2a2208e7d674cc421afb1027`；[meta](../../../target/round76/gate/attempt-2.meta.txt)802B/`71c9ca8c…`。meta记门禁后crates/tools/docs零差异；其后C3与D为文档批。
- 门禁命令与内部跳过项原样见meta：`cargo test --workspace -- --skip duo_ --skip fault_pg`；12/12不能扩称所有测试无条件执行。
- 环境披露：A debug的ring/MSVC D8050→`CARGO_PROFILE_DEV_DEBUG=0`；C的R76/R72独立复跑需`--features mir-render/legacy`。legacy clippy两处既有lint EXIT101，官方无legacy workspace clippy EXIT0。
- A R59补做另修本地index的written_png 106→116；其首跑失败日志覆盖损失照实保留声明；B早期失败日志永久损失（裁定2）；B固定work/共享target偏差、测试头M编号错位均见B原清单，不当批准变异定义。
- 原号PARTIAL：GAP-5/R70-C1其余26技能；R70-C3全局锚点/居中；GAP-1/2/R70-C2完整DrawBlend/rate/Mode及src×0.8未实施且不可从BlendFactor推导；GAP-3/4/6 AnimationOffset/Light/DrawColour/mask/tint/shadow消费未齐。
- 原号PARTIAL：R71时钟/对象、R73-T1严格NextMotion/Repeat/26寿命/owner/ActionFeed；R72 B plan-M4/legacy lint；R71-C7永久/历史E1仅补救CLOSED；R69条件5；R68/R49–R70/R61–R67；R74 artifacts实数34；R75-E1/C-M1、R75-E2/C-M2不追溯清零。
- 新流程台账：A sidecar瑕疵PARTIAL、B早期失败日志永久PARTIAL；C原M7无效历史保全，C2只闭合有效复现缺口；不得称生产寿命已修或严格Crystal等价。
- [post-audit 逐项清单](final-manifest.json)（含终审报告；T2 记为 producer-declared 无生成器的 inventory 仍 PARTIAL）与[单行外置摘要](manifest.sha256)无自引用；本地A/B/C证据inventory有明确排除的构建/源码副本范围，不替代原始证据。
- D自检要求的`target/round76/d/attempt-1/checks.json`与`logs/`当前快照不存在，未伪造；已实际执行JSON/逐项SHA/bytes/外置摘要、链接检查、diff检查并回报EXIT。链接工具跳过target证据，A/B/C原始证据另查实存。
- 编号披露：D 初稿曾按授权在文末追加同号节（client §5.40/roadmap §二十三与 R75 重复）；Lead 复核后重编号为 client §5.41/roadmap §二十四，R75 历史节编号保持不变。
