# R65 尚书省验收：ACCEPT-WITH-PARTIAL（限定 G1）

- 主单 `JJC-20261027-002`；独立验收官 gpt-6.1-sol；建议 **ACCEPT-WITH-PARTIAL，必改 0**；尚待门下确认，不代表本轮最终通过。
- 基准 HEAD=`01b81ca0d12888c706f3856698e5bdaf48a3af83`；功能 `3730e2b` → 文档 `ffed7a9` → 终审/清单/裁决 `01b81ca`；[计划](round65-plan.md)108行、[门下](round65-menxia-review.md)49行及[派发](round65-shangshu-dispatch.md)50行均实读，G2整项移出。
- 验收链：门下 C1–C8/M1–M6 → A/B测试补证 → [C独立核验](../testing/round65/round65-c-evidence.md)60次/13命令、必改0 → Lead唯一门禁 → D文档/Lead补清单 → [T独立终审](../testing/round65/round65-final-audit.md) → [队长口径裁决](round65-lead-ruling.md) → 本验收；不代门下确认。

## 独立重算

|对象|bytes / lines|SHA256（逐字节重算）|结果|
|---|---|---|---|
|README|6380 / 37|`ca55fa90c900d443275f0c92729064b44656db85cb6873a6d409977adcf56331`|匹配，tracked|
|contract|4468 / 37|`2a1fe773e389a7dd4c85b7d84d1174fdcc824ce14b341ef8ec578d007ae0d777`|匹配，tracked|
|C报告|10269 / 74|`8cf5d5848e829411eb6aa5ce193558acebc35ba9d05b65b06a13a2a4384d530a`|匹配，PASS-WITH-PARTIAL|
|gate log|206593 / 3538|`3c9f40fcf0b889bb51291ca9ba88125b80ca894ed0fefc65082e25300c64bfb3`|匹配，untracked|
|gate meta|346 / 11|`25ff19d60f4255c21c05832d38f51de03ff4e5d472398fcbb698da13c5da00c0`|匹配，untracked|
|net.rs|398910 / 9779|`e25640508c6e2495cb95d0a3eb771cfe346346403a8ea94ce0fd9a8e867d8e94`|匹配，tracked|
|app.rs|180794 / 4435|`b180d516e27379a8937072fdb40057518f3feb74c6dfc8a82bf0c95793d442e4`|匹配，tracked|
|B集成测试|28113 / 649|`dfef7383843f9e72c97f6999c058281c0d61ddf5f00b64d878d001c9667333e2`|匹配，tracked|
|T报告|6494 / 39|`cc7167928554b4ded7f4fdfd35717704b5f7bd0bf097a12dc6460c0325be096e`|匹配；保留原BLOCK，唯一M1按裁决与独立判断关闭|

- [重算JSON](../../target/round65/shangshu/attempt-1/recompute.json)、[SHA清单](../../target/round65/shangshu/attempt-1/sha256.txt)、[静态命令/UTC/EXIT](../../target/round65/shangshu/attempt-1/static-commands.json)：manifest九项bytes/lines/SHA/tracked全匹配；原八项哈希未变，三文档ffed7a9及三源码3730e2b声明blob均匹配；JSON解析成功。
- meta十一字段全核：gate_head完整值为上述3730e2b提交，short一致；EXIT0；UTC `2026-10-26T19:49:35Z`–`19:53:18Z`；bytes/SHA/path一致；operator=Lead；GB18030日志末汇总独立读得12 OK/0 FAIL。
- `git diff 3730e2b..HEAD -- crates`与含工作树的`git diff 3730e2b -- crates`均空；功能提交仅net +413/app +129测试段及B新649行，未变生产逻辑；wire协议diff空，历史R61–R64 testing区间diff空。
- tracked按`git ls-files`判定：T报告在01b81ca已跟踪且HEAD blob SHA一致；manifest的`git_ls_files_at_audit:false`及“待Lead纳管”是终审时历史快照，现已过时；“3730e2b/ffed7a9无blob”仍成立，不伪报当前未跟踪。stage.final_audit=BLOCK、验收NOT_YET/board=false均保留为前序阶段，不据此声称已收口。

## 独立快证抽核

|命令|通过/失败/忽略或结果|EXIT / 实测秒|
|---|---|---|
|`cargo test -p mir-client --lib r65_ -- --nocapture`|5 / 0 / 0|0 / 47.344|
|`cargo test -p mir-client --test round65_projectile_lifecycle -- --nocapture`|5 / 0 / 0|0 / 11.016|
|`cargo fmt -p mir-client -- --check`|无格式差异|0 / 1.032|

- 独立CARGO_TARGET_DIR=`target/round65/shangshu/attempt-1/cargo-target`，TEMP/TMP/TMPDIR均为其兄弟`tmp`，实际均解析为仓库下绝对路径；UTC `2026-10-26T20:19:30.545827Z`–`20:20:30.586363Z`，完整环境、命令、原始EXIT、双流日志及SHA见[runs](../../target/round65/shangshu/attempt-1/runs.json)。L2抽核不能以静态检查等价替代；非第二次门禁。
- `py -3 tools/docs/check_links.py`与`git diff --check`实跑EXIT0；新增ignore/allow=0；有效历史ignore属性7，无新增（排除注释/字符串里的字样）；中文诊断非零，不将其写成零。

## M1 独立判断与范围

- **认可验收口径澄清，非证据豁免**：R65计划/门下C6/contract未规定零中文诊断；真实断言比较事件身份、数值、结构与cue资源标识，中文为失败诊断，未发现中文UI/本地化文本相等断言。M1由T明确要求Lead/原授权方澄清，队长已书面给出可执行含义，且保留T原文、源码、红绿与门禁原证据；无功能缺陷被放过。
- T报告自洽：BLOCK严格对应字面中文诊断限制，唯一必改M1，并明确其余事实可接受、未发现功能缺陷与UI中文文案相等断言。接受澄清不篡改T的历史BLOCK，也不称T原结论PASS。
- 裁决历史论据须限缩：R64现有测试中文诊断可证；当前R62指定round62_projectile_wire测试未检出中文字符串，不据此宣称其中文诊断普遍存在。R61终审60行仅记“未新增中文断言”，不足单独证明当年本意；本验收依R65明示范围及实值断言关闭M1，不把历史动机当已证事实。
- [README](../testing/round65/README.md)/[契约](../testing/round65/round65-contract.md)/PARTIAL台账诚实：G1.2仅clone离线值独立，不钉权威前缀；无生产多消费者/并发/全局最低水位；G2未做；total饱和为直写构造，不证恢复或全局唯一序号；backlog/clone/known_positions可增长。
- 服务端回收、容量/LRU、生产SkipFrames/ActionFeed、素材/真实渲染/设备Live音频，R61 P01–P12、R62四项、R63 P1–P7、R64六项按原编号继续PARTIAL；不因本轮单UI补证清零。
- D超时且仅产出README/contract、Lead机械补manifest的身份披露充分；A patch GB18030、B旧v1日志覆盖及C独立红重放均已披露，保留非阻塞证据限制，不扩称原首败完整保全。
- **必改清单：0。** 历史论据与清单阶段文字的边界已在本验收显式标明；当前未发现不可接受功能/证据问题，无需修改源码或重跑唯一门禁。
- 结论仅建议G1本地补证ACCEPT-WITH-PARTIAL；不代表全局水位、容量、多消费者或全链完成，不称最终通过；本官仅写本报告及尚书attempt-1证据，未commit/push、未跑ci.ps1、未执行看板done/state/confirm，最终由门下独立确认。
