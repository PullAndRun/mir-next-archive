//! 极简调试叠层：3×5 点阵字 + 屏幕中心准星。
//!
//! 只画 F1 打开时需要的中心格坐标，不引入字体库。字模是 3 列 × 5 行，每行 3 bit
//! （高位在左），放大 [`GLYPH_SCALE`] 倍；不认识的字符按空格处理。
//! 为了在任何颜色的地图上都可读，文字画两遍：先在右下方画黑色阴影，再在左上画白字。

use mir_core::Point;
use mir_render::Frame;

/// 字模放大倍数。
const GLYPH_SCALE: u32 = 2;
/// 字模宽度（像素）。
const GLYPH_WIDTH: u32 = 3;
/// 字符步进（3 列 + 1 列间隔）。
const ADVANCE: u32 = 4;

/// 文字颜色。
const TEXT: [u8; 4] = [230, 255, 180, 255];
/// 阴影颜色。
const SHADOW: [u8; 4] = [0, 0, 0, 255];
/// 准星颜色。
const CROSSHAIR: [u8; 4] = [255, 0, 255, 255];

/// 画叠层：左上角显示中心格坐标，屏幕中心画一个准星。
pub fn draw(frame: &mut Frame, center: Point) {
    draw_line(frame, 9, 9, &format!("X {}  Y {}", center.x, center.y));
    draw_crosshair(frame, frame.width / 2, frame.height / 2);
}

/// 联机叠层：坐标、实体数、HP/MP，聊天输入时显示 `CHAT`。
///
/// 只是调试信息，不参与游戏逻辑；没有字体库，全部用 3×5 点阵数字与少量字母。
pub fn draw_live(
    frame: &mut Frame,
    center: Point,
    entities: usize,
    hp: i32,
    mp: i32,
    chat_active: bool,
) {
    draw_line(frame, 9, 9, &format!("X {}  Y {}", center.x, center.y));
    draw_line(frame, 9, 23, &format!("E {entities}"));
    draw_line(frame, 9, 37, &format!("H {hp}  M {mp}"));
    if chat_active {
        draw_line(frame, 9, 51, "CHAT");
    }
    draw_crosshair(frame, frame.width / 2, frame.height / 2);
}

/// 画一行带阴影的点阵字。
fn draw_line(frame: &mut Frame, x: u32, y: u32, text: &str) {
    draw_text(frame, x + 1, y + 1, text, SHADOW);
    draw_text(frame, x, y, text, TEXT);
}

/// 在 `(x, y)` 画一行字。
fn draw_text(frame: &mut Frame, x: u32, y: u32, text: &str, color: [u8; 4]) {
    for (index, character) in text.chars().enumerate() {
        let glyph = glyph(character);
        let origin_x = x + u32::try_from(index).unwrap_or(0) * ADVANCE * GLYPH_SCALE;
        for (row, bits) in glyph.iter().enumerate() {
            for column in 0..GLYPH_WIDTH {
                if bits & (1 << (GLYPH_WIDTH - 1 - column)) == 0 {
                    continue;
                }
                let pixel_x = origin_x + column * GLYPH_SCALE;
                let pixel_y = y + u32::try_from(row).unwrap_or(0) * GLYPH_SCALE;
                for dy in 0..GLYPH_SCALE {
                    for dx in 0..GLYPH_SCALE {
                        frame.set_pixel(pixel_x + dx, pixel_y + dy, color);
                    }
                }
            }
        }
    }
}

/// 中心准星：上下左右各 6 像素。
fn draw_crosshair(frame: &mut Frame, center_x: u32, center_y: u32) {
    for offset in 0..6 {
        frame.set_pixel(center_x + offset, center_y, CROSSHAIR);
        frame.set_pixel(center_x.saturating_sub(offset), center_y, CROSSHAIR);
        frame.set_pixel(center_x, center_y + offset, CROSSHAIR);
        frame.set_pixel(center_x, center_y.saturating_sub(offset), CROSSHAIR);
    }
}

/// 3×5 字模；不认识的字符返回空格。
fn glyph(character: char) -> [u8; 5] {
    match character {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b001, 0b001],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        'X' => [0b101, 0b101, 0b010, 0b101, 0b101],
        'Y' => [0b101, 0b101, 0b010, 0b010, 0b010],
        'E' => [0b111, 0b100, 0b111, 0b100, 0b111],
        'H' => [0b101, 0b101, 0b111, 0b101, 0b101],
        'M' => [0b101, 0b111, 0b111, 0b101, 0b101],
        'A' => [0b010, 0b101, 0b111, 0b101, 0b101],
        'C' => [0b011, 0b100, 0b100, 0b100, 0b011],
        'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        '-' => [0b000, 0b000, 0b111, 0b000, 0b000],
        ',' => [0b000, 0b000, 0b000, 0b010, 0b100],
        _ => [0, 0, 0, 0, 0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_marks_pixels_and_stays_in_bounds() {
        let mut frame = Frame::new(200, 100);
        frame.clear([10, 20, 30, 255]);
        draw(&mut frame, Point::new(330, 270));
        // 中心准星。
        assert_eq!(frame.pixel(100, 50), Some(CROSSHAIR));
        // 左上文字区域被改写。
        assert_ne!(frame.pixel(10, 10), Some([10, 20, 30, 255]));
        assert!(frame.distinct_color_count() >= 3);
    }

    #[test]
    fn unknown_glyphs_are_blank() {
        assert_eq!(glyph('?'), [0, 0, 0, 0, 0]);
        assert_ne!(glyph('0'), [0, 0, 0, 0, 0]);
    }

    #[test]
    fn live_overlay_draws_entity_and_chat_markers() {
        let mut frame = Frame::new(200, 120);
        frame.clear([0, 0, 0, 255]);
        draw_live(&mut frame, Point::new(330, 270), 12, 15, 9, true);
        assert_ne!(frame.pixel(9, 9), Some([0, 0, 0, 255]));
        assert_ne!(frame.pixel(11, 51), Some([0, 0, 0, 255]));
        assert_eq!(frame.pixel(100, 60), Some(CROSSHAIR));
    }
}
