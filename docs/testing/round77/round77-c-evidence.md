# R77-C 刑部独立核验

2026-10-28 UTC；子单 JJC-20261028-004-sub-31c36f9a；HEAD `0bbc322a7901a8c85bfbea157ef1471b50de07e7`；只读核验，25 分钟内交付。
**Verdict：FAIL；必改 2 项，退 A 原 owner，B 对新 SHA 重验；当前不得交 Lead 提交/全量门禁。**
命令、UTC 起止、EXIT、原 stdout/SHA 见 [runs](../../../target/round77/c/attempt-1/runs.json)、[补录](../../../target/round77/c/attempt-1/supplement-runs.json)、[回归](../../../target/round77/c/attempt-1/regression-runs.json)；细项见 [findings](../../../target/round77/c/attempt-1/findings.json)。

|必核项|裁定|独立证据与界限|
|---|---|---|
|1 范围/卫生|PARTIAL（全目录未穷尽）|初始仅 A 两新文件与两历史 freeze；tracked diff 空；03:34:42 UTC 时142 个选定素材/源码/freeze/ignore 文件前后 SHA 相同、无新增 ignore/allow；收尾检测 A 原件并发变化，见 [并发记录](../../../target/round77/c/attempt-1/concurrent-source-change.json)，本裁定仅限旧 SHA；全目录 SHA 首遍超8分钟未完成，为25分钟预算终止，见 [范围说明](../../../target/round77/c/attempt-1/full-assets-partial.json)。|
|2 A 红绿|PASS|`git -c core.autocrlf=false archive 0bbc322` 物理副本、A 两文件同 SHA、素材只读复制；独立 target/TEMP/TMP/TMPDIR、`CARGO_PROFILE_DEV_DEBUG=0`；指定 cargo 七绿0忽略 EXIT0；bundled Python `--facts/--assert/--selftest` 均 EXIT0。|
|2 A 历史红抽核|PASS|原 log：EXIT101、6 passed/1 failed/0 ignored，唯一 G6 :1303、Some(1)/Some(0)；红绿同测试 SHA 证明与实测一致；见 [红核对](../../../target/round77/c/attempt-1/a-red-audit.json)。|
|3 六变异|PASS（B 实际口径）|六独立物理副本、各自空 target/TEMP；各 pre七绿0→变异 no-run0→唯一指定G红101→恢复原字节SHA→七绿0，零忽略；见 [isolated-summary](../../../target/round77/c/attempt-1/mutations/isolated-summary.json)。|
|4 新 oracle|PASS 数值；PARTIAL 摘要|新写 C 二进制解析器，不导入/执行 A/B oracle；三源 **323/323 agree、0 mismatch、EXIT0**；A 82 项 literal EXPECTED 可复算；摘要说明见下。|
|5 G1|PARTIAL→FAIL F2|缺索引/越界事实成立；将解压 RGBA 大小与压缩文件余量比较后声称全库物理不存在，推理不成立。|
|5 G2/G3/G4|PASS|2025 八帧 format5/zlib、连续边界、RGB565 行对齐尺寸一致但非 RGBA；UNVERIFIED_SEMANTICS；Lib 双 SHA/count/gzip RGBA/邻居空帧一致，AWAITING_BINDING。|
|5 G5|FAIL F1|矩阵 current 错写 TrapHexagon 1390/10/100ms/4000ms/Add/offset_once；实际 Trap 为 Magic/0起/10/60ms/700ms/居中 Over；100→120 矩阵变异仍七绿 EXIT0。|
|5 G6/G7|PASS|禁自动 READY，合成 binding 仅证明算法；真实绑定仍须独立审议；R76 18 帧/R59 116 帧冻结文本与生产 tracked 零改。|
|5 门下 M1|PARTIAL 未关闭|A scope/B facts 已披露 Lib 待认证候选承接 R78；本轮无 D，仍须 D 披露后 C 只读复核，不得冒充 M1 已关闭。|
|6 诚实/范围|FAIL F2；其余 PASS|无认证/READY/生产已修/Crystal等价扩称；2025 与 Lib 语义已区分；物理绝对不存在扩称须纠正；B M5 偏差披露诚实但不能替代 G5。|
|7 静态|PASS|`py -3 tools/docs/check_links.py`（`PYTHONUTF8=1`）EXIT0；`git diff --check` EXIT0；最终链接计数见 [原件](../../../target/round77/c/attempt-1/logs/c-final-links.log)。|

独立有效复跑：baseline 7；六变异各四阶段（12 套绿/84 项、6 套唯一断言红）；G5 错矩阵额外七绿并恢复七绿；历史回归 **R76 8/R72 9/R59 6，23/23 EXIT0**，见 [回归原件](../../../target/round77/c/attempt-1/logs/c-regressions.log)。

|变异→目标|独立红实值（left/right）|首败测试行|编译/红/恢复绿|
|---|---|---|---|
|M1→G1|1460 / 1461|656|0/101/0|
|M2→G2|Some(40132) / Some(40133)|808|0/101/0|
|M3→G3|UNVERIFIED_SEMANTICS / READY|938|0/101/0|
|M4→G4|Some(2790) / Some(2791)|985|0/101/0|
|M5→G7|R76 `8b680973a30193113c65e6cc936cccd6cfc8ac33941dbff43d4867419f3562d0` / 末字节改 `d1`|1385|0/101/0|
|M6→G6|Some(0) / Some(2)|1271|0/101/0|

**F1 必改（A）**：更正 [测试矩阵:315](../../../target/round77/c/attempt-1/copies/baseline/crates/mir-client/tests/round77_trap_asset_admission_contract.rs#L315)；逐字段可失败断言，补 dispatch M5（100→120 或8→7）真实 G5 红。源锚为 [Rust duration:40](../../../crates/mir-client/src/ui/spellfx.rs#L40)、[通用路径:418](../../../crates/mir-client/src/ui/spellfx.rs#L418)、[Crystal Trap:132–138](../../../github/Crystal/Client/MirObjects/SpellObject.cs#L132)；Hexagon 应单独标明。
**F2 必改（A，B 同步说明）**：[工具:826](../../../target/round77/c/attempt-1/copies/baseline/tools/testing/round77_asset_admission.py#L826) 的 2,637,986B 未索引空间 < 4,809,024B RGBA 不能证明压缩字节不存在；同一 Lib 八帧 gzip 载荷仅 **114,603B**（加8×17B头共114,739B）。应保留缺索引 NOT_READY，将“2025 绝对地址 13,316,944 > 默认 EOF 10,290,606”限定为对应地址不可读，移除未经穷尽证明的全库物理不存在/空间不可能断语；不得更改生产。
[独立 oracle 结果](../../../target/round77/c/attempt-1/oracle/result.json)：default WZX 5,968B/declared1460/slots1480，2360 需索引字节9488而越界；WZL 尾部余量0、未索引2,637,986B；2025 八帧 RGB565 行对齐，Lib 八帧 gzip=`w*h*4`，双候选 v2/count2790/SHA `f449aa4c96632f13454d7746ab47678c06175be02e69a6dc8fdfd036755fc0d2`；数值 agree 不等于接受 F2 推论。
A digest `eecc21140c7ab60a0abb1a77c35db629bd89e222b38100dae3d8724d59bcc681`：**PARTIAL 非阻断说明项**；交付物未给规范序列化/生成器，12 种常用序列化均不匹配；原件字节 SHA 与82逐项值可复算；B 已如实披露，不伪称摘要重现，见 [digest-probe](../../../target/round77/c/attempt-1/oracle/result.json#L2663)。
B M5 裁定：**披露充分，执行口径 PARTIAL，G5 缺口阻断 F1**；B 原 [README:31](../../../target/round77/b/attempt-2/README.md#L31)/[manifest:171](../../../target/round77/b/attempt-2/manifest.json#L171) 已明示 G7 代替 dispatch G5；C 实做 [100→120 负控](../../../target/round77/c/attempt-1/logs/g5-isolated-matrix-120.log) 仍7绿，故不能将该偏差关闭。
PARTIAL 原号继续承接：门下 M1（Lib→R78/D待披露）及 [计划:130–144](../../process/round77-plan.md#L130)/[门下:33](../../process/round77-menxia-review.md#L33) 所列历史项均未擅自关闭；R78 才审真实绑定与18→26 supersede。
证据诚实性：初轮共享 C target 的 M6 缓存假绿整组弃用，保全 [旧summary](../../../target/round77/c/attempt-1/mutations/summary.json)，以上只采各独立 target 重做；初次 oracle B-array 适配错误/Windows GBK 链接输出错误均保全，分别修 C adapter/设UTF8后 EXIT0；不计环境错误为红。
范围声明：仅写本报告与 [C attempt-1](../../../target/round77/c/attempt-1/)；A 两文件实测 SHA 为 `514b13d8341e90550fe554f382681d16bf69206c0817d5099a175b9ebd5fad14` / `4e1735b513abeb8c77b62e7f6cfafa6cc9c44d1e109e0ee329c83996bbdaf907`；未写实现/测试/工具/素材原件，两 freeze 实测未改；142 项之外全目录 SHA 未穷尽；无 commit/push/全量 ci。
退回要求：A/B 用新增 attempt 与新 SHA 交付，保留本轮原件；先修 F1/F2，B 重验后再交 C，必改清零且达到 PASS-WITH-PARTIAL 才可交 Lead。
