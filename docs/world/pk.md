# 玩家对玩家（PK）与 PK 值

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：
> `Server/MirObjects/PlayerObject.cs`（`IsAttackTarget`、`Die`、`RedDeathDrop`、
> `GetNameColour`、`RefreshNameColour`）、
> `Server/MirObjects/HumanObject.cs`（`Attacked(HumanObject)`、`DeathDrop`、
> `BroadcastColourChange`）、
> `Server/MirObjects/MapObject.cs`（`PKPoints` 衰减、`BrownTime`、`NameColour`）、
> `Server/MirObjects/Monsters/Guard.cs` / `ArcherGuard.cs` /
> `Server/MirObjects/MonsterObject.cs`（守卫 AI）、
> `Server/MirEnvir/Settings.cs`（`PKDelay`、`PlayerDiedItemTimeOut`）、
> `Server/MirNetwork/MirConnection.cs`（`C.ChangeAMode`）。
>
> Rust 侧实现：
> `crates/mir-server/src/combat.rs`（[`resolve_player_vs_player`]）、
> `crates/mir-server/src/world.rs`（攻击目标选择、PvP 受击、PK 值、名字颜色、死亡掉落、
> `C.ChangeAMode`）、
> `crates/mir-server/src/items.rs`（`drop_player_death_loot`）、
> `crates/mir-server/src/monster.rs`（守卫 AI）。
> 测试：`crates/mir-server/tests/pk.rs`（27 个确定性用例）。

---

## 1. 结论先行

- **攻击模式是私有状态**：`C.ChangeAMode` 只给本人回一条 `S.ChangeAMode`，
  Crystal **不广播**给周围玩家（`MirConnection.cs:1432-1439`）。本作照抄。
- **玩家是否能打玩家由目标判定**：`PlayerObject.IsAttackTarget(HumanObject)`
  （`PlayerObject.cs:4650-4674`）先看死亡/安全区/`NoFight`，再按**攻击方**的
  `AttackMode` 决定。组队/行会关系用的是**目标**的队伍/行会。
- **一次 PvP 攻击同样是两段式**：`S.ObjectAttack` 立刻广播，伤害 300ms 后结算
  （`HumanObject.cs:3201`）；与打怪共用 `pending_hits` 队列。
- **PK 值只在「击杀白名/棕名玩家」时增加 100**：受害者已经是红名
  （`PKPoints >= 200`）或棕名时，凶手**不**加分（`PlayerObject.cs:607-611`）。
- **红名阈值 200、黄名阈值 100**，没有 300 分界（`PlayerObject.cs:1855`、`:1886`）。
  PK 值每 **12 秒**自然 -1（`Settings.PKDelay = 12`，`Settings.cs:119`；
  `MapObject.cs:212-216`）。
- **攻击白名玩家会让攻方变棕名 1 分钟**（`HumanObject.cs:7199-7200`）；
  棕名不是红名，但 `RedBrown` 模式下可被攻击。
- **死亡掉落的触发条件**（`PlayerObject.cs:638-641`）：
  `PKPoints > 200` → `RedDeathDrop`（**严格大于**）；
  否则不在安全区 → `DeathDrop`。
- **Crystal 的 `pkbodydrop` 恒为 true**（`HumanObject.cs:1393-1398`），
  所以**被怪物杀死也会爆**。本作照抄源码并在 §10 记为异常。
- **死亡掉落的物品没有归属**（`ItemObject.cs:53-82` 不设置 `Owner`），
  凶手可以捡；存活 120 分钟（`Settings.PlayerDiedItemTimeOut = 120`，`Settings.cs:116`）。
- **守卫只攻击红名**（AI 6/58/113，`PlayerObject.cs:4679`），攻击距离是视野半径、
  防御类型 `AC`；守卫不可被玩家攻击（`Monsters/Guard.cs:54-55`）。

---

## 2. 常量速查

| 名称 | 值 | Rust 常量 | Crystal |
|---|---|---|---|
| 每击杀加 PK 值 | 100 | `PK_POINTS_PER_KILL` | `PlayerObject.cs:611` |
| 红名阈值 | 200 | `RED_NAME_PK_POINTS` | `PlayerObject.cs:1855`、`:4670`、`:4679` |
| 黄名阈值 | 100 | `YELLOW_NAME_PK_POINTS` | `PlayerObject.cs:1886` |
| PK 值衰减间隔 | 12000 ms | `PK_DELAY_MS` | `Settings.cs:119`（`PKDelay = 12` 秒）+ `MapObject.cs:212-216` |
| 棕名时长 | 60000 ms | `BROWN_NAME_DURATION_MS` | `HumanObject.cs:7199-7200`（`Settings.Minute`，`Settings.cs:16`） |
| `LastHitter` 保留 | 10000 ms | `LAST_HITTER_TIMEOUT_MS` | `HumanObject.cs:7195` + `MapObject.cs:218-221` |
| 死亡掉落物存活 | 7200000 ms | `PLAYER_DIED_ITEM_EXPIRE_MS` | `Settings.cs:116`（120 分钟）+ `ItemObject.cs:53-58` |
| 守卫攻击动作锁定 | 500 ms | `GUARD_ATTACK_ACTION_MS` | `Monsters/Guard.cs:76` |
| 守卫伤害延迟 | 300 ms | `GUARD_ATTACK_DAMAGE_DELAY_MS` | `Monsters/Guard.cs:85` |
| 白名颜色 | `-1`（0xFFFFFFFF） | `NAME_COLOUR_WHITE` | `MapObject.cs:67`（`Color.White`） |
| 红名颜色 | 0xFFFF0000 | `NAME_COLOUR_RED` | `PlayerObject.cs:1856`（`Color.Red`） |
| 棕名颜色 | 0xFF8B4513 | `NAME_COLOUR_BROWN` | `PlayerObject.cs:1859`（`Color.SaddleBrown`） |
| 黄名颜色 | 0xFFFFFF00 | `NAME_COLOUR_YELLOW` | `PlayerObject.cs:1887`（`Color.Yellow`） |
| 守卫名字颜色 | 0xFF87CEEB | `NAME_COLOUR_GUARD` | `Monsters/Guard.cs:27`（`Color.SkyBlue`） |

> **Crystal 源码里不存在的设置**（任务书提到但本仓库的 Crystal 分支没有）：
> `Settings.PKPointRate`、`PkDropRate`、`PkPointKillCount`、`RedNameDuration`、
> `PKPointTick`。击杀加分是硬编码的 `+100`，衰减是 `MapObject.Process` 内联的
> `PKDelay` 秒，掉落是 `DeathDrop`/`RedDeathDrop` 里的硬编码概率。本作按源码实现。

---

## 3. 攻击模式（`AttackMode`）

`Player.attack_mode` 对应 Crystal `HumanObject.AMode` → `CharacterInfo.AMode`
（`HumanObject.cs:69-73`）。枚举值与 `Shared/Enums.cs` 一致：

| 值 | 变体 | Rust |
|---|---|---|
| 0 | `Peace` | `AttackMode::Peace` |
| 1 | `Group` | `AttackMode::Group` |
| 2 | `Guild` | `AttackMode::Guild` |
| 3 | `EnemyGuild` | `AttackMode::EnemyGuild` |
| 4 | `RedBrown` | `AttackMode::RedBrown` |
| 5 | `All` | `AttackMode::All` |

`C.ChangeAMode{Mode}`（包号 44）→ `World::change_attack_mode`
（`MirConnection.cs:1432-1439`）：

```csharp
if (Stage != GameStage.Game) return;
Player.AMode = p.Mode;
Enqueue(new S.ChangeAMode { Mode = Player.AMode });
```

本作没有「阶段校验」这一步（会话层已经在 `Stage::Game` 才处理包），
其余照抄：只回本人、**不广播**、未知模式在协议解码时落回 `Peace`。
登录时补发一条 `S.ChangeAMode`（`PlayerObject.cs:1217`）。

---

## 4. 玩家能否攻击玩家

`World::is_attack_target_player(attacker, target)` 逐行移植
`PlayerObject.IsAttackTarget(HumanObject)`（`PlayerObject.cs:4650-4674`）：

```csharp
if (Dead || InSafeZone || attacker.InSafeZone || attacker == this || GMGameMaster) return false;
if (CurrentMap.Info.NoFight) return false;
switch (attacker.AMode) {
    case All:        return true;
    case Group:      return GroupMembers == null || !GroupMembers.Contains(attacker);
    case Guild:      return MyGuild == null || MyGuild != attacker.MyGuild;
    case EnemyGuild: return MyGuild != null && MyGuild.IsEnemy(attacker.MyGuild);
    case Peace:      return false;
    case RedBrown:   return PKPoints >= 200 || Envir.Time < BrownTime;
}
```

实现细节：

* **安全区**：`InSafeZone` 由 `Map.GetSafeZone`（`Map.cs:2386-2395`）判定，
  是 `SafeZoneInfo{X, Y, Size}` 的 Chebyshev 距离。本作按 `MapInfo.SafeZones`
  现算（`World::in_safe_zone`，见 `docs/world/guild.md` 的仓库安全区）。
  **攻方与目标都必须在安全区外**。
* **`NoFight` 地图**：`MapInfo.NoFight` 为真时禁止任何 PvP（`PlayerObject.cs:4655`）。
* **组队**：比较 `group_id`；只有「双方都有队伍且相同」才算队友。
* **行会**：比较 `GuildRef.guild_id`；目标没有行会时可以被同会模式攻击。
* **`EnemyGuild`**：Crystal 要求 `MyGuild.IsEnemy(attacker.MyGuild)`；本作没有
  行会战争/敌对表（见 `docs/world/guild.md`），所以**恒为 false**。
* **`RedBrown`**：目标 `PKPoints >= 200` **或** `now < BrownTime`。
* `GMGameMaster`、`WarZone` 未实现；`BattleRing` 在源码里也不存在（任务书的推测）。

目标选择在 `World::melee_target`：把攻击者前方一格里的玩家与怪物按**对象 ID 升序**
合并（对象 ID 是全局递增分配的，等价于 Crystal 格子里的加入顺序），取第一个
`IsAttackTarget` 为真的对象（`HumanObject.cs:3029-3035`）。守卫永远跳过
（`Monsters/Guard.cs:54-55`）。

---

## 5. 玩家受击：`Attacked(HumanObject)`

`combat::resolve_player_vs_player` 移植 `HumanObject.Attacked(HumanObject)`
（`HumanObject.cs:7092-7221`）。抽签顺序与源码**逐行一致**：

1. `GetArmour`：先掷闪避（`Random.Next(Agility + 1) > Accuracy`），再掷护甲
   （`:7101`，`MapObject.GetArmour`，`MapObject.cs:460-507`）；
2. 未命中 → `DamageType.Miss`，返回 0（`:7103-7106`）；
3. **反射抽签**：`Random.Next(100) < Reflect`（`:7116`，无论 `reflect` 是否为 0 都抽）；
4. 魔法盾减伤：`damage -= damage * DamageReductionPercent / 100`（`:7127-7130`）；
5. `armour >= damage` → Miss（`:7132-7136`）；
6. 气功波：`Random.Next(100) < EnergyShieldPercent`（`:7145-7154`，只有 `> 0` 才抽）；
7. 暴击：`Random.Next(100) < CriticalRate * 5`（`:7156-7161`），
   `damage += floor(damage * (CriticalDamage / 50) * 10)`。

`World::damage_player_from_player` 负责世界侧效果与包序：

| # | 包 | 收件人 | 出处 |
|---|---|---|---|
| 1 | `S.ObjectEffect{Critical}` | 周围玩家 | `HumanObject.cs:7158` |
| 2 | `S.DamageIndicator{Critical, 0}` | 本人 + 周围 | `:7160`、`MapObject.cs:916-926` |
| 3 | `S.Struck{AttackerID}` | 受害者本人 | `:7214`（**没有** 500ms 节流） |
| 4 | `S.ObjectStruck{ObjectID, AttackerID, Location, Direction}` | 周围玩家（含攻击者） | `:7215` |
| 5 | `S.DamageIndicator{Hit, armour - damage}` | 本人 + 周围 | `:7217` |
| 6 | `S.HealthChanged{HP, MP}` | 本人 | `ChangeHP` → `SendHealthChanged`（`:7219`） |
| 7 | `S.ObjectHealth` | 同图队友 | `BroadcastHealthChange`（`MapObject.cs:841-914`） |

死亡时再做 PK 结算 + 掉落 + `S.Death` / `S.ObjectDied`（见 §7、§8）。

**未移植**（都在源码里标了 `// 简化：`）：`ArmourRate` / `DamageRate`（恒 1）、
`Stat.AttackBonus`、`DamageWeapon`、`HPDrainRatePercent`、毒/麻痹/冰冻/失明
（`ApplyNegativeEffects`，`MapObject.cs:509-...`；`Blindness` 是毒的一种，
`HumanObject.cs:7444-7448`）、反射伤害本身（`reflect` 恒 0，只保留抽签）、
`ElementalBarrier`、`CounterAttack`、`Hidden`。

补充：本仓库的 Crystal 分支里**没有** `CombatDelay` 字段，也没有服务端宏；
`Struck` 就是一条普通包。`GetAttackPower`（`MapObject.cs:243-260`）**不读
`PKPoints`**——红名没有攻击力加成，任务书里的「PKPoints 加成」在本源码中不存在。

---

## 6. PK 值、棕名与名字颜色

### 6.1 击杀加分

`PlayerObject.Die`（`PlayerObject.cs:599-622`）：

```csharp
if (LastHitter != null && LastHitter.Race == ObjectType.Player) {
    if (AtWar(hitter) || WarZone) { /* 受法律保护，不加分 */ }
    else if (Envir.Time > BrownTime && PKPoints < 200) {   // 这里的字段属于受害者
        hitter.PKPoints = Math.Min(int.MaxValue, LastHitter.PKPoints + 100);
        if (weapon != null && weapon.AddedStats[Luck] > -Settings.MaxLuck
            && Envir.Random.Next(4) == 0) {
            weapon.AddedStats[Luck]--;                      // 武器诅咒
        }
    }
}
```

三条容易看错的点，全部照抄：

1. 判定用的是**受害者**的 `BrownTime` / `PKPoints`，不是凶手的；
2. 因此**杀红名 / 杀棕名都不加分**；
3. 武器诅咒要抽 `Random.Next(4)`：只有武器存在、幸运 > `-10`
   （`Settings.MaxLuck = 10`，`Settings.cs:304`）时才抽。

本作还发 `S.RefreshItem` 同步被诅咒的武器（`:619`）。

`AtWar`（`PlayerObject.cs:10329-10343`）在本作只剩「地图 `Fight` 标记」，
行会战争未实现。

### 6.2 自然衰减

`MapObject.Process`（`MapObject.cs:212-221`）：

```csharp
if (PKPoints > 0 && Envir.Time > PKPointTime) {
    PKPointTime = Envir.Time + Settings.PKDelay * Settings.Second;  // 12 秒
    PKPoints--;
}
if (LastHitter != null && Envir.Time > LastHitTime) LastHitter = null;
```

`World::process_pk_points` 在每个玩家的 tick 里做同样的事。注意 **严格大于**：
下一次衰减发生在 `PKPointTime + 1ms`，因此在 100ms tick 粒度下实际是 12.1 秒。

### 6.3 棕名

`HumanObject.Attacked(HumanObject)`（`HumanObject.cs:7199-7200`）：

```csharp
if (Envir.Time > BrownTime && PKPoints < 200 && !AtWar(attacker))
    attacker.BrownTime = Envir.Time + Settings.Minute;
```

* 判定用的是**受害者**的 `BrownTime` / `PKPoints`：打一个已经棕名或红名的目标
  不会让攻方再变棕；
* 棕名持续 60 秒；
* 死亡时受害者 `BrownTime = Envir.Time`（`PlayerObject.cs:647`），棕名立即结束。

### 6.4 名字颜色

`PlayerObject.GetNameColour`（`PlayerObject.cs:1849-1891`）的顺序：

1. `PKPoints >= 200` → `Color.Red`；
2. `Envir.Time < BrownTime` → `Color.SaddleBrown`；
3. `WarZone` / 行会战 → 绿/蓝/橙（未实现）；
4. `PKPoints >= 100` → `Color.Yellow`；
5. 否则 `Color.White`。

`World::refresh_name_colour` 每个 tick 在玩家簿记末尾调用（Crystal 在
`HumanObject.Process` 末尾，`HumanObject.cs:358-360`），颜色变化时：

* 本人收 `S.ColourChanged{NameColour}`（`PlayerObject.cs:1845`）；
* 同图视野内其它玩家收 `S.ObjectColourChanged{ObjectID, NameColour}`
  （`HumanObject.cs:2351-2363`）。

新玩家进场时，`S.ObjectPlayer.NameColour` 直接带上计算好的颜色。
**没有** `S.PKPoints` / `S.SetPkFlag` 这种包——本仓库的 Crystal 分支根本不存在它们。

---

## 7. 死亡掉落

### 7.1 触发条件

`PlayerObject.Die`（`PlayerObject.cs:638-641`）：

```csharp
if (PKPoints > 200) RedDeathDrop(LastHitter);
else if (!InSafeZone) DeathDrop(LastHitter);
```

* `> 200` 是**严格大于**：正好 200 分仍走普通掉落；
* 红名掉落**不看安全区**；
* 普通掉落要求**不在安全区**；
* 击杀者是不是玩家**不影响**是否掉落（见 §10 的 `pkbodydrop` 异常）。

### 7.2 普通掉落（`HumanObject.DeathDrop`，`HumanObject.cs:1391-1567`）

| 位置 | 条件 | 概率/数量 | 出处 |
|---|---|---|---|
| 装备（单件） | `Bind` 不含 `DontDeathdrop` | `Random.Next(30) == 0`（1/30） | `:1460` |
| 装备（堆叠 > 1） | 同上 | `RandomomRange(10, 8)` 决定百分比，`ceil(Count/10*percent)` | `:1438-1459` |
| 背包（单件） | 同上 | `Random.Next(10) == 0`（1/10） | `:1537` |
| 背包（堆叠 > 1） | 同上 | `RandomomRange(10, 8)`；抽到 0 就跳过 | `:1511-1536` |

### 7.3 红名掉落（`PlayerObject.RedDeathDrop`，`PlayerObject.cs:668-793`）

| 位置 | 条件 | 概率/数量 | 出处 |
|---|---|---|---|
| 装备（单件） | 同普通 | `Random.Next(10) == 0`（1/10） | `:719` |
| 装备（堆叠 > 1） | 同普通 | `RandomomRange(10, 4)` | `:697-718` |
| 背包 | 同普通 | **无条件全部掉光** | `:750-790` |

### 7.4 排除与归属

排除（两种掉落共用，`death_drop_allowed`）：

* `BindMode.DontDeathdrop`（`HumanObject.cs:1407`、`:1502`；`PlayerObject.cs:679`、`:757`）；
* 婚戒（`WeddingRing != -1`；装备分支只跳过左戒那枚，本作统一跳过）；
* 封印中（`SealedInfo` 未过期；本作只要有 `SealedInfo` 就跳过）；
* 租借物品（Crystal 会先邮寄给出租人；本作留在身上）。

归属（`ItemObject(dropper, item, deathDrop: true)`，`ItemObject.cs:53-58`）：

* **不设置 `Owner`**——谁都能捡，凶手也可以；
* `ExpireTime = now + Settings.PlayerDiedItemTimeOut * Minute` = **120 分钟**
  （`Settings.cs:116`）。

落点用 `ItemObject.Drop(Settings.DropRange = 4)` 的确定性环形扫描
（`ItemObject.cs:190-275`）。此时受害者**还没有** `Dead = true`，所以自己脚下的格子
算「被阻挡」，第一件掉落不会压在自己身上。

### 7.5 随机数顺序

死亡掉落与战斗共用一个 `World.rng`。一次「PvP 击杀」的抽签顺序：

1. 攻方 `GetAttackPower`（`min == max` 时 0 次）；
2. 守方闪避（`Agility == 0` 时 0 次）；
3. 守方护甲 `GetDefencePower`（`min == max` 时 0 次）；
4. 反射 `Random.Next(100)`；
5. 气功波（只有 `EnergyShieldPercent > 0`）；
6. 暴击 `Random.Next(100)`；
7. 武器诅咒 `Random.Next(4)`（只有条件满足）；
8. 死亡掉落：装备槽 0..13 依次抽，再背包槽 0..45 依次抽。

测试 `normal_death_drops_equipment_and_inventory_by_probability` 固定 seed 20，
断言药水叠掉出一部分且总数守恒。

---

## 8. 守卫（Guard / ArcherGuard）

### 8.1 识别

`MonsterKind::is_guard()`：`MonsterInfo.AI ∈ {6, 58, 113}`
（`MonsterObject.cs:30-31`、`:141-142`、`:261-262`；`ArcherGuard : Guard`，
`Monsters/ArcherGuard.cs:6`）。

真实数据冒烟测试 `real_database_exposes_safe_zone_and_guard_template` 会打印
样本数据库里第一个守卫的名字/AI/等级/HP/视野，当前样本输出：

```
安全区样本：map=0 title=BichonProvince center=(288, 616) size=10 start_point=true
守卫样本：name=Guard ai=6 level=0 hp=9999 view=7
```

### 8.2 行为

最小闭环（`World::process_ai` 的守卫分支）：

1. **索敌**：只把红名玩家（`PKPoints >= 200`）当目标
   （`PlayerObject.IsAttackTarget(MonsterObject)`，`PlayerObject.cs:4679`）；
   洗白后立即脱战（`MonsterObject.cs:1198-1199`）；
2. **攻击**：距离 ≤ `ViewRange`（不是相邻）、`DefenceType.AC`、延迟 300ms
   （`Monsters/Guard.cs:40-45`、`:79-86`）。攻击动画广播在**目标背后一格**
   （`Target.Back`），随后补一条 `S.ObjectTurn`（`:70-74`）；
3. **回位**：没有目标且不在出生点时，朝出生点走一格。

简化：

* Crystal 的守卫靠 `RespawnInfo.RoutePath` 巡逻（`Guard.CanMove` 要求
  `Route.Count > 0`）；本作没有路线表，把出生点 `Monster.home` 当作「唯一路线点」；
* `ArcherGuard` 的远程投射物（`ObjectRangeAttack` + `ProjectileAttack`）没有移植，
  按近战守卫处理；
* `Guard.Attacked` / `Die` 在 Crystal 里是空实现/抛异常；本作通过
  「守卫不可被攻击」达到同样效果（`Guard.IsAttackTarget` 恒 false）；
* 守卫回血被禁用（`Guard.ProcessRegen` 空实现，`:47`）。

守卫名字颜色是 `Color.SkyBlue`（`Monsters/Guard.cs:24-28`），
`S.ObjectMonster.NameColour` 带这个值。

---

## 9. Rust API

| API | 说明 |
|---|---|
| `Player.attack_mode: AttackMode` | 当前攻击模式 |
| `Player.pk_points: i32` | PK 值 |
| `Player.brown_time: i64` | 棕名到期时刻 |
| `Player.pk_point_time: i64` | 下次 PK 衰减时刻 |
| `Player.last_hitter / last_hit_time` | 最后伤害来源（死亡 PK 结算用） |
| `Player.name_colour: Color` | 当前缓存的名字颜色 |
| `World::change_attack_mode(object_id, mode) -> bool` | 处理 `C.ChangeAMode`，给本人回 `S.ChangeAMode` |
| `World::attack_mode_of(object_id) -> Option<AttackMode>` | 登录补发用 |
| `World::is_attack_target_player(attacker, target) -> bool` | `PlayerObject.IsAttackTarget(HumanObject)` |
| `World::name_colour_for(object_id) -> Color` | 按当前状态计算颜色 |
| `World::pending_hits() -> &[PendingHit]` | 尚未结算的延迟伤害（测试/运维） |
| `World::map_no_fight(map) / map_fight(map)` | `MapInfo.NoFight` / `Fight` |
| `combat::resolve_player_vs_player(...) -> PvpHit` | PvP 伤害结算 |
| `combat::resolve_monster_attack(..., defence, ...)` | 怪物/守卫受击结算（新增 `defence` 参数） |
| `World::drop_player_death_loot(player_id, red)` | 死亡掉落（crate 内部） |
| `MonsterKind::is_guard()` | AI 6/58/113 |

---

## 10. 与 Crystal 的差异/异常清单

1. **`pkbodydrop` 恒为 true**（`HumanObject.cs:1393-1398`）：
   `if ((killer == null) || ((pkbodydrop) || ...))` 因为 `pkbodydrop` 被初始化成
   `true` 且用 `||`，条件永远成立。结论：**被怪物杀死也会爆装备/背包**
   （只要不在安全区）。本作照抄，测试
   `monster_kill_outside_safe_zone_still_drops_in_crystal` 锁定该行为。
   任务书里「被怪杀死不爆」的预期与本仓库 Crystal 分支不符。
2. **红名死亡掉落的背包是无条件的**（`PlayerObject.cs:750-790`），
   连 1/10 的抽签都没有；普通死亡才有概率。
3. **红名掉落用 `> 200` 而不是 `>= 200`**（`PlayerObject.cs:638`）：
   正好 200 分（已经显示红名）仍走普通掉落。
4. **`GetArmour` 的 Miss 广播次数**：Crystal 在 `GetArmour` 里就
   `BroadcastDamageIndicator(Miss)`（`MapObject.cs:469` 等），随后
   `armour >= damage` 还会再发一次。本作只在结算后统一发一次 Miss。
5. **名字颜色的行会/战争分支未实现**：`WarZone`、`MyGuild.IsAtWar()`、
   `IsEnemy` 在 Crystal 里是绿/蓝/橙；本作没有攻城/行会战，只有红/棕/黄/白。
   `EnemyGuild` 攻击模式因此恒不命中。
6. **`IsFriendlyTarget` 没有参与物理攻击**：Crystal 只在「宠物集火」里用它
   （`HumanObject.cs:3035-3054`），真正的命中只看 `IsAttackTarget`。本作同样。
7. **`ObjectPlayer.NameColour` 用目标自身的缓存**：Crystal `GetInfo`
   直接用 `NameColour`（`PlayerObject.cs:4784`），`BroadcastColourChange`
   才按观察者计算；在本作实现的四种颜色（红/棕/黄/白）里两者一致，
   所以没有可观察差异。
8. **PK 值不持久化**：Crystal 存在 `CharacterInfo.PKPoints`
   （`CharacterInfo.cs:66`、`:187`、`:429`）；本作不改 `mir-store`，
   重启后归零，同进程内死亡/复活保留（测试 `revive_keeps_pk_points`）。
   攻击模式同理（Crystal 存 `CharacterInfo.AMode`）。
9. **丢弃了 Crystal 的一些随机数**：`AttackBonus`、`DamageWeapon`、毒/麻痹、
   `HPDrain` 等未实现系统不抽签。若后续接入，需要按 §7.5 的顺序补回。
10. **`BreakOnDeath` 的装备直接删除**：Crystal 删完后还会继续走掉落分支
    （`HumanObject.cs:1417-1426` 没有 `continue`），属于源码 bug；本作删除后跳过。
11. **`randomom_range` 抽到 0 时**：Crystal 的装备分支仍会创建一个 0 数量的物品
    并掉落（`:1438-1459` 没有 `if (percent == 0) continue`）；本作跳过 0 数量，
    避免客户端看到「0 个」的物品堆。
12. **守卫回位是近似**：没有 `Route` 表，用出生点代替；`ArcherGuard` 没有远程攻击。
13. **协议不缺包**：`C.ChangeAMode`、`S.ChangeAMode`、`S.ColourChanged`、
    `S.ObjectColourChanged`、`S.ObjectPlayer.NameColour`、`S.DeleteItem`、
    `S.RefreshItem`、`S.ObjectStruck`、`S.Struck`、`S.DamageIndicator` 都已存在。
    本仓库的 Crystal 分支**没有** `S.PKPoints` / `S.SetPkFlag` 包。

---

## 11. 测试清单（`crates/mir-server/tests/pk.rs`，27 个）

| 用例 | 断言 |
|---|---|
| `safe_zone_blocks_player_attack` | 双方在安全区 → 不选中玩家 |
| `peace_mode_refuses_player_target` | `Peace` 模式拒绝 |
| `group_mode_refuses_group_mate` | 组队模式拒绝队友 |
| `group_mode_allows_non_member` | 组队模式放行非队员 |
| `guild_mode_refuses_same_guild` | 行会模式拒绝同会 |
| `enemy_guild_mode_hits_nobody_without_wars` | 无行会战 → `EnemyGuild` 不命中 |
| `red_brown_mode_only_hits_red_or_brown_players` | `RedBrown` 只打红/棕 |
| `all_mode_hits_any_player` | `All` 可以打 |
| `pvp_attack_delays_damage_and_broadcasts_packets` | 300ms 延迟、`Struck`/`ObjectStruck`/`HealthChanged`/`DamageIndicator` |
| `pvp_critical_broadcasts_object_effect_and_indicator` | 暴击 `ObjectEffect{Critical}` + 伤害翻倍 |
| `first_hit_makes_attacker_brown` | 首次攻击使攻方棕名 60 秒 |
| `kill_adds_pk_points_and_turns_name_red` | +100 到 200 → 红名 + `ColourChanged`/`ObjectColourChanged` |
| `killing_a_red_player_adds_no_pk_points` | 杀红名不加分 |
| `attacking_a_red_player_adds_no_pk_and_no_brown` | 打红名不加 PK 值、不染棕 |
| `pk_points_decay_one_per_pk_delay` | 12 秒严格边界衰减 |
| `red_name_falls_back_to_yellow_after_decay` | 201 → 200 → 199，红转黄发颜色包 |
| `revive_keeps_pk_points` | 复活后 PK 值保留 |
| `normal_death_drops_equipment_and_inventory_by_probability` | 固定 seed 下装备/背包按概率掉、绑定物品不掉、总量守恒 |
| `red_death_drops_whole_inventory` | 红名背包全掉 |
| `monster_kill_inside_safe_zone_does_not_drop` | 安全区内死亡不爆 |
| `monster_kill_outside_safe_zone_still_drops_in_crystal` | Crystal `pkbodydrop` 异常：被怪杀死也爆 |
| `killer_can_pick_up_death_drops` | 死亡掉落无归属，凶手可捡 |
| `guard_attacks_red_player_and_uses_ac_defence` | 守卫打红名、攻击动画在目标背后、`AC` 防御 |
| `guard_ignores_white_and_brown_players_until_red` | 守卫只打红名 |
| `guard_walks_back_home_when_idle` | 守卫无目标时走回出生点 |
| `change_attack_mode_echoes_to_self_only` | `C.ChangeAMode` 只回本人、不广播 |
| `real_database_exposes_safe_zone_and_guard_template` | 真实数据安全区 + 守卫模板解析 |

---

## 12. 协议顺序速查

**登录**（`PlayerObject.cs:1217-1219`）：

```
S.UserInformation → S.ChangeAMode{Peace} → S.SwitchGroup{true} → ...
```

**切换攻击模式**：

```
C.ChangeAMode{Mode} → S.ChangeAMode{Mode}（只给本人）
```

**PvP 攻击**：

```
C.Attack{Direction} →
  本人 S.UserLocation
  周围 S.ObjectAttack
  300ms 后：
    暴击时 S.ObjectEffect{Critical} + S.DamageIndicator{Critical}
    S.Struck（本人）
    S.ObjectStruck（周围，含攻方）
    S.DamageIndicator{Hit, -damage}（本人 + 周围）
    S.HealthChanged（本人）
  死亡时：
    S.DeleteItem（掉落的每一件）
    S.Death（本人）
    S.ObjectDied（周围）
```

**PK 值变化（下一个 tick）**：

```
S.ColourChanged（本人） + S.ObjectColourChanged（周围）
```
