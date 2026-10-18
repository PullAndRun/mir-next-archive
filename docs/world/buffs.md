# 增益（buff）容器：登记 / 属性叠加 / 到期 / 包

> 对应代码：`crates/mir-server/src/buffs.rs`、`Player.buffs`；规则逐条对照 Crystal
> （参考根目录 `C:\Users\diana\ref\Crystal`，下文只写相对路径）。

---

## 1. 结论先行

| 问题 | 答案 |
|---|---|
| 增益存在哪？ | `World::players` 里每个 `Player` 的 `buffs: Vec<ActiveBuff>`（Crystal `MapObject.Buffs`，`Server/MirObjects/MapObject.cs:49`） |
| 到期怎么算？ | 存**绝对到期时刻** `expire_at`（服务器单调时钟），每 tick 精确判定；Crystal 存「剩余时长」并每秒扣一次（`HumanObject.cs:439-448`） |
| 属性怎么叠加？ | `World::refresh_stats` 里**装备之后、`apply_caps` 之前**把每个 buff 的 `Stats` 累加（Crystal `RefreshBuffs`，`HumanObject.cs:2320-2350`） |
| 到期会发包吗？ | `S.RemoveBuff` 给本人（可见的再广播）；上限变化时补 `S.HealthChanged` + `S.ObjectHealth` |
| 谁会上线？ | 9 个增益技能 + 11 种药水增益；**不持久化**（Crystal 的 `CharacterInfo` 存档里没有 buff 块） |
| 行会 buff 在这个模块吗？ | **不在**。行会增益由 `Guild::buffs` 持有、`World::refresh_stats` 里的 `apply_guild_buffs` 叠加，没有 `S.AddBuff`；见 `docs/world/guild.md` §8。 |
| 新增的协议包？ | **没有**。`S.AddBuff`/`S.RemoveBuff`/`S.ObjectEffect` 在 `mir-protocol` 里早就存在 |

---

## 2. 常量速查

| 名称 | 值 | Rust 常量 | Crystal |
|---|---|---|---|
| 一秒毫秒数 | 1000 | `buffs::SECOND_MS` | `Settings.Second = 1000`（`Server/Settings.cs:16`） |
| 盾类区域半径 | 3（7×7） | `buffs::SHIELD_AREA_RADIUS` | `Map.cs:1052-1057` |
| 气功波概率下限 | 2 | `buffs::ENERGY_SHIELD_MIN_CHANCE` | `HumanObject.cs:4762`（`if (chance < 2) chance = 2;`） |
| 魔法盾每点伤害扣时 | 60ms | `buffs::MAGIC_SHIELD_MS_PER_DAMAGE` | `HumanObject.cs:7165`（`(damage - armour) * 60`） |
| 药水回复分批间隔 | 200ms | `world::PLAYER_POT_DELAY_MS` | `HumanObject.PotDelay = 200`（`HumanObject.cs:186`） |
| 药水每批基础量 | 5 | `world::POT_REGEN_BASE` | `5 + (Level / 10)`（`HumanObject.cs:577`） |
| 一分钟 | 60000ms | `items::MINUTE_MS` | `Settings.Minute`（`Settings.cs:16`） |

---

## 3. 数据结构

```rust
// crates/mir-server/src/buffs.rs
pub struct ActiveBuff {
    pub kind: BuffKind,     // 增益种类（Crystal BuffType 的子集）
    pub expire_at: i64,     // 绝对到期时刻（毫秒）
    pub values: Vec<i32>,   // 技能附加参数（本作目前恒为空，与 ClientBuff.Values 对齐）
    pub stats: Stats,       // 属性加成（mir_protocol::data::Stats，即 Crystal 的 Stats 字典）
    pub caster_id: u32,     // 施法者（Crystal Buff.Caster，不上行）
}
```

`BuffKind` 覆盖：

| 分类 | 种类 |
|---|---|
| 技能 | `ProtectionField`、`MagicShield`、`SoulShield`、`BlessedArmour`、`EnergyShield`、`Haste`、`LightBody`、`Fury`、`MagicBooster` |
| 药水（`Shape == 3/4/5`） | `Impact`、`Magic`、`Taoist`、`Storm`、`HealthAid`、`ManaAid`、`Defence`、`MagicDefence`、`BagWeight`、`Exp`、`Drop` |

每个 `BuffKind` 都映射到 Crystal 的 `BuffType`（`Shared/Enums.cs:1334-1399`）、
`Visible` 标志与叠加规则，见 `BuffKind::buff_type` / `visible` / `stacks_duration`。

---

## 4. 9 个技能增益的公式与包序

`L` = 技能等级（0..=3）。

| 技能 | 加成 | 时长 | 生效时机 | Crystal |
|---|---|---|---|---|
| `ProtectionField` | `MinAC += a; MaxAC += a`，`a = round(MaxAC × (0.2 + 0.03L))` | `45 + 15L` 秒 | 立刻 | `HumanObject.cs:4955-4963`、`:3650-3651` |
| `Haste` | `AttackSpeed += 2L + 2` | `25 + 15L` 秒 | +500ms | `:6149-6153`、`:3544-3546` |
| `LightBody` | `Agility += 2(L+1)` | `30(L+1)` 秒 | +500ms | `:6187-6191` |
| `Fury` | `AttackSpeed += 4` | `60 + 10L` 秒 | +500ms | `:6160-6164` |
| `MagicShield` | `DamageReductionPercent += 10(L+2)` | `GetPower(GetAttackPower(MinMC,MaxMC)+15)` 秒 | +500ms | `:3590-3591`、`:6198-6204` |
| `MagicBooster` | `MinMC += b; MaxMC += b`（`b = 6 + 6L`）、`ManaPenaltyPercent += 6 + L` | 60 秒 | +500ms | `:4345-4350`、`:6228-6240` |
| `EnergyShield` | `EnergyShieldPercent += round(100 / chance)`、`EnergyShieldHPGain += GetPower(GetAttackPower(MinSC,MaxSC))`，`chance = max(2, 10 - (Luck/3 + L + 1))` | `30 + 50L` 秒 | 立刻 | `:4751-4783`、`:3672-3673` |
| `SoulShield` | `MaxMAC += Level/7 + 4`（受术者等级） | `GetAttackPower(MinSC,MaxSC) × 4 + 50(L+1)` 秒 | `距离 × 50 + 500` ms，7×7 | `:4512-4525`、`Map.cs:1046-1097` |
| `BlessedArmour` | `MaxAC += Level/7 + 4`（受术者等级） | 同上 | 同上 | 同上 |

### 4.1 包序（重要）

`World::add_buff` 的发包顺序与 Crystal `HumanObject.AddBuff`（`HumanObject.cs:7460-7489`）一致：

1. `MagicShield` 先广播 `S.ObjectEffect{MagicShieldUp}`（`CurrentMap.Broadcast`，**连自己**）；
2. `S.AddBuff` **一定**发给本人；
3. `Visible == true` 的 buff（`Fury`/`EnergyShield`/`MagicBooster`）再广播给同图视野内其它玩家；
4. `RefreshStats()` → 上限变化时补 `S.HealthChanged` + `S.ObjectHealth`。

`remove_buff` 对应 `S.RemoveBuff`（本人 + 可见的广播）与 `MagicShield` 的 `MagicShieldDown` 特效。

### 4.2 魔法盾的「每点伤害扣 60ms」

受击路径（`World::damage_player`）在未命中判定之后、`S.Struck` 之前：

```csharp
if (HasBuff(BuffType.MagicShield, out Buff magicShield))
{
    var duration = (int)Math.Min(int.MaxValue, magicShield.ExpireTime - ((damage - armour) * 60));
    AddBuff(BuffType.MagicShield, this, duration, null);   // ResetDuration → 换成新时长
}
```

本作把 `expire_at` 减去 `伤害 × 60ms`，并像 Crystal 一样补发一条 `S.AddBuff`
（不可见 buff 只发给本人）。**没有**按 `damage - armour` 的绝对值做下限：扣成负数等价于「立刻过期」。

### 4.3 叠加规则

| 类别 | Crystal `BuffStackType` | 行为 |
|---|---|---|
| 9 个技能增益 | `ResetDuration` | 重复施放只把到期时刻推后，**属性沿用第一次**（`MapObject.cs:591-595`） |
| 药水增益 | `StackDuration` | 时长累加（`MapObject.cs:596-600`） |

`MagicShield` 特殊：`CompleteMagic` 开头 `if (HasBuff(BuffType.MagicShield, out _)) return;`
（`HumanObject.cs:6200`）——还挂着盾时再施放**不刷新也不加经验**，但蓝已经扣了、`S.Magic` 已经发了。

---

## 5. 属性叠加（`World::refresh_stats`）

```
base(等级/职业) → 装备 → **buff** → apply_caps → 夹当前 HP/MP
```

对应 `HumanObject.RefreshStats`（`HumanObject.cs:1730-1769`）的
`RefreshLevelStats` → `RefreshEquipmentStats` → `RefreshBuffs` → `RefreshStatCaps`。

`add_stat`（`items.rs`）把 `Stats` 字典里的键映射到 `CombatStats` 字段；
新增的四个 buff 专用属性：

| Crystal `Stat` | `CombatStats` 字段 | 谁写入 | 谁读 |
|---|---|---|---|
| `DamageReductionPercent`（124） | `damage_reduction_percent` | `MagicShield` | `combat::resolve_monster_attack`（`HumanObject.cs:7244-7248`） |
| `EnergyShieldPercent`（125） | `energy_shield_percent` | `EnergyShield` | `World::damage_player`（`:7262-7271`） |
| `EnergyShieldHPGain`（126） | `energy_shield_hp_gain` | `EnergyShield` | 同上 |
| `ManaPenaltyPercent`（127） | `mana_penalty_percent` | `MagicBooster` | `UserMagic::cost_with_penalty`（`:3375-3398`） |

药水的 `ExpRatePercent`/`ItemDropRatePercent` 只记录在 buff 里并下发给客户端，**服务端暂不消费**
（本作的 `WinExperience` 还没乘经验加成，掉落表也没乘爆率）。

---

## 6. 协议包一览（本作实际发的）

| 方向 | 包 | 时机 | 字段要点 | Crystal |
|---|---|---|---|---|
| S→C | `S.AddBuff`（144） | 增益生效/刷新/魔法盾扣时 | `ClientBuff{type, visible, object_id, expire_time, infinite, paused, stats, values}` | `HumanObject.cs:7474-7481` |
| S→C | `S.RemoveBuff`（145） | 到期、被顶掉、显式移除 | `type_, object_id` | `HumanObject.cs:452-458` |
| S→C | `S.ObjectEffect`（124） | 魔法盾开/关 | `object_id, effect = MagicShieldUp/Down` | `HumanObject.cs:7466-7470`、`:485-487` |
| S→C | `S.HealthChanged`（77） | 上限变化（buff 加/撤） | `hp, mp` | `HumanObject.cs:870-874` |
| S→C | `S.ObjectHealth`（139） | 上限变化 | `object_id, percent, expire` | **本作扩展**，见 §8 |

两个关于 `ClientBuff.ExpireTime` 的坑（Crystal 原样）：

* 上行的是**剩余毫秒数**，不是 Unix 时间戳——客户端收到后自己 `+= CMain.Time`
  （`Client/MirScenes/GameScene.cs:5181`）；
* `ClientBuff.ObjectID` 是**被增益的对象**（`Buff.ObjectID = ObjectID`，`MapObject.cs:577`），
  不是施法者；施法者 `Buff.Caster` 从不上行（`BuffInfo.cs:255`）。

---

## 7. 测试清单（`crates/mir-server/tests/use_items.rs`，18 个）

| # | 用例 | 断言要点 |
|---|---|---|
| 1 | `normal_potion_fills_the_pot_pool_and_heals_over_time` | 药水池 +30、堆叠减 1、200ms 后 +5、`S.HealthChanged` |
| 2 | `normal_potion_at_full_health_is_rejected_without_consuming` | `PotionNoEffect`、失败包、聊天、不消耗 |
| 3 | `mana_potion_restores_mp_through_the_pot_pool` | 蓝药也走药水池 |
| 4 | `sun_potion_heals_the_flat_amount_immediately` | 太阳水立刻 +50（**固定值，不是百分比**） |
| 5 | `potion_class_requirement_is_enforced` | `WrongClass` + 提示 + 不消耗 |
| 6 | `potion_level_requirement_is_enforced` | `LowLevel` + 提示 |
| 7 | `dead_player_cannot_use_a_potion_but_a_resurrection_scroll_revives` | 死亡拒绝药水；复活卷轴复活并消耗 |
| 8 | `unknown_item_shapes_and_scrolls_are_rejected_without_consuming` | 传送卷拒绝 + 提示 |
| 9 | `using_a_book_still_learns_the_spell` | 书走 `use_book`，`S.NewMagic` + `S.UseItem{true}` |
| 10 | `protection_field_raises_defence_until_it_expires` | +2 AC、`S.AddBuff`、45 秒后 `S.RemoveBuff` + 属性回退 |
| 11 | `haste_raises_attack_speed_and_shortens_the_attack_delay` | 500ms 后 +2 攻速、攻击间隔 -120ms |
| 12 | `magic_shield_reduces_incoming_monster_damage` | 20% 减伤：10 → 8 |
| 13 | `visible_buffs_are_broadcast_and_magic_shield_sends_object_effects` | `Fury` 广播 `S.AddBuff`；魔法盾只广播 `ObjectEffect` |
| 14 | `expiring_a_health_buff_resends_health_and_object_health` | 药水 buff 加/撤各发 `S.HealthChanged` + `S.ObjectHealth` |
| 15 | `magic_booster_raises_magic_power_and_mana_cost` | +6 MC、耗蓝 +6% |
| 16 | `area_shields_buff_nearby_players_with_level_scaled_defence` | 幽灵盾 7×7、`Level/7 + 4` |
| 17 | `drop_gold_spawns_a_gold_pile_and_loses_gold` | 落地/扣钱/`S.LoseGold`/`S.ObjectGold`/钱不够报错 |
| 18 | `real_database_exposes_potions_and_shield_spells` | Jev 数据里有药水与护盾技能模板 |

模块内单元测试（`src/buffs.rs`）：`BuffKind` → `BuffType` 映射与可见性、
加成/时长换算（`minutes_to_ms`）、百分比血量夹取。

---

## 8. 与 Crystal 的差异（都是有意为之）

1. **到期精度**：Crystal 每秒扣一次 `ExpireTime` 并带 1 秒的 `NextTime` 节流
   （`HumanObject.cs:439-448`），本作每 tick 精确比较 `expire_at`，
   因此到期最多比 Crystal 早 1 秒。
2. **移除是立即的**：Crystal 的 `RemoveBuff` 只打 `FlagForRemoval`，真正移除在下一秒的
   `ProcessBuffs`；本作立即移除并发包。
3. **玩家侧的血量广播**：Crystal 的 `BroadcastHealthChange` 对玩家只发给**队友**
   （`MapObject.cs:856-871`），本作没有队伍，于是上限变化时把 `S.ObjectHealth` 发给
   「本人 + 视野内玩家」，保证客户端血条即时刷新（**本作扩展**）。
4. **不持久化**：下线再上线 buff 就没了（Crystal 的 `CharacterInfo.Save` 里没有 buff 块，
   行为一致）。
5. **`Stat.ExpRatePercent` / `Stat.ItemDropRatePercent` 不参与服务端计算**：只记录在
   buff 里并下发给客户端（Crystal 在 `GainExp`/掉率里会读）。
6. **没有暂停机制**（`PauseBuff` / `BuffProperty.PauseInSafeZone`）：
   本作没有安全区暂停 buff 的需求（药水 buff 都不带 `PauseInSafeZone` 之外的属性）。

---

## 9. 发现的 Crystal 异常行为

1. **`AddBuff` 的 `ResetDuration` 不刷新属性**（`MapObject.cs:591-595`）：
   先放一个低等级护盾、再放高等级同一个护盾，加成仍然是第一次算出来的那个值。
2. **`MagicShield` 提前返回但蓝照扣**：`CompleteMagic` 在 `if (HasBuff(...)) return;`
   之前已经扣过蓝并发过 `S.Magic`（`HumanObject.cs:3442-3451`、`:6200`），
   玩家会「施法成功但什么也没发生」。
3. **`Buff.ExpireTime` 语义混用**：`Attacked` 里 `AddBuff(..., magicShield.ExpireTime - …)`
   传的是「剩余时长」（正确），但同名的 `ExpireTime` 在 `ClientBuff` 里也是剩余时长，
   而 `Buff.NextTime` / `LastTime` 是绝对时刻——三者混在一处极易写错（本作换成了绝对 `expire_at`）。
4. **`Buff.ToClientBuff` 的 `ObjectID` 是被增益者**，而 `Caster` 只在服务端用；
   客户端因此无法直接知道「谁给我加的盾」。
5. **`S.AddBuff` 每次受击都重发**（魔法盾扣时那条路径），频次随受击次数线性增长。

---

## 10. 还没做的

- **行会增益（guild buff）不属于本模块**：它由 `Guild::buffs` + `GuildSettings::buff_list`
  驱动（`crates/mir-server/src/guild_buffs.rs`），通过 `PlayerObject.RefreshGuildBuffs`
  等价路径叠加属性，**不**创建 `Player.buffs`、**不**发 `S.AddBuff`。
  详见 `docs/world/guild.md` §8。这里之后可能会复用 `Stats`/`apply_caps` 的顺序，
  但生命周期（分钟级、行会级、可持久化）与个人 buff 完全不同。
- 其余增益技能：`Rage`、`ImmortalSkin`、`UltimateEnhancer`、`Hiding`、`PetEnhancer`、
  `ElementalBarrier`（见 `magic::REGISTERED_UNSUPPORTED`）。
- 怪物/英雄的 buff（`MonsterObject.AddBuff`，`MonsterObject.cs:2838-2852`）。
- buff 的暂停（安全区/掉线）与 `PauseBuff`/`UnpauseBuff` 包。
- 药水增益里的经验/爆率真正参与结算。
