//! NPC 对话页：解析 `<文本/@跳转键>` 按钮标记、排版正文与可点击按钮。
//!
//! Crystal 的 NPC 对话框把按钮写成行内标记（`Client/MirScenes/Dialogs/NPCDialogs.cs:18`
//! 的 `R = <((.*?)\/(\@.*?))>`），例如 `<买东西/@BUY>`。本模块把这些标记从正文里摘出来，
//! 变成正文下方的按钮行；点击按钮时 UI 发 `C.CallNPC { object_id, key }`。
//!
//! 没有 `@exit` 按钮时自动补一个「关闭」，保证客户端始终有办法关窗（`Esc` 同样发
//! `C.CallNPC` + `@exit`）。

use crate::geom::Rect;
use crate::ui::labels::Label;
use mir_core::Language;

/// 对话面板最大宽度。
pub const DIALOG_MAX_WIDTH: u32 = 470;
/// 关闭对话的页键（`C.CallNPC` 的 `@exit`；服务端页面里的 `<关闭/@exit>`）。
pub const EXIT_KEY: &str = "@exit";
/// 正文行高。
pub const LINE_HEIGHT: u32 = 18;
/// 标题区高度。
pub const TITLE_HEIGHT: u32 = 24;
/// 按钮高度。
pub const BUTTON_HEIGHT: u32 = 26;
/// 按钮横向间距。
pub const BUTTON_GAP: u32 = 6;
/// 按钮最小宽度。
pub const BUTTON_MIN_WIDTH: u32 = 56;

/// 一个可点击按钮。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogButton {
    /// 按钮文字。
    pub label: String,
    /// 发给服务端的页键（含 `@`）。
    pub key: String,
}

/// 解析后的对话页。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedPage {
    /// 去掉按钮/颜色标记后的正文行。
    pub lines: Vec<String>,
    /// 页面上出现的按钮（按出现顺序，按键去重）。
    pub buttons: Vec<DialogButton>,
}

impl ParsedPage {
    /// 解析服务端发来的原始行。
    #[must_use]
    pub fn parse(lines: &[String]) -> Self {
        let mut page = Self::default();
        for line in lines {
            let (cleaned, buttons) = parse_line(line);
            for button in buttons {
                if page
                    .buttons
                    .iter()
                    .any(|existing| existing.key.eq_ignore_ascii_case(&button.key))
                {
                    continue;
                }
                page.buttons.push(button);
            }
            if !cleaned.trim().is_empty() {
                page.lines.push(cleaned.trim_end().to_owned());
            }
        }
        if !page
            .buttons
            .iter()
            .any(|button| button.key.eq_ignore_ascii_case(EXIT_KEY))
        {
            page.buttons.push(DialogButton {
                // 文案在绘制时按语言取；这里用中文默认值，UI 会在绘制前替换。
                label: String::new(),
                key: EXIT_KEY.to_owned(),
            });
        }
        page
    }

    /// 页面是否没有任何正文与按钮（理论上只剩自动补的关闭按钮）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.buttons.is_empty()
    }
}

/// 对话面板布局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogLayout {
    /// 面板矩形。
    pub panel: Rect,
    /// 正文区域。
    pub lines_area: Rect,
    /// 每个按钮的矩形（与 [`ParsedPage::buttons`] 同序）。
    pub buttons: Vec<Rect>,
}

impl DialogLayout {
    /// 命中测试：点在哪个按钮上。
    #[must_use]
    pub fn button_index_at(&self, point: mir_core::Point) -> Option<usize> {
        self.buttons.iter().position(|rect| rect.contains(point))
    }
}

/// 计算对话面板布局。
///
/// `measure` 把按钮文字量成像素宽；绘制与命中测试必须传同一个量宽函数，否则
/// 按钮会「看着在这里、点不到」。
#[must_use]
pub fn dialog_layout(
    frame: Rect,
    page: &ParsedPage,
    measure: impl Fn(&str) -> u32,
) -> DialogLayout {
    let width = DIALOG_MAX_WIDTH
        .min(frame.width.saturating_sub(32))
        .max(220);
    let content_width = width.saturating_sub(24);
    let button_widths: Vec<u32> = page
        .buttons
        .iter()
        .map(|button| button_width(&button.label, &measure))
        .collect();
    let rows = button_rows(&button_widths, content_width);
    let text_height = u32::try_from(page.lines.len())
        .unwrap_or(0)
        .saturating_mul(LINE_HEIGHT);
    let buttons_height = if page.buttons.is_empty() {
        0
    } else {
        rows.saturating_mul(BUTTON_HEIGHT)
            .saturating_add(rows.saturating_sub(1).saturating_mul(BUTTON_GAP))
    };
    let mut height = TITLE_HEIGHT + 8 + text_height.max(LINE_HEIGHT) + 8;
    if !page.buttons.is_empty() {
        height = height.saturating_add(buttons_height + 8);
    }
    height = height.max(130).min(frame.height.max(130));

    let x =
        frame.x + (i32::try_from(frame.width).unwrap_or(0) - i32::try_from(width).unwrap_or(0)) / 2;
    let preferred = frame.y
        + ((i32::try_from(frame.height).unwrap_or(0) - i32::try_from(height).unwrap_or(0)) * 2 / 3);
    let y = preferred
        .min(frame.bottom() - i32::try_from(height).unwrap_or(0) - 8)
        .max(frame.y + 58);
    let panel = Rect::new(x, y, width, height);
    let lines_area = Rect::new(
        panel.x + 12,
        panel.y + i32::try_from(TITLE_HEIGHT + 4).unwrap_or(0),
        content_width,
        text_height.max(LINE_HEIGHT),
    );
    let buttons_top = panel.bottom() - 10 - i32::try_from(buttons_height).unwrap_or(0);
    let mut buttons = Vec::with_capacity(page.buttons.len());
    let mut row_y = buttons_top;
    let mut row_x = 0u32;
    for width in button_widths {
        if row_x > 0 && row_x.saturating_add(width) > content_width {
            row_x = 0;
            row_y = row_y.saturating_add(i32::try_from(BUTTON_HEIGHT + BUTTON_GAP).unwrap_or(0));
        }
        buttons.push(Rect::new(
            panel.x + 12 + i32::try_from(row_x).unwrap_or(0),
            row_y,
            width,
            BUTTON_HEIGHT,
        ));
        row_x = row_x.saturating_add(width + BUTTON_GAP);
    }
    DialogLayout {
        panel,
        lines_area,
        buttons,
    }
}

/// 按钮宽度：文字宽 + 内边距，夹在 `[56, 200]`。
#[must_use]
pub fn button_width(label: &str, measure: &impl Fn(&str) -> u32) -> u32 {
    measure(label)
        .saturating_add(20)
        .clamp(BUTTON_MIN_WIDTH, 200)
}

/// 按钮行数。
fn button_rows(widths: &[u32], content_width: u32) -> u32 {
    let mut rows = 1u32;
    let mut x = 0u32;
    for width in widths {
        if x > 0 && x.saturating_add(*width) > content_width {
            rows = rows.saturating_add(1);
            x = 0;
        }
        x = x.saturating_add(width.saturating_add(BUTTON_GAP));
    }
    rows
}

/// 解析一行：返回（去掉标记的正文，按钮列表）。
#[must_use]
pub fn parse_line(line: &str) -> (String, Vec<DialogButton>) {
    // 大按钮 `<<文本/@键>>` 直接按普通按钮处理，避免多余尖括号混进正文。
    let normalized = line.replace("<<", "<").replace(">>", ">");
    let mut cleaned = String::new();
    let mut buttons = Vec::new();
    let mut rest = normalized.as_str();
    while let Some(start) = rest.find('<') {
        cleaned.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else {
            cleaned.push_str(&rest[start..]);
            return (strip_colour(&cleaned), buttons);
        };
        let inner = &after[..end];
        match inner.split_once('/') {
            Some((label, key)) if key.trim().starts_with('@') && !label.trim().is_empty() => {
                buttons.push(DialogButton {
                    label: strip_colour(label.trim()),
                    key: key.trim().to_owned(),
                });
            }
            _ => {
                cleaned.push('<');
                cleaned.push_str(inner);
                cleaned.push('>');
            }
        }
        rest = &after[end + 1..];
    }
    cleaned.push_str(rest);
    (strip_colour(&cleaned), buttons)
}

/// 去掉 `{文本/颜色}` 的颜色标记，只留文本。
#[must_use]
pub fn strip_colour(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let inner = &after[..end];
        if let Some((text, _colour)) = inner.split_once('/') {
            out.push_str(text);
        } else {
            out.push('{');
            out.push_str(inner);
            out.push('}');
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// 加在按钮上的「关闭」按钮文案（绘制时用当前语言）。
#[must_use]
pub fn close_button_label(language: Language) -> &'static str {
    Label::Close.text(language)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir_core::Point;

    fn measure(label: &str) -> u32 {
        u32::try_from(label.chars().count()).unwrap_or(0) * 12 + 8
    }

    #[test]
    fn parses_buttons_and_strips_markup() {
        let (cleaned, buttons) = parse_line("你好，<买东西/@BUY>！");
        assert_eq!(cleaned, "你好，！");
        assert_eq!(buttons.len(), 1);
        assert_eq!(buttons[0].label, "买东西");
        assert_eq!(buttons[0].key, "@BUY");

        let (cleaned, buttons) = parse_line("{红色文字/Red}普通文字");
        assert_eq!(cleaned, "红色文字普通文字");
        assert!(buttons.is_empty());

        let (cleaned, buttons) = parse_line("<<大按钮/@BIG>>");
        assert_eq!(cleaned, "");
        assert_eq!(buttons[0].key, "@BIG");
    }

    #[test]
    fn page_always_has_an_exit_button() {
        let page = ParsedPage::parse(&["只有正文".to_owned()]);
        assert_eq!(page.lines, vec!["只有正文".to_owned()]);
        assert!(page.buttons.iter().any(|button| button.key == "@exit"));
        assert!(!page.is_empty());
    }

    #[test]
    fn duplicate_buttons_are_merged() {
        let page = ParsedPage::parse(&[
            "<买/@BUY> <买/@buy> <走/@exit>".to_owned(),
            "<买/@BUY>".to_owned(),
        ]);
        assert_eq!(page.buttons.len(), 2);
        assert_eq!(page.buttons[0].key, "@BUY");
        assert_eq!(page.buttons[1].key, "@exit");
    }

    #[test]
    fn layout_buttons_are_inside_the_panel_and_hit_testable() {
        let lines = vec![
            "第一行正文".to_owned(),
            "第二行正文".to_owned(),
            "<按钮甲/@A>".to_owned(),
            "<按钮乙/@B>".to_owned(),
            "<离开/@exit>".to_owned(),
        ];
        let page = ParsedPage::parse(&lines);
        assert_eq!(page.buttons.len(), 3);
        let frame = Rect::screen(800, 600);
        let layout = dialog_layout(frame, &page, measure);
        assert!(layout.panel.y >= 58);
        assert!(layout.panel.bottom() <= 600);
        for (index, rect) in layout.buttons.iter().enumerate() {
            assert!(
                layout.panel.contains(rect.center()),
                "按钮 {index} 应在面板内：{rect:?} / {:?}",
                layout.panel
            );
            assert_eq!(layout.button_index_at(rect.center()), Some(index));
        }
        assert_eq!(layout.button_index_at(Point::new(0, 0)), None);
        // 按钮行之间不重叠。
        for (index, first) in layout.buttons.iter().enumerate() {
            for second in &layout.buttons[index + 1..] {
                assert!(first.intersect(*second).is_none());
            }
        }
    }

    #[test]
    fn narrow_frames_still_produce_a_visible_dialog() {
        let page = ParsedPage::parse(&["很小的窗口也要能点按钮".to_owned()]);
        let layout = dialog_layout(Rect::screen(320, 240), &page, measure);
        assert!(layout.panel.width <= 320);
        assert!(!layout.buttons.is_empty());
        assert!(layout.buttons[0].width >= BUTTON_MIN_WIDTH);
    }
}
