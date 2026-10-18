# 攻城与城主任命（conquest / 沙巴克攻城）

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：
> `Server/MirObjects/ConquestObject.cs`（整份文件：时间表、开战/停战、计分、夺城）、
> `Server/MirDatabase/ConquestGuildInfo.cs`（运行期状态与 `.mcd` 存档）、
> `Server/MirDatabase/ConquestInfo.cs`（静态配表与 `Server.MirDB` 段）、
> `Server/MirEnvir/Envir.cs`（`LoadConquests` `:3119-3170`、`SaveConquests` `:2734-2749`、
> 每 10 秒的 `Process` `:2339-2346`）、
> `Server/MirObjects/NPC/NPCSegment.cs`（攻城 NPC 动作 `:4223-4298`）、
> `Server/MirObjects/NPC/NPCScript.cs`（`PriceRate` `:111-133`、收税 `:1211-1214`）、
> `Server/MirEnvir/Map.cs`（`GetConquest` `:2412-2422`）、
> `Server/MirObjects/PlayerObject.cs`（`CheckConquest` `:13949-13979`、`GetNameColour` `:1849-1891`）、
> `Server/MirObjects/Monsters/ConquestArcher.cs` / `TownArcher.cs`。
>
> Rust 侧实现：
> `crates/mir-server/src/conquest.rs`（[`Conquest`]、[`ConquestSettings`]、[`ConquestError`]、
> `impl World` 的装载/申请/时间表/计分/城主收益）、
> `crates/mir-server/src/npc.rs`（攻城 NPC 动作 + 城主加价与收税）、
> `crates/mir-server/src/monster.rs`（AI 80 攻城箭塔）、
> `crates/mir-server/src/world.rs`（战区名字颜色、战区 PK 保护、tick 挂载）、
> `crates/mir-server/src/maps.rs`（切图后的皇宫占领检查）、
> `crates/mir-store/migrations/0008_conquests.sql`（持久化）。
> 测试：`crates/mir-server/tests/conquest.rs`（25 个确定性用例）。

---

## 1. 结论先行

- **没有「攻城」专用协议包**。Crystal 的全部交互都在 NPC 脚本里：
  `SCHEDULECONQUEST`（申请）、`STARTCONQUEST`（GM 开/停战）、`TAKECONQUESTGOLD`（取税金）、
  `SETCONQUESTRATE`（设加价），解析在 `NPCSegment.cs:1055-1080`，执行在 `:4223-4298`。
  任务书里提到的 `C.GuildRequestWar` / `C.GuildWarReturn` 是**行会战**（guild vs guild）
  的包，**不是攻城**（见 §9）。
- **攻城的时间表来自配表**，不是运行期状态：`ConquestInfo.StartHour` / `WarLength` /
  七个星期布尔（`ConquestInfo.cs:29-41`）。运行期每 **10 秒**跑一次
  `ConquestObject.Process`（`Envir.cs:2339-2346`），其中时间表每 **60 秒**检查一次
  （`ScheduleTimer = Envir.Time + Settings.Minute`，`ConquestObject.cs:505`）。
- **申请攻城只是记一个 `AttackerID`**（`NPCSegment.cs:4293-4296`），真正开战由时间表
  在时间窗内触发（`ConquestObject.cs:482-503`）。**Crystal 不收购城费用、不查行会等级**，
  这些条件写在各服自己的 NPC 脚本 `#IF` 段里。
- **四种玩法**（`ConquestGame`，`Shared/Enums.cs:1888-1895`）：
  `CapturePalace`（走进皇宫即夺城）、`KingOfHill`（王座站桩积分）、
  `Classic`（皇宫里只剩一个行会即夺城）、`ControlPoints`（控制点积分），
  `Random` 没有实现分支。真实数据（`Jev/Server.MirDB`）用 `Classic`。
- **城主收益是「NPC 加价 + 税金池」**：攻城区 NPC 对非城主行会成员多收
  `NPCRate`%，差价进 `GoldStorage`（`NPCScript.cs:1211-1214`、`PlayerObject.cs:8150`），
  城主用 NPC 动作 `TAKECONQUESTGOLD` 一次性取走（`NPCSegment.cs:4223-4236`）。
- **战区里杀人**：不加 PK 值、只给凶手一条「你受到了法律的保护。」
  （`PlayerObject.cs:603-606`）；**但打人仍然会棕名**（`HumanObject.cs:7199-7200`
  没有 `WarZone` 判断）。
- **只有城主行会的成员能走 `ConquestIndex > 0` 的传送点**
  （`PlayerObject.cs:4489-4493`，真实数据里地图 `3` 有 3 条）。
- **建筑类对象没做**：守卫（箭塔）刷出来了并能射击；城门 / 城墙 / 器械 / 旗子 /
  控制点 NPC 的实体与血量都**没有**实现（见 §8）。

---

## 2. 常量速查

| 名称 | 值 | Rust 常量 | Crystal |
|---|---|---|---|
| 攻城处理周期 | 10000 ms | `CONQUEST_PROCESS_INTERVAL_MS` | `Envir.cs:2341`（`Settings.Second * 10`） |
| 时间表检查周期 | 60000 ms | `CONQUEST_SCHEDULE_INTERVAL_MS` | `ConquestObject.cs:505` |
| 「王者之城」满分 | 18 | `MAX_KING_POINTS` | `ConquestObject.cs:74` |
| 控制点满分 | 6 | `MAX_CONTROL_POINTS` | `ConquestObject.cs:75` |
| 控制点争夺半径 | 3 | `CONTROL_POINT_RANGE` | `ConquestObject.cs:740` |
| 一天的分钟数 | 1440 | `MINUTES_PER_DAY` | `ConquestObject.cs:467-469` |
| 攻城申请费 | **0（默认）** | `ConquestSettings::war_cost` | **Crystal 没有这个配置**（见 §9） |
| 申请所需行会等级 | **0（默认）** | `ConquestSettings::required_guild_level` | **Crystal 没有这个配置** |

---

## 3. 数据装载

### 3.1 静态配表（`Server.MirDB` → `db.conquests`）

`mir-db` 早已把 `ConquestInfo` 段读出来了（`crates/mir-db/src/conquest.rs`），
本作只补了两层访问器与运行时结构：

| Rust | Crystal |
|---|---|
| `GameData::conquests()` / `GameData::conquest(index)` / `GameData::conquest_count()` | `Envir.ConquestInfoList`（`Envir.cs:122`） |
| [`Conquest::info`] | `ConquestObject.Info`（`ConquestObject.cs:16-20`） |
| [`Conquest::map`] / [`palace_map`] / [`extra_maps`] | `ConquestObject.ConquestMap` / `PalaceMap` / `Info.ExtraMaps`（`:60-61`） |

`World::init_conquests` 做 Crystal `Envir.LoadConquests`（`Envir.cs:3126-3168`）的事：

1. 遍历 `db.conquests`；
2. `MapIndex` 解析成**地图文件名**（找不到地图就跳过，Crystal 的 `if (tempMap == null) continue;`，
   `:3130-3132`）；
3. `PalaceIndex` / `ExtraMaps` 同样解析；
4. 建立 [`Conquest`]，初始 `owner = -1`、`attacker = -1`、`gold_storage = 0`、
   `npc_rate = 0`、未开战、控制点归属 `-1`。

### 3.2 运行期状态

| 字段 | Crystal | 说明 |
|---|---|---|
| `owner` | `ConquestGuildInfo.Owner`（`ConquestGuildInfo.cs:17`） | `-1` = 无主 |
| `attacker` | `ConquestGuildInfo.AttackerID`（`:19`） | `-1` = 没人申请 |
| `gold_storage` | `ConquestGuildInfo.GoldStorage`（`:18`） | 城主税收池 |
| `npc_rate` | `ConquestGuildInfo.NPCRate`（`:20`） | 城主加价百分比 |
| `war_is_on` | `ConquestObject.WarIsOn`（`:212-223`） | 开战标志 |
| `start_type` / `game_type` | `ConquestObject.StartType` / `GameType`（`:68-69`） | 本场战争的类型/玩法 |
| `war_start_time` / `war_end_time` | `ConquestObject.WarStartTime` / `WarEndTime`（`:65-66`） | Unix 毫秒；`war_end_time` 只对 `Forced` 有值 |
| `king_points` | `ConquestObject.KingPoints`（`:77`） | 「王者之城」比分 |
| `control_points` | `ConquestObject.ControlPoints`（`:52-56`） | 控制点归属 + 各会分数 |
| `dirty` | `ConquestGuildInfo.NeedSave`（`:24`） | 待落盘 |

### 3.3 真实数据

`resource/community/suprcode-crystal-database/Jev/Server.MirDB` 里只有 **1 条**攻城区
（`GameData::conquest_count() == 1`）：

```
db.conquests 数量 = 1
ConquestInfo {
    index: 1, full_map: false,
    location: (641, 287), size: 30, name: "Sabuk Wall",
    map_index: 172, palace_index: 245,
    guard_index: 12, gate_index: 1, wall_index: 3, siege_index: 0, flag_index: 0,
    guards: 12 × { mob_index: 499, name: "Guard", repair_cost: 1000 },
    gates:  1 × { index: 1, location: (672, 330), mob_index: 568, name: "SabukGate" },
    walls:  3 × { LeftWall/CenterWall/RightWall, mob_index: 569/570/571 },
    sieges: [], flags: [],
    start_hour: 18, war_length: 30,
    conquest_type: 0 (Request), game: 3 (Classic),
    monday..sunday: true × 7,
    king_location: (0,0), king_size: 0,
    control_point_index: 0, control_points: [],
}
```

也就是说：真实数据是**每天 18:00 开战、持续 30 分钟、`Request` + `Classic`**，
判定完全落在「皇宫（地图 245）里只剩一个行会」上，**不需要旗子或控制点**。

---

## 4. 申请攻城

Crystal 的唯一入口是 NPC 脚本动作（`NPCSegment.cs:4287-4298`）：

```csharp
case ActionType.ScheduleConquest:
    var conquest = Envir.Conquests.FirstOrDefault(z => z.Info.Index == tempInt);
    if (conquest == null) return;
    if (player.MyGuild != null && player.MyGuild.Guildindex != conquest.GuildInfo.Owner
        && !conquest.WarIsOn)
        conquest.GuildInfo.AttackerID = player.MyGuild.Guildindex;
```

本作把它做成 [`World::request_conquest_war_at`]（生产路径
[`World::request_conquest_war`] 传 `wall_now()`），并接在 NPC 动作
`SCHEDULECONQUEST <index>` 上：

| 校验 | 变体 | Crytal 出处 |
|---|---|---|
| 不在世界 | `NotInWorld` | 本作扩展 |
| 不在行会 | `NotInGuild` | `CheckType.ConquestAvailable` 要求 `MyGuild != null`（`:2764-2767`） |
| 不是会长 | `NotLeader` | NPC 脚本 `[@REQUESTWAR]` 的门槛（`NPCScript.cs:1066-1069`） |
| 行会等级不够 | `LevelTooLow` | **本作扩展**（`ConquestSettings::required_guild_level`，默认 0） |
| 行会资金不够 | `NotEnoughGold` | **本作扩展**（`ConquestSettings::war_cost`，默认 0） |
| 找不到攻城区 | `NotFound` | `if (conquest == null) return;`（`:4291`） |
| 正在开战 | `AtWar` | `&& !conquest.WarIsOn`（`:4293`） |
| 自己已是城主 | `AlreadyOwner` | `Guildindex != conquest.GuildInfo.Owner`（`:4293`） |
| 已经有人申请 | `AlreadyScheduled` | `CheckType.ConquestAvailable` 把 `AttackerID != -1` 判为不可用（`:2765`） |

申请成功只是把 `attacker` 记成自己的行会：

- 若**此刻在时间窗内**，本作立刻跑一轮时间表（Crystal 最多等 60 秒），
  返回 `WarRequestReport.war_started == true`；
- 否则返回 `next_war_at`，等时间表到点开战。

`WarRequestReport` 还带回 `gold_spent`（本作扩展的收费）与 `guild_id`。

---

## 5. 时间表（开战 / 停战）

移植自 `ConquestObject.AutoSchedule`（`:465-506`），逻辑逐行对齐：

```csharp
int start  = StartHour * 60;
int finish = StartHour * 60 + WarLength;
int now    = Envir.Now.Hour * 60 + Envir.Now.Minute;

if (WarIsOn && StartType == Forced && WarEndTime <= Envir.Now) EndWar(Info.Game);

if (StartType != Forced) {
    if (WarIsOn && (now > start && finish <= now)) EndWar(Info.Game);
    else if (start <= now && finish > now && CheckDay()) {
        if (!WarIsOn) {
            if (Info.Type == Request) { if (AttackerID != -1) StartWar(Info.Game); }
            else StartWar(Info.Game);
        }
    }
}
```

要点：

- **`Request` 类型必须有人申请**才会开战；`Auto` 类型到点就开（`:486-499`）。
- `Forced`（GM 用 `STARTCONQUEST` 强制）走 `WarEndTime` 这一条，
  `WarEndTime = WarStartTime + WarLength 分钟`（`AtWarChanged`，`:320-325`）。
- 停战判定是「**当天第几分钟**」，跨午夜会失效（见 §10）。
- `[... World::process_conquests_at(now_wall)`] 是纯函数式的驱动点：测试与 GM 工具
  传入任意 Unix 毫秒即可复现，不依赖真实时钟。

---

## 6. 玩法与城主判定

`ScorePoints` 只有 `KingOfHill` / `Classic` / `ControlPoints` 三个分支
（`ConquestObject.cs:511`、`:716-905`），`CapturePalace` 靠「切图」触发，
`Random` 没有实现。

### 6.1 `Classic`（真实数据用的就是它）

```csharp
int guildCounter = 0; GuildObject takingGuild = null;
for (每个皇宫里活着的玩家) {
    if (MyGuild != null) { if (takingGuild == null || takingGuild != MyGuild) guildCounter++;
                           takingGuild = MyGuild; }
    else guildCounter++;
}
if (guildCounter == 1 && takingGuild != Guild) {
    if (StartType == Request && takingGuild.Guildindex != GuildInfo.AttackerID) return;
    TakeConquest(null, takingGuild);
}
```

- `guildCounter` 数的是**行会切换次数**：同一行会的多个人算 1；出现第二个行会、
  或者中间夹了一个无行会玩家，计数就继续加。
- 因此「皇宫里只剩一个行会且没有散人」才算占领。
- `Request` 类型下**只有申请方**能夺城；别人占住皇宫什么也不会发生。
- `Classic` 夺城**不结束战争**（只有 `CapturePalace` 会 `EndWar`，`:575`）。

### 6.2 `KingOfHill`

站进 `KingLocation ± KingSize` 且在自己行会名下：每 10 秒 +1 分（上限 18），
同时**其它所有有分的行会 -1**（`:805-840`）。分数最高的行会（从当前城主起算）
立刻夺城（`:844-869`）。

### 6.3 `ControlPoints`

每个控制点各自维护「各会分数」：站在控制点 3 格内的活人给本会 +1（上限 6）、
把其它会 -1；分最高的会拿到这个控制点，并全服公告
「【攻城】X 占领了控制点 Y。」（`:722-800`）。停战时控制点最多的行会当城主
（`EndWar` 的 `ControlPoints` 分支，`:921-962`）。

### 6.4 `CapturePalace`

玩家**切图进入皇宫**时结算：`PlayerObject.CheckConquest(true)`（`:13949-13966`，
由 `CompleteMapMovement` 在 `:4617` 调用）→ `TakeConquest(player)` → 立刻 `EndWar`。
本作把这一步接在 `World::change_map` 的末尾。

### 6.5 `TakeConquest`（`ConquestObject.cs:551-632`）

```csharp
if (winningGuild.Conquest != null) return;                       // 一行会只能有一座城
if (StartType == Request && winningGuild.Guildindex != AttackerID) break;
if (Guild != null) { Guild.Conquest = null; AttackerID = 上一任城主; }
GuildInfo.Owner = winningGuild.Guildindex;
Guild.Conquest = this;
```

本作对应 [`World::take_conquest`]，返回 `bool`（被校验拦下时 `false`，
Crystal 是静默 `return`/`break`）。夺城后：

- 全服公告「【攻城】X 的新城主是 Y。」；
- 新城主行会的在线成员收一次 `S.GuildStatus`（Crystal `UpdatePlayers`，`:634-650`）；
- 刷新战区名字颜色。

---

## 7. 城主收益

### 7.1 加价（`NPCRate`）

`NPCScript.PriceRate`（`:111-133`）：

```csharp
if (callingNPC.Conq == null || baseRate) return callingNPC.Info.Rate / 100F;
if (player.MyGuild != null && player.MyGuild.Guildindex == callingNPC.Conq.GuildInfo.Owner)
    return callingNPC.Info.Rate / 100F;
return (((Rate / 100F) * Conq.GuildInfo.NPCRate) + Rate) / 100F;
```

- [`World::conquest_price_rate`] 是它的实现；
- NPC 归属于哪个攻城区由 `NpcInfo.Conquest`（`Npc.conquest`）决定
  （Crystal `ConquestObject.LoadNPCs`，`:265-306`）；
- 城主行会成员按**基础价**买东西；
- 设置加价：NPC 动作 `SETCONQUESTRATE <index> <percent>`（`:4237-4249`），
  只有城主行会能改（[`World::set_conquest_rate`]）。

### 7.2 税金池

- 买东西时：`GoldStorage += cost - baseCost`（`NPCScript.cs:1211-1214`）；
- 修装备时：`GoldStorage += cost - baseCost`（`PlayerObject.cs:8150`）；
- 本作在 `World::buy_item` / `World::repair_item` 里调
  `collect_conquest_tax`（`crates/mir-server/src/npc.rs`），差价按
  「实付 − 基础价」计算，负数夹到 0（C# 的 `uint` 相减会回绕）。
- 取钱：NPC 动作 `TAKECONQUESTGOLD <index>`（`:4223-4236`）→
  [`World::take_conquest_gold`]，钱进**行会资金**并发
  `S.GuildStorageGoldChange{Type = 3}`。

---

## 8. 战区（WarZone）与守卫

### 8.1 战区判定

`Map.GetConquest`（`Map.cs:2412-2422`）+ `PlayerObject.CheckConquest`
（`PlayerObject.cs:13949-13979`）：

- 主地图：`FullMap` 或坐标落在 `Location ± Size` 内，且**正在开战**；
- 皇宫 / 附加地图：开战时**整张图**都算。

本作不缓存 `WarZone` 布尔，按当前状态现算（[`World::in_war_zone`]），
与 Crystal「每次移动后重算」等价，并且不需要给 `Player` 加字段。

### 8.2 战区的影响

| 行为 | Crystal | 本作 |
|---|---|---|
| 名字颜色 | 观察者视角：同会/无行会绿、敌对行会橙（`PlayerObject.cs:1861-1870`） | 一律绿（只发自己视角的颜色，见 §10） |
| 杀人加 PK 值 | **不加**，给凶手「你受到了法律的保护。」（`:603-606`） | 照抄 |
| 打人棕名 | **仍然会棕**（`HumanObject.cs:7199-7200` 没有 `WarZone` 判断） | 照抄 |

### 8.4 传送点限制

`MovementInfo.ConquestIndex > 0` 的传送点只有**拥有这座城**的行会成员能过
（`PlayerObject.cs:4489-4493`）：

```csharp
if (info.ConquestIndex > 0) {
    if (MyGuild == null || MyGuild.Conquest == null) continue;
    if (MyGuild.Conquest.Info.Index != info.ConquestIndex) continue;
}
```

本作在 `World::queue_movement_trigger`（`crates/mir-server/src/maps.rs`）里按
[`World::guild_conquest`] 判定；真实数据里地图 `3` 有 3 条这样的传送点
（`conquest_index = 1`）。

### 8.5 攻城箭塔（AI 80）
`ConquestArcher` 是 `TownArcher` 的子类（`ConquestArcher.cs:6`）：

- `FindTarget` 跳过「本行会已经拥有这座城」的玩家、跳过隐身、要求 `WarIsOn`
  （`:53`）；
- 攻击距离 `TownArcher.AttackRange = 10`（`TownArcher.cs:10`）；
- 站桩不动（`CanMove` 要求 `Route.Count > 0`，`:11-14`）。

本作：

- [`MonsterKind::is_conquest_archer`] 认 AI 80；
- [`World::spawn_conquest_archers`] 按 `ConquestInfo.Guards` 刷出，
  并把 `Monster.conquest` 绑到攻城区上（模板 AI 不是 80 就跳过，
  Crystal `ConquestGuildArcherInfo.cs:525`）；
- `find_target` 里加了「未开战不索敌」+「不射城主行会成员」两条过滤；
- `process_target` 按守卫的「视野半径内攻击、不追击」处理。

**简化**：Crystal 走 `S.ObjectRangeAttack` + `ProjectileAttack`（`TownArcher.cs:39`、`:47`），
本作还没移植远程抛射物协议，箭塔按普通攻击广播结算（伤害判定仍是远程）。

---

## 9. 协议与 NPC 动作

### 9.1 没有新建协议包

Crystal 的攻城**没有**任何专属包：

| 包 | 实际用途 | 出处 |
|---|---|---|
| `S.GuildRequestWar` | 行会战：让客户端弹「输入敌对行会名」 | `Shared/ServerPackets.cs:4709`、`NPCScript.cs:1071` |
| `C.GuildWarReturn` | 行会战：把名字回填给服务端 | `Shared/ClientPackets.cs:1787`、`PlayerObject.cs:10283-10327` |
| `C.GuildTerritoryPage` / `C.PurchaseGuildTerritory` | **行会领地租赁**（`Envir.GTMapList`，另一套数据表） | `PlayerObject.cs:10438-10525` |
| `S.GuildStatus` | 行会面板；**没有**城主/税收字段（`Shared/ServerPackets.cs:4533-4588`） | —— |

所以：**攻城不新增包**，全部走 NPC 脚本 + `S.Chat` 系统公告。
`crates/mir-protocol` 本轮**没有改动**。

### 9.2 本作支持的攻城 NPC 动作

| 动作 | Crystal | 本作 |
|---|---|---|
| `SCHEDULECONQUEST <index>` | `:4287-4298` | `request_conquest_war` |
| `STARTCONQUEST <index>` | `:4250-4286`（开/停战切换） | `toggle_conquest_war` |
| `TAKECONQUESTGOLD <index>` | `:4223-4236` | `take_conquest_gold` |
| `SETCONQUESTRATE <index> <percent>` | `:4237-4249` | `set_conquest_rate` |

`CONQUESTGUARD` / `CONQUESTGATE` / `CONQUESTWALL` / `CONQUESTREPAIRALL` / `OPENGATE` /
`CLOSEGATE` 都**没有**实现（需要建筑对象，见 §11）。
`#IF` 侧的 `CHECKCONQUEST` / `CONQUESTOWNER` / `CONQUESTAVAILABLE` 与
`$CONQUESTGOLD()` / `$CONQUESTRATE()` / `$CONQUESTSCHEDULE()` 变量同样未实现。

---

## 10. 持久化

Crystal 的存档是 `Conquests/<index>.mcd` 一个二进制文件
（`Envir.SaveConquests`，`:2734-2749`；`LoadConquests`，`:3119-3170`），
只写 `NeedSave` 的那些。本作落 `conquests` 表
（`crates/mir-store/migrations/0008_conquests.sql`）：

| 列 | Crystal |
|---|---|
| `conquest_index` | `ConquestInfo.Index`（也是文件名） |
| `owner_guild_id` / `attacker_guild_id` | `ConquestGuildInfo.Owner` / `AttackerID`（`-1` → `NULL`） |
| `gold_storage` / `npc_rate` | `ConquestGuildInfo.GoldStorage` / `NPCRate` |
| `war_is_on` / `war_start_time` / `war_end_time` | `ConquestObject.WarIsOn` / `WarStartTime` / `WarEndTime` |
| `start_type` / `game_type` | `ConquestObject.StartType` / `GameType` |
| `schedule_start_hour` / `schedule_war_length` / `schedule_days` | 时间表快照（`ConquestInfo.StartHour`/`WarLength`/星期布尔） |
| `king_points` / `flag_owners` | `ConquestObject.KingPoints` / 控制点归属（文本编码） |
| `next_war_at` | **本作扩展**：下一次开战时刻（Crystal 每分钟现算） |

流程：

- 启动：`server.rs` 先 `init_conquests()`（配表）→ 刷箭塔 → `list_conquests()` →
  [`World::load_conquests`]（状态覆盖配表）；
- 运行：tick 里状态一改就 `dirty = true`，主循环用
  [`persist_conquest_writes`] 落盘（与行会落盘同一条路）。

---

## 11. 未做清单

| 项目 | Crystal 出处 | 说明 |
|---|---|---|
| 城门 / 城墙 / 器械实体 | `ConquestGuildGateInfo` / `ConquestGuildWallInfo` / `ConquestGuildSiegeInfo`（`ConquestGuildInfo.cs:94-476`） | AI 81 / 82 / 72；血量、修理（`GetRepairCost`/`Repair`）、`OPENGATE`/`CLOSEGATE` 全未实现 |
| 箭塔血量与「复活」 | `ConquestGuildArcherInfo.Alive`（`:486`、`:503-516`） | 本作不存血量，重启后满血重刷；`CONQUESTGUARD <index> <n>` 修复动作未实现 |
| 旗子 NPC | `ConquestGuildFlagInfo.Spawn`（`:218-243`） | 旗子贴图/颜色随城主变化（`Guild.Info.FlagImage`/`FlagColour`）未实现；对计分无影响 |
| 控制点 NPC 对象 | `ConquestGuildFlagInfo` | 计分与归属已实现，但控制点没有可点击的旗子对象 |
| NPC 可见性 | `ConquestObject.NPCVisibility`（`:375-440`）、`NpcInfo.ConquestVisible` | 开战时才显示/隐藏的攻城 NPC 未实现 |
| 王座圈装饰 | `ConquestObject.CreateZone`（`:669-714`） | `Spell.TrapHexagon` 装饰对象未实现（只影响观感） |
| 攻城 NPC 绑定列表 | `ConquestObject.ConquestNPCs` / `LoadNPCs`（`:265-306`） | 本作按 `NpcInfo.Conquest` 现算，不做列表 |
| 修理费用（城门/城墙/器械） | `GetRepairCost`（`:168-182` 等） | 未实现 |
| 行会战（guild vs guild） | `GuildObject.WarringGuilds`、`PlayerObject.GuildWarReturn`（`:10283-10327`） | 与攻城无关，`C.GuildWarReturn` 目前只记日志 |
| 行会领地租赁 | `GTMapList`、`GetGuildTerritories`/`PurchaseGuildTerritory`（`:10438-10525`） | 另一套数据表，未实现 |
| 攻城 GM 变量 | `$CONQUESTGOLD` / `$CONQUESTRATE` / `$CONQUESTSCHEDULE` / `$CONQUESTOWNER`（`NPCSegment.cs:1420-1460`） | 未实现 |

---

## 12. Crystal 异常行为

1. **`ConquestGuildSiegeInfo.Spawn` 判了错的 AI**（`ConquestGuildInfo.cs:138`）：
   `if (monsterInfo.AI != 72) return;` 又 `if (monsterInfo.AI == 72) Gate = (Gate)…`。
   AI 72 是 `FinialTurtle`（`MonsterObject.cs:171-172`），城门是 AI 81。
   也就是说**器械永远刷不出来**。本作直接不实现器械。
2. **`EndWar` 之后 `StartType` 不复位**（`ConquestObject.cs:915-967`）：
   GM 用 `STARTCONQUEST` 强制开过一次之后，`AutoSchedule` 的
   `if (StartType != Forced)` 永远不成立，**定时攻城从此失效**。
   本作在停战时把 `start_type` 复位成配表的类型（记录在案的偏离）。
3. **定时开战不发公告**：`StartWar` 只 `BroadcastInfo()`（`:907-913`），
   只有 GM 的 `STARTCONQUEST` 才 `ReceiveChat(WarStarted, System)`（`NPCSegment.cs:4261-4280`）。
   本作两条路径都发 `S.Chat` 系统公告。
4. **`Classic` 的空引用**：皇宫里只有一个**散人**时 `guildCounter == 1`
   且 `takingGuild == null`，`takingGuild.Guildindex`（`:895`）会 NRE。
   本作直接跳过。
5. **跨午夜的停战判定会失效**：`finish = StartHour*60 + WarLength` 是「当天第几分钟」，
   `WarLength` 让 `finish` 超过 1440 时（例如 23:30 开战 60 分钟），
   `finish <= now` 永远不成立，战争不会自动结束。
6. **非 `Forced` 路径不写 `WarStartTime`**（`:482-503`），
   但 `AtWarChanged` 的 else 分支又依赖它——本作补写一个观测用的开战时刻。
7. **攻城没有费用/等级门槛**：`Settings.cs` 里只有行会战的 `Guild_WarCost = 3000`
   （`:329`，配置项 `[Guilds] WarCost`，`:1344`），没有 `Castle_WarCost`。
   本作把门槛做成可配置（默认 0 = Crystal 行为）。
8. **`S.GuildStatus` 没有城主/税收字段**（`Shared/ServerPackets.cs:4533-4588`），
   `GuildTerritoryPage` 也不是攻城——它是行会领地租赁。

---

## 13. 与 Crystal 的偏离（记录在案）

| 偏离 | 原因 |
|---|---|
| 战区名字颜色一律绿 | Crystal 的 `GetNameColour` 是**观察者视角**（敌对行会橙），而本作的名字颜色是按玩家缓存一份再广播；只发「自己视角」（恒绿）最简单且与 `RefreshNameColour` 一致 |
| 定时开战也发系统公告 | 任务要求「向全服广播战争开始」，Crystal 只在 GM 路径发 |
| 停战复位 `start_type` | 见 §12.2 |
| 申请攻城可配费用/等级 | 任务要求校验；Crystal 无（默认 0 保持兼容） |
| 申请后若在时间窗内立刻开战 | Crystal 最多等 60 秒的下一轮 `AutoSchedule`；本作直接跑一轮 |
| 箭塔按普通攻击广播 | 远程抛射物协议（`S.ObjectRangeAttack`）未移植 |
| `next_war_at` 落库 | Crystal 每分钟现算；本作缓存一份便于 GM/监控 |
