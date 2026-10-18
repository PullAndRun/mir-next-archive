//! 字体可移植性自检：无 CJK 字形时，「纯中文文案像素差」断言必须优雅跳过。
//!
//! CI（Ubuntu）装的是 `fonts-noto-cjk`，正常有中文字形；这里额外用系统里
//! **没有中文字形**的拉丁字体（DejaVuSans / Arial / LiberationSans）模拟
//! 「早期 CI 只有 DejaVuSans」的场景，确认：
//!
//! 1. [`common::cjk_available`] 能识别出字体缺字形；
//! 2. [`common::skip_if_no_cjk`] 返回 `true`（调用方跳过断言）；
//! 3. 两段纯中文文案在缺字形字体下渲染像素完全相同（说明跳过是必要的）。
//!
//! 整个测试套件也可以用 `MIR2_TEST_FONT=<无 CJK 字体>` 跑一遍（见 tests/common/mod.rs）。

mod common;

use std::path::PathBuf;

use mir_client::ui::UiText;
use mir_render::Frame;

/// 找一个系统里没有中文字形的拉丁字体。
fn non_cjk_font() -> Option<PathBuf> {
    let candidates = [
        // Windows
        "C:/Windows/Fonts/arial.ttf",
        "C:/Windows/Fonts/segoeui.ttf",
        // Debian/Ubuntu
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        // macOS
        "/System/Library/Fonts/Supplemental/Arial.ttf",
    ];
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

/// 把一段文字画到小帧上，返回像素（用于比较两个文案）。
fn render_text(text: &UiText, content: &str) -> Frame {
    let mut frame = Frame::new(160, 32);
    text.draw(&mut frame, 0, 0, content, 16, [255, 255, 255, 255]);
    frame
}

#[test]
fn missing_cjk_glyphs_are_detected_and_skipped() {
    let system = common::load_text();
    // 系统字体有 CJK 时（Noto CJK / 微软雅黑）走强断言分支；没有时走跳过分支。
    println!(
        "系统字体 {:?}，CJK 字形可用 = {}",
        system.font_path(),
        common::cjk_available(&system)
    );

    let Some(path) = non_cjk_font() else {
        eprintln!("跳过：找不到无 CJK 字形的拉丁字体");
        return;
    };
    let font = UiText::load(Some(&path));
    if common::cjk_available(&font) {
        // 少数发行版会给 Arial/DejaVu 链接 CJK 回退；此时本机无法模拟缺字形场景。
        eprintln!(
            "注意：{} 意外含中文字形，无法在本机模拟缺字形场景",
            path.display()
        );
        return;
    }
    assert!(
        common::skip_if_no_cjk(&font, "字体可移植性自检"),
        "缺 CJK 字形时应当要求跳过像素断言"
    );
    // 两段纯中文文案在缺字形字体下都画不出来 → 像素完全相同（这正是旧 CI 的失败原因）。
    let ready = render_text(&font, "对方：已准备");
    let not_ready = render_text(&font, "对方：未准备");
    assert_eq!(
        ready.pixels, not_ready.pixels,
        "缺中文字形时两段中文应当渲染成同样的空帧"
    );
    // 同一字体下的 ASCII 文案仍然能画出来（跳过只影响中文断言，不影响其它测试）。
    let ascii = render_text(&font, "Ready");
    assert!(
        ascii.pixels.chunks_exact(4).any(|pixel| pixel[3] != 0),
        "拉丁字体应当能渲染 ASCII"
    );
}
