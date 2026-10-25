# R51-A 礼部文档批取证与交付

- 子单：`JJC-20261025-003-sub-82bdab8e`；执行角色 `libu / gpt-6.1-sol`。
- 取证日期：2026-10-25，时区 `Asia/Shanghai`；cwd：`C:\Users\diana\Documents\github\mir-next`。
- 基线：`HEAD = origin/main = 716db4292ad6d475e92bf8b6cbd4f230f307684f`；本批交付工作区差异，由 Lead 提交。
- 范围：[商城](../world/gameshop.md)、[客户端](../client.md)及本记录；无 crates 改动，无 cargo/ci 或 git 写操作。
- 原始日志按执行者直接指令落 `target/round51/r51-a/`；此要求优先于[派发契约](round51-shangshu-dispatch.md)第22行“不额外写target”的一般约束。

## 当前陈述与源证据

| 当前陈述 | 证据与边界 |
|---|---|
| R50 已完成，整体 PASS-WITH-PARTIAL；P01/P09/P11 闭环 | [R50 验收](round50-shangshu-acceptance.md)、[T2 复审](../testing/round50/round50-final-audit.md)；保留 P02–P08/P10 与 P12 历史取证边界 |
| `S.GameShopInfo` Index250，先 Item（含 ItemInfo）再 StockLevel；不压缩 | [手写协议定义](../../crates/mir-protocol/src/manual/server.rs#L1089)；商品字段沿用商城 §2 |
| `S.GameShopStock` Index251，先 GIndex 再 StockLevel；不压缩 | [生成协议定义](../../crates/mir-protocol/src/packets/server.rs#L5961)，只读核对 |
| GIndex 为商品 Index；登录目录在 UserInformation/物品模板之后；限量商品购买事务成功才刷新库存 | [R50-B 证据](../../target/round50/r50-b/README.md)、商城 §7.3.1–7.3.3；保留与 Crystal Info.Index 的既有差异 |
| 独立面板 K、分类、搜索、双币；GIndex 重复目录幂等 | [R50-C 面板证据](../../target/round50/r50-c/c2-gameshop-panel.log)、商城 §7.3.4；不把硬编码 K 说成可重绑 |
| 原始 stock=0 显示 ∞；限量剩余≤0 售罄/禁购；初始目录与刷新同一口径 | [R50-C2 红绿证据](../../target/round50/r50-c2/README.md#L71)：lib 15/15、面板 13/13、exit0；T2 确认 P11 闭环；非法负总量实际仍按 <=0 防御 |

## 本批实际验证

- 命令：`py -3 tools/docs/check_links.py`、`git diff --check`、`git diff --stat`、`git diff --numstat`、`git diff --name-only`、`git rev-parse HEAD`、`git rev-parse origin/main`、`git status --porcelain`，均在上述 cwd 实际执行。
- 最新原始 exit、每命令起止时间、日志路径/字节/SHA256，见 [verification.json](../../target/round51/r51-a/final-pass/verification.json)；链接检查仅证明链接/包号检查，不替代语义取证。
- 最终链接检查 926/926、2 篇 README 引用、exit0；差异检查 exit0。原始 `check-links.log` 保留 Windows GBK 字节。
- 产品文档差异：客户端 +13/-2；商城 +25/-9；合计 +38/-11。本记录为新增允许路径；最终范围另见原始 stat/path 日志。
- 活动商城状态中过期断言匹配数 0，`status-scan.log` 保留正则与上下文；旧提交仅作为历史保留。
- 客户端 R49 怪物章节仍有两条“R49 当前 gate=PARTIAL，待队长最终全量门禁”的当轮记录（当前行1415/1883）；按仅更新 P12 活动状态、保护历史要求保留，不能当作 R51/P12 当前结论。其余“待”命中为待写队列、待投递、等待回包等行为描述。
- 原有未跟踪的方案/评审/派发及两个 monster-native 冻结 JSON 均未修改/删除/加入提交。
- 新增本记录后的首次检查在 08:06:57 返回 exit1：926 条中 1 断链，原因是新增引用指向不存在的 R50-C README；已改为存在的面板测试日志。失败原始日志/exit/哈希保留在 `target/round51/r51-a/final/`，修后另用 `final-pass/`，未覆盖失败证据。
- 采集器最初遇到 GBK 控制台编码异常及 Git 行尾警告污染路径断言；修正采集器后所有实际文档验收命令通过。这两个采集器失败不是项目验收失败，原始工具错误记录单独保留。

## 七技能归属与 PARTIAL

| 技能 | 本文档批结论 |
|---|---|
| Blink | B 批目标，本批未实现/未测试，不宣称完成 |
| ShoulderDash | B 批目标，本批未实现/未测试，不宣称完成 |
| StormEscape | B 批目标，本批未实现/未测试，不宣称完成 |
| FrostCrunch | B 批目标，本批未实现/未测试，不宣称完成 |
| ThunderStorm | B 批目标，本批未实现/未测试，不宣称完成 |
| PoisonCloud | 依派发契约固定 PARTIAL：必需耗材排除，不改为免耗施法 |
| TrapHexagon | 依派发契约固定 PARTIAL：必需耗材排除，不改为免耗施法 |

本批无新增功能完成宣称；文档当前口径已修正，历史 PARTIAL/Crystal 差异继续保留。Lead 可核对后独立提交本批三份文档；后续功能批及全量/终审由对应责任人执行。
