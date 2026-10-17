//! 客户端切图的注入式测试：收到 `S.MapChanged` 后快照清空、坐标/地图号更新，
//! UI（app）记录的「重载地形请求」指向 `<地图目录>/<文件名>.map`。
//!
//! 不起服务器、不开窗口：直接把 `ServerPacket::MapChanged` 与 `NetEvent::MapChanged`
//! 注入给快照/事件循环，模拟 `mir-client` 网络线程的处理顺序（先 `Snapshot` 后 `MapChanged`）。

use std::path::PathBuf;

use mir_client::net::{MapReloadRequest, NetEvent, WorldSnapshot};
use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_protocol::ServerPacket;
use mir_protocol::manual::server::UserInformation;
use mir_protocol::packets::server::{MapChanged, ObjectMonster};

/// 最小 `S.UserInformation`：自己的对象 ID / 坐标。
fn user_information() -> UserInformation {
    UserInformation {
        object_id: 42,
        name: "测试角色".to_owned(),
        class: MirClass::Warrior,
        gender: MirGender::Male,
        level: 1,
        location: Point::new(20, 20),
        direction: MirDirection::Down,
        hp: 10,
        mp: 5,
        max_experience: 100,
        ..UserInformation::default()
    }
}

/// 一条 `S.MapChanged`（字段见 `Shared/ServerPackets.cs:2979-3025`）。
fn map_changed(file_name: &str) -> MapChanged {
    MapChanged {
        map_index: 7,
        file_name: file_name.to_owned(),
        title: "龙之谷".to_owned(),
        location: Point::new(3, 4),
        direction: MirDirection::Left,
        ..MapChanged::default()
    }
}

/// 模拟 `ClientApp::poll_network` 的事件循环：只记录切图触发的重载请求，不建窗口。
fn record_reload_requests(events: Vec<NetEvent>, map_dir: &str) -> Vec<MapReloadRequest> {
    let mut latest: Option<WorldSnapshot> = None;
    let mut requests = Vec::new();
    for event in events {
        match event {
            NetEvent::Snapshot(snapshot) => latest = Some(*snapshot),
            NetEvent::MapChanged { .. } => {
                if let Some(snapshot) = &latest {
                    if let Some(request) = snapshot.map_reload_request(map_dir) {
                        requests.push(request);
                    }
                }
            }
            NetEvent::Error(_) | NetEvent::Closed(_) => {}
        }
    }
    requests
}

#[test]
fn map_changed_clears_entities_updates_position_and_records_a_reload_request() {
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::UserInformation(user_information()));
    snapshot.apply_packet(&ServerPacket::ObjectMonster(ObjectMonster {
        object_id: 7,
        name: "鸡".to_owned(),
        location: Point::new(21, 20),
        ..ObjectMonster::default()
    }));
    assert_eq!(snapshot.entities.len(), 1, "切图前应有 1 个实体");

    let changed = map_changed("D2083");
    snapshot.apply_packet(&ServerPacket::MapChanged(changed.clone()));

    // 快照：地图号/文件名更新、实体清空、自己的坐标与朝向跟着 MapChanged 走。
    let map = snapshot.map.as_ref().expect("地图信息");
    assert_eq!(map.map_index, 7);
    assert_eq!(map.file_name, "D2083");
    assert_eq!(map.title, "龙之谷");
    assert!(snapshot.entities.is_empty(), "切图必须清空旧实体");
    let me = snapshot.self_player.as_ref().expect("自己的状态");
    assert_eq!(me.position, Point::new(3, 4));
    assert_eq!(me.direction, MirDirection::Left);
    assert_eq!(snapshot.packet_count("MapChanged"), 1);

    // 网络线程先发快照、再发切图事件；UI 记录的重载请求指向新地图文件。
    let requests = record_reload_requests(
        vec![
            NetEvent::Snapshot(Box::new(snapshot.clone())),
            NetEvent::MapChanged {
                map_index: changed.map_index,
                position: changed.location,
            },
        ],
        "resource/map",
    );
    assert_eq!(requests.len(), 1, "应记录 1 次重载请求");
    assert_eq!(requests[0].map_index, 7);
    assert_eq!(requests[0].file_name, "D2083");
    assert_eq!(requests[0].position, Point::new(3, 4));
    assert_eq!(requests[0].path, PathBuf::from("resource/map/D2083.map"));
}

#[test]
fn empty_file_name_falls_back_to_the_map_index_convention() {
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::UserInformation(user_information()));
    // 简化过的服务端/老协议可能只给地图主键；客户端按「地图号 == 文件名」找图。
    snapshot.apply_packet(&ServerPacket::MapChanged(MapChanged {
        map_index: 3,
        file_name: String::new(),
        location: Point::new(10, 11),
        ..MapChanged::default()
    }));

    let request = snapshot
        .map_reload_request("resource/map")
        .expect("没有文件名时也应有重载请求");
    assert_eq!(request.file_name, "3");
    assert_eq!(request.path, PathBuf::from("resource/map/3.map"));
    assert_eq!(request.position, Point::new(10, 11));
}

#[test]
fn map_changed_before_user_information_still_updates_the_map() {
    let mut snapshot = WorldSnapshot::default();
    // 还没收到 `UserInformation` 时切图：地图号要更新、实体为空、没有自己的坐标。
    snapshot.apply_packet(&ServerPacket::MapChanged(map_changed("1")));
    assert_eq!(snapshot.map.as_ref().map(|map| map.map_index), Some(7));
    assert!(snapshot.self_player.is_none());
    assert!(snapshot.entities.is_empty());
    let request = snapshot
        .map_reload_request("resource/map")
        .expect("重载请求");
    assert_eq!(request.position, Point::new(0, 0));
}
