# 第74轮尚书省验收

裁决：**BLOCK（S-K 看板收口阻断 1；功能必改 0、技术证据必改 0）**；主单 `JJC-20261028-001`；被审 HEAD=origin/main=`c3542c5fea27184d039379a94396928f4416b7a3`。
核验窗口：UTC 2026-10-27T18:34:37Z–18:36:55Z；验收官 gpt-6.1-sol。逐命令/UTC/原始 EXIT/日志见 [runs](../../target/round74/shangshu/attempt-1/runs.json)，独算见 [checks](../../target/round74/shangshu/attempt-1/checks.json)。

|必做项|结果|独立证据与结论（命令完整参数及 UTC 见 runs）|
|---|---|---|
|1 父单与 A/B/C/D|FAIL/S-K|五次 `node …/edict.mjs show <ID> --json` 均 EXIT0；父单 Doing、todos=[]、ready_to_close=false；A/B/C Done 且 delegation_result 非空；D Doing 且字段缺失。18:36:55Z 再 show D，仍同。|
|2 提交链/生产范围|PASS|`git log --oneline c80ef2a..c3542c5` 链吻合；`git diff c222fe6..c3542c5 -- crates tools` 与 `--stat -- crates` 均 EXIT0/空；`git show c1d0c76 --stat --patch` 仅 spellfx:309 一行注释，零可执行差异。|
|2 冻结测试|PASS|1375行/49,849B；工作树与 delivery_head blob SHA256=`a2b329ce88a3640ad3609c804a2e589a4177a7de8c0e1e6df19c6d5c40013cc1`；新增 ignore/allow=0。|
|3 唯一门禁原字节|PASS|log 213,918B/e2b1c25a…；meta 573B/4e622efe…；清单 bytes/SHA 全匹配。HEAD c222fe6、EXIT0、12 OK/0 FAIL、17:49:40–17:54:13Z、operator lead 逐字段 mismatch=0（清单 Lead 仅大小写归一）。|
|4 清单独立复算|PASS|34 artifacts 全部 bytes/SHA256（tracked 另核 blob ID）吻合，mismatch=0；delivery_head=ec49369；目录按排序 path:bytes LF inventory 32项/64,773B；清单15,045B/6ccfae09…、外置摘要匹配，无自引用。|
|5 R74 抽证|PASS|`cargo test -p mir-client --features mir-render/legacy --test round74_normal_blend_lock`；18:34:37–18:35:23Z，EXIT0，5/0/0；实测45.297秒。|
|5 R73 抽证|PASS|同 features、`--test round73_thunderbolt_hit_mapping`；18:35:23–25Z，EXIT0，9/0/0；2.877秒。|
|5 R72 抽证|PASS|同 features、`--test round72_poison_cloud_mapping`；18:35:25–30Z，EXIT0，9/0/0；4.691秒。|
|5 spellfx 抽证|PASS|同 features、`--lib ui::spellfx`；18:35:30–41Z，EXIT0，4/0/0（328 filtered）；10.818秒。|
|5 fmt/clippy|PASS|`cargo fmt --all -- --check` 18:35:41–46Z EXIT0/4.727秒；`cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings` 18:35:46–18:36:22Z EXIT0/36.456秒。|
|6 终审历史保全|PASS|T 原文 BLOCK/E1，C 原文43行/7353B/ab7f9904…；两份 `git log --oneline -- path` 各仅首次提交。C 原 M3 明确为 x/y 各 add 两次，由独立附录纠正，未删改原结论。|
|6 C-M3/T2 补证|PASS|精确 patch 73831540…，实体 pre/restored f8dff727…；T2 patch 同字节。C十份/T2六份日志原字节全部吻合（mismatch=0）；5绿→build0→全套1/4/0 EXIT101 G1:643 (8,2)→G2 0/1/0 EXIT101 :797 (6,9)→恢复5绿；T2 原文 E1 CLOSED/剩余必改0。|
|7 文档诚实/Crystal|PASS/PARTIAL|README 无正面整轮通过/验收完成；disclosures 保留 T BLOCK/E1/锚点纠偏。实读 Effect:20/24/81–96、MLibrary:692–711/748–801、MirGraphics/DXManager:353–389，NORMAL SourceAlpha/One、INVLIGHT BlendFactor/InverseSourceColor，rate0.8 未被推定实现；contract/client/roadmap 旧 pre-audit 快照保留为非阻塞披露。|
|8 卫生|PASS|起始及报告写入前仅两 freeze 未跟踪；`git diff --check` 两次 EXIT0；`git diff c222fe6..HEAD -- '*.rs'` 空，无新增 ignore/allow；两 freeze 44,844,165B/70845aef…、45,219,533B/c7d896e7…，与 C 历史 bytes/SHA 一致。|
|9 CLI|BLOCK/校验PASS|因 S-K 不执行 `done JJC-20261028-001 --as shangshu`，无原始 EXIT（未调用）；18:36:55Z `node …/edict.mjs validate --as shangshu` 原始 EXIT0，246 条任务符合 schema v3.1。|

四子单全号：A=`JJC-20261028-001-sub-39860302`；B=`JJC-20261028-001-sub-82b02278`；C=`JJC-20261028-001-sub-c55440fc`；D=`JJC-20261028-001-sub-05088f98`。
唯一必改 S-K：由真实礼部承接者提交 D 的有效 delegation_result 并达到 Done，再独立复核父子单；本官不代签、不绕过 CLI、不宣称已 ACCEPT。
抽证隔离：CARGO_TARGET_DIR=`target/round74/shangshu/attempt-1/cargo-target`，TEMP=TMP=TMPDIR=`target/round74/shangshu/attempt-1/tmp`（均解析为绝对路径），CARGO_PROFILE_DEV_DEBUG=0；测试均带 legacy；L2 抽证按任务指定不可降档，静态原字节核验 L0。
PARTIAL 原号：GAP-1/2、R70-C2、GAP-5/R70-C1（双记）、R70-C3、R71时钟/对象、R72 B plan-M4、R73-T1、GAP-3/4/6；R71-C7永久PARTIAL、E1仅补救CLOSED；R69条件5、R68/R49–R70/R61–R67照承。
LIMIT：技术核验仅覆盖普通 NORMAL/rate1 CPU Add 跨 Magic/Magic2 测试锁 + rate/Mode 差异合同 + spellfx:309 注释切片；完整 DrawBlend/BlendMode/DXManager、rate0.8/INVLIGHT设备状态、严格NextMotion/对象寿命、其他技能、真实素材/GPU/设备视觉/全UI/跨后端/Live音频继续PARTIAL；不升级历史结论。
写入仅本报告与授权 shangshu/attempt-1 证据目录；未改实现/测试/清单/其他文档，未commit/push，未运行 tools/dev/ci.ps1；早期空 justification 的 write 参数校验拒绝、零写入，纠正后才创建脚本。
