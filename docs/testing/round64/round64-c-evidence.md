# R64-C 刑部独立核验证据（attempt-1＋F1/F3 修复复核）

- 子单：`JJC-20261027-001-sub-a39c0338`（R64-C）；核验者：xingbu-r64-c（deepseek）
- 仓库/基线：`mir-next`，HEAD `6f82f3ba2b21f37823ef000ea2f65da3ec3652a6`；工作树含 A/B 未提交改动
- 独立性：新建 `CARGO_TARGET_DIR=target/round64/c/cargo-target`、`TEMP=TMP=target/round64/c/tmp`；未复用 Lead/A/B 二进制；未运行 `ci.ps1`
- 区间 UTC：首轮 2026-10-26T17:16:24Z→17:18:40Z；F1/F3 复核 17:26:46Z→17:27:05Z；日志在 `target/round64/c/attempt-1/`（复核另存 `recheck-comment-fix/`）
- **结论：PASS-WITH-PARTIAL**（定向测试全绿、真红成立、wire 零改动；F1/F3 已仅注释修复并经 C 复算证实＝**CLOSED-with-comment-only**；PARTIAL＝既定范围限制＋F2 证据补录＋F4 措辞由 D 承接，见 §7/§8）

## 0. 交付面（修复后复算）

- `crates/mir-client/src/net.rs` 9366 行 `3f8046bdb5fa1d179d8d111a05f311d16dada9c239951f32e7583456e2277825`（+304）
- `crates/mir-client/src/ui/projectile.rs` 2080 行 `b82950a5ef521c75f82a5348e26a18d95b78b09c5624bb7645a041f46df5f9ce`；`app.rs` 4306 行 `a0c746e989cfa39e8479b5c0f27906caff30e225da78faa0c059542f059edf8f`（+141）
- `tests/round64_event_reclaim.rs` 126 行 `9a3e831409c8fead15df4932a3af0d746af4daebfc2b871b0d3d8c8b8c889d05`（未变）；`tests/round64_projectile_ack.rs` 231 行 `b57c6318f4bdc78940d2502847ac24cb8787259bfb8d05710a75b05da3b7d5dd`（未变）
- diffstat 548+/34−；仅 3 修改＋2 新增测试＋`docs/testing/round64/`；未跟踪 process 文档 3＋R63 冻结 JSON 2，与基线一致。

## 1. 独立复跑（C7，首轮全部 exit 0）

| 命令 | exit | 结果 |
| --- | --- | --- |
| `cargo fmt -p mir-client -- --check` | 0 | 干净（空输出） |
| `cargo test -p mir-client --lib` | 0 | **326 passed / 0 failed / 0 ignored** |
| `cargo test -p mir-client --lib r64_` | 0 | 7 passed（net 5＋ui 2）/319 filtered |
| `cargo test -p mir-client --test round64_event_reclaim` | 0 | 4 passed |
| `cargo test -p mir-client --test round64_projectile_ack` | 0 | 3 passed |
| `cargo test -p mir-client --test round63_projectile_wire` | 0 | 3 passed |
| `cargo test -p mir-client --test round62_projectile_wire` | 0 | 5 passed |
| `cargo test -p mir-client --test round59_projectile_render` | 0 | 16 passed |
| `cargo test -p mir-client --test round61_projectile_queue` | 0 | 11 passed |
| `cargo test -p mir-client --bin mir-client r64_` | 0 | 1 passed / 23 filtered |
| `cargo clippy -p mir-client --all-targets -- -D warnings` | 0 | 无警告（40.65s） |

**计数自证**：现 lib 326＝基线 319＋A 5＋B 2（`--lib r64_` 7＝net 5＋ui 2，319 filtered→基线 319）。A 红日志 `5 failed + 319 filtered = 324`（A 后）；B 红日志 `2 failed + 324 filtered = 326`（A+B 后）。**324/326 都是各自"后数"，基线＝319**；A/B 自报与实测一致。

## 2. C1 真红核验（首轮快照，SHA 为修复前）

- A：桩 net.rs `2c78b317…`（`net-r64-stub-final.rs`）＝红日志首条 SHA；实现 `09551dbe…`（`net-final-backup.rs`）＝首轮 net.rs；R64 测试块红=绿 `e928aaf4…`（171 行，当前 net.rs 连续子串）；集成测试红=绿 `9a3e8314…`。stub→impl 仅 3 hunk（`reclaimed`/`trim`/`apply`），测试行 0 改动。
- B：桩 app `c9b9a338…`、projectile `f357f7ad…`；集成测试红=绿 `b57c6318…`；stub→impl 仅 3 行实现差异（无测试行变化）。
- 红日志为真实 assert：A lib 0 passed/5 failed、A 集成 0/4（`round64_event_reclaim.rs:99:5`）；B lib 0/2、bin 0/1（left (0,0) vs right (0,1)；桩版 app.rs:3835＝当前 3839，因桩短 2 行，相符）、集成 0/3。**无"桩下通过"测例**；无编译错误/`unimplemented!` 充红。
- 局限：红绿字节相同由证据 SHA 与当前文件 SHA 交叉证实；日志本身不打印 SHA（C8 允许同 attempt 记录）。

## 3. C2 非 wire

- `git status --porcelain crates/mir-protocol/` 与 `git diff -- crates/mir-protocol/` 均空：wire/协议零改动。
- `handle_command` ack 分支（net.rs:4592）位于 InGame gate（:4300-4302）之后、`send(stream,&packet)`（:4604）之前并 `return Ok(())`；`NetCommand` 仅进程内枚举、无 encode/serde 路径；全仓 match 点只该分支与 `local_animation` 的 `_ => None` 兜底。
- 独立绿：`net::tests::r64_ack_command_emits_no_client_packet`（真实 TcpListener/非阻塞 server 断言 ack 后写 0 字节，`Turn` 正对照确实写字节）。

## 4. 语义边界（定向断言全绿＋静态复核）

- `trim_projectile_events_through`：只 drain 前缀（`through-already` 条）；`through<=reclaimed` no-op；`through>=total` 夹到表尾不 panic；`total` 只增；`reclaimed()+len==total` 由测试钉死。
- generation 不等忽略；跨代 UI 水位成对归零；trim 后空表不复位/不重放、中途消费者经 dropped 路径跳过前缀；`through==0`/重复水位不发 ack（app host 测试）；trim 后再 push 序号连续（A/UI 双算术核对一致）。

## 5. M3 注释真值

- 门禁点名两处已更正：net.rs:2781-2804（字段文档）与 ui/projectile.rs:688-707（`consume` 文档）；**无** `total==len`/`dropped==0` 假陈述残留。
- **F1（首轮 PARTIAL）→ 第 2 轮已仅注释修复：CLOSED-with-comment-only（见 §8）**；另无新增 `#[ignore]`/`#[allow]`、无正面禁称。

## 6. 红线/边界

- 新增 `#[ignore]`/`#[allow]` = 0；mir-client tests 既有 `#[ignore]`=7、全仓 34，HEAD/now 同数。
- 改动文件增行无禁称词；唯一命中为未改动的 net.rs:3752「状态数有界」（远端运动状态数，非本轮声明）。
- 未降断言：既有测试文件未修改（git status 仅 3 修改/2 新增）；R59–R63 四个 target 复跑全绿且计数一致。
- 归属：未动 `crates/mir-protocol` 或其他 crate；未 commit/push；`ci.ps1` 未运行。

## 7. 发现与偏差（均不阻塞）

- F1：首轮 M3 模块级注释残留 → 第 2 轮仅注释修复，§8 确认 CLOSED-with-comment-only。
- F2：A 证据未列新变体触达的全部 match 点（M6 要求）；C 已补录——仅 net.rs 2 处，其余文件只构造，无白名单越界（证据补录项，随交付保留）。
- F3：首轮 net.rs:9202 注释引用不存在的 `red.log` → 第 2 轮改为 `red-final-lib.log`（首次未 fmt 版 `red-lib.log`），§8 确认 CLOSED-with-comment-only。
- F4（P3 措辞）：`events[i]` 的 0-based 索引口径与 `through`/`consumed` 1-based 条数口径混用；行为自洽、无功能缺陷，**建议 D 文档统一口径**（不阻塞）。
- 自报偏差：A/B README 数字与实测一致；第 2 轮 A 自报 4 SHA 与 C 逐字节复算一致（§8）。

## 8. F1/F3 修复复核（第 2 轮，仅注释）

- 修复后交付面：net.rs `3f8046bd…`（9366 行）、ui/projectile.rs `b82950a5…`（2080 行）；`round64_event_reclaim.rs`、`app.rs` SHA 不变；R64 测试块 `e928aaf4…`→`d95b865e…`。
- 独立复算：C 重算 4 文件 SHA 与 A `after-sha.log`/`final-files.sha256` 完全一致；A `test-block-current.rs`＝`test-block-after.rs`＝C 独立抽取块＝`d95b865e…`。
- 仅注释证明①（F3）：当前 net.rs vs 首轮备份（`net-final-backup.rs`）全文件 diff 仅 net.rs:9199-9200 两行注释；测试块 diff 亦仅同两行，测试代码 0 行变化（`test-block.diff` 仅注释）。
- 仅注释证明②（F1）：将 A `my-f1-hunks.diff` 首 hunk 逆应用到当前 ui/projectile.rs，得候选 SHA=`8fab7a0c…`＝首轮 before SHA（逐字节），即当前文件＝before＋模块文档注释；新增表述为「R64 A 本地 trim、非 wire、total 不减、未确认 backlog 可增长、不称有界/限容、PARTIAL」，与 trim 事实一致、无正面禁称。
- 证据瑕疵（不阻塞）：A 的 `my-f1-hunks.diff` 实为对 HEAD 的截断 diff（80 行、末 hunk 不完整，含 B 的 20 行代码），非 F1-only；C 改用逆推 SHA 法证明。
- 复跑（独立 target，UTC 17:26:46Z→17:27:05Z，全 exit 0）：lib **326/0/0**；`--lib r64_` 7/319；`--test round64_event_reclaim` **4**；`--test round64_projectile_ack` **3**；`--bin mir-client r64_` 1/23；fmt 空；clippy `-D warnings` 0（4.38s）。

## PARTIAL 承接（与门下 C9 口径一致）

客户端单消费者前缀回收：定向测试绿＋真红成立＋零 wire；服务端回收未做、多消费者/全局最低水位未证、事件表未限容、known_positions 仅契约、ack 触发＝快照到达。本地绿只称定向绿，门禁由 Lead 唯一在功能 HEAD 执行。

边界：本单仅新增本文件与 `target/round64/c/**`；其余只读；未改实现/测试、未 commit/push、未跑 `ci.ps1`。
