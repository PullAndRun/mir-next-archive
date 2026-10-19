//! 行会面板（`G`；Crystal `GuildDialog` / `Client/MirScenes/Dialogs/GuildDialog.cs:14`）。
//!
//! 四个页签（对应 Crystal 的左右两栏按钮）：
//!
//! | 页签 | 数据来源 | 动作 |
//! |---|---|---|
//! | 成员 | `S.GuildStatus` + `S.GuildMemberChange{Status=255}` | `C.EditGuildMember{ChangeType=2}` 升/降职、`{ChangeType=1}` 踢人 |
//! | 仓库 | `S.GuildStorageList`（112 格）、`S.GuildStorageGoldChange` | `C.GuildStorageItemChange`（拖拽）、`C.GuildStorageGoldChange`（存/取资金） |
//! | 增益 | `S.GuildBuffList` | `C.GuildBuffUpdate{Action=1/2}` |
//! | 公告 | `S.GuildNoticeChange` | `C.EditGuildNotice` |
//!
//! 权限：`S.GuildStatus.MyOptions` 是职位权限位掩码（`GuildRankOptions`），按钮只在有权限时
//! 可用（会长是 `255`，见 `Server/MirDatabase/GuildInfo.cs:54`）。
//!
//! 简化：
//! * 成员表的「地图」列**恒为空**：协议 `GuildMember` 只有名字/编号/最后登录/投票/在线
//!   （`Shared/Data/GuildData.cs:62-90`），没有所在地图；Crystal 的成员页也没有这一列
//!   （只有名字/职位/在线状态 + 邀请按钮），这一列按需求保留但显示 `--`；
//! * Crystal 的成员表用下拉框改职位（`MembersRanks`，`GuildDialog.cs` 的
//!   `MirDropDownBox`），本作用「升职 / 降职」两个按钮（职位序号 ±1）；
//! * 没有「新建职位 / 职位权限编辑 / 邀请入会」控件（Crystal 有，需求没要求；服务端
//!   `C.EditGuildMember{ChangeType=0/3/4/5}` 已实现但界面没接）；
//! * 行会仓库直接画满 112 格（Crystal 是 8 列 × 14 行、一页 8 行滚动，
//!   `GuildDialog.cs:672-689`）；面板足够高，这里不滚动；
//! * 行会资金没有「金额输入框 + 存取按钮」之外的反馈（Crystal 用 `MirAmountBox`）。

use mir_core::{Language, Point};
use mir_render::{Frame, Image};

use crate::geom::Rect;
use crate::net::{GUILD_STORAGE_SLOTS, GuildMemberView, WorldSnapshot};
use crate::ui::grid::{self, GridLayout};
use crate::ui::hud::{BODY_COLOR, GOLD_COLOR, MUTED_COLOR, SIZE_BODY, SIZE_SMALL, TITLE_COLOR};
use crate::ui::input::{self, TextField};
use crate::ui::labels::{self, Label};
use crate::ui::{FieldId, PanelContext, UiText};

/// 面板宽度。
///
/// 取 500 而不是 Crystal 的 ~700：本作的面板不可拖动，靠左放才能让右侧的背包面板
/// （`ui::inventory`，屏幕右侧 268 px）露出来，背包 → 行会仓库的拖拽才点得到。
pub const PANEL_WIDTH: u32 = 500;
/// 面板高度。
pub const PANEL_HEIGHT: u32 = 520;
/// 内边距。
pub const PADDING: u32 = 10;
/// 标题高度。
pub const TITLE_HEIGHT: u32 = 24;
/// 页签行高度。
pub const TABS_HEIGHT: u32 = 26;
/// 表头高度。
pub const HEADER_HEIGHT: u32 = 18;
/// 成员行高度。
pub const MEMBER_ROW_HEIGHT: u32 = 22;
/// 一页能显示多少成员。
pub const MEMBER_ROWS: usize = 12;
/// 增益行高度。
pub const BUFF_ROW_HEIGHT: u32 = 24;
/// 一页能显示多少行增益。
pub const BUFF_ROWS: usize = 10;
/// 行会仓库列数（Crystal `StorageGrid = new MirItemCell[8 * 14]`）。
pub const STORAGE_COLUMNS: usize = 8;
/// 行会仓库格子边长。
pub const STORAGE_CELL: u32 = 26;
/// 行会仓库格子间距。
pub const STORAGE_GAP: u32 = 3;
/// 底部按钮高度。
pub const BUTTON_HEIGHT: u32 = 22;
/// 在线成员的名字颜色。
pub const ONLINE_COLOR: [u8; 4] = [140, 240, 150, 255];

/// 行会面板的五个页签。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GuildTab {
    /// 成员。
    #[default]
    Members,
    /// 仓库。
    Storage,
    /// 增益。
    Buffs,
    /// 联盟。
    Ally,
    /// 公告。
    Notice,
}

impl GuildTab {
    /// 全部页签（按钮顺序）。
    pub const ALL: [Self; 5] = [
        Self::Members,
        Self::Storage,
        Self::Buffs,
        Self::Ally,
        Self::Notice,
    ];

    /// 页签名。
    #[must_use]
    pub const fn title(self, language: Language) -> &'static str {
        match self {
            Self::Members => Label::Members.text(language),
            Self::Storage => Label::GuildStorageTab.text(language),
            Self::Buffs => Label::GuildBuffsTab.text(language),
            Self::Ally => Label::GuildAllyTab.text(language),
            Self::Notice => Label::Notice.text(language),
        }
    }

    /// 页签下标（与 [`GuildLayout::tabs`] 同序）。
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Members => 0,
            Self::Storage => 1,
            Self::Buffs => 2,
            Self::Ally => 3,
            Self::Notice => 4,
        }
    }
}

/// 行会面板布局（五个页签共用同一块面板）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildLayout {
    /// 面板矩形。
    pub panel: Rect,
    /// 五个页签按钮。
    pub tabs: [Rect; 5],
    /// 内容区。
    pub content: Rect,
    /// 成员表表头。
    pub member_header: Rect,
    /// 可见成员行。
    pub member_rows: Vec<Rect>,
    /// 当前页第一个成员（展平后的绝对下标）。
    pub first_member: usize,
    /// 升职。
    pub promote: Rect,
    /// 降职。
    pub demote: Rect,
    /// 踢人。
    pub kick: Rect,
    /// 资金行。
    pub gold_row: Rect,
    /// 资金输入框。
    pub gold_field: Rect,
    /// 存入。
    pub deposit: Rect,
    /// 取出。
    pub withdraw: Rect,
    /// 行会仓库网格。
    pub storage_grid: GridLayout,
    /// 可见增益行。
    pub buff_rows: Vec<Rect>,
    /// 每行增益的「购买/激活」按钮（与 `buff_rows` 同序）。
    pub buff_buttons: Vec<Rect>,
    /// 当前页第一条增益的绝对下标。
    pub first_buff: usize,
    /// 公告正文区。
    pub notice_area: Rect,
    /// 公告输入框。
    pub notice_field: Rect,
    /// 保存公告。
    pub save_notice: Rect,
    /// 联盟：盟友行会名输入框。
    pub ally_field: Rect,
    /// 联盟：申请结盟。
    pub ally_request: Rect,
    /// 联盟：同意申请。
    pub ally_accept: Rect,
    /// 联盟：解除联盟。
    pub ally_remove: Rect,
    /// 底部提示。
    pub footer: Rect,
}

impl GuildLayout {
    /// 命中测试：点在哪个页签上（返回 [`GuildTab::index`]）。
    #[must_use]
    pub fn tab_at(&self, point: Point) -> Option<usize> {
        self.tabs.iter().position(|rect| rect.contains(point))
    }

    /// 命中测试：点在哪个成员行上（返回展平后的绝对下标）。
    #[must_use]
    pub fn member_at(&self, point: Point) -> Option<usize> {
        self.member_rows
            .iter()
            .position(|rect| rect.contains(point))
            .map(|index| self.first_member + index)
    }

    /// 命中测试：点在哪个增益行上（返回绝对下标）。
    #[must_use]
    pub fn buff_at(&self, point: Point) -> Option<usize> {
        self.buff_rows
            .iter()
            .position(|rect| rect.contains(point))
            .map(|index| self.first_buff + index)
    }

    /// 命中测试：点在哪个增益按钮上（返回绝对下标）。
    #[must_use]
    pub fn buff_button_at(&self, point: Point) -> Option<usize> {
        self.buff_buttons
            .iter()
            .position(|rect| rect.contains(point))
            .map(|index| self.first_buff + index)
    }
}

/// 计算行会面板布局。
#[must_use]
pub fn guild_layout(
    frame: Rect,
    member_count: usize,
    buff_count: usize,
    member_scroll: usize,
    buff_scroll: usize,
) -> GuildLayout {
    let width = PANEL_WIDTH.min(frame.width.saturating_sub(16)).max(200);
    let height = PANEL_HEIGHT.min(frame.height.saturating_sub(16)).max(160);
    // 靠左、状态栏下方；高度 520 在 800×600 里正好放满 14 行仓库格。
    let panel = Rect::new(
        frame.x + 8,
        (frame.y + 60).min(frame.bottom() - i32::try_from(height).unwrap_or(0) - 8),
        width,
        height,
    );
    let content_x = panel.x + i32::try_from(PADDING).unwrap_or(0);
    let content_width = width.saturating_sub(PADDING * 2);
    let tabs_y = panel.y + i32::try_from(TITLE_HEIGHT + 2).unwrap_or(0);
    let mut tabs = [Rect::default(); 5];
    let tab_gap = 4;
    let tab_width = content_width.saturating_sub(4 * tab_gap) / 5;
    for (index, tab) in tabs.iter_mut().enumerate() {
        *tab = Rect::new(
            content_x
                + i32::try_from(index).unwrap_or(0)
                    * i32::try_from(tab_width + tab_gap).unwrap_or(0),
            tabs_y,
            tab_width,
            TABS_HEIGHT - 4,
        );
    }
    let content_top = tabs_y + i32::try_from(TABS_HEIGHT).unwrap_or(0);
    let content_bottom = panel.bottom() - i32::try_from(BUTTON_HEIGHT + 12).unwrap_or(0);
    let content = Rect::new(
        content_x,
        content_top,
        content_width,
        u32::try_from(content_bottom.saturating_sub(content_top)).unwrap_or(0),
    );
    let footer = Rect::new(
        content_x,
        panel.bottom() - i32::try_from(BUTTON_HEIGHT + 4).unwrap_or(0),
        content_width,
        BUTTON_HEIGHT,
    );
    let button_y = footer.y;

    // 成员页。
    let member_header = Rect::new(content.x, content.y, content.width, HEADER_HEIGHT);
    let visible_members = member_count.min(MEMBER_ROWS);
    let max_member_scroll = member_count.saturating_sub(visible_members.max(1));
    let first_member = member_scroll.min(max_member_scroll);
    let mut member_rows = Vec::with_capacity(visible_members);
    for index in 0..visible_members {
        member_rows.push(Rect::new(
            content.x,
            content.y
                + i32::try_from(HEADER_HEIGHT).unwrap_or(0)
                + i32::try_from(index).unwrap_or(0) * i32::try_from(MEMBER_ROW_HEIGHT).unwrap_or(0),
            content.width,
            MEMBER_ROW_HEIGHT,
        ));
    }
    let action_width = 78;
    let kick = Rect::new(
        panel.right() - i32::try_from(PADDING + action_width).unwrap_or(0),
        button_y,
        action_width,
        BUTTON_HEIGHT,
    );
    let demote = Rect::new(
        kick.x - i32::try_from(action_width + 6).unwrap_or(0),
        button_y,
        action_width,
        BUTTON_HEIGHT,
    );
    let promote = Rect::new(
        demote.x - i32::try_from(action_width + 6).unwrap_or(0),
        button_y,
        action_width,
        BUTTON_HEIGHT,
    );

    // 仓库页：资金行 + 112 格。
    let gold_row = Rect::new(content.x, content.y, content.width, 24);
    let gold_field = Rect::new(content.x + 200, content.y + 1, 120, 22);
    let deposit = Rect::new(content.x + 326, content.y + 1, 70, 22);
    let withdraw = Rect::new(content.x + 402, content.y + 1, 70, 22);
    let grid_area = Rect::new(
        content.x,
        content.y + 28,
        content.width,
        content.height.saturating_sub(30),
    );
    let storage_grid = grid::grid_layout(
        grid_area,
        GUILD_STORAGE_SLOTS,
        STORAGE_COLUMNS,
        STORAGE_CELL,
        STORAGE_GAP,
        0,
    );

    // 增益页。
    let visible_buffs = buff_count.min(BUFF_ROWS);
    let max_buff_scroll = buff_count.saturating_sub(visible_buffs.max(1));
    let first_buff = buff_scroll.min(max_buff_scroll);
    let mut buff_rows = Vec::with_capacity(visible_buffs);
    let mut buff_buttons = Vec::with_capacity(visible_buffs);
    for index in 0..visible_buffs {
        let row = Rect::new(
            content.x,
            content.y
                + i32::try_from(index).unwrap_or(0) * i32::try_from(BUFF_ROW_HEIGHT).unwrap_or(0),
            content.width,
            BUFF_ROW_HEIGHT,
        );
        buff_buttons.push(Rect::new(
            row.right() - 84,
            row.y + 2,
            76,
            BUTTON_HEIGHT - 4,
        ));
        buff_rows.push(row);
    }

    // 公告页。
    let notice_area = Rect::new(
        content.x,
        content.y,
        content.width,
        content.height.saturating_sub(34),
    );
    let notice_field = Rect::new(
        content.x,
        panel.bottom() - i32::try_from(BUTTON_HEIGHT + 12).unwrap_or(0),
        content.width.saturating_sub(96),
        22,
    );
    let save_notice = Rect::new(notice_field.right() + 6, notice_field.y, 80, 22);

    // 联盟页：输入框一行 + 三个命令按钮一行（都在内容区底部，窄屏也不会出屏）。
    let ally_row2 = content.bottom() - 26;
    let ally_row1 = ally_row2 - 26;
    let ally_field = Rect::new(content.x, ally_row1, content.width, 22);
    let ally_button = content.width.saturating_sub(12) / 3;
    let ally_request = Rect::new(content.x, ally_row2, ally_button, 22);
    let ally_accept = Rect::new(ally_request.right() + 6, ally_row2, ally_button, 22);
    let ally_remove = Rect::new(ally_accept.right() + 6, ally_row2, ally_button, 22);

    GuildLayout {
        panel,
        tabs,
        content,
        member_header,
        member_rows,
        first_member,
        promote,
        demote,
        kick,
        gold_row,
        gold_field,
        deposit,
        withdraw,
        storage_grid,
        buff_rows,
        buff_buttons,
        first_buff,
        notice_area,
        notice_field,
        save_notice,
        ally_field,
        ally_request,
        ally_accept,
        ally_remove,
        footer,
    }
}

/// 行会概况行文案（`行会 · 等级 3 · 成员 5/20 · 行会资金 10,000`）。
#[must_use]
pub fn summary_text(snapshot: &WorldSnapshot, language: Language) -> String {
    let Some(guild) = &snapshot.guild else {
        return Label::NoGuild.text(language).to_owned();
    };
    if guild.name.is_empty() {
        return Label::NoGuild.text(language).to_owned();
    }
    let max_members = if guild.max_members > 0 {
        guild.max_members.to_string()
    } else {
        "∞".to_owned()
    };
    format!(
        "{} · {} {} · {} {}/{} · {} {}",
        guild.name,
        Label::Level.text(language),
        guild.level,
        Label::Members.text(language),
        guild.member_count,
        max_members,
        Label::GuildFunds.text(language),
        labels::format_number(u64::from(guild.gold))
    )
}

/// 成员行的「在线/离线」文案。
#[must_use]
pub fn member_status_text(member: &GuildMemberView, language: Language) -> &'static str {
    if member.online {
        Label::Online.text(language)
    } else {
        Label::Offline.text(language)
    }
}

/// 增益状态文案（Crystal `GuildDialog` 的 `Available/Obtained/Expired/CountingDown`）。
#[must_use]
pub fn buff_state_text(
    snapshot: &WorldSnapshot,
    info: &crate::net::GuildBuffInfoView,
    language: Language,
) -> String {
    let guild_level = snapshot.guild.as_ref().map_or(0, |guild| guild.level);
    let points = snapshot
        .guild
        .as_ref()
        .map_or(0, |guild| guild.spare_points);
    if let Some(active) = snapshot.guild_active_buff(info.id) {
        if active.active {
            if active.active_time_remaining_minutes <= 0 {
                return format!(
                    "{} · {}",
                    Label::Obtained.text(language),
                    Label::Infinite.text(language)
                );
            }
            return format!(
                "{} · {} {} {}",
                Label::Obtained.text(language),
                Label::TimeLeft.text(language),
                active.active_time_remaining_minutes,
                Label::MinutesLeft.text(language)
            );
        }
        return format!(
            "{} · {}",
            Label::Obtained.text(language),
            Label::Expired.text(language)
        );
    }
    if guild_level < info.level_requirement || points < info.points_requirement {
        return format!(
            "{} {} · {} {}",
            Label::Level.text(language),
            info.level_requirement,
            Label::Points.text(language),
            info.points_requirement
        );
    }
    Label::Available.text(language).to_owned()
}

/// 画行会面板。
// 参数覆盖「当前页签 + 选中项 + 两个输入框 + 焦点 + 鼠标」，拆分结构体只会增加样板。
#[allow(clippy::too_many_arguments)]
pub fn draw(
    frame: &mut Frame,
    layout: &GuildLayout,
    context: &PanelContext<'_>,
    tab: GuildTab,
    selected_member: Option<usize>,
    gold_field: &TextField,
    notice_field: &TextField,
    ally_field: &TextField,
    allies: &[String],
    ally_pending: Option<&str>,
    focus: Option<FieldId>,
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
        &format!("{} (G)", Label::Guild.text(language)),
        SIZE_BODY,
        TITLE_COLOR,
        Rect::new(
            layout.panel.x,
            layout.panel.y,
            layout.panel.width,
            TITLE_HEIGHT,
        ),
    );
    text.draw_right(
        frame,
        layout.panel.right() - 10,
        layout.panel.y + 5,
        &summary_text(snapshot, language),
        SIZE_SMALL,
        if snapshot
            .guild
            .as_ref()
            .is_some_and(|guild| !guild.name.is_empty())
        {
            GOLD_COLOR
        } else {
            MUTED_COLOR
        },
    );
    for (index, tab_kind) in GuildTab::ALL.into_iter().enumerate() {
        let rect = layout.tabs[index];
        let active = tab_kind == tab;
        let fill = if active {
            [86, 74, 32, 240]
        } else if rect.contains(mouse) {
            [52, 64, 84, 240]
        } else {
            [32, 38, 48, 220]
        };
        frame.fill_rect(rect.x, rect.y, rect.width, rect.height, fill);
        frame.fill_rect(rect.x, rect.y, rect.width, 1, [110, 124, 146, 255]);
        text.draw_center(
            frame,
            rect,
            tab_kind.title(language),
            SIZE_SMALL,
            if active {
                [255, 232, 160, 255]
            } else {
                BODY_COLOR
            },
        );
    }
    if snapshot
        .guild
        .as_ref()
        .is_none_or(|guild| guild.name.is_empty())
    {
        text.draw_center(
            frame,
            layout.content,
            Label::NoGuild.text(language),
            SIZE_BODY,
            MUTED_COLOR,
        );
        return;
    }
    match tab {
        GuildTab::Members => draw_members(
            frame,
            text,
            layout,
            snapshot,
            selected_member,
            mouse,
            language,
        ),
        GuildTab::Storage => draw_storage(
            frame, text, layout, snapshot, gold_field, focus, mouse, icon, language,
        ),
        GuildTab::Buffs => draw_buffs(frame, text, layout, snapshot, mouse, language),
        GuildTab::Ally => {
            draw_ally(
                frame,
                text,
                layout,
                ally_field,
                allies,
                ally_pending,
                focus,
                mouse,
                language,
            );
        }
        GuildTab::Notice => {
            draw_notice(
                frame,
                text,
                layout,
                snapshot,
                notice_field,
                focus,
                mouse,
                language,
            );
        }
    }
    text.draw_clipped(
        frame,
        layout.footer.x,
        layout.footer.y,
        Label::GuildHint.text(language),
        SIZE_SMALL,
        MUTED_COLOR,
        layout.footer,
    );
}

/// 画一个按钮。
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

/// 成员页。
fn draw_members(
    frame: &mut Frame,
    text: &UiText,
    layout: &GuildLayout,
    snapshot: &WorldSnapshot,
    selected_member: Option<usize>,
    mouse: Point,
    language: Language,
) {
    let members = snapshot.guild_members();
    let header = layout.member_header;
    text.draw_clipped(
        frame,
        header.x + 6,
        header.y,
        Label::Rank.text(language),
        SIZE_SMALL,
        MUTED_COLOR,
        Rect::new(header.x + 6, header.y, 120, HEADER_HEIGHT),
    );
    text.draw_clipped(
        frame,
        header.x + 130,
        header.y,
        Label::Members.text(language),
        SIZE_SMALL,
        MUTED_COLOR,
        Rect::new(header.x + 130, header.y, 200, HEADER_HEIGHT),
    );
    text.draw_clipped(
        frame,
        header.x + 300,
        header.y,
        Label::Online.text(language),
        SIZE_SMALL,
        MUTED_COLOR,
        Rect::new(header.x + 300, header.y, 80, HEADER_HEIGHT),
    );
    text.draw_clipped(
        frame,
        header.x + 380,
        header.y,
        Label::MapColumn.text(language),
        SIZE_SMALL,
        MUTED_COLOR,
        Rect::new(header.x + 380, header.y, 80, HEADER_HEIGHT),
    );
    for (visible_index, row) in layout.member_rows.iter().enumerate() {
        let index = layout.first_member + visible_index;
        let Some(entry) = members.get(index) else {
            break;
        };
        let (rank, member) = *entry;
        if selected_member == Some(index) {
            frame.fill_rect(row.x, row.y, row.width, row.height, [86, 74, 32, 235]);
            frame.fill_rect(row.x, row.y, 2, row.height, [255, 216, 96, 255]);
        } else if row.contains(mouse) {
            frame.fill_rect(row.x, row.y, row.width, row.height, [44, 56, 76, 235]);
        } else if visible_index % 2 == 1 {
            frame.fill_rect(row.x, row.y, row.width, row.height, [26, 30, 38, 160]);
        }
        text.draw_clipped(
            frame,
            row.x + 6,
            row.y + 3,
            &rank.name,
            SIZE_SMALL,
            MUTED_COLOR,
            Rect::new(row.x + 6, row.y + 3, 120, SIZE_SMALL + 4),
        );
        text.draw_clipped(
            frame,
            row.x + 130,
            row.y + 3,
            &member.name,
            SIZE_BODY,
            if member.online {
                ONLINE_COLOR
            } else {
                BODY_COLOR
            },
            Rect::new(row.x + 130, row.y + 3, 200, SIZE_BODY + 4),
        );
        text.draw_clipped(
            frame,
            row.x + 300,
            row.y + 3,
            member_status_text(member, language),
            SIZE_SMALL,
            if member.online {
                ONLINE_COLOR
            } else {
                MUTED_COLOR
            },
            Rect::new(row.x + 300, row.y + 3, 80, SIZE_SMALL + 4),
        );
        // 简化：协议里的 GuildMember 没有地图字段（见模块文档），这里显示 `--`。
        text.draw_clipped(
            frame,
            row.x + 380,
            row.y + 3,
            "--",
            SIZE_SMALL,
            MUTED_COLOR,
            Rect::new(row.x + 380, row.y + 3, 80, SIZE_SMALL + 4),
        );
    }
    let can_change_rank = snapshot
        .guild
        .as_ref()
        .is_some_and(crate::net::GuildView::can_change_rank);
    let can_kick = snapshot
        .guild
        .as_ref()
        .is_some_and(crate::net::GuildView::can_kick);
    let selected = selected_member
        .and_then(|index| members.get(index))
        .copied();
    // 升职：职位序号 -1（会长是 0，不能升）；降职：+1（已经是最后一档就不降）。
    let max_rank = snapshot.guild_ranks.len().saturating_sub(1);
    let can_promote = can_change_rank && selected.is_some_and(|(rank, _)| rank.index > 0);
    let can_demote = can_change_rank
        && selected.is_some_and(|(rank, _)| {
            usize::try_from(rank.index).is_ok_and(|index| index < max_rank)
        });
    draw_button(
        frame,
        text,
        layout.promote,
        Label::Promote.text(language),
        mouse,
        can_promote,
    );
    draw_button(
        frame,
        text,
        layout.demote,
        Label::Demote.text(language),
        mouse,
        can_demote,
    );
    draw_button(
        frame,
        text,
        layout.kick,
        Label::Kick.text(language),
        mouse,
        can_kick && selected.is_some(),
    );
}

/// 仓库页（112 格 + 资金）。
#[allow(clippy::too_many_arguments)]
fn draw_storage(
    frame: &mut Frame,
    text: &UiText,
    layout: &GuildLayout,
    snapshot: &WorldSnapshot,
    gold_field: &TextField,
    focus: Option<FieldId>,
    mouse: Point,
    icon: &mut dyn FnMut(u16) -> Option<Image>,
    language: Language,
) {
    let gold = snapshot.guild.as_ref().map_or(0, |guild| guild.gold);
    text.draw_clipped(
        frame,
        layout.gold_row.x,
        layout.gold_row.y + 4,
        &format!(
            "{} {}",
            Label::GuildFunds.text(language),
            labels::format_number(u64::from(gold))
        ),
        SIZE_BODY,
        GOLD_COLOR,
        Rect::new(
            layout.gold_row.x,
            layout.gold_row.y + 2,
            layout.gold_row.width.saturating_sub(240),
            20,
        ),
    );
    input::draw(
        frame,
        text,
        layout.gold_field,
        gold_field,
        focus == Some(FieldId::GuildGold),
        Label::Amount.text(language),
    );
    let can_store = snapshot
        .guild
        .as_ref()
        .is_some_and(|guild| guild.allows(mir_core::enums::GuildRankOptions::CanStoreItem));
    let can_take = snapshot
        .guild
        .as_ref()
        .is_some_and(|guild| guild.allows(mir_core::enums::GuildRankOptions::CanRetrieveItem));
    draw_button(
        frame,
        text,
        layout.deposit,
        Label::Deposit.text(language),
        mouse,
        can_store,
    );
    draw_button(
        frame,
        text,
        layout.withdraw,
        Label::Withdraw.text(language),
        mouse,
        can_take,
    );
    grid::draw(
        frame,
        text,
        &layout.storage_grid,
        &snapshot.guild_storage,
        snapshot,
        None,
        None,
        None,
        icon,
    );
}

/// 增益页。
fn draw_buffs(
    frame: &mut Frame,
    text: &UiText,
    layout: &GuildLayout,
    snapshot: &WorldSnapshot,
    mouse: Point,
    language: Language,
) {
    let can_activate = snapshot
        .guild
        .as_ref()
        .is_some_and(crate::net::GuildView::can_activate_buff);
    for (visible_index, row) in layout.buff_rows.iter().enumerate() {
        let index = layout.first_buff + visible_index;
        let Some(info) = snapshot.guild_buff_infos.get(index) else {
            break;
        };
        if visible_index % 2 == 1 {
            frame.fill_rect(row.x, row.y, row.width, row.height, [26, 30, 38, 160]);
        }
        text.draw_clipped(
            frame,
            row.x + 6,
            row.y + 4,
            &info.name,
            SIZE_BODY,
            BODY_COLOR,
            Rect::new(row.x + 6, row.y + 4, 180, SIZE_BODY + 4),
        );
        text.draw_clipped(
            frame,
            row.x + 200,
            row.y + 5,
            &buff_state_text(snapshot, info, language),
            SIZE_SMALL,
            MUTED_COLOR,
            Rect::new(
                row.x + 200,
                row.y + 5,
                row.width.saturating_sub(300),
                SIZE_SMALL + 4,
            ),
        );
        text.draw_right(
            frame,
            row.right() - 92,
            row.y + 5,
            &format!("{}", info.activation_cost),
            SIZE_SMALL,
            GOLD_COLOR,
        );
        let obtained = snapshot.guild_active_buff(info.id).is_some();
        let active = snapshot
            .guild_active_buff(info.id)
            .is_some_and(|buff| buff.active);
        let label = if obtained {
            Label::Activate.text(language)
        } else {
            Label::Purchase.text(language)
        };
        // Crystal：已激活的增益不能再买（`BuffIsActive`），按钮置灰。
        draw_button(
            frame,
            text,
            layout.buff_buttons[visible_index],
            label,
            mouse,
            can_activate && !active,
        );
    }
}

/// 联盟页：本地记录的盟友列表 + 输入框 + 申请/同意/解除按钮。
///
/// 服务端没有「盟友名单」协议包（`/ally` 是聊天命令扩展），所以列表是**客户端
/// 本地台账**：申请加入「申请中」，同意/解除按输入的名字更新；真实状态以服务端
/// 聊天回执为准。
#[allow(clippy::too_many_arguments)]
fn draw_ally(
    frame: &mut Frame,
    text: &UiText,
    layout: &GuildLayout,
    ally_field: &TextField,
    allies: &[String],
    pending: Option<&str>,
    focus: Option<FieldId>,
    mouse: Point,
    language: Language,
) {
    let content = layout.content;
    text.draw_clipped(
        frame,
        content.x,
        content.y,
        Label::GuildAllyHint.text(language),
        SIZE_SMALL,
        MUTED_COLOR,
        Rect::new(content.x, content.y, content.width, 16),
    );
    let mut y = content.y + 18;
    let mut rows = 0;
    for name in allies.iter().take(10) {
        let row = Rect::new(content.x, y, content.width, 22);
        frame.fill_rect(row.x, row.y, row.width, row.height, [30, 34, 42, 220]);
        text.draw_clipped(
            frame,
            row.x + 6,
            row.y + 3,
            name,
            SIZE_SMALL,
            BODY_COLOR,
            Rect::new(row.x, row.y, row.width, row.height),
        );
        if pending == Some(name.as_str()) {
            text.draw_right(
                frame,
                row.right() - 6,
                row.y + 3,
                Label::GuildAllyPending.text(language),
                SIZE_SMALL,
                GOLD_COLOR,
            );
        }
        y += 22;
        rows += 1;
    }
    if let Some(pending) = pending
        && !allies.iter().any(|name| name == pending)
        && rows < 10
    {
        let row = Rect::new(content.x, y, content.width, 22);
        frame.fill_rect(row.x, row.y, row.width, row.height, [44, 40, 28, 220]);
        text.draw_clipped(
            frame,
            row.x + 6,
            row.y + 3,
            pending,
            SIZE_SMALL,
            BODY_COLOR,
            Rect::new(row.x, row.y, row.width.saturating_sub(80), row.height),
        );
        text.draw_right(
            frame,
            row.right() - 6,
            row.y + 3,
            Label::GuildAllyPending.text(language),
            SIZE_SMALL,
            GOLD_COLOR,
        );
    }
    if allies.is_empty() && pending.is_none() {
        text.draw_clipped(
            frame,
            content.x,
            content.y + 20,
            Label::GuildAllyEmpty.text(language),
            SIZE_SMALL,
            MUTED_COLOR,
            Rect::new(content.x, content.y, content.width, content.height),
        );
    }
    input::draw(
        frame,
        text,
        layout.ally_field,
        ally_field,
        focus == Some(FieldId::GuildAlly),
        Label::GuildAllyName.text(language),
    );
    for (rect, label) in [
        (layout.ally_request, Label::GuildAllyRequest),
        (layout.ally_accept, Label::GuildAllyAccept),
        (layout.ally_remove, Label::GuildAllyRemove),
    ] {
        let fill = if rect.contains(mouse) {
            [92, 82, 42, 255]
        } else {
            [52, 64, 84, 255]
        };
        frame.fill_rect(rect.x, rect.y, rect.width, rect.height, fill);
        text.draw_center(frame, rect, label.text(language), SIZE_SMALL, BODY_COLOR);
    }
}

/// 公告页。
#[allow(clippy::too_many_arguments)]
fn draw_notice(
    frame: &mut Frame,
    text: &UiText,
    layout: &GuildLayout,
    snapshot: &WorldSnapshot,
    notice_field: &TextField,
    focus: Option<FieldId>,
    mouse: Point,
    language: Language,
) {
    let area = layout.notice_area;
    let mut y = area.y + 4;
    if snapshot.guild_notice.is_empty() {
        text.draw_clipped(
            frame,
            area.x + 4,
            y,
            Label::Notice.text(language),
            SIZE_BODY,
            MUTED_COLOR,
            area,
        );
        y = y.saturating_add(18);
    }
    for line in &snapshot.guild_notice {
        if y + 14 > area.bottom() {
            break;
        }
        text.draw_clipped(
            frame,
            area.x + 4,
            y,
            line,
            SIZE_BODY,
            BODY_COLOR,
            Rect::new(area.x + 4, y, area.width.saturating_sub(8), SIZE_BODY + 4),
        );
        y = y.saturating_add(i32::try_from(SIZE_BODY + 4).unwrap_or(18));
    }
    let can_edit = snapshot
        .guild
        .as_ref()
        .is_some_and(crate::net::GuildView::can_change_notice);
    input::draw(
        frame,
        text,
        layout.notice_field,
        notice_field,
        focus == Some(FieldId::GuildNotice),
        Label::Notice.text(language),
    );
    draw_button(
        frame,
        text,
        layout.save_notice,
        Label::Save.text(language),
        mouse,
        can_edit,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_and_member_rows_are_hit_testable() {
        let frame = Rect::screen(800, 600);
        let layout = guild_layout(frame, 5, 3, 0, 0);
        for (index, rect) in layout.tabs.iter().enumerate() {
            assert_eq!(layout.tab_at(rect.center()), Some(index));
        }
        assert_eq!(layout.member_rows.len(), 5);
        for (index, row) in layout.member_rows.iter().enumerate() {
            assert!(layout.panel.contains(row.center()));
            assert_eq!(layout.member_at(row.center()), Some(index));
        }
        assert_eq!(layout.buff_rows.len(), 3);
    }

    #[test]
    fn member_and_buff_scrolling_is_clamped() {
        let frame = Rect::screen(800, 600);
        let layout = guild_layout(frame, 40, 30, 99, 99);
        assert_eq!(layout.member_rows.len(), MEMBER_ROWS);
        assert_eq!(layout.first_member, 40 - MEMBER_ROWS);
        assert_eq!(layout.first_buff, 30 - BUFF_ROWS);
    }

    #[test]
    fn storage_grid_has_one_hundred_and_twelve_slots() {
        let frame = Rect::screen(800, 600);
        let layout = guild_layout(frame, 0, 0, 0, 0);
        assert_eq!(layout.storage_grid.slots.len(), GUILD_STORAGE_SLOTS);
        assert_eq!(layout.storage_grid.max_scroll, 0);
    }

    #[test]
    fn summary_and_status_texts_are_localized() {
        let mut snapshot = WorldSnapshot::default();
        assert_eq!(summary_text(&snapshot, Language::ZhCn), "未加入行会");
        snapshot.guild = Some(crate::net::GuildView {
            name: "沙巴克".to_owned(),
            level: 3,
            member_count: 5,
            max_members: 20,
            gold: 10_000,
            ..crate::net::GuildView::default()
        });
        let text = summary_text(&snapshot, Language::ZhCn);
        assert!(text.contains("沙巴克"));
        assert!(text.contains("10,000"));
        assert_eq!(
            member_status_text(&GuildMemberView::default(), Language::ZhCn),
            "离线"
        );
        let online = GuildMemberView {
            online: true,
            ..GuildMemberView::default()
        };
        assert_eq!(member_status_text(&online, Language::En), "Online");
    }
}
