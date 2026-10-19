//! 攻城器械/HUD/新包（t69）。
//!
//! 覆盖：300/301/302 包 roundtrip、客户端快照字段、无包兼容、HUD 倒计时、
//! 器械色块回退、结算横幅 5 秒、截图 `target/client-conquest.png`。

use std::path::Path;

use mir_client::net::WorldSnapshot;
use mir_client::ui::{self, UiState, conquest};
use mir_core::Language;
use mir_core::Point;
use mir_protocol::packets::server::{
    ConquestControlPoint, ConquestInfo, ConquestObject, ConquestObjectType, ConquestResult,
};
use mir_protocol::{PacketReader, PacketWriter, ServerPacket};
use mir_render::{Frame, MapScene};

fn conquest_info() -> ConquestInfo {
    ConquestInfo {
        index: 1,
        name: "沙巴克".to_owned(),
        owner: 7,
        war_is_on: true,
        start_time: 1_700_000_000_000,
        end_time: 1_700_000_600_000,
        control_points: vec![
            ConquestControlPoint {
                index: 0,
                owner: 7,
                points: 3,
            },
            ConquestControlPoint {
                index: 1,
                owner: 9,
                points: 5,
            },
        ],
    }
}

fn object(object_id: u32, kind: ConquestObjectType, x: i32, y: i32) -> ConquestObject {
    ConquestObject {
        object_id,
        conquest_index: 1,
        kind,
        location: Point::new(x, y),
        direction: mir_core::enums::MirDirection::Down,
        hp: 60,
        max_hp: 100,
        owner: 7,
        destroyed: false,
        open: false,
    }
}

fn scene() -> MapScene {
    let map = mir_world::Map {
        width: 64,
        height: 64,
        cells: vec![mir_world::MapCell::default(); 64 * 64],
        file_name: "conquest-test".to_owned(),
        version: mir_world::MapVersion::V0,
        doors: Vec::new(),
    };
    MapScene::from_parts(
        map,
        mir_render::TileLibraries::empty("."),
        mir_render::Viewport::DEFAULT,
    )
}

fn snapshot_with_conquest() -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::ConquestInfo(conquest_info()));
    snapshot.apply_packet(&ServerPacket::ConquestObject(object(
        10,
        ConquestObjectType::Gate,
        30,
        30,
    )));
    snapshot.apply_packet(&ServerPacket::ConquestObject(object(
        11,
        ConquestObjectType::Wall,
        32,
        30,
    )));
    snapshot.apply_packet(&ServerPacket::ConquestObject(object(
        12,
        ConquestObjectType::Archer,
        31,
        28,
    )));
    snapshot
}

#[test]
fn conquest_packets_roundtrip_through_encode_decode() {
    let info = conquest_info();
    let mut writer = PacketWriter::new();
    info.encode(&mut writer);
    let bytes = writer.into_inner();
    assert_eq!(
        ConquestInfo::decode(&mut PacketReader::new(&bytes)).unwrap(),
        info
    );
    assert_eq!(ConquestInfo::INDEX, 300);

    let gate = object(10, ConquestObjectType::Gate, 30, 30);
    let mut writer = PacketWriter::new();
    gate.encode(&mut writer);
    let bytes = writer.into_inner();
    assert_eq!(
        ConquestObject::decode(&mut PacketReader::new(&bytes)).unwrap(),
        gate
    );
    assert_eq!(ConquestObject::INDEX, 301);

    let result = ConquestResult {
        index: 1,
        winner: 7,
        owner: 7,
        control_points: conquest_info().control_points,
    };
    let mut writer = PacketWriter::new();
    result.encode(&mut writer);
    let bytes = writer.into_inner();
    assert_eq!(
        ConquestResult::decode(&mut PacketReader::new(&bytes)).unwrap(),
        result
    );
    assert_eq!(ConquestResult::INDEX, 302);
}

#[test]
fn snapshot_tracks_objects_and_destroyed_chat() {
    let mut snapshot = snapshot_with_conquest();
    let view = snapshot.conquest.as_ref().expect("征服状态");
    assert_eq!(view.name, "沙巴克");
    assert_eq!(view.objects.len(), 3);
    assert_eq!(view.control_points.len(), 2);
    assert!(view.war_is_on);
    // 开战提示已经进聊天。
    assert!(snapshot.chat.iter().any(|line| matches!(
        line.system,
        Some(mir_client::net::SystemChat::ConquestStarted { .. })
    )));

    // 摧毁城门：置 destroyed，并只提示一次。
    let mut gate = object(10, ConquestObjectType::Gate, 30, 30);
    gate.destroyed = true;
    let before = snapshot.chat.len();
    snapshot.apply_packet_at(&ServerPacket::ConquestObject(gate.clone()), 1_000);
    let view = snapshot.conquest.as_ref().expect("征服状态");
    assert!(view.objects[&10].destroyed);
    assert_eq!(view.objects[&10].destroyed_at_ms, 1_000);
    assert_eq!(snapshot.chat.len(), before + 1);
    snapshot.apply_packet_at(&ServerPacket::ConquestObject(gate), 2_000);
    assert_eq!(snapshot.chat.len(), before + 1, "重复摧毁包不应刷屏");

    // 结算横幅：写入 result + 结束聊天。
    let result = ConquestResult {
        index: 1,
        winner: 9,
        owner: 9,
        control_points: Vec::new(),
    };
    snapshot.apply_packet_at(&ServerPacket::ConquestResult(result), 3_000);
    let view = snapshot.conquest.as_ref().expect("征服状态");
    let result = view.result.as_ref().expect("结算");
    assert_eq!(result.winner, 9);
    assert_eq!(result.received_ms, 3_000);
    assert!(snapshot.chat.iter().any(|line| matches!(
        line.system,
        Some(mir_client::net::SystemChat::ConquestEnded { .. })
    )));
}

#[test]
fn no_conquest_packets_leave_snapshot_and_ui_unchanged() {
    let snapshot = WorldSnapshot::default();
    assert!(snapshot.conquest.is_none());
    assert!(!conquest::has_conquest(&snapshot));
    let mut scene = scene();
    let mut frame = Frame::new(320, 240);
    let text = ui::UiText::load(None);
    let mut state = UiState::new(Language::ZhCn);
    ui::draw(
        &mut state,
        &mut frame,
        &mut scene,
        &snapshot,
        &text,
        None,
        Point::new(10, 10),
    );
    // 老服务器/单机：没有新包也不 panic，画面照旧有内容。
    assert!(frame.distinct_color_count() >= 1);
}

#[test]
fn hud_timer_and_result_banner_expire() {
    let mut snapshot = snapshot_with_conquest();
    let view = snapshot.conquest.as_ref().expect("征服状态");
    assert_eq!(conquest::remaining_seconds(view, 1_700_000_000_000), 600);
    assert_eq!(conquest::format_remaining(600), "10:00");
    assert_eq!(conquest::format_remaining(65), "01:05");
    assert_eq!(conquest::remaining_seconds(view, 1_700_000_700_000), 0);

    snapshot.apply_packet_at(
        &ServerPacket::ConquestResult(ConquestResult {
            index: 1,
            winner: 7,
            owner: 7,
            control_points: Vec::new(),
        }),
        10_000,
    );
    let mut scene = scene();
    let text = ui::UiText::load(None);
    let mut state = UiState::new(Language::ZhCn);
    let mut inside = Frame::new(320, 240);
    state.set_time(10_000);
    ui::draw(
        &mut state,
        &mut inside,
        &mut scene,
        &snapshot,
        &text,
        None,
        Point::new(10, 10),
    );
    let mut outside = Frame::new(320, 240);
    state.set_time(15_000);
    ui::draw(
        &mut state,
        &mut outside,
        &mut scene,
        &snapshot,
        &text,
        None,
        Point::new(10, 10),
    );
    let changed = inside
        .pixels
        .chunks_exact(4)
        .zip(outside.pixels.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(changed > 20, "5 秒后结算横幅应当消失：{changed}");
}

#[test]
fn objects_render_with_fallback_blocks_and_save_png() {
    let snapshot = snapshot_with_conquest();
    let mut scene = scene();
    let text = ui::UiText::load(None);
    let mut state = UiState::new(Language::ZhCn);
    state.set_time(1_700_000_100_000);
    let mut frame = Frame::new(800, 600);
    ui::draw(
        &mut state,
        &mut frame,
        &mut scene,
        &snapshot,
        &text,
        None,
        Point::new(30, 30),
    );
    assert!(frame.distinct_color_count() > 20, "器械/HUD 应当有颜色");
    let output = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target/client-conquest.png");
    std::fs::create_dir_all(output.parent().expect("target")).expect("创建 target");
    frame.save_png(&output).expect("写攻城截图");

    // 器械在格子上方有像素变化（空图库 → 色块回退）。
    let (x, y) = scene.world_to_screen(Point::new(30, 30), Point::new(30, 30));
    let mut painted = 0;
    for dy in -60..10 {
        for dx in -10..50 {
            if let (Ok(x), Ok(y)) = (u32::try_from(x + dx), u32::try_from(y + dy))
                && frame.pixel(x, y).is_some_and(|pixel| pixel[3] != 0)
            {
                painted += 1;
            }
        }
    }
    assert!(painted > 100, "城门色块没有画出来：{painted}");
    // 类型文字的中文/英文标签存在。
    assert_ne!(
        mir_client::ui::labels::conquest_object_name(ConquestObjectType::Gate, Language::ZhCn),
        ""
    );
}

fn render_at(snapshot: &WorldSnapshot, now: u64) -> Frame {
    let mut scene = scene();
    let text = ui::UiText::load(None);
    let mut state = UiState::new(Language::ZhCn);
    state.set_time(i64::try_from(now).unwrap_or(i64::MAX));
    let mut frame = Frame::new(800, 600);
    ui::draw(
        &mut state,
        &mut frame,
        &mut scene,
        snapshot,
        &text,
        None,
        Point::new(30, 30),
    );
    frame
}

#[test]
fn gate_open_state_changes_the_rendering() {
    let mut snapshot = snapshot_with_conquest();
    let closed = render_at(&snapshot, 1_000);
    // 服务端 `ConquestObject.open = true`：城门开启（同一格、同一 HP）。
    let mut gate = object(10, ConquestObjectType::Gate, 30, 30);
    gate.open = true;
    snapshot.apply_packet_at(&ServerPacket::ConquestObject(gate), 2_000);
    let view = snapshot.conquest.as_ref().expect("征服状态");
    assert!(conquest::gate_is_open(&view.objects[&10]));
    let opened = render_at(&snapshot, 2_000);
    let changed = closed
        .pixels
        .chunks_exact(4)
        .zip(opened.pixels.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(changed > 100, "开/关城门应有明显像素差：{changed}");
    let output = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target/client-conquest-gate.png");
    std::fs::create_dir_all(output.parent().expect("target")).expect("创建 target");
    opened.save_png(&output).expect("写城门截图");
}

#[test]
fn archer_range_attack_spawns_a_projectile() {
    let mut snapshot = snapshot_with_conquest();
    // 城墙旁边那只箭塔（object_id=12）开火。
    snapshot.apply_packet_at(
        &ServerPacket::ObjectRangeAttack(mir_protocol::packets::server::ObjectRangeAttack {
            object_id: 12,
            location: Point::new(31, 28),
            direction: mir_core::enums::MirDirection::Down,
            target_id: 7,
            target: Point::new(30, 30),
            type_: 0,
            spell: mir_core::enums::Spell::None,
            level: 1,
        }),
        5_000,
    );
    assert_eq!(snapshot.conquest_arrows.len(), 1);
    let arrow = snapshot.conquest_arrows[0];
    assert_eq!(arrow.from, Point::new(31, 28));
    assert_eq!(arrow.to, Point::new(30, 30));
    assert_eq!(arrow.started_ms, 5_000);
    let with_arrow = render_at(&snapshot, 5_100);
    let mut plain = snapshot.clone();
    plain.conquest_arrows.clear();
    let without_arrow = render_at(&plain, 5_100);
    let changed = with_arrow
        .pixels
        .chunks_exact(4)
        .zip(without_arrow.pixels.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(changed > 10, "箭塔弹道应当画出来：{changed}");
}
