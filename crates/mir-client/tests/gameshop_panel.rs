//! R50-C：独立商城面板（固定热键 `K`）的集成测试。
//!
//! 覆盖点（每条断言都对准 Crystal `GameshopDialog` 的语义，改坏实现会红）：
//!
//! 1. **真实包链**：`S.GameShopInfo`/`S.GameShopStock` 经
//!    `ServerPacket::encode_frame → codec::FrameBuffer → decode_server_packet` 的
//!    编解码链进入 `WorldSnapshot.gameshop`——与 `net.rs` 后台线程收包后走的是同一套
//!    解码代码，不用手搓 payload；同一 `GIndex` 的迟到/重复目录包不重复入列（C4）。
//! 2. **目录/库存语义**：`S.GameShopStock{StockLevel = 0}` 标「售罄」但保留条目可见
//!    （R50 §1.3 库存零可见；Crystal 会删条目），未知 `GIndex` 不凭空造条目。
//! 3. **筛选**：职业 / 分区 / 分类 / 搜索按 `GameshopDialog.GetCategories()` 的顺序
//!    过滤，结果按物品名排序；分类列表按出现顺序去重。
//! 4. **热键守卫**：`K` 只在无输入焦点时开关；目录到达不会自动弹窗；面板单实例
//!    （重复按是开/关切换而不是叠窗口）；`Esc` 关面板不发包。
//! 5. **双币购买**：点商品格 → 选份数 → 切元宝/金币 → 点「买下」产出
//!    `UiAction::GameShopBuy{GIndex, Quantity, PType}`，再映射成 `C.GameshopBuy`；
//!    余额不足/不支持该货币/售罄时本地拦截不发包。
//! 6. **与市场页签隔离**：市场页签的选中/页码/搜索状态不被商城操作改动，反之亦然。
//! 7. **绘制**：非空目录、空目录、断线态都能画出来（像素证据存
//!    `target/round50/r50-c/`），320×240 不出屏、不 panic。
//!
//! 说明：`K` 只在窗口层（`app.rs::handle_live_key`）的 live 分支里调用
//! [`UiState::toggle_gameshop`]，输入焦点由 `app.rs` 的文本输入早退先挡掉；本文件用
//! [`UiState::toggle_gameshop`] 自身的焦点守卫做第二道防线的断言（删掉守卫会红），
//! 窗口层早退的顺序证据见 `target/round50/r50-c/k-hotkey-evidence.txt`。

mod common;

use std::path::{Path, PathBuf};

use mir_client::geom::Rect;
use mir_client::net::{GameShopEntry, NetCommand, NetStatus, WorldSnapshot};
use mir_client::ui::gameshop::{
    self, BuyOrder, BuyProblem, ClassFilter, SectionFilter, StockDisplay,
};
use mir_client::ui::{self, EscapeResult, FieldId, UiAction, UiState, UiText};
use mir_core::enums::{
    ItemType, MarketItemType, MarketPanelType, MirClass, MirDirection, MirGender,
};
use mir_core::{DateTimeBinary, Language, Point};
use mir_protocol::ServerPacket;
use mir_protocol::codec::FrameBuffer;
use mir_protocol::data::{ClientAuction, GameShopItem, ItemInfo, UserItem};
use mir_protocol::packets::server::{
    GameShopInfo, GameShopStock, NPCMarket, UserInformation, decode_server_packet,
};
use mir_render::{Frame, MapScene, TileLibraries, Viewport};
use mir_world::{Map, MapCell, MapVersion};

/// 测试用的「当前时间」（秒），Crystal 新品窗口 = 上架时间在 7 天内。
const NOW_SEC: i64 = 1_700_000_000;
/// 7 天（秒）。
const WEEK_SEC: i64 = 7 * 24 * 60 * 60;

// ===== 路径 =====

/// 工作区根目录（`CARGO_MANIFEST_DIR` 是 `crates/mir-client`）。
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// 证据目录：`target/round50/r50-c/`（R50 契约指定的证据落盘位置）。
fn evidence(name: &str) -> PathBuf {
    let directory = workspace_root().join("target/round50/r50-c");
    std::fs::create_dir_all(&directory).ok();
    directory.join(name)
}

// ===== 造数据 =====

fn item_info(index: i32, name: &str, stack_size: u16) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        image: u16::try_from(index.unsigned_abs() % 20_000).unwrap_or(1),
        stack_size,
        item_type: ItemType::Potion,
        ..ItemInfo::default()
    }
}

/// 一条商城商品（默认：职业 `All`、分类「药品」、金币 100 / 元宝 40、每份 1 个、
/// 上架时间 30 天前、双币可买、不限量）。
fn shop_item(g_index: i32, name: &str) -> GameShopItem {
    GameShopItem {
        item_index: 1000 + g_index,
        g_index,
        info: item_info(1000 + g_index, name, 100),
        gold_price: 100,
        credit_price: 40,
        count: 1,
        class: "All".to_owned(),
        category: "药品".to_owned(),
        stock: 0,
        i_stock: false,
        deal: false,
        top_item: false,
        date: DateTimeBinary::from_unix_seconds(NOW_SEC - 30 * 24 * 60 * 60),
        can_buy_credit: true,
        can_buy_gold: true,
    }
}

/// 登录成功（live 态）的快照：`S.UserInformation` + `NetStatus::InGame`。
fn live_snapshot(gold: u32, credit: u32) -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
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
        gold,
        credit,
        inventory: Some(vec![None; 8]),
        ..UserInformation::default()
    }));
    snapshot.status = NetStatus::InGame;
    snapshot
}

/// 走真实帧编解码链喂一个服务端包（与后台线程 `next_frame` + `decode_server_packet`
/// 同一条路径），并断言帧头包号/解码类型一致。
fn feed_wire(snapshot: &mut WorldSnapshot, packet: &ServerPacket) {
    let bytes = packet.encode_frame();
    let mut frames = FrameBuffer::new();
    frames.push(&bytes);
    let (index, payload) = frames
        .next_frame()
        .expect("拆帧不报错")
        .expect("一次收到整帧");
    assert_eq!(index, packet.index(), "帧头包号与包结构一致");
    assert!(frames.is_empty(), "一帧不应残留字节");
    let decoded = decode_server_packet(index, &payload).expect("解包");
    assert_eq!(
        std::mem::discriminant(&decoded),
        std::mem::discriminant(packet),
        "解出的包类型与编码前一致"
    );
    snapshot.apply_packet(&decoded);
}

/// 只走帧链、不合入快照：返回解码后的包（用于断言 wire 字段）。
fn decode_wire(packet: &ServerPacket) -> ServerPacket {
    let bytes = packet.encode_frame();
    let mut frames = FrameBuffer::new();
    frames.push(&bytes);
    let (index, payload) = frames
        .next_frame()
        .expect("拆帧不报错")
        .expect("一次收到整帧");
    decode_server_packet(index, &payload).expect("解包")
}

fn feed_info(snapshot: &mut WorldSnapshot, item: &GameShopItem, stock_level: i32) {
    feed_wire(
        snapshot,
        &ServerPacket::GameShopInfo(GameShopInfo {
            item: item.clone(),
            stock_level,
        }),
    );
}

fn feed_stock(snapshot: &mut WorldSnapshot, g_index: i32, stock_level: i32) {
    feed_wire(
        snapshot,
        &ServerPacket::GameShopStock(GameShopStock {
            g_index,
            stock_level,
        }),
    );
}

/// 两个商品的目录：`Alpha`（`GIndex` 11）/ `Beta`（`GIndex` 12）。
fn snapshot_with_catalog(gold: u32, credit: u32) -> WorldSnapshot {
    let mut snapshot = live_snapshot(gold, credit);
    let mut alpha = shop_item(11, "Alpha");
    "药品".clone_into(&mut alpha.category);
    let mut beta = shop_item(12, "Beta");
    "武器".clone_into(&mut beta.category);
    beta.date = DateTimeBinary::from_unix_seconds(NOW_SEC - 2 * 24 * 60 * 60);
    feed_info(&mut snapshot, &alpha, 0);
    feed_info(&mut snapshot, &beta, 5);
    snapshot
}

// ===== 界面/帧辅助 =====

fn text() -> UiText {
    UiText::fallback()
}

/// 开面板：`Language::En`（`UiText::fallback()` 的点阵只有 ASCII，像素断言更稳）。
fn open_state(snapshot: &WorldSnapshot) -> UiState {
    let mut state = UiState::new(Language::En);
    state.set_time(NOW_SEC * 1000);
    state.sync(snapshot);
    assert!(
        state.toggle_gameshop(Some(MirClass::Warrior)),
        "焦点为空时 K 应能打开商城"
    );
    state
}

fn layout_for(state: &UiState, snapshot: &WorldSnapshot, frame: Rect) -> gameshop::GameShopLayout {
    let now_sec = state.time_millis / 1000;
    let categories = gameshop::category_names(
        &snapshot.gameshop.entries,
        state.gameshop_class,
        state.gameshop_section,
        &state.gameshop_search.value,
        now_sec,
    );
    gameshop::gameshop_layout(frame, categories.len())
}

fn center(rect: Rect) -> Point {
    Point::new(
        rect.x + i32::try_from(rect.width / 2).unwrap_or(0),
        rect.y + i32::try_from(rect.height / 2).unwrap_or(0),
    )
}

/// 左键点一下（带世界格坐标，验证「面板吃掉点击不穿透成走路」）。
fn click(
    state: &mut UiState,
    snapshot: &WorldSnapshot,
    frame: Rect,
    point: Point,
) -> Option<UiAction> {
    state.mouse = point;
    state.on_click(snapshot, &text(), frame, Some(Point::new(32, 32)))
}

/// 合成地图（不依赖 `resource/`）。
fn synthetic_map() -> Map {
    let width = 64;
    let height = 64;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "synthetic-gameshop".to_owned(),
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

/// 画一整帧界面（世界 + 面板），返回帧。
fn draw_ui(state: &mut UiState, snapshot: &WorldSnapshot, width: u32, height: u32) -> Frame {
    let (mut scene, mut frame, center) = scene_and_frame(width, height);
    let entities = snapshot.entity_views(0);
    scene.render_with_entities(&mut frame, center, &entities);
    ui::draw(
        state,
        &mut frame,
        &mut scene,
        snapshot,
        &text(),
        None,
        center,
    );
    frame
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

// ===== 1. 真实包链与 GIndex 幂等（C4） =====

#[test]
fn wire_packet_chain_catalog_gindex_idempotent_and_stock_refresh() {
    let mut snapshot = live_snapshot(1_000, 500);
    assert!(snapshot.gameshop.is_empty(), "未收到目录前商城为空");
    assert_eq!(snapshot.gameshop.catalog_version, 0);

    // 两个商品经真实编解码链入列；顺序 = 服务端下发顺序。
    let mut alpha = shop_item(11, "Alpha");
    let mut beta = shop_item(12, "Beta");
    beta.info.image = 4;
    feed_info(&mut snapshot, &alpha, 0);
    feed_info(&mut snapshot, &beta, 5);
    assert_eq!(snapshot.gameshop.entries.len(), 2, "两条目录两个条目");
    assert_eq!(snapshot.gameshop.catalog_version, 2);
    assert_eq!(snapshot.gameshop.entries[0].item.g_index, 11);
    assert_eq!(snapshot.gameshop.entries[1].item.g_index, 12);
    assert_eq!(snapshot.gameshop.entry(11).expect("11 在列").stock_level, 0);
    assert_eq!(snapshot.gameshop.entry(12).expect("12 在列").stock_level, 5);

    // 迟到的重复目录包：同一 GIndex 只刷新内容，不重复入列，也不把「不限量 0 档」
    // 误标成售罄（售罄只由 StockLevel = 0 的库存包决定）。
    alpha.gold_price = 250;
    alpha.info.name = "Alpha+".to_owned();
    feed_info(&mut snapshot, &alpha, 0);
    assert_eq!(
        snapshot.gameshop.entries.len(),
        2,
        "重复 GIndex 不得重复入列（C4 幂等）"
    );
    let entry = snapshot.gameshop.entry(11).expect("11 在列");
    assert_eq!(entry.item.gold_price, 250, "迟到包应刷新商品内容");
    assert_eq!(entry.item.info.name, "Alpha+", "迟到包应刷新显示名");
    assert!(!entry.sold_out, "目录包不限量 0 档不等于售罄");
    assert_eq!(
        snapshot.gameshop.catalog_version, 3,
        "每次目录包都推进版本号"
    );

    // 库存刷新：正数覆盖剩余量。
    feed_stock(&mut snapshot, 11, 7);
    assert_eq!(snapshot.gameshop.stock_version, 1);
    let entry = snapshot.gameshop.entry(11).expect("11 在列");
    assert_eq!(entry.stock_level, 7);
    assert!(!entry.sold_out);

    // StockLevel = 0：标售罄但条目保留（R50 §1.3「库存零可见」，Crystal 会删条目）。
    feed_stock(&mut snapshot, 12, 0);
    assert_eq!(snapshot.gameshop.entries.len(), 2, "售罄不删条目");
    let entry = snapshot.gameshop.entry(12).expect("12 仍在列");
    assert_eq!(entry.stock_level, 0);
    assert!(entry.sold_out, "库存刷新到 0 → 售罄可见");

    // 未知 GIndex 的库存包：只推进版本号，不凭空造条目（目录可能还没到）。
    feed_stock(&mut snapshot, 999, 3);
    assert!(
        snapshot.gameshop.entry(999).is_none(),
        "未知 GIndex 不造条目"
    );
    assert_eq!(snapshot.gameshop.entries.len(), 2);
    assert_eq!(snapshot.gameshop.stock_version, 3);

    // 再上架：售罄标记撤销。
    feed_stock(&mut snapshot, 12, 9);
    let entry = snapshot.gameshop.entry(12).expect("12 在列");
    assert_eq!(entry.stock_level, 9);
    assert!(!entry.sold_out, "库存回到正数后恢复可买");

    // wire 字段原样往返（真实包链，不是手搓 payload）。
    let direct = decode_wire(&ServerPacket::GameShopInfo(GameShopInfo {
        item: alpha.clone(),
        stock_level: 7,
    }));
    let ServerPacket::GameShopInfo(info) = direct else {
        panic!("解出的不是 GameShopInfo：{direct:?}");
    };
    assert_eq!(info.item.g_index, 11);
    assert_eq!(info.item.gold_price, 250);
    assert_eq!(info.stock_level, 7);
    assert_eq!(info.item.info.name, "Alpha+");
    assert_eq!(GameShopInfo::INDEX, 250, "S.GameShopInfo 包号");
    assert_eq!(GameShopStock::INDEX, 251, "S.GameShopStock 包号");
}

// ===== 2. 布局命中测试与小屏不出屏 =====

#[test]
fn layout_hit_tests_and_tiny_frames_stay_on_screen() {
    let snapshot = snapshot_with_catalog(1_000, 500);
    let state = open_state(&snapshot);
    let frame = Rect::new(0, 0, 800, 600);
    let layout = layout_for(&state, &snapshot, frame);

    // 面板与大屏同屏、8 个商品格、所有热区都能命中自己的中心。
    assert!(frame.contains(Point::new(layout.panel.x, layout.panel.y)));
    assert_eq!(layout.cells.len(), gameshop::PAGE_SIZE);
    assert_eq!(layout.panel.width, gameshop::PANEL_WIDTH);
    assert_eq!(layout.panel.height, gameshop::PANEL_HEIGHT);
    for (index, cell) in layout.cells.iter().enumerate() {
        assert_eq!(
            layout.cell_at(center(*cell)),
            Some(index),
            "第 {index} 格可命中"
        );
        assert!(
            layout.panel.contains(center(*cell)),
            "第 {index} 格在面板内"
        );
    }
    assert_eq!(layout.cell_at(Point::new(0, 0)), None, "面板外不命中商品格");
    assert!(!layout.panel.contains(Point::new(0, 0)));

    assert_eq!(layout.class_buttons.len(), ClassFilter::ALL_FILTERS.len());
    for (index, button) in layout.class_buttons.iter().enumerate() {
        assert_eq!(
            layout.class_at(center(*button)),
            Some(ClassFilter::ALL_FILTERS[index]),
            "职业按钮 {index} 命中"
        );
    }
    assert_eq!(
        layout.section_buttons.len(),
        SectionFilter::ALL_SECTIONS.len()
    );
    for (index, button) in layout.section_buttons.iter().enumerate() {
        assert_eq!(
            layout.section_at(center(*button)),
            Some(SectionFilter::ALL_SECTIONS[index]),
            "分区按钮 {index} 命中"
        );
    }

    // 分类栏：第 0 行「全部分类」，第 1 行起是 category_names 下标。
    let first_row = Point::new(layout.category_area.x + 4, layout.category_area.y + 2);
    let second_row = Point::new(
        layout.category_area.x + 4,
        layout.category_area.y + i32::try_from(gameshop::CATEGORY_ROW).unwrap_or(18) + 2,
    );
    assert_eq!(layout.category_at(first_row), Some(0));
    assert_eq!(layout.category_at(second_row), Some(1));
    assert_eq!(layout.category_at(Point::new(0, 0)), None, "分类栏外不命中");

    for rect in [
        layout.close,
        layout.search,
        layout.pay_credit,
        layout.pay_gold,
        layout.qty_minus,
        layout.qty_value,
        layout.qty_plus,
        layout.buy,
        layout.prev,
        layout.next,
    ] {
        assert!(
            layout.panel.contains(center(rect)),
            "热区 {rect:?} 在面板内"
        );
    }

    // 320×240：面板缩到最小尺寸，所有热区仍在屏幕内（可点击命中，不 panic）。
    let tiny = Rect::new(0, 0, 320, 240);
    let layout = layout_for(&state, &snapshot, tiny);
    assert!(layout.panel.width <= 320 && layout.panel.height <= 240);
    let mut rects = vec![
        layout.panel,
        layout.close,
        layout.search,
        layout.category_area,
        layout.pay_credit,
        layout.pay_gold,
        layout.qty_minus,
        layout.qty_plus,
        layout.buy,
        layout.prev,
        layout.next,
    ];
    rects.extend(layout.cells.iter().copied());
    for rect in rects {
        assert!(rect.x >= 0 && rect.y >= 0, "小屏热区越界左上：{rect:?}");
        assert!(rect.right() <= 320, "小屏热区越界右侧：{rect:?}");
        assert!(rect.bottom() <= 240, "小屏热区越界底部：{rect:?}");
    }
    assert_eq!(layout.cells.len(), gameshop::PAGE_SIZE, "小屏同样 8 格");
}

// ===== 3. 职业 / 分区 / 分类 / 搜索筛选与排序 =====

fn filter_catalog() -> WorldSnapshot {
    let mut snapshot = live_snapshot(1_000, 500);
    let mut zulu = shop_item(11, "Zulu");
    "药品".clone_into(&mut zulu.category);
    zulu.top_item = true;
    let mut alpha = shop_item(12, "Alpha");
    "武器".clone_into(&mut alpha.category);
    "Wizard".clone_into(&mut alpha.class);
    alpha.date = DateTimeBinary::from_unix_seconds(NOW_SEC - 2 * 24 * 60 * 60);
    let mut mike = shop_item(13, "Mike");
    "药品".clone_into(&mut mike.category);
    mike.deal = true;
    feed_info(&mut snapshot, &zulu, 0);
    feed_info(&mut snapshot, &alpha, 5);
    feed_info(&mut snapshot, &mike, 0);
    snapshot
}

fn names(entries: &[&GameShopEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| entry.item.info.name.clone())
        .collect()
}

#[test]
fn filters_follow_crystal_get_categories() {
    let snapshot = filter_catalog();
    let entries = &snapshot.gameshop.entries;
    let all = gameshop::visible_entries(
        entries,
        ClassFilter::All,
        SectionFilter::All,
        None,
        "",
        NOW_SEC,
    );
    assert_eq!(
        names(&all),
        vec!["Alpha", "Mike", "Zulu"],
        "默认按物品名排序"
    );

    // 职业：本职业 + 通用（`Class == "All"`）可见，其他职业商品被滤掉。
    let taoist = gameshop::visible_entries(
        entries,
        ClassFilter::Taoist,
        SectionFilter::All,
        None,
        "",
        NOW_SEC,
    );
    assert_eq!(names(&taoist), vec!["Mike", "Zulu"], "法师商品对道士不可见");
    let wizard = gameshop::visible_entries(
        entries,
        ClassFilter::Wizard,
        SectionFilter::All,
        None,
        "",
        NOW_SEC,
    );
    assert_eq!(names(&wizard), vec!["Alpha", "Mike", "Zulu"]);

    // 分区：新品 = 上架 7 天内；促销 = Deal；置顶 = TopItem。
    let new = gameshop::visible_entries(
        entries,
        ClassFilter::All,
        SectionFilter::New,
        None,
        "",
        NOW_SEC,
    );
    assert_eq!(names(&new), vec!["Alpha"], "只有 2 天前上架的商品算新品");
    let deal = gameshop::visible_entries(
        entries,
        ClassFilter::All,
        SectionFilter::Deal,
        None,
        "",
        NOW_SEC,
    );
    assert_eq!(names(&deal), vec!["Mike"]);
    let top = gameshop::visible_entries(
        entries,
        ClassFilter::All,
        SectionFilter::Top,
        None,
        "",
        NOW_SEC,
    );
    assert_eq!(names(&top), vec!["Zulu"]);

    // 分类 + 搜索（大小写不敏感，匹配显示名）。
    let category = gameshop::visible_entries(
        entries,
        ClassFilter::All,
        SectionFilter::All,
        Some("药品"),
        "",
        NOW_SEC,
    );
    assert_eq!(names(&category), vec!["Mike", "Zulu"]);
    let search = gameshop::visible_entries(
        entries,
        ClassFilter::All,
        SectionFilter::All,
        None,
        "MI",
        NOW_SEC,
    );
    assert_eq!(names(&search), vec!["Mike"], "搜索忽略大小写");
    let trimmed = gameshop::visible_entries(
        entries,
        ClassFilter::All,
        SectionFilter::All,
        None,
        "  zulu  ",
        NOW_SEC,
    );
    assert_eq!(names(&trimmed), vec!["Zulu"], "搜索去首尾空格");

    // 分类列表：按出现顺序去重（Crystal `GetCategories()`），且服从职业/分区筛选。
    assert_eq!(
        gameshop::category_names(entries, ClassFilter::All, SectionFilter::All, "", NOW_SEC),
        vec!["药品".to_owned(), "武器".to_owned()]
    );
    assert_eq!(
        gameshop::category_names(
            entries,
            ClassFilter::Taoist,
            SectionFilter::All,
            "",
            NOW_SEC
        ),
        vec!["药品".to_owned()],
        "被职业筛掉的商品不贡献分类"
    );

    // 新品标记：Crystal 用 `NewButton.Visible` 控制「新品」分区按钮。
    assert!(gameshop::has_new_items(entries, NOW_SEC));
    assert!(gameshop::is_new(
        entries
            .iter()
            .find(|entry| entry.item.g_index == 12)
            .expect("商品 12"),
        NOW_SEC
    ));
    assert!(!gameshop::is_new(
        entries
            .iter()
            .find(|entry| entry.item.g_index == 11)
            .expect("商品 11"),
        NOW_SEC
    ));
}

// ===== 4. K 热键守卫：单实例、输入焦点不触发、目录不自动弹窗、Esc 不发包 =====

#[test]
fn hotkey_toggle_guard_single_panel_and_focus() {
    let snapshot = snapshot_with_catalog(1_000, 500);
    let mut state = UiState::new(Language::En);
    state.set_time(NOW_SEC * 1000);

    // 目录到达不会自动弹窗（Crystal 也是按钮/热键触发）。
    state.sync(&snapshot);
    assert!(!state.show_gameshop, "收到商城目录不得自动开面板");

    // 输入焦点守卫：焦点在输入框时 K 不触发（app.rs 文本早退之外的第二道防线）。
    state.focus = Some(FieldId::GameShopSearch);
    assert!(
        !state.toggle_gameshop(Some(MirClass::Warrior)),
        "输入框聚焦时 K 不得触发"
    );
    assert!(!state.show_gameshop);
    state.clear_focus();

    // 打开 → 再按一次是关闭，不叠窗口（单实例）。
    assert!(state.toggle_gameshop(Some(MirClass::Warrior)));
    assert!(state.show_gameshop);
    assert!(state.toggle_gameshop(Some(MirClass::Warrior)));
    assert!(!state.show_gameshop);
    assert_eq!(state.gameshop_selected, None);

    // 每次打开都按 Crystal `Show()` 重置筛选状态。
    state.gameshop_class = ClassFilter::Taoist;
    state.gameshop_section = SectionFilter::New;
    state.gameshop_category = Some("药品".to_owned());
    state.gameshop_page = 1;
    state.gameshop_quantity = 7;
    state.gameshop_pay_type = gameshop::PAY_CREDIT;
    state.gameshop_selected = Some(11);
    state.gameshop_notice = Some(BuyProblem::NotEnoughGold);
    assert!(state.toggle_gameshop(Some(MirClass::Wizard)));
    assert!(state.show_gameshop);
    assert_eq!(
        state.gameshop_class,
        ClassFilter::from_class(Some(MirClass::Wizard)),
        "职业筛选用玩家职业"
    );
    assert_eq!(state.gameshop_section, SectionFilter::All);
    assert_eq!(state.gameshop_category, None);
    assert_eq!(state.gameshop_page, 0);
    assert_eq!(state.gameshop_quantity, 1);
    assert_eq!(state.gameshop_pay_type, gameshop::PAY_GOLD, "默认金币结算");
    assert_eq!(state.gameshop_selected, None);
    assert_eq!(state.gameshop_notice, None);
    assert!(state.gameshop_search.value.is_empty());

    // Esc 只关本地窗口、返回 Panels（不发包）；全关时返回 Nothing。
    assert!(matches!(state.escape(), EscapeResult::Panels));
    assert!(!state.show_gameshop);
    assert!(matches!(state.escape(), EscapeResult::Nothing));

    // 点关闭按钮同样只关窗口。
    let frame = Rect::new(0, 0, 800, 600);
    assert!(state.toggle_gameshop(Some(MirClass::Warrior)));
    let layout = layout_for(&state, &snapshot, frame);
    assert!(click(&mut state, &snapshot, frame, center(layout.close)).is_none());
    assert!(!state.show_gameshop);

    // 点搜索框聚焦 → 可输入/退格；聚焦时 K 被拦下且面板保持打开。
    assert!(state.toggle_gameshop(Some(MirClass::Warrior)));
    assert!(click(&mut state, &snapshot, frame, center(layout.search)).is_none());
    assert_eq!(state.focus, Some(FieldId::GameShopSearch));
    assert!(state.type_char('A'));
    assert!(state.type_char('l'));
    assert_eq!(state.gameshop_search.value, "Al");
    assert!(state.backspace());
    assert_eq!(state.gameshop_search.value, "A");
    assert!(
        !state.toggle_gameshop(Some(MirClass::Warrior)),
        "搜索框聚焦时 K 被守卫拦下"
    );
    assert!(state.show_gameshop, "被拦下时面板保持原状");
    state.clear_focus();
}

// ===== 5. 8 格分页与滚轮 =====

fn paged_catalog() -> WorldSnapshot {
    let mut snapshot = live_snapshot(10_000, 10_000);
    for index in 0..9 {
        let mut item = shop_item(11 + index, &format!("Item{index}"));
        "药品".clone_into(&mut item.category);
        feed_info(&mut snapshot, &item, 0);
    }
    snapshot
}

#[test]
fn paging_and_wheel_follow_8_cells() {
    let snapshot = paged_catalog();
    let mut state = open_state(&snapshot);
    let frame = Rect::new(0, 0, 800, 600);
    assert_eq!(gameshop::page_count(9), 2);
    assert_eq!(gameshop::page_count(0), 1, "空目录也显示 1 页");
    let layout = layout_for(&state, &snapshot, frame);
    assert_eq!(layout.cells.len(), 8, "一页固定 8 格");

    // 第 2 页只剩 1 个商品（第 9 个）。
    state.mouse = center(layout.panel);
    assert!(state.on_wheel(&snapshot, 1), "滚轮向下翻到第 2 页");
    assert_eq!(state.gameshop_page, 1);
    let filtered = gameshop::visible_entries(
        &snapshot.gameshop.entries,
        state.gameshop_class,
        state.gameshop_section,
        state.gameshop_category.as_deref(),
        &state.gameshop_search.value,
        NOW_SEC,
    );
    let page = gameshop::page_slice(&filtered, state.gameshop_page);
    assert_eq!(page.len(), 1);
    assert_eq!(
        page[0].item.info.name, "Item8",
        "第 2 页是排序后第 9 个按名商品"
    );

    // 已在最后一页：滚轮不再变化、不越界。
    assert!(!state.on_wheel(&snapshot, 1));
    assert_eq!(state.gameshop_page, 1);
    // 点第 2 页唯一格子能选中（`Item8`，GIndex 19）。
    click(&mut state, &snapshot, frame, center(layout.cells[0]));
    assert_eq!(state.gameshop_selected, Some(19));
    assert!(state.on_wheel(&snapshot, -1), "滚轮向上翻回第 1 页");
    assert_eq!(state.gameshop_page, 0);
    assert!(!state.on_wheel(&snapshot, -1), "第 1 页再向上不变化");
    assert_eq!(state.gameshop_page, 0);
}

// ===== 6. 双币购买点击流 =====

fn buy_click(state: &mut UiState, snapshot: &WorldSnapshot, frame: Rect) -> Option<UiAction> {
    let layout = layout_for(state, snapshot, frame);
    click(state, snapshot, frame, center(layout.buy))
}

#[test]
fn purchase_flow_gold_and_credit() {
    let mut snapshot = snapshot_with_catalog(1_000, 500);
    let mut state = open_state(&snapshot);
    let frame = Rect::new(0, 0, 800, 600);
    let mut layout = layout_for(&state, &snapshot, frame);

    // 点商品格选中 → 加份数 → 切元宝 → 买下 → C.GameshopBuy{GIndex, Quantity, PType}。
    click(&mut state, &snapshot, frame, center(layout.cells[0]));
    assert_eq!(state.gameshop_selected, Some(11));
    click(&mut state, &snapshot, frame, center(layout.qty_plus));
    assert_eq!(state.gameshop_quantity, 2);
    click(&mut state, &snapshot, frame, center(layout.pay_credit));
    assert_eq!(state.gameshop_pay_type, gameshop::PAY_CREDIT);
    let action = buy_click(&mut state, &snapshot, frame).expect("元宝买下应发包");
    let UiAction::GameShopBuy {
        g_index,
        quantity,
        p_type,
    } = action
    else {
        panic!("买下动作不是 GameShopBuy：{action:?}");
    };
    assert_eq!((g_index, quantity, p_type), (11, 2, 0));
    let command = action.net_command().expect("映射成 C.GameshopBuy");
    let NetCommand::GameShopBuy {
        g_index,
        quantity,
        p_type,
    } = command
    else {
        panic!("网络命令不是 GameShopBuy：{command:?}");
    };
    assert_eq!((g_index, quantity, p_type), (11, 2, 0));

    // 换商品：份数回 1；库存 5 的商品份数被夹到 5，金币结算。
    layout = layout_for(&state, &snapshot, frame);
    click(&mut state, &snapshot, frame, center(layout.cells[1]));
    assert_eq!(
        state.gameshop_selected,
        Some(12),
        "第 2 格是 Beta（GIndex 12）"
    );
    assert_eq!(state.gameshop_quantity, 1, "换商品份数回 1");
    click(&mut state, &snapshot, frame, center(layout.pay_gold));
    for _ in 0..9 {
        click(&mut state, &snapshot, frame, center(layout.qty_plus));
    }
    assert_eq!(state.gameshop_quantity, 5, "份数夹到剩余库存 5");
    let action = buy_click(&mut state, &snapshot, frame).expect("金币买下应发包");
    let UiAction::GameShopBuy {
        g_index,
        quantity,
        p_type,
    } = action
    else {
        panic!("买下动作不是 GameShopBuy：{action:?}");
    };
    assert_eq!((g_index, quantity, p_type), (12, 5, 1));

    // 服务端库存刷新到 2 后，已选份数被收边到 2。
    feed_stock(&mut snapshot, 12, 2);
    state.sync(&snapshot);
    assert_eq!(state.gameshop_quantity, 2, "库存变少后份数收边");
    let action = buy_click(&mut state, &snapshot, frame).expect("剩余库存内仍可买");
    let UiAction::GameShopBuy { quantity, .. } = action else {
        panic!("买下动作不是 GameShopBuy：{action:?}");
    };
    assert_eq!(quantity, 2);

    // 售罄：可见但不可买，本地拦截、不发包。
    feed_stock(&mut snapshot, 12, 0);
    assert!(buy_click(&mut state, &snapshot, frame).is_none());
    assert_eq!(state.gameshop_notice, Some(BuyProblem::SoldOut));
    assert!(snapshot.gameshop.entry(12).is_some(), "售罄仍保留条目可见");

    // 未选中商品直接点买下：提示，不发包。
    let mut empty_state = open_state(&snapshot);
    assert!(buy_click(&mut empty_state, &snapshot, frame).is_none());
    assert_eq!(empty_state.gameshop_notice, Some(BuyProblem::NoSelection));

    // 余额不足：金币 50 < 单价 100。
    let poor = snapshot_with_catalog(50, 500);
    let mut poor_state = open_state(&poor);
    click(&mut poor_state, &poor, frame, center(layout.cells[0]));
    assert!(buy_click(&mut poor_state, &poor, frame).is_none());
    assert_eq!(poor_state.gameshop_notice, Some(BuyProblem::NotEnoughGold));

    // 不支持金币：切到金币后本地拦截。
    let mut no_gold = snapshot_with_catalog(1_000, 500);
    no_gold.gameshop.entries[0].item.can_buy_gold = false;
    let mut no_gold_state = open_state(&no_gold);
    layout = layout_for(&no_gold_state, &no_gold, frame);
    click(&mut no_gold_state, &no_gold, frame, center(layout.cells[0]));
    assert!(buy_click(&mut no_gold_state, &no_gold, frame).is_none());
    assert_eq!(
        no_gold_state.gameshop_notice,
        Some(BuyProblem::PayTypeUnavailable)
    );

    // 断线态：面板能开，但买下被拦（断线不伪造成功）。
    let mut offline = snapshot_with_catalog(1_000, 500);
    offline.status = NetStatus::Connecting;
    let mut offline_state = open_state(&offline);
    layout = layout_for(&offline_state, &offline, frame);
    click(&mut offline_state, &offline, frame, center(layout.cells[0]));
    assert!(buy_click(&mut offline_state, &offline, frame).is_none());
    assert_eq!(
        offline_state.gameshop_notice,
        Some(BuyProblem::Disconnected)
    );
}

// ===== 7. 纯函数矩阵（Crystal BuyProduct / 份数夹取 / 库存显示 / 新品窗口） =====

fn entry_with(mut item: GameShopItem, stock_level: i32) -> GameShopEntry {
    item.info.stack_size = 100;
    GameShopEntry {
        item,
        stock_level,
        sold_out: false,
    }
}

#[test]
fn pure_helpers_match_crystal_buy_and_quantity_rules() {
    let unlimited = entry_with(shop_item(11, "Alpha"), 0);
    assert_eq!(gameshop::stock_display(&unlimited), StockDisplay::Unlimited);
    assert_eq!(
        gameshop::stock_text(&unlimited, Language::ZhCn),
        "∞",
        "不限量显示 ∞"
    );
    let mut sold_out = unlimited.clone();
    sold_out.sold_out = true;
    assert_eq!(gameshop::stock_display(&sold_out), StockDisplay::SoldOut);
    assert!(!gameshop::stock_text(&sold_out, Language::En).is_empty());
    assert_ne!(
        gameshop::stock_text(&sold_out, Language::ZhCn),
        gameshop::stock_text(&sold_out, Language::En),
        "售罄文案必须中英双语"
    );
    let mut at_least = unlimited.clone();
    at_least.stock_level = 99;
    assert_eq!(gameshop::stock_display(&at_least), StockDisplay::AtLeast99);
    assert_eq!(gameshop::stock_text(&at_least, Language::ZhCn), "99+");
    let mut exact = unlimited.clone();
    exact.stock_level = 5;
    assert_eq!(gameshop::stock_display(&exact), StockDisplay::Count(5));
    assert_eq!(gameshop::stock_text(&exact, Language::ZhCn), "5");

    // 单价 / 总价：元宝 = CreditPrice，金币 = GoldPrice。
    assert_eq!(gameshop::unit_price(&unlimited, gameshop::PAY_GOLD), 100);
    assert_eq!(gameshop::unit_price(&unlimited, gameshop::PAY_CREDIT), 40);
    assert_eq!(
        gameshop::total_price(&unlimited, 3, gameshop::PAY_GOLD),
        300
    );
    assert_eq!(
        gameshop::total_price(&unlimited, 0, gameshop::PAY_CREDIT),
        40,
        "0 份按 1 份算（防除零）"
    );

    // 份数夹取：1..=99；不超过 5 组堆叠；限量库存再夹。
    assert_eq!(gameshop::clamp_quantity(&unlimited, 0), 1);
    assert_eq!(gameshop::clamp_quantity(&unlimited, 120), 99, "上限 99");
    let mut two_per = entry_with(shop_item(21, "Stack"), 0);
    two_per.item.count = 2;
    assert_eq!(gameshop::clamp_quantity(&two_per, 99), 99);
    let mut big_stack = entry_with(shop_item(22, "Stack"), 0);
    big_stack.item.count = 10;
    big_stack.item.info.stack_size = 20;
    assert_eq!(
        gameshop::clamp_quantity(&big_stack, 99),
        10,
        "不超过 5 组堆叠"
    );
    let mut limited = entry_with(shop_item(23, "Limited"), 5);
    assert_eq!(gameshop::clamp_quantity(&limited, 99), 5, "不超过剩余库存");
    limited.stock_level = 0;
    assert_eq!(
        gameshop::clamp_quantity(&limited, 99),
        99,
        "0 = 不限量，不夹"
    );
    let limited_stock = entry_with(shop_item(23, "Limited"), 5);

    // 购买检查矩阵（Crystal BuyProduct 的分支一一对应）。
    assert_eq!(
        gameshop::check_buy(Some(&unlimited), 1, gameshop::PAY_GOLD, 1_000, 500, false),
        Err(BuyProblem::Disconnected)
    );
    assert_eq!(
        gameshop::check_buy(None, 1, gameshop::PAY_GOLD, 1_000, 500, true),
        Err(BuyProblem::NoSelection)
    );
    assert_eq!(
        gameshop::check_buy(Some(&sold_out), 1, gameshop::PAY_GOLD, 1_000, 500, true),
        Err(BuyProblem::SoldOut)
    );
    let mut no_credit = unlimited.clone();
    no_credit.item.can_buy_credit = false;
    assert_eq!(
        gameshop::check_buy(Some(&no_credit), 1, gameshop::PAY_CREDIT, 1_000, 500, true),
        Err(BuyProblem::PayTypeUnavailable)
    );
    assert_eq!(
        gameshop::check_buy(Some(&unlimited), 11, gameshop::PAY_GOLD, 1_000, 500, true),
        Err(BuyProblem::NotEnoughGold)
    );
    assert_eq!(
        gameshop::check_buy(Some(&unlimited), 1, gameshop::PAY_CREDIT, 1_000, 0, true),
        Err(BuyProblem::NotEnoughCredit)
    );
    assert_eq!(
        gameshop::check_buy(Some(&unlimited), 2, gameshop::PAY_GOLD, 1_000, 500, true),
        Ok(BuyOrder {
            g_index: 11,
            quantity: 2,
            p_type: 1
        })
    );
    let clamped = gameshop::check_buy(
        Some(&limited_stock),
        99,
        gameshop::PAY_GOLD,
        100_000,
        100_000,
        true,
    );
    assert_eq!(
        clamped,
        Ok(BuyOrder {
            g_index: 23,
            quantity: 5,
            p_type: 1
        }),
        "通过检查后份数按库存夹取再发包"
    );

    // 新品窗口：严格大于 now - 7 天（Crystal `AddDays(-7)` 后按上架时间判定）。
    let mut fresh = entry_with(shop_item(31, "Fresh"), 0);
    fresh.item.date = DateTimeBinary::from_unix_seconds(NOW_SEC - WEEK_SEC + 1);
    assert!(gameshop::is_new(&fresh, NOW_SEC));
    fresh.item.date = DateTimeBinary::from_unix_seconds(NOW_SEC - WEEK_SEC);
    assert!(!gameshop::is_new(&fresh, NOW_SEC), "正好 7 天不算新品");
    fresh.item.date = DateTimeBinary::from_unix_seconds(0);
    assert!(!gameshop::is_new(&fresh, NOW_SEC), "无上架时间不算新品");
}

// ===== 8. 与市场页签共存且状态隔离 =====

fn auction(auction_id: u64, price: u32) -> ClientAuction {
    ClientAuction {
        auction_id,
        item: UserItem {
            unique_id: auction_id + 1_000,
            item_index: 1_000,
            count: 1,
            ..UserItem::default()
        },
        seller: "张三".to_owned(),
        price,
        consignment_date: DateTimeBinary::from_unix_seconds(NOW_SEC - 3_600),
        item_type: MarketItemType::GameShop,
    }
}

fn market_snapshot() -> WorldSnapshot {
    let mut snapshot = snapshot_with_catalog(1_000, 500);
    feed_wire(
        &mut snapshot,
        &ServerPacket::NPCMarket(NPCMarket {
            listings: vec![auction(1, 100), auction(2, 200)],
            pages: 1,
            user_mode: false,
        }),
    );
    snapshot
}

#[test]
fn market_tab_state_is_isolated_from_gameshop() {
    let snapshot = market_snapshot();
    assert_eq!(
        snapshot.market.listings.len(),
        2,
        "市场数据来自 S.NPCMarket"
    );
    assert_eq!(
        snapshot.gameshop.entries.len(),
        2,
        "商城数据来自 S.GameShopInfo"
    );

    let mut state = UiState::new(Language::En);
    state.set_time(NOW_SEC * 1000);
    state.sync(&snapshot);
    // 打开市场「商城页签」并留下标记状态。
    state.show_market = true;
    state.market_tab = MarketPanelType::GameShop;
    state.market_selected = Some(1);
    state.market_search.set("武器");
    state.market_page = 0;

    let frame = Rect::new(0, 0, 800, 600);
    assert!(state.toggle_gameshop(Some(MirClass::Warrior)));
    let layout = layout_for(&state, &snapshot, frame);
    // 商城操作一圈：选商品、切职业、切分区、搜索、翻页。
    click(&mut state, &snapshot, frame, center(layout.cells[0]));
    click(
        &mut state,
        &snapshot,
        frame,
        center(layout.class_buttons[2]),
    );
    click(
        &mut state,
        &snapshot,
        frame,
        center(layout.section_buttons[3]),
    );
    state.gameshop_search.set("Al");
    state.on_wheel(&snapshot, 1);

    // 市场页签状态一字不变（选中/搜索/页签/页码）。
    assert!(state.show_market, "商城打开不得关市场");
    assert_eq!(state.market_tab, MarketPanelType::GameShop);
    assert_eq!(state.market_selected, Some(1));
    assert_eq!(state.market_search.value, "武器");
    assert_eq!(state.market_page, 0);
    // 数据源互不影响。
    assert_eq!(snapshot.market.listings.len(), 2);
    assert_eq!(snapshot.gameshop.entries.len(), 2);

    // Esc 先关最上层的商城，市场保留；再按才关市场。
    assert!(matches!(state.escape(), EscapeResult::Panels));
    assert!(!state.show_gameshop);
    assert!(state.show_market);
    assert_eq!(state.market_tab, MarketPanelType::GameShop);
    assert_eq!(state.market_search.value, "武器");
    assert!(matches!(state.escape(), EscapeResult::Panels));
    assert!(!state.show_market);

    // 关市场不影响商城数据与筛选状态；再开商城也不改市场页签状态。
    state.toggle_gameshop(Some(MirClass::Warrior));
    assert!(state.show_gameshop);
    state.close_gameshop();
    assert_eq!(snapshot.gameshop.entries.len(), 2);
    state.show_market = true;
    state.market_tab = MarketPanelType::Consign;
    state.market_selected = Some(2);
    state.toggle_gameshop(Some(MirClass::Warrior));
    assert_eq!(state.market_tab, MarketPanelType::Consign);
    assert_eq!(state.market_selected, Some(2));
}

// ===== 9. 绘制：非空 / 空目录 / 断线 / 320×240 + PNG 证据 =====

#[test]
fn panel_draws_empty_disconnected_and_png_evidence() {
    // R50-D 清单已交付（真实图标可用），缺失路径必须报 partial（不伪造图标）。
    assert!(
        !gameshop::assets_partial_default(),
        "docs/assets/gameshop-manifest.json 已交付，不应处于 partial"
    );
    assert!(
        gameshop::assets_partial(&workspace_root().join("target/round50/r50-c/no-such-assets")),
        "清单缺失时必须回退占位图标 + partial"
    );

    let snapshot = snapshot_with_catalog(1_000, 500);
    let frame_rect = Rect::new(0, 0, 800, 600);
    let mut panel_state = open_state(&snapshot);
    let mut closed_state = UiState::new(Language::En);
    closed_state.set_time(NOW_SEC * 1000);
    closed_state.sync(&snapshot);
    let closed = draw_ui(&mut closed_state, &snapshot, 800, 600);
    let painted = draw_ui(&mut panel_state, &snapshot, 800, 600);
    let layout = layout_for(&panel_state, &snapshot, frame_rect);
    let diff = changed_pixels(&closed, &painted, layout.panel);
    assert!(diff > 3_000, "商城面板应画出可见内容，实际 {diff} 像素");
    painted
        .save_png(evidence("gameshop-panel.png"))
        .expect("保存非空面板 PNG 证据");

    // 空目录：画空态提示，不 panic。
    let empty_snapshot = live_snapshot(1_000, 500);
    let mut empty_state = open_state(&empty_snapshot);
    let mut empty_closed = UiState::new(Language::En);
    empty_closed.set_time(NOW_SEC * 1000);
    empty_closed.sync(&empty_snapshot);
    let empty_base = draw_ui(&mut empty_closed, &empty_snapshot, 800, 600);
    let empty = draw_ui(&mut empty_state, &empty_snapshot, 800, 600);
    let empty_layout = layout_for(&empty_state, &empty_snapshot, frame_rect);
    assert!(
        changed_pixels(&empty_base, &empty, empty_layout.panel) > 1_000,
        "空目录也要有面板边框与空态提示"
    );
    empty
        .save_png(evidence("gameshop-panel-empty.png"))
        .expect("保存空态 PNG 证据");

    // 断线态：照常画面板（购买被拦），不 panic。
    let mut offline_snapshot = snapshot_with_catalog(1_000, 500);
    offline_snapshot.status = NetStatus::Connecting;
    let mut offline_state = open_state(&offline_snapshot);
    let offline = draw_ui(&mut offline_state, &offline_snapshot, 800, 600);
    offline
        .save_png(evidence("gameshop-panel-disconnected.png"))
        .expect("保存断线态 PNG 证据");

    // 320×240：最小尺寸下整块面板仍在屏幕内。
    let mut small_state = open_state(&snapshot);
    let small = draw_ui(&mut small_state, &snapshot, 320, 240);
    let small_layout = layout_for(&small_state, &snapshot, Rect::new(0, 0, 320, 240));
    assert!(small_layout.panel.x >= 0 && small_layout.panel.y >= 0);
    assert!(small_layout.panel.right() <= 320 && small_layout.panel.bottom() <= 240);
    small
        .save_png(evidence("gameshop-panel-320x240.png"))
        .expect("保存小屏 PNG 证据");
}

// ===== 10. K 热键接线证据（live 分支 + 文本输入早退 + 唯一调用点） =====

fn line_of(source: &str, position: usize) -> usize {
    source
        .get(..position)
        .map_or(0, |prefix| prefix.matches('\n').count() + 1)
}

fn excerpt_around(source: &str, line: usize, before: usize, after: usize) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let start = line.saturating_sub(before).max(1);
    let end = (line + after).min(lines.len());
    (start..=end)
        .map(|number| format!("{number:>5} | {}", lines[number - 1]))
        .collect::<Vec<String>>()
        .join("\n")
}

#[test]
fn k_hotkey_wiring_evidence() {
    let source = include_str!("../src/app.rs");
    let live_entry = source
        .find("fn live_toggle_gameshop")
        .expect("app.rs 应有 live_toggle_gameshop");
    let toggle_call = source
        .find("live.ui.toggle_gameshop(class)")
        .expect("helper 内应调用 UiState::toggle_gameshop");
    assert!(
        live_entry < toggle_call,
        "toggle 调用必须在 helper 内（模式为 Live 且带玩家职业）"
    );
    assert_eq!(
        source.matches(".toggle_gameshop(").count(),
        1,
        "全文件只有一个 toggle 调用点（入口收敛在 helper）"
    );

    let handle_live = source
        .find("fn handle_live_key")
        .expect("app.rs 应有 handle_live_key");
    let early_return = source
        .find("if text_handled && text_consumed_key {")
        .expect("文本输入早退块必须存在");
    let k_arm = source.find("\"k\" => {").expect("live 热键表必须有 K 分支");
    let k_helper_call = source
        .find("self.live_toggle_gameshop();")
        .expect("K 分支必须调用 live_toggle_gameshop");
    assert_eq!(
        source.matches("\"k\" => {").count(),
        1,
        "K 分支只出现一次（不与其它热键共用）"
    );
    assert!(
        handle_live < early_return,
        "文本早退必须在 live 键处理函数内部"
    );
    assert!(
        early_return < k_arm,
        "K 分支必须在文本输入早退 return 之后（焦点在输入框时不触发）"
    );
    assert!(
        k_arm < k_helper_call && k_helper_call < k_arm + 400,
        "K 分支内部直接调用 helper"
    );
    assert!(
        source.matches("self.handle_live_key(").count() >= 1,
        "handle_key 的 live 分支必须调用 handle_live_key"
    );

    let live_line = line_of(source, handle_live);
    let early_line = line_of(source, early_return);
    let k_line = line_of(source, k_arm);
    let k_call_line = line_of(source, k_helper_call);
    let toggle_line = line_of(source, toggle_call);
    let report = format!(
        "R50-C K 热键接线证据（本测试自动生成；源码 crates/mir-client/src/app.rs）\n\
HEAD: d5707c6（派发时给定；本子单不执行 git 命令，行号为本子单改动后的本地行号）\n\
\n\
1) live 键派发入口 handle_live_key : app.rs:{live_line}\n\
2) 文本输入早退块 `if text_handled && text_consumed_key {{ ...; return; }}` : app.rs:{early_line}\n\
3) K 分支 `\"k\" => {{ ... }}` : app.rs:{k_line}（{early_line} < {k_line}，早退先于 K）\n\
4) K 分支调用 `self.live_toggle_gameshop();` : app.rs:{k_call_line}\n\
5) helper 内唯一 toggle 调用 `live.ui.toggle_gameshop(class)` : app.rs:{toggle_line}\n\
6) 守卫：app.rs handle_live_key 在焦点非空时先行 return；UiState::toggle_gameshop 自身\n   `if self.focus.is_some() {{ return false; }}` 为第二道防线；K 只在 Mode::Live 分支处理\n   （登录/连接态走不到 handle_live_key）。\n\
7) 无 App 级键盘单测：tests/ 下 grep KeyEvent/handle_key 零命中（见 R50-C delegate-result）。\n\
\n\
-- app.rs 摘录（K 分支上下文） --\n{excerpt}\n",
        excerpt = excerpt_around(source, k_line, 4, 4),
    );
    std::fs::write(evidence("k-hotkey-evidence.txt"), report).expect("写 K 热键证据文件");
}
