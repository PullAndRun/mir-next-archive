# 玩家间交易（Trade）

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：
> * `Server/MirObjects/PlayerObject.cs` 的 `#region Trading`（`:10528-10960`：`DepositTradeItem`
>   `:10530-10581`、`RetrieveTradeItem` `:10582-10619`、`TradeRequest` `:10623-10707`、
>   `TradeReply` `:10708-10743`、`TradeGold` `:10744-10760`、`TradeItem` `:10761-10777`、
>   `TradeUnlock` `:10779-10787`、`TradeConfirm` `:10789-10888`、`TradeCancel` `:10889-10958`）
> * `Server/MirDatabase/CharacterInfo.cs`（`Trade = new UserItem[10]` `:75`；`AllowTrade` `:62`，
>   读档 `:258`、存档 `:485`）
> * `Server/Settings.cs`（`TradeDelay = 2000` `:337`，读 INI `:496`、写 INI `:774`）
> * `Shared/Globals.cs:37`（`DataRange = 16`）
> * `Server/MirNetwork/MirConnection.cs`（包分发 `:367-371`、`:439-440`、`:589-602`；
>   处理器 `:1272-1283`、`:1448-1452`、`:1861-1889`）
> * `Shared/ClientPackets.cs`（`DepositTradeItem` `:454-469`、`RetrieveTradeItem` `:471-486`、
>   `ChangeTrade` `:786`、`TradeReply` `:1413-1426`、`TradeRequest` `:1427-1436`、
>   `TradeGold` `:1438-1451`、`TradeConfirm` `:1452-1465`、`TradeCancel` `:1466-1476`）
> * `Shared/ServerPackets.cs`（`DepositTradeItem` `:1541`、`RetrieveTradeItem` `:1565`、
>   `TradeRequest` `:1896-1909`、`TradeAccept` `:1910-1924`、`TradeGold` `:1925-1943`、
>   `TradeItem` `:1944-1973`、`TradeConfirm` `:1974-1988`、`TradeCancel` `:1989-2004`）
> * `Client/MirScenes/GameScene.cs`（交易窗口：`ChangeLockState` `:2804-2830`、`:6312-6320`，
>   `TradeCancel{Unlock=true}` 只解锁 `:6323-6338`）
>
> 本作实现：
> * `crates/mir-server/src/trade.rs`（`TradeState`、全部交易流程、`CanGainItems`/`CanGainGold`）
> * `crates/mir-server/src/world.rs`（`Player::trade` / `trade_invitation` / `next_trade_time` /
>   `allow_trade`；`World::leave` / `walk` / `run` / `turn` / `player_died` 五处中止挂点）
> * `crates/mir-server/src/maps.rs`（`World::change_map` 里切图中止交易）
> * `crates/mir-server/src/session.rs`（包分派 `:314-330`，处理器 `:1809-1908`）
> * `crates/mir-server/src/mail.rs`（取消交易时背包满 → `World::mail_item_to_self`）
> * 测试：`crates/mir-server/tests/trade.rs`（28 个端到端用例，另有 `trade.rs` 内 3 个单元测试）
>
> **`mir-protocol` 没有改动**：`C.TradeRequest` / `C.TradeReply` / `C.DepositTradeItem` /
> `C.RetrieveTradeItem` / `C.TradeGold` / `C.TradeConfirm` / `C.TradeCancel` / `C.ChangeTrade`
> 与 `S.TradeRequest` / `S.TradeAccept` / `S.TradeGold` / `S.TradeItem` / `S.TradeConfirm` /
> `S.TradeCancel` / `S.DepositTradeItem` / `S.RetrieveTradeItem` 都已存在。

---

## 1. 总览

| 主题 | Crystal | 本作 |
|---|---|---|
| 交易格 | `CharacterInfo.Trade`，`UserItem[10]`（`CharacterInfo.cs:75`） | `TradeState::items: Vec<Option<UserItem>>`，固定 `TRADE_SLOTS = 10` |
| 对方 | `PlayerObject.TradePartner`（`PlayerObject.cs:177`） | `TradeState::partner: Option<对象 ID>` |
| 「确认」 | `PlayerObject.TradeLocked`（`:178`） | `TradeState::locked` |
| 交易金币 | `PlayerObject.TradeGoldAmount`（`:179`，`uint`） | `TradeState::gold: u32` |
| 待回应邀请 | `PlayerObject.TradeInvitation`（`:175`） | `Player::trade_invitation: Option<对象 ID>` |
| 允许被交易 | `Info.AllowTrade`（`CharacterInfo.cs:62`，**随角色存档**） | `Player::allow_trade`（运行期，进世界默认 `true`，**不落盘**） |
| 发起节流 | `NextTradeTime` + `Settings.TradeDelay = 2000`（`:10625-10626`） | `World::next_trade_time` + `TRADE_DELAY_MS` |
| 距离 | `Globals.DataRange = 16`（`Globals.cs:37`） | `TRADE_RANGE = 16`（复用 `Globals.DataRange`） |
| 发起目标 | 面朝格 `Functions.PointMove(CurrentLocation, Direction, 1)`（`:10634`） | 面朝格；一格多人时取对象 ID 最大者（Crystal 取格子对象列表里最后一个玩家） |
| 状态生命周期 | 临时状态；下线/切图/移动/转身都会 `TradeCancel` | 同左，另加「死亡即中止」扩展 |
| 持久化 | `Trade` 数组随 `CharacterInfo` 序列化，但正常下线前已被清空 | 完全运行期，不写库 |

**交易没有「确认后自动关闭」之外的超时**：Crystal 不给 `TradePartner` 加计时器，双方挂着交易窗口不动也不会
自动取消；只有走/跑/转身/切图/下线/死亡等动作会中止。

---

## 2. 协议与包序

| 语义 | 客户端包 | 服务端包 | Crystal |
|---|---|---|---|
| 发起 | `C.TradeRequest{}`（96，**没有目标名**） | 目标 ← `S.TradeRequest{Name=发起者}`（192） | `MirConnection.cs:1861-1865`、`PlayerObject.cs:10704-10705` |
| 回应 | `C.TradeReply{AcceptInvite}`（97） | 接受：双方 ← `S.TradeAccept{Name=对方}`（193）；拒绝：只给邀请者一句系统聊天 | `PlayerObject.cs:10708-10743` |
| 放物品 | `C.DepositTradeItem{From,To}`（30） | 本人 ← `S.DepositTradeItem{From,To,Success}`（50）；成功时对方 ← `S.TradeItem{TradeItems}`（195，整份 10 格） | `PlayerObject.cs:10530-10581` |
| 取回 | `C.RetrieveTradeItem{From,To}`（31） | 成功时对方 ← `S.TradeItem`；本人 ← `S.RetrieveTradeItem{From,To,Success}`（51） | `PlayerObject.cs:10582-10619` |
| 放金币 | `C.TradeGold{Amount}`（98） | 本人 ← `S.LoseGold{Gold=Amount}`（68）；对方 ← `S.TradeGold{Amount=对方累计额}`（194） | `PlayerObject.cs:10744-10760` |
| 确认 | `C.TradeConfirm{Locked}`（99） | 单方确认：对方 ← 系统聊天「某某正在等待你确认交易。」；成交：逐件 `S.GainedItem`（66）/`S.GainedGold`（67）→ 双方各一句 `S.Chat`「交易成功。」→ `S.TradeConfirm{}`（196） | `PlayerObject.cs:10789-10888` |
| 取消/中止 | `C.TradeCancel{}`（100） | 双方各自：逐件 `S.RetrieveTradeItem{Success=true}`（51）→ `S.GainedGold`（67，若有金币）→ `S.TradeCancel{Unlock=false}`（197） | `PlayerObject.cs:10889-10958` |
| 允许被交易 | `C.ChangeTrade{AllowTrade}`（46） | 无回包（只改 `Info.AllowTrade`） | `MirConnection.cs:1448-1452` |

### 2.1 放进交易格的精确包序

`PlayerObject.DepositTradeItem(from, to)`（`:10530-10581`）：

1. 下标越界（`from >= Inventory.Length` 或 `to >= Trade.Length`）→ `S.DepositTradeItem{false}`；
2. 背包格为空 → 同上；
3. 模板 `BindMode.DontTrade` → 同上；
4. 租借绑定 `RentalInformation.BindingFlags` 含 `DontTrade` → 同上；
5. 目标交易格已占用 → 同上；
6. 移动物品 → `RefreshBagWeight()` → `TradeItem()`（先给**对方**发 `S.TradeItem`，同时清掉双方
   「确认」）→ `p.Success = true; Enqueue(p)`（本人收成功回执）。

服务端内部顺序是「对方的 `S.TradeItem` 先入队，本人的回执后入队」；本作 `trade_deposit_item`
照抄这个顺序（`trade_items_changed` 在 `send_deposit_ack` 之前）。

### 2.2 取回与放金币

* **取回**（`:10582-10619`）：下标越界 → 交易格为空 → 目标背包格已占用 → 失败回执；
  成功则移动、`p.Success = true`、`RefreshBagWeight()`、`TradeItem()`（对方收 `S.TradeItem`）、
  最后本人才收 `S.RetrieveTradeItem{true}`。
* **放金币**（`:10744-10760`）：先 `TradeUnlock()`（清双方确认），再校验交易对象与金额；
  金额累加进 `TradeGoldAmount`，同时从 `Account.Gold` 扣除，本人收 `S.LoseGold`，对方收
  `S.TradeGold{Amount = 累计额}`。**协议里没有「取回金币」**，要拿回只能取消交易。

### 2.3 成交的精确包序

双方都 `TradeConfirm(true)` 后（`:10855-10887`）：

```text
对 p in [对方, 自己]:              # Crystal 的 TradePair 顺序
    每件交易物品:                  # 先处理 p 的整份交易格
        对方.GainItem(物品)         → 对方收 S.GainedItem
        p.Info.Trade[格] = null
    若 p.TradeGoldAmount > 0:
        对方.GainGold(金币)         → 对方收 S.GainedGold
        p.TradeGoldAmount = 0
    p 收系统聊天「交易成功。」
    p 收 S.TradeConfirm{}           → 客户端关闭交易窗
    p.TradeLocked = false
    p.TradePartner = null
```

### 2.4 取消的精确包序

`PlayerObject.TradeCancel()`（`:10889-10958`）按 `[对方, 自己]` 两边处理：

```text
TradeUnlock()
若没有交易对象: return（不发任何包）
对 p in [对方, 自己]:
    对 p 的每个交易格 t:
        FreeSpace(p.Inventory) < 1 → GainItemMail(物品, 1) + S.DeleteItem + 清格
        否则找第一个空背包格:
            CanGainItem(物品) → p.RetrieveTradeItem(t, 空背包格)
                                 （→ p 收 S.RetrieveTradeItem{true}，对方收 S.TradeItem）
            否则              → GainItemMail + S.DeleteItem
    若 p.TradeGoldAmount > 0:
        p.GainGold(金币)            → p 收 S.GainedGold
        p.TradeGoldAmount = 0
    p.TradeLocked = false
    p.TradePartner = null
    p 收 S.TradeCancel{Unlock=false}
```

「取消」是**真正的退货并关窗**；而成交前发现对方收不下物品/金币时发的是
`S.TradeCancel{Unlock=true}`（只解锁、不关窗、不退货，见 §5）。

---

## 3. 发起与接受

### 3.1 发起：`World::trade_request`（`C.TradeRequest`）

Crystal `PlayerObject.TradeRequest()`（`:10623-10707`）的校验顺序：

| # | 检查 | Crystal 行为 | 本作 |
|---|---|---|---|
| 1 | `Envir.Time < NextTradeTime` | **静默**返回（但冷却已在下一步之前被重置，见 §11） | `TradeError::Cooldown`，不发包 |
| 2 | 自己在交易中 | 聊天「你已经在交易中」 | `TradeError::AlreadyTrading` + 同一句聊天 |
| 3 | 面朝格不是合法点 | **静默**返回（`:10636`） | 找不到目标 → 聊天「请面对你要交易的对象。」（**差异**，见 §8） |
| 4 | 面朝格没有对象/没有玩家 | 聊天「请面对你要交易的对象」 | 同左 |
| 5 | 双方没有面对面（`Functions.FacingEachOther`） | 同上聊天 | `TradeError::NotFacing` + 同一句聊天 |
| 6 | 目标是自己 | 聊天「不能和自己交易」 | `TradeError::SelfTrade` |
| 7 | 任意一方死亡 | 聊天「死亡状态不能交易」 | `TradeError::Dead` |
| 8 | 目标已有待回应邀请 | 聊天「某某已经收到了交易邀请」 | `TradeError::TargetInvited` |
| 9 | 目标 `AllowTrade == false` | 聊天「某某不允许交易」 | `TradeError::TargetNotAllowingTrade` |
| 10 | 不同地图或距离 > `DataRange(16)` | 聊天「某某不在交易范围内」 | `TradeError::OutOfRange` |
| 11 | 目标已在交易中 | 聊天「某某已经在交易中」 | `TradeError::TargetTrading` |
| — | 通过 | `player.TradeInvitation = this; player.Enqueue(S.TradeRequest{Name})` | 同左 |

**发起者不会收到任何回执**：Crystal 只在失败时给发起者聊天；成功时唯一的包发给目标。
协议里也没有「交易请求被拒绝」的回执，拒绝只让邀请者收到一句系统聊天（`:10716-10721`）。

### 3.2 本作扩展：`World::trade_request_target`

Crystal 的 `C.TradeRequest` 不带目标，服务端按「面朝格」选人（`PlayerObject.cs:10623-10658`）。
为了测试与将来「点击玩家 → 交易」的客户端，本作额外暴露
`World::trade_request_target(object_id, target_id)`：先做同样的冷却与「自己是否在交易中」检查，
再从 `:10668` 的「不能和自己交易」开始执行同一套校验（顺序完全一致）。线上协议路径仍是
`World::trade_request`，扩展入口不改变任何包。

### 3.3 接受 / 拒绝：`World::trade_reply`（`C.TradeReply`）

Crystal `TradeReply`（`:10708-10743`）：

1. `TradeInvitation == null || TradeInvitation.Info == null` → 清邀请、**静默**返回；
2. 拒绝 → 给邀请者一句「某某拒绝交易」，清邀请，**不给回应者发包**；
3. 接受时：自己已在交易中 → 聊天「你已经在交易中」，清邀请；
4. 邀请者已在交易中 → 聊天「某某已经在交易中」，清邀请；
5. 通过 → 双方 `TradePartner` 互指，**双方**各收 `S.TradeAccept{Name=对方}`。

**接受时不复查距离/朝向/死亡/`AllowTrade`**：从收到邀请到点「接受」之间对方走开也能成交（之后
`TradeConfirm` 的距离检查才会发现），本作照抄。

---

## 4. 交易格、金币与「确认」的联动

### 4.1 放入 / 取回不检查「确认」

`DepositTradeItem` / `RetrieveTradeItem` 本身**不检查** `TradeLocked`；移动成功后调用
`TradeItem()`，而 `TradeItem()` 第一行就是 `TradeUnlock()`（`:10761-10787`），把**双方**的
`TradeLocked` 清掉。等价于「改动交易内容即撤销双方的确认」。

本作照抄这一语义：`trade_items_changed` 先 `trade_unlock`，再给对方发 `S.TradeItem`。
客户端在 `S.DepositTradeItem` / `S.RetrieveTradeItem` / `S.TradeItem` / `S.TradeGold` 上也会
`ChangeLockState(false)`（`GameScene.cs:2804-2830`、`:6312-6320`），服务端与客户端不会出现
「服务端锁着、客户端已解锁」的错位。

### 4.2 金币的边界

* 单次放入金额 `< 1` 或超过 `Account.Gold` → Crystal **静默**返回，不发包；但 `TradeUnlock()`
  已经执行，所以双方确认仍会被清掉（`:10746-10753`）。
* 金币是**累加**的，每次本人收 `S.LoseGold{本次金额}`、对方收 `S.TradeGold{累计额}`。
* 成交前检查收款方 `CanGainGold`：`(ulong)gold + Account.Gold <= uint.MaxValue`
  （`PlayerObject.cs:7638-7641`）。
* 取消时 `GainGold` 退回，收 `S.GainedGold`。

### 4.3 背包能否收下：`CanGainItems`

`trade_can_gain_items` 逐行移植 `HumanObject.CanGainItems`（`HumanObject.cs:7629-7660`）：

```text
itemCount = 非空交易格数
stackOffset = 0
对每件可堆叠物品（StackSize > 1）:
    取背包里第一件同模板物品
    若 它的 Count + 交易物品 Count > StackSize → stackOffset += 1
要求 FreeSpace(Inventory) >= itemCount + stackOffset
```

注意它只看**第一件**同类堆叠（Crystal 原样如此），不是所有同类堆。

---

## 5. 确认与结算

`World::trade_confirm` 对照 `PlayerObject.TradeConfirm(bool)`（`:10789-10888`）：

| # | 条件 | 行为 |
|---|---|---|
| 1 | `confirm == false` | 只把自己的 `TradeLocked` 置 `false`，不发包 |
| 2 | `confirm == true` 且没有交易对象 | 直接 `TradeCancel()` 并返回错误 |
| 3 | 不同图 / 距离 > 16 / 不再面对面 | 直接 `TradeCancel()`（真·退货关窗） |
| 4 | 自己锁定、对方未锁定 | 自己 `TradeLocked = true`，给对方一句「某某正在等待你确认交易。」，等待 |
| 5 | 双方都锁定 | 双向检查「对方能否收下我的物品/金币」 |
| 6 | 某项检查失败 | 双方各一句聊天 + 各收 `S.TradeCancel{Unlock=true}`（**只解锁、不关窗、不退货**） |
| 7 | 全部通过 | 按 §2.3 交换物品/金币，双方各收 `S.Chat` + `S.TradeConfirm{}`，清理交易状态 |

**失败时服务端不会清 `TradeLocked`**：`TradeCancel{Unlock=true}` 只让客户端解锁
（`GameScene.cs:6323-6338`），服务端的 `TradeLocked` 仍是 `true`；下一次确认会立刻重新进入
「双方都锁定」的检查。只要任意一方改动交易格，`TradeItem()` → `TradeUnlock()` 才会真正清掉
服务端的锁。本作照抄这个特殊状态。

**交换顺序**：Crystal 的 `TradePair = [TradePartner, this]`，先给对方物品、再给自己；本作按
`for giver in [partner, self]` 相同顺序执行，每件物品 `S.GainedItem`、金币 `S.GainedGold` 的
相对顺序与 Crystal 一致。

---

## 6. 中止点与取消

### 6.1 哪些动作会中止交易

| 触发 | Crystal | 本作 |
|---|---|---|
| 下线 / `StopGame` | `PlayerObject.cs:407` `TradeCancel();` | `World::leave` |
| 走一步 | `HumanObject.cs:2491-2497` → `PlayerObject.Moved()`（`:568-574`） | `World::walk` |
| 跑 | `HumanObject.cs:2547` | `World::run` |
| 转身 | `PlayerObject.cs:4300-4301`（在广播 `ObjectTurn` 之前） | `World::turn` |
| 切图 | `PlayerObject.cs:1477` | `World::change_map`（`maps.rs`） |
| 死亡 | **没有**（`Die()` 不清交易状态，见 `PlayerObject.cs:578-667`） | `World::player_died`（**本作扩展**） |
| 主动取消 | `C.TradeCancel` → `TradeCancel()` | `Session::on_trade_cancel` → `World::trade_cancel` |

> **死亡差异**：Crystal 的 `Die()` 只把 `Dead = true`，交易状态要等到死亡后下一次移动/切图才会
> 被 `Moved()`/`CompleteMapMovement` 清掉；需求要求「死亡即中止」，本作在 `player_died` 里补了
> 一次 `trade_cancel`。这是唯一新增的中止点。

### 6.2 取消时的物品去向

1. 背包还有空格（`FreeSpace >= 1`）：找**第一个**空背包格放回（`RetrieveTradeItem`），
   本人收 `S.RetrieveTradeItem{true}`，对方收 `S.TradeItem`；
2. 背包满（`FreeSpace < 1`）：走 `GainItemMail(temp, 1)`（`:10910-10918`）——本作对应
   `World::mail_item_to_self`，发件人是「比奇管理员」，正文说明「交易取消后背包已满」，
   邮包状态为未受理（`Collected = false`），玩家需要去邮局领取；本人收 `S.DeleteItem`
   把交易格里的物品删掉。

Crystal 的「有空格但不满足 `CanGainItem`」还会再走一次邮件（`:10925-10935`）；本作在
`free_space >= 1` 时直接移动，不再复查 `CanGainItem`。只有「单格堆叠数超过模板上限」这种
异常物品才会碰到差异（登记在 §8）。

### 6.3 幂等

没有交易对象时 `trade_cancel` 只做一次 `TradeUnlock()`，不发任何包。连续两次取消：
第一次退货关窗，第二次静默，测试 `cancelling_twice_is_idempotent` 断言这一点。

---

## 7. 持久化

**交易状态不落盘。**

* Crystal 的 `CharacterInfo.Trade` 虽然会随角色序列化（`CharacterInfo.cs:75`、存档 `:476-485`），
  但交易本身是临时状态：正常下线/切图/移动都会先 `TradeCancel`，所以存档里通常是 10 个空格；
  `AllowTrade` 则**是**持久化字段（`:62`、`:258`、`:485`）。
* 本作不改 `mir-store`：`Player::trade`、`trade_invitation`、`next_trade_time` 全在运行期，
  `allow_trade` 进世界时默认 `true`（Crystal 新角色 `AllowTrade` 默认 `true`），
  `C.ChangeTrade` 只改内存，下线即恢复 `true`。

如果将来要持久化 `AllowTrade`，给 `characters` 加一列并在 `session::on_start_game` 里读即可；
交易格本身不建议落盘（Crystal 的落盘值正常情况下也是空的）。

---

## 8. 与 Crystal 的**有意**差异

1. **交易状态不落盘**：见 §7。Crystal 序列化 `Info.Trade`/`Info.AllowTrade`，本作只在运行期保存；
   区别只在「服务器崩溃/强杀时 Crystal 可能把交易格里的物品写进存档，本作丢失交易格内容」——
   Crystal 的协议路径也不会把交易格物品当资产处理，故未复制。
2. **死亡即中止**：`World::player_died` 补 `trade_cancel`（Crystal 要等下一次移动/切图）。
3. **面朝格不合法时发聊天**：Crystal 在 `!CurrentMap.ValidPoint(target)` 时静默返回
   （`:10636`）；本作统一按「面朝格没有可交易玩家」处理，给一句「请面对你要交易的对象。」。
   用户可见结果更一致，但严格说多了一个包。
4. **一格多人的目标选择**：Crystal 遍历 `cell.Objects` 取**最后一个** `ObjectType.Player`
   （`:10646-10652`）；本作在「同图同坐标」的玩家中取**对象 ID 最大者**。两者都只在「两个玩家
   叠在同一格」这种非正常状态下才有区别，本作选择可回放的确定性规则。
5. **取消时的空格复查**：Crystal 有空格还会 `CanGainItem` 再决定是否寄邮件；本作有空格就放入。
6. **没有审计/Report**：Crystal 的 `Report.ItemMoved` / `Report.GoldChanged` 会写运营日志；
   本作只有 `tracing::debug!`。
7. **没有 `CheckItem` 补发模板**：Crystal 的 `TradeItem()` 会给对方逐件 `CheckItem`
   （`:10767-10774`）；本作客户端启动时自己加载全部物品模板，不需要按需补发。

## 9. 简化

* 交易对象用**对象 ID**（`Option<u32>`）而不是 `PlayerObject` 引用，避免借用检查与悬垂引用；
  对方下线时 `players.get` 失败即视为交易中止。
* 交易格是固定长度的 `Vec<Option<UserItem>>`，与 `CharacterInfo.Trade[10]` 一一对应。
* 不维护 `RefreshBagWeight` 缓存：本作负重即时计算（`World::bag_weight`）。
* 错误通过 `TradeError` 返回给会话层写日志；玩家可见提示由 `World` 直接发系统聊天，
  与 Crystal 的「聊天 + 静默」组合等价。

## 10. 与 AGENTS.md 约定的偏差

按 `AGENTS.md`「玩家可见文本一律走 `mir-core::i18n`」的约定，本模块**尚未达标**：
`trade.rs` 里的系统聊天（「你已经在交易中。」「请面对你要交易的对象。」「交易成功。」等）
是硬编码简体中文，没有 `TextKey`，也不会随玩家语言切换。这是本轮**登记但未修**的偏差，
下一轮做 i18n 收口时统一迁移到 `mir-core::i18n`（新增 `TradeAlreadyTrading` 等键，
并在 `config/lang/{zh-cn,en}.json` 补齐）。

> 同理：`market.rs`、`storage.rs`、`mail.rs` 等模块也有硬编码提示语；`docs/i18n.md` 的
> 「新增文本的流程」是最终目标，不是当前完整状态。

---

## 11. 常量与配置

| 常量 | 值 | Crystal |
|---|---|---|
| `TRADE_SLOTS` | 10 | `CharacterInfo.cs:75` `Trade = new UserItem[10]` |
| `TRADE_DELAY_MS` | 2_000 | `Settings.TradeDelay = 2000`（`Settings.cs:337`） |
| `TRADE_RANGE` | 16 | `Globals.DataRange = 16`（`Globals.cs:37`；注释写着 `//Was 24`） |
| 背包格数 | 46 | `CharacterInfo.Inventory = new UserItem[46]`（`CharacterInfo.cs:75`） |

本作**不读** `Configs/Game.ini` 的 `TradeDelay`（Crystal `Settings.cs:496`）；常量硬编码在
`trade.rs`，与默认值一致。

---

## 12. 测试清单

`crates/mir-server/tests/trade.rs`（28 个端到端用例，全部确定性：固定 seed + `advance_clock`）：

| # | 用例 | 覆盖点 |
|---|---|---|
| 1 | `trade_request_sends_invitation_to_the_faced_player` | 面朝格选人、`S.TradeRequest{Name}`、发起者无回包 |
| 2 | `trade_request_requires_a_faced_player` | 面朝格没人 → 聊天、无包 |
| 3 | `trade_request_rejects_a_player_that_does_not_face_back` | `FacingEachOther` |
| 4 | `trade_request_rejects_a_target_out_of_range` | 同图 16 格上限 |
| 5 | `trade_request_rejects_trading_with_yourself` | 自己交易自己 |
| 6 | `trade_request_rejects_players_already_trading_or_invited` | 双方已在交易 / 已有邀请 |
| 7 | `trade_request_respects_the_allow_trade_switch` | `C.ChangeTrade{false}` |
| 8 | `trade_request_is_throttled_by_trade_delay` | 2000ms 冷却与恢复 |
| 9 | `accepting_the_invitation_puts_both_players_into_trade` | 双方 `S.TradeAccept`、状态互指 |
| 10 | `declining_the_invitation_notifies_the_inviter` | 拒绝只聊邀请者 |
| 11 | `depositing_and_retrieving_an_item_keeps_both_sides_in_sync` | 双向移动、`S.DepositTradeItem`/`S.TradeItem`/`S.RetrieveTradeItem` |
| 12 | `depositing_a_dont_trade_item_is_rejected` | `BindMode.DontTrade` |
| 13 | `deposit_and_retrieve_reject_out_of_range_or_occupied_slots` | 下标与占用校验 |
| 14 | `depositing_gold_is_deducted_and_refunded_on_cancel` | 扣钱、`S.LoseGold`/`S.TradeGold`、取消退回 |
| 15 | `depositing_more_gold_than_held_is_rejected` | 金币不足 |
| 16 | `a_single_confirmation_does_not_trade` | 单方确认只提示对方 |
| 17 | `both_confirmations_swap_items_and_gold` | 成交包序、物品/金币归属 |
| 18 | `changing_the_trade_resets_both_preparations` | 放物品/金币清双方确认 |
| 19 | `full_bag_makes_the_trade_fail_without_exchanging_anything` | `CanGainItems` 失败 → `Unlock=true`、不成交 |
| 20 | `cancelling_returns_items_and_gold_to_the_owner` | 取消退货退钱 |
| 21 | `cancelling_with_a_full_bag_mails_the_item_instead` | 背包满 → 系统邮件 + `S.DeleteItem` |
| 22 | `logging_out_cancels_the_trade` | 下线中止 |
| 23 | `walking_away_cancels_the_trade` | 走一步中止 |
| 24 | `turning_around_cancels_the_trade` | 转身中止 |
| 25 | `changing_map_cancels_the_trade` | 切图中止 |
| 26 | `dying_cancels_the_trade` | 死亡中止（本作扩展） |
| 27 | `item_counts_and_unique_ids_are_conserved_by_the_swap` | 物品数量/UUID 守恒 |
| 28 | `cancelling_twice_is_idempotent` | 重复取消无副作用 |

`trade.rs` 内另有 3 个单元测试：`facing_each_other_matches_crystal`、
`trade_state_starts_with_ten_empty_slots`、`gold_packet_carries_the_running_total`。

---

## 13. Crystal 异常行为（照抄或登记）

1. **冷却先置位再校验**：`NextTradeTime` 在「自己是否在交易中」「面朝格」等检查之前就
   `= Envir.Time + TradeDelay`（`:10625-10626`），所以一次失败（甚至面朝格没人）也会让之后
   2 秒内的发起全部静默失败。本作照抄。
2. **`ValidPoint` 失败是静默的**：面朝地图边界外时 Crystal 什么也不发（`:10636`）；本作会聊天。
3. **一格多人取最后一个玩家**：`:10646-10652` 的循环没有 `break`，`player = Envir.GetPlayer(ob.Name)`
   会覆盖成格子对象列表里的最后一个玩家。
4. **接受邀请不复查条件**：`TradeReply(true)` 不检查距离、朝向、死亡、`AllowTrade`
   （`:10708-10743`），只检查双方是否已在交易中。之后 `TradeConfirm` 的距离检查才可能中止。
5. **放金币先解锁**：`TradeGold` 第一行 `TradeUnlock()`（`:10746`），金额非法/金币不足时直接
   `return`，但双方的「确认」已经没了。
6. **放入/取回不检查锁定**：靠 `TradeItem()` 的 `TradeUnlock()` 清锁（`:10761-10763`）。
7. **成交失败只解锁不关窗**：`TradeConfirm` 发现对方收不下时发 `S.TradeCancel{Unlock=true}`，
   不清 `TradeLocked`、不退货、不关窗（`:10829-10851`）；服务端锁会一直保留到下一次改格或
   `TradeConfirm(false)`。
8. **拒绝交易只聊邀请者**：`TradeReply(false)` 没有给拒绝者的回包（`:10716-10721`），
   客户端的窗口靠本地关闭。
9. **取消时背包满会寄邮件**：物品从交易格直接进邮箱（`GainItemMail`，`:10910-10918`），
   同时给本人 `S.DeleteItem`；邮包是「未受理」状态，必须去邮局领取。
10. **取消的包序固定为「对方先、自己后」**：双方都会收到自己的 `S.RetrieveTradeItem` 与
    对方的 `S.TradeItem`，最后各收一个 `S.TradeCancel{Unlock=false}`。
11. **交易不检查安全区/地图规则**：Crystal 的 `TradeRequest` 只检查距离与朝向，安全区、战斗区
    都可以交易；本作一致。
12. **`TradeRequest` 不检查目标的 `TradePartner` 是否离线**：`TradePartner` 是对象引用，
    正常下线会先 `TradeCancel`，所以线上路径不会留下悬垂引用；本作以对象 ID 查表，等价。

---

## 14. 未做 / 明确不做

* **交易审计/运营日志**：Crystal 的 `Report.ItemMoved` / `Report.GoldChanged` 没有移植。
* **客户端 `S.TradeItem` 的按需物品模板**：Crystal 的 `CheckItem` 补发；本作客户端启动时
  全量加载模板。
* **交易邀请超时**：Crystal 没有超时，本作也没有。
* **交易中的「锁定后倒计时/自动确认」**：两边都没有。
* **英雄（Hero）交易、宠物/召唤物交易**：Crystal 的 `TradeRequest` 只接受 `ObjectType.Player`，
  本作相同。
* **`AllowTrade` 持久化**：见 §7。
* **玩家可见文本的 i18n**：见 §10（本轮登记）。
