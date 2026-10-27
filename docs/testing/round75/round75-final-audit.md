# R75 刑部独立终审

- verdict：**PASS-WITH-PARTIAL；必改清单：无（0）**。仅裁定所审提交与证据；不表示整轮验收完成。
- 被审 HEAD/origin/main：`6b6e4bc4e620b23aac18353c7ca385987659e6d7`；核验窗 UTC 2026-10-27T20:57Z–21:06Z；执行者 gpt-6.1-sol。
- 完整命令、UTC起止、EXIT、计数、日志bytes/SHA：[runs](../../../target/round75/t/attempt-1/runs.json)、[checks](../../../target/round75/t/attempt-1/checks.json)、[补核](../../../target/round75/t/attempt-1/supplement-runs.json)；以下 PASS 均有独立实算。

|必做项|裁定|实值与依据|
|---|---|---|
|1 提交链/范围|PASS|`git log --oneline 6502a90..HEAD` 恰5提交：96e46cc→a6e77ef→30246bc→baf71d2→6b6e4bc；入场status仅两历史freeze；相对96e46cc，crates仅A新增测试；protocol/server diff空、生产src零改。|
|2 manifest|PASS|38/38逐项复算bytes/SHA；tracked由git cat-file blob取原字节并复核OID，worktree由文件字节；delivery_head=baf71d2，4个交付文档OID均等于该提交；stage=pre-audit，无自引用。|
|3 门禁原字节|PASS|gate_head=30246bc为HEAD祖先；meta 12字段（含attempt）及gate段一致；GB18030解码原log末段12 OK/0 FAIL、「全部通过（12 项）」；UTC20:44:09Z–20:47:52Z、EXIT0。|
|4 独立隔离复跑|PASS|HEAD archive全新物理副本，测试覆盖SHA校验；R75=7/0/0；按实际测试文件R74/R73/R72=5/9/9，全部EXIT0；fmt0、双包clippy默认features0。|
|5 真红/历史日志/oracle|PASS|M1/M4均no-run0→exact红101（0/1/0、6 filtered）→SHA恢复→各7/0/0；A/B抽11份日志UTC/EXIT/计数一致；独立抽2整帧及2像素一致，见下。|
|6 文档诚实性|PASS/PARTIAL|无生产寿命修复/严格等价/Blizzard迁移/整轮已通过的正面扩称；C首FAIL→C2 BLOCK→三份裁定/勘误→C3 PASS链如实披露；A/B只增补录。README:29逐套回归计数错位为轻微勘误，合计23不变。|
|7 卫生/交接|PASS|新增ignore/allow=0，tracked target文件=0；README53行/6803B、contract40行/4810B、manifest581行/19766B；client/roadmap仅尾部追加12/14行；两freeze原字节不变。|

- 清单实算：19,766B，SHA256 `5c881579988a9baf628acaa9e360823a01c20d291aa8cb6fefbe2e85225a07aa`；外置SHA摘要内容一致，**HEAD blob及工作树均87B（CRLF），题面86B应勘误**；不影响摘要或38项身份。
- 门禁log：214,417B，SHA256 `e6eec12095ea80b042dde250f4f1fd55d3d4b76aa5708a94d2da15df2aa38342`；meta：481B，SHA256 `4483324991985472bc1bbb7e4a023fdb5d1d53b37e88564512858edd7c8a1fba`。
- 测试：56,403B/1454行/7 tests，SHA256 `ecff550dfa0bcd671d32723bd18fbcd444c93c4d173c1562bfbc0f8ad185a05c`；G1/2/4/5/6生产实画，G3/7纯模型；G语义与计划/合同/测试一致。
- freeze-before：44,844,165B／`70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de`；freeze：45,219,533B／`c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32`。
- 隔离副本/环境：`target/round75/t/attempt-1/repo`，nlink=1；全新CARGO_TARGET_DIR=`target/round75/t/attempt-1/cargo-target`；TEMP/TMP/TMPDIR均同根tmp；CARGO_PROFILE_DEV_DEBUG=0。
- 独立命令：`cargo test -p mir-client --features mir-render/legacy --test <R75/R74/R73/R72文件>`；`cargo fmt --check`；`cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings`（默认features，未冒充legacy lint）。
- 独立绿合计44 passed：R75基线7+两次恢复14+历史回归23；另两条有效红各1 failed、两次no-run0；不重跑全量门禁。
- M1：仅模型`now >= self.next`→`>`；首败测试940行，left `(0,1300)` / right `(1,1420)`；EXIT101，恢复测试SHA=ecff550d…后7绿。
- M4：仅副本spellfx.rs:30 `AREA_MS 4_000→2_000`；首败测试1142行，age2000 pixel(391,262) actual `[0,0,0,255]` / expected `[248,0,248,255]`；EXIT101，生产原SHA恢复、测试SHA不变后7绿。
- Oracle：读取A冻结与独立oracle结论、B82/82、C125 agree/0 mismatch；本官自写Node WZX定位/WZL解压/倒序RGB565整帧复算，无调用A/B/C生成器：G1 SHA=8b9e6c36…，pixel(381,261)=[248,0,0,255]；G4 SHA=2daeabaa…，pixel(391,262)=[248,0,248,255]，均与冻结值一致。
- A/B日志抽核11项：A七exact，B基线/回归/fmt/clippy（静态UTC/EXIT由原summary核对）；均一致；C3 55 PASS/0 FAIL与原报告相符，不冒充本次矩阵。
- 历史PARTIAL原号承接：GAP-1/2、R70-C2；GAP-5/R70-C1、R70-C3、GAP-3/4/6；R71时钟/对象、R73-T1；R72 B plan-M4/legacy lint；R71-C7永久/历史E1仅补救CLOSED；R69条件5；R68/R49–R70/R61–R67全部原号。
- 细项不清零：R61 P01–P12、R62①–④、R63 P1–P7、R64①–⑥、R65 G1.2/G2、R66生命周期/水位、R67容量保护/map epoch；R74 artifacts=34；R75-E1/C-M1 CLOSED-AS-PARTIAL（失败字节不可恢复），R75-E2/C-M2声明级PARTIAL（首次actual stdout缺失）；C3只关基线身份冲突。
- 非阻断勘误：README:29的R74/R73/R72=9/9/5应按文件为5/9/9；外置SHA87B；均在本报告记实，不改交付物或清单。真实素材/GPU/设备/完整UI/跨后端/Live音频仍未覆盖。
- 工具偏差：首次tar绝对Windows路径被当远端，EXIT128及runner EXIT1已保留；改相对路径后解包0。静态解析曾将GB18030当UTF8、B summary当内嵌header，均在读取原字节后修正，未改历史证据。
- 范围声明：仅写本报告及`target/round75/t/attempt-1/**`；不改测试/生产/manifest、不commit/push、不运行tools/dev/ci.ps1；隔离副本保留。看板回报另见证据内delegate-result日志（若终态拒绝照录）。
