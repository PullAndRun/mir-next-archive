//! 物品悬浮提示（tooltip）：名字、类型、需求、攻防属性、持久度与堆叠。
//!
//! 数据来自快照里的 `S.NewItemInfo`（`ItemInfoView`）与 `UserInformation` 的物品实例；
//! 提示框跟随鼠标、并在贴到屏幕右/下边缘时翻到鼠标另一侧（[`layout`] 负责收边）。
//!
//! 绘制与测试共用同一份 [`TooltipLayout`]：无窗口测试先算矩形，再用它统计文字像素。

use mir_core::enums::Stat;
use mir_core::{Language, Point};
use mir_render::Frame;

use crate::geom::Rect;
use crate::net::{InventoryItem, WorldSnapshot};
use crate::ui::UiText;
use crate::ui::hud::{BODY_COLOR, GOLD_COLOR, MUTED_COLOR, SIZE_SMALL};
use crate::ui::labels::{self, Label};

/// 提示框内边距。
pub const PADDING: u32 = 6;
/// 行间距。
pub const LINE_GAP: u32 = 2;
/// 名字与正文之间的间距。
pub const TITLE_GAP: u32 = 3;
/// 鼠标右下偏移。
pub const MOUSE_OFFSET: i32 = 16;
/// 距离屏幕边缘的最小留白。
pub const EDGE_MARGIN: i32 = 4;
/// 提示框背景色（比普通面板更不透明，保证压在物品格上也能读）。
pub const BACKGROUND: [u8; 4] = [10, 12, 18, 242];
/// 提示框边框色。
pub const BORDER: [u8; 4] = [122, 138, 164, 255];

/// 提示框的一行文本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooltipLine {
    /// 文本。
    pub text: String,
    /// 颜色。
    pub color: [u8; 4],
}

/// 提示框内容（名字 + 若干行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooltipContent {
    /// 物品名。
    pub title: String,
    /// 名字颜色（按品级）。
    pub title_color: [u8; 4],
    /// 正文行。
    pub lines: Vec<TooltipLine>,
}

/// 计算后的提示框位置与尺寸。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooltipLayout {
    /// 背景矩形。
    pub panel: Rect,
    /// 名字行。
    pub title: TooltipLine,
    /// 正文行。
    pub lines: Vec<TooltipLine>,
}

/// 构造一件物品的提示内容。
///
/// 类型/需求/属性来自物品模板（`ItemInfo.stats`），持久度与堆叠来自物品实例。
#[must_use]
pub fn content_for_item(
    snapshot: &WorldSnapshot,
    item: &InventoryItem,
    language: Language,
) -> TooltipContent {
    let Some(info) = snapshot.item_info(item.item_index) else {
        return TooltipContent {
            title: labels::unknown_item(language, item.item_index),
            title_color: [236, 240, 246, 255],
            lines: Vec::new(),
        };
    };
    let mut lines = Vec::new();
    lines.push(TooltipLine {
        text: format!(
            "{} {}",
            Label::Type.text(language),
            labels::item_type_name(info.item_type, language)
        ),
        color: MUTED_COLOR,
    });
    if info.required_amount > 0 {
        lines.push(TooltipLine {
            text: format!(
                "{} {} {}",
                Label::Require.text(language),
                labels::required_type_name(info.required_type, language),
                info.required_amount
            ),
            color: MUTED_COLOR,
        });
    }
    for (label, min, max) in [
        (Label::Attack, Stat::MinDC, Stat::MaxDC),
        (Label::Defence, Stat::MinAC, Stat::MaxAC),
        (Label::MagicDefence, Stat::MinMAC, Stat::MaxMAC),
        (Label::MagicAttack, Stat::MinMC, Stat::MaxMC),
        (Label::Taoism, Stat::MinSC, Stat::MaxSC),
    ] {
        if let Some((low, high)) = info.stat_range(min, max) {
            lines.push(TooltipLine {
                text: format!(
                    "{} {}",
                    label.text(language),
                    super::equipment::range_text((low, high))
                ),
                color: [196, 226, 255, 255],
            });
        }
    }
    for (label, stat) in [
        (Label::Accuracy, Stat::Accuracy),
        (Label::Agility, Stat::Agility),
        (Label::AttackSpeed, Stat::AttackSpeed),
        (Label::Luck, Stat::Luck),
    ] {
        let value = info.stat(stat);
        if value != 0 {
            lines.push(TooltipLine {
                text: format!("{} +{value}", label.text(language)),
                color: [196, 226, 255, 255],
            });
        }
    }
    if item.max_dura > 0 || info.durability > 0 {
        lines.push(TooltipLine {
            text: format!(
                "{} {}/{}",
                Label::Durability.text(language),
                item.current_dura,
                item.max_dura.max(info.durability)
            ),
            color: BODY_COLOR,
        });
    }
    if info.stack_size > 1 {
        lines.push(TooltipLine {
            text: format!(
                "{} {}/{}",
                Label::Stack.text(language),
                item.count,
                info.stack_size
            ),
            color: BODY_COLOR,
        });
    }
    if info.price > 0 {
        lines.push(TooltipLine {
            text: format!(
                "{} {} {}",
                Label::Price.text(language),
                labels::format_number(u64::from(info.price)),
                Label::Gold.text(language)
            ),
            color: GOLD_COLOR,
        });
    }
    TooltipContent {
        title: info.name.clone(),
        title_color: labels::grade_color(info.grade),
        lines,
    }
}

/// 计算提示框矩形：跟随鼠标，越界时翻到另一侧并收边。
#[must_use]
pub fn layout(content: &TooltipContent, text: &UiText, frame: Rect, mouse: Point) -> TooltipLayout {
    let title_line = TooltipLine {
        text: content.title.clone(),
        color: content.title_color,
    };
    let title_width = text.text_width(&content.title, SIZE_SMALL);
    let mut widest = title_width;
    for line in &content.lines {
        widest = widest.max(text.text_width(&line.text, SIZE_SMALL));
    }
    let line_height = text.line_height(SIZE_SMALL).max(1);
    let rows = u32::try_from(content.lines.len()).unwrap_or(0);
    let height = PADDING * 2
        + line_height
        + if rows > 0 {
            TITLE_GAP + rows * (line_height + LINE_GAP) - LINE_GAP
        } else {
            0
        };
    let width = widest + PADDING * 2;
    let mut x = mouse.x + MOUSE_OFFSET;
    let mut y = mouse.y + MOUSE_OFFSET;
    if x + i32::try_from(width).unwrap_or(0) > frame.right() - EDGE_MARGIN {
        x = mouse.x - i32::try_from(width).unwrap_or(0) - MOUSE_OFFSET / 2;
    }
    if y + i32::try_from(height).unwrap_or(0) > frame.bottom() - EDGE_MARGIN {
        y = mouse.y - i32::try_from(height).unwrap_or(0) - MOUSE_OFFSET / 2;
    }
    x = x.clamp(
        EDGE_MARGIN,
        (frame.right() - i32::try_from(width).unwrap_or(0) - EDGE_MARGIN).max(EDGE_MARGIN),
    );
    y = y.clamp(
        EDGE_MARGIN,
        (frame.bottom() - i32::try_from(height).unwrap_or(0) - EDGE_MARGIN).max(EDGE_MARGIN),
    );
    TooltipLayout {
        panel: Rect::new(x, y, width, height),
        title: title_line,
        lines: content.lines.clone(),
    }
}

/// 画提示框。
pub fn draw(frame: &mut Frame, text: &UiText, layout: &TooltipLayout) {
    let panel = layout.panel;
    frame.fill_rect_alpha(
        panel.x + 2,
        panel.y + 2,
        panel.width,
        panel.height,
        [0, 0, 0, 90],
    );
    frame.fill_rect_alpha(panel.x, panel.y, panel.width, panel.height, BACKGROUND);
    frame.fill_rect(panel.x, panel.y, panel.width, 1, BORDER);
    frame.fill_rect(panel.x, panel.bottom() - 1, panel.width, 1, BORDER);
    frame.fill_rect(panel.x, panel.y, 1, panel.height, BORDER);
    frame.fill_rect(panel.right() - 1, panel.y, 1, panel.height, BORDER);
    let line_height = text.line_height(SIZE_SMALL).max(1);
    text.draw_clipped(
        frame,
        panel.x + i32::try_from(PADDING).unwrap_or(0),
        panel.y + i32::try_from(PADDING).unwrap_or(0),
        &layout.title.text,
        SIZE_SMALL,
        layout.title.color,
        panel,
    );
    let mut y = panel.y + i32::try_from(PADDING + line_height + TITLE_GAP).unwrap_or(0);
    for line in &layout.lines {
        text.draw_clipped(
            frame,
            panel.x + i32::try_from(PADDING).unwrap_or(0),
            y,
            &line.text,
            SIZE_SMALL,
            line.color,
            panel,
        );
        y = y.saturating_add(i32::try_from(line_height + LINE_GAP).unwrap_or(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::ItemInfoView;
    use mir_core::enums::{ItemGrade, ItemType, RequiredType};

    fn snapshot() -> WorldSnapshot {
        let mut snapshot = WorldSnapshot::default();
        snapshot.item_infos.insert(
            1,
            ItemInfoView {
                index: 1,
                name: "炼狱".to_owned(),
                item_type: ItemType::Weapon,
                required_type: RequiredType::Level,
                required_amount: 22,
                durability: 5_000,
                price: 1_200,
                grade: ItemGrade::Rare,
                stats: [
                    (Stat::MinDC, 5),
                    (Stat::MaxDC, 12),
                    (Stat::Accuracy, 2),
                    (Stat::Agility, 1),
                ]
                .into_iter()
                .collect(),
                ..ItemInfoView::default()
            },
        );
        snapshot
    }

    fn item() -> InventoryItem {
        InventoryItem {
            unique_id: 7,
            item_index: 1,
            count: 1,
            current_dura: 4_200,
            max_dura: 5_000,
        }
    }

    #[test]
    fn content_lists_type_requirement_stats_durability_and_price() {
        let content = content_for_item(&snapshot(), &item(), Language::ZhCn);
        assert_eq!(content.title, "炼狱");
        let texts: Vec<&str> = content
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect();
        assert!(texts.iter().any(|line| line.contains("武器")), "{texts:?}");
        assert!(
            texts.iter().any(|line| line.contains("等级 22")),
            "{texts:?}"
        );
        assert!(texts.iter().any(|line| line.contains("5-12")), "{texts:?}");
        assert!(
            texts.iter().any(|line| line.contains("准确 +2")),
            "{texts:?}"
        );
        assert!(
            texts.iter().any(|line| line.contains("4200/5000")),
            "{texts:?}"
        );
        assert!(texts.iter().any(|line| line.contains("价格")), "{texts:?}");
    }

    #[test]
    fn tooltip_flips_at_screen_edges_and_stays_inside() {
        let content = content_for_item(&snapshot(), &item(), Language::ZhCn);
        let text = UiText::fallback();
        let frame = Rect::screen(800, 600);
        let near = layout(&content, &text, frame, Point::new(780, 580));
        assert!(frame.contains_xy(near.panel.x, near.panel.y));
        assert!(near.panel.right() <= frame.right() - EDGE_MARGIN);
        assert!(near.panel.bottom() <= frame.bottom() - EDGE_MARGIN);
        let far = layout(&content, &text, frame, Point::new(100, 100));
        assert_eq!(far.panel.x, 100 + MOUSE_OFFSET);
        assert!(far.panel.width > 0 && far.panel.height > 0);
    }

    #[test]
    fn unknown_items_still_get_a_title() {
        let mut snapshot = WorldSnapshot::default();
        snapshot.item_infos.clear();
        let content = content_for_item(&snapshot, &item(), Language::ZhCn);
        assert_eq!(content.title, "未知物品 #1");
        assert!(content.lines.is_empty());
    }
}
