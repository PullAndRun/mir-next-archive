# R64 尚书独立验收

**Verdict：ACCEPT-WITH-PARTIAL（限定功能与证据验收）**。A-M1/A-M2 已闭环；A-M3 与历史 PARTIAL 保留，不宣称 R64 全项 PASS，不代门下确认。
验收者：gpt-6.1-sol；父单 `JJC-20261027-001`；基线 `HEAD=origin/main=dc094a32c600f97375520f13859973845b498044`。
先读中书计划、门下评审、尚书派发及刑部终审；旧审计 BLOCK 是修复前结论，本报告独立复验后给出上述限定验收结论。
证据根：[attempt-1](../../target/round64/shangshu/attempt-1/)；独立构建目录 `target/round64/shangshu/cargo-target`。

| 核验项 | 判定 | 独立证据与边界 |
|---|---|---|
| 1 A-M1 | PASS | `run-checks.sh:14/18/28` 为 18 路径/计数/负控，含 audit；单独 manifest/negative 均 exit0；20 项全跑全绿。 |
| 2 A-M2/C6 | PASS | README:43–47、C证据:87–93 披露 reclaimed/未确认len/total＝2/0/2、2/1/3、3/0/3；来源 ack测试:91–116；独立3例通过。 |
| 3 A-M3 | PARTIAL | README:47 明示半消费→半ack→再push UI组合未新增测试、未实跑，承接R65；未冒称已覆盖。 |
| 4 manifest | PASS | 独立重算18项路径/字节/实际LF行数/工作树SHA/HEAD blob SHA/tracked，mismatch=0；自身不列入；gate字段全符。 |
| 5 门禁/提交链 | PASS | gate原字节205352B/指定SHA吻合，12 OK/0 FAIL；meta实有12字段（含command/operator），逐键核对全符；四提交及顺序存在、origin同步。 |
| 6 范围/诚实性 | PASS | bec6415仅5个round64文档；新增ignore/allow=0；协议零diff、文档批后crates零diff；stage仍“门禁通过…终审待…未PASS”；8行树哈希一致。 |
| 7 残留声明 | PARTIAL | README:49–56、audit:35/44、roadmap:217–251逐项承接；仅一个生产UI消费者，net.rs:3881另有同步接收器，禁外推全仓唯一接收点。 |

## 独立复跑

- 命令：`R64_LOG_DIR=target/round64/shangshu/attempt-1 bash docs/testing/round64/run-checks.sh`；通过外层脚本设置独立 CARGO_TARGET_DIR，未复用Lead二进制。
- UTC：2026-10-26T18:05:08Z–18:07:13Z；外层实测125秒，脚本循环124秒（启动/取整差异）；EXIT=0。
- 原始摘要：`SUMMARY PASS=20 FAIL=0 SKIP=0 TOTAL=20 seconds=124`，见 [summary.tsv](../../target/round64/shangshu/attempt-1/summary.tsv) 与 [run.meta](../../target/round64/shangshu/attempt-1/run.meta.txt)。
- R64 lib7、A集成4、B集成3、bin1；历史R59/R61/R62/R63为16/11/5/3；测试均0 failed/0 ignored。fmt/clippy、链接、diff、树哈希、gate/meta/manifest、禁称、属性、负控与语法均通过。
- 单独 `--verify manifest` / `--verify negative` EXIT=0；负控拒绝坏字节、同长篡改SHA、meta及18项哈希错误。只读确认refresh分支写manifest，**未执行refresh**。
- 单独 `cargo test -p mir-client --test round64_projectile_ack`：18:08:42Z–18:08:43Z，1秒，EXIT=0，3 passed/0 failed/0 ignored；TEMP/TMP均为 `target/round64/shangshu/tmp` 的绝对Windows路径。
- 三值核对：测试:92–95直接断言2/0/2；:98追加一条且:99同步UI不修改网络表，因此确认前为2/1/3；:113–116直接断言3/0/3。日志不打印三值，以已复跑通过的源码断言及状态变换为依据。
- 全跑脚本内部按既定行为将TEMP/TMP置于attempt-1/tmp；另一次单项复跑使用上述shangshu/tmp，均在获准目录内。

## 哈希、范围与来源

- [独立JSON](../../target/round64/shangshu/attempt-1/independent-audit.json)记录全部重算值；[scope](../../target/round64/shangshu/attempt-1/scope.log)记录初始status、提交链与bec6415五路径。
- 提交链：`063c6f6 → 7ace28f → bec6415 → dc094a3`；`063c6f6^..dc094a3 -- crates/mir-protocol`及`7ace28f..dc094a3 -- crates`差异均0字节。
- 门禁SHA256：`3e228cef3ce730eac76ce936679b8557308d8d4f20ed1d766dbfb7a0b92b2604`；UTC17:31:43Z–17:35:16Z、exit0；身份仅按meta署名Lead，不以日志证明现实身份。
- 树清单8行SHA256 `802a11ec2d5e8f28ea729fec85a837f3366e229f637a2495f8b9fb02711710e1`；7项取063c6f6 blob，README当时MISSING；另记录7ace28f blob，未与当前文档混用。
- 当前README 64行/6427B/SHA256 `64afe5dc3ff8116d2e8b35b8b3f49575aec2d2e9ea822d323ecfe8a68be2e560`。
- 当前C证据93行/9389B/SHA256 `77e4b11b6b29726f7533ef2abbc8d014670b45823790d07fab1dff3813bcaa14`。
- 当前audit 46行/6868B/SHA256 `9245b0663140f15454d0bcc9f62ad7fe0f586b576777a66347c02e4ae631e7be`；三文件当前值均与manifest一致。
- README所引“C原证据85行/8780B/30d08ae3…f416”是7ace28f历史快照，独立git show验证相符；当前补录版由manifest另记，不能称当前文件仍为该旧SHA。

## 残留 PARTIAL（逐条保留）

1. 多消费者/克隆快照全局最低水位未做；仅app生产UI消费者，另有同步wait_for_snapshot接收器。
2. 事件表未限容，未确认backlog可增长；本地安全前缀裁剪不证明全场景内存有界。
3. known_positions容量/LRU仅契约，无实现、无任意容量常数。
4. 服务端回收协议未做；本地NetCommand水位不产生wire ack。
5. A-M3半消费组合UI覆盖缺口未实跑，后续补测试须按门禁规则重新验证，承接R65。
6. R61 P01–P12按原编号承接：回收/clone/容量、生产SkipFrames、缓存、ImageInfo.offset、设备/Live、33路削波、105sfx、全局HiddenSpanSource、CWD/素材、会话边界、缺素材early-return、R60 P-1..P-6；本轮子项进展不整项清零。

## 交付边界与流程说明

仅新增本文及 `target/round64/shangshu/**`；未修改实现/测试/既有脚本/manifest，未commit/push，未运行ci.ps1；未触碰两份既有未跟踪冻结JSON。
看板只读附查：[board-show](../../target/round64/shangshu/attempt-1/board-show.json)由默认edict CLI返回Doing、todos空、ready_to_close=false，与委派提示Review不符，已通知Lead核对EDICT_HOME/实际看板；本验收不修改看板、不声称主单已收口。
本次未发现功能或证据必改项；看板状态差异由Lead核实后再走对口确认，不将限定验收自动提升为父任务Done。
