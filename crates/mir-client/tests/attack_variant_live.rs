//! R47 live 证据：真实 `NetClient` + `MemoryStore`，用生产 `NetCommand::Attack`
//! 触发 Attack1/Attack3，观察者收到 `ObjectAttack`，真实 game-assets 逐帧 PNG。
//!
//! 测试注入的是**确定性变体**（等价于选中 roll=20/0），不是直接改
//! `SelfPlayer`/`EntityView`；命令仍走 `handle_command` 的生产本地动作链路，
//! 并且只发既有 `C.Attack`。

use std::fmt::Write as _;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mir_client::assets::{AssetMode, ResolvedAssets};
use mir_client::camera;
use mir_client::net::{ConnectOptions, CreateCharacter, NetClient, NetCommand, WorldSnapshot};
use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_render::animation::AttackVariant;
use mir_render::entities::{EntityLibraries, PlayerLayer, SpriteLibrary, player_layer_plan_at};
use mir_render::{ActorAction, Frame, MapScene, Viewport};
use mir_server::{MemoryStore, ServerConfig, serve_forever};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(15);
const ATTACK_TIMEOUT: Duration = Duration::from_secs(3);
const ATTACK_GAP: Duration = Duration::from_millis(950);

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

fn start_server(root: &Path) -> (tokio::runtime::Runtime, SocketAddr) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .expect("临时端口");
    let address = listener.local_addr().expect("地址");
    let config = Arc::new(ServerConfig {
        listen: address,
        map_root: root.join("resource/map").display().to_string(),
        language_dir: root.join("config/lang").display().to_string(),
        load_game_data: false,
        demo_monsters: false,
        ..ServerConfig::default()
    });
    runtime.spawn(serve_forever(
        listener,
        config,
        Arc::new(MemoryStore::new()),
    ));
    (runtime, address)
}

fn login(
    address: SocketAddr,
    account: &str,
    character: &str,
    gender: MirGender,
) -> (NetClient, WorldSnapshot) {
    let client = NetClient::connect(ConnectOptions {
        server: address.to_string(),
        account: account.to_owned(),
        password: "attackpass".to_owned(),
        character: character.to_owned(),
        create: Some(CreateCharacter {
            name: character.to_owned(),
            class: MirClass::Warrior,
            gender,
        }),
        auto_register: true,
        manual_select: false,
    })
    .expect("连接");
    let snapshot = client.wait_until_in_game(LOGIN_TIMEOUT).expect("进游戏");
    (client, snapshot)
}

fn load_evidence_scene(root: &Path, map_name: &str) -> (MapScene, String, &'static str) {
    let asset_root = root.join("game-assets");
    let mnm = asset_root.join("maps").join(format!("{map_name}.mnm"));
    assert!(mnm.is_file(), "默认素材缺少地图 {}", mnm.display());

    // 证据走真实默认启动路径：auto → game-assets 散装（gfx）→ 不允许 legacy。
    let assets = ResolvedAssets::load(&asset_root, AssetMode::Auto);
    assert_eq!(
        assets.effective(),
        Some(AssetMode::Dev),
        "默认启动必须解析到 game-assets 散装，不能回退 legacy"
    );
    let mut scene = MapScene::load_with_sources(
        &mnm,
        asset_root.join("data"),
        Some(asset_root.join("gfx").as_path()),
        Some(asset_root.as_path()),
        assets.map.as_ref(),
        assets.packs.as_ref(),
        false,
    )
    .expect("默认 game-assets 地图/图库必须可加载");

    let source = scene
        .entity_libraries_mut()
        .source_kind("hum")
        .expect("hum 图库");
    assert_eq!(source, "unpacked", "默认构建必须使用 game-assets/gfx 拆包");
    for library in [SpriteLibrary::Hum(1), SpriteLibrary::Hum(2)] {
        for frame in 344..=351 {
            assert!(
                scene.entity_libraries_mut().has_frame(library, frame),
                "{library:?} 缺少 Attack3 帧 {frame}（默认散装）"
            );
        }
    }
    (
        scene,
        format!("game-assets/maps/{map_name}.mnm（library-map.json + gfx 散装）"),
        source,
    )
}

fn assert_legacy_cross_check(root: &Path) {
    let mut legacy = EntityLibraries::open_with_sources(
        root.join("resource/data"),
        None,
        None,
        None,
        None,
        true,
    );
    assert_eq!(
        legacy.source_kind("hum"),
        Some("legacy-wzl"),
        "legacy 构建应显式使用完整 WZL 交叉校验"
    );
    for frame in 344..=351 {
        assert!(
            legacy.has_frame(SpriteLibrary::Hum(1), frame),
            "legacy hum.wzl 缺少 Attack3 帧 {frame}"
        );
    }
}

fn assert_release_pack_attack3(asset_root: &Path) -> String {
    let assets = ResolvedAssets::load(asset_root, AssetMode::Release);
    assert_eq!(
        assets.effective(),
        Some(AssetMode::Release),
        "release 小包必须可解析"
    );
    let packs_root = asset_root.join("packs");
    let mut libraries = EntityLibraries::open_with_sources(
        asset_root.join("data"),
        None,
        Some(packs_root.as_path()),
        assets.map.as_ref(),
        assets.packs.as_ref(),
        false,
    );
    assert_eq!(
        libraries.source_kind("hum"),
        Some("pack"),
        "release 证据必须来自 packs，而不是散装回退"
    );
    for library in [SpriteLibrary::Hum(1), SpriteLibrary::Hum(2)] {
        for frame in 344..=351 {
            assert!(
                libraries.has_frame(library, frame),
                "{library:?} release 包缺少 Attack3 帧 {frame}"
            );
        }
    }
    format!(
        "packs={}（hum/hum2 Attack3 344..351 已验证）",
        assets
            .packs
            .as_ref()
            .map_or(0, mir_asset::PackSet::pack_count)
    )
}

fn pixel_hash(frame: &Frame) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in &frame.pixels {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

struct Sample {
    action: &'static str,
    variant: AttackVariant,
    frame_index: u32,
    elapsed_ms: u64,
    simulated_now_ms: u64,
    wall_captured_ms: u64,
    source_started_ms: u64,
    /// Crystal 逻辑攻击序列帧号；非攻击（已回 Standing）为 None。
    logical_index: Option<u16>,
    /// 扁平素材攻击序列帧号；非攻击（已回 Standing）为 None。
    attack_frame_index: Option<u16>,
    /// 实际 body 层渲染帧（含身体外观块偏移；Standing 为 32/16/16）。
    actual_index: u32,
    action_now: ActorAction,
    view_variant: Option<AttackVariant>,
    camera_center: Point,
    self_screen: (i32, i32),
    layers: Vec<(String, u32)>,
    png: String,
    /// 解码后像素 FNV（跨运行稳定的画面指纹）。
    pixel_hash: u64,
    /// PNG 文件字节 FNV（证明 manifest 与磁盘文件同批）。
    png_byte_hash: u64,
    distinct_colors: usize,
}

fn byte_hash(path: &Path) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in fs::read(path).expect("读回 PNG") {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn capture_attack(
    scene: &mut MapScene,
    snapshot: &WorldSnapshot,
    variant: AttackVariant,
    label: &'static str,
    out_dir: &Path,
    root: &Path,
    max_frame: u32,
) -> Vec<Sample> {
    let me = snapshot.self_player.as_ref().expect("self");
    let name = me.name.clone();
    let direction = me.direction;
    let started = me.action_started_ms;
    let mut samples = Vec::new();
    for frame_index in 0..=max_frame {
        let elapsed = u64::from(frame_index) * 100;
        let now = started + elapsed;
        let mut runtime = snapshot.clone();
        let mut motion = None;
        let prepared = camera::prepare_frame(&mut runtime, &mut motion, Point::new(0, 0), now);
        let self_view = prepared
            .entities
            .iter()
            .find(|view| view.name.as_deref() == Some(name.as_str()))
            .expect("自机实体");
        let animation = self_view.animation();
        // Crystal 逻辑帧（136/232 起）与扁平素材物理索引（200/328 起）分列。
        let logical = variant.crystal_sequence().frame_at(direction, elapsed);
        let converted = variant.converted_sequence().frame_at(direction, elapsed);
        let layers = player_layer_plan_at(self_view, now)
            .layers()
            .iter()
            .map(|layer| (layer_name(layer.layer).to_owned(), layer.frame))
            .collect::<Vec<_>>();
        let body_actual = layers
            .iter()
            .find_map(|(name, frame)| (*name == "body").then_some(*frame))
            .expect("body 层必须存在");
        let attacking = self_view.action == ActorAction::Attacking;
        let logical_index = attacking.then_some(logical.index);
        let attack_frame_index = attacking.then_some(converted.index);
        let (sx, sy) = mir_render::geometry::world_to_screen(
            Viewport::DEFAULT,
            prepared.center,
            self_view.position,
        );
        let self_screen = (
            sx + prepared.offset.0 + self_view.pixel_offset.0,
            sy + prepared.offset.1 + self_view.pixel_offset.1,
        );
        assert_eq!(self_screen, Viewport::DEFAULT.center_origin(), "自机居中");
        let mut frame = Frame::new(800, 600);
        scene.render_with_entities_at_offset(
            &mut frame,
            prepared.center,
            prepared.offset,
            &prepared.entities,
            now,
        );
        assert!(frame.distinct_color_count() > 1, "渲染不能只有背景");
        let hash = pixel_hash(&frame);
        let name = format!("{label}-frame{frame_index}.png");
        let path = out_dir.join(&name);
        frame.save_png(&path).expect("PNG");
        let png_byte_hash = byte_hash(&path);
        let sample = Sample {
            action: label,
            variant,
            frame_index,
            elapsed_ms: elapsed,
            simulated_now_ms: now,
            wall_captured_ms: now_millis(),
            source_started_ms: started,
            logical_index,
            attack_frame_index,
            actual_index: body_actual,
            action_now: self_view.action,
            view_variant: self_view.attack_variant,
            camera_center: prepared.center,
            self_screen,
            layers,
            png: path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string(),
            pixel_hash: hash,
            png_byte_hash,
            distinct_colors: frame.distinct_color_count(),
        };
        assert_eq!(
            sample.actual_index,
            sample
                .layers
                .iter()
                .find_map(|(name, frame)| (*name == "body").then_some(*frame))
                .expect("body 层"),
            "actual_index 必须是实际 body 层帧"
        );
        if elapsed < variant.converted_sequence().duration_ms() {
            assert_eq!(self_view.attack_variant, Some(variant));
            assert_eq!(animation.attack_variant, Some(variant));
            assert_eq!(animation.frames, variant.converted_sequence());
            assert_eq!(sample.attack_frame_index, Some(converted.index));
        } else {
            assert_eq!(self_view.action, ActorAction::Standing);
            assert_eq!(sample.attack_frame_index, None);
            assert_eq!(sample.logical_index, None);
        }
        samples.push(sample);
    }
    samples
}

fn layer_name(layer: PlayerLayer) -> &'static str {
    match layer {
        PlayerLayer::Body => "body",
        PlayerLayer::Weapon => "weapon",
        PlayerLayer::Hair => "hair",
        PlayerLayer::Shield => "shield",
        PlayerLayer::Wing => "wing",
        PlayerLayer::Mount => "mount",
    }
}

#[allow(clippy::too_many_arguments)]
fn perform_attack(
    attacker: &mut NetClient,
    snapshot: &mut WorldSnapshot,
    observer: &mut NetClient,
    observer_snapshot: &mut WorldSnapshot,
    target_id: u32,
    direction: MirDirection,
    variant: AttackVariant,
    label: &'static str,
    max_frame: u32,
    scene: &mut MapScene,
    out_dir: &Path,
    root: &Path,
) -> (Vec<Sample>, serde_json::Value) {
    let before_start = snapshot
        .self_player
        .as_ref()
        .expect("self")
        .action_started_ms;
    let before_observer = observer_snapshot.packet_count("ObjectAttack");
    let sent_wall_ms = now_millis();
    assert!(
        attacker.send(NetCommand::Attack {
            direction,
            variant: Some(variant),
        }),
        "生产命令应进入网络队列"
    );
    let command_enqueued_wall_ms = now_millis();
    let next = attacker
        .wait_for_snapshot(
            move |snap| {
                snap.self_player.as_ref().is_some_and(|me| {
                    me.action == ActorAction::Attacking
                        && me.attack_variant == Some(variant)
                        && me.action_started_ms > before_start
                })
            },
            ATTACK_TIMEOUT,
        )
        .expect("攻击者本地动作应建立");
    let attacker_ack_wall_ms = now_millis();
    *snapshot = next;
    let observer_next = observer
        .wait_for_snapshot(
            move |snap| {
                snap.packet_count("ObjectAttack") > before_observer
                    && snap
                        .entities
                        .get(&target_id)
                        .is_some_and(|entity| entity.action == ActorAction::Attacking)
            },
            ATTACK_TIMEOUT,
        )
        .expect("观察者应收到 ObjectAttack");
    *observer_snapshot = observer_next;
    let samples = capture_attack(scene, snapshot, variant, label, out_dir, root, max_frame);
    let step = serde_json::json!({
        "label": label,
        "direction": format!("{direction:?}"),
        "variant": variant.as_str(),
        "sent_wall_ms": sent_wall_ms,
        "command_enqueued_wall_ms": command_enqueued_wall_ms,
        "attacker_ack_wall_ms": attacker_ack_wall_ms,
        "send_to_ack_ms": attacker_ack_wall_ms.saturating_sub(sent_wall_ms),
        "action_started_ms": snapshot
            .self_player
            .as_ref()
            .expect("自己")
            .action_started_ms,
        "injected_roll": if variant == AttackVariant::Attack3 { 0 } else { 20 },
        "crystal_logical": {
            "start": variant.crystal_sequence().start,
            "count": variant.crystal_sequence().count,
            "skip": variant.crystal_sequence().skip,
            "effect_start": variant.crystal_effect_sequence().start,
            "effect_count": variant.crystal_effect_sequence().count,
        },
        "converted": {
            "start": variant.converted_sequence().start,
            "count": variant.converted_sequence().count,
            "skip": variant.converted_sequence().skip,
            "interval_ms": variant.converted_sequence().interval_ms,
            "effect_start": variant.converted_effect_sequence().start,
            "effect_count": variant.converted_effect_sequence().count,
        },
        "source_started_ms": samples[0].source_started_ms,
        "observer_object_attack_before": before_observer,
        "observer_object_attack_after": observer_snapshot.packet_count("ObjectAttack"),
    });
    (samples, step)
}

#[test]
fn two_clients_attack1_attack3_live_evidence() {
    let root = workspace_root();
    let (runtime, address) = start_server(&root);
    let (mut female_client, mut female_snapshot) =
        login(address, "r47female", "R47Female", MirGender::Female);
    let (mut male_client, mut male_snapshot) =
        login(address, "r47male", "R47Male", MirGender::Male);
    let female_id = female_snapshot.self_player.as_ref().unwrap().object_id;
    let male_id = male_snapshot.self_player.as_ref().unwrap().object_id;
    female_snapshot = female_client
        .wait_for_snapshot(
            |snap| snap.entities.contains_key(&male_id),
            Duration::from_secs(10),
        )
        .expect("female 看到 male");
    male_snapshot = male_client
        .wait_for_snapshot(
            |snap| snap.entities.contains_key(&female_id),
            Duration::from_secs(10),
        )
        .expect("male 看到 female");
    let map_name = female_snapshot
        .map
        .as_ref()
        .map_or_else(|| "0".to_owned(), |map| map.file_name.clone());
    let (mut scene, terrain_source, assets_source) = load_evidence_scene(&root, &map_name);
    assert_eq!(assets_source, "unpacked");
    let legacy_cross_check = if std::env::var_os("MIR2_TEST_LEGACY").is_some() {
        assert_legacy_cross_check(&root);
        "enabled（--features mir-render/legacy + MIR2_TEST_LEGACY=1）"
    } else {
        "skipped（默认构建不启用 legacy；legacy 构建加 MIR2_TEST_LEGACY=1 才交叉校验）"
    };
    let release_pack_check = assert_release_pack_attack3(&root.join("game-assets"));
    let out_dir = root.join("build/evidence/attack-variants");
    if out_dir.exists() {
        for entry in fs::read_dir(&out_dir).expect("旧证据目录") {
            let path = entry.expect("目录项").path();
            if path.extension().is_some_and(|extension| extension == "png") {
                fs::remove_file(path).expect("删除旧 PNG");
            }
        }
    }
    fs::create_dir_all(&out_dir).expect("证据目录");
    let mut samples = Vec::new();
    let mut steps = Vec::new();

    // female：Attack1 纵向、Attack1 横向（像素对照）、Attack3 横向；male：Attack1 横向。
    let (mut extra, step) = perform_attack(
        &mut female_client,
        &mut female_snapshot,
        &mut male_client,
        &mut male_snapshot,
        female_id,
        MirDirection::Down,
        AttackVariant::Attack1,
        "female-attack1-down",
        6,
        &mut scene,
        &out_dir,
        &root,
    );
    samples.append(&mut extra);
    steps.push(step);
    std::thread::sleep(ATTACK_GAP);

    let (mut extra, step) = perform_attack(
        &mut female_client,
        &mut female_snapshot,
        &mut male_client,
        &mut male_snapshot,
        female_id,
        MirDirection::Right,
        AttackVariant::Attack1,
        "female-attack1-right",
        1,
        &mut scene,
        &out_dir,
        &root,
    );
    samples.append(&mut extra);
    steps.push(step);
    std::thread::sleep(ATTACK_GAP);

    let (mut extra, step) = perform_attack(
        &mut female_client,
        &mut female_snapshot,
        &mut male_client,
        &mut male_snapshot,
        female_id,
        MirDirection::Right,
        AttackVariant::Attack3,
        "female-attack3-right",
        8,
        &mut scene,
        &out_dir,
        &root,
    );
    samples.append(&mut extra);
    steps.push(step);
    std::thread::sleep(ATTACK_GAP);

    let (mut extra, step) = perform_attack(
        &mut male_client,
        &mut male_snapshot,
        &mut female_client,
        &mut female_snapshot,
        male_id,
        MirDirection::Right,
        AttackVariant::Attack1,
        "male-attack1-right",
        6,
        &mut scene,
        &out_dir,
        &root,
    );
    samples.append(&mut extra);
    steps.push(step);

    // 像素级：同一 female/Right/装备/背景，Attack1 与 Attack3 第一帧必须不同。
    let attack1 = samples
        .iter()
        .find(|sample| sample.action == "female-attack1-right" && sample.frame_index == 0)
        .expect("Attack1 像素");
    let attack3 = samples
        .iter()
        .find(|sample| sample.action == "female-attack3-right" && sample.frame_index == 0)
        .expect("Attack3 像素");
    assert_ne!(
        attack1.pixel_hash, attack3.pixel_hash,
        "两变体必须选到不同素材"
    );
    assert_ne!(attack1.actual_index, attack3.actual_index);
    assert_eq!(attack1.actual_index, 216, "Right=2, Attack1 body=200+16");
    assert_eq!(attack3.actual_index, 344, "Right=2, Attack3 body=328+16");
    assert_eq!(attack1.attack_frame_index, Some(216));
    assert_eq!(attack3.attack_frame_index, Some(344));

    // Standing 的实际 body 帧：Down=32，Right=16；不是攻击序列的 200/328。
    let down_end = samples
        .iter()
        .find(|sample| sample.action == "female-attack1-down" && sample.frame_index == 6)
        .expect("female Attack1 Down 终点");
    let right_end = samples
        .iter()
        .find(|sample| sample.action == "female-attack3-right" && sample.frame_index == 8)
        .expect("female Attack3 Right 终点");
    let male_end = samples
        .iter()
        .find(|sample| sample.action == "male-attack1-right" && sample.frame_index == 6)
        .expect("male Attack1 Right 终点");
    for sample in [down_end, right_end, male_end] {
        assert_eq!(sample.action_now, ActorAction::Standing);
        assert_eq!(sample.attack_frame_index, None);
        assert_eq!(sample.logical_index, None);
    }
    assert_eq!(down_end.actual_index, 32, "Down Standing body=32");
    assert_eq!(right_end.actual_index, 16, "Right Standing body=16");
    assert_eq!(male_end.actual_index, 16, "Male Right Standing body=16");

    let generated_at = now_millis();
    let manifest = serde_json::json!({
        "test": "two_clients_attack1_attack3_live_evidence",
        "batch_id": generated_at,
        "generated_at_unix_ms": generated_at,
        "server": {"store": "MemoryStore", "address": address.to_string()},
        "map": map_name,
        "asset_mode": "dev",
        "assets_source": assets_source,
        "terrain_source": terrain_source,
        "release_pack_check": release_pack_check,
        "legacy_cross_check": legacy_cross_check,
        "crystal_reference": "github/Crystal HEAD 0e315fe327192afe52c3d7357ddd1f5b7e26c5b8",
        "baseline": "55bd709",
        "actors": {
            "female": {"object_id": female_id, "name": "R47Female", "class": "Warrior", "gender": "Female"},
            "male": {"object_id": male_id, "name": "R47Male", "class": "Warrior", "gender": "Male"},
        },
        "entry": "NetCommand::Attack { variant } → ClientApp/NetClient 生产本地动作 → C.Attack",
        "protocol": {
            "client_packet": "C.Attack（direction+spell=None）",
            "variant_in_protocol": false,
            "extra_client_attack_packets": 0,
            "clock": "client wall clock; server packets have no protocol timestamp",
            "action_progress": "action_started_ms..now_ms; send/transmission never added to progress",
        },
        "layer_availability": {
            "loaded": ["body", "hair"],
            "absent_standard_character": ["weapon", "shield", "wing", "mount"],
            "note": "标准新角色无武器/盾/翅膀/坐骑；武器/盾/翅膀层由 entities 单测锁帧契约",
            "attack3_swing_effect": "degraded: no generic effect.wzl swing mapping; intentionally skipped",
        },
        "rng_injection": "测试注入确定性变体（Attack3 等价 roll=0，Attack1 等价 roll=20）；A1 单测覆盖选择器",
        "steps": steps,
        "samples": samples.iter().map(|sample| serde_json::json!({
            "action": sample.action,
            "variant": sample.variant.as_str(),
            "frame_index": sample.frame_index,
            "elapsed_ms": sample.elapsed_ms,
            "simulated_now_ms": sample.simulated_now_ms,
            "wall_captured_ms": sample.wall_captured_ms,
            "source_started_ms": sample.source_started_ms,
            "logical_index": sample.logical_index,
            "attack_frame_index": sample.attack_frame_index,
            "actual_index": sample.actual_index,
            "actual_index_semantics": "body 层实际渲染帧（含外观块偏移）；Standing 为 32/16/16",
            "action_now": sample.action_now.as_str(),
            "view_variant": sample.view_variant.map(AttackVariant::as_str),
            "camera_center": [sample.camera_center.x, sample.camera_center.y],
            "self_screen": [sample.self_screen.0, sample.self_screen.1],
            "layers": sample.layers.iter().map(|(name, frame)| serde_json::json!({"layer": name, "frame": frame})).collect::<Vec<_>>(),
            "png": sample.png,
            "pixel_hash": format!("{:016x}", sample.pixel_hash),
            "png_byte_hash": format!("{:016x}", sample.png_byte_hash),
            "distinct_colors": sample.distinct_colors,
        })).collect::<Vec<_>>(),
    });
    let manifest_path = out_dir.join("live-manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).expect("manifest"),
    )
    .expect("写 manifest");

    // 同一批：run log、build manifest、docs manifest、25 张 PNG 都使用同一个 batch_id。
    let mut log = format!(
        "batch_id={generated_at}\ntest=two_clients_attack1_attack3_live_evidence\nsamples={}\n",
        samples.len()
    );
    for sample in &samples {
        let file_name = Path::new(&sample.png)
            .file_name()
            .expect("PNG 文件名")
            .to_string_lossy();
        writeln!(
            log,
            "{} frame={} actual={} attack_frame={:?} logical={:?} png={} png_byte_hash={:016x}",
            sample.action,
            sample.frame_index,
            sample.actual_index,
            sample.attack_frame_index,
            sample.logical_index,
            file_name,
            sample.png_byte_hash
        )
        .expect("格式化 live run log");
    }
    let log_path = out_dir.join("live-run.log");
    fs::write(&log_path, &log).expect("写 live run log");

    let docs_dir = root.join("docs/testing/attack-variants");
    fs::create_dir_all(&docs_dir).expect("docs 证据目录");
    for entry in fs::read_dir(&docs_dir).expect("docs 证据目录") {
        let path = entry.expect("目录项").path();
        if path.extension().is_some_and(|extension| extension == "png") {
            fs::remove_file(path).expect("删除旧 docs PNG");
        }
    }
    for sample in &samples {
        let file_name = Path::new(&sample.png).file_name().expect("PNG 文件名");
        fs::copy(out_dir.join(file_name), docs_dir.join(file_name)).expect("复制 PNG");
        assert_eq!(
            byte_hash(&docs_dir.join(file_name)),
            sample.png_byte_hash,
            "docs PNG 字节哈希必须与 manifest 同批"
        );
    }
    fs::copy(&manifest_path, docs_dir.join("live-manifest.json")).expect("复制 manifest");
    fs::copy(&log_path, docs_dir.join("live-run.log")).expect("复制 live run log");
    assert_eq!(
        fs::read(&manifest_path).expect("build manifest"),
        fs::read(docs_dir.join("live-manifest.json")).expect("docs manifest"),
        "build/docs manifest 必须字节一致"
    );
    println!(
        "PASS attack-variant live：batch={generated_at}；female {female_id}/male {male_id}；{} 帧；manifest {}；docs {}",
        samples.len(),
        manifest_path.display(),
        docs_dir.display()
    );
    drop((female_client, male_client, runtime));
}
