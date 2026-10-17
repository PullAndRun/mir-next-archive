//! 极简调试叠层：3×5 点阵字 + 屏幕中心准星。
//!
//! 只画 F1 打开时需要的坐标/实体数/HP/MP，不依赖 TrueType 字体；点阵字模与绘制
//! 实现在 `mir_render::pixel` 里（`UiText` 在没有字体时也复用它）。字模 3 列 × 5 行，
//! 每行 3 bit（高位在左），放大 [`GLYPH_SCALE`] 倍；不认识的字符按空格处理。
//! 为了在任何颜色的地图上都可读，文字画两遍：先右下方黑色阴影，再左上亮色。

use mir_core::Point;
use mir_render::{Frame, pixel};

/// 字模放大倍数。
const GLYPH_SCALE: u32 = 2;

/// 文字颜色。
const TEXT: [u8; 4] = [230, 255, 180, 255];
/// 准星颜色。
const CROSSHAIR: [u8; 4] = [255, 0, 255, 255];

/// 画叠层：左上角显示中心格坐标，屏幕中心画一个准星。
pub fn draw(frame: &mut Frame, center: Point) {
    draw_line(frame, 9, 9, &format!("X {}  Y {}", center.x, center.y));
    draw_crosshair(frame, frame.width / 2, frame.height / 2);
}

/// 联机叠层：坐标、实体数、HP/MP，聊天输入时显示 `CHAT`。
///
/// 只是调试信息，不参与游戏逻辑；全部用 3×5 点阵数字与少量字母。
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
fn draw_line(frame: &mut Frame, x: i32, y: i32, text: &str) {
    pixel::draw_text_shadowed(frame, x, y, text, GLYPH_SCALE, TEXT);
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
        assert_eq!(pixel::glyph('\u{4e2d}'), [0, 0, 0, 0, 0]);
        assert_ne!(pixel::glyph('0'), [0, 0, 0, 0, 0]);
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
