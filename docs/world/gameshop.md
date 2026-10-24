# 商城（GameShop）与元宝

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
>
> **Crystal 侧**
> `Server/MirObjects/PlayerObject.cs:13701-13946`（`GameShopStock` / `GameshopBuy` / `GetGameShop`）、
> `Server/MirEnvir/Envir.cs:119`（`GameShopList`）、`:133`（`GameshopLog`）、`:2911-2923`（读商城段）、
> `:4340-4342`（GM 加条目 `GIndex = ++GameshopIndex`）、`:4550-4561`（`Main.BindGameShop`）、
> `:2598-2599` / `:3054`（`GameshopLog` 存档/读档）、
> `Server/MirDatabase/CharacterInfo.cs:100`（`GSpurchases`）、`:358` / `:556-559`（随角色存档）、
> `Shared/Data/ItemData.cs:778-794`（`GameShopItem`）、`:800-827` / `:848-865`（两条序列化路径）、
> `Shared/ServerPackets.cs:5348-5369`（`S.GameShopInfo` / `S.GameShopStock`）、
> `Shared/ClientPackets.cs:1564`（`C.MarketSearch`）、
> 客户端 `Client/MirScenes/Dialogs/GameshopDialog.cs`、`Client/MirControls/MirGameShopCell.cs`、
> `Client/MirScenes/GameScene.cs:6679-6697`（`GameShopUpdate` / `GameShopStock` 增删语义）。
>
> **Rust 侧**
> `crates/mir-protocol/src/data/game_shop_item.rs`（线格式，包路径与物品库路径）、
> `crates/mir-db/src/lib.rs`（`Server.MirDB` 商城段，`mir_db::GameShopItem` 直通协议类型）、
> `crates/mir-server/src/gamedata.rs`（`GameData::game_shop`，启动时绑定物品模板）、
> `crates/mir-server/src/market.rs`（`GameShopPayment` / `GameShopPurchases` / `GameShopError` /
> `World::gameshop_buy` / 商城页签 / `flush_gameshop_update` / R50-B 的
> `game_shop_info_packet`、`World::game_shop_info_packets`、`GameShopPurchaseReport::stock_packet`）、
> `crates/mir-server/src/session.rs`（登录链 `send_gameshop_info`、购买事务与库存包时序、回滚）、
> `crates/mir-server/src/store.rs`（`GameShopUpdate` / `AccountStore::apply_gameshop_update`）、
> `crates/mir-server/src/pg_store.rs`（单事务实现）、
> `crates/mir-store/src/{models.rs,repo.rs}`（`GameShopStock` / `GameShopPurchase` 与读写）、
> `crates/mir-store/migrations/0011_gameshop.sql`、
> `crates/mir-client/src/{net.rs,ui/market.rs}`（`NetCommand::GameShopBuy`、商城页签）、
> `crates/mir-client/src/ui/gameshop.rs`（R50-C 独立商城面板）、
> `crates/mir-client/src/{ui/mod.rs,app.rs}`（`UiState::show_gameshop` 与 `K` 键接线）、
> `docs/assets/gameshop-manifest.json`（R50-D 商城素材 manifest）。
>
> 本文与 [`docs/world/market.md`](market.md) 是姐妹篇：市场页签的**协议框架**（`C.MarketSearch`
> / `C.MarketPage` / `C.MarketBuy` / `S.NPCMarket` / `S.NPCMarketPage` / `S.MarketSuccess` /
> `S.MarketFail`）在 market.md §2，这里只写商城特有的部分。
>
> **本轮（R50）状态口径**：本文所有「已实现/已通过」都附证据路径；没有证据或未实测的一律写
> PARTIAL。代码行号一律写作「`文件:行` @ `2a7e77e3`+R50 未提交改动（2026-10-25 取样）」——
> R50 的功能改动**尚未提交**，行号只是当次工作树的快照，不是永久事实。
>
> **行号漂移实测（2026-10-25 06:2x 复核）**：`app.rs` 的 R50-C 取证值已小幅漂移——
> `k-hotkey-evidence.txt` 记录 `handle_live_key` :2853、`"k" =>` :2951-2953、
> `live_toggle_gameshop` :2272；复核当前工作树为 :2857 / :2955-2957 / :2268。
> 本文引用一律写**取证快照值**，引用前请按内容 grep（`"k" =>`、`fn live_toggle_gameshop`）。
> `session.rs` 的库存包门槛与回滚分支已按复核值写（:3106-3113 / :3124-3148）。

---

## 1. 结论先行

- **两条入口并存**：
  * **市场面板第四个页签**（`MarketPanelType::GameShop = 3`）：`S.NPCMarket` 的每条
    `ClientAuction` 里 `auction_id = 商品 GIndex`、`item_type = GameShop`、`price = 元宝单价`。
    这是第 49 轮就有的路径，本轮未改动语义。
  * **独立商城面板**（Crystal `GameshopDialog` 风格，**R50-C 已实现**）：登录链逐商品下发
    `S.GameShopInfo`，购买成功后（且**只在事务提交成功后**）发 `S.GameShopStock` 刷新剩余量，
    客户端 `K` 键打开。
- **Crystal 的原样差异要记住**：Crystal 客户端在 `S.GameShopStock` 的 `StockLevel == 0` 时
  **把条目从列表里删掉**（`GameScene.cs:6686-6696`）；本作**保留条目并置「售罄」、禁止购买**
  （`crates/mir-client/src/net.rs::GameShopState::apply_stock`）。这是有意差异，见 §7.4。
- **商品目录来自 `Server.MirDB` 的商城段**（`Envir.cs:2911-2923`），启动时按 `ItemIndex`
  绑定物品模板，模板不存在的条目丢弃（`Main.BindGameShop`，`Envir.cs:4550-4561`）。
  没有数据库时是空目录：两个入口都是空列表、购买回「商城暂未开放」，服务器照常启动。
- **购买主包是 `C.GameshopBuy`（133）**：`GIndex + Quantity + PType`。
  服务端以 Crystal 的校验顺序为基线（数量 → 找商品 → 堆叠 → 库存 → 支付 → 余额），
  在找商品之后**插入**职业/等级两道扩展校验（顺序见 §5）；成功后
  `S.LoseCredit`/`S.LoseGold` → 发货（`S.GainedItem`，背包放不下转邮件）→
  （事务提交成功）`S.GameShopStock`。
- **失败零包**：任何校验失败都**不改状态、不发任何包**（静默类连提示都不发）；
  落盘失败则回滚世界层并**一个库存包都不发**（见 §8.4、§7.3）。
- **元宝是本作的角色级货币**（`characters.credit`），Crystal 挂在账号
  （`AccountInfo.Credit`）。同账号多角色不共享余额，这是第 1 轮的既定取舍（§4）。
- **库存计数持久化在 `gameshop_stock` / `gameshop_purchases` 两张表**（迁移 `0011`），
  与角色行、商城邮件在**同一个事务**里提交（`GameShopUpdate`）。
- **商城提示仍是中文硬编码**（`GameShopError::message()`），已登记在
  [`docs/i18n.md`](../i18n.md) §7.2 的收口清单。

### 1.1 R50 交付与证据（fail-closed）

| 项 | 结论 | 证据 |
|---|---|---|
| 服务端登录链 `S.GameShopInfo` | 已实现：`session.rs:1244`（`@2a7e77e3+R50`）在 `UserInformation`+物品模板之后、`ChangeAMode` 之前调用 `send_gameshop_info`；`session.rs:3174` 实现逐商品入队 | `target/round50/r50-b/README.md`、`04-test-gameshop-wire.log` |
| `S.GameShopStock` 时序 | 已实现：仅 `flush_gameshop` 提交成功后发（`session.rs:3106-3113`，2026-10-25 06:2x 复核）；失败/回滚分支零库存包 | `target/round50/r50-b/05-test-gameshop-failure.log` |
| 个人/全服库存公式 | 已实现，见 §8.1（`market.rs:299-314`） | 同上 + `mir-server/src/market.rs` 模块文档 |
| 事务失败回滚不刷 | 已实现：`rollback_gameshop_purchase` + 不发 `S.GameShopStock` | `gameshop_failure.rs`（1 passed） |
| 服务端测试 | `cargo test -p mir-server` exit 0：**879 passed / 0 failed**（55 个套件求和）；`gameshop` **21**、`gameshop_wire` **3**、`gameshop_failure` **1** | `target/round50/r50-b/08-test-mir-server.log`、`03/04/05-*.log` |
| 客户端面板 | 已实现：`ui/gameshop.rs`（680×500 默认、8 格/页、筛选/搜索/双币/空态/售罄）、`ui/mod.rs` 状态、`app.rs` `K` 键 | `target/round50/r50-c/c1-lib-gameshop.log`（15 passed）、`c2-gameshop-panel.log`（10 passed） |
| K 热键规则 | `app.rs:2951-2953`（`@2a7e77e3+R50`）：`"k" => { self.live_toggle_gameshop(); return; }`，在文本输入早退块之后、仅 `Mode::Live` 处理 | `target/round50/r50-c/k-hotkey-evidence.txt` |
| 素材 manifest | 已产出：82 条、ok 82、partial 0；`license = resource-policy-2026-10-20`；每项带 `source_path/bytes/sha256` | `docs/assets/gameshop-manifest.json`、`target/round50/r50-d/README.md` |
| i18n | no-test strict **0**（基线 15）；strict **60**（≤70）；`ci.ps1:136` 阈值 15→0 | `target/round50/r50-e/README.md` |

**PARTIAL / 不在本轮范围（不得写成已完成）**：

1. **面板素材的「全量同源」未达成**：82 条的商品图标是 `resource/data/items.wzl` 派生 + 社区库
   同索引补齐（`items.Lib` 104 / `hbacc DX` 37 / `Arcadia` 27），manifest 的 5 条 caveats 已登记；
   `clean-report` 里 3 个图标模板（index 533/589/590）与 1 个黑底占位模板（index 403）未被本批
   82 条使用。**不是**「全部图标都有授权与哈希」的完成态。
2. **无 pixel-diff / 视觉回归基线**：C 的截图（`target/round50/r50-c/gameshop-panel*.png` 4 张）
   是单次取证，没有跨轮像素对比。
3. **键位不可改**：`K` 是固定键；`KeybindOptions.GameShop` 的改键与 `RequireAlt` 语义留 R51。
4. **`Category` 二级筛选的服务端过滤**仍未做（客户端本地分节 + 分类下拉；
   服务端只按物品类型/名字/职业过滤）。
5. **发布包链路**只做了 manifest 指向性检查（`--verify-release` exit 0），没有在真机上跑
   release `game-assets/packs` 的完整商城面板往返。

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
一件物品可以在商城里以不同 `GIndex` 卖不同数量/价格。Crystal 的 `GIndex` 是
自增计数器（`Envir.cs:4342` `GIndex = ++GameshopIndex`），与 `Info.Index`（物品模板编号）
**是两个不同的数**——这一点是 §7.3 「GIndex 口径」差异的根源。

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

**任何一步失败都不改状态、不发包**（`gameshop_buy` 文档注释明写
「此时**不改任何状态、不发任何包**」）。是否给玩家提示取决于 `GameShopError::is_silent()`：
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
| 8 | 库存：`stock != 0` 时 `stock − 已购 ≥ quantity` | `OutOfStock`（「库存不足。」） | `:13758-13785`；Crystal 在拒绝时还会补一发 `GameShopStock(Product)`（`:13772`） |
| 9 | 支付方式：`p_type == 0` 元宝 / `1` 金币 | `UnsupportedPayment`（「不支持的支付方式。」） | `:13791`、`:13800`、`:13810-13813`（Crystal 回「钱不够」） |
| 10 | 该商品允许这种货币：`CanBuyCredit` / `CanBuyGold` | `CreditNotAllowed` / `GoldNotAllowed`（静默） | `:13794`、`:13803` |
| 11 | 余额：`单价 × 数量 ≤ 余额` | `NotEnoughCredit` / `NotEnoughGold`（静默） | 同上 |
| 12 | 扣款 +（金额非 0 时）`S.LoseCredit`/`S.LoseGold` | — | `:13824-13836` |
| 13 | 发货（§6） | `CannotGain`（兜底） | `:13870-13910` |
| 14 | 计数推进 + 返回 `GameShopPurchaseReport`（不落库） | — | `:13838-13861` |

> 小瑕疵：`RequiredType::MaxLevel`（等级过高）也复用 `WrongLevel`，提示是「等级不够」，
> 语义偏误导；错误类型里带了 `level`，修文案时一并处理（已登记在 `docs/i18n.md` §7.2）。

**顺序说明**：以 Crystal 的顺序为基线（数量 → 找商品 → 堆叠 → 库存 → 支付 → 余额），
在「找商品」之后**插入**职业/等级两道扩展校验（第 6/7 步），其余顺序与 Crystal 一致。
职业校验用**商品级** `GameShopItem.Class`（不是物品模板的 `RequiredClass` 位掩码）：
Crystal 商城的职业限定本来就是商品字段，用模板位掩码会把「商城限职业但物品不限」错杀。

`p_type` 的非法值（例如 2）归到 `UnsupportedPayment`，走市场页签的 `C.MarketBuy` 时
对应 `S.MarketFail` 的 `TooFar(7)`（见 §7.2）。

落盘失败（§8）时 `World::rollback_gameshop_purchase` 会**原样撤回**：
退回货币（`S.GainedCredit`/`S.GainedGold`）、从背包删掉本次到货、删除商城邮件与待写队列，
再给玩家一条「商城购买失败：已退回支付的货币与物品，请稍后再试。」。
**回滚路径不发 `S.GameShopStock`**（见 §8.4）。

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
  → S.GameShopStock{GIndex, StockLevel}          // R50：仅在 flush_gameshop 提交成功后
  → （无 S.MarketSuccess：C.GameshopBuy 是独立包，Crystal 同样不回执）

C.MarketBuy（商城页签兼容路径）
  → 同上扣款/发货/库存刷新
  → S.MarketSuccess{message:"你购买了 {name} × {N}。"}   // 仅在走市场页签时
```

失败：

* **静默类**（`GameShopError::is_silent()`：数量越界、超过 5 组、余额不足、
  不允许该支付方式）——**什么都不发**，与 Crystal 的裸 `return` 一致；
* 其余——`S.Chat{System}` 一条（`GameShopError::message()`），不扣款、不发货、**不发库存包**；
  走 `C.MarketBuy` 时改成 `S.MarketFail{Reason}`。

> 商城邮件与玩家邮件的落库走同一条 `MailWrite` 队列（`mail.rs`），
> 与角色行、库存计数在**同一个事务**里提交（§8）。

---

## 7. 客户端：页签与独立面板

### 7.1 打开页签（市场面板第 4 页）

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
  → 扣款 → 发货 → 库存刷新（§6）

C.MarketBuy{AuctionID = GIndex, BidPrice}   // 74，商城页签打开时的兼容路径
  → 数量固定 1、PType 固定 0（元宝）；成功 S.MarketSuccess，失败 S.MarketFail{Reason}
```

兼容路径的存在是为了「页签里直接点买」的旧客户端：`market_buy` 在
`MarketPanelType::GameShop` 时把 `auction_id` 当 `GIndex`、数量 1、元宝支付，
失败原因映射见 `GameShopError::market_reason()`：
余额不够 → `LowGold(4)`、背包满 → `NoSpace(5)`、其余 → `TooFar(7)`。

### 7.3 独立商城面板：Crystal 包链对齐（R50 已实现）

#### 7.3.1 服务端下发点（两条）

| 时机 | 实现 | Crystal 对应 |
|---|---|---|
| 登录进世界 | `session.rs:1244`（`@2a7e77e3+R50`）在 `UserInformation` + 物品模板之后、`ChangeAMode` 之前调用 `send_gameshop_info(character.id)`；`session.rs:3174` 先确认有目录，再从存储层**现读**两组计数（世界层不缓存），逐条 `World::game_shop_info_packets(&purchases)` 入队 | `PlayerObject.cs:1203-1206` `CheckConquest(); GetGameShop();`，`GetGameShop` 在 `:13913-13944` |
| 购买成功（事务提交后） | `session.rs:3106-3113`：`flush_gameshop(...)` 返回 `Ok(())` 才 `report.stock_packet()` → `S.GameShopStock` | `PlayerObject.cs:13861` `if (Product.Stock != 0) GameShopStock(Product);`（Crystal 在计数推进后同步调用，无事务概念） |

不发的情形（都已在代码注释与测试里固定）：

* 无商城目录（`load_game_data = false` 或无商城段）→ 登录链**一个包都不发**，登录包序不变；
* 已购计数读取失败 → 也**不发**（`session.rs:3181-3191` 打 `warn!`）：
  「宁缺勿错」——把「读不到」当 0 会向客户端谎报库存；
* 限量商品剩余为负（`stock − purchased < 0`）→ 该商品**不发**（见 7.3.2）；
* 购买事务失败 → 回滚且**零库存包**（见 §8.4）。

#### 7.3.2 单条 `S.GameShopInfo` 的库存口径

`market.rs:325-340`（`fn game_shop_info_packet`，2026-10-25 06:2x 复核行号）逐字复刻
`GetGameShop` 的循环体：

```text
Stock != 0（限量）:
    purchased = iStock ? 个人已购(角色) : 全服已购
    Stock − purchased >= 0 → 发 S.GameShopInfo{Item, StockLevel = Stock − purchased}
    Stock − purchased <  0 → 不发（Crystal 的 if 为假，直接跳过）
Stock == 0（不限量）:
    发 S.GameShopInfo{Item, StockLevel = Stock}   // 即 0，表示「∞」
```

`StockLevel = 0` 在这条链上有两种含义，客户端必须按「来源」区分：
**目录包里 `StockLevel == 0` 且 `Stock == 0` = 不限量**（显示 `∞`）；
**`S.GameShopStock` 里 `StockLevel == 0` = 限量商品卖完**（显示「售罄」）。
`net.rs::GameShopState::upsert` 的注释已把这个区分写死（0 档位不置售罄），
`apply_stock` 才置售罄。

#### 7.3.3 `GIndex` 口径：**本作有意偏离 Crystal**（T 核对点）

Crystal 的三处键不一致，必须先看清再谈「对齐」：

* 目录包 `S.GameShopInfo` 里 `Item.GIndex` 是商品自己的编号（`Envir.cs:4342`
  自增计数），客户端入列用 `GameShopInfoList.Add(p.Item)`（`GameScene.cs:6682`）；
* 库存包 `S.GameShopStock.GIndex` 却被赋值为 **`item.Info.Index`**（`PlayerObject.cs:13720`、
  `:13918-13941` 的计数读取也用 `Info.Index`）——**物品模板编号**；
* 客户端按 `GameShopInfoList[i].GIndex == p.GIndex` 匹配（`GameScene.cs:6690`）。

于是 Crystal 自己只在「商品 `GIndex` == 物品 `Info.Index`」时才能正确刷新库存；否则库存包
静默匹配不上（列表不更新，卖完也不会移除）。Crystal 的购买写回又用 `Product.GIndex`
（`PlayerObject.cs:13843-13858`），与读计数用 `Info.Index` 相互矛盾（本作第 49 轮已在
§8.1 记过这件事）。

**本作决策（R50 中书决策 1）**：全线统一用**商品 `GIndex`**——
`game_shop_info_packet` 下发 `item.g_index`，`stock_packet` 也下发
`report.g_index`（`market.rs` 的 `GameShopPurchaseReport::stock_packet` 注释：
「`g_index` 必须是商品 `GIndex`（中书决策 1：修正 Crystal 用 `Info.Index` 的缺陷）」），
客户端 `upsert`/`apply_stock` 都按 `item.g_index` 建键。

**因此不能把本作包链写成「与 Crystal 逐字节一致」**：目录包里 `Item.GIndex` 一致，
但库存包字段值在「商品编号 ≠ 物品模板编号」时**与 Crystal 不同**（本作能刷新，Crystal 不能）。
这是修正而非退化；若将来要对接 Crystal 客户端（`MirGameShopCell`），需要按
`item.Info.Index` 再发一份兼容包——**当前不做**，登记为差异。

#### 7.3.4 客户端面板（`crates/mir-client/src/ui/gameshop.rs`）

| 项 | 实现 | 证据 |
|---|---|---|
| 尺寸 | 默认 `680×500`（`PANEL_WIDTH/PANEL_HEIGHT`），最小 `300×180`，`PADDING=8`、`TITLE_HEIGHT=24` | `ui/gameshop.rs:58-98`；`gameshop-panel.png`、`gameshop-panel-320x240.png` |
| 分页 | `GRID_COLUMNS=4 × GRID_ROWS=2 = PAGE_SIZE 8`；翻页条 20px + 滚轮 | 测试 `paging_and_wheel_follow_8_cells` |
| 筛选 | 职业 6 项（`All` + 5 职业，`ClassFilter::ALL_FILTERS`）、分节 4 项（`All/Top/Deal/New`，`SectionFilter::ALL_SECTIONS`）、分类下拉（来自 `Category` 字符串） | 测试 `filters_follow_crystal_get_categories` |
| 搜索 | `TextField` 24 字符；空串 = 不筛 | 同上 |
| 双币 | 元宝/金币两个按钮（`PAY_CREDIT=0` / `PAY_GOLD=1`）；工具栏显示两种余额 | 测试 `purchase_flow_gold_and_credit` |
| 数量 | `1..=99`，客户端按「不超过 5 组」先夹一次（`pure_helpers_match_crystal_buy_and_quantity_rules`） | 同上 |
| 库存显示 | `>= 99` → `99+`；`0`（不限量）→ `∞`；售罄 → 「售罄」灰字 + 禁止购买 | `ui/gameshop.rs:26,157,275` |
| 空态 | 目录为空 → 「商城暂无商品」 | `ui/gameshop.rs:284`；`gameshop-panel-empty.png` |
| 断线 | 面板只在 `Mode::Live` 可开；断线时 `WorldSnapshot` 不再是 live，面板画「未连接」态 | 测试 `panel_draws_empty_disconnected_and_png_evidence`；`gameshop-panel-disconnected.png` |
| 缺图标 | 见 §7.3.5 | 测试断言 `assets_partial` + 占位方块 |
| 幂等 | 同一 `GIndex` 的重复/迟到 `S.GameShopInfo` 覆盖旧条目；`S.GameShopStock` 找不到 `GIndex` 时只推进版本号、不造条目 | `net.rs::GameShopState::upsert/apply_stock`；测试 `wire_packet_chain_catalog_gindex_idempotent_and_stock_refresh` |
| 状态隔离 | 商城的页/筛选/选中与市场页签互不影响 | 测试 `market_tab_state_is_isolated_from_gameshop` |

`K` 热键规则（`target/round50/r50-c/k-hotkey-evidence.txt` 逐条取证）：

* 入口 `app.rs:2853`（`@2a7e77e3+R50`）`handle_live_key`；
* 文本输入早退块 `app.rs:2920`（`if text_handled && text_consumed_key { …; return; }`）**先于**
  `K` 分支（`2920 < 2951`），所以聊天/搜索框输入 `k` 不会开面板；
* `K` 分支 `app.rs:2951-2953`：`"k" => { self.live_toggle_gameshop(); return; }`；
* helper 内唯一 toggle 调用 `app.rs:2272`（`live.ui.toggle_gameshop(class)`）；
* 两道守卫：`handle_live_key` 在焦点非空时先 `return`；`UiState::toggle_gameshop`
  自身 `if self.focus.is_some() { return false; }`；`K` 只在 `Mode::Live` 分支可达
  （登录/选角态走不到 `handle_live_key`）；
* 既有热键 `i/e/m/b/p/q/y/t/n/g/f/o/v/d/s/r` 已占用，`K` 空闲；键位重绑留 R51。

### 7.3.5 商品图标与素材 manifest（R50-D）

`ui/gameshop.rs:752-845` 的取值顺序（`assets_partial` :752、`manifest_icon_images` :789、
`icon_image_for` :826，2026-10-25 06:2x 复核行号）：

1. **包内 `ItemInfo.image` 优先**（`icon_image_for`）：非 0 直接用，与背包/市场图标同源；
2. 否则查 `docs/assets/gameshop-manifest.json` 的 `gindex → icon_image`；
3. 都没有 → 返回 0，面板画**占位方块**并置 `partial` 标记（`assets_partial()`：
   manifest 文件不存在时为 `true`）。**不伪造图标**；
4. manifest 里 `"partial": true` 的条目在解析阶段就被跳过（`manifest_icon_images`），
   回退到占位——与 D 的 `--check` fail-closed 口径一致（§11）。

manifest 字段与授权口径见 [`docs/assets/licensing.md`](../assets/licensing.md) §5、
[`docs/assets/game-assets.md`](../assets/game-assets.md) §5.1.1；
`license.policy_id = resource-policy-2026-10-20`，每项带
`source_path / bytes / sha256 / icon_image / icon_pack`。

### 7.4 与 Crystal 客户端语义的差异：**移除条目 vs 售罄禁购**

Crystal：`GameScene.cs:6686-6696` 收到 `StockLevel == 0` 的 `S.GameShopStock` 时
`GameShopInfoList.Remove(...)`——卖完的商品**从列表消失**，客户端不区分
「不限量」与「卖完」以外的状态，也**没有本地禁用购买**的逻辑（只有列表存在性）。

本作（`net.rs::GameShopState::apply_stock`）：

* `stock_level <= 0` → `stock_level = 0; sold_out = true`，**保留条目**；
* 面板对 `sold_out` 的条目画「售罄」灰字、`购买` 按钮禁用（点击被 `BuyProblem`
  拦下，不发包）；
* 只为「目录包 `StockLevel == 0` 且 `Stock == 0`」显示 `∞`（不限量），**不会**把
  不限量当售罄（`upsert` 的 0 档位不置售罄）。

选择理由：商城面板是「目录 + 库存」两包分开发的，若按 Crystal 删除条目，则后续
`S.GameShopInfo` 重发（重连/重开）会让商品「复活」，玩家会看到列表抖动；
保留条目 + 禁用是更稳定的可观察行为，也更符合「售罄」这一业务语义。
代价：本作**永远不会**因为卖完而减少列表长度（Crystal 会），分页数因此可能不同。

---

## 8. 库存与持久化

### 8.1 迁移 `0011_gameshop.sql` 与公式

| 表 | 列 | Crystal 对应 |
|---|---|---|
| `gameshop_stock` | `g_index INTEGER PRIMARY KEY`、`purchased BIGINT`、`updated_at` | `Envir.GameshopLog`（`Envir.cs:133`，`Dictionary<int,int>`，存档 `:2598-2599`、读档 `:3054`） |
| `gameshop_purchases` | `character_id BIGINT REFERENCES characters ON DELETE CASCADE`、`g_index INTEGER`、`quantity BIGINT`、`updated_at`，主键 `(character_id, g_index)` | `CharacterInfo.GSpurchases`（`CharacterInfo.cs:100`，存档 `:358`、`:556-559`） |

**个人 / 全服库存公式**（`market.rs:300` `GameShopPurchases::purchased`、
`:311` `remaining`；2026-10-25 06:2x 复核行号）：

```text
量      purchased(product) = iStock ? gameshop_purchases[(character_id, g_index)]
                                    : gameshop_stock[(g_index)]
剩余    remaining(product) = (stock != 0) ? i64(stock) − purchased(product) : None(不限量)

登录下发   remaining == None            → StockLevel = 0（∞）
          remaining >= 0               → StockLevel = remaining
          remaining <  0（超卖历史）     → 不发该条
购买校验   stock != 0 时要求 remaining >= quantity，否则 OutOfStock
购买成功   remaining_after == None       → 不发库存包（不限量不刷）
          remaining_after <  0          → 不发（防御性兜底）
          remaining_after >= 0          → 发 S.GameShopStock{StockLevel = remaining_after}
```

口径要点：

* `Stock == 0` 表示不限量，**不写任何计数行**（`PlayerObject.cs:13782-13785`）；
* `iStock == true`（个人库存）读 `gameshop_purchases`，否则读 `gameshop_stock`
  （`PlayerObject.cs:13708-13715`）；
* **键统一用 `GIndex`**（与 §7.3.3 一致）。Crystal 自己不一致：读计数用 `Info.Index`
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

### 8.3 与 Crystal 存档的对照

Crystal 把 `GameshopLog` 写进 `Server.MirADB` 的全局段（`Envir.cs:2598-2599`），
`GSpurchases` 随角色存档（`CharacterInfo.cs:556-559`）；本作拆成两张表，
语义相同、写入粒度更小（只写本次购买的行）。字段级说明见
[`docs/store/schema.md`](../store/schema.md)。

### 8.4 R50 库存包时序与失败零包

| 阶段 | 行为 | 证据 |
|---|---|---|
| 校验失败 | `gameshop_buy` 返回 `Err` → 会话层只发提示（或静默）→ **无 `S.GameShopStock`** | `market.rs::gameshop_buy` 文档注释；`gameshop.rs` 单测 |
| 扣款/发货后落盘失败 | `flush_gameshop` 返回 `Err` → `rollback_gameshop_purchase`（退货币、按模板+数量反向扣到货、删商城邮件与待写队列）→ **零库存包** | `session.rs:3124-3148`、`tests/gameshop_failure.rs`（1 passed） |
| 落盘成功 | 先 `flush_gameshop`，`Ok(())` 之后才 `stock_packet()` | `session.rs:3106-3113` |
| 客户端可见性 | 不会出现「库存减了但没扣款」：失败时世界层回滚且不刷库存 | 同上 |

> 与 Crystal 的差别：Crystal 在**拒绝购买**时反而会补发一次 `GameShopStock`
> （`PlayerObject.cs:13772`，让客户端的库存显示回到真实值）；本作在拒绝时不发，
> 因为客户端的库存只在「自己买成功」后可能失真，而失败路径世界层未改状态，
> 不需要校正。若将来发现客户端库存与服务端漂移（多人并发限量商品），再按 Crystal
> 补发校正包——**当前不做**，登记为差异。

---

## 9. 与 Crystal 的差异（有意）

1. **元宝是角色级**（§4），Crystal 是账号级。
2. **投递背包优先**，Crystal 一律寄商城邮件（§6）。
3. **服务端补职业/等级校验**（§5 第 6/7 步），Crystal 只在客户端按
   `GameShopItem.Class` 字符串分组过滤，服务端不查。
4. **库存键统一 `GIndex`**（§8.1），Crystal 读写键不一致；连带的
   `S.GameShopStock.GIndex` 也与 Crystal 不同（§7.3.3，T 核对点）。
5. **数量/组数越界与余额不足都是静默的**：Crystal 对 `Quantity < 1 || > 99`、超过 5 组、
   余额不足都是裸 `return`，本作在 `GameShopError::is_silent()` 里显式登记这几类，
   会话层同样**不发提示**（可观察行为一致，只是错误类型可测）。
6. **非法 `PType`**：Crystal 回「钱不够」（`YouDontHaveEnoughCurrency`）；本作回
   「不支持的支付方式」。
7. **商城有两套 UI**：市场面板第 4 页签（第 49 轮）+ 独立面板（R50-C）。
   Crystal 的独立 `GameshopDialog` 与它自己的市场页签商城分支（服务端被注释掉，
   `PlayerObject.cs:8385-8390`）是历史遗留的两套；本作两套都通，且共用同一条购买/事务链。
8. **售罄保留条目**：Crystal 客户端收到 `StockLevel == 0` 直接 `Remove`；本作保留条目
   并置「售罄」禁购（§7.4）。分页长度因此可能不同。
9. **拒绝购买不补发库存校正包**（Crystal `:13772` 会补发），见 §8.4。
10. **购买提示走 `TextKey`**：`GameShopError::message(language)` 的 13 条提示第 3 轮就
    收口为第 1 类 TextKey（`market.rs` 32 处，见 [`docs/i18n.md`](../i18n.md) §7.1/§7.2），
    不是硬编码中文。R50-E 在 `market.rs` 里只改了职业名匹配表（5 条别名），与本条无关；
    仍未做的是「金额/库存的**参数化**提示」（现在提示里不带具体数值）。

---

## 10. 测试清单

### 10.1 服务端（`target/round50/r50-b/`）

`crates/mir-server/src/market.rs` 模块内单元测试覆盖 `GameShopPayment::from_wire`、
`GameShopPurchases::remaining`、`GameShopError::market_reason`、`game_shop_stacks` 的分堆规则。

`crates/mir-server/tests/gameshop.rs`（第 2 轮新增，**21 个用例**；R50-B 由 15 增至 21）：

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
| 16-21 | R50-B 新增 6 条（目录下发/库存包/失败零包/`StockLevel` 三态等，见 `target/round50/r50-b/README.md` §测试） | 以 `03-test-gameshop.log` 的 **21 passed / 0 failed** 为准 |

`crates/mir-server/tests/gameshop_wire.rs`（R50-B 新增，**3 个用例**）：
`04-test-gameshop-wire.log` = `3 passed; 0 failed`。
`crates/mir-server/tests/gameshop_failure.rs`（R50-B 新增，**1 个用例**）：
`05-test-gameshop-failure.log` = `1 passed; 0 failed`。

全量回归：`cargo test -p mir-server` → `08-test-mir-server.log`，55 个套件求和
**879 passed / 0 failed**，`EXIT_CODE=0`。

> 测试 helper 的容差调整：`gameshop.rs` 测试 helper 的容差 200→512，有 Lead 批准
> （`target/round50/r50-b/10-gameshop-helper-diff.txt`）。这是测试夹具口径，不改生产逻辑。

### 10.2 客户端（`target/round50/r50-c/`）

| 套件 | 结果 | 日志 |
|---|---|---|
| `cargo test -p mir-client --lib ui::gameshop` | **15 passed** / 0 failed / 273 filtered | `c1-lib-gameshop.log` |
| `cargo test -p mir-client --test gameshop_panel` | **10 passed** / 0 failed | `c2-gameshop-panel.log` |
| `cargo test -p mir-client --test round2_panels`（显式 `TEMP/TMP` 覆盖） | **28 passed** / 0 failed | `c3-round2-panels-tmp-override.log` |
| 同上，未覆盖 `TEMP` 的一次运行 | 27 passed / **1 failed**：`settings_roundtrip_through_disk_survives_restart`（`round2_panels.rs:198`，`std::env::temp_dir()` 写权限） | `c3-round2-panels.log`；环境问题，与商城改动无关 |
| `cargo test -p mir-client --lib ui::labels` | **7 passed** | `c4-lib-labels.log` |

10 个面板用例：`wire_packet_chain_catalog_gindex_idempotent_and_stock_refresh`、
`layout_hit_tests_and_tiny_frames_stay_on_screen`、`filters_follow_crystal_get_categories`、
`hotkey_toggle_guard_single_panel_and_focus`、`paging_and_wheel_follow_8_cells`、
`purchase_flow_gold_and_credit`、`pure_helpers_match_crystal_buy_and_quantity_rules`、
`market_tab_state_is_isolated_from_gameshop`、`panel_draws_empty_disconnected_and_png_evidence`、
`k_hotkey_wiring_evidence`。

截图证据：`gameshop-panel.png`、`gameshop-panel-320x240.png`、`gameshop-panel-empty.png`、
`gameshop-panel-disconnected.png`、`gameshop-market-coexist.png`（同屏两套 UI）。

另外：`tests/skills.rs` 覆盖技能侧；`tests/market_flow.rs` 覆盖市场页签的 TCP 端到端；
`tests/round2_panels.rs` 覆盖商城页签的列表/购买命令。

---

## 11. 未做 / 明确不做（R50 之后）

* **`K` 键改键与 `RequireAlt` 修饰键**：固定 `K`，重绑留 R51（`KeybindSettings.cs` 全表未接）。
* **服务端 `Category` 过滤**：客户端本地分节/分类下拉，服务端只按类型/名字/职业过滤。
* **拒绝购买补发库存校正包**（Crystal `:13772`）：当前不发，见 §8.4。
* **Crystal 客户端互通**：`S.GameShopStock.GIndex` 用商品编号，Crystal 客户端按
  `Info.Index` 匹配；若要兼容需要另发一份，当前不做（§7.3.3）。
* **商城条目编辑/定价管理**（Crystal 走 GM 命令 `AddGameShopItem` 等，`Envir.cs:4340`、`:4380`）。
* **元宝充值/充值渠道**（协议里没有）。
* **跨服/多世界共享库存**（`gameshop_stock` 是单库全局）。
* **发布包真机往返**：只做了 `--verify-release`（entries=827 / icons ok 82），
  没有在 release `game-assets/packs` 上跑完整面板往返（§1.1 PARTIAL 5）。

---

## 12. R50 交付索引与残余（原「R50 前置条件」已完成，保留结论）

第 49 轮本节登记的三项前置**本轮已完成**，证据如下；若 T/队长复核时发现证据不成立，
本节整体降级为 PARTIAL，不得引用 §1.1 的「已实现」结论。

| 原前置 | 完成情况 | 证据 |
|---|---|---|
| 12.1 素材授权与哈希 manifest | 已交付：82 条 / ok 82 / partial 0，每项 `source_path/bytes/sha256`，policy `resource-policy-2026-10-20`；`--check` exit 0 | `docs/assets/gameshop-manifest.json`、`target/round50/r50-d/README.md`、[`licensing.md`](../assets/licensing.md) §5 |
| 12.2 真实包链 | 已交付：`S.GameShopInfo` 由真实 `Server.MirDB` 商城段驱动（`World::game_shop_info_packets`），登录链与购买刷新都有 3+1 条集成测试 | `target/round50/r50-b/README.md`、`04/05-*.log` |
| 12.3 事务失败包序 | 已交付：失败回滚 + 零库存包；`gameshop_failure.rs` 1 passed | `target/round50/r50-b/05-test-gameshop-failure.log` |

**残余 / 留给 T 的核对点（fail-closed）**：

1. `S.GameShopStock.GIndex` 用商品 `GIndex` 而非 Crystal 的 `Info.Index`（§7.3.3）——
   是否符合中书决策 1 的书面口径，请核对决策原文（本子单只能读到代码注释与
   `r50-b/README.md`）。
2. 客户端面板**只做单次截图**、无像素回归基线；「售罄保留条目」与 Crystal 的
   「移除条目」差异（§7.4）请在验收单里确认按本作口径。
3. §10.1 第 16-21 条只给了「6 条新增」的总数，逐条用例名以
   `target/round50/r50-b/README.md` 为准；若 T 需要本文逐条列出，请回填后我再补。
4. **`market.rs` 归属**：R50-E 改的 `game_shop_class_allows`（职业名匹配表 → 别名表，
   见 [`docs/i18n.md`](../i18n.md) §7.8 deviation 2）与 R50-B 的商城改动**同文件**，
   T 核对 `git diff --stat` 时不要把整个文件的行数都算给 B 或都算给 E。
   另：`GameShopError::message(language)` 早已是第 1 类 TextKey，R50 未改动；
   不要因为「扫描 0」就写「商城文案本轮刚完成 i18n」。
