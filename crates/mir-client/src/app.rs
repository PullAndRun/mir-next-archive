//! winit 窗口 + softbuffer 软件呈现。
//!
//! 渲染本身全部在 `mir-render` 里完成（纯 headless），界面在 `mir_client::ui` 里；
//! 这里只负责：
//!
//! * 创建 800×600 的窗口，失败时打印可读的原因并以成功状态退出（无显示器的 CI）；
//! * 把 [`Frame`] 的 RGBA8 转成 softbuffer 需要的 `0x00RRGGBB`；
//! * 离线模式（`--map`）：方向键 / WASD 平移相机、`S` 截图、`F1` 调试叠层、`Q` 退出；
//! * 联机模式（`--connect`）：方向键/WASD 移动、`Space` 攻击（锁定目标时朝目标攻击/
//!   靠近）、`P` 捡取、`T` 聊天、`I` 背包、`E` 装备、`F3` 技能、`M` 小地图、`B` 大地图、
//!   `Q` 任务、`N` 邮件、`G` 行会（仓库面板由服务端 `S.NPCStorage` 自动打开）、
//!   `1..9` 施法、鼠标悬停/点击锁定世界目标、左键双击目标攻击、鼠标点 NPC/面板按钮、
//!   点空地走 A\* 路径、点地面物品拾取、`Esc` 关面板、`F1` 叠层、`F2` 截图、`F10` 退出；
//! * 每帧按 `S.MapInformation.Music` / `S.MapChanged.Music` 切换背景音乐；
//! * 每 100 ms 递增一次动画帧（对应 Crystal 的 `MapControl.AnimationCount++`，
//!   `GameScene.cs:1188-1193`）。
//!
//! 网络线程由 [`NetClient`] 管理；窗口创建失败时 `ClientApp` 被丢弃，
//! `NetClient` 的 `Drop` 会发送 `Quit` 并回收线程，不会留下半开连接。

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mir_client::audio::{SoundLibrary, SoundPlayer};
use mir_client::camera::{self, SelfMotion};
use mir_client::geom::Rect;
use mir_client::net::{
    ConnectOptions, MapReloadRequest, NetClient, NetCommand, NetEvent, NetStatus, WorldSnapshot,
};
use mir_client::ui::reconnect::ReconnectState;
use mir_client::ui::select::{self, CharacterAction, LoginField, ServerAction};
use mir_client::ui::{self, UiAction, UiState, UiText};
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{ItemType, MirDirection, Spell};
use mir_render::sound::SoundId;
use mir_render::{ActorAction, Frame, MapScene};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::overlay;
use mir_client::settings::ClientSettings;

/// 默认窗口宽（与 `MapScene` 默认视口一致）。
pub const WINDOW_WIDTH: u32 = 800;
/// 默认窗口高。
pub const WINDOW_HEIGHT: u32 = 600;

/// 动画节拍：与 Crystal 的 `MoveTime = CMain.Time + 100` 一致（`GameScene.cs:1190`）。
const ANIMATION_INTERVAL: Duration = Duration::from_millis(100);
/// 按住左键连续走路的重复间隔（至少隔一帧，避免一次点击发两包）。
const WALK_REPEAT: Duration = Duration::from_millis(150);

/// 客户端运行模式。
enum Mode {
    /// 离线看地图（`--map`）。
    Offline,
    /// 联机世界（`--connect`）。
    Live(Box<LiveState>),
    /// 登录窗口（`--login`）：服务器选择 → 角色选择 → 进入世界。
    Login(Box<LoginState>),
}

/// 联机状态：网络句柄 + 最近一次快照 + 本地 UI 状态。
struct LiveState {
    /// 网络线程句柄。
    net: NetClient,
    /// 最近一次世界快照。
    snapshot: WorldSnapshot,
    /// 界面状态（面板开关、选中项、鼠标）。
    ui: UiState,
    /// 音效播放器（没有音频设备时是空操作，见 [`crate::audio`]）。
    sound: SoundPlayer,
    /// 已消费到哪一条音效事件（快照整体克隆，用递增 ID 去重）。
    last_sound_id: u64,
    /// 已消费到哪一条撞墙/回拉事件（本地提示去重；不发包）。
    last_wall_bump_id: u64,
    /// 聊天输入框；`Some` 表示正在输入。
    chat_input: Option<String>,
    /// 是否按住 Shift（移动键 + Shift = 跑）。
    shift: bool,
    /// 上一次打印的聊天行，避免重复刷屏。
    last_chat: Option<String>,
    /// 上一次打印的网络状态。
    last_status: NetStatus,
    /// 是否已经打印过「进入世界」。
    entered: bool,
    /// 是否按住鼠标左键（点地连续走）。
    left_held: bool,
    /// 按住左键时的目标格。
    walk_target: Option<Point>,
    /// 下一次重复走路的时间。
    next_walk: Instant,
    /// 上一次鼠标所在的格子（悬停高亮变化时才重绘）。
    last_hover_cell: Option<Point>,
    /// 断线自动重连状态机。
    reconnect: ReconnectState,
    /// 建立连接用的参数（重连时复用账号/角色）。
    connect: ConnectOptions,
    /// 自己最近一步的移动（Crystal `OffSetMove` 插值 + 相机跟随，见
    /// [`mir_client::camera`]）。
    motion: Option<SelfMotion>,
}

impl LiveState {
    /// 用刚应用的权威快照更新 motion（X1-01：动作与插值共用接收/应用时刻）。
    ///
    /// 有效走/跑落点由网络层在应用 `S.UserLocation` 时把自机动作重置到同一时刻
    /// （`net::apply_self_location`）；这里直接用 `SelfPlayer::action_started_ms`
    /// 作为 motion 起点，不再用 UI 收到快照的 `now`，两者因此同钟。
    /// 同坐标快照（`old == new`）不重启计时；传送/跳变等清空 motion。
    fn update_self_motion(&mut self, old: Point, new: Point) {
        match camera::motion_after_authoritative_step(&self.snapshot, old, new) {
            camera::StepMotion::Unchanged => {}
            camera::StepMotion::Invalid => self.motion = None,
            camera::StepMotion::Started(motion) => self.motion = Some(motion),
        }
    }

    /// 发命令；网络线程已退出时打印一次可读原因。
    ///
    /// 走/跑/攻击/转身在发包前先更新本地快照的动作，画面不用等服务器回包
    /// （Crystal 客户端也是先播放动作再发包，`PlayerObject.cs:1313-1334`）。
    /// 音效也在这里**本地预播**：服务端不会把 `ObjectWalk`/`ObjectRun`/`ObjectAttack`
    /// 回发给动作发起者（`mir-server/src/world.rs:2428-2449`），不本地播的话自己走路没声音。
    fn send(&mut self, command: NetCommand) {
        if let Some((action, direction)) = command.local_animation() {
            self.snapshot
                .apply_local_action(action, direction, now_millis_u64());
            // 本地动作 → 音效（Crystal 同样在动画帧上播，`PlayerObject.cs:3596-3599`、
            // `:4779-4847`）。
            let sound = match action {
                ActorAction::Walking => Some(SoundId::Walk),
                ActorAction::Running => Some(SoundId::Run),
                ActorAction::Attacking => Some(SoundId::Swing),
                _ => None,
            };
            if let Some(sound) = sound {
                self.snapshot.push_local_sound(sound, Spell::None);
            }
        }
        // 立刻把待播事件送进播放器，不等下一份快照（网络卡顿时也能听到自己的脚步）。
        self.play_pending_sounds(now_millis_u64());
        if !self.net.send(command) {
            eprintln!("网络线程已退出，命令被丢弃");
        }
    }

    /// 把快照里新增的音效事件交给播放器（自己与别人走同一条队列）。
    ///
    /// `now_ms` 是 Unix 毫秒，喂给 100 ms 频控（[`mir_render::SOUND_COOLDOWN_MS`]）。
    fn play_pending_sounds(&mut self, now_ms: u64) {
        let (events, last) = self.snapshot.sound_events_since(self.last_sound_id);
        self.last_sound_id = last;
        for event in events {
            let played = match event.sound {
                // 施法音随技能变（`SoundManager.cs:87-89` 的 `M<技能>-0.wav`）。
                SoundId::Cast if event.spell != Spell::None => {
                    self.sound.play_spell(event.spell, now_ms)
                }
                sound => self.sound.play_event(sound, event.female, now_ms),
            };
            tracing::debug!(?event.sound, played, "音效事件");
        }
    }
}

/// 启动离线地图窗口。
///
/// 没有图形环境（`EventLoop::new` 失败、窗口/表面创建失败）时打印原因并返回，
/// 不 panic、不返回非零退出码。离线模式只有 F1 点阵叠层，`language` 暂不影响画面。
pub fn run(scene: MapScene, center: Point, _language: Language) {
    // 离线模式不会切图，地图目录只是占位（`ClientApp` 要求有这个字段）。
    let assets = mir_client::assets::AssetRoot::resolve(
        None,
        std::env::var(mir_client::assets::ASSET_ROOT_ENV)
            .ok()
            .as_deref(),
        None,
    );
    run_app(
        scene,
        center,
        Mode::Offline,
        assets.map_dir(),
        assets.data_dir(),
        ClientSettings::default(),
        ClientSettings::default_path(),
    );
}

/// 联机窗口的启动参数：目录与音效播放器。
///
/// 打包成结构体是为了让 [`run_live`] 的参数不至于太多，也方便以后加新选项。
pub struct LiveOptions {
    /// 地图目录（`resource/map`）。
    pub map_dir: PathBuf,
    /// 素材目录（`resource/data`）：用来打开小地图图库 `mmap.wzl`。
    pub data_dir: PathBuf,
    /// 音效播放器（没有设备时是空操作）。
    pub sound: SoundPlayer,
    /// 设置文件内容（设置面板按它显示初始值）。
    pub settings: ClientSettings,
    /// 设置文件路径（设置面板每次改动都写回这里）。
    pub settings_path: PathBuf,
}

/// 启动联机窗口。
///
/// `snapshot` 是进入世界后的首个快照（`wait_until_in_game` 的返回值），
/// `center` 通常是自己的坐标。切图（`S.MapChanged`）时用地形目录
/// （[`LiveOptions::map_dir`]）拼出 `<map_dir>/<file>.map` 重新加载地形；
/// [`LiveOptions::data_dir`] 用来打开小地图图库（缺文件时面板画占位）。
/// 窗口创建失败时连接会被 `Drop` 自动关闭。
pub fn run_live(
    scene: MapScene,
    center: Point,
    net: NetClient,
    snapshot: WorldSnapshot,
    language: Language,
    connect: ConnectOptions,
    options: LiveOptions,
) {
    let status = snapshot.status;
    let mut ui = UiState::new(language);
    // 登录事件：进入世界后的第一帧播放（Crystal LoginScene.cs:314 的 LoginEffect）。
    let mut sound = options.sound;
    sound.play(mir_client::audio::LOGIN_SOUND_FILE, now_millis_u64());
    let (minimap_scene, reason) = mir_render::MinimapScene::open_dir(&options.data_dir);
    if let Some(reason) = reason {
        println!("小地图素材不可用：{reason}（M 面板会显示纯色占位）");
    }
    ui.minimap_scene = minimap_scene;
    // 设置面板的初始值与设置文件一致（命令行 `--volume` 等只覆盖播放器，不写文件）。
    ui.settings = options.settings.panel();
    ui.settings.language = language;
    // 新手链数据表（缺失/损坏时用内置默认，不 panic）。
    ui.newbie_chain = mir_client::newbie::NewbieChain::load("config/newbie/chain.toml").0;
    // 统一配置：语言按 client.toml（缺失/损坏已回落默认并生成）。
    let (config, _issue) =
        mir_client::client_config::ClientConfig::load(mir_client::client_config::default_path());
    if config.language == "en" {
        ui.language = Language::En;
        ui.settings.language = Language::En;
    }
    ui.language = language;
    let mode = Mode::Live(Box::new(LiveState {
        net,
        ui,
        snapshot,
        sound,
        last_sound_id: 0,
        last_wall_bump_id: 0,
        chat_input: None,
        shift: false,
        last_chat: None,
        last_status: status,
        entered: false,
        left_held: false,
        walk_target: None,
        next_walk: Instant::now(),
        last_hover_cell: None,
        reconnect: ReconnectState::default(),
        connect,
        motion: None,
    }));
    run_app(
        scene,
        center,
        mode,
        options.map_dir,
        options.data_dir,
        options.settings,
        options.settings_path,
    );
}

/// 登录窗口的启动参数。
///
/// `--login` 打开「服务器选择 → 角色选择」流程：地址可配（默认取设置文件里的
/// `server`）、连接失败可重试；进入世界后复用联机窗口。
pub struct LoginOptions {
    /// 地图目录（进入世界后按服务器地图加载）。
    pub map_dir: PathBuf,
    /// 素材目录。
    pub data_dir: PathBuf,
    /// 音效播放器（进入世界后交给 `LiveState`）。
    pub sound: SoundPlayer,
    /// 设置文件内容。
    pub settings: ClientSettings,
    /// 设置文件路径。
    pub settings_path: PathBuf,
    /// 界面语言。
    pub language: Language,
    /// 预填账号（`--account`，可空）。
    pub account: String,
    /// 预填口令（`--password`，可空）。
    pub password: String,
}

/// 登录流程的异步结果（网络线程 → 窗口线程）。
enum LoginOutcome {
    /// 登录成功，拿到角色列表（手动选角阶段）。
    Characters(NetClient, Box<WorldSnapshot>),
    /// 已进入世界。
    InGame(NetClient, Box<WorldSnapshot>),
    /// 连接/登录失败。
    Failed(select::ConnectError, String),
}

/// 登录窗口状态（服务器选择 + 角色选择）。
struct LoginState {
    /// 服务器选择页状态。
    server: select::ServerSelectState,
    /// 角色选择页状态。
    characters: select::CharacterSelectState,
    /// 当前页。
    screen: LoginScreen,
    /// 网络会话（登录成功后保留，选角/进入世界都用它）。
    net: Option<NetClient>,
    /// 正在等待的网络结果。
    pending: Option<Receiver<LoginOutcome>>,
    /// 本次连接开始时刻（UI 侧看门狗，防止线程异常时按钮永远停在「连接中」）。
    connect_started: Option<Instant>,
    /// 鼠标位置（逻辑坐标）。
    mouse: Point,
    /// 界面语言。
    language: Language,
    /// 音效播放器（进入世界时移交给 `LiveState`）。
    sound: Option<SoundPlayer>,
    /// 已经打印过「连接中」（避免每帧刷屏）。
    announced: bool,
}

/// 登录窗口的两页。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoginScreen {
    /// 服务器地址 + 账号 + 口令。
    Server,
    /// 角色列表（含创建表单）。
    Characters,
}

impl LoginState {
    fn new(
        sound: SoundPlayer,
        settings: &ClientSettings,
        language: Language,
        account: &str,
        password: &str,
    ) -> Self {
        let mut server = select::ServerSelectState::with_server(&settings.server());
        if !account.is_empty() {
            server.account.set(account);
        }
        if !password.is_empty() {
            server.password.set(password);
        }
        let characters = select::CharacterSelectState {
            equipment_cache: settings.equipment.clone(),
            ..select::CharacterSelectState::default()
        };
        Self {
            server,
            characters,
            screen: LoginScreen::Server,
            net: None,
            pending: None,
            connect_started: None,
            mouse: Point::new(0, 0),
            language,
            sound: Some(sound),
            announced: false,
        }
    }

    fn busy(&self) -> bool {
        self.pending.is_some()
    }

    fn draw(&mut self, frame: &mut Frame, text: &UiText) {
        frame.clear([14, 18, 26, 255]);
        let screen = Rect::screen(frame.width, frame.height);
        match self.screen {
            LoginScreen::Server => {
                let layout = select::server_layout(screen);
                select::draw_server(frame, &layout, &self.server, text, self.language);
            }
            LoginScreen::Characters => {
                let layout = select::character_layout(
                    screen,
                    self.characters.characters.len(),
                    self.characters.creating,
                );
                select::draw_characters(frame, &layout, &self.characters, text, self.language);
            }
        }
    }
}

/// 启动登录窗口（`--login`）：先画服务器选择页，进入世界后复用联机窗口。
///
/// 地形用空白地图占位（`resource/data` 缺失也能开窗）；进入世界时再按服务器
/// `S.MapInformation.FileName` 加载真实地形。
pub fn run_login(options: LoginOptions) {
    let LoginOptions {
        map_dir,
        data_dir,
        sound,
        settings,
        settings_path,
        language,
        account,
        password,
    } = options;
    let scene = blank_scene(&data_dir);
    let center = Point::new(330, 270);
    let state = LoginState::new(sound, &settings, language, &account, &password);
    run_app(
        scene,
        center,
        Mode::Login(Box::new(state)),
        map_dir,
        data_dir,
        settings,
        settings_path,
    );
}

/// 空白地形（登录窗口背景）：1024×1024 全可走格子 + 真实瓦片图库（没有地形图像，
/// 但保留图库供诊断/后续使用）+ 实体图库。
fn blank_scene(data_dir: &Path) -> MapScene {
    let width = 1024;
    let height = 1024;
    let map = mir_world::Map {
        width,
        height,
        cells: vec![mir_world::MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "login".to_owned(),
        version: mir_world::MapVersion::V0,
        doors: Vec::new(),
    };
    // `open_dir` 失败（素材目录不存在/全缺）时退回空图库，登录窗口仍能开；
    // 只 warn 一次，避免每帧刷屏。
    let gfx_dir = mir_client::assets::gfx_dir_for_data_dir(data_dir);
    let assets = mir_client::assets::ResolvedAssets::discover_for_data_dir(data_dir);
    let asset_root = data_dir.parent();
    let allow_legacy = assets
        .effective()
        .is_none_or(mir_client::assets::AssetMode::allow_legacy);
    let libraries = mir_render::TileLibraries::open_with_sources(
        data_dir,
        Some(gfx_dir.as_path()),
        asset_root,
        assets.map.as_ref(),
        assets.packs.as_ref(),
        allow_legacy,
    )
    .unwrap_or_else(|error| {
        eprintln!("警告：登录背景加载素材图库失败（{error}），使用空图库");
        mir_render::TileLibraries::empty(data_dir)
    });
    println!(
        "素材：{} · 图库 {} 个（+WIL {} 个）",
        data_dir.display(),
        libraries.len(),
        libraries.wil_len()
    );
    MapScene::from_parts_with_entities(
        map,
        libraries,
        mir_render::Viewport::DEFAULT,
        mir_render::EntityLibraries::open_with_sources(
            data_dir,
            Some(gfx_dir.as_path()),
            asset_root,
            assets.map.as_ref(),
            assets.packs.as_ref(),
            allow_legacy,
        ),
    )
}

/// 按服务器地图名加载地形，失败时退回空白图（登录后也能进世界）。
fn load_world_scene(map_name: &str, map_dir: &Path, data_dir: &Path) -> MapScene {
    let path = if map_name.to_ascii_lowercase().ends_with(".map") {
        PathBuf::from(map_name)
    } else {
        map_dir.join(format!("{map_name}.map"))
    };
    let gfx_dir = mir_client::assets::gfx_dir_for_data_dir(data_dir);
    let assets = mir_client::assets::ResolvedAssets::discover_for_data_dir(data_dir);
    let asset_root = data_dir.parent();
    let allow_legacy = assets
        .effective()
        .is_none_or(mir_client::assets::AssetMode::allow_legacy);
    match MapScene::load_with_sources(
        &path,
        data_dir,
        Some(gfx_dir.as_path()),
        asset_root,
        assets.map.as_ref(),
        assets.packs.as_ref(),
        allow_legacy,
    ) {
        Ok(scene) => {
            println!(
                "素材：{} · 地图库 {} 个（+WIL {} 个）· 地图 {}（{}×{}）",
                data_dir.display(),
                scene.libraries().len(),
                scene.libraries().wil_len(),
                path.display(),
                scene.map.width,
                scene.map.height,
            );
            scene
        }
        Err(error) => {
            eprintln!("加载地图 {} 失败：{error}；使用空白地形", path.display());
            blank_scene(data_dir)
        }
    }
}

/// 聊天框提交 → 网络命令。
///
/// * `!!` 前缀是组队频道（Crystal `PlayerObject.cs:2001-2013`），仍包成 `C.Chat`，
///   由服务端按前缀转组队广播；
/// * `@` 开头是 GM/脚本命令，**原样**走 `C.Chat`，客户端不翻译、不解析、不维护命令列表；
/// * 空白输入不发送。
#[must_use]
fn chat_submit_command(message: &str) -> Option<NetCommand> {
    if message.trim().is_empty() {
        return None;
    }
    Some(
        mir_client::ui::group::group_chat_command(message)
            .unwrap_or_else(|| NetCommand::Chat(message.to_owned())),
    )
}

/// 把网络层错误文案归类成登录页的提示类别（原始详情只进控制台/日志）。
///
/// `NetClient` 的错误是 `anyhow` 文本（例如「登录失败（result = 5）」、
/// 「连接已关闭：…」），这里只做粗分类；登录页只显示本地化类别
/// （[`mir_client::ui::select::error_text`]，`Label` 中英双语），不会把中文诊断串
/// 画到界面上——英文玩家看到的也是英文。原始 `message` 仍由调用方 `eprintln!`
/// 输出，并保留在 `ServerSelectState::message` 里供诊断/测试读取。
#[must_use]
pub fn classify_login_error(message: &str) -> select::ConnectError {
    let lower = message.to_lowercase();
    if message.contains("密码")
        || message.contains("口令")
        || message.contains("账号")
        || lower.contains("password")
        || lower.contains("account")
    {
        select::ConnectError::Credentials
    } else if message.contains("超时") || lower.contains("timeout") {
        select::ConnectError::Timeout
    } else if message.contains("版本") || lower.contains("version") {
        select::ConnectError::Version
    } else if message.contains("拒绝") || lower.contains("refused") {
        select::ConnectError::Refused
    } else if message.contains("连接") || lower.contains("connect") {
        select::ConnectError::Unreachable
    } else {
        select::ConnectError::Other
    }
}

/// 当前角色的装备摘要（`武器 / 衣服 / ...`；没有装备或没有模板时为空串）。
#[must_use]
pub fn equipment_summary(snapshot: &WorldSnapshot) -> String {
    snapshot
        .equipment
        .iter()
        .flatten()
        .filter_map(|item| {
            snapshot
                .item_infos
                .get(&item.item_index)
                .map(|info| info.name.clone())
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

/// 公共窗口事件循环。
fn run_app(
    scene: MapScene,
    center: Point,
    mode: Mode,
    map_dir: PathBuf,
    data_dir: PathBuf,
    settings: ClientSettings,
    settings_path: PathBuf,
) {
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("无法初始化窗口系统（可能没有图形环境）：{error}");
            eprintln!("渲染器仍可用：`cargo test -p mir-render` 会把地图渲染到 target/。");
            return;
        }
    };
    let mut app = ClientApp::new(
        scene,
        center,
        mode,
        map_dir,
        data_dir,
        settings,
        settings_path,
    );
    if let Err(error) = event_loop.run_app(&mut app) {
        eprintln!("窗口事件循环异常退出：{error}");
    }
}

/// 客户端窗口状态。
struct ClientApp {
    scene: MapScene,
    /// 相机中心格。
    center: Point,
    /// 当前帧缓冲（尺寸跟随窗口；启用缩小时是**逻辑**尺寸，见 [`ClientApp::ui_scale`]）。
    frame: Frame,
    /// 运行模式。
    mode: Mode,
    /// 界面文字（TrueType 或点阵兜底）。
    text: UiText,
    /// 地图目录：切图时按 `<map_dir>/<file>.map` 重载地形。
    map_dir: PathBuf,
    /// 素材目录（登录后按服务器地图加载地形、小地图图库）。
    data_dir: PathBuf,
    /// 窗口句柄（`resumed` 里创建）。
    window: Option<Arc<Window>>,
    /// softbuffer 表面。
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    /// softbuffer 上下文（须与表面同生命周期）。
    context: Option<softbuffer::Context<Arc<Window>>>,
    /// 是否显示调试叠层。
    show_overlay: bool,
    /// 下一次动画节拍。
    next_animation: Instant,
    /// 设置文件内容（设置面板每次改动都写回 `settings_path`）。
    settings: ClientSettings,
    /// 设置文件路径。
    settings_path: PathBuf,
    /// 统一设置（`config/client.toml`）。
    client_config: mir_client::client_config::ClientConfig,
    /// 渲染缩放：1 = 原生，2/3 = 逻辑分辨率缩小后放大（Crystal 的「分辨率/缩放」）。
    ui_scale: u32,
    /// 已渲染帧数（性能统计用）。
    render_frames: u64,
    /// 累计渲染耗时（性能统计用）。
    render_total: Duration,
}

impl ClientApp {
    fn new(
        scene: MapScene,
        center: Point,
        mode: Mode,
        map_dir: PathBuf,
        data_dir: PathBuf,
        settings: ClientSettings,
        settings_path: PathBuf,
    ) -> Self {
        let ui_scale = settings.scale();
        let (client_config, config_issue) = mir_client::client_config::ClientConfig::load(
            mir_client::client_config::default_path(),
        );
        if config_issue.is_some()
            && let Err(error) = client_config.save(mir_client::client_config::default_path())
        {
            eprintln!("生成 config/client.toml 失败：{error}");
        }
        Self {
            scene,
            center,
            frame: Frame::new(WINDOW_WIDTH, WINDOW_HEIGHT),
            mode,
            text: UiText::load(None),
            map_dir,
            data_dir,
            window: None,
            surface: None,
            context: None,
            show_overlay: false,
            next_animation: Instant::now() + ANIMATION_INTERVAL,
            settings,
            settings_path,
            client_config,
            ui_scale,
            render_frames: 0,
            render_total: Duration::ZERO,
        }
    }

    /// 创建窗口与软件渲染表面。
    fn create_window(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let title = match &self.mode {
            Mode::Live(_) => {
                mir_client::ui::labels::Label::WindowTitleLive.text(self.settings.language())
            }
            Mode::Login(_) => {
                mir_client::ui::labels::Label::WindowTitleLogin.text(self.settings.language())
            }
            Mode::Offline => {
                mir_client::ui::labels::Label::WindowTitleMap.text(self.settings.language())
            }
        };
        // 分辨率来自设置文件（默认 800×600），启动时应用一次。
        let (width, height) = self.settings.resolution();
        let attributes = Window::default_attributes()
            .with_title(title)
            .with_inner_size(winit::dpi::LogicalSize::new(width, height))
            .with_min_inner_size(winit::dpi::LogicalSize::new(320u32, 240u32))
            .with_resizable(true);
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("无法创建窗口（可能没有图形环境）：{error}");
                eprintln!("渲染器仍可用：`cargo test -p mir-render` 会把地图渲染到 target/。");
                event_loop.exit();
                return;
            }
        };
        let context = match softbuffer::Context::new(window.clone()) {
            Ok(context) => context,
            Err(error) => {
                eprintln!("无法创建软件渲染上下文：{error}");
                event_loop.exit();
                return;
            }
        };
        let surface = match softbuffer::Surface::new(&context, window.clone()) {
            Ok(surface) => surface,
            Err(error) => {
                eprintln!("无法创建软件渲染表面：{error}");
                event_loop.exit();
                return;
            }
        };
        self.surface = Some(surface);
        self.context = Some(context);
        self.window = Some(window);
        self.center = self.clamp_center(self.center);
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_animation));
    }

    /// 把相机中心限制在地图范围内（地图为空时保持原值）。
    fn clamp_center(&self, center: Point) -> Point {
        let max_x = self.scene.map.width.saturating_sub(1).max(0);
        let max_y = self.scene.map.height.saturating_sub(1).max(0);
        Point::new(center.x.clamp(0, max_x), center.y.clamp(0, max_y))
    }

    /// 平移相机并请求重绘（离线模式）。
    fn pan(&mut self, dx: i32, dy: i32) {
        self.center = self.clamp_center(Point::new(self.center.x + dx, self.center.y + dy));
        self.request_redraw();
    }

    /// 请求窗口重绘。
    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// 渲染当前帧（地形 + 联机实体）。
    ///
    /// 实体动作按**真实时间**推进：先让快照把播完的走/跑/攻击切回 Standing、
    /// 把死亡切到 Dead，再把 `now_ms` 传给 `entity_views` 与渲染器。离线模式只画地形，
    /// 行为不受影响。
    fn render_current(&mut self) {
        let started = Instant::now();
        let now = now_millis_u64();
        let fallback_center = self.center;
        let rendered = match &mut self.mode {
            Mode::Offline | Mode::Login(_) => None,
            Mode::Live(live) => {
                // 生产渲染前处理（与延迟回包回归测试共用）：
                // tick_animations → 自然到期/显式打断判定 → apply_camera。
                let prepared = camera::prepare_frame(
                    &mut live.snapshot,
                    &mut live.motion,
                    fallback_center,
                    now,
                );
                Some((prepared.entities, prepared.center, prepared.offset))
            }
        };
        match rendered {
            Some((entities, center, offset)) => {
                self.center = self.clamp_center(center);
                self.scene.render_with_entities_at_offset(
                    &mut self.frame,
                    self.center,
                    offset,
                    &entities,
                    now,
                );
            }
            None => {
                self.scene.render(&mut self.frame, self.center);
            }
        }
        // 粗略性能统计：每 120 帧打印一次平均耗时（含首次解码），便于现场看 FPS。
        self.render_frames = self.render_frames.saturating_add(1);
        self.render_total += started.elapsed();
        if self.render_frames % 120 == 0 {
            let average_ms = self.render_total.as_secs_f64() * 1000.0 / 120.0;
            let fps = if average_ms > 0.0 {
                1000.0 / average_ms
            } else {
                0.0
            };
            println!(
                "渲染统计：{} 帧 · 平均 {average_ms:.1} ms/帧（≈{fps:.0} FPS）· 瓦片缓存 {} · 实体缓存 {} · 地图 {}×{}",
                self.render_frames,
                self.scene.image_cache_len(),
                self.scene.entity_image_cache_len(),
                self.scene.map.width,
                self.scene.map.height,
            );
            self.render_total = Duration::ZERO;
        }
    }

    /// 画调试叠层。
    fn draw_overlay(&mut self) {
        match &self.mode {
            Mode::Offline => overlay::draw(&mut self.frame, self.center),
            Mode::Login(_) => {} // 登录页没有世界叠层
            Mode::Live(live) => {
                let me = live.snapshot.self_player.as_ref();
                overlay::draw_live(
                    &mut self.frame,
                    self.center,
                    live.snapshot.entities.len() + usize::from(me.is_some()),
                    me.map_or(0, |player| player.hp),
                    me.map_or(0, |player| player.mp),
                    live.chat_input.is_some(),
                );
            }
        }
    }

    /// 按当前地图切换背景音乐（切图时 Crystal 也是 `MapControl.LoadMap` 里换曲，
    /// `GameScene.cs:10454-10466`）。
    ///
    /// 优先用服务器 `S.MapInformation.Music` / `S.MapChanged.Music` 的编号
    /// （`music_for_index`），编号为 0 时用 `music_for_map` 兜底（示例数据库全是 0）。
    /// 同一个文件已经在播时 `play_music` 不会重开。
    fn sync_music(&mut self) {
        // 先只读地算出「该放哪个文件 / 该停」，再可变地碰播放器。
        enum MusicChoice {
            Play(std::path::PathBuf),
            Stop,
        }
        let choice = {
            let Mode::Live(live) = &self.mode else {
                return;
            };
            let Some(map) = &live.snapshot.map else {
                return;
            };
            let file = mir_render::music_for_index(map.music)
                .or_else(|| mir_render::music_for_map(&map.file_name, &map.title));
            let path = file.and_then(|file| {
                let resolved = live.sound.library().resolve(file);
                if resolved.is_none() {
                    tracing::debug!(file, "背景音乐素材缺失，静音");
                }
                resolved
            });
            path.map_or(MusicChoice::Stop, MusicChoice::Play)
        };
        let Mode::Live(live) = &mut self.mode else {
            return;
        };
        match choice {
            MusicChoice::Play(path) => {
                if live.sound.music_path() != Some(path.as_path()) {
                    live.sound.play_music(&path);
                }
            }
            MusicChoice::Stop => live.sound.stop_music(),
        }
    }

    /// 画联机界面（世界文字、状态栏、增益、聊天、背包/装备/商店/技能/对话）。
    fn draw_ui(&mut self) {
        let center = self.center;
        if let Mode::Login(login) = &mut self.mode {
            login.draw(&mut self.frame, &self.text);
            return;
        }
        let Mode::Live(live) = &mut self.mode else {
            return;
        };
        live.ui.set_time(now_millis());
        ui::draw(
            &mut live.ui,
            &mut self.frame,
            &mut self.scene,
            &live.snapshot,
            &self.text,
            live.chat_input.as_deref(),
            center,
        );
        // 断线重连提示（覆盖在世界/面板之上）。
        if let Some(message) = live.reconnect.message(live.ui.language) {
            let screen = Rect::screen(self.frame.width, self.frame.height);
            let width = (self.text.text_width(&message, 14) + 20).min(screen.width);
            let banner = Rect::new(
                (screen.width.saturating_sub(width) / 2) as i32,
                screen.y + 76,
                width,
                24,
            );
            self.frame.fill_rect_alpha(
                banner.x,
                banner.y,
                banner.width,
                banner.height,
                [40, 18, 18, 235],
            );
            self.text
                .draw_center(&mut self.frame, banner, &message, 14, [255, 210, 190, 255]);
        }
    }

    /// 渲染当前状态并呈现到窗口。
    fn draw(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let size = window.inner_size();
        let physical_width = size.width.max(1);
        let physical_height = size.height.max(1);
        // 缩放：逻辑帧 = 窗口 / scale，呈现时最近邻放大（scale = 1 时等于原样拷贝）。
        let scale = self.ui_scale.max(1);
        let width = (physical_width / scale).max(1);
        let height = (physical_height / scale).max(1);
        if self.frame.width != width || self.frame.height != height {
            self.frame = Frame::new(width, height);
        }
        self.render_current();
        self.draw_ui();
        if self.show_overlay {
            self.draw_overlay();
        }
        if let Some(surface) = self.surface.as_mut() {
            present(surface, &self.frame, physical_width, physical_height, scale);
        }
    }

    /// 保存截图到 `build/screenshot-<n>.png`。
    fn save_screenshot(&mut self) {
        // 先确保当前帧与相机状态一致（窗口还没创建时也能截图）。
        self.render_current();
        self.draw_ui();
        if self.show_overlay {
            self.draw_overlay();
        }
        let path = next_screenshot_path();
        match self.frame.save_png(&path) {
            Ok(()) => println!(
                "截图已保存：{}（{}×{}）",
                path.display(),
                self.frame.width,
                self.frame.height
            ),
            Err(error) => eprintln!("截图保存失败：{error}"),
        }
    }

    /// 是否正在聊天输入。
    fn chat_active(&self) -> bool {
        matches!(&self.mode, Mode::Live(live) if live.chat_input.is_some())
    }

    /// 聊天输入状态下的按键：回车发送、退格删除、Esc 取消。
    fn handle_chat_key(&mut self, event: &KeyEvent) {
        let Mode::Live(live) = &mut self.mode else {
            return;
        };
        match &event.logical_key {
            Key::Named(NamedKey::Enter) => {
                if let Some(message) = live.chat_input.take() {
                    if let Some(command) = chat_submit_command(&message) {
                        live.send(command);
                    }
                }
            }
            Key::Named(NamedKey::Escape) => live.chat_input = None,
            Key::Named(NamedKey::Backspace) => {
                if let Some(input) = &mut live.chat_input {
                    input.pop();
                }
            }
            Key::Named(NamedKey::Space) => {
                if let Some(input) = &mut live.chat_input {
                    input.push(' ');
                }
            }
            Key::Character(text) => {
                if let Some(input) = &mut live.chat_input {
                    if !text.chars().any(char::is_control) {
                        input.push_str(text);
                    }
                }
            }
            _ => {}
        }
    }

    /// 联机移动：方向与当前朝向相同则走/跑，否则转身。
    ///
    /// Crystal 的键盘语义在 `PlayerObject.cs:1313-1334`：解析出的动作是
    /// `MirAction.Standing` 时发 `C.Turn`、`Walking` 时发 `C.Walk`、`Running` 时发
    /// `C.Run`。本客户端没有行走/跑步缓冲，直接用「方向是否与当前朝向一致」决定。
    fn live_move(&mut self, direction: MirDirection) {
        let Mode::Live(live) = &mut self.mode else {
            return;
        };
        // 键盘移动打断 A* 点地走路。
        live.ui.cancel_path();
        let Some(me) = &live.snapshot.self_player else {
            return;
        };
        let shift = live.shift;
        let command = if direction == me.direction {
            if shift {
                NetCommand::Run(direction)
            } else {
                NetCommand::Walk(direction)
            }
        } else {
            NetCommand::Turn(direction)
        };
        live.send(command);
    }

    /// 联机攻击：有锁定目标时朝目标攻击/靠近，否则朝当前朝向挥一下。
    ///
    /// 锁定目标后的语义见 [`UiState::attack_target`]：相邻 → `C.Attack`，
    /// 不相邻 → 先朝目标走一步（最多 3 步，走到相邻自动攻击）。
    fn live_attack(&mut self) {
        if let Mode::Live(live) = &mut self.mode {
            live.ui.cancel_path();
        }
        let action = match &mut self.mode {
            Mode::Live(live) => {
                if live.ui.target_object_id.is_some() {
                    live.ui.set_time(now_millis());
                    live.ui.attack_target(&live.snapshot)
                } else {
                    None
                }
            }
            Mode::Offline | Mode::Login(_) => None,
        };
        if let Some(action) = action {
            self.dispatch_ui_action(action);
            return;
        }
        if let Mode::Live(live) = &mut self.mode {
            if let Some(direction) = live.snapshot.self_player.as_ref().map(|me| me.direction) {
                live.send(NetCommand::Attack(direction));
            }
        }
    }

    /// 每个节拍推进「靠近锁定目标」：相邻后自动攻击，走满 3 步放弃。
    fn tick_targeting(&mut self) {
        let action = match &mut self.mode {
            Mode::Live(live) => {
                live.ui.set_time(now_millis());
                live.ui.tick_approach(&live.snapshot)
            }
            Mode::Offline | Mode::Login(_) => None,
        };
        if let Some(action) = action {
            self.dispatch_ui_action(action);
        }
    }

    /// 每个节拍推进点地走路的 A* 路径（每 600 ms 一格，见 `UiState::tick_path`）。
    fn tick_path(&mut self) {
        let action = match &mut self.mode {
            Mode::Live(live) => {
                live.ui.set_time(now_millis());
                live.ui.set_path_running(live.shift);
                live.ui.tick_path(&live.snapshot)
            }
            Mode::Offline | Mode::Login(_) => None,
        };
        if let Some(action) = action {
            self.dispatch_ui_action(action);
        }
    }

    /// 联机捡取。
    fn live_pickup(&mut self) {
        if let Mode::Live(live) = &mut self.mode {
            live.send(NetCommand::PickUp);
        }
    }

    /// 打开聊天输入。
    fn live_open_chat(&mut self) {
        if let Mode::Live(live) = &mut self.mode {
            if live.snapshot.is_ready() {
                live.chat_input = Some(String::new());
                println!("聊天输入：输入内容后回车发送，Esc 取消");
            }
        }
    }

    /// 背包面板是否打开。
    fn live_inventory_open(&self) -> bool {
        matches!(&self.mode, Mode::Live(live) if live.ui.show_inventory)
    }

    /// 商店面板是否打开。
    fn live_shop_open(&self) -> bool {
        matches!(&self.mode, Mode::Live(live) if live.ui.shop_open)
    }

    /// 把界面动作映射成网络命令。
    fn dispatch_ui_action(&mut self, action: UiAction) {
        // 设置动作不需要网络线程，先在这里落地（音量/语言/显示 + 写回设置文件）。
        if let UiAction::Settings(action) = &action {
            self.apply_settings(*action);
            self.request_redraw();
            return;
        }
        let Mode::Live(live) = &mut self.mode else {
            return;
        };
        match action {
            // 纯数据动作直接走 `UiAction::net_command`（与无头测试共用同一映射）。
            action @ (UiAction::BuyItem { .. }
            | UiAction::SellItem { .. }
            | UiAction::DropItem { .. }
            | UiAction::MoveItem { .. }
            | UiAction::WalkStep { .. }
            | UiAction::PickUp
            | UiAction::TownRevive
            | UiAction::RepairItem { .. }
            | UiAction::AbandonQuest { .. }
            | UiAction::ReadMail { .. }
            | UiAction::CollectParcel { .. }
            | UiAction::LockMail { .. }
            | UiAction::DeleteMail { .. }
            | UiAction::StoreItem { .. }
            | UiAction::TakeBackItem { .. }
            | UiAction::GuildStorageItemChange { .. }
            | UiAction::GuildStorageGoldChange { .. }
            | UiAction::GuildBuffUpdate { .. }
            | UiAction::EditGuildMember { .. }
            | UiAction::EditGuildNotice { .. }
            | UiAction::RequestGuildInfo { .. }
            | UiAction::TradeRequest
            | UiAction::TradeReply { .. }
            | UiAction::TradeItem { .. }
            | UiAction::TradeTakeBack { .. }
            | UiAction::TradeGold { .. }
            | UiAction::TradeConfirm { .. }
            | UiAction::TradeCancel
            | UiAction::AddMember { .. }
            | UiAction::DelMember { .. }
            | UiAction::GroupReply { .. }
            | UiAction::SwitchGroup { .. }
            | UiAction::MarketSearch { .. }
            | UiAction::MarketRefresh
            | UiAction::MarketPage { .. }
            | UiAction::MarketBuy { .. }
            | UiAction::MarketSellNow { .. }
            | UiAction::MarketGetBack { .. }
            | UiAction::GameShopBuy { .. }
            | UiAction::Chat(_)
            | UiAction::AddFriend { .. }
            | UiAction::RemoveFriend { .. }
            | UiAction::RefreshFriends
            | UiAction::AddMemo { .. }
            | UiAction::ConsignItem { .. }) => {
                if let Some(command) = action.net_command() {
                    live.send(command);
                }
            }
            // 写信：发送成功后清空表单（服务端会回 `S.MailSent`，这里先按成功清空；
            // 失败时服务端重发列表，表单内容已丢——简化，Crystal 也是关窗即清）。
            UiAction::SendMail {
                name,
                message,
                gold,
                items_idx,
                stamped,
            } => {
                live.send(NetCommand::SendMail {
                    name,
                    message,
                    gold,
                    items_idx,
                    stamped,
                });
                live.ui.mail_compose.clear();
                live.ui.mail_tab = mir_client::ui::mail::MailTab::Inbox;
                live.ui.clear_focus();
            }
            UiAction::MailCost {
                gold,
                items_idx,
                stamped,
            } => {
                live.send(NetCommand::MailCost {
                    gold,
                    items_idx,
                    stamped,
                });
            }
            UiAction::UseItem { unique_id } => {
                // 喝药音：Crystal 在点背包里的药品时播 `ClickDrug`
                // （`MirItemCell.cs:2047-2048`）；这里只在「用」药品时播（简化）。
                if used_item_is_potion(&live.snapshot, unique_id) {
                    live.snapshot.push_local_sound(SoundId::Drink, Spell::None);
                    live.play_pending_sounds(now_millis_u64());
                }
                live.send(NetCommand::UseItem { unique_id });
            }
            UiAction::CallNpc { object_id, key } => {
                // `@exit` 由 UI 层立刻关窗（服务端随后回空页确认）。
                live.ui.close_npc_for_key(&key);
                live.send(NetCommand::CallNpc { object_id, key });
            }
            UiAction::CastMagic {
                spell,
                direction,
                target_id,
                location,
            } => {
                // 施法音（`PlayerObject.cs:1538` 的 `PlaySound(20000 + Spell × 10)`）：
                // 服务端不回显，本地先播。
                live.snapshot.push_local_sound(SoundId::Cast, spell);
                live.play_pending_sounds(now_millis_u64());
                live.send(NetCommand::Magic {
                    spell,
                    direction,
                    target_id,
                    location,
                });
            }
            UiAction::BindMagic {
                spell,
                key,
                old_key,
            } => {
                live.send(NetCommand::MagicKey {
                    spell,
                    key,
                    old_key,
                });
                // 服务端不回显绑定，先在本地更新，技能面板下一帧就能看到新键位。
                if let Some(magic) = live
                    .snapshot
                    .magics
                    .iter_mut()
                    .find(|magic| magic.spell == spell)
                {
                    magic.key = key;
                }
            }
            UiAction::WorldMove { direction, step } => {
                let facing = live.snapshot.self_player.as_ref().map(|me| me.direction);
                if step || facing == Some(direction) {
                    live.send(NetCommand::Walk(direction));
                } else {
                    live.send(NetCommand::Turn(direction));
                }
            }
            // 朝锁定目标攻击（`C.Attack`）。目标相邻时由 `UiState::attack_target`
            // 产生，非相邻时会先产生 `WorldMove`（走一步靠近）。
            UiAction::AttackTarget { direction } => {
                live.send(NetCommand::Attack(direction));
            }
            // 设置动作在上面已经提前返回（不需要 `live` 之外的窗口句柄等字段）。
            UiAction::Settings(_) | UiAction::DismissDeath => {}
        }
        self.request_redraw();
    }

    /// 落实设置面板动作：运行期生效 + 写回设置文件（重启后仍生效）。
    ///
    /// 对应 Crystal `OptionDialog` 的音量滑条（`Client/MirScenes/Dialogs/MainDialogs.cs:2833-2846`）
    /// 与 `Settings.Save()`（`Client/Forms/CMain.cs:699-713`）；语言切换立即重绘
    /// （`ui.language` 与面板 `state.language` 同步）。
    fn apply_settings(&mut self, action: ui::settings::SettingsAction) {
        use ui::settings::SettingsAction as Action;
        // 面板状态已被点击更新，这里按它落实运行期 + 设置文件。
        let mut language = None;
        if let Mode::Live(live) = &mut self.mode {
            let panel = live.ui.settings.clone();
            match action {
                Action::ToggleSound => {
                    live.sound.set_enabled(panel.sound_on);
                }
                Action::ToggleMusic => {
                    live.sound.set_music_enabled(panel.music_on);
                }
                Action::SoundVolume(percent) => {
                    live.sound
                        .set_volume(ui::settings::SettingsState::fraction(percent));
                }
                Action::MusicVolume(percent) => {
                    live.sound
                        .set_music_volume(ui::settings::SettingsState::fraction(percent));
                }
                Action::SetLanguage(next) => {
                    live.ui.language = next;
                    self.client_config.language.clear();
                    self.client_config.language.push_str(next.code());
                    language = Some(next);
                    // 同步服务端账号语言：协议里没有语言包，服务端提供 `/language <代码>`
                    // 聊天扩展命令（`mir-server/src/session.rs` 的 `on_chat`），会写回
                    // `accounts.language` 并同步世界层，之后的系统聊天/邮件/市场回执
                    // 都按新语言下发。不发这条命令的话，界面切了英文但服务端提示仍是中文。
                    live.send(NetCommand::Chat(format!("/language {}", next.code())));
                }
                Action::MasterVolume(percent) => {
                    let master = ui::settings::SettingsState::fraction(percent);
                    self.client_config.master_volume = master;
                    live.sound.set_master_volume(master);
                }
                Action::NextWindowMode => {
                    self.client_config.window_mode = panel.window_mode;
                    if let Some(window) = &self.window {
                        use winit::window::Fullscreen;
                        window.set_fullscreen(match panel.window_mode {
                            mir_client::client_config::WindowMode::Windowed => None,
                            mir_client::client_config::WindowMode::Fullscreen
                            | mir_client::client_config::WindowMode::Borderless => {
                                Some(Fullscreen::Borderless(None))
                            }
                        });
                    }
                }
                Action::ToggleMinimapGates => {
                    self.client_config.show_minimap_gates = panel.show_minimap_gates;
                }
                Action::RebindKey(_) => {
                    live.ui.settings.rebind_error = None;
                }
                Action::SetResolution(width, height) => {
                    if let Some(window) = &self.window {
                        let _ =
                            window.request_inner_size(winit::dpi::LogicalSize::new(width, height));
                    }
                }
                Action::SetScale(scale) => {
                    self.ui_scale = scale.clamp(1, 3);
                }
                Action::Close => {
                    live.ui.show_settings = false;
                }
            }
            self.settings.absorb_panel(&panel);
        } else {
            return;
        }
        if let Some(language) = language {
            // 语言切换立即生效：设置面板/状态栏/各面板都读 `ui.language`。
            tracing::info!(?language, "界面语言切换");
        }
        if let Err(error) = self.settings.save(&self.settings_path) {
            eprintln!("保存设置失败（{}）：{error}", self.settings_path.display());
        }
        if let Err(error) = self
            .client_config
            .save(mir_client::client_config::default_path())
        {
            eprintln!("保存 config/client.toml 失败：{error}");
        }
    }

    /// 打开设置面板前把设置文件的值同步进面板（命令行覆盖只影响本次运行）。
    fn sync_settings_panel(&mut self) {
        if let Mode::Live(live) = &mut self.mode {
            let mut panel = self.settings.panel();
            panel.master_volume =
                ui::settings::SettingsState::percent(self.client_config.master_volume);
            panel.window_mode = self.client_config.window_mode;
            panel.show_minimap_gates = self.client_config.show_minimap_gates;
            panel.keys = self.client_config.keys.clone();
            panel.rebinding = None;
            panel.rebind_error = None;
            live.ui.settings = panel;
        }
    }

    /// 登录页按键：地址/账号/口令输入、回车连接、Tab 换焦点、Esc 退出/返回。
    fn handle_login_key(&mut self, event_loop: &ActiveEventLoop, event: &KeyEvent) {
        let mut connect = false;
        let mut enter = false;
        let mut back = false;
        let mut exit = false;
        let mut redraw = false;
        if let Mode::Login(login) = &mut self.mode {
            match login.screen {
                LoginScreen::Server => match &event.logical_key {
                    Key::Named(NamedKey::Escape) => exit = true,
                    Key::Named(NamedKey::Enter) => connect = login.server.can_connect(),
                    Key::Named(NamedKey::Tab) => {
                        login.server.focus = match login.server.focus {
                            LoginField::Server => LoginField::Account,
                            LoginField::Account => LoginField::Password,
                            LoginField::Password => LoginField::Server,
                        };
                        redraw = true;
                    }
                    Key::Named(NamedKey::Backspace) => {
                        match login.server.focus {
                            LoginField::Server => login.server.address.backspace(),
                            LoginField::Account => login.server.account.backspace(),
                            LoginField::Password => login.server.password.backspace(),
                        }
                        redraw = true;
                    }
                    Key::Character(text) => {
                        for character in text.chars() {
                            match login.server.focus {
                                LoginField::Server => login.server.address.insert(character),
                                LoginField::Account => login.server.account.insert(character),
                                LoginField::Password => login.server.password.insert(character),
                            }
                        }
                        redraw = true;
                    }
                    _ => {}
                },
                LoginScreen::Characters => match &event.logical_key {
                    Key::Named(NamedKey::Escape) => back = true,
                    Key::Named(NamedKey::Enter) => {
                        enter = login.characters.current().is_some() && !login.characters.creating;
                    }
                    Key::Named(NamedKey::Backspace) if login.characters.creating => {
                        login.characters.create_name.backspace();
                        redraw = true;
                    }
                    Key::Character(text) if login.characters.creating => {
                        for character in text.chars() {
                            login.characters.create_name.insert(character);
                        }
                        redraw = true;
                    }
                    _ => {}
                },
            }
        }
        if exit {
            event_loop.exit();
            return;
        }
        if back {
            self.login_back_to_server();
            return;
        }
        if connect {
            self.login_connect();
            return;
        }
        if enter {
            self.login_enter_selected();
            return;
        }
        if redraw {
            self.request_redraw();
        }
    }

    /// 登录页点击：服务器页的连接/退出/焦点，选角页的选中/进入/创建。
    fn login_click(&mut self, event_loop: &ActiveEventLoop) {
        let screen = Rect::screen(self.frame.width, self.frame.height);
        let mut connect = false;
        let mut enter = false;
        let mut create = false;
        let mut back = false;
        let mut quit = false;
        if let Mode::Login(login) = &mut self.mode {
            match login.screen {
                LoginScreen::Server => {
                    let layout = select::server_layout(screen);
                    match select::server_click(&layout, &mut login.server, login.mouse) {
                        Some(ServerAction::Quit) => quit = true,
                        Some(ServerAction::Connect) => connect = true,
                        Some(ServerAction::Focus(_)) | None => {}
                    }
                }
                LoginScreen::Characters => {
                    let layout = select::character_layout(
                        screen,
                        login.characters.characters.len(),
                        login.characters.creating,
                    );
                    match select::character_click(&layout, &mut login.characters, login.mouse) {
                        Some(CharacterAction::Back) => back = true,
                        Some(CharacterAction::Enter(_)) => enter = true,
                        Some(CharacterAction::Create) => create = true,
                        Some(
                            CharacterAction::Select(_)
                            | CharacterAction::ToggleCreate
                            | CharacterAction::CycleClass(_)
                            | CharacterAction::CycleGender(_),
                        )
                        | None => {}
                    }
                }
            }
        }
        if quit {
            event_loop.exit();
            return;
        }
        if back {
            self.login_back_to_server();
            return;
        }
        if connect {
            self.login_connect();
            return;
        }
        if create {
            self.login_create();
            return;
        }
        if enter {
            self.login_enter_selected();
            return;
        }
        self.request_redraw();
    }

    /// 发起登录（连接失败可重试：`ServerSelectState::fail` 之后按钮变成「重试」）。
    fn login_connect(&mut self) {
        let Mode::Login(login) = &mut self.mode else {
            return;
        };
        if login.busy() || !login.server.can_connect() {
            return;
        }
        let server = login.server.address.value.trim().to_owned();
        let options = ConnectOptions {
            server: server.clone(),
            account: login.server.account.value.trim().to_owned(),
            password: login.server.password.value.clone(),
            character: String::new(),
            create: None,
            auto_register: true,
            manual_select: true,
        };
        login.server.begin_connect();
        tracing::info!(
            server = %server,
            account = %options.account,
            "登录页发起连接"
        );
        if !login.announced {
            println!("连接服务器 {server} …");
            login.announced = true;
        }
        // 记住服务器地址（下次启动默认用它）。
        self.settings.set_server(&server);
        if let Err(error) = self.settings.save(&self.settings_path) {
            eprintln!("保存设置失败（{}）：{error}", self.settings_path.display());
        }
        let (sender, receiver) = mpsc::channel();
        login.pending = Some(receiver);
        login.connect_started = Some(Instant::now());
        std::thread::spawn(move || {
            match NetClient::connect(options) {
                Ok(client) => {
                    // 手动选角：等到 `SelectingCharacter`（`S.LoginSuccess` 之后）或失败。
                    match client.wait_for_snapshot(
                        |snapshot| {
                            matches!(
                                snapshot.status,
                                NetStatus::SelectingCharacter | NetStatus::Failed
                            )
                        },
                        Duration::from_secs(15),
                    ) {
                        Ok(snapshot) => {
                            let _ =
                                sender.send(LoginOutcome::Characters(client, Box::new(snapshot)));
                        }
                        Err(error) => {
                            let message = format!("{error:#}");
                            let _ = sender.send(LoginOutcome::Failed(
                                classify_login_error(&message),
                                message,
                            ));
                        }
                    }
                }
                Err(error) => {
                    let message = format!("{error:#}");
                    let _ = sender.send(LoginOutcome::Failed(
                        select::ConnectError::Unreachable,
                        message,
                    ));
                }
            }
        });
        self.request_redraw();
    }

    /// 进入选中角色（`C.StartGame`）。
    fn login_enter_selected(&mut self) {
        let Mode::Login(login) = &mut self.mode else {
            return;
        };
        if login.busy() {
            return;
        }
        let Some(index) = login.characters.current().map(|card| card.index) else {
            return;
        };
        let Some(client) = login.net.take() else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        login.pending = Some(receiver);
        login.connect_started = Some(Instant::now());
        let exited = mir_client::ui::labels::Label::NetworkThreadExited
            .text(self.settings.language())
            .to_owned();
        std::thread::spawn(move || {
            if !client.send(NetCommand::SelectCharacter { index }) {
                let _ = sender.send(LoginOutcome::Failed(select::ConnectError::Other, exited));
                return;
            }
            match client.wait_until_in_game(Duration::from_secs(20)) {
                Ok(snapshot) => {
                    let _ = sender.send(LoginOutcome::InGame(client, Box::new(snapshot)));
                }
                Err(error) => {
                    let message = format!("{error:#}");
                    let _ = sender.send(LoginOutcome::Failed(
                        classify_login_error(&message),
                        message,
                    ));
                }
            }
        });
    }

    /// 创建角色（`C.NewCharacter`），成功后回到选角页。
    fn login_create(&mut self) {
        let Mode::Login(login) = &mut self.mode else {
            return;
        };
        if login.busy() || !login.characters.can_create() {
            return;
        }
        let name = login.characters.create_name.value.trim().to_owned();
        let class = login.characters.create_class;
        let gender = login.characters.create_gender;
        let Some(client) = login.net.take() else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        login.pending = Some(receiver);
        login.connect_started = Some(Instant::now());
        let exited = mir_client::ui::labels::Label::NetworkThreadExited
            .text(self.settings.language())
            .to_owned();
        std::thread::spawn(move || {
            if !client.send(NetCommand::NewCharacter {
                name: name.clone(),
                class,
                gender,
            }) {
                let _ = sender.send(LoginOutcome::Failed(select::ConnectError::Other, exited));
                return;
            }
            let wanted = name.clone();
            match client.wait_for_snapshot(
                move |snapshot| snapshot.characters.iter().any(|card| card.name == wanted),
                Duration::from_secs(15),
            ) {
                Ok(snapshot) => {
                    let _ = sender.send(LoginOutcome::Characters(client, Box::new(snapshot)));
                }
                Err(error) => {
                    let message = format!("{error:#}");
                    let _ = sender.send(LoginOutcome::Failed(
                        classify_login_error(&message),
                        message,
                    ));
                }
            }
        });
    }

    /// 返回服务器选择页（丢弃当前连接）。
    fn login_back_to_server(&mut self) {
        if let Mode::Login(login) = &mut self.mode {
            login.net = None;
            login.pending = None;
            login.connect_started = None;
            login.screen = LoginScreen::Server;
            login.server.succeed();
            login.characters.clear_error();
        }
        self.request_redraw();
    }

    /// 自动化/诊断入口：`MIR2_LOGIN_AUTOCONNECT=1` 时登录窗口自动点一次「连接」。
    ///
    /// 只用于脚本化复现（playtest/CI），默认不启用；账号/口令由 `--account`/`--password` 预填。
    fn maybe_auto_connect_login(&mut self) {
        static ONCE: std::sync::Once = std::sync::Once::new();
        if std::env::var_os("MIR2_LOGIN_AUTOCONNECT").is_none() {
            return;
        }
        if !matches!(self.mode, Mode::Login(_)) {
            return;
        }
        let mut run = false;
        ONCE.call_once(|| run = true);
        if run {
            self.login_connect();
        }
    }

    /// 登录看门狗：网络线程异常/卡住时，别让按钮永远停在「连接中」。
    fn check_login_watchdog(&mut self) {
        const LIMIT: Duration = Duration::from_secs(25);
        let stale = match &self.mode {
            Mode::Login(login) => {
                login.pending.is_some()
                    && login
                        .connect_started
                        .is_some_and(|started| started.elapsed() > LIMIT)
            }
            _ => false,
        };
        if !stale {
            return;
        }
        tracing::warn!("登录页等待超过 {}s，重置为可重试", LIMIT.as_secs());
        if let Mode::Login(login) = &mut self.mode {
            login.pending = None;
            login.connect_started = None;
            login.connect_started = None;
            login.server.fail(
                select::ConnectError::Other,
                Some("等待服务器响应超时".to_owned()),
            );
        }
        self.request_redraw();
    }

    /// 轮询登录线程的结果。
    fn poll_login(&mut self) {
        let outcome = {
            let Mode::Login(login) = &mut self.mode else {
                return;
            };
            let Some(receiver) = &login.pending else {
                return;
            };
            match receiver.try_recv() {
                Ok(outcome) => Some(outcome),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(LoginOutcome::Failed(
                    select::ConnectError::Other,
                    mir_client::ui::labels::Label::LoginThreadExited
                        .text(self.settings.language())
                        .to_owned(),
                )),
            }
        };
        let Some(outcome) = outcome else {
            return;
        };
        if let Mode::Login(login) = &mut self.mode {
            login.pending = None;
            login.connect_started = None;
        }
        match outcome {
            LoginOutcome::Characters(client, snapshot) => {
                let count = snapshot.characters.len();
                if let Mode::Login(login) = &mut self.mode {
                    login.server.succeed();
                    login.characters.set_characters(&snapshot.characters);
                    login.characters.clear_error();
                    login.net = Some(client);
                    login.screen = LoginScreen::Characters;
                }
                println!("登录成功：账号下 {count} 个角色");
            }
            LoginOutcome::InGame(client, snapshot) => {
                self.enter_world(client, *snapshot);
            }
            LoginOutcome::Failed(error, message) => {
                tracing::warn!(error = ?error, %message, "登录页连接失败");
                eprintln!("登录失败：{message}");
                if let Mode::Login(login) = &mut self.mode {
                    // 连接已经随线程丢弃，回到服务器页让玩家改地址/重试。
                    login.net = None;
                    login.screen = LoginScreen::Server;
                    login.server.fail(error, Some(message));
                }
            }
        }
        self.request_redraw();
    }

    /// 登录成功：加载服务器地图并切到联机模式（复用 `LiveState`）。
    fn enter_world(&mut self, net: NetClient, snapshot: WorldSnapshot) {
        let map_name = snapshot
            .map
            .as_ref()
            .map(|map| map.file_name.clone())
            .unwrap_or_default();
        let scene = load_world_scene(&map_name, &self.map_dir, &self.data_dir);
        let center = snapshot
            .self_player
            .as_ref()
            .map_or(Point::new(330, 270), |me| me.position);
        let language = self.settings.language();
        let mut ui = UiState::new(language);
        let (minimap_scene, reason) = mir_render::MinimapScene::open_dir(&self.data_dir);
        if let Some(reason) = reason {
            println!("小地图素材不可用：{reason}（M 面板会显示纯色占位）");
        }
        ui.minimap_scene = minimap_scene;
        ui.settings = self.settings.panel();
        ui.settings.language = language;
        let status = snapshot.status;
        let sound = if let Mode::Login(login) = &mut self.mode {
            login.sound.take().unwrap_or_else(|| {
                SoundPlayer::without_device(SoundLibrary::default_roots(), false, 0.0)
            })
        } else {
            SoundPlayer::without_device(SoundLibrary::default_roots(), false, 0.0)
        };
        println!(
            "已进入世界：{} · 对象 {} 个 · 地图 {}（{}×{}）",
            snapshot
                .self_player
                .as_ref()
                .map_or("?", |player| player.name.as_str()),
            snapshot.entities.len(),
            scene.map.file_name,
            scene.map.width,
            scene.map.height
        );
        println!(
            "方向键/WASD 走 · Space 攻击 · T 聊天 · I 背包 · E 装备 · F3 技能 · M 小地图 · O 设置 · Esc 关面板 · F10 退出"
        );
        // 记录装备摘要（选角页显示）。
        if let Some(me) = snapshot.self_player.as_ref() {
            let summary = equipment_summary(&snapshot);
            if !summary.is_empty() && self.settings.equipment.get(&me.name) != Some(&summary) {
                self.settings.equipment.insert(me.name.clone(), summary);
                if let Err(error) = self.settings.save(&self.settings_path) {
                    eprintln!("保存设置失败（{}）：{error}", self.settings_path.display());
                }
            }
        }
        self.scene = scene;
        self.center = center;
        self.mode = Mode::Live(Box::new(LiveState {
            net,
            ui,
            snapshot,
            sound,
            last_sound_id: 0,
            last_wall_bump_id: 0,
            chat_input: None,
            shift: false,
            last_chat: None,
            last_status: status,
            entered: false,
            left_held: false,
            walk_target: None,
            next_walk: Instant::now(),
            last_hover_cell: None,
            reconnect: ReconnectState::default(),
            connect: ConnectOptions::default(),
            motion: None,
        }));
        if let Some(window) = &self.window {
            window.set_title(
                mir_client::ui::labels::Label::WindowTitleLive.text(self.settings.language()),
            );
        }
    }

    /// 市场搜索框回车 / 「搜索」按钮（`C.MarketSearch`）。
    fn live_market_search(&mut self) -> bool {
        let actions = match &mut self.mode {
            Mode::Live(live) => live.ui.market_open_requests(),
            Mode::Offline | Mode::Login(_) => Vec::new(),
        };
        let handled = !actions.is_empty();
        for action in actions {
            self.dispatch_ui_action(action);
        }
        handled
    }

    /// 选中的背包物品（唯一编号, 数量）。
    fn selected_inventory(&self) -> Option<(u64, u16)> {
        let Mode::Live(live) = &self.mode else {
            return None;
        };
        live.ui
            .selected_inventory_item(&live.snapshot)
            .map(|item| (item.unique_id, item.count))
    }

    /// 按数字键施法；没有技能时返回 `false`（让按键落到别处）。
    fn live_cast_key(&mut self, key: u8) -> bool {
        let actions = match &mut self.mode {
            Mode::Live(live) => live.ui.press_magic_key(&live.snapshot, key),
            Mode::Offline | Mode::Login(_) => Vec::new(),
        };
        let handled = !actions.is_empty();
        for action in actions {
            self.dispatch_ui_action(action);
        }
        handled
    }

    /// 卖一件选中的背包物品（商店面板打开时 `S`）。
    ///
    /// 动作由 [`UiState::sell_selected_item`] 产生（`app.rs` 与无头测试共用一份判断），
    /// 再走 [`Self::dispatch_ui_action`] 发 `C.SellItem`。
    fn live_sell_selected(&mut self) -> bool {
        let action = match &self.mode {
            Mode::Live(live) => live.ui.sell_selected_item(&live.snapshot),
            Mode::Offline | Mode::Login(_) => None,
        };
        let Some(action) = action else {
            return false;
        };
        self.dispatch_ui_action(action);
        true
    }

    /// 修理选中的背包物品（商店面板打开时 `R`）。
    fn live_repair_selected(&mut self) -> bool {
        let action = match &self.mode {
            Mode::Live(live) => live.ui.repair_selected_item(&live.snapshot),
            Mode::Offline | Mode::Login(_) => None,
        };
        let Some(action) = action else {
            return false;
        };
        self.dispatch_ui_action(action);
        true
    }

    /// 丢弃选中的背包物品（背包面板打开时 `D`）。
    fn live_drop_selected(&mut self) -> bool {
        let Some((unique_id, _)) = self.selected_inventory() else {
            return false;
        };
        if let Mode::Live(live) = &mut self.mode {
            live.send(NetCommand::DropItem {
                unique_id,
                count: 1,
            });
        }
        true
    }

    /// `I`：开关背包。
    fn live_toggle_inventory(&mut self) {
        let mut redraw = false;
        if let Mode::Live(live) = &mut self.mode {
            live.ui.show_inventory = !live.ui.show_inventory;
            redraw = true;
        }
        if redraw {
            self.request_redraw();
        }
    }

    /// `F3`：开关技能列表。
    fn live_toggle_skills(&mut self) {
        let mut redraw = false;
        if let Mode::Live(live) = &mut self.mode {
            live.ui.show_skills = !live.ui.show_skills;
            redraw = true;
        }
        if redraw {
            self.request_redraw();
        }
    }

    /// `E`：开关装备纸娃娃面板。
    fn live_toggle_equipment(&mut self) {
        let mut redraw = false;
        if let Mode::Live(live) = &mut self.mode {
            live.ui.show_equipment = !live.ui.show_equipment;
            redraw = true;
        }
        if redraw {
            self.request_redraw();
        }
    }

    /// `P`：开关组队面板（Crystal `KeybindOptions.Group = Keys.P`，
    /// `Client/KeyBindSettings.cs:225` → `GroupDialog.Show()/Hide()`，`GameScene.cs:717-719`）。
    ///
    /// 组队数据由服务端推送（`S.AddMember` / `S.GroupMembersMap` / `S.SendMemberLocation`），
    /// 打开面板不需要额外请求包。
    fn live_toggle_group(&mut self) {
        let mut redraw = false;
        if let Mode::Live(live) = &mut self.mode {
            live.ui.toggle_group();
            redraw = true;
        }
        if redraw {
            self.request_redraw();
        }
    }

    /// `Y`：发起交易（`C.TradeRequest`；服务端按朝向取相邻玩家）。
    fn live_request_trade(&mut self) {
        if let Mode::Live(live) = &mut self.mode {
            if live.snapshot.is_ready() {
                live.send(NetCommand::TradeRequest);
            }
        }
    }

    /// `M`：开关小地图（Crystal `KeybindOptions.Minimap` → `MiniMapDialog.Toggle()`，
    /// `GameScene.cs:770-772`）。
    fn live_toggle_minimap(&mut self) {
        let mut redraw = false;
        if let Mode::Live(live) = &mut self.mode {
            live.ui.toggle_minimap();
            redraw = true;
        }
        if redraw {
            self.request_redraw();
        }
    }

    /// 开关设置面板（`O`）：打开前把设置文件的值同步进面板。
    fn open_settings(&mut self) {
        self.sync_settings_panel();
        let mut redraw = false;
        if let Mode::Live(live) = &mut self.mode {
            live.ui.toggle_settings();
            redraw = true;
        }
        if redraw {
            self.request_redraw();
        }
    }

    /// `B`：开关大地图（Crystal `KeybindOptions.Bigmap` → `BigMapDialog.Toggle()`，
    /// `GameScene.cs:773-774`）。
    fn live_toggle_bigmap(&mut self) {
        let mut redraw = false;
        if let Mode::Live(live) = &mut self.mode {
            live.ui.toggle_bigmap();
            redraw = true;
        }
        if redraw {
            self.request_redraw();
        }
    }

    /// `Q`：开关任务面板（Crystal `KeybindOptions.Quests` → `QuestLogDialog`，
    /// `GameScene.cs:660-664`；默认键就是 `Q`，见 `KeyBindSettings.cs:219`）。
    fn live_toggle_quests(&mut self) {
        let mut redraw = false;
        if let Mode::Live(live) = &mut self.mode {
            live.ui.toggle_quests();
            redraw = true;
        }
        if redraw {
            self.request_redraw();
        }
    }

    /// 宠物模式三态循环（`C.ChangePMode`，index 45）。
    ///
    /// 键位用 `V`（`P` 已被 Crystal 的组队窗口占用，见 `KeybindSettings.cs:225`）。
    /// 没有宠物时只发一条本地系统提示；有宠物时乐观更新 + 发包 + 聊天行提示。
    fn live_cycle_pet_mode(&mut self) {
        let mut redraw = false;
        if let Mode::Live(live) = &mut self.mode {
            if live.snapshot.is_ready() {
                if live.snapshot.has_pet() {
                    let mode = mir_client::net::next_pet_mode(live.snapshot.pet_mode);
                    live.snapshot.pet_mode = mode;
                    live.send(NetCommand::ChangePetMode(mode));
                    live.snapshot
                        .notify(mir_client::net::SystemChat::PetModeChanged { mode });
                } else {
                    live.snapshot.notify(mir_client::net::SystemChat::NoPet);
                }
                redraw = true;
            }
        }
        if redraw {
            self.request_redraw();
        }
    }

    /// `N`：开关邮件面板（Crystal 的邮件窗口没有默认键，本作按需求绑 `N`）。
    fn live_toggle_mail(&mut self) {
        let mut redraw = false;
        if let Mode::Live(live) = &mut self.mode {
            live.ui.toggle_mail();
            redraw = true;
        }
        if redraw {
            self.request_redraw();
        }
    }

    /// `G`：开关行会面板（Crystal `KeybindOptions.Guilds` → `GuildDialog`，
    /// `GameScene.cs:648-655`）。
    ///
    /// 打开时顺手拉公告/成员/仓库/增益（Crystal `GuildDialog.Show()` 里用
    /// `RequestGuildInfo` / `GuildStorageItemChange{Type=3}` / `GuildBuffUpdate{Action=0}` 拉）。
    fn live_toggle_guild(&mut self) {
        let mut actions = Vec::new();
        let mut opened = false;
        if let Mode::Live(live) = &mut self.mode {
            let was_open = live.ui.show_guild;
            live.ui.toggle_guild();
            opened = live.ui.show_guild && !was_open;
            if opened {
                actions = ui::UiState::guild_open_requests();
            }
        }
        for action in actions {
            self.dispatch_ui_action(action);
        }
        if opened {
            self.request_redraw();
        }
    }

    /// 好友面板：回车提交当前输入框（名字 → 加好友；备注 → 保存备注）。
    fn live_friend_enter(&mut self) {
        let Mode::Live(live) = &mut self.mode else {
            return;
        };
        match live.ui.focused_field() {
            Some(ui::FieldId::FriendName) => {
                let name = live.ui.friends.name.value.trim().to_owned();
                if name.is_empty() {
                    return;
                }
                let blocked = live.ui.friends.blocked;
                live.send(NetCommand::AddFriend { name, blocked });
                live.ui.friends.name.set("");
            }
            Some(ui::FieldId::FriendMemo) => {
                let Some(selected) = live.ui.friends.selected else {
                    return;
                };
                let Some(friend) = live.snapshot.friends.get(selected) else {
                    return;
                };
                let memo = live.ui.friends.memo.value.clone();
                live.send(NetCommand::AddMemo {
                    character_index: friend.index,
                    memo,
                });
            }
            _ => {}
        }
    }

    /// 开关好友面板（`F`）；打开时发一次 `C.RefreshFriends`(129)。
    fn live_toggle_friends(&mut self) {
        let mut refresh = false;
        if let Mode::Live(live) = &mut self.mode {
            let was_open = live.ui.show_friends;
            live.ui.toggle_friends();
            refresh = live.ui.show_friends && !was_open;
        }
        if refresh {
            self.dispatch_ui_action(ui::UiAction::RefreshFriends);
            self.request_redraw();
        }
    }

    /// 鼠标移动：记录位置；面板打开、悬停格子变化时重绘（更新悬停高亮与目标框）。
    fn handle_cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        // 窗口坐标 → 逻辑坐标（缩放 > 1 时 UI/世界格都按逻辑坐标算）。
        let scale = f64::from(self.ui_scale.max(1));
        let mouse = Point::new(
            (position.x / scale).round() as i32,
            (position.y / scale).round() as i32,
        );
        if let Mode::Login(login) = &mut self.mode {
            login.mouse = mouse;
            self.request_redraw();
            return;
        }
        let cell = self.scene.screen_to_cell(self.center, mouse);
        let mut redraw = false;
        if let Mode::Live(live) = &mut self.mode {
            live.ui.mouse = mouse;
            live.ui.set_time(now_millis());
            // 悬停高亮按格画（`ui::target`），格子变了才需要重绘。
            redraw = live.last_hover_cell != Some(cell)
                || live.ui.show_inventory
                || live.ui.show_skills
                || live.ui.show_equipment
                || live.ui.show_quests
                || live.ui.show_storage
                || live.ui.show_mail
                || live.ui.show_guild
                || live.ui.show_group
                || live.ui.show_trade
                || live.ui.show_bigmap
                || live.ui.show_settings
                || live.ui.show_market
                || live.ui.shop_open
                || live.ui.dialog.is_some()
                || live.ui.drag.is_some()
                || live.ui.context_menu.is_some();
            live.last_hover_cell = Some(cell);
        }
        if redraw {
            self.request_redraw();
        }
    }

    /// 鼠标左键按下：点 NPC/对话按钮/商店行/背包格/技能行/怪物/地面。
    fn handle_left_press(&mut self, event_loop: &ActiveEventLoop) {
        if matches!(&self.mode, Mode::Login(_)) {
            self.login_click(event_loop);
            return;
        }
        let frame = Rect::screen(self.frame.width, self.frame.height);
        let mut action = None;
        let mut cell = None;
        let mut door_target = None;
        if let Mode::Live(live) = &mut self.mode {
            if !live.snapshot.is_ready() {
                return;
            }
            live.ui.set_time(now_millis());
            let mouse = live.ui.mouse;
            let world_cell = self.scene.screen_to_cell(self.center, mouse);
            cell = Some(world_cell);
            door_target = mir_client::ui::door::click(&self.scene.map, world_cell);
            // 地图可走性闭包：A* 只在点到空地时使用（`UiState::on_click_with_path`）。
            let map = &self.scene.map;
            let walkable = |x: i32, y: i32| map.walkable(x, y);
            action = live.ui.on_click_with_path(
                &live.snapshot,
                &self.text,
                frame,
                Some(world_cell),
                Some(&walkable),
            );
        }
        // 门优先于“点地走路”：面板/菜单动作仍优先（只在无动作或 WorldMove 时开门）。
        if let Some(door) = door_target
            && matches!(action.as_ref(), None | Some(UiAction::WorldMove { .. }))
        {
            if let Mode::Live(live) = &mut self.mode {
                live.send(NetCommand::OpenDoor(door.door_index));
            }
            action = None;
        }
        if let Some(action) = action {
            // 双击目标产生的「靠近一步」不进按住连续走：它由 `UiState::tick_approach`
            // 按 600 ms 节拍推进，最多 3 步（需求）。A* 的 `WalkStep` 由 `tick_path`
            // 自己推进，也不进按住连续走。
            let walking = matches!(action, UiAction::WorldMove { .. }) && !self.live_approaching();
            self.dispatch_ui_action(action);
            if walking {
                self.start_walk_hold(cell);
            }
        } else {
            // 选中目标/关闭右键菜单等没有网络动作的点击也要重绘（目标准星）。
            self.request_redraw();
        }
    }

    /// 是否正在「靠近锁定目标」（由 `UiState::approach` 驱动）。
    fn live_approaching(&self) -> bool {
        matches!(&self.mode, Mode::Live(live) if live.ui.approach.is_some())
    }

    /// 鼠标左键释放：结束连续走并处理拖拽落点。
    fn handle_left_release(&mut self) {
        let frame = Rect::screen(self.frame.width, self.frame.height);
        let mut action = None;
        if let Mode::Live(live) = &mut self.mode {
            live.left_held = false;
            live.walk_target = None;
            action = live.ui.on_mouse_release(&live.snapshot, frame);
        }
        if let Some(action) = action {
            self.dispatch_ui_action(action);
        } else {
            self.request_redraw();
        }
    }

    /// 鼠标右键：背包物品弹出上下文菜单。
    fn handle_right_press(&mut self) {
        let frame = Rect::screen(self.frame.width, self.frame.height);
        let mut action = None;
        if let Mode::Live(live) = &mut self.mode {
            if !live.snapshot.is_ready() {
                return;
            }
            action = live.ui.on_right_click(&live.snapshot, frame);
        }
        if let Some(action) = action {
            self.dispatch_ui_action(action);
        } else {
            self.request_redraw();
        }
    }

    /// 鼠标滚轮：商店打开时滚动货架。
    fn handle_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => y,
            MouseScrollDelta::PixelDelta(position) => position.y as f32 / 40.0,
        };
        let step = i32::from(lines < 0.0) - i32::from(lines > 0.0);
        if step == 0 {
            return;
        }
        let mut changed = false;
        if let Mode::Live(live) = &mut self.mode {
            changed = live.ui.on_wheel(&live.snapshot, step);
        }
        if changed {
            self.request_redraw();
        }
    }

    /// 记录「按住左键连续走」的目标并在下一个节拍开始重复。
    fn start_walk_hold(&mut self, cell: Option<Point>) {
        if let Mode::Live(live) = &mut self.mode {
            live.left_held = true;
            live.walk_target = cell;
            live.next_walk = Instant::now() + WALK_REPEAT;
        }
    }

    /// 每帧检查是否该继续朝目标格走一步。
    fn repeat_walk(&mut self) {
        let Mode::Live(live) = &mut self.mode else {
            return;
        };
        if live.ui.walk_path.is_some() {
            // A* 路径优先；按住左键的连续走不要和它抢节拍。
            return;
        }
        if !live.left_held || Instant::now() < live.next_walk {
            return;
        }
        live.next_walk = Instant::now() + WALK_REPEAT;
        let Some(target) = live.walk_target else {
            return;
        };
        let Some(intent) = mir_client::ui::walk_intent(&live.snapshot, target) else {
            return;
        };
        if intent.step {
            live.send(NetCommand::Walk(intent.direction));
        } else {
            live.send(NetCommand::Turn(intent.direction));
        }
    }

    /// Esc：聊天输入 > UI 面板按「右键菜单 > 拖拽 > 大地图 > NPC 对话 > 背包/装备/技能」；
    /// 关对话时给服务端发 `@exit`。具体顺序由 [`UiState::escape`] 决定（可无头测试）。
    fn live_escape(&mut self) {
        let mut redraw = false;
        if let Mode::Live(live) = &mut self.mode {
            if live.chat_input.is_some() {
                live.chat_input = None;
                redraw = true;
            } else {
                let result = live.ui.escape();
                match result {
                    ui::EscapeResult::Npc { object_id } => {
                        live.send(NetCommand::CallNpc {
                            object_id,
                            key: "@exit".to_owned(),
                        });
                    }
                    // 关交易窗 = 取消交易（Crystal `CloseButton.Click → TradeCancel()`）。
                    ui::EscapeResult::Trade => live.send(NetCommand::TradeCancel),
                    _ => {}
                }
                redraw = result.closed();
            }
        }
        if redraw {
            self.request_redraw();
        }
    }

    /// 从网络线程取事件，更新快照与相机。
    /// 从网络线程取事件，**按到达顺序**更新快照与相机；切图事件在地形重载后重置相机。
    ///
    /// 事件顺序由网络线程保证：先 [`NetEvent::Snapshot`]（快照已按 `S.MapChanged` 清空实体、
    /// 更新坐标与地图号），再 [`NetEvent::MapChanged`]（触发本地地形重载）。
    fn poll_network(&mut self) {
        let now = now_millis_u64();
        let mut events = Vec::new();
        if let Mode::Live(live) = &mut self.mode {
            while let Some(event) = live.net.try_recv() {
                events.push(event);
            }
        }
        let mut closed = false;
        let mut ready = false;
        for event in events {
            match event {
                NetEvent::Snapshot(snapshot) => {
                    ready |= snapshot.is_ready();
                    self.apply_snapshot(*snapshot);
                }
                NetEvent::MapChanged {
                    map_index,
                    position,
                } => self.apply_map_changed(map_index, position),
                NetEvent::Error(message) => eprintln!("{message}"),
                NetEvent::Closed(reason) => {
                    eprintln!("连接关闭：{reason}");
                    closed = true;
                }
            }
        }
        let Mode::Live(live) = &mut self.mode else {
            return;
        };
        if ready && live.reconnect.is_reconnecting() {
            live.reconnect.succeeded();
        }
        if closed {
            // 断线：当前走/跑插值作废，重连成功后从新快照重新起算。
            live.motion = None;
            live.reconnect.disconnected(now);
        }
        // 退避到点：发起一次重连（复用账号/角色，自动登录并选角）。
        if let Some(attempt) = live.reconnect.due(now) {
            match NetClient::connect(live.connect.clone()) {
                Ok(client) => {
                    eprintln!("第 {attempt} 次重连：已发起");
                    live.net = client;
                    live.entered = false;
                    // 重连会重新落点/可能换图：清掉旧动作的插值，避免残留偏移。
                    live.motion = None;
                }
                Err(error) => {
                    eprintln!("第 {attempt} 次重连失败：{error}");
                    live.reconnect.attempt_failed(now);
                }
            }
        }
        // 3 次失败：回登录界面（音效播放器随状态带走，模式切换后重新初始化）。
        if live.reconnect.failed() {
            let language = live.ui.language;
            let account = live.connect.account.clone();
            let password = live.connect.password.clone();
            let sound = std::mem::replace(&mut live.sound, Self::silent_sound());
            self.mode = Mode::Login(Box::new(LoginState::new(
                sound,
                &self.settings,
                language,
                &account,
                &password,
            )));
        }
    }

    /// 重连失败回登录界面时占位的静音播放器（不打开音频设备）。
    fn silent_sound() -> SoundPlayer {
        SoundPlayer::new(SoundLibrary::with_roots(Vec::new()), false, 0.0)
    }

    /// 应用一份世界快照：更新状态、聊天日志与相机。
    fn apply_snapshot(&mut self, snapshot: WorldSnapshot) {
        let position = snapshot.self_player.as_ref().map(|player| player.position);
        let status = snapshot.status;
        let ready = snapshot.is_ready();
        let language = match &self.mode {
            Mode::Live(live) => live.ui.language,
            _ => self.settings.language(),
        };
        // 聊天日志：服务端文本原样显示，客户端合成的系统消息按玩家语言渲染。
        let new_chat = snapshot
            .chat
            .last()
            .map(|line| mir_client::ui::labels::chat_line_text(line, language));
        let now = now_millis_u64();
        let mut market_actions = Vec::new();
        if let Mode::Live(live) = &mut self.mode {
            if live.last_status != status {
                println!("网络状态：{}", status.label());
                live.last_status = status;
            }
            let old_market_open = live.snapshot.market.open_version;
            let old_self_pos = live
                .snapshot
                .self_player
                .as_ref()
                .map(|player| player.position);
            live.snapshot = snapshot;
            if let (Some(old), Some(new)) = (
                old_self_pos,
                live.snapshot
                    .self_player
                    .as_ref()
                    .map(|player| player.position),
            ) {
                live.update_self_motion(old, new);
            }
            // 死亡/复活等会打断动作：清掉插值，避免尸体或复活瞬间残留偏移。
            if live.snapshot.self_player.as_ref().is_some_and(|me| me.dead) {
                live.motion = None;
            }
            live.ui.sync(&live.snapshot);
            // 市场窗口刚打开：按当前页签补一次 `C.MarketSearch`（服务端打开时只发
            // 第一页，`GetMarket` 的 Min/MaxShapes 还是 0；Crystal `TMerchantDialog(type)`
            // 打开页签时也会发一次搜索，`TrustMerchantDialog.cs:1160-1166`）。
            // `S.NPCConsign` 的 NPC 页是 `[@CONSIGN]`，服务端不接受搜索；此时不发，
            // 等玩家切页签时由 `ui::market` 发 `@MARKET` 换页。
            if live.snapshot.market.open_version != old_market_open {
                market_actions = live.ui.market_open_requests();
            }
            // 撞墙/移动被拒：服务端回同坐标 UserLocation，本地提示（零发包）。
            let bump = live.snapshot.wall_bump_id;
            if bump != live.last_wall_bump_id {
                live.last_wall_bump_id = bump;
                live.ui.wall_bump.inject(now);
            }
            // 别人身上的动作（走/跑/攻击/受击/死亡/升级…）与服务器通知的音效都在这里播。
            live.play_pending_sounds(now);
            if let Some(line) = new_chat {
                if live.last_chat.as_ref() != Some(&line) {
                    println!("{line}");
                    live.last_chat = Some(line);
                }
            }
            if ready && !live.entered {
                live.entered = true;
                if let Some(me) = &live.snapshot.self_player {
                    println!(
                        "已进入世界：{} · 坐标 ({}, {}) · HP {} · MP {}",
                        me.name, me.position.x, me.position.y, me.hp, me.mp
                    );
                }
            }
        }
        for action in market_actions {
            self.dispatch_ui_action(action);
        }
        if let Some(position) = position {
            self.center = self.clamp_center(position);
            self.request_redraw();
        }
    }

    /// 切图：按快照里的地图文件名重载本地地形，并把相机重置到落点。
    ///
    /// 对应 Crystal 客户端 `GameScene.MapChanged`（`Client/MirScenes/GameScene.cs:3970-4009`）：
    /// 地图编号变了就 `MapControl.LoadMap()`，然后 `User.CurrentLocation = p.Location`、
    /// `MapControl.InputDelay = CMain.Time + 400`（本作没有输入延迟缓冲）。
    /// 缺文件时**保留旧地图**并打印警告（黑屏占位比旧图更糟，见需求）。
    fn apply_map_changed(&mut self, map_index: i32, position: Point) {
        let map_dir = self.map_dir.clone();
        let snapshot = match &self.mode {
            Mode::Live(live) => live.snapshot.clone(),
            Mode::Offline | Mode::Login(_) => return,
        };
        match reload_after_map_change(&mut self.scene, &snapshot, &map_dir, map_index) {
            Ok(request) => {
                println!(
                    "切换地图：{}（编号 {}）· 地形 {}×{} · 落点 ({}, {})",
                    request.file_name,
                    request.map_index,
                    self.scene.map.width,
                    self.scene.map.height,
                    position.x,
                    position.y
                );
                self.center = self.clamp_center(position);
                // 切图事件：SelectMusic（SoundList.cs:38，10147 → Select2.wav）。
                if let Mode::Live(live) = &mut self.mode {
                    // 跨图落点不插值：清掉上一张图的走/跑动作，避免残留偏移。
                    live.motion = None;
                    live.sound
                        .play(mir_client::audio::MAP_CHANGE_SOUND_FILE, now_millis_u64());
                }
                self.request_redraw();
            }
            Err(error) => eprintln!("警告：{error}"),
        }
    }

    /// 处理按键；返回是否请求了退出。
    fn handle_key(&mut self, event_loop: &ActiveEventLoop, event: &KeyEvent) {
        tracing::debug!(
            key = ?event.logical_key,
            state = ?event.state,
            repeat = event.repeat,
            "键盘事件"
        );
        if event.state != ElementState::Pressed {
            return;
        }
        if self.chat_active() {
            self.handle_chat_key(event);
            return;
        }
        if matches!(&self.mode, Mode::Login(_)) {
            self.handle_login_key(event_loop, event);
            return;
        }
        if matches!(&self.mode, Mode::Live(_)) {
            self.handle_live_key(event_loop, event);
            return;
        }
        match &event.logical_key {
            Key::Named(NamedKey::ArrowLeft) => self.pan(-1, 0),
            Key::Named(NamedKey::ArrowRight) => self.pan(1, 0),
            Key::Named(NamedKey::ArrowUp) => self.pan(0, -1),
            Key::Named(NamedKey::ArrowDown) => self.pan(0, 1),
            Key::Named(NamedKey::F1) => {
                self.show_overlay = !self.show_overlay;
                self.request_redraw();
            }
            Key::Named(NamedKey::F2) => self.save_screenshot(),
            Key::Named(NamedKey::Escape) => event_loop.exit(),
            Key::Character(text) => match text.to_ascii_lowercase().as_str() {
                // 相机平移：A/D 左右、W 上；下用方向键（S 被截图占用）。
                "a" => self.pan(-1, 0),
                "d" => self.pan(1, 0),
                "w" => self.pan(0, -1),
                "q" => event_loop.exit(),
                "s" => self.save_screenshot(),
                _ => {}
            },
            _ => {}
        }
    }

    /// 联机模式的按键。
    ///
    /// 面板按键优先于移动键：`I` 背包、`E` 装备、`F3` 技能；`D` 在背包打开且有选中
    /// 物品时是「丢弃」，否则是向右走；`S` 在商店打开且有选中物品时是「出售」，
    /// 否则是向下走；商店打开时 ↑/↓ 滚动货架。
    fn handle_live_key(&mut self, event_loop: &ActiveEventLoop, event: &KeyEvent) {
        // 设置面板的键位捕获：下一次按键（非修饰键）直接绑定，冲突则提示并保留原键。
        if let Mode::Live(live) = &mut self.mode
            && live.ui.show_settings
            && let Some(index) = live.ui.settings.rebinding
        {
            if let Some(name) = key_binding_name(&event.logical_key) {
                let action = mir_client::client_config::KEY_ACTIONS[index];
                match live.ui.settings.keys.set(action, &name) {
                    Ok(()) => {
                        live.ui.settings.rebind_error = None;
                        self.client_config.keys = live.ui.settings.keys.clone();
                        if let Err(error) = self
                            .client_config
                            .save(mir_client::client_config::default_path())
                        {
                            eprintln!("保存 config/client.toml 失败：{error}");
                        }
                    }
                    Err(conflict) => {
                        live.ui.settings.rebind_error =
                            Some(format!("键位冲突：{name} 已被 {conflict} 使用"));
                    }
                }
                live.ui.settings.rebinding = None;
            }
            self.request_redraw();
            return;
        }
        // 表单输入优先：写信页/行会公告/行会资金聚焦时，字符键只进输入框。
        let mut text_handled = false;
        let mut text_consumed_key = true;
        let mut market_search_enter = false;
        let mut friend_enter = false;
        if let Mode::Live(live) = &mut self.mode {
            if live.ui.focused_field().is_some() {
                text_handled = true;
                match &event.logical_key {
                    Key::Character(text) => {
                        for character in text.chars() {
                            live.ui.type_char(character);
                        }
                    }
                    Key::Named(NamedKey::Backspace) => {
                        live.ui.backspace();
                    }
                    Key::Named(NamedKey::Escape) => {
                        live.ui.clear_focus();
                    }
                    // 回车/退格之外的命名键不吞（F2 截图、F10 退出仍可用）。
                    Key::Named(NamedKey::Enter) => {
                        // 市场搜索框里按回车 = 点「搜索」（Crystal `SearchTextBox_KeyPress`，
                        // `TrustMerchantDialog.cs:1044-1056`）。
                        market_search_enter =
                            live.ui.focused_field() == Some(ui::FieldId::MarketSearch);
                        friend_enter = matches!(
                            live.ui.focused_field(),
                            Some(ui::FieldId::FriendName | ui::FieldId::FriendMemo)
                        );
                    }
                    _ => text_consumed_key = false,
                }
            }
        }
        if text_handled && text_consumed_key {
            if market_search_enter {
                self.live_market_search();
            }
            if friend_enter {
                self.live_friend_enter();
            }
            self.request_redraw();
            return;
        }
        if let Key::Character(text) = &event.logical_key {
            let lower = text.to_ascii_lowercase();
            match lower.as_str() {
                "i" => {
                    self.live_toggle_inventory();
                    return;
                }
                "e" => {
                    self.live_toggle_equipment();
                    return;
                }
                "m" => {
                    self.live_toggle_minimap();
                    return;
                }
                "b" => {
                    self.live_toggle_bigmap();
                    return;
                }
                // `P` 在 Crystal 是组队窗口（`KeybindSettings.cs:225` 的 `KeybindOptions.Group`），
                // 拾取因此挪到 `Tab`（Crystal 的 `KeybindOptions.Pickup = Keys.Tab`，`:334`）。
                "p" => {
                    self.live_toggle_group();
                    return;
                }
                // `Y`：发起交易。Crystal 的交易键是 `T`（`:340`），本作 `T` 已用于聊天。
                "y" => {
                    self.live_request_trade();
                    return;
                }
                // `T` 打开聊天；任务面板按 Crystal 的默认键 `Q`（`KeyBindSettings.cs:219`），
                // 退出改到 F10（原先是 `Q`）。
                "t" => {
                    self.live_open_chat();
                    return;
                }
                "q" => {
                    self.live_toggle_quests();
                    return;
                }
                "n" => {
                    self.live_toggle_mail();
                    return;
                }
                "g" => {
                    self.live_toggle_guild();
                    return;
                }
                // `F`：好友/黑名单面板（本作扩展键；Crystal 没有好友 UI）。
                "f" => {
                    self.live_toggle_friends();
                    return;
                }
                // `O`：设置面板（Crystal `KeybindOptions.Options2` 的默认键，
                // `Client/KeyBindSettings.cs:223-224`；`F12` 是同组的另一个默认键）。
                "o" => {
                    self.open_settings();
                    return;
                }
                // `V`：宠物模式三态循环（`C.ChangePMode`）。`P` 是 Crystal 的组队窗口，
                // 不能占用；`V` 在 Crystal 里没有默认绑定，作为本作扩展键。
                "v" => {
                    self.live_cycle_pet_mode();
                    return;
                }
                _ => {}
            }
            if lower == "d" && self.live_inventory_open() && self.live_drop_selected() {
                return;
            }
            if lower == "s" && self.live_shop_open() && self.live_sell_selected() {
                return;
            }
            if lower == "r" && self.live_shop_open() && self.live_repair_selected() {
                return;
            }
            if let Some(key) = digit_key(&lower) {
                if self.live_cast_key(key) {
                    return;
                }
            }
        }
        // 商店打开时，上下键滚动货架而不是走路。
        if self.live_shop_open() {
            let delta = match &event.logical_key {
                Key::Named(NamedKey::ArrowUp) => Some(-1),
                Key::Named(NamedKey::ArrowDown) => Some(1),
                _ => None,
            };
            if let Some(delta) = delta {
                let mut changed = false;
                if let Mode::Live(live) = &mut self.mode {
                    changed = live.ui.scroll_shop(&live.snapshot, delta);
                }
                if changed {
                    self.request_redraw();
                }
                return;
            }
        }
        if let Some(direction) = direction_from_key(&event.logical_key) {
            self.live_move(direction);
            return;
        }
        match &event.logical_key {
            Key::Named(NamedKey::Space) => self.live_attack(),
            // Crystal 的拾取键就是 `Tab`（`KeybindSettings.cs:334`）；`P` 让给组队窗口。
            Key::Named(NamedKey::Tab) => self.live_pickup(),
            Key::Named(NamedKey::F1) => {
                self.show_overlay = !self.show_overlay;
                self.request_redraw();
            }
            Key::Named(NamedKey::F2) => self.save_screenshot(),
            Key::Named(NamedKey::F3) => self.live_toggle_skills(),
            // 退出从 `Q` 挪到 `F10`：`Q` 现在是任务面板（Crystal 的默认键）。
            Key::Named(NamedKey::F10) => event_loop.exit(),
            // F12：打开/关闭统一设置面板（与 O 等价，便于文档/快捷键发现）。
            Key::Named(NamedKey::F12) => self.open_settings(),
            Key::Named(NamedKey::Escape) => self.live_escape(),
            _ => {}
        }
    }
}

/// 要使用的物品是不是药（决定要不要播喝药音）。
///
/// 走「唯一编号 → 背包格 → 模板」这条链：Crystal 在 `MirItemCell` 里能直接拿到 `Item.Info`，
/// 我们只有快照，所以要绕一圈（`ItemInfoView.item_type`，`ItemType::Potion` 见
/// `mir-core` 的 `ItemType` 枚举，Crystal `Shared/ItemInfo.cs` 的 `ItemType.Potion`）。
fn used_item_is_potion(snapshot: &WorldSnapshot, unique_id: u64) -> bool {
    let Some(slot) = snapshot.inventory_slot_of(unique_id) else {
        return false;
    };
    let Some(item) = snapshot.inventory_item(slot) else {
        return false;
    };
    snapshot
        .item_info(item.item_index)
        .is_some_and(|info| info.item_type == ItemType::Potion)
}

/// 单个数字键 `1..=9`（技能快捷键）。
fn digit_key(text: &str) -> Option<u8> {
    let bytes = text.as_bytes();
    match bytes {
        [digit @ b'1'..=b'9'] => Some(digit - b'0'),
        _ => None,
    }
}

/// 把方向键 / WASD 映射成移动方向。
fn direction_from_key(key: &Key) -> Option<MirDirection> {
    match key {
        Key::Named(NamedKey::ArrowUp) => Some(MirDirection::Up),
        Key::Named(NamedKey::ArrowRight) => Some(MirDirection::Right),
        Key::Named(NamedKey::ArrowDown) => Some(MirDirection::Down),
        Key::Named(NamedKey::ArrowLeft) => Some(MirDirection::Left),
        Key::Character(text) => match text.to_ascii_lowercase().as_str() {
            "w" => Some(MirDirection::Up),
            "d" => Some(MirDirection::Right),
            "s" => Some(MirDirection::Down),
            "a" => Some(MirDirection::Left),
            _ => None,
        },
        _ => None,
    }
}

/// 切图后重载地形，返回发起的重载请求（失败时把原因作为 `Err`，旧地图保持不变）。
///
/// 提成自由函数是为了能在不创建窗口的情况下驱动「注入 `S.MapChanged` → 记录重载请求」：
/// 单元测试直接给它一份 [`WorldSnapshot`] 与临时地图目录即可。
fn reload_after_map_change(
    scene: &mut MapScene,
    snapshot: &WorldSnapshot,
    map_dir: &Path,
    map_index: i32,
) -> Result<MapReloadRequest, String> {
    let request = snapshot.map_reload_request(map_dir).ok_or_else(|| {
        format!("收到切图（地图编号 {map_index}）但快照里没有地图信息，保留当前地形")
    })?;
    scene.reload_map(&request.path).map_err(|error| {
        format!(
            "加载地图 {} 失败：{error}；保留当前地形（{}）",
            request.path.display(),
            scene.map.file_name
        )
    })?;
    Ok(request)
}

/// 把 RGBA8 帧缓冲送进 softbuffer 并呈现。
///
/// `scale > 1` 时逻辑帧比窗口小：按最近邻放大（`scale = 2` 时每个逻辑像素画成 2×2）。
fn present(
    surface: &mut softbuffer::Surface<Arc<Window>, Arc<Window>>,
    frame: &Frame,
    width: u32,
    height: u32,
    scale: u32,
) {
    let (Some(width), Some(height)) = (NonZeroU32::new(width), NonZeroU32::new(height)) else {
        return;
    };
    if let Err(error) = surface.resize(width, height) {
        eprintln!("调整渲染表面失败：{error}");
        return;
    }
    let mut buffer = match surface.buffer_mut() {
        Ok(buffer) => buffer,
        Err(error) => {
            eprintln!("获取渲染缓冲失败：{error}");
            return;
        }
    };
    let scale = scale.max(1);
    if scale == 1 && buffer.len() == frame.pixels.len() / 4 {
        // softbuffer 的像素格式是 0x00RRGGBB（无 alpha）。
        for (destination, source) in buffer.iter_mut().zip(frame.pixels.chunks_exact(4)) {
            *destination =
                (u32::from(source[0]) << 16) | (u32::from(source[1]) << 8) | u32::from(source[2]);
        }
    } else {
        // 放大：目标像素取对应逻辑像素（最近邻；retro 风格也最省 CPU）。
        let logical_width = frame.width.max(1);
        let logical_height = frame.height.max(1);
        let physical_width = usize::try_from(width.get()).unwrap_or(0);
        for (index, destination) in buffer.iter_mut().enumerate() {
            let x = index % physical_width;
            let y = index / physical_width;
            let sx = (x / usize::try_from(scale).unwrap_or(1))
                .min(usize::try_from(logical_width - 1).unwrap_or(0));
            let sy = (y / usize::try_from(scale).unwrap_or(1))
                .min(usize::try_from(logical_height - 1).unwrap_or(0));
            let offset = (sy * usize::try_from(logical_width).unwrap_or(0) + sx) * 4;
            let source = &frame.pixels[offset..offset + 4];
            *destination =
                (u32::from(source[0]) << 16) | (u32::from(source[1]) << 8) | u32::from(source[2]);
        }
    }
    if let Err(error) = buffer.present() {
        eprintln!("呈现画面失败：{error}");
    }
}

impl ApplicationHandler for ClientApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.create_window(event_loop);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    let scale = self.ui_scale.max(1);
                    self.frame =
                        Frame::new((size.width / scale).max(1), (size.height / scale).max(1));
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                if let Mode::Live(live) = &mut self.mode {
                    live.shift = modifiers.state().shift_key();
                }
            }
            WindowEvent::CursorMoved { position, .. } => self.handle_cursor_moved(position),
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, MouseButton::Left) => self.handle_left_press(event_loop),
                (ElementState::Released, MouseButton::Left) => self.handle_left_release(),
                (ElementState::Pressed, MouseButton::Right) => self.handle_right_press(),
                _ => {}
            },
            WindowEvent::MouseWheel { delta, .. } => self.handle_mouse_wheel(delta),
            WindowEvent::KeyboardInput { event, .. } => self.handle_key(event_loop, &event),
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.maybe_auto_connect_login();
        self.check_login_watchdog();
        self.poll_login();
        self.poll_network();
        self.tick_targeting();
        self.tick_path();
        self.sync_music();
        self.repeat_walk();
        let now = Instant::now();
        if now >= self.next_animation {
            self.next_animation = now + ANIMATION_INTERVAL;
            self.scene
                .set_animation_frame(self.scene.animation_frame().wrapping_add(1));
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_animation));
    }
}

/// 把 winit 按键转成配置里的键名（设置面板捕获用；修饰键返回 `None`）。
fn key_binding_name(key: &Key) -> Option<String> {
    match key {
        Key::Character(text) => {
            let upper = text.to_uppercase();
            if upper.chars().count() == 1 {
                Some(upper)
            } else {
                None
            }
        }
        Key::Named(NamedKey::Space) => Some("Space".to_owned()),
        Key::Named(NamedKey::Enter) => Some("Enter".to_owned()),
        Key::Named(NamedKey::Tab) => Some("Tab".to_owned()),
        Key::Named(NamedKey::Escape) => Some("Escape".to_owned()),
        Key::Named(NamedKey::ArrowUp) => Some("Up".to_owned()),
        Key::Named(NamedKey::ArrowDown) => Some("Down".to_owned()),
        Key::Named(NamedKey::ArrowLeft) => Some("Left".to_owned()),
        Key::Named(NamedKey::ArrowRight) => Some("Right".to_owned()),
        Key::Named(NamedKey::F1) => Some("F1".to_owned()),
        Key::Named(NamedKey::F2) => Some("F2".to_owned()),
        Key::Named(NamedKey::F3) => Some("F3".to_owned()),
        Key::Named(NamedKey::F4) => Some("F4".to_owned()),
        Key::Named(NamedKey::F5) => Some("F5".to_owned()),
        Key::Named(NamedKey::F6) => Some("F6".to_owned()),
        Key::Named(NamedKey::F7) => Some("F7".to_owned()),
        Key::Named(NamedKey::F8) => Some("F8".to_owned()),
        Key::Named(NamedKey::F9) => Some("F9".to_owned()),
        Key::Named(NamedKey::F10) => Some("F10".to_owned()),
        Key::Named(NamedKey::F11) => Some("F11".to_owned()),
        Key::Named(NamedKey::F12) => Some("F12".to_owned()),
        _ => None,
    }
}

/// 当前 Unix 毫秒（无符号；动作帧时间戳用）。
fn now_millis_u64() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

/// 当前 Unix 毫秒（伤害飘字/增益倒计时用）。
fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}

/// 找到下一个可用的截图路径：`build/screenshot-<n>.png`，`n` 从 1 递增。
fn next_screenshot_path() -> PathBuf {
    let directory = PathBuf::from("build");
    let mut highest = 0u32;
    if let Ok(entries) = std::fs::read_dir(&directory) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Some(rest) = name
                .strip_prefix("screenshot-")
                .and_then(|rest| rest.strip_suffix(".png"))
            else {
                continue;
            };
            if let Ok(number) = rest.parse::<u32>() {
                highest = highest.max(number);
            }
        }
    }
    directory.join(format!("screenshot-{}.png", highest.saturating_add(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshots_are_numbered_sequentially() {
        let path = next_screenshot_path();
        let name = path.file_name().expect("文件名").to_string_lossy();
        assert!(name.starts_with("screenshot-"));
        assert!(name.ends_with(".png"));
    }

    #[test]
    fn chat_submit_keeps_gm_and_group_prefixes() {
        // `@` 是 GM/脚本命令：原样走 `C.Chat`，客户端不解析、不本地化、不泄漏命令列表。
        assert_eq!(
            chat_submit_command("@help"),
            Some(NetCommand::Chat("@help".to_owned()))
        );
        assert_eq!(
            chat_submit_command("@language en"),
            Some(NetCommand::Chat("@language en".to_owned()))
        );
        // `!!` 组队前缀也只是 `C.Chat`（服务端按前缀转频道）。
        assert_eq!(
            chat_submit_command("!!集合"),
            Some(NetCommand::Chat("!!集合".to_owned()))
        );
        assert_eq!(
            chat_submit_command("你好"),
            Some(NetCommand::Chat("你好".to_owned()))
        );
        assert_eq!(chat_submit_command("   "), None);
    }

    /// 新规则：运行期只读自有格式（`game-assets/`），不再读 `resource/data`。
    #[test]
    fn blank_scene_loads_dev_tree_libraries_when_game_assets_exist() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-assets");
        let data_dir = root.join("data");
        if !root.join("library-map.json").is_file() {
            eprintln!("跳过：缺 {}", root.join("library-map.json").display());
            return;
        }
        let scene = blank_scene(&data_dir);
        assert!(
            scene.libraries().len() >= 3,
            "登录背景应当从 game-assets 加载自有图库：{}",
            scene.libraries().len()
        );
        // 完全没有素材根时退回空图库（不 panic）。
        let empty_root = std::env::temp_dir().join("mir2-client-no-assets");
        std::fs::create_dir_all(&empty_root).expect("mkdir");
        let empty = blank_scene(&empty_root.join("data"));
        assert!(empty.libraries().is_empty());
        std::fs::remove_dir_all(&empty_root).ok();
    }

    #[test]
    fn direction_keys_cover_wasd_and_arrows() {
        assert_eq!(
            direction_from_key(&Key::Character("W".into())),
            Some(MirDirection::Up)
        );
        assert_eq!(
            direction_from_key(&Key::Named(NamedKey::ArrowDown)),
            Some(MirDirection::Down)
        );
        assert_eq!(direction_from_key(&Key::Character("x".into())), None);
    }

    #[test]
    fn digit_keys_map_to_skill_slots_only_for_one_to_nine() {
        assert_eq!(digit_key("1"), Some(1));
        assert_eq!(digit_key("9"), Some(9));
        assert_eq!(digit_key("0"), None);
        assert_eq!(digit_key("12"), None);
        assert_eq!(digit_key("a"), None);
    }

    /// 注入一条 `S.MapChanged`，验证「重载请求被记录、旧地图在缺文件时保留」。
    ///
    /// 不创建窗口：`reload_after_map_change` 只碰 `MapScene` 与快照。
    #[test]
    fn map_changed_records_a_reload_request_and_keeps_the_old_map_when_the_file_is_missing() {
        use mir_protocol::ServerPacket;
        use mir_protocol::packets::server::MapChanged;

        let mut snapshot = WorldSnapshot::default();
        snapshot.apply_packet(&ServerPacket::MapChanged(MapChanged {
            map_index: 7,
            file_name: "D2083".to_owned(),
            location: Point::new(3, 4),
            direction: MirDirection::Down,
            ..MapChanged::default()
        }));
        let mut scene = MapScene::from_parts(
            crate::blank_map(),
            mir_render::TileLibraries::empty("."),
            mir_render::Viewport::DEFAULT,
        );
        let before = scene.map.file_name.clone();

        let error =
            reload_after_map_change(&mut scene, &snapshot, Path::new("resource/不存在的目录"), 7)
                .expect_err("缺文件时应返回错误");
        assert!(error.contains("D2083"), "错误里应带文件名：{error}");
        assert_eq!(scene.map.file_name, before, "缺文件时必须保留旧地图");

        // 请求本身仍在快照里可查（UI/日志用它记录「要重载哪张图」）。
        let request = snapshot
            .map_reload_request("resource/map")
            .expect("切图后应能算出重载请求");
        assert_eq!(request.map_index, 7);
        assert_eq!(request.file_name, "D2083");
        assert_eq!(request.path, PathBuf::from("resource/map/D2083.map"));
    }

    /// 有地图文件时真的把地形换掉（拷一张真实地图到临时目录，素材缺失就跳过）。
    #[test]
    fn map_changed_reloads_the_terrain_when_the_file_exists() {
        use mir_protocol::ServerPacket;
        use mir_protocol::packets::server::MapChanged;

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = root.join("resource/map/0100.map");
        if !source.is_file() {
            eprintln!("跳过：{} 不存在", source.display());
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "mir-next-reload-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建临时目录");
        std::fs::copy(&source, dir.join("D2083.map")).expect("复制地图");

        let mut snapshot = WorldSnapshot::default();
        snapshot.apply_packet(&ServerPacket::MapChanged(MapChanged {
            map_index: 7,
            file_name: "D2083".to_owned(),
            location: Point::new(5, 6),
            ..MapChanged::default()
        }));
        let mut scene = MapScene::from_parts(
            crate::blank_map(),
            mir_render::TileLibraries::empty("."),
            mir_render::Viewport::DEFAULT,
        );
        let request =
            reload_after_map_change(&mut scene, &snapshot, &dir, 7).expect("有文件时应重载成功");
        assert_eq!(request.file_name, "D2083");
        assert_eq!(scene.map.file_name, "D2083", "地形已换成新地图");
        assert!(scene.map.width > 0 && scene.map.height > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
