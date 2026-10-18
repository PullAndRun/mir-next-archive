# GM 命令（第 5 轮新增）

面向 admin / 运维的最小可用 GM 命令集。实现：`crates/mir-server/src/gm.rs`，
入口 `World::run_gm_command(object_id: u32, text: &str) -> bool`（返回 `true` = 这条聊天
已被命令消费，调用方不再走普通聊天）。

## 1. 权限与入口

* **权限门在会话层**：`crates/mir-server/src/session.rs` 的 `on_chat` 里
  `if message.starts_with('@') && self.is_gm()` 才会调用 `run_gm_command`；
  `run_gm_command` 自己**不查权限**（`World` 里没有账号信息）。
* **非 GM 的 `@...`**：与普通聊天完全一致——静默、不报错、也不泄露命令是否存在
  （Crystal `PlayerObject.Chat` 同样口径）。
* **判定** `gm::is_gm(account, config)`：`accounts.admin`（Crystal `AccountInfo.AdminAccount`）
  **或** 账号名在 `ServerConfig::gm_accounts` 名单里（大小写不敏感）。
* **配置**：`ServerConfig::gm_accounts: Vec<String>`，环境变量 **`MIR2_GM_ACCOUNTS`**
  （逗号分隔账号名；空白/重复忽略，大小写不敏感去重；默认空 = 只认 `accounts.admin`）。
  解析函数 `gm::parse_gm_accounts`。
* 命令本身以 `@` 开头、**全英文回复**；`@help` 就是权威命令列表。

## 2. 命令表

| 命令 | 参数校验 | 语义 |
|---|---|---|
| `@level <1-255>` | `1..=255`，越界回 `Usage:` | 设等级 → 按新等级重算 `max_experience`（`exp_table.exp_for_level`）→ `refresh_stats` → HP/MP 回满 → `S.LevelChanged` |
| `@exp <n>` | `n >= 0` | 设当前经验，按当前等级重算下一级上限并回 `S.LevelChanged` |
| `@gold <n>` | `n >= 0` | 累加金币（`gain_gold`，带 `S.GainedGold`） |
| `@credit <n>` | `n >= 0` | 累加元宝（`gain_credit`，带 `S.GainedCredit`） |
| `@item <名称\|索引> [数量]` | 数量 `>= 1`；先按索引、再精确名（大小写/空格不敏感）、最后唯一片段匹配 | 发到背包；放不下回 `Bag full or overweight: …` |
| `@map <名称\|索引> [x y]` | 坐标必须是整数；索引走 `MapInfo.index`，名称走 `MapInfo.file_name` | `place_player` 传送；不可进入回 `Cannot enter map '…'.` |
| `@heal` | — | 死亡时先 `revive_player`，否则 HP/MP 回满 |
| `@kill [角色名]` | 目标必须在线（缺省自己） | **最小死亡流程**：`HP=0`/`Dead=true`/`ReviveTime=now+PLAYER_REVIVE_DELAY_MS`、清毒/隐身、杀宠、`S.Death` + `S.ObjectDied` 广播；**不发死亡掉落、不中断交易**（差异，10 秒后自动复活） |
| `@who` | — | 逐行输出在线玩家：`  name lvN @map (#objectId)`，首行 `N player(s) online:` |
| `@kick <账号\|角色>` | 目标必须是已登记会话或在线角色 | 立刻断开（`server.rs` 读循环用 `select!` 监听踢线通道）；未登记会话标待踢，下一条包生效（`SessionError::Kicked`） |
| `@recall <角色名>` | 目标必须在线 | 把目标传送到自己身边；失败回 `Cannot move …` |
| `@mob <名称\|索引> [数量]` | 数量 `1..=50`（`MAX_SPAWN_BATCH`）；名称支持唯一片段匹配 | 在 GM 脚下 3×3 邻格轮流刷怪（不设归属、不计入刷怪点），回 `Spawned N x …`；未知怪回 `Monster not found` |
| `@clearitem` | — | 清掉 GM 当前地图的**地面物品与金币堆**，回 `Cleared N ground item(s) on <map>.` |
| `@revive [角色名]` | 目标必须在线且已死亡（缺省自己） | `revive_player`；未死亡回 `… is not dead.` |
| `@setpk <点数>` | `>= 0` | 设 PK 值并立即 `refresh_name_colour`（红/黄/棕名同步广播） |
| `@warp <角色> <地图> [x y]` | 目标在线；地图名/索引 + 可选整数坐标 | 把**别的**角色传送到指定地图（GM 自己用 `@map`），回 `Warped …` |
| `@mapinfo` | — | 当前地图/坐标/站立格可走性 + 本图怪物/玩家/地面物品数（多行） |
| `@clearbag` | — | 清空背包（**不碰装备栏/仓库**），逐件 `S.DeleteItem`，回 `Cleared N item stack(s) from the bag.` |
| `@reboot <秒>` | `0..=86400` | 安排优雅停机：广播倒计时（每个整秒只喊一次）→ 到点踢掉全部会话并停止 accept；`0` = 立即重启 |
| `@help` | — | 命令表（`help_lines()`） |

## 3. 错误与回执口径

* 非法参数 → `Usage: …`；
* 找不到物品 → `Item not found: '<query>'.`；
* 找不到怪物 → `Monster not found: '<query>'.`；
* 找不到玩家 → `Player not found: '<query>'.`；
* 找不到地图 → `Map not found: '<query>'.`；
* 未知命令 → `Unknown GM command '@x'. Type @help for the command list.`；
* 成功回执示例：`Level set to 42 (next level at 123456).`、`Gave 5 x 金创药(小量).`、
  `Kicked 'alice' (session 7).`

## 4. 审计

每条命令一条结构化日志（`tracing::info!`）：

```text
info!(gm = %gm_name, object_id, command = %command, args = ?args, "GM command")
```

参数里的敏感值（如 `@kick` 的账号名）会原样进日志，便于追责/排障。

## 5. 与 Crystal 的差异（有意）

1. **权限**：Crystal 只认 `AdminAccount`；本作额外支持 `MIR2_GM_ACCOUNTS` 运维名单（取或）。
2. **回复语言**：全英文（admin/ops 可见），**不进玩家 i18n 词表 / `TextKey` 统计**；
   非 GM 看不到这些文本。
3. **`@kill` 是最小死亡**：不发死亡掉落、不中断交易，10 秒后自动复活；
   Crystal 的 `@KILL` 走完整 `Die()`。
4. **`@level` 显式刷新属性与下一级经验**（Crystal `RefreshLevelStats` 同款行为，这里写明）。
5. **命令集**：12 条基础命令 + 8 条第 6 轮扩展（`@mob/@clearitem/@revive/@setpk/@warp/@mapinfo/@clearbag/@reboot`），
   共 **20 条**；Crystal 还有 `@MOVE/@GIVE/@SET/@CLEARBUFFS…` 未实现。
6. `@who` / `@mob` / `@clearitem` / `@mapinfo` / `@clearbag` / `@reboot` 是本作扩展
   （Crystal 无同名命令）；`@reboot` 的倒计时广播与优雅停机也是本作新增。

## 6. 宠物命令的 dispatch（同一轮顺带）

`session.rs` 在 `C.ChangePMode`（index 45）分支调 `world.set_pet_mode` 并无条件回
`S.ChangePMode`(63)（Crystal `MirConnection.cs:1441-1448`）。宠物模式语义见
[`docs/world/magic.md`](magic.md) §6.6。

## 7. 测试

* `crates/mir-server/tests/gm.rs`（23 条）：权限判定、未知命令、各命令成功/失败分支、
  `@mob/@clearitem/@mapinfo/@revive/@setpk/@warp/@clearbag/@reboot` 的新用例、
  `@kick` 待踢、`@who` 多行；
* `crates/mir-server/tests/reboot.rs`：`@reboot` 倒计时与优雅停机（踢线 + 停止 accept）；
* `crates/mir-server/tests/login_flow.rs` 的 GM e2e（登录后 `@help` / `@level`）。
