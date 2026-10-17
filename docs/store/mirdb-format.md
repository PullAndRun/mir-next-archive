# `Server.MirDB` 二进制容器格式（`mir-tools` 实现规格）

> 参考实现：`C:\Users\diana\ref\Crystal`（相对路径均以该目录为根）。
> 读取代码：`Server/MirEnvir/Envir.cs:2821-2942`（`LoadDB`）；写入代码：`Server/MirEnvir/Envir.cs:2439-2507`（`SaveDB`）。
> 被测样本：`resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB`，452,106 字节，`Version = 105`。
> **本文档的字段布局已用代码推导的解析器对该样本做完整消费验证：解析 452,106 字节，`leftover = 0`（见 §8）。**

---

## 0. 一句话结论

`Server.MirDB` 是**无 magic、无压缩、无校验和、无索引**的裸 `BinaryWriter` 顺序流：10 个小端 `int32` 头部计数 + 10 个顺序记录段。想读它只需要一张字段顺序表；难的不是解码，而是**版本门（version gating）**和几个「写入从不读取」的尾部字段。

它**不包含任何账号或角色数据**（账号在 `Server.MirADB`，见 §9）。

---

## 1. 文件头（固定 40 字节 = 10 × `int32`）

| 偏移 | 类型 | 字段 | 说明 |
|---|---|---|---|
| 0 | `int32` | `Version` | 写入时的 `Envir.Version`；语义见 §6 |
| 4 | `int32` | `CustomVersion` | 写入时的 `Envir.CustomVersion`（当前恒为 0） |
| 8 | `int32` | `MapIndex` | 地图 ID 计数器（最后分配值） |
| 12 | `int32` | `ItemIndex` | 物品模板 ID 计数器 |
| 16 | `int32` | `MonsterIndex` | 怪物 ID 计数器 |
| 20 | `int32` | `NPCIndex` | NPC ID 计数器 |
| 24 | `int32` | `QuestIndex` | 任务 ID 计数器 |
| 28 | `int32` | `GameshopIndex` | 商城条目 ID 计数器（仅 `Version ≥ 63` 时存在） |
| 32 | `int32` | `ConquestIndex` | 攻城区域 ID 计数器（仅 `Version ≥ 66`） |
| 36 | `int32` | `RespawnIndex` | 刷怪点 ID 计数器（仅 `Version ≥ 68`） |

写入顺序见 `Envir.cs:2457-2466`，读取顺序见 `Envir.cs:2847-2865`。注意 `Envir` 里还有一个 `ScriptIndex`（`Envir.cs:117`），它**不落盘**。

> 计数器语义是「下一个可用值」还是「最后使用值」在代码里并不严格一致：样本中 `ItemIndex = 1348` 但实际存在 `Index = 1349` 的物品（实测，见 §8.4）。迁移时把它当**不透明序列值**继续用，不要用它推断记录数。

---

## 2. 段结构总表

固定头之后是 10 个段，**顺序固定、无段头、无长度字段、无填充**——必须逐条记录顺序读完全部内容才能到达下一段。列表段都是 `int32 count` + `count` 条记录。

| # | 段 | 起始处 | 记录类型 | 版本条件 |
|---|---|---|---|---|
| 1 | 地图 | `int32 count` | `MapInfo` | 无条件 |
| 2 | 物品模板 | `int32 count` | `ItemInfo` | 无条件 |
| 3 | 怪物 | `int32 count` | `MonsterInfo` | 无条件 |
| 4 | NPC | `int32 count` | `NPCInfo` | 无条件 |
| 5 | 任务 | `int32 count` | `QuestInfo`（**只有头部**） | 无条件 |
| 6 | 龙系统 | 无 count，单条 | `DragonInfo` | 无条件 |
| 7 | 技能 | `int32 count` | `MagicInfo` | 无条件 |
| 8 | 商城 | `int32 count` | `GameShopItem` | `≥ 63` |
| 9 | 攻城 | `int32 count` | `ConquestInfo` | `≥ 66` |
| 10 | 刷怪计时器 | 无 count，单条 | `RespawnTimer` | `> 67` |
| 11 | GT 城列表 | `int32 count` | `GTMap` | **写但不读**（§5） |

写入顺序：`Envir.cs:2468-2505`；读取顺序：`Envir.cs:2868-2937`。

---

## 3. 通用编码

| 项 | 规则 | 代码证据 |
|---|---|---|
| 字节序 | 小端（.NET `BinaryWriter` 默认） | — |
| 字符串 | **7-bit 变长长度（LEB128）+ UTF-8 字节**，无 NUL、无 BOM；空串 = `0x00` | `BinaryWriter.Write(string)` |
| 布尔 | 1 字节，`0`/`1` | — |
| 日期 | `int64` = `DateTime.ToBinary()`（ticks + Kind 位），**不是** Unix 时间 | `Envir.cs` 各处 `DateTime.FromBinary` |
| 枚举 | 显式强转成 `byte`/`short`/`int` 后写 | 例 `writer.Write((byte)Type)` |
| `Stats` | `int32 count` + `count × (u8 statId, i32 value)`，只存非 0 项 | `Shared/Data/Stat.cs:36-59` |
| `Point` | `int32 X` + `int32 Y` | 例 `MapInfo` 无 Point，`NPCInfo` 用两 int |
| 列表 | `int32 count` + 元素 | — |
| 槽位数组 | `int32 count` + 每槽 `bool` + 元素 | 见 §4.2（**两种相反语义**） |

`Stat` 的 id 是稀疏枚举（0-9、10-23、30-36、40-48、100-108、120-129），完整表在 `Shared/Data/Stat.cs:85-152`。样本中单条 `ItemInfo` 的 `Stats` 最多 12 项（实测），键如 `4 (MinDC)`、`5 (MaxDC)`、`10 (Accuracy)`、`14 (AttackSpeed)`。

---

## 4. 逐段字段布局

> 每张表的「条件」列就是 `LoadVersion` 门；`Save()` 一律按**最新**布局写，`Load()` 按版本读。因此：**旧文件缺尾部字段是合法的，新文件不缺。**

### 4.1 `MapInfo`（`Server/MirDatabase/MapInfo.cs:47-124` 读 / `:126-194` 写）

| # | 字段 | 类型 | 条件 | 说明 |
|---|---|---|---|---|
| 1 | `Index` | `i32` | — | 主键 |
| 2 | `FileName` | `string` | — | 地图文件名（如 `"0"`） |
| 3 | `Title` | `string` | — | 显示名（如 `比奇省`） |
| 4 | `MiniMap` | `u16` | — | 小地图编号 |
| 5 | `Light` | `u8` | — | `LightSetting` |
| 6 | `BigMap` | `u16` | — | 大地图编号 |
| 7 | `SafeZones` | `i32 count` + `SafeZoneInfo[]` | — | 安全区 |
| 8 | `Respawns` | `i32 count` + `RespawnInfo[]` | — | 刷怪点 |
| 9 | `Movements` | `i32 count` + `MovementInfo[]` | — | 传送点 |
| 10 | `NoTeleport`,`NoReconnect` | `bool ×2` | — | 禁制标志 |
| 11 | `NoReconnectMap` | `string` | — | |
| 12 | `NoRandom`,`NoEscape`,`NoRecall`,`NoDrug`,`NoPosition`,`NoThrowItem`,`NoDropPlayer`,`NoDropMonster`,`NoNames`,`Fight`,`Fire` | `bool ×11` | — | 顺序敏感 |
| 13 | `FireDamage` | `i32` | — | |
| 14 | `Lightning` | `bool` | — | |
| 15 | `LightningDamage` | `i32` | — | |
| 16 | `MapDarkLight` | `u8` | — | |
| 17 | `MineZones` | `i32 count` + `MineZone[]` | — | 矿区 |
| 18 | `MineIndex` | `u8` | — | |
| 19 | `NoMount`,`NeedBridle`,`NoFight` | `bool ×3` | — | |
| 20 | `Music` | `u16` | — | |
| 21 | `NoTownTeleport` | `bool` | `≥ 78` | |
| 22 | `NoReincarnation` | `bool` | `≥ 79` | |
| 23 | `WeatherParticles` | `u16` | `≥ 110` | |
| 24 | `GT` / `GTIndex` | `bool` / `u8` | `≥ 111` | |
| 25 | `NoExperience`,`NoGroup`,`NoPets`,`NoIntelligentCreatures`,`NoHero` | `bool ×5` | `≥ 114` | |
| 26 | `RequiredGroupSize` | `i32` | `≥ 114` | |
| 27 | `RequiredGroup`,`FireWallLimit` | `bool ×2` | `≥ 114` | |
| 28 | `FireWallCount` | `i32` | `≥ 114` | |

子记录：

| 子类型 | 布局 | 出处 |
|---|---|---|
| `SafeZoneInfo` | `i32 X`, `i32 Y`, `u16 Size`, `bool StartPoint` = **11 字节** | `Server/MirDatabase/SafeZoneInfo.cs:14-27` |
| `RespawnInfo` | `i32 MonsterIndex`, `i32 X`, `i32 Y`, `u16 Count`, `u16 Spread`, `u16 Delay`, `u8 Direction`, `string RoutePath`, 且当 `> 67` 时再跟 `u16 RandomDelay`, `i32 RespawnIndex`, `bool SaveRespawnTime`, `u16 RespawnTicks` | `Server/MirDatabase/RespawnInfo.cs:27-100` |
| `MovementInfo` | `i32 MapIndex`, `i32 SrcX`, `i32 SrcY`, `i32 DstX`, `i32 DstY`, `bool NeedHole`, `bool NeedMove`, `≥69: i32 ConquestIndex`, `≥95: bool ShowOnBigMap`, `i32 Icon` | `Server/MirDatabase/MovementInfo.cs:18-46` |
| `MineZone` | `i32 X`, `i32 Y`, `u16 Size`, `u8 Mine` | `Server/MirDatabase/MineInfo.cs:82-95` |

**不落盘**的 `MapInfo` 成员：`NPCs`（读档后按 `NPCInfo.MapIndex` 重建，`MapInfo.cs:199-204`）、`ActiveCoords`。

### 4.2 `ItemInfo`（`Shared/Data/ItemData.cs:69-204` 读 / `:208-258` 写）

| # | 字段 | 类型 | 条件 | 说明 |
|---|---|---|---|---|
| 1 | `Index` | `i32` | — | 主键 |
| 2 | `Name` | `string` | — | 模板名 |
| 3 | `Type` | `u8` | — | `ItemType` |
| 4 | `Grade` | `u8` | — | `ItemGrade` |
| 5 | `RequiredType` | `u8` | — | |
| 6 | `RequiredClass` | `u8` | — | 位掩码 |
| 7 | `RequiredGender` | `u8` | — | |
| 8 | `Set` | `u8` | — | `ItemSet` |
| 9 | `Shape` | **`i16`** | — | 注意是**有符号** |
| 10 | `Weight`,`Light`,`RequiredAmount` | `u8 ×3` | — | |
| 11 | `Image`,`Durability` | `u16 ×2` | — | |
| 12 | `StackSize` | `u16` | `> 84`（`≤84` 为 `u32`） | |
| 13 | `Price` | `u32` | — | |
| 14 | （旧版固定属性块） | 15 项 | `≤ 84` | `MinAC…MaxSC` u8/u16、`Luck/AttackSpeed` **`sbyte`** |
| 15 | `StartItem` | `bool` | — | |
| 16 | （旧版权重/效果） | 3+1 字节 | `≤ 84` | `BagWeight/HandWeight/WearWeight` 后接 `Effect` |
| 17 | `Effect` | `u8` | — | |
| 18 | （旧版属性块 2） | 10 项 | `≤ 84` | `Strong…CriticalDamage` |
| 19 | `bools` | `u8` 位域 | — | bit0 `NeedIdentify`、bit1 `ShowGroupPickup`、bit2 `ClassBased`、bit3 `LevelBased`、bit4 `CanMine`、bit5 `GlobalDropNotify`（`≥77`） |
| 20 | （旧版属性块 3） | 5 项 | `≤ 84` | `MaxACRatePercent…PoisonAttack` |
| 21 | `Bind` | `i16` | — | `BindMode` |
| 22 | （旧版属性块 4） | 2 项 | `≤ 84` | `Reflect/HPDrainRatePercent` |
| 23 | `Unique` | `i16` | — | `SpecialItemMode` |
| 24 | `RandomStatsId` | `u8` | — | 指向 `Settings.RandomItemStatsList` |
| 25 | `CanFastRun` | `bool` | — | |
| 26 | `CanAwakening` | `bool` | — | |
| 27 | `Slots` | `u8` | `> 83` | 子槽数量 |
| 28 | `Stats` | `Stats` | `> 84`（`≤84` 用固定块） | 属性字典 |
| 29 | `ToolTip` | `bool` + `string` | — | 先写「非 null」标志；默认 `""` 所以**几乎总是 true + 空串** |

实测：样本 1349 条模板**全部**带 `tooltip`（值 `""`），`Stats` 用字典格式，说明文件由 `Version ≥ 85` 的版本写出，与头部 105 一致。

### 4.3 `MonsterInfo`（`Server/MirDatabase/MonsterInfo.cs:53-148` 读 / `:196-226` 写）

| # | 字段 | 类型 | 条件 |
|---|---|---|---|
| 1 | `Index` | `i32` | — |
| 2 | `Name` | `string` | — |
| 3 | `Image` | `u16` | — |
| 4 | `AI`,`Effect` | `u8 ×2` | — |
| 5 | `Level` | `u16` | `≥ 62`（`<62` 为 `u8`） |
| 6 | `ViewRange`,`CoolEye` | `u8 ×2` | — |
| 7 | `Stats` | `Stats` | `> 84`（`≤84` 为 HP u32 + 10×u16 + Accuracy/Agility u8） |
| 8 | `Light` | `u8` | — |
| 9 | `AttackSpeed`,`MoveSpeed` | `u16 ×2` | — |
| 10 | `Experience` | `u32` | — |
| 11 | `CanPush`,`CanTame` | `bool ×2` | — |
| 12 | `AutoRev`,`Undead` | `bool ×2` | `≥ 18` |
| 13 | `DropPath` | `string` | `≥ 89` |
| 14 | `CanRecall` | `bool` | `≥ 115` |
| 15 | `IsBoss` | `bool` | `≥ 116` |

`Drops` 列表**不落盘**：掉落来自 `Envir/Drops/*.txt`（`MonsterInfo.cs:372-435`；`DropInfo` 只存在于文本解析结果）。

### 4.4 `NPCInfo`（`Server/MirDatabase/NPCInfo.cs:44-103` 读 / `:104-141` 写）

| # | 字段 | 类型 | 条件 |
|---|---|---|---|
| 1 | `Index` | `i32` | — |
| 2 | `MapIndex` | `i32` | — |
| 3 | `CollectQuestIndexes` | `i32 count` + `i32[]` | — |
| 4 | `FinishQuestIndexes` | `i32 count` + `i32[]` | — |
| 5 | `FileName`,`Name` | `string ×2` | — |
| 6 | `Location` | `i32 X`,`i32 Y` | — |
| 7 | `Image` | `u16` | `≥ 72`（`<72` 为 `u8`） |
| 8 | `Rate` | `u16` | — |
| 9 | `TimeVisible` | `bool` | `≥ 64` |
| 10 | `HourStart`,`MinuteStart`,`HourEnd`,`MinuteEnd` | `u8 ×4` | `≥ 64` |
| 11 | `MinLev`,`MaxLev` | `i16 ×2` | `≥ 64` |
| 12 | `DayofWeek`,`ClassRequired` | `string ×2` | `≥ 64` |
| 13 | `Conquest` | `i32` | `≥ 66`（`<66` 为 `bool Sabuk`） |
| 14 | `FlagNeeded` | `i32` | `≥ 64` |
| 15 | `ShowOnBigMap` | `bool` | `> 95` |
| 16 | `BigMapIcon` | `i32` | `> 95` |
| 17 | `CanTeleportTo` | `bool` | `> 96` |
| 18 | `ConquestVisible` | `bool` | `≥ 107` |

### 4.5 `QuestInfo`（`Server/MirDatabase/QuestInfo.cs:85-110` 读 / `:112-128` 写）

**DB 里只有头部，没有任务正文/奖励**——构造时调用 `LoadInfo()` 从 `Envir/Quests/{FileName}.txt` 解析（`QuestInfo.cs:109, 130-146`），奖励与任务目标全是文本文件里的 `[@KILLTASKS]` 等小节。

| # | 字段 | 类型 | 条件 |
|---|---|---|---|
| 1 | `Index` | `i32` | — |
| 2 | `Name`,`Group`,`FileName` | `string ×3` | — |
| 3 | `RequiredMinLevel`,`RequiredMaxLevel`,`RequiredQuest` | `i32 ×3` | — |
| 4 | `RequiredClass` | `u8` | — |
| 5 | `Type` | `u8` | — |
| 6 | `GotoMessage`,`KillMessage`,`ItemMessage`,`FlagMessage` | `string ×4` | — |
| 7 | `TimeLimitInSeconds` | `i32` | `> 90` |

> 实测：样本 157 条任务记录总长仅 13,925 字节（平均 89 字节/条），与其「只有头部」一致。

### 4.6 `DragonInfo`（`Server/MirDatabase/DragonInfo.cs:49-69` 读 / `:70-86` 写）

| # | 字段 | 类型 |
|---|---|---|
| 1 | `Enabled` | `bool` |
| 2 | `MapFileName`,`MonsterName`,`BodyName` | `string ×3` |
| 3 | `Location` | `i32 X`,`i32 Y` |
| 4 | `DropAreaTop` | `i32 X`,`i32 Y` |
| 5 | `DropAreaBottom` | `i32 X`,`i32 Y` |
| 6 | `Exps` | **12 × `i64`**（`Globals.MaxDragonLevel - 1`，`Shared/Globals.cs:27` 中 `MaxDragonLevel = 13`） |

无版本门；`Drops` 来自 `Envir/Drops/DragonItem.txt`（`DragonInfo.cs:88-143`）。固定长度 = 1 + 3 串 + 6×4 + 12×8。

### 4.7 `MagicInfo`（`Server/MirDatabase/MagicInfo.cs:34-61` 读 / `:63-85` 写）

| # | 字段 | 类型 | 条件 |
|---|---|---|---|
| 1 | `Name` | `string` | — |
| 2 | `Spell` | `u8` | — |
| 3 | `BaseCost`,`LevelCost`,`Icon` | `u8 ×3` | — |
| 4 | `Level1`,`Level2`,`Level3` | `u8 ×3` | — |
| 5 | `Need1`,`Need2`,`Need3` | `u16 ×3` | — |
| 6 | `DelayBase`,`DelayReduction` | `u32 ×2` | — |
| 7 | `PowerBase`,`PowerBonus`,`MPowerBase`,`MPowerBonus` | `u16 ×4` | — |
| 8 | `Range` | `u8` | `> 66` |
| 9 | `MultiplierBase`,`MultiplierBonus` | **`f32 ×2`** | `> 70` |

### 4.8 `GameShopItem`（`Shared/Data/ItemData.cs:800-827` 读 / `:848-865` 写）

| # | 字段 | 类型 | 条件 |
|---|---|---|---|
| 1 | `ItemIndex` | `i32` | — |
| 2 | `GIndex` | `i32` | — |
| 3 | `GoldPrice`,`CreditPrice` | `u32 ×2` | — |
| 4 | `Count` | `u16` | `> 84`（`≤84` 为 `u32`） |
| 5 | `Class`,`Category` | `string ×2` | — |
| 6 | `Stock` | `i32` | — |
| 7 | `iStock`,`Deal`,`TopItem` | `bool ×3` | — |
| 8 | `Date` | `i64`（`ToBinary`） | — |
| 9 | `CanBuyCredit`,`CanBuyGold` | `bool ×2` | **`> 105`** |

`Info`（`ItemInfo`）不落盘，只存 `ItemIndex` 引用。实测样本 `Version = 105` → 无第 9 项（符合）。

### 4.9 `ConquestInfo`（`Server/MirDatabase/ConquestInfo.cs:53-141` 读 / `:143-218` 写）

| # | 字段 | 类型 | 条件 |
|---|---|---|---|
| 1 | `Index` | `i32` | — |
| 2 | `FullMap` | `bool` | `> 73` |
| 3 | `Location` | `i32 X`,`i32 Y` | — |
| 4 | `Size` | `u16` | — |
| 5 | `Name` | `string` | — |
| 6 | `MapIndex`,`PalaceIndex` | `i32 ×2` | — |
| 7 | `GuardIndex`,`GateIndex`,`WallIndex`,`SiegeIndex` | `i32 ×4` | — |
| 8 | `FlagIndex` | `i32` | `> 72` |
| 9 | `ConquestGuards` | `i32 count` + `ConquestArcherInfo[]` | — |
| 10 | `ExtraMaps` | `i32 count` + `i32[]` | — |
| 11 | `ConquestGates` | `i32 count` + `ConquestGateInfo[]` | — |
| 12 | `ConquestWalls` | `i32 count` + `ConquestWallInfo[]` | — |
| 13 | `ConquestSieges` | `i32 count` + `ConquestSiegeInfo[]` | — |
| 14 | `ConquestFlags` | `i32 count` + `ConquestFlagInfo[]` | `> 72` |
| 15 | `StartHour` | `u8` | — |
| 16 | `WarLength` | `i32` | — |
| 17 | `Type`,`Game` | `u8 ×2` | — |
| 18 | `Monday…Sunday` | `bool ×7` | — |
| 19 | `KingLocation` | `i32 X`,`i32 Y` | — |
| 20 | `KingSize` | `u16` | — |
| 21 | `ControlPointIndex` | `i32` | `> 74` |
| 22 | `ControlPoints` | `i32 count` + `ConquestFlagInfo[]` | `> 74` |

子记录（注意 `RepairCost` 的类型随版本变化）：

| 子类型 | 布局 | 出处 |
|---|---|---|
| `ConquestArcherInfo` | `i32 Index`, `i32 X`, `i32 Y`, `i32 MobIndex`, `string Name`, `u32 RepairCost` | `ConquestInfo.cs:365-382` |
| `ConquestGateInfo` / `ConquestWallInfo` / `ConquestSiegeInfo` | `i32 Index`, `i32 X`, `i32 Y`, `i32 MobIndex`, `string Name`, `i32 RepairCost`（`≤84` 时按 `u32` 读） | `ConquestInfo.cs:236-261, 279-304, 322-347` |
| `ConquestFlagInfo` | `i32 Index`, `i32 X`, `i32 Y`, `string Name`, `string FileName` | `ConquestInfo.cs:399-414` |

### 4.10 `RespawnTimer`（`Server/MirEnvir/RespawnTimer.cs:72-93`）

| # | 字段 | 类型 | 条件 |
|---|---|---|---|
| 1 | `BaseSpawnRate` | `u8` | `> 67` |
| 2 | `CurrentTickcounter` | `u64` | `> 67` |
| 3 | `Respawn` 选项 | `i32 count` + (`i32 UserCount`, **`f64 DelayLoss`**) | `> 67` |

`LastTick`、`LastUsercount`、`CurrentDelay` 都是运行期派生值，不落盘（`RespawnTimer.cs:58-63`）。
样本：`{base_spawn_rate: 20, current_tick_counter: 71, options: [(user_count=0, delay_loss=1.0)]}`，共 **25 字节**（1+8+4+(4+8)），正好占满文件尾。

---

## 5. `GTMap`：**写进去但永远不读**的尾部段

`SaveDB` 最后无条件写：

```csharp
// Envir.cs:2503-2505
writer.Write(GTMapList.Count);
for (var i = 0; i < GTMapList.Count; i++)
    GTMapList[i].Save(writer);
```

而 `LoadDB` 在读完全部段之后**直接结束**（`Envir.cs:2935-2937` 之后没有任何读取）。因此：

- 由 117 版写出的文件，在 `RespawnTimer` 之后还会有一段 `i32 count + GTMap[count]`，当前服务端**忽略**它（等于每次重启丢失 GT 城租借信息）。
- 由 105 版写出的样本**没有**这段（实测 `RespawnTimer` 结束后恰好到文件尾，`leftover = 0`）——因为 105 时该功能还不存在。

`GTMap` 布局（`Server/MirDatabase/GTMap.cs:27-51`）：
`i32 Index`, `i32 Key`, `string Name`, `string Owner`, `string Leader`, `string Leader2`, `i32 Price`, `i32 Days`, `i32 Begin`。
（注意：`GTMap.Maps` 字段不在落盘范围。）

**给 `mir-tools` 的建议**：解析完第 10 段后，若 `remaining >= 4`，按 `i32 count + GTMap[count]` 尝试解析；成功则该段是 GT 城，失败则视为未知尾部数据并报错。不要因为它的存在与否而判定文件损坏。

---

## 6. 版本语义与不匹配时的行为

### 6.1 常量与版本门

| 常量 | 值 | 出处 |
|---|---|---|
| `Envir.Version` | **117** | `Envir.cs:57` |
| `Envir.MinVersion` | **60** | `Envir.cs:56` |
| `Envir.CustomVersion` | **0** | `Envir.cs:58` |

- `Version` 是**写入**时的格式版本；`LoadVersion` 是**读取**时的文件版本（`Envir.cs:69-70`）。
- `CustomVersion` 会一路传进各记录构造函数（如 `new ItemInfo(reader, LoadVersion, LoadCustomVersion)`，`Envir.cs:2877`），但在参考源里**没有任何分支使用它**——可以安全忽略。
- 全量版本门（按值）：

| 门 | 影响 |
|---|---|
| `≥ 63` | `GameshopIndex` 头字段、商城段（`GameShopItem`）；角色 `GSpurchases` |
| `≥ 66` | `ConquestIndex` 头字段 + 攻城段；`NPCInfo.Conquest` 替代 `Sabuk` |
| `≥ 68` | `RespawnIndex` 头字段 |
| `> 67` | `RespawnInfo` 的 `RandomDelay/RespawnIndex/SaveRespawnTime/RespawnTicks`；`RespawnTimer` 段 |
| `≥ 62` / `< 62` | 怪物/角色 `Level` 的 u16 / u8 |
| `≤ 70` | 载入后执行 `UpdateMagicInfo()`（`Envir.cs:2908-2909`） |
| `> 66` / `> 70` | `MagicInfo.Range` / `MultiplierBase,Bonus` |
| `> 72` / `> 73` / `> 74` | 攻城的 `FlagIndex`+旗子列表 / `FullMap` / 控制点 |
| `> 75` | 账号 `HasExpandedStorage` + 到期时间 |
| `> 81` | 角色 `LastLoginDate` |
| `> 90` | 任务 `TimeLimitInSeconds` |
| `> 95` / `> 96` / `≥ 107` | NPC `ShowOnBigMap`+`BigMapIcon` / `CanTeleportTo` / `ConquestVisible` |
| `≥ 110` / `≥ 111` / `≥ 114` | 地图 `WeatherParticles` / `GT,GTIndex` / 一组禁制字段 |
| `≥ 115` / `≥ 116` | 怪物 `CanRecall` / `IsBoss` |
| `> 105` | 商城 `CanBuyCredit,CanBuyGold` |
| `≤ 84` 整体 | 旧的「固定字段」编码（`Stats` 不是字典、`Count` 是 u32 等） |

### 6.2 版本不匹配时

```csharp
// Envir.cs:2833-2845
LoadVersion = reader.ReadInt32();
LoadCustomVersion = reader.ReadInt32();

if (LoadVersion < MinVersion)
{
    MessageQueue.Enqueue(... CannotLoadDatabaseMinSupported, LoadVersion, MinVersion);
    return false;
}
else if (LoadVersion > Version)
{
    MessageQueue.Enqueue(... CannotLoadDatabaseMaxSupported, LoadVersion, Version);
    return false;
}
```

- 版本过低（<60）或过高（>117）：**立即 `return false`**，不读任何记录，世界数据全空。
- 调用方 `StartEnvir()` 里写的是裸调用 `LoadDB();`（`Envir.cs:3321`），**丢弃了返回值**——所以版本不符时服务器会"带着空世界"继续启动（只在消息队列里留一条日志）。这是一个真实的运维陷阱，Rust 版应改为硬失败。
- 没有 magic/校验和，所以"文件损坏"与"版本不同"在读取层是不可区分的：任何错位最终都会表现为「读到非法 bool（值 >1）」「读到大得离谱的 count」或 `EndOfStreamException`。建议 `mir-tools` 在读到 `bool ∉ {0,1}` 或 `count > 合理上限`（例如 10 万）时立即带偏移量报错。

---

## 7. 实现 `mir-tools`（Rust 读取器）的清单

```rust
// 建议结构
struct MirDb { header: Header, maps: Vec<MapInfo>, items: Vec<ItemInfo>, monsters: Vec<MonsterInfo>,
               npcs: Vec<NpcInfo>, quests: Vec<QuestInfo>, dragon: DragonInfo, magics: Vec<MagicInfo>,
               gameshop: Vec<GameShopItem>, conquests: Vec<ConquestInfo>, respawn: Option<RespawnTimer>,
               gt_maps: Option<Vec<GtMap>> }
```

必须实现的细节：

1. **7-bit varint 长度**：`while b & 0x80 != 0 { len |= (b & 0x7f) << shift; shift += 7 }`，之后读 `len` 字节 UTF-8（`.NET` 用 `replace` 语义，Rust 用 `String::from_utf8_lossy` 足够，因为地图/物品名都是合法 UTF-8）。
2. **严格按 `version` 分支**，顺序照 §4 的表；不要按「最新版」硬读旧文件。
3. **bool 校验**：`assert b <= 1`，否则报「偏移 X 处疑似错位，文件版本 Y」。
4. **`count` 上限保护**：接 `count` 后先检查 `count as u64 * min_record_size <= remaining`。
5. **`UserItem` 的槽位标志是反的**（`true` = 空），与背包的「`true` = 有物品」相反。`MirDB` 本身不含 `UserItem`（物品实例都在 `MirADB`），但如果 `mir-tools` 同时读 `MirADB` 就必须处理。
6. **`Stats` 用有序映射**（HashMap 即可）保留稀疏键。
7. **`DateTime`**：`DateTime::from_binary(i64)`（ticks + Kind）；`0` 视为 `None`。
8. **`i16 Shape`（`ItemInfo`）/`i16 Bind`/`i16 Unique`** 是有符号，不要读 u16。
9. **尾部 GTMap** 可选解析（§5）。
10. **不要**期待段间对齐/填充：`QuestInfo` 段之后紧跟 `DragonInfo` 的 `bool`，没有任何对齐字节。

伪代码（骨架）：

```rust
let mut r = Reader::new(&bytes);
let version = r.i32();                      // 样本 = 105
let custom  = r.i32();                      // 样本 = 0（当前代码未使用）
let map_index      = r.i32();
let item_index     = r.i32();
let monster_index  = r.i32();
let npc_index      = r.i32();
let quest_index    = r.i32();
let gameshop_index = if version >= 63 { Some(r.i32()) } else { None };
let conquest_index = if version >= 66 { Some(r.i32()) } else { None };
let respawn_index  = if version >= 68 { Some(r.i32()) } else { None };

let maps = read_list(&mut r, |r| map_info(r, version))?;
let items = read_list(&mut r, |r| item_info(r, version))?;
let monsters = read_list(&mut r, |r| monster_info(r, version))?;
let npcs = read_list(&mut r, |r| npc_info(r, version))?;
let quests = read_list(&mut r, |r| quest_info(r, version))?;
let dragon = dragon_info(&mut r)?;
let magics = read_list(&mut r, |r| magic_info(r, version))?;
let gameshop = if version >= 63 { read_list(&mut r, |r| gameshop_item(r, version))? } else { vec![] };
let conquests = if version >= 66 { read_list(&mut r, |r| conquest_info(r, version))? } else { vec![] };
let respawn = if version > 67 { Some(respawn_timer(&mut r)?) } else { None };
let gt = if r.remaining() >= 4 { read_list(&mut r, |r| gt_map(r)).ok() } else { None };
assert_eq!(r.pos(), bytes.len(), "trailing bytes");
```

---

## 8. 实测校验（样本 `cjlaaa/Server.MirDB`）

### 8.1 文件前 64 字节（hex）

```
69 00 00 00 00 00 00 00 85 01 00 00 44 05 00 00
27 02 00 00 35 01 00 00 9D 00 00 00 76 00 00 00
00 00 00 00 3F 17 00 00 85 01 00 00 01 00 00 00
01 30 09 E6 AF 94 E5 A5 87 E7 9C 81 65 00 00 87
```

逐段解码：

| 偏移 | 字节 | 解码 | 说明 |
|---|---|---|---|
| 0 | `69 00 00 00` | `105` | **Version = 105** |
| 4 | `00 00 00 00` | `0` | CustomVersion |
| 8 | `85 01 00 00` | `389` | MapIndex |
| 12 | `44 05 00 00` | `1348` | ItemIndex |
| 16 | `27 02 00 00` | `551` | MonsterIndex |
| 20 | `35 01 00 00` | `309` | NPCIndex |
| 24 | `9D 00 00 00` | `157` | QuestIndex |
| 28 | `76 00 00 00` | `118` | GameshopIndex（`≥63`） |
| 32 | `00 00 00 00` | `0` | ConquestIndex（`≥66`；该库无攻城） |
| 36 | `3F 17 00 00` | `5951` | RespawnIndex（`≥68`） |
| 40 | `85 01 00 00` | `389` | **地图条数** |
| 44 | `01 00 00 00` | `1` | `MapInfo[0].Index` |
| 48 | `01` | len=1 | `FileName` 长度 |
| 49 | `30` | `'0'` | `FileName = "0"` |
| 50 | `09` | len=9 | `Title` 长度 |
| 51-59 | `E6 AF 94 E5 A5 87 E7 9C 81` | `比奇省` | `Title` |
| 60-61 | `65 00` | `101` | `MiniMap` |
| 62 | `00` | `0` | `Light` |
| 63-64 | `87 00` | `135` | `BigMap`（第 64 字节 `0x87` 正好是 64 字节窗口的最后一字节，所以窗口在此截断） |

> 注意偏移 44-47 的 `01 00 00 00` 是 `MapInfo[0].Index`，之后没有任何对齐/填充——第 **48** 字节就是 `FileName` 的长度前缀 `01`，第 49 字节是字符 `'0'`（`0x30`）。

### 8.2 段偏移与条数（同一解析器实测）

| 段 | count 所在偏移 | 段结束偏移 | 条数 | 段长（字节） |
|---|---|---|---|---|
| 固定头 | — | 40 | — | 40 |
| 地图 `MapInfo` | 40 | 251,090 | 389 | 251,050 |
| 物品 `ItemInfo` | 251,090 | 355,442 | 1,349 | 104,352 |
| 怪物 `MonsterInfo` | 355,442 | 398,505 | 544 | 43,063 |
| NPC `NPCInfo` | 398,505 | 429,110 | 309 | 30,605 |
| 任务 `QuestInfo` | 429,110 | 443,035 | 157 | 13,925 |
| `DragonInfo` | —（无 count） | 443,174 | 1 | 139 |
| 技能 `MagicInfo` | 443,174 | 448,441 | 107 | 5,267 |
| 商城 `GameShopItem` | 448,441 | 452,077 | 82 | 1,636 |
| 攻城 `ConquestInfo` | 452,077 | 452,081 | **0** | 4（只有 count） |
| `RespawnTimer` | — | **452,106** | 1 | 25 |
| GTMap（尾段） | — | — | **不存在**（105 版无此段） | 0 |

**解析结果：`consumed = 452,106`，`size = 452,106`，`leftover = 0`。**
→ 说明 §4 的字段顺序表对 `Version = 105` 是完备的：少一个字节或多一个字节都会导致偏移错位。

### 8.3 抽样记录（可用于单测断言）

```
Map[0]      : index=1  file_name="0"        title=比奇省   mini_map=101 light=0 big_map=135
              safe_zones=5  respawns=358     movements=70
Map[1]      : index=2  file_name="0122"     title=皇宫     mini_map=0  light=2 big_map=0
Item[0]     : index=1  name=祈祷之剑  type=1 grade=2 required_class=31 required_gender=3
              shape=13 weight=10 required_amount=20 image=54 durability=20000
              stack_size=1 price=30000 bind=256 unique=0 random_stats_id=1
              slots=0 stats={MaxDC:20, MinDC:8, Accuracy:3, AttackSpeed:-1} tooltip=""
Monster[0]  : index=1 name=弓箭守卫 image=139 ai=57 level=0 view_range=7 cool_eye=0
              stats={MinAC..MaxSC:255, Accuracy:255, Agility:255, HP:9999}
              light=20 attack_speed=2000 move_speed=0
              experience=0 can_push=false can_tame=false auto_rev=false undead=false drop_path=""
NPC[0]      : index=1 map_index=1 file="BichonProvince/BorderVillage/Border_Transport-0"
              name="Teleport_Gilbert" x=287 y=615 image=15 rate=100
Quest[0]    : index=1 name=助理的要求 group=比奇省 file_name="1"
              required_min_level=1 required_max_level=255 required_class=31
Magic[0]    : name=基本剑术 spell=1 icon=2 level1=7 level2=9 level3=12
              need1=27 need2=60 need3=130 delay_base=1800 range=9 multiplier_base=1.0
GameShop[0] : item_index=1268 g_index=1 gold_price=82500 credit_price=220 count=1
              class="All" category="Potion" stock=0
Dragon      : enabled=true map="D2083" monster="Evil Mir" body="00" exps=[10000, 20000, 30000, ...]
RespawnTimer: base_spawn_rate=20 current_tick_counter=71 options=[(0, 1.0)]
```

### 8.4 版本门实测（验证分支确实被走到）

| 检查 | 结果 | 含义 |
|---|---|---|
| `ItemInfo.Stats` 是否为字典格式 | 是（最多 12 项） | `Version > 84` ✅ |
| `ItemInfo.ToolTip` | 1,349/1,349 条存在 | 无条件字段 ✅ |
| 地图 `weather_particles` | **0/389** 出现 | `≥110` 门生效 ✅ |
| 怪物 `can_recall` | **0/544** 出现 | `≥115` 门生效 ✅ |
| NPC `conquest_visible` | **0/309** 出现 | `≥107` 门生效 ✅ |
| 商城 `can_buy_credit/gold` | **0/82** 出现 | `>105` 门生效 ✅ |
| 物品索引范围 | 1 … **1349**（头部 `ItemIndex = 1348`） | 头部计数器与记录数**不一致**，不要用它推算 ⚠️ |

---

## 9. 附：账号容器 `Server.MirADB`（代码推导，**未**用样本验证）

`Server.MirDB` 里没有账号，所以重写要存「账号 + 角色」时真正的输入是 `Server.MirADB`。其布局来自 `SaveAccounts`（`Envir.cs:2571-2612`）/ `LoadAccounts`（`Envir.cs:2944-3087`）：

```
i32   Version
i32   CustomVersion
i32   NextAccountID
i32   NextCharacterID
u64   NextUserItemID
i32   NextHeroID                 (Version > 98)
i32   GuildCount                 (= GuildList.Count)
i32   NextGuildID
i32   HeroCount ; HeroInfo[...]  (Version > 102)
i32   AccountCount ; AccountInfo[...]   // 每个账号内联其 Characters
u64   NextAuctionID
i32   AuctionCount ; AuctionInfo[...]
u64   NextMailID
i32   MailCount ; MailInfo[...]   (仅 Version <= 80 会读；**写端从不写此段**)
i32   LogCount ; (i32, i32)[...]  (读：Version >= 63；写：无条件)
i32   SaveCount ; RespawnSave[...] (读：Version >= 68；写：无条件)
```

> 注意 `SaveAccounts` 里没有 Mail 段（`Envir.cs:2571-2612` 全程没有写 Mail），但 `LoadAccounts` 在 `≤ 80` 时会去读它（`Envir.cs:3033-3047`）——又一处写读不对称。

- `AccountInfo` / `CharacterInfo` / `UserItem` 的字段级布局见 `schema.md` §2/§3/§4。
- `AuctionInfo`：`u64 AuctionID`, `UserItem`, `i64 ConsignmentDate`, `u32 Price`, `i32 SellerIndex`, `bool Expired`, `bool Sold`, 且 `>79` 时 `u8 ItemType`, `u32 CurrentBid`, `i32 CurrentBuyerIndex`（`AuctionInfo.cs:47-87`）。
- `RespawnSave`：`bool Spawned`, `u64 NextSpawnTick`, `i32 RespawnIndex`（`RespawnTimer.cs:11-23`）。

**⚠️ 验证状态**：资源库中现有两个 `Server.MirADB` 样本（`justkkalways-crystal-mir2-env-db/dbbackup`，`Version=110`；`masmx86-legend-of-mir-2`，`Version=116`）**无法**按 117 版字段顺序完整解析：头部计数器能自洽读出（v116 样本：`NextAccountID=11, NextCharacterID=10, NextUserItemID=320322, NextHeroID=10`），但在「英雄列表 → 账号列表」边界处错位。推断这两个文件来自旧版或二次开发服务端，`HeroInfo`/`AccountInfo` 的字段集合与参考源不一致（**未验证**）。
→ 结论：**账号导入器必须用与目标服务端同版本的 MirADB 样本开发与对拍**，不要直接照抄本节顺序硬解旧文件。相比之下 `Server.MirDB`（§1-§8）已经字节级验证完毕，可直接实现。
