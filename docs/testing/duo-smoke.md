# 双人联机冒烟（`tests/duo_smoke.rs`，第 8 轮新增）

目的：用**两个真实 TCP 客户端**在同一个进程内服务端上把「可玩闭环」走一遍，
覆盖单人测试看不到的「另一个玩家视角」与包序。实现见
`crates/mir-server/tests/duo_smoke.rs`。

## 1. 运行方式

```powershell
cargo test -p mir-server --test duo_smoke -- --nocapture
```

* 每条测试自己 `boot()` 一个服务端（临时端口 + `MemoryStore`），互不影响；
* 基础配置：`load_game_data = false`、`idle_timeout_secs = 0`、
  连接/包限流全 0（避免测试环境触发抗刷）、不启 metrics；
* 物品/重登类用例需要真实 `Server.MirDB`：
  `resource/community/clgforever910-mir2database/Daneo1989/Server.MirDB`
  （缺失时该用例 `eprintln!` 明示跳过，不伪装通过）；
* 不依赖 PostgreSQL（`MemoryStore` 内存存档），也不需要先起独立服务端。

## 2. 场景矩阵

| 测试 | 覆盖 | 关键断言 |
|---|---|---|
| `duo_players_see_each_other_and_walks_sync` | 双人进世界互相可见 + 走位同步 | 双方都收到对方的 `S.ObjectPlayer`；走一步后对方收到 `S.ObjectWalk`/位置更新 |
| `duo_group_invite_and_membership_sync` | 组队邀请/接受 | 双方 `S.AddMember`/`S.GroupMembersMap` 成员表一致 |
| `duo_group_experience_is_shared_in_range_only` | 组队经验共享 | 组队（A 10 级 / B 5 级）后 A 杀怪：**双方**都收到 `S.GainExperience`，且 A 份额 > B（等级比例 + `PARTY_EXP_RATE` 人数加成） |
| `duo_group_experience_skips_member_out_of_range` | 组队经验距离限制 | B 走出 17 格（>`GROUP_EXP_RANGE = 16`）后杀怪：只有 A 收到经验 |
| `duo_trade_handshake_and_gold_transfer` | 交易握手 + 金币 | `TradeRequest`/`TradeAccept`/放金币/两段确认 → 成交后 A 的金币转到 B；**物品存取/成交**由 `tests/trade.rs` 的 `MoveItem{Grid=Trade}` 用例覆盖 |
| `duo_pk_damages_and_kills_then_gm_revive` | PK 与 GM 复活 | 改攻击模式 → 打人掉血 → 死亡广播 → `@revive` 复活（广播包序见 §4） |
| `duo_disconnect_removes_avatar_and_relogin_keeps_state` | 掉线/重登 | 断线后对方看到角色消失；重登后等级/背包/位置一致 |

**约定**：动作有服务端冷却（走位 600ms、攻击 550ms）时用「重发直到收到目标包 + 超时」
的确定性重试，不用固定 `sleep` 赌时序。

## 3. 组队经验共享（已验证）

`World::win_experience` 在击杀时按 `group_exp_shares` 给**同图 `GROUP_EXP_RANGE = 16`
格内**、活着的队员按等级比例分经验，并乘人数加成 `PARTY_EXP_RATE`（Crystal
`ProcessExperience` 的 `Globals.DataRange`/`ExpRate` 口径）；`monster.rs` 击杀调的就是它。
上述两条 e2e 用例分别验证「范围内分摊（高等级份额更大）」与「超出 16 格不分」。

## 4. 第 8 轮补齐的链路（均已实现）

* **复活广播**（`world.rs::revive_player`）：旁观点包序
  `S.ObjectRemove` → `S.ObjectRevived{effect=true}` → `S.ObjectAppearance`；本人
  `S.ObjectRevived{effect=true}` → `S.Revived{}`（Crystal `HumanObject.Revive`、
  `PlayerObject.cs:1443` 含本人）。测试
  `tests/trade.rs::revive_broadcasts_object_revived_to_observer_and_self`；
* **交易放物品**（`C.MoveItem{Grid=Trade}`，`items.rs::move_item` 新增分支，复用
  `trade.rs::trade_deposit_item`/`trade_retrieve_item`）：协议无方向字段，`from` 在背包
  有物品即存入、否则按取回；存入成功本人收 `S.DepositTradeItem{success=true}`、对方收
  `S.TradeItem`（整份交易格）；取回成功本人收 `S.RetrieveTradeItem`、对方收
  `S.TradeItem`；成交沿用 `trade_confirm`。测试
  `tests/trade.rs::move_item_trade_grid_deposits_retrieves_and_completes_trade`。
  duo_smoke 的交易用例仍只走**握手 + 单向金币**（物品路径由 trade.rs 覆盖）。

## 5. 其它范围

* 双人冒烟不含 NPC 商店/邮件/市场等需要更多前置状态的链路（各有单独的集成测试）。

## 6. 与其它测试的关系

| 层级 | 位置 | 说明 |
|---|---|---|
| 单元/模块 | 各 crate 的 `#[cfg(test)]` | 纯函数与编解码 |
| 单客户端集成 | `crates/mir-server/tests/*.rs` | 包序、状态机、存档 |
| 双人冒烟 | `tests/duo_smoke.rs` | 本文件：真实 socket + 两个玩家视角 |
| 客户端无窗口 | `crates/mir-client/tests/*.rs` | UI 快照/命中/面板 |

双人冒烟是「服务端协议 + 两个会话」这一层的最后一道，不替代刑部的全量 workspace 验收。
