# 移动系统：Walk / Run / Turn 的精确规则

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：`Server/MirObjects/HumanObject.cs`（玩家/英雄共用）、`Server/MirObjects/PlayerObject.cs`（玩家特有）、
> `Server/MirObjects/MapObject.cs`（动作表与广播）、`Server/MirEnvir/Map.cs`（格子/门）、
> `Server/MirNetwork/MirConnection.cs`（收包与限速）、`Shared/Functions/Functions.cs`（方向/坐标）。
> 本文只写「服务器权威逻辑」；客户端表现（动画时长）不在范围内。

---

## 1. 坐标与方向

- 方向枚举（`Shared/Enums.cs:833-843`）：

  | 值 | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
  |---|---|---|---|---|---|---|---|---|
  | `MirDirection` | `Up` | `UpRight` | `Right` | `DownRight` | `Down` | `DownLeft` | `Left` | `UpLeft` |

- 单步位移 `Functions.PointMove(p, dir, i)`（`Shared/Functions/Functions.cs:217-247`）：
  `Up→(0,-i)`、`UpRight→(+i,-i)`、`Right→(+i,0)`、`DownRight→(+i,+i)`、`Down→(0,+i)`、`DownLeft→(-i,+i)`、`Left→(-i,0)`、`UpLeft→(-i,-i)`。
- 反方向 `ReverseDirection`（`Functions.cs:318-341`），顺序旋转 `NextDir/PreviousDir`（`Functions.cs:134-179`）。
- 距离判定统一用切比雪夫距离：`InRange(a,b,i) = |a.X-b.X| <= i && |a.Y-b.Y| <= i`（`Functions.cs:71-74`）。
- 地图尺寸 `Width × Height`，合法坐标 `0 <= x < Width && 0 <= y < Height`（`Server/MirEnvir/Map.cs:637-644`）。

---

## 2. 动作/时间模型：`ActionTime` + `ActionList`

### 2.1 时间基准

- `Envir.Time` = `Stopwatch.ElapsedMilliseconds`——**服务器启动以来的单调毫秒数**（`Server/MirEnvir/Envir.cs:73,75`；
  赋值点 `Envir.cs:2000,2055`）。所有 deadline 都是「绝对毫秒时刻」，比较方式统一为 `Envir.Time >= deadline`。
- 所有时间是**服务器侧的时间**，与客户端时钟无关；客户端时钟只影响它自己何时发下一个移动包。

### 2.2 每个对象的时间字段

`Server/MirObjects/HumanObject.cs:186-189`：

```csharp
public const long LoyaltyDelay = 1000, ItemExpireDelay = 60000, DuraDelay = 10000, RegenDelay = 10000,
                  PotDelay = 200, HealDelay = 600, VampDelay = 500, MoveDelay = 600;
public long StruckTime, RunTime, ActionTime, AttackTime, RegenTime, SpellTime, ...;
protected int _stepCounter, _runCounter;
```

`MapObject` 侧（`Server/MirObjects/MapObject.cs:66`）：`public long CellTime, PKPointTime, LastHitTime, EXPOwnerTime;`

| 字段 | 含义 | 谁写 |
|---|---|---|
| `ActionTime` | **下一次可以开始任何动作的最早时刻**（走/跑/转向/攻击/施法都检查它） | `Walk` `Run` `Turn` `Harvest` `Attack` `Magic` |
| `AttackTime` | 下一次可攻击的最早时刻 | 攻击、施法（`HumanObject.cs:3424`） |
| `SpellTime` | 下一次可施法的最早时刻 | 施法（`HumanObject.cs:3425`，+1800ms） |
| `CellTime` | 「我刚移动过」的宽限截止时刻，用于**临时不阻挡别人** | `Walk`/`Run`/`Pushed`/传送/部分怪物 AI（400+ 处，典型值 `now+500`） |
| `RegenTime` | 下一次自然回血/回蓝结算时刻 | `ProcessRegen`（+10000ms） |

### 2.3 `ActionList`（延迟动作队列）

- 每个 `MapObject` 有 `public List<DelayedAction> ActionList`（`MapObject.cs:182`）。
- `DelayedAction { DelayedType Type; long Time; long StartTime; object[] Params; }`（`Server/MirObjects/DelayedAction.cs:27-48`）；
  `DelayedType` 见 `DelayedAction.cs:5-25`（`Magic/Damage/RangeDamage/Spawn/Die/Recall/MapMovement/Mine/NPC/Poison/DamageIndicator/Quest/SpellEffect`）。
- 每帧在 `MapObject.Process()` 里顺序扫描（`MapObject.cs:228-233`）：

```csharp
for (int i = 0; i < ActionList.Count; i++) {
    if (Envir.Time < ActionList[i].Time) continue;
    Process(ActionList[i]);
    ActionList.RemoveAt(i);          // 注意：RemoveAt 在循环内，会跳过一个元素（已有行为，保持一致）
}
```

- **没有 `AddAction()` 这种封装方法**——全部调用点都是 `ActionList.Add(new DelayedAction(type, Envir.Time + delay, ...))`。
- 地图也有自己的 `ActionList`（`Server/MirEnvir/Map.cs:38`），在 `Map.Process()` 里同样方式消费（`Map.cs:732-738`），
  用于 `DelayedType.Magic`（魔法分段伤害）和 `DelayedType.Spawn`（延时刷怪/刷法术）。

---

## 3. 走 / 跑 / 转身的耗时（**不按职业、不按护甲重量分档**）

这是最容易按「老传奇印象」写错的地方，Crystal 的实际情况是：

> **移动速度与职业、护甲重量、武器重量没有任何关系。** 所有 `HumanObject` 子类（战士/法师/道士/刺客/弓手、英雄）
> 共用同一个 `MoveDelay = 600ms`；`GetDelayTime()` 里唯一的修正因子是「减速毒」（`HumanObject.cs:247-254`）。

| 动作 | 基础耗时 | 修正 | 代码 |
|---|---|---|---|
| `Walk`（走 1 格） | **600 ms** | `Slow` 毒 → ×2 = 1200 ms | `HumanObject.cs:2494`，`GetDelayTime` `HumanObject.cs:247-254` |
| `Run`（跑 2 格，见 §5） | **600 ms**（一次动作） | 同上 | `HumanObject.cs:2616` |
| `Turn`（原地转身） | **350 ms** | 同上 | `PlayerObject.cs:113,4270` |
| `Harvest`（挖尸） | **350 ms** | 同上 | `PlayerObject.cs:113,4319` |
| 物理攻击 | `ActionTime += 600`；`AttackTime = 1400 - (AttackSpeed*60 + min(370, Level*14))`（`Stats[Stat.AttackSpeed]` 参与，下限 550） | — | `HumanObject.cs:1766-1768` |
| 施法 | `ActionTime += 600`、`SpellTime += 1800` | — | `HumanObject.cs:3424-3430` |
| 地图传送冷却 | `MovementTime = now + 2000` | — | `PlayerObject.cs:113,4564` |

统计口径：

- 走路 = **600ms/格**（1.67 格/秒）。
- 跑步 = **600ms/2 格 = 300ms/格**（3.33 格/秒）；骑乘或 `ActiveSwiftFeet` 时为 **600ms/3 格 = 200ms/格**（5 格/秒）。
- 「重量」唯一的影响是**背包负重**：`CanRun` 要求 `CurrentBagWeight <= Stats[Stat.BagWeight]`（`HumanObject.cs:135`），
  超额时 `Run()` 会退化成 `Walk()`（`HumanObject.cs:2516-2519`）。**人物装备重量（`CurrentWearWeight`/`CurrentHandWeight`）不参与任何移动计算。**

其它与移动互斥/叠加的状态（`HumanObject.cs:117-159`）：

```csharp
CanMove  = !Dead && Time >= ActionTime && !Poison(Paralysis|LRParalysis|Frozen);
CanWalk  = !Dead && Time >= ActionTime && !InTrapRock && !Poison(Paralysis|LRParalysis|Frozen);
CanRun   = !Dead && Time >= ActionTime && (_stepCounter > 0 || FastRun)
           && (!Sneaking || ActiveSwiftFeet) && CurrentBagWeight <= Stats[Stat.BagWeight]
           && !Poison(Paralysis|LRParalysis|Frozen);
CanAttack= !Dead && Time >= ActionTime && Time >= AttackTime
           && !Poison(Paralysis|LRParalysis|Frozen|Dazed) && Mount.CanAttack;
CanCast  = !Dead && Time >= ActionTime && Time >= SpellTime
           && !Poison(Stun|Dazed|Paralysis|Frozen) && Mount.CanAttack;
```

对应 `PoisonType`（`Shared/Enums.cs:1010-1024`，位标志）：`Slow=4`（移动/动作耗时 ×2）、`Frozen=8`、`Stun=16`（**只封法术，不封走路**）、
`Paralysis=32`、`LRParalysis=256`（远程麻痹）、`Dazed=1024`（封攻击/施法）。

---

## 4. `Walk(dir)` 的完整判定序列

`Server/MirObjects/HumanObject.cs:2404-2513`。Rust 端必须**按这个顺序**短路，否则同一 tick 内多个包的结果会与 Crystal 不同。

```
Walk(dir):
 1. if !CanMove || !CanWalk:                     → 发 S.UserLocation(当前dir,当前坐标) 给本人；return false     (:2406-2410)
 2. location = PointMove(CurrentLocation, dir, 1)
 3. if !CurrentMap.ValidPoint(location):          → 同上回拉；return false                                (:2414-2418)
       // 边界外 或 Cells[location].Attribute != Walk（三种墙位任一置位）
 4. if !CurrentMap.CheckDoorOpen(location):       → 同上回拉；return false                                (:2420-2424)
       // 该格有门且 DoorState != Open
 5. 遍历目标格 cell.Objects，若有阻挡者            → 同上回拉；return false                                (:2427-2447)
       · Race==Merchant 且自己是 Player：NPC 不可见(Visible==false 或 VisibleLog[自己Index] 为假) → 不算阻挡
       · 其它对象：若 !ob.Blocking 继续；若 CheckCellTime && ob.CellTime >= Envir.Time 继续（刚移动过的对象有 500ms 宽限）
 6. 若身上有 Concentration buff → 标记 Interrupted，UpdateConcentration                        (:2449-2458)
 7. 若 Hidden → RemoveBuff(Hiding)                                                             (:2460-2463)
 8. Direction = dir;                                                                          (:2465)
 9. if CheckMovement(location) → return false    // 地图传送/脚本坐标，见 §9（注意：此时方向已改、坐标未改）(:2466)
10. 旧格 Remove(this); RemoveObjects(dir,1);                                                   (:2468-2470)
11. CurrentLocation = location; 新格 Add(this); AddObjects(dir,1);                            (:2472-2476)
12. _stepCounter++                                                                             (:2478)
13. 安全区：GetSafeZone(CurrentLocation) → SetBindSafeZone(szi)，InSafeZone = (szi != null)     (:2480-2488)
14. if RidingMount → DecreaseMountLoyalty(1)；Moved()                                          (:2490-2491)
15. CellTime = now + 500;  ActionTime = now + GetDelayTime(600)                                (:2493-2494)
16. Enqueue(S.UserLocation{Direction, CurrentLocation})         // 只发给本人                  (:2496)
17. Broadcast(S.ObjectWalk{ObjectID, Direction, Location})      // 周围玩家（不含自己）       (:2497)
18. GetPlayerLocation()                                        // 组队/行会的坐标同步         (:2498)
19. 目标格上的所有 SpellObject.ProcessSpell(this)               // 火墙/毒等                   (:2500-2510)
    return true
```

`RemoveObjects/AddObjects`（`HumanObject.cs:7864`、`:8152`）负责视野内对象的增量下发：
沿移动方向扫描 `DataRange` 范围内的条带，进入视野的发 `GetInfo()`，离开视野的发 `S.ObjectRemove`。

**失败时的统一动作**：只给本人发 `S.UserLocation`（客户端据此把角色拉回服务器坐标），不广播、不改状态。

---

## 5. `Run(dir)` 的完整判定序列

`Server/MirObjects/HumanObject.cs:2514-2645`。

```
Run(dir):
 0. if CurrentBagWeight > Stats[BagWeight]: Walk(dir)     // 超重退化；注意没有 return，会继续往下走        (:2516-2519)
 1. steps = (RidingMount || (ActiveSwiftFeet && !Sneaking)) ? 3 : 2                                        (:2521)
 2. if !CanMove || !CanWalk || !CanRun:  → 发 UserLocation 回拉；return false                              (:2523-2527)
       // CanRun 内含 (_stepCounter > 0 || FastRun) —— 即「必须刚走过一步才能跑」
 3. Concentration 打断处理                                                                                 (:2529-2538)
 4. if Hidden && !Sneaking → 移除 Hiding / MoonLight / DarkBody                                            (:2540-2545)
 5. Moved()                                                 // 注意：跑之前调用，跑成功后不再调用          (:2547)
 6. Direction = dir
 7. for j in 1..steps:                                                                                     (:2551-2585)
       location = PointMove(start, dir, j)
       · !ValidPoint(location) → 回拉 UserLocation(旧位置)；return false
       · !CheckDoorOpen(location) → 回拉；return false
       · 该格 Objects 有阻挡者（规则同 Walk 第 5 步） → 回拉；return false
       · CheckMovement(location) 为真 → return false
       // 逐格校验：任一格不通过，整次跑完全失败，人物留在原地（不会走一半）
 8. Enqueue(S.UserLocation{Direction=dir, Location=最终格})                                                (:2586)
 9. Broadcast(S.ObjectRun{ObjectID, Direction=dir, Location=最终格})                                        (:2587)
       // 注意：这两包在真正改坐标(第 11 步)之前发出
10. if RidingMount && !Sneaking → DecreaseMountLoyalty(2)                                                  (:2589-2592)
11. Direction = dir；旧格 Remove；RemoveObjects(dir, steps)；CurrentLocation = 最终格；新格 Add；AddObjects  (:2594-2602)
12. 安全区刷新（同 Walk）                                                                                   (:2605-2613)
13. CellTime = now + 500; ActionTime = now + GetDelayTime(600)                                            (:2615-2616)
14. if !RidingMount: _runCounter++；若 _runCounter > 10 → _runCounter -= 8 且 ChangeHP(-1)                 (:2618-2625)
15. GetPlayerLocation()                                                                                    (:2627)
16. 对路径上每一格（从旧位置起 1..steps）执行 SpellObject.ProcessSpell(this)                               (:2629-2642)
    return true
```

### 5.1 「跑」的两个计数器

- `_stepCounter`（`HumanObject.cs:189`）：每走一步 +1（`:2478`）；**转身清零**（`PlayerObject.cs:4266`）；
  `ShoulderDash` 冲刺结算时清零（`HumanObject.cs:5156`）；`Process()` 中若 `CellTime + 700 < Envir.Time`（≈0.7 秒没动）也清零（`HumanObject.cs:259`）。
  效果：**站桩/转身后必须先走一步才能跑**。
- `_runCounter`（跑步「体力」）：每跑一次 +1；进程内每 1500ms 减 1（`HumanObject.cs:288-292`）；
  超过 10 时立刻 `-= 8` 并 `ChangeHP(-1)`（`:2619-2625`）。骑乘时不加 `_runCounter`。
- `FastRun`（`:218`）：由装备（`ItemInfo.CanFastRun`，`HumanObject.cs:1886-1888`、`:2340`）置位，可绕过 `_stepCounter` 门槛。

### 5.2 骑乘/疾风步的 3 格

`steps = 3` 的条件是 `RidingMount || (ActiveSwiftFeet && !Sneaking)`（`HumanObject.cs:2521`）。
`MovementInfo`/`Settings` 里没有其它倍率；`MapInfo.NoMount` 控制能否骑马（`Server/MirDatabase/MapInfo.cs:24-26,92`）。

---

## 6. `Turn(dir)`（转身）

`Server/MirObjects/PlayerObject.cs:4264-4310`：

```
Turn(dir):
 1. _stepCounter = 0                                  // 转身后不能立刻跑                     (:4266)
 2. if CanMove:                                                                             (:4268)
      · ActionTime = now + GetDelayTime(350)                                                  (:4270)
      · Direction = dir
      · if CheckMovement(CurrentLocation) → return    // 自己脚下就有传送点的情况             (:4273)
      · 安全区刷新（BindLocation/BindMapIndex/InSafeZone）                                     (:4275-4284)
      · 该格 SpellObject.ProcessSpell(this)                                                   (:4286-4298)
      · TradePartner != null → TradeCancel()；ItemRentalPartner != null → CancelItemRental()   (:4300-4304)
      · Broadcast(S.ObjectTurn{ObjectID, Direction, Location})   // 给周围玩家               (:4306)
 3. Enqueue(S.UserLocation{Direction, CurrentLocation})   // 无论成功失败都回显给本人        (:4309)
```

要点：
- 转身**不移动格子**，只改 `Direction`；
- 转身会取消交易/物品租赁（和走路不同——走路是 `Moved()` 里取消，`PlayerObject.cs:568-577`）；
- 转身后立刻跑步会被 `CanRun` 的 `_stepCounter > 0` 拦下，服务器回发 `S.UserLocation`，客户端表现为「原地未动」。

---

## 7. 服务器侧限速与「不丢包」重试

所有移动包先进入连接层，`Server/MirNetwork/MirConnection.cs:1155-1181`：

```csharp
private void Turn(C.Turn p) { if (Stage != GameStage.Game) return;
    if (Player.ActionTime > Envir.Time) _retryList.Enqueue(p); else Player.Turn(p.Direction); }
private void Walk(C.Walk p) { ... if (Player.ActionTime > Envir.Time) _retryList.Enqueue(p); else Player.Walk(p.Direction); }
private void Run (C.Run  p) { ... if (Player.ActionTime > Envir.Time) _retryList.Enqueue(p); else Player.Run(p.Direction);  }
```

- 早到的包**不丢弃**，进 `_retryList`；连接 `Process()` 在每个循环末尾把它们整体搬回 `_receiveList`（`MirConnection.cs:264-265`），
  下一个处理周期重新走一遍判断。所以**客户端可以按自己的节奏连发，服务器会排队补执行**。
- 攻击/远程攻击/收割同样处理（`MirConnection.cs:1454-1480`），判断条件是 `ActionTime > now || AttackTime > now`。
- 连接 `Process()` 由主循环驱动：`Envir.cs:2066-2087`——只要 `conTime != Time`（毫秒数变化）就执行一次。

---

## 8. 广播的包、收件人与时机

包 ID 来自 `Shared/Enums.cs` 的 `ServerPacketIds`/`ClientPacketIds`（顺序枚举，实测值）：

| 方向 | 包 | ID | 字段（线格式，小端） | 收件人 |
|---|---|---|---|---|
| C→S | `C.Turn` | **10** | `Direction: u8`（1 字节） | — |
| C→S | `C.Walk` | **11** | `Direction: u8` | — |
| C→S | `C.Run` | **12** | `Direction: u8` | — |
| S→C | `S.UserLocation` | **23** | `X: i32, Y: i32, Direction: u8`（9 字节） | **仅本人** |
| S→C | `S.ObjectRemove` | 26 | `ObjectID: u32` | 视野内玩家 |
| S→C | `S.ObjectTurn` | **27** | `ObjectID: u32, X: i32, Y: i32, Direction: u8`（13 字节） | 视野内其它玩家 |
| S→C | `S.ObjectWalk` | **28** | 同上 | 视野内其它玩家 |
| S→C | `S.ObjectRun` | **29** | 同上 | 视野内其它玩家 |
| S→C | `S.Pushed` | 127 | `Direction, Location` | 仅本人 |
| S→C | `S.ObjectPushed` | 128 | `ObjectID, Direction, Location` | 视野内其它玩家 |

代码位置：`Shared/ClientPackets.cs:252-294`；`Shared/ServerPackets.cs:870-951`（UserLocation）、`:1048`（ObjectRemove）、
`:1068-1145`（ObjectTurn/ObjectWalk/ObjectRun）。`S.UserLocation.Observable == false`（`ServerPackets.cs:932-935`），
即「观战者」不会收到本人的回拉包。

广播范围与规则（`Server/MirObjects/MapObject.cs:408-420`）：

```csharp
public virtual void Broadcast(Packet p) {
    for (每个 player in CurrentMap.Players) {
        if (player == this) continue;                                  // 自己不发
        if (Functions.InRange(CurrentLocation, player.CurrentLocation, Globals.DataRange)) player.Enqueue(p);
    }
}
```

- `Globals.DataRange = 16`（`Shared/Globals.cs:37`，注释「Was 24」），切比雪夫距离 ≤16 且**同一张地图**。
  注意：广播用的是**移动后的 `CurrentLocation`**（第 16/17 步已经改过坐标），所以范围以新位置为中心。
- 时机：`Walk` 在第 15 步写 `ActionTime` **之后**、第 19 步结算法术**之前**发出；
  `Run` 在状态更新**之前**发出（`HumanObject.cs:2586-2587` 早于 `:2600`）——Rust 端在同一 tick 内同步完成即可，无外部可观测差异。
- 被拒绝的移动**不广播**任何 `ObjectWalk/ObjectRun`，只回拉本人。

---

## 9. 移动的联动逻辑

| 联动 | 触发点 | 代码 |
|---|---|---|
| 地图传送/脚本坐标 | `CheckMovement(location)`；`Walk` 中在第 9 步（已改 `Direction`、未改坐标），`Run` 中对每一格调用 | `PlayerObject.cs:4459-4570` |
| · 触发脚本坐标 | `MapInfo.ActiveCoords` 命中 → `CallDefaultNPC(DefaultNPCType.MapCoord, ...)` | `PlayerObject.cs:4463-4471` |
| · 传送点 | `MapInfo.Movements` 的 `Source == location`，可带 `NeedHole`（需要挖洞法术）、`ConquestIndex`（占领条件） | `PlayerObject.cs:4473-4500+` |
| · 传送冷却 | `if (Envir.Time < MovementTime) return false;`，`MovementTime = now + 2000` | `PlayerObject.cs:4461,4564` |
| 取消交易 | `Moved()` → `PlayerObject.Moved()` → `TradeCancel()` / `CancelItemRental()` | `PlayerObject.cs:568-577` |
| 安全区绑定 | 每次落脚都查 `Map.GetSafeZone` 并 `SetBindSafeZone` | `HumanObject.cs:2480-2488,2605-2613` |
| 破隐/打断 | `RemoveBuff(Hiding)`；`Concentration` 被打断 | `HumanObject.cs:2449-2463` |
| 陷阱 | `InTrapRock` 时 `CanWalk == false`（只能被打/等待） | `HumanObject.cs:128`、`MapObject.cs:945-952` |
| 骑乘忠诚 | 走 −1、跑 −2 | `HumanObject.cs:2490,2589-2592` |
| 格子法术 | 落脚后对该格所有 `SpellObject` 执行 `ProcessSpell` | `HumanObject.cs:2500-2510,2629-2642` |
| 组队/行会坐标 | `GetPlayerLocation()` | `HumanObject.cs:2727+` |

击退/位移走的是另一条路径 `Pushed(pusher, dir, distance)`（`HumanObject.cs:2649-2725`）：
逐格判定是否阻挡（阻挡则停在原地），成功后 `Direction = ReverseDirection(dir)`，
发 `S.Pushed`(127) + `S.ObjectPushed`(128)，并统一 `ActionTime = now + 500`（**不是** 600）。

---

## 10. 怪物移动（对照）

`Server/MirObjects/MonsterObject.cs:2095-2149`：

```csharp
public virtual bool Walk(MirDirection dir) {
    if (!CanMove) return false;                       // CanMove: !Dead && Time > ActionTime && Time > MoveTime && ...
    location = PointMove(CurrentLocation, dir, 1);
    if (!CurrentMap.ValidPoint(location)) return false;      // 不检查门！
    if (格子上有 Blocking 对象) return false;
    ... 换格、RemoveObjects/AddObjects ...
    CellTime   = now + 500;
    ActionTime = now + 300;
    MoveTime   = now + MoveSpeed;
    if (MoveTime > AttackTime) AttackTime = MoveTime;
    Broadcast(new S.ObjectWalk{...});                 // 怪物也发 ObjectWalk
    ... 格子法术 ...
}
```

- `MoveSpeed` 来自 `MonsterInfo.MoveSpeed`（默认 **1800ms**，`Server/MirDatabase/MonsterInfo.cs:32`），
  载入后下限 400ms（`MonsterObject.cs:829-830`），部分 AI（宠物、Deer 等）会动态 ±。
- 怪物**不会跑步**（没有 `Run`），**不检查门**（只看 `ValidPoint`），**不调用 `CheckMovement`**（不触发传送）。
- 转身 `MonsterObject.Turn`（`MonsterObject.cs:2073-2093`）只改方向并广播 `S.ObjectTurn`，不消耗 `ActionTime`。
- 地面物品 `ItemObject`、法术圈 `SpellObject` 等不参与移动判定。

---

## 11. Rust / tokio 实现清单

1. **时钟**：一个进程级单调时钟（`Instant` 起点 + `now_ms()`），等价 `Stopwatch.ElapsedMilliseconds`；所有 deadline 用 `u64` 毫秒。
   不要用 `SystemTime`（会被系统时间跳变影响）。
2. **`ActionTime`/`AttackTime`/`SpellTime`/`CellTime` 全部保留**，比较统一 `now >= deadline`（Crystal 用 `Envir.Time >=`，
   `MirConnection` 层用 `>`，两处差一跳，实现时照抄即可）。
3. **判定顺序照抄 §4/§5**：`CanMove/CanWalk` → 边界/可走 → 门 → 占位 → buff 打断 → 方向 → `CheckMovement` → 换格 → 计时 → 发包。
   顺序不同会导致「同一 position 下包被接受/拒绝」的差异。
4. **`Run` 是「全有或全无」**：2（或 3）格必须同时可通行，任一格失败则整次失败、位置不变。
5. **`_stepCounter` / `_runCounter` / `FastRun`** 必须实现，否则玩家可以连续奔跑（等于移速翻倍）。
6. **`CellTime` 的 500ms 宽限必须实现**，否则多人同格时会互相卡死（两人同时走进对方格子会被判阻挡）。
7. **发包**：成功才广播；`UserLocation`(23) 给本人、`ObjectWalk`(28)/`ObjectRun`(29)/`ObjectTurn`(27) 给 `DataRange=16` 内的其它玩家；
   被拒绝只发 `UserLocation`。
8. **排队重试**：Crystal 把早到的包放进 `_retryList` 下个周期重试；Rust 端可以做得更简单（直接按 `ActionTime` 排队一个 deadline 事件），
   但必须保证「早到的移动包不会被丢弃」，否则客户端会有输入丢失的手感差异。
9. **单线程玩家、可按地图并发**：Crystal 用 `Settings.Multithreaded` + `Settings.ThreadLimit`（默认 true/2）把怪物按地图分片，
   玩家始终在主循环处理（`Envir.cs:2128-2136`）。Rust 端建议「每地图一个 actor，玩家所在图的任务串行」，等价且更安全。
