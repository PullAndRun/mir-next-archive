//! 单行文本输入控件：邮件写信页（收件人/主题/正文/金额）与行会公告编辑。
//!
//! Crystal 用的是 `WinForms` 的 `MirTextBox`（`Client/MirControls/MirTextBox.cs`），支持多行、
//! 滚动与 IME。本作没有控件系统，窗口层把键盘事件直接交给 [`crate::ui::UiState`]：
//! [`TextField`] 只保存字符串与长度上限，`|=|` 光标由绘制函数画在末尾。
//!
//! 简化：输入框是**单行**的（Crystal 的正文/公告是多行 `TextBox`），超出宽度的部分被裁掉；
//! 也不处理输入法组合中间态（winit 的 `Ime` 事件没接），中文依赖输入法自己提交
//! `Key::Character`。原因：本作的输入栈只有「字符 → 字符串」，没有光标移动/选区。

use mir_core::Point;
use mir_render::Frame;

use crate::geom::Rect;
use crate::ui::UiText;
use crate::ui::hud::{BODY_COLOR, MUTED_COLOR, SIZE_BODY};

/// 输入框背景色。
pub const FIELD_BACKGROUND: [u8; 4] = [24, 30, 40, 235];
/// 输入框边框色（未聚焦）。
pub const FIELD_BORDER: [u8; 4] = [70, 84, 104, 255];
/// 输入框边框色（聚焦）。
pub const FIELD_FOCUS_BORDER: [u8; 4] = [255, 216, 96, 255];
/// 聚焦时的光标颜色。
pub const CARET_COLOR: [u8; 4] = [255, 244, 190, 255];

/// 单行文本字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextField {
    /// 当前文本。
    pub value: String,
    /// 最大字符数（按 Unicode 标量计，与服务端 UTF-16 计数略有差异，见模块文档）。
    pub max_chars: usize,
    /// 是否只接受 ASCII 数字（金币/金额输入）。
    pub numeric: bool,
    /// 光标位置（**字符**下标，0..=字符数；不是字节下标）。
    pub cursor: usize,
}

impl TextField {
    /// 新建一个普通文本字段。
    #[must_use]
    pub const fn new(max_chars: usize) -> Self {
        Self {
            value: String::new(),
            max_chars,
            numeric: false,
            cursor: 0,
        }
    }

    /// 新建一个只接受数字的字段。
    #[must_use]
    pub const fn numeric(max_chars: usize) -> Self {
        Self {
            value: String::new(),
            max_chars,
            numeric: true,
            cursor: 0,
        }
    }

    /// 追加一个字符；超出上限或数字字段收到非数字时忽略。
    ///
    /// 换行/制表等控制字符一律丢弃（单行输入）。
    pub fn insert(&mut self, character: char) {
        if character.is_control() {
            return;
        }
        if self.numeric && !character.is_ascii_digit() {
            return;
        }
        if self.value.chars().count() >= self.max_chars {
            return;
        }
        let byte = self.cursor_byte_index();
        self.value.insert(byte, character);
        self.cursor += 1;
    }

    /// 删掉光标前的一个字符（按 UTF-8 字符边界，中文/表情不会切一半）。
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let end = self.cursor_byte_index();
        let start = self
            .value
            .char_indices()
            .nth(self.cursor - 1)
            .map_or(0, |(index, _)| index);
        self.value.replace_range(start..end, "");
        self.cursor -= 1;
    }

    /// 光标左移一个字符。
    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// 光标右移一个字符。
    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.value.chars().count());
    }

    /// 光标所在的字节下标（绘制/切分用）。
    #[must_use]
    pub fn cursor_byte_index(&self) -> usize {
        self.value
            .char_indices()
            .nth(self.cursor)
            .map_or(self.value.len(), |(index, _)| index)
    }

    /// 直接写入文本（测试与「回填」用）；会按上限截断并过滤非数字。
    pub fn set(&mut self, text: &str) {
        self.value.clear();
        self.cursor = 0;
        for character in text.chars() {
            self.insert(character);
        }
    }

    /// 文本是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// 数字字段的数值（非数字字段或空串返回 0）。
    #[must_use]
    pub fn number(&self) -> u32 {
        self.value.parse().unwrap_or(0)
    }
}

impl Default for TextField {
    fn default() -> Self {
        Self::new(0)
    }
}

/// 画一个输入框（含占位文案与聚焦光标）。
pub fn draw(
    frame: &mut Frame,
    text: &UiText,
    rect: Rect,
    field: &TextField,
    focused: bool,
    placeholder: &str,
) {
    frame.fill_rect(rect.x, rect.y, rect.width, rect.height, FIELD_BACKGROUND);
    let border = if focused {
        FIELD_FOCUS_BORDER
    } else {
        FIELD_BORDER
    };
    frame.fill_rect(rect.x, rect.y, rect.width, 1, border);
    frame.fill_rect(rect.x, rect.bottom() - 1, rect.width, 1, border);
    frame.fill_rect(rect.x, rect.y, 1, rect.height, border);
    frame.fill_rect(rect.right() - 1, rect.y, 1, rect.height, border);
    let inner = rect.inset(4);
    if field.value.is_empty() && !focused {
        text.draw_clipped(
            frame,
            inner.x,
            inner.y,
            placeholder,
            SIZE_BODY,
            MUTED_COLOR,
            inner,
        );
        return;
    }
    // 聚焦时在**光标位置**画竖线（字符边界之间，中文/表情不会切一半）。
    let shown = if focused {
        let byte = field.cursor_byte_index();
        format!("{}|{}", &field.value[..byte], &field.value[byte..])
    } else {
        field.value.clone()
    };
    text.draw_clipped(
        frame, inner.x, inner.y, &shown, SIZE_BODY, BODY_COLOR, inner,
    );
}

/// 点击是否落在输入框上（命中测试与绘制共用同一矩形）。
#[must_use]
pub fn hit(rect: Rect, point: Point) -> bool {
    rect.contains(point)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_fields_enforce_limits_and_numeric_filtering() {
        let mut field = TextField::new(3);
        field.set("abcde");
        assert_eq!(field.value, "abc");
        field.backspace();
        assert_eq!(field.value, "ab");

        let mut amount = TextField::numeric(6);
        amount.set("12a3b");
        assert_eq!(amount.value, "123");
        assert_eq!(amount.number(), 123);
        amount.set("");
        assert_eq!(amount.number(), 0);
    }

    #[test]
    fn control_characters_are_dropped() {
        let mut field = TextField::new(10);
        field.insert('a');
        field.insert('\n');
        field.insert('\t');
        field.insert('b');
        assert_eq!(field.value, "ab");
    }

    #[test]
    fn chinese_characters_count_as_one_each() {
        let mut field = TextField::new(2);
        field.set("屠龙刀");
        assert_eq!(field.value, "屠龙");
    }

    #[test]
    fn cursor_edits_stay_on_utf8_boundaries() {
        let mut field = TextField::new(16);
        field.set("中文abc");
        assert_eq!(field.cursor, 5);
        // 光标退到 'a' 前面，插入/退格都按字符边界处理，不 panic。
        field.move_left();
        field.move_left();
        field.move_left();
        assert_eq!(field.cursor, 2);
        assert_eq!(field.cursor_byte_index(), "中文".len());
        field.insert('好');
        assert_eq!(field.value, "中文好abc");
        field.backspace();
        assert_eq!(field.value, "中文abc");
        field.backspace();
        assert_eq!(field.value, "中abc");
        // 连续退格到空、再退格不 panic（光标在中间时先把光标移到末尾）。
        for _ in 0..10 {
            field.move_right();
            field.backspace();
        }
        assert!(field.value.is_empty());
        assert_eq!(field.cursor, 0);
        field.move_right();
        assert_eq!(field.cursor, 0);
    }
}
