//! 单个连接的会话状态机。
//!
//! 阶段与 Crystal 的 `GameStage` 对齐：`None`（刚连上，等版本包）→ `Login`
//! （可注册/登录）→ `Select`（已登录，可建/删/选角）→ `Game`（在世界里）。
//! 每个处理函数都自己判定阶段，与 Crystal 一致：阶段不对就静默丢弃。

use std::sync::Arc;

use mir_core::enums::{
    ChatType, HeroBehaviour, IntelligentCreatureType, LightSetting, MirClass, MirDirection,
    MirGridType, PanelType, WeatherSetting,
};
use mir_core::i18n::LanguageLibrary;
use mir_core::{DateTimeBinary, LanguageCatalog, Point, TextKey};
use mir_protocol::data::{ClientMagic, SelectInfo, UserItem};
use mir_protocol::manual::server::{MapInformation, UserInformation};
use mir_protocol::packets::server::{
    ClientVersion as ClientVersionResponse, Connected, DeleteCharacter as DeleteCharacterResult,
    DeleteCharacterSuccess, LogOutSuccess, Login as LoginResult, LoginBanned, LoginSuccess,
    NewAccount as NewAccountResult, NewCharacter as NewCharacterResult, NewCharacterSuccess,
    StartGame,
};
use mir_protocol::{ClientPacket, ServerPacket};
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;

use crate::config::ServerConfig;
use crate::items::{
    decode_equipment, decode_inventory, encode_slots, new_equipment, new_inventory,
};
use crate::magic::{decode_magics, encode_magics};
use crate::npc::NpcState;
use crate::quests::{decode_quests, encode_quests};
use crate::store::{Account, AccountStore, Character, StoreError};
use crate::world::{Player, World};

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
    /// 协议错误。
    #[error("协议错误：{0}")]
    Protocol(#[from] mir_protocol::ProtocolError),
    /// 存储错误。
    #[error("存储错误：{0}")]
    Store(#[from] StoreError),
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
    pub async fn handle(&mut self, packet: ClientPacket) -> Result<(), SessionError> {
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
            ClientPacket::Chat(packet) => self.on_chat(&packet.message).await,
            ClientPacket::PickUp(_) => self.on_pickup().await,
            ClientPacket::DropItem(packet) => {
                self.on_drop_item(packet.unique_id, packet.count).await;
            }
            ClientPacket::MoveItem(packet) => {
                self.on_move_item(packet.grid, packet.from, packet.to).await;
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
            ClientPacket::Disconnect(_) => {
                self.leave_world().await;
            }
            _ => {
                tracing::debug!(session = self.id, "忽略未实现的客户端包");
            }
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

        let mut world = self.world.lock().await;
        // 读档里出现过的 UUID 要计入发生器，否则新掉的物品可能撞号
        // （Crystal 用账号库里的 `NextUserItemID` 做同一件事，`Envir.cs:127,2579`）。
        world.observe_item_uids(inventory.iter().chain(equipment.iter()).flatten());
        let object_id = world.allocate_object_id();
        let position = Point::new(character.x, character.y);
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
            visible_monsters: std::collections::BTreeSet::new(),
            visible_npcs: std::collections::BTreeSet::new(),
            movement_time: 0,
            inventory: inventory.clone(),
            equipment: equipment.clone(),
            magics: magics.clone(),
            magic: crate::magic::MagicState::default(),
            gold: character.gold,
            credit: character.credit,
            visible_items: std::collections::BTreeSet::new(),
            quests: quest_save.quests.clone(),
            completed_quests: quest_save.completed_quests.clone(),
            // buff 不持久化（Crystal 的 `CharacterInfo` 存档里没有 buff 块），登录即空。
            buffs: Vec::new(),
            pot_health_amount: 0,
            pot_mana_amount: 0,
            pot_time: 0,
            unlock_curse: false,
            npc: NpcState::default(),
            sender: self.out.clone(),
        });
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
        )));
        // 任务表（`PlayerObject.cs:1185-1212`）：全套 `S.NewQuestInfo` → `S.CompleteQuest` →
        // 每条已接任务的 `S.ChangeQuest{Add}`。
        for packet in quest_packets {
            self.send(packet);
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

    /// 接任务（`C.AcceptQuest`，Crystal `PlayerObject.AcceptQuest`，`PlayerObject.cs:11251-11348`）。
    async fn on_accept_quest(&mut self, npc_index: u32, quest_index: i32) {
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
    /// 本作没有组队系统，`World::share_quest` 会走 Crystal 的失败分支（系统提示）。
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
        let mut world = self.world.lock().await;
        match world.chat(object_id, message) {
            Ok(receivers) => {
                tracing::debug!(session = self.id, receivers, "聊天广播");
            }
            Err(error) => tracing::debug!(session = self.id, %error, "聊天失败"),
        }
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
        let leaving = {
            let mut world = self.world.lock().await;
            world.leave(object_id)
        };
        if let Some(player) = leaving {
            if let Some(character) = &mut self.character {
                character.inventory = encode_slots(&player.inventory);
                character.equipment = encode_slots(&player.equipment);
                // 技能块：`CharacterInfo.Save` 的「`Magics.Count` + 逐条 `UserMagic.Save`」
                // （`CharacterInfo.cs:458-463`）。
                character.magics = encode_magics(&player.magics);
                // 任务块：`CharacterInfo.Save` 的「`CurrentQuests.Count` + 逐条」与
                // 「`CompletedQuests.Count` + 逐条」（`CharacterInfo.cs:488-492`、`:514-518`）。
                character.quests = encode_quests(&crate::quests::QuestSave {
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
                // 切图后角色要在新图上重登，地图名必须一起落盘
                // （Crystal `CharacterInfo` 存的是当前地图，`Envir.SaveAccounts`）。
                character.map.clone_from(&player.map);
            }
        }
        if let Some(character) = self.character.clone() {
            if let Err(error) = self.store.save_character(character).await {
                tracing::warn!(session = self.id, %error, "下线存盘失败");
            }
        }
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
        quest_inventory: Some(vec![None; 40]),
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
        assert!(is_valid_account_id("diana"));
        assert!(!is_valid_account_id("di"), "至少 3 位");
        assert!(!is_valid_account_id("diana_1"), "只允许字母数字");
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
