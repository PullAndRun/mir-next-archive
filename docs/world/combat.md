# 战斗与怪物 AI

> 参考实现根目录：`C:\Users\diana\ref\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：`Server/MirObjects/MapObject.cs`（`GetAttackPower`/`GetDefencePower`/`GetArmour`/广播）、
> `Server/MirObjects/HumanObject.cs`（玩家攻击、受击、死亡）、`Server/MirObjects/MonsterObject.cs`（怪物 AI、攻击、死亡）、
> `Server/MirDatabase/MonsterInfo.cs`（怪物默认值）、`Shared/BaseStats.cs`（玩家基础属性曲线）。
>
> Rust 侧实现：`crates/mir-server/src/combat.rs`（属性与结算）、`crates/mir-server/src/monster.rs`（怪物与 AI）、
> `crates/mir-server/src/world.rs`（世界 tick、延迟伤害、视野、刷怪）。
> 测试：`crates/mir-server/tests/monster_combat.rs`（9 个端到端用例）。

---

## 1. 结论先行

- **一次物理攻击是「两段式」的**：动作包立刻广播，伤害 **300ms 后**才由延迟动作结算。
  玩家和怪物都是这样（`HumanObject.cs:3201`、`MonsterObject.cs:2171`）。
- **命中判定在防御方**：`GetArmour` 先掷闪避（`Random.Next(Agility + 1) > Accuracy`），再掷护甲
  （`GetDefencePower`）；`armour >= damage` 直接算 Miss。
- **暴击只发生在「打怪物」这一侧**：`MonsterObject.Attacked` 有暴击判定，
  `HumanObject.Attacked(MonsterObject)` **没有**——怪物打玩家永远不暴击（Crystal 原样行为，见 §8）。
- **怪物 AI 有 2 秒调度粒度**：`SetOperateTime` 让每只怪最快 2 秒被 `Process()` 一次；
  附近没玩家时整只怪不思考（`Alone`）。
- **怪物不会跑、不查门、不触发传送**（`MonsterObject.Walk`，`MonsterObject.cs:2095-2149`）。

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

### 5.3 玩家死亡：`PlayerObject.Die`（`PlayerObject.cs:578-667`）的最小实现

- `hp = 0`、`dead = true`；
- 本人收到 `S.Death`（坐标 + 朝向）；
- 周围玩家收到 `S.ObjectDied`；
- 尸体**不会**被移除（Crystal 会等复活），`dead` 状态屏蔽走/跑/转身/攻击（`MoveError::Dead`）。

### 5.4 怪物死亡：`MonsterObject.Die`（`MonsterObject.cs:966-1006`）

- `HP = 0`、`Dead = true`、`DeadTime = now + 180000`；
- 广播 `S.ObjectDied`；
- `DeadTime` 到期后从世界移除并广播 `S.ObjectRemove`（`Despawn`，`MapObject.cs:363`）；
- **经验与掉落未实现**（TODO：`EXPOwner.WinExp` / `Drop()`）。

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

## 8. Crystal 里值得记一笔的地方

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

---

## 9. 本作的简化（每条都在代码里用 `// 简化：` 标注）

| 简化 | 原因 |
|---|---|
| 广播半径沿用 `VIEW_RANGE = 10`（Crystal `DataRange = 16`） | 避免改动已通过的移动/聊天测试；索敌视野（7）不受影响 |
| 只有「野生敌对怪物」一条 AI 分支 | 没有宠物（`Master`/`PMode`）、巡逻路线（`Route`）、毒/麻痹/buff、`ShockTime`、`Rage`、隐身、召唤、传送 |
| 怪物只打玩家，不打怪物 | 野生怪物之间 `IsAttackTarget` 返回 false（`MonsterObject.cs:2537-2539`），本来就不会互殴 |
| 玩家近战只打怪物 | PK/红名/行会战未实现；`HumanObject.Attack` 的技能分支（Slaying/Thrusting/HalfMoon/…）全部跳过 |
| 没有装备/技能/buff，玩家属性=等级曲线基础值 | 装备系统未实现，等 `mir-db` 接装备后重算 |
| `reflect`/`CriticalRate` 等属性恒 0（但抽签保留） | 只影响装备加成，抽签保留才能保证随机序列与 Crystal 一致 |
| 刷怪用 `SpawnGroup`（半径内撒点） | Crystal 用地图预计算的 `WalkableCells`；`mir-db` 接入后用真实刷新点 |
| 尸体 180s 后直接移除，没有掉落/经验 | 掉落物、经验表在 `mir-db` 里 |
| 死亡后不复活、不变幽灵 | 复活逻辑（`PlayerObject.Die` 后半段）留给后续里程碑 |
| 没有安全区 | 地图安全区数据未接入；Crystal 里安全区会让怪物无法选中玩家 |
| 没有 `ActionList` 数据结构，用世界级 `pending_hits` | 单目标场景下与「每对象各自队列」等价；顺序按入队顺序 |

---

## 10. 测试清单（`crates/mir-server/tests/monster_combat.rs`）

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
