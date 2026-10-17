//! DXT1（BC1）解码。
//!
//! 旧 `.Lib` 有两种像素存储：
//!
//! * **gzip RGBA32**：Crystal 客户端与 `LibraryEditor` 的 `MLibraryV2`（`MLibraryV2.cs:510-546`）。
//! * **DXT1**：`LibraryEditor` 的 `MLibraryV1` 用 `ManagedSquish` 压/解（`MLibraryV1.cs:433-442`、
//!   `MLibraryV1.cs:466-495`）。
//!
//! 语料里 `ArtificialMir2` 客户端的那批 `.Lib`（1350 个文件）存的是 DXT1：每条记录的数据长度
//! 正好是 `⌈宽/4⌉ × ⌈高/4⌉ × 8`，而 Crystal 布局的 `.Lib` 是 gzip（以 `1f 8b` 开头）。
//!
//! # 与 Squish 的一致性
//!
//! * 颜色展开用**位复制**（`r << 3 | r >> 2`），与 libsquish 的 `DecompressColour` 一致。
//! * `color0 > color1` 时是四色模式，否则第四色是全透明黑（三色 + 透明模式）；
//!   中间两色按 `(2c0+c1)/3`、`(c0+2c1)/3` 整数除法混合。
//! * 输出是**自上而下**的 RGBA：Squish 把 DXT1 解到 32bpp 位图，本身没有翻转
//!   （`MLibraryV1.cs:466-495` 只做 R/B 交换）。
//! * 压缩前 `MLibraryV1` 会把图补到 4 的整数倍（`MLibraryV1.cs:388-408`），
//!   所以块网格按向上取整计算，解码后裁掉右侧/下侧的补齐像素。

/// DXT1 数据的字节数：`⌈宽/4⌉ × ⌈高/4⌉ × 8`（`Squish.GetStorageRequirements`，
/// `MLibraryV1.cs:433`）。
pub(crate) fn storage_size(width: u16, height: u16) -> u64 {
    let blocks_x = u64::from(width).div_ceil(4).max(1);
    let blocks_y = u64::from(height).div_ceil(4).max(1);
    blocks_x * blocks_y * 8
}

/// 解码 DXT1 为自上而下的 RGBA8。
///
/// `data` 必须至少有 [`storage_size`] 字节。
pub(crate) fn decode(data: &[u8], width: u16, height: u16) -> Vec<u8> {
    let width = usize::from(width);
    let height = usize::from(height);
    let blocks_x = width.div_ceil(4);
    let mut pixels = vec![0u8; width * height * 4];
    for block_y in 0..height.div_ceil(4) {
        for block_x in 0..blocks_x {
            let start = (block_y * blocks_x + block_x) * 8;
            let Some(block) = data.get(start..start + 8) else {
                return pixels;
            };
            let colours = decode_colours(block);
            let indices = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
            for row in 0..4usize {
                let y = block_y * 4 + row;
                if y >= height {
                    break;
                }
                for column in 0..4usize {
                    let x = block_x * 4 + column;
                    if x >= width {
                        continue;
                    }
                    let selector =
                        usize::try_from((indices >> (2 * (row * 4 + column))) & 0b11).unwrap_or(3);
                    let offset = (y * width + x) * 4;
                    pixels[offset..offset + 4].copy_from_slice(&colours[selector]);
                }
            }
        }
    }
    pixels
}

/// 把 8 字节块的两个颜色与两个插值色展开成 4 个 RGBA。
fn decode_colours(block: &[u8]) -> [[u8; 4]; 4] {
    let colour0 = u16::from_le_bytes([block[0], block[1]]);
    let colour1 = u16::from_le_bytes([block[2], block[3]]);
    let first = expand565(colour0);
    let second = expand565(colour1);
    let mut colours = [[0u8; 4]; 4];
    colours[0] = first;
    colours[1] = second;
    if colour0 > colour1 {
        colours[2] = [
            mix(first[0], second[0], 2),
            mix(first[1], second[1], 2),
            mix(first[2], second[2], 2),
            0xFF,
        ];
        colours[3] = [
            mix(first[0], second[0], 1),
            mix(first[1], second[1], 1),
            mix(first[2], second[2], 1),
            0xFF,
        ];
    } else {
        colours[2] = [
            average(first[0], second[0]),
            average(first[1], second[1]),
            average(first[2], second[2]),
            0xFF,
        ];
        // 三色模式：选择器 3 表示透明黑。
    }
    colours
}

/// RGB565 展开成 8 位，按 libsquish 的位复制方式。
fn expand565(value: u16) -> [u8; 4] {
    let red = ((value >> 11) & 0x1F) as u8;
    let green = ((value >> 5) & 0x3F) as u8;
    let blue = (value & 0x1F) as u8;
    [
        (red << 3) | (red >> 2),
        (green << 2) | (green >> 4),
        (blue << 3) | (blue >> 2),
        0xFF,
    ]
}

/// `(first × weight + second × (3 - weight)) / 3`，与 libsquish 的插值一致。
fn mix(first: u8, second: u8, weight: u16) -> u8 {
    let first = u16::from(first);
    let second = u16::from(second);
    ((first * weight + second * (3 - weight)) / 3) as u8
}

/// 三色模式的中间色：`(first + second) / 2`（`u16::midpoint` 等价且不溢出）。
fn average(first: u8, second: u8) -> u8 {
    u16::midpoint(u16::from(first), u16::from(second)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(colour0: u16, colour1: u16, indices: u32) -> [u8; 8] {
        let mut bytes = [0u8; 8];
        bytes[0..2].copy_from_slice(&colour0.to_le_bytes());
        bytes[2..4].copy_from_slice(&colour1.to_le_bytes());
        bytes[4..8].copy_from_slice(&indices.to_le_bytes());
        bytes
    }

    #[test]
    fn storage_size_matches_squish_get_storage_requirements() {
        assert_eq!(storage_size(20, 16), 160);
        assert_eq!(storage_size(1, 1), 8);
        assert_eq!(storage_size(4, 4), 8);
        assert_eq!(storage_size(5, 3), 16);
        assert_eq!(storage_size(8, 9), 48);
    }

    #[test]
    fn solid_block_expands_with_bit_replication() {
        // color0 = 纯红 0xF800，索引全 0。
        let pixels = decode(&block(0xF800, 0x07E0, 0), 4, 4);
        // 5 位红 0x1F → 0xFF（位复制），绿 0x3F → 0xFF，蓝 0 → 0。
        assert_eq!(&pixels[0..4], &[0xFF, 0, 0, 0xFF]);
        assert_eq!(pixels.len(), 4 * 4 * 4);
    }

    #[test]
    fn four_colour_mode_interpolates_third_and_second() {
        // color0 = 白 0xFFFF > color1 = 黑 0x0000：四色模式。
        // 第 0 行选择器 2，第 1 行选择器 3，第 2/3 行选择器 0。
        let indices = 0b10_10_10_10u32 | (0b11_11_11_11 << 8);
        let pixels = decode(&block(0xFFFF, 0x0000, indices), 4, 4);
        // 选择器 2：(2*255+0)/3 = 170
        assert_eq!(&pixels[0..4], &[170, 170, 170, 255]);
        // 选择器 3：(255+2*0)/3 = 85
        assert_eq!(&pixels[16..20], &[85, 85, 85, 255]);
        // 选择器 0：白
        assert_eq!(&pixels[32..36], &[255, 255, 255, 255]);
    }

    #[test]
    fn three_colour_mode_has_transparent_selector() {
        // color0 = 黑 0x0000 < color1 = 白 0xFFFF：三色 + 透明模式。
        // 第 0 行选择器 0（黑），第 1 行 3（透明），第 2 行 2（中灰），第 3 行 1（白）。
        let indices = (0b11_11_11_11 << 8) | (0b10_10_10_10 << 16) | (0b01_01_01_01 << 24);
        let pixels = decode(&block(0x0000, 0xFFFF, indices), 4, 4);
        assert_eq!(&pixels[0..4], &[0, 0, 0, 255]);
        assert_eq!(&pixels[16..20], &[0, 0, 0, 0]);
        // (0 + 255) / 2 = 127
        assert_eq!(&pixels[32..36], &[127, 127, 127, 255]);
        assert_eq!(&pixels[48..52], &[0xFF, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn crops_padded_pixels() {
        // 5×3：2×1 个块，右侧多出 3 列、下侧多出 1 行应当被裁掉。
        let mut data = Vec::new();
        data.extend_from_slice(&block(0xF800, 0x07E0, 0));
        data.extend_from_slice(&block(0x001F, 0x07E0, 0));
        let pixels = decode(&data, 5, 3);
        assert_eq!(pixels.len(), 5 * 3 * 4);
        // 第 4 列来自第 2 个块，是纯蓝（每像素 4 字节）。
        let offset = 4 * 4;
        assert_eq!(&pixels[offset..offset + 4], &[0, 0, 0xFF, 0xFF]);
        // 第 3 行不存在，输出里没有它。
        assert_eq!(pixels.len(), 60);
    }

    #[test]
    fn short_data_stops_without_panic() {
        let pixels = decode(&[0u8; 3], 8, 8);
        assert_eq!(pixels.len(), 8 * 8 * 4);
        assert!(pixels.iter().all(|&byte| byte == 0));
    }
}
