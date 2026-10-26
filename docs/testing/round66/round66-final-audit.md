# R66 刑部独立终审

**verdict：PASS-WITH-PARTIAL；必改清单：空（0）。** 仅测试/契约层成立，生产能力及整轮最终验收不由本报告外推。
审计对象：功能 `b99874211c906cfe9d0a75f03dabd5050863547e`；文档 `02ca88bb89cf74744c3268cb51fc2d99735f66d3`；基线 `f14a81d`。
T＝gpt-6.1-sol 独立终审；Lead 仅为既有门禁 operator，不代签独立结论。复跑/复算依据为指定提交字节及工作树，不以 A/B/C 自报替代核验。

|必做项|判定|独立核验事实 / 证据|
|---|---|---|
|1 门禁字节/提交链|PASS|原 log 3549行/213139B，SHA `1d04184a0de71f36f454229177a237847912507a34d3ac9b8bdab7a94891d524`；meta 15行/454B，SHA `6bcd9d4f4aff097e6cb8c94eedf68f9d896f434827bbc18f5e36c69ecc93024e`；最终摘要12 OK/0 FAIL、EXIT0、UTC 21:48:08–21:51:46Z；提交链 f14a81d→b998742→02ca88b，crates门禁后空diff。[静态证据][static]|
|2 隔离复跑|PASS|全新 T target/tmp，先 cargo clean -p mir-client；下表57项，补bin1后58项，全部EXIT0/0失败/0忽略；fmt-all、workspace/all-targets Clippy -D warnings均0。[运行记录][runs]|
|3 G1.1/1.4 与全集合优先级|PASS|support:47–74按 InvalidBounds→NoConsumers→GenerationMismatch→BelowReclaimed→BeyondTail→min；contract:206–343五Err真实表不变/裁剪0，:360–434交叉成员正反序，:454–554晚加入/旧代/空集与合法floor2。|
|3 G1.2/1.3|PASS|boundary:250–351真实71/125，min2→(2,2,4)[3,4]→(2,3,5)[3,4,5]；慢新增[3,4,5]/cue3、快[5]/cue1、重复0；:356–501现状分支(4,1,5)、慢[5]/cue1，新增差分1对3，旧S4仍1..4。|
|3 G1.5 与公式|PASS|net:9850–9915预送同Receiver S1/S2、命中S2，try_recv=None、commands=Empty；无本地ack/线程/睡眠。1-based含本条水位=reclaimed+i+1；boundary:209–212实断言，饱和恢复仍PARTIAL。|
|3 G1.6 四行兼容矩阵|PASS|单app UI：app:210–229单水位上报；旧clone：boundary:496–500值独立、不钉权威表；waiter：net:3853/3869共用Receiver；未来成员：support:41仅显式假设集合。生产源码不引用模型，注册/广播未实现。|
|4 有效真红|PASS|A m1–m4、B m1/m2/m2b/m3原始EXIT101且运行到目标assert；C独立attempt-2/3仍在，非编译错误/0tests/skip。左右实值及各日志/补丁SHA见[红复核][red]；下文裁定保全偏差。|
|5 原清单14项|PASS|逐项lines/bytes/SHA全吻合；4个crates从git show b998742复算且git_blob相同，3个文档从git show 02ca88b复算，其他从工作树；A/B清单110个SHA均匹配。原manifest154行/5003B/SHA `29af7eae7632ff705aa3ef42110b44517ee75eca815ca3a7c35c97b1bb53cb94`。[复算][manifest]|
|6 文档诚实性|PASS / PARTIAL|README140行/7402B、水位60行/3422B、C证据61行/8786B，均匹配指定SHA；旧FAIL表明确为修复前快照，最终A3/总58已订正。无生产能力或R66最终验收正面完成断言；命名偏差已披露；roadmap冻结合理。|
|7 卫生/范围|PASS|仅4文件+1269；net旧前缀逐字节未变、新增138行位于既有cfg(test)；wire/server零diff；新增ignore/allow=0，历史ignore=8→8；两freeze初始及当前均??、SHA与派发前相同、未暂存。links与diff-check均EXIT0。|
|8 终审裁定|PASS-WITH-PARTIAL|无实现/门禁/核心证据实质缺陷；偏差登记非阻断。无需Lead修实现；后续尚书/门下依职责验收，提交此报告后的清单须继续据实维护。|

|独立命令（cargo test -p mir-client）|passed / failed / ignored|
|---|---|
|--test round66_watermark_contract|3 / 0 / 0|
|--test round66_consumer_boundary|2 / 0 / 0|
|--lib r66_|1 / 0 / 0|
|--test round65_projectile_lifecycle；--lib r65_|5 / 0 / 0；5 / 0 / 0|
|--test round63_projectile_wire；--test round62_projectile_wire|3 / 0 / 0；5 / 0 / 0|
|--test round64_event_reclaim；--test round64_projectile_ack|4 / 0 / 0；3 / 0 / 0|
|--lib ui::projectile::tests::|26 / 0 / 0|
|补充 --bin mir-client r65_|1 / 0 / 0（总58）|

与A/B/C最终自报一致；C旧57项因旧A仅2项，最终58含bin1；本次必做不含bin的57项与其口径区分清楚。
真红实值：A m1 :252 Ok(4)/Ok(2)，m2 :300 None/Some(NoConsumers)，m3 :513 None/Some(GenerationMismatch)，m4 :407 Some(BeyondTail)/Some(BelowReclaimed)。
B m1 :190 suffix 0/2；m2/m2b :205身份1/3；m3变异net:9916 commands非Empty。C attempt-2 :407 Some(BelowReclaimed)/Some(GenerationMismatch)，attempt-3复核A三红，同最终contract SHA `62f36a3923d682bc01dcd8a53446c09add97d68e588e7654bbb796b66acd54eb`。
历史裁定：A README:8/80明确v1/m1–m3覆盖，违反首败永久保全，属于证据保全偏差；C initial/attempt-2/3仍能独立复核核心主张，故非BLOCK，不把A旧首败称为已保全。以后每变异/修复新建attempt、封存原件及SHA，禁止覆写。
B硬链接target假红已披露并clean重编消除；T使用全新目录复跑r65_/r66_均绿，不沿用A“外部net改动”旧归因。C首轮F1已由A修复、交叉优先级绿及C attempt-3证据成立。
命名偏差：直接指令采用watermark，旧计划contract名未产出；已公开且不影响内容可定位，非阻断。T仅复核既有红原件及C独立红，不伪称本次另执行变异。
C1–C8逐项：C1范围PASS；C2模型/现状区分PASS；C3成员边界PASS；C4身份/cue差分PASS；C5同ReceiverPASS；C6有效红PASS（保全偏差另列）；C7历史PARTIAL保留；C8当前门禁/SHA无漂移（历史覆盖偏差另列）。
PARTIAL承接：R49–R60/R60 P-1–P-6；R61 P01–P12（含多消费者/长期内存、生产动作、位置容量、渲染、设备Live/削波、105sfx、HiddenSpanSource、CWD、饱和、缺素材、历史）；R62原四项；R63 P1–P7；R64原六项；R65 G1.2仅值独立/G2未做，原编号均不清零。
生产多消费者/全局min、注册退出租约补发、并发fan-out、事件/位置容量LRU、服务端回收、u64::MAX恢复、真实素材/可听/Live仍PARTIAL，须另案设计与可失败验收。
独立性：T未改crates/冻结文档或JSON，未commit/push/跑ci.ps1；本报告针对上述功能HEAD及文档HEAD，后续crates变化须新功能提交/新门禁。Lead仅operator签注，不替代独立终审或最终责任链。

[runs]: ../../../target/round66/t/attempt-1/runs.json
[static]: ../../../target/round66/t/attempt-1/static-audit.json
[red]: ../../../target/round66/t/attempt-1/independent-red-meta.json
[manifest]: ../../../target/round66/t/attempt-1/manifest-recompute.json
