//! 任务面板（`Q`，Crystal `QuestDiaryDialog` / `Client/MirScenes/Dialogs/QuestDialogs.cs:642`）。
//!
//! 数据来自快照：任务定义（`S.NewQuestInfo`）+ 进度（`S.ChangeQuest`）。进度文本由服务端
//! 算好（`crates/mir-server/src/quests.rs` 的 `task_list`，对应 Crystal
//! `QuestProgressInfo.UpdateTasks`），例如「击杀 鹿 3/10」，客户端原样显示。
//!
//! 行为：
//! * 列出**进行中**任务（名字、目标进度、限时剩余）；
//! * 目标已达成（`QuestView::completed`）的任务名变绿并标「已完成 · 去交付」；
//! * 每行右侧的「放弃」按钮发 `C.AbandonQuest { quest_index }`；
//! * 任务多于可见行时用滚轮滚动（[`MAX_QUEST_ROWS`] 行）。
//!
//! 简化：
//! * Crystal 的任务日记按 `QuestInfo.Group` 折叠分组（`QuestDialogs.cs:716-760`），本作按
//!   接取顺序平铺，行里显示分组名；
//! * Crystal 的任务面板还有「详情页/奖励选择/共享/追踪」四个控件
//!   （`QuestListDialog`、`QuestDetailDialog`、`QuestTrackingDialog`），本作只做需求里的
//!   「进度 + 放弃 + 完成提示」；
//! * 协议不带任务接取时间，限时剩余只能从客户端第一次看到任务开始算
//!   （见 [`crate::net::QuestView::accepted_at_ms`]）。

use mir_core::Language;
use mir_render::{Frame, Image, scale_to_fit};

use crate::geom::Rect;
use crate::net::{QuestView, WorldSnapshot};
use crate::ui::PanelContext;
use crate::ui::UiText;
use crate::ui::hud::{BODY_COLOR, GOLD_COLOR, MUTED_COLOR, SIZE_BODY, SIZE_SMALL, TITLE_COLOR};
use crate::ui::labels::Label;

/// 面板宽度。
pub const PANEL_WIDTH: u32 = 468;
/// 每行高度。
pub const ROW_HEIGHT: u32 = 46;
/// 一屏最多显示多少行任务。
pub const MAX_QUEST_ROWS: usize = 7;
/// 标题高度。
pub const TITLE_HEIGHT: u32 = 24;
/// 顶部计数行高度。
pub const HEADER_HEIGHT: u32 = 18;
/// 底部提示高度。
pub const FOOTER_HEIGHT: u32 = 18;
/// 「任务物品」区高度。
pub const QUEST_ITEMS_HEIGHT: u32 = 34;
/// 任务物品图标边长。
pub const QUEST_ITEM_ICON: u32 = 24;
/// 任务物品条左侧标题预留宽度（「任务物品」/「Quest items」）。
pub const QUEST_ITEMS_LABEL_WIDTH: u32 = 96;
/// 每个任务物品占用的横向步进（图标 + 间距）。
pub const QUEST_ITEM_STEP: u32 = 32;
/// 内边距。
pub const PADDING: u32 = 8;
/// 「放弃」按钮宽度。
pub const ABANDON_WIDTH: u32 = 48;
/// 「放弃」按钮高度。
pub const ABANDON_HEIGHT: u32 = 20;
/// 完成任务的文字色（Crystal 用绿色表示可交付）。
pub const COMPLETE_COLOR: [u8; 4] = [140, 240, 150, 255];
/// 新任务标记色。
pub const NEW_COLOR: [u8; 4] = [255, 236, 140, 255];
/// 过期（剩余 0）的颜色。
pub const EXPIRED_COLOR: [u8; 4] = [232, 96, 96, 255];

/// 一行任务的可点击区域。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestRow {
    /// 整行（点它选中任务）。
    pub row: Rect,
    /// 「放弃」按钮。
    pub abandon: Rect,
}

/// 任务面板布局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestLayout {
    /// 面板矩形。
    pub panel: Rect,
    /// 可见行。
    pub rows: Vec<QuestRow>,
    /// 第一条可见任务在 `snapshot.quests` 里的下标。
    pub first_index: usize,
    /// 底部提示区域。
    pub footer: Rect,
    /// 「任务物品」区（`S.GainedQuestItem` / `S.DeleteQuestItem` 的列表）。
    pub items: Rect,
    /// 任务物品区一屏能放几个（按面板宽度算；40 格时需要滚动）。
    pub item_capacity: usize,
    /// 最大滚动行数（0 = 不需要滚动）。
    pub max_scroll: usize,
}

impl QuestLayout {
    /// 命中测试：点在哪个任务行上（返回 `snapshot.quests` 的绝对下标）。
    #[must_use]
    pub fn row_at(&self, point: mir_core::Point) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.row.contains(point))
            .map(|index| self.first_index + index)
    }

    /// 命中测试：点在哪个「放弃」按钮上（返回绝对下标）。
    #[must_use]
    pub fn abandon_at(&self, point: mir_core::Point) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.abandon.contains(point))
            .map(|index| self.first_index + index)
    }
}

/// 计算任务面板布局（居中偏左，与 Crystal 的任务日记位置接近）。
///
/// 小窗口（320×240）按可用高度**减少可见行**，保证行与「任务物品」区都在屏内：
/// 预算高度 = `frame.height - 70`（与面板最大高度一致），先扣标题/计数/任务物品/
/// 底部提示/内边距，剩下的才分给任务行。
#[must_use]
pub fn quest_layout(frame: Rect, quest_count: usize, scroll: usize) -> QuestLayout {
    let width = PANEL_WIDTH.min(frame.width.saturating_sub(32)).max(240);
    let max_height = frame.height.saturating_sub(70).max(160);
    let overhead = TITLE_HEIGHT + HEADER_HEIGHT + QUEST_ITEMS_HEIGHT + FOOTER_HEIGHT + PADDING * 2;
    let capacity = usize::try_from(max_height.saturating_sub(overhead) / ROW_HEIGHT)
        .unwrap_or(1)
        .max(1);
    let visible = quest_count.min(MAX_QUEST_ROWS).min(capacity);
    let rows_height = u32::try_from(visible.max(1)).unwrap_or(1) * ROW_HEIGHT;
    let height = (overhead + rows_height).min(max_height).max(160);
    // 小窗口：宽度先夹到屏内，x 再按右边界回退（320 宽时 40+288 会出屏）。
    let x = (frame.x + 40)
        .min(frame.right() - i32::try_from(width).unwrap_or(0) - 8)
        .max(frame.x + 8);
    let y = frame.y + 60;
    let panel = Rect::new(x, y, width, height);
    let max_scroll = quest_count.saturating_sub(visible.max(1));
    let first_index = scroll.min(max_scroll);
    let mut rows = Vec::with_capacity(visible);
    for index in 0..visible {
        let row = Rect::new(
            panel.x + i32::try_from(PADDING).unwrap_or(0),
            panel.y
                + i32::try_from(TITLE_HEIGHT + HEADER_HEIGHT).unwrap_or(0)
                + i32::try_from(index).unwrap_or(0) * i32::try_from(ROW_HEIGHT).unwrap_or(0),
            panel.width.saturating_sub(PADDING * 2),
            ROW_HEIGHT,
        );
        let abandon = Rect::new(
            row.right() - i32::try_from(ABANDON_WIDTH + 4).unwrap_or(0),
            row.y + 4,
            ABANDON_WIDTH,
            ABANDON_HEIGHT,
        );
        rows.push(QuestRow { row, abandon });
    }
    let items = Rect::new(
        panel.x + i32::try_from(PADDING).unwrap_or(0),
        panel.y
            + i32::try_from(TITLE_HEIGHT + HEADER_HEIGHT).unwrap_or(0)
            + i32::try_from(rows_height).unwrap_or(0),
        panel.width.saturating_sub(PADDING * 2),
        QUEST_ITEMS_HEIGHT,
    );
    let item_capacity =
        usize::try_from(items.width.saturating_sub(QUEST_ITEMS_LABEL_WIDTH + 8) / QUEST_ITEM_STEP)
            .unwrap_or(1)
            .max(1);
    let footer = Rect::new(
        panel.x + i32::try_from(PADDING).unwrap_or(0),
        panel.bottom() - i32::try_from(FOOTER_HEIGHT + 4).unwrap_or(0),
        panel.width.saturating_sub(PADDING * 2),
        FOOTER_HEIGHT,
    );
    QuestLayout {
        panel,
        rows,
        first_index,
        footer,
        items,
        item_capacity,
        max_scroll,
    }
}

/// 取任务追踪里的目标行（服务端第 42 轮起在 `task_list` 首行写入
/// `前往 <地图>(<file>) <NPC> (<x>,<y>)`；找不到时返回 `None`）。
#[must_use]
pub fn target_line(quest: &QuestView) -> Option<&str> {
    quest
        .task_list
        .iter()
        .find(|line| line.trim_start().starts_with("前往"))
        .map(String::as_str)
}

/// 一行的状态文案（测试与绘制共用）。
#[must_use]
pub fn status_text(quest: &QuestView, language: Language) -> String {
    if quest.ready_to_return() {
        return Label::QuestComplete.text(language).to_owned();
    }
    if quest.taken {
        Label::QuestInProgress.text(language).to_owned()
    } else {
        Label::QuestAvailable.text(language).to_owned()
    }
}

/// 限时剩余文案（`剩余 12:34`）；不限时返回空串。
#[must_use]
pub fn time_left_text(quest: &QuestView, now_ms: u64, language: Language) -> String {
    let Some(seconds) = quest.remaining_seconds(now_ms) else {
        return String::new();
    };
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    format!(
        "{} {:02}:{:02}",
        Label::TimeLeft.text(language),
        minutes,
        seconds
    )
}

/// 一行任务的名字（缺定义时按当前语言显示 `任务 #n`）。
#[must_use]
pub fn quest_name(quest: &QuestView, language: Language) -> String {
    if quest.name.is_empty() {
        format!("{} #{}", Label::Quests.text(language), quest.id)
    } else {
        quest.name.clone()
    }
}

/// 任务标题行：`名字 · 分组`（分组为空时只有名字）。
#[must_use]
pub fn title_text(quest: &QuestView, language: Language) -> String {
    let name = quest_name(quest, language);
    if quest.group.is_empty() {
        name
    } else {
        format!("{name} · {}", quest.group)
    }
}

/// 画任务面板。
#[allow(clippy::too_many_arguments)]
pub fn draw(
    frame: &mut Frame,
    layout: &QuestLayout,
    context: &PanelContext<'_>,
    snapshot: &WorldSnapshot,
    selected: Option<i32>,
    hover: Option<usize>,
    mouse: mir_core::Point,
    now_ms: u64,
    item_scroll: usize,
    icon: &mut dyn FnMut(u16) -> Option<Image>,
) {
    let text = context.text;
    let language = context.language;
    crate::ui::draw_panel(frame, layout.panel);
    text.draw_clipped(
        frame,
        layout.panel.x + 10,
        layout.panel.y + 4,
        &format!("{} (Q)", Label::QuestDiary.text(language)),
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
        &format!("{}", snapshot.quests.len()),
        SIZE_SMALL,
        MUTED_COLOR,
    );
    if snapshot.quests.is_empty() {
        text.draw_center(
            frame,
            Rect::new(
                layout.panel.x,
                layout.panel.y + i32::try_from(TITLE_HEIGHT + HEADER_HEIGHT).unwrap_or(0),
                layout.panel.width,
                ROW_HEIGHT,
            ),
            Label::NoQuests.text(language),
            SIZE_BODY,
            MUTED_COLOR,
        );
    } else {
        for (visible_index, row_layout) in layout.rows.iter().enumerate() {
            let index = layout.first_index + visible_index;
            let Some(quest) = snapshot.quests.get(index) else {
                break;
            };
            let row = row_layout.row;
            let is_selected = selected == Some(quest.id);
            let is_hover = hover == Some(index);
            if is_selected {
                frame.fill_rect(row.x, row.y, row.width, row.height, [86, 74, 32, 235]);
                frame.fill_rect(row.x, row.y, 2, row.height, [255, 216, 96, 255]);
            } else if is_hover {
                frame.fill_rect(row.x, row.y, row.width, row.height, [44, 56, 76, 235]);
                frame.fill_rect(row.x, row.y, 2, row.height, [128, 176, 240, 255]);
            } else if visible_index % 2 == 1 {
                frame.fill_rect(row.x, row.y, row.width, row.height, [26, 30, 38, 160]);
            }
            // 名字：完成/新任务用不同颜色（Crystal 的任务列表同样按状态着色）。
            let name_color = if quest.ready_to_return() {
                COMPLETE_COLOR
            } else if quest.new {
                NEW_COLOR
            } else {
                BODY_COLOR
            };
            let name_area = Rect::new(
                row.x + 8,
                row.y + 3,
                row.width.saturating_sub(ABANDON_WIDTH + 20),
                SIZE_BODY + 4,
            );
            text.draw_clipped(
                frame,
                name_area.x,
                name_area.y,
                &title_text(quest, language),
                SIZE_BODY,
                name_color,
                name_area,
            );
            // 目标进度：逐行显示服务端算好的文本（最多两行，超出裁掉）。
            let mut task_y = name_area.y + i32::try_from(SIZE_BODY + 2).unwrap_or(16);
            for line in quest.task_list.iter().take(2) {
                text.draw_clipped(
                    frame,
                    row.x + 8,
                    task_y,
                    line,
                    SIZE_SMALL,
                    if quest.ready_to_return() {
                        COMPLETE_COLOR
                    } else {
                        MUTED_COLOR
                    },
                    Rect::new(
                        row.x + 8,
                        task_y,
                        row.width.saturating_sub(ABANDON_WIDTH + 20),
                        SIZE_SMALL + 3,
                    ),
                );
                task_y = task_y.saturating_add(i32::try_from(SIZE_SMALL + 2).unwrap_or(13));
            }
            // 右下角：状态 + 限时剩余。
            let mut status = status_text(quest, language);
            let left = time_left_text(quest, now_ms, language);
            if !left.is_empty() {
                status.push_str(" · ");
                status.push_str(&left);
            }
            let status_color = if quest.remaining_seconds(now_ms) == Some(0) {
                EXPIRED_COLOR
            } else if quest.ready_to_return() {
                COMPLETE_COLOR
            } else {
                GOLD_COLOR
            };
            text.draw_right(
                frame,
                row.right() - 8,
                row.bottom() - i32::try_from(SIZE_SMALL + 4).unwrap_or(15),
                &status,
                SIZE_SMALL,
                status_color,
            );
            // 「放弃」按钮。
            let button = row_layout.abandon;
            let hovered = button.contains(mouse);
            let fill = if hovered {
                [120, 56, 56, 255]
            } else {
                [64, 40, 44, 255]
            };
            frame.fill_rect(button.x, button.y, button.width, button.height, fill);
            frame.fill_rect(button.x, button.y, button.width, 1, [140, 100, 100, 255]);
            frame.fill_rect(
                button.x,
                button.bottom() - 1,
                button.width,
                1,
                [48, 32, 34, 255],
            );
            text.draw_center(
                frame,
                button,
                Label::Abandon.text(language),
                SIZE_SMALL,
                [255, 220, 200, 255],
            );
        }
    }
    draw_quest_items(frame, layout, snapshot, text, language, item_scroll, icon);
    text.draw_clipped(
        frame,
        layout.footer.x,
        layout.footer.y,
        Label::QuestHint.text(language),
        SIZE_SMALL,
        MUTED_COLOR,
        layout.footer,
    );
    if layout.max_scroll > 0 {
        text.draw_right(
            frame,
            layout.panel.right() - 10,
            layout.footer.y,
            &format!("{}/{}", layout.first_index + 1, snapshot.quests.len()),
            SIZE_SMALL,
            MUTED_COLOR,
        );
    }
}

/// 画「任务物品」区：标题 + 图标 + 数量（图标来自 `items.wzl`，与背包共用图库）。
///
/// 40 格任务物品栏放不下一屏：`item_scroll` 指定从第几件开始画，
/// 右侧显示 `当前页/总页` 和「滚轮翻页」提示（`MouseWheel` 在物品条上时翻页）。
fn draw_quest_items(
    frame: &mut Frame,
    layout: &QuestLayout,
    snapshot: &WorldSnapshot,
    text: &UiText,
    language: Language,
    item_scroll: usize,
    icon: &mut dyn FnMut(u16) -> Option<Image>,
) {
    let strip = layout.items;
    frame.fill_rect_alpha(
        strip.x,
        strip.y,
        strip.width,
        strip.height,
        [24, 28, 34, 220],
    );
    frame.fill_rect(strip.x, strip.y, strip.width, 1, [70, 78, 92, 255]);
    let title = Label::QuestItems.text(language);
    text.draw_clipped(
        frame,
        strip.x + 6,
        strip.y + 9,
        title,
        SIZE_SMALL,
        GOLD_COLOR,
        Rect::new(strip.x + 6, strip.y + 9, 80, 14),
    );
    let mut x = strip.x + 10 + i32::try_from(text.text_width(title, SIZE_SMALL)).unwrap_or(80);
    if snapshot.quest_items.is_empty() {
        text.draw_clipped(
            frame,
            x,
            strip.y + 9,
            Label::Empty.text(language),
            SIZE_SMALL,
            MUTED_COLOR,
            Rect::new(
                x,
                strip.y + 9,
                u32::try_from((strip.right() - x - 4).max(0)).unwrap_or(0),
                14,
            ),
        );
        return;
    }
    // 40 格放不下一屏：按容量分页，`item_scroll` 是起始下标（页首）。
    let capacity = layout.item_capacity.max(1);
    let max_scroll = snapshot.quest_items.len().saturating_sub(capacity);
    let first = item_scroll.min(max_scroll);
    let right_limit = strip.right() - if max_scroll > 0 { 110 } else { 20 };
    for item in snapshot.quest_items.iter().skip(first).take(capacity) {
        if x + i32::try_from(QUEST_ITEM_ICON).unwrap_or(24) > right_limit {
            break;
        }
        let loaded = snapshot.item_image(item.item_index).and_then(&mut *icon);
        if let Some(image) = loaded {
            let image = scale_to_fit(&image, QUEST_ITEM_ICON, QUEST_ITEM_ICON);
            frame.blit(&image, x, strip.y + 5);
        } else {
            // 素材缺失时画一个占位方块（与背包/商店的缺图表现一致，不吞掉条目）。
            let size = i32::try_from(QUEST_ITEM_ICON).unwrap_or(24);
            frame.fill_rect(
                x,
                strip.y + 5,
                u32::try_from(size).unwrap_or(0),
                u32::try_from(size).unwrap_or(0),
                [40, 44, 52, 255],
            );
            frame.fill_rect(
                x,
                strip.y + 5,
                u32::try_from(size).unwrap_or(0),
                1,
                [90, 96, 108, 255],
            );
        }
        let mut label = String::new();
        if item.count > 1 {
            label = format!("×{}", item.count);
        }
        if !label.is_empty() {
            let icon_size = i32::try_from(QUEST_ITEM_ICON).unwrap_or(24);
            text.draw_clipped(
                frame,
                x + icon_size - 2,
                strip.y + 16,
                &label,
                SIZE_SMALL,
                [240, 240, 240, 255],
                Rect::new(x + icon_size - 2, strip.y + 16, 26, 12),
            );
        }
        x += i32::try_from(QUEST_ITEM_ICON).unwrap_or(24) + 8;
    }
    if max_scroll > 0 {
        let pages = snapshot.quest_items.len().div_ceil(capacity);
        let page = first / capacity + 1;
        let hint = format!(
            "{page}/{pages} {}",
            Label::QuestItemsScrollHint.text(language)
        );
        text.draw_right(
            frame,
            strip.right() - 6,
            strip.y + 9,
            &hint,
            SIZE_SMALL,
            MUTED_COLOR,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir_core::Point;

    fn quest(id: i32, completed: bool) -> QuestView {
        QuestView {
            id,
            name: format!("任务{id}"),
            group: "新手".to_owned(),
            task_list: vec!["击杀 鹿 3/10".to_owned()],
            taken: true,
            completed,
            new: false,
            time_limit_seconds: 600,
            accepted_at_ms: Some(1_000),
        }
    }

    #[test]
    fn layout_rows_are_inside_the_panel_and_hit_testable() {
        let frame = Rect::screen(800, 600);
        let layout = quest_layout(frame, 3, 0);
        assert_eq!(layout.rows.len(), 3);
        for (index, row) in layout.rows.iter().enumerate() {
            assert!(layout.panel.contains(row.row.center()));
            assert_eq!(layout.row_at(row.row.center()), Some(index));
            assert_eq!(layout.abandon_at(row.abandon.center()), Some(index));
        }
        assert_eq!(layout.abandon_at(Point::new(0, 0)), None);
        assert_eq!(layout.max_scroll, 0);
    }

    #[test]
    fn small_screen_keeps_items_and_footer_inside_the_panel() {
        let screen = Rect::screen(320, 240);
        let layout = quest_layout(screen, 3, 0);
        assert!(layout.panel.right() <= screen.right(), "面板不能出右边界");
        assert!(layout.panel.bottom() <= screen.bottom(), "面板不能出下边界");
        assert!(layout.rows.len() < 3, "矮窗口要减少可见行");
        assert!(
            layout.panel.contains(layout.items.center()),
            "任务物品区越界"
        );
        assert!(
            layout.panel.contains(layout.footer.center()),
            "底部提示越界"
        );
        assert!(layout.items.bottom() <= layout.footer.y);
        // 320 宽一屏放得下 5 件（40 格需要 8 页）；800 宽更多。
        assert!(layout.item_capacity >= 3);
        assert!(quest_layout(Rect::screen(800, 600), 3, 0).item_capacity > layout.item_capacity);
    }

    #[test]
    fn scrolling_is_clamped_to_the_last_page() {
        let frame = Rect::screen(800, 600);
        let layout = quest_layout(frame, 20, 99);
        assert_eq!(layout.rows.len(), MAX_QUEST_ROWS);
        assert_eq!(layout.max_scroll, 20 - MAX_QUEST_ROWS);
        assert_eq!(layout.first_index, 20 - MAX_QUEST_ROWS);
        assert_eq!(
            layout.row_at(layout.rows[0].row.center()),
            Some(layout.first_index)
        );
    }

    #[test]
    fn status_and_time_texts_are_localized() {
        let active = quest(7, false);
        assert_eq!(status_text(&active, Language::ZhCn), "进行中");
        assert_eq!(status_text(&active, Language::En), "In progress");
        let completed = quest(7, true);
        assert_eq!(status_text(&completed, Language::ZhCn), "已完成 · 去交付");
        assert_eq!(
            time_left_text(&active, 1_000 + 60_000, Language::ZhCn),
            "剩余 09:00"
        );
        let unlimited = QuestView {
            time_limit_seconds: 0,
            ..active.clone()
        };
        assert_eq!(time_left_text(&unlimited, 1_000, Language::ZhCn), "");
        assert_eq!(quest_name(&active, Language::ZhCn), "任务7");
    }

    #[test]
    fn task_panel_target_line_is_exposed_for_tracking() {
        let mut tracking = quest(1, false);
        tracking
            .task_list
            .insert(0, "前往 比奇省(0) 工艺夫人 (294,619)".to_owned());
        assert_eq!(
            target_line(&tracking),
            Some("前往 比奇省(0) 工艺夫人 (294,619)")
        );
        // 面板绘制循环读取 task_list 的前两行；服务端把目标行放首行，保证可见。
        assert_eq!(
            tracking.task_list.iter().take(2).next().map(String::as_str),
            target_line(&tracking)
        );
    }
}
