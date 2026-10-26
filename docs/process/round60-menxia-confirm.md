# R60 门下省确认

- 任务：JJC-20261026-007；独立确认者：menxia（gpt-6.1-sol）。
- **结论：PASS-WITH-PARTIAL 收口成立。** 尚书 ACCEPT-WITH-PARTIAL 与刑部终审口径一致。
- 受审 HEAD：220c5f6；代码 bb53321、文档 e053905；仅新建本文与自身证据目录，未复跑门禁。

## 六项抽核

1. 四件字节/SHA 实测一致：acceptance 5034 B，4e0f490d2c5d48a247bad632fce59afabe1f6352e4382c908a74387a6ebc88c9；manifest 12859 B，7f992e0625e9f0c0fbbb28cd1090bc3af123f1dd3f385bacc10dc08028c262c3；audit 13844 B，f17b258cc0b401afead6616f62b7fededc5d124c2dfbcac2b1b844ae5840069f；meta 134 B，935c28202dbb0a0592441ed87c2aac0a58290b6af498e9c467cdea83eaf5d10c。
2. 门禁 SHA 为 193b6fe4f8a0a46397a0026c2e8a2d66f0e041c9fa33f0ad6ca3a17d1f3678dd；末三行确认“全部通过（12 项）”；205317 B、exit 0、head=bb53321 由已核 meta/manifest/冻结清单交叉绑定，未重跑。
3. git log 确认三提交顺序；bb53321..HEAD 的 crates 差异为空。status 为两份历史冻结 JSON 加本轮尚书验收文档（未跟踪），属验收后时点差异；本文创建后另增本文。
4. 树清单抽两条复算一致：app.rs 153666 B / e16740ef6e131b06afb1e597bd81d2b739bbd70a6f7f90ca83eddeebde4ffe9e；ui/projectile.rs 48100 B / 95087857bac7292ff4916e674d2d8d4c2d67462b462d5a1ed074b720a10d5c21。
5. 独立读 acceptance A–G 与结论、manifest partials/must_fix/non_claims、终审全文：P-1..P-6、MF-1/MF-2 及否定主张一致；M-4 数量完整性不扩称。
6. 三件禁词命中均为否定语境或禁称扫描引用；未见“125 全链通过”“R60 完整”“音频完整”“真实可听已证”的正面断言。门禁 12 项不替代脚本 11 项，更不替代可听性证据。

## PARTIAL / 必改与 R61 承接

| 项 | 保留事实 | R61 承接 |
|---|---|---|
| P-1 | 可听性未证、无 Live 窗口集成测试 | 真机手听留证与可注入帧集成测试 |
| P-2 | from_repo_root 依赖 CWD，缺 M34 为 SKIP 非 PASS | 根路径收口 |
| P-3 | ring 16 仍丢最旧历史 | 容量/队列评估 |
| P-4 | 同文件 100ms 频控与 Crystal 有差异 | 并发上限评估 |
| P-5 | R59 community Crystal wav 导入，素材未入库 | 脚本化导入与来源复现 |
| P-6 | 首跑 TEMP 不可写失败保留；target/tmp 后 11/11 | 固化环境前置 |
| MF-1 | low、非阻断；app.rs:249 注释字面 ignore 误报 | 改写注释措辞 |
| MF-2 | low、非阻断；client.md:1304 错引 P-6 | 改为 P-4/R-5 |

- 禁称全链/完整/真实可听已证；不得把上述 PARTIAL 写成 PASS。
- 本文先于确认 CLI 落盘；状态流转及最终结果以 target/round60/menxia-confirm/01-cli.log 为准，抽核与 PendingConfirm JSON 见同目录 02-checks.log。
