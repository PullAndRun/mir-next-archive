//! winit 窗口 + softbuffer 软件呈现。
//!
//! 渲染本身全部在 `mir-render` 里完成（纯 headless），界面在 `mir_client::ui` 里；
//! 这里只负责：
//!
//! * 创建 800×600 的窗口，失败时打印可读的原因并以成功状态退出（无显示器的 CI）；
//! * 把 [`Frame`] 的 RGBA8 转成 softbuffer 需要的 `0x00RRGGBB`；
//! * 离线模式（`--map`）：方向键 / WASD 平移相机、`S` 截图、`F1` 调试叠层、`Q` 退出；
//! * 联机模式（`--connect`）：方向键/WASD 移动、`Space` 攻击、`P` 捡取、`T` 聊天、
//!   `I` 背包、`F3` 技能、`1..9` 施法、鼠标点 NPC/面板按钮、`Esc` 关面板、
//!   `F1` 叠层、`F2` 截图、`Q` 退出；
//! * 每 100 ms 递增一次动画帧（对应 Crystal 的 `MapControl.AnimationCount++`，
//!   `GameScene.cs:1188-1193`）。
//!
//! 网络线程由 [`NetClient`] 管理；窗口创建失败时 `ClientApp` 被丢弃，
//! `NetClient` 的 `Drop` 会发送 `Quit` 并回收线程，不会留下半开连接。

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mir_client::geom::Rect;
use mir_client::net::{
    MapReloadRequest, NetClient, NetCommand, NetEvent, NetStatus, WorldSnapshot,
};
use mir_client::ui::{self, UiAction, UiState, UiText};
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::MirDirection;
use mir_render::{Frame, MapScene};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::overlay;

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
}

/// 联机状态：网络句柄 + 最近一次快照 + 本地 UI 状态。
struct LiveState {
    /// 网络线程句柄。
    net: NetClient,
    /// 最近一次世界快照。
    snapshot: WorldSnapshot,
    /// 界面状态（面板开关、选中项、鼠标）。
    ui: UiState,
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
}

impl LiveState {
    /// 发命令；网络线程已退出时打印一次可读原因。
    fn send(&mut self, command: NetCommand) {
        if !self.net.send(command) {
            eprintln!("网络线程已退出，命令被丢弃");
        }
    }
}

/// 启动离线地图窗口。
///
/// 没有图形环境（`EventLoop::new` 失败、窗口/表面创建失败）时打印原因并返回，
/// 不 panic、不返回非零退出码。离线模式只有 F1 点阵叠层，`language` 暂不影响画面。
pub fn run(scene: MapScene, center: Point, _language: Language) {
    // 离线模式不会切图，地图目录只是占位（`ClientApp` 要求有这个字段）。
    run_app(scene, center, Mode::Offline, PathBuf::from("resource/map"));
}

/// 启动联机窗口。
///
/// `snapshot` 是进入世界后的首个快照（`wait_until_in_game` 的返回值），
/// `center` 通常是自己的坐标；`map_dir` 是 `resource/map` 一类的地图目录，
/// 切图（`S.MapChanged`）时用它拼出 `<map_dir>/<file>.map` 重新加载地形。
/// 窗口创建失败时连接会被 `Drop` 自动关闭。
pub fn run_live(
    scene: MapScene,
    center: Point,
    net: NetClient,
    snapshot: WorldSnapshot,
    language: Language,
    map_dir: PathBuf,
) {
    let status = snapshot.status;
    let mode = Mode::Live(Box::new(LiveState {
        net,
        ui: UiState::new(language),
        snapshot,
        chat_input: None,
        shift: false,
        last_chat: None,
        last_status: status,
        entered: false,
        left_held: false,
        walk_target: None,
        next_walk: Instant::now(),
    }));
    run_app(scene, center, mode, map_dir);
}

/// 公共窗口事件循环。
fn run_app(scene: MapScene, center: Point, mode: Mode, map_dir: PathBuf) {
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("无法初始化窗口系统（可能没有图形环境）：{error}");
            eprintln!("渲染器仍可用：`cargo test -p mir-render` 会把地图渲染到 target/。");
            return;
        }
    };
    let mut app = ClientApp::new(scene, center, mode, map_dir);
    if let Err(error) = event_loop.run_app(&mut app) {
        eprintln!("窗口事件循环异常退出：{error}");
    }
}

/// 客户端窗口状态。
struct ClientApp {
    scene: MapScene,
    /// 相机中心格。
    center: Point,
    /// 当前帧缓冲（尺寸跟随窗口）。
    frame: Frame,
    /// 运行模式。
    mode: Mode,
    /// 界面文字（TrueType 或点阵兜底）。
    text: UiText,
    /// 地图目录：切图时按 `<map_dir>/<file>.map` 重载地形。
    map_dir: PathBuf,
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
}

impl ClientApp {
    fn new(scene: MapScene, center: Point, mode: Mode, map_dir: PathBuf) -> Self {
        Self {
            scene,
            center,
            frame: Frame::new(WINDOW_WIDTH, WINDOW_HEIGHT),
            mode,
            text: UiText::load(None),
            map_dir,
            window: None,
            surface: None,
            context: None,
            show_overlay: false,
            next_animation: Instant::now() + ANIMATION_INTERVAL,
        }
    }

    /// 创建窗口与软件渲染表面。
    fn create_window(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let title = if matches!(&self.mode, Mode::Live(_)) {
            "传奇-Next · 联机"
        } else {
            "传奇-Next · 地图"
        };
        let attributes = Window::default_attributes()
            .with_title(title)
            .with_inner_size(winit::dpi::LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
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
    fn render_current(&mut self) {
        let entities = match &self.mode {
            Mode::Offline => None,
            Mode::Live(live) => Some(live.snapshot.entity_views()),
        };
        match entities {
            Some(entities) => {
                self.scene
                    .render_with_entities(&mut self.frame, self.center, &entities);
            }
            None => {
                self.scene.render(&mut self.frame, self.center);
            }
        }
    }

    /// 画调试叠层。
    fn draw_overlay(&mut self) {
        match &self.mode {
            Mode::Offline => overlay::draw(&mut self.frame, self.center),
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

    /// 画联机界面（世界文字、状态栏、增益、聊天、背包/装备/商店/技能/对话）。
    fn draw_ui(&mut self) {
        let center = self.center;
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
    }

    /// 渲染当前状态并呈现到窗口。
    fn draw(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        if self.frame.width != width || self.frame.height != height {
            self.frame = Frame::new(width, height);
        }
        self.render_current();
        self.draw_ui();
        if self.show_overlay {
            self.draw_overlay();
        }
        if let Some(surface) = self.surface.as_mut() {
            present(surface, &self.frame, width, height);
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
                    if !message.trim().is_empty() {
                        live.send(NetCommand::Chat(message));
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

    /// 联机攻击（朝当前朝向）。
    fn live_attack(&mut self) {
        let Mode::Live(live) = &mut self.mode else {
            return;
        };
        if let Some(direction) = live.snapshot.self_player.as_ref().map(|me| me.direction) {
            live.send(NetCommand::Attack(direction));
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
        let Mode::Live(live) = &mut self.mode else {
            return;
        };
        match action {
            // 纯数据动作直接走 `UiAction::net_command`（与无头测试共用同一映射）。
            action @ (UiAction::BuyItem { .. }
            | UiAction::UseItem { .. }
            | UiAction::DropItem { .. }
            | UiAction::MoveItem { .. }) => {
                if let Some(command) = action.net_command() {
                    live.send(command);
                }
            }
            UiAction::CallNpc { object_id, key } => {
                if key.eq_ignore_ascii_case("@exit") {
                    live.ui.close_npc();
                }
                live.send(NetCommand::CallNpc { object_id, key });
            }
            UiAction::CastMagic {
                spell,
                direction,
                target_id,
                location,
            } => {
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
        }
        self.request_redraw();
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
            Mode::Offline => Vec::new(),
        };
        let handled = !actions.is_empty();
        for action in actions {
            self.dispatch_ui_action(action);
        }
        handled
    }

    /// 卖一件选中的背包物品（商店面板打开时 `S`）。
    fn live_sell_selected(&mut self) -> bool {
        let Some((unique_id, _)) = self.selected_inventory() else {
            return false;
        };
        if let Mode::Live(live) = &mut self.mode {
            live.send(NetCommand::SellItem {
                unique_id,
                count: 1,
            });
        }
        true
    }

    /// 修理选中的背包物品（商店面板打开时 `R`）。
    fn live_repair_selected(&mut self) -> bool {
        let Some((unique_id, _)) = self.selected_inventory() else {
            return false;
        };
        if let Mode::Live(live) = &mut self.mode {
            live.send(NetCommand::RepairItem { unique_id });
        }
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

    /// 鼠标移动：记录位置；有面板/拖拽/右键菜单时重绘以更新悬停高亮与幽灵图标。
    fn handle_cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        let mut redraw = false;
        if let Mode::Live(live) = &mut self.mode {
            live.ui.mouse = Point::new(position.x as i32, position.y as i32);
            redraw = live.ui.show_inventory
                || live.ui.show_skills
                || live.ui.show_equipment
                || live.ui.shop_open
                || live.ui.dialog.is_some()
                || live.ui.drag.is_some()
                || live.ui.context_menu.is_some();
        }
        if redraw {
            self.request_redraw();
        }
    }

    /// 鼠标左键按下：点 NPC/对话按钮/商店行/背包格/技能行/地面。
    fn handle_left_press(&mut self) {
        let frame = Rect::screen(self.frame.width, self.frame.height);
        let mut action = None;
        let mut cell = None;
        if let Mode::Live(live) = &mut self.mode {
            if !live.snapshot.is_ready() {
                return;
            }
            let mouse = live.ui.mouse;
            let world_cell = self.scene.screen_to_cell(self.center, mouse);
            cell = Some(world_cell);
            action = live
                .ui
                .on_click(&live.snapshot, &self.text, frame, Some(world_cell));
        }
        if let Some(action) = action {
            let walking = matches!(action, UiAction::WorldMove { .. });
            self.dispatch_ui_action(action);
            if walking {
                self.start_walk_hold(cell);
            }
        }
        // 没有动作（点面板空白处/同一个格子）时什么也不做。
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

    /// Esc：按「右键菜单 > 拖拽 > 聊天 > NPC 对话 > 背包/装备/技能」的顺序关闭；
    /// 关对话时给服务端发 `@exit`。
    fn live_escape(&mut self) {
        let mut redraw = false;
        if let Mode::Live(live) = &mut self.mode {
            if live.ui.context_menu.is_some() {
                live.ui.context_menu = None;
                redraw = true;
            } else if live.ui.drag.is_some() {
                live.ui.drag = None;
                redraw = true;
            } else if live.chat_input.is_some() {
                live.chat_input = None;
                redraw = true;
            } else if live.ui.dialog.is_some() || live.ui.shop_open {
                let object_id = live.ui.npc_object_id.unwrap_or(0);
                live.send(NetCommand::CallNpc {
                    object_id,
                    key: "@exit".to_owned(),
                });
                live.ui.close_npc();
                redraw = true;
            } else if live.ui.show_inventory || live.ui.show_skills || live.ui.show_equipment {
                live.ui.close_panels();
                redraw = true;
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
        let mut events = Vec::new();
        if let Mode::Live(live) = &mut self.mode {
            while let Some(event) = live.net.try_recv() {
                events.push(event);
            }
        }
        for event in events {
            match event {
                NetEvent::Snapshot(snapshot) => self.apply_snapshot(*snapshot),
                NetEvent::MapChanged {
                    map_index,
                    position,
                } => self.apply_map_changed(map_index, position),
                NetEvent::Error(message) => eprintln!("{message}"),
                NetEvent::Closed(reason) => eprintln!("连接关闭：{reason}"),
            }
        }
    }

    /// 应用一份世界快照：更新状态、聊天日志与相机。
    fn apply_snapshot(&mut self, snapshot: WorldSnapshot) {
        let position = snapshot.self_player.as_ref().map(|player| player.position);
        let status = snapshot.status;
        let ready = snapshot.is_ready();
        let new_chat = snapshot.chat.last().map(mir_client::net::ChatLine::display);
        if let Mode::Live(live) = &mut self.mode {
            if live.last_status != status {
                println!("网络状态：{}", status.label());
                live.last_status = status;
            }
            live.snapshot = snapshot;
            live.ui.sync(&live.snapshot);
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
            Mode::Offline => return,
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
                "p" => {
                    self.live_pickup();
                    return;
                }
                "t" => {
                    self.live_open_chat();
                    return;
                }
                "q" => {
                    event_loop.exit();
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
            Key::Named(NamedKey::F1) => {
                self.show_overlay = !self.show_overlay;
                self.request_redraw();
            }
            Key::Named(NamedKey::F2) => self.save_screenshot(),
            Key::Named(NamedKey::F3) => self.live_toggle_skills(),
            Key::Named(NamedKey::Escape) => self.live_escape(),
            _ => {}
        }
    }
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
fn present(
    surface: &mut softbuffer::Surface<Arc<Window>, Arc<Window>>,
    frame: &Frame,
    width: u32,
    height: u32,
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
    // softbuffer 的像素格式是 0x00RRGGBB（无 alpha）。
    for (destination, source) in buffer.iter_mut().zip(frame.pixels.chunks_exact(4)) {
        *destination =
            (u32::from(source[0]) << 16) | (u32::from(source[1]) << 8) | u32::from(source[2]);
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
                    self.frame = Frame::new(size.width, size.height);
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
                (ElementState::Pressed, MouseButton::Left) => self.handle_left_press(),
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
        self.poll_network();
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
