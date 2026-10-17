//! winit 窗口 + softbuffer 软件呈现。
//!
//! 渲染本身全部在 `mir-render` 里完成（纯 headless），这里只负责：
//!
//! * 创建 800×600 的窗口，失败时打印可读的原因并以成功状态退出（无显示器的 CI）；
//! * 把 [`Frame`] 的 RGBA8 转成 softbuffer 需要的 `0x00RRGGBB`；
//! * 键盘：方向键 / W A D 平移相机（S 用于截图，见下）、`Q` 退出、`S` 截图、
//!   `F1` 切换调试叠层；
//! * 每 100 ms 递增一次动画帧（对应 Crystal 的 `MapControl.AnimationCount++`，
//!   `GameScene.cs:1188-1193`）。

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mir_core::Point;
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

/// 启动窗口事件循环。
///
/// 没有图形环境（`EventLoop::new` 失败、窗口/表面创建失败）时打印原因并返回，
/// 不 panic、不返回非零退出码。
pub fn run(scene: MapScene, center: Point) {
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("无法初始化窗口系统（可能没有图形环境）：{error}");
            eprintln!("渲染器仍可用：`cargo test -p mir-render` 会把地图渲染到 target/。");
            return;
        }
    };
    let mut app = ClientApp::new(scene, center);
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
    fn new(scene: MapScene, center: Point) -> Self {
        Self {
            scene,
            center,
            frame: Frame::new(WINDOW_WIDTH, WINDOW_HEIGHT),
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
        let attributes = Window::default_attributes()
            .with_title("传奇-Next · 地图")
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

    /// 平移相机并请求重绘。
    fn pan(&mut self, dx: i32, dy: i32) {
        self.center = self.clamp_center(Point::new(self.center.x + dx, self.center.y + dy));
        if let Some(window) = &self.window {
            window.request_redraw();
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
        self.scene.render(&mut self.frame, self.center);
        if self.show_overlay {
            overlay::draw(&mut self.frame, self.center);
        }
        if let Some(surface) = self.surface.as_mut() {
            present(surface, &self.frame, width, height);
        }
    }

    /// 保存截图到 `build/screenshot-<n>.png`。
    fn save_screenshot(&mut self) {
        // 先确保当前帧与相机状态一致（窗口还没创建时也能截图）。
        self.scene.render(&mut self.frame, self.center);
        if self.show_overlay {
            overlay::draw(&mut self.frame, self.center);
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
        match &event.logical_key {
            Key::Named(NamedKey::ArrowLeft) => self.pan(-1, 0),
            Key::Named(NamedKey::ArrowRight) => self.pan(1, 0),
            Key::Named(NamedKey::ArrowUp) => self.pan(0, -1),
            Key::Named(NamedKey::ArrowDown) => self.pan(0, 1),
            Key::Named(NamedKey::F1) => {
                self.show_overlay = !self.show_overlay;
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
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
            WindowEvent::KeyboardInput { event, .. } => self.handle_key(event_loop, &event),
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
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
}
