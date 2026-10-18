# 任务系统（脚本 / 接取 / 目标推进 / 奖励 / 持久化）

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：`Server/MirDatabase/QuestInfo.cs`（脚本解析、`CanAccept`、`CreateClientQuestInfo`）、
> `Server/MirDatabase/QuestProgressInfo.cs`（进度、目标判定、超时、进度文本）、
> `Server/MirObjects/PlayerObject.cs`（`AcceptQuest`/`FinishQuest`/`AbandonQuest`/`ShareQuest`/
> `CheckNeedQuest*`/`GetQuestInfo`）、`Server/MirObjects/NPC/NPCScript.cs`（`ParseQuests`）、
> `Server/MirObjects/NPC/NPCSegment.cs`（`CHECKQUEST` 条件）、
> `Server/MirDatabase/CharacterInfo.cs`（存档）、`Shared/Data/ClientData.cs`（`ClientQuestInfo` /
> `ClientQuestProgress`）、`Shared/ServerPackets.cs` / `Shared/ClientPackets.cs`（任务包）。
>
> Rust 侧实现：
> `crates/mir-db/src/quest_script.rs`（**纯数据**脚本解析）、
> `crates/mir-server/src/quests.rs`（运行时状态、包、存档编解码）、
> `crates/mir-server/src/gamedata.rs`（脚本缓存 + 任务头部查询）、
> `crates/mir-server/src/npc.rs`（`CHECKQUEST` 条件与三个任务动作）、
> `crates/mir-server/src/world.rs`（`Player::quests` / `completed_quests` / `wall_now` / tick）、
> `crates/mir-server/src/monster.rs`（击杀推进）、`crates/mir-server/src/items.rs`（收集推进）、
> `crates/mir-server/src/session.rs`（`C.AcceptQuest` 等四个包 + 登录下发 + 下线存盘）、
> `crates/mir-store/migrations/0004_character_quests.sql`（`characters.quests`）。
> 测试：`crates/mir-server/tests/quests.rs`（26 个用例，含真实数据冒烟）、
> `crates/mir-db/src/quest_script.rs` 内 10 个单元测试、`crates/mir-server/src/quests.rs` 内 5 个单元测试、
> `crates/mir-db/src/npc_script.rs` 内 2 个 `[Quests]` 用例。

---

## 1. 结论先行

- **任务数据分两半**：数据库里只有**头部**（编号/名字/分组/文件/等级/职业/前置/时限/四条提示，
  `QuestInfo.cs:85-110`），描述、目标、奖励全在 `Envir/Quests/<file>.txt`（`QuestInfo.cs:130-146`）。
  本作照此拆成 `mir_db::QuestInfo`（已有）+ `mir_db::quest_script::QuestScript`（本次新增）。
- **谁是任务的发布 NPC / 交付 NPC 不在数据库里**：`QuestInfo.NpcIndex` / `FinishNpcIndex`
  这两个字段**没有**落盘（`QuestInfo.cs:85-110` 不读它们），而是 `NPCScript.ParseQuests`
  从 NPC 脚本的 `[Quests]` 段回填（`NPCScript.cs:685-721`）：**正数=发布、负数=交付**。
  实测这份样本数据库的 `NpcInfo.CollectQuestIndexes` / `FinishQuestIndexes` **全是空的**，
  所以 NPC 脚本的 `[Quests]` 是**唯一**可用的关联来源。
- **`FinishNpcIndex` 有默认值**：没有单独的交付 NPC 时，发布者自己就是交付者
  （`QuestInfo.cs:31-35` 的 `FinishNpcIndex => _finishNpcIndex == 0 ? NpcIndex : _finishNpcIndex`）。
  本作按同样规则回退。
- **任务用到的协议包全是既有的**：本次实现只用到
  `C.AcceptQuest` / `C.FinishQuest` / `C.AbandonQuest` / `C.ShareQuest` 与
  `S.NewQuestInfo` / `S.ChangeQuest` / `S.CompleteQuest` / `S.SendOutputMessage`，
  **没有改 `crates/mir-protocol`**（协议里没有 `C.SelectQuestReward`，也不需要）。
- **可选奖励不需要额外包**：`C.FinishQuest` 自带 `SelectedItemIndex`
  （Crystal `PlayerObject.FinishQuest(int questIndex, int selectedItemIndex = -1)`，`:11350`），
  客户端在交任务时把选中的下标一起发上来，服务端按它发奖（`:11394-11420`）。
- **超时 = 放弃**：Crystal 在接取时排一个 `DelayedAction(DelayedType.Quest, …, TimeExpired, true)`，
  到点调 `AbandonQuest`（`QuestProgressInfo.cs:538-559`、`PlayerObject.cs:4364-4385`）。
  本作改成每 tick 比对 `expires_at`。
- **空目标任务天然可交**：`CheckCompleted` 在三张目标表都为空时直接返回 true
  （`QuestProgressInfo.cs:321-348`）。真实样本里有 55 个这种纯剧情任务。

---

## 2. 脚本格式（`Envir/Quests/<file>.txt`）

文件路径取自数据库头部 `QuestInfo.file_name`（相对 `Envir/Quests/`，可带子目录，
例如 `BichonProvince/BichonWall/1`），Crystal 用
`Path.Combine(Settings.QuestPath, FileName + ".txt")`（`QuestInfo.cs:136`；
`Settings.QuestPath = ./Envir/Quests`，`Server/Settings.cs:27`）。

### 2.1 段一览（Crystal 认的 13 个）

| 段 | 语义 | Crystal |
|---|---|---|
| `[@DESCRIPTION]` | 任务简介（多行正文） | `QuestInfo.cs:164`、`:204-206` |
| `[@TASKDESCRIPTION]` | 任务目标描述（多行正文） | `:165`、`:207-209` |
| `[@RETURNDESCRIPTION]` | 交任务时的描述（多行正文） | `:167`、`:210-212` |
| `[@COMPLETION]` | 完成后的描述（多行正文） | `:168`、`:213-215` |
| `[@CARRYITEMS]` | 接取时发给玩家的物品 | `:169`、`:216-219` |
| `[@KILLTASKS]` | 击杀目标 | `:170`、`:220-223` |
| `[@ITEMTASKS]` | 收集目标 | `:171`、`:224-227` |
| `[@FLAGTASKS]` | 标记位目标 | `:172`、`:228-231` |
| `[@FIXEDREWARDS]` | 必定发放的物品 | `:173`、`:232-236` |
| `[@SELECTREWARDS]` | 任选其一的物品 | `:174`、`:237-241` |
| `[@EXPREWARD]` | 奖励经验（一个整数） | `:175`、`:242-244` |
| `[@GOLDREWARD]` | 奖励金币（一个整数） | `:176`、`:245-247` |
| `[@CREDITREWARD]` | 奖励元宝（一个整数） | `:177`、`:248-250` |

> **`[@GOTO]` 段不存在**。任务书里提到它，但 Crystal 的段表（`QuestInfo.cs:178-183`）里没有，
> 「去某某处交任务」的文案来自数据库头部的 `GotoMessage`（`QuestInfo.cs:42`），
> 由 `QuestProgressInfo.UpdateGotoTask` 在任务完成后追加一行（`QuestProgressInfo.cs:527-532`）。
> 本作只认上表 13 个段，遇到 `[@GOTO]` 会记一条告警（不失败）。

### 2.2 数据行

| 行 | 解析 | Crystal |
|---|---|---|
| `怪物名 [数量] ["消息"]` | `QuestKillTask { monster, count, message }` | `ParseKill`，`QuestInfo.cs:284-302` |
| `物品名 [数量] ["消息"]` | `QuestItemTask { item, count, message }`（`[@ITEMTASKS]` / `[@CARRYITEMS]` 共用） | `ParseItem`，`:304-326` |
| `标记号 ["消息"]` | `QuestFlagTask { number, message }`；`0..=999` 之外丢弃 | `ParseFlag`，`:328-348` |
| `物品名 [数量]` | `QuestRewardLine { item, count }`（`[@FIXEDREWARDS]` / `[@SELECTREWARDS]` 共用） | `ParseReward`，`:259-282` |
| `500` 之类的整数 | `u32` 奖励值 | `:242-250` |

行内 `"…"` 是**任务列表里的提示文案**（Crystal 用正则 `"([^"]*)"` 取第一对双引号，`QuestInfo.cs:80`）。

`[@FLAGTASKS]` 的上限：Crystal 丢弃 `number < 0 || number > Globals.FlagIndexCount - 1000` 的行
（`:339`），`FlagIndexCount = 1999`（`Shared/Globals.cs:31`），所以合法区间是 `0..=999`
（本作常量 `mir_db::quest_script::MAX_FLAG_TASK_NUMBER`）。

### 2.3 注释与空行

- 空行：忽略（Crystal `QuestInfo.cs:200`）。
- `;注释` / `#注释`：任务书要求忽略；Crystal 只跳空行，本作**扩展**了这两种注释
  （真实样本里没出现过，见 `docs/world/quests.md` §9 的差异表）。
- `//注释`：段在这里终止（Crystal `innerLine.StartsWith("//")`，`:199`）。

### 2.4 解析规则里几个关键的细节

1. **段名大小写不敏感、顺序随意**：Crystal 对每个段头**扫全文件**（`:187-257`），
   所以 `[@KillTasks]` 与 `[@KILLTASKS]` 等价，段也可以出现在文件任何位置。
2. **同一个段出现多次会累加**：`ParseFile` 处理完一段不会 `break`，第二处同名段照样进列表。
3. **数值写坏 → 0**：`uint.TryParse(innerLine, out ExpReward)`（`:243`）失败时把值写成 0，
   本作照抄并记告警；重复的 `[@EXPREWARD]` 段后者覆盖前者。
4. **模板不存在的行被丢弃**：Crystal 在解析阶段就 `Envir.GetMonsterInfo(...) == null` 过滤
   （`:292`、`:312`、`:268`）。`mir-db` 没有模板表，所以只解析**名字**，
   由服务端在 `World::quest_definition` / 奖励发放时再查（查不到就跳过并 debug 日志）。
5. **切列宽容度**：Crystal 用 `line.Split(' ')`，行内连写两个空格会切出空字段
   （`TryParse("")` → 数量 0）。本作按空白切分并给「缺省 1」，见 §9。

---

## 3. 运行时状态

### 3.1 `Player` 上的两个字段

```rust
pub struct Player {
    /// 任务进度（Crystal `CharacterInfo.CurrentQuests`）。
    pub quests: Vec<QuestProgress>,
    /// 已完成的任务编号（Crystal `CharacterInfo.CompletedQuests`）。
    pub completed_quests: BTreeSet<i32>,
    // …
}
```

`QuestProgress`（`crates/mir-server/src/quests.rs`）：

| 字段 | 语义 | Crystal |
|---|---|---|
| `quest_index` | 任务编号 | `QuestProgressInfo.Index` |
| `state` | `NotStarted` / `InProgress` / `Completed`（交付后） | 用两个 `DateTime` 哨兵表达（`QuestProgressInfo.cs:30-38`） |
| `objectives_met` | 目标是否已全部达成（**可以交任务了**） | `QuestProgressInfo.Completed`（`EndDateTime` 被写） |
| `taken_at` | 接取时刻（Unix 毫秒） | `StartDateTime` |
| `expires_at` | 超时时刻（Unix 毫秒，`0` = 不限时） | `StartDateTime + TimeLimitInSeconds` |
| `kill_count` | `Vec<(归一化怪物名, 已击杀)>` | `KillTaskCount[i].Count` + `MonsterID` |
| `item_collected` | `Vec<(归一化物品名, 背包现有数)>` | `ItemTaskCount[i].Count` + `ItemID` |
| `flags` | 已点亮的标记号 | `FlagTaskSet[i].State` |

键用**归一化模板名**（小写 + 去空白，`mir_server::gamedata::normalize_item_name`）而不是
Crystal 的模板 ID：本作没有「怪物/物品模板的稳定数字 ID」这条约束，
名字归一化在中文/英文数据上都稳定（`Forest Yeti` / `forestyeti` 命中同一条）。

### 3.2 目标判定

- **击杀**：`monster_matches_task(task, monster)` = 两边归一化后
  `monster.starts_with(task)`——逐字对齐 Crystal 的
  `mInfo.Name.StartsWith(task.Info.Monster.Name, OrdinalIgnoreCase)`（`QuestProgressInfo.cs:373`、`:391`）。
- **收集**：模板名归一化后相等（Crystal 比的是模板引用，本作比名字）。
- **标记**：`flags.contains(number)`。
- 全部满足 = `is_objectives_complete(script, progress)`（Crystal `CheckCompleted`，`QuestProgressInfo.cs:321-362`）。

> **`objectives_met` ≠ `state == Completed`**：前者是「目标达成、可以回去交任务」
> （Crystal `Completed`，客户端 `ClientQuestProgress.Completed` 读它来显示问号图标），
> 后者要等真正交付才置位。每次推进（击杀/收集/接取/登录）都会重算 `objectives_met`
> 并随 `S.ChangeQuest{Update}` 下发。

### 3.3 时钟

任务时间戳用 `World::wall_now()` = **世界创建时刻的 Unix 毫秒 + 单调时钟**：

- 存档里存的是真实墙钟时间戳，重启后仍然有意义（Crystal 存 `DateTime`）；
- `World::advance_clock` 快进时两条时间线一起走，超时用例因此**确定性**。

---

## 4. 接口（API）

### 4.1 `mir-db`（`mir_db::quest_script`）

| 项 | 说明 |
|---|---|
| `QuestScript::load(file_name, envir_root)` | 读 `Envir/Quests/<file>.txt`（`QuestInfo.LoadInfo`，`QuestInfo.cs:130-146`） |
| `QuestScript::parse(file_name, &[String])` | 从内存行解析（`ParseFile`，`:161-257`） |
| `QuestScript` 字段 | `description` / `task_description` / `return_description` / `completion_description` / `carry_items` / `kill_tasks` / `item_tasks` / `flag_tasks` / `fixed_rewards` / `select_rewards` / `exp_reward` / `gold_reward` / `credit_reward` / `warnings` |
| `QuestScript::has_tasks()` | 是否有击杀/收集/标记目标 |
| `parse_kill_line` / `parse_item_line` / `parse_flag_line` / `parse_reward_line` | 单行解析（公开，便于测试与工具） |
| `resolve_script_path(file_name, envir_root)` | 拼 `Envir/Quests/…`（两种路径分隔符都吃） |
| `QuestKillTask` / `QuestItemTask` / `QuestFlagTask` / `QuestRewardLine` | 目标与奖励行 |
| `MAX_FLAG_TASK_NUMBER = 999` | 标记位上限（`Globals.FlagIndexCount - 1000`） |

`mir_db::npc_script::NpcScript` 新增：`quest_indexes()`（带符号的 `[Quests]` 原文）、
`publishes_quest(n)`、`finishes_quest(n)`，以及动作 `NpcActionKind::AcceptQuest` /
`FinishQuest` / `AbandonQuest`（本作扩展，见 §7）。

### 4.2 `mir-server`（`mir_server::quests`）

| 项 | 语义 / Crystal |
|---|---|
| `World::quest_definition(index)` | 头部 + 脚本；脚本缺失时用空脚本（Crystal `LoadInfo` 失败后同样是空的） |
| `World::client_quest_info(index)` | `ClientQuestInfo`（`QuestInfo.CreateClientQuestInfo`，`QuestInfo.cs:385-415`） |
| `World::quest_indexes()` / `quest_count()` | 全库任务编号 / 条数 |
| `World::quest_progress(player, index)` | 某个任务的进度 |
| `World::accept_quest(player, npc_object_id, index)` | 接取（`PlayerObject.AcceptQuest`，`:11251-11348`） |
| `World::finish_quest(player, index, selected_item_index)` | 交付（`:11350-11461`） |
| `World::abandon_quest(player, index)` | 放弃（`:11462-11471`） |
| `World::share_quest(player, index)` | 分享给同图队友（`:11472-11492`，见 `docs/world/group.md`） |
| `World::advance_quest_kills(player, monster_name)` | 击杀推进（`CheckNeedQuestKill`，`:11578-11590`） |
| `World::refresh_quest_items(player)` | 收集重算（`CheckNeedQuestItem`，`:11539-11561`）+ 变化时发 `S.ChangeQuest{Update}` |
| `World::sync_quest_items(player)` | 同上但**不发包**（登录/存档前用） |
| `World::inventory_item_counts(player)` | 背包按模板名聚合的数量表 |
| `World::tick_quests(now)` | 超时检查（`tick()` 内调用） |
| `World::quest_login_packets(player)` | 进世界时要发的任务包（`Vec<ServerPacket>`） |
| `World::send_completed_quests(player)` | 单独发一条 `S.CompleteQuest` |
| `World::gain_credit(player, amount)` | 加元宝（`GainCredit` + `S.GainedCredit`） |
| `World::take_inventory_items(player, item_name, count)` | 从背包扣物品（`TakeQuestItem`，`:7778-7803`） |
| `World::npc_gives_quest(&npc, index)` / `npc_finishes_quest(&npc, index)` | NPC 关联判定（`NpcInfo` 列表 ∪ 脚本 `[Quests]`） |
| `World::quest_publisher_npc(index)` / `quest_finisher_npc(index)` | 世界里的发布/交付 NPC |
| `monster_matches_task` / `item_matches_task` / `is_objectives_complete` | 目标匹配与完成判定 |
| `task_list(definition, progress)` | 进度文本（`QuestProgressInfo.UpdateTasks`，`:428-532`） |
| `encode_quests(&QuestSave)` / `decode_quests(&[u8])` | 存档编解码（见 §8） |
| `class_flag(MirClass)` | 职业 → `RequiredClass` 位（`Shared/Enums.cs:1068-1077`） |
| `MAX_CONCURRENT_QUESTS = 20` | `Globals.MaxConcurrentQuests`（`Shared/Globals.cs:33`） |
| `QUEST_NEW_WINDOW_MS` | 「新任务」窗口 24 小时（`QuestProgressInfo.cs:40-43`） |

---

## 5. 接取（`C.AcceptQuest`）

Crystal 的判定顺序（`PlayerObject.AcceptQuest`，`:11251-11348`），本作逐步照抄：

| # | 判定 | 本作的错误 |
|---|---|---|
| 1 | 已经接过同一个任务 → 返回 | `AlreadyTaken` |
| 2 | 任务不存在 → 返回（Crystal 会在 `info.NpcIndex` 上 NRE） | `NoSuchQuest` |
| 3 | 同图找 NPC（按对象 ID）+ 距离 `Globals.DataRange = 16` | `NoSuchNpc` / `OutOfRange` |
| 4 | 该 NPC 是不是这个任务的发布者 | `NotAQuestPublisher` |
| 5 | `info.CanAccept(this)`：等级区间 `RequiredMinLevel..RequiredMaxLevel` | `LevelOutOfRange` |
| 6 | 职业位掩码 `RequiredClass.HasFlag(职业)`（`None = 31` 表示全职业） | `WrongClass` |
| 7 | 同时进行数 `< Globals.MaxConcurrentQuests`（20） | `TooManyQuests` |
| 8 | 已完成过的任务不能再接（可重复任务除外） | `AlreadyCompleted` |
| 9 | 前置任务链（`RequiredQuest` 一路往上，最多 64 跳防环） | `PrerequisiteMissing` |
| 10 | 发放 `[@CARRYITEMS]`（本作进普通背包；背包放不下就整体失败） | `InventoryFull` |

成功后：建进度 → 按背包重算收集目标 → 发 `S.ChangeQuest { QuestState = Add, TrackQuest = true }`
（`:11345`；`trackQuest = true` 只在接取时为真，登录重放时是 `false`）。

**简化**：Crystal 还查 `npc.VisibleLog[Info.Index]` 与 `npc.Visible`（`:11269`）——
那是「客户端有没有看到这个 NPC 的任务图标」的缓存；本作只在 `call_npc` 里做可见性判定，
接取时不复查（否则 GM 命令/脚本动作会被状态机挡住）。

---

## 6. 交付（`C.FinishQuest`）

1. 交付前重算收集目标，取进度（没接 → `NotTaken`）；
2. 目标未达成 → `NotCompleted`；
3. 附近（同图 16 格内）要有能交付这个任务的 NPC：先找 `[Quests]` 里带 `-N` /
   `NpcInfo.FinishQuestIndexes` 的，找不到就退回发布者
   （Crystal `FinishNpcIndex` 默认指向 `NpcIndex`，`QuestInfo.cs:31-35`）→ 否则 `NoFinisherNearby`；
4. 组奖励清单：固定奖励 + 选中下标的可选奖励（`selected_item_index < 0` 表示不选）；
   背包放不下 → `InventoryFull`（**在任何副作用之前**判定）；
5. 按 Crystal 的顺序发包：

| # | 包 | Crystal |
|---|---|---|
| 1 | `S.CompleteQuest { CompletedQuests }` | `:11430-11431`（仅非可重复任务） |
| 2 | `S.ChangeQuest { QuestState = Remove }` | `:11434` |
| 3 | `S.DeleteItem` × 扣掉的携带物品/收集物品 | `:11436-11447`（Crystal 是 `S.DeleteQuestItem`） |
| 4 | `S.GainedItem` × 每件奖励 | `:11449-11452` |
| 5 | `S.GainedGold` | `:11456`（`GoldReward * Settings.DropRate`，默认 1.0） |
| 6 | `S.GainExperience` | `:11457`（`ExpReward * Settings.ExpRate`，默认 1.0） |
| 7 | `S.GainedCredit` | `:11458` |

6. 进度标记 `Completed`（可重复任务下一次接取时会覆盖这条记录），非可重复任务同时写入
   `completed_quests`。

---

## 7. NPC 脚本集成

### 7.1 `CHECKQUEST`

```text
CHECKQUEST <任务序号> ACTIVE|COMPLETE
```

- `ACTIVE` → `player.quests` 里存在 `state == InProgress` 的同号任务；
- **其它任何写法** → `player.completed_quests.contains(序号)`。

> ⚠️ 这是 Crystal 的真实语义（`NPCSegment.cs:2525-2534`）：`if (tempString == "ACTIVE") … else …`。
> 真实样本里 32 处全是数字写法（`CHECKQUEST 155 1`、`CHECKQUEST 135 0`），
> 按 Crystal 的逻辑它们问的都是「**完成了吗**」。本作照抄，见 §9 异常清单第 1 条。

### 7.2 三个任务动作（本作扩展）

| 动作 | 语义 |
|---|---|
| `ACCEPTQUEST <序号>` | 用**当前对话的 NPC** 走一遍 `accept_quest`（同样的前置校验） |
| `FINISHQUEST <序号>` | 走 `finish_quest(…, -1)`（脚本无法指定可选奖励下标） |
| `ABANDONQUEST <序号>` | 走 `abandon_quest` |

> Crystal 的 `NPCActions` 里**没有**这三个动作（全仓库搜 `ACCEPTQUEST` / `FINISHQUEST` /
> `ABANDONQUEST` 零命中）：任务接取/交付只能由客户端弹内置任务面板后发
> `C.AcceptQuest` / `C.FinishQuest`。加这三个动作是为了让「纯脚本驱动」的任务流程也能跑通
> （任务书的要求），真实 161 份任务脚本 + 659 份 NPC 脚本里一处也没用到——
> 测试 `real_database_quests_and_npc_links` 会断言这一点，防止以后把扩展误当成 Crystal 行为。

失败原因写进 `PageRun::quest_events` 并打 debug 日志，页面照常返回（不打断对话）。

### 7.3 `[Quests]` 段

```text
[Quests]
154        ← 正数：本 NPC 发布任务 154
-154       ← 负数：本 NPC 交付任务 154
```

解析在 `mir_db::npc_script::parse_quest_indexes`（Crystal `NPCScript.ParseQuests`，
`NPCScript.cs:685-721`）：非数字行被 `int.TryParse` 解成 0 后**静默跳过**
（样本里 `BichonProvince/BorderVillage/Pedlar.txt` 的 `[Quests]` 段里混进了一个 `<Close/@exit>` 按钮）。

---

## 8. 持久化

迁移：`crates/mir-store/migrations/0004_character_quests.sql`

```sql
ALTER TABLE characters ADD COLUMN quests BYTEA NOT NULL DEFAULT '\x'::bytea;
```

编码/解码：`mir_server::quests::encode_quests` / `decode_quests`（`QuestSave`）。

布局（全部小端，仿 Crystal 的 `CharacterInfo.Save` quest 段，`CharacterInfo.cs:488-492`、
`:514-518`；`QuestProgressInfo.Save`，`QuestProgressInfo.cs:291-318`，
但把「模板 ID」换成模板名）：

```text
i32    任务条数
每条：
  i32    quest_index（QuestInfo.Index）
  u8     state（0 = NotStarted / 1 = InProgress / 2 = Completed）
  u8     objectives_met（0/1，Crystal `Completed`：目标是否已达成）
  i64    taken_at（Unix 毫秒）
  i64    expires_at（Unix 毫秒，0 = 不限时）
  i32    kill_count 条数 + 每条 { string 归一化怪物名, i32 计数 }
  i32    item_collected 条数 + 每条 { string 归一化物品名, u16 计数 }
  i32    flags 条数 + 每条 { i32 标记号 }
i32    已完成任务条数 + 每条 i32
```

- 空块（新角色）→ 空存档；零长度 BYTEA 也解成空存档。
- 条数上限 `MAX_STORED_QUESTS = 256`（防损坏文件把内存吃满）。
- 状态字节非法 → `QuestCodecError::InvalidState`；会话层捕获后按「没有任务」处理并告警，
  不拒绝登录（与背包/装备/技能块同口径）。

读写时机：`Session::on_start_game` 解码 → `Player { quests, completed_quests }` →
`World::sync_quest_items`；`Session::leave_world` 编码回 `Character::quests` → `save_character`。

---

## 9. Crystal 的异常行为 / 本作的差异

| # | Crystal 的行为 | 本作 |
|---|---|---|
| 1 | `CHECKQUEST n <非 ACTIVE>` 一律按 `COMPLETE` 判（`NPCSegment.cs:2525-2534`），真实脚本用 `0/1` 写 | **照抄**；`docs/world/npc.md` 与 `npc.rs` 已标注 |
| 2 | `QuestInfo.NpcIndex/FinishNpcIndex` 不落盘，靠 NPC 脚本 `[Quests]` 回填；同一个任务被多个脚本引用时**后加载的覆盖前面的**（全局单例） | 改成「现查世界里的 NPC」：任一 NPC 的 `NpcInfo` 列表或脚本 `[Quests]` 命中即可 |
| 3 | `FinishNpcIndex == 0` 时默认等于 `NpcIndex`（`QuestInfo.cs:31-35`） | 照抄（附近没有专用交付 NPC 时回退到发布者） |
| 4 | `AcceptQuest` 对不存在的任务直接解引用 `info.NpcIndex` → `NullReferenceException`（`:11257-11263`） | 返回 `NoSuchQuest` |
| 5 | `ParseKill`/`ParseItem`/`ParseReward` 对模板不存在的行**静默丢弃**（`:292`、`:312`、`:268`） | 解析层保留名字，服务端查模板失败时跳过并 debug 日志 |
| 6 | `uint.TryParse` 写坏 → 值变 0；重复段**后者覆盖** | 照抄 + 告警 |
| 7 | `CheckCompleted` 在目标表全空时直接返回 true（55 个真实任务如此） | 照抄 |
| 8 | `Count >= Info.Count`：坏数据里数量 0 的目标天然算完成 | 照抄 |
| 9 | `ProcessKill` 只给**第一个**名字匹配的任务条目 `++`，而且不看它是否已完成 → 同一任务里两条同怪物目标时第二条永远涨不上去 | 改成「第一个**未完成**的匹配项」 |
| 10 | 奖励按堆叠上限拆分，`StackSize == 0` 时 `count -= 0` **死循环** | 用 `stack_size.max(1)` 兜底 |
| 11 | 任务物品走独立的 40 格 `QuestInventory`，怪物掉落会“截胡”（`MonsterObject.cs:1130-1138`），另有 `S.GainedQuestItem`/`S.DeleteQuestItem` | 走普通背包：收集进度 = 背包实时数量，交付时扣背包（见 §10） |
| 12 | 超时是接取时排的 `DelayedAction`（`QuestProgressInfo.cs:538-559`） | 每 tick 比对 `expires_at`（结果等价；`ignoreIfComplete = true` 也照抄：已完成的不会被清） |
| 13 | 登录时把**全库任务**的 `ClientQuestInfo` 推一遍（`:1185-1187`） | 照抄；但**没有任何任务数据时一个包都不发**（避免给空世界塞 `S.CompleteQuest{[]}`） |
| 14 | `ShareQuest` 需要组队，没队伍只回一条系统提示（`:11485-11491`） | 照抄：只发给同图 `DataRange` 内活着的队友，一个都没发出去时提示（`QuestError::NotShared`），见 `docs/world/group.md` |
| 15 | 奖励金币/经验乘 `Settings.DropRate` / `ExpRate`（默认 1.0） | 固定 1.0，直接按配置值发放 |
| 16 | 交付后把进度从 `CurrentQuests` **删掉**，只留 `CompletedQuests` | 保留一条 `state = Completed` 的记录（可查询/可存档），同时维护同语义的 `completed_quests`；可重复任务再接受时会覆盖它 |
| 17 | `CallDefaultNPC(OnAcceptQuest/OnFinishQuest)` 会跑默认 NPC 脚本（`:11347`、`:11460`） | 未实现（本作没有默认 NPC 脚本机制） |

---

## 10. 明确未做（TODO）

- **任务物品栏**（`QuestInventory` 40 格、`S.GainedQuestItem` / `S.DeleteQuestItem`、
  怪物掉落的“截胡”）：见 §9 第 11 条。
- **地图/区域类目标**（Crystal 的 `//TODO: ZoneTasks`，`QuestInfo.cs:71`）与护送任务
  （`//TODO: EscortTasks`，`:72`）——Crystal 自己也没做。
- **`CallDefaultNPC`**（`OnAcceptQuest` / `OnFinishQuest`）。
- **`Settings.DropRate` / `ExpRate` 的可配置化**（现在固定 1.0）。
- **NPC 任务图标的 `VisibleLog`**（`NPCObject.cs`）：接取时不复查可见性。
- **每日任务重置**（`QuestType.Daily` 的日切逻辑）：Crystal 侧也没有实现，只有图标区分。

---

## 11. 测试清单（`crates/mir-server/tests/quests.rs`，26 个用例）

| 用例 | 覆盖 |
|---|---|
| `real_quest_script_parses_into_tasks_and_rewards` | 真实脚本 `BichonProvince/BichonWall/1.txt` 解析（击杀 5 只 + 固定奖励 + 200/50） |
| `synthetic_script_parses_every_supported_section` | 合成脚本：携带物品 / 标记位 / 空目标 |
| `accept_quest_records_progress_and_sends_change_quest` | 接取成功、`S.ChangeQuest{Add, track=true}`、进度初值 |
| `accept_rejects_level_class_and_prerequisite` | 等级上限 / 职业位掩码 / 前置任务链（含「完成前置后放行」） |
| `accept_rejects_the_wrong_npc` | 无关 NPC（旁观者）、脚本没关联该任务（工匠）、距离太远 |
| `accept_rejects_duplicates_and_completed_quests` | 重复接取 / 完成后再接 |
| `npc_script_quest_list_links_the_craft_lady` | NPC 脚本 `[Quests]` 关联（数据库列表为空时） |
| `kill_progress_completes_the_quest` | 大小写/空格归一化、计数、`S.ChangeQuest{Update}`、`S.SendOutputMessage`、进度文本 |
| `monster_death_advances_the_killer_quest` | **真实战斗链路**：攻击 → 300ms 延迟伤害 → 怪物死亡 → 击杀计数 |
| `item_progress_counts_the_inventory` | 背包数量 → 收集目标 |
| `finish_quest_pays_gold_exp_and_fixed_items` | 金币/经验/固定物品 + **包顺序**（CompleteQuest → Remove → Item → Gold → Exp） |
| `finish_uses_the_selected_optional_reward` | 可选奖励下标 0 / 1 / 越界 / `-1` |
| `finish_requires_the_finisher_npc_nearby` | 走开后无法交付 |
| `finish_rejects_unfinished_quests` | 未达成 / 未接取时的交付与放弃 |
| `timed_quest_expires_and_is_abandoned` | 限时 10 秒：9 秒不触发、11 秒清进度 + `S.ChangeQuest{Remove}` |
| `abandon_quest_clears_the_progress` | 放弃清进度 |
| `carry_items_are_given_on_accept_and_taken_on_finish` | `[@CARRYITEMS]` 发放/收回 + 标记位目标 |
| `npc_checkquest_reads_the_real_state` | `CHECKQUEST … ACTIVE` 三分支 + 数字写法的 `COMPLETE` 怪癖 |
| `npc_finishquest_action_completes_the_quest` | `ACCEPTQUEST` / `FINISHQUEST` / `ABANDONQUEST` 三个动作 |
| `quest_progress_round_trips_through_the_byte_block` | 存档往返（含超时时刻、完成列表） |
| `login_packets_carry_the_quest_catalogue` | 进世界下发：`S.NewQuestInfo` × 全库 + `S.CompleteQuest` + `S.ChangeQuest{Add}`；空世界不发 |
| `npc_object_packet_lists_script_quests` | `S.ObjectNPC.QuestIDs` 合并脚本 `[Quests]` 的发布任务 |
| `real_database_quests_and_npc_links` | **真实数据冒烟**：任务 > 100、脚本 ≥ 90% 可解析、NPC 关联来源、动作统计 |
| `real_quest_can_be_completed_end_to_end` | **真实任务完整跑通**：铁匠 → 接 5 号 → 杀 10+10 → 交付（30 金币 / 120 经验 / 铁手镯） |
| `real_luke_script_gates_the_dialog_on_quest_completion` | 真实 NPC 脚本 `Luke.txt` 的 `CHECKQUEST 154 COMPLETE` 门禁 |
| `harness_helpers_are_wired` | 脚手架自检（等级/对话包） |

`mir-db` 侧另有：`quest_script.rs` 10 个单元测试（13 个段、段名大小写、注释、重复段、
坏行告警、未知段、双引号消息、数量缺省/写坏、标记位边界、路径拼接）、
`npc_script.rs` 2 个 `[Quests]` / 任务动作用例；
`mir-server/src/quests.rs` 5 个单元测试（存档往返、空块、名字匹配、
目标完成判定、状态值往返）。

### 真实数据冒烟结果

样本：`resource/community/suprcode-crystal-database/Jev/Server.MirDB`（v88）+ 同目录 `Envir/`。

| 指标 | 数值 |
|---|---|
| `GameData.db.quests`（任务头部） | **154**（要求 > 100 ✅） |
| `Envir/Quests` 里的 `.txt` | 161（其中 154 份被任务头部引用） |
| 被引用的脚本解析成功 | **154 / 154 = 100%**（要求 ≥ 90% ✅） |
| `Envir/Quests` 全部 `.txt` 解析成功 | **161 / 161 = 100%**（含没有被任务头部引用的 7 份） |
| 带 `[@FLAGTASKS]` 非空的脚本 | 10 |
| 带 `[@ITEMTASKS]` 非空的脚本 | 53 |
| 带 `[@SELECTREWARDS]` 非空的脚本 | 17 |
| 带 `[@CARRYITEMS]` 非空的脚本 | 18 |
| 带 `[@CREDITREWARD]` 的脚本 | 1（`BichonProvince/BichonWall/26.txt` = 150） |
| 空目标任务（纯剧情） | 55 |
| 数据库 NPC 的 `CollectQuestIndexes`/`FinishQuestIndexes` | **全部为空**（0/375） |
| NPC 脚本带 `[Quests]` 段的（被 DB 引用的 375 份里） | 81（`Envir/NPCs` 的 659 份文件里是 178 份） |
| 任务 ↔ NPC 关联条数 | 360（每个任务通常 2 条：`+N` 发布 + `-N` 交付） |
| NPC 脚本里的 `CHECKQUEST` | 659 份文件里 32 行；被 DB 引用的脚本解析后 15 处 |
| `ACCEPTQUEST` / `FINISHQUEST` / `ABANDONQUEST` | **0 处**（Crystal 没有这些动作） |

完整的真实任务跑通例子：任务 5 `The Smith's 1st Test`（`BichonProvince/BorderVillage/5`）——
铁匠 `Blacksmith_Smith`（脚本 `[Quests]` 里 `5` 与 `-5`）发布+交付，
目标「杀 10 只 Deer + 10 只 Scarecrow」，奖励 `WornIronBracelet` ×1、30 金币、120 经验。
