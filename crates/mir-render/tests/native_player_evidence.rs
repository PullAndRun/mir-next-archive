//! R48 原生玩家默认路径证据：直接加载 `game-assets`（auto → dev/unpacked），
//! 用真实地图/原生库渲染 A1–A4 每一帧并落 PNG + manifest。
//!
//! 这不是网络 live 测试；网络动作选择/时序由 client 测试覆盖。此测试只证明
//! **默认素材路径**能逐帧画出原生身体、头发与武器层，不会悄悄回退
//! Standing/旧扁平库；已知的源素材缺帧会显式记录为 `weapon_frame_missing`。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use mir_asset::LibraryMap;
use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_render::animation::AttackVariant;
use mir_render::entities::{EntityKind, EntityView};
use mir_render::player_layout::{
    NativeAction, NativeDrawLayer, NativeFrameTable, NativeHand, NativeLayer, NativeLibrary,
    PlayerLayout, WeaponSegment, draw_order, frame_index,
};
use mir_render::{ActorAction, EntityLibraries, Frame, MapScene};

const CROP_LEFT: u32 = 50;
const CROP_RIGHT: u32 = 170;
const CROP_TOP: u32 = 30;
const CROP_BOTTOM: u32 = 260;

/// 测试内自带的 SHA-256（避免为证据测试新增运行时/测试依赖）。
#[allow(clippy::all, clippy::pedantic)]
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, slot) in w.iter_mut().take(16).enumerate() {
            let start = i * 4;
            *slot = u32::from_be_bytes([
                chunk[start],
                chunk[start + 1],
                chunk[start + 2],
                chunk[start + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|v| format!("{v:08x}")).collect()
}

fn crop_sha256(frame: &Frame, background: &Frame) -> (String, u64) {
    let mut bytes = Vec::new();
    let mut changed = 0u64;
    for y in CROP_TOP..CROP_BOTTOM {
        for x in CROP_LEFT..CROP_RIGHT {
            let pixel = frame.pixel(x, y).unwrap_or([0, 0, 0, 0]);
            let base = background.pixel(x, y).unwrap_or([0, 0, 0, 0]);
            bytes.extend_from_slice(&pixel);
            if pixel != base {
                changed += 1;
            }
        }
    }
    (sha256_hex(&bytes), changed)
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn source_kind(source: &mir_asset::LibrarySource) -> &'static str {
    match source {
        mir_asset::LibrarySource::Unpacked(_) | mir_asset::LibrarySource::UnpackedDirs(_) => {
            "unpacked"
        }
        _ => "pack",
    }
}

#[test]
#[ignore = "需要 game-assets 原生玩家拆包；生成 docs/testing/native-player-layout 证据"]
fn render_default_native_player_a1_to_a4_frames() {
    let workspace = workspace_root();
    let root = workspace.join("game-assets");
    let map = LibraryMap::load(root.join("library-map.json")).expect("dev library map");
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
    let mut libraries = EntityLibraries::open_with_sources(
        root.join("data"),
        Some(&root.join("gfx")),
        Some(&root),
        Some(&map),
        None,
        false,
    );

    let out_dir = workspace.join("docs/testing/native-player-layout");
    std::fs::create_dir_all(&out_dir).expect("create evidence dir");
    for entry in std::fs::read_dir(&out_dir).expect("read evidence dir") {
        let path = entry.expect("evidence entry").path();
        if path.extension().is_some_and(|ext| ext == "png")
            || path.file_name().is_some_and(|name| name == "manifest.json")
        {
            std::fs::remove_file(&path).expect("remove stale evidence");
        }
    }
    let started = 1_000u64;
    let mut samples = Vec::new();
    let mut png_hashes = HashSet::new();
    let mut frame_hashes = HashSet::new();
    let mut previous_frame: Option<(String, String)> = None;

    let cases = [
        (
            MirClass::Warrior,
            NativeAction::Attack1,
            0u16,
            MirDirection::Down,
            false,
        ),
        (
            MirClass::Warrior,
            NativeAction::Attack2,
            0,
            MirDirection::Down,
            false,
        ),
        (
            MirClass::Warrior,
            NativeAction::Attack3,
            0,
            MirDirection::Down,
            false,
        ),
        (
            MirClass::Warrior,
            NativeAction::Attack4,
            0,
            MirDirection::Down,
            false,
        ),
        (
            MirClass::Wizard,
            NativeAction::Attack1,
            0,
            MirDirection::Down,
            false,
        ),
        (
            MirClass::Wizard,
            NativeAction::Attack3,
            0,
            MirDirection::Down,
            false,
        ),
        (
            MirClass::Taoist,
            NativeAction::Attack1,
            0,
            MirDirection::Down,
            false,
        ),
        (
            MirClass::Taoist,
            NativeAction::Attack3,
            0,
            MirDirection::Down,
            false,
        ),
        (
            MirClass::Assassin,
            NativeAction::Attack1,
            100,
            MirDirection::Down,
            false,
        ),
        (
            MirClass::Assassin,
            NativeAction::Attack2,
            100,
            MirDirection::Down,
            false,
        ),
        (
            MirClass::Assassin,
            NativeAction::Attack3,
            100,
            MirDirection::Down,
            false,
        ),
        (
            MirClass::Assassin,
            NativeAction::Attack4,
            100,
            MirDirection::Down,
            false,
        ),
        (
            MirClass::Archer,
            NativeAction::Attack1,
            200,
            MirDirection::Down,
            false,
        ),
        (
            MirClass::Archer,
            NativeAction::Attack2,
            200,
            MirDirection::Down,
            false,
        ),
        (
            MirClass::Archer,
            NativeAction::Attack3,
            200,
            MirDirection::Down,
            false,
        ),
        // 生产语义：普通 Attacking 走 A1/alt=false；Walking 持弓走 WalkingBow/alt=true。
        (
            MirClass::Archer,
            NativeAction::Attack1,
            200,
            MirDirection::Up,
            true,
        ),
        (
            MirClass::Archer,
            NativeAction::Attack1,
            200,
            MirDirection::Left,
            true,
        ),
        (
            MirClass::Archer,
            NativeAction::Attack1,
            200,
            MirDirection::Down,
            true,
        ),
        (
            MirClass::Archer,
            NativeAction::Attack1,
            200,
            MirDirection::Right,
            true,
        ),
        (
            MirClass::Archer,
            NativeAction::WalkingBow,
            200,
            MirDirection::Left,
            true,
        ),
        (
            MirClass::Archer,
            NativeAction::WalkingBow,
            200,
            MirDirection::Right,
            true,
        ),
    ];

    for (class, action, weapon_shape, direction, production) in cases {
        for gender in [MirGender::Male, MirGender::Female] {
            let (layout, actor_action) = if production {
                let actor_action = match action {
                    NativeAction::WalkingBow => ActorAction::Walking,
                    NativeAction::RunningBow => ActorAction::Running,
                    _ => ActorAction::Attacking,
                };
                let variant =
                    (actor_action == ActorAction::Attacking).then_some(AttackVariant::Attack1);
                let layout = PlayerLayout::for_action(
                    class,
                    gender,
                    actor_action,
                    variant,
                    weapon_shape,
                    false,
                )
                .expect("production native layout");
                (layout, actor_action)
            } else {
                let alt_anim = matches!(class, MirClass::Assassin | MirClass::Archer);
                (
                    PlayerLayout::new(class, gender, alt_anim, weapon_shape),
                    ActorAction::Attacking,
                )
            };
            let alt_anim = layout.alt_anim;
            let id = layout.library_id();
            let sequence = NativeFrameTable::get(action);
            let label = format!(
                "{}-{}-{}-{}-{}-{}",
                format!("{class:?}").to_lowercase(),
                format!("{gender:?}").to_lowercase(),
                if alt_anim { "alt" } else { "plain" },
                format!("{action:?}").to_lowercase(),
                format!("{direction:?}").to_lowercase(),
                if production { "prod" } else { "manual" }
            );
            if production {
                assert_eq!(class, MirClass::Archer);
                let body = id
                    .library_key(NativeLayer::Armour, 0)
                    .expect("production archer body");
                let main = id.library_key(NativeLayer::Weapon(NativeHand::Main), weapon_shape);
                let off = id
                    .library_key(NativeLayer::Weapon(NativeHand::Off), weapon_shape)
                    .expect("production archer WeaponLibrary2");
                assert!(main.is_none(), "Crystal 弓手 WeaponLibrary1=null");
                assert!(
                    body.starts_with("carmour") || body.starts_with("ararmour"),
                    "body domain"
                );
                assert!(off.starts_with("arweapon"));
                if action == NativeAction::Attack1 {
                    assert!(!alt_anim, "普通弓箭攻击必须是 alt=false");
                    assert!(body.starts_with("carmour"));
                    assert_eq!(off, "arweapon-00");
                } else {
                    assert!(alt_anim, "WalkingBow 必须是 alt=true");
                    assert!(body.starts_with("ararmour"));
                    assert_eq!(off, "arweapon-00-s");
                }
            }
            for frame_index_in_action in 0..sequence.count {
                let mut view =
                    EntityView::new(Point::new(330, 270), direction, EntityKind::Player, 0);
                view.class = class;
                view.gender = gender;
                view.armour = 0;
                view.weapon = 0;
                view.weapon_shape = weapon_shape;
                view.hair = 0;
                view.action = actor_action;
                view.action_started_ms = started;
                view.native_action = Some(action);
                view.player_layout = Some(layout);
                let now = started + u64::from(frame_index_in_action) * 100;
                let mut frame = Frame::new(220, 280);
                scene.render_with_entities_at_offset(
                    &mut frame,
                    Point::new(330, 270),
                    (0, 0),
                    std::slice::from_ref(&view),
                    now,
                );
                let mut background = Frame::new(220, 280);
                scene.render_with_entities_at_offset(
                    &mut background,
                    Point::new(330, 270),
                    (0, 0),
                    &[],
                    now,
                );
                let (crop_hash, character_pixels) = crop_sha256(&frame, &background);
                assert!(
                    character_pixels > 20,
                    "{label} frame {frame_index_in_action} 角色裁剪区只有 {character_pixels} 个变化像素"
                );
                if let Some((prev_label, prev_hash)) = &previous_frame {
                    if prev_label == &label {
                        assert_ne!(
                            prev_hash, &crop_hash,
                            "{label} frame {frame_index_in_action} 与前一帧哈希相同"
                        );
                    }
                }
                previous_frame = Some((label.clone(), crop_hash.clone()));
                assert!(
                    frame.distinct_color_count() > 1,
                    "{label} frame {frame_index_in_action} 不应只有背景"
                );
                let png = out_dir.join(format!("{label}-frame{frame_index_in_action}.png"));
                frame.save_png(&png).expect("save native evidence png");
                let png_hash = sha256_hex(&std::fs::read(&png).expect("read evidence png"));
                png_hashes.insert(png_hash.clone());
                frame_hashes.insert(crop_hash.clone());
                let body_index = frame_index(
                    sequence,
                    direction,
                    frame_index_in_action,
                    id.armour_offset(),
                );
                let body_key = id
                    .library_key(NativeLayer::Armour, 0)
                    .expect("native body key");
                let body_source = {
                    let source =
                        NativeLibrary::open(&mut libraries, &body_key).expect("body library");
                    source_kind(source)
                };
                let info = NativeLibrary::open(&mut libraries, &body_key)
                    .expect("body library")
                    .info(body_index as usize)
                    .expect("body frame in range");
                assert!(
                    !info.is_empty(),
                    "{body_key} frame {body_index} 为空，原生路径不能回退 Standing"
                );
                let hair_key = id
                    .library_key(NativeLayer::Hair, 0)
                    .expect("native hair key");
                let hair_index =
                    frame_index(sequence, direction, frame_index_in_action, id.hair_offset());
                let hair_info = NativeLibrary::open(&mut libraries, &hair_key)
                    .expect("hair library")
                    .info(hair_index as usize)
                    .expect("hair frame in range");
                assert!(!hair_info.is_empty(), "{hair_key} frame {hair_index} 为空");
                let main_key = id.library_key(NativeLayer::Weapon(NativeHand::Main), weapon_shape);
                let off_key = id.library_key(NativeLayer::Weapon(NativeHand::Off), weapon_shape);
                let (weapon_hand, weapon_key) = match (main_key, off_key) {
                    (Some(key), _) => ("main", key),
                    (None, Some(key)) => ("off", key),
                    (None, None) => panic!("native weapon key missing for {class:?}"),
                };
                let weapon_index = frame_index(
                    sequence,
                    direction,
                    frame_index_in_action,
                    id.weapon_offset(),
                );
                let weapon_info = NativeLibrary::open(&mut libraries, &weapon_key)
                    .expect("weapon library")
                    .info(weapon_index as usize);
                let weapon_missing = match &weapon_info {
                    Ok(info) => info.is_empty(),
                    Err(_) => true,
                };
                let known_gender_limit = action == NativeAction::Attack4
                    && layout.weapon_segment == WeaponSegment::Common
                    && gender == MirGender::Female;
                assert!(
                    !weapon_missing || known_gender_limit,
                    "{weapon_key} frame {weapon_index} 为空（仅女性 C 系 A4 越界允许）"
                );
                let is_archer_weapon =
                    class == MirClass::Archer && layout.weapon_segment == WeaponSegment::Archer;
                let (order, order_len) = draw_order(
                    direction,
                    weapon_hand == "main",
                    weapon_hand == "off",
                    is_archer_weapon,
                );
                let draw_order: Vec<String> = order[..order_len]
                    .iter()
                    .map(|layer| match layer {
                        NativeDrawLayer::Body => "body".to_owned(),
                        NativeDrawLayer::Head => "head".to_owned(),
                        NativeDrawLayer::Wing => "wing".to_owned(),
                        NativeDrawLayer::Weapon(NativeHand::Main) => "weapon-main".to_owned(),
                        NativeDrawLayer::Weapon(NativeHand::Off) => "weapon-off".to_owned(),
                    })
                    .collect();
                if production {
                    assert_eq!(
                        body_source, "unpacked",
                        "默认 auto→dev 路径必须来自解包目录"
                    );
                    assert_eq!(weapon_hand, "off");
                    assert!(
                        draw_order
                            .iter()
                            .filter(|layer| layer.as_str() == "weapon-off")
                            .count()
                            == 2,
                        "弓手每个方向都额外 DrawWeapon2：{direction:?}"
                    );
                }
                samples.push(serde_json::json!({
                    "label": label.clone(),
                    "class": format!("{class:?}"),
                    "gender": format!("{gender:?}"),
                    "alt_anim": alt_anim,
                    "production": production,
                    "action": format!("{action:?}"),
                    "direction": format!("{direction:?}"),
                    "frame": frame_index_in_action,
                    "elapsed_ms": frame_index_in_action * 100,
                    "body_frame_index": body_index,
                    "body_library": body_key,
                    "body_source": body_source,
                    "hair_library": hair_key,
                    "hair_frame_index": hair_index,
                    "weapon_library": weapon_key,
                    "weapon_hand": weapon_hand,
                    "weapon_frame_index": weapon_index,
                    "weapon_frame_missing": weapon_missing,
                    "draw_order": draw_order,
                    "png": png.strip_prefix(&workspace).unwrap_or(&png).display().to_string(),
                    "png_sha256": png_hash,
                    "character_crop_sha256": crop_hash,
                    "character_non_background_pixels": character_pixels,
                    "distinct_colors": frame.distinct_color_count(),
                }));
            }
        }
    }

    // 非 alt 的刺客/弓手必须落 C 系，不能拿 A/AR 偏移去越界。
    for (class, expected) in [
        (MirClass::Assassin, "carmour-00"),
        (MirClass::Archer, "carmour-00"),
    ] {
        let layout = PlayerLayout::new(class, MirGender::Female, false, 0);
        let key = layout
            .library_id()
            .library_key(mir_render::player_layout::NativeLayer::Armour, 0)
            .expect("plain armour key");
        assert_eq!(key, expected);
    }

    let generated_at = now_millis();
    assert!(
        png_hashes.len() > 100,
        "证据 PNG 唯一哈希只有 {}，不足 100",
        png_hashes.len()
    );
    let manifest = serde_json::json!({
        "batch": generated_at,
        "generated_at_unix_ms": generated_at,
        "asset_root": "game-assets",
        "asset_mode": "auto -> dev/unpacked",
        "note": "renderer-only default-path evidence; no NetClient/server claim",
        "unique_png_sha256": png_hashes.len(),
        "unique_character_crop_sha256": frame_hashes.len(),
        "samples": samples,
    });
    let manifest_path = out_dir.join("manifest.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).expect("manifest json"),
    )
    .expect("write manifest");
    eprintln!(
        "native evidence: {} samples -> {}",
        samples.len(),
        manifest_path.display()
    );
}
