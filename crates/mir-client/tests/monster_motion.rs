//! R49 t48 怪物走/跑插值与联机快照参数证据。
//!
//! 非 ignored 用例不需要素材：用合成帧表覆盖
//! - `NetClient` 会话首帧快照携带完整帧表与每怪 Count/Interval；
//! - Right 从 `-OffSetMove` 起点收敛到终点，没有 96px 跳变；
//! - Count10×100ms 的 Walk 在 600ms 仍有帧、约 1000ms 才 Standing；
//! - 无帧表库回退 6×100ms。
//!
//! ignored 用例读默认 `game-assets` 的 005/022/184 真实 `frames.json`，生成
//! `docs/testing/monster-motion/` 的 PNG/manifest。运行：
//! `$env:TEMP='...\target\tmp'; $env:TMP=$env:TEMP; cargo test -p mir-client
//! --test monster_motion -- --ignored --nocapture`。

use std::collections::BTreeMap;
use std::io::Read;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

use mir_asset::{Frame, FrameSet};
use mir_client::camera::MonsterMotionParams;
use mir_client::net::{ConnectOptions, NetClient, WorldSnapshot};
use mir_core::Point;
use mir_core::enums::{MirDirection, Monster};
use mir_protocol::packets::server::{ObjectMonster, ObjectWalk, ServerPacket};
use mir_render::entities::EntityView;
use mir_render::{ActorAction, EntityLibraries, Frame as RenderFrame, MapScene};
use serde_json::json;

fn now_millis_u64() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

fn workspace_root() -> PathBuf {
    let mut dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    while !dir.join("game-assets/library-map.json").is_file() {
        dir = dir.parent().expect("workspace root");
    }
    dir.to_path_buf()
}

fn frame(action: u8, start: i32, count: i32, skip: i32, interval: i32) -> Frame {
    Frame {
        action,
        start,
        count,
        skip,
        interval,
        effect_start: 0,
        effect_count: 0,
        effect_skip: 0,
        effect_interval: 0,
        reverse: false,
        blend: false,
    }
}

fn walk_frames(count: i32, interval: i32) -> FrameSet {
    FrameSet {
        frames: vec![
            frame(0, 0, 6, 0, 500),
            frame(1, 48, count, 0, interval),
            frame(9, 96, 6, 0, 100),
        ],
        unknown_actions: Vec::new(),
    }
}

/// 生成 ISO-8601 UTC batch 时间戳，避免为证据测试新增 chrono 依赖。
fn iso_utc(unix_ms: u64) -> String {
    fn civil_from_days(days: i64) -> (i64, u32, u32) {
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let year = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        (if month <= 2 { year + 1 } else { year }, month, day)
    }
    let seconds = (unix_ms / 1000) as i64;
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

fn connect_options(server: String, frames: BTreeMap<u16, FrameSet>) -> ConnectOptions {
    ConnectOptions {
        server,
        account: "r49-t48".to_owned(),
        password: "r49-t48".to_owned(),
        character: "r49-t48".to_owned(),
        monster_frames: frames,
        auto_register: false,
        ..ConnectOptions::default()
    }
}

#[test]
fn session_initial_snapshot_carries_real_frame_parameters() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind fake server");
    let address = listener.local_addr().expect("fake server address");
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept client");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read timeout");
        let mut buffer = [0u8; 512];
        // 客户端 Drop 时会发 Disconnect；读到 EOF/超时即退出。
        while matches!(stream.read(&mut buffer), Ok(size) if size > 0) {}
    });

    let frames = BTreeMap::from([(5u16, walk_frames(6, 100)), (184u16, walk_frames(10, 100))]);
    let client = NetClient::connect(connect_options(address.to_string(), frames)).expect("connect");
    let snapshot = client
        .wait_for_snapshot(|_| true, Duration::from_secs(5))
        .expect("session 立即发送首帧快照");

    // X1-03：生产 session 的首个 NetEvent::Snapshot 就带完整帧表与每怪参数，
    // 不依赖 UI 副本或后续服务器包补写。
    assert_eq!(snapshot.monster_frames.len(), 2);
    assert_eq!(snapshot.monster_frames[&5].get(1).expect("walk").count, 6);
    assert_eq!(
        snapshot.monster_frames[&184].get(1).expect("walk").count,
        10
    );
    assert_eq!(snapshot.monster_motion_params[&5].walk_count, 6);
    assert_eq!(snapshot.monster_motion_params[&184].walk_count, 10);
    assert_ne!(
        snapshot.monster_motion_params[&5],
        snapshot.monster_motion_params[&184]
    );

    drop(client);
    server.join().expect("fake server thread");
}

fn apply_walk(
    snapshot: &mut WorldSnapshot,
    image: u16,
    from: Point,
    direction: MirDirection,
    now: u64,
) {
    snapshot.apply_packet_at(
        &ServerPacket::ObjectMonster(ObjectMonster {
            object_id: u32::from(image) + 1_000,
            location: from,
            image: Monster::from_value(image).unwrap_or_default(),
            direction,
            ..ObjectMonster::default()
        }),
        now,
    );
    snapshot.apply_packet_at(
        &ServerPacket::ObjectWalk(ObjectWalk {
            object_id: u32::from(image) + 1_000,
            location: from.moved(direction, 1),
            direction,
        }),
        now,
    );
}

fn view(snapshot: &WorldSnapshot, now: u64, image: u16) -> EntityView {
    snapshot
        .entity_views(now)
        .into_iter()
        .find(|view| view.image == image)
        .unwrap_or_else(|| panic!("找不到 image {image} 的实体视图"))
}

#[test]
fn right_walk_advance_converges_to_destination_without_96px_jump() {
    let mut snapshot = WorldSnapshot::default();
    snapshot.monster_frames.insert(5, walk_frames(6, 100));
    let started = 1_000;
    apply_walk(
        &mut snapshot,
        5,
        Point::new(9, 10),
        MirDirection::Right,
        started,
    );

    let from_x = 9 * 48;
    let to_x = 10 * 48;
    let at_0 = view(&snapshot, started, 5);
    assert_eq!(at_0.action, ActorAction::Walking);
    assert_eq!(at_0.position, Point::new(9, 10));
    assert_eq!(at_0.pixel_offset, (8, 0));
    assert_eq!(from_x + at_0.pixel_offset.0, from_x + 8);

    let at_500 = view(&snapshot, started + 500, 5);
    assert_eq!(at_500.action, ActorAction::Walking);
    assert_eq!(at_500.pixel_offset, (48, 0));
    assert_eq!(from_x + at_500.pixel_offset.0, to_x);

    // 旧实现直接加 OffSetMove：末帧 from_x - 48 = 384，下一帧跳到 480。
    // advance_at 的符号修正后末帧已在终点，motion 到期不会再跳 96px。
    assert_eq!(to_x - (from_x - 48), 96);

    let at_600 = view(&snapshot, started + 600, 5);
    assert_eq!(at_600.action, ActorAction::Standing);
    assert_eq!(at_600.position, Point::new(10, 10));
    assert_eq!(at_600.pixel_offset, (0, 0));
    assert_eq!(at_600.position.x * 48, to_x);
}

#[test]
fn count10_walk_keeps_frames_at_600ms_and_stands_at_1000ms() {
    let mut snapshot = WorldSnapshot::default();
    snapshot.monster_frames.insert(184, walk_frames(10, 100));
    let started = 2_000;
    apply_walk(
        &mut snapshot,
        184,
        Point::new(10, 10),
        MirDirection::Right,
        started,
    );

    // Count×Interval=1000ms：600ms 时仍在第 6 帧，不能按旧 600ms 提前 Standing。
    let at_600 = view(&snapshot, started + 600, 184);
    assert_eq!(at_600.action, ActorAction::Walking);
    assert_eq!(at_600.position, Point::new(10, 10));
    assert_eq!(at_600.pixel_offset.0, 34);

    // 1000ms 到期先由 effective_action 回 Standing，motion 同时清掉。
    let at_999 = view(&snapshot, started + 999, 184);
    assert_eq!(at_999.action, ActorAction::Walking);
    assert_eq!(at_999.pixel_offset.0, 48);
    let at_1000 = view(&snapshot, started + 1_000, 184);
    assert_eq!(at_1000.action, ActorAction::Standing);
    assert_eq!(at_1000.position, Point::new(11, 10));
    assert_eq!(at_1000.pixel_offset, (0, 0));
}

#[test]
fn missing_frame_table_falls_back_to_six_frames_of_100ms() {
    let mut snapshot = WorldSnapshot::default();
    snapshot
        .monster_motion_params
        .insert(5, MonsterMotionParams::default());
    let started = 3_000;
    apply_walk(
        &mut snapshot,
        5,
        Point::new(10, 10),
        MirDirection::Down,
        started,
    );

    let at_500 = view(&snapshot, started + 500, 5);
    assert_eq!(at_500.action, ActorAction::Walking);
    assert_eq!(at_500.pixel_offset, (0, 32));
    let at_600 = view(&snapshot, started + 600, 5);
    assert_eq!(at_600.action, ActorAction::Standing);
    assert_eq!(at_600.position, Point::new(10, 11));
    assert_eq!(at_600.pixel_offset, (0, 0));
}

#[test]
#[ignore = "需要 game-assets 原生怪物拆包；生成 docs/testing/monster-motion 证据"]
fn real_monster_walk_advance_and_evidence() {
    let workspace = workspace_root();
    let root = workspace.join("game-assets");
    let frames_path = root.join("gfx/sprites/monster-native/monster-184/frames.json");
    assert!(
        frames_path.is_file(),
        "R49 证据需要 game-assets 原生怪物拆包：缺 {}（本测试带 #[ignore]，缺素材必须失败而不是空过）",
        frames_path.display()
    );
    let map = mir_asset::LibraryMap::load(root.join("library-map.json")).expect("dev library map");
    let libraries = EntityLibraries::open_with_sources(
        root.join("data"),
        Some(&root.join("gfx")),
        Some(&root),
        Some(&map),
        None,
        false,
    );
    let mut scene = MapScene::load_with_sources(
        root.join("maps/0.mnm"),
        root.join("data"),
        Some(&root.join("gfx")),
        Some(&root),
        Some(&map),
        None,
        false,
    )
    .expect("default dev map/scene");

    let mut snapshot = WorldSnapshot::default();
    for image in [5u16, 22, 184] {
        let key = format!("monster-{image:03}");
        let frames = libraries
            .monster_frames(&key)
            .unwrap_or_else(|| panic!("生产加载器应能读取 {key}/frames.json"));
        snapshot.monster_frames.insert(image, (*frames).clone());
    }
    // 005/022 的真实 Walk 都是 6×100ms；184 是 10×100ms。用不同 Count 证明
    // 每怪时长不是同一个 600ms 常量。
    assert_eq!(snapshot.monster_frames[&5].get(1).unwrap().count, 6);
    assert_eq!(snapshot.monster_frames[&22].get(1).unwrap().count, 6);
    assert_eq!(snapshot.monster_frames[&184].get(1).unwrap().count, 10);

    let started = 1_795_000_000_000u64;
    for (image, from, direction) in [
        (5u16, Point::new(330, 270), MirDirection::Right),
        (22, Point::new(334, 270), MirDirection::Down),
        (184, Point::new(338, 270), MirDirection::Left),
    ] {
        apply_walk(&mut snapshot, image, from, direction, started);
    }

    let out_dir = workspace.join("docs/testing/monster-motion");
    std::fs::create_dir_all(&out_dir).expect("create evidence dir");
    for name in [
        "frame0.png",
        "frame5.png",
        "frame6_005_600ms.png",
        "frame10_184_1000ms.png",
        "self_moving.png",
        "landed.png",
    ] {
        let path = out_dir.join(name);
        if path.is_file() {
            std::fs::remove_file(path).expect("remove stale png");
        }
    }

    let mut samples = Vec::new();
    for (label, elapsed, center) in [
        ("frame0", 0u64, Point::new(334, 270)),
        ("frame5", 500, Point::new(334, 270)),
        ("frame6_005_600ms", 600, Point::new(334, 270)),
        ("frame10_184_1000ms", 1_000, Point::new(334, 270)),
    ] {
        let now = started + elapsed;
        let views = snapshot.entity_views(now);
        let right = views.iter().find(|view| view.image == 5).expect("005 view");
        let down = views
            .iter()
            .find(|view| view.image == 22)
            .expect("022 view");
        let left = views
            .iter()
            .find(|view| view.image == 184)
            .expect("184 view");

        match elapsed {
            0 => {
                assert_eq!(right.pixel_offset, (8, 0));
                assert_eq!(down.pixel_offset, (0, 6));
                assert_eq!(left.pixel_offset, (-4, 0));
            }
            500 => {
                assert_eq!(right.pixel_offset, (48, 0));
                assert_eq!(down.pixel_offset, (0, 32));
                assert_eq!(left.pixel_offset, (-28, 0));
            }
            600 => {
                // 005/022 的 6×100ms 已到期；184 的 10×100ms 仍在第 6 帧。
                assert_eq!(right.action, ActorAction::Standing);
                assert_eq!(right.position, Point::new(331, 270));
                assert_eq!(right.pixel_offset, (0, 0));
                assert_eq!(down.action, ActorAction::Standing);
                assert_eq!(down.position, Point::new(334, 271));
                assert_eq!(left.action, ActorAction::Walking);
                assert_eq!(left.position, Point::new(338, 270));
                assert_eq!(left.pixel_offset, (-34, 0));
            }
            1_000 => {
                assert_eq!(left.action, ActorAction::Standing);
                assert_eq!(left.position, Point::new(337, 270));
                assert_eq!(left.pixel_offset, (0, 0));
            }
            _ => unreachable!(),
        }

        // 屏幕连续性：005 的 15848→15888（0→500ms）→15888（600ms 落点），
        // 184 的 16220→16200→16190→16176（1000ms 落点），都没有回跳。
        if elapsed <= 500 {
            let screen = right.position.x * 48 + right.pixel_offset.0;
            assert!((15_848..=15_888).contains(&screen), "005 screen_x={screen}");
        }

        let mut frame = RenderFrame::new(800, 600);
        scene.render_with_entities_at(&mut frame, center, &views, now);
        let name = format!("{label}.png");
        let path = out_dir.join(&name);
        frame.save_png(&path).expect("write evidence png");
        let walk_frame = |image: u16, direction: MirDirection| {
            let frame = snapshot.monster_frames[&image]
                .get(1)
                .expect("Walk 帧表记录");
            let range = frame
                .frame_indices(direction as usize)
                .expect("Walk 方向范围");
            let motion = snapshot.monster_motions.get(&(u32::from(image) + 1_000));
            json!({
                "start": frame.start,
                "count": frame.count,
                "skip": frame.skip,
                "interval_ms": frame.interval,
                "direction_range": [range.start, range.end],
                "frame_index": motion.map(|motion| motion.frame_index(now)),
            })
        };
        samples.push(json!({
            "name": label,
            "elapsed_ms": elapsed,
            "simulated_now_ms": now,
            "wall_captured_ms": now_millis_u64(),
            "camera_center": [center.x, center.y],
            "monsters": [
                {"image": 5, "action": format!("{:?}", right.action),
                 "from": [330, 270], "to": [331, 270], "direction": "Right",
                 "position": [right.position.x, right.position.y],
                 "pixel_offset": [right.pixel_offset.0, right.pixel_offset.1],
                 "walk": {"count": 6, "interval_ms": 100, "duration_ms": 600,
                          "frame": walk_frame(5, MirDirection::Right)}},
                {"image": 22, "action": format!("{:?}", down.action),
                 "from": [334, 270], "to": [334, 271], "direction": "Down",
                 "position": [down.position.x, down.position.y],
                 "pixel_offset": [down.pixel_offset.0, down.pixel_offset.1],
                 "walk": {"count": 6, "interval_ms": 100, "duration_ms": 600,
                          "frame": walk_frame(22, MirDirection::Down)}},
                {"image": 184, "action": format!("{:?}", left.action),
                 "from": [338, 270], "to": [337, 270], "direction": "Left",
                 "position": [left.position.x, left.position.y],
                 "pixel_offset": [left.pixel_offset.0, left.pixel_offset.1],
                 "walk": {"count": 10, "interval_ms": 100, "duration_ms": 1000,
                          "frame": walk_frame(184, MirDirection::Left)}}
            ],
            "png": format!("docs/testing/monster-motion/{name}"),
        }));
    }

    let generated = now_millis_u64();
    let manifest = json!({
        "test": "real_monster_walk_advance_and_evidence",
        "batch": iso_utc(generated),
        "generated_at_unix_ms": generated,
        "terrain_source": "game-assets/maps/0.mnm + library-map.json + gfx",
        "frame_source": "EntityLibraries::monster_frames (生产 frames.json 解析器)",
        "crystal_reference": "github/Crystal Client/MirObjects/MonsterObject.cs:384-437",
        "formula": "Movement=CurrentLocation-1/2格; pixel_offset=-OffSetMove; OffSetMove 8方向 48/32 每格并做 %2 偶数化",
        "data_note": "005 与 022 的真实 Walk 都是 6×100ms（仅图像 offset 不同），所以另加 184（10×100ms）证明每怪时长来自各自 Count×Interval；这不是参数未接入。",
        "ticket": "R49 t48 (audit t44 X1-01..X1-04)",
        "findings": {
            "X1-01": "renderer/frames 保留有符号 Skip；010/041/043/062 Standing 像素见 stride-zero-render.json",
            "X1-02": "实际时长用 Count×Interval；184 Walk 10×100=1000ms、Attack 8×100=800ms",
            "X1-03": "ConnectOptions→initial_world_snapshot→NetClient 首帧快照带 frames/params；每怪按 image 取自己的表",
            "X1-04": "pixel_offset=-OffSetMove；Right 8→48→落点 0，无 96px 跳变"
        },
        "limitations": [
            "005/022 的 frames.json 动作数值完全相同（Walk 6×100），无法用真实数据证明两者参数不同；用 184 的 10×100 证明按怪生效。",
            "010/041/043/062 的 frames.json 没有 Walking/Running 动作；Crystal Frame==null 时安静跳过，故这两动作像素为 0 属数据限制，见 stride-zero-render.json。",
            "证据是软件光栅渲染；未启动 GUI 做真人视觉验收。"
        ],
        "samples": samples,
    });
    std::fs::write(
        out_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("manifest json"),
    )
    .expect("write manifest");
    println!(
        "monster-motion evidence: {} batch={}",
        out_dir.display(),
        iso_utc(generated)
    );
}
