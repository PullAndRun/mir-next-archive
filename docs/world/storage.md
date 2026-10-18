# 玩家仓库（Personal Storage）

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：
> * `Server/MirObjects/PlayerObject.cs`（`StoreItem` `:5376-5456`、`TakeBackItem` `:5457-5526`、
>   `SendStorage` `:8161-8181`、`CanAccessStorage` `:8208-8213`、仓库密码 `:8183-8206`）
> * `Server/MirObjects/NPC/NPCScript.cs`（`StorageKey = "[@STORAGE]"` `:55`、`ProcessSpecial` 的
>   `case StorageKey:` `:1002-1006`）
> * `Server/MirDatabase/AccountInfo.cs`（`Storage = new UserItem[80]` `:69`、读档 `:176-186`、
>   存档 `:241-248`、`ExpandStorage` `:266-275`、`IsValidStorageIndex` `:277-286`）
> * `Shared/Globals.cs`（`StorageGridSize = 80` `:19`、`DataRange = 16` `:37`）
> * 客户端：`Client/MirControls/MirItemCell.cs:1362-1372`（拖入仓库发 `C.StoreItem`）、
>   `Client/MirScenes/Dialogs/NPCDialogs.cs:2798`（`StorageDialog`）
>
> 本作实现：
> * `crates/mir-server/src/storage.rs`（常量、编解码、全部校验与发包）
> * `crates/mir-server/src/{world.rs,session.rs,npc.rs}`（`Player::storage`、包分发、`[@STORAGE]` 页面）
> * `crates/mir-server/src/{store.rs,pg_store.rs}`（`Character.storage` 往返）
> * `crates/mir-store/migrations/0001_init.sql:101`（`characters.storage BYTEA`，本作**没有**新增迁移）
> * 测试：`crates/mir-server/tests/storage_mail.rs`（26 个端到端用例，其中仓库 8 个；
>   另有 `storage.rs` 内 3 个单元测试）
>
> **其它 crate 未改动**：`mir-protocol` 里仓库相关的包（`C.StoreItem` / `C.TakeBackItem` /
> `S.StoreItem` / `S.TakeBackItem` / `S.UserStorage` / `S.NPCStorage`）都已存在。

---

## 1. 总览

| 主题 | Crystal | 本作 |
|---|---|---|
| 数据归属 | `AccountInfo.Storage`（**账号级**，同账号多角色共享，`AccountInfo.cs:69`） | `characters.storage`（**角色级**，每个角色一个仓库） |
| 格数 | `UserItem[80]`，可扩建到 160（`AccountInfo.cs:69`、`:266-275`） | 恒 80（`storage::STORAGE_SIZE`），不扩建 |
| 读写入口 | `PlayerObject.StoreItem` / `TakeBackItem` | `World::storage_store_item` / `storage_take_back_item` |
| 打开界面 | `NPCScript` 的 `[@STORAGE]` → `SendStorage()` + `S.NPCStorage` | `World::send_storage_panels`（`npc::open_npc_panel` 的 `STORAGE_KEY` 分支） |
| 列表下发 | `S.UserStorage{Storage}`，**每连接一次**（`Connection.StorageSent`） | `World::storage_sent: BTreeSet<对象 ID>`，登录后第一次打开时下发 |
| 仓库密码 | `AccountInfo.StoragePassword` + `Settings.RequireStoragePassword`（`PlayerObject.cs:8208-8213`） | **未做**（恒无密码，`CanAccessStorage` 恒真） |
| 仓库金币 | **不存在**（见 §6） | **不存在**（行会金币仓库在 `crate::guild_storage`） |
| 持久化 | `Server.MirADB` 的 `AccountInfo` 整块（`AccountInfo.cs:241-248`） | `characters.storage` 的 `UserItem[]` 字节块（`encode_slots` / `decode_storage`） |
| 负重刷新 | `RefreshBagWeight()`（`:5447`、`:5520`） | 不需要：本作负重即时算（`World::bag_weight`） |

---

## 2. 协议与包序

| 语义 | 客户端包 | 服务端包 | Crystal |
|---|---|---|---|
| 打开仓库 | `C.CallNPC{ObjectID, Key="[@STORAGE]"}`（50） | `S.UserStorage`（130，**每连接一次**）→ `S.NPCStorage`（110） | `NPCScript.cs:1002-1006`、`PlayerObject.cs:8161-8181` |
| 存物品 | `C.StoreItem{From=背包格, To=仓库格}`（15） | `S.StoreItem{From, To, Success}`（43） | `PlayerObject.cs:5376-5456` |
| 取物品 | `C.TakeBackItem{From=仓库格, To=背包格}`（16） | `S.TakeBackItem{From, To, Success}`（42） | `:5457-5526` |

包序要点：

* **打开仓库**：`S.UserStorage` 的负载是 `Option<Vec<Option<UserItem>>>`；第一次打开发
  「80 格完整数组」，之后同一连接再打开只发 `S.NPCStorage`（空包）。Crystal 用
  `MirConnection.StorageSent` 标记（`MirConnection.cs:70`、`PlayerObject.cs:8169-8170`），
  本作用 `World::storage_sent`，两者都随连接/对象销毁。
* **失败回执**：Crystal 每条拒绝分支都只 `Enqueue(p)`（`p.Success` 保持 `false`），
  **不发聊天**；本作照抄。测试里可以直接断言 `success == false`。
* **成功回执**：Crystal 在移动完成后发 `Success = true`（`:5451-5452`、`:5519-5521`）。

---

## 3. NPC 页面与访问校验

Crystal 的入口是 NPC 脚本里的 `[@STORAGE]` 页面（`NPCScript.cs:55`、`:1002-1006`）：

```text
[@Main]
#SAY
要存东西吗？
<仓库/@Storage> <关闭/@exit>

[@Storage]
#SAY
仓库
```

`storage::check_storage_access` 按 Crystal 的顺序校验（每条都对应一次
`Enqueue(p); return;`）：

1. 玩家在世界里、未死亡（本作补的 `Dead` 判定；Crystal 的 `C.StoreItem` 入口本身不查，
   但 NPC 对话要求活着）；
2. `player.npc.page_key == "[@STORAGE]"`（大小写不敏感，Crystal
   `String.Equals(..., CurrentCultureIgnoreCase)`，`PlayerObject.cs:5380`、`:5461`）；
3. `player.npc.npc_object_id` 指向的 NPC **在当前地图**上（`:5386-5391`、`:5466-5472`）；
4. 距离 ≤ `Globals.DataRange = 16`（`:5393`、`:5474`；本作复用 `npc::NPC_TALK_RANGE`）；
5. `CanAccessStorage()` —— 仓库密码，本作恒真（见 §10）。

> **页面必须真的存在**：`ProcessSpecial` 只在页面被处理时调用（`NPCScript.cs:895-900`）。
> 只有按钮指向 `@Storage`、脚本里没有 `[@Storage]` 页面时，`call_npc` 会走「脚本里没有这个页面」，
> 不会发 `S.UserStorage`/`S.NPCStorage`。

---

## 4. 存物品：`C.StoreItem`

`World::storage_store_item(player_id, from, to)` 逐条移植 `PlayerObject.StoreItem`
（`:5376-5456`）：

| # | 检查 | Crystal |
|---|---|---|
| 1 | 访问校验（§3） | `:5380-5403` |
| 2 | `from` 在背包范围（46 格） | `:5405-5409` |
| 3 | `to` 在仓库范围（80 格） | `:5411-5415` |
| 4 | `Account.IsValidStorageIndex(to)` | `:5417-5421`（本作等价于第 3 条，见 §5） |
| 5 | 背包格有物品 | `:5423-5429` |
| 6 | 模板 `BindMode.DontStore` / 租借 `DontStore` | `:5431-5441` |
| 7 | 目标仓库格为空 | `:5443-5455` |
| 8 | 移动 + `RefreshBagWeight()` | `:5445-5447` |

成功后 `S.StoreItem{From, To, Success=true}`。

**模板缺失时放行**：Crystal 的 `Item.Info` 不可能为 `null`；本作老存档里可能有未知模板，
`cannot_store` 对查不到模板的物品返回「不限制」，避免它们永远卡在仓库外
（与 `guild_storage::cannot_store` 同一策略）。

---

## 5. 取物品与容量

`World::storage_take_back_item(player_id, from, to)` 移植 `PlayerObject.TakeBackItem`
（`:5457-5526`）：访问校验 → 仓库下标 → 背包下标 → 仓库格有物品 → **背包目标格为空**
（`:5512-5525`）→ 移动。

本作**没有**「取到任意空格」的语义：Crystal 也是指定目标格，客户端负责选格。

### 容量

* 默认/上限都是 `STORAGE_SIZE = 80`（`AccountInfo.cs:69` / `Globals.cs:19`）。
* `IsValidStorageIndex`（`AccountInfo.cs:277-286`）在未扩建时只允许 `0..80`，扩建后允许
  `0..160`；本作不扩建，所以 `to >= 80` 一律拒绝（`StorageError::StorageIndex`）。
* **112 是行会仓库的格数**（`GuildInfo.StoredItems = new GuildStorageItem[112]`，
  `Server/MirDatabase/GuildInfo.cs:22`），不是个人仓库。
* 存档解码允许读到 160 格（`decode_slots_with_limit`），再裁到 80 并记日志——
  这样「在别的服务端扩建过仓库」的存档不会让登录直接失败。

**「仓库满了」在 Crystal 里不会出现单独的报错**：客户端按目标格存放，目标格占用即失败
（§4 第 7 条）。测试 `store_item_rejects_out_of_range_and_occupied_slots` 覆盖了
「越界」与「目标格占用」两种形态。

---

## 6. 金币存取：Crystal 没有这个功能

任务书里提到 `C.DepositGold{amount}` / `C.TakeBackGold{amount}` / `S.StorageGold`，
但**当前 Crystal 分支里不存在**：

* `Shared/ClientPackets.cs` / `Shared/ServerPackets.cs` 里没有 `DepositGold`、`TakeBackGold`、
  `StorageGold` 任何一个类（全仓库 grep 0 命中）；
* `AccountInfo` 只有 `Gold`（账号钱包，`AccountInfo.cs:72`），没有「仓库金币」字段；
* 客户端 `StorageDialog`（`Client/MirScenes/Dialogs/NPCDialogs.cs:2798`）只有物品格子，
  没有金币输入；
* 个人仓库的 `StoreItem`/`TakeBackItem` 也只处理 `UserItem`。

金币仓库是**行会**才有的：`C.GuildStorageGoldChange`（`Shared/ClientPackets.cs:1802`）→
`GuildStorageGoldChange`（`PlayerObject.cs:10061-10119`），本作已在
`crate::guild_storage` 实现（见 `docs/world/guild.md` §7）。

因此本作**不发明协议**：`mir-protocol` 没有改动，个人仓库存取金币没有实现。
`docs/roadmap.md` 把这一点写成「个人仓库没有金币功能」。

---

## 7. 持久化

### 字节块格式

`characters.storage` 是 Crystal `UserItem[]` 的整块格式（与背包/装备同一套）：

```text
i32 count
per slot: bool 有物品？ + （有则一个完整 UserItem）
```

* 编码：`storage::encode_storage` = `items::encode_slots`（`CharacterInfo.cs:431-447` 的同一格式；
  Crystal 仓库存档在 `AccountInfo.cs:241-248`）；
* 解码：`storage::decode_storage`，空块得到 80 格空仓库；最多允许 160 格，裁到 80；
* 登录：`session::on_start_game` 解码到 `Player::storage`；
* 下线：`session::leave_world` 编码回 `Character.storage` 并整行保存
  （`mir-store::save_character` 会写 `storage` 列，见 `crates/mir-store/src/repo.rs:313`）。

### 与 Crystal 的差异

Crystal 把仓库放在**账号**上（同账号角色共享），本作按任务要求放在**角色行**上
（`migrations/0001_init.sql:101` 的列注释已注明）。这带来两个可观察差异：

1. 同账号的角色 A 存的东西，角色 B 看不到；
2. 账号内多角色不会共享 80 格。

如果将来要恢复账号级语义，需要新增 `accounts.storage` 并把 `AccountStore` 的仓库函数
移到账号维度；本次没有做。

---

## 8. 登录/下线往返（测试覆盖）

`crates/mir-server/tests/storage_mail.rs::session_login_round_trips_storage` 用真实
`Session::handle` 走：

```text
ClientVersion → Login → StartGame → 存物品 → LogOut → StartGame
```

断言：`MemoryStore.character_storage` 的字节块非空、解码后第 12 格是刚存的物品、
重登后 `Player.storage[12]` 仍在、长度 80。

---

## 9. 测试清单（仓库部分）

| # | 用例 | 覆盖点 |
|---|---|---|
| 1 | `store_item_moves_into_storage_and_acks` | 成功回执、背包清空、仓库格内容、80 格 |
| 2 | `store_item_requires_storage_dialog_npc_and_range` | 无对话 / 错误页面 / 距离太远 / NPC 不存在；失败回执 |
| 3 | `store_item_rejects_out_of_range_and_occupied_slots` | `to=80/200`、目标格占用、`from=46`、正数控制 |
| 4 | `store_item_rejects_bound_items` | `BindMode.DontStore` 拒绝且物品留在背包 |
| 5 | `take_back_item_returns_to_inventory_and_rejects_full_bag` | 取回成功回执、背包目标格占用、空仓库格 |
| 6 | `storage_page_sends_user_storage_once_then_only_npc_storage` | `S.UserStorage` 只发一次、`S.NPCStorage` 每次都发 |
| 7 | `npc_pages_open_storage_and_mail_panels` | `[@Storage]`/`[@SendParcel]`/`[@CollectParcel]` 页面接线 |
| 8 | `session_login_round_trips_storage` | 下线/重登的字节块往返 |
| — | `storage::tests::*`（3 个单元测试） | 空块、往返、坏块（不在上述 26 个之内） |

---

## 10. 未做 / 明确不做

* **仓库扩容道具**（`AccountInfo.ExpandStorage`，`:266-275`；商城卖「扩充仓库」）——不做。
* **仓库密码**（`AccountInfo.StoragePassword`、`C.UnlockStorage`/`C.SetStoragePassword`/
  `C.RemoveStoragePassword`，`MirConnection.cs:894-1006`）——不做；因此
  `S.UserInformation` 的 `has_storage_password`/`require_storage_password` 恒 `false`，
  `storage_password_last_set`/`expanded_storage_expiry_time` 恒 0。
* **账号级仓库**：本作是角色级（§7）。
* **金币存取**：Crystal 没有（§6）。
* **`CheckItem` / `S.NewItemInfo`**：Crystal 在 `SendStorage` 里会给仓库物品补发模板信息
  （`PlayerObject.cs:8172-8178`）；本作没有「按需下发物品模板」的通道，只记 `debug!`。
  客户端启动时自己加载全部模板。
* **`RefreshBagWeight`**：本作负重即时计算，不需要缓存刷新。
* **仓库物品过期 / 自动清理**：Crystal 只有 `ServerTextKeys.ItemExpiredFromStorage`
  文案（`Shared/Language.cs` 的 `ServerTextKeys`），没有实现逻辑；本作也不做。

---

## 11. Crystal 异常行为（照抄或登记）

1. **仓库是账号级**：`AccountInfo.Storage`（`AccountInfo.cs:69`），不是角色级。本作按任务
   改成角色级，属于**有意差异**。
2. **仓库列表每连接只发一次**：`Connection.StorageSent` 在 `SendStorage` 里置真
   （`PlayerObject.cs:8169-8170`），**没有**「关窗后重置」；同一次登录里重复打开仓库时，
   客户端拿到的仓库内容可能是旧的（本作照抄这个节流）。
3. **失败无提示**：所有拒绝分支只有 `Success=false` 的包，没有聊天。
4. **`IsValidStorageIndex` 与长度检查重复**：未扩建时 `to >= 80` 已被
   `to >= Account.Storage.Length`（`:5411`）拦下，`IsValidStorageIndex` 只在
   「`Storage.Length == 160` 但没有 `HasExpandedStorage`」这类不一致存档里才起作用。
5. **NPC 查找取第一个匹配**：Crystal 在 `CurrentMap.NPCs` 里线性找 `ObjectID`
   （`:5386-5391`），同 ID 重复时用第一个；本作 `iter().find` 相同。
6. **`DataRange` 是 16**：注释写着「Was 24」（`Shared/Globals.cs:37`）；老客户端/攻略里的
   24 格不适用。
7. **`[@STORAGE]` 大小写不敏感**：Crystal 用 `CurrentCultureIgnoreCase` 比较页面 key
   （`:5380`），本作用 `eq_ignore_ascii_case`；脚本页面本身在解析时已被规范成大写
   （`mir-db::npc_script::normalize_key`），实际不会出现差异。
8. **金币存款包不存在**：任务书里的 `C.DepositGold`/`C.TakeBackGold`/`S.StorageGold`
   在当前 Crystal 分支里**没有**（§6），个人仓库没有金币功能；行会仓库金币是另一套包。
