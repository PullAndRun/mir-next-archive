# NPC 系统（实体 / 脚本对话 / 商店 / 修理）

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：`Server/MirObjects/NPCObject.cs`（NPC 对象、可见性、`GetInfo`）、
> `Server/MirObjects/NPC/NPCScript.cs`（脚本装载、`Call`、`ProcessSpecial`、`Buy`）、
> `Server/MirObjects/NPC/NPCSegment.cs`（`#IF`/`#ACT` 的解析与执行、`ParseSay`、`Compare`）、
> `Server/MirObjects/NPC/NPCPage.cs`、`NPCChecks.cs`、`NPCActions.cs`、
> `Server/MirObjects/PlayerObject.cs`（`CallNPC`/`BuyItem`/`SellItem`/`RepairItem`/`CheckMapInfo`）、
> `Server/MirDatabase/NPCInfo.cs`、`Server/MirEnvir/Map.cs`（建图时刷 NPC）、
> `Server/MirEnvir/Envir.cs`（`CreateShopItem`/`GetNPCInfo`）、`Server/Settings.cs`、
> `Shared/ServerPackets.cs`、`Shared/ClientPackets.cs`、`Shared/Data/ItemData.cs`（`Price`/`RepairPrice`）。
>
> Rust 侧实现：
> `crates/mir-db/src/npc_script.rs`（**纯数据**脚本解析）、
> `crates/mir-server/src/npc.rs`（实体、对话执行、商店、修理、定价、进图资料）、
> `crates/mir-server/src/gamedata.rs`（脚本缓存 + NPC 模板查询）、
> `crates/mir-server/src/world.rs`（`npcs` 列表、`Player::npc` 状态、NPC 挡路、进图发 `ObjectNPC`）、
> `crates/mir-server/src/session.rs`（`C.CallNPC`/`C.BuyItem`/`C.SellItem`/`C.RepairItem`、`S.NewMapInfo`）。
> 测试：`crates/mir-server/tests/npc.rs`（25 个确定性用例）、
> `crates/mir-db/tests/npc_scripts.rs`（659 份真实脚本）、
> `crates/mir-db/src/npc_script.rs` 内的 16 个单元测试。

---

## 1. 结论先行

- **NPC 是建图时一次性刷出的静态对象**：`MapInfo.CreateMap` 把 `MapIndex` 匹配的
  `NPCInfo` 塞进这张图（`Server/MirDatabase/MapInfo.cs:197-204`），`Map.Load` 再
  `AddObject(new NPCObject(info))`（`Server/MirEnvir/Map.cs:490-496`）。
  本作 `World::spawn_npcs(map_file)` 做同样的事，且 NPC **挡路**
  （`NPCObject.Blocking => Visible`，`NPCObject.cs:418-421`）。
- **脚本商品表在 NPC 脚本里**，不在 `Envir/Goods/`：`[TRADE]` 段每行 `物品名 [数量]`
  （`NPCScript.ParseGoods`，`NPCScript.cs:650-684`）。`Envir/Goods/<序号>.msd` 是**二进制**
  的「玩家卖回的二手货」列表（`NPCScript.LoadGoods`，`:180-217`），本作未实现。
- **`MapInformation` 里没有 NPC**：Crystal 的 `ServerPackets.MapInformation`
  （`Shared/ServerPackets.cs:469-517`）只有地图索引/文件名/标题/小地图/光照/音乐/天气。
  进图时下发的 **NPC 列表 + 传送点** 在 `S.NewMapInfo`（`:519-540`）的 `ClientMapInfo`
  （`Shared/Data/ClientData.cs:320-357`）里；屏幕上的 NPC 外观则靠 `S.ObjectNPC`
  （`NPCObject.GetInfo`，`NPCObject.cs:381-395`）。本作严格照此实现（见 §6）。
- **买东西的货号 = 脚本行号**：`Envir.CreateShopItem(info, (uint)i)` 的 `i` 是
  `[Trade]` 那一行在脚本里的下标（`NPCScript.cs:667`），客户端用这个号在 `C.BuyItem` 里点货。
- **卖价固定是 `Price() / 2`**（`PlayerObject.cs:8084`），Crystal 这个版本**没有**
  `SellRate` 之类的配置项；**修理**按 `RepairPrice() * PriceRate` 收费，且每次修理让
  持久上限掉 `(MaxDura - CurrentDura) / 30`（`:8152`）。
- **对话按钮是白名单机制**：客户端只能点「当前页按检查结果实际展示出来的按钮」，
  否则服务端记日志并拒绝（`NPCScript.cs:831-857`；本作 `World::call_npc` 同）。

---

## 2. 脚本格式（`Envir/NPCs/<file>.txt`）

脚本文件名来自 `NpcInfo.FileName`（相对 `Envir/NPCs/`，不含 `.txt`；
Crystal `Path.Combine(Settings.NPCPath, FileName + ".txt")`，`NPCScript.cs:142`；
`Settings.NPCPath = ./Envir/NPCs`，`Server/Settings.cs:24`）。

### 2.1 文件级指令

| 语法 | 语义 | Crystal |
|---|---|---|
| `;注释` | 整行注释（页面扫描与段解析都跳过） | `NPCScript.cs:431`、`NPCSegment.cs:530` |
| 空行 | 在切页之前被丢掉（`Say` 列表里不会出现空串） | `NPCScript.cs:419` |
| `#INSERT [相对路径] …` | 读 `Envir/<相对路径>`，把内容**追加到文件尾**；展开后删掉所有 `#INSERT` 行 | `NPCScript.cs:303-328` |
| `#INCLUDE [相对路径] @Section` | 在外部文件里找 `[@Section]`，取其后 `{` … `}` 之间的行，**插入到本行之后** | `NPCScript.cs:330-385` |
| `[@Section]` | 一个对话页（key 大写、带方括号；本作只收 `[@…]` 形式的页） | `NPCScript.cs:415-507` |
| `[Types]` | 每行一个 `ItemType` 整数：这个 NPC 收购/修理允许的类型 | `NPCScript.cs:622-635` |
| `[UsedTypes]` | 二手货回收的类型限制（本作未用） | `NPCScript.cs:637-648` |
| `[Trade]` | 每行 `物品名 [数量]` → 商品表 | `NPCScript.cs:650-684` |
| `[Quests]` / `[Recipe]` / `[Speech]` | 任务/锻造/闲聊，本作**未实现** | `NPCScript.cs:685-783` |

### 2.2 页面内的指令

| 指令 | 语义 | Crystal |
|---|---|---|
| `#IF` | 后面每行是一个条件（见 §3） | `NPCSegment.cs:537-540` |
| `#SAY` | 正文；`<文本/@goto>` 是按钮；`{文本/颜色}` 是颜色标记 | `:541-544` |
| `#ACT` | 检查通过时执行的动作 | `:545-548` |
| `#ELSEIF` | **Crystal 不支持**（解析时抛 `NotImplementedException`，`:557-558`）。本作接受并忽略其条件行 | — |
| `#ELSESAY` | 检查不通过时的正文 | `:549-552` |
| `#ELSEACT` | 检查不通过时执行的动作 | `:553-556` |
| 其它 `#xxx` | Crystal 直接抛异常；本作忽略该指令行 | `:557-558` |

段落的切分规则：**每个 `#IF` 开一个新段**（`NPCScript.cs:454-458` 的 `nextSection`），
页头之后的裸正文属于第一段（`NPCSegment.cs:522` 的 `currentSay = say` 初值）。
一段的流程是「条件 → (`#ACT` 或 `#ELSEACT`) → (`#SAY` 或 `#ELSESAY`)」，
所有段的条件互不相关：每段各自决定走 SAY 还是 ELSESAY，正文按段顺序累积
（`NPCSegment.Success/Failed`，`:4956-4974`）。

### 2.3 按钮

- `<文本/@goto>`：按钮 key 是 `<` 之后第一个 `/@…` 到 `>` 之间的内容，遇到 `/` 截断
  （Crystal 正则 `<.*?/(\@.*?)>` + `argu.Split('/')[0]`，`NPCScript.cs:524,570-573`）。
  文本里可以嵌 `<标签>`（例如 `<[ 1 ]/@Guess(1)>`）；
- `<Close/@exit>`：**客户端自己关窗口、不发包**（`Client/MirScenes/Dialogs/NPCDialogs.cs:288-293`）。
  本作仍接受 `C.CallNPC{Key="[@exit]"}`，把服务端对话状态清空并回一条空正文的 `S.NPCResponse`；
- `#ACT GOTO @页面` 一类的跳转不经过客户端：服务端执行完当前段就**立即**跟进
  （`DelayedAction(DelayedType.NPC, -1, …)` + `CallNPCNextPage`，`NPCSegment.cs:3684-3689`、
  `PlayerObject.cs:7930-7942`）。本作在 `World::call_npc` 里循环跟进，上限
  `MAX_NPC_HOPS = 8` 跳（Crystal 没有上限）。

### 2.4 正文里的占位符与颜色

- `<$NAME>`：服务端替换（Crystal `NPCSegment.ReplaceValue`，`:1266-1641`）。
  本作支持：`NPCNAME`、`USERNAME`、`LEVEL`、`CLASS`、`MAP`、`X_COORD`、`Y_COORD`、
  `HP`、`MAXHP`、`MP`、`MAXMP`、`GAMEGOLD`、`PKPOINT`、`USERCOUNT`、
  以及装备槽 `ARMOUR`/`WEAPON`/`RING_L`/`RING_R`/`BRACELET_L`/`BRACELET_R`/
  `NECKLACE`/`BELT`/`BOOTS`/`HELMET`/`AMULET`/`STONE`/`TORCH`。
  未知占位符**原样保留**（Crystal 同）。
- `{文本/颜色}`：**客户端**渲染的颜色标记（`NPCDialogs.cs` 的富文本解析）。
  本作客户端还不支持富文本，所以服务端在 `render_say_line` 里把外壳剥掉、只留文字
  （`strip_color_markup`，支持嵌套与 `<$…>` 混写，例如 `{<$WEAPON>/CORAL}`）。
- 按钮 `<文本/@goto>` 原样保留在正文里，客户端据此渲染可点文字。

---

## 3. 条件支持矩阵（`#IF`）

比较运算符全部来自 `NPCSegment.Compare`（`:4996-5008`）：`<` `>` `<=` `>=` `==` `!=`。
运算符写错时 Crystal 记日志并**让整段检查通过**（`:2116-2120` 的 `catch { return true; }`），
本作同样处理（`CheckOutcome::InvalidOperator`）。

| 关键字 | 参数 | 状态 | 语义 / Crystal |
|---|---|---|---|
| `LEVEL`（也接受 `CHECKLEVEL`） | `<op> <整数>` | ✅ | `player.Level` 比较；`ushort.TryParse` 失败算不通过（`:2104-2122`） |
| `CHECKGOLD` | `<op> <整数>` | ✅ | `Account.Gold` 比较（`:2124-2140`） |
| `CHECKPKPOINT` | `<op> <整数>` | ✅ | PK 系统未实现，左侧恒为 0（`:2292-2308`） |
| `CHECKITEM` | `<物品名> [数量] [持久秒]` | ✅ | 遍历背包含该模板的物品；给了持久就跳过 `CurrentDura < 秒*1000` 的（`:2176-2207`） |
| `CHECKCLASS` | `<职业名>` | ✅ | 忽略大小写（`MirClass.TryParse(..., true)`，`:2221-2231`） |
| `CHECKGENDER` | `<Male\|Female>` | ✅ | 区分大小写（`MirGender.TryParse(..., false)`，`:2209-2219`） |
| `CHECKQUEST` | `<序号> ACTIVE\|COMPLETE` | ✅（占位） | 任务系统未实现；读 `Player::npc.quests_active/completed`（`:2518-2535`） |
| `CHECK` | `[<flag>] <0\|1>` | ✅ | 标记位；下标须在 `0..=1999`（`Globals.cs:31`），越界算不通过（`:2334-2348`） |
| `RANDOM` | `<n>` | ✅ | `Random.Next(0, n) == 0` 才算通过 → 概率 **1/n**（不是百分比，`:2414-2422`） |
| `LEVEL` 之外的 `CHECK*`（`CHECKMAP`、`CHECKNAMELIST`、`CHECKHUM`、`CHECKMON`、`GROUPCOUNT`、`PETCOUNT`、`HASBAGSPACE`、`ISADMIN`、`CHECKTIMER`…） | — | ❌ | 记为「不支持」，**按不通过处理**（走 else 分支），并写进 `NpcReply::unsupported_checks` |
| 完全不认识的关键字 | — | ❌ | 同上（`NpcCheckKind::Unknown`） |

> 「不支持的条件按不通过处理」是本作的保守选择：宁可让玩家看到 else 分支，
> 也不让一条没有验证过的条件放行。Crystal 会真的去执行（本作缺系统时无法等价）。

---

## 4. 动作支持矩阵（`#ACT` / `#ELSEACT`）

| 关键字 | 状态 | 语义 / Crystal |
|---|---|---|
| `GOTO @页` | ✅ | 立即跟进目标页（`:3684-3689`）；本作上限 8 跳 |
| `BREAK` | ✅ | 本页剩余段不再处理（`:3700-3704`，`page.BreakFromSegments`） |
| `GIVEGOLD <n>` | ✅ | 加钱（`S.GainedGold`），溢出按 `u32::MAX` 截断（`:3244-3253`） |
| `TAKEGOLD <n>` | ✅ | 扣钱（`S.LoseGold`），不够就扣光（`:3255-3264`） |
| `SET [<flag>] <0\|1>` | ✅ | 写标记位；下标越界丢弃（`:3706-3726`） |
| `GIVEITEM` / `TAKEITEM` / `GIVEEXP` / `MOVE` / `MOV` / `CALC` / `MONGEN` / `CALL` / `TIMERECALL` / 行会/攻城/邮件/婚姻/英雄… | ❌ | 记为「不支持」，写进 `NpcReply::unsupported_actions`，不执行 |

---

## 5. 商店与修理

### 5.1 商品表的建立

1. `NpcScript::load` 解析 `[TRADE]` 段，每行得到 `GoodsItem { unique_id: 行号, name, count }`
   （`NPCScript.cs:650-684`）；
2. `World::npc_goods` 用 `GameData::item_by_name` 把名字解析成模板（找不到就跳过，
   Crystal 在解析阶段就过滤，`:663-665`），并生成「商店物品」：
   `UniqueID = 行号`、`CurrentDura = MaxDura = ItemInfo.Durability`、`IsShopItem = true`
   （`Envir.CreateShopItem`，`Envir.cs:4426-4439`）；
3. 打开页面时下发 `S.NPCGoods { List, Rate, Type = Buy, HideAddedStats = true }`
   （`NPCScript.SendNPCGoods`，`PlayerObject.cs:14677-14680`；`Settings.GoodsHideAddedStats = true`，
   `Settings.cs:286`）。

### 5.2 定价公式

`Price()`（`Shared/Data/ItemData.cs:516-542`）：

```
p = Info.Price
if Info.Durability > 0:
    r = (Info.Price / 2F) / Info.Durability
    p = (uint)(MaxDura * r)
    r = MaxDura > 0 ? CurrentDura / (float)MaxDura : 0
    p = (uint)Math.Floor(p / 2F + (p / 2F) * r + Info.Price / 2F)
p = (uint)(p * (AddedStats.Count * 0.1F + 1F))     # AddedStats.Count = 属性绝对值之和
return p * Count
```

`RepairPrice()`（`:543-563`）：

```
if Info.Durability == 0: return 0
p = Info.Price
p = (uint)Math.Floor(MaxDura * ((Info.Price / 2F) / Info.Durability) + Info.Price / 2F)
p = (uint)(p * (AddedStats.Count * 0.1F + 1F))
cost = p * Count - Price()          # 本作用 saturating_sub，见 §7
return cost                         # 无 RentalInformation → 不乘 2
```

Rust：`npc::item_price` / `npc::repair_price`（`f32` 运算 + 向零截断，与 C# 对齐），
价格倍率 `npc::apply_price_rate(cost, rate)`。

### 5.3 买卖

**买**（`C.BuyItem{ItemIndex, Count, Type}` → `World::buy_item`）：

| 步骤 | 判定 | Crystal |
|---|---|---|
| 1 | 已死亡 / `count < 1` → 忽略 | `PlayerObject.cs:7946` |
| 2 | `PanelType != Buy` → 忽略 | `:7963-7968` |
| 3 | 当前页必须是 `[@BUY]`/`[@BUYSELL]`/`[@BUYBACK]`/`[@BUYUSED]`/`[@PEARLBUY]`/`[@BUYNEW]`/`[@BUYSELLNEW]` | `:7948-7955` |
| 4 | NPC 必须在 `DataRange`（16）格内 | `:7961` |
| 5 | 货号必须在商品表里；`count <= Info.StackSize` | `NPCScript.cs:1180` |
| 6 | `cost = Price() * PriceRate`；`cost > Gold` → 忽略 | `:1185-1195` |
| 7 | `CanGainItem` 为假 → 忽略 | `:1200` |
| 8 | 扣钱 → `S.LoseGold`；`GainItem` → `S.GainedItem` | `:1208-1217` |

**扩展**：本作在 `World::enforce_bag_weight = true`（默认）时额外查一次负重，
超重返回 `NpcError::TooHeavy`。原因：本作还没有「力量/负重」之外的限制，
拾取路径已经这么做了（见 `docs/world/items.md`），买卖保持一致。

**卖**（`C.SellItem{UniqueID, Count}` → `World::sell_item`）：

| 步骤 | 判定 | Crystal |
|---|---|---|
| 1 | 无论如何先回 `S.SellItem{UniqueID, Count, Success}`（失败 `Success=false`） | `PlayerObject.cs:7990,8082-8090` |
| 2 | 已死亡 / `count == 0` → 失败 | `:7992-7996` |
| 3 | 当前页必须是 `[@SELL]`/`[@BUYSELL]` | `:7998-8002` |
| 4 | 离开 16 格 → **直接返回，连失败包都不发** | `:8008` |
| 5 | 背包里找不到 UUID 或 `count > 物品数量` → 失败 | `:8021-8025` |
| 6 | `BindMode.DontSell` → 失败 | `:8027-8031` |
| 7 | `[Types]` 非空且不含该类型 → 系统聊天 + 失败 | `:8041-8046` |
| 8 | 可堆叠且只卖一部分 → 复制出一件、原格数量减少；否则整格清空 | `:8048-8062` |
| 9 | `Success = true` → 进账 `Price() / 2`（`S.GainedGold`） | `:8082-8084` |

**简化**：`Settings.GoodsOn` 的「回购列表」（`callingNPC.BuyBack`，`:8066-8080`）未实现
（`Envir/Goods/*.msd` 也未实现）；`RentalInformation` 的 `DontSell`（`:8033-8037`）不适用
（本作没有租借）。

### 5.4 修理

`C.RepairItem{UniqueID}` → `World::repair_item`：

1. **先**发 `S.RepairItem{UniqueID}`（`PlayerObject.cs:8094`，客户端用它清「修理中」标记）；
2. 死亡 / 当前页不是 `[@REPAIR]` → 返回；
3. 离开 16 格 → 返回；背包里找不到物品 → 返回（不再发别的包）；
4. `BindMode.DontRepair` → 系统聊天「无法修理」；`[Types]` 不含该类型 → 系统聊天「这里不能修」；
5. `cost = RepairPrice() * PriceRate`；`cost > Gold` → 返回（一包不发，除了第 1 步那条）；
6. 扣钱 → `S.LoseGold`；
7. `MaxDura = max(0, MaxDura - (MaxDura - CurrentDura)/30)`，`CurrentDura = MaxDura`；
8. `S.ItemRepaired{UniqueID, MaxDura, CurrentDura}`。

特修（`[@SREPAIR]`，费用 ×3）**未接入**：客户端包 `C.SRepairItem` 本作还没有对应入口。

---

## 6. 协议包一览

| 方向 | 包（Rust 名） | 编号 | 何时发 | Crystal |
|---|---|---|---|---|
| S→C | `MapInformation` | 17 | 进世界 | `ServerPackets.cs:469-517`（**不含** NPC/传送点） |
| S→C | `NewMapInfo` | 18 | 进世界（紧随 `MapInformation`） | `:519-540` + `PlayerObject.cs:1009-1056` |
| S→C | `ObjectNPC` | 92 | 玩家进视野 / NPC 刷出 | `NPCObject.GetInfo`，`NPCObject.cs:381-395` |
| S→C | `NPCResponse` | 93 | 每次页面执行完（含 GOTO 的中间页） | `NPCScript.Response`，`:895-900` |
| S→C | `NPCGoods` | 102 | 打开 `[@BUY]`/`[@BUYSELL]` | `ProcessSpecial`，`:927-953`（**gzip 压缩**） |
| S→C | `NPCSell` | 103 | 打开 `[@SELL]` / `[@BUYSELL]` | `:949-951`、`:968-970` |
| S→C | `NPCRepair` | 104 | 打开 `[@REPAIR]`（带 `Rate`） | `:971-973` |
| S→C | `NPCSRepair` | 105 | 打开 `[@SREPAIR]` | `:974-976` |
| S→C | `LoseGold` | 68 | 买/修扣钱、`TAKEGOLD` | `NPCScript.cs:1209`、`PlayerObject.cs:8149` |
| S→C | `GainedGold` | 67 | 卖东西、`GIVEGOLD` | `PlayerObject.cs:8084`（`GainGold`） |
| S→C | `SellItem` | 111 | 卖东西（成功/失败都发） | `PlayerObject.cs:7990-8090` |
| S→C | `GainedItem` | 66 | 买到手 | `HumanObject.GainItem`（`:7824-7835`） |
| S→C | `RepairItem` | 113 | 修理请求的应答（最先发） | `PlayerObject.cs:8094` |
| S→C | `ItemRepaired` | 114 | 修理完成 | `:8157` |
| S→C | `Chat`（`System`） | 30 | 「这里不收这种东西」一类的提示 | `:8043`、`:8121-8129` |
| C→S | `CallNPC{ObjectID, Key}` | 50 | 点 NPC / 点按钮（`Key` = `[@goto]`） | `MirConnection.cs`、`NPCDialogs.cs:305-311` |
| C→S | `BuyItem{ItemIndex, Count, Type}` | 51 | 买 | `ServerPackets`/`ClientPackets` |
| C→S | `SellItem{UniqueID, Count}` | 52 | 卖 | 同上 |
| C→S | `RepairItem{UniqueID}` | 54 | 修 | 同上 |

**为什么 NPC 列表走 `NewMapInfo` 而不是塞进 `MapInformation`**：`MapInformation` 是
Crystal 客户端的既有协议，追加字段会让真实客户端解包错位；而 `ClientMapInfo`
（`NewMapInfo` 的负载）本来就有 `Movements` 与 `NPCs` 两个列表，且新版的
`ClientNPCInfo`（`Shared/Data/ClientData.cs:265-317`）字段与任务描述里的 12 个字段
一一对应。`World::map_entry_info` 就是按 `PlayerObject.CheckMapInfo`（`:1009-1056`）
构造这份数据：

* `Movements` 只收 `ShowOnBigMap` 的传送点，`Title` 取**目的地**地图的标题（`:1030-1045`）；
* `NPCs` 只收 `ShowOnBigMap` 的 NPC，按 `BigMapIcon` 升序（`:1047-1052`），
  `ObjectID` 填运行期 ID，其余字段来自 `NpcInfo.ClientInformation`；
* 屏幕上的 NPC 用 `S.ObjectNPC` 补齐（视野内、进世界时随 `World::enter` 一起发）。

---

## 7. 与 Crystal 的差异

| # | 差异 | 原因 |
|---|---|---|
| 1 | `MapInformation` 不含 `NPCs`/`Movements`，改由 `S.NewMapInfo` 携带 | 保持 Crystal 线格式（见 §6） |
| 2 | 不支持的条件/动作**按不通过/不执行**处理并记录，而不是崩溃 | 缺任务/行会/攻城/宠物等系统，无法等价执行 |
| 3 | `#ELSEIF` 被接受但忽略 | Crystal 在这里 `throw NotImplementedException`（`NPCSegment.cs:557-558`）；真实样本 0 处使用 |
| 4 | `#INSERT` 展开有 64 次上限、同文件最多 8 次 | Crystal 的 `lines.AddRange` 自引用会无限增长 |
| 5 | 页/段的查找用**精确 key**，Crystal 用 `StartsWith` 前景扫描 | 真实脚本的页头都是 `[@Name]`，结果等价且更安全 |
| 6 | `<$占位符>` 逐个替换；Crystal 用贪婪正则一次只替换最宽的一处 | 一行多个占位符时 Crystal 会漏替换（`:1268,1640`） |
| 7 | `{文字/颜色}` 由服务端剥掉 | 本作客户端还不支持富文本；Crystal 交给客户端渲染 |
| 8 | 空装备槽的 `<$WEAPON>` 等返回「无」 | Crystal 从语言表取「无武器」等键；本作没有这些键 |
| 9 | `Price()` 的 `p * Count`、`RepairPrice()` 的 `p * Count - Price()` 用饱和运算 | C# 是无符号回绕，会把价格算成天文数字；本作选择不溢出 |
| 10 | 买的时候额外查负重（可关） | 本作已有 `enforce_bag_weight` 扩展，保持与拾取一致 |
| 11 | `GOTO` 跟进上限 8 跳 | Crystal 无上限（脚本自跳会卡住连接） |
| 12 | `@exit` 由服务端清状态并回空正文 | Crystal 客户端自己关窗不发包；本作仍接受该 key |
| 13 | 对话状态放在 `Player::npc`（`NpcState`） | Crystal 分散在 `NPCObjectID`/`NPCPage`/`NPCSuccess`/`Info.Flags`；打包后便于测试与持久化扩展 |
| 14 | NPC 的时段/星期可见性、攻城可见性未实现 | `NPCObject.cs:228-254`、`:428-435` |

---

## 8. 常量速查

| 名称 | 值 | Rust 常量 | Crystal |
|---|---|---|---|
| NPC 交互距离 | 16 格（切比雪夫） | `npc::NPC_TALK_RANGE` | `Globals.DataRange = 16`（`Shared/Globals.cs:37`）+ `Functions.InRange` |
| 一轮对话最多跳转 | 8 | `npc::MAX_NPC_HOPS` | 无（`PlayerObject.cs:7930-7942`） |
| 标记位数量上限 | 1999 | `npc::FLAG_INDEX_COUNT` | `Globals.FlagIndexCount = 1999`（`Shared/Globals.cs:31`） |
| NPC 名字颜色 | `0xFF00FF00`（`-16711936`） | `npc::NPC_NAME_COLOUR` | `NPCObject.cs:50`（`Color.Lime`） |
| 商品面板隐藏附加属性 | `true` | `npc::GOODS_HIDE_ADDED_STATS` | `Settings.GoodsHideAddedStats`（`Settings.cs:286`） |
| 主页面 key | `[@MAIN]` | `npc_script::MAIN_SECTION` | `NPCScript.MainKey`（`NPCScript.cs:44`） |
| 修理掉耐久比例 | `/30` | `World::repair_item` | `PlayerObject.cs:8152` |
| 卖价比例 | `/2` | `World::sell_item` | `PlayerObject.cs:8084` |

---

## 9. 真实数据冒烟（Jev 数据集）

数据集：`resource/community/suprcode-crystal-database/Jev/`（`Server.MirDB` v111 + `Envir/NPCs` 660 份脚本）。

`crates/mir-db/tests/npc_scripts.rs` 的统计（每次跑测试都会打印）：

```
脚本 659 份（空文件 26、无 @Main 页 3）／页 5621／段 5622／
按钮 9853／商品行 4748（242 份脚本带商品）／带类型表 272／告警 0
```

`crates/mir-server/tests/npc.rs` 的两个冒烟用例：

```
比奇省 NPC 数量：46
能解析的脚本：46，其中有页面的：45
进图资料：movements=6 npcs=40
Blacksmith_Smith 的商品数：19
```

> 说明：比奇省（地图文件 `0`）在数据库里只有 46 个 NPC、6 个「大地图传送点」；
> 40 个 NPC 带 `ShowOnBigMap`（所以都进了 `NewMapInfo`）。
> `Envir/NPCs` 里 660 个 `.txt` 有 26 个是 0 字节空文件（例如
> `BichonProvince/HighPriestShinsu.txt`），解析器把它们当作「没有页面」的脚本。

---

## 10. 未做清单（TODO）

* **仓库**（`[@STORAGE]` / `C.StoreItem` / `C.TakeBackItem` / `C.DepositGold` / `S.NPCStorage` /
  `S.UserStorage` / `S.StorageGold`）：需要账号级存储与密码逻辑
  （`PlayerObject.cs:8161-8266`），本作还没有账号存储模型。
* **二手货 / 回购**（`Envir/Goods/<序号>.msd` 二进制 + `callingNPC.BuyBack` +
  `Settings.GoodsBuyBackTime`）：`NPCScript.LoadGoods`（`:180-217`）、`NPCObject.ProcessGoods`（`:68-134`）。
* **特修**（`[@SREPAIR]`，`C.SRepairItem`，费用 ×3）：`PlayerObject.cs:8092-8160` 的 `special` 分支。
* **任务接取/交付**（`[Quests]` 段、`CHECKQUEST` 的真实数据源、`C.AcceptQuest` 等）。
* **NPC 时段/星期可见性**（`NPCObject.Process`，`:228-254`）与 **攻城可见性**（`:428-435`）。
* **NPC 闲聊**（`[Speech]` 段 + `NPCObject.cs:256-270` 的 `ObjectChat` 广播）。
* **`#INSERT`/`#INCLUDE` 之外的系统脚本**（`DefaultNPC`、`Robot`、`AutoPlayer/AutoMonster`
  三种 `NPCScriptType`，`NPCScript.cs:151-162`）。
* **`%ARG(n)` 页面参数**（`NPCSegment.cs:51-68`）与 `%A1` 变量（`:86-100`）、
  `NPCVar` / `NPCData`（`Call` 里的输入框流程 `[@@"`，`:863-868`）。
* **`GIVEITEM`/`TAKEITEM`/`GIVEEXP`/`MOVE`/`MONGEN`/`CALL` 等动作**（见 §4）。
* **NPC 转身动画**（`NPCObject.cs:205-220` 每 10 秒随机转身 + `S.ObjectTurn` 广播）。
* **标记位与任务状态的持久化**：本作的 `NpcState::flags` / `quests_active` /
  `quests_completed` 只活在会话里（下线即丢）。Crystal 把 `Flags` 存在
  `CharacterInfo` 里（随角色行落盘），任务进度也落盘；等任务系统接入时一起补。
* **`Envir/NPCs` 里 `\` 分隔符的兼容**：`mir_db::npc_script::resolve_script_path` 已把
  两种分隔符统一成 `/`，但 `#INSERT` 的相对路径只在 Windows 风格下验证过。
