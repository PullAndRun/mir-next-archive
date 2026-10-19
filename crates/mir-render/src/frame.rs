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

/// 把图像按最近邻缩放到不超过 `max_width × max_height`（不放大）。
///
/// 背包/商店的格子比素材里的物品图标小，直接用原图会压到相邻格子；这个函数
/// 保持纵横比缩到格子里。已经放得下时原样返回。空图/非法尺寸返回克隆。
#[must_use]
pub fn scale_to_fit(image: &Image, max_width: u32, max_height: u32) -> Image {
    let source_width = u32::from(image.info.width);
    let source_height = u32::from(image.info.height);
    if source_width == 0
        || source_height == 0
        || max_width == 0
        || max_height == 0
        || (source_width <= max_width && source_height <= max_height)
    {
        return image.clone();
    }
    let width_ratio = f64::from(max_width) / f64::from(source_width);
    let height_ratio = f64::from(max_height) / f64::from(source_height);
    let ratio = width_ratio.min(height_ratio);
    let target_width = ((f64::from(source_width) * ratio).floor() as u32).max(1);
    let target_height = ((f64::from(source_height) * ratio).floor() as u32).max(1);
    let mut pixels = vec![0u8; usize::try_from(target_width * target_height * 4).unwrap_or(0)];
    for y in 0..target_height {
        let source_y = y * source_height / target_height;
        for x in 0..target_width {
            let source_x = x * source_width / target_width;
            let source_offset =
                usize::try_from((source_y * source_width + source_x) * 4).unwrap_or(0);
            let target_offset = usize::try_from((y * target_width + x) * 4).unwrap_or(0);
            if let (Some(source), Some(target)) = (
                image.pixels.get(source_offset..source_offset + 4),
                pixels.get_mut(target_offset..target_offset + 4),
            ) {
                target.copy_from_slice(source);
            }
        }
    }
    Image {
        info: mir_asset::ImageInfo {
            width: u16::try_from(target_width).unwrap_or(u16::MAX),
            height: u16::try_from(target_height).unwrap_or(u16::MAX),
            offset_x: 0,
            offset_y: 0,
            format: mir_asset::format::RGBA32_GZIP,
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
    /// 帧缓冲是**不透明**画布：目标 alpha 一律写回 255，`color[3]` 被忽略。
    /// 需要真正半透明时用 [`Frame::fill_rect_alpha`]（把 alpha 混进 RGB）。
    /// 实体占位方块、血条、面板格子与调试标记用。宽或高为 0 时什么都不做。
    pub fn fill_rect(&mut self, x: i32, y: i32, width: u32, height: u32, color: [u8; 4]) {
        let left = i64::from(x).max(0);
        let top = i64::from(y).max(0);
        let right = (i64::from(x) + i64::from(width)).min(i64::from(self.width));
        let bottom = (i64::from(y) + i64::from(height)).min(i64::from(self.height));
        if left >= right || top >= bottom {
            return;
        }
        let opaque = [color[0], color[1], color[2], 255];
        for destination_y in top..bottom {
            let row =
                usize::try_from((destination_y * i64::from(self.width) + left) * 4).unwrap_or(0);
            for destination_x in left..right {
                let offset = row + usize::try_from(destination_x - left).unwrap_or(0) * 4;
                if let Some(target) = self.pixels.get_mut(offset..offset + 4) {
                    target.copy_from_slice(&opaque);
                }
            }
        }
    }

    /// 把一个像素以 source-over 方式混合进帧（`alpha` 与 `color[3]` 相乘）。
    ///
    /// 文字光栅化用：覆盖度作为 alpha 传进来，颜色本身可以是任意 RGBA。
    /// 越界时什么都不做。
    pub fn blend_pixel(&mut self, x: i32, y: i32, color: [u8; 4], alpha: u8) {
        let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
            return;
        };
        if x >= self.width || y >= self.height {
            return;
        }
        let offset = pixel_offset(self.width, x, y);
        let source = [color[0], color[1], color[2], alpha];
        blend_pixel(&mut self.pixels, offset, &source, BlendMode::Over);
    }

    /// 用带 alpha 的颜色填充矩形（半透明 UI 背板）。
    ///
    /// 与 [`Frame::fill_rect`] 一样自动裁剪越界部分；`color[3]` 是混合用的 alpha。
    pub fn fill_rect_alpha(&mut self, x: i32, y: i32, width: u32, height: u32, color: [u8; 4]) {
        if color[3] == 0 || width == 0 || height == 0 {
            return;
        }
        let left = i64::from(x).max(0);
        let top = i64::from(y).max(0);
        let right = (i64::from(x) + i64::from(width)).min(i64::from(self.width));
        let bottom = (i64::from(y) + i64::from(height)).min(i64::from(self.height));
        if left >= right || top >= bottom {
            return;
        }
        let source = [color[0], color[1], color[2], color[3]];
        for destination_y in top..bottom {
            let row =
                usize::try_from((destination_y * i64::from(self.width) + left) * 4).unwrap_or(0);
            for destination_x in left..right {
                let offset = row + usize::try_from(destination_x - left).unwrap_or(0) * 4;
                blend_pixel(&mut self.pixels, offset, &source, BlendMode::Over);
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

    /// 按预计算的「不透明段」贴图（`(row, col, len)`，坐标相对图像左上角）。
    ///
    /// 地图瓦片在 [`crate::MapScene`] 解码时算好段表，绘制时每个连续段一次
    /// `copy_from_slice`，跳过透明像素的逐点分支；段已裁剪到帧内。
    pub fn blit_spans(&mut self, image: &Image, spans: &[(u32, u32, u32)], x: i32, y: i32) {
        let source_width = i64::from(image.info.width);
        let frame_width = i64::from(self.width);
        let frame_height = i64::from(self.height);
        for &(row, column, length) in spans {
            let destination_y = i64::from(y) + i64::from(row);
            if destination_y < 0 || destination_y >= frame_height || length == 0 {
                continue;
            }
            let start_x = i64::from(x) + i64::from(column);
            let left = start_x.max(0);
            let right = (start_x + i64::from(length)).min(frame_width);
            if left >= right {
                continue;
            }
            let skipped = usize::try_from(left - start_x).unwrap_or(0);
            let pixels = usize::try_from(right - left).unwrap_or(0);
            let source_offset =
                usize::try_from((i64::from(row) * source_width + i64::from(column)) * 4)
                    .unwrap_or(0)
                    + skipped * 4;
            let destination_offset =
                usize::try_from((destination_y * frame_width + left) * 4).unwrap_or(0);
            if let (Some(source), Some(destination)) = (
                image.pixels.get(source_offset..source_offset + pixels * 4),
                self.pixels
                    .get_mut(destination_offset..destination_offset + pixels * 4),
            ) {
                destination.copy_from_slice(source);
            }
        }
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
            let row_pixels = usize::try_from(right - left).unwrap_or(0);
            if mode == BlendMode::Over {
                // 行内「不透明段」合批：连续 alpha=255 的像素一次 memcpy，
                // 只对半透明像素逐点混合。用切片直取（已裁剪，索引必在界内），
                // 避免逐像素 `get`/Option 检查。
                let source_x_offset = usize::try_from(left - i64::from(x)).unwrap_or(0) * 4;
                let Some(source_row_pixels) = image.pixels.get(
                    source_row + source_x_offset..source_row + source_x_offset + row_pixels * 4,
                ) else {
                    break;
                };
                let Some(destination_row_pixels) = self
                    .pixels
                    .get_mut(destination_row..destination_row + row_pixels * 4)
                else {
                    break;
                };
                let mut column = 0usize;
                while column < row_pixels {
                    let alpha = source_row_pixels[column * 4 + 3];
                    if alpha != 255 {
                        if alpha != 0 {
                            let source = &source_row_pixels[column * 4..column * 4 + 4];
                            let destination =
                                &mut destination_row_pixels[column * 4..column * 4 + 4];
                            let scale = u32::from(alpha);
                            for channel in 0..3 {
                                destination[channel] = u8::try_from(
                                    (u32::from(source[channel]) * scale
                                        + u32::from(destination[channel]) * (255 - scale))
                                        / 255,
                                )
                                .unwrap_or(255);
                            }
                        }
                        column += 1;
                        continue;
                    }
                    let run_start = column;
                    while column < row_pixels && source_row_pixels[column * 4 + 3] == 255 {
                        column += 1;
                    }
                    destination_row_pixels[run_start * 4..column * 4]
                        .copy_from_slice(&source_row_pixels[run_start * 4..column * 4]);
                }
                continue;
            }
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

    #[test]
    fn scale_to_fit_preserves_aspect_ratio_and_never_upscales() {
        let image = test_image(8, 4, vec![255; 8 * 4 * 4]);
        let scaled = scale_to_fit(&image, 4, 4);
        assert_eq!((scaled.info.width, scaled.info.height), (4, 2));
        assert_eq!(scaled.pixels.len(), 4 * 2 * 4);
        let untouched = scale_to_fit(&image, 100, 100);
        assert_eq!((untouched.info.width, untouched.info.height), (8, 4));
        assert_eq!(untouched.pixels, image.pixels);
    }

    #[test]
    fn fill_rect_always_writes_opaque_pixels() {
        let mut frame = Frame::new(1, 1);
        frame.fill_rect(0, 0, 1, 1, [10, 20, 30, 40]);
        assert_eq!(frame.pixel(0, 0), Some([10, 20, 30, 255]));
    }
}
