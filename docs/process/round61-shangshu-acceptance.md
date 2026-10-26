# R61 尚书验收

verdict：**ACCEPT-WITH-PARTIAL**。仅验收 R61 限定交付，12 项 PARTIAL 全量承接 R62。

## A 门禁原件
- 真读 `target/round61/gate/attempt-1.log`：207593 B，SHA256 `6ff5f68dd112020e20d42ddf8d6f835be7099dab999e1a411d1b9b4cff77ad96` 复算一致。
- 真读 `.meta.txt`，十字段存在且一致：attempt=1、branch=main、head=`ef317b040fdb6ef78a478b1fa247ae3cee775e20`、exit=0、capture=full(stderr+stdout via *>)、log 路径/字节/哈希吻合。
- UTC `2026-10-26T12:06:56Z`–`2026-10-26T12:09:29Z`；时间/head/退出码来自历史 meta，并非本验收重新执行证明。
- 原日志 3442–3453 为十二项 OK，3454 为「全部通过（12 项）」；原始核验输出见 `target/round61/shangshu-acceptance/01-gate.txt`。

## B 提交边界
- HEAD=origin/main=`7baa3a8121cfdf8769edd8edb98aac7fba150a0f`。
- `git show --stat ef317b0`：8 文件 +2216/−232；`1e6cfca`：文档批 11 文件 +458/−8；`7baa3a8`：D4 审计两文件 +152。
- `git diff 1e6cfca 7baa3a8 -- crates/` 为空、EXIT=0；代码提交到文档批的 crates 差异亦为空。
- 从 `docs/testing/round61/commit-tree-hashes.txt` 八个代码 blob 抽 net.rs、ui/projectile.rs、audio.rs，以 `git show ef317b0:<path> | sha256sum` 复算，3/3 MATCH，管道两端均 EXIT=0。
- 详见 `target/round61/shangshu-acceptance/02-commits.txt`。

## C 交付事实
- 原门禁 :845–921 确认 D1 queue 11、D2 sound 8、回归 wire 7、render 16，均 0 failed。
- D4 `docs/testing/round61/final-manifest.json`：9331 B，SHA256 `a9f66a40caa02f7dff20ed3c6f6d17366948c80ac0e3501f91c9535b6009c232`。
- D4 `docs/testing/round61/round61-final-audit.md`：5690 B / 71 行，SHA256 `b86ed44183a13ad0efabf71670e56a1b118f67040650a6ddb8da179700459902`。
- 真读两产物：verdict=PASS-WITH-PARTIAL、必改 0、PARTIAL 12；其 head=1e6cfca 是审计时点，后继提交仅登记审计产物。

## D PARTIAL 诚实性
- 真读 R61 README 与 `docs/client.md` §5.28：禁用词只出现在禁止性表述，未正面宣称「125 全链/渲染完整/真实可听已证/R61 完整」。
- append-only 无回收、完整 NextFrame/SkipFrames、TargetPoint、像素 offset、设备/Live、33 叠加削波、105 sfx、HiddenSpanSource 本 crate 边界均保留；未把离线样本当设备证明。
- 先落盘 `05-links.txt`：`py -3 tools/docs/check_links.py` EXIT=0，1773 相对链接、2 篇 README 引用与市场包编号一致；`git diff --check` EXIT=0。
- 产物大小/哈希与差异检查见 `03-checks.txt`；记录均位于 `target/round61/shangshu-acceptance/`。

## E 看板与 CLI
- 父单 `JJC-20261026-008`；D1=`sub-7e992ec6`、D2=`sub-c7281e94`、D3=`sub-d0668f24`、D4=`sub-840540ca`，coordinator=`sub-16fb7ccb`（子单前缀均为父单 ID）。
- 实际 `show` 抽查 D1、D4，均 Done、委派结果已提交，EXIT=0；父单读取 EXIT=0，初始 Doing，todo #3 仍 in-progress。
- CLI 五条结果：memo EXIT=0；todo #1 completed EXIT=0；done EXIT=3；validate EXIT=0（181 条任务 schema v3.1）；最终 show EXIT=0、仍 Doing。
- done 原文：`❌ JJC-20261026-008 todos 未完成（2/3），禁止直接收口`；todo #3「独立门下审议」仍 in-progress，流程未闭合，未绕过。完整原始 stdout/退出码见 `04-cli.log`。

## F R62 承接清单
1. P01：append-only 确认、安全裁剪、回收契约与 snapshot clone/历史容量增长。
2. P02：完整状态式 NextFrame/SkipFrames。
3. P03：完整 TargetPoint 语义。
4. P04：像素 offset 增量运动。
5. P05：真实设备可听与 Live 验证。
6. P06：33 cue peak=22.844788 的真实输出削波风险。
7. P07：105 sfx 逐一验证（当前实测 M34-1/2）。
8. P08：HiddenSpanSource 仅本 crate decoder_for，未作 rodio 全局修复。
9. P09：CWD 与素材来源解析边界。
10. P10：无会话代号，非空重建超过旧游标仍有歧义。
11. P11：缺素材 early-return，SKIP 必须记 PARTIAL，runner 保留失败处理。
12. P12：R60 P-1..P-6 不自动清零，渲染/效果完整性仍待验证。

本验收未运行环境测试、ci.ps1，未改代码、历史证据或门禁日志，未 commit/push；未闭合看板项以 CLI 实际结果为准。
