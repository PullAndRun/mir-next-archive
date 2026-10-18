# 行会（Guild）

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：
> * `Server/MirDatabase/GuildInfo.cs`（行会实体 + `.mgd` 存档，`:7-257`）
> * `Server/MirObjects/GuildObject.cs`（运行期：发包、上线/下线、职位、经验、聊天、仓库持有、buff，`:7-1066`）
> * `Server/MirObjects/PlayerObject.cs` 的 `#region Guilds`（`:9706-10500`：仓库与 buff 的入口在 `:10061-10436`）
> * `Server/MirObjects/NPC/NPCScript.cs`（`[@CREATEGUILD]` 页面 → `S.GuildNameRequest`，`:60`、`:1050-1062`）
> * `Server/MirEnvir/Envir.cs`（`GuildList`/`Guilds`、`GetGuild`、`DeleteGuild`、`FindGuildBuffInfo`、每分钟的 `guild.Process()`，`:126-147`、`:2330-2337`、`:3089-3117`、`:4832-4851`、`:5026-5037`、`:5478-5485`）
> * `Server/Settings.cs`（`Guild_RequiredLevel`/`Guild_ExpRate`/`Guild_PointPerLevel`/`Guild_CreationCostList`/`Guild_ExperienceList`/`Guild_MembercapList`/`Guild_BuffList`，`:324-335`、`:1331-1376`）
> * `Shared/Data/GuildData.cs`（`GuildRank`/`GuildStorageItem`/`GuildBuffInfo`/`GuildBuff` 的线格式与 INI 读写）、`Shared/Enums.cs:1897-1908`（`GuildRankOptions` 位掩码）
>
> 本作实现：
> * `crates/mir-server/src/guild.rs`（状态 + 全部流程 + 配置，含 `[Buff-N]` 解析）
> * `crates/mir-server/src/guild_storage.rs`（仓库物品/金币运行期 + 安全区判定）
> * `crates/mir-server/src/guild_buffs.rs`（行会 buff 购买/激活/到期/属性叠加）
> * `crates/mir-server/src/world.rs`（`Player::guild`、`World::guilds`/`guild_invites`/`guild_writes`、
>   `guild_storage_requested`/`guild_buff_info_sent`、`chat`/`gain_experience`/`tick` 接入点）
> * `crates/mir-server/src/{session.rs,npc.rs}`（客户端包与 NPC 页面）
> * `crates/mir-store/migrations/{0005_guilds.sql,0006_guild_storage.sql}` + `crates/mir-store/src/{models.rs,repo.rs}`（持久化）
> * `crates/mir-server/src/{store.rs,pg_store.rs}`（`AccountStore` 的行会函数）
> * 测试：`crates/mir-server/tests/guild.rs`（27 个）、`crates/mir-server/tests/guild_storage.rs`（21 个，
>   含真实 `GuildSettings.ini` 冒烟）、`crates/mir-store/tests/guild_roundtrip.rs` 与
>   `crates/mir-server/tests/postgres_guild.rs`（需要 `MIR2_TEST_DATABASE_URL`，默认跳过）
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
| 仓库 | `GuildStorageItem[] StoredItems = new GuildStorageItem[112]`（`GuildInfo.cs:22`） | `Guild::storage: Vec<Option<GuildStorageSlot>>`（固定 112 格） |
| 仓库金币 | `GuildInfo.Gold`（**同一份**行会资金） | `Guild::gold`（`guilds.gold`；**没有**独立的 `storage_gold`，见 §7.2） |
| 仓库列表节流 | `PlayerObject.GuildCanRequestItems`（`:146`） | `World::guild_storage_requested`（按对象 ID 记「已经拉过」） |
| buff 定义 | `Settings.Guild_BuffList`（`Settings.cs:335`，来自 `GuildSettings.ini`） | `GuildSettings::buff_list`（`Vec<GuildBuffInfo>`） |
| buff 持有 | `GuildInfo.BuffList`（`List<GuildBuff>`，`GuildInfo.cs:23`） | `Guild::buffs: Vec<GuildBuff>` |
| buff 可选列表标志 | `PlayerObject.RequestedGuildBuffInfo`（`:147`） | `World::guild_buff_info_sent` |
| 持久化 | `Guilds/<index>.mgd` 二进制（`GuildInfo.Save`，`:189-255`） | PostgreSQL 五张表（见 §9） |
| 上线/下线 | `GuildObject.PlayerLogged`（`:162-192`） | `World::guild_bind_player`（静默）/ `guild_player_online`（发包）/ `guild_player_offline` |
| 经验 | `GuildObject.GainExp`（`:644-704`），由 `PlayerObject.GainExp` 调用（`:902-903`） | `World::guild_gain_exp`，由 `World::gain_experience` 调用 |
| buff 到期 | `GuildObject.Process`（`:796-842`），`Envir.Process` 每分钟调一次（`Envir.cs:2330-2337`） | `World::process_guild_buffs`（测试直接调）/ `tick_guild_buffs`（每分钟节流） |

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
| 接受/拒绝 | `C.GuildInvite{AcceptInvite}`（81） | `S.GuildMemberChange{Status=2}` + `S.GuildStatus` + 有 buff 时 `S.GuildBuffList{ActiveBuffs}`（248） | `:9978-10011` |
| 踢人 / 退会 | `C.EditGuildMember{ChangeType=1, Name}` | 其余成员 `S.GuildMemberChange{Status=3/4}`；被移除者 `S.Chat{ChatType.Guild}` → `S.GuildStatus{空}` | `GuildObject.cs:433-564` |
| 升/降职 | `C.EditGuildMember{ChangeType=2, RankIndex, Name}` | 本人 `{Status=8, Ranks=[新职位]}`、其他人 `{Status=5, RankIndex}` | `GuildObject.cs:238-332` |
| 改职位名 | `C.EditGuildMember{ChangeType=3, RankIndex, RankName}` | `{Status=7, Ranks=[职位]}`（全体在线成员） | `GuildObject.cs:380-431` |
| 新职位 | `C.EditGuildMember{ChangeType=4}` | `{Status=6, Ranks=[新职位]}` | `GuildObject.cs:334-352` |
| 职位权限 | `C.EditGuildMember{ChangeType=5, RankIndex, Name="true"/"false", RankName="位序号"}` | `{Status=7, Ranks=[职位]}` | `GuildObject.cs:354-379` |
| 改公告 | `C.EditGuildNotice{Notice}`（80） | `S.GuildNoticeChange{update=-1}`（166） | `PlayerObject.cs:9958-9977`、`GuildObject.cs:582-600` |
| 拉公告/成员 | `C.RequestGuildInfo{Type}`（83） | `S.GuildNoticeChange{notice}` / `S.GuildMemberChange{Status=255, Ranks=全部}` | `PlayerObject.cs:10012-10028` |
| 行会聊天 | `C.Chat{"!~…"}` | `S.Chat{ChatType.Guild}`（只发在线成员） | `PlayerObject.cs:2014-2024` |
| 行会经验 | — | `S.GuildExpGain{Amount}`（170）/ 升级时 `S.GuildStatus` | `GuildObject.cs:644-704` |
| 上线/下线 | — | `S.GuildMemberChange{Status=1/0}` | `GuildObject.cs:162-192` |
| **存物品** | `C.GuildStorageItemChange{Type=0, From=背包格, To=仓库格}`（85） | `S.GuildStorageItemChange{Type=0, User=角色 ID, Item, To, From}`（173，**广播全行会**） | `PlayerObject.cs:10140-10184` |
| **取物品** | `{Type=1, From=仓库格, To=背包格}` | `{Type=1, User, To, From}`（**没有 Item**，广播） | `:10185-10223` |
| **仓库内移动** | `{Type=2, From, To}` | `{Type=2, User, Item=移动后目标格, To, From}`（广播） | `:10224-10266` |
| **拉仓库列表** | `{Type=3}` | `S.GuildStorageList{Items[112]}`（174，**只给请求者**） | `:10267-10279` |
| 仓库失败回执 | — | `S.GuildStorageItemChange{Type=3+Type, From, To}`（只给请求者；取物品权限失败例外，见 §7.4） | `:10123` |
| **存金币** | `C.GuildStorageGoldChange{Type=0, Amount}`（84） | `S.LoseGold`（本人）+ `S.GuildStorageGoldChange{Type=0, Name, Amount}`（广播，172） | `:10075-10094` |
| **取金币** | `{Type=1, Amount}`（Crystal 把任何非 0 值都当「取」） | `S.GainedGold`（本人）+ `S.GuildStorageGoldChange{Type=1, Name, Amount}`（广播） | `:10095-10119` |
| **buff 列表** | `C.GuildBuffUpdate{Action=0}`（131） | `S.GuildBuffList{GuildBuffs=全部}`（248，只给请求者） | `:10357-10360` |
| **买 buff** | `{Action=1, Id}` | `S.GuildStorageGoldChange{Type=2}`（有金币消耗时）→ `S.GuildBuffList{ActiveBuffs=[新增益]}` → `S.GuildStatus` ×在线成员 → 行会频道成功消息 | `GuildObject.cs:912-983`、`PlayerObject.cs:10361-10419` |
| **重新激活 buff** | `{Action=2, Id}` | `S.GuildBuffList{ActiveBuffs=[增益]}` → `S.GuildStorageGoldChange{Type=2}` | `GuildObject.cs:992-1017`、`:10420-10434` |
| **buff 到期** | — | `S.GuildBuffList{ActiveBuffs=[Active=false 的增益]}` | `GuildObject.cs:796-842` |

包序要点（实现里逐条照抄，测试有断言）：

* **创建**：`S.LoseGold` → `S.GuildMemberChange{Status=2}` → `S.GuildStatus`
  （`PlayerObject.cs:9780`、`:9829-9835`）。
* **登录**：`S.StartGame` → `MapInformation` → `NewMapInfo` → `UserInformation` → `SwitchGroup` →
  `S.GuildBuffList{GuildBuffs=全部}`（`:1223-1224`，**没有行会也发**）→ 任务包 →
  `S.GuildMemberChange{Status=1}` → `S.GuildStatus` → 有激活增益时 `S.GuildBuffList{ActiveBuffs}`
  （最后一条是本作修正，见 §12）。
* **加入行会**：`Status=2` → `S.GuildStatus` → `RefreshStats()` → `S.GuildBuffList{ActiveBuffs}`
  （`:9995-10010`），同时重置「仓库列表已拉取」标志（`:10006`）。
* **踢人**：`Status=3`（被踢）/`Status=4`（自己退）只发给**其余**在线成员；
  被移除者先收到一行 `ChatType.Guild` 文本，再收到空 `S.GuildStatus`（`GuildObject.cs:541-563`）。
* **升级**：`S.GuildStatus` 发给所有在线成员（`GuildObject.cs:685-694`）；
  没升级时 `S.GuildExpGain` 每 10 秒最多一次（`:696-703`）。
* **买 buff**：金币包 → 增益列表 → 行会状态 → 行会频道成功消息（`GuildObject.cs:952-978`、
  `PlayerObject.cs:10400-10418`）。
* **激活 buff**：增益列表 → 金币包（`GuildObject.cs:1011-1012`）。

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
| 8 | 广播 `Status=2` + `S.GuildStatus`；重置仓库列表节流 | `:9833-9835`、`:9831` |

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
| `CanStoreItem` | 8 | 存仓库 / 仓库内移动（`PlayerObject.cs:10141`、`:10226`） |
| `CanRetrieveItem` | 16 | 取仓库（`:10186`） |
| `CanAlterAlliance` | 32 | 联盟/敌对（**未做**） |
| `CanChangeNotice` | 64 | 改公告 |
| `CanActivateBuff` | 128 | 买/激活行会 buff（`:10362`、`:10421`） |

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
| 每级升级点 | `Guild_PointPerLevel`，`Math.Min(byte.MaxValue, SparePoints + n)` | `:660`、`Settings.cs:327` |
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
TotalBuffs=16
[Exp] Level-0=45000 …
[Cap] Level-0=5 …
[Required-0] ItemName= / Amount=1000000
[Buff-0] Id=1 / Icon=24 / Name=Reputation / LevelReq=0 / PointsReq=1 /
         TimeLimit=2400 / ActivationCost=0 / BuffAc=0 / … / BuffGoldRate=0
```

候选路径：`config/GuildSettings.ini` → 素材里社区数据库的
`Configs/GuildSettings.ini`（`CJLAAA`/`JEV` 等）。找不到时用代码默认值
（等级 22、金币 1,000,000、倍率 0.01、升级点 0、经验/人数表全 0、buff 列表为空）并打一条
`warn!`——与 Crystal「没配 `GuildSettings.ini` 时行会不升级、不限人数、没有 buff」的效果一致。

升级点的累加路径已经与 Crystal 对齐：`guild_gain_exp` 里
`spare_points = spare_points.saturating_add(point_per_level)`（等价 `Math.Min(byte.MaxValue, …)`），
测试 `guild_level_up_awards_spare_points_that_buffs_consume` 断言了「1000 经验 →
5 级 / 5 点 / 余数 500」以及 buff 购买对点数的扣减。

---

## 7. 行会仓库

### 7.1 数据形状

| 本作 | Crystal | 说明 |
|---|---|---|
| `Guild::storage: Vec<Option<GuildStorageSlot>>`，长度 112 | `GuildStorageItem[] StoredItems = new GuildStorageItem[112]`（`GuildInfo.cs:22`） | 下标即格子号；Crystal **没有**「仓库容量」配置项 |
| `GuildStorageSlot{ item: GuildStorageItem, stored_at: i64 }` | `GuildStorageItem{ Item, UserId }`（`Shared/Data/GuildData.cs:44-60`） | `stored_at` 是本作扩展（只审计，不参与判定） |
| 线上 `S.GuildStorageList` / `S.GuildStorageItemChange` | 同左 | 直接复用 `mir_protocol::data::GuildStorageItem` |

持久化时物品用 **`encode_slots(&[Some(UserItem)])` 的单格字节块**（与角色背包/装备同一套
编解码），存放在 `guild_storage.item BYTEA`；`user_id`、`stored_at` 单独成列
（`migrations/0006_guild_storage.sql`）。读档时 `decode_slots` 失败/越界的格子只记 `warn!`
并跳过（Crystal 的 `j < StoredItems.Length` 同样会丢弃越界项，`GuildInfo.cs:129`）。

> 行会仓库**不用** `S.NPCStorage`（`Shared/ServerPackets.cs:3212`）：那是个人仓库/拍卖一类
> NPC 界面的包；Crystal 的行会仓库只有 `S.GuildStorageList` / `S.GuildStorageItemChange`
> / `S.GuildStorageGoldChange` 三个包。

### 7.2 金币：没有独立的 `storage_gold`

Crystal 的 `GuildInfo.Gold` **既是行会资金也是仓库金币**：
`PlayerObject.GuildStorageGoldChange` 的存入/取出直接加减 `MyGuild.Gold`
（`PlayerObject.cs:10075-10119`），行会 buff 的花费也从同一份金币扣
（`GuildObject.cs:952-957`、`:1004`）。因此本作复用 `guilds.gold`，
**不新增** `guilds.storage_gold` 列——同一笔钱只保留一份事实来源。

### 7.3 安全区

Crystal 要求 `InSafeZone`（`MapObject.InSafeZone`，`MapObject.cs:168`；由
`Map.GetSafeZone` 判定，`Map.cs:2386-2395`：安全区是 `SafeZoneInfo{X, Y, Size}` 的
**Chebyshev 距离**判定，`Shared/Functions/Functions.cs:71-74`）。
本作从 `MapInfo.SafeZones`（`crates/mir-db/src/map.rs:30`）现算，
见 `World::in_safe_zone`。**例外**：`C.GuildStorageItemChange{Type=3}`（拉列表）不要求安全区
（`PlayerObject.cs:10131` 的 `&& type != 3`）。

### 7.4 操作语义（逐条照抄 `PlayerObject.cs:10121-10282`）

入口 `World::guild_storage_item_change(player_id, type, from, to)`：

| `Type` | 流程 | 失败回执 |
|---|---|---|
| `0` 存 | 不在行会 → 安全区 → `CanStoreItem` → 下标范围（背包 46 / 仓库 112）→ 背包有物品 → `BindMode.DontStore` → 目标格为空 → 移动 | 一律回 `S.GuildStorageItemChange{Type=3}`；「不在行会」「不在安全区」「没有权限」「目标格非空」另有系统聊天 |
| `1` 取 | 不在行会 → 安全区 → `CanRetrieveItem` → 下标范围 → 背包目标格为空 → 仓库有物品 → `DontStore` → 移动 | 回 `{Type=4}`；**权限不足时不回包**，只有系统聊天（Crystal 的不对称写法，`:10186-10191`） |
| `2` 仓库内移动 | 不在行会 → 安全区 → `CanStoreItem` → 两个下标都在仓库范围 → 来源格有物品 → `DontStore` → 交换 | 回 `{Type=5}`；权限不足另有聊天 |
| `3` 拉列表 | 不在行会 → **不查安全区** → `GuildCanRequestItems` 节流 → 回 `S.GuildStorageList` | 节流期间**静默丢弃**（连失败包都没有，`:10268`） |

成功包的 `User` 字段是**操作者的角色 ID**（`Info.Index`），不是对象 ID；
`Type=1`（取回）**不带物品**（客户端自己在仓库格与背包格之间搬），`Type=0/2` 带物品。

`World::guild_storage_gold_change(player_id, type, amount)`：

* `Type=0` 存入：玩家金币够 → 行会资金不会溢出 `uint.MaxValue` → 扣玩家钱（`S.LoseGold`）→
  加行会钱 → 广播 `S.GuildStorageGoldChange{Type=0, Name=角色名, Amount}`；
* `Type!=0` 取出：行会资金够 → 玩家金币不会溢出 `uint.MaxValue`（`CanGainGold`，
  `PlayerObject.cs:7638-7641`）→ **只有会长**（`MyGuildRank.Index == 0`）→ 扣行会钱 →
  `GainGold`（`S.GainedGold`）→ 广播 `{Type=1, Name, Amount}`。

### 7.5 列表节流与重登

`GuildCanRequestItems` 在 Crystal 里是 `PlayerObject` 的字段（`:146`）：
加入/创建行会时置 `true`（`:10006`、`:9831`），拉一次列表后置 `false`（`:10269`），
下线随对象销毁。本作把它放在 `World::guild_storage_requested`（按对象 ID 记「已拉取」），
`guild_player_offline` 清掉；加入/创建行会也会清掉。

### 7.6 被踢出后的物品

Crystal **不检查存放者**：`StoredItems[j].UserId` 只是记录，任何有 `CanRetrieveItem`
的成员都能取走它（`:10208-10219` 只判空、绑定与目标格）。因此：

* 被踢的人不再有行会 → 取物品时直接 `NotPartOfGuild`（并回一条失败包，`:10124-10129`）；
* 物品仍留在行会仓库，**其它成员可以取走**（测试 `kicked_member_cannot_retrieve_but_another_member_can`）。

### 7.7 简化

* **`CheckItem` / `SendItemInfo` 没做**：Crystal 在物品变动后给全行会补发 `S.NewItemInfo`
  （`GuildObject.SendItemInfo`，`GuildObject.cs:617-630`），本作没有「按需下发物品模板」的通道，
  只记 `debug!`。
* **`RefreshBagWeight` 不需要**：本作负重是即时计算的，没有缓存。

---

## 8. 行会增益（guild buff）

### 8.1 定义来源

`GuildSettings.ini` 的 `[Guilds] TotalBuffs=N` + `[Buff-0] … [Buff-(N-1)]`
（`Server/Settings.cs:1371-1375`、读入 ctor `Shared/Data/GuildData.cs:122-151`）：

| 键 | 字段 | 类型/默认 |
|---|---|---|
| `Id` | `GuildBuffInfo.Id` | `i32`，0 |
| `Icon` | `Icon` | `i32`，0 |
| `Name` | `Name` | `string`，"" |
| `LevelReq` | `LevelRequirement` | `u8`，0 |
| `PointsReq` | `PointsRequirement` | `u8`，**1**（`GuildData.cs:98`） |
| `TimeLimit` | `TimeLimit` | `i32`，0；单位**分钟**，0 = 永久 |
| `ActivationCost` | `ActivationCost` | `i32`，0 |
| `BuffAc`/`BuffMAC`/`BuffDc`/`BuffMc`/`BuffSc`/`BuffMineRate`/`BuffGemRate`/`BuffFishRate`/`BuffExpRate`/`BuffCraftRate`/`BuffSkillRate`/`BuffHpRegen`/`BuffMpRegen`/`BuffAttack`/`BuffDropRate`/`BuffGoldRate` | `Stats` 对应项 | 按 `ReadByte` 读（>255 或非法 → 0） |
| `BuffMaxHp`/`BuffMaxMp` | `Stat.HP` / `Stat.MP` | 按 `ReadInt32` 读 |

> 单位容易看错：`TimeLimit` 与 `ActiveTimeRemaining` 都是**分钟**，不是秒。
> `GuildObject.Process` 每分钟减 1（`Envir.cs:2330-2337` → `GuildObject.cs:806`），
> 客户端文案也是 `BuffLengthMinutes` / `TimeRemainingMinutes`
> （`Client/MirScenes/Dialogs/GuildDialog.cs:1237`、`:1241`）。

缺段时 Crystal 仍会 `Guild_BuffList.Add(new GuildBuffInfo(reader, i))`（全是默认值），
本作照抄；因此 `TotalBuffs` 大于实际段数时会得到 `Id=0`、`Name=""` 的占位项。

### 8.2 持有状态

`Guild::buffs: Vec<GuildBuff>`，元素与协议层 `GuildBuff` 相同：
`{ id, active, active_time_remaining }`。`Info` 不在结构里，按 `id` 从
`GuildSettings::guild_buff_info` 查（Crystal `Envir.FindGuildBuffInfo`，`Envir.cs:5026-5037`）。

持久化到 `guild_buffs(guild_id, buff_id, active, active_time_remaining)`，
与 Crystal 的 `GuildInfo.Save` 写 `BuffList`（`:235-239`）对应。

### 8.3 购买（`C.GuildBuffUpdate{Action=1}`）

校验顺序（`PlayerObject.cs:10361-10418` + `GuildObject.NewBuff`，`GuildObject.cs:912-983`）：

1. `CanActivateBuff`（128）→ 否则 `NoCorrectGuildRank`；
2. `Envir.FindGuildBuffInfo(id)` 存在 → 否则 `BuffNotExist`；
3. 行会还没有这个 buff → 否则 `BuffAlreadyObtained`；
4. `行会等级 >= LevelRequirement` → 否则 `GuildLevelRequirementNotMet`；
5. `升级点 >= PointsRequirement` → 否则 `GuildSparePointsInsufficient`；
6. `requiresGold = TimeLimit > 0 && ActivationCost > 0`，为真时
   `行会资金 >= ActivationCost` → 否则 `GuildBankFundsInsufficient`；
7. `NewBuff`：扣升级点；`requiresGold` 再扣金币并广播 `{Type=2}`；
   加入 `BuffList`（`Active = true`、`ActiveTimeRemaining = TimeLimit`）；
8. 广播 `S.GuildBuffList{ActiveBuffs=[新增益]}`，给所有在线成员发 `S.GuildStatus`
   （升级点变了），落盘，`RefreshAllStats()`；
9. 行会频道成功消息（点数+金币 / 金币 / 点数 / 免费四种文案，`:10400-10418`）。

### 8.4 重新激活（`C.GuildBuffUpdate{Action=2}`）

`GuildObject.ActivateBuff`（`:992-1017`）：已获得但 `Active == false` 的 buff，
金币够就 `Active = true; ActiveTimeRemaining = TimeLimit;` 扣金币；
广播 `S.GuildBuffList` → `S.GuildStorageGoldChange{Type=2}`；落盘并刷新属性。
**静默返回**（无聊天、无包）的两种情况：`Buff.Active == true`、金币不足（`:10432`）。

### 8.5 到期（每分钟）

`World::process_guild_buffs()` 对应 `GuildObject.Process`（`:796-842`）：

* `Info == null`（配置删了）或 `TimeLimit == 0`（永久）→ 跳过；
* `Active == false` → 跳过；
* `ActiveTimeRemaining -= 1`；`< 0` 时 `Active = false` 并收集；
* 有变化时广播 `S.GuildBuffList{ActiveBuffs=[…]}`（`Remove` 仍为 0，客户端看 `Active`），
  再 `RefreshAllStats()`。

`World::tick_guild_buffs` 用 `MINUTE_MS` 节流，`World::tick` 每 100ms 调它；
测试直接调 `process_guild_buffs()` 精确控制「第几分钟」。与 Crystal 的 `guildTime` 门限一样，
**一次 tick 只扣 1 分钟**：即使服务器卡顿 10 分钟，下一次 tick 也只结算一次
（`Envir.cs:2330-2332` 的 `if (Time >= guildTime) { guildTime = Time + Minute; … }`）。

Crystal 的行会 buff **没有暂停机制**（`GuildObject.Process` 不看安全区/离线）；
它的属性由在线成员的 `RefreshGuildBuffs` 叠加，成员离线自然不再享受。
`PauseBuff`/`BuffProperty.PauseInSafeZone` 只作用于 `MapObject.Buffs` 里的个人 buff，
与行会增益无关（见 `docs/world/buffs.md` §10）。

### 8.6 属性生效

`PlayerObject.RefreshGuildBuffs`（`:1824-1836`）把所有 `Active` 的 `Info.Stats`
无脑累加进 `Stats`。本作在 `World::refresh_stats`（`crates/mir-server/src/items.rs`）里
**装备之后、`apply_caps` 之前**调用 `guild_buffs::apply_guild_buffs`，位置与
`HumanObject.RefreshStats` 的 `RefreshGuildBuffs`（`HumanObject.cs:1747`）一致。

它**不**创建 `Player.buffs` 里的 `ActiveBuff`，所以**没有 `S.AddBuff`**；
客户端看到的只是 `S.GuildBuffList` 与 `S.GuildStatus`。

### 8.7 登录 / 加入行会时的下发

| 时机 | Crystal | 本作 |
|---|---|---|
| 登录 | `StartGame` 无条件 `S.GuildBuffList{GuildBuffs=全部}`（`:1223-1224`），置 `RequestedGuildBuffInfo = true` | 会话在 `SwitchGroup` 之后调 `World::send_guild_buff_info`；`guild_player_online` 兜底 |
| `Action=0` | `if (RequestedGuildBuffInfo) return;`（`:10358`）——登录后永远不会再回 | 同左（标志在 `World::guild_buff_info_sent`） |
| 加入行会 | `S.GuildStatus` → `RefreshStats()` → 有 buff 时 `S.GuildBuffList{ActiveBuffs}`（`:10003-10010`） | 同左 |
| 登录（已持有 buff） | **不发** `ActiveBuffs`（重登后客户端不知道哪些还开着） | **补发一条** `S.GuildBuffList{ActiveBuffs}`（本作修正） |

---

## 9. 持久化

### 9.1 表结构

`crates/mir-store/migrations/0005_guilds.sql`（核心）：

| 表 | 列 | Crystal |
|---|---|---|
| `guilds` | `id`（IDENTITY，接受显式值）、`name`（`lower(name)` 唯一）、`leader_character_id`、`level`、`experience`、`gold`、`spare_points`、`notice`（`\n` 连接）、`created_at`、`updated_at` | `GuildInfo.cs:9-14`、`:24` |
| `guild_ranks` | `(guild_id, rank_index)` 主键、`name`、`options` | `GuildRank`（`Shared/Data/GuildData.cs:1`） |
| `guild_members` | `character_id` **主键**（一个角色只能在一个行会）、`guild_id`、`rank_index`、`joined_at` | `GuildRank.Members`（`:62`） |

`crates/mir-store/migrations/0006_guild_storage.sql`（本次新增）：

| 表 | 列 | Crystal |
|---|---|---|
| `guild_storage` | `(guild_id, slot)` 主键、`item BYTEA`（`encode_slots(&[Some(UserItem)])`）、`user_id`、`stored_at`（本作扩展） | `StoredItems`（`GuildInfo.cs:22`、存档 `:224-233`） |
| `guild_buffs` | `(guild_id, buff_id)` 主键、`active`、`active_time_remaining`（分钟） | `BuffList`（`:23`、存档 `:235-239`） |

**没有** `guilds.storage_gold`：Crystal 只有一个 `Gold`，见 §7.2。

### 9.2 仓储函数（`crates/mir-store/src/repo.rs`）

| 函数 | 行为 |
|---|---|
| `create_guild(&mut PgConnection, &Guild)` | 写 `guilds` + 全部职位/成员/仓库格/buff；`id > 0` 时按显式 ID 写；重名 → `StoreError::GuildExists` |
| `load_guild(&mut PgConnection, id)` | 行会 + 五张表的内容；不存在 → `NotFound("行会")` |
| `load_guild_by_name(&mut PgConnection, name)` | 去空格 + 忽略大小写（`Envir.GetGuild` 语义） |
| `save_guild(&mut PgConnection, &Guild)` | `UPDATE guilds` + 职位/成员/仓库/buff **全量替换**；0 行 → `NotFound` |
| `delete_guild(&mut PgConnection, id)` | 删 `guilds` 一行（子表 `ON DELETE CASCADE`） |
| `list_guilds(&mut PgConnection)` | 五条查询拼装（启动时灌进世界） |
| `set_character_guild(&mut PgConnection, character_id, guild_id)` | 只写 `characters.guild_index` |

这些函数收 `&mut PgConnection` 而不是 `PgExecutor`：它们都是「一条主表 + 四张子表」的
多语句操作，调用方要把整组语句放进同一个事务（`&mut *tx` 可以直接传进来）。

### 9.3 服务端侧的写入队列

世界层是同步的、存储层是异步的，所以行会操作只把「要写什么」排进
`World::guild_writes`（`GuildWrite::Save` / `Delete` / `SetCharacterGuild`），由：

* 会话在每次处理完行会包、以及 `leave_world` 时调 `guild::persist_guild_writes`；
* 服务端 100ms 的 tick 循环定期取走（行会经验、buff 到期都可能在 tick 里发生）。

`AccountStore` 增加了四个方法（`save_guild` / `delete_guild` / `list_guilds` /
`set_character_guild`），`MemoryStore` 与 `PgAccountStore` 都实现了：
前者在内存里存 `Vec<GuildRecord>`（测试用），后者按 `mir_store` 的行会函数落库。

> **已知限制**：如果两个写入者恰好同时取队列，写入顺序可能与事件顺序不同
> （例如 Save 与随后的 Delete 被拆到两次落盘）。队列本身在锁内 `take`，不会丢事件；
> 要彻底有序需要把「写库」也放进世界锁，代价是 I/O 阻塞 tick。留作后续优化。

---

## 10. 测试清单

### 10.1 `crates/mir-server/tests/guild.rs`（27 个，行会核心）

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
| 27 | `real_data_guild_scripts_and_creation_flow` | 真实数据冒烟（§11） |

### 10.2 `crates/mir-server/tests/guild_storage.rs`（21 个，仓库 + buff）

| # | 用例 | 覆盖 |
|---|---|---|
| 1 | `store_item_checks_safe_zone_permission_and_slot` | 仓库外拒绝 → 无权限拒绝 → 打开 `CanStoreItem` 后成功；`Type=3` 失败包、广播 `Type=0`、`UserId`、背包清空 |
| 2 | `store_item_rejects_dont_store_bind_and_occupied_slot` | `BindMode.DontStore`、目标格非空 |
| 3 | `full_storage_rejects_new_items` | 填满 112 格后再存 → `TargetSlotNotEmpty` |
| 4 | `retrieve_item_requires_an_empty_inventory_slot` | 目标格被占 → `Type=4`；腾空后成功且包**不带 Item** |
| 5 | `retrieve_when_inventory_is_full_is_rejected` | 46 格背包全满 → 取回拒绝；腾一格后成功 |
| 6 | `retrieve_without_permission_sends_chat_but_no_failure_packet` | 取物品权限不足只有聊天、**没有失败包**；开权限后成功 |
| 7 | `storage_list_request_is_throttled_until_rejoin` | `S.GuildStorageList` 112 格内容、节流、加入行会重置、安全区外可拉列表 |
| 8 | `gold_deposit_and_withdrawal_broadcast_to_the_guild` | 存入/取出、`S.LoseGold`/`S.GainedGold`、广播 `Name/Type/Amount`、非会长取钱、资金不足 |
| 9 | `gold_deposit_is_capped_at_uint_max` | 存入正好 `u32::MAX` 可以，再多 1 → `GuildGoldLimitReached` |
| 10 | `gold_withdrawal_respects_the_player_gold_cap` | 玩家金币将溢出 → `PlayerGoldLimitReached` |
| 11 | `guild_buff_purchase_requires_permission_and_a_known_buff` | 无 `CanActivateBuff`、未知编号 |
| 12 | `guild_buff_purchase_checks_level_points_and_bank_gold` | 等级 / 升级点 / 银行金币三种拒绝；成功后点数金币扣减、包序（金币→列表→状态）、成功消息、重复购买 |
| 13 | `guild_level_up_awards_spare_points_that_buffs_consume` | 升级点累加（5 级 5 点）、经验封顶、买 buff 扣点 |
| 14 | `guild_buff_applies_stats_to_all_online_members_and_expires` | 全员 +50 HP、`S.GuildBuffList`、每分钟扣时、第 3 分钟失效、属性回退、到期落盘 |
| 15 | `guild_buff_reactivation_charges_gold_again` | 金币不足静默、重新激活的包序（列表→金币）、已激活静默、未获得拒绝 |
| 16 | `guild_buff_info_list_is_pushed_at_login_and_on_demand` | 登录无条件下发定义列表（含无行会玩家）、`Action=0` 的 `RequestedGuildBuffInfo` 语义、`id < 0` |
| 17 | `joining_a_guild_pushes_its_active_buffs` | 加入行会下发 `ActiveBuffs` + `RefreshStats` |
| 18 | `kicked_member_cannot_retrieve_but_another_member_can` | 被踢者取回拒绝、物品留在仓库、其他成员可取 |
| 19 | `storage_and_buffs_survive_a_store_roundtrip` | `MemoryStore` + `GuildRecord` 编解码往返：仓库格子、`stored_at`、buff 激活状态、重登后属性 |
| 20 | `guild_settings_parses_buff_sections_with_crystal_defaults` | 合成 `[Buff-N]`：`TotalBuffs`、字段/属性映射、缺段默认项、未知键忽略 |
| 21 | `real_guild_settings_buff_list_can_be_activated` | 真实 `GuildSettings.ini` 冒烟（§11） |

数据库侧另有：

* `crates/mir-store/tests/guild_roundtrip.rs`：建/读/回写/删 + 显式 ID + 重名 +
  **仓库格与 buff 的整份替换** + `guild_index`，整个事务回滚，需要 `MIR2_TEST_DATABASE_URL`；
* `crates/mir-server/tests/postgres_guild.rs`：`PgAccountStore` 的行会函数 +
  `guild_index` 整行往返，同样需要 `MIR2_TEST_DATABASE_URL`。

**方案说明（「重启后恢复」测试）**：默认测试集用 [`MemoryStore`]（`Vec<GuildRecord>`），
世界层只产生 `GuildWrite::Save(GuildRecord)`，测试用 `persist_guild_writes` 写进 store，
再在新 `World` 里 `load_guilds` 回来断言。这样覆盖了「世界视图 ↔ `GuildRecord` ↔
`UserItem` 字节块」的**全部编解码**，且无需临时 PostgreSQL；真实 SQL 的往返由上面两个
`MIR2_TEST_DATABASE_URL` 用例覆盖（默认跳过）。

---

## 11. 真实数据冒烟

### 11.1 NPC 创建脚本

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

### 11.2 真实 `[Buff-N]` 列表

`real_guild_settings_buff_list_can_be_activated` 读取候选路径里第一个存在的
`GuildSettings.ini`（当前素材是
`resource/community/suprcode-crystal-database/cjlaaa/Configs/GuildSettings.ini`，
`TotalBuffs=16`），打印每个 buff 的
`id / icon / name / level / points / time / cost / stats`，然后：

* 断言解析出至少 1 个 buff、且至少 1 个带属性；
* 选出第一个 `LevelReq=0 && PointsReq<=1` 且带属性的 buff（当前数据是
  `id=2 Specialized Crafter`，`CraftRatePercent=1`）；
* 用真实经验表（`Level-0=1000000`）打 1,000,001 经验把行会升到 1 级（`PointPerLevel=1`，
  刚好 1 个升级点），再 `C.GuildBuffUpdate{Action=1}` 购买该 buff；
* 断言 `active == true`、升级点归零。

---

## 12. 与 Crystal 的差异

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
8. **仓库格子的 `stored_at`**：本作扩展（Crystal 的 `GuildStorageItem` 只有 `Item` + `UserId`）。
   只用于审计，不参与任何判定。
9. **`guilds.storage_gold` 不存在**：Crystal 的仓库金币就是 `GuildInfo.Gold`（§7.2），
   本作复用 `guilds.gold`，没有第二份。
10. **`GuildCanRequestItems` / `RequestedGuildBuffInfo` 放在世界上**：Crystal 是
    `PlayerObject` 字段；本作没有把这些一次性标志加进 `Player`，而是按对象 ID 记在
    `World`（与 `guild_invites` 同一做法），下线时清理。
11. **buff 到期会落盘**：Crystal `GuildObject.Process` 不置 `NeedSave`，只有关服强制保存
    才写入（`Envir.cs:2202`、`:2614-2648`），崩溃时已到期的 buff 会「复活」。本作在到期时
    直接排一条 `GuildWrite::Save`（**修正**）。
12. **登录补发 `ActiveBuffs`**：Crystal 只在加入行会时发（`:10009-10010`），重登后客户端
    不知道哪些 buff 还开着；本作在 `guild_player_online` 末尾补一条（**修正**）。
13. **buff 列表登录下发的位置**：Crystal 在 `StartGame` 的 `DefaultNPC` 之后
    （`:1221-1224`）；本作没有 `DefaultNPC`，紧跟 `SwitchGroup` 发（`:1219`）。
14. **任务书与源码的差异**：任务书提到的 `GuildInfo.GetStorageIndex` / `CanStoreItem` /
    `GuildCanStoreItem` / `GuildCanRetrieveItem` 这些方法在 canonical Crystal clone 里
    **不存在**；实际的权限与边界检查全部内联在 `PlayerObject.GuildStorageItemChange`
    （`:10121-10282`）。本作按源码逐条移植，没有发明这些 helper。
15. **buff 的读取顺序**：Crystal 的 `.mgd` 保留 `BuffList` 的**购买顺序**；本作的
    `guild_buffs` 表按主键 `(guild_id, buff_id)` 读回，顺序变成 `buff_id` 升序。
    客户端是按 `GuildBuffInfos` 的定义顺序画界面、按 `Id` 查激活状态的
    （`Client/MirScenes/Dialogs/GuildDialog.cs:1121-1133`），因此没有可见影响。

---

## 13. Crystal 异常行为（照抄时踩到的坑）

1. **行会经验回写缺失**（`GuildObject.cs:644-704`）：
   ```csharp
   Info.Experience += expAmount;      // :652
   var experience = Info.Experience;  // :654
   while (experience > Info.MaxExperience) { … experience -= Info.MaxExperience; … }  // :656-674
   ```
   循环里的局部变量 `experience` 从不写回 `Info.Experience`，于是**每次加经验都会重新
   消耗同一份累计经验再升一级**。本作把余数写回（`World::guild_gain_exp`）。
2. **关闭权限用的是异或**（`GuildObject.cs:370`）：
   `Options ^= (GuildRankOptions)(1 << Option)` —— 当这一位本来就是 0 时，点「关闭」
   反而会**打开**权限。本作用「清位」语义（`&= !bit`）。
3. **同名行会比较忽略空格**（`Envir.cs:4836`）：`"Test Guild"` 与 `"TestGuild"` 算重名，
   但数据库里存的是原文。本作沿用同一比较语义，并用 `lower(name)` 唯一索引兜住精确重名。
4. **踢人只发 `Status=3` 给其他成员**（`GuildObject.cs:541-552`）：被踢的人自己
   收不到 `S.GuildMemberChange`，客户端只能靠空 `S.GuildStatus` 清界面。
5. **会长退出条件**（`GuildObject.cs:463-476`）：`Ranks[0].Members.Count < 2 &&
   Membercount < 2` 才允许解散；否则「只有一个会长、行会里还有别人」时**谁都退不了**。
6. **同职位名**：Crystal 不阻止两个职位同名（`ChangeRankName` 只查长度）。
7. **取物品权限失败不发失败包**（`PlayerObject.cs:10186-10191`）：与存物品分支的
   `Enqueue(p)` 不对称，客户端手里那些「锁定」的格子只能等下一次 `S.GuildStorageList`。
   本作照抄（测试 `retrieve_without_permission_sends_chat_but_no_failure_packet`）。
8. **`C.GuildStorageGoldChange` 的 `Type` 只有 0 是存入**（`:10075`、`:10095`）：
   任何非 0 值（包括 2/3——服务端自己广播的花费类型）都会走「取钱」分支。
   本作照抄这个 `if (type == 0) else` 结构。
9. **`C.GuildStorageItemChange{Type=3}` 绕过安全区检查**（`:10131`）：
   在安全区外也能拉仓库列表。本作照抄。
10. **`GuildStatus.BuffCount` 恒为 0**（`GuildObject.cs:216`）：
    `BuffCount = (byte)0,//(byte)BuffList.Count` 被注释掉了。本作照抄（真正的 buff
    数据走 `S.GuildBuffList`）。
11. **`RequestedGuildBuffInfo` 让 `Action=0` 变成死代码**（`:1224`、`:10358`）：
    登录时已经置 true，客户端之后再也拿不到「按需」列表。好在登录包已经带了定义列表。
12. **`GuildBuffInfo.TimeLimit` 的单位是分钟**：客户端 `BuffLengthMinutes` /
    `TimeRemainingMinutes`（`GuildDialog.cs:1237`、`:1241`），`Process` 每分钟减 1。
    按「秒」理解会得到 2400 分钟 = 40 小时的离谱时长。本作按分钟实现。
13. **buff 到期不落盘**（§12.11）：`GuildObject.Process` 改了 `Active` 却没 `NeedSave = true`。
14. **`GuildBuff` 的 `Info` 不上行**：客户端按 `Id` 从 `GuildBuffInfos` 查
    （`Client/MirScenes/Dialogs/GuildDialog.cs:1121-1133`），服务端也不存它。
15. **仓库物品没有「所有者」限制**：`UserId` 只记录，谁能取只看职位权限（§7.6）。

---

## 14. 未做（明确划出本次范围）

* **攻城 / 领地**：`C.GuildRequestWar`（`GuildWarReturn`）、`C.PurchaseGuildTerritory`、
  `C.GuildTerritoryPage`、`GuildObject.GoToWar`/`GTForSale`/`EndGT`（`:707-900`）全部未实现，
  收到包只记 `debug!` 日志。`GuildInfo` 里的 `GTRent`/`GTIndex`/`GTKey`/`GTPrice`/`GTBegin`
  也不落库。
* **行会联盟/敌对**：`AllyGuilds`/`WarringGuilds`（`GuildObject.cs:59-64`）、
  `CanAlterAlliance` 权限未实现。
* **投票**：`GuildInfo.Votes`/`Voting`/`LastVoteAttempt`（`:16-19`）未实现，
  `S.GuildStatus.Voting` 固定 `false`。
* **战旗**：`FlagImage`/`FlagColour`（`GuildInfo.cs:29-30`）未实现。
* **`NewbieGuild`**、**`EnableGuildInvite` 开关**、**行会联盟广播**：见 §12。
* **`CheckItem` / `S.NewItemInfo`**：仓库物品变动不会补发物品模板（§7.7）。
* **buff 图标/客户端展示**：`GuildBuffInfo.Icon` 原样下行，本作不解析图标素材。
* **`NewbieGuildBuffEnabled` / `NewbieGuildExpBuff`**（`Settings.cs:325-326`）：
  新手行会的额外 buff 未实现。
* **地图进入规则里的行会限制**（`RequiredGuild`/`NoGuild`）属于地图进入规则系统，未实现。
