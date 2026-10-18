//! TrueType 文字光栅化：把中英文文本画进 [`Frame`]。
//!
//! 客户端是中文优先的，界面文字（生命/魔法/金币/背包/商店/对话）必须能显示中文，
//! 所以这里用 [`ab_glyph`] 直接栅格化系统字体，而不是像 F1 调试叠层那样用 3×5 点阵。
//!
//! # 字体发现顺序
//!
//! [`TextRenderer::discover`] 按下面的顺序逐个尝试，第一个能成功解析的字体胜出：
//!
//! 1. `resource/fonts/*.ttf|*.otf|*.ttc`（工作目录与编译期工作区根各找一次，按文件名排序）；
//! 2. Windows：`%WINDIR%\Fonts\msyh.ttc` → `simhei.ttf` → `simsun.ttc`；
//! 3. Linux：常见 `DejaVu` / `Noto CJK` 路径，再递归扫 `/usr/share/fonts`（最多 4 层）。
//!
//! 一个都找不到时返回 [`TextError::NoFont`]；调用方（`mir-client` 的 `UiText`）退回
//! [`pixel`] 里的 3×5 点阵字并打印警告。**任何情况下都不会 panic**：字体文件损坏、
//! 目录不可读、`.ttc` 集合索引越界都只是让候选字体被跳过。
//!
//! # 缓存与布局
//!
//! 每个 `(字符, 字号)` 的栅格结果缓存在 `HashMap` 里（[`TextRenderer::cache_len`] 可查），
//! 同一帧里重复出现的字只栅格化一次。排版是简单的横向累进：按 `h_advance` 前进，
//! 并叠加 `kern`；不支持竖排、双向文字与字体回退链。`\n` 换行，`\r` 忽略。
//!
//! # 坐标
//!
//! `draw(frame, x, y, text, size, color)` 的 `(x, y)` 是**文字行的左上角**，
//! 基线由字体 ascent 自动算出；`size` 是像素字号（`PxScale`），不是磅值。

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ab_glyph::{Font, FontArc, FontVec, PxScale, ScaleFont, point};

use crate::Frame;

/// 支持的字体扩展名。
pub const FONT_EXTENSIONS: [&str; 3] = ["ttf", "otf", "ttc"];
/// 单帧字形缓存上限；超过后整体清空（正常界面同时出现的字形远小于这个数）。
pub const MAX_CACHED_GLYPHS: usize = 4096;
/// 字体库目录（相对工作目录）。
pub const FONT_DIRECTORY: &str = "resource/fonts";

/// 文字光栅化的错误。
#[derive(Debug, thiserror::Error)]
pub enum TextError {
    /// 候选字体一个都不存在。
    #[error("找不到可用字体（resource/fonts 与系统字体目录都没有 TrueType 字体）")]
    NoFont,
    /// 字体文件读取失败。
    #[error("读取字体 {path} 失败：{source}")]
    Io {
        /// 字体路径。
        path: PathBuf,
        /// 底层 I/O 错误。
        #[source]
        source: std::io::Error,
    },
    /// 字体数据无法解析。
    #[error("字体 {path} 不是有效的 TrueType/OpenType 文件：{source}")]
    InvalidFont {
        /// 字体路径。
        path: PathBuf,
        /// `ab_glyph` 的解析错误。
        #[source]
        source: ab_glyph::InvalidFont,
    },
}

/// 像素矩形裁剪区（`max` 为开区间）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextClip {
    /// 左边界（含）。
    pub min_x: i32,
    /// 上边界（含）。
    pub min_y: i32,
    /// 右边界（不含）。
    pub max_x: i32,
    /// 下边界（不含）。
    pub max_y: i32,
}

impl TextClip {
    /// 用左上角与宽高构造。
    #[must_use]
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            min_x: x,
            min_y: y,
            max_x: x.saturating_add(i32::try_from(width).unwrap_or(i32::MAX)),
            max_y: y.saturating_add(i32::try_from(height).unwrap_or(i32::MAX)),
        }
    }

    /// 整帧范围。
    #[must_use]
    pub fn frame(frame: &Frame) -> Self {
        Self::new(0, 0, frame.width, frame.height)
    }

    /// 是否包含某个像素。
    #[must_use]
    pub const fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.min_x && x < self.max_x && y >= self.min_y && y < self.max_y
    }

    /// 两个裁剪区求交；没有交集时返回空矩形。
    #[must_use]
    pub fn intersect(self, other: Self) -> Self {
        let result = Self {
            min_x: self.min_x.max(other.min_x),
            min_y: self.min_y.max(other.min_y),
            max_x: self.max_x.min(other.max_x),
            max_y: self.max_y.min(other.max_y),
        };
        if result.max_x < result.min_x || result.max_y < result.min_y {
            Self {
                min_x: 0,
                min_y: 0,
                max_x: 0,
                max_y: 0,
            }
        } else {
            result
        }
    }

    /// 是否为空（不会画出任何像素）。
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.max_x <= self.min_x || self.max_y <= self.min_y
    }
}

/// 一个已栅格化的字形。
#[derive(Debug, Clone)]
struct CachedGlyph {
    /// 位图宽（像素）。
    width: u32,
    /// 位图高（像素）。
    height: u32,
    /// 位图左边界相对笔位置（基线原点）的偏移。
    left: i32,
    /// 位图上边界相对基线的偏移（通常是负数）。
    top: i32,
    /// 行优先覆盖度（0..=255）。
    mask: Vec<u8>,
}

/// `(字符, 字号) → 字形` 的缓存类型。
type GlyphCache = HashMap<(char, u32), Option<Rc<CachedGlyph>>>;

/// TrueType 文字渲染器。
///
/// 字体只加载一次；字形按需栅格化并缓存。渲染器不是 `Send`（缓存用 [`std::rc::Rc`]），
/// 和 winit 事件循环一样只活在主线程。
pub struct TextRenderer {
    /// 已加载的字体。
    font: FontArc,
    /// 字体路径（诊断用）。
    path: PathBuf,
    /// `(字符, 字号) → 字形`；`None` 表示该字形为空（空格或字体缺字）。
    glyphs: RefCell<GlyphCache>,
}

impl fmt::Debug for TextRenderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextRenderer")
            .field("path", &self.path)
            .field("cached_glyphs", &self.cache_len())
            .finish_non_exhaustive()
    }
}

impl TextRenderer {
    /// 从一个明确的字体文件加载。
    ///
    /// `.ttc` 字体集合取第 0 个字面。
    ///
    /// # Errors
    /// 文件不存在/不可读时返回 [`TextError::Io`]，数据不是有效字体时返回
    /// [`TextError::InvalidFont`]。
    pub fn new(path: impl AsRef<Path>) -> Result<Self, TextError> {
        let path = path.as_ref().to_path_buf();
        let data = std::fs::read(&path).map_err(|source| TextError::Io {
            path: path.clone(),
            source,
        })?;
        let font =
            FontVec::try_from_vec_and_index(data, 0).map_err(|source| TextError::InvalidFont {
                path: path.clone(),
                source,
            })?;
        Ok(Self {
            font: FontArc::from(font),
            path,
            glyphs: RefCell::new(HashMap::new()),
        })
    }

    /// 按本模块文档列出的顺序自动寻找字体。
    ///
    /// 本项目默认全中文，所以**优先选带中文字形的字体**：常见 Linux 发行版把 `DejaVu` 排在
    /// 系统字体扫描结果前面，而它没有 CJK 字形，直接用会让界面中文全变方块。只有在所有
    /// 候选都不含中文字形时才退回第一个能解析的字体（并打印警告）。
    ///
    /// # Errors
    /// 没有任何候选字体、或候选字体全部无法解析时返回错误。
    pub fn discover() -> Result<Self, TextError> {
        let mut last_error = None;
        let mut fallback = None;
        for path in font_candidates() {
            if !path.is_file() {
                continue;
            }
            match Self::new(&path) {
                Ok(renderer) => {
                    if renderer.covers_cjk() {
                        return Ok(renderer);
                    }
                    if fallback.is_none() {
                        fallback = Some(renderer);
                    }
                }
                Err(error) => {
                    eprintln!("警告：字体 {} 不可用：{error}", path.display());
                    last_error = Some(error);
                }
            }
        }
        if let Some(renderer) = fallback {
            eprintln!(
                "警告：没有找到含中文字形的字体，退回 {}（界面中文会显示为方块；\
                 可把中文字体放进 resource/fonts 或安装 fonts-noto-cjk）",
                renderer.path.display()
            );
            return Ok(renderer);
        }
        match last_error {
            Some(error) => Err(error),
            None => Err(TextError::NoFont),
        }
    }

    /// 该字体是否真的带中文字形（`中` 的 glyph id 不是 `.notdef`）。
    #[must_use]
    pub fn covers_cjk(&self) -> bool {
        self.font.glyph_id('中') != ab_glyph::GlyphId(0)
    }

    /// 当前使用的字体路径。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 已缓存的字形数量（含空字形）。
    #[must_use]
    pub fn cache_len(&self) -> usize {
        self.glyphs.borrow().len()
    }

    /// 一段文字在指定字号下的行高（像素）。
    #[must_use]
    pub fn line_height(&self, size: u32) -> u32 {
        let scaled = self.font.as_scaled(PxScale::from(f32_from(size)));
        let height = scaled.height().ceil();
        if height.is_finite() && height > 1.0 {
            height as u32
        } else {
            size.max(1)
        }
    }

    /// 基线到行顶的距离（像素）。
    #[must_use]
    pub fn ascent(&self, size: u32) -> u32 {
        let ascent = self.font.as_scaled(PxScale::from(f32_from(size))).ascent();
        if ascent.is_finite() && ascent > 0.0 {
            ascent.round() as u32
        } else {
            size
        }
    }

    /// 一段文字的最大行宽（像素）；`\n` 分行后取最宽的一行。
    #[must_use]
    pub fn text_width(&self, text: &str, size: u32) -> u32 {
        let scaled = self.font.as_scaled(PxScale::from(f32_from(size)));
        let mut widest = 0.0f32;
        for line in text.split('\n') {
            let mut width = 0.0f32;
            let mut previous = None;
            for character in line.chars() {
                let id = self.font.glyph_id(character);
                if let Some(previous) = previous {
                    width += scaled.kern(previous, id);
                }
                width += scaled.h_advance(id);
                previous = Some(id);
            }
            widest = widest.max(width);
        }
        if widest.is_finite() && widest > 0.0 {
            widest.ceil() as u32
        } else {
            0
        }
    }

    /// 把一段文字画到帧上（裁剪到帧边界）。
    ///
    /// `(x, y)` 是文字行左上角；超长文本不会自动换行（只有 `\n` 换行）。
    pub fn draw(&self, frame: &mut Frame, x: i32, y: i32, text: &str, size: u32, color: [u8; 4]) {
        let clip = TextClip::frame(frame);
        self.draw_clipped(frame, x, y, text, size, color, clip);
    }

    /// 把一段文字画到帧上，并裁剪到 `clip`。
    // 位置 + 字号 + 颜色 + 裁剪区是光栅化原语的天然参数，拆成结构体反而让调用方更啰嗦。
    #[allow(clippy::too_many_arguments)]
    pub fn draw_clipped(
        &self,
        frame: &mut Frame,
        x: i32,
        y: i32,
        text: &str,
        size: u32,
        color: [u8; 4],
        clip: TextClip,
    ) {
        if size == 0 || color[3] == 0 || clip.is_empty() {
            return;
        }
        let line_height = self.line_height(size);
        if line_height == 0 {
            return;
        }
        let baseline_offset = i32::try_from(self.ascent(size)).unwrap_or(0);
        let scaled = self.font.as_scaled(PxScale::from(f32_from(size)));
        let mut line_top = y;
        for line in text.split('\n') {
            let baseline = line_top.saturating_add(baseline_offset);
            let mut pen_x = f32_from_signed(x);
            let mut previous = None;
            for character in line.chars() {
                if character == '\r' {
                    continue;
                }
                let id = self.font.glyph_id(character);
                if let Some(previous) = previous {
                    pen_x += scaled.kern(previous, id);
                }
                let advance = scaled.h_advance(id);
                if let Some(glyph) = self.glyph(character, size) {
                    blit_glyph(&glyph, frame, pen_x, baseline, color, clip);
                }
                pen_x += advance;
                previous = Some(id);
            }
            line_top = line_top.saturating_add(i32::try_from(line_height).unwrap_or(0));
        }
    }

    /// 把一个字形覆盖度混合到帧上。
    /// 取一个字形（带缓存）。
    fn glyph(&self, character: char, size: u32) -> Option<Rc<CachedGlyph>> {
        let key = (character, size);
        if let Some(cached) = self.glyphs.borrow().get(&key) {
            return cached.clone();
        }
        if self.glyphs.borrow().len() >= MAX_CACHED_GLYPHS {
            self.glyphs.borrow_mut().clear();
        }
        let rasterized = self.rasterize(character, size).map(Rc::new);
        self.glyphs.borrow_mut().insert(key, rasterized.clone());
        rasterized
    }

    /// 栅格化一个字形；空白/缺字返回 `None`。
    fn rasterize(&self, character: char, size: u32) -> Option<CachedGlyph> {
        let scale = PxScale::from(f32_from(size));
        let id = self.font.glyph_id(character);
        if character.is_whitespace() || id.0 == 0 {
            return None;
        }
        let glyph = id.with_scale_and_position(scale, point(0.0, 0.0));
        let outlined = self.font.outline_glyph(glyph)?;
        let bounds = outlined.px_bounds();
        let width = if bounds.width().is_finite() && bounds.width() > 0.0 {
            bounds.width() as u32
        } else {
            return None;
        };
        let height = if bounds.height().is_finite() && bounds.height() > 0.0 {
            bounds.height() as u32
        } else {
            return None;
        };
        let mut mask = vec![0u8; usize::try_from(width.checked_mul(height)?).ok()?];
        outlined.draw(|x, y, coverage| {
            let (Ok(x), Ok(y), Ok(row_width)) = (
                usize::try_from(x),
                usize::try_from(y),
                usize::try_from(width),
            ) else {
                return;
            };
            let index = y.checked_mul(row_width).and_then(|row| row.checked_add(x));
            if let Some(slot) = index.and_then(|index| mask.get_mut(index)) {
                let value = (coverage.clamp(0.0, 1.0) * 255.0).round();
                *slot = value as u8;
            }
        });
        Some(CachedGlyph {
            width,
            height,
            left: bounds.min.x.round() as i32,
            top: bounds.min.y.round() as i32,
            mask,
        })
    }
}

/// 把一个字形覆盖度混合到帧上。
fn blit_glyph(
    glyph: &CachedGlyph,
    frame: &mut Frame,
    pen_x: f32,
    baseline: i32,
    color: [u8; 4],
    clip: TextClip,
) {
    if glyph.mask.is_empty() || glyph.width == 0 || glyph.height == 0 {
        return;
    }
    let origin_x = pen_x.round() as i32 + glyph.left;
    for row in 0..glyph.height {
        let destination_y = baseline + glyph.top + i32::try_from(row).unwrap_or(0);
        if destination_y < clip.min_y || destination_y >= clip.max_y {
            continue;
        }
        for column in 0..glyph.width {
            let destination_x = origin_x + i32::try_from(column).unwrap_or(0);
            if destination_x < clip.min_x || destination_x >= clip.max_x {
                continue;
            }
            let index = usize::try_from(row * glyph.width + column).unwrap_or(usize::MAX);
            let coverage = glyph.mask.get(index).copied().unwrap_or(0);
            if coverage == 0 {
                continue;
            }
            let alpha = ((u32::from(color[3]) * u32::from(coverage) + 127) / 255) as u8;
            frame.blend_pixel(destination_x, destination_y, color, alpha);
        }
    }
}

/// 字号 → 浮点（`u32 as f32` 的集中写法；字号远小于 f32 精度上限）。
// 字号是界面常量（<= 200），转 f32 不会损失有效像素精度。
#[allow(clippy::cast_precision_loss)]
fn f32_from(value: u32) -> f32 {
    value as f32
}

/// 坐标 → 浮点（负坐标也能正确转换）。
#[allow(clippy::cast_precision_loss)]
fn f32_from_signed(value: i32) -> f32 {
    value as f32
}

/// 按模块文档的顺序列出候选字体路径（可能包含不存在的路径）。
#[must_use]
pub fn font_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for directory in font_directories() {
        for file in font_files_in(&directory) {
            push_unique(&mut candidates, file);
        }
    }
    for file in windows_font_candidates() {
        push_unique(&mut candidates, file);
    }
    for file in linux_font_candidates() {
        push_unique(&mut candidates, file);
    }
    candidates
}

/// 追加一个路径（保持首次出现的顺序）。
fn push_unique(candidates: &mut Vec<PathBuf>, path: PathBuf) {
    if !candidates.contains(&path) {
        candidates.push(path);
    }
}

/// `resource/fonts` 的候选目录：工作目录 + 编译期工作区根。
fn font_directories() -> Vec<PathBuf> {
    let mut directories = vec![PathBuf::from(FONT_DIRECTORY)];
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest.join("../..");
    let workspace_fonts = workspace_root.join(FONT_DIRECTORY);
    if !directories.contains(&workspace_fonts) {
        directories.push(workspace_fonts);
    }
    directories
}

/// 目录里所有字体文件（按文件名排序）。
fn font_files_in(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_font_file(path))
        .collect();
    files.sort();
    files
}

/// 是否是受支持的字体文件。
fn is_font_file(path: &Path) -> bool {
    let Some(extension) = path.extension() else {
        return false;
    };
    let extension = extension.to_string_lossy().to_ascii_lowercase();
    FONT_EXTENSIONS.contains(&extension.as_str())
}

/// Windows 系统字体候选（`%WINDIR%\Fonts`）。
fn windows_font_candidates() -> Vec<PathBuf> {
    let mut root = std::env::var("WINDIR")
        .or_else(|_| std::env::var("SystemRoot"))
        .unwrap_or_else(|_| "C:\\Windows".to_owned());
    if root.is_empty() {
        return Vec::new();
    }
    if !root.ends_with('\\') && !root.ends_with('/') {
        root.push('\\');
    }
    let fonts = Path::new(&root).join("Fonts");
    ["msyh.ttc", "simhei.ttf", "simsun.ttc"]
        .into_iter()
        .map(|name| fonts.join(name))
        .collect()
}

/// Linux 常见字体路径 + `/usr/share/fonts` 递归扫描。
///
/// 顺序很重要：本项目默认全中文，**优先选带 CJK 字形的字体**，再把 `DejaVu` 作为兜底
/// （它没有中文字形，一旦排在前面，界面中文会全变成方块）。
#[cfg(target_os = "linux")]
fn linux_font_candidates() -> Vec<PathBuf> {
    let known = [
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansSC-Regular.otf",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    ];
    let mut candidates: Vec<PathBuf> = known.into_iter().map(PathBuf::from).collect();
    collect_font_files(Path::new("/usr/share/fonts"), 0, &mut candidates);
    candidates
}

/// 非 Linux 平台没有 Linux 字体路径。
#[cfg(not(target_os = "linux"))]
fn linux_font_candidates() -> Vec<PathBuf> {
    Vec::new()
}

/// 递归收集目录里的字体（最多 4 层），只收常见中文字体名。
#[cfg(target_os = "linux")]
fn collect_font_files(directory: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    const MAX_DEPTH: u32 = 4;
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut files = Vec::new();
    let mut directories = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            directories.push(path);
        } else if is_font_file(&path) {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if ["dejavu", "noto", "wqy", "droidsans", "sourcehansans"]
                .iter()
                .any(|needle| name.contains(needle))
            {
                files.push(path);
            }
        }
    }
    files.sort();
    out.extend(files);
    directories.sort();
    for directory in directories {
        collect_font_files(&directory, depth + 1, out);
    }
}

/// 3×5 点阵字：没有 TrueType 字体时的兜底。
///
/// 字模 3 列 × 5 行、每行 3 bit（高位在左），按 `scale` 放大。只覆盖 ASCII
/// （小写自动转大写）；中文会留下空白——这正是「没有字体」警告要提醒的降级状态。
pub mod pixel {
    use super::TextClip;
    use crate::Frame;

    /// 字模宽度（列）。
    pub const GLYPH_WIDTH: u32 = 3;
    /// 字模高度（行）。
    pub const GLYPH_HEIGHT: u32 = 5;
    /// 字符步进（3 列 + 1 列间隔）。
    pub const ADVANCE: u32 = 4;

    /// 一段文字的像素宽度。
    #[must_use]
    pub fn text_width(text: &str, scale: u32) -> u32 {
        let chars = text.chars().filter(|character| *character != '\n').count();
        u32::try_from(chars).unwrap_or(u32::MAX) * ADVANCE * scale.max(1)
    }

    /// 一行文字的高度（像素）。
    #[must_use]
    pub const fn line_height(scale: u32) -> u32 {
        GLYPH_HEIGHT * scale
    }

    /// 把点阵字画到帧上（裁剪到帧边界），`(x, y)` 是左上角。
    pub fn draw_text(frame: &mut Frame, x: i32, y: i32, text: &str, scale: u32, color: [u8; 4]) {
        let clip = TextClip::frame(frame);
        draw_text_clipped(frame, x, y, text, scale, color, clip);
    }

    /// 把点阵字画到帧上，并裁剪到 `clip`。
    pub fn draw_text_clipped(
        frame: &mut Frame,
        x: i32,
        y: i32,
        text: &str,
        scale: u32,
        color: [u8; 4],
        clip: TextClip,
    ) {
        let scale = scale.max(1);
        let mut origin_x = x;
        for character in text.chars() {
            if character == '\n' || character == '\r' {
                continue;
            }
            let glyph = glyph(character);
            for (row, bits) in glyph.iter().enumerate() {
                for column in 0..GLYPH_WIDTH {
                    if bits & (1 << (GLYPH_WIDTH - 1 - column)) == 0 {
                        continue;
                    }
                    let pixel_x = origin_x + i32::try_from(column * scale).unwrap_or(0);
                    let pixel_y =
                        y + i32::try_from(u32::try_from(row).unwrap_or(0) * scale).unwrap_or(0);
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let destination_x = pixel_x + i32::try_from(dx).unwrap_or(0);
                            let destination_y = pixel_y + i32::try_from(dy).unwrap_or(0);
                            if clip.contains(destination_x, destination_y) {
                                frame.set_pixel(
                                    u32::try_from(destination_x).unwrap_or(u32::MAX),
                                    u32::try_from(destination_y).unwrap_or(u32::MAX),
                                    color,
                                );
                            }
                        }
                    }
                }
            }
            origin_x += i32::try_from(ADVANCE * scale).unwrap_or(0);
        }
    }

    /// 带左上阴影的点阵字（叠层用；在任意底色上都可读）。
    pub fn draw_text_shadowed(
        frame: &mut Frame,
        x: i32,
        y: i32,
        text: &str,
        scale: u32,
        color: [u8; 4],
    ) {
        draw_text(frame, x + 1, y + 1, text, scale, [0, 0, 0, 255]);
        draw_text(frame, x, y, text, scale, color);
    }

    /// 取一个字符的字模；不认识的字符返回空格。
    #[must_use]
    pub fn glyph(character: char) -> [u8; 5] {
        match character.to_ascii_uppercase() {
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
            'A' => [0b010, 0b101, 0b111, 0b101, 0b101],
            'B' => [0b110, 0b101, 0b110, 0b101, 0b110],
            'C' => [0b011, 0b100, 0b100, 0b100, 0b011],
            'D' => [0b110, 0b101, 0b101, 0b101, 0b110],
            'E' => [0b111, 0b100, 0b110, 0b100, 0b111],
            'F' => [0b111, 0b100, 0b110, 0b100, 0b100],
            'G' => [0b011, 0b100, 0b101, 0b101, 0b011],
            'H' => [0b101, 0b101, 0b111, 0b101, 0b101],
            'I' => [0b111, 0b010, 0b010, 0b010, 0b111],
            'J' => [0b001, 0b001, 0b001, 0b101, 0b010],
            'K' => [0b101, 0b101, 0b110, 0b101, 0b101],
            'L' => [0b100, 0b100, 0b100, 0b100, 0b111],
            'M' => [0b101, 0b111, 0b111, 0b101, 0b101],
            'N' => [0b110, 0b101, 0b101, 0b101, 0b101],
            'O' => [0b010, 0b101, 0b101, 0b101, 0b010],
            'P' => [0b110, 0b101, 0b110, 0b100, 0b100],
            'Q' => [0b010, 0b101, 0b101, 0b110, 0b011],
            'R' => [0b110, 0b101, 0b110, 0b101, 0b101],
            'S' => [0b011, 0b100, 0b010, 0b001, 0b110],
            'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
            'U' => [0b101, 0b101, 0b101, 0b101, 0b011],
            'V' => [0b101, 0b101, 0b101, 0b101, 0b010],
            'W' => [0b101, 0b101, 0b111, 0b111, 0b101],
            'X' => [0b101, 0b101, 0b010, 0b101, 0b101],
            'Y' => [0b101, 0b101, 0b010, 0b010, 0b010],
            'Z' => [0b111, 0b001, 0b010, 0b100, 0b111],
            '-' => [0b000, 0b000, 0b111, 0b000, 0b000],
            ',' => [0b000, 0b000, 0b000, 0b010, 0b100],
            '.' => [0b000, 0b000, 0b000, 0b000, 0b010],
            ':' => [0b000, 0b010, 0b000, 0b010, 0b000],
            '/' => [0b001, 0b001, 0b010, 0b100, 0b100],
            '+' => [0b000, 0b010, 0b111, 0b010, 0b000],
            '%' => [0b101, 0b001, 0b010, 0b100, 0b101],
            '(' => [0b001, 0b010, 0b010, 0b010, 0b001],
            ')' => [0b100, 0b010, 0b010, 0b010, 0b100],
            '?' => [0b110, 0b001, 0b010, 0b000, 0b010],
            _ => [0, 0, 0, 0, 0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 找一台机器上的可用字体；没有字体时返回 `None`，测试据此跳过。
    fn renderer() -> Option<TextRenderer> {
        match TextRenderer::discover() {
            Ok(renderer) => Some(renderer),
            Err(error) => {
                eprintln!("跳过 TrueType 测试：{error}");
                None
            }
        }
    }

    #[test]
    fn missing_font_path_returns_an_error() {
        let error = TextRenderer::new("target/definitely-missing-font.ttf")
            .expect_err("不存在的字体必须报错");
        assert!(matches!(error, TextError::Io { .. }), "实际：{error}");
    }

    #[test]
    fn invalid_font_data_returns_an_error() {
        let path = std::env::temp_dir().join("mir-next-not-a-font.ttf");
        std::fs::write(&path, b"not a font").expect("写临时文件");
        let error = TextRenderer::new(&path).expect_err("坏字体必须报错");
        assert!(
            matches!(error, TextError::InvalidFont { .. }),
            "实际：{error}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn draws_ascii_and_chinese_pixels() {
        let Some(renderer) = renderer() else {
            return;
        };
        let mut ascii = Frame::new(240, 60);
        ascii.clear([0, 0, 0, 255]);
        renderer.draw(&mut ascii, 4, 4, "HP 15/15", 18, [255, 255, 255, 255]);
        assert!(
            ascii.distinct_color_count() > 1,
            "ASCII 文字应改变像素（字体：{}）",
            renderer.path().display()
        );

        let mut chinese = Frame::new(240, 60);
        chinese.clear([0, 0, 0, 255]);
        renderer.draw(
            &mut chinese,
            4,
            4,
            "生命 魔法 金币",
            18,
            [255, 255, 255, 255],
        );
        assert!(
            chinese.distinct_color_count() > 1,
            "中文文字应改变像素（字体：{}）",
            renderer.path().display()
        );
        assert!(
            renderer.text_width("生命 魔法", 18) > 40,
            "中文宽度应大于几十像素"
        );
    }

    #[test]
    fn text_width_is_monotonic_and_zero_for_empty() {
        let Some(renderer) = renderer() else {
            return;
        };
        assert_eq!(renderer.text_width("", 16), 0);
        let narrow = renderer.text_width("i", 16);
        let wide = renderer.text_width("你好世界", 16);
        let wider = renderer.text_width("你好世界你好世界", 16);
        assert!(narrow < wide, "narrow={narrow} wide={wide}");
        assert!(wide < wider, "wide={wide} wider={wider}");
        assert!(renderer.line_height(16) >= 16);
    }

    #[test]
    fn clipping_stops_pixels_outside_the_rect() {
        let Some(renderer) = renderer() else {
            return;
        };
        let mut frame = Frame::new(120, 40);
        frame.clear([0, 0, 0, 255]);
        let clip = TextClip::new(0, 0, 20, 10);
        renderer.draw_clipped(&mut frame, 0, 0, "WWWWWW", 18, [255, 0, 0, 255], clip);
        for y in 10..40 {
            for x in 0..120 {
                assert_eq!(
                    frame.pixel(x, y),
                    Some([0, 0, 0, 255]),
                    "裁剪区外 ({x},{y}) 不应被画到"
                );
            }
        }
    }

    #[test]
    fn pixel_fallback_draws_ascii_and_skips_chinese() {
        let mut frame = Frame::new(80, 20);
        frame.clear([0, 0, 0, 255]);
        pixel::draw_text(&mut frame, 0, 0, "HP 15", 2, [255, 255, 255, 255]);
        assert!(frame.distinct_color_count() > 1);
        assert_eq!(pixel::text_width("HP 15", 2), 5 * pixel::ADVANCE * 2);
        let mut chinese = Frame::new(40, 20);
        chinese.clear([0, 0, 0, 255]);
        pixel::draw_text(&mut chinese, 0, 0, "生命", 2, [255, 255, 255, 255]);
        assert_eq!(chinese.distinct_color_count(), 1, "点阵字没有中文字模");
        assert_eq!(pixel::glyph('?'), [0b110, 0b001, 0b010, 0, 0b010]);
        assert_eq!(pixel::glyph('h'), pixel::glyph('H'), "小写转大写");
    }

    #[test]
    fn clip_intersection_is_empty_when_disjoint() {
        let first = TextClip::new(0, 0, 10, 10);
        let second = TextClip::new(20, 20, 10, 10);
        assert!(first.intersect(second).is_empty());
    }
}
