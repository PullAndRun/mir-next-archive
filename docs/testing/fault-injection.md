# 故障注入（第 21 轮）

## 已验证

### Redis 无依赖
- 用例：`redis_absent_does_not_break_server_startup_or_protocol_handshake`
- 真 `TcpStream`：启动 `serve_forever`（MemoryStore）→ `Connected` → `ClientVersion` 握手；不依赖 `REDIS_URL`。

### 写失败不丢数据
- 用例：`save_failure_keeps_old_row_and_next_success_persists_new_row`
- `MemoryStore::fail_next_save_character` 注入失败：旧行 111 保留；内存脏快照 999 未丢；下一次成功写后 `DB == 内存快照`。

### 场景 A：瞬时 terminate 对玩家透明
- 用例：`transient_terminate_is_transparent_and_metrics_show_success_not_failure`
- 真 `serve_forever` + `PgAccountStore` + 真 TCP 登录；`pg_terminate_backend` 定向断开本库其它连接后 `Walk` 触发 autosave；
- 断言：`db_saves_total` 继续增长、`db_save_failed_total` **不变**、客户端 TCP 未关闭（500ms 读超时视为仍存活）。
- 结论：瞬时断连对业务透明，sqlx 自动换新连接，不应误记失败。

### 场景 B：持续故障有界且可恢复
- 用例：`sustained_pool_close_records_failure_and_reconnects_new_store`
- `pool().close().await` 后 `Walk` 触发 autosave；
- 断言：`db_save_failed_total` **+1**（实测 0→1）、`db_saves_total` 不变、客户端 TCP 未关闭；
- 新 `PgAccountStore::connect` 恢复后保存成功并读回，`DB == 内存快照`（gold=4242）。

### 已知边界 / 生产判断
- 同一已关闭 pool 不会自愈（`attempted to acquire a connection on a closed pool`）；场景 B 的“自愈”定义为重连新 store。
- **生产风险（第 22 轮单独查）**：`pg_store` 只在启动时建池；若运行期池被判定为坏并 `close()`，当前没有重建路径，可能导致永久无法恢复。生产代码不会主动 close 池，但需要专项确认/补重建。
- 指标读全局 `metrics()`；场景 A/B 用 `METRICS_LOCK` 串行，并在用例结束 abort 自己的 server task，避免并行污染。

## 未完成（顺延第 22 轮）
- 强杀 RPO（`Stop-Process -Force` + 双断言 `loss≤1` / `kill−T_save≤2s`）
- 负向自证①：关 autosave 后 RPO 用例必须变红
- 负向自证②：清零重试计数后写失败用例必须变红
