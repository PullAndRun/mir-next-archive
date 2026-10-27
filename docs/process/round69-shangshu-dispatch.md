# R69 尚书省派发令
- 主单 `JJC-20261027-006`；HEAD `9e8d9e828742f866f8b3a42f4bf352c21ce9667c`；本轮仅候选 C 文档/工具契约，不改生产渲染。
- 依据：[计划](round69-plan.md)78行，SHA256 `d24b3df5a53696b29766bec40d7ddc419c8986fd4e183c7b77eb47f606efdd8c`；[门下评审](round69-menxia-review.md)条件1–6强制收口。

|单|真实ID|部门/执行模型|depends_on|只写范围|
|---|---|---|---|---|
|主|JJC-20261027-006|尚书省/gpt-6.1-sol|—|本派发文档、target/round69/shangshu/**|
|A|JJC-20261027-006-sub-4757d48a|兵部/deepseek|无|tools/testing/round69_render_contract.py；docs/testing/round69/render-contract.json；target/round69/a/**|
|B|JJC-20261027-006-sub-9ca99e64|工部/deepseek|A|docs/testing/round69/contract-tests.py；target/round69/b/**|
|C|JJC-20261027-006-sub-fcc589b0|刑部/gpt|B|docs/testing/round69/round69-c-evidence.md；target/round69/c/**|
|D|JJC-20261027-006-sub-6326a5ec|礼部/gpt|C|docs/testing/round69/README.md；docs/client.md；docs/roadmap.md；target/round69/d/**|

- DAG：A→B→C→D；上游最终版本冻结后才启动下游；单文件单owner，C独立于A/B。
- 模型列为执行路由要求；本派发代理深度已满，Lead接收真实ID后负责启动及补真实agent映射；此表不冒称执行代理已启动。
- 所有子单已在CLI交付契约明示白名单；禁止改crates、既有测试、资源、历史证据、两freeze JSON、计划与评审；不commit/push，不跑cargo/ci.ps1。

|门下条件|责任映射与关闭证据|
|---|---|
|1 actionable缺口|A逐G写Crystal事实→Rust实际消费点→未实现→最小修复点→可失败验收→优先级/责任方；补GameScene条件offset、Draw重载、照明边界；C逐项核实，D记PARTIAL。|
|2 独立expected|A先人工逐段确认冻结expected与来源SHA；工具仅产observed比较，严禁observed回填；B实际注入矩阵缺项/重复ID并记录调用路径，C独立核原文。|
|3 有效语义红|B四变异仅改完整可解析C#有效方法；R1→G2裁剪顺序、R2→G3恢复参数、R3→G4 mask坐标、R4→G5有效字段读取；C独立重做。SHA变化不得抢先拒绝；G1独立核false不偏移、true一次且先裁剪。|
|4 隔离/保全|B/C每次独立物理副本、无symlink/hardlink，记录文件ID/链接数/源副本SHA及绝对root/out/tmp；可信最终工具和测试从冻结主版本加载，同SHA贯穿绿→四红→恢复绿，首败与重试完整保全。|
|5 真执行|B≥7命名测试含五G语义，零skip；C核实际目标断言首败原件；0 tests、导入/语法/编译错、EXIT2、既有失败均不算红；D只抄真实命令/计数/EXIT/SHA。|
|6 R70生产前置|A给来源与最小点，C核坐标契约，D收口前登记下述单一候选及判据；本轮不得将静态PASS升级为生产像素/GPU一致。|

- A契约：标准库`audit(root: Path) -> dict`，完整唯一方法体及注释/字符串排除；schema_version=1，source_files/checks/coverage/limitations，G1–G5各一次，含observed/expected/pass/anchors；G5冻结Client全集标识符扫描。
- A验收：`py -3 tools/testing/round69_render_contract.py --root . --out target/round69/a/audit.json`，EXIT0、checks=5、全pass、零缺源。
- B验收：`py -3 docs/testing/round69/contract-tests.py --root . --out target/round69/b/green.json`，EXIT0、≥7命名测试、0skip；缺文件/重复方法拒绝及矩阵缺项/重复ID必须实际调用。
- B命名语义：g1_draw_offset_order、g2_blend_offset_order、g3_restore_and_factors、g4_mask_coordinate、g5_shadow_usage_inventory。
- B/C四红：同一冻结工具/测试命令仅改`--root`/`--out`，每个R1/R2/R3/R4独立副本，EXIT1必须来自G2/G3/G4/G5目标语义首败，日志含期望/实际/方法/行号/目标断言布尔；恢复绿EXIT0。
- C验收：独立复跑A/B命令（out改C目录）及四红；A/B/C逐文件manifest SHA一致无自引用；条件1–6逐条证据化，剩余必改0才交D；`git diff --check` EXIT0。
- D验收：真实回执/索引/PARTIAL台账；`py -3 tools/docs/check_links.py`、`git diff --check`、UTF-8无BOM/LF探针EXIT0；不得复制计划命令冒充实跑。
- R70前置选定候选：只迁移`crates/mir-client/src/ui/spellfx.rs:257–265`中FireWall单一路径的渲染坐标消费，评估取消宽高居中并接`crates/mir-render/src/entities.rs:1847–1868`逐帧offset入口；其他spell保持既有行为。此处登记候选，不执行生产修改或声称契约已成立。
- R70依据/准入：Crystal `github/Crystal/Client/MirObjects/SpellObject.cs:323–346`地图/运动锚点后显式true；`github/Crystal/Client/MirGraphics/MLibrary.cs:658–670`先signed offset再裁剪。C/A须验证FireWall该调用坐标契约及图库映射，若不成立须Lead另选已证单一路径，不能直接替换全部居中入口。
- R70验收：单一路径逐帧signed X/Y在裁剪前仅消费一次；非零正负、跨帧、屏外合成整帧RGBA断言及失效变异；保持magic映射、图库/帧号、时序、事件、wire；补真实71/125回归并保留素材/设备限制。R69仅事实前置登记，R70另行生产范围审批。
- 门禁边界：按计划12–15，本轮文档/工具契约轮不启全量Rust门禁；后续确有需要由Lead裁定。保留spellfx/全UI/真实素材设备/生产DrawBlend-shadow-mask的PARTIAL。
- 风险：方法误定位、全集扫描漏标识符、mask直觉误读、默认blend状态误推；A准确提取与来源冻结，C逐段重读及四红验证缓解。
- 回滚：仅撤回本轮新增工具/文档及client/roadmap对应文本；全部失败证据保留，不reset/clean、不回写Crystal、不覆盖旧attempt。
- 真实CLI回执：四次delegate均EXIT0；`state JJC-20261027-006 Doing --as shangshu` EXIT0；`validate --json --as shangshu` EXIT0，tasks=221、issues=[]；show-after=Doing。
- 证据：`target/round69/shangshu/attempt-1/runs.json`及各CLI原始stdout/stderr；A登记后包装stdout的GBK打印emoji故障EXIT1已保全，读取真实A ID后仅续B/C/D，无重复委派，CLI本身无异常。
