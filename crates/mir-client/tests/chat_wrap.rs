//! t110：聊天按显示宽度折行/避头尾 + 截图。

use std::path::Path;

use mir_client::net::{ChatLine, WorldSnapshot};
use mir_client::ui::{self, UiState, hud};
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::ChatType;
use mir_render::{Frame, MapScene};

fn scene() -> MapScene {
    let map = mir_world::Map {
        width: 32,
        height: 32,
        cells: vec![mir_world::MapCell::default(); 32 * 32],
        file_name: "chat-test".to_owned(),
        version: mir_world::MapVersion::V0,
        doors: Vec::new(),
    };
    MapScene::from_parts(
        map,
        mir_render::TileLibraries::empty("."),
        mir_render::Viewport::DEFAULT,
    )
}

#[test]
fn chat_wraps_long_mixed_text_and_saves_png() {
    let mut snapshot = WorldSnapshot::default();
    for (kind, text) in [
        (
            ChatType::Normal,
            "这是一条很长的中文聊天消息，用来验证按显示宽度折行，并且标点不能出现在行首。这是一条很长的中文聊天消息，用来验证按显示宽度折行。",
        ),
        (
            ChatType::Group,
            "Party message with a very long English sentence to test width wrapping 1234567890",
        ),
        (
            ChatType::Guild,
            "行会消息：今晚八点攻城，请各位准时上线，带好药水与随机卷轴！",
        ),
        (ChatType::System, "系统提示：经验值 +120，金币 +35。"),
    ] {
        snapshot.chat.push(ChatLine {
            speaker: None,
            text: text.to_owned(),
            kind,
            system: None,
        });
    }
    let text = ui::UiText::load(None);
    let mut state = UiState::new(Language::ZhCn);
    let mut scene = scene();
    let mut frame = Frame::new(800, 600);
    ui::draw(
        &mut state,
        &mut frame,
        &mut scene,
        &snapshot,
        &text,
        None,
        Point::new(16, 16),
    );

    // 折行后每条消息至少 1 行，长消息必须 >1 行。
    let panel = hud::chat_layout(mir_client::geom::Rect::screen(800, 600));
    let area = hud::chat_lines_area(panel);
    let wrapped = ui::typography::wrap_by_width(
        "这是一条很长的中文聊天消息，用来验证按显示宽度折行，并且标点不能出现在行首。这是一条很长的中文聊天消息，用来验证按显示宽度折行。",
        area.width,
        3,
        |probe| text.text_width(probe, 14),
    );
    assert!(wrapped.len() > 1, "{wrapped:?}");
    for line in &wrapped {
        assert!(!line.starts_with('，'), "标点出现在行首：{line}");
        assert!(!line.starts_with('。'), "标点出现在行首：{line}");
    }

    let output = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target/client-chat-wrap.png");
    std::fs::create_dir_all(output.parent().expect("target")).expect("target");
    frame.save_png(&output).expect("写聊天折行截图");
    assert!(std::fs::metadata(&output).is_ok_and(|meta| meta.len() > 4_096));
}
