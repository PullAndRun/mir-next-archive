//! 界面文字门面：优先 TrueType（中文），找不到字体时退回 3×5 点阵。
//!
//! [`UiText`] 是 UI 唯一接触的字体接口：
//!
//! * 有 TrueType 字体时用 `mir_render::text::TextRenderer`；
//! * 一个字体都找不到时打印一次警告并退回 `mir_render::pixel`（英文可读、中文空白），
//!   绝不 panic，也不影响地图/实体的绘制。
//!
//! 所有方法都接受像素字号 `size`；点阵兜底会把字号映射成整数放大倍数。

use std::path::{Path, PathBuf};

use mir_render::text::TextRenderer;
use mir_render::{Frame, TextClip, pixel};

use crate::geom::Rect;

/// UI 文字门面。
#[derive(Debug)]
pub struct UiText {
    /// TrueType 渲染器；`None` 表示点阵兜底。
    renderer: Option<TextRenderer>,
    /// 字体路径（兜底时为 `None`）。
    font_path: Option<PathBuf>,
}

impl UiText {
    /// 加载字体：先试 `preferred`，再按 `mir-render` 的发现顺序找，最后退点阵。
    ///
    /// 指定的字体不存在/损坏不是致命错误，只打印警告并继续找系统字体。
    #[must_use]
    pub fn load(preferred: Option<&Path>) -> Self {
        if let Some(path) = preferred {
            match TextRenderer::new(path) {
                Ok(renderer) => return Self::from_renderer(renderer),
                Err(error) => {
                    eprintln!("警告：指定字体不可用：{error}；改为自动寻找字体");
                }
            }
        }
        match TextRenderer::discover() {
            Ok(renderer) => Self::from_renderer(renderer),
            Err(error) => {
                eprintln!("警告：{error}；HUD 退回 3×5 点阵字（中文将无法显示）");
                Self::fallback()
            }
        }
    }

    /// 强制使用 3×5 点阵字（无字体环境与单元测试用）。
    #[must_use]
    pub const fn fallback() -> Self {
        Self {
            renderer: None,
            font_path: None,
        }
    }

    /// 用已加载的渲染器构造。
    #[must_use]
    fn from_renderer(renderer: TextRenderer) -> Self {
        let path = renderer.path().to_path_buf();
        eprintln!("界面字体：{}", path.display());
        Self {
            renderer: Some(renderer),
            font_path: Some(path),
        }
    }

    /// 是否使用 TrueType 字体（否则是点阵兜底）。
    #[must_use]
    pub const fn is_font(&self) -> bool {
        self.renderer.is_some()
    }

    /// 字体路径（点阵兜底时 `None`）。
    #[must_use]
    pub fn font_path(&self) -> Option<&Path> {
        self.font_path.as_deref()
    }

    /// 一段文字的宽度（像素）。
    #[must_use]
    pub fn text_width(&self, text: &str, size: u32) -> u32 {
        match &self.renderer {
            Some(renderer) => renderer.text_width(text, size),
            None => pixel::text_width(text, pixel_scale(size)),
        }
    }

    /// 一行文字的高度（像素）。
    #[must_use]
    pub fn line_height(&self, size: u32) -> u32 {
        match &self.renderer {
            Some(renderer) => renderer.line_height(size),
            None => pixel::line_height(pixel_scale(size)),
        }
    }

    /// 画一段文字（裁剪到帧边界）。
    pub fn draw(&self, frame: &mut Frame, x: i32, y: i32, text: &str, size: u32, color: [u8; 4]) {
        match &self.renderer {
            Some(renderer) => renderer.draw(frame, x, y, text, size, color),
            None => pixel::draw_text(frame, x, y, text, pixel_scale(size), color),
        }
    }

    /// 画一段文字并裁剪到 `clip`。
    // 位置 + 字号 + 颜色 + 裁剪区是绘制原语的天然参数；调用方（面板）很多，拆结构体反而更吵。
    #[allow(clippy::too_many_arguments)]
    pub fn draw_clipped(
        &self,
        frame: &mut Frame,
        x: i32,
        y: i32,
        text: &str,
        size: u32,
        color: [u8; 4],
        clip: Rect,
    ) {
        // 超宽时尾部加省略号（物品名/技能/任务/按钮都走这里，悬停不换行）。
        let fitted = if clip.width > 0 && self.text_width(text, size) > clip.width {
            crate::ui::typography::truncate_with_ellipsis(text, clip.width, &|probe: &str| {
                self.text_width(probe, size)
            })
        } else {
            text.to_owned()
        };
        let text = fitted.as_str();
        let text_clip = TextClip::new(clip.x, clip.y, clip.width, clip.height);
        match &self.renderer {
            Some(renderer) => renderer.draw_clipped(frame, x, y, text, size, color, text_clip),
            None => {
                pixel::draw_text_clipped(frame, x, y, text, pixel_scale(size), color, text_clip);
            }
        }
    }

    /// 右对齐画一行（`right` 是右边界，不含）。
    pub fn draw_right(
        &self,
        frame: &mut Frame,
        right: i32,
        y: i32,
        text: &str,
        size: u32,
        color: [u8; 4],
    ) {
        let width = self.text_width(text, size);
        self.draw(
            frame,
            right - i32::try_from(width).unwrap_or(0),
            y,
            text,
            size,
            color,
        );
    }

    /// 在矩形里水平居中画一行（垂直方向也居中）。
    pub fn draw_center(
        &self,
        frame: &mut Frame,
        rect: Rect,
        text: &str,
        size: u32,
        color: [u8; 4],
    ) {
        let width = self.text_width(text, size);
        let height = self.line_height(size);
        let x = rect.x
            + (i32::try_from(rect.width).unwrap_or(0) - i32::try_from(width).unwrap_or(0)) / 2;
        let y = rect.y
            + (i32::try_from(rect.height).unwrap_or(0) - i32::try_from(height).unwrap_or(0)) / 2;
        self.draw_clipped(frame, x, y.max(rect.y), text, size, color, rect);
    }
}

/// 字号 → 点阵放大倍数（12~17 像素用 2 倍，18 以上用 3 倍）。
#[must_use]
pub const fn pixel_scale(size: u32) -> u32 {
    if size >= 18 {
        3
    } else if size >= 9 {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_width_is_monotonic_and_line_height_positive() {
        let text = UiText::fallback();
        assert!(!text.is_font());
        assert_eq!(text.text_width("", 14), 0);
        assert!(text.text_width("HP", 14) < text.text_width("HP 15/15", 14));
        assert!(text.line_height(14) >= 5);
    }

    #[test]
    fn fallback_draws_ascii_pixels() {
        let text = UiText::fallback();
        let mut frame = Frame::new(80, 30);
        frame.clear([0, 0, 0, 255]);
        text.draw(&mut frame, 2, 2, "HP 15", 14, [255, 255, 255, 255]);
        assert!(frame.distinct_color_count() > 1);
    }

    #[test]
    fn missing_preferred_font_does_not_panic() {
        let text = UiText::load(Some(Path::new("target/does-not-exist.ttf")));
        // 本机通常有系统字体；没有字体时也会退点阵，两者都不应 panic。
        let mut frame = Frame::new(40, 20);
        frame.clear([0, 0, 0, 255]);
        text.draw(&mut frame, 0, 0, "OK", 12, [255, 255, 255, 255]);
        if text.is_font() {
            assert!(text.font_path().is_some());
        }
    }

    #[test]
    fn centered_text_stays_inside_its_rect() {
        let text = UiText::fallback();
        let mut frame = Frame::new(100, 40);
        frame.clear([0, 0, 0, 255]);
        let rect = Rect::new(10, 10, 60, 20);
        text.draw_center(&mut frame, rect, "OK", 12, [255, 255, 0, 255]);
        for y in 0..40u32 {
            for x in 0..100u32 {
                if frame.pixel(x, y) != Some([0, 0, 0, 255]) {
                    assert!(rect.contains_xy(x as i32, y as i32), "({x},{y}) 越界");
                }
            }
        }
    }
}
