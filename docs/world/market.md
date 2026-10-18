# 市场 / 寄售 / 拍卖（Market）

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：
> * `Server/MirObjects/PlayerObject.cs` 的 `#region Consignment`（`:8217-8802`：
>   `ConsignItem` `:8218-8316`、`Match` `:8318-8325`、`MarketPage` `:8327-8374`、
>   `GetMarket` `:8376-8422`、`MarketSearch` `:8424-8448`、`MarketBuy` `:8450-8589`、
>   `MarketSellNow` `:8591-8666`、`MarketGetBack` `:8668-8793`）
> * `Server/MirDatabase/AuctionInfo.cs`（挂单实体 + `Save`/`CreateClientAuction`，`:5-117`）
> * `Server/MirEnvir/Envir.cs`（`NextAuctionID` `:127`、`Auctions` `:131`、
>   `ProcessAuction` `:2388-2418`、每 10 分钟调用 `:2354-2358`、
>   存档 `:2591-2594`、读档 `:3018-3029`）
> * `Server/MirObjects/NPC/NPCScript.cs`（`ConsignKey = "[@CONSIGN]"` `:56`、
>   `MarketKey = "[@MARKET]"` `:57`、`case` 分支 `:1043-1049`）
> * `Shared/Globals.cs`（`Commission` `:39`、`SearchDelay` `:41`、`ConsignmentLength` `:42`、
>   `ConsignmentCost` `:43`、`MinConsignment` `:44`、`MaxConsignment` `:45`、
>   `AuctionCost` `:46`、`MinStartingBid` `:47`、`MaxStartingBid` `:48`、`DataRange` `:37`）
> * `Server/MirNetwork/MirConnection.cs`（包分发 `:511-530`、处理器 `:1662-1722`、
>   刷新 `:1692-1697`）
> * `Shared/ClientPackets.cs`（`ConsignItem` `:1513-1533`、`MarketSearch` `:1564-1592`、
>   `MarketRefresh` `:1593-1603`、`MarketPage` `:1604-1617`、`MarketBuy` `:1618-1635`、
>   `MarketSellNow` `:1636-1650`、`MarketGetBack` `:1651-1668`）
> * `Shared/ServerPackets.cs`（`NPCConsign` `:4212-4222`、`NPCMarket` `:4223-4251`、
>   `NPCMarketPage` `:4252-4272`、`ConsignItem` `:4298-4315`、`MarketFail` `:4316-4342`、
>   `MarketSuccess` `:4343-4357`）
> * `Shared/Data/ClientData.cs`（`ClientAuction` 线格式）
>
> 本作实现：
> * `crates/mir-server/src/market.rs`（常量、`Auction`、搜索状态、全部流程）
> * `crates/mir-server/src/world.rs`（`World::auctions` / `market_searches` / `next_auction_id`；
>   `World::leave` 清搜索状态；`World::tick` 调 `tick_market`）
> * `crates/mir-server/src/session.rs`（七个市场包分派 `:331-363`，处理器 `:1912-2010`）
> * `crates/mir-server/src/npc.rs`（两个 NPC 页面 `:1737-1749`，对话距离 `NPC_TALK_RANGE = 16` `:55`）
> * `crates/mir-server/src/server.rs`（启动加载/过期清理 `:76-114`）
> * `crates/mir-store/migrations/0009_market.sql` + `crates/mir-store/src/{models.rs,repo.rs}`
>   （`MarketAuction`、`list_auctions` / 低层 `save_auctions`）
> * `crates/mir-server/src/store.rs` / `pg_store.rs`（`MarketUpdate` + `apply_market_update` 单事务）
> * 测试：`crates/mir-server/src/market.rs` 内 4 个单元测试；
>   `crates/mir-server/tests/market_persistence.rs`（落盘/重启/原子性 9 个）；
>   `crates/mir-server/tests/market_flow.rs`（真实 TCP 端到端 4 个）
>
> **`mir-protocol` 没有改动**：`C.ConsignItem` / `C.MarketSearch` / `C.MarketRefresh` /
> `C.MarketPage` / `C.MarketBuy` / `C.MarketSellNow` / `C.MarketGetBack` 与
> `S.NPCConsign` / `S.NPCMarket` / `S.NPCMarketPage` / `S.ConsignItem` / `S.MarketFail` /
> `S.MarketSuccess` 都已存在；`ClientAuction` 也是现成的线格式数据类。

---

## 1. 总览

市场不是「玩家在街边摆摊」，而是 **NPC 托管的寄售/拍卖行**：

* **寄售（Consign）**：一口价；上架扣手续费，买家点「买下」后物品直接进背包；
* **拍卖（Auction）**：卖家定起拍价，买家只能出价（不能买断）；到期或卖家「提前成交」后，
  物品通过邮件寄给最高出价者，卖家拿到成交价扣除 5% 佣金后的金币。

| 主题 | Crystal | 本作 |
|---|---|---|
| 挂单实体 | `AuctionInfo`（`AuctionInfo.cs:5-117`） | `market::Auction` |
| 编号 | `++Envir.NextAuctionID`（`ulong`，`:33`） | `World::next_auction_id`；启动时按 `max(id)+1` 续号 |
| 全局列表 | `Envir.Auctions`（`LinkedList`，`AddFirst`，最新在前） | `World::auctions: Vec<Auction>`，`insert(0, …)` 新在前 |
| 卖家列表 | `Account.Auctions`（账号级） | 按 `seller_character_id` 过滤（角色级） |
| 搜索状态 | `PlayerObject` 的 `Search/PageSent/MatchName/…`（`:132-139`） | `World::market_searches: HashMap<对象 ID, MarketSearchState>`，下线即清 |
| 物品归属 | `AuctionInfo.Item`（`UserItem`） | `Auction.item`（`UserItem`） |
| 卖家/买家名 | `SellerInfo` / `CurrentBuyerInfo` 引用 | `seller_name` / `current_buyer_name` 名字快照 |
| 持久化 | 写进 MirADB 的全局 `Auctions` 段（`Envir.cs:2591-2594`、`:3018-3029`） | PostgreSQL `market_auctions`（迁移 `0009`），整表快照 |
| 页大小 | 固定 10（`:8356`、`:8405`） | `MARKET_PAGE_SIZE = 10` |
| 到期 | `ProcessAuction` 每 10 分钟：有高于起拍价的出价就自动成交，否则置 `Expired` | `tick_market` 每 tick 置 `Expired`；启动时清理已过期挂单（见 §11） |
| 商城页签 | `MarketPanelType.GameShop` 读 `Envir.GameShopList`（代码被注释掉） | **已实现**（第 2 轮）：页签列出 `Server.MirDB` 商城段，`auction_id = GIndex`、`price = 元宝单价`；见 [`docs/world/gameshop.md`](gameshop.md) |

---

## 2. 协议与包序

| 语义 | 客户端包 | 服务端包 | Crystal |
|---|---|---|---|
| 打开寄售面板 | `C.CallNPC{Key="[@CONSIGN]"}`（50） | `S.NPCConsign{}`（154） | `NPCScript.cs:1043-1045` |
| 打开市场面板 | `C.CallNPC{Key="[@MARKET]"}`（50） | `S.NPCMarket{Listings, Pages, UserMode}`（155） | `NPCScript.cs:1046-1049` → `GetMarket` |
| 上架 | `C.ConsignItem{UniqueID, Price, Type}`（70） | `S.ConsignItem{UniqueID, Success}`（157）→ 成功时 `S.LoseGold{手续费}`（68） | `PlayerObject.cs:8218-8316` |
| 搜索/切页签 | `C.MarketSearch{Match, Type, UserMode, MinShape, MaxShape, MarketType}`（71） | `S.NPCMarket` | `PlayerObject.cs:8424-8448` |
| 刷新 | `C.MarketRefresh{}`（72） | `S.NPCMarket` | `MirConnection.cs:1692-1697` |
| 翻页 | `C.MarketPage{Page}`（73） | `S.NPCMarketPage{Listings}`（156） | `PlayerObject.cs:8327-8374` |
| 购买/出价 | `C.MarketBuy{AuctionID, BidPrice}`（74） | 成功：`S.LoseGold` → `S.GainedItem`（寄售）/`S.GainedGold`（拍卖退回）→ `S.MarketSuccess{Message}`（159）；失败：`S.MarketFail{Reason}`（158） | `PlayerObject.cs:8450-8589` |
| 提前成交 | `C.MarketSellNow{AuctionID}`（76） | `S.MarketSuccess` | `PlayerObject.cs:8591-8666` |
| 下架 / 收款 | `C.MarketGetBack{Mode, AuctionID}`（75） | `S.GainedItem` / `S.GainedGold` → `S.MarketSuccess`；失败 `S.MarketFail` | `PlayerObject.cs:8668-8793` |

### 2.1 上架的精确包序

`PlayerObject.ConsignItem(uniqueID, price, panelType)`（`:8218-8316`）：

```text
校验（死亡/没有 NPC 页面、面板类型、价格区间、手续费、NPC 距离、背包物品、DontSell）
→ 建 AuctionInfo（++NextAuctionID，ConsignmentDate = Envir.Now）
→ Account.Auctions.AddLast(auction); Envir.Auctions.AddFirst(auction)
→ S.ConsignItem{UniqueID, Success=true}
→ Info.Inventory[index] = null
→ Account.Gold -= cost; S.LoseGold{Gold=cost}
→ RefreshBagWeight()
→ （函数末尾又 Enqueue(p) 一次；见 §13）
```

**注意顺序**：回执在「物品移出背包、扣手续费」之前发出；`Envir.Auctions` 用 `AddFirst`，
所以最新挂单排在全局列表最前面。本作照抄顺序：`auctions.insert(0, auction)` →
`S.ConsignItem{true}` → 清背包格 → `lose_gold`（发 `S.LoseGold`）。

失败分支只回 `S.ConsignItem{Success=false}`、不发聊天；**NPC 找不到/距离太远是直接 `return`**
（`:8265-8269`），连失败回执都没有。本作照抄。

### 2.2 搜索与刷新的包序

```text
C.MarketSearch
  → 记录 UserMatch/MinShapes/MaxShapes/MarketPanelType（MirConnection.cs:1685-1688）
  → MarketSearch(match, type)（PlayerObject.cs:8424-8448）
      → 冷却检查（SearchTime）
      → 非商城：检查 NPCPage 必须是 [@MARKET]、NPC 在 16 格内
      → GetMarket(match, type)（:8376-8422）
          → 重建 Search 列表（UserMatch ? 自己的挂单 : 全局）
          → 首页最多 10 条 → S.NPCMarket{Listings, Pages, UserMode}
```

`C.MarketRefresh` 走 `MarketSearch("", MatchType)`（`MirConnection.cs:1696`）——**清空关键字、
保留物品类型**。本作 `market_refresh` 复用上一次的 `match_name`（见 §9 差异 5）。

### 2.3 翻页

```text
C.MarketPage{Page}
  → 冷却 / 死亡 / NPCPage 必须是 [@MARKET] / page > PageSent
  → 取 Search[Page*10 .. Page*10+10]（不足 10 条就少发）
  → PageSent = Page
  → S.NPCMarketPage{Listings}
```

`page <= PageSent` 直接返回（`:8335`），所以客户端不能重复请求当前页。空结果时
`Pages = (0 - 1) / 10 + 1 = 1`（C# 与 Rust 的整数除法都向 0 截断）。

### 2.4 购买 / 出价

`C.MarketBuy{AuctionID, BidPrice}`（`:8450-8589`）。失败原因编号：

| Reason | 含义 | 触发点 |
|---|---|---|
| 0 | 死亡 | 入口 |
| 1 | 没有通过市场 NPC 打开面板 | `NPCPage` 不是 `[@MARKET]` |
| 2 | 已售出 | `auction.Sold` |
| 3 | 已过期/已被移除 | `auction.Expired` / 不在 `Envir.Auctions` |
| 4 | 金币不够 | `auction.Price > Gold` 或 `bidPrice > Gold` |
| 5 | 背包放不下 | `CanGainItem(auction.Item)` |
| 6 | 不能买自己挂的 | 挂单属于自己 |
| 7 | NPC 太远 / 找不到挂单 | 距离、或 `Search` 里没有该 ID |
| 8 | 金币上限（收不下） | `MarketSellNow`/`MarketGetBack` 的 `CanGainGold` |
| 9 | 出价低于当前价 / 没有出价 | 拍卖分支 |
| 10 | 拍卖已经结束 | `MarketSellNow` 的过期/已售判定 |

**寄售**（`:8543-8555`）：`Sold = true` → 扣 `Price` 发 `S.LoseGold` → `GainItem` 发
`S.GainedItem` → 给卖家 `MessageAccount`「已售出」→ 买家 `S.MarketSuccess` → 用上次关键字
重新 `MarketSearch`。

**拍卖**（`:8556-8581`）：出价必须 `>= CurrentBid`；把上一个最高出价者的 `CurrentBid` 通过
`Envir.MailCharacter` **邮寄退回** → 记录新的 `CurrentBid`/`CurrentBuyer` → 扣买家的钱 →
给卖家 `MessageAccount`「有人出价」→ 买家 `S.MarketSuccess` → 刷新列表。

### 2.5 提前成交（卖家）

`C.MarketSellNow{AuctionID}`（`:8591-8666`）：

1. 死亡 → Reason 0；没有 `[@MARKET]` → 1；NPC 太远 → **静默**返回；
2. 只允许**卖家自己**的**拍卖**单；寄售单直接返回；
3. `CurrentBid <= Price` 或没有买家 → Reason 9；
4. 已售出/已过期/超过 7 天 → Reason 10；
5. `CanGainGold(CurrentBid)` 失败 → Reason 8；
6. `Sold = true`；物品 `MailCharacter` 给最高出价者；从 `Account.Auctions` + `Envir.Auctions`
   移除；
7. 卖家 `GainGold(成交价 - 5% 佣金)` → `S.MarketSuccess` → 刷新列表。

### 2.6 下架 / 收款

`C.MarketGetBack{Mode, AuctionID}`（`:8668-8793`）：

* `Mode = Any(0)`：按 `AuctionID` 处理**一条**；找不到 → 最终 Reason 7；
* `Mode = Sold(1)` / `Expired(2)`：批量处理该玩家**所有**符合状态的挂单，处理数 > 0 才刷新，
  否则 Reason 7；
* 单条 `TakeAuction` 的分支：
  * `Sold && Expired` → 记日志、返回 false（不能领取）；
  * 未售出/已过期 → 物品退回背包（`CanGainItem` 失败 → Reason 5）；若有人出价，
    把出价邮寄退回；从列表移除；
  * 已售出 → 成交价（寄售取 `Price`，拍卖取 `CurrentBid`）；`CanGainGold` 失败 → Reason 8；
    卖家拿到扣除 5% 佣金后的金币 → `S.MarketSuccess`。

---

## 3. 常量与公式

| 常量 | 值 | Crystal |
|---|---|---|
| `CONSIGNMENT_COST` | 5_000 | `Globals.ConsignmentCost`（`Globals.cs:43`） |
| `MIN_CONSIGNMENT` / `MAX_CONSIGNMENT` | 5_000 / 50_000_000 | `Globals.cs:44-45` |
| `AUCTION_COST` | 5_000 | `Globals.cs:46` |
| `MIN_STARTING_BID` / `MAX_STARTING_BID` | 0 / 50_000 | `Globals.cs:47-48` |
| `CONSIGNMENT_LENGTH_DAYS` | 7 | `Globals.ConsignmentLength`（`Globals.cs:42`） |
| `COMMISSION` | 0.05 | `Globals.Commission`（`Globals.cs:39`） |
| `SEARCH_DELAY_MS` | 500 | `Globals.SearchDelay`（`Globals.cs:41`） |
| `MARKET_PAGE_SIZE` | 10 | `PlayerObject.cs:8356`、`:8405` |

**卖家到手金币**（`PlayerObject.cs:8640`、`:8720`）：

```text
gold = (uint)Math.Max(0, cost - cost * Globals.Commission)
     = floor(cost - cost * 0.05)          // f64 转 u32 截断
```

| 成交价 | 到手 |
|---|---|
| 5_000 | 4_750 |
| 100 | 95 |
| 0 | 0 |

**手续费是上架时就扣的固定值**（寄售/拍卖都是 5_000），与成交价无关；成交时再扣 5% 佣金。

**到期时间**：`ConsignmentDate + 7 天`（`Envir.ProcessAuction`，`Envir.cs:2396`）。

**距离**：复用 `npc::NPC_TALK_RANGE = 16`（`Globals.DataRange`，`Globals.cs:37`）。

---

## 4. NPC 入口与访问校验

NPC 脚本里两个页面键：

```text
[@Main]
#SAY
要寄售还是看市场？
<寄售/@Consign> <市场/@Market> <关闭/@exit>

[@Consign]
#SAY
#ACT
...
```

* `[@CONSIGN]`：`npc::open_npc_panel` 的 `CONSIGN_KEY` 分支（`npc.rs:1737-1743`）→
  `World::open_consign` → `S.NPCConsign{}`。服务端不检查页面内容，只发空包让客户端开窗。
* `[@MARKET]`：`MARKET_KEY` 分支（`npc.rs:1745-1749`）→ `World::open_market` →
  `UserMatch = false` → `GetMarket("", ItemType.Nothing)` → `S.NPCMarket`。
* 两个入口都要求玩家未死亡。

之后每个市场包都要重新校验：

| 操作 | 页面键要求 | NPC 距离 | 死亡 |
|---|---|---|---|
| `ConsignItem` | 只要有 `NPCPage`（不要求是 `[@MARKET]`，玩家此时在 `[@CONSIGN]`） | 16 格，**太远静默** | 回失败回执 |
| `MarketSearch` | 必须是 `[@MARKET]` | 16 格，**静默** | 静默 |
| `MarketRefresh` | 必须是 `[@MARKET]` | 16 格，**静默** | — |
| `MarketPage` | 必须是 `[@MARKET]` | 16 格，**静默** | 静默 |
| `MarketBuy` | 必须是 `[@MARKET]` | 16 格，**静默** | Reason 0 |
| `MarketSellNow` | 必须是 `[@MARKET]` | 16 格，**静默** | Reason 0 |
| `MarketGetBack` | 必须是 `[@MARKET]` | 16 格，**静默** | Reason 0 |

> **NPC 必须还在当前地图上**：`check_npc_in_range` 在 `World::npcs` 里按
> `object_id + map` 查找，找不到就是 `NoSuchNpc`；玩家离开地图后旧页面立即失效。

---

## 5. 上架流程（`World::consign_item`）

对照 `PlayerObject.ConsignItem`（`:8218-8316`）：

| # | 检查 | 失败行为 | Crystal |
|---|---|---|---|
| 1 | 对象在世界里 | 错误返回 | — |
| 2 | 未死亡且 `npc_object_id != 0` | `S.ConsignItem{false}` | `:8222-8226` |
| 3 | 面板类型 `Consign` / `Auction` | 其它面板 `S.ConsignItem{false}` | `:8228-8263` |
| 4 | 价格区间：寄售 `5000..=50_000_000`；拍卖 `0..=50_000` | 同上 | `:8232-8251` |
| 5 | 金币 ≥ 手续费（5_000） | 同上 | `:8238-8257` |
| 6 | NPC 同图且 16 格内 | **静默返回**，不发包 | `:8265-8269` |
| 7 | 背包里按 `UniqueID` 找到物品 | `S.ConsignItem{false}` | `:8271-8286` |
| 8 | 模板/租借绑定不含 `BindMode.DontSell` | 同上 | `:8288-8292` |
| 9 | 建条目：`id = ++NextAuctionID`、`ConsignmentDate = now`、拍卖 `CurrentBid = Price` | — | `:8299`、`AuctionInfo.cs:31-45` |
| 10 | `auctions.insert(0, …)`（等价 `Envir.Auctions.AddFirst`） | — | `:8302` |
| 11 | `S.ConsignItem{true}` | — | `:8304-8305` |
| 12 | 物品移出背包 | — | `:8307` |
| 13 | 扣手续费 → `S.LoseGold{cost}` | — | `:8309-8311` |

`Auction` 的字段与 `AuctionInfo` 一一对应（`AuctionInfo.cs:12-23`）：
`id`/`item`/`consignment_date`/`price`/`current_bid`/`seller_character_id`/`seller_name`/
`current_buyer_character_id`/`current_buyer_name`/`expired`/`sold`/`item_type`。

**模板缺失时放行**：本作老存档里可能有未知模板，`item_info(...).is_none_or(...)` 按「不限制」
处理；Crystal 的 `temp.Info` 不可能为 `null` 所以没有这一分支。

---

## 6. 搜索 / 刷新 / 翻页

### 6.1 匹配规则（`Match`，`:8318-8325`）

```text
(UserMatch || (!Expired && !Sold))
&& (UserMatch ? 面板过滤 : true)
&& (MatchType == Nothing || Item.Type == MatchType)
&& (Item.Shape >= MinShapes && Item.Shape <= MaxShapes)
&& (MatchName 为空 || 去空格后的 Item.Name.Contains(MatchName, OrdinalIgnoreCase))
```

* `UserMatch = true`：只看**自己的**挂单（Crystal `Account.Auctions`，本作按角色 ID），
  且面板 `Auction` 只看拍卖、其它面板只看寄售；
* `UserMatch = false`：全局挂单，排除已售出/已过期；
* 名字匹配**去掉空格**、大小写不敏感；中文与英文等价；
* `MatchType` 用 `ItemType::Nothing` 表示「不限类型」；
* `MinShapes/MaxShapes` 默认 `0/0`；客户端 `C.MarketSearch` 的默认 `MaxShape` 是 `5000`
  （`ClientPackets.cs:1571`），服务端只按收到的值过滤。

### 6.2 分页与页数

`Pages = (Search.Count - 1) / 10 + 1`（`:8419`）。首页在 `S.NPCMarket` 里直接带 10 条；
后续页走 `S.NPCMarketPage`。`PageSent` 用于拒绝重复页请求。

### 6.3 刷新

本作 `World::market_refresh` 用上次的 `match_name/match_type` 重新 `GetMarket`，
并在 `< SEARCH_DELAY_MS` 时返回 `Throttled`；Crystal 的 `C.MarketRefresh` 用
**空关键字 + 上次类型**（`MirConnection.cs:1696`）。差异见 §9。

---

## 7. 购买 / 出价（`World::market_buy`）

校验顺序与 `PlayerObject.MarketBuy`（`:8450-8589`）一致：

```text
死亡(0) → 市场 NPC/页面(1，NPC 太远静默) → 挂单必须在上次 Search 列表里(7)
→ Sold(2) → Expired(3) → 背包能否放下(5) → 不能买自己的(6)
→ Price/bidPrice 是否超过金币(4) → 拍卖出价是否 >= CurrentBid(9)
```

**寄售买断**：

```text
Sold = true
S.LoseGold{Price}
GainItem(Item)                  → S.GainedItem
卖家 MessageAccount(已售出)      → 本作只给在线角色发系统聊天；离线丢弃
买家 S.MarketSuccess{买下}
MarketSearch(上次关键字/类型)     → 通常被 500ms 冷却吞掉（Crystal 同样）
```

**拍卖出价**：

```text
若已有最高出价者: MailCharacter(旧买家, gold=CurrentBid, 被超过的提示)
CurrentBid = bidPrice; CurrentBuyer = 买家
S.LoseGold{bidPrice}
卖家 MessageAccount(有人出价)
买家 S.MarketSuccess{已出价}
MarketSearch(…)
```

> **上一个出价者的退款是「系统邮件」而不是直接加钱**；邮件正文说明被谁超过、退回多少金币。
> 本作 `mail_system_to_character` 发送的邮件是未受理包裹（`Collected = false`），
> 需要收件人去邮局领取。

---

## 8. 提前成交与取回

### 8.1 `MarketSellNow`

见 §2.5。要点：

* 只有**拍卖**能提前成交，寄售只能等买家或下架；
* 必须**已经有人出价且 `CurrentBid > Price`**；
* 物品**邮寄**给最高出价者（不是直接进背包）；
* 卖家到手 `CurrentBid - 5% 佣金`；`Sold = true` 后从两个列表移除；
* 佣金按 `uint` 截断（`f64 as u32`）。

### 8.2 `MarketGetBack`

`Mode` 枚举：`Any = 0` / `Sold = 1` / `Expired = 2`（`MarketCollectionMode`）。

| 模式 | 处理范围 | 行为 |
|---|---|---|
| `Any` | 指定 `AuctionID` 的一条 | 按 `TakeAuction` 决定退货或收款 |
| `Sold` | 自己所有已售出挂单 | 逐条收款（寄售 `Price` / 拍卖 `CurrentBid`，扣 5%） |
| `Expired` | 自己所有已过期挂单 | 逐条退货；有人出价则邮寄退款 |

`Sold && Expired` 的挂单不能领取（Crystal 记一条消息后返回 false）。批量模式返回处理的
条目数；一条都没处理时回 `MarketFail{Reason=7}`。

---

## 9. 过期

* **运行期**：`World::tick` 每 tick 调 `tick_market`，把
  `!expired && !sold && now >= consignment_date + 7 天` 的条目标成 `expired`；
* **重启清理**：`server.rs` 启动时 `store.load_auctions()` → `World::load_auctions(records)` →
  `World::purge_expired_auctions(wall_now)`：已过期挂单**先把物品寄回卖家、把最高出价寄回
  买家**（系统邮件，`Collected = false`），再删除挂单；返回被清理的挂单 ID。清理后的
  挂单快照 + 退回邮件装进同一个 `MarketUpdate`，由 `store.apply_market_update(cleanup)`
  在**一个事务**里落库（见 §11.2）；
* **持久化**：`expired` 会写回 `market_auctions.expired`，运行期已标记的过期状态在重启后
  仍能被 `purge_expired_auctions` 识别。

**与 Crystal 的差异**：Crystal 的 `Envir.ProcessAuction` 每 10 分钟扫描一次
（`Envir.cs:2354-2358`、`:2388-2418`），到期时：

* 拍卖已有高于起拍价的出价 → **自动成交**（物品寄给买家、卖家收钱）；
* 否则 → 只把 `Expired` 置真，物品继续挂在列表里，等卖家 `MarketGetBack` 取回。

本作不做「自动成交」：运行期只置 `expired`；真正清理由**重启时的 `purge_expired_auctions`**
完成（物品/退款走系统邮件后删单），避免过期挂单永远占着列表、也避免吞掉买家已出的金币。
玩家在运行期仍可通过 `MarketGetBack{Mode=Expired}` 提前取回。

---

## 10. 与 Crystal 的**有意**差异

1. **持久化从「MirADB 全局段」改成 PostgreSQL 表**：字段一一对应（见 §11），但写入方式是
   「世界层全量快照 → `market_auctions` 整表替换」，而不是 Crystal 的整文件重写。
2. **卖家列表是角色级**：Crystal 的 `Account.Auctions` 是账号级，同账号另一个角色挂的单会
   出现在「我的寄售」里，也不能被同账号其他角色买走；本作按 `seller_character_id` 过滤，
   同账号角色之间可以互相买卖。与仓库「角色级」是同一套取舍。
3. **`Envir.MessageAccount` 只给在线角色发**：Crystal 会写进账号的消息队列，离线也能收到；
   本作 `World::message_character` 只给在线角色发系统聊天，离线丢弃。真正需要离线送达的
   内容（拍卖成交、出价被超过、下架退款）走**系统邮件**，这一点与 Crystal 一致。
4. **商城页签按户部实现**：`MarketPanelType::GameShop` 的搜索返回 `Server.MirDB` 的商城条目
   （Crystal 的 `GetMarket` 商城分支被注释掉了，`PlayerObject.cs:8385-8390`），
   购买主路径是 `C.GameshopBuy`（133），`C.MarketBuy` 只作页签兼容路径（数量 1、元宝）。
   详见 [`docs/world/gameshop.md`](gameshop.md)。
5. **刷新复用关键字**：本作用上次的 `MatchName` 重新搜索；Crystal 的 `MarketRefresh`
   清空关键字只保留类型（`MirConnection.cs:1696`）。本作的行为更符合玩家直觉，但与参考实现
   在「刷新后是否保留搜索词」上不同。
6. **过期的处理方式不同**：Crystal `ProcessAuction` 在到期时若有高价出价会自动成交，否则只置
   `Expired` 等卖家取回；本作运行期只置 `expired`，服务端**重启时**由
   `World::purge_expired_auctions` 把物品寄回卖家、最高出价寄回买家后删除挂单。因此
   「拍卖到期后买家自动收到物品」在本作不存在；玩家在运行期可主动 `MarketGetBack` 取回。
7. **`ConsignItem` 只发一次回执**：Crystal 在成功分支 `Enqueue(p)` 后，函数末尾又
   `Enqueue(p)` 一次（`:8304-8305`、`:8315`），成功时会收到两个 `S.ConsignItem`；本作只发一次。
8. **挂单 ID 续号**：Crystal 持久化 `NextAuctionID`（`Envir.cs:2591`）；本作不存计数器，
   启动时用 `max(id) + 1`。若历史上最大 ID 的挂单被删除后又重启，理论上会复用该 ID
   （当前单世界进程不会并发分配，风险仅限「已删除条目 ID 被新条目复用」）。
9. **搜索状态挂在世界层**：Crystal 是 `PlayerObject` 的字段（`:132-139`），随对象销毁；
   本作 `World::market_searches` 在下线（`World::leave`）时删除，语义等价。
10. **列表存 ID 而不是引用**：Crystal 的 `Search` 是 `List<AuctionInfo>`；本作存 `Vec<u64>`，
    每次按 ID 回查，天然处理「两次请求之间被买走/下架」的情况（与 Crystal 的
    `Envir.Auctions.Contains` 检查等价，`:8519`）。
11. **`TakeAuction` 批量失败提前返回**：Crystal 批量模式遇到「背包满」会继续尝试后面的条目，
    最后统一回 Reason 7；本作在第一条失败时直接返回对应 Reason（5/8）。
12. **负数页码**：Crystal 的 `MarketPage` 没有校验 `page >= 0`，负数会索引越界抛异常；
    本作把 `page` 夹到 `>= 0`。

## 11. 持久化

### 11.1 迁移

`crates/mir-store/migrations/0009_market.sql`（字段对照见该文件头）：

| 列 | Crystal |
|---|---|
| `id BIGINT PRIMARY KEY` | `AuctionID`（`AuctionInfo.cs:12`，`++NextAuctionID`） |
| `item BYTEA` | `Item`（`:14`，`encode_slots(&[Some(item)])` 单格字节块） |
| `consignment_date TIMESTAMPTZ` | `ConsignmentDate`（`:15`） |
| `price BIGINT` | `Price`（`:16`，`uint` 上界 CHECK） |
| `current_bid BIGINT` | `CurrentBid`（`:16`） |
| `seller_character_id BIGINT`（外键，级联删） | `SellerIndex`（`:18`） |
| `seller_name TEXT` | `SellerInfo.Name`（`:19`、`:96`） |
| `current_buyer_character_id BIGINT NULL` | `CurrentBuyerIndex`（`:18`，`-1` → `NULL`） |
| `current_buyer_name TEXT NULL` | `CurrentBuyerInfo.Name` |
| `expired` / `sold BOOLEAN` | `AuctionInfo.Expired` / `Sold`（`:21`） |
| `item_type SMALLINT` | `MarketItemType`（`:23`，0/1/2） |

`mir-store` 新增：

* `models::MarketAuction`（与列一一对应；服务端侧对应 `store::AuctionRecord` 转换）；
* `repo::list_auctions(conn)`：`SELECT … ORDER BY id DESC`（新 → 旧，等价 `AddFirst`）；
* `repo::save_auctions(conn, auctions)`：**低层**整表替换（`DELETE` + 批量 `INSERT`），
  供工具/测试或 `MarketUpdate` 事务内部调用；正常市场链路不直接调它。
* 服务端 `store::MarketUpdate { character, mail_writes, auctions }` +
  `AccountStore::apply_market_update(update)`：把角色行、系统邮件、挂单快照放进**同一个**
  存储事务（`pg_store.rs` 里一次 `begin` → 更新角色 → 写邮件 → 整表替换挂单 → `commit`）。

### 11.2 运行期：`MarketUpdate` 单事务 + 串行 flush

世界层持有全部挂单（`World::auctions`）。持久化入口统一为
`store::MarketUpdate { character, mail_writes, auctions }` 与
`AccountStore::apply_market_update(update)`：**当前角色行 + 系统邮件 + 挂单全量快照**
在同一个存储事务里提交（PostgreSQL 见 `pg_store.rs`；`MemoryStore` 按相同语义实现）。
对手方（卖家/被超价的买家）只通过邮件收款/收货，不需要跨会话写对方角色行。

| 时机 | 实际调用链 | 说明 |
|---|---|---|
| 服务器启动 | `store.load_auctions()` → `World::load_auctions(records)` | 读取失败直接中止启动，避免空市场覆盖库；按 `id DESC` 还原全局列表 |
| 启动清理 | `World::purge_expired_auctions(now_ms)` → 构造 `MarketUpdate{character:None, mail_writes: world.take_mail_writes(), auctions: world.auction_snapshot()}` → `store.apply_market_update(cleanup)` | 退回邮件与删除挂单**同一事务**；失败整组回滚，重试不会重复退款 |
| 市场包成功 | `Session::handle` 的市场分支置 `market_mutation`，包处理完统一走 `Session::flush_auction_writes` → `market::flush_market_update(store, &world, Some(character_snapshot))` | 覆盖上架/买断/出价/提前成交/取回的成功分支；失败分支不改快照 |
| 下线 | `leave_world` → `Session::flush_auction_writes` | 补一次，兜住动作路径遗漏 |

`market::flush_market_update` 的并发契约（刑部 t7 F1）：

1. 先拿进程级 `MARKET_FLUSH_LOCK`（独立于 world 锁，串行化所有会话的市场落盘）；
2. 再拿 world 锁，取**最新**的 `auction_snapshot()` 与待写邮件队列，随即释放 world 锁；
3. 调 `apply_market_update` 在单事务里写角色行/邮件/挂单。

因此不会出现「旧快照后提交、整表覆盖新状态」；写库期间不持有 world 锁，不阻塞 tick
或其它会话。`Session::flush_auction_writes` 负责生成当前角色的 `Character` 快照
（`market::character_snapshot`：背包/装备/仓库/技能/任务 + 金币/HP/MP/等级/经验/坐标/
地图/行会），保证「客户端看到的市场背包/金币变化」与挂单、邮件同事务落地。

**事务失败时邮件回灌**（`market.rs:1509-1547`，第 1 轮刑部 t11/t12 的收口）：

* 邮件与挂单在**同一个**事务里提交，所以「事务成功」才是队列真正清空的唯一条件；
* `flush_market_update` 在调用 `apply_market_update` 之前先把本次 drain 的邮件 `clone()`
  一份（`:1525`）。一旦返回 `Err`，它重新拿 world 锁，把这份邮件**按原顺序**放回
  `World::mail_writes` 队首，再把等待期间其它路径追加的新邮件接到后面（`:1533-1545`），
  并记一条 `warn!`（`restored` 为回灌条数）；
* 因此 DB 故障 / 事务回滚不会吞掉退款、成交、过期退回等系统邮件：下一次 flush（可能是
  同一次会话的下一个市场包，也可能是下线兜底）会用新快照重试同一批邮件；
* 对应的回归用例是 `market_persistence.rs::flush_failure_keeps_mail_queue_for_retry`
  （见 §14）。

`World::auction_snapshot()` 生成 `Vec<AuctionRecord>`；`market::persist_auctions` 仍保留为
**低层工具**（只写挂单、不写角色行/邮件），正常市场链路使用 `flush_market_update`。
`next_auction_id` 在 `World::load_auctions` 时按 `max(id) + 1` 续号（含解码失败但 id 有效的
记录，避免覆盖旧挂单）。

### 11.3 与 Crystal 存档的对照

Crystal 把挂单写在 `Server.MirADB` 的全局段（`Envir.cs:2591-2594`），读档时逐条
`BindItem`/`BindCharacter`（`:3018-3029`）；本作把同一份字段放进 `market_auctions`，
物品仍是 `UserItem[]` 字节块，只是从「整文件重写」变成「单表快照」。字段级说明见
[`docs/store/schema.md`](../store/schema.md) §9 与迁移文件头。

---

## 12. 与 AGENTS.md 约定的偏差

`market.rs` 的玩家可见文本（`S.MarketSuccess` 的成交/出价提示、系统邮件正文、`seller_label`
的「出售中/已售出/已过期/已有出价/无人出价」）目前是**硬编码简体中文**，没有走
`mir-core::i18n`。具体位置（`crates/mir-server/src/market.rs`）：

| 行 | 内容 |
|---|---|
| `:161-190` | `Auction::seller_label`：已售出/已过期/出售中/已有出价/无人出价（随 `S.NPCMarket` 的 `ClientAuction.seller` 下发） |
| `:563`、`:568` | 过期清理的系统邮件正文（出价退回 / 物品退回） |
| `:965`、`:969` | 寄售卖出的卖家/买家提示 |
| `:987` | 出价被超过的退款邮件 |
| `:1010`、`:1014` | 有人出价的卖家提示、买家出价成功提示 |
| `:1083`、`:1091` | 拍卖赢家邮件、卖家提前成交提示 |
| `:1365`、`:1389` | 下架退款邮件、收款提示 |

这与 `AGENTS.md`「玩家可见文本一律走 `mir-core::i18n`」的约定不符；本轮登记，
下一轮做 i18n 收口时迁移（新增 `MarketSold/Bought/Bid/…` 键，中英语言包补齐）。
审计结论与复跑命令见 [`docs/i18n.md`](../i18n.md) §6。

---

## 13. Crystal 异常行为（照抄或登记）

1. **`ConsignItem` 成功会发两次回执**：成功分支 `Enqueue(p)`（`:8304-8305`）后，循环外的
   `Enqueue(p)`（`:8315`）又发一次；本作只发一次。
2. **`ConsignItem` 的 NPC 距离失败是静默的**：`:8269` 直接 `return`，客户端收不到任何包，
   只能靠背包/金币没变化判断失败。
3. **`MarketSearch` 先置 `SearchTime` 再校验**：`:8428` 先设置 500ms 冷却，后面检查
   `NPCPage`/距离失败时也已进入冷却——连续失败搜索要等 500ms 才能重试。
4. **`MarketSearch` 的 NPC 循环没有 `break`**：`:8438-8446` 找到 NPC 后仍会继续遍历，
   同 ID 重复时可能多次 `GetMarket`；本作用 `find` 只取第一个。
5. **`MarketPage` 没有负数保护**：`page` 为负会索引越界；本作夹到 0。
6. **`MarketBuy` 末尾的刷新常被冷却吞掉**：成交后 `MarketSearch(MatchName, MatchType)`
   （`:8554`、`:8580`）只在上次搜索已过 500ms 时才会真正刷新；否则客户端要自己再搜一次。
   本作同样用 `market_refresh`，行为一致。
7. **`MarketBuy` 的挂单只在上次搜索结果里找**：没搜过、或搜索后列表被清空，会回 Reason 7
   （`:8502`、`:8588`）。所以「打开市场 → 直接买」必须先有首页搜索结果。
8. **Crystal 的商城分支是死代码**：`GetMarket` 的 GameShop 分支整段被注释（`:8385-8390`），
   `MarketBuy` 的 GameShop 分支仍引用 `Account.Credit`。本作**已实现**商城页签与
   `C.GameshopBuy`（`docs/world/gameshop.md`），这里保留 Crystal 的原状描述作对照。
9. **到期自动成交**：`ProcessAuction` 在 7 天到期时若有 `CurrentBid > Price`，会直接把物品
   邮寄给最高出价者并给双方消息（`Envir.cs:2398-2409`），不是简单的「过期」。本作未实现，
   见 §10 第 6 条。
10. **`ProcessAuction` 每 10 分钟才跑一次**：`Envir.cs:2354-2358`，所以 Crystal 的过期最多
    延迟 10 分钟；本作每 tick 比较，过期更及时。
11. **`MarketGetBack` 批量模式可能什么都不发**：`count == 0` 时最终回 Reason 7；
    本作相同。
12. **`MarketSellNow` 的佣金是 `uint` 截断**：`(uint)Math.Max(0, cost - cost * 0.05F)`
    （`:8640`），浮点误差可能导致个位数差异；本作用 `f64` 同样截断，并有单元测试锁定
    `5000 → 4750`、`100 → 95`。
13. **`ConsignmentDate` 是 `Envir.Now`（服务器启动时刻 + Stopwatch）**，不是真实 UTC 时间；
    本作写 `DateTimeBinary::from_unix_seconds(wall_now/1000)`，落库后是真实 UTC 时刻。
    这不影响 7 天判定，只影响跨时区/重开机的显示。

---

## 14. 测试清单

`market.rs` 内 4 个单元测试：

| # | 用例 | 覆盖点 |
|---|---|---|
| 1 | `commission_matches_crystal_formula` | `after_commission(5000)=4750`、`(100)=95`、`(0)=0` |
| 2 | `consign_labels_match_crystal` | 寄售/拍卖的「出售中/已售出/已过期/已有出价/无人出价」与 `ClientAuction.price` |
| 3 | `market_state_defaults_match_crystal` | `MinShapes/MaxShapes=0`、`MatchType=Nothing`、`UserMarket=false`、面板默认 `Market` |
| 4 | `market_fail_reasons_match_crystal_numbers` | Reason 0..10 的编号 |

`crates/mir-server/tests/market_persistence.rs`（户部本轮新增，重启/落盘/原子性 9 个）：

| # | 用例 | 覆盖点 |
|---|---|---|
| 1 | `market_auction_survives_restart` | 上架 → 重建 `World` → 挂单字段（物品/价格/卖家）完整恢复 |
| 2 | `market_buyout_survives_restart` | 寄售买断后 `sold` 落盘、卖家收款；寄售的买家字段恒 `None`（拍卖买家归属见下一条） |
| 3 | `market_bid_attribution_survives_restart` | 拍卖出价者/最高价落盘，重启后归属正确 |
| 4 | `expired_auctions_are_purged_on_restart` | 过期挂单启动清理：物品寄回卖家、出价寄回买家、列表删除 |
| 5 | `market_update_persists_character_and_auctions_atomically` | `MarketUpdate` 把角色行与挂单快照同事务写入；中途失败不出现「角色已改、挂单没写」 |
| 6 | `market_cleanup_refund_is_at_most_once` | 启动清理的退邮件+删除挂单同事务，崩溃重试不会重复退款 |
| 7 | `reload_advances_next_auction_id_from_max_id` | `max(id)+1` 续号（含坏行 id），不覆盖旧挂单 |
| 8 | `postgres_market_persistence_roundtrip` | 同一组断言走 MemoryStore 与 PgStore；Pg 按 `MIR2_TEST_DATABASE_URL` 条件跳过（CI/本地已实测） |
| 9 | `flush_failure_keeps_mail_queue_for_retry` | `apply_market_update` 失败时，本次 drain 的邮件按原顺序回灌 `World::mail_writes` 队首（并发追加的新邮件排在其后）；下一次 flush 能重试同一批邮件，退款/通知不丢 |

`crates/mir-server/tests/market_flow.rs`（本轮新增，真实 TCP 端到端）：

| # | 用例 | 覆盖点 |
|---|---|---|
| 1 | `market_search_round_trips_over_the_wire` | `C.MarketSearch(71)` → `S.NPCMarket(155)`；空列表 `Pages=1` |
| 2 | `market_page_and_refresh_round_trip` | `C.MarketPage(73)` → `S.NPCMarketPage(156)`；`C.MarketRefresh(72)` → `S.NPCMarket` |
| 3 | `market_operations_on_missing_listing_report_failure` | 缺失挂单时：`C.MarketBuy(74)` → `S.MarketFail(158){Reason=7 TooFar}`；`C.MarketGetBack(75)` / `C.MarketSellNow(76)` → `S.MarketFail{Reason=1 NoDialog}`（测试世界没有 NPC 对话，先卡在面板前置）；三者都不破坏世界状态 |
| 4 | `consign_without_npc_dialog_is_rejected` | 没和 NPC 对话时 `C.ConsignItem(70)` → `S.ConsignItem(157){Success=false}` |

> **为什么 Buy=7、GetBack/SellNow=1**：`market_flow.rs` 的测试世界没有让角色与市场 NPC
> 对话（`NPCPage == null`）。三个包的前置不同：
>
> * `C.MarketBuy`：测试先发了一次 `MarketSearch{MarketType=GameShop}`，商城分支**跳过**
>   NPC 检查（`PlayerObject.cs:8458-8486`），随后在上次搜索结果里找不到挂单 → `TooFar(7)`
>   （`:8502`、`:8588`）；
> * `C.MarketGetBack` / `C.MarketSellNow`：不看搜索面板，直接要求 `[@MARKET]` 页面，
>   没有对话就先回 `NoDialog(1)`（`:8737-8747`、`:8599-8609`）。
>
> 这与生产路径的优先级一致，不是测试漏断言。

> **仍未覆盖**：真实的「上架 → 搜索 → 买断/出价 → 提前成交 → 取回」全链路包序
> （需要 NPC 对话与多角色/多连接），以及 `MarketGetBack` 的批量收款/退货分支；
> 这些登记为后续端到端用例，客户端面板的协议对接测试由
> `crates/mir-client/tests/market_panel.rs` 覆盖（工部）。

---

## 15. 未做 / 明确不做

* **商城（GameShop）**：已实现（第 2 轮），见 [`docs/world/gameshop.md`](gameshop.md)；
  独立商城面板（`S.GameShopInfo`/`S.GameShopStock`）与 `Category` 二级筛选仍未做。
* **拍卖到期自动成交**：见 §10 第 6 条。
* **出价历史 / 一口价买断拍卖**：Crystal 也没有。
* **离线消息队列**：`MessageAccount` 只发给在线角色；退款/成交走邮件。
* **跨服/多世界**：单进程世界，`Envir.Auctions` 的设计就是全服一个列表。
* **市场审计日志**：Crystal 的 `MessageQueue.EnqueueDebugging`（`:8421`）没有移植。
* **挂单上限 / 单角色最多挂几件**：Crystal 的 `//TODO Check Max Consignment.`（`:8297`）
  没有实现；本作也没有。
* **物品模板按需下发**：Crystal 的 `CheckItem`（`MarketPage`/`GetMarket` 里逐条补发模板，
  `:8367-8370`、`:8416-8417`）没有移植；客户端启动时全量加载模板。
* **玩家可见文本 i18n**：见 §12。
