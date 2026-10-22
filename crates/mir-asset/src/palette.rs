//! `WeMade` 8 位调色板。
//!
//! `.wzl` 的 8 位图像**不自带调色板**：Crystal 的 `WeMadeLibrary` 直接使用源码里硬编码的
//! 256 色表（`LibraryEditor/Graphics/WeMadeLibrary.cs:77`）。只有旧式 `.wil` 才会从文件头读
//! 调色板（`WeMadeLibrary.cs:97-107`），`.wzl`（`_nType == 1`，`WeMadeLibrary.cs:43-46`）
//! 走不到那段，所以索引一律映射到这张表。
//!
//! 每项是 `0xAARRGGBB`（C# 的 `int` 被直接写进 32bpp 位图，`WeMadeLibrary.cs:467`）。
//! 256 项 alpha 全是 `0xFF`：**这张表没有透明色**，`palette[0]` 是不透明的黑。
//! `.Lib` 里的透明像素来自写库时的 removeBlack（`LibraryEditor/Graphics/MLibraryV2.cs:419-426`），
//! 与调色板无关。
//!
//! 调色板前 16 项是 Windows 经典 16 色（黑、暗红、暗绿……），其余是 `WeMade` 美术用的渐变表；
//! 后 8 项是纯色（红/绿/黄/蓝/品红/青/白），说明这是一张从旧版调色板扩出来的表。

/// `WeMade` 256 色调色板，`0xAARRGGBB`。
pub(crate) const WEMADE_PALETTE: [u32; 256] = [
    0xFF00_0000,
    0xFF80_0000,
    0xFF00_8000,
    0xFF80_8000,
    0xFF00_0080,
    0xFF80_0080,
    0xFF00_8080,
    0xFFC0_C0C0,
    0xFF55_8097,
    0xFF9D_B9C8,
    0xFF7B_7373,
    0xFF2D_2929,
    0xFF5A_5252,
    0xFF63_5A5A,
    0xFF42_3939,
    0xFF1D_1818,
    0xFF18_1010,
    0xFF29_1818,
    0xFF10_0808,
    0xFFF2_7971,
    0xFFE1_675F,
    0xFFFF_5A5A,
    0xFFFF_3131,
    0xFFD6_5A52,
    0xFF94_1000,
    0xFF94_2918,
    0xFF39_0800,
    0xFF73_1000,
    0xFFB5_1800,
    0xFFBD_6352,
    0xFF42_1810,
    0xFFFF_AA99,
    0xFF5A_1000,
    0xFF73_3929,
    0xFFA5_4A31,
    0xFF94_7B73,
    0xFFBD_5231,
    0xFF52_2110,
    0xFF7B_3118,
    0xFF2D_1810,
    0xFF8C_4A31,
    0xFF94_2900,
    0xFFBD_3100,
    0xFFC6_7352,
    0xFF6B_3118,
    0xFFC6_6B42,
    0xFFCE_4A00,
    0xFFA5_6339,
    0xFF5A_3118,
    0xFF2A_1000,
    0xFF15_0800,
    0xFF3A_1800,
    0xFF08_0000,
    0xFF29_0000,
    0xFF4A_0000,
    0xFF9D_0000,
    0xFFDC_0000,
    0xFFDE_0000,
    0xFFFB_0000,
    0xFF9C_7352,
    0xFF94_6B4A,
    0xFF73_4A29,
    0xFF52_3118,
    0xFF8C_4A18,
    0xFF88_4411,
    0xFF4A_2100,
    0xFF21_1810,
    0xFFD6_945A,
    0xFFC6_6B21,
    0xFFEF_6B00,
    0xFFFF_7700,
    0xFFA5_9484,
    0xFF42_3121,
    0xFF18_1008,
    0xFF29_1808,
    0xFF21_1000,
    0xFF39_2918,
    0xFF8C_6339,
    0xFF42_2910,
    0xFF6B_4218,
    0xFF7B_4A18,
    0xFF94_4A00,
    0xFF8C_847B,
    0xFF6B_635A,
    0xFF4A_4239,
    0xFF29_2118,
    0xFF46_3929,
    0xFFB5_A594,
    0xFF7B_6B5A,
    0xFFCE_B194,
    0xFFA5_8C73,
    0xFF8C_735A,
    0xFFB5_9473,
    0xFFD6_A573,
    0xFFEF_A54A,
    0xFFEF_C68C,
    0xFF7B_6342,
    0xFF6B_5639,
    0xFFBD_945A,
    0xFF63_3900,
    0xFFD6_C6AD,
    0xFF52_4229,
    0xFF94_6318,
    0xFFEF_D6AD,
    0xFFA5_8C63,
    0xFF63_5A4A,
    0xFFBD_A57B,
    0xFF5A_4218,
    0xFFBD_8C31,
    0xFF35_3129,
    0xFF94_8463,
    0xFF7B_6B4A,
    0xFFA5_8C5A,
    0xFF5A_4A29,
    0xFF9C_7B39,
    0xFF42_3110,
    0xFFEF_AD21,
    0xFF18_1000,
    0xFF29_2100,
    0xFF9C_6B00,
    0xFF94_845A,
    0xFF52_4218,
    0xFF6B_5A29,
    0xFF7B_6321,
    0xFF9C_7B21,
    0xFFDE_A500,
    0xFF5A_5239,
    0xFF31_2910,
    0xFFCE_BD7B,
    0xFF63_5A39,
    0xFF94_844A,
    0xFFC6_A529,
    0xFF10_9C18,
    0xFF42_8C4A,
    0xFF31_8C42,
    0xFF10_9429,
    0xFF08_1810,
    0xFF08_1818,
    0xFF08_2910,
    0xFF18_4229,
    0xFFA5_B5AD,
    0xFF6B_7373,
    0xFF18_2929,
    0xFF18_424A,
    0xFF31_424A,
    0xFF63_C6DE,
    0xFF44_DDFF,
    0xFF8C_D6EF,
    0xFF73_6B39,
    0xFFF7_DE39,
    0xFFF7_EF8C,
    0xFFF7_E700,
    0xFF6B_6B5A,
    0xFF5A_8CA5,
    0xFF39_B5EF,
    0xFF4A_9CCE,
    0xFF31_84B5,
    0xFF31_526B,
    0xFFDE_DED6,
    0xFFBD_BDB5,
    0xFF8C_8C84,
    0xFFF7_F7DE,
    0xFF00_0818,
    0xFF08_1839,
    0xFF08_1029,
    0xFF08_1800,
    0xFF08_2900,
    0xFF00_52A5,
    0xFF00_7BDE,
    0xFF10_294A,
    0xFF10_396B,
    0xFF10_528C,
    0xFF21_5AA5,
    0xFF10_315A,
    0xFF10_4284,
    0xFF31_5284,
    0xFF18_2131,
    0xFF4A_5A7B,
    0xFF52_6BA5,
    0xFF29_3963,
    0xFF10_4ADE,
    0xFF29_2921,
    0xFF4A_4A39,
    0xFF29_2918,
    0xFF4A_4A29,
    0xFF7B_7B42,
    0xFF9C_9C4A,
    0xFF5A_5A29,
    0xFF42_4214,
    0xFF39_3900,
    0xFF59_5900,
    0xFFCA_352C,
    0xFF6B_7321,
    0xFF29_3100,
    0xFF31_3910,
    0xFF31_3918,
    0xFF42_4A00,
    0xFF52_6318,
    0xFF5A_7329,
    0xFF31_4A18,
    0xFF18_2100,
    0xFF18_3100,
    0xFF18_3910,
    0xFF63_844A,
    0xFF6B_BD4A,
    0xFF63_B54A,
    0xFF63_BD4A,
    0xFF5A_9C4A,
    0xFF4A_8C39,
    0xFF63_C64A,
    0xFF63_D64A,
    0xFF52_844A,
    0xFF31_7329,
    0xFF63_C65A,
    0xFF52_BD4A,
    0xFF10_FF00,
    0xFF18_2918,
    0xFF4A_884A,
    0xFF4A_E74A,
    0xFF00_5A00,
    0xFF00_8800,
    0xFF00_9400,
    0xFF00_DE00,
    0xFF00_EE00,
    0xFF00_FB00,
    0xFF4A_5A94,
    0xFF63_73B5,
    0xFF7B_8CD6,
    0xFF6B_7BD6,
    0xFF77_88FF,
    0xFFC6_C6CE,
    0xFF94_949C,
    0xFF9C_94C6,
    0xFF31_3139,
    0xFF29_1884,
    0xFF18_0084,
    0xFF4A_4252,
    0xFF52_427B,
    0xFF63_5A73,
    0xFFCE_B5F7,
    0xFF8C_7B9C,
    0xFF77_22CC,
    0xFFDD_AAFF,
    0xFFF0_B42A,
    0xFFDF_009F,
    0xFFE3_17B3,
    0xFFFF_FBF0,
    0xFFA0_A0A4,
    0xFF80_8080,
    0xFFFF_0000,
    0xFF00_FF00,
    0xFFFF_FF00,
    0xFF00_00FF,
    0xFFFF_00FF,
    0xFF00_FFFF,
    0xFFFF_FFFF,
];

/// 调色板的 RGB 字节数（256 × 3，不含 alpha）。
pub const PALETTE_RGB_LEN: usize = 768;

/// `WeMade` 调色板导出成 768 字节（`R,G,B` 顺序），用于 `.mnp` 的 `kind=3` 调色板条目。
#[must_use]
pub fn wemade_palette_rgb() -> [u8; PALETTE_RGB_LEN] {
    let mut out = [0u8; PALETTE_RGB_LEN];
    for (index, value) in WEMADE_PALETTE.iter().enumerate() {
        out[index * 3] = u8::try_from((value >> 16) & 0xFF).unwrap_or(0);
        out[index * 3 + 1] = u8::try_from((value >> 8) & 0xFF).unwrap_or(0);
        out[index * 3 + 2] = u8::try_from(value & 0xFF).unwrap_or(0);
    }
    out
}

/// RGBA 像素反查成 8 位调色板索引；任何像素不在表里返回 `None`（调用方回退 zlib RGBA）。
///
/// 纯黑（`0,0,0`，含去黑后的 alpha=0）一律映射到 0 号色——与
/// `black_to_transparent` 的约定一致，展开时 0 号色再置 alpha=0。
#[must_use]
pub fn palette8_indices(pixels: &[u8]) -> Option<Vec<u8>> {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static REVERSE: OnceLock<HashMap<u32, u8>> = OnceLock::new();
    let reverse = REVERSE.get_or_init(|| {
        let mut map = HashMap::with_capacity(256);
        for (index, value) in WEMADE_PALETTE.iter().enumerate() {
            let rgb = value & 0x00FF_FFFF;
            map.entry(rgb).or_insert(u8::try_from(index).unwrap_or(0));
        }
        map
    });
    if pixels.len() % 4 != 0 {
        return None;
    }
    let mut indices = Vec::with_capacity(pixels.len() / 4);
    for pixel in pixels.chunks_exact(4) {
        let key = (u32::from(pixel[0]) << 16) | (u32::from(pixel[1]) << 8) | u32::from(pixel[2]);
        if key == 0 {
            indices.push(0);
            continue;
        }
        indices.push(*reverse.get(&key)?);
    }
    Some(indices)
}

/// 索引展开成 RGBA；0 号色 alpha=0（等价 `remove_black`），其余 alpha=255。
#[must_use]
pub fn palette8_to_rgba(indices: &[u8], palette: &[u8; PALETTE_RGB_LEN]) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(indices.len() * 4);
    for &index in indices {
        let offset = usize::from(index) * 3;
        let alpha = if index == 0 { 0 } else { 255 };
        pixels.extend_from_slice(&[
            palette[offset],
            palette[offset + 1],
            palette[offset + 2],
            alpha,
        ]);
    }
    pixels
}

#[cfg(test)]
mod palette8_tests {
    use super::*;

    #[test]
    fn wemade_palette_round_trips_through_indices() {
        let palette = wemade_palette_rgb();
        let indices: Vec<u8> = (0..=255u8).collect();
        let rgba = palette8_to_rgba(&indices, &palette);
        let back = palette8_indices(&rgba).expect("必须能反查");
        assert_eq!(back, indices, "256 色索引 → RGBA → 索引 必须完全一致");
        assert_eq!(rgba[3], 0, "0 号色展开后 alpha=0");
        assert_eq!(rgba[7], 255, "1 号色 alpha=255");
    }

    #[test]
    fn unmapped_colour_falls_back_to_none() {
        let pixels = vec![1, 2, 3, 255];
        assert!(
            palette8_indices(&pixels).is_none(),
            "调色板外颜色必须返回 None"
        );
    }
}
