# 物品系统（地面物品 / 拾取丢弃 / 背包与装备 / 持久化）

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：`Server/MirObjects/ItemObject.cs`（地面物品与落点扫描）、
> `Server/MirObjects/PlayerObject.cs`（`PickUp`/`DropItem`/`MoveItem`/`EquipItem`/`RemoveItem`/`GainGold`）、
> `Server/MirObjects/HumanObject.cs`（`GainItem`/`CanGainItem`/`AddItem`/`CanEquipItem`/`RefreshStats`）、
> `Server/MirObjects/MonsterObject.cs`（`Drop`/`DropItem`/`DropGold`）、
> `Server/MirEnvir/Envir.cs`（`GetItemInfo`/`CreateDropItem`/`ReloadDrops`）、
> `Server/MirDatabase/CharacterInfo.cs`（`UserItem[]` 的落盘格式）、
> `Shared/Data/ItemData.cs`（`ItemInfo`/`UserItem`/`Stats`）、`Shared/ServerPackets.cs`（`ObjectItem`/`GainedItem`/…）。
>
> Rust 侧实现：`crates/mir-server/src/items.rs`（数据、地面物品、拾取/丢弃/搬运/穿戴、编解码、UUID）、
> `crates/mir-server/src/gamedata.rs`（模板查找 + 掉落表缓存）、
> `crates/mir-server/src/world.rs`（`Player` 的背包/装备字段、tick、外观包）、
> `crates/mir-server/src/session.rs`（登录读档 / 下线存盘 / `C.PickUp`、`C.DropItem`、`C.MoveItem`）。
> 测试：`crates/mir-server/tests/items.rs`（20 个确定性用例）+ `gamedata.rs` 的模板/掉落表用例。

---

## 1. 结论先行

- **物品模板来自 `Server.MirDB` 段 2**（`MirDb.items`），名字查找**大小写不敏感且忽略空格**
  （`Envir.GetItemInfo(string)`，`Envir.cs:4745-4754`）。
- **地面物品是独立对象**（`GroundItem`，对应 `ItemObject`），与玩家/怪物**共用同一个对象 ID 空间**，
  Crystal 里它们都在 `Envir.Objects` 一条链表上（`Envir.cs:2119-2138`）。
- **掉落落地是确定性的**：`ItemObject.Drop(distance)` 从脚下开始环形扫描，
  第一个空格就是落点；`Settings.DropRange = 4`（`Settings.cs:117`）——**没有随机偏移**。
- **金币是地面物品的另一种形态**：`ItemObject` 的 `Item` 与 `Gold` 永远只有一个有效
  （`ItemObject.cs:39-40`），本作把金币放在 `GroundItem::gold` 并在协议层发 `S.ObjectGold`。
- **拾取只扫脚下这一格**（`PlayerObject.cs:7491-7497`），不会顺手捡相邻格。
- **背包 46 格 / 装备 14 槽**（`CharacterInfo.cs:75`），任务书里的「40 格」是任务物品栏。
- Crystal **没有 `S.ItemChanged` 这条包**：搬运有 `S.MoveItem`、穿戴有 `S.EquipItem`、
  脱下有 `S.RemoveItem`、丢弃有 `S.DropItem`、获得有 `S.GainedItem`/`S.GainedGold`；
  **外观变化是 `S.PlayerUpdate`**（`HumanObject.UpdateLooks`，`HumanObject.cs:7058-7061`），
  而 `S.ObjectItem` 是**地面物品**的 `GetInfo`（`ItemObject.cs:365-383`）——本作按这个结论实现。
- **存档是 `UserItem[]` 整块**：`int32 count` + 每槽 `bool 有物品?` + `UserItem`
  （`CharacterInfo.cs:431-456` 写、`:190-224` 读）。
- **UUID** 用 `ItemUidGenerator`（`AtomicU64`，时间戳起点 + 读档时按数据库最大值抬升），
  对应 Crystal 的全局 `Envir.NextUserItemID`（`Envir.cs:127`，**存在账号库文件头里**，
  `:2579` 写 / `:2975` 读）。

---

## 2. 常量速查

| 名称 | 值 | Rust 常量 | Crystal |
|---|---|---|---|
| 背包格数 | 46 | `items::INVENTORY_SIZE` | `CharacterInfo.cs:75`（`new UserItem[46]`） |
| 装备槽数 | 14 | `items::EQUIPMENT_SLOTS` | `Shared/Enums.cs:954-970` |
| 腰带区结束 | 6 | `items::BELT_SIZE` | `HumanObject.cs:182`（`BeltSize => 6`） |
| 药水优先区 | 0..4 | `items::POTION_BELT_MAX` | `HumanObject.cs:178-179` |
| 护身符优先区 | 4..6 | `items::AMULET_BELT_MIN/MAX` | `HumanObject.cs:180-181` |
| 背包扩容上限 | 86 | `items::MAX_INVENTORY_SIZE` | `CharacterInfo.cs:596` |
| 地面物品存活 | **3 分钟**（本作） | `items::ITEM_EXPIRE_MS` | `Settings.ItemTimeOut = 30` 分钟（`Settings.cs:115`）+ `ItemObject.cs:56-58` |
| 掉落归属保护 | 1 分钟 | `items::GROUND_ITEM_OWNER_MS` | `Settings.Minute`（`MonsterObject.cs:1152-1156`） |
| 掉落搜索半径 | 4 | `items::DROP_RANGE` | `Settings.DropRange = 4`（`Settings.cs:117`） |
| 同格物品堆上限 | 5 | `items::DROP_STACK_SIZE` | `Settings.DropStackSize = 5`（`Settings.cs:118`） |
| 单堆金币上限 | 2000 | `items::MAX_DROP_GOLD` | `Settings.MaxDropGold = 2000`（`Settings.cs:228`） |
| 全局掉率倍率 | 1 | `items::DROP_RATE` | `Settings.DropRate = 1F`（`Settings.cs:112`） |
| 玩家负重曲线 | `50 + Level²/Gain` | `combat::weight_cap` | `Shared/BaseStats.cs:18,33,48,65,78` + `StatFormula.Weight`（`:180`） |

> 负重 `Gain`：战士 3、法师 5、道士 4、刺客 3.5、弓箭手 4（同一批行）。
> **假设**：玩家基础负重 = `Base + (Level / Gain) * Level`（浮点除法后截断），
> 例如 1 级战士 `50 + 1/3 = 50`、50 级战士 `50 + (50/3)*50 = 883`。

---

## 3. 掉落（`World::drop_monster_loot`）

对应 `MonsterObject.Drop`（`MonsterObject.cs:1093-1145`）：

```
if (Master == null && EXPOwner != null) Drop();                    // :997-998
  for drop in Info.Drops:                                          // :1104-1144
      reward = drop.AttemptDrop(...)                               // 本作 = DropTable::roll
      if reward.Gold  > 0: DropGold(bonus_gold)                    // :1114-1117
      for item in reward.Items:
          item = Envir.CreateDropItem(dropItem)                    // :1121
          if (!DropItem(item)) return;                             // :1141 —— 没落点就中止后续掉落
```

要点：

1. **没有击杀者就不掉**：`expo_owner` 为空（例如 GM 直接删怪）时整段跳过。
2. **掉落表来源**：`MonsterInfo.DropPath`，为空时用怪物名；路径是
   `<Server.MirDB 所在目录>/Envir/Drops/<名字>.txt`（Crystal `Settings.DropPath = ./Envir/Drops`，
   `Settings.cs:28`；拼法见 `Envir.cs:5405-5419`）。文件不存在 → 这只怪不掉东西（Crystal 同样静默）。
3. **掉落表按名字惰性加载并缓存**（`GameData::monster_drop_table`），掷骰用
   `DropTable::roll(&mut world.rng, 1)`（`crates/mir-db/src/drops.rs`，格式见 `docs/store/drops.md`）。
4. **物品实例**：`Envir.CreateDropItem`（`Envir.cs:4407-4424`）——

   ```
   UniqueID = ++NextUserItemID
   MaxDura  = info.Durability
   CurrentDura = min(Durability, Random.Next(Durability) + 1000)
   if (!NeedIdentify) Identified = true
   ```

   随机数顺序照抄：**先抽耐久**，掉落表掷骰的随机数消耗在它之前。
5. **落点**：`ItemObject.Drop(Settings.DropRange)`（`ItemObject.cs:190-275`）确定性环形扫描：
   `d = 0..=4`，每圈扫 `y-d..=y+d`，行内步长 `abs(y-cy)==d ? 1 : 2d`；
   第一个「不阻挡且同格物品数 < 5」的空格直接落；全是半满格就落物品最少的那格；都没有就 `None`。
   简化：Crystal 还会跳过本图 `Movement` 起点（传送门，`:211-220`），世界层没有运行时 `Movements` 表，省略。
6. **归属**：`Owner = EXPOwner`、`OwnerTime = now + 1 分钟`（`MonsterObject.cs:1152-1156`），
   到点由 `tick_ground_items` 清空（`ItemObject.cs:138-139`）。
7. **金币拆堆**：`drop_gold`，`count = gold/2000 == 0 ? 1 : gold/2000 + 1`，
   最后一堆是 `gold % 2000`（`MonsterObject.cs:1177-1187`）。
   简化：Crystal 在整除时会建出一堆 **0 金币**（客户端显示「金币 0」），本作跳过空堆。

---

## 4. 地面物品（`GroundItem`）与视野同步

```rust
pub struct GroundItem {
    pub object_id: u32,
    pub item: Option<UserItem>,   // None = 金币堆
    pub gold: u32,                // 物品堆恒为 0
    pub position: Point,
    pub map: String,
    pub expire_time: i64,
    pub owner: Option<u32>,
    pub owner_time: i64,
}
```

- 进视野发 `S.ObjectItem`（物品）或 `S.ObjectGold`（金币），出视野/被捡走/到期发 `S.ObjectRemove`，
  与怪物那套 `visible_monsters` 完全同构（`Player::visible_items` + `sync_ground_item_view` /
  `sync_player_ground_items`）。Crystal 的 `AddObjects`/`RemoveObjects` 在 `HumanObject.cs:7864`、`:8152`。
- 显示名：先按 `ItemInfo.FriendlyName` 去掉**结尾数字**与**方括号段**（`ItemData.cs:52-62`），
  再按 `Count > 1` 拼成 `名字 (数量)`（`ItemObject.cs:371`）。本作实现见 `items::friendly_name`。
- 图标：`UserItem.Image` 会按数量给**护身符/毒药**换图（`ItemData.cs:641-677`），
  其余物品就是模板图标。本作实现见 `items::item_image`。
- 到期：`ItemObject.Process` 的 `if (Envir.Time > ExpireTime)`（`ItemObject.cs:131`，**严格大于**），
  由 `World::tick` 末尾的 `tick_ground_items` 执行。
- 世界 tick 顺序：`玩家簿记 → 到期伤害 → 刷怪点 → 怪物 AI → 地面物品到期`。
  Crystal 只有一个对象链表按插入顺序轮转，这里把地面物品放最后，保证本 tick 新生成/拾取的物品
  不会在同一 tick 里被判过期。

---

## 5. 拾取（`World::pickup`）

对应 `PlayerObject.PickUp`（`PlayerObject.cs:7483-7537`）：

| # | 判定 | Crystal |
|---|---|---|
| 1 | 死了不能捡 | `:7485-7489` |
| 2 | **只扫脚下这一格** | `cell = CurrentMap.GetCell(CurrentLocation)`，`:7491-7497` |
| 3 | 归属不是自己（且不是队友）→ 记失败、看下一件 | `:7501-7505` |
| 4 | 物品：`CanGainItem` 为假就跳过 | `:7510`、`HumanObject.cs:7586-7628` |
| 5 | 物品：`GainItem` → 本人 `S.GainedItem` → 从地面移除 | `:7516-7521`、`HumanObject.cs:7824-7835` |
| 6 | 金币：`CanGainGold` 为假就跳过；`GainGold` → 本人 `S.GainedGold` | `:7526-7531`、`:7638-7683` |
| 7 | 一件都没捡到且有归属失败 → 系统聊天「不能捡别人的东西」 | `:7534-7535`（本作返回 `ItemError::NotOwner` 交给会话层） |

- `CanGainItem`（`HumanObject.cs:7586-7628`）：**有空位就成功**；否则只有可堆叠物品在
  「现有堆 + 新数量 ≤ StackSize」时才算成功。
- `AddItem`（`HumanObject.cs:1577-1630`）的落位顺序：先并入已有堆 → 药水/卷轴/`Script+Effect==1`
  用 `0..4` → 护身符用 `4..6` → 其余从 6 号格开始 → 都不行再从 0 号格找。
- **负重（本作扩展）**：Crystal 拾取时**不查重量**，重量只让 `CanRun` 失败
  （`HumanObject.cs:135`、`:2516`）。本作按需求在拾取时也拒绝超重，
  `TooHeavy { weight, limit }`，可用 `World::set_enforce_bag_weight(false)` 关掉。
- 背包满时 Crystal 是静默跳过（`:7510` 的 `continue`）；本作返回 `ItemError::InventoryFull`，
  只在日志/测试里可见，协议层不发包。

---

## 6. 丢弃（`World::drop_item`）

对应 `PlayerObject.DropItem`（`PlayerObject.cs:7359-7472`）+ `HumanObject.DropItem`（`HumanObject.cs:1380-1390`）：

1. 死亡 / 找不到 UUID / 数量夹取（`count == 0 || count > Count → Count`，`:7344`）；
2. `BindMode.DontDrop` 拒绝（`:7416-7420`）；
3. **先找落点，再动背包**（Crystal 的 `if (!DropItem(temp)) { Enqueue(p); return; }` 在清空背包格之前，
   `:7430-7444`）；
4. 整件丢：清空背包格；拆开丢：`Envir.CreateFreshItem` + `Count = count`，原堆 `-= count`（`:7449-7457`）；
5. 肉掉 2000 点耐久（`HumanObject.cs:1386-1387`）；
6. 本人收 `S.DropItem{UniqueID, Count, HeroItem = false, Success}`（`:7361`、`:7459-7460`）；
7. 落地后视野内所有人（含自己）收 `S.ObjectItem`（`ItemObject.Drop` → `Spawned()`）。

简化：地图 `NoThrowItem`（`:7368-7373`）、英雄背包、租借/封印/婚戒等绑定规则未实现。

---

## 7. 背包交换 / 装备穿戴（`World::move_item`）

### 7.1 `MirGridType::Inventory`

`PlayerObject.MoveItem` 的 `Inventory` 分支（`PlayerObject.cs:5350-5374`）：

- 两个下标都要在 `0..Inventory.Length`；
- `from` 必须是**有物品**的格（`:5352-5358`）；
- 直接 `swap`（目标格可以是空），本人收 `S.MoveItem{Grid, From, To, Success}`（`:5369-5370`）。
- **没有 UUID 可校验**：`C.MoveItem` 只有 `Grid/From/To`（`Shared/ClientPackets.cs`），
  按 UUID 定位的是 `C.EquipItem`/`C.DropItem`/`C.UseItem`。

### 7.2 `MirGridType::Equipment`

Crystal 把穿/脱拆成两条客户端包：

- 穿上 = `C.EquipItem{Grid = Inventory, UniqueID, To = 装备槽}`（`MirConnection.cs:1297-1301`
  → `PlayerObject.EquipItem`，`:5527-5693`）；
- 脱下 = `C.RemoveItem{Grid = Equipment, UniqueID, To = 背包格}`（`MirConnection.cs:1303-1307`
  → `PlayerObject.RemoveItem`，`:5013-5135`）。

本作的客户端包只有 `C.MoveItem`，所以 `World::move_equipment` 按下面的顺序判方向：

1. `to` 是装备槽、`from` 是背包格、`can_equip_item` 通过 → **穿上**
   （旧装备换回 `from` 号背包格，`:5674-5678`；发 `S.EquipItem{Grid = Inventory, To = 槽, Success = true}`）；
2. `from` 是有物品的装备槽、`to` 是背包格 → **脱下**
   （目标格必须为空，否则 `SlotOccupied`；发 `S.RemoveItem{Grid = Equipment, To = 背包格, Success = true}`）；
3. 都解释不了 → `CannotEquip` / `SlotOutOfRange`，并发 `S.MoveItem{Success = false}`。

成功后 `refresh_stats` 重算属性；**外观变了**才广播 `S.PlayerUpdate{ObjectID, Light, Weapon,
WeaponEffect, Armour, WingEffect}`（`HumanObject.UpdateLooks`，`HumanObject.cs:7058-7061`；
`GetUpdateInfo`，`:7046-7057`）。

> 注意：这一版 Crystal **没有** `S.EquipmentChanged` 这条包；`S.ObjectItem` 也不是装备外观，
> 而是地面物品的 `GetInfo`。所以任务书里「`S.ObjectItem` → 实际是 `S.EquipmentChanged`？」的答案是
> ——都不是，是 `S.PlayerUpdate`。

### 7.3 装备槽映射表（`EquipmentSlot`，`Shared/Enums.cs:954-970`）

| 槽位 | 编号 | 接受的 `ItemType`（`CanEquipItem`，`HumanObject.cs:7663-7721`） | 默认落位（`equipment_slot_for`） |
|---|---|---|---|
| Weapon | 0 | `Weapon` | 武器 |
| Armour | 1 | `Armour` | 衣服 |
| Helmet | 2 | `Helmet` | 头盔 |
| Torch | 3 | `Torch` | 火把/蜡烛 |
| Necklace | 4 | `Necklace` | 项链 |
| BraceletL | 5 | `Bracelet` | 左手镯 |
| BraceletR | 6 | `Bracelet` **或** `Amulet` | 右手镯（护身符也可以） |
| RingL | 7 | `Ring` | 左戒指 |
| RingR | 8 | `Ring` | 右戒指 |
| Amulet | 9 | `Amulet` | 护身符 |
| Belt | 10 | `Belt` | 腰带 |
| Boots | 11 | `Boots` | 鞋 |
| Stone | 12 | `Stone` | 宝石 |
| Mount | 13 | `Mount` | 坐骑 |

槽位是**客户端指定**的（`C.EquipItem.To`），服务端只用 `slot_accepts` 校验，所以双戒指/双护腕由玩家挑槽；
「默认落位」仅供测试与文档使用。

`can_equip_item` 的检查顺序（`HumanObject.cs:7661-7823`）：
槽位类型 → 性别（`RequiredGender` 位掩码）→ 职业（`RequiredClass` 位掩码）→
`RequiredType`/`RequiredAmount`（等级或某项属性）→ 手/身重量上限。
简化：骑乘时不能换装（`:7817-7820`）、诅咒/封印/灵魂绑定（`:5624-5656`）未实现。

### 7.4 属性叠加（`World::refresh_stats`）

`HumanObject.RefreshStats`（`HumanObject.cs:1730-1769`）的四步：

1. `RefreshLevelStats()` → 等级/职业曲线（`combat::player_base_stats`，含三个负重上限）；
2. `RefreshEquipmentStats()` → 逐件装备 `Stats.Add(realItem.Stats)` + `Stats.Add(temp.AddedStats)`
   （`:1858-1859`），**耐久为 0 且模板有耐久的装备不提供属性**（`:1836`）；
3. `RefreshStatCaps()` → 职业上限（`MagicResist 2` / `CriticalRate 18` / `CriticalDamage 10`，
   `Shared/BaseStats.cs:91-99`）+ 非负 + `MinDC <= MaxDC`；
4. `if (HP > Stats[HP]) SetHP(Stats[HP])`。

简化：觉醒属性（`:1861-1874`）、套装/套装加成（`:1893-1914`）、镶嵌宝石（`RefreshSocketStats`）、
末尾的百分比加成（`:1751-1759`）、技能/buff/行会加成（`:1745-1747`）都未实现。

外观（`World::looks`）：盔甲给 `Armour = Shape`、`Wings = Effect`；武器给
`Weapon = Shape`、`WeaponEffect = Effect`；`Light` 取所有装备的最大值
（`HumanObject.cs:1838-1876`）。空手时 `Weapon = -1`（字段初值，`:164`）。

---

## 8. 持久化

### 8.1 字节格式（`CharacterInfo.Save`，`CharacterInfo.cs:431-456`）

```
int32  count            // 槽位数（背包 46、装备 14）
count × {
    bool  present       // true = 这一槽有物品
    UserItem item       // present 为 true 时才写
}
```

`UserItem.Save`（`Shared/Data/ItemData.cs:460`）的字段顺序由 `mir-protocol` 的
`data::user_item` 实现（与包路径逐字节相同），这里直接复用：

```
u64 UniqueID, i32 ItemIndex, u16 CurrentDura, u16 MaxDura, u16 Count, i32 SoulBoundId,
u8 Bools(bit0=Identified, bit1=Cursed), i32 Slots.Length, Slots…（空槽只写 bool 占位）,
u16 GemCount, Stats AddedStats, Awake, u8 RefinedValue, u8 RefineAdded,
i32 RefineSuccessChance, i32 WeddingRing, bool ExpireInfo?, bool RentalInformation?,
bool IsShopItem, bool SealedInfo?, bool GMMade
```

> 反直觉点：**背包/装备/仓库的槽位标志是正语义**（`true` = 有物品，`CharacterInfo.cs:434`），
> 而 `UserItem.Slots` 里的**镶嵌槽是反语义**（`true` = 空，`ItemData.cs:476-480` 的
> `writer.Write(Slots[i] == null)`）。
> 这两处千万别写混（`docs/store/mirdb-format.md` §8 也记了这个坑）。

### 8.2 读档 / 存档路径

- 登录（`C.StartGame`）：`decode_inventory(character.inventory)` / `decode_equipment(...)`，
  空字节块 → 空背包/空装备栏；解析失败 → 记 `warn` 并按空处理，不拒绝登录。
  解码后把每个 `unique_id` 交给 `World::observe_item_uids`，避免新物品撞号。
- `UserInformation`：`inventory`/`equipment` 按**当前内容**下发（不再发空数组），
  `gold` 取当前金币（`i64` 截到 `u32`，Crystal 的金币上限就是 `uint.MaxValue`）。
- 下线（`Session::leave_world`）：从世界取回玩家，`encode_slots` 写回
  `Character.inventory` / `Character.equipment`，金币/HP/MP/等级/经验/坐标一起同步，
  然后 `store.save_character`。
- 槽位数比本作支持的多（Crystal 扩容背包，最多 86 格，`CharacterInfo.cs:596`）：
  **多的槽位丢弃并记 warn**（还没有扩容功能）；少的补空。

---

## 9. UUID（`ItemUidGenerator`）

| | Crystal | 本作 |
|---|---|---|
| 计数器 | `Envir.NextUserItemID`（`ulong`） | `ItemUidGenerator`（`AtomicU64`） |
| 取值 | `UniqueID = ++NextUserItemID`，从 1 开始（`Envir.cs:4394`、`:4413`） | `fetch_add(1) + 1` |
| 持久化 | 写在**账号库**文件头（`:2579` 写、`:2975` 读） | 每个 `UserItem.unique_id` 跟着角色行存；启动时按「时间戳起点」+ 登录时 `observe(数据库最大值)` 双保险 |
| 起点 | 0 | `Unix 毫秒 << 20`（低 20 位留给同毫秒自增） |

差异说明：Crystal 是 1、2、3… 的紧凑序列；本作是从时间戳起跳的大数。
对客户端没有可观察差异（UUID 只用来寻址物品），但**跨进程重启也不会撞号**，
而且不需要额外的全局表。`observe` 用 CAS 单调抬升，多会话并发读档也安全。

---

## 10. 协议包一览（本作实际发的）

| 场景 | 包 | 收件人 | Crystal |
|---|---|---|---|
| 地面物品出现 | `S.ObjectItem` / `S.ObjectGold` | 视野内所有人（含丢的人） | `ItemObject.GetInfo`（`ItemObject.cs:365-383`） |
| 地面物品消失 | `S.ObjectRemove` | 看见过它的人 | `MapObject.Despawn`（`MapObject.cs:363`） |
| 拾到物品 | `S.GainedItem{Item}` | 本人 | `HumanObject.GainItem`（`:7831`） |
| 拾到金币 | `S.GainedGold{Gold}` | 本人 | `PlayerObject.GainGold`（`:7682`） |
| 扣金币（GM/后续） | `S.LoseGold{Gold}` | 本人 | `PlayerObject.DropGold`（`:7481`） |
| 丢弃 | `S.DropItem{UniqueID, Count, HeroItem, Success}` | 本人 | `PlayerObject.DropItem`（`:7361`、`:7459`） |
| 背包内移动 | `S.MoveItem{Grid, From, To, Success}` | 本人 | `PlayerObject.MoveItem`（`:5289`、`:5369`） |
| 穿戴 | `S.EquipItem{Grid, UniqueID, To, Success}` | 本人 | `PlayerObject.EquipItem`（`:5529`、`:5682`） |
| 脱下 | `S.RemoveItem{Grid, UniqueID, To, Success}` | 本人 | `PlayerObject.RemoveItem`（`:5015`、`:5123`） |
| 外观变化 | `S.PlayerUpdate{ObjectID, Light, Weapon, WeaponEffect, Armour, WingEffect}` | 周围玩家（不含本人） | `HumanObject.UpdateLooks`（`:7058-7061`） |

---

## 11. 与 Crystal 的差异

| 本作 | Crystal | 原因 |
|---|---|---|
| 拾取时检查背包负重 | 拾取不查重量，只在 `CanRun` 里查（`HumanObject.cs:135`） | 任务要求的扩展；`set_enforce_bag_weight(false)` 可还原 |
| 地面物品 3 分钟过期 | `Settings.ItemTimeOut = 30` 分钟（`Settings.cs:115`） | 调试友好默认；`set_item_expire_ms` 可调 |
| 丢弃/穿脱统一走 `C.MoveItem` | 三条包 `C.MoveItem`/`C.EquipItem`/`C.RemoveItem` | 客户端目前只发 `C.MoveItem`；方向判定见 §7.2 |
| 背包满时返回 `InventoryFull` | 静默 `continue`（`PlayerObject.cs:7510`） | 便于日志定位，协议层表现一致（不发包） |
| 金币 0 的堆不生成 | 整除时会建出 0 金币堆（`MonsterObject.cs:1177-1187`） | 避免客户端显示「金币 0」 |
| 落点跳过 `Movement` 不实现 | `ItemObject.cs:211-220` 跳过传送门起点 | 世界层没有运行时 `Movements` 表 |
| 掉落物不带 `GMMade`/任务物品消耗 | `MonsterObject.cs:1125-1140` | 没有 GM 制造与任务系统 |
| 背包扩容上限 86 只用于解码校验 | `ResizeInventory`（`CharacterInfo.cs:594-607`） | 还没有扩容道具/功能 |

---

## 12. 发现的 Crystal 异常行为

1. **`CanGainItem` 的可堆叠分支会在空槽上解引用 `null`**——如果背包**有**空格却在进入分支前没有返回。
   实际上它先 `if (FreeSpace > 0) return true;`（`HumanObject.cs:7588-7591`），所以走不到；
   但两个分支的空值检查不一致（`Amulet` 分支有 `bagItem == null ||`，通用分支没有，`:7601` vs `:7619`），
   属于「靠调用顺序侥幸不崩」的写法。
2. **怪物掉落中途失败会吞掉剩余掉落**：`if (!DropItem(item)) return;`（`MonsterObject.cs:1141`）——
   一只怪掉多件时，只要有一件找不到落点，后面所有件都不掉了（不是 `continue`）。
3. **金币拆堆会产出 0 金币的 `ItemObject`**：`count = gold/MaxDropGold + 1` 且最后一堆取
   `gold % MaxDropGold`（`MonsterObject.cs:1177-1187`），整除时最后一堆是 0。
4. **`gold / MaxDropGold == 0 ? 1 : gold / MaxDropGold + 1` 的语义是「向上取整 + 1」**：
   `gold = 2000` 时会拆成 2 堆（2000 + 0），`gold = 2001` 时拆成 2 堆（2000 + 1）。
5. **`Envir.GetItemInfo(string)` 只去掉模板名里的空格**（`info.Name.Replace(" ", "")`，
   `Envir.cs:4750`），查询串里的空格不去；所以 `"金创药 (小量)"` 在 Crystal 里**查不到**，
   必须写 `"金创药(小量)"` 或 `"金创药 (小量)"` 恰好与模板名一致。本作两边都归一化（更宽松）。
6. **`C.MoveItem` 没有 UUID 字段**，服务端只能按格子搬运（`PlayerObject.cs:5287-5375`），
   所以「UUID 不符要拒绝」这件事在 Crystal 协议里无从谈起。
7. **`ItemObject.Blocking => false`**（`ItemObject.cs:34-37`）：地面物品既不阻挡也不参与移动判定。
8. **`ItemObject.Drop` 的 `x` 步进写法很怪**：
   `x += Math.Abs(y - CurrentLocation.Y) == d ? 1 : d * 2`（`ItemObject.cs:205`）——
   中心行只扫两端两个点，其余行才逐格扫；`d = 0` 时步长退化为 1。照抄后圈形覆盖是「方形环 + 中心」，
   不是严格的螺旋。
9. **`FriendlyName` 会吃掉名字里的方括号内容和结尾数字**（`ItemData.cs:52-62` 的两条正则）：
   模板叫 `[7天]屠龙` 的物品，在客户端上显示为 `屠龙`；叫 `金创药10` 的显示为 `金创药`。
   也就是说**同一个模板在不同用途下的名字可能不一样**（查找用 `Name`、显示用 `FriendlyName`）。
   本作照抄了这个行为（`items::friendly_name`）。
10. **`UserItem.Image` 不是模板 `Image`**：护身符与两种毒药按堆叠数量换图标
    （`ItemData.cs:641-677`，例如护身符 `>=300` 是 3662、`>=200` 是 3661、其余 3660）。
    本作照抄（`items::item_image`）。

---

## 13. 测试清单（`crates/mir-server/tests/items.rs`，20 个）

| # | 用例 | 覆盖点 |
|---|---|---|
| 1 | `item_lookup_accepts_index_and_ignores_case_and_spaces_in_names` | `GameData::item` / `item_by_name` / `item_count` |
| 2 | `monster_death_places_drop_items_and_gold_around_the_corpse` | 掉落生成：金币区间、物品实例、归属、`DropRange`、视野包、耐久抽取 |
| 3 | `monster_without_killer_or_drop_table_drops_nothing` | 没有掉落表 → 不掉 |
| 4 | `ground_item_enters_and_leaves_view_following_player_movement` | 地面物品视野进出 |
| 5 | `ground_item_expires_after_the_configured_timeout_and_broadcasts_remove` | 过期（严格大于）+ `ObjectRemove` |
| 6 | `pickup_moves_item_into_inventory_and_sends_gained_item` | 拾取成功（落位 6 号格）+ `GainedItem` + 旁观者 `ObjectRemove` |
| 7 | `pickup_only_reaches_the_players_own_cell` | 只扫脚下格 |
| 8 | `pickup_fails_when_inventory_is_full` | 背包满 |
| 9 | `pickup_rejects_overweight_but_can_be_disabled` | 超重（扩展）+ 开关 |
| 10 | `pickup_respects_owner_protection_until_it_expires` | 归属保护期 |
| 11 | `picking_up_gold_credits_the_player_and_sends_gained_gold` | 金币入账 + `GainedGold` |
| 12 | `drop_item_removes_it_from_the_bag_and_creates_a_ground_item` | 丢弃成功 + 失败包 |
| 13 | `dropping_part_of_a_stack_keeps_the_rest_in_the_bag` | 拆堆丢弃 |
| 14 | `moving_inside_the_inventory_swaps_two_slots` | 背包交换 + 空格/越界拒绝 |
| 15 | `equipping_a_weapon_applies_its_stats_and_broadcasts_player_update` | 穿装备 → 属性叠加 + `EquipItem` + `PlayerUpdate` |
| 16 | `equipping_checks_slot_type_class_and_required_level` | 槽位类型 / 等级 / 职业 |
| 17 | `unequipping_requires_an_empty_inventory_slot` | 脱下目标格占用 + 属性回落 + `RemoveItem` |
| 18 | `inventory_blob_roundtrips_through_crystal_layout` | 持久化编解码（含嵌套槽位、空块、截断、非法计数） |
| 19 | `item_uid_generator_is_monotonic_and_restores_from_a_database_maximum` | UUID 生成/恢复 |
| 20 | `newly_created_items_get_unique_ids_from_the_generator` | 新物品 UUID 唯一且递增 |

另有 `crates/mir-server/src/gamedata.rs` 的 `sample_database_exposes_item_templates_and_drop_tables`：
用真实 `Server.MirDB` 验证物品模板查找与 `Envir/Drops` 掉落表解析。

---

## 14. 还没做的

- 药水/卷轴的使用（`C.UseItem` → `PlayerObject.UseItem`）、堆叠拆分（`C.SplitItem`）、
  合并（`C.MergeItem`）、仓库/交易/商城/邮件；
- 装备诅咒/封印/租借/婚戒/灵魂绑定、觉醒与镶嵌、套装效果；
- 耐久损耗（`DamageDura`/`DamageWeapon`，`HumanObject.cs:7837-7862`）；
- 任务物品判定与 `BindMode` 的完整语义；
- 背包扩容（`CharacterInfo.ResizeInventory`）；
- `C.EquipItem` / `C.RemoveItem` 两条原始客户端包的接收（目前统一走 `C.MoveItem`）。
