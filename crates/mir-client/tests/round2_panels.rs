//! 第 2 轮客户端功能的集成测试（无头，不连服务器、不开窗口）。
//!
//! 覆盖工部 t5 的验收点：
//!
//! 1. **设置面板**：`ClientSettings` 写盘 → 读回（重启仍生效）、音量百分比换算、
//!    面板布局在 320×240 / 800×600 不出屏、点击开关/语言/分辨率/缩放/关闭；
//! 2. **服务器选择与角色选择**：默认地址、失败后可重试、空角色引导创建、
//!    角色卡显示职业/等级/装备摘要、两种窗口尺寸不出屏；
//! 3. **商城页签**：切到第四个页签会发 `C.MarketSearch{MarketType=GameShop}`、
//!    列表来自 `S.NPCMarket`（`item_type = GameShop`）、价格按元宝显示、
//!    购买按钮发 `C.GameshopBuy`（数量按 5 组夹取、`p_type = 0`）、
//!    元宝余额来自 `S.UserInformation.Credit` / `S.LoseCredit`；
//! 4. **技能表现**：`S.ObjectMagic` / `S.ObjectSpell` / `S.Magic` 产生可见特效事件，
//!    `S.ObjectHidden` 让对象从渲染/名字/点选中消失。
//!
//! 截图写到 `target/client-*.png`。

mod common;

use std::path::{Path, PathBuf};

use mir_client::geom::Rect;
use mir_client::net::{
    CharacterInfo, InventoryItem, NetCommand, QuestView, SystemChat, WorldSnapshot, next_pet_mode,
};
use mir_client::settings::ClientSettings;
use mir_client::ui::labels;
use mir_client::ui::settings::{self as settings_ui, SettingsAction};
use mir_client::ui::{self, UiAction, UiState, UiText, hud, market, quests, select};
use mir_core::enums::{
    ChatType, ItemType, LightSetting, MarketItemType, MarketPanelType, MirClass, MirDirection,
    MirGender, OutputMessageType, PetMode, Spell, WeatherSetting,
};
use mir_core::{DateTimeBinary, Language, Point};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ClientAuction, ClientMail, ItemInfo, SelectInfo, UserItem};
use mir_protocol::packets::server::{
    Chat, DeleteQuestItem, GainedGold, GainedItem, GainedQuestItem, LoginSuccess, Magic,
    MapChanged, MarketFail, NPCMarket, NewItemInfo, ObjectHidden, ObjectMagic, ObjectMonster,
    ObjectSpell, ReceiveMail, SendOutputMessage, StartGame, TimeOfDay, TradeConfirm,
    UserInformation,
};
use mir_render::{Frame, MapScene, TileLibraries, Viewport};
use mir_world::{Map, MapCell, MapVersion};

/// 工作区根目录（`CARGO_MANIFEST_DIR` 是 `crates/mir-client`）。
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// 截图路径：`target/client-<name>.png`。
fn screenshot(name: &str) -> PathBuf {
    let directory = workspace_root().join("target");
    std::fs::create_dir_all(&directory).ok();
    directory.join(format!("client-{name}.png"))
}

/// 合成地图（不依赖 `resource/`）。
fn synthetic_map() -> Map {
    let width = 64;
    let height = 64;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "synthetic-round2".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
}

fn scene_and_frame(width: u32, height: u32) -> (MapScene, Frame, Point) {
    let scene = MapScene::from_parts(
        synthetic_map(),
        TileLibraries::empty("."),
        Viewport::DEFAULT,
    );
    (scene, Frame::new(width, height), Point::new(32, 32))
}

fn text() -> UiText {
    common::load_text()
}

fn user_item(unique_id: u64, item_index: i32, count: u16) -> UserItem {
    UserItem {
        unique_id,
        item_index,
        count,
        current_dura: 1000,
        max_dura: 1000,
        ..UserItem::default()
    }
}

fn item_info(index: i32, name: &str, image: u16, stack_size: u16) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        image,
        stack_size,
        ..ItemInfo::default()
    }
}

fn auction(auction_id: u64, item_index: i32, count: u16, price: u32) -> ClientAuction {
    ClientAuction {
        auction_id,
        item: user_item(0, item_index, count),
        seller: String::new(),
        price,
        consignment_date: DateTimeBinary::from_unix_seconds(1_704_067_200),
        item_type: MarketItemType::GameShop,
    }
}

/// 登录成功的快照：`S.NewItemInfo` + `S.UserInformation`（元宝 5000）。
fn snapshot_with_player(credit: u32, equipment: &[(u32, i32)]) -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
        info: item_info(1000, "元宝商品", 10, 100),
    }));
    let mut info = UserInformation {
        object_id: 7,
        real_id: 7,
        name: "测试者".to_owned(),
        class: MirClass::Warrior,
        gender: MirGender::Male,
        level: 22,
        location: Point::new(32, 32),
        direction: MirDirection::Down,
        hp: 10,
        mp: 10,
        gold: 1_000,
        credit,
        inventory: Some(vec![None; 8]),
        ..UserInformation::default()
    };
    let mut slots = vec![None; 14];
    for (slot, index) in equipment {
        if let Some(cell) = slots.get_mut(usize::try_from(*slot).unwrap_or(0)) {
            *cell = Some(user_item(0, *index, 1));
        }
    }
    info.equipment = Some(slots);
    snapshot.apply_packet(&ServerPacket::UserInformation(info));
    snapshot
}

/// 统计矩形里与底帧不同的像素数。
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

// ===== 1. 设置面板 =====

#[test]
fn settings_roundtrip_through_disk_survives_restart() {
    let path = std::env::temp_dir().join(format!(
        "mir-next-round2-settings-{}.json",
        std::process::id()
    ));
    std::fs::remove_file(&path).ok();
    // 第一次启动：默认值。
    let mut settings = ClientSettings::load(&path);
    assert_eq!(settings.language(), Language::ZhCn);
    assert_eq!(settings.server(), "127.0.0.1:7000");
    // 设置面板改语言/音量/分辨率/缩放 → 写盘。
    let mut panel = settings.panel();
    panel.language = Language::En;
    panel.sound_on = false;
    panel.music_volume = 80;
    panel.resolution = (1280, 720);
    panel.scale = 2;
    settings.absorb_panel(&panel);
    settings.save(&path).expect("保存设置");
    // 「重启」：读回来逐字段相等。
    let loaded = ClientSettings::load(&path);
    assert_eq!(loaded, settings);
    assert_eq!(loaded.language(), Language::En);
    assert!(!loaded.sound);
    assert!((loaded.music_volume() - 0.8).abs() < f32::EPSILON);
    assert_eq!(loaded.resolution(), (1280, 720));
    assert_eq!(loaded.scale(), 2);
    assert_eq!(loaded.panel(), panel);
    // 音量百分比 ⇄ 0.0..=1.0 互转。
    assert_eq!(
        settings_ui::SettingsState::percent(loaded.sound_volume()),
        50
    );
    assert!(
        (settings_ui::SettingsState::fraction(80) - 0.8).abs() < f32::EPSILON,
        "百分比应当能转回音量"
    );
    std::fs::remove_file(&path).ok();
}

#[test]
fn settings_panel_layout_and_clicks() {
    for (width, height) in [(320, 240), (800, 600)] {
        let screen = Rect::screen(width, height);
        let layout = settings_ui::settings_layout(screen);
        assert!(
            layout.panel.right() <= screen.right() && layout.panel.bottom() <= screen.bottom(),
            "{width}x{height}: 面板出屏"
        );
        let mut frame = Frame::new(width, height);
        let before = frame.clone();
        let mut state = settings_ui::SettingsState::default();
        let text = text();
        settings_ui::draw(&mut frame, &layout, &state, &text);
        let changed = changed_pixels(&before, &frame, layout.panel);
        assert!(changed > 100, "{width}x{height}: 设置面板没有画出来");
        frame
            .save_png(screenshot(&format!("settings-{width}x{height}")))
            .expect("保存设置面板截图");
        if width == 800 {
            frame
                .save_png(screenshot("settings"))
                .expect("保存统一设置截图");
        }

        // 点击：音效开关 → 音量 → 语言 → 分辨率 → 缩放。
        let point = |rect: Rect| Point::new(rect.x + 2, rect.y + 2);
        assert_eq!(
            settings_ui::click(&layout, &mut state, point(layout.sound_toggle)),
            Some(SettingsAction::ToggleSound)
        );
        assert!(!state.sound_on);
        assert_eq!(
            settings_ui::click(&layout, &mut state, point(layout.sound_up)),
            Some(SettingsAction::SoundVolume(60))
        );
        assert_eq!(
            settings_ui::click(&layout, &mut state, point(layout.language_en)),
            Some(SettingsAction::SetLanguage(Language::En))
        );
        assert_eq!(state.language, Language::En);
        assert_eq!(
            settings_ui::click(&layout, &mut state, point(layout.close)),
            Some(SettingsAction::Close)
        );
    }
}

#[test]
fn languages_switch_labels_immediately() {
    use mir_client::ui::labels::Label;

    // 文案表：同一个标签在中/英下不同（且都非空）。
    for label in [
        Label::Settings,
        Label::GameShop,
        Label::Credit,
        Label::KeyBindings,
        Label::SelectCharacter,
        Label::CharacterName,
    ] {
        assert!(!label.text(Language::ZhCn).is_empty());
        assert!(!label.text(Language::En).is_empty());
        assert_ne!(
            label.text(Language::ZhCn),
            label.text(Language::En),
            "{label:?} 的中英文案不该相同"
        );
    }
    // 面板本体：同一状态下中/英两版画出来的像素不同（说明切换语言会立即重绘文案）。
    let text = text();
    let screen = Rect::screen(800, 600);
    let layout = settings_ui::settings_layout(screen);
    let mut zh = Frame::new(800, 600);
    let mut en = Frame::new(800, 600);
    let mut state = settings_ui::SettingsState::default();
    settings_ui::draw(&mut zh, &layout, &state, &text);
    state.language = Language::En;
    settings_ui::draw(&mut en, &layout, &state, &text);
    assert!(
        changed_pixels(&zh, &en, layout.panel) > 50,
        "切换语言后设置面板文案应当变化"
    );
}

#[test]
fn settings_panel_is_modal_in_the_ui_state() {
    let (mut scene, mut frame, center) = scene_and_frame(800, 600);
    let snapshot = snapshot_with_player(0, &[]);
    let text = text();
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    state.toggle_settings();
    assert!(state.show_settings);
    // 面板画在 UI 上（不是空白）。
    let before = frame.clone();
    ui::draw(
        &mut state, &mut frame, &mut scene, &snapshot, &text, None, center,
    );
    let layout = settings_ui::settings_layout(Rect::screen(frame.width, frame.height));
    assert!(changed_pixels(&before, &frame, layout.panel) > 100);
    // 点击「关闭」→ 面板收起；面板外点击被吃掉（不走路）。
    state.mouse = Point::new(layout.close.x + 2, layout.close.y + 2);
    assert!(
        state
            .on_click(&snapshot, &text, Rect::screen(800, 600), None)
            .is_none()
    );
    assert!(!state.show_settings);
    // Esc 也能收起。
    state.toggle_settings();
    assert!(state.show_settings);
    state.escape();
    assert!(!state.show_settings);
}

// ===== 2. 服务器选择与角色选择 =====

fn character_info(index: i32, name: &str, level: u16, class: MirClass) -> CharacterInfo {
    CharacterInfo {
        index,
        name: name.to_owned(),
        level,
        class,
        gender: MirGender::Male,
        last_access: DateTimeBinary::from_unix_seconds(1_704_067_200),
    }
}

#[test]
fn server_select_defaults_to_localhost_and_can_retry() {
    let layout = select::server_layout(Rect::screen(800, 600));
    let mut state = select::ServerSelectState::default();
    assert_eq!(state.address.value, "127.0.0.1:7000");
    assert!(!state.can_connect(), "没填账号/口令时不能连接");
    state.account.set("hero");
    state.password.set("secret");
    // 第一次连接失败：错误提示 + 按钮变成「重试」。
    state.begin_connect();
    state.fail(
        select::ConnectError::Unreachable,
        Some("连接被拒绝".to_owned()),
    );
    assert!(state.error.is_some());
    assert_eq!(state.status, select::ConnectState::Idle);
    let action = select::server_click(
        &layout,
        &mut state,
        Point::new(layout.connect.x + 2, layout.connect.y + 2),
    );
    assert_eq!(action, Some(select::ServerAction::Connect));
    assert!(state.error.is_none(), "重试要先清掉旧错误");
    assert_eq!(state.status, select::ConnectState::Connecting);
}

#[test]
fn character_select_shows_class_level_and_equipment_summary() {
    let mut state = select::CharacterSelectState::default();
    state
        .equipment_cache
        .insert("张三".to_owned(), "木剑 / 布衣".to_owned());
    state.set_characters(&[
        character_info(0, "张三", 22, MirClass::Warrior),
        character_info(3, "李四", 9, MirClass::Taoist),
    ]);
    assert_eq!(state.characters.len(), 2);
    assert!(!state.creating);
    let card = &state.characters[0];
    assert!(card.headline(Language::ZhCn).contains("战士"));
    assert!(card.headline(Language::ZhCn).contains("22"));
    assert_eq!(card.equipment_text(Language::ZhCn), "装备: 木剑 / 布衣");
    assert!(
        card.last_access_text(Language::ZhCn).contains("2024"),
        "上次登录时间应当格式化"
    );
    assert_eq!(
        state.characters[1].equipment_text(Language::ZhCn),
        "装备: 暂无装备"
    );
    // 空列表 → 引导创建。
    let mut empty = select::CharacterSelectState::default();
    empty.set_characters(&[]);
    assert!(empty.creating, "空角色列表要展开创建表单");
    assert!(!empty.can_create());
    empty.create_name.set("新人");
    assert!(empty.can_create());
    let layout = select::character_layout(Rect::screen(800, 600), 0, true);
    assert_eq!(
        select::character_click(
            &layout,
            &mut empty,
            Point::new(layout.confirm.x + 2, layout.confirm.y + 2)
        ),
        Some(select::CharacterAction::Create)
    );
}

#[test]
fn login_screens_render_at_both_sizes() {
    let text = text();
    let mut server_state = select::ServerSelectState::default();
    server_state.account.set("hero");
    server_state.password.set("secret");
    server_state.fail(select::ConnectError::Timeout, Some("等待超时".to_owned()));

    let mut character_state = select::CharacterSelectState::default();
    character_state.set_characters(&[
        character_info(0, "张三", 22, MirClass::Warrior),
        character_info(1, "李四", 9, MirClass::Taoist),
    ]);

    for (width, height) in [(320, 240), (800, 600)] {
        let screen = Rect::screen(width, height);
        let server_layout = select::server_layout(screen);
        assert!(server_layout.panel.right() <= screen.right());
        assert!(server_layout.panel.bottom() <= screen.bottom());
        let mut frame = Frame::new(width, height);
        let before = frame.clone();
        select::draw_server(
            &mut frame,
            &server_layout,
            &server_state,
            &text,
            Language::ZhCn,
        );
        assert!(
            changed_pixels(&before, &frame, server_layout.panel) > 100,
            "{width}x{height}: 登录页没画出来"
        );
        frame
            .save_png(screenshot(&format!("login-server-{width}x{height}")))
            .expect("保存登录页截图");

        let character_layout = select::character_layout(screen, 2, false);
        assert!(character_layout.panel.bottom() <= screen.bottom());
        let mut frame = Frame::new(width, height);
        let before = frame.clone();
        select::draw_characters(
            &mut frame,
            &character_layout,
            &character_state,
            &text,
            Language::ZhCn,
        );
        assert!(
            changed_pixels(&before, &frame, character_layout.panel) > 100,
            "{width}x{height}: 选角页没画出来"
        );
        frame
            .save_png(screenshot(&format!("character-select-{width}x{height}")))
            .expect("保存选角页截图");
    }
}

// ===== 3. 商城页签 =====

/// 打开市场面板并切到商城页签。
fn open_game_shop_tab(state: &mut UiState, snapshot: &WorldSnapshot) {
    state.sync(snapshot);
    state.show_market = true;
    state.market_tab = MarketPanelType::GameShop;
    state.market_page = 0;
    state.market_scroll = 0;
}

#[test]
fn game_shop_tab_sends_market_search_and_buys_with_credits() {
    let text = text();
    let mut snapshot = snapshot_with_player(5_000, &[]);
    snapshot.apply_packet(&ServerPacket::NPCMarket(NPCMarket {
        listings: vec![auction(4242, 1000, 20, 250)],
        pages: 1,
        user_mode: false,
    }));
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    assert!(state.show_market, "S.NPCMarket 应当打开市场面板");
    // 切到商城页签：发 `C.MarketSearch{MarketType=GameShop}`（不是本地假数据）。
    let screen = Rect::screen(800, 600);
    let layout = market::market_layout(screen, MarketPanelType::Market, 1, 0, 0);
    state.mouse = Point::new(layout.tabs[3].x + 2, layout.tabs[3].y + 2);
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("点商城页签要产生动作");
    assert_eq!(
        action,
        UiAction::MarketSearch {
            match_text: String::new(),
            item_type: ItemType::Nothing,
            user_mode: false,
            min_shape: 0,
            max_shape: 5000,
            panel: MarketPanelType::GameShop,
        }
    );
    assert_eq!(state.market_tab, MarketPanelType::GameShop);
    assert_eq!(state.market_price.value, "1", "商城数量默认 1 份");

    // 服务端回商城列表：选中后点「购买」。
    let layout = market::market_layout(
        screen,
        MarketPanelType::GameShop,
        snapshot.market.listings.len(),
        state.market_page,
        state.market_scroll,
    );
    state.mouse = Point::new(layout.rows[0].x + 2, layout.rows[0].y + 2);
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.market_selected, Some(4242), "行选中要记下 auction_id");
    // 价格按元宝显示。
    let listing = &snapshot.market.listings[0];
    assert_eq!(
        market::price_text(listing, Language::ZhCn),
        "250 元宝",
        "商城价格必须是元宝"
    );
    assert_eq!(
        market::seller_text(listing, Language::ZhCn),
        "商城",
        "服务端卖家为空时显示「商城」"
    );
    assert_eq!(market::stock_text(listing, Language::ZhCn), "在售");

    // 数量 50 → 按 5 组（5 × 100 / 20 = 25）夹到 25。
    state.market_price.set("50");
    state.mouse = Point::new(layout.primary.x + 2, layout.primary.y + 2);
    let buy = state
        .on_click(&snapshot, &text, screen, None)
        .expect("购买按钮要产生动作");
    assert_eq!(
        buy,
        UiAction::GameShopBuy {
            g_index: 4242,
            quantity: 25,
            p_type: 0,
        }
    );
    assert_eq!(
        buy.net_command(),
        Some(NetCommand::GameShopBuy {
            g_index: 4242,
            quantity: 25,
            p_type: 0,
        }),
        "购买按钮必须发 C.GameshopBuy（包 133）"
    );
}

#[test]
fn game_shop_tab_fits_small_and_large_screens() {
    let mut snapshot = snapshot_with_player(5_000, &[]);
    snapshot.apply_packet(&ServerPacket::NPCMarket(NPCMarket {
        listings: vec![auction(1, 1000, 1, 100), auction(2, 1000, 20, 250)],
        pages: 1,
        user_mode: false,
    }));
    let text = text();
    let mut state = UiState::new(Language::ZhCn);
    open_game_shop_tab(&mut state, &snapshot);
    for (width, height) in [(320, 240), (800, 600)] {
        let screen = Rect::screen(width, height);
        let layout = market::market_layout(
            screen,
            MarketPanelType::GameShop,
            snapshot.market.listings.len(),
            state.market_page,
            state.market_scroll,
        );
        assert!(
            layout.panel.right() <= screen.right() && layout.panel.bottom() <= screen.bottom(),
            "{width}x{height}: 商城面板出屏"
        );
        for rect in &layout.tabs {
            assert!(
                layout.panel.contains(Point::new(rect.x, rect.y)),
                "{width}x{height}: 页签不在面板内"
            );
        }
        for row in &layout.rows {
            assert!(
                row.right() <= layout.panel.right() && row.bottom() <= layout.panel.bottom(),
                "{width}x{height}: 列表行出屏"
            );
        }
        assert!(
            layout.primary.right() <= layout.panel.right()
                && layout.primary.bottom() <= layout.panel.bottom(),
            "{width}x{height}: 购买按钮出屏"
        );

        let (mut scene, mut frame, center) = scene_and_frame(width, height);
        let entities = snapshot.entity_views(0);
        scene.render_with_entities(&mut frame, center, &entities);
        let before = frame.clone();
        ui::draw(
            &mut state, &mut frame, &mut scene, &snapshot, &text, None, center,
        );
        assert!(
            changed_pixels(&before, &frame, layout.panel) > 100,
            "{width}x{height}: 商城页签没画出来"
        );
        frame
            .save_png(screenshot(&format!("gameshop-{width}x{height}")))
            .expect("保存商城页签截图");
    }
}

#[test]
fn credit_balance_updates_from_lose_credit() {
    let mut snapshot = snapshot_with_player(5_000, &[]);
    assert_eq!(
        snapshot.self_player.as_ref().map(|me| me.credit),
        Some(5_000)
    );
    snapshot.apply_packet(&ServerPacket::LoseCredit(
        mir_protocol::packets::server::LoseCredit { credit: 250 },
    ));
    assert_eq!(
        snapshot.self_player.as_ref().map(|me| me.credit),
        Some(4_750)
    );
}

// ===== 4. 技能表现与隐身 =====

#[test]
fn spell_packets_create_visible_effects() {
    let mut snapshot = snapshot_with_player(0, &[]);
    // 别人放火墙：锚在目标格。
    snapshot.apply_packet(&ServerPacket::ObjectMagic(ObjectMagic {
        object_id: 7,
        location: Point::new(32, 32),
        direction: MirDirection::Right,
        spell: Spell::FireWall,
        target_id: 0,
        target: Point::new(34, 32),
        cast: true,
        level: 3,
        self_broadcast: false,
        secondary_target_ids: Vec::new(),
    }));
    assert_eq!(snapshot.spell_events.len(), 1);
    assert_eq!(snapshot.spell_events[0].spell, Spell::FireWall);
    assert_eq!(
        snapshot.spell_events[0].location,
        Point::new(34, 32),
        "火墙锚在目标格"
    );
    // 瞬息移动：锚在施法者脚下。
    snapshot.apply_packet(&ServerPacket::ObjectMagic(ObjectMagic {
        object_id: 7,
        location: Point::new(32, 32),
        direction: MirDirection::Right,
        spell: Spell::Teleport,
        target_id: 0,
        target: Point::new(40, 40),
        cast: true,
        level: 3,
        self_broadcast: false,
        secondary_target_ids: Vec::new(),
    }));
    assert_eq!(snapshot.spell_events[1].location, Point::new(32, 32));
    // `S.ObjectSpell`（包 149）：火墙的持续 SpellObject 落点。
    snapshot.apply_packet(&ServerPacket::ObjectSpell(ObjectSpell {
        object_id: 7,
        location: Point::new(35, 33),
        spell: Spell::FireWall,
        direction: MirDirection::Right,
        param: true,
    }));
    assert_eq!(snapshot.spell_events[2].location, Point::new(35, 33));
    // 自己的 `S.Magic` 回执也会产生表现。
    snapshot.apply_packet(&ServerPacket::Magic(Magic {
        spell: Spell::ThunderBolt,
        target_id: 0,
        target: Point::new(36, 33),
        cast: true,
        level: 3,
        secondary_target_ids: Vec::new(),
    }));
    assert_eq!(snapshot.spell_events[3].spell, Spell::ThunderBolt);

    // 画到帧上：落点附近必须有像素变化（可见反馈）。
    let (mut scene, mut frame, center) = scene_and_frame(800, 600);
    scene.render(&mut frame, center);
    let before = frame.clone();
    let text = text();
    ui::spellfx::draw(
        &mut frame,
        &snapshot.spell_events,
        &snapshot,
        &scene,
        center,
        &text,
        Language::ZhCn,
        snapshot.spell_events[0].started_ms,
    );
    let (x, y) = scene.world_to_screen(center, Point::new(34, 32));
    let area = Rect::new(x - 40, y - 40, 120, 80);
    assert!(
        changed_pixels(&before, &frame, area) > 20,
        "火墙的表现应当画在地图格上"
    );
    frame
        .save_png(screenshot("spellfx"))
        .expect("保存技能表现截图");
}

#[test]
fn object_hidden_hides_entities_everywhere() {
    use mir_protocol::packets::server::ObjectPlayer;

    let mut snapshot = snapshot_with_player(0, &[]);
    snapshot.apply_packet(&ServerPacket::ObjectPlayer(ObjectPlayer {
        object_id: 8,
        name: "隐身人".to_owned(),
        location: Point::new(33, 32),
        class: MirClass::Wizard,
        gender: MirGender::Male,
        level: 10,
        ..ObjectPlayer::default()
    }));
    assert!(
        snapshot
            .entity_views(0)
            .iter()
            .any(|view| view.name.as_deref() == Some("隐身人")),
        "未隐身时应当渲染"
    );
    snapshot.apply_packet(&ServerPacket::ObjectHidden(ObjectHidden {
        object_id: 8,
        hidden: true,
    }));
    assert!(
        snapshot
            .entities
            .get(&8)
            .is_some_and(|entity| entity.hidden),
        "S.ObjectHidden 应当把实体标成隐身"
    );
    assert!(
        !snapshot
            .entity_views(0)
            .iter()
            .any(|view| view.name.as_deref() == Some("隐身人")),
        "隐身对象不渲染"
    );
    assert!(
        mir_client::ui::target::entity_at(&snapshot, Point::new(33, 32)).is_none(),
        "隐身对象不能被点选"
    );
    // 解除隐身：重新可见。
    snapshot.apply_packet(&ServerPacket::ObjectHidden(ObjectHidden {
        object_id: 8,
        hidden: false,
    }));
    assert!(
        snapshot
            .entity_views(0)
            .iter()
            .any(|view| view.name.as_deref() == Some("隐身人")),
        "显形后应当恢复渲染"
    );
}

#[test]
fn login_success_exposes_character_list_for_manual_select() {
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::LoginSuccess(LoginSuccess {
        characters: vec![
            SelectInfo {
                index: 0,
                name: "张三".to_owned(),
                level: 22,
                class: MirClass::Warrior,
                gender: MirGender::Male,
                last_access: DateTimeBinary::from_unix_seconds(1_704_067_200),
            },
            SelectInfo {
                index: 3,
                name: "李四".to_owned(),
                level: 9,
                class: MirClass::Taoist,
                gender: MirGender::Female,
                last_access: DateTimeBinary::default(),
            },
        ],
    }));
    assert_eq!(snapshot.characters.len(), 2);
    assert_eq!(snapshot.characters[1].name, "李四");
    assert_eq!(snapshot.characters[1].class, MirClass::Taoist);
    assert_eq!(snapshot.characters_version, 1);
}

// ===== 5. 第 3 轮：i18n 收口与面板打磨 =====

/// 面板开关（名字 + 打开方式）：语言切换重绘测试用。
type PanelOpener = (&'static str, fn(&mut UiState));

/// 受控渲染：设置语言后整屏画一帧（世界 + UI）。
fn frame_with_language(
    state: &mut UiState,
    snapshot: &WorldSnapshot,
    text: &UiText,
    language: Language,
) -> Frame {
    state.language = language;
    let (mut scene, mut frame, center) = scene_and_frame(800, 600);
    let entities = snapshot.entity_views(0);
    scene.render_with_entities(&mut frame, center, &entities);
    ui::draw(state, &mut frame, &mut scene, snapshot, text, None, center);
    frame
}

#[test]
fn server_chat_is_shown_as_sent_and_client_system_chat_is_localized() {
    let mut snapshot = snapshot_with_player(0, &[]);
    // 服务端已经按玩家语言翻译好的文本：客户端原样显示（中/英都不二次翻译）。
    snapshot.apply_packet(&ServerPacket::Chat(Chat {
        message: "你好，冒险者".to_owned(),
        type_: ChatType::System,
    }));
    snapshot.apply_packet(&ServerPacket::Chat(Chat {
        message: "Hello, adventurer".to_owned(),
        type_: ChatType::System,
    }));
    let zh_line = &snapshot.chat[snapshot.chat.len() - 2];
    let en_line = &snapshot.chat[snapshot.chat.len() - 1];
    assert_eq!(
        labels::chat_line_text(zh_line, Language::En),
        "你好，冒险者"
    );
    assert_eq!(
        labels::chat_line_text(en_line, Language::ZhCn),
        "Hello, adventurer"
    );

    // 客户端合成的系统消息：结构化存下来，按玩家语言渲染（同一条消息可随时切换语言）。
    snapshot.apply_packet(&ServerPacket::GainedGold(GainedGold { gold: 500 }));
    let system = snapshot.chat.last().expect("拾取金币应当有系统消息");
    assert!(
        matches!(system.system, Some(SystemChat::GoldGained { gold: 500 })),
        "客户端系统消息应当结构化保存"
    );
    assert!(system.text.is_empty(), "系统消息不预渲染中文");
    assert_eq!(
        labels::chat_line_text(system, Language::ZhCn),
        "获得金币 500"
    );
    assert_eq!(
        labels::chat_line_text(system, Language::En),
        "Gained 500 gold"
    );

    // 说话者前缀。
    let with_speaker = mir_client::net::ChatLine {
        speaker: Some("甲".to_owned()),
        text: "hi".to_owned(),
        kind: ChatType::Normal,
        system: None,
    };
    assert_eq!(
        labels::chat_line_text(&with_speaker, Language::En),
        "[甲] hi"
    );
    // 连接阶段状态文案也走 `Label`（HUD 在还没有自己的角色时显示）。
    snapshot.apply_packet(&ServerPacket::StartGame(StartGame {
        result: 4,
        ..StartGame::default()
    }));
    assert_eq!(snapshot.status, mir_client::net::NetStatus::InGame);
    assert_eq!(
        labels::net_status_label(snapshot.status, Language::En),
        "In game"
    );
    assert_eq!(
        labels::net_status_label(snapshot.status, Language::ZhCn),
        "已进入世界"
    );
}

#[test]
fn market_tab_switch_clears_selection_and_notice() {
    let text = text();
    let mut snapshot = snapshot_with_player(5_000, &[]);
    snapshot.apply_packet(&ServerPacket::NPCMarket(NPCMarket {
        listings: vec![auction(4242, 1000, 20, 250)],
        pages: 1,
        user_mode: false,
    }));
    snapshot.apply_packet(&ServerPacket::MarketFail(MarketFail { reason: 1 }));
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    state.show_market = true;
    state.market_tab = MarketPanelType::Market;
    // 先选中一行。
    let screen = Rect::screen(800, 600);
    let layout = market::market_layout(
        screen,
        MarketPanelType::Market,
        snapshot.market.listings.len(),
        0,
        0,
    );
    state.mouse = Point::new(layout.rows[0].x + 2, layout.rows[0].y + 2);
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.market_selected, Some(4242));
    assert_eq!(
        state.market_notice(&snapshot),
        (Some(1), None),
        "失败提示应当先显示"
    );

    // 切到商城页签：选中行、页码、上一页的失败提示都清掉。
    state.mouse = Point::new(layout.tabs[3].x + 2, layout.tabs[3].y + 2);
    let action = state.on_click(&snapshot, &text, screen, None);
    assert!(matches!(
        action,
        Some(UiAction::MarketSearch {
            panel: MarketPanelType::GameShop,
            ..
        })
    ));
    assert!(state.market_selected.is_none(), "切页签要清空选中行");
    assert_eq!(state.market_page, 0);
    assert_eq!(
        state.market_notice(&snapshot),
        (None, None),
        "切页签要清掉上一页的失败提示"
    );

    // 新的失败提示（版本更大）仍然要显示。
    snapshot.apply_packet(&ServerPacket::MarketFail(MarketFail { reason: 4 }));
    assert_eq!(state.market_notice(&snapshot), (Some(4), None));
}

#[test]
fn chat_panel_renders_server_and_client_text_in_both_languages() {
    let mut snapshot = snapshot_with_player(0, &[]);
    // 服务端文本（已按玩家语言翻译）+ 客户端系统消息混排。
    snapshot.apply_packet(&ServerPacket::Chat(Chat {
        message: "欢迎来到传奇-Next".to_owned(),
        type_: ChatType::System,
    }));
    snapshot.apply_packet(&ServerPacket::GainedGold(GainedGold { gold: 500 }));
    snapshot.apply_packet(&ServerPacket::GainedItem(GainedItem {
        item: user_item(99, 1000, 2),
    }));
    snapshot.apply_packet(&ServerPacket::TradeConfirm(TradeConfirm {}));
    assert!(snapshot.chat.len() >= 4, "聊天应当有 4 行");

    let text = text();
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    let zh = frame_with_language(&mut state, &snapshot, &text, Language::ZhCn);
    let en = frame_with_language(&mut state, &snapshot, &text, Language::En);
    let chat = mir_client::ui::hud::chat_layout(Rect::screen(800, 600));
    assert!(
        changed_pixels(&zh, &en, chat) > 100,
        "同一份聊天记录切语言后应当重绘（客户端系统消息按语言渲染）"
    );
    zh.save_png(screenshot("chat-zh"))
        .expect("保存中文聊天截图");
    en.save_png(screenshot("chat-en"))
        .expect("保存英文聊天截图");
}

#[test]
fn language_switch_redraws_every_open_panel() {
    let text = text();
    let mut snapshot = snapshot_with_player(0, &[]);
    snapshot.apply_packet(&ServerPacket::NPCMarket(NPCMarket {
        listings: vec![auction(1, 1000, 1, 100)],
        pages: 1,
        user_mode: false,
    }));
    // 每个面板单独打开，比较中/英两帧的差异（面板内至少有一处 `Label` 文案）。
    let panels: [PanelOpener; 13] = [
        ("inventory", |state| state.show_inventory = true),
        ("equipment", |state| state.show_equipment = true),
        ("skills", |state| state.show_skills = true),
        ("quests", |state| state.show_quests = true),
        ("mail", |state| state.show_mail = true),
        ("guild", |state| state.show_guild = true),
        ("group", |state| state.show_group = true),
        ("storage", |state| state.show_storage = true),
        ("trade", |state| state.show_trade = true),
        ("market", |state| state.show_market = true),
        ("settings", |state| state.show_settings = true),
        ("bigmap", |state| state.show_bigmap = true),
        ("minimap", |state| state.show_minimap = true),
    ];
    for (name, open) in panels {
        let mut state = UiState::new(Language::ZhCn);
        state.sync(&snapshot);
        // 有的面板会被 `sync` 自动打开，先统一关掉再按用例打开。
        state.close_panels();
        open(&mut state);
        let zh = frame_with_language(&mut state, &snapshot, &text, Language::ZhCn);
        let en = frame_with_language(&mut state, &snapshot, &text, Language::En);
        let whole = Rect::screen(800, 600);
        assert!(
            changed_pixels(&zh, &en, whole) > 100,
            "{name} 面板在切语言后应当重绘（文案变化）"
        );
    }
}

/// 服务器选择页单独渲染一帧（不画世界，便于逐像素比较）。
fn server_select_frame(
    layout: &select::ServerLayout,
    state: &select::ServerSelectState,
    text: &UiText,
    language: Language,
) -> Frame {
    let mut frame = Frame::new(800, 600);
    select::draw_server(&mut frame, layout, state, text, language);
    frame
}

fn has_cjk(text: &str) -> bool {
    text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

#[test]
fn english_login_error_never_shows_raw_chinese_diagnostics() {
    let text = text();
    let layout = select::server_layout(Rect::screen(800, 600));
    let mut state = select::ServerSelectState::default();
    state.account.set("hero");
    state.password.set("secret");
    // 模拟 net.rs 的中文 anyhow 详情（以前会直接画在类别下面）。
    let raw = "无法连接服务器 127.0.0.1:7000：连接被拒绝（os error 10061）";
    state.fail(select::ConnectError::Unreachable, Some(raw.to_owned()));

    // 1) 所有类别在英文下都是纯英文（不含 CJK），中文下非空。
    for error in [
        select::ConnectError::Unreachable,
        select::ConnectError::Credentials,
        select::ConnectError::Refused,
        select::ConnectError::Timeout,
        select::ConnectError::Version,
        select::ConnectError::Other,
    ] {
        let en = select::error_text(error, Language::En);
        let zh = select::error_text(error, Language::ZhCn);
        assert!(!zh.is_empty(), "{error:?} 缺中文提示");
        assert!(!en.is_empty(), "{error:?} 缺英文提示");
        assert!(!has_cjk(en), "{error:?} 英文提示混进中文：{en}");
    }

    // 2) 同一类别、不同原始详情画出来的像素完全一致 ⇒ 详情没有被画到 UI 上。
    let mut other = state.clone();
    other.message = Some("完全不同的诊断：后台任务 panic，栈里全是中文".to_owned());
    for language in [Language::ZhCn, Language::En] {
        let a = server_select_frame(&layout, &state, &text, language);
        let b = server_select_frame(&layout, &other, &text, language);
        assert_eq!(
            changed_pixels(&a, &b, layout.panel),
            0,
            "{language:?}: 原始诊断串不应影响画面（只进日志）"
        );
    }

    // 3) 英文界面整体渲染仍能出画面（不是空白），原始详情仍在诊断结构里可取。
    let en = server_select_frame(&layout, &state, &text, Language::En);
    let blank = Frame::new(800, 600);
    assert!(changed_pixels(&blank, &en, layout.panel) > 100);
    assert_eq!(state.message.as_deref(), Some(raw));
}

// ===== 第 4 轮：任务物品 / 拍卖提示 / 时间天气 =====

#[test]
fn quest_items_track_gained_and_deleted_items() {
    let mut snapshot = snapshot_with_player(0, &[]);
    snapshot.apply_packet(&ServerPacket::GainedQuestItem(GainedQuestItem {
        item: user_item(900, 1000, 2),
    }));
    assert_eq!(snapshot.quest_items.len(), 1);
    assert_eq!(snapshot.quest_items[0].count, 2);
    assert_eq!(snapshot.quest_items_version, 1);
    let line = snapshot.chat.last().expect("获得任务物品要有提示");
    assert!(matches!(
        &line.system,
        Some(SystemChat::QuestItemGained {
            name: Some(name),
            count: 2,
            ..
        }) if name == "元宝商品"
    ));
    assert_eq!(
        labels::chat_line_text(line, Language::ZhCn),
        "获得任务物品 元宝商品 ×2"
    );
    assert_eq!(
        labels::chat_line_text(line, Language::En),
        "Quest item gained: 元宝商品 ×2"
    );

    // 同 `unique_id` 覆盖（服务端发「合并后的整堆」时 count 变大），版本号继续增长。
    snapshot.apply_packet(&ServerPacket::GainedQuestItem(GainedQuestItem {
        item: user_item(900, 1000, 3),
    }));
    assert_eq!(snapshot.quest_items.len(), 1);
    assert_eq!(snapshot.quest_items[0].count, 3);
    assert_eq!(snapshot.quest_items_version, 2);

    // 服务端发增量克隆（新 unique_id、同模板）：按模板并堆，不能显示成两堆
    // （Crystal `GameScene.AddQuestItem`，`GameScene.cs:6559-6586`）。
    snapshot.apply_packet(&ServerPacket::GainedQuestItem(GainedQuestItem {
        item: user_item(901, 1000, 4),
    }));
    assert_eq!(snapshot.quest_items.len(), 1, "同模板增量要并进已有堆");
    assert_eq!(snapshot.quest_items[0].count, 7);
    assert_eq!(
        snapshot.quest_items[0].unique_id, 900,
        "保留先入堆的 unique_id"
    );

    // 扣数量：7 扣 2 剩 5（仍保留），提示「失去任务物品」。
    snapshot.apply_packet(&ServerPacket::DeleteQuestItem(DeleteQuestItem {
        unique_id: 900,
        count: 2,
    }));
    assert_eq!(snapshot.quest_items.len(), 1);
    assert_eq!(snapshot.quest_items[0].count, 5);
    assert!(matches!(
        &snapshot.chat.last().expect("扣除要有提示").system,
        Some(SystemChat::QuestItemLost { count: 2, .. })
    ));

    // 再扣 5 到 0：从列表移除。
    snapshot.apply_packet(&ServerPacket::DeleteQuestItem(DeleteQuestItem {
        unique_id: 900,
        count: 5,
    }));
    assert!(snapshot.quest_items.is_empty());

    // 未知 `unique_id`：不改状态、不发聊天。
    let before = snapshot.chat.len();
    snapshot.apply_packet(&ServerPacket::DeleteQuestItem(DeleteQuestItem {
        unique_id: 123_456,
        count: 1,
    }));
    assert!(snapshot.quest_items.is_empty());
    assert_eq!(snapshot.chat.len(), before);
}

#[test]
fn user_information_restores_quest_inventory() {
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
        info: item_info(1000, "元宝商品", 10, 100),
    }));
    // 登录/重连：`UserInformation.QuestInventory`（40 格，`null` = 保留旧数据）。
    snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
        object_id: 7,
        real_id: 7,
        name: "测试者".to_owned(),
        quest_inventory: Some(vec![
            Some(user_item(77, 1000, 2)),
            None,
            Some(user_item(78, 1000, 1)),
        ]),
        ..UserInformation::default()
    }));
    assert_eq!(snapshot.quest_items.len(), 2, "空格不生成条目");
    assert_eq!(snapshot.quest_items[0].unique_id, 77);
    assert_eq!(snapshot.quest_items[0].count, 2);
    assert_eq!(snapshot.quest_items[1].unique_id, 78);
    assert_eq!(snapshot.quest_items_version, 1);

    // 之后 `DeleteQuestItem` 按 unique_id/count 扣减。
    snapshot.apply_packet(&ServerPacket::DeleteQuestItem(DeleteQuestItem {
        unique_id: 77,
        count: 1,
    }));
    assert_eq!(snapshot.quest_items.len(), 2);
    assert_eq!(snapshot.quest_items[0].count, 1);
}

#[test]
fn quest_panel_shows_quest_items_and_fits_small_screens() {
    let text = text();
    let mut snapshot = snapshot_with_player(0, &[]);
    snapshot.quests.push(QuestView {
        id: 5,
        name: "猎鹿".to_owned(),
        group: "新手".to_owned(),
        task_list: vec!["击杀 鹿 3/10".to_owned()],
        taken: true,
        completed: false,
        new: false,
        time_limit_seconds: 600,
        accepted_at_ms: Some(1_000),
    });
    snapshot.quest_items.push(InventoryItem {
        unique_id: 900,
        item_index: 1000,
        count: 2,
        current_dura: 0,
        max_dura: 0,
    });

    // 320×240：行数按高度缩减，「任务物品」区与底部提示都不能出屏。
    let small = quests::quest_layout(Rect::screen(320, 240), snapshot.quests.len(), 0);
    let screen = Rect::screen(320, 240);
    assert!(screen.contains(small.panel.center()));
    assert!(small.panel.right() <= screen.right());
    assert!(small.panel.bottom() <= screen.bottom());
    assert!(small.rows.len() < 3, "小窗口要减少可见行");
    for row in &small.rows {
        assert!(small.panel.contains(row.row.center()), "{row:?} 越出面板");
        assert!(screen.contains(row.row.center()), "{row:?} 越出屏幕");
    }
    assert!(small.panel.contains(small.items.center()), "任务物品区越界");
    assert!(small.panel.contains(small.footer.center()), "底部提示越界");
    assert!(
        small.items.bottom() <= small.footer.y,
        "任务物品区不能盖住底部提示"
    );

    for (width, height) in [(320, 240), (800, 600)] {
        let mut state = UiState::new(Language::ZhCn);
        state.sync(&snapshot);
        state.show_quests = true;
        let (mut scene, mut frame, center) = scene_and_frame(width, height);
        let entities = snapshot.entity_views(0);
        scene.render_with_entities(&mut frame, center, &entities);
        ui::draw(
            &mut state, &mut frame, &mut scene, &snapshot, &text, None, center,
        );
        frame
            .save_png(screenshot(&format!("quest-items-{width}x{height}")))
            .expect("保存任务物品截图");
    }

    // 切语言重绘（任务面板文案 + 「任务物品」标题）。
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    state.show_quests = true;
    let zh = frame_with_language(&mut state, &snapshot, &text, Language::ZhCn);
    let en = frame_with_language(&mut state, &snapshot, &text, Language::En);
    let layout = quests::quest_layout(Rect::screen(800, 600), snapshot.quests.len(), 0);
    assert!(
        changed_pixels(&zh, &en, layout.panel) > 100,
        "任务面板切语言后应当重绘"
    );
}

#[test]
fn auction_mail_and_expiry_messages_render() {
    let text = text();
    let mut snapshot = snapshot_with_player(0, &[]);
    // 自动成交/过期退回由服务端下发邮件 + 系统消息，客户端原样显示。
    snapshot.apply_packet(&ServerPacket::ReceiveMail(ReceiveMail {
        mail: vec![ClientMail {
            mail_id: 7,
            sender_name: "拍卖行".to_owned(),
            message: "你的拍卖品已成交，货款已附上。".to_owned(),
            opened: false,
            locked: false,
            can_reply: false,
            collected: false,
            date_sent: DateTimeBinary::from_unix_seconds(1_704_067_200),
            gold: 1_200,
            items: Vec::new(),
        }],
    }));
    assert_eq!(snapshot.mails.len(), 1);
    assert_eq!(snapshot.mails[0].sender_name, "拍卖行");
    assert_eq!(snapshot.mails[0].gold, 1_200);

    snapshot.apply_packet(&ServerPacket::Chat(Chat {
        message: "拍卖成交：木剑".to_owned(),
        type_: ChatType::System,
    }));
    let line = snapshot.chat.last().expect("拍卖成交通知");
    // 服务端已本地化，客户端不二次翻译（英文界面也原样显示服务端文本）。
    assert_eq!(labels::chat_line_text(line, Language::En), "拍卖成交：木剑");

    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    state.show_mail = true;
    let zh = frame_with_language(&mut state, &snapshot, &text, Language::ZhCn);
    let en = frame_with_language(&mut state, &snapshot, &text, Language::En);
    let mail = mir_client::ui::mail::mail_layout(
        Rect::screen(800, 600),
        state.mail_tab,
        snapshot.mails.len(),
        0,
        snapshot.inventory.len(),
    );
    assert!(
        changed_pixels(&zh, &en, mail.panel) > 100,
        "邮件面板切语言后应当重绘"
    );
    en.save_png(screenshot("auction-mail"))
        .expect("保存拍卖邮件截图");
}

#[test]
fn time_and_weather_follow_server_packets() {
    let mut snapshot = snapshot_with_player(0, &[]);
    // `S.TimeOfDay` 在协议里存在（index 61）；本作服务端目前不发，收到才显示。
    assert_eq!(snapshot.lights, None);
    snapshot.apply_packet(&ServerPacket::TimeOfDay(TimeOfDay {
        lights: LightSetting::Evening,
    }));
    // 天气随 `S.MapChanged.weather` 下发（协议没有独立的 `S.Weather`）。
    snapshot.apply_packet(&ServerPacket::MapChanged(MapChanged {
        weather: WeatherSetting::Rain,
        ..MapChanged::default()
    }));
    assert_eq!(snapshot.lights, Some(LightSetting::Evening));
    assert_eq!(snapshot.weather, Some(WeatherSetting::Rain));
    assert_eq!(
        hud::time_weather_text(&snapshot, Language::ZhCn),
        "黄昏 · 雨"
    );
    assert_eq!(
        hud::time_weather_text(&snapshot, Language::En),
        "Evening · Rain"
    );

    let text = text();
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    let zh = frame_with_language(&mut state, &snapshot, &text, Language::ZhCn);
    let en = frame_with_language(&mut state, &snapshot, &text, Language::En);
    let status = hud::status_layout(Rect::screen(800, 600));
    assert!(
        changed_pixels(&zh, &en, status.time) > 5,
        "时间/天气文案应当随语言变化"
    );
    en.save_png(screenshot("time-weather"))
        .expect("保存时间天气截图");
}

// ===== 第 5 轮：宠物模式 / 任务物品滚动 / @ 命令 =====

#[test]
fn pet_mode_cycles_and_prints_localized_chat() {
    let text = text();
    let mut snapshot = snapshot_with_player(0, &[]);
    // `S.ObjectMonster.master_object_id` 指向自己 = 宠物。
    snapshot.apply_packet(&ServerPacket::ObjectMonster(ObjectMonster {
        object_id: 42,
        master_object_id: 7,
        name: "神兽".to_owned(),
        ..ObjectMonster::default()
    }));
    assert!(snapshot.has_pet());
    assert_eq!(snapshot.pet_object_id(), Some(42));

    // 三态循环：跟随 → 攻击 → 休息 → 跟随（`C.ChangePMode` 的三种）。
    let mode = next_pet_mode(snapshot.pet_mode);
    assert_eq!(mode, PetMode::AttackOnly);
    snapshot.pet_mode = mode;
    snapshot.notify(SystemChat::PetModeChanged { mode });
    let line = snapshot.chat.last().expect("宠物模式提示");
    assert_eq!(
        labels::chat_line_text(line, Language::ZhCn),
        "宠物模式：攻击"
    );
    assert_eq!(
        labels::chat_line_text(line, Language::En),
        "Pet mode: Attack"
    );

    // 无宠物时给提示（客户端只提示，不发包）。
    let mut no_pet = snapshot_with_player(0, &[]);
    assert!(!no_pet.has_pet());
    no_pet.notify(SystemChat::NoPet);
    assert_eq!(
        labels::chat_line_text(no_pet.chat.last().expect("无宠物提示"), Language::En),
        "No pet to command."
    );

    // 截图：HUD + 聊天行（中英各一张）。
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    let zh = frame_with_language(&mut state, &snapshot, &text, Language::ZhCn);
    let en = frame_with_language(&mut state, &snapshot, &text, Language::En);
    let chat = hud::chat_layout(Rect::screen(800, 600));
    assert!(
        changed_pixels(&zh, &en, chat) > 20,
        "宠物模式提示要随语言变化"
    );
    zh.save_png(screenshot("pet-mode-zh"))
        .expect("保存宠物模式截图");
    en.save_png(screenshot("pet-mode-en"))
        .expect("保存宠物模式英文截图");
}

#[test]
fn quest_items_strip_scrolls_with_wheel_and_pages() {
    let text = text();
    let mut snapshot = snapshot_with_player(0, &[]);
    for index in 0..40u64 {
        snapshot.quest_items.push(InventoryItem {
            unique_id: 1_000 + index,
            item_index: 1000,
            // 数量不同，翻页截图/像素对比能看出内容变了（图标是同一占位方块）。
            count: u16::try_from(index % 4).unwrap_or(0) + 2,
            current_dura: 0,
            max_dura: 0,
        });
    }
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    state.show_quests = true;

    // 先画一帧：绘制会把「任务物品」条矩形与容量写回 UiState。
    let (mut scene, mut frame, center) = scene_and_frame(800, 600);
    let entities = snapshot.entity_views(0);
    scene.render_with_entities(&mut frame, center, &entities);
    ui::draw(
        &mut state, &mut frame, &mut scene, &snapshot, &text, None, center,
    );
    let capacity = state.quest_item_capacity;
    assert!(capacity > 0 && capacity < 40, "40 格需要分页");
    let items_rect = state.quest_items_rect;
    let layout = quests::quest_layout(Rect::screen(800, 600), 0, 0);
    assert_eq!(items_rect, layout.items);

    // 滚轮落在物品条上：翻物品页，不动任务列表。
    state.mouse = Point::new(items_rect.x + 2, items_rect.y + 2);
    assert!(state.on_wheel(&snapshot, 1));
    assert_eq!(state.quest_item_scroll, 1);
    assert_eq!(state.quest_scroll, 0);

    // 往下滚到底会被夹到 max（40 - capacity）。
    for _ in 0..60 {
        state.on_wheel(&snapshot, 1);
    }
    assert_eq!(state.quest_item_scroll, 40 - capacity);

    // 两页都画出来：物品条像素必须不同（分页生效），并保存截图。
    state.quest_item_scroll = 0;
    let first = frame_with_language(&mut state, &snapshot, &text, Language::ZhCn);
    state.quest_item_scroll = 40 - capacity;
    let last = frame_with_language(&mut state, &snapshot, &text, Language::ZhCn);
    assert!(
        changed_pixels(&first, &last, items_rect) > 20,
        "翻页后任务物品条应当重绘"
    );
    first
        .save_png(screenshot("quest-items-page1"))
        .expect("保存任务物品第一页");
    last.save_png(screenshot("quest-items-page2"))
        .expect("保存任务物品最后一页");

    // 滚轮不在物品条上时仍滚任务列表（任务列表为空时返回 false）。
    state.mouse = Point::new(items_rect.x + 2, items_rect.y - 40);
    assert!(!state.on_wheel(&snapshot, 1));
}

#[test]
fn at_prefixed_commands_are_sent_as_plain_chat() {
    // 客户端不解析 `@`：原样作为 `C.Chat` 发送（真正的命令列表只在服务端）。
    // 这里复用 app 的单测口径；集成侧确认 `@help` 的服务端回复按原文显示。
    let mut snapshot = snapshot_with_player(0, &[]);
    snapshot.apply_packet(&ServerPacket::Chat(Chat {
        message: "@help → /language, @move, @level".to_owned(),
        type_: ChatType::System,
    }));
    let line = snapshot.chat.last().expect("@help 回复");
    assert_eq!(
        labels::chat_line_text(line, Language::En),
        "@help → /language, @move, @level",
        "服务端回复原样显示，不二次翻译"
    );
}

#[test]
fn quest_output_message_is_visible_after_quest_item_gained() {
    // 服务端 t34 顺序：205（任务物品）→ 223（SendOutputMessage{type=Quest}）。
    let text = text();
    let mut snapshot = snapshot_with_player(0, &[]);
    snapshot.apply_packet(&ServerPacket::GainedQuestItem(GainedQuestItem {
        item: user_item(900, 1000, 1),
    }));
    snapshot.apply_packet(&ServerPacket::SendOutputMessage(SendOutputMessage {
        message: "你找到了 鹿肉".to_owned(),
        type_: OutputMessageType::Quest,
    }));
    assert_eq!(snapshot.chat.len(), 2, "205 和 223 都要进聊天日志");
    let output = snapshot.chat.last().expect("223 输出");
    assert_eq!(output.text, "你找到了 鹿肉", "服务端文本原样显示");
    assert_eq!(output.kind, ChatType::System);
    assert!(matches!(
        snapshot.chat[0].system,
        Some(SystemChat::QuestItemGained { .. })
    ));
    assert!(matches!(
        &snapshot.chat[0].system,
        Some(SystemChat::QuestItemGained { name: Some(name), .. }) if name == "元宝商品"
    ));

    // 英文界面下服务端文本同样原样显示（不二次翻译），聊天面板能画出来。
    let mut state = UiState::new(Language::En);
    state.sync(&snapshot);
    let en = frame_with_language(&mut state, &snapshot, &text, Language::En);
    en.save_png(screenshot("quest-found"))
        .expect("保存任务物品获得提示截图");
}

#[test]
fn r50c_gameshop_hotkey_is_isolated_from_market_tab() {
    // R50-C：独立商城（固定热键 `K`）与市场「商城页签」共存且状态/数据隔离。
    // 商城目录来自 `S.GameShopInfo`，市场列表来自 `S.NPCMarket{ItemType=GameShop}`，
    // 两套状态（页签/选中/搜索）互不覆盖；`Esc` 先关最上层的商城再关市场。
    let text = text();
    let mut snapshot = snapshot_with_player(500, &[]);
    snapshot.apply_packet(&ServerPacket::NPCMarket(
        mir_protocol::packets::server::NPCMarket {
            listings: vec![auction(1, 1000, 1, 100)],
            pages: 1,
            user_mode: false,
        },
    ));
    assert_eq!(snapshot.market.listings.len(), 1);
    assert!(
        snapshot.gameshop.is_empty(),
        "S.NPCMarket 不得污染商城目录（数据源隔离）"
    );

    let mut state = UiState::new(Language::En);
    state.set_time(1_700_000_000_000);
    state.sync(&snapshot);
    state.show_market = true;
    state.market_tab = mir_core::enums::MarketPanelType::GameShop;
    state.market_selected = Some(1);
    state.market_search.set("元宝");

    assert!(state.toggle_gameshop(None), "无输入焦点时 K 打开商城");
    assert!(state.show_gameshop);
    assert!(state.show_market, "打开商城不得关掉市场页签");
    assert_eq!(state.market_tab, mir_core::enums::MarketPanelType::GameShop);
    assert_eq!(state.market_selected, Some(1));
    assert_eq!(state.market_search.value, "元宝");

    // 两窗同开也能画（商城画在市场之后/上层）；证据写入 R50-C 专属证据目录。
    let frame = frame_with_language(&mut state, &snapshot, &text, Language::En);
    let evidence_dir = workspace_root().join("target/round50/r50-c");
    std::fs::create_dir_all(&evidence_dir).expect("创建 R50-C 证据目录");
    frame
        .save_png(evidence_dir.join("gameshop-market-coexist.png"))
        .expect("保存商城与市场页签共存截图");
    assert!(state.show_gameshop);
    assert!(state.show_market);

    // Esc：先关商城、市场保留；再按一次才关市场。
    assert!(matches!(
        state.escape(),
        mir_client::ui::EscapeResult::Panels
    ));
    assert!(!state.show_gameshop);
    assert!(state.show_market);
    assert_eq!(state.market_search.value, "元宝");
    assert!(matches!(
        state.escape(),
        mir_client::ui::EscapeResult::Panels
    ));
    assert!(!state.show_market);
}
