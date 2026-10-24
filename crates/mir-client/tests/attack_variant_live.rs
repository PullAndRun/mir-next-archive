//! R47 live 证据：真实 `NetClient` + `MemoryStore`，用生产 `NetCommand::Attack`
//! 触发 Attack1/Attack3，观察者收到 `ObjectAttack`，真实 game-assets 逐帧 PNG。
//!
//! 测试注入的是**确定性变体**（等价于选中 roll=20/0），不是直接改
//! `SelfPlayer`/`EntityView`；命令仍走 `handle_command` 的生产本地动作链路，
//! 并且只发既有 `C.Attack`。
//!
//! 证据只写**不入库**目录（默认 `target/attack-variants/`，可用
//! `MIR_ATTACK_EVIDENCE_DIR` 覆盖），绝不写入 R47 冻结目录
//! `docs/testing/attack-variants/`。R48 起渲染默认走原生玩家库，所以 manifest
//! 把扁平计划帧号与原生实际帧号分列陈述：`planned_flat_index` 只是扁平版式的
//! 计划值，**不是**实际渲染帧；实际选中的库/帧见 `native_selection`。

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
use mir_render::animation::{AttackVariant, FrameSequence};
use mir_render::entities::{
    EntityLibraries, EntityView, NO_LAYER, PlayerLayer, SpriteLibrary, player_layer_plan_at,
};
use mir_render::player_layout::{
    NativeAction, NativeDrawLayer, NativeEffectTable, NativeFrameTable, NativeHand, NativeLayer,
    WeaponSegment, draw_order,
};
use mir_render::{ActorAction, Frame, MapScene, Viewport};
use mir_server::{MemoryStore, ServerConfig, serve_forever};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(15);
const ATTACK_TIMEOUT: Duration = Duration::from_secs(3);
const ATTACK_GAP: Duration = Duration::from_millis(950);
/// 冻结的 R47 证据目录：只读参照，本测试运行时不写入/不覆盖。
const FROZEN_EVIDENCE_DIR: &str = "docs/testing/attack-variants";
/// 默认证据输出目录（相对仓库根）；`target/` 不入库，不污染 tracked 证据。
const DEFAULT_EVIDENCE_DIR: &str = "target/attack-variants";
/// 覆盖证据输出目录的环境变量（相对仓库根或绝对路径）。
const EVIDENCE_DIR_ENV: &str = "MIR_ATTACK_EVIDENCE_DIR";

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

/// 本次运行的证据输出目录：`MIR_ATTACK_EVIDENCE_DIR` 优先，否则 `target/attack-variants/`。
///
/// 防呆：任何落在 `docs/`（入库证据）下的路径都直接拒绝，避免再次污染冻结批次。
fn evidence_dir(root: &Path) -> PathBuf {
    let configured = std::env::var_os(EVIDENCE_DIR_ENV)
        .map_or_else(|| PathBuf::from(DEFAULT_EVIDENCE_DIR), PathBuf::from);
    let dir = if configured.is_absolute() {
        configured
    } else {
        root.join(configured)
    };
    assert!(
        dir.strip_prefix(root.join("docs")).is_err(),
        "证据输出目录不能落在入库的 docs/ 下：{}（用 {EVIDENCE_DIR_ENV} 覆盖）",
        dir.display()
    );
    dir
}

/// 递归收集目录内文件（相对路径、绝对路径），用于内容指纹。
fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out);
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            out.push((relative, path));
        }
    }
}

/// 目录内容指纹：排序后的 `(相对路径, 文件字节 FNV)`，用来证明冻结目录零变化。
fn directory_fingerprint(dir: &Path) -> Vec<(String, u64)> {
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files);
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
        .into_iter()
        .map(|(relative, path)| (relative, byte_hash(&path)))
        .collect()
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
        monster_frames: std::collections::BTreeMap::new(),
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
    /// 扁平素材攻击序列帧号（不含外观块基址）；非攻击（已回 Standing）为 None。
    planned_flat_attack_index: Option<u16>,
    /// 旧扁平版式的**计划** body 帧（外观块基址 + 序列索引）。
    ///
    /// R48 起实际渲染默认走原生库，这个值只是 `player_layer_plan_at` 的对照，
    /// 不是实际渲染帧；实际帧见 [`Sample::native`]。
    planned_flat_index: u32,
    action_now: ActorAction,
    view_variant: Option<AttackVariant>,
    camera_center: Point,
    self_screen: (i32, i32),
    /// `player_layer_plan_at` 的计划层（扁平版式的对照）。
    planned_flat_layers: Vec<(String, u32)>,
    /// 原生渲染路径实际选中的动作/层/帧（渲染器同款公开公式复算）。
    native: NativeRender,
    png: String,
    /// 解码后像素 FNV（跨运行稳定的画面指纹）。
    pixel_hash: u64,
    /// PNG 文件字节 FNV（证明 manifest 与磁盘文件同批）。
    png_byte_hash: u64,
    distinct_colors: usize,
}

fn byte_hash(path: &Path) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in fs::read(path).expect("读回文件") {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// 原生路径实际参与绘制的一层。
struct NativeDrawnLayer {
    /// 水晶绘制层名：`body` / `head` / `wing` / `weapon-main` / `weapon-off`。
    layer: &'static str,
    library: String,
    frame: u32,
    /// 图像是否真实存在；`body` 缺帧时渲染器会画洋红占位（仍算已处理该层）。
    drawn: bool,
}

/// R48 原生玩家路径（`draw_native_player`）在当前帧实际会选中的动作/序列/帧。
///
/// 计算只使用 `mir-render` 的公开帧表、外观列偏移与 `sequence.index_of` 公式，
/// 与渲染器逐行一致；本结构只写证据，不改变渲染行为。
struct NativeRender {
    /// `native`：库存在，实际走原生路径；`flat-fallback`：渲染器会回退旧扁平版式。
    path: &'static str,
    fallback_reason: Option<&'static str>,
    /// 原生库身份：`(class, gender, alt_anim)`。
    layout: Option<(String, String, bool)>,
    /// Crystal 原生动作名（`NativeAction`）。
    action: Option<String>,
    sequence: Option<FrameSequence>,
    elapsed_ms: u64,
    local: u16,
    /// `sequence.index_of(direction, local)`，即库内方向索引（不含外观列偏移）。
    direction_index: u16,
    /// 外观列偏移（普通女性 808 / 男性 0）。
    armour_offset: u16,
    body_library: Option<String>,
    /// 实际 body 帧 = `armour_offset + direction_index`；回退路径为 None。
    body_frame: Option<u32>,
    body_image_present: bool,
    layers: Vec<NativeDrawnLayer>,
}

impl NativeRender {
    fn flat_fallback(reason: &'static str) -> Self {
        Self {
            path: "flat-fallback",
            fallback_reason: Some(reason),
            layout: None,
            action: None,
            sequence: None,
            elapsed_ms: 0,
            local: 0,
            direction_index: 0,
            armour_offset: 0,
            body_library: None,
            body_frame: None,
            body_image_present: false,
            layers: Vec::new(),
        }
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "path": self.path,
            "fallback_reason": self.fallback_reason,
            "layout": self.layout.as_ref().map(|(class, gender, alt_anim)| serde_json::json!({
                "class": class,
                "gender": gender,
                "alt_anim": alt_anim,
            })),
            "action": self.action,
            "sequence": self.sequence.map(|sequence| serde_json::json!({
                "start": sequence.start,
                "count": sequence.count,
                "skip": sequence.skip,
                "interval_ms": sequence.interval_ms,
            })),
            "elapsed_ms": self.elapsed_ms,
            "local": self.local,
            "direction_index": self.direction_index,
            "armour_offset": self.armour_offset,
            "body_library": self.body_library,
            "body_frame": self.body_frame,
            "body_image_present": self.body_image_present,
            "layers": self.layers.iter().map(|layer| serde_json::json!({
                "layer": layer.layer,
                "library": layer.library,
                "frame": layer.frame,
                "drawn": layer.drawn,
            })).collect::<Vec<_>>(),
        })
    }
}

/// 与 `draw_native_player` 相同的局部帧计算：`Standing` 循环取模，其余夹到最后一帧。
fn native_local(sequence: FrameSequence, elapsed_ms: u64, looping: bool) -> u16 {
    let interval = sequence.interval_ms.max(1);
    let frame_number = elapsed_ms / interval;
    let local = if looping {
        frame_number % u64::from(sequence.count.max(1))
    } else {
        frame_number.min(u64::from(sequence.count.saturating_sub(1)))
    };
    u16::try_from(local).unwrap_or(0)
}

/// 复算 `draw_native_player` 当前帧会选中的原生库/帧（只读证据，不参与渲染）。
fn native_render_record(
    libraries: &mut EntityLibraries,
    view: &EntityView,
    now_ms: u64,
) -> NativeRender {
    let Some(layout) = view.player_layout else {
        return NativeRender::flat_fallback("view 没有 player_layout（非玩家/骑乘/未知动作）");
    };
    let action = view
        .native_action
        .unwrap_or_else(|| layout.native_action(view.action, view.attack_variant));
    let sequence = NativeFrameTable::get(action);
    let elapsed = now_ms.saturating_sub(view.action_started_ms);
    let local = native_local(sequence, elapsed, action == NativeAction::Standing);
    let direction_index = sequence.index_of(view.direction, local);
    let id = layout.library_id();
    let Some(body_key) = id.library_key(NativeLayer::Armour, view.armour) else {
        return NativeRender::flat_fallback("armour 外观号超出原生库形状范围（>=100）");
    };
    // 渲染器只在“原生身体库不存在”时回退扁平版式；库存在就必须留在原生路径。
    if libraries.native_library(&body_key).is_none() {
        return NativeRender::flat_fallback("原生身体库不存在，渲染器回退扁平版式");
    }
    let body_frame = u32::from(id.armour_offset()) + u32::from(direction_index);
    let body_image_present = libraries.native_image(&body_key, body_frame).is_some();

    let weapon_shape = view.weapon_shape;
    let main_key = (weapon_shape != NO_LAYER)
        .then(|| id.library_key(NativeLayer::Weapon(NativeHand::Main), weapon_shape))
        .flatten();
    let off_key = (weapon_shape != NO_LAYER
        && matches!(
            layout.weapon_segment,
            WeaponSegment::Assassin | WeaponSegment::Archer
        ))
    .then(|| id.library_key(NativeLayer::Weapon(NativeHand::Off), weapon_shape))
    .flatten();
    let weapon_frame = u32::from(id.weapon_offset()) + u32::from(direction_index);
    let hair_key = (view.hair != NO_LAYER)
        .then(|| id.library_key(NativeLayer::Hair, view.hair))
        .flatten();
    let hair_frame = u32::from(id.hair_offset()) + u32::from(direction_index);
    let wing_key = (view.wings != 0)
        .then(|| id.library_key(NativeLayer::Wing, view.wings.saturating_sub(1)))
        .flatten();
    // 翅膀/特效帧表是独立表（Crystal `HumEffect`），与动作帧表分开算 local。
    let effect = NativeEffectTable::get(action);
    let effect_local = native_local(effect, elapsed, action == NativeAction::Standing);
    let wing_frame =
        u32::from(id.wing_offset()) + u32::from(effect.index_of(view.direction, effect_local));

    let archer_extra_off =
        layout.class == MirClass::Archer && layout.weapon_segment == WeaponSegment::Archer;
    let (order, order_len) = draw_order(
        view.direction,
        main_key.is_some(),
        off_key.is_some(),
        archer_extra_off,
    );
    let mut layers = Vec::with_capacity(order_len);
    for layer in &order[..order_len] {
        match layer {
            NativeDrawLayer::Body => layers.push(NativeDrawnLayer {
                layer: "body",
                library: body_key.clone(),
                frame: body_frame,
                drawn: body_image_present,
            }),
            NativeDrawLayer::Head => {
                if let Some(key) = &hair_key {
                    layers.push(NativeDrawnLayer {
                        layer: "head",
                        library: key.clone(),
                        frame: hair_frame,
                        drawn: libraries.native_image(key, hair_frame).is_some(),
                    });
                }
            }
            NativeDrawLayer::Wing => {
                if let Some(key) = &wing_key {
                    layers.push(NativeDrawnLayer {
                        layer: "wing",
                        library: key.clone(),
                        frame: wing_frame,
                        drawn: libraries.native_image(key, wing_frame).is_some(),
                    });
                }
            }
            NativeDrawLayer::Weapon(hand) => {
                let key = match hand {
                    NativeHand::Main => &main_key,
                    NativeHand::Off => &off_key,
                };
                if let Some(key) = key {
                    layers.push(NativeDrawnLayer {
                        layer: match hand {
                            NativeHand::Main => "weapon-main",
                            NativeHand::Off => "weapon-off",
                        },
                        library: key.clone(),
                        frame: weapon_frame,
                        drawn: libraries.native_image(key, weapon_frame).is_some(),
                    });
                }
            }
        }
    }
    NativeRender {
        path: "native",
        fallback_reason: None,
        layout: Some((
            format!("{:?}", layout.class),
            format!("{:?}", layout.gender),
            layout.alt_anim,
        )),
        action: Some(format!("{action:?}")),
        sequence: Some(sequence),
        elapsed_ms: elapsed,
        local,
        direction_index,
        armour_offset: id.armour_offset(),
        body_library: Some(body_key),
        body_frame: Some(body_frame),
        body_image_present,
        layers,
    }
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
        let planned_flat_body = layers
            .iter()
            .find_map(|(name, frame)| (*name == "body").then_some(*frame))
            .expect("body 层必须存在");
        // R48 起渲染器优先原生玩家路径；这里用同一套公开公式复算实际会画的帧。
        let native = native_render_record(scene.entity_libraries_mut(), self_view, now);
        let attacking = self_view.action == ActorAction::Attacking;
        let logical_index = attacking.then_some(logical.index);
        let planned_flat_attack_index = attacking.then_some(converted.index);
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
            planned_flat_attack_index,
            planned_flat_index: planned_flat_body,
            action_now: self_view.action,
            view_variant: self_view.attack_variant,
            camera_center: prepared.center,
            self_screen,
            planned_flat_layers: layers,
            native,
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
            sample.planned_flat_index,
            sample
                .planned_flat_layers
                .iter()
                .find_map(|(name, frame)| (*name == "body").then_some(*frame))
                .expect("body 层"),
            "planned_flat_index 必须是扁平计划 body 层帧（对照值，不是实际渲染帧）"
        );
        if sample.native.path == "native" {
            assert_eq!(
                sample.native.body_frame,
                Some(
                    u32::from(sample.native.armour_offset)
                        + u32::from(sample.native.direction_index)
                ),
                "原生 body 帧 = 外观列偏移 + sequence.index_of(direction, local)"
            );
            assert!(
                sample
                    .native
                    .layers
                    .iter()
                    .any(|layer| layer.layer == "body"),
                "原生路径必须记录 body 层"
            );
            if let Some(head) = sample
                .native
                .layers
                .iter()
                .find(|layer| layer.layer == "head")
            {
                assert_eq!(
                    head.frame,
                    sample.native.body_frame.unwrap_or(0),
                    "原生头/身体共用方向索引（hair_offset == armour_offset）"
                );
            }
            if attacking && elapsed == 0 {
                let expected = match variant {
                    AttackVariant::Attack1 => "Attack1",
                    AttackVariant::Attack3 => "Attack3",
                };
                assert_eq!(
                    sample.native.action.as_deref(),
                    Some(expected),
                    "原生动作必须来自同一个 view.attack_variant"
                );
            }
        } else {
            assert!(
                sample.native.fallback_reason.is_some(),
                "扁平回退路径必须记录原因"
            );
        }
        if elapsed < variant.converted_sequence().duration_ms() {
            assert_eq!(self_view.attack_variant, Some(variant));
            assert_eq!(animation.attack_variant, Some(variant));
            assert_eq!(animation.frames, variant.converted_sequence());
            assert_eq!(sample.planned_flat_attack_index, Some(converted.index));
        } else {
            assert_eq!(self_view.action, ActorAction::Standing);
            assert_eq!(sample.planned_flat_attack_index, None);
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
            native_action: None,
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

/// R47 冻结证据轮次的历史测试：需要完整 `game-assets` 与真实渲染管线，
/// 全量门禁默认不跑（否则会刷新 tracked 证据目录）；显式 `--ignored` 单独执行，
/// 输出只写 `target/attack-variants/`（或 `MIR_ATTACK_EVIDENCE_DIR`）。
#[ignore = "需要完整 game-assets 与真实渲染管线；全量门禁默认不跑，显式 --ignored 单独执行"]
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
    let out_dir = evidence_dir(&root);
    // 冻结 R47 目录只读：运行前抓指纹，运行结束后必须逐字节一致。
    let frozen_dir = root.join(FROZEN_EVIDENCE_DIR);
    let frozen_before = directory_fingerprint(&frozen_dir);
    let frozen_batch_id = fs::read(frozen_dir.join("live-manifest.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|manifest| manifest.get("batch_id").and_then(serde_json::Value::as_u64));
    if out_dir.exists() {
        for entry in fs::read_dir(&out_dir).expect("旧证据目录") {
            let path = entry.expect("目录项").path();
            let stale_png = path.extension().is_some_and(|extension| extension == "png");
            let stale_manifest = matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("live-manifest.json" | "live-run.log")
            );
            if stale_png || stale_manifest {
                fs::remove_file(path).expect("删除旧证据");
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
    // 扁平计划索引保留作对照，改名并说明“不是实际渲染帧”。
    assert_ne!(attack1.planned_flat_index, attack3.planned_flat_index);
    assert_eq!(
        attack1.planned_flat_index, 216,
        "扁平计划 Right=2, Attack1 body=200+16"
    );
    assert_eq!(
        attack3.planned_flat_index, 344,
        "扁平计划 Right=2, Attack3 body=328+16"
    );
    assert_eq!(attack1.planned_flat_attack_index, Some(216));
    assert_eq!(attack3.planned_flat_attack_index, Some(344));

    // R48 起实际渲染走原生库：Attack1/Attack3 的 Start 136/232，Right=2 的库内
    // 方向索引 148/248，再叠加普通女性外观列偏移 808 → body 帧 956/1056。
    let attack1_native = &attack1.native;
    assert_eq!(attack1_native.path, "native", "标准角色应走原生路径");
    assert_eq!(attack1_native.action.as_deref(), Some("Attack1"));
    assert_eq!(attack1_native.body_library.as_deref(), Some("carmour-00"));
    assert_eq!(
        attack1_native.direction_index, 148,
        "Attack1 Right=2: 136+2*6"
    );
    assert_eq!(attack1_native.armour_offset, 808, "普通女性外观列偏移");
    assert_eq!(attack1_native.body_frame, Some(956));
    let attack3_native = &attack3.native;
    assert_eq!(attack3_native.path, "native");
    assert_eq!(attack3_native.action.as_deref(), Some("Attack3"));
    assert_eq!(attack3_native.body_library.as_deref(), Some("carmour-00"));
    assert_eq!(
        attack3_native.direction_index, 248,
        "Attack3 Right=2: 232+2*8"
    );
    assert_eq!(attack3_native.body_frame, Some(1056));
    let male_attack1 = samples
        .iter()
        .find(|sample| sample.action == "male-attack1-right" && sample.frame_index == 0)
        .expect("male Attack1 像素");
    assert_eq!(male_attack1.native.path, "native");
    assert_eq!(male_attack1.native.action.as_deref(), Some("Attack1"));
    assert_eq!(male_attack1.native.armour_offset, 0, "男性外观列偏移");
    assert_eq!(male_attack1.native.body_frame, Some(148));

    // Standing 的扁平计划 body 帧：Down=32，Right=16；不是攻击序列的 200/328。
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
        assert_eq!(sample.planned_flat_attack_index, None);
        assert_eq!(sample.logical_index, None);
        assert_eq!(sample.native.action.as_deref(), Some("Standing"));
        // camera::prepare_frame 内的 WorldSnapshot::tick_animations 把播完的攻击切回
        // Standing 时把 action_started_ms 归一到当前 now，所以原生 Standing 从 local=0 起。
        assert_eq!(sample.native.elapsed_ms, 0);
        assert_eq!(sample.native.local, 0);
    }
    assert_eq!(
        down_end.planned_flat_index, 32,
        "Down Standing 扁平计划 body=32"
    );
    assert_eq!(
        right_end.planned_flat_index, 16,
        "Right Standing 扁平计划 body=16"
    );
    assert_eq!(
        male_end.planned_flat_index, 16,
        "Male Right Standing 扁平计划 body=16"
    );
    // 原生 Standing 实际渲染帧 = 外观列偏移 + direction*4 + local(0)：
    // body 824/816、male 8；与同批 25 张 PNG 的 native body 层一一对应。
    assert_eq!(down_end.native.body_frame, Some(824), "Down=4: 808+4*4+0");
    assert_eq!(right_end.native.body_frame, Some(816), "Right=2: 808+2*4+0");
    assert_eq!(
        male_end.native.body_frame,
        Some(8),
        "Male Right: 808→0, 2*4+0"
    );

    // 冻结目录只读校验：本测试不再写入/覆盖 docs/testing/attack-variants/。
    let frozen_after = directory_fingerprint(&frozen_dir);
    assert_eq!(
        frozen_before,
        frozen_after,
        "冻结证据目录必须零变化：{}（本测试只读该目录）",
        frozen_dir.display()
    );

    let generated_at = now_millis();
    let evidence_relative = out_dir
        .strip_prefix(&root)
        .unwrap_or(&out_dir)
        .display()
        .to_string();
    let sample_json: Vec<serde_json::Value> = samples
        .iter()
        .map(|sample| {
            serde_json::json!({
                "action": sample.action,
                "variant": sample.variant.as_str(),
                "frame_index": sample.frame_index,
                "elapsed_ms": sample.elapsed_ms,
                "simulated_now_ms": sample.simulated_now_ms,
                "wall_captured_ms": sample.wall_captured_ms,
                "source_started_ms": sample.source_started_ms,
                "logical_index": sample.logical_index,
                "planned_flat_attack_index": sample.planned_flat_attack_index,
                "planned_flat_index": sample.planned_flat_index,
                "planned_flat_index_semantics": "旧扁平版式计划 body 帧（含外观块基址）；不是实际渲染帧，实际值见 native_selection。",
                "render_path": sample.native.path,
                "native_selection": sample.native.to_json(),
                "action_now": sample.action_now.as_str(),
                "view_variant": sample.view_variant.map(AttackVariant::as_str),
                "camera_center": [sample.camera_center.x, sample.camera_center.y],
                "self_screen": [sample.self_screen.0, sample.self_screen.1],
                "planned_flat_layers": sample
                    .planned_flat_layers
                    .iter()
                    .map(|(name, frame)| serde_json::json!({"layer": name, "frame": frame}))
                    .collect::<Vec<_>>(),
                "png": sample.png,
                "pixel_hash": format!("{:016x}", sample.pixel_hash),
                "png_byte_hash": format!("{:016x}", sample.png_byte_hash),
                "distinct_colors": sample.distinct_colors,
            })
        })
        .collect();
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
        "renderer": "crates/mir-render/src/entities.rs draw_player 优先 draw_native_player（R48+）：player_layout + native_action + NativeFrameTable + 外观列偏移 + sequence.index_of(direction, local)；原生身体库缺失时才回退扁平版式",
        "index_semantics": {
            "planned_flat_index": "旧扁平 hum*.wzl 计划版的 body 帧（外观块基址 + 扁平攻击/站立序列索引）。R48 起渲染器默认走原生玩家库，此字段仅作对照，不是实际渲染帧；历史文档里的 actual_index 就是它。",
            "planned_flat_attack_index": "扁平素材攻击序列的库内帧号（不含外观块基址）；非攻击（已回 Standing）为 null。",
            "render_path": "实际渲染路径：native = draw_native_player；flat-fallback = view 无 player_layout 或原生身体库缺失时回退旧扁平版式。",
            "native_selection": "原生路径实际选中的动作/序列/局部帧/外观列偏移/逐层库与帧。帧号 = sequence.index_of(direction, local) + 该层外观列偏移（普通女性 armour/hair=808、weapon=416、wing=840；男性=0）。",
            "native_start_right_baseline": "Attack1: Start 136 + Right(2)*6 = 148；Attack3: Start 232 + Right(2)*8 = 248；叠加普通女性外观列偏移 808 后 body 帧为 956/1056，男性为 148/248。",
        },
        "evidence": {
            "output_dir": evidence_relative,
            "output_dir_env": EVIDENCE_DIR_ENV,
            "tracked": false,
            "note": "证据只写不入库的 target/ 目录；R47 冻结目录 docs/testing/attack-variants/ 只读参照，运行前后字节指纹一致。",
            "frozen_r47": {
                "path": FROZEN_EVIDENCE_DIR,
                "entries": frozen_before.len(),
                "fingerprint_unchanged": frozen_before == frozen_after,
                "on_disk_manifest_batch_id": frozen_batch_id,
                "restore_note": "若旧目录被历史全量门禁刷新过，恢复 tracked 原样由队长执行 git checkout -- docs/testing/attack-variants/（本测试不写该目录）。",
            },
        },
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
        "samples": sample_json,
    });
    let manifest_path = out_dir.join("live-manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).expect("manifest"),
    )
    .expect("写 manifest");

    // 同一批：run log、manifest、25 张 PNG 都使用同一个 batch_id，且都在非入库目录。
    let mut log = format!(
        "batch_id={generated_at}\ntest=two_clients_attack1_attack3_live_evidence\nsamples={}\nevidence_dir={}\n",
        samples.len(),
        evidence_relative
    );
    for sample in &samples {
        let file_name = Path::new(&sample.png)
            .file_name()
            .expect("PNG 文件名")
            .to_string_lossy();
        writeln!(
            log,
            "{} frame={} render={} planned_flat={} planned_flat_attack={:?} native_body={:?} native_action={} logical={:?} png={} png_byte_hash={:016x}",
            sample.action,
            sample.frame_index,
            sample.native.path,
            sample.planned_flat_index,
            sample.planned_flat_attack_index,
            sample.native.body_frame,
            sample.native.action.as_deref().unwrap_or("-"),
            sample.logical_index,
            file_name,
            sample.png_byte_hash
        )
        .expect("格式化 live run log");
    }
    let log_path = out_dir.join("live-run.log");
    fs::write(&log_path, &log).expect("写 live run log");

    println!(
        "PASS attack-variant live：batch={generated_at}；female {female_id}/male {male_id}；{} 帧；render_path={}；manifest {}；证据目录 {}（R47 冻结目录只读未变）",
        samples.len(),
        samples.first().map_or("-", |sample| sample.native.path),
        manifest_path.display(),
        out_dir.display()
    );
    drop((female_client, male_client, runtime));
}
