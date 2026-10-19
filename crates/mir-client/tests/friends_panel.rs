//! 好友/黑名单面板与行会联盟页签（t60）。
//!
//! - 320×240 布局：所有矩形在面板/屏幕内；
//! - `row_at(scroll)` 返回绝对下标；
//! - 加/删/备注/刷新动作 → `NetCommand` → 编码出 127–130 的包；
//! - `S.FriendUpdate`(245) 整表替换并重绘；
//! - 中英切换重绘；截图 `target/client-friends.png` / `client-ally.png`。

use std::path::Path;

use mir_client::geom::Rect;
use mir_client::net::{NetCommand, WorldSnapshot};
use mir_client::ui::{self, UiState, friends, guild};
use mir_core::Language;
use mir_core::Point;
use mir_protocol::data::ClientFriend;
use mir_protocol::packets::client as c;
use mir_protocol::packets::server::{FriendUpdate, GuildAllies, GuildAlly, GuildStatus};
use mir_protocol::{PacketWriter, ServerPacket};
use mir_render::{Frame, MapScene};

fn snapshot_with_friends(count: usize) -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    let friends = (0..count)
        .map(|index| ClientFriend {
            index: i32::try_from(index).unwrap_or(0) + 100,
            name: format!("好友{index}"),
            memo: if index % 2 == 0 {
                format!("备注{index}")
            } else {
                String::new()
            },
            blocked: index == 2,
            online: index % 3 == 0,
        })
        .collect();
    snapshot.apply_packet(&ServerPacket::FriendUpdate(FriendUpdate { friends }));
    snapshot
}

fn scene() -> MapScene {
    let map = mir_world::Map {
        width: 64,
        height: 64,
        cells: vec![mir_world::MapCell::default(); 64 * 64],
        file_name: "friends-test".to_owned(),
        version: mir_world::MapVersion::V0,
        doors: Vec::new(),
    };
    MapScene::from_parts(
        map,
        mir_render::TileLibraries::empty("."),
        mir_render::Viewport::DEFAULT,
    )
}

fn render(
    state: &mut UiState,
    snapshot: &WorldSnapshot,
    text: &ui::UiText,
    width: u32,
    height: u32,
) -> Frame {
    let mut scene = scene();
    let mut frame = Frame::new(width, height);
    ui::draw(
        state,
        &mut frame,
        &mut scene,
        snapshot,
        text,
        None,
        Point::new(10, 10),
    );
    frame
}

fn workspace(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn friends_layout_fits_small_screen_and_row_at_uses_scroll() {
    let layout = friends::friends_layout(Rect::screen(320, 240));
    assert!(
        layout.panel.right() <= 320 && layout.panel.bottom() <= 240,
        "320×240 面板出屏：{:?}",
        layout.panel
    );
    for rect in layout.rows.iter().chain([
        &layout.name_field,
        &layout.memo_field,
        &layout.add,
        &layout.save_memo,
        &layout.remove,
        &layout.refresh,
    ]) {
        assert!(
            layout.panel.contains(rect.center())
                && rect.right() <= layout.panel.right()
                && rect.bottom() <= layout.panel.bottom(),
            "控件出面板：{rect:?} / {:?}",
            layout.panel
        );
    }
    let row = layout.rows[0];
    assert_eq!(
        layout.row_at(row.center(), 7),
        Some(7),
        "滚动 7 行后第一行应返回绝对下标 7"
    );
}

#[test]
fn friend_actions_map_to_packets_127_to_130() {
    let add = ui::UiAction::AddFriend {
        name: "张三".to_owned(),
        blocked: true,
    };
    assert_eq!(
        add.net_command(),
        Some(NetCommand::AddFriend {
            name: "张三".to_owned(),
            blocked: true
        })
    );
    let remove = ui::UiAction::RemoveFriend {
        character_index: 42,
    };
    assert_eq!(
        remove.net_command(),
        Some(NetCommand::RemoveFriend {
            character_index: 42
        })
    );
    assert_eq!(
        ui::UiAction::RefreshFriends.net_command(),
        Some(NetCommand::RefreshFriends)
    );
    let memo = ui::UiAction::AddMemo {
        character_index: 42,
        memo: "老朋友".to_owned(),
    };
    assert_eq!(
        memo.net_command(),
        Some(NetCommand::AddMemo {
            character_index: 42,
            memo: "老朋友".to_owned()
        })
    );

    // 实际编码字节：AddFriend(127) = 名字 + blocked；AddMemo(130) = index + memo。
    let mut writer = PacketWriter::new();
    c::AddFriend {
        name: "张三".to_owned(),
        blocked: true,
    }
    .encode(&mut writer);
    let bytes = writer.into_inner();
    assert!(bytes.ends_with(&[1]), "末尾应当是 blocked=true");
    assert_eq!(c::AddFriend::INDEX, 127);

    let mut writer = PacketWriter::new();
    c::AddMemo {
        character_index: 42,
        memo: "老朋友".to_owned(),
    }
    .encode(&mut writer);
    let bytes = writer.into_inner();
    assert_eq!(&bytes[..4], &42i32.to_le_bytes());
    assert_eq!(c::AddMemo::INDEX, 130);
    assert_eq!(c::RemoveFriend::INDEX, 128);
    assert_eq!(c::RefreshFriends::INDEX, 129);
}

#[test]
fn friends_panel_renders_selects_and_saves_png() {
    let text = ui::UiText::load(None);
    let snapshot = snapshot_with_friends(10);
    let mut state = UiState::new(Language::ZhCn);
    state.toggle_friends();
    let screen = Rect::screen(800, 600);
    let layout = friends::friends_layout(screen);

    let frame = render(&mut state, &snapshot, &text, 800, 600);
    assert!(frame.distinct_color_count() > 10);
    let output = workspace("target/client-friends.png");
    std::fs::create_dir_all(output.parent().expect("target")).expect("创建 target");
    frame.save_png(&output).expect("写好友截图");

    // 点第 2 行（scroll=0）→ 选中 1，并把备注载入输入框。
    state.mouse = layout.rows[1].center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.friends.selected, Some(1));
    assert_eq!(state.friends.memo.value, snapshot.friends[1].memo);

    // 删除选中好友 → C.RemoveFriend(好友 index)。
    state.mouse = layout.remove.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("删除按钮应产生动作");
    assert_eq!(
        action,
        ui::UiAction::RemoveFriend {
            character_index: snapshot.friends[1].index
        }
    );

    // 备注 → C.AddMemo。
    state.friends.memo.set("新备注");
    state.mouse = layout.save_memo.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("保存备注应产生动作");
    assert_eq!(
        action,
        ui::UiAction::AddMemo {
            character_index: snapshot.friends[1].index,
            memo: "新备注".to_owned()
        }
    );

    // 加好友（勾黑名单）→ C.AddFriend。
    state.mouse = layout.name_field.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.focus, Some(ui::FieldId::FriendName));
    state.friends.name.set("新朋友");
    state.mouse = layout.blocked.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    state.mouse = layout.add.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("添加按钮应产生动作");
    assert_eq!(
        action,
        ui::UiAction::AddFriend {
            name: "新朋友".to_owned(),
            blocked: true
        }
    );

    // 刷新 → C.RefreshFriends。
    state.mouse = layout.refresh.center();
    assert_eq!(
        state.on_click(&snapshot, &text, screen, None),
        Some(ui::UiAction::RefreshFriends)
    );
}

#[test]
fn scrolled_click_selects_the_absolute_row_and_removes_that_friend() {
    let text = ui::UiText::load(None);
    let snapshot = snapshot_with_friends(10);
    let mut state = UiState::new(Language::ZhCn);
    state.toggle_friends();
    state.friends.scroll = 7;
    let screen = Rect::screen(800, 600);
    let layout = friends::friends_layout(screen);
    state.mouse = layout.rows[0].center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.friends.selected, Some(7), "滚动后第一行是第 7 条好友");
    assert_eq!(state.friends.memo.value, snapshot.friends[7].memo);
    state.mouse = layout.remove.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("删除按钮应产生动作");
    assert_eq!(
        action,
        ui::UiAction::RemoveFriend {
            character_index: snapshot.friends[7].index
        },
        "删除的必须是滚动后选中的第 7 条，而不是第 0 条"
    );
}

#[test]
fn friend_update_replaces_the_list_and_language_switch_redraws() {
    let text = ui::UiText::load(None);
    let snapshot = snapshot_with_friends(3);
    let mut state = UiState::new(Language::ZhCn);
    state.toggle_friends();
    let zh = render(&mut state, &snapshot, &text, 800, 600);
    state.language = Language::En;
    let en = render(&mut state, &snapshot, &text, 800, 600);
    let changed = zh
        .pixels
        .chunks_exact(4)
        .zip(en.pixels.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(changed > 20, "切英文后面板应重绘：{changed}");

    // `S.FriendUpdate` 整表替换：再发一条只剩 1 个好友的包。
    let mut updated = snapshot.clone();
    updated.apply_packet(&ServerPacket::FriendUpdate(FriendUpdate {
        friends: vec![ClientFriend {
            index: 7,
            name: "唯一好友".to_owned(),
            memo: "备注".to_owned(),
            blocked: false,
            online: true,
        }],
    }));
    assert_eq!(updated.friends.len(), 1);
    assert_eq!(updated.friends[0].name, "唯一好友");
}

#[test]
fn guild_ally_tab_fits_small_screen_and_saves_png() {
    let text = ui::UiText::load(None);
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::GuildStatus(GuildStatus {
        guild_name: "测试行会".to_owned(),
        level: 3,
        member_count: 5,
        max_members: 20,
        gold: 10_000,
        ..GuildStatus::default()
    }));
    snapshot.apply_packet(&ServerPacket::GuildAllies(GuildAllies {
        allies: vec![GuildAlly {
            index: 7,
            name: "盟友会".to_owned(),
            leader: "盟主".to_owned(),
        }],
    }));
    let mut state = UiState::new(Language::ZhCn);
    state.show_guild = true;
    state.guild_tab = guild::GuildTab::Ally;

    let small = guild::guild_layout(Rect::screen(320, 240), 0, 0, 0, 0);
    assert!(
        small.panel.right() <= 320,
        "行会面板出屏：{:?}",
        small.panel
    );
    for tab in small.tabs {
        assert!(tab.right() <= small.panel.right(), "页签出屏：{tab:?}");
    }
    assert!(small.ally_remove.right() <= small.panel.right());
    assert!(small.ally_field.bottom() <= small.panel.bottom());

    let frame = render(&mut state, &snapshot, &text, 800, 600);
    let output = workspace("target/client-ally.png");
    std::fs::create_dir_all(output.parent().expect("target")).expect("创建 target");
    frame.save_png(&output).expect("写联盟截图");
    let allies_output = workspace("target/client-guild-allies.png");
    frame.save_png(&allies_output).expect("写盟友名单截图");
    assert!(frame.distinct_color_count() > 10);
}

#[test]
fn guild_allies_packet_roundtrips_and_clears_without_guild() {
    let packet = GuildAllies {
        allies: vec![
            GuildAlly {
                index: 7,
                name: "盟友会".to_owned(),
                leader: "盟主".to_owned(),
            },
            GuildAlly {
                index: 9,
                name: "兄弟会".to_owned(),
                leader: "二哥".to_owned(),
            },
        ],
    };
    let mut writer = PacketWriter::new();
    packet.encode(&mut writer);
    let bytes = writer.into_inner();
    let decoded =
        GuildAllies::decode(&mut mir_protocol::PacketReader::new(&bytes)).expect("解码盟友名单");
    assert_eq!(decoded, packet);
    assert_eq!(GuildAllies::INDEX, 303);

    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::GuildAllies(packet));
    assert_eq!(snapshot.guild_allies.len(), 2);
    assert_eq!(snapshot.guild_allies[0].leader, "盟主");

    // 空表（解除联盟）整表替换为 0。
    snapshot.apply_packet(&ServerPacket::GuildAllies(GuildAllies {
        allies: Vec::new(),
    }));
    assert!(snapshot.guild_allies.is_empty());

    // 登录进无行会：服务端不发 GuildAllies，收到空 GuildStatus 要清空列表。
    snapshot.apply_packet(&ServerPacket::GuildAllies(GuildAllies {
        allies: vec![GuildAlly {
            index: 7,
            name: "盟友会".to_owned(),
            leader: "盟主".to_owned(),
        }],
    }));
    snapshot.apply_packet(&ServerPacket::GuildStatus(GuildStatus::default()));
    assert!(snapshot.guild_allies.is_empty(), "无行会时应清空盟友列表");
}
