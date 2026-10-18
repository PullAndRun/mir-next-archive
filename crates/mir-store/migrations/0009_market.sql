-- 0009_market.sql — 寄售 / 拍卖（NPC 市场）挂单持久化。
--
-- 对应 Crystal 的 `AuctionInfo`（`Server/MirDatabase/AuctionInfo.cs:5-117`）与全局链表
-- `Envir.Auctions`（`Server/MirEnvir/Envir.cs:131`）。Crystal 把挂单写进 **MirADB 存档的
-- 全局段**，不是角色行：`SaveAccounts` 先写 `NextAuctionID`、`Auctions.Count`，再逐条
-- `auction.Save(writer)`（`Envir.cs:2591-2594`；`AuctionInfo.Save` `:71-87`），读档时
-- 按同样的顺序重建并 `BindItem`/`BindCharacter`（`Envir.cs:3018-3029`）。
-- `AccountInfo.Auctions` 只是运行期反向引用（`docs/store/schema.md:92`），不落盘。
--
-- 本迁移同样做成**全局表**：一行一个挂单，不挂到某个角色行上。卖家/买家只存角色主键 +
-- 名字快照：Crystal 通过 `SellerInfo`/`CurrentBuyerInfo` 引用拿名字（`AuctionInfo.cs:19`、
-- `:96`），本作把名字冗余下来，离线卖家也能显示（见 `crates/mir-server/src/market.rs`）。
--
-- ## 字段对照
--
-- | Crystal | 本迁移 | 说明 |
-- |---|---|---|
-- | `AuctionID`（`AuctionInfo.cs:12`，`ulong`，`++Envir.NextAuctionID`，`:33`） | `id` | 主键；服务器加载时用 `max(id) + 1` 续号 |
-- | `Item`（`:14`，`UserItem`） | `item` | `encode_slots(&[Some(item)])` 字节块（与背包/装备/邮件/行会仓库同一套 `UserItem[]` 编解码） |
-- | `ConsignmentDate`（`:15`，`DateTime`） | `consignment_date` | 上架时刻（`Envir.Now`）；落 `timestamptz` |
-- | `Price`（`:16`，`uint`） | `price` | 寄售是一口价，拍卖是起拍价 |
-- | `CurrentBid`（`:16`，`uint`） | `current_bid` | 当前最高出价；寄售恒为 0，拍卖建单时等于起拍价（`:41-44`） |
-- | `SellerIndex`（`:18`，int，角色主键） | `seller_character_id` | 卖家角色；角色硬删除时级联删挂单 |
-- | `SellerInfo.Name`（`:19`、`:96`） | `seller_name` | 卖家名字快照 |
-- | `CurrentBuyerIndex`（`:18`，int，`-1` = 无） | `current_buyer_character_id` | 当前最高出价者；`NULL` = 没人出价 |
-- | `CurrentBuyerInfo.Name`（`:19`） | `current_buyer_name` | 最高出价者名字快照；`NULL` = 没人出价 |
-- | `Expired`（`:21`） | `expired` | 是否已过期（`Envir.ProcessAuction`，`Envir.cs:2396`） |
-- | `Sold`（`:21`） | `sold` | 是否已成交 |
-- | `ItemType`（`:23`，`MarketItemType`） | `item_type` | 0 Consign / 1 Auction / 2 GameShop（`Shared/Enums.cs`） |
-- | （`Envir.cs:2591` 的 `NextAuctionID`） | —— | **不落列**：加载时按 `max(id) + 1` 续号，见 `crates/mir-server/src/market.rs` |
--
-- ## 与 Crystal 的差异
--
-- * **顺序**：Crystal 按链表顺序存（`AddFirst`，`PlayerObject.cs:8302`，最新的在最前），
--   本表按 `id DESC` 读回，等价于同一个「新 → 旧」顺序（`id` 单调递增）。
-- * **卖家/买家名字**：Crystal 不存，读档后靠 `BindCharacter` 重新绑定；本作直接存名字快照，
--   省掉一次全表角色查找，也避免角色改名后历史挂单显示不一致。
-- * **物品块**：Crystal 是 BinaryReader/Writer 内联 `UserItem`（`AuctionInfo.cs:51`、`:75`），
--   本作沿用项目 MVP 的 `BYTEA` 整块，尚未拆成独立的 `user_items` 表（见 `crates/mir-store/src/lib.rs`）。
-- * **过期清理**：Crystal 只把 `Expired` 置真，等卖家 `MarketGetBack` 取回（`PlayerObject.cs:8668-8793`）；
--   本作在服务器启动时把已过期挂单清理掉（物品/出价按系统邮件退回），详见
--   `crates/mir-server/src/market.rs` 的 `World::purge_expired_auctions`。

CREATE TABLE market_auctions (
    -- Crystal AuctionID（ulong）；服务器层显式写号，不从序列分配。
    id                         BIGINT      PRIMARY KEY,
    -- Crystal AuctionInfo.Item：`encode_slots(&[Some(item)])` 单格字节块。
    item                       BYTEA       NOT NULL,
    -- Crystal AuctionInfo.ConsignmentDate（Envir.Now）。
    consignment_date           TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Crystal AuctionInfo.Price（uint）：寄售一口价 / 拍卖起拍价。
    price                      BIGINT      NOT NULL,
    -- Crystal AuctionInfo.CurrentBid（uint）：当前最高出价。
    current_bid                BIGINT      NOT NULL DEFAULT 0,
    -- Crystal AuctionInfo.SellerIndex：卖家角色主键。
    seller_character_id        BIGINT      NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    -- Crystal SellerInfo.Name：卖家名字快照。
    seller_name                TEXT        NOT NULL DEFAULT '',
    -- Crystal AuctionInfo.CurrentBuyerIndex：当前最高出价者（-1 → NULL）。
    current_buyer_character_id BIGINT      REFERENCES characters (id) ON DELETE SET NULL,
    -- Crystal CurrentBuyerInfo.Name：最高出价者名字快照。
    current_buyer_name         TEXT,
    -- Crystal AuctionInfo.Expired / Sold。
    expired                    BOOLEAN     NOT NULL DEFAULT FALSE,
    sold                       BOOLEAN     NOT NULL DEFAULT FALSE,
    -- Crystal AuctionInfo.ItemType（MarketItemType：0 Consign / 1 Auction / 2 GameShop）。
    item_type                  SMALLINT    NOT NULL DEFAULT 0,
    updated_at                 TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- 上界照抄 C# 类型：Price / CurrentBid 都是 uint（`AuctionInfo.cs:16`）。
    CONSTRAINT market_auctions_price_uint
        CHECK (price >= 0 AND price <= 4294967295),
    CONSTRAINT market_auctions_current_bid_uint
        CHECK (current_bid >= 0 AND current_bid <= 4294967295),
    CONSTRAINT market_auctions_item_type_valid
        CHECK (item_type >= 0 AND item_type <= 2)
);

-- 卖家查自己的挂单（`PlayerObject.MarketSearch` 的 `UserMatch` 分支，`:8393-8399`）。
CREATE INDEX market_auctions_seller_idx
    ON market_auctions (seller_character_id, id DESC);

-- 买家搜索只看「未售出且未过期」的挂单（`PlayerObject.Match`，`:8318-8325`）。
CREATE INDEX market_auctions_active_idx
    ON market_auctions (sold, expired, id DESC);
