//! 组队面板的无头测试（不连服务器、不开窗口）。
//!
//! 流程与 `tests/panels.rs` 一致：用 `WorldSnapshot::apply_packet` 注入假包
//! （`S.SwitchGroup` / `S.AddMember` / `S.GroupMembersMap` / `S.SendMemberLocation` /
//! `S.ObjectHealth` / `S.GroupInvite`），渲染整屏到 `target/client-group.png`，
//! 再用布局函数算出的矩形点按钮，断言 [`UiAction`] 与 [`NetCommand`] 正确。
//!
//! 断言覆盖：
//!
//! 1. 成员行（名字/等级/职业/地图/在线/血量）与血量条颜色（高血量绿、低血量红）；
//! 2. HUD 队友血条（聊天面板上方）与地图上队友的蓝色名字、小地图蓝色点；
//! 3. 「邀请」→ `NetCommand::AddMember{name}`、「离队」/「踢人」→ `NetCommand::DelMember{name}`；
//! 4. 「允许组队」开关 → `NetCommand::SwitchGroup{allow_group}`；
//! 5. 组队邀请弹窗 → `NetCommand::GroupReply{accept}`；
//! 6. `!!` 前缀聊天走 `C.Chat`，服务端回显为 `ChatType::Group`。

use std::path::{Path, PathBuf};

use mir_client::geom::Rect;
use mir_client::net::{NetCommand, WorldSnapshot};
use mir_client::ui::{
    self, FieldId, UiAction, UiState, UiText, group, hud, inventory, minimap, worldtext,
};
use mir_core::enums::{ChatType, MirClass, MirDirection, MirGender};
use mir_core::{Language, Point};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ItemInfo, UserItem};
use mir_protocol::packets::server::{
    AddMember, GroupInvite, GroupMembersMap, NewItemInfo, ObjectChat, ObjectHealth,
    SendMemberLocation, SwitchGroup, UserInformation,
};
use mir_render::{EntityKind, Frame, MapScene, MarkerKind, TileLibraries, Viewport};
use mir_world::{Map, MapCell, MapVersion};

/// 工作区根目录（`CARGO_MANIFEST_DIR` 是 `crates/mir-client`）。
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// 无 `resource/` 时的合成地图（与其它无头测试相同）。
fn synthetic_map() -> Map {
    let width = 64;
    let height = 64;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "synthetic-group".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
}

/// 让 `MapScene` 与 `Frame` 就绪，并返回「只有世界」的底帧。
fn scene_and_frame() -> (MapScene, Frame, Point) {
    let scene = MapScene::from_parts(
        synthetic_map(),
        TileLibraries::empty("."),
        Viewport::DEFAULT,
    );
    let frame = Frame::new(800, 600);
    (scene, frame, Point::new(32, 32))
}

/// 渲染一份快照 + 界面状态：先画世界，再叠 UI；返回 `(世界底帧, 成帧)`。
fn render(state: &mut UiState, snapshot: &WorldSnapshot, text: &UiText) -> (Frame, Frame) {
    let (mut scene, mut frame, center) = scene_and_frame();
    let entities = snapshot.entity_views(0);
    scene.render_with_entities(&mut frame, center, &entities);
    let world_only = frame.clone();
    ui::draw(state, &mut frame, &mut scene, snapshot, text, None, center);
    (world_only, frame)
}

/// 统计矩形里与底帧不同的像素数。
fn changed_pixels(before: &Frame, after: &Frame, rect: Rect) -> usize {
    let mut count = 0;
    let left = rect.x.max(0);
    let top = rect.y.max(0);
    let right = rect
        .right()
        .min(i32::try_from(after.width).unwrap_or(i32::MAX));
    let bottom = rect
        .bottom()
        .min(i32::try_from(after.height).unwrap_or(i32::MAX));
    for y in top..bottom {
        for x in left..right {
            let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
                continue;
            };
            if before.pixel(x, y) != after.pixel(x, y) {
                count += 1;
            }
        }
    }
    count
}

/// 统计矩形里「偏绿」的像素数（高血量条 `group::hp_color` 的绿色档）。
fn greenish_pixels(frame: &Frame, rect: Rect) -> usize {
    let mut count = 0;
    for y in rect.y.max(0)
        ..rect
            .bottom()
            .min(i32::try_from(frame.height).unwrap_or(i32::MAX))
    {
        for x in rect.x.max(0)
            ..rect
                .right()
                .min(i32::try_from(frame.width).unwrap_or(i32::MAX))
        {
            let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
                continue;
            };
            if let Some([r, g, b, _]) = frame.pixel(x, y) {
                if g >= 130 && g > r.saturating_add(40) && g > b.saturating_add(40) {
                    count += 1;
                }
            }
        }
    }
    count
}

/// 统计矩形里「偏红」的像素数（低血量条 `group::hp_color` 的红色档）。
fn reddish_pixels(frame: &Frame, rect: Rect) -> usize {
    let mut count = 0;
    for y in rect.y.max(0)
        ..rect
            .bottom()
            .min(i32::try_from(frame.height).unwrap_or(i32::MAX))
    {
        for x in rect.x.max(0)
            ..rect
                .right()
                .min(i32::try_from(frame.width).unwrap_or(i32::MAX))
        {
            let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
                continue;
            };
            if let Some([r, g, b, _]) = frame.pixel(x, y) {
                if r >= 150 && r > g.saturating_add(60) && r > b.saturating_add(60) {
                    count += 1;
                }
            }
        }
    }
    count
}

fn user_item(unique_id: u64, item_index: i32, count: u16, dura: u16) -> UserItem {
    UserItem {
        unique_id,
        item_index,
        count,
        current_dura: dura,
        max_dura: dura,
        ..UserItem::default()
    }
}

fn item_info(index: i32, name: &str, image: u16, price: u32, stack_size: u16) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        image,
        price,
        stack_size,
        ..ItemInfo::default()
    }
}

/// 基础世界：自己是「面板测试」（12 级战士）。
fn base_snapshot() -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    let mut slots = vec![None; inventory::INVENTORY_SLOTS];
    slots[0] = Some(user_item(11, 1, 1, 5_000));
    snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
        object_id: 7,
        real_id: 7,
        name: "面板测试".to_owned(),
        location: Point::new(32, 32),
        direction: MirDirection::Down,
        class: MirClass::Warrior,
        gender: MirGender::Male,
        level: 12,
        hair: 1,
        hp: 50,
        mp: 30,
        gold: 4_321,
        inventory: Some(slots),
        ..UserInformation::default()
    }));
    snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
        info: item_info(1, "木剑", 1, 1_200, 1),
    }));
    snapshot
}

/// 队伍：自己是队长 + 一个可见的法师队友 + 一个视野外的队友。
fn group_snapshot() -> WorldSnapshot {
    let mut snapshot = base_snapshot();
    snapshot.apply_packet(&ServerPacket::SwitchGroup(SwitchGroup {
        allow_group: true,
    }));
    // 服务端把「已有成员」逐个发给接受者，再把新成员发给大家；自己是队长时第一条就是自己。
    for name in ["面板测试", "队友甲", "队友乙"] {
        snapshot.apply_packet(&ServerPacket::AddMember(AddMember {
            name: name.to_owned(),
        }));
    }
    for (name, map) in [
        ("面板测试", "比奇省"),
        ("队友甲", "比奇省"),
        ("队友乙", "盟重省"),
    ] {
        snapshot.apply_packet(&ServerPacket::GroupMembersMap(GroupMembersMap {
            player_name: name.to_owned(),
            player_map: map.to_owned(),
        }));
    }
    // 队友甲的坐标（大地图队友点用）。
    snapshot.apply_packet(&ServerPacket::SendMemberLocation(SendMemberLocation {
        member_name: "队友甲".to_owned(),
        member_location: Point::new(34, 30),
    }));
    // 队友甲在自己的视野里：`S.ObjectPlayer` 的界面子集（直接建实体比造整包短）。
    snapshot.entities.insert(
        91,
        mir_client::net::WorldEntity {
            object_id: 91,
            kind: EntityKind::Player,
            name: "队友甲".to_owned(),
            position: Point::new(34, 30),
            direction: MirDirection::Left,
            image: 0,
            looks: mir_client::net::PlayerLooks::default(),
            dead: false,
            hp_percent: None,
            level: 22,
            class: MirClass::Wizard,
            name_colour: None,
            action: mir_render::animation::ActorAction::Standing,
            action_started_ms: 0,
        },
    );
    // 同图近距离时服务端会补发队友血条（`crates/mir-server/src/group.rs` 的 `group_join`）。
    snapshot.apply_packet(&ServerPacket::ObjectHealth(ObjectHealth {
        object_id: 91,
        percent: 70,
        expire: 0,
    }));
    snapshot
}

#[test]
fn group_panel_shows_members_health_and_sends_group_packets() {
    let mut snapshot = group_snapshot();
    // 成员表：3 人，自己是队长。
    let infos = snapshot.group_member_infos();
    assert_eq!(infos.len(), 3);
    assert!(infos[0].is_self && infos[0].is_leader, "members[0] 是队长");
    assert_eq!(infos[0].name, "面板测试");
    assert_eq!(infos[1].name, "队友甲");
    assert_eq!(infos[1].level, 22);
    assert_eq!(infos[1].class, Some(MirClass::Wizard));
    assert_eq!(infos[1].map, "比奇省");
    assert!(infos[1].online, "队友甲在视野里 → 在线");
    assert_eq!(infos[1].hp_percent, Some(70), "血量来自 S.ObjectHealth");
    assert!(!infos[2].online, "队友乙不在视野里 → 离线");
    assert_eq!(infos[2].hp_percent, None);
    assert_eq!(
        snapshot
            .group
            .member("队友甲")
            .and_then(|member| member.location),
        Some(Point::new(34, 30)),
        "S.SendMemberLocation 应记录队员坐标"
    );

    let text = UiText::load(None);
    let mut state = UiState::new(Language::ZhCn);
    state.show_group = true;
    state.sync(&snapshot);
    let screen = Rect::screen(800, 600);
    let layout = group::group_layout(screen, snapshot.group.members.len(), 0);
    assert_eq!(layout.rows.len(), 3);

    let (world_only, frame) = render(&mut state, &snapshot, &text);
    let changed = changed_pixels(&world_only, &frame, layout.panel);
    assert!(changed > 1_000, "组队面板应覆盖地图像素，实际 {changed}");

    // 队友甲那一行的血量条是绿色（70%），队友乙没有血量条。
    let row = layout.rows[1];
    let bar = Rect::new(
        row.x + group::column_offsets(layout.header.width)[5],
        row.y,
        group::HP_BAR_WIDTH,
        group::ROW_HEIGHT,
    );
    let green = greenish_pixels(&frame, bar);
    assert!(green > 30, "70% 血量条应是绿色，实际 {green} 像素");
    let offline_row = layout.rows[2];
    let offline_bar = Rect::new(
        offline_row.x + group::column_offsets(layout.header.width)[5],
        offline_row.y,
        group::HP_BAR_WIDTH,
        group::ROW_HEIGHT,
    );
    assert_eq!(
        greenish_pixels(&frame, offline_bar),
        0,
        "没有血量的队友不画绿条"
    );

    // HUD 队友血条（聊天面板上方）也画出来了。
    let party = hud::party_layout(screen, 2).expect("有队友时应显示 HUD 血条面板");
    assert!(party.bottom() <= hud::chat_layout(screen).y);
    assert!(
        changed_pixels(&world_only, &frame, party) > 200,
        "HUD 队友血条应画出来"
    );

    let output = workspace_root().join("target/client-group.png");
    frame
        .save_png(&output)
        .expect("写出 target/client-group.png");
    assert!(std::fs::metadata(&output).is_ok_and(|meta| meta.len() > 4_096));

    // 低血量时血量条变红（`S.ObjectHealth` 再发一次 10%）。
    let mut low = snapshot.clone();
    low.apply_packet(&ServerPacket::ObjectHealth(ObjectHealth {
        object_id: 91,
        percent: 10,
        expire: 0,
    }));
    let (_, low_frame) = render(&mut state, &low, &text);
    let red = reddish_pixels(&low_frame, bar);
    assert!(red > 20, "10% 血量条应是红色，实际 {red} 像素");

    // 地图上的队友名字是队伍蓝；小地图上的队友点是蓝色（`MarkerKind::Group`）。
    let teammate = snapshot.entities.get(&91).expect("队友甲的实体");
    assert_eq!(
        worldtext::name_color(&snapshot, teammate),
        group::GROUP_NAME_COLOR,
        "队友的头顶名字应是队伍蓝"
    );
    let view = minimap::view_for(&snapshot, (64, 64));
    let marker = view
        .markers
        .iter()
        .find(|marker| marker.cell == Point::new(34, 30))
        .expect("小地图应有队友点");
    assert_eq!(marker.kind, MarkerKind::Group);

    // 选中第二行 → 邀请输入名字 → 点「邀请」→ `C.AddMember{Name}`。
    state.mouse = layout.rows[1].center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.group_selected, Some(1));
    state.mouse = layout.invite_field.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.focus, Some(FieldId::GroupName));
    for character in "新人".chars() {
        assert!(state.type_char(character));
    }
    state.mouse = layout.invite.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("邀请按钮应产生动作");
    assert_eq!(
        action,
        UiAction::AddMember {
            name: "新人".to_owned()
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::AddMember {
            name: "新人".to_owned()
        })
    );

    // 选中第二行后点「踢人」→ `C.DelMember{Name = 队友甲}`（队长才有权限）。
    state.mouse = layout.rows[1].center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    state.mouse = layout.kick.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("踢人按钮应产生动作");
    assert_eq!(
        action,
        UiAction::DelMember {
            name: "队友甲".to_owned()
        }
    );

    // 「离队」→ `C.DelMember{Name = 自己的名字}`（Crystal 的退队就是这个包）。
    state.mouse = layout.leave.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("离队按钮应产生动作");
    assert_eq!(
        action,
        UiAction::DelMember {
            name: "面板测试".to_owned()
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::DelMember {
            name: "面板测试".to_owned()
        })
    );

    // 「允许组队」开关 → `C.SwitchGroup{AllowGroup = !当前值}`。
    state.mouse = layout.switch.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("允许组队开关应产生动作");
    assert_eq!(action, UiAction::SwitchGroup { allow_group: false });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::SwitchGroup { allow_group: false })
    );

    // 组队邀请弹窗 → `C.GroupInvite{AcceptInvite}`。
    let mut invited = base_snapshot();
    invited.apply_packet(&ServerPacket::GroupInvite(GroupInvite {
        name: "队长甲".to_owned(),
    }));
    let mut invite_state = UiState::new(Language::ZhCn);
    invite_state.sync(&invited);
    assert_eq!(invite_state.group_invite.as_deref(), Some("队长甲"));
    let (world_only, invite_frame) = render(&mut invite_state, &invited, &text);
    let notice = ui::notice::notice_layout(screen);
    assert!(
        changed_pixels(&world_only, &invite_frame, notice.panel) > 1_000,
        "组队邀请弹窗应画在屏幕中央"
    );
    invite_state.mouse = notice.accept.center();
    let action = invite_state
        .on_click(&invited, &text, screen, None)
        .expect("点「接受」应产生动作");
    assert_eq!(action, UiAction::GroupReply { accept: true });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::GroupReply { accept: true })
    );
    assert!(invite_state.group_invite.is_none());

    // `!!` 前缀聊天：客户端发普通 `C.Chat`，服务端按前缀转成组队广播。
    assert!(group::is_group_chat("!!集合"));
    assert_eq!(
        group::group_chat_command("!!集合"),
        Some(NetCommand::Chat("!!集合".to_owned()))
    );
    snapshot.apply_packet(&ServerPacket::ObjectChat(ObjectChat {
        object_id: 7,
        text: "面板测试: 集合".to_owned(),
        type_: ChatType::Group,
    }));
    let last = snapshot.chat.last().expect("应有聊天行");
    assert_eq!(last.kind, ChatType::Group, "组队聊天用 ChatType::Group");
    assert!(last.text.contains("集合"));

    // 队伍解散（`S.DeleteGroup`）后成员表清空、面板显示「未组队」。
    snapshot.apply_packet(&ServerPacket::DeleteGroup(
        mir_protocol::packets::server::DeleteGroup {},
    ));
    assert!(snapshot.group.members.is_empty());
    assert!(!snapshot.has_group());
}

/// 编译期探针：布局函数在小窗口下仍然可用。
#[test]
fn group_layouts_survive_small_windows() {
    let small = Rect::screen(320, 240);
    assert!(group::group_layout(small, 3, 0).panel.right() <= 320);
    assert!(hud::party_layout(small, 2).is_some());
    assert!(ui::notice::notice_layout(small).panel.width <= 320);
}
