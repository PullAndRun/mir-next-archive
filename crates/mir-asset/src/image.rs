//! 图像头信息、像素缓冲与像素格式常量。

/// [`ImageInfo::format`] 的取值。
///
/// `.wzl` 的这个字节来自记录头第 0 字节；旧 `.Lib` 没有格式字节（像素是 gzip RGBA32
/// 或 DXT1，`MLibrary.cs:914`、`MLibraryV1.cs:482`），本实现用 [`RGBA32_GZIP`] /
/// [`DXT1`] 两个标记表示。
pub mod format {
    /// 0 号格式：语料里几乎都是空占位记录（`.wzl` 偏移表指向 48 的块，宽高与长度都是 0）。
    ///
    /// 注意 Crystal 只用「格式字节是否等于 5」区分 16 位与 8 位（`WeMadeLibrary.cs:319`），
    /// 所以格式 0 的记录**只要有数据**就按 8 位调色板解码（`mmap.wzl` 里有 50 条这样的地图图）；
    /// 判定空图请看 [`ImageInfo::is_empty`]。`format_name()` 对 0 与 3 都返回 `"palette8"`。
    pub const EMPTY: u8 = 0;
    /// 8 位调色板索引。`.wzl` 里格式字节是 3 的图像都属于这一种
    /// （`WeMadeLibrary.cs:464-467` 的 `palette[bytes[index++]]`）。
    pub const PALETTE8: u8 = 3;
    /// 16 位 RGB565。判定条件是格式字节等于 5（`WeMadeLibrary.cs:319`）。
    pub const RGB565: u8 = 5;
    /// 24 位 RGB。这不是 Crystal 的格式，语料里只有 2 条记录（`ui1.wzl`、
    /// `objects31.wzl`），解压后大小正好是 `宽×高×3`，按 24 位解码能看到原始水印文字。
    pub const RGB24: u8 = 6;
    /// 旧 `.Lib` 的 gzip RGBA32。只作为 [`ImageInfo::format`] 的标记，文件里没有这个字节。
    pub const RGBA32_GZIP: u8 = 0xFF;
    /// 旧 `.Lib` 的 DXT1/BC1（`LibraryEditor/Graphics/MLibraryV1.cs:433-442`）。
    /// 同样只是标记：`.Lib` 没有格式字节，本实现按数据开头/长度识别。
    pub const DXT1: u8 = 0xFE;
}

/// 单张图的头信息（不解码像素）。
///
/// 字段来源见各字段文档；`.wzl` 与 `.Lib` 的记录头字段并不完全一样，
/// 两个容器里不存在的字段用 0 / `false` 填充。
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo {
    /// 宽（像素）。
    pub width: u16,
    /// 高（像素）。
    pub height: u16,
    /// 绘制原点相对左上角的 X 偏移（`.Lib` 的 `X`、`.wzl` 的 `x`，`MLibrary.cs:886`、
    /// `WeMadeLibrary.cs:323`）。
    pub offset_x: i16,
    /// 绘制原点相对左上角的 Y 偏移。
    pub offset_y: i16,
    /// 像素格式，取值见 [`format`] 模块。
    pub format: u8,
    /// 阴影 X 偏移。只有 `.Lib` 有（`MLibrary.cs:887`）；`.wzl` 记录头没有这两个字段
    /// （`WeMadeLibrary.cs:322-325`），恒为 0。
    pub shadow_x: i16,
    /// 阴影 Y 偏移。只有 `.Lib` 有；`.wzl` 恒为 0。
    pub shadow_y: i16,
    /// 保留字节。
    ///
    /// * `.Lib`：`[shadow, 0, 0]`，`shadow` 最高位表示是否有第二层
    ///   （`MLibrary.cs:889-893`）。
    /// * `.wzl`：记录头第 1..4 字节，Crystal 只读第一个字节判断 16 位、其余三个直接跳过
    ///   （`WeMadeLibrary.cs:319-320`），这里原样保留。
    pub reserved: [u8; 3],
    /// 是否有第二层（mask）。`.Lib` 的 `shadow & 0x80`（`MLibrary.cs:893`）；
    /// `.wzl` 恒为 `false`。
    pub has_mask: bool,
    /// 像素数据在文件中的偏移。
    pub data_offset: u64,
    /// 像素数据的压缩长度（`.Lib` 是 gzip 长度，`.wzl` 是 zlib 长度）。
    pub data_len: u32,
}

impl ImageInfo {
    /// 是否为空图（宽或高为 0）。空图解码后 `pixels` 为空。
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// 格式的稳定名称，CLI 与日志用。
    #[must_use]
    pub const fn format_name(&self) -> &'static str {
        match self.format {
            format::EMPTY | format::PALETTE8 => "palette8",
            format::RGB565 => "rgb565",
            format::RGB24 => "rgb24",
            format::RGBA32_GZIP => "rgba32-gzip",
            format::DXT1 => "dxt1",
            _ => "unknown",
        }
    }
}

/// 把纯黑像素置为透明（**只对 8 位调色板格式生效**）。
///
/// 规则与渲染层 `mir-render::scene::remove_black` 完全一致：只处理
/// [`format::EMPTY`] / [`format::PALETTE8`]，且仅当 `R=G=B=0` 时把 alpha 置 0；
/// RGB565 / RGB24 / DXT1 / 真 RGBA 的 alpha **一个字节都不动**。
///
/// 为什么在拆包时烘焙：Crystal 的 `.Lib` 在写库时就做过这一步
/// （`LibraryEditor/Graphics/MLibraryV2.cs:419-426` 的 `removeBlack`），
/// 本项目渲染层也对调色板图做同样处理。拆包产物提前烘焙后，运行时的
/// `remove_black` 变成幂等操作，`UnpackedLibrary` 与 `.wzl` 两条路径画出来一样。
pub fn black_to_transparent(format_code: u8, pixels: &mut [u8]) {
    if !matches!(format_code, format::EMPTY | format::PALETTE8) {
        return;
    }
    for pixel in pixels.chunks_exact_mut(4) {
        if pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 0 {
            pixel[3] = 0;
        }
    }
}

/// 解码后的一张图：RGBA8、行优先、`width × height × 4` 字节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// 头信息（与实际解码用的头一致）。
    pub info: ImageInfo,
    /// 像素缓冲：`[R, G, B, A, R, G, B, A, ...]`，行优先、自上而下。
    ///
    /// 空图（[`ImageInfo::is_empty`]）时为空向量。
    pub pixels: Vec<u8>,
}

impl Image {
    /// 是否为没有像素的空图。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pixels.is_empty()
    }
}

/// 每行字节数：Crystal 的 `WidthBytes`（`WeMadeLibrary.cs:231-234`）：
/// `(((width * bits) + 31) >> 5) * 4`，即按 4 字节对齐。
#[cfg(feature = "legacy")]
pub(crate) fn row_bytes(bits_per_pixel: u32, width: u16) -> u64 {
    let bits = u64::from(width) * u64::from(bits_per_pixel);
    ((bits + 31) >> 5) * 4
}

/// 每种 `.wzl` 格式的每像素位数。
#[cfg(feature = "legacy")]
pub(crate) const fn pixel_bits(format: u8) -> u32 {
    match format {
        format::RGB565 => 16,
        format::RGB24 => 24,
        _ => 8,
    }
}

/// 16 位 RGB565 转 RGBA。
///
/// 与 Crystal 的 `convert16bitTo32bit` 完全一致（`WeMadeLibrary.cs:223-229`）：
/// `5/6/5` 位分别**左移** 3/2/3 位，不做低位复制，所以 `0xF800` 会得到 248 而不是 255；
/// alpha 恒为 255（`.wzl` 的 16 位图没有透明通道）。
#[cfg(feature = "legacy")]
pub(crate) fn expand_rgb565(value: u16) -> [u8; 4] {
    let red = ((value >> 11) & 0x1F) << 3;
    let green = ((value >> 5) & 0x3F) << 2;
    let blue = (value & 0x1F) << 3;
    [red as u8, green as u8, blue as u8, 0xFF]
}

/// 调色板索引转 RGBA：查 `WeMade` 256 色表，alpha 取表里的值（恒为 255）。
#[cfg(feature = "legacy")]
pub(crate) fn palette_rgba(index: u8) -> [u8; 4] {
    rgba_from_argb(crate::palette::WEMADE_PALETTE[usize::from(index)])
}

/// `0xAARRGGBB` 转 RGBA 字节序。
#[cfg(feature = "legacy")]
pub(crate) const fn rgba_from_argb(argb: u32) -> [u8; 4] {
    [
        ((argb >> 16) & 0xFF) as u8,
        ((argb >> 8) & 0xFF) as u8,
        (argb & 0xFF) as u8,
        ((argb >> 24) & 0xFF) as u8,
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn black_to_transparent_only_touches_palette_formats() {
        let mut palette = vec![0, 0, 0, 255, 1, 2, 3, 255, 0, 0, 0, 128];
        super::black_to_transparent(super::format::PALETTE8, &mut palette);
        assert_eq!(
            palette,
            vec![0, 0, 0, 0, 1, 2, 3, 255, 0, 0, 0, 0],
            "调色板图里的纯黑必须变透明，非黑像素不动（含原有 alpha）"
        );

        let mut rgb565 = vec![0, 0, 0, 255, 1, 2, 3, 255];
        super::black_to_transparent(super::format::RGB565, &mut rgb565);
        assert_eq!(
            rgb565,
            vec![0, 0, 0, 255, 1, 2, 3, 255],
            "rgb565 的 alpha 不许被改"
        );
    }

    use super::*;

    #[test]
    #[cfg(feature = "legacy")]
    fn row_bytes_matches_crystal_width_bytes() {
        // WidthBytes(8, 82) = (((82*8)+31)>>5)*4 = 84
        assert_eq!(row_bytes(8, 82), 84);
        // WidthBytes(16, 7) = (((7*16)+31)>>5)*4 = 16
        assert_eq!(row_bytes(16, 7), 16);
        // 已经是 4 字节整数倍时没有补齐
        assert_eq!(row_bytes(8, 48), 48);
        assert_eq!(row_bytes(16, 48), 96);
        assert_eq!(row_bytes(24, 400), 1200);
    }

    #[test]
    #[cfg(feature = "legacy")]
    fn rgb565_expansion_matches_crystal() {
        assert_eq!(expand_rgb565(0x0000), [0, 0, 0, 255]);
        assert_eq!(expand_rgb565(0xF800), [248, 0, 0, 255]);
        assert_eq!(expand_rgb565(0x07E0), [0, 252, 0, 255]);
        assert_eq!(expand_rgb565(0x001F), [0, 0, 248, 255]);
        assert_eq!(expand_rgb565(0xFFFF), [248, 252, 248, 255]);
    }

    #[test]
    #[cfg(feature = "legacy")]
    fn palette_zero_is_opaque_black() {
        // WeMadeLibrary.cs:77 的第一项是 -16777216 = 0xFF000000
        assert_eq!(palette_rgba(0), [0, 0, 0, 255]);
        assert_eq!(crate::palette::WEMADE_PALETTE.len(), 256);
        assert!(
            crate::palette::WEMADE_PALETTE
                .iter()
                .all(|argb| argb >> 24 == 0xFF),
            "语料用的 WeMade 调色板所有项 alpha 都是 255"
        );
    }

    #[test]
    fn format_names_are_stable() {
        let mut info = ImageInfo {
            width: 1,
            height: 1,
            offset_x: 0,
            offset_y: 0,
            format: format::RGB565,
            shadow_x: 0,
            shadow_y: 0,
            reserved: [0; 3],
            has_mask: false,
            data_offset: 0,
            data_len: 0,
        };
        assert_eq!(info.format_name(), "rgb565");
        info.format = format::EMPTY;
        assert_eq!(info.format_name(), "palette8");
        info.format = format::PALETTE8;
        assert_eq!(info.format_name(), "palette8");
        info.format = format::RGB24;
        assert_eq!(info.format_name(), "rgb24");
        info.format = format::RGBA32_GZIP;
        assert_eq!(info.format_name(), "rgba32-gzip");
        info.format = format::DXT1;
        assert_eq!(info.format_name(), "dxt1");
        info.format = 42;
        assert_eq!(info.format_name(), "unknown");
        assert_eq!(info.width, 1);
        assert!(!info.is_empty());
    }
}
