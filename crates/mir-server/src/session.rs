//! 单个连接的会话状态机。
//!
//! 阶段与 Crystal 的 `GameStage` 对齐：`None`（刚连上，等版本包）→ `Login`
//! （可注册/登录）→ `Select`（已登录，可建/删/选角）→ `Game`（在世界里）。
//! 每个处理函数都自己判定阶段，与 Crystal 一致：阶段不对就静默丢弃。

use std::sync::Arc;

use mir_core::enums::{
    ChatType, HeroBehaviour, IntelligentCreatureType, LightSetting, MirClass, MirDirection,
    WeatherSetting,
};
use mir_core::{DateTimeBinary, LanguageCatalog, Point, TextKey};
use mir_protocol::data::SelectInfo;
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
        }
    }

    /// 当前阶段。
    #[must_use]
    pub fn stage(&self) -> Stage {
        self.stage
    }

    fn send(&self, packet: ServerPacket) {
        let frame = packet.encode_frame();
        let _ = self.out.send(frame);
    }

    /// 连接建立：Crystal 先发 `Connected`（`MirConnection.cs:97`）。
    pub fn on_connected(&mut self) {
        self.send(ServerPacket::Connected(Connected {}));
    }

    /// 处理一个客户端包。
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
            ClientPacket::Chat(packet) => self.on_chat(&packet.message).await,
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

        let mut world = self.world.lock().await;
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
            class: character.class,
            gender: character.gender,
            hair: character.hair,
            hp: character.hp,
            mp: character.mp,
            sender: self.out.clone(),
        });
        drop(world);

        self.object_id = Some(object_id);
        self.stage = Stage::Game;

        // Crystal 的进世界序列：StartGame{Result=4} → NewItemInfo → MapInformation → UserInformation。
        self.send(ServerPacket::StartGame(StartGame {
            result: 4,
            resolution: self.config.allowed_resolution,
        }));
        self.send(ServerPacket::MapInformation(MapInformation {
            map_index: 0,
            file_name: character.map.clone(),
            title: "比奇省".to_owned(),
            mini_map: 0,
            big_map: 0,
            lights: LightSetting::Day,
            lightning: false,
            fire: false,
            map_dark_light: 0,
            music: 0,
            weather_particles: WeatherSetting::None,
        }));
        self.send(ServerPacket::UserInformation(user_information(
            object_id, &character, position,
        )));
        let welcome = self.language.text(TextKey::Welcome);
        self.send_system_chat(welcome);
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
        let mut world = self.world.lock().await;
        if let Err(error) = world.turn(object_id, direction) {
            tracing::debug!(session = self.id, %error, "转身失败");
        }
    }

    async fn on_move(&mut self, direction: MirDirection, running: bool) {
        let Some(object_id) = self.object_id else {
            return;
        };
        let position = {
            let mut world = self.world.lock().await;
            match world.walk(object_id, direction, running) {
                Ok(position) => position,
                Err(error) => {
                    tracing::debug!(session = self.id, %error, "移动被拒绝");
                    return;
                }
            }
        };
        if let Some(character) = &mut self.character {
            character.x = position.x;
            character.y = position.y;
            character.direction = direction;
        }
    }

    async fn on_chat(&mut self, message: &str) {
        // 游戏内语言切换：`/language <代码>`（协议里没有对应包，是我们的扩展）。
        if message.starts_with("/language") {
            let reply = self.language.handle_command(message);
            if let Some(text) = reply {
                self.send_system_chat(text);
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
    pub async fn leave_world(&mut self) {
        let Some(object_id) = self.object_id.take() else {
            return;
        };
        let mut world = self.world.lock().await;
        world.leave(object_id);
        drop(world);
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

fn user_information(object_id: u32, character: &Character, position: Point) -> UserInformation {
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
        inventory: Some(vec![None; 46]),
        equipment: Some(vec![None; 14]),
        quest_inventory: Some(vec![None; 40]),
        gold: character.gold,
        credit: 0,
        has_expanded_storage: false,
        has_storage_password: false,
        require_storage_password: false,
        storage_password_last_set: DateTimeBinary(0),
        expanded_storage_expiry_time: DateTimeBinary(0),
        magics: Vec::new(),
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
