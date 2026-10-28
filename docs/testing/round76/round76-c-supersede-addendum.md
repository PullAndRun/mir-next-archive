# R76-C3 R59 supersede 追加核验

- **R59 supersede CLOSED；整体 C 仍 PASS-WITH-PARTIAL；revision=`41425b1d2bbe497d15e4e173f65ef7575344279f`。**
- UTC 2026-10-28；命令、起止 UTC、EXIT、实测耗时逐条见 `target/round76/c3/attempt-1/runs.json` 与 `logs/extra-check.json`。

| 必做项 | 结论 | 独立结果 |
|---|---|---|
| 1 字节保全 | PASS | 旧19,447B/585行/`2cfff50ce46a967e742b29ead5a7e794655287fa279b316335b54e290d089f34`；新20,596B/600行/`849770b254ccadeef5e8fce05a5ffd2b01ab67e81e209bbab7457d52faf5fe2a`；旧git blob=A副本、新blob=主树，与B一致 |
| 2 diff范围 | PARTIAL（口径勘误） | `git diff 9bf6ec6 41425b1`实际2文件：R59+Lead裁定文档；`41425b1^..41425b1`才仅R59、29+/14−；其它测试零改动 |
| 3 指纹 | PASS | 116=`e642330a568402b21528afb04459108f3dc596f7e37ad5c276670ef342940eff`；排除1390..1399的106=`e465026a50ee6f86d6762e3959386fa0132ffabf63da9c92afeebdf3d52a1e98` |
| 4 独立复跑 | PASS | R59 6/0/0、R76 8/0/0、R72 9/0/0；fmt0、官方 `cargo clippy --workspace --all-targets -- -D warnings`0；交叉回归带 `--features mir-render/legacy -- --test-threads=1` |
| 5 门禁 | PASS | attempt-2 215,013B/`85bdd3e6b5b725dd52d32ec9d0e43f038318f09c2a2208e7d674cc421afb1027`，末段12/12 OK；meta802B/`71c9ca8caad3b18512bd407ce4eac60137835eea8d1b7685282803069007cc67`字段一致，00:28:12–00:31:25Z/EXIT0 |
| 6/7 报告与证据 | PASS | 本报告≤18行；证据根 `target/round76/c3/attempt-1/`，自写recompute.py、recompute.log、diff-check.txt、runs.json、logs；edict progress成功，结果回报Lead |
- 语义：去除授权注释/常量/追加帧行/106→116措辞后可执行正文逐字节一致；SAMPLE_FRAMES、六测试名、断言结构、WZL哈希、index解析、指纹算法未变，无新增ignore/allow。
- 隔离：`git -c core.autocrlf=false archive 41425b1`，必要素材物理复制，120文件SHA一致/nlink=1；独立target/TMP/TEMP/TMPDIR，`CARGO_PROFILE_DEV_DEBUG=0`；L2/L3指定复跑不可用静态检查等价替代。
- 披露：attempt-1原日志恰3个R59失败，116/106冲突与裁定5（35行/`ec92c6c7…`）充分一致；原失败未由后绿抹除。R59默认diff实际9 hunks（背景“6处”为分组口径）。
- 执行偏差：系统Python启动49、范围断言1、GBK打印1及无legacy的R76/R72各101均保留；改用bundled Python/UTF8及正确feature后完成，未冒充首次全绿。残余PARTIAL承接C/C2历史、A侧录、B日志永久损失、Crystal时钟/寿命、DrawBlend/src×0.8、700ms和本地素材。
- 范围：只新增本报告与C3指定证据目录；未改交付物/既有证据，未commit/push，未跑ci.ps1；主树既有两份freeze未跟踪文件保留。
