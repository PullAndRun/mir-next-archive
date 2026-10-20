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
- 主判据：所有写入序号墙钟早于 `kill_time − 2s` 的写**都必须在 DB 中**；
- `loss ≤ 1` 仅在 K 次写间隔 ≥1s 时保留，否则删除该计数断言；
- `T_save` 尽量使用服务端实际落盘时间（metrics 的 `db_save_total` 增量时刻）；
- Windows 强杀记录 `Stop-Process -Id <pid> -Force`（= TerminateProcess，等价 SIGKILL）与 PID；
- **清理契约**：`try/finally` 内必须执行 `Stop-Process -Id <精确 PID> -Force`（服务端与任何指标/代理子进程）、删除临时目录、删除 pid 文件；任何断言失败也不得留下进程。
- 当前状态：**未执行**（等待工部停 31204 后的空载确认），本文件不写伪造结果。

## 负向自证
- 关 autosave → RPO 必须红：随 RPO 顺延；
- 清重试计数 → 写失败必须红：随 RPO 顺延。

## 已知边界
- 同一已关闭 pool 不自愈；生产不会主动 close。
- 指标用全局 `metrics()`；fault_pg 串行 `METRICS_LOCK` 并 abort 自己的 server task。
