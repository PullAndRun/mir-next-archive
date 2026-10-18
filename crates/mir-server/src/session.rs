//! 单个连接的会话状态机。
//!
//! 阶段与 Crystal 的 `GameStage` 对齐：`None`（刚连上，等版本包）→ `Login`
//! （可注册/登录）→ `Select`（已登录，可建/删/选角）→ `Game`（在世界里）。
//! 每个处理函数都自己判定阶段，与 Crystal 一致：阶段不对就静默丢弃。

use std::sync::Arc;

use mir_core::enums::{
    AttackMode, ChatType, HeroBehaviour, IntelligentCreatureType, LightSetting,
    MarketCollectionMode, MarketPanelType, MirClass, MirDirection, MirGridType, PanelType, PetMode,
    WeatherSetting,
};
use mir_core::i18n::LanguageLibrary;
use mir_core::{DateTimeBinary, LanguageCatalog, Point, TextKey};
use mir_protocol::data::{ClientMagic, SelectInfo, UserItem};
use mir_protocol::manual::server::{MapInformation, UserInformation};
use mir_protocol::packets::server::{
    ChangeAMode, ChangePMode as ChangePModeResponse, ClientVersion as ClientVersionResponse,
    Connected, DeleteCharacter as DeleteCharacterResult, DeleteCharacterSuccess, LogOutSuccess,
    Login as LoginResult, LoginBanned, LoginSuccess, MarketFail, MarketSuccess,
    NewAccount as NewAccountResult, NewCharacter as NewCharacterResult, NewCharacterSuccess,
    StartGame,
};
use mir_protocol::{ClientPacket, ServerPacket};
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;

use crate::config::ServerConfig;
use crate::items::{decode_equipment, decode_inventory, new_equipment, new_inventory};
use crate::magic::decode_magics;
use crate::mail::{Mail, MailRecipient, MailState};
use crate::market::GameShopPurchases;
use crate::npc::NpcState;
use crate::quests::decode_quests;
use crate::storage::{decode_storage, new_storage};
use crate::store::{
    Account, AccountStore, Character, GameShopPurchaseRecord, GameShopStockRecord, GameShopUpdate,
    StoreError,
};
use crate::world::{Player, World};

/// 商城购买的失败/成功回包风格：`C.GameshopBuy` 用系统聊天（Crystal 的 `ReceiveChat`），
/// 商城页签发的 `C.MarketBuy` 用 `S.MarketSuccess`/`S.MarketFail`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GameShopReply {
    /// `C.GameshopBuy`：失败用系统聊天（静默类失败不发包）。
    Chat,
    /// 商城页签上的 `C.MarketBuy`：失败 `S.MarketFail`，成功 `S.MarketSuccess`。
    Market,
}

/// 会话阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// 刚建立连接，只接受 `ClientVersion`。
    None,
    /// 版本校验通过，可以注册/登录。
    Login,
    /// 已登录，可以操作角色。
    Select,
    /// 已进入世界。
    Game,
}

/// 会话错误：一律导致连接关闭。
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// 协议错误（开发者可见，统一英文）。
    #[error("protocol error: {0}")]
    Protocol(#[from] mir_protocol::ProtocolError),
    /// 存储错误（开发者可见，统一英文）。
    #[error("store error: {0}")]
    Store(#[from] StoreError),
    /// 被 GM `@kick` 踢下线（错误串是开发者可见的日志文本，用英文）。
    #[error("kicked by GM")]
    Kicked,
}

/// 一个客户端连接。
pub struct Session<S: AccountStore> {
    /// 连接 ID。
    pub id: u64,
    /// 客户端地址。
    pub peer: String,
    config: Arc<ServerConfig>,
    store: Arc<S>,
    world: Arc<Mutex<World>>,
    out: UnboundedSender<Vec<u8>>,
    stage: Stage,
    account: Option<Account>,
    character: Option<Character>,
    object_id: Option<u32>,
    /// 该会话的语言（默认简体中文，可在游戏内用 `/language` 切换）。
    language: LanguageCatalog,
    /// 语言包（服务端文本与地图标题的运行时覆盖层）。
    languages: Arc<LanguageLibrary>,
    /// 包速率限流的窗口起点（每秒重置一次，见 [`Session::rate_allows_packet`]）。
    rate_window_start: std::time::Instant,
    /// 当前窗口内已处理的包数。
    rate_window_count: u32,
    /// 本窗口是否已经因为超速记过 warn（每个窗口只喊一次，避免日志刷屏）。
    rate_warned: bool,
}

impl<S: AccountStore> Session<S> {
    /// 新建会话。
    #[must_use]
    pub fn new(
        id: u64,
        peer: String,
        config: Arc<ServerConfig>,
        store: Arc<S>,
        world: Arc<Mutex<World>>,
        out: UnboundedSender<Vec<u8>>,
        languages: Arc<LanguageLibrary>,
    ) -> Self {
        Self {
            id,
            peer,
            config,
            store,
            world,
            out,
            stage: Stage::None,
            account: None,
            character: None,
            object_id: None,
            // 项目要求默认全中文化；玩家可以在游戏内用 `/language en` 切换。
            language: LanguageCatalog::new(),
            languages,
            rate_window_start: std::time::Instant::now(),
            rate_window_count: 0,
            rate_warned: false,
        }
    }

    /// 当前阶段。
    #[must_use]
    pub fn stage(&self) -> Stage {
        self.stage
    }

    #[allow(clippy::needless_pass_by_value)] // 包需要被 `encode_frame` 消费
    fn send(&self, packet: ServerPacket) {
        // 包按值传进来是为了能直接 `encode_frame()` 消费它；这里不是「没用完」。
        let frame = packet.encode_frame();
        let _ = self.out.send(frame);
    }

    /// 连接建立：Crystal 先发 `Connected`（`MirConnection.cs:97`）。
    pub fn on_connected(&mut self) {
        self.send(ServerPacket::Connected(Connected {}));
    }

    /// 处理一个客户端包。
    #[allow(clippy::needless_pass_by_value)] // 包按值传入，内部按需移动字段
    /// 包速率限制：每秒最多 `config.max_packets_per_second` 个包（0 = 不限制）。
    ///
    /// 用单调时钟做「一秒窗口」，窗口内超限返回 `false`（调用方丢弃该包）。
    /// 每个窗口只记一条 warn，避免被刷屏时日志自己压垮磁盘。
    fn rate_allows_packet(&mut self) -> bool {
        let limit = self.config.max_packets_per_second;
        if limit == 0 {
            return true;
        }
        let now = std::time::Instant::now();
        if now.duration_since(self.rate_window_start) >= std::time::Duration::from_secs(1) {
            self.rate_window_start = now;
            self.rate_window_count = 0;
            self.rate_warned = false;
        }
        self.rate_window_count = self.rate_window_count.saturating_add(1);
        if self.rate_window_count > limit {
            if !self.rate_warned {
                self.rate_warned = true;
                tracing::warn!(
                    session = self.id,
                    peer = %self.peer,
                    limit,
                    "客户端包速率超限：本秒余下的包丢弃（不踢线）"
                );
            }
            return false;
        }
        true
    }

    pub async fn handle(&mut self, packet: ClientPacket) -> Result<(), SessionError> {
        // 包速率限制（抗刷）：超限的包直接丢弃并记一次 warn，**不踢人**
        // （网络抖动时客户端会把积压的包一起发上来，踢掉反而是误伤）。
        if !self.rate_allows_packet() {
            return Ok(());
        }
        // GM `@kick` 的兜底路径：连接建立时登记过通道的会话由读循环直接断开；
        // 没登记的（测试/裸 Session）在这里退出。
        if crate::gm::take_pending_kick(self.id) {
            tracing::info!(session = self.id, "会话被 GM 踢下线");
            return Err(SessionError::Kicked);
        }
        // 市场变更包（上架/买断/提前成交/取回）成功后要整表快照落盘；搜索/翻页只改
        // 会话里的浏览状态，不写库。判断放在 match 之前、落盘放在 match 之后，
        // 七个市场分派分支与 handler 保持队长提交时的原样（见 t4 合同）。
        let market_mutation = matches!(
            &packet,
            ClientPacket::ConsignItem(_)
                | ClientPacket::MarketBuy(_)
                | ClientPacket::MarketSellNow(_)
                | ClientPacket::MarketGetBack(_)
        );
        match packet {
            ClientPacket::ClientVersion(packet) => self.on_client_version(&packet),
            ClientPacket::KeepAlive(packet) => {
                self.send(ServerPacket::KeepAlive(
                    mir_protocol::packets::server::KeepAlive { time: packet.time },
                ));
            }
            ClientPacket::NewAccount(packet) => self.on_new_account(&packet).await?,
            ClientPacket::Login(packet) => self.on_login(&packet).await?,
            ClientPacket::NewCharacter(packet) => self.on_new_character(&packet).await?,
            ClientPacket::DeleteCharacter(packet) => self.on_delete_character(&packet).await?,
            ClientPacket::StartGame(packet) => self.on_start_game(&packet).await?,
            ClientPacket::LogOut(_) => self.on_log_out().await?,
            ClientPacket::Turn(packet) => self.on_turn(packet.direction).await,
            ClientPacket::Walk(packet) => self.on_move(packet.direction, false).await,
            ClientPacket::Run(packet) => self.on_move(packet.direction, true).await,
            ClientPacket::Attack(packet) => self.on_attack(packet.direction).await,
            // 攻击模式：`C.ChangeAMode`（Crystal `MirConnection.cs:1432-1439`）。
            ClientPacket::ChangeAMode(packet) => self.on_change_attack_mode(packet.mode).await,
            // 宠物模式：`C.ChangePMode`（Crystal `MirConnection.cs:1441-1448`）——
            // 写入 `PMode` 并无条件回一条 `S.ChangePMode`。
            ClientPacket::ChangePMode(packet) => self.on_change_pet_mode(packet.mode).await,
            ClientPacket::Chat(packet) => self.on_chat(&packet.message).await,
            ClientPacket::PickUp(_) => self.on_pickup().await,
            ClientPacket::DropItem(packet) => {
                self.on_drop_item(packet.unique_id, packet.count).await;
            }
            ClientPacket::MoveItem(packet) => {
                self.on_move_item(packet.grid, packet.from, packet.to).await;
            }
            // 玩家仓库：`C.StoreItem` / `C.TakeBackItem`（Crystal `MirConnection.cs:346-347`、
            // `:485` → `PlayerObject.cs:5376-5526`，见 [`crate::storage`]）。
            ClientPacket::StoreItem(packet) => {
                self.on_store_item(packet.from, packet.to).await;
            }
            ClientPacket::TakeBackItem(packet) => {
                self.on_take_back_item(packet.from, packet.to).await;
            }
            ClientPacket::CallNPC(packet) => {
                self.on_call_npc(packet.object_id, &packet.key).await;
            }
            ClientPacket::BuyItem(packet) => {
                self.on_buy_item(packet.item_index, packet.count, packet.type_)
                    .await;
            }
            ClientPacket::SellItem(packet) => {
                self.on_sell_item(packet.unique_id, packet.count).await;
            }
            ClientPacket::RepairItem(packet) => {
                self.on_repair_item(packet.unique_id).await;
            }
            ClientPacket::UseItem(packet) => {
                self.on_use_item(packet.unique_id, packet.grid).await;
            }
            ClientPacket::DropGold(packet) => {
                self.on_drop_gold(packet.amount).await;
            }
            ClientPacket::Magic(packet) => {
                self.on_magic(
                    packet.spell,
                    packet.direction,
                    packet.target_id,
                    packet.location,
                )
                .await;
            }
            ClientPacket::MagicKey(packet) => {
                self.on_magic_key(packet.spell, packet.key).await;
            }
            // 任务：`C.AcceptQuest` / `C.FinishQuest` / `C.AbandonQuest` / `C.ShareQuest`
            // （Crystal `PlayerObject.cs:11251`、`:11350`、`:11462`、`:11472`）。
            ClientPacket::AcceptQuest(packet) => {
                self.on_accept_quest(packet.npc_index, packet.quest_index)
                    .await;
            }
            ClientPacket::FinishQuest(packet) => {
                self.on_finish_quest(packet.quest_index, packet.selected_item_index)
                    .await;
            }
            ClientPacket::AbandonQuest(packet) => {
                self.on_abandon_quest(packet.quest_index).await;
            }
            ClientPacket::ShareQuest(packet) => {
                self.on_share_quest(packet.quest_index).await;
            }
            ClientPacket::SpellToggle(packet) => {
                // `SpellToggle`（`HumanObject.cs:8501-`）只对「开关型」技能有意义
                // （Thrusting/HalfMoon/FlamingSword…），本作这些技能都还没实现效果，
                // 收到后只记一条日志，不改任何状态。见 `docs/world/magic.md`。
                tracing::debug!(
                    session = self.id,
                    spell = ?packet.spell,
                    state = ?packet.can_use,
                    "忽略 SpellToggle（开关型技能未实现）"
                );
            }
            // 组队：`C.SwitchGroup` / `C.AddMember` / `C.DelMember` / `C.GroupInvite`
            // （Crystal `MirConnection.cs:1583-1605` → `PlayerObject.cs:9217-9485`，见 [`crate::group`]）。
            ClientPacket::SwitchGroup(packet) => {
                self.on_switch_group(packet.allow_group).await;
            }
            ClientPacket::AddMember(packet) => {
                self.on_add_member(&packet.name).await;
            }
            ClientPacket::DelMember(packet) => {
                self.on_del_member(&packet.name).await;
            }
            ClientPacket::GroupInvite(packet) => {
                self.on_group_invite(packet.accept_invite).await;
            }
            // 行会：`C.EditGuildMember` / `C.EditGuildNotice` / `C.GuildInvite` /
            // `C.RequestGuildInfo` / `C.GuildNameReturn`
            // （Crystal `MirConnection.cs:1256-1270` → `PlayerObject.cs:9840-10060`，见 [`crate::guild`]）。
            ClientPacket::EditGuildMember(packet) => {
                self.on_edit_guild_member(
                    packet.change_type,
                    packet.rank_index,
                    &packet.name,
                    &packet.rank_name,
                )
                .await;
            }
            ClientPacket::EditGuildNotice(packet) => {
                self.on_edit_guild_notice(packet.notice).await;
            }
            ClientPacket::GuildInvite(packet) => {
                self.on_guild_invite(packet.accept_invite).await;
            }
            ClientPacket::RequestGuildInfo(packet) => {
                self.on_request_guild_info(packet.type_).await;
            }
            ClientPacket::GuildNameReturn(packet) => {
                self.on_guild_name_return(&packet.name).await;
            }
            // 行会仓库与行会 buff：Crystal `PlayerObject.cs:10061-10282`、`:10350-10436`，
            // 见 [`crate::guild_storage`] 与 [`crate::guild_buffs`]。
            ClientPacket::GuildStorageGoldChange(packet) => {
                self.on_guild_storage_gold_change(packet.type_, packet.amount)
                    .await;
            }
            ClientPacket::GuildStorageItemChange(packet) => {
                self.on_guild_storage_item_change(packet.type_, packet.from, packet.to)
                    .await;
            }
            ClientPacket::GuildBuffUpdate(packet) => {
                self.on_guild_buff_update(packet.action, packet.id).await;
            }
            // 邮件：`C.SendMail` / `C.ReadMail` / `C.CollectParcel` / `C.DeleteMail` /
            // `C.LockMail` / `C.MailLockedItem` / `C.MailCost`
            // （Crystal `MirConnection.cs:652-672`、`:2003-2052`，见 [`crate::mail`]）。
            ClientPacket::SendMail(packet) => {
                self.on_send_mail(packet).await;
            }
            ClientPacket::ReadMail(packet) => {
                self.on_read_mail(packet.mail_id).await;
            }
            ClientPacket::CollectParcel(packet) => {
                self.on_collect_parcel(packet.mail_id).await;
            }
            ClientPacket::DeleteMail(packet) => {
                self.on_delete_mail(packet.mail_id).await;
            }
            ClientPacket::LockMail(packet) => {
                self.on_lock_mail(packet.mail_id, packet.lock).await;
            }
            ClientPacket::MailLockedItem(packet) => {
                self.on_mail_locked_item(packet.unique_id, packet.locked)
                    .await;
            }
            ClientPacket::MailCost(packet) => {
                self.on_mail_cost(packet.gold, &packet.items_idx, packet.stamped)
                    .await;
            }
            // 玩家交易：`C.ChangeTrade` / `C.TradeRequest` / `C.TradeReply` /
            // `C.DepositTradeItem` / `C.RetrieveTradeItem` / `C.TradeGold` /
            // `C.TradeConfirm` / `C.TradeCancel`
            // （Crystal `MirConnection.cs:367-371`、`:439-440`、`:589-602` →
            // `PlayerObject.cs:10528-10960`，见 [`crate::trade`]）。
            ClientPacket::ChangeTrade(packet) => self.on_change_trade(packet.allow_trade).await,
            ClientPacket::TradeRequest(_) => self.on_trade_request().await,
            ClientPacket::TradeReply(packet) => self.on_trade_reply(packet.accept_invite).await,
            ClientPacket::DepositTradeItem(packet) => {
                self.on_deposit_trade_item(packet.from, packet.to).await;
            }
            ClientPacket::RetrieveTradeItem(packet) => {
                self.on_retrieve_trade_item(packet.from, packet.to).await;
            }
            ClientPacket::TradeGold(packet) => self.on_trade_gold(packet.amount).await,
            ClientPacket::TradeConfirm(packet) => self.on_trade_confirm(packet.locked).await,
            ClientPacket::TradeCancel(_) => self.on_trade_cancel().await,
            // NPC 寄售/拍卖市场：`C.ConsignItem`（70）/ `C.MarketSearch`（71）/
            // `C.MarketRefresh`（72）/ `C.MarketPage`（73）/ `C.MarketBuy`（74）/
            // `C.MarketGetBack`（75）/ `C.MarketSellNow`（76）
            // （Crystal `MirConnection.cs:1685-1700` → `PlayerObject.cs:8217-8802`，
            // 见 [`crate::market`] 与 `docs/world/market.md`）。
            ClientPacket::ConsignItem(packet) => {
                self.on_consign_item(packet.unique_id, packet.price, packet.type_)
                    .await;
            }
            ClientPacket::MarketSearch(packet) => {
                self.on_market_search(
                    &packet.match_text,
                    packet.item_type,
                    packet.usermode,
                    packet.min_shape,
                    packet.max_shape,
                    packet.market_type,
                )
                .await;
            }
            ClientPacket::MarketRefresh(_) => self.on_market_refresh().await,
            ClientPacket::MarketPage(packet) => self.on_market_page(packet.page).await,
            ClientPacket::MarketBuy(packet) => {
                self.on_market_buy(packet.auction_id, packet.bid_price)
                    .await;
            }
            ClientPacket::MarketSellNow(packet) => {
                self.on_market_sell_now(packet.auction_id).await;
            }
            ClientPacket::MarketGetBack(packet) => {
                self.on_market_get_back(packet.mode, packet.auction_id)
                    .await;
            }
            // 商城购买：`C.GameshopBuy{GIndex, Quantity, PType}`（包 133，Crystal
            // `PlayerObject.GameshopBuy`，`PlayerObject.cs:13725-13911`，见 [`crate::market`]）。
            ClientPacket::GameshopBuy(packet) => {
                self.on_gameshop_buy(
                    packet.g_index,
                    packet.quantity,
                    packet.p_type,
                    GameShopReply::Chat,
                )
                .await;
            }
            // 开门：`C.Opendoor`（Crystal `PlayerObject.Opendoor`，`PlayerObject.cs:7818-7826`）。
            // 世界层负责发包（本人 `S.Opendoor` + 同图视野广播）与 5 秒后自动关门。
            ClientPacket::Opendoor(packet) => self.on_opendoor(packet.door_index).await,
            // 行会战（`C.GuildWarReturn`，`PlayerObject.cs:10283-10327`）与行会领地
            // （`GuildTerritoryPage`/`PurchaseGuildTerritory`，`:10438-10525`）仍未实现：
            // 前者需要 `GuildObject.WarringGuilds`，后者需要 `Envir.GTMapList`（另一套数据表），
            // 都不属于本轮「攻城」范围。见 `docs/world/conquest.md` 的「未做」。
            ClientPacket::GuildWarReturn(packet) => {
                tracing::debug!(
                    session = self.id,
                    name = %packet.name,
                    "忽略 C.GuildWarReturn（行会战未实现；攻城走 NPC 脚本）"
                );
            }
            ClientPacket::PurchaseGuildTerritory(_) | ClientPacket::GuildTerritoryPage(_) => {
                tracing::debug!(session = self.id, "忽略行会领地租赁包（未实现）");
            }
            ClientPacket::Disconnect(_) => {
                self.leave_world().await;
            }
            _ => {
                tracing::debug!(session = self.id, "忽略未实现的客户端包");
            }
        }
        if market_mutation {
            self.flush_auction_writes().await;
        }
        Ok(())
    }

    fn on_client_version(&mut self, packet: &mir_protocol::packets::client::ClientVersion) {
        if self.stage != Stage::None {
            return;
        }
        if self.config.check_version
            && !self
                .config
                .version_hashes
                .iter()
                .any(|hash| hash == &packet.version_hash)
        {
            // Crystal：先回 Result=0，再软断开（`MirConnection.cs:857-862`）。
            self.send(ServerPacket::ClientVersion(ClientVersionResponse {
                result: 0,
            }));
            self.stage = Stage::None;
            tracing::info!(session = self.id, peer = %self.peer, "客户端版本不匹配，拒绝连接");
            return;
        }
        self.send(ServerPacket::ClientVersion(ClientVersionResponse {
            result: 1,
        }));
        self.stage = Stage::Login;
        tracing::info!(session = self.id, peer = %self.peer, "客户端版本校验通过");
    }

    async fn on_new_account(
        &mut self,
        packet: &mir_protocol::packets::client::NewAccount,
    ) -> Result<(), SessionError> {
        if self.stage != Stage::Login {
            return Ok(());
        }
        let result = if !self.config.allow_new_account {
            0
        } else if !is_valid_account_id(&packet.account_id) {
            1
        } else if !is_valid_password(&packet.password) {
            2
        } else if !packet.e_mail_address.is_empty()
            && (packet.e_mail_address.len() > 50 || !packet.e_mail_address.contains('@'))
        {
            3
        } else {
            match self
                .store
                .create_account(&packet.account_id, &packet.password, &packet.e_mail_address)
                .await
            {
                Ok(_) => 8,
                Err(StoreError::AccountExists(_)) => 7,
                Err(error) => return Err(error.into()),
            }
        };
        self.send(ServerPacket::NewAccount(NewAccountResult { result }));
        Ok(())
    }

    async fn on_login(
        &mut self,
        packet: &mir_protocol::packets::client::Login,
    ) -> Result<(), SessionError> {
        if self.stage != Stage::Login {
            return Ok(());
        }
        if !self.config.allow_login {
            self.send(ServerPacket::Login(LoginResult { result: 0 }));
            return Ok(());
        }
        if !is_valid_account_id(&packet.account_id) {
            self.send(ServerPacket::Login(LoginResult { result: 1 }));
            return Ok(());
        }
        if !is_valid_password(&packet.password) {
            self.send(ServerPacket::Login(LoginResult { result: 2 }));
            return Ok(());
        }
        let Some(account) = self.store.find_account(&packet.account_id).await? else {
            self.send(ServerPacket::Login(LoginResult { result: 3 }));
            return Ok(());
        };
        if account.banned {
            let expired = account
                .ban_expiry
                .is_some_and(|expiry| expiry.to_unix_seconds() > now_unix());
            if expired {
                self.send(ServerPacket::LoginBanned(LoginBanned {
                    reason: account.ban_reason.clone(),
                    expiry_date: account.ban_expiry.unwrap_or_default(),
                }));
                return Ok(());
            }
        }
        if !account.verify_password(&packet.password) {
            self.send(ServerPacket::Login(LoginResult { result: 4 }));
            tracing::info!(session = self.id, account = %account.username, "口令不匹配");
            return Ok(());
        }
        if account.require_password_change {
            self.send(ServerPacket::Login(LoginResult { result: 5 }));
            return Ok(());
        }

        let characters = self.store.list_characters(account.id).await?;
        tracing::info!(
            session = self.id,
            account = %account.username,
            characters = characters.len(),
            "登录成功"
        );
        // 语言设置随账号持久化：老账号默认中文，玩家上次选的会被带回来。
        if let Some(language) = mir_core::Language::from_code(&account.language) {
            self.language.set(language);
        }
        self.account = Some(account);
        self.stage = Stage::Select;
        self.send(ServerPacket::LoginSuccess(LoginSuccess {
            characters: characters.iter().map(select_info).collect(),
        }));
        Ok(())
    }

    async fn on_new_character(
        &mut self,
        packet: &mir_protocol::packets::client::NewCharacter,
    ) -> Result<(), SessionError> {
        if self.stage != Stage::Select {
            return Ok(());
        }
        let Some(account) = self.account.clone() else {
            return Ok(());
        };
        if !self.config.allow_new_character {
            self.send(ServerPacket::NewCharacter(NewCharacterResult { result: 0 }));
            return Ok(());
        }
        if !is_valid_character_name(&packet.name) {
            self.send(ServerPacket::NewCharacter(NewCharacterResult { result: 1 }));
            return Ok(());
        }
        let existing = self.store.list_characters(account.id).await?;
        if existing.len() >= self.config.max_character_count {
            self.send(ServerPacket::NewCharacter(NewCharacterResult { result: 4 }));
            return Ok(());
        }
        let class = match packet.class {
            MirClass::Warrior | MirClass::Wizard | MirClass::Taoist => packet.class,
            // Crystal 允许刺客/弓箭手，但经典版本默认关闭；这里按原样接受。
            other => other,
        };
        let character = self
            .store
            .create_character(
                account.id,
                &packet.name,
                class,
                packet.gender,
                &self.config.start_map,
                self.config.start_point,
            )
            .await;
        match character {
            Ok(character) => {
                tracing::info!(session = self.id, name = %character.name, "角色创建成功");
                self.send(ServerPacket::NewCharacterSuccess(NewCharacterSuccess {
                    char_info: select_info(&character),
                }));
            }
            Err(StoreError::CharacterExists(_)) => {
                self.send(ServerPacket::NewCharacter(NewCharacterResult { result: 2 }));
            }
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    async fn on_delete_character(
        &mut self,
        packet: &mir_protocol::packets::client::DeleteCharacter,
    ) -> Result<(), SessionError> {
        if self.stage != Stage::Select {
            return Ok(());
        }
        let Some(account) = self.account.clone() else {
            return Ok(());
        };
        if !self.config.allow_delete_character {
            self.send(ServerPacket::DeleteCharacter(DeleteCharacterResult {
                result: 0,
            }));
            return Ok(());
        }
        let deleted = self
            .store
            .delete_character(account.id, i64::from(packet.character_index))
            .await?;
        if deleted {
            self.send(ServerPacket::DeleteCharacterSuccess(
                DeleteCharacterSuccess {
                    character_index: packet.character_index,
                },
            ));
        } else {
            self.send(ServerPacket::DeleteCharacter(DeleteCharacterResult {
                result: 1,
            }));
        }
        Ok(())
    }

    async fn on_start_game(
        &mut self,
        packet: &mir_protocol::packets::client::StartGame,
    ) -> Result<(), SessionError> {
        if self.stage != Stage::Select {
            return Ok(());
        }
        let Some(account) = self.account.clone() else {
            self.send(ServerPacket::StartGame(StartGame {
                result: 1,
                resolution: 0,
            }));
            return Ok(());
        };
        if !self.config.allow_start_game && !account.admin {
            self.send(ServerPacket::StartGame(StartGame {
                result: 0,
                resolution: 0,
            }));
            return Ok(());
        }
        let Some(mut character) = self
            .store
            .find_character(account.id, i64::from(packet.character_index))
            .await?
        else {
            self.send(ServerPacket::StartGame(StartGame {
                result: 2,
                resolution: 0,
            }));
            return Ok(());
        };

        // 首次进入世界时补全角色属性（Crystal 在建角时把等级留 0）。
        if character.level == 0 {
            character.level = 1;
            character.hair = u8::try_from(character.id % 9).unwrap_or(0);
            character.hp = 15;
            character.mp = 15;
            self.store.save_character(character.clone()).await?;
        }

        // 每日任务惰性重置：必须在**解码 `character.quests` 之前**作用在角色记录上，
        // 这样重置结果随本次登录的 `Player` 生效。
        // Crystal 是服务器 0 点对全角色 `ClearDailyQuests`（`Envir.cs:4856-4866`）；
        // 本作按角色 24h 惰性补，差异见 `docs/world/quests.md`。
        //
        // **立即落盘**（刑部第 5 轮 F1）：只改内存的话，崩溃/被踢会让这次重置丢失，
        // 且 `characters.last_daily_reset_at` 会停在旧值直到下线；所以命中重置就在
        // `world.enter` 之前写一次角色行。写失败只 warn、**不阻断登录**：
        // 重置是幂等的（下次登录会重做），把玩家挡在门外更糟。
        let reset_now = {
            let world = self.world.lock().await;
            world.wall_now()
        };
        let reset_done = {
            let world = self.world.lock().await;
            world.apply_daily_reset(&mut character, reset_now)
        };
        if reset_done {
            match self.store.save_character(character.clone()).await {
                Ok(()) => tracing::debug!(
                    session = self.id,
                    character = character.id,
                    "登录时补做每日任务重置并立即落盘"
                ),
                Err(error) => tracing::warn!(
                    session = self.id,
                    character = character.id,
                    %error,
                    "每日重置落盘失败，本次仍继续登录（重置幂等，下次登录会重做）"
                ),
            }
        }

        // 背包/装备：Crystal 存的是 `UserItem[]` 整块（`CharacterInfo.cs:431-447`），
        // 空块（新角色）解成空背包/空装备栏。解析失败时按「空」处理并记日志，不拒绝登录。
        let inventory = match decode_inventory(&character.inventory) {
            Ok(inventory) => inventory,
            Err(error) => {
                tracing::warn!(session = self.id, %error, "背包字节块解析失败，按空背包处理");
                new_inventory()
            }
        };
        let equipment = match decode_equipment(&character.equipment) {
            Ok(equipment) => equipment,
            Err(error) => {
                tracing::warn!(session = self.id, %error, "装备字节块解析失败，按空装备处理");
                new_equipment()
            }
        };
        // 技能块：Crystal 的 `CharacterInfo.Magics`（`CharacterInfo.cs:226-234`）。
        let magics = match decode_magics(&character.magics) {
            Ok(magics) => magics,
            Err(error) => {
                tracing::warn!(session = self.id, %error, "技能字节块解析失败，按未学技能处理");
                Vec::new()
            }
        };
        // 任务块：Crystal 的 `CharacterInfo.CurrentQuests` + `CompletedQuests`
        // （`CharacterInfo.cs:262-277`、`:307-313`），格式见 `crate::quests::encode_quests`。
        let quest_save = match decode_quests(&character.quests) {
            Ok(save) => save,
            Err(error) => {
                tracing::warn!(session = self.id, %error, "任务字节块解析失败，按没有任务处理");
                crate::quests::QuestSave::default()
            }
        };
        // 仓库：Crystal `AccountInfo.Storage` 的 `UserItem[80]`（`AccountInfo.cs:69`），
        // 本作存在角色行的 `characters.storage` 上；空块（新角色）解成 80 格空仓库。
        let storage = match decode_storage(&character.storage) {
            Ok(storage) => storage,
            Err(error) => {
                tracing::warn!(session = self.id, %error, "仓库字节块解析失败，按空仓库处理");
                new_storage()
            }
        };
        // 邮箱：Crystal 在登录流程末尾调 `GetMail()`（`PlayerObject.cs:1190`）。
        // 本作从 `mail` 表装载，并按扩展的 30 天上限过滤+删除过期邮件（见 `crate::mail`）。
        let mailbox = self.load_mailbox(character.id).await;
        // 任务物品栏：Crystal `CharacterInfo.QuestInventory`（40 格，`CharacterInfo.cs:75`），
        // 本作存在角色行的 `characters.quest_inventory`（迁移 0012）；空块解成空栏。
        let quest_inventory =
            match crate::quest_inventory::QuestInventory::decode(&character.quest_inventory) {
                Ok(inventory) => inventory,
                Err(error) => {
                    tracing::warn!(
                        session = self.id,
                        %error,
                        "任务物品栏字节块解析失败，按空栏处理"
                    );
                    crate::quest_inventory::QuestInventory::new()
                }
            };

        let mut world = self.world.lock().await;
        // 读档里出现过的 UUID 要计入发生器，否则新掉的物品可能撞号
        // （Crystal 用账号库里的 `NextUserItemID` 做同一件事，`Envir.cs:127,2579`）。
        world.observe_item_uids(inventory.iter().chain(equipment.iter()).flatten());
        let object_id = world.allocate_object_id();
        let position = Point::new(character.x, character.y);
        // 行会：按角色 ID 反查成员表（`Character.guild_index` 只是反向指针，
        // 行会的成员表才是事实来源）。`World::enter` 会再校正一次。
        let guild_ref = world.guild_ref_of_character(character.id);
        world.enter(Player {
            object_id,
            session_id: self.id,
            character_id: character.id,
            name: character.name.clone(),
            map: character.map.clone(),
            position,
            direction: character.direction,
            level: character.level,
            // 经验与升级需求：`max_experience` 由 `World::enter` 按经验表补上
            // （Crystal `PlayerObject.RefreshMaxExperience`，`PlayerObject.cs:9627-9630`）。
            experience: character.experience,
            max_experience: 0,
            class: character.class,
            gender: character.gender,
            hair: character.hair,
            hp: character.hp,
            mp: character.mp,
            // 战斗属性按等级/职业推出基础值（Crystal `BaseStats` 表）；玩家带装备时
            // `World::enter` 会再叠加一次装备加成（`HumanObject.cs:1730-1769`）。
            stats: crate::combat::player_base_stats(character.level, character.class),
            // 动作计时由 `World::enter` 按服务器时钟初始化。
            action_time: 0,
            attack_time: 0,
            struck_time: 0,
            // Crystal 的 `RegenTime` 初值是 0，第一次 `Process` 就进入回复分支
            // （`HumanObject.cs:145-151`）；`death_time`/`revive_time` 同样从 0 开始。
            regen_time: 0,
            death_time: 0,
            revive_time: 0,
            cell_time: 0,
            step_counter: 0,
            run_counter: 0,
            run_time: 0,
            dead: false,
            // 攻击模式/PK 状态：Crystal 的 `AMode` 与 `PKPoints` 都存在 `CharacterInfo`
            // （`CharacterInfo.cs:66`、`:249`）。本作还没有把这两项写进角色库，
            // 登录时从默认值开始（偏差见 `docs/world/pk.md`）。
            attack_mode: AttackMode::Peace,
            pk_points: 0,
            brown_time: 0,
            pk_point_time: 0,
            last_hitter: None,
            last_hit_time: 0,
            name_colour: crate::world::NAME_COLOUR_WHITE,
            visible_monsters: std::collections::BTreeSet::new(),
            visible_npcs: std::collections::BTreeSet::new(),
            movement_time: 0,
            inventory: inventory.clone(),
            equipment: equipment.clone(),
            storage: storage.clone(),
            magics: magics.clone(),
            magic: crate::magic::MagicState::default(),
            gold: character.gold,
            credit: character.credit,
            visible_items: std::collections::BTreeSet::new(),
            quests: quest_save.quests.clone(),
            completed_quests: quest_save.completed_quests.clone(),
            // 邮箱：登录时从 `mail` 表装入（Crystal `GetMail()`，`PlayerObject.cs:1190`）。
            mail: MailState {
                mails: mailbox,
                next_send_time: 0,
                new_mail: false,
            },
            // buff 不持久化（Crystal 的 `CharacterInfo` 存档里没有 buff 块），登录即空。
            buffs: Vec::new(),
            pot_health_amount: 0,
            pot_mana_amount: 0,
            pot_time: 0,
            unlock_curse: false,
            npc: NpcState::default(),
            // 组队是临时状态（Crystal 的 `CharacterInfo` 没有队伍块）：进世界时不在任何队伍里，
            // 也没有待接受的邀请。`AllowGroup` 在 Crystal 里随存档持久化
            // （`CharacterInfo.cs:62`、`:249`、`:476`），本作不改 `mir-store`，所以默认允许组队
            // （偏差说明见 `docs/world/group.md`）。
            group_id: None,
            group_invitation: None,
            group_invite_time: 0,
            allow_group: true,
            // 行会：由上面的 `guild_ref` 传入（`World::enter` 会校正并在行会里标记上线）。
            guild: guild_ref,
            trade: crate::trade::TradeState::default(),
            trade_invitation: None,
            next_trade_time: 0,
            allow_trade: true,
            sender: self.out.clone(),
        });
        // 玩家语言：账号语言随登录写进世界层映射，World 的市场/交易/邮件/NPC 提示据此渲染。
        world.set_player_language(object_id, self.language.language());
        // 任务物品栏：装进世界侧表，下线时由 `leave_world` 取走存盘。
        world.set_quest_inventory(object_id, quest_inventory.clone());
        // 宠物命令模式（Crystal `CharacterInfo.PMode`，列 `characters.pet_mode`；迁移 0001）。
        // 越界/老存档按 `Both`（Crystal 默认）。
        world.set_pet_mode(
            object_id,
            mir_core::enums::PetMode::from_value(u8::try_from(character.pet_mode).unwrap_or(0))
                .unwrap_or(mir_core::enums::PetMode::Both),
        );
        // 进图资料：Crystal `PlayerObject.CheckMapInfo`（`PlayerObject.cs:1009-1056`）。
        // 必须在释放世界锁之前取，Npc 列表与传送点都来自世界内的实时状态。
        let map_index = world
            .game_data()
            .and_then(|data| data.map_info(&character.map))
            .map(|info| info.index);
        let entry_title = self
            .languages
            .map_title(self.language.language(), &character.map);
        let entry_info = world.map_entry_info(&character.map, &entry_title);
        // 已学技能（Crystal `UserInformation.Magics`，`ServerPackets.cs:640-680` 里逐条
        // `magic.CreateClientMagic()`）。同样必须在释放世界锁之前取。
        let client_magics = world.client_magics(object_id);
        // 收集类任务目标按读档后的背包重算一次（**不发包**：此时还没进世界）。
        world.sync_quest_items(object_id);
        // 组队开关的当前值：Crystal 在登录流程里会补一条 `S.SwitchGroup { AllowGroup }`
        // （`PlayerObject.cs:1219`），客户端组队窗口据此点亮开关。
        let allow_group = world.allow_group_of(object_id).unwrap_or(true);
        // 任务下发（Crystal `GetQuestInfo` / `GetCompletedQuests` / 逐条 `ChangeQuest{Add}`，
        // `PlayerObject.cs:1185-1212`）。这些包要排在 `UserInformation` 之后，
        // 所以先收集，等会话自己的包发完再发。
        let quest_packets = world.quest_login_packets(object_id);
        drop(world);

        self.object_id = Some(object_id);
        self.stage = Stage::Game;

        // Crystal 的进世界序列：StartGame{Result=4} → NewItemInfo → MapInformation → UserInformation。
        self.send(ServerPacket::StartGame(StartGame {
            result: 4,
            resolution: self.config.allowed_resolution,
        }));
        self.send(ServerPacket::MapInformation(MapInformation {
            map_index: map_index.unwrap_or(0),
            file_name: character.map.clone(),
            title: entry_title.clone(),
            mini_map: 0,
            big_map: 0,
            lights: LightSetting::Day,
            lightning: false,
            fire: false,
            map_dark_light: 0,
            music: 0,
            weather_particles: WeatherSetting::None,
        }));
        // 进图详情（NPC 列表 + 传送点）：Crystal `PlayerObject.CheckMapInfo` 发的
        // `S.NewMapInfo`（`PlayerObject.cs:1054`）。`MapInformation` 本身没有这两个字段
        // （`Shared/ServerPackets.cs:469-517`），见 [`crate::npc`] 模块文档。
        if let Some(info) = entry_info {
            self.send(ServerPacket::NewMapInfo(
                mir_protocol::packets::server::NewMapInfo {
                    map_index: map_index.unwrap_or(0),
                    info,
                },
            ));
        }
        self.send(ServerPacket::UserInformation(user_information(
            object_id,
            &character,
            position,
            &inventory,
            &equipment,
            &client_magics,
            quest_inventory.slots(),
        )));
        // `Enqueue(new S.ChangeAMode { Mode = AMode });`（`PlayerObject.cs:1217`）：
        // 排在 `TimeOfDay` 之后、`SwitchGroup` 之前；本作没有 `TimeOfDay`，所以直接紧跟
        // `UserInformation`。
        let attack_mode = {
            let world = self.world.lock().await;
            world.attack_mode_of(object_id)
        }
        .unwrap_or_default();
        self.send(ServerPacket::ChangeAMode(ChangeAMode { mode: attack_mode }));
        // `Enqueue(new S.SwitchGroup { AllowGroup = AllowGroup });`（`PlayerObject.cs:1219`）。
        self.send(ServerPacket::SwitchGroup(
            mir_protocol::packets::server::SwitchGroup { allow_group },
        ));
        // 行会 buff 的可选列表：Crystal 在 `SwitchGroup` 之后、`DefaultNPC` 之后发
        // `S.GuildBuffList { GuildBuffs = Settings.Guild_BuffList }`（`PlayerObject.cs:1223-1224`），
        // 而且**不看有没有行会**。本作的 `DefaultNPC` 还没实现，所以紧跟 `SwitchGroup`。
        {
            let mut world = self.world.lock().await;
            world.send_guild_buff_info(object_id);
        }
        // 任务表（`PlayerObject.cs:1185-1212`）：全套 `S.NewQuestInfo` → `S.CompleteQuest` →
        // 每条已接任务的 `S.ChangeQuest{Add}`。
        for packet in quest_packets {
            self.send(packet);
        }
        // 邮箱：Crystal `GetMail()` 排在 `GetCompletedQuests()` 之后（`PlayerObject.cs:1190`），
        // 本作同样放在任务包后面、行会状态包之前。
        let mail_packet = {
            let world = self.world.lock().await;
            world.mail_list_packet(object_id)
        };
        if let Some(packet) = mail_packet {
            self.send(packet);
        }
        // 行会：`MyGuild.PlayerLogged(this, true)` → `S.GuildMemberChange{Status=1}` 给所有
        // 在线成员，再给自己一条 `S.GuildStatus`（`PlayerObject.cs:1309-1316`）。
        // 必须排在 `S.UserInformation` 之后，所以放在这里补发。
        {
            let mut world = self.world.lock().await;
            world.guild_player_online(object_id);
        }
        let welcome = self.language.text_with(&self.languages, TextKey::Welcome);
        self.send_system_chat(&welcome);
        tracing::info!(
            session = self.id,
            character = %character.name,
            object_id,
            "进入世界"
        );
        self.character = Some(character);
        // 让 `@kick <账号|角色>` 能在会话登记表里按名字反查（见 `crate::gm`）。
        if let Some(account) = &self.account {
            let character_name = self
                .character
                .as_ref()
                .map_or_else(String::new, |character| character.name.clone());
            crate::gm::set_session_identity(self.id, &account.username, &character_name);
        }
        Ok(())
    }

    async fn on_log_out(&mut self) -> Result<(), SessionError> {
        if self.stage != Stage::Select && self.stage != Stage::Game {
            return Ok(());
        }
        self.leave_world().await;
        if let Some(character) = self.character.clone() {
            self.store.save_character(character).await?;
        }
        self.character = None;
        self.stage = Stage::Select;
        let characters = if let Some(account) = &self.account {
            self.store.list_characters(account.id).await?
        } else {
            Vec::new()
        };
        self.send(ServerPacket::LogOutSuccess(LogOutSuccess {
            characters: characters.iter().map(select_info).collect(),
        }));
        Ok(())
    }

    async fn on_turn(&mut self, direction: MirDirection) {
        let Some(object_id) = self.object_id else {
            return;
        };
        // Crystal 的做法是「早到的动作包进 retryList，下一个处理周期再试」；
        // 这里用「等到 ActionTime 再重试一次」等价实现，避免丢包导致角色卡住。
        for attempt in 0..2 {
            let error = {
                let mut world = self.world.lock().await;
                match world.turn(object_id, direction) {
                    Ok(()) => return,
                    Err(error) => error,
                }
            };
            if attempt == 0 && self.wait_for_action(&error).await {
                continue;
            }
            tracing::debug!(session = self.id, %error, "转身失败");
            return;
        }
    }

    async fn on_move(&mut self, direction: MirDirection, running: bool) {
        let Some(object_id) = self.object_id else {
            return;
        };
        for attempt in 0..2 {
            let error = {
                let mut world = self.world.lock().await;
                let result = if running {
                    world.run(object_id, direction)
                } else {
                    world.walk(object_id, direction)
                };
                match result {
                    Ok(position) => {
                        // 传送点可能在走/跑之后把玩家切到别的图，坐标回写时连地图名一起同步
                        // （Crystal 的 `CurrentMap` 是对象字段，存档里存 `CurrentMapIndex` 对应的地图）。
                        let map = world.player(object_id).map(|player| player.map.clone());
                        if let Some(character) = &mut self.character {
                            character.x = position.x;
                            character.y = position.y;
                            character.direction = direction;
                            if let Some(map) = map {
                                character.map = map;
                            }
                        }
                        return;
                    }
                    Err(error) => error,
                }
            };
            if attempt == 0 && self.wait_for_action(&error).await {
                continue;
            }
            tracing::debug!(session = self.id, %error, "移动被拒绝");
            return;
        }
    }

    /// 物理攻击（`C.Attack`，Crystal `ClientPackets.Attack`）。
    ///
    /// Crystal 的连接层会在 `ActionTime > now || AttackTime > now` 时把包放进 `_retryList`
    /// 下个周期重试（`Server/MirNetwork/MirConnection.cs:1454-1480`）；
    /// 这里沿用与移动相同的「等到 deadline 再重试一次」策略。
    async fn on_attack(&mut self, direction: MirDirection) {
        let Some(object_id) = self.object_id else {
            return;
        };
        for attempt in 0..2 {
            let busy_until = {
                let mut world = self.world.lock().await;
                match world.attack(object_id, direction) {
                    Ok(report) => {
                        if let Some(report) = report {
                            tracing::debug!(
                                session = self.id,
                                target = report.target,
                                damage = report.damage,
                                "发起攻击"
                            );
                        }
                        return;
                    }
                    Err(crate::world::CombatError::Busy { until }) => until,
                    Err(error) => {
                        tracing::debug!(session = self.id, %error, "攻击被拒绝");
                        return;
                    }
                }
            };
            if attempt == 0 && self.wait_until(busy_until).await {
                continue;
            }
            return;
        }
    }

    /// 切换攻击模式（`C.ChangeAMode`）。
    ///
    /// Crystal `MirConnection.ChangeAMode`（`Server/MirNetwork/MirConnection.cs:1432-1439`）：
    ///
    /// ```csharp
    /// if (Stage != GameStage.Game) return;
    /// Player.AMode = p.Mode;
    /// Enqueue(new S.ChangeAMode { Mode = Player.AMode });
    /// ```
    ///
    /// 没有额外校验（未知值在协议解码时已经落回 `AttackMode::Peace`），也不广播。
    async fn on_change_attack_mode(&mut self, mode: AttackMode) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        if world.change_attack_mode(object_id, mode) {
            tracing::debug!(session = self.id, ?mode, "切换攻击模式");
        }
    }

    /// 切换宠物模式（`C.ChangePMode`）。
    ///
    /// Crystal `MirConnection.ChangePMode`（`Server/MirNetwork/MirConnection.cs:1441-1448`）：
    /// `Player.PMode = p.Mode; Enqueue(new S.ChangePMode { Mode = Player.PMode });`
    /// —— 写状态 + **无条件**回包（回包让客户端把下拉框同步到服务端值）。
    async fn on_change_pet_mode(&mut self, mode: PetMode) {
        let Some(object_id) = self.object_id else {
            return;
        };
        {
            let mut world = self.world.lock().await;
            world.set_pet_mode(object_id, mode);
        }
        tracing::debug!(session = self.id, ?mode, "切换宠物模式");
        self.send(ServerPacket::ChangePMode(ChangePModeResponse { mode }));
    }

    /// 拾取脚下物品（`C.PickUp`，Crystal `PlayerObject.PickUp`，`PlayerObject.cs:7483-7537`）。
    async fn on_pickup(&mut self) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.pickup(object_id) {
            Ok(report) => {
                tracing::debug!(
                    session = self.id,
                    ground = report.ground,
                    item = report.item.as_ref().map(|item| item.item_index),
                    gold = report.gold,
                    "拾取成功"
                );
            }
            Err(error) => tracing::debug!(session = self.id, %error, "拾取被拒绝"),
        }
    }

    /// 丢弃背包物品（`C.DropItem`，Crystal `PlayerObject.DropItem`，`PlayerObject.cs:7359-7472`）。
    async fn on_drop_item(&mut self, unique_id: u64, count: u16) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.drop_item(object_id, unique_id, count) {
            Ok(ground) => {
                tracing::debug!(session = self.id, ground, "丢弃成功");
            }
            Err(error) => tracing::debug!(session = self.id, %error, "丢弃被拒绝"),
        }
    }

    /// 背包内移动 / 装备穿戴（`C.MoveItem`）。
    ///
    /// Crystal 把「背包内交换」和「穿/脱装备」拆成三个包（`C.MoveItem` / `C.EquipItem` /
    /// `C.RemoveItem`，`MirConnection.cs:1217-1307`）；本作统一走 `C.MoveItem`，
    /// 由 [`World::move_item`] 按 `grid` 分派，见那里的说明。
    async fn on_move_item(&mut self, grid: MirGridType, from: i32, to: i32) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.move_item(object_id, grid, from, to) {
            Ok(()) => tracing::debug!(session = self.id, ?grid, from, to, "物品移动成功"),
            Err(error) => tracing::debug!(session = self.id, %error, "物品移动被拒绝"),
        }
    }

    /// 点 NPC / 点对话按钮（`C.CallNPC`，Crystal `PlayerObject.CallNPC`，`PlayerObject.cs:7900-7929`）。
    ///
    /// 脚本执行、`S.NPCResponse` 与商店面板的发送都在 [`World::call_npc`] 里；
    /// 这里只负责把世界锁拿住、把结果记到日志。
    async fn on_call_npc(&mut self, npc_object_id: u32, key: &str) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.call_npc(object_id, npc_object_id, key) {
            Ok(reply) => tracing::debug!(
                session = self.id,
                npc = npc_object_id,
                page = %reply.page_key,
                lines = reply.page.len(),
                panels = ?reply.panels,
                closed = reply.closed,
                unsupported = reply.unsupported_checks.len() + reply.unsupported_actions.len(),
                "NPC 对话"
            ),
            Err(error) => tracing::debug!(session = self.id, %error, "NPC 对话被拒绝"),
        }
    }

    /// 买（`C.BuyItem`，Crystal `PlayerObject.BuyItem`，`PlayerObject.cs:7944-7969`）。
    async fn on_buy_item(&mut self, item_index: u64, count: u16, panel: PanelType) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.buy_item(object_id, item_index, count, panel) {
            Ok(report) => tracing::debug!(
                session = self.id,
                item = %report.item.item_index,
                count = report.item.count,
                cost = report.cost,
                "买到物品"
            ),
            Err(error) => tracing::debug!(session = self.id, %error, "买东西被拒绝"),
        }
    }

    /// 卖（`C.SellItem`，Crystal `PlayerObject.SellItem`，`:7988-8091`）。
    async fn on_sell_item(&mut self, unique_id: u64, count: u16) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.sell_item(object_id, unique_id, count) {
            Ok(report) => tracing::debug!(
                session = self.id,
                item = %report.item_name,
                count = report.count,
                gold = report.gold,
                "卖出物品"
            ),
            Err(error) => tracing::debug!(session = self.id, %error, "卖东西被拒绝"),
        }
    }

    /// 修（`C.RepairItem`，Crystal `PlayerObject.RepairItem`，`:8092-8160`）。
    async fn on_repair_item(&mut self, unique_id: u64) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.repair_item(object_id, unique_id) {
            Ok(report) => tracing::debug!(
                session = self.id,
                unique_id,
                cost = report.cost,
                max_dura = report.max_dura,
                "修理完成"
            ),
            Err(error) => tracing::debug!(session = self.id, %error, "修理被拒绝"),
        }
    }

    /// 用物品（`C.UseItem`，Crystal `PlayerObject.UseItem`，`PlayerObject.cs:5799-6338`）。
    ///
    /// 药水/卷轴/技能书都在 [`World::use_item`] 里分派：药水走进「药水池 + 增益」，
    /// 复活卷轴在死亡状态下才有意义，书学会/升级技能。其余物品类型会收到
    /// `S.UseItem{Success=false}` 且不被消耗。
    async fn on_use_item(&mut self, unique_id: u64, grid: MirGridType) {
        let Some(object_id) = self.object_id else {
            return;
        };
        if grid != MirGridType::Inventory {
            tracing::debug!(session = self.id, ?grid, "忽略非背包网格的 UseItem");
            return;
        }
        let mut world = self.world.lock().await;
        match world.use_item(object_id, unique_id) {
            Ok(report) => tracing::debug!(
                session = self.id,
                item = %report.item_name,
                hp = report.hp,
                mp = report.mp,
                pot_hp = report.pot_health,
                pot_mp = report.pot_mana,
                buffs = report.buffs.len(),
                spell = ?report.learned.map(|magic| magic.spell),
                "使用物品"
            ),
            Err(error) => tracing::debug!(session = self.id, %error, "使用物品被拒绝"),
        }
    }

    /// 丢金币（`C.DropGold`，Crystal `PlayerObject.DropGold`，`PlayerObject.cs:7473-7482`）。
    async fn on_drop_gold(&mut self, amount: u32) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.drop_gold(object_id, amount) {
            Ok(Some(ground)) => {
                tracing::debug!(session = self.id, amount, ground, "丢金币");
            }
            Ok(None) => tracing::debug!(session = self.id, amount, "丢金币：附近没有空格"),
            Err(error) => tracing::debug!(session = self.id, amount, %error, "丢金币被拒绝"),
        }
    }

    /// 施法（`C.Magic`，Crystal `HumanObject.Magic`，`HumanObject.cs:3400-3780`）。
    ///
    /// Crystal 的连接层会在 `ActionTime > now || SpellTime > now` 时把包放进 `_retryList`
    /// 下个周期重试（`MirConnection.cs:1577-1580`）；与移动/攻击一致，这里用
    /// 「等到 deadline 再重试一次」等价实现。
    async fn on_magic(
        &mut self,
        spell: mir_core::enums::Spell,
        direction: MirDirection,
        target_id: u32,
        location: Point,
    ) {
        let Some(object_id) = self.object_id else {
            return;
        };
        for attempt in 0..2 {
            let busy_until = {
                let mut world = self.world.lock().await;
                match world.cast_magic(object_id, spell, direction, target_id, location) {
                    Ok(report) => {
                        tracing::debug!(
                            session = self.id,
                            spell = ?report.spell,
                            target = report.target_id,
                            damage = report.damage,
                            heal = report.heal,
                            cost = report.cost,
                            "施法"
                        );
                        return;
                    }
                    Err(crate::magic::MagicError::Busy { until }) => until,
                    Err(error) => {
                        tracing::debug!(session = self.id, %error, "施法被拒绝");
                        return;
                    }
                }
            };
            if attempt == 0 && self.wait_until(busy_until).await {
                continue;
            }
            return;
        }
    }

    /// 改技能快捷键（`C.MagicKey`，Crystal `MirConnection.MagicKey`，`MirConnection.cs:1543-1566`）。
    async fn on_magic_key(&mut self, spell: mir_core::enums::Spell, key: u8) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        let changed = world.set_magic_key(object_id, spell, key);
        tracing::debug!(session = self.id, ?spell, key, changed, "技能快捷键");
    }

    /// 惰性每日重置：除登录外，每次任务交互前补做一次。
    ///
    /// Crystal 在服务器跨天（0 点）对全角色 `ClearDailyQuests`（`Envir::ProcessNewDay`，
    /// `Envir.cs:4856-4866`）；本作没有全局日切，按角色 24h 惰性补（判定见
    /// [`crate::quests::daily_reset_due`]），差异见 `docs/world/quests.md`。
    async fn maybe_reset_daily_quests(&mut self) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let Some(last_reset) = self
            .character
            .as_ref()
            .map(|character| character.last_daily_reset_at)
        else {
            return;
        };
        let now = {
            let world = self.world.lock().await;
            world.wall_now()
        };
        if !crate::quests::daily_reset_due(last_reset, now) {
            return;
        }
        {
            let mut world = self.world.lock().await;
            world.reset_daily_quests(object_id);
        }
        if let Some(character) = self.character.as_mut() {
            character.last_daily_reset_at = now;
        }
        tracing::debug!(session = self.id, "任务交互前补做每日任务重置");
    }

    /// 接任务（`C.AcceptQuest`，Crystal `PlayerObject.AcceptQuest`，`PlayerObject.cs:11251-11348`）。
    async fn on_accept_quest(&mut self, npc_index: u32, quest_index: i32) {
        self.maybe_reset_daily_quests().await;
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.accept_quest(object_id, npc_index, quest_index) {
            Ok(report) => tracing::debug!(
                session = self.id,
                quest = report.quest_index,
                name = %report.name,
                tasks = report.task_list.len(),
                "接取任务"
            ),
            Err(error) => tracing::debug!(session = self.id, %error, "接取任务被拒绝"),
        }
    }

    /// 交任务（`C.FinishQuest`，Crystal `PlayerObject.FinishQuest`，`:11350-11461`）。
    ///
    /// `selected_item_index` 就是 Crystal 的「选第几个可选奖励」（`:11350` 的默认参数 `-1`）。
    async fn on_finish_quest(&mut self, quest_index: i32, selected_item_index: i32) {
        self.maybe_reset_daily_quests().await;
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.finish_quest(object_id, quest_index, selected_item_index) {
            Ok(report) => tracing::debug!(
                session = self.id,
                quest = report.quest_index,
                gold = report.gold,
                exp = report.exp,
                credit = report.credit,
                items = report.items.len(),
                "交付任务"
            ),
            Err(error) => tracing::debug!(session = self.id, %error, "交付任务被拒绝"),
        }
    }

    /// 放弃任务（`C.AbandonQuest`，Crystal `PlayerObject.AbandonQuest`，`:11462-11471`）。
    async fn on_abandon_quest(&mut self, quest_index: i32) {
        self.maybe_reset_daily_quests().await;
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.abandon_quest(object_id, quest_index) {
            Ok(()) => tracing::debug!(session = self.id, quest = quest_index, "放弃任务"),
            Err(error) => tracing::debug!(session = self.id, %error, "放弃任务被拒绝"),
        }
    }

    /// 分享任务（`C.ShareQuest`，Crystal `PlayerObject.ShareQuest`，`:11472-11492`）。
    ///
    /// 组队系统的范围校验在 `World::share_quest` 里（同图 + `Globals.DataRange` + 活着），
    /// 不在队伍里会走 Crystal 的失败分支（系统提示）。
    async fn on_share_quest(&mut self, quest_index: i32) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.share_quest(object_id, quest_index) {
            Ok(receivers) => {
                tracing::debug!(
                    session = self.id,
                    quest = quest_index,
                    receivers,
                    "分享任务"
                );
            }
            Err(error) => tracing::debug!(session = self.id, %error, "分享任务失败"),
        }
    }

    /// 开关组队（`C.SwitchGroup`，Crystal `PlayerObject.SwitchGroup`，`PlayerObject.cs:9217-9227`）。
    async fn on_switch_group(&mut self, allow: bool) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.switch_group(object_id, allow) {
            Ok(changed) => {
                tracing::debug!(session = self.id, allow, changed, "切换组队开关");
            }
            Err(error) => tracing::debug!(session = self.id, %error, "切换组队开关失败"),
        }
    }

    /// 邀请组队（`C.AddMember`，Crystal `PlayerObject.AddMember`，`PlayerObject.cs:9273-9338`）。
    async fn on_add_member(&mut self, name: &str) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.group_invite(object_id, name) {
            Ok(target) => tracing::debug!(session = self.id, target, "已发出组队邀请"),
            Err(error) => tracing::debug!(session = self.id, %error, "组队邀请失败"),
        }
    }

    /// 踢人/退队（`C.DelMember`，Crystal `PlayerObject.DelMember`，`PlayerObject.cs:9340-9370`）。
    async fn on_del_member(&mut self, name: &str) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.group_kick(object_id, name) {
            Ok(removed) => tracing::debug!(session = self.id, removed, "移除队员"),
            Err(error) => tracing::debug!(session = self.id, %error, "移除队员失败"),
        }
    }

    /// 接受/拒绝组队邀请（`C.GroupInvite`，Crystal `PlayerObject.GroupInvite`，`PlayerObject.cs:9372-9485`）。
    async fn on_group_invite(&mut self, accept: bool) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.group_join(object_id, accept) {
            Ok(outcome) => {
                tracing::debug!(session = self.id, accept, ?outcome, "处理组队邀请");
            }
            Err(error) => tracing::debug!(session = self.id, %error, "接受组队邀请失败"),
        }
    }

    /// 行会成员编辑（`C.EditGuildMember`，Crystal `PlayerObject.EditGuildMember`，
    /// `PlayerObject.cs:9840-9957`）。
    ///
    /// 世界层按 `ChangeType` 分派并负责发包/提示，这里只管日志与落盘。
    async fn on_edit_guild_member(
        &mut self,
        change_type: u8,
        rank_index: u8,
        name: &str,
        rank_name: &str,
    ) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let edit = crate::guild::GuildMemberEdit {
            change_type,
            rank_index,
            name: name.to_owned(),
            rank_name: rank_name.to_owned(),
        };
        {
            let mut world = self.world.lock().await;
            match world.guild_edit_member(object_id, &edit) {
                Ok(()) => tracing::debug!(
                    session = self.id,
                    change_type,
                    rank_index,
                    name,
                    "行会成员编辑"
                ),
                Err(error) => {
                    tracing::debug!(session = self.id, %error, "行会成员编辑被拒绝");
                }
            }
        }
        self.flush_guild_writes().await;
    }

    /// 改公告（`C.EditGuildNotice`，Crystal `PlayerObject.EditGuildNotice`，`PlayerObject.cs:9958-9977`）。
    async fn on_edit_guild_notice(&mut self, notice: Vec<String>) {
        let Some(object_id) = self.object_id else {
            return;
        };
        {
            let mut world = self.world.lock().await;
            match world.guild_edit_notice(object_id, notice) {
                Ok(()) => tracing::debug!(session = self.id, "行会公告已更新"),
                Err(error) => tracing::debug!(session = self.id, %error, "行会公告更新被拒绝"),
            }
        }
        self.flush_guild_writes().await;
    }

    /// 接受/拒绝行会邀请（`C.GuildInvite`，Crystal `PlayerObject.GuildInvite`，
    /// `PlayerObject.cs:9978-10011`）。
    async fn on_guild_invite(&mut self, accept: bool) {
        let Some(object_id) = self.object_id else {
            return;
        };
        {
            let mut world = self.world.lock().await;
            match world.guild_join(object_id, accept) {
                Ok(outcome) => tracing::debug!(session = self.id, accept, ?outcome, "处理行会邀请"),
                Err(error) => tracing::debug!(session = self.id, %error, "接受行会邀请失败"),
            }
        }
        self.flush_guild_writes().await;
    }

    /// 拉取行会公告/成员表（`C.RequestGuildInfo`，Crystal `PlayerObject.RequestGuildInfo`，
    /// `PlayerObject.cs:10012-10028`）。
    async fn on_request_guild_info(&mut self, type_: u8) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.guild_request_info(object_id, type_) {
            Ok(()) => tracing::debug!(session = self.id, type_, "行会信息已下发"),
            Err(error) => tracing::debug!(session = self.id, %error, "行会信息下发失败"),
        }
    }

    /// 行会仓库金币存取（`C.GuildStorageGoldChange`，Crystal `PlayerObject.GuildStorageGoldChange`，
    /// `PlayerObject.cs:10061-10120`）。
    async fn on_guild_storage_gold_change(&mut self, type_: u8, amount: u32) {
        let Some(object_id) = self.object_id else {
            return;
        };
        {
            let mut world = self.world.lock().await;
            match world.guild_storage_gold_change(object_id, type_, amount) {
                Ok(()) => tracing::debug!(session = self.id, type_, amount, "行会仓库金币变动"),
                Err(error) => tracing::debug!(session = self.id, %error, "行会仓库金币变动被拒绝"),
            }
        }
        self.flush_guild_writes().await;
    }

    /// 行会仓库物品存取（`C.GuildStorageItemChange`，Crystal
    /// `PlayerObject.GuildStorageItemChange`，`PlayerObject.cs:10121-10282`）。
    async fn on_guild_storage_item_change(&mut self, type_: u8, from: i32, to: i32) {
        let Some(object_id) = self.object_id else {
            return;
        };
        {
            let mut world = self.world.lock().await;
            match world.guild_storage_item_change(object_id, type_, from, to) {
                Ok(()) => tracing::debug!(session = self.id, type_, from, to, "行会仓库物品变动"),
                Err(error) => {
                    tracing::debug!(session = self.id, %error, "行会仓库物品变动被拒绝");
                }
            }
        }
        self.flush_guild_writes().await;
    }

    /// 行会增益（`C.GuildBuffUpdate`，Crystal `PlayerObject.GuildBuffUpdate`，
    /// `PlayerObject.cs:10350-10436`）。
    async fn on_guild_buff_update(&mut self, action: u8, id: i32) {
        let Some(object_id) = self.object_id else {
            return;
        };
        {
            let mut world = self.world.lock().await;
            match world.guild_buff_update(object_id, action, id) {
                Ok(()) => tracing::debug!(session = self.id, action, id, "行会增益请求已处理"),
                Err(error) => tracing::debug!(session = self.id, %error, "行会增益请求被拒绝"),
            }
        }
        self.flush_guild_writes().await;
    }

    /// 客户端把行会名送回来（`C.GuildNameReturn`，Crystal `PlayerObject.GuildNameReturn`，
    /// `PlayerObject.cs:10029-10060`）。
    ///
    /// 只有点过 NPC 的 `[@CREATEGUILD]` 页面（`Player.npc.can_create_guild`）才接受，
    /// 与 Crystal 的 `CanCreateGuild` 一致；无论成败都清掉这个标记（`:10031`、`:10059`）。
    async fn on_guild_name_return(&mut self, name: &str) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        let allowed = world
            .player(object_id)
            .is_some_and(|player| player.npc.can_create_guild);
        if !allowed {
            tracing::debug!(
                session = self.id,
                "忽略 C.GuildNameReturn（没有申请创建行会）"
            );
            return;
        }
        if name.is_empty() {
            if let Some(player) = world.player_mut(object_id) {
                player.npc.can_create_guild = false;
            }
            return;
        }
        let result = world.create_guild(object_id, name);
        if let Some(player) = world.player_mut(object_id) {
            player.npc.can_create_guild = false;
        }
        drop(world);
        match &result {
            Ok(guild_id) => tracing::info!(session = self.id, guild = guild_id, %name, "创建行会"),
            Err(error) => {
                tracing::debug!(session = self.id, %error, "创建行会被拒绝");
                self.send_system_chat(&error.message(self.language.language()));
            }
        }
        self.flush_guild_writes().await;
    }

    /// 把世界上积压的行会写入落盘（见 [`crate::guild::persist_guild_writes`]）。
    async fn flush_guild_writes(&self) {
        let writes = {
            let mut world = self.world.lock().await;
            world.take_guild_writes()
        };
        if writes.is_empty() {
            return;
        }
        if let Err(error) = crate::guild::persist_guild_writes(self.store.as_ref(), writes).await {
            tracing::warn!(session = self.id, %error, "行会落盘失败");
        }
    }

    /// 把世界里的全部市场挂单按**整表快照**落盘（见 [`crate::market::flush_market_update`]）。
    ///
    /// 只在上架/买断/提前成交/取回这些变更包处理完之后由 [`Session::handle`] 的统一钩子
    /// 调用；下线时再补一次。落盘内容包括：
    /// * **当前角色行快照**（背包/金币等市场操作刚改过的字段）——F3；
    /// * 市场操作产生的系统邮件（成交邮寄/出价退回）——F3；
    /// * 最新挂单快照——F1/F3。
    ///
    /// 三者由 [`crate::market::flush_market_update`] 串行化并在同一事务提交；
    /// 这里不在 world 锁内做 IO。
    async fn flush_auction_writes(&self) {
        let character = {
            let world = self.world.lock().await;
            self.object_id.and_then(|object_id| {
                let player = world.player(object_id)?;
                self.character_snapshot(
                    player,
                    world.quest_inventory(object_id),
                    world.pet_mode(object_id),
                )
            })
        };
        if let Err(error) =
            crate::market::flush_market_update(self.store.as_ref(), &self.world, character).await
        {
            tracing::warn!(session = self.id, %error, "市场变更原子落盘失败");
        }
    }

    /// 用世界里的玩家状态刷新本会话的角色存档视图。
    ///
    /// 与 [`crate::market::character_snapshot`] 共用同一份编解码；市场落盘和下线存盘
    /// 都必须看到同一份「世界 → 角色行」映射。
    ///
    /// `quest_inventory` 是**世界里当前**的任务物品栏：`market::character_snapshot` 从
    /// 登录时的 base 克隆，不覆写的话市场/商城任何一次落盘都会把
    /// `characters.quest_inventory` 写回登录值（丢任务物品）。
    ///
    /// `pet_mode` 同理：世界里的宠物命令才是活值，市场/商城落盘不能把登录时那一字节写回去。
    fn character_snapshot(
        &self,
        player: &Player,
        quest_inventory: Option<&crate::quest_inventory::QuestInventory>,
        pet_mode: mir_core::enums::PetMode,
    ) -> Option<Character> {
        self.character.as_ref().map(|base| {
            // `pet_mode` 由 `market::character_snapshot` 一起写进行（市场/商城/下线口径统一）。
            let mut snapshot = crate::market::character_snapshot(base, player, pet_mode);
            if let Some(inventory) = quest_inventory {
                snapshot.quest_inventory = inventory.encode();
            }
            snapshot
        })
    }

    /// 客户端把物品放进仓库：`C.StoreItem`（Crystal `PlayerObject.StoreItem`，
    /// `Server/MirObjects/PlayerObject.cs:5376-5456`）。
    ///
    /// 成败都由世界层直接回 `S.StoreItem{Success}`（Crystal 每条拒绝分支只 `Enqueue(p)`），
    /// 这里只记日志。
    async fn on_store_item(&mut self, from: i32, to: i32) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let result = {
            let mut world = self.world.lock().await;
            world.storage_store_item(object_id, from, to)
        };
        match result {
            Ok(()) => tracing::debug!(session = self.id, from, to, "物品已存入仓库"),
            Err(error) => tracing::debug!(session = self.id, from, to, %error, "存仓库被拒绝"),
        }
    }

    /// 客户端从仓库取回物品：`C.TakeBackItem`（Crystal `PlayerObject.TakeBackItem`，
    /// `PlayerObject.cs:5457-5526`）。
    async fn on_take_back_item(&mut self, from: i32, to: i32) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let result = {
            let mut world = self.world.lock().await;
            world.storage_take_back_item(object_id, from, to)
        };
        match result {
            Ok(()) => tracing::debug!(session = self.id, from, to, "物品已从仓库取出"),
            Err(error) => tracing::debug!(session = self.id, from, to, %error, "取仓库被拒绝"),
        }
    }

    /// `C.SendMail`：Crystal `MirConnection.SendMail`（`MirConnection.cs:2003-2015`）→
    /// `PlayerObject.SendMail(name, message, gold, items, stamped)`（`:11727-11845`）。
    ///
    /// 收件人由会话按角色名查（大小写不敏感、允许离线），世界层只做游戏规则。
    async fn on_send_mail(&mut self, packet: mir_protocol::packets::client::SendMail) {
        if self.stage != Stage::Game {
            return;
        }
        let Some(object_id) = self.object_id else {
            return;
        };
        let recipient = match self.store.find_character_by_name(&packet.name).await {
            Ok(Some(character)) => {
                let mail_count = match self.store.list_mail(character.id).await {
                    Ok(mails) => mails.len(),
                    Err(error) => {
                        tracing::warn!(session = self.id, %error, "读收件人邮箱失败，按 0 封处理");
                        0
                    }
                };
                Some(MailRecipient {
                    character_id: character.id,
                    name: character.name,
                    mail_count,
                })
            }
            Ok(None) => None,
            Err(error) => {
                tracing::warn!(session = self.id, %error, "按名查角色失败");
                None
            }
        };
        {
            let mut world = self.world.lock().await;
            if let Err(error) = world.mail_send(
                object_id,
                recipient,
                &packet.message,
                packet.gold,
                &packet.items_idx,
                packet.stamped,
            ) {
                tracing::debug!(session = self.id, %error, "发送邮件被拒绝");
            }
        }
        self.flush_mail_writes().await;
    }

    /// `C.ReadMail`：标记已读并重发邮箱（Crystal `PlayerObject.ReadMail`，`:11847-11856`）。
    async fn on_read_mail(&mut self, mail_id: u64) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let result = {
            let mut world = self.world.lock().await;
            world.mail_read(object_id, mail_id)
        };
        if let Err(error) = result {
            tracing::debug!(session = self.id, mail_id, %error, "阅读邮件失败");
        }
        self.flush_mail_writes().await;
    }

    /// `C.CollectParcel`：领取附件（Crystal `PlayerObject.CollectMail`，`:11858-11902`）。
    async fn on_collect_parcel(&mut self, mail_id: u64) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let result = {
            let mut world = self.world.lock().await;
            world.mail_collect(object_id, mail_id)
        };
        if let Err(error) = result {
            tracing::debug!(session = self.id, mail_id, %error, "领取邮件附件失败");
        }
        self.flush_mail_writes().await;
    }

    /// `C.DeleteMail`：删除邮件（Crystal `PlayerObject.DeleteMail`，`:11904-11913`）。
    async fn on_delete_mail(&mut self, mail_id: u64) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let result = {
            let mut world = self.world.lock().await;
            world.mail_delete(object_id, mail_id)
        };
        if let Err(error) = result {
            tracing::debug!(session = self.id, mail_id, %error, "删除邮件失败");
        }
        self.flush_mail_writes().await;
    }

    /// `C.LockMail`：锁定/解锁（Crystal `PlayerObject.LockMail`，`:11915-11924`）。
    async fn on_lock_mail(&mut self, mail_id: u64, lock: bool) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let result = {
            let mut world = self.world.lock().await;
            world.mail_lock(object_id, mail_id, lock)
        };
        if let Err(error) = result {
            tracing::debug!(session = self.id, mail_id, lock, %error, "锁定邮件失败");
        }
        self.flush_mail_writes().await;
    }

    /// `C.MailLockedItem`：写信界面的附件格子锁，原样回显（`MirConnection.cs:667-669`）。
    async fn on_mail_locked_item(&mut self, unique_id: u64, locked: bool) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        world.mail_locked_item(object_id, unique_id, locked);
    }

    /// `C.MailCost`：报价（Crystal `MirConnection.MailCost`，`MirConnection.cs:2045-2052`）。
    async fn on_mail_cost(&mut self, gold: u32, items_idx: &[u64; 5], stamped: bool) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        world.mail_cost_request(object_id, items_idx, gold, stamped);
    }

    /// 从 `mail` 表装载邮箱，并过滤/删除过期邮件（扩展，见 [`crate::mail`]）。
    async fn load_mailbox(&self, character_id: i64) -> Vec<Mail> {
        let records = match self.store.list_mail(character_id).await {
            Ok(records) => records,
            Err(error) => {
                tracing::warn!(session = self.id, %error, "读邮箱失败，按空邮箱处理");
                return Vec::new();
            }
        };
        let now = now_unix();
        let mut mails = Vec::with_capacity(records.len());
        for record in records {
            match Mail::from_record(&record) {
                Ok(mail) if !mail.is_expired(now) => mails.push(mail),
                Ok(mail) => {
                    // `// 扩展：` Crystal 没有邮件过期；本作查询时顺手删掉过期邮件。
                    tracing::debug!(session = self.id, mail_id = mail.id, "邮件已过期，删除");
                    if let Err(error) = self.store.delete_mail(mail.id).await {
                        tracing::warn!(session = self.id, %error, "删除过期邮件失败");
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        session = self.id,
                        mail_id = record.id,
                        %error,
                        "邮件附件解析失败，跳过这封邮件"
                    );
                }
            }
        }
        mails
    }

    /// 把世界上积压的邮件写入落盘（见 [`crate::mail::persist_mail_writes`]）。
    async fn flush_mail_writes(&self) {
        let writes = {
            let mut world = self.world.lock().await;
            world.take_mail_writes()
        };
        if writes.is_empty() {
            return;
        }
        if let Err(error) = crate::mail::persist_mail_writes(self.store.as_ref(), writes).await {
            tracing::warn!(session = self.id, %error, "邮件落盘失败");
        }
    }

    // -----------------------------------------------------------------------
    // 玩家交易（见 [`crate::trade`] 与 `docs/world/trade.md`）
    // -----------------------------------------------------------------------

    /// 允许/禁止别人向自己发起交易：`C.ChangeTrade{AllowTrade}`
    /// （Crystal `MirConnection.cs:1448-1452` → `PlayerObject.AllowTrade = p.AllowTrade`）。
    async fn on_change_trade(&mut self, allow: bool) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        if world.set_allow_trade(object_id, allow) {
            tracing::debug!(session = self.id, allow, "切换交易开关");
        }
    }

    /// 发起交易：`C.TradeRequest{}`（Crystal `PlayerObject.TradeRequest`，
    /// `PlayerObject.cs:10623-10707`）。目标由「面朝格」决定（协议里没有名字）。
    async fn on_trade_request(&mut self) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.trade_request(object_id) {
            Ok(target) => tracing::debug!(session = self.id, target, "已发出交易请求"),
            Err(error) => tracing::debug!(session = self.id, %error, "交易请求被拒绝"),
        }
    }

    /// 回应交易邀请：`C.TradeReply{AcceptInvite}`（`PlayerObject.TradeReply`，`:10708-10743`）。
    async fn on_trade_reply(&mut self, accept: bool) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.trade_reply(object_id, accept) {
            Ok(outcome) => tracing::debug!(session = self.id, ?outcome, "回应交易邀请"),
            Err(error) => tracing::debug!(session = self.id, %error, "回应交易邀请被拒绝"),
        }
    }

    /// 把物品放进交易格：`C.DepositTradeItem{From,To}`（`PlayerObject.cs:10530-10581`）。
    async fn on_deposit_trade_item(&mut self, from: i32, to: i32) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.trade_deposit_item(object_id, from, to) {
            Ok(()) => tracing::debug!(session = self.id, from, to, "物品放进交易格"),
            Err(error) => tracing::debug!(session = self.id, %error, "放物品进交易格被拒绝"),
        }
    }

    /// 从交易格取回物品：`C.RetrieveTradeItem{From,To}`（`PlayerObject.cs:10582-10619`）。
    async fn on_retrieve_trade_item(&mut self, from: i32, to: i32) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.trade_retrieve_item(object_id, from, to) {
            Ok(()) => tracing::debug!(session = self.id, from, to, "物品取回背包"),
            Err(error) => tracing::debug!(session = self.id, %error, "取回交易物品被拒绝"),
        }
    }

    /// 放金币进交易：`C.TradeGold{Amount}`（`PlayerObject.cs:10744-10760`）。
    async fn on_trade_gold(&mut self, amount: u32) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.trade_gold(object_id, amount) {
            Ok(()) => tracing::debug!(session = self.id, amount, "金币放进交易"),
            Err(error) => tracing::debug!(session = self.id, %error, "放金币进交易被拒绝"),
        }
    }

    /// 确认交易：`C.TradeConfirm{Locked}`（`PlayerObject.cs:10789-10888`）。
    async fn on_trade_confirm(&mut self, confirm: bool) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.trade_confirm(object_id, confirm) {
            Ok(()) => tracing::debug!(session = self.id, confirm, "确认交易"),
            Err(error) => tracing::debug!(session = self.id, %error, "确认交易被拒绝"),
        }
    }

    /// 取消交易：`C.TradeCancel{}`（`PlayerObject.cs:10889-10958`）。
    ///
    /// 取消时背包满的物品会转成系统邮件（`PlayerObject.cs:10910-10918`），所以这里
    /// 顺手把邮件写入落盘。
    async fn on_trade_cancel(&mut self) {
        let Some(object_id) = self.object_id else {
            return;
        };
        {
            let mut world = self.world.lock().await;
            world.trade_cancel(object_id);
        }
        self.flush_mail_writes().await;
    }

    // -----------------------------------------------------------------------
    // NPC 寄售/拍卖市场（见 [`crate::market`] 与 `docs/world/market.md`）
    // -----------------------------------------------------------------------

    /// 上架：`C.ConsignItem{UniqueID,Price,Type}`（Crystal `PlayerObject.ConsignItem`，
    /// `PlayerObject.cs:8218-8316`）。成功时背包扣物品、扣手续费并回 `S.ConsignItem`。
    async fn on_consign_item(&mut self, unique_id: u64, price: u32, panel: MarketPanelType) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.consign_item(object_id, unique_id, price, panel) {
            Ok(auction_id) => tracing::debug!(session = self.id, auction_id, "物品已上架"),
            Err(error) => tracing::debug!(session = self.id, %error, "上架被拒绝"),
        }
    }

    /// 搜索市场：`C.MarketSearch`（Crystal `PlayerObject.MarketSearch`，`:8424-8448`）。
    async fn on_market_search(
        &mut self,
        match_text: &str,
        item_type: mir_core::enums::ItemType,
        user_mode: bool,
        min_shape: i16,
        max_shape: i16,
        panel: MarketPanelType,
    ) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.market_search(
            object_id, match_text, item_type, user_mode, min_shape, max_shape, panel,
        ) {
            Ok(()) => tracing::debug!(session = self.id, "市场搜索"),
            Err(error) => tracing::debug!(session = self.id, %error, "市场搜索被拒绝"),
        }
    }

    /// 刷新市场列表：`C.MarketRefresh`（`MirConnection.cs:1692-1697`）。
    async fn on_market_refresh(&mut self) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.market_refresh(object_id) {
            Ok(()) => tracing::debug!(session = self.id, "刷新市场"),
            Err(error) => tracing::debug!(session = self.id, %error, "刷新市场被拒绝"),
        }
    }

    /// 翻页：`C.MarketPage{Page}`（Crystal `PlayerObject.MarketPage`，`:8327-8374`）。
    async fn on_market_page(&mut self, page: i32) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.market_page(object_id, page) {
            Ok(()) => tracing::debug!(session = self.id, page, "市场翻页"),
            Err(error) => tracing::debug!(session = self.id, %error, "市场翻页被拒绝"),
        }
    }

    /// 购买/出价：`C.MarketBuy{AuctionID,BidPrice}`（`PlayerObject.cs:8450-8589`）。
    ///
    /// **商城页签**（`MarketPanelType::GameShop`）下 Crystal 走另一条分支
    /// （`:8458-8486`）：`AuctionID` 其实是商品 `GIndex`，只按元宝买 1 份。本作把它转发给
    /// [`Session::on_gameshop_buy`]，与 `C.GameshopBuy` 共用同一套校验/扣款/发货/落盘，
    /// 只是回包换成 `S.MarketSuccess`/`S.MarketFail`。
    async fn on_market_buy(&mut self, auction_id: u64, bid_price: u32) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let panel = {
            let world = self.world.lock().await;
            world.market_panel_of(object_id)
        };
        if panel == MarketPanelType::GameShop {
            // 编号超出 `i32`（不可能是合法 GIndex）：按「列表里没有这条」处理。
            let g_index = i32::try_from(auction_id).unwrap_or(-1);
            self.on_gameshop_buy(g_index, 1, 0, GameShopReply::Market)
                .await;
            return;
        }
        let mut world = self.world.lock().await;
        match world.market_buy(object_id, auction_id, bid_price) {
            Ok(()) => tracing::debug!(session = self.id, auction_id, bid_price, "市场成交/出价"),
            Err(error) => tracing::debug!(session = self.id, %error, "市场成交被拒绝"),
        }
    }

    /// 提前成交拍卖：`C.MarketSellNow{AuctionID}`（`PlayerObject.cs:8591-8666`）。
    async fn on_market_sell_now(&mut self, auction_id: u64) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.market_sell_now(object_id, auction_id) {
            Ok(()) => tracing::debug!(session = self.id, auction_id, "拍卖提前成交"),
            Err(error) => tracing::debug!(session = self.id, %error, "拍卖提前成交被拒绝"),
        }
    }

    /// 下架/收款：`C.MarketGetBack{Mode,AuctionID}`（`PlayerObject.cs:8668-8793`）。
    async fn on_market_get_back(&mut self, mode: MarketCollectionMode, auction_id: u64) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        match world.market_get_back(object_id, mode, auction_id) {
            Ok(count) => tracing::debug!(session = self.id, auction_id, count, "从市场取回"),
            Err(error) => tracing::debug!(session = self.id, %error, "从市场取回被拒绝"),
        }
    }

    // -----------------------------------------------------------------------
    // 商城（见 [`crate::market`] 的「商城（GameShop）」一节）
    // -----------------------------------------------------------------------

    /// 商城购买：`C.GameshopBuy{GIndex, Quantity, PType}`（Crystal
    /// `PlayerObject.GameshopBuy`，`PlayerObject.cs:13725-13911`）。
    ///
    /// 流程（整个流程持有 [`crate::market::lock_game_shop`]，见那里的说明）：
    /// 1. 从存储层读「全服已购 + 本角色已购」两份计数（Crystal 的 `Envir.GameshopLog` /
    ///    `Info.GSpurchases`），世界层不缓存它们，所以重启后库存天然正确；
    /// 2. 世界层校验并扣元宝/金币、发货（背包优先，放不下的攒一封商城邮件）；
    /// 3. 成功时把「角色行（含元宝）+ 邮件 + 两组计数」放进**同一个事务**落盘
    ///    （[`crate::market::flush_gameshop_update`]）。
    ///
    /// 失败回包按 Crystal 的语义分两类（[`crate::market::GameShopError::is_silent`]）：
    /// 数量越界/余额不足/不允许该支付方式是**静默**的；商品不存在/库存不足/职业等级不符
    /// 回一条系统聊天。`reply == Market`（商城页签发的 `C.MarketBuy`）时改回
    /// `S.MarketFail{Reason}` + 成功时的 `S.MarketSuccess`。
    async fn on_gameshop_buy(
        &mut self,
        g_index: i32,
        quantity: u8,
        p_type: i32,
        reply: GameShopReply,
    ) {
        if self.stage != Stage::Game {
            return;
        }
        let Some(object_id) = self.object_id else {
            return;
        };
        let Some(character_id) = self.character.as_ref().map(|character| character.id) else {
            return;
        };
        // 串行化「读计数 → 校验/扣款/发货 → 落盘」：两个会话并发购买时不能拿着同一份
        // 旧计数各自通过库存检查。
        let _guard = crate::market::lock_game_shop().await;
        let purchases = match self.load_game_shop_counts(character_id).await {
            Ok(purchases) => purchases,
            Err(error) => {
                // 读不到计数就不能判断库存：直接拒绝（按「没买过」放行会超卖限量商品）。
                tracing::warn!(session = self.id, %error, "读商城已购计数失败，拒绝本次购买");
                match reply {
                    GameShopReply::Market => {
                        self.send_market_fail(crate::market::MarketFailReason::TooFar);
                    }
                    GameShopReply::Chat => {
                        let message = self
                            .language
                            .text_with(&self.languages, TextKey::GameshopStoreUnavailable);
                        self.send_system_chat(&message);
                    }
                }
                return;
            }
        };
        let result = {
            let mut world = self.world.lock().await;
            world.gameshop_buy(object_id, g_index, quantity, p_type, &purchases)
        };
        match result {
            Ok(report) => {
                tracing::info!(
                    session = self.id,
                    g_index,
                    quantity,
                    cost = report.cost,
                    gained = report.gained,
                    mailed = report.mailed,
                    "商城购买成功，开始落盘"
                );
                let (purchases, stock) = report.to_records(character_id);
                match self.flush_gameshop(purchases, stock).await {
                    Ok(()) => {
                        if reply == GameShopReply::Market {
                            let message = self.language.format_with(
                                &self.languages,
                                TextKey::GameshopBoughtMarketSuccess,
                                &[
                                    ("item", &report.product_name),
                                    ("count", &report.quantity.to_string()),
                                ],
                            );
                            self.send(ServerPacket::MarketSuccess(MarketSuccess { message }));
                        }
                    }
                    Err(error) => {
                        // 事务失败 = 数据库里没有任何变更：把世界层的扣款/发货原样撤回，
                        // 否则下线时角色行会落盘而库存计数不会（计数只由这个事务写）。
                        tracing::warn!(
                            session = self.id,
                            g_index,
                            %error,
                            "商城购买落盘失败，回滚本次购买"
                        );
                        {
                            let mut world = self.world.lock().await;
                            world.rollback_gameshop_purchase(object_id, &report);
                        }
                        match reply {
                            GameShopReply::Market => {
                                self.send_market_fail(crate::market::MarketFailReason::TooFar);
                            }
                            GameShopReply::Chat => {
                                let message = self
                                    .language
                                    .text_with(&self.languages, TextKey::GameshopRolledBack);
                                self.send_system_chat(&message);
                            }
                        }
                    }
                }
            }
            Err(error) => {
                tracing::debug!(session = self.id, g_index, %error, "商城购买被拒绝");
                match reply {
                    GameShopReply::Market => self.send_market_fail(error.market_reason()),
                    // Crystal：数量/组数越界、余额不足、`CanBuy*` 为假都是静默 return。
                    GameShopReply::Chat if !error.is_silent() => {
                        self.send_system_chat(&error.message(self.language.language()));
                    }
                    GameShopReply::Chat => {}
                }
            }
        }
    }

    /// 读一次商城已购计数（全服 + 本角色），供 [`World::gameshop_buy`](crate::World) 校验库存。
    async fn load_game_shop_counts(
        &self,
        character_id: i64,
    ) -> Result<GameShopPurchases, StoreError> {
        let stock = self.store.load_game_shop_stock().await?;
        let purchases = self.store.load_game_shop_purchases(character_id).await?;
        Ok(GameShopPurchases::from_records(&stock, &purchases))
    }

    /// 把一次商城购买原子落盘：角色行（含元宝）+ 商城邮件 + 两组已购计数。
    ///
    /// 失败时**不吞错**：调用方（[`Session::on_gameshop_buy`]）必须回滚世界层的扣款/发货，
    /// 否则角色行会在下线时落盘而计数不会（计数只由这个事务写）。
    async fn flush_gameshop(
        &self,
        purchases: Vec<GameShopPurchaseRecord>,
        stock: Vec<GameShopStockRecord>,
    ) -> Result<(), StoreError> {
        let character = {
            let world = self.world.lock().await;
            self.object_id.and_then(|object_id| {
                let player = world.player(object_id)?;
                self.character_snapshot(
                    player,
                    world.quest_inventory(object_id),
                    world.pet_mode(object_id),
                )
            })
        };
        let update = GameShopUpdate {
            // 角色行与待写邮件由 `flush_gameshop_update` 补齐（它以最新的邮件队列为准）。
            character: None,
            mail_writes: Vec::new(),
            purchases,
            stock,
        };
        crate::market::flush_gameshop_update(self.store.as_ref(), &self.world, character, update)
            .await
    }

    /// 商城页签路径上的 `S.MarketFail`（Crystal `MarketBuy` 的商城分支，`:8458-8486`）。
    fn send_market_fail(&self, reason: crate::market::MarketFailReason) {
        self.send(ServerPacket::MarketFail(MarketFail {
            reason: reason.reason(),
        }));
    }

    /// 开门：`C.Opendoor{DoorIndex}`（Crystal `PlayerObject.Opendoor`，`PlayerObject.cs:7818-7826`）。
    ///
    /// 成败都不回失败包（Crystal 找不到门时静默返回）；世界层在成功时给本人与同图视野内
    /// 的玩家发 `S.Opendoor`，并安排 5 秒后自动关门（`Map.cs:657-665`）。
    async fn on_opendoor(&mut self, door_index: u8) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let mut world = self.world.lock().await;
        let opened = world.open_door(object_id, door_index);
        tracing::debug!(session = self.id, door_index, opened, "开门请求");
    }

    /// 如果是因为「动作还没结束」被拒，等到 `ActionTime` 之后再试一次。
    ///
    /// 返回 `true` 表示值得重试。等待时长有上限，避免异常状态把连接卡死。
    async fn wait_for_action(&self, error: &crate::world::MoveError) -> bool {
        let crate::world::MoveError::Busy { until } = *error else {
            return false;
        };
        self.wait_until(until).await
    }

    /// 睡到服务器时钟的 `until`（最多 1 秒），返回是否值得重试。
    async fn wait_until(&self, until: i64) -> bool {
        let now = {
            let world = self.world.lock().await;
            world.now()
        };
        let wait_ms = (until - now).clamp(0, 1_000);
        if wait_ms <= 0 {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(wait_ms as u64)).await;
        true
    }

    async fn on_chat(&mut self, message: &str) {
        // 游戏内语言切换：`/language <代码>`（协议里没有对应包，是我们的扩展）。
        if message.starts_with("/language") {
            let Some(result) = self.language.parse_command(message) else {
                return;
            };
            if let Ok(language) = result {
                // 语言随账号持久化，下次登录直接生效。
                if let Some(account) = &self.account {
                    let account_id = account.id;
                    if let Err(error) = self
                        .store
                        .set_language(account_id, language.code().to_owned())
                        .await
                    {
                        tracing::warn!(session = self.id, %error, "保存语言设置失败");
                    } else if let Some(account) = &mut self.account {
                        language.code().clone_into(&mut account.language);
                    }
                }
                let reply = self
                    .language
                    .text_with(&self.languages, TextKey::LanguageChanged);
                self.send_system_chat(&reply);
                // 世界层也同步一份：否则市场/交易/邮件/NPC 的提示仍用旧语言。
                if let Some(object_id) = self.object_id {
                    let mut world = self.world.lock().await;
                    world.set_player_language(object_id, language);
                }
            } else {
                let reply = self
                    .language
                    .text_with(&self.languages, TextKey::UnknownLanguage);
                self.send_system_chat(&reply);
            }
            return;
        }
        let Some(object_id) = self.object_id else {
            return;
        };
        // GM 命令：只有 GM 账号才消费 `@...`；非 GM 的 `@...` 按普通聊天走，
        // 不报错、也不泄露命令是否存在（Crystal 在 `PlayerObject.Chat` 里做同样的判定）。
        if message.starts_with('@') && self.is_gm() {
            let mut world = self.world.lock().await;
            if world.run_gm_command(object_id, message) {
                return;
            }
        }
        let mut world = self.world.lock().await;
        match world.chat(object_id, message) {
            Ok(receivers) => {
                tracing::debug!(session = self.id, receivers, "聊天广播");
            }
            Err(error) => tracing::debug!(session = self.id, %error, "聊天失败"),
        }
    }

    /// 当前账号是否 GM（`accounts.admin` 或 `MIR2_GM_ACCOUNTS` 名单，见 [`crate::gm::is_gm`]）。
    #[must_use]
    pub fn is_gm(&self) -> bool {
        self.account
            .as_ref()
            .is_some_and(|account| crate::gm::is_gm(account, &self.config))
    }

    /// 给本会话发一条系统提示（走 `Chat` 包，`ChatType::System`）。
    fn send_system_chat(&self, message: &str) {
        self.send(ServerPacket::Chat(mir_protocol::packets::server::Chat {
            message: message.to_owned(),
            type_: ChatType::System,
        }));
    }

    /// 离开世界并按需存盘。
    ///
    /// 存盘时把世界里的**当前**状态回写到角色行：背包/装备编成 Crystal 的 `UserItem[]`
    /// 字节块（`CharacterInfo.cs:431-447`），金币/HP/MP/等级/经验/坐标也一并同步——
    /// Crystal 的 `CharacterInfo` 是整行落盘的（`Envir.cs` 的 `SaveAccounts`），
    /// 本作的角色行同样是整行回写（`mir_store::save_character`）。
    pub async fn leave_world(&mut self) {
        let Some(object_id) = self.object_id.take() else {
            return;
        };
        let (leaving, quest_inventory, pet_mode) = {
            let mut world = self.world.lock().await;
            // 先取走任务物品栏与宠物命令（`leave` 会清理侧表，取晚了就只剩默认值）。
            let quest_inventory = world.take_quest_inventory(object_id);
            let pet_mode = world.pet_mode(object_id);
            (world.leave(object_id), quest_inventory, pet_mode)
        };
        if let Some(player) = leaving {
            // 世界 → 角色行的映射与市场落盘共用 `market::character_snapshot`，
            // 保证「下线存盘」与「市场事务落盘」不会写出两套不同的背包/金币语义。
            if let Some(character) =
                self.character_snapshot(&player, Some(&quest_inventory), pet_mode)
            {
                self.character = Some(character);
            }
        }
        if let Some(character) = self.character.clone() {
            if let Err(error) = self.store.save_character(character).await {
                tracing::warn!(session = self.id, %error, "下线存盘失败");
            }
        }
        // 下线期间可能还积压着行会写入（成员离线、行会经验……），一起落盘。
        self.flush_guild_writes().await;
        // 邮件的状态变更（已读/锁定/领取/删除）同样补齐。
        self.flush_mail_writes().await;
        // 市场挂单再补一次全量快照（含 tick 里刚标记的过期状态）。
        self.flush_auction_writes().await;
        self.stage = Stage::Select;
    }

    /// 断开：离开世界并保存。
    pub async fn on_disconnected(&mut self) {
        self.leave_world().await;
    }
}

fn select_info(character: &Character) -> SelectInfo {
    SelectInfo {
        index: character.id as i32,
        name: character.name.clone(),
        level: character.level,
        class: character.class,
        gender: character.gender,
        last_access: character.last_access,
    }
}

fn user_information(
    object_id: u32,
    character: &Character,
    position: Point,
    inventory: &[Option<UserItem>],
    equipment: &[Option<UserItem>],
    magics: &[ClientMagic],
    quest_inventory: &[Option<UserItem>],
) -> UserInformation {
    UserInformation {
        object_id,
        real_id: character.id as u32,
        name: character.name.clone(),
        name_colour: mir_core::Color(-1),
        class: character.class,
        gender: character.gender,
        level: character.level,
        location: position,
        direction: character.direction,
        hair: character.hair,
        hp: character.hp,
        mp: character.mp,
        experience: character.experience,
        max_experience: 100,
        level_effects: mir_core::enums::LevelEffects::None,
        has_hero: false,
        hero_behaviour: HeroBehaviour::default(),
        // 背包/装备按**当前内容**下发（Crystal `UserInformation` 里就是 `Info.Inventory` /
        // `Info.Equipment` 整块，`ServerPackets.cs:640-680`）。
        inventory: Some(inventory.to_vec()),
        equipment: Some(equipment.to_vec()),
        // 任务物品栏按**存档里的真实内容**下发（Crystal `Info.QuestInventory`，
        // `ServerPackets.cs:640-680`；`None` 的语义是「保留客户端现值」，登录必须发真值）。
        quest_inventory: Some(quest_inventory.to_vec()),
        gold: u32::try_from(character.gold.max(0)).unwrap_or(u32::MAX),
        credit: u32::try_from(character.credit.max(0)).unwrap_or(u32::MAX),
        has_expanded_storage: false,
        has_storage_password: false,
        require_storage_password: false,
        storage_password_last_set: DateTimeBinary(0),
        expanded_storage_expiry_time: DateTimeBinary(0),
        magics: magics.to_vec(),
        intelligent_creatures: Vec::new(),
        summoned_creature_type: IntelligentCreatureType::None,
        creature_summoned: false,
        allow_observe: true,
        observer: false,
        ..UserInformation::default()
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

/// 账号名校验：Crystal `AccountIDReg` = `^[A-Za-z0-9]{3,15}$`。
#[must_use]
pub fn is_valid_account_id(value: &str) -> bool {
    let length = value.chars().count();
    (3..=15).contains(&length) && value.chars().all(|ch| ch.is_ascii_alphanumeric())
}

/// 口令校验：Crystal `PasswordReg` = `^[A-Za-z0-9]{5,15}$`。
#[must_use]
pub fn is_valid_password(value: &str) -> bool {
    let length = value.chars().count();
    (5..=15).contains(&length) && value.chars().all(|ch| ch.is_ascii_alphanumeric())
}

/// 角色名校验：Crystal `CharacterReg` = `^[\u4e00-\u9fa5_A-Za-z0-9]{3,15}$`。
#[must_use]
pub fn is_valid_character_name(value: &str) -> bool {
    let length = value.chars().count();
    (3..=15).contains(&length)
        && value.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || ch == '_' || ('\u{4e00}'..='\u{9fa5}').contains(&ch)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_rules_match_crystal() {
        assert!(is_valid_account_id("tester"));
        assert!(!is_valid_account_id("di"), "至少 3 位");
        assert!(!is_valid_account_id("tester_1"), "只允许字母数字");
        assert!(!is_valid_account_id(&"a".repeat(16)), "最多 15 位");
        assert!(is_valid_password("secret"));
        assert!(!is_valid_password("1234"), "口令至少 5 位");
    }

    #[test]
    fn character_name_allows_chinese_and_underscore() {
        assert!(is_valid_character_name("裁决者"));
        assert!(is_valid_character_name("战士_01"));
        assert!(!is_valid_character_name("战士!"), "标点不允许");
        assert!(!is_valid_character_name("裁决"), "至少 3 个字");
        assert!(!is_valid_character_name("ab"));
    }
}
