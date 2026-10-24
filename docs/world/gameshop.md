# 商城（GameShop）与元宝

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
>
> **Crystal 侧**
> `Server/MirObjects/PlayerObject.cs:13701-13946`（`GameShopStock` / `GameshopBuy` / `GetGameShop`）、
> `Server/MirEnvir/Envir.cs:119`（`GameShopList`）、`:133`（`GameshopLog`）、`:2911-2923`（读商城段）、
> `:4550-4561`（`Main.BindGameShop`）、`:2598-2599` / `:3054`（`GameshopLog` 存档/读档）、
> `Server/MirDatabase/CharacterInfo.cs:100`（`GSpurchases`）、`:358` / `:556-559`（随角色存档）、
> `Shared/Data/ItemData.cs:778-794`（`GameShopItem`）、`:800-827` / `:848-865`（两条序列化路径）、
> `Shared/ServerPackets.cs:5348-5369`（`S.GameShopInfo` / `S.GameShopStock`）、
> `Shared/ClientPackets.cs:1564`（`C.MarketSearch`）、
> 客户端 `Client/MirScenes/Dialogs/GameshopDialog.cs`、`Client/MirControls/MirGameShopCell.cs`。
>
> **Rust 侧**
> `crates/mir-protocol/src/data/game_shop_item.rs`（线格式，包路径与物品库路径）、
> `crates/mir-db/src/lib.rs`（`Server.MirDB` 商城段，`mir_db::GameShopItem` 直通协议类型）、
> `crates/mir-server/src/gamedata.rs`（`GameData::game_shop`，启动时绑定物品模板）、
> `crates/mir-server/src/market.rs`（`GameShopPayment` / `GameShopPurchases` / `GameShopError` /
> `World::gameshop_buy` / 商城页签 / `flush_gameshop_update`）、
> `crates/mir-server/src/store.rs`（`GameShopUpdate` / `AccountStore::apply_gameshop_update`）、
> `crates/mir-server/src/pg_store.rs`（单事务实现）、
> `crates/mir-store/src/{models.rs,repo.rs}`（`GameShopStock` / `GameShopPurchase` 与读写）、
> `crates/mir-store/migrations/0011_gameshop.sql`、
> `crates/mir-client/src/{net.rs,ui/market.rs}`（`NetCommand::GameShopBuy`、商城页签）。
>
> 本文与 [`docs/world/market.md`](market.md) 是姐妹篇：市场页签的**协议框架**（`C.MarketSearch`
> / `C.MarketPage` / `C.MarketBuy` / `S.NPCMarket` / `S.NPCMarketPage` / `S.MarketSuccess` /
> `S.MarketFail`）在 market.md §2，这里只写商城特有的部分。

---

## 1. 结论先行

- **商城不是独立面板，而是市场面板的第四个页签**（`MarketPanelType::GameShop = 3`）。
  Crystal 有独立的 `GameshopDialog`（登录时 `GetGameShop` 逐条下发 `S.GameShopInfo`），
  而市场面板的商城分支在 Crystal 里**被注释掉了**（`PlayerObject.cs:8385-8390`）。
  本作按任务要求只做**页签**：`S.NPCMarket` 的每条 `ClientAuction` 里
  `auction_id = 商品 GIndex`、`item_type = GameShop`、`price = 元宝单价`。
- **商品目录来自 `Server.MirDB` 的商城段**（`Envir.cs:2911-2923`），启动时按 `ItemIndex`
  绑定物品模板，模板不存在的条目丢弃（`Main.BindGameShop`，`Envir.cs:4550-4561`）。
  没有数据库时是空目录：页签空列表、购买回「商城暂未开放」，服务器照常启动。
- **购买主包是 `C.GameshopBuy`（133）**：`GIndex + Quantity + PType`。
  服务端以 Crystal 的校验顺序为基线（数量 → 找商品 → 堆叠 → 库存 → 支付 → 余额），
  在找商品之后**插入**职业/等级两道扩展校验（顺序见 §5）；成功后
  `S.LoseCredit`/`S.LoseGold` → 发货（`S.GainedItem`，背包放不下转邮件）。
- **元宝是本作的角色级货币**（`characters.credit`），Crystal 挂在账号
  （`AccountInfo.Credit`）。同账号多角色不共享余额，这是第 1 轮的既定取舍（§4）。
- **库存计数持久化在 `gameshop_stock` / `gameshop_purchases` 两张表**（迁移 `0011`），
  与角色行、商城邮件在**同一个事务**里提交（`GameShopUpdate`）。
- **商城提示仍是中文硬编码**（`GameShopError::message()`），已登记在
  [`docs/i18n.md`](../i18n.md) §7.2 的收口清单。

**第 49 轮没有实现独立商城面板**：本轮只收口市场面板的商城页签与库存/事务边界。独立面板
（`GameshopDialog` 风格、`S.GameShopInfo`/`S.GameShopStock`）登记为 **R50 候选**，前置条件见
§12；在满足 §12 前，任何文档都不得把页签写成“独立商城已完成”。

---

## 2. 数据模型 `GameShopItem`

线格式实现：`crates/mir-protocol/src/data/game_shop_item.rs`。
Crystal 这个类有**两条互不兼容的序列化路径**：

| 路径 | Crystal 读 | Crystal 写 | 含 `Info` | 用途 |
|---|---|---|---|---|
| 包路径 `packet = true` | `ItemData.cs:829` | `:848`（`Save(writer, true)`） | 是 | `S.GameShopInfo`（`ServerPackets.cs:5348-5369`） |
| 物品库路径 | `ItemData.cs:800`（`version = int.MaxValue`） | `:848`（`Save(writer)` 缺省） | 否 | `Server.MirDB` 商城段（`Envir.cs:2911-2923`） |

两条路径**只有 `Info` 一项不同**，其余字段顺序一致。Rust 侧
`decode` / `encode` 是包路径，`decode_db` / `encode_db` 是物品库路径
（`mir-db` 用后者读 `Server.MirDB`，读入后 `info` 是 `ItemInfo::default()`，
由 `gamedata.rs` 按 `item_index` 查表回填）。

字段（`ItemData.cs:780-794`）：

| 字段 | 类型 | Crystal | 说明 |
|---|---|---|---|
| `item_index` | `i32` | `ItemIndex`（`:780`） | 物品模板编号（不是商品的唯一键） |
| `g_index` | `i32` | `GIndex`（`:781`） | **商品编号**，客户端购买时原样回填 |
| `info` | `ItemInfo` | `Info`（`:782`） | 仅包路径；整块 `ItemInfo` |
| `gold_price` | `u32` | `GoldPrice`（`:783`） | 金币单价 |
| `credit_price` | `u32` | `CreditPrice`（`:784`） | 元宝单价 |
| `count` | `u16` | `Count = 1`（`:785`） | **每份**包含的物品数量；默认 1（手写 `Default`，见下） |
| `class` | `String` | `Class`（`:786`） | 客户端分组用的**字符串**（`Warrior`/`Wizard`/`All`…），与职业枚举无关 |
| `category` | `String` | `Category`（`:787`） | 分类名（客户端按它做二级筛选项） |
| `stock` | `i32` | `Stock`（`:788`） | 库存；**`0` = 不限量** |
| `i_stock` | `bool` | `iStock`（`:789`） | `true` = 个人库存（按角色计数），`false` = 全服库存 |
| `deal` | `bool` | `Deal`（`:790`） | 促销标记 |
| `top_item` | `bool` | `TopItem`（`:791`） | 置顶标记 |
| `date` | `i64`（DateTimeBinary） | `Date`（`:792`） | 上架时间；客户端用它显示「新」 |
| `can_buy_credit` | `bool` | `CanBuyCredit`（`:794`） | 允许元宝购买 |
| `can_buy_gold` | `bool` | `CanBuyGold`（`:793`） | 允许金币购买 |

要点：

- `GameShopItem::default()` **手写**（`game_shop_item.rs:86-108`）：Crystal 的
  `Count` 初始值是 **1**（`ItemData.cs:785`），派生 `Default` 会写出 0 个。
- 包路径里 `Info` 位于 `GIndex` 与 `GoldPrice` **之间**（`ItemData.cs:833`），
  这是最容易写错的一项；`game_shop_item.rs` 的测试
  `info_sits_between_gindex_and_gold_price` 逐字节锁定。
- 物品库路径里 `CanBuyCredit` / `CanBuyGold` 只在 `version > 105` 时存在
  （`ItemData.cs:821-825`）；`Server.MirDB` 版本门见
  [`docs/store/mirdb-format.md`](../store/mirdb-format.md) §4.8。

---

## 3. 商品目录从哪里来

服务端启动：

1. `MirDb` 读 `Server.MirDB` 的商城段（`≥ 63` 版本才有；`GameshopIndex` 头字段 + 条目列表）；
2. `GameData::from_parts`（`gamedata.rs`）把每条商品的 `info` 按 `item_index` 绑定到物品模板；
   **模板不存在的条目直接丢弃**并记一条 `warn!`（Crystal `Main.BindGameShop` 同样 `continue`）；
3. `World::game_shop()` / `World::game_shop_item(g_index)` 提供只读视图；
   没有数据库时返回空切片。

`GIndex` 在全局唯一（客户端用它购买/显示库存），但**不是**物品模板编号：
一件物品可以在商城里以不同 `GIndex` 卖不同数量/价格。

---

## 4. 元宝（Credit）的归属

| 项 | Crystal | 本作 |
|---|---|---|
| 存储位置 | `AccountInfo.Credit`（**账号级**；同账号多角色共享） | `characters.credit`（**角色级**，迁移 `0001_init.sql:84-86`；`gold` 在 `:85`、`credit` 在 `:86`） |
| 读取/显示 | `S.UserInformation.Credit` | 同左（`S.UserInformation.credit`，登录时从角色行填入） |
| 扣款包 | `S.LoseCredit{Credit}`（`PlayerObject.cs:13828`） | 同左（`market.rs` 的 `ServerPacket::LoseCredit`） |
| 金币 | `AccountInfo.Gold`（账号级） | `characters.gold`（角色级，第 1 轮同款取舍） |
| 任务奖励 | `GainCredit` 写账号 | 写当前角色行（`docs/world/quests.md`） |

**这是有意的差异，不要写成「与 Crystal 一致」**：本作自第 1 轮起把金币/元宝/仓库都放在
角色行（见 `crates/mir-store/src/lib.rs` 的「取舍」一节与迁移 `0001` 的注释）。
影响：

- 同账号的角色 A 买的元宝，角色 B 看不到；
- 商城购买事务只需要写**一个**角色行，不需要跨账号聚合；
- 若将来要改回账号级，需要同步改 `S.UserInformation`、任务奖励、金币与客户端显示，
  属于跨层改动。

NPC 入口：脚本动作 `GIVECREDIT` / `TAKECREDIT` / `CHECKCREDIT` 可以给/扣/查**当前角色**的
元宝（扣款发 `S.LoseCredit`、发放发 `S.GainedCredit`）；不产生新的玩家可见文案
（`CHECKCREDIT` 走任务/脚本变量）。

---

## 5. `C.GameshopBuy` 的校验顺序

包：`C.GameshopBuy`（133，`GameshopBuy { g_index: i32, quantity: u8, p_type: i32 }`，
`crates/mir-protocol/src/packets/client.rs`），入口 `World::gameshop_buy`
（`market.rs`，Crystal `PlayerObject.GameshopBuy`，`PlayerObject.cs:13725-13911`）。

失败时**不改任何状态**；是否给玩家提示取决于 `GameShopError::is_silent()`：
数量越界、超过 5 组、余额不足、不允许该支付方式是**静默**的（与 Crystal 的裸 `return`
一致），其余由会话层把 `GameShopError::message()` 发成 `S.Chat{System}`（见 §6 的包序）。

| # | 校验 | 失败原因 | Crystal |
|---|---|---|---|
| 1 | 玩家在世界里 | `NotInWorld` | — |
| 2 | `1 ≤ quantity ≤ 99` | `InvalidQuantity`（静默） | `:13727`（Crystal 直接 `return`） |
| 3 | 已加载商城数据 | `NoGameData`（「商城暂未开放。」） | — |
| 4 | 按 `GIndex` 找商品 | `ProductNotFound`（「商城里没有这件商品。」） | `:13749-13754` |
| 5 | 堆叠组数：`ceil(quantity × Count / StackSize) ≤ 5` | `TooManyStacks`（静默） | `:13756`（Crystal 静默 `return`） |
| 6 | 职业：`GameShopItem.Class` 字符串（`Warrior`/`Wizard`/`Taoist`/`Assassin`/`Archer`/`All`；空串/`All`/`None` = 不限，中文职业名也认） | `WrongClass`（「你的职业不能购买这件商品。」） | **本作扩展**；Crystal 服务端不查，只在客户端按 `Class` 分组过滤（`GameshopDialog.cs:288-373`、`:508`） |
| 7 | 等级：物品模板的 `RequiredType::Level` / `MaxLevel` + `RequiredAmount` | `WrongLevel`（「等级不够：需要 N 级。」） | **本作扩展**（商品没有等级字段，与穿戴同一口径） |

> 小瑕疵：`RequiredType::MaxLevel`（等级过高）也复用 `WrongLevel`，提示是「等级不够」，
> 语义偏误导；错误类型里带了 `level`，修文案时一并处理（已登记在 `docs/i18n.md` §7.2）。
| 8 | 库存：`Stock != 0` 时 `Stock - 已购 ≥ quantity` | `OutOfStock`（「库存不足。」） | `:13758-13785` |
| 9 | 支付方式：`p_type == 0` 元宝 / `1` 金币 | `UnsupportedPayment`（「不支持的支付方式。」） | `:13791`、`:13800`、`:13810-13813`（Crystal 回「钱不够」） |
| 10 | 该商品允许这种货币：`CanBuyCredit` / `CanBuyGold` | `CreditNotAllowed` / `GoldNotAllowed`（静默） | `:13794`、`:13803` |
| 11 | 余额：`单价 × 数量 ≤ 余额` | `NotEnoughCredit` / `NotEnoughGold`（静默） | 同上 |
| 12 | 扣款 +（金额非 0 时）`S.LoseCredit`/`S.LoseGold` | — | `:13824-13836` |
| 13 | 发货（§6） | `CannotGain`（兜底） | `:13870-13910` |
| 14 | 计数推进 + 返回 `GameShopPurchaseReport`（不落库） | — | `:13838-13861` |

**顺序说明**：以 Crystal 的顺序为基线（数量 → 找商品 → 堆叠 → 库存 → 支付 → 余额），
在「找商品」之后**插入**职业/等级两道扩展校验（第 6/7 步），其余顺序与 Crystal 一致。
职业校验用**商品级** `GameShopItem.Class`（不是物品模板的 `RequiredClass` 位掩码）：
Crystal 商城的职业限定本来就是商品字段，用模板位掩码会把「商城限职业但物品不限」错杀。

`p_type` 的非法值（例如 2）归到 `UnsupportedPayment`，走市场页签的 `C.MarketBuy` 时
对应 `S.MarketFail` 的 `TooFar(7)`（见 §7）。

落盘失败（§8.3）时 `World::rollback_gameshop_purchase` 会**原样撤回**：
退回货币（`S.GainedCredit`/`S.GainedGold`）、从背包删掉本次到货、删除商城邮件与待写队列，
再给玩家一条「商城购买失败：已退回支付的货币与物品，请稍后再试。」。

---

## 6. 发货：背包优先，放不下转邮件

Crystal **一律寄商城邮件**（`MailInfo Sender = "Gameshop"`，`PlayerObject.cs:13900-13910`），
本作按任务要求改成：

1. `game_shop_stacks` 把一次购买拆成待投递的物品（`market.rs`）：
   * `StackSize <= 1` 或总数为 1：每份一个物品（Crystal 在这一支忽略 `Product.Count`）；
   * 否则按 `StackSize` 切堆，最后一堆可能不满；
2. 逐堆 `can_gain_item`：放得下就 `gain_item`（发 `S.GainedItem`，包 66）；
3. 放不下的**攒成一封商城邮件**（`World::mail_gameshop_to_character`，
   `mail.rs`）：发件人 `"商城"`、正文「感谢您购买商城商品。」、附件是本次所有
   放不下的堆叠（Crystal `Sender = "Gameshop"`、正文 `ThankYouPurchaseGameshop`，
   `PlayerObject.cs:13900-13907`）；包裹需要先去邮局受理（`Collected = false`）；
4. 同时给买家发一条系统聊天「背包放不下的 N 份物品已寄到邮箱，请到邮局领取。」；
5. 无论直接进包还是转邮件，**元宝/金币都已经扣过**（`S.LoseCredit`/`S.LoseGold` 在发货前）。

包序（成功）：

```text
C.GameshopBuy
  → S.LoseCredit{Credit} 或 S.LoseGold{Gold}     // 扣款（金额为 0 时不发）
  → S.GainedItem{Item} × 进背包的堆数             // 每堆一条
  → [S.Chat{System}] 背包放不下时的提示           // 仅在转邮件时
  → （无 S.MarketSuccess：C.GameshopBuy 是独立包，Crystal 同样不回执）

C.MarketBuy（商城页签兼容路径）
  → 同上扣款/发货
  → S.MarketSuccess{message:"你购买了 {name} × {N}。"}   // 仅在走市场页签时
```

失败：

* **静默类**（`GameShopError::is_silent()`：数量越界、超过 5 组、余额不足、
  不允许该支付方式）——**什么都不发**，与 Crystal 的裸 `return` 一致；
* 其余——`S.Chat{System}` 一条（`GameShopError::message()`），不扣款、不发货；
  走 `C.MarketBuy` 时改成 `S.MarketFail{Reason}`。

> 商城邮件与玩家邮件的落库走同一条 `MailWrite` 队列（`mail.rs`），
> 与角色行、库存计数在**同一个事务**里提交（§8）。

---

## 7. 客户端页签与包序

### 7.1 打开页签

市场面板的 4 个页签对应 `MarketPanelType`（`0 Market / 1 Consign / 2 Auction / 3 GameShop`）。
商城页签的打开流程（本作）：

```text
C.MarketSearch{Match:"", Type:Nothing, Usermode:false, MinShape:0, MaxShape:0, MarketType:GameShop}
  → S.NPCMarket{Listings:[ClientAuction…], Pages, UserMode}     // 155，首页最多 10 条
C.MarketPage{Page:N}          → S.NPCMarketPage{Listings:[…]}    // 156
C.MarketRefresh{}             → S.NPCMarket{…}                   // 重发首页（受 500ms 节流）
```

与市场页签的区别：**商城页签不检查 NPC 对话**（Crystal 的 `GetMarket` 商城分支就是
`:8430` 的 `if (MarketPanelType == GameShop) …` 跳过 NPC 校验），所以玩家可以在任何地方
打开；普通市场/寄售/拍卖页签仍要求 `[@MARKET]` 页面与 NPC 在范围内。

`ClientAuction` 的字段映射（`market.rs::game_shop_client_auction`）：

| 字段 | 值 |
|---|---|
| `auction_id` | 商品 `GIndex`（购买时原样回填 `C.GameshopBuy.g_index`） |
| `item` | 模板 ID + 每份数量（`unique_id`/耐久为 0，只供客户端画图标与名字） |
| `seller` | 空串（商城没有卖家） |
| `price` | **元宝单价 `CreditPrice`**（金币价不在列表展示；Crystal 的商城页签也只显示 Credit，`TrustMerchantDialog.cs:979`） |
| `consignment_date` | 商品上架时间 |
| `item_type` | `MarketItemType::GameShop = 2` |

过滤规则（`World::game_shop_matches`）：物品类型（`Type != Nothing` 时相等）、
名字关键字（物品名**去掉空格**后 `to_lowercase().contains(关键字小写化)`）、
职业可用（`game_shop_class_allows` 读**商品级** `GameShopItem.Class` 字符串，
空串/`All`/`None` 不限，英文职业名大小写不敏感、中文职业名也认；**本作扩展**，
Crystal 只在客户端按 `Class` 分组过滤）。`MinShapes/MaxShapes` 不参与商城过滤。

排序：直接用 `Server.MirDB` 里的商城段顺序（`GameData::game_shop` 的读入序），
本作不按 `TopItem`/`Date` 重排；Crystal 的独立商城面板同样按 `GameShopList` 原序显示。

### 7.2 购买

```text
C.GameshopBuy{GIndex, Quantity, PType}      // 133，主路径
  PType = 0 → 元宝；PType = 1 → 金币
  → 扣款 → 发货（§6）

C.MarketBuy{AuctionID = GIndex, BidPrice}   // 74，商城页签打开时的兼容路径
  → 数量固定 1、PType 固定 0（元宝）；成功 S.MarketSuccess，失败 S.MarketFail{Reason}
```

兼容路径的存在是为了「页签里直接点买」的旧客户端：`market_buy` 在
`MarketPanelType::GameShop` 时把 `auction_id` 当 `GIndex`、数量 1、元宝支付，
失败原因映射见 `GameShopError::market_reason()`：
余额不够 → `LowGold(4)`、背包满 → `NoSpace(5)`、其余 → `TooFar(7)`。

### 7.3 Crystal 独立商城面板（未实现；R50 前置，见 §12）

Crystal 的 `GameshopDialog` 用 `S.GameShopInfo`（250）逐条下发目录、`S.GameShopStock`（251）
刷新库存，客户端按 `Class`/`Category`/搜索框过滤。本作**没有**这两个包的下发路径
（协议定义仍在 `mir-protocol` 里，`mir-client` 也没有接收分支），原因是任务只要求
市场面板的商城页签。由此带来两个可观察差异：

* **列表不显示实时库存**：页签的「库存」列是占位（客户端显示「在售」），
  真库存只在购买校验时用；卖完的商品仍留在列表里，购买时回「库存不足。」；
* 购买成功后列表不会自动刷新库存（`C.MarketRefresh` / 重新搜索可以拿到新列表）。

如果将来要兼容 Crystal 客户端的内置商城面板，需要：

1. 登录/打开面板时把 `GameShopItem`（包路径）逐条塞进 `S.GameShopInfo`；
2. 购买后按剩余库存发 `S.GameShopStock{GIndex, StockLevel}`；
3. 客户端 `GameScene.GameShopInfoList` 的增删语义见 `GameScene.cs:6679-6696`。

R49 不实现该面板；R50 若启动，先满足 §12 的素材授权/哈希 manifest、真实包链、事务失败包序
三项前置，再谈客户端页签与库存刷新。

---

## 8. 库存与持久化

### 8.1 迁移 `0011_gameshop.sql`

| 表 | 列 | Crystal 对应 |
|---|---|---|
| `gameshop_stock` | `g_index INTEGER PRIMARY KEY`、`purchased BIGINT`、`updated_at` | `Envir.GameshopLog`（`Envir.cs:133`，`Dictionary<int,int>`，存档 `:2598-2599`、读档 `:3054`） |
| `gameshop_purchases` | `character_id BIGINT REFERENCES characters ON DELETE CASCADE`、`g_index INTEGER`、`quantity BIGINT`、`updated_at`，主键 `(character_id, g_index)` | `CharacterInfo.GSpurchases`（`CharacterInfo.cs:100`，存档 `:358`、`:556-559`） |

口径：

* **剩余库存 = `Stock − purchased`**（`PlayerObject.cs:13717`、`:13770`）；
* `Stock == 0` 表示不限量，**不写任何计数行**（`PlayerObject.cs:13782-13785`）；
* `iStock == true`（个人库存）读 `gameshop_purchases`，否则读 `gameshop_stock`
  （`PlayerObject.cs:13708-13715`）；
* **键统一用 `GIndex`**。Crystal 自己不一致：读计数用 `Info.Index`
  （`:13710`、`:13763`、`:13926`），写回却用 `GIndex`（`:13843-13858`）——
  本作统一按 `GIndex` 存取，是**有意修正**；
* 全服计数**每次购买都推进**（与 Crystal 相同，即使 `Stock == 0` 也会写一行）；
  个人计数只在 `iStock && Stock != 0` 时推进。

### 8.2 事务与并发

`GameShopUpdate`（`store.rs`）= 角色行（含扣款后的 `credit`/`gold` 与背包）
+ 商城邮件 + 两组计数。`flush_gameshop_update`（`market.rs`）与市场落盘共用
`MARKET_FLUSH_LOCK` 与「事务失败回灌邮件队列」逻辑，区别只是事务里写
`gameshop_purchases`/`gameshop_stock` 而不是挂单快照。

购买链路另有进程级 `GAMESHOP_LOCK`（`market::lock_game_shop`），把
「读计数 → 校验/扣款/发货 → 落盘」整段串行化：两个会话并发购买时不会拿着同一份
旧计数各自通过库存检查。

不变量：

- 客户端看到的「扣钱 + 到货」与两张计数表**同事务**提交；
- 崩溃只可能发生在「都没写」或「都写了」两种状态，不会出现「扣了钱没减库存」；
- 购买前由会话从存储层读两份计数（`load_game_shop_stock` / `load_game_shop_purchases`），
  在进程级锁下完成「读计数 → 校验/扣款/发货 → 写计数」，避免同进程两个会话读旧值超卖；
  读计数失败时**拒绝本次购买**（「商城暂时不可用，请稍后再试。」），不按「没买过」放行；
- 世界层不缓存计数（Crystal 缓存在 `PlayerObject.Info` / `Envir` 上），重启后的库存只依赖数据库。

**落盘失败的回滚**（`World::rollback_gameshop_purchase`，`session.rs` 失败分支调用）：
事务失败 = 数据库里没有任何变更，所以世界层把本次购买**原样撤回**——
退回元宝/金币（`S.GainedCredit`/`S.GainedGold`）、把已进背包的物品按
**模板 + 数量**反向扣减（`GameShopPurchaseReport::delivered: Vec<GameShopDelivery{item_index,count}>`；
可堆叠物品可能已并进旧堆，**不能按 `UniqueID` 删**）、删除商城邮件与待写队列里的对应
`MailWrite`。回滚函数**自己不发聊天**，提示由会话层统一发（避免连发两条）：
`C.GameshopBuy` 路径发「商城购买失败：已退回支付的货币与物品，请稍后再试。」，
市场页签路径回 `S.MarketFail{TooFar}`。不回滚就会出现「下线时角色行落盘、库存计数没落盘」的半状态。

### 8.3 与 Crystal 存档的对照

Crystal 把 `GameshopLog` 写进 `Server.MirADB` 的全局段（`Envir.cs:2598-2599`），
`GSpurchases` 随角色存档（`CharacterInfo.cs:556-559`）；本作拆成两张表，
语义相同、写入粒度更小（只写本次购买的行）。字段级说明见
[`docs/store/schema.md`](../store/schema.md)。

---

## 9. 与 Crystal 的差异（有意）

1. **元宝是角色级**（§4），Crystal 是账号级。
2. **投递背包优先**，Crystal 一律寄商城邮件（§6）。
3. **服务端补职业/等级校验**（§5 第 5/6 步），Crystal 只在客户端按
   `GameShopItem.Class` 字符串分组过滤，服务端不查。
4. **库存键统一 `GIndex`**（§8.1），Crystal 读写键不一致。
5. **数量/组数越界与余额不足都是静默的**：Crystal 对 `Quantity < 1 || > 99`、超过 5 组、
   余额不足都是裸 `return`，本作在 `GameShopError::is_silent()` 里显式登记这几类，
   会话层同样**不发提示**（可观察行为一致，只是错误类型可测）。
6. **非法 `PType`**：Crystal 回「钱不够」（`YouDontHaveEnoughCurrency`）；本作回
   「不支持的支付方式」。
7. **只有市场面板页签**，没有独立的 `GameshopDialog` / `S.GameShopInfo` / `S.GameShopStock`
   下发（§7.3）。
8. **库存计数的「超卖」处理**：Crystal 在 `Stock - purchased >= 0` 才发库存包，
   本作在 `remaining < quantity` 时直接拒绝；剩余量可能为负时按 `OutOfStock` 处理。
9. **购买提示是中文硬编码**（`GameShopError::message()`），i18n 收口见
   [`docs/i18n.md`](../i18n.md) §7.2。

---

## 10. 测试清单

`crates/mir-server/src/market.rs` 模块内单元测试覆盖 `GameShopPayment::from_wire`、
`GameShopPurchases::remaining`、`GameShopError::market_reason`、`game_shop_stacks` 的分堆规则。

`crates/mir-server/tests/gameshop.rs`（第 2 轮新增，15 个用例）：

| # | 用例 | 断言要点 |
|---|---|---|
| 1 | `catalog_is_loaded_and_bound_to_item_templates` | `Server.MirDB` 商城段 → `GameData::game_shop`，模板缺失的条目被丢弃 |
| 2 | `purchase_deducts_credit_and_delivers_item` | 扣元宝 + `S.LoseCredit` + `S.GainedItem` |
| 3 | `purchase_without_enough_credit_is_rejected_silently` | 余额不足静默拒绝（不扣款、不发聊天） |
| 4 | `quantity_limits_are_enforced` | `quantity = 0/100`、超 5 组都拒绝 |
| 5 | `bag_full_purchase_goes_to_mailbox` | 背包满 → 一封商城邮件（发件人「商城」）+ `S.Chat` |
| 6 | `rollback_restores_credit_items_and_mail` | 落盘失败回滚：退货币、删到货、删邮件 |
| 7 | `rollback_handles_merged_stacks` | 可堆叠物品并堆后按「模板 + 数量」反向扣减 |
| 8 | `failed_flush_rolls_back_purchase` | `apply_gameshop_update` 失败 → 世界层回滚 + 邮件队列回灌 |
| 9 | `stock_and_class_limits_are_enforced` | 剩余库存、`GameShopItem.Class` 职业限制 |
| 10 | `payment_flags_are_enforced` | `CanBuyCredit/CanBuyGold` |
| 11 | `market_panel_lists_shop_items_and_pages_by_ten` | 页签列表 `item_type=GameShop`、每页 10 条 |
| 12 | `npc_script_can_give_and_check_credit` | NPC 动作 `GIVECREDIT`/`TAKECREDIT`/`CHECKCREDIT` |
| 13 | `restart_keeps_credit_and_stock` | 重启后元宝余额与两张计数表一致 |
| 14 | `gameshop_buy_round_trips_over_the_wire` | `C.GameshopBuy`（133）→ `S.LoseCredit`/`S.GainedItem` 的 TCP 端到端 |
| 15 | `postgres_gameshop_purchase_persists` | PgStore 事务往返（`MIR2_TEST_DATABASE_URL` 缺失时跳过） |

另外：`tests/skills.rs` 覆盖技能侧；`tests/market_flow.rs` 覆盖市场页签的 TCP 端到端；客户端
`tests/round2_panels.rs` 覆盖商城页签的列表/购买命令。

---

## 11. 未做 / 明确不做

* 独立商城面板（`GameshopDialog` 风格）与 `S.GameShopInfo`/`S.GameShopStock` 下发
  （R50 前置条件见 §12；R49 未实现）。
* 商城 `Category` 二级筛选（服务端不按 `Category` 过滤；页签只按物品类型/名字/职业过滤）。
* 商城条目编辑/定价管理（Crystal 走 GM 命令 `AddGameShopItem` 等，`Envir.cs:4340`、`:4380`）。
* 元宝充值/充值渠道（协议里没有）。
* 跨服/多世界共享库存（`gameshop_stock` 是单库全局）。

---

## 12. R50 前置条件（未完成，不构成“已实现”）

第 49 轮**没有**实现独立商城面板；本节只登记 R50 若启动该功能必须先满足的条件，避免把
市场页签或测试内手工 fixture 当成 Crystal `GameshopDialog` 的等价实现。

### 12.1 素材授权与哈希 manifest

* 面板/页签/图标/空态素材必须先在 [`docs/assets/licensing.md`](../assets/licensing.md) 的授权政策下登记；
  直接引用 `resource/` 未登记文件不算完成。
* 生成可复核的商城素材 manifest：`GIndex → 图标/名称/价格/页签分组`，每项带 `source_path`、
  `bytes`、`sha256`；发布包链只消费 manifest 指向的 `.mnp` 条目（dev/release 双路径一致）。
* 新增玩家可见文字（页签、空态、错误、按钮）一律走 `TextKey`/`Label`，中英模板占位符一致；
  不允许在客户端面板里新增中文字面量。

### 12.2 真实包链

* `S.GameShopInfo`(250)/`S.GameShopStock`(251) 必须由真实 `Server.MirDB` 商城段驱动
  （`GameShopItem` 绑定 `ItemIndex`），不能用测试内手写目录冒充。
* 必须同时跑通 dev 散装 `game-assets/gfx` 与 release `game-assets/packs`（含
  `manifest.packs.json` 回读）；缺包/坏包要有明确失败，不得静默回退到空列表。
* 商品图标与背包/市场图标必须同源哈希；面板列表的 `Class`/`Category` 过滤要能对照
  Crystal `GameScene.cs:6679-6696` 的增删语义。

### 12.3 事务失败包序

* 独立面板的“购买→扣款→发货→库存刷新”必须与现有 `GameShopUpdate` 同事务；事务失败时
  回滚包序固定为：退货币（`S.GainedCredit`/`S.GainedGold`）→ 反向扣减/删除到货物品 →
  删除商城邮件 → 回灌待写队列；客户端不得先显示成功再收到回滚。
* `S.GameShopStock` 刷新必须在事务提交成功后发；失败时不得发新库存，只能发失败回执。
* 端到端用例至少覆盖：并发购买不超卖、事务失败回滚、断线重连后库存一致、背包满转邮件后
  再领取；每条用例要断言包序与最终 DB 状态，而不是只看“收到成功包”。

> 以上三项未全部满足前，路线图/README 的独立商城只能写“未完成（R50 前置）”，不能写“已收口”。
