//! 端到端联机测试：真实 `mir-server` + 无窗口客户端网络层 + 一帧渲染。
//!
//! 流程：
//!
//! 1. 在进程内用 `MemoryStore` 起一个真实服务端（`127.0.0.1:0`，演示怪物开关打开）；
//! 2. 客户端自动注册 → 登录 → 建角 → `StartGame` → 进入世界；
//! 3. 断言收到了自己的 `UserInformation`、至少一只 `ObjectMonster`，并且走路会产生
//!    `UserLocation`（自己的坐标发生变化）；
//! 4. 用快照渲染一帧到 `target/client-live.png`，断言不是空白。
//!
//! 所有等待都有超时，不使用固定 sleep。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use mir_client::net::{ConnectOptions, CreateCharacter, NetClient, NetCommand};
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_render::{EntityKind, Frame, MapScene, TileLibraries, Viewport};
use mir_server::{MemoryStore, ServerConfig, serve_forever};
use mir_world::{Map, MapCell, MapVersion};

/// 登录链路等待上限。
const LOGIN_TIMEOUT: Duration = Duration::from_secs(15);
/// 单次走路回包等待上限。
const WALK_TIMEOUT: Duration = Duration::from_secs(2);

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

#[test]
fn live_client_walks_and_renders_a_frame() {
    let root = workspace_root();
    let map_dir = root.join("resource/map");
    let data_dir = root.join("resource/data");

    // 1. 起真实服务端。地图目录用绝对路径，测试从哪个工作目录跑都一样。
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
        map_root: map_dir.display().to_string(),
        language_dir: root.join("config/lang").display().to_string(),
        load_game_data: false,
        demo_monsters: true,
        ..ServerConfig::default()
    });
    let store = Arc::new(MemoryStore::new());
    runtime.spawn(serve_forever(
        listener,
        Arc::clone(&config),
        Arc::clone(&store),
    ));

    // 2. 登录 → 建角 → 进世界。
    let client = NetClient::connect(ConnectOptions {
        server: address.to_string(),
        account: "liveacct".to_owned(),
        password: "livepass".to_owned(),
        character: "LiveHero".to_owned(),
        create: Some(CreateCharacter {
            name: "LiveHero".to_owned(),
            class: MirClass::Warrior,
            gender: MirGender::Male,
        }),
        auto_register: true,
    })
    .expect("启动客户端网络线程");

    let mut snapshot = client
        .wait_until_in_game(LOGIN_TIMEOUT)
        .expect("客户端应走完登录链路进入世界");
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
    for direction in [
        MirDirection::Down,
        MirDirection::DownRight,
        MirDirection::Right,
        MirDirection::UpRight,
        MirDirection::Up,
        MirDirection::UpLeft,
        MirDirection::Left,
        MirDirection::DownLeft,
    ] {
        let before_position = snapshot.self_player.as_ref().map(|player| player.position);
        let before_packets = snapshot.packet_count("UserLocation");
        assert!(client.send(NetCommand::Walk(direction)), "网络线程应存活");
        let Ok(next) = client.wait_for_snapshot(
            |snapshot| snapshot.packet_count("UserLocation") > before_packets,
            WALK_TIMEOUT,
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
        .map_or(mir_core::Point::new(330, 270), |player| player.position);
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
