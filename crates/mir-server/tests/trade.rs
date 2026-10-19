//! 玩家间交易的端到端测试（Crystal `PlayerObject.cs:10528-10960`）。
//!
//! 全部确定性：世界随机数固定种子、时间只用 [`World::advance_clock`] 快进（不 sleep），
//! 玩家用固定对象 ID / 坐标 / 朝向构造。每个断言对应 Crystal 的一处行为，注释里写了出处；
//! 包序断言直接解出 `ServerPacket` 再比对。
//!
//! 覆盖（对应任务清单 1-17）：
//! 1. 请求成功；2. 目标不在（面朝格没人）；3. 距离太远；4. 已在交易中；5. 自己交易自己；
//! 6. 接受后进入交易；7. 放物品与取回；8. 放金币与取回（取消时退回）；
//! 9. 单方确认不成交；10. 双方确认成交；11. 改动物品重置准备；
//! 12. 背包满时成交失败（解锁、不成交）；13. 取消交易退回物品；
//! 14. 下线中断；15. 移动/转身/切图中断；16. `DontTrade` 拒绝；17. 物品数量守恒；
//!
//! 另加：交易开关、拒绝邀请、背包满时取消改寄邮件、槽位校验、重复取消幂等。

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{
    AttackMode, BindMode, ChatType, ItemType, MirClass, MirDirection, MirGender, MirGridType, Stat,
};
use mir_db::MirDb;
use mir_protocol::ServerPacket;
use mir_protocol::data::{ItemInfo, Stats, UserItem};
use mir_server::combat::{self, CombatStats};
use mir_server::gamedata::GameData;
use mir_server::items;
use mir_server::trade::{TRADE_DELAY_MS, TRADE_SLOTS};
use mir_server::world::{Player, World};
use mir_server::{NpcState, TradeError};
use tokio::sync::mpsc;

const MAP: &str = "0";

/// 真实语言包（`config/lang`，仓库根 = `CARGO_MANIFEST_DIR/../..`）。
///
/// 断言里的玩家可见文本一律从语言包取模板渲染，**不在测试里写死中英文案**：
/// 文案调整时测试不会变红，只有「渲染走错语言/参数名写错」才会失败。
fn language_pack() -> &'static mir_core::i18n::LanguageLibrary {
    static PACK: std::sync::OnceLock<mir_core::i18n::LanguageLibrary> = std::sync::OnceLock::new();
    PACK.get_or_init(|| {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("config/lang");
        mir_core::i18n::LanguageLibrary::load_dir(&dir)
            .unwrap_or_else(|error| panic!("读取语言包 {} 失败：{error}", dir.display()))
    })
}

/// 简体中文下的无参文本。
fn zh(key: mir_core::TextKey) -> String {
    language_pack().text(mir_core::Language::ZhCn, key)
}

/// 简体中文下的带参文本（参数名与 `mir-core` 的 `{snake_case}` 占位符一致）。
fn zh_args(key: mir_core::TextKey, args: &[(&str, &str)]) -> String {
    language_pack().format(mir_core::Language::ZhCn, key, args)
}

/// 正常可交易物品。
const SWORD: i32 = 1;
/// 另一件正常可交易物品。
const AXE: i32 = 2;
/// `BindMode::DontTrade` 物品。
const BOUND: i32 = 3;

// ---------------------------------------------------------------------------
// 测试脚手架
// ---------------------------------------------------------------------------

struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
    next_uid: u64,
}

impl Harness {
    fn new(seed: u64) -> Self {
        let mut world = World::new(999);
        world.seed_rng(seed);
        let db = MirDb {
            items: vec![
                item_info(SWORD, "测试之剑", BindMode::None),
                item_info(AXE, "测试之斧", BindMode::None),
                item_info(BOUND, "禁交易之剑", BindMode::DontTrade),
            ],
            ..MirDb::default()
        };
        world.set_game_data(Arc::new(GameData::from_db(db, None)));
        Self {
            world,
            receivers: HashMap::new(),
            next_uid: 1,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_player(
        &mut self,
        object_id: u32,
        name: &str,
        position: Point,
        direction: MirDirection,
        gold: i64,
    ) -> u32 {
        let stats = combat::player_base_stats(10, MirClass::Warrior);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.enter(Player {
            object_id,
            session_id: u64::from(object_id),
            character_id: i64::from(object_id),
            name: name.to_owned(),
            map: MAP.to_owned(),
            position,
            direction,
            level: 10,
            experience: 0,
            max_experience: 0,
            class: MirClass::Warrior,
            gender: MirGender::Male,
            hair: 1,
            hp: stats.hp,
            mp: stats.mp,
            stats,
            action_time: 0,
            attack_time: 0,
            struck_time: 0,
            regen_time: i64::MAX,
            death_time: 0,
            revive_time: 0,
            cell_time: 0,
            step_counter: 0,
            run_counter: 0,
            run_time: 0,
            dead: false,
            attack_mode: AttackMode::Peace,
            pk_points: 0,
            brown_time: 0,
            pk_point_time: 0,
            last_hitter: None,
            last_hit_time: 0,
            name_colour: mir_core::Color(-1),
            visible_monsters: BTreeSet::new(),
            visible_npcs: BTreeSet::new(),
            movement_time: 0,
            inventory: items::new_inventory(),
            equipment: items::new_equipment(),
            storage: mir_server::storage::new_storage(),
            magics: Vec::new(),
            magic: mir_server::MagicState::default(),
            gold,
            credit: 0,
            visible_items: BTreeSet::new(),
            quests: Vec::new(),
            completed_quests: BTreeSet::new(),
            mail: mir_server::MailState::default(),
            buffs: Vec::new(),
            pot_health_amount: 0,
            pot_mana_amount: 0,
            pot_time: 0,
            unlock_curse: false,
            npc: NpcState::default(),
            group_id: None,
            group_invitation: None,
            group_invite_time: 0,
            allow_group: true,
            guild: None,
            trade: mir_server::TradeState::default(),
            trade_invitation: None,
            next_trade_time: 0,
            allow_trade: true,
            sender,
        });
        self.receivers.insert(object_id, receiver);
        object_id
    }

    /// 建立一对「面对面」的玩家：甲 (10,10) 朝右，乙 (11,10) 朝左。
    fn duet(&mut self) -> (u32, u32) {
        let a = self.add_player(1, "甲", Point::new(10, 10), MirDirection::Right, 0);
        let b = self.add_player(2, "乙", Point::new(11, 10), MirDirection::Left, 1_000);
        self.clear(a);
        self.clear(b);
        (a, b)
    }

    fn packets(&mut self, object_id: u32) -> Vec<ServerPacket> {
        let receiver = self.receivers.get_mut(&object_id).expect("玩家通道");
        let mut packets = Vec::new();
        while let Ok(frame) = receiver.try_recv() {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
        }
        packets
    }

    fn clear(&mut self, object_id: u32) {
        let _ = self.packets(object_id);
    }

    /// 往背包的 `slot` 放一件物品，返回 `unique_id`。
    fn put_item(&mut self, object_id: u32, slot: usize, item_index: i32) -> u64 {
        let info = self
            .world
            .item_info(item_index)
            .cloned()
            .expect("物品模板存在");
        let mut item = self.world.create_fresh_item(&info);
        let unique_id = self.next_uid;
        self.next_uid += 1;
        item.unique_id = unique_id;
        if let Some(player) = self.world.player_mut(object_id) {
            player.inventory[slot] = Some(item);
        }
        unique_id
    }

    /// 把背包塞满（空格都放「测试之剑」）。
    fn fill_inventory(&mut self, object_id: u32) {
        for slot in 0..items::INVENTORY_SIZE {
            if self.world.player(object_id).expect("玩家").inventory[slot].is_none() {
                self.put_item(object_id, slot, SWORD);
            }
        }
    }

    fn inventory_count(&self, object_id: u32) -> usize {
        self.world
            .player(object_id)
            .expect("玩家")
            .inventory
            .iter()
            .flatten()
            .count()
    }

    fn inventory_uids(&self, object_id: u32) -> Vec<u64> {
        let mut ids: Vec<u64> = self
            .world
            .player(object_id)
            .expect("玩家")
            .inventory
            .iter()
            .flatten()
            .map(|item| item.unique_id)
            .collect();
        ids.sort_unstable();
        ids
    }

    fn gold(&self, object_id: u32) -> i64 {
        self.world.player(object_id).expect("玩家").gold
    }

    /// 走完「请求 → 接受」，把双方都送进交易状态并清空过程包。
    fn start_trade(&mut self, a: u32, b: u32) {
        // 清掉上一次交易留下的 2 秒节流（`Settings.TradeDelay`）。
        self.world.advance_clock(TRADE_DELAY_MS);
        self.world.trade_request(a).expect("发起交易");
        self.world.trade_reply(b, true).expect("接受交易");
        self.clear(a);
        self.clear(b);
    }
}

fn stats(pairs: &[(Stat, i32)]) -> Stats {
    Stats::from_iter(pairs.iter().copied())
}

fn item_info(index: i32, name: &str, bind: BindMode) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        item_type: ItemType::Weapon,
        bind,
        weight: 1,
        image: u16::try_from(index).unwrap_or(0),
        durability: 5_000,
        stack_size: 1,
        price: 100,
        shape: 0,
        stats: stats(&[(Stat::MaxDC, 1)]),
        ..ItemInfo::default()
    }
}

fn trade_items(packet: &ServerPacket) -> Option<&Vec<Option<UserItem>>> {
    match packet {
        ServerPacket::TradeItem(item) => Some(&item.trade_items),
        _ => None,
    }
}

/// 从已解出的包序列里挑出系统聊天文本。
fn system_chats(packets: &[ServerPacket]) -> Vec<String> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::Chat(chat) if chat.type_ == ChatType::System => {
                Some(chat.message.clone())
            }
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. 发起交易
// ---------------------------------------------------------------------------

#[test]
fn trade_request_sends_invitation_to_the_faced_player() {
    let mut harness = Harness::new(1);
    let (a, b) = harness.duet();

    let target = harness
        .world
        .trade_request(a)
        .expect("面朝着的玩家可以交易");
    assert_eq!(target, b);

    // `player.TradeInvitation = this; player.Enqueue(new S.TradeRequest { Name = Info.Name });`
    // （`PlayerObject.cs:10704-10705`）
    assert_eq!(harness.world.trade_invitation_of(b), Some(a));
    let packets = harness.packets(b);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::TradeRequest(request) if request.name == "甲"
        )),
        "乙应收到 S.TradeRequest{{Name=甲}}，实际 {packets:?}"
    );
    // 发起者自己不收任何交易包。
    assert!(harness.packets(a).is_empty(), "发起者不应收到交易包");
}

#[test]
fn trade_request_requires_a_faced_player() {
    let mut harness = Harness::new(2);
    let a = harness.add_player(1, "甲", Point::new(10, 10), MirDirection::Up, 0);
    // 乙站在甲的**背后**（右边），但甲朝上：面朝格没人。
    let b = harness.add_player(2, "乙", Point::new(11, 10), MirDirection::Left, 0);
    harness.clear(a);
    harness.clear(b);

    let error = harness
        .world
        .trade_request(a)
        .expect_err("面朝格没人不能交易");
    assert_eq!(error, TradeError::NoTarget);
    // `ReceiveChat(FaceToTrade, ChatType.System)`（`PlayerObject.cs:10642`）
    assert_eq!(
        system_chats(&harness.packets(a)),
        vec![zh(mir_core::TextKey::TradeNotFacing)]
    );
    assert_eq!(harness.world.trade_invitation_of(b), None);
}

#[test]
fn trade_request_rejects_a_player_that_does_not_face_back() {
    let mut harness = Harness::new(3);
    let a = harness.add_player(1, "甲", Point::new(10, 10), MirDirection::Right, 0);
    // 乙站对了格子，但**也朝右**（没回头）：`Functions.FacingEachOther` 不通过。
    harness.add_player(2, "乙", Point::new(11, 10), MirDirection::Right, 0);
    harness.clear(a);

    let error = harness
        .world
        .trade_request(a)
        .expect_err("没有面对面不能交易");
    assert_eq!(error, TradeError::NotFacing);
    assert_eq!(
        system_chats(&harness.packets(a)),
        vec![zh(mir_core::TextKey::TradeNotFacing)]
    );
    assert_eq!(harness.world.trade_invitation_of(2), None);
}

#[test]
fn trade_request_rejects_a_target_out_of_range() {
    let mut harness = Harness::new(4);
    let a = harness.add_player(1, "甲", Point::new(10, 10), MirDirection::Right, 0);
    // 超出 `Globals.DataRange = 16`（`Shared/Globals.cs:37`）。
    let b = harness.add_player(2, "乙", Point::new(40, 40), MirDirection::Left, 0);
    harness.clear(a);
    harness.clear(b);

    let error = harness
        .world
        .trade_request_target(a, b)
        .expect_err("超出 DataRange 应被拒绝");
    assert_eq!(error, TradeError::OutOfRange("乙".to_owned()));
    // `ReceiveChat(PlayerNotInTradeRange, ...)`（`PlayerObject.cs:10694`）
    assert_eq!(
        system_chats(&harness.packets(a)),
        vec![zh_args(
            mir_core::TextKey::TradeTargetOutOfRange,
            &[("target_name", "乙")]
        )]
    );
    assert_eq!(harness.world.trade_invitation_of(b), None);
}

#[test]
fn trade_request_rejects_trading_with_yourself() {
    let mut harness = Harness::new(5);
    let a = harness.add_player(1, "甲", Point::new(10, 10), MirDirection::Right, 0);
    harness.clear(a);

    // Crystal `if (player == this)`（`:10668-10672`）。
    let error = harness
        .world
        .trade_request_target(a, a)
        .expect_err("不能和自己交易");
    assert_eq!(error, TradeError::SelfTrade);
    assert_eq!(
        system_chats(&harness.packets(a)),
        vec![zh(mir_core::TextKey::TradeSelf)]
    );
    assert_eq!(harness.world.trade_partner(a), None);
}

#[test]
fn trade_request_rejects_players_already_trading_or_invited() {
    let mut harness = Harness::new(6);
    let (a, b) = harness.duet();
    let c = harness.add_player(3, "丙", Point::new(12, 10), MirDirection::Left, 0);
    let d = harness.add_player(4, "丁", Point::new(20, 20), MirDirection::Left, 0);
    harness.clear(c);
    harness.clear(d);
    harness.start_trade(a, b);

    // 丙 → 甲：甲已经在交易中（`:10698-10702`）。
    harness
        .world
        .trade_request_target(c, a)
        .expect_err("目标已在交易中");
    assert_eq!(
        system_chats(&harness.packets(c)),
        vec![zh_args(
            mir_core::TextKey::TradeTargetAlreadyTrading,
            &[("target_name", "甲")]
        )]
    );

    // 甲 → 丙：自己已经在交易中（`:10628-10632`）。
    harness.world.advance_clock(TRADE_DELAY_MS);
    let error = harness
        .world
        .trade_request_target(a, c)
        .expect_err("自己在交易中");
    assert_eq!(error, TradeError::AlreadyTrading);
    assert_eq!(
        system_chats(&harness.packets(a)),
        vec![zh(mir_core::TextKey::TradeAlreadyTrading)]
    );

    // 结束甲/乙的交易，然后：丙 → 丁 成功；甲 → 丁 撞上「丁已经收到邀请」
    // （`PlayerAlreadyHasTradeInvitation`，`:10680-10684`）。
    harness.world.trade_cancel(a);
    harness.world.advance_clock(TRADE_DELAY_MS);
    harness
        .world
        .trade_request_target(c, d)
        .expect("第一次邀请成功");
    harness.world.advance_clock(TRADE_DELAY_MS);
    harness
        .world
        .trade_request_target(a, d)
        .expect_err("丁已经有邀请");
    assert_eq!(
        system_chats(&harness.packets(a)),
        vec![zh_args(
            mir_core::TextKey::TradeTargetAlreadyInvited,
            &[("target_name", "丁")]
        )]
    );
}

#[test]
fn trade_request_respects_the_allow_trade_switch() {
    let mut harness = Harness::new(7);
    let (a, b) = harness.duet();
    // `C.ChangeTrade{AllowTrade=false}`（`MirConnection.cs:1448-1452`）。
    assert!(harness.world.set_allow_trade(b, false));
    assert_eq!(harness.world.allow_trade_of(b), Some(false));

    let error = harness
        .world
        .trade_request(a)
        .expect_err("对方关掉交易开关");
    assert_eq!(error, TradeError::TargetNotAllowingTrade("乙".to_owned()));
    assert_eq!(
        system_chats(&harness.packets(a)),
        vec![zh_args(
            mir_core::TextKey::TradeTargetDisallowed,
            &[("target_name", "乙")]
        )]
    );
}

#[test]
fn trade_request_is_throttled_by_trade_delay() {
    let mut harness = Harness::new(8);
    let a = harness.add_player(1, "甲", Point::new(10, 10), MirDirection::Up, 0);
    harness.clear(a);

    // 第一次：面朝格没人，但 `NextTradeTime` 已经被推后（`:10625-10626`）。
    harness.world.trade_request(a).expect_err("面朝格没人");
    // 立刻再请求：静默丢弃（`Envir.Time < NextTradeTime`）。
    let error = harness.world.trade_request(a).expect_err("节流");
    assert_eq!(error, TradeError::Cooldown);
    // 只应有一条系统提示（节流那次什么都不发）。
    assert_eq!(
        system_chats(&harness.packets(a)),
        vec![zh(mir_core::TextKey::TradeNotFacing)]
    );
}

// ---------------------------------------------------------------------------
// 2. 接受 / 拒绝
// ---------------------------------------------------------------------------

#[test]
fn accepting_the_invitation_puts_both_players_into_trade() {
    let mut harness = Harness::new(9);
    let (a, b) = harness.duet();

    harness.world.trade_request(a).expect("发起");
    harness.clear(a);
    harness.clear(b);
    harness.world.trade_reply(b, true).expect("接受");

    // `TradePartner = TradeInvitation; TradeInvitation.TradePartner = this;`（`:10737-10738`）
    assert_eq!(harness.world.trade_partner(a), Some(b));
    assert_eq!(harness.world.trade_partner(b), Some(a));
    assert_eq!(harness.world.trade_invitation_of(b), None);
    // 双方各一条 `S.TradeAccept{Name=对方}`（`:10741-10742`）。
    let a_packets = harness.packets(a);
    let b_packets = harness.packets(b);
    assert!(
        a_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::TradeAccept(accept) if accept.name == "乙"
        )),
        "甲应收到 S.TradeAccept{{Name=乙}}，实际 {a_packets:?}"
    );
    assert!(
        b_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::TradeAccept(accept) if accept.name == "甲"
        )),
        "乙应收到 S.TradeAccept{{Name=甲}}，实际 {b_packets:?}"
    );
}

#[test]
fn declining_the_invitation_notifies_the_inviter() {
    let mut harness = Harness::new(10);
    let (a, b) = harness.duet();

    harness.world.trade_request(a).expect("发起");
    harness.clear(a);
    harness.clear(b);
    harness.world.trade_reply(b, false).expect("拒绝不是错误");

    // `TradeInvitation.ReceiveChat(PlayerRefusedTrade…)`（`:10716-10721`）。
    assert_eq!(
        system_chats(&harness.packets(a)),
        vec![zh_args(
            mir_core::TextKey::TradeDeclined,
            &[("invitee_name", "乙")]
        )]
    );
    assert_eq!(harness.world.trade_partner(a), None);
    assert_eq!(harness.world.trade_partner(b), None);
    assert_eq!(harness.world.trade_invitation_of(b), None);
}

// ---------------------------------------------------------------------------
// 3. 交易格
// ---------------------------------------------------------------------------

#[test]
fn depositing_and_retrieving_an_item_keeps_both_sides_in_sync() {
    let mut harness = Harness::new(11);
    let (a, b) = harness.duet();
    let uid = harness.put_item(a, 6, SWORD);
    harness.start_trade(a, b);

    // 放进交易格：背包 6 → 交易格 0。
    harness.world.trade_deposit_item(a, 6, 0).expect("放入");
    assert!(harness.world.player(a).expect("甲").inventory[6].is_none());
    let state = harness.world.trade_state(a).expect("甲");
    assert_eq!(
        state.items[0].as_ref().map(|item| item.unique_id),
        Some(uid)
    );

    let a_packets = harness.packets(a);
    assert!(
        a_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::DepositTradeItem(ack)
                if ack.from == 6 && ack.to == 0 && ack.success
        )),
        "本人应收到 S.DepositTradeItem{{6,0,true}}，实际 {a_packets:?}"
    );
    // `TradePartner.Enqueue(new S.TradeItem { TradeItems = Info.Trade });`（`:10776`）：
    // 对方得到**整份 10 格交易格**。
    let b_packets = harness.packets(b);
    let shown = b_packets
        .iter()
        .find_map(trade_items)
        .expect("乙应收到 S.TradeItem");
    assert_eq!(shown.len(), TRADE_SLOTS);
    assert_eq!(shown[0].as_ref().map(|item| item.unique_id), Some(uid));

    // 取回：交易格 0 → 背包 7。
    harness.world.trade_retrieve_item(a, 0, 7).expect("取回");
    let state = harness.world.trade_state(a).expect("甲");
    assert!(state.items.iter().all(Option::is_none));
    assert_eq!(
        harness.world.player(a).expect("甲").inventory[7]
            .as_ref()
            .map(|item| item.unique_id),
        Some(uid)
    );
    let a_packets = harness.packets(a);
    assert!(
        a_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::RetrieveTradeItem(ack)
                if ack.from == 0 && ack.to == 7 && ack.success
        )),
        "本人应收到 S.RetrieveTradeItem{{0,7,true}}，实际 {a_packets:?}"
    );
    // Crystal 的顺序是 `TradeItem()`（通知对方）**先**、本人回执**后**（`:10613-10618`）。
    let b_packets = harness.packets(b);
    assert!(
        b_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::TradeItem(item) if item.trade_items.iter().all(Option::is_none)
        )),
        "乙应收到「交易格已空」的 S.TradeItem，实际 {b_packets:?}"
    );
}

#[test]
fn depositing_a_dont_trade_item_is_rejected() {
    let mut harness = Harness::new(12);
    let (a, b) = harness.duet();
    harness.put_item(a, 6, BOUND);
    harness.start_trade(a, b);

    // `if (temp.Info.Bind.HasFlag(BindMode.DontTrade))`（`PlayerObject.cs:10554-10558`）。
    let error = harness
        .world
        .trade_deposit_item(a, 6, 0)
        .expect_err("禁交易");
    assert_eq!(error, TradeError::ItemBound);
    let a_packets = harness.packets(a);
    assert!(
        a_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::DepositTradeItem(ack) if !ack.success && ack.from == 6
        )),
        "应收到失败回执，实际 {a_packets:?}"
    );
    assert!(harness.world.player(a).expect("甲").inventory[6].is_some());
    assert!(
        harness
            .world
            .trade_state(a)
            .expect("甲")
            .items
            .iter()
            .all(Option::is_none)
    );
}

#[test]
fn deposit_and_retrieve_reject_out_of_range_or_occupied_slots() {
    let mut harness = Harness::new(13);
    let (a, b) = harness.duet();
    harness.put_item(a, 6, SWORD);
    harness.put_item(a, 7, AXE);
    harness.start_trade(a, b);

    assert_eq!(
        harness
            .world
            .trade_deposit_item(a, -1, 0)
            .expect_err("背包下标越界"),
        TradeError::InvalidSlot(-1)
    );
    assert_eq!(
        harness
            .world
            .trade_deposit_item(a, 6, TRADE_SLOTS as i32)
            .expect_err("交易格下标越界"),
        TradeError::InvalidSlot(TRADE_SLOTS as i32)
    );
    assert_eq!(
        harness
            .world
            .trade_deposit_item(a, 20, 0)
            .expect_err("空格"),
        TradeError::NoItem
    );

    harness.world.trade_deposit_item(a, 6, 0).expect("放入");
    // 目标交易格已被占用（`:10566`）。
    assert_eq!(
        harness
            .world
            .trade_deposit_item(a, 7, 0)
            .expect_err("交易格占用"),
        TradeError::SlotOccupied
    );
    // 取回到一个已占用的背包格（`:10606`）。
    harness.put_item(a, 8, AXE);
    assert_eq!(
        harness
            .world
            .trade_retrieve_item(a, 0, 8)
            .expect_err("背包格占用"),
        TradeError::InventorySlotOccupied
    );
}

// ---------------------------------------------------------------------------
// 4. 金币
// ---------------------------------------------------------------------------

#[test]
fn depositing_gold_is_deducted_and_refunded_on_cancel() {
    let mut harness = Harness::new(14);
    let (a, b) = harness.duet();
    harness.start_trade(a, b);
    assert_eq!(harness.gold(b), 1_000);

    // `TradeGoldAmount += amount; Account.Gold -= amount;`（`:10755-10756`）
    harness.world.trade_gold(b, 400).expect("放金币");
    assert_eq!(harness.gold(b), 600);
    assert_eq!(harness.world.trade_state(b).expect("乙").gold, 400);
    let b_packets = harness.packets(b);
    assert!(
        b_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::LoseGold(lost) if lost.gold == 400
        )),
        "本人应收到 S.LoseGold{{400}}，实际 {b_packets:?}"
    );
    // `TradePartner.Enqueue(new S.TradeGold { Amount = TradeGoldAmount });`（`:10759`）
    let a_packets = harness.packets(a);
    assert!(
        a_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::TradeGold(gold) if gold.amount == 400
        )),
        "甲应收到 S.TradeGold{{400}}，实际 {a_packets:?}"
    );

    // 累加：再放 250 → 对方看到 650。
    harness.world.trade_gold(b, 250).expect("再放金币");
    assert_eq!(harness.gold(b), 350);
    let a_packets = harness.packets(a);
    assert!(
        a_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::TradeGold(gold) if gold.amount == 650
        )),
        "甲应收到 S.TradeGold{{650}}，实际 {a_packets:?}"
    );

    // 取消 → 金币退回（`GainGold`，`:10948`）。
    harness.world.trade_cancel(b);
    assert_eq!(harness.gold(b), 1_000);
    assert_eq!(harness.world.trade_state(b).expect("乙").gold, 0);
    let b_packets = harness.packets(b);
    assert!(
        b_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::GainedGold(gained) if gained.gold == 650
        )),
        "取消后应收到 S.GainedGold{{650}}，实际 {b_packets:?}"
    );
}

#[test]
fn depositing_more_gold_than_held_is_rejected() {
    let mut harness = Harness::new(15);
    let (a, b) = harness.duet();
    harness.start_trade(a, b);

    let error = harness.world.trade_gold(b, 5_000).expect_err("金币不够");
    assert_eq!(
        error,
        TradeError::NotEnoughGold {
            requested: 5_000,
            held: 1_000
        }
    );
    assert_eq!(harness.gold(b), 1_000);
    assert_eq!(harness.world.trade_state(b).expect("乙").gold, 0);
}

// ---------------------------------------------------------------------------
// 5. 确认成交
// ---------------------------------------------------------------------------

#[test]
fn a_single_confirmation_does_not_trade() {
    let mut harness = Harness::new(16);
    let (a, b) = harness.duet();
    let uid = harness.put_item(a, 6, SWORD);
    harness.start_trade(a, b);
    harness.world.trade_deposit_item(a, 6, 0).expect("放入");
    harness.clear(a);
    harness.clear(b);

    harness.world.trade_confirm(a, true).expect("确认");

    assert!(harness.world.trade_state(a).expect("甲").locked);
    assert!(!harness.world.trade_state(b).expect("乙").locked);
    // 物品仍在交易格里，背包没有拿回来。
    assert_eq!(
        harness.world.trade_state(a).expect("甲").items[0]
            .as_ref()
            .map(|item| item.unique_id),
        Some(uid)
    );
    assert_eq!(harness.inventory_count(a), 0);
    let b_packets = harness.packets(b);
    // `TradePartner.ReceiveChat(PlayerWaitingConfirmTrade…)`（`:10814`）
    assert_eq!(
        system_chats(&b_packets),
        vec![zh_args(
            mir_core::TextKey::TradePartnerWaiting,
            &[("name", "甲")]
        )]
    );
    assert!(
        !b_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::TradeConfirm(_))),
        "单方确认不应成交"
    );
}

#[test]
fn move_item_trade_grid_deposits_retrieves_and_completes_trade() {
    let mut harness = Harness::new(70);
    let (a, b) = harness.duet();
    let uid = harness.put_item(a, 6, SWORD);
    harness.start_trade(a, b);
    harness.clear(a);
    harness.clear(b);

    // 背包 6 → 交易格 0（`C.MoveItem{Grid=Trade}`，无方向字段，按 from 是否有物品判存入）。
    harness
        .world
        .move_item(a, MirGridType::Trade, 6, 0)
        .expect("MoveItem 存入");
    assert!(harness.world.player(a).expect("甲").inventory[6].is_none());
    assert_eq!(
        harness.world.trade_state(a).expect("甲").items[0]
            .as_ref()
            .map(|item| item.unique_id),
        Some(uid)
    );
    let a_packets = harness.packets(a);
    assert!(
        a_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::DepositTradeItem(ack) if ack.success && ack.from == 6 && ack.to == 0
        )),
        "本人应收到 S.DepositTradeItem 成功：{a_packets:?}"
    );
    let b_packets = harness.packets(b);
    assert!(
        b_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::TradeItem(trade)
                if trade.trade_items[0].as_ref().is_some_and(|item| item.unique_id == uid)
        )),
        "对方应收到 S.TradeItem 同步：{b_packets:?}"
    );

    // 交易格 0 → 背包 6（`from` 在背包没有物品，按取回处理）。
    harness
        .world
        .move_item(a, MirGridType::Trade, 0, 6)
        .expect("MoveItem 取回");
    assert_eq!(harness.inventory_uids(a), vec![uid]);
    assert!(harness.world.trade_state(a).expect("甲").items[0].is_none());
    let a_packets = harness.packets(a);
    assert!(
        a_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::RetrieveTradeItem(ack) if ack.success && ack.from == 0 && ack.to == 6
        )),
        "本人应收到 S.RetrieveTradeItem 成功：{a_packets:?}"
    );

    // 再存入并成交：B 收到物品。
    harness
        .world
        .move_item(a, MirGridType::Trade, 6, 0)
        .expect("再次存入");
    harness.clear(a);
    harness.clear(b);
    harness.world.trade_confirm(a, true).expect("甲确认");
    harness.world.trade_confirm(b, true).expect("乙确认");
    assert_eq!(harness.inventory_uids(b), vec![uid]);
    let b_packets = harness.packets(b);
    assert!(
        b_packets.iter().any(
            |packet| matches!(packet, ServerPacket::GainedItem(item) if item.item.unique_id == uid)
        ),
        "成交后乙应收到 S.GainedItem：{b_packets:?}"
    );
}

#[test]
fn revive_broadcasts_object_revived_to_observer_and_self() {
    let mut harness = Harness::new(71);
    let (observer, revived) = harness.duet();
    {
        let target = harness.world.player_mut(revived).expect("乙");
        target.dead = true;
        target.hp = 0;
    }
    harness.clear(observer);
    harness.clear(revived);

    assert!(harness.world.revive_player(revived));
    let observer_packets = harness.packets(observer);
    let revived_packets = harness.packets(revived);
    for (id, packets) in [(observer, &observer_packets), (revived, &revived_packets)] {
        assert!(
            packets.iter().any(|packet| matches!(
                packet,
                ServerPacket::ObjectRevived(object) if object.object_id == revived
            )),
            "玩家 {id} 应收到 S.ObjectRevived{{object_id={revived}}}：{packets:?}"
        );
    }
    assert!(
        revived_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Revived(_))),
        "复活者本人仍应收到 S.Revived"
    );
}

#[test]
fn both_confirmations_swap_items_and_gold() {
    let mut harness = Harness::new(17);
    let (a, b) = harness.duet();
    let sword = harness.put_item(a, 6, SWORD);
    let axe = harness.put_item(b, 6, AXE);
    harness.start_trade(a, b);
    harness.world.trade_deposit_item(a, 6, 0).expect("甲放入");
    harness.world.trade_deposit_item(b, 6, 0).expect("乙放入");
    harness.world.trade_gold(b, 500).expect("乙放金币");
    harness.clear(a);
    harness.clear(b);

    harness.world.trade_confirm(a, true).expect("甲确认");
    harness.world.trade_confirm(b, true).expect("乙确认成交");

    // 背包互换（`TradePair[o].GainItem(u)`，`:10867`）
    assert_eq!(harness.inventory_uids(a), vec![axe]);
    assert_eq!(harness.inventory_uids(b), vec![sword]);
    // 金币互换（`TradePair[o].GainGold(...)`，`:10877`）
    assert_eq!(harness.gold(a), 500);
    assert_eq!(harness.gold(b), 500);
    // 交易状态清空（`:10884-10885`）
    for player in [a, b] {
        let state = harness.world.trade_state(player).expect("玩家");
        assert_eq!(state.partner, None);
        assert!(!state.locked);
        assert_eq!(state.gold, 0);
        assert!(state.items.iter().all(Option::is_none));
    }

    // 甲收到：作为 `TradePair[0]`（对方）先拿到「交易成功」+ `S.TradeConfirm`
    // （`:10881-10882`），之后作为接收方在第二轮才拿到乙的物品/金币（`:10867`、`:10877`）。
    // 这个「成交包先于物品包」的顺序是 Crystal 的原始顺序。
    let a_packets = harness.packets(a);
    let gained_item = a_packets
        .iter()
        .position(
            |packet| matches!(packet, ServerPacket::GainedItem(item) if item.item.unique_id == axe),
        )
        .expect("甲应收到乙的 S.GainedItem");
    let gained_gold = a_packets
        .iter()
        .position(|packet| matches!(packet, ServerPacket::GainedGold(gold) if gold.gold == 500))
        .expect("甲应收到 S.GainedGold{500}");
    let confirmed = a_packets
        .iter()
        .position(|packet| matches!(packet, ServerPacket::TradeConfirm(_)))
        .expect("甲应收到 S.TradeConfirm");
    let chatted = a_packets
        .iter()
        .position(|packet| matches!(packet, ServerPacket::Chat(_)))
        .expect("甲应收到「交易成功」");
    assert!(
        chatted < confirmed && confirmed < gained_item && gained_item < gained_gold,
        "包序应为聊天→成交→物品→金币：{a_packets:?}"
    );
    assert_eq!(
        system_chats(&a_packets),
        vec![zh(mir_core::TextKey::TradeSuccess)]
    );

    // 乙（最后确认的一方）先收到甲的物品，再收到「交易成功」+ 成交包。
    let b_packets = harness.packets(b);
    let b_gained = b_packets
        .iter()
        .position(
            |packet| matches!(packet, ServerPacket::GainedItem(item) if item.item.unique_id == sword),
        )
        .expect("乙应收到甲的 S.GainedItem");
    let b_chat = b_packets
        .iter()
        .position(
            |packet| matches!(packet, ServerPacket::Chat(chat) if chat.message == zh(mir_core::TextKey::TradeSuccess)),
        )
        .expect("乙应收到「交易成功」");
    let b_confirm = b_packets
        .iter()
        .position(|packet| matches!(packet, ServerPacket::TradeConfirm(_)))
        .expect("乙应收到 S.TradeConfirm");
    // 乙在甲确认时还收到过「甲 正在等待你确认交易。」，那条排在物品之前。
    assert!(
        b_gained < b_chat && b_chat < b_confirm,
        "乙的包序：物品→聊天→成交：{b_packets:?}"
    );
}

#[test]
fn changing_the_trade_resets_both_preparations() {
    let mut harness = Harness::new(18);
    let (a, b) = harness.duet();
    harness.put_item(a, 6, SWORD);
    harness.put_item(b, 6, AXE);
    harness.start_trade(a, b);
    harness.world.trade_deposit_item(a, 6, 0).expect("甲放入");

    // 甲确认 → 只有甲锁定。
    harness.world.trade_confirm(a, true).expect("确认");
    assert!(harness.world.trade_state(a).expect("甲").locked);

    // 乙放入物品 → `TradeItem()` → `TradeUnlock()`：**双方**解锁（`:10761-10787`）。
    harness.world.trade_deposit_item(b, 6, 0).expect("乙放入");
    assert!(!harness.world.trade_state(a).expect("甲").locked);
    assert!(!harness.world.trade_state(b).expect("乙").locked);

    // 再确认一次，然后乙放金币 → `TradeGold()` 里的 `TradeUnlock()`（`:10746`）再次解锁。
    harness.world.trade_confirm(a, true).expect("再确认");
    assert!(harness.world.trade_state(a).expect("甲").locked);
    harness.world.trade_gold(b, 100).expect("放金币");
    assert!(!harness.world.trade_state(a).expect("甲").locked);
    assert!(!harness.world.trade_state(b).expect("乙").locked);
}

#[test]
fn full_bag_makes_the_trade_fail_without_exchanging_anything() {
    let mut harness = Harness::new(19);
    let (a, b) = harness.duet();
    let sword = harness.put_item(a, 6, SWORD);
    harness.put_item(b, 6, AXE);
    harness.start_trade(a, b);
    harness.world.trade_deposit_item(a, 6, 0).expect("甲放入");
    harness.world.trade_deposit_item(b, 6, 0).expect("乙放入");
    // 乙的背包塞满：收不下甲的剑。
    harness.fill_inventory(b);
    assert_eq!(harness.inventory_count(b), items::INVENTORY_SIZE);
    harness.clear(a);
    harness.clear(b);

    harness.world.trade_confirm(a, true).expect("甲确认");
    harness.world.trade_confirm(b, true).expect("乙确认");

    let a_packets = harness.packets(a);
    let b_packets = harness.packets(b);
    // Crystal：只发 `S.TradeCancel{Unlock = true}`，**不关窗、不退货**（`:10829-10839`）。
    for (player, packets) in [(a, &a_packets), (b, &b_packets)] {
        assert!(
            packets.iter().any(|packet| matches!(
                packet,
                ServerPacket::TradeCancel(cancel) if cancel.unlock
            )),
            "玩家 {player} 应收到 S.TradeCancel{{Unlock=true}}，实际 {packets:?}"
        );
        assert!(
            !packets
                .iter()
                .any(|packet| matches!(packet, ServerPacket::TradeConfirm(_))),
            "成交应当失败"
        );
        // Crystal 只给客户端发 `S.TradeCancel{Unlock=true}`（客户端据此解锁），
        // 服务端**不改** `TradeLocked`（`PlayerObject.cs:10829-10851` 里没有赋值）。
        assert!(harness.world.trade_state(player).expect("玩家").locked);
        assert_eq!(
            harness.world.trade_partner(player),
            Some(if player == a { b } else { a })
        );
    }
    // 物品都还在各自交易格里，谁的背包都没变。
    assert_eq!(
        harness.world.trade_state(a).expect("甲").items[0]
            .as_ref()
            .map(|item| item.unique_id),
        Some(sword)
    );
    assert_eq!(harness.inventory_count(a), 0);
    assert_eq!(harness.inventory_count(b), items::INVENTORY_SIZE);
    assert_eq!(
        system_chats(&a_packets),
        vec!["你的交易对方无法接受全部物品。"]
    );
    assert_eq!(
        system_chats(&b_packets),
        vec![
            zh_args(mir_core::TextKey::TradePartnerWaiting, &[("name", "甲")]),
            zh(mir_core::TextKey::TradeItemsOverflow),
        ]
    );
}

// ---------------------------------------------------------------------------
// 6. 取消与中止
// ---------------------------------------------------------------------------

#[test]
fn cancelling_returns_items_and_gold_to_the_owner() {
    let mut harness = Harness::new(20);
    let (a, b) = harness.duet();
    let sword = harness.put_item(a, 6, SWORD);
    harness.start_trade(a, b);
    harness.world.trade_deposit_item(a, 6, 0).expect("放入");
    harness.world.trade_gold(b, 300).expect("乙放金币");
    harness.clear(a);
    harness.clear(b);

    harness.world.trade_cancel(a);

    // 物品回到背包（`RetrieveTradeItem(t, i)`，`:10927`）。
    assert_eq!(harness.inventory_uids(a), vec![sword]);
    assert_eq!(harness.gold(b), 1_000);
    let a_packets = harness.packets(a);
    let b_packets = harness.packets(b);
    for (player, packets) in [(a, &a_packets), (b, &b_packets)] {
        assert!(
            packets.iter().any(|packet| matches!(
                packet,
                ServerPacket::TradeCancel(cancel) if !cancel.unlock
            )),
            "玩家 {player} 应收到 S.TradeCancel{{Unlock=false}}，实际 {packets:?}"
        );
        assert_eq!(harness.world.trade_partner(player), None);
    }
    assert!(
        a_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::RetrieveTradeItem(ack) if ack.success && ack.from == 0
        )),
        "甲应收到物品退回回执，实际 {a_packets:?}"
    );
}

#[test]
fn cancelling_with_a_full_bag_mails_the_item_instead() {
    let mut harness = Harness::new(21);
    let (a, b) = harness.duet();
    let sword = harness.put_item(a, 6, SWORD);
    harness.start_trade(a, b);
    harness.world.trade_deposit_item(a, 6, 0).expect("放入");
    // 背包塞满 → `FreeSpace(Inventory) < 1` → `GainItemMail(temp, 1)`（`:10910-10918`）。
    harness.fill_inventory(a);
    harness.clear(a);
    harness.clear(b);

    harness.world.trade_cancel(a);

    let mailbox = harness.world.mailbox(a).expect("邮箱");
    assert_eq!(mailbox.len(), 1, "物品应改成系统邮件");
    assert_eq!(mailbox[0].items.len(), 1);
    assert_eq!(mailbox[0].items[0].unique_id, sword);
    assert!(
        !mailbox[0].collected,
        "默认 MailAutoSendItems=false → 包裹未受理"
    );
    assert!(harness.world.trade_state(a).expect("甲").items[0].is_none());
    let a_packets = harness.packets(a);
    assert!(
        a_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::DeleteItem(item) if item.unique_id == sword
        )),
        "应收到 S.DeleteItem 把物品从交易格移除，实际 {a_packets:?}"
    );
}

#[test]
fn logging_out_cancels_the_trade() {
    let mut harness = Harness::new(22);
    let (a, b) = harness.duet();
    let sword = harness.put_item(a, 6, SWORD);
    harness.start_trade(a, b);
    harness.world.trade_deposit_item(a, 6, 0).expect("放入");
    harness.clear(a);
    harness.clear(b);

    // `TradeCancel();`（`PlayerObject.cs:407`，StopGame 里）。
    let leaver = harness.world.leave(a).expect("甲离开世界");
    assert_eq!(harness.world.trade_partner(b), None);
    // 离开的人拿回自己的东西（交易状态在 `Player` 上也已清空）。
    // 退回时用**第一个空背包格**（`PlayerObject.cs:10920-10927`），所以落在 0 号格。
    assert_eq!(leaver.trade.partner, None);
    let mut ids: Vec<u64> = leaver
        .inventory
        .iter()
        .flatten()
        .map(|item| item.unique_id)
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![sword]);
    let b_packets = harness.packets(b);
    assert!(
        b_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::TradeCancel(_))),
        "留下来的乙应收到 S.TradeCancel，实际 {b_packets:?}"
    );
}

#[test]
fn walking_away_cancels_the_trade() {
    let mut harness = Harness::new(23);
    let (a, b) = harness.duet();
    harness.start_trade(a, b);
    harness.clear(a);
    harness.clear(b);

    // `Moved()`（`PlayerObject.cs:568-574`）→ `TradeCancel()`。
    harness.world.advance_clock(600);
    harness
        .world
        .walk(b, MirDirection::Right)
        .expect("乙走一步");

    assert_eq!(harness.world.trade_partner(a), None);
    assert_eq!(harness.world.trade_partner(b), None);
    let a_packets = harness.packets(a);
    assert!(
        a_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::TradeCancel(_))),
        "甲应收到 S.TradeCancel，实际 {a_packets:?}"
    );
}

#[test]
fn turning_around_cancels_the_trade() {
    let mut harness = Harness::new(24);
    let (a, b) = harness.duet();
    harness.start_trade(a, b);
    harness.clear(a);
    harness.clear(b);

    // `if (TradePartner != null) TradeCancel();`（`PlayerObject.cs:4300-4301`，Turn 里）。
    harness.world.turn(a, MirDirection::Up).expect("转身");

    assert_eq!(harness.world.trade_partner(a), None);
    assert_eq!(harness.world.trade_partner(b), None);
    let b_packets = harness.packets(b);
    assert!(
        b_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::TradeCancel(_))),
        "乙应收到 S.TradeCancel，实际 {b_packets:?}"
    );
}

#[test]
fn changing_map_cancels_the_trade() {
    let mut harness = Harness::new(25);
    let (a, b) = harness.duet();
    harness.start_trade(a, b);
    harness.clear(a);
    harness.clear(b);

    // `if (TradePartner != null) TradeCancel();`（`PlayerObject.cs:1477`，Teleport 里）。
    harness
        .world
        .change_map(a, MAP, Point::new(5, 5), MirDirection::Down)
        .expect("同图传送");

    assert_eq!(harness.world.trade_partner(a), None);
    assert_eq!(harness.world.trade_partner(b), None);
}

#[test]
fn dying_cancels_the_trade() {
    let mut harness = Harness::new(26);
    let (a, b) = harness.duet();
    let sword = harness.put_item(b, 6, SWORD);
    harness.start_trade(a, b);
    harness.world.trade_deposit_item(b, 6, 0).expect("乙放入");
    harness.clear(a);
    harness.clear(b);

    // 让甲一刀打死乙（本作扩展：死亡即中止交易，见 `docs/world/trade.md` 差异 3）。
    let lethal = CombatStats {
        hp: 100,
        min_dc: 100,
        max_dc: 100,
        accuracy: 100,
        ..CombatStats::default()
    };
    if let Some(player) = harness.world.player_mut(a) {
        player.stats = lethal;
        player.attack_mode = AttackMode::All;
    }
    if let Some(player) = harness.world.player_mut(b) {
        player.hp = 1;
        player.stats.min_ac = 0;
        player.stats.max_ac = 0;
        player.stats.agility = 0;
        player.stats.critical_rate = 0;
        player.stats.luck = 0;
    }
    harness
        .world
        .attack(a, MirDirection::Right)
        .expect("攻击")
        .expect("有目标");
    harness.world.advance_clock(300);
    harness.world.tick();

    assert!(harness.world.player(b).expect("乙").dead, "乙应该死亡");
    assert_eq!(harness.world.trade_partner(a), None);
    assert_eq!(harness.world.trade_partner(b), None);
    // 物品退回乙的背包（死亡掉落只处理背包/装备，交易格里的不会掉）。
    assert!(
        harness.inventory_uids(b).contains(&sword),
        "交易格里的物品应退回背包"
    );
    let a_packets = harness.packets(a);
    assert!(
        a_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::TradeCancel(_))),
        "甲应收到 S.TradeCancel，实际 {a_packets:?}"
    );
}

// ---------------------------------------------------------------------------
// 7. 守恒与幂等
// ---------------------------------------------------------------------------

#[test]
fn item_counts_and_unique_ids_are_conserved_by_the_swap() {
    let mut harness = Harness::new(27);
    let (a, b) = harness.duet();
    let sword = harness.put_item(a, 6, SWORD);
    let axe = harness.put_item(b, 6, AXE);
    let before_a = harness.inventory_uids(a);
    let before_b = harness.inventory_uids(b);
    harness.start_trade(a, b);
    harness.world.trade_deposit_item(a, 6, 0).expect("甲放入");
    harness.world.trade_deposit_item(b, 6, 0).expect("乙放入");
    harness.world.trade_confirm(a, true).expect("甲确认");
    harness.world.trade_confirm(b, true).expect("乙确认成交");

    // 总数不变：2 件还是 2 件，UUID 集合也不变（只是换了主人）。
    assert_eq!(harness.inventory_count(a) + harness.inventory_count(b), 2);
    let mut after: Vec<u64> = harness.inventory_uids(a);
    after.extend(harness.inventory_uids(b));
    let mut before: Vec<u64> = before_a;
    before.extend(before_b);
    after.sort_unstable();
    before.sort_unstable();
    assert_eq!(after, before, "物品 UUID 必须守恒");
    assert_eq!(after, vec![sword.min(axe), sword.max(axe)]);
    // 交易格全部清空。
    for player in [a, b] {
        assert!(
            harness
                .world
                .trade_state(player)
                .expect("玩家")
                .items
                .iter()
                .all(Option::is_none)
        );
    }
}

#[test]
fn cancelling_twice_is_idempotent() {
    let mut harness = Harness::new(28);
    let (a, b) = harness.duet();
    harness.put_item(a, 6, SWORD);
    harness.start_trade(a, b);
    harness.world.trade_deposit_item(a, 6, 0).expect("放入");

    harness.world.trade_cancel(a);
    // 再取消一次：没有交易对象，什么都不该发生（`if (TradePartner == null) return;`）。
    harness.clear(a);
    harness.clear(b);
    harness.world.trade_cancel(a);
    assert!(harness.packets(a).is_empty());
    assert!(harness.packets(b).is_empty());
    assert_eq!(harness.world.trade_state(a).expect("甲").gold, 0);
}
