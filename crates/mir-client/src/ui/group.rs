//! 组队面板（`P`；Crystal `GroupDialog`，`Client/MirScenes/Dialogs/GroupDialog.cs`）。
//!
//! Crystal 的组队窗口只有两列名字 + 地图悬浮提示（`GroupDialog.cs:28-47,151-163`），
//! 按键是 `P`（`Client/KeyBindSettings.cs:225` 的 `KeybindOptions.Group = Keys.P`）。
//! 本作按需求把成员信息铺开成表格：名字 / 等级 / 职业 / 地图 / 在线 / 血量百分比，
//! 并在底部放邀请输入框、「邀请」「离队」「踢人」三个按钮与「允许组队」开关。
//!
//! | 元素 | 本项目 | Crystal 出处 |
//! |---|---|---|
//! | 成员表 | `S.AddMember` 顺序 + `GroupMembersMap` 地图 + 实体表关联等级/职业/血量 | `GroupList` / `GroupMembersMap`（`:12-14`） |
//! | 队长标记 | 第一行画「长」 | `GroupList[0]` 就是队长（`GroupDialog.cs:127-131`） |
//! | 邀请 | 输入名字 → `C.AddMember{Name}`（只有队长/没有队伍时可用） | `AddButton.Click → MirInputBox → C.AddMember`（`:184-206`） |
//! | 离队 | `C.DelMember{Name = 自己的名字}` | `DelButton` 输名字 = 退队（`:207-224`） |
//! | 踢人 | 选中成员 → `C.DelMember{Name = 成员}` | 同上 |
//! | 允许组队 | 标题右侧开关 → `C.SwitchGroup{AllowGroup}` | `SwitchButton`（`:69-80`） |
//! | 组队聊天 | 底部提示「聊天输入 !! 前缀发组队消息」 | `PlayerObject.cs:2001-2013`（`!!` 前缀） |
//!
//! 简化：
//!
//! * 没有 Crystal 的 `MirInputBox` 弹窗，邀请/踢人都用面板里的单行输入框；
//! * 成员表一屏 12 行（上限 15 人），滚轮滚动（Crystal 只有 15 个固定 `MirLabel`，
//!   两列排布，一屏放得下）；
//! * 「在线」按「队友对象是否在自己的视野里」判定：协议没有单独的在线包，
//!   服务端只在同图近距离时补发 `S.ObjectPlayer`/`S.ObjectHealth`
//!   （`crates/mir-server/src/group.rs` 的 `group_join`）。

use mir_core::{Language, Point};
use mir_render::Frame;

use crate::geom::Rect;
use crate::net::{GroupMemberInfo, MAX_GROUP, NetCommand};
use crate::ui::UiText;
use crate::ui::hud::{BODY_COLOR, GOLD_COLOR, MUTED_COLOR, SIZE_BODY, SIZE_SMALL, TITLE_COLOR};
use crate::ui::input::{self, TextField};
use crate::ui::labels::{self, Label};
use crate::ui::{FieldId, PanelContext};

/// 面板宽度。
pub const PANEL_WIDTH: u32 = 420;
/// 面板高度（9 行成员 + 表头 + 按钮，800×600 下不压聊天面板）。
pub const PANEL_HEIGHT: u32 = 340;
/// 内边距。
pub const PADDING: u32 = 10;
/// 标题高度。
pub const TITLE_HEIGHT: u32 = 24;
/// 表头高度。
pub const HEADER_HEIGHT: u32 = 18;
/// 成员行高度。
pub const ROW_HEIGHT: u32 = 26;
/// 一屏显示多少行（队伍上限 15，多出来的滚轮滚动）。
pub const ROWS: usize = 9;
/// 按钮高度。
pub const BUTTON_HEIGHT: u32 = 22;
/// 血量条宽度。
pub const HP_BAR_WIDTH: u32 = 54;
/// 血量条高度。
pub const HP_BAR_HEIGHT: u32 = 8;
/// 队友名字颜色（Crystal 的组队蓝：小地图 `MainDialogs.cs:1947-1948`、
/// 血条 `MapObject.cs:485` 的 10 号组队血条）。
pub const GROUP_NAME_COLOR: [u8; 4] = [96, 160, 255, 255];
/// 血量条的黄色档（与 `mir-render` 的实体血条分档一致）。
pub const HP_WARN_COLOR: [u8; 4] = [240, 200, 60, 255];
/// 血量条的红色档。
pub const HP_LOW_COLOR: [u8; 4] = [220, 60, 60, 255];
/// 血量条背景。
pub const HP_BAR_BACKGROUND: [u8; 4] = [40, 28, 30, 255];
/// 组队聊天前缀（Crystal `PlayerObject.cs:2001` 的 `message.StartsWith("!!")`）。
pub const GROUP_CHAT_PREFIX: &str = "!!";

/// 组队面板布局（绘制与命中测试共用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupLayout {
    /// 面板。
    pub panel: Rect,
    /// 表头（列名）。
    pub header: Rect,
    /// 可见成员行。
    pub rows: Vec<Rect>,
    /// 第一行的绝对下标。
    pub first_row: usize,
    /// 邀请名字输入框。
    pub invite_field: Rect,
    /// 「邀请」按钮。
    pub invite: Rect,
    /// 「离队」按钮。
    pub leave: Rect,
    /// 「踢人」按钮。
    pub kick: Rect,
    /// 「允许组队」开关。
    pub switch: Rect,
    /// 底部提示。
    pub footer: Rect,
}

impl GroupLayout {
    /// 命中成员行（绝对下标）。
    #[must_use]
    pub fn row_at(&self, point: Point) -> Option<usize> {
        self.rows
            .iter()
            .position(|rect| rect.contains(point))
            .map(|index| self.first_row + index)
    }
}

/// 计算组队面板布局（背包左边的空位、状态栏下方）。
///
/// Crystal 的 `GroupDialog` 是可拖动窗口、初始位置居中（`GroupDialog.cs:26`）；
/// 本作不可拖动，放在「右侧背包面板左边」的空位——既不挡背包，也不压左下的聊天面板。
#[must_use]
pub fn group_layout(frame: Rect, member_count: usize, scroll: usize) -> GroupLayout {
    // 小窗口下先保证不出屏（见 `trade::trade_layout` 的同一处理）。
    let available_width = frame.width.saturating_sub(16);
    let width = PANEL_WIDTH
        .min(available_width)
        .max(320.min(available_width));
    let available_height = frame.height.saturating_sub(70);
    let height = PANEL_HEIGHT
        .min(available_height)
        .max(220.min(available_height));
    // 给右侧背包面板留位置（背包宽 `inventory::panel_width()`）。
    let reserved = crate::ui::inventory::panel_width() + 20;
    let x = (frame.right() - i32::try_from(width.saturating_add(reserved)).unwrap_or(0))
        .max(frame.x + 8);
    let panel = Rect::new(x, frame.y + 60, width, height);
    let content_x = panel.x + i32::try_from(PADDING).unwrap_or(0);
    let content_width = width.saturating_sub(PADDING * 2);
    let header = Rect::new(
        content_x,
        panel.y + i32::try_from(TITLE_HEIGHT).unwrap_or(0),
        content_width,
        HEADER_HEIGHT,
    );
    let visible = member_count.min(ROWS);
    let max_scroll = member_count.saturating_sub(visible.max(1));
    let first_row = scroll.min(max_scroll);
    // 320×240：三按钮 + 输入框放不进一行，改两行并把成员行数收窄到不压按钮。
    let two_rows = content_width < 380;
    let extra_row = if two_rows {
        i32::try_from(BUTTON_HEIGHT + 4).unwrap_or(0)
    } else {
        0
    };
    let buttons_top = panel.bottom() - i32::try_from(BUTTON_HEIGHT + 26).unwrap_or(0) - extra_row;
    let row_area_top = header.y + i32::try_from(HEADER_HEIGHT).unwrap_or(0);
    let row_capacity = usize::try_from(
        (buttons_top - 6 - row_area_top).max(0) / i32::try_from(ROW_HEIGHT).unwrap_or(1),
    )
    .unwrap_or(0);
    let visible = visible.min(row_capacity.max(1));
    let max_scroll = member_count.saturating_sub(visible.max(1));
    let first_row = first_row.min(max_scroll);
    let mut rows = Vec::with_capacity(visible);
    for index in 0..visible {
        rows.push(Rect::new(
            content_x,
            row_area_top
                + i32::try_from(index).unwrap_or(0) * i32::try_from(ROW_HEIGHT).unwrap_or(0),
            content_width,
            ROW_HEIGHT,
        ));
    }
    let (invite_field, invite, leave, kick) = if two_rows {
        let row1 = buttons_top;
        let row2 = buttons_top + i32::try_from(BUTTON_HEIGHT + 4).unwrap_or(0);
        let field_width = content_width.saturating_sub(76);
        let invite_field = Rect::new(content_x, row1, field_width, BUTTON_HEIGHT);
        let invite = Rect::new(invite_field.right() + 6, row1, 70, BUTTON_HEIGHT);
        let leave = Rect::new(content_x, row2, 70, BUTTON_HEIGHT);
        let kick = Rect::new(leave.right() + 6, row2, 84, BUTTON_HEIGHT);
        (invite_field, invite, leave, kick)
    } else {
        let button_y = panel.bottom() - i32::try_from(BUTTON_HEIGHT + 26).unwrap_or(0);
        let invite_field = Rect::new(content_x, button_y, 132, BUTTON_HEIGHT);
        let invite = Rect::new(invite_field.right() + 6, button_y, 70, BUTTON_HEIGHT);
        let leave = Rect::new(invite.right() + 6, button_y, 70, BUTTON_HEIGHT);
        let kick = Rect::new(leave.right() + 6, button_y, 84, BUTTON_HEIGHT);
        (invite_field, invite, leave, kick)
    };
    let switch = Rect::new(
        panel.right() - i32::try_from(PADDING + 108).unwrap_or(0),
        panel.y + 4,
        108,
        18,
    );
    let footer = Rect::new(
        content_x,
        panel.bottom() - i32::try_from(BUTTON_HEIGHT).unwrap_or(0),
        content_width,
        BUTTON_HEIGHT,
    );
    GroupLayout {
        panel,
        header,
        rows,
        first_row,
        invite_field,
        invite,
        leave,
        kick,
        switch,
        footer,
    }
}

/// 组队邀请弹窗正文（「XXX 邀请你组队」）。
#[must_use]
pub fn invite_message(name: &str, language: Language) -> String {
    format!("{name} {}", Label::GroupInviteSuffix.text(language))
}

/// 一条聊天是不是组队频道（`!!` 前缀，Crystal `PlayerObject.cs:2001-2013`）。
#[must_use]
pub fn is_group_chat(message: &str) -> bool {
    message.starts_with(GROUP_CHAT_PREFIX)
}

/// 把一条聊天输入翻成网络命令：`!!` 开头的走 `C.Chat`（服务端 `group_chat` 会按组队广播，
/// 见 `crates/mir-server/src/group.rs` 的 `World::group_chat`）；其它聊天返回 `None`，
/// 由调用方按普通聊天处理。
#[must_use]
pub fn group_chat_command(message: &str) -> Option<NetCommand> {
    if !is_group_chat(message) {
        return None;
    }
    let trimmed = message.trim();
    if trimmed.len() <= GROUP_CHAT_PREFIX.len() {
        return None;
    }
    Some(NetCommand::Chat(trimmed.to_owned()))
}

/// 血量百分比 → 颜色（> 50% 绿、> 25% 黄、其余红）。
#[must_use]
pub fn hp_color(percent: u8) -> [u8; 4] {
    if percent > 50 {
        [96, 220, 96, 255]
    } else if percent > 25 {
        HP_WARN_COLOR
    } else {
        HP_LOW_COLOR
    }
}

/// 画组队面板。
// 参数覆盖「布局 + 成员 + 选中 + 输入框 + 焦点 + 鼠标 + 开关状态」，拆结构体只会增加样板。
#[allow(clippy::too_many_arguments)]
pub fn draw(
    frame: &mut Frame,
    layout: &GroupLayout,
    context: &PanelContext<'_>,
    members: &[GroupMemberInfo],
    selected: Option<usize>,
    name_field: &TextField,
    focus: Option<FieldId>,
    allow_group: bool,
    is_leader: bool,
    mouse: Point,
) {
    let text = context.text;
    let language = context.language;
    crate::ui::draw_panel(frame, layout.panel);
    let title = if members.is_empty() {
        Label::Group.text(language).to_owned()
    } else {
        format!(
            "{} {}/{}",
            Label::Group.text(language),
            members.len(),
            MAX_GROUP
        )
    };
    text.draw_clipped(
        frame,
        layout.panel.x + 10,
        layout.panel.y + 4,
        &title,
        SIZE_BODY,
        TITLE_COLOR,
        Rect::new(
            layout.panel.x,
            layout.panel.y,
            layout.panel.width.saturating_sub(120),
            TITLE_HEIGHT,
        ),
    );
    // 「允许组队」开关（Crystal `SwitchButton`，`GroupDialog.cs:69-80`）。
    let switch_label = format!(
        "{}：{}",
        Label::GroupAllow.text(language),
        if allow_group {
            Label::ToggleOn.text(language)
        } else {
            Label::ToggleOff.text(language)
        }
    );
    let switch_fill = if layout.switch.contains(mouse) {
        [92, 82, 42, 255]
    } else if allow_group {
        [44, 74, 48, 255]
    } else {
        [66, 40, 40, 255]
    };
    frame.fill_rect(
        layout.switch.x,
        layout.switch.y,
        layout.switch.width,
        layout.switch.height,
        switch_fill,
    );
    text.draw_center(frame, layout.switch, &switch_label, SIZE_SMALL, BODY_COLOR);

    // 表头。
    let columns = column_offsets(layout.header.width);
    for (index, label) in [
        Label::Members.text(language),
        Label::Level.text(language),
        Label::Profession.text(language),
        Label::MapColumn.text(language),
        Label::Online.text(language),
        Label::Health.text(language),
    ]
    .into_iter()
    .enumerate()
    {
        text.draw_clipped(
            frame,
            layout.header.x + columns[index],
            layout.header.y + 2,
            label,
            SIZE_SMALL,
            MUTED_COLOR,
            Rect::new(
                layout.header.x + columns[index],
                layout.header.y,
                column_width(index),
                HEADER_HEIGHT,
            ),
        );
    }

    if members.is_empty() {
        text.draw_center(
            frame,
            Rect::new(layout.panel.x, layout.header.y + 20, layout.panel.width, 40),
            Label::NoGroup.text(language),
            SIZE_BODY,
            MUTED_COLOR,
        );
    }

    for (visible_index, row) in layout.rows.iter().enumerate() {
        let index = layout.first_row + visible_index;
        let Some(member) = members.get(index) else {
            break;
        };
        if selected == Some(index) {
            frame.fill_rect(row.x, row.y, row.width, row.height, [86, 74, 32, 235]);
            frame.fill_rect(row.x, row.y, 2, row.height, [255, 216, 96, 255]);
        } else if row.contains(mouse) {
            frame.fill_rect(row.x, row.y, row.width, row.height, [44, 56, 76, 235]);
        } else if visible_index % 2 == 1 {
            frame.fill_rect(row.x, row.y, row.width, row.height, [26, 30, 38, 160]);
        }
        let name_color = if member.is_self {
            GOLD_COLOR
        } else {
            GROUP_NAME_COLOR
        };
        text.draw_clipped(
            frame,
            row.x + columns[0],
            row.y + 4,
            &member.name,
            SIZE_BODY,
            name_color,
            Rect::new(
                row.x + columns[0],
                row.y + 4,
                column_width(0),
                SIZE_BODY + 4,
            ),
        );
        if member.is_leader {
            text.draw_right(
                frame,
                row.x + columns[1] - 4,
                row.y + 6,
                Label::GroupLeader.text(language),
                SIZE_SMALL,
                TITLE_COLOR,
            );
        }
        text.draw_clipped(
            frame,
            row.x + columns[1],
            row.y + 6,
            &if member.level > 0 {
                member.level.to_string()
            } else {
                "--".to_owned()
            },
            SIZE_SMALL,
            BODY_COLOR,
            Rect::new(
                row.x + columns[1],
                row.y + 6,
                column_width(1),
                SIZE_SMALL + 4,
            ),
        );
        text.draw_clipped(
            frame,
            row.x + columns[2],
            row.y + 6,
            &member.class.map_or_else(
                || "--".to_owned(),
                |class| labels::class_name(class, language).to_owned(),
            ),
            SIZE_SMALL,
            BODY_COLOR,
            Rect::new(
                row.x + columns[2],
                row.y + 6,
                column_width(2),
                SIZE_SMALL + 4,
            ),
        );
        text.draw_clipped(
            frame,
            row.x + columns[3],
            row.y + 6,
            if member.map.is_empty() {
                "--"
            } else {
                member.map.as_str()
            },
            SIZE_SMALL,
            if member.online {
                BODY_COLOR
            } else {
                MUTED_COLOR
            },
            Rect::new(
                row.x + columns[3],
                row.y + 6,
                column_width(3),
                SIZE_SMALL + 4,
            ),
        );
        text.draw_clipped(
            frame,
            row.x + columns[4],
            row.y + 6,
            if member.online {
                Label::Online.text(language)
            } else {
                Label::Offline.text(language)
            },
            SIZE_SMALL,
            if member.online {
                [140, 240, 150, 255]
            } else {
                MUTED_COLOR
            },
            Rect::new(
                row.x + columns[4],
                row.y + 6,
                column_width(4),
                SIZE_SMALL + 4,
            ),
        );
        // 血量条 + 百分比。
        let bar = Rect::new(
            row.x + columns[5],
            row.y + i32::try_from(ROW_HEIGHT.saturating_sub(HP_BAR_HEIGHT) / 2).unwrap_or(0),
            HP_BAR_WIDTH,
            HP_BAR_HEIGHT,
        );
        frame.fill_rect(bar.x, bar.y, bar.width, bar.height, HP_BAR_BACKGROUND);
        if let Some(percent) = member.hp_percent {
            let filled = bar.width * u32::from(percent.min(100)) / 100;
            if filled > 0 {
                frame.fill_rect(bar.x, bar.y, filled, bar.height, hp_color(percent));
            }
        }
        text.draw_clipped(
            frame,
            bar.right() + 4,
            row.y + 6,
            &member
                .hp_percent
                .map_or_else(|| "--".to_owned(), |percent| format!("{percent}%")),
            SIZE_SMALL,
            BODY_COLOR,
            Rect::new(
                bar.right() + 4,
                row.y + 6,
                u32::try_from(row.right() - bar.right() - 6).unwrap_or(0),
                SIZE_SMALL + 4,
            ),
        );
    }

    // 底部按钮。
    input::draw(
        frame,
        text,
        layout.invite_field,
        name_field,
        focus == Some(FieldId::GroupName),
        Label::InviteName.text(language),
    );
    let can_invite = is_leader || members.is_empty();
    draw_button(
        frame,
        text,
        layout.invite,
        Label::Invite.text(language),
        mouse,
        can_invite && !name_field.is_empty(),
    );
    draw_button(
        frame,
        text,
        layout.leave,
        Label::Leave.text(language),
        mouse,
        true,
    );
    let can_kick = is_leader
        && selected.is_some_and(|index| members.get(index).is_some_and(|member| !member.is_self));
    draw_button(
        frame,
        text,
        layout.kick,
        Label::GroupKick.text(language),
        mouse,
        can_kick,
    );
    text.draw_clipped(
        frame,
        layout.footer.x,
        layout.footer.y + 2,
        &format!(
            "{} · {}",
            Label::GroupHint.text(language),
            Label::GroupChatHint.text(language)
        ),
        SIZE_SMALL,
        MUTED_COLOR,
        layout.footer,
    );
}

/// 列偏移（相对于行左边界）：名字 / 等级 / 职业 / 地图 / 在线 / 血量。
///
/// 按内容宽度等比分配，窗口变小时列会收紧（文字由 `draw_clipped` 裁剪）。
#[must_use]
pub fn column_offsets(content_width: u32) -> [i32; 6] {
    let scale = |ratio: u32| i32::try_from(content_width * ratio / 100).unwrap_or(0);
    [6, scale(28), scale(38), scale(52), scale(70), scale(84)]
}

/// 第 `index` 列的宽度。
fn column_width(index: usize) -> u32 {
    match index {
        0 => 110,
        1 => 32,
        2 => 56,
        3 => 74,
        4 => 40,
        _ => HP_BAR_WIDTH + 40,
    }
}

/// 画一个按钮（与行会/交易面板同款样式）。
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_rows_and_buttons_are_hit_testable() {
        let frame = Rect::screen(800, 600);
        let layout = group_layout(frame, 3, 0);
        assert_eq!(layout.rows.len(), 3);
        for (index, row) in layout.rows.iter().enumerate() {
            assert!(layout.panel.contains(row.center()));
            assert_eq!(layout.row_at(row.center()), Some(index));
        }
        for rect in [
            layout.invite_field,
            layout.invite,
            layout.leave,
            layout.kick,
            layout.switch,
        ] {
            assert!(layout.panel.contains(rect.center()), "{rect:?} 应在面板内");
        }
        assert!(layout.panel.right() <= frame.right());
    }

    #[test]
    fn scrolling_is_clamped_to_the_last_page() {
        let frame = Rect::screen(800, 600);
        let layout = group_layout(frame, 15, 99);
        assert_eq!(layout.rows.len(), ROWS);
        assert_eq!(layout.first_row, 15 - ROWS);
    }

    #[test]
    fn group_chat_prefix_maps_to_a_chat_command() {
        assert!(is_group_chat("!!集合"));
        assert!(!is_group_chat("集合"));
        assert_eq!(
            group_chat_command("!!集合"),
            Some(NetCommand::Chat("!!集合".to_owned()))
        );
        assert_eq!(group_chat_command("!!"), None);
        assert_eq!(group_chat_command("你好"), None);
    }

    #[test]
    fn hp_colors_change_with_the_percentage() {
        assert_ne!(hp_color(80), hp_color(40));
        assert_ne!(hp_color(40), hp_color(10));
        assert_eq!(hp_color(80), [96, 220, 96, 255]);
    }

    #[test]
    fn invite_messages_are_localized() {
        assert_eq!(invite_message("张三", Language::ZhCn), "张三 邀请你组队");
        assert_eq!(
            invite_message("Tom", Language::En),
            "Tom invites you to a group"
        );
    }

    #[test]
    fn small_windows_keep_the_panel_on_screen() {
        let layout = group_layout(Rect::screen(320, 240), 2, 0);
        assert!(layout.panel.width <= 320);
        assert!(layout.panel.right() <= 320);
    }
}
