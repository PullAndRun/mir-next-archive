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

use std::path::{Path, PathBuf};

use mir_client::geom::Rect;
use mir_client::net::{CharacterInfo, NetCommand, WorldSnapshot};
use mir_client::settings::ClientSettings;
use mir_client::ui::settings::{self as settings_ui, SettingsAction};
use mir_client::ui::{self, UiAction, UiState, UiText, market, select};
use mir_core::enums::{
    ItemType, MarketItemType, MarketPanelType, MirClass, MirDirection, MirGender, Spell,
};
use mir_core::{DateTimeBinary, Language, Point};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ClientAuction, ItemInfo, SelectInfo, UserItem};
use mir_protocol::packets::server::{
    LoginSuccess, Magic, NPCMarket, NewItemInfo, ObjectHidden, ObjectMagic, ObjectSpell,
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
    UiText::load(None)
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
