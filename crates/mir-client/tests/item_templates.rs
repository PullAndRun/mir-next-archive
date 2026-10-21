//! 物品模板（`S.NewItemInfo`）缺失/晚到时的展示链路验收。
//!
//! 真实协议里 `S.UserInformation` / `S.NPCGoods` / `S.GainedItem` / `S.NPCMarket`
//! **只带 `item_index`**；名字/图标/价格要靠 `S.NewItemInfo` 下发模板。这个文件用
//! 纯协议包把三种时序固定成断言：
//!
//! 1. **模板准时到达**：背包/装备槽按模板取非 0 图标号，名字/价格来自模板；
//! 2. **模板没到**：不 panic，按当前降级行为显示（背包/装备槽/网格画**编号数字**，
//!    商店/拍卖退回「未知物品 #n」或编号字符串，价格 **0**，图标请求 **0 号帧**）；
//! 3. **模板晚到（包序颠倒）**：背包/装备/纸娃娃逐帧读模板 → 自动收敛；
//!    但商店货架、拍卖列表、拾取聊天行是**在包到达那一刻捕获的**，不重发就不会收敛。
//!
//! 全部走 `WorldSnapshot::apply_packet`，不连服务器、不 sleep、不依赖 `resource/`。

use mir_client::geom::Rect;
use mir_client::net::{EQUIPMENT_SLOTS, SystemChat, WorldSnapshot};
use mir_client::ui::labels::{self, Label};
use mir_client::ui::{PanelContext, UiText, equipment, inventory, shop, tooltip};
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{
    EquipmentSlot, ItemType, MarketItemType, MirClass, MirDirection, MirGender, PanelType,
};
use mir_protocol::data::{ClientAuction, ItemInfo, UserItem};
use mir_protocol::packets::server::{
    GainedItem, NPCGoods, NPCMarket, NPCResponse, NewItemInfo, ObjectNPC, UserInformation,
};
use mir_protocol::{DateTimeBinary, ServerPacket};

/// 自己的对象 ID。
const SELF_ID: u32 = 7;
/// 对话用 NPC（`S.NPCResponse` 靠最近的 NPC 定位 `object_id`）。
const NPC_ID: u32 = 90_001;
/// 测试物品的模板编号（`item_index`）。
const ITEM: i32 = 7;
/// 模板里的图标号（非 0，方便断言「请求的是模板图标而不是 0 号帧」）。
const ITEM_IMAGE: u16 = 42;
/// 模板里的价格。
const ITEM_PRICE: u32 = 1_200;

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

fn item_info(name: &str) -> ItemInfo {
    ItemInfo {
        index: ITEM,
        name: name.to_owned(),
        item_type: ItemType::Weapon,
        image: ITEM_IMAGE,
        price: ITEM_PRICE,
        stack_size: 1,
        durability: 5_000,
        ..ItemInfo::default()
    }
}

fn new_item_info(name: &str) -> ServerPacket {
    ServerPacket::NewItemInfo(NewItemInfo {
        info: item_info(name),
    })
}

fn text() -> UiText {
    UiText::fallback()
}

fn screen() -> Rect {
    Rect::screen(800, 600)
}

/// 进世界：带一件背包物品 + 一件武器槽装备，**不带模板**。
fn entered_world_without_templates() -> WorldSnapshot {
    let mut equipment = vec![None; EQUIPMENT_SLOTS];
    equipment[EquipmentSlot::Weapon as usize] = Some(user_item(21, ITEM, 1, 5_000));
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
        object_id: SELF_ID,
        real_id: SELF_ID,
        name: "模板验收".to_owned(),
        location: Point::new(330, 270),
        direction: MirDirection::Down,
        class: MirClass::Warrior,
        gender: MirGender::Male,
        level: 10,
        hp: 50,
        mp: 20,
        gold: 5_000,
        inventory: Some(vec![Some(user_item(11, ITEM, 1, 5_000)), None]),
        equipment: Some(equipment),
        ..UserInformation::default()
    }));
    snapshot
}

/// 打开商店页（`S.NPCResponse` + `S.NPCGoods`），货架上一件 `ITEM`。
fn open_shop(snapshot: &mut WorldSnapshot) {
    snapshot.apply_packet(&ServerPacket::ObjectNPC(ObjectNPC {
        object_id: NPC_ID,
        name: "模板商人".to_owned(),
        image: 5,
        location: Point::new(331, 270),
        direction: MirDirection::Left,
        ..ObjectNPC::default()
    }));
    snapshot.apply_packet(&ServerPacket::NPCResponse(NPCResponse {
        page: vec!["<买东西/@BuySell> 商店".to_owned()],
    }));
    snapshot.apply_packet(&ServerPacket::NPCGoods(NPCGoods {
        list: vec![user_item(501, ITEM, 1, 5_000)],
        rate: 1.0,
        type_: PanelType::Buy,
        hide_added_stats: true,
    }));
}

fn market_packet() -> ServerPacket {
    ServerPacket::NPCMarket(NPCMarket {
        listings: vec![ClientAuction {
            auction_id: 9,
            item: user_item(700, ITEM, 1, 0),
            seller: "卖家".to_owned(),
            price: 500,
            consignment_date: DateTimeBinary(0),
            item_type: MarketItemType::Consign,
        }],
        pages: 1,
        user_mode: false,
    })
}

/// 画背包，返回图标闭包被请求过的图标号。
fn draw_inventory(snapshot: &WorldSnapshot) -> Vec<u16> {
    let mut buffer = mir_render::Frame::new(800, 600);
    let ui_text = text();
    let context = PanelContext {
        snapshot,
        text: &ui_text,
        language: Language::ZhCn,
    };
    let panel = inventory::inventory_panel_rect(screen());
    let mut requested = Vec::new();
    inventory::draw(&mut buffer, panel, &context, Some(0), None, &mut |image| {
        requested.push(image);
        None
    });
    requested
}

/// 画装备面板，返回图标闭包被请求过的图标号。
fn draw_equipment(snapshot: &WorldSnapshot) -> Vec<u16> {
    let mut buffer = mir_render::Frame::new(800, 600);
    let ui_text = text();
    let context = PanelContext {
        snapshot,
        text: &ui_text,
        language: Language::ZhCn,
    };
    let layout = equipment::equipment_layout(screen());
    let mut requested = Vec::new();
    equipment::draw(&mut buffer, &layout, &context, None, &mut |image| {
        requested.push(image);
        None
    });
    requested
}

/// 画商店，返回图标闭包被请求过的图标号。
fn draw_shop(snapshot: &WorldSnapshot) -> Vec<u16> {
    let goods = snapshot.npc_goods.as_ref().expect("货架");
    let mut buffer = mir_render::Frame::new(800, 600);
    let ui_text = text();
    let context = PanelContext {
        snapshot,
        text: &ui_text,
        language: Language::ZhCn,
    };
    let layout = shop::shop_layout(screen(), goods.list.len(), 0, shop::SHOP_LEFT);
    let mut requested = Vec::new();
    shop::draw(
        &mut buffer,
        &layout,
        &context,
        goods,
        None,
        None,
        &mut |image| {
            requested.push(image);
            None
        },
    );
    requested
}

#[test]
fn missing_templates_degrade_to_the_index_number_and_never_panic() {
    let snapshot = entered_world_without_templates();
    assert!(snapshot.item_info(ITEM).is_none(), "模板未到");
    assert_eq!(snapshot.item_image(ITEM), None);
    assert_eq!(snapshot.item_price(ITEM), None);

    // 背包/装备：没有图标号就画编号数字，图标闭包一次都不被调用（不 panic）。
    assert!(draw_inventory(&snapshot).is_empty(), "没有模板时不请求图标");
    assert!(
        draw_equipment(&snapshot).is_empty(),
        "装备槽没有模板时也不请求图标"
    );

    // tooltip：只给「未知物品 #n」标题，没有正文行。
    let item = snapshot.inventory_item(0).expect("背包物品");
    let tip = tooltip::content_for_item(&snapshot, item, Language::ZhCn);
    assert_eq!(tip.title, "未知物品 #7");
    assert!(tip.lines.is_empty(), "缺模板时 tooltip 没有类型/属性行");
    assert_eq!(labels::unknown_item(Language::En, ITEM), "Unknown item #7");

    // 右手菜单：缺模板判断不出可装备类型，只能「使用/丢弃」两项。
    let menu = mir_client::ui::ContextMenu {
        source: mir_client::ui::DragSource::Inventory(0),
        unique_id: 11,
        item_index: ITEM,
        count: 1,
        anchor: Point::new(10, 10),
    };
    let entries = menu.entries(&snapshot, Language::ZhCn);
    assert_eq!(
        entries.len(),
        2,
        "缺模板时不出现「装备」项：{:?}",
        entries.iter().map(|entry| &entry.label).collect::<Vec<_>>()
    );
}

#[test]
fn inventory_and_equipment_converge_when_the_template_arrives_late() {
    let mut snapshot = entered_world_without_templates();
    assert!(draw_inventory(&snapshot).is_empty());

    // `S.NewItemInfo` 晚到：快照里的名字/图标/价格立刻可用。
    snapshot.apply_packet(&new_item_info("木剑"));
    assert_eq!(
        snapshot.item_info(ITEM).map(|info| info.name.as_str()),
        Some("木剑")
    );
    assert_eq!(snapshot.item_image(ITEM), Some(ITEM_IMAGE));
    assert_eq!(snapshot.item_price(ITEM), Some(ITEM_PRICE));

    // 背包槽按模板请求**非 0** 图标号；装备槽同理（逐帧读模板 → 自动收敛）。
    assert_eq!(draw_inventory(&snapshot), vec![ITEM_IMAGE]);
    assert_eq!(draw_equipment(&snapshot), vec![ITEM_IMAGE]);

    // 名字走模板（tooltip）；可装备类型已知 → 右键菜单出现「装备」。
    let item = snapshot.inventory_item(0).expect("背包物品");
    assert_eq!(
        tooltip::content_for_item(&snapshot, item, Language::ZhCn).title,
        "木剑"
    );
    let menu = mir_client::ui::ContextMenu {
        source: mir_client::ui::DragSource::Inventory(0),
        unique_id: 11,
        item_index: ITEM,
        count: 1,
        anchor: Point::new(10, 10),
    };
    assert_eq!(
        menu.entries(&snapshot, Language::ZhCn).len(),
        3,
        "使用/装备/丢弃"
    );
}

#[test]
fn shop_rows_freeze_the_template_at_npc_goods_time() {
    let mut snapshot = entered_world_without_templates();
    open_shop(&mut snapshot);
    let entry = snapshot.npc_goods.as_ref().expect("货架").list[0].clone();
    assert_eq!(entry.name, "", "S.NPCGoods 只带 UserItem，名字等模板");
    assert_eq!(
        entry.price, 0,
        "缺模板时价格是 0（见报告：货架会显示 0 金币）"
    );
    assert_eq!(entry.image, 0);
    assert_eq!(
        shop::row_text(&entry, Language::ZhCn),
        "未知物品 #7  0 金币"
    );
    assert_eq!(
        shop::row_text(&entry, Language::En),
        "Unknown item #7  0 Gold"
    );
    // 缺模板时货架请求 0 号图标帧（items.wzl 第 0 帧会被画成别的物品）。
    assert_eq!(draw_shop(&snapshot), vec![0]);

    // 模板晚到：既有货架**不会**重算（名字/价格/图标都冻结在 S.NPCGoods 时刻）。
    snapshot.apply_packet(&new_item_info("木剑"));
    let frozen = snapshot.npc_goods.as_ref().expect("货架").list[0].clone();
    assert_eq!(
        (frozen.name.as_str(), frozen.price, frozen.image),
        ("", 0, 0),
        "既有货架不随晚到的模板收敛"
    );

    // 服务端重发一次货架（或用新版本号）才收敛。
    snapshot.apply_packet(&ServerPacket::NPCGoods(NPCGoods {
        list: vec![user_item(501, ITEM, 1, 5_000)],
        rate: 1.0,
        type_: PanelType::Buy,
        hide_added_stats: true,
    }));
    let entry = snapshot.npc_goods.as_ref().expect("货架").list[0].clone();
    assert_eq!(
        (entry.name.as_str(), entry.price, entry.image),
        ("木剑", ITEM_PRICE, ITEM_IMAGE)
    );
    assert_eq!(shop::row_text(&entry, Language::ZhCn), "木剑  1,200 金币");
    assert_eq!(draw_shop(&snapshot), vec![ITEM_IMAGE]);
}

#[test]
fn market_listings_freeze_the_template_at_npc_market_time() {
    let mut snapshot = entered_world_without_templates();
    snapshot.apply_packet(&market_packet());
    let listing = &snapshot.market.listings[0];
    assert_eq!(listing.name, "7", "缺模板时拍卖行退回编号字符串");
    assert_eq!(listing.image, 0);
    assert_eq!(listing.price, 500, "价格来自拍卖包本身，不依赖模板");

    snapshot.apply_packet(&new_item_info("木剑"));
    let listing = &snapshot.market.listings[0];
    assert_eq!(listing.name, "7", "既有列表不随晚到的模板收敛");
    assert_eq!(listing.image, 0);

    snapshot.apply_packet(&market_packet());
    let listing = &snapshot.market.listings[0];
    assert_eq!((listing.name.as_str(), listing.image), ("木剑", ITEM_IMAGE));
    assert_eq!(listing.price, 500);
}

#[test]
fn gained_item_chat_keeps_the_unknown_placeholder_captured_at_pickup_time() {
    let mut snapshot = entered_world_without_templates();
    snapshot.apply_packet(&ServerPacket::GainedItem(GainedItem {
        item: user_item(11, ITEM, 1, 0),
    }));
    let captured = snapshot
        .chat
        .last()
        .and_then(|line| line.system.clone())
        .expect("拾取应生成系统聊天行");
    assert!(
        matches!(&captured, SystemChat::ItemGained { name: None, .. }),
        "缺模板时拾取聊天捕获 None，渲染时才降级"
    );
    assert!(
        labels::system_chat_text(&captured, Language::ZhCn).contains("未知物品 #7"),
        "降级文案：{}",
        labels::system_chat_text(&captured, Language::ZhCn)
    );

    // 模板晚到：**旧聊天行**已捕获 None，不会升级；新拾取才用模板名。
    snapshot.apply_packet(&new_item_info("木剑"));
    assert!(
        labels::system_chat_text(&captured, Language::ZhCn).contains("未知物品 #7"),
        "聊天行冻结在拾取时刻（见报告）"
    );
    snapshot.apply_packet(&ServerPacket::GainedItem(GainedItem {
        item: user_item(12, ITEM, 1, 0),
    }));
    let fresh = snapshot
        .chat
        .last()
        .and_then(|line| line.system.clone())
        .expect("第二次拾取");
    assert!(matches!(&fresh, SystemChat::ItemGained { name: Some(name), .. } if name == "木剑"));
    assert!(labels::system_chat_text(&fresh, Language::ZhCn).contains("木剑"));
}

#[test]
fn label_helpers_stay_stable_for_missing_templates() {
    // 这两条是 UI 文案的公共降级入口：以后改文案/占位符时先看这里。
    assert_eq!(labels::unknown_item(Language::ZhCn, 7), "未知物品 #7");
    assert_eq!(labels::unknown_item(Language::En, 7), "Unknown item #7");
    assert_eq!(Label::UnknownItem.text(Language::ZhCn), "未知物品");
    assert_eq!(Label::UnknownItem.text(Language::En), "Unknown item");
}
