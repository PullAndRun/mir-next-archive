# R64 尚书派发：125 事件表确认水位与安全前缀回收

结论：门下 PASS-WITH-CHANGES 的 M1–M6 与 C1–C10 已并入 A/B/C/D 实际子单提示词；主单已转 Doing。此为派发登记，不代表实现、真红、绿、门禁或终审已通过。

## 基线与看板

- 主单 `JJC-20261027-001`（尚书令，收旨 2026-10-26T16:44:38Z）：派发前 Assigned，派发后 Doing；todos 未代填，未申请 Done。
- 仓库 `C:/Users/diana/Documents/github/mir-next`；实查 `HEAD=6f82f3ba2b21f37823ef000ea2f65da3ec3652a6`；派发前 `git status --short` 4 项未跟踪（R64 计划、门下评审、两份 freeze JSON），本文落盘后 5 项，受控文件仍无改动。
- 中书计划 `docs/process/round64-plan.md`：99 行 / 11,305 B / SHA256 `ca4ba4aae7b8c06909478ede091e4e9b0f9130d0c120b6e36074154c6a3e454c`。
- 门下评审 `docs/process/round64-menxia-review.md`：59 行 / 7,437 B / SHA256 `67b0922ab31b4066b50cf6f77357a481ab2ad088045642addd29e7d5ea8faed1`；裁决 ①批准 ②修改批准 ③批准仅契约 ④修改批准（限 net.rs 范围）。
- 子单经 `delegate` 登记：四次委派、主单 `state Doing`、`progress`、`validate`（196 条符合 schema v3.1）全部 EXIT=0，原始命令/输出见 `target/round64/shangshu/01-delegate.log`。
- CLI 无 `edit --assign`；承接方即责任列。委派子单创建即 `Doing`（`delegation_bypass=true`），故未重复执行 `state <子ID> Doing`（该转移 Doing→Doing 非法）；四单实查 state=Doing。

## 子单契约（A→B→C→D 串行，写范围互斥）

| 单 | 子单 ID / 责任 | 依赖 | 写白名单 | 交付契约 | 验收 / 证据 |
|---|---|---|---|---|---|
| A | `JJC-20261027-001-sub-dd8d1bbc` / 兵部 bingbu | 派发准入 | `crates/mir-client/src/net.rs`、`crates/mir-client/tests/round64_event_reclaim.rs`、`target/round64/a/attempt-1/**` | `trim_projectile_events_through`、`reclaimed()`、`AckProjectileEvents{generation,through}`、ack 分支（明述 InGame gate 前后）、无包分支；`round64_*` 测试红→绿；更正 `net.rs:2760-2777` 假陈述 | `cargo test -p mir-client --lib net::tests::r64_`；`--test round64_event_reclaim -- --nocapture`；实际非 0 测试 + exit 0；ack 对 stream 0 字节 |
| B | `JJC-20261027-001-sub-432bfa44` / 工部 gongbu | A 交付 API | `crates/mir-client/src/app.rs`、`crates/mir-client/src/ui/projectile.rs`、`crates/mir-client/tests/round64_projectile_ack.rs`、`target/round64/b/attempt-1/**` | `consumed_abs_seq()`；`app.rs:2756` 同步后仅水位推进时发 ack + last_acked；`app.rs:3409` 宿主驱动测试；更正 `ui/projectile.rs:670-674` 假陈述 | `cargo test -p mir-client --lib ui::projectile::tests::r64_`；`--test round64_projectile_ack`；禁 0 tests；禁改 net.rs 控制流 |
| C | `JJC-20261027-001-sub-a39c0338` / 刑部 xingbu | A、B 交付 | `docs/testing/round64/round64-c-evidence.md`、`target/round64/c/attempt-1/**`；其余只读 | 独立复跑 + 代码对照 + 真红核验 + 禁称/边界；reclaimed、未确认 len、total 三列实值；四历史 targets | 独立 `CARGO_TARGET_DIR`；禁 `ci.ps1`；clippy/fmt/diff；证据含 HEAD/UTC/命令/exit/SHA |
| D | `JJC-20261027-001-sub-0286eab0` / 礼部 libu（gpt subagent） | C 通过 + Lead 功能提交/唯一门禁 | `docs/testing/round64/{README.md,final-manifest.json,run-checks.sh,gen-tree-hashes.sh,commit-tree-hashes.txt}`、`docs/client.md` §5.31、`docs/roadmap.md`、`target/round64/d/**` | 证据文档批；known_positions 仅契约（无上限、禁容量常数）；服务端边界不发明协议；reclaimed/len/total 分列；阶段字段按实际 | `check_links.py`、`git diff --check`、JSON 解析、≤1MiB、UTF-8 无 BOM/LF、SHA 复算；不覆盖 C 证据与 R59–R63 冻结证据 |

## M1–M6 落点（门下必改）

- M1：A 提示词明确 `AckProjectileEvents` 为「本地确认水位（进程内 NetCommand，非 wire ack）」，B 同口径；禁称服务器确认/协议 ack。
- M2：按裁决②接受桩红；A/B 须列桩 diff、桩/测试/实现 SHA 与断言读取行，桩下通过的测例必须改断言重取红。
- M3：A 更正 `net.rs:2760-2777`，B 更正 `ui/projectile.rs:670-674` 的「total==len / dropped==0」假陈述，禁遗留。
- M4：单消费者依据写入 B 提示词——静态事实仅 `app.rs:2639` 接收 `NetEvent::Snapshot`；多消费者/克隆快照入 PARTIAL，不写成已证。
- M5：B 测试由 `app.rs:3409` 宿主驱动，`r64_ack_sent_only_when_watermark_advances` 落点与 seam 明述，禁 0 tests 当 PASS。
- M6：A 须列新变体触达的全部 match 点；穷尽匹配溢出 `net.rs` 白名单即停并报 BLOCK；B 若需 `ui/mod.rs`（计划 §四）须先报追加白名单，不得默改。

## C1–C10、PARTIAL 与禁称

- 条件已落各单：C1 红真实性（A/B 自证 + C 复核同测试文件 SHA 红绿相等）；C2 本地水位非 wire（A）；C3 前缀边界与不变量（A/C）；C4 未确认不丢（B/C）；C5 混合/旧代/中途消费者（A/B/C）；C6 分列披露（C/D）；C7 独立复跑隔离（C，独立 CARGO_TARGET_DIR 或先删 `target/debug/deps/mir_client-*`）；C8 证据口径（C）；C9 阶段口径（D 不预写 PASS）；C10 门禁边界（C/D 禁跑 `ci.ps1`）。
- PARTIAL：多消费者/克隆快照全局最低水位未证；`known_positions` 容量/LRU 仅契约；服务端回收协议未做且未发明；SkipFrames/ActionFeed 未接；R61 P01–P12 按原编号承接。
- 禁称：事件表已限容/有界/无内存增长、已物理回收全场景、生产 SkipFrames 已接、R64 已 PASS、门禁通过、全场景无丢重。

## DAG、门禁与边界

`A → B → C → Lead 功能提交 → Lead 唯一 ci.ps1 → D → 文档批 → 独立终审 → 指定 confirm_by`。
- 定向测试绿只称「定向测试绿」；SKIP→PARTIAL，总结果非全 PASS；ignored 按实测列数量与位置。
- 门禁后任何功能/测试改动使旧 HEAD 门禁失效，须重跑；失败日志保留、另编号 attempt-N。
- 本派发只新增本文与 `target/round64/shangshu/**` 本地日志；未改 crates/计划/评审/其他文档，未 commit/push，未运行 cargo 或 `ci.ps1`。执行 agent 由 Lead 按 DAG 启动并记录 ID 映射，本文不冒称四部已执行。
