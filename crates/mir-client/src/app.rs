//! winit 窗口 + softbuffer 软件呈现。
//!
//! 渲染本身全部在 `mir-render` 里完成（纯 headless），这里只负责：
//!
//! * 创建 800×600 的窗口，失败时打印可读的原因并以成功状态退出（无显示器的 CI）；
//! * 把 [`Frame`] 的 RGBA8 转成 softbuffer 需要的 `0x00RRGGBB`；
//! * 离线模式（`--map`）：方向键 / WASD 平移相机、`S` 截图、`F1` 调试叠层、`Q` 退出；
//! * 联机模式（`--connect`）：方向键/WASD 发送转身/走/跑、`Space` 攻击、`P` 捡取、
//!   `T` 打开聊天输入、`Esc` 关闭聊天/NPC 对话、`F1` 叠层、`F2` 截图、`Q` 退出；
//! * 每 100 ms 递增一次动画帧（对应 Crystal 的 `MapControl.AnimationCount++`，
//!   `GameScene.cs:1188-1193`）。
//!
//! 网络线程由 [`NetClient`] 管理；窗口创建失败时 `ClientApp` 被丢弃，
//! `NetClient` 的 `Drop` 会发送 `Quit` 并回收线程，不会留下半开连接。

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mir_client::net::{NetClient, NetCommand, NetEvent, NetStatus, WorldSnapshot};
use mir_core::Point;
use mir_core::enums::MirDirection;
use mir_render::{Frame, MapScene};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
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
/// 不 panic、不返回非零退出码。
pub fn run(scene: MapScene, center: Point) {
    run_app(scene, center, Mode::Offline);
}

/// 启动联机窗口。
///
/// `snapshot` 是进入世界后的首个快照（`wait_until_in_game` 的返回值），
/// `center` 通常是自己的坐标。窗口创建失败时连接会被 `Drop` 自动关闭。
pub fn run_live(scene: MapScene, center: Point, net: NetClient, snapshot: WorldSnapshot) {
    let status = snapshot.status;
    let mode = Mode::Live(Box::new(LiveState {
        net,
        snapshot,
        chat_input: None,
        shift: false,
        last_chat: None,
        last_status: status,
        entered: false,
    }));
    run_app(scene, center, mode);
}

/// 公共窗口事件循环。
fn run_app(scene: MapScene, center: Point, mode: Mode) {
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("无法初始化窗口系统（可能没有图形环境）：{error}");
            eprintln!("渲染器仍可用：`cargo test -p mir-render` 会把地图渲染到 target/。");
            return;
        }
    };
    let mut app = ClientApp::new(scene, center, mode);
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
    fn new(scene: MapScene, center: Point, mode: Mode) -> Self {
        Self {
            scene,
            center,
            frame: Frame::new(WINDOW_WIDTH, WINDOW_HEIGHT),
            mode,
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

    /// Esc：关聊天、关 NPC 对话。
    fn live_close_dialog(&mut self) {
        if let Mode::Live(live) = &mut self.mode {
            if live.chat_input.is_some() {
                live.chat_input = None;
            } else {
                live.snapshot.npc_dialog = None;
            }
        }
    }

    /// 从网络线程取事件，更新快照与相机。
    fn poll_network(&mut self) {
        let mut snapshots = Vec::new();
        let mut messages = Vec::new();
        if let Mode::Live(live) = &mut self.mode {
            while let Some(event) = live.net.try_recv() {
                match event {
                    NetEvent::Snapshot(snapshot) => snapshots.push(*snapshot),
                    NetEvent::Error(message) => messages.push(message),
                    NetEvent::Closed(reason) => messages.push(format!("连接关闭：{reason}")),
                }
            }
        }
        for message in messages {
            eprintln!("{message}");
        }
        for snapshot in snapshots {
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
    fn handle_live_key(&mut self, event_loop: &ActiveEventLoop, event: &KeyEvent) {
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
            Key::Named(NamedKey::Escape) => self.live_close_dialog(),
            Key::Character(text) => match text.to_ascii_lowercase().as_str() {
                "p" => self.live_pickup(),
                "t" => self.live_open_chat(),
                "q" => event_loop.exit(),
                _ => {}
            },
            _ => {}
        }
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
            WindowEvent::KeyboardInput { event, .. } => self.handle_key(event_loop, &event),
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.poll_network();
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
}
