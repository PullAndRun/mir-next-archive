# 行会（Guild）

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：
> * `Server/MirDatabase/GuildInfo.cs`（行会实体 + `.mgd` 存档，`:7-257`）
> * `Server/MirObjects/GuildObject.cs`（运行期：发包、上线/下线、职位、经验、聊天，`:7-1066`）
> * `Server/MirObjects/PlayerObject.cs` 的 `#region Guilds`（`:9706-10500`，本作只取核心部分）
> * `Server/MirObjects/NPC/NPCScript.cs`（`[@CREATEGUILD]` 页面 → `S.GuildNameRequest`，`:60`、`:1050-1062`）
> * `Server/MirEnvir/Envir.cs`（`GuildList`/`Guilds`、`GetGuild`、`DeleteGuild`，`:126-147`、`:3089-3117`、`:4832-4851`、`:5478-5485`）
> * `Server/Settings.cs`（`Guild_RequiredLevel`/`Guild_ExpRate`/`Guild_PointPerLevel`/`Guild_CreationCostList`/`Guild_ExperienceList`/`Guild_MembercapList`，`:324-335`、`:1331-1417`）
> * `Shared/Data/GuildData.cs`（`GuildRank`/`GuildMember` 的线格式）、`Shared/Enums.cs:1897-1908`（`GuildRankOptions` 位掩码）
>
> 本作实现：
> * `crates/mir-server/src/guild.rs`（状态 + 全部流程 + 配置）
> * `crates/mir-server/src/world.rs`（`Player::guild`、`World::guilds`/`guild_invites`/`guild_writes`、`chat`/`gain_experience`/`enter`/`leave` 接入点）
> * `crates/mir-server/src/{session.rs,npc.rs}`（客户端包与 NPC 页面）
> * `crates/mir-store/migrations/0005_guilds.sql` + `crates/mir-store/src/{models.rs,repo.rs}`（持久化）
> * `crates/mir-server/src/{store.rs,pg_store.rs}`（`AccountStore` 的行会四件套）
> * 测试：`crates/mir-server/tests/guild.rs`（27 个用例，含真实脚本冒烟）、
>   `crates/mir-store/tests/guild_roundtrip.rs` 与 `crates/mir-server/tests/postgres_guild.rs`（需要 `MIR2_TEST_DATABASE_URL`，默认跳过）
>
> **其它 crate 未改动**：`mir-protocol` 里行会相关的包与数据类型都已齐全，本次没有新增/修改任何包。

---

## 1. 总览

| 主题 | Crystal | 本作 |
|---|---|---|
| 行会本体 | `GuildInfo`（存档）+ `GuildObject`（运行期包装） | `guild::Guild`（合并成一个结构），`World::guilds: HashMap<i32, Guild>` |
| 行会 ID | `Envir.NextGuildID++`（`Envir.cs:126`、`PlayerObject.cs:9821`） | `World::next_guild_id`（同样自己分配，落库时按同一个号写入） |
| 成员身份 | `GuildRank.Members`（成员嵌在职位里，`Shared/Data/GuildData.cs:1-43`） | `Guild::members: HashMap<角色 ID, GuildMemberState>` + `Guild::ranks`（只管职位） |
| 玩家的行会 | `PlayerObject.MyGuild` + `MyGuildRank` | `Player::guild: Option<GuildRef{ guild_id, rank_index }>` |
| 待接受邀请 | `PlayerObject.PendingGuildInvite`（`PlayerObject.cs:143`） | `World::guild_invites: HashMap<对象 ID, 行会 ID>` |
| 角色侧的指针 | `CharacterInfo.GuildIndex` | `characters.guild_index`（`-1` = 无行会） |
| 持久化 | `Guilds/<index>.mgd` 二进制（`GuildInfo.Save`，`:189-255`） | PostgreSQL 三张表（见 §7） |
| 上线/下线 | `GuildObject.PlayerLogged`（`:162-192`） | `World::guild_bind_player`（静默）/ `guild_player_online`（发包）/ `guild_player_offline` |
| 经验 | `GuildObject.GainExp`（`:644-704`），由 `PlayerObject.GainExp` 调用（`:902-903`） | `World::guild_gain_exp`，由 `World::gain_experience` 调用 |

**成员表是唯一事实来源**：Crystal 里「谁在哪个职位」只存在于 `Ranks[i].Members`；
本作把成员平铺进 `Guild::members`（带 `rank_index`），发包时再用
`Guild::rank_snapshot` 拼成 `S.GuildMemberChange` 需要的 `GuildRank{ members }`。
这样「一个角色只能在一个行会」可以直接用数据库主键表达，登录时也只查一行。

---

## 2. 协议与包序

行会包全部已存在于 `mir-protocol`（本次**没有**改动该 crate）：

| 语义 | 客户端包 | 服务端包 | Crystal |
|---|---|---|---|
| 申请创建（NPC 按钮） | — | `S.GuildNameRequest`（171） | `NPCScript.cs:1050-1062` |
| 提交行会名 | `C.GuildNameReturn{Name}`（82） | `S.GuildMemberChange{Status=2}` + `S.GuildStatus`（168） | `PlayerObject.cs:10029-10060`、`:9829-9835` |
| 邀请 | `C.EditGuildMember{ChangeType=0, Name}`（79） | `S.GuildInvite{Name=行会名}`（169） | `PlayerObject.cs:9849-9888` |
| 接受/拒绝 | `C.GuildInvite{AcceptInvite}`（81） | `S.GuildMemberChange{Status=2}` + `S.GuildStatus` | `:9978-10011` |
| 踢人 / 退会 | `C.EditGuildMember{ChangeType=1, Name}` | 其余成员 `S.GuildMemberChange{Status=3/4}`；被移除者 `S.GuildStatus{空}` + `S.Chat{ChatType.Guild}` | `GuildObject.cs:433-564` |
| 升/降职 | `C.EditGuildMember{ChangeType=2, RankIndex, Name}` | 本人 `{Status=8, Ranks=[新职位]}`、其他人 `{Status=5, RankIndex}` | `GuildObject.cs:238-332` |
| 改职位名 | `C.EditGuildMember{ChangeType=3, RankIndex, RankName}` | `{Status=7, Ranks=[职位]}`（全体在线成员） | `GuildObject.cs:380-431` |
| 新职位 | `C.EditGuildMember{ChangeType=4}` | `{Status=6, Ranks=[新职位]}` | `GuildObject.cs:334-352` |
| 职位权限 | `C.EditGuildMember{ChangeType=5, RankIndex, Name="true"/"false", RankName="位序号"}` | `{Status=7, Ranks=[职位]}` | `GuildObject.cs:354-379` |
| 改公告 | `C.EditGuildNotice{Notice}`（80） | `S.GuildNoticeChange{update=-1}`（166） | `PlayerObject.cs:9958-9977`、`GuildObject.cs:582-600` |
| 拉公告/成员 | `C.RequestGuildInfo{Type}`（83） | `S.GuildNoticeChange{notice}` / `S.GuildMemberChange{Status=255, Ranks=全部}` | `PlayerObject.cs:10012-10028` |
| 行会聊天 | `C.Chat{"!~…"}` | `S.Chat{ChatType.Guild}`（只发在线成员） | `PlayerObject.cs:2014-2024` |
| 行会经验 | — | `S.GuildExpGain{Amount}`（170）/ 升级时 `S.GuildStatus` | `GuildObject.cs:644-704` |
| 上线/下线 | — | `S.GuildMemberChange{Status=1/0}` | `GuildObject.cs:162-192` |

包序要点（实现里逐条照抄，测试有断言）：

* **创建**：`S.LoseGold` → `S.GuildMemberChange{Status=2}` → `S.GuildStatus`
  （`PlayerObject.cs:9780`、`:9829-9835`）。
* **登录**：行会包排在 `S.UserInformation` 之后。Crystal 在 `StartGame` 里先校验
  `Info.GuildIndex`（`:1153-1170`），到 `:1309-1316` 才 `PlayerLogged` 发
  `S.GuildMemberChange{Status=1}` + `S.GuildStatus`；本作由会话在 `on_start_game`
  的末尾显式调 `World::guild_player_online`。
* **踢人**：`Status=3`（被踢）/`Status=4`（自己退）只发给**其余**在线成员；
  被移除者收到空 `S.GuildStatus` + 一行 `ChatType.Guild` 文本（`GuildObject.cs:541-563`）。
* **升级**：`S.GuildStatus` 发给所有在线成员（`GuildObject.cs:685-694`）；
  没升级时 `S.GuildExpGain` 每 10 秒最多一次（`:696-703`）。

---

## 3. 创建行会

两条入口，共用同一套校验（`World::create_guild`）：

1. **NPC 页面**（Crystal 的正式流程）：脚本里放一个 `[@CREATEGUILD]` 页面
   （可以是空页）和一个指向它的按钮，例如：

   ```text
   [@Main]
   #SAY
   要创建行会吗？
   <创建行会/@CREATEGUILD>

   [@CREATEGUILD]
   ```

   点按钮时 `NPCScript.ProcessSpecial` 的 `case GuildCreateKey` 会：
   等级不够 → 只提示（`ServerTextKeys.YouNeedLevelToCreateGuild`，`NPCScript.cs:1051-1054`）；
   已在行会 → 提示（`:1060-1061`）；否则 `CanCreateGuild = true` + `S.GuildNameRequest`
   （`:1057-1058`）。客户端随后发 `C.GuildNameReturn{Name}`。

2. **脚本动作**（**本作扩展**，Crystal 没有）：`#ACT` 里直接写
   `CREATEGUILD <行会名>`。任务书要求的写法，`mir-db` 的
   `NpcActionKind::CreateGuild` 识别关键字，`npc.rs` 调同一个 `World::create_guild`。

校验顺序（`World::create_guild`，照 `PlayerObject.CreateGuild`，`:9708-9838`）：

| # | 检查 | Crystal |
|---|---|---|
| 1 | 已在行会 | `:9710` |
| 2 | 重名（去空格 + 忽略大小写，`Envir.GetGuild`） | `:9711`、`Envir.cs:4836` |
| 3 | 名字长度 3–20、不含 `\`（本作连控制字符一起拒） | `:10033-10043` |
| 4 | 等级 ≥ `Guild_RequiredLevel`（默认 22） | `:9713-9717` |
| 5 | 金币 ≥ `Guild_CreationCostList` 里的金币项（默认 1,000,000） | `:9728-9737`、`Settings.cs:1335` |
| 6 | 扣钱（`S.LoseGold`） | `:9773-9783` |
| 7 | 建行会 + `Leader` 职位（`Options=255`）+ 会长入会 | `:9820-9831`、`GuildInfo.cs:54-58` |
| 8 | 广播 `Status=2` + `S.GuildStatus` | `:9833-9835` |

---

## 4. 成员管理

* **邀请**：`ChangeType=0` → 需要 `CanRecruit`（2）→ 目标在线、未入会、没有待处理邀请
  → 只给目标发 `S.GuildInvite{行会名}`；邀请者没有任何回包（`PlayerObject.cs:9849-9888`）。
* **接受**：`C.GuildInvite{true}` → 人数检查（`GuildObject.HasRoom`）→ 需要时补一个
  `Members` 职位（`GuildObject.cs:224-227`）→ 落在**最低职位** → `Status=2` + `S.GuildStatus`。
* **踢人/退会**：`ChangeType=1`，`Name` 是自己就是退会。权限 `CanKick`（4）；
  只能动**严格更低**的职位（`Kicker.Rank >= Target.Rank && Kicker.Rank != 0 && 不是自己`
  → 拒绝，`GuildObject.cs:457-461`；注意是 `>=`，同级也不行）。
* **升/降职**：`ChangeType=2`，权限 `CanChangeRank`（1）。
  * 升成会长（`rank_index = 0`）要求目标等级 ≥ `Guild_RequiredLevel`（`:258-262`）；
  * 不能动比自己高的职位（会长例外，`:264`）；
  * 降职会长时：会长少于 3 个 → `GuildNeedsTwoLeaders`；没有别的会长在线 →
    `NeedOneLeaderOnline`（`:265-279`）。
  * Crystal 没有「转让会长」包，**升成 rank 0 就是转让**；会长可以多名（`Ranks[0].Members`）。
* **职位名**：`ChangeType=3`，长度 3–20、不含 `\`；只能改**自己这一级或更低**的职位名
  （`SelfRankIndex > RankIndex` 才拒绝，`:395`）。
* **新职位**：`ChangeType=4`，最多 255 个；插在最低职位之前，名字 `Rank-<序号>`
  （`ServerTextKeys.RankNum` = `"Rank-{0}"`，`Shared/Language.cs:2483`）。
  插入后本作会把所有职位重排序号并同步成员的 `rank_index`（Crystal 靠对象引用跟着走）。
* **职位权限**：`ChangeType=5`，`Name` 是 `"true"`/`"false"`、`RankName` 是位序号 `0..=7`
  （`1 << option`）。不能改自己这一级（`GuildObject.cs:361`）。
* **公告**：需要 `CanChangeNotice`（64），最多 200 行（`PlayerObject.cs:9965-9975`）；
  改完给在线成员发 `S.GuildNoticeChange{update=-1}`，客户端再自己拉正文。

权限位（`Shared/Enums.cs:1897-1908`，`GuildRankOptions`，u8 位掩码）：

| 位 | 值 | 含义 |
|---|---|---|
| `CanChangeRank` | 1 | 升降职 / 改职位名 / 新职位 / 改权限 |
| `CanRecruit` | 2 | 邀请 |
| `CanKick` | 4 | 踢人 |
| `CanStoreItem` | 8 | 存仓库（**未做**） |
| `CanRetrieveItem` | 16 | 取仓库（**未做**） |
| `CanAlterAlliance` | 32 | 联盟/敌对（**未做**） |
| `CanChangeNotice` | 64 | 改公告 |
| `CanActivateBuff` | 128 | 开行会 buff（**未做**） |

---

## 5. 行会聊天（`!~`）

`World::chat` 在 `!!`（组队）之后接 `!~` 分支（Crystal 的 `if/else if` 链，
`PlayerObject.cs:2014-2024`）：

* 不在行会 → 静默丢弃（返回 0）；
* 否则给自己与所有**在线成员**发 `S.Chat{ "角色名: 正文", ChatType.Guild }`
  （`GuildObject.SendMessage` → `ReceiveChat`，`GuildObject.cs:113-126`）；
* **不走本地图广播**：站在旁边的非成员收不到（测试 `guild_chat_only_reaches_online_members`）。

---

## 6. 行会经验与等级

`World::gain_experience`（角色经验入口）在「加完经验、判定角色升级之前」调用
`World::guild_gain_exp`（Crystal `PlayerObject.cs:902-903`）。

| 规则 | 值 | 出处 |
|---|---|---|
| 行会经验 = 角色经验 × `Guild_ExpRate`（截断） | 默认 `0.01` | `Settings.cs:328`、`GuildObject.cs:649` |
| `MaxExperience <= 0` 不涨 | — | `:647` |
| 升级条件 | `experience > MaxExperience`（**严格大于**） | `:656` |
| 每级升级点 | `Guild_PointPerLevel`（代码默认 0，`GuildSettings.ini` 样本 1） | `:660`、`Settings.cs:327` |
| 经验表 | `Guild_ExperienceList[level]`，缺项 0 = 封顶 | `GuildInfo.cs:63-66`、`:159-162` |
| 人数表 | `Guild_MembercapList[level]`，0 = 不限 | `GuildObject.cs:632-643` |
| 广播 | 升级：`S.GuildStatus` × 在线成员；未升级：`S.GuildExpGain` 每 10 秒最多一次 | `:676-703` |
| 新手行会 | `NewbieGuild` 不加经验 | `:902` |

`GuildSettings`（`crates/mir-server/src/guild.rs`）按 Crystal 的
`Configs/GuildSettings.ini` 解析：

```ini
[Guilds]
MinimumLevel=22
ExpRate=0.01
PointPerLevel=1
[Exp] Level-0=45000 …
[Cap] Level-0=5 …
[Required-0] ItemName= / Amount=1000000
```

候选路径：`config/GuildSettings.ini` → 素材里社区数据库的
`Configs/GuildSettings.ini`（`SUPRCODE_JEV`/`CJLAAA` 等）。找不到时用代码默认值
（等级 22、金币 1,000,000、倍率 0.01、升级点 0、经验/人数表全 0）并打一条 `warn!`——
与 Crystal「没配 `GuildSettings.ini` 时行会不升级、不限人数」的效果一致。

---

## 7. 持久化

### 7.1 表结构（`crates/mir-store/migrations/0005_guilds.sql`）

| 表 | 列 | Crystal |
|---|---|---|
| `guilds` | `id`（IDENTITY，接受显式值）、`name`（`lower(name)` 唯一）、`leader_character_id`、`level`、`experience`、`gold`、`spare_points`、`notice`（`\n` 连接）、`created_at`、`updated_at` | `GuildInfo.cs:9-14`、`:24` |
| `guild_ranks` | `(guild_id, rank_index)` 主键、`name`、`options` | `GuildRank`（`Shared/Data/GuildData.cs:1`） |
| `guild_members` | `character_id` **主键**（一个角色只能在一个行会）、`guild_id`、`rank_index`、`joined_at` | `GuildRank.Members`（`:62`） |

* `guilds.id` 用 `GENERATED BY DEFAULT AS IDENTITY`：世界层自己分配 ID
  （`Envir.NextGuildID`），落库必须按同一个号写；插入后会 `setval` 把序列推到 `MAX(id)`。
* `guild_ranks.rank_index` 避开 SQL 关键字 `index`（Crystal 的 `GuildRank.Index`）。
* `guild_members.character_id` 是主键 → 「一个角色只能在一个行会」，对应 Crystal 的
  `CharacterInfo.GuildIndex` 单值。
* `notice` 把 `List<string>` 用 `\n` 拼成一个 `TEXT`（空行保留）。
* 角色侧另有 `characters.guild_index`（`-1` = 无行会），迁移 0001 里已有。

### 7.2 仓储函数（`crates/mir-store/src/repo.rs`）

| 函数 | 行为 |
|---|---|
| `create_guild(&mut PgConnection, &Guild)` | 写 `guilds` + 全部职位 + 全部成员；`id > 0` 时按显式 ID 写；重名 → `StoreError::GuildExists` |
| `load_guild(&mut PgConnection, id)` | 行会 + 职位 + 成员；不存在 → `NotFound("行会")` |
| `load_guild_by_name(&mut PgConnection, name)` | 去空格 + 忽略大小写（`Envir.GetGuild` 语义） |
| `save_guild(&mut PgConnection, &Guild)` | `UPDATE guilds` + 职位/成员**全量替换**；0 行 → `NotFound` |
| `delete_guild(&mut PgConnection, id)` | 删 `guilds` 一行（子表 `ON DELETE CASCADE`） |
| `list_guilds(&mut PgConnection)` | 三条查询拼装（启动时灌进世界） |
| `set_character_guild(&mut PgConnection, character_id, guild_id)` | 只写 `characters.guild_index` |

这些函数收 `&mut PgConnection` 而不是 `PgExecutor`：它们都是「一条主表 + 两张子表」
的多语句操作，调用方要把整组语句放进同一个事务（`&mut *tx` 可以直接传进去）。

### 7.3 服务端侧的写入队列

世界层是同步的、存储层是异步的，所以行会操作只把「要写什么」排进
`World::guild_writes`（`GuildWrite::Save` / `Delete` / `SetCharacterGuild`），由：

* 会话在每次处理完行会包、以及 `leave_world` 时调 `guild::persist_guild_writes`；
* 服务端 100ms 的 tick 循环定期取走（行会经验来自怪物击杀，那些写入没有会话触发）。

`AccountStore` 增加了四个方法（`save_guild` / `delete_guild` / `list_guilds` /
`set_character_guild`），`MemoryStore` 与 `PgAccountStore` 都实现了：
前者在内存里存 `Vec<GuildRecord>`（测试用），后者按 `mir_store` 的行会函数落库。

> **已知限制**：如果两个写入者恰好同时取队列，写入顺序可能与事件顺序不同
> （例如 Save 与随后的 Delete 被拆到两次落盘）。队列本身在锁内 `take`，不会丢事件；
> 要彻底有序需要把「写库」也放进世界锁，代价是 I/O 阻塞 tick。留作后续优化。

---

## 8. 测试清单（`crates/mir-server/tests/guild.rs`，27 个）

| # | 用例 | 覆盖 |
|---|---|---|
| 1 | `create_guild_deducts_gold_and_marks_writes` | 扣金币、`S.LoseGold`、行会实体、`Status=2` + `S.GuildStatus`、两条落盘写入 |
| 2 | `create_guild_rejects_short_long_and_illegal_names` | 名字长度 / `\` / 换行 |
| 3 | `create_guild_rejects_duplicate_name_ignoring_case_and_spaces` | 重名（忽略大小写与空格） |
| 4 | `create_guild_rejects_insufficient_gold` | 金币不足 |
| 5 | `create_guild_rejects_low_level` | 等级不足 |
| 6 | `create_guild_twice_is_rejected` | 已有行会 |
| 7 | `invite_then_accept_adds_member_and_sends_status` | 邀请 → 接受 → 成员列表（`Status=255`） |
| 8 | `accepting_twice_or_joining_twice_is_rejected` | 重复加入 / 无邀请 |
| 9 | `guild_invite_requires_recruit_permission` | `CanRecruit` 权限 + 系统提示 |
| 10 | `promote_and_demote_member` | 新职位、升职 `Status=8/5`、降职 |
| 11 | `promote_to_leader_requires_level_and_transfers_leadership` | 会长最低等级、会长转让（多名会长） |
| 12 | `kick_member_clears_guild_and_sends_empty_status` | 踢人：其他人 `Status=3`、被踢者空 `GuildStatus` + 行会频道提示、落盘 `-1` |
| 13 | `kick_requires_permission_and_rank` | 无 `CanKick`、同级/越级拒绝、高职位可踢低职位 |
| 14 | `edit_notice_broadcasts_and_requires_permission` | 公告广播、权限、200 行上限、拉取正文 |
| 15 | `guild_chat_only_reaches_online_members` | `!~` 只发行会成员、非成员静默 |
| 16 | `guild_gains_exp_and_levels_up` | 行会经验、10 秒节流、升级、升级点、人数上限、封顶 |
| 17 | `member_leave_removes_from_guild` | 成员退会 |
| 18 | `leader_can_transfer_then_leave` | 会长转让后退会 |
| 19 | `last_leader_cannot_leave_while_others_remain` | 最后一个会长不能退（还有别人） |
| 20 | `last_member_leaving_disbands_and_deletes_row` | 最后一人退会 → 解散 + 删库 + `guild_index = -1`（`MemoryStore`） |
| 21 | `character_guild_index_round_trips_through_store` | `Character.guild_index` 存档往返 + `set_character_guild` |
| 22 | `relogin_rebinds_guild_membership` | 下线 `Status=0`、重登按角色 ID 找回行会 |
| 23 | `guild_settings_parse_the_real_ini_shape` | `GuildSettings.ini` 解析 |
| 24 | `npc_createguild_page_requests_a_name` | `[@CREATEGUILD]` → `S.GuildNameRequest` + `CanCreateGuild` |
| 25 | `npc_createguild_page_rejects_low_level` | NPC 页面按等级拒绝 |
| 26 | `npc_createguild_action_creates_the_guild` | 脚本动作 `CREATEGUILD <名字>` |
| 27 | `real_data_guild_scripts_and_creation_flow` | 真实数据冒烟（见 §9） |

数据库侧另有：

* `crates/mir-store/tests/guild_roundtrip.rs`：建/读/回写/删 + 显式 ID + 重名 +
  `guild_index`，整个事务回滚，需要 `MIR2_TEST_DATABASE_URL`；
* `crates/mir-server/tests/postgres_guild.rs`：`PgAccountStore` 的行会四件套 +
  `guild_index` 整行往返，同样需要 `MIR2_TEST_DATABASE_URL`。

**方案说明（任务书要求的「存档往返」）**：默认测试集用一个**临时 Postgres 库**
（`mir2_guild_test`，由一次性脚本 `CREATE DATABASE` 出来后跑上面两个用例、跑完删除）
验证了真实 SQL；仓库里提交的两个用例在没有 `MIR2_TEST_DATABASE_URL` 时跳过，
而 `MemoryStore` 扩展了行会四件套，让世界层的 27 个用例在**没有数据库**的环境下
也能断言「写库」的结果（`GuildWrite` 队列 → `MemoryStore` → 读回断言）。

---

## 9. 真实数据冒烟

素材里（`resource/community/*/Envir/NPCs`）：

| 数据集 | 用到 `[@CREATEGUILD]` 的脚本 | 直接写 `CREATEGUILD` 动作的脚本 |
|---|---|---|
| `suprcode-crystal-database/cjlaaa` | 1 份（`GM/GM-Manager.txt`，1 处页面） | 0 |
| `suprcode-crystal-database/Jev` | 3 份（`GM/GM-Manager.txt` 等三个变体） | 0 |

（数字由测试 `real_data_guild_scripts_and_creation_flow` 现场扫描 `GameData::discover()`
指向的 `Envir/NPCs` 打印出来；测试断言「至少 1 份」且「真实脚本里没有本作扩展的
`CREATEGUILD` 动作」。）

**创建流程冒烟**：测试加载**真实的** `GM/GM-Manager.txt`，用一个合成的 `NpcInfo`
（`file_name = "GM/GM-Manager"`）把它刷到测试地图上，然后：

1. 把玩家推进「已在对话中且 `[@CREATEGUILD]` 按钮可见」的状态
   （真实入口页要求 `#IF ISADMIN`，普通玩家点不到那个按钮，测试直接填
   `NpcState.npc_object_id` + `buttons`，这是唯一被绕过的部分）；
2. `World::call_npc(player, npc, "[@CREATEGUILD]")` → 真实脚本的页面被处理 →
   `S.GuildNameRequest` + `CanCreateGuild = true`；
3. 等价于客户端回 `C.GuildNameReturn` 的 `World::create_guild` 成功建会。

---

## 10. 与 Crystal 的差异

1. **`CREATEGUILD` 脚本动作是本作扩展**：Crystal 只有 `[@CREATEGUILD]` 页面 +
   `C.GuildNameReturn`（`NPCScript.cs:1050-1062`、`PlayerObject.cs:10029-10060`）。
   `PlayerObject.cs:3264` 的 `case "CREATEGUILD":` 是 **GM 聊天命令**，不是脚本动作。
2. **创建费用只扣金币**：Crystal 的 `Guild_CreationCostList` 还能要求物品
   （默认 1 个 `WoomaHorn`，`Settings.cs:1336`）；本作只实现金币项（`Required-0`）。
3. **`NewbieGuild` 的特权没做**：Crystal 只允许 GM 创建它、并把它等级拉满 21、
   人数上限用 `NewbieGuildMaxSize`（`PlayerObject.cs:9719-9723`、`GuildInfo.cs:68-72`）。
   本作的 `Player` 没有管理员标志，也没有这个特殊行会。
4. **`EnableGuildInvite` 开关没做**：Crystal 可以关掉「别人邀请我」（`PlayerObject.cs:2467`），
   本作没有对应命令，邀请总被接受与否由被邀请者决定。
5. **公告/成员表不做脏标记**：Crystal 用 `GuildNoticeChanged`/`GuildMembersChanged`
   去重（`:10019`、`:10024`）；本作 `C.RequestGuildInfo` 每次都回（幂等）。
6. **成员顺序**：Crystal 是「职位内追加顺序」；本作按 `(joined_at, character_id)` 稳定排序。
7. **离线成员等级**：升成会长要查目标等级（`GuildObject.cs:258`），Crystal 读角色库；
   本作缓存登录时的等级（`GuildMemberState.level`），离线成员读到的是 1（未知）。
   离线成员被直接升成会长时会因此被拒绝——需要的话可以后续把等级写进 `guild_members`。
8. **`GuildRank` 的成员不落库**：Crystal 的 `.mgd` 里成员嵌在职位里；本作拆表。
9. **`C.RequestGuildInfo{Type}` 只有两种类型**：`0` 公告、`1` 成员表
   （`PlayerObject.cs:10016-10027`）。任务书写的「返回 `S.GuildStatus`/`S.GuildStorage*`」
   与源码不符：`S.GuildStatus` 由创建/接受/升级/登录主动推；仓库列表走的是
   `GuildCanRequestItems`（`:10268-10269`，每 10 分钟一次）而不是这个包。
   本作按源码实现，仓库未做。
10. **`C.EditGuildMember` 的 `ChangeType=0` 才是「邀请」**，`C.GuildInvite` 是「接受/拒绝」。
    任务书把两者混在一起描述，实现按源码。

---

## 11. Crystal 异常行为（照抄时踩到的坑）

1. **行会经验回写缺失**（`GuildObject.cs:644-704`）：
   ```csharp
   Info.Experience += expAmount;      // :652
   var experience = Info.Experience;  // :654
   while (experience > Info.MaxExperience) { … experience -= Info.MaxExperience; … }  // :656-674
   ```
   循环里的局部变量 `experience` 从不写回 `Info.Experience`，于是**每次加经验都会重新
   消耗同一份累计经验再升一级**。本作把余数写回（`World::guild_gain_exp`），
   并在测试 `guild_gains_exp_and_levels_up` 里断言 `level`/`experience` 的准确值。
2. **关闭权限用的是异或**（`GuildObject.cs:370`）：
   `Options ^= (GuildRankOptions)(1 << Option)` —— 当这一位本来就是 0 时，点「关闭」
   反而会**打开**权限。本作用「清位」语义（`&= !bit`）。
3. **同名行会比较忽略空格**（`Envir.cs:4836`）：`"Test Guild"` 与 `"TestGuild"` 算重名，
   但数据库里存的是原文——Crystal 的 `GetGuild` 与 `GuildInfo.Name` 不一致时会出现
   「看起来重名、文件里却是两个」。本作沿用同一比较语义，同时用
   `lower(name)` 唯一索引兜住精确重名。
4. **踢人只发 `Status=3` 给其他成员**（`GuildObject.cs:541-552`）：被踢的人自己
   收不到 `S.GuildMemberChange`，客户端只能靠空 `S.GuildStatus` 清界面。
5. **会长退出条件**（`GuildObject.cs:463-476`）：`Ranks[0].Members.Count < 2 &&
   Membercount < 2` 才允许解散；否则「只有一个会长、行会里还有别人」时
   **谁都退不了**（会提示 `YouNeedLastLeaderToDisbandGuild`）。本作照抄。
6. **同职位名**：Crystal 不阻止两个职位同名（`ChangeRankName` 只查长度）。

---

## 12. 未做（明确划出本次范围）

* **攻城 / 领地**：`C.GuildRequestWar`（`GuildWarReturn`）、`C.PurchaseGuildTerritory`、
  `C.GuildTerritoryPage`、`C.GuildBuffUpdate` 对应的 `GuildObject.GoToWar`/`GTForSale`/
  `EndGT`（`:707-900`）全部未实现，收到包只记 `debug!` 日志。
  `GuildInfo` 里的 `GTRent`/`GTIndex`/`GTKey`/`GTPrice`/`GTBegin` 也不落库。
* **行会仓库**：`C.GuildStorageGoldChange` / `C.GuildStorageItemChange`、
  `GuildInfo.StoredItems`（112 格）、`GuildCanRequestItems`（`PlayerObject.cs:10061-10350`）
  未实现；`S.GuildStatus.ItemCount` 固定填 112（结构体要求），但仓库永远是空的。
* **行会 buff**：`C.GuildBuffUpdate`、`GuildInfo.BuffList`、`GuildBuffInfo`
  （`GuildObject.cs:912-1028`）未实现；`S.GuildStatus.BuffCount` 固定 0。
* **行会联盟/敌对**：`AllyGuilds`/`WarringGuilds`（`GuildObject.cs:59-64`）未实现。
* **投票**：`GuildInfo.Votes`/`Voting`/`LastVoteAttempt`（`:16-19`）未实现，
  `S.GuildStatus.Voting` 固定 `false`。
* **战旗**：`FlagImage`/`FlagColour`（`GuildInfo.cs:29-30`）未实现。
* **`NewbieGuild`**、**`EnableGuildInvite` 开关**、**行会联盟广播**、**`GuildCanRequestItems`
  的「每 10 分钟一次」节流**：见 §10。
* **地图进入规则里的行会限制**（`RequiredGuild`/`NoGuild`）属于地图进入规则系统，未实现。
