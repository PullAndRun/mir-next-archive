//! 组队（party）的端到端测试。
//!
//! 全部确定性：
//! * 世界随机数固定种子（[`World::seed_rng`]）；
//! * 时间用 [`World::advance_clock`] 快进（不 sleep），邀请冷却/邀请有效期都由它驱动；
//! * 地形是内存里合成的 60×60 空地（[`MapCell::default`] 可走），
//!   地图标题/`BigMap` 在内存数据库里构造。
//!
//! 每个断言尽量对应 Crystal 的一处行为，注释里写了出处（`文件.cs:行号`）。
//! 关键源码：`Server/MirObjects/PlayerObject.cs` 的 `#region Groups`（`:9215-9533`）、
//! 组队聊天（`:2001-2013`）与 `WinExp`（`:794-845`）。

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{ChatType, MirClass, MirDirection, MirGender};
use mir_db::{MapInfo, MirDb};
use mir_protocol::ServerPacket;
use mir_server::gamedata::GameData;
use mir_server::{
    GROUP_EXP_RANGE, GROUP_INVITE_DELAY_MS, GROUP_INVITE_EXPIRE_MS, GroupError, GroupJoinOutcome,
    MAX_GROUP, NpcState, Player, World, combat,
};
use mir_world::{Map, MapCell, MapVersion};
use tokio::sync::mpsc;

/// 地图 A（默认所在地图，`BigMap = 9`）。
const MAP_A: &str = "0";
/// 地图 B（切图目标，`BigMap = 0`）。
const MAP_B: &str = "1";
/// 地图 A 的数据库主键。
const INDEX_A: i32 = 10;
/// 地图 B 的数据库主键。
const INDEX_B: i32 = 20;

// ---------------------------------------------------------------------------
// 脚手架
// ---------------------------------------------------------------------------

/// 每张图 60×60 的可走空地。
fn synthetic_map(name: &str) -> Arc<Map> {
    let width = 60;
    let height = 60;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: name.to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
    .into()
}

/// 最小数据库：两张地图（带标题，A 图有大地图），没有 NPC/刷怪点。
fn base_db() -> MirDb {
    MirDb {
        maps: vec![
            MapInfo {
                index: INDEX_A,
                file_name: MAP_A.to_owned(),
                title: "比奇省".to_owned(),
                big_map: 9,
                ..MapInfo::default()
            },
            MapInfo {
                index: INDEX_B,
                file_name: MAP_B.to_owned(),
                title: "边界村".to_owned(),
                ..MapInfo::default()
            },
        ],
        ..MirDb::default()
    }
}

/// 测试世界 + 每个玩家的收包通道。
struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Harness {
    fn new(seed: u64) -> Self {
        let mut world = World::new(999);
        world.seed_rng(seed);
        world.insert_map(MAP_A, synthetic_map(MAP_A));
        world.insert_map(MAP_B, synthetic_map(MAP_B));
        world.set_game_data(Arc::new(GameData::from_db(base_db(), None)));
        Self {
            world,
            receivers: HashMap::new(),
        }
    }

    /// 放一个 1 级战士进世界，返回对象 ID。
    fn add_player(&mut self, object_id: u32, name: &str, map: &str, position: Point) -> u32 {
        let stats = combat::player_base_stats(1, MirClass::Warrior);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.enter(Player {
            object_id,
            session_id: u64::from(object_id),
            character_id: i64::from(object_id),
            name: name.to_owned(),
            map: map.to_owned(),
            position,
            direction: MirDirection::Down,
            level: 1,
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
            // 关掉自然回血，免得 10 秒一次的回复混进血量断言（本文件不断言血量，但保持一致）。
            regen_time: i64::MAX,
            death_time: 0,
            revive_time: 0,
            cell_time: 0,
            step_counter: 0,
            run_counter: 0,
            run_time: 0,
            dead: false,
            visible_monsters: BTreeSet::new(),
            visible_npcs: BTreeSet::new(),
            movement_time: 0,
            inventory: mir_server::items::new_inventory(),
            equipment: mir_server::items::new_equipment(),
            storage: mir_server::storage::new_storage(),
            magics: Vec::new(),
            magic: mir_server::MagicState::default(),
            gold: 0,
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
            sender,
        });
        self.receivers.insert(object_id, receiver);
        object_id
    }

    fn name_of(&self, object_id: u32) -> String {
        self.world
            .player(object_id)
            .map_or_else(|| panic!("玩家 {object_id} 不在世界里"), |p| p.name.clone())
    }

    /// 取走某个玩家当前收到的全部包。
    fn packets(&mut self, object_id: u32) -> Vec<ServerPacket> {
        let receiver = self.receivers.get_mut(&object_id).expect("玩家通道");
        let mut packets = Vec::new();
        while let Ok(frame) = receiver.try_recv() {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
        }
        packets
    }

    /// 丢掉某个玩家已经收到的包。
    fn clear(&mut self, object_id: u32) {
        let _ = self.packets(object_id);
    }

    /// 走到邀请冷却之后，再邀请一个人（`Settings.GroupInviteDelay = 2000`，`PlayerObject.cs:9276`）。
    fn invite(&mut self, inviter: u32, invitee: u32) {
        self.world.advance_clock(GROUP_INVITE_DELAY_MS);
        let name = self.name_of(invitee);
        self.world
            .group_invite(inviter, &name)
            .unwrap_or_else(|error| panic!("邀请 {name} 应成功：{error}"));
    }

    /// 让被邀请者接受，返回队伍 ID。
    fn accept(&mut self, invitee: u32) -> u32 {
        match self
            .world
            .group_join(invitee, true)
            .expect("接受邀请应成功")
        {
            GroupJoinOutcome::Joined(group_id) => group_id,
            GroupJoinOutcome::Declined => panic!("不该走到拒绝分支"),
        }
    }

    /// 组一支队伍（第一个人当队长），返回队伍 ID。
    fn form_group(&mut self, members: &[u32]) -> u32 {
        let (leader, rest) = members.split_first().expect("至少一个人");
        assert!(!rest.is_empty(), "至少两个人才能组队");
        let mut group_id = None;
        for member in rest {
            self.invite(*leader, *member);
            group_id = Some(self.accept(*member));
        }
        group_id.expect("至少两人")
    }
}

/// 某个玩家的 `ObjectChat`。
fn chats(packets: &[ServerPacket]) -> Vec<(&str, ChatType)> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectChat(chat) => Some((chat.text.as_str(), chat.type_)),
            _ => None,
        })
        .collect()
}

/// 某个玩家的系统提示（`Chat` 包）。
fn system_chats(packets: &[ServerPacket]) -> Vec<&str> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::Chat(chat) if chat.type_ == ChatType::System => {
                Some(chat.message.as_str())
            }
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. 邀请
// ---------------------------------------------------------------------------

#[test]
fn invite_sends_group_invite_to_target_and_switch_group_to_inviter() {
    let mut harness = Harness::new(1);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(13, 10));
    harness.clear(alice);
    harness.clear(bob);

    // 名字大小写不敏感（Crystal `Envir.GetPlayer` 用 `OrdinalIgnoreCase`，`Envir.cs:4696-4703`）。
    assert_eq!(harness.world.group_invite(alice, "bob"), Ok(bob));

    // 邀请者：`SwitchGroup(true)`（`PlayerObject.cs:9335` → `:9219`）。
    assert!(
        harness
            .packets(alice)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::SwitchGroup(s) if s.allow_group)),
        "邀请者应收到 S.SwitchGroup{{true}}"
    );
    // 被邀请者：`S.GroupInvite{Name = 邀请者名}`（`:9336`）。
    let packets = harness.packets(bob);
    assert!(
        packets.iter().any(
            |packet| matches!(packet, ServerPacket::GroupInvite(invite) if invite.name == "阿甲")
        ),
        "被邀请者应收到 S.GroupInvite{{阿甲}}，实际 {packets:?}"
    );
    // 还没组队：只是记下了邀请（`player.GroupInvitation = this`，`:9337`）。
    assert!(!harness.world.in_group(alice), "接受之前不算组队");
    assert!(!harness.world.in_group(bob));
    let invitation = harness.world.group_invitation_of(bob).expect("有邀请");
    assert_eq!(invitation.inviter, alice);
    assert_eq!(harness.world.group_members(alice), &[] as &[u32]);
}

#[test]
fn invite_unknown_name_is_rejected_with_a_hint() {
    let mut harness = Harness::new(2);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    harness.clear(alice);

    // `if (player == null) ReceiveChat(CouldNotBeFound);`（`PlayerObject.cs:9299-9303`）。
    assert_eq!(
        harness.world.group_invite(alice, "查无此人"),
        Err(GroupError::NotFound("查无此人".to_owned()))
    );
    let packets = harness.packets(alice);
    let hints = system_chats(&packets);
    assert!(
        hints.iter().any(|hint| hint.contains("查无此人")),
        "应提示找不到该玩家，实际 {hints:?}"
    );
}

#[test]
fn invite_self_is_rejected() {
    let mut harness = Harness::new(3);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));

    assert_eq!(
        harness.world.group_invite(alice, "阿甲"),
        Err(GroupError::CannotInviteSelf)
    );
}

#[test]
fn invite_cooldown_is_silent_and_does_not_reach_the_target() {
    let mut harness = Harness::new(4);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    // Bob 只是用来顶住第一次邀请，让第二次邀请撞上冷却。
    let _bob = harness.add_player(2, "Bob", MAP_A, Point::new(13, 10));
    let carol = harness.add_player(3, "Carol", MAP_A, Point::new(14, 10));
    // 第一次邀请成功，同时把 `NextGroupInviteTime` 推到 2000ms 之后。
    assert!(harness.world.group_invite(alice, "Bob").is_ok());
    harness.clear(alice);
    harness.clear(carol);

    // `if (Envir.Time < NextGroupInviteTime) return;`（`:9275`）：静默失败，连提示都没有。
    assert_eq!(
        harness.world.group_invite(alice, "Carol"),
        Err(GroupError::InviteCooldown)
    );
    assert!(harness.packets(alice).is_empty(), "冷却中不应有任何回包");
    assert!(
        !harness
            .packets(carol)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::GroupInvite(_))),
        "冷却中的邀请不应发给目标"
    );

    // 过了 `Settings.GroupInviteDelay` 就能再邀请。
    harness.world.advance_clock(GROUP_INVITE_DELAY_MS);
    assert_eq!(harness.world.group_invite(alice, "Carol"), Ok(carol));
}

#[test]
fn invite_target_already_in_a_group_is_rejected() {
    let mut harness = Harness::new(5);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(13, 10));
    let carol = harness.add_player(3, "Carol", MAP_A, Point::new(14, 10));
    harness.form_group(&[alice, bob]);
    harness.clear(bob);
    harness.clear(carol);

    // `if (player.GroupMembers != null) AlreadyInAnotherGroup;`（`:9316-9320`）。
    assert_eq!(
        harness.world.group_invite(carol, "Bob"),
        Err(GroupError::TargetAlreadyInGroup("Bob".to_owned()))
    );
    assert!(
        !harness
            .packets(bob)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::GroupInvite(_))),
        "已在队伍里的玩家不该再收到邀请"
    );
}

#[test]
fn invite_group_full_is_rejected() {
    let mut harness = Harness::new(6);
    let mut ids = Vec::new();
    for index in 1..=u32::try_from(MAX_GROUP).expect("小数字") {
        ids.push(harness.add_player(
            index,
            &format!("玩家{index}"),
            MAP_A,
            Point::new(10 + i32::try_from(index).unwrap_or(0), 10),
        ));
    }
    let extra = harness.add_player(100, "多余的人", MAP_A, Point::new(40, 40));
    harness.form_group(&ids);
    assert_eq!(harness.world.group_members(ids[0]).len(), MAX_GROUP);

    harness.world.advance_clock(GROUP_INVITE_DELAY_MS);
    // `if (GroupMembers.Count >= Globals.MaxGroup) YouGroupMaxMembers;`（`:9284-9288`）。
    assert_eq!(
        harness.world.group_invite(ids[0], "多余的人"),
        Err(GroupError::GroupFull)
    );
    assert!(!harness.world.in_group(extra));
}

#[test]
fn invite_expires_after_the_extension_timeout() {
    let mut harness = Harness::new(7);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(13, 10));
    harness.invite(alice, bob);

    // 本作扩展：邀请 4 秒有效（Crystal 没有超时，见 `GROUP_INVITE_EXPIRE_MS`）。
    harness.world.advance_clock(GROUP_INVITE_EXPIRE_MS + 1);
    harness.clear(bob);
    assert_eq!(
        harness.world.group_join(bob, true),
        Err(GroupError::InvitationExpired)
    );
    assert!(!harness.world.in_group(bob), "过期邀请不能建队");
    assert!(!harness.world.in_group(alice));
    assert!(harness.world.group_invitation_of(bob).is_none());
    let packets = harness.packets(bob);
    let hints = system_chats(&packets);
    assert!(
        hints.iter().any(|hint| hint.contains("过期")),
        "应提示邀请过期，实际 {hints:?}"
    );
}

#[test]
fn inviter_logging_out_makes_the_invitation_stale() {
    let mut harness = Harness::new(23);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(13, 10));
    harness.invite(alice, bob);

    // 邀请者下线：Crystal 在 `Process` 里靠 `GroupInvitation.Node == null` 清掉（`:480-481`），
    // 本作在接受时判定为 `InviterOffline`。
    assert!(harness.world.leave(alice).is_some());
    assert_eq!(
        harness.world.group_join(bob, true),
        Err(GroupError::InviterOffline)
    );
    assert!(!harness.world.in_group(bob));
    assert!(
        harness.world.group_invitation_of(bob).is_none(),
        "失效邀请已清掉"
    );
}

// ---------------------------------------------------------------------------
// 2. 接受 / 拒绝
// ---------------------------------------------------------------------------

#[test]
fn accept_creates_the_group_and_pushes_the_member_list() {
    let mut harness = Harness::new(8);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(13, 10));
    harness.invite(alice, bob);
    harness.clear(alice);
    harness.clear(bob);

    let group_id = harness.accept(bob);

    // 队伍：队长是邀请者，接受者追加在后面（`GroupMembers.Add(this)`，`:9468`）。
    assert_eq!(harness.world.group_members(alice), &[alice, bob]);
    assert_eq!(harness.world.group_id_of(bob), Some(group_id));
    assert_eq!(
        harness.world.group(alice).expect("有队伍").leader(),
        Some(alice)
    );

    // 队长：`S.AddMember{Bob}`（`:9444`）与 `S.GroupMembersMap{Bob, 地图标题}`（`:9494`）。
    let alice_packets = harness.packets(alice);
    assert!(
        alice_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::AddMember(m) if m.name == "Bob")),
        "队长应收到 AddMember{{Bob}}，实际 {alice_packets:?}"
    );
    assert!(
        alice_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::GroupMembersMap(m) if m.player_name == "Bob" && m.player_map == "比奇省"
        )),
        "队长应收到队员所在地图（`CurrentMap.Info.Title`），实际 {alice_packets:?}"
    );
    // 同图近距离：双方互发一条 `S.ObjectHealth`（`:9447-9452`）。
    assert!(
        alice_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectHealth(h) if h.object_id == bob)),
        "队长应收到接受者的血条，实际 {alice_packets:?}"
    );

    // 接受者：自己的名字收到一次（`:9473` 的 `Enqueue(p)`；循环里发的是**老队员**的名字）。
    let bob_packets = harness.packets(bob);
    let own_entries = bob_packets
        .iter()
        .filter(|packet| matches!(packet, ServerPacket::AddMember(m) if m.name == "Bob"))
        .count();
    assert_eq!(own_entries, 1, "接受者收到一条自己的 AddMember（`:9473`）");
    assert!(
        bob_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::AddMember(m) if m.name == "阿甲")),
        "接受者应收到队长的 AddMember，实际 {bob_packets:?}"
    );
    assert!(
        bob_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::GroupMembersMap(m) if m.player_name == "阿甲" && m.player_map == "比奇省"
        )),
        "接受者应收到队长的所在地图，实际 {bob_packets:?}"
    );
    assert!(
        bob_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectHealth(h) if h.object_id == alice)),
        "接受者应收到队长的血条，实际 {bob_packets:?}"
    );
    // `BigMap > 0` 的地图才会发成员坐标（`HumanObject.cs:2735`）。
    assert!(
        bob_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::SendMemberLocation(l) if l.member_name == "阿甲"
        )),
        "接受者应收到队长的坐标，实际 {bob_packets:?}"
    );
}

#[test]
fn decline_notifies_the_inviter_without_creating_a_group() {
    let mut harness = Harness::new(9);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(13, 10));
    harness.invite(alice, bob);
    harness.clear(alice);
    harness.clear(bob);

    // `if (!accept) { GroupInvitation.ReceiveChat(...DeclinedGroupInvite...); }`（`:9380-9385`）。
    assert_eq!(
        harness.world.group_join(bob, false),
        Ok(GroupJoinOutcome::Declined)
    );
    assert!(!harness.world.in_group(bob));
    assert!(!harness.world.in_group(alice));
    assert!(
        harness.world.group_invitation_of(bob).is_none(),
        "邀请已清掉"
    );
    let packets = harness.packets(alice);
    let hints = system_chats(&packets);
    assert!(
        hints.iter().any(|hint| hint.contains("拒绝")),
        "邀请者应收到拒绝提示，实际 {hints:?}"
    );
    assert!(
        !harness.packets(bob).iter().any(|packet| matches!(
            packet,
            ServerPacket::AddMember(_) | ServerPacket::DeleteGroup(_)
        )),
        "拒绝者不该收到组队列表相关的包"
    );
}

#[test]
fn accepting_without_an_invitation_is_rejected() {
    let mut harness = Harness::new(10);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    harness.clear(alice);

    // `if (GroupInvitation == null) NotInvitedToGroup;`（`:9374-9378`）。
    assert_eq!(
        harness.world.group_join(alice, true),
        Err(GroupError::NoInvitation(alice))
    );
    let packets = harness.packets(alice);
    let hints = system_chats(&packets);
    assert!(
        hints.iter().any(|hint| hint.contains("没有收到")),
        "应提示没有邀请，实际 {hints:?}"
    );
}

// ---------------------------------------------------------------------------
// 3. 退出 / 踢人
// ---------------------------------------------------------------------------

#[test]
fn leaving_notifies_the_remaining_members_with_delete_member() {
    let mut harness = Harness::new(11);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(13, 10));
    let carol = harness.add_player(3, "Carol", MAP_A, Point::new(14, 10));
    harness.form_group(&[alice, bob, carol]);
    harness.clear(alice);
    harness.clear(bob);
    harness.clear(carol);

    // 注意：Crystal 的 `C.DelMember` **只允许队长**操作（`:9347-9351`），非队长想退队只能
    // 关掉组队开关或下线；这里让队长把自己移出队伍，走的就是 `DelMember` + `LeaveGroup`。
    assert_eq!(harness.world.group_kick(alice, "阿甲"), Ok(alice));

    // 剩下 2 人：每人一条 `S.DeleteMember{阿甲}`（`:9240-9245`）。
    for member in [bob, carol] {
        let packets = harness.packets(member);
        assert!(
            packets
                .iter()
                .any(|packet| matches!(packet, ServerPacket::DeleteMember(m) if m.name == "阿甲")),
            "剩余成员应收到 DeleteMember{{阿甲}}，实际 {packets:?}"
        );
    }
    assert_eq!(harness.world.group_members(bob), &[bob, carol]);
    // 队长顺延给剩下成员里的第一个（Crystal 只看 `GroupMembers[0]`）。
    assert_eq!(
        harness.world.group(bob).expect("队伍还在").leader(),
        Some(bob)
    );
    assert!(!harness.world.in_group(alice));
    // 离队者自己收到 `S.DeleteGroup`（`DelMember` 先发一条 `:9368`，`LeaveGroup` 再发一条 `:9255`）。
    let alice_deletes = harness
        .packets(alice)
        .iter()
        .filter(|packet| matches!(packet, ServerPacket::DeleteGroup(_)))
        .count();
    assert_eq!(alice_deletes, 2, "Crystal 会发两条 DeleteGroup（照抄）");
}

#[test]
fn last_member_leaving_dissolves_the_group() {
    let mut harness = Harness::new(12);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(13, 10));
    harness.form_group(&[alice, bob]);
    harness.clear(alice);
    harness.clear(bob);

    // 2 人队里 Bob 用 `LeaveGroup` 退队（关组队开关/下线都会走到这里，`:9224-9226`、`:406`）
    // → 只剩 Alice 一人 → 队伍解散（`:9246-9251`）。
    assert!(harness.world.leave_group(bob));
    assert!(!harness.world.in_group(alice), "最后一人被清空队伍");
    assert!(harness.world.group(alice).is_none(), "队伍已解散");
    assert!(harness.world.group_members(bob).is_empty());
    let packets = harness.packets(alice);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::DeleteGroup(_))),
        "最后一人应收到 DeleteGroup，实际 {packets:?}"
    );
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::DeleteMember(_))),
        "只剩一人时发的是 DeleteGroup，不是 DeleteMember"
    );
}

#[test]
fn only_the_leader_can_remove_members() {
    let mut harness = Harness::new(13);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(13, 10));
    let carol = harness.add_player(3, "Carol", MAP_A, Point::new(14, 10));
    harness.form_group(&[alice, bob, carol]);

    // `if (GroupMembers[0] != this) NotGroupLeader;`（`:9347-9351`）。
    assert_eq!(
        harness.world.group_kick(bob, "Carol"),
        Err(GroupError::NotLeader(bob))
    );
    assert_eq!(harness.world.group_members(alice), &[alice, bob, carol]);

    // 队长踢人：Bob 出队，剩下 Alice/Carol 收到 `DeleteMember{Bob}`。
    harness.clear(carol);
    assert_eq!(harness.world.group_kick(alice, "Bob"), Ok(bob));
    assert_eq!(harness.world.group_members(alice), &[alice, carol]);
    assert!(
        harness
            .packets(carol)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::DeleteMember(m) if m.name == "Bob")),
        "剩下的队员应收到 DeleteMember{{Bob}}"
    );
}

#[test]
fn switching_the_group_flag_off_leaves_the_group() {
    let mut harness = Harness::new(14);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(13, 10));
    harness.form_group(&[alice, bob]);
    harness.clear(alice);
    harness.clear(bob);

    // `if (!AllowGroup && GroupMembers != null) LeaveGroup();`（`:9224-9226`）。
    assert_eq!(harness.world.switch_group(bob, false), Ok(true));
    assert!(!harness.world.in_group(bob));
    assert!(!harness.world.in_group(alice), "另一个人也被解散");
    let packets = harness.packets(bob);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::SwitchGroup(s) if !s.allow_group)),
        "应先回一条 SwitchGroup{{false}}，实际 {packets:?}"
    );
}

// ---------------------------------------------------------------------------
// 4. 组队聊天（`!!`）
// ---------------------------------------------------------------------------

#[test]
fn group_chat_reaches_members_only() {
    let mut harness = Harness::new(15);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(13, 10));
    let carol = harness.add_player(3, "Carol", MAP_A, Point::new(14, 10));
    harness.form_group(&[alice, bob]);
    harness.clear(alice);
    harness.clear(bob);
    harness.clear(carol);

    // `if (message.StartsWith("!!"))`（`PlayerObject.cs:2001-2013`）。
    assert_eq!(harness.world.chat(alice, "!!你好"), Ok(2));

    for member in [alice, bob] {
        let packets = harness.packets(member);
        let messages = chats(&packets);
        assert_eq!(
            messages,
            vec![("阿甲: 你好", ChatType::Group)],
            "队员应收到一条组队聊天（名字前缀由服务端拼）"
        );
    }
    let outsider = harness.packets(carol);
    assert!(
        chats(&outsider).is_empty(),
        "同图第三者不该收到组队聊天，实际 {outsider:?}"
    );
}

#[test]
fn group_chat_outside_a_group_is_dropped_silently() {
    let mut harness = Harness::new(16);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(13, 10));
    harness.form_group(&[alice, bob]);
    let carol = harness.add_player(3, "Carol", MAP_A, Point::new(14, 10));
    harness.clear(alice);
    harness.clear(bob);
    harness.clear(carol);

    // `if (GroupMembers == null) return;`（`:2003`）：Crystal 既不报错也不广播。
    assert_eq!(harness.world.chat(carol, "!!有人吗"), Ok(0));
    for player in [alice, bob, carol] {
        let packets = harness.packets(player);
        assert!(
            chats(&packets).is_empty(),
            "组外 `!!` 不应产生任何聊天包，实际 {packets:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 5. 经验共享（`PlayerObject.WinExp`，`:794-845`）
// ---------------------------------------------------------------------------

#[test]
fn nearby_group_members_share_experience_with_the_party_bonus() {
    let mut harness = Harness::new(17);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(13, 10));
    harness.form_group(&[alice, bob]);
    harness.clear(alice);
    harness.clear(bob);

    // 两级都是 1 级、同图同屏（距离 3 ≤ `Globals.DataRange`）：
    // nearCount = 2 → partyExpRate[1] = 1.3；sumLevel = 2；
    // 每人 = 100 * 1.3 * 1 / 2 = 65（`:805`、`:822-830`）。
    harness.world.win_experience(alice, 100, 1);

    for member in [alice, bob] {
        let packets = harness.packets(member);
        assert!(
            packets.iter().any(
                |packet| matches!(packet, ServerPacket::GainExperience(gain) if gain.amount == 65)
            ),
            "队员应收到 S.GainExperience{{65}}，实际 {packets:?}"
        );
        assert_eq!(harness.world.player(member).unwrap().experience, 65);
    }
}

#[test]
fn distant_group_members_do_not_share_experience() {
    let mut harness = Harness::new(18);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    // 距离 30 > `GROUP_EXP_RANGE = 16`。
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(10, 10 + GROUP_EXP_RANGE + 14));
    harness.form_group(&[alice, bob]);
    harness.clear(alice);
    harness.clear(bob);

    harness.world.win_experience(alice, 100, 1);

    // 只有击杀者自己在范围内：nearCount = 1 → rate 1.0，全给他。
    let packets = harness.packets(alice);
    assert!(
        packets.iter().any(
            |packet| matches!(packet, ServerPacket::GainExperience(gain) if gain.amount == 100)
        ),
        "范围内的击杀者应拿满 100，实际 {packets:?}"
    );
    let far = harness.packets(bob);
    assert!(
        !far.iter()
            .any(|packet| matches!(packet, ServerPacket::GainExperience(_))),
        "范围外的队员不该分到经验，实际 {far:?}"
    );
    assert_eq!(harness.world.player(bob).unwrap().experience, 0);
}

#[test]
fn dead_members_still_count_towards_the_party_size_crystal_quirk() {
    let mut harness = Harness::new(19);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(12, 10));
    harness.form_group(&[alice, bob]);
    if let Some(player) = harness.world.player_mut(bob) {
        player.dead = true;
    }
    harness.clear(alice);
    harness.clear(bob);

    // Crystal 的 `sumLevel/nearCount` 循环**不检查** `Dead`（`:815`），
    // 只有真正分经验时才过滤（`:828`）——于是死人占了一份额度：
    // nearCount = 2 → partyExpRate[1] = 1.3；sumLevel = 2；击杀者得 100 * 1.3 / 2 = 65，死者 0。
    harness.world.win_experience(alice, 100, 1);

    let packets = harness.packets(alice);
    assert!(
        packets.iter().any(
            |packet| matches!(packet, ServerPacket::GainExperience(gain) if gain.amount == 65)
        ),
        "击杀者按 2 人算但只自己拿：65，实际 {packets:?}"
    );
    let dead = harness.packets(bob);
    assert!(
        !dead
            .iter()
            .any(|packet| matches!(packet, ServerPacket::GainExperience(_))),
        "死亡队员不分经验"
    );
}

#[test]
fn off_map_members_count_for_the_party_size_but_receive_nothing() {
    let mut harness = Harness::new(20);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    // 在另一张图的同一坐标：距离为 0，但地图不同。
    let bob = harness.add_player(2, "Bob", MAP_B, Point::new(10, 10));
    harness.form_group(&[alice, bob]);
    harness.clear(alice);
    harness.clear(bob);

    // `sumLevel/nearCount` 只看坐标（`:815`），真正分配才比地图（`:827`）：
    // nearCount = 2 → partyExpRate[1] = 1.3；sumLevel = 2 → 击杀者 100 * 1.3 / 2 = 65。
    harness.world.win_experience(alice, 100, 1);

    let packets = harness.packets(alice);
    assert!(
        packets.iter().any(
            |packet| matches!(packet, ServerPacket::GainExperience(gain) if gain.amount == 65)
        ),
        "异图同坐标的队友会稀释队长经验（Crystal 原样行为），实际 {packets:?}"
    );
    assert!(
        !harness
            .packets(bob)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::GainExperience(_))),
        "异图队员分不到经验"
    );
}

// ---------------------------------------------------------------------------
// 6. 切图 / 下线
// ---------------------------------------------------------------------------

#[test]
fn member_map_change_notifies_the_group_and_keeps_it() {
    let mut harness = Harness::new(21);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(13, 10));
    harness.form_group(&[alice, bob]);
    harness.clear(alice);
    harness.clear(bob);

    harness
        .world
        .change_map(bob, MAP_B, Point::new(10, 10), MirDirection::Down)
        .expect("切图应成功");

    // 队友收到「Bob 现在在边界村」（`GroupMemberMapNameChanged`，`PlayerObject.cs:1568` → `:9494`）。
    let packets = harness.packets(alice);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::GroupMembersMap(m) if m.player_name == "Bob" && m.player_map == "边界村"
        )),
        "队友应收到切图通知，实际 {packets:?}"
    );
    // Bob 自己也收到队长的地图（`:9495`）。
    let bob_packets = harness.packets(bob);
    assert!(
        bob_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::GroupMembersMap(m) if m.player_name == "阿甲" && m.player_map == "比奇省"
        )),
        "切图者应收到队友所在地图，实际 {bob_packets:?}"
    );
    // 切图**保留**队伍。
    assert_eq!(harness.world.group_members(alice), &[alice, bob]);
    assert_eq!(harness.world.player(bob).unwrap().map, MAP_B);
}

#[test]
fn logging_out_removes_the_member_from_the_group() {
    let mut harness = Harness::new(22);
    let alice = harness.add_player(1, "阿甲", MAP_A, Point::new(10, 10));
    let bob = harness.add_player(2, "Bob", MAP_A, Point::new(13, 10));
    harness.form_group(&[alice, bob]);
    harness.clear(alice);

    // `PlayerObject.StopGame` → `LeaveGroup()`（`PlayerObject.cs:406`）：下线 = 退队。
    assert!(harness.world.leave(bob).is_some());
    assert!(!harness.world.in_group(bob));
    assert!(!harness.world.in_group(alice), "只剩一人时队伍解散");
    let packets = harness.packets(alice);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::DeleteGroup(_))),
        "留下的最后一人应收到 DeleteGroup，实际 {packets:?}"
    );
    assert!(
        packets.iter().any(
            |packet| matches!(packet, ServerPacket::ObjectRemove(remove) if remove.object_id == bob)
        ),
        "下线广播 ObjectRemove 先于组队清理"
    );
}
