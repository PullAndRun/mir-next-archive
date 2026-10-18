//! PostgreSQL 存储实现（把 `mir-store` 的仓储适配成服务端的 [`AccountStore`]）。

use chrono::{DateTime, TimeZone, Utc};
use mir_core::DateTimeBinary;
use mir_core::enums::{MarketItemType, MirClass, MirDirection, MirGender};
use mir_store::PgPool;

use crate::store::{
    Account, AccountStore, AuctionRecord, Character, ConquestRecord, GameShopPurchaseRecord,
    GameShopStockRecord, GameShopUpdate, GuildBuffRecord, GuildMemberRecord, GuildRankRecord,
    GuildRecord, GuildStorageRecord, MailRecord, MarketUpdate, StoreError,
};

/// 基于 PostgreSQL 的账号/角色存储。
#[derive(Debug, Clone)]
pub struct PgAccountStore {
    pool: PgPool,
}

impl PgAccountStore {
    /// 连接数据库并跑完迁移。
    ///
    /// # Errors
    /// 连接失败或迁移失败时返回错误。
    pub async fn connect(database_url: &str) -> Result<Self, StoreError> {
        let pool = mir_store::connect(database_url)
            .await
            .map_err(|error| StoreError::Internal(error.to_string()))?;
        mir_store::migrate(&pool)
            .await
            .map_err(|error| StoreError::Internal(error.to_string()))?;
        Ok(Self { pool })
    }

    /// 底层连接池（后台任务、工具会用到）。
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

fn map_store_error(error: mir_store::StoreError) -> StoreError {
    match error {
        mir_store::StoreError::AccountExists(username) => StoreError::AccountExists(username),
        mir_store::StoreError::CharacterNameTaken(name) => StoreError::CharacterExists(name),
        mir_store::StoreError::GuildExists(name) => StoreError::GuildExists(name),
        mir_store::StoreError::InvalidLanguage(code) => StoreError::InvalidLanguage(code),
        mir_store::StoreError::NotFound(entity) => StoreError::NotFound(entity),
        other => StoreError::Internal(other.to_string()),
    }
}

/// Unix 秒 → `DateTime<Utc>`；越界或未设置（0）时返回 `None`。
///
/// 用于 `created`/`expiry` 与邮件时间这类「可能为空」的列。
fn to_datetime(value: DateTimeBinary) -> Option<DateTime<Utc>> {
    if value.is_unset() {
        return None;
    }
    // 用 `timestamp_opt` 而不是 `timestamp`：越界值（比如存档里的 0）不应该 panic。
    Utc.timestamp_opt(value.to_unix_seconds(), 0).single()
}

fn from_datetime(value: Option<DateTime<Utc>>) -> DateTimeBinary {
    value.map_or(DateTimeBinary(0), |date| {
        DateTimeBinary::from_unix_seconds(date.timestamp())
    })
}

fn from_store_account(account: mir_store::Account) -> Account {
    Account {
        id: account.id,
        username: account.username,
        password_hash: account.password_hash,
        salt: account.salt,
        email: account.email,
        banned: account.banned,
        ban_reason: account.ban_reason,
        ban_expiry: account.expiry_date.map(|date| from_datetime(Some(date))),
        require_password_change: account.require_password_change,
        wrong_password_count: u32::try_from(account.wrong_password_count).unwrap_or(0),
        admin: account.admin,
        language: account.language,
    }
}

fn from_store_character(character: mir_store::Character) -> Character {
    Character {
        id: character.id,
        account_id: character.account_id,
        name: character.name,
        level: u16::try_from(character.level).unwrap_or(0),
        class: MirClass::from_value(u8::try_from(character.class).unwrap_or(0)).unwrap_or_default(),
        gender: MirGender::from_value(u8::try_from(character.gender).unwrap_or(0))
            .unwrap_or_default(),
        hair: u8::try_from(character.hair).unwrap_or(0),
        map: character.current_map_index.to_string(),
        x: character.current_x,
        y: character.current_y,
        direction: MirDirection::from_value(u8::try_from(character.direction).unwrap_or(4))
            .unwrap_or_default(),
        hp: character.hp,
        mp: character.mp,
        experience: character.experience,
        gold: character.gold,
        credit: character.credit,
        inventory: character.inventory,
        equipment: character.equipment,
        storage: character.storage,
        magics: character.magics,
        quests: character.quests,
        quest_inventory: character.quest_inventory,
        last_daily_reset_at: character.last_daily_reset_at,
        pet_mode: character.pet_mode,
        guild_index: character.guild_index,
        last_access: from_datetime(character.last_login_date),
    }
}

/// 把服务端的角色视图写回存储行。
fn apply_to_store_character(character: &Character, target: &mut mir_store::Character) {
    target.name.clone_from(&character.name);
    target.level = i16::try_from(character.level).unwrap_or(1);
    target.class = i16::from(character.class as u8);
    target.gender = i16::from(character.gender as u8);
    target.hair = i16::from(character.hair);
    target.current_map_index = character
        .map
        .parse::<i32>()
        .unwrap_or(target.current_map_index);
    target.current_x = character.x;
    target.current_y = character.y;
    target.direction = i16::from(character.direction as u8);
    target.hp = character.hp;
    target.mp = character.mp;
    target.experience = character.experience;
    target.gold = character.gold;
    target.credit = character.credit;
    target.inventory.clone_from(&character.inventory);
    target.equipment.clone_from(&character.equipment);
    // 仓库：`characters.storage`（Crystal 挂在账号上的那块 `UserItem[80]`）。
    target.storage.clone_from(&character.storage);
    target.magics.clone_from(&character.magics);
    target.quests.clone_from(&character.quests);
    // 任务物品栏：`characters.quest_inventory`（迁移 0012，Crystal `CharacterInfo.QuestInventory`）。
    target
        .quest_inventory
        .clone_from(&character.quest_inventory);
    target.last_daily_reset_at = character.last_daily_reset_at;
    target.pet_mode = character.pet_mode;
    target.guild_index = character.guild_index;
}

// ---------------------------------------------------------------------------
// 行会：服务端视图 ↔ `mir-store` 行
// ---------------------------------------------------------------------------

/// Unix 毫秒 → `DateTime<Utc>`（越界值退到纪元，不 panic）。
///
/// 用于成员的 `joined_at`、仓库格子的 `stored_at`（都是本作扩展的毫秒时间戳）。
fn millis_to_datetime(millis: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp_millis(millis).unwrap_or_default()
}

/// 服务端的行会视图 → `mir-store` 的行会。
fn to_store_guild(guild: &GuildRecord) -> mir_store::Guild {
    mir_store::Guild {
        id: guild.id,
        name: guild.name.clone(),
        leader_character_id: guild.leader_character_id,
        level: guild.level,
        experience: guild.experience,
        gold: guild.gold,
        spare_points: guild.spare_points,
        notice: guild.notice.clone(),
        ranks: guild
            .ranks
            .iter()
            .map(|rank| mir_store::GuildRankRow {
                guild_id: guild.id,
                rank_index: i16::from(rank.index),
                name: rank.name.clone(),
                options: i16::from(rank.options),
            })
            .collect(),
        members: guild
            .members
            .iter()
            .map(|member| mir_store::GuildMemberRow {
                guild_id: guild.id,
                character_id: member.character_id,
                name: member.name.clone(),
                rank_index: i16::from(member.rank_index),
                joined_at: millis_to_datetime(member.joined_at),
            })
            .collect(),
        storage: guild
            .storage
            .iter()
            .map(|item| mir_store::GuildStorageRow {
                guild_id: guild.id,
                slot: i16::from(item.slot),
                item: item.item.clone(),
                user_id: item.user_id,
                stored_at: millis_to_datetime(item.stored_at),
            })
            .collect(),
        buffs: guild
            .buffs
            .iter()
            .map(|buff| mir_store::GuildBuffRow {
                guild_id: guild.id,
                buff_id: buff.id,
                active: buff.active,
                active_time_remaining: buff.active_time_remaining,
            })
            .collect(),
    }
}

/// `mir-store` 的行会 → 服务端的行会视图。
fn from_store_guild(guild: mir_store::Guild) -> GuildRecord {
    GuildRecord {
        id: guild.id,
        name: guild.name,
        leader_character_id: guild.leader_character_id,
        level: guild.level,
        experience: guild.experience,
        gold: guild.gold,
        spare_points: guild.spare_points,
        notice: guild.notice,
        ranks: guild
            .ranks
            .into_iter()
            .map(|rank| GuildRankRecord {
                index: u8::try_from(rank.rank_index).unwrap_or(0),
                name: rank.name,
                options: u8::try_from(rank.options).unwrap_or(0),
            })
            .collect(),
        members: guild
            .members
            .into_iter()
            .map(|member| GuildMemberRecord {
                character_id: member.character_id,
                name: member.name,
                rank_index: u8::try_from(member.rank_index).unwrap_or(0),
                joined_at: member.joined_at.timestamp_millis(),
            })
            .collect(),
        storage: guild
            .storage
            .into_iter()
            .map(|item| GuildStorageRecord {
                slot: u8::try_from(item.slot).unwrap_or(0),
                item: item.item,
                user_id: item.user_id,
                stored_at: item.stored_at.timestamp_millis(),
            })
            .collect(),
        buffs: guild
            .buffs
            .into_iter()
            .map(|buff| GuildBuffRecord {
                id: buff.buff_id,
                active: buff.active,
                active_time_remaining: buff.active_time_remaining,
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// 邮件：服务端视图 ↔ `mir-store` 行
// ---------------------------------------------------------------------------

/// `mir-store` 的邮件行 → 服务端视图。
fn from_store_mail(mail: mir_store::Mail) -> MailRecord {
    MailRecord {
        id: u64::try_from(mail.id).unwrap_or(0),
        character_id: mail.character_id,
        sender_name: mail.sender_name,
        subject: mail.subject,
        message: mail.message,
        items: mail.items,
        gold: u32::try_from(mail.gold).unwrap_or(u32::MAX),
        date_sent: from_datetime(Some(mail.date_sent)),
        opened: mail.opened,
        locked: mail.locked,
        collected: mail.collected,
    }
}

/// 服务端的邮件视图 → `mir-store` 的行。
fn to_store_mail(mail: &MailRecord) -> mir_store::Mail {
    mir_store::Mail {
        id: i64::try_from(mail.id).unwrap_or(0),
        character_id: mail.character_id,
        sender_name: mail.sender_name.clone(),
        subject: mail.subject.clone(),
        message: mail.message.clone(),
        items: mail.items.clone(),
        gold: i64::from(mail.gold),
        // 未设置的时间落成纪元（`DateTime::<Utc>::default()`），不 panic。
        date_sent: to_datetime(mail.date_sent).unwrap_or_default(),
        opened: mail.opened,
        locked: mail.locked,
        collected: mail.collected,
    }
}

// ---------------------------------------------------------------------------
// 攻城：服务端视图 ↔ `mir-store` 行
// ---------------------------------------------------------------------------

/// Unix 毫秒 → 可空时间列；`0`（未设置）落成 `NULL`。
fn optional_millis(millis: i64) -> Option<DateTime<Utc>> {
    (millis > 0).then(|| millis_to_datetime(millis))
}

/// 可空时间列 → Unix 毫秒；`NULL` 落成 `0`（= 未设置）。
fn millis_or_zero(value: Option<DateTime<Utc>>) -> i64 {
    value.map_or(0, |date| date.timestamp_millis())
}

/// 服务端的攻城视图 → `mir-store` 的行。
fn to_store_conquest(conquest: &ConquestRecord) -> mir_store::Conquest {
    mir_store::Conquest {
        conquest_index: conquest.conquest_index,
        owner_guild_id: conquest.owner,
        attacker_guild_id: conquest.attacker,
        gold_storage: i64::from(conquest.gold_storage),
        npc_rate: i16::from(conquest.npc_rate),
        war_is_on: conquest.war_is_on,
        war_start_time: optional_millis(conquest.war_start_time),
        war_end_time: optional_millis(conquest.war_end_time),
        start_type: i16::from(conquest.start_type),
        game_type: i16::from(conquest.game_type),
        schedule_start_hour: i16::from(conquest.schedule_start_hour),
        schedule_war_length: conquest.schedule_war_length,
        schedule_days: i16::from(conquest.schedule_days),
        king_points: conquest.king_points.clone(),
        flag_owners: conquest.flag_owners.clone(),
        next_war_at: optional_millis(conquest.next_war_at),
    }
}

/// `mir-store` 的攻城行 → 服务端视图。
fn from_store_conquest(conquest: mir_store::Conquest) -> ConquestRecord {
    ConquestRecord {
        conquest_index: conquest.conquest_index,
        owner: conquest.owner_guild_id,
        attacker: conquest.attacker_guild_id,
        gold_storage: u32::try_from(conquest.gold_storage).unwrap_or(u32::MAX),
        npc_rate: u8::try_from(conquest.npc_rate).unwrap_or(u8::MAX),
        war_is_on: conquest.war_is_on,
        war_start_time: millis_or_zero(conquest.war_start_time),
        war_end_time: millis_or_zero(conquest.war_end_time),
        start_type: u8::try_from(conquest.start_type).unwrap_or(0),
        game_type: u8::try_from(conquest.game_type).unwrap_or(0),
        schedule_start_hour: u8::try_from(conquest.schedule_start_hour).unwrap_or(0),
        schedule_war_length: conquest.schedule_war_length,
        schedule_days: u8::try_from(conquest.schedule_days).unwrap_or(0),
        king_points: conquest.king_points,
        flag_owners: conquest.flag_owners,
        next_war_at: millis_or_zero(conquest.next_war_at),
    }
}

// ---------------------------------------------------------------------------
// 市场：服务端视图 ↔ `mir-store` 行
// ---------------------------------------------------------------------------

/// `mir-store` 的市场挂单行 → 服务端视图。
fn from_store_auction(auction: mir_store::MarketAuction) -> AuctionRecord {
    AuctionRecord {
        id: u64::try_from(auction.id).unwrap_or(0),
        item: auction.item,
        consignment_date: from_datetime(Some(auction.consignment_date)),
        price: u32::try_from(auction.price).unwrap_or(0),
        current_bid: u32::try_from(auction.current_bid).unwrap_or(0),
        seller_character_id: auction.seller_character_id,
        seller_name: auction.seller_name,
        current_buyer_character_id: auction.current_buyer_character_id,
        current_buyer_name: auction.current_buyer_name,
        expired: auction.expired,
        sold: auction.sold,
        item_type: MarketItemType::from_value(u8::try_from(auction.item_type).unwrap_or(u8::MAX))
            .unwrap_or_default(),
    }
}

/// 服务端的市场挂单视图 → `mir-store` 的行。
fn to_store_auction(auction: &AuctionRecord) -> mir_store::MarketAuction {
    mir_store::MarketAuction {
        id: i64::try_from(auction.id).unwrap_or(0),
        item: auction.item.clone(),
        // 上架时间恒有值；未设置时落成纪元（`DateTime::<Utc>::default()`），不 panic。
        consignment_date: to_datetime(auction.consignment_date).unwrap_or_default(),
        price: i64::from(auction.price),
        current_bid: i64::from(auction.current_bid),
        seller_character_id: auction.seller_character_id,
        seller_name: auction.seller_name.clone(),
        current_buyer_character_id: auction.current_buyer_character_id,
        current_buyer_name: auction.current_buyer_name.clone(),
        expired: auction.expired,
        sold: auction.sold,
        item_type: i16::from(u8::from(auction.item_type)),
    }
}

// 返回类型写成 `impl Future + Send` 是为了满足 trait 的 Send 契约（见 `store.rs`），
// 因此这里不能用 `async fn` 语法糖。
#[allow(clippy::manual_async_fn)]
impl AccountStore for PgAccountStore {
    fn create_account<'a>(
        &'a self,
        username: &'a str,
        password: &'a str,
        email: &'a str,
    ) -> impl std::future::Future<Output = Result<Account, StoreError>> + Send + 'a {
        async move {
            let mut new = mir_store::NewAccount::new(username, password);
            new.email = email.to_owned();
            let account = mir_store::create_account(&self.pool, &new)
                .await
                .map_err(map_store_error)?;
            Ok(from_store_account(account))
        }
    }

    fn find_account<'a>(
        &'a self,
        username: &'a str,
    ) -> impl std::future::Future<Output = Result<Option<Account>, StoreError>> + Send + 'a {
        async move {
            let account = mir_store::find_account(&self.pool, username)
                .await
                .map_err(map_store_error)?;
            Ok(account.map(from_store_account))
        }
    }

    fn create_character<'a>(
        &'a self,
        account_id: i64,
        name: &'a str,
        class: MirClass,
        gender: MirGender,
        map: &'a str,
        point: (i32, i32),
    ) -> impl std::future::Future<Output = Result<Character, StoreError>> + Send + 'a {
        async move {
            let new = mir_store::NewCharacter::new(
                account_id,
                name,
                i16::from(class as u8),
                i16::from(gender as u8),
            );
            let mut row = mir_store::create_character(&self.pool, &new)
                .await
                .map_err(map_store_error)?;
            // 出生点由服务器决定：建号接口只写姓名/职业/性别（与 Crystal 一致）。
            row.current_map_index = map.parse::<i32>().unwrap_or(0);
            row.current_x = point.0;
            row.current_y = point.1;
            row.level = 1;
            row.hp = 15;
            row.mp = 15;
            row.direction = i16::from(MirDirection::Down as u8);
            mir_store::save_character(&self.pool, &row)
                .await
                .map_err(map_store_error)?;
            Ok(from_store_character(row))
        }
    }

    fn list_characters(
        &self,
        account_id: i64,
    ) -> impl std::future::Future<Output = Result<Vec<Character>, StoreError>> + Send + '_ {
        async move {
            let summaries = mir_store::list_characters(&self.pool, account_id)
                .await
                .map_err(map_store_error)?;
            let mut characters = Vec::with_capacity(summaries.len());
            for summary in summaries {
                if let Ok(row) = mir_store::load_character(&self.pool, summary.id).await {
                    characters.push(from_store_character(row));
                }
            }
            Ok(characters)
        }
    }

    fn find_character(
        &self,
        account_id: i64,
        character_id: i64,
    ) -> impl std::future::Future<Output = Result<Option<Character>, StoreError>> + Send + '_ {
        async move {
            match mir_store::load_character(&self.pool, character_id).await {
                Ok(row) if row.account_id == account_id => Ok(Some(from_store_character(row))),
                Ok(_) | Err(mir_store::StoreError::NotFound(_)) => Ok(None),
                Err(error) => Err(map_store_error(error)),
            }
        }
    }

    fn delete_character(
        &self,
        account_id: i64,
        character_id: i64,
    ) -> impl std::future::Future<Output = Result<bool, StoreError>> + Send + '_ {
        async move {
            match mir_store::delete_character(&self.pool, character_id, account_id).await {
                Ok(()) => Ok(true),
                Err(mir_store::StoreError::NotFound(_)) => Ok(false),
                Err(error) => Err(map_store_error(error)),
            }
        }
    }

    fn save_character(
        &self,
        character: Character,
    ) -> impl std::future::Future<Output = Result<(), StoreError>> + Send + '_ {
        async move {
            let mut row = match mir_store::load_character(&self.pool, character.id).await {
                Ok(row) => row,
                Err(mir_store::StoreError::NotFound(_)) => {
                    return Err(StoreError::Internal(format!(
                        "character {} not found; cannot save",
                        character.id
                    )));
                }
                Err(error) => return Err(map_store_error(error)),
            };
            apply_to_store_character(&character, &mut row);
            mir_store::save_character(&self.pool, &row)
                .await
                .map_err(map_store_error)
        }
    }

    fn set_language(
        &self,
        account_id: i64,
        language: String,
    ) -> impl std::future::Future<Output = Result<(), StoreError>> + Send + '_ {
        async move {
            mir_store::set_account_language(&self.pool, account_id, &language)
                .await
                .map_err(map_store_error)
        }
    }

    fn save_guild(
        &self,
        guild: GuildRecord,
    ) -> impl std::future::Future<Output = Result<i32, StoreError>> + Send + '_ {
        async move {
            // 一条 `guilds` 语句 + 两张子表的批量替换 → 放同一个事务里。
            let mut tx = self
                .pool
                .begin()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;
            let row = to_store_guild(&guild);
            let id = if row.id > 0 {
                match mir_store::save_guild(&mut tx, &row).await {
                    Ok(()) => row.id,
                    // 有主键但库里没有这一行（例如内存里的行会还没落过盘）：改成插入。
                    Err(mir_store::StoreError::NotFound(_)) => {
                        mir_store::create_guild(&mut tx, &row)
                            .await
                            .map_err(map_store_error)?
                            .id
                    }
                    Err(error) => return Err(map_store_error(error)),
                }
            } else {
                mir_store::create_guild(&mut tx, &row)
                    .await
                    .map_err(map_store_error)?
                    .id
            };
            tx.commit()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;
            Ok(id)
        }
    }

    fn delete_guild(
        &self,
        guild_id: i32,
    ) -> impl std::future::Future<Output = Result<(), StoreError>> + Send + '_ {
        async move {
            let mut connection = self
                .pool
                .acquire()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;
            mir_store::delete_guild(&mut connection, guild_id)
                .await
                .map_err(map_store_error)
        }
    }

    fn list_guilds(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<GuildRecord>, StoreError>> + Send + '_ {
        async move {
            let mut connection = self
                .pool
                .acquire()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;
            let guilds = mir_store::list_guilds(&mut connection)
                .await
                .map_err(map_store_error)?;
            Ok(guilds.into_iter().map(from_store_guild).collect())
        }
    }

    fn set_character_guild(
        &self,
        character_id: i64,
        guild_id: i32,
    ) -> impl std::future::Future<Output = Result<(), StoreError>> + Send + '_ {
        async move {
            let mut connection = self
                .pool
                .acquire()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;
            mir_store::set_character_guild(&mut connection, character_id, guild_id)
                .await
                .map_err(map_store_error)
        }
    }

    fn find_character_by_name<'a>(
        &'a self,
        name: &'a str,
    ) -> impl std::future::Future<Output = Result<Option<Character>, StoreError>> + Send + 'a {
        async move {
            let character = mir_store::find_character_by_name(&self.pool, name)
                .await
                .map_err(map_store_error)?;
            Ok(character.map(from_store_character))
        }
    }

    fn create_mail(
        &self,
        mail: MailRecord,
    ) -> impl std::future::Future<Output = Result<MailRecord, StoreError>> + Send + '_ {
        async move {
            let mut connection = self
                .pool
                .acquire()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;
            let row = to_store_mail(&mail);
            let stored = mir_store::create_mail(&mut connection, &row)
                .await
                .map_err(map_store_error)?;
            Ok(from_store_mail(stored))
        }
    }

    fn list_mail(
        &self,
        character_id: i64,
    ) -> impl std::future::Future<Output = Result<Vec<MailRecord>, StoreError>> + Send + '_ {
        async move {
            let mut connection = self
                .pool
                .acquire()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;
            let mails = mir_store::list_mail(&mut connection, character_id)
                .await
                .map_err(map_store_error)?;
            Ok(mails.into_iter().map(from_store_mail).collect())
        }
    }

    fn save_mail(
        &self,
        mail: MailRecord,
    ) -> impl std::future::Future<Output = Result<(), StoreError>> + Send + '_ {
        async move {
            let mut connection = self
                .pool
                .acquire()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;
            let row = to_store_mail(&mail);
            mir_store::save_mail(&mut connection, &row)
                .await
                .map_err(map_store_error)
        }
    }

    fn delete_mail(
        &self,
        mail_id: u64,
    ) -> impl std::future::Future<Output = Result<(), StoreError>> + Send + '_ {
        async move {
            let mut connection = self
                .pool
                .acquire()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;
            let id = i64::try_from(mail_id)
                .map_err(|_| StoreError::Internal(format!("mail id {mail_id} exceeds i64")))?;
            mir_store::delete_mail(&mut connection, id)
                .await
                .map_err(map_store_error)
        }
    }

    fn list_conquests(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<ConquestRecord>, StoreError>> + Send + '_
    {
        async move {
            let mut connection = self
                .pool
                .acquire()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;
            let rows = mir_store::list_conquests(&mut connection)
                .await
                .map_err(map_store_error)?;
            Ok(rows.into_iter().map(from_store_conquest).collect())
        }
    }

    fn save_conquest(
        &self,
        conquest: ConquestRecord,
    ) -> impl std::future::Future<Output = Result<(), StoreError>> + Send + '_ {
        async move {
            let mut connection = self
                .pool
                .acquire()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;
            let row = to_store_conquest(&conquest);
            mir_store::save_conquest(&mut connection, &row)
                .await
                .map_err(map_store_error)
        }
    }

    fn load_auctions(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<AuctionRecord>, StoreError>> + Send + '_ {
        async move {
            let mut connection = self
                .pool
                .acquire()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;
            let rows = mir_store::list_auctions(&mut connection)
                .await
                .map_err(map_store_error)?;
            Ok(rows.into_iter().map(from_store_auction).collect())
        }
    }

    fn save_auctions(
        &self,
        auctions: Vec<AuctionRecord>,
    ) -> impl std::future::Future<Output = Result<(), StoreError>> + Send + '_ {
        async move {
            // 全表替换 = DELETE + 多条 INSERT，必须同一个事务：中途失败要回滚，
            // 否则「旧行已删、新行没插」会让市场凭空清空。
            let mut tx = self
                .pool
                .begin()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;
            let rows: Vec<mir_store::MarketAuction> =
                auctions.iter().map(to_store_auction).collect();
            mir_store::save_auctions(&mut tx, &rows)
                .await
                .map_err(map_store_error)?;
            tx.commit()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))
        }
    }

    fn apply_market_update(
        &self,
        update: MarketUpdate,
    ) -> impl std::future::Future<Output = Result<(), StoreError>> + Send + '_ {
        async move {
            let MarketUpdate {
                character,
                mail_writes,
                auctions,
            } = update;
            // 角色行 + 邮件 + 挂单同一事务（F2/F3）：任何一步失败都整组回滚。
            let mut tx = self
                .pool
                .begin()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;

            if let Some(character) = character {
                let mut row = mir_store::load_character(&mut *tx, character.id)
                    .await
                    .map_err(map_store_error)?;
                apply_to_store_character(&character, &mut row);
                mir_store::save_character(&mut *tx, &row)
                    .await
                    .map_err(map_store_error)?;
            }

            for write in mail_writes {
                match write {
                    crate::mail::MailWrite::Create(mail) => {
                        let row = to_store_mail(&mail);
                        mir_store::create_mail(&mut tx, &row)
                            .await
                            .map_err(map_store_error)?;
                    }
                    crate::mail::MailWrite::Save(mail) => {
                        let row = to_store_mail(&mail);
                        mir_store::save_mail(&mut tx, &row)
                            .await
                            .map_err(map_store_error)?;
                    }
                    crate::mail::MailWrite::Delete { mail_id } => {
                        let id = i64::try_from(mail_id).map_err(|_| {
                            StoreError::Internal(format!("mail id {mail_id} exceeds i64"))
                        })?;
                        mir_store::delete_mail(&mut tx, id)
                            .await
                            .map_err(map_store_error)?;
                    }
                }
            }

            let rows: Vec<mir_store::MarketAuction> =
                auctions.iter().map(to_store_auction).collect();
            mir_store::save_auctions(&mut tx, &rows)
                .await
                .map_err(map_store_error)?;

            tx.commit()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))
        }
    }

    fn load_game_shop_stock(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<GameShopStockRecord>, StoreError>> + Send + '_
    {
        async move {
            let mut connection = self
                .pool
                .acquire()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;
            let rows = mir_store::list_gameshop_stock(&mut *connection)
                .await
                .map_err(map_store_error)?;
            Ok(rows
                .into_iter()
                .map(|row| GameShopStockRecord {
                    g_index: row.g_index,
                    purchased: row.purchased,
                })
                .collect())
        }
    }

    fn load_game_shop_purchases(
        &self,
        character_id: i64,
    ) -> impl std::future::Future<Output = Result<Vec<GameShopPurchaseRecord>, StoreError>> + Send + '_
    {
        async move {
            let mut connection = self
                .pool
                .acquire()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;
            let rows = mir_store::list_gameshop_purchases(&mut *connection, character_id)
                .await
                .map_err(map_store_error)?;
            Ok(rows
                .into_iter()
                .map(|row| GameShopPurchaseRecord {
                    character_id: row.character_id,
                    g_index: row.g_index,
                    quantity: row.quantity,
                })
                .collect())
        }
    }

    fn apply_gameshop_update(
        &self,
        update: GameShopUpdate,
    ) -> impl std::future::Future<Output = Result<(), StoreError>> + Send + '_ {
        async move {
            let GameShopUpdate {
                character,
                mail_writes,
                purchases,
                stock,
            } = update;
            // 角色行（含元宝）+ 商城邮件 + 两组已购计数同一事务：任何一步失败都整组回滚，
            // 不会出现「扣了元宝没写库存」或「发了货没扣钱」。
            let mut tx = self
                .pool
                .begin()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))?;

            if let Some(character) = character {
                let mut row = mir_store::load_character(&mut *tx, character.id)
                    .await
                    .map_err(map_store_error)?;
                apply_to_store_character(&character, &mut row);
                mir_store::save_character(&mut *tx, &row)
                    .await
                    .map_err(map_store_error)?;
            }

            for write in mail_writes {
                match write {
                    crate::mail::MailWrite::Create(mail) => {
                        let row = to_store_mail(&mail);
                        mir_store::create_mail(&mut tx, &row)
                            .await
                            .map_err(map_store_error)?;
                    }
                    crate::mail::MailWrite::Save(mail) => {
                        let row = to_store_mail(&mail);
                        mir_store::save_mail(&mut tx, &row)
                            .await
                            .map_err(map_store_error)?;
                    }
                    crate::mail::MailWrite::Delete { mail_id } => {
                        let id = i64::try_from(mail_id).map_err(|_| {
                            StoreError::Internal(format!("mail id {mail_id} exceeds i64"))
                        })?;
                        mir_store::delete_mail(&mut tx, id)
                            .await
                            .map_err(map_store_error)?;
                    }
                }
            }

            let stock_rows: Vec<mir_store::GameShopStock> = stock
                .iter()
                .map(|row| mir_store::GameShopStock {
                    g_index: row.g_index,
                    purchased: row.purchased,
                })
                .collect();
            mir_store::save_gameshop_stock(&mut tx, &stock_rows)
                .await
                .map_err(map_store_error)?;

            let purchase_rows: Vec<mir_store::GameShopPurchase> = purchases
                .iter()
                .map(|row| mir_store::GameShopPurchase {
                    character_id: row.character_id,
                    g_index: row.g_index,
                    quantity: row.quantity,
                })
                .collect();
            mir_store::save_gameshop_purchases(&mut tx, &purchase_rows)
                .await
                .map_err(map_store_error)?;

            tx.commit()
                .await
                .map_err(|error| StoreError::Internal(error.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datetime_roundtrip_handles_unset() {
        assert!(to_datetime(DateTimeBinary(0)).is_none());
        let value = DateTimeBinary::from_unix_seconds(1_704_067_200);
        let date = to_datetime(value).expect("合法时间");
        assert_eq!(date.timestamp(), 1_704_067_200);
        assert_eq!(from_datetime(Some(date)), value);
    }
}
