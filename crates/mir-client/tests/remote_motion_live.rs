//! R46 live 证据：两个真实 `NetClient` + `MemoryStore` 服务端。
//!
//! A 真实走/跑，B 收到 `S.ObjectWalk`/`S.ObjectRun` 并验证 A 的权威落点；
//! B 用生产渲染前处理（`camera::prepare_frame`）逐帧采样 A 的远端插值，
//! 用真实 `game-assets` 地形写 PNG + manifest。随后 B 也走一步，作为
//! 「观察者自己也移动」的合成证据。
//!
//! 注意：本测试会写 `build/evidence/remote-motion/`（gitignore 的临时证据目录）；
//! 最终门禁后会重新冻结。窗口四类/FPS 不在本测试范围。

use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mir_client::camera::{self, RemoteMotion, SelfMotion};
use mir_client::net::{ConnectOptions, CreateCharacter, NetClient, NetCommand, WorldSnapshot};
use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_render::{ActorAction, EntityKind, Frame, MapScene, TileLibraries, Viewport, geometry};
use mir_server::{MemoryStore, ServerConfig, serve_forever};
use mir_world::{Map, MapCell, MapVersion};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(15);
const MOVE_TIMEOUT: Duration = Duration::from_secs(3);
const MOVE_DELAY_PAUSE: Duration = Duration::from_millis(650);
const RETRY_PAUSE: Duration = Duration::from_millis(220);

const DIRECTIONS: [MirDirection; 8] = [
    MirDirection::Down,
    MirDirection::DownRight,
    MirDirection::Right,
    MirDirection::UpRight,
    MirDirection::Up,
    MirDirection::UpLeft,
    MirDirection::Left,
    MirDirection::DownLeft,
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

fn synthetic_map() -> Map {
    let width = 64;
    let height = 64;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "synthetic".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
}

fn start_server(root: &Path) -> (tokio::runtime::Runtime, SocketAddr) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("创建 tokio runtime");
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取临时端口");
    let config = Arc::new(ServerConfig {
        listen: address,
        map_root: root.join("resource/map").display().to_string(),
        language_dir: root.join("config/lang").display().to_string(),
        load_game_data: false,
        demo_monsters: true,
        ..ServerConfig::default()
    });
    let store = Arc::new(MemoryStore::new());
    runtime.spawn(serve_forever(listener, config, store));
    (runtime, address)
}

fn login(address: SocketAddr, account: &str, character: &str) -> (NetClient, WorldSnapshot) {
    let client = NetClient::connect(ConnectOptions {
        server: address.to_string(),
        account: account.to_owned(),
        password: "livepass".to_owned(),
        character: character.to_owned(),
        create: Some(CreateCharacter {
            name: character.to_owned(),
            class: MirClass::Warrior,
            gender: MirGender::Male,
        }),
        auto_register: true,
        manual_select: false,
        monster_frames: std::collections::BTreeMap::new(),
    })
    .expect("启动客户端网络线程");
    let snapshot = client
        .wait_until_in_game(LOGIN_TIMEOUT)
        .expect("应进入世界");
    (client, snapshot)
}

fn load_evidence_scene(
    root: &Path,
    map_name: &str,
    fallback_data_dir: &Path,
) -> (MapScene, String) {
    let asset_root = root.join("game-assets");
    let mnm = asset_root.join("maps").join(format!("{map_name}.mnm"));
    let library_map = mir_asset::LibraryMap::load(asset_root.join("library-map.json")).ok();
    if mnm.is_file() && library_map.is_some() {
        let gfx = asset_root.join("gfx");
        match MapScene::load_with_sources(
            &mnm,
            asset_root.join("data"),
            Some(gfx.as_path()),
            Some(asset_root.as_path()),
            library_map.as_ref(),
            None,
            false,
        ) {
            Ok(scene) => {
                return (
                    scene,
                    format!("game-assets/maps/{map_name}.mnm（library-map.json + gfx）"),
                );
            }
            Err(error) => eprintln!("game-assets 加载失败，回退 resource/：{error}"),
        }
    }
    match MapScene::load(
        root.join("resource/map").join(format!("{map_name}.map")),
        fallback_data_dir,
    ) {
        Ok(scene) => (
            scene,
            format!("resource/map/{map_name}.map + resource/data"),
        ),
        Err(error) => {
            eprintln!("没有可用地形，改用合成地图：{error}");
            (
                MapScene::from_parts(
                    synthetic_map(),
                    TileLibraries::empty("."),
                    Viewport::DEFAULT,
                ),
                "synthetic（无地形素材）".to_owned(),
            )
        }
    }
}

fn direction_candidates(preferred: MirDirection) -> Vec<MirDirection> {
    let mut candidates = vec![preferred];
    candidates.extend(
        DIRECTIONS
            .iter()
            .copied()
            .filter(|direction| *direction != preferred),
    );
    candidates
}

struct PendingRemote {
    motion: RemoteMotion,
    snapshot: WorldSnapshot,
    action: &'static str,
    step: u32,
}

struct Sample {
    action: &'static str,
    frame_index: u32,
    elapsed_ms: u64,
    simulated_now_ms: u64,
    wall_captured_ms: u64,
    camera_center: Point,
    global_offset: (i32, i32),
    observer_from: Point,
    observer_to: Point,
    observer_pixel_offset: (i32, i32),
    observer_screen: (i32, i32),
    remote_from: Option<Point>,
    remote_to: Option<Point>,
    remote_pixel_offset: Option<(i32, i32)>,
    remote_screen: Option<(i32, i32)>,
    png: String,
    terrain_layer_identical: bool,
}

#[allow(clippy::too_many_arguments)]
fn capture_remote(
    scene: &mut MapScene,
    snapshot: &WorldSnapshot,
    motion: RemoteMotion,
    action: &'static str,
    step: u32,
    observer_id: u32,
    target_id: u32,
    target_name: &str,
    out_dir: &Path,
    root: &Path,
) -> Vec<Sample> {
    let mut samples = Vec::new();
    let mut first_terrain: Option<Vec<u8>> = None;
    for frame_index in 0..=6_u32 {
        let elapsed = u64::from(frame_index) * 100;
        let now = motion.started_ms + elapsed;
        let mut snapshot = snapshot.clone();
        let mut self_motion: Option<SelfMotion> = None;
        let prepared =
            camera::prepare_frame(&mut snapshot, &mut self_motion, Point::new(0, 0), now);
        let mut frame = Frame::new(800, 600);
        scene.render_with_entities_at_offset(
            &mut frame,
            prepared.center,
            prepared.offset,
            &prepared.entities,
            now,
        );
        let self_view = prepared
            .entities
            .iter()
            .find(|view| view.name.as_deref() == Some("Observer"))
            .or_else(|| {
                prepared
                    .entities
                    .iter()
                    .find(|view| view.kind == EntityKind::Player)
            })
            .expect("观察者自己的实体");
        let remote_view = prepared
            .entities
            .iter()
            .find(|view| view.name.as_deref() == Some(target_name))
            .expect("目标玩家实体");
        let (self_x, self_y) =
            geometry::world_to_screen(Viewport::DEFAULT, prepared.center, self_view.position);
        let self_screen = (
            self_x + prepared.offset.0 + self_view.pixel_offset.0,
            self_y + prepared.offset.1 + self_view.pixel_offset.1,
        );
        let (remote_x, remote_y) =
            geometry::world_to_screen(Viewport::DEFAULT, prepared.center, remote_view.position);
        let remote_screen = (
            remote_x + prepared.offset.0 + remote_view.pixel_offset.0,
            remote_y + prepared.offset.1 + remote_view.pixel_offset.1,
        );
        // 地形层单独渲染（无实体），只比较地形本身：并行运行时有怪物/其他实体
        // 进入裁剪区也不会造成 flake；远端 pixel_offset 若污染地形会立刻暴露。
        let mut terrain_frame = Frame::new(800, 600);
        scene.render_offset(&mut terrain_frame, prepared.center, prepared.offset);
        let terrain = terrain_frame.pixels.clone();
        let terrain_ok = first_terrain.as_ref().is_none_or(|first| first == &terrain);
        if first_terrain.is_none() {
            first_terrain = Some(terrain);
        }
        let name = format!("{action}-step{step}-frame{frame_index}.png");
        let path = out_dir.join(&name);
        frame.save_png(&path).expect("写 PNG");
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        samples.push(Sample {
            action,
            frame_index,
            elapsed_ms: elapsed,
            simulated_now_ms: now,
            wall_captured_ms: now_millis(),
            camera_center: prepared.center,
            global_offset: prepared.offset,
            observer_from: self_view.position,
            observer_to: self_view.position,
            observer_pixel_offset: self_view.pixel_offset,
            observer_screen: self_screen,
            remote_from: Some(remote_view.position),
            remote_to: Some(motion.to),
            remote_pixel_offset: Some(remote_view.pixel_offset),
            remote_screen: Some(remote_screen),
            png: relative,
            terrain_layer_identical: terrain_ok,
        });
        assert_eq!(
            self_screen,
            Viewport::DEFAULT.center_origin(),
            "观察者自机锚点"
        );
        assert_eq!(prepared.entities[0].kind, EntityKind::Player);
        if elapsed < 600 {
            assert_eq!(remote_view.position, motion.from, "远端绘制在 from");
            assert_eq!(
                remote_view.pixel_offset,
                camera::self_motion_offset(motion.from, motion.to, elapsed)
            );
            assert_eq!(
                remote_screen,
                (
                    geometry::world_to_screen(Viewport::DEFAULT, prepared.center, motion.from).0
                        + prepared.offset.0
                        + remote_view.pixel_offset.0,
                    geometry::world_to_screen(Viewport::DEFAULT, prepared.center, motion.from).1
                        + prepared.offset.1
                        + remote_view.pixel_offset.1
                )
            );
        } else {
            assert_eq!(remote_view.position, motion.to, "600ms 落终点");
            assert_eq!(remote_view.pixel_offset, (0, 0));
        }
        let _ = (observer_id, target_id);
    }
    assert!(
        samples.iter().all(|sample| sample.terrain_layer_identical),
        "地形层（无实体）应逐帧一致；并行实体不参与该断言"
    );
    samples
}

/// R49-X1-03 说明：本测试需要**两个真实联机客户端**（同一进程内起真实服务端 +
/// 两条 `NetClient` 连接 + 可用回环端口），并依赖本机真实 `game-assets`，属于
/// 双客户端插值门禁（R46 语义）。全量门禁默认不跑它（`#[ignore]`），需要单独
/// 显式执行以保留该门禁的证据：
/// `cargo test -p mir-client --test remote_motion_live -- --ignored`
#[test]
#[ignore = "需要两个真实联机客户端与可用回环端口（自起真实服务端，回环 TCP）；全量门禁默认不跑，显式 --ignored 单独执行"]
fn two_clients_observe_remote_walk_run_interpolation() {
    let root = workspace_root();
    let (runtime, address) = start_server(&root);
    let (a_client, mut a_snapshot) = login(address, "rmtobsA", "TargetHero");
    let (b_client, mut b_snapshot) = login(address, "rmtobsB", "Observer");
    let target_id = a_snapshot.self_player.as_ref().expect("A self").object_id;
    let observer_id = b_snapshot.self_player.as_ref().expect("B self").object_id;
    let target_name = a_snapshot
        .self_player
        .as_ref()
        .expect("A self")
        .name
        .clone();
    b_snapshot = b_client
        .wait_for_snapshot(
            |snapshot| snapshot.entities.contains_key(&target_id),
            Duration::from_secs(10),
        )
        .expect("B 应看到 A 的 ObjectPlayer");
    let map_name = b_snapshot
        .map
        .as_ref()
        .map_or_else(|| "0".to_owned(), |map| map.file_name.clone());
    let (mut scene, terrain_source) =
        load_evidence_scene(&root, &map_name, &root.join("resource/data"));
    let out_dir = root.join("build/evidence/remote-motion");
    fs::create_dir_all(&out_dir).expect("创建证据目录");

    let mut samples = Vec::new();
    let mut pending: Vec<PendingRemote> = Vec::new();
    let mut steps = Vec::new();
    let mut a_pos = a_snapshot.self_player.as_ref().expect("A self").position;
    let mut last_direction = MirDirection::Down;
    let mut walk_count = 0_u32;
    for step in 0..2_u32 {
        if step > 0 {
            std::thread::sleep(MOVE_DELAY_PAUSE);
        }
        let mut moved = false;
        for direction in direction_candidates(last_direction) {
            let before_a = a_snapshot.packet_count("UserLocation");
            let before_b = b_snapshot.packet_count("ObjectWalk");
            if !a_client.send(NetCommand::Walk(direction)) {
                continue;
            }
            let Ok(next_a) = a_client.wait_for_snapshot(
                |snapshot| snapshot.packet_count("UserLocation") > before_a,
                MOVE_TIMEOUT,
            ) else {
                continue;
            };
            let Some(a_me) = next_a.self_player.as_ref() else {
                continue;
            };
            let to = a_me.position;
            if to == a_pos {
                a_snapshot = next_a;
                continue;
            }
            let Ok(next_b) = b_client.wait_for_snapshot(
                |snapshot| {
                    snapshot.packet_count("ObjectWalk") > before_b
                        && snapshot
                            .entities
                            .get(&target_id)
                            .is_some_and(|entity| entity.position == to)
                },
                MOVE_TIMEOUT,
            ) else {
                a_snapshot = next_a;
                continue;
            };
            let motion = *next_b
                .remote_motions
                .get(&target_id)
                .expect("B 应建立 A 的远端 motion");
            assert_eq!(motion.action, ActorAction::Walking);
            assert_eq!(motion.from, a_pos);
            assert_eq!(motion.to, to);
            assert_eq!(
                next_b.entities[&target_id].action_started_ms, motion.started_ms,
                "远端动作与 motion 同钟"
            );
            pending.push(PendingRemote {
                motion,
                snapshot: next_b.clone(),
                action: "walk",
                step: step + 1,
            });
            steps.push(serde_json::json!({
                "action": "walk",
                "from": [a_pos.x, a_pos.y],
                "to": [to.x, to.y],
                "direction": format!("{direction:?}"),
                "a_user_location_before": before_a,
                "b_object_walk_before": before_b,
                "motion_started_ms": motion.started_ms,
                "wall_captured_ms": now_millis(),
            }));
            a_pos = to;
            a_snapshot = next_a;
            b_snapshot = next_b;
            last_direction = direction;
            walk_count += 1;
            moved = true;
            break;
        }
        assert!(moved, "A 第 {} 次走失败", step + 1);
    }

    // A 跑：先走一步置 step_counter > 0（上面已走），等 600ms 后 C.Run。
    std::thread::sleep(MOVE_DELAY_PAUSE);
    let mut run_moved = false;
    let mut a_motion_for_concurrent: Option<RemoteMotion> = None;
    let mut a_snapshot_for_concurrent: Option<WorldSnapshot> = None;
    for direction in direction_candidates(last_direction) {
        let before_a = a_snapshot.packet_count("UserLocation");
        let before_b = b_snapshot.packet_count("ObjectRun");
        if !a_client.send(NetCommand::Run(direction)) {
            continue;
        }
        let Ok(next_a) = a_client.wait_for_snapshot(
            |snapshot| snapshot.packet_count("UserLocation") > before_a,
            MOVE_TIMEOUT,
        ) else {
            continue;
        };
        let Some(a_me) = next_a.self_player.as_ref() else {
            continue;
        };
        let to = a_me.position;
        if to == a_pos {
            a_snapshot = next_a;
            std::thread::sleep(RETRY_PAUSE);
            continue;
        }
        let Ok(next_b) = b_client.wait_for_snapshot(
            |snapshot| {
                snapshot.packet_count("ObjectRun") > before_b
                    && snapshot
                        .entities
                        .get(&target_id)
                        .is_some_and(|entity| entity.position == to)
            },
            MOVE_TIMEOUT,
        ) else {
            a_snapshot = next_a;
            continue;
        };
        let motion = *next_b
            .remote_motions
            .get(&target_id)
            .expect("B 应建立 A 的跑步远端 motion");
        assert_eq!(motion.action, ActorAction::Running);
        assert_eq!(motion.from, a_pos);
        assert_eq!(motion.to, to);
        pending.push(PendingRemote {
            motion,
            snapshot: next_b.clone(),
            action: "run",
            step: 1,
        });
        steps.push(serde_json::json!({
            "action": "run",
            "from": [a_pos.x, a_pos.y],
            "to": [to.x, to.y],
            "direction": format!("{direction:?}"),
            "a_user_location_before": before_a,
            "b_object_run_before": before_b,
            "motion_started_ms": motion.started_ms,
            "wall_captured_ms": now_millis(),
        }));
        a_motion_for_concurrent = Some(motion);
        a_snapshot_for_concurrent = Some(next_b.clone());
        b_snapshot = next_b;
        run_moved = true;
        break;
    }
    assert!(run_moved, "A 跑失败");

    // B 也移动：不再等 A 到期，而是在 A 的 RemoteMotion 仍活动时让 B 走一步；
    // 同一采样帧同时验证 observer（B）self anchor/global offset 和 remote（A）screen。
    let a_motion = a_motion_for_concurrent.expect("A run motion");
    let a_snapshot = a_snapshot_for_concurrent.expect("A run snapshot");
    assert!(
        a_snapshot.remote_motions.contains_key(&target_id),
        "A run snapshot 应保留 A 的远端 motion"
    );
    let mut b_motion: Option<SelfMotion> = None;
    let mut b_self_snapshot = b_snapshot.clone();
    for direction in direction_candidates(MirDirection::Right) {
        let before = b_snapshot.packet_count("UserLocation");
        if !b_client.send(NetCommand::Walk(direction)) {
            continue;
        }
        let Ok(next_b) = b_client.wait_for_snapshot(
            |snapshot| snapshot.packet_count("UserLocation") > before,
            MOVE_TIMEOUT,
        ) else {
            continue;
        };
        let Some(me) = next_b.self_player.as_ref() else {
            continue;
        };
        if me.position == b_snapshot.self_player.as_ref().expect("B self").position {
            b_snapshot = next_b;
            continue;
        }
        let from = b_snapshot.self_player.as_ref().expect("B self").position;
        let to = me.position;
        let motion = match camera::motion_after_authoritative_step(&next_b, from, to) {
            camera::StepMotion::Started(motion) => motion,
            other => panic!("B 自机走一步应 Started，实际 {other:?}"),
        };
        b_motion = Some(motion);
        b_self_snapshot = next_b;
        break;
    }
    let b_motion = b_motion.expect("B 应能走一步");
    assert!(
        b_self_snapshot.remote_motions.contains_key(&target_id),
        "B 自己移动后的快照里 A 的 motion 仍应活动（并发证据）"
    );
    let sample_now = (b_motion.started_ms + 300).min(a_motion.started_ms + 500);
    assert!(
        sample_now >= b_motion.started_ms,
        "B 启动过晚，无法在 A 的 600ms 窗口内采样"
    );
    let a_elapsed = sample_now.saturating_sub(a_motion.started_ms);
    let b_elapsed = sample_now.saturating_sub(b_motion.started_ms);
    assert!(
        a_elapsed < 600,
        "B 移动采样时 A 的 motion 必须仍活动（a_elapsed={a_elapsed}）"
    );
    assert!(b_elapsed < 600, "B 自身 motion 必须仍活动");
    let mut self_motion = Some(b_motion);
    let prepared = camera::prepare_frame(
        &mut b_self_snapshot,
        &mut self_motion,
        Point::new(0, 0),
        sample_now,
    );
    let self_view = prepared
        .entities
        .iter()
        .find(|view| view.name.as_deref() == Some("Observer"))
        .expect("B self");
    let remote_view = prepared
        .entities
        .iter()
        .find(|view| view.name.as_deref() == Some(&target_name))
        .expect("A remote");
    let (sx, sy) =
        geometry::world_to_screen(Viewport::DEFAULT, prepared.center, self_view.position);
    let observer_screen = (
        sx + prepared.offset.0 + self_view.pixel_offset.0,
        sy + prepared.offset.1 + self_view.pixel_offset.1,
    );
    assert_eq!(
        observer_screen,
        Viewport::DEFAULT.center_origin(),
        "B 移动时自机仍居中"
    );
    assert_eq!(
        prepared.offset,
        camera::world_offset(b_motion.from, b_motion.to, b_elapsed),
        "B 移动时全局 offset 只来自 B 自己的 SelfMotion"
    );
    assert_eq!(self_view.position, b_motion.from);
    assert_eq!(
        self_view.pixel_offset,
        camera::self_motion_offset(b_motion.from, b_motion.to, b_elapsed)
    );
    assert_eq!(remote_view.position, a_motion.from, "A 仍绘制在 from");
    assert_eq!(
        remote_view.pixel_offset,
        a_motion.advance_at(sample_now),
        "A 的远端偏移只来自 A 的 RemoteMotion"
    );
    let (rx, ry) =
        geometry::world_to_screen(Viewport::DEFAULT, prepared.center, remote_view.position);
    let remote_screen = (
        rx + prepared.offset.0 + remote_view.pixel_offset.0,
        ry + prepared.offset.1 + remote_view.pixel_offset.1,
    );
    let mut frame = Frame::new(800, 600);
    scene.render_with_entities_at_offset(
        &mut frame,
        prepared.center,
        prepared.offset,
        &prepared.entities,
        sample_now,
    );
    let path = out_dir.join("concurrent-observer-remote-frame0.png");
    frame.save_png(&path).expect("写 B 自移动并发 PNG");
    samples.push(Sample {
        action: "observer",
        frame_index: 0,
        elapsed_ms: b_elapsed,
        simulated_now_ms: sample_now,
        wall_captured_ms: now_millis(),
        camera_center: prepared.center,
        global_offset: prepared.offset,
        observer_from: b_motion.from,
        observer_to: b_motion.to,
        observer_pixel_offset: self_view.pixel_offset,
        observer_screen,
        remote_from: Some(remote_view.position),
        remote_to: Some(a_motion.to),
        remote_pixel_offset: Some(remote_view.pixel_offset),
        remote_screen: Some(remote_screen),
        png: path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .display()
            .to_string(),
        terrain_layer_identical: true,
    });

    // 联机时序全部结束后再渲染/落 PNG，避免 PNG 写盘拖过服务端 600ms 动作窗口
    // （否则 A 的 step_counter 会被 idle reset，跑步发不出去）。
    for item in &pending {
        samples.extend(capture_remote(
            &mut scene,
            &item.snapshot,
            item.motion,
            item.action,
            item.step,
            observer_id,
            target_id,
            &target_name,
            &out_dir,
            &root,
        ));
    }

    let manifest = serde_json::json!({
        "test": "two_clients_observe_remote_walk_run_interpolation",
        "generated_at_unix_ms": now_millis(),
        "server": {"store": "MemoryStore", "address": address.to_string(), "demo_monsters": true},
        "map": map_name,
        "terrain_source": terrain_source,
        "viewport": [800, 600],
        "crystal_reference": "github/Crystal HEAD 0e315fe327192afe52c3d7357ddd1f5b7e26c5b8",
        "baseline": "85bfc21",
        "observer": {"object_id": observer_id, "name": "Observer"},
        "target": {"object_id": target_id, "name": target_name},
        "sampling": "模拟采样 now = 该 ObjectWalk/ObjectRun 应用时刻 + k×100ms；wall_captured_ms 为实际墙钟，二者分列",
        "steps": steps,
        "samples": samples.iter().map(|sample| serde_json::json!({
            "action": sample.action,
            "frame_index": sample.frame_index,
            "elapsed_ms": sample.elapsed_ms,
            "simulated_now_ms": sample.simulated_now_ms,
            "wall_captured_ms": sample.wall_captured_ms,
            "camera_center": [sample.camera_center.x, sample.camera_center.y],
            "global_offset": [sample.global_offset.0, sample.global_offset.1],
            "observer_from": [sample.observer_from.x, sample.observer_from.y],
            "observer_to": [sample.observer_to.x, sample.observer_to.y],
            "observer_pixel_offset": [sample.observer_pixel_offset.0, sample.observer_pixel_offset.1],
            "observer_screen": [sample.observer_screen.0, sample.observer_screen.1],
            "remote_from": sample.remote_from.map(|point| serde_json::json!([point.x, point.y])),
            "remote_to": sample.remote_to.map(|point| serde_json::json!([point.x, point.y])),
            "remote_pixel_offset": sample.remote_pixel_offset.map(|offset| serde_json::json!([offset.0, offset.1])),
            "remote_screen": sample.remote_screen.map(|screen| serde_json::json!([screen.0, screen.1])),
            "png": sample.png,
            "terrain_layer_identical": sample.terrain_layer_identical,
        })).collect::<Vec<_>>(),
    });
    let manifest_path = out_dir.join("manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).expect("manifest JSON"),
    )
    .expect("写 manifest");
    assert!(walk_count >= 2);
    assert_eq!(samples.iter().filter(|s| s.action == "run").count(), 7);
    assert_eq!(samples.iter().filter(|s| s.action == "walk").count(), 14);
    println!(
        "PASS remote-motion live：A id={target_id}，B id={observer_id}；walk={walk_count} run=1；{} 样本；manifest {}",
        samples.len(),
        manifest_path.display()
    );
    drop(a_client);
    drop(b_client);
    drop(runtime);
}
