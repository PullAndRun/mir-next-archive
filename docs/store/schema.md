# Crystal 持久化数据模型（Rust 重写用 schema 说明）

> 参考实现：`github\Crystal`（下称「参考源」，文中相对路径均以该目录为根）。
> 数据库版本：`Envir.Version = 117`，`Envir.MinVersion = 60`，`Envir.CustomVersion = 0`（`Server/MirEnvir/Envir.cs:56-58`）。
> 目标：为 `传奇-Next`（Rust + PostgreSQL + sqlx）提供「等价存储」所需的字段级规格。
> 本文所有结论都标注了 `文件:行号`；凡是没能从代码确认的，明确写成「未验证」。

---

## 0. 存储容器一览（先搞清楚数据到底在哪个文件）

Crystal 没有 SQL 数据库，也没有「表」的概念；它用**多个独立的二进制文件**分摊持久化职责。账号与角色**不在** `Server.MirDB` 里。

| 文件 | 内容 | 读写入口 |
|---|---|---|
| `Server.MirDB` | 世界数据：地图 / 物品模板 / 怪物 / NPC / 任务头 / 技能 / 商城 / 攻城定义 / 刷怪 tick | `Envir.LoadDB` / `Envir.SaveDB`（`Envir.cs:2821` / `Envir.cs:2439`） |
| `Server.MirADB` | **账号 + 角色 + 英雄 + 拍卖 + 刷怪进度存档**（本文第 2、3 节的数据真正落在这里） | `Envir.LoadAccounts` / `Envir.SaveAccounts`（`Envir.cs:2944` / `Envir.cs:2550`） |
| `Guilds\{i}.mgd` | 行会（每个行会一个文件，文件名是列表下标而非 GuildIndex） | `Envir.LoadGuilds` / `Envir.SaveGuilds`（`Envir.cs:3089` / `Envir.cs:2614`） |
| `Envir\Goods\{npcIndex}.msdn` | NPC 商店已售出商品/补货状态 | `Envir.SaveGoods`（`Envir.cs:2671`） |
| `Conquests\{index}.mcdn` | 攻城战运行态（归属、城门/城墙血量、资金） | `Envir.SaveConquests`（`Envir.cs:2734`） |
| `Archive\{Name}__MMddyyyy_HHmmss.MirCA` | 被归档（删除/长期未登录）的角色 | `Envir.SaveArchivedCharacter`（`Envir.cs:2537`） |
| `Back Up\Database\Database yyyy-MM-dd HH-mm-ss.bak` | 每次 `SaveDB` 前的整库备份 | `Envir.cs:2441-2452` |
| `Back Up\Accounts\Accounts …bak` | 每次账号落盘前的备份 | `Envir.cs:2779-2786` |

路径常量：`Envir.cs:58-64`；`Guilds/Conquests/Goods/Recipe/Quests/Drops` 由 `Settings` 定义（`Server/Settings.cs:22-28`）。

> **对 Rust 重写的直接结论**：`Server.MirDB` 只包含「世界配置」，不含任何玩家数据；真实数据的迁移对象是 `Server.MirADB` + `Guilds/*.mgd` + `Conquests/*.mcdn` + `Envir/Goods/*.msdn`。前者格式见 `mirdb-format.md`，后者见该文第 9 节。

---

## 1. 二进制编码约定（贯穿所有记录）

全部用 .NET `BinaryWriter`/`BinaryReader` 默认行为（`Envir.cs:2454-2455`、`Envir.cs:2830-2831`）：

| 逻辑类型 | 编码 | 备注 |
|---|---|---|
| `int` / `uint` | 4 字节小端 | |
| `long` / `ulong` | 8 字节小端 | `NextUserItemID`、`MailID`、`AuctionID` 都是 `ulong` |
| `short` / `ushort` | 2 字节小端 | |
| `byte` / `sbyte` / `bool` | 1 字节 | `bool` 只允许 0/1，读到其它值说明已错位 |
| `float` | 4 字节 IEEE754 小端 | 仅 `MagicInfo.MultiplierBase/Bonus` |
| `string` | **7-bit 变长长度 + UTF-8 字节**（无 BOM、无 NUL 结尾） | 空串编码为 `0x00` |
| `DateTime` | `DateTime.ToBinary()` 的 int64 | 高 2 位是 `Kind`，其余是 ticks；**不是** Unix 时间戳 |
| `enum` | 显式 `(byte)`/`(short)`/`(int)` 强转后按整数写 | |
| `Point` | 两个 `int32`（X 后 Y） | |
| `Stats` | `int32 count` + `count × (byte statId, int32 value)` | `Shared/Data/Stat.cs:36-59`；`Stat` 枚举定义在 `Stat.cs:85-152` |
| `List<T>` / `T[]` | `int32 count` + `count × 元素` | 长度是数组长度，不是「有效元素数」 |
| 定长槽位数组 | `int32 count` + 每槽 `1 字节存在标志` + 元素 | 见第 4 节的两个相反语义 |

`Stats` 的 `statId` 是**稀疏枚举**（0-9 基础、10-23、30-36、40-48、100-108、120-129），只写非 0 项（`Stat.cs:6-26` 的 indexer 在赋值 0 时删除键），因此记录长度不固定。

`sbyte` 在 `Stats` 里也被提升为 `int32`，不用单独处理。

---

## 2. `AccountInfo`（账号）— 共 **31 个成员，其中 27 个落盘**

定义：`Server/MirDatabase/AccountInfo.cs:8-301`。落盘顺序见 `AccountInfo.cs:201-250`，读入见 `AccountInfo.cs:98-199`。
主键：`Index`（`int`），由 `++NextAccountID` 分配（`Envir.cs:3726`）；`AccountID` 上有唯一性检查（`Envir.cs:3720` → `AccountExists`）。

| # | 字段 | C# 类型 | 默认值 | 落盘 | 版本门 | 含义 |
|---|---|---|---|---|---|---|
| 1 | `Index` | `int` | `0` | ✅ i32 | — | 账号主键 |
| 2 | `AccountID` | `string` | `string.Empty` | ✅ string | — | 登录名（`^[A-Za-z0-9]{3,15}$`） |
| 3 | `password`（属性 `Password`） | `string` | `string.Empty` | ✅ string | `<94` 走 setter，`≥94` 直接赋值 | PBKDF2 哈希被 UTF-8 解码后的乱码串（见 §6） |
| 4 | `Salt` | `byte[]` | `new byte[24]` | ✅ i32 长度 + 原始字节 | `>93` | 密码盐，24 字节 |
| 5 | `RequirePasswordChange` | `bool` | `false` | ✅ bool | 读：`>97`（**写：无条件**） | 下次登录强制改密 |
| 6 | `storagePassword`（属性 `StoragePassword`） | `string` | `string.Empty` | ✅ string | `≥117` | 仓库二级密码哈希 |
| 7 | `StorageSalt` | `byte[]` | `new byte[24]` | ✅ i32 长度 + 原始字节 | `≥117` | 仓库密码盐 |
| 8 | `StoragePasswordLastSet` | `DateTime` | `DateTime.MinValue` | ✅ i64 | `≥117` | 仓库密码设置时间 |
| 9 | `HasStoragePassword` | `bool` | 计算属性 | ❌ | — | `!string.IsNullOrEmpty(storagePassword)`（`AccountInfo.cs:46`） |
| 10 | `UserName` | `string` | `string.Empty` | ✅ string | — | 真实姓名/昵称（≤20 字符，`Envir.cs:3700`） |
| 11 | `BirthDate` | `DateTime` | `default`(=MinValue=`0` 的 ToBinary) | ✅ i64 | — | 生日 |
| 12 | `SecretQuestion` | `string` | `string.Empty` | ✅ string | — | 密保问题（≤30，`Envir.cs:3706`） |
| 13 | `SecretAnswer` | `string` | `string.Empty` | ✅ string | — | 密保答案（**明文**，≤30，`Envir.cs:3712`） |
| 14 | `EMailAddress` | `string` | `string.Empty` | ✅ string | — | 邮箱（≤50，`Envir.cs:3693`） |
| 15 | `CreationIP` | `string` | `string.Empty` | ✅ string | — | 注册 IP |
| 16 | `CreationDate` | `DateTime` | `default` | ✅ i64 | — | 注册时间（`Envir.Now`，游戏内时钟） |
| 17 | `Banned` | `bool` | `false` | ✅ bool | — | 封禁标志 |
| 18 | `BanReason` | `string` | `string.Empty` | ✅ string | — | 封禁原因（多语言 key 结果） |
| 19 | `ExpiryDate` | `DateTime` | `default` | ✅ i64 | — | 封禁/锁定到期时间 |
| 20 | `WrongPasswordCount` | `int` | `0` | ❌ **不落盘** | — | 连续错误密码计数（重启即清零，见 §7.4） |
| 21 | `LastIP` | `string` | `string.Empty` | ✅ string | — | 最后登录 IP |
| 22 | `LastDate` | `DateTime` | `default` | ✅ i64 | — | 最后登录时间 |
| 23 | `Characters` | `List<CharacterInfo>` | 空列表 | ✅ i32 count + 内联记录 | — | 角色列表（真正的主数据，见 §3） |
| 24 | `Storage` | `UserItem[80]` | 长度 80 | ✅ i32 count + 每槽 bool + `UserItem` | — | 仓库（可扩建到 160，`AccountInfo.cs:266-275`） |
| 25 | `HasExpandedStorage` | `bool` | `false` | ✅ bool | `>75` | 是否已购买扩建仓库 |
| 26 | `ExpandedStorageExpiryDate` | `DateTime` | `default` | ✅ i64 | `>75` | 扩建到期时间 |
| 27 | `Gold` | `uint` | `0` | ✅ u32 | — | 账号级金币（邮寄/共享用） |
| 28 | `Credit` | `uint` | `0` | ✅ u32 | `≥63` | 充值点数（商城货币） |
| 29 | `Connection` | `MirConnection` | `null` | ❌ | — | 在线连接（运行时） |
| 30 | `Auctions` | `LinkedList<AuctionInfo>` | 空 | ❌（按账号不存） | — | 该账号的拍卖挂单；真实存储是 MirADB 的**全局** `Auctions` 列表 |
| 31 | `AdminAccount` | `bool` | `false` | ✅ bool | `≥10` | GM 账号标志 |

### 2.1 落盘字节顺序（v117）

```
i32  Index
str  AccountID
str  password            // 乱码哈希串
i32  Salt.Length (=24) ; bytes Salt
bool RequirePasswordChange
str  storagePassword
i32  StorageSalt.Length ; bytes StorageSalt
i64  StoragePasswordLastSet.ToBinary()
str  UserName
i64  BirthDate.ToBinary()
str  SecretQuestion
str  SecretAnswer
str  EMailAddress
str  CreationIP
i64  CreationDate.ToBinary()
bool Banned
str  BanReason
i64  ExpiryDate.ToBinary()
str  LastIP
i64  LastDate.ToBinary()
i32  Characters.Count ; Characters[i] ...          // CharacterInfo 内联，见 §3
bool HasExpandedStorage
i64  ExpandedStorageExpiryDate.ToBinary()
u32  Gold
u32  Credit
i32  Storage.Length ; { bool present; UserItem } ...
bool AdminAccount
```

### 2.2 写读不对称（移植时必读的坑）

`Save()` **总是**按最新布局写全部字段，而构造函数按 `Envir.LoadVersion` 决定读不读：

- `RequirePasswordChange`：写无条件（`AccountInfo.cs:208`），读仅 `LoadVersion > 97`（`AccountInfo.cs:111-112`）。
- `storagePassword/StorageSalt/StoragePasswordLastSet`：写无条件（`AccountInfo.cs:210-213`），读仅 `LoadVersion >= 117`（`AccountInfo.cs:114-119`）。

因此 **不能用当前的 `Save()` 去回写一个被标成旧版本号的文件**（会立刻错位）。反过来，旧文件缺失这些尾部字段是合法的，解析器必须严格按文件里的 `Version` 走分支——这也是 `mir-tools` 的实现要求（详见 `mirdb-format.md` §6）。

### 2.3 账号层面的派生约束

- `Storage` 用 `Globals.StorageGridSize = 80` 起步（`Shared/Globals.cs:19`），购买扩建后变 160；`IsValidStorageIndex` 在 `AccountInfo.cs:277-286`。
- 删除宽限期：读档时若角色 `Deleted` 且 `DeleteDate + ArchiveDeletedCharacterAfterMonths`（默认 1 个月，`Server/Settings.cs:341`）已过，则**不加载**，改写入 `Archive/*.MirCA`（`AccountInfo.cs:143-148`）。
- 长期未登录：`LastLoginDate == MinValue` 且 `CreationDate + 12 个月` 已过 → 归档（`AccountInfo.cs:150-155`）；已登录过的用 `LastLoginDate + 12 个月`（`AccountInfo.cs:157-162`）。默认值见 `Server/Settings.cs:340-341`。

---

## 3. `CharacterInfo`（角色）— 共 **78 个声明字段，其中 65 个落盘**

定义：`Server/MirDatabase/CharacterInfo.cs:8-609`。`Save`：`CharacterInfo.cs:391-570`；`Load`：`CharacterInfo.cs:127-389`。
主键：`Index`（`int`），由 `++NextCharacterID` 分配（`Envir.cs:4062`）；`Name` 全局唯一（`CharacterExists`，`Envir.cs:4056`）。

| # | 字段 | C# 类型 | 默认值 | 落盘 | 版本门 / 备注 |
|---|---|---|---|---|---|
| 1 | `Index` | `int` | `0` | ✅ i32 | 主键 |
| 2 | `Name` | `string` | `null`（无初值） | ✅ string | `^[\u4e00-\u9fa5_A-Za-z0-9]{3,15}$`（`Envir.cs:186`） |
| 3 | `Level` | `ushort` | `0` | ✅ u16 | `<62` 时是 u8（`CharacterInfo.cs:132-139`） |
| 4 | `Class` | `MirClass` | `Warrior(0)` | ✅ u8 | |
| 5 | `Gender` | `MirGender` | `Male(0)` | ✅ u8 | |
| 6 | `Hair` | `byte` | `0` | ✅ u8 | |
| 7 | `GuildIndex` | `int` | `-1` | ✅ i32 | -1 = 无行会 |
| 8 | `CreationIP` | `string` | `null` | ✅ string | |
| 9 | `CreationDate` | `DateTime` | `default` | ✅ i64 | |
| 10 | `Banned` | `bool` | `false` | ✅ bool | 角色级封禁 |
| 11 | `BanReason` | `string` | `""` | ✅ string | |
| 12 | `ExpiryDate` | `DateTime` | `default` | ✅ i64 | |
| 13 | `ChatBanned` | `bool` | `false` | ❌ **不落盘** | 只在内存生效，重启失效 |
| 14 | `ChatBanExpiryDate` | `DateTime` | `default` | ❌ **不落盘** | 同上 |
| 15 | `LastIP` | `string` | `""` | ✅ string | 登出时写（`PlayerObject.cs:419`） |
| 16 | `LastLogoutDate` | `DateTime` | `default` | ✅ i64 | 登出时写（`PlayerObject.cs:420`） |
| 17 | `LastLoginDate` | `DateTime` | `default` | ✅ i64 | 仅 `LoadVersion > 81` 才读（`CharacterInfo.cs:155-158`） |
| 18 | `Deleted` | `bool` | `false` | ✅ bool | 软删除标记 |
| 19 | `DeleteDate` | `DateTime` | `default` | ✅ i64 | 删除时间（决定归档） |
| 20 | `Married` | `int` | `0` | ✅ i32 | 配偶角色 Index |
| 21 | `MarriedDate` | `DateTime` | `default` | ✅ i64 | 结婚时间 |
| 22 | `Mentor` | `int` | `0` | ✅ i32 | 师傅/徒弟角色 Index |
| 23 | `MentorDate` | `DateTime` | `default` | ✅ i64 | |
| 24 | `IsMentor` | `bool` | `false` | ✅ bool | 该角色是否为师傅方 |
| 25 | `MentorExp` | `long` | `0` | ✅ i64 | 师徒经验池 |
| 26 | `CurrentMapIndex` | `int` | `0` | ✅ i32 | 当前地图（指向 `MapInfo.Index`） |
| 27 | `CurrentLocation` | `Point` | `(0,0)` | ✅ i32 X + i32 Y | |
| 28 | `Direction` | `MirDirection` | `Up(0)` | ✅ u8 | |
| 29 | `BindMapIndex` | `int` | `0` | ✅ i32 | 回城点地图 |
| 30 | `BindLocation` | `Point` | `(0,0)` | ✅ i32 X + i32 Y | |
| 31 | `HP` | `int` | `0` | ✅ i32 | `≤84` 时是 u16（`CharacterInfo.cs:169-178`） |
| 32 | `MP` | `int` | `0` | ✅ i32 | 同上 |
| 33 | `Experience` | `long` | `0` | ✅ i64 | |
| 34 | `AMode` | `AttackMode` | `Peace(0)` | ✅ u8 | 攻击模式 |
| 35 | `PMode` | `PetMode` | `Both(0)` | ✅ u8 | 宠物模式 |
| 36 | `AllowGroup` | `bool` | `false` | ✅ bool | |
| 37 | `AllowTrade` | `bool` | `false` | ✅ bool | |
| 38 | `AllowObserve` | `bool` | `false` | ✅ bool | 仅 `LoadVersion > 104` 才读（`CharacterInfo.cs:259-260`） |
| 39 | `PKPoints` | `int` | `0` | ✅ i32 | 仅 `LoadVersion > 34`（`CharacterInfo.cs:185-188`） |
| 40 | `NewDay` | `bool` | `false` | ❌ 不落盘 | 每日刷新标志（内存） |
| 41 | `Thrusting` | `bool` | `false` | ✅ bool | 战士技能被动开关 |
| 42 | `HalfMoon` | `bool` | `false` | ✅ bool | |
| 43 | `CrossHalfMoon` | `bool` | `false` | ✅ bool | |
| 44 | `DoubleSlash` | `bool` | `false` | ✅ bool | |
| 45 | `MentalState` | `byte` | `0` | ✅ u8 | 精神力战法状态 |
| 46 | `MentalStateLvl` | `byte` | `0` | ❌ **不落盘** | 等级丢失（只有 `MentalState` 存盘） |
| 47 | `Inventory` | `UserItem[46]` | 长度 46 | ✅ i32 长度 + 每槽 bool + item | 可扩容到 86（`CharacterInfo.cs:594-608`） |
| 48 | `Equipment` | `UserItem[14]` | 长度 14 | ✅ 同上 | 装备槽定长 14 |
| 49 | `Trade` | `UserItem[10]` | 长度 10 | ❌ **不落盘** | 交易暂存（只活一次会话） |
| 50 | `QuestInventory` | `UserItem[40]` | 长度 40 | ✅ 同上 | 任务专用背包 |
| 51 | `Refine` | `UserItem[16]` | 长度 16 | ❌ **不落盘** | 精炼暂存数组（结果只存 `CurrentRefine`） |
| 52 | `RentedItems` | `List<ItemRentalInformation>` | 空 | ✅ i32 count + 元素（`>75`） | 出租中物品（`CharacterInfo.cs:334-343`） |
| 53 | `RentedItemsToRemove` | `List<ItemRentalInformation>` | 空 | ❌ 不落盘 | 待回收队列 |
| 54 | `HasRentedItem` | `bool` | `false` | ✅ bool | 是否有出租物（`>75`） |
| 55 | `CurrentRefine` | `UserItem` | `null` | ✅ bool 非空 + item | 精炼中的物品 |
| 56 | `CollectTime` | `long` | `0` | ❌ 不落盘（派生） | 读档时 `= Envir.Time + RefineTimeRemaining`（`CharacterInfo.cs:326`） |
| 57 | `RefineTimeRemaining` | `long` | `0` | ✅ i64 | 存档时按 `CollectTime - Envir.Time` 重算并夹到 ≥0（`CharacterInfo.cs:527-532`） |
| 58 | `Magics` | `List<UserMagic>` | 空 | ✅ i32 count + 元素 | 已学技能 |
| 59 | `Pets` | `List<PetInfo>` | 空 | ✅ i32 count + 元素 | 召唤兽/宝宝 |
| 60 | `Buffs` | `List<Buff>` | 空 | ✅ i32 count + 元素 | 身上 Buff |
| 61 | `Poisons` | `List<Poison>` | 空 | ❌ **不落盘** | 中毒状态重启即清 |
| 62 | `Mail` | `List<MailInfo>` | 空 | ✅ i32 count + 元素 | 角色信箱（v>80 后邮件内嵌到角色） |
| 63 | `Friends` | `List<FriendInfo>` | 空 | ✅ i32 count + 元素 | 好友/黑名单 |
| 64 | `IntelligentCreatures` | `List<UserIntelligentCreature>` | 空 | ✅ i32 count + 元素 | 智能宠物 |
| 65 | `PearlCount` | `int` | `0` | ✅ i32 | 珍珠数（洗点货币） |
| 66 | `CurrentQuests` | `List<QuestProgressInfo>` | 空 | ✅ i32 count + 元素 | 进行中任务 |
| 67 | `CompletedQuests` | `List<int>` | 空 | ✅ i32 count + i32 数组 | 已完成任务 Index |
| 68 | `Flags` | `bool[1999]` | 全 false | ✅ 1999 个裸 bool（**无 count**） | `Globals.FlagIndexCount = 1999`（`Shared/Globals.cs:31`） |
| 69 | `AccountInfo` | `AccountInfo` | `null` | ❌ | 反向引用（运行时） |
| 70 | `Player` | `PlayerObject` | `null` | ❌ | 在线对象（运行时） |
| 71 | `Mount` | `MountInfo` | `null` | ❌ | 坐骑（运行时视图） |
| 72 | `GSpurchases` | `Dictionary<int,int>` | 空 | ✅ i32 count + (i32,i32)（`≥63`） | 商城购买计数（限购） |
| 73 | `Rank` | `int[2]` | `new int[2]` | ❌（源码注释明确「dont save this in db!」`CharacterInfo.cs:101`） | 排行榜缓存 |
| 74 | `MaximumHeroCount` | `int` | `1` | ✅ i32（`>98`） | 可携带英雄数 |
| 75 | `Heroes` | `HeroInfo[]` | `null` | ✅ 长度 + 每项 i32（英雄 Index，0=空）（`>102`） | `≤102` 时内联完整 `HeroInfo`（`CharacterInfo.cs:362-384`） |
| 76 | `CurrentHeroIndex` | `int` | `0` | ✅ i32（`>98`） | 当前出战英雄 |
| 77 | `HeroSpawned` | `bool` | `false` | ✅ bool（`>98`） | |
| 78 | `HeroBehaviour` | `HeroBehaviour` | `0` | ✅ u8（`>100`） | 英雄 AI 模式 |

### 3.1 落盘字节顺序（v117）

```
i32 Index ; str Name ; u16 Level ; u8 Class ; u8 Gender ; u8 Hair
str CreationIP ; i64 CreationDate
bool Banned ; str BanReason ; i64 ExpiryDate
str LastIP ; i64 LastLogoutDate ; i64 LastLoginDate
bool Deleted ; i64 DeleteDate
i32 CurrentMapIndex ; i32 CurrentLocation.X ; i32 CurrentLocation.Y ; u8 Direction
i32 BindMapIndex ; i32 BindLocation.X ; i32 BindLocation.Y
i32 HP ; i32 MP ; i64 Experience
u8 AMode ; u8 PMode
i32 PKPoints
i32 Inventory.Length      ; { bool present; UserItem } ...
i32 Equipment.Length      ; { bool present; UserItem } ...
i32 QuestInventory.Length ; { bool present; UserItem } ...
i32 Magics.Count          ; UserMagic ...
bool Thrusting; bool HalfMoon; bool CrossHalfMoon; bool DoubleSlash
u8 MentalState
i32 Pets.Count ; PetInfo ...
bool AllowGroup
bool Flags[1999]           // 1999 个单字节，无 count
i32 GuildIndex
bool AllowTrade ; bool AllowObserve
i32 CurrentQuests.Count ; QuestProgressInfo ...
i32 Buffs.Count ; Buff ...
i32 Mail.Count ; MailInfo ...
i32 IntelligentCreatures.Count ; UserIntelligentCreature ...
i32 PearlCount
i32 CompletedQuests.Count ; i32 ...
bool hasCurrentRefine ; UserItem ...
i64 RefineTimeRemaining
i32 Friends.Count ; FriendInfo ...
i32 RentedItems.Count ; ItemRentalInformation ...
bool HasRentedItem
i32 Married ; i64 MarriedDate ; i32 Mentor ; i64 MentorDate ; bool IsMentor ; i64 MentorExp
i32 GSpurchases.Count ; (i32,i32) ...
i32 MaximumHeroCount ; i32 Heroes[i] ...   // 只写 Index，0 表示空槽
i32 CurrentHeroIndex ; bool HeroSpawned ; u8 HeroBehaviour
```

### 3.2 角色内嵌的子记录布局

| 类型 | 落盘布局 | 出处 |
|---|---|---|
| `UserMagic` | `u8 Spell` + `u8 Level` + `u8 Key` + `u16 Experience` + `bool IsTempSpell` + `i64 CastTime`（`CastTime` 仅 `>64`；读入后立即被重置为 `int.MinValue`，`CharacterInfo.cs:232`） | `Server/MirDatabase/MagicInfo.cs:88-144` |
| `PetInfo` | `i32 MonsterIndex` + `i32 HP`（`≤84` 为 u32）+ `u32 Experience` + `u8 Level` + `u8 MaxPetLevel` | `CharacterInfo.cs:611-656` |
| `Buff` | `u8 Type` + `u32 ObjectID` + `i64 ExpireTime` + `Stats` + `i32 dataCount` + `(str key, i32 len, byte[len])` + `i32 valuesCount` + `i32[]` | `Server/MirDatabase/BuffInfo.cs:210-239` |
| `QuestProgressInfo` | `i32 Index` + `i64 StartDateTime` + `i64 EndDateTime` + 击杀 `(i32 MonsterID, i32 Count)` 列表 + 交物 `(i32 ItemID, i32 Count)` 列表 + 旗标 `(i32 Number, bool State)` 列表；`<90` 为旧的「只有数值」格式 | `Server/MirDatabase/QuestProgressInfo.cs:291-318` |
| `MailInfo` | `u64 MailID` + `str Sender` + `i32 RecipientIndex` + `str Message` + `u32 Gold` + `i32 itemCount` + `UserItem[]`（**无存在标志**）+ `i64 DateSent` + `i64 DateOpened` + `bool Locked` + `bool Collected` + `bool CanReply` | `Server/MirDatabase/MailInfo.cs:79-97` |
| `FriendInfo` | `i32 Index` + `bool Blocked` + `str Memo` | `CharacterInfo.cs:745-757` |
| `ItemRentalInformation` | `u64 ItemId` + `str ItemName` + `str RentingPlayerName` + `i64 ItemReturnDate` | `Shared/Data/ItemData.cs:1099-1113` |
| `UserIntelligentCreature` | `u8 PetType` + `str CustomName` + `i32 Fullness` + `i32 SlotIndex` + `i64 ExpireSeconds`（`-9999` 表示永久）+ `i64 BlackstoneTime` + `u8 petMode` + 9 个 `bool` 过滤器 + `u8 PickupGrade` + `i64 MaintainFoodTime` | `Server/MirDatabase/IntelligentCreatureInfo.cs:134-160`、`Shared/Data/IntelligentCreatureData.cs:144-156` |
| `HeroInfo` | **不是** `CharacterInfo` 的完整布局，见 §3.4 | `Server/MirDatabase/HeroInfo.cs:98-147` |

> `MailInfo` 的 `Items` 是「count + 裸 `UserItem`」——与背包的「count + bool + item」不同，解析时必须区分（`MailInfo.cs:87-89`）。

### 3.3 存档语义上的重要事实（Rust 端设计影响很大）

1. **没有「玩家对象 → 持久化对象」的拷贝步骤**。`HumanObject` 的属性直接代理到 `CharacterInfo`：
   `HP { get => Info.HP; set => Info.HP = value; }`（`Server/MirObjects/HumanObject.cs:59-68`）、
   `Experience`（`:79-83`）、`Buffs`（`:99-103`）、`CurrentMapIndex/CurrentLocation/Direction/Level`（`:31-50`）。
   也就是说**内存中的 `CharacterInfo` 就是数据库行**，定期落盘只是把它序列化。
   → Rust 端可以彻底反转：内存实体为 `Player`，落盘时显式映射到行；不要复制这种「双身份」设计。
2. `Friends` 的 count 与记录数可能不一致：`writer.Write(Friends.Count)` 之后对 `Info == null` 的项 `continue`（`CharacterInfo.cs:534-539`）。只要有一个好友的角色已被删除，整个账号文件就会**写坏**（后续所有账号偏移全错）。这是参考实现里最危险的持久化 bug，Rust 端不得复制。
3. 读档时**静默丢弃**无法绑定的数据（详见 §7.3）：物品（`Envir.BindItem` 返回 false）、技能（`magic.Info == null`）、任务（孤儿任务）、智能宠物（`creature.Info == null`）。
4. `Flags` 是 1999 个裸 `bool`，**没有长度前缀**。`Globals.FlagIndexCount` 一旦变化，老文件立刻不可解析——迁移时必须把该常量当作格式的一部分固化下来。
5. `Inventory` 长度会随扩容变化（46 → 54 → …→ 86，`CharacterInfo.cs:594-608`），所以存档里写的是**数组长度**而不是 46。
6. `Trade`、`Refine`、`Poisons`、`ChatBanned`、`NewDay`、`MentalStateLvl` 都不落盘：Rust 端如果希望这些状态重启后保留，属于**超出原版的增强**，需要额外列。

### 3.4 `HeroInfo` 的落盘差异

`HeroInfo : CharacterInfo`（`Server/MirDatabase/HeroInfo.cs:5`），但 `Save` 被重写为更短的布局（`HeroInfo.cs:98-147`）：

```
i32 Index ; str Name ; u16 Level ; u8 Class ; u8 Gender ; u8 Hair
i64 CreationDate.ToBinary()
bool Deleted ; i64 DeleteDate
i32 HP ; i32 MP ; i64 Experience
i32 Inventory.Length ; { bool present; UserItem } ...
i32 Equipment.Length ; { bool present; UserItem } ...
i32 Magics.Count ; UserMagic ...
bool AutoPot ; u8 Grade ; i32 HPItemIndex ; i32 MPItemIndex ; u8 AutoHPPercent ; u8 AutoMPPercent
u16 SealCount
```

注意：英雄**没有** `CreationIP/Banned/LastIP/地图坐标/Buffs/Quests/Mail/...`；`SealCount` 仅在 `version > 101` 时读（`HeroInfo.cs:93-94`）。英雄记录存放在 `Server.MirADB` 顶层的 `HeroList` 里（`Envir.cs:2584-2586`），角色只保存英雄 Index。

---

## 4. `UserItem`（物品实例）— 完整字段与精确二进制

定义：`Shared/Data/ItemData.cs:277-709`；`Save`：`ItemData.cs:460-509`；`Load`：`ItemData.cs:338-458`。
主键：`UniqueID`（`ulong`），由 `++NextUserItemID` 分配；`ItemIndex` 指向 `ItemInfo.Index` 模板。

| # | 字段 | C# 类型 | 默认值 | 落盘 | 版本门 | 含义 |
|---|---|---|---|---|---|---|
| 1 | `UniqueID` | `ulong` | `0` | ✅ u64 | — | 物品全局唯一 ID |
| 2 | `ItemIndex` | `int` | `0` | ✅ i32 | — | 模板 Index |
| 3 | `Info` | `ItemInfo` | `null` | ❌（由 `ItemIndex` 反查） | — | 模板引用 |
| 4 | `CurrentDura` | `ushort` | `0` | ✅ u16 | — | 当前耐久 |
| 5 | `MaxDura` | `ushort` | `0` | ✅ u16 | — | 最大耐久 |
| 6 | `Count` | `ushort` | `1` | ✅ u16 | `≤84` 为 u32（`ItemData.cs:346-353`） | 堆叠数量 |
| 7 | `GemCount` | `ushort` | `0` | ✅ u16 | `≤84` 为 u32 | 镶嵌宝石数 |
| 8 | `RefinedValue` | `RefinedValue` | `None(0)` | ✅ u8 | — | 精炼属性种类 |
| 9 | `RefineAdded` | `byte` | `0` | ✅ u8 | — | 精炼追加值 |
| 10 | `RefineSuccessChance` | `int` | `0` | ✅ i32 | `>85` | 下次精炼成功率 |
| 11 | `DuraChanged` | `bool` | `false` | ❌ | — | 脏标记（运行时） |
| 12 | `SoulBoundId` | `int` | `-1` | ✅ i32 | — | 绑定角色 Index，-1=未绑定 |
| 13 | `Identified` | `bool` | `false` | ✅ bit0 of `Bools` | — | 已鉴定 |
| 14 | `Cursed` | `bool` | `false` | ✅ bit1 of `Bools` | — | 被诅咒 |
| 15 | `WeddingRing` | `int` | `-1` | ✅ i32 | — | 结婚戒指绑定的角色 Index |
| 16 | `Slots` | `UserItem[]` | 长度 0 | ✅ i32 count + 每槽 `bool` + item | — | 子槽（镶嵌/坐骑件）；**bool 语义与背包相反** |
| 17 | `BuybackExpiryDate` | `DateTime` | `default` | ❌ | — | 回购到期（运行时） |
| 18 | `ExpireInfo` | `ExpireInfo` | `null` | ✅ `bool` + `i64 ExpiryDate` | `≥65` | 时效道具到期 |
| 19 | `RentalInformation` | `RentalInformation` | `null` | ✅ `bool` + (`str OwnerName`,`i16 BindMode`,`i64 ExpiryDate`,`bool RentalLocked`) | `≥76` | 租赁信息 |
| 20 | `SealedInfo` | `SealedInfo` | `null` | ✅ `bool` + `i64 ExpiryDate` + `i64 NextSealDate` | `≥92`（`NextSealDate` 仅 `>92`） | 封印（封）信息 |
| 21 | `IsShopItem` | `bool` | `false` | ✅ bool | `≥83` | 商城直购标记 |
| 22 | `Awake` | `Awake` | `new Awake()` | ✅ `u8 Type` + `i32 count` + `byte[count]` | — | 觉醒属性 |
| 23 | `AddedStats` | `Stats` | `null`（`new UserItem(info)` 才初始化） | ✅ `Stats` | `>84` 才是字典格式 | 追加属性（+1 +2 …） |
| 24 | `GMMade` | `bool` | `false`（自动属性） | ✅ bool | `>107` | GM 制造标记 |
| 25 | `IsAdded` | `bool` | 计算属性 | ❌ | — | `AddedStats.Count > 0 \|\| Slots.Length > Info.Slots` |
| 26 | `Weight` / `FriendlyName` / `Price()` 等 | — | — | ❌ | — | 计算值 |

### 4.1 精确字节布局（v117 全量）

```text
off  size  字段
0    8     UniqueID                u64
8    4     ItemIndex               i32
12   2     CurrentDura             u16
14   2     MaxDura                 u16
16   2     Count                   u16          (≤84: u32)
18   4     SoulBoundId             i32          (-1 = 未绑定)
22   1     Bools                   u8           bit0=Identified, bit1=Cursed
23   4     SlotCount               i32          = Slots.Length
27   var   槽位循环 SlotCount 次:
           1   slotIsEmpty        bool         *** true 表示空槽 ***
           n   UserItem           递归（仅当 slotIsEmpty == false）
     --    空槽只占 1 字节；非空槽占 1 + 嵌套物品长度
x    2     GemCount                u16          (≤84: u32)
x    var   AddedStats              Stats = i32 count + count*(u8 statId, i32 value)
x    1     Awake.Type              u8
x    4     Awake.Count             i32
x    var   Awake.Values            Count * u8
x    1     RefinedValue            u8
x    1     RefineAdded             u8
x    4     RefineSuccessChance     i32          (>85)
x    4     WeddingRing             i32
x    1     HasExpireInfo           bool         (≥65)
x     8      ExpireInfo.ExpiryDate i64        (仅当 HasExpireInfo)
x    1     HasRental               bool         (≥76)
x     var     Rental.OwnerName      string
x     2       Rental.BindingFlags   i16
x     8       Rental.ExpiryDate     i64
x     1       Rental.RentalLocked   bool
x    1     IsShopItem              bool         (≥83)
x    1     HasSealed               bool         (≥92)
x     8       Sealed.ExpiryDate     i64
x     8       Sealed.NextSealDate   i64        (>92)
x    1     GMMade                  bool         (>107)
```

**最小尺寸**（v117，`Stats` 为空、无槽位、无附加信息的一件普通物品）：
`8+4+2+2+2+4+1+4+2+(4)+1+4+1+1+4+4+1+1+1+1+1 = 53 字节`
（依次为 `UniqueID 8`、`ItemIndex 4`、`CurrentDura 2`、`MaxDura 2`、`Count 2`、`SoulBoundId 4`、`Bools 1`、`SlotCount 4`、`GemCount 2`、`Stats.count 4`、`Awake.Type 1`、`Awake.Count 4`、`RefinedValue 1`、`RefineAdded 1`、`RefineSuccessChance 4`、`WeddingRing 4`、`HasExpireInfo 1`、`HasRental 1`、`IsShopItem 1`、`HasSealed 1`、`GMMade 1`）。

### 4.2 三个必须照抄的「怪癖」

1. **子槽存在标志是反的。** 写：`writer.Write(Slots[i] == null); if (Slots[i] == null) continue;`（`ItemData.cs:476-483`），读：`if (reader.ReadBoolean()) continue;`（`ItemData.cs:397-402`）。
   而背包/仓库/装备用的是正语义：`writer.Write(Inventory[i] != null)`（`CharacterInfo.cs:431-438`）、`writer.Write(Storage[i] != null)`（`AccountInfo.cs:242-248`）。
   → 同一文件里两种相反约定，`mir-tools` 必须逐处区分。
2. **`Stats` 里 `sbyte` 被提升为 `int32`。** 旧格式（`≤84`）里 `Luck/AttackSpeed` 按 `sbyte` 写；新格式统一进 `Stats` 字典，值域是 `int32`，所以 `AttackSpeed = -1` 在 v105 样本里就是 `{14: -1}`（本项目实测，见 `mirdb-format.md` §8）。
3. **`Count` 的宽度随版本变化**（u32 → u16，版本 84/85 分界），同样影响 `ItemInfo.StackSize`、`GemCount`、`GameShopItem.Count`、`Conquest*Info.RepairCost`（`ConquestInfo.cs:243-250`）。

---

## 5. 其他表/记录清单（主键 + 一句话用途）

「完整列」只有 `AccountInfo`/`CharacterInfo`/`UserItem` 三张在这一版给出；其余记录先给主键与用途，逐列布局在 `mirdb-format.md`（世界数据段）与后续文档补齐。

| 记录 | 主键 | 一句话用途 | 存储位置 |
|---|---|---|---|
| `MapInfo` | `int Index` | 地图：文件名/标题/小地图/光照/安全区/刷怪点/传送点/矿区/各类禁制标志 | MirDB |
| `ItemInfo` | `int Index` | 物品模板：类型/需求/属性/绑定/价格/堆叠/随机属性档/套装 | MirDB |
| `UserItem` | `ulong UniqueID` | 物品实例（见 §4） | 内嵌于角色/仓库/邮件/拍卖/配方 |
| `MonsterInfo` | `int Index` | 怪物模板：形象/AI/属性/攻击/移动/经验/掉落文件 | MirDB |
| `NPCInfo` | `int Index` | NPC：脚本文件名/地图坐标/形象/营业时段/等级限制/所属攻城 | MirDB |
| `QuestInfo` | `int Index` | 任务定义（DB 只存头部；正文/奖励在 `Envir/Quests/{FileName}.txt`） | MirDB + 文本 |
| `MagicInfo` | `Spell Spell`（枚举即主键） | 技能模板：等级要求/消耗/延迟/威力/倍率 | MirDB |
| `UserMagic` | (`角色 Index`, `Spell`) | 角色已学技能：等级/经验/快捷键 | 角色内嵌 |
| `BuffInfo` | `BuffType Type` | Buff 静态定义（图标/可见性/叠加规则） | `BuffInfo.Load()`（`BuffInfo.cs:14`） |
| `Buff` | `uint ObjectID` + 存在时长 | 角色身上的 Buff 实例（含 Stats 与自定义数据字典） | 角色内嵌 |
| `QuestProgressInfo` | (`角色`, `int Index`) | 任务进度：击杀/交物/旗标计数与起止时间 | 角色内嵌 |
| `MailInfo` | `ulong MailID` | 邮件：寄件人/正文/附件物品/金币/已读已领 | 角色内嵌（v>80） |
| `AuctionInfo` | `ulong AuctionID` | 拍卖/寄售挂单：物品、价格、当前出价、买家 | MirADB 全局列表 |
| `GuildInfo` | `int GuildIndex` | 行会：等级/经验/资金/职位表/仓库/公告/攻城租借 | `Guilds/{i}.mgd` |
| `GuildRank` | (行会, 职位序号) | 行会职位与权限位 | 行会内嵌 |
| `GuildStorageItem` | (行会, 槽位) | 行会仓库槽（物品 + 存入者） | 行会内嵌 |
| `HeroInfo` | `int Index` | 英雄角色（`CharacterInfo` 子类，见 §3.4） | MirADB 顶层 `HeroList` |
| `IntelligentCreatureInfo` | `IntelligentCreatureType PetType` | 智能宠物规则（拾取范围/饱食度/产出） | 代码内静态表 |
| `UserIntelligentCreature` | (`角色`, `int SlotIndex`) | 角色拥有的智能宠物实例 | 角色内嵌 |
| `ConquestInfo` | `int Index` | 攻城区域定义：宫殿/城墙/城门/弓箭手/旗子/控制点/日程 | MirDB |
| `ConquestGuildInfo` | `int Info.Index` | 攻城运行态：归属行会、城门/城墙血量、资金、攻击方 | `Conquests/{index}.mcdn` |
| `RecipeInfo` | `string Name` | 合成配方：产物/材料/工具/要求/成功率/费用 | `Envir/Recipe/*.txt` |
| `RespawnInfo` | `int RespawnIndex` | 刷怪点：怪物/数量/范围/延迟/朝向/路径/是否存时间 | MirDB（MapInfo 内） |
| `RespawnTimer` | 单例 | 全局刷怪 tick（基础间隔/当前计数/在线人数补偿） | MirDB 尾部 |
| `RespawnSave` | `int RespawnIndex` | 刷怪点存档状态（下次刷新 tick/是否已刷） | MirADB 尾部 |
| `SafeZoneInfo` | (`MapInfo`, 顺序) | 安全区（中心点/半径/是否出生点） | MirDB（MapInfo 内） |
| `MovementInfo` | (`MapInfo`, 顺序) | 地图传送点（源/目标地图与坐标/需空洞/需移动） | MirDB（MapInfo 内） |
| `MineZone` | (`MapInfo`, 顺序) | 矿区（中心/半径/矿区类型） | MirDB（MapInfo 内） |
| `GameShopItem` | `int GIndex` | 商城条目：物品/金价/点价/库存/限购/上下架 | MirDB |
| `GTMap` | `int Index` | GT（攻城区）租借信息：拥有者/会长/价格/天数 | MirDB 尾部（**只写不读**，见 `mirdb-format.md` §5） |
| `NPCGoods`（`UsedGoods`） | (`NPCInfo.Index`, 槽位) | NPC 商店已售出/待补货列表 | `Envir/Goods/{npcIndex}.msdn` |
| `AccountInfo` | `int Index` | 账号（§2） | MirADB |
| `CharacterInfo` | `int Index` | 角色（§3） | MirADB（账号内嵌） |
| `ItemRentalInformation` | `ulong ItemId` | 出租物品记录（租给谁、何时归还） | 角色内嵌 |

---

## 6. 密码存储：算法与那个必须照抄的 quirk

### 6.1 算法

`Server/Utils/Crypto.cs` 全文只有 26 行，规格如下：

| 项 | 值 | 出处 |
|---|---|---|
| KDF | PBKDF2（`Rfc2898DeriveBytes`） | `Crypto.cs:22` |
| PRF | **HMAC-SHA1** | `Crypto.cs:22` 的 `HashAlgorithmName.SHA1` |
| 盐长度 | **24 字节**，`RandomNumberGenerator.Create().GetBytes()` | `Crypto.cs:8-18` |
| 迭代次数 | **50** | `Crypto.cs:10` |
| 派生密钥长度 | **24 字节** | `Crypto.cs:9, 23` |
| 最终存储形态 | `Encoding.UTF8.GetString(派生字节)` —— **把哈希字节当 UTF-8 解码成字符串** | `Crypto.cs:23` |
| 比较方式 | `string.CompareOrdinal`（序数比较，**非常量时间**） | `Envir.cs:3884`、`AccountInfo.cs:293` |

### 6.2 quirk 原文引用

```csharp
// Server/Utils/Crypto.cs:20-24
public static string HashPassword(string password, byte[] salt)
{
    Rfc2898DeriveBytes pbkdf2 = new Rfc2898DeriveBytes(password, salt, Iterations, HashAlgorithmName.SHA1);
    return Encoding.UTF8.GetString(pbkdf2.GetBytes(HashSize));
}
```

调用处（新建账号 / 改密）：

```csharp
// Server/MirDatabase/AccountInfo.cs:21-30
public string Password
{
    get { return password; }
    set
    {
        Salt = Crypto.GenerateSalt();
        password = Crypto.HashPassword(value, Salt);
    }
}
```

登录校验：

```csharp
// Server/MirEnvir/Envir.cs:3882-3884
p.Password = Utils.Crypto.HashPassword(p.Password, account.Salt);
if (string.CompareOrdinal(account.Password, p.Password) != 0)
```

### 6.3 为什么这是「quirk」而不是「编码选择」

24 个**随机**字节几乎必然不是合法 UTF-8。`Encoding.UTF8.GetString` 用替换回退（U+FFFD）把非法序列变成一个替换字符，然后再由 `BinaryWriter.Write(string)` 用 UTF-8 重新编码写入文件：

- 原始 24 字节 → 字符串长度 **8~24 个 char**（含 U+FFFD）→ 重新编码后 **24~72 字节**。
- 信息**不可逆**：从文件里的字符串无法还原出那 24 个哈希字节，也无法还原「哪些字节非法」。
- 旁证（**未完全验证**）：在 `masmx86-legend-of-mir-2/Server.MirADB` 头部附近可观察到「长度为 5 的账号名（`never`）紧跟一个长度 **65** 的字符串字段」，与「24 字节哈希经 UTF-8 解码再编码后 24~72 字节」的预期吻合；但该文件整体布局未能完整解析（见 §7.5），所以这条只作参考。

### 6.4 Rust 端要怎么做

迁移期（必须兼容老数据）：
1. 从旧文件按 §2 读出 `Salt`（24 字节）与 `password`（那个乱码字符串，**原样保留**，不要试图 "修复"）。
2. 登录校验：`pbkdf2_hmac::<Sha1>(password, salt, 50, 24)` 得到 24 字节 → 用**与 .NET 完全一致的 UTF-8 替换解码**转成字符串 → 与存档字符串做字节级比较。
   Rust 的 `String::from_utf8_lossy` 与 .NET 的替换策略在「一个非法序列产出几个 U+FFFD」上可能不同（.NET 按最长非法前缀替换、Rust 按 Unicode 最大子部分替换）。**必须用真实样本对拍**，否则老账号会全部登录失败。
3. 新数据：**保持 Crystal 的「哈希 → UTF-8 替换解码」语义，落库前再做一次标准 Base64**。
   当前实现（`crates/mir-store/src/password.rs`、迁移 `0010_password_hash_base64.sql`）：

   * `accounts.salt BYTEA`：24 字节盐（与 Crystal 相同）；
   * `accounts.password_hash TEXT`：`base64(hash_password(password, salt).as_bytes())`，
     其中 `hash_password` 返回的仍是 Crystal 的 lossy 字符串（`String::from_utf8_lossy`
     对 24 字节 PBKDF2 派生值做替换解码）；标准 Base64 只含 `A-Z a-z 0-9 + / =`，
     不会出现 NUL；
   * 登录校验：用账号自带的 salt 重算 → `encode_hash` → 常量时间比较（`verify_password`）；
   * 迁移 0010 把旧行转换一次：`encode(convert_to(password_hash, 'UTF8'), 'base64')`，
     旧值里的 U+FFFD 与 NUL 都按原字节保留在 Base64 解码结果里，老账号无需重置口令。

   **为什么选 Base64 TEXT 而不是 BYTEA**：

   1. **不需要版本列/兼容分支**：旧行的「lossy 字符串」和新代码的「重算后 Base64」在
      同一列、同一验证语义下比较；如果改成 `BYTEA`，旧行是 lossy 字符串、新行是 24 字节
      派生值，必须加 `password_version` 列或按长度猜格式，才能决定怎么比较。
   2. **不引入新依赖/新存储类型**：`mir-store` 自带标准 Base64 编码（与 PostgreSQL
      `encode(bytea,'base64')` 逐字符一致），迁移也只是文本变换；`BYTEA` 需要额外处理
      `Vec<u8>` 与 ORM/查询参数的类型映射，收益仅是少 8 字节/行。
   3. **避免原问题的根因**：直接存 lossy 字符串会因 24 个随机字节里出现 `0x00`
      （合法 UTF-8 但 PostgreSQL TEXT 拒绝）触发 `22021 invalid byte sequence`；Base64
      把任意字节映射成 ASCII，彻底绕开该错误，同时保留全部原始字节供验证。

   将来若要升级到 Argon2id，可以新增 `password_algo` 列并渐进回填；那是**新算法迁移**，
   与本次「不破坏老 Crystal 口令」的目标分开做。

   安全提示（照抄即可，不必照抄漏洞）：50 次 SHA1-PBKDF2 + 24 字节输出 + 序数比较，在今天
   属于弱配置；Rust 版建议 Argon2id 并将 `WrongPasswordCount` 落库（原版不落盘，见 §7.4）。

---

## 7. 存档 / 读档的触发点与校验

### 7.1 时间线（谁在什么时候写盘）

| 时机 | 行为 | 出处 |
|---|---|---|
| 服务器启动，`Server.MirDB` 不存在 | 先 `SaveDB()` 建空库 | `Envir.cs:2825-2828` |
| 启动 | `StartEnvir()` → `LoadDB()`，随后加载 Buff/Recipe 等配置 | `Envir.cs:3308-3321`（注意：**`LoadDB()` 的返回值被忽略**） |
| 启动 | `LoadAccounts()` 读账号；文件不存在则先 `SaveAccounts()` | `Envir.cs:2963-2966` |
| 运行期，每 `Settings.SaveDelay` 分钟（默认 **5** 分钟） | `BeginSaveAccounts()` + `SaveDB()` + `SaveGuilds()` + `SaveGoods()` + `SaveConquests()` | `Envir.cs:2147-2155`、`Server/Settings.cs:102`（`SaveDelay = 5`）、`Server/Settings.cs:16`（`Minute = 60000` ms） |
| 玩家登录选角 | `new PlayerObject(info, this)`（角色已在内存，不重新读盘） | `MirConnection.cs:1122` |
| 玩家进入游戏 | `Info.LastLoginDate = Envir.Now` | `PlayerObject.cs:263` |
| 玩家登出 | 只写 `Info.LastIP` 与 `Info.LastLogoutDate`；其余状态因直写代理已实时在 `Info` 里 | `PlayerObject.cs:419-420`、`HumanObject.cs:59-103` |
| 服务器停止 | `Envir.Stop()` → `Running = false` → 工作线程退出后 `SaveAccounts(); SaveGuilds(true); SaveConquests(true);` | `Envir.cs:3264-3266`、`Envir.cs:2199-2203`、调用方 `Server.MirForms/SMain.cs:205,211` |
| 角色删除超期 / 长期未登录 | 读档时搬到 `Archive/{Name}__MMddyyyy_HHmmss.MirCA` | `AccountInfo.cs:143-162`、`Envir.cs:2537-2548` |
| 行会/NPC/攻城变更 | 只写「脏」的（`NeedSave` 标志） | `Envir.cs:2629-2648`、`Envir.cs:2737-2740` |

**没有**以下机制：单角色增量保存、写前日志（WAL）、事务、行级锁。账号文件是**整文件重写**：

```
写 Server.MirADBn  → 若已有 Server.MirADB 则改名为 Server.MirADBo
                   → 把 .MirADBn 改名成 .MirADB → 删除 .MirADBo
```
（`Envir.cs:2550-2569`；异步版 `Envir.cs:2770-2819` 用 `Saving` 标志互斥）。`SaveDB` 则先复制旧库到 `Back Up/Database/*.bak` 再 `File.Create` 覆盖（`Envir.cs:2441-2455`）。

> Rust/PostgreSQL 版本的收益点：把「5 分钟整库重写 + 崩溃丢 5 分钟」换成「事务内按角色 UPDATE + 定时快照」，语义等价且不会因为一个坏好友记录毁掉整库。

### 7.2 脏标记

只有行会、NPC 商店、攻城有 `NeedSave`（`GuildInfo.cs:32`、`Server/MirObjects/NPCObject.cs:112`、`Envir.cs:3156`）。**账号和角色没有脏标记**——周期保存时无条件序列化所有账号。

### 7.3 读档校验（什么会丢、什么会拒）

| 检查 | 行为 | 出处 |
|---|---|---|
| `LoadVersion < MinVersion(60)` | 拒绝加载，入队一条错误消息，`return false` | `Envir.cs:2836-2840` |
| `LoadVersion > Version(117)` | 同上（"数据库版本高于服务端"） | `Envir.cs:2841-2845` |
| 调用方忽略返回值 | `StartEnvir` 直接 `LoadDB();`，**版本不符时服务器会带着空世界继续跑** | `Envir.cs:3321` |
| 物品模板缺失 | `Envir.BindItem(item)` 为 false → 该物品被**静默置空**（背包、仓库、邮件、拍卖均如此） | `CharacterInfo.cs:197-201`、`AccountInfo.cs:183-186`、`MailInfo.cs:66-68` |
| 技能模板缺失 | `magic.Info == null` → 丢弃 | `CharacterInfo.cs:229-234` |
| 任务模板缺失 | 孤儿任务 → 丢弃并打印 | `CharacterInfo.cs:266-277` |
| 智能宠物模板缺失 | 丢弃 | `CharacterInfo.cs:296-299` |
| 好友角色已删除 | 读时保留（`FriendInfo.Info` 惰性查询），**写时仍按 `Friends.Count` 写** → 见 §3.3(2) 的写坏风险 | `CharacterInfo.cs:534-539` |
| 账号名重复 | 无检查（唯一性只在创建时校验） | `Envir.cs:3720` |
| 角色名重复 | 建角色时 `CharacterExists` | `Envir.cs:4056` |

### 7.4 校验与限流（创建/登录）

| 规则 | 值 | 出处 |
|---|---|---|
| 账号名 | `^[A-Za-z0-9]{3,15}$` | `Envir.cs:183`、`Globals.cs:7-8` |
| 密码 | `^[A-Za-z0-9]{5,15}$` | `Envir.cs:184`、`Globals.cs:10-11` |
| 邮箱 | `\w+([-+.]\w+)*@\w+([-.]\w+)*\.\w+([-.]\w+)*`，≤50 | `Envir.cs:185,3693` |
| 真实姓名 | ≤20 | `Envir.cs:3700` |
| 密保问题/答案 | 各 ≤30 | `Envir.cs:3706-3716` |
| 角色名 | `^[\u4e00-\u9fa5_A-Za-z0-9]{3,15}$`（可中文） | `Envir.cs:186`、`Globals.cs:13-14` |
| 单账号角色数 | `Globals.MaxCharacterCount = 4`（只算未删除） | `Globals.cs:15`、`Envir.cs:4047` |
| 同 IP 建号 | 1 小时内 >2 个账号 → 封 IP 24h | `Envir.cs:3655-3674` |
| 同 IP 建角色 | 1 小时内 >4 个 → 封 IP 24h | `Envir.cs:3985-4002` |
| 密码错误 | ≥5 次 → 账号 `Banned`，`ExpiryDate = Now + 2min` | `Envir.cs:3886-3897` |
| `WrongPasswordCount` | **不落盘**，重启清零 → 可被「重启绕过」猜密码 | `AccountInfo.cs` 无读写（§2） |
| 强制改密 | `RequirePasswordChange` 时只允许改密流程 | `Envir.cs:3905-3909` |
| 封禁角色 | 角色级 `Banned/ExpiryDate` 同样在选角时判断 | `PlayerObject`/`Envir` 登录流程 |

### 7.5 与真实数据的对拍结果

- `Server.MirDB`：已用代码推导的解析器**完整消费**了 `resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB`（452,106 字节，`leftover = 0`），细节见 `mirdb-format.md` §8。
- `Server.MirADB`：资源库中两个样本（`justkkalways-crystal-mir2-env-db/dbbackup/Server.MirADB`，version=110；`masmx86-legend-of-mir-2/Server.MirADB`，version=116）**无法**用当前（117）代码的字段顺序完整解析：头部计数器能自洽读出（如 v116 样本 `NextAccountID=11, NextCharacterID=10, NextUserItemID=320322`），但在「英雄列表 → 账号」边界处即错位。
  结论（**未验证**，仅为推断）：这两个文件来自旧版或二次开发的服务端，`MirADB` 中 `HeroInfo`/`AccountInfo` 的字段排布与参考源 117 版不同（很可能是英雄字段集合或账号自定义字段有差异）。
  → 迁移前必须拿到**与目标版本一致**的 MirADB 样本再实现账号导入器，不要直接照本节布局硬解旧文件。

---

## 8. 本作（PostgreSQL）的存档触发点与 RPO（t85/t86 审计）

角色行（背包/金币/装备/技能/任务/任务物品栏/仓库/好友/宠物模式/坐标/HP/MP/经验）
**第 9 轮起有周期 autosave**（t86 F1 修复），RPO 已由 autosave 兜底，完整语义见
[`docs/world/persistence.md`](../world/persistence.md)：

| 状态 | 写库时机 | 崩溃窗口 |
|---|---|---|
| 角色整行 | **周期 autosave（默认 30s，只写脏角色）**、首次进世界补属性、每日重置命中、`C.LogOut`、TCP 断开、market/gameshop 事务 | **≤ autosave_secs + 1s（默认 31 秒）** |
| 行会 / 攻城 | dirty → server tick（100ms）；会话动作/下线补写；失败回灌 + 5s 重试闸门 | ≤100ms（失败重试） |
| 邮件 | 每次邮件动作 + 下线补写；失败回灌 + 5s 重试 | 动作后即写 |
| 寄售/拍卖 | 上架/买断/取回/到期成交事务；tick 到期扫描 + 失败 5s 重试 | 动作后即写，有重试 |
| 商城元宝/已购计数 | 每次购买同一事务 | 购买后即写 |
| 账号语言 | 切换立即写 | 立即 |

**周期自动保存（本作扩展）**：配置 `ServerConfig.autosave_secs` / `MIR2_AUTOSAVE_SECS`
（默认 **30**，`0` = 关闭 → 退回 Crystal 的「仅下线存盘」语义）。会话读循环每 250ms 轮询，
用世界时钟（`World::wall_now()`）判定到点 → `Session::autosave_tick()`。**只写脏角色**：
脏判定 = 当前世界快照 != 上次成功写库的角色行（整行 `PartialEq`），在线但没改动的玩家
一次 IO 都不做。Crystal 只在 `PlayerObject.LogOut` 写 `Characters`，进程崩溃 = 回档到登录时刻；
本作的 autosave 是对 Crystal 的扩展。

**写失败与重试**：失败 `tracing::error!`（`session`/`character`/`failures`/`backoff_ms`/`error`）
并保留脏标记；退避 **1s → 2s → 4s → 8s → 16s → 30s（上限 30s）**，成功清零。
行会/邮件 flush 失败会把批次**回灌到世界队列队首**（保序，`Save` 整行覆盖后写更新），
另有 5 秒重试闸门避免 DB 故障时按 100ms tick 捶库。

**RPO 口径**：角色行崩溃丢失窗口 **≤ `autosave_secs` + 1 秒（默认 31 秒）**；
`MIR2_AUTOSAVE_SECS=0` 时退回 Crystal 语义（仅下线存盘，RPO 无上限）。

审计证据（`crates/mir-server/tests/persistence_audit.rs`，18/18）：`autosave_bounds_the_crash_window_to_autosave_secs`
用注入世界 + `World::advance_clock` 快进（不 sleep），断言到点前库里旧值 → 推进 1 秒后新金币已入库
且会话仍连着；另有旧版本拒绝/并发最新胜/同版本仅一胜/失败重试、只写脏角色等用例；
以及 LogOut/断 TCP 后重连一致、并发保存串行化、写失败旧行保持、装备/背包/技能/仓库/任务/攻城/行会字段覆盖。

**并发写**：每角色一把 `save_locks` 互斥锁 + `save_version`（严格递增，旧快照 `Ok(false)` 丢弃），
整行单条 UPDATE 原子；详见 [`docs/world/persistence.md`](../world/persistence.md) §3.1。

**失败语义**：Store 层已有 `tracing::error!` 并返回 `Err`，旧行不变、调用方内存不被改写；
`leave_world` 失败会回灌重试；`mir2_db_save_failed_total` / `mir2_autosave_total` /
`mir2_batch_write_failed_total` 提供失败与自动存档计数（见 `docs/server/metrics.md`）。
**结论**：断线一致性与崩溃一致性均达标（RPO 有界，默认 ≤31 秒）。

---

## 9. 给 Rust + PostgreSQL 的映射建议（MVP 范围）

1. **表**：`accounts`、`characters`、`user_items`（`owner_kind`/`owner_id`/`slot` 三列表示背包/装备/仓库/邮件/拍卖），不要把物品塞进 JSON——`UserItem` 有 20 多个字段且需要按 `UniqueID` 独立寻址。
2. **保留原版 ID 分配器**：`NextAccountID`、`NextCharacterID`、`NextUserItemID`(u64)、`NextMailID`、`NextAuctionID`、`NextGuildID`、`NextHeroID` 都是**全局单调计数器**，迁移时要作为 `sequences` 的起点继续用，避免 ID 复用导致物品串号。
3. **`Flags[1999]`**：建议 `bit(1999)` 或 `bytea(250)`；把 `FlagIndexCount` 固化进代码常量并写进迁移脚本。
4. **`Stats`**：`jsonb` 即可（键是稀疏小整数），但排序/比较逻辑要保留「0 值不存」的语义。
5. **`DateTime`**：原版存的是 `DateTime.ToBinary()`（含 Kind，且多为 `Envir.Now` 这个「服务器启动时刻 + Stopwatch 毫秒」的伪时钟，`Envir.cs:72`、`Envir.cs:85-86`）。迁移时统一转 `timestamptz`，并对 `MinValue`（二进制 0）做 `NULL` 映射。
6. **别复制的设计**：内存对象即持久化对象、整文件重写、无脏标记、`WrongPasswordCount` 不落盘、`Friends` 计数 bug、`UserItem.Slots` 的反语义标志。

---

## 10. 本作 PostgreSQL 映射：迁移 `0009_market.sql`（寄售/拍卖挂单）

Crystal 的 `AuctionInfo` 存在 MirADB 的**全局** `Auctions` 段（见 §5、`Envir.cs:2591-2594` /
`:3018-3029`），不是角色行。本作沿用「全局表」的语义，把每条挂单落成
`market_auctions` 的一行；卖家/买家只存角色主键 + 名字快照，物品仍是
`encode_slots(&[Some(item)])` 的单格 `UserItem[]` 字节块。

### 9.1 表结构

`crates/mir-store/migrations/0009_market.sql`：

| 列 | 类型 | 约束 / 默认 | Crystal 字段 |
|---|---|---|---|
| `id` | `BIGINT` | 主键；服务器显式写号（不建序列） | `AuctionInfo.AuctionID`（`AuctionInfo.cs:12`，`ulong`，`:33`） |
| `item` | `BYTEA` | `NOT NULL`；`encode_slots(&[Some(item)])` | `AuctionInfo.Item`（`:14`） |
| `consignment_date` | `TIMESTAMPTZ` | `NOT NULL DEFAULT now()` | `AuctionInfo.ConsignmentDate`（`:15`，`Envir.Now`） |
| `price` | `BIGINT` | `NOT NULL`；`0..=4294967295`（`uint`） | `AuctionInfo.Price`（`:16`） |
| `current_bid` | `BIGINT` | `NOT NULL DEFAULT 0`；`0..=4294967295` | `AuctionInfo.CurrentBid`（`:16`） |
| `seller_character_id` | `BIGINT` | `NOT NULL REFERENCES characters(id) ON DELETE CASCADE` | `AuctionInfo.SellerIndex`（`:18`） |
| `seller_name` | `TEXT` | `NOT NULL DEFAULT ''` | `SellerInfo.Name`（`:19`、`:96`） |
| `current_buyer_character_id` | `BIGINT` | `NULL`；`REFERENCES characters(id) ON DELETE SET NULL` | `AuctionInfo.CurrentBuyerIndex`（`:18`，`-1` → `NULL`） |
| `current_buyer_name` | `TEXT` | `NULL` | `CurrentBuyerInfo.Name`（`:19`） |
| `expired` | `BOOLEAN` | `NOT NULL DEFAULT FALSE` | `AuctionInfo.Expired`（`:21`） |
| `sold` | `BOOLEAN` | `NOT NULL DEFAULT FALSE` | `AuctionInfo.Sold`（`:21`） |
| `item_type` | `SMALLINT` | `NOT NULL DEFAULT 0`；`0..=2` | `AuctionInfo.ItemType`（`:23`：Consign/Auction/GameShop） |
| `updated_at` | `TIMESTAMPTZ` | `NOT NULL DEFAULT now()` | 本作扩展（最后写入时间） |

索引：

* `market_auctions_seller_idx (seller_character_id, id DESC)`：卖家查自己的挂单
  （Crystal `Account.Auctions` 的 `UserMatch` 分支，`PlayerObject.cs:8393-8399`）；
* `market_auctions_active_idx (sold, expired, id DESC)`：买家搜索只看未售出/未过期
  （`PlayerObject.Match`，`:8318-8325`）。

### 9.2 与 Crystal 的存档差异

1. **顺序**：Crystal 链表按 `AddFirst` 保存（最新在前，`PlayerObject.cs:8302`）；
   本表按 `id DESC` 读回，二者等价（`id` 单调递增）。
2. **`NextAuctionID` 不落列**：Crystal 在 MirADB 里持久化计数器（`Envir.cs:2591`）；
   本作启动时按 `max(id) + 1` 续号。
3. **名字快照**：Crystal 读档后靠 `BindCharacter` 重新绑定卖家/买家
   （`Envir.cs:3018-3029`）；本作把名字冗余在行里，离线卖家也能显示。
4. **写入方式**：Crystal 每次 `SaveAccounts` 重写整个 `Auctions` 段；本作由
   `save_auctions` 在事务内 `DELETE` + 批量 `INSERT` 做整表快照（见
   `crates/mir-store/src/repo.rs` 的 `list_auctions` / `save_auctions`）。
5. **过期清理**：Crystal 只把 `Expired` 置真等卖家取回（`Envir.cs:2396`）；本作在服务器
   启动时 `World::purge_expired_auctions` 把物品/最高出价用系统邮件退回后删除挂单，
   再把清理后的快照写回本表。

Rust 结构：`mir_store::MarketAuction`（存储层）↔ `mir_server::store::AuctionRecord`
（服务端世界层）。字段以迁移文件为准，二者按列名一一映射。

---

## 11. 婚姻 / 师徒字段：复用 `0001_init.sql`，**没有 0017 迁移**

`characters` 表在 `crates/mir-store/migrations/0001_init.sql:102-108` 就已经带了
Crystal `CharacterInfo` 的社交字段（`Married`/`MarriedDate`/`Mentor`/`MentorDate`/
`IsMentor`/`MentorExp`）：

| 列 | 类型 | 语义 |
|---|---|---|
| `married` | `INTEGER NOT NULL DEFAULT 0` | 配偶角色 Index（0 = 未婚） |
| `married_date` | `TIMESTAMPTZ` | 结婚时间 |
| `mentor` | `INTEGER NOT NULL DEFAULT 0` | 师傅/徒弟角色 Index |
| `mentor_date` | `TIMESTAMPTZ` | 拜师/收徒时间 |
| `is_mentor` | `BOOLEAN NOT NULL DEFAULT FALSE` | 该角色是否为师傅方 |
| `mentor_exp` | `BIGINT NOT NULL DEFAULT 0` | 师徒经验池 |

**当前迁移最新为 `0016_conquest_objects.sql`；婚姻/师徒没有新增迁移**（也没有 0017）。
以后再翻迁移列表时不要以为漏了——这些列从 0001 起就在。


