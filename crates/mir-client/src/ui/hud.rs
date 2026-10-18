//! HUD：顶部状态栏 + 左下角聊天面板。
//!
//! 状态栏画名字/等级、红色生命条、蓝色魔法条（都带数字）、金币和地图名；聊天面板
//! 画最近若干行聊天，并在 `T` 输入时显示带光标的输入行。所有文字走 [`UiText`]，
//! 中文优先；结构色板在亮/暗地图上都可读。

use mir_core::Language;

use crate::geom::Rect;
use crate::net::{ChatLine, WorldSnapshot};
use crate::ui::labels::{self, Label};
use crate::ui::text::UiText;

/// 状态栏高度。
pub const STATUS_HEIGHT: u32 = 52;
/// HUD 正文字号。
pub const SIZE_BODY: u32 = 14;
/// 小字字号（金币/提示）。
pub const SIZE_SMALL: u32 = 11;
/// 聊天面板默认宽度。
pub const CHAT_WIDTH: u32 = 430;
/// 聊天面板默认高度（含输入行）。
pub const CHAT_HEIGHT: u32 = 170;
/// 聊天面板离屏幕边缘的间距。
pub const CHAT_MARGIN: u32 = 8;
/// 聊天面板一次显示的最大行数。
pub const CHAT_VISIBLE_LINES: usize = 8;
/// 输入行高度。
pub const CHAT_INPUT_HEIGHT: u32 = 24;

/// 面板背景色（半透明深色；背包/商店/技能/对话共用）。
pub const PANEL_BACKGROUND: [u8; 4] = [14, 18, 26, 232];
/// 聊天面板背景（比其它面板更透，能看见脚下的地图）。
pub const CHAT_BACKGROUND: [u8; 4] = [10, 14, 22, 150];
/// 状态栏背景。
pub const STATUS_BACKGROUND: [u8; 4] = [12, 16, 24, 205];
/// 面板边框色。
pub const PANEL_BORDER: [u8; 4] = [82, 100, 124, 255];
/// 标题文字色。
pub const TITLE_COLOR: [u8; 4] = [246, 232, 170, 255];
/// 正文文字色。
pub const BODY_COLOR: [u8; 4] = [232, 236, 242, 255];
/// 次要文字色。
pub const MUTED_COLOR: [u8; 4] = [168, 178, 190, 255];
/// 金币文字色。
pub const GOLD_COLOR: [u8; 4] = [255, 216, 96, 255];
/// 生命条颜色。
pub const HP_COLOR: [u8; 4] = [204, 48, 48, 255];
/// 生命条背景色。
pub const HP_BACKGROUND: [u8; 4] = [64, 18, 20, 255];
/// 魔法条颜色。
pub const MP_COLOR: [u8; 4] = [52, 104, 224, 255];
/// 魔法条背景色。
pub const MP_BACKGROUND: [u8; 4] = [18, 26, 68, 255];
/// 地图名颜色。
pub const MAP_COLOR: [u8; 4] = [168, 224, 255, 255];

/// 状态栏的布局（绘制与测试共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusLayout {
    /// 整条背景。
    pub panel: Rect,
    /// 名字/等级区域。
    pub name: Rect,
    /// 生命条。
    pub hp_bar: Rect,
    /// 魔法条。
    pub mp_bar: Rect,
    /// 金币区域（右对齐）。
    pub gold: Rect,
    /// 地图名区域（右对齐）。
    pub map: Rect,
}

/// 计算状态栏布局。
#[must_use]
pub fn status_layout(frame: Rect) -> StatusLayout {
    let width = frame.width.max(320);
    StatusLayout {
        panel: Rect::new(0, 0, width, STATUS_HEIGHT),
        name: Rect::new(10, 5, 300, 18),
        hp_bar: Rect::new(10, 28, 190, 16),
        mp_bar: Rect::new(210, 28, 190, 16),
        gold: Rect::new(width.saturating_sub(180) as i32, 5, 170, 18),
        map: Rect::new(width.saturating_sub(330) as i32, 30, 320, 16),
    }
}

/// 画状态栏。
pub fn draw_status(
    frame: &mut mir_render::Frame,
    snapshot: &WorldSnapshot,
    text: &UiText,
    language: Language,
) {
    let layout = status_layout(Rect::screen(frame.width, frame.height));
    frame.fill_rect_alpha(
        layout.panel.x,
        layout.panel.y,
        layout.panel.width,
        layout.panel.height,
        STATUS_BACKGROUND,
    );
    frame.fill_rect(
        layout.panel.x,
        layout.panel.bottom() - 1,
        layout.panel.width,
        1,
        PANEL_BORDER,
    );

    let label_hp = Label::Hp.text(language);
    let label_mp = Label::Mp.text(language);
    if let Some(me) = &snapshot.self_player {
        text.draw_clipped(
            frame,
            layout.name.x,
            layout.name.y,
            &format!("{}  {} {}", me.name, Label::Level.text(language), me.level),
            SIZE_BODY,
            BODY_COLOR,
            layout.name,
        );
        let hp_text = format!("{label_hp} {}/{}", me.hp.max(0), me.hp_max.max(1));
        draw_bar(
            frame,
            layout.hp_bar,
            ratio(me.hp, me.hp_max),
            HP_BACKGROUND,
            HP_COLOR,
            &hp_text,
            text,
        );
        let mp_text = format!("{label_mp} {}/{}", me.mp.max(0), me.mp_max.max(1));
        draw_bar(
            frame,
            layout.mp_bar,
            ratio(me.mp, me.mp_max),
            MP_BACKGROUND,
            MP_COLOR,
            &mp_text,
            text,
        );
        text.draw_right(
            frame,
            layout.gold.right(),
            layout.gold.y + 2,
            &format!(
                "{} · {}",
                labels::gold_text(language, me.gold),
                labels::credit_text(language, me.credit)
            ),
            SIZE_SMALL,
            GOLD_COLOR,
        );
    } else {
        text.draw(
            frame,
            layout.name.x,
            layout.name.y,
            snapshot.status.label(),
            SIZE_BODY,
            BODY_COLOR,
        );
    }

    let map_name = map_name(snapshot, language);
    text.draw_right(
        frame,
        layout.map.right(),
        layout.map.y,
        &format!("{}: {map_name}", Label::Map.text(language)),
        SIZE_SMALL,
        MAP_COLOR,
    );
}

/// 地图标题：优先 `MapInformation.title`，其次文件名，最后「未知地图」。
#[must_use]
pub fn map_name(snapshot: &WorldSnapshot, language: Language) -> String {
    let Some(map) = &snapshot.map else {
        return Label::UnknownMap.text(language).to_owned();
    };
    if !map.title.is_empty() {
        return map.title.clone();
    }
    if !map.file_name.is_empty() {
        return map.file_name.clone();
    }
    Label::UnknownMap.text(language).to_owned()
}

/// 生命/魔法比例（0.0..=1.0）。
#[must_use]
pub fn ratio(current: i32, maximum: i32) -> f32 {
    if maximum <= 0 {
        return 0.0;
    }
    // 用 f64 中转，避免 `i32 as f32` 的精度告警；结果本身只用于进度条宽度。
    let current = f64::from(current.max(0));
    let maximum = f64::from(maximum);
    (current / maximum).clamp(0.0, 1.0) as f32
}

/// 画一条带数字的进度条。
fn draw_bar(
    frame: &mut mir_render::Frame,
    rect: Rect,
    ratio: f32,
    background: [u8; 4],
    fill: [u8; 4],
    label: &str,
    text: &UiText,
) {
    frame.fill_rect(rect.x, rect.y, rect.width, rect.height, [0, 0, 0, 255]);
    frame.fill_rect(
        rect.x + 1,
        rect.y + 1,
        rect.width.saturating_sub(2),
        rect.height.saturating_sub(2),
        background,
    );
    let filled = (f64::from(rect.width.saturating_sub(2)) * f64::from(ratio)).round() as u32;
    if filled > 0 {
        frame.fill_rect(
            rect.x + 1,
            rect.y + 1,
            filled,
            rect.height.saturating_sub(2),
            fill,
        );
    }
    text.draw_center(frame, rect, label, SIZE_SMALL, [255, 255, 255, 255]);
}

/// 队友血条面板：一行高度。
pub const PARTY_ROW_HEIGHT: u32 = 18;
/// 队友血条面板：最多显示多少行（多出来的不画，Crystal 的组队窗一屏也只放得下这些）。
pub const PARTY_MAX_ROWS: usize = 8;
/// 队友血条面板宽度。
pub const PARTY_WIDTH: u32 = 206;
/// 队友血条颜色（与实体头顶血条的分档一致）。
pub const PARTY_HP_WARN: [u8; 4] = [240, 200, 60, 255];
/// 队友血条低血量颜色。
pub const PARTY_HP_LOW: [u8; 4] = [220, 60, 60, 255];
/// 队友血条高血量颜色。
pub const PARTY_HP_GOOD: [u8; 4] = [96, 220, 96, 255];

/// 队友血条面板矩形：贴在聊天面板上方（左侧）。
///
/// 没有队伍时返回 `None`（不画）。队友数量超过 [`PARTY_MAX_ROWS`] 时只画前几行。
#[must_use]
pub fn party_layout(frame: Rect, member_count: usize) -> Option<Rect> {
    if member_count == 0 {
        return None;
    }
    let rows = u32::try_from(member_count.min(PARTY_MAX_ROWS)).unwrap_or(0);
    let chat = chat_layout(frame);
    let height = rows * PARTY_ROW_HEIGHT + 6;
    let width = PARTY_WIDTH.min(frame.width.saturating_sub(16)).max(120);
    Some(Rect::new(
        chat.x,
        chat.y - i32::try_from(height).unwrap_or(0) - 4,
        width,
        height,
    ))
}

/// 画队友血条（HUD）。
///
/// 只画**队友**（跳过自己：自己的血条在状态栏）。数据来自
/// [`WorldSnapshot::group_member_infos`]：名字/在线/血量百分比都由快照关联好，
/// 视野外的队友没有血量（画空条 + `--`）。
pub fn draw_party(
    frame: &mut mir_render::Frame,
    snapshot: &WorldSnapshot,
    text: &UiText,
    language: Language,
) {
    let members = snapshot.group_member_infos();
    let teammates = members.iter().filter(|member| !member.is_self).count();
    let Some(panel) = party_layout(Rect::screen(frame.width, frame.height), teammates) else {
        return;
    };
    frame.fill_rect_alpha(panel.x, panel.y, panel.width, panel.height, CHAT_BACKGROUND);
    frame.fill_rect(panel.x, panel.y, panel.width, 1, PANEL_BORDER);
    frame.fill_rect(panel.x, panel.bottom() - 1, panel.width, 1, PANEL_BORDER);
    frame.fill_rect(panel.x, panel.y, 1, panel.height, PANEL_BORDER);
    frame.fill_rect(panel.right() - 1, panel.y, 1, panel.height, PANEL_BORDER);
    let bar_x = panel.x + 84;
    let bar_width = panel.width.saturating_sub(84 + 40);
    let mut y = panel.y + 3;
    for member in members.iter().filter(|member| !member.is_self) {
        if y + i32::try_from(PARTY_ROW_HEIGHT).unwrap_or(0) > panel.bottom() {
            break;
        }
        text.draw_clipped(
            frame,
            panel.x + 5,
            y,
            &member.name,
            SIZE_SMALL,
            if member.online {
                crate::ui::group::GROUP_NAME_COLOR
            } else {
                MUTED_COLOR
            },
            Rect::new(panel.x + 5, y, 76, SIZE_SMALL + 4),
        );
        let bar = Rect::new(bar_x, y + 4, bar_width, 6);
        frame.fill_rect(bar.x, bar.y, bar.width, bar.height, HP_BACKGROUND);
        if let Some(percent) = member.hp_percent {
            let filled = bar.width * u32::from(percent.min(100)) / 100;
            if filled > 0 {
                let color = if percent > 50 {
                    PARTY_HP_GOOD
                } else if percent > 25 {
                    PARTY_HP_WARN
                } else {
                    PARTY_HP_LOW
                };
                frame.fill_rect(bar.x, bar.y, filled, bar.height, color);
            }
        }
        text.draw_right(
            frame,
            panel.right() - 5,
            y,
            &member.hp_percent.map_or_else(
                || Label::Empty.text(language).to_owned(),
                |p| format!("{p}%"),
            ),
            SIZE_SMALL,
            MUTED_COLOR,
        );
        y += i32::try_from(PARTY_ROW_HEIGHT).unwrap_or(0);
    }
}

/// 聊天面板矩形（左下角）。
#[must_use]
pub fn chat_layout(frame: Rect) -> Rect {
    let width = CHAT_WIDTH.min(frame.width.saturating_sub(CHAT_MARGIN * 2).max(160));
    let height = CHAT_HEIGHT.min(
        frame
            .height
            .saturating_sub(STATUS_HEIGHT + CHAT_MARGIN * 2)
            .max(80),
    );
    let x = i32::try_from(CHAT_MARGIN).unwrap_or(0);
    let y = frame.height as i32
        - i32::try_from(height).unwrap_or(0)
        - i32::try_from(CHAT_MARGIN).unwrap_or(0);
    Rect::new(x, y, width, height)
}

/// 聊天正文区域（输入行以上）。
#[must_use]
pub fn chat_lines_area(panel: Rect) -> Rect {
    let height = panel.height.saturating_sub(CHAT_INPUT_HEIGHT + 10).max(10);
    Rect::new(
        panel.x + 6,
        panel.y + 5,
        panel.width.saturating_sub(12),
        height,
    )
}

/// 聊天输入行区域。
#[must_use]
pub fn chat_input_area(panel: Rect) -> Rect {
    Rect::new(
        panel.x + 6,
        panel.bottom() - i32::try_from(CHAT_INPUT_HEIGHT).unwrap_or(0) - 4,
        panel.width.saturating_sub(12),
        CHAT_INPUT_HEIGHT,
    )
}

/// 取最近的 `max` 行聊天。
#[must_use]
pub fn visible_chat_lines(chat: &[ChatLine], max: usize) -> &[ChatLine] {
    let start = chat.len().saturating_sub(max);
    &chat[start..]
}

/// 画聊天面板。
pub fn draw_chat(
    frame: &mut mir_render::Frame,
    snapshot: &WorldSnapshot,
    text: &UiText,
    chat_input: Option<&str>,
    language: Language,
) {
    let panel = chat_layout(Rect::screen(frame.width, frame.height));
    frame.fill_rect_alpha(panel.x, panel.y, panel.width, panel.height, CHAT_BACKGROUND);
    frame.fill_rect(panel.x, panel.y, panel.width, 1, PANEL_BORDER);
    frame.fill_rect(panel.x, panel.bottom() - 1, panel.width, 1, PANEL_BORDER);
    frame.fill_rect(panel.x, panel.y, 1, panel.height, PANEL_BORDER);
    frame.fill_rect(panel.right() - 1, panel.y, 1, panel.height, PANEL_BORDER);

    let lines_area = chat_lines_area(panel);
    let line_height = text.line_height(SIZE_BODY).max(1);
    let max_lines = (lines_area.height / line_height).max(1);
    let max_lines = usize::try_from(max_lines)
        .unwrap_or(1)
        .min(CHAT_VISIBLE_LINES);
    let visible = visible_chat_lines(&snapshot.chat, max_lines);
    let total_height = u32::try_from(visible.len())
        .unwrap_or(0)
        .saturating_mul(line_height);
    let mut y = lines_area.bottom() - i32::try_from(total_height).unwrap_or(0);
    for line in visible {
        let color = labels::chat_color(line.kind);
        text.draw_clipped(
            frame,
            lines_area.x,
            y,
            &line.display(),
            SIZE_BODY,
            color,
            lines_area,
        );
        y = y.saturating_add(i32::try_from(line_height).unwrap_or(0));
    }

    let input = chat_input_area(panel);
    frame.fill_rect(input.x, input.y - 3, input.width, 1, [60, 72, 88, 255]);
    match chat_input {
        Some(value) => {
            let line = format!("> {value}|");
            text.draw_clipped(
                frame,
                input.x,
                input.y,
                &line,
                SIZE_BODY,
                [255, 255, 255, 255],
                input,
            );
        }
        None => {
            text.draw_clipped(
                frame,
                input.x,
                input.y,
                Label::ChatHint.text(language),
                SIZE_SMALL,
                MUTED_COLOR,
                input,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{ChatLine, WorldSnapshot};
    use mir_core::Language;
    use mir_core::enums::ChatType;

    fn sample_chat(count: usize) -> Vec<ChatLine> {
        (0..count)
            .map(|index| ChatLine {
                speaker: None,
                text: format!("第 {index} 行"),
                kind: ChatType::Normal,
            })
            .collect()
    }

    #[test]
    fn status_layout_keeps_bars_and_text_inside_the_panel() {
        let layout = status_layout(Rect::screen(800, 600));
        assert_eq!(layout.panel, Rect::new(0, 0, 800, STATUS_HEIGHT));
        assert!(layout.panel.contains_xy(layout.hp_bar.x, layout.hp_bar.y));
        assert!(layout.panel.contains(layout.mp_bar.center()));
        assert!(layout.panel.contains(layout.gold.center()));
        assert!(layout.panel.contains(layout.map.center()));
        assert!(
            layout.hp_bar.intersect(layout.mp_bar).is_none(),
            "HP/MP 条不应重叠"
        );
    }

    #[test]
    fn chat_layout_matches_the_bottom_left_corner() {
        let frame = Rect::screen(800, 600);
        let panel = chat_layout(frame);
        assert_eq!(panel.x, 8);
        assert_eq!(panel.bottom(), 592);
        assert!(chat_lines_area(panel).bottom() <= chat_input_area(panel).y);
        assert!(chat_input_area(panel).bottom() <= panel.bottom());
    }

    #[test]
    fn visible_chat_lines_are_capped_and_keep_the_newest() {
        let chat = sample_chat(20);
        let visible = visible_chat_lines(&chat, CHAT_VISIBLE_LINES);
        assert_eq!(visible.len(), CHAT_VISIBLE_LINES);
        assert_eq!(visible.last().unwrap().text, "第 19 行");
    }

    #[test]
    fn map_name_falls_back_from_title_to_file_to_unknown() {
        let mut snapshot = WorldSnapshot::default();
        assert_eq!(map_name(&snapshot, Language::ZhCn), "未知地图");
        snapshot.map = Some(crate::net::MapSnapshot {
            file_name: "0".to_owned(),
            title: String::new(),
            ..crate::net::MapSnapshot::default()
        });
        assert_eq!(map_name(&snapshot, Language::ZhCn), "0");
        snapshot.map = Some(crate::net::MapSnapshot {
            file_name: "0".to_owned(),
            title: "比奇省".to_owned(),
            ..crate::net::MapSnapshot::default()
        });
        assert_eq!(map_name(&snapshot, Language::ZhCn), "比奇省");
    }

    #[test]
    fn ratio_clamps_to_unit_interval() {
        assert!(ratio(0, 10).abs() < f32::EPSILON);
        assert!((ratio(5, 10) - 0.5).abs() < f32::EPSILON);
        assert!((ratio(20, 10) - 1.0).abs() < f32::EPSILON);
        assert!(ratio(5, 0).abs() < f32::EPSILON);
    }
}
