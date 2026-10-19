//! 中文排版：按显示宽度折行、避头尾、尾部省略号。
//!
//! 客户端字体是像素字体，`UiText::text_width` 给出真实像素宽；测试与聊天面板的
//! 「宽度单位」约定为：CJK 全角 2、ASCII 1（与 Crystal 的固定字符格接近）。
//!
//! # 规则
//! - 折行优先在**显示宽度**（像素或 2/1 单位）内放尽可能多的字符；
//! - 标点避头尾：行首不能是 `，。！？；：、）》」』】…` 等收尾标点，
//!   若折行点正好落在这些字符前，把上一字符一起挪到下一行；
//! - 超过 `max_lines` 行时，最后一行截断并加 `…`（宽度不够时退化 `..`/`.`）；
//! - 显式 `\n` 强制换行。

/// 收尾标点：不能出现在行首。
const NO_LINE_START: &[char] = &[
    '，', '。', '、', '；', '：', '？', '！', '）', '】', '》', '」', '』', '”', '’', '…', '·',
    '～', ';', ':', '?', '!', ',', '.', ')', ']', '}',
];

/// 开头标点：不能出现在行尾。
const NO_LINE_END: &[char] = &['（', '【', '《', '「', '『', '“', '‘', '(', '[', '{'];

/// 单个字符的显示宽度（CJK 2，组合/控制 0，其它 1）。
#[must_use]
pub fn char_display_width(character: char) -> u32 {
    if character.is_control() {
        return 0;
    }
    if character == '…' {
        return 2;
    }
    let code = u32::from(character);
    let wide = matches!(
        code,
        0x1100..=0x115F
            | 0x2E80..=0x303E
            | 0x3041..=0x33FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xA000..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE30..=0xFE4F
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x1F300..=0x1FAFF
            | 0x20000..=0x3FFFD
    );
    if wide { 2 } else { 1 }
}

/// 字符串显示宽度（CJK 2/ASCII 1）。
#[must_use]
pub fn display_width(text: &str) -> u32 {
    text.chars().map(char_display_width).sum()
}

/// 按显示宽度折行，最多 `max_lines` 行；超出行数时最后一行加省略号。
///
/// `width` 是单字符宽度函数（聊天传 `UiText::text_width`，测试传 2/1 规则）。
pub fn wrap_by_width<F>(text: &str, max_width: u32, max_lines: usize, width: F) -> Vec<String>
where
    F: Fn(&str) -> u32,
{
    let max_lines = max_lines.max(1);
    let mut lines: Vec<String> = Vec::new();
    for paragraph in text.split('\n') {
        let mut remaining: Vec<char> = paragraph.chars().collect();
        while !remaining.is_empty() {
            if lines.len() + 1 == max_lines {
                // 最后一行：放得多少放多少，剩余用省略号。
                let mut line = String::new();
                let mut used = 0u32;
                let mut consumed = 0usize;
                while consumed < remaining.len() {
                    let mut probe = line.clone();
                    probe.push(remaining[consumed]);
                    let probe_width = width(&probe);
                    if probe_width > max_width && !line.is_empty() {
                        break;
                    }
                    line = probe;
                    used = probe_width;
                    consumed += 1;
                }
                let _ = used;
                if consumed < remaining.len() {
                    // 预留省略号宽度，避免「刚好填满」时漏掉省略号。
                    let ellipsis = "…";
                    let ellipsis_width = width(ellipsis);
                    let mut trimmed = String::new();
                    if ellipsis_width <= max_width {
                        for character in line.chars() {
                            let mut probe = trimmed.clone();
                            probe.push(character);
                            if width(&probe) + ellipsis_width > max_width {
                                break;
                            }
                            trimmed = probe;
                        }
                        trimmed.push_str(ellipsis);
                    }
                    line = trimmed;
                }
                lines.push(line);
                return lines;
            }
            let mut line = String::new();
            let mut consumed = 0usize;
            while consumed < remaining.len() {
                let mut probe = line.clone();
                probe.push(remaining[consumed]);
                if width(&probe) > max_width && !line.is_empty() {
                    break;
                }
                line = probe;
                consumed += 1;
            }
            if consumed == 0 {
                // 单字符比整行还宽：强制放一个，避免死循环。
                line.push(remaining[0]);
                consumed = 1;
            }
            // 避头尾：下一行以收尾标点开头时，把本行最后一个字符挪下去。
            if consumed < remaining.len()
                && NO_LINE_START.contains(&remaining[consumed])
                && line.chars().count() > 1
            {
                if let Some(last) = line.pop() {
                    remaining.insert(consumed, last);
                }
            }
            // 行尾不能是开头标点：把它也挪到下一行。
            if consumed < remaining.len()
                && line
                    .chars()
                    .last()
                    .is_some_and(|last| NO_LINE_END.contains(&last))
                && line.chars().count() > 1
            {
                if let Some(last) = line.pop() {
                    remaining.insert(consumed, last);
                }
            }
            lines.push(line);
            remaining.drain(..consumed.min(remaining.len()));
        }
        if paragraph.is_empty() {
            lines.push(String::new());
        }
    }
    lines
}

/// 尾部省略号截断（`…`，宽度不够时 `..`/`.`）。
#[must_use]
pub fn truncate_with_ellipsis<F>(text: &str, max_width: u32, width: &F) -> String
where
    F: Fn(&str) -> u32,
{
    if width(text) <= max_width {
        return text.to_owned();
    }
    for ellipsis in ["…", "..", "."] {
        let ellipsis_width = width(ellipsis);
        if ellipsis_width > max_width {
            continue;
        }
        let mut line = String::new();
        for character in text.chars() {
            let mut probe = line.clone();
            probe.push(character);
            probe.push_str(ellipsis);
            if width(&probe) > max_width {
                break;
            }
            line.push(character);
        }
        line.push_str(ellipsis);
        return line;
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_width(text: &str) -> u32 {
        display_width(text)
    }

    #[test]
    fn display_width_counts_cjk_as_two() {
        assert_eq!(display_width("中文abc"), 7);
        assert_eq!(display_width("，。"), 4);
        assert_eq!(display_width(""), 0);
    }

    #[test]
    fn wraps_mixed_chinese_and_ascii_by_width() {
        // 宽度 6：中文 3 字 / 或 6 个 ASCII。
        let lines = wrap_by_width("中文abc中文", 6, 4, unit_width);
        assert_eq!(lines, vec!["中文ab", "c中文"], "{lines:?}");
        let lines = wrap_by_width("abcdef", 3, 4, unit_width);
        assert_eq!(lines, vec!["abc", "def"]);
    }

    #[test]
    fn closing_punctuation_never_starts_a_line() {
        // 宽度 4：正常会把「。」挤到第二行行首；规则要求把「好」一起挪下来。
        let lines = wrap_by_width("你好。世界", 4, 4, unit_width);
        assert_eq!(lines[0], "你");
        assert_eq!(lines[1], "好。");
        for line in &lines {
            assert!(!NO_LINE_START.contains(&line.chars().next().unwrap_or(' ')));
        }
    }

    #[test]
    fn truncates_to_max_lines_with_ellipsis() {
        let lines = wrap_by_width("这是一段很长很长的聊天消息需要截断", 6, 2, unit_width);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[1].ends_with('…'), "{lines:?}");
        assert!(unit_width(&lines[1]) <= 6);
    }

    #[test]
    fn explicit_newlines_are_kept() {
        let lines = wrap_by_width("第一行\n第二行", 20, 4, unit_width);
        assert_eq!(lines, vec!["第一行", "第二行"]);
    }

    #[test]
    fn truncation_uses_ascii_fallback_for_narrow_width() {
        assert_eq!(truncate_with_ellipsis("abcdef", 4, &unit_width), "ab…");
        assert_eq!(truncate_with_ellipsis("中文", 3, &unit_width), "…");
        assert_eq!(truncate_with_ellipsis("短", 10, &unit_width), "短");
    }
}
