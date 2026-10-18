//! 无头界面测试：真实登录 + 注入假 NPC/商店/角色数据，把整屏 UI 画到 PNG。
//!
//! 流程：
//!
//! 1. 进程内起真实 `mir-server`，客户端走完「注册 → 登录 → 建角 → 进世界」；
//! 2. 用 `WorldSnapshot::apply_packet` 注入假的 `UserInformation`（背包/技能/金币）、
//!    `NewItemInfo`、`ObjectNPC`、`NPCResponse`、`NPCGoods` 与几条聊天；
//! 3. 打开背包/技能面板，渲染地图 + 全部面板到 `target/client-ui.png`；
//! 4. 断言：整帧不是空白、HUD 文字像素存在、面板确实覆盖了地图、按钮/格子/货架
//!    的命中测试返回正确的动作。
//!
//! 测试不需要窗口、字体或 `resource/`：没有字体时用 3×5 点阵兜底，没有地图素材时
//! 用合成空地图。所有等待都有超时。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use mir_client::geom::Rect;
use mir_client::net::{ConnectOptions, CreateCharacter, NetClient, WorldSnapshot};
use mir_client::ui::dialog::ParsedPage;
use mir_client::ui::{self, UiAction, UiState, UiText, hud, inventory, shop};
use mir_core::Language;
use mir_core::enums::{ChatType, MirClass, MirDirection, MirGender, PanelType, Spell};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ClientMagic, ItemInfo, UserItem};
use mir_protocol::packets::server::{
    Chat, ItemRepaired, NPCGoods, NPCResponse, NewItemInfo, ObjectNPC, UserInformation,
};
use mir_render::{Frame, MapScene, TileLibraries, Viewport};
use mir_server::{MemoryStore, ServerConfig, serve_forever};
use mir_world::{Map, MapCell, MapVersion};

/// 登录链路等待上限。
const LOGIN_TIMEOUT: Duration = Duration::from_secs(15);
/// 测试用的 NPC 对象 ID（假注入，不与真实对象冲突）。
const TEST_NPC_ID: u32 = 90_001;
/// 商店货架物品的唯一编号。
const SHOP_ITEM_ID: u64 = 501;

/// 工作区根目录（`CARGO_MANIFEST_DIR` 是 `crates/mir-client`）。
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// 无 `resource/` 时的合成地图（与 `live_world.rs` 相同）。
fn synthetic_map() -> Map {
    let width = 64;
    let height = 64;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "synthetic-ui".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
}

fn user_item(unique_id: u64, item_index: i32, count: u16, dura: u16) -> UserItem {
    UserItem {
        unique_id,
        item_index,
        count,
        current_dura: dura,
        max_dura: dura,
        ..UserItem::default()
    }
}

fn item_info(index: i32, name: &str, image: u16, price: u32, stack_size: u16) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        image,
        price,
        stack_size,
        ..ItemInfo::default()
    }
}

fn magic(name: &str, spell: Spell, key: u8, level: u8, cost: u8) -> ClientMagic {
    ClientMagic {
        name: name.to_owned(),
        spell,
        base_cost: cost,
        level,
        key,
        range: 3,
        ..ClientMagic::default()
    }
}

/// 往快照里注入一套「看起来像在 NPC 商店里」的假数据。
fn inject_fake_world(snapshot: &mut WorldSnapshot) {
    let position = snapshot
        .self_player
        .as_ref()
        .map_or(mir_core::Point::new(330, 270), |me| me.position);

    // 假的角色资料：等级/金币/背包/技能。
    let information = UserInformation {
        object_id: snapshot.self_player.as_ref().map_or(1, |me| me.object_id),
        real_id: 1,
        name: "界面测试".to_owned(),
        location: position,
        direction: MirDirection::Down,
        class: MirClass::Taoist,
        gender: MirGender::Female,
        level: 12,
        hair: 1,
        hp: 42,
        mp: 30,
        gold: 4_321,
        inventory: Some(vec![
            Some(user_item(11, 1, 1, 5_000)),
            Some(user_item(12, 2, 17, 0)),
            None,
            Some(user_item(13, 3, 1, 0)),
        ]),
        magics: vec![
            magic("治愈术", Spell::Healing, 1, 2, 5),
            magic("火球术", Spell::FireBall, 2, 3, 3),
        ],
        ..UserInformation::default()
    };
    snapshot.apply_packet(&ServerPacket::UserInformation(information));

    // 物品模板。
    snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
        info: item_info(1, "木剑", 1, 1_200, 1),
    }));
    snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
        info: item_info(2, "金创药", 2, 50, 100),
    }));
    snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
        info: item_info(3, "回城卷", 3, 100, 1),
    }));

    // 假 NPC：对话按钮需要 object_id，服务端 NPCResponse 不带它，客户端取最近的 NPC。
    snapshot.apply_packet(&ServerPacket::ObjectNPC(ObjectNPC {
        object_id: TEST_NPC_ID,
        name: "测试商人".to_owned(),
        image: 5,
        location: position.moved(MirDirection::Right, 1),
        direction: MirDirection::Left,
        ..ObjectNPC::default()
    }));

    // 带按钮与颜色标记的对话页。
    snapshot.apply_packet(&ServerPacket::NPCResponse(NPCResponse {
        page: vec![
            "欢迎光临，{测试商人/Red}！".to_owned(),
            "你要买点什么？<买东西/@BUYSELL>".to_owned(),
            "也可以<离开/@exit>。".to_owned(),
        ],
    }));
    snapshot.apply_packet(&ServerPacket::NPCGoods(NPCGoods {
        list: vec![
            user_item(SHOP_ITEM_ID, 1, 1, 5_000),
            user_item(502, 2, 5, 0),
        ],
        rate: 0.9,
        type_: PanelType::Buy,
        hide_added_stats: true,
    }));

    // 聊天面板内容。
    for (message, type_) in [
        ("欢迎来到传奇-Next", ChatType::System),
        ("组队去祖玛寺庙吗？", ChatType::Normal),
        ("注意：前方有怪物", ChatType::Hint),
    ] {
        snapshot.apply_packet(&ServerPacket::Chat(Chat {
            message: message.to_owned(),
            type_,
        }));
    }

    // 修理包不会改变界面布局，只验证快照消费。
    snapshot.apply_packet(&ServerPacket::ItemRepaired(ItemRepaired {
        unique_id: 11,
        max_dura: 6_000,
        current_dura: 6_000,
    }));
    assert_eq!(
        snapshot.inventory_item(0).map(|item| item.max_dura),
        Some(6_000),
        "ItemRepaired 应更新持久度"
    );
}

/// 统计矩形里的「文字像素」数量。
///
/// 文字是白色的抗锯齿像素：深色面板上要求 g/b 都被提亮，红色生命条上则表现为
/// 红色底（G=48）被抬到 100 以上；纯白核心像素占比很小，所以阈值必须容忍抗锯齿。
fn text_pixels(frame: &Frame, rect: Rect) -> usize {
    let mut count = 0;
    let left = rect.x.max(0);
    let top = rect.y.max(0);
    let right = rect
        .right()
        .min(i32::try_from(frame.width).unwrap_or(i32::MAX));
    let bottom = rect
        .bottom()
        .min(i32::try_from(frame.height).unwrap_or(i32::MAX));
    for y in top..bottom {
        for x in left..right {
            if let Some([r, g, b, _]) =
                frame.pixel(u32::try_from(x).unwrap_or(0), u32::try_from(y).unwrap_or(0))
            {
                if r >= 150 && g >= 100 && b >= 100 {
                    count += 1;
                }
            }
        }
    }
    count
}

/// 统计两个帧里发生变化的像素数（限制在矩形内）。
fn changed_pixels(before: &Frame, after: &Frame, rect: Rect) -> usize {
    let mut count = 0;
    let left = rect.x.max(0);
    let top = rect.y.max(0);
    let right = rect
        .right()
        .min(i32::try_from(after.width).unwrap_or(i32::MAX));
    let bottom = rect
        .bottom()
        .min(i32::try_from(after.height).unwrap_or(i32::MAX));
    for y in top..bottom {
        for x in left..right {
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
fn headless_ui_renders_all_panels_and_dispatches_clicks() {
    let root = workspace_root();
    let map_dir = root.join("resource/map");
    let data_dir = root.join("resource/data");

    // 1. 真实服务端 + 登录链路。
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

    let client = NetClient::connect(ConnectOptions {
        server: address.to_string(),
        account: "uitestacct".to_owned(),
        password: "uitestpass".to_owned(),
        character: "UiHero".to_owned(),
        create: Some(CreateCharacter {
            name: "UiHero".to_owned(),
            class: MirClass::Taoist,
            gender: MirGender::Female,
        }),
        auto_register: true,
        manual_select: false,
    })
    .expect("启动客户端网络线程");
    let mut snapshot = client
        .wait_until_in_game(LOGIN_TIMEOUT)
        .expect("客户端应走完登录链路进入世界");
    assert!(snapshot.is_ready());

    // 2. 注入假数据（不需要真实 NPC）。
    inject_fake_world(&mut snapshot);
    assert!(
        snapshot
            .npc_dialog
            .as_ref()
            .is_some_and(|dialog| !dialog.lines.is_empty())
    );
    assert_eq!(
        snapshot.npc_goods.as_ref().map(|goods| goods.list.len()),
        Some(2)
    );
    assert_eq!(snapshot.magics.len(), 2);
    assert_eq!(snapshot.inventory_item(1).map(|item| item.count), Some(17));

    // 3. 场景 + 面板。
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
        .map_or(mir_core::Point::new(330, 270), |me| me.position);
    let mut frame = Frame::new(800, 600);
    let entities = snapshot.entity_views(0);
    scene.render_with_entities(&mut frame, center, &entities);
    let world_only = frame.clone();

    let text = UiText::load(None);
    let mut state = UiState::new(Language::ZhCn);
    state.show_inventory = true;
    state.show_skills = true;
    state.sync(&snapshot);
    assert!(state.dialog.is_some(), "NPCResponse 应生成对话页");
    assert!(state.shop_open, "NPCGoods 应打开商店面板");
    state.mouse = mir_core::Point::new(400, 300);
    ui::draw(
        &mut state,
        &mut frame,
        &mut scene,
        &snapshot,
        &text,
        Some("你好，传奇-Next"),
        center,
    );

    // 4. 保存 PNG。
    let output = root.join("target/client-ui.png");
    frame.save_png(&output).expect("写出 target/client-ui.png");
    let metadata = std::fs::metadata(&output).expect("PNG 应存在");
    assert!(metadata.len() > 4_096, "PNG 不应为空文件");

    // 5. 非空白 + 面板确实覆盖了地图。
    assert!(
        frame.distinct_color_count() >= 20,
        "整帧应有足够多的颜色，实际 {}",
        frame.distinct_color_count()
    );
    let non_black = frame
        .pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0)
        .count();
    assert!(non_black > 10_000, "非黑像素太少：{non_black}");
    let screen = Rect::screen(800, 600);
    let inventory_panel = inventory::inventory_panel_rect(screen);
    let inventory_changed = changed_pixels(&world_only, &frame, inventory_panel);
    assert!(
        inventory_changed > 100,
        "背包面板应覆盖地图像素，实际变化 {inventory_changed}"
    );
    let shop_panel = shop::shop_layout(screen, 2, 0, shop::SHOP_LEFT).panel;
    assert!(
        changed_pixels(&world_only, &frame, shop_panel) > 100,
        "商店面板应覆盖地图像素"
    );

    // 6. HUD 文字像素：HP 条里的数字（ASCII，任何字体都能画）。
    let status = hud::status_layout(screen);
    let hp_text = text_pixels(&frame, status.hp_bar);
    assert!(hp_text > 8, "生命条数字应有文字像素，实际 {hp_text}");
    if text.is_font() {
        // 有 TrueType 字体时名字/等级（中文）也必须是可见像素。
        let name_text = text_pixels(&frame, status.name);
        assert!(name_text > 10, "名字区域应有中文文字像素，实际 {name_text}");
    }

    // 7. 命中测试：点对话按钮 → 发 @BUYSELL；点背包格 → 选中；点货架 → 买。
    let page = state.dialog.clone().expect("对话页");
    let layout = mir_client::ui::dialog::dialog_layout(screen, &page, |label| {
        text.text_width(label, hud::SIZE_BODY)
    });
    state.mouse = layout.buttons[0].center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("点按钮应产生动作");
    assert_eq!(
        action,
        UiAction::CallNpc {
            object_id: TEST_NPC_ID,
            key: "@BUYSELL".to_owned()
        },
        "按钮应带 NPC 对象 ID 与脚本键"
    );

    let slot = inventory::inventory_slot_rect(inventory_panel, 0).expect("0 号格");
    state.mouse = slot.center();
    assert!(
        state.on_click(&snapshot, &text, screen, None).is_none(),
        "点背包格只选中，不发包"
    );
    assert_eq!(state.selected_slot, Some(0));

    let shop_layout = shop::shop_layout(screen, 2, 0, shop::SHOP_LEFT);
    state.mouse = shop_layout.rows[1].center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("点货架应产生购买动作");
    assert_eq!(action, UiAction::BuyItem { unique_id: 502 });

    // 对话页解析：按钮标签去掉标记，颜色标记被剥离。
    let parsed = ParsedPage::parse(&[
        "欢迎光临，{测试商人/Red}！".to_owned(),
        "你要买点什么？<买东西/@BUYSELL>".to_owned(),
    ]);
    assert_eq!(parsed.lines[0], "欢迎光临，测试商人！");
    assert!(parsed.buttons.iter().any(|button| button.key == "@BUYSELL"));

    println!(
        "client-ui.png：{} · {}×{} · {} 种颜色 · 字体 {} · 背包变化 {} px",
        output.display(),
        frame.width,
        frame.height,
        frame.distinct_color_count(),
        text.font_path()
            .map_or_else(|| "3×5 点阵".to_owned(), |path| path.display().to_string()),
        inventory_changed
    );

    drop(client);
    drop(runtime);
}
