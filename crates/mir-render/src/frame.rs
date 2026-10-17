//! 帧缓冲与光栅化：RGBA8、行优先、自上而下。
//!
//! 帧缓冲按「不透明 RGB 画布」处理：alpha 混合时只混合 RGB，目标 alpha 一律写回
//! 255。这是客户端窗口的语义（`MapControl.BackColour = Color.Black`，
//! `GameScene.cs:10406`），也让 PNG 截图始终可见。
//!
//! 混合公式与 Direct3D 一致：
//!
//! * [`Frame::blit`]（source-over）：`out = (src * a + dst * (255 - a) + 127) / 255`，
//!   对应 Crystal 的 `DXManager.Draw`（`SpriteFlags.AlphaBlend`）；
//! * [`Frame::blit_add`]（加法）：`out = min(255, dst + (src * a + 127) / 255)`，
//!   对应 Crystal 的 `DrawBlend`：`DXManager.SetBlend(true)` 把
//!   `DestBlend` 设为 `Blend.One`（`DXManager.cs` 的 `SetBlend`），灯光类图像靠加法
//!   混合把黑色背景“加没”。
//!
//! `a = 0` 时完全跳过，`a = 255` 时 source-over 直接覆盖；`+127` 是四舍五入。

use std::path::{Path, PathBuf};

use mir_asset::Image;

use crate::{BACKGROUND, RenderError};

/// 一帧 RGBA8 像素。
///
/// `pixels` 长度恒为 `width * height * 4`，行优先、自上而下。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// 宽（像素）。
    pub width: u32,
    /// 高（像素）。
    pub height: u32,
    /// RGBA8 像素缓冲。
    pub pixels: Vec<u8>,
}

impl Frame {
    /// 新建一帧，像素全 0（透明黑）。
    ///
    /// 渲染器每帧都会先用不透明黑清屏（见 [`Frame::clear`]），所以「新建后立刻是透明黑」
    /// 只影响直接构造帧的调用方。
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let len = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            .unwrap_or(0);
        Self {
            width,
            height,
            pixels: vec![0; len],
        }
    }

    /// 用同一种颜色填满整帧。
    pub fn clear(&mut self, color: [u8; 4]) {
        for pixel in self.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&color);
        }
    }

    /// 取一个像素；越界返回 `None`。
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let offset = pixel_offset(self.width, x, y);
        self.pixels.get(offset..offset + 4).map(|slice| {
            let mut value = [0u8; 4];
            value.copy_from_slice(slice);
            value
        })
    }

    /// 覆盖一个像素（不混合）；越界时什么都不做。
    ///
    /// 调试叠层（坐标文字、准星）用。
    pub fn set_pixel(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let offset = pixel_offset(self.width, x, y);
        if let Some(target) = self.pixels.get_mut(offset..offset + 4) {
            target.copy_from_slice(&color);
        }
    }

    /// 用同一种颜色填充一个矩形；坐标为像素坐标，越界部分自动裁剪。
    ///
    /// 实体占位方块、血条与调试标记用。宽或高为 0 时什么都不做。
    pub fn fill_rect(&mut self, x: i32, y: i32, width: u32, height: u32, color: [u8; 4]) {
        let left = i64::from(x).max(0);
        let top = i64::from(y).max(0);
        let right = (i64::from(x) + i64::from(width)).min(i64::from(self.width));
        let bottom = (i64::from(y) + i64::from(height)).min(i64::from(self.height));
        if left >= right || top >= bottom {
            return;
        }
        for destination_y in top..bottom {
            let row =
                usize::try_from((destination_y * i64::from(self.width) + left) * 4).unwrap_or(0);
            for destination_x in left..right {
                let offset = row + usize::try_from(destination_x - left).unwrap_or(0) * 4;
                if let Some(target) = self.pixels.get_mut(offset..offset + 4) {
                    target.copy_from_slice(&color);
                }
            }
        }
    }

    /// 把一张图以 source-over 方式贴到 `(x, y)`。
    ///
    /// * `x`/`y` 可以为负（左侧/上方越界会被裁掉）；
    /// * 整张图完全在帧外时直接返回；
    /// * 源图 alpha 为 0 的像素不改变目标；alpha 为 255 时直接覆盖；
    /// * 源图像的 `pixels` 长度不合法时不做任何事（防御性检查，正常解码不会出现）。
    pub fn blit(&mut self, image: &Image, x: i32, y: i32) {
        self.blit_impl(image, x, y, BlendMode::Over);
    }

    /// 把一张图以**加法混合**贴到 `(x, y)`：`out = min(255, dst + src * a / 255)`。
    ///
    /// 对应 Crystal 的 `DrawBlend`：`DXManager.SetBlend(true)` 默认把
    /// `DestBlend` 设成 `Blend.One`（`DXManager.cs` 的 `SetBlend`），
    /// 所以混合前层（灯光、光晕）是叠加而不是覆盖。纯黑像素叠加后不改变画面，
    /// 正好对应瓦片素材用纯黑表示透明的做法。
    pub fn blit_add(&mut self, image: &Image, x: i32, y: i32) {
        self.blit_impl(image, x, y, BlendMode::Add);
    }

    /// blit 的公共实现；`mode` 决定混合方式。
    fn blit_impl(&mut self, image: &Image, x: i32, y: i32, mode: BlendMode) {
        let source_width = i32::from(image.info.width);
        let source_height = i32::from(image.info.height);
        if source_width <= 0 || source_height <= 0 {
            return;
        }
        let needed = usize::try_from(source_width)
            .ok()
            .and_then(|w| {
                usize::try_from(source_height)
                    .ok()
                    .and_then(|h| w.checked_mul(h))
            })
            .and_then(|pixels| pixels.checked_mul(4));
        if needed.is_none_or(|needed| image.pixels.len() < needed) {
            return;
        }

        // 裁剪到帧内。用 i64 避免 `x + 宽度` 在极端坐标下溢出。
        let frame_width = i64::from(self.width);
        let frame_height = i64::from(self.height);
        let left = i64::from(x).max(0);
        let top = i64::from(y).max(0);
        let right = (i64::from(x) + i64::from(source_width)).min(frame_width);
        let bottom = (i64::from(y) + i64::from(source_height)).min(frame_height);
        if left >= right || top >= bottom {
            return;
        }

        for destination_y in top..bottom {
            let source_y = destination_y - i64::from(y);
            let source_row = usize::try_from(source_y * i64::from(source_width) * 4).unwrap_or(0);
            let destination_row =
                usize::try_from((destination_y * i64::from(self.width) + left) * 4).unwrap_or(0);
            for destination_x in left..right {
                let source_x = destination_x - i64::from(x);
                let source_offset = source_row + usize::try_from(source_x * 4).unwrap_or(0);
                let destination_offset =
                    destination_row + usize::try_from((destination_x - left) * 4).unwrap_or(0);
                let Some(source) = image.pixels.get(source_offset..source_offset + 4) else {
                    break;
                };
                blend_pixel(&mut self.pixels, destination_offset, source, mode);
            }
        }
    }

    /// 整帧不同颜色的数量（按 `0xRRGGBB` 归并，忽略 alpha）。
    ///
    /// 集成测试用它判断「不是一整片纯色」；调用方也能用它做快速自检。
    #[must_use]
    pub fn distinct_color_count(&self) -> usize {
        let mut colors = std::collections::HashSet::new();
        for pixel in self.pixels.chunks_exact(4) {
            colors
                .insert(u32::from(pixel[0]) << 16 | u32::from(pixel[1]) << 8 | u32::from(pixel[2]));
        }
        colors.len()
    }

    /// 把帧写成 PNG（RGBA、8 位深），自动创建父目录。
    ///
    /// # Errors
    /// 目录创建、文件创建或 PNG 编码失败时返回 [`RenderError`]。
    pub fn save_png(&self, path: impl AsRef<Path>) -> Result<(), RenderError> {
        let path: PathBuf = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| RenderError::CreatePng {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let file = std::fs::File::create(&path).map_err(|source| RenderError::CreatePng {
            path: path.clone(),
            source,
        })?;
        let mut encoder = png::Encoder::new(file, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&self.pixels)?;
        Ok(())
    }

    /// 用不透明黑清屏，等价于 `clear(BACKGROUND)`。
    pub fn clear_black(&mut self) {
        self.clear(BACKGROUND);
    }
}

/// 一帧之内某个像素的字节偏移。
fn pixel_offset(width: u32, x: u32, y: u32) -> usize {
    (usize::try_from(y).unwrap_or(0) * usize::try_from(width).unwrap_or(0)
        + usize::try_from(x).unwrap_or(0))
        * 4
}

/// 混合方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlendMode {
    /// source-over：`dst = src * a + dst * (1 - a)`。
    Over,
    /// 加法：`dst = dst + src * a`（Crystal `DrawBlend` 的默认状态）。
    Add,
}

/// 把一个源像素混进目标缓冲。
fn blend_pixel(destination: &mut [u8], offset: usize, source: &[u8], mode: BlendMode) {
    let Some(target) = destination.get_mut(offset..offset + 4) else {
        return;
    };
    let alpha = source[3];
    if alpha == 0 {
        return;
    }
    match mode {
        BlendMode::Over => {
            if alpha == 255 {
                target[0] = source[0];
                target[1] = source[1];
                target[2] = source[2];
            } else {
                for channel in 0..3 {
                    target[channel] = blend_channel(target[channel], source[channel], alpha);
                }
            }
        }
        BlendMode::Add => {
            for channel in 0..3 {
                target[channel] = add_channel(target[channel], source[channel], alpha);
            }
        }
    }
    target[3] = 255;
}

/// 单个通道的 source-over 混合（四舍五入）。
///
/// 公开成 `pub(crate)` 便于单元测试直接验证取值。
pub(crate) fn blend_channel(destination: u8, source: u8, alpha: u8) -> u8 {
    let alpha = i32::from(alpha);
    let source = i32::from(source);
    let destination = i32::from(destination);
    u8::try_from((source * alpha + destination * (255 - alpha) + 127) / 255).unwrap_or(255)
}

/// 单个通道的加法混合，结果饱和到 255。
pub(crate) fn add_channel(destination: u8, source: u8, alpha: u8) -> u8 {
    let contribution = (i32::from(source) * i32::from(alpha) + 127) / 255;
    u8::try_from((i32::from(destination) + contribution).min(255)).unwrap_or(255)
}

#[cfg(test)]
mod tests {
    use mir_asset::{ImageInfo, format};

    use super::*;

    fn test_image(width: u16, height: u16, pixels: Vec<u8>) -> Image {
        Image {
            info: ImageInfo {
                width,
                height,
                offset_x: 0,
                offset_y: 0,
                format: format::RGBA32_GZIP,
                shadow_x: 0,
                shadow_y: 0,
                reserved: [0; 3],
                has_mask: false,
                data_offset: 0,
                data_len: 0,
            },
            pixels,
        }
    }

    /// 2×2：左上红、右上绿、左下蓝、右下半透明白。
    fn two_by_two() -> Image {
        test_image(
            2,
            2,
            vec![
                255, 0, 0, 255, // (0,0) 红
                0, 255, 0, 255, // (1,0) 绿
                0, 0, 255, 255, // (0,1) 蓝
                255, 255, 255, 0, // (1,1) 全透明
            ],
        )
    }

    #[test]
    fn clear_fills_every_pixel() {
        let mut frame = Frame::new(3, 2);
        frame.clear([1, 2, 3, 255]);
        assert_eq!(frame.pixels.len(), 3 * 2 * 4);
        assert_eq!(frame.pixel(0, 0), Some([1, 2, 3, 255]));
        assert_eq!(frame.pixel(2, 1), Some([1, 2, 3, 255]));
        assert_eq!(frame.pixel(3, 1), None);
    }

    #[test]
    fn fill_rect_clips_to_the_frame() {
        let mut frame = Frame::new(4, 4);
        frame.clear([0, 0, 0, 255]);
        frame.fill_rect(1, 1, 2, 2, [255, 0, 0, 255]);
        assert_eq!(frame.pixel(1, 1), Some([255, 0, 0, 255]));
        assert_eq!(frame.pixel(2, 2), Some([255, 0, 0, 255]));
        assert_eq!(frame.pixel(0, 0), Some([0, 0, 0, 255]));
        assert_eq!(frame.pixel(3, 3), Some([0, 0, 0, 255]));

        // 越界部分被裁剪，不 panic。
        frame.fill_rect(-2, -2, 3, 3, [0, 255, 0, 255]);
        assert_eq!(frame.pixel(0, 0), Some([0, 255, 0, 255]));
        frame.fill_rect(3, 3, 10, 10, [0, 0, 255, 255]);
        assert_eq!(frame.pixel(3, 3), Some([0, 0, 255, 255]));
        assert_eq!(frame.pixel(2, 2), Some([255, 0, 0, 255]));
    }

    #[test]
    fn blit_at_origin_uses_source_over() {
        let mut frame = Frame::new(2, 2);
        frame.clear([0, 0, 0, 255]);
        frame.blit(&two_by_two(), 0, 0);
        assert_eq!(frame.pixel(0, 0), Some([255, 0, 0, 255]));
        assert_eq!(frame.pixel(1, 0), Some([0, 255, 0, 255]));
        assert_eq!(frame.pixel(0, 1), Some([0, 0, 255, 255]));
        // 全透明像素不改变目标。
        assert_eq!(frame.pixel(1, 1), Some([0, 0, 0, 255]));
    }

    #[test]
    fn blit_clips_negative_coordinates() {
        let mut frame = Frame::new(4, 4);
        frame.clear([0, 0, 0, 255]);
        frame.blit(&two_by_two(), -1, -1);
        // 只有源图右下角（透明）落在 (0,0)，因此什么也看不到。
        assert_eq!(frame.pixel(0, 0), Some([0, 0, 0, 255]));

        let mut frame = Frame::new(4, 4);
        frame.clear([0, 0, 0, 255]);
        frame.blit(&two_by_two(), -1, 0);
        // 源图右上角绿色落在 (0,0)。
        assert_eq!(frame.pixel(0, 0), Some([0, 255, 0, 255]));
        assert_eq!(frame.pixel(1, 0), Some([0, 0, 0, 255]));
    }

    #[test]
    fn blit_clips_right_and_bottom_edges() {
        let mut frame = Frame::new(1, 1);
        frame.clear([0, 0, 0, 255]);
        frame.blit(&two_by_two(), 0, 0);
        assert_eq!(frame.pixel(0, 0), Some([255, 0, 0, 255]));
    }

    #[test]
    fn blit_offscreen_does_nothing() {
        let mut frame = Frame::new(4, 4);
        frame.clear([9, 9, 9, 255]);
        let before = frame.pixels.clone();
        frame.blit(&two_by_two(), 4, 0);
        frame.blit(&two_by_two(), 0, 4);
        frame.blit(&two_by_two(), -2, 0);
        frame.blit(&two_by_two(), 0, -2);
        frame.blit(&two_by_two(), i32::MIN, i32::MIN);
        assert_eq!(frame.pixels, before);
    }

    #[test]
    fn alpha_blending_matches_rounded_source_over() {
        // alpha=128 的红叠在蓝上：R = 255*128/255 = 128，B = 255*127/255 = 127。
        assert_eq!(blend_channel(0, 255, 128), 128);
        assert_eq!(blend_channel(255, 0, 128), 127);
        // alpha=0 与 alpha=255 是精确端点。
        assert_eq!(blend_channel(17, 200, 0), 17);
        assert_eq!(blend_channel(17, 200, 255), 200);

        let image = test_image(1, 1, vec![255, 0, 0, 128]);
        let mut frame = Frame::new(1, 1);
        frame.clear([0, 0, 255, 255]);
        frame.blit(&image, 0, 0);
        assert_eq!(frame.pixel(0, 0), Some([128, 0, 127, 255]));

        let image = test_image(1, 1, vec![255, 255, 255, 0]);
        frame.blit(&image, 0, 0);
        assert_eq!(frame.pixel(0, 0), Some([128, 0, 127, 255]));
    }

    #[test]
    fn additive_blending_saturates() {
        // 纯黑叠加不改变目标（灯光图的背景）。
        assert_eq!(add_channel(30, 0, 255), 30);
        // 白色叠加饱和。
        assert_eq!(add_channel(200, 255, 255), 255);
        // 50% 半透明白叠加一半。
        assert_eq!(add_channel(100, 255, 128), 228);

        let image = test_image(1, 1, vec![0, 0, 0, 255]);
        let mut frame = Frame::new(1, 1);
        frame.clear([40, 50, 60, 255]);
        frame.blit_add(&image, 0, 0);
        assert_eq!(frame.pixel(0, 0), Some([40, 50, 60, 255]));

        let image = test_image(1, 1, vec![255, 255, 255, 255]);
        frame.blit_add(&image, 0, 0);
        assert_eq!(frame.pixel(0, 0), Some([255, 255, 255, 255]));
        // alpha=0 的像素不做任何事。
        let image = test_image(1, 1, vec![0, 0, 0, 0]);
        frame.blit_add(&image, 0, 0);
        assert_eq!(frame.pixel(0, 0), Some([255, 255, 255, 255]));
    }

    #[test]
    fn malformed_image_pixels_are_ignored() {
        let mut frame = Frame::new(2, 2);
        frame.clear([5, 5, 5, 255]);
        let image = test_image(2, 2, vec![255, 0, 0, 255]);
        let before = frame.pixels.clone();
        frame.blit(&image, 0, 0);
        assert_eq!(frame.pixels, before);
    }

    #[test]
    fn distinct_colors_counts_rgb_only() {
        let mut frame = Frame::new(2, 1);
        frame.clear([0, 0, 0, 255]);
        frame.pixels[0..4].copy_from_slice(&[10, 20, 30, 0]);
        assert_eq!(frame.distinct_color_count(), 2);
    }
}
