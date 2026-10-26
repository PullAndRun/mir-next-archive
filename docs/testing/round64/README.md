# R64 本地确认水位与安全前缀裁剪

阶段：**门禁通过；独立复跑绿；终审待刑部/尚书/门下，未 PASS**。
子单 `JJC-20261027-001-sub-0286eab0`（R64-D，礼部）；功能提交 `063c6f6`，5 文件 +905/−34，已提交并推送；本次文档不提交。

## 范围、裁决与非目标

限定交付 app.rs 单消费者的本地确认水位和事件表安全前缀裁剪；不扩展协议或会话循环。
门下四裁决：主批准入；no-op 桩红＝修改批准（反空泛绿）；known_positions 仅契约 PARTIAL；net.rs 限 +1 变体/ack 分支/无包分支/测试，未改会话循环。
Crystal：`github/Crystal/Client/MirScenes/GameScene.cs:4687-4715`、`github/Crystal/Shared/ServerPackets.cs:3560-3578`；作为投射物与 125 包字段依据，不提供本地水位或缓存容量常数。
Rust：`crates/mir-client/src/net.rs:707/3030/3046/3070/4592`；`crates/mir-client/src/ui/projectile.rs:602` `consumed_abs_seq`；`crates/mir-client/src/app.rs:210` `next_projectile_ack`、`:2807` 快照接入。

## 实现与 F4 统一口径

`NetCommand::AckProjectileEvents{generation,through}` 是**本地确认水位（进程内，非 wire ack）**；`crates/mir-protocol` 零 diff。
`total` 为累计收到数，不减；不变量 `total == len + reclaimed`。API 注释的 `events[i]` 绝对序号 `reclaimed()+i` 指 **0-based 绝对下标**。
`through` / `consumed_abs_seq` 是 **1-based 已确认条数（水位）**；若事件序号同按 1-based 计，`events[i].abs_seq = reclaimed()+i+1`（数学记号，非字段）。
只回收 1-based 序号 `<= through` 的前缀，即 0-based 下标 `< through`；例 `reclaimed=2, through=3` 删除下标 2 的一条。重复/滞后无操作、超尾夹取，代号不等忽略。
ack 分支在 `send` 前 return，stream 写入 0 字节；ack 触发＝`poll_network` 收到快照后，零/重复水位不发，无独立定时器。

## 测试矩阵与独立证据

| 项 | 核验结果/计数 |
| --- | --- |
| lib 基线→A→A+B | 319→324→326 passed；最终 0 failed / 0 ignored |
| `--lib r64_` | 7（net 5 + ui 2） |
| `round64_event_reclaim` / `round64_projectile_ack` | 4 / 3 |
| `--bin mir-client r64_` | 1 passed / 23 filtered |
| 历史 R59/R61/R62/R63 target | 16 / 11 / 5 / 3 |
| fmt / clippy | exit 0 |
| C 独立 CARGO_TARGET_DIR | PASS-WITH-PARTIAL；[C 原证据](round64-c-evidence.md)，85 行/8780B，SHA256 `30d08ae3c6457417b08c788e4102a6bf2b4d474325e7cefbef7abdcabbc9f416` |

红→绿：A 桩 net `2c78b317…`→实现 `3f8046bdb5fa1d179d8d111a05f311d16dada9c239951f32e7583456e2277825`；测试块红=绿 `e928aaf4…`→仅注释修复 `d95b865e…`。
A 集成红=绿 `9a3e831409c8fead15df4932a3af0d746af4daebfc2b871b0d3d8c8b8c889d05`；B 桩 `c9b9a338…`、测试红=绿 `b57c6318f4bdc78940d2502847ac24cb8787259bfb8d05710a75b05da3b7d5dd`；桩下无通过测例。
F1/F3 CLOSED-with-comment-only；F2 由 C 补录 match 触点；F4 以上文统一计数口径承接。

## Lead 唯一门禁事实

attempt-1 HEAD=`063c6f609e2ee61fc4d3cd8c5d322dcc32c49ce2`，EXIT=0，12/12 OK。
日志 `target/round64/gate/attempt-1.log`：205352B，SHA256 `3e228cef3ce730eac76ce936679b8557308d8d4f20ed1d766dbfb7a0b92b2604`。
UTC `2026-10-26T17:31:43Z`–`2026-10-26T17:35:16Z`；meta `target/round64/gate/attempt-1.meta.txt`，含 EXIT/HEAD/UTC/字节/SHA/operator。

## 审计后修复（Lead 维护，A-M1/A-M2/A-M3）

- A-M1（阻塞）CLOSED：`run-checks.sh` 白名单由 17 项扩为 18 项（`:14` paths、`:18` artifact count、`:28` 负控循环与提示文案），纳入 `round64-final-audit.md`；`--verify manifest`/`--verify negative` 在 18 项下重跑通过。
- A-M2（披露）CLOSED：C6 三项实值补录——首次 ack 后 `reclaimed=2`、未确认 `len=0`、`total=2`；再 push 未确认态 `2/1/3`；再 ack 到 3 `3/0/3`（来源 `crates/mir-client/tests/round64_projectile_ack.rs:91-95,101-110,112-116`，审计独立复跑同值；本 README 与 C 证据 §9 同步）。
- A-M3（轻微补证）PARTIAL：半消费→半 ack→再 push 的 UI 组合场景未新增自动化测试（未实跑，不称已覆盖）；如需补测按规则改测试并重跑门禁，承接 R65。

## PARTIAL（六项分别保留）

1. 多消费者/克隆快照全局最低水位未实现，当前仅 app.rs 单消费者。
2. 事件表未限容，未确认 backlog 仍可增长。
3. known_positions 容量/LRU 仅契约；Crystal 无容量依据，不填容量常数。
4. 服务端回收协议未做，不从本地确认推导服务端能力。
5. 本地定向绿不等于全量门禁，门禁由 Lead 唯一执行。
6. 延续 R61/R62/R63 PARTIAL 交接，生产动作/特效、素材、设备可听等历史边界不整项清零。

## 交付与复跑

[run-checks.sh](run-checks.sh)（`--list`，20 项）；[gen-tree-hashes.sh](gen-tree-hashes.sh)；[commit-tree-hashes.txt](commit-tree-hashes.txt)；[final-manifest.json](final-manifest.json)（18 artifacts，自身不列入；含 [round64-final-audit.md](round64-final-audit.md)）；[客户端 §5.31](../../client.md)、[路线图 R64/R65](../../roadmap.md)。
提交树清单冻结 5 功能源码/测试 + client/roadmap/本 README 共 8 路径；按 `git show 063c6f6:<path>` 重算。README 在该提交不存在，记 `MISSING`；其余 7 项为 blob 内容 SHA256，不与工作树文档 SHA 混用。
树清单 SHA256：`802a11ec2d5e8f28ea729fec85a837f3366e229f637a2495f8b9fb02711710e1`（8 行）；manifest 保存清单自身 SHA，避免自引用。manifest 同列工作树字节/行数/SHA、HEAD blob 内容 SHA256（缺失为 null）与 tracked 状态。
证据 `target/round64/d/attempt-1/` 保存命令、UTC、stdout/stderr、exit、逐项秒数与完整输出；检查含非零测试计数、错误输入拒绝、链接/镜像索引、diff、树哈希、gate/meta、manifest、禁称正面声明与新增 ignore/allow。
执行成本：L0 文档/哈希检查与 L2 定向 cargo；默认共享 target，未新增全量门禁。首轮 2026-10-26T17:40:03Z–17:40:27Z，`SUMMARY PASS=20 FAIL=0 SKIP=0 TOTAL=20 seconds=24`，exit=0；原始记录见 `summary.tsv`，文档定稿后另存 `final-recheck/` 复查。修复后（18 项脚本，A-M1/A-M2 修复提交前）2026-10-26T17:59:36Z–18:00:11Z 复跑 `SUMMARY PASS=20 FAIL=0 SKIP=0 TOTAL=20 seconds=35`，exit=0，日志 `target/round64/fix/attempt-1/`。
