# 战斗与怪物 AI

> 参考实现根目录：`C:\Users\diana\ref\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：`Server/MirObjects/MapObject.cs`（`GetAttackPower`/`GetDefencePower`/`GetArmour`/广播）、
> `Server/MirObjects/HumanObject.cs`（玩家攻击、受击、死亡、回血）、`Server/MirObjects/MonsterObject.cs`
> （怪物 AI、攻击、死亡）、`Server/MirObjects/PlayerObject.cs`（经验、升级、复活）、
> `Server/MirEnvir/Map.cs` + `Server/MirEnvir/RespawnTimer.cs`（刷怪点重生）、
> `Server/MirDatabase/MonsterInfo.cs`（怪物默认值）、`Shared/BaseStats.cs`（玩家基础属性曲线）。
>
> Rust 侧实现：`crates/mir-server/src/combat.rs`（属性与结算）、`crates/mir-server/src/monster.rs`（怪物与 AI）、
> `crates/mir-server/src/world.rs`（世界 tick、延迟伤害、视野、刷怪重生、经验、死亡复活、自然回血）、
> `crates/mir-server/src/exp.rs`（等级经验表）。
> 测试：`crates/mir-server/tests/monster_combat.rs`（9 个端到端用例）、
> `crates/mir-server/tests/world_lifecycle.rs`（12 个生命周期用例）、
> `crates/mir-server/src/exp.rs` 的单测（5 个）。

---

## 1. 结论先行

- **一次物理攻击是「两段式」的**：动作包立刻广播，伤害 **300ms 后**才由延迟动作结算。
  玩家和怪物都是这样（`HumanObject.cs:3201`、`MonsterObject.cs:2171`）。
- **命中判定在防御方**：`GetArmour` 先掷闪避（`Random.Next(Agility + 1) > Accuracy`），再掷护甲
  （`GetDefencePower`）；`armour >= damage` 直接算 Miss。
- **暴击只发生在「打怪物」这一侧**：`MonsterObject.Attacked` 有暴击判定，
  `HumanObject.Attacked(MonsterObject)` **没有**——怪物打玩家永远不暴击（Crystal 原样行为，见 §12）。
- **怪物 AI 有 2 秒调度粒度**：`SetOperateTime` 让每只怪最快 2 秒被 `Process()` 一次；
  附近没玩家时整只怪不思考（`Alone`）。
- **怪物不会跑、不查门、不触发传送**（`MonsterObject.Walk`，`MonsterObject.cs:2095-2149`）。
- **刷怪点是「登记 + 铺满 + 到点补缺口」**：死亡只把该组存活数 `-1`，补怪由全局刷新周期驱动
  （`MapRespawn`，见 §8）。
- **经验是「当前等级内的进度」**：`MaxExperience = ExperienceList[Level - 1]`，升级时扣掉
  （`PlayerObject.cs:905-921`，见 §9）。
- **死亡后 Crystal 不会自动复活**：由客户端弹窗、玩家确认才发 `C.TownRevive`；本作改成服务端
  计时自动原地复活（见 §10、§12）。

---

## 2. 常量速查

| 名称 | 值 | Rust 常量 | Crystal |
|---|---|---|---|
| 搜索间隔 | 3000 ms | `SEARCH_DELAY_MS` | `MonsterObject.cs:602`（`SearchDelay`） |
| 漫游间隔 | 1000 ms | `ROAM_DELAY_MS` | `MonsterObject.cs:602`（`RoamDelay`） |
| Alone 检查间隔 | 3000 ms | `ALONE_DELAY_MS` | `MonsterObject.cs:602`（`AloneDelay`） |
| 怪物回血间隔 | 10000 ms | `REGEN_DELAY_MS` | `MonsterObject.cs:602`（`RegenDelay`） |
| 怪物走一步的动作锁定 | 300 ms | `MONSTER_WALK_ACTION_MS` | `MonsterObject.cs:2128` |
| 怪物攻击的动作锁定 | 300 ms | `MONSTER_ATTACK_ACTION_MS` | `MonsterObject.cs:2165` |
| 怪物攻击 → 伤害的延迟 | 300 ms | `MONSTER_ATTACK_DAMAGE_DELAY_MS` | `MonsterObject.cs:2171` |
| 怪物移动后的格子宽限 | 500 ms | `MONSTER_CELL_GRACE_MS` | `MonsterObject.cs:2127` |
| 出生后的动作锁定 | 2000 ms | `MONSTER_SPAWN_ACTION_MS` | `MonsterObject.cs:736` |
| 怪物调度上限 | 2000 ms | `MONSTER_OPERATE_DELAY_MS` | `MonsterObject.cs:1234` |
| 出生调度抖动 | 0–99 ms | `MONSTER_OPERATE_JITTER_MS` | `MapObject.cs:155,351`（`OperateDelay`） |
| 尸体停留 | 180000 ms | `CORPSE_DELAY_MS` | `MonsterObject.cs:584-599`（`DeadDelay`） |
| 默认视野 | 7 | `DEFAULT_VIEW_RANGE` | `MonsterInfo.cs:27`（`ViewRange = 7`） |
| 默认攻击间隔 | 2500 ms | `DEFAULT_ATTACK_SPEED_MS` | `MonsterInfo.cs:32`（`AttackSpeed = 2500`） |
| 默认移动间隔 | 1800 ms | `DEFAULT_MOVE_SPEED_MS` | `MonsterInfo.cs:32`（`MoveSpeed = 1800`） |
| 玩家攻击的动作锁定 | 550 ms | `PLAYER_ATTACK_ACTION_MS` | `HumanObject.cs:2975` |
| 玩家攻击 → 伤害的延迟 | 300 ms | `PLAYER_ATTACK_DAMAGE_DELAY_MS` | `HumanObject.cs:3201` |
| 玩家受击动画节流 | 500 ms | `PLAYER_STRUCK_DELAY_MS` | `HumanObject.cs:7304-7309`（`StruckTime`） |
| 幸运上限 | 10 | `MAX_LUCK` | `Settings.cs:304`（`MaxLuck`） |
| 魔法闪避权重 | 10 | `MAGIC_RESIST_WEIGHT` | `Settings.cs:296` |
| 暴击率权重 | 5 | `CRITICAL_RATE_WEIGHT` | `Settings.cs:298` |
| 暴击伤害权重 | 50 | `CRITICAL_DAMAGE_WEIGHT` | `Settings.cs:299` |
| 一「分钟」 | 60000 ms | `MINUTE_MS` | `Settings.cs:16`（`Minute = 60 * Second`） |
| 刷怪间隔 | `Delay` 分钟 | `SpawnGroup::delay_minutes` | `RespawnInfo.cs:36` + `Map.cs:759-760` |
| 刷怪随机延迟 | `RandomDelay` 分钟 | `SpawnGroup::random_delay_minutes` | `RespawnInfo.cs:41-47` + `Map.cs:759` |
| 刷怪失败冷却 | 60000 ms | `RESPAWN_FAIL_DELAY_MS` | `Map.cs:770`（`+ 1 * Settings.Minute`） |
| 刷怪基准 tick | 20 分钟 | （文档值，本作折成倍率） | `RespawnTimer.cs:58`（`BaseSpawnRate = 20`） |
| 刷怪全局倍率 | 1.0 | `World::respawn_tick_scale` | `RespawnTimer.cs:112-123`（`CurrentDelay`） |
| 玩家回血/回蓝间隔 | 10000 ms | `PLAYER_REGEN_DELAY_MS` | `HumanObject.cs:186`（`RegenDelay`） |
| 玩家每次回复比例 | 3% + 1 | `PLAYER_REGEN_RATE` | `HumanObject.cs:562`、`:568` |
| 玩家复活等待 | 10000 ms | `PLAYER_REVIVE_DELAY_MS` | 本作扩展（Crystal 无服务端复活计时器，见 §12） |

玩家攻速（毫秒，`HumanObject.cs:1766-1768`）：

```
AttackSpeed = 1400 - (Stats[AttackSpeed] * 60 + min(370, Level * 14))   下限 550
```

1 级 0 攻速 → **1386ms**；50 级 → **1030ms**。见 `combat::player_attack_delay_ms`。

---

## 3. 战斗属性（`CombatStats`）

对应 Crystal `MapObject.Stats` 里参与物理战斗的字段（`Stat.MinDC`…`Stat.Reflect`）。

| 字段 | Crystal `Stat` | 用途 |
|---|---|---|
| `min_dc` / `max_dc` | `MinDC` / `MaxDC` | 物理攻击力区间（`GetAttackPower`） |
| `min_mc` / `max_mc` | `MinMC` / `MaxMC` | 魔法攻击力区间（本里程碑未使用） |
| `min_sc` / `max_sc` | `MinSC` / `MaxSC` | 道术攻击力区间（未使用） |
| `min_ac` / `max_ac` | `MinAC` / `MaxAC` | 物理防御区间（`GetDefencePower`） |
| `min_mac` / `max_mac` | `MinMAC` / `MaxMAC` | 魔法防御区间（`MAC`/`MACAgility` 时用） |
| `accuracy` | `Accuracy` | 准确（防守方闪避判定用） |
| `agility` | `Agility` | 敏捷（`Random.Next(Agility + 1) > Accuracy` 则闪避） |
| `luck` | `Luck` | 幸运：>0 更容易打上限，<0 更容易打下限 |
| `attack_speed` | `AttackSpeed` | 攻速属性（不是毫秒，见 §2 公式） |
| `critical_rate` / `critical_damage` | `CriticalRate` / `CriticalDamage` | 暴击率/暴击伤害 |
| `magic_resist` | `MagicResist` | 魔法闪避（`MAC` 类防御时先判一次） |
| `reflect` | `Reflect` | 伤害反射（无装备，恒 0；**抽签照做**以保持随机序列） |
| `hp` / `mp` | `HP` / `MP` | **上限值**；当前血量在 `Player.hp` / `Monster.hp` |

玩家基础属性由等级/职业推出（`combat::player_base_stats`），逐条移植 `Shared/BaseStats.cs:12-89`
的 `BaseStat` 表与 `BaseStat.Calculate`（`:155-184`），并套用 `RefreshStatCaps`（`HumanObject.cs:2229-2253`）：

| 等级 1 | HP | MP | 准确 | 敏捷 |
|---|---|---|---|---|
| 战士 | 18 | 14 | 5 | 15 |
| 法师 | 15 | 17 | 5 | 15 |
| 道士 | 16 | 13 | 5 | 18 |
| 刺客 | 17 | 16 | 5 | 20 |
| 弓手 | 17 | 15 | 8 | 15 |

---

## 4. 三个核心公式与随机数顺序

Crystal 只有一个全局 `Envir.Random`（`Envir.cs:100`）。mir-next 用 `World::rng` 单实例
（xorshift128+，可 `seed_rng` 回放），**每次抽签的顺序都必须与 Crystal 一致**。

### 4.1 `GetAttackPower(min, max)`（`MapObject.cs:243-260`）

```
min = max(min, 0);  if (min > max) max = min
if luck > 0:  抽一次 r ∈ [0, MaxLuck-1]；若 luck > r   → 直接返回 max
elif luck < 0: 抽一次 r ∈ [0, MaxLuck-1]；若 luck < -r  → 直接返回 min
返回 [min, max] 的均匀随机
```

即**幸运不为 0 时消耗 2 次抽签**，否则 1 次。

### 4.2 `GetDefencePower(min, max)`（`MapObject.cs:274-280`）

```
min = max(min, 0);  if (min > max) max = min
返回 [min, max] 的均匀随机
```

### 4.3 `GetArmour(type, attacker, out hit)`（`MapObject.cs:460-507`）

| 防御类型 | 抽签顺序 |
|---|---|
| `ACAgility` | ① `Next(Agility+1) > Accuracy` → Miss；② `GetDefencePower(MinAC, MaxAC)` |
| `AC` | ① `GetDefencePower(MinAC, MaxAC)` |
| `MACAgility` | ① `Next(MagicResistWeight) < MagicResist` → Miss；② 敏捷判定；③ `GetDefencePower(MinMAC, MaxMAC)` |
| `MAC` | ① 魔法闪避判定；② `GetDefencePower(MinMAC, MaxMAC)` |
| `Agility` | ① 敏捷判定（护甲为 0） |
| `Repulsion` / `None` | 无（护甲 0、必中） |

**未命中时护甲值仍然会被抽出来**——随机数消耗与命中与否无关，这一点必须照抄。

### 4.4 一次完整攻击的抽签顺序

```
1. 攻方 GetAttackPower(MinDC, MaxDC)                 # 1~2 次
2. 守方 GetArmour(ACAgility, ...)                    # 2 次：闪避 + 护甲
3. if (armour >= damage) → Miss（不再抽）
4. 暴击抽签 Next(100) < CriticalRate * 5             # 仅「打怪物」时存在
```

---

## 5. 伤害结算

### 5.1 玩家打怪物：`MonsterObject.Attacked(HumanObject)`（`MonsterObject.cs:2569-2684`）

```
1. Target == null 且可攻击 → Target = attacker
2. (armour, hit) = GetArmour(...)；!hit → 广播 Miss、return 0
3. armour >= damage → 广播 Miss（DamageIndicator）、return 0
4. Next(100) < CriticalRate*5 →
     Broadcast(ObjectEffect{Critical}) + DamageIndicator{Critical, 0}
     damage += floor(damage * (CriticalDamage / 50) * 10)
5. Target 归属、EXPOwner（首个攻击者）
6. Broadcast(ObjectStruck)
7. DamageIndicator{Hit, armour - damage}（**负数**）
8. ChangeHP(armour - damage) → 血量归零则 Die()
9. ObjectHealth 只发给 EXPOwner（`MapObject.cs:841-914`）
```

### 5.2 怪物打玩家：`HumanObject.Attacked(MonsterObject)`（`HumanObject.cs:7222-7315`）

```
1. Next(100) < Reflect → 反射（本作恒 0，但抽签照做）
2. (armour, hit) = GetArmour(ACAgility, ...)；!hit → 广播 Miss
3. armour >= damage → 广播 Miss
4. **没有暴击判定**
5. S.Struck 给本人（500ms 节流） + Broadcast(ObjectStruck) 给周围玩家
6. DamageIndicator{Hit, armour - damage}（负数）
7. ChangeHP(...) → SendHealthChanged（HealthChanged 给本人）
8. 血量归零 → PlayerObject.Die()
```

### 5.3 玩家死亡：`PlayerObject.Die`（`PlayerObject.cs:578-667`）的 MVP 实现

- `hp = 0`、`dead = true`、`death_time = now`、`revive_time = now + 10000`；
- 本人收到 `S.Death`（坐标 + 朝向）；
- 周围玩家收到 `S.ObjectDied`；
- 尸体**不会**被移除，`dead` 状态屏蔽走/跑/转身/攻击（`MoveError::Dead`）；
- 到点自动原地复活（本作扩展，详见 §10）。

### 5.4 怪物死亡：`MonsterObject.Die`（`MonsterObject.cs:966-1006`）

- `HP = 0`、`Dead = true`、`DeadTime = now + 180000`；
- 广播 `S.ObjectDied`；
- `EXPOwner.WinExp(Experience, Level)` 给击杀者加经验（见 §9）；
- `Respawn.Count--` 让刷怪组进入补怪排程（见 §8.3）；
- `DeadTime` 到期后从世界移除并广播 `S.ObjectRemove`（`Despawn`，`MapObject.cs:363`）；
- **掉落已实现**：`if (Master == null && EXPOwner != null) Drop();`（`:997-998`）→
  `World::drop_monster_loot`，见 `docs/world/items.md` §3。

---

## 6. 怪物 AI 状态机

`World::process_ai` 忠实复现 `MonsterObject.ProcessAI`（`MonsterObject.cs:1671-1733`）：

```
CheckAlone()                  # 视野×2 内没玩家 → 整只怪跳过 AI（Settings.MonsterProcessWhenAlone = false）
if Alone: return
ProcessStacking()             # 与别人踩同格 → 朝当前方向走 / 随机顺时针或逆时针找空格
ProcessSearch()               # 每 3s；无目标，或 1/3 概率，重新 FindTarget
ProcessRoam()                 # 无目标且每 1s：10% 概率真正漫游（1/3 转身、2/3 前进一步）
ProcessTarget()               # 只做两件事：够得着就 Attack，够不着就 MoveTo
```

要点（都来自 Crystal 源码）：

- `ProcessTarget` 的第一行是 `if (Target == null || !CanAttack) return;`——**攻击冷却期间怪物连追都不追**。
- `CanMove = !Dead && now > MoveTime && now > ActionTime`，`CanAttack = !Dead && now > AttackTime && now > ActionTime`
  （注意是**严格大于**，与玩家动作的 `now >= ActionTime` 不同）。
- `FindTarget` 按「视野环形扫描」：距离 `d` 从小到大，同环内 `y`、`x` 升序取第一个可攻击对象；
  已有目标时不会替换（`Target ??= ob`）。
- `MoveTo`：先朝目标方向走一步；走不通按 `Random.Next(2)` 选顺时针/逆时针各试 7 个方向；
  目标在隔壁且格子被占时**原地不动**（不绕路）。
- `Walk` 成功后：`CellTime = now+500`、`ActionTime = now+300`、`MoveTime = now+MoveSpeed`，
  且若 `MoveTime > AttackTime` 则 `AttackTime = MoveTime`（走一步会把攻击往后推）。
- `Attack`：广播 `ObjectAttack` → `ActionTime = now+300`、`AttackTime = now+AttackSpeed` →
  抽伤害（0 则不排队）→ 300ms 后结算。
- `ProcessRegen`：每 10s，若 `HP < MaxHP`，回 `floor(MaxHP * 0.022) + 1`（`MonsterObject.cs:1425-1426`）。
- `SetOperateTime`：取 `now + 2000` 与所有未来 deadline 的最小值 → **每只怪最快 2 秒思考一次**。

### 6.1 怪物视野与"脱战"

- 索敌：`MonsterKind::view_range`（Crystal `MonsterInfo.ViewRange`，默认 7）。
- 脱战：目标死亡、换图、或超出广播半径 `VIEW_RANGE`（10）。
  简化：Crystal 用 `Globals.DataRange`（16），mir-next 用 10（见 §7）。

---

## 7. 客户端包

| 包 | ID | 收件人 | 时机 |
|---|---|---|---|
| `ObjectMonster` | 71 | 视野内玩家 | 刷怪、玩家进场、怪物走进视野 |
| `ObjectAttack` | 72 | 除发布者外的视野内玩家 | 攻击动作开始（伤害还没落地） |
| `Struck` | 73 | **受害者本人** | 挨打（500ms 节流） |
| `ObjectStruck` | 74 | 除发布者外的视野内玩家 | 挨打（打怪时攻击者自己也能看到） |
| `DamageIndicator` | 75 | 本人 + 视野内玩家 | Miss / 暴击 / 命中（命中时 `Damage = armour - damage`，是**负数**） |
| `HealthChanged` | 77 | **本人** | 扣血/回血后同步 HP/MP |
| `Death` | 80 | **本人** | 玩家死亡 |
| `ObjectDied` | 81 | 除本人外的视野内玩家 | 玩家/怪物死亡 |
| `ObjectEffect` | 124 | 视野内玩家 | 暴击（`SpellEffect.Critical`）等特效 |
| `ObjectHealth` | 139 | 怪物只发给 `EXPOwner` | 怪物血量百分比 |
| `ObjectRemove` | 26 | 看见过该对象的玩家 | 尸体到期、玩家下线、走出视野 |

广播范围（`MapObject.Broadcast`，`MapObject.cs:408-420`）：同图、切比雪夫距离 ≤ 广播半径、
**不发给发布者自己**。所以：

- 玩家自己发起的包（`ObjectAttack`/`ObjectWalk`/…）不会回给自己；
- 怪物发起的包（`ObjectStruck` 等）视野内**所有玩家**都收得到，包括刚打完它的那个玩家。

---

## 8. 刷怪点重生（`MapRespawn` / `RespawnTimer`）

Rust 侧：`World::register_spawn_group` / `spawn_from_group` / `tick_respawns` / `respawn_state`
（`crates/mir-server/src/world.rs`），数据来自 `GameData::spawn_groups`（`gamedata.rs`）。

### 8.1 Crystal 的数据结构

| Crystal | 含义 | Rust |
|---|---|---|
| `RespawnInfo`（`Server/MirDatabase/RespawnInfo.cs:6-52`） | 落盘的刷怪点：`MonsterIndex`/`Location`/`Count`/`Spread`/`Delay`/`RandomDelay`/`RespawnTicks` | [`SpawnGroup`]（`delay_minutes`/`random_delay_minutes`） |
| `MapRespawn`（`Server/MirEnvir/Map.cs:2532-2591`） | 运行期状态：`Count`/`RespawnTime`/`NextSpawnTick`/`ErrorCount` | 内部 `RespawnGroupState`，快照为 [`RespawnState`] |
| `Map.Respawns`（`Map.cs:473-488`） | 每张地图的刷怪组列表 | `World::respawn_groups` |

建图时 Crystal 先把 `WalkableCells` 裁剪到「刷新点 ± `Spread`」的方形范围内
（`Map.cs:473-488`），刷怪时从里面随机取一格（`MonsterObject.cs:706-732`）。

### 8.2 两种时间模型（`Map.ProcessRespawns`，`Map.cs:740-790`）

```csharp
foreach respawn in Respawns:
    if respawn.Info.RespawnTicks != 0 && Envir.RespawnTick.CurrentTickcounter < respawn.NextSpawnTick: continue; // tick 制
    if respawn.Info.RespawnTicks == 0 && Envir.Time < respawn.RespawnTime: continue;                              // 时间制
    if respawn.Count < respawn.Info.Count * Envir.SpawnMultiplier:
        补齐 (Count*Multiplier - Count) 只
    if 成功:
        delay = max(1, Delay - RandomDelay + Random.Next(RandomDelay * 2))     // :759
        respawn.RespawnTime = Envir.Time + delay * Settings.Minute             // :760
    else:
        respawn.RespawnTime = Envir.Time + 1 * Settings.Minute                 // :770
        ErrorCount++（到 6 时写日志，`:771-785`）
```

- `Settings.Minute == 60000`（`Settings.cs:16`）；`Envir.SpawnMultiplier` 默认 1（`Envir.cs:160`）。
- `Random.Next(RandomDelay * 2)` **左闭右开** → 实际间隔是 `Delay - RandomDelay ..= Delay + RandomDelay - 1`
  分钟；`RandomDelay == 0` 时 `.NET` 的 `Next(0)` 直接返回 0（不抽签）。
- `RespawnTimer`（`Server/MirEnvir/RespawnTimer.cs:51-124`）是全局刷怪心跳：
  `CurrentDelay = BaseSpawnRate(20 分钟) * 60000 * bonus`，`CurrentDelay` 到了就 `CurrentTickcounter++`，
  `RespawnTicks != 0` 的刷怪组按「tick 计数器」而不是绝对时间去判断。

### 8.3 死亡与补怪

- **死亡**：`MonsterObject.Die` 里 `if (Respawn != null) Respawn.Count--;`（`MonsterObject.cs:994-995`）；
  刷出时 `Respawn.Count++`（`MonsterObject.cs:728`）。
- **本作**：`Monster::respawn_group` 记住所属组下标；死亡时 `World::on_respawn_monster_died`
  把存活数 `-1`，并在「没有排程」时排一个完整周期之后补 1 只。

### 8.4 本作的 API 与差异

| API | 作用 |
|---|---|
| `World::register_spawn_group(SpawnGroup) -> Vec<u32>` | 登记组 + 立即铺满 `count`，返回对象 ID |
| `World::spawn_group(&SpawnGroup)` | 旧名，等价于上一条 |
| `World::respawn_state() -> Vec<RespawnState>` | 只读快照（`alive` / `next_spawn_at`，给测试与 GM 命令） |
| `World::respawn_tick_scale()` / `set_respawn_tick_scale(f64)` | Crystal `RespawnTimer` 的全局节奏倍率（默认 1.0） |

简化（每条都在代码里标了原因）：

1. **每个周期只补 1 只**，Crystal 是一次补齐所有缺口（`Map.cs:749-755`）。原因：一次补齐会在到点
   瞬间刷出一整群怪；逐只补更接近「打个不停」的手感，也让 `next_spawn_at` 的语义恒为「下一只」。
2. **`RespawnTicks`（tick 制）没有单独实现**，统一折算成「时间制 × 全局倍率」
   `respawn_tick_scale`。原因：本作还没有「在线人数档位」这套 `DelayLoss` 配置，
   先把倍率做成显式旋钮；数据里的 `respawn_ticks` 字段已在 `mir-db` 里，接入时只需在
   `register_spawn_group` 里换算。
3. **出生点撒点**代替 `WalkableCells` 裁剪（半径内随机 + 16 次重试）。
4. **随机数抽签的时机不同**：Crystal 在 `ProcessRespawns` 成功补怪之后才抽
   `Random.Next(RandomDelay * 2)`（`Map.cs:759`），本作在**排程时**抽（死亡瞬间或补完下一只瞬间）。
   公式与取值范围完全一致，只是与其它抽签（战斗、刷怪落点）交错的顺序不同，
   因此同一 seed 下本作与 Crystal 的随机序列不能逐位对齐——这是逐只补怪模型带来的必然差异。

---

## 9. 经验与升级（`WinExp` / `GainExp` / `LevelUp`）

Rust 侧：`World::win_experience` / `gain_experience` / `player_level_up`、
`crates/mir-server/src/exp.rs`（等级经验表）。

### 9.1 经验从哪里来

`MonsterObject.Die` 在广播 `ObjectDied` 之后：

```csharp
if (EXPOwner != null && EXPOwner.Node != null && Master == null && EXPOwner.Race == ObjectType.Player)
    EXPOwner.WinExp(Experience, Level);                    // MonsterObject.cs:982-992
if (Respawn != null) Respawn.Count--;                      // :994-995
```

`WinExp(amount, targetLevel)`（`PlayerObject.cs:794-845`）→ `ReduceExp` → `Settings.ExpRate`：

```csharp
// HumanObject.ReduceExp（HumanObject.cs:2365-2381）
if (Level < targetLevel + 10 || !Settings.ExpMobLevelDifference)
    expPoint = amount;
else
    expPoint = amount - (int)Math.Round(Math.Max(amount / 15, 1) * ((double)Level - (targetLevel + 10)));
if (expPoint <= 0) expPoint = 1;
```

`Math.Round` 是 .NET 的**银行家舍入**，Rust 侧用 `f64::round_ties_even` 对齐。
`Settings.ExpRate` 默认 1.0（`Settings.cs:112`），`ExpMobLevelDifference` 默认 true（`:98`）。

### 9.2 `GainExp`（`PlayerObject.cs:846-931`）

```csharp
if (!CanGainExp || amount == 0) return;                    // :848-850
Experience += amount;                                      // :891
Enqueue(new S.GainExperience { Amount = amount });         // :893，只给本人
if (Experience < MaxExperience) return;                    // :905
if (Level >= ushort.MaxValue) return;                      // :906
while (experience >= MaxExperience) {                      // :911
    Level++;
    experience -= MaxExperience;
    RefreshLevelStats();                                   // → RefreshMaxExperience
    if (Level >= ushort.MaxValue) break;
}
Experience = experience;                                   // :921
LevelUp();                                                 // :923
```

### 9.3 `LevelUp` 发出的包（顺序照抄）

| # | 包 | 收件人 | 出处 |
|---|---|---|---|
| 1 | `HealthChanged` | 本人 | `SetHP(Stats[HP])` → `SendHealthChanged`（`HumanObject.cs:848`） |
| 2 | `HealthChanged` | 本人 | `SetMP(Stats[MP])` → `SendHealthChanged`（`:849`） |
| 3 | `ObjectLeveled` | **周围玩家**（`Broadcast` 不含本人） | `HumanObject.cs:851` |
| 4 | `LevelChanged` | 本人 | `Enqueue`，`PlayerObject.cs:938`（**不是广播**） |

1、2 是两条内容不同的 `HealthChanged`（先补 HP、再补 MP），客户端以最后一条为准。

### 9.4 经验表：来源与兜底

| 项 | 说明 |
|---|---|
| Crystal 来源 | `Configs/ExpList.ini`，`[Exp]` 段里 `Level1=100` … `Level500=…`（`Settings.cs:839-848`） |
| 取值语义 | `MaxExperience = ExperienceList[Level - 1]`（`PlayerObject.cs:9627-9630`）——**本级升到下一级**的需求 |
| Rust 候选路径 | `config/ExpList.ini` → `config/Explist.ini` → 素材索引里的 `resource/community/*/Configs/ExpList.ini`（cjlaaa / Jev / Daneo1989） |
| 缺失/解析失败 | 退回近似曲线 **`exp = level^3 * 100`**，并 `tracing::warn!` 一次（`ExpTable::discover`） |
| 默认表 | `World::new` 用兜底曲线（测试确定性）；生产在 `server.rs::build_world` 里 `set_exp_table(ExpTable::discover())` |

`ExpTable` 的 API：`exp_for_level(level)`、`level_for_total_exp(total)`、`total_exp_for_level(level)`、
`parse_ini` / `from_ini` / `load` / `discover` / `fallback`。

---

## 10. 玩家死亡与复活（`PlayerObject.Die` / `HumanObject.Revive`）

### 10.1 死亡（`PlayerObject.cs:578-667`）

```csharp
HP = 0; Dead = true;                                       // :643-644
LogTime = Envir.Time; BrownTime = Envir.Time;              // :646-647
Enqueue(new S.Death { Direction, Location });              // :649，只给本人
Broadcast(new S.ObjectDied { ObjectID, Direction, Location });  // :650，给周围玩家
// 之后清毒、移除 RemoveOnDeath 的 buff、CallDefaultNPC(Die)、统计
```

本作额外记录 `death_time` 与 `revive_time = death_time + PLAYER_REVIVE_DELAY_MS`。

### 10.2 复活（`PlayerObject.cs:1379-1391` → `HumanObject.cs:1690-1721`）

```csharp
if (!Dead) return;
Dead = false;
SetHP(hp);                                                 // → S.HealthChanged
CurrentMap.RemoveObject(this);
Broadcast(new S.ObjectRemove { ObjectID });                // → 周围玩家
CurrentMap = CurrentMap; CurrentLocation = CurrentLocation;   // 原地（源码里就是两行空操作）
CurrentMap.AddObject(this);
Enqueue(new S.MapChanged { ... });                         // 本作不发（无地图元数据）
Enqueue(new S.Revived());                                  // → 本人
Broadcast(new S.ObjectRevived { ObjectID, Effect = effect });  // → 周围玩家
```

- `TownRevive`（`PlayerObject.cs:1392-1450`）走另一条路：传送到绑定回城点，`SetHP`/`SetMP` 补满，
  再发 `MapChanged` + `Revived` + `ObjectRevived`。本作**没有**回城复活，只有原地复活。
- 本作在 `ObjectRevived` 之后**多补一条 `ObjectPlayer`**：Crystal 的 `Map.AddObject`
  （`Map.cs:2361-2373`）只把对象塞回格子、不再广播 `GetInfo()`，而我们的客户端把 `ObjectRemove`
  当彻底删除，不补外观包的话复活后的角色在旁人客户端里会永久消失。

### 10.3 自动复活（本作扩展）

Crystal 服务端**没有**复活计时器：客户端在死亡动画结束（`DeadTime`）后弹窗，玩家确认才发
`C.TownRevive`（`Client/MirScenes/GameScene.cs:1269-1281` → `Server/MirNetwork/MirConnection.cs:1643-1648`）。
本作在 `World::tick_players` 里到点自动调 `World::revive_player`，等待时间
`PLAYER_REVIVE_DELAY_MS = 10000`（取客户端「死亡动画 + 弹窗」的量级）。

死亡期间走/跑/转身/攻击全部返回 `MoveError::Dead` / `CombatError::Dead`。

---

## 11. 玩家自然回血回蓝（`HumanObject.ProcessRegen`）

Rust 侧：`World::process_player_regen`（在 `tick_players` 里每 tick 调用一次）。

```csharp
if (Dead) return;                                                   // :552
if (CanRegen) {                                                     // :556，即 Time >= RegenTime
    RegenTime = Envir.Time + RegenDelay;                            // :558，10000ms
    if (HP < Stats[HP]) healthRegen += (int)(Stats[HP] * 0.03F) + 1;   // :562
    if (MP < Stats[MP]) manaRegen   += (int)(Stats[MP] * 0.03F) + 1;   // :568
}
if (healthRegen > 0) { ChangeHP(healthRegen); BroadcastDamageIndicator(DamageType.Hit, healthRegen); }  // :635-639
if (manaRegen > 0) ChangeMP(manaRegen);                             // :647
```

要点（都是 Crystal 原文行为）：

1. **回复只和 `RegenTime` 有关**（`CanRegen`，`HumanObject.cs:145-151`）。`RegenTime` 会被
   **攻击/施法/受击**推后一个完整周期（`HumanObject.cs:2976`、`:3450`、`:7196`、`:7295`、`:7367`），
   但 **走路/跑步不会重置它**——所以 Crystal 里边走边回血是正常现象。
   本作的测试 `attacking_postpones_regen_but_walking_does_not` 就是钉这个行为。
2. 回血后**多一条 `DamageIndicator{Hit, +N}`**（`:638`），本人与周围玩家都看得到。
3. 回蓝走的也是 `ChangeMP` → `SendHealthChanged`（`S.HealthChanged{HP,MP}`）：
   **Crystal 没有 `S.MPChanged` 这种包**。
4. 满血时不会发任何血量包，但 `RegenTime` 照样推进（`:556-558`）。

简化：`Stats[HealthRecovery]`/`Stats[SpellRecovery]` 与药水/治疗/吸血三个计时器
（`HumanObject.cs:563`、`:569`、`:573-633`）未实现——本作没有装备与消耗品。

---

## 12. Crystal 里值得记一笔的地方

1. **暴击不对称**：`MonsterObject.Attacked` 判暴击（`MonsterObject.cs:2594`），
   `HumanObject.Attacked(MonsterObject)` 不判（`HumanObject.cs:7222-7315`），
   只有 `Attacked(HumanObject)`（PvP）才判（`HumanObject.cs:7156`）。
   结果：**怪物打玩家永远不会暴击**。本作照抄。
2. **`DamageIndicator` 的 `Damage` 是负数**（`armour - damage`），客户端用它做飘字。
3. **未命中也会抽护甲**：`GetArmour` 里闪避失败后仍然执行 `GetDefencePower`，随机序列照旧推进。
4. **怪物攻击 0 伤害也会播动作**：`Attack()` 先广播 `ObjectAttack` 再判 `damage == 0` 提前返回
   （`MonsterObject.cs:2163-2169`），只是不排队结算。
5. **严格 `>` 与 `>=` 混用**：怪物动作/调度用 `now > deadline`（`Envir.cs:2130`、`MonsterObject.cs:640`），
   玩家动作和聊天用 `now >= ActionTime`（`HumanObject.cs:123`）。差 1ms，但影响同一 tick 的判定。
6. **`ProcessTarget` 里的 `!CanAttack` 早退**导致「攻击后摇（`AttackTime`）期间怪物不移动」，
   所以怪物的追击是「跳一下、打一下、再跳」的节奏，而不是边打边追。
7. **`MonsterObject.Walk` 不看门、不查 `CheckMovement`**：怪物会卡在关闭的门前，但不会触发传送点。
8. **`ObjectHealth` 不给周围玩家**：怪物血量只有 `EXPOwner`（首个攻击者）能看到；
   这解释了 Crystal 里「别人打过的怪我看不到血条」。
9. **`RespawnTimer` 不是「间隔倍率」而是「全局 tick 计数器」**：`BaseSpawnRate = 20`（分钟/tick），
   每过一个 `CurrentDelay` 计数器 `+1`（`RespawnTimer.cs:96-123`）。`RespawnTicks != 0` 的刷怪组
   比较的是**计数器**（`Map.cs:746`），时间制的组则完全不看它（`Map.cs:747`）。
   所以「`Delay` 分钟乘以一个全局倍率」在 Crystal 里并不存在——那是本作的等价折叠（见 §8.4）。
10. **刷怪是「到点整批补」，不是「每死一只补一只」**：`Map.cs:749-755` 一次补满所有缺口。
11. **没有 `Envir.ProcessDeadPlayers`，也没有 `ReviveTime`**：全仓库搜不到这两个名字；
   死亡后的复活完全由客户端驱动（`GameScene.cs:1269-1281` → `C.TownRevive` → `TownRevive()`）。
   本作的自动复活是扩展（§10.3）。
12. **`HumanObject.Revive` 里的 `CurrentMap = CurrentMap; CurrentLocation = CurrentLocation;`
   是两行空操作**（`HumanObject.cs:1703-1704`）——作者显然想写「传送回出生点」，实际效果是原地复活。
   本作按**实际效果**（原地）实现。
13. **等级超过经验表长度会爆升**：`MaxExperience = Level < Count ? ExperienceList[Level-1] : 0`
   （`PlayerObject.cs:9629`），等级 ≥ 501 时为 0，而 `while (experience >= MaxExperience)` 对 0 恒真
   → 一路升到 65535 级。本作在 `max_experience <= 0` 时停止升级。
14. **`ExperienceList` 永远有 500 项**：`LoadEXP` 的循环固定 `1..=500`，文件缺项时沿用上一项
   （`Settings.cs:841-848`），所以「文件里只写了 20 级」在服务器眼里是「20 级之后经验不变」。
15. **没有 `S.MPChanged` 包**：回蓝复用 `S.HealthChanged{HP,MP}`（`HumanObject.cs:870-884`）。
16. **走路不会推迟回血**：`RegenTime` 只在攻击/施法/受击时被重置（`HumanObject.cs:2976`、`:3450`、
   `:7196`、`:7295`、`:7367`），`Walk`/`Run` 里没有这一行。所以「站着才回血」在 Crystal 里
   并不成立——回血与移动完全正交。
17. **死亡时 `Die()` 早于 `SendHealthChanged()`**：`ChangeHP` 先把 HP 打到 0、判死、发 `S.Death` +
   `ObjectDied`，然后才发 `S.HealthChanged`（`HumanObject.cs:885-907`）。客户端会先收到死亡再收到
   「HP=0」。本作照抄这个顺序。
18. **`MapInfo.Respawns` 的 `Delay = 0`** 会被 `Math.Max(1, ...)` 兜成 1 分钟（`Map.cs:759`）；
   同理 `RandomDelay > 0` 且 `Delay < RandomDelay` 时，间隔会落在 1..=(RandomDelay - 1) 分钟。

---

## 13. 本作的简化（每条都在代码里用 `// 简化：` 标注）

| 简化 | 原因 |
|---|---|
| 广播半径沿用 `VIEW_RANGE = 10`（Crystal `DataRange = 16`） | 避免改动已通过的移动/聊天测试；索敌视野（7）不受影响 |
| 只有「野生敌对怪物」一条 AI 分支 | 没有宠物（`Master`/`PMode`）、巡逻路线（`Route`）、毒/麻痹/buff、`ShockTime`、`Rage`、隐身、召唤、传送 |
| 怪物只打玩家，不打怪物 | 野生怪物之间 `IsAttackTarget` 返回 false（`MonsterObject.cs:2537-2539`），本来就不会互殴 |
| 玩家近战只打怪物 | PK/红名/行会战未实现；`HumanObject.Attack` 的技能分支（Slaying/Thrusting/HalfMoon/…）全部跳过 |
| 没有技能/buff，玩家属性=等级曲线基础值 + 装备加成 | 装备系统见 `docs/world/items.md`；技能/buff 仍未实现 |
| `reflect`/`CriticalRate` 等属性基础值恒 0（但抽签保留） | 只影响装备加成，抽签保留才能保证随机序列与 Crystal 一致 |
| 刷怪用 `SpawnGroup`（半径内撒点） | Crystal 用地图预计算的 `WalkableCells`；`mir-db` 接入后用真实刷新点 |
| 刷怪「每个周期只补 1 只」 | Crystal 一次补齐（`Map.cs:749-755`）；逐只补避免到点瞬间刷出一整群 |
| `RespawnTicks`（tick 制刷怪）折成时间制 × `respawn_tick_scale` | 还没有在线人数档位（`DelayTicks`）；字段已在 `mir-db` 里，接入时换算即可 |
| 尸体 180s 后移除，掉落物自己再活 3 分钟 | Crystal 地面物品 `Settings.ItemTimeOut = 30` 分钟（`Settings.cs:115`），见 `docs/world/items.md` §2 |
| 死亡后由服务端计时自动原地复活（10s） | Crystal 由客户端弹窗 + `C.TownRevive`；服务端自动复活是为了让单人测试与挂机场景不卡死 |
| 复活不发 `MapChanged` | 世界层没有地图标题/小地图/光照等元数据（只在 `MapInformation` 发过一次），且复活不换图 |
| 复活额外补一条 `ObjectPlayer` | 我们的客户端把 `ObjectRemove` 当彻底删除；Crystal 不发（`Map.cs:2361-2373` 只把对象塞回格子） |
| 升级需求用「兜底曲线」当默认 | `World::new` 不碰文件系统，保证测试确定性；生产在 `server.rs` 里装 `ExpList.ini` |
| 没有安全区 | 地图安全区数据未接入；Crystal 里安全区会让怪物无法选中玩家 |
| 没有 `ActionList` 数据结构，用世界级 `pending_hits` | 单目标场景下与「每对象各自队列」等价；顺序按入队顺序 |
| 玩家回复只做「自然回血/回蓝」 | 药水/治疗术/吸血三个计时器与 `HealthRecovery` 装备属性未实现 |

---

## 14. 测试清单（`crates/mir-server/tests/monster_combat.rs`）

| 用例 | 断言 |
|---|---|
| `monster_acquires_target_inside_view_range` | 距离 4 ≤ 视野 7 → `target` 指向该玩家 |
| `monster_outside_view_range_does_not_aggro` | 距离 8 > 视野 7 → `target` 仍为 `None` |
| `monster_walks_one_step_toward_target_and_broadcasts_walk` | 走一格、朝向正确、`CellTime/ActionTime/MoveTime` 三个计时正确、玩家收到 `ObjectWalk` |
| `monster_attacks_after_delay_and_damages_player` | `AttackTime ≈ now+2500`、立刻广播 `ObjectAttack`、**300ms 内不掉血**、300ms 后掉 5 血、本人收 `Struck`+`HealthChanged`、旁观者收 `ObjectStruck`+`DamageIndicator(-5)` |
| `monster_kills_player_and_sends_death_packet` | 玩家 `dead`、`hp=0`、收到 `Death` 与 `HealthChanged(0)`、死后再走/攻击被拒 |
| `player_kills_monster_and_broadcasts_object_died_then_remove` | 一击 50 打死 10 血怪、攻击者看到 `ObjectStruck`、广播 `ObjectDied`、3 分钟后尸体移除并广播 `ObjectRemove` |
| `player_entering_world_receives_nearby_monsters` | 先有怪后有人 → 进场即收 `ObjectMonster` |
| `walking_out_of_range_removes_monster_and_walking_back_adds_it` | 走出 10 格 → `ObjectRemove`；走回 → `ObjectMonster` |
| `rng_is_reproducible_for_the_same_seed` | 同一 seed 下 6 次 tick 后位置与朝向完全一致 |

### 14.1 生命周期四件套（`crates/mir-server/tests/world_lifecycle.rs`，12 个用例）

| 用例 | 断言 |
|---|---|
| `register_spawn_group_fills_count_and_reports_state` | 登记即铺满 `count`；`respawn_state()` 的 `alive/count/center/next_spawn_at` 正确；怪带上 `respawn_group` 下标 |
| `killing_one_monster_respawns_one_after_delay_and_never_exceeds_count` | 死 1 只 → `alive-1`、排程 +60000ms；差 1ms 不补；到点补 1 只；再等 10 分钟也不超过 `count` |
| `random_delay_stays_inside_crystals_window` | `Delay=3, RandomDelay=2` → 间隔落在 1..=4 分钟；`RandomDelay=0` → 固定 2 分钟 |
| `respawn_tick_scale_stretches_the_interval` | 倍率 2.0 时 1 分钟不补、2 分钟才补 |
| `killing_a_monster_grants_experience_and_notifies_only_the_killer` | 经验 +50、`GainExperience{50}` 只给杀手、没升级就没有 `LevelChanged` |
| `high_level_killer_gets_reduced_experience_from_a_weak_monster` | 20 级打 1 级怪：`ReduceExp` 150 → 60 |
| `enough_experience_levels_up_recalculates_stats_and_sends_packets` | 120 经验 → 2 级、余 20、`max_experience=800`、属性重算、HP/MP 补满、本人收 `LevelChanged`、旁观者收 `ObjectLeveled`、本人**不**收 `ObjectLeveled` |
| `master_experience_table_comes_from_the_index_sample_when_present` | `ExpTable::discover()` 首级需求 100；`set_exp_table` 生效 |
| `player_dies_then_revives_in_place_and_can_walk_again` | 死亡 → `Death`、`revive_time`；死亡期间不能走；差 1ms 不复活；到点原地满血满蓝、收 `Revived` + `HealthChanged`；旁观者收 `ObjectRemove` + `ObjectRevived` + `ObjectPlayer`；复活后能走一步 |
| `wounded_idle_player_regenerates_after_the_delay_and_receives_health_changed` | 10s 前不回；到点 +1 HP / +1 MP、`HealthChanged{11}`、`DamageIndicator{+1}`、`regen_time` 后推 10s |
| `full_health_player_resets_the_timer_without_healing` | 满血不发 `HealthChanged`，但 `regen_time` 照样推进 |
| `attacking_postpones_regen_but_walking_does_not` | 出手把 `regen_time` 推后 10s；走路不改变它（Crystal 原样行为），走路期间不回血 |

### 14.2 经验表单元测试（`crates/mir-server/src/exp.rs`，5 个用例）

`parses_crystal_exp_list`（缺项沿用上一项）、`rejects_garbage_instead_of_guessing`、
`fallback_curve_is_cubic_and_starts_at_crystal_first_entry`、`total_exp_and_level_lookup_are_inverse`、
`discovers_the_index_sample_when_available`（有 `ExpList.ini` 时首级 = 100 且单调）。
