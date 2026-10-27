# R72-C 独立刑部核验证据

- 子单：`JJC-20261027-009-sub-da472dd2`；基线 `69b2e0c`；结论 **BLOCK**；必改/待裁量 **2 项**，不可据本报告直接放行提交门禁。
- 实跑 UTC 2026-10-27 11:49:19–11:54:14；最终静态封存 11:57 UTC。所有命令、cwd、UTC、EXIT 见 [runs](../../../target/round72/c/attempt-1/runs.json)、[基线 clippy 对照](../../../target/round72/c/attempt-1/baseline-clippy-run.json)。
- 固定生产 SHA `df572677d7b013c3c09dab9c764afbda2095cacec820f3292bfc7abca2f78be6`（386行/17755B）；测试 SHA `58f64ab2d30b69f3aea5c07a8975e988ca842bce45972499a6f93e120958baea`（1660行/65322B）。

|必做项|结论|独立事实及边界|
|---|---|---|
|1 范围/诚实|PASS|前后 status 恰 A 两文件+两 freeze 共4条；protocol/server/wire diff 空；新增 ignore/allow=0；两 freeze SHA/mtime 与 B/R69一致。|
|2 新副本/回归|FAIL|全新 archive+覆盖A，nlink=1/无链接；T72 9、R71 8、R70 9、R63 3、projectile 26全部0失败0忽略 EXIT0；fmt0；同feature clippy101，见必改2。|
|3 A 红相位|PASS|同测试SHA，副本生产取基线：1 passed/8 failed/0 ignored EXIT101；恢复9/0/0 EXIT0；均真实运行9例。|
|4 B 变异抽查|FAIL|B编号M2 clamp及M4 offset独立build0→红101→恢复9/0/0；其余B日志/SHA自洽，但三项语义不符合获批矩阵；C另完成获批七语义，见下。|
|5 独立oracle|PASS|新Python直接struct/zlib解WZX/WZL；G1三像素、G3移入/裁剪、G4双格Add，四例整帧SHA与A expected/生产actual三源一致。|
|6 Crystal对照|PASS-WITH-PARTIAL|Magic2 1650–1669/20×120ms/格锚点/先offset后裁剪接线正确；Light3、rate0.8、严格NextMotion/寿命等继续PARTIAL，未冒称等价。|
|7 真红/只增|PARTIAL|A/B既有142文件前后SHA/bytes/mtime全同；B七红均真实9例且无编译错；M6保offset合理且有披露；其余矩阵替换未披露并有错误结论。|
|8 文档诚实|PASS|docs全文检索未见R72全量通过/Crystal等价扩称；尚无R72产品交付文档。G7全量仍只由Lead执行，本轮C未运行。|

独立复跑命令（实际带串行参数处以 runs 原件为准）：
```text
 git -c core.autocrlf=false archive 69b2e0c -o target/round72/c/attempt-1/baseline.tar
 cargo test -p mir-client --features mir-render/legacy --test round72_poison_cloud_mapping -- --test-threads=1
 cargo test -p mir-client --features mir-render/legacy --test round71_firewall_mapping
 cargo test -p mir-client --features mir-render/legacy --test round70_effect_blend_offset
 cargo test -p mir-client --features mir-render/legacy --test round63_projectile_wire
 cargo test -p mir-client --features mir-render/legacy --lib ui::projectile::tests
 cargo fmt --check
 cargo clippy -p mir-client --features mir-render/legacy --all-targets -- -D warnings
```
- 副本 `target/round72/c/repo`；独立 `CARGO_TARGET_DIR=target/round72/c/cargo-target`、`TEMP=TMP=TMPDIR=target/round72/c/tmp`、`CARGO_PROFILE_DEV_DEBUG=0`，均绝对路径实际传入。
- A红 UTC11:50:25 EXIT101：串行首败 `r72_g1_wire_mapping:939:5`，(381,261) actual `[0,0,0,255]` expected `[248,4,0,255]`；目标 `r72_g3_offscreen_crop:1262:5` 同时红，(2,194) actual `[0,0,0,255]` expected `[248,252,0,255]`。不把并发调度顺序当一致性条件。
- B抽查 clamp：UTC11:50:56，7/2/0 EXIT101，首败age_window:1090；offset：UTC11:51:27，2/7/0 EXIT101，首败G1:939。两项先 `--no-run` EXIT0，恢复分别UTC11:51:00/11:51:28绿9/0/0。
- C按获批语义七项：Magic、Bolt、clamp、尺寸居中、offset=false、保offset Over、去来源限制；各独立物理副本、独立target（从C热缓存物理复制，无硬链接），测试SHA不变；build全部0、红全部101、恢复全部9/0/0。依次红计数1/8、2/7、7/2、2/7、2/7、7/2、8/1；patch与恢复SHA见 [mutations](../../../target/round72/c/attempt-1/mutations.json)。
- oracle UTC11:50:39，诊断测试9/0/0 EXIT0；只在另副本转储A expected/生产actual及保留夹具，诊断SHA `93bd851bbb53dd0ec1011774ecdb741e25aed4124e5cf5699509aceeaa9ea270`，与原测试独立复跑明确分开；Python未调用A/B计算helper。
- oracle G1三点(381/382/383,261)各 `[248,4,0,255]`；G3移入(2,194) `[248,252,0,255]`，负裁剪丢3源像素后(0,0) `[0,0,248,255]`；G4同格双事件 `[255,58,60,255]`。四整帧 THREE_SOURCE_MATCH，原始buffer/夹具/三SHA见 [oracle输出](../../../target/round72/c/attempt-1/oracle/oracle-output.json)。
- Crystal实读 `SpellObject.cs:57–64/323–339`、`MLibrary.cs:692–709`；Rust `PoisonCloud && from_object_spell`、`(age/120)%20`、`1650+phase`、Swing零起点、格锚点与offset=true均吻合有限契约。
- PARTIAL承接：严格NextMotion/初始100ms取整/每Process一步、149持久场/26移除及真实寿命、rate0.8/DrawColour/INVLIGHT/Light3/mask/tint/shadow、其余26技能GAP-5、真实素材/GPU视觉；4000ms是产品瞬态窗。R71-C7历史缺口不清零。

必改/待Lead裁量（发现后已通知Lead，C未越权自修）：
1. **B证据属主**：获批M1 Magic/M2 Bolt/M3 clamp/M4居中/M5 offset/M6 Over/M7来源；B实做base1640/clamp/移到center/offset/Magic/Over/1200ms，缺Bolt、尺寸居中、来源限制。其 [manifest结论:103](../../../target/round72/b/attempt-1/manifest.md#L103)“一一对应”不实。须新attempt追加获批矩阵与更正，不覆写原件；C成功补测不自动注销B契约责任。M6 `draw_effect_with_offset`正是计划的单因子隔离，偏差披露合理。
2. **Lead/相应实现属主**：本要求同legacy feature clippy EXIT101；`mir-render/src/minimap.rs:353`、`tiles.rs:311`均 `clippy::needless_return`。取基线生产重跑同命令仍101，证明既有问题；B clippy0未带legacy，不能作本项绿证。计划原命令确未带feature，但当前核验要求更严格，需Lead显式裁量或属主修复再完整复验，C不改实现。

- [静态审计](../../../target/round72/c/attempt-1/static-audit.json)、[最终审计](../../../target/round72/c/attempt-1/final-audit.json)、[逐文件SHA清单](../../../target/round72/c/attempt-1/sha256.json)及logs保留全部结果；源码/测试/主树其他文档均未写，未commit/push、未跑ci.ps1。
