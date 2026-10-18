#![allow(dead_code)]

//! 客户端集成测试的共享辅助（字体可移植性）。
//!
//! CI 的 Ubuntu runner 只装 `DejaVuSans` / `Noto CJK` 两种；本地 Windows 有微软雅黑。
//! **中文字形不是所有环境都有**：像 `已准备` / `未准备` 这种「只有文案不同」的像素断言，
//! 在缺 CJK 字形时会被渲染成同样的空/方块，恒为 0 像素差。
//!
//! 这里提供：
//! * [`load_text`]：默认走系统自动发现；设置 `MIR2_TEST_FONT` 可强制指定字体
//!   （用于模拟「只有 DejaVuSans/没有中文字形」的场景，例如
//!   `MIR2_TEST_FONT=C:\Windows\Fonts\arial.ttf cargo test -p mir-client`）；
//! * [`cjk_available`]：真实渲染一个中文字符，判断字体里有没有它的字形；
//! * [`skip_if_no_cjk`]：缺字形时打印原因并让调用方跳过「中文文案像素差」断言。

use std::path::Path;

use mir_client::ui::UiText;
use mir_render::Frame;

/// 测试用字体：`MIR2_TEST_FONT` 优先，否则按 `mir-render` 的系统字体发现顺序。
#[must_use]
pub fn load_text() -> UiText {
    match std::env::var_os("MIR2_TEST_FONT") {
        Some(path) => {
            let path = std::path::PathBuf::from(path);
            println!("测试字体（MIR2_TEST_FONT）：{}", path.display());
            UiText::load(Some(Path::new(&path)))
        }
        None => UiText::load(None),
    }
}

/// 当前字体是否真的有中文字形。
///
/// 做法：把「已准备」画到一张小帧上。`ab_glyph` 对字体里没有的字符返回 `.notdef`
/// （字形 0，没有轮廓），栅格化后是空图；只有真正有 CJK glyph 的字体才会产生像素。
/// 点阵兜底（`font_path()` 为 `None`）没有中文，直接算不可用。
#[must_use]
pub fn cjk_available(text: &UiText) -> bool {
    if text.font_path().is_none() {
        return false;
    }
    let mut frame = Frame::new(96, 32);
    text.draw(&mut frame, 0, 0, "已准备", 16, [255, 255, 255, 255]);
    frame.pixels.chunks_exact(4).any(|pixel| pixel[3] != 0)
}

/// 缺 CJK 字形时返回 `true`：调用方跳过「中文文案像素差」断言并打印原因。
///
/// 正常 Linux（Noto CJK）与 Windows（微软雅黑）返回 `false`，断言保持原有强度。
#[must_use]
pub fn skip_if_no_cjk(text: &UiText, what: &str) -> bool {
    if cjk_available(text) {
        return false;
    }
    eprintln!(
        "跳过「{what}」像素断言：字体 {} 没有中文字形（如 DejaVuSans/Arial）",
        text.font_path().map_or_else(
            || "<点阵兜底>".to_owned(),
            |path| path.display().to_string()
        )
    );
    true
}
