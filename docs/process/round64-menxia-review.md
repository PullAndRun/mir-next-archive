# R64 门下独立评审：PASS-WITH-CHANGES

结论：附条件准奏主批准入；四项裁决＝①批准 ②修改批准 ③批准 PARTIAL ④修改批准（限定范围）；必改 M1–M6。不代改计划、不派发、不转 Doing。

## 基线与独立性核对

- HEAD=`origin/main`=`6f82f3ba2b21f37823ef000ea2f65da3ec3652a6`；`git status --short` 仅 3 项未跟踪（本计划 + R63 两份资源冻结 JSON），受控文件无改动。
- 计划 99 行 / 11,305 B / UTF-8 无 BOM / 无 CR；SHA256 `ca4ba4aae7b8c06909478ede091e4e9b0f9130d0c120b6e36074154c6a3e454c` 复算与输入一致。
- 「125 无 ack/序号→不发明协议」**属实**：Crystal `Shared/ServerPackets.cs:3560-3578` 仅 Spell/Source/Destination；Rust `push_projectile_event`（`net.rs:7296-7312`）同三字段。
- append-only 属实：`projectile_events_total` 饱和自增、无上限、无删除，仅整份重建清表（`net.rs:2760-2777`）；每快照 clone 整表（`net.rs:4524-4526`）。
- 单消费者有静态依据：全仓仅 `app.rs:2639` 接收 `NetEvent::Snapshot`，`app.rs:2756` 调 `ui.sync`；`ui/projectile.rs:567` 为绝对游标，`consume` 在 `:685-732`。
- 回收落点可行：`handle_command` 持 `&mut WorldSnapshot`（`net.rs:4215-4219`），ack 分支可就地接收并先行 trim，无须改会话循环结构；6 处 `send_snapshot`（3898/3911/3921/3937/3946/3971）不必增删。

## 四项待裁决

| # | 裁决 | 理由 / 限制 |
|---|---|---|
| ① 主批准入 | **批准** | 只回收「≤ consumed」前缀且 total 不减、generation 校验，旧代与克隆快照不被改写（net 线程快照与 `live.snapshot` 副本分离）。多消费者/全局最低水位未证 → 入 PARTIAL，禁称「安全已证」。 |
| ② no-op 桩红 | **修改批准** | 接受 test-only+桩的 assert 红（较 R63 M2 重建基线更省），但须反空泛绿：桩下通过的测例视为无鉴别力，必须改断言重取红；红绿测试文件字节 SHA 必须相同。 |
| ③ known_positions | **批准仅契约** | Crystal `PlayerObject.cs:3546-3556` 仅 TargetPoint 兜底，无容量/LRU 依据；任意上限会丢可信落点。合同须写「无上限、增长风险未解」，禁写容量常数。 |
| ④ 改 net.rs | **修改批准（限范围）** | 仅允许：新增 1 个 `NetCommand` 变体、`handle_command` 内 ack 分支（置于 InGame gate 前后须明述）、无包分支、测试；禁改既有变体/包序/`send`/snapshot 节奏。若须改会话循环控制流 → 停并报 BLOCK。 |

## 附条件（可证伪）

- **C1 红真实性**：红日志为 assert panic（含文件:行与 expected/actual），桩 diff 与 SHA 入证据；红绿同一测试文件 SHA 相等；`unimplemented!`/编译错误/panic 不计红；桩下假绿必须改测。
- **C2 本地水位非 wire**：diff 对 `mir-protocol`/wire 编解码零改动；`r64_ack_command_emits_no_client_packet` 断言 ack 返回 Ok 且对 stream 写出 0 字节；文中禁「服务器确认/协议 ack」，只可称本地确认水位。
- **C3 前缀边界**：`through < reclaimed` 无操作；`through ≥ total` 则 len=0；越界夹取；total 永不减；断言不变量 `reclaimed == total - len`；空表 `total==0 → reclaimed==0`。
- **C4 未确认不丢**：慢消费者（消费半→ack 半→再 push）无丢失无重放；`consume` 算术不改仍成立（静态核：`dropped=total-len` 恰等于 reclaimed，`index=cursor-dropped` 与绝对序号 `reclaimed+i` 一致）；R59–R63 既有断言字节不变。
- **C5 混合/旧代/中途消费者**：重连 generation+1 且 total/len/reclaimed 归零；旧代 ack 不得删新代事件；中途新建 `Projectiles`（consumed=0）遇 reclaimed>0 时经 dropped 路径跳过已回收前缀、不重放——须有测试并登记为限制。
- **C6 分列披露**：报告/README 必须同时列 `reclaimed`、未确认 `len`、`total` 三个实值（取 ack 接线测试输出）；禁「已限容/有界/无内存增长/已物理回收全场景」。
- **C7 独立复跑隔离**：C 必须用独立 `CARGO_TARGET_DIR`（如 `target/round64/c/target`）或先删除 `target/debug/deps/mir_client-*`，记录目录/命令/exit（R63 D3 共享 target 陈旧二进制教训）；不得复用 Lead 门禁二进制。
- **C8 证据口径**：HEAD、UTC 起止、命令、exit、断言输出、桩/测试/实现 SHA 落 `target/round64/c/`；文档 UTF-8 无 BOM/LF；行数/字节/SHA 复算。
- **C9 阶段口径**：D 不预写 PASS；批次固定表述「客户端单消费者前缀回收 PASS-WITH-PARTIAL；服务端回收未做、多消费者/全局水位未证」。
- **C10 门禁边界**：`ci.ps1` 仅 Lead 在功能 HEAD 跑一次；C/D 不得运行；本地绿只能称定向测试绿。

## 必改（尚书落实后准派）

- **M1 措辞**：`AckProjectileEvents` 须在计划与代码注释定义为「本地确认水位（进程内 NetCommand，非 wire ack）」，G2/B 文案同步；Crystal 无 ack 字段，不限定会被读成发明协议，与 §三非目标冲突。
- **M2 反空泛绿**：§五.1 补 C1 条文，列桩 diff/SHA 与断言行。
- **M3 注释真值**：trim 后 `net.rs:2760-2777`、`ui/projectile.rs:670-674` 的「total==len、dropped==0」成假陈述，A/B 须在白名单内同步更正，禁遗留。
- **M4 单消费者依据**：§六写明「仅 `app.rs:2639` 一处接收」为静态事实，并把多消费者/克隆快照入 PARTIAL，不得写成已证。
- **M5 测试宿主**：`app.rs` 有 tests 模块（`:3409`），计划仅给 `ui::projectile::tests::r64_` 与集成文件；须明确 `r64_ack_sent_only_when_watermark_advances` 落点与 seam，禁 0 tests 当 PASS。
- **M6 越界检查**：A 须列出新变体触达的全部 match 点；若穷尽匹配溢出 `net.rs` 白名单 → 退门下，不得默扩。

## 风险与不做清单

- R1（高）：多消费者共用同一回收表时，先 ack 者会裁掉他人未确认前缀；本轮以单消费者+PARTIAL 限制，禁外推。
- R2（中）：`dropped` 名实不符（trim 后等于 reclaimed），沿用旧词会误报「丢失」，报告须分列。
- R3（中）：桩红若断言过弱可假绿，由 C1 兜底；R4（低）：6 处 send_snapshot 的 ack 时序若插错位置会重复/漏裁，由 C3/C4 测试钉死。
- 不做：服务端协议与回收、known_positions 容量/LRU、SkipFrames/ActionFeed、wire/素材/渲染、R61 十二项清零、主任务转 Doing。

## 派发要求（给尚书）

- 先落 M1–M6 再建 A/B/C/D 子单；A→B→C→D 串行，禁止先改实现后补测；每单提示词附 C1–C10 与 M1–M6。
- 事实补录：当前树 3 项未跟踪归属、桩红责任人与路径、A 变体的 no-packet 断言位置；红绿日志/门禁/实际 ignored 只能实施后据实写。
- 顺序：功能 commit → Lead 唯一 `ci.ps1`（功能 HEAD、attempt 日志/exit/项数/hash）→ D 文档批；失败保留、另编号，门禁后功能/测试变动须重门禁。
- C 报告须含 reclaimed/未确认 len/total 实值、独立复跑目录、四历史 targets 结果；SKIP→PARTIAL 且总结果非全 PASS。

边界：本审仅新增本文并用 edict CLI 留裁决、转 Assigned、validate；未改 crates/计划/其他文档，未 commit/push，未运行 cargo/`ci.ps1`。Assigned 表示附条件准入，功能绿与唯一门禁均未在本审验证。
