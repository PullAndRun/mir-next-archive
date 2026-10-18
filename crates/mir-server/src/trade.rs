//! 玩家间交易（Crystal `PlayerObject.Trading` 区，`Server/MirObjects/PlayerObject.cs:10528-10960`）。
//!
//! 逐条对照 Crystal（参考根目录 `github\Crystal`，下文只写相对路径）：
//!
//! | 本模块 | Crystal |
//! |---|---|
//! | [`TRADE_SLOTS`] | `public UserItem[] Inventory = new UserItem[46], …, Trade = new UserItem[10]`（`Server/MirDatabase/CharacterInfo.cs:75`） |
//! | [`TRADE_DELAY_MS`] | `Settings.TradeDelay = 2000`（`Server/Settings.cs:337`），`NextTradeTime = Envir.Time + Settings.TradeDelay`（`PlayerObject.cs:10625-10626`） |
//! | [`TRADE_RANGE`] | `Globals.DataRange = 16`（`Shared/Globals.cs:37`），用于 `Functions.InRange(…, Globals.DataRange)`（`PlayerObject.cs:10692`、`:10803`） |
//! | [`TradeState`] | `PlayerObject.TradePartner` / `TradeLocked` / `TradeGoldAmount`（`PlayerObject.cs:177-179`）+ `Info.Trade`（`CharacterInfo.cs:75`） |
//! | [`World::trade_request`] | `PlayerObject.TradeRequest()`（`:10623-10707`） |
//! | [`World::trade_request_target`] | 同上「选人之后」的校验（**本作扩展**，见下） |
//! | [`World::trade_reply`] | `PlayerObject.TradeReply(bool)`（`:10708-10743`） |
//! | [`World::trade_deposit_item`] | `PlayerObject.DepositTradeItem(from, to)`（`:10530-10581`） |
//! | [`World::trade_retrieve_item`] | `PlayerObject.RetrieveTradeItem(from, to)`（`:10582-10619`） |
//! | [`World::trade_gold`] | `PlayerObject.TradeGold(uint)`（`:10744-10760`） |
//! | [`World::trade_confirm`] | `PlayerObject.TradeConfirm(bool)`（`:10789-10888`） |
//! | [`World::trade_cancel`] | `PlayerObject.TradeCancel()`（`:10889-10958`） |
//! | `trade_items_changed` | `PlayerObject.TradeItem()`（`:10761-10777`） |
//! | `trade_unlock` | `PlayerObject.TradeUnlock()`（`:10779-10787`） |
//! | `can_gain_items` | `HumanObject.CanGainItems(UserItem[])`（`HumanObject.cs:7629-7660`） |
//! | `can_gain_gold` | `PlayerObject.CanGainGold(uint)`（`PlayerObject.cs:7638-7641`） |
//! | `trade_gain_item` | `HumanObject.GainItem(UserItem)`（`HumanObject.cs:7824-7835`） |
//!
//! # 包序
//!
//! | 语义 | 客户端包 | 服务端包 | Crystal |
//! |---|---|---|---|
//! | 发起 | `C.TradeRequest{}`（96，**没有目标名**，服务器按「面朝格」选人） | 目标 ← `S.TradeRequest{Name=发起者}`（192） | `MirConnection.cs:1861-1865`、`PlayerObject.cs:10704-10705` |
//! | 回应 | `C.TradeReply{AcceptInvite}`（97） | 双方 ← `S.TradeAccept{Name=对方}`（193） | `PlayerObject.cs:10741-10742` |
//! | 放物品 | `C.DepositTradeItem{From,To}`（30） | 本人 ← `S.DepositTradeItem{From,To,Success}`（50）；对方 ← `S.TradeItem{TradeItems}`（195，整份交易格） | `PlayerObject.cs:10566-10577` |
//! | 取回 | `C.RetrieveTradeItem{From,To}`（31） | 对方 ← `S.TradeItem`；本人 ← `S.RetrieveTradeItem{From,To,Success}`（51） | `PlayerObject.cs:10606-10618` |
//! | 放金币 | `C.TradeGold{Amount}`（98） | 本人 ← `S.LoseGold{Gold=Amount}`（68）；对方 ← `S.TradeGold{Amount=对方累计额}`（194） | `PlayerObject.cs:10755-10759` |
//! | 确认 | `C.TradeConfirm{Locked}`（99） | 提示「等待确认」→ `S.Chat`；成交 → 每件 `S.GainedItem`（66）/`S.GainedGold`(67) → 本人 `S.Chat`「交易成功」→ 本人 `S.TradeConfirm{}`（196） | `PlayerObject.cs:10812-10886` |
//! | 取消/中止 | `C.TradeCancel{}`（100） | 逐件 `S.RetrieveTradeItem{Success=true}`（或 `S.DeleteItem`+邮件）→ 对方 `S.TradeItem` → `S.GainedGold` → `S.TradeCancel{Unlock=false}`（197） | `PlayerObject.cs:10889-10958` |
//! | 允许被交易 | `C.ChangeTrade{AllowTrade}`（46） | 无（只改 `Info.AllowTrade`） | `MirConnection.cs:1448-1452` |
//!
//! # 与 Crystal 的**有意**差异
//!
//! 1. **交易状态不落盘**：Crystal 的 `Info.Trade` 数组虽然序列化在 `CharacterInfo` 里
//!    （`CharacterInfo.cs:431-447` 一带），但交易本身是临时状态（下线/切图/移动都会
//!    `TradeCancel`，`PlayerObject.cs:407`、`:572-573`、`:1477`、`:4300`），所以本作把
//!    [`Player::trade`](crate::world::Player::trade) 完全放在运行期，不写库。
//! 2. **`TradeRequest` 没有目标名**：协议里 `C.TradeRequest` 是空包
//!    （`Shared/ClientPackets.cs`、`Client/MirScenes/GameDays` 的 `GameScene.cs:777`），
//!    Crystal 用 `Functions.PointMove(CurrentLocation, Direction, 1)` 取**面朝格**
//!    （`PlayerObject.cs:10634-10658`）。本作照抄这条路径；另外把「选人之后」的校验
//!    暴露成 [`World::trade_request_target`]（**本作扩展**，用于测试与以后「点玩家交易」
//!    的客户端；校验顺序与源码完全一致）。
//! 3. **死亡会中止交易**：Crystal 的 `Die()`（`PlayerObject.cs:578-667`）**不**清交易状态，
//!    只有死亡后的下一次移动/切图才会 `TradeCancel`。需求要求死亡即中止，因此本作在
//!    [`World::player_died`](crate::world::World::player_died) 里补一次
//!    [`World::trade_cancel`]，属于**有意扩展**（见 `docs/world/trade.md`）。
//! 4. **交易格变更即解锁**：Crystal 的 `DepositTradeItem`/`RetrieveTradeItem` 本身不检查
//!    `TradeLocked`，而是移动成功后走 `TradeItem()` → `TradeUnlock()` 把**双方**解锁
//!    （`PlayerObject.cs:10761-10787`），客户端在 `S.DepositTradeItem`/`S.RetrieveTradeItem`/
//!    `S.TradeItem`/`S.TradeGold` 上也会 `ChangeLockState(false)`（`Client/MirScenes/GameScene.cs:2804-2830`、`:6312-6320`）。
//!    本作照抄：不额外拒绝「已锁定」的放入/取回，而是移动后解锁双方——效果等价且不会
//!    出现「服务端锁着但客户端已解锁」的错位。
//! 5. **背包满时成交是「解锁」不是「中止」**：Crystal 在 `TradeConfirm` 里发现对方收不下
//!    物品/金币时，只给双方 `S.TradeCancel{Unlock = true}`（**不关窗、不退货**，
//!    `PlayerObject.cs:10829-10851`），客户端只解锁（`GameScene.cs:6323-6338`），玩家可以
//!    继续调整交易格。真正「退出并把东西退回」的是 `TradeCancel`。本作照抄。
//! 6. **取消时背包满 → 寄邮件**：`TradeCancel` 在 `FreeSpace(Inventory) < 1` 时走
//!    `GainItemMail(temp, 1)`（`PlayerObject.cs:10910-10918`），即 `Envir.MailCharacter`
//!    （`Envir.cs` 的 `MailCharacter`：发件人 `Bichon Administrator`，正文
//!    `You have been mailed due to the following reason: … Could not return item to bag after trade.`）。
//!    本作走 [`World::mail_item_to_self`](crate::mail::World::mail_item_to_self)，正文与发件人
//!    本地化成中文，其余（未受理包裹 `Collected=false`）一致。

use mir_core::Point;
use mir_core::TextKey;
use mir_core::enums::{BindMode, MirDirection};
use mir_protocol::ServerPacket;
use mir_protocol::data::UserItem;
use mir_protocol::packets::server::{
    DeleteItem, DepositTradeItem, LoseGold, RetrieveTradeItem, TradeAccept, TradeCancel,
    TradeConfirm, TradeGold, TradeItem, TradeRequest,
};

use crate::world::World;

/// 交易格数量：Crystal `Trade = new UserItem[10]`（`Server/MirDatabase/CharacterInfo.cs:75`）。
pub const TRADE_SLOTS: usize = 10;

/// 两次发起交易的间隔：`Settings.TradeDelay = 2000`（`Server/Settings.cs:337`）。
pub const TRADE_DELAY_MS: i64 = 2_000;

/// 交易距离：Crystal 用 `Globals.DataRange = 16`（`Shared/Globals.cs:37`，
/// `PlayerObject.cs:10692`、`:10803`）。
pub const TRADE_RANGE: i32 = 16;

/// 一个玩家的交易状态。
///
/// 对应 Crystal 分散在 `PlayerObject` / `CharacterInfo` 上的四个字段：
///
/// | 本结构 | Crystal |
/// |---|---|
/// | [`TradeState::partner`] | `PlayerObject.TradePartner`（`PlayerObject.cs:177`） |
/// | [`TradeState::locked`] | `PlayerObject.TradeLocked`（`:178`） |
/// | [`TradeState::gold`] | `PlayerObject.TradeGoldAmount`（`:179`，`uint`） |
/// | [`TradeState::items`] | `CharacterInfo.Trade`（`CharacterInfo.cs:75`，`UserItem[10]`） |
///
/// 交易状态**不落盘**（见模块文档差异 1）。
#[derive(Debug, Clone, PartialEq)]
pub struct TradeState {
    /// 交易对方的对象 ID（`None` = 没在交易）。
    pub partner: Option<u32>,
    /// 是否已经按下「确认」（`TradeLocked`）。
    pub locked: bool,
    /// 已放进去的金币累计额（`TradeGoldAmount`；放进去的金币先从角色金币里扣）。
    pub gold: u32,
    /// 交易格（`Info.Trade`，固定 [`TRADE_SLOTS`] 格）。
    pub items: Vec<Option<UserItem>>,
}

impl TradeState {
    /// 新建一个空交易状态（10 个空格）。
    #[must_use]
    pub fn new() -> Self {
        Self {
            partner: None,
            locked: false,
            gold: 0,
            items: vec![None; TRADE_SLOTS],
        }
    }

    /// 是否正在交易。
    #[must_use]
    pub fn is_trading(&self) -> bool {
        self.partner.is_some()
    }

    /// 交易格里放了几件物品。
    #[must_use]
    pub fn item_count(&self) -> usize {
        self.items.iter().flatten().count()
    }
}

impl Default for TradeState {
    fn default() -> Self {
        Self::new()
    }
}

/// 交易操作被拒绝的原因。
///
/// 每个变体都对应 Crystal 里一次 `ReceiveChat(...)` + `return`（提示文本已经由
/// [`World`] 的方法直接发出，会话层只用它写日志）。`Cooldown`、`NoInvitation`、
/// `InviterGone` 在 Crystal 里是**静默**返回（`PlayerObject.cs:10625`、`:10710-10714`）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TradeError {
    /// 对象不在世界里。
    #[error("对象 {0} 不在世界里")]
    NotInWorld(u32),
    /// 交易请求太频繁（`Envir.Time < NextTradeTime`，静默，`PlayerObject.cs:10625`）。
    #[error("交易请求太频繁")]
    Cooldown,
    /// 自己已经在交易中（`ServerTextKeys.AlreadyTrading`，`:10628-10632`）。
    #[error("你已经在交易中")]
    AlreadyTrading,
    /// 面朝格上没有可以交易的玩家（`ServerTextKeys.FaceToTrade`，`:10640-10658`）。
    #[error("请面对你要交易的对象")]
    NoTarget,
    /// 双方没有面对面（`ServerTextKeys.FaceToTrade`，`:10662-10666`）。
    #[error("交易双方必须面对面")]
    NotFacing,
    /// 不能和自己交易（`ServerTextKeys.CannotTradeWithSelf`，`:10668-10672`）。
    #[error("不能和自己交易")]
    SelfTrade,
    /// 自己或对方已经死亡（`ServerTextKeys.CannotTradeWhenDead`，`:10674-10678`）。
    #[error("死亡状态不能交易")]
    Dead,
    /// 对方已经收到别人的交易邀请（`PlayerAlreadyHasTradeInvitation`，`:10680-10684`）。
    #[error("{0} 已经收到了交易邀请")]
    TargetInvited(String),
    /// 对方关掉了「允许交易」（`PlayerNotAllowingTrade`，`:10686-10690`）。
    #[error("{0} 不允许交易")]
    TargetNotAllowingTrade(String),
    /// 对方不在同一张图，或超出 [`TRADE_RANGE`]（`PlayerNotInTradeRange`，`:10692-10696`、`:10803`）。
    #[error("{0} 不在交易范围内")]
    OutOfRange(String),
    /// 对方已经在交易中（`PlayerAlreadyTrading`，`:10698-10702`）。
    #[error("{0} 已经在交易中")]
    TargetTrading(String),
    /// 没有待接受的交易邀请（静默，`:10710-10714`）。
    #[error("你没有收到交易邀请")]
    NoInvitation,
    /// 邀请者已经下线（`TradeInvitation.Info == null`，静默，`:10710-10714`）。
    #[error("邀请者已经下线")]
    InviterGone,
    /// 没有交易对象（`TradePartner == null`，静默）。
    #[error("你没有在交易中")]
    NoPartner,
    /// 格子下标越界（`PlayerObject.cs:10534-10544`、`:10586-10596`）。
    #[error("格子下标 {0} 越界")]
    InvalidSlot(i32),
    /// 背包/交易格里没有这件物品（`:10546-10552`、`:10598-10604`）。
    #[error("格子里没有物品")]
    NoItem,
    /// 物品绑定「禁止交易」（`BindMode.DontTrade`，`:10554-10564`）。
    #[error("这件物品不能交易")]
    ItemBound,
    /// 目标交易格已经被占用（`:10566`）。
    #[error("目标交易格已经被占用")]
    SlotOccupied,
    /// 目标背包格已经被占用（`:10606`）。
    #[error("目标背包格已经被占用")]
    InventorySlotOccupied,
    /// 放入的金币小于 1（`:10750`）。
    #[error("放入的金币必须大于 0")]
    InvalidGold,
    /// 金币不够（`:10750`）。
    #[error("金币不够：需要 {requested}，只有 {held}")]
    NotEnoughGold {
        /// 想放进去的数量。
        requested: u32,
        /// 当前持有。
        held: i64,
    },
}

/// `C.TradeReply` 的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeReplyOutcome {
    /// 接受：双方进入交易状态（各自收到 `S.TradeAccept`）。
    Joined,
    /// 明确拒绝：邀请者收到系统提示。
    Declined,
}

// ---------------------------------------------------------------------------
// 几何小工具
// ---------------------------------------------------------------------------

/// `Functions.FacingEachOther`（`Shared/Functions/Functions.cs:76-83`）：
/// 双方都朝着对方所在的格子。
#[must_use]
pub fn facing_each_other(
    a_direction: MirDirection,
    a_position: Point,
    b_direction: MirDirection,
    b_position: Point,
) -> bool {
    a_direction == a_position.direction_to(b_position)
        && b_direction == b_position.direction_to(a_position)
}

// ---------------------------------------------------------------------------
// 世界层：交易
// ---------------------------------------------------------------------------

impl World {
    /// 当前交易状态（只读，测试/工具用）。
    #[must_use]
    pub fn trade_state(&self, object_id: u32) -> Option<&TradeState> {
        self.players.get(&object_id).map(|player| &player.trade)
    }

    /// 当前收到的交易邀请来自谁（Crystal `PlayerObject.TradeInvitation`）。
    #[must_use]
    pub fn trade_invitation_of(&self, object_id: u32) -> Option<u32> {
        self.players
            .get(&object_id)
            .and_then(|player| player.trade_invitation)
    }

    /// 是否允许别人向自己发起交易（Crystal `Info.AllowTrade`）。
    #[must_use]
    pub fn allow_trade_of(&self, object_id: u32) -> Option<bool> {
        self.players
            .get(&object_id)
            .map(|player| player.allow_trade)
    }

    /// 是否允许别人向自己发起交易：`C.ChangeTrade{AllowTrade}`（Crystal
    /// `MirConnection.cs:1448-1452` → `PlayerObject.AllowTrade = p.AllowTrade`）。
    ///
    /// 简化：Crystal 把它存在 `CharacterInfo.AllowTrade` 里（`CharacterInfo.cs:62`），
    /// 本作不改 `mir-store`，所以不持久化，进世界时默认允许（`true`）。
    pub fn set_allow_trade(&mut self, object_id: u32, allow: bool) -> bool {
        let Some(player) = self.players.get_mut(&object_id) else {
            return false;
        };
        player.allow_trade = allow;
        true
    }

    /// 发起交易（`C.TradeRequest`）：Crystal `PlayerObject.TradeRequest()`
    /// （`Server/MirObjects/PlayerObject.cs:10623-10707`）。
    ///
    /// 校验顺序照抄：
    /// 1. `Envir.Time < NextTradeTime` → **静默**返回（`:10625-10626`）；
    /// 2. 自己已经在交易中 → 聊天「你已经在交易中。」（`:10628-10632`）；
    /// 3. 面朝格（`Functions.PointMove(CurrentLocation, Direction, 1)`，`:10634`）里
    ///    必须有玩家，否则「请面对你要交易的对象。」（`:10636-10658`）；
    /// 4. 双方必须面对面（`:10662-10666`）；
    /// 5. 其余校验见 [`World::trade_request_target`]（`:10668-10706`）。
    ///
    /// 返回被邀请者的对象 ID。
    pub fn trade_request(&mut self, object_id: u32) -> Result<u32, TradeError> {
        let now = self.now();
        let Some(player) = self.players.get(&object_id) else {
            return Err(TradeError::NotInWorld(object_id));
        };
        // 1. `if (Envir.Time < NextTradeTime) return;`（`:10625`）
        if now < player.next_trade_time {
            return Err(TradeError::Cooldown);
        }
        let map = player.map.clone();
        let position = player.position;
        let direction = player.direction;
        let in_trade = player.trade.partner.is_some();
        // `NextTradeTime = Envir.Time + Settings.TradeDelay;`（`:10626`）
        if let Some(player) = self.players.get_mut(&object_id) {
            player.next_trade_time = now + TRADE_DELAY_MS;
        }
        // 2. `if (TradePartner != null)`（`:10628-10632`）
        if in_trade {
            self.send_system_chat(
                object_id,
                &self.text(object_id, TextKey::TradeAlreadyTrading),
            );
            return Err(TradeError::AlreadyTrading);
        }
        // 3. 面朝格（`:10634-10658`）。Crystal 在一格里可能有多个对象，
        //    循环里 `player = Envir.GetPlayer(ob.Name)` 取**最后一个**玩家；
        //    本作用对象 ID 最大者近似「最后放进去的那个」，保证可回放。
        let facing = position.adjacent(direction);
        let target = self
            .players
            .values()
            .filter(|other| {
                other.object_id != object_id && other.map == map && other.position == facing
            })
            .map(|other| other.object_id)
            .max();
        let Some(target_id) = target else {
            self.send_system_chat(object_id, &self.text(object_id, TextKey::TradeNotFacing));
            return Err(TradeError::NoTarget);
        };
        // 4. `Functions.FacingEachOther(...)`（`:10662-10666`）
        let target_direction = self.players[&target_id].direction;
        if !facing_each_other(direction, position, target_direction, facing) {
            self.send_system_chat(object_id, &self.text(object_id, TextKey::TradeNotFacing));
            return Err(TradeError::NotFacing);
        }
        // 5. 选人之后的校验（`:10668-10706`）
        self.trade_invite(object_id, target_id)?;
        Ok(target_id)
    }

    /// 按对象 ID 发起交易请求（**本作扩展**）。
    ///
    /// Crystal 的 `C.TradeRequest` 不带目标（`PlayerObject.cs:10623`），目标由服务器按
    /// 「面朝格」选出；本入口把「选人之后」的同一套校验单独暴露出来，供测试与以后
    /// 「点击玩家 → 交易」的客户端使用。校验顺序与 [`World::trade_request`] 完全一致：
    /// 先 [`TRADE_DELAY_MS`] 节流、再「自己是否在交易中」，然后从
    /// `PlayerObject.cs:10668` 的「不能和自己交易」开始。
    pub fn trade_request_target(
        &mut self,
        object_id: u32,
        target_id: u32,
    ) -> Result<(), TradeError> {
        let now = self.now();
        let Some(player) = self.players.get(&object_id) else {
            return Err(TradeError::NotInWorld(object_id));
        };
        if now < player.next_trade_time {
            return Err(TradeError::Cooldown);
        }
        let in_trade = player.trade.partner.is_some();
        if let Some(player) = self.players.get_mut(&object_id) {
            player.next_trade_time = now + TRADE_DELAY_MS;
        }
        if in_trade {
            self.send_system_chat(
                object_id,
                &self.text(object_id, TextKey::TradeAlreadyTrading),
            );
            return Err(TradeError::AlreadyTrading);
        }
        self.trade_invite(object_id, target_id)
    }

    /// 交易邀请的最后一段校验并给目标发 `S.TradeRequest`
    /// （Crystal `PlayerObject.cs:10668-10706`）。
    fn trade_invite(&mut self, object_id: u32, target_id: u32) -> Result<(), TradeError> {
        let Some(requester) = self.players.get(&object_id) else {
            return Err(TradeError::NotInWorld(object_id));
        };
        let requester_name = requester.name.clone();
        let requester_map = requester.map.clone();
        let requester_position = requester.position;
        let requester_dead = requester.dead;
        let Some(target) = self.players.get(&target_id) else {
            self.send_system_chat(object_id, &self.text(object_id, TextKey::TradeNotFacing));
            return Err(TradeError::NoTarget);
        };
        let target_name = target.name.clone();
        // 4. `if (player == this)`（`:10668-10672`）
        if target_id == object_id {
            self.send_system_chat(object_id, &self.text(object_id, TextKey::TradeSelf));
            return Err(TradeError::SelfTrade);
        }
        // 5. `if (player.Dead || Dead)`（`:10674-10678`）
        if target.dead || requester_dead {
            self.send_system_chat(object_id, &self.text(object_id, TextKey::TradeDead));
            return Err(TradeError::Dead);
        }
        // 6. `if (player.TradeInvitation != null)`（`:10680-10684`）
        if target.trade_invitation.is_some() {
            let message = self.format(
                object_id,
                TextKey::TradeTargetAlreadyInvited,
                &[("target_name", &target_name)],
            );
            self.send_system_chat(object_id, &message);
            return Err(TradeError::TargetInvited(target_name));
        }
        // 7. `if (!player.AllowTrade)`（`:10686-10690`）
        if !target.allow_trade {
            let message = self.format(
                object_id,
                TextKey::TradeTargetDisallowed,
                &[("target_name", &target_name)],
            );
            self.send_system_chat(object_id, &message);
            return Err(TradeError::TargetNotAllowingTrade(target_name));
        }
        // 8. 同图 + `Functions.InRange(..., Globals.DataRange)`（`:10692-10696`）
        if target.map != requester_map || !target.position.in_range(requester_position, TRADE_RANGE)
        {
            let message = self.format(
                object_id,
                TextKey::TradeTargetOutOfRange,
                &[("target_name", &target_name)],
            );
            self.send_system_chat(object_id, &message);
            return Err(TradeError::OutOfRange(target_name));
        }
        // 9. `if (player.TradePartner != null)`（`:10698-10702`）
        if target.trade.partner.is_some() {
            let message = self.format(
                object_id,
                TextKey::TradeTargetAlreadyTrading,
                &[("target_name", &target_name)],
            );
            self.send_system_chat(object_id, &message);
            return Err(TradeError::TargetTrading(target_name));
        }
        // `player.TradeInvitation = this; player.Enqueue(new S.TradeRequest { Name = Info.Name });`
        // （`:10704-10705`）
        if let Some(target) = self.players.get_mut(&target_id) {
            target.trade_invitation = Some(object_id);
        }
        self.send_to_player(
            target_id,
            &ServerPacket::TradeRequest(TradeRequest {
                name: requester_name,
            })
            .encode_frame(),
        );
        Ok(())
    }

    /// 回应交易邀请（`C.TradeReply`）：Crystal `PlayerObject.TradeReply(bool)`
    /// （`Server/MirObjects/PlayerObject.cs:10708-10743`）。
    pub fn trade_reply(
        &mut self,
        object_id: u32,
        accept: bool,
    ) -> Result<TradeReplyOutcome, TradeError> {
        let Some(invitee_name) = self.players.get(&object_id).map(|p| p.name.clone()) else {
            return Err(TradeError::NotInWorld(object_id));
        };
        // `if (TradeInvitation == null || TradeInvitation.Info == null) { TradeInvitation = null; return; }`
        // （`:10710-10714`）
        let Some(inviter_id) = self
            .players
            .get_mut(&object_id)
            .and_then(|player| player.trade_invitation.take())
        else {
            return Err(TradeError::NoInvitation);
        };
        let Some(inviter) = self.players.get(&inviter_id) else {
            return Err(TradeError::InviterGone);
        };
        let inviter_name = inviter.name.clone();
        if !accept {
            // `TradeInvitation.ReceiveChat(PlayerRefusedTrade…)`（`:10716-10721`）
            let message = self.format(
                inviter_id,
                TextKey::TradeDeclined,
                &[("invitee_name", &invitee_name)],
            );
            self.send_system_chat(inviter_id, &message);
            return Ok(TradeReplyOutcome::Declined);
        }
        // `if (TradePartner != null)`（`:10723-10728`）
        if self.players[&object_id].trade.partner.is_some() {
            self.send_system_chat(
                object_id,
                &self.text(object_id, TextKey::TradeAlreadyTrading),
            );
            return Err(TradeError::AlreadyTrading);
        }
        // `if (TradeInvitation.TradePartner != null)`（`:10730-10735`）
        if inviter.trade.partner.is_some() {
            let message = self.format(
                object_id,
                TextKey::TradeInviterAlreadyTrading,
                &[("inviter_name", &inviter_name)],
            );
            self.send_system_chat(object_id, &message);
            return Err(TradeError::TargetTrading(inviter_name));
        }
        // `TradePartner = TradeInvitation; TradeInvitation.TradePartner = this; TradeInvitation = null;`
        // （`:10737-10739`）
        if let Some(player) = self.players.get_mut(&object_id) {
            player.trade.partner = Some(inviter_id);
        }
        if let Some(player) = self.players.get_mut(&inviter_id) {
            player.trade.partner = Some(object_id);
        }
        // `Enqueue(new S.TradeAccept { Name = TradePartner.Info.Name });` 双方各一条（`:10741-10742`）
        self.send_to_player(
            object_id,
            &ServerPacket::TradeAccept(TradeAccept { name: inviter_name }).encode_frame(),
        );
        self.send_to_player(
            inviter_id,
            &ServerPacket::TradeAccept(TradeAccept { name: invitee_name }).encode_frame(),
        );
        Ok(TradeReplyOutcome::Joined)
    }

    /// 把背包物品放进交易格：`C.DepositTradeItem{From,To}`
    /// （Crystal `PlayerObject.DepositTradeItem`，`PlayerObject.cs:10530-10581`）。
    ///
    /// 顺序照抄：下标越界 → 物品不存在 → `BindMode.DontTrade` → 租借绑定
    /// （`RentalInformation.BindingFlags`）→ 目标格已占用，全部失败都回
    /// `S.DepositTradeItem{Success=false}`；成功则移动 → `TradeItem()`（解锁双方 +
    /// 给对方发 `S.TradeItem`）→ `S.DepositTradeItem{Success=true}`。
    ///
    /// 注意 Crystal **不检查** `TradeLocked`，而是靠 `TradeItem()` 里的 `TradeUnlock()`
    /// 解锁双方（见模块文档差异 4）。
    pub fn trade_deposit_item(
        &mut self,
        object_id: u32,
        from: i32,
        to: i32,
    ) -> Result<(), TradeError> {
        let Some(player) = self.players.get(&object_id) else {
            return Err(TradeError::NotInWorld(object_id));
        };
        // `if (from < 0 || from >= Info.Inventory.Length)` / `if (to < 0 || to >= Info.Trade.Length)`
        // （`:10534-10544`）
        if from < 0
            || usize::try_from(from).map_or(true, |from| from >= player.inventory.len())
            || to < 0
            || usize::try_from(to).map_or(true, |to| to >= player.trade.items.len())
        {
            self.send_deposit_ack(object_id, from, to, false);
            return Err(TradeError::InvalidSlot(if from < 0 { from } else { to }));
        }
        let from_slot = usize::try_from(from).unwrap_or(0);
        let to_slot = usize::try_from(to).unwrap_or(0);
        // `UserItem temp = Info.Inventory[from]; if (temp == null)`（`:10546-10552`）
        let Some(item) = player.inventory[from_slot].clone() else {
            self.send_deposit_ack(object_id, from, to, false);
            return Err(TradeError::NoItem);
        };
        // `if (temp.Info.Bind.HasFlag(BindMode.DontTrade))`（`:10554-10558`）
        let bound = self.item_info(item.item_index).is_none_or(|info| {
            info.bind.contains(BindMode::DontTrade)
                || item
                    .rental_information
                    .as_ref()
                    .is_some_and(|rental| rental.binding_flags.contains(BindMode::DontTrade))
        });
        if bound {
            self.send_deposit_ack(object_id, from, to, false);
            return Err(TradeError::ItemBound);
        }
        // `if (Info.Trade[to] == null)`（`:10566`）
        if player.trade.items[to_slot].is_some() {
            self.send_deposit_ack(object_id, from, to, false);
            return Err(TradeError::SlotOccupied);
        }
        if let Some(player) = self.players.get_mut(&object_id) {
            player.trade.items[to_slot] = Some(item);
            player.inventory[from_slot] = None;
        }
        // `TradeItem();`（`:10571`）
        self.trade_items_changed(object_id);
        // `p.Success = true; Enqueue(p);`（`:10575-10576`）
        self.send_deposit_ack(object_id, from, to, true);
        Ok(())
    }

    /// 从交易格取回物品：`C.RetrieveTradeItem{From,To}`
    /// （Crystal `PlayerObject.RetrieveTradeItem`，`PlayerObject.cs:10582-10619`）。
    ///
    /// 顺序：下标越界 → 交易格为空 → 目标背包格已占用 → 失败回
    /// `S.RetrieveTradeItem{Success=false}`；成功则移动 → `TradeItem()`（先给对方发
    /// `S.TradeItem`）→ 本人收 `S.RetrieveTradeItem{Success=true}`。
    pub fn trade_retrieve_item(
        &mut self,
        object_id: u32,
        from: i32,
        to: i32,
    ) -> Result<(), TradeError> {
        let Some(player) = self.players.get(&object_id) else {
            return Err(TradeError::NotInWorld(object_id));
        };
        // `if (from < 0 || from >= Info.Trade.Length)` / `if (to < 0 || to >= Info.Inventory.Length)`
        // （`:10586-10596`）
        if from < 0
            || usize::try_from(from).map_or(true, |from| from >= player.trade.items.len())
            || to < 0
            || usize::try_from(to).map_or(true, |to| to >= player.inventory.len())
        {
            self.send_retrieve_ack(object_id, from, to, false);
            return Err(TradeError::InvalidSlot(if from < 0 { from } else { to }));
        }
        let from_slot = usize::try_from(from).unwrap_or(0);
        let to_slot = usize::try_from(to).unwrap_or(0);
        // `UserItem temp = Info.Trade[from]; if (temp == null)`（`:10598-10604`）
        let Some(item) = player.trade.items[from_slot].clone() else {
            self.send_retrieve_ack(object_id, from, to, false);
            return Err(TradeError::NoItem);
        };
        // `if (Info.Inventory[to] == null)`（`:10606`）
        if player.inventory[to_slot].is_some() {
            self.send_retrieve_ack(object_id, from, to, false);
            return Err(TradeError::InventorySlotOccupied);
        }
        if let Some(player) = self.players.get_mut(&object_id) {
            player.inventory[to_slot] = Some(item);
            player.trade.items[from_slot] = None;
        }
        // `p.Success = true; RefreshBagWeight(); TradeItem(); Report...; Enqueue(p);`（`:10611-10618`）
        self.trade_items_changed(object_id);
        self.send_retrieve_ack(object_id, from, to, true);
        Ok(())
    }

    /// 往交易里放金币：`C.TradeGold{Amount}`（Crystal `PlayerObject.TradeGold(uint)`，
    /// `PlayerObject.cs:10744-10760`）。
    ///
    /// 注意两点：
    /// * `TradeUnlock()` 在**最前面**（`:10746`）——即使后面因为金额非法/金币不够而
    ///   `return`，双方的「确认」也已经被清掉；
    /// * 金币是**累加**放进交易（`TradeGoldAmount += amount`），先从角色金币里扣，
    ///   本人收 `S.LoseGold`，对方收 `S.TradeGold{Amount=累计额}`。协议里没有「取回金币」，
    ///   取回只能靠取消交易（`TradeCancel` 会 `GainGold` 退回）。
    pub fn trade_gold(&mut self, object_id: u32, amount: u32) -> Result<(), TradeError> {
        // `TradeUnlock();`（`:10746`）
        self.trade_unlock(object_id);
        // `if (TradePartner == null) return;`（`:10748`）
        let Some(partner_id) = self.trade_partner(object_id) else {
            return Err(TradeError::NoPartner);
        };
        let Some(player) = self.players.get(&object_id) else {
            return Err(TradeError::NotInWorld(object_id));
        };
        // `if (amount < 1 || Account.Gold < amount) return;`（`:10750-10753`）
        if amount < 1 {
            return Err(TradeError::InvalidGold);
        }
        if player.gold < i64::from(amount) {
            return Err(TradeError::NotEnoughGold {
                requested: amount,
                held: player.gold,
            });
        }
        // `TradeGoldAmount += amount; Account.Gold -= amount;`（`:10755-10756`）
        let total = player.trade.gold.saturating_add(amount);
        if let Some(player) = self.players.get_mut(&object_id) {
            player.trade.gold = total;
            player.gold -= i64::from(amount);
        }
        // `Enqueue(new S.LoseGold { Gold = amount });`（`:10758`）
        self.send_to_player(
            object_id,
            &ServerPacket::LoseGold(LoseGold { gold: amount }).encode_frame(),
        );
        // `TradePartner.Enqueue(new S.TradeGold { Amount = TradeGoldAmount });`（`:10759`）
        self.send_to_player(
            partner_id,
            &ServerPacket::TradeGold(TradeGold { amount: total }).encode_frame(),
        );
        Ok(())
    }

    /// 确认交易：`C.TradeConfirm{Locked}`（Crystal `PlayerObject.TradeConfirm(bool)`，
    /// `PlayerObject.cs:10789-10888`）。
    ///
    /// 两次确认语义：
    /// 1. `confirm == false` 只清自己的「确认」（`:10791-10795`）；
    /// 2. `confirm == true` 时若对方不在（`:10797-10801`）、不在范围/不同图/没面对面
    ///    （`:10803-10808`）→ 直接 [`World::trade_cancel`]（真·中止并退货）；
    /// 3. 自己锁定；自己锁了对方没锁 → 给对方一句「某某正在等待你确认交易。」（`:10810-10815`）；
    /// 4. 双方都锁了才结算：先双向检查「对方能不能收下我的物品/金币」
    ///    （`:10824-10852`），失败只发 `S.TradeCancel{Unlock=true}` 并解锁（**不关窗、不退货**）；
    /// 5. 通过则交换：对方 `GainItem`（`S.GainedItem`）/`GainGold`（`S.GainedGold`），
    ///    本人 `S.Chat`「交易成功」→ `S.TradeConfirm`，双方清空交易状态（`:10855-10887`）。
    pub fn trade_confirm(&mut self, object_id: u32, confirm: bool) -> Result<(), TradeError> {
        let Some(player) = self.players.get(&object_id) else {
            return Err(TradeError::NotInWorld(object_id));
        };
        // `if (!confirm) { TradeLocked = false; return; }`（`:10791-10795`）
        if !confirm {
            if let Some(player) = self.players.get_mut(&object_id) {
                player.trade.locked = false;
            }
            return Ok(());
        }
        // `if (TradePartner == null) { TradeCancel(); return; }`（`:10797-10801`）
        let Some(partner_id) = player.trade.partner else {
            self.trade_cancel(object_id);
            return Err(TradeError::NoPartner);
        };
        let partner_name = self.players[&partner_id].name.clone();
        // `if (!Functions.InRange(...) || TradePartner.CurrentMap != CurrentMap || !Functions.FacingEachOther(...))`
        // → `TradeCancel(); return;`（`:10803-10808`）
        if !self.trade_pair_valid(object_id, partner_id) {
            self.trade_cancel(object_id);
            return Err(TradeError::OutOfRange(partner_name));
        }
        let Some(name) = self
            .players
            .get(&object_id)
            .map(|player| player.name.clone())
        else {
            return Err(TradeError::NotInWorld(object_id));
        };
        // `TradeLocked = true;`（`:10810`）
        let partner_locked = if let Some(player) = self.players.get_mut(&object_id) {
            player.trade.locked = true;
            self.players[&partner_id].trade.locked
        } else {
            return Err(TradeError::NotInWorld(object_id));
        };
        // `if (TradeLocked && !TradePartner.TradeLocked) partner.ReceiveChat(PlayerWaitingConfirmTrade…)`
        // （`:10812-10815`）——只有「我刚锁、对方还没锁」才提示**对方**。
        if !partner_locked {
            let message = self.format(partner_id, TextKey::TradePartnerWaiting, &[("name", &name)]);
            self.send_system_chat(partner_id, &message);
            return Ok(());
        }
        // 双方都锁了：先双向检查能不能收下（`:10824-10852`）
        for (receiver, giver) in [(partner_id, object_id), (object_id, partner_id)] {
            let Some(giver_state) = self.players.get(&giver).map(|p| p.trade.clone()) else {
                return Err(TradeError::NotInWorld(giver));
            };
            if !self.trade_can_gain_items(receiver, &giver_state.items) {
                self.trade_reject_swap(giver, receiver, false);
                return Ok(());
            }
            if !self.can_gain_gold(receiver, giver_state.gold) {
                self.trade_reject_swap(giver, receiver, true);
                return Ok(());
            }
        }
        // 交换（`:10855-10887`）：先 partner → 再自己
        for giver in [partner_id, object_id] {
            let receiver = if giver == partner_id {
                object_id
            } else {
                partner_id
            };
            let Some(state) = self.players.get(&giver).map(|p| p.trade.clone()) else {
                return Err(TradeError::NotInWorld(giver));
            };
            for (slot, item) in state.items.iter().enumerate() {
                let Some(item) = item else { continue };
                // `TradePair[o].GainItem(u);`（`:10867`）——先给对方发 `S.GainedItem` 再入包。
                self.gain_item(receiver, item.clone());
                if let Some(player) = self.players.get_mut(&giver) {
                    if let Some(target) = player.trade.items.get_mut(slot) {
                        *target = None;
                    }
                }
            }
            if state.gold > 0 {
                self.gain_gold(receiver, state.gold);
                if let Some(player) = self.players.get_mut(&giver) {
                    player.trade.gold = 0;
                }
            }
            // `TradePair[p].ReceiveChat(TradeSuccessful…); TradePair[p].Enqueue(new S.TradeConfirm());`
            // （`:10881-10882`）
            self.send_system_chat(giver, &self.text(giver, TextKey::TradeSuccess));
            self.send_to_player(
                giver,
                &ServerPacket::TradeConfirm(TradeConfirm {}).encode_frame(),
            );
            // `TradePair[p].TradeLocked = false; TradePair[p].TradePartner = null;`（`:10884-10885`）
            if let Some(player) = self.players.get_mut(&giver) {
                player.trade.locked = false;
                player.trade.partner = None;
            }
        }
        Ok(())
    }

    /// 取消交易：`C.TradeCancel`（Crystal `PlayerObject.TradeCancel()`，
    /// `PlayerObject.cs:10889-10958`）。
    ///
    /// 先把**双方**的交易格物品逐件放回背包（背包满则寄邮件，见模块文档差异 6），
    /// 再退回金币，最后 `S.TradeCancel{Unlock=false}`（客户端会关掉交易窗口）。
    ///
    /// 也用于「移动/转身/切图/下线/死亡」等中止点（Crystal 的挂点见模块文档）。
    /// 没有交易对象时只解锁、什么也不发。
    pub fn trade_cancel(&mut self, object_id: u32) {
        // `TradeUnlock();`（`:10891`）
        self.trade_unlock(object_id);
        // `if (TradePartner == null) return;`（`:10893-10896`）
        let Some(partner_id) = self.trade_partner(object_id) else {
            return;
        };
        for side in [partner_id, object_id] {
            // `for (int t = 0; t < Info.Trade.Length; t++)`（`:10904-10941`）
            for slot in 0..TRADE_SLOTS {
                let Some(item) = self
                    .players
                    .get(&side)
                    .and_then(|player| player.trade.items.get(slot))
                    .and_then(Option::as_ref)
                    .cloned()
                else {
                    continue;
                };
                // `if (FreeSpace(Info.Inventory) < 1) { GainItemMail(temp, 1); … }`（`:10910-10918`）
                if self.free_space(side) < 1 {
                    self.trade_mail_back(side, slot, &item);
                    continue;
                }
                // 找第一个空背包格（`:10920-10923`）
                let Some(target_slot) = self
                    .players
                    .get(&side)
                    .and_then(|player| player.inventory.iter().position(Option::is_none))
                else {
                    self.trade_mail_back(side, slot, &item);
                    continue;
                };
                // `TradePair[p].RetrieveTradeItem(t, i);`（`:10927`）——
                // 这里直接内联那一步：移动 + `TradeItem()` + `S.RetrieveTradeItem`。
                if let Some(player) = self.players.get_mut(&side) {
                    player.inventory[target_slot] = Some(item);
                    if let Some(cell) = player.trade.items.get_mut(slot) {
                        *cell = None;
                    }
                }
                // `RetrieveTradeItem` 内部的 `TradeItem()`（`:10613`）会给**对方**发
                // `S.TradeItem`（此时对方的 `TradePartner` 还没清空）。
                self.trade_items_changed(side);
                self.send_retrieve_ack(side, slot as i32, target_slot as i32, true);
            }
            // 退回金币（`:10943-10950`）
            let gold = self
                .players
                .get(&side)
                .map_or(0, |player| player.trade.gold);
            if gold > 0 {
                self.gain_gold(side, gold);
                if let Some(player) = self.players.get_mut(&side) {
                    player.trade.gold = 0;
                }
            }
            // `TradeLocked = false; TradePartner = null; Enqueue(new S.TradeCancel { Unlock = false });`
            // （`:10952-10955`）
            if let Some(player) = self.players.get_mut(&side) {
                player.trade.locked = false;
                player.trade.partner = None;
            }
            self.send_to_player(
                side,
                &ServerPacket::TradeCancel(TradeCancel { unlock: false }).encode_frame(),
            );
        }
    }

    /// 交易对方（`TradePartner`）。
    #[must_use]
    pub fn trade_partner(&self, object_id: u32) -> Option<u32> {
        self.players.get(&object_id)?.trade.partner
    }

    /// `TradeUnlock()`：把自己和对方的「确认」都清掉（`PlayerObject.cs:10779-10787`）。
    pub fn trade_unlock(&mut self, object_id: u32) {
        let partner = self.trade_partner(object_id);
        if let Some(player) = self.players.get_mut(&object_id) {
            player.trade.locked = false;
        }
        if let Some(partner) = partner {
            if let Some(player) = self.players.get_mut(&partner) {
                player.trade.locked = false;
            }
        }
    }

    /// `TradeItem()`：解锁双方，并把**自己的**交易格整份发给对方
    /// （`PlayerObject.cs:10761-10777`）。
    ///
    /// Crystal 先 `TradeUnlock()` 再判 `TradePartner == null`，所以没有交易对象时
    /// 也会解锁（这是故意的：放物品进交易格本身就会重置「确认」）。
    fn trade_items_changed(&mut self, object_id: u32) {
        self.trade_unlock(object_id);
        let Some(partner_id) = self.trade_partner(object_id) else {
            return;
        };
        let Some(items) = self
            .players
            .get(&object_id)
            .map(|player| player.trade.items.clone())
        else {
            return;
        };
        self.send_to_player(
            partner_id,
            &ServerPacket::TradeItem(TradeItem { trade_items: items }).encode_frame(),
        );
    }

    /// 成交前「对方能不能收下这些物品」：Crystal `HumanObject.CanGainItems`
    /// （`HumanObject.cs:7629-7660`）。
    ///
    /// 逐行照抄：非空件数 `itemCount`；可堆叠物品只检查背包里**第一件**同模板物品，
    /// 若加上去会超过 `StackSize` 就让 `stackOffset++`；最后要求
    /// `FreeSpace >= itemCount + stackOffset`。
    #[must_use]
    pub fn trade_can_gain_items(&self, object_id: u32, items: &[Option<UserItem>]) -> bool {
        let Some(player) = self.players.get(&object_id) else {
            return false;
        };
        let item_count = items.iter().flatten().count();
        if item_count < 1 {
            return true;
        }
        let mut stack_offset = 0usize;
        for item in items.iter().flatten() {
            let Some(info) = self.item_info(item.item_index) else {
                continue;
            };
            if info.stack_size > 1 {
                let first = player
                    .inventory
                    .iter()
                    .flatten()
                    .find(|bag| bag.item_index == info.index);
                if let Some(bag) = first {
                    if u32::from(bag.count) + u32::from(item.count) > u32::from(info.stack_size) {
                        stack_offset += 1;
                    }
                }
            }
        }
        let free = player
            .inventory
            .iter()
            .filter(|slot| slot.is_none())
            .count();
        free >= item_count + stack_offset
    }

    /// 成交前「对方能不能收下这些金币」：Crystal `PlayerObject.CanGainGold`
    /// （`PlayerObject.cs:7638-7641`）：`(ulong)gold + Account.Gold <= uint.MaxValue`。
    #[must_use]
    pub fn can_gain_gold(&self, object_id: u32, gold: u32) -> bool {
        self.players.get(&object_id).is_some_and(|player| {
            // `(ulong)gold + Account.Gold <= uint.MaxValue`
            let held = u64::try_from(player.gold.max(0)).unwrap_or(0);
            u32::try_from(u64::from(gold).saturating_add(held)).is_ok()
        })
    }

    /// 交易双方是否仍然满足「同图 + 在 [`TRADE_RANGE`] 内 + 面对面」
    /// （Crystal `TradeConfirm` 的距离检查，`PlayerObject.cs:10803-10804`）。
    #[must_use]
    pub fn trade_pair_valid(&self, a: u32, b: u32) -> bool {
        let (Some(a), Some(b)) = (self.players.get(&a), self.players.get(&b)) else {
            return false;
        };
        a.map == b.map
            && a.position.in_range(b.position, TRADE_RANGE)
            && facing_each_other(a.direction, a.position, b.direction, b.position)
    }

    /// 成交检查失败：双方各一句提示 + `S.TradeCancel{Unlock = true}`（不关窗）
    /// （Crystal `PlayerObject.cs:10829-10851`）。
    fn trade_reject_swap(&mut self, giver: u32, receiver: u32, gold: bool) {
        let (giver_key, receiver_key) = if gold {
            (
                TextKey::TradePartnerGoldOverflow,
                TextKey::TradeGoldOverflow,
            )
        } else {
            (
                TextKey::TradePartnerItemsOverflow,
                TextKey::TradeItemsOverflow,
            )
        };
        let giver_text = self.text(giver, giver_key);
        let receiver_text = self.text(receiver, receiver_key);
        self.send_system_chat(giver, &giver_text);
        self.send_system_chat(receiver, &receiver_text);
        let frame = ServerPacket::TradeCancel(TradeCancel { unlock: true }).encode_frame();
        self.send_to_player(giver, &frame);
        self.send_to_player(receiver, &frame);
    }

    /// 取消交易时把物品寄给自己（Crystal `GainItemMail` → `Envir.MailCharacter`，
    /// 见模块文档差异 6），并给本人发 `S.DeleteItem`（从交易格里删掉）。
    fn trade_mail_back(&mut self, player_id: u32, slot: usize, item: &UserItem) {
        if let Some(player) = self.players.get_mut(&player_id) {
            if let Some(cell) = player.trade.items.get_mut(slot) {
                *cell = None;
            }
        }
        self.mail_item_to_self(player_id, item.clone());
        self.send_to_player(
            player_id,
            &ServerPacket::DeleteItem(DeleteItem {
                unique_id: item.unique_id,
                count: item.count,
            })
            .encode_frame(),
        );
    }

    fn send_deposit_ack(&self, object_id: u32, from: i32, to: i32, success: bool) {
        self.send_to_player(
            object_id,
            &ServerPacket::DepositTradeItem(DepositTradeItem { from, to, success }).encode_frame(),
        );
    }

    fn send_retrieve_ack(&self, object_id: u32, from: i32, to: i32, success: bool) {
        self.send_to_player(
            object_id,
            &ServerPacket::RetrieveTradeItem(RetrieveTradeItem { from, to, success })
                .encode_frame(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facing_each_other_matches_crystal() {
        let left = Point::new(10, 10);
        let right = Point::new(11, 10);
        assert!(facing_each_other(
            MirDirection::Right,
            left,
            MirDirection::Left,
            right
        ));
        assert!(!facing_each_other(
            MirDirection::Right,
            left,
            MirDirection::Right,
            right
        ));
    }

    #[test]
    fn trade_state_starts_with_ten_empty_slots() {
        let state = TradeState::default();
        assert_eq!(state.items.len(), TRADE_SLOTS);
        assert!(!state.is_trading());
        assert_eq!(state.item_count(), 0);
    }

    #[test]
    fn gold_packet_carries_the_running_total() {
        // 只是文档化：`TradeGoldAmount += amount`（PlayerObject.cs:10755）。
        let mut state = TradeState::default();
        state.gold = state.gold.saturating_add(5_000);
        state.gold = state.gold.saturating_add(2_500);
        assert_eq!(state.gold, 7_500);
    }
}
