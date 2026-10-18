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
//! | [`World::tick_market`] | `Envir.Process` 里的过期标记（`Server/MirEnvir/Envir.cs:2396`） |
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
//!
//! # 与 Crystal 的**有意**差异
//!
//! 1. **落盘方式**：Crystal 把挂单写进 `MirADB` 的全局 `Auctions` 段
//!    （`Envir.SaveAccounts`，`Server/MirEnvir/Envir.cs:2591-2594`；`AuctionInfo.Save`，
//!    `AuctionInfo.cs:71-87`），读档时靠 `BindCharacter` 重新绑定卖家/买家
//!    （`Envir.cs:3018-3029`）。本作落 `market_auctions` 表（`migrations/0009_market.sql`）：
//!    启动时 [`World::load_auctions`]、每次市场操作成功后整表快照回写
//!    （[`persist_auctions`]）、下线再补一次；卖家/买家名字随条目冗余保存。
//!    启动时会调用 [`World::purge_expired_auctions`] 把已过期挂单清掉（物品/最高出价按
//!    系统邮件退回）；Crystal 只把 `Expired` 置真、等卖家 `MarketGetBack` 取回
//!    （`Envir.cs:2396`、`PlayerObject.cs:8668-8793`）。
//! 2. **没有商城（GameShop）**：Crystal 的 `MarketPanelType.GameShop` 读 `Envir.GameShopList`
//!    （`PlayerObject.cs:8385-8390` 里那段还被注释掉了）。本作没有商城数据，游戏商城页签
//!    永远返回空列表。
//! 3. **`Envir.MessageAccount` 只给在线角色发**：Crystal 是写进账号的消息队列
//!    （`Envir.MessageAccount`），离线也能收到；本作只给在线角色发系统聊天，离线丢弃。
//! 4. **`ConsignItem` 末尾的重复回执**：Crystal 在成功分支 `Enqueue(p)` 之后，函数末尾
//!    又 `Enqueue(p)` 一次（`PlayerObject.cs:8304-8305`、`:8315`）。本作只发一次。
//! 5. **「搜索」状态放在世界层**：Crystal 的 `Search` / `PageSent` / `MatchName` / `MatchType` /
//!    `UserMatch` / `MinShapes` / `MaxShapes` / `MarketPanelType` / `SearchTime` 是
//!    `PlayerObject` 的字段（`PlayerObject.cs:132-139`）；本作放在
//!    [`World::market_searches`]，下线即清（与 Crystal 随对象销毁等价）。
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
use mir_core::enums::{BindMode, ItemType, MarketCollectionMode, MarketItemType, MarketPanelType};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ClientAuction, UserItem};
use mir_protocol::packets::server::{
    ConsignItem, MarketFail, MarketSuccess, NPCConsign, NPCMarket, NPCMarketPage,
};

use mir_db::ItemInfo;

use crate::items::{decode_slots, encode_slots, friendly_name};
use crate::store::{AccountStore, AuctionRecord, Character, MarketUpdate, StoreError};
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
pub const CONSIGNMENT_LENGTH_DAYS: i64 = 7;

/// 成交佣金比例：`Globals.Commission = 0.05F`（`Globals.cs:39`）。
pub const COMMISSION: f64 = 0.05;

/// 搜索/翻页节流：`Globals.SearchDelay = 500`（`Globals.cs:41`）。
pub const SEARCH_DELAY_MS: i64 = 500;

/// 每页条目数：Crystal 固定 10（`PlayerObject.cs:8356`、`:8405`）。
pub const MARKET_PAGE_SIZE: usize = 10;

/// 打开寄售（丢物品定价）面板的 NPC 页面键：`NPCScript.ConsignKey = "[@CONSIGN]"`（`NPCScript.cs:56`）。
pub const CONSIGN_KEY: &str = "[@CONSIGN]";

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
    /// 卖家展示文本：Crystal `AuctionInfo.GetSellerLabel`（`AuctionInfo.cs:89-102`），中文化。
    #[must_use]
    pub fn seller_label(&self, user_match: bool) -> String {
        match self.item_type {
            // `return userMatch ? (Sold ? "Sold" : (Expired ? "Expired" : "For Sale")) : SellerInfo.Name;`
            MarketItemType::Consign => {
                if user_match {
                    if self.sold {
                        "已售出".to_owned()
                    } else if self.expired {
                        "已过期".to_owned()
                    } else {
                        "出售中".to_owned()
                    }
                } else {
                    self.seller_name.clone()
                }
            }
            // `return userMatch ? (Sold ? "Sold" : (Expired ? "Expired" : CurrentBid > Price ? "Bid Met" : "No Bid")) : SellerInfo.Name;`
            MarketItemType::Auction => {
                if user_match {
                    if self.sold {
                        "已售出".to_owned()
                    } else if self.expired {
                        "已过期".to_owned()
                    } else if self.current_bid > self.price {
                        "已有出价".to_owned()
                    } else {
                        "无人出价".to_owned()
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
    pub fn to_client_auction(&self, user_match: bool) -> ClientAuction {
        ClientAuction {
            auction_id: self.id,
            item: self.item.clone(),
            seller: self.seller_label(user_match),
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
    #[error("对象 {0} 不在世界里")]
    NotInWorld(u32),
    /// 死亡状态。
    #[error("死亡状态不能用市场")]
    Dead,
    /// 没通过 NPC 打开对应面板（Crystal `NPCPage == null` / 键不对）。
    #[error("没有通过 NPC 打开市场面板")]
    NoDialog,
    /// 当前地图上找不到对话过的 NPC。
    #[error("找不到对话的 NPC {object_id}")]
    NoSuchNpc {
        /// NPC 对象 ID。
        object_id: u32,
    },
    /// 离 NPC 太远。
    #[error("离 NPC {object_id} 太远（上限 {range}）")]
    OutOfRange {
        /// NPC 对象 ID。
        object_id: u32,
        /// 上限。
        range: i32,
    },
    /// 面板类型不对（`MarketPanelType::Market` 不能用来上架）。
    #[error("面板类型 {0:?} 不能上架")]
    InvalidPanel(MarketPanelType),
    /// 价格越界。
    #[error("价格 {price} 不在 [{min}, {max}] 内")]
    PriceOutOfRange {
        /// 给定价格。
        price: u32,
        /// 下限。
        min: u32,
        /// 上限。
        max: u32,
    },
    /// 金币不够（手续费/购买）。
    #[error("金币不够：需要 {needed}，只有 {held}")]
    NotEnoughGold {
        /// 需要。
        needed: u32,
        /// 持有。
        held: i64,
    },
    /// 背包里找不到这件物品。
    #[error("背包里找不到物品 {0}")]
    ItemNotFound(u64),
    /// 物品绑定「禁止出售」（`BindMode.DontSell`，`PlayerObject.cs:8288`）。
    #[error("这件物品不能出售")]
    ItemUnsellable,
    /// 背包/金币收不下。
    #[error("收不下")]
    CannotGain,
    /// 搜索列表里没有这条拍卖（没搜过/已被买走）。
    #[error("搜索列表里没有拍卖 {0}")]
    ListingNotFound(u64),
    /// 已经卖出。
    #[error("拍卖 {0} 已经卖出")]
    ListingSold(u64),
    /// 已经过期。
    #[error("拍卖 {0} 已经过期")]
    ListingExpired(u64),
    /// 自己的挂单。
    #[error("不能买自己挂的东西")]
    OwnListing,
    /// 出价低于当前最高价。
    #[error("出价 {bid} 低于当前价 {current}")]
    BidTooLow {
        /// 当前价。
        current: u32,
        /// 出价。
        bid: u32,
    },
    /// 拍卖还没有人出价。
    #[error("拍卖还没有人出价")]
    NoBid,
    /// 拍卖已结束。
    #[error("拍卖已经结束")]
    AuctionEnded,
    /// 节流（`Globals.SearchDelay`）。
    #[error("搜索太频繁")]
    Throttled,
    /// 这个面板还没做（商城）。
    #[error("商城面板未实现")]
    UnsupportedPanel,
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
                let message = format!(
                    "你在 {item_name} 上的出价已退回：拍卖已过期（{} 金币）。",
                    auction.current_bid
                );
                self.mail_system_to_character(buyer, None, auction.current_bid, &message);
            }
            let message = format!("你挂出的 {item_name} 已过期，物品已通过邮件退回。");
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
        let listings: Vec<ClientAuction> = state
            .listings
            .iter()
            .skip(start)
            .take(MARKET_PAGE_SIZE)
            .filter_map(|auction_id| self.auction(*auction_id))
            .map(|auction| auction.to_client_auction(state.user_match))
            .collect();
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
                let seller_message =
                    format!("你挂出的 {item_name} 已售出，收入 {} 金币。", auction.price);
                self.message_character(auction.seller_character_id, &seller_message);
                self.send_market_success(
                    player_id,
                    format!("你以 {} 金币买下了 {item_name}。", auction.price),
                );
                let _ = self.market_refresh(player_id);
                Ok(())
            }
            // 拍卖出价（`:8556-8581`）
            MarketItemType::Auction | MarketItemType::GameShop => {
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
                    let refund = format!(
                        "你在 {item_name} 上的出价被超过，{} 金币已通过邮件退回。",
                        auction.current_bid
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
                let seller_message = format!("有人为 {item_name} 出价 {bid_price} 金币。");
                self.message_character(auction.seller_character_id, &seller_message);
                self.send_market_success(
                    player_id,
                    format!("你为 {item_name} 出价 {bid_price} 金币。"),
                );
                let _ = self.market_refresh(player_id);
                Ok(())
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
            let message = format!("你在拍卖中赢得了 {item_name}，成交价 {cost} 金币。");
            self.mail_system_to_character(buyer, Some(auction.item.clone()), 0, &message);
        }
        self.remove_auction(auction_id);
        // `GainGold(gold);`（`:8657`）
        self.gain_gold(player_id, gold);
        self.send_market_success(
            player_id,
            format!("你以 {cost} 金币卖出了 {item_name}，扣除佣金后收入 {gold} 金币。"),
        );
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

    /// 过期标记：Crystal `Envir.Process`（`Server/MirEnvir/Envir.cs:2396`）：
    /// `if (!info.Expired && !info.Sold && Now >= info.ConsignmentDate.AddDays(Globals.ConsignmentLength)) info.Expired = true;`
    pub fn tick_market(&mut self) {
        let now = self.wall_now();
        for auction in &mut self.auctions {
            if !auction.expired && !auction.sold && Self::auction_expired_at(auction, now) {
                auction.expired = true;
            }
        }
    }

    /// 条目是否已到 [`CONSIGNMENT_LENGTH_DAYS`] 天。
    fn auction_expired_at(auction: &Auction, now_ms: i64) -> bool {
        let length = CONSIGNMENT_LENGTH_DAYS * 24 * 60 * 60;
        now_ms / 1000 >= auction.consignment_date.to_unix_seconds() + length
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
    fn get_market(&mut self, player_id: u32, match_text: &str, item_type: ItemType) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let character_id = player.character_id;
        let mut state = self.market_searches.remove(&player_id).unwrap_or_default();
        // `MatchName = name.Replace(" ", ""); MatchType = type; PageSent = 0;`（`:8379-8381`）
        state.match_name = match_text.replace(' ', "");
        state.match_type = item_type;
        state.page_sent = 0;
        // 搜索结果：`UserMatch ? Account.Auctions : Envir.Auctions`（`:8393-8399`）
        let mut listings = Vec::new();
        if state.panel != MarketPanelType::GameShop {
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
        let shown: Vec<ClientAuction> = listings
            .iter()
            .take(MARKET_PAGE_SIZE)
            .filter_map(|id| self.auction(*id))
            .map(|auction| auction.to_client_auction(state.user_match))
            .collect();
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
                let message = format!(
                    "你挂出的 {item_name} 已被卖家下架，{} 金币已通过邮件退回。",
                    auction.current_bid
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
            self.send_market_success(
                player_id,
                format!("你卖出了 {item_name}，成交价 {cost}，扣除佣金后收入 {gold} 金币。"),
            );
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

/// 串行化「取最新挂单快照 + 落库」的进程级锁（刑部 t7 F1）。
///
/// 世界层是同步的、存储层是异步的：如果每个会话各自「先取快照、释放 world 锁、再 await
/// 写库」，两个会话交错时旧快照可能后提交，把新状态整表覆盖（买断/取回被回滚）。
/// 本锁把「取最新快照 + 单事务落库」整段串行化；锁在拿 world 锁之前获取、写库期间
/// 不持有 world 锁（不会阻塞 tick/其它会话）。
static MARKET_FLUSH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// 单测专用：让**下一次** flush 在「取快照 → 落库」之间停顿，用来确定性复现 F1 交错。
#[cfg(test)]
static SLOW_NEXT_FLUSH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 用世界里的玩家状态生成角色行快照（市场原子落盘与下线存盘共用）。
///
/// 对应 Crystal `CharacterInfo.Save` 的字段编解码：背包/装备/仓库/技能/任务是字节块，
/// 金币/HP/MP/等级/经验/坐标/地图/行会归属逐字段同步。市场操作改了背包与金币，
/// 必须把这份快照和挂单放进同一个事务（F3）。
#[must_use]
pub fn character_snapshot(base: &Character, player: &Player) -> Character {
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
    character.guild_index = player.guild.map_or(-1, |reference| reference.guild_id);
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
    let _flush_guard = MARKET_FLUSH_LOCK.lock().await;
    let (auctions, mut mail_writes) = {
        let mut world = world.lock().await;
        (world.auction_snapshot(), world.take_mail_writes())
    };
    // 仅单测：把「取快照 → 落库」窗口拉长，让并发用例能确定性复现 F1 的交错。
    // 集成测试链接的是正常编译的库，不会走这里。
    #[cfg(test)]
    if SLOW_NEXT_FLUSH.swap(false, std::sync::atomic::Ordering::SeqCst) {
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let pending = mail_writes.clone();
    let result = store
        .apply_market_update(MarketUpdate {
            character,
            mail_writes: pending,
            auctions,
        })
        .await;
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
        let mut auction = Auction {
            id: 1,
            item: UserItem::default(),
            consignment_date: DateTimeBinary::default(),
            price: 5_000,
            current_bid: 0,
            seller_character_id: 1,
            seller_name: "甲".to_owned(),
            current_buyer_character_id: None,
            current_buyer_name: None,
            expired: false,
            sold: false,
            item_type: MarketItemType::Consign,
        };
        assert_eq!(auction.seller_label(false), "甲");
        assert_eq!(auction.seller_label(true), "出售中");
        auction.sold = true;
        assert_eq!(auction.seller_label(true), "已售出");
        auction.sold = false;
        auction.expired = true;
        assert_eq!(auction.seller_label(true), "已过期");

        // 拍卖：`CurrentBid > Price ? "Bid Met" : "No Bid"`（`AuctionInfo.cs:98`）。
        let mut auction = Auction {
            item_type: MarketItemType::Auction,
            current_bid: 5_000,
            price: 5_000,
            expired: false,
            sold: false,
            ..auction
        };
        assert_eq!(auction.seller_label(true), "无人出价");
        auction.current_bid = 6_000;
        assert_eq!(auction.seller_label(true), "已有出价");
        // `CreateClientAuction` 的 `Price` 取 `CurrentBid`（`AuctionInfo.cs:111`）。
        assert_eq!(auction.to_client_auction(false).price, 6_000);
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
            seller_name: "卖家".to_owned(),
            current_buyer_character_id: Some(22),
            current_buyer_name: Some("买家".to_owned()),
            expired: false,
            sold: false,
            item_type: MarketItemType::Auction,
        };

        let record = auction.to_record();
        assert!(!record.item.is_empty(), "物品应序列化成非空字节块");
        let loaded = Auction::from_record(record).expect("记录应能还原");
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
            seller_name: "卖家".to_owned(),
            current_buyer_character_id: None,
            current_buyer_name: None,
            expired: false,
            sold: false,
            item_type: MarketItemType::Consign,
        }
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
            .expect("建测试账号");
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
            .expect("建测试角色");
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

        first.await.expect("任务 1 可 join").expect("任务 1 落盘");
        second.await.expect("任务 2 可 join").expect("任务 2 落盘");

        let ids: Vec<u64> = store.auctions().await.iter().map(|row| row.id).collect();
        assert_eq!(ids, vec![2, 1], "最新快照必须胜出，不能被旧快照覆盖");
    }
}
