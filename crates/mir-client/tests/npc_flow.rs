//! NPC 对话 / 商店的**客户端逻辑层**验收：只注入 `S.*` 包，不连真实服务端、不 sleep。
//!
//! 覆盖：
//!
//! 1. `S.NPCResponse` → 对话页保存原文行、UI 解析出 `(文本, 键)` 按钮、`@exit` 关窗；
//! 2. 点 `<View/@BuySell>` / `<Repair/@Repair>` → `UiAction::CallNpc` → `C.CallNPC`；
//! 3. `S.NPCGoods` + `S.NewItemInfo` → 货架条目（名称/价格/货币）与面板绘制；
//! 4. 点货架 → `C.BuyItem`；商店里卖选中的背包物品 → `C.SellItem`（修理同理）；
//! 5. 对话/商店用到的 `Label` 中英双语齐备。
//!
//! 服务端页面的格式见 `crates/mir-server/src/npc.rs`：正文行 + `<文本/@键>` 按钮行。
//! 用例里的 `WorldSnapshot` 完全由协议包拼出来（`apply_packet`），没有任何
//! `TcpStream`/`tokio`/`thread::sleep`，所以不依赖真实服务端也不会挂。

use mir_client::geom::Rect;
use mir_client::net::{NetCommand, WorldSnapshot};
use mir_client::ui::labels::Label;
use mir_client::ui::{self, UiAction, UiState, UiText, dialog, hud, shop};
use mir_core::Language;
use mir_core::enums::{MirClass, MirDirection, MirGender, PanelType};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ItemInfo, UserItem};
use mir_protocol::packets::server::{
    NPCGoods, NPCRepair, NPCResponse, NPCSell, NewItemInfo, ObjectNPC, UserInformation,
};

/// 测试用的 NPC 对象 ID（假注入，不与真实对象冲突）。
const NPC_ID: u32 = 90_001;
/// 自己站在比奇广场，NPC 在右边一格。
const SELF: (i32, i32) = (330, 270);
const NPC_CELL: (i32, i32) = (331, 270);
/// 货架两件商品的唯一编号。
const POTION_ID: u64 = 501;
const SWORD_ID: u64 = 502;

/// 服务端发来的一页（对照 `mir-server/src/npc.rs` 的页面格式）。
fn server_page() -> Vec<String> {
    [
        "Welcome. Thanks for dropping in.",
        "I see you're wearing: 无",
        "<View/@BuySell> Store.",
        "<Repair/@Repair> Weapon.",
        "",
        "<Close/@exit>",
    ]
    .iter()
    .map(|line| (*line).to_owned())
    .collect()
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

/// 一套「进世界 → 站在 NPC 旁边 → 打开对话页 → 收到货架」的假数据。
///
/// 全部走 `apply_packet`，等价于真实服务端依次下发这些包，但没有任何网络。
fn snapshot_with_npc() -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
        object_id: 7,
        real_id: 7,
        name: "验收者".to_owned(),
        location: mir_core::Point::new(SELF.0, SELF.1),
        direction: MirDirection::Down,
        class: MirClass::Warrior,
        gender: MirGender::Male,
        level: 12,
        hp: 60,
        mp: 20,
        gold: 4_321,
        inventory: Some(vec![
            Some(user_item(11, 1, 1, 5_000)),
            Some(user_item(12, 2, 17, 0)),
            None,
        ]),
        ..UserInformation::default()
    }));
    snapshot.apply_packet(&ServerPacket::ObjectNPC(ObjectNPC {
        object_id: NPC_ID,
        name: "测试商人".to_owned(),
        image: 5,
        location: mir_core::Point::new(NPC_CELL.0, NPC_CELL.1),
        direction: MirDirection::Left,
        ..ObjectNPC::default()
    }));
    snapshot.apply_packet(&ServerPacket::NPCResponse(NPCResponse {
        page: server_page(),
    }));
    snapshot
}

/// 在 `snapshot_with_npc` 之上再发一页货架（含两个物品模板）。
fn snapshot_with_shop() -> WorldSnapshot {
    let mut snapshot = snapshot_with_npc();
    snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
        info: item_info(1, "金创药", 2, 1_200, 100),
    }));
    snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
        info: item_info(2, "木剑", 1, 50, 1),
    }));
    snapshot.apply_packet(&ServerPacket::NPCGoods(NPCGoods {
        list: vec![
            user_item(POTION_ID, 1, 5, 0),
            user_item(SWORD_ID, 2, 1, 5_000),
        ],
        rate: 0.9,
        type_: PanelType::Buy,
        hide_added_stats: true,
    }));
    // 商店页的「可出售 / 可修理」提示来自独立的两条包。
    snapshot.apply_packet(&ServerPacket::NPCSell(NPCSell {}));
    snapshot.apply_packet(&ServerPacket::NPCRepair(NPCRepair::default()));
    snapshot
}

fn text() -> UiText {
    UiText::fallback()
}

fn frame() -> Rect {
    Rect::screen(800, 600)
}

/// 点对话页上某个页键对应的按钮。
fn click_dialog_button(state: &mut UiState, snapshot: &WorldSnapshot, key: &str) -> UiAction {
    let page = state.dialog.as_ref().expect("对话页应已打开");
    let layout = dialog::dialog_layout(frame(), page, |label| {
        text().text_width(label, hud::SIZE_BODY)
    });
    let index = page
        .buttons
        .iter()
        .position(|button| button.key.eq_ignore_ascii_case(key))
        .unwrap_or_else(|| panic!("页面上应有 {key} 按钮：{:?}", page.buttons));
    state.mouse = layout.buttons[index].center();
    state
        .on_click(snapshot, &text(), frame(), None)
        .unwrap_or_else(|| panic!("点 {key} 按钮应产生动作"))
}

#[test]
fn npc_response_shows_the_original_lines_and_parses_buttons() {
    let snapshot = snapshot_with_npc();
    let raw = snapshot
        .npc_dialog
        .as_ref()
        .expect("S.NPCResponse 应生成对话页");
    assert_eq!(raw.lines, server_page(), "快照必须保存服务端原文行");
    assert_eq!(
        raw.object_id,
        Some(NPC_ID),
        "S.NPCResponse 不带对象 ID，应按最近的 NPC 定位"
    );
    assert_eq!(raw.version, 1);

    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    let page = state.dialog.as_ref().expect("UI 应打开对话页");
    assert_eq!(
        page.lines,
        vec![
            "Welcome. Thanks for dropping in.",
            "I see you're wearing: 无",
            // 按钮标记**就地**摘掉，标记后的空格保留（Crystal 的 `R.Replace` 也是就地替换）。
            " Store.",
            " Weapon.",
            // 页面里的空行不画（`ParsedPage::parse` 过滤空正文）。
        ],
        "按钮标记应从正文里摘掉，原文其余部分逐字保留"
    );
    let buttons: Vec<(&str, &str)> = page
        .buttons
        .iter()
        .map(|button| (button.label.as_str(), button.key.as_str()))
        .collect();
    assert_eq!(
        buttons,
        vec![
            ("View", "@BuySell"),
            ("Repair", "@Repair"),
            ("Close", "@exit"),
        ],
        "按钮应解析成 (文本, 键)，按出现顺序去重"
    );
    assert_eq!(state.npc_object_id, Some(NPC_ID));
}

#[test]
fn dialog_buttons_dispatch_their_script_keys_as_call_npc() {
    let snapshot = snapshot_with_npc();
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);

    for key in ["@BuySell", "@Repair"] {
        let action = click_dialog_button(&mut state, &snapshot, key);
        assert_eq!(
            action,
            UiAction::CallNpc {
                object_id: NPC_ID,
                key: key.to_owned()
            },
            "点按钮应带 NPC 对象 ID 与页键"
        );
        assert_eq!(
            action.net_command(),
            Some(NetCommand::CallNpc {
                object_id: NPC_ID,
                key: key.to_owned()
            }),
            "应映射成 C.CallNPC"
        );
        assert!(!state.close_npc_for_key(key), "{key} 不应关窗");
        assert!(state.dialog.is_some(), "普通页键不应关掉对话页");
    }
}

#[test]
fn the_exit_button_closes_the_dialog_and_the_shop() {
    let snapshot = snapshot_with_shop();
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    assert!(state.dialog.is_some());
    assert!(state.shop_open, "S.NPCGoods 应打开商店面板");

    let action = click_dialog_button(&mut state, &snapshot, "@exit");
    assert_eq!(
        action,
        UiAction::CallNpc {
            object_id: NPC_ID,
            key: "@exit".to_owned()
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::CallNpc {
            object_id: NPC_ID,
            key: "@exit".to_owned()
        }),
        "关闭按钮也要给服务端发 C.CallNPC（@exit）"
    );

    // 窗口层派发 `UiAction::CallNpc` 时做的本地副作用（`app.rs` 的 `close_npc_for_key`）。
    let UiAction::CallNpc { key, .. } = &action else {
        unreachable!("上面已断言是 CallNpc");
    };
    assert!(state.close_npc_for_key(key), "@exit 应立即关窗");
    assert!(state.dialog.is_none(), "对话页应关掉");
    assert!(!state.shop_open, "商店面板应一起关掉");
    assert_eq!(state.npc_object_id, None);
    assert_eq!(state.selected_goods, None);
}

#[test]
fn dialog_without_an_exit_button_still_gets_a_localized_close() {
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::NPCResponse(NPCResponse {
        page: vec!["只有正文，没有按钮".to_owned()],
    }));
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    let page = state.dialog.as_ref().expect("对话页");
    assert!(
        page.buttons
            .iter()
            .any(|button| button.key == dialog::EXIT_KEY),
        "没有 @exit 按钮时应自动补一个关闭按钮"
    );
    assert_eq!(
        dialog::close_button_label(Language::ZhCn),
        "关闭",
        "自动补的关闭按钮文案走 Label，中英各有值"
    );
    assert_eq!(dialog::close_button_label(Language::En), "Close");
    assert!(state.close_npc_for_key(dialog::EXIT_KEY));
    assert!(state.dialog.is_none());
}

#[test]
fn npc_goods_packet_fills_the_shelf_with_names_prices_and_currency() {
    let snapshot = snapshot_with_shop();
    let goods = snapshot.npc_goods.as_ref().expect("S.NPCGoods 应生成货架");
    assert_eq!(goods.list.len(), 2);
    assert!((goods.rate - 0.9).abs() < f32::EPSILON);

    let potion = &goods.list[0];
    assert_eq!(potion.unique_id, POTION_ID, "购买用商店物品唯一编号");
    assert_eq!(potion.name, "金创药", "名称来自 S.NewItemInfo 模板");
    assert_eq!(potion.price, 1_200, "价格来自模板");
    assert_eq!(potion.count, 5);
    assert_eq!(potion.image, 2);
    let sword = &goods.list[1];
    assert_eq!(
        (sword.unique_id, sword.name.as_str(), sword.price),
        (SWORD_ID, "木剑", 50)
    );

    // 货币类型：货架是金币（元宝只在商城页签）。
    assert_eq!(shop::price_text(1_200, Language::ZhCn), "1,200 金币");
    assert_eq!(shop::price_text(1_200, Language::En), "1,200 Gold");
    assert_eq!(Label::Gold.text(Language::ZhCn), "金币");
    assert_eq!(Label::Gold.text(Language::En), "Gold");
    assert_eq!(
        shop::row_text(potion, Language::ZhCn),
        "金创药 ×5  1,200 金币"
    );
    assert_eq!(shop::row_text(sword, Language::En), "木剑  50 Gold");
    assert_eq!(shop::rate_text(goods.rate, Language::ZhCn), "倍率 90%");

    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    assert!(state.shop_open, "货架非空应打开商店面板");
    assert!(snapshot.npc_panels.sell, "S.NPCSell 应记录「可出售」");
    assert!(snapshot.npc_panels.repair, "S.NPCRepair 应记录「可修理」");

    // 面板绘制：货架行落位、可命中、选中行有高亮底色（不需要窗口/素材）。
    let layout = shop::shop_layout(frame(), goods.list.len(), 0, shop::SHOP_LEFT);
    assert_eq!(layout.rows.len(), 2);
    for (index, row) in layout.rows.iter().enumerate() {
        assert!(layout.panel.contains(row.center()));
        assert_eq!(layout.row_at(row.center()), Some(index));
    }
    let mut buffer = mir_render::Frame::new(800, 600);
    let ui_text = text();
    let context = ui::PanelContext {
        snapshot: &snapshot,
        text: &ui_text,
        language: Language::ZhCn,
    };
    shop::draw(
        &mut buffer,
        &layout,
        &context,
        goods,
        Some(0),
        None,
        &mut |_image: u16| None,
    );
    let non_transparent = count_pixels(&buffer, layout.panel, |pixel| pixel[3] != 0);
    assert!(
        non_transparent > 1_000,
        "商店面板应真的画出来，实际 {non_transparent} 像素"
    );
    let highlight = count_pixels(&buffer, layout.rows[0], |pixel| pixel == [86, 74, 32, 255]);
    assert!(highlight > 100, "选中行应有高亮底色，实际 {highlight} 像素");
}

/// 统计矩形里满足条件的像素数（越界自动裁剪）。
fn count_pixels(
    frame: &mir_render::Frame,
    rect: Rect,
    predicate: impl Fn([u8; 4]) -> bool,
) -> usize {
    let mut count = 0;
    for y in rect.y.max(0)
        ..rect
            .bottom()
            .min(i32::try_from(frame.height).unwrap_or(i32::MAX))
    {
        for x in rect.x.max(0)
            ..rect
                .right()
                .min(i32::try_from(frame.width).unwrap_or(i32::MAX))
        {
            let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
                continue;
            };
            if frame.pixel(x, y).is_some_and(&predicate) {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn clicking_a_shelf_row_buys_one_item_with_c_buy_item() {
    let snapshot = snapshot_with_shop();
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    let goods = snapshot.npc_goods.as_ref().expect("货架");
    let layout = shop::shop_layout(frame(), goods.list.len(), 0, shop::SHOP_LEFT);

    state.mouse = layout.rows[1].center();
    let action = state
        .on_click(&snapshot, &text(), frame(), None)
        .expect("点货架应产生购买动作");
    assert_eq!(
        action,
        UiAction::BuyItem {
            unique_id: SWORD_ID
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::BuyItem {
            unique_id: SWORD_ID,
            count: 1
        }),
        "购买应映射成 C.BuyItem（数量 1）"
    );
    assert_eq!(state.selected_goods, Some(1));
}

#[test]
fn selling_the_selected_inventory_item_sends_c_sell_item() {
    let snapshot = snapshot_with_shop();
    let mut state = UiState::new(Language::ZhCn);
    state.show_inventory = true;
    state.sync(&snapshot);
    assert!(state.shop_open);

    // 没选中物品时没有动作。
    assert_eq!(state.sell_selected_item(&snapshot), None);
    assert_eq!(state.repair_selected_item(&snapshot), None);

    // 点背包第 0 格只选中，不发包。
    let panel = ui::inventory::inventory_panel_rect(frame());
    let slot = ui::inventory::inventory_slot_rect(panel, 0).expect("0 号格");
    state.mouse = slot.center();
    assert!(state.on_click(&snapshot, &text(), frame(), None).is_none());
    assert_eq!(state.selected_slot, Some(0));

    let sell = state
        .sell_selected_item(&snapshot)
        .expect("商店里选中物品后应能卖");
    assert_eq!(
        sell,
        UiAction::SellItem {
            unique_id: 11,
            count: 1
        }
    );
    assert_eq!(
        sell.net_command(),
        Some(NetCommand::SellItem {
            unique_id: 11,
            count: 1
        }),
        "卖出应映射成 C.SellItem"
    );

    let repair = state
        .repair_selected_item(&snapshot)
        .expect("商店里选中物品后应能修");
    assert_eq!(repair, UiAction::RepairItem { unique_id: 11 });
    assert_eq!(
        repair.net_command(),
        Some(NetCommand::RepairItem { unique_id: 11 }),
        "修理应映射成 C.RepairItem"
    );

    // 商店关掉后不再产生卖/修动作（`S` / `R` 只在商店页生效）。
    let mut closed = snapshot.clone();
    closed.npc_goods = None;
    state.sync(&closed);
    assert!(!state.shop_open);
    assert_eq!(state.sell_selected_item(&closed), None);
    assert_eq!(state.repair_selected_item(&closed), None);
}

#[test]
fn npc_dialog_and_shop_labels_are_bilingual() {
    // 对话框标题/提示、关闭按钮、商店面板与货币都走 `Label`：中英都非空、英文不含 CJK。
    for label in [
        Label::Dialog,
        Label::NpcHint,
        Label::Close,
        Label::Shop,
        Label::ShopHint,
        Label::Gold,
        Label::SellAllowed,
        Label::RepairAllowed,
        Label::UnknownMap,
    ] {
        let zh = label.text(Language::ZhCn);
        let en = label.text(Language::En);
        assert!(!zh.is_empty(), "{label:?} 缺中文");
        assert!(!en.is_empty(), "{label:?} 缺英文");
        assert!(
            !en.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "{label:?} 英文里混进中文：{en}"
        );
    }
    // `Label::ALL` 的数量断言在 `ui::labels` 的 `every_label_has_both_languages`；
    // 本任务没有新增 Label，所以那里无需同步。
    assert!(Label::ALL.len() >= 300, "Label::ALL 应覆盖全部变体");
}
