# R77 门下省独立审议

- 主单 JJC-20261028-004；裁决 **PASS-WITH-CHANGES / approve**；仅批准生产零改的差异/准入合同，M1/C1–C5为后续交付约束，不代表七G六M已通过。
- 全文实读计划153行/19,895B/SHA256 `d2138698ff4751009775881fa97d2914cc2721005721477683a9a0c6fb6ceaaa`；HEAD=origin/main=`69428266cd00d900abafa97bd415b47b4f4782a5`。
- 六项结论：①源码锚点PASS；②有限完成定义PASS；③八套WZX事实PASS、全部格式来源调查PARTIAL；④七G六M设计PASS、实施实跑PARTIAL；⑤历史合同处置PASS；⑥范围/门禁设计PASS、未来落地PARTIAL。

|实读来源|核验结论|
|---|---|
|Crystal `SpellObject.cs:30–38,132–138,225–231`|Location/Repeat=true；Trap=Magic2/2360/100ms/8/Blend；Portal同段但非同协议语义。|
|Crystal `SpellObject.cs:288–302,323–339,363–370`|首100ms取整、>=且每Process一步；格投影、DrawBlend(true,0.8F)；AnimationOffset为格单位。|
|Crystal `MLibrary.cs:692–709,778–795`；`DXManager.cs:353–384`|offset先裁剪；mask复用主point/尺寸；NORMAL SourceAlpha/One，不能推出src×0.8。|
|Rust `spellfx.rs:28,40–60,309–435`|Trap默认700ms，60ms取帧clamp0..9，Magic/居中/Over；149 TrapHexagon独立4000ms分支。|
|Rust `entities.rs:497–523,1893–1927`|Swing=Magic2/start0；offset与Add分开；直接寻址能力不证明Trap接线。|
|Rust `net.rs:1541–1560,5624–5647,7424–7434`；`generated.rs:5616`|149写event及field、身份分离；Trap=98；无核心锚点臆造。|
|历史 `round76_trap_hexagon_mapping.rs:2297–2311`；`round59_projectile_assets.rs:98–100`；`analyze_usage.py:330–338`|Magic2精确0–2/10–14/20–29共18；R59目录为magic，现集合116。|

- 素材独立读字节：默认 `resource/data/magic2.wzx`=5968B、declared1460/physical1480；WZL=10,290,606B，2360..2367均MISSING_INDEX；尾20槽无准入价值。
- 八WZX清单与大小/SHA详见[二进制证据](../../target/round77/menxia-review/logs/asset-independent.json)：default、Shanda mirt/mir/story/return/sec、2025、Arcadia；声明数依次1460/1218/1460/1130/1210/1218/2580/1210。
- 2025 WZX=10,368B、WZL=14,033,479B；2360..2367偏移13,316,944至13,356,455，八帧format5，zlib解压长度逐一匹配4字节对齐RGB565；语义仍UNVERIFIED_SEMANTICS，NOT_READY合理。
- 新候选：`resource/community/mirfiles-default-crystal/Default Crystal/Data/Magic2.Lib`，v2/count2790/21,091,901B/SHA `f449aa4c96632f13454d7746ab47678c06175be02e69a6dc8fdfd036755fc0d2`；2360..2367 gzip八帧均匹配w×h×4。
- `resource/community/mirfiles-crystal-patch/crystal__patch_gz__Data__Magic2.Lib`同库同SHA；Crystal `MLibrary.cs:513–532,883–890`支持该格式。名称/相同副本仅为认证线索，无实存语义绑定，尚不能宣告来源已认证；详见[候选证据](../../target/round77/menxia-review/logs/crystal-lib-candidates.json)。
- **必改M1（计划:25/47/144/149）**：交付文档限定“八套本地WZX”而非全部Magic2格式；承接上述Lib路径/SHA/可解压事实与来源绑定缺口至R78。不得默认为2025背书、不得据候选改本轮生产或扩格式白名单；C核验D落盘后关闭此项。
- 可选建议：将解析格式/上限/损坏错误分类写进工具模块说明；Lib v1候选不属于现Crystal v2+读取证据，不以文件名认证。

- G1–G7逐项设计PASS：G1真实149错库/锚点金样；G2负age/60ms/700ms/极值；G3来源/param/控制技能；G4八帧公开API与offset/Add；G5拒绝/绑定/损坏退出码；G6精确case与116/18旧集合；G7真实UI/缺帧/状态不变。每项均有可失败固定断言。
- M1–M6逐项设计PASS：换库→G1；700→800→G2；Over→Add→G1；禁offset→G4；放行未认证→G5；Trap→TrapHexagon解析→G6。按既有签名均可形成有效变异；未在本审议实施变异。
- C1（计划:67–70/78）：合成known fixture绑定只证明算法；真实binding须独立门下审议。无绑定/SHA错/证据SHA错require2；普通audit0不等于READY；错误1与NOT_READY2分开。
- C2（计划:70/83/96/109）：七绿0失败0忽略；每M单patch、no-run0、指定G恰1断言红101、恢复SHA后七绿；0tests/编译错/资源缺失/异常不算红。先冻expected/fixture及首actual，独立Python整帧oracle不调用生产helper。
- C3（计划:96/104–105/125）：B/C分别六M，物理副本nlink1，禁symlink/junction/hardlink；独立target/temp；源码/测试/夹具/工具原SHA保全；清单生成器固定排除/排序并可重算，失败attempt只增。
- C4（计划:107/124–126）：D只记本人实跑文档检查与原日志；全量仅Lead且不SkipTests/SkipPostgres，内部skip逐项披露；官方/legacy clippy原EXIT分列；全量后crates/tools变化重闸。
- C5（计划:52–62/126/144）：未来代码仅新增一测试一只读工具；生产/wire/server/历史测试/素材零改；D/Lead获批文档例外按表执行。唯一Lead提交；本审议无commit/push。
- 历史冲突PASS：R76 G8旧18与R59 magic116保持精确断言；新只读审计不改变analyze_usage集合；R77不新增supersede。R78若映射，先认证/绑定、保全旧帧，再裁定18→26及R77 G1/G2/G3/G7替代范围。
- PARTIAL照承计划:130–144全部原号：GAP-1..6/R70-C1..3、R71时钟/对象/R73-T1、R72/R59有限supersede及lint、R75七项、R61–R67细项、R76证据损失；不补造历史、不宣称生产映射/严格Crystal/设备视觉/素材认证/整轮通过。

- CLI state原值：`state JJC-20261028-004 Assigned --as menxia`，EXIT=0；UTC `2026-10-28T01:52:31.673Z`→`01:52:31.739Z`，1条状态回执，Menxia→Assigned。
- CLI validate原值：`validate --as menxia`，EXIT=0；UTC `2026-10-28T01:52:31.741Z`→`01:52:31.812Z`，原文“257 条任务符合 schema v3.1”；无重试。show后态Assigned。
- 原始argv/UTC/EXIT/bytes/SHA/完整stdout/stderr见[runs](../../target/round77/menxia-review/runs.json)、[notes](../../target/round77/menxia-review/notes.md)和logs；审议未跑cargo/CI，未改计划或生产。
