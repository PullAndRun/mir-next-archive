# 组队（Party）

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：`Server/MirObjects/PlayerObject.cs` 的 `#region Groups`（`:9215-9533`）、
> 组队聊天（`:2001-2013`）、`WinExp`（`:794-845`）、`GainExp`（`:846-931`）、
> 商店/拾取里的队友分支（`:7501-7514`）、组队召唤（`:2490-2530`）；
> `Server/MirObjects/HumanObject.cs`（`GetPlayerLocation`，`:2727-2741`；
> `SendHealthChanged` → `BroadcastHealthChange`，`:858-874`）、
> `Server/MirObjects/MapObject.cs`（`BroadcastHealthChange` 的玩家分支，`:856-871`）、
> `Server/MirObjects/MonsterObject.cs`（怪物死亡的经验与任务杀怪，`:982-998`）、
> `Server/MirEnvir/Envir.cs`（`GetPlayer(name)`，`:4696-4703`）、
> `Shared/Globals.cs`（`MaxGroup`/`DataRange`）、`Server/Settings.cs`（`GroupInviteDelay`/`ExpRate`）。
>
> 本作实现：`crates/mir-server/src/group.rs`（状态 + 全部流程）、
> `world.rs`（`win_experience` / `chat` / `send_health_changed` / `leave` / `revive_player` 的接入点）、
> `maps.rs`（切图通知）、`monster.rs`（任务杀怪）、`quests.rs`（任务分享）、
> `session.rs`（四个客户端包的处理）。
> 测试：`crates/mir-server/tests/group.rs`（23 个用例）。

---

## 1. 总览

| 主题 | Crystal | 本作 |
|---|---|---|
| 队伍本体 | 每个成员 `MapObject.GroupMembers` 指向**同一个** `List<PlayerObject>`（`MapObject.cs:162`、`PlayerObject.cs:9437`） | `World::groups: HashMap<u32, Group>` + 每个 `Player::group_id` |
| 队长 | 约定 `GroupMembers[0]`（`:9278`、`:9347`） | `Group::leader()` = `members[0]`，顺序同上 |
| 成员身份 | `PlayerObject` 引用（在线对象） | 对象 ID `u32` |
| 待接受邀请 | `PlayerObject.GroupInvitation`（`:173`） | `Player::group_invitation: Option<GroupInvite>` |
| 邀请冷却 | `PlayerObject.NextGroupInviteTime`（`:15`） | `Player::group_invite_time` |
| 组队开关 | `CharacterInfo.AllowGroup`（`CharacterInfo.cs:62`，随存档） | `Player::allow_group`（**不持久化**，默认 `true`） |
| 持久化 | 无（`CharacterInfo` 没有队伍块） | 无（下线即离开队伍） |

> 成员为什么按**对象 ID** 存：Crystal 的 `GroupMembers` 存的就是 `PlayerObject`（在线对象），
> 而本作「下线 = 退队」（`PlayerObject.StopGame → LeaveGroup`，`PlayerObject.cs:406`），
> 队伍里不可能留下离线角色；对象 ID 在世界实例内唯一、与 `World.players` 的键一致，
> 不必再做「角色 ID → 在线对象」的二次查找。队伍 ID 是**本作新增**的内部标识
> （Crystal 靠共享引用区分），用于 `HashMap` 存取与日志。

---

## 2. 协议：真实包名（任务书里的 `GroupMembersChanged` / `GroupLeave` 并不存在）

协议按 Crystal 的 `Shared/ClientPackets.cs` / `Shared/ServerPackets.cs` 逐字生成，
组队相关的包**只有**下面这些（`crates/mir-protocol/src/packets/{client,server}.rs` 已全部就绪）：

| 语义 | 客户端包 | 服务端包 | 客户端处理 |
|---|---|---|---|
| 邀请（按名字） | `C.AddMember{Name}`（索引 60） | `S.GroupInvite{Name}`（134） | `MirConnection.cs:1589-1593`、`:1601-1605` |
| 接受 / 拒绝 | `C.GroupInvite{AcceptInvite}`（62） | `S.AddMember{Name}`（135）、`S.GroupMembersMap{PlayerName,PlayerMap}`（274）、`S.SendMemberLocation{MemberName,MemberLocation}`（275） | `GameScene.cs:4994-5019` |
| 退出 / 踢人 | `C.DelMember{Name}`（61） | `S.DeleteMember{Name}`（133）、`S.DeleteGroup{}`（132） | `GameScene.cs:4978-4990` |
| 组队开关 | `C.SwitchGroup{AllowGroup}`（59） | `S.SwitchGroup{AllowGroup}`（131） | `GameScene.cs:4970-4976` |
| 组队聊天 | `C.Chat{Message="!!…"}` | `S.ObjectChat{ObjectID, Text, Type=Group}`（31） | `ChatDialog`（`ChatType.Group`） |
| 经验 | — | `S.GainExperience{Amount}`（85） | — |

> **报告用要点（任务书与实际协议的差异）**：
> * 任务书写的 `C.GroupInvite { name }` 实际是 `C.AddMember { name }`；`C.GroupInvite` 是**接受/拒绝**包
>   `{ accept_invite: bool }`。
> * 任务书写的 `C.GroupLeave` **不存在**；客户端退队发的是 `C.DelMember{自己名字}`，
>   而服务端 `DelMember` **只允许队长**操作（`:9347-9351`），所以客户端其实只能靠
>   「关掉组队开关」或下线来退队（见 §9 异常清单第 5 条）。
> * 任务书写的 `S.GroupMembersChanged { members }` **不存在**；成员列表由
>   `S.AddMember` / `S.DeleteMember` / `S.DeleteGroup` 三条增量包 + `S.GroupMembersMap`（地图名）维护。
> * 任务书写的 `S.WinExp` 实际是 `S.GainExperience`（索引 85，Crystal `PlayerObject.GainExp` 里的 `Enqueue`，`:893`）。
> * 任务书提到的 `GroupRemove` / `GroupMembersChanged` / `GroupMemberMapChanged` 在源码里都没有；
>   对应的真实函数是 `DelMember`（踢人）与 `GroupMemberMapNameChanged`（地图名刷新）。
> * **协议无需改动**：`crates/mir-protocol` 没有任何缺失的包。

---

## 3. 状态模型（`crates/mir-server/src/group.rs`）

```rust
pub struct Group { pub id: u32, pub members: Vec<u32> }   // members[0] = 队长
pub struct GroupInvite { pub inviter: u32, pub expires_at: i64 }  // expires_at 是本作扩展
```

`World` 侧：

| 字段 | 含义 | Crystal |
|---|---|---|
| `groups: HashMap<u32, Group>` | 所有队伍 | 共享的 `List<PlayerObject>` |
| `next_group_id` | 队伍 ID 递增器 | — |

`Player` 侧（`crates/mir-server/src/world.rs` 的 `Player` 结构体）：

| 字段 | 含义 | Crystal |
|---|---|---|
| `group_id: Option<u32>` | 所在队伍 | `GroupMembers != null` |
| `group_invitation: Option<GroupInvite>` | 待接受邀请 | `GroupInvitation`（`:173`） |
| `group_invite_time: i64` | 下次可邀请时刻 | `NextGroupInviteTime`（`:15`） |
| `allow_group: bool` | 组队开关 | `AllowGroup` → `CharacterInfo.AllowGroup`（`CharacterInfo.cs:62`） |

公开查询 API：

| API | 说明 |
|---|---|
| `World::group(player_id) -> Option<&Group>` | 队伍本体 |
| `World::group_members(player_id) -> &[u32]` | 成员对象 ID（`members[0]` 是队长） |
| `World::group_id_of(player_id) -> Option<u32>` | 队伍 ID |
| `World::in_group(player_id) -> bool` | 是否在队伍里 |
| `World::group_invitation_of(player_id) -> Option<GroupInvite>` | 待接受邀请 |
| `World::allow_group_of(player_id) -> Option<bool>` | 组队开关 |
| `World::player_by_name(name) -> Option<&Player>` | 大小写不敏感的名字查找（`Envir.cs:4696-4703`） |

---

## 4. 组队流程

### 4.1 邀请：`World::group_invite`（`C.AddMember` → `PlayerObject.AddMember`，`:9273-9338`）

校验顺序逐条照抄源码（**冷却最先扣掉，即使后面失败也不退**）：

| # | 检查 | 失败结果 | 出处 |
|---|---|---|---|
| 1 | `Envir.Time < NextGroupInviteTime`（默认 2000ms 冷却） | `Err(InviteCooldown)`，**静默**（没有提示） | `:9275-9276` |
| 2 | 自己在队伍里但不是 `GroupMembers[0]` | `Err(NotLeader)`，提示「你不是队长」 | `:9278-9282` |
| 3 | 队伍 `>= MaxGroup(15)` | `Err(GroupFull)` | `:9284-9288` |
| 4 | 自己在 `NoGroup` 地图 | `Err(InviterOnSoloMap)` | `:9291-9295` |
| 5 | `Envir.GetPlayer(name)` 找不到 | `Err(NotFound)` | `:9297-9303` |
| 6 | 目标是自己 | `Err(CannotInviteSelf)` | `:9304-9308` |
| 7 | 目标 `AllowGroup == false` | `Err(TargetNotAllowGroup)` | `:9310-9314` |
| 8 | 目标已在队伍里 | `Err(TargetAlreadyInGroup)` | `:9316-9320` |
| 9 | 目标正被其他人邀请 | `Err(TargetAlreadyInvited)` | `:9322-9326` |
| 10 | 目标在 `NoGroup` 地图 | `Err(TargetOnSoloMap)`，双方各一条提示 | `:9328-9333` |

全部通过后的包序：

1. `SwitchGroup(true)` → 邀请者 ← `S.SwitchGroup{AllowGroup=true}`（`:9335` → `:9219`；
   即使已经是 `true` 也会发，见 `:9219-9221`）；
2. 目标 ← `S.GroupInvite{Name = 邀请者名}`（`:9336`）；
3. 目标记下 `GroupInvitation = 邀请者`（`:9337`）。

注意：**邀请不建队**，队伍在对方接受时才创建（`:9428-9430`）。

### 4.2 接受 / 拒绝：`World::group_join`（`C.GroupInvite` → `PlayerObject.GroupInvite`，`:9372-9485`）

拒绝（`:9380-9385`）：邀请者 ← 系统提示「XXX 拒绝了你的组队邀请」，清掉邀请，队伍状态不变。

接受时的校验顺序（`:9387-9426`）：

| # | 检查 | 失败结果 |
|---|---|---|
| 1 | 自己没有待接受邀请 | `Err(NoInvitation)` |
| 2 | **本作扩展**：邀请过期（> 4s） | `Err(InvitationExpired)` |
| 3 | 自己已经在队伍里（`:9387`） | `Err(AlreadyInGroup)` |
| 4 | 邀请者已下线（`:9413`） | `Err(InviterOffline)` |
| 5 | 邀请者已不是队长（`:9394`） | `Err(InviterNotLeader)` |
| 6 | 邀请者队伍已满（`:9401`） | `Err(GroupFull)` |
| 7 | 邀请者关掉了组队开关（`:9407`） | `Err(InviterNotAllowGroup)` |
| 8 | 邀请者在 `NoGroup` 地图（`:9420`） | `Err(InviterOnSoloMap)`，双方各一条提示 |

> 校验顺序上的小差异：Crystal 在 `:9413` 才检查邀请者是否在线，本作提前到第 4 步（读邀请者状态前必须
> 先确认在线）；其余顺序一致。

成功的包序（**逐行对照源码**）：

1. 若邀请者原本没有队伍（`:9428-9434`）：
   1. 邀请者 ← `S.AddMember{邀请者}`（`:9431`）
   2. 邀请者 ← `S.GroupMembersMap{邀请者, 邀请者地图标题}`（`:9432`）
   3. 邀请者 ← `S.SendMemberLocation{邀请者, 邀请者坐标}`（`:9433`，这一条**不看 `BigMap`**）
2. 对每个已有队员 `m`（此时列表里还没有接受者）：
   1. `m` ← `S.AddMember{接受者}`（`:9444`）
   2. 接受者 ← `S.AddMember{m}`（`:9445`）
   3. 若 `m` 与接受者同图且切比雪夫距离 ≤ `Globals.DataRange(16)`：
      `m` ← `S.ObjectHealth{接受者, 百分比}`、接受者 ← `S.ObjectHealth{m, 百分比}`（`:9447-9452`；
      英雄/宠物的血条 `:9454-9464` 本作没有）；
3. 接受者 ← `S.AddMember{接受者}`（`:9473`）；
4. `GroupMemberMapNameChanged()`（`:9474` → `:9487-9498`）：
   对每个成员（此时含接受者）`m`：`m` ← `S.GroupMembersMap{接受者, 接受者地图}`、
   接受者 ← `S.GroupMembersMap{m, m 的地图}`；最后接受者 ← `S.GroupMembersMap{接受者, 接受者地图}`
   （因此接受者会收到**三次**自己的地图名，见异常清单第 4 条）；
5. `GetPlayerLocation()`（`:9475` → `HumanObject.cs:2727-2741`）：对每个 `BigMap > 0` 的成员 `m`：
   `m` ← `S.SendMemberLocation{接受者, …}`、接受者 ← `S.SendMemberLocation{m, …}`；
   最后接受者 ← `S.SendMemberLocation{接受者, …}`。

返回值是 `GroupJoinOutcome::{Joined(group_id), Declined}`。

### 4.3 退队 / 踢人：`World::group_kick`（`C.DelMember` → `PlayerObject.DelMember`，`:9340-9370`）

* 只有队长能操作（`:9347-9351`），否则 `Err(NotLeader)`；
* 名字按 `OrdinalIgnoreCase` 在队伍里找（`:9355-9360`），找不到 `Err(NotInYourGroup)`；
* 找到后：目标 ← `S.DeleteGroup`（`:9368`）→ `LeaveGroup()`（`:9369`）。

客户端就是这么退队的：`GroupDialog` 的「减号」按钮弹输入框，把**自己的名字**填进去
（`Client/MirScenes/Dialogs/GroupDialog.cs:207-224`）。

`World::leave_group`（`PlayerObject.LeaveGroup`，`:9229-9271`）按剩余人数分三种情况：

| 剩余人数 | 处理 | 出处 |
|---|---|---|
| ≥ 2 | 每个剩余成员 ← `S.DeleteMember{离队者}`；队伍保留，队长顺延给剩余 `members[0]` | `:9240-9245` |
| 1 | 剩下那个人 ← `S.DeleteGroup` 并把它的 `group_id` 清空，队伍解散 | `:9246-9251` |
| 0 | 队伍记录直接删除 | —（Crystal 里 `GroupMembers` 变成空列表） |

离队者本人总会再收到一条 `S.DeleteGroup`（`:9254-9255`）。因为 `DelMember` 在 `:9368` 已经先发了一条，
**被踢/自己走的人会收到两条 `S.DeleteGroup`**（异常清单第 2 条）。

`World::leave`（下线）在广播 `S.ObjectRemove` 之后调用 `leave_group`（`PlayerObject.cs:403-406`），
所以下线 = 自动退队，队伍里的其他人收到 `S.DeleteMember` 或 `S.DeleteGroup`。

### 4.4 组队开关：`World::switch_group`（`C.SwitchGroup` → `PlayerObject.SwitchGroup`，`:9217-9227`）

1. 先给本人回 `S.SwitchGroup{AllowGroup}`（`:9219`，无论值是否变化都会发）；
2. 值没变 → 直接返回 `false`；
3. 改值；若关掉开关且正在队伍里 → `leave_group`（`:9224-9226`）。

登录时也会补一条 `S.SwitchGroup`（`PlayerObject.cs:1219`），本作在 `session` 的进世界序列
（`UserInformation` 之后）照发。

---

## 5. 组队聊天（`!!`）

Crystal 的 `PlayerObject.Chat`（`:1892` 起）是一条 `if / else if` 链，`!!` 分支在 `:2001-2013`：

```csharp
else if (message.StartsWith("!!")) {
    if (GroupMembers == null) return;                     // 组外：静默丢弃
    message = String.Format("{0}:{1}", Name, " " + message.Remove(0, 2));
    Packet p = new S.ObjectChat { ObjectID = ObjectID, Text = message, Type = ChatType.Group };
    for (int i = 0; i < GroupMembers.Count; i++)
        GroupMembers[i].Enqueue(p);                       // 只发给队员（含自己），不广播
}
```

本作：`World::chat` 检测 `!!` 前缀后转给 `World::group_chat`：

* 文本 = `"{发送者名}: {去掉 !! 的内容}"`；
* 包 = `S.ObjectChat{ObjectID=发送者, Text, Type=ChatType.Group}`；
* 收件人 = 队伍里所有在线成员（含发送者）；同图第三者**收不到**，也不会走本地图广播；
* 不在队伍里 → 返回 0，**什么都不发**（Crystal 既不报错也不提示，见异常清单第 6 条）。

---

## 6. 经验共享

### 6.1 公式（`PlayerObject.WinExp`，`:794-845`）

```csharp
expPoint = ReduceExp(amount, targetLevel);            // :801
expPoint = (int)(expPoint * Settings.ExpRate);        // :802（ExpRate 默认 1.0，Settings.cs:112）
float[] partyExpRate = { 1.0F, 1.3F, 1.4F, 1.5F, 1.6F, 1.7F, 1.8F, 1.9F, 2F, 2.1F, 2.2F };  // :805
if (GroupMembers != null) {
    int sumLevel = 0, nearCount = 0;
    foreach (member) if (Functions.InRange(member.CurrentLocation, CurrentLocation, Globals.DataRange)) { sumLevel += member.Level; nearCount++; }   // :811-820
    if (nearCount > partyExpRate.Length) nearCount = partyExpRate.Length;   // :822
    foreach (member)
        if (member.CurrentMap == CurrentMap && InRange(...) && !member.Dead)   // :827-828
            member.GainExp((uint)((float)expPoint * partyExpRate[nearCount - 1] * (float)member.Level / (float)sumLevel));   // :830
} else GainExp(expPoint);                              // :834-837
```

要点：

* **范围**是 `Globals.DataRange = 16`（`Shared/Globals.cs:37`），不是视野半径
  （本作 `VIEW_RANGE = 10`，见 `crates/mir-server/src/world.rs` 的注释；两者在 Crystal 里也是两个数）；
* `expPoint` **只按击杀者等级衰减一次**，然后整份按等级比例分给队友，不会再各自衰减；
* 人数加成表的**下标是「范围内人数 - 1」**：2 人 → 1.3、3 人 → 1.4、…、11 人以上 → 2.2；
* 每人所得 = `trunc(expPoint × rate × 自己等级 / 范围内等级和)`，**单精度浮点 + 向零截断**
  （C# 的 `(uint)`）；本作`group.rs` 用 `f32` 逐位对齐；
* 每个人走自己的 `GainExp`（`:846-931`）：各自收到 `S.GainExperience`，够阈值就各自升级
  （`S.ObjectLeveled` 广播周围、`S.LevelChanged` 给自己）。

### 6.2 算例（本作用例 `group.rs` 的断言就是这些数）

| 场景 | nearCount | sumLevel | rate | 结果 |
|---|---|---|---|---|
| 两个 1 级同屏 | 2 | 2 | 1.3 | 100 × 1.3 ÷ 2 = **65** 每人 |
| 队友在 16 格外 | 1 | 1 | 1.0 | 击杀者 **100**，队友 0 |
| 三个 1 级同屏 | 3 | 3 | 1.4 | 100 × 1.4 ÷ 3 = **46** 每人（截断） |
| 1 级 + 2 级同屏 | 2 | 3 | 1.3 | 1 级 43、2 级 86 |

### 6.3 本作的接入点

* `World::win_experience` 先算 `ReduceExp`，再调 `World::group_exp_shares`：
  * 不在队伍里 → `None` → `gain_experience(击杀者, expPoint)`；
  * 在队伍里 → 返回 `Vec<(成员, 经验)>`，按成员顺序逐个 `gain_experience`。
* 简化：`Settings.ExpRate` 固定 1.0（未接入配置）；英雄（`:839-844`）、夫妻/师徒/行会加成
  （`GainExp`，`:854-884`）、地图 `NoExperience` 都还没实现。

### 6.4 任务杀怪与任务分享也走队伍

* `PlayerObject.CheckGroupQuestKill`（`:11494-11508`）：队伍里**同图、`DataRange` 内、活着**的成员
  （含击杀者自己）都算击杀。本作在 `monster_died` 里用 `World::quest_kill_recipients` 实现。
* `PlayerObject.ShareQuest`（`:11472-11492`）：把任务分享给**同图、`DataRange` 内、活着、不是自己**的队友
  （`S.ShareQuest{QuestIndex, SharerName}`），一个都没发出去时提示 `QuestNotShared`。
  本作 `World::share_quest` 原先是「没有组队，恒失败」，现在按源码实现。

---

## 7. 其它会随组队变化的行为

| 行为 | Crystal | 本作 |
|---|---|---|
| 血量同步 | `HumanObject.SendHealthChanged`（`:858-874`）→ `BroadcastHealthChange`（`MapObject.cs:841-914`）：**玩家只发给同图 `DataRange` 内的队友**，包是 `S.ObjectHealth{ObjectID, Percent, Expire}` | `World::broadcast_health_to_group`，由 `send_health_changed` 调用 |
| **升级**是否刷新成员列表 | **不刷新**（`WinExp`/`LevelUp` 里没有组队包） | 同 |
| **血量变化**是否刷新成员列表 | **不刷新**成员列表，只发 `S.ObjectHealth` | 同 |
| 切图 | `CompleteMapMovement` 末尾 `GroupMemberMapNameChanged()`（`:1568`）+ `GetPlayerLocation()`（`:4601`） | `World::change_map` 第 10 步调用两者；队伍**保留** |
| 复活 | `Revive` / `TownRevive` 里 `GroupMemberMapNameChanged()` + `GetPlayerLocation()`（`:1389-1390`、`:1448-1449`） | `World::revive_player` 调用两者 |
| 下线 | `StopGame` → `LeaveGroup()`（`:406`） | `World::leave` → `leave_group` |
| 死亡 | Crystal **不处理**队伍关系；只有分经验时跳过死人（`:828`），`BeforeDie`/`Die` 里没有组队逻辑 | 同（死亡不离队、不解散） |
| 拾取提示 | `ShowGroupPickup` 物品被捡时通知队友（`:7512-7514`） | 未实现（物品提示系统不在本次范围） |
| 组队召唤 / 允许召唤 | `GROUPCALL`/`ENABLEGROUPRECALL`（`:2490-2530`） | 未实现（没有 GM 命令与召唤系统） |
| 组队掷骰 | `@Roll` 发给队友（`:2675-2681`） | 未实现（没有骰子命令） |
| 地图进入规则 | `NoGroup` 地图自动解散（`:1509-1513`）、`RequiredGroup` 地图准入（`:1457-1467`） | **未实现**；只在邀请/接受时检查 `NoGroup`（`:9291-9295`、`:9328-9333`、`:9420-9426`） |
| PK / 攻击模式 | `AttackMode.Group` 的敌我判定（`PlayerObject.cs:4661-4662`、`:4703-4704`、`:4724-4725`，`MonsterObject.cs:2445-2449`、`:2548-2549`） | **跳过**：本作还没有 PK 系统与攻击模式（`C.ChangeAMode` 未处理） |
| 金币 | `WinGold`（`:7642-7672`）会按队友平分，但只在 `!Settings.DropGold` 时才会被调；默认 `DropGold = true`（`Settings.cs:229`）时金币**落地**（`MonsterObject.cs:1169-1190`） | 本作金币一直落地（`World::drop_monster_gold`），所以没有金币分配，与 Crystal 默认配置一致 |

---

## 8. 持久化

**不落盘。** Crystal 的 `CharacterInfo`（`Server/MirDatabase/CharacterInfo.cs`）里没有队伍块，
下线时 `StopGame` 直接 `LeaveGroup()`（`:406`）。本作 `Player::group_id` / `group_invitation` /
`group_invite_time` 都是运行时字段，`session` 的存档回写不包含它们；`mir-store` 无需改动。

唯一**被 Crystal 持久化**的相关字段是 `CharacterInfo.AllowGroup`（`CharacterInfo.cs:62`、`:249`、`:476`），
本作为了不改 `mir-store` 而没有存它，见 §9 第 9 条。

---

## 9. 与 Crystal 的差异与异常行为

### 本作的有意差异

1. **邀请有效期 4 秒**（`GROUP_INVITE_EXPIRE_MS = 4000`）。Crystal 的 `GroupInvitation` **没有超时**：
   只在 `PlayerObject.Process` 里发现「邀请者已离线」时清掉（`:480-481`），
   所以被邀请者可以挂很久再接受。本作按任务书加 TTL，并把「邀请者已离线」的邀请也当作失效
   （接受时报 `InviterOffline`；别人再邀请这个被邀请者时会顺手清掉它并继续），
   与 Crystal 每 tick 的 `Process` 清理效果一致。
2. **`AllowGroup` 不持久化，进世界默认 `true`**。Crystal 从角色存档读（`CharacterInfo.cs:249`）；
   本作不改 `mir-store`，且客户端还没有组队窗口（无法打开开关），所以默认允许组队，
   但仍支持 `C.SwitchGroup` 关闭。关闭后立刻退队（照抄 `:9224-9226`）。
3. **地图进入规则未实现**：`NoGroup` 地图的自动解散、`RequiredGroup` 地图的准入检查都没有
   （见 §7）。邀请/接受时的 `NoGroup` 检查已照抄。
4. **校验顺序微调**：接受邀请时先检查邀请者是否在线，再检查「是否仍是队长」等（Crystal 在
   `:9413` 才检查在线）；语义等价。
5. **没有英雄/宠物**：入队时只同步玩家血条（Crystal 还会同步双方英雄与宠物的血条，`:9454-9464`）。
6. **没有物品链接**：组队聊天不做 `ProcessChatItems`（`:2007`）。
7. **没有 GM/召唤/掷骰**：`GROUPCALL`、`ENABLEGROUPRECALL`、`@Roll` 都未实现。

### Crystal 的异常行为（照抄或刻意规避）

1. **人数/等级和只看距离，不看地图与生死**（`:811-820` 没有 `CurrentMap`/`Dead` 判断，
   真正分经验时才过滤，`:827-828`）。后果：隔壁图的队友、甚至已经死掉的队友都会「占一份额度」，
   把留在击杀者身上的经验稀释掉。本作**照抄**并有两条测试钉住
   （`dead_members_still_count_towards_the_party_size_crystal_quirk`、
   `off_map_members_count_for_the_party_size_but_receive_nothing`）。
2. **`nearCount == 0` 会数组越界**：`:830` 的 `partyExpRate[nearCount - 1]` 在 `nearCount == 0` 时下标为 -1，
   C# 会抛 `IndexOutOfRangeException`。实际上走不到（击杀者自己一定在队伍列表里、且与自己的距离为 0），
   但本作用 `index.max(0)` 兜底，异常输入下给击杀者自己发一份，而不是 panic。
3. **被踢/自己走会收到两条 `S.DeleteGroup`**：`DelMember` 先给目标发一条（`:9368`），
   紧接着的 `LeaveGroup` 又发一条（`:9255`）。本作照抄，测试断言正好 2 条。
4. **入队时「自己的地图名」重复三次**：`GroupMemberMapNameChanged` 的循环里
   `member.Enqueue(...)` 与 `Enqueue(...)` 在 `member == this` 时会各发一次，末尾再补一次（`:9491-9497`）。
   客户端只按名字覆盖字典（`GameScene.cs:5011-5019`），多发无害。
5. **非队长无法主动退队**：客户端退队只能发 `C.DelMember{自己名字}`（`GroupDialog.cs:207-224`），
   而服务端 `DelMember` 只允许队长（`:9347-9351`）→ 普通队员点「减号」只会收到「你不是队长」。
   真正能退队的途径只有关掉组队开关（`:9224-9226`）或下线（`:406`）。本作照抄这个限制，
   另外提供 `World::leave_group` 作为内部退队入口。
6. **组外发 `!!` 静默丢失**：`:2003` 的 `return` 既不提示也不广播。本作返回 0、不发包。
7. **队长不是显式字段**：只是 `GroupMembers[0]`。队长退队后「队长」自动变成剩下成员里的第一个
   （Crystal 的列表是共享引用，`Remove(this)` 之后 `[0]` 自然顺延）。本作 `Group::leader()` 同语义。
8. **邀请频率限制是「按人」而不是「按目标」**：`NextGroupInviteTime` 在 `AddMember` 一进门就写入
   （`:9276`），失败也照样消耗。本作照抄（`group_invite` 在任何校验之前扣冷却）。
9. **`CharacterInfo.AllowGroup` 默认 `false`**（新角色没写过存档时），意味着**新角色默认不能被邀请**；
   本作改为默认 `true`（见上面第 2 条差异）。
10. **`GetPlayerLocation` 只在 `BigMap > 0` 的图上发**（`HumanObject.cs:2735`），
    而新建队伍时的那一条 `SendMemberLocation`（`:9433`）**不看 `BigMap`**——两处不一致，照抄。
11. **`S.GroupMembersMap` 的 `PlayerMap` 是地图标题**（`CurrentMap.Info.Title`），不是文件名；
    没有 `GameData` 或标题为空时本作用地图文件名兜底（Crystal 的 `Title` 永远有值）。
12. **经验分成里的浮点截断**：C# 的 `(uint)` 是向零截断，F#/Rust 的 `as u32` 同语义；
    但 `f32` 在 `expPoint` 很大（> 2^24）时会丢精度，Crystal 也一样。
13. **`ReduceExp` 用「击杀者等级 vs 怪物等级」只算一次**（`:801`），队友等级更高也不会各自再算一次，
    低等级队友因此可能拿到比单刷更多的经验（队伍加成 > 等级差惩罚时）。
14. **`WinExp` 在队伍里时击杀者自己也只是一份**（`:824-832` 循环包含自己），并不会「独占一份 + 分给队友」。
15. **组队经验没有配置项**：`partyExpRate` 是写死在方法里的局部数组（`:805`），
    `Settings` 里**没有** `GroupExpRate`（只有行会的 `Guild_ExpRate`，`Settings.cs:328`）。

---

## 10. 常量表

| 常量 | 值 | 出处 |
|---|---|---|
| `MAX_GROUP` | 15 | `Globals.MaxGroup`，`Shared/Globals.cs:21` |
| `GROUP_EXP_RANGE` | 16 | `Globals.DataRange`，`Shared/Globals.cs:37` |
| `GROUP_INVITE_DELAY_MS` | 2000 | `Settings.GroupInviteDelay`，`Server/Settings.cs:336`（读 `:495`、写 `:773`），用于 `PlayerObject.cs:9276` |
| `PARTY_EXP_RATE` | `[1.0, 1.3, 1.4, 1.5, 1.6, 1.7, 1.8, 1.9, 2.0, 2.1, 2.2]` | `PlayerObject.cs:805` |
| `GROUP_INVITE_EXPIRE_MS` | 4000 | **本作扩展**（Crystal 无邀请超时，见 §9 第 1 条） |
| `Settings.ExpRate` | 1.0（本作固定） | `Server/Settings.cs:112` |
| 血量 `Expire` | `max(5, (RevTime - now)/1000)`，本作取 5 | `MapObject.cs:845` |

---

## 11. 测试清单（`crates/mir-server/tests/group.rs`，23 个用例，全部确定性）

固定种子 + `advance_clock` 驱动邀请冷却与邀请有效期；地图是内存里的 60×60 空地 +
内存数据库里的地图标题（比奇省 / 边界村）。

| # | 用例 | 钉住的行为 |
|---|---|---|
| 1 | `invite_sends_group_invite_to_target_and_switch_group_to_inviter` | 邀请成功：目标收 `S.GroupInvite`、邀请者收 `S.SwitchGroup{true}`、名字大小写不敏感、尚未建队 |
| 2 | `invite_unknown_name_is_rejected_with_a_hint` | 目标不在线 → `NotFound` + 系统提示 |
| 3 | `invite_self_is_rejected` | 邀请自己 → `CannotInviteSelf` |
| 4 | `invite_cooldown_is_silent_and_does_not_reach_the_target` | 2 秒冷却：静默失败、目标收不到邀请；到点后可再邀请 |
| 5 | `invite_target_already_in_a_group_is_rejected` | 目标已在队伍 → `TargetAlreadyInGroup` |
| 6 | `invite_group_full_is_rejected` | 15 人满员 → `GroupFull` |
| 7 | `invite_expires_after_the_extension_timeout` | 4 秒后接受 → `InvitationExpired`，不建队 |
| 8 | `accept_creates_the_group_and_pushes_the_member_list` | 接受后的成员列表、队长、`AddMember`/`GroupMembersMap`/`ObjectHealth`/`SendMemberLocation` 包序 |
| 9 | `decline_notifies_the_inviter_without_creating_a_group` | 拒绝：邀请者收提示、不建队、邀请清空 |
| 10 | `accepting_without_an_invitation_is_rejected` | 没有邀请 → `NoInvitation` |
| 11 | `leaving_notifies_the_remaining_members_with_delete_member` | 队长把自己移出：剩余成员收 `DeleteMember`、队长顺延、离队者收两条 `DeleteGroup` |
| 12 | `last_member_leaving_dissolves_the_group` | 最后一人退队 → 队伍解散、留下的人收 `DeleteGroup` |
| 13 | `only_the_leader_can_remove_members` | 非队长踢人 → `NotLeader`；队长踢人成功 |
| 14 | `switching_the_group_flag_off_leaves_the_group` | 关组队开关 → `S.SwitchGroup{false}` + 退队 |
| 15 | `group_chat_reaches_members_only` | `!!` 只发给队员（同图第三者收不到），文本带名字前缀 |
| 16 | `group_chat_outside_a_group_is_dropped_silently` | 组外 `!!` 静默丢弃 |
| 17 | `nearby_group_members_share_experience_with_the_party_bonus` | 同图近距离按等级比例 + 人数加成（65/65） |
| 18 | `distant_group_members_do_not_share_experience` | 16 格外不分（击杀者拿满 100） |
| 19 | `dead_members_still_count_towards_the_party_size_crystal_quirk` | 死人占额度（击杀者只拿 65） |
| 20 | `off_map_members_count_for_the_party_size_but_receive_nothing` | 异图同坐标也占额度（65） |
| 21 | `member_map_change_notifies_the_group_and_keeps_it` | 切图发 `S.GroupMembersMap`、队伍保留 |
| 22 | `logging_out_removes_the_member_from_the_group` | 下线自动离队，`ObjectRemove` 先于组队清理 |
| 23 | `inviter_logging_out_makes_the_invitation_stale` | 邀请者下线后接受 → `InviterOffline`，失效邀请清空 |

另外 `crates/mir-server/tests/login_flow.rs` 的进世界用例新增了对登录 `S.SwitchGroup{true}` 的断言。

---

## 12. 会话层接线（`session.rs`）

| 客户端包 | 处理函数 | World API |
|---|---|---|
| `C.SwitchGroup{AllowGroup}` | `on_switch_group` | `World::switch_group` |
| `C.AddMember{Name}` | `on_add_member` | `World::group_invite` |
| `C.DelMember{Name}` | `on_del_member` | `World::group_kick` |
| `C.GroupInvite{AcceptInvite}` | `on_group_invite` | `World::group_join` |
| `C.Chat{Message="!!…"}` | `on_chat` | `World::chat` → `World::group_chat` |
| `C.ShareQuest` | `on_share_quest` | `World::share_quest`（现在按队伍分享） |

所有处理函数都对「不在世界里」静默返回，与其它动作包一致。
