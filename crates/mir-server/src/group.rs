//! 组队（party）。
//!
//! Crystal 把队伍放在 `MapObject` 上：`public List<PlayerObject> GroupMembers`（`Server/MirObjects/MapObject.cs:162`），
//! **所有成员共享同一个 `List` 实例**（`GroupMembers = GroupInvitation.GroupMembers`，`:9437`），
//! 于是「删掉自己」对所有人同时生效（`:9237`），列表首元素 `GroupMembers[0]` 就是队长
//! （邀请 `:9278`、踢人 `:9347`、组队召唤 `:2497` 都只认 `[0]`）。
//! 组队是**纯内存**状态：`CharacterInfo` 存档里没有队伍块（`Server/MirDatabase/CharacterInfo.cs`），
//! 所以本作也不落盘。
//!
//! # 本作的状态模型
//!
//! | Crystal | 本作 |
//! |---|---|
//! | `MapObject.GroupMembers`（共享 `List<PlayerObject>` 引用，`MapObject.cs:162`） | [`World::groups`]：`HashMap<组 ID, Group>` + 每个 [`Player`] 的 `group_id` |
//! | `GroupMembers[0]` 是队长 | [`Group::leader`] = `members[0]`（顺序照抄：创建者永远在最前，`AddMember` 追加 `:9468`） |
//! | `PlayerObject.GroupInvitation`（邀请者对象引用，`PlayerObject.cs:173`） | [`Player::group_invitation`]：`Option<GroupInvite>`（邀请者对象 ID + 过期时刻） |
//! | `PlayerObject.NextGroupInviteTime`（`:15`） | [`Player::group_invite_time`] |
//! | `CharacterInfo.AllowGroup`（`CharacterInfo.cs:62`） | [`Player::allow_group`]（**不持久化**，见文档「与 Crystal 的差异」） |
//!
//! 成员存的是**对象 ID**（`object_id`）而不是角色 ID：Crystal 的 `GroupMembers` 存的就是
//! `PlayerObject`（在线对象），而本作「下线 = 退出队伍」（`PlayerObject.StopGame → LeaveGroup`，
//! `PlayerObject.cs:406`），所以组里不可能出现离线角色；对象 ID 在每个世界实例内唯一，
//! 与 `World::players` 的键一致，不必再做 `角色 ID → 在线对象` 的二次查找。
//!
//! # 包序
//!
//! 每个函数都照抄 Crystal 的 `Enqueue` 顺序，详见各方法的文档与 `docs/world/group.md`。
//! 注意协议里**没有** `S.GroupMembersChanged` / `C.GroupLeave` / `C.GroupInvite{Name}`：
//!
//! | Crystal 语义 | 客户端包 | 服务端包 |
//! |---|---|---|
//! | 邀请（按名字） | `C.AddMember{Name}`（索引 60，`MirConnection.cs:1589-1593`） | `S.GroupInvite{Name}`（134） |
//! | 接受/拒绝 | `C.GroupInvite{AcceptInvite}`（62，`:1601-1605`） | `S.AddMember`（135）/ `S.GroupMembersMap`（274）/ `S.SendMemberLocation`（275） |
//! | 退出 / 踢人 | `C.DelMember{Name}`（61，`:1595-1599`） | `S.DeleteMember`（133）/ `S.DeleteGroup`（132） |
//! | 允许/禁止组队开关 | `C.SwitchGroup{AllowGroup}`（59，`:1583-1587`） | `S.SwitchGroup{AllowGroup}`（131） |
//!
//! 也就是说：**邀请 == `AddMember`，退出 == `DelMember(自己的名字)`**（客户端 `GroupDialog` 的
//! 加人/踢人按钮发的就是这两个包，`Client/MirScenes/Dialogs/GroupDialog.cs:181`、`:220`）。
//!
//! # 经验共享
//!
//! `PlayerObject.WinExp`（`:794-845`）在组队时按「同图 + 范围内人数」给每个人加成再按等级分配，
//! 见 [`World::group_exp_shares`] 与 [`PARTY_EXP_RATE`]。
//!
//! # 简化与差异（完整清单见 `docs/world/group.md`）
//!
//! * **邀请有效期**：Crystal 的 `GroupInvitation` 没有超时（只在 `Process` 里检查邀请者是否还在线，
//!   `PlayerObject.cs:480-481`）。本作按任务书要求加了一条 `GROUP_INVITE_EXPIRE_MS = 4s` 的过期
//!   （**本作扩展**）；同时把「邀请者已离线」的邀请当作失效（接受时会清掉并报错，
//!   别人再邀请该玩家时也会清掉，见 [`World::group_invite`]）。
//! * **`AllowGroup`**：Crystal 从角色存档读（`CharacterInfo.cs:249`）。本作不改 `mir-store`，
//!   所以进世界时默认 `true`，并支持 `C.SwitchGroup` 关闭。
//! * **地图进入规则**：`NoGroup` 地图的自动解散（`PlayerObject.cs:1509-1513`）与
//!   `RequiredGroup` 地图的准入（`:1457-1467`）属于「地图进入规则」系统，本作尚未实现；
//!   只有邀请/接受时的 `NoGroup` 校验照抄了（`:9291-9295`、`:9328-9333`、`:9420-9426`）。
//! * **英雄/宠物**：Crystal 入队时会把英雄、宠物的血条同步给队友（`:9454-9464`），本作没有英雄与宠物。
//! * **物品链接**：`ProcessChatItems`（`:2007`）会处理聊天里的物品链接，本作没有链接系统。
//! * **金币共享**：`PlayerObject.WinGold`（`:7642-7672`）只在 `!Settings.DropGold` 时才会被调到；
//!   默认 `Settings.DropGold = true`（`Server/Settings.cs:229`），金币是**落地**的
//!   （`MonsterObject.cs:1169-1190`），所以组队时也没有金币分配——本作与默认配置一致。

use mir_core::Point;
use mir_core::enums::ChatType;
use mir_core::i18n::TextKey;
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{
    AddMember as AddMemberPacket, DeleteGroup as DeleteGroupPacket,
    DeleteMember as DeleteMemberPacket, GroupInvite as GroupInvitePacket, GroupMembersMap,
    ObjectChat, ObjectHealth, SendMemberLocation, SwitchGroup as SwitchGroupPacket,
};

use crate::world::{OBJECT_HEALTH_EXPIRE_SECONDS, Player, World};

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 队伍人数上限：Crystal `Globals.MaxGroup = 15`（`Shared/Globals.cs:21`）。
pub const MAX_GROUP: usize = 15;

/// 两次邀请之间的最短间隔：Crystal `Settings.GroupInviteDelay = 2000`
/// （`Server/Settings.cs:336`，`NextGroupInviteTime = Envir.Time + GroupInviteDelay`，`PlayerObject.cs:9276`）。
pub const GROUP_INVITE_DELAY_MS: i64 = 2_000;

/// 组队经验/任务杀怪共享半径：Crystal `Globals.DataRange = 16`（`Shared/Globals.cs:37`）。
///
/// 注意它与 [`crate::world::VIEW_RANGE`]（本作简化为 10）**不是**同一个数：
/// Crystal 的 `WinExp`/`CheckGroupQuestKill` 用的是 `Globals.DataRange`（`PlayerObject.cs:815`、`:11500`），
/// 这里照抄 16。
pub const GROUP_EXP_RANGE: i32 = 16;

/// 组队经验加成表：Crystal `WinExp` 里的 `partyExpRate`（`PlayerObject.cs:805`）。
///
/// 下标是「范围内队友人数 - 1」，人数超过表长时按最后一项（`:822` 的 `if (nearCount > partyExpRate.Length) nearCount = partyExpRate.Length;`）。
pub const PARTY_EXP_RATE: [f32; 11] = [1.0, 1.3, 1.4, 1.5, 1.6, 1.7, 1.8, 1.9, 2.0, 2.1, 2.2];

/// 邀请的有效期（毫秒）。
///
/// **本作扩展**：Crystal 的组队邀请**没有**超时——`GroupInvitation` 只会在
/// `PlayerObject.Process` 里发现「邀请者已经下线」时被清掉（`PlayerObject.cs:480-481`），
/// 被邀请者可以隔很久再点接受。任务书要求「4 秒有效」，于是这里加了一条 TTL；
/// 另外「邀请者已离线」的邀请也按失效处理（[`World::group_join`] 与 [`World::group_invite`] 都会清理）。
pub const GROUP_INVITE_EXPIRE_MS: i64 = 4_000;

// ---------------------------------------------------------------------------
// 状态
// ---------------------------------------------------------------------------

/// 一支队伍。
///
/// 对应 Crystal 里被所有成员共享的那个 `List<PlayerObject>`；`members[0]` 是队长
/// （创建顺序：邀请者在前，`GroupMembers.Add(this)` 追加接受者，`PlayerObject.cs:9430`、`:9468`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// 队伍 ID（本作内部标识；Crystal 没有这个字段，靠共享引用区分）。
    pub id: u32,
    /// 成员对象 ID，按入队顺序；`members[0]` 是队长。
    pub members: Vec<u32>,
}

impl Group {
    /// 队长（`GroupMembers[0]`，可能为空——空队伍会被立即解散，所以正常状态下是 `Some`）。
    #[must_use]
    pub fn leader(&self) -> Option<u32> {
        self.members.first().copied()
    }

    /// 是否已满（`GroupMembers.Count >= Globals.MaxGroup`，`PlayerObject.cs:9284`）。
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.members.len() >= MAX_GROUP
    }

    /// 成员数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// 是否没有成员。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
}

/// 一条待接受的组队邀请（Crystal `PlayerObject.GroupInvitation`，`PlayerObject.cs:173`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupInvite {
    /// 邀请者的对象 ID。
    pub inviter: u32,
    /// 过期时刻（服务器单调时钟毫秒）。**本作扩展**，见 [`GROUP_INVITE_EXPIRE_MS`]。
    pub expires_at: i64,
}

/// 组队操作失败原因。
///
/// 每个变体都对应 Crystal `PlayerObject.AddMember`/`GroupInvite`/`DelMember` 里的一次
/// `ReceiveChat(...)` + `return`（行号见各方法文档）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GroupError {
    /// 对象不在世界里。
    #[error("对象 {0} 不在世界里")]
    NotInWorld(u32),
    /// 自己不在队伍里（Crystal `ServerTextKeys.NotInGroup`，`PlayerObject.cs:9342-9346`）。
    #[error("你不在队伍中")]
    NotInGroup(u32),
    /// 不是队长（`ServerTextKeys.NotGroupLeader`，`:9278-9282`、`:9347-9351`）。
    #[error("只有队长可以这么做")]
    NotLeader(u32),
    /// 队伍已满（`ServerTextKeys.YouGroupMaxMembers`，`:9284-9288`）。
    #[error("队伍人数已满")]
    GroupFull,
    /// 邀请冷却中（Crystal 是**静默** `return`，`:9275`）。
    #[error("邀请太频繁")]
    InviteCooldown,
    /// 找不到这个名字的在线玩家（`ServerTextKeys.CouldNotBeFound`，`:9299-9303`）。
    #[error("找不到玩家 {0}")]
    NotFound(String),
    /// 不能邀请自己（`ServerTextKeys.CannotGroupSelf`，`:9304-9308`）。
    #[error("不能邀请自己")]
    CannotInviteSelf,
    /// 对方关闭了组队开关（`ServerTextKeys.NotAllowGroup`，`:9310-9314`）。
    #[error("{0} 不允许组队")]
    TargetNotAllowGroup(String),
    /// 对方已经在别的队伍里（`ServerTextKeys.AlreadyInAnotherGroup`，`:9316-9320`）。
    #[error("{0} 已经在队伍中")]
    TargetAlreadyInGroup(String),
    /// 对方正被其他人邀请（`ServerTextKeys.AlreadyReceivingInviteFromOtherPlayer`，`:9322-9326`）。
    #[error("{0} 正在接收其他人的组队邀请")]
    TargetAlreadyInvited(String),
    /// 邀请者在禁止组队的地图上（`ServerTextKeys.YouCannotInviteOnSoloMaps`，`:9291-9295`）。
    #[error("地图 {0} 上不能组队")]
    InviterOnSoloMap(String),
    /// 被邀请者在禁止组队的地图上（`ServerTextKeys.SoloMapTargetCannotAccept`，`:9328-9333`）。
    #[error("目标在地图 {0} 上，无法组队")]
    TargetOnSoloMap(String),
    /// 没有待接受的邀请（`ServerTextKeys.NotInvitedToGroup`，`:9374-9378`）。
    #[error("没有待接受的组队邀请")]
    NoInvitation(u32),
    /// 邀请已过期（**本作扩展**，见 [`GROUP_INVITE_EXPIRE_MS`]）。
    #[error("邀请已过期")]
    InvitationExpired,
    /// 接受时自己已经在队伍里（`ServerTextKeys.CannotJoinGroup`，`:9387-9392`）。
    #[error("你已经在队伍中")]
    AlreadyInGroup(u32),
    /// 邀请者不再是队长（`ServerTextKeys.NoLongerGroupLeader`，`:9394-9399`）。
    #[error("邀请者已经不是队长")]
    InviterNotLeader,
    /// 邀请者关掉了组队开关（`ServerTextKeys.NotInAllowGroup`，`:9407-9412`）。
    #[error("邀请者不再允许组队")]
    InviterNotAllowGroup,
    /// 邀请者已下线（`ServerTextKeys.PlayerNoLongerOnline`，`:9413-9418`）。
    #[error("邀请者已经下线")]
    InviterOffline,
    /// 目标不在自己的队伍里（`ServerTextKeys.NotInYourGroup`，`:9362-9366`）。
    #[error("{0} 不在你的队伍中")]
    NotInYourGroup(String),
}

/// `C.GroupInvite` 的处理结果（接受成功 / 明确拒绝）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupJoinOutcome {
    /// 已加入（值是队伍 ID）。
    Joined(u32),
    /// 明确拒绝；邀请者收到系统提示，队伍状态不变。
    Declined,
}

// ---------------------------------------------------------------------------
// 帧构造函数
// ---------------------------------------------------------------------------

/// `S.AddMember{Name}`（`ServerPackets.cs` 索引 135；客户端把它追加到组队列表，
/// `Client/MirScenes/GameScene.cs:5006-5009`）。
fn add_member_frame(name: &str) -> Vec<u8> {
    ServerPacket::AddMember(AddMemberPacket {
        name: name.to_owned(),
    })
    .encode_frame()
}

/// `S.DeleteMember{Name}`（索引 133；客户端从组队列表移除该名字，`GameScene.cs:4986-4990`）。
fn delete_member_frame(name: &str) -> Vec<u8> {
    ServerPacket::DeleteMember(DeleteMemberPacket {
        name: name.to_owned(),
    })
    .encode_frame()
}

/// `S.DeleteGroup{}`（索引 132；客户端清空组队列表，`GameScene.cs:4978-4982`）。
fn delete_group_frame() -> Vec<u8> {
    ServerPacket::DeleteGroup(DeleteGroupPacket {}).encode_frame()
}

/// `S.GroupMembersMap{PlayerName, PlayerMap}`（索引 274；组队窗口里显示的所在地图，`GameScene.cs:5011-5019`）。
fn group_members_map_frame(name: &str, map: &str) -> Vec<u8> {
    ServerPacket::GroupMembersMap(GroupMembersMap {
        player_name: name.to_owned(),
        player_map: map.to_owned(),
    })
    .encode_frame()
}

/// `S.SendMemberLocation{MemberName, MemberLocation}`（索引 275；组队窗口的「定位」）。
fn member_location_frame(name: &str, location: Point) -> Vec<u8> {
    ServerPacket::SendMemberLocation(SendMemberLocation {
        member_name: name.to_owned(),
        member_location: location,
    })
    .encode_frame()
}

/// `S.ObjectHealth{ObjectID, Percent, Expire}`（组队血条）。
///
/// Crystal 的 `Expire = Math.Min(byte.MaxValue, (byte)Math.Max(5, (RevTime - Envir.Time) / 1000))`
/// （`MapObject.cs:845`）；本作玩家没有复活计时（`revive_time` 由服务端安排），
/// 取 `max(5, …)` 的下限 [`OBJECT_HEALTH_EXPIRE_SECONDS`]。
fn object_health_frame(object_id: u32, percent: u8) -> Vec<u8> {
    ServerPacket::ObjectHealth(ObjectHealth {
        object_id,
        percent,
        expire: OBJECT_HEALTH_EXPIRE_SECONDS,
    })
    .encode_frame()
}

// ---------------------------------------------------------------------------
// 地图信息小工具
// ---------------------------------------------------------------------------

/// 地图标题：Crystal `CurrentMap.Info.Title`（`PlayerObject.cs:9432`、`:9494`）。
///
/// 简化：没有 `GameData`（单元测试/无数据库服务器）或标题为空时用地图文件名兜底——
/// Crystal 的 `MapInfo.Title` 永远有值，本作的地图表可能没有加载。
fn map_title(world: &World, map: &str) -> String {
    world
        .game_data
        .as_ref()
        .and_then(|data| data.map_info(map))
        .map(|info| info.title.clone())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| map.to_owned())
}

/// 地图的 `BigMap` 字段（`MapInfo.BigMap`，`Server/MirDatabase/MapInfo.cs`）。
///
/// Crystal 只在 `BigMap > 0` 的图上给队友发成员位置（`HumanObject.cs:2735`）。
fn map_big_map(world: &World, map: &str) -> u16 {
    world
        .game_data
        .as_ref()
        .and_then(|data| data.map_info(map))
        .map_or(0, |info| info.big_map)
}

/// 地图是否禁止组队（`MapInfo.NoGroup`；Crystal `CurrentMap.Info.NoGroup`，`PlayerObject.cs:9291`）。
fn map_no_group(world: &World, map: &str) -> bool {
    world
        .game_data
        .as_ref()
        .and_then(|data| data.map_info(map))
        .is_some_and(|info| info.no_group)
}

/// 血量百分比：Crystal `MapObject.PercentHealth = (byte)(Health / (float)MaxHealth * 100)`
/// （`MapObject.cs:51-54`）。
fn percent_health(hp: i32, max_hp: i32) -> u8 {
    if max_hp <= 0 {
        return 0;
    }
    let percent = i64::from(hp) * 100 / i64::from(max_hp);
    u8::try_from(percent.clamp(0, 100)).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// World：队伍查询
// ---------------------------------------------------------------------------

impl World {
    /// 查找某个玩家所在队伍。
    #[must_use]
    pub fn group(&self, player_id: u32) -> Option<&Group> {
        let group_id = self.players.get(&player_id)?.group_id?;
        self.groups.get(&group_id)
    }

    /// 某个玩家所在队伍的成员对象 ID（不在队伍里返回空切片）。
    #[must_use]
    pub fn group_members(&self, player_id: u32) -> &[u32] {
        self.group(player_id)
            .map_or(&[][..], |group| group.members.as_slice())
    }

    /// 某个玩家是否在队伍里（Crystal `GroupMembers != null`）。
    #[must_use]
    pub fn in_group(&self, player_id: u32) -> bool {
        self.players
            .get(&player_id)
            .is_some_and(|player| player.group_id.is_some())
    }

    /// 某个玩家的队伍 ID。
    #[must_use]
    pub fn group_id_of(&self, player_id: u32) -> Option<u32> {
        self.players.get(&player_id)?.group_id
    }

    /// 某个玩家是否队伍里「允许被邀请/邀请」的那个人（Crystal `PlayerObject.AllowGroup`）。
    #[must_use]
    pub fn allow_group_of(&self, player_id: u32) -> Option<bool> {
        self.players
            .get(&player_id)
            .map(|player| player.allow_group)
    }

    /// 某个玩家收到的组队邀请（Crystal `GroupInvitation`）。
    #[must_use]
    pub fn group_invitation_of(&self, player_id: u32) -> Option<GroupInvite> {
        self.players.get(&player_id)?.group_invitation
    }

    /// 分配一个新的队伍 ID。
    fn allocate_group_id(&mut self) -> u32 {
        let id = self.next_group_id;
        self.next_group_id = self.next_group_id.wrapping_add(1).max(1);
        id
    }

    /// 按角色名找在线玩家（Crystal `Envir.GetPlayer(name)`，`Server/MirEnvir/Envir.cs:4696-4703`，
    /// `string.Compare(..., OrdinalIgnoreCase)`）。
    ///
    /// 简化：Rust 的 `eq_ignore_ascii_case` 只折叠 ASCII 大小写；角色名里的汉字没有大小写，
    /// 所以与 Crystal 的 `OrdinalIgnoreCase` 在实际数据上等价。
    #[must_use]
    pub fn player_by_name(&self, name: &str) -> Option<&Player> {
        self.players
            .values()
            .find(|player| player.name.eq_ignore_ascii_case(name))
    }

    /// 清掉某个玩家的待接受邀请（返回被清掉的那条）。
    fn take_invitation(&mut self, player_id: u32) -> Option<GroupInvite> {
        let player = self.players.get_mut(&player_id)?;
        player.group_invitation.take()
    }
}

// ---------------------------------------------------------------------------
// World：组队流程
// ---------------------------------------------------------------------------

impl World {
    /// 开关组队（`C.SwitchGroup`，Crystal `PlayerObject.SwitchGroup`，`PlayerObject.cs:9217-9227`）。
    ///
    /// 包序：**先**给本人回 `S.SwitchGroup{AllowGroup}`（`:9219`），再比较新旧值；
    /// 值没变就直接返回（但仍然发了包）。关掉开关时如果正在队伍里，立刻退队（`:9224-9226`）。
    ///
    /// 返回值：`true` 表示状态真的变了。
    ///
    /// # Errors
    /// 玩家不在世界里时返回 [`GroupError::NotInWorld`]。
    pub fn switch_group(&mut self, player_id: u32, allow: bool) -> Result<bool, GroupError> {
        if !self.players.contains_key(&player_id) {
            return Err(GroupError::NotInWorld(player_id));
        }
        self.send_to_player(
            player_id,
            &ServerPacket::SwitchGroup(SwitchGroupPacket { allow_group: allow }).encode_frame(),
        );
        let Some(player) = self.players.get_mut(&player_id) else {
            return Err(GroupError::NotInWorld(player_id));
        };
        if player.allow_group == allow {
            return Ok(false);
        }
        player.allow_group = allow;
        if !allow {
            self.leave_group(player_id);
        }
        Ok(true)
    }

    /// 邀请组队（`C.AddMember`，Crystal `PlayerObject.AddMember`，`PlayerObject.cs:9273-9338`）。
    ///
    /// 校验顺序照抄源码（**冷却最先扣掉，即使后面校验失败也不退**）：
    ///
    /// | # | 检查 | 失败时给邀请者的提示 | 出处 |
    /// |---|---|---|---|
    /// | 1 | 邀请冷却（`NextGroupInviteTime`） | **无**（静默返回） | `:9275-9276` |
    /// | 2 | 自己在队伍里但不是队长 | 「你不是队长」 | `:9278-9282` |
    /// | 3 | 队伍人数 `>= MaxGroup` | 「你的队伍人数已满」 | `:9284-9288` |
    /// | 4 | 自己在 `NoGroup` 地图 | 「当前地图不能邀请」 | `:9291-9295` |
    /// | 5 | 名字找不到 | 「找不到 XXX」 | `:9297-9303` |
    /// | 6 | 目标是自己 | 「不能邀请自己」 | `:9304-9308` |
    /// | 7 | 目标 `AllowGroup == false` | 「XXX 不允许组队」 | `:9310-9314` |
    /// | 8 | 目标已在队伍里 | 「XXX 已经在队伍中」 | `:9316-9320` |
    /// | 9 | 目标正被其他人邀请 | 「XXX 正在接收其他人的邀请」 | `:9322-9326` |
    /// | 10 | 目标在 `NoGroup` 地图 | 双方各一条提示 | `:9328-9333` |
    ///
    /// 全部通过后：`SwitchGroup(true)`（邀请者收到 `S.SwitchGroup{true}`，`:9335`）→
    /// 目标收到 `S.GroupInvite{Name = 邀请者名}`（`:9336`）→ 记下 `GroupInvitation`（`:9337`）。
    ///
    /// # Errors
    /// 见上表；每个失败都对应一个 [`GroupError`] 变体。
    pub fn group_invite(&mut self, inviter_id: u32, name: &str) -> Result<u32, GroupError> {
        let now = self.now();
        let Some(inviter) = self.players.get(&inviter_id) else {
            return Err(GroupError::NotInWorld(inviter_id));
        };
        // `if (Envir.Time < NextGroupInviteTime) return;`（`:9275`）——连提示都没有。
        if now < inviter.group_invite_time {
            return Err(GroupError::InviteCooldown);
        }
        // `NextGroupInviteTime = Envir.Time + Settings.GroupInviteDelay;`（`:9276`）
        let inviter_name = inviter.name.clone();
        let inviter_map = inviter.map.clone();
        if let Some(inviter) = self.players.get_mut(&inviter_id) {
            inviter.group_invite_time = now + GROUP_INVITE_DELAY_MS;
        }

        // `if (GroupMembers != null && GroupMembers[0] != this)`（`:9278`）
        if let Some(group) = self.group(inviter_id) {
            if group.leader() != Some(inviter_id) {
                self.send_system_chat(
                    inviter_id,
                    &self.text(inviter_id, TextKey::GroupInviteNotLeader),
                );
                return Err(GroupError::NotLeader(inviter_id));
            }
            // `if (GroupMembers != null && GroupMembers.Count >= Globals.MaxGroup)`（`:9284`）
            if group.is_full() {
                self.send_system_chat(inviter_id, &self.text(inviter_id, TextKey::GroupInviteFull));
                return Err(GroupError::GroupFull);
            }
        }
        // `if (CurrentMap != null && CurrentMap.Info.NoGroup)`（`:9291`）
        if map_no_group(self, &inviter_map) {
            self.send_system_chat(
                inviter_id,
                &self.text(inviter_id, TextKey::GroupInviteMapForbidden),
            );
            return Err(GroupError::InviterOnSoloMap(inviter_map));
        }

        // `PlayerObject player = Envir.GetPlayer(name);`（`:9297`）
        let Some(target_id) = self.player_by_name(name).map(|player| player.object_id) else {
            self.send_system_chat(
                inviter_id,
                &self.format(
                    inviter_id,
                    TextKey::GroupInviteTargetMissing,
                    &[("name", name)],
                ),
            );
            return Err(GroupError::NotFound(name.to_owned()));
        };
        let target = self
            .players
            .get(&target_id)
            .expect("player_by_name 返回的 ID 一定在 players 里");
        // `if (player == this)`（`:9304`）
        if target_id == inviter_id {
            self.send_system_chat(inviter_id, &self.text(inviter_id, TextKey::GroupInviteSelf));
            return Err(GroupError::CannotInviteSelf);
        }
        let target_name = target.name.clone();
        let target_map = target.map.clone();
        // `if (!player.AllowGroup)`（`:9310`）
        if !target.allow_group {
            self.send_system_chat(
                inviter_id,
                &self.format(
                    inviter_id,
                    TextKey::GroupInviteTargetDisallowed,
                    &[("target_name", &target_name)],
                ),
            );
            return Err(GroupError::TargetNotAllowGroup(target_name));
        }
        // `if (player.GroupMembers != null)`（`:9316`）
        if target.group_id.is_some() {
            self.send_system_chat(
                inviter_id,
                &self.format(
                    inviter_id,
                    TextKey::GroupInviteTargetInGroup,
                    &[("target_name", &target_name)],
                ),
            );
            return Err(GroupError::TargetAlreadyInGroup(target_name));
        }
        // `if (player.GroupInvitation != null)`（`:9322`）
        //
        // Crystal 靠 `Process` 每 tick 把「邀请者已离线」的邀请清掉（`:480-481`），本作没有
        // 每 tick 的邀请轮询，于是在这里顺手做过期/失效判断：失效的邀请不算「正被邀请」。
        if let Some(invitation) = target.group_invitation {
            let stale =
                now > invitation.expires_at || !self.players.contains_key(&invitation.inviter);
            if stale {
                if let Some(player) = self.players.get_mut(&target_id) {
                    player.group_invitation = None;
                }
            } else {
                self.send_system_chat(
                    inviter_id,
                    &self.format(
                        inviter_id,
                        TextKey::GroupInviteTargetBusy,
                        &[("target_name", &target_name)],
                    ),
                );
                return Err(GroupError::TargetAlreadyInvited(target_name));
            }
        }
        // `if (player.CurrentMap != null && player.CurrentMap.Info.NoGroup)`（`:9328`）
        if map_no_group(self, &target_map) {
            self.send_system_chat(
                inviter_id,
                &self.format(
                    inviter_id,
                    TextKey::GroupInviteTargetMapForbidden,
                    &[("target_name", &target_name)],
                ),
            );
            self.send_system_chat(
                target_id,
                &self.text(target_id, TextKey::GroupJoinMapForbidden),
            );
            return Err(GroupError::TargetOnSoloMap(target_map));
        }

        // `SwitchGroup(true);`（`:9335`）：邀请者每次邀请都会收到一条 `S.SwitchGroup{true}`。
        self.switch_group(inviter_id, true)
            .map_err(|_| GroupError::NotInWorld(inviter_id))?;
        // `player.Enqueue(new S.GroupInvite { Name = Name });`（`:9336`）
        self.send_to_player(
            target_id,
            &ServerPacket::GroupInvite(GroupInvitePacket { name: inviter_name }).encode_frame(),
        );
        // `player.GroupInvitation = this;`（`:9337`）
        if let Some(target) = self.players.get_mut(&target_id) {
            target.group_invitation = Some(GroupInvite {
                inviter: inviter_id,
                expires_at: now + GROUP_INVITE_EXPIRE_MS,
            });
        }
        Ok(target_id)
    }

    /// 接受/拒绝组队邀请（`C.GroupInvite`，Crystal `PlayerObject.GroupInvite`，`PlayerObject.cs:9372-9485`）。
    ///
    /// 拒绝（`:9380-9385`）：给邀请者一条系统提示「XXX 拒绝了你的组队邀请」，清掉邀请，状态不变。
    ///
    /// 接受时的校验顺序（`:9387-9426`）：自己已在队伍 → 邀请者不再是队长 → 邀请者队伍已满 →
    /// 邀请者关闭了组队 → 邀请者下线 → 邀请者所在地图禁止组队。全部通过后建队/入队，
    /// 包序见 `docs/world/group.md`（`S.AddMember` × N、`S.GroupMembersMap`、`S.SendMemberLocation`、
    /// 同图近距离还会互发 `S.ObjectHealth`）。
    ///
    /// # Errors
    /// 没有邀请时返回 [`GroupError::NoInvitation`]；邀请过期返回 [`GroupError::InvitationExpired`]；
    /// 其余见上。
    pub fn group_join(
        &mut self,
        invitee_id: u32,
        accept: bool,
    ) -> Result<GroupJoinOutcome, GroupError> {
        let now = self.now();
        let Some(invitee) = self.players.get(&invitee_id) else {
            return Err(GroupError::NotInWorld(invitee_id));
        };
        let invitee_name = invitee.name.clone();
        // `if (GroupInvitation == null)`（`:9374`）
        let Some(invitation) = invitee.group_invitation else {
            self.send_system_chat(
                invitee_id,
                &self.text(invitee_id, TextKey::GroupJoinNoInvite),
            );
            return Err(GroupError::NoInvitation(invitee_id));
        };
        let inviter_id = invitation.inviter;

        // `if (!accept)`（`:9380-9385`）
        if !accept {
            self.take_invitation(invitee_id);
            if self.players.contains_key(&inviter_id) {
                self.send_system_chat(
                    inviter_id,
                    &self.format(
                        inviter_id,
                        TextKey::GroupJoinDeclined,
                        &[("invitee_name", &invitee_name)],
                    ),
                );
            }
            return Ok(GroupJoinOutcome::Declined);
        }

        // 本作扩展：邀请 TTL（Crystal 没有超时，见 `GROUP_INVITE_EXPIRE_MS`）。
        if now > invitation.expires_at {
            self.take_invitation(invitee_id);
            self.send_system_chat(
                invitee_id,
                &self.text(invitee_id, TextKey::GroupJoinExpired),
            );
            return Err(GroupError::InvitationExpired);
        }

        // `if (GroupMembers != null)`（`:9387`）
        if self.in_group(invitee_id) {
            let inviter_name = self
                .players
                .get(&inviter_id)
                .map(|player| player.name.clone())
                .unwrap_or_default();
            self.take_invitation(invitee_id);
            self.send_system_chat(
                invitee_id,
                &self.format(
                    invitee_id,
                    TextKey::GroupJoinAlreadyInGroup,
                    &[("inviter_name", &inviter_name)],
                ),
            );
            return Err(GroupError::AlreadyInGroup(invitee_id));
        }

        // `if (GroupInvitation.Node == null)`（`:9413`）——邀请者下线。
        let Some(inviter) = self.players.get(&inviter_id) else {
            self.take_invitation(invitee_id);
            self.send_system_chat(
                invitee_id,
                &self.text(invitee_id, TextKey::GroupJoinInviterOffline),
            );
            return Err(GroupError::InviterOffline);
        };
        let inviter_name = inviter.name.clone();
        let inviter_map = inviter.map.clone();
        let inviter_position = inviter.position;
        let inviter_allow_group = inviter.allow_group;
        let existing_group = self.group(inviter_id).cloned();

        // `if (GroupInvitation.GroupMembers != null && GroupInvitation.GroupMembers[0] != GroupInvitation)`（`:9394`）
        if let Some(group) = &existing_group {
            if group.leader() != Some(inviter_id) {
                self.take_invitation(invitee_id);
                self.send_system_chat(
                    invitee_id,
                    &self.format(
                        invitee_id,
                        TextKey::GroupJoinInviterNotLeader,
                        &[("inviter_name", &inviter_name)],
                    ),
                );
                return Err(GroupError::InviterNotLeader);
            }
            // `if (GroupInvitation.GroupMembers.Count >= Globals.MaxGroup)`（`:9401`）
            if group.is_full() {
                self.take_invitation(invitee_id);
                self.send_system_chat(
                    invitee_id,
                    &self.format(
                        invitee_id,
                        TextKey::GroupJoinInviterFull,
                        &[("inviter_name", &inviter_name)],
                    ),
                );
                return Err(GroupError::GroupFull);
            }
        }
        // `if (!GroupInvitation.AllowGroup)`（`:9407`）
        if !inviter_allow_group {
            self.take_invitation(invitee_id);
            self.send_system_chat(
                invitee_id,
                &self.format(
                    invitee_id,
                    TextKey::GroupJoinInviterDisallowed,
                    &[("inviter_name", &inviter_name)],
                ),
            );
            return Err(GroupError::InviterNotAllowGroup);
        }
        // `if (GroupInvitation.CurrentMap.Info.NoGroup)`（`:9420`）
        if map_no_group(self, &inviter_map) {
            self.take_invitation(invitee_id);
            self.send_system_chat(
                invitee_id,
                &self.format(
                    invitee_id,
                    TextKey::GroupJoinInviterMapForbidden,
                    &[("inviter_name", &inviter_name)],
                ),
            );
            self.send_system_chat(
                inviter_id,
                &self.text(inviter_id, TextKey::GroupMapForbidden),
            );
            return Err(GroupError::InviterOnSoloMap(inviter_map));
        }

        // `GroupInvitation = null;`（`:9438`）：邀请在入队成功后失效。
        self.take_invitation(invitee_id);

        // 新队：邀请者自己先成为第一个成员（`:9428-9434`）。
        let group_id = if let Some(group) = existing_group {
            group.id
        } else {
            let group_id = self.allocate_group_id();
            self.groups.insert(
                group_id,
                Group {
                    id: group_id,
                    members: vec![inviter_id],
                },
            );
            if let Some(inviter) = self.players.get_mut(&inviter_id) {
                inviter.group_id = Some(group_id);
            }
            let inviter_title = map_title(self, &inviter_map);
            // `GroupInvitation.Enqueue(new S.AddMember { Name = GroupInvitation.Name });`（`:9431`）
            self.send_to_player(inviter_id, &add_member_frame(&inviter_name));
            // `GroupInvitation.Enqueue(new S.GroupMembersMap { ... });`（`:9432`）
            self.send_to_player(
                inviter_id,
                &group_members_map_frame(&inviter_name, &inviter_title),
            );
            // `GroupInvitation.Enqueue(new S.SendMemberLocation { ... });`（`:9433`）
            // 注意这一条**不看 BigMap**（`GetPlayerLocation` 才看），照抄源码无条件发。
            self.send_to_player(
                inviter_id,
                &member_location_frame(&inviter_name, inviter_position),
            );
            group_id
        };

        // `Packet p = new S.AddMember { Name = Name };`（`:9436`）——内容是**接受者**的名字。
        let add_joiner = add_member_frame(&invitee_name);
        let invitee_map = self.players[&invitee_id].map.clone();
        let invitee_position = self.players[&invitee_id].position;
        let invitee_percent = {
            let player = &self.players[&invitee_id];
            percent_health(player.hp, player.stats.hp)
        };
        // `for (int i = 0; i < GroupMembers.Count; i++)`（`:9440-9466`）：
        // 此时列表里还没有接受者自己（Crystal 在 `:9468` 才 Add）。
        let existing_members = self.groups[&group_id].members.clone();
        for member_id in &existing_members {
            let (member_name, member_map, member_percent, member_position) = {
                let Some(member) = self.players.get(member_id) else {
                    continue;
                };
                (
                    member.name.clone(),
                    member.map.clone(),
                    percent_health(member.hp, member.stats.hp),
                    member.position,
                )
            };
            // `member.Enqueue(p);`（`:9444`）：老队员收到「新队员入队」。
            self.send_to_player(*member_id, &add_joiner);
            // `Enqueue(new S.AddMember { Name = member.Name });`（`:9445`）：接受者收到每个老队员。
            self.send_to_player(invitee_id, &add_member_frame(&member_name));
            // `if (CurrentMap == member.CurrentMap && Functions.InRange(CurrentLocation, member.CurrentLocation, Globals.DataRange))`（`:9447`）
            if invitee_map == member_map
                && invitee_position.in_range(member_position, GROUP_EXP_RANGE)
            {
                // 老队员收到**接受者**的血条（`:9451`），接受者收到**老队员**的血条（`:9452`）。
                self.send_to_player(
                    *member_id,
                    &object_health_frame(invitee_id, invitee_percent),
                );
                self.send_to_player(invitee_id, &object_health_frame(*member_id, member_percent));
                // 简化：Crystal 还会同步双方英雄与宠物的血条（`:9454-9464`），本作没有英雄/宠物。
            }
        }

        // `GroupMembers.Add(this);`（`:9468`）
        if let Some(group) = self.groups.get_mut(&group_id) {
            group.members.push(invitee_id);
        }
        if let Some(invitee) = self.players.get_mut(&invitee_id) {
            invitee.group_id = Some(group_id);
        }
        // `Enqueue(p);`（`:9473`）：接受者也会收到一条「自己入队」。
        self.send_to_player(invitee_id, &add_joiner);
        // `GroupMemberMapNameChanged();`（`:9474`）
        self.group_member_map_name_changed(invitee_id);
        // `GetPlayerLocation();`（`:9475`）
        self.group_player_location(invitee_id);
        tracing::debug!(
            group = group_id,
            invitee = invitee_id,
            inviter = inviter_id,
            members = self.groups[&group_id].members.len(),
            "组队：新成员入队"
        );
        Ok(GroupJoinOutcome::Joined(group_id))
    }

    /// 退出队伍（Crystal `PlayerObject.LeaveGroup`，`PlayerObject.cs:9229-9271`）。
    ///
    /// 离队者再收到一条 `S.DeleteGroup`（`:9255`）；剩下的成员：
    /// * 还有 **2 人及以上** → 每人一条 `S.DeleteMember{离队者}`（`:9240-9245`），队伍保留，
    ///   队长顺延为剩下成员里的第一个（Crystal 只看 `GroupMembers[0]`）；
    /// * 只剩 **1 人** → 那个人收到 `S.DeleteGroup` 并被清空队伍（`:9246-9251`），队伍解散；
    /// * 一个人都不剩 → 队伍直接销毁。
    ///
    /// 返回值：`true` 表示确实退了一支队伍。
    ///
    /// 下线时会走到这里（`PlayerObject.StopGame` → `LeaveGroup()`，`PlayerObject.cs:406`）。
    pub fn leave_group(&mut self, player_id: u32) -> bool {
        let Some(group_id) = self.players.get(&player_id).and_then(|p| p.group_id) else {
            return false;
        };
        let Some(name) = self.players.get(&player_id).map(|p| p.name.clone()) else {
            return false;
        };
        let Some(group) = self.groups.get_mut(&group_id) else {
            // 状态不一致：把玩家的 group_id 清掉就行。
            if let Some(player) = self.players.get_mut(&player_id) {
                player.group_id = None;
            }
            return false;
        };
        group.members.retain(|member| *member != player_id);
        let remaining = group.members.clone();
        if let Some(player) = self.players.get_mut(&player_id) {
            player.group_id = None;
        }

        match remaining.len() {
            0 => {
                self.groups.remove(&group_id);
            }
            1 => {
                let last = remaining[0];
                // `GroupMembers[0].Enqueue(new S.DeleteGroup()); GroupMembers[0].GroupMembers = null;`（`:9249-9250`）
                self.send_to_player(last, &delete_group_frame());
                if let Some(player) = self.players.get_mut(&last) {
                    player.group_id = None;
                }
                self.groups.remove(&group_id);
            }
            _ => {
                let packet = delete_member_frame(&name);
                for member in &remaining {
                    // `GroupMembers[i]?.Enqueue(p);`（`:9243-9244`）
                    self.send_to_player(*member, &packet);
                }
            }
        }
        // `GroupMembers = null; Enqueue(new S.DeleteGroup());`（`:9254-9255`）
        self.send_to_player(player_id, &delete_group_frame());
        // 退队后邀请别人/被邀请的缓存一并作废：Crystal 不处理 `NextGroupInviteTime`，
        // 这里也不清（冷却照旧）。
        true
    }

    /// 踢人 / 自己退队（`C.DelMember`，Crystal `PlayerObject.DelMember`，`PlayerObject.cs:9340-9370`）。
    ///
    /// Crystal 只有**队长**能踢人（`:9347-9351`）；把自己踢出去就等于退队
    /// （客户端也是这么退队的，`GroupDialog.cs:207-224`）。
    ///
    /// 注意源码的重复包：踢人时先给目标 `S.DeleteGroup`（`:9368`），紧接着的 `LeaveGroup`
    /// 又会给目标发一条 `S.DeleteGroup`（`:9255`）——目标会收到**两条**同样的包，这里照抄。
    ///
    /// # Errors
    /// 不在队伍、不是队长、名字不在队伍里时返回对应错误，并给操作者一条系统提示。
    pub fn group_kick(&mut self, player_id: u32, name: &str) -> Result<u32, GroupError> {
        // `if (GroupMembers == null)`（`:9342`）
        let Some(group) = self.group(player_id).cloned() else {
            self.send_system_chat(
                player_id,
                &self.text(player_id, TextKey::GroupKickNotInGroup),
            );
            return Err(GroupError::NotInGroup(player_id));
        };
        // `if (GroupMembers[0] != this)`（`:9347`）
        if group.leader() != Some(player_id) {
            self.send_system_chat(
                player_id,
                &self.text(player_id, TextKey::GroupKickNotLeader),
            );
            return Err(GroupError::NotLeader(player_id));
        }
        // `for (...) if (string.Compare(GroupMembers[i].Name, name, OrdinalIgnoreCase) != 0) continue;`（`:9355-9360`）
        let target = group.members.iter().copied().find(|member_id| {
            self.players
                .get(member_id)
                .is_some_and(|player| player.name.eq_ignore_ascii_case(name))
        });
        let Some(target) = target else {
            self.send_system_chat(
                player_id,
                &self.format(player_id, TextKey::GroupKickNotMember, &[("name", name)]),
            );
            return Err(GroupError::NotInYourGroup(name.to_owned()));
        };
        // `player.Enqueue(new S.DeleteGroup());`（`:9368`）
        self.send_to_player(target, &delete_group_frame());
        // `player.LeaveGroup();`（`:9369`）
        self.leave_group(target);
        Ok(target)
    }

    /// 组队聊天（聊天内容以 `!!` 开头，Crystal `PlayerObject.Chat` 的组队分支，
    /// `PlayerObject.cs:2001-2013`）。
    ///
    /// 不在队伍里时 Crystal **直接 `return`**（`:2003`）——既不报错也不广播，
    /// 所以这里返回 0，调用方什么也不发。
    ///
    /// 文本格式：`"{名字}: {去掉 !! 后的内容}"`（源码是 `String.Format("{0}:{1}", Name, " " + message.Remove(0, 2))`，
    /// `:2005`）。包是 `S.ObjectChat{ObjectID = 发送者, Type = ChatType.Group}`，**只发给队员**（含自己），
    /// 不走本地图广播。
    ///
    /// 返回实际收到的人数。
    pub fn group_chat(&mut self, player_id: u32, message: &str) -> usize {
        let Some(group) = self.group(player_id) else {
            return 0;
        };
        let Some(player) = self.players.get(&player_id) else {
            return 0;
        };
        let body = message.strip_prefix("!!").unwrap_or(message);
        let text = format!("{}: {body}", player.name);
        let frame = ServerPacket::ObjectChat(ObjectChat {
            object_id: player_id,
            text,
            type_: ChatType::Group,
        })
        .encode_frame();
        let receivers = group
            .members
            .iter()
            .filter(|member| self.players.contains_key(member))
            .count();
        for member in group.members.clone() {
            self.send_to_player(member, &frame);
        }
        receivers
    }

    /// 把「我在哪张图」同步给全队（Crystal `PlayerObject.GroupMemberMapNameChanged`，
    /// `PlayerObject.cs:9487-9498`）。
    ///
    /// 包序（对 `this` 的每个队友发两遍 + 最后再发自己一遍，照抄源码）：
    ///
    /// ```text
    /// for each member:
    ///     member ← S.GroupMembersMap{ this.Name, this.CurrentMap.Title }
    ///     this   ← S.GroupMembersMap{ member.Name, member.CurrentMap.Title }
    /// this ← S.GroupMembersMap{ this.Name, this.CurrentMap.Title }
    /// ```
    ///
    /// 只在**切图**（`CompleteMapMovement`，`:1568`）与**复活**（`:1389`、`:1448`）时调用；
    /// 升级/血量变化**不会**触发它（血量走 `S.ObjectHealth`，见 [`World::broadcast_health_to_group`]）。
    pub fn group_member_map_name_changed(&mut self, player_id: u32) {
        let Some(group) = self.group(player_id).cloned() else {
            return;
        };
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let name = player.name.clone();
        let title = map_title(self, &player.map);
        for member_id in &group.members {
            self.send_to_player(*member_id, &group_members_map_frame(&name, &title));
            let Some(member) = self.players.get(member_id) else {
                continue;
            };
            let member_name = member.name.clone();
            let member_title = map_title(self, &member.map);
            self.send_to_player(
                player_id,
                &group_members_map_frame(&member_name, &member_title),
            );
        }
        self.send_to_player(player_id, &group_members_map_frame(&name, &title));
    }

    /// 把「我在哪个坐标」同步给全队（Crystal `HumanObject.GetPlayerLocation`，`HumanObject.cs:2727-2741`）。
    ///
    /// Crystal 只给 **`BigMap > 0`** 的地图上的队友发（`:2735` 的 `continue`），
    /// 因为只有那些图有「队友定位」窗口。包序同样照抄：
    ///
    /// ```text
    /// for each member:
    ///     if member.CurrentMap.Info.BigMap <= 0: continue
    ///     member ← S.SendMemberLocation{ this.Name, this.CurrentLocation }
    ///     this   ← S.SendMemberLocation{ member.Name, member.CurrentLocation }
    /// this ← S.SendMemberLocation{ this.Name, this.CurrentLocation }
    /// ```
    pub fn group_player_location(&mut self, player_id: u32) {
        let Some(group) = self.group(player_id).cloned() else {
            return;
        };
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let name = player.name.clone();
        let location = player.position;
        for member_id in &group.members {
            let Some(member) = self.players.get(member_id) else {
                continue;
            };
            // `if (member.CurrentMap.Info.BigMap <= 0) continue;`（`HumanObject.cs:2735`）
            if map_big_map(self, &member.map) == 0 {
                continue;
            }
            let member_name = member.name.clone();
            let member_location = member.position;
            self.send_to_player(*member_id, &member_location_frame(&name, location));
            self.send_to_player(
                player_id,
                &member_location_frame(&member_name, member_location),
            );
        }
        self.send_to_player(player_id, &member_location_frame(&name, location));
    }

    /// 把某个玩家的血条同步给同图范围内的队友（Crystal `MapObject.BroadcastHealthChange`
    /// 的玩家分支，`MapObject.cs:856-871`）。
    ///
    /// 只在**组队**时发（这是 Crystal 的原行为：玩家血量变化只通知队友，不广播给周围所有人）。
    pub(crate) fn broadcast_health_to_group(&self, player_id: u32) {
        let Some(group) = self.group(player_id) else {
            return;
        };
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let percent = percent_health(player.hp, player.stats.hp);
        let map = player.map.as_str();
        let position = player.position;
        let frame = object_health_frame(player_id, percent);
        for member_id in &group.members {
            if *member_id == player_id {
                // `if (this == member) continue;`（`MapObject.cs:864`）
                continue;
            }
            let Some(member) = self.players.get(member_id) else {
                continue;
            };
            if member.map != map || !member.position.in_range(position, GROUP_EXP_RANGE) {
                continue;
            }
            self.send_to_player(*member_id, &frame);
        }
    }
}

// ---------------------------------------------------------------------------
// World：经验 / 任务杀怪共享
// ---------------------------------------------------------------------------

impl World {
    /// 组队经验分配表：返回 `(成员对象 ID, 应得经验)`；不在队伍里返回 `None`
    /// （Crystal `WinExp` 的 `if (GroupMembers != null)` 分支，`PlayerObject.cs:807-833`）。
    ///
    /// 公式逐项照抄：
    ///
    /// ```csharp
    /// int sumLevel = 0, nearCount = 0;
    /// for (int i = 0; i < GroupMembers.Count; i++) {         // :811-820
    ///     PlayerObject player = GroupMembers[i];
    ///     if (Functions.InRange(player.CurrentLocation, CurrentLocation, Globals.DataRange)) {
    ///         sumLevel += player.Level;
    ///         nearCount++;
    ///     }
    /// }
    /// if (nearCount > partyExpRate.Length) nearCount = partyExpRate.Length;   // :822
    /// for (...) if (player.CurrentMap == CurrentMap && InRange(...) && !player.Dead)   // :827-828
    ///     player.GainExp((uint)((float)expPoint * partyExpRate[nearCount - 1] * (float)player.Level / (float)sumLevel));
    /// ```
    ///
    /// 也就是说：**人数与等级和只看距离、不看地图与生死**（`:815` 没有 `CurrentMap`/`Dead` 判断，
    /// 只有真正分经验时才过滤同图与死亡，`:827-828`）。这个不对称是 Crystal 的原样行为，
    /// 见 `docs/world/group.md` 的「异常行为」。
    ///
    /// `exp_point` 是**击杀者**已经过 `ReduceExp` 与 `Settings.ExpRate` 的经验值。
    #[must_use]
    pub fn group_exp_shares(&self, killer_id: u32, exp_point: u32) -> Option<Vec<(u32, u32)>> {
        let group = self.group(killer_id)?;
        let killer = self.players.get(&killer_id)?;
        let mut sum_level: u32 = 0;
        let mut near_count: usize = 0;
        for member_id in &group.members {
            let Some(member) = self.players.get(member_id) else {
                continue;
            };
            if member.position.in_range(killer.position, GROUP_EXP_RANGE) {
                sum_level += u32::from(member.level);
                near_count += 1;
            }
        }
        // `if (nearCount > partyExpRate.Length) nearCount = partyExpRate.Length;`（`:822`）
        let index = near_count.min(PARTY_EXP_RATE.len()).max(1) - 1;
        let rate = PARTY_EXP_RATE[index];
        // `sumLevel` 为 0 在 Crystal 里走不到（击杀者一定在自己的 `DataRange` 内，且一定在队伍列表里）；
        // 真的出现就返回空分配，避免除零 panic。
        if sum_level == 0 {
            return Some(Vec::new());
        }
        let mut shares = Vec::with_capacity(group.members.len());
        for member_id in &group.members {
            let Some(member) = self.players.get(member_id) else {
                continue;
            };
            // `player.CurrentMap == CurrentMap`（`:827`）
            if member.map != killer.map {
                continue;
            }
            // `Functions.InRange(player.CurrentLocation, CurrentLocation, Globals.DataRange)`（`:828`）
            if !member.position.in_range(killer.position, GROUP_EXP_RANGE) {
                continue;
            }
            // `!player.Dead`（`:828`）
            if member.dead {
                continue;
            }
            // C# 侧是 `(uint)((float)expPoint * rate * (float)player.Level / (float)sumLevel)`：
            // 全程单精度浮点，最后向零截断。
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            let amount = ((exp_point as f32) * rate * f32::from(member.level) / (sum_level as f32))
                .max(0.0) as u32;
            shares.push((*member_id, amount));
        }
        Some(shares)
    }

    /// 击杀推进任务时应该记账的玩家：队伍里同图、`DataRange` 内、活着的人（含击杀者自己）；
    /// 不在队伍里就只有击杀者。
    ///
    /// 对应 Crystal `PlayerObject.CheckGroupQuestKill`（`PlayerObject.cs:11494-11508`）：
    ///
    /// ```csharp
    /// if (GroupMembers != null) {
    ///     foreach (PlayerObject player in GroupMembers.Where(player =>
    ///         player.CurrentMap == CurrentMap &&
    ///         Functions.InRange(player.CurrentLocation, CurrentLocation, Globals.DataRange) &&
    ///         !player.Dead))
    ///         player.CheckNeedQuestKill(mInfo);
    /// } else CheckNeedQuestKill(mInfo);
    /// ```
    #[must_use]
    pub(crate) fn quest_kill_recipients(&self, killer_id: u32) -> Vec<u32> {
        let Some(group) = self.group(killer_id) else {
            return vec![killer_id];
        };
        let Some(killer) = self.players.get(&killer_id) else {
            return vec![killer_id];
        };
        group
            .members
            .iter()
            .copied()
            .filter(|member_id| {
                self.players.get(member_id).is_some_and(|member| {
                    member.map == killer.map
                        && member.position.in_range(killer.position, GROUP_EXP_RANGE)
                        && !member.dead
                })
            })
            .collect()
    }
}
