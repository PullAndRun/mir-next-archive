//! 客户端市场/寄售/拍卖面板的无头测试（不连服务器、不开窗口）。
//!
//! 流程与 `tests/trade_panel.rs` 一致：用 `WorldSnapshot::apply_packet` 注入真实协议包
//! （`S.NPCMarket` / `S.NPCMarketPage` / `S.NPCConsign` / `S.ConsignItem` /
//! `S.MarketFail` / `S.MarketSuccess`），渲染整屏到 `target/client-market.png`，
//! 再用布局函数算出的矩形点按钮，断言 [`UiAction`] 与 [`NetCommand`] 正确。
//!
//! 覆盖：
//!
//! 1. 布局在 320×240 与大屏都不出屏（三个页签 × 两页）；
//! 2. 快照：列表/页码/UserMode/失败提示/成功文案/上架回执；
//! 3. 失败码 0..=10 的中英文案（9=BidTooLow、10=AuctionEnded 不能漏）；
//! 4. 交互：搜索、翻页、买断、出价预填、寄售上架、取回、一键收款、提前成交；
//! 5. `UiAction` 与 `NetCommand` 一一对应。

mod common;

use std::path::{Path, PathBuf};

use mir_client::geom::Rect;
use mir_client::net::{NetCommand, WorldSnapshot};
use mir_client::ui::{self, FieldId, UiAction, UiState, UiText, inventory, market};
use mir_core::enums::{
    ItemType, MarketCollectionMode, MarketItemType, MarketPanelType, MirClass, MirDirection,
    MirGender,
};
use mir_core::{DateTimeBinary, Language, Point};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ClientAuction, ItemInfo, UserItem};
use mir_protocol::packets::server::{
    ConsignItem as ConsignItemPacket, MarketFail, MarketSuccess, NPCConsign, NPCMarket,
    NPCMarketPage, NewItemInfo, UserInformation,
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

/// 无 `resource/` 时的合成地图。
fn synthetic_map() -> Map {
    let width = 64;
    let height = 64;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "synthetic-market".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
}

/// 让 `MapScene` 与 `Frame` 就绪，并返回「只有世界」的底帧。
fn scene_and_frame() -> (MapScene, Frame, Point) {
    let scene = MapScene::from_parts(
        synthetic_map(),
        TileLibraries::empty("."),
        Viewport::DEFAULT,
    );
    let frame = Frame::new(800, 600);
    (scene, frame, Point::new(32, 32))
}

/// 渲染一份快照 + 界面状态：先画世界，再叠 UI；返回 `(世界底帧, 成帧)`。
fn render(state: &mut UiState, snapshot: &WorldSnapshot, text: &UiText) -> (Frame, Frame) {
    let (mut scene, mut frame, center) = scene_and_frame();
    let entities = snapshot.entity_views(0);
    scene.render_with_entities(&mut frame, center, &entities);
    let world_only = frame.clone();
    ui::draw(state, &mut frame, &mut scene, snapshot, text, None, center);
    (world_only, frame)
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

/// 一条市场条目。
fn auction(
    auction_id: u64,
    item_index: i32,
    count: u16,
    seller: &str,
    price: u32,
    item_type: MarketItemType,
) -> ClientAuction {
    ClientAuction {
        auction_id,
        item: user_item(auction_id + 1_000, item_index, count, 0),
        seller: seller.to_owned(),
        price,
        consignment_date: DateTimeBinary::from_unix_seconds(1_704_067_200),
        item_type,
    }
}

/// 基础世界：自己 + 两件背包物品 + 物品模板。
fn base_snapshot() -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    let mut slots = vec![None; inventory::INVENTORY_SLOTS];
    slots[0] = Some(user_item(11, 1, 1, 5_000));
    slots[1] = Some(user_item(12, 2, 7, 0));
    snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
        object_id: 7,
        real_id: 7,
        name: "市场测试".to_owned(),
        location: Point::new(32, 32),
        direction: MirDirection::Down,
        class: MirClass::Warrior,
        gender: MirGender::Male,
        level: 12,
        hair: 1,
        hp: 50,
        mp: 30,
        gold: 100_000,
        inventory: Some(slots),
        ..UserInformation::default()
    }));
    snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
        info: item_info(1, "木剑", 1, 1_200, 1),
    }));
    snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
        info: item_info(2, "金创药", 2, 50, 100),
    }));
    snapshot
}

/// 市场页打开后的快照：一条寄售 + 一条拍卖。
fn listings_snapshot() -> WorldSnapshot {
    let mut snapshot = base_snapshot();
    snapshot.apply_packet(&ServerPacket::NPCMarket(NPCMarket {
        listings: vec![
            auction(100, 1, 1, "张三", 1_200, MarketItemType::Consign),
            auction(101, 2, 3, "李四", 5_000, MarketItemType::Auction),
        ],
        pages: 1,
        user_mode: false,
    }));
    snapshot
}

/// 10 条一页的列表（用于翻页测试）。
fn paged_snapshot() -> WorldSnapshot {
    let mut snapshot = base_snapshot();
    let first_page = (0..10)
        .map(|index| {
            auction(
                200 + index,
                1,
                1,
                "张三",
                1_000 + u32::try_from(index).unwrap_or(0),
                MarketItemType::Consign,
            )
        })
        .collect();
    snapshot.apply_packet(&ServerPacket::NPCMarket(NPCMarket {
        listings: first_page,
        pages: 2,
        user_mode: false,
    }));
    snapshot
}

/// 布局里的所有矩形（测试用：全部必须落在面板/屏幕内）。
fn all_rects(layout: &market::MarketLayout) -> Vec<Rect> {
    let mut rects = vec![
        layout.panel,
        layout.close,
        layout.gold,
        layout.toolbar,
        layout.search_field,
        layout.find,
        layout.refresh,
        layout.price_field,
        layout.sell,
        layout.consign_item,
        layout.header,
        layout.item_header,
        layout.price_header,
        layout.seller_header,
        layout.expiry_header,
        layout.pager,
        layout.prev,
        layout.page_label,
        layout.next,
        layout.action,
        layout.bid_field,
        layout.primary,
        layout.sell_now,
        layout.collect,
        layout.footer,
    ];
    rects.extend(layout.tabs.iter().copied());
    rects.extend(layout.rows.iter().copied());
    rects
}

#[test]
fn market_layouts_fit_small_and_large_screens() {
    for (width, height) in [(320, 240), (800, 600)] {
        let screen = Rect::screen(width, height);
        for tab in market::TABS {
            for page in [0, 1] {
                let layout = market::market_layout(screen, tab, 13, page, 0);
                assert!(
                    layout.panel.x >= 0
                        && layout.panel.y >= 0
                        && layout.panel.right() <= i32::try_from(width).unwrap_or(i32::MAX)
                        && layout.panel.bottom() <= i32::try_from(height).unwrap_or(i32::MAX),
                    "{width}×{height} {tab:?} 面板出屏：{:?}",
                    layout.panel
                );
                for rect in all_rects(&layout) {
                    assert!(
                        rect.width > 0 && rect.height > 0,
                        "{width}×{height} {tab:?} 出现空矩形：{rect:?}"
                    );
                    assert!(
                        layout.panel.contains(rect.center()),
                        "{width}×{height} {tab:?} 矩形中心不在面板内：{rect:?} / {:?}",
                        layout.panel
                    );
                }
                // 第 2 轮加了第四个「商城」页签。
                assert_eq!(layout.tabs.len(), 4);
                assert_eq!(layout.tabs.len(), market::TABS.len());
                if page == 0 {
                    assert_eq!(layout.first_row, 0);
                } else {
                    assert_eq!(layout.first_row, 10);
                }
                if tab == MarketPanelType::Market {
                    assert!(layout.rows.len() <= market::MAX_ROWS);
                }
            }
        }
    }
    // 行命中测试返回列表绝对下标（含页码偏移）。
    let screen = Rect::screen(800, 600);
    let layout = market::market_layout(screen, MarketPanelType::Market, 13, 1, 0);
    assert_eq!(layout.row_at(layout.rows[0].center()), Some(10));
    assert_eq!(layout.row_at(Point::new(1, 1)), None);
}

#[test]
fn market_snapshot_tracks_listings_pages_and_messages() {
    let snapshot = listings_snapshot();
    assert!(snapshot.market.open, "S.NPCMarket 应打开市场窗口");
    assert!(!snapshot.market.user_mode);
    assert_eq!(snapshot.market.pages, 1);
    assert_eq!(snapshot.market.listings.len(), 2);
    assert_eq!(snapshot.market.listings[0].name, "木剑");
    assert_eq!(snapshot.market.listings[0].image, 1);
    assert_eq!(snapshot.market.listings[0].seller, "张三");
    assert_eq!(snapshot.market.listings[0].price, 1_200);
    assert!(snapshot.market.listings[1].is_bid());
    assert_eq!(snapshot.packet_count("NPCMarket"), 1);

    // 翻页：追加一页并记 page_version，Crystal 用 (Listings.Count - 1) / 10 算当前页。
    let mut paged = paged_snapshot();
    let page_version = paged.market.page_version;
    paged.apply_packet(&ServerPacket::NPCMarketPage(NPCMarketPage {
        listings: vec![
            auction(300, 1, 1, "王五", 2_000, MarketItemType::Consign),
            auction(301, 1, 1, "王五", 2_100, MarketItemType::Consign),
            auction(302, 1, 1, "王五", 2_200, MarketItemType::Consign),
        ],
    }));
    assert_eq!(paged.market.listings.len(), 13);
    assert!(paged.market.page_version > page_version);
    assert_eq!(paged.packet_count("NPCMarketPage"), 1);

    // UI sync：打开时显示背包、当前页按列表长度回落。
    let text = common::load_text();
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&paged);
    assert!(state.show_market);
    assert!(
        state.show_inventory,
        "市场打开时背包要一起显示（寄售要选物品）"
    );
    assert_eq!(state.market_page, 1, "13 条 → 当前页 = (13-1)/10 = 1");
    assert_eq!(state.market_tab, MarketPanelType::Market);

    // 失败的版本号 + 原因（本地化文案在底部显示，见 `market_fail_reason_shows_localized_text`）。
    paged.apply_packet(&ServerPacket::MarketFail(MarketFail { reason: 4 }));
    assert_eq!(paged.market.fail, Some(4));
    assert_eq!(paged.market.fail_version, 1, "S.MarketFail 应推进版本号");
    assert_eq!(
        market::fail_label(4).text(Language::ZhCn),
        "金币不足",
        "失败码 4 = LowGold"
    );

    // 成功后清掉失败、记服务端文案并按原样进聊天（Crystal `GameScene.cs:5710-5714`）。
    paged.apply_packet(&ServerPacket::MarketSuccess(MarketSuccess {
        message: "你以 1,200 金币买下了 木剑。".to_owned(),
    }));
    assert_eq!(paged.market.fail, None);
    assert_eq!(
        paged.market.message.as_deref(),
        Some("你以 1,200 金币买下了 木剑。")
    );
    assert_eq!(
        paged.chat.last().map(|line| line.text.as_str()),
        Some("你以 1,200 金币买下了 木剑。")
    );

    // 寄售面板：`S.NPCConsign` 打开并切到寄售页签。
    paged.apply_packet(&ServerPacket::NPCConsign(NPCConsign {}));
    state.sync(&paged);
    assert!(paged.market.consign_open);
    assert!(state.show_market);
    assert_eq!(state.market_tab, MarketPanelType::Consign);
    assert!(
        state.market_consign_only,
        "S.NPCConsign 会话的 NPC 页是 [@CONSIGN]，搜索会被服务端拒绝"
    );
    // 切到别的页签要先回 `[@MARKET]` 页（`C.CallNPC{Key="[@MARKET]"}`）。
    let layout = market::market_layout(
        Rect::screen(800, 600),
        state.market_tab,
        paged.market.listings.len(),
        state.market_page,
        state.market_scroll,
    );
    state.mouse = layout.tabs[0].center();
    match state.on_click(&paged, &text, Rect::screen(800, 600), None) {
        Some(UiAction::CallNpc { key, .. }) => assert_eq!(key, market::MARKET_KEY),
        other => panic!("纯寄售会话切页签应先 @MARKET，实际 {other:?}"),
    }
    assert!(!state.market_consign_only);

    // 上架回执：成功后把物品从背包移除（服务端只回执，`GameScene.cs:5655-5668`）。
    assert!(paged.inventory_item(0).is_some());
    paged.apply_packet(&ServerPacket::ConsignItem(ConsignItemPacket {
        unique_id: 11,
        success: true,
    }));
    assert!(
        paged.inventory_item(0).is_none(),
        "S.ConsignItem{{Success=true}} 应把背包物品拿掉"
    );
    assert_eq!(paged.market.consign_result, Some((11, true)));
    assert_eq!(paged.market.consign_version, 1);

    // Esc 关窗：只关本地（Crystal `Hide()` 不发包）。
    assert_eq!(state.escape(), ui::EscapeResult::Panels);
    assert!(!state.show_market);

    // 重新打开（新的 UI 状态收到一份 `S.NPCMarket`）时按当前页签补搜索
    // （app.rs 用这个列表发 `C.MarketSearch`）。
    let mut reopened = UiState::new(Language::ZhCn);
    reopened.sync(&listings_snapshot());
    let requests = reopened.market_open_requests();
    assert_eq!(requests.len(), 1);
    assert!(matches!(
        requests[0],
        UiAction::MarketSearch {
            user_mode: false,
            panel: MarketPanelType::Market,
            max_shape: 5000,
            ..
        }
    ));
    let _ = text;
}

#[test]
fn market_fail_reason_shows_localized_text() {
    // Crystal 全表 0..=10（`GameScene.cs:5669-5707`），9/10 是 BidTooLow / AuctionEnded。
    for reason in 0..=10u8 {
        let label = market::fail_label(reason);
        let zh = label.text(Language::ZhCn);
        let en = label.text(Language::En);
        assert!(!zh.is_empty(), "失败码 {reason} 缺中文");
        assert!(!en.is_empty(), "失败码 {reason} 缺英文");
        assert_ne!(zh, en, "失败码 {reason} 中英文不应相同");
        assert_ne!(
            label,
            mir_client::ui::labels::Label::MarketFailGeneric,
            "失败码 {reason} 不应落到通用文案"
        );
    }
    assert_eq!(
        market::fail_label(9).text(Language::ZhCn),
        "出价太低",
        "9 = BidTooLow"
    );
    assert_eq!(
        market::fail_label(10).text(Language::ZhCn),
        "拍卖已结束",
        "10 = AuctionEnded"
    );
    assert_eq!(market::fail_label(4).text(Language::ZhCn), "金币不足");
    assert_eq!(market::fail_label(4).text(Language::En), "Not enough gold");
    assert_eq!(
        market::fail_label(255),
        mir_client::ui::labels::Label::MarketFailGeneric
    );
}

#[test]
fn market_panel_search_page_buy_bid_and_consign() {
    let text = common::load_text();
    let screen = Rect::screen(800, 600);
    let snapshot = listings_snapshot();
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    assert!(state.show_market);

    // 搜索：点输入框 → 输入 → 点「搜索」→ `C.MarketSearch`。
    let layout = market::market_layout(
        screen,
        state.market_tab,
        snapshot.market.listings.len(),
        state.market_page,
        state.market_scroll,
    );
    state.mouse = layout.search_field.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.focus, Some(FieldId::MarketSearch));
    for character in "木剑".chars() {
        assert!(state.type_char(character));
    }
    state.mouse = layout.find.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("搜索按钮应产生动作");
    assert_eq!(
        action,
        UiAction::MarketSearch {
            match_text: "木剑".to_owned(),
            item_type: ItemType::Nothing,
            user_mode: false,
            min_shape: 0,
            max_shape: 5000,
            panel: MarketPanelType::Market,
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::MarketSearch {
            match_text: "木剑".to_owned(),
            item_type: ItemType::Nothing,
            user_mode: false,
            min_shape: 0,
            max_shape: 5000,
            panel: MarketPanelType::Market,
        })
    );
    state.focus = None;

    // 选中寄售行 → 主按钮「买下」→ `C.MarketBuy`（寄售 BidPrice 只是占位）。
    state.mouse = layout.rows[0].center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.market_selected, Some(100));
    assert!(state.market_price.is_empty(), "寄售不需要预填出价");
    state.mouse = layout.primary.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("买下按钮应产生动作");
    assert_eq!(
        action,
        UiAction::MarketBuy {
            auction_id: 100,
            bid_price: 1_200
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::MarketBuy {
            auction_id: 100,
            bid_price: 1_200
        })
    );

    // 选中拍卖行 → 出价框预填「当前价 + 1」并聚焦 → 主按钮「出价」。
    state.mouse = layout.rows[1].center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.market_selected, Some(101));
    assert_eq!(state.market_price.number(), 5_001);
    assert_eq!(state.focus, Some(FieldId::MarketPrice));
    state.mouse = layout.primary.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("出价按钮应产生动作");
    assert_eq!(
        action,
        UiAction::MarketBuy {
            auction_id: 101,
            bid_price: 5_001
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::MarketBuy {
            auction_id: 101,
            bid_price: 5_001
        })
    );

    // 切到寄售页签 → 发 `C.MarketSearch{UserMode=true, MarketType=Consign}`。
    state.mouse = layout.tabs[1].center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("切页签应产生搜索动作");
    assert!(matches!(
        action,
        UiAction::MarketSearch {
            user_mode: true,
            panel: MarketPanelType::Consign,
            ..
        }
    ));
    assert_eq!(state.market_tab, MarketPanelType::Consign);
    assert!(action.net_command().is_some());

    // 寄售：在背包里选中「木剑」→ 填 5000 → 点「上架」→ `C.ConsignItem`。
    let inventory_panel = inventory::inventory_panel_rect(screen);
    state.mouse = inventory::inventory_slot_rect(inventory_panel, 0)
        .expect("背包 0 号格")
        .center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.selected_slot, Some(0));

    let consign_layout = market::market_layout(
        screen,
        state.market_tab,
        snapshot.market.listings.len(),
        state.market_page,
        state.market_scroll,
    );
    state.mouse = consign_layout.price_field.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.focus, Some(FieldId::MarketPrice));
    for character in "5000".chars() {
        assert!(state.type_char(character));
    }
    state.mouse = consign_layout.sell.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("上架按钮应产生动作");
    assert_eq!(
        action,
        UiAction::ConsignItem {
            unique_id: 11,
            price: 5_000,
            panel: MarketPanelType::Consign,
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::ConsignItem {
            unique_id: 11,
            price: 5_000,
            panel: MarketPanelType::Consign,
        })
    );

    // 寄售页选中自己的条目 → 「取回」= `C.MarketGetBack{Any}`；「一键收款」= `{Sold}`。
    state.mouse = consign_layout.rows[0].center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    state.mouse = consign_layout.primary.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("取回按钮应产生动作");
    assert_eq!(
        action,
        UiAction::MarketGetBack {
            mode: MarketCollectionMode::Any,
            auction_id: 100,
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::MarketGetBack {
            mode: MarketCollectionMode::Any,
            auction_id: 100,
        })
    );
    state.mouse = consign_layout.collect.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("一键收款应产生动作");
    assert_eq!(
        action,
        UiAction::MarketGetBack {
            mode: MarketCollectionMode::Sold,
            auction_id: 0,
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::MarketGetBack {
            mode: MarketCollectionMode::Sold,
            auction_id: 0,
        })
    );

    // 拍卖页签：主按钮变「取回」；选中已有人出价的条目 → 「提前成交」。
    state.market_tab = MarketPanelType::Auction;
    let auction_layout = market::market_layout(
        screen,
        state.market_tab,
        snapshot.market.listings.len(),
        state.market_page,
        state.market_scroll,
    );
    state.mouse = auction_layout.rows[1].center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    state.mouse = auction_layout.sell_now.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("提前成交按钮应产生动作");
    assert_eq!(action, UiAction::MarketSellNow { auction_id: 101 });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::MarketSellNow { auction_id: 101 })
    );

    // 关闭按钮只关窗。
    state.mouse = auction_layout.close.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert!(!state.show_market);
}

#[test]
fn market_panel_pages_with_buttons_and_only_requests_unfetched_pages() {
    let text = common::load_text();
    let screen = Rect::screen(800, 600);
    let mut snapshot = paged_snapshot();
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    assert_eq!(state.market_page, 0);

    let layout = market::market_layout(
        screen,
        state.market_tab,
        snapshot.market.listings.len(),
        state.market_page,
        state.market_scroll,
    );
    assert_eq!(layout.rows.len(), 10, "800×600 下一屏放得下 10 行");

    // 下一页还没拿过 → 发 `C.MarketPage{1}`，本地页不提前变。
    state.mouse = layout.next.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("下一页应产生动作");
    assert_eq!(action, UiAction::MarketPage { page: 1 });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::MarketPage { page: 1 })
    );
    assert_eq!(state.market_page, 0);

    // 服务端回一页 → 列表累积、当前页按 (13-1)/10 = 1。
    snapshot.apply_packet(&ServerPacket::NPCMarketPage(NPCMarketPage {
        listings: vec![
            auction(400, 1, 1, "王五", 3_000, MarketItemType::Consign),
            auction(401, 1, 1, "王五", 3_100, MarketItemType::Consign),
            auction(402, 1, 1, "王五", 3_200, MarketItemType::Consign),
        ],
    }));
    state.sync(&snapshot);
    assert_eq!(snapshot.market.listings.len(), 13);
    assert_eq!(state.market_page, 1);

    let page_one = market::market_layout(screen, state.market_tab, 13, 1, 0);
    assert_eq!(page_one.row_at(page_one.rows[0].center()), Some(10));
    assert_eq!(page_one.rows.len(), 3, "第二页只剩 3 条");

    // 上一页在本地翻转，不发包。
    state.mouse = page_one.prev.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.market_page, 0);

    // 再翻到第 1 页：已经拿过 → 本地翻，不发包。
    let page_zero = market::market_layout(screen, state.market_tab, 13, 0, 0);
    state.mouse = page_zero.next.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.market_page, 1);

    // 第 2 页不存在（pages = 2），点下一页不动。
    let page_one = market::market_layout(screen, state.market_tab, 13, 1, 0);
    state.mouse = page_one.next.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.market_page, 1);
}

#[test]
fn market_panel_renders_and_saves_png() {
    let text = common::load_text();
    let snapshot = listings_snapshot();
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    let layout = market::market_layout(
        Rect::screen(800, 600),
        state.market_tab,
        snapshot.market.listings.len(),
        state.market_page,
        state.market_scroll,
    );
    let (world_only, frame) = render(&mut state, &snapshot, &text);
    let changed = changed_pixels(&world_only, &frame, layout.panel);
    assert!(changed > 5_000, "市场面板应覆盖地图像素，实际 {changed}");
    assert!(
        changed_pixels(&world_only, &frame, layout.rows[0]) > 100,
        "第一行应画上物品"
    );

    // 失败提示画在底部：同一帧注入失败前后像素不同。
    // 提示与失败原因都是中文：无 CJK 字形的环境两者都为空，像素差恒为 0，跳过并打印原因。
    let mut failed = snapshot.clone();
    failed.apply_packet(&ServerPacket::MarketFail(MarketFail { reason: 4 }));
    let (_, failed_frame) = render(&mut state, &failed, &text);
    if !common::skip_if_no_cjk(&text, "市场失败提示") {
        assert!(
            changed_pixels(&frame, &failed_frame, layout.footer) > 5,
            "失败原因应显示在底部"
        );
    }

    let output = workspace_root().join("target/client-market.png");
    frame
        .save_png(&output)
        .expect("写出 target/client-market.png");
    assert!(std::fs::metadata(&output).is_ok_and(|meta| meta.len() > 4_096));
}
