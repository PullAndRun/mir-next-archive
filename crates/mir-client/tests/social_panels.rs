//! t91：组队面板 / 行会成员页。
//!
//! - 组队：320×240 下按钮两行、成员行不压按钮；中文/英文重绘；
//! - 行会成员：名字/等级/职位/在线，刷新按钮发 `C.RequestGuildInfo{type=1}`；
//! - 截图 `target/client-group.png`、`target/client-guild-members.png`。

use std::path::Path;

use mir_client::geom::Rect;
use mir_client::net::{GuildMemberView, GuildRankView, GuildView, SelfPlayer, WorldSnapshot};
use mir_client::ui::{self, UiAction, UiState, group, guild};
use mir_core::Language;
use mir_core::Point;
use mir_render::{Frame, MapScene};

fn workspace(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn scene() -> MapScene {
    let map = mir_world::Map {
        width: 32,
        height: 32,
        cells: vec![mir_world::MapCell::default(); 32 * 32],
        file_name: "social-test".to_owned(),
        version: mir_world::MapVersion::V0,
        doors: Vec::new(),
    };
    MapScene::from_parts(
        map,
        mir_render::TileLibraries::empty("."),
        mir_render::Viewport::DEFAULT,
    )
}

#[allow(clippy::field_reassign_with_default)]
fn group_snapshot() -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    snapshot.self_player = Some(SelfPlayer {
        object_id: 1,
        real_id: 1,
        name: "队长".to_owned(),
        position: Point::new(10, 10),
        direction: mir_core::enums::MirDirection::Down,
        hp: 100,
        mp: 50,
        hp_max: 100,
        mp_max: 50,
        level: 42,
        class: mir_core::enums::MirClass::Warrior,
        gender: mir_core::enums::MirGender::Male,
        hair: 0,
        gold: 0,
        credit: 0,
        dead: false,
        action: mir_render::ActorAction::Standing,
        action_started_ms: 0,
        attack_variant: None,
        native_action: None,
        looks: mir_client::net::PlayerLooks::default(),
    });
    for (index, name) in ["队长", "道士甲", "法师乙"].iter().enumerate() {
        snapshot.group.members.push(mir_client::net::GroupMember {
            name: (*name).to_owned(),
            map: format!("比奇省{index}"),
            location: Some(Point::new(10 + i32::try_from(index).unwrap_or(0), 20)),
        });
    }
    snapshot
}

#[allow(clippy::field_reassign_with_default)]
fn guild_snapshot() -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    snapshot.guild = Some(GuildView {
        name: "测试行会".to_owned(),
        level: 3,
        member_count: 3,
        max_members: 20,
        ..GuildView::default()
    });
    snapshot.self_player = Some(SelfPlayer {
        object_id: 1,
        real_id: 1,
        name: "队长".to_owned(),
        position: Point::new(10, 10),
        direction: mir_core::enums::MirDirection::Down,
        hp: 100,
        mp: 50,
        hp_max: 100,
        mp_max: 50,
        level: 42,
        class: mir_core::enums::MirClass::Warrior,
        gender: mir_core::enums::MirGender::Male,
        hair: 0,
        gold: 0,
        credit: 0,
        dead: false,
        action: mir_render::ActorAction::Standing,
        action_started_ms: 0,
        attack_variant: None,
        native_action: None,
        looks: mir_client::net::PlayerLooks::default(),
    });
    snapshot.guild_ranks = vec![
        GuildRankView {
            index: 0,
            name: "会长".to_owned(),
            options: 0,
            members: vec![GuildMemberView {
                name: "会长甲".to_owned(),
                id: 1,
                online: true,
                ..GuildMemberView::default()
            }],
        },
        GuildRankView {
            index: 1,
            name: "长老".to_owned(),
            options: 0,
            members: vec![
                GuildMemberView {
                    name: "队长".to_owned(),
                    id: 2,
                    online: true,
                    ..GuildMemberView::default()
                },
                GuildMemberView {
                    name: "离线乙".to_owned(),
                    id: 3,
                    online: false,
                    ..GuildMemberView::default()
                },
            ],
        },
    ];
    snapshot
}

fn render(state: &mut UiState, snapshot: &WorldSnapshot, size: (u32, u32)) -> Frame {
    let text = ui::UiText::load(None);
    let mut scene = scene();
    let mut frame = Frame::new(size.0, size.1);
    ui::draw(
        state,
        &mut frame,
        &mut scene,
        snapshot,
        &text,
        None,
        Point::new(4, 4),
    );
    frame
}

#[test]
fn group_panel_fits_320x240_and_redraws_in_english() {
    let snapshot = group_snapshot();
    let small = Rect::screen(320, 240);
    let layout = group::group_layout(small, 3, 0);
    assert!(layout.panel.right() <= 320 && layout.panel.bottom() <= 240);
    for rect in [
        layout.invite_field,
        layout.invite,
        layout.leave,
        layout.kick,
        layout.switch,
        layout.footer,
    ] {
        assert!(
            layout.panel.contains(rect.center()),
            "组队控件出面板：{rect:?} / {:?}",
            layout.panel
        );
    }
    assert!(layout.invite.intersect(layout.leave).is_none());
    assert!(layout.leave.intersect(layout.kick).is_none());
    for row in &layout.rows {
        assert!(
            row.bottom() <= layout.invite_field.y - 2,
            "成员行压到按钮：{row:?}"
        );
    }

    let mut state = UiState::new(Language::ZhCn);
    state.show_group = true;
    let zh = render(&mut state, &snapshot, (320, 240));
    let output = workspace("target/client-group.png");
    std::fs::create_dir_all(output.parent().expect("target")).expect("target");
    zh.save_png(&output).expect("写组队截图");

    state.language = Language::En;
    let en = render(&mut state, &snapshot, (320, 240));
    let changed = zh
        .pixels
        .chunks_exact(4)
        .zip(en.pixels.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(changed > 50, "中英切换应重绘：{changed}");
}

#[test]
fn guild_member_page_shows_levels_and_refresh_request() {
    let snapshot = guild_snapshot();
    assert_eq!(snapshot.player_level_by_name("队长"), Some(42));
    assert_eq!(snapshot.player_level_by_name("离线乙"), None);

    let screen = Rect::screen(800, 600);
    let layout = guild::guild_layout(screen, snapshot.guild_members().len(), 0, 0, 0);
    let columns = guild::guild_member_columns(layout.member_header.width);
    assert!(columns[3] < i32::try_from(layout.member_header.width).unwrap_or(i32::MAX));
    for row in &layout.member_rows {
        assert!(row.right() <= layout.panel.right());
    }
    assert!(layout.refresh.right() <= layout.panel.right());
    assert!(layout.kick.right() <= layout.panel.right());

    let mut state = UiState::new(Language::ZhCn);
    state.show_guild = true;
    state.guild_tab = guild::GuildTab::Members;
    let zh = render(&mut state, &snapshot, (800, 600));
    let output = workspace("target/client-guild-members.png");
    zh.save_png(&output).expect("写行会成员截图");
    assert!(std::fs::metadata(&output).is_ok_and(|meta| meta.len() > 4_096));

    // 点刷新 → 请求成员表。
    state.mouse = layout.refresh.center();
    let action = state.on_click(&snapshot, &ui::UiText::load(None), screen, None);
    assert!(matches!(
        action,
        Some(UiAction::RequestGuildInfo { type_: 1 })
    ));

    state.language = Language::En;
    let en = render(&mut state, &snapshot, (800, 600));
    let changed = zh
        .pixels
        .chunks_exact(4)
        .zip(en.pixels.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(changed > 50, "行会成员页中英切换应重绘：{changed}");
}
