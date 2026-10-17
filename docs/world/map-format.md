# `.map` 地图文件格式与服务器加载器

> 参考实现根目录：`C:\Users\diana\ref\Crystal`（下文所有 `路径:行号` 均相对该目录）。
> 实测样本：`resource/map/0.map`（5,880,052 字节）、`resource/map/0100.map`（3,292 字节）。
> 本文描述的是 **服务器端**（`Server/MirEnvir/Map.cs`）实际解析的格式；客户端 `Client/MirObjects/MapCode.cs` 的 `MapReader` 只用于渲染，在个别字节上与服务器不一致，已在 §4.4 单独标注。

---

## 1. 加载入口与流程

一条完整链路：数据库 `MapInfo` → `CreateMap()` → `new Map(info)` → `Map.Load()`。

| 步骤 | 代码位置 | 说明 |
|---|---|---|
| 逐条建图 | `Server/MirEnvir/Envir.cs:3341-3378` | 启动时遍历 `MapInfoList`，调用 `MapInfo.CreateMap()` |
| 建图 | `Server/MirDatabase/MapInfo.cs:197-215` | 先把属于该地图的 NPC 挂上，`new Map(this)`，`map.Load()` 失败则**不加入** `Envir.MapList`；成功则收集 `StartPoint` 安全区作为出生点 |
| 打开文件 | `Server/MirEnvir/Map.cs:432-439` | 文件名 = `Path.Combine(Settings.MapPath, Info.FileName + ".map")`；`Settings.MapPath` 默认 `.\Maps`（`Server/Settings.cs:20`）；一次性 `File.ReadAllBytes` |
| 版本识别 | `Server/MirEnvir/Map.cs:73-110` | `FindType(byte[])`，见 §2 |
| 单元格解析 | `Server/MirEnvir/Map.cs:440-469` | `switch (FindType(...))` 分派到 `LoadMapCellsv0/1/2/3/4/5/6/7/V100` |
| 加载后处理 | `Server/MirEnvir/Map.cs:471-503` | ① `GetWalkableCells()` 缓存所有 `Attribute == Walk` 的坐标（`Map.cs:516-527`）；② 每个 `RespawnInfo` 生成 `MapRespawn`，并把 `WalkableCells` 裁剪到 `Location ± Spread` 的方形范围内（`Map.cs:473-488`）；③ 放置 NPC（`AddObject(new NPCObject(info))`）；④ 建安全区（`Map.cs:529-590`）；⑤ 建矿区（`Map.cs:592-625`） |
| 失败处理 | `Server/MirEnvir/Map.cs:506-513` | 整个 `Load()` 包在 `try/catch` 里，异常被吞掉并转成 `MessageQueue` 文本，返回 `false`。**没有任何长度/边界校验**，文件被截断只会在循环中抛 `IndexOutOfRangeException` 然后整图加载失败 |

> 关键点：`Cells` 是 `Cell[Width, Height]`，而读文件时是 `for (x) for (y)` 双层循环（`Map.cs:123-151` 等），
> 因此 **单元格在文件中是「列优先」（column-major）排列**：第 `i` 个格子的起始偏移 = `header + i * BytesPerCell`，
> 其中 `i = x * Height + y`。Rust 实现必须保持同样的顺序，否则地图会整体错位。

---

## 2. 版本识别 `FindType`

`Server/MirEnvir/Map.cs:73-110`，按顺序短路匹配，全部不匹配则返回 `0`（老版 Mir2 格式，最常用）。

| 顺序 | 判定条件（`input` = 整个文件字节） | 返回 | 格式来源 |
|---|---|---|---|
| 1 | `input[2]==0x43 && input[3]==0x23`（`"C#"`，即 `0x43 0x23` 出现在偏移 2） | `100` | Crystal 自定义格式（`Map.cs:76-79`） |
| 2 | `input[0]==0` | `5` | Wemade Mir3：文件开头是空白字节，没有标题（`Map.cs:81-82`） |
| 3 | `input[0]==0x0F && input[5]==0x53 && input[14]==0x33` | `6` | Shanda Mir3，标题 `(C) SNDA, MIR3.`（`Map.cs:84-85`） |
| 4 | `input[0]==0x15 && input[4]==0x32 && input[6]==0x41 && input[19]==0x31` | `4` | Wemade AntiHack（迷宫图），标题 `Mir2 AntiHack`（`Map.cs:88-89`） |
| 5 | `input[0]==0x10 && input[2]==0x61 && input[7]==0x31 && input[14]==0x31` | `1` | Wemade 2010，标题 `Map 2010 Ver 1.0`（`Map.cs:92-93`） |
| 6 | `input[4]==0x0F` **或** (`input[4]==0x03 && input[18]==0x0D && input[19]==0x0A`) | `3` 或 `2` | Shanda 2012 / 老 Shanda。两者头相同，用文件大小区分（`Map.cs:96-104`）：`len > 52 + W*H*14` → `3`，否则 `2` |
| 7 | `input[0]==0x0D && input[1]==0x4C && input[7]==0x20 && input[11]==0x6D` | `7` | Mir3/4 Heroes（Myth/Lifcos）（`Map.cs:107-108`） |
| 8 | 其它 | `0` | 老版 Mir2 12 字节格式 |

两个注意事项：

1. **运算符优先级坑**：`Map.cs:96` 写成 `(input[4] == 0x0F) || (input[4] == 0x03) && (input[18] == 0x0D) && (input[19] == 0x0A)`。
   C# 中 `&&` 优先于 `||`，即等价于 `A || (B && C && D)`。单独的 `input[4] == 0x0F` 就足以命中该分支。
2. **`input[0]==0` 优先于默认分支**。老版 Mir2 格式把宽度放在偏移 0 的 `Int16`（小端），若宽度低字节为 0（宽度是 256 的倍数，如 256/512/768 或 0），会被误判成 Mir3 v5。Rust 实现建议**先按尺寸自洽性校验**（`len == 52 + 12*W*H`）再决定版本，避免误判。

---

## 3. 各版本结构总表

下表「字节/格」由代码中的 `offSet` 累加推导；`Map.cs` 注释里给出的总数与算术结果不一致处已注明。

| ver | 函数 | 单元格起始偏移 | 宽高来源 | 字节/格 | 墙体判定（读取位置） | 门字节偏移 | 光照字节偏移 |
|---|---|---|---|---|---|---|---|
| `0` | `LoadMapCellsv0` `Map.cs:112-152` | `52` | `Int16@0`=W，`Int16@2`=H | **12** | word@0 bit15 → `HighWall`；word@2 bit15 → `LowWall`；word@4 bit15 → `HighWall` | **8** | **11** |
| `1` | `LoadMapCellsv1` `Map.cs:154-193` | `54` | `Int16@21`=w，`Int16@23`=xor，`Int16@25`=h；`W=w^xor`，`H=h^xor` | 15 | `Int32@0`，异或 `0xAA38AA38` 后 bit29(0x20000000) → `HighWall`；`Int16@6` 异或 `xor` 后 bit15 → `LowWall` | 8 | 13 |
| `2` | `LoadMapCellsv2` `Map.cs:195-234` | `52` | `Int16@0`/`@2` | **14** | word@0 / word@2 / word@4，各取 bit15 | **6** | **11** |
| `3` | `LoadMapCellsv3` `Map.cs:236-274` | `52` | `Int16@0`/`@2` | **36** | word@0 / word@2 / word@4，各取 bit15 | **6** | 18 |
| `4` | `LoadMapCellsv4` `Map.cs:276-307` | `64` | `Int16@31`=w，`Int16@33`=xor，`Int16@35`=h；`W=w^xor`，`H=h^xor` | **12** | word@0 / word@2，各取 bit15 | **6** | 无（不读） |
| `5` | `LoadMapCellsv5` `Map.cs:309-335` | `28 + 3*((W/2)+(W%2))*(H/2)` | `Int16@22`/`@24` | 14 | `byte@0`：`bit0==0` → `HighWall`；`bit1==0` → `LowWall`；否则 `Walk` | 无 | 13 |
| `6` | `LoadMapCellsv6` `Map.cs:337-359` | `40` | `Int16@16`/`@18` | **20** | `byte@0`：`bit0==0` → `HighWall`；`bit1==0` → `LowWall` | 无 | 无 |
| `7` | `LoadMapCellsv7` `Map.cs:361-394` | `54` | `Int16@21`=W，`Int16@25`=H（注意 `offSet += 4`） | **15** | word@0、word@6，各取 bit15 | **8** | 12 |
| `100` | `LoadMapCellsV100` `Map.cs:396-430` | `8` | `Int16@4`=W，`Int16@6`=H；仅支持 `Bytes[0]==1 && Bytes[1]==0` | **26** | `Int32@2` bit29(0x20000000) → `HighWall`；`Int16@12` bit15 → `LowWall` | **14** | **25** |

补充：

- v5 的 `28 + 3*...` 那段偏移是为了跳过 Mir3 的「瓦片索引表」（3 字节 × 半个宽 × 半个高的格子）。
- v100 的前 4 字节是版本号 `01 00` + 保留，随后才是宽高（`Map.cs:398-406`）；`LoadMapCellsV100` 在版本不匹配时直接 `return`，此时 `Width/Height/Cells` 全为 0，`Load()` 仍会继续跑到 `GetWalkableCells()`（`Map.cs:471`），会因 `Cells == null` 抛异常并被捕获 → 地图加载失败。
- 只有 v0/v1/v2/v3/v4/v7 读「门」，v4 完全不读「光照」，v6 只读 1 个字节的墙体位图。

---

## 4. v0 单元格 12 字节布局（重点）

两个实测样本 `0.map` 与 `0100.map` 都是 **v0**（`FindType` 返回 0），这也是 Mir2 1.76 系地图的默认格式。

### 4.1 服务器实际读取的字段

`LoadMapCellsv0`（`Server/MirEnvir/Map.cs:112-152`），单元格起点记为 `o`：

| 单元格内偏移 | 大小 | 服务器行为 | 代码 |
|---|---|---|---|
| `o+0 .. o+1` | `u16` LE | `word & 0x8000 != 0` → `Cells[x,y] = Cell.HighWall`（注释：Can Fire Over） | `Map.cs:126-129` |
| `o+2 .. o+3` | `u16` LE | `word & 0x8000 != 0` → `Cells[x,y] = Cell.LowWall`（注释：Can't Fire Over） | `Map.cs:130-133` |
| `o+4 .. o+5` | `u16` LE | `word & 0x8000 != 0` → `Cells[x,y] = Cell.HighWall`（注释：No Floor Tile） | `Map.cs:135-136` |
| `o+6 .. o+7` | `u16` | **跳过，不读**（`offSet += 4` 覆盖 `o+4..o+7`） | `Map.cs:140` |
| `o+8` | `u8` | `> 0` → `DoorIndex[x,y] = AddDoor(byte & 0x7F, (x,y))` | `Map.cs:142-145` |
| `o+9 .. o+10` | 2 字节 | **跳过，不读**（`offSet += 3` 覆盖 `o+8..o+10`） | `Map.cs:145` |
| `o+11` | `u8` | `100 <= v <= 119` → `Cells[x,y].FishingAttribute = v - 100`；其余值忽略 | `Map.cs:147-150` |

判定顺序 = 覆盖顺序（后写的胜出）：

```
默认 Walk  →  word@0 bit15 → HighWall  →  word@2 bit15 → LowWall  →  word@4 bit15 → HighWall
```

即：只要 `o+4..o+5` 的 bit15 被置位，最终结果一定是 `HighWall`；只有前两个都不置位、且 `o+4` 未置位时，格子才是 `Walk`。
`if (Cells[x, y] == null) Cells[x, y] = new Cell { Attribute = CellAttribute.Walk };` 出现在第三次判定之前（`Map.cs:138`），
所以实际语义等价于「三个 bit15 全为 0 → Walk」。

### 4.2 `Cell` 与 `CellAttribute`

`Server/MirEnvir/Map.cs:2475-2531`：

```csharp
public enum CellAttribute : byte { Walk = 0, HighWall = 1, LowWall = 2 }   // Shared/Enums.cs:802-807
public class Cell {
    public List<MapObject> Objects;      // 该格上的所有对象（玩家/怪物/物品/法术）
    public CellAttribute Attribute;
    public sbyte FishingAttribute = -1;  // 由 light 字节 100..119 推导
    public bool Valid => Attribute == CellAttribute.Walk;   // Map.cs:2480-2483
}
```

> **重要**：在当前 Crystal 服务器里 `HighWall` 与 `LowWall` **没有任何行为差异**——全仓库只有 `Attribute == Walk` 被使用
> （`Map.cs:524`、`Map.cs:2480-2483`）。注释里的 "Can Fire Over / Can't Fire Over" 只是从旧代码继承下来的语义说明，
> 未参与任何射线/穿越判定（`MapObject.CanFly` 也只查 `Valid`，见 `Server/MirObjects/MapObject.cs:321-337`）。
> Rust 实现可以只剩一个 `blocked: bool`，但建议保留 `WallKind` 字段以便将来做高低墙的视线判定。

`Map.ValidPoint` 同时负责边界与可走判定（`Map.cs:637-644`）：

```csharp
public bool ValidPoint(Point p) => p.X >= 0 && p.X < Width && p.Y >= 0 && p.Y < Height && GetCell(p).Valid;
```

### 4.3 其余字节（服务器不解释，供渲染/工具使用）

客户端 `MapReader`（`Client/MirObjects/MapCode.cs:226-268`）对同样的 12 字节给出如下字段名：

```
o+0..1  BackImage            (u16，bit15 保留为「无地板/不可走」标记)
o+2..3  MiddleImage          (u16)
o+4..5  FrontImage           (u16，bit15 = 「无地板瓦片」)
o+6     DoorIndex  (& 0x7F)  ← 与服务器不一致，见 §4.4
o+7     DoorOffset
o+8     FrontAnimationFrame  ← 与服务器不一致，见 §4.4
o+9     FrontAnimationTick
o+10    FrontIndex (+2 后作为瓦片库序号)
o+11    Light
```

客户端把 `BackImage` 的 bit15 转换成 `0x20000000`（`MapCode.cs:254-255`），并在 `EmptyCell` 里用
`(BackImage & 0x20000000) != 0 || (FrontImage & 0x8000) != 0` 判断不可行走（`Client/MirScenes/GameScene.cs:12074-12084`），
这与服务器「word@0 bit15 或 word@4 bit15 即阻挡」的判断**完全一致**。

`Light` 字节在客户端被当作光照/钓鱼标记（`MapCode.cs:257-258`、`GameScene.cs:11209-11249`）；
服务器只用它算 `FishingAttribute`（`Map.cs:147-150`）。每格光照强度（0..255 的渐变）服务器不参与任何计算。

### 4.4 门字节的偏移：服务器 `o+8` vs 客户端 `o+6`

同一份代码里 **v2 / v3 / v4 读门都取 `o+6`**（`Map.cs:223-224`、`Map.cs:263-264`、`Map.cs:303-304`），
只有 **v0 取 `o+8`**（`Map.cs:142-143`）；而客户端 `LoadMapType0` 取 `o+6`（`MapCode.cs:248-249`）。
四者互相矛盾，实测数据支持**服务器的 `o+8`**：

| 证据 | 结论 |
|---|---|
| `resource/map/e702.map`：`o+8` 非零的 125 个格子里，纵向 1 格宽的走廊（`x=12` 那一列）在 `y=20` 处正好有一个门格，上下都是通道 | `o+8` = 门索引 |
| `resource/map/GA0.map`：`o+8` 非零的 123 个格子沿地面/墙体边界排成一条竖线 | `o+8` = 门索引 |
| `resource/map/0.map`：`o+8` 非零的 42 个格子中，40 个格子的 12 字节内容完全相同（`.. .. .. .. a4 0a 00 00 8a 00 00 05`，即 `FrontImage=0x0AA4`、`o+11=5`），且这些格子全部是**可走格** | 门是画在「前层」的图（可以从门框下走过），符合 Mir2 门的渲染方式 |
| `resource/map/0.map`：`o+6` 非零的 198 个格子聚成一团，且全部落在**厚实墙体内部**（`word@0` bit15 置位） | `o+6` 在可走性上没有意义，不是服务器用的门字节 |

结论：**Rust 端按 v0 的 `o+8` 取门**，与服务器行为一致；`o+6/o+7/o+9/o+10` 可以只做保留字段。
（若将来要让客户端渲染对齐，建议把两套偏移都解析出来，用 `o+8` 做服务端逻辑。）

### 4.5 门对象

`Server/MirEnvir/Map.cs:50-71`：

```csharp
public Door AddDoor(byte DoorIndex, Point location) {
    DoorIndex &= 0x7F;                      // bit7 被丢弃
    for (...) if (Doors[i].index == DoorIndex) return Doors[i];   // 同索引复用同一个 Door
    ...
}
```

- `Doors` 是 `List<Door>`，`DoorIndex[x,y]` 是指向它的二维引用表；同索引的所有格共享一个 `Door`。
- 因此原始字节 `0x8A` → 索引 `0x0A`（10）；`0x80` → 索引 **0**（`0.map` 中有 2 格这样的数据，会生成 index 0 的假门）。
- 通行判定 `CheckDoorOpen`（`Map.cs:646-651`）：格子上没有门 → 可通行；有门且 `DoorState != Open` → 不可通行。
- 门在 `Map.Process` 中自动关闭：`DoorState == Open && LastTick + 5000 < Envir.Time` → 置 0 并广播 `S.Opendoor{Close=true}`（`Map.cs:657-665`）。

---

## 5. Light 相关：`.map` 里没有全局光照

`LightSetting` 属于 **数据库里的 `MapInfo`**，不是地图文件的字段：

```csharp
public enum LightSetting : byte { Normal = 0, Dawn = 1, Day = 2, Evening = 3, Night = 4 }  // Shared/Enums.cs:809-816
public LightSetting Light;          // Server/MirDatabase/MapInfo.cs:21，序列化于 :53（1 字节）
public byte MapDarkLight = 0;       // Server/MirDatabase/MapInfo.cs:22
```

三条相互独立的「光」：

1. **地图自带光照** `MapInfo.Light`（0..4）：登录/切图时通过 `S.MapInformation` 下发
   （字段 `Lights`，`Shared/ServerPackets.cs:480,492`；发送处 `Server/MirObjects/PlayerObject.cs:1710-1726`，另有 `HumanObject.cs:1713`、`PlayerObject.cs:1434/4574`）。
   客户端收到 `Normal` 时改用全局昼夜值（`Client/MirScenes/GameScene.cs:10589-10591`）。
2. **全局昼夜** `Envir.Lights`：由真实时间驱动，`AdjustLights()`（`Server/MirEnvir/Envir.cs:2290-2307`）
   取 `Now.Hour * 2 % 24`：`6,7` → `Dawn`；`8..15` → `Day`；`16,17` → `Evening`；其余 → `Night`。
   变化时广播 `S.TimeOfDay{Lights}`（id 61，`Shared/ServerPackets.cs:2071-2089`）。
   `Envir.Now = _startTime(UTC) + Envir.Time`（`Envir.cs:72,85`），即「服务器启动时刻 + 运行毫秒数」。
3. **每格 light 字节**（v0 的 `o+11`）：服务器只把它当钓鱼属性——
   `100..119` → `FishingAttribute = v - 100`（`Map.cs:147-150`），供钓鱼点判定使用
   （`Server/MirObjects/PlayerObject.cs:10990`、`:11114` 按 `FishingAttribute` 过滤 `Envir.FishingDrops`）。
   其余取值（含 `0`）在服务器侧完全无意义。

`MapInfo` 里与「地图层」相关的其它字段：`Lightning`/`LightningDamage`、`Fire`/`FireDamage`、`WeatherParticles`、
`MineIndex`、`SafeZones`、`Movements`、`Respawns`、`ActiveCoords`（`Server/MirDatabase/MapInfo.cs:18-40`）。

---

## 6. 实测：两个真实文件

样本解析脚本按 §4.1 的规则逐格读取（列优先），并做长度自洽校验。

### 6.1 `0100.map`（3,292 字节）

| 项 | 值 |
|---|---|
| 文件大小 | `3292` |
| 头部 | `0F 00 12 00 | 0D "Legend of mir" 00 00 | ...` |
| `FindType` | `0`（`input[0]=0x0F≠0`；`input[4]=0x0D` 不是 `0x0F/0x03`；不命中 v1/v4/v5/v6/v7/v100） |
| 宽 × 高 | **W = 15**（`Int16@0 = 0x000F`），**H = 18**（`Int16@2 = 0x0012`） |
| 公式校验 | `52 + 12 * 15 * 18 = 52 + 3240 = 3292` ✔ 与文件大小严格相等 |
| 可走 / 高墙 / 低墙 | `47 / 223 / 0`（`word@0` bit15 命中 223 格；`word@2` bit15 命中 0 格；`word@4` bit15 命中 223 格，全部与前者重合） |
| 门 | 0 个（`o+8` 全为 0） |
| light 字节 | `0` × 269，`5` × 1 |
| `o+6/o+7/o+9/o+10` | 全为 0 |

抽样（`bytes` 为该格 12 字节原样）：

```
(0,0)  00 80 00 00 00 80 00 00 00 00 00 00  w1=0x8000* w2=0x0000 w3=0x8000*  light=0   → HighWall
(13,10)00 80 00 00 00 80 00 00 00 00 00 00  w1=0x8000* w3=0x8000*         light=0   → HighWall
(7,6)  00 00 00 00 3C 0E 00 00 00 00 00 00  w1=0x0000  w3=0x0E3C          light=0   → Walk
(4,10) 00 00 00 00 30 0E 00 00 00 00 00 00  w1=0x0000  w3=0x0E30          light=0   → Walk
(8,11) 00 00 00 00 4A 0E 00 00 00 00 00 00  w1=0x0000  w3=0x0E4A          light=0   → Walk
(9,11) 00 00 00 00 51 0E 00 00 00 00 00 00  w1=0x0000  w3=0x0E51          light=0   → Walk
```
（`*` = bit15 置位。）可见该图的可走格 `BackImage = 0`、`FrontImage` 是 3644~3666 之间的前层图号；墙体格两层都是 `0x8000`。

整图（x 向右，y 向下；`#`=HighWall，`.`=Walk）：

```
      000000000011111
      012345678901234
   0 ###############
   1 ###############
   2 ###############
   3 ###############
   4 ###############
   5 ###############
   6 #######.#######
   7 ######...######
   8 #####....######
   9 ####......#####
  10 ###..........##
  11 ###...##....###
  12 #######.....###
  13 ######......###
  14 ######...##.###
  15 #######.#######
  16 ###############
  17 ###############
```

### 6.2 `0.map`（5,880,052 字节）

| 项 | 值 |
|---|---|
| 文件大小 | `5880052` |
| 头部 | `BC 02 BC 02 | 0D "Legend of mir" 00 00 | ...` |
| `FindType` | `0` |
| 宽 × 高 | **W = 700**（`Int16@0 = 0x02BC`），**H = 700**（`Int16@2 = 0x02BC`） |
| 公式校验 | `52 + 12 * 700 * 700 = 52 + 5,880,000 = 5,880,052` ✔ 严格相等 |
| 可走 / 高墙 / 低墙 | `327,180 / 162,820 / 0`（`word@0` bit15 命中 162,820；`word@2` bit15 命中 0；`word@4` bit15 命中 109） |
| 门（`o+8` 非零） | 42 格；取值分布 `0x8A`×40、`0x80`×2 → `AddDoor` 后得到索引 10 的门 ×40 格、索引 0 的门 ×2 格 |
| light 字节 | `0` × 489,915，`5` × 85 |
| `word@1` 取值 Top | `0x0000`×115,674、`0x8000`×110,582、`0x05AE`×15,192、`0x05AB`×15,100、`0x05AC`×14,932、`0x05AF`×14,801 |
| `word@2` 取值 Top | `0x0000`×480,653、`0x0040`×204、`0x0042`×184、`0x0044`×178 |
| `word@3` 取值 Top | `0x0000`×461,294、`0x0583`×444、`0x057A`×443、`0x057B`×443 |
| `o+6` 取值 Top | `0`×489,802、`0x80`×16、`0x37`×15、`0x3C`×12、`0xAD`×10、`0xBD`×9（服务器不读） |
| `o+7` 取值 Top | `0`×489,803、`0x1B`×28、`0x07`×25、`0x04`×20、`0x05`×19、`0x2D`×13（服务器不读） |
| `o+9` | 全为 0 |
| `o+10` 取值 Top | `0`×489,411、`0x05`×385、`0x03`×129、`0x04`×75（服务器不读） |

抽样：

```
(0,0)      3C 87 00 00 00 00 00 00 00 00 00 00  w1=0x873C* w3=0x0000      → HighWall（后层图号 0x073C=1852）
(0,1)      00 80 00 00 00 00 00 00 00 00 00 00  w1=0x8000*               → HighWall（纯墙、无图）
(265,261)  57 03 00 00 A4 0A 00 00 8A 00 00 05  w1=0x0357  w3=0x0AA4 门=0x8A light=5 → Walk + 门(索引 10)
(320,600)  02 80 00 00 00 00 00 00 00 00 00 00  w1=0x8002*               → HighWall
(699,699)  00 80 00 00 00 00 00 00 00 00 00 00  w1=0x8000*               → HighWall（地图右下角仍在墙里）
```

左上角 80×24 区域（`#`=HighWall，`.`=Walk）：

```
      00000000001111111111222222222233333333334444444444555555555566666666667777777777
      01234567890123456789012345678901234567890123456789012345678901234567890123456789
   0 ################################################################################
   1 ################################################################################
   2 ################################################################################
   3 ################################################################################
   4 ################################################################################
   5 ################################################################################
   6 ################################################################################
   7 ################################################################################
   8 ################################################################################
   9 ##################################################################..############
  10 ################################################################.....###########
  11 ################################################################......##########
  12 ################################################################......##########
  13 #############################################################............#######
  14 ############################################################.........#....######
  15 ###########################################################........###.....#####
  16 ##########################################################.......#####......####
  17 #########################################################.......#####........###
  18 ########################################################.....########.........##
  19 #####################################################.......##########........##
  20 ####################################################........##########..........
  21 ###################################################.........#######........#....
  22 ##################################################...........#####.........#....
  23 #################################################.........#..####..............#
```

---

## 7. Rust 实现要点

```rust
// 读整个文件
const HEADER_V0: usize = 52;
const CELL_V0: usize = 12;

pub struct MapCell {
    pub blocked: bool,        // word@0 bit15 | word@4 bit15
    pub wall_kind: WallKind,  // None / High / Low（Low 由 word@2 bit15 得到，当前无行为差异）
    pub door: Option<u8>,     // byte@8 & 0x7F（原始 > 0 才建门）
    pub fishing: i8,          // byte@11 ∈ [100,119] → v-100，否则 -1
    // 仅保留、不参与逻辑：
    pub back_image: u16, pub mid_image: u16, pub front_image: u16,
    pub b6: u8, pub b7: u8, pub b9: u8, pub b10: u8,
}
```

必须遵守的几点：

1. **尺寸校验**：`len == 52 + 12 * W * H`（v0）。Crystal 没有校验，靠异常兜底；Rust 应显式报错并拒绝加载该图。
2. **列优先**：`i = x * H + y`，`for x { for y { ... } }`。写成行优先会导致整图错位。
3. **判定顺序**：`blocked = (w0 & 0x8000) || (w4 & 0x8000)`；`low = (w2 & 0x8000)`。注意 `w4` 的覆盖语义：
   若 `w0` 与 `w4` 同时置位，服务器会先写 `HighWall` 再写一次 `HighWall`，结果仍是 `HighWall`，因此「或」逻辑等价。
4. **门索引掩码**：`raw & 0x7F`，且 `raw > 0` 才建门 —— `0x80` 会造出索引 0 的门（`0.map` 实测 2 格），要保持一致。
5. **门表**：`DoorIndex: Vec<Option<DoorId>>` 或 `HashMap<(u16,u16), DoorId>`，同索引复用同一 `Door`（`Map.cs:53-55`）。
6. **v1..v100 暂不需要**：`resource/map` 下 1113 个文件中，绝大多数是 v0；建议先只实现 v0 + v2/v3（14/36 字节）三条路径，
   其余版本在加载时明确报「不支持」而不是静默返回空图。
