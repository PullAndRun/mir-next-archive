//! 设置面板（Crystal `OptionDialog`，`Client/MirScenes/Dialogs/OptionDialog.cs`；
//! 默认按键 `F12`，备用 `O`，见 `Client/KeyBindSettings.cs:221-224`）。
//!
//! 本面板把三组设置摆在一起，改动**即时生效并写回设置文件**（由窗口层 `app.rs`
//! 派发 [`SettingsAction`]）：音频（音效/背景音乐开关与音量）、语言（中/英，切换后
//! 面板自己的文案也跟着变）、显示（分辨率预设 + 渲染缩放）、按键说明表。
//!
//! 与其它面板一致：布局函数与命中测试共用同一份矩形，`Esc`/右上角「关闭」都能收起
//! （`UiState::escape` 里排在前面，不会抢别的面板的 Esc）。

use mir_core::{Language, Point};

use crate::client_config::{KEY_ACTIONS, KeyBindings, WindowMode};
use crate::geom::Rect;
use crate::ui::hud::{BODY_COLOR, GOLD_COLOR, MUTED_COLOR, SIZE_SMALL, TITLE_COLOR};
use crate::ui::labels::Label;
use crate::ui::text::UiText;

/// 面板宽度。
pub const PANEL_WIDTH: u32 = 520;
/// 面板高度。
pub const PANEL_HEIGHT: u32 = 470;
/// 小窗口下的最小宽度。
pub const MIN_PANEL_WIDTH: u32 = 280;
/// 小窗口下的最小高度。
pub const MIN_PANEL_HEIGHT: u32 = 180;
/// 内边距。
pub const PADDING: u32 = 10;
/// 标题高度。
pub const TITLE_HEIGHT: u32 = 22;
/// 每组设置的标题高度。
pub const SECTION_HEIGHT: u32 = 16;
/// 控件行高度。
pub const ROW_HEIGHT: u32 = 22;
/// 行间距。
pub const ROW_GAP: u32 = 4;
/// 按钮宽度（开关 / 加减）。
pub const BUTTON_WIDTH: u32 = 46;
/// 音量条宽度。
pub const VOLUME_BAR_WIDTH: u32 = 120;
/// 分辨率按钮宽度。
pub const RESOLUTION_WIDTH: u32 = 78;
/// 底部提示高度。
pub const FOOTER_HEIGHT: u32 = 16;
/// 音量步长（百分比）。
pub const VOLUME_STEP: i32 = 10;
/// 可选的渲染分辨率（窗口像素）。
pub const RESOLUTIONS: [(u32, u32); 4] = [(800, 600), (1024, 768), (1280, 720), (1600, 900)];
/// 可选的渲染缩放（1 = 原生；2 = 半分辨率放大两倍；3 = 三分之一分辨率）。
pub const SCALES: [u32; 3] = [1, 2, 3];

/// 按键说明表：`(按键, 动作文案)`。
///
/// 与 `app.rs` 的实际绑定一一对应（方向键/WASD、Shift 跑、Space 攻击、Tab 拾取、
/// `T` 聊天、`I` 背包、`E` 装备、`F3` 技能、`M` 小地图、`B` 大地图、`Q` 任务、
/// `N` 邮件、`G` 行会、`P` 组队、`Y` 交易、`O` 设置、`V` 宠物模式、`Esc` 关面板、`F10` 退出）。
pub const KEY_BINDINGS: [(&str, Label); 19] = [
    ("WASD / \u{2190}\u{2191}\u{2193}\u{2192}", Label::KeyMove),
    // 键名只用 ASCII（"Shift + WASD"），动作文案走 `Label::KeyRun`。
    ("Shift + WASD", Label::KeyRun),
    ("Space", Label::KeyAttack),
    ("Tab", Label::KeyPickUp),
    ("T", Label::KeyChat),
    ("I", Label::Inventory),
    ("E", Label::Equipment),
    ("F3", Label::Skills),
    ("M", Label::Minimap),
    ("B", Label::Bigmap),
    ("Q", Label::Quests),
    ("N", Label::Mail),
    ("G", Label::Guild),
    ("P", Label::Group),
    ("Y", Label::Trade),
    ("O", Label::Settings),
    // `V` 是宠物模式循环（`C.ChangePMode`；`P` 已被组队占用）。
    ("V", Label::PetMode),
    ("Esc", Label::KeyEscape),
    ("F10", Label::Quit),
];

/// 设置面板的可编辑状态（窗口层用设置文件初始化，并把每个动作写回文件）。
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsState {
    /// 音效开关。
    pub sound_on: bool,
    /// 背景音乐开关。
    pub music_on: bool,
    /// 音效音量（0..=100）。
    pub sound_volume: i32,
    /// 背景音乐音量（0..=100）。
    pub music_volume: i32,
    /// 主音量（0..=100）。
    pub master_volume: i32,
    /// 界面语言。
    pub language: Language,
    /// 分辨率预设（窗口像素）。
    pub resolution: (u32, u32),
    /// 渲染缩放（[`SCALES`] 之一）。
    pub scale: u32,
    /// 窗口模式。
    pub window_mode: WindowMode,
    /// 小地图是否显示传送点。
    pub show_minimap_gates: bool,
    /// 可重绑键位。
    pub keys: KeyBindings,
    /// 正在等待按键的动作下标（`None` = 不在捕获）。
    pub rebinding: Option<usize>,
    /// 上次重绑的冲突提示（按键名冲突/空键）。
    pub rebind_error: Option<String>,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            sound_on: true,
            music_on: true,
            sound_volume: 50,
            music_volume: 50,
            master_volume: 50,
            language: Language::default(),
            resolution: RESOLUTIONS[0],
            scale: 1,
            window_mode: WindowMode::Windowed,
            show_minimap_gates: true,
            keys: KeyBindings::default(),
            rebinding: None,
            rebind_error: None,
        }
    }
}

impl SettingsState {
    /// 把 0.0..=1.0 的音量换算成面板里的百分比（非有限值退回 50）。
    #[must_use]
    pub fn percent(volume: f32) -> i32 {
        if volume.is_finite() {
            #[allow(clippy::cast_possible_truncation)]
            let percent = (volume.clamp(0.0, 1.0) * 100.0).round() as i32;
            percent
        } else {
            50
        }
    }

    /// 百分比 → 0.0..=1.0 的音量。
    #[must_use]
    pub fn fraction(percent: i32) -> f32 {
        f32::from(i16::try_from(percent.clamp(0, 100)).unwrap_or(50)) / 100.0
    }

    /// 归一化（音量夹到 0..=100，缩放只允许 [`SCALES`]）。
    pub fn normalise(&mut self) {
        self.sound_volume = self.sound_volume.clamp(0, 100);
        self.music_volume = self.music_volume.clamp(0, 100);
        if !SCALES.contains(&self.scale) {
            self.scale = 1;
        }
    }
}

/// 面板产生的设置动作；窗口层负责落实（改音量/改语言/改窗口/写设置文件）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAction {
    /// 开关音效。
    ToggleSound,
    /// 开关背景音乐。
    ToggleMusic,
    /// 设置音效音量（0..=100）。
    SoundVolume(i32),
    /// 设置背景音乐音量（0..=100）。
    MusicVolume(i32),
    /// 切换界面语言。
    SetLanguage(Language),
    /// 主音量（0..=100）。
    MasterVolume(i32),
    /// 切换窗口模式。
    NextWindowMode,
    /// 切换小地图传送点显示。
    ToggleMinimapGates,
    /// 进入键位捕获（动作下标）。
    RebindKey(usize),
    /// 设置分辨率（窗口像素）。
    SetResolution(u32, u32),
    /// 设置渲染缩放。
    SetScale(u32),
    /// 关闭面板。
    Close,
}

/// 设置面板布局（绘制与命中测试共用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsLayout {
    /// 面板。
    pub panel: Rect,
    /// 关闭按钮。
    pub close: Rect,
    /// 音频分组标题。
    pub audio: Rect,
    /// 音乐标签。
    pub music_label: Rect,
    /// 音乐开关。
    pub music_toggle: Rect,
    /// 音乐音量减。
    pub music_down: Rect,
    /// 音乐音量条。
    pub music_bar: Rect,
    /// 音乐音量加。
    pub music_up: Rect,
    /// 音效标签。
    pub sound_label: Rect,
    /// 音效开关。
    pub sound_toggle: Rect,
    /// 音效音量减。
    pub sound_down: Rect,
    /// 音效音量条。
    pub sound_bar: Rect,
    /// 音效音量加。
    pub sound_up: Rect,
    /// 语言分组标题。
    pub language: Rect,
    /// 中文按钮。
    pub language_zh: Rect,
    /// 英文按钮。
    pub language_en: Rect,
    /// 显示分组标题。
    pub display: Rect,
    /// 分辨率标签。
    pub resolution_label: Rect,
    /// 分辨率按钮（顺序同 [`RESOLUTIONS`]）。
    pub resolutions: Vec<Rect>,
    /// 缩放的减号。
    pub scale_down: Rect,
    /// 缩放值显示。
    pub scale_value: Rect,
    /// 缩放的加号。
    pub scale_up: Rect,
    /// 按键说明分组标题。
    pub keys: Rect,
    /// 按键说明的可见行（面板放不下时截断）。
    pub key_rows: Vec<Rect>,
    /// 键位行的重绑按钮（与 `key_rows` 同序）。
    pub key_buttons: Vec<Rect>,
    /// 主音量行。
    pub master_label: Rect,
    /// 主音量减。
    pub master_down: Rect,
    /// 主音量条。
    pub master_bar: Rect,
    /// 主音量加。
    pub master_up: Rect,
    /// 窗口模式标签。
    pub window_label: Rect,
    /// 窗口模式按钮。
    pub window_button: Rect,
    /// 传送点标签。
    pub gates_label: Rect,
    /// 传送点开关。
    pub gates_button: Rect,
}

impl SettingsLayout {
    /// 命中的分辨率按钮（返回像素）。
    #[must_use]
    pub fn resolution_at(&self, point: Point) -> Option<(u32, u32)> {
        self.resolutions
            .iter()
            .position(|rect| rect.contains(point))
            .map(|index| RESOLUTIONS[index])
    }

    /// 命中的按键说明行。
    #[must_use]
    pub fn key_row_at(&self, point: Point) -> Option<usize> {
        self.key_rows.iter().position(|rect| rect.contains(point))
    }
}

/// 计算设置面板布局（`frame` 是整屏）。
#[must_use]
pub fn settings_layout(frame: Rect) -> SettingsLayout {
    let available_width = frame.width.saturating_sub(8);
    let width = PANEL_WIDTH
        .min(available_width)
        .max(MIN_PANEL_WIDTH.min(available_width));
    let available_height = frame.height.saturating_sub(8);
    let height = PANEL_HEIGHT
        .min(available_height)
        .max(MIN_PANEL_HEIGHT.min(available_height));
    let panel = Rect::new(frame.x + 4, frame.y + 4, width, height);
    let content_x = panel.x + i32::try_from(PADDING).unwrap_or(0);
    let content_width = width.saturating_sub(PADDING * 2);
    let content_right = content_x + i32::try_from(content_width).unwrap_or(0);
    let mut y = panel.y + 4;

    let title_height = i32::try_from(TITLE_HEIGHT).unwrap_or(22);
    let close = Rect::new(content_right - 18, y, 18, 18);
    y += title_height;

    let row = i32::try_from(ROW_HEIGHT).unwrap_or(22);
    let gap = i32::try_from(ROW_GAP).unwrap_or(4);
    let section = i32::try_from(SECTION_HEIGHT).unwrap_or(16);
    let button = BUTTON_WIDTH;
    let bar_width = VOLUME_BAR_WIDTH;
    let label_width = (content_width.saturating_sub(button * 2 + bar_width + 24)).max(60);

    let audio = Rect::new(content_x, y, content_width, SECTION_HEIGHT);
    y += section;
    // 主音量（新增，三条滑杆之一）。
    let master_label = Rect::new(content_x, y, label_width, ROW_HEIGHT);
    let master_down = Rect::new(
        content_x + i32::try_from(label_width + button).unwrap_or(0),
        y,
        18,
        ROW_HEIGHT,
    );
    let master_bar = Rect::new(master_down.right() + 4, y, bar_width, ROW_HEIGHT);
    let master_up = Rect::new(master_bar.right() + 4, y, 18, ROW_HEIGHT);
    y += row + gap;
    let music_label = Rect::new(content_x, y, label_width, ROW_HEIGHT);
    let music_toggle = Rect::new(
        content_x + i32::try_from(label_width).unwrap_or(0),
        y,
        button,
        ROW_HEIGHT,
    );
    let music_down = Rect::new(music_toggle.right() + 6, y, 18, ROW_HEIGHT);
    let music_bar = Rect::new(music_down.right() + 4, y, bar_width, ROW_HEIGHT);
    let music_up = Rect::new(music_bar.right() + 4, y, 18, ROW_HEIGHT);
    y += row + gap;
    let sound_label = Rect::new(content_x, y, label_width, ROW_HEIGHT);
    let sound_toggle = Rect::new(
        content_x + i32::try_from(label_width).unwrap_or(0),
        y,
        button,
        ROW_HEIGHT,
    );
    let sound_down = Rect::new(sound_toggle.right() + 6, y, 18, ROW_HEIGHT);
    let sound_bar = Rect::new(sound_down.right() + 4, y, bar_width, ROW_HEIGHT);
    let sound_up = Rect::new(sound_bar.right() + 4, y, 18, ROW_HEIGHT);
    y += row + gap * 2;

    let language = Rect::new(content_x, y, content_width, SECTION_HEIGHT);
    y += section;
    let language_zh = Rect::new(content_x, y, 72, ROW_HEIGHT);
    let language_en = Rect::new(language_zh.right() + 6, y, 72, ROW_HEIGHT);
    y += row + gap * 2;

    let display = Rect::new(content_x, y, content_width, SECTION_HEIGHT);
    y += section;
    let resolution_label = Rect::new(content_x, y, 64, ROW_HEIGHT);
    // 分辨率按钮按可用宽度均分（320×240 下也不会画出面板）。
    let available = content_width.saturating_sub(64 + 6 + 3 * 4);
    let resolution_width =
        (available / u32::try_from(RESOLUTIONS.len()).unwrap_or(4)).clamp(48, RESOLUTION_WIDTH);
    let mut x = resolution_label.right() + 6;
    let mut resolutions = Vec::with_capacity(RESOLUTIONS.len());
    for _ in RESOLUTIONS {
        resolutions.push(Rect::new(x, y, resolution_width, ROW_HEIGHT));
        x += i32::try_from(resolution_width).unwrap_or(78) + 4;
    }
    y += row + gap;
    let scale_label_width = 64;
    let scale_label = Rect::new(content_x, y, scale_label_width, ROW_HEIGHT);
    let scale_down = Rect::new(scale_label.right() + 6, y, 22, ROW_HEIGHT);
    let scale_value = Rect::new(scale_down.right() + 4, y, 48, ROW_HEIGHT);
    let scale_up = Rect::new(scale_value.right() + 4, y, 22, ROW_HEIGHT);
    y += row + gap;
    // 画面：窗口模式 + 小地图传送点。
    let window_label = Rect::new(content_x, y, 64, ROW_HEIGHT);
    let window_button = Rect::new(window_label.right() + 6, y, 96, ROW_HEIGHT);
    let gates_label = Rect::new(window_button.right() + 10, y, 96, ROW_HEIGHT);
    let gates_button = Rect::new(gates_label.right() + 4, y, 70, ROW_HEIGHT);
    y += row + gap * 2;

    let keys = Rect::new(content_x, y, content_width, SECTION_HEIGHT);
    y += section;
    let footer_top = panel.bottom() - i32::try_from(PADDING + FOOTER_HEIGHT).unwrap_or(0);
    // 6 个可重绑动作，每个一行（行尾是键位按钮）。
    let mut key_rows = Vec::new();
    let mut key_buttons = Vec::new();
    let mut row_y = y;
    while row_y + row <= footer_top - 2 && key_rows.len() < KEY_ACTIONS.len() {
        key_rows.push(Rect::new(content_x, row_y, content_width, ROW_HEIGHT));
        key_buttons.push(Rect::new(content_right - 96, row_y, 96, ROW_HEIGHT));
        row_y += row;
    }

    SettingsLayout {
        panel,
        close,
        audio,
        music_label,
        music_toggle,
        music_down,
        music_bar,
        music_up,
        sound_label,
        sound_toggle,
        sound_down,
        sound_bar,
        sound_up,
        language,
        language_zh,
        language_en,
        display,
        resolution_label,
        resolutions,
        scale_down,
        scale_value,
        scale_up,
        keys,
        key_rows,
        key_buttons,
        master_label,
        master_down,
        master_bar,
        master_up,
        window_label,
        window_button,
        gates_label,
        gates_button,
    }
}

/// 画一个按钮（与其它面板同款：深色底 + 1px 边框 + 小字）。
fn draw_button(
    frame: &mut mir_render::Frame,
    text: &UiText,
    rect: Rect,
    label: &str,
    active: bool,
) {
    let fill = if active {
        [82, 68, 30, 240]
    } else {
        [34, 42, 54, 235]
    };
    frame.fill_rect(rect.x, rect.y, rect.width, rect.height, fill);
    let border = if active {
        [255, 216, 96, 255]
    } else {
        [92, 110, 136, 255]
    };
    frame.fill_rect(rect.x, rect.y, rect.width, 1, border);
    frame.fill_rect(rect.x, rect.bottom() - 1, rect.width, 1, border);
    frame.fill_rect(rect.x, rect.y, 1, rect.height, border);
    frame.fill_rect(rect.right() - 1, rect.y, 1, rect.height, border);
    text.draw_clipped(
        frame,
        rect.x + 5,
        rect.y + 3,
        label,
        SIZE_SMALL,
        if active { TITLE_COLOR } else { BODY_COLOR },
        rect,
    );
}

/// 画音量条（0..=100）。
fn draw_volume_bar(frame: &mut mir_render::Frame, text: &UiText, rect: Rect, percent: i32) {
    frame.fill_rect(
        rect.x,
        rect.y + 6,
        rect.width,
        rect.height - 12,
        [20, 24, 32, 235],
    );
    let filled = u32::try_from(percent.clamp(0, 100)).unwrap_or(0) * rect.width / 100;
    if filled > 0 {
        frame.fill_rect(rect.x, rect.y + 6, filled, rect.height - 12, GOLD_COLOR);
    }
    text.draw_center(
        frame,
        rect,
        &format!("{percent}%"),
        SIZE_SMALL,
        [255, 255, 255, 255],
    );
}

/// 画设置面板。
pub fn draw(
    frame: &mut mir_render::Frame,
    layout: &SettingsLayout,
    state: &SettingsState,
    text: &UiText,
) {
    let language = state.language;
    crate::ui::draw_panel(frame, layout.panel);
    text.draw_clipped(
        frame,
        layout.panel.x + 10,
        layout.panel.y + 4,
        Label::Settings.text(language),
        crate::ui::hud::SIZE_BODY,
        TITLE_COLOR,
        Rect::new(
            layout.panel.x,
            layout.panel.y,
            layout.panel.width.saturating_sub(40),
            TITLE_HEIGHT,
        ),
    );
    draw_button(
        frame,
        text,
        layout.close,
        Label::Close.text(language),
        false,
    );

    // 音频
    text.draw_clipped(
        frame,
        layout.audio.x,
        layout.audio.y,
        Label::Audio.text(language),
        SIZE_SMALL,
        GOLD_COLOR,
        layout.audio,
    );
    text.draw_clipped(
        frame,
        layout.master_label.x,
        layout.master_label.y + 4,
        Label::MasterVolume.text(language),
        SIZE_SMALL,
        BODY_COLOR,
        layout.master_label,
    );
    draw_button(frame, text, layout.master_down, "-", false);
    draw_volume_bar(frame, text, layout.master_bar, state.master_volume);
    draw_button(frame, text, layout.master_up, "+", false);
    text.draw_clipped(
        frame,
        layout.music_label.x,
        layout.music_label.y + 4,
        Label::Music.text(language),
        SIZE_SMALL,
        BODY_COLOR,
        layout.music_label,
    );
    draw_button(
        frame,
        text,
        layout.music_toggle,
        if state.music_on {
            Label::ToggleOn.text(language)
        } else {
            Label::ToggleOff.text(language)
        },
        state.music_on,
    );
    draw_button(frame, text, layout.music_down, "-", false);
    draw_volume_bar(frame, text, layout.music_bar, state.music_volume);
    draw_button(frame, text, layout.music_up, "+", false);

    text.draw_clipped(
        frame,
        layout.sound_label.x,
        layout.sound_label.y + 4,
        Label::Sound.text(language),
        SIZE_SMALL,
        BODY_COLOR,
        layout.sound_label,
    );
    draw_button(
        frame,
        text,
        layout.sound_toggle,
        if state.sound_on {
            Label::ToggleOn.text(language)
        } else {
            Label::ToggleOff.text(language)
        },
        state.sound_on,
    );
    draw_button(frame, text, layout.sound_down, "-", false);
    draw_volume_bar(frame, text, layout.sound_bar, state.sound_volume);
    draw_button(frame, text, layout.sound_up, "+", false);

    // 语言
    text.draw_clipped(
        frame,
        layout.language.x,
        layout.language.y,
        Label::Language.text(language),
        SIZE_SMALL,
        GOLD_COLOR,
        layout.language,
    );
    draw_button(
        frame,
        text,
        layout.language_zh,
        Language::ZhCn.native_name(),
        state.language == Language::ZhCn,
    );
    draw_button(
        frame,
        text,
        layout.language_en,
        Language::En.native_name(),
        state.language == Language::En,
    );

    // 显示
    text.draw_clipped(
        frame,
        layout.display.x,
        layout.display.y,
        Label::Display.text(language),
        SIZE_SMALL,
        GOLD_COLOR,
        layout.display,
    );
    text.draw_clipped(
        frame,
        layout.resolution_label.x,
        layout.resolution_label.y + 4,
        Label::Resolution.text(language),
        SIZE_SMALL,
        BODY_COLOR,
        layout.resolution_label,
    );
    for (rect, (width, height)) in layout.resolutions.iter().zip(RESOLUTIONS.iter()) {
        let active = state.resolution == (*width, *height);
        draw_button(
            frame,
            text,
            *rect,
            &format!("{width}\u{00d7}{height}"),
            active,
        );
    }
    text.draw_clipped(
        frame,
        layout.scale_down.x.saturating_sub(56),
        layout.scale_down.y + 4,
        Label::UiScale.text(language),
        SIZE_SMALL,
        BODY_COLOR,
        Rect::new(
            layout.scale_down.x.saturating_sub(56),
            layout.scale_down.y,
            52,
            ROW_HEIGHT,
        ),
    );
    draw_button(frame, text, layout.scale_down, "-", false);
    text.draw_center(
        frame,
        layout.scale_value,
        &format!("{}x", state.scale),
        SIZE_SMALL,
        BODY_COLOR,
    );
    draw_button(frame, text, layout.scale_up, "+", false);
    text.draw_clipped(
        frame,
        layout.window_label.x,
        layout.window_label.y + 4,
        Label::WindowMode.text(language),
        SIZE_SMALL,
        BODY_COLOR,
        layout.window_label,
    );
    draw_button(
        frame,
        text,
        layout.window_button,
        match state.window_mode {
            WindowMode::Windowed => "Windowed",
            WindowMode::Fullscreen => "Fullscreen",
            WindowMode::Borderless => "Borderless",
        },
        true,
    );
    text.draw_clipped(
        frame,
        layout.gates_label.x,
        layout.gates_label.y + 4,
        Label::MinimapGates.text(language),
        SIZE_SMALL,
        BODY_COLOR,
        layout.gates_label,
    );
    draw_button(
        frame,
        text,
        layout.gates_button,
        if state.show_minimap_gates {
            Label::ToggleOn.text(language)
        } else {
            Label::ToggleOff.text(language)
        },
        state.show_minimap_gates,
    );

    // 键位（6 个可重绑动作：点击行尾按钮后按任意键）
    text.draw_clipped(
        frame,
        layout.keys.x,
        layout.keys.y,
        Label::KeyBindings.text(language),
        SIZE_SMALL,
        GOLD_COLOR,
        layout.keys,
    );
    for (index, (row, button)) in layout
        .key_rows
        .iter()
        .zip(layout.key_buttons.iter())
        .enumerate()
    {
        let Some(action) = KEY_ACTIONS.get(index) else {
            break;
        };
        let label = match *action {
            "attack" => Label::Attack,
            "magic" => Label::KeyMagic,
            "inventory" => Label::Inventory,
            "group" => Label::Group,
            "friends" => Label::Friends,
            _ => Label::Settings,
        };
        text.draw_clipped(
            frame,
            row.x + 2,
            row.y + 4,
            label.text(language),
            SIZE_SMALL,
            BODY_COLOR,
            *row,
        );
        let bound = if state.rebinding == Some(index) {
            "…".to_owned()
        } else {
            state.keys.get(action).to_owned()
        };
        draw_button(frame, text, *button, &bound, state.rebinding == Some(index));
    }

    // 底部提示
    let footer = Rect::new(
        layout.panel.x + i32::try_from(PADDING).unwrap_or(0),
        layout.panel.bottom() - i32::try_from(PADDING + FOOTER_HEIGHT).unwrap_or(0),
        layout.panel.width.saturating_sub(PADDING * 2),
        FOOTER_HEIGHT,
    );
    text.draw_clipped(
        frame,
        footer.x,
        footer.y,
        state
            .rebind_error
            .as_deref()
            .unwrap_or_else(|| Label::SettingsHint.text(language)),
        SIZE_SMALL,
        if state.rebind_error.is_some() {
            [255, 120, 110, 255]
        } else {
            MUTED_COLOR
        },
        footer,
    );
}

/// 处理一次点击（就地改 `state`，返回要给窗口层落实的动作）。
pub fn click(
    layout: &SettingsLayout,
    state: &mut SettingsState,
    point: Point,
) -> Option<SettingsAction> {
    if !layout.panel.contains(point) {
        return None;
    }
    if layout.close.contains(point) {
        return Some(SettingsAction::Close);
    }
    if layout.master_down.contains(point) {
        state.master_volume = (state.master_volume - VOLUME_STEP).clamp(0, 100);
        return Some(SettingsAction::MasterVolume(state.master_volume));
    }
    if layout.master_up.contains(point) {
        state.master_volume = (state.master_volume + VOLUME_STEP).clamp(0, 100);
        return Some(SettingsAction::MasterVolume(state.master_volume));
    }
    if layout.window_button.contains(point) {
        state.window_mode = state.window_mode.next();
        return Some(SettingsAction::NextWindowMode);
    }
    if layout.gates_button.contains(point) {
        state.show_minimap_gates = !state.show_minimap_gates;
        return Some(SettingsAction::ToggleMinimapGates);
    }
    if let Some(index) = layout
        .key_buttons
        .iter()
        .position(|button| button.contains(point))
    {
        state.rebinding = Some(index);
        return Some(SettingsAction::RebindKey(index));
    }
    if layout.music_toggle.contains(point) {
        state.music_on = !state.music_on;
        return Some(SettingsAction::ToggleMusic);
    }
    if layout.sound_toggle.contains(point) {
        state.sound_on = !state.sound_on;
        return Some(SettingsAction::ToggleSound);
    }
    if layout.music_down.contains(point) {
        state.music_volume = (state.music_volume - VOLUME_STEP).clamp(0, 100);
        return Some(SettingsAction::MusicVolume(state.music_volume));
    }
    if layout.music_up.contains(point) {
        state.music_volume = (state.music_volume + VOLUME_STEP).clamp(0, 100);
        return Some(SettingsAction::MusicVolume(state.music_volume));
    }
    if layout.sound_down.contains(point) {
        state.sound_volume = (state.sound_volume - VOLUME_STEP).clamp(0, 100);
        return Some(SettingsAction::SoundVolume(state.sound_volume));
    }
    if layout.sound_up.contains(point) {
        state.sound_volume = (state.sound_volume + VOLUME_STEP).clamp(0, 100);
        return Some(SettingsAction::SoundVolume(state.sound_volume));
    }
    if layout.language_zh.contains(point) {
        state.language = Language::ZhCn;
        return Some(SettingsAction::SetLanguage(Language::ZhCn));
    }
    if layout.language_en.contains(point) {
        state.language = Language::En;
        return Some(SettingsAction::SetLanguage(Language::En));
    }
    if let Some((width, height)) = layout.resolution_at(point) {
        state.resolution = (width, height);
        return Some(SettingsAction::SetResolution(width, height));
    }
    if layout.scale_down.contains(point) {
        let index = SCALES
            .iter()
            .position(|scale| *scale == state.scale)
            .unwrap_or(0);
        state.scale = SCALES[index.saturating_sub(1)];
        return Some(SettingsAction::SetScale(state.scale));
    }
    if layout.scale_up.contains(point) {
        let index = SCALES
            .iter()
            .position(|scale| *scale == state.scale)
            .unwrap_or(0);
        state.scale = SCALES[(index + 1).min(SCALES.len() - 1)];
        return Some(SettingsAction::SetScale(state.scale));
    }
    // 面板其它位置吃掉点击（避免误点成走路），不产生动作。
    None
}

/// 当前分辨率文案（诊断/测试用）。
#[must_use]
pub fn resolution_text(resolution: (u32, u32)) -> String {
    format!("{}\u{00d7}{}", resolution.0, resolution.1)
}

/// 百分比文案（测试/叠层用）。
#[must_use]
pub fn volume_text(percent: i32) -> String {
    format!("{}%", percent.clamp(0, 100))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: u32, height: u32) -> Rect {
        Rect::screen(width, height)
    }

    #[test]
    fn layout_fits_small_and_large_screens() {
        for (width, height) in [(320, 240), (800, 600)] {
            let screen = frame(width, height);
            let layout = settings_layout(screen);
            assert!(
                layout.panel.right() <= screen.right(),
                "{width}x{height} 面板越右边界"
            );
            assert!(
                layout.panel.bottom() <= screen.bottom(),
                "{width}x{height} 面板越下边界"
            );
            for rect in [
                layout.close,
                layout.music_toggle,
                layout.sound_toggle,
                layout.music_down,
                layout.music_up,
                layout.sound_down,
                layout.sound_up,
                layout.language_zh,
                layout.language_en,
                layout.scale_down,
                layout.scale_up,
            ] {
                assert!(
                    layout.panel.contains(Point::new(rect.x, rect.y)),
                    "{width}x{height} 控件不在面板内"
                );
            }
            for rect in &layout.resolutions {
                assert!(
                    rect.right() <= layout.panel.right(),
                    "{width}x{height} 分辨率按钮出面板"
                );
                assert!(
                    rect.right() <= screen.right(),
                    "{width}x{height} 分辨率按钮出屏"
                );
            }
        }
    }

    #[test]
    fn clicks_change_audio_language_display_and_close() {
        let screen = frame(800, 600);
        let layout = settings_layout(screen);
        let mut state = SettingsState::default();
        let center = |rect: Rect| Point::new(rect.x + 2, rect.y + 2);

        assert_eq!(
            click(&layout, &mut state, center(layout.music_toggle)),
            Some(SettingsAction::ToggleMusic)
        );
        assert!(!state.music_on);
        assert_eq!(
            click(&layout, &mut state, center(layout.music_up)),
            Some(SettingsAction::MusicVolume(60))
        );
        assert_eq!(state.music_volume, 60);
        for _ in 0..12 {
            click(&layout, &mut state, center(layout.music_down));
        }
        assert_eq!(state.music_volume, 0);
        assert_eq!(
            click(&layout, &mut state, center(layout.sound_down)),
            Some(SettingsAction::SoundVolume(40))
        );
        assert_eq!(
            click(&layout, &mut state, center(layout.language_en)),
            Some(SettingsAction::SetLanguage(Language::En))
        );
        assert_eq!(state.language, Language::En);
        let resolution_rect = layout.resolutions[2];
        assert_eq!(
            click(&layout, &mut state, center(resolution_rect)),
            Some(SettingsAction::SetResolution(1280, 720))
        );
        assert_eq!(state.resolution, (1280, 720));
        assert_eq!(
            click(&layout, &mut state, center(layout.scale_up)),
            Some(SettingsAction::SetScale(2))
        );
        assert_eq!(state.scale, 2);
        assert_eq!(
            click(&layout, &mut state, center(layout.close)),
            Some(SettingsAction::Close)
        );
    }

    #[test]
    fn click_outside_the_panel_is_ignored() {
        let layout = settings_layout(frame(800, 600));
        let mut state = SettingsState::default();
        assert_eq!(
            click(&layout, &mut state, Point::new(0, 599)),
            None,
            "面板外的点击应交给世界层"
        );
    }

    #[test]
    fn scale_stays_inside_the_allowed_range() {
        let layout = settings_layout(frame(800, 600));
        let mut state = SettingsState::default();
        for _ in 0..5 {
            click(
                &layout,
                &mut state,
                Point::new(layout.scale_up.x + 2, layout.scale_up.y + 2),
            );
        }
        assert_eq!(state.scale, 3);
        for _ in 0..5 {
            click(
                &layout,
                &mut state,
                Point::new(layout.scale_down.x + 2, layout.scale_down.y + 2),
            );
        }
        assert_eq!(state.scale, 1);
    }

    #[test]
    fn volume_percent_and_fraction_roundtrip() {
        assert_eq!(SettingsState::percent(0.5), 50);
        assert_eq!(SettingsState::percent(1.5), 100);
        assert_eq!(SettingsState::percent(f32::NAN), 50);
        assert!((SettingsState::fraction(75) - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn key_table_is_localized() {
        for (key, label) in KEY_BINDINGS {
            assert!(!key.is_empty());
            assert!(!label.text(Language::ZhCn).is_empty());
            assert!(!label.text(Language::En).is_empty());
        }
        assert_eq!(resolution_text((800, 600)), "800\u{00d7}600");
        assert_eq!(volume_text(150), "100%");
    }
}
