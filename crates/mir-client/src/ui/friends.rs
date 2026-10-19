//! 好友/黑名单面板（键 `F`）。
//!
//! 数据来自 `S.FriendUpdate`(245) 整表下发；操作走客户端包：
//!
//! | 操作 | 包 | 说明 |
//! |---|---|---|
//! | 添加/拉黑 | `C.AddFriend`(127) | `name` + `blocked` |
//! | 删除 | `C.RemoveFriend`(128) | `character_index`（`S.FriendUpdate` 里的 `index`） |
//! | 刷新 | `C.RefreshFriends`(129) | 无字段 |
//! | 改备注 | `C.AddMemo`(130) | `character_index` + `memo`（≤200 字） |
//!
//! 列表行：名字、备注、在线/离线、黑名单标记。空列表给提示。
//! 320×240 下自动压缩高度，按钮与输入框始终在面板内。

use mir_core::Point;

use crate::geom::Rect;
use crate::ui::input::{self, TextField};
use crate::ui::labels::Label;
use crate::ui::{FieldId, PanelContext, draw_panel};
use mir_protocol::data::ClientFriend;
use mir_render::Frame;

/// 面板宽度。
pub const PANEL_WIDTH: u32 = 380;
/// 面板高度。
pub const PANEL_HEIGHT: u32 = 300;
/// 标题高度。
pub const TITLE_HEIGHT: u32 = 24;
/// 内边距。
pub const PADDING: u32 = 10;
/// 行高。
pub const ROW_HEIGHT: u32 = 22;
/// 按钮高度。
pub const BUTTON_HEIGHT: u32 = 22;
/// 底部操作行数（名字/备注/删除+刷新）。
pub const FOOTER_ROWS: u32 = 3;

/// 面板布局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FriendsLayout {
    /// 面板。
    pub panel: Rect,
    /// 好友行。
    pub rows: Vec<Rect>,
    /// 名字输入框。
    pub name_field: Rect,
    /// 黑名单开关。
    pub blocked: Rect,
    /// 「添加」按钮。
    pub add: Rect,
    /// 备注输入框。
    pub memo_field: Rect,
    /// 「保存备注」按钮。
    pub save_memo: Rect,
    /// 「删除」按钮。
    pub remove: Rect,
    /// 「刷新」按钮。
    pub refresh: Rect,
}

impl FriendsLayout {
    /// 点在哪一行上（返回**绝对下标**：`slot + scroll`）。
    #[must_use]
    pub fn row_at(&self, point: Point, scroll: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|rect| rect.contains(point))
            .map(|slot| slot + scroll)
    }
}

/// 计算布局：屏幕越大行越多；320×240 下仍能完整显示输入/按钮。
#[must_use]
pub fn friends_layout(screen: Rect) -> FriendsLayout {
    let width = PANEL_WIDTH.min(screen.width.saturating_sub(16)).max(200);
    let height = PANEL_HEIGHT.min(screen.height.saturating_sub(16)).max(180);
    let x = (screen.right() - i32::try_from(width).unwrap_or(0) - 8).max(screen.x + 8);
    let y = (screen.y + 60).max(screen.y + 8);
    let panel = Rect::new(
        x,
        y.min(screen.bottom() - i32::try_from(height).unwrap_or(0) - 8),
        width,
        height,
    );
    let content_x = panel.x + i32::try_from(PADDING).unwrap_or(0);
    let content_width = panel.width.saturating_sub(PADDING * 2);
    let footer_y = panel.bottom()
        - i32::try_from(BUTTON_HEIGHT * FOOTER_ROWS + 4 * (FOOTER_ROWS + 1)).unwrap_or(0);
    let list_top = panel.y + i32::try_from(TITLE_HEIGHT + 4).unwrap_or(0);
    let list_height = footer_y.saturating_sub(list_top).max(0);
    let visible_rows =
        usize::try_from((list_height / i32::try_from(ROW_HEIGHT).unwrap_or(1)).max(1)).unwrap_or(1);
    let mut rows = Vec::with_capacity(visible_rows);
    for index in 0..visible_rows {
        rows.push(Rect::new(
            content_x,
            list_top + i32::try_from(index).unwrap_or(0) * i32::try_from(ROW_HEIGHT).unwrap_or(0),
            content_width,
            ROW_HEIGHT,
        ));
    }

    let row_y = footer_y;
    let add_width = 64;
    let blocked_width = 60;
    let name_width = content_width.saturating_sub(add_width + blocked_width + 8);
    let name_field = Rect::new(content_x, row_y, name_width, BUTTON_HEIGHT);
    let blocked = Rect::new(name_field.right() + 4, row_y, blocked_width, BUTTON_HEIGHT);
    let add = Rect::new(blocked.right() + 4, row_y, add_width, BUTTON_HEIGHT);

    let row_y = row_y + i32::try_from(BUTTON_HEIGHT + 4).unwrap_or(0);
    let save_width = 84;
    let memo_width = content_width.saturating_sub(save_width + 4);
    let memo_field = Rect::new(content_x, row_y, memo_width, BUTTON_HEIGHT);
    let save_memo = Rect::new(memo_field.right() + 4, row_y, save_width, BUTTON_HEIGHT);

    let row_y = row_y + i32::try_from(BUTTON_HEIGHT + 4).unwrap_or(0);
    let half = (content_width.saturating_sub(8)) / 2;
    let remove = Rect::new(content_x, row_y, half, BUTTON_HEIGHT);
    let refresh = Rect::new(remove.right() + 8, row_y, half, BUTTON_HEIGHT);

    FriendsLayout {
        panel,
        rows,
        name_field,
        blocked,
        add,
        memo_field,
        save_memo,
        remove,
        refresh,
    }
}

/// 画好友面板。
pub fn draw(
    frame: &mut Frame,
    layout: &FriendsLayout,
    context: &PanelContext<'_>,
    friends: &[ClientFriend],
    state: &FriendsState,
    focus: Option<FieldId>,
    mouse: Point,
) {
    let text = context.text;
    let language = context.language;
    draw_panel(frame, layout.panel);
    text.draw_clipped(
        frame,
        layout.panel.x + 10,
        layout.panel.y + 4,
        Label::Friends.text(language),
        16,
        [255, 232, 160, 255],
        Rect::new(
            layout.panel.x,
            layout.panel.y,
            layout.panel.width,
            TITLE_HEIGHT,
        ),
    );

    if friends.is_empty() {
        text.draw_clipped(
            frame,
            layout.panel.x + 12,
            layout.panel.y + i32::try_from(TITLE_HEIGHT + 10).unwrap_or(0),
            Label::FriendEmpty.text(language),
            12,
            [180, 180, 180, 255],
            Rect::new(
                layout.panel.x + 8,
                layout.panel.y + i32::try_from(TITLE_HEIGHT).unwrap_or(0),
                layout.panel.width.saturating_sub(16),
                layout.panel.height.saturating_sub(TITLE_HEIGHT),
            ),
        );
    } else {
        let first = state.scroll.min(friends.len().saturating_sub(1));
        for (slot, rect) in layout.rows.iter().enumerate() {
            let Some(friend) = friends.get(first + slot) else {
                break;
            };
            let selected = state.selected == Some(first + slot);
            let fill = if selected {
                [86, 74, 32, 240]
            } else if rect.contains(mouse) {
                [58, 66, 82, 220]
            } else if slot % 2 == 0 {
                [30, 34, 42, 220]
            } else {
                [24, 28, 36, 220]
            };
            frame.fill_rect(rect.x, rect.y, rect.width, rect.height, fill);
            let online = if friend.online {
                Label::Online.text(language)
            } else {
                Label::Offline.text(language)
            };
            let status_color = if friend.online {
                [120, 220, 120, 255]
            } else {
                [150, 150, 150, 255]
            };
            let name = if friend.blocked {
                format!("{} [{}]", friend.name, Label::FriendBlocked.text(language))
            } else {
                friend.name.clone()
            };
            text.draw_clipped(
                frame,
                rect.x + 6,
                rect.y + 3,
                &name,
                12,
                if friend.blocked {
                    [220, 140, 140, 255]
                } else {
                    [235, 235, 235, 255]
                },
                Rect::new(rect.x, rect.y, rect.width.saturating_sub(70), rect.height),
            );
            text.draw_right(
                frame,
                rect.right() - 6,
                rect.y + 3,
                online,
                11,
                status_color,
            );
            if !friend.memo.is_empty() {
                text.draw_clipped(
                    frame,
                    rect.x + 6,
                    rect.y + 12,
                    &friend.memo,
                    10,
                    [170, 180, 190, 255],
                    Rect::new(rect.x + 4, rect.y + 12, rect.width.saturating_sub(70), 10),
                );
            }
        }
    }

    input::draw(
        frame,
        text,
        layout.name_field,
        &state.name,
        focus == Some(FieldId::FriendName),
        Label::FriendName.text(language),
    );
    let blocked_fill = if state.blocked {
        [120, 80, 50, 255]
    } else {
        [52, 56, 64, 255]
    };
    frame.fill_rect(
        layout.blocked.x,
        layout.blocked.y,
        layout.blocked.width,
        layout.blocked.height,
        blocked_fill,
    );
    text.draw_center(
        frame,
        layout.blocked,
        Label::FriendBlocked.text(language),
        11,
        [235, 235, 235, 255],
    );
    draw_button(
        frame,
        text,
        layout.add,
        Label::FriendAdd.text(language),
        mouse,
    );
    input::draw(
        frame,
        text,
        layout.memo_field,
        &state.memo,
        focus == Some(FieldId::FriendMemo),
        Label::FriendMemo.text(language),
    );
    draw_button(
        frame,
        text,
        layout.save_memo,
        Label::FriendSaveMemo.text(language),
        mouse,
    );
    draw_button(
        frame,
        text,
        layout.remove,
        Label::FriendRemove.text(language),
        mouse,
    );
    draw_button(
        frame,
        text,
        layout.refresh,
        Label::FriendRefresh.text(language),
        mouse,
    );
}

fn draw_button(frame: &mut Frame, text: &crate::ui::UiText, rect: Rect, label: &str, mouse: Point) {
    let fill = if rect.contains(mouse) {
        [92, 82, 42, 255]
    } else {
        [52, 64, 84, 255]
    };
    frame.fill_rect(rect.x, rect.y, rect.width, rect.height, fill);
    text.draw_center(frame, rect, label, 11, [240, 240, 240, 255]);
}

/// 好友面板的界面状态。
#[derive(Debug, Clone)]
pub struct FriendsState {
    /// 选中行（绝对下标）。
    pub selected: Option<usize>,
    /// 列表滚动起点。
    pub scroll: usize,
    /// 名字输入。
    pub name: TextField,
    /// 备注输入。
    pub memo: TextField,
    /// 是否勾选黑名单。
    pub blocked: bool,
}

impl Default for FriendsState {
    fn default() -> Self {
        Self {
            selected: None,
            scroll: 0,
            // `TextField::default()` 的 `max_chars` 是 0（永远输不进字），必须显式给上限。
            name: TextField::new(24),
            memo: TextField::new(200),
            blocked: false,
        }
    }
}
