# 故障注入（第 22 轮）

## T1/T3/T2 池三场景

### T1 定向 terminate（application_name 隔离）
- 用例：`pg_terminate_disconnects_save_and_next_save_recovers`
- 连接串统一加 `application_name=mir2_fault_test`；只 terminate：
  `datname=current_database() AND application_name='mir2_fault_test' AND pid<>pg_backend_pid()`；
- 用例打印实际被 terminate 的 PID 列表与数量，证明未误伤其它会话；
- 结果：终止后 sqlx 自动换新连接；本用例另用 `pool().close()` 稳定复现第一笔失败，新 store 恢复后保存成功。

### T3 测试池打满
- 用例：`saturated_test_pool_waits_then_succeeds_after_release`
- 新增仅测试构造器 `PgAccountStore::connect_with_max_connections(url, 2)`；生产默认仍 16；
- 持 2 条后第 3 次 save 排队，释放后 8s 内成功，`gold=5150` 落库；
- 结果：等待而非永久失败。

### T2 本地代理断 30 秒
- 用例：`proxy_outage_fails_save_then_recovers_after_resume`
- `tools/dev/pg_proxy.py`：`127.0.0.1:15432 -> MIR2_TEST_DATABASE_URL`，控制文件 `pause`/`resume`；
- 服务端连 15432；断代理 30s 期间 autosave 失败、`db_save_failed_total` 增长、`db_saves_total` 不增长、客户端 TCP 未关闭；
- 恢复后 20s 内自动存盘成功（实测 `saves 1 -> 2`，总 33.68s）；
- **语义差异**：这是“网络不可达”，不等同 PG 进程停止；将来有一次性 PG 实例再补真停用例（待补非遗漏）。

## 判定
三场景均恢复 → **无需 pool 重建**：
- 生产代码无 `pool.close()` 调用；
- `pg_store` 只在启动建池，无重建路径；
- T1/T2/T3 均证明 sqlx 自动换连接/排队后恢复。
**无条件重建会掩盖真实 DB 故障，禁止。**

## RPO（第 22 轮 PARTIAL，顺延第 23 轮）

- **第 22 轮未产出可用测量**：独立 server 能启动、登录与 `@gold` 写均成功，但 RPO 测试的 `/metrics` 读取探针（raw socket / PowerShell / curl 三种）在测试进程内均返回 None；没有拿到 `T_save`，因此没有 `K/P/T_save`，不伪造、不把窗口外数据当结果。
- RPO 用例已落在 `fault_injection.rs::rpo_bounded_by_autosave_after_hard_kill`，当前 `#[ignore]`，避免红着冻结。
- 已规划 23:44/23:49 两次四项空载检查输出（无 cargo/rustc/mir-server、无 soak.lock）；但缺少有效 T_save，前置通过不等于 RPO 通过。

### 「测试进程读不到 /metrics」三条排查（第 22 轮）
1. **端口是否真在监听**：服务端子进程日志明确打印
   `metrics 端点已启动（GET /metrics） address=127.0.0.1:9095`，说明 listener 已 bind；
   外部 PowerShell `Invoke-WebRequest http://127.0.0.1:9095/metrics` 在同一台机可读到 15 个字段；
   但测试进程内的 raw socket / PowerShell / `C:\Windows\System32\curl.exe` 三种探针都返回 None。
   结论：**不是服务端未启动 metrics**，差异在测试进程上下文（子进程输出捕获/测试沙箱）或 raw HTTP 解析；下一轮应在测试里单独 `TcpStream::connect` 探 9095 并打印连接结果，以区分“连不上”与“HTTP 解析失败”。
2. **metrics 是否默认启**：`ServerConfig::metrics_listen` 默认 **`None` = 默认关闭**；只有显式设置 `MIR2_METRICS_LISTEN` 才启动。RPO spawn 已显式传 `127.0.0.1:9095`，日志也证明启动。
   **运维级发现**：不设置该变量时生产 `/metrics` 根本不存在（不是端口占用/防火墙），部署/soak 必须显式配置。
3. **Windows 防火墙**：`Get-NetFirewallProfile` 三个 profile 均 `Enabled=True`，Inbound 为 `NotConfigured`；没有匹配 mir/curl/rust 的显式入站规则。Loopback 一般不受防火墙影响，因此**不太可能**是根因，但下一轮连接探针需记录实际错误码。


**前置四项（执行时必须全部为空，并静置 5 分钟）**：
```powershell
Get-Process cargo,rustc,mir-server,soak-stability -ErrorAction SilentlyContinue |
  Select-Object Id,ProcessName,StartTime
Test-Path target/soak.lock            # 期望 False
# 静置 5 分钟后再查一次同样四条，输出贴回执
```

- 独立 server 进程 + 专用端口（7041+）+ 专用 metrics（9095）+ `MIR2_AUTOSAVE_SECS=1`；
- **seq 日志（第 23 轮 B）**：`save_character` 写库成功后分配进程级 `AtomicU64`，输出
  `db_save ok character=<id> seq=<n> t=<unix_ms>` 并 flush；`T_save` 取 **max(seq)** 行的 `t`，
  不能取文件尾行。测试 `crates/mir-server/tests/save_seq.rs` 覆盖 ≥8 并发（seq 唯一、t 非降）
  和模拟重启后 seq=1。
- **诊断记录有界**：`DB_SAVE_RECORDS` 是容量 1024 的 `VecDeque` 环形缓冲（生产构建也生效），
  满了丢最旧，内存恒定 ≈16KB；`pg_store` 单测 `db_save_records_are_bounded_and_drop_oldest`
  断言超限后首条 seq 前进、末条保留最新。
- 主判据：所有写入序号墙钟早于 `kill_time − 2s` 的写**都必须在 DB 中**；
- `loss ≤ 1` 仅在 K 次写间隔 ≥1s 时保留，否则删除该计数断言；
- **T_save 读法**：服务端 stdout/stderr 的 `db_save ok character=<id> seq=<n> t=<unix_ms>`；
  取 `seq` 最大的一条的 `t`，不是文件尾行；轮询/日志采集粒度需在回执写明。
- **边界**：`save_seq.rs` 覆盖 ≥8 并发（同进程 seq 唯一、按 seq 排序 t 非降）与模拟重启 seq=1；
  强杀后新进程 natural 从 1 重新开始。
- Windows 强杀记录 `Stop-Process -Id <pid> -Force`（= TerminateProcess，等价 SIGKILL）与 PID；
- **清理契约**：`try/finally` 内必须执行 `Stop-Process -Id <精确 PID> -Force`（服务端与任何指标/代理子进程）、删除临时目录、删除 pid 文件；任何断言失败也不得留下进程。
- 当前状态：**第 23 轮已实测通过**（空载窗口 03:30–03:45）：
  - `K=5` 次 `@gold 1`（间隔 1.1s），`P=5`，`loss=0`；
  - `T_save=1792525156961`（max(seq) 日志行），`kill_time=1792525158372`，
    `kill−T_save=1411ms ≤2000ms`；
  - 所有写入墙钟早于 `kill−2s` 的数量 =4，DB 持久化 ≥4（实际 5）；
  - `Stop-Process -Id <pid> -Force` 强杀，重启核验后无残留进程/端口/临时目录。
- 负向自证仍未执行（关 autosave 必须红）；列入第 24 轮。
- **stdout 日志发现**：`tracing_subscriber` 默认写 **stdout**；若部署/测试把 stdout 丢弃（`Stdio::null()`），
  `db_save ok ...` 等于不存在，RPO 无法计算 `T_save`。服务端启动模板必须把 stdout 重定向到文件/
  日志采集器，不能只捕获 stderr。

## 未完成（顺延第 24 轮）
- **RPO 负向自证**：关闭 autosave 后重跑 `rpo_bounded_by_autosave_after_hard_kill`，必须变红并贴输出；
  本轮因 04:10 的 4h soak 窗口未执行。
- 写失败负向自证（清重试计数）同样顺延第 24 轮。

## 已知边界
- 同一已关闭 pool 不自愈；生产不会主动 close。
- 指标用全局 `metrics()`；fault_pg 串行 `METRICS_LOCK` 并 abort 自己的 server task。
