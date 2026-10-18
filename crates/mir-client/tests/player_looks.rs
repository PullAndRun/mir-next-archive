//! 注入式纸娃娃测试：装备外观 → 快照 → 渲染（`target/client-looks.png`）。
//!
//! 不连服务器，直接用 [`WorldSnapshot::apply_packet`] 注入：
//!
//! 1. `S.UserInformation`（带武器/衣服/发型）与 `S.NewItemInfo`（带 `shape` 的模板）；
//! 2. `S.ObjectPlayer`（别人穿什么就画什么）；
//! 3. `S.EquipItem`（换武器 → 外观必须刷新）；
//!
//! 然后把「自己 + 路人」渲染到 `target/client-looks.png`，并断言：
//!
//! * 快照推导出的 `armour / weapon / weapon_shape / hair` 与模板一致；
//! * `EntityView` 带上了同样的层号（渲染层的唯一输入）；
//! * 换装前后的两帧在角色区域**不同**（说明装备层真的被重画）；
//! * 整帧不是空白（有真实素材时是 sprite，没有素材时是占位方块）。

use std::path::{Path, PathBuf};

use mir_client::geom::Rect;
use mir_client::net::{EQUIPMENT_SLOTS, WorldSnapshot};
use mir_client::ui::{self, UiState, UiText};
use mir_core::Language;
use mir_core::enums::{EquipmentSlot, ItemType, MirClass, MirDirection, MirGender};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ItemInfo, UserItem};
use mir_protocol::packets::server::{EquipItem, NewItemInfo, ObjectPlayer, UserInformation};
use mir_render::{Frame, MapScene, NO_LAYER, TileLibraries, Viewport};
use mir_world::{Map, MapCell, MapVersion};

/// 自己站的位置（合成地图与真实 `0.map` 都用这个中心）。
const POSITION: mir_core::Point = mir_core::Point::new(330, 270);

/// 工作区根目录（`CARGO_MANIFEST_DIR` 是 `crates/mir-client`）。
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// 无 `resource/` 时的合成空地图（与其它无头测试相同）。
fn synthetic_map() -> Map {
    let width = 64;
    let height = 64;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "synthetic-looks".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
}

fn user_item(unique_id: u64, item_index: i32) -> UserItem {
    UserItem {
        unique_id,
        item_index,
        count: 1,
        current_dura: 5_000,
        max_dura: 5_000,
        ..UserItem::default()
    }
}

fn item_info(index: i32, name: &str, item_type: ItemType, shape: i16, image: u16) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        item_type,
        shape,
        image,
        durability: 5_000,
        price: 1_000,
        ..ItemInfo::default()
    }
}

/// 注入「自己 + 两个路人 + 装备模板」。
fn injected_world() -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();

    // 先到模板，再到装备：外观在 `UserInformation` 到达时就该算好。
    for info in [
        item_info(30, "炼狱", ItemType::Weapon, 36, 30),
        item_info(31, "天魔神甲", ItemType::Armour, 2, 31),
        item_info(32, "屠龙", ItemType::Weapon, 210, 32),
    ] {
        snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo { info }));
    }

    let mut equipment = vec![None; EQUIPMENT_SLOTS];
    equipment[EquipmentSlot::Weapon as usize] = Some(user_item(1_001, 30));
    equipment[EquipmentSlot::Armour as usize] = Some(user_item(1_002, 31));
    snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
        object_id: 7,
        real_id: 1,
        name: "纸娃娃".to_owned(),
        location: POSITION,
        direction: MirDirection::Down,
        class: MirClass::Warrior,
        gender: MirGender::Male,
        level: 22,
        hair: 2,
        hp: 88,
        mp: 30,
        gold: 1_234,
        equipment: Some(equipment),
        inventory: Some(vec![Some(user_item(1_003, 32))]),
        ..UserInformation::default()
    }));

    // 路人甲朝上（武器画在身体之后，会被挡住）、路人乙朝下（武器画在最前面）。
    for (object_id, name, direction, armour, weapon) in [
        (101u32, "路人甲", MirDirection::Up, 3i16, 7i16),
        (102, "路人乙", MirDirection::Down, 4, 8),
    ] {
        snapshot.apply_packet(&ServerPacket::ObjectPlayer(ObjectPlayer {
            object_id,
            name: name.to_owned(),
            class: MirClass::Taoist,
            gender: MirGender::Female,
            level: 20,
            location: mir_core::Point::new(
                POSITION.x + i32::try_from(object_id).unwrap_or(1) - 101,
                POSITION.y + 1,
            ),
            direction,
            hair: 3,
            armour,
            weapon,
            ..ObjectPlayer::default()
        }));
    }
    snapshot
}

/// 渲染一帧（地形 + 实体 + 装备面板），返回帧与场景。
fn render(snapshot: &WorldSnapshot, root: &Path) -> Frame {
    let map_dir = root.join("resource/map");
    let data_dir = root.join("resource/data");
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
    let mut frame = Frame::new(640, 480);
    let entities = snapshot.entity_views();
    scene.render_with_entities(&mut frame, POSITION, &entities);
    let text = UiText::load(None);
    let mut state = UiState::new(Language::ZhCn);
    state.show_equipment = true;
    state.mouse = mir_core::Point::new(1, 1);
    ui::draw(
        &mut state, &mut frame, &mut scene, snapshot, &text, None, POSITION,
    );
    frame
}

/// 统计帧里非纯黑的像素数。
fn non_black(frame: &Frame) -> usize {
    frame
        .pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0)
        .count()
}

/// 统计两个帧在矩形里的差异像素数。
fn changed_pixels(before: &Frame, after: &Frame, rect: Rect) -> usize {
    let mut count = 0;
    for y in rect.y.max(0)
        ..rect
            .bottom()
            .min(i32::try_from(after.height).unwrap_or(i32::MAX))
    {
        for x in rect.x.max(0)
            ..rect
                .right()
                .min(i32::try_from(after.width).unwrap_or(i32::MAX))
        {
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

#[test]
fn equipment_layers_reach_the_snapshot_and_the_frame() {
    let root = workspace_root();
    let mut snapshot = injected_world();

    // 1. 快照推导：武器 36（weapon.wzl）、衣服 2（hum.wzl）、发型 2（hair.wzl）。
    let looks = snapshot.self_player.as_ref().expect("自己").looks;
    assert_eq!(looks.armour, 2, "衣服外观号来自 ItemInfo.Shape");
    assert_eq!(looks.weapon, 36);
    assert_eq!(looks.weapon_shape, 36);
    assert_eq!(looks.hair, 2);
    assert_eq!(looks.shield, NO_LAYER, "没有盾牌物品类型");

    // 2. 渲染层的唯一输入也必须带上这些层号。
    let views = snapshot.entity_views();
    assert_eq!(views.len(), 3, "自己 + 两个路人");
    let me = &views[0];
    assert_eq!(
        (me.armour, me.weapon, me.weapon_shape, me.hair),
        (2, 36, 36, 2)
    );
    let other = views
        .iter()
        .find(|view| view.name.as_deref() == Some("路人甲"))
        .expect("路人甲");
    assert_eq!((other.armour, other.weapon, other.hair), (3, 7, 3));

    // 3. 换武器（背包里的 210 段武器 → 武器槽 0）：外观刷新 + 重绘后画面不同。
    let before = render(&snapshot, &root);
    snapshot.apply_packet(&ServerPacket::EquipItem(EquipItem {
        grid: mir_core::enums::MirGridType::Inventory,
        unique_id: 1_003,
        to: EquipmentSlot::Weapon as i32,
        success: true,
    }));
    let looks = snapshot.self_player.as_ref().expect("自己").looks;
    assert_eq!(looks.weapon_shape, 210, "200 段武器");
    assert_eq!(looks.weapon, 10, "块号 = Shape % 100");
    assert_eq!(looks.armour, 2, "衣服不受影响");
    let view = &snapshot.entity_views()[0];
    assert_eq!((view.weapon, view.weapon_shape), (10, 210));

    let after = render(&snapshot, &root);
    let player_rect = Rect::new(POSITION.x, POSITION.y, 48, 96);
    let _ = player_rect;
    // 角色就在画面中心：取中心附近一块区域比较换装前后。
    let center_rect = Rect::new(640 / 2 - 40, 480 / 2 - 96, 80, 120);
    let changed = changed_pixels(&before, &after, center_rect);
    println!("换装前后中心区域变化 {changed} 像素");
    assert!(
        changed > 0,
        "换武器后角色区域必须重绘（真实素材下武器 sprite 变了）"
    );

    // 4. 保存 PNG 并断言非空白。
    let output = root.join("target/client-looks.png");
    after
        .save_png(&output)
        .expect("写出 target/client-looks.png");
    let metadata = std::fs::metadata(&output).expect("PNG 应存在");
    assert!(metadata.len() > 2_048, "PNG 不应为空文件");
    let painted = non_black(&after);
    assert!(painted > 5_000, "非黑像素太少：{painted}");
    assert!(
        after.distinct_color_count() >= 20,
        "整帧颜色太少：{}",
        after.distinct_color_count()
    );

    // 5. 装备面板确实画在帧里（面板区域不是纯黑）。
    let panel = mir_client::ui::equipment::equipment_layout(Rect::screen(640, 480)).panel;
    let panel_pixels = {
        let mut count = 0;
        for y in panel.y.max(0)..panel.bottom().min(480) {
            for x in panel.x.max(0)..panel.right().min(640) {
                let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
                    continue;
                };
                if let Some(pixel) = after.pixel(x, y) {
                    if pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0 {
                        count += 1;
                    }
                }
            }
        }
        count
    };
    assert!(panel_pixels > 1_000, "装备面板应有内容：{panel_pixels}");

    println!(
        "client-looks.png：{} · {}×{} · {} 种颜色 · 非黑 {} px",
        output.display(),
        after.width,
        after.height,
        after.distinct_color_count(),
        painted
    );
}
