//! 服务器选择与角色选择界面（Crystal `LoginScene` / `SelectScene`）。
//!
//! 对应 Crystal 的两段流程：
//!
//! * **登录页**（`Client/MirScenes/LoginScene.cs:12-120`）：服务器地址 + 账号 + 口令，
//!   连接失败时留在原地并给出可重试的中文提示；
//! * **选角页**（`Client/MirScenes/SelectScene.cs:10-230`）：角色列表显示职业/等级/
//!   上次登录，空列表时引导创建（Crystal 的「新建角色」按钮，`SelectScene.cs:245-260`）。
//!
//! 服务器地址与分辨率一样存在本地设置里（默认 [`DEFAULT_SERVER`]），重启后保留。
//! 选角页的**装备摘要**取自本地缓存（上次进入世界时从 `S.UserInformation.Equipment`
//! 记录，按角色名保存）：协议里的 `SelectInfo` 只有名字/等级/职业/性别/上次登录
//! （`Shared/Data/SharedData.cs:3-30`），没有装备字段，所以没有缓存时显示
//! 「暂无装备」而不是伪造数据。

use std::collections::BTreeMap;

use mir_core::DateTimeBinary;
use mir_core::enums::{MirClass, MirGender};
use mir_core::{Language, Point};
use mir_render::Frame;

use crate::geom::Rect;
use crate::net::CharacterInfo;
use crate::ui::UiText;
use crate::ui::hud::{BODY_COLOR, GOLD_COLOR, MUTED_COLOR, SIZE_BODY, SIZE_SMALL, TITLE_COLOR};
use crate::ui::input::{self, TextField};
use crate::ui::labels::{self, Label};

/// 默认服务器地址（Crystal 登录页默认连本机；服务端默认端口 7000）。
pub const DEFAULT_SERVER: &str = "127.0.0.1:7000";
/// 登录面板宽度。
pub const PANEL_WIDTH: u32 = 420;
/// 登录面板高度。
pub const PANEL_HEIGHT: u32 = 240;
/// 选角面板宽度。
pub const SELECT_PANEL_WIDTH: u32 = 560;
/// 选角面板高度。
pub const SELECT_PANEL_HEIGHT: u32 = 330;
/// 小窗口下的最小宽度。
pub const MIN_PANEL_WIDTH: u32 = 280;
/// 小窗口下的最小高度。
pub const MIN_PANEL_HEIGHT: u32 = 150;
/// 内边距。
pub const PADDING: u32 = 10;
/// 标题高度。
pub const TITLE_HEIGHT: u32 = 24;
/// 输入行高度。
pub const FIELD_HEIGHT: u32 = 24;
/// 行间距。
pub const ROW_GAP: u32 = 6;
/// 角色卡高度。
pub const CARD_HEIGHT: u32 = 62;
/// 按钮高度。
pub const BUTTON_HEIGHT: u32 = 24;
/// 底部提示高度。
pub const HINT_HEIGHT: u32 = 16;
/// 角色名长度上限（Crystal 角色名 7 个 UTF-16 单元，这里按字符放宽到 12）。
pub const NAME_MAX_CHARS: usize = 12;

/// 登录页焦点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginField {
    /// 服务器地址。
    Server,
    /// 账号。
    Account,
    /// 口令。
    Password,
}

/// 连接状态（由窗口层根据网络结果更新）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectState {
    /// 还没开始 / 已经失败，可以点「连接」或「重试」。
    Idle,
    /// 正在连接/登录。
    Connecting,
}

/// 连接失败的类别（决定提示文案，见 [`error_label`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectError {
    /// 连不上服务器（地址/端口/网络）。
    Unreachable,
    /// 账号或口令错误。
    Credentials,
    /// 服务器拒绝连接。
    Refused,
    /// 超时。
    Timeout,
    /// 客户端/服务端版本不匹配。
    Version,
    /// 其它错误（原始文本只进日志，不画到 UI）。
    Other,
}

/// 失败提示文案：只给玩家看**本地化类别**，不含网络/服务端原文。
#[must_use]
pub const fn error_label(error: ConnectError) -> Label {
    match error {
        ConnectError::Unreachable => Label::ConnectUnreachable,
        ConnectError::Credentials => Label::ConnectCredentials,
        ConnectError::Refused => Label::ConnectRefused,
        ConnectError::Timeout => Label::ConnectTimeout,
        ConnectError::Version => Label::ConnectVersionMismatch,
        ConnectError::Other => Label::ConnectFailed,
    }
}

/// 失败提示的成品文本（本地化；测试与绘制共用）。
#[must_use]
pub const fn error_text(error: ConnectError, language: Language) -> &'static str {
    error_label(error).text(language)
}

/// 登录页状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSelectState {
    /// 服务器地址（`host:port`）。
    pub address: TextField,
    /// 账号。
    pub account: TextField,
    /// 口令。
    pub password: TextField,
    /// 当前焦点。
    pub focus: LoginField,
    /// 连接状态。
    pub status: ConnectState,
    /// 失败提示（`None` = 没有错误）。
    pub error: Option<ConnectError>,
    /// 失败详情（网络层的原始诊断串，**只进日志/测试，不画到 UI**）。
    pub message: Option<String>,
}

impl Default for ServerSelectState {
    fn default() -> Self {
        let mut address = TextField::new(64);
        address.set(DEFAULT_SERVER);
        Self {
            address,
            account: TextField::new(24),
            password: TextField::new(24),
            focus: LoginField::Account,
            status: ConnectState::Idle,
            error: None,
            message: None,
        }
    }
}

impl ServerSelectState {
    /// 用设置文件里的地址初始化（空值退回 [`DEFAULT_SERVER`]）。
    #[must_use]
    pub fn with_server(server: &str) -> Self {
        let mut state = Self::default();
        if !server.trim().is_empty() {
            state.address.set(server);
        }
        state
    }

    /// 开始（或重试）连接：清掉上次错误，进入「连接中」。
    pub fn begin_connect(&mut self) {
        self.error = None;
        self.message = None;
        self.status = ConnectState::Connecting;
    }

    /// 记录一次失败（连接失败后可重试）。
    pub fn fail(&mut self, error: ConnectError, message: Option<String>) {
        self.status = ConnectState::Idle;
        self.error = Some(error);
        self.message = message;
    }

    /// 连接成功（窗口层切到选角页）。
    pub fn succeed(&mut self) {
        self.status = ConnectState::Idle;
        self.error = None;
        self.message = None;
    }

    /// 当前是否可以向窗口层发「连接」请求：地址、账号、口令都非空且不在连接中。
    #[must_use]
    pub fn can_connect(&self) -> bool {
        self.status != ConnectState::Connecting
            && !self.address.value.trim().is_empty()
            && !self.account.value.trim().is_empty()
            && !self.password.value.is_empty()
    }
}

/// 选角页的一张角色卡（`SelectInfo` + 本地装备缓存）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterCard {
    /// 角色槽位序号（`C.StartGame.CharacterIndex`）。
    pub index: i32,
    /// 角色名。
    pub name: String,
    /// 等级。
    pub level: u16,
    /// 职业。
    pub class: MirClass,
    /// 性别。
    pub gender: MirGender,
    /// 上次登录时间（`DateTimeBinary`；未登录过为默认值）。
    pub last_access: DateTimeBinary,
    /// 装备摘要（来自本地缓存；空表示新角色/没记录）。
    pub equipment: String,
}

impl CharacterCard {
    /// 从协议信息 + 本地缓存构造。
    #[must_use]
    pub fn from_info(info: &CharacterInfo, equipment: Option<String>) -> Self {
        Self {
            index: info.index,
            name: info.name.clone(),
            level: info.level,
            class: info.class,
            gender: info.gender,
            last_access: info.last_access,
            equipment: equipment.unwrap_or_default(),
        }
    }

    /// 卡片第一行：`战士 · 等级 22 · 男`。
    #[must_use]
    pub fn headline(&self, language: Language) -> String {
        format!(
            "{} · {} {} · {}",
            labels::class_name(self.class, language),
            Label::Level.text(language),
            self.level,
            gender_name(self.gender, language)
        )
    }

    /// 卡片第二行：`上次登录 2026-10-18 18:00` / `从未登录`。
    #[must_use]
    pub fn last_access_text(&self, language: Language) -> String {
        if self.last_access.is_unset() {
            return format!(
                "{} {}",
                Label::LastAccess.text(language),
                Label::Never.text(language)
            );
        }
        format!(
            "{} {}",
            Label::LastAccess.text(language),
            date_text(self.last_access)
        )
    }

    /// 卡片第三行：装备摘要（没有就显示「暂无装备」）。
    #[must_use]
    pub fn equipment_text(&self, language: Language) -> String {
        if self.equipment.trim().is_empty() {
            format!(
                "{}: {}",
                Label::Equipment.text(language),
                Label::NoEquipmentSummary.text(language)
            )
        } else {
            format!("{}: {}", Label::Equipment.text(language), self.equipment)
        }
    }
}

/// 性别名。
#[must_use]
pub const fn gender_name(gender: MirGender, language: Language) -> &'static str {
    match (gender, language) {
        (MirGender::Male, Language::ZhCn) => Label::Male.text(language),
        (MirGender::Male, Language::En) => "Male",
        (MirGender::Female, Language::ZhCn) => Label::Female.text(language),
        (MirGender::Female, Language::En) => "Female",
    }
}

/// `DateTimeBinary` → `YYYY-MM-DD HH:MM`（与服务端 `DateTime.ToBinary()` 一致）。
#[must_use]
pub fn date_text(date: DateTimeBinary) -> String {
    let seconds = date.to_unix_seconds().max(0);
    let days = seconds / 86_400;
    let minutes_of_day = (seconds % 86_400) / 60;
    let (year, month, day) = crate::ui::mail::civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        minutes_of_day / 60,
        minutes_of_day % 60
    )
}

/// 选角页状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterSelectState {
    /// 角色列表（按服务端顺序）。
    pub characters: Vec<CharacterCard>,
    /// 选中的角色下标。
    pub selected: usize,
    /// 是否展开「创建角色」表单。
    pub creating: bool,
    /// 新角色名。
    pub create_name: TextField,
    /// 新角色职业。
    pub create_class: MirClass,
    /// 新角色性别。
    pub create_gender: MirGender,
    /// 失败提示（创建失败/进入失败）。
    pub error: Option<ConnectError>,
    /// 失败详情（网络层原始诊断串，**只进日志/测试，不画到 UI**）。
    pub message: Option<String>,
    /// 装备摘要缓存（角色名 → 摘要），由窗口层从设置文件灌入。
    pub equipment_cache: BTreeMap<String, String>,
}

impl Default for CharacterSelectState {
    fn default() -> Self {
        Self {
            characters: Vec::new(),
            selected: 0,
            creating: false,
            create_name: TextField::new(NAME_MAX_CHARS),
            create_class: MirClass::Warrior,
            create_gender: MirGender::Male,
            error: None,
            message: None,
            equipment_cache: BTreeMap::new(),
        }
    }
}

impl CharacterSelectState {
    /// 用角色列表刷新（保留当前选中；空列表时选中 0 并建议创建）。
    pub fn set_characters(&mut self, infos: &[CharacterInfo]) {
        self.characters = infos
            .iter()
            .map(|info| {
                CharacterCard::from_info(info, self.equipment_cache.get(&info.name).cloned())
            })
            .collect();
        if self.characters.is_empty() {
            self.selected = 0;
            // 账号下没有角色：直接展开创建表单（Crystal 也要求先建角才能进游戏）。
            self.creating = true;
        } else {
            self.selected = self.selected.min(self.characters.len() - 1);
        }
    }

    /// 当前选中的角色。
    #[must_use]
    pub fn current(&self) -> Option<&CharacterCard> {
        self.characters.get(self.selected)
    }

    /// 记录一次失败（进入游戏/创建角色失败）。
    pub fn fail(&mut self, error: ConnectError, message: Option<String>) {
        self.error = Some(error);
        self.message = message;
    }

    /// 清掉错误（重试/重新选择时）。
    pub fn clear_error(&mut self) {
        self.error = None;
        self.message = None;
    }

    /// 新角色名是否可提交（非空且不重名）。
    #[must_use]
    pub fn can_create(&self) -> bool {
        let name = self.create_name.value.trim();
        !name.is_empty() && !self.characters.iter().any(|card| card.name == name)
    }
}

/// 登录页布局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerLayout {
    /// 面板。
    pub panel: Rect,
    /// 标题。
    pub title: Rect,
    /// 服务器地址输入框。
    pub address: Rect,
    /// 账号输入框。
    pub account: Rect,
    /// 口令输入框。
    pub password: Rect,
    /// 连接/重试按钮。
    pub connect: Rect,
    /// 退出按钮。
    pub quit: Rect,
    /// 失败提示区。
    pub error: Rect,
    /// 底部提示。
    pub hint: Rect,
}

/// 选角页布局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterLayout {
    /// 面板。
    pub panel: Rect,
    /// 标题。
    pub title: Rect,
    /// 角色卡（可见部分）。
    pub cards: Vec<Rect>,
    /// 进入游戏。
    pub enter: Rect,
    /// 创建角色（展开表单）。
    pub create: Rect,
    /// 返回服务器选择。
    pub back: Rect,
    /// 失败提示区。
    pub error: Rect,
    /// 底部提示。
    pub hint: Rect,
    /// 创建表单：角色名输入框。
    pub name_field: Rect,
    /// 创建表单：职业左/值/右。
    pub class_left: Rect,
    /// 职业值。
    pub class_value: Rect,
    /// 职业右。
    pub class_right: Rect,
    /// 创建表单：性别左/值/右。
    pub gender_left: Rect,
    /// 性别值。
    pub gender_value: Rect,
    /// 性别右。
    pub gender_right: Rect,
    /// 创建表单：确认。
    pub confirm: Rect,
    /// 创建表单：取消。
    pub cancel: Rect,
}

fn panel_rect(frame: Rect, width: u32, height: u32) -> Rect {
    let available_width = frame.width.saturating_sub(8);
    let width = width
        .min(available_width)
        .max(MIN_PANEL_WIDTH.min(available_width));
    let available_height = frame.height.saturating_sub(8);
    let height = height
        .min(available_height)
        .max(MIN_PANEL_HEIGHT.min(available_height));
    Rect::new(
        frame.x + i32::try_from((frame.width.saturating_sub(width)) / 2).unwrap_or(0),
        frame.y + i32::try_from((frame.height.saturating_sub(height)) / 2).unwrap_or(0),
        width,
        height,
    )
}

/// 登录页布局。
#[must_use]
pub fn server_layout(frame: Rect) -> ServerLayout {
    let panel = panel_rect(frame, PANEL_WIDTH, PANEL_HEIGHT);
    let content_x = panel.x + i32::try_from(PADDING).unwrap_or(0);
    let content_width = panel.width.saturating_sub(PADDING * 2);
    let mut y = panel.y + i32::try_from(TITLE_HEIGHT).unwrap_or(24);
    let field_height = i32::try_from(FIELD_HEIGHT).unwrap_or(24);
    let gap = i32::try_from(ROW_GAP).unwrap_or(6);
    let address = Rect::new(content_x, y, content_width, FIELD_HEIGHT);
    y += field_height + gap;
    let account = Rect::new(content_x, y, content_width, FIELD_HEIGHT);
    y += field_height + gap;
    let password = Rect::new(content_x, y, content_width, FIELD_HEIGHT);
    y += field_height + gap * 2;
    let button_height = i32::try_from(BUTTON_HEIGHT).unwrap_or(24);
    let connect = Rect::new(content_x, y, 100, BUTTON_HEIGHT);
    let quit = Rect::new(connect.right() + 8, y, 80, BUTTON_HEIGHT);
    y += button_height + gap;
    let error = Rect::new(content_x, y, content_width, 32);
    let hint = Rect::new(
        content_x,
        panel.bottom() - i32::try_from(PADDING + HINT_HEIGHT).unwrap_or(0),
        content_width,
        HINT_HEIGHT,
    );
    ServerLayout {
        panel,
        title: Rect::new(content_x, panel.y + 7, content_width, TITLE_HEIGHT),
        address,
        account,
        password,
        connect,
        quit,
        error,
        hint,
    }
}

/// 选角页布局（`creating` 决定是否留出创建表单的位置）。
#[must_use]
pub fn character_layout(frame: Rect, count: usize, creating: bool) -> CharacterLayout {
    let panel = panel_rect(frame, SELECT_PANEL_WIDTH, SELECT_PANEL_HEIGHT);
    let content_x = panel.x + i32::try_from(PADDING).unwrap_or(0);
    let content_width = panel.width.saturating_sub(PADDING * 2);
    let y = panel.y + i32::try_from(TITLE_HEIGHT).unwrap_or(24);
    let hint = Rect::new(
        content_x,
        panel.bottom() - i32::try_from(PADDING + HINT_HEIGHT).unwrap_or(0),
        content_width,
        HINT_HEIGHT,
    );
    let button_height = i32::try_from(BUTTON_HEIGHT).unwrap_or(24);
    let gap = i32::try_from(ROW_GAP).unwrap_or(6);
    let actions_top = hint.y - 4 - button_height;
    let error = Rect::new(content_x, actions_top - 34, content_width, 30);
    let form_top = error.y - 4;
    let card_height = i32::try_from(CARD_HEIGHT).unwrap_or(62);
    let cards_bottom = if creating {
        form_top - i32::try_from(4 * (FIELD_HEIGHT + ROW_GAP)).unwrap_or(120)
    } else {
        actions_top - 4
    };
    let mut cards = Vec::new();
    let visible = usize::try_from(((cards_bottom - y).max(0) / (card_height + gap)).max(0))
        .unwrap_or(0)
        .min(count);
    for index in 0..visible {
        cards.push(Rect::new(
            content_x,
            y + i32::try_from(index).unwrap_or(0) * (card_height + gap),
            content_width,
            CARD_HEIGHT,
        ));
    }

    let button_y = actions_top;
    let enter = Rect::new(content_x, button_y, 100, BUTTON_HEIGHT);
    let create = Rect::new(enter.right() + 8, button_y, 100, BUTTON_HEIGHT);
    let back = Rect::new(create.right() + 8, button_y, 110, BUTTON_HEIGHT);

    let form_y = form_top - i32::try_from(4 * (FIELD_HEIGHT + ROW_GAP)).unwrap_or(120)
        + i32::try_from(FIELD_HEIGHT + ROW_GAP).unwrap_or(30);
    let name_field = Rect::new(content_x, form_y, content_width.min(240), FIELD_HEIGHT);
    let row2 = form_y + i32::try_from(FIELD_HEIGHT + ROW_GAP).unwrap_or(30);
    let class_left = Rect::new(content_x, row2, 22, FIELD_HEIGHT);
    let class_value = Rect::new(class_left.right() + 4, row2, 90, FIELD_HEIGHT);
    let class_right = Rect::new(class_value.right() + 4, row2, 22, FIELD_HEIGHT);
    let row3 = row2 + i32::try_from(FIELD_HEIGHT + ROW_GAP).unwrap_or(30);
    let gender_left = Rect::new(content_x, row3, 22, FIELD_HEIGHT);
    let gender_value = Rect::new(gender_left.right() + 4, row3, 90, FIELD_HEIGHT);
    let gender_right = Rect::new(gender_value.right() + 4, row3, 22, FIELD_HEIGHT);
    let row4 = row3 + i32::try_from(FIELD_HEIGHT + ROW_GAP).unwrap_or(30);
    let confirm = Rect::new(content_x, row4, 100, BUTTON_HEIGHT);
    let cancel = Rect::new(confirm.right() + 8, row4, 80, BUTTON_HEIGHT);

    CharacterLayout {
        panel,
        title: Rect::new(content_x, panel.y + 7, content_width, TITLE_HEIGHT),
        cards,
        enter,
        create,
        back,
        error,
        hint,
        name_field,
        class_left,
        class_value,
        class_right,
        gender_left,
        gender_value,
        gender_right,
        confirm,
        cancel,
    }
}

fn draw_button(frame: &mut Frame, text: &UiText, rect: Rect, label: &str, active: bool) {
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
    text.draw_center(
        frame,
        rect,
        label,
        SIZE_SMALL,
        if active { TITLE_COLOR } else { BODY_COLOR },
    );
}

fn draw_hint(frame: &mut Frame, text: &UiText, rect: Rect, label: &str) {
    text.draw_clipped(frame, rect.x, rect.y, label, SIZE_SMALL, MUTED_COLOR, rect);
}

/// 画登录页。
pub fn draw_server(
    frame: &mut Frame,
    layout: &ServerLayout,
    state: &ServerSelectState,
    text: &UiText,
    language: Language,
) {
    crate::ui::draw_panel(frame, layout.panel);
    text.draw_clipped(
        frame,
        layout.title.x,
        layout.title.y,
        &format!(
            "{} - {}",
            Label::Login.text(language),
            Label::Server.text(language)
        ),
        SIZE_BODY,
        TITLE_COLOR,
        layout.title,
    );
    input::draw(
        frame,
        text,
        layout.address,
        &state.address,
        state.focus == LoginField::Server,
        Label::ServerAddress.text(language),
    );
    input::draw(
        frame,
        text,
        layout.account,
        &state.account,
        state.focus == LoginField::Account,
        Label::Account.text(language),
    );
    input::draw(
        frame,
        text,
        layout.password,
        &state.password,
        state.focus == LoginField::Password,
        Label::Password.text(language),
    );
    let connecting = state.status == ConnectState::Connecting;
    let connect_label = if connecting {
        Label::Connecting.text(language)
    } else if state.error.is_some() {
        Label::Retry.text(language)
    } else {
        Label::Connect.text(language)
    };
    draw_button(frame, text, layout.connect, connect_label, true);
    draw_button(frame, text, layout.quit, Label::Quit.text(language), false);
    if let Some(error) = state.error {
        // 只画本地化类别；原始诊断串（`state.message`）留在日志里，避免英文界面泄漏中文。
        text.draw_clipped(
            frame,
            layout.error.x,
            layout.error.y,
            error_text(error, language),
            SIZE_SMALL,
            [255, 132, 132, 255],
            Rect::new(layout.error.x, layout.error.y, layout.error.width, 14),
        );
    }
    draw_hint(frame, text, layout.hint, Label::LoginHint.text(language));
}

/// 画选角页（含创建表单）。
pub fn draw_characters(
    frame: &mut Frame,
    layout: &CharacterLayout,
    state: &CharacterSelectState,
    text: &UiText,
    language: Language,
) {
    crate::ui::draw_panel(frame, layout.panel);
    text.draw_clipped(
        frame,
        layout.title.x,
        layout.title.y,
        Label::SelectCharacter.text(language),
        SIZE_BODY,
        TITLE_COLOR,
        layout.title,
    );
    if state.characters.is_empty() {
        text.draw_clipped(
            frame,
            layout.title.x,
            layout.title.y + 26,
            Label::NoCharacters.text(language),
            SIZE_SMALL,
            MUTED_COLOR,
            Rect::new(
                layout.title.x,
                layout.title.y + 24,
                layout.title.width,
                layout.title.height,
            ),
        );
    }
    for (index, rect) in layout.cards.iter().enumerate() {
        let Some(card) = state.characters.get(index) else {
            break;
        };
        let selected = index == state.selected;
        frame.fill_rect(
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            if selected {
                [86, 74, 32, 235]
            } else {
                [26, 30, 38, 200]
            },
        );
        if selected {
            frame.fill_rect(rect.x, rect.y, 3, rect.height, GOLD_COLOR);
        }
        text.draw_clipped(
            frame,
            rect.x + 8,
            rect.y + 4,
            &format!("{} ({})", card.name, card.headline(language)),
            SIZE_BODY,
            if selected { TITLE_COLOR } else { BODY_COLOR },
            Rect::new(rect.x + 8, rect.y + 4, rect.width.saturating_sub(12), 16),
        );
        text.draw_clipped(
            frame,
            rect.x + 8,
            rect.y + 22,
            &card.last_access_text(language),
            SIZE_SMALL,
            MUTED_COLOR,
            Rect::new(rect.x + 8, rect.y + 22, rect.width.saturating_sub(12), 14),
        );
        text.draw_clipped(
            frame,
            rect.x + 8,
            rect.y + 38,
            &card.equipment_text(language),
            SIZE_SMALL,
            MUTED_COLOR,
            Rect::new(rect.x + 8, rect.y + 38, rect.width.saturating_sub(12), 14),
        );
    }
    let has_selection = !state.characters.is_empty();
    draw_button(
        frame,
        text,
        layout.enter,
        Label::EnterGame.text(language),
        has_selection,
    );
    draw_button(
        frame,
        text,
        layout.create,
        Label::CreateCharacter.text(language),
        false,
    );
    draw_button(
        frame,
        text,
        layout.back,
        Label::BackToServer.text(language),
        false,
    );

    if state.creating {
        input::draw(
            frame,
            text,
            layout.name_field,
            &state.create_name,
            true,
            Label::CharacterName.text(language),
        );
        draw_button(frame, text, layout.class_left, "<", false);
        text.draw_center(
            frame,
            layout.class_value,
            labels::class_name(state.create_class, language),
            SIZE_SMALL,
            BODY_COLOR,
        );
        draw_button(frame, text, layout.class_right, ">", false);
        draw_button(frame, text, layout.gender_left, "<", false);
        text.draw_center(
            frame,
            layout.gender_value,
            gender_name(state.create_gender, language),
            SIZE_SMALL,
            BODY_COLOR,
        );
        draw_button(frame, text, layout.gender_right, ">", false);
        draw_button(
            frame,
            text,
            layout.confirm,
            Label::CreateCharacter.text(language),
            state.can_create(),
        );
        draw_button(
            frame,
            text,
            layout.cancel,
            Label::Close.text(language),
            false,
        );
    }

    if let Some(error) = state.error {
        // 同服务器选择页：只画本地化类别，原始诊断串不进 UI。
        text.draw_clipped(
            frame,
            layout.error.x,
            layout.error.y,
            error_text(error, language),
            SIZE_SMALL,
            [255, 132, 132, 255],
            Rect::new(layout.error.x, layout.error.y, layout.error.width, 14),
        );
    }
    draw_hint(
        frame,
        text,
        layout.hint,
        Label::CharSelectHint.text(language),
    );
}

/// 登录页点击结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerAction {
    /// 连接（失败后再点就是重试）。
    Connect,
    /// 退出客户端。
    Quit,
    /// 焦点切到某个输入框。
    Focus(LoginField),
}

/// 处理登录页点击。
pub fn server_click(
    layout: &ServerLayout,
    state: &mut ServerSelectState,
    point: Point,
) -> Option<ServerAction> {
    if !layout.panel.contains(point) {
        return None;
    }
    if layout.quit.contains(point) {
        return Some(ServerAction::Quit);
    }
    if layout.connect.contains(point) {
        if !state.can_connect() {
            return None;
        }
        state.begin_connect();
        return Some(ServerAction::Connect);
    }
    for (rect, field) in [
        (layout.address, LoginField::Server),
        (layout.account, LoginField::Account),
        (layout.password, LoginField::Password),
    ] {
        if rect.contains(point) {
            state.focus = field;
            return Some(ServerAction::Focus(field));
        }
    }
    None
}

/// 选角页点击结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterAction {
    /// 选中某张角色卡。
    Select(usize),
    /// 进入游戏（角色槽位序号）。
    Enter(i32),
    /// 展开/收起创建表单。
    ToggleCreate,
    /// 提交创建（名字/职业/性别）。
    Create,
    /// 返回服务器选择。
    Back,
    /// 职业左右切换（`-1` / `+1`）。
    CycleClass(i32),
    /// 性别左右切换。
    CycleGender(i32),
}

/// 处理选角页点击（就地改 `state`）。
pub fn character_click(
    layout: &CharacterLayout,
    state: &mut CharacterSelectState,
    point: Point,
) -> Option<CharacterAction> {
    if !layout.panel.contains(point) {
        return None;
    }
    if layout.back.contains(point) {
        return Some(CharacterAction::Back);
    }
    if layout.create.contains(point) {
        state.creating = !state.creating;
        state.clear_error();
        return Some(CharacterAction::ToggleCreate);
    }
    if state.creating {
        if layout.cancel.contains(point) {
            state.creating = false;
            return Some(CharacterAction::ToggleCreate);
        }
        if layout.confirm.contains(point) && state.can_create() {
            return Some(CharacterAction::Create);
        }
        if layout.class_left.contains(point) {
            state.create_class = cycle_class(state.create_class, -1);
            return Some(CharacterAction::CycleClass(-1));
        }
        if layout.class_right.contains(point) {
            state.create_class = cycle_class(state.create_class, 1);
            return Some(CharacterAction::CycleClass(1));
        }
        if layout.gender_left.contains(point) {
            state.create_gender = cycle_gender(state.create_gender, -1);
            return Some(CharacterAction::CycleGender(-1));
        }
        if layout.gender_right.contains(point) {
            state.create_gender = cycle_gender(state.create_gender, 1);
            return Some(CharacterAction::CycleGender(1));
        }
    }
    if layout.enter.contains(point) {
        let index = state.current()?.index;
        return Some(CharacterAction::Enter(index));
    }
    for (index, rect) in layout.cards.iter().enumerate() {
        if rect.contains(point) {
            state.selected = index;
            state.clear_error();
            return Some(CharacterAction::Select(index));
        }
    }
    None
}

/// 职业轮换（跳过不存在的职业，按 `MirClass::ALL` 顺序）。
#[must_use]
pub fn cycle_class(class: MirClass, delta: i32) -> MirClass {
    let all = MirClass::ALL;
    let index = all.iter().position(|value| *value == class).unwrap_or(0);
    let len = i32::try_from(all.len()).unwrap_or(1);
    let next = (i32::try_from(index).unwrap_or(0) + delta).rem_euclid(len);
    all[usize::try_from(next).unwrap_or(0)]
}

/// 性别轮换。
#[must_use]
pub const fn cycle_gender(gender: MirGender, delta: i32) -> MirGender {
    if delta == 0 {
        return gender;
    }
    match gender {
        MirGender::Male => MirGender::Female,
        MirGender::Female => MirGender::Male,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: u32, height: u32) -> Rect {
        Rect::screen(width, height)
    }

    fn info(index: i32, name: &str, level: u16, class: MirClass) -> CharacterInfo {
        CharacterInfo {
            index,
            name: name.to_owned(),
            level,
            class,
            gender: MirGender::Male,
            last_access: DateTimeBinary::default(),
        }
    }

    #[test]
    fn server_select_defaults_to_localhost_and_remembers_a_saved_address() {
        let state = ServerSelectState::default();
        assert_eq!(state.address.value, "127.0.0.1:7000");
        assert!(!state.can_connect(), "账号/口令没填时不能连接");
        let state = ServerSelectState::with_server("10.0.0.5:7000");
        assert_eq!(state.address.value, "10.0.0.5:7000");
        let state = ServerSelectState::with_server("   ");
        assert_eq!(state.address.value, DEFAULT_SERVER);
    }

    #[test]
    fn failure_shows_a_retry_button_and_retry_clears_the_error() {
        let layout = server_layout(frame(800, 600));
        let mut state = ServerSelectState::default();
        state.account.set("hero");
        state.password.set("secret");
        state.error = Some(ConnectError::Unreachable);
        // 失败后按钮文案是「重试」：再点一次会重新进入连接中。
        let action = server_click(
            &layout,
            &mut state,
            Point::new(layout.connect.x + 2, layout.connect.y + 2),
        );
        assert_eq!(action, Some(ServerAction::Connect));
        assert_eq!(state.status, ConnectState::Connecting);
        assert!(state.error.is_none(), "重试要先清掉旧错误");
        // 连接中再点不会重复发请求。
        assert_eq!(
            server_click(
                &layout,
                &mut state,
                Point::new(layout.connect.x + 2, layout.connect.y + 2)
            ),
            None
        );
        state.fail(ConnectError::Timeout, Some("超时".to_owned()));
        assert_eq!(state.error, Some(ConnectError::Timeout));
        assert_eq!(error_label(ConnectError::Timeout), Label::ConnectTimeout);
        // 原始诊断只保留在状态里（日志/测试可取），不进 UI 文案。
        assert_eq!(state.message.as_deref(), Some("超时"));
    }

    /// 英文提示里不能出现 CJK（避免把中文诊断串漏给英文玩家）。
    fn has_cjk(text: &str) -> bool {
        text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
    }

    #[test]
    fn error_text_is_localized_and_never_forwards_raw_diagnostics() {
        for error in [
            ConnectError::Unreachable,
            ConnectError::Credentials,
            ConnectError::Refused,
            ConnectError::Timeout,
            ConnectError::Version,
            ConnectError::Other,
        ] {
            let zh = error_text(error, Language::ZhCn);
            let en = error_text(error, Language::En);
            assert!(!zh.is_empty(), "{error:?} 缺中文提示");
            assert!(!en.is_empty(), "{error:?} 缺英文提示");
            assert!(!has_cjk(en), "{error:?} 英文提示混进中文：{en}");
        }
        // 原始 anyhow 文本仍完整保留在诊断字段里（只由窗口层 eprintln）。
        let raw = "无法连接服务器 127.0.0.1:7000：连接被拒绝（os error 10061）";
        let mut state = ServerSelectState::default();
        state.fail(ConnectError::Unreachable, Some(raw.to_owned()));
        assert_eq!(state.message.as_deref(), Some(raw));
        assert!(!error_text(ConnectError::Unreachable, Language::En).contains("127.0.0.1"));
    }

    #[test]
    fn clicking_fields_changes_the_focus() {
        let layout = server_layout(frame(800, 600));
        let mut state = ServerSelectState::default();
        assert_eq!(
            server_click(
                &layout,
                &mut state,
                Point::new(layout.password.x + 2, layout.password.y + 2)
            ),
            Some(ServerAction::Focus(LoginField::Password))
        );
        assert_eq!(state.focus, LoginField::Password);
    }

    #[test]
    fn server_layout_fits_small_and_large_screens() {
        for (width, height) in [(320, 240), (800, 600)] {
            let screen = frame(width, height);
            let layout = server_layout(screen);
            assert!(layout.panel.right() <= screen.right());
            assert!(layout.panel.bottom() <= screen.bottom());
            for rect in [
                layout.address,
                layout.account,
                layout.password,
                layout.connect,
                layout.quit,
                layout.hint,
            ] {
                assert!(layout.panel.contains(Point::new(rect.x, rect.y)));
            }
        }
    }

    #[test]
    fn character_cards_show_class_level_and_equipment_summary() {
        let mut state = CharacterSelectState::default();
        state
            .equipment_cache
            .insert("张三".to_owned(), "木剑 / 布衣".to_owned());
        state.set_characters(&[
            info(0, "张三", 22, MirClass::Warrior),
            info(1, "李四", 3, MirClass::Taoist),
        ]);
        assert_eq!(state.characters.len(), 2);
        assert!(!state.creating, "有角色时不展开创建表单");
        let card = &state.characters[0];
        assert_eq!(card.headline(Language::ZhCn), "战士 · 等级 22 · 男");
        assert_eq!(card.equipment_text(Language::ZhCn), "装备: 木剑 / 布衣");
        assert!(card.last_access_text(Language::ZhCn).contains("从未登录"));
        assert_eq!(
            state.characters[1].equipment_text(Language::ZhCn),
            "装备: 暂无装备"
        );
        assert_eq!(
            state.characters[1].headline(Language::En),
            "Taoist · Lv 3 · Male"
        );
    }

    #[test]
    fn empty_character_list_guides_creation_and_creates_with_class_and_gender() {
        let layout = character_layout(frame(800, 600), 0, true);
        let mut state = CharacterSelectState::default();
        state.set_characters(&[]);
        assert!(state.creating, "空角色列表要引导创建");
        assert!(!state.can_create(), "没填名字不能创建");

        state.create_name.set("新人");
        assert!(state.can_create());
        assert_eq!(
            character_click(
                &layout,
                &mut state,
                Point::new(layout.class_right.x + 2, layout.class_right.y + 2)
            ),
            Some(CharacterAction::CycleClass(1))
        );
        assert_eq!(state.create_class, MirClass::Wizard);
        assert_eq!(
            character_click(
                &layout,
                &mut state,
                Point::new(layout.gender_right.x + 2, layout.gender_right.y + 2)
            ),
            Some(CharacterAction::CycleGender(1))
        );
        assert_eq!(state.create_gender, MirGender::Female);
        assert_eq!(
            character_click(
                &layout,
                &mut state,
                Point::new(layout.confirm.x + 2, layout.confirm.y + 2)
            ),
            Some(CharacterAction::Create)
        );
    }

    #[test]
    fn selecting_and_entering_a_character_returns_its_slot() {
        let layout = character_layout(frame(800, 600), 2, false);
        let mut state = CharacterSelectState::default();
        state.set_characters(&[
            info(0, "张三", 22, MirClass::Warrior),
            info(3, "李四", 9, MirClass::Archer),
        ]);
        assert_eq!(
            character_click(
                &layout,
                &mut state,
                Point::new(layout.cards[1].x + 2, layout.cards[1].y + 2)
            ),
            Some(CharacterAction::Select(1))
        );
        assert_eq!(
            character_click(
                &layout,
                &mut state,
                Point::new(layout.enter.x + 2, layout.enter.y + 2)
            ),
            Some(CharacterAction::Enter(3))
        );
    }

    #[test]
    fn character_layout_fits_small_and_large_screens() {
        for (width, height) in [(320, 240), (800, 600)] {
            for creating in [false, true] {
                let screen = frame(width, height);
                let layout = character_layout(screen, 3, creating);
                assert!(layout.panel.right() <= screen.right());
                assert!(layout.panel.bottom() <= screen.bottom());
                for rect in [layout.enter, layout.create, layout.back, layout.hint] {
                    assert!(layout.panel.contains(Point::new(rect.x, rect.y)));
                }
                for rect in &layout.cards {
                    assert!(layout.panel.contains(Point::new(rect.x, rect.y)));
                    assert!(rect.bottom() <= layout.panel.bottom());
                }
            }
        }
    }

    #[test]
    fn class_cycle_wraps_around() {
        assert_eq!(cycle_class(MirClass::Warrior, 1), MirClass::Wizard);
        assert_eq!(cycle_class(MirClass::Warrior, -1), MirClass::Archer);
        assert_eq!(cycle_gender(MirGender::Female, 1), MirGender::Male);
    }
}
