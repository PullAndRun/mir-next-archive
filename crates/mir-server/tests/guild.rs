//! 行会（guild）的端到端测试。
//!
//! 全部确定性：
//! * 世界随机数固定种子（[`World::seed_rng`]）；
//! * 时间用 [`World::advance_clock`] 快进（不 sleep），行会经验的 10 秒广播节流由它驱动；
//! * 地形是内存里合成的 60×60 空地，地图/ NPC 在内存数据库里构造；
//! * 行会配置（等级/金币/经验表/人数表）在每个用例里显式构造，不依赖 `resource/`；
//! * 持久化用 [`MemoryStore`]（实现了 `AccountStore` 的行会四件套）：
//!   世界只产生 [`GuildWrite`] 队列，测试用 [`persist_guild_writes`] 把它写进
//!   `MemoryStore`，再读回来断言——等价于 PostgreSQL 路径的仓储语义
//!   （真实 Postgres 的往返见 `crates/mir-store/tests/store_roundtrip.rs` 与
//!   `crates/mir-server/tests/postgres_guild.rs`，后者需要 `MIR2_TEST_DATABASE_URL`）。
//!
//! 每个断言尽量对应 Crystal 的一处行为，注释里写了出处（`文件.cs:行号`）。关键源码：
//! `Server/MirObjects/GuildObject.cs`（行会运行期）、`Server/MirObjects/PlayerObject.cs`
//! 的 `#region Guilds`（`:9706-10060`）与行会聊天（`:2014-2024`）。

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{ChatType, MirClass, MirDirection, MirGender};
use mir_db::{MapInfo, MirDb, NpcInfo};
use mir_protocol::ServerPacket;
use mir_server::gamedata::GameData;
use mir_server::guild::persist_guild_writes;
use mir_server::npc::GUILD_CREATE_KEY;
use mir_server::{
    AccountStore, GuildError, GuildJoinOutcome, GuildMemberEdit, GuildSettings, GuildWrite,
    MemoryStore, NpcState, Player, World, combat,
};
use mir_world::{Map, MapCell, MapVersion};
use tokio::sync::mpsc;

/// 测试地图（文件名 = 数据库里的 `file_name`）。
const MAP: &str = "0";
/// 地图数据库主键。
const MAP_INDEX: i32 = 10;
/// 会长（测试里的第一个玩家）的等级：Crystal `Guild_RequiredLevel = 22`。
const LEADER_LEVEL: u16 = 30;
/// 新建行会的金币花费（`Settings.cs:1335`）。
const CREATION_GOLD: u32 = 1_000_000;

// ---------------------------------------------------------------------------
// 脚手架
// ---------------------------------------------------------------------------

/// 行会测试用的配置：等级 22、金币 1,000,000、倍率 1%、每级 1 点升级点。
///
/// 经验表只有两级，第 2 级是 `-1`（Crystal `Level-22=-1` 的「到此为止」写法），
/// 这样「加经验 → 升级 → 不能再升」三个状态都能断言。
fn test_settings() -> GuildSettings {
    GuildSettings {
        required_level: 22,
        creation_gold: CREATION_GOLD,
        exp_rate: 0.01,
        point_per_level: 1,
        experience_list: vec![100, 200, -1],
        member_cap_list: vec![5, 5, 10],
        // 本轮不测行会 buff，保持「没有可选增益」。
        buff_list: Vec::new(),
    }
}

/// 60×60 的可走空地。
fn synthetic_map() -> Arc<Map> {
    Map {
        width: 60,
        height: 60,
        cells: vec![MapCell::default(); 3600],
        file_name: MAP.to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
    .into()
}

/// 最小数据库：一张地图，没有 NPC/刷怪点。
fn base_db() -> MirDb {
    MirDb {
        maps: vec![MapInfo {
            index: MAP_INDEX,
            file_name: MAP.to_owned(),
            title: "比奇省".to_owned(),
            ..MapInfo::default()
        }],
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
        world.insert_map(MAP, synthetic_map());
        world.set_game_data(Arc::new(GameData::from_db(base_db(), None)));
        world.set_guild_settings(test_settings());
        Self {
            world,
            receivers: HashMap::new(),
        }
    }

    /// 放一个玩家进世界（`character_id` 与对象 ID 相同）。
    fn add_player(&mut self, object_id: u32, name: &str, level: u16, gold: i64) -> u32 {
        self.add_player_as(object_id, i64::from(object_id), name, level, gold)
    }

    /// 放一个指定角色 ID 的玩家进世界（落盘测试要用真实角色行）。
    fn add_player_as(
        &mut self,
        object_id: u32,
        character_id: i64,
        name: &str,
        level: u16,
        gold: i64,
    ) -> u32 {
        let stats = combat::player_base_stats(level, MirClass::Warrior);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.enter(Player {
            object_id,
            session_id: u64::from(object_id),
            character_id,
            name: name.to_owned(),
            map: MAP.to_owned(),
            position: Point::new(10, 10),
            direction: MirDirection::Down,
            level,
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
            attack_mode: mir_core::enums::AttackMode::Peace,
            pk_points: 0,
            brown_time: 0,
            pk_point_time: 0,
            last_hitter: None,
            last_hit_time: 0,
            name_colour: mir_core::Color(-1),
            visible_monsters: BTreeSet::new(),
            visible_npcs: BTreeSet::new(),
            movement_time: 0,
            inventory: mir_server::items::new_inventory(),
            equipment: mir_server::items::new_equipment(),
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

    fn name_of(&self, object_id: u32) -> String {
        self.world
            .player(object_id)
            .map_or_else(|| panic!("玩家 {object_id} 不在世界里"), |p| p.name.clone())
    }

    /// 取走某个玩家收到的全部包。
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

    /// 让 `leader` 建立一个行会（等级/金币足够），返回 (行会 ID, 行会名)。
    fn create_guild(&mut self, leader: u32, name: &str) -> i32 {
        self.world
            .create_guild(leader, name)
            .unwrap_or_else(|error| panic!("创建行会 {name} 应成功：{error}"))
    }

    /// 邀请并让 `member` 接受；返回岗位序号。
    fn invite_and_join(&mut self, inviter: u32, member: u32) -> u8 {
        let name = self.name_of(member);
        self.world
            .guild_invite(inviter, &name)
            .unwrap_or_else(|error| panic!("邀请 {name} 应成功：{error}"));
        match self
            .world
            .guild_join(member, true)
            .unwrap_or_else(|error| panic!("{name} 接受邀请应成功：{error}"))
        {
            GuildJoinOutcome::Joined(_) => {}
            GuildJoinOutcome::Declined => panic!("不该走到拒绝分支"),
        }
        self.world
            .guild_ref_of(member)
            .expect("加入后应有行会")
            .rank_index
    }

    /// 把世界积压的行会写入落到 `MemoryStore`。
    async fn flush(&mut self, store: &MemoryStore) {
        let writes = self.world.take_guild_writes();
        persist_guild_writes(store, writes)
            .await
            .expect("行会落盘应成功");
    }
}

/// 某个玩家的 `S.Chat` 文本与类型。
fn chats(packets: &[ServerPacket]) -> Vec<(&str, ChatType)> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::Chat(chat) => Some((chat.message.as_str(), chat.type_)),
            _ => None,
        })
        .collect()
}

/// 某个玩家的 `S.GuildStatus` 列表。
fn statuses(packets: &[ServerPacket]) -> Vec<&mir_protocol::packets::server::GuildStatus> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::GuildStatus(status) => Some(status),
            _ => None,
        })
        .collect()
}

/// `S.GuildMemberChange` 的 `(名字, Status, RankIndex)`。
fn member_changes(packets: &[ServerPacket]) -> Vec<(&str, u8, u8)> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::GuildMemberChange(change) => {
                Some((change.name.as_str(), change.status, change.rank_index))
            }
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. 创建
// ---------------------------------------------------------------------------

#[test]
fn create_guild_deducts_gold_and_marks_writes() {
    let mut harness = Harness::new(1);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 1_000_500);
    harness.clear(leader);

    let guild_id = harness.create_guild(leader, "测试行会");

    // `Info.AccountInfo.Gold -= Required.Amount; Enqueue(new S.LoseGold …)`（`PlayerObject.cs:9780-9781`）。
    assert_eq!(
        harness.world.player(leader).map(|player| player.gold),
        Some(500),
        "应扣掉 1,000,000 金币"
    );
    let packets = harness.packets(leader);
    assert!(
        packets.iter().any(
            |packet| matches!(packet, ServerPacket::LoseGold(gold) if gold.gold == CREATION_GOLD)
        ),
        "应收到 S.LoseGold，实际 {packets:?}"
    );

    // `Info.GuildIndex = guildInfo.GuildIndex; MyGuild = guild; MyGuildRank = guild.FindRank(Name)`
    // （`PlayerObject.cs:9825-9828`）。
    let guild = harness.world.guild(guild_id).expect("行会应存在");
    assert_eq!(guild.name, "测试行会");
    assert_eq!(guild.level, 0);
    assert_eq!(guild.members.len(), 1, "只有会长一个人");
    assert_eq!(
        guild.ranks.len(),
        1,
        "创建时只有 Leader 职位（GuildInfo.cs:54-58）"
    );
    assert_eq!(guild.ranks[0].name, "Leader");
    assert_eq!(guild.ranks[0].options, 255, "会长是全部权限");
    assert_eq!(guild.leader, i64::from(leader));
    assert_eq!(
        harness.world.guild_ref_of(leader),
        Some(mir_server::GuildRef {
            guild_id,
            rank_index: 0
        })
    );

    // `PlayerLogged(newMember, true, New: true)` → `GuildMemberChange{Status=2}`（`GuildObject.cs:186`）；
    // `MyGuild.SendGuildStatus(this)`（`PlayerObject.cs:9835`）。
    assert!(
        member_changes(&packets).contains(&("阿甲", 2, 0)),
        "应收到 Status=2 的成员变动，实际 {packets:?}"
    );
    let status = statuses(&packets);
    assert_eq!(status.len(), 1, "应收到一条 S.GuildStatus");
    assert_eq!(status[0].guild_name, "测试行会");
    assert_eq!(status[0].guild_rank_name, "Leader");
    assert_eq!(status[0].member_count, 1);
    assert_eq!(status[0].my_rank_id, 0);
    assert_eq!(status[0].my_options.value(), 255);

    // 写库：整份行会 + 角色的 guild_index。
    let writes = harness.world.take_guild_writes();
    assert!(
        writes.iter().any(|write| matches!(
            write,
            GuildWrite::Save(record) if record.id == guild_id && record.name == "测试行会"
        )),
        "应排入一条 Save，实际 {writes:?}"
    );
    assert!(
        writes.iter().any(|write| matches!(
            write,
            GuildWrite::SetCharacterGuild {
                character_id,
                guild_id: id
            } if *character_id == i64::from(leader) && *id == guild_id
        )),
        "应排入一条 SetCharacterGuild，实际 {writes:?}"
    );
}

#[test]
fn create_guild_rejects_short_long_and_illegal_names() {
    let mut harness = Harness::new(2);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);

    // `if ((Name.Length < 3) || (Name.Length > 20))`（`PlayerObject.cs:10033`）。
    assert_eq!(
        harness.world.create_guild(leader, "行会"),
        Err(GuildError::NameLength)
    );
    assert_eq!(
        harness.world.create_guild(leader, &"会".repeat(21)),
        Err(GuildError::NameLength)
    );
    // `if (Name.Contains('\\'))`（`:10039`）；换行是本作额外拒绝的（见 `validate_guild_name`）。
    assert_eq!(
        harness.world.create_guild(leader, "行\\会"),
        Err(GuildError::NameInvalid)
    );
    assert_eq!(
        harness.world.create_guild(leader, "行\n会"),
        Err(GuildError::NameInvalid)
    );
    assert!(harness.world.guilds().is_empty(), "失败不应建出行会");
    assert_eq!(
        harness.world.player(leader).map(|player| player.gold),
        Some(10_000_000),
        "失败不应扣钱"
    );
}

#[test]
fn create_guild_rejects_duplicate_name_ignoring_case_and_spaces() {
    let mut harness = Harness::new(3);
    let alice = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let bob = harness.add_player(2, "Bob", LEADER_LEVEL, 10_000_000);
    harness.create_guild(alice, "TestGuild");

    // `Envir.GetGuild(name)` 比较时 `Name.Replace(" ", "")` + `OrdinalIgnoreCase`（`Envir.cs:4836`）。
    assert_eq!(
        harness.world.create_guild(bob, "testguild"),
        Err(GuildError::NameTaken("testguild".to_owned()))
    );
    assert_eq!(
        harness.world.create_guild(bob, "Test Guild"),
        Err(GuildError::NameTaken("Test Guild".to_owned()))
    );
    assert_eq!(harness.world.guilds().len(), 1);
}

#[test]
fn create_guild_rejects_insufficient_gold() {
    let mut harness = Harness::new(4);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, i64::from(CREATION_GOLD) - 1);

    // `if (Info.AccountInfo.Gold < Required.Amount)`（`PlayerObject.cs:9733-9737`）。
    assert_eq!(
        harness.world.create_guild(leader, "测试行会"),
        Err(GuildError::NotEnoughGold {
            required: CREATION_GOLD,
            gold: i64::from(CREATION_GOLD) - 1,
        })
    );
    assert!(harness.world.guilds().is_empty());
}

#[test]
fn create_guild_rejects_low_level() {
    let mut harness = Harness::new(5);
    let leader = harness.add_player(1, "阿甲", 21, 10_000_000);

    // `if (Info.Level < Settings.Guild_RequiredLevel)`（`PlayerObject.cs:9713-9717`）。
    assert_eq!(
        harness.world.create_guild(leader, "测试行会"),
        Err(GuildError::LevelTooLow {
            required: 22,
            level: 21
        })
    );
    assert!(harness.world.guilds().is_empty());
}

#[test]
fn create_guild_twice_is_rejected() {
    let mut harness = Harness::new(6);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    harness.create_guild(leader, "第一行会");

    // `if ((MyGuild != null) || (Info.GuildIndex != -1)) return false;`（`:9710`）。
    assert_eq!(
        harness.world.create_guild(leader, "第二行会"),
        Err(GuildError::AlreadyInGuild)
    );
    assert_eq!(
        harness.world.player(leader).map(|player| player.gold),
        Some(10_000_000 - i64::from(CREATION_GOLD)),
        "第二次尝试不应再扣钱"
    );
}

// ---------------------------------------------------------------------------
// 2. 邀请 / 加入
// ---------------------------------------------------------------------------

#[test]
fn invite_then_accept_adds_member_and_sends_status() {
    let mut harness = Harness::new(7);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.clear(leader);
    harness.clear(bob);

    // 邀请：只给被邀请者发 `S.GuildInvite{Name = MyGuild.Name}`（`PlayerObject.cs:9886`）。
    let target = harness
        .world
        .guild_invite(leader, "bob")
        .expect("邀请应成功");
    assert_eq!(target, bob, "角色名查找忽略大小写");
    assert_eq!(harness.world.guild_invitation_of(bob), Some(guild_id));
    let packets = harness.packets(bob);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::GuildInvite(invite) if invite.name == "测试行会")),
        "被邀请者应收到 S.GuildInvite，实际 {packets:?}"
    );
    assert!(harness.packets(leader).is_empty(), "邀请者不应收到任何回包");
    assert_eq!(
        harness
            .world
            .guild(guild_id)
            .map(|guild| guild.members.len()),
        Some(1),
        "接受之前不算成员"
    );

    // 接受：`PendingGuildInvite.NewMember(this)`（`:9995`）。
    match harness.world.guild_join(bob, true) {
        Ok(GuildJoinOutcome::Joined(id)) => assert_eq!(id, guild_id),
        other => panic!("接受邀请应成功，实际 {other:?}"),
    }
    let guild = harness.world.guild(guild_id).expect("行会");
    assert_eq!(guild.members.len(), 2);
    // `GuildObject.NewMember` 会补一个 `Members` 职位（`GuildObject.cs:224-227`）。
    assert_eq!(guild.ranks.len(), 2);
    assert_eq!(guild.ranks[1].name, "Members");
    assert_eq!(
        harness.world.guild_ref_of(bob),
        Some(mir_server::GuildRef {
            guild_id,
            rank_index: 1
        }),
        "新人落在最低职位"
    );

    // 双方都收到 `Status=2`；被邀请者还收到 `S.GuildStatus`（`:10001-10003`）。
    let bob_packets = harness.packets(bob);
    assert!(
        member_changes(&bob_packets).contains(&("Bob", 2, 1)),
        "Bob 应收到 Status=2，实际 {bob_packets:?}"
    );
    assert_eq!(statuses(&bob_packets).len(), 1, "Bob 应收到行会状态");
    assert!(
        member_changes(&harness.packets(leader)).contains(&("Bob", 2, 1)),
        "会长也应收到新成员广播"
    );

    // 成员列表（`S.GuildMemberChange{Status=255}`，`PlayerObject.cs:10025`）。
    harness.clear(leader);
    harness
        .world
        .guild_request_info(leader, 1)
        .expect("拉取成员表");
    let packets = harness.packets(leader);
    let change = packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::GuildMemberChange(change) if change.status == 255 => Some(change),
            _ => None,
        })
        .expect("应收到整份成员表");
    assert_eq!(change.ranks.len(), 2);
    let names: Vec<&str> = change
        .ranks
        .iter()
        .flat_map(|rank| rank.members.iter().map(|member| member.name.as_str()))
        .collect();
    assert_eq!(names, vec!["阿甲", "Bob"]);
    assert_eq!(change.ranks[0].options, 255);
    assert!(change.ranks[0].members[0].online, "在线的成员 Online=true");
}

#[test]
fn accepting_twice_or_joining_twice_is_rejected() {
    let mut harness = Harness::new(8);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);

    // 没有邀请时再接受 → `ServerTextKeys.GuildNotInvited`（`PlayerObject.cs:9980-9984`）。
    assert_eq!(
        harness.world.guild_join(bob, true),
        Err(GuildError::NoInvitation)
    );

    // 已入会的人再被邀请 → `ServerTextKeys.AlreadyInGuild`（`:9864-9868`）。
    assert_eq!(
        harness.world.guild_invite(leader, "Bob"),
        Err(GuildError::TargetAlreadyInGuild("Bob".to_owned()))
    );
    assert_eq!(
        harness
            .world
            .guild(guild_id)
            .map(|guild| guild.members.len()),
        Some(2)
    );
}

#[test]
fn guild_invite_requires_recruit_permission() {
    let mut harness = Harness::new(9);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let carol = harness.add_player(3, "Carol", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);
    assert_eq!(
        harness
            .world
            .guild_ref_of(bob)
            .map(|reference| reference.guild_id),
        Some(guild_id)
    );
    harness.clear(bob);
    harness.clear(carol);

    // `if (!MyGuildRank.Options.HasFlag(GuildRankOptions.CanRecruit))`（`PlayerObject.cs:9850-9854`）。
    // 走真实的包路径 `C.EditGuildMember{ChangeType=0}`，这样权限提示才会发出来。
    assert_eq!(
        harness.world.guild_edit_member(
            bob,
            &GuildMemberEdit {
                change_type: 0,
                rank_index: 0,
                name: "Carol".to_owned(),
                rank_name: String::new(),
            }
        ),
        Err(GuildError::Permission("CanRecruit"))
    );
    assert!(
        harness.packets(carol).is_empty(),
        "无权邀请时目标不应收到邀请"
    );
    let packets = harness.packets(bob);
    let hints = chats(&packets);
    assert!(
        hints
            .iter()
            .any(|(text, kind)| *kind == ChatType::System && text.contains("CanRecruit")),
        "邀请者应收到权限提示，实际 {hints:?}"
    );
}

// ---------------------------------------------------------------------------
// 3. 职位 / 踢人
// ---------------------------------------------------------------------------

#[test]
fn promote_and_demote_member() {
    let mut harness = Harness::new(10);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let bob = harness.add_player(2, "Bob", LEADER_LEVEL, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);

    // 先建一个新职位（`ChangeType=4`，`GuildObject.NewRank`，`GuildObject.cs:334-352`）：
    // Leader(0) / Rank-1(1) / Members(2)。
    harness
        .world
        .guild_edit_member(
            leader,
            &GuildMemberEdit {
                change_type: 4,
                rank_index: 0,
                name: String::new(),
                rank_name: String::new(),
            },
        )
        .expect("新职位应成功");
    let guild = harness.world.guild(guild_id).expect("行会");
    assert_eq!(guild.ranks.len(), 3);
    assert_eq!(guild.ranks[1].name, "Rank-1");
    // 插入后 Members 的序号从 1 变成 2，成员跟着搬（本作的序号同步，见 `guild_new_rank`）。
    assert_eq!(
        harness
            .world
            .guild_ref_of(bob)
            .map(|reference| reference.rank_index),
        Some(2)
    );

    harness.clear(leader);
    harness.clear(bob);

    // 升职：`MyGuild.ChangeRank(this, Name, RankIndex, RankName)`（`PlayerObject.cs:9909`）。
    harness
        .world
        .guild_edit_member(
            leader,
            &GuildMemberEdit {
                change_type: 2,
                rank_index: 1,
                name: "Bob".to_owned(),
                rank_name: String::new(),
            },
        )
        .expect("升职应成功");
    assert_eq!(
        harness
            .world
            .guild_ref_of(bob)
            .map(|reference| reference.rank_index),
        Some(1)
    );
    // 被调动的人：`GuildMemberChange{Name = 操作者, Status = 8, Ranks = [新职位]}`（`GuildObject.cs:319`）。
    let bob_packets = harness.packets(bob);
    let change = bob_packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::GuildMemberChange(change) if change.status == 8 => Some(change),
            _ => None,
        })
        .expect("被调动的人应收到 Status=8");
    assert_eq!(change.name, "阿甲");
    assert_eq!(change.ranks.len(), 1);
    assert_eq!(change.ranks[0].index, 1);
    assert!(
        change.ranks[0]
            .members
            .iter()
            .any(|member| member.name == "Bob")
    );
    // 其他人：`GuildMemberChange{Name = 被调动者, Status = 5, RankIndex}`（`:328`）。
    assert!(
        member_changes(&harness.packets(leader)).contains(&("Bob", 5, 1)),
        "会长应收到 Status=5"
    );

    // 降职：回到最低职位 Members(2)。
    harness
        .world
        .guild_edit_member(
            leader,
            &GuildMemberEdit {
                change_type: 2,
                rank_index: 2,
                name: "Bob".to_owned(),
                rank_name: String::new(),
            },
        )
        .expect("降职应成功");
    assert_eq!(
        harness
            .world
            .guild_ref_of(bob)
            .map(|reference| reference.rank_index),
        Some(2)
    );
}

#[test]
fn promote_to_leader_requires_level_and_transfers_leadership() {
    let mut harness = Harness::new(11);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let lowbie = harness.add_player(2, "Lowbie", 10, 0);
    let bob = harness.add_player(3, "Bob", LEADER_LEVEL, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, lowbie);
    harness.invite_and_join(leader, bob);

    // 目标等级 < `Guild_RequiredLevel` → `ServerTextKeys.GuildLeaderMinLevel`（`GuildObject.cs:258-262`）。
    assert_eq!(
        harness.world.guild_edit_member(
            leader,
            &GuildMemberEdit {
                change_type: 2,
                rank_index: 0,
                name: "Lowbie".to_owned(),
                rank_name: String::new(),
            }
        ),
        Err(GuildError::LeaderLevelTooLow { required: 22 })
    );

    // 会长转让（Crystal 没有单独的「转让」包，就是升成 rank 0）。
    harness
        .world
        .guild_edit_member(
            leader,
            &GuildMemberEdit {
                change_type: 2,
                rank_index: 0,
                name: "Bob".to_owned(),
                rank_name: String::new(),
            },
        )
        .expect("转让会长应成功");
    let guild = harness.world.guild(guild_id).expect("行会");
    assert_eq!(guild.leader, i64::from(bob));
    // Crystal 的会长是 `Ranks[0].Members`（可以有多人，`GuildObject.cs:265-279` 允许两个会长）；
    // 本作的 `leader` 记「最近一次升成 rank 0 的人」，`leader_name()` 是 rank 0 的第一个成员
    // （Crystal `Ranks[0].Members[0]`，`GuildObject.cs:307`），所以这里仍是入会最早的阿甲。
    assert_eq!(guild.leader_name(), "阿甲");
    assert!(
        guild
            .members
            .values()
            .filter(|member| member.rank_index == 0)
            .count()
            == 2,
        "两个会长同时存在"
    );
    assert_eq!(
        harness
            .world
            .guild_ref_of(bob)
            .map(|reference| reference.rank_index),
        Some(0)
    );
}

#[test]
fn kick_member_clears_guild_and_sends_empty_status() {
    let mut harness = Harness::new(12);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);
    harness.clear(leader);
    harness.clear(bob);

    // `ChangeType=1` → `MyGuild.DeleteMember(this, Name)`（`PlayerObject.cs:9897`）。
    harness
        .world
        .guild_edit_member(
            leader,
            &GuildMemberEdit {
                change_type: 1,
                rank_index: 0,
                name: "Bob".to_owned(),
                rank_name: String::new(),
            },
        )
        .expect("踢人应成功");

    assert_eq!(
        harness
            .world
            .guild(guild_id)
            .map(|guild| guild.members.len()),
        Some(1)
    );
    assert_eq!(harness.world.guild_ref_of(bob), None, "被踢的人不再有行会");
    let packets = harness.packets(bob);
    // `MemberDeleted(name, formerMember, kickSelf: false)` 只把 `Status=3` 发给**其余**在线成员
    // （`GuildObject.cs:541-552`）；被踢的人收到的是空 `GuildStatus` + 行会频道提示。
    assert!(
        !member_changes(&packets)
            .iter()
            .any(|(_, status, _)| *status == 3),
        "被踢的人不应收到 Status=3，实际 {packets:?}"
    );
    // `formerMember.Enqueue(new S.GuildStatus { GuildName = "", … })`（`:561`）。
    let status = statuses(&packets);
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].guild_name, "");
    assert_eq!(status[0].my_rank_id, 0, "空状态的其余字段取结构体默认值");
    // `ReceiveChat(YouRemovedFromGuild, ChatType.Guild)`（`:559`）。
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, kind)| *kind == ChatType::Guild && text.contains("移出")),
        "被踢的人应收到行会频道提示，实际 {packets:?}"
    );
    // 其余成员收到 `Status=3`。
    assert!(
        member_changes(&harness.packets(leader)).contains(&("Bob", 3, 0)),
        "剩下的成员应收到 Status=3"
    );
    // 离线成员也能踢：这里顺带断言落盘里带了 `guild_index = -1`。
    assert!(
        harness
            .world
            .take_guild_writes()
            .iter()
            .any(|write| matches!(
                write,
                GuildWrite::SetCharacterGuild {
                    character_id,
                    guild_id: -1
                } if *character_id == i64::from(bob)
            )),
        "应排入一条把 guild_index 清成 -1 的写入"
    );
}

#[test]
fn kick_requires_permission_and_rank() {
    let mut harness = Harness::new(13);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let carol = harness.add_player(3, "Carol", 10, 0);
    harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);
    harness.invite_and_join(leader, carol);

    // Bob 是最低职位（Options=0），没有 CanKick（`PlayerObject.cs:9890-9894`）。
    assert_eq!(
        harness.world.guild_edit_member(
            bob,
            &GuildMemberEdit {
                change_type: 1,
                rank_index: 0,
                name: "Carol".to_owned(),
                rank_name: String::new(),
            }
        ),
        Err(GuildError::Permission("CanKick"))
    );

    // 有 CanKick 也踢不动同级/更高的职位（`GuildObject.cs:457-461`：
    // `Kicker.MyGuildRank.Index >= MemberRank.Index && Kicker.MyGuildRank.Index != 0`）。
    //
    // 先建一个新职位，得到 Leader(0) / Rank-1(1) / Members(2)；Bob、Carol 都在 Members(2)。
    harness
        .world
        .guild_edit_member(
            leader,
            &GuildMemberEdit {
                change_type: 4,
                rank_index: 0,
                name: String::new(),
                rank_name: String::new(),
            },
        )
        .expect("新职位");
    // 会长用 `ChangeType=5` 打开 Members(2) 的 CanKick（`GuildObject.cs:354-379`）。
    harness
        .world
        .guild_edit_member(
            leader,
            &GuildMemberEdit {
                change_type: 5,
                rank_index: 2,
                name: "true".to_owned(),
                rank_name: "2".to_owned(),
            },
        )
        .expect("打开 Members 的 CanKick 权限");
    assert!(
        harness
            .world
            .guild_options_of(bob)
            .is_some_and(|options| options & 4 != 0)
    );

    // 动不了会长。
    assert_eq!(
        harness.world.guild_edit_member(
            bob,
            &GuildMemberEdit {
                change_type: 1,
                rank_index: 0,
                name: "阿甲".to_owned(),
                rank_name: String::new(),
            }
        ),
        Err(GuildError::RankNotAdequate)
    );
    // 同级也不行（Crystal 的 `>=` 而不是 `>`）。
    assert_eq!(
        harness.world.guild_edit_member(
            bob,
            &GuildMemberEdit {
                change_type: 1,
                rank_index: 0,
                name: "Carol".to_owned(),
                rank_name: String::new(),
            }
        ),
        Err(GuildError::RankNotAdequate)
    );
    assert_eq!(
        harness.world.guild_ref_of(carol).map(|r| r.guild_id),
        harness.world.guild_ref_of(bob).map(|r| r.guild_id),
        "Carol 应还在行会里"
    );

    // 升到中间职位后就能踢最低职位的人了（中间职位要单独开 CanKick，
    // 因为权限是按职位算的，`GuildObject.cs:354-379`）。
    harness
        .world
        .guild_edit_member(
            leader,
            &GuildMemberEdit {
                change_type: 2,
                rank_index: 1,
                name: "Bob".to_owned(),
                rank_name: String::new(),
            },
        )
        .expect("升 Bob 到 Rank-1");
    harness
        .world
        .guild_edit_member(
            leader,
            &GuildMemberEdit {
                change_type: 5,
                rank_index: 1,
                name: "true".to_owned(),
                rank_name: "2".to_owned(),
            },
        )
        .expect("打开 Rank-1 的 CanKick 权限");
    harness
        .world
        .guild_edit_member(
            bob,
            &GuildMemberEdit {
                change_type: 1,
                rank_index: 0,
                name: "Carol".to_owned(),
                rank_name: String::new(),
            },
        )
        .expect("高职位可以踢低职位");
    assert_eq!(harness.world.guild_ref_of(carol), None);
}

// ---------------------------------------------------------------------------
// 4. 公告
// ---------------------------------------------------------------------------

#[test]
fn edit_notice_broadcasts_and_requires_permission() {
    let mut harness = Harness::new(14);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);
    harness.clear(leader);
    harness.clear(bob);

    // `MyGuild.NewNotice(notice)` → `SendServerPacket(new S.GuildNoticeChange{update = -1})`
    // （`GuildObject.cs:582-600`）。
    harness
        .world
        .guild_edit_notice(leader, vec!["第一行".to_owned(), "第二行".to_owned()])
        .expect("改公告应成功");
    assert_eq!(
        harness
            .world
            .guild(guild_id)
            .map(|guild| guild.notice.clone()),
        Some(vec!["第一行".to_owned(), "第二行".to_owned()])
    );
    for player in [leader, bob] {
        let packets = harness.packets(player);
        assert!(
            packets.iter().any(|packet| matches!(
                packet,
                ServerPacket::GuildNoticeChange(change) if change.update == -1 && change.notice.is_empty()
            )),
            "在线成员都应收到 update = -1 的公告变更通知，实际 {packets:?}"
        );
    }

    // `if (!MyGuildRank.Options.HasFlag(GuildRankOptions.CanChangeNotice))`（`PlayerObject.cs:9965-9970`）。
    assert_eq!(
        harness
            .world
            .guild_edit_notice(bob, vec!["我要改".to_owned()]),
        Err(GuildError::Permission("CanChangeNotice"))
    );

    // `if (notice.Count > 200)`（`:9971-9975`）。
    assert_eq!(
        harness
            .world
            .guild_edit_notice(leader, vec![String::new(); 201]),
        Err(GuildError::NoticeTooLong)
    );

    // 拉公告：`S.GuildNoticeChange{notice}`（`:10018-10022`）。
    harness.clear(leader);
    harness
        .world
        .guild_request_info(leader, 0)
        .expect("拉取公告应成功");
    let packets = harness.packets(leader);
    let notice = packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::GuildNoticeChange(change) if change.update >= 0 => Some(change),
            _ => None,
        })
        .expect("应收到公告正文");
    assert_eq!(
        notice.notice,
        vec!["第一行".to_owned(), "第二行".to_owned()]
    );
}

// ---------------------------------------------------------------------------
// 5. 聊天
// ---------------------------------------------------------------------------

#[test]
fn guild_chat_only_reaches_online_members() {
    let mut harness = Harness::new(15);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let carol = harness.add_player(3, "Carol", 10, 0);
    harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);
    harness.clear(leader);
    harness.clear(bob);
    harness.clear(carol);

    // `else if (message.StartsWith("!~"))`（`PlayerObject.cs:2014-2024`）：
    // `MyGuild.SendMessage(string.Format("{0}: {1}", Name, message))`。
    let receivers = harness
        .world
        .chat(leader, "!~大家好")
        .expect("行会聊天应成功");
    assert_eq!(receivers, 2, "行会聊天只发给两个在线成员");

    for member in [leader, bob] {
        let packets = harness.packets(member);
        assert!(
            chats(&packets)
                .iter()
                .any(|(text, kind)| *kind == ChatType::Guild && *text == "阿甲: 大家好"),
            "成员应收到行会频道消息，实际 {packets:?}"
        );
    }
    assert!(
        harness.packets(carol).is_empty(),
        "非成员（即使站在旁边）不应收到行会聊天"
    );

    // 不在行会里的人发 `!~` → 静默丢弃（`:2016`）。
    assert_eq!(
        harness
            .world
            .chat(carol, "!~我不是成员")
            .expect("聊天本身不算错"),
        0
    );
    assert!(harness.packets(leader).is_empty());
}

// ---------------------------------------------------------------------------
// 6. 行会经验
// ---------------------------------------------------------------------------

#[test]
fn guild_gains_exp_and_levels_up() {
    let mut harness = Harness::new(16);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);
    harness.clear(leader);
    harness.clear(bob);
    // 让 `NextExpUpdate`（0）落在 now 之前，第一次加经验就会广播（`GuildObject.cs:698`）。
    harness.world.advance_clock(1);

    // 成员获得 10,000 经验 → 行会得到 `10000 * 0.01 = 100`（`GuildObject.cs:649`）。
    harness.world.win_experience(leader, 10_000, LEADER_LEVEL);
    let guild = harness.world.guild(guild_id).expect("行会");
    assert_eq!(guild.experience, 100);
    assert_eq!(guild.level, 0, "100 经验还不满足 > 100 的升级条件");
    let packets = harness.packets(leader);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::GuildExpGain(gain) if gain.amount == 100)),
        "应收到 S.GuildExpGain{{100}}，实际 {packets:?}"
    );

    // 10 秒内的第二次加经验不发 `S.GuildExpGain`（`NextExpUpdate` 节流，`:698-700`），
    // 但经验照加：100 + 100 = 200 > 100 → 升到 1 级，余 100；升级广播的是 `S.GuildStatus`
    // （升级分支优先于节流分支，`:676-703`）。
    harness.clear(leader);
    harness.world.win_experience(leader, 10_000, LEADER_LEVEL);
    let packets = harness.packets(leader);
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::GuildExpGain(_))),
        "10 秒节流内不应发 S.GuildExpGain"
    );
    assert!(
        statuses(&packets).iter().any(|status| status.level == 1),
        "升级时应发 S.GuildStatus"
    );
    let guild = harness.world.guild(guild_id).expect("行会");
    assert_eq!(guild.level, 1, "200 经验跨过 1 级");
    assert_eq!(guild.experience, 100, "余数写回");
    assert_eq!(guild.max_experience, 200);

    // 再来 20,000 经验 → 总经验 300：> 200 升到 2 级（余 100），2 级的 MaxExperience 是 -1
    // （封顶级，Crystal `Level-22=-1` 的写法），循环停下。
    harness.world.advance_clock(10_001);
    harness.clear(leader);
    harness.clear(bob);
    harness.world.win_experience(leader, 20_000, LEADER_LEVEL);
    let guild = harness.world.guild(guild_id).expect("行会");
    assert_eq!(guild.level, 2, "应升到 2 级");
    assert_eq!(
        guild.experience, 100,
        "余数应写回（Crystal 的局部变量 bug 已修正，见 docs/world/guild.md）"
    );
    assert_eq!(guild.spare_points, 2, "每级 1 点升级点");
    assert_eq!(
        guild.max_experience, -1,
        "2 级是封顶级（Level-22=-1 的写法）"
    );
    assert_eq!(guild.member_cap, 10, "人数上限按新等级刷新");
    // 升级时给所有在线成员发 `S.GuildStatus`（`GuildObject.cs:685-694`）。
    for member in [leader, bob] {
        let packets = harness.packets(member);
        assert!(
            statuses(&packets)
                .iter()
                .any(|status| status.level == 2 && status.experience == 100),
            "升级后应广播行会状态，实际 {packets:?}"
        );
    }

    // 封顶之后再加经验：`if (Info.MaxExperience <= 0) return;`（`:647`）。
    harness.world.advance_clock(10_001);
    harness.world.win_experience(leader, 100_000, LEADER_LEVEL);
    assert_eq!(
        harness.world.guild(guild_id).map(|guild| guild.experience),
        Some(100),
        "封顶后不再涨经验"
    );
}

// ---------------------------------------------------------------------------
// 7. 退出 / 解散
// ---------------------------------------------------------------------------

#[test]
fn member_leave_removes_from_guild() {
    let mut harness = Harness::new(17);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);
    harness.clear(leader);
    harness.clear(bob);

    harness.world.guild_leave(bob).expect("退会应成功");

    assert_eq!(harness.world.guild_ref_of(bob), None);
    assert_eq!(
        harness
            .world
            .guild(guild_id)
            .map(|guild| guild.members.len()),
        Some(1)
    );
    // `MemberDeleted(..., kickSelf: true)` → `Status = 4`（`GuildObject.cs:548`）。
    assert!(
        member_changes(&harness.packets(leader)).contains(&("Bob", 4, 0)),
        "剩下的成员应收到 Status=4"
    );
    assert!(
        chats(&harness.packets(bob))
            .iter()
            .any(|(text, kind)| *kind == ChatType::Guild && text.contains("离开")),
        "退会者应收到行会频道提示"
    );
    assert!(
        harness
            .world
            .take_guild_writes()
            .iter()
            .any(|write| matches!(
                write,
                GuildWrite::SetCharacterGuild {
                    character_id,
                    guild_id: -1
                } if *character_id == i64::from(bob)
            ))
    );
}

#[test]
fn leader_can_transfer_then_leave() {
    let mut harness = Harness::new(18);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let bob = harness.add_player(2, "Bob", LEADER_LEVEL, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);

    // 转让会长后再退会：会长职位上还有 Bob，`MemberRank.Members.Count > 1` → `AllOk`
    // （`GuildObject.cs:471`）。
    harness
        .world
        .guild_edit_member(
            leader,
            &GuildMemberEdit {
                change_type: 2,
                rank_index: 0,
                name: "Bob".to_owned(),
                rank_name: String::new(),
            },
        )
        .expect("转让会长");
    harness.world.guild_leave(leader).expect("会长退会应成功");

    let guild = harness.world.guild(guild_id).expect("行会还在");
    assert_eq!(guild.members.len(), 1);
    assert_eq!(guild.leader, i64::from(bob));
    assert_eq!(guild.leader_name(), "Bob");
    assert_eq!(harness.world.guild_ref_of(leader), None);
}

#[test]
fn last_leader_cannot_leave_while_others_remain() {
    let mut harness = Harness::new(19);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);

    // `MemberRank.Index == 0 && MemberRank.Members.Count == 1 && Membercount >= 2` →
    // `ServerTextKeys.YouNeedLastLeaderToDisbandGuild`（`GuildObject.cs:463-476`）。
    assert_eq!(
        harness.world.guild_leave(leader),
        Err(GuildError::LastLeaderCannotLeave)
    );
    assert_eq!(
        harness
            .world
            .guild(guild_id)
            .map(|guild| guild.members.len()),
        Some(2)
    );
}

#[tokio::test]
async fn last_member_leaving_disbands_and_deletes_row() {
    let store = MemoryStore::new();
    let account = store
        .create_account("diana", "secret", "")
        .await
        .expect("建号");
    let character = store
        .create_character(
            account.id,
            "阿甲",
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (10, 10),
        )
        .await
        .expect("建角");

    let mut harness = Harness::new(20);
    let leader = harness.add_player_as(1, character.id, "阿甲", LEADER_LEVEL, 10_000_000);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.flush(&store).await;

    // 建行会已经落库：行会一行 + 角色的 guild_index。
    let rows = store.guilds().await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, guild_id);
    assert_eq!(rows[0].name, "测试行会");
    assert_eq!(rows[0].ranks.len(), 1);
    assert_eq!(rows[0].members.len(), 1);
    assert_eq!(rows[0].members[0].character_id, character.id);
    assert_eq!(
        store.character_guild_index(character.id).await,
        Some(guild_id),
        "characters.guild_index 应写进行会 ID"
    );

    // 最后一个人退出 → `LeaderOk` → `Envir.DeleteGuild(this)`（`GuildObject.cs:494-508`）。
    harness.world.guild_leave(leader).expect("解散行会");
    assert!(
        harness.world.guild(guild_id).is_none(),
        "内存里的行会应删除"
    );
    harness.flush(&store).await;

    assert!(store.guilds().await.is_empty(), "数据库里的行会应删除");
    assert_eq!(
        store.character_guild_index(character.id).await,
        Some(-1),
        "角色的 guild_index 应清回 -1"
    );
    // 删库的写入也出现在队列里（顺序：Save → Delete）。
    let writes = harness.world.take_guild_writes();
    assert!(writes.is_empty());
}

#[tokio::test]
async fn character_guild_index_round_trips_through_store() {
    let store = MemoryStore::new();
    let account = store
        .create_account("diana", "secret", "")
        .await
        .expect("建号");
    let character = store
        .create_character(
            account.id,
            "阿甲",
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (10, 10),
        )
        .await
        .expect("建角");
    assert_eq!(character.guild_index, -1, "新角色没有行会");

    // 保存一次带行会的角色：`mir-store` 的 `characters.guild_index` 列。
    let mut updated = character.clone();
    updated.guild_index = 7;
    store.save_character(updated).await.expect("存档");

    let reloaded = store
        .list_characters(account.id)
        .await
        .expect("读角色")
        .into_iter()
        .next()
        .expect("应有一个角色");
    assert_eq!(reloaded.guild_index, 7, "guild_index 应整行往返");

    // `set_character_guild` 单独改这一列，等价于 `mir-store::set_character_guild`。
    store
        .set_character_guild(character.id, 42)
        .await
        .expect("写行会归属");
    assert_eq!(store.character_guild_index(character.id).await, Some(42));
}

// ---------------------------------------------------------------------------
// 8. 重登 / 数据往返
// ---------------------------------------------------------------------------

#[test]
fn relogin_rebinds_guild_membership() {
    let mut harness = Harness::new(21);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);
    harness.clear(leader);

    // Bob 下线：成员标记离线并广播 `Status=0`（`GuildObject.cs:177-186`）。
    harness.world.leave(bob);
    let guild = harness.world.guild(guild_id).expect("行会");
    assert!(
        guild
            .members
            .values()
            .all(|member| member.online != Some(bob))
    );
    assert!(
        member_changes(&harness.packets(leader)).contains(&("Bob", 0, 1)),
        "会长应收到下线广播（Status=0，RankIndex 是 Bob 的 1）"
    );

    // 重登：新对象 ID，同样的角色 ID —— `World::enter` 会从成员表反查回来。
    let bob_again = harness.add_player_as(9, i64::from(bob), "Bob", 10, 0);
    assert_eq!(
        harness.world.guild_ref_of(bob_again),
        Some(mir_server::GuildRef {
            guild_id,
            rank_index: 1
        }),
        "重登后应按角色 ID 找回行会"
    );
    // `guild_player_online` 才是发包入口（会话在 `UserInformation` 之后调用）。
    harness.world.guild_player_online(bob_again);
    let packets = harness.packets(bob_again);
    assert!(
        member_changes(&packets).contains(&("Bob", 1, 1)),
        "上线广播 Status=1，实际 {packets:?}"
    );
    assert_eq!(statuses(&packets).len(), 1, "应收到自己的行会状态");
}

#[test]
fn guild_settings_parse_the_real_ini_shape() {
    let text = "[Guilds]\nMinimumLevel=22\nExpRate=0.01\nPointPerLevel=1\n\
                [Exp]\nLevel-0=45000\nLevel-1=700000\nLevel-22=-1\n\
                [Cap]\nLevel-0=5\nLevel-1=5\n\
                [Required-0]\nItemName=\nAmount=1000000\n";
    let settings = GuildSettings::parse_ini(text);
    assert_eq!(settings.required_level, 22);
    assert_eq!(settings.creation_gold, 1_000_000);
    assert_eq!(settings.point_per_level, 1);
    assert!((settings.exp_rate - 0.01).abs() < f64::EPSILON);
    assert_eq!(settings.max_experience(0), 45_000);
    assert_eq!(settings.max_experience(1), 700_000);
    assert_eq!(settings.max_experience(2), 0, "缺项按 0（不能升级）");
    assert_eq!(
        settings.max_experience(22),
        -1,
        "Level-22=-1 是 Crystal 的封顶写法"
    );
    assert_eq!(settings.member_cap(0), 5);
    assert_eq!(settings.member_cap(2), 0, "缺项 = 不限");
}

// ---------------------------------------------------------------------------
// 9. NPC 脚本（合成脚本 + 真实数据冒烟）
// ---------------------------------------------------------------------------

/// 一个独立的 `Envir/` 目录（退出时删除）。
struct Envir {
    root: std::path::PathBuf,
}

impl Envir {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "mir-next-guild-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("NPCs/Test")).expect("建 NPC 目录");
        Self { root }
    }

    fn script(&self, name: &str, text: &str) -> &Self {
        let path = self.root.join("NPCs").join(format!("{name}.txt"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("建脚本目录");
        }
        std::fs::write(&path, text).expect("写脚本");
        self
    }

    fn drop_dir(&self) -> std::path::PathBuf {
        self.root.join("Drops")
    }
}

impl Drop for Envir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// 带一个行会 NPC 的世界。
fn npc_harness(tag: &str, script: &str) -> (Harness, Envir) {
    let envir = Envir::new(tag);
    envir.script("Test/Guild", script);
    let db = MirDb {
        npcs: vec![NpcInfo {
            index: 1,
            map_index: MAP_INDEX,
            file_name: "Test/Guild".to_owned(),
            name: "行会管理员".to_owned(),
            location: Point::new(11, 10),
            image: 5,
            rate: 100,
            ..NpcInfo::default()
        }],
        ..base_db()
    };
    let mut world = World::new(999);
    world.seed_rng(0x6_11D);
    world.set_game_data(Arc::new(GameData::from_db(db, Some(envir.drop_dir()))));
    world.set_guild_settings(test_settings());
    let spawned = world.spawn_npcs(MAP);
    assert_eq!(spawned.len(), 1, "行会 NPC 应刷出来");
    (
        Harness {
            world,
            receivers: HashMap::new(),
        },
        envir,
    )
}

#[test]
fn npc_createguild_page_requests_a_name() {
    // 真实脚本的写法：`[@Main]` 里放 `<…/@CREATEGUILD>` 按钮，并且**必须有一个
    // `[@CREATEGUILD]` 页面**（可以是空页）——Crystal 的 `ProcessSpecial` 只在页面被处理时
    // 才调用（`NPCScript.cs:895-900`），GM-Manager 脚本第 45 行就是这样一个空页。
    let script = "\
[@Main]\n\
#SAY\n\
要创建行会吗？\n\
<创建行会/@CREATEGUILD> <关闭/@exit>\n\
[@CREATEGUILD]\n";
    let (mut harness, _envir) = npc_harness("page", script);
    let player = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let npc = harness.world.npcs()[0].object_id;

    harness
        .world
        .call_npc(player, npc, "[@MAIN]")
        .expect("打开行会 NPC");
    harness.clear(player);

    // `case GuildCreateKey: … player.CanCreateGuild = true; player.Enqueue(new S.GuildNameRequest())`
    // （`NPCScript.cs:1050-1059`）。
    harness
        .world
        .call_npc(player, npc, GUILD_CREATE_KEY)
        .expect("点创建行会按钮");
    let packets = harness.packets(player);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::GuildNameRequest(_))),
        "应收到 S.GuildNameRequest，实际 {packets:?}"
    );
    assert!(
        harness
            .world
            .player(player)
            .is_some_and(|player| player.npc.can_create_guild),
        "应记下 CanCreateGuild（`PlayerObject.cs:141`）"
    );
}

#[test]
fn npc_createguild_page_rejects_low_level() {
    let script = "[@Main]\n#SAY\n要创建行会吗？\n<创建行会/@CREATEGUILD>\n[@CREATEGUILD]\n";
    let (mut harness, _envir) = npc_harness("low", script);
    let player = harness.add_player(1, "阿甲", 10, 10_000_000);
    let npc = harness.world.npcs()[0].object_id;
    harness
        .world
        .call_npc(player, npc, "[@MAIN]")
        .expect("打开");
    harness.clear(player);

    // `if (player.Info.Level < Settings.Guild_RequiredLevel) ReceiveChat(YouNeedLevelToCreateGuild)`（`:1051-1054`）。
    harness
        .world
        .call_npc(player, npc, GUILD_CREATE_KEY)
        .expect("按钮本身可用");
    let packets = harness.packets(player);
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::GuildNameRequest(_))),
        "等级不够不应弹输入框"
    );
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, kind)| *kind == ChatType::System && text.contains("22")),
        "应提示需要 22 级，实际 {packets:?}"
    );
}

#[test]
fn npc_createguild_action_creates_the_guild() {
    // 本作扩展的脚本动作：`#ACT / CREATEGUILD <名字>`（Crystal 只有 `[@CREATEGUILD]` 页面）。
    let script = "[@Main]\n#ACT\nCREATEGUILD 脚本行会\n#SAY\n祝你好运。\n";
    let (mut harness, _envir) = npc_harness("action", script);
    let player = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    let npc = harness.world.npcs()[0].object_id;

    harness
        .world
        .call_npc(player, npc, "[@MAIN]")
        .expect("跑脚本");
    let guild = harness
        .world
        .guild_by_name("脚本行会")
        .unwrap_or_else(|| panic!("脚本里的 CREATEGUILD 应建出行会"));
    assert_eq!(guild.members.len(), 1);
    assert_eq!(
        harness.world.player(player).map(|player| player.gold),
        Some(10_000_000 - i64::from(CREATION_GOLD))
    );
    let packets = harness.packets(player);
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, kind)| *kind == ChatType::System && text.contains("创建成功")),
        "应收到成功提示，实际 {packets:?}"
    );
}

/// 真实数据冒烟：统计素材里用到 `CREATEGUILD` 的脚本，并跑通真实 GM 脚本的创建流程。
///
/// * 统计部分扫描 `GameData::discover()` 指向的 `Envir/NPCs`（缺失时跳过）；
/// * 流程部分用**真实脚本文件** `GM/GM-Manager.txt`（`[@Main]` 里 `<Guild Application/@CreateGuildCheck>`，
///   一路 `GOTO @CREATEGUILD`），NPC 用合成的一行数据库（真实数据库里这个 NPC 在 GM 地图上，
///   而测试世界只加载一张图；脚本本身是真实文件，这是本用例唯一合成的输入）。
#[test]
fn real_data_guild_scripts_and_creation_flow() {
    let Ok(data) = GameData::discover() else {
        eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
        return;
    };
    let Some(envir) = data.envir_dir().map(Path::to_path_buf) else {
        eprintln!("跳过：数据库不在文件系统上");
        return;
    };
    let npc_dir = envir.join("NPCs");
    if !npc_dir.is_dir() {
        eprintln!("跳过：{} 下没有 NPCs 目录", envir.display());
        return;
    }

    // ---- 统计 `[@CREATEGUILD]` / `CREATEGUILD` 的使用 ----
    let mut page_key_files = 0_usize;
    let mut page_key_hits = 0_usize;
    let mut action_hits = 0_usize;
    let mut stack = vec![npc_dir.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let key_hits = text.to_uppercase().matches("[@CREATEGUILD]").count();
            page_key_hits += key_hits;
            if key_hits > 0 {
                page_key_files += 1;
            }
            action_hits += text
                .lines()
                .filter(|line| line.trim_start().to_uppercase().starts_with("CREATEGUILD"))
                .count();
        }
    }
    eprintln!(
        "真实 NPC 脚本里 [@CREATEGUILD] 页面：{page_key_hits} 处 / {page_key_files} 份脚本；\
         直接写 CREATEGUILD 动作的：{action_hits} 处（Crystal 没有这个动作，真实脚本里应当为 0）"
    );
    assert!(
        page_key_files > 0,
        "素材里至少应有一份脚本用 [@CREATEGUILD]（GM-Manager）"
    );
    assert_eq!(
        action_hits, 0,
        "真实脚本不应出现本作扩展的 CREATEGUILD 动作"
    );

    // ---- 用真实脚本跑一遍创建流程 ----
    let script_path = npc_dir.join("GM").join("GM-Manager.txt");
    if !script_path.is_file() {
        eprintln!("跳过流程：{} 不存在", script_path.display());
        return;
    }
    let mut world = World::new(999);
    world.seed_rng(2024);
    world.set_guild_settings(GuildSettings {
        // 真实脚本会 `GIVEGOLD`/`CHANGELEVEL` 把玩家补到门槛，这里直接给足等级与金币。
        ..test_settings()
    });
    // 合成一行 NPC 记录，指向真实的 `GM/GM-Manager.txt`。
    let db = MirDb {
        npcs: vec![NpcInfo {
            index: 1,
            map_index: MAP_INDEX,
            file_name: "GM/GM-Manager".to_owned(),
            name: "GM 管理员".to_owned(),
            location: Point::new(11, 10),
            image: 5,
            rate: 100,
            ..NpcInfo::default()
        }],
        ..base_db()
    };
    world.set_game_data(Arc::new(GameData::from_db(db, Some(envir.join("Drops")))));
    world.insert_map(MAP, synthetic_map());
    assert_eq!(world.spawn_npcs(MAP).len(), 1, "GM 脚本 NPC 应刷出来");
    let npc = world.npcs()[0].object_id;

    let mut harness = Harness {
        world,
        receivers: HashMap::new(),
    };
    let player = harness.add_player(1, "阿甲", LEADER_LEVEL, 10_000_000);
    // 真实 GM 脚本的入口页要求 GM 身份（`#IF ISADMIN`），普通玩家点不到里面的按钮；
    // 为了跑通「真实脚本的 `[@CREATEGUILD]` 页面」这一段，直接把对话状态与按钮白名单
    // 填好（等价于客户端点了 `[@Main]` 里那个 GM 才看得见的按钮）。
    {
        let state = &mut harness.world.player_mut(player).expect("玩家").npc;
        state.npc_object_id = npc;
        state.page_key = "[@MAIN]".to_owned();
        state.buttons.insert(GUILD_CREATE_KEY.to_owned());
    }
    harness.clear(player);
    harness
        .world
        .call_npc(player, npc, GUILD_CREATE_KEY)
        .expect("点 @CREATEGUILD 页面");
    let packets = harness.packets(player);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::GuildNameRequest(_))),
        "真实脚本的 [@CREATEGUILD] 页面应触发 S.GuildNameRequest，实际 {packets:?}"
    );
    assert!(
        harness
            .world
            .player(player)
            .is_some_and(|player| player.npc.can_create_guild),
        "真实脚本应把 CanCreateGuild 置位（`NPCScript.cs:1057`）"
    );

    // 客户端把名字送回来（等价于 `C.GuildNameReturn`）。
    let guild_id = harness
        .world
        .create_guild(player, "真实脚本行会")
        .expect("创建行会");
    assert_eq!(
        harness
            .world
            .guild(guild_id)
            .map(|guild| guild.name.clone()),
        Some("真实脚本行会".to_owned())
    );
}
