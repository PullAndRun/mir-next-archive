//! 任务 / 仓库 / 邮件 / 行会四个面板的无头测试（不连服务器、不开窗口）。
//!
//! 每个面板一个测试，流程一致：
//!
//! 1. 用 `WorldSnapshot::apply_packet` 注入假包（`S.UserInformation`、`S.NewItemInfo`、
//!    `S.NewQuestInfo`/`S.ChangeQuest`、`S.UserStorage`/`S.NPCStorage`、`S.ReceiveMail`、
//!    `S.GuildStatus`/`S.GuildMemberChange`/`S.GuildStorageList`/`S.GuildBuffList`…）；
//! 2. 渲染整屏到 `target/client-<面板>.png`，断言面板区域相对「只有世界」那一帧有足够多的
//!    像素变化（面板真的画出来了）；
//! 3. 用布局函数算出的矩形点按钮/格子，断言产生的 [`UiAction`] 与
//!    [`mir_client::net::NetCommand`] 正确（放弃任务、拖拽存取、收取附件、激活增益…）。
//!
//! 不依赖真实素材：没有地图时用合成空地图，没有字体时 `UiText::load` 自动退回点阵。

mod common;

use std::path::{Path, PathBuf};

use mir_client::geom::Rect;
use mir_client::net::{NetCommand, WorldSnapshot};
use mir_client::ui::{
    self, UiAction, UiState, UiText, guild, inventory, mail, quests, shop, storage,
};
use mir_core::enums::{
    GuildRankOptions, MirClass, MirDirection, MirGender, QuestState, QuestType, Stat,
};
use mir_core::{DateTimeBinary, Language, Point};
use mir_protocol::ServerPacket;
use mir_protocol::data::guild_rank::{GuildMember, GuildRank};
use mir_protocol::data::{
    ClientMail, ClientQuestInfo, ClientQuestProgress, GuildBuffInfo, GuildStorageItem, ItemInfo,
    UserItem,
};
use mir_protocol::packets::server::{
    ChangeQuest, GuildBuffList, GuildMemberChange, GuildNoticeChange, GuildStatus,
    GuildStorageList, NPCStorage, NewItemInfo, NewQuestInfo, ReceiveMail, UserInformation,
    UserStorage,
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
        file_name: "synthetic-panels".to_owned(),
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

/// 统计矩形里「偏绿」的像素数（完成任务的提示色 `quests::COMPLETE_COLOR`）。
///
/// 有 TrueType 字体时文字是抗锯齿的，精确颜色只有核心几个像素；这里放宽成
/// 「绿通道明显高于红/蓝」。
fn greenish_pixels(frame: &Frame, rect: Rect) -> usize {
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
            if let Some([r, g, b, _]) = frame.pixel(x, y) {
                if g >= 130 && g > r.saturating_add(40) && g > b.saturating_add(30) {
                    count += 1;
                }
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
    let mut inventory = vec![None; inventory::INVENTORY_SLOTS];
    inventory[0] = Some(user_item(11, 1, 1, 5_000));
    inventory[1] = Some(user_item(12, 2, 17, 0));
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
        inventory: Some(inventory),
        ..UserInformation::default()
    }));
    snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
        info: item_info(1, "木剑", 1, 1_200, 1),
    }));
    snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
        info: item_info(2, "金创药", 2, 50, 100),
    }));
    snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
        info: item_info(3, "鹿肉", 3, 20, 20),
    }));
    snapshot
}

fn quest_info(index: i32, name: &str, group: &str, limit: i32) -> ClientQuestInfo {
    ClientQuestInfo {
        index,
        npc_index: 1,
        name: name.to_owned(),
        group: group.to_owned(),
        description: vec!["去村口看看。".to_owned()],
        task_description: vec!["击杀 鹿".to_owned()],
        return_description: vec!["回去复命".to_owned()],
        completion_description: vec!["干得漂亮".to_owned()],
        min_level_needed: 1,
        max_level_needed: 30,
        quest_type: QuestType::General,
        time_limit_in_seconds: limit,
        reward_gold: 500,
        reward_exp: 1_000,
        finish_npc_index: 2,
        ..ClientQuestInfo::default()
    }
}

#[test]
fn quest_panel_renders_progress_and_abandons_the_selected_quest() {
    let mut snapshot = base_snapshot();
    snapshot.apply_packet(&ServerPacket::NewQuestInfo(NewQuestInfo {
        info: quest_info(1, "猎鹿", "新手村", 3_600),
    }));
    snapshot.apply_packet(&ServerPacket::NewQuestInfo(NewQuestInfo {
        info: quest_info(2, "送信", "新手村", 0),
    }));
    // 接取时刻用固定时钟，方便断言「限时剩余」。
    snapshot.apply_packet_at(
        &ServerPacket::ChangeQuest(ChangeQuest {
            quest: ClientQuestProgress {
                id: 1,
                task_list: vec!["击杀 鹿 3/10".to_owned()],
                taken: true,
                completed: false,
                new: true,
            },
            quest_state: QuestState::Add,
            track_quest: true,
        }),
        1_000,
    );
    snapshot.apply_packet_at(
        &ServerPacket::ChangeQuest(ChangeQuest {
            quest: ClientQuestProgress {
                id: 2,
                task_list: vec!["收集 信件 1/1（已完成）".to_owned()],
                taken: true,
                completed: true,
                new: false,
            },
            quest_state: QuestState::Add,
            track_quest: false,
        }),
        1_000,
    );
    assert_eq!(snapshot.quests.len(), 2);
    assert!(
        snapshot
            .quest(2)
            .is_some_and(mir_client::net::QuestView::ready_to_return)
    );
    assert_eq!(
        snapshot
            .quest(1)
            .and_then(|quest| quest.remaining_seconds(61_000)),
        Some(3_540)
    );

    let text = common::load_text();
    let mut state = UiState::new(Language::ZhCn);
    state.show_quests = true;
    state.set_time(61_000);
    let (world_only, frame) = render(&mut state, &snapshot, &text);
    let screen = Rect::screen(800, 600);
    let layout = quests::quest_layout(screen, snapshot.quests.len(), 0);
    assert!(state.show_quests);
    let changed = changed_pixels(&world_only, &frame, layout.panel);
    assert!(changed > 1_000, "任务面板应覆盖地图像素，实际 {changed}");

    // 任务完成的那一行用绿色名字（`quests::COMPLETE_COLOR`）。
    let completed_row = layout.rows[1].row;
    let green = greenish_pixels(&frame, completed_row);
    assert!(green > 10, "完成任务的提示应是绿色，实际 {green} 像素");

    let output = workspace_root().join("target/client-quests.png");
    frame
        .save_png(&output)
        .expect("写出 target/client-quests.png");
    assert!(
        std::fs::metadata(&output).is_ok_and(|meta| meta.len() > 4_096),
        "PNG 不应为空文件"
    );

    // 点第一行的「放弃」→ `C.AbandonQuest { quest_index: 1 }`。
    state.mouse = layout.rows[0].abandon.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("放弃按钮应产生动作");
    assert_eq!(action, UiAction::AbandonQuest { quest_index: 1 });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::AbandonQuest { quest_index: 1 })
    );

    // 点第二行主体只选中，不发包。
    state.mouse = layout.rows[1].row.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.selected_quest, Some(2));

    // 滚轮在任务多于可见行时才有变化（这里 2 条，不变）。
    assert!(!state.on_wheel(&snapshot, 1));
    // 点面板空白处被吃掉，不会变成走路。
    state.mouse = layout.panel.center();
    assert!(
        state
            .on_click(&snapshot, &text, screen, Some(Point::new(40, 40)))
            .is_none()
    );
}

fn storage_snapshot() -> WorldSnapshot {
    let mut snapshot = base_snapshot();
    let mut slots = vec![None; 80];
    slots[0] = Some(user_item(21, 3, 5, 0));
    slots[10] = Some(user_item(22, 1, 1, 1_000));
    snapshot.apply_packet(&ServerPacket::UserStorage(UserStorage {
        storage: Some(slots),
    }));
    snapshot.apply_packet(&ServerPacket::NPCStorage(NPCStorage {}));
    snapshot
}

#[test]
fn storage_panel_opens_from_npc_storage_and_moves_items_both_ways() {
    let snapshot = storage_snapshot();
    let text = common::load_text();
    let mut state = UiState::new(Language::ZhCn);
    state.sync(&snapshot);
    assert!(state.show_storage, "S.NPCStorage 应自动打开仓库");
    assert!(
        state.show_inventory,
        "仓库打开时背包也要一起显示（拖拽存取）"
    );
    let (world_only, frame) = render(&mut state, &snapshot, &text);
    let screen = Rect::screen(800, 600);
    let layout = storage::storage_layout(screen);
    let changed = changed_pixels(&world_only, &frame, layout.panel);
    assert!(changed > 1_000, "仓库面板应覆盖地图像素，实际 {changed}");
    assert_eq!(storage::used_slots(&snapshot), 2);

    let output = workspace_root().join("target/client-storage.png");
    frame
        .save_png(&output)
        .expect("写出 target/client-storage.png");
    assert!(std::fs::metadata(&output).is_ok_and(|meta| meta.len() > 4_096));

    // 背包 0 号格 → 仓库 5 号格：`C.StoreItem { from: 0, to: 5 }`。
    let inventory_panel = inventory::inventory_panel_rect(screen);
    let from = inventory::inventory_slot_rect(inventory_panel, 0).expect("背包 0 号格");
    let to = layout.grid.slots[5];
    state.mouse = from.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert!(state.drag.is_some(), "按下背包格应开始拖拽");
    state.mouse = to.center();
    let action = state
        .on_mouse_release(&snapshot, screen)
        .expect("松手在仓库格应产生存物品动作");
    assert_eq!(action, UiAction::StoreItem { from: 0, to: 5 });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::StoreItem { from: 0, to: 5 })
    );

    // 仓库 0 号格 → 背包 2 号格：`C.TakeBackItem { from: 0, to: 2 }`。
    state.mouse = layout.grid.slots[0].center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert!(state.drag.is_some(), "按下仓库格应开始拖拽");
    state.mouse = inventory::inventory_slot_rect(inventory_panel, 2)
        .expect("背包 2 号格")
        .center();
    let action = state
        .on_mouse_release(&snapshot, screen)
        .expect("松手在背包格应产生取回动作");
    assert_eq!(action, UiAction::TakeBackItem { from: 0, to: 2 });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::TakeBackItem { from: 0, to: 2 })
    );

    // 服务端回执把物品搬进快照。
    let mut moved = snapshot.clone();
    moved.apply_packet(&ServerPacket::StoreItem(
        mir_protocol::packets::server::StoreItem {
            from: 0,
            to: 5,
            success: true,
        },
    ));
    assert!(moved.inventory_item(0).is_none());
    assert_eq!(
        moved.storage_item(5).map(|item| item.unique_id),
        Some(11),
        "S.StoreItem 应把背包物品搬进仓库"
    );
    moved.apply_packet(&ServerPacket::TakeBackItem(
        mir_protocol::packets::server::TakeBackItem {
            from: 0,
            to: 2,
            success: true,
        },
    ));
    assert_eq!(
        moved.inventory_item(2).map(|item| item.unique_id),
        Some(21),
        "S.TakeBackItem 应把仓库物品搬回背包"
    );

    // `NPCResponse`（换页）会关掉仓库面板。
    moved.apply_packet(&ServerPacket::NPCResponse(
        mir_protocol::packets::server::NPCResponse { page: Vec::new() },
    ));
    assert!(!moved.storage_open);
    state.sync(&moved);
    assert!(!state.show_storage);
}

fn mail_snapshot() -> WorldSnapshot {
    let mut snapshot = base_snapshot();
    snapshot.apply_packet(&ServerPacket::ReceiveMail(ReceiveMail {
        mail: vec![
            ClientMail {
                mail_id: 100,
                sender_name: "张三".to_owned(),
                message: "你好\n附上鹿肉，请查收。".to_owned(),
                opened: false,
                locked: false,
                can_reply: true,
                collected: false,
                date_sent: DateTimeBinary::from_unix_seconds(1_704_067_200),
                gold: 500,
                items: vec![user_item(31, 3, 2, 0)],
            },
            ClientMail {
                mail_id: 101,
                sender_name: "李四".to_owned(),
                message: "这是锁定的信。".to_owned(),
                opened: true,
                locked: true,
                can_reply: false,
                collected: true,
                date_sent: DateTimeBinary::from_unix_seconds(1_709_642_040),
                gold: 0,
                items: Vec::new(),
            },
        ],
    }));
    snapshot
}

#[test]
fn mail_panel_reads_collects_locks_deletes_and_sends() {
    let snapshot = mail_snapshot();
    let text = common::load_text();
    let mut state = UiState::new(Language::ZhCn);
    state.show_mail = true;
    state.sync(&snapshot);
    assert_eq!(snapshot.mails.len(), 2);
    assert!(
        snapshot
            .mail(100)
            .is_some_and(mir_client::net::MailView::can_collect)
    );

    let (world_only, frame) = render(&mut state, &snapshot, &text);
    let screen = Rect::screen(800, 600);
    let layout = mail::mail_layout(
        screen,
        mail::MailTab::Inbox,
        snapshot.mails.len(),
        0,
        snapshot.inventory.len(),
    );
    let changed = changed_pixels(&world_only, &frame, layout.panel);
    assert!(changed > 1_000, "邮件面板应覆盖地图像素，实际 {changed}");

    let output = workspace_root().join("target/client-mail.png");
    frame
        .save_png(&output)
        .expect("写出 target/client-mail.png");
    assert!(std::fs::metadata(&output).is_ok_and(|meta| meta.len() > 4_096));

    // 点第一行 → `C.ReadMail`。
    state.mouse = layout.rows[0].center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("点邮件行应产生阅读动作");
    assert_eq!(action, UiAction::ReadMail { mail_id: 100 });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::ReadMail { mail_id: 100 })
    );
    assert_eq!(state.mail_selected, Some(100));

    // 「收取附件」→ `C.CollectParcel`。
    state.mouse = layout.collect.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("收取附件按钮应产生动作");
    assert_eq!(action, UiAction::CollectParcel { mail_id: 100 });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::CollectParcel { mail_id: 100 })
    );

    // 「锁定」→ `C.LockMail { lock: true }`。
    state.mouse = layout.lock.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("锁定按钮应产生动作");
    assert_eq!(
        action,
        UiAction::LockMail {
            mail_id: 100,
            lock: true
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::LockMail {
            mail_id: 100,
            lock: true
        })
    );

    // 「删除」→ `C.DeleteMail`（未锁定才允许）。
    state.mouse = layout.delete.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("删除按钮应产生动作");
    assert_eq!(action, UiAction::DeleteMail { mail_id: 100 });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::DeleteMail { mail_id: 100 })
    );

    // 切到写信页：填表单 + 点背包格挂附件 + 贴邮票，然后发送。
    state.mouse = layout.compose_tab.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.mail_tab, mail::MailTab::Compose);
    let compose_layout = mail::mail_layout(
        screen,
        mail::MailTab::Compose,
        snapshot.mails.len(),
        0,
        snapshot.inventory.len(),
    );
    state.mouse = compose_layout.subject.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.focus, Some(ui::FieldId::MailSubject));
    assert!(state.type_char('主'));
    assert!(state.type_char('题'));
    assert!(state.backspace());

    state.mail_compose.recipient.set("李四");
    state.mail_compose.message.set("给力");
    state.mail_compose.gold.set("1000");
    // 点写信页自带的背包挑选网格（`pick_grid`）把背包 0 号物品挂成附件。
    // 邮件面板盖住了右侧背包面板，所以这里走面板内的挑选网格。
    state.mouse = compose_layout.pick_grid.slots[0].center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.mail_compose.attachment_count(), 1);
    // 贴邮票。
    state.mouse = compose_layout.stamp.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert!(state.mail_compose.stamped);

    // 写信页也要真的画出来（字段/附件格/背包挑选格/按钮）。
    let (_, compose_frame) = render(&mut state, &snapshot, &text);
    assert!(
        changed_pixels(&world_only, &compose_frame, compose_layout.panel) > 1_000,
        "写信页应覆盖地图像素"
    );

    state.mouse = compose_layout.send.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("发送按钮应产生动作");
    assert_eq!(
        action,
        UiAction::SendMail {
            name: "李四".to_owned(),
            message: "给力".to_owned(),
            gold: 1_000,
            items_idx: [11, 0, 0, 0, 0],
            stamped: true,
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::SendMail {
            name: "李四".to_owned(),
            message: "给力".to_owned(),
            gold: 1_000,
            items_idx: [11, 0, 0, 0, 0],
            stamped: true,
        })
    );

    // 「询价」→ `C.MailCost`。
    state.mouse = compose_layout.quote.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("询价按钮应产生动作");
    assert_eq!(
        action.net_command(),
        Some(NetCommand::MailCost {
            gold: 1_000,
            items_idx: [11, 0, 0, 0, 0],
            stamped: true,
        })
    );
}

fn guild_snapshot() -> WorldSnapshot {
    let mut snapshot = base_snapshot();
    snapshot.apply_packet(&ServerPacket::GuildStatus(GuildStatus {
        guild_name: "沙巴克".to_owned(),
        guild_rank_name: "会长".to_owned(),
        level: 3,
        experience: 12_000,
        max_experience: 45_000,
        gold: 10_000,
        spare_points: 2,
        member_count: 2,
        max_members: 20,
        item_count: 112,
        my_options: GuildRankOptions(255),
        my_rank_id: 0,
        ..GuildStatus::default()
    }));
    let leader = GuildMember {
        name: "会长大人".to_owned(),
        id: 1,
        online: true,
        ..GuildMember::default()
    };
    let member = GuildMember {
        name: "小明".to_owned(),
        id: 2,
        online: false,
        ..GuildMember::default()
    };
    snapshot.apply_packet(&ServerPacket::GuildMemberChange(GuildMemberChange {
        name: "会长大人".to_owned(),
        rank_index: 0,
        status: 255,
        ranks: vec![
            GuildRank {
                name: "会长".to_owned(),
                options: 255,
                index: 0,
                members: vec![leader],
            },
            GuildRank {
                name: "成员".to_owned(),
                options: 0,
                index: 1,
                members: vec![member],
            },
        ],
    }));
    snapshot.apply_packet(&ServerPacket::GuildNoticeChange(GuildNoticeChange {
        update: 1,
        notice: vec!["今晚八点攻沙。".to_owned(), "请提前集合。".to_owned()],
    }));
    let mut items = vec![None; 112];
    items[0] = Some(GuildStorageItem {
        item: user_item(41, 3, 9, 0),
        user_id: 1,
    });
    items[7] = Some(GuildStorageItem {
        item: user_item(42, 2, 3, 0),
        user_id: 2,
    });
    snapshot.apply_packet(&ServerPacket::GuildStorageList(GuildStorageList { items }));
    snapshot.apply_packet(&ServerPacket::GuildBuffList(GuildBuffList {
        remove: 0,
        active_buffs: Vec::new(),
        guild_buffs: vec![GuildBuffInfo {
            id: 3,
            icon: 12,
            name: "公会之力".to_owned(),
            level_requirement: 1,
            points_requirement: 1,
            time_limit: 60,
            activation_cost: 500,
            stats: [(Stat::MaxDC, 5)].into_iter().collect(),
        }],
    }));
    snapshot
}

#[test]
fn guild_panel_shows_members_storage_buffs_and_notice_actions() {
    let snapshot = guild_snapshot();
    assert_eq!(snapshot.guild_members().len(), 2);
    assert_eq!(snapshot.guild_ranks.len(), 2);
    assert_eq!(snapshot.guild_notice.len(), 2);
    assert_eq!(snapshot.guild_buff_infos.len(), 1);
    assert_eq!(
        snapshot.guild.as_ref().map(|guild| guild.gold),
        Some(10_000)
    );

    let text = common::load_text();
    let mut state = UiState::new(Language::ZhCn);
    state.show_guild = true;
    // 行会仓库的拖拽存取需要背包一起可见（Crystal 的仓库页也是和背包并排用）。
    state.show_inventory = true;
    state.sync(&snapshot);
    let (world_only, frame) = render(&mut state, &snapshot, &text);
    let screen = Rect::screen(800, 600);
    let layout = guild::guild_layout(
        screen,
        snapshot.guild_members().len(),
        snapshot.guild_buff_infos.len(),
        0,
        0,
    );
    let changed = changed_pixels(&world_only, &frame, layout.panel);
    assert!(changed > 1_000, "行会面板应覆盖地图像素，实际 {changed}");

    let output = workspace_root().join("target/client-guild.png");
    frame
        .save_png(&output)
        .expect("写出 target/client-guild.png");
    assert!(std::fs::metadata(&output).is_ok_and(|meta| meta.len() > 4_096));

    // 成员页：选中「小明」（第 2 行）→ 升职 → `C.EditGuildMember{ChangeType=2}`。
    state.mouse = layout.member_rows[1].center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.guild_selected_member, Some(1));
    state.mouse = layout.promote.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("升职按钮应产生动作");
    assert_eq!(
        action,
        UiAction::EditGuildMember {
            change_type: 2,
            rank_index: 0,
            name: "小明".to_owned(),
            rank_name: String::new(),
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::EditGuildMember {
            change_type: 2,
            rank_index: 0,
            name: "小明".to_owned(),
            rank_name: String::new(),
        })
    );

    // 增益页：点「购买」→ `C.GuildBuffUpdate{action: 1, id: 3}`。
    state.mouse = layout.tabs[2].center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.guild_tab, guild::GuildTab::Buffs);
    let (_, buffs_frame) = render(&mut state, &snapshot, &text);
    assert!(
        changed_pixels(&world_only, &buffs_frame, layout.panel) > 1_000,
        "增益页应覆盖地图像素"
    );
    state.mouse = layout.buff_buttons[0].center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("增益按钮应产生动作");
    assert_eq!(action, UiAction::GuildBuffUpdate { action: 1, id: 3 });
    assert_eq!(
        action.net_command(),
        Some(NetCommand::GuildBuffUpdate { action: 1, id: 3 })
    );

    // 仓库页：输入金额 → 存入 → `C.GuildStorageGoldChange{Type=0}`。
    state.mouse = layout.tabs[1].center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.guild_tab, guild::GuildTab::Storage);
    let (_, storage_frame) = render(&mut state, &snapshot, &text);
    assert!(
        changed_pixels(&world_only, &storage_frame, layout.panel) > 1_000,
        "行会仓库页应覆盖地图像素"
    );
    state.mouse = layout.gold_field.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.focus, Some(ui::FieldId::GuildGold));
    for digit in ['5', '0', '0'] {
        assert!(state.type_char(digit));
    }
    state.mouse = layout.deposit.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("存入按钮应产生动作");
    assert_eq!(
        action,
        UiAction::GuildStorageGoldChange {
            type_: 0,
            amount: 500
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::GuildStorageGoldChange {
            type_: 0,
            amount: 500
        })
    );

    // 行会仓库拖拽：背包 0 号格 → 行会仓库 5 号格 → `C.GuildStorageItemChange{Type=0}`。
    let inventory_panel = inventory::inventory_panel_rect(screen);
    state.mouse = inventory::inventory_slot_rect(inventory_panel, 0)
        .expect("背包 0 号格")
        .center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    state.mouse = layout.storage_grid.slots[5].center();
    let action = state
        .on_mouse_release(&snapshot, screen)
        .expect("松手在行会仓库格应产生存物品动作");
    assert_eq!(
        action,
        UiAction::GuildStorageItemChange {
            type_: 0,
            from: 0,
            to: 5
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::GuildStorageItemChange {
            type_: 0,
            from: 0,
            to: 5
        })
    );

    // 公告页：输入公告并保存 → `C.EditGuildNotice`。
    state.mouse = layout.tabs[3].center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.guild_tab, guild::GuildTab::Notice);
    let (_, notice_frame) = render(&mut state, &snapshot, &text);
    assert!(
        changed_pixels(&world_only, &notice_frame, layout.panel) > 1_000,
        "公告页应覆盖地图像素"
    );
    state.mouse = layout.notice_field.center();
    assert!(state.on_click(&snapshot, &text, screen, None).is_none());
    assert_eq!(state.focus, Some(ui::FieldId::GuildNotice));
    state.guild_notice.set("新公告");
    state.mouse = layout.save_notice.center();
    let action = state
        .on_click(&snapshot, &text, screen, None)
        .expect("保存公告应产生动作");
    assert_eq!(
        action,
        UiAction::EditGuildNotice {
            notice: vec!["新公告".to_owned()]
        }
    );
}

/// 已有商店绘制（`ui::shop`）的测试不需要再复制一份，但这里留一条编译断言：
/// 面板布局函数在新窗口尺寸下仍然可用。
#[test]
fn layouts_survive_small_windows() {
    let small = Rect::screen(320, 240);
    assert!(quests::quest_layout(small, 3, 0).panel.width <= 320);
    assert!(storage::storage_layout(small).panel.x >= 0);
    assert!(
        mail::mail_layout(small, mail::MailTab::Compose, 5, 0, 46)
            .panel
            .width
            <= 320
    );
    assert!(guild::guild_layout(small, 5, 5, 0, 0).panel.height <= 240);
    let _ = shop::shop_layout(small, 3, 0, shop::SHOP_LEFT);
}
