//! 交易面板的无头测试（不连服务器、不开窗口）。
//!
//! 流程与 `tests/panels.rs` 一致：用 `WorldSnapshot::apply_packet` 注入假包
//! （`S.TradeRequest` / `S.TradeAccept` / `S.TradeItem` / `S.TradeGold`），渲染整屏到
//! `target/client-trade.png`，再用布局函数算出的矩形点按钮，断言
//! [`UiAction`] 与 [`NetCommand`] 正确。
//!
//! 断言覆盖：
//!
//! 1. 邀请弹窗真的画出来、点「接受」发 `C.TradeReply{AcceptInvite=true}`；
//! 2. 交易窗两栏各 10 格都有内容（自己/对方两栏像素都变化）；
//! 3. 「准备」→ `NetCommand::TradeConfirm{locked:true}`，按钮变成「确认」；
//! 4. 背包物品拖进交易格 → `NetCommand::TradeItem{from,to}`（= `C.DepositTradeItem`）；
//! 5. 金币输入 + 「放入金币」→ `NetCommand::TradeGold{amount}`；
//! 6. 「取消交易」→ `NetCommand::TradeCancel` 且面板收起；
//! 7. 对方改报价（`S.TradeItem`）会把自己的准备状态清掉（服务端 `TradeUnlock()` 的镜像）。

mod common;

use std::path::{Path, PathBuf};

use mir_client::geom::Rect;
use mir_client::net::{NetCommand, WorldSnapshot};
use mir_client::ui::{self, FieldId, UiAction, UiState, UiText, inventory, trade};
use mir_core::enums::{ChatType, MirClass, MirDirection, MirGender};
use mir_core::{Language, Point};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ItemInfo, UserItem};
use mir_protocol::packets::server::{
    NewItemInfo, TradeAccept, TradeGold, TradeItem as TradeItemPacket, TradeRequest,
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

/// 无 `resource/` 时的合成地图（与其它无头测试相同）。
fn synthetic_map() -> Map {
    let width = 64;
    let height = 64;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "synthetic-trade".to_owned(),
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

/// 基础世界：自己 + 两件背包物品 + 物品模板。
fn base_snapshot() -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    let mut slots = vec![None; inventory::INVENTORY_SLOTS];
    slots[0] = Some(user_item(11, 1, 1, 5_000));
    slots[1] = Some(user_item(12, 2, 7, 0));
    snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
        object_id: 7,
        real_id: 7,
        name: "面板测试".to_owned(),
        location: Point::new(32, 32),
        direction: MirDirection::Down,
        class: MirClass::Warrior,
        gender: MirGender::Male,
        level: 12,
        hair: 1,
        hp: 50,
        mp: 30,
        gold: 4_321,
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

/// 打开交易窗之后的快照（对方放上一件物品与金币）。
fn trade_snapshot() -> WorldSnapshot {
    let mut snapshot = base_snapshot();
    snapshot.apply_packet(&ServerPacket::TradeRequest(TradeRequest {
        name: "张三".to_owned(),
    }));
    snapshot.apply_packet(&ServerPacket::TradeAccept(TradeAccept {
        name: "张三".to_owned(),
    }));
    // 对方放上一件「金创药」。
    let mut partner_items = vec![None; mir_client::net::TRADE_SLOTS];
    partner_items[4] = Some(user_item(31, 2, 3, 0));
    snapshot.apply_packet(&ServerPacket::TradeItem(TradeItemPacket {
        trade_items: partner_items,
    }));
    snapshot.apply_packet(&ServerPacket::TradeGold(TradeGold { amount: 800 }));
    // 自己把 0 号格的「木剑」放进 2 号交易格（服务端回执）。
    snapshot.apply_packet(&ServerPacket::DepositTradeItem(
        mir_protocol::packets::server::DepositTradeItem {
            from: 0,
            to: 2,
            success: true,
        },
    ));
    // 「对方已准备」标记：协议没有这个包，注入式测试直接写（见 `TradeView::partner_ready`）。
    snapshot.trade.partner_ready = true;
    snapshot
}

#[test]
fn trade_panel_accepts_items_gold_and_confirm() {
    let mut snapshot = trade_snapshot();
    assert!(snapshot.trade.open, "S.TradeAccept 应打开交易窗");
    assert_eq!(snapshot.trade.partner, "张三");
    assert_eq!(snapshot.trade.partner_gold, 800);
    assert_eq!(
        snapshot.trade.partner_items[4]
            .as_ref()
            .map(|item| item.count),
        Some(3)
    );
    assert_eq!(
        snapshot.trade.items[2].as_ref().map(|item| item.unique_id),
        Some(11),
        "S.DepositTradeItem 应把背包物品搬进交易格"
    );
    assert!(snapshot.inventory_item(0).is_none(), "背包 0 号格应已空");

    let text = common::load_text();
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    assert!(state.show_trade, "S.TradeAccept 应自动打开交易面板");
    assert!(
        state.show_inventory,
        "交易窗打开时背包也要一起显示（拖拽要用）"
    );

    let screen = Rect::screen(800, 600);
    let layout = trade::trade_layout(screen);
    let (world_only, frame) = render(&mut state, &snapshot, &text);
    let changed = changed_pixels(&world_only, &frame, layout.panel);
    assert!(changed > 1_000, "交易面板应覆盖地图像素，实际 {changed}");

    // 两栏都要画出来（各 10 格）。
    let own = changed_pixels(&world_only, &frame, layout.own_grid.area);
    let partner = changed_pixels(&world_only, &frame, layout.partner_grid.area);
    assert!(own > 300, "自己那一栏应有内容，实际 {own}");
    assert!(partner > 300, "对方那一栏应有内容，实际 {partner}");
    assert_eq!(layout.own_grid.slots.len(), 10);
    assert_eq!(layout.partner_grid.slots.len(), 10);

    // 「对方：已准备」与「未准备」两种状态的像素不同（注入字段直接验证）。
    // 这组文案只有中文：字体没有 CJK 字形时两者都画不出来，像素差恒为 0，
    // 此时跳过断言并打印原因（正常 Noto CJK / 微软雅黑 下保持严格断言）。
    let mut not_ready = snapshot.clone();
    not_ready.trade.partner_ready = false;
    let (_, not_ready_frame) = render(&mut state, &not_ready, &text);
    if !common::skip_if_no_cjk(&text, "对方已准备/未准备") {
        assert!(
            changed_pixels(&not_ready_frame, &frame, layout.partner_state) > 5,
            "对方状态行应随 partner_ready 变化"
        );
    }

    let output = workspace_root().join("target/client-trade.png");
    frame
        .save_png(&output)
        .expect("写出 target/client-trade.png");
    assert!(std::fs::metadata(&output).is_ok_and(|meta| meta.len() > 4_096));

    // 交易邀请弹窗：把邀请重新注入一次，断言弹窗被画出来，点「接受」回 `C.TradeReply`。
    let mut invited = base_snapshot();
    invited.apply_packet(&ServerPacket::TradeRequest(TradeRequest {
        name: "李四".to_owned(),
    }));
    let mut invite_state = UiState::new(Language::ZhCn);
    invite_state.sync(&invited);
    assert_eq!(invite_state.trade_invite.as_deref(), Some("李四"));
    let (world_only, invite_frame) = render(&mut invite_state, &invited, &text);
    let notice = ui::notice::notice_layout(screen);
    assert!(
        changed_pixels(&world_only, &invite_frame, notice.panel) > 1_000,
        "交易邀请弹窗应画在屏幕中央"
    );
    invite_state.mouse = notice.accept.center();
    let action = invite_state
        .on_click(&invited, &text, screen, None)
        .expect("点「接受」应产生动作");
    assert_eq!(action, UiAction::TradeReply { accept: true });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::TradeReply { accept: true })
    );
    assert!(invite_state.trade_invite.is_none(), "回应后弹窗应关闭");

    // 「准备」→ `C.TradeConfirm{locked:true}`。
    state.mouse = layout.ready.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("准备按钮应产生动作");
    assert_eq!(action, UiAction::TradeConfirm { locked: true });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::TradeConfirm { locked: true })
    );
    assert!(state.trade_locked, "点过准备后本地状态应已锁定");
    let (_, locked_frame) = render(&mut state, &snapshot, &text);
    assert!(
        changed_pixels(&frame, &locked_frame, layout.ready) > 5,
        "准备后按钮应变成「确认」"
    );

    // 背包 1 号格 → 交易 3 号格：`C.DepositTradeItem { from: 1, to: 3 }`。
    let inventory_panel = inventory::inventory_panel_rect(screen);
    state.mouse = inventory::inventory_slot_rect(inventory_panel, 1)
        .expect("背包 1 号格")
        .center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert!(state.drag.is_some(), "按下背包格应开始拖拽");
    state.mouse = layout.own_grid.slots[3].center();
    let action = state
        .on_mouse_release(&snapshot, screen)
        .expect("松手在交易格应产生放入动作");
    assert_eq!(action, UiAction::TradeItem { from: 1, to: 3 });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::TradeItem { from: 1, to: 3 }),
        "TradeItem 应映射到 C.DepositTradeItem"
    );

    // 交易格的物品拖回背包：`C.RetrieveTradeItem`。
    state.mouse = layout.own_grid.slots[2].center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert!(state.drag.is_some(), "按下交易格应开始拖拽");
    state.mouse = inventory::inventory_slot_rect(inventory_panel, 5)
        .expect("背包 5 号格")
        .center();
    let action = state
        .on_mouse_release(&snapshot, screen)
        .expect("松手在背包格应产生取回动作");
    assert_eq!(action, UiAction::TradeTakeBack { from: 2, to: 5 });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::TradeTakeBack { from: 2, to: 5 })
    );

    // 金币：点输入框聚焦、输入 500、点「放入金币」→ `C.TradeGold{500}`。
    state.mouse = layout.gold_field.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.focus, Some(FieldId::TradeGold));
    for digit in ['5', '0', '0'] {
        assert!(state.type_char(digit));
    }
    assert_eq!(state.trade_gold.number(), 500);
    state.mouse = layout.gold_add.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("放入金币应产生动作");
    assert_eq!(action, UiAction::TradeGold { amount: 500 });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::TradeGold { amount: 500 })
    );
    assert!(state.trade_gold.is_empty(), "放入后输入框应清空");

    // 对方改报价（`S.TradeItem`）会把双方解锁 → 本地准备状态也要清掉。
    snapshot.apply_packet(&ServerPacket::TradeItem(TradeItemPacket {
        trade_items: vec![None; mir_client::net::TRADE_SLOTS],
    }));
    state.sync(&snapshot);
    assert!(!state.trade_locked, "对方改报价后应回到「未准备」");
    assert!(!snapshot.trade.partner_ready);

    // 「取消交易」→ `C.TradeCancel`，面板收起。
    let (_, frame) = render(&mut state, &snapshot, &text);
    let layout = trade::trade_layout(screen);
    let _ = frame;
    state.mouse = layout.cancel.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("取消按钮应产生动作");
    assert_eq!(action, UiAction::TradeCancel);
    assert_eq!(action.net_command(), Some(NetCommand::TradeCancel));
    assert!(!state.show_trade, "取消后面板应关闭");

    // 交易被取消后快照里会留一条系统提示（Crystal 弹「交易已取消」提示框，
    // `GameScene.cs:6341-6345`，本作改成聊天行）。
    let mut cancelled = trade_snapshot();
    cancelled.apply_packet(&ServerPacket::TradeCancel(
        mir_protocol::packets::server::TradeCancel { unlock: false },
    ));
    assert!(
        !cancelled.trade.open,
        "S.TradeCancel{{Unlock=false}} 应关窗"
    );
    assert_eq!(
        cancelled.chat.last().map(|line| line.kind),
        Some(ChatType::System)
    );
}

/// 编译期探针：`ui::trade` / `ui::notice` 的公开布局函数在新窗口尺寸下仍可用。
#[test]
fn trade_layouts_survive_small_windows() {
    let small = Rect::screen(320, 240);
    assert!(trade::trade_layout(small).panel.width <= 320);
    assert!(ui::notice::notice_layout(small).panel.width <= 320);
}
