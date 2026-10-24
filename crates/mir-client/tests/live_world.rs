//! 端到端联机测试：真实 `mir-server` + 无窗口客户端网络层 + 连续多帧渲染。
//!
//! 两条用例：
//!
//! 1. `live_client_walks_and_renders_a_frame`：登录 → 走路 → 一帧 PNG（基础冒烟）；
//! 2. `live_client_walk_run_capture_continuous_frames`：真实登录链路下连续 2 次走 +
//!    1 次跑，逐帧（6 × 100ms）走**生产渲染入口**
//!    `MapScene::render_with_entities_at_offset`，断言服务端权威坐标、方向、自机
//!    屏幕锚点、相机/地形偏移，并把 PNG 与 `evidence-manifest.json` 写到
//!    `build/evidence/self-motion/`。
//!
//! 跑步按服务端正常条件触发：先走一步把 `step_counter` 置正、等 600ms 动作结束，
//! 再发 `C.Run`；不使用 GM/直接改服务端状态。
//!
//! 所有回包等待都有超时，不使用固定 sleep 等回包；只有「等走/跑 600ms 动作结束」
//! 与「避开拒绝回包 200ms 节流」两处按时钟等待。

use std::collections::HashSet;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mir_client::camera::{self, MOTION_FRAMES, MOTION_INTERVAL_MS, SelfMotion};
use mir_client::net::{ConnectOptions, CreateCharacter, NetClient, NetCommand, WorldSnapshot};
use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_render::{ActorAction, EntityKind, Frame, MapScene, TileLibraries, Viewport, geometry};
use mir_server::{MemoryStore, ServerConfig, serve_forever};
use mir_world::{Map, MapCell, MapVersion};

/// 登录链路等待上限。
const LOGIN_TIMEOUT: Duration = Duration::from_secs(15);
/// 单次走/跑回包等待上限。
const MOVE_TIMEOUT: Duration = Duration::from_secs(2);
/// 服务端走/跑动作 600ms（`world.rs` 的 `MOVE_DELAY_MS`）；等动作结束再发下一步。
const MOVE_DELAY_PAUSE: Duration = Duration::from_millis(650);
/// 移动被拒的 `S.UserLocation` 有 ≥200ms 节流；换方向重试前等一小会儿。
const RETRY_PAUSE: Duration = Duration::from_millis(220);
/// 候选方向（从出生点往外探测的顺序与其它用例一致）。
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

/// 工作区根目录（`CARGO_MANIFEST_DIR` 是 `crates/mir-client`）。
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// 无 `resource/` 时的合成地图：64×64 空地图，仍然能画实体占位方块。
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

/// 当前 Unix 毫秒（证据时间戳/帧号基准）。
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// 起一个真实服务端（`MemoryStore` + 演示怪物），返回运行时与监听地址。
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

/// 走完真实登录链路（自动注册 → 登录 → 建角 → `StartGame`）。
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
        .expect("客户端应走完登录链路进入世界");
    (client, snapshot)
}

/// 从 `start` 沿 `direction` 连续可走、且当前没有实体占据的格数（最多 `max_steps`）。
///
/// 同时排除带门字节的格子（门可能关着，服务端 `check_cell` 会拒）。
fn open_steps(
    map: Option<&Map>,
    occupied: &HashSet<(i32, i32)>,
    start: Point,
    direction: MirDirection,
    max_steps: i32,
) -> i32 {
    let mut steps = 0;
    for step in 1..=max_steps {
        let cell = start.moved(direction, step);
        if occupied.contains(&(cell.x, cell.y)) {
            break;
        }
        let map_ok = match map {
            Some(map) => {
                map.walkable(cell.x, cell.y)
                    && map
                        .cell(cell.x, cell.y)
                        .is_some_and(|map_cell| map_cell.door_index.is_none())
            }
            None => true,
        };
        if !map_ok {
            break;
        }
        steps = step;
    }
    steps
}

/// 快照里其他实体（怪物/NPC/其他玩家）占的格；自己不算。
fn occupied_cells(snapshot: &WorldSnapshot, me: Point) -> HashSet<(i32, i32)> {
    snapshot
        .entities
        .values()
        .map(|entity| (entity.position.x, entity.position.y))
        .filter(|cell| *cell != (me.x, me.y))
        .collect()
}

/// 优先用 `preferred`，再依次试其它方向。
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

/// 证据渲染用的地形：优先自有格式 `game-assets/`（`.mnm` + `library-map.json`），
/// 退回 `resource/`（老扁平素材），最后合成地图。返回场景与来源说明（写进清单）。
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
            Err(error) => eprintln!("game-assets 地形加载失败，回退 resource/：{error}"),
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
            eprintln!("没有可用的地图素材，改用合成地图：{error}");
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

/// 一步成功落格的服务端证据。
struct RecordedStep {
    action: &'static str,
    from: Point,
    to: Point,
    direction: MirDirection,
    user_location_before: u32,
    user_location_after: u32,
    received_ms: u64,
}

/// 等联机时序走完后补渲染的一次动作（快照/相机在收到落点时冻结）。
struct PendingCapture {
    snapshot: WorldSnapshot,
    motion: SelfMotion,
    action: &'static str,
    step: u32,
    direction: MirDirection,
}

/// 一个渲染时刻（帧）的证据。
struct EvidenceSample {
    action: &'static str,
    step: u32,
    frame_index: u32,
    elapsed_ms: u64,
    steps: i32,
    direction: MirDirection,
    from: Point,
    to: Point,
    camera_center: Point,
    world_offset: (i32, i32),
    self_offset: (i32, i32),
    self_screen: (i32, i32),
    screen_center: (i32, i32),
    terrain_anchor: (i32, i32),
    rendered_at_ms: u64,
    /// 实际渲染这一帧的墙钟时间（与 `rendered_at_ms` 的模拟采样时刻区分）。
    captured_at_wall_ms: u64,
    png: String,
}

/// 已渲染、待落盘的证据帧。
struct PendingFrame {
    sample: EvidenceSample,
    frame: Frame,
}

/// 证据写入器：持有 `MapScene`，逐帧调用生产渲染入口；PNG 在联机时序结束后统一落盘
/// （写入 PNG 较慢，不能挤占「走完 600ms 后才能跑」的窗口）。
struct EvidenceWriter {
    scene: MapScene,
    root: PathBuf,
    evidence_dir: PathBuf,
    frames: Vec<PendingFrame>,
}

impl EvidenceWriter {
    fn new(root: &Path, scene: MapScene) -> Self {
        let evidence_dir = root.join("build/evidence/self-motion");
        fs::create_dir_all(&evidence_dir).expect("创建 build/evidence/self-motion");
        Self {
            scene,
            root: root.to_path_buf(),
            evidence_dir,
            frames: Vec::new(),
        }
    }

    /// 为一次走/跑渲染 6 帧（帧 k = 收到落点 + k×100ms），逐帧断言并暂存。
    fn capture(
        &mut self,
        snapshot: &WorldSnapshot,
        motion: &SelfMotion,
        action: &'static str,
        step: u32,
        direction: MirDirection,
    ) {
        let steps = (motion.to.x - motion.from.x)
            .abs()
            .max((motion.to.y - motion.from.y).abs());
        assert_eq!(
            action == "run",
            steps == camera::RUN_STEPS,
            "证据动作 {action} 与位移 {steps} 格不符"
        );
        let expected_actor = if steps == camera::RUN_STEPS {
            ActorAction::Running
        } else {
            ActorAction::Walking
        };
        let viewport = Viewport::new(800, 600);
        let first_index = self.frames.len();
        let mut previous_offset: Option<(i32, i32)> = None;
        let mut terrain_base: Option<((i32, i32), (i32, i32))> = None;
        for frame_index in 0..MOTION_FRAMES {
            let frame_u32 = u32::try_from(frame_index).expect("帧号在 0..6");
            let frame_elapsed = u64::from(frame_u32) * MOTION_INTERVAL_MS;
            let now = motion.started_ms + frame_elapsed;
            let mut entities = snapshot.entity_views(now);
            assert!(
                entities.len() >= 2,
                "至少应有自己 + 一只怪物，实际 {}",
                entities.len()
            );
            assert_eq!(entities[0].kind, EntityKind::Player, "entities[0] 是自机");
            assert_eq!(
                entities[0].action, expected_actor,
                "{action} 第 {frame_index} 帧自机动作应为 {expected_actor:?}"
            );
            // 生产函数：相机 + 自机抵消偏移（与 app.rs 的 render_current 同一入口）。
            let camera =
                camera::apply_camera(&mut entities, motion, now).expect("600ms 动作时间内应有相机");
            assert_eq!(camera.elapsed_ms, frame_elapsed);
            assert_eq!(
                camera.center, motion.from,
                "相机中心必须是本次移动的起点 Movement（若中心错移，self anchor 会同步错移而照样过）"
            );
            assert_eq!(
                camera.offset,
                camera::world_offset(motion.from, motion.to, frame_elapsed),
                "世界偏移 = Crystal OffSetMove"
            );
            assert_eq!(
                camera.self_offset,
                camera::self_motion_offset(motion.from, motion.to, frame_elapsed),
                "自机偏移 = 世界偏移的相反数"
            );
            let mut frame = Frame::new(viewport.width, viewport.height);
            self.scene.render_with_entities_at_offset(
                &mut frame,
                camera.center,
                camera.offset,
                &entities,
                now,
            );
            // 自机 = 格原点 + 世界偏移 + 自身偏移，必须恒为屏幕中心。
            let (origin_x, origin_y) = self
                .scene
                .world_to_screen(camera.center, entities[0].position);
            let self_screen = (
                origin_x + camera.offset.0 + entities[0].pixel_offset.0,
                origin_y + camera.offset.1 + entities[0].pixel_offset.1,
            );
            let screen_center = self.scene.viewport().center_origin();
            assert_eq!(self_screen, screen_center, "自机应始终在屏幕中心");
            // 地形锚点（同一格的瓦片左上角）吃同一个世界偏移，逐帧随偏移滚动。
            let (tile_x, tile_y) =
                geometry::tile_origin(self.scene.viewport(), camera.center, motion.from);
            let terrain_anchor = (tile_x + camera.offset.0, tile_y + camera.offset.1);
            if let Some((base_anchor, base_offset)) = terrain_base {
                assert_eq!(
                    (
                        terrain_anchor.0 - base_anchor.0,
                        terrain_anchor.1 - base_anchor.1
                    ),
                    (
                        camera.offset.0 - base_offset.0,
                        camera.offset.1 - base_offset.1
                    ),
                    "地形逐帧位移应等于世界偏移变化"
                );
            } else {
                terrain_base = Some((terrain_anchor, camera.offset));
            }
            if let Some(previous) = previous_offset {
                assert_ne!(camera.offset, previous, "世界偏移应逐帧推进，不能卡住");
            }
            previous_offset = Some(camera.offset);
            assert!(
                frame.distinct_color_count() >= 3,
                "证据帧不应是纯色（实际 {} 种颜色）",
                frame.distinct_color_count()
            );
            let name = format!("{action}-step{step}-frame{frame_index}.png");
            let path = self.evidence_dir.join(&name);
            let relative = path
                .strip_prefix(&self.root)
                .unwrap_or(&path)
                .display()
                .to_string();
            self.frames.push(PendingFrame {
                sample: EvidenceSample {
                    action,
                    step,
                    frame_index: frame_u32,
                    elapsed_ms: frame_elapsed,
                    steps,
                    direction,
                    from: motion.from,
                    to: motion.to,
                    camera_center: camera.center,
                    world_offset: camera.offset,
                    self_offset: camera.self_offset,
                    self_screen,
                    screen_center,
                    terrain_anchor,
                    rendered_at_ms: now,
                    captured_at_wall_ms: now_millis(),
                    png: relative,
                },
                frame,
            });
        }
        let last_index = first_index + usize::try_from(MOTION_FRAMES).expect("6 帧");
        let changed = self.frames[first_index]
            .frame
            .pixels
            .chunks_exact(4)
            .zip(self.frames[last_index - 1].frame.pixels.chunks_exact(4))
            .filter(|(left, right)| left != right)
            .count();
        assert!(
            changed > 100,
            "{action} 第 {step} 步首末帧应有明显像素变化，实际 {changed} px"
        );
    }

    /// 把暂存帧统一写成 PNG（联机时序结束后才做，避免拖过跑步窗口）。
    fn save_pngs(&self) -> usize {
        for pending in &self.frames {
            let path = self.root.join(&pending.sample.png);
            pending.frame.save_png(&path).expect("写出证据 PNG");
            assert!(
                fs::metadata(&path).is_ok_and(|metadata| metadata.len() > 1024),
                "PNG 不应为空文件：{}",
                path.display()
            );
        }
        self.frames.len()
    }

    /// 全部证据帧的样本描述。
    fn samples(&self) -> impl Iterator<Item = &EvidenceSample> {
        self.frames.iter().map(|pending| &pending.sample)
    }

    /// 写 `evidence-manifest.json`（时间戳/帧索引/坐标/偏移/文件）。
    fn write_manifest(
        &self,
        // (地图名, 地形素材来源, 服务端地址, 步骤记录) 由调用方给出，writer 不持有测试状态。
        map_name: &str,
        terrain_source: &str,
        address: SocketAddr,
        steps: &[RecordedStep],
    ) -> PathBuf {
        let samples: Vec<serde_json::Value> = self
            .samples()
            .map(|sample| {
                serde_json::json!({
                    "action": sample.action,
                    "step": sample.step,
                    "frame_index": sample.frame_index,
                    "elapsed_ms": sample.elapsed_ms,
                    "i": sample.steps,
                    "direction": format!("{:?}", sample.direction),
                    "from": [sample.from.x, sample.from.y],
                    "to": [sample.to.x, sample.to.y],
                    "camera_center": [sample.camera_center.x, sample.camera_center.y],
                    "world_offset": [sample.world_offset.0, sample.world_offset.1],
                    "self_offset": [sample.self_offset.0, sample.self_offset.1],
                    "self_screen": [sample.self_screen.0, sample.self_screen.1],
                    "screen_center": [sample.screen_center.0, sample.screen_center.1],
                    "terrain_anchor": [sample.terrain_anchor.0, sample.terrain_anchor.1],
                    "rendered_at_ms": sample.rendered_at_ms,
                    "captured_at_wall_ms": sample.captured_at_wall_ms,
                    "png": sample.png,
                })
            })
            .collect();
        let steps_json: Vec<serde_json::Value> = steps
            .iter()
            .map(|step| {
                serde_json::json!({
                    "action": step.action,
                    "from": [step.from.x, step.from.y],
                    "to": [step.to.x, step.to.y],
                    "direction": format!("{:?}", step.direction),
                    "user_location_before": step.user_location_before,
                    "user_location_after": step.user_location_after,
                    "received_at_ms": step.received_ms,
                })
            })
            .collect();
        let manifest = serde_json::json!({
            "test": "live_client_walk_run_capture_continuous_frames",
            "generated_at_unix_ms": now_millis(),
            "server": {
                "store": "MemoryStore",
                "address": address.to_string(),
                "demo_monsters": true,
            },
            "map": map_name,
            "terrain_source": terrain_source,
            "viewport": [800, 600],
            "cell": {"width": 48, "height": 32},
            "crystal_reference": "github/Crystal HEAD 0e315fe327192afe52c3d7357ddd1f5b7e26c5b8",
            "rust_baseline": "90b8188c7143bb4800d35abb65bd548a7f6d5f12",
            "sampling": "帧 0 = 本地收到该步 UserLocation 的时刻；帧 k = 帧0 + k×100ms；rendered_at_ms 是模拟采样时刻，captured_at_wall_ms 才是实际渲染墙钟",
            "evidence_nature": "权威回包快照冻结后的确定性时间重放（每步 6×100ms），不是窗口实时录屏/实时截图",
            "formula_proof": "整数偏移公式以 camera 模块单测为独立证明；live_world 的 offset 断言复用同一 helper，仅作生产路径回归",
            "frames_per_step": MOTION_FRAMES,
            "offset_contract": "world_offset = Crystal OffSetMove（走右为负）；self_offset = -world_offset；自机格原点恒为 Viewport::center_origin() = (384, 256)",
            "steps": steps_json,
            "samples": samples,
        });
        let path = self.evidence_dir.join("evidence-manifest.json");
        fs::write(
            &path,
            serde_json::to_string_pretty(&manifest).expect("序列化证据清单"),
        )
        .expect("写出 evidence-manifest.json");
        path
    }
}

#[test]
fn live_client_walks_and_renders_a_frame() {
    let root = workspace_root();
    let map_dir = root.join("resource/map");
    let data_dir = root.join("resource/data");

    // 1. 起真实服务端。地图目录用绝对路径，测试从哪个工作目录跑都一样。
    let (runtime, address) = start_server(&root);

    // 2. 登录 → 建角 → 进世界。
    let (client, mut snapshot) = login(address, "liveacct", "LiveHero");
    assert!(snapshot.is_ready(), "快照应标记为已进入世界");
    let me = snapshot
        .self_player
        .as_ref()
        .expect("自己的 UserInformation");
    assert_eq!(me.name, "LiveHero");
    assert!(me.object_id > 0, "对象 ID 必须存在");
    assert_eq!(
        snapshot.packet_count("UserInformation"),
        1,
        "应收到一次自己的 UserInformation"
    );
    assert!(
        snapshot.packet_count("MapInformation") >= 1,
        "应收到 MapInformation"
    );
    assert!(
        snapshot
            .map
            .as_ref()
            .is_some_and(|map| !map.file_name.is_empty()),
        "MapInformation 应带地图名"
    );
    let monster_count = snapshot
        .entities
        .values()
        .filter(|entity| entity.kind == EntityKind::Monster)
        .count();
    assert!(
        monster_count >= 1,
        "出生点附近应至少看到一只演示怪物，实际实体：{:?}",
        snapshot.entities
    );
    assert!(
        snapshot.packet_count("ObjectMonster") >= 1,
        "应收到 ObjectMonster"
    );

    // 3. 走路：依次尝试八个方向，至少一个方向能移动；每一步都必须回 UserLocation。
    let mut walked = false;
    for direction in DIRECTIONS {
        let before_position = snapshot.self_player.as_ref().map(|player| player.position);
        let before_packets = snapshot.packet_count("UserLocation");
        assert!(client.send(NetCommand::Walk(direction)), "网络线程应存活");
        let Ok(next) = client.wait_for_snapshot(
            |snapshot| snapshot.packet_count("UserLocation") > before_packets,
            MOVE_TIMEOUT,
        ) else {
            continue;
        };
        if next.self_player.as_ref().map(|player| player.position) != before_position {
            snapshot = next;
            walked = true;
            break;
        }
        snapshot = next;
    }
    assert!(
        walked,
        "八个方向里至少应有一个能走（UserLocation 坐标变化）"
    );
    assert!(
        snapshot.packet_count("UserLocation") >= 1,
        "进世界后走路应收到 UserLocation，实际 {} 次",
        snapshot.packet_count("UserLocation")
    );

    // 4. 渲染一帧并保存 PNG。
    let entity_views = snapshot.entity_views(0);
    assert!(
        entity_views.len() >= 2,
        "至少应有自己 + 一只怪物，实际 {}",
        entity_views.len()
    );
    let mut scene = match MapScene::load(map_dir.join("0.map"), &data_dir) {
        Ok(scene) => scene,
        Err(error) => {
            eprintln!("没有可用的地图素材，改用合成地图：{error}");
            MapScene::from_parts(
                synthetic_map(),
                TileLibraries::empty("."),
                Viewport::DEFAULT,
            )
        }
    };
    let center = snapshot
        .self_player
        .as_ref()
        .map_or(Point::new(330, 270), |player| player.position);
    let mut frame = Frame::new(800, 600);
    scene.render_with_entities(&mut frame, center, &entity_views);
    assert!(
        frame.distinct_color_count() >= 3,
        "PNG 不应是一整片纯色（实际 {} 种颜色）",
        frame.distinct_color_count()
    );
    let non_black = frame
        .pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0)
        .count();
    assert!(
        non_black > 100,
        "PNG 至少要有 100 个非黑像素，实际 {non_black}"
    );

    let output = root.join("target/client-live.png");
    frame
        .save_png(&output)
        .expect("写出 target/client-live.png");
    let metadata = std::fs::metadata(&output).expect("PNG 应存在");
    assert!(metadata.len() > 1024, "PNG 不应为空文件");
    println!(
        "client-live.png：{} · {}×{} · {} 种颜色 · 实体 {} · 怪物 {}",
        output.display(),
        frame.width,
        frame.height,
        frame.distinct_color_count(),
        entity_views.len(),
        monster_count
    );

    drop(client);
    drop(runtime);
}

#[test]
fn live_client_walk_run_capture_continuous_frames() {
    let root = workspace_root();
    let map_dir = root.join("resource/map");
    let data_dir = root.join("resource/data");

    let (runtime, address) = start_server(&root);
    let (client, mut snapshot) = login(address, "walkrunacct", "WalkRunHero");
    assert!(snapshot.is_ready(), "快照应标记为已进入世界");

    // 1. 真实服务端地图信息 + 本地地形（生产素材；缺素材时退化成合成地图）。
    let map_name = snapshot
        .map
        .as_ref()
        .map(|map| map.file_name.clone())
        .expect("MapInformation 应带地图名");
    assert!(!map_name.is_empty(), "地图名不能为空");
    let map = Map::load(&map_dir.join(format!("{map_name}.map"))).ok();
    let start = snapshot
        .self_player
        .as_ref()
        .expect("自己的 UserInformation")
        .position;
    let occupied = occupied_cells(&snapshot, start);

    // 2. 选一个能连续走 4 格的方向：2 次走（各 1 格）+ 1 次跑（2 格）。
    let mut direction = DIRECTIONS[0];
    let mut max_open = 0;
    for candidate in DIRECTIONS {
        let open = open_steps(map.as_ref(), &occupied, start, candidate, 4);
        if open > max_open {
            max_open = open;
            direction = candidate;
        }
    }
    assert!(
        max_open >= 3,
        "出生点 ({}, {}) 附近至少要有 3 格连续可走（1 次走 + 1 次跑），实际最多 {max_open} 格（地图 {}，地图数据 {:?}）",
        start.x,
        start.y,
        map_name,
        map.as_ref().map(|map| (map.width, map.height))
    );
    let walk_count = if max_open >= 4 { 2 } else { 1 };
    println!(
        "PASS 选路：出生点 ({}, {}) · 方向 {:?} · 连续空位 {max_open} 格 · 计划 {walk_count} 次走 + 1 次跑（地图 {}）",
        start.x, start.y, direction, map_name
    );

    let (evidence_scene, terrain_source) = load_evidence_scene(&root, &map_name, &data_dir);
    println!("PASS 地形素材：{terrain_source}");
    // 证据帧在联机时序结束后才渲染/落盘（PNG 慢，不能挤占跑步窗口）。
    let mut captures: Vec<PendingCapture> = Vec::new();

    // 3. 连续走：每次都要服务端权威坐标推进、方向正确、UserLocation 计数增加。
    let mut recorded: Vec<RecordedStep> = Vec::new();
    for step_index in 0..walk_count {
        if step_index > 0 {
            std::thread::sleep(MOVE_DELAY_PAUSE);
        }
        let mut moved = None;
        for candidate in direction_candidates(direction) {
            let from = snapshot
                .self_player
                .as_ref()
                .expect("自己的 UserInformation")
                .position;
            let occupied_now = occupied_cells(&snapshot, from);
            if open_steps(map.as_ref(), &occupied_now, from, candidate, 1) < 1 {
                continue;
            }
            let before = snapshot.packet_count("UserLocation");
            assert!(client.send(NetCommand::Walk(candidate)), "网络线程应存活");
            let Ok(next) = client.wait_for_snapshot(
                |snapshot| snapshot.packet_count("UserLocation") > before,
                MOVE_TIMEOUT,
            ) else {
                continue;
            };
            let Some(me) = next.self_player.as_ref() else {
                continue;
            };
            let to = me.position;
            let toward = me.direction;
            let after = next.packet_count("UserLocation");
            snapshot = next;
            if toward == candidate && to == from.moved(candidate, 1) {
                moved = Some((from, to, candidate, before, after));
                break;
            }
        }
        let (from, to, used_direction, before, after) = moved.unwrap_or_else(|| {
            panic!(
                "第 {} 次走失败：方向 {:?} 附近没有可用落点（当前位置 {:?}）",
                step_index + 1,
                direction,
                snapshot.self_player.as_ref().map(|me| me.position)
            )
        });
        let received_ms = now_millis();
        // 与 app 相同：motion 起点取网络层应用该 UserLocation 后重设的动作时刻
        // （X1-01 统一时钟），不用 UI 收到快照的 now。
        let motion = match camera::motion_after_authoritative_step(&snapshot, from, to) {
            camera::StepMotion::Started(motion) => motion,
            other => panic!("走一步应可插值，实际 {other:?}"),
        };
        captures.push(PendingCapture {
            snapshot: snapshot.clone(),
            motion,
            action: "walk",
            step: u32::try_from(step_index + 1).unwrap(),
            direction: used_direction,
        });
        assert_eq!(
            after,
            before + 1,
            "一次成功走路应只回一条 UserLocation（{before} → {after}）"
        );
        println!(
            "PASS walk #{} 权威坐标 ({}, {}) -> ({}, {}) 方向 {:?} · UserLocation {} -> {}",
            step_index + 1,
            from.x,
            from.y,
            to.x,
            to.y,
            used_direction,
            before,
            after
        );
        recorded.push(RecordedStep {
            action: "walk",
            from,
            to,
            direction: used_direction,
            user_location_before: before,
            user_location_after: after,
            received_ms,
        });
        direction = used_direction;
    }

    // 4. 跑：等服务端 600ms 动作结束，`step_counter > 0` 时发 `C.Run`（正常跑步条件）。
    std::thread::sleep(MOVE_DELAY_PAUSE);
    let last_walk_at = recorded.last().map_or(0, |step| step.received_ms);
    let mut run_moved = None;
    for candidate in direction_candidates(direction) {
        let from = snapshot
            .self_player
            .as_ref()
            .expect("自己的 UserInformation")
            .position;
        let occupied_now = occupied_cells(&snapshot, from);
        let open = open_steps(
            map.as_ref(),
            &occupied_now,
            from,
            candidate,
            camera::RUN_STEPS,
        );
        if open < camera::RUN_STEPS {
            eprintln!(
                "[evidence] 跳过跑方向 {candidate:?}：从 ({}, {}) 只余 {open} 格可走",
                from.x, from.y
            );
            continue;
        }
        let before = snapshot.packet_count("UserLocation");
        assert!(client.send(NetCommand::Run(candidate)), "网络线程应存活");
        let Ok(next) = client.wait_for_snapshot(
            |snapshot| snapshot.packet_count("UserLocation") > before,
            MOVE_TIMEOUT,
        ) else {
            eprintln!(
                "[evidence] 跑方向 {candidate:?} 等 UserLocation 超时（距上次走 {}ms）",
                now_millis().saturating_sub(last_walk_at)
            );
            continue;
        };
        let Some(me) = next.self_player.as_ref() else {
            continue;
        };
        let to = me.position;
        let toward = me.direction;
        let after = next.packet_count("UserLocation");
        snapshot = next;
        if toward == candidate && to == from.moved(candidate, camera::RUN_STEPS) {
            run_moved = Some((from, to, candidate, before, after));
            break;
        }
        std::thread::sleep(RETRY_PAUSE);
    }
    let (from, to, used_direction, before, after) = run_moved.unwrap_or_else(|| {
        panic!(
            "跑失败：方向 {:?} 附近没有 2 格可用落点（当前位置 {:?}）",
            direction,
            snapshot.self_player.as_ref().map(|me| me.position)
        )
    });
    let received_ms = now_millis();
    // 同上：跑步也必须与动作共用网络层应用落点的时刻。
    let motion = match camera::motion_after_authoritative_step(&snapshot, from, to) {
        camera::StepMotion::Started(motion) => motion,
        other => panic!("跑两步应可插值，实际 {other:?}"),
    };
    captures.push(PendingCapture {
        snapshot: snapshot.clone(),
        motion,
        action: "run",
        step: 1,
        direction: used_direction,
    });
    assert_eq!(
        after,
        before + 1,
        "一次成功跑步应只回一条 UserLocation（{before} → {after}）"
    );
    assert_eq!(
        from.moved(used_direction, 2),
        to,
        "跑步必须一次跨 2 格（服务端正常跑步语义）"
    );
    println!(
        "PASS run  权威坐标 ({}, {}) -> ({}, {}) 方向 {:?} · UserLocation {} -> {}",
        from.x, from.y, to.x, to.y, used_direction, before, after
    );
    recorded.push(RecordedStep {
        action: "run",
        from,
        to,
        direction: used_direction,
        user_location_before: before,
        user_location_after: after,
        received_ms,
    });

    // 5. 步骤链核对（服务端权威坐标必须连续推进）。
    let mut expected_from = start;
    for step in &recorded {
        assert_eq!(step.from, expected_from, "每一步都从上一部落点出发");
        assert_eq!(
            step.to,
            step.from
                .moved(step.direction, if step.action == "run" { 2 } else { 1 }),
            "{} 的位移应为 {} 格",
            step.action,
            if step.action == "run" { 2 } else { 1 }
        );
        expected_from = step.to;
    }
    assert_eq!(
        snapshot.self_player.as_ref().expect("自己").position,
        expected_from,
        "最终快照坐标应等于最后一部落点"
    );

    // 6. 用生产渲染入口逐帧出证据（此时已无联机时序压力）。
    let mut writer = EvidenceWriter::new(&root, evidence_scene);
    for capture in &captures {
        writer.capture(
            &capture.snapshot,
            &capture.motion,
            capture.action,
            capture.step,
            capture.direction,
        );
    }
    let png_count = writer.save_pngs();

    // 7. 证据与偏移的最终核对。
    let walk_samples = writer
        .samples()
        .filter(|sample| sample.action == "walk")
        .count();
    let run_samples = writer
        .samples()
        .filter(|sample| sample.action == "run")
        .count();
    assert_eq!(
        walk_samples,
        usize::try_from(walk_count).unwrap() * usize::try_from(MOTION_FRAMES).unwrap(),
        "走路每步 6 帧证据"
    );
    assert_eq!(
        run_samples,
        usize::try_from(MOTION_FRAMES).unwrap(),
        "跑步 6 帧证据"
    );
    for sample in writer.samples() {
        assert_eq!(
            sample.self_screen, sample.screen_center,
            "每一帧自机都应钉在屏幕中心"
        );
    }
    let run_frame_offsets: Vec<(i32, i32)> = writer
        .samples()
        .filter(|sample| sample.action == "run")
        .map(|sample| sample.world_offset)
        .collect();
    let run_step = recorded.last().expect("跑步记录");
    let run_expected: Vec<(i32, i32)> = (0..MOTION_FRAMES)
        .map(|frame_index| {
            camera::world_offset(
                run_step.from,
                run_step.to,
                u64::try_from(frame_index).unwrap() * MOTION_INTERVAL_MS,
            )
        })
        .collect();
    assert_eq!(
        run_frame_offsets, run_expected,
        "跑步 6 帧世界偏移应照 Crystal OffSetMove 整数公式"
    );

    // 8. 证据清单与 PASS 行。
    let manifest = writer.write_manifest(&map_name, &terrain_source, address, &recorded);
    assert!(
        fs::metadata(&manifest).is_ok_and(|metadata| metadata.len() > 1024),
        "manifest 不应为空：{}",
        manifest.display()
    );
    assert_eq!(png_count, writer.frames.len(), "每个样本都要有 PNG");
    println!(
        "PASS 证据：{} 个时刻样本 · {} 张 PNG · manifest {}",
        writer.frames.len(),
        png_count,
        manifest.display()
    );
    println!(
        "PASS 总位移：({}, {}) -> ({}, {}) · 走 {} 步 + 跑 2 格",
        start.x, start.y, expected_from.x, expected_from.y, walk_count
    );
    let first_center = writer.samples().next().expect("至少一帧证据").screen_center;
    println!(
        "PASS 相机：自机屏幕锚点恒为 ({}, {}) · 世界偏移跑序列 {:?}",
        first_center.0, first_center.1, run_frame_offsets
    );

    drop(client);
    drop(runtime);
}
