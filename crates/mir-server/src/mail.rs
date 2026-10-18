//! 邮件（邮箱）：发送、阅读、包裹受理、领取附件、锁定/删除、过期。
//!
//! 逐条对照 Crystal（参考根目录 `github\Crystal`，下文只写相对路径）：
//!
//! | 本模块 | Crystal |
//! |---|---|
//! | [`MAIL_TEXT_MAX`] | `if (message.Length > 500)`（`Server/MirObjects/PlayerObject.cs:11684`、`:11737`） |
//! | [`MAIL_RATE_MS`] | `NextMailTime = Envir.Time + 10000`（`:11682`、`:11735`） |
//! | [`MAIL_RECIPIENT_LIMIT`] | `if (player.Mail.Count > 50)`（`:11698`、`:11751`） |
//! | [`MAIL_CAPACITY`] | `Settings.MailCapacity = 100`（`Server/Settings.cs:247`），`GetMail` 只取最后 100 条（`:11963`） |
//! | [`MAIL_MAX_ATTACHMENTS`] | `ulong[] ItemsIdx = new ulong[5]`（`Shared/ClientPackets.cs:2129`） |
//! | [`MAIL_ATTACHMENTS_WITHOUT_STAMP`] | `for (int j = 0; j < (hasStamp ? 5 : 1); j++)`（`PlayerObject.cs:11790`） |
//! | [`MAIL_COST_PER_1K_GOLD`] | `Settings.MailCostPer1KGold = 100`（`Settings.cs:245`） |
//! | [`MAIL_ITEM_INSURANCE_PERCENT`] | `Settings.MailItemInsurancePercentage = 5`（`Settings.cs:246`） |
//! | [`MAIL_FREE_WITH_STAMP`] | `Settings.MailFreeWithStamp = true`（`Settings.cs:244`） |
//! | [`MAIL_AUTO_SEND_GOLD`] / [`MAIL_AUTO_SEND_ITEMS`] | `Settings.MailAutoSendGold/Items = false`（`Settings.cs:242-243`） |
//! | [`World::mail_send`] | `PlayerObject.SendMail(name, message, gold, items, stamped)`（`:11727-11845`） |
//! | [`World::mail_read`] | `PlayerObject.ReadMail`（`:11847-11856`） |
//! | [`World::mail_collect`] | `PlayerObject.CollectMail`（`:11858-11902`） |
//! | [`World::mail_delete`] | `PlayerObject.DeleteMail`（`:11904-11913`） |
//! | [`World::mail_lock`] | `PlayerObject.LockMail`（`:11915-11924`） |
//! | [`World::mail_cost`] | `PlayerObject.GetMailCost`（`:11926-11957`） |
//! | [`World::mail_list_packet`] | `PlayerObject.GetMail`（`:11959-11988`） |
//! | [`World::mail_collect_parcel_page`] | `NPCScript.cs` 的 `case CollectParcelKey:`（`:1081-1098`） |
//! | [`World::mail_open_send_page`] | `NPCScript.cs` 的 `case SendParcelKey:`（`:1078-1080`） |
//! | [`MailRecord`] | `Server/MirDatabase/MailInfo.cs:5-167`（落 `mail` 表，见 `migrations/0007_mail.sql`） |
//!
//! # 包序
//!
//! | 语义 | 客户端包 | 服务端包 | Crystal |
//! |---|---|---|---|
//! | 打开写信界面 | `C.CallNPC{Key="[@SENDPARCEL]"}` | `S.MailSendRequest`（233，空包） | `NPCScript.cs:1078-1080` |
//! | 询价 | `C.MailCost{Gold, ItemsIdx[5], Stamped}`（123） | `S.MailCost{Cost}`（236） | `MirConnection.cs:2045-2052` |
//! | 发送 | `C.SendMail{Name, Message, Gold, ItemsIdx[5], Stamped}`（117） | 逐件 `S.DeleteItem`（79）→ `S.LoseGold`（68，邮费非 0 时）→ `S.MailSent{Result}`（234） | `PlayerObject.cs:11727-11845` |
//! | 阅读 | `C.ReadMail{MailID}`（118） | `S.ReceiveMail`（231，整个列表） | `:11847-11856` |
//! | 邮局受理包裹 | `C.CallNPC{Key="[@COLLECTPARCEL]"}` | `S.ParcelCollected{Result}`（235）→ `S.ReceiveMail` | `NPCScript.cs:1081-1098` |
//! | 领取附件 | `C.CollectParcel{MailID}`（119） | 逐件 `S.GainedItem`（66）→ `S.GainedGold`（67）→ `S.ParcelCollected{Result=1}` → `S.ReceiveMail` | `PlayerObject.cs:11858-11902` |
//! | 锁定/解锁 | `C.LockMail{MailID, Lock}`（121） | `S.ReceiveMail` | `:11915-11924` |
//! | 删除 | `C.DeleteMail{MailID}`（120） | `S.ReceiveMail` | `:11904-11913` |
//! | 锁定附件格子 | `C.MailLockedItem{UniqueID, Locked}`（122） | `S.MailLockedItem`（232，原样回显） | `MirConnection.cs:667-669` |
//!
//! 登录时 `S.ReceiveMail` 紧跟任务包（Crystal `PlayerObject.cs:1190` 的 `GetMail()` 在
//! `GetCompletedQuests()` 之后、`GetFriends()` 之前）。
//!
//! # 与 Crystal 的**有意**差异
//!
//! 1. **多维护一个 `doc/邮件` 表**：Crystal 把 `List<MailInfo>` 塞在 `CharacterInfo` 的
//!    整块存档里（`CharacterInfo.cs:500-503`），本作拆成 `mail` 表。
//! 2. **`subject` 字段**：Crystal 没有主题（`C.SendMail` 只有收件人与正文），本作保留
//!    一列恒为空串，见 `migrations/0007_mail.sql`。
//! 3. **过期**：[`MAIL_EXPIRE_DAYS`] 是本作扩展——Crystal **没有**邮件过期
//!    （`Settings.cs:241-247` 没有相关常量，`MailInfo` 也没有过期字段）。
//! 4. **删除保护**：Crystal 的 `DeleteMail` 不检查 `Locked`、也不检查附件是否已领取
//!    （`:11904-11913`），客户端负责拦。本作在服务端补两道保护（[`MailError::Locked`]、
//!    [`MailError::UncollectedAttachments`]），见 `docs/world/mail.md`。
//! 5. **好友黑名单**：Crystal 发送前会检查双方好友里的 `Blocked`（`:11704-11714`）；
//!    本作没有好友系统，跳过。
//! 6. **金币在角色上**：Crystal 的 `Account.Gold` 挂在账号上，本作在角色行（`characters.gold`）。
//! 7. **系统邮件没做**：`Envir.MailCharacter`（拍卖/租借/GM 补发，`:13900-13910`、
//!    `Envir.cs:5069` 一带）不在本次范围，`can_reply` 因此恒为真。

use mir_core::DateTimeBinary;
use mir_core::enums::{BindMode, ItemType};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ClientMail, UserItem};
use mir_protocol::packets::server::{
    DeleteItem, GainedItem, LoseGold, MailCost as MailCostPacket, MailLockedItem, MailSendRequest,
    MailSent, ParcelCollected, ReceiveMail,
};

use crate::items::{ItemCodecError, decode_slots, encode_slots};
use crate::store::{AccountStore, MailRecord, StoreError};
use crate::world::World;

/// 邮件正文字符上限：Crystal `if (message.Length > 500)`（`PlayerObject.cs:11684`、`:11737`）。
///
/// C# 的 `string.Length` 是 UTF-16 码元数，本作用 `encode_utf16().count()` 对齐。
pub const MAIL_TEXT_MAX: usize = 500;

/// 发送邮件的最小间隔（毫秒）：Crystal `NextMailTime = Envir.Time + 10000`
/// （`PlayerObject.cs:11682`、`:11735`）。
pub const MAIL_RATE_MS: i64 = 10_000;

/// 收件人邮箱上限：Crystal `if (player.Mail.Count > 50)`（`PlayerObject.cs:11698`、`:11751`）。
///
/// 注意判定是 `> 50`：第 51 封起才拒绝。
pub const MAIL_RECIPIENT_LIMIT: usize = 50;

/// 一次下发多少封邮件：`Settings.MailCapacity = 100`（`Server/Settings.cs:247`），
/// `GetMail` 从末尾取 100 条（`PlayerObject.cs:11963`）。
pub const MAIL_CAPACITY: usize = 100;

/// 附件槽数：`C.SendMail.ItemsIdx = new ulong[5]`（`Shared/ClientPackets.cs:2129`）。
pub const MAIL_MAX_ATTACHMENTS: usize = 5;

/// 没贴邮票时只认第 1 个附件：`for (int j = 0; j < (hasStamp ? 5 : 1); j++)`
/// （`PlayerObject.cs:11790`）。
pub const MAIL_ATTACHMENTS_WITHOUT_STAMP: usize = 1;

/// 每 1000 金币的邮费：`Settings.MailCostPer1KGold = 100`（`Settings.cs:245`）。
pub const MAIL_COST_PER_1K_GOLD: u32 = 100;

/// 每件附件的保险费百分比：`Settings.MailItemInsurancePercentage = 5`（`Settings.cs:246`）。
pub const MAIL_ITEM_INSURANCE_PERCENT: u32 = 5;

/// 贴邮票是否免邮费：`Settings.MailFreeWithStamp = true`（`Settings.cs:244`）。
pub const MAIL_FREE_WITH_STAMP: bool = true;

/// 包裹里的金币是否自动「受理」：`Settings.MailAutoSendGold = false`（`Settings.cs:242`）。
pub const MAIL_AUTO_SEND_GOLD: bool = false;

/// 包裹里的物品是否自动「受理」：`Settings.MailAutoSendItems = false`（`Settings.cs:243`）。
pub const MAIL_AUTO_SEND_ITEMS: bool = false;

/// 邮件保留天数（**本作扩展**）。
///
/// `// 扩展：` Crystal 没有邮件过期机制（见模块文档差异 3）；本作按任务要求加一个
/// 保守的 30 天上限，查询时过滤并顺手删除。改这个常量即可调整。
pub const MAIL_EXPIRE_DAYS: i64 = 30;

/// 邮件保留秒数。
pub const MAIL_EXPIRE_SECONDS: i64 = MAIL_EXPIRE_DAYS * 24 * 60 * 60;

/// 一封邮件（服务端运行期视图；Crystal `MailInfo`）。
#[derive(Debug, Clone, PartialEq)]
pub struct Mail {
    /// Crystal `MailInfo.MailID`。
    pub id: u64,
    /// 收件角色主键（Crystal `MailInfo.RecipientIndex`）。
    pub character_id: i64,
    /// 发件人角色名（Crystal `MailInfo.Sender`）。
    pub sender_name: String,
    /// 主题（**本作扩展**，恒为空串）。
    pub subject: String,
    /// 正文（Crystal `MailInfo.Message`）。
    pub message: String,
    /// 附件物品（Crystal `MailInfo.Items`）。
    pub items: Vec<UserItem>,
    /// 附件金币（Crystal `MailInfo.Gold`）。
    pub gold: u32,
    /// 发送时间（Crystal `MailInfo.DateSent`）。
    pub date_sent: DateTimeBinary,
    /// 是否已读（Crystal `MailInfo.Opened`）。
    pub opened: bool,
    /// 是否锁定（Crystal `MailInfo.Locked`）。
    pub locked: bool,
    /// 包裹是否已被邮局受理（Crystal `MailInfo.Collected`）。
    pub collected: bool,
}

impl Mail {
    /// 是否是「包裹」：带金币或带物品（Crystal `MailInfo.Parcel`，`MailInfo.cs:39-42`）。
    #[must_use]
    pub fn is_parcel(&self) -> bool {
        self.gold > 0 || !self.items.is_empty()
    }

    /// 是否已经过期（**本作扩展**，见 [`MAIL_EXPIRE_DAYS`]）。
    #[must_use]
    pub fn is_expired(&self, now_unix: i64) -> bool {
        now_unix.saturating_sub(self.date_sent.to_unix_seconds()) > MAIL_EXPIRE_SECONDS
    }

    /// 转成存储视图（附件编码成 `UserItem[]` 字节块）。
    #[must_use]
    pub fn to_record(&self) -> MailRecord {
        let slots: Vec<Option<UserItem>> = self.items.iter().cloned().map(Some).collect();
        MailRecord {
            id: self.id,
            character_id: self.character_id,
            sender_name: self.sender_name.clone(),
            subject: self.subject.clone(),
            message: self.message.clone(),
            items: encode_slots(&slots),
            gold: self.gold,
            date_sent: self.date_sent,
            opened: self.opened,
            locked: self.locked,
            collected: self.collected,
        }
    }

    /// 从存储视图还原（附件字节块 → `Vec<UserItem>`）。
    ///
    /// # Errors
    /// 附件字节块损坏时返回 [`ItemCodecError`]。
    pub fn from_record(record: &MailRecord) -> Result<Self, ItemCodecError> {
        let items = if record.items.is_empty() {
            Vec::new()
        } else {
            decode_slots(&record.items)?.into_iter().flatten().collect()
        };
        Ok(Self {
            id: record.id,
            character_id: record.character_id,
            sender_name: record.sender_name.clone(),
            subject: record.subject.clone(),
            message: record.message.clone(),
            items,
            gold: record.gold,
            date_sent: record.date_sent,
            opened: record.opened,
            locked: record.locked,
            collected: record.collected,
        })
    }

    /// 转成线上格式（`S.ReceiveMail` 的一条；Crystal `MailInfo.CreateClientMail`，`MailInfo.cs:145-160`）。
    #[must_use]
    pub fn to_client_mail(&self) -> ClientMail {
        ClientMail {
            mail_id: self.id,
            sender_name: self.sender_name.clone(),
            message: self.message.clone(),
            opened: self.opened,
            locked: self.locked,
            // 只有玩家之间发送的邮件（`new MailInfo(index, true)`，`PlayerObject.cs:11833`）；
            // 系统邮件本作不做，见模块文档差异 7。
            can_reply: true,
            collected: self.collected,
            date_sent: self.date_sent,
            gold: self.gold,
            items: self.items.clone(),
        }
    }
}

/// 一个玩家的邮箱状态。
///
/// Crystal 的对应字段分散在 `PlayerObject` 上（`Info.Mail`、`NextMailTime`、`NewMail`，
/// `PlayerObject.cs:21`、`:91`、`CharacterInfo.cs:85`），本作合成一个结构放进
/// [`Player::mail`](crate::world::Player)。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MailState {
    /// 收件箱（按 `MailID` 升序）。
    pub mails: Vec<Mail>,
    /// 下一次允许发送的时刻（Crystal `PlayerObject.NextMailTime`，`:21`）。
    pub next_send_time: i64,
    /// 是否有新邮件待提示（Crystal `PlayerObject.NewMail`，`:91`）。
    ///
    /// Crystal 在 `PlayerObject.Process` 里看到它就发一条系统聊天并 `GetMail()`
    /// （`:499-503`），本作在 [`World::tick`] 的 [`World::process_new_mail`] 里做同一件事。
    pub new_mail: bool,
}

/// 邮件的发送目标（由会话按角色名查出来，世界层只做游戏规则）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailRecipient {
    /// 收件角色主键。
    pub character_id: i64,
    /// 收件角色名。
    pub name: String,
    /// 收件人当前邮箱里的邮件数（Crystal `player.Mail.Count`）。
    pub mail_count: usize,
}

/// 邮件操作失败原因。
///
/// 每条对应 Crystal 的一次 `ReceiveChat(...)`、一次 `S.MailSent{Result=-1}`，或本作补的
/// 保护（见模块文档差异 3/4）。失败包/提示已由 [`World`] 内部发出。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MailError {
    /// 玩家不在世界里。
    #[error("玩家 {0} 不在世界里")]
    NotInWorld(u32),
    /// 发送太频繁（Crystal 静默丢弃，`:11676-11680`）。
    #[error("发送邮件太快")]
    RateLimited,
    /// 正文超过 [`MAIL_TEXT_MAX`]（`ServerTextKeys.TextExceedsMailSizeLimit`，`:11737-11741`）。
    #[error("邮件正文超过 500 字")]
    TextTooLong,
    /// 收件人不存在（`ServerTextKeys.CouldNotFindPlayer`，`:11743-11749`）。
    #[error("找不到收件人")]
    RecipientMissing,
    /// 收件人邮箱已满（`ServerTextKeys.RecipientsMailboxFull`，`:11751-11755`）。
    #[error("收件人邮箱已满")]
    RecipientMailboxFull,
    /// 金币不够付「附件 + 邮费」（`S.MailSent{Result=-1}`，`:11763-11767`）。
    #[error("金币不够：需要 {needed}，只有 {held}")]
    NotEnoughGold {
        /// 附件金币 + 邮费。
        needed: i64,
        /// 实际持有。
        held: i64,
    },
    /// 物品不允许邮寄（`BindMode.DontTrade` / `BindMode.NoMail`，`:11800-11817`）。
    #[error("物品 {unique_id} 不允许邮寄")]
    CannotMail {
        /// 物品 UUID。
        unique_id: u64,
    },
    /// 附件数量超过协议上限（`ItemsIdx[5]`；本作 API 的防线）。
    #[error("附件数量 {count} 超过上限 5")]
    TooManyAttachments {
        /// 请求的附件数。
        count: usize,
    },
    /// 邮件不存在（Crystal 静默返回，`:11851`、`:11862`）。
    #[error("找不到邮件 {mail_id}")]
    NotFound {
        /// 邮件编号。
        mail_id: u64,
    },
    /// 包裹还没在邮局受理（`ServerTextKeys.MailCollectFromPostOffice`，`:11864-11868`）。
    #[error("请先到邮局受理包裹")]
    NotCollected,
    /// 背包放不下附件（`ServerTextKeys.CannotCollectWhenBagFull`，`:11870-11876`）。
    #[error("背包已满，无法领取附件")]
    InventoryFull,
    /// 邮件已锁定，不能删除（**本作扩展**，Crystal 不拦）。
    #[error("邮件已锁定")]
    Locked,
    /// 附件还没领取，不能删除（**本作扩展**，Crystal 不拦）。
    #[error("附件还没领取")]
    UncollectedAttachments,
    /// 附件字节块损坏（本作解码器防线）。
    #[error("邮件附件解析失败：{0}")]
    Codec(String),
}

impl From<ItemCodecError> for MailError {
    fn from(error: ItemCodecError) -> Self {
        Self::Codec(error.to_string())
    }
}

impl MailError {
    /// 给玩家看的中文提示（Crystal 用 `GameLanguage.ServerTextMap` 的 `ServerTextKeys`）。
    ///
    /// `RateLimited` 与 `TooManyAttachments` 不提示（Crystal 前者静默，后者是本作防线）。
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::NotInWorld(_) => "你不在游戏世界里。".to_owned(),
            Self::RateLimited | Self::TooManyAttachments { .. } => String::new(),
            Self::TextTooLong => format!("邮件正文不能超过 {MAIL_TEXT_MAX} 字。"),
            Self::RecipientMissing => "找不到该玩家。".to_owned(),
            Self::RecipientMailboxFull => "收件人的邮箱已满。".to_owned(),
            Self::NotEnoughGold { needed, held } => {
                format!("金币不足：需要 {needed}，你只有 {held}。")
            }
            Self::CannotMail { .. } => "这件物品不能邮寄。".to_owned(),
            Self::NotFound { .. } => "找不到这封邮件。".to_owned(),
            Self::NotCollected => "请先到邮局受理包裹。".to_owned(),
            Self::InventoryFull => "背包已满，无法领取附件。".to_owned(),
            Self::Locked => "邮件已锁定，不能删除。".to_owned(),
            Self::UncollectedAttachments => "附件还没领取，不能删除。".to_owned(),
            Self::Codec(_) => "邮件附件数据损坏。".to_owned(),
        }
    }
}

/// 一条待落盘的邮件变更（世界层是同步的，存储层是异步的）。
///
/// 与 [`GuildWrite`](crate::world::GuildWrite) 同一做法：世界只记录「要写什么」，
/// 会话在包处理完后用 [`persist_mail_writes`] 写库。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailWrite {
    /// 新建一封邮件（收件箱）。
    Create(Box<MailRecord>),
    /// 回写一封邮件的状态（已读/锁定/受理/附件被领走）。
    Save(Box<MailRecord>),
    /// 删除一封邮件。
    Delete {
        /// 邮件编号。
        mail_id: u64,
    },
}

/// 把世界积压的邮件写入落到存储层。
///
/// # Errors
/// 任何一条写入失败都会立刻返回（与 `guild::persist_guild_writes` 一致）。
pub async fn persist_mail_writes<S: AccountStore>(
    store: &S,
    writes: Vec<MailWrite>,
) -> Result<(), StoreError> {
    for write in writes {
        match write {
            MailWrite::Create(mail) => {
                store.create_mail(*mail).await?;
            }
            MailWrite::Save(mail) => {
                store.save_mail(*mail).await?;
            }
            MailWrite::Delete { mail_id } => {
                store.delete_mail(mail_id).await?;
            }
        }
    }
    Ok(())
}

/// 邮件重发的正文长度按 C# `string.Length`（UTF-16 码元）计算。
fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// 一封新邮件的 `Collected` 初值（Crystal `MailInfo.Send`，`MailInfo.cs:99-143`）。
///
/// 非包裹恒为 `true`；包裹按 `MailAutoSendGold` / `MailAutoSendItems` 决定。
/// 两个开关都是 `false` 时，任何包裹都要先去邮局受理（`Collected=false`）。
fn collected_on_send(has_gold: bool, has_items: bool) -> bool {
    if !has_gold && !has_items {
        return true;
    }
    if has_gold && has_items {
        MAIL_AUTO_SEND_GOLD && MAIL_AUTO_SEND_ITEMS
    } else if has_items {
        MAIL_AUTO_SEND_ITEMS
    } else {
        MAIL_AUTO_SEND_GOLD
    }
}

impl World {
    /// 分配一个邮件编号（Crystal `++Envir.NextMailID`，`PlayerObject.cs:11835`）。
    ///
    /// 与物品 UUID 同一思路：起点取「Unix 毫秒 << 20」，单调递增且与数据库里的历史值
    /// 不冲突（Crystal 的 ID 是 1、2、3… 的紧凑序列；本作只要能唯一即可）。
    pub fn allocate_mail_id(&mut self) -> u64 {
        if self.next_mail_id == 0 {
            let now = u64::try_from(self.wall_now().max(0)).unwrap_or(0);
            self.next_mail_id = now << 20;
        }
        let id = self.next_mail_id;
        self.next_mail_id = self.next_mail_id.saturating_add(1);
        id
    }

    /// 取走积压的邮件写入。
    pub fn take_mail_writes(&mut self) -> Vec<MailWrite> {
        std::mem::take(&mut self.mail_writes)
    }

    /// 直接放入一封邮件（登录装载 / 测试用），按 `MailID` 升序。
    pub fn insert_mail(&mut self, player_id: u32, mail: Mail) {
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        let index = player
            .mail
            .mails
            .partition_point(|existing| existing.id < mail.id);
        player.mail.mails.insert(index, mail);
    }

    /// 邮箱（只读；测试与 UI 用）。
    #[must_use]
    pub fn mailbox(&self, player_id: u32) -> Option<&[Mail]> {
        self.players
            .get(&player_id)
            .map(|player| player.mail.mails.as_slice())
    }

    /// 按 `MailID` 取一封邮件。
    #[must_use]
    pub fn mail(&self, player_id: u32, mail_id: u64) -> Option<&Mail> {
        self.players
            .get(&player_id)?
            .mail
            .mails
            .iter()
            .find(|mail| mail.id == mail_id)
    }

    /// 移除一封邮件并记录待落盘的删除。
    fn remove_mail(&mut self, player_id: u32, mail_id: u64) {
        if let Some(player) = self.players.get_mut(&player_id) {
            player.mail.mails.retain(|mail| mail.id != mail_id);
        }
        self.mail_writes.push(MailWrite::Delete { mail_id });
    }

    /// 记录一封邮件的状态变更（已读/锁定/受理/领取）。
    fn save_mail_state(&mut self, player_id: u32, mail_id: u64) {
        let Some(mail) = self
            .players
            .get(&player_id)
            .and_then(|player| player.mail.mails.iter().find(|mail| mail.id == mail_id))
            .cloned()
        else {
            return;
        };
        self.mail_writes
            .push(MailWrite::Save(Box::new(mail.to_record())));
    }

    /// 生成一份 `S.ReceiveMail`（Crystal `PlayerObject.GetMail`，`:11959-11988`）。
    ///
    /// 只取最后 [`MAIL_CAPACITY`] 封（`start = Info.Mail.Count - MailCapacity`）。
    #[must_use]
    pub fn mail_list_packet(&self, player_id: u32) -> Option<ServerPacket> {
        let player = self.players.get(&player_id)?;
        let start = player.mail.mails.len().saturating_sub(MAIL_CAPACITY);
        let mail = player.mail.mails[start..]
            .iter()
            .map(Mail::to_client_mail)
            .collect();
        Some(ServerPacket::ReceiveMail(ReceiveMail { mail }))
    }

    /// 给玩家下发整个邮箱（Crystal `GetMail()` 的最后一行 `Enqueue(new S.ReceiveMail …)`）。
    ///
    /// 同时清掉 `NewMail` 标志（`:11985`）。
    fn send_mail_list(&mut self, player_id: u32) {
        let Some(packet) = self.mail_list_packet(player_id) else {
            return;
        };
        if let Some(player) = self.players.get_mut(&player_id) {
            player.mail.new_mail = false;
        }
        self.send_to_player(player_id, &packet.encode_frame());
    }

    /// 新邮件提示：Crystal `PlayerObject.Process` 的 `if (NewMail)`（`:499-503`）。
    ///
    /// 由 [`World::tick`] 调用。收件人在线时才会走到这里（离线邮件没有 Player 对象）。
    pub fn process_new_mail(&mut self, player_id: u32) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        if !player.mail.new_mail {
            return;
        }
        self.send_system_chat(player_id, "你有新邮件。");
        self.send_mail_list(player_id);
    }

    /// 计算一封邮件的邮费：Crystal `PlayerObject.GetMailCost`（`:11926-11957`）。
    ///
    /// ```text
    /// cost = 0
    /// if (!MailFreeWithStamp || !stamped) {
    ///     if (gold > 0 && MailCostPer1KGold > 0) cost += floor(gold / 1000) * MailCostPer1KGold;
    ///     if (items.Length > 0 && MailItemInsurancePercentage > 0)
    ///         for j in 0..(stamped ? 5 : 1):
    ///             cost += floor(item.Price() / 100 * MailItemInsurancePercentage);
    /// }
    /// ```
    ///
    /// 物品价格用 [`crate::npc::item_price`]（逐位对照 `UserItem.Price()`，
    /// `Shared/Data/ItemData.cs:516-542`）。找不到的物品跳过。
    #[must_use]
    pub fn mail_cost(&self, player_id: u32, items: &[u64], gold: u32, stamped: bool) -> u32 {
        if MAIL_FREE_WITH_STAMP && stamped {
            return 0;
        }
        let mut cost = 0u32;
        if gold > 0 && MAIL_COST_PER_1K_GOLD > 0 {
            cost = cost.saturating_add((gold / 1000).saturating_mul(MAIL_COST_PER_1K_GOLD));
        }
        if !items.is_empty() && MAIL_ITEM_INSURANCE_PERCENT > 0 {
            let limit = if stamped {
                MAIL_MAX_ATTACHMENTS
            } else {
                MAIL_ATTACHMENTS_WITHOUT_STAMP
            };
            for unique_id in items.iter().take(limit) {
                if *unique_id < 1 {
                    continue;
                }
                let Some(item) = self
                    .find_inventory_item(player_id, *unique_id)
                    .map(|(_, item)| item)
                else {
                    continue;
                };
                let Some(info) = self.item_info(item.item_index) else {
                    continue;
                };
                let price = crate::npc::item_price(info, &item);
                // C#：`(uint)Math.Floor((double)price / 100 * percentage)`。
                cost = cost.saturating_add(
                    price
                        .saturating_mul(MAIL_ITEM_INSURANCE_PERCENT)
                        .saturating_div(100),
                );
            }
        }
        cost
    }

    /// `C.MailCost`：只回报价，不改任何状态（`MirConnection.cs:2045-2052`）。
    pub fn mail_cost_request(&mut self, player_id: u32, items: &[u64], gold: u32, stamped: bool) {
        let cost = self.mail_cost(player_id, items, gold, stamped);
        self.send_to_player(
            player_id,
            &ServerPacket::MailCost(MailCostPacket { cost }).encode_frame(),
        );
    }

    /// `C.MailLockedItem`：原样回显（`MirConnection.cs:667-669`）。
    ///
    /// 这是写信界面里「锁定某个附件格子」的纯客户端状态，服务端不做校验。
    pub fn mail_locked_item(&mut self, player_id: u32, unique_id: u64, locked: bool) {
        self.send_to_player(
            player_id,
            &ServerPacket::MailLockedItem(MailLockedItem { unique_id, locked }).encode_frame(),
        );
    }

    /// NPC `[@SENDPARCEL]` 页面：让客户端打开写信界面（`NPCScript.cs:1078-1080`）。
    pub fn mail_open_send_page(&mut self, player_id: u32) {
        self.send_to_player(
            player_id,
            &ServerPacket::MailSendRequest(MailSendRequest {}).encode_frame(),
        );
    }

    /// 发送一封邮件：Crystal `PlayerObject.SendMail(name, message, gold, items, stamped)`
    /// （`:11727-11845`）。
    ///
    /// `recipient` 由会话按角色名查出来（大小写不敏感，允许离线）；`items` 是背包物品
    /// 的 UUID 列表，最多 [`MAIL_MAX_ATTACHMENTS`] 项，`0` 表示空槽。
    ///
    /// # Errors
    /// 见 [`MailError`]；对应的聊天/失败包已在内部发出。
    pub fn mail_send(
        &mut self,
        sender_id: u32,
        recipient: Option<MailRecipient>,
        message: &str,
        gold: u32,
        items: &[u64],
        stamped: bool,
    ) -> Result<(), MailError> {
        let Some(player) = self.players.get(&sender_id) else {
            return Err(MailError::NotInWorld(sender_id));
        };
        // 1. 限速（`:11676-11682`、`:11729-11735`）：太快直接丢弃，不提示。
        let now = self.now();
        if now < player.mail.next_send_time {
            return Err(MailError::RateLimited);
        }
        if let Some(player) = self.players.get_mut(&sender_id) {
            player.mail.next_send_time = now + MAIL_RATE_MS;
        }
        // 2. 正文长度（`:11737-11741`）。
        if utf16_len(message) > MAIL_TEXT_MAX {
            self.send_system_chat(sender_id, &MailError::TextTooLong.message());
            return Err(MailError::TextTooLong);
        }
        // 3. 收件人（`:11743-11749`）。
        let Some(recipient) = recipient else {
            self.send_system_chat(sender_id, &MailError::RecipientMissing.message());
            return Err(MailError::RecipientMissing);
        };
        // 4. 收件人邮箱上限（`:11751-11755`）。
        if recipient.mail_count > MAIL_RECIPIENT_LIMIT {
            self.send_system_chat(sender_id, &MailError::RecipientMailboxFull.message());
            return Err(MailError::RecipientMailboxFull);
        }
        if items.len() > MAIL_MAX_ATTACHMENTS {
            return Err(MailError::TooManyAttachments { count: items.len() });
        }
        // 5. 邮费与总价（`:11757-11767`）。
        let parcel_cost = self.mail_cost(sender_id, items, gold, stamped);
        let total = i64::from(gold) + i64::from(parcel_cost);
        let held = self.players[&sender_id].gold;
        if held < total || held < i64::from(gold) {
            self.send_to_player(
                sender_id,
                &ServerPacket::MailSent(MailSent { result: -1 }).encode_frame(),
            );
            return Err(MailError::NotEnoughGold {
                needed: total,
                held,
            });
        }
        // 6. 邮票（`:11769-11786`）：ItemType.Nothing + Shape 1，消耗 1 个。
        let mut has_stamp = false;
        if stamped {
            if let Some((slot, stamp)) = self.find_stamp(sender_id) {
                has_stamp = true;
                if stamp.count > 1 {
                    if let Some(player) = self.players.get_mut(&sender_id) {
                        if let Some(Some(existing)) = player.inventory.get_mut(slot) {
                            existing.count -= 1;
                        }
                    }
                } else if let Some(player) = self.players.get_mut(&sender_id) {
                    player.inventory[slot] = None;
                }
                self.send_to_player(
                    sender_id,
                    &ServerPacket::DeleteItem(DeleteItem {
                        unique_id: stamp.unique_id,
                        count: 1,
                    })
                    .encode_frame(),
                );
            }
        }
        // 7. 附件（`:11788-11824`）。没贴邮票只认第 1 格。
        let limit = if has_stamp {
            MAIL_MAX_ATTACHMENTS
        } else {
            MAIL_ATTACHMENTS_WITHOUT_STAMP
        };
        let mut gifts: Vec<UserItem> = Vec::new();
        for unique_id in items.iter().take(limit) {
            if *unique_id < 1 {
                continue;
            }
            let Some((slot, item)) = self.find_inventory_item(sender_id, *unique_id) else {
                // Crystal 找不到就跳过（`:11794-11799`），不发任何包。
                continue;
            };
            let bind = self.item_info(item.item_index).map(|info| info.bind);
            let rental_blocked = item
                .rental_information
                .as_ref()
                .is_some_and(|rental| rental.binding_flags.contains(BindMode::DontTrade));
            let blocked = bind.is_some_and(|bind| {
                bind.contains(BindMode::DontTrade) || bind.contains(BindMode::NoMail)
            });
            if blocked || rental_blocked {
                self.send_system_chat(
                    sender_id,
                    &MailError::CannotMail {
                        unique_id: item.unique_id,
                    }
                    .message(),
                );
                // `DontTrade` 分支不发 `S.MailSent`，`NoMail` 分支会发 `-1`（`:11800-11811`）。
                if bind.is_some_and(|bind| bind.contains(BindMode::NoMail)) {
                    self.send_to_player(
                        sender_id,
                        &ServerPacket::MailSent(MailSent { result: -1 }).encode_frame(),
                    );
                }
                return Err(MailError::CannotMail {
                    unique_id: item.unique_id,
                });
            }
            if let Some(player) = self.players.get_mut(&sender_id) {
                player.inventory[slot] = None;
            }
            self.send_to_player(
                sender_id,
                &ServerPacket::DeleteItem(DeleteItem {
                    unique_id: item.unique_id,
                    count: item.count,
                })
                .encode_frame(),
            );
            gifts.push(item);
        }
        // 8. 扣钱（`:11826-11830`）：注意扣的是「附件金币 + 邮费」。
        if total > 0 {
            if let Some(player) = self.players.get_mut(&sender_id) {
                player.gold -= total;
            }
            let lost = u32::try_from(total).unwrap_or(u32::MAX);
            self.send_to_player(
                sender_id,
                &ServerPacket::LoseGold(LoseGold { gold: lost }).encode_frame(),
            );
        }
        // 9. 投递（`:11832-11842`）：`Gold` 只记附件金币，邮费不进邮件。
        let mail_id = self.allocate_mail_id();
        let sender_name = self.players[&sender_id].name.clone();
        let collected = collected_on_send(gold > 0, !gifts.is_empty());
        let mail = Mail {
            id: mail_id,
            character_id: recipient.character_id,
            sender_name,
            subject: String::new(),
            message: message.to_owned(),
            gold,
            items: gifts,
            date_sent: DateTimeBinary::from_unix_seconds(self.wall_now().max(0) / 1000),
            opened: false,
            locked: false,
            collected,
        };
        self.mail_writes
            .push(MailWrite::Create(Box::new(mail.to_record())));
        self.deliver_mail(mail);
        // 10. 成功回执（`:11844`）。
        self.send_to_player(
            sender_id,
            &ServerPacket::MailSent(MailSent { result: 1 }).encode_frame(),
        );
        Ok(())
    }

    /// 把一封新邮件放进收件人的运行期邮箱（在线时）；离线邮件只落库。
    fn deliver_mail(&mut self, mail: Mail) {
        let recipient_object = self
            .players
            .values()
            .find(|player| player.character_id == mail.character_id)
            .map(|player| player.object_id);
        let Some(object_id) = recipient_object else {
            return;
        };
        self.insert_mail(object_id, mail);
        if let Some(player) = self.players.get_mut(&object_id) {
            // Crystal `RecipientInfo.Player.NewMail = true`（`MailInfo.cs:137-140`）。
            player.mail.new_mail = true;
        }
    }

    /// 找邮票：`ItemType.Nothing`、`Shape == 1`、数量 ≥ 1（`PlayerObject.cs:11776`）。
    fn find_stamp(&self, player_id: u32) -> Option<(usize, UserItem)> {
        let player = self.players.get(&player_id)?;
        player
            .inventory
            .iter()
            .enumerate()
            .find_map(|(slot, item)| {
                let item = item.as_ref()?;
                let info = self.item_info(item.item_index)?;
                if info.item_type == ItemType::Nothing && info.shape == 1 && item.count >= 1 {
                    Some((slot, item.clone()))
                } else {
                    None
                }
            })
    }

    /// `C.ReadMail`：标记已读并重发整个邮箱（Crystal `PlayerObject.ReadMail`，`:11847-11856`）。
    ///
    /// 注意 Crystal **没有**专门的「读信回执」包，`GetMail()` 就是回执。
    pub fn mail_read(&mut self, player_id: u32, mail_id: u64) -> Result<(), MailError> {
        let Some(player) = self.players.get_mut(&player_id) else {
            return Err(MailError::NotInWorld(player_id));
        };
        let Some(mail) = player.mail.mails.iter_mut().find(|mail| mail.id == mail_id) else {
            return Err(MailError::NotFound { mail_id });
        };
        mail.opened = true;
        self.save_mail_state(player_id, mail_id);
        self.send_mail_list(player_id);
        Ok(())
    }

    /// NPC `[@COLLECTPARCEL]` 页面：把所有包裹标记为「已受理」并重发邮箱
    /// （Crystal `NPCScript.cs:1081-1098`）。
    ///
    /// 没有待受理的包裹时 `Result = -1`；否则 `Result = 0`，并且**照样**下发邮箱。
    pub fn mail_collect_parcel_page(&mut self, player_id: u32) -> Result<i8, MailError> {
        if !self.players.contains_key(&player_id) {
            return Err(MailError::NotInWorld(player_id));
        }
        let awaiting = self.players[&player_id]
            .mail
            .mails
            .iter()
            .any(|mail| !mail.collected);
        let result = if awaiting {
            // `if (mail.Parcel) mail.Collected = true;`（`NPCScript.cs:1091-1094`）。
            let ids: Vec<u64> = self.players[&player_id]
                .mail
                .mails
                .iter()
                .filter(|mail| mail.is_parcel())
                .map(|mail| mail.id)
                .collect();
            for id in ids {
                if let Some(player) = self.players.get_mut(&player_id) {
                    if let Some(mail) = player.mail.mails.iter_mut().find(|mail| mail.id == id) {
                        mail.collected = true;
                    }
                }
                self.save_mail_state(player_id, id);
            }
            0
        } else {
            -1
        };
        self.send_to_player(
            player_id,
            &ServerPacket::ParcelCollected(ParcelCollected { result }).encode_frame(),
        );
        self.send_mail_list(player_id);
        Ok(result)
    }

    /// `C.CollectParcel`：领取附件（Crystal `PlayerObject.CollectMail`，`:11858-11902`）。
    ///
    /// 前置条件：包裹必须已经在邮局受理（`Collected == true`）。
    pub fn mail_collect(&mut self, player_id: u32, mail_id: u64) -> Result<(), MailError> {
        let Some(mail) = self.mail(player_id, mail_id).cloned() else {
            return Err(MailError::NotFound { mail_id });
        };
        if !mail.collected {
            self.send_system_chat(player_id, &MailError::NotCollected.message());
            return Err(MailError::NotCollected);
        }
        if !mail.items.is_empty() && !self.can_gain_mail_items(player_id, &mail.items) {
            self.send_system_chat(player_id, &MailError::InventoryFull.message());
            return Err(MailError::InventoryFull);
        }
        // `for (...) GainItem(mail.Items[i]);`（`:11878-11882`）——逐件进背包并发 GainedItem。
        for item in &mail.items {
            self.send_to_player(
                player_id,
                &ServerPacket::GainedItem(GainedItem { item: item.clone() }).encode_frame(),
            );
            if !self.add_item_to_inventory(player_id, item.clone()) {
                tracing::warn!(
                    player = player_id,
                    unique_id = item.unique_id,
                    "领取邮件附件失败"
                );
            }
        }
        if mail.gold > 0 {
            // `if (gold + Account.Gold >= uint.MaxValue)` 截断（`:11888-11891`）。
            self.gain_gold(player_id, mail.gold);
        }
        if let Some(player) = self.players.get_mut(&player_id) {
            if let Some(stored) = player.mail.mails.iter_mut().find(|row| row.id == mail_id) {
                stored.items.clear();
                stored.gold = 0;
                stored.collected = true;
            }
        }
        self.save_mail_state(player_id, mail_id);
        self.send_to_player(
            player_id,
            &ServerPacket::ParcelCollected(ParcelCollected { result: 1 }).encode_frame(),
        );
        self.send_mail_list(player_id);
        Ok(())
    }

    /// 邮件附件的「能不能全放下」判定：Crystal `HumanObject.CanGainItems`
    /// （`Server/MirObjects/HumanObject.cs:7629-7660`）。
    ///
    /// Crystal 的算法是：可堆叠物品若在背包里有同类堆且会溢出，就额外占一格，
    /// 再看 `FreeSpace >= 物品数 + 溢出数`。本作逐位照抄。
    #[must_use]
    pub fn can_gain_mail_items(&self, player_id: u32, items: &[UserItem]) -> bool {
        let Some(player) = self.players.get(&player_id) else {
            return false;
        };
        if items.is_empty() {
            return true;
        }
        let mut stack_offset = 0usize;
        for item in items {
            let Some(info) = self.item_info(item.item_index) else {
                return false;
            };
            if info.stack_size > 1 {
                for bag_item in &player.inventory {
                    let Some(bag_item) = bag_item else {
                        continue;
                    };
                    if self
                        .item_info(bag_item.item_index)
                        .is_none_or(|bag_info| bag_info.index != info.index)
                    {
                        continue;
                    }
                    if bag_item.count.saturating_add(item.count) > info.stack_size {
                        stack_offset += 1;
                    }
                    break;
                }
            }
        }
        let free = player
            .inventory
            .iter()
            .filter(|slot| slot.is_none())
            .count();
        free >= items.len() + stack_offset
    }

    /// `C.DeleteMail`：删除一封邮件（Crystal `PlayerObject.DeleteMail`，`:11904-11913`）。
    ///
    /// 本作扩展两道保护：已锁定、附件未领取的邮件不能删（Crystal 交给客户端拦）。
    pub fn mail_delete(&mut self, player_id: u32, mail_id: u64) -> Result<(), MailError> {
        let Some(mail) = self.mail(player_id, mail_id).cloned() else {
            return Err(MailError::NotFound { mail_id });
        };
        if mail.locked {
            self.send_system_chat(player_id, &MailError::Locked.message());
            return Err(MailError::Locked);
        }
        // 只要附件还在（金币或物品），就不允许删除——包括「已到邮局受理但还没领取」的包裹。
        if mail.is_parcel() {
            self.send_system_chat(player_id, &MailError::UncollectedAttachments.message());
            return Err(MailError::UncollectedAttachments);
        }
        self.remove_mail(player_id, mail_id);
        self.send_mail_list(player_id);
        Ok(())
    }

    /// `C.LockMail`：锁定/解锁（Crystal `PlayerObject.LockMail`，`:11915-11924`）。
    pub fn mail_lock(&mut self, player_id: u32, mail_id: u64, lock: bool) -> Result<(), MailError> {
        let Some(player) = self.players.get_mut(&player_id) else {
            return Err(MailError::NotInWorld(player_id));
        };
        let Some(mail) = player.mail.mails.iter_mut().find(|mail| mail.id == mail_id) else {
            return Err(MailError::NotFound { mail_id });
        };
        mail.locked = lock;
        self.save_mail_state(player_id, mail_id);
        self.send_mail_list(player_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_matches_crystal_formula() {
        assert_eq!(MAIL_COST_PER_1K_GOLD, 100);
        assert_eq!(MAIL_ITEM_INSURANCE_PERCENT, 5);
        // `floor(gold / 1000) * 100`：999 金免费，1000 金收 100，1999 金仍收 100。
        assert_eq!((999u32 / 1000).saturating_mul(MAIL_COST_PER_1K_GOLD), 0);
        assert_eq!((1_000u32 / 1000).saturating_mul(MAIL_COST_PER_1K_GOLD), 100);
        assert_eq!((1_999u32 / 1000).saturating_mul(MAIL_COST_PER_1K_GOLD), 100);
    }

    #[test]
    fn parcel_collected_default_depends_on_auto_settings() {
        assert!(collected_on_send(false, false), "纯信件直接算已受理");
        assert!(!collected_on_send(true, false), "金币包裹要先去邮局");
        assert!(!collected_on_send(false, true), "物品包裹要先去邮局");
        assert!(!collected_on_send(true, true), "混合包裹要先去邮局");
    }

    #[test]
    fn expiry_uses_thirty_days() {
        let now = 1_700_000_000i64;
        let mut mail = Mail {
            id: 1,
            character_id: 1,
            sender_name: "阿甲".to_owned(),
            subject: String::new(),
            message: String::new(),
            items: Vec::new(),
            gold: 0,
            date_sent: DateTimeBinary::from_unix_seconds(now - MAIL_EXPIRE_SECONDS),
            opened: false,
            locked: false,
            collected: true,
        };
        assert!(!mail.is_expired(now), "刚好 30 天不过期");
        mail.date_sent = DateTimeBinary::from_unix_seconds(now - MAIL_EXPIRE_SECONDS - 1);
        assert!(mail.is_expired(now), "超过 30 天过期");
    }

    #[test]
    fn record_roundtrip_keeps_attachments() {
        let mail = Mail {
            id: 9,
            character_id: 3,
            sender_name: "阿甲".to_owned(),
            subject: String::new(),
            message: "屠龙送你".to_owned(),
            items: vec![UserItem {
                unique_id: 77,
                item_index: 42,
                count: 2,
                ..UserItem::default()
            }],
            gold: 1_000,
            date_sent: DateTimeBinary::from_unix_seconds(1_000),
            opened: true,
            locked: false,
            collected: true,
        };
        let record = mail.to_record();
        assert!(!record.items.is_empty());
        assert_eq!(Mail::from_record(&record).expect("解码"), mail);
    }
}
