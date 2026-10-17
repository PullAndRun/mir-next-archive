# Crystal 协议：复杂数据包（ReadPacket 非纯线性）

> 参照实现：`C:\Users\diana\ref\Crystal`（只读）。本文件是 Rust 重写的实现契约。
> 帧格式与 `ReceivePacket`/`GetPacketBytes` 语义见 [framing.md](framing.md)；全部数据包编号见 [packet-ids.md](packet-ids.md)。

## 0. 判定标准与统计

判定依据是各包 `protected override void ReadPacket(BinaryReader reader)` 的方法体。凡是方法体**不是**纯线性 `field = reader.ReadXxx();` 语句序列的，都算“复杂”，即包含：

- 循环（`for` / `foreach` / `while`），尤其是 `count` 前缀的变长数组；
- 条件分支（`if`、`switch`、提前 `return`）；
- `reader.ReadBytes(<expression>)`；
- 任何 helper 调用（`new UserItem(reader)`、`new Point(...)`、`Color.FromArgb(...)`、`DateTime.FromBinary(...)` 等）。

只做 `field = reader.ReadXxx();` 的**线性包不在此逐条列出**，数量统计如下：

| 方向 | 包总数 | 复杂包 | 线性包 | 其中空负载（只有包头） |
| --- | --- | --- | --- | --- |
| Server→Client | 279 | **103** | **176** | 25 |
| Client→Server | 153 | **11** | **142** | 22 |

> 计数口径：以 `ServerPacketIds` / `ClientPacketIds` 的成员为准（每个成员恰好对应一个 `Packet` 子类，已逐一核对无缺漏）。
> 线性包中仍有一批是**空负载包**：`ReadPacket` 为空、线上只有 4 字节包头（2 字节长度 + 2 字节编号），Rust 侧应能正确解析长度 = 4 的包。

复杂包分三个子类（同一包同时满足多项时取最高者）：

- **A**：含循环 / 条件 / 变长 / `ReadBytes` —— 必须真正实现解析逻辑；
- **B**：单一 helper 构造（`new Xxx(reader)`），只需复用该结构体的读取函数；
- **C**：没有 helper 对象，只是把 `Point` / `Color` / `DateTime` 这些 BCL 值类型拆成定长小端整数（或继承父包布局），逐字段线性即可。

嵌套 helper 结构的读取函数位置（Rust 需按同名 `read` 实现）：

| 结构 | 读取函数位置 |
| --- | --- |
| `BaseStats` | `Shared/BaseStats.cs:102` |

> **版本兼容陷阱**：`ItemInfo`、`UserItem`、`ItemRentalInformation` 的读取构造函数都带 `int version = int.MaxValue, int customVersion = int.MaxValue` 参数（用于兼容旧存档格式）。数据包内一律写成 `new ItemInfo(reader)` / `new UserItem(reader)`，即**使用默认值 `int.MaxValue`**，因此走的永远是"最新版本"分支（例如 `Shared/Data/ItemData.cs:88-95` 的 `if (version <= 84) ... else ...` 会走 `else`）。Rust 实现只需实现最新分支，不要照抄版本兼容代码。
| `ChatItem` | `Shared/Data/ItemData.cs:1446` `ChatItem(BinaryReader)` |
| `ClientAuction` | `Shared/Data/ClientData.cs:218` |
| `ClientBuff` | `Shared/Data/ClientData.cs:593` |
| `ClientFriend` | `Shared/Data/ClientData.cs:133` |
| `ClientGTMap` | `Shared/Data/SharedData.cs:155` |
| `ClientHeroInformation` | `Shared/Data/ClientData.cs:645` |
| `ClientIntelligentCreature` | `Shared/Data/IntelligentCreatureData.cs:181` |
| `ClientMagic` | `Shared/Data/ClientData.cs:19` `ClientMagic(BinaryReader)` |
| `ClientMail` | `Shared/Data/ClientData.cs:168` |
| `ClientMapInfo` | `Shared/Data/ClientData.cs:331` |
| `ClientMonsterInfo` | `Shared/Data/MonsterData.cs:23` |
| `ClientNPCInfo` | `Shared/Data/ClientData.cs:283` |
| `ClientQuestInfo` | `Shared/Data/ClientData.cs:395` |
| `ClientQuestProgress` | `Shared/Data/ClientData.cs:546` |
| `ClientRecipeInfo` | `Shared/Data/ClientData.cs:82` |
| `GameShopItem` | `Shared/Data/ItemData.cs:829` `GameShopItem(BinaryReader, bool packet)` |
| `GuildBuff` | `Shared/Data/GuildData.cs:231` |
| `GuildBuffInfo` | `Shared/Data/GuildData.cs:109` |
| `GuildRank` | `Shared/Data/GuildData.cs:10` `GuildRank(BinaryReader, bool offline)` |
| `GuildStorageItem` | `Shared/Data/GuildData.cs:50` |
| `ItemInfo` | `Shared/Data/ItemData.cs:69` `ItemInfo(BinaryReader)` |
| `ItemRentalInformation` | `Shared/Data/ItemData.cs:1099` `ItemRentalInformation(BinaryReader)` |
| `Notice` | `Shared/Data/Notice.cs:9` |
| `RankCharacterInfo` | `Shared/Data/SharedData.cs:58` |
| `SelectInfo` | `Shared/Data/SharedData.cs:14` |
| `UserItem` | `Shared/Data/ItemData.cs:338` `UserItem(BinaryReader)` |
| `WorldMapSetup` | `Shared/Data/SharedData.cs:102` |

## 1. Server → Client 复杂包（103 个，按 Index 升序）

| Index | Hex | 包名 | 类别 |
| --- | --- | --- | --- |
| 6 | 0x0006 | `ChangePasswordBanned` | C |
| 8 | 0x0008 | `LoginBanned` | C |
| 9 | 0x0009 | `LoginSuccess` | A |
| 11 | 0x000B | `NewCharacterSuccess` | B |
| 15 | 0x000F | `StartGameBanned` | C |
| 17 | 0x0011 | `MapInformation` | A |
| 18 | 0x0012 | `NewMapInfo` | B |
| 19 | 0x0013 | `WorldMapSetupInfo` | B |
| 21 | 0x0015 | `UserInformation` | A |
| 22 | 0x0016 | `UserSlotsRefresh` | A |
| 23 | 0x0017 | `UserLocation` | B |
| 24 | 0x0018 | `ObjectPlayer` | A |
| 25 | 0x0019 | `ObjectHero` | C |
| 27 | 0x001B | `ObjectTurn` | B |
| 28 | 0x001C | `ObjectWalk` | B |
| 29 | 0x001D | `ObjectRun` | B |
| 32 | 0x0020 | `NewItemInfo` | B |
| 33 | 0x0021 | `NewMonsterInfo` | B |
| 34 | 0x0022 | `NewNPCInfo` | B |
| 35 | 0x0023 | `NewHeroInfo` | B |
| 36 | 0x0024 | `NewChatItem` | B |
| 44 | 0x002C | `SplitItem` | A |
| 57 | 0x0039 | `PlayerInspect` | A |
| 58 | 0x003A | `LogOutSuccess` | A |
| 64 | 0x0040 | `ObjectItem` | B |
| 65 | 0x0041 | `ObjectGold` | B |
| 66 | 0x0042 | `GainedItem` | B |
| 71 | 0x0047 | `ObjectMonster` | A |
| 72 | 0x0048 | `ObjectAttack` | B |
| 74 | 0x004A | `ObjectStruck` | B |
| 80 | 0x0050 | `Death` | B |
| 81 | 0x0051 | `ObjectDied` | B |
| 82 | 0x0052 | `ColourChanged` | C |
| 83 | 0x0053 | `ObjectColourChanged` | C |
| 90 | 0x005A | `ObjectHarvest` | B |
| 91 | 0x005B | `ObjectHarvested` | B |
| 92 | 0x005C | `ObjectNPC` | A |
| 93 | 0x005D | `NPCResponse` | A |
| 98 | 0x0062 | `MapChanged` | B |
| 102 | 0x0066 | `NPCGoods` | A |
| 116 | 0x0074 | `ItemSealChanged` | C |
| 117 | 0x0075 | `NewMagic` | B |
| 120 | 0x0078 | `Magic` | A |
| 123 | 0x007B | `ObjectMagic` | A |
| 126 | 0x007E | `RangeAttack` | B |
| 127 | 0x007F | `Pushed` | B |
| 128 | 0x0080 | `ObjectPushed` | B |
| 130 | 0x0082 | `UserStorage` | A |
| 141 | 0x008D | `MapEffect` | B |
| 143 | 0x008F | `ObjectRangeAttack` | B |
| 144 | 0x0090 | `AddBuff` | B |
| 148 | 0x0094 | `RefreshItem` | B |
| 149 | 0x0095 | `ObjectSpell` | B |
| 150 | 0x0096 | `UserDash` | B |
| 151 | 0x0097 | `ObjectDash` | B |
| 152 | 0x0098 | `UserDashFail` | B |
| 153 | 0x0099 | `ObjectDashFail` | B |
| 155 | 0x009B | `NPCMarket` | A |
| 156 | 0x009C | `NPCMarketPage` | A |
| 160 | 0x00A0 | `ObjectSitDown` | B |
| 162 | 0x00A2 | `BaseStatsInfo` | B |
| 163 | 0x00A3 | `HeroBaseStatsInfo` | B |
| 165 | 0x00A5 | `ChatItemStats` | B |
| 166 | 0x00A6 | `GuildNoticeChange` | A |
| 167 | 0x00A7 | `GuildMemberChange` | A |
| 173 | 0x00AD | `GuildStorageItemChange` | A |
| 174 | 0x00AE | `GuildStorageList` | A |
| 176 | 0x00B0 | `HeroCreateRequest` | A |
| 178 | 0x00B2 | `HeroInformation` | A |
| 184 | 0x00B8 | `ManageHeroes` | A |
| 188 | 0x00BC | `NPCImageUpdate` | C |
| 195 | 0x00C3 | `TradeItem` | A |
| 200 | 0x00C8 | `FishingUpdate` | B |
| 201 | 0x00C9 | `ChangeQuest` | B |
| 202 | 0x00CA | `CompleteQuest` | A |
| 204 | 0x00CC | `NewQuestInfo` | B |
| 205 | 0x00CD | `GainedQuestItem` | B |
| 209 | 0x00D1 | `UserBackStep` | B |
| 210 | 0x00D2 | `ObjectBackStep` | B |
| 211 | 0x00D3 | `UserDashAttack` | B |
| 212 | 0x00D4 | `ObjectDashAttack` | B |
| 213 | 0x00D5 | `UserAttackMove` | B |
| 215 | 0x00D7 | `ItemUpgraded` | B |
| 219 | 0x00DB | `ObjectDeco` | B |
| 228 | 0x00E4 | `AwakeningNeedMaterials` | A |
| 231 | 0x00E7 | `ReceiveMail` | A |
| 238 | 0x00EE | `ResizeStorage` | C |
| 239 | 0x00EF | `NewIntelligentCreature` | B |
| 240 | 0x00F0 | `UpdateIntelligentCreatureList` | A |
| 243 | 0x00F3 | `NPCPearlGoods` | A |
| 245 | 0x00F5 | `FriendUpdate` | A |
| 246 | 0x00F6 | `LoverUpdate` | C |
| 248 | 0x00F8 | `GuildBuffList` | A |
| 250 | 0x00FA | `GameShopInfo` | B |
| 252 | 0x00FC | `Rankings` | A |
| 254 | 0x00FE | `GetRentedItems` | A |
| 260 | 0x0104 | `UpdateRentalItem` | A |
| 266 | 0x010A | `NewRecipeInfo` | B |
| 271 | 0x010F | `UpdateNotice` | B |
| 273 | 0x0111 | `SetCompass` | C |
| 275 | 0x0113 | `SendMemberLocation` | B |
| 276 | 0x0114 | `GuildTerritoryPage` | A |
| 278 | 0x0116 | `StoragePasswordResult` | C |

### `ChangePasswordBanned`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 6 (`0x0006`) |
| 类别 | C（仅 BCL 值类型转换，逐字段线性） |
| 源 | `Shared/ServerPackets.cs:156` |

```csharp
Reason = reader.ReadString();
ExpiryDate = DateTime.FromBinary(reader.ReadInt64());
```

**Rust 要点**：`string Reason` + i64（`DateTime.ToBinary()`：低 62 位是 ticks，高 2 位是 Kind）。

### `LoginBanned`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 8 (`0x0008`) |
| 类别 | C（仅 BCL 值类型转换，逐字段线性） |
| 源 | `Shared/ServerPackets.cs:269` |

```csharp
Reason = reader.ReadString();
ExpiryDate = DateTime.FromBinary(reader.ReadInt64());
```

**Rust 要点**：`string Reason` + i64（`DateTime.ToBinary()`：低 62 位是 ticks，高 2 位是 Kind）。

### `LoginSuccess`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 9 (`0x0009`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:290` |

```csharp
int count = reader.ReadInt32();

for (int i = 0; i < count; i++)
    Characters.Add(new SelectInfo(reader));
```

**Rust 要点**：先读 `i32 数量`，再循环读 `SelectInfo`（`Shared/Data/SharedData.cs:14`：`i32 Index`、`string Name`、`u16 Level`、`u8 Class`、`u8 Gender`、`DateTime LastAccess`），单项长度随名字变化，不能按定长处理。

### `NewCharacterSuccess`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 11 (`0x000B`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:344` |

```csharp
CharInfo = new SelectInfo(reader);
```

**Rust 要点**：读一个 `SelectInfo`（`Shared/Data/SharedData.cs:14`）。

### `StartGameBanned`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 15 (`0x000F`) |
| 类别 | C（仅 BCL 值类型转换，逐字段线性） |
| 源 | `Shared/ServerPackets.cs:436` |

```csharp
Reason = reader.ReadString();
ExpiryDate = DateTime.FromBinary(reader.ReadInt64());
```

**Rust 要点**：`string Reason` + i64（`DateTime.ToBinary()`：低 62 位是 ticks，高 2 位是 Kind）。

### `MapInformation`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 17 (`0x0011`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:484` |

```csharp
MapIndex = reader.ReadInt32();
FileName = reader.ReadString();
Title = reader.ReadString();
MiniMap = reader.ReadUInt16();
BigMap = reader.ReadUInt16();
Lights = (LightSetting)reader.ReadByte();
byte bools = reader.ReadByte();
if ((bools & 0x01) == 0x01) Lightning = true;
if ((bools & 0x02) == 0x02) Fire = true;
MapDarkLight = reader.ReadByte();
Music = reader.ReadUInt16();
WeatherParticles = (WeatherSetting)reader.ReadUInt16();
```

**Rust 要点**：读完常规字段后读 1 字节 `bools` 位掩码：bit0 = Lightning、bit1 = Fire，其余位丢弃；Rust 侧 `let b = u8; lightning = b & 1 != 0; fire = b & 2 != 0;`，切勿写成两个独立 bool 字节。

### `NewMapInfo`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 18 (`0x0012`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:528` |

```csharp
MapIndex = reader.ReadInt32();
Info = new ClientMapInfo(reader);
```

**Rust 要点**：`MapIndex:i32` 后接 `ClientMapInfo`（`Shared/Data/ClientData.cs:331`：`string Title`、`i32 Width/Height/BigMap`，然后 `i32 数量 × ClientMovementInfo`、再 `i32 数量 × ClientNPCInfo`——**两段循环各自带 count**）。

### `WorldMapSetupInfo`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 19 (`0x0013`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:551` |
| 命名 | 枚举成员是 `WorldMapSetup`，类名是 `WorldMapSetupInfo` |

```csharp
Setup = new WorldMapSetup(reader);
TeleportToNPCCost = reader.ReadInt32();
```

**Rust 要点**：`WorldMapSetup`（`Shared/Data/SharedData.cs:102`，内部是图标列表循环）+ `TeleportToNPCCost:i32`。

### `UserInformation`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 21 (`0x0015`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:631` |

```csharp
ObjectID = reader.ReadUInt32();
RealId = reader.ReadUInt32();
Name = reader.ReadString();
GuildName = reader.ReadString();
GuildRank = reader.ReadString();
NameColour = Color.FromArgb(reader.ReadInt32());
Class = (MirClass)reader.ReadByte();
Gender = (MirGender)reader.ReadByte();
Level = reader.ReadUInt16();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
Hair = reader.ReadByte();
HP = reader.ReadInt32();
MP = reader.ReadInt32();

Experience = reader.ReadInt64();
MaxExperience = reader.ReadInt64();

LevelEffects = (LevelEffects)reader.ReadUInt16();
HasHero = reader.ReadBoolean();
HeroBehaviour = (HeroBehaviour)reader.ReadByte();

if (reader.ReadBoolean())
{
    Inventory = new UserItem[reader.ReadInt32()];
    for (int i = 0; i < Inventory.Length; i++)
    {
        if (!reader.ReadBoolean()) continue;
        Inventory[i] = new UserItem(reader);
    }
}

if (reader.ReadBoolean())
{
    Equipment = new UserItem[reader.ReadInt32()];
    for (int i = 0; i < Equipment.Length; i++)
    {
        if (!reader.ReadBoolean()) continue;
        Equipment[i] = new UserItem(reader);
    }
}

if (reader.ReadBoolean())
{
    QuestInventory = new UserItem[reader.ReadInt32()];
    for (int i = 0; i < QuestInventory.Length; i++)
    {
        if (!reader.ReadBoolean()) continue;
        QuestInventory[i] = new UserItem(reader);
    }
}

Gold = reader.ReadUInt32();
Credit = reader.ReadUInt32();

HasExpandedStorage = reader.ReadBoolean();
HasStoragePassword = reader.ReadBoolean();
RequireStoragePassword = reader.ReadBoolean();
StoragePasswordLastSet = DateTime.FromBinary(reader.ReadInt64());
ExpandedStorageExpiryTime = DateTime.FromBinary(reader.ReadInt64());

int count = reader.ReadInt32();

for (int i = 0; i < count; i++)
{
    Magics.Add(new ClientMagic(reader));
}

count = reader.ReadInt32();
for (int i = 0; i < count; i++)
{
    IntelligentCreatures.Add(new ClientIntelligentCreature(reader));
}
SummonedCreatureType = (IntelligentCreatureType)reader.ReadByte();
CreatureSummoned = reader.ReadBoolean();
AllowObserve = reader.ReadBoolean();
Observer = reader.ReadBoolean();
```

**Rust 要点**：三段可选物品栏，每段结构为 `[flag:bool]` → 有则 `[count:i32]` → count 次 `[present:bool][UserItem]`（present=false 时该槽 0 字节）；随后 `Gold/Credit`、两个 DateTime 与两段 `count` 循环（Magics、IntelligentCreatures）——第二个 count 复用同一局部变量，必须重新读取。

### `UserSlotsRefresh`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 22 (`0x0016`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:814` |

```csharp
if (reader.ReadBoolean())
{
    Inventory = new UserItem[reader.ReadInt32()];
    for (int i = 0; i < Inventory.Length; i++)
    {
        if (!reader.ReadBoolean()) continue;
        Inventory[i] = new UserItem(reader);
    }
}

if (reader.ReadBoolean())
{
    Equipment = new UserItem[reader.ReadInt32()];
    for (int i = 0; i < Equipment.Length; i++)
    {
        if (!reader.ReadBoolean()) continue;
        Equipment[i] = new UserItem(reader);
    }
}
```

**Rust 要点**：与 `UserInformation` 相同的“可选物品栏”模式，但只有 Inventory、Equipment 两段。

### `UserLocation`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 23 (`0x0017`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:885` |

```csharp
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`Point`（2 × i32，X 先 Y 后） 后接 `Direction:u8`，共 9 字节。

### `ObjectPlayer`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 24 (`0x0018`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:938` |

```csharp
            ObjectID = reader.ReadUInt32();
            Name = reader.ReadString();
            GuildName = reader.ReadString();
            GuildRankName = reader.ReadString();
            NameColour = Color.FromArgb(reader.ReadInt32());
            Class = (MirClass)reader.ReadByte();
            Gender = (MirGender)reader.ReadByte();
            Level = reader.ReadUInt16();
            Location = new Point(reader.ReadInt32(), reader.ReadInt32());
            Direction = (MirDirection)reader.ReadByte();
            Hair = reader.ReadByte();
            Light = reader.ReadByte();
            Weapon = reader.ReadInt16();
			WeaponEffect = reader.ReadInt16();
			Armour = reader.ReadInt16();
            Poison = (PoisonType)reader.ReadUInt16();
            Dead = reader.ReadBoolean();
            Hidden = reader.ReadBoolean();
            Effect = (SpellEffect)reader.ReadByte();
            WingEffect = reader.ReadByte();
            Extra = reader.ReadBoolean();
            MountType = reader.ReadInt16();
            RidingMount = reader.ReadBoolean();
            Fishing = reader.ReadBoolean();

            TransformType = reader.ReadInt16();

            ElementOrbEffect = reader.ReadUInt32();
            ElementOrbLvl = reader.ReadUInt32();
            ElementOrbMax = reader.ReadUInt32();

            int count = reader.ReadInt32();
            for (int i = 0; i < count; i++)
            {
                Buffs.Add((BuffType)reader.ReadByte());
            }

            LevelEffects = (LevelEffects)reader.ReadUInt16();
```

**Rust 要点**：结尾 `Buffs` 是 `i32 数量 × u8(BuffType)`；注意 `Weapon/WeaponEffect/Armour/MountType/TransformType` 为 **i16 有符号**，`Poison/LevelEffects` 为 u16。

### `ObjectHero`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 25 (`0x0019`) |
| 类别 | C（仅 BCL 值类型转换，逐字段线性） |
| 源 | `Shared/ServerPackets.cs:1033` |

```csharp
base.ReadPacket(reader);

OwnerName = reader.ReadString();
```

**Rust 要点**：**继承 `ObjectPlayer`**：先按 `ObjectPlayer` 的完整布局读（含结尾的 Buffs 循环），再接 `OwnerName:string`；Rust 实现应复用 `read_object_player()` 再追加一个字符串。

### `ObjectTurn`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 27 (`0x001B`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:1078` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`ObjectID:u32` + `Point`（2 × i32，X 先 Y 后） + `Direction:u8`。

### `ObjectWalk`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 28 (`0x001C`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:1104` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`ObjectID:u32` + `Point`（2 × i32，X 先 Y 后） + `Direction:u8`（与 ObjectTurn 布局完全相同，只有 Index 不同）。

### `ObjectRun`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 29 (`0x001D`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:1130` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`ObjectID:u32` + `Point`（2 × i32，X 先 Y 后） + `Direction:u8`（与 ObjectTurn 布局完全相同，只有 Index 不同）。

### `NewItemInfo`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 32 (`0x0020`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:1208` |

```csharp
Info = new ItemInfo(reader);
```

**Rust 要点**：读一个 `ItemInfo`（`Shared/Data/ItemData.cs:69`，含 `string Name`、多个枚举、`i16 Shape`、`u16 Image/Durability`、`u32 Price`、Stats 块等；注意版本参数一律取默认 `int.MaxValue`，见 §0 的提示）。

### `NewMonsterInfo`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 33 (`0x0021`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:6799` |

```csharp
Info = new ClientMonsterInfo(reader);
```

**Rust 要点**：读一个 `ClientMonsterInfo`（`Shared/Data/MonsterData.cs:23`）。

### `NewNPCInfo`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 34 (`0x0022`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:6820` |

```csharp
Info = new ClientNPCInfo(reader);
```

**Rust 要点**：读一个 `ClientNPCInfo`（`Shared/Data/ClientData.cs:283`）。

### `NewHeroInfo`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 35 (`0x0023`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:1228` |

```csharp
Info = new ClientHeroInformation(reader);
StorageIndex = reader.ReadInt32();
```

**Rust 要点**：`ClientHeroInformation`（`Shared/Data/ClientData.cs:645`）+ `StorageIndex:i32`。

### `NewChatItem`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 36 (`0x0024`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:1249` |

```csharp
Item = new UserItem(reader);
```

**Rust 要点**：读一个 `UserItem`（`Shared/Data/ItemData.cs:338`）。

### `SplitItem`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 44 (`0x002C`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:1598` |

```csharp
if (reader.ReadBoolean())
    Item = new UserItem(reader);

Grid = (MirGridType)reader.ReadByte();
```

**Rust 要点**：`[flag:bool]` 为 true 才读 `UserItem`，随后 `Grid:u8`；flag=false 时物品两字节省略。

### `PlayerInspect`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 57 (`0x0039`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:1802` |

```csharp
Name = reader.ReadString();
GuildName = reader.ReadString();
GuildRank = reader.ReadString();
Equipment = new UserItem[reader.ReadInt32()];
for (int i = 0; i < Equipment.Length; i++)
{
    if (reader.ReadBoolean())
        Equipment[i] = new UserItem(reader);
}

Class = (MirClass)reader.ReadByte();
Gender = (MirGender)reader.ReadByte();
Hair = reader.ReadByte();
Level = reader.ReadUInt16();
LoverName = reader.ReadString();
AllowObserve = reader.ReadBoolean();
IsHero = reader.ReadBoolean();
```

**Rust 要点**：`Equipment` 先读 `i32 长度`，再逐项 `[present:bool][UserItem]`；数组长度可为 0。

### `LogOutSuccess`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 58 (`0x003A`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:2018` |

```csharp
int count = reader.ReadInt32();

for (int i = 0; i < count; i++)
    Characters.Add(new SelectInfo(reader));
```

**Rust 要点**：`i32 数量 × SelectInfo`（角色选择列表）。

### `ObjectItem`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 64 (`0x0040`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:2142` |

```csharp
ObjectID = reader.ReadUInt32();
Name = reader.ReadString();
NameColour = Color.FromArgb(reader.ReadInt32());
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Image = reader.ReadUInt16();
grade = (ItemGrade)reader.ReadByte();
```

**Rust 要点**：`u32 ObjectID`、`string Name`、`Color`（小端 i32 的 0xAARRGGBB 位模式，读端用 `Color.FromArgb(int)`）、`Point`（2 × i32，X 先 Y 后）、`Image:u16`、`grade:u8`。

### `ObjectGold`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 65 (`0x0041`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:2175` |

```csharp
ObjectID = reader.ReadUInt32();
Gold = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
```

**Rust 要点**：`u32 ObjectID`、`u32 Gold`、`Point`（2 × i32，X 先 Y 后）。

### `GainedItem`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 66 (`0x0042`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:2199` |

```csharp
Item = new UserItem(reader);
```

**Rust 要点**：读一个 `UserItem`。

### `ObjectMonster`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 71 (`0x0047`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:2311` |

```csharp
ObjectID = reader.ReadUInt32();
Name = reader.ReadString();
NameColour = Color.FromArgb(reader.ReadInt32());
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Image = (Monster)reader.ReadUInt16();
Direction = (MirDirection)reader.ReadByte();
Effect = reader.ReadByte();
AI = reader.ReadByte();
Light = reader.ReadByte();
Dead = reader.ReadBoolean();
Skeleton = reader.ReadBoolean();
Poison = (PoisonType)reader.ReadUInt16();
Hidden = reader.ReadBoolean();
ShockTime = reader.ReadInt64();
BindingShotCenter = reader.ReadBoolean();
Extra = reader.ReadBoolean();
ExtraByte = reader.ReadByte();
MasterObjectId= reader.ReadUInt32();
Rarity= (MonsterType)reader.ReadByte();

int count = reader.ReadInt32();
for (int i = 0; i < count; i++)
{
    Buffs.Add((BuffType)reader.ReadByte());
}
```

**Rust 要点**：结尾 `Buffs` 为 `i32 数量 × u8`；前面 `Image` 是 u16 的 `Monster` 枚举、`Poison` 是 u16 位标志、`ShockTime` 是 i64。

### `ObjectAttack`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 72 (`0x0048`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:2385` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
Spell = (Spell)reader.ReadByte();
Level = reader.ReadByte();
Type = reader.ReadByte();
```

**Rust 要点**：`u32 ObjectID`、`Point`（2 × i32，X 先 Y 后）、`Direction:u8`、`Spell:u8`、`Level:u8`、`Type:u8`。

### `ObjectStruck`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 74 (`0x004A`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:2437` |

```csharp
ObjectID = reader.ReadUInt32();
AttackerID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`u32 ObjectID`、`u32 AttackerID`、`Point`（2 × i32，X 先 Y 后）、`Direction:u8`。

### `Death`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 80 (`0x0050`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:2578` |

```csharp
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`Point`（2 × i32，X 先 Y 后）、`Direction:u8`。

### `ObjectDied`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 81 (`0x0051`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:2603` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
Type = reader.ReadByte();
```

**Rust 要点**：`u32 ObjectID`、`Point`（2 × i32，X 先 Y 后）、`Direction:u8`、`Type:u8`。

### `ColourChanged`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 82 (`0x0052`) |
| 类别 | C（仅 BCL 值类型转换，逐字段线性） |
| 源 | `Shared/ServerPackets.cs:2629` |

```csharp
NameColour = Color.FromArgb(reader.ReadInt32());
```

**Rust 要点**：`Color`（小端 i32 的 0xAARRGGBB 位模式，读端用 `Color.FromArgb(int)`）。

### `ObjectColourChanged`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 83 (`0x0053`) |
| 类别 | C（仅 BCL 值类型转换，逐字段线性） |
| 源 | `Shared/ServerPackets.cs:2649` |

```csharp
ObjectID = reader.ReadUInt32();
NameColour = Color.FromArgb(reader.ReadInt32());
```

**Rust 要点**：`u32 ObjectID` + `Color`（小端 i32 的 0xAARRGGBB 位模式，读端用 `Color.FromArgb(int)`）。

### `ObjectHarvest`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 90 (`0x005A`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:2801` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`u32 ObjectID`、`Point`（2 × i32，X 先 Y 后）、`Direction:u8`。

### `ObjectHarvested`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 91 (`0x005B`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:2827` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`u32 ObjectID`、`Point`（2 × i32，X 先 Y 后）、`Direction:u8`。

### `ObjectNPC`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 92 (`0x005C`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:2860` |
| 命名 | 枚举成员是 `ObjectNpc`，类名是 `ObjectNPC` |

```csharp
ObjectID = reader.ReadUInt32();
Name = reader.ReadString();
NameColour = Color.FromArgb(reader.ReadInt32());
Image = reader.ReadUInt16();
Colour = Color.FromArgb(reader.ReadInt32());
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();

int count = reader.ReadInt32();

for (var i = 0; i < count; i++)
    QuestIDs.Add(reader.ReadInt32());
```

**Rust 要点**：结尾 `QuestIDs` 为 `i32 数量 × i32`（无 per-item 标志）。

### `NPCResponse`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 93 (`0x005D`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:2898` |

```csharp
Page = new List<string>();

int count = reader.ReadInt32();

for (int i = 0; i < count; i++)
    Page.Add(reader.ReadString());
```

**Rust 要点**：`Page` 为 `i32 数量 × string`，每个字符串自带 LEB128 长度前缀，**不应**假设总长为定值。

### `MapChanged`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 98 (`0x0062`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:2996` |

```csharp
MapIndex = reader.ReadInt32();
FileName = reader.ReadString();
Title = reader.ReadString();
MiniMap = reader.ReadUInt16();
BigMap = reader.ReadUInt16();
Lights = (LightSetting)reader.ReadByte();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
MapDarkLight = reader.ReadByte();
Music = reader.ReadUInt16();
Weather = (WeatherSetting)reader.ReadUInt16();
```

**Rust 要点**：`i32 MapIndex`、2 个 string、`u16 MiniMap`、`u16 BigMap`、`u8 Lights`、`Point`（2 × i32，X 先 Y 后）、`u8 Direction`、`u8 MapDarkLight`、`u16 Music`、`u16 Weather`。

### `NPCGoods`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 102 (`0x0066`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:3083` |
| 压缩 | **是**（`Compressed => true`，包体为 gzip 流） |

```csharp
int count = reader.ReadInt32();
for (int i = 0; i < count; i++)
    List.Add(new UserItem(reader));

Rate = reader.ReadSingle();
Type = (PanelType)reader.ReadByte();

HideAddedStats = reader.ReadBoolean();
```

**Rust 要点**：全协议唯一 `Compressed => true` 的包：读取时输入是 gzip 解压后的字节流；`i32 数量 × UserItem`，随后 `Rate:f32`、`Type:u8`、`HideAddedStats:bool`。

### `ItemSealChanged`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 116 (`0x0074`) |
| 类别 | C（仅 BCL 值类型转换，逐字段线性） |
| 源 | `Shared/ServerPackets.cs:3305` |

```csharp
UniqueID = reader.ReadUInt64();
ExpiryDate = DateTime.FromBinary(reader.ReadInt64());
```

**Rust 要点**：`u64 UniqueID` + i64（`DateTime.ToBinary()`：低 62 位是 ticks，高 2 位是 Kind）。

### `NewMagic`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 117 (`0x0075`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:3327` |

```csharp
Magic = new ClientMagic(reader);
Hero = reader.ReadBoolean();
```

**Rust 要点**：`ClientMagic`（`Shared/Data/ClientData.cs:19`）+ `Hero:bool`。

### `Magic`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 120 (`0x0078`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:3399` |

```csharp
Spell = (Spell)reader.ReadByte();
TargetID = reader.ReadUInt32();
Target = new Point(reader.ReadInt32(), reader.ReadInt32());
Cast = reader.ReadBoolean();
Level = reader.ReadByte();

var count = reader.ReadInt32();
SecondaryTargetIDs = new List<uint>();
for (int i = 0; i < count; i++)
{
    SecondaryTargetIDs.Add(reader.ReadUInt32());
}
```

**Rust 要点**：`SecondaryTargetIDs` 为 `i32 数量 × u32`（链式技能目标）。

### `ObjectMagic`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 123 (`0x007B`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:3484` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();

Spell = (Spell)reader.ReadByte();
TargetID = reader.ReadUInt32();

Target = new Point(reader.ReadInt32(), reader.ReadInt32());
Cast = reader.ReadBoolean();
Level = reader.ReadByte();
SelfBroadcast = reader.ReadBoolean();

var count = reader.ReadInt32();
SecondaryTargetIDs = new List<uint>();
for (int i = 0; i < count; i++)
{
    SecondaryTargetIDs.Add(reader.ReadUInt32());
}
```

**Rust 要点**：同 `Magic` 的 `i32 数量 × u32` 目标列表，另加 `SelfBroadcast:bool`。

### `RangeAttack`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 126 (`0x007E`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:3589` |

```csharp
TargetID = reader.ReadUInt32();
Target = new Point(reader.ReadInt32(), reader.ReadInt32());
Spell = (Spell)reader.ReadByte();
```

**Rust 要点**：`u32 TargetID` + `Target``Point`（2 × i32，X 先 Y 后） + `Spell:u8`，共 13 字节。

### `Pushed`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 127 (`0x007F`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:3615` |

```csharp
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`Point`（2 × i32，X 先 Y 后）、`Direction:u8`。

### `ObjectPushed`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 128 (`0x0080`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:3639` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`u32 ObjectID`、`Point`（2 × i32，X 先 Y 后）、`Direction:u8`。

### `UserStorage`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 130 (`0x0082`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:3678` |

```csharp
if (!reader.ReadBoolean()) return;

Storage = new UserItem[reader.ReadInt32()];
for (int i = 0; i < Storage.Length; i++)
{
    if (!reader.ReadBoolean()) continue;
    Storage[i] = new UserItem(reader);
}
```

**Rust 要点**：第一个字节是是否存在仓库的标志：为 false 时 **包在此结束**（不能继续读）；为 true 时读 `i32 长度` + 逐槽 `[present:bool][UserItem]`。

### `MapEffect`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 141 (`0x008D`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:3901` |

```csharp
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Effect = (SpellEffect)reader.ReadByte();
Value = reader.ReadByte();
```

**Rust 要点**：`Point`（2 × i32，X 先 Y 后）、`Effect:u8`、`Value:u8`。

### `ObjectRangeAttack`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 143 (`0x008F`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:3950` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
TargetID = reader.ReadUInt32();
Target = new Point(reader.ReadInt32(), reader.ReadInt32());
Type = reader.ReadByte();
Spell = (Spell)reader.ReadByte();
Level = reader.ReadByte();
```

**Rust 要点**：`u32 ObjectID`、`Point`（2 × i32，X 先 Y 后）、`u8 Direction`、`u32 TargetID`、`Point`（2 × i32，X 先 Y 后）（目标）、`u8 Type`、`u8 Spell`、`u8 Level`。

### `AddBuff`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 144 (`0x0090`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:3982` |

```csharp
Buff = new ClientBuff(reader);
```

**Rust 要点**：读一个 `ClientBuff`（`Shared/Data/ClientData.cs:593`）。

### `RefreshItem`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 148 (`0x0094`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:4052` |

```csharp
Item = new UserItem(reader);
```

**Rust 要点**：读一个 `UserItem`。

### `ObjectSpell`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 149 (`0x0095`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:4074` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Spell = (Spell)reader.ReadByte();
Direction = (MirDirection)reader.ReadByte();
Param = reader.ReadBoolean();
```

**Rust 要点**：`u32 ObjectID`、`Point`（2 × i32，X 先 Y 后）、`Spell:u8`、`Direction:u8`、`Param:bool`。

### `UserDash`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 150 (`0x0096`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:4104` |

```csharp
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`Point`（2 × i32，X 先 Y 后）、`Direction:u8`。

### `ObjectDash`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 151 (`0x0097`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:4129` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`u32 ObjectID`、`Point`（2 × i32，X 先 Y 后）、`Direction:u8`。

### `UserDashFail`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 152 (`0x0098`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:4155` |

```csharp
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`Point`（2 × i32，X 先 Y 后）、`Direction:u8`。

### `ObjectDashFail`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 153 (`0x0099`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:4180` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`u32 ObjectID`、`Point`（2 × i32，X 先 Y 后）、`Direction:u8`。

### `NPCMarket`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 155 (`0x009B`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:4230` |

```csharp
int count = reader.ReadInt32();

for (int i = 0; i < count; i++)
    Listings.Add(new ClientAuction(reader));

Pages = reader.ReadInt32();
UserMode = reader.ReadBoolean();
```

**Rust 要点**：`i32 数量 × ClientAuction`（`Shared/Data/ClientData.cs:218`），随后 `Pages:i32`、`UserMode:bool`。

### `NPCMarketPage`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 156 (`0x009C`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:4257` |

```csharp
int count = reader.ReadInt32();

for (int i = 0; i < count; i++)
    Listings.Add(new ClientAuction(reader));
```

**Rust 要点**：`i32 数量 × ClientAuction`。

### `ObjectSitDown`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 160 (`0x00A0`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:4365` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
Sitting = reader.ReadBoolean();
```

**Rust 要点**：`u32 ObjectID`、`Point`（2 × i32，X 先 Y 后）、`Direction:u8`、`Sitting:bool`。

### `BaseStatsInfo`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 162 (`0x00A2`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:4404` |

```csharp
Stats = new BaseStats(reader);
```

**Rust 要点**：读一个 `BaseStats`（`Shared/BaseStats.cs:102`，定长数值块）。

### `HeroBaseStatsInfo`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 163 (`0x00A3`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:4424` |

```csharp
Stats = new BaseStats(reader);
```

**Rust 要点**：读一个 `BaseStats`（同上，只有 Index 不同）。

### `ChatItemStats`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 165 (`0x00A5`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:4456` |

```csharp
ChatItemId = reader.ReadUInt64();
Stats = new UserItem(reader);
```

**Rust 要点**：`u64 ChatItemId` + `UserItem`。

### `GuildNoticeChange`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 166 (`0x00A6`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:4477` |

```csharp
update = reader.ReadInt32();
for (int i = 0; i < update; i++)
    notice.Add(reader.ReadString());
```

**Rust 要点**：`update:i32` 为负数是“清空公告”的哨兵值，此时不读任何字符串；否则读 `update` 条 string。写端等价于写 `notice.Count`。

### `GuildMemberChange`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 167 (`0x00A7`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:4506` |

```csharp
Name = reader.ReadString();
RankIndex = reader.ReadByte();
Status = reader.ReadByte();
if (Status > 5)
{
    int rankcount = reader.ReadInt32();
    for (int i = 0; i < rankcount; i++)
        Ranks.Add(new GuildRank(reader));
}
```

**Rust 要点**：只有 `Status > 5` 时才继续读 `i32 rankcount × GuildRank`；其余状态包体在 3 个字段后结束。

### `GuildStorageItemChange`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 173 (`0x00AD`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:4655` |

```csharp
Type = reader.ReadByte();
To = reader.ReadInt32();
From = reader.ReadInt32();
User = reader.ReadInt32();
if (!reader.ReadBoolean()) return;
Item = new GuildStorageItem
{
    UserId = reader.ReadInt64(),
    Item = new UserItem(reader)
};
```

**Rust 要点**：`[flag:bool]` 为 false 时包体结束；为 true 时读 `i64 UserId` + `UserItem` 组成 `GuildStorageItem`（**不是**调用 helper 构造，而是内联两字段）。

### `GuildStorageList`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 174 (`0x00AE`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:4687` |

```csharp
Items = new GuildStorageItem[reader.ReadInt32()];
for (int i = 0; i < Items.Length; i++)
{
    if (reader.ReadBoolean() == true)
        Items[i] = new GuildStorageItem(reader);
}
```

**Rust 要点**：先读 `i32 长度`，再 count 次 `[present:bool]`，present=true 才读 `GuildStorageItem`（i64 + UserItem）。

### `HeroCreateRequest`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 176 (`0x00B0`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:4724` |

```csharp
int count = reader.ReadInt32();
CanCreateClass = new bool[count];
for (int i = 0; i < count; i++)
    CanCreateClass[i] = reader.ReadBoolean();
```

**Rust 要点**：`i32 数量 × bool`（每个职业 1 字节可用标记）。

### `HeroInformation`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 178 (`0x00B2`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:4779` |

```csharp
ObjectID = reader.ReadUInt32();
Name = reader.ReadString();
Class = (MirClass)reader.ReadByte();
Gender = (MirGender)reader.ReadByte();
Level = reader.ReadUInt16();
Hair = reader.ReadByte();

HP = reader.ReadInt32();
MP = reader.ReadInt32();

Experience = reader.ReadInt64();
MaxExperience = reader.ReadInt64();

if (reader.ReadBoolean())
{
    Inventory = new UserItem[reader.ReadInt32()];
    for (int i = 0; i < Inventory.Length; i++)
    {
        if (!reader.ReadBoolean()) continue;
        Inventory[i] = new UserItem(reader);
    }
}

if (reader.ReadBoolean())
{
    Equipment = new UserItem[reader.ReadInt32()];
    for (int i = 0; i < Equipment.Length; i++)
    {
        if (!reader.ReadBoolean()) continue;
        Equipment[i] = new UserItem(reader);
    }
}

int count = reader.ReadInt32();

for (int i = 0; i < count; i++)
{
    Magics.Add(new ClientMagic(reader));
}

AutoPot = reader.ReadBoolean();
AutoHPPercent = reader.ReadByte();
AutoMPPercent = reader.ReadByte();
HPItemIndex = reader.ReadInt32();
MPItemIndex = reader.ReadInt32();
```

**Rust 要点**：英雄完整档案：两段可选物品栏（同 `UserInformation` 模式）+ `i32 数量 × ClientMagic` + 自动喝药字段；注意它**自己重写**了全部字段，未复用 `UserInformation` 的实例布局。

### `ManageHeroes`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 184 (`0x00B8`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:4967` |

```csharp
MaximumCount = reader.ReadInt32();

if (reader.ReadBoolean())
    CurrentHero = new ClientHeroInformation(reader);

if (reader.ReadBoolean())
{
    int count = reader.ReadInt32();
    Heroes = new ClientHeroInformation[count];
    for (int i = 0; i < count; i++)
    {
        if (reader.ReadBoolean())
            Heroes[i] = new ClientHeroInformation(reader);
    }
}
```

**Rust 要点**：两个可选块：`[flag]` → 单个 `ClientHeroInformation`；`[flag]` → `[count:i32]` → count 次 `[present:bool][ClientHeroInformation]`。

### `NPCImageUpdate`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 188 (`0x00BC`) |
| 类别 | C（仅 BCL 值类型转换，逐字段线性） |
| 源 | `Shared/ServerPackets.cs:5062` |

```csharp
ObjectID = reader.ReadUInt32();
Image = reader.ReadUInt16();
Colour = Color.FromArgb(reader.ReadInt32());
```

**Rust 要点**：`u32 ObjectID`、`u16 Image` + `Color`（小端 i32 的 0xAARRGGBB 位模式，读端用 `Color.FromArgb(int)`）。

### `TradeItem`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 195 (`0x00C3`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:1952` |

```csharp
TradeItems = new UserItem[reader.ReadInt32()];
for (int i = 0; i < TradeItems.Length; i++)
{
    if (reader.ReadBoolean())
        TradeItems[i] = new UserItem(reader);
}
```

**Rust 要点**：同 `PlayerInspect`：`i32 长度` + 逐项 `[present:bool][UserItem]`。

### `FishingUpdate`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 200 (`0x00C8`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:5159` |

```csharp
ObjectID = reader.ReadUInt32();
Fishing = reader.ReadBoolean();
ProgressPercent = reader.ReadInt32();
ChancePercent = reader.ReadInt32();
FishingPoint = new Point(reader.ReadInt32(), reader.ReadInt32());
FoundFish = reader.ReadBoolean();
```

**Rust 要点**：`u32 ObjectID`、`bool Fishing`、`i32 ProgressPercent`、`i32 ChancePercent`、`Point`（2 × i32，X 先 Y 后）、`bool FoundFish`。

### `ChangeQuest`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 201 (`0x00C9`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:5224` |

```csharp
Quest = new ClientQuestProgress(reader);
QuestState = (QuestState)reader.ReadByte();
TrackQuest = reader.ReadBoolean();
```

**Rust 要点**：`ClientQuestProgress`（`Shared/Data/ClientData.cs:546`）+ `QuestState:u8` + `TrackQuest:bool`。

### `CompleteQuest`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 202 (`0x00CA`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:5247` |

```csharp
int count = reader.ReadInt32();
for (var i = 0; i < count; i++)
    CompletedQuests.Add(reader.ReadInt32());
```

**Rust 要点**：`i32 数量 × i32`（已完成任务索引列表）。

### `NewQuestInfo`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 204 (`0x00CC`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:5293` |

```csharp
Info = new ClientQuestInfo(reader);
```

**Rust 要点**：读一个 `ClientQuestInfo`（`Shared/Data/ClientData.cs:395`）。

### `GainedQuestItem`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 205 (`0x00CD`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:5313` |

```csharp
Item = new UserItem(reader);
```

**Rust 要点**：读一个 `UserItem`。

### `UserBackStep`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 209 (`0x00D1`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:5431` |

```csharp
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`Point`（2 × i32，X 先 Y 后）、`Direction:u8`。

### `ObjectBackStep`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 210 (`0x00D2`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:5458` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
Distance = reader.ReadInt32();
```

**Rust 要点**：`u32 ObjectID`、`Point`（2 × i32，X 先 Y 后）、`Direction:u8`、`Distance:i32`。

### `UserDashAttack`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 211 (`0x00D3`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:5486` |

```csharp
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`Point`（2 × i32，X 先 Y 后）、`Direction:u8`。

### `ObjectDashAttack`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 212 (`0x00D4`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:5513` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
Distance = reader.ReadInt32();
```

**Rust 要点**：`u32 ObjectID`、`Point`（2 × i32，X 先 Y 后）、`Direction:u8`、`Distance:i32`。

### `UserAttackMove`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 213 (`0x00D5`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:5542` |

```csharp
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Direction = (MirDirection)reader.ReadByte();
```

**Rust 要点**：`Point`（2 × i32，X 先 Y 后）、`Direction:u8`（战士 SlashingBurst 的位移包）。

### `ItemUpgraded`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 215 (`0x00D7`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:5597` |

```csharp
Item = new UserItem(reader);
```

**Rust 要点**：读一个 `UserItem`。

### `ObjectDeco`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 219 (`0x00DB`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:5672` |

```csharp
ObjectID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
Image = reader.ReadInt32();
```

**Rust 要点**：`u32 ObjectID`、`Point`（2 × i32，X 先 Y 后）、`Image:i32`。

### `AwakeningNeedMaterials`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 228 (`0x00E4`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:5815` |

```csharp
if (!reader.ReadBoolean()) return;

int count = reader.ReadInt32();
Materials = new ItemInfo[count];
MaterialsCount = new byte[count];

for (int i = 0; i < Materials.Length; i++)
{
    if (!reader.ReadBoolean()) continue;
    Materials[i] = new ItemInfo(reader);
    MaterialsCount[i] = reader.ReadByte();
}
```

**Rust 要点**：首个 bool 为 false 时包体结束；否则 `count:i32` → count 次 `[present:bool]`，present=true 时读 `ItemInfo` + `MaterialsCount:u8`。present=false 时 **不读** 那 1 字节，MaterialsCount 保持默认 0。

### `ReceiveMail`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 231 (`0x00E7`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:5894` |

```csharp
int count = reader.ReadInt32();

for (int i = 0; i < count; i++)
    Mail.Add(new ClientMail(reader));
```

**Rust 要点**：`i32 数量 × ClientMail`（`Shared/Data/ClientData.cs:168`，内部含 DateTime 与 UserItem）。

### `ResizeStorage`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 238 (`0x00EE`) |
| 类别 | C（仅 BCL 值类型转换，逐字段线性） |
| 源 | `Shared/ServerPackets.cs:6029` |

```csharp
Size = reader.ReadInt32();
HasExpandedStorage = reader.ReadBoolean();
ExpiryTime = DateTime.FromBinary(reader.ReadInt64());
```

**Rust 要点**：`Size:i32`、`HasExpandedStorage:bool` + i64（`DateTime.ToBinary()`：低 62 位是 ticks，高 2 位是 Kind）。

### `NewIntelligentCreature`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 239 (`0x00EF`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:6051` |

```csharp
Creature = new ClientIntelligentCreature(reader);
```

**Rust 要点**：读一个 `ClientIntelligentCreature`（`Shared/Data/IntelligentCreatureData.cs:181`）。

### `UpdateIntelligentCreatureList`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 240 (`0x00F0`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:6073` |

```csharp
int count = reader.ReadInt32();
for (int i = 0; i < count; i++)
    CreatureList.Add(new ClientIntelligentCreature(reader));
CreatureSummoned = reader.ReadBoolean();
SummonedCreatureType = (IntelligentCreatureType)reader.ReadByte();
PearlCount = reader.ReadInt32();
```

**Rust 要点**：`i32 数量 × ClientIntelligentCreature`，随后 `CreatureSummoned:bool`、`SummonedCreatureType:u8`、`PearlCount:i32`。

### `NPCPearlGoods`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 243 (`0x00F3`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:6138` |

```csharp
int count = reader.ReadInt32();

for (int i = 0; i < count; i++)
    List.Add(new UserItem(reader));

Rate = reader.ReadSingle();

Type = (PanelType)reader.ReadByte();
```

**Rust 要点**：`i32 数量 × UserItem`，随后 `Rate:f32`、`Type:u8`（没有 `HideAddedStats`，与 `NPCGoods` 不同）。

### `FriendUpdate`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 245 (`0x00F5`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:6170` |

```csharp
int count = reader.ReadInt32();

for (int i = 0; i < count; i++)
    Friends.Add(new ClientFriend(reader));
```

**Rust 要点**：`i32 数量 × ClientFriend`（`Shared/Data/ClientData.cs:133`）。

### `LoverUpdate`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 246 (`0x00F6`) |
| 类别 | C（仅 BCL 值类型转换，逐字段线性） |
| 源 | `Shared/ServerPackets.cs:6228` |

```csharp
Name = reader.ReadString();
Date = DateTime.FromBinary(reader.ReadInt64());
MapName = reader.ReadString();
MarriedDays = reader.ReadInt16();
```

**Rust 要点**：`string Name` + i64（`DateTime.ToBinary()`：低 62 位是 ticks，高 2 位是 Kind） + `string MapName` + `MarriedDays:i16`。

### `GuildBuffList`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 248 (`0x00F8`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:6195` |

```csharp
Remove = reader.ReadByte();
int count = reader.ReadInt32();
for (int i = 0; i < count; i++)
    ActiveBuffs.Add(new GuildBuff(reader));
count = reader.ReadInt32();
for (int i = 0; i < count; i++)
    GuildBuffs.Add(new GuildBuffInfo(reader));
```

**Rust 要点**：两段独立循环：先 `i32 数量 × GuildBuff`，再**重新读一个** `i32 数量 × GuildBuffInfo`；两个 count 不必相等。

### `GameShopInfo`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 250 (`0x00FA`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:5357` |

```csharp
Item = new GameShopItem(reader, true);
StockLevel = reader.ReadInt32();
```

**Rust 要点**：`GameShopItem(reader, true)`（`Shared/Data/ItemData.cs:829`）+ `StockLevel:i32`。

### `Rankings`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 252 (`0x00FC`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:6307` |

```csharp
RankType = reader.ReadByte();
MyRank = reader.ReadInt32();
int count = reader.ReadInt32();
for (int i = 0; i < count; i++)
{
    ListingDetails.Add(new RankCharacterInfo(reader));
}
count = reader.ReadInt32();
for (int i = 0; i < count; i++)
{
    Listings.Add(reader.ReadInt64());
}
Count = reader.ReadInt32();
```

**Rust 要点**：`RankType:u8`、`MyRank:i32`，随后两个**相互独立**的 count 循环（RankCharacterInfo 列表、i64 列表），最后 `Count:i32`。

### `GetRentedItems`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 254 (`0x00FE`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:6365` |

```csharp
var count = reader.ReadInt32();

for (var i = 0; i < count; i++)
    RentedItems.Add(new ItemRentalInformation(reader));
```

**Rust 要点**：`i32 数量 × ItemRentalInformation`（`Shared/Data/ItemData.cs:1099`）。

### `UpdateRentalItem`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 260 (`0x0104`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:6501` |

```csharp
HasData = reader.ReadBoolean();

if (HasData)
    LoanItem = new UserItem(reader);
```

**Rust 要点**：`HasData:bool` 为 false 时不读 `LoanItem`，包体在 1 字节后结束。

### `NewRecipeInfo`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 266 (`0x010A`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:6619` |

```csharp
Info = new ClientRecipeInfo(reader);
```

**Rust 要点**：读一个 `ClientRecipeInfo`（`Shared/Data/ClientData.cs:82`）。

### `UpdateNotice`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 271 (`0x010F`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:6731` |

```csharp
Notice = new Notice(reader);
```

**Rust 要点**：读一个 `Notice`（`Shared/Data/Notice.cs:9`）。

### `SetCompass`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 273 (`0x0111`) |
| 类别 | C（仅 BCL 值类型转换，逐字段线性） |
| 源 | `Shared/ServerPackets.cs:6774` |

```csharp
var x = reader.ReadInt32();
var y = reader.ReadInt32();

Location = new Point(x, y);
```

**Rust 要点**：`Point`（2 × i32，X 先 Y 后）（先读入两个局部变量再组装）。

### `SendMemberLocation`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 275 (`0x0113`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ServerPackets.cs:3796` |

```csharp
MemberName = reader.ReadString();
MemberLocation = new Point(reader.ReadInt32(), reader.ReadInt32());
```

**Rust 要点**：`string MemberName` + `Point`（2 × i32，X 先 Y 后）。

### `GuildTerritoryPage`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 276 (`0x0114`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ServerPackets.cs:4279` |

```csharp
length = reader.ReadInt32();
int count = reader.ReadInt32();

for (int i = 0; i < count; i++)
    Listings.Add(new ClientGTMap(reader));
```

**Rust 要点**：开头的 `length:i32` 只是一个普通整数字段（与包长无关，原样保留），随后 `i32 数量 × ClientGTMap`（`Shared/Data/SharedData.cs:155`）。

### `StoragePasswordResult`（S→C）

| 项 | 值 |
| --- | --- |
| 方向 | Server → Client |
| Index | 278 (`0x0116`) |
| 类别 | C（仅 BCL 值类型转换，逐字段线性） |
| 源 | `Shared/ServerPackets.cs:217` |

```csharp
Result = reader.ReadByte();
Removing = reader.ReadBoolean();
HasPassword = reader.ReadBoolean();
LastSetTime = DateTime.FromBinary(reader.ReadInt64());
```

**Rust 要点**：`Result:u8`、`Removing:bool`、`HasPassword:bool` + i64（`DateTime.ToBinary()`：低 62 位是 ticks，高 2 位是 Kind）。

## 2. Client → Server 复杂包（11 个，按 Index 升序）

| Index | Hex | 包名 | 类别 |
| --- | --- | --- | --- |
| 0 | 0x0000 | `ClientVersion` | A |
| 3 | 0x0003 | `NewAccount` | C |
| 13 | 0x000D | `Chat` | A |
| 48 | 0x0030 | `RangeAttack` | B |
| 53 | 0x0035 | `CraftItem` | A |
| 58 | 0x003A | `Magic` | B |
| 80 | 0x0050 | `EditGuildNotice` | A |
| 117 | 0x0075 | `SendMail` | A |
| 123 | 0x007B | `MailCost` | A |
| 124 | 0x007C | `UpdateIntelligentCreature` | B |
| 134 | 0x0086 | `ReportIssue` | A |

### `ClientVersion`（C→S）

| 项 | 值 |
| --- | --- |
| 方向 | Client → Server |
| Index | 0 (`0x0000`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ClientPackets.cs:13` |

```csharp
VersionHash = reader.ReadBytes(reader.ReadInt32());
```

**Rust 要点**：`VersionHash = reader.ReadBytes(reader.ReadInt32())`：i32 字节数 + 原始字节。Rust 必须把长度与剩余字节比较（参考实现会直接按该长度分配，恶意长度可导致 OOM）。

### `NewAccount`（C→S）

| 项 | 值 |
| --- | --- |
| 方向 | Client → Server |
| Index | 3 (`0x0003`) |
| 类别 | C（仅 BCL 值类型转换，逐字段线性） |
| 源 | `Shared/ClientPackets.cs:67` |

```csharp
AccountID = reader.ReadString();
Password = reader.ReadString();
BirthDate = DateTime.FromBinary(reader.ReadInt64());
UserName = reader.ReadString();
SecretQuestion = reader.ReadString();
SecretAnswer = reader.ReadString();
EMailAddress = reader.ReadString();
```

**Rust 要点**：7 个 string（AccountID、Password、UserName、SecretQuestion、SecretAnswer、EMailAddress，外加夹在中间的 BirthDate）+ i64（`DateTime.ToBinary()`：低 62 位是 ticks，高 2 位是 Kind）；**密码是明文字符串**。

### `Chat`（C→S）

| 项 | 值 |
| --- | --- |
| 方向 | Client → Server |
| Index | 13 (`0x000D`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ClientPackets.cs:301` |

```csharp
Message = reader.ReadString();

int count = reader.ReadInt32();

for (int i = 0; i < count; i++)
    LinkedItems.Add(new ChatItem(reader));
```

**Rust 要点**：`Message:string` 后接 `i32 数量 × ChatItem`（`Shared/Data/ItemData.cs:1446`）。

### `RangeAttack`（C→S）

| 项 | 值 |
| --- | --- |
| 方向 | Client → Server |
| Index | 48 (`0x0030`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ClientPackets.cs:827` |

```csharp
Direction = (MirDirection)reader.ReadByte();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
TargetID = reader.ReadUInt32();
TargetLocation = new Point(reader.ReadInt32(), reader.ReadInt32());
```

**Rust 要点**：`Direction:u8` + `Location`（2 × i32）+ `u32 TargetID` + `TargetLocation`（2 × i32），共 21 字节。

### `CraftItem`（C→S）

| 项 | 值 |
| --- | --- |
| 方向 | Client → Server |
| Index | 53 (`0x0035`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ClientPackets.cs:923` |

```csharp
UniqueID = reader.ReadUInt64();
Count = reader.ReadUInt16();

Slots = new int[reader.ReadInt32()];
for (int i = 0; i < Slots.Length; i++)
{
    Slots[i] = reader.ReadInt32();
}
```

**Rust 要点**：`u64 UniqueID`、`u16 Count`，然后 `i32 数量 × i32` 的 `Slots` 数组。

### `Magic`（C→S）

| 项 | 值 |
| --- | --- |
| 方向 | Client → Server |
| Index | 58 (`0x003A`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ClientPackets.cs:1121` |

```csharp
ObjectID = reader.ReadUInt32();
Spell = (Spell) reader.ReadByte();
Direction = (MirDirection)reader.ReadByte();
TargetID = reader.ReadUInt32();
Location = new Point(reader.ReadInt32(), reader.ReadInt32());
SpellTargetLock = reader.ReadBoolean();
```

**Rust 要点**：`u32 ObjectID`、`Spell:u8`、`Direction:u8`、`u32 TargetID`、`Point`（2 × i32，X 先 Y 后）、`SpellTargetLock:bool`。

### `EditGuildNotice`（C→S）

| 项 | 值 |
| --- | --- |
| 方向 | Client → Server |
| Index | 80 (`0x0050`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ClientPackets.cs:1729` |

```csharp
int LineCount = reader.ReadInt32();
for (int i = 0; i < LineCount; i++)
    notice.Add(reader.ReadString());
```

**Rust 要点**：`i32 LineCount × string`（每行一条公告，逐条带 LEB128 长度前缀）。

### `SendMail`（C→S）

| 项 | 值 |
| --- | --- |
| 方向 | Client → Server |
| Index | 117 (`0x0075`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ClientPackets.cs:2131` |

```csharp
Name = reader.ReadString();
Message = reader.ReadString();
Gold = reader.ReadUInt32();

for (int i = 0; i < 5; i++)
{
    ItemsIdx[i] = reader.ReadUInt64();
}

Stamped = reader.ReadBoolean();
```

**Rust 要点**：`Name:string`、`Message:string`、`Gold:u32`，然后 **固定 5 × u64**（`ItemsIdx`，没有长度前缀），最后 `Stamped:bool`。

### `MailCost`（C→S）

| 项 | 值 |
| --- | --- |
| 方向 | Client → Server |
| Index | 123 (`0x007B`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ClientPackets.cs:2253` |

```csharp
Gold = reader.ReadUInt32();

for (int i = 0; i < 5; i++)
{
    ItemsIdx[i] = reader.ReadUInt64();
}

Stamped = reader.ReadBoolean();
```

**Rust 要点**：`Gold:u32`，然后 **固定 5 × u64**（没有长度前缀），最后 `Stamped:bool`。

### `UpdateIntelligentCreature`（C→S）

| 项 | 值 |
| --- | --- |
| 方向 | Client → Server |
| Index | 124 (`0x007C`) |
| 类别 | B（单一 helper 构造，复用其读取函数） |
| 源 | `Shared/ClientPackets.cs:2304` |

```csharp
Creature = new ClientIntelligentCreature(reader);
SummonMe = reader.ReadBoolean();
UnSummonMe = reader.ReadBoolean();
ReleaseMe = reader.ReadBoolean();
```

**Rust 要点**：`ClientIntelligentCreature` + 3 个 bool（`SummonMe`、`UnSummonMe`、`ReleaseMe`）。

### `ReportIssue`（C→S）

| 项 | 值 |
| --- | --- |
| 方向 | Client → Server |
| Index | 134 (`0x0086`) |
| 类别 | A（循环 / 条件 / 变长 / `ReadBytes`） |
| 源 | `Shared/ClientPackets.cs:2487` |

```csharp
Image = reader.ReadBytes(reader.ReadInt32());
ImageSize = reader.ReadInt32();
ImageChunk = reader.ReadInt32();
```

**Rust 要点**：`Image = ReadBytes(i32 长度)`（截图原始字节），随后 `ImageSize:i32`、`ImageChunk:i32`；长度必须做上界校验。

## 3. 空负载包（只有包头，4 字节）

这些包 `ReadPacket` 为空，线上内容只有 `[长度=4][编号]`。Rust 侧不要为它们编造字段。

**Server → Client（25 个）**：`Connected`、`LogOutFailed`、`ReturnToLogin`、`TeleportIn`、`NPCSell`、`NPCCheckRefine`、`NPCStorage`、`DeleteGroup`、`Revived`、`NPCConsign`、`GuildNameRequest`、`GuildRequestWar`、`UnlockHeroAutoPot`、`TradeConfirm`、`CancelReincarnation`、`RequestReincarnation`、`NPCAwakening`、`NPCDisassemble`、`NPCDowngrade`、`NPCReset`、`MailSendRequest`、`IntelligentCreatureEnableRename`、`CancelItemRental`、`CanConfirmItemRental`、`ConfirmItemRental`

**Client → Server（22 个）**：`Disconnect`、`LogOut`、`RefineCancel`、`PickUp`、`TownRevive`、`MarketRefresh`、`MarriageRequest`、`ChangeMarriage`、`DivorceRequest`、`AllowMentor`、`CancelMentor`、`TradeRequest`、`TradeCancel`、`AcceptReincarnation`、`CancelReincarnation`、`RefreshFriends`、`GetRentedItems`、`ItemRentalRequest`、`CancelItemRental`、`ItemRentalLockFee`、`ItemRentalLockItem`、`ConfirmItemRental`

## 4. 实现提醒

1. **不要把 `count` 当定长**：所有 `count` 前缀都是小端 `i32`，且允许为 0。为负时行为分两种：若该值用于 `new T[count]`（如 `PlayerInspect.Equipment`、`CraftItem.Slots`）会抛 `OverflowException`；若只用于 `for (int i = 0; i < count; i++)` 则循环体不执行、一个字节都不消费。Rust 应显式拒绝负值。
2. **`[present:bool]` 槽位标志**：物品槽每个位置都有一个 1 字节存在标志，为 `false` 时该槽不占后续字节；这是 `UserItem[]` 空槽的编码方式，必须逐槽读。
3. **helper 必须先实现**：`UserItem`、`ItemInfo`、`ClientMagic` 等结构体的读写函数是这些包的前置依赖，见 §0 的对照表。
4. **不要依赖读满**：`BinaryReader` 不校验“读完”，包体尾部多余字节会被静默忽略；Rust 解码器应以“成功消费至少所需字节”为准，而不是要求恰好读完。
