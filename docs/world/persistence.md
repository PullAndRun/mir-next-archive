# 存档与崩溃一致性（`persistence`，第 9 轮）

本文件说明本作的写盘时机、周期自动保存（autosave）与崩溃一致性的 RPO 口径。

## 1. 为什么需要 autosave

Crystal 只在 `PlayerObject.LogOut` 把角色写回 `Characters`（`PlayerObject.cs:419-420`），
进程崩溃 = 回档到登录时刻；本作的 **周期 autosave 是对 Crystal 的扩展**，把崩溃丢失窗口
从「整段在线时长」压到 **≤ `autosave_secs + 1` 秒**。

## 2. 写盘时机总表

| 状态 | 写库时机 | 崩溃窗口 |
|---|---|---|
| 角色整行（背包/金币/装备/技能/任务/任务物品栏/仓库/好友/宠物模式/坐标/HP/MP/经验） | **周期 autosave（默认 30s，只写脏角色）**、首次进世界补属性、每日重置命中、`C.LogOut`、TCP 断开、market/gameshop 事务 | **≤ autosave_secs + 1s（默认 31 秒）** |
| 行会 / 攻城 | dirty → server tick（100ms）；会话动作/下线补写；失败回灌 + 5s 重试闸门 | ≤100ms（失败重试） |
| 邮件 | 每次邮件动作 + 下线补写；失败回灌 + 5s 重试 | 动作后即写 |
| 寄售/拍卖 | 上架/买断/取回/到期成交事务；tick 到期扫描 + 失败 5s 重试 | 动作后即写，有重试 |
| 商城元宝/已购计数 | 每次购买同一事务 | 购买后即写 |
| 账号语言 | 切换立即写 | 立即 |

## 3. autosave 语义与 RPO

* **配置**：`ServerConfig.autosave_secs` / 环境变量 `MIR2_AUTOSAVE_SECS`，默认 **30**；
  `0` = **关闭**（关闭后只在 `LogOut`/断线/市场事务里落盘，退回 Crystal 语义，RPO 无上限）；
* **触发**：服务端会话读循环每 **250ms** 轮询一次，用**世界时钟**（`World::wall_now()`）
  判定是否到点 → `Session::autosave_tick()`；
* **只写脏角色**：脏判定 =「当前世界快照 != 上次成功写库的角色行」（整行 `Character: PartialEq`，
  覆盖背包/装备/金币/元宝/技能/任务/仓库/好友/坐标/HP/MP）；在线但没改动的玩家**一次 IO 都不做**；
* **RPO**：`autosave_secs + 1 秒`（默认 **≤31 秒**；+1s 是 250ms 轮询粒度的上界）；
* **退出不丢**：`C.LogOut` 与 TCP 断线仍走「离开世界 → 立即保存」，与之前一致；
* 写盘直接复用 `Character` 整行 UPDATE（与市场/商城事务同一套 `save_character`）。

### 3.1 并发写：每角色锁 + 版本号（t111）

* **锁/队列**：每个角色一把 `Arc<tokio::sync::Mutex>`（`MemoryStore`/`PgStore` 内
  `save_locks`）——同一角色同时只有一个写，不同角色并行；
* **版本语义**：session 的 `save_version = max(Unix ms, last + 1)`；store 按角色记录
  **最后成功版本**，只接受**严格更大**的版本；旧快照返回 `Ok(false)` 直接丢弃；
  失败不推进版本、可重试；整行仍是**单条 UPDATE 原子**；
* **RPO 验证**：`autosave_secs=1` 时，+1.1s 后读到最新 777 金币，仍满足
  **≤ autosave_secs + 1 秒**。

## 4. 写失败语义与重试

* 失败：`tracing::error!`（字段 `session` / `character` / `failures` / `backoff_ms` / `error`），
  并**保留脏标记**（不推进基准行）；
* 退避曲线：**1s → 2s → 4s → 8s → 16s → 30s（上限 30s）**，成功后退避与失败计数清零；
* **不丢批**：行会/邮件的 flush 失败会把批次**回灌到世界队列队首**（期间新追加的排在其后；
  `Save` 是整行覆盖，后写更新，顺序保序），另有 **5 秒重试闸门**避免 DB 故障时按 100ms tick 捶库；
  服务端 100ms tick 里的行会落盘失败同样回灌；
* Store 层失败返回 `Err`、旧行不变、调用方内存值不被改写。

## 5. 指标与日志

`/metrics`（Prometheus 文本，见 [`docs/server/metrics.md`](../server/metrics.md)）：

| 指标 | 含义 |
|---|---|
| `mir2_db_save_failed_total` | 角色落盘失败次数（含 autosave 与下线存盘） |
| `mir2_autosave_total` | 周期自动存档**实际写库**次数（快照没变跳过的不计） |
| `mir2_batch_write_failed_total` | 行会/邮件批次写库失败次数（失败即回灌重试） |
| `mir2_autosave_failures_total` | 与 `mir2_db_save_failed_total` 同值的语义别名 |

30 秒汇总日志（`metrics summary`）新增 `db_save_failed=` / `autosaves=` 两个字段；
完整字段见 metrics.md §5。

## 6. 测试证据（`crates/mir-server/tests/persistence_audit.rs`，**18/18**）

基础 10 条：LogOut 与直接断 TCP 后重连金币一致、只恢复到上次成功写库、并发保存串行化、
写失败旧行保持、装备/背包/技能/仓库/任务/攻城/行会字段覆盖等。

autosave/版本相关（t111 新增 6 条）：旧版本拒绝、并发时最新版本胜出、同版本仅一个成功、
失败后可重试、autosave 与 LogOut 的版本顺序、RPO 断言（`autosave_secs=1` + 1.1s 读到最新值）。

* `autosave_bounds_the_crash_window_to_autosave_secs`：用注入世界 + `World::advance_clock`
  快进（**不 sleep**），断言 autosave 到点前库里是旧值 → 推进 1 秒后新金币已入库，
  且会话仍连着（= 进程此刻被杀后的库内容）；
* 「只写脏角色」：未改动玩家不产生写库；
* 「失败后重试成功」：注入失败 → 计数/退避 → 恢复后同一条脏行写成功。

## 7. 平滑重启（第 16 轮 t123）

* **强制存盘**：停机倒计时到点后，每个在线会话先 `force_save_for_shutdown`——完整角色行
  + 好友/婚姻师徒/任务 + 市场/行会/邮件 flush；失败记录并**重试一次**，再失败告警；
* **倒计时**：**60 / 30 / 10 秒**三个节点广播，最后每秒广播；`TextKey::RebootCountdown`
  （`reboot_countdown`，参数 `{seconds}`）中英双语；
* **重连宽限**：`RECONNECT_GRACE_MS = 90 秒`；停机时登记「账号 → 最后角色」，重启后 90 秒内
  登录自动 `StartGame` 恢复（Crystal StartGame 成功 result=4），超期走正常选角；
* 测试：`tests/reboot.rs` **6/6**。

## 8. 与其它文档

* 表结构/字段：`docs/store/schema.md` §8（RPO 已由本文件的 autosave 兜底）；
* 指标端点：`docs/server/metrics.md`；
* 市场/商城事务：`docs/world/market.md` §9、`docs/world/gameshop.md`。
