# R74 门下省独立审议

裁决：**PASS-WITH-CHANGES（附条件准奏）**；主单 JJC-20261028-001；允许尚书按 C1–C4 登记派发，非执行验收。
基线 HEAD=origin/main=0ee34e1b86b6fa2a9a9e1506a08e022eee5bb965；本审只静态读证，不跑 cargo/CI、不提交。

|六项审查|结论|实读依据与判断|
|---|---|---|
|1 切片正当性|PASS|计划:6–18/110/117；普通 NORMAL CPU 回归锁有独立增量，可作为正式测试锁轮；须有行为断言，不要求生产行为修正。|
|2 G1–G5 可失败性|PASS|计划:57–78；G1错库/20、G2 signed offset、G3逐步混合、G4 alpha128、G5缺帧续画均比较独立金样；M1–M5可击中，实际红绿待执行。|
|3 执行契约与门禁|PARTIAL|计划:43–47/83–105 DAG和物理隔离清楚；C1补先冻结时序；保留 Lead 唯一全量门禁，理由见下。|
|4 写范围与保全|PARTIAL|只准新 round74_normal_blend_lock.rs 和 spellfx.rs:309 注释；不开放 frame.rs 单测区或额外 helper 文件；C2补独立注释提交；新增 allow/ignore=0。|
|5 D 文档与禁称|PARTIAL|计划:47/91/113–117已有六文件与禁称；C3固定章节编号、清单防自引和逐项核验；不得称修复完整混合缺口。|
|6 事实与研究|PASS|计划118行/10663B/SHA c4c2066ea4231d93af783133809c63d9309d23bc1d3049877d2c22c51add7b93，UTF-8/LF；初始status恰计划+2 freeze；research:21–30与原码一致。|

C1（A/B/C，验收前）：首个生产 actual 日志前保存独立期望 RGBA/夹具/测试 bytes+SHA+UTC；B/C各自解析实际WZX/WZL、另算G4向量，三源整帧一致。每个M保存前绿5/0/0→build EXIT0→指定断言 EXIT101→原字节恢复→后绿5/0/0；同测试/夹具SHA，独立target/TEMP且普通文件nlink=1，无链接；禁0tests/ignored/编译或feature错冒充红。
C2（Lead，提交前）：注释独立小提交只改 spellfx.rs:309 为 Time>NextFrame 才推进；记录parent/commit/blob/SHA与单行diff，可执行语句零差异。其余 crates 仅一个新测试文件；历史测试/生产零差异，新增allow/ignore=0；game-assets与两freeze不入库、freeze逐字节SHA不变。
C3（D/终审）：交付 docs/testing/round74 下 README、round74-normal-blend-contract、final-manifest、manifest.sha256，另 docs/client.md §5.39、docs/roadmap.md §二十二；清单逐项blob或工作树来源/bytes/SHA，排除自身和外置摘要；外置摘要校验manifest；链接有效，原号与禁称逐项核对。
C4（Lead）：按计划:96–103运行5新+38历史（按实数核计）及fmt/clippy；收口唯一Lead运行 ci.ps1 -KillStale，不SkipTests/SkipPostgres，原始UTC/EXIT/实际门禁计数全存 target/round74/gate；失败保留attempt。D纯文档后静态核验，crates/tools再变更重闸。

门禁裁决：**本轮须全量**。tools/dev/ci.ps1:1–3明确唯一门禁入口、收口前全量；:118–120全workspace含legacy。新集成测试进入workspace且含服务端开发依赖，仅定向不足以满足现有收口合同；不要求A/B/C重复全量。脚本头注释13项与计划12项不等价，按实际结果逐项计数，不能硬填12。

|Crystal 实读路径与行号|事实|
|---|---|
|github/Crystal/Client/MirGraphics/DXManager.cs:353–384|缓存bool/rate/Mode；NORMAL :378–379=SourceAlpha/One；INVLIGHT :374–375=BlendFactor/InverseSourceColor；设置BlendFactor不能证明NORMAL乘rate。|
|github/Crystal/Client/MirGraphics/MLibrary.cs:692–709|:699先offset，:701后裁剪；:704仅存bool，:705传rate，:709默认参数恢复bool。|
|github/Crystal/Client/MirObjects/Effect.cs:24/38–61/81–96/113–130|Rate默认1；NextFrame初始化；Time<=NextFrame返回，严格>才推进；owner/source落点、DrawBlend传Rate。|
|github/Crystal/Client/MirObjects/SpellObject.cs:288–302/335–339|首次100ms取整；>=NextMotion每次一步且从当前时间重排；149调用rate0.8。|

Rust实读：entities.rs:509–522为Magic/Magic2与0/20产品映射，:1883/1903–1927为一次offset与Add；frame.rs:428–470给出四舍五入CPU公式，alpha128 [101,53,7]叠底[10,20,30]确为[61,47,34]。G4可用公开Frame/Image在新集成测试完成，无须扩大写范围。
R70测试:3/42–64/1041–1093仅Magic单库；跨库同索引异色、Magic2 0/20/21诱饵与交替序列有增量。spellfx.rs:294–309和net.rs:1541–1561支持无状态draw不足以重建两套Process时钟的否决理由。
PARTIAL承接：GAP-1/2 / R70-C2为完整DrawBlend；GAP-5 / R70-C1为映射（题面C1歧义并记）；R70-C3、R71时钟/对象、R72 B plan-M4、R73-T1、GAP-3/4/6原号不变。R71-C7永久PARTIAL、E1补救CLOSED；R69条件5及R68/R49–R70/R61–R67照承。仅注释勘误可单独CLOSED。
风险：RGB565仅alpha0/255，G4直接RGBA只锁CPU；NORMAL/rate/Mode设备状态、真实素材/GPU/视觉/全UI/跨后端/Live音频未验收。禁称整轮通过、严格Crystal等价、设备视觉签核、完整DrawBlend闭合、rate0.8已实现、时钟/寿命已修复、C7已补齐、基线功能红或无条件PASS。
证据：target/round74/menxia-review/notes.md 与 checks.json，含命令原始EXIT/UTC、冻结文件校验及看板状态/validate条数；本报告不冒称未来测试已通过。
