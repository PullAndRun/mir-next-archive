# 世界主循环（Tick Loop）与时间模型

> 参考实现根目录：`C:\Users\diana\ref\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：`Server/MirEnvir/Envir.cs`（`WorkLoop`/`Process`/`ThreadLoop`）、`Server/MirEnvir/Map.cs`（`Map.Process`、刷怪）、
> `Server/MirEnvir/RespawnTimer.cs`、各对象的 `SetOperateTime()`。

---

## 1. 结论先行：Crystal **没有固定 tick interval**

服务器世界循环是一个**无 sleep 的忙循环**，靠三套机制维持节奏：

1. **`Envir.Time` 单调毫秒时钟**（`Stopwatch`）——所有业务计时都用绝对毫秒 deadline；
2. **每对象的 `OperateTime`**——决定该对象本轮要不要被 `Process()`；
3. **每个循环迭代内 20ms 的对象处理预算**——把一轮里「能处理多少对象」限制住，其余留到下一轮。

因此「tick 间隔」不是一个常量，而是三者的共同结果：

| 维度 | 实际节奏 |
|---|---|
| 主循环迭代本身 | 尽可能快地空转（无 `Thread.Sleep`，`Envir.cs:2177` 处的 `Sleep(1)` 已被注释掉） |
| 玩家（`HumanObject`）的 `Process()` | **每次迭代都会执行**（`OperateTime = Envir.Time`，`HumanObject.cs:392-395`） |
| 怪物/物品/法术/NPC 的 `Process()` | 由 `SetOperateTime()` 决定，**最短 2000ms 一次**（除有更近的 deadline），见 §3 |
| 连接收包/发包 | `Envir.Time` 每变化 1 毫秒执行一次（`if (conTime != Time)`，`Envir.cs:2066`） |
| 每轮对象处理时间片 | 20 毫秒（`Envir.cs:2119`） |
| 地图逻辑 `Map.Process()` | **每次迭代都跑所有地图**（`Envir.cs:2140-2141`） |
| 全局周期任务 | 1s / 10s / 1min / 5min / 10min 多档，见 §5 |

> **Rust 重写建议**：用固定 16ms（或 20ms）的 `tokio::time::interval` 作为「世界 tick」，
> 并在同一 tick 内完成「连接收包 → 对象处理 → 地图处理 → 全局定时器 → 广播」的固定顺序，即可等价复现 Crystal 的可观测行为；
> 但要保留 §7 列出的语义约束（尤其是 `OperateTime` 的 2000ms 上限和 `ActionTime` 的绝对 deadline）。

---

## 2. 时间基准

```csharp
private readonly DateTime _startTime = DateTime.UtcNow;                    // Envir.cs:72
public readonly Stopwatch Stopwatch = Stopwatch.StartNew();                // Envir.cs:73
public long Time { get; private set; }                                     // Envir.cs:75
public DateTime Now => _startTime.AddMilliseconds(Time);                   // Envir.cs:85-86
```

- `Envir.Time`：**服务器进程启动以来经过的毫秒数**（`Stopwatch.ElapsedMilliseconds`），在 `WorkLoop` 开头与每次迭代开头刷新
  （`Envir.cs:2000`、`Envir.cs:2055`）。它是**单调**的（不受系统时间调整影响）。
- `Envir.Now`：`UTC 启动时刻 + Time`，只用于「昼夜光照」（`AdjustLights`，`Envir.cs:2294`）和「每日任务」判断（`Envir.cs:2311`）。
  注意它用的是 **UTC**，所以 `AdjustLights` 的 `Dawn/Day/Evening/Night` 是按 UTC 小时划分的。
- 连接层超时使用 `TimeOutTime = Envir.Time + Settings.TimeOut`（默认 `10000`，`Server/Settings.cs:67`；`MirConnection.cs:257,267`）。

---

## 3. 主循环 `WorkLoop()` 的完整结构

线程创建：`Envir.cs:3260`（`new Thread(WorkLoop) { IsBackground = true }`）。核心循环 `Envir.cs:2053-2179`：

```
while (Running):
  Time = Stopwatch.ElapsedMilliseconds                                    # :2055

  # (a) 每秒重置「上一秒处理了多少对象」的统计
  if Time >= processTime: LastCount/LastRealCount = 计数; 计数清零; processTime = Time + 1000     # :2057-2064

  # (b) 每「毫秒变化」做一次：昼夜光照 + 所有连接 Process（收包/发包）
  if conTime != Time:                                                     # :2066-2087
      conTime = Time
      AdjustLights()                                                      # :2070，见 map-format.md §5
      foreach c in Connections:       c.Process()
      foreach c in StatusConnections: c.Process()

  # (c) 对象处理：从 Envir.Objects 链表当前节点继续，最多 20ms
  if Settings.Multithreaded:
      唤醒所有 MobThread，并在主线程同步执行 ThreadLoop(MobThreads[0])     # :2099-2115
  Start = Stopwatch.ElapsedMilliseconds
  while !TheEnd && (ElapsedMilliseconds - Start) < 20:                    # :2119
      next = current.Next
      if !Multithreaded || current.Race != Monster || current.Master != null:   # :2128
          if Time > current.OperateTime:                                  # :2130
              current.Process(); current.SetOperateTime()
      current = next
  # current 指针跨迭代保留，所以下一轮从上次停下的位置继续（长时间公平轮转）

  # (d) 地图逻辑（每轮全部地图）
  for i in MapList: MapList[i].Process()                                  # :2140-2141
  DragonSystem?.Process()                                                 # :2143

  # (e) 全局周期任务
  Process()                                                               # :2145，见 §5

  # (f) 存档 / 公告（低频）
  if Time >= saveTime:   存档(账号/DB/行会/商品/攻城)，saveTime = Time + 5 分钟     # :2147-2155
  if Time >= userTime:   广播在线人数，userTime = Time + 5 分钟                    # :2157-2165
  if Time >= lineMessageTime: 广播跑马灯，lineMessageTime = Time + 10 分钟         # :2167-2175
```

异地异常处理：`Envir.cs:2181-2197` —— 任意异常会**踢掉所有连接**（`SendDisconnect(3)`）然后退出循环，这是很硬的容错策略；
Rust 端不应照搬（应记录并继续，或至少按 player 粒度隔离）。

### 3.1 多线程对象处理（`Settings.Multithreaded`）

- 默认开启：`Settings.Multithreaded = true`、`Settings.ThreadLimit = 2`（`Server/Settings.cs:46-47`）。
- 每张地图在构造时随机分配线程号：`Thread = Envir.Random.Next(Settings.ThreadLimit)`（`Map.cs:47`）。
- 怪物在 `Spawned()` 时根据所属地图的线程号挂到 `Envir.MobThreads[SpawnThread].ObjectsList`（`MapObject.cs:345-349`）。
- `ThreadLoop`（`Envir.cs:2220-2288`）的处理条件同样只看 `OperateTime`；
  主线程（Id 0）会一直跑到所有子线程 `Stop == true` 才返回（`Envir.cs:2259-2272`），形成每轮同步屏障。
- **玩家永远在主循环处理**：主循环的对象遍历跳过「怪物且无主人」的对象（`Envir.cs:2128`），
  即玩家、宠物（有 `Master` 的怪物）、物品、法术、NPC 都在主线程。

---

## 4. 每对象调度：`SetOperateTime()`

`MapObject` 中 `OperateTime` 表示**下次最晚必须被处理的时间**（`MapObject.cs:154-155`，默认 `OperateDelay = 100`；
`Spawned()` 时初始化为 `Time + Random.Next(OperateDelay)` 以打散负载，`MapObject.cs:351`）。
主循环条件：`if (Time > current.OperateTime) { Process(); SetOperateTime(); }`（`Envir.cs:2130-2134`）。

各对象的实现：

| 类型 | `SetOperateTime()` | 效果 |
|---|---|---|
| `HumanObject`（玩家/英雄） | `OperateTime = Envir.Time;`（`HumanObject.cs:392-395`） | **每次迭代都处理** |
| `MonsterObject` | 取 `now + 2000` 与一组定时字段的最小值：`AloneTime/DeadTime/OwnerTime/ExpireTime/PKPointTime/LastHitTime/EXPOwnerTime/SearchTime/RoamTime/ShockTime/...`（`MonsterObject.cs:1232-1300+`） | 最快 2000ms 一次，除非有更近的事件 |
| `SpellObject` | `now + 2000` 与 `TickTime/OwnerTime/ExpireTime/.../ActionList/PoisonList` 的最小值（`SpellObject.cs:418-460`） | 法术按 `TickSpeed` 醒来 |
| `ItemObject` | `now + 2000` 与 `OwnerTime/ExpireTime/PKPointTime/LastHitTime/EXPOwnerTime` 的最小值（`ItemObject.cs:144-165+`） | 地面物品最快 2000ms 一次 |
| `NPCObject` | `now + 2000` 与 `TurnTime/OwnerTime/ExpireTime/...` 的最小值（`NPCObject.cs:273-300+`） | 同上 |
| `DecoObject` | 同 `now + 2000` 模式（`DecoObject.cs:46-70+`） | 同上 |

`MapObject.Process()`（`MapObject.cs:204-234`）里每个对象都会做：清理失效引用、`PKPoints` 衰减、`LastHitter`/`EXPOwner` 超时、
以及**消费 `ActionList` 里所有到期的 `DelayedAction`**（见 movement.md §2.3）。玩家额外执行 `HumanObject.Process()`
（buff、回复、毒、装备耐久、坐骑忠诚、`_runCounter` 衰减等，`HumanObject.cs:255-390`）。

---

## 5. 周期任务表

### 5.1 `Envir.Process()`（每个主循环迭代都调用，内部用 deadline 分档）

字段声明：`Envir.cs:200-201`；实现：`Envir.cs:2309-2386`。

| 周期 | 任务 | 代码 |
|---|---|---|
| 每天（`Now.Day` 变化） | `ProcessNewDay()`（每日重置） | `Envir.cs:2311-2315` |
| **1 分钟** | 行会战计时 `GuildsAtWar[i].TimeRemaining -= 60000`，到期 `EndWar()` | `Envir.cs:2317-2328` |
| **1 分钟** | 所有行会 `Guild.Process()` | `Envir.cs:2330-2337` |
| **10 秒** | 所有攻城 `ConquestObject.Process()` | `Envir.cs:2339-2346` |
| **5 分钟** | `ProcessRentedItems()`（物品租赁到期） | `Envir.cs:2348-2352` |
| **10 分钟** | `ProcessAuction()`（拍卖结算） | `Envir.cs:2354-2358` |
| **10 秒** | **`Envir.RespawnTick.Process()`（全局刷怪心跳计数器）** | `Envir.cs:2360-2364` |
| **1 分钟** | `Robot.Process(RobotNPC)` | `Envir.cs:2366-2370` |
| **1 秒** | 清理到期的 `Envir.Timers` | `Envir.cs:2372-2385` |

### 5.2 `Map.Process()`（每个迭代、每张地图都调用，**自身没有再分档**）

`Server/MirEnvir/Map.cs:653-738`，顺序固定：

```
Map.Process():
 1. ProcessRespawns()                                    # 刷怪，见 §6
 2. foreach door: if Open && LastTick + 5000 < Time → 关门 + Broadcast(S.Opendoor{Close=true})   # :657-665
 3. if Info.Lightning && Time > LightningTime:
        LightningTime = Time + Random.Next(3000, 15000)
        对每个玩家生成 Spell.MapLightning 法术对象（1/4 概率落在玩家脚下，否则 ±10 范围随机点），
        ExpireTime = Time+1000, TickSpeed = 500                                          # :667-697
 4. if Info.Fire && Time > FireTime: 同上，生成 Spell.MapLava（3000~15000ms 间隔）        # :699-730
 5. 消费地图级 ActionList（到期 → Process(action) → RemoveAt(i)）                        # :732-738
```

要点：
- 门的自动关闭是 **全局固定 5000ms**（`Map.cs:659`），与开门者的操作无关；
- 雷鸣/火焰是**每张地图独立**的随机计时，间隔 `[3000, 15000)` 毫秒；
- `ProcessRespawns()` 只做「是否到点 + 补齐数量」，真正的刷怪在 `MapRespawn.Spawn()`。

---

## 6. 刷怪（Respawn）

### 6.1 两种时间模型（`Map.ProcessRespawns`，`Map.cs:740-790`）

```csharp
foreach respawn in Respawns:
    if respawn.Info.RespawnTicks != 0 && Envir.RespawnTick.CurrentTickcounter < respawn.NextSpawnTick: continue;  // 全局 tick 制
    if respawn.Info.RespawnTicks == 0 && Envir.Time < respawn.RespawnTime: continue;                              // 时间制
    if respawn.Count < respawn.Info.Count * Envir.SpawnMultiplier:
        补齐 (Count*Multiplier - Count) 只
    if Success:
        delay = max(1, Delay - RandomDelay + Random.Next(RandomDelay*2))
        respawn.RespawnTime = Envir.Time + delay * Settings.Minute        // 分钟级
        if RespawnTicks != 0: respawn.NextSpawnTick = CurrentTickcounter + RespawnTicks
    else:
        respawn.RespawnTime = Envir.Time + 1 分钟                          // 失败冷却
        失败计数 ErrorCount 累加，>5 时写日志
```

- `Settings.Minute == 60000`（`Server/Settings.cs:16`）。
- `Envir.SpawnMultiplier` 默认 `1`（`Envir.cs:160`）。

### 6.2 全局刷怪心跳 `RespawnTimer`

`Server/MirEnvir/RespawnTimer.cs:51-124`：

```csharp
public byte BaseSpawnRate = 20;          // 基准：20 分钟一个 tick
public ulong CurrentTickcounter = 0;     // 全局单调计数器（地图的 NextSpawnTick 与之比较）
public void Process() {                  // 由 Envir.Process 每 10 秒调用一次
    GetTickSpeed();
    if (Envir.Time >= LastTick + CurrentDelay) { CurrentTickcounter++; LastTick = Envir.Time; }
}
public void GetTickSpeed() {             // 按在线人数调整 tick 间隔（DelayLoss）
    bonus = min over RespawnTickOption where Option.UserCount <= PlayerCount
    CurrentDelay = BaseSpawnRate * 60000 * bonus;
}
```

即：`RespawnTicks` 的「tick」= 基准 20 分钟 × 在线人数系数；计数器 `CurrentTickcounter` 与玩家存档一起持久化（`Envir.cs:2501,2936`）。

### 6.3 出生点约束

- 建图时把 `WalkableCells` 裁剪到「刷新点 ± Spread」的方形范围内（`Map.cs:473-488`）；
- 刷新时随机取其中一个格：`Respawn.WalkableCells[Random.Next(count)]`（`MonsterObject.cs:706-732`）；
- 若 `WalkableCells` 为空则刷怪失败（`MonsterObject.cs:711`）；
- 刷出的怪物 `ActionTime = now + 2000`（`MonsterObject.cs:734-736`），即出生后 2 秒才开始行动。

### 6.4 死亡与重生

- 怪物死亡后 `DeadTime` 等字段参与 `SetOperateTime`，由 `Process` 触发 `Die/Revival`；
  复活时统一 `ActionTime = Envir.Time + RevivalDelay`——怪物见 `MonsterObject.cs:1024`，英雄见 `HeroObject.cs:681`。

---

## 7. Rust / tokio 实现必须保留的语义

### 7.1 时钟

```rust
// 等价 Envir.Time：进程启动以来的单调毫秒
static START: OnceLock<Instant> = OnceLock::new();
fn now_ms() -> u64 { START.get_or_init(Instant::now).elapsed().as_millis() as u64 }
```

- 全部 deadline 用 `u64` 毫秒绝对值（`ActionTime`/`AttackTime`/`OperateTime`/`CellTime`/`RespawnTime`/`NextSpawnTick`…）。
- 比较约定：对象调度用 `now > operate_time`（严格大于，`Envir.cs:2130`）；
  身份动作（走/跑/攻击）用 `now >= action_time`（`HumanObject.cs:121-158`）。

### 7.2 调度

| 必须保留 | 原因 |
|---|---|
| 玩家每 tick 处理、怪物最快 2000ms 一次 | 怪物 AI 的「反应时间」直接决定手感；若把怪物做成每 tick 处理，AI 行为频率会变化（攻击/搜索/漫游的时间戳都以 `Process` 为粒度推进） |
| `Map.Process()` 每 tick 全量执行 | 关门 5000ms、地图 `ActionList`（魔法分段伤害）、雷火特效都依赖于此 |
| 刷怪的「全局 tick 计数器」模型 | 老玩家的刷怪节奏与 `RespawnTicks`、在线人数系数绑定；改成按怪物个体计时会改变刷新密度 |
| `MapRespawn` 的「一次补齐到 Count×Multiplier」 | 死亡后不会逐个补，而是到点整批补 |
| 20ms 时间片的「延续式」对象轮转（`current` 跨迭代保留） | 保证大对象量时不会饿死尾部；Rust 端可用「按 OperateTime 排序的最小堆 + 每 tick 处理直到预算耗尽」等价实现 |

### 7.3 推荐结构（tokio）

```
[world task]  -- 单一 task 独占世界状态，避免锁
   loop {
       tokio::select! {
           _ = interval.tick() => world.tick(),          // 固定 16ms（或 20ms）
           Some(ev) = net_rx.recv() => world.on_client_event(ev),
           _ = shutdown.recv() => break,
       }
   }

world.tick():
   now = now_ms()
   1) drain 网络收包队列（等价 Envir.cs:2066-2087 的连接 Process）
   2) 处理玩家（每 tick 全部）→ 处理怪物/物品/法术/NPC（operate_time <= now）
   3) for map in maps: map.process(now)      // 刷怪、门、雷火、地图 ActionList
   4) 分档定时器：1s / 10s / 1min / 5min / 10min（§5.1）
   5) flush 广播（按 DataRange=16 的脏集合）
```

- **网络 I/O 独立 task**：每个连接一个读 task，读到包后 `mpsc::send` 给 world task；
  写侧可以是每连接的 `mpsc` + 独立 writer task（Crystal 是「连接对象自带 `_sendList`，主循环里批量 `BeginSend`」）。
- **不要**给每个怪物/法术开 tokio task：会破坏确定性顺序、放大调度开销，也与 Crystal 的「单线程对象轮转」语义不符。
- **确定性**：Crystal 的对象顺序 = `Envir.Objects`（`LinkedList`）的插入顺序（`MapObject.cs:344`），即**创建顺序**。
  Rust 端用 `Vec` + 稳定索引迭代（不要用 `HashMap` 迭代顺序）即可保持一致；随机数用**每对象独立的 RNG**更能保证复现性
  （Crystal 用的是全局 `RandomProvider`，见 `Envir.cs:100`）。
- **存档**：Crystal 每 5 分钟全量存档（`Envir.cs:2147-2155`，`Settings.SaveDelay = 5`，`Server/Settings.cs:102`）并先做 `.bak` 备份
  （`Envir.cs:2441-2452`）。Rust 端建议同样保留「定时全量 + 原子替换」。

---

## 8. 时间常量速查

| 名称 | 值 | 位置 |
|---|---|---|
| `Settings.Second` / `Minute` / `Hour` / `Day` | 1000 / 60000 / 3,600,000 / 86,400,000 ms | `Server/Settings.cs:16` |
| 连接超时 `Settings.TimeOut` | 10000 ms | `Settings.cs:67` |
| 存档间隔 `Settings.SaveDelay` | 5 分钟 | `Settings.cs:102` |
| 跑马灯 `LineMessageTimer` | 10 分钟 | `Settings.cs:99` |
| `MoveDelay`（走/跑/攻击/施法的动作预算） | 600 ms | `HumanObject.cs:186` |
| `TurnDelay` / `HarvestDelay` | 350 ms | `PlayerObject.cs:113` |
| `RegenDelay`（自然回复） | 10000 ms | `HumanObject.cs:186` |
| `PotDelay` / `HealDelay` / `VampDelay` | 200 / 600 / 500 ms | `HumanObject.cs:186` |
| `DuraDelay` / `ItemExpireDelay` / `LoyaltyDelay` | 10000 / 60000 / 1000 ms | `HumanObject.cs:186` |
| 移动后格子宽限 `CellTime` | 500 ms | `HumanObject.cs:2493` |
| 门的自动关闭 | 5000 ms | `Map.cs:659` |
| 地图雷/火间隔 | 3000–15000 ms | `Map.cs:669,701` |
| 安全区回血（法术对象 `TickSpeed=2000`, `Value=25`） | 2000 ms | `Map.cs:572-580` |
| 传送冷却 `MovementDelay` | 2000 ms | `PlayerObject.cs:113` |
| 怪物默认 `MoveSpeed` / `AttackSpeed` | 1800 / 2500 ms（下限 400） | `MonsterInfo.cs:32`、`MonsterObject.cs:829-830` |
| 刷怪基准 `BaseSpawnRate` | 20 分钟/tick | `RespawnTimer.cs:58` |
| 广播/视野 `Globals.DataRange` | 16 格（切比雪夫） | `Shared/Globals.cs:37` |
