# R67-C 刑部独立核验证据
**verdict：BLOCK；必改 3 项（A lint、B lint、B manifest），退原 owner；不得据此进入 Lead 门禁或宣称 R67 通过。**
子单 `JJC-20261027-004-sub-0b916504`；核验 gpt-6.1-sol / xingbu；基线 HEAD=origin/main=`52d95b133d040e6d3c4ddded113be126059b2111`。
本报告对应 A/B 原始交付 SHA 快照；后续 owner 修订须新 attempt 复核，不覆盖首败。
UTC：任务起点 2026-10-26T23:21:22Z；主要命令 23:22:38–23:26:02Z；结束以 CLI 回执为准，15min 硬预算内。
证据根：[attempt-1](../../../target/round67/c/attempt-1/)；[逐命令 UTC/EXIT/双流 SHA](../../../target/round67/c/attempt-1/runs.json)、[补充命令](../../../target/round67/c/attempt-1/supplemental-runs.json)、[静态审计](../../../target/round67/c/attempt-1/static-audit.json)、[交付 SHA](../../../target/round67/c/attempt-1/deliverable-sha256.json)。

|必做项|结论|独立事实与左右值|
|---|---|---|
|1 范围/白名单|PASS|`git status --porcelain` EXIT0：crates 仅 A 两新文件+B一新文件；`git diff` EXIT0 空，生产/wire/server/旧测试零变更，无新增 ignore/allow；另三份批准流程文档与两份历史 freeze 为未跟踪，非实现变更。|
|1 历史保全|PASS|两 freeze SHA 对门下 initial 清单逐一相同：before=`70845aef…3dc9de`，after=`c7d896e7…ecde32`；实际生产路径是 `crates/mir-client/src/ui/projectile.rs`，SHA=`b82950a5ef521c75f82a5348e26a18d95b78b09c5624bb7645a041f46df5f9ce`，与 HEAD 相同。|
|2 独立复跑|FAIL|9组定向测试28 passed/0 failed/0 ignored；补计划要求2组29 passed；fmt EXIT0；clippy EXIT101（下述5条 lint），不能算通过。|
|3 独立实现变异红|PASS|`git archive HEAD` EXIT0 解包后覆盖 A/B 三文件；仅 support 的 over_budget 算式改为0；同测试 SHA，目标 assert:40 左0/右1，EXIT101，真实1测试失败，非编译错/skip/故意panic。|
|4 G1.1|PASS|固定 entries/protected/budget2→evict[20,30]/remaining2/over0；反序同值，并列tick按id；数值assert与独立绿一致。|
|4 G1.2|PASS|全保护超额3/1；budget0→[20,30]/1/1；budget≥len、未知/重复保护、重复entry Err且输入不变均有断言；独立红命中全保护超额。|
|4 G1.3|PASS|同代gen0；真实71源7/目标1..4预热；125序列[1,2,3,4,999]；完整四身份四坐标/cue4，对候选[1]/(11,10)/cue1；两支consumed5、重复sync新增view/cue0。|
|4 G1.4|PASS|71可见刷新到(19,10)→隐藏→125回退(19,10)；新代清cache view/cue0、游标1；本代(13,10)/cue1、重复0；独立绿与B有效红对应。|
|4 G1.5|PARTIAL|实现范围已核；五行最终矩阵属D后续写域，本次仅核批准计划，不声称D交付已存在或通过。|
|5 M1–M6|PARTIAL|M1–M5事实核对如下；M6最终矩阵待D，且当前BLOCK禁止先行放行。|
|6 证据SHA|FAIL|A manifest 127 evidence_files全部SHA/bytes匹配、无重复；B manifest 126条中125匹配，唯一自引用条目失配；A四红+B三红测试文件均与各自绿SHA一致，源码multi_link_count均0。|
|7 诚实性|PASS|测试文件明确生产无界、测试提案/历史模拟/非wire夹具；无生产限容/全局水位/125全链/R67通过正称；中文诊断存在，未按中文UI文案断言（C6非阻断）。|
|8 静态|PASS|`py -3 tools/docs/check_links.py` EXIT0；`git diff --check` EXIT0；最终报告新增后复查记录于 final-checks。|

所有绿命令使用全新绝对 `CARGO_TARGET_DIR=C:\Users\diana\Documents\github\mir-next\target\round67\c\cargo-target`，TMP/TEMP/TMPDIR 均为同根 `c\tmp`。
下表前缀为 `cargo test -p mir-client`；每项均 EXIT0、failed0、ignored0，实际命令/精确UTC见 runs，非0 tests。

|参数|passed|UTC起止（2026-10-26）|
|---|---:|---|
|`--test round67_position_policy`|2|23:22:38–23:23:20|
|`--test round67_position_boundary`|2|23:23:20–23:23:23|
|`--lib r66_`|1|23:23:23–23:23:33|
|`--test round66_watermark_contract`|3|23:23:33–23:23:36|
|`--test round66_consumer_boundary`|2|23:23:36–23:23:38|
|`--test round65_projectile_lifecycle`|5|23:23:38–23:23:41|
|`--lib r65_`|5|23:23:41–23:23:42|
|`--test round63_projectile_wire`|3|23:23:42–23:23:44|
|`--test round62_projectile_wire`|5|23:23:44–23:23:47|
|`--lib ui::projectile::tests:: -- --nocapture`（计划补充）|26|23:25:59–23:26:00|
|`--test round64_projectile_ack`（计划补充）|3|23:26:00–23:26:02|
合计57次通过执行，lib筛选与projectile集合可能重叠，不称57个互异测试；指定9组计数28。

独立红：[meta/源码SHA/绝对路径/环境](../../../target/round67/c/attempt-1/independent-red-meta.json)、[首败](../../../target/round67/c/attempt-1/independent-red/first-failure.txt)。
命令：`cargo test --manifest-path C:\Users\diana\Documents\github\mir-next\target\round67\c\red-copy\Cargo.toml -p mir-client --test round67_position_policy r67_protected_overflow_zero_and_duplicates -- --exact --nocapture`。
cwd绝对同 `c\red-copy`；target=`c\red-target`，TMP三变量=`c\red-tmp`；stderr明确编译该副本；源码无硬链接（multi_link_count=0），工作树测试SHA不变。
UTC 23:24:29–23:25:10；EXIT101、0 passed/1 failed/0 ignored/1 filtered；指定G1.2 assert `round67_position_policy.rs:40:5`，entries三项全保护/budget2，left0/right1。
测试红绿SHA=`b3953cb5e5682d2748e81b6557c8b3bea2f8339c73dac1da2c3362e83d134545`；support变异前=`80b88e62…ab49`、后=`ccb494fc603f59d5c2764a9620d054fec6fcd196f01c1bce7b5c23e38d4b78fa`。
红stdout SHA=`91cc7eac8b3d84d361b207311c7b66e7f1ad9adfad56ce16202739e202da26c1`；stderr SHA=`ffc7beff5df5c088ae30c5dcd5f9cb9fcf45ec586855d2f62d2c2945d3a3f885`。
本次独立变异为常量0，与A m4预算减remaining不同；两者在全保护分支同为0≠1。首败和被核输入副本永久保留于证据目录。

|门下项|结论|核验实值/披露|
|---|---|---|
|M1|PASS|B用default同代0，未用new_session清掉预热；UI/stream断言0，consumed5；手工代号与历史筛选非wire已披露。|
|M2|PASS|B删clear：line392实际[(1,(19,10))]≠空；去可见刷新：line367左(11,10)/右(19,10)，两者EXIT101、1pass/1fail；交换优先级可能被刷新/advance覆盖，未计为有效红。|
|M3|PASS|隔离support仅忽略保护ID1，保留[7,4]为算法推导；真实UI line286左[4]/右[1]，EXIT101；B测试SHA=`229facde…122e`与绿同。|
|M4|PASS（回执口径）|A初次看板回执约17min，含m4补证Lead输入约21min；B约15min；均≤75min，共约36min≤150min；命令UTC另存，不把命令运行时间等同owner全工时。|
|M5|PASS|A四份、B有效三份绝对cwd/manifest/target/tmp齐全，编译来源指向red-copy；独立重算同测试SHA，7份源码multi_link_count=0；C红亦满足。|
|M6|PARTIAL|计划/派发含五行矩阵、非wire代号、幸存原因/替代红、实现变异位置要求；D待写，不预填最终通过数。|

必改清单（已 send_message Lead，均不由C修复）：
1. A：`cargo clippy -p mir-client --all-targets -- -D warnings`，policy:9/12/14 `doc_markdown`；policy:30 `needless_pass_by_value`；真实EXIT101，建议补反引号、expected改引用并更新调用。原始日志 [clippy/stderr](../../../target/round67/c/attempt-1/clippy/stderr)。
2. B：同命令，boundary:377 `field_reassign_with_default`；使用结构初始化表达式替代先default再赋值（不加allow），真实EXIT101。
3. B：manifest.files的 `attempt-1/manifest.json` 自引用SHA/bytes失配；删除自引用并以外置self SHA登记，保全旧清单；其余125条已匹配。
修订会改变测试SHA，须新红绿attempt与清单同步后再独立复核；不得将旧SHA证据冒称新文件已验证。B requirements若干行号已漂移（cue4实际282、候选坐标292等），属非阻断索引偏差，D引用以当前源码实位为准。
偏差披露：B attempt-1 CRLF变异未生效、EXIT0，不计红；有效为attempt-2/3/4，原件保留。C静态脚本首次以GBK解读UTF8失败，明确改UTF8后重跑EXIT0，未触碰A/B。
PARTIAL原编号承接：R49–R60/R60 P-1–P-6；R61 P01–P12；R62①–④；R63 P1–P7；R64①–⑥；R65 G1.2仅值独立/G2未做；R66生产注册/退出/租约/补发/fan-out/全局水位全部保留。
生产known_positions仍无界无淘汰；自动保护集、map epoch/同代ID复用未做；G1.3只测试历史模拟；素材/设备/Live/u64::MAX/服务端回收等历史缺口未清零。
C只写本文及 `target/round67/c/**`，未改实现/旧测试、未commit/push、未跑ci.ps1；本次BLOCK是审计交付完成，不能作为Lead放行结论。
