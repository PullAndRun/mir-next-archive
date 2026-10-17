//! 客户端网络会话：连接 Crystal 服务端、走完登录链路、维护世界快照。
//!
//! # 为什么用阻塞 socket + 线程
//!
//! `mir-client` 的窗口是 winit 的单线程事件循环，网络只需要一个后台线程：
//! [`std::net::TcpStream`] 设 20 ms 读超时，线程每轮先处理 UI 通过 channel 发来的
//! [`NetCommand`]，再读一批服务器包；UI 侧用 [`NetClient::try_recv`] 取最新快照。
//! 这样没有 tokio runtime 也能跑，端到端测试同样可以直接驱动（见
//! `crates/mir-client/tests/live_world.rs`）。`mir-client` 的 `tokio` 依赖只用于测试里
//! 在进程内启动真实服务端。
//!
//! # 登录包序（与 `mir-server/src/session.rs` 对齐）
//!
//! ```text
//! TCP 连接
//!   ← S.Connected
//!   → C.ClientVersion { 16 字节占位哈希 }      // 服务端 check_version=false
//!   ← S.ClientVersion { result = 1 }
//!   → C.Login { account, password }
//!   ← S.Login { result = 3 }                   // 账号不存在
//!   → C.NewAccount { ...空资料... }            // 默认自动注册，--no-register 可关
//!   ← S.NewAccount { result = 8 }
//!   → C.Login { account, password }
//!   ← S.LoginSuccess { characters }
//!   → C.StartGame { character_index }          // 角色不存在时先 C.NewCharacter
//!   ← S.NewCharacterSuccess / S.NewCharacter
//!   ← S.StartGame { result = 4 }
//!   ← S.MapInformation
//!   ← S.NewMapInfo（服务端有数据库时）
//!   ← S.UserInformation（自己的对象 ID / 坐标 / HP/MP）
//! ```
//!
//! 详细语义见 `docs/protocol/login-flow.md`；这个模块只实现客户端侧的状态机，
//! 遇到任何阶段错误都通过 [`NetEvent::Error`] 抛出，不静默卡住。
//!
//! # 快照
//!
//! [`WorldSnapshot`] 是渲染层的唯一输入：它由 `ServerPacket` 增量更新，客户端每批
//! 包处理完把整个快照克隆给 UI。快照里的 `packet_counts` 记录收到过的包种类，
//! 端到端测试用它断言 `UserInformation`、`ObjectMonster`、`UserLocation` 都到了。

use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use mir_core::Point;
use mir_core::enums::{ChatType, MirClass, MirDirection, MirGender};
use mir_protocol::codec::FrameBuffer;
use mir_protocol::packets::client as c;
use mir_protocol::{ClientPacket, DateTimeBinary, ServerPacket};
use mir_render::{EntityKind, EntityView};

/// 连接超时。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// 单次 `read` 的超时：决定 UI 命令（走路/聊天）的最大延迟。
const READ_TIMEOUT: Duration = Duration::from_millis(20);
/// `KeepAlive` 间隔（Crystal 客户端在没有待发包时定期发送）。
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);
/// 聊天记录上限。
const CHAT_LOG_LIMIT: usize = 100;
/// 客户端版本哈希的占位值：Crystal 发的是客户端 exe 的 MD5（16 字节），
/// 服务端 `check_version = false` 时内容不重要，但长度保持 16 字节。
const VERSION_HASH: &[u8] = b"mir-next-client!";

/// 网络会话阶段。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NetStatus {
    /// 后台线程正在连接。
    #[default]
    Connecting,
    /// 已连上，等版本校验。
    Verifying,
    /// 正在登录/注册。
    LoggingIn,
    /// 已登录，正在选角/建角。
    SelectingCharacter,
    /// 正在进入世界。
    StartingGame,
    /// 已进入世界（`StartGame{4}` 已收到）。
    InGame,
    /// 服务器或本地关闭了连接。
    Closed,
    /// 登录/连接失败。
    Failed,
}

impl NetStatus {
    /// 可读名称（日志/叠层用）。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Connecting => "连接中",
            Self::Verifying => "版本校验",
            Self::LoggingIn => "登录中",
            Self::SelectingCharacter => "选择角色",
            Self::StartingGame => "进入世界",
            Self::InGame => "已进入世界",
            Self::Closed => "已断开",
            Self::Failed => "失败",
        }
    }
}

/// 创建角色的参数（对应 `--create-character --class --gender`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateCharacter {
    /// 角色名。
    pub name: String,
    /// 职业。
    pub class: MirClass,
    /// 性别。
    pub gender: MirGender,
}

/// 连接参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectOptions {
    /// 服务器地址（`host:port`，默认 `127.0.0.1:7000`）。
    pub server: String,
    /// 账号。
    pub account: String,
    /// 口令。
    pub password: String,
    /// 角色名。
    pub character: String,
    /// 角色不存在时是否自动创建。
    pub create: Option<CreateCharacter>,
    /// 账号不存在时是否自动注册（默认 `true`；`--no-register` 关闭）。
    pub auto_register: bool,
}

/// UI → 网络线程的命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetCommand {
    /// 转身（`C.Turn`）。
    Turn(MirDirection),
    /// 走一步（`C.Walk`）。
    Walk(MirDirection),
    /// 跑两步（`C.Run`）。
    Run(MirDirection),
    /// 物理攻击（`C.Attack`）。
    Attack(MirDirection),
    /// 捡起脚下物品（`C.PickUp`）。
    PickUp,
    /// 聊天（`C.Chat`）。
    Chat(String),
    /// 关闭连接。
    Quit,
}

/// 网络线程 → UI 的通知。
#[derive(Debug, Clone)]
pub enum NetEvent {
    /// 世界快照更新（登录阶段也会发，`status` 在快照里）。
    ///
    /// 装箱是为了避免每个 `NetEvent`（包括错误/关闭）都被快照撑大。
    Snapshot(Box<WorldSnapshot>),
    /// 致命错误（随后还会有 `Closed`）。
    Error(String),
    /// 连接已关闭。
    Closed(String),
}

/// 地图信息（来自 `S.MapInformation` / `S.NewMapInfo` / `S.MapChanged`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MapSnapshot {
    /// 地图编号。
    pub map_index: i32,
    /// 地图文件名（不含 `.map`，对应 `resource/map/<name>.map`）。
    pub file_name: String,
    /// 地图标题。
    pub title: String,
    /// 宽（`NewMapInfo` 才有；`MapInformation` 里没有）。
    pub width: i32,
    /// 高。
    pub height: i32,
}

/// 自己的角色状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfPlayer {
    /// 对象 ID。
    pub object_id: u32,
    /// 名字。
    pub name: String,
    /// 坐标。
    pub position: Point,
    /// 朝向。
    pub direction: MirDirection,
    /// 当前 HP。
    pub hp: i32,
    /// 当前 MP。
    pub mp: i32,
    /// 等级。
    pub level: u16,
    /// 职业。
    pub class: MirClass,
    /// 性别。
    pub gender: MirGender,
    /// 发型（暂未用于绘制）。
    pub hair: u8,
    /// 金币。
    pub gold: u32,
    /// 是否死亡。
    pub dead: bool,
}

/// 世界里的其他对象。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldEntity {
    /// 对象 ID。
    pub object_id: u32,
    /// 类型。
    pub kind: EntityKind,
    /// 名字。
    pub name: String,
    /// 坐标。
    pub position: Point,
    /// 朝向。
    pub direction: MirDirection,
    /// 图像号（含义随 `kind` 变化，见 `mir_render::EntityView`）。
    pub image: u16,
    /// 是否死亡。
    pub dead: bool,
    /// 生命百分比（`S.ObjectHealth`；怪物才有）。
    pub hp_percent: Option<u8>,
}

/// 一条聊天记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatLine {
    /// 说话者；`None` 表示系统消息。
    pub speaker: Option<String>,
    /// 内容。
    pub text: String,
    /// 频道。
    pub kind: ChatType,
}

impl ChatLine {
    /// 单行展示文本（控制台日志用；没有字体时 UI 不画）。
    #[must_use]
    pub fn display(&self) -> String {
        match &self.speaker {
            Some(speaker) => format!("[{speaker}] {}", self.text),
            None => self.text.clone(),
        }
    }
}

/// 客户端看到的世界。
#[derive(Debug, Clone)]
pub struct WorldSnapshot {
    /// 会话阶段。
    pub status: NetStatus,
    /// 当前地图。
    pub map: Option<MapSnapshot>,
    /// 自己。
    pub self_player: Option<SelfPlayer>,
    /// 其他对象（玩家/怪物/NPC/地面物品），按对象 ID 排序。
    pub entities: BTreeMap<u32, WorldEntity>,
    /// 聊天记录（最新在末尾，最多 [`CHAT_LOG_LIMIT`] 条）。
    pub chat: Vec<ChatLine>,
    /// NPC 对话页（`S.NPCResponse` 的原文行）。
    pub npc_dialog: Option<Vec<String>>,
    /// 最近一次商店商品数量（`S.NPCGoods`，没有 UI，只记数量）。
    pub npc_goods: Option<usize>,
    /// 收到过的包种类 → 次数。
    pub packet_counts: BTreeMap<&'static str, u32>,
    /// 解析失败的包数量。
    pub decode_errors: u32,
    /// 最近一次错误（登录失败、服务器断开原因等）。
    pub last_error: Option<String>,
}

impl Default for WorldSnapshot {
    fn default() -> Self {
        Self {
            status: NetStatus::Connecting,
            map: None,
            self_player: None,
            entities: BTreeMap::new(),
            chat: Vec::new(),
            npc_dialog: None,
            npc_goods: None,
            packet_counts: BTreeMap::new(),
            decode_errors: 0,
            last_error: None,
        }
    }
}

impl WorldSnapshot {
    /// 当前是否已经进入世界且拿到了自己的 `UserInformation`。
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.status == NetStatus::InGame && self.self_player.is_some()
    }

    /// 某个包收到过多少次（测试/调试用）。
    #[must_use]
    pub fn packet_count(&self, name: &str) -> u32 {
        self.packet_counts.get(name).copied().unwrap_or(0)
    }

    /// 转成渲染层的实体列表：自己 + 其他对象。
    ///
    /// 自己的外观暂时固定为 0 号身体（`UserInformation` 不带装备外观；等号角/装备
    /// 外观表接进来后再填）。自己的实体 ID 如果同时也出现在 `entities` 里，会被跳过，
    /// 避免画两遍。
    #[must_use]
    pub fn entity_views(&self) -> Vec<EntityView> {
        let mut views = Vec::with_capacity(self.entities.len() + 1);
        let self_id = self.self_player.as_ref().map(|me| me.object_id);
        if let Some(me) = &self.self_player {
            views.push(EntityView {
                position: me.position,
                direction: me.direction,
                kind: EntityKind::Player,
                image: 0,
                name: Some(me.name.clone()),
                dead: me.dead,
                hp_percent: None,
            });
        }
        for (object_id, entity) in &self.entities {
            if Some(*object_id) == self_id {
                continue;
            }
            views.push(EntityView {
                position: entity.position,
                direction: entity.direction,
                kind: entity.kind,
                image: entity.image,
                name: Some(entity.name.clone()),
                dead: entity.dead,
                hp_percent: entity.hp_percent,
            });
        }
        views
    }
}

/// 客户端会话句柄。
///
/// 创建时启动一个后台线程；线程连接失败、登录被拒、服务器断开都会通过
/// [`NetEvent`] 通知。`Drop` 会发送 `Quit` 并等线程退出（读超时 20 ms，最多等
/// 一次连接超时）。
pub struct NetClient {
    commands: Sender<NetCommand>,
    events: Receiver<NetEvent>,
    thread: Option<JoinHandle<()>>,
}

impl NetClient {
    /// 启动后台会话线程。
    ///
    /// 返回的句柄不代表已经连上；用 [`NetClient::wait_until_in_game`] 等登录链路走完。
    ///
    /// # Errors
    /// 线程创建失败时返回错误（连接错误通过事件通知）。
    pub fn connect(options: ConnectOptions) -> Result<Self> {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("mir-client-net".to_owned())
            .spawn(move || {
                tracing::info!(server = %options.server, account = %options.account, "开始联机会话");
                let result = session(&options, &command_rx, &event_tx);
                match result {
                    Ok(()) => {
                        let _ = event_tx.send(NetEvent::Closed("连接已关闭".to_owned()));
                    }
                    Err(error) => {
                        let message = format!("{error:#}");
                        tracing::warn!(%message, "联机会话失败");
                        let _ = event_tx.send(NetEvent::Error(message.clone()));
                        let _ = event_tx.send(NetEvent::Closed(message));
                    }
                }
            })
            .context("启动网络线程失败")?;
        Ok(Self {
            commands: command_tx,
            events: event_rx,
            thread: Some(thread),
        })
    }

    /// 给网络线程发一个命令；线程已经退出时返回 `false`。
    #[must_use]
    pub fn send(&self, command: NetCommand) -> bool {
        self.commands.send(command).is_ok()
    }

    /// 非阻塞取一个事件。
    #[must_use]
    pub fn try_recv(&self) -> Option<NetEvent> {
        self.events.try_recv().ok()
    }

    /// 等到进入世界并且收到自己的 `UserInformation`。
    ///
    /// # Errors
    /// 超时、登录被拒、服务器断开时返回错误。
    pub fn wait_until_in_game(&self, timeout: Duration) -> Result<WorldSnapshot> {
        self.wait_for_snapshot(WorldSnapshot::is_ready, timeout)
    }

    /// 等到满足条件的快照。
    ///
    /// # Errors
    /// 超时、收到 [`NetEvent::Error`] 或连接关闭时返回错误。
    pub fn wait_for_snapshot(
        &self,
        predicate: impl Fn(&WorldSnapshot) -> bool,
        timeout: Duration,
    ) -> Result<WorldSnapshot> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("等待服务器响应超时（{} ms）", timeout.as_millis());
            }
            match self.events.recv_timeout(remaining) {
                Ok(NetEvent::Snapshot(snapshot)) => {
                    let snapshot = *snapshot;
                    if predicate(&snapshot) {
                        return Ok(snapshot);
                    }
                }
                Ok(NetEvent::Error(message)) => bail!("{message}"),
                Ok(NetEvent::Closed(reason)) => bail!("连接已关闭：{reason}"),
                Err(RecvTimeoutError::Timeout) => {
                    bail!("等待服务器响应超时（{} ms）", timeout.as_millis());
                }
                Err(RecvTimeoutError::Disconnected) => bail!("网络线程已退出"),
            }
        }
    }

    /// 主动关闭连接并等后台线程退出（可重复调用）。
    pub fn shutdown(&mut self) {
        let _ = self.commands.send(NetCommand::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for NetClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 后台会话主循环。
fn session(
    options: &ConnectOptions,
    commands: &Receiver<NetCommand>,
    events: &Sender<NetEvent>,
) -> Result<()> {
    let mut stream = connect(&options.server)?;
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(READ_TIMEOUT))?;

    let mut snapshot = WorldSnapshot {
        status: NetStatus::Verifying,
        ..WorldSnapshot::default()
    };
    send_snapshot(events, &snapshot);

    let mut stage = Stage::AwaitConnected;
    let mut frames = FrameBuffer::new();
    let mut buffer = [0u8; 8192];
    let mut last_keepalive = Instant::now();
    let mut dirty = false;

    loop {
        while let Ok(command) = commands.try_recv() {
            if matches!(command, NetCommand::Quit) {
                let _ = send(&mut stream, &ClientPacket::Disconnect(c::Disconnect {}));
                snapshot.status = NetStatus::Closed;
                send_snapshot(events, &snapshot);
                return Ok(());
            }
            handle_command(&mut stream, &snapshot, command)?;
        }

        match stream.read(&mut buffer) {
            Ok(0) => {
                snapshot.status = NetStatus::Closed;
                send_snapshot(events, &snapshot);
                return Ok(());
            }
            Ok(read) => {
                frames.push(&buffer[..read]);
                while let Some((index, payload)) = frames.next_frame()? {
                    match ServerPacket::decode(index, &payload) {
                        Ok(packet) => {
                            apply_packet(&mut snapshot, &packet);
                            advance(&mut stream, options, &mut stage, &mut snapshot, &packet)?;
                            dirty = true;
                        }
                        Err(error) => {
                            tracing::warn!(index, %error, "无法解析服务端包，已跳过");
                            snapshot.decode_errors = snapshot.decode_errors.saturating_add(1);
                            dirty = true;
                        }
                    }
                }
                if dirty {
                    send_snapshot(events, &snapshot);
                    dirty = false;
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(error) => return Err(error).context("读取服务器数据失败"),
        }

        if stage == Stage::InGame && last_keepalive.elapsed() >= KEEPALIVE_INTERVAL {
            let time = now_millis();
            send(&mut stream, &ClientPacket::KeepAlive(c::KeepAlive { time }))?;
            last_keepalive = Instant::now();
        }
    }
}

/// 连接服务器：逐个地址尝试，带 5 秒超时。
fn connect(address: &str) -> Result<TcpStream> {
    let addresses = address
        .to_socket_addrs()
        .with_context(|| format!("无法解析服务器地址 {address}"))?;
    let mut last_error = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error),
        }
    }
    match last_error {
        Some(error) => Err(error).with_context(|| format!("无法连接服务器 {address}")),
        None => Err(anyhow!("服务器地址 {address} 没有可用的套接字地址")),
    }
}

/// 登录状态机。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// 等 `S.Connected`。
    AwaitConnected,
    /// 等 `S.ClientVersion`。
    AwaitVersion,
    /// 等 `S.Login` / `S.LoginSuccess`。
    AwaitLogin,
    /// 等 `S.NewAccount`。
    AwaitNewAccount,
    /// 等 `S.NewCharacterSuccess` / `S.NewCharacter`。
    AwaitNewCharacter,
    /// 等 `S.StartGame`。
    AwaitStartGame,
    /// 已进入世界。
    InGame,
}

/// 根据收到的服务器包推进登录状态机。
fn advance(
    stream: &mut TcpStream,
    options: &ConnectOptions,
    stage: &mut Stage,
    snapshot: &mut WorldSnapshot,
    packet: &ServerPacket,
) -> Result<()> {
    match (*stage, packet) {
        (Stage::AwaitConnected, ServerPacket::Connected(_)) => {
            snapshot.status = NetStatus::Verifying;
            send(
                stream,
                &ClientPacket::ClientVersion(c::ClientVersion {
                    version_hash: VERSION_HASH.to_vec(),
                }),
            )?;
            *stage = Stage::AwaitVersion;
        }
        (Stage::AwaitVersion, ServerPacket::ClientVersion(version)) => {
            if version.result != 1 {
                bail!("服务器拒绝客户端版本（ClientVersion.result = 0）");
            }
            snapshot.status = NetStatus::LoggingIn;
            send_login(stream, options)?;
            *stage = Stage::AwaitLogin;
        }
        (Stage::AwaitLogin, ServerPacket::Login(result)) => match result.result {
            3 if options.auto_register => {
                send_new_account(stream, options)?;
                *stage = Stage::AwaitNewAccount;
            }
            3 => bail!(
                "账号 {} 不存在（已用 --no-register 关闭自动注册）",
                options.account
            ),
            4 => bail!("账号或口令错误"),
            5 => bail!("账号需要修改口令后才能登录"),
            1 => bail!("账号格式非法（3~15 位字母数字）"),
            2 => bail!("口令格式非法（5~15 位字母数字）"),
            0 => bail!("服务器禁止登录"),
            other => bail!("登录被拒绝（result = {other}）"),
        },
        (Stage::AwaitLogin, ServerPacket::LoginBanned(banned)) => {
            bail!("账号被封禁：{}", banned.reason);
        }
        (Stage::AwaitLogin, ServerPacket::LoginSuccess(login)) => {
            snapshot.status = NetStatus::SelectingCharacter;
            if let Some(character) = find_character(&login.characters, &options.character) {
                send_start_game(stream, character.index)?;
                *stage = Stage::AwaitStartGame;
            } else if let Some(create) = &options.create {
                send_new_character(stream, create)?;
                *stage = Stage::AwaitNewCharacter;
            } else {
                bail!(
                    "账号下没有角色 {}，请用 --create-character {} --class N --gender M 自动创建",
                    options.character,
                    options.character
                );
            }
        }
        (Stage::AwaitNewAccount, ServerPacket::NewAccount(result)) => match result.result {
            8 | 7 => {
                snapshot.status = NetStatus::LoggingIn;
                send_login(stream, options)?;
                *stage = Stage::AwaitLogin;
            }
            other => bail!("注册账号失败（result = {other}）"),
        },
        (Stage::AwaitNewCharacter, ServerPacket::NewCharacterSuccess(success)) => {
            send_start_game(stream, success.char_info.index)?;
            *stage = Stage::AwaitStartGame;
        }
        (Stage::AwaitNewCharacter, ServerPacket::NewCharacter(result)) => {
            let reason = match result.result {
                0 => "服务器禁止创建角色",
                1 => "角色名非法（3~15 位中文/字母/数字/下划线）",
                2 => "角色名已存在",
                4 => "角色数量已满",
                other => return Err(anyhow!("创建角色失败（result = {other}）")),
            };
            bail!("{reason}");
        }
        (Stage::AwaitStartGame, ServerPacket::StartGame(result)) => {
            if result.result == 4 {
                snapshot.status = NetStatus::InGame;
                *stage = Stage::InGame;
            } else {
                let reason = match result.result {
                    0 => "服务器禁止进入游戏",
                    1 => "会话没有账号",
                    2 => "角色不存在",
                    other => return Err(anyhow!("进入游戏失败（result = {other}）")),
                };
                bail!("{reason}");
            }
        }
        (Stage::AwaitStartGame, ServerPacket::StartGameBanned(banned)) => {
            bail!("无法进入游戏：{}", banned.reason);
        }
        (_, ServerPacket::Disconnect(disconnect)) => {
            bail!("服务器断开了连接（reason = {}）", disconnect.reason);
        }
        _ => {}
    }
    Ok(())
}

/// 查找角色：先精确匹配，再不区分大小写匹配。
fn find_character<'a>(
    characters: &'a [mir_protocol::data::SelectInfo],
    name: &str,
) -> Option<&'a mir_protocol::data::SelectInfo> {
    characters
        .iter()
        .find(|character| character.name == name)
        .or_else(|| {
            characters
                .iter()
                .find(|character| character.name.eq_ignore_ascii_case(name))
        })
}

fn send_login(stream: &mut TcpStream, options: &ConnectOptions) -> Result<()> {
    send(
        stream,
        &ClientPacket::Login(c::Login {
            account_id: options.account.clone(),
            password: options.password.clone(),
        }),
    )
}

fn send_new_account(stream: &mut TcpStream, options: &ConnectOptions) -> Result<()> {
    tracing::info!(account = %options.account, "账号不存在，自动注册");
    send(
        stream,
        &ClientPacket::NewAccount(c::NewAccount {
            account_id: options.account.clone(),
            password: options.password.clone(),
            birth_date: DateTimeBinary(0),
            user_name: String::new(),
            secret_question: String::new(),
            secret_answer: String::new(),
            e_mail_address: String::new(),
        }),
    )
}

fn send_new_character(stream: &mut TcpStream, create: &CreateCharacter) -> Result<()> {
    tracing::info!(name = %create.name, class = ?create.class, "创建角色");
    send(
        stream,
        &ClientPacket::NewCharacter(c::NewCharacter {
            name: create.name.clone(),
            gender: create.gender,
            class: create.class,
        }),
    )
}

fn send_start_game(stream: &mut TcpStream, character_index: i32) -> Result<()> {
    tracing::info!(character_index, "进入世界");
    send(
        stream,
        &ClientPacket::StartGame(c::StartGame { character_index }),
    )
}

/// 把 UI 命令转成客户端包；不在游戏里时静默丢弃。
fn handle_command(
    stream: &mut TcpStream,
    snapshot: &WorldSnapshot,
    command: NetCommand,
) -> Result<()> {
    if snapshot.status != NetStatus::InGame {
        return Ok(());
    }
    let packet = match command {
        NetCommand::Turn(direction) => ClientPacket::Turn(c::Turn { direction }),
        NetCommand::Walk(direction) => ClientPacket::Walk(c::Walk { direction }),
        NetCommand::Run(direction) => ClientPacket::Run(c::Run { direction }),
        NetCommand::Attack(direction) => ClientPacket::Attack(c::Attack {
            direction,
            spell: mir_core::enums::Spell::None,
        }),
        NetCommand::PickUp => ClientPacket::PickUp(c::PickUp {}),
        NetCommand::Chat(message) => {
            if message.trim().is_empty() {
                return Ok(());
            }
            // Crystal 的 `Globals.MaxChatLength = 80`（`MirConnection.cs:1183-1189`）；
            // 客户端先截断，避免一个超长粘贴包把 u16 帧长撑爆。
            let message = message.chars().take(80).collect::<String>();
            ClientPacket::Chat(c::Chat {
                message,
                linked_items: Vec::new(),
            })
        }
        NetCommand::Quit => return Ok(()),
    };
    send(stream, &packet)
}

/// 发一个客户端包（含帧头）。
fn send(stream: &mut TcpStream, packet: &ClientPacket) -> Result<()> {
    let frame = packet.encode_frame();
    stream.write_all(&frame).context("写入服务器失败")?;
    stream.flush().context("刷新服务器连接失败")?;
    Ok(())
}

/// 把快照发给 UI；UI 已经不在了就忽略（线程会在下一轮发现命令通道断开）。
fn send_snapshot(events: &Sender<NetEvent>, snapshot: &WorldSnapshot) {
    let _ = events.send(NetEvent::Snapshot(Box::new(snapshot.clone())));
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}

/// 包种类名；只列会被快照处理的包，其余归到 `"Other"`。
fn packet_name(packet: &ServerPacket) -> &'static str {
    match packet {
        ServerPacket::StartGame(_) => "StartGame",
        ServerPacket::MapInformation(_) => "MapInformation",
        ServerPacket::NewMapInfo(_) => "NewMapInfo",
        ServerPacket::MapChanged(_) => "MapChanged",
        ServerPacket::UserInformation(_) => "UserInformation",
        ServerPacket::UserLocation(_) => "UserLocation",
        ServerPacket::ObjectPlayer(_) => "ObjectPlayer",
        ServerPacket::ObjectMonster(_) => "ObjectMonster",
        ServerPacket::ObjectNPC(_) => "ObjectNPC",
        ServerPacket::ObjectItem(_) => "ObjectItem",
        ServerPacket::ObjectGold(_) => "ObjectGold",
        ServerPacket::ObjectRemove(_) => "ObjectRemove",
        ServerPacket::ObjectTurn(_) => "ObjectTurn",
        ServerPacket::ObjectWalk(_) => "ObjectWalk",
        ServerPacket::ObjectRun(_) => "ObjectRun",
        ServerPacket::ObjectAttack(_) => "ObjectAttack",
        ServerPacket::ObjectStruck(_) => "ObjectStruck",
        ServerPacket::ObjectDied(_) => "ObjectDied",
        ServerPacket::ObjectRevived(_) => "ObjectRevived",
        ServerPacket::Death(_) => "Death",
        ServerPacket::Revived(_) => "Revived",
        ServerPacket::HealthChanged(_) => "HealthChanged",
        ServerPacket::ObjectHealth(_) => "ObjectHealth",
        ServerPacket::LevelChanged(_) => "LevelChanged",
        ServerPacket::GainedItem(_) => "GainedItem",
        ServerPacket::GainedGold(_) => "GainedGold",
        ServerPacket::LoseGold(_) => "LoseGold",
        ServerPacket::Chat(_) => "Chat",
        ServerPacket::ObjectChat(_) => "ObjectChat",
        ServerPacket::NPCResponse(_) => "NPCResponse",
        ServerPacket::NPCGoods(_) => "NPCGoods",
        ServerPacket::ObjectName(_) => "ObjectName",
        ServerPacket::PlayerUpdate(_) => "PlayerUpdate",
        ServerPacket::Disconnect(_) => "Disconnect",
        _ => "Other",
    }
}

/// 用一条服务端包更新快照。
fn apply_packet(snapshot: &mut WorldSnapshot, packet: &ServerPacket) {
    *snapshot
        .packet_counts
        .entry(packet_name(packet))
        .or_default() += 1;

    match packet {
        ServerPacket::StartGame(result) => {
            if result.result == 4 {
                snapshot.status = NetStatus::InGame;
            }
        }
        ServerPacket::MapInformation(info) => {
            snapshot.map = Some(MapSnapshot {
                map_index: info.map_index,
                file_name: info.file_name.clone(),
                title: info.title.clone(),
                width: 0,
                height: 0,
            });
        }
        ServerPacket::NewMapInfo(info) => {
            let map = snapshot.map.get_or_insert_with(MapSnapshot::default);
            map.map_index = info.map_index;
            map.title.clone_from(&info.info.title);
            map.width = info.info.width;
            map.height = info.info.height;
            for npc in &info.info.npcs {
                if npc.object_id == 0 {
                    continue;
                }
                snapshot.entities.insert(
                    npc.object_id,
                    WorldEntity {
                        object_id: npc.object_id,
                        kind: EntityKind::Npc,
                        name: npc.name.clone(),
                        position: npc.location,
                        direction: MirDirection::Down,
                        image: npc.image,
                        dead: false,
                        hp_percent: None,
                    },
                );
            }
        }
        ServerPacket::MapChanged(changed) => {
            snapshot.map = Some(MapSnapshot {
                map_index: changed.map_index,
                file_name: changed.file_name.clone(),
                title: changed.title.clone(),
                width: 0,
                height: 0,
            });
            snapshot.entities.clear();
            if let Some(me) = &mut snapshot.self_player {
                me.position = changed.location;
                me.direction = changed.direction;
            }
        }
        ServerPacket::UserInformation(info) => {
            snapshot.self_player = Some(SelfPlayer {
                object_id: info.object_id,
                name: info.name.clone(),
                position: info.location,
                direction: info.direction,
                hp: info.hp,
                mp: info.mp,
                level: info.level,
                class: info.class,
                gender: info.gender,
                hair: info.hair,
                gold: info.gold,
                dead: false,
            });
        }
        ServerPacket::UserLocation(location) => {
            if let Some(me) = &mut snapshot.self_player {
                me.position = location.location;
                me.direction = location.direction;
            }
        }
        ServerPacket::ObjectPlayer(player) => {
            let image = u16::try_from(player.armour.max(0)).unwrap_or(0);
            upsert_entity(
                snapshot,
                WorldEntity {
                    object_id: player.object_id,
                    kind: EntityKind::Player,
                    name: player.name.clone(),
                    position: player.location,
                    direction: player.direction,
                    image,
                    dead: player.dead,
                    hp_percent: None,
                },
            );
        }
        ServerPacket::ObjectMonster(monster) => {
            upsert_entity(
                snapshot,
                WorldEntity {
                    object_id: monster.object_id,
                    kind: EntityKind::Monster,
                    name: monster.name.clone(),
                    position: monster.location,
                    direction: monster.direction,
                    image: u16::from(monster.image),
                    dead: monster.dead,
                    hp_percent: None,
                },
            );
        }
        ServerPacket::ObjectNPC(npc) => {
            upsert_entity(
                snapshot,
                WorldEntity {
                    object_id: npc.object_id,
                    kind: EntityKind::Npc,
                    name: npc.name.clone(),
                    position: npc.location,
                    direction: npc.direction,
                    image: npc.image,
                    dead: false,
                    hp_percent: None,
                },
            );
        }
        ServerPacket::ObjectItem(item) => {
            upsert_entity(
                snapshot,
                WorldEntity {
                    object_id: item.object_id,
                    kind: EntityKind::Item,
                    name: item.name.clone(),
                    position: item.location,
                    direction: MirDirection::Down,
                    image: item.image,
                    dead: false,
                    hp_percent: None,
                },
            );
        }
        ServerPacket::ObjectGold(gold) => {
            // Crystal 用 112..116 五帧画不同数量的金币堆（`ItemObject.cs:55-64`）。
            let image = match gold.gold {
                0..=99 => 112,
                100..=199 => 113,
                200..=499 => 114,
                500..=999 => 115,
                _ => 116,
            };
            upsert_entity(
                snapshot,
                WorldEntity {
                    object_id: gold.object_id,
                    kind: EntityKind::Item,
                    name: format!("金币 {}", gold.gold),
                    position: gold.location,
                    direction: MirDirection::Down,
                    image,
                    dead: false,
                    hp_percent: None,
                },
            );
        }
        ServerPacket::ObjectRemove(remove) => {
            snapshot.entities.remove(&remove.object_id);
        }
        ServerPacket::ObjectTurn(turn) => {
            update_entity(snapshot, turn.object_id, |entity| {
                entity.position = turn.location;
                entity.direction = turn.direction;
            });
        }
        ServerPacket::ObjectWalk(walk) => {
            update_entity(snapshot, walk.object_id, |entity| {
                entity.position = walk.location;
                entity.direction = walk.direction;
            });
        }
        ServerPacket::ObjectRun(run) => {
            update_entity(snapshot, run.object_id, |entity| {
                entity.position = run.location;
                entity.direction = run.direction;
            });
        }
        ServerPacket::ObjectAttack(attack) => {
            update_entity(snapshot, attack.object_id, |entity| {
                entity.position = attack.location;
                entity.direction = attack.direction;
            });
        }
        ServerPacket::ObjectStruck(struck) => {
            update_entity(snapshot, struck.object_id, |entity| {
                entity.direction = struck.direction;
            });
        }
        ServerPacket::ObjectDied(died) => {
            update_entity(snapshot, died.object_id, |entity| {
                entity.dead = true;
                entity.position = died.location;
                entity.direction = died.direction;
                entity.hp_percent = Some(0);
            });
        }
        ServerPacket::ObjectRevived(revived) => {
            update_entity(snapshot, revived.object_id, |entity| {
                entity.dead = false;
            });
        }
        ServerPacket::ObjectHealth(health) => {
            update_entity(snapshot, health.object_id, |entity| {
                entity.hp_percent = Some(health.percent.min(100));
            });
        }
        ServerPacket::Death(death) => {
            if let Some(me) = &mut snapshot.self_player {
                me.dead = true;
                me.position = death.location;
                me.direction = death.direction;
            }
        }
        ServerPacket::Revived(_) => {
            if let Some(me) = &mut snapshot.self_player {
                me.dead = false;
            }
        }
        ServerPacket::HealthChanged(health) => {
            if let Some(me) = &mut snapshot.self_player {
                me.hp = health.hp;
                me.mp = health.mp;
            }
        }
        ServerPacket::LevelChanged(level) => {
            if let Some(me) = &mut snapshot.self_player {
                me.level = level.level;
            }
        }
        ServerPacket::GainedItem(gained) => {
            push_chat(
                snapshot,
                ChatLine {
                    speaker: None,
                    text: format!(
                        "获得物品 #{} ×{}",
                        gained.item.item_index, gained.item.count
                    ),
                    kind: ChatType::System,
                },
            );
        }
        ServerPacket::GainedGold(gained) => {
            if let Some(me) = &mut snapshot.self_player {
                me.gold = me.gold.saturating_add(gained.gold);
            }
            push_chat(
                snapshot,
                ChatLine {
                    speaker: None,
                    text: format!("获得金币 {}", gained.gold),
                    kind: ChatType::System,
                },
            );
        }
        ServerPacket::LoseGold(lost) => {
            if let Some(me) = &mut snapshot.self_player {
                me.gold = me.gold.saturating_sub(lost.gold);
            }
        }
        ServerPacket::Chat(chat) => {
            tracing::info!(message = %chat.message, "系统/频道聊天");
            push_chat(
                snapshot,
                ChatLine {
                    speaker: None,
                    text: chat.message.clone(),
                    kind: chat.type_,
                },
            );
        }
        ServerPacket::ObjectChat(chat) => {
            let speaker = snapshot
                .entities
                .get(&chat.object_id)
                .map(|entity| entity.name.clone());
            tracing::info!(
                speaker = speaker.as_deref().unwrap_or("?"),
                text = %chat.text,
                "对象聊天"
            );
            push_chat(
                snapshot,
                ChatLine {
                    speaker,
                    text: chat.text.clone(),
                    kind: chat.type_,
                },
            );
        }
        ServerPacket::NPCResponse(response) => {
            tracing::info!(lines = response.page.len(), "NPC 对话");
            snapshot.npc_dialog = Some(response.page.clone());
        }
        ServerPacket::NPCGoods(goods) => {
            tracing::info!(
                items = goods.list.len(),
                rate = goods.rate,
                "NPC 商店商品列表"
            );
            snapshot.npc_goods = Some(goods.list.len());
        }
        ServerPacket::ObjectName(name) => {
            update_entity(snapshot, name.object_id, |entity| {
                entity.name.clone_from(&name.name);
            });
        }
        ServerPacket::PlayerUpdate(update) => {
            update_entity(snapshot, update.object_id, |entity| {
                entity.image = u16::try_from(update.armour.max(0)).unwrap_or(0);
            });
        }
        ServerPacket::Disconnect(disconnect) => {
            snapshot.status = NetStatus::Failed;
            snapshot.last_error = Some(format!("服务器断开连接（reason = {}）", disconnect.reason));
        }
        _ => {}
    }
}

/// 插入或更新一个实体。
fn upsert_entity(snapshot: &mut WorldSnapshot, entity: WorldEntity) {
    snapshot.entities.insert(entity.object_id, entity);
}

/// 修改一个实体；不存在时什么也不做。
fn update_entity(
    snapshot: &mut WorldSnapshot,
    object_id: u32,
    update: impl FnOnce(&mut WorldEntity),
) {
    if let Some(entity) = snapshot.entities.get_mut(&object_id) {
        update(entity);
    } else if let Some(me) = &mut snapshot.self_player {
        // 自己的 ObjectWalk/ObjectRun 在协议里只通过 UserLocation 回显；这里保留兜底，
        // 万一带了自己的 ID 也不会丢坐标。
        if me.object_id == object_id {
            // 用一个临时实体承接更新，再写回自己（字段集合不同，简单映射坐标/朝向）。
            let mut temporary = WorldEntity {
                object_id,
                kind: EntityKind::Player,
                name: me.name.clone(),
                position: me.position,
                direction: me.direction,
                image: 0,
                dead: me.dead,
                hp_percent: None,
            };
            update(&mut temporary);
            me.position = temporary.position;
            me.direction = temporary.direction;
            me.dead = temporary.dead;
        }
    }
}

/// 追加聊天记录并裁剪到上限。
fn push_chat(snapshot: &mut WorldSnapshot, line: ChatLine) {
    snapshot.chat.push(line);
    if snapshot.chat.len() > CHAT_LOG_LIMIT {
        snapshot.chat.remove(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir_core::enums::Monster as MonsterImage;
    use mir_protocol::data::SelectInfo;
    use mir_protocol::packets::server::{
        ObjectItem, ObjectMonster, ObjectWalk, UserInformation, UserLocation,
    };

    fn user_information() -> UserInformation {
        UserInformation {
            object_id: 7,
            real_id: 1,
            name: "测试者".to_owned(),
            location: Point::new(330, 270),
            direction: MirDirection::Down,
            class: MirClass::Warrior,
            gender: MirGender::Male,
            level: 1,
            hair: 1,
            hp: 15,
            mp: 15,
            gold: 0,
            ..UserInformation::default()
        }
    }

    #[test]
    fn snapshot_handles_login_and_entities() {
        let mut snapshot = WorldSnapshot::default();
        apply_packet(
            &mut snapshot,
            &ServerPacket::UserInformation(user_information()),
        );
        assert_eq!(snapshot.packet_count("UserInformation"), 1);
        let me = snapshot.self_player.as_ref().expect("自己");
        assert_eq!(me.object_id, 7);
        assert_eq!(me.position, Point::new(330, 270));

        apply_packet(
            &mut snapshot,
            &ServerPacket::ObjectMonster(ObjectMonster {
                object_id: 8,
                name: "骷髅".to_owned(),
                image: MonsterImage::Skeleton,
                location: Point::new(331, 271),
                direction: MirDirection::Left,
                ..ObjectMonster::default()
            }),
        );
        apply_packet(
            &mut snapshot,
            &ServerPacket::ObjectItem(ObjectItem {
                object_id: 9,
                name: "金币".to_owned(),
                location: Point::new(330, 271),
                image: 112,
                ..ObjectItem::default()
            }),
        );
        assert_eq!(snapshot.entities.len(), 2);
        assert_eq!(snapshot.entity_views().len(), 3);

        apply_packet(
            &mut snapshot,
            &ServerPacket::ObjectWalk(ObjectWalk {
                object_id: 8,
                location: Point::new(332, 271),
                direction: MirDirection::Right,
            }),
        );
        assert_eq!(
            snapshot.entities[&8].position,
            Point::new(332, 271),
            "ObjectWalk 应更新怪物坐标"
        );

        apply_packet(
            &mut snapshot,
            &ServerPacket::UserLocation(UserLocation {
                location: Point::new(330, 271),
                direction: MirDirection::Down,
            }),
        );
        assert_eq!(
            snapshot.self_player.as_ref().unwrap().position,
            Point::new(330, 271)
        );

        apply_packet(
            &mut snapshot,
            &ServerPacket::ObjectRemove(mir_protocol::packets::server::ObjectRemove {
                object_id: 8,
            }),
        );
        assert!(!snapshot.entities.contains_key(&8));
    }

    #[test]
    fn chat_log_is_capped_and_formats_speaker() {
        let mut snapshot = WorldSnapshot::default();
        for index in 0..(CHAT_LOG_LIMIT + 5) {
            push_chat(
                &mut snapshot,
                ChatLine {
                    speaker: None,
                    text: format!("第 {index} 条"),
                    kind: ChatType::System,
                },
            );
        }
        assert_eq!(snapshot.chat.len(), CHAT_LOG_LIMIT);
        assert_eq!(snapshot.chat.last().unwrap().text, "第 104 条");
        let line = ChatLine {
            speaker: Some("甲".to_owned()),
            text: "你好".to_owned(),
            kind: ChatType::Normal,
        };
        assert_eq!(line.display(), "[甲] 你好");
    }

    #[test]
    fn find_character_matches_exact_then_case_insensitive() {
        let characters = vec![
            SelectInfo {
                index: 3,
                name: "Hero".to_owned(),
                ..SelectInfo::default()
            },
            SelectInfo {
                index: 4,
                name: "mage".to_owned(),
                ..SelectInfo::default()
            },
        ];
        assert_eq!(
            find_character(&characters, "Hero").map(|c| c.index),
            Some(3)
        );
        assert_eq!(
            find_character(&characters, "MAGE").map(|c| c.index),
            Some(4)
        );
        assert!(find_character(&characters, "missing").is_none());
    }
}
