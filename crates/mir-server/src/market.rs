//! 摆摊 / 寄售 / 拍卖（Crystal `PlayerObject.Consignment` 区，
//! `Server/MirObjects/PlayerObject.cs:8217-8802`）。
//!
//! Crystal 把「摆摊」做成 NPC 托管的市场：玩家把物品**寄售**在 NPC 处（`MarketItemType.Consign`，
//! 一口价），或**拍卖**（`MarketItemType.Auction`，可出价），物品离开背包、进 `Envir.Auctions`
//! 全局列表，其他玩家通过市场窗口搜索/翻页/购买。没有「站在街边摆摊」的对象。
//!
//! 逐条对照 Crystal（参考根目录 `github\Crystal`，下文只写相对路径）：
//!
//! | 本模块 | Crystal |
//! |---|---|
//! | [`CONSIGNMENT_COST`] / [`AUCTION_COST`] | `Globals.ConsignmentCost = 5000` / `Globals.AuctionCost = 5000`（`Shared/Globals.cs:43`、`:46`） |
//! | [`MIN_CONSIGNMENT`] / [`MAX_CONSIGNMENT`] | `Globals.MinConsignment = 5000` / `MaxConsignment = 50000000`（`Globals.cs:44-45`） |
//! | [`MIN_STARTING_BID`] / [`MAX_STARTING_BID`] | `Globals.MinStartingBid = 0` / `MaxStartingBid = 50000`（`Globals.cs:47-48`） |
//! | [`CONSIGNMENT_LENGTH_DAYS`] | `Globals.ConsignmentLength = 7`（`Globals.cs:42`） |
//! | [`COMMISSION`] | `Globals.Commission = 0.05F`（`Globals.cs:39`） |
//! | [`SEARCH_DELAY_MS`] | `Globals.SearchDelay = 500`（`Globals.cs:41`） |
//! | [`MARKET_PAGE_SIZE`] | `for (int i = 0; i < 10; i++)`（`PlayerObject.cs:8356`、`:8405`） |
//! | [`MARKET_KEY`] | `NPCScript.MarketKey = "[@MARKET]"`（`Server/MirObjects/NPC/NPCScript.cs:57`） |
//! | [`CONSIGN_KEY`] | `NPCScript.ConsignKey = "[@CONSIGN]"`（`NPCScript.cs:56`） |
//! | [`World::open_market`] | `NPCScript.cs` 的 `case MarketKey:`（`:1046-1049`） |
//! | [`World::consign_item`] | `PlayerObject.ConsignItem`（`:8218-8316`） |
//! | [`World::market_search`] | `PlayerObject.MarketSearch`（`:8424-8448`）+ `GetMarket`（`:8376-8422`） |
//! | [`World::market_page`] | `PlayerObject.MarketPage`（`:8327-8374`） |
//! | [`World::market_buy`] | `PlayerObject.MarketBuy`（`:8450-8589`） |
//! | [`World::market_sell_now`] | `PlayerObject.MarketSellNow`（`:8591-8666`） |
//! | [`World::market_get_back`] | `PlayerObject.MarketGetBack`（`:8668-8793`） |
//! | [`World::tick_market`] | 兼容空壳（`world.rs` 每 tick 调一次，O(1)）；过期标记已并入
//!   [`World::process_auctions`]（`Envir.Process` 的 `Expired` 标记，`Server/MirEnvir/Envir.cs:2396`） |
//! | [`World::process_auctions`] | `Envir.ProcessAuction`（`:2388-2418`，每 10 分钟 `:2354-2358`） |
//! | [`Auction`] | `Server/MirDatabase/AuctionInfo.cs:5-117` |
//!
//! # 包序
//!
//! | 语义 | 客户端包 | 服务端包 | Crystal |
//! |---|---|---|---|
//! | 打开寄售面板 | `C.CallNPC{Key="[@CONSIGN]"}` | `S.NPCConsign{}`（154） | `NPCScript.cs:1043-1045` |
//! | 上架 | `C.ConsignItem{UniqueID, Price, Type}`（70） | `S.ConsignItem{UniqueID, Success}`（157）→ 成功时 `S.LoseGold{手续费}`（68） | `PlayerObject.cs:8218-8316` |
//! | 打开市场 | `C.CallNPC{Key="[@MARKET]"}` | `S.NPCMarket{Listings, Pages, UserMode}`（155） | `NPCScript.cs:1046-1049` → `GetMarket` |
//! | 搜索/切页签 | `C.MarketSearch{Match, Type, UserMode, MinShape, MaxShape, MarketType}`（71） | `S.NPCMarket` | `PlayerObject.cs:8424-8448` |
//! | 刷新 | `C.MarketRefresh{}`（72） | `S.NPCMarket` | `MirConnection.cs:1692-1697` |
//! | 翻页 | `C.MarketPage{Page}`（73） | `S.NPCMarketPage{Listings}`（156） | `PlayerObject.cs:8327-8374` |
//! | 购买/出价 | `C.MarketBuy{AuctionID, BidPrice}`（74） | `S.LoseGold` → `S.GainedItem` → `S.MarketSuccess`（159），失败 `S.MarketFail{Reason}`（158） | `PlayerObject.cs:8450-8589` |
//! | 提前成交拍卖 | `C.MarketSellNow{AuctionID}`（76） | `S.MarketSuccess` | `PlayerObject.cs:8591-8666` |
//! | 下架/收款 | `C.MarketGetBack{Mode, AuctionID}`（75） | `S.GainedItem`/`S.GainedGold` → `S.MarketSuccess` | `PlayerObject.cs:8668-8793` |
//! | 商城购买 | `C.GameshopBuy{GIndex, Quantity, PType}`（133） | `S.LoseCredit`/`S.LoseGold` → `S.GainedItem`（背包满转邮件 → `S.Chat{Hint}`） | `PlayerObject.cs:13725-13911` |
//!
//! # 与 Crystal 的**有意**差异
//!
//! 1. **落盘方式**：Crystal 把挂单写进 `MirADB` 的全局 `Auctions` 段
//!    （`Envir.SaveAccounts`，`Server/MirEnvir/Envir.cs:2591-2594`；`AuctionInfo.Save`，
//!    `AuctionInfo.cs:71-87`），读档时靠 `BindCharacter` 重新绑定卖家/买家
//!    （`Envir.cs:3018-3029`）。本作落 `market_auctions` 表（`migrations/0009_market.sql`）：
//!    启动时 [`World::load_auctions`]、每次市场操作成功后整表快照回写
//!    （[`persist_auctions`]）、下线再补一次；卖家/买家名字随条目冗余保存。
//!    启动时会先按 [`World::process_auctions`] 结算停机期间到期的拍卖（有有效出价的
//!    自动成交），再用 [`World::purge_expired_auctions`] 把剩下的过期挂单清掉
//!    （物品/最高出价按系统邮件退回）；Crystal 只把 `Expired` 置真、等卖家 `MarketGetBack`
//!    取回（`Envir.cs:2396`、`PlayerObject.cs:8668-8793`）。
//!    运行期 [`World::process_auctions`] 由 `server.rs` 的 tick 任务按
//!    [`AUCTION_SCAN_INTERVAL_MS`] 节流调用（Crystal 是每 10 分钟一次）。
//! 2. **商城（GameShop）走市场面板页签**：Crystal 有两条商城入口——独立的
//!    `GameshopDialog`（`S.GameShopInfo`/`S.GameShopStock` 下发目录，`PlayerObject.GetGameShop`，
//!    `PlayerObject.cs:13913-13944`）和市场面板的 `MarketPanelType.GameShop` 页签
//!    （`GetMarket` 里的分支被注释掉了，`:8385-8390`，页签原本永远空白）。本作按任务要求
//!    只做**页签**：`S.NPCMarket` 的每条 `ClientAuction` 里 `auction_id = 商品 GIndex`、
//!    `item_type = GameShop`、`price = CreditPrice`，每页 [`MARKET_PAGE_SIZE`] 条。
//!    购买主路径是 `C.GameshopBuy`（包 133）；页签打开时也接受 `C.MarketBuy`（数量 1、
//!    元宝支付），回 `S.MarketSuccess`/`S.MarketFail`——等价于 Crystal 那段被注释掉的设计。
//!    商品目录来自 `Server.MirDB` 的商城段（`GameData::game_shop`，Crystal `Envir.GameShopList`，
//!    `Envir.cs:2911-2923`）；没有数据库时是空列表，购买回「商城里没有商品」。
//! 3. **商城的投递方式**：Crystal 买什么都是**整单寄邮件**（`PlayerObject.cs:13900-13907`），
//!    本作按任务要求「背包有空位就直接进背包（`S.GainedItem`），放不下的才转系统邮件」。
//!    元宝余额沿用角色行的 `credit`（Crystal 存在账号 `AccountInfo.Credit` 上，见
//!    `migrations/0011_gameshop.sql` 的「元宝」一节）。
//! 4. **`Envir.MessageAccount` 只给在线角色发**：Crystal 是写进账号的消息队列
//!    （`Envir.MessageAccount`），离线也能收到；本作只给在线角色发系统聊天，离线丢弃。
//! 5. **`ConsignItem` 末尾的重复回执**：Crystal 在成功分支 `Enqueue(p)` 之后，函数末尾
//!    又 `Enqueue(p)` 一次（`PlayerObject.cs:8304-8305`、`:8315`）。本作只发一次。
//! 6. **「搜索」状态放在世界层**：Crystal 的 `Search` / `PageSent` / `MatchName` / `MatchType` /
//!    `UserMatch` / `MinShapes` / `MaxShapes` / `MarketPanelType` / `SearchTime` 是
//!    `PlayerObject` 的字段（`PlayerObject.cs:132-139`）；本作放在
//!    [`World::market_searches`]，下线即清（与 Crystal 随对象销毁等价）。
//! 7. **多语言**：市场/商城的全部玩家可见文本（`S.MarketSuccess`、聊天行、系统邮件、
//!    `ClientAuction.seller` 标签）都走 `mir_core::i18n` 的 `market_*` / `gameshop_*` 文本键，
//!    按**查看者/收件人**语言渲染（`World::text` / `World::format`，离线收件人回落
//!    服务器默认 `zh-cn`，见 [`World::text_for_character`] 的说明）。
//!    `thiserror` 的 `#[error]` 文本属于开发者可见的日志，保持中文不动。
//!
//! # 商城链路的几条约定
//!
//! * **`C.MarketBuy` 的商城分支只在会话层**：`session::on_market_buy` 看到当前面板是
//!   `GameShop` 时把 `AuctionID` 当 `GIndex`、按「1 份 + 元宝」转发到
//!   [`World::gameshop_buy`]（Crystal `PlayerObject.cs:8458-8486` 的设计）。直接调
//!   `World::market_buy` 不会走商城——它只处理 `Envir.Auctions` 里的挂单。
//! * **不发 `S.GameShopInfo` / `S.GameShopStock`**：本作只做市场面板页签，页签不显示库存；
//!   库存不足时按 Crystal 回聊天提示（`YouBuyMoreThanAvailable`，`:13776`），不推
//!   `S.GameShopStock`（客户端的独立商城窗口不在本轮范围）。
//! * **`Count`/`StackSize` 为 0 时按 1 处理**：Crystal 直接拿 `Product.Count` 算数量，
//!   0 会算出一封空邮件；本作 `max(1)` 兜底。
//! * **落盘失败要回滚**：`gameshop_buy` 先扣款发货、会话随后在单事务里落盘；事务失败时
//!   会话调 [`World::rollback_gameshop_purchase`] 把内存状态退回，保证「事务成功 ⇔
//!   内存变更保留」（计数只由该事务写，只记日志会导致库存回退后超卖）。
//!
//! # 持久化不变量（刑部 t7 F1/F2/F3 修复后）
//!
//! 1. **串行快照**：所有会话的市场落盘都经过 [`flush_market_update`]；它在独立的
//!    [`MARKET_FLUSH_LOCK`] 内**重新取**最新挂单快照，再写库。任何时刻都不会出现
//!    「旧快照后提交、整表覆盖新状态」。
//! 2. **单事务**：一次市场操作（上架/买断/出价/提前成交/取回）的持久化副作用——
//!    当前角色行、系统邮件、挂单全量快照——由 [`crate::store::MarketUpdate`] 装进
//!    同一个数据库事务。崩溃只会看到「全部没写」或「全部写了」，不会复制/丢失物品或金币。
//! 3. **启动清理至多一次**：过期挂单的退回邮件与删除挂单也在同一个
//!    `apply_market_update` 事务里；失败则全部回滚，重试不会重复退款；成功后挂单已删，
//!    下次启动不会再产生同一批退款。
//!
//! 对手方（卖家/被超价的买家）只通过 **邮件** 收取物品/金币，因此不需要跨会话写对方
//! 角色行；当前操作者自己的背包/金币变化则由第 2 条保证落盘。
//!
//! # 简化
//!
//! * 简化：Crystal 的 `Search` 是 `List<AuctionInfo>` 引用；本作存拍卖 **ID**（`Vec<u64>`），
//!   每次按 ID 回查。原因：条目可能在两次请求之间被买走/下架，按 ID 回查能自然得到
//!   「不存在」的处理（与 Crystal 的 `Envir.Auctions.Contains(auction)` 检查等价，`:8519`）。
//! * 简化：Crystal 的 `Match` 用 `info.Item.Info.Name.Replace(" ", "").IndexOf(MatchName)`；
//!   本作先把名字与关键字都去掉空格再 `contains`（大小写不敏感），对中文与英文等价。

use mir_core::DateTimeBinary;
use mir_core::TextKey;
use mir_core::enums::{
    BindMode, ItemType, MarketCollectionMode, MarketItemType, MarketPanelType, PetMode,
    RequiredType,
};
use mir_core::i18n::{format_text, text};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ClientAuction, GameShopItem, UserItem};
use mir_protocol::packets::server::{
    ConsignItem, GainedCredit, GainedGold, LoseCredit, LoseGold, MarketFail, MarketSuccess,
    NPCConsign, NPCMarket, NPCMarketPage,
};

use mir_db::ItemInfo;

use crate::items::{decode_slots, encode_slots, friendly_name};
use crate::store::{
    AccountStore, AuctionRecord, Character, GameShopPurchaseRecord, GameShopStockRecord,
    GameShopUpdate, MarketUpdate, StoreError,
};
use crate::world::{Player, World};

pub use crate::npc::MARKET_KEY;
use crate::npc::NPC_TALK_RANGE;

/// 寄售手续费：`Globals.ConsignmentCost = 5000`（`Shared/Globals.cs:43`）。
pub const CONSIGNMENT_COST: u32 = 5_000;

/// 寄售价格下限：`Globals.MinConsignment = 5000`（`Globals.cs:44`）。
pub const MIN_CONSIGNMENT: u32 = 5_000;

/// 寄售价格上限：`Globals.MaxConsignment = 50000000`（`Globals.cs:45`）。
pub const MAX_CONSIGNMENT: u32 = 50_000_000;

/// 拍卖起拍价下限：`Globals.MinStartingBid = 0`（`Globals.cs:47`）。
pub const MIN_STARTING_BID: u32 = 0;

/// 拍卖起拍价上限：`Globals.MaxStartingBid = 50000`（`Globals.cs:48`）。
pub const MAX_STARTING_BID: u32 = 50_000;

/// 拍卖手续费：`Globals.AuctionCost = 5000`（`Globals.cs:46`）。
pub const AUCTION_COST: u32 = 5_000;

/// 寄售/拍卖挂单有效期（天）：`Globals.ConsignmentLength = 7`（`Globals.cs:42`）。
///
/// 保留这个 Crystal 常量用于文档/对照；本作运营口径按 t119 使用
/// [`AUCTION_EXPIRE_MS`]（48 小时）。
pub const CONSIGNMENT_LENGTH_DAYS: i64 = 7;

/// 挂单超时（毫秒）：默认 **48 小时**（t119；Crystal 没有独立的
/// `Settings.AuctionDuration`，其 `Globals.ConsignmentLength` 是 7 天，本作按运营要求取 48h）。
pub const AUCTION_EXPIRE_MS: i64 = 48 * 60 * 60 * 1_000;

/// 单次批量上架/下架的上限（本作扩展；Crystal 没有批量接口）。
pub const MAX_AUCTION_BATCH: usize = 10;

/// 成交佣金比例：`Globals.Commission = 0.05F`（`Globals.cs:39`）。
pub const COMMISSION: f64 = 0.05;

/// 搜索/翻页节流：`Globals.SearchDelay = 500`（`Globals.cs:41`）。
pub const SEARCH_DELAY_MS: i64 = 500;

/// 每页条目数：Crystal 固定 10（`PlayerObject.cs:8356`、`:8405`）。
pub const MARKET_PAGE_SIZE: usize = 10;

/// 拍卖到期结算（[`World::process_auctions`]）的最小扫描间隔（毫秒）。
///
/// **方案**：固定间隔节流，扫描状态放在 `server.rs` 的 tick 任务里（`next_auction_scan`），
/// 不放进 [`World`]（避免为一次扫描给世界加字段）。
///
/// **理由**：
/// * Crystal 是 `if (Time >= auctionTime) { auctionTime = Time + Settings.Minute * 10; ProcessAuction(); }`
///   （`Server/MirEnvir/Envir.cs:2354-2358`），即每 10 分钟扫一遍；本作 tick 是 100ms，
///   照抄会每 tick 全表扫描 + 可能的邮件生成。
/// * 取 60 秒而不是 10 分钟：挂单有效期是 [`CONSIGNMENT_LENGTH_DAYS`] 天，到期后多等
///   几十秒没有实质影响，但比 Crystal 的 10 分钟窗口更及时；同时把「全表扫描 + 潜在的
///   邮件/落盘」限制在每分钟最多一次（1000 条挂单也只有一次 O(n) 遍历）。
/// * 不做到期时间索引：索引要在上架/下架/成交/载入/回滚五处维护，收益只是把 O(n) 的
///   每分钟一次遍历变成 O(log n)，在本作挂单量级（单表快照本来就每次市场操作全量回写）
///   下不值得引入额外一致性负担。
/// * **每 tick 的 [`World::tick_market`] 已经是 O(1) 空壳**：到期标记也并入这个节流扫描，
///   所以 100ms 的 `World::tick` 不再遍历挂单表（合同要求「避免每 tick 全表扫描」）。
///   代价是 `Expired` 标记最多晚一个周期出现（卖家的取回分支另有
///   [`World::auction_expired_at`] 兜底，功能不缺）。
pub const AUCTION_SCAN_INTERVAL_MS: i64 = 60_000;

/// 拍卖成交落盘失败后的重试退避（毫秒）。
///
/// 成交在内存里已经发生（挂单删除 + 两封邮件入队），`server.rs` 的 tick 任务用
/// `sale_flush_pending` 继续重试 [`flush_auction_sales`]，直到事务提交。这个闸门保证
/// **DB 故障时不会按 100ms tick 的频率捶库**：第一次尝试是即时的，失败后等
/// [`SALE_FLUSH_RETRY_MS`] 再试（与 `docs/world/market.md` 写的「5 秒退避」一致）。
pub const SALE_FLUSH_RETRY_MS: i64 = 5_000;

/// 打开寄售（丢物品定价）面板的 NPC 页面键：`NPCScript.ConsignKey = "[@CONSIGN]"`（`NPCScript.cs:56`）。
pub const CONSIGN_KEY: &str = "[@CONSIGN]";

/// 商城单次购买的数量上限：`C.GameshopBuy.Quantity` 是 `byte`，Crystal 只拒 `0` 与 `> 99`
/// （`PlayerObject.cs:13727` 的 `if (Quantity < 1 || Quantity > 99) return;`）。
pub const GAMESHOP_MAX_QUANTITY: u8 = 99;

/// 单次购买最多占用的堆叠数：Crystal `if (((decimal)(Quantity * Product.Count) / StackSize) > 5) return;`
/// （`PlayerObject.cs:13756`）。
pub const GAMESHOP_MAX_STACKS: u64 = 5;

/// 商城支付方式（`C.GameshopBuy.PType`，Crystal `PlayerObject.cs:13791`、`:13800`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameShopPayment {
    /// `PType == 0`：元宝（Crystal `AccountInfo.Credit`，回包 `S.LoseCredit`）。
    Credit,
    /// `PType == 1`：金币（Crystal `AccountInfo.Gold`，回包 `S.LoseGold`）。
    Gold,
}

impl GameShopPayment {
    /// 线上编号 → 支付方式；`None` 表示客户端发了非法值（Crystal 回「钱不够」提示）。
    #[must_use]
    pub const fn from_wire(p_type: i32) -> Option<Self> {
        match p_type {
            0 => Some(Self::Credit),
            1 => Some(Self::Gold),
            _ => None,
        }
    }
}

/// 商城已购计数快照（Crystal `CharacterInfo.GSpurchases` + `Envir.GameshopLog`）。
///
/// 世界层不缓存这两份计数（Crystal 存在 `PlayerObject.Info` 与 `Envir` 上）：每次购买前由
/// 会话从存储层读一份，连同购买请求一起交给 [`World::gameshop_buy`]，成功后再和角色行
/// 同一个事务写回（见 [`flush_gameshop_update`]）。这样「重启后库存正确」只依赖数据库，
/// 不依赖进程内状态。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameShopPurchases {
    /// 按角色记的已购数量（Crystal `Info.GSpurchases`）：`GIndex` → 已购。
    pub individual: std::collections::HashMap<i32, i64>,
    /// 全服已购数量（Crystal `Envir.GameshopLog`）：`GIndex` → 已购。
    pub server: std::collections::HashMap<i32, i64>,
}

impl GameShopPurchases {
    /// 用存储层读到的两份计数构造。
    #[must_use]
    pub fn from_records(
        stock: &[GameShopStockRecord],
        purchases: &[GameShopPurchaseRecord],
    ) -> Self {
        Self {
            individual: purchases
                .iter()
                .map(|row| (row.g_index, row.quantity))
                .collect(),
            server: stock
                .iter()
                .map(|row| (row.g_index, row.purchased))
                .collect(),
        }
    }

    /// 某件商品在本玩家视角下已购多少（Crystal `GameShopStock`，`PlayerObject.cs:13708-13715`）：
    /// `iStock` 为真按角色算，否则按全服算。
    #[must_use]
    pub fn purchased(&self, product: &GameShopItem) -> i64 {
        let table = if product.i_stock {
            &self.individual
        } else {
            &self.server
        };
        table.get(&product.g_index).copied().unwrap_or(0)
    }

    /// 剩余库存；`None` 表示不限量（`GameShopItem.Stock == 0`，`PlayerObject.cs:13782-13785`）。
    #[must_use]
    pub fn remaining(&self, product: &GameShopItem) -> Option<i64> {
        (product.stock != 0).then(|| i64::from(product.stock) - self.purchased(product))
    }
}

/// 一次成功购买的结算结果（会话层据此把「角色行 + 元宝 + 库存计数」写进同一个事务）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameShopPurchaseReport {
    /// 商品 `GIndex`。
    pub g_index: i32,
    /// 商品名（物品模板的友好名，日志与提示用）。
    pub product_name: String,
    /// 购买份数。
    pub quantity: u8,
    /// 支付方式。
    pub payment: GameShopPayment,
    /// 总价（`单价 × 份数`）。
    pub cost: u32,
    /// 全服已购累计（Crystal `GameshopLog[gIndex]`；Crystal 每次购买都推进它，`:13851-13859`）。
    pub server_purchased: i64,
    /// 按角色已购累计；只在 `iStock && Stock != 0` 时才是 `Some`（`PlayerObject.cs:13838-13849`）。
    pub individual_purchased: Option<i64>,
    /// 剩余库存（`None` = 不限量）。
    pub remaining: Option<i64>,
    /// 直接进背包的份数。
    pub gained: usize,
    /// 改寄系统邮件的份数（背包放不下，`PlayerObject.cs:13900-13907` 的同款兜底）。
    pub mailed: usize,
    /// 直接进背包的投递明细（落盘失败时 [`World::rollback_gameshop_purchase`] 按它撤回）。
    ///
    /// **不记 `UniqueID`**：`gain_item` 会把可堆叠物品并进已有堆
    /// （`crates/mir-server/src/items.rs:1411-1435`），新物品的 `UniqueID` 根本不会出现在
    /// 背包里；回滚只能按「模板 + 数量」反向扣减。
    pub delivered: Vec<GameShopDelivery>,
    /// 商城邮件编号（背包满时才有；落盘失败时按它撤回邮件）。
    pub mail_id: Option<u64>,
}

/// 一件商城投递明细（回滚用）：从哪个模板放进了多少数量。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameShopDelivery {
    /// 物品模板 ID。
    pub item_index: i32,
    /// 实际放进背包的数量（可堆叠物品会并进已有堆，所以按数量而不是 `UniqueID` 记账）。
    pub count: u16,
}

impl GameShopPurchaseReport {
    /// 转成存储层的两组计数行（角色行与邮件由 [`flush_gameshop_update`] 补齐）。
    #[must_use]
    pub fn to_records(
        &self,
        character_id: i64,
    ) -> (Vec<GameShopPurchaseRecord>, Vec<GameShopStockRecord>) {
        let purchases = self.individual_purchased.map_or_else(Vec::new, |quantity| {
            vec![GameShopPurchaseRecord {
                character_id,
                g_index: self.g_index,
                quantity,
            }]
        });
        let stock = vec![GameShopStockRecord {
            g_index: self.g_index,
            purchased: self.server_purchased,
        }];
        (purchases, stock)
    }
}

/// 商城购买被拒绝的原因（提示包由会话层发，世界层只返回原因）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GameShopError {
    /// 对象不在世界里。
    #[error("object {0} is not in the world")]
    NotInWorld(u32),
    /// 没有加载 `Server.MirDB`（或数据库里没有商城段）。
    #[error("game shop data is not loaded")]
    NoGameData,
    /// 商城里没有这个 `GIndex`。
    #[error("game shop has no product {g_index}")]
    ProductNotFound {
        /// 客户端给的 `GIndex`。
        g_index: i32,
    },
    /// 数量不在 `1..=99`。
    #[error("invalid purchase quantity {0} (1..=99)")]
    InvalidQuantity(u8),
    /// 超过单次 5 组的上限（`PlayerObject.cs:13756`）。
    #[error("at most {stacks} stacks per purchase")]
    TooManyStacks {
        /// 本次会占用的堆叠数。
        stacks: u64,
    },
    /// 职业不符（`ItemInfo.RequiredClass`，`PlayerObject.cs:13725` 之前由客户端过滤，
    /// 本作在服务端也校验一次）。
    #[error("class mismatch: {class}")]
    WrongClass {
        /// 需要的职业描述。
        class: String,
    },
    /// 等级不符（`ItemInfo.RequiredType == Level/MaxLevel` + `RequiredAmount`）。
    #[error("level mismatch: requires {required}, current {level}")]
    WrongLevel {
        /// 模板里的等级要求。
        required: u8,
        /// 角色当前等级。
        level: u16,
    },
    /// 库存不足。
    #[error("out of stock: {remaining} remaining")]
    OutOfStock {
        /// 剩余数量（可能为负，表示已经超卖）。
        remaining: i64,
    },
    /// 该商品不允许用元宝购买（`GameShopItem.CanBuyCredit == false`，`:13794`）。
    #[error("this item cannot be purchased with credits")]
    CreditNotAllowed,
    /// 该商品不允许用金币购买（`GameShopItem.CanBuyGold == false`，`:13803`）。
    #[error("this item cannot be purchased with gold")]
    GoldNotAllowed,
    /// 元宝不够。
    #[error("insufficient credits: requires {needed}, holding {held}")]
    NotEnoughCredit {
        /// 需要。
        needed: u64,
        /// 持有。
        held: i64,
    },
    /// 金币不够。
    #[error("insufficient gold: requires {needed}, holding {held}")]
    NotEnoughGold {
        /// 需要。
        needed: u64,
        /// 持有。
        held: i64,
    },
    /// `PType` 不是 0/1（Crystal 回「钱不够」，`:13810-13813`）。
    #[error("unsupported payment method {0}")]
    UnsupportedPayment(i32),
    /// 背包既放不下、也寄不了邮件（理论上不会发生，留作兜底）。
    #[error("item does not fit in the inventory")]
    CannotGain,
}

impl GameShopError {
    /// 面向玩家的提示文案（Crystal 用 `ServerTextKeys` 本地化；本作走 `mir_core::i18n`
    /// 的 `gameshop_*` 文本键，按玩家当前语言渲染）。
    #[must_use]
    pub fn message(&self, language: mir_core::Language) -> String {
        match self {
            Self::NotInWorld(_) => text(language, TextKey::GameshopNotInWorld).to_owned(),
            Self::NoGameData => text(language, TextKey::GameshopUnavailable).to_owned(),
            Self::ProductNotFound { .. } => {
                text(language, TextKey::GameshopProductNotFound).to_owned()
            }
            Self::InvalidQuantity(_) => text(language, TextKey::GameshopInvalidQuantity).to_owned(),
            Self::TooManyStacks { stacks } => format_text(
                language,
                TextKey::GameshopTooManyStacks,
                &[
                    ("max", &GAMESHOP_MAX_STACKS.to_string()),
                    ("stacks", &stacks.to_string()),
                ],
            ),
            Self::WrongClass { .. } => text(language, TextKey::GameshopWrongClass).to_owned(),
            Self::WrongLevel { required, .. } => format_text(
                language,
                TextKey::GameshopWrongLevel,
                &[("required", &required.to_string())],
            ),
            Self::OutOfStock { .. } => text(language, TextKey::GameshopOutOfStock).to_owned(),
            Self::CreditNotAllowed | Self::GoldNotAllowed => {
                text(language, TextKey::GameshopPaymentNotAllowed).to_owned()
            }
            Self::NotEnoughCredit { .. } => {
                text(language, TextKey::GameshopNotEnoughCredit).to_owned()
            }
            Self::NotEnoughGold { .. } => text(language, TextKey::GameshopNotEnoughGold).to_owned(),
            Self::UnsupportedPayment(_) => {
                text(language, TextKey::GameshopUnsupportedPayment).to_owned()
            }
            Self::CannotGain => text(language, TextKey::GameshopCannotGain).to_owned(),
        }
    }

    /// `C.GameshopBuy` 路径上这个失败**是否静默**（不发任何包、不聊天）。
    ///
    /// 照抄 Crystal：数量越界与超过 5 组是静默 `return`（`PlayerObject.cs:13727`、`:13756`），
    /// 余额不足 / `CanBuyCredit`、`CanBuyGold` 为假同样是静默（`canAfford == false` →
    /// `:13863-13866`）。只有「商品不存在」（`:13751`）、「库存不足」（`:13776`）、
    /// 「支付方式非法」（`:13811`）才 `ReceiveChat`。
    ///
    /// 职业/等级是本作扩展的校验（Crystal 服务端不查），失败给一条提示更便于排查。
    #[must_use]
    pub const fn is_silent(&self) -> bool {
        matches!(
            self,
            Self::InvalidQuantity(_)
                | Self::TooManyStacks { .. }
                | Self::NotEnoughCredit { .. }
                | Self::NotEnoughGold { .. }
                | Self::CreditNotAllowed
                | Self::GoldNotAllowed
        )
    }

    /// 走市场面板（`C.MarketBuy`）时对应的 `S.MarketFail` 编号。
    ///
    /// Crystal 的商城分支只用 4（金币不足）/5（背包满）两个（`PlayerObject.cs:8465-8474`）；
    /// 本作把「找不到商品/库存不足/职业等级不符」归到 7（`TooFar`，语义是「列表里没有这条」）。
    #[must_use]
    pub const fn market_reason(&self) -> MarketFailReason {
        match self {
            Self::NotEnoughCredit { .. } | Self::NotEnoughGold { .. } => MarketFailReason::LowGold,
            Self::CannotGain => MarketFailReason::NoSpace,
            Self::NotInWorld(_)
            | Self::NoGameData
            | Self::ProductNotFound { .. }
            | Self::InvalidQuantity(_)
            | Self::TooManyStacks { .. }
            | Self::WrongClass { .. }
            | Self::WrongLevel { .. }
            | Self::OutOfStock { .. }
            | Self::CreditNotAllowed
            | Self::GoldNotAllowed
            | Self::UnsupportedPayment(_) => MarketFailReason::TooFar,
        }
    }
}

/// 商城商品的职业限制（`GameShopItem.Class`）。
///
/// Crystal 的商城职业限定是**商品级**字段：客户端按它分组过滤
/// （`Client/MirScenes/Dialogs/GameshopDialog.cs:288-373`，`:508` 用
/// `GameScene.User.Class.ToString()` 选中自己职业的页签），取值是 `"Warrior"` /
/// `"Wizard"` / `"Taoist"` / `"Assassin"` / `"Archer"` / `"All"`。
/// 服务端**不查**（`PlayerObject.GameshopBuy` 里没有任何职业判断）。
///
/// 本作在服务端补这一道（防改包），口径：
/// * 空串 / `All` / `None` → 不限职业（`None` 是管理工具新建商品的默认值，
///   `Server.MirForms/Database/GameShop.cs:432`，按「没填」处理）；
/// * 其它值按职业名大小写不敏感比较（中文职业名也认，方便本地化数据）。
#[must_use]
pub fn game_shop_class_allows(product: &GameShopItem, class: mir_core::enums::MirClass) -> bool {
    let wanted = product.class.trim();
    if wanted.is_empty()
        || wanted.eq_ignore_ascii_case("all")
        || wanted.eq_ignore_ascii_case("none")
    {
        return true;
    }
    let english = crate::npc::class_name(class);
    if wanted.eq_ignore_ascii_case(english) {
        return true;
    }
    // 中文职业名（`npc::class_name` 是英文，这里补一张小表）。
    let chinese = match class {
        mir_core::enums::MirClass::Warrior => "战士",
        mir_core::enums::MirClass::Wizard => "法师",
        mir_core::enums::MirClass::Taoist => "道士",
        mir_core::enums::MirClass::Assassin => "刺客",
        mir_core::enums::MirClass::Archer => "弓箭手",
    };
    wanted == chinese
}

/// 一条市场条目（Crystal `AuctionInfo`，`Server/MirDatabase/AuctionInfo.cs:5-117`）。
///
/// 字段与 Crystal 一一对应；`seller_name` / `current_buyer_name` 是 Crystal 通过
/// `CharacterInfo` 引用拿到的，本作把名字随条目存下来（离线卖家也能显示）。
#[derive(Debug, Clone, PartialEq)]
pub struct Auction {
    /// `AuctionInfo.AuctionID`（`AuctionInfo.cs:12`）。
    pub id: u64,
    /// `AuctionInfo.Item`（`:14`）。
    pub item: UserItem,
    /// `AuctionInfo.ConsignmentDate`（`:15`）。
    pub consignment_date: DateTimeBinary,
    /// `AuctionInfo.Price`（`:16`）：寄售是一口价，拍卖是起拍价。
    pub price: u32,
    /// `AuctionInfo.CurrentBid`（`:16`）：当前最高出价；寄售恒为 0。
    pub current_bid: u32,
    /// `AuctionInfo.SellerIndex`（`:18`，本作按角色 ID 记）。
    pub seller_character_id: i64,
    /// 卖家角色名（Crystal `SellerInfo.Name`）。
    pub seller_name: String,
    /// `AuctionInfo.CurrentBuyerIndex`（`:18`，本作按角色 ID 记）。
    pub current_buyer_character_id: Option<i64>,
    /// 当前最高出价者角色名（Crystal `CurrentBuyerInfo.Name`）。
    pub current_buyer_name: Option<String>,
    /// `AuctionInfo.Expired`（`:21`）。
    pub expired: bool,
    /// `AuctionInfo.Sold`（`:21`）。
    pub sold: bool,
    /// `AuctionInfo.ItemType`（`:23`）。
    pub item_type: MarketItemType,
}

impl Auction {
    /// 卖家展示文本：Crystal `AuctionInfo.GetSellerLabel`（`AuctionInfo.cs:89-102`）。
    ///
    /// 五个标签走 `market_label_*` 文本键，按**查看者**的语言渲染
    /// （`ClientAuction.seller` 是客户端直接显示的字符串）。
    #[must_use]
    pub fn seller_label(&self, user_match: bool, language: mir_core::Language) -> String {
        match self.item_type {
            // `return userMatch ? (Sold ? "Sold" : (Expired ? "Expired" : "For Sale")) : SellerInfo.Name;`
            MarketItemType::Consign => {
                if user_match {
                    if self.sold {
                        text(language, TextKey::MarketLabelSold).to_owned()
                    } else if self.expired {
                        text(language, TextKey::MarketLabelExpired).to_owned()
                    } else {
                        text(language, TextKey::MarketLabelOnSale).to_owned()
                    }
                } else {
                    self.seller_name.clone()
                }
            }
            // `return userMatch ? (Sold ? "Sold" : (Expired ? "Expired" : CurrentBid > Price ? "Bid Met" : "No Bid")) : SellerInfo.Name;`
            MarketItemType::Auction => {
                if user_match {
                    if self.sold {
                        text(language, TextKey::MarketLabelSold).to_owned()
                    } else if self.expired {
                        text(language, TextKey::MarketLabelExpired).to_owned()
                    } else if self.current_bid > self.price {
                        text(language, TextKey::MarketLabelHasBid).to_owned()
                    } else {
                        text(language, TextKey::MarketLabelNoBid).to_owned()
                    }
                } else {
                    self.seller_name.clone()
                }
            }
            MarketItemType::GameShop => String::new(),
        }
    }

    /// 线上条目：Crystal `AuctionInfo.CreateClientAuction`（`AuctionInfo.cs:104-115`）。
    #[must_use]
    pub fn to_client_auction(
        &self,
        user_match: bool,
        language: mir_core::Language,
    ) -> ClientAuction {
        ClientAuction {
            auction_id: self.id,
            item: self.item.clone(),
            seller: self.seller_label(user_match, language),
            // `Price = ItemType == MarketItemType.Auction ? CurrentBid : Price;`
            price: if self.item_type == MarketItemType::Auction {
                self.current_bid
            } else {
                self.price
            },
            consignment_date: self.consignment_date,
            item_type: self.item_type,
        }
    }

    /// 成交后卖家实际到手的金币：Crystal `Math.Max(0, cost - cost * Globals.Commission)`
    /// （`PlayerObject.cs:8640`、`:8720`），`uint` 截断。
    #[must_use]
    pub fn after_commission(cost: u32) -> u32 {
        let cost_f = f64::from(cost);
        let net = cost_f - cost_f * COMMISSION;
        if net <= 0.0 { 0 } else { net as u32 }
    }

    /// 转成数据库行：物品编成 `encode_slots(&[Some(item)])` 单格字节块
    /// （与背包/邮件/行会仓库共用同一套 `UserItem[]` 编解码）。
    ///
    /// 对应 Crystal `AuctionInfo.Save`（`AuctionInfo.cs:71-87`）逐字段写 `BinaryWriter`
    /// 的那一段。
    #[must_use]
    fn to_record(&self) -> AuctionRecord {
        AuctionRecord {
            id: self.id,
            item: encode_slots(&[Some(self.item.clone())]),
            consignment_date: self.consignment_date,
            price: self.price,
            current_bid: self.current_bid,
            seller_character_id: self.seller_character_id,
            seller_name: self.seller_name.clone(),
            current_buyer_character_id: self.current_buyer_character_id,
            current_buyer_name: self.current_buyer_name.clone(),
            expired: self.expired,
            sold: self.sold,
            item_type: self.item_type,
        }
    }

    /// 从数据库行还原；物品块解不出来时返回 `None`（调用方跳过并记日志）。
    ///
    /// 对应 Crystal `AuctionInfo(BinaryReader, …)`（`AuctionInfo.cs:47-69`）逐字段读回。
    fn from_record(record: AuctionRecord) -> Option<Self> {
        let mut slots = decode_slots(&record.item).ok()?;
        if slots.len() != 1 {
            return None;
        }
        let item = slots.pop().flatten()?;
        Some(Self {
            id: record.id,
            item,
            consignment_date: record.consignment_date,
            price: record.price,
            current_bid: record.current_bid,
            seller_character_id: record.seller_character_id,
            seller_name: record.seller_name,
            current_buyer_character_id: record.current_buyer_character_id,
            current_buyer_name: record.current_buyer_name,
            expired: record.expired,
            sold: record.sold,
            item_type: record.item_type,
        })
    }
}

/// 一个玩家的市场浏览状态（Crystal `PlayerObject` 的 8 个字段，`PlayerObject.cs:132-139`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketSearchState {
    /// `PlayerObject.Search`：搜索结果的拍卖 ID（按 `Envir.Auctions` 的顺序）。
    pub listings: Vec<u64>,
    /// `PlayerObject.PageSent`（`:138`）。
    pub page_sent: i32,
    /// `PlayerObject.SearchTime`（节流，`:137`）。
    pub search_time: i64,
    /// `PlayerObject.MatchName`（`:133`，已去掉空格）。
    pub match_name: String,
    /// `PlayerObject.MatchType`（`:134`）。
    pub match_type: ItemType,
    /// `PlayerObject.MinShapes`（`:136`）。
    pub min_shapes: i16,
    /// `PlayerObject.MaxShapes`（`:136`）。
    pub max_shapes: i16,
    /// `PlayerObject.UserMatch`（`:132`）。
    pub user_match: bool,
    /// `PlayerObject.MarketPanelType`（`:135`）。
    pub panel: MarketPanelType,
}

impl Default for MarketSearchState {
    fn default() -> Self {
        // Crystal 的默认值：`short MinShapes, MaxShapes` 都是 0，`MatchType` 是
        // `ItemType.Nothing`（0），`UserMatch=false`，`MarketPanelType=Market`（0）。
        Self {
            listings: Vec::new(),
            page_sent: 0,
            search_time: 0,
            match_name: String::new(),
            match_type: ItemType::Nothing,
            min_shapes: 0,
            max_shapes: 0,
            user_match: false,
            panel: MarketPanelType::Market,
        }
    }
}

/// `S.MarketFail{Reason}` 的编号（Crystal `PlayerObject.cs:8454-8788`；
/// 客户端文案见 `Client/MirScenes/GameScene.cs:5669-5708`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketFailReason {
    /// 0：死亡状态不能用市场。
    Dead,
    /// 1：没有通过 NPC 打开市场。
    NoDialog,
    /// 2：物品已经卖出。
    Sold,
    /// 3：物品已过期或已被下架。
    Expired,
    /// 4：金币不够。
    LowGold,
    /// 5：背包放不下。
    NoSpace,
    /// 6：不能买自己挂的东西。
    OwnItem,
    /// 7：离 NPC 太远 / 找不到条目。
    TooFar,
    /// 8：金币上限，收不下。
    CannotHoldGold,
    /// 9：出价低于当前最高价。
    BidTooLow,
    /// 10：拍卖已经结束。
    AuctionEnded,
}

impl MarketFailReason {
    /// 线上编号。
    #[must_use]
    pub const fn reason(self) -> u8 {
        match self {
            Self::Dead => 0,
            Self::NoDialog => 1,
            Self::Sold => 2,
            Self::Expired => 3,
            Self::LowGold => 4,
            Self::NoSpace => 5,
            Self::OwnItem => 6,
            Self::TooFar => 7,
            Self::CannotHoldGold => 8,
            Self::BidTooLow => 9,
            Self::AuctionEnded => 10,
        }
    }
}

/// 市场操作被拒绝的原因（提示包已经由 [`World`] 的方法发出，这里只给会话层写日志）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MarketError {
    /// 对象不在世界里。
    #[error("object {0} is not in the world")]
    NotInWorld(u32),
    /// 死亡状态。
    #[error("the market cannot be used while dead")]
    Dead,
    /// 没通过 NPC 打开对应面板（Crystal `NPCPage == null` / 键不对）。
    #[error("the market panel was not opened through an NPC")]
    NoDialog,
    /// 当前地图上找不到对话过的 NPC。
    #[error("NPC {object_id} in dialogue not found")]
    NoSuchNpc {
        /// NPC 对象 ID。
        object_id: u32,
    },
    /// 离 NPC 太远。
    #[error("too far from NPC {object_id} (max {range})")]
    OutOfRange {
        /// NPC 对象 ID。
        object_id: u32,
        /// 上限。
        range: i32,
    },
    /// 面板类型不对（`MarketPanelType::Market` 不能用来上架）。
    #[error("panel type {0:?} cannot list items")]
    InvalidPanel(MarketPanelType),
    /// 价格越界。
    #[error("price {price} is outside [{min}, {max}]")]
    PriceOutOfRange {
        /// 给定价格。
        price: u32,
        /// 下限。
        min: u32,
        /// 上限。
        max: u32,
    },
    /// 金币不够（手续费/购买）。
    #[error("insufficient gold: requires {needed}, holding {held}")]
    NotEnoughGold {
        /// 需要。
        needed: u32,
        /// 持有。
        held: i64,
    },
    /// 背包里找不到这件物品。
    #[error("item {0} not found in the inventory")]
    ItemNotFound(u64),
    /// 物品绑定「禁止出售」（`BindMode.DontSell`，`PlayerObject.cs:8288`）。
    #[error("this item cannot be sold")]
    ItemUnsellable,
    /// 背包/金币收不下。
    #[error("cannot receive")]
    CannotGain,
    /// 搜索列表里没有这条拍卖（没搜过/已被买走）。
    #[error("auction {0} not found in the search list")]
    ListingNotFound(u64),
    /// 已经卖出。
    #[error("auction {0} has been sold")]
    ListingSold(u64),
    /// 已经过期。
    #[error("auction {0} has expired")]
    ListingExpired(u64),
    /// 自己的挂单。
    #[error("you cannot buy your own auction")]
    OwnListing,
    /// 出价低于当前最高价。
    #[error("bid {bid} is below the current price {current}")]
    BidTooLow {
        /// 当前价。
        current: u32,
        /// 出价。
        bid: u32,
    },
    /// 拍卖还没有人出价。
    #[error("no bids on this auction yet")]
    NoBid,
    /// 拍卖已结束。
    #[error("the auction has ended")]
    AuctionEnded,
    /// 节流（`Globals.SearchDelay`）。
    #[error("searching too frequently")]
    Throttled,
    /// 这个面板还没做（商城）。
    #[error("the game shop panel is not implemented")]
    UnsupportedPanel,
    /// 批量上架/下架超过单次上限（本作扩展）。
    #[error("batch size {count} exceeds the maximum {max}")]
    BatchTooLarge {
        /// 请求条数。
        count: usize,
        /// 上限。
        max: usize,
    },
}

// ---------------------------------------------------------------------------
// 世界层：市场
// ---------------------------------------------------------------------------

impl World {
    /// 全部市场条目（只读；测试与工具用）。顺序等价于 Crystal 的 `Envir.Auctions`
    /// （`AddFirst`，所以最新的在最前面）。
    #[must_use]
    pub fn auctions(&self) -> &[Auction] {
        &self.auctions
    }

    /// 按 ID 取一条（只读）。
    #[must_use]
    pub fn auction(&self, auction_id: u64) -> Option<&Auction> {
        self.auctions
            .iter()
            .find(|auction| auction.id == auction_id)
    }

    /// 某个玩家的市场浏览状态（只读；测试与工具用）。
    #[must_use]
    pub fn market_state_of(&self, player_id: u32) -> Option<&MarketSearchState> {
        self.market_searches.get(&player_id)
    }

    /// 某个角色挂着的条目 ID（**按世界顺序**，新->旧）。
    #[must_use]
    pub fn auctions_of_character(&self, character_id: i64) -> Vec<u64> {
        self.auctions
            .iter()
            .filter(|auction| auction.seller_character_id == character_id)
            .map(|auction| auction.id)
            .collect()
    }

    /// 启动时把数据库里的市场挂单灌进世界（对应 Crystal `Envir.LoadAccounts` 重建
    /// `Envir.Auctions` 的那段，`Server/MirEnvir/Envir.cs:3018-3029`）。
    ///
    /// 重复调用会**整份替换**当前列表（与存储层的全量快照语义一致）。解码失败的条目
    /// 只记日志跳过（Crystal 的 `BindItem` 失败同样静默丢弃，`Envir.cs:3025`）；
    /// 但它的 ID 仍会推进 `next_auction_id`，避免之后复用旧号。
    /// `next_auction_id` 按 `max(id) + 1` 续号（Crystal 直接存 `NextAuctionID`，
    /// `Envir.cs:2591`；本作没有单独的计数表）。
    ///
    /// 返回成功载入的条数。
    pub fn load_auctions(&mut self, records: Vec<AuctionRecord>) -> usize {
        self.auctions.clear();
        let mut loaded = 0;
        for record in records {
            let id = record.id;
            self.next_auction_id = self.next_auction_id.max(id.saturating_add(1));
            if let Some(auction) = Auction::from_record(record) {
                self.auctions.push(auction);
                loaded += 1;
            } else {
                tracing::warn!(auction = id, "市场挂单的物品块无法解码，跳过");
            }
        }
        // 新 → 旧，等价于 Crystal `AddFirst` 的链表顺序（`PlayerObject.cs:8302`）。
        self.auctions
            .sort_by_key(|auction| std::cmp::Reverse(auction.id));
        loaded
    }

    /// 当前全部挂单的存储快照（新 → 旧），用于全量回写 `market_auctions`。
    #[must_use]
    pub fn auction_snapshot(&self) -> Vec<AuctionRecord> {
        self.auctions.iter().map(Auction::to_record).collect()
    }

    /// 当前待落盘邮件数量（测试/观测用；只读，不 drain 队列）。
    #[must_use]
    pub fn pending_mail_writes(&self) -> usize {
        self.mail_writes.len()
    }

    /// `/metrics` 用：当前有多少个玩家持有「市场搜索态」（每个在线玩家至多 1 条，下线即 `remove`）。
    ///
    /// 慢泄漏观测（第 24 轮）：这个数应当**跟着在线人数走**（`<= players_online`），
    /// 若它随运行时间单调上涨而在线人数不变，就是清理路径漏了。
    #[must_use]
    pub fn market_search_count(&self) -> usize {
        self.market_searches.len()
    }

    /// 启动清理：把已过期（`expired == true` 或已到
    /// [`CONSIGNMENT_LENGTH_DAYS`] 天）且未售出的挂单**退回并删除**。
    ///
    /// 对应 Crystal `Envir.ProcessAuction` 的过期判定（`Server/MirEnvir/Envir.cs:2396`：
    /// `!Expired && !Sold && Now >= ConsignmentDate.AddDays(ConsignmentLength)`），
    /// 但清理时机与收尾不同：Crystal 只置 `Expired`，等卖家 `MarketGetBack` 取回
    /// （`PlayerObject.cs:8668-8793`）；本作在服务器启动时处理积压的过期挂单：
    ///
    /// * 物品按系统邮件退回卖家（`Envir.MailCharacter`，`PlayerObject.cs:8652` 同款）；
    /// * 已有最高出价的，把 `current_bid` 按系统邮件退回买家（`PlayerObject.cs:8564-8569`）；
    /// * 从 `World::auctions` 移除（调用方随后回写快照）。
    ///
    /// 不直接删行是为了**不吞物品/金币**；退回邮件走 `World::mail_writes`，启动方负责
    /// 用 [`crate::mail::persist_mail_writes`] 落库。
    ///
    /// 返回被清理的挂单 ID（新 → 旧）。
    pub fn purge_expired_auctions(&mut self, now_ms: i64) -> Vec<u64> {
        let expired: Vec<Auction> = self
            .auctions
            .iter()
            .filter(|auction| {
                !auction.sold && (auction.expired || Self::auction_expired_at(auction, now_ms))
            })
            .cloned()
            .collect();
        if expired.is_empty() {
            return Vec::new();
        }

        let mut purged = Vec::with_capacity(expired.len());
        for auction in &expired {
            let item_name = friendly_name(&self.item_info(auction.item.item_index).map_or_else(
                || auction.item.item_index.to_string(),
                |info| info.name.clone(),
            ));
            // 先把最高出价退回买家，再把物品退回卖家（与 `TakeAuction` 的收尾顺序一致）。
            if auction.current_bid > 0
                && let Some(buyer) = auction.current_buyer_character_id
            {
                let message = self.format_for_character(
                    buyer,
                    TextKey::MarketMailBidRefundExpired,
                    &[
                        ("item", &item_name),
                        ("gold", &auction.current_bid.to_string()),
                    ],
                );
                self.mail_system_to_character(buyer, None, auction.current_bid, &message);
            }
            let message = self.format_for_character(
                auction.seller_character_id,
                TextKey::MarketMailItemExpired,
                &[("item", &item_name)],
            );
            self.mail_system_to_character(
                auction.seller_character_id,
                Some(auction.item.clone()),
                0,
                &message,
            );
            purged.push(auction.id);
        }

        self.auctions
            .retain(|auction| !purged.contains(&auction.id));
        tracing::info!(count = purged.len(), "已清理过期的市场挂单");
        purged
    }

    /// 打开市场列表：`NPCScript.cs` 的 `case MarketKey:`（`:1046-1049`）。
    ///
    /// ```csharp
    /// player.UserMatch = false;
    /// player.GetMarket(string.Empty, ItemType.Nothing);
    /// ```
    ///
    /// 先发 `S.NPCMarket{Listings, Pages, UserMode}`，客户端据此打开市场窗口。
    pub fn open_market(&mut self, player_id: u32) -> Result<(), MarketError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(MarketError::NotInWorld(player_id));
        };
        if player.dead {
            return Err(MarketError::Dead);
        }
        self.market_searches
            .entry(player_id)
            .or_default()
            .user_match = false;
        self.get_market(player_id, "", ItemType::Nothing);
        Ok(())
    }

    /// 打开寄售面板：`NPCScript.cs` 的 `case ConsignKey:`（`:1043-1045`）→ `S.NPCConsign{}`。
    pub fn open_consign(&mut self, player_id: u32) -> Result<(), MarketError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(MarketError::NotInWorld(player_id));
        };
        if player.dead {
            return Err(MarketError::Dead);
        }
        self.send_to_player(
            player_id,
            &ServerPacket::NPCConsign(NPCConsign {}).encode_frame(),
        );
        Ok(())
    }

    /// 上架（寄售或拍卖）：`C.ConsignItem`，Crystal `PlayerObject.ConsignItem`
    /// （`PlayerObject.cs:8218-8316`）。
    ///
    /// 校验顺序照抄：
    /// 1. 死亡或没在对话 NPC → 回 `S.ConsignItem{Success=false}`（`:8222-8226`）；
    /// 2. 面板类型：`Consign` 校验 [`MIN_CONSIGNMENT`]..=[`MAX_CONSIGNMENT`] 与
    ///    [`CONSIGNMENT_COST`]；`Auction` 校验 [`MIN_STARTING_BID`]..=[`MAX_STARTING_BID`]
    ///    与 [`AUCTION_COST`]；其它面板失败（`:8228-8263`）；
    /// 3. NPC 必须还在同图 [`NPC_TALK_RANGE`] 内——Crystal 在这一支**直接 return、不发包**
    ///    （`:8265-8269`），本作照抄；
    /// 4. 背包里按 `UniqueID` 找物品（`:8271-8286`）；
    /// 5. `BindMode.DontSell` 拒绝（`:8288-8292`）；
    /// 6. 建条目 → 从背包移除 → 扣手续费（`S.LoseGold`）→ 回 `S.ConsignItem{Success=true}`
    ///    （`:8299-8313`）。
    ///
    /// 返回新条目的 ID。
    pub fn consign_item(
        &mut self,
        player_id: u32,
        unique_id: u64,
        price: u32,
        panel: MarketPanelType,
    ) -> Result<u64, MarketError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(MarketError::NotInWorld(player_id));
        };
        // `if (Dead || NPCPage == null)`（`:8222-8226`）
        if player.dead || player.npc.npc_object_id == 0 {
            self.send_consign_ack(player_id, unique_id, false);
            return Err(if player.dead {
                MarketError::Dead
            } else {
                MarketError::NoDialog
            });
        }
        // 面板与价格/手续费（`:8228-8263`）
        let (item_type, cost, min_price, max_price) = match panel {
            MarketPanelType::Consign => (
                MarketItemType::Consign,
                CONSIGNMENT_COST,
                MIN_CONSIGNMENT,
                MAX_CONSIGNMENT,
            ),
            MarketPanelType::Auction => (
                MarketItemType::Auction,
                AUCTION_COST,
                MIN_STARTING_BID,
                MAX_STARTING_BID,
            ),
            _ => {
                self.send_consign_ack(player_id, unique_id, false);
                return Err(MarketError::InvalidPanel(panel));
            }
        };
        if price < min_price || price > max_price {
            self.send_consign_ack(player_id, unique_id, false);
            return Err(MarketError::PriceOutOfRange {
                price,
                min: min_price,
                max: max_price,
            });
        }
        let held = player.gold;
        if held < i64::from(cost) {
            self.send_consign_ack(player_id, unique_id, false);
            return Err(MarketError::NotEnoughGold { needed: cost, held });
        }
        // NPC 距离（`:8265-8269`）：Crystal 在这里静默 return，不发回执。
        // 注意 `ConsignItem` 只要求「对话过 NPC」（`NPCPage != null`），**不要求**
        // 当前页是 `[@MARKET]`——玩家此时在 `[@CONSIGN]` 页上。
        self.check_npc_in_range(player_id)?;
        // 背包物品（`:8271-8286`）
        let Some((slot, item)) = self.find_inventory_item(player_id, unique_id) else {
            self.send_consign_ack(player_id, unique_id, false);
            return Err(MarketError::ItemNotFound(unique_id));
        };
        // `if (temp.Info.Bind.HasFlag(BindMode.DontSell))`（`:8288-8292`）
        let unsellable = self.item_info(item.item_index).is_none_or(|info| {
            info.bind.contains(BindMode::DontSell)
                || item
                    .rental_information
                    .as_ref()
                    .is_some_and(|rental| rental.binding_flags.contains(BindMode::DontSell))
        });
        if unsellable {
            self.send_consign_ack(player_id, unique_id, false);
            return Err(MarketError::ItemUnsellable);
        }
        // `AuctionInfo auction = new AuctionInfo(Info, temp, price, type);`（`:8299`）
        let auction_id = self.allocate_auction_id();
        let seller_character_id = self.players[&player_id].character_id;
        let seller_name = self.players[&player_id].name.clone();
        let auction = Auction {
            id: auction_id,
            item: item.clone(),
            consignment_date: DateTimeBinary::from_unix_seconds(self.wall_now().max(0) / 1000),
            price,
            // `if (itemType == MarketItemType.Auction) CurrentBid = Price;`（`AuctionInfo.cs:41-44`）
            current_bid: if item_type == MarketItemType::Auction {
                price
            } else {
                0
            },
            seller_character_id,
            seller_name,
            current_buyer_character_id: None,
            current_buyer_name: None,
            expired: false,
            sold: false,
            item_type,
        };
        // `Envir.Auctions.AddFirst(auction);`（`:8302`）
        self.auctions.insert(0, auction);
        // `p.Success = true; Enqueue(p);`（`:8304-8305`）
        self.send_consign_ack(player_id, unique_id, true);
        // `Info.Inventory[index] = null;`（`:8307`）
        if let Some(player) = self.players.get_mut(&player_id) {
            player.inventory[slot] = None;
        }
        // `Account.Gold -= cost; Enqueue(new S.LoseGold { Gold = cost });`（`:8309-8311`）
        if self.lose_gold(player_id, cost).is_err() {
            let held = self.players[&player_id].gold;
            return Err(MarketError::NotEnoughGold { needed: cost, held });
        }
        Ok(auction_id)
    }

    /// 批量上架（本作扩展；Crystal 只有单件 `ConsignItem`）。
    ///
    /// 单次最多 [`MAX_AUCTION_BATCH`] 件；先做**整批前置校验**（面板、价格、绑定性、
    /// 背包物品、总手续费），再逐个落地；任何一步失败都把背包/金币/挂单表/ID 计数器
    /// 恢复到调用前，做到「全成或全回滚」。
    ///
    /// # Errors
    ///
    /// 超过批量上限、面板/价格/绑定性/金币不满足、NPC 不在范围内等；失败时世界状态不变。
    pub fn consign_items_batch(
        &mut self,
        player_id: u32,
        items: &[(u64, u32)],
        panel: MarketPanelType,
    ) -> Result<Vec<u64>, MarketError> {
        if items.is_empty() || items.len() > MAX_AUCTION_BATCH {
            return Err(MarketError::BatchTooLarge {
                count: items.len(),
                max: MAX_AUCTION_BATCH,
            });
        }
        let Some(player) = self.players.get(&player_id) else {
            return Err(MarketError::NotInWorld(player_id));
        };
        if player.dead || player.npc.npc_object_id == 0 {
            return Err(if player.dead {
                MarketError::Dead
            } else {
                MarketError::NoDialog
            });
        }
        let (_, cost, min_price, max_price) = match panel {
            MarketPanelType::Consign => (
                MarketItemType::Consign,
                CONSIGNMENT_COST,
                MIN_CONSIGNMENT,
                MAX_CONSIGNMENT,
            ),
            MarketPanelType::Auction => (
                MarketItemType::Auction,
                AUCTION_COST,
                MIN_STARTING_BID,
                MAX_STARTING_BID,
            ),
            _ => return Err(MarketError::InvalidPanel(panel)),
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut total_fee = 0_u32;
        for &(unique_id, price) in items {
            if !seen.insert(unique_id) {
                return Err(MarketError::ItemNotFound(unique_id));
            }
            if price < min_price || price > max_price {
                return Err(MarketError::PriceOutOfRange {
                    price,
                    min: min_price,
                    max: max_price,
                });
            }
            let Some((_, item)) = self.find_inventory_item(player_id, unique_id) else {
                return Err(MarketError::ItemNotFound(unique_id));
            };
            let unsellable = self.item_info(item.item_index).is_none_or(|info| {
                info.bind.contains(BindMode::DontSell)
                    || item
                        .rental_information
                        .as_ref()
                        .is_some_and(|rental| rental.binding_flags.contains(BindMode::DontSell))
            });
            if unsellable {
                return Err(MarketError::ItemUnsellable);
            }
            total_fee = total_fee.saturating_add(cost);
        }
        let held = self.players[&player_id].gold;
        if held < i64::from(total_fee) {
            return Err(MarketError::NotEnoughGold {
                needed: total_fee,
                held,
            });
        }
        self.check_npc_in_range(player_id)?;

        // 整批快照：任何一件失败都回到这里。
        let auctions_before = self.auctions.clone();
        let next_id_before = self.next_auction_id;
        let mail_len_before = self.mail_writes.len();
        let Some(player_before) = self.players.get(&player_id) else {
            return Err(MarketError::NotInWorld(player_id));
        };
        let inventory_before = player_before.inventory.clone();
        let gold_before = player_before.gold;

        let mut ids = Vec::with_capacity(items.len());
        for &(unique_id, price) in items {
            match self.consign_item(player_id, unique_id, price, panel) {
                Ok(id) => ids.push(id),
                Err(error) => {
                    self.auctions = auctions_before;
                    self.next_auction_id = next_id_before;
                    self.mail_writes.truncate(mail_len_before);
                    if let Some(player) = self.players.get_mut(&player_id) {
                        player.inventory = inventory_before;
                        player.gold = gold_before;
                    }
                    return Err(error);
                }
            }
        }
        Ok(ids)
    }

    /// 批量下架/取回（本作扩展）：一次最多 [`MAX_AUCTION_BATCH`] 条。
    ///
    /// 先校验全部 `auction_id` 都属于该角色且未成交，再逐条取回；任何一条失败就把
    /// 挂单表/背包/金币/邮件队列恢复到调用前（全成或全回滚）。
    ///
    /// # Errors
    ///
    /// 超过批量上限、死亡/无 NPC 对话、挂单不属于本人/已成交、背包放不下等。
    pub fn market_get_back_batch(
        &mut self,
        player_id: u32,
        mode: MarketCollectionMode,
        auction_ids: &[u64],
    ) -> Result<usize, MarketError> {
        if auction_ids.is_empty() || auction_ids.len() > MAX_AUCTION_BATCH {
            return Err(MarketError::BatchTooLarge {
                count: auction_ids.len(),
                max: MAX_AUCTION_BATCH,
            });
        }
        let Some(player) = self.players.get(&player_id) else {
            return Err(MarketError::NotInWorld(player_id));
        };
        if player.dead {
            return Err(MarketError::Dead);
        }
        let character_id = player.character_id;
        for &auction_id in auction_ids {
            let Some(auction) = self.auction(auction_id) else {
                return Err(MarketError::ListingNotFound(auction_id));
            };
            if auction.seller_character_id != character_id || auction.sold {
                return Err(MarketError::ListingNotFound(auction_id));
            }
        }
        match self.check_market_npc(player_id) {
            Ok(()) => {}
            Err(error) => return Err(error),
        }

        let auctions_before = self.auctions.clone();
        let next_id_before = self.next_auction_id;
        let mail_len_before = self.mail_writes.len();
        let Some(player_before) = self.players.get(&player_id) else {
            return Err(MarketError::NotInWorld(player_id));
        };
        let inventory_before = player_before.inventory.clone();
        let gold_before = player_before.gold;

        let mut count = 0_usize;
        for &auction_id in auction_ids {
            let result = if mode == MarketCollectionMode::Any {
                self.market_get_back(player_id, mode, auction_id)
            } else {
                let Some(auction) = self.auction(auction_id).cloned() else {
                    return Err(MarketError::ListingNotFound(auction_id));
                };
                match self.take_auction(player_id, mode, &auction) {
                    Ok(true) => {
                        self.remove_auction(auction_id);
                        Ok(1)
                    }
                    Ok(false) => Ok(0),
                    Err(error) => Err(error),
                }
            };
            match result {
                Ok(0) => {
                    self.auctions = auctions_before;
                    self.next_auction_id = next_id_before;
                    self.mail_writes.truncate(mail_len_before);
                    if let Some(player) = self.players.get_mut(&player_id) {
                        player.inventory = inventory_before;
                        player.gold = gold_before;
                    }
                    return Err(MarketError::ListingNotFound(auction_id));
                }
                Ok(done) => count += done,
                Err(error) => {
                    self.auctions = auctions_before;
                    self.next_auction_id = next_id_before;
                    self.mail_writes.truncate(mail_len_before);
                    if let Some(player) = self.players.get_mut(&player_id) {
                        player.inventory = inventory_before;
                        player.gold = gold_before;
                    }
                    return Err(error);
                }
            }
        }
        let _ = self.market_refresh(player_id);
        Ok(count)
    }

    /// 搜索市场：`C.MarketSearch`，Crystal `PlayerObject.MarketSearch`（`:8424-8448`）
    /// 与 `GetMarket`（`:8376-8422`）。
    ///
    /// 节流 [`SEARCH_DELAY_MS`]；非商城面板必须是通过 `[@MARKET]` 打开的、且 NPC 还在范围内，
    /// 否则静默返回（`:8426-8447`）。结果按 [`MARKET_PAGE_SIZE`] 分页，首条 `S.NPCMarket`。
    // 参数与 `C.MarketSearch` 的线字段一一对应（Match/Type/UserMode/MinShape/MaxShape/MarketType），
    // 打包成结构体只会把「协议字段」和「世界对象」混在一起。
    #[allow(clippy::too_many_arguments)]
    pub fn market_search(
        &mut self,
        player_id: u32,
        match_text: &str,
        match_type: ItemType,
        user_mode: bool,
        min_shape: i16,
        max_shape: i16,
        panel: MarketPanelType,
    ) -> Result<(), MarketError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(MarketError::NotInWorld(player_id));
        };
        if player.dead {
            return Err(MarketError::Dead);
        }
        let now = self.now();
        let mut state = self.market_searches.remove(&player_id).unwrap_or_default();
        // `if (Dead || Envir.Time < SearchTime) return;`（`:8426-8429`）
        if now < state.search_time {
            self.market_searches.insert(player_id, state);
            return Err(MarketError::Throttled);
        }
        state.search_time = now + SEARCH_DELAY_MS;
        state.user_match = user_mode;
        state.min_shapes = min_shape;
        state.max_shapes = max_shape;
        state.panel = panel;
        self.market_searches.insert(player_id, state);
        // `if (MarketPanelType == GameShop) GetMarket(...) else { …NPC 检查… }`（`:8430-8447`）
        if panel != MarketPanelType::GameShop {
            self.check_market_npc(player_id)?;
        }
        self.get_market(player_id, match_text, match_type);
        Ok(())
    }

    /// 刷新：`C.MarketRefresh`（`MirConnection.cs:1692-1697`）→ 用上次的 `MatchName/MatchType`
    /// 重新搜索（`PlayerObject.cs:1696`）。没有搜索过时按空关键字处理。
    pub fn market_refresh(&mut self, player_id: u32) -> Result<(), MarketError> {
        let Some(state) = self.market_searches.get(&player_id).cloned() else {
            return Err(MarketError::Throttled);
        };
        let now = self.now();
        if now < state.search_time {
            return Err(MarketError::Throttled);
        }
        if let Some(state) = self.market_searches.get_mut(&player_id) {
            state.search_time = now + SEARCH_DELAY_MS;
        }
        if state.panel != MarketPanelType::GameShop {
            self.check_market_npc(player_id)?;
        }
        let (name, item_type) = (state.match_name.clone(), state.match_type);
        self.get_market(player_id, &name, item_type);
        Ok(())
    }

    /// 翻页：`C.MarketPage`，Crystal `PlayerObject.MarketPage`（`:8327-8374`）。
    ///
    /// 只接受**比已经发过的页码更大**的页（`page <= PageSent` 直接返回），每次翻页也要
    /// 重新过 [`SEARCH_DELAY_MS`] 节流；最多 10 条 `S.NPCMarketPage`。
    pub fn market_page(&mut self, player_id: u32, page: i32) -> Result<(), MarketError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(MarketError::NotInWorld(player_id));
        };
        if player.dead {
            return Err(MarketError::Dead);
        }
        let now = self.now();
        let Some(mut state) = self.market_searches.get(&player_id).cloned() else {
            return Err(MarketError::NoDialog);
        };
        if now < state.search_time {
            return Err(MarketError::Throttled);
        }
        if state.panel != MarketPanelType::GameShop {
            // `if (NPCPage == null || key != MarketKey || page <= PageSent) return;`（`:8335`）
            self.check_market_npc(player_id)?;
            if page <= state.page_sent {
                return Err(MarketError::NoDialog);
            }
            state.search_time = now + SEARCH_DELAY_MS;
        }
        // `for (int i = 0; i < 10; i++) { if (i + page * 10 >= Search.Count) break; … }`（`:8356-8360`）
        let start = usize::try_from(page.max(0)).unwrap_or(0) * MARKET_PAGE_SIZE;
        let listings: Vec<ClientAuction> = if state.panel == MarketPanelType::GameShop {
            // 商城页签：`Search` 里存的是商品 `GIndex`（见 [`World::get_market`]），
            // 这里换成商城条目的线格式（`ClientAuction.item_type = GameShop`）。
            state
                .listings
                .iter()
                .skip(start)
                .take(MARKET_PAGE_SIZE)
                .filter_map(|g_index| i32::try_from(*g_index).ok())
                .filter_map(|g_index| self.game_shop_client_auction(g_index))
                .collect()
        } else {
            state
                .listings
                .iter()
                .skip(start)
                .take(MARKET_PAGE_SIZE)
                .filter_map(|auction_id| self.auction(*auction_id))
                .map(|auction| {
                    auction.to_client_auction(state.user_match, self.player_language(player_id))
                })
                .collect()
        };
        state.page_sent = page;
        self.market_searches.insert(player_id, state);
        // `Enqueue(new S.NPCMarketPage { Listings = clientListings });`（`:8373`）
        self.send_to_player(
            player_id,
            &ServerPacket::NPCMarketPage(NPCMarketPage { listings }).encode_frame(),
        );
        Ok(())
    }

    /// 购买/出价：`C.MarketBuy`，Crystal `PlayerObject.MarketBuy`（`:8450-8589`）。
    ///
    /// 失败原因编号见 [`MarketFailReason`]，顺序照抄源码：
    /// 死亡(0) → 没开面板(1) → 找不到/NPC 太远(7) → 已卖(2) → 过期(3) → 背包满(5) →
    /// 自己挂的(6) → 金币不够(4) → 拍卖出价太低(9)。
    ///
    /// 寄售：直接买断（`Sold = true`），`S.LoseGold` → `S.GainedItem` → `S.MarketSuccess`。
    /// 拍卖：把上一个最高出价者的金币**邮寄退回**，记录新的最高价，`S.LoseGold` → `S.MarketSuccess`。
    pub fn market_buy(
        &mut self,
        player_id: u32,
        auction_id: u64,
        bid_price: u32,
    ) -> Result<(), MarketError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(MarketError::NotInWorld(player_id));
        };
        // `if (Dead) { Enqueue(new S.MarketFail { Reason = 0 }); return; }`（`:8452-8456`）
        if player.dead {
            self.send_market_fail(player_id, MarketFailReason::Dead);
            return Err(MarketError::Dead);
        }
        // 非商城面板：`NPCPage == null || key != MarketKey` → Reason 1（`:8489-8493`），
        // NPC 找不到/太远则静默返回（`:8495-8500`）。
        let panel = self
            .market_searches
            .get(&player_id)
            .map_or(MarketPanelType::Market, |state| state.panel);
        if panel != MarketPanelType::GameShop {
            match self.check_market_npc(player_id) {
                Ok(()) => {}
                Err(MarketError::NoDialog) => {
                    self.send_market_fail(player_id, MarketFailReason::NoDialog);
                    return Err(MarketError::NoDialog);
                }
                Err(error) => return Err(error),
            }
        }
        // 只在**上次搜索结果**里找（`:8502`）
        let Some(state) = self.market_searches.get(&player_id).cloned() else {
            self.send_market_fail(player_id, MarketFailReason::TooFar);
            return Err(MarketError::ListingNotFound(auction_id));
        };
        if !state.listings.contains(&auction_id) {
            self.send_market_fail(player_id, MarketFailReason::TooFar);
            return Err(MarketError::ListingNotFound(auction_id));
        }
        let Some(auction) = self.auction(auction_id).cloned() else {
            self.send_market_fail(player_id, MarketFailReason::Expired);
            return Err(MarketError::ListingNotFound(auction_id));
        };
        // `if (auction.Sold)` → 2；`if (auction.Expired)` → 3；`!Envir.Auctions.Contains` → 3（`:8507-8523`）
        if auction.sold {
            self.send_market_fail(player_id, MarketFailReason::Sold);
            return Err(MarketError::ListingSold(auction_id));
        }
        if auction.expired {
            self.send_market_fail(player_id, MarketFailReason::Expired);
            return Err(MarketError::ListingExpired(auction_id));
        }
        // `if (!CanGainItem(auction.Item))` → 5（`:8525-8529`）
        let Some(item_info) = self.item_info(auction.item.item_index).cloned() else {
            self.send_market_fail(player_id, MarketFailReason::NoSpace);
            return Err(MarketError::CannotGain);
        };
        if !self.can_gain_item(player_id, &item_info, auction.item.count) {
            self.send_market_fail(player_id, MarketFailReason::NoSpace);
            return Err(MarketError::CannotGain);
        }
        // `if (Account.Auctions.Contains(auction))` → 6（`:8531-8535`）
        if auction.seller_character_id == player.character_id {
            self.send_market_fail(player_id, MarketFailReason::OwnItem);
            return Err(MarketError::OwnListing);
        }
        // `if (auction.Price > Account.Gold || bidPrice > Account.Gold)` → 4（`:8537-8541`）
        let held = self.players[&player_id].gold;
        if held < i64::from(auction.price) || held < i64::from(bid_price) {
            self.send_market_fail(player_id, MarketFailReason::LowGold);
            return Err(MarketError::NotEnoughGold {
                needed: auction.price.max(bid_price),
                held,
            });
        }
        let buyer_name = self.players[&player_id].name.clone();
        let item_name = friendly_name(&self.item_info(auction.item.item_index).map_or_else(
            || auction.item.item_index.to_string(),
            |info| info.name.clone(),
        ));
        match auction.item_type {
            // 寄售买断（`:8543-8555`）
            MarketItemType::Consign => {
                if let Some(listing) = self.auctions.iter_mut().find(|a| a.id == auction_id) {
                    listing.sold = true;
                }
                if self.lose_gold(player_id, auction.price).is_err() {
                    let held = self.players[&player_id].gold;
                    return Err(MarketError::NotEnoughGold {
                        needed: auction.price,
                        held,
                    });
                }
                self.gain_item(player_id, auction.item.clone());
                // `Envir.MessageAccount(auction.SellerInfo.AccountInfo, SoldItemForGold…)`（`:8552`）
                let seller_message = self.format_for_character(
                    auction.seller_character_id,
                    TextKey::MarketMailSold,
                    &[("item", &item_name), ("gold", &auction.price.to_string())],
                );
                self.message_character(auction.seller_character_id, &seller_message);
                let message = self.format(
                    player_id,
                    TextKey::MarketBought,
                    &[("gold", &auction.price.to_string()), ("item", &item_name)],
                );
                self.send_market_success(player_id, message);
                let _ = self.market_refresh(player_id);
                Ok(())
            }
            // 拍卖出价（`:8556-8581`）
            MarketItemType::Auction => {
                if auction.current_bid > bid_price {
                    self.send_market_fail(player_id, MarketFailReason::BidTooLow);
                    return Err(MarketError::BidTooLow {
                        current: auction.current_bid,
                        bid: bid_price,
                    });
                }
                // `if (auction.CurrentBuyerInfo != null) MailCharacter(CurrentBuyerInfo, gold: CurrentBid…)`
                // （`:8564-8569`）
                if let Some(previous_buyer) = auction.current_buyer_character_id {
                    let refund = self.format_for_character(
                        previous_buyer,
                        TextKey::MarketMailOutbidRefund,
                        &[
                            ("item", &item_name),
                            ("gold", &auction.current_bid.to_string()),
                        ],
                    );
                    self.mail_system_to_character(
                        previous_buyer,
                        None,
                        auction.current_bid,
                        &refund,
                    );
                }
                if let Some(listing) = self.auctions.iter_mut().find(|a| a.id == auction_id) {
                    listing.current_bid = bid_price;
                    listing.current_buyer_character_id =
                        Some(self.players[&player_id].character_id);
                    listing.current_buyer_name = Some(buyer_name);
                }
                if self.lose_gold(player_id, bid_price).is_err() {
                    let held = self.players[&player_id].gold;
                    return Err(MarketError::NotEnoughGold {
                        needed: bid_price,
                        held,
                    });
                }
                let seller_message = self.format_for_character(
                    auction.seller_character_id,
                    TextKey::MarketMailNewBid,
                    &[("item", &item_name), ("bid", &bid_price.to_string())],
                );
                self.message_character(auction.seller_character_id, &seller_message);
                let message = self.format(
                    player_id,
                    TextKey::MarketBidPlaced,
                    &[("item", &item_name), ("bid", &bid_price.to_string())],
                );
                self.send_market_success(player_id, message);
                let _ = self.market_refresh(player_id);
                Ok(())
            }
            // 商城条目不会进 `World::auctions`（它们来自 `GameData::game_shop`），
            // 这里只是让 match 保持穷尽：真出现就当「列表里没有」处理。
            MarketItemType::GameShop => {
                self.send_market_fail(player_id, MarketFailReason::TooFar);
                Err(MarketError::ListingNotFound(auction_id))
            }
        }
    }

    /// 提前成交拍卖：`C.MarketSellNow`，Crystal `PlayerObject.MarketSellNow`（`:8591-8666`）。
    ///
    /// 只有**卖家自己**、**拍卖类型**、**已经有人出价且高于起拍价**的条目才能提前成交：
    /// 物品邮寄给最高出价者，卖家拿到 `成交价 - 佣金`。
    pub fn market_sell_now(&mut self, player_id: u32, auction_id: u64) -> Result<(), MarketError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(MarketError::NotInWorld(player_id));
        };
        if player.dead {
            self.send_market_fail(player_id, MarketFailReason::Dead);
            return Err(MarketError::Dead);
        }
        match self.check_market_npc(player_id) {
            Ok(()) => {}
            Err(MarketError::NoDialog) => {
                self.send_market_fail(player_id, MarketFailReason::NoDialog);
                return Err(MarketError::NoDialog);
            }
            Err(error) => return Err(error),
        }
        let character_id = player.character_id;
        // `foreach (AuctionInfo auction in Account.Auctions)`（`:8611`）
        let Some(auction) = self
            .auctions
            .iter()
            .find(|a| a.id == auction_id && a.seller_character_id == character_id)
            .cloned()
        else {
            self.send_market_fail(player_id, MarketFailReason::TooFar);
            return Err(MarketError::ListingNotFound(auction_id));
        };
        // `if (auction.ItemType != MarketItemType.Auction) return;`（`:8615-8618`）
        if auction.item_type != MarketItemType::Auction {
            return Err(MarketError::ListingNotFound(auction_id));
        }
        // `if (auction.CurrentBid <= auction.Price || auction.CurrentBuyerInfo == null)` → 9（`:8620-8624`）
        if auction.current_bid <= auction.price || auction.current_buyer_character_id.is_none() {
            self.send_market_fail(player_id, MarketFailReason::BidTooLow);
            return Err(MarketError::NoBid);
        }
        // `if (auction.Expired || auction.Sold || Now >= ConsignmentDate.AddDays(...))` → 10（`:8632-8636`）
        if auction.expired || auction.sold || Self::auction_expired_at(&auction, self.wall_now()) {
            self.send_market_fail(player_id, MarketFailReason::AuctionEnded);
            return Err(MarketError::AuctionEnded);
        }
        let cost = auction.current_bid;
        let gold = Auction::after_commission(cost);
        // `if (!CanGainGold(auction.CurrentBid))` → 8（`:8642-8646`）
        if !self.can_gain_gold(player_id, cost) {
            self.send_market_fail(player_id, MarketFailReason::CannotHoldGold);
            return Err(MarketError::CannotGain);
        }
        let item_name = friendly_name(&self.item_info(auction.item.item_index).map_or_else(
            || auction.item.item_index.to_string(),
            |info| info.name.clone(),
        ));
        if let Some(listing) = self.auctions.iter_mut().find(|a| a.id == auction_id) {
            listing.sold = true;
        }
        // 物品邮寄给最高出价者（`:8652`）
        if let Some(buyer) = auction.current_buyer_character_id {
            let message = self.format_for_character(
                buyer,
                TextKey::MarketMailAuctionWon,
                &[("item", &item_name), ("cost", &cost.to_string())],
            );
            self.mail_system_to_character(buyer, Some(auction.item.clone()), 0, &message);
        }
        self.remove_auction(auction_id);
        // `GainGold(gold);`（`:8657`）
        self.gain_gold(player_id, gold);
        let message = self.format(
            player_id,
            TextKey::MarketSoldNow,
            &[
                ("cost", &cost.to_string()),
                ("item", &item_name),
                ("gold", &gold.to_string()),
            ],
        );
        self.send_market_success(player_id, message);
        let _ = self.market_refresh(player_id);
        Ok(())
    }

    /// 下架 / 收款：`C.MarketGetBack`，Crystal `PlayerObject.MarketGetBack`（`:8668-8793`）。
    ///
    /// `mode == Any` 时按 `auction_id` 处理一条；`Sold` / `Expired` 时批量处理该玩家
    /// **所有**符合状态的条目（Crystal 的 `foreach (node in Account.Auctions)`）。
    ///
    /// 单条处理 `TakeAuction` 的分支照抄（`:8680-8728`）：
    /// * 未售出且未过期（或已过期）→ 物品退回背包，若有人出价则把出价邮寄退回；
    /// * 已售出 → 收钱（寄售是 `Price`，拍卖是 `CurrentBid`），扣 [`COMMISSION`]。
    ///
    /// 返回处理的条目数。
    pub fn market_get_back(
        &mut self,
        player_id: u32,
        mode: MarketCollectionMode,
        auction_id: u64,
    ) -> Result<usize, MarketError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(MarketError::NotInWorld(player_id));
        };
        if player.dead {
            self.send_market_fail(player_id, MarketFailReason::Dead);
            return Err(MarketError::Dead);
        }
        match self.check_market_npc(player_id) {
            Ok(()) => {}
            Err(MarketError::NoDialog) => {
                self.send_market_fail(player_id, MarketFailReason::NoDialog);
                return Err(MarketError::NoDialog);
            }
            Err(error) => return Err(error),
        }
        let character_id = player.character_id;
        if mode == MarketCollectionMode::Any {
            // `if (mode == 0) { GetAuction(auctionID); if (TakeAuction) { remove; MarketSearch; return; } }`
            // （`:8749-8762`）
            let Some(auction) = self
                .auctions
                .iter()
                .find(|a| a.id == auction_id && a.seller_character_id == character_id)
                .cloned()
            else {
                self.send_market_fail(player_id, MarketFailReason::TooFar);
                return Err(MarketError::ListingNotFound(auction_id));
            };
            if !self.take_auction(player_id, mode, &auction)? {
                return Ok(0);
            }
            self.remove_auction(auction_id);
            let _ = self.market_refresh(player_id);
            return Ok(1);
        }
        // 批量：`while (node != null) { TakeAuction; count++; }`（`:8766-8788`）
        let ids = self.auctions_of_character(character_id);
        let mut count = 0usize;
        for id in ids {
            let Some(auction) = self.auction(id).cloned() else {
                continue;
            };
            if self.take_auction(player_id, mode, &auction)? {
                self.remove_auction(id);
                count += 1;
            }
        }
        if count > 0 {
            let _ = self.market_refresh(player_id);
            return Ok(count);
        }
        self.send_market_fail(player_id, MarketFailReason::TooFar);
        Err(MarketError::ListingNotFound(auction_id))
    }

    /// 每个 tick 的兼容入口（`world.rs` 的 [`World::tick`] 每 100ms 调一次）。
    ///
    /// **这里不做全表扫描**：到期标记与自动成交统一由节流的 [`World::process_auctions`]
    /// 完成（`server.rs` 的 tick 任务按 [`AUCTION_SCAN_INTERVAL_MS`] 每分钟调一次；
    /// Crystal 是每 10 分钟一次，`Server/MirEnvir/Envir.cs:2354-2358`）。
    ///
    /// 保留这个空壳只是为了不动 `world.rs`（兵部独占）里的调用点；等那边的调用删掉，
    /// 本函数即可删除。语义上唯一的可见差异：挂单到期后 `Expired` 标记最多延迟
    /// [`AUCTION_SCAN_INTERVAL_MS`]（卖家 `MarketGetBack` 的取回分支另有
    /// [`World::auction_expired_at`] 兜底，功能不缺）。
    pub fn tick_market(&mut self) {}

    /// 到期的拍卖是否该**自动成交**（[`World::process_auctions`] 的成交分支）：
    /// 拍卖类型 + 最高出价**高于起拍价** + 记了出价者（Crystal `Envir.cs:2398`）。
    ///
    /// 寄售（`Consign`）买断走 `market_buy` 当场成交（`Sold`），不经过这里；出价只在
    /// 拍卖类型上产生（`PlayerObject.cs:8556-8581`）。
    #[must_use]
    pub fn auction_will_sell(auction: &Auction) -> bool {
        auction.item_type == MarketItemType::Auction
            && auction.current_bid > auction.price
            && auction.current_buyer_character_id.is_some()
    }

    /// 到期结算：Crystal `Envir.ProcessAuction`（`Server/MirEnvir/Envir.cs:2388-2418`）。
    ///
    /// 对每条到期（`ConsignmentDate + ConsignmentLength`，`!Sold`）的挂单：
    ///
    /// * **有有效出价**（[`World::auction_will_sell`]）→ 自动成交：
    ///   物品按系统邮件寄给最高出价者、`成交价 − 佣金` 按系统邮件寄给卖家，挂单从
    ///   [`World::auctions`] 移除并作为返回值交给调用方落盘；在线双方各收一条聊天提示
    ///   （Crystal `MessageAccount(..., ChatType.Hint)`，`:2405-2408`）。
    /// * 否则置 `Expired`（由启动清理 [`World::purge_expired_auctions`] 或卖家
    ///   `MarketGetBack` 收尾）。
    ///
    /// **为什么用金币邮件而不是直接加卖家角色行**：卖家可能在离线，而
    /// [`crate::store::MarketUpdate`] 的不变量是「对手方只通过 `mail_writes` 收款/收货，
    /// 不跨会话写对方角色行」——直接写离线角色行既需要额外的行加载，又会和卖家会话的
    /// 整行快照互相覆盖。Crystal 的做法是卖家登录后 `MarketGetBack` 领钱；本作自动成交
    /// 后挂单即删除，所以把金币直接寄到卖家邮箱（离线也能到账），原子性由
    /// [`flush_auction_sales`] 的同一个事务保证。
    ///
    /// **幂等**：成交的挂单会被移出列表，重复调用不会重复成交。本函数只改内存，
    /// 落盘由调用方用 [`flush_auction_sales`] 完成——事务失败时**内存里的成交结果保留**
    /// （挂单已删、邮件留在 [`World::mail_writes`] 队列），调用方在下一轮扫描重试落盘即可
    /// （会话侧的市场操作也是这个语义：内存先行、失败重试）。这样不会出现
    /// 「回滚挂单 + 但邮件已被别的会话提交」的双发；崩溃窗口内（内存改了、事务没提交）
    /// 重启后以库为准，由启动路径重新结算，同样只成交一次。
    ///
    /// **取回窗口**：到期后到下一轮扫描之间（≤ [`AUCTION_SCAN_INTERVAL_MS`]，Crystal 是
    /// 10 分钟）卖家仍可用 `MarketGetBack` 把物品取回（买家当场收到出价退款）；取回会
    /// 删掉挂单，本函数之后不再成交——与 Crystal「扫描前取回即作废」一致。
    ///
    /// 返回本次成交的挂单（新 → 旧），供调用方观测/测试断言与日志。
    pub fn process_auctions(&mut self, now_ms: i64) -> Vec<Auction> {
        let due: Vec<Auction> = self
            .auctions
            .iter()
            .filter(|auction| {
                !auction.sold && !auction.expired && Self::auction_expired_at(auction, now_ms)
            })
            .cloned()
            .collect();
        if due.is_empty() {
            return Vec::new();
        }
        let mut sold = Vec::new();
        for auction in due {
            if Self::auction_will_sell(&auction) {
                self.sell_auction_on_expiry(&auction);
                sold.push(auction);
            } else if let Some(listing) = self
                .auctions
                .iter_mut()
                .find(|listing| listing.id == auction.id)
            {
                listing.expired = true;
            }
        }
        if !sold.is_empty() {
            let ids: std::collections::BTreeSet<u64> = sold.iter().map(|a| a.id).collect();
            self.auctions.retain(|listing| !ids.contains(&listing.id));
        }
        sold
    }

    /// 自动成交收尾：物品寄给最高出价者、`成交价 − 佣金` 寄给卖家，在线双方各一条提示。
    ///
    /// 出价的金币在出价时就已从买家扣除（`market_buy` 的拍卖分支），所以这里只发货、
    /// 不再扣钱；买家拿不到物品的风险由「邮件与挂单删除同事务」兜住。
    fn sell_auction_on_expiry(&mut self, auction: &Auction) {
        let item_name = friendly_name(&self.item_info(auction.item.item_index).map_or_else(
            || auction.item.item_index.to_string(),
            |info| info.name.clone(),
        ));
        let cost = auction.current_bid;
        let payout = Auction::after_commission(cost);
        let Some(buyer) = auction.current_buyer_character_id else {
            return;
        };
        // 物品 → 最高出价者（Crystal `MailCharacter(CurrentBuyerInfo, item: Item, customMessage)`，`:2403`）
        let item_message = self.format_for_character(
            buyer,
            TextKey::MarketMailAuctionWon,
            &[("item", &item_name), ("cost", &cost.to_string())],
        );
        self.mail_system_to_character(buyer, Some(auction.item.clone()), 0, &item_message);
        // 金币 → 卖家（Crystal 让卖家 `MarketGetBack` 领；本作随成交直接寄，离线也能到账）。
        // 正文用 `market_mail_sold_commission`：把「成交价 {cost}」和「扣佣金后到手 {gold}」
        // 都写清楚（Crystal 的 `YouSoldForGold` 只有金额，佣金是 `MarketGetBack` 时才算）。
        if payout > 0 {
            let gold_message = self.format_for_character(
                auction.seller_character_id,
                TextKey::MarketMailSoldCommission,
                &[
                    ("item", &item_name),
                    ("cost", &cost.to_string()),
                    ("gold", &payout.to_string()),
                ],
            );
            self.mail_system_to_character(auction.seller_character_id, None, payout, &gold_message);
        }
        // 在线提示（Crystal `MessageAccount(..., ChatType.Hint)`，`:2405-2408`）。
        let buyer_chat = self.format_for_character(
            buyer,
            TextKey::MarketBought,
            &[("gold", &cost.to_string()), ("item", &item_name)],
        );
        self.message_character(buyer, &buyer_chat);
        let seller_chat = self.format_for_character(
            auction.seller_character_id,
            TextKey::MarketSoldNow,
            &[
                ("cost", &cost.to_string()),
                ("item", &item_name),
                ("gold", &payout.to_string()),
            ],
        );
        self.message_character(auction.seller_character_id, &seller_chat);
        tracing::info!(
            auction = auction.id,
            buyer,
            seller = auction.seller_character_id,
            cost,
            payout,
            "拍卖到期自动成交"
        );
    }

    /// 条目是否已到 [`CONSIGNMENT_LENGTH_DAYS`] 天。
    fn auction_expired_at(auction: &Auction, now_ms: i64) -> bool {
        now_ms.max(0) >= auction.consignment_date.to_unix_seconds() * 1_000 + AUCTION_EXPIRE_MS
    }

    /// 分配拍卖 ID（Crystal `++Envir.NextAuctionID`，`AuctionInfo.cs:33`）。
    pub(crate) fn allocate_auction_id(&mut self) -> u64 {
        if self.next_auction_id == 0 {
            self.next_auction_id = 1;
        }
        let id = self.next_auction_id;
        self.next_auction_id += 1;
        id
    }

    /// `GetMarket`：搜索 + 首页 `S.NPCMarket`（Crystal `PlayerObject.cs:8376-8422`）。
    ///
    /// 商城页签分支对应 Crystal 里被注释掉的那段（`:8385-8390`）：`Search` 换成
    /// `Envir.GameShopList` 的过滤结果，条目 ID 是商品 `GIndex`（`u64` 承载）。
    fn get_market(&mut self, player_id: u32, match_text: &str, item_type: ItemType) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let character_id = player.character_id;
        let class = player.class;
        let mut state = self.market_searches.remove(&player_id).unwrap_or_default();
        // `MatchName = name.Replace(" ", ""); MatchType = type; PageSent = 0;`（`:8379-8381`）
        state.match_name = match_text.replace(' ', "");
        state.match_type = item_type;
        state.page_sent = 0;
        // 搜索结果：`UserMatch ? Account.Auctions : Envir.Auctions`（`:8393-8399`）
        let mut listings = Vec::new();
        if state.panel == MarketPanelType::GameShop {
            // 商城页签：过滤规则与 `Match` 同构（物品类型 / 名字关键字），外加
            // 「职业可用」这一条（本作扩展，见 [`Self::game_shop_matches`]）。
            for product in self.game_shop() {
                if !Self::game_shop_matches(&state, product, class) {
                    continue;
                }
                listings.push(u64::from(u32::try_from(product.g_index).unwrap_or(0)));
            }
        } else {
            for auction in &self.auctions {
                if state.user_match && auction.seller_character_id != character_id {
                    continue;
                }
                if !Self::market_matches(&state, auction) {
                    continue;
                }
                let Some(info) = self.item_info(auction.item.item_index) else {
                    continue;
                };
                if !Self::market_item_matches(&state, info) {
                    continue;
                }
                listings.push(auction.id);
            }
        }
        let shown: Vec<ClientAuction> = if state.panel == MarketPanelType::GameShop {
            listings
                .iter()
                .take(MARKET_PAGE_SIZE)
                .filter_map(|g_index| i32::try_from(*g_index).ok())
                .filter_map(|g_index| self.game_shop_client_auction(g_index))
                .collect()
        } else {
            listings
                .iter()
                .take(MARKET_PAGE_SIZE)
                .filter_map(|id| self.auction(*id))
                .map(|auction| {
                    auction.to_client_auction(state.user_match, self.player_language(player_id))
                })
                .collect()
        };
        // `Pages = (Search.Count - 1) / 10 + 1`（`:8419`）
        let pages = (i32::try_from(listings.len()).unwrap_or(i32::MAX) - 1) / 10 + 1;
        state.listings = listings;
        let user_mode = state.user_match;
        self.market_searches.insert(player_id, state);
        // `Enqueue(new S.NPCMarket { Listings = clientListings, Pages = …, UserMode = … });`
        self.send_to_player(
            player_id,
            &ServerPacket::NPCMarket(NPCMarket {
                listings: shown,
                pages,
                user_mode,
            })
            .encode_frame(),
        );
    }

    /// 商城商品是不是这次搜索想要的（Crystal `Match` 的商城版，`:8322-8324` + `:8387-8389`）。
    ///
    /// * 物品类型：`MatchType != Nothing` 时必须相等（Crystal 注释里那条过滤）；
    /// * 名字：去空格、大小写不敏感地包含关键字（与 [`Self::market_item_matches`] 同口径）；
    /// * 职业：`GameShopItem.Class` 必须匹配本职业（**本作扩展**，Crystal 只在客户端过滤，
    ///   见 [`game_shop_class_allows`]）；
    /// * 外观区间（`MinShapes`/`MaxShapes`）不参与：商城页签没有这个筛选 UI，默认值 0/0
    ///   会把所有商品都滤掉。
    fn game_shop_matches(
        state: &MarketSearchState,
        product: &GameShopItem,
        class: mir_core::enums::MirClass,
    ) -> bool {
        let info = &product.info;
        if state.match_type != ItemType::Nothing && info.item_type != state.match_type {
            return false;
        }
        if !game_shop_class_allows(product, class) {
            return false;
        }
        if !state.match_name.is_empty() {
            let name = info.name.replace(' ', "").to_lowercase();
            if !name.contains(&state.match_name.to_lowercase()) {
                return false;
            }
        }
        true
    }

    /// 一件商城商品的线上条目（`S.NPCMarket` 的 `ClientAuction`）。
    ///
    /// 字段映射：
    /// * `auction_id` = 商品 `GIndex`（购买时原样回填 `C.GameshopBuy.g_index`）；
    /// * `item` = 模板 ID + 每份数量（`unique_id`/耐久为 0，仅供客户端显示图标与名字）；
    /// * `price` = **元宝**单价 `CreditPrice`（金币价不在列表里展示，Crystal 的
    ///   `TrustMerchantDialog` 商城页也只显示 Credit，`TrustMerchantDialog.cs:979`）；
    /// * `seller` 空串、`item_type = GameShop`、`consignment_date` = 上架时间。
    fn game_shop_client_auction(&self, g_index: i32) -> Option<ClientAuction> {
        let product = self.game_shop_item(g_index)?;
        Some(ClientAuction {
            auction_id: u64::from(u32::try_from(product.g_index).ok()?),
            item: UserItem {
                item_index: product.item_index,
                count: product.count.max(1),
                ..UserItem::default()
            },
            seller: String::new(),
            price: product.credit_price,
            consignment_date: product.date,
            item_type: MarketItemType::GameShop,
        })
    }

    /// `Match(AuctionInfo)`：Crystal `PlayerObject.cs:8318-8325`。
    fn market_matches(state: &MarketSearchState, auction: &Auction) -> bool {
        // `(UserMatch || !info.Expired && !info.Sold)`
        if !state.user_match && (auction.expired || auction.sold) {
            return false;
        }
        // 按面板过滤：拍卖页签只看拍卖，其余只看寄售
        if state.user_match {
            let wanted = if state.panel == MarketPanelType::Auction {
                MarketItemType::Auction
            } else {
                MarketItemType::Consign
            };
            if auction.item_type != wanted {
                return false;
            }
        }
        true
    }

    /// 匹配的「物品属性」部分：类型、外观区间、名字关键字。
    ///
    /// Crystal `Match` 把这几项写在同一个 `&&` 链里（`PlayerObject.cs:8322-8324`）。
    fn market_item_matches(state: &MarketSearchState, info: &ItemInfo) -> bool {
        if state.match_type != ItemType::Nothing && info.item_type != state.match_type {
            return false;
        }
        if info.shape < state.min_shapes || info.shape > state.max_shapes {
            return false;
        }
        if !state.match_name.is_empty() {
            let name = info.name.replace(' ', "").to_lowercase();
            if !name.contains(&state.match_name.to_lowercase()) {
                return false;
            }
        }
        true
    }

    /// 校验「玩家此刻能访问市场」：`[@MARKET]` 页面 + NPC 在同图范围内
    /// （Crystal `PlayerObject.cs:8335`、`:8436-8446`、`:8489-8500`、`:8599-8609`、`:8737-8747`）。
    fn check_market_npc(&self, player_id: u32) -> Result<(), MarketError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(MarketError::NotInWorld(player_id));
        };
        if !player.npc.page_key.eq_ignore_ascii_case(MARKET_KEY) {
            return Err(MarketError::NoDialog);
        }
        self.check_npc_in_range(player_id)
    }

    /// 只校验「对话的 NPC 还在同图范围内」（不查页面键）。
    ///
    /// `ConsignItem` 只检查 `NPCPage != null`（`PlayerObject.cs:8222`），不要求页面键是
    /// `[@MARKET]`——玩家此时停在 `[@CONSIGN]` 页。
    fn check_npc_in_range(&self, player_id: u32) -> Result<(), MarketError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(MarketError::NotInWorld(player_id));
        };
        let npc_object_id = player.npc.npc_object_id;
        if npc_object_id == 0 {
            return Err(MarketError::NoDialog);
        }
        let player_map = player.map.clone();
        let player_position = player.position;
        let Some(npc) = self
            .npcs
            .iter()
            .find(|npc| npc.object_id == npc_object_id && npc.map == player_map)
        else {
            return Err(MarketError::NoSuchNpc {
                object_id: npc_object_id,
            });
        };
        if !npc.position.in_range(player_position, NPC_TALK_RANGE) {
            return Err(MarketError::OutOfRange {
                object_id: npc_object_id,
                range: NPC_TALK_RANGE,
            });
        }
        Ok(())
    }

    /// `TakeAuction`：Crystal `PlayerObject.MarketGetBack` 的内部函数（`:8680-8728`）。
    ///
    /// 返回 `Ok(true)` 表示条目应被移除。
    fn take_auction(
        &mut self,
        player_id: u32,
        mode: MarketCollectionMode,
        auction: &Auction,
    ) -> Result<bool, MarketError> {
        // `if (auction.Sold && auction.Expired) { log; return false; }`（`:8682-8686`）
        if auction.sold && auction.expired {
            tracing::debug!(auction = auction.id, "条目既已售出又已过期，不能领取");
            return Ok(false);
        }
        let item_name = friendly_name(&self.item_info(auction.item.item_index).map_or_else(
            || auction.item.item_index.to_string(),
            |info| info.name.clone(),
        ));
        // 退货分支（`:8688-8708`）：`mode == Any || (mode == Expired && auction.Expired)`
        if (mode == MarketCollectionMode::Any
            || (mode == MarketCollectionMode::Expired && auction.expired))
            && (!auction.sold || auction.expired)
        {
            let Some(info) = self.item_info(auction.item.item_index).cloned() else {
                self.send_market_fail(player_id, MarketFailReason::NoSpace);
                return Err(MarketError::CannotGain);
            };
            if !self.can_gain_item(player_id, &info, auction.item.count) {
                self.send_market_fail(player_id, MarketFailReason::NoSpace);
                return Err(MarketError::CannotGain);
            }
            // 退还当前出价（`:8698-8703`）
            if let Some(buyer) = auction.current_buyer_character_id {
                let message = self.format_for_character(
                    buyer,
                    TextKey::MarketMailItemWithdrawn,
                    &[
                        ("item", &item_name),
                        ("gold", &auction.current_bid.to_string()),
                    ],
                );
                self.mail_system_to_character(buyer, None, auction.current_bid, &message);
            }
            self.gain_item(player_id, auction.item.clone());
            return Ok(true);
        }
        // 收款分支（`:8710-8725`）：`mode == Any || (mode == Sold && auction.Sold)`
        if mode == MarketCollectionMode::Any || (mode == MarketCollectionMode::Sold && auction.sold)
        {
            let cost = if auction.item_type == MarketItemType::Consign {
                auction.price
            } else {
                auction.current_bid
            };
            if !self.can_gain_gold(player_id, cost) {
                self.send_market_fail(player_id, MarketFailReason::CannotHoldGold);
                return Err(MarketError::CannotGain);
            }
            let gold = Auction::after_commission(cost);
            self.gain_gold(player_id, gold);
            let message = self.format(
                player_id,
                TextKey::MarketMailSoldCommission,
                &[
                    ("item", &item_name),
                    ("cost", &cost.to_string()),
                    ("gold", &gold.to_string()),
                ],
            );
            self.send_market_success(player_id, message);
            return Ok(true);
        }
        Ok(false)
    }

    /// 从全局列表里移除条目（Crystal `Envir.Auctions.Remove(auction)`）。
    fn remove_auction(&mut self, auction_id: u64) {
        self.auctions.retain(|auction| auction.id != auction_id);
    }

    fn send_market_fail(&self, player_id: u32, reason: MarketFailReason) {
        self.send_to_player(
            player_id,
            &ServerPacket::MarketFail(MarketFail {
                reason: reason.reason(),
            })
            .encode_frame(),
        );
    }

    fn send_market_success(&self, player_id: u32, message: String) {
        self.send_to_player(
            player_id,
            &ServerPacket::MarketSuccess(MarketSuccess { message }).encode_frame(),
        );
    }

    fn send_consign_ack(&self, player_id: u32, unique_id: u64, success: bool) {
        self.send_to_player(
            player_id,
            &ServerPacket::ConsignItem(ConsignItem { unique_id, success }).encode_frame(),
        );
    }
}

// ---------------------------------------------------------------------------
// 世界层：商城（GameShop，Crystal `PlayerObject.cs:13701-13946`）
// ---------------------------------------------------------------------------

impl World {
    /// 商城商品目录（Crystal `Envir.GameShopList`，`Envir.cs:119`）。
    ///
    /// 数据来自 `Server.MirDB` 的商城段，由 [`crate::gamedata::GameData`] 在启动时绑定
    /// 物品模板（Crystal `Main.BindGameShop`，`Envir.cs:4550-4561`：模板不存在的条目直接丢弃）。
    /// 没有加载数据库（或数据库里没有商城段）时是空列表——服务器照常启动，
    /// 购买回 [`GameShopError::NoGameData`] / [`GameShopError::ProductNotFound`]。
    #[must_use]
    pub fn game_shop(&self) -> &[GameShopItem] {
        match self.game_data() {
            Some(data) => data.game_shop(),
            None => &[],
        }
    }

    /// 按 `GIndex` 取一件商城商品（只读）。
    #[must_use]
    pub fn game_shop_item(&self, g_index: i32) -> Option<&GameShopItem> {
        self.game_shop()
            .iter()
            .find(|product| product.g_index == g_index)
    }

    /// 商城购买：`C.GameshopBuy`，Crystal `PlayerObject.GameshopBuy`
    /// （`Server/MirObjects/PlayerObject.cs:13725-13911`）。
    ///
    /// 校验顺序照抄 Crystal，并在服务端补一道职业/等级检查（Crystal 只在客户端按
    /// `GameShopItem.Class` 分组过滤，服务端不查；本作按**物品模板**的
    /// `RequiredClass` / `RequiredType::Level` 防改包）：
    ///
    /// 1. 商品存在（`:13749-13754`）；
    /// 2. 数量 `1..=99`（`:13727`）；
    /// 3. 单次不超过 [`GAMESHOP_MAX_STACKS`] 组（`:13756`）；
    /// 4. 职业/等级（本作扩展，见上）；
    /// 5. 库存：`Stock != 0` 时 `Stock - 已购 >= Quantity`（`:13758-13785`）；
    /// 6. 支付：`PType == 0` 元宝 / `1` 金币，且 `CanBuyCredit`/`CanBuyGold` 必须为真、
    ///    余额够（`:13791-13819`）；扣款并发 `S.LoseCredit`/`S.LoseGold`（`:13821-13836`）；
    /// 7. 发货：每份物品先看背包，放得下就 `gain_item`（`S.GainedItem`），
    ///    放不下的**攒成一封商城邮件**（Crystal 一律寄邮件，`:13900-13907`；本作见模块文档
    ///    第 3 条）。
    ///
    /// `purchases` 是本次校验用的已购计数快照，成功时返回的
    /// [`GameShopPurchaseReport`] 带**累加后**的计数值，由会话层与角色行同一事务写库
    /// （[`flush_gameshop_update`]）。
    ///
    /// # Errors
    /// 任何一项校验失败都返回 [`GameShopError`]，此时**不改任何状态、不发任何包**。
    pub fn gameshop_buy(
        &mut self,
        player_id: u32,
        g_index: i32,
        quantity: u8,
        p_type: i32,
        purchases: &GameShopPurchases,
    ) -> Result<GameShopPurchaseReport, GameShopError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(GameShopError::NotInWorld(player_id));
        };
        let character_id = player.character_id;
        let class = player.class;
        let level = player.level;
        let credit = player.credit;
        let gold = player.gold;
        // `if (Quantity < 1 || Quantity > 99) return;`（`:13727`）——Crystal 的第一步校验。
        if quantity == 0 || quantity > GAMESHOP_MAX_QUANTITY {
            return Err(GameShopError::InvalidQuantity(quantity));
        }
        if self.game_data().is_none() {
            return Err(GameShopError::NoGameData);
        }
        // `if (Product == null) { ReceiveChat(YouBuyItemNotInShop…); return; }`（`:13749-13754`）
        let Some(product) = self.game_shop_item(g_index).cloned() else {
            return Err(GameShopError::ProductNotFound { g_index });
        };
        let info = product.info.clone();
        // `if (((decimal)(Quantity * Product.Count) / StackSize) > 5) return;`（`:13756`）
        let total = u64::from(quantity) * u64::from(product.count.max(1));
        let stack_size = u64::from(info.stack_size.max(1));
        let stack_count = total.div_ceil(stack_size);
        if stack_count > GAMESHOP_MAX_STACKS {
            return Err(GameShopError::TooManyStacks {
                stacks: stack_count,
            });
        }
        // 职业（**本作扩展**：Crystal 服务端不查，只在客户端按 `GameShopItem.Class` 分组过滤；
        // 见 [`game_shop_class_allows`]）。
        if !game_shop_class_allows(&product, class) {
            return Err(GameShopError::WrongClass {
                class: product.class.clone(),
            });
        }
        // 等级（**本作扩展**）：商品没有等级字段，用物品模板的
        // `RequiredType::Level`/`MaxLevel` + `RequiredAmount`，与穿戴同一口径
        // （`crate::items::World::equip_item`）。
        match info.required_type {
            RequiredType::Level if level < u16::from(info.required_amount) => {
                return Err(GameShopError::WrongLevel {
                    required: info.required_amount,
                    level,
                });
            }
            RequiredType::MaxLevel if level > u16::from(info.required_amount) => {
                return Err(GameShopError::WrongLevel {
                    required: info.required_amount,
                    level,
                });
            }
            _ => {}
        }
        // 库存（`:13758-13785`）
        let remaining = purchases.remaining(&product);
        if let Some(remaining) = remaining
            && remaining < i64::from(quantity)
        {
            return Err(GameShopError::OutOfStock { remaining });
        }
        // 支付方式与余额（`:13791-13819`）
        let Some(payment) = GameShopPayment::from_wire(p_type) else {
            return Err(GameShopError::UnsupportedPayment(p_type));
        };
        let unit_price = match payment {
            GameShopPayment::Credit => product.credit_price,
            GameShopPayment::Gold => product.gold_price,
        };
        let cost = u64::from(unit_price) * u64::from(quantity);
        let cost_u32 = u32::try_from(cost).unwrap_or(u32::MAX);
        match payment {
            GameShopPayment::Credit => {
                // `if (Product.CanBuyCredit && cost <= Account.Credit)`（`:13794`）
                if !product.can_buy_credit {
                    return Err(GameShopError::CreditNotAllowed);
                }
                let held = credit.max(0);
                if u64::try_from(held).unwrap_or(0) < cost {
                    return Err(GameShopError::NotEnoughCredit { needed: cost, held });
                }
            }
            GameShopPayment::Gold => {
                // `if (Product.CanBuyGold && goldcost <= Account.Gold)`（`:13803`）
                if !product.can_buy_gold {
                    return Err(GameShopError::GoldNotAllowed);
                }
                let held = gold.max(0);
                if u64::try_from(held).unwrap_or(0) < cost {
                    return Err(GameShopError::NotEnoughGold { needed: cost, held });
                }
            }
        }
        // 扣款（`:13824-13836`）：`Account.Credit -= CreditCost` + `if (CreditCost != 0) S.LoseCredit`
        let deduction = i64::try_from(cost).unwrap_or(i64::MAX);
        if let Some(player) = self.players.get_mut(&player_id) {
            match payment {
                GameShopPayment::Credit => player.credit -= deduction,
                GameShopPayment::Gold => player.gold -= deduction,
            }
        }
        // Crystal 只在金额非 0 时发扣款包（`PlayerObject.cs:13826-13835`）。
        if cost != 0 {
            let lose_frame = match payment {
                GameShopPayment::Credit => {
                    ServerPacket::LoseCredit(LoseCredit { credit: cost_u32 }).encode_frame()
                }
                GameShopPayment::Gold => {
                    ServerPacket::LoseGold(LoseGold { gold: cost_u32 }).encode_frame()
                }
            };
            self.send_to_player(player_id, &lose_frame);
        }
        // 发货（`:13870-13910`）：先背包，放不下的攒一封商城邮件。
        let stacks = game_shop_stacks(self, &product, quantity);
        let mut gained = 0usize;
        let mut delivered = Vec::new();
        let mut mail_items = Vec::new();
        for item in stacks {
            if self.can_gain_item(player_id, &info, item.count) {
                delivered.push(GameShopDelivery {
                    item_index: item.item_index,
                    count: item.count,
                });
                let _ = self.gain_item(player_id, item);
                gained += 1;
            } else {
                mail_items.push(item);
            }
        }
        let mailed = mail_items.len();
        let mail_id = if mailed > 0 {
            let mail_id = self.mail_gameshop_to_character(character_id, mail_items);
            let message = self.format(
                player_id,
                TextKey::GameshopMailedItems,
                &[("count", &mailed.to_string())],
            );
            self.send_system_chat(player_id, &message);
            Some(mail_id)
        } else {
            None
        };
        // 计数（`:13838-13861`）：全服计数每次购买都推进；按角色计数只在「个人库存」时推进。
        let server_purchased =
            purchases.server.get(&product.g_index).copied().unwrap_or(0) + i64::from(quantity);
        let individual_purchased = (product.i_stock && product.stock != 0).then(|| {
            purchases
                .individual
                .get(&product.g_index)
                .copied()
                .unwrap_or(0)
                + i64::from(quantity)
        });
        Ok(GameShopPurchaseReport {
            g_index: product.g_index,
            product_name: friendly_name(&info.name),
            quantity,
            payment,
            cost: cost_u32,
            server_purchased,
            individual_purchased,
            remaining: remaining.map(|remaining| remaining - i64::from(quantity)),
            gained,
            mailed,
            delivered,
            mail_id,
        })
    }

    /// 商城面板当前是否「已在商城页签」（`C.MarketBuy` 的分派要用）。
    #[must_use]
    pub fn market_panel_of(&self, player_id: u32) -> MarketPanelType {
        self.market_searches
            .get(&player_id)
            .map_or(MarketPanelType::Market, |state| state.panel)
    }

    /// 扣元宝并发 `S.LoseCredit`（Crystal `NPCSegment.cs:3297-3305` 的 `TAKECREDIT`；
    /// 商城购买内部的扣款也走同一套算术，见 [`World::gameshop_buy`]）。
    ///
    /// 与 Crystal 一致：`credit` 超过持有量时按持有量扣
    /// （`if (credit >= player.Account.Credit) credit = player.Account.Credit;`），
    /// 返回**实际**扣掉的数额；0 时不发包。
    pub fn lose_credit(&mut self, player_id: u32, credit: u32) -> u32 {
        let Some(player) = self.players.get_mut(&player_id) else {
            return 0;
        };
        let held = u32::try_from(player.credit.max(0)).unwrap_or(0);
        let deducted = credit.min(held);
        if deducted == 0 {
            return 0;
        }
        player.credit -= i64::from(deducted);
        let _ = player
            .sender
            .send(ServerPacket::LoseCredit(LoseCredit { credit: deducted }).encode_frame());
        deducted
    }

    /// 商城购买落盘失败时的回滚：把 [`World::gameshop_buy`] 的内存副作用原样撤回。
    ///
    /// 触发条件见 `session::on_gameshop_buy`：`apply_gameshop_update` 事务失败时，客户端
    /// 虽然已经收到 `S.LoseCredit` / `S.GainedItem`，但数据库里一行都没变。若只记日志，
    /// 玩家下线时**角色行（扣款+背包）会落盘而库存计数不会**（计数只有
    /// `apply_gameshop_update` 写），重新登录后库存回退，等于凭空多卖。
    ///
    /// 本函数把状态恢复到购买前：
    /// * 元宝/金币加回，并发 `S.GainedCredit` / `S.GainedGold`；
    /// * 已进背包的物品按「模板 + 数量」（[`GameShopPurchaseReport::delivered`]）反向扣减
    ///   （可堆叠物品可能并进了旧堆，不能按 `UniqueID` 删）；
    /// * 商城邮件从运行期邮箱与待写队列（[`crate::mail::MailWrite`]）里删掉；
    /// * **不发**聊天：提示由会话层的失败分支统一发，避免连发两条。
    ///
    /// 客户端会看到物品「进来又消失」，但服务端状态与数据库完全一致——这是
    /// 「事务成功 ⇔ 内存变更保留」这条不变量的代价，见模块文档。
    pub fn rollback_gameshop_purchase(&mut self, player_id: u32, report: &GameShopPurchaseReport) {
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        match report.payment {
            GameShopPayment::Credit => player.credit += i64::from(report.cost),
            GameShopPayment::Gold => player.gold += i64::from(report.cost),
        }
        if report.cost != 0 {
            let refund = match report.payment {
                GameShopPayment::Credit => ServerPacket::GainedCredit(GainedCredit {
                    credit: report.cost,
                })
                .encode_frame(),
                GameShopPayment::Gold => {
                    ServerPacket::GainedGold(GainedGold { gold: report.cost }).encode_frame()
                }
            };
            let _ = player.sender.send(refund);
        }
        // 按「模板 + 数量」反向扣减：可堆叠物品可能并进了购买前就有的堆，
        // 按 `UniqueID` 删会删不到（那样退款后玩家净赚一份）。
        for delivery in &report.delivered {
            let mut remaining = delivery.count;
            for slot in player.inventory.iter_mut().rev() {
                if remaining == 0 {
                    break;
                }
                let Some(item) = slot else { continue };
                if item.item_index != delivery.item_index {
                    continue;
                }
                let take = item.count.min(remaining);
                item.count -= take;
                remaining -= take;
                if item.count == 0 {
                    *slot = None;
                }
            }
        }
        if let Some(mail_id) = report.mail_id {
            player.mail.mails.retain(|mail| mail.id != mail_id);
            self.mail_writes.retain(|write| {
                !matches!(write, crate::mail::MailWrite::Create(mail) if mail.id == mail_id)
            });
        }
        // 提示由会话层发（`session::on_gameshop_buy` 的失败分支），这里不重复聊天。
    }
}

/// 把一次购买拆成待投递的物品（Crystal `PlayerObject.cs:13870-13898`）。
///
/// * `StackSize <= 1` 或总数只有 1：每份一个物品（Crystal 在这一支忽略 `Product.Count`）；
/// * 否则按 `StackSize` 切成若干堆，最后一堆可能不满。
fn game_shop_stacks(
    world: &World,
    product: &GameShopItem,
    quantity: u8,
) -> Vec<mir_protocol::data::UserItem> {
    let info = &product.info;
    let total = u64::from(quantity) * u64::from(product.count.max(1));
    let stack_size = info.stack_size.max(1);
    let mut stacks = Vec::new();
    if stack_size <= 1 || total == 1 {
        // `for (int i = 0; i < Quantity; i++) mailItems.Add(Envir.CreateFreshItem(...));`
        for _ in 0..quantity {
            stacks.push(world.create_fresh_item(info));
        }
        return stacks;
    }
    // `while (quantity > 0) { … 每堆最多 StackSize 个 … }`
    let mut remaining = total;
    while remaining > 0 {
        let mut item = world.create_fresh_item(info);
        item.count = u16::try_from(remaining.min(u64::from(stack_size))).unwrap_or(u16::MAX);
        if item.count == 0 {
            break;
        }
        remaining -= u64::from(item.count);
        stacks.push(item);
    }
    stacks
}

/// 串行化「取最新挂单快照 + 落库」的进程级锁（刑部 t7 F1）。
///
/// 世界层是同步的、存储层是异步的：如果每个会话各自「先取快照、释放 world 锁、再 await
/// 写库」，两个会话交错时旧快照可能后提交，把新状态整表覆盖（买断/取回被回滚）。
/// 本锁把「取最新快照 + 单事务落库」整段串行化；锁在拿 world 锁之前获取、写库期间
/// 不持有 world 锁（不会阻塞 tick/其它会话）。
static MARKET_FLUSH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// 串行化「读已购计数 → 校验/扣款/发货 → 落盘」的进程级锁（商城）。
///
/// 已购计数存在数据库里（`gameshop_stock` / `gameshop_purchases`），世界层不缓存：
/// 如果两个会话各自「先读计数、再校验」，就会拿着同一份旧计数通过库存检查，把最后一件
/// 卖给两个人。会话层在处理一次商城购买的**整个**流程（含写库）里持有本锁；
/// 锁顺序固定为 `GAMESHOP_LOCK` → [`MARKET_FLUSH_LOCK`]（见 [`flush_gameshop_update`]），
/// 不会与市场链路互相死锁。
static GAMESHOP_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// 取 [`GAMESHOP_LOCK`] 的守卫（会话层在商城购买流程里持有到落盘结束）。
pub async fn lock_game_shop() -> tokio::sync::MutexGuard<'static, ()> {
    GAMESHOP_LOCK.lock().await
}

/// 单测专用：让**下一次** flush 在「取快照 → 落库」之间停顿，用来确定性复现 F1 交错。
#[cfg(test)]
static SLOW_NEXT_FLUSH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 用世界里的玩家状态生成角色行快照（市场原子落盘与下线存盘共用）。
///
/// 对应 Crystal `CharacterInfo.Save` 的字段编解码：背包/装备/仓库/技能/任务是字节块，
/// 金币/HP/MP/等级/经验/坐标/地图/行会归属逐字段同步。市场操作改了背包与金币，
/// 必须把这份快照和挂单放进同一个事务（F3）。
#[must_use]
pub fn character_snapshot(base: &Character, player: &Player, pet_mode: PetMode) -> Character {
    let mut character = base.clone();
    character.inventory = encode_slots(&player.inventory);
    character.equipment = encode_slots(&player.equipment);
    character.storage = crate::storage::encode_storage(&player.storage);
    character.magics = crate::magic::encode_magics(&player.magics);
    character.quests = crate::quests::encode_quests(&crate::quests::QuestSave {
        quests: player.quests.clone(),
        completed_quests: player.completed_quests.clone(),
    });
    character.gold = player.gold;
    character.credit = player.credit;
    character.hp = player.hp;
    character.mp = player.mp;
    character.level = player.level;
    character.experience = player.experience;
    character.x = player.position.x;
    character.y = player.position.y;
    character.direction = player.direction;
    character.map.clone_from(&player.map);
    // 绑定回城点（Crystal `CharacterInfo.BindMapIndex/BindLocation`）：世界侧是活值，
    // 市场/商城/下线任何一次落盘都必须带上，否则回城点会被登录时的 base 覆盖。
    character.bind_map_index = player.bind_map_index;
    character.bind_x = player.bind_point.x;
    character.bind_y = player.bind_point.y;
    character.guild_index = player.guild.map_or(-1, |reference| reference.guild_id);
    // 宠物命令模式（Crystal `CharacterInfo.PMode`，单字节；运行时在 World 侧表）。
    character.pet_mode = i16::from(pet_mode.value());
    character
}

/// 原子落盘一次市场变更（F1/F3）：角色行 + 邮件 + 挂单快照同一事务。
///
/// 流程：
/// 1. 拿 [`MARKET_FLUSH_LOCK`]（独立于 world 锁，串行化所有会话的市场落盘）；
/// 2. 拿 world 锁取**最新**挂单快照与待写邮件队列，随即释放；
/// 3. 调 [`AccountStore::apply_market_update`] 在单事务里写角色行/邮件/挂单。
///
/// `character` 是发起本次操作的在线角色快照（[`character_snapshot`]）；启动过期清理
/// 传 `None`。对手方通过 `mail_writes` 收款/收货，不跨会话写对方角色行。
///
/// **不变量**：客户端可见的市场背包/金币变化与 `market_auctions` 行同事务提交；
/// 崩溃只会发生在「都没写」或「都写了」两种状态，不会出现物品/金币复制或丢失。
///
/// **邮件队列（刑部 t11 medium / t12）**：邮件与事务一起提交，但只有**事务成功后**队列才
/// 真正算清空。若 `apply_market_update` 失败，本函数会把已 drain 的邮件按原顺序回灌到
/// `World::mail_writes` 队首（期间新追加的邮件排在其后），下一次 flush 会重试同一批邮件。
/// 因此 DB 故障不会让退款/通知邮件消失。
///
/// # Errors
/// 存储层失败时返回错误，调用方记日志；下一次操作会以新快照重试整组写入。
pub async fn flush_market_update<S: AccountStore>(
    store: &S,
    world: &tokio::sync::Mutex<World>,
    character: Option<Character>,
) -> Result<(), StoreError> {
    commit_market_like(store, world, character, None).await
}

/// 原子落盘一次商城购买：角色行（含元宝）+ 商城邮件 + 已购计数同一事务。
///
/// 与 [`flush_market_update`] 共用 [`MARKET_FLUSH_LOCK`] 与邮件的回灌逻辑，区别只有
/// 事务里写的是 `gameshop_purchases` / `gameshop_stock`（见
/// [`AccountStore::apply_gameshop_update`]）而不是挂单快照——商城购买不会改挂单，
/// 也就不需要把整张 `market_auctions` 重写一遍。
///
/// `update` 里只带本次购买的计数行（[`GameShopPurchaseReport::to_records`]），
/// 角色行快照与待写邮件由本函数补齐（邮件先 drain 再随事务提交，失败时回灌队列）。
///
/// # Errors
/// 存储层失败时返回错误，**调用方必须回滚**：此时客户端已经收到扣款/到货，但数据库里
/// 一行都没变；`gameshop_stock` / `gameshop_purchases` 只由这个事务写，只记日志的话
/// 玩家下线时角色行会落盘而库存计数不会，重新登录后库存回退 → 超卖。
/// `session::on_gameshop_buy` 在失败分支调用 [`World::rollback_gameshop_purchase`]。
pub async fn flush_gameshop_update<S: AccountStore>(
    store: &S,
    world: &tokio::sync::Mutex<World>,
    character: Option<Character>,
    update: GameShopUpdate,
) -> Result<(), StoreError> {
    commit_market_like(store, world, character, Some(update)).await
}

/// [`flush_market_update`] 与 [`flush_gameshop_update`] 的共用实现。
///
/// `gameshop` 为 `None` 时走「挂单全量快照」事务；为 `Some` 时走「商城计数」事务。
/// 两条路都串行化在 [`MARKET_FLUSH_LOCK`] 下：它们都会写当前角色行，若交错提交，
/// 旧快照可能覆盖新快照（F1 的同类问题）。
async fn commit_market_like<S: AccountStore>(
    store: &S,
    world: &tokio::sync::Mutex<World>,
    character: Option<Character>,
    gameshop: Option<GameShopUpdate>,
) -> Result<(), StoreError> {
    let _flush_guard = MARKET_FLUSH_LOCK.lock().await;
    let (auctions, mut mail_writes) = {
        let mut world = world.lock().await;
        // 商城事务不写挂单：省掉一次整表快照克隆。
        let auctions = if gameshop.is_some() {
            Vec::new()
        } else {
            world.auction_snapshot()
        };
        (auctions, world.take_mail_writes())
    };
    // 仅单测：把「取快照 → 落库」窗口拉长，让并发用例能确定性复现 F1 的交错。
    // 集成测试链接的是正常编译的库，不会走这里。
    #[cfg(test)]
    if SLOW_NEXT_FLUSH.swap(false, std::sync::atomic::Ordering::SeqCst) {
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let pending = mail_writes.clone();
    let result = match gameshop {
        Some(update) => {
            store
                .apply_gameshop_update(GameShopUpdate {
                    character,
                    mail_writes: pending,
                    ..update
                })
                .await
        }
        None => {
            store
                .apply_market_update(MarketUpdate {
                    character,
                    mail_writes: pending,
                    auctions,
                })
                .await
        }
    };
    if let Err(error) = &result {
        // t12：事务失败 → 邮件没有落库，必须放回队列，等待下一次 flush 重试。
        // 期间别的路径可能往队尾追加了新邮件；把本次 drain 的放在前面，保持原顺序。
        let mut world = world.lock().await;
        let restored = mail_writes.len();
        mail_writes.append(&mut world.mail_writes);
        world.mail_writes = mail_writes;
        tracing::warn!(
            %error,
            restored,
            "市场事务失败：已把待写邮件回灌队列"
        );
    }
    result
}

/// 原子落盘一批「拍卖到期自动成交」。
///
/// 内容与 [`flush_market_update`] 完全一致——买家物品邮件 + 卖家金币邮件 + 挂单快照
/// （已不含成交的挂单）在**同一个事务**里提交，因此「物品寄出」「金币寄出」「挂单删除」
/// 不可能只成功一半。世界层的成交结果由 [`World::process_auctions`] 先写入内存，
/// 本函数只负责落盘：
///
/// * **成功**：DB 与内存一致（挂单已删、两封邮件在库里）；重启后同样一致。
/// * **失败**：事务一行未写，已 drain 的邮件按原顺序回灌 [`World::mail_writes`]
///   （与 [`flush_market_update`] 同一套逻辑）；内存里的成交结果**保持**，调用方
///   （`server.rs` 的 tick 任务）在下一轮重试本函数即可提交。**不回滚挂单**是刻意的：
///   其它会话的市场操作也会 flush 整个邮件队列与快照，若这里把挂单放回世界，
///   可能与「邮件已被别的会话提交」叠成二次成交/双发。
/// * **进程崩溃**（内存改了、事务未提交）：库里挂单仍在且已过期，重启的启动路径
///   （[`World::process_auctions`] → [`World::purge_expired_auctions`]）重新结算，
///   邮件只会在那次事务里写一次，不会双发。
///
/// # Errors
/// 存储层失败时返回错误，调用方记日志并在下一轮重试（`server.rs` 的 tick 任务用
/// 短退避重试）。
pub async fn flush_auction_sales<S: AccountStore>(
    store: &S,
    world: &tokio::sync::Mutex<World>,
) -> Result<(), StoreError> {
    flush_market_update(store, world, None).await
}

/// 把当前挂单快照落到存储层（整表替换）。
///
/// 会话在上架/买断/提前成交/取回成功后调用（见 `session.rs` 的统一 flush 钩子），
/// 服务器启动清理过期挂单后也用它落盘。世界层是同步的、存储层是异步的，所以由调用方
/// 先取 [`World::auction_snapshot`]，再在包处理之外 `await` 本函数。
///
/// **注意**：本函数只写挂单，不写角色行/邮件；正常市场链路应使用
/// [`flush_market_update`]（F3 原子性）。保留它给工具/测试做低层挂单写入。
///
/// # Errors
/// 存储层失败时返回错误，调用方只记日志：下一轮操作会拿新快照重试整份写入，
/// 不需要补偿事务。
pub async fn persist_auctions<S: AccountStore>(
    store: &S,
    snapshot: Vec<AuctionRecord>,
) -> Result<(), StoreError> {
    store.save_auctions(snapshot).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commission_matches_crystal_formula() {
        // `(uint)Math.Max(0, cost - cost * Globals.Commission)`（`PlayerObject.cs:8640`）。
        assert_eq!(Auction::after_commission(5_000), 4_750);
        assert_eq!(Auction::after_commission(0), 0);
        // 100 * 0.05 = 5 → 95
        assert_eq!(Auction::after_commission(100), 95);
    }

    #[test]
    fn consign_labels_match_crystal() {
        // 期望值从语言模块取（不写死中文），并顺带断言中英两套都成立。
        use mir_core::Language;
        use mir_core::i18n::text;
        let zh = Language::ZhCn;
        let en = Language::En;
        let mut auction = Auction {
            id: 1,
            item: UserItem::default(),
            consignment_date: DateTimeBinary::default(),
            price: 5_000,
            current_bid: 0,
            seller_character_id: 1,
            seller_name: "seller_a".to_owned(),
            current_buyer_character_id: None,
            current_buyer_name: None,
            expired: false,
            sold: false,
            item_type: MarketItemType::Consign,
        };
        assert_eq!(auction.seller_label(false, zh), "seller_a");
        assert_eq!(
            auction.seller_label(true, zh),
            text(zh, TextKey::MarketLabelOnSale)
        );
        assert_eq!(
            auction.seller_label(true, en),
            text(en, TextKey::MarketLabelOnSale)
        );
        auction.sold = true;
        assert_eq!(
            auction.seller_label(true, zh),
            text(zh, TextKey::MarketLabelSold)
        );
        assert_eq!(
            auction.seller_label(true, en),
            text(en, TextKey::MarketLabelSold)
        );
        auction.sold = false;
        auction.expired = true;
        assert_eq!(
            auction.seller_label(true, zh),
            text(zh, TextKey::MarketLabelExpired)
        );

        // 拍卖：`CurrentBid > Price ? "Bid Met" : "No Bid"`（`AuctionInfo.cs:98`）。
        let mut auction = Auction {
            item_type: MarketItemType::Auction,
            current_bid: 5_000,
            price: 5_000,
            expired: false,
            sold: false,
            ..auction
        };
        assert_eq!(
            auction.seller_label(true, zh),
            text(zh, TextKey::MarketLabelNoBid)
        );
        auction.current_bid = 6_000;
        assert_eq!(
            auction.seller_label(true, en),
            text(en, TextKey::MarketLabelHasBid)
        );
        // `CreateClientAuction` 的 `Price` 取 `CurrentBid`（`AuctionInfo.cs:111`）。
        assert_eq!(auction.to_client_auction(false, zh).price, 6_000);
    }

    #[test]
    fn market_state_defaults_match_crystal() {
        let state = MarketSearchState::default();
        assert_eq!(state.min_shapes, 0);
        assert_eq!(state.max_shapes, 0);
        assert_eq!(state.match_type, ItemType::Nothing);
        assert!(!state.user_match);
        assert_eq!(state.panel, MarketPanelType::Market);
    }

    #[test]
    fn market_fail_reasons_match_crystal_numbers() {
        assert_eq!(MarketFailReason::Dead.reason(), 0);
        assert_eq!(MarketFailReason::NoDialog.reason(), 1);
        assert_eq!(MarketFailReason::Sold.reason(), 2);
        assert_eq!(MarketFailReason::Expired.reason(), 3);
        assert_eq!(MarketFailReason::LowGold.reason(), 4);
        assert_eq!(MarketFailReason::NoSpace.reason(), 5);
        assert_eq!(MarketFailReason::OwnItem.reason(), 6);
        assert_eq!(MarketFailReason::TooFar.reason(), 7);
        assert_eq!(MarketFailReason::CannotHoldGold.reason(), 8);
        assert_eq!(MarketFailReason::BidTooLow.reason(), 9);
        assert_eq!(MarketFailReason::AuctionEnded.reason(), 10);
    }

    #[test]
    fn auction_record_roundtrip_keeps_every_field() {
        // 一条带出价、带买家名字的拍卖：字段与 `AuctionInfo.Save`（`AuctionInfo.cs:71-87`）一致。
        let auction = Auction {
            id: 42,
            item: UserItem {
                unique_id: 7,
                item_index: 3,
                count: 2,
                ..UserItem::default()
            },
            consignment_date: DateTimeBinary::from_unix_seconds(1_700_000_000),
            price: 5_000,
            current_bid: 6_000,
            seller_character_id: 11,
            seller_name: "seller".to_owned(),
            current_buyer_character_id: Some(22),
            current_buyer_name: Some("buyer".to_owned()),
            expired: false,
            sold: false,
            item_type: MarketItemType::Auction,
        };

        let record = auction.to_record();
        assert!(
            !record.item.is_empty(),
            "item must serialize to a non-empty blob"
        );
        let loaded = Auction::from_record(record).expect("record round-trips");
        assert_eq!(loaded, auction);
    }

    #[test]
    fn corrupt_item_blob_is_rejected() {
        let record = AuctionRecord {
            id: 1,
            item: vec![0xFF, 0xFF, 0xFF, 0xFF],
            ..AuctionRecord::default()
        };
        assert!(Auction::from_record(record).is_none());
    }

    fn sample_auction(id: u64) -> Auction {
        Auction {
            id,
            item: UserItem::default(),
            consignment_date: DateTimeBinary::default(),
            price: 5_000,
            current_bid: 0,
            seller_character_id: 1,
            seller_name: "seller".to_owned(),
            current_buyer_character_id: None,
            current_buyer_name: None,
            expired: false,
            sold: false,
            item_type: MarketItemType::Consign,
        }
    }

    /// 到期标记与自动成交统一由节流的 `process_auctions` 负责；`tick_market`（`world.rs`
    /// 每 100ms 调一次）必须是 O(1)，不能每 tick 全表扫描（合同 t22 第 2 条）。
    #[test]
    fn process_auctions_owns_expiry_and_auto_sale() {
        let mut world = World::new(999);
        let now = world.wall_now();
        let old = DateTimeBinary::from_unix_seconds(
            now / 1000 - (CONSIGNMENT_LENGTH_DAYS + 1) * 24 * 60 * 60,
        );
        // 1 号：拍卖、有高于起拍价的出价 → 自动成交（不置过期）。
        let mut sellable = sample_auction(1);
        sellable.item_type = MarketItemType::Auction;
        sellable.consignment_date = old;
        sellable.current_bid = 6_000;
        sellable.current_buyer_character_id = Some(2);
        // 2 号：拍卖但没有出价 → 置过期。
        let mut plain = sample_auction(2);
        plain.item_type = MarketItemType::Auction;
        plain.consignment_date = old;
        world.auctions.push(sellable);
        world.auctions.push(plain);

        // 每 tick 的入口不再扫描：到期标记不会在这里发生。
        world.tick_market();
        assert!(
            !world.auction(1).expect("auction 1 exists").expired
                && !world.auction(2).expect("auction 2 exists").expired,
            "tick_market must stay O(1): it does not scan the auction table"
        );

        // 节流的扫描：成交一条、另一条置过期。
        let sold = world.process_auctions(now);
        assert_eq!(
            sold.len(),
            1,
            "an expired auction with a higher bid must sell"
        );
        assert!(world.auction(1).is_none(), "a sold auction leaves the list");
        assert!(
            world.auction(2).expect("auction 2 exists").expired,
            "an expired auction without a qualifying bid is only marked expired"
        );
        assert_eq!(
            world.pending_mail_writes(),
            2,
            "one mail for the buyer item and one for the seller gold"
        );
    }

    /// F1：两个会话交错 flush 时，后提交的必须是**最新**快照。
    ///
    /// `flush_market_update` 在 `cfg(test)` 下会在「取快照 → 落库」之间 sleep 25ms。
    /// 没有 [`MARKET_FLUSH_LOCK`] 时：任务 1 先取 [1]，主线程改出 [2,1]，任务 2 取 [2,1]
    /// 先落库，任务 1 醒来再用 [1] 整表覆盖 → 断言失败。有锁时任务 2 必须等任务 1 落完，
    /// 再取最新快照 [2,1]，最终状态正确。
    #[tokio::test]
    async fn concurrent_flushes_keep_the_latest_snapshot() {
        use std::sync::Arc;

        use crate::store::MemoryStore;

        let store = Arc::new(MemoryStore::new());
        // `apply_market_update` 会校验挂单的卖家外键；先建一个角色（id 固定从 1 开始）。
        let account = store
            .create_account("f1", "secret", "")
            .await
            .expect("create test account");
        store
            .create_character(
                account.id,
                "F1",
                mir_core::enums::MirClass::Warrior,
                mir_core::enums::MirGender::Male,
                "0",
                (0, 0),
            )
            .await
            .expect("create test character");
        let world = Arc::new(tokio::sync::Mutex::new(World::new(999)));
        world.lock().await.auctions.push(sample_auction(1));

        // 只让任务 1 在取到快照后停顿：没有锁时任务 2 会先落最新快照、任务 1 再用旧快照覆盖。
        SLOW_NEXT_FLUSH.store(true, std::sync::atomic::Ordering::SeqCst);
        let first = {
            let store = Arc::clone(&store);
            let world = Arc::clone(&world);
            tokio::spawn(
                async move { flush_market_update(store.as_ref(), world.as_ref(), None).await },
            )
        };
        // 给任务 1 时间拿锁并取到「只有 1 号」的快照（随后进入 25ms 的落库窗口）。
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // 会话 2 在这期间改世界：新增 2 号挂单。
        world.lock().await.auctions.insert(0, sample_auction(2));
        let second = {
            let store = Arc::clone(&store);
            let world = Arc::clone(&world);
            tokio::spawn(
                async move { flush_market_update(store.as_ref(), world.as_ref(), None).await },
            )
        };

        first.await.expect("task 1 joins").expect("task 1 flushes");
        second.await.expect("task 2 joins").expect("task 2 flushes");

        let ids: Vec<u64> = store.auctions().await.iter().map(|row| row.id).collect();
        assert_eq!(
            ids,
            vec![2, 1],
            "the latest snapshot must win over a stale one"
        );
    }
}
