# R67 刑部终审证据报告

审计时间 UTC：2026-10-27T00:05:15Z–2026-10-27T00:07:37Z（复核命令窗口）。
审计 HEAD：909a55c；功能/门禁 HEAD：78a8613；终审官：gpt-6.1-sol。
**verdict：PASS-WITH-PARTIAL；必改 0。此为本次审计结论，非整轮验收。**

| 项目 | 判定 | 本轮证据 |
|---|---|---|
| 复跑 | PASS | R67 2+2、R65 5+R66 2，均 failed0/ignored0；fmt、clippy、links EXIT0；首次缺临时目录编译EXIT101如实保留。 |
| 门禁 | PASS | attempt-1 meta EXIT0/12 OK/0 FAIL；两原件bytes/SHA完全一致；git diff 78a8613 --stat -- crates为空。 |
| 清单 | PASS | node逐项statSync/SHA256重算14项，mismatch=[]、self_reference=false；3项tracked crates的78a8613 git_blob匹配；外置摘要匹配。 |
| 范围 | PASS | 52d95b1..909a55c仅3个新增测试文件+723行；无新增ignore/allow；写报告前status仅两历史freeze JSON。 |
| 真红披露 | PASS | 读原件核A m4左0/右1、B三红旧缓存/刷新/保护断言；README披露七份rev2红、rev1被替代及未生效CRLF变异不计红；本次未重跑变异。 |
| 文档诚实性 | PARTIAL | README/contract明确测试提案、非wire夹具、cue非可听、125非全链；清单仍pre-audit且docs tracked=false，README历史未跟踪措辞待Lead post-audit刷新。 |

## 独立复跑
- 首次R67于00:05:15Z：cargo EXIT101，LNK1104临时文件不可打开；tail EXIT0，不冒称测试红，未产生测试计数。
- 创建target/round67/t2/tmp后00:05:28Z重试EXIT0（尾部policy 2/0）；00:07:06Z补获完整计数：boundary2+policy2，EXIT0，均failed0/ignored0。
- 00:06:29Z回归：R65 lifecycle5+R66 consumer2，EXIT0，failed0/ignored0。共11个互异测试；含R67重复确认共15次通过执行，不称15个互异测试。
- 00:06:32Z fmt EXIT0；00:06:33Z clippy EXIT0；00:06:34Z links EXIT0、2115链接；链接控制台中文乱码已披露，不补造原文。
- 详细命令、UTC、真实工具/管道退出码及摘要：target/round67/t2/attempt-1/runs.json；仅两次后续R67成功执行，未再重试。

## 门禁与清单
- log：207862B；SHA256 `056255b32147ac506f52c42522b73d7f68b6641231b581d77f6a30e4ca54bc82`。
- meta：285B；SHA256 `75d0bf415cffce0869f257a08fe6547942f2da333fe41f13d6ead6307fe655aa`。
- 门禁UTC 2026-10-26T23:43:26Z–23:47:08Z，operator=lead，post_gate_crates_diff_lines=0；本次不重跑唯一门禁。
- 清单14/14匹配，mismatch 0、自引用0；文件SHA256 `c262ebb84b7e3e0ec693c0af20cbeab3331b7d5ee5dac0a3afa8b81f1a7419ed` 与manifest.sha256一致；未改二者。

## PARTIAL 承接与范围
生产known_positions无界无淘汰；多消费者与生产注册/退出/租约/补发/fan-out、全局最低水位、125无点坐标、服务端回收继续PARTIAL；自动保护集、map epoch/同代ID复用及素材/设备/Live等历史项不清零。
crates只3新测试+723，无ignore/allow新增，生产/wire/server零变更；未修改实现、测试、README或清单，未commit/push。
本报告仅陈述本轮证据；清单 post-audit 刷新与提交由 Lead 完成，本报告不宣称 R67 通过。
