//! 邮件面板（`N`；Crystal `MailListDialog` / `MailDialogs.cs:9`）。
//!
//! 数据来自 `S.ReceiveMail`（服务端每次阅读/领取/锁定/删除后都会重发整份列表）：
//! 发件人、正文、日期、附带金币与物品、已读/锁定/已领取标志。
//!
//! 页面：
//! * **收件箱**：10 行一页（Crystal `Rows = new MailItemRow[10]`，`MailDialogs.cs:26`），
//!   列出「类型图标 / 发件人 / 主题 / 日期」；点击一行会发 `C.ReadMail`（Crystal 在
//!   `MailReadParcelDialog.ReadMail` 里发，`MailDialogs.cs:1098`）并显示正文与附件；
//! * **阅读**：正文 + 附件图标 + 按钮「收取附件 / 锁定 / 删除」；
//! * **写信**：收件人 / 主题 / 正文 / 金币 / 贴邮票 / 5 个附件格 + 邮费，发
//!   `C.SendMail`；「询价」发 `C.MailCost`，回执是 `S.MailCost`。
//!
//! 与 Crystal / 协议不一致的地方（都有 `简化` 注释）：
//! * **主题**：Crystal 的 `ClientMail` 没有主题字段（`Shared/Data/ClientData.cs:154`），
//!   服务端 `mail` 表的 `subject` 列也不上线。列表的「主题」列显示正文首行；
//!   写信页仍按需求提供主题输入框，但 `C.SendMail` 没有对应字段，发送时忽略。
//! * **附件选择**：Crystal 从背包拖 `MirItemCell` 到 5 个邮寄格
//!   （`MailDialogs.cs:889-893`）；本作改成「点背包物品切换附件」，避免再扩一套拖拽目标。
//! * **邮件类型**：Crystal 用 `Type` 列区分「信 / 包裹」图标；本作用一个小色块
//!   （金色 = 带附件，灰色 = 普通信），没有图库素材。
//! * **回复**：`C.SendMail` 只是新写一封，Crystal 的「回复」按钮只是预填收件人
//!   （`MailDialogs.cs:189-219`），本作没做预填（可在收件人框里自己填）。
//! * 没有「黑名单 / 举报」按钮（Crystal 有，本作没有对应系统）。
//! * 日期按 `MM-DD HH:MM` 显示（Crystal 用「今天/昨天/N 天前」文案，
//!   `MailDialogs.cs` 的 `DaysAgo/Today/Yesterday`）；纯 UTC 计算，不做时区换算。

use mir_core::{DateTimeBinary, Language, Point};
use mir_render::{Frame, Image};

use crate::geom::Rect;
use crate::net::{MailView, WorldSnapshot};
use crate::ui::grid::{self, GridLayout};
use crate::ui::hud::{BODY_COLOR, GOLD_COLOR, MUTED_COLOR, SIZE_BODY, SIZE_SMALL, TITLE_COLOR};
use crate::ui::input::{self, TextField};
use crate::ui::labels::{self, Label};
use crate::ui::{PanelContext, UiText};

/// 面板宽度。
pub const PANEL_WIDTH: u32 = 620;
/// 面板高度。
pub const PANEL_HEIGHT: u32 = 452;
/// 内边距。
pub const PADDING: u32 = 10;
/// 标题高度。
pub const TITLE_HEIGHT: u32 = 24;
/// 页签行高度。
pub const TABS_HEIGHT: u32 = 26;
/// 列表表头高度。
pub const LIST_HEADER_HEIGHT: u32 = 16;
/// 每行邮件高度。
pub const ROW_HEIGHT: u32 = 21;
/// 一页多少封邮件（Crystal `Rows = new MailItemRow[10]`）。
pub const LIST_ROWS: usize = 10;
/// 正文详情区高度。
pub const DETAIL_HEIGHT: u32 = 104;
/// 底部按钮行高度。
pub const BUTTON_HEIGHT: u32 = 24;
/// 附件格边长（写信页 5 格）。
pub const ATTACHMENT_SIZE: u32 = 34;
/// 写信页背包挑选区的格子边长。
pub const PICK_CELL: u32 = 22;
/// 写信页背包挑选区的间距。
pub const PICK_GAP: u32 = 2;
/// 写信页背包挑选区列数。
pub const PICK_COLUMNS: usize = 10;
/// 邮件最多几个附件（`C.SendMail.ItemsIdx` 固定 5 项）。
pub const MAX_ATTACHMENTS: usize = 5;
/// 邮件正文字数上限（服务端 `MAIL_TEXT_MAX = 500`）。
pub const MESSAGE_MAX_CHARS: usize = 500;
/// 收件人名字最多多少字（服务端角色名上限 20）。
pub const RECIPIENT_MAX_CHARS: usize = 20;
/// 主题输入框上限（只用于本地显示，见模块文档）。
pub const SUBJECT_MAX_CHARS: usize = 40;

/// 邮件面板的两个页签。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MailTab {
    /// 收件箱。
    #[default]
    Inbox,
    /// 写信。
    Compose,
}

impl MailTab {
    /// 页签名。
    #[must_use]
    pub const fn title(self, language: Language) -> &'static str {
        match self {
            Self::Inbox => Label::Inbox.text(language),
            Self::Compose => Label::Compose.text(language),
        }
    }
}

/// 写信页的状态（三个文本字段 + 附件 + 邮费）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailCompose {
    /// 收件人。
    pub recipient: TextField,
    /// 主题（**不发送**，见模块文档）。
    pub subject: TextField,
    /// 正文。
    pub message: TextField,
    /// 附带金币。
    pub gold: TextField,
    /// 附件物品唯一编号（`None` = 空；`0` 也视为空，与 `C.SendMail.ItemsIdx` 一致）。
    pub attachments: [Option<u64>; MAX_ATTACHMENTS],
    /// 是否贴邮票。
    pub stamped: bool,
}

impl Default for MailCompose {
    fn default() -> Self {
        Self {
            recipient: TextField::new(RECIPIENT_MAX_CHARS),
            subject: TextField::new(SUBJECT_MAX_CHARS),
            message: TextField::new(MESSAGE_MAX_CHARS),
            gold: TextField::numeric(9),
            attachments: [None; MAX_ATTACHMENTS],
            stamped: false,
        }
    }
}

impl MailCompose {
    /// 清空表单（发送成功后调用）。
    pub fn clear(&mut self) {
        self.recipient.set("");
        self.subject.set("");
        self.message.set("");
        self.gold.set("");
        self.attachments = [None; MAX_ATTACHMENTS];
        self.stamped = false;
    }

    /// `C.SendMail.ItemsIdx`：固定 5 项，空格写 `0`。
    #[must_use]
    pub fn items_idx(&self) -> [u64; MAX_ATTACHMENTS] {
        let mut out = [0u64; MAX_ATTACHMENTS];
        for (index, item) in self.attachments.iter().enumerate() {
            if let Some(unique_id) = item {
                out[index] = *unique_id;
            }
        }
        out
    }

    /// 把一个背包物品挂成附件（已在附件里则取消）；附件满时忽略。
    ///
    /// 返回 `true` 表示附件列表发生了变化。
    pub fn toggle_attachment(&mut self, unique_id: u64) -> bool {
        if let Some(index) = self
            .attachments
            .iter()
            .position(|slot| *slot == Some(unique_id))
        {
            self.attachments[index] = None;
            return true;
        }
        if let Some(index) = self.attachments.iter().position(Option::is_none) {
            self.attachments[index] = Some(unique_id);
            return true;
        }
        false
    }

    /// 附件个数。
    #[must_use]
    pub fn attachment_count(&self) -> usize {
        self.attachments
            .iter()
            .filter(|slot| slot.is_some())
            .count()
    }
}

/// 邮件面板布局（收件箱与写信页共用同一块面板，矩形按页签给出）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailLayout {
    /// 面板矩形。
    pub panel: Rect,
    /// 收件箱页签。
    pub inbox_tab: Rect,
    /// 写信页签。
    pub compose_tab: Rect,
    /// 列表表头。
    pub list_header: Rect,
    /// 可见邮件行（下标 `0` 对应 `first_index`）。
    pub rows: Vec<Rect>,
    /// 本页第一封邮件的下标。
    pub first_index: usize,
    /// 正文详情区。
    pub detail: Rect,
    /// 上一页。
    pub prev: Rect,
    /// 下一页。
    pub next: Rect,
    /// 收取附件。
    pub collect: Rect,
    /// 锁定/解锁。
    pub lock: Rect,
    /// 删除。
    pub delete: Rect,
    /// 收件人输入框。
    pub recipient: Rect,
    /// 主题输入框。
    pub subject: Rect,
    /// 金币输入框。
    pub gold: Rect,
    /// 正文输入框。
    pub message: Rect,
    /// 贴邮票开关。
    pub stamp: Rect,
    /// 5 个附件格。
    pub attachments: [Rect; MAX_ATTACHMENTS],
    /// 写信页的背包挑选网格。
    pub pick_grid: GridLayout,
    /// 询价按钮。
    pub quote: Rect,
    /// 发送按钮。
    pub send: Rect,
    /// 返回收件箱按钮。
    pub back: Rect,
}

impl MailLayout {
    /// 命中测试：点在页签上（`0` = 收件箱，`1` = 写信）。
    #[must_use]
    pub fn tab_at(&self, point: Point) -> Option<usize> {
        if self.inbox_tab.contains(point) {
            Some(0)
        } else if self.compose_tab.contains(point) {
            Some(1)
        } else {
            None
        }
    }

    /// 命中测试：点在哪个邮件行上（返回绝对下标）。
    #[must_use]
    pub fn row_at(&self, point: Point) -> Option<usize> {
        self.rows
            .iter()
            .position(|rect| rect.contains(point))
            .map(|index| self.first_index + index)
    }
}

/// 邮件页数（至少 1）。
#[must_use]
pub fn page_count(mail_count: usize) -> usize {
    mail_count.div_ceil(LIST_ROWS).max(1)
}

/// 计算邮件面板布局。
#[must_use]
pub fn mail_layout(
    frame: Rect,
    _tab: MailTab,
    mail_count: usize,
    page: usize,
    inventory_len: usize,
) -> MailLayout {
    let width = PANEL_WIDTH.min(frame.width.saturating_sub(24)).max(320);
    let height = PANEL_HEIGHT.min(frame.height.saturating_sub(24)).max(180);
    let x =
        frame.x + (i32::try_from(frame.width).unwrap_or(0) - i32::try_from(width).unwrap_or(0)) / 2;
    // 与其它面板一样落在状态栏下方（Crystal 的邮件窗口是可拖动窗口，本作固定）。
    let y = frame.y + 60;
    let panel = Rect::new(x.max(4), y, width, height);
    let content_x = panel.x + i32::try_from(PADDING).unwrap_or(0);
    let content_width = width.saturating_sub(PADDING * 2);
    let tabs_y = panel.y + i32::try_from(TITLE_HEIGHT + 2).unwrap_or(0);
    let inbox_tab = Rect::new(content_x, tabs_y, 76, TABS_HEIGHT - 4);
    let compose_tab = Rect::new(content_x + 82, tabs_y, 76, TABS_HEIGHT - 4);
    let content_top = tabs_y + i32::try_from(TABS_HEIGHT).unwrap_or(0);

    let visible_rows = mail_count.min(LIST_ROWS);
    let first_index = page
        .saturating_mul(LIST_ROWS)
        .min((page_count(mail_count) - 1) * LIST_ROWS);
    let list_header = Rect::new(content_x, content_top, content_width, LIST_HEADER_HEIGHT);
    let mut rows = Vec::with_capacity(visible_rows);
    for index in 0..visible_rows {
        rows.push(Rect::new(
            content_x,
            content_top
                + i32::try_from(LIST_HEADER_HEIGHT).unwrap_or(0)
                + i32::try_from(index).unwrap_or(0) * i32::try_from(ROW_HEIGHT).unwrap_or(0),
            content_width,
            ROW_HEIGHT,
        ));
    }
    let list_bottom = content_top
        + i32::try_from(LIST_HEADER_HEIGHT).unwrap_or(0)
        + i32::try_from(LIST_ROWS).unwrap_or(0) * i32::try_from(ROW_HEIGHT).unwrap_or(0);
    let detail = Rect::new(content_x, list_bottom + 4, content_width, DETAIL_HEIGHT);
    let button_y = panel.bottom() - i32::try_from(BUTTON_HEIGHT + 6).unwrap_or(0);
    let prev = Rect::new(content_x, button_y, 60, BUTTON_HEIGHT);
    let next = Rect::new(content_x + 126, button_y, 60, BUTTON_HEIGHT);
    let action_width = 96;
    let delete = Rect::new(
        panel.right() - i32::try_from(PADDING + action_width).unwrap_or(0),
        button_y,
        action_width,
        BUTTON_HEIGHT,
    );
    let lock = Rect::new(
        delete.x - i32::try_from(action_width + 6).unwrap_or(0),
        button_y,
        action_width,
        BUTTON_HEIGHT,
    );
    let collect = Rect::new(
        lock.x - i32::try_from(action_width + 6).unwrap_or(0),
        button_y,
        action_width,
        BUTTON_HEIGHT,
    );

    // 写信页的字段区（同一块面板，页签切换时复用这些矩形）。
    let field_x = content_x + 70;
    let field_width = content_width.saturating_sub(70);
    let field = |row: u32| {
        Rect::new(
            field_x,
            content_top + i32::try_from(row * 28).unwrap_or(0),
            field_width,
            22,
        )
    };
    let recipient = field(0);
    let subject = field(1);
    let gold = Rect::new(field_x, content_top + 56, 120, 22);
    let stamp = Rect::new(field_x + 128, content_top + 56, 92, 22);
    let message = field(3);
    let mut attachments = [Rect::default(); MAX_ATTACHMENTS];
    for (index, slot) in attachments.iter_mut().enumerate() {
        *slot = Rect::new(
            field_x
                + i32::try_from(index).unwrap_or(0)
                    * i32::try_from(ATTACHMENT_SIZE + 4).unwrap_or(0),
            content_top + 112,
            ATTACHMENT_SIZE,
            ATTACHMENT_SIZE,
        );
    }
    let pick_area = Rect::new(content_x, content_top + 152, content_width, 118);
    let pick_grid = grid::grid_layout(
        pick_area,
        inventory_len.max(1),
        PICK_COLUMNS,
        PICK_CELL,
        PICK_GAP,
        0,
    );
    let quote = Rect::new(content_x, button_y, 84, BUTTON_HEIGHT);
    let send = Rect::new(content_x + 92, button_y, 84, BUTTON_HEIGHT);
    let back = Rect::new(
        panel.right() - i32::try_from(PADDING + 84).unwrap_or(0),
        button_y,
        84,
        BUTTON_HEIGHT,
    );

    MailLayout {
        panel,
        inbox_tab,
        compose_tab,
        list_header,
        rows,
        first_index,
        detail,
        prev,
        next,
        collect,
        lock,
        delete,
        recipient,
        subject,
        gold,
        message,
        stamp,
        attachments,
        pick_grid,
        quote,
        send,
        back,
    }
}

/// 邮件日期文案（`MM-DD HH:MM`，UTC；`DateTime.MinValue` 显示 `--`）。
#[must_use]
pub fn format_date(date: DateTimeBinary) -> String {
    if date.is_unset() {
        return "--".to_owned();
    }
    let seconds = date.to_unix_seconds().max(0);
    let days = seconds / 86_400;
    let minutes_of_day = (seconds % 86_400) / 60;
    let (_, month, day) = civil_from_days(days);
    format!(
        "{month:02}-{day:02} {:02}:{:02}",
        minutes_of_day / 60,
        minutes_of_day % 60
    )
}

/// 民用历换算（Howard Hinnant 的 `civil_from_days`）：返回 `(年, 月, 日)`。
///
/// `ui::market` 的到期时间也用它（同一套无依赖的日期换算）。
pub(crate) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// 列表行的「主题」：正文首行（协议没有主题字段）；没有正文时按语言给占位文案。
#[must_use]
pub fn subject_text(mail: &MailView, language: Language) -> String {
    let line = mail.subject_line();
    if line.is_empty() {
        Label::NoBody.text(language).to_owned()
    } else {
        line.to_owned()
    }
}

/// 附件摘要：`附件 2 件 · 500 金币`。
#[must_use]
pub fn attachment_text(mail: &MailView, language: Language) -> String {
    if !mail.is_parcel() {
        return String::new();
    }
    let mut parts = Vec::new();
    if !mail.items.is_empty() {
        parts.push(format!(
            "{} {}",
            Label::Attachment.text(language),
            mail.items.len()
        ));
    }
    if mail.gold > 0 {
        parts.push(labels::gold_text(language, mail.gold));
    }
    parts.join(" · ")
}

/// 画邮件面板。
#[allow(clippy::too_many_arguments)]
pub fn draw(
    frame: &mut Frame,
    layout: &MailLayout,
    context: &PanelContext<'_>,
    tab: MailTab,
    page: usize,
    selected: Option<u64>,
    compose: &MailCompose,
    focus: Option<crate::ui::FieldId>,
    mouse: Point,
    icon: &mut dyn FnMut(u16) -> Option<Image>,
) {
    let text = context.text;
    let language = context.language;
    let snapshot = context.snapshot;
    crate::ui::draw_panel(frame, layout.panel);
    text.draw_clipped(
        frame,
        layout.panel.x + 10,
        layout.panel.y + 4,
        &format!("{} (N)", Label::Mail.text(language)),
        SIZE_BODY,
        TITLE_COLOR,
        Rect::new(
            layout.panel.x,
            layout.panel.y,
            layout.panel.width,
            TITLE_HEIGHT,
        ),
    );
    draw_tab(
        frame,
        text,
        layout.inbox_tab,
        Label::Inbox.text(language),
        tab == MailTab::Inbox,
        layout.inbox_tab.contains(mouse),
    );
    draw_tab(
        frame,
        text,
        layout.compose_tab,
        Label::Compose.text(language),
        tab == MailTab::Compose,
        layout.compose_tab.contains(mouse),
    );
    match tab {
        MailTab::Inbox => draw_inbox(
            frame, text, layout, snapshot, page, selected, mouse, icon, language,
        ),
        MailTab::Compose => draw_compose(
            frame, text, layout, snapshot, compose, focus, mouse, icon, language,
        ),
    }
}

/// 画一个页签按钮。
fn draw_tab(
    frame: &mut Frame,
    text: &UiText,
    rect: Rect,
    label: &str,
    active: bool,
    hovered: bool,
) {
    let fill = if active {
        [86, 74, 32, 240]
    } else if hovered {
        [52, 64, 84, 240]
    } else {
        [32, 38, 48, 220]
    };
    frame.fill_rect(rect.x, rect.y, rect.width, rect.height, fill);
    frame.fill_rect(rect.x, rect.y, rect.width, 1, [110, 124, 146, 255]);
    text.draw_center(
        frame,
        rect,
        label,
        SIZE_SMALL,
        if active {
            [255, 232, 160, 255]
        } else {
            BODY_COLOR
        },
    );
}

/// 画一个普通按钮。
fn draw_button(
    frame: &mut Frame,
    text: &UiText,
    rect: Rect,
    label: &str,
    mouse: Point,
    enabled: bool,
) {
    let hovered = enabled && rect.contains(mouse);
    let fill = if !enabled {
        [34, 38, 44, 220]
    } else if hovered {
        [92, 82, 42, 255]
    } else {
        [44, 52, 66, 255]
    };
    frame.fill_rect(rect.x, rect.y, rect.width, rect.height, fill);
    frame.fill_rect(rect.x, rect.y, rect.width, 1, [140, 156, 180, 255]);
    frame.fill_rect(rect.x, rect.bottom() - 1, rect.width, 1, [36, 44, 56, 255]);
    text.draw_center(
        frame,
        rect,
        label,
        SIZE_SMALL,
        if enabled {
            [255, 240, 180, 255]
        } else {
            [120, 126, 134, 255]
        },
    );
}

/// 收件箱页。
#[allow(clippy::too_many_arguments)]
fn draw_inbox(
    frame: &mut Frame,
    text: &UiText,
    layout: &MailLayout,
    snapshot: &WorldSnapshot,
    page: usize,
    selected: Option<u64>,
    mouse: Point,
    icon: &mut dyn FnMut(u16) -> Option<Image>,
    language: Language,
) {
    // 表头：类型 / 发件人 / 主题 / 日期。
    let header = layout.list_header;
    text.draw_clipped(
        frame,
        header.x + 24,
        header.y,
        Label::Sender.text(language),
        SIZE_SMALL,
        MUTED_COLOR,
        Rect::new(header.x + 24, header.y, 140, header.height),
    );
    text.draw_clipped(
        frame,
        header.x + 170,
        header.y,
        Label::Subject.text(language),
        SIZE_SMALL,
        MUTED_COLOR,
        Rect::new(header.x + 170, header.y, 240, header.height),
    );
    text.draw_right(
        frame,
        header.right() - 4,
        header.y,
        Label::Date.text(language),
        SIZE_SMALL,
        MUTED_COLOR,
    );
    if snapshot.mails.is_empty() {
        text.draw_center(
            frame,
            Rect::new(header.x, header.y + 20, header.width, ROW_HEIGHT * 2),
            Label::NoMail.text(language),
            SIZE_BODY,
            MUTED_COLOR,
        );
    }
    for (visible_index, row) in layout.rows.iter().enumerate() {
        let index = layout.first_index + visible_index;
        let Some(mail) = snapshot.mails.get(index) else {
            break;
        };
        let is_selected = selected == Some(mail.mail_id);
        let hovered = row.contains(mouse);
        if is_selected {
            frame.fill_rect(row.x, row.y, row.width, row.height, [86, 74, 32, 235]);
            frame.fill_rect(row.x, row.y, 2, row.height, [255, 216, 96, 255]);
        } else if hovered {
            frame.fill_rect(row.x, row.y, row.width, row.height, [44, 56, 76, 235]);
            frame.fill_rect(row.x, row.y, 2, row.height, [128, 176, 240, 255]);
        } else if visible_index % 2 == 1 {
            frame.fill_rect(row.x, row.y, row.width, row.height, [26, 30, 38, 160]);
        }
        // 类型：包裹金色、普通信灰色；未读在旁边加一个亮点。
        let parcel = mail.is_parcel();
        let color = if parcel {
            [224, 184, 72, 255]
        } else {
            [150, 160, 174, 255]
        };
        frame.fill_rect(row.x + 5, row.y + 6, 9, 9, color);
        if !mail.opened {
            frame.fill_rect(row.x + 15, row.y + 6, 3, 9, [255, 250, 200, 255]);
        }
        text.draw_clipped(
            frame,
            row.x + 24,
            row.y + 3,
            &mail.sender_name,
            SIZE_BODY,
            if mail.opened {
                BODY_COLOR
            } else {
                [255, 250, 200, 255]
            },
            Rect::new(row.x + 24, row.y + 3, 140, SIZE_BODY + 4),
        );
        let mut subject = subject_text(mail, language);
        if mail.locked {
            subject.push_str(" [");
            subject.push_str(Label::Lock.text(language));
            subject.push(']');
        }
        if mail.collected {
            subject.push_str(" [");
            subject.push_str(Label::Collected.text(language));
            subject.push(']');
        }
        text.draw_clipped(
            frame,
            row.x + 174,
            row.y + 3,
            &subject,
            SIZE_BODY,
            BODY_COLOR,
            Rect::new(
                row.x + 174,
                row.y + 3,
                row.width.saturating_sub(320),
                SIZE_BODY + 4,
            ),
        );
        text.draw_right(
            frame,
            row.right() - 4,
            row.y + 4,
            &format_date(mail.date_sent),
            SIZE_SMALL,
            MUTED_COLOR,
        );
    }
    // 页码。
    let pages = page_count(snapshot.mails.len());
    let label = format!("{}/{}", (page + 1).min(pages), pages);
    text.draw_center(
        frame,
        Rect::new(layout.prev.right() + 2, layout.prev.y, 60, BUTTON_HEIGHT),
        &label,
        SIZE_SMALL,
        MUTED_COLOR,
    );
    draw_button(frame, text, layout.prev, "◀", mouse, page > 0);
    draw_button(frame, text, layout.next, "▶", mouse, page + 1 < pages);

    // 详情区：正文 + 附件 + 状态。
    let detail = layout.detail;
    frame.fill_rect(detail.x, detail.y, detail.width, 1, [60, 72, 88, 255]);
    if let Some(mail_id) = selected {
        if let Some(mail) = snapshot.mail(mail_id) {
            let mut y = detail.y + 4;
            for line in mail.message.lines().take(3) {
                text.draw_clipped(
                    frame,
                    detail.x + 4,
                    y,
                    line,
                    SIZE_SMALL,
                    BODY_COLOR,
                    Rect::new(
                        detail.x + 4,
                        y,
                        detail.width.saturating_sub(200),
                        SIZE_SMALL + 4,
                    ),
                );
                y = y.saturating_add(i32::try_from(SIZE_SMALL + 3).unwrap_or(14));
            }
            text.draw_clipped(
                frame,
                detail.x + 4,
                y,
                &attachment_text(mail, language),
                SIZE_SMALL,
                GOLD_COLOR,
                Rect::new(
                    detail.x + 4,
                    y,
                    detail.width.saturating_sub(200),
                    SIZE_SMALL + 4,
                ),
            );
            // 附件图标（最多 5 个，与写信页的 5 格一致）。
            let mut x = detail.right() - 4;
            for item in mail.items.iter().take(MAX_ATTACHMENTS).rev() {
                x -= i32::try_from(ATTACHMENT_SIZE + 2).unwrap_or(0);
                let rect = Rect::new(x, detail.y + 4, ATTACHMENT_SIZE, ATTACHMENT_SIZE);
                frame.fill_rect(rect.x, rect.y, rect.width, rect.height, [0, 0, 0, 255]);
                frame.fill_rect(
                    rect.x + 1,
                    rect.y + 1,
                    rect.width.saturating_sub(2),
                    rect.height.saturating_sub(2),
                    [34, 40, 50, 235],
                );
                if let Some(image) = snapshot.item_image(item.item_index).and_then(&mut *icon) {
                    let scaled = mir_render::scale_to_fit(
                        &image,
                        rect.width.saturating_sub(4),
                        rect.height.saturating_sub(4),
                    );
                    frame.blit(&scaled, rect.x + 2, rect.y + 2);
                }
            }
            // 状态行（锁定/已领取/未读）。
            let mut state = Vec::new();
            if mail.locked {
                state.push(Label::Lock.text(language));
            }
            if mail.collected {
                state.push(Label::Collected.text(language));
            }
            if !mail.opened {
                state.push(Label::Unread.text(language));
            }
            text.draw_right(
                frame,
                detail.right() - 4,
                detail.bottom() - i32::try_from(SIZE_SMALL + 4).unwrap_or(15),
                &state.join(" · "),
                SIZE_SMALL,
                MUTED_COLOR,
            );
        }
    } else {
        text.draw_clipped(
            frame,
            detail.x + 4,
            detail.y + 4,
            Label::MailHint.text(language),
            SIZE_SMALL,
            MUTED_COLOR,
            detail,
        );
    }

    let can_collect = selected
        .and_then(|mail_id| snapshot.mail(mail_id))
        .is_some_and(MailView::can_collect);
    let locked = selected
        .and_then(|mail_id| snapshot.mail(mail_id))
        .is_some_and(|mail| mail.locked);
    draw_button(
        frame,
        text,
        layout.collect,
        Label::Collect.text(language),
        mouse,
        can_collect,
    );
    draw_button(
        frame,
        text,
        layout.lock,
        if locked {
            Label::Unlock.text(language)
        } else {
            Label::Lock.text(language)
        },
        mouse,
        selected.is_some(),
    );
    draw_button(
        frame,
        text,
        layout.delete,
        Label::Delete.text(language),
        mouse,
        selected.is_some() && !locked,
    );
}

/// 写信页。
#[allow(clippy::too_many_arguments)]
fn draw_compose(
    frame: &mut Frame,
    text: &UiText,
    layout: &MailLayout,
    snapshot: &WorldSnapshot,
    compose: &MailCompose,
    focus: Option<crate::ui::FieldId>,
    mouse: Point,
    icon: &mut dyn FnMut(u16) -> Option<Image>,
    language: Language,
) {
    use crate::ui::FieldId;
    let label_x = layout.panel.x + i32::try_from(PADDING).unwrap_or(0);
    let rows = [
        (
            Label::Recipient.text(language),
            layout.recipient,
            FieldId::MailRecipient,
        ),
        (
            Label::Subject.text(language),
            layout.subject,
            FieldId::MailSubject,
        ),
        (Label::Amount.text(language), layout.gold, FieldId::MailGold),
        (
            Label::Message.text(language),
            layout.message,
            FieldId::MailMessage,
        ),
    ];
    for (label, rect, field) in rows {
        text.draw_clipped(
            frame,
            label_x,
            rect.y + 4,
            label,
            SIZE_SMALL,
            MUTED_COLOR,
            Rect::new(label_x, rect.y, 64, 20),
        );
        let value = match field {
            FieldId::MailRecipient => &compose.recipient,
            FieldId::MailSubject => &compose.subject,
            FieldId::MailGold => &compose.gold,
            _ => &compose.message,
        };
        input::draw(
            frame,
            text,
            rect,
            value,
            focus == Some(field),
            if field == FieldId::MailSubject {
                Label::NoSubjectField.text(language)
            } else {
                ""
            },
        );
    }
    // 贴邮票开关。
    let stamp_on = compose.stamped;
    draw_button(
        frame,
        text,
        layout.stamp,
        &format!(
            "{} {}",
            Label::Stamp.text(language),
            if stamp_on { "√" } else { "×" }
        ),
        mouse,
        true,
    );
    // 附件格。
    text.draw_clipped(
        frame,
        label_x,
        layout.attachments[0].y + 8,
        Label::Attachment.text(language),
        SIZE_SMALL,
        MUTED_COLOR,
        Rect::new(label_x, layout.attachments[0].y, 64, 20),
    );
    for (index, rect) in layout.attachments.iter().enumerate() {
        let border = if rect.contains(mouse) {
            [168, 208, 255, 255]
        } else {
            [70, 84, 104, 255]
        };
        frame.fill_rect(rect.x, rect.y, rect.width, rect.height, [0, 0, 0, 255]);
        frame.fill_rect(
            rect.x + 1,
            rect.y + 1,
            rect.width.saturating_sub(2),
            rect.height.saturating_sub(2),
            [30, 36, 46, 235],
        );
        frame.fill_rect(rect.x, rect.y, rect.width, 1, border);
        frame.fill_rect(rect.x, rect.bottom() - 1, rect.width, 1, border);
        frame.fill_rect(rect.x, rect.y, 1, rect.height, border);
        frame.fill_rect(rect.right() - 1, rect.y, 1, rect.height, border);
        let Some(unique_id) = compose.attachments[index] else {
            continue;
        };
        let Some(slot) = snapshot.inventory_slot_of(unique_id) else {
            continue;
        };
        let Some(item) = snapshot.inventory_item(slot) else {
            continue;
        };
        if let Some(image) = snapshot.item_image(item.item_index).and_then(&mut *icon) {
            let scaled = mir_render::scale_to_fit(
                &image,
                rect.width.saturating_sub(4),
                rect.height.saturating_sub(4),
            );
            frame.blit(&scaled, rect.x + 2, rect.y + 2);
        }
        if item.count > 1 {
            text.draw_right(
                frame,
                rect.right() - 2,
                rect.bottom() - 12,
                &format!("×{}", item.count),
                SIZE_SMALL,
                GOLD_COLOR,
            );
        }
    }
    // 背包挑选网格（点击切换附件）。
    text.draw_right(
        frame,
        layout.panel.right() - i32::try_from(PADDING).unwrap_or(0),
        layout.attachments[0].y + 8,
        Label::MailHint.text(language),
        SIZE_SMALL,
        MUTED_COLOR,
    );
    grid::draw(
        frame,
        text,
        &layout.pick_grid,
        &snapshot.inventory,
        snapshot,
        None,
        None,
        None,
        icon,
    );
    // 邮费。
    let cost = snapshot
        .mail_cost
        .map_or_else(|| "--".to_owned(), |cost| labels::gold_text(language, cost));
    text.draw_clipped(
        frame,
        layout.pick_grid.area.right() + 12,
        layout.pick_grid.area.y + 4,
        &format!("{}: {cost}", Label::Postage.text(language)),
        SIZE_BODY,
        GOLD_COLOR,
        Rect::new(
            layout.pick_grid.area.right() + 12,
            layout.pick_grid.area.y + 4,
            u32::try_from((layout.panel.right() - layout.pick_grid.area.right() - 20).max(0))
                .unwrap_or(0),
            SIZE_BODY + 4,
        ),
    );
    let hint = format!(
        "{} {} · {} {}",
        Label::Attachment.text(language),
        compose.attachment_count(),
        Label::Recipient.text(language),
        compose.recipient.value
    );
    text.draw_clipped(
        frame,
        layout.pick_grid.area.right() + 12,
        layout.pick_grid.area.y + 26,
        &hint,
        SIZE_SMALL,
        MUTED_COLOR,
        Rect::new(
            layout.pick_grid.area.right() + 12,
            layout.pick_grid.area.y + 26,
            u32::try_from((layout.panel.right() - layout.pick_grid.area.right() - 20).max(0))
                .unwrap_or(0),
            SIZE_SMALL + 4,
        ),
    );
    draw_button(
        frame,
        text,
        layout.quote,
        Label::Inquiry.text(language),
        mouse,
        true,
    );
    draw_button(
        frame,
        text,
        layout.send,
        Label::Send.text(language),
        mouse,
        !compose.recipient.is_empty(),
    );
    draw_button(
        frame,
        text,
        layout.back,
        Label::Back.text(language),
        mouse,
        true,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(mail_id: u64, locked: bool, parcel: bool) -> MailView {
        MailView {
            mail_id,
            sender_name: "张三".to_owned(),
            message: "你好\n附件是屠龙".to_owned(),
            opened: true,
            locked,
            can_reply: true,
            collected: false,
            date_sent: DateTimeBinary::from_unix_seconds(1_704_067_200),
            gold: if parcel { 500 } else { 0 },
            items: Vec::new(),
        }
    }

    #[test]
    fn inbox_rows_are_hit_testable_and_paged() {
        let frame = Rect::screen(800, 600);
        let layout = mail_layout(frame, MailTab::Inbox, 23, 1, 46);
        assert_eq!(layout.rows.len(), LIST_ROWS);
        assert_eq!(layout.first_index, LIST_ROWS);
        for (index, row) in layout.rows.iter().enumerate() {
            assert!(layout.panel.contains(row.center()));
            assert_eq!(
                layout.row_at(row.center()),
                Some(layout.first_index + index)
            );
        }
        assert_eq!(page_count(23), 3);
        assert_eq!(page_count(0), 1);
        assert_eq!(layout.tab_at(layout.inbox_tab.center()), Some(0));
        assert_eq!(layout.tab_at(layout.compose_tab.center()), Some(1));
    }

    #[test]
    fn compose_fields_and_grid_are_inside_the_panel() {
        let frame = Rect::screen(800, 600);
        let layout = mail_layout(frame, MailTab::Compose, 0, 0, 46);
        for rect in [
            layout.recipient,
            layout.subject,
            layout.gold,
            layout.message,
            layout.stamp,
            layout.quote,
            layout.send,
            layout.back,
        ] {
            assert!(layout.panel.contains(rect.center()), "{rect:?} 应在面板内");
        }
        for slot in layout.attachments {
            assert!(layout.panel.contains(slot.center()));
        }
        assert_eq!(layout.pick_grid.slots.len(), 46);
    }

    #[test]
    fn sent_dates_are_formatted_and_unset_dates_show_a_dash() {
        assert_eq!(
            format_date(DateTimeBinary::from_unix_seconds(1_704_067_200)),
            "01-01 00:00"
        );
        assert_eq!(format_date(DateTimeBinary::default()), "--");
        // 2024-03-05T12:34:00Z
        assert_eq!(
            format_date(DateTimeBinary::from_unix_seconds(1_709_642_040)),
            "03-05 12:34"
        );
    }

    #[test]
    fn compose_attachments_toggle_and_serialize() {
        let mut compose = MailCompose::default();
        assert!(compose.toggle_attachment(7));
        assert!(compose.toggle_attachment(8));
        assert_eq!(compose.attachment_count(), 2);
        assert_eq!(compose.items_idx(), [7, 8, 0, 0, 0]);
        assert!(compose.toggle_attachment(7));
        assert_eq!(compose.attachment_count(), 1);
        assert!(compose.items_idx().contains(&8), "8 号物品应留在附件里");
        assert!(!compose.items_idx().contains(&7));
        compose.clear();
        assert_eq!(compose.attachment_count(), 0);
    }

    #[test]
    fn parcel_text_and_subject_fall_back_to_the_first_line() {
        let letter = mail(1, false, false);
        assert_eq!(subject_text(&letter, Language::ZhCn), "你好");
        assert!(!letter.is_parcel());
        let parcel = mail(2, true, true);
        assert!(parcel.is_parcel());
        assert!(attachment_text(&parcel, Language::ZhCn).contains("金币"));
        assert!(attachment_text(&parcel, Language::ZhCn).contains("500"));
    }
}
