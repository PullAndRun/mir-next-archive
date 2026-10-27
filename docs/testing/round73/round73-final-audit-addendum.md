# R73 御史台 T2 复审附录

- **最终 verdict：PASS-WITH-PARTIAL；T-D1 CLOSED；必改 0；不一致 0。**
- 提交链：delivery_head `55f61c424a1ba731903135fc3c837a811de2bd20` → 原清单/origin/main `34d5e17a46ff9c001c63f67e7d1ff275c1eb40e9` → 修复/本次 HEAD `95e780717cf5ddfd7c78a4977f9fd61c33f01a6d`。
- 修复提交仅三处：final-manifest.json、manifest.sha256、round73-final-audit.md；指定 crates/tools/client/roadmap 路径 diff 为空，无实现变更。
- 清单 JSON 解析成功；33/33 artifact 的 bytes/SHA256 原字节复算一致：3 项 git_blob 按各 revision 读取，30 项 worktree 按工作树读取；13 项 tracked 的 Git OID/head_sha256 均与 delivery_head 原始 blob 一致；无自引用。
- `tools/assets/analyze_usage.py` 实值：source=`git_blob`；revision=`55f61c424a1ba731903135fc3c837a811de2bd20`；bytes=`67171`；sha256=head_sha256=`e2d9f327f43b52e96a203a75ee3fb8f730db990e8e28ef40460162e98f9a40c1`；blob_oid=git_blob=`b1da9074c17e26bfc4264289e2df381d8c952477`。
- 外置 manifest.sha256 与清单原字节一致：`58fe7972af3162407c4768617ca56625266a282af31ed05f2ac191530a108b34`；清单同时与修复提交 blob 全等。
- 首轮 round73-final-audit.md 保持 31 行/6,218B，SHA256=`1e23f41865582907ea0ad39d77c701d5d0b473ba9e689d5fb1bdb8ce4c80022a`，与本次提交 blob 全等；原 BLOCK 为历史结论，由本附录复审关闭 T-D1。
- 门禁 attempt-3 原日志 213,076B，SHA256=`676b9d4f36a9a5ef59c47365eac890d42463d195c80532a6d12944ef49beddac`，与清单 gate 段一致；本次仅只读复算。
- freeze before：44,844,165B / `70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de`；after：45,219,533B / `c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32`；两份仍未跟踪，原字节摘要未变。
- 范围：仅 149 雷电命中切片；首轮全部 PARTIAL 原号继续承接，含 GAP-5/R70-C1、R70-C3、R71时钟/对象、R73-T1、GAP-1/2/R70-C2、GAP-3/4/6、R72 B plan-M4/legacy lint、R71-C7/E1/R69条件5、R68/R49–R70/R61–R67；不扩大为整轮、全部雷电、严格 Crystal 等价或设备视觉签核。
- 复算命令：`py -3 target/round73/t2/attempt-1/verify.py`（EXIT0）；完整逐条 Git 命令、UTC、退出码、日志摘要及结果见同目录 runs.json、checks.json、logs/；本次仅写该证据目录和本附录，未改交付物/清单，未 commit/push，未运行 ci.ps1。
