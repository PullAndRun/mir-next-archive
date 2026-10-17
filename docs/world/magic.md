# 技能（魔法）系统（学书 / 施法 / 伤害治疗结算 / 持久化）

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：`Server/MirObjects/HumanObject.cs`（`Magic`/`LevelMagic`/`CanCast`/`MagicCost`/`CompleteMagic`/`ProcessRegen`）、
> `Server/MirObjects/PlayerObject.cs`（`UseItem` 的书本分支、`Process(DelayedType.Magic)`）、
> `Server/MirEnvir/Map.cs`（地图级 `CompleteMagic`：HellFire/FireBang/MassHealing）、
> `Server/MirDatabase/MagicInfo.cs`（`MagicInfo`/`UserMagic`/`GetPower`/`GetDamage`/`GetDelay`）、
> `Server/MirDatabase/CharacterInfo.cs`（技能列表落盘）、
> `Server/MirNetwork/MirConnection.cs`（`C.Magic`/`C.MagicKey`/`C.SpellToggle` 入口）、
> `Shared/Enums.cs`（`Spell`）、`Shared/ServerPackets.cs`（`Magic`/`ObjectMagic`/`NewMagic`/`MagicLeveled`/`MagicDelay`/`UseItem`）。
>
> Rust 侧实现：`crates/mir-server/src/magic.rs`（数据、编解码、施法、延迟效果、经验）、
> `crates/mir-server/src/world.rs`（`Player.magics`/`Player.magic`、tick 调度、`ChangeMP`、治疗分段）、
> `crates/mir-server/src/gamedata.rs`（`MagicInfo` 查找）、
> `crates/mir-server/src/session.rs`（登录下发、下线存盘、`C.Magic`/`C.MagicKey`/`C.UseItem`）。
> 测试：`crates/mir-server/tests/magic.rs`（21 个确定性用例）+ `magic.rs` 模块内 6 个单元测试。

---

## 1. 结论先行

- **技能模板来自 `Server.MirDB` 的技能段**（`MirDb.magics`，`Envir.cs:2899-2905`）。
  Jev 样本里 **111 条**，按 `Spell` 去重后仍是 111 条（Crystal `Envir.MagicExists` 先出现者胜出）。
- **玩家技能是 `Vec<UserMagic>`**，字段 `spell/level/key/experience` 与 Crystal 一一对应；
  随角色存档往返，格式是 `int32 条数` + 每条 14 字节（`CharacterInfo.cs:458-463` + `MagicInfo.cs:135-144`）。
- **学技能只认 `ItemType::Book`**：书的 `Shape` 就是 `Spell`（`PlayerObject.cs:6078`），
  学会后发 `S.NewMagic` + `S.UseItem{Success=true}`，消耗一本。
- **施法流程逐条对齐 `HumanObject.Magic`**：`CanCast` → 学会 → 射程 → 写计时器 →
  冷却 → 蓝量 → 扣蓝 → 目标分派 → 延迟效果 → `S.Magic`（本人）+ `S.ObjectMagic`（广播）。
- **伤害走 `DefenceType::MAC`**（Crystal `CompleteMagic` 的 `target.Attacked(this, value, DefenceType.MAC, false)`，
  `HumanObject.cs:5929`）：魔法闪避 → 魔法防御 → 扣血 → 暴击抽签，与物理攻击共用
  `combat::resolve_player_attack`。
- **治疗走「先记 HealAmount、再每 600ms 放一段」**（`HumanObject.cs:602-617`），
  而不是一次性加满。
- **`S.MagicDelay` 只在技能升级时发一次**（`HumanObject.cs:6972-6976`），**不是每次施法都发**；
  每次施法里客户端靠 `S.Magic` + `ClientMagic.Delay` 自己算冷却。
- 本作已实现 8 个技能的效果（4 个单体、HellFire、FireBang、Healing、MassHealing），
  其余「登记但无效果」（原因见 §9）。

---

## 2. 常量速查

| 名称 | 值 | Rust 常量 | Crystal |
|---|---|---|---|
| 技能最高等级 | 3 | `magic::MAGIC_LEVEL_MAX` | `MagicInfo.cs:16`（只有 `Level1..3`）+ `HumanObject.cs:6968-6969`（`default:` 返回） |
| 施法动作锁 | 600ms | `magic::MAGIC_CAST_ACTION_MS` | `HumanObject.cs:186`（`MoveDelay = 600`）、`:3424`、`:3429` |
| 施法全局间隔 | 1800ms | `magic::MAGIC_SPELL_TIME_MS` | `HumanObject.cs:3425`（`SpellTime = Envir.Time + 1800`） |
| 单次修炼值 | 1..=3 | `magic::MAGIC_EXP_ROLL_MAX` | `HumanObject.cs:6905`（`Random.Next(3) + 1`） |
| HellFire 射程 | 4 格 | `magic::HELLFIRE_RANGE` | `HumanObject.cs:4107`（`DelayedAction(..., 4)`） |
| HellFire 步进 | 100ms | `magic::HELLFIRE_STEP_MS` | `Map.cs:885` |
| 区域半径 | 1（3×3） | `magic::AREA_RADIUS` | `Map.cs:956-961`（FireBang）、`:1259-1264`（MassHealing） |
| 治疗分段间隔 | 600ms | `magic::HEAL_DELAY_MS` | `HumanObject.cs:186`（`HealDelay = 600`）、`:602-604` |
| 目标搜索半径 | 10 格 | `magic::MAGIC_TARGET_SEARCH_RANGE` | `HumanObject.cs:3468`（`FindObject(targetID, 10)`） |
| 命中容差 | 2 格 | `magic::MAGIC_HIT_TOLERANCE` | `HumanObject.cs:5928`（`InRange(target.CurrentLocation, targetLocation, 2)`） |
| 初始 `CastTime` | `int.MinValue` | `magic::INITIAL_CAST_TIME` | `CharacterInfo.cs:232`（读档强制重置） |
| 一条技能记录 | 14 字节 | `magic::MAGIC_RECORD_LEN` | `MagicInfo.cs:135-144` |
| 技能条数上限 | 64 | `magic::MAX_MAGICS` | **本作扩展**（读档防御；客户端技能栏 24 格） |

本作沿用的既有常量：施法不消耗物品、不查负重、不需要药水；`RegenDelay = 10000`
（`HumanObject.cs:186`、`:3450` 施法会重置 `RegenTime`）。

---

## 3. 数据模型与持久化

### 3.1 `UserMagic`（`crates/mir-server/src/magic.rs`）

```rust
pub struct UserMagic {
    pub spell: Spell,     // Crystal UserMagic.Spell
    pub level: u8,        // Crystal UserMagic.Level（0..=3）
    pub key: u8,          // Crystal UserMagic.Key（0 = 未绑定）
    pub experience: u32,  // Crystal UserMagic.Experience 是 ushort；本作放宽到 u32，下发时截断
}
```

Crystal 还有两个字段没有建模（都没有可观察行为）：

- `IsTempSpell`（GM 临时授予、下线回收，`HumanObject.cs:2270`、`:2279-2293`）：
  本作没有 GM 授予通道；存档里恒写 `false`。
- `CastTime`：Crystal 读档后强制写成 `int.MinValue`（`CharacterInfo.cs:232`），
  于是首次施法永不触发冷却。本作把「上次施法时刻」放在运行时的
  [`MagicState::casts`](../..) 里，存档里恒写 0。

运行期状态（不入档）：

```rust
pub struct MagicState {
    pub spell_time: i64,                 // HumanObject.SpellTime
    pub casts: HashMap<Spell, i64>,      // 每个技能的「上次施法时刻」（UserMagic.CastTime）
    pub heal_amount: u32,                // MapObject.HealAmount
    pub heal_time: i64,                  // HumanObject.HealTime
}
```

### 3.2 存档格式（`CharacterInfo.Save`，`CharacterInfo.cs:458-463`）

```text
i32  count
per magic (14 bytes):
    u8   Spell
    u8   Level
    u8   Key
    u16  Experience
    bool IsTempSpell
    i64  CastTime
```

- 编码：`magic::encode_magics(&[UserMagic]) -> Vec<u8>`
- 解码：`magic::decode_magics(&[u8]) -> Result<Vec<UserMagic>, MagicCodecError>`
  （空块 = 新角色 = 空列表）
- 数据库：`characters.magics BYTEA`（迁移 `crates/mir-store/migrations/0003_character_magics.sql`），
  与背包/装备/仓库同一种「整块二进制」存法。
- 读档差异：Crystal 对**查不到模板**的技能直接丢弃（`CharacterInfo.cs:230`）；
  本作解码器保留原样，只在组装客户端视图（`World::client_magics`）时跳过。
  因此「数据里删掉一个技能模板」不会导致角色存档被改写。

### 3.3 登录/下线

- 登录：`session.rs` 用 `decode_magics(&character.magics)` 填 `Player.magics`；
  `S.UserInformation.Magics` 由 `World::client_magics` 组装（`ClientMagic`，
  对应 `UserMagic.CreateClientMagic`，`MagicInfo.cs:155-178`）。
- 下线：`encode_magics(&player.magics)` 写回 `character.magics`，随角色行整行落盘。

---

## 4. 学技能（用书）

入口：`C.UseItem{UniqueID, Grid}` → `World::use_book(object_id, unique_id)`。
（Crystal 只有一条 `PlayerObject.UseItem` 巨函数，本作只实现了书本分支。）

```text
1. 玩家存在且活着                       // PlayerObject.cs:5820-5824
2. 背包里按 UUID 找到物品                // :5806-5818
3. 物品模板存在                          // :5814
4. 类型必须是 ItemType.Book              // :6077
5. CanUseItem：性别 / 职业 / 等级        // HumanObject.cs:1020-1160
6. Shape → Spell，且 GameData 里查得到    // PlayerObject.cs:6078-6084
7. 不能已经学会（本作扩展，见 §10）       //
8. push UserMagic{level:0, key:0, exp:0} // :6086
9. 发 S.NewMagic（SendMagicInfo，HumanObject.cs:2275-2278）
10. 扣一本（count-- 或清空格）            // :6330-6331
11. 发 S.UseItem{Success=true}           // :6336-6337
```

失败路径（`use_book` 内部已发 `S.UseItem{Success=false}`）：

| 情况 | 额外提示 | 错误 |
|---|---|---|
| 死亡 | — | `MagicError::Dead` |
| 背包里没有这个 UUID | — | `ItemNotFound` |
| 模板缺失 | — | `UnknownItem` |
| 不是书 | — | `NotABook { item_index }` |
| 性别不符 | `S.Chat` 系统 | `WrongGender` |
| 职业不符（`required_class` 位不含本职业） | `S.Chat` 系统 | `WrongClass` |
| 等级不够（`required_type = Level`） | `S.Chat` 系统 | `LowLevel` |
| 等级过高（`required_type = MaxLevel`） | `S.Chat` 系统 | `LowLevel` |
| 模板里没有这个 `Spell` | — | `UnknownSpell` |
| 已学会 | `S.Chat` 系统 | `AlreadyLearned` |

真实数据核对（Jev 样本）：109 本书，`Shape` 就是 `Spell` 值，
`RequiredClass` 是职业位（战士 1 / 法师 2 / 道士 4）、`RequiredType = Level`、
`RequiredAmount` 是最低角色等级，例如 `FireBall` 书 = shape 31 / class 2 / level 7。

---

## 5. 施法流程（`World::cast_magic`）

对应 `HumanObject.Magic`（`HumanObject.cs:3400-3780`）。判定顺序**照抄**：

| # | 检查/动作 | 失败时发包 | Crystal |
|---|---|---|---|
| 1 | 玩家存在、未死亡 | — | `:3402-3406` |
| 2 | `CanCast`：`now >= ActionTime && now >= SpellTime` | `S.UserLocation` | `:152-158`、`:3402-3406` |
| 3 | 已学会（`GetMagic`） | `S.UserLocation` | `:3408-3414` |
| 4 | 模板存在 | `S.UserLocation` | `:3410` |
| 5 | 技能类别支持（本作扩展） | `S.UserLocation` | — |
| 6 | 目标合法（本作扩展，见 §10） | — | `:3460-3474` |
| 7 | 射程：`InRange(自己, 落点, Range)`，`Range == 0` 跳过 | **什么都不发** | `:3416` |
| 8 | 写 `AttackTime = now+600`、`SpellTime = now+1800`、`ActionTime = now+600`、`RegenTime = now+10000`、`Direction = dir` | — | `:3424-3432`、`:3456` |
| 9 | 冷却：`now < CastTime + GetDelay()` | `S.UserLocation` | `:3434-3440` |
| 10 | 蓝量：`cost > MP`（**等于时允许**，MP 变 0） | `S.UserLocation` | `:3442-3448` |
| 11 | `ChangeMP(-cost)` → `S.HealthChanged` | — | `:3451`、`HumanObject.cs:912-926` |
| 12 | 本人 `S.UserLocation` | — | `:3458` |
| 13 | 按 `Spell` 抽伤害/治疗，排进延迟队列 | — | `:3499-3771` |
| 14 | 记 `CastTime = now` | — | `:3773-3776` |
| 15 | 本人 `S.Magic`，周围 `S.ObjectMagic` | — | `:3778-3779` |

要点：

- **第 8 步在冷却/蓝量检查之前**：一次因冷却或缺蓝被拒的施法，仍然锁 600ms 动作 + 1800ms 技能间隔。
- **第 7 步什么都不发**（Crystal 原样行为）。
- 即使本次没有任何目标被命中，第 15 步的 `S.Magic`/`S.ObjectMagic` 也照发（`cast = true`）。
- `Broadcast` 不发给施法者自己（`MapObject.cs:408-420`），所以本人只收 `S.Magic`。

### 5.1 目标规则

| 类别 | 目标 | 说明 |
|---|---|---|
| `Bolt`（FireBall/GreatFireBall/ThunderBolt/SoulFireBall） | 怪物对象 | 必须有目标；距离 ≤ `Range` 且 ≤ 10（`FindObject` 搜索半径）。落点为空时取目标坐标。 |
| `HellFire` | 无（方向 + 自己脚下） | 落点为空时用自己位置 |
| `FireBang` | 落点坐标 | 落点为空 `(0,0)` 时退回自己脚下（Crystal 会照 `(0,0)` 施法） |
| `Heal` | 玩家（自己/他人） | 没给目标时默认自己（`DefaultMagicTarget`，`HumanObject.cs:3399`、`:3506-3512`） |
| `MassHealing` | 落点坐标（3×3） | 同上 |

简化：PK 未实现，**攻击类技能只认怪物**；目标玩家一律 `InvalidTarget`。

---

## 6. 伤害公式（`UserMagic::power` / `damage` / `delay_ms`）

逐字对照 `Server/MirDatabase/MagicInfo.cs:180-222`：

```text
MPower()      = MPowerBonus > 0 ? Random.Next(MPowerBase, MPowerBase + MPowerBonus) : MPowerBase   // :195-203
DefPower()    = PowerBonus  > 0 ? Random.Next(PowerBase,  PowerBase  + PowerBonus)  : PowerBase    // :204-212
GetPower()    = round( (MPower() / 4.0) * (Level + 1) + DefPower() )                               // :190-193
GetMultiplier = MultiplierBase + Level * MultiplierBonus                                            // :185-188
GetDamage(b)  = (int)( (b + GetPower()) * GetMultiplier )                                           // :180-183
GetDelay()    = DelayBase - Level * DelayReduction                                                  // :219-222
cost          = BaseCost + LevelCost * Level                                                        // HumanObject.cs:3377
```

- `Math.Round(float)` 是**银行家舍入**（中点取偶），Rust 用 `f64::round_ties_even()` 对齐；
  内部运算用 `f64`（C# 是 `float`，取值范围内整数结果一致）。
- **抽签顺序**：先抽 `GetAttackPower(MinMC, MaxMC)`（或 `MinSC/MaxSC`），
  再抽 `MPower()`、`DefPower()`。同一 seed 下与 Crystal 的随机序列一致。
- 结算时走怪物 `Attacked(..., DefenceType.MAC)`：
  `combat::resolve_player_attack` → 魔法闪避（`next_bounded(10) < MagicResist`）
  → 魔法防御 `MinMAC..MaxMAC` → `armour >= damage` 记为 Miss → 暴击抽签
  （`MonsterObject.cs:2594-2599`）。
- `LevelMagic` 只在**目标真的掉血**（`> 0`）时调用（`HumanObject.cs:5929`）。

### 6.1 各技能的具体算法

| 技能 | 伤害/治疗基数 | 延迟 | Crystal |
|---|---|---|---|
| `FireBall` / `GreatFireBall` | `GetAttackPower(MinMC, MaxMC)` | `MaxDistance(自己,目标) * 50 + 500` | `HumanObject.cs:3943-3947` |
| `ThunderBolt` | 同上（未做不死系 ×1.5） | 500ms | `:4124-4128` |
| `SoulFireBall` | `GetAttackPower(MinSC, MaxSC)`（未消耗护身符） | `MaxDistance * 50 + 500` | `:4388-4392` |
| `HellFire` | `GetAttackPower(MinMC, MaxMC)` | 500ms 起，每格 +100ms，直线 4 格；3 级同时向左右 45° 各一道 | `:4103-4119` + `Map.cs:875-910` |
| `FireBang` | `GetAttackPower(MinMC, MaxMC)` | 500ms，落点 3×3 | `:4142-4148` + `Map.cs:951-991` |
| `Healing` | `GetDamage(GetAttackPower(MinSC, MaxSC) * 2) + Level` | 500ms | `:4355-4364` |
| `MassHealing` | `GetDamage(GetAttackPower(MinSC, MaxSC))` | 500ms，落点 3×3 | `:4526-4532` + `Map.cs:1255-1296` |

HellFire 的「3 级三向」用 `(Direction + 1) % 8` / `(Direction - 1 + 8) % 8`
（`HumanObject.cs:4110-4119`）。

---

## 7. 治疗生效方式（分段）

Crystal 的治疗术**不是**一次加满：`Healing` 完成后把数值累加进 `target.HealAmount`，
然后由目标的 `ProcessRegen` 每 `HealDelay = 600ms` 放出一段（`HumanObject.cs:602-617`）：

```text
if (now > HealTime) {
    HealTime = now + 600;
    incHeal  = Level / 10 + HealAmount / 10;
    chunk    = 5 + incHeal;
    if (HealAmount > chunk) { healthRegen += chunk; HealAmount -= chunk; }
    else                    { healthRegen += HealAmount; HealAmount = 0; }
}
...
if (healthRegen > 0) { ChangeHP(healthRegen); BroadcastDamageIndicator(Hit, healthRegen); }  // :635-639
if (HP == Stats[HP]) { HealAmount = 0; }                                                    // :641-645
```

本作在 `World::process_player_regen` 里照搬，并把治疗分段与自然回复**相加后一次性** `ChangeHP`，
发出 `S.HealthChanged`（本人）与 `DamageIndicator{Hit, +N}`（本人 + 周围）。
因此「治疗满血后剩余治疗量清零」也被复现。

> 注意：`HealTime` 的判定是**严格大于**（`now > HealTime`），测试里快进 600ms 整不会触发第二段，
> 需要 601ms。这是 Crystal 的行为，不是本作的偏差。

---

## 8. 协议包一览

| 方向 | 包 | 时机 | 字段要点 | Crystal |
|---|---|---|---|---|
| C→S | `C.Magic`（58） | 施法 | `spell, direction, target_id, location, spell_target_lock` | `ClientPackets.Magic` |
| C→S | `C.MagicKey`（57） | 改快捷键 | `spell, key, old_key` | `MirConnection.cs:1543-1566` |
| C→S | `C.SpellToggle`（69） | 开关型技能 | `spell, can_use` | `MirConnection.cs:1650-1661`（本作只记日志，见 §11） |
| C→S | `C.UseItem`（22） | 用书 | `unique_id, grid` | `PlayerObject.UseItem` |
| S→C | `S.Magic`（120） | 施法成功，**只给本人** | `spell, target_id, target(落点), cast, level, secondary_target_ids` | `HumanObject.cs:3778` |
| S→C | `S.ObjectMagic`（123） | 施法成功，**广播给周围玩家**（不含本人） | 多 `object_id/location/direction/self_broadcast` | `:3779` |
| S→C | `S.NewMagic`（117） | 学会技能 | `ClientMagic{...}` + `hero:false` | `HumanObject.cs:2275-2278`、`MagicInfo.cs:146-153` |
| S→C | `S.MagicLeveled`（119） | 每次 `LevelMagic`（角色等级达标时） | `object_id, spell, level, experience(u16)` | `HumanObject.cs:6978` |
| S→C | `S.MagicDelay`（121） | **只在技能升级时**（告诉客户端新冷却） | `object_id, spell, delay` | `HumanObject.cs:6972-6976` |
| S→C | `S.UseItem`（52） | 学书成功/失败 | `unique_id, success, grid` | `PlayerObject.cs:5801`、`:6336` |
| S→C | `S.HealthChanged`（77） | 扣蓝、治疗、自然回复 | `hp, mp` | `HumanObject.cs:870-884` |
| S→C | `S.DamageIndicator`（31） | 命中/闪避/暴击/治疗 | `damage, type_, object_id` | `MapObject.cs:916-926` |
| S→C | `S.AddBuff`（144） | 增益生效/刷新 | `ClientBuff{type, visible, object_id(被增益者), expire_time(剩余 ms), stats, values}` | `HumanObject.cs:7474-7481`、`BuffInfo.cs:256-270` |
| S→C | `S.RemoveBuff`（145） | 增益到期/被移除 | `type_, object_id` | `HumanObject.cs:452-458` |
| S→C | `S.ObjectEffect`（124） | 魔法盾开/关特效 | `object_id, effect(MagicShieldUp/Down), …` | `HumanObject.cs:7466-7470`、`:485-487` |
| S→C | `S.Chat`（30） | 学书失败提示 | `message, type_:System` | `ReceiveChat`（`HumanObject.cs:1047` 等） |

**没有** `S.UserMagic`（任务书里的写法）：Crystal 的实际包名是 `S.NewMagic`；
`mir-protocol` 里 `NewMagic`/`MagicLeveled`/`MagicDelay`/`Magic`/`ObjectMagic`/`UseItem` 全部已存在，
本次**没有改协议 crate**。

登录时的 `S.UserInformation.Magics` 直接复用既有字段（`user_information` 的 `magics`），
以前恒为空，现在按 `Player.magics` 下发。

---

## 9. 支持矩阵

| 技能 | `Spell` 值 | 类别 | 状态 | 备注 |
|---|---|---|---|---|
| FireBall | 31 | 单体 | ✅ | 火球，`MAC` 防御 |
| GreatFireBall | 34 | 单体 | ✅ | 同 FireBall，数值来自模板 |
| ThunderBolt | 36 | 单体 | ✅ | 未做「不死系 ×1.5」（`MonsterKind` 没有 `Undead`） |
| SoulFireBall | 64 | 单体 | ✅ | 未消耗护身符（`HumanObject.cs:4382-4396`） |
| HellFire | 35 | 前方贯穿 | ✅ | 直线 4 格、每格 100ms；3 级三向 |
| FireBang | 38 | 3×3 区域 | ✅ | 落点来自包里的 `location` |
| Healing | 61 | 单体治疗 | ✅ | 分段生效（§7） |
| MassHealing | 75 | 3×3 治疗 | ✅ | 分段生效（§7） |
| ProtectionField | 12 | 增益（护盾） | ✅ 已实现 | `MinAC/MaxAC += round(MaxAC × (0.2 + 0.03L))`，`45 + 15L` 秒（`HumanObject.cs:4955-4963`） |
| Haste | 93 | 增益（攻速） | ✅ 已实现 | `AttackSpeed += 2L + 2`，`25 + 15L` 秒（`:6149-6153`） |
| MagicShield | 43 | 增益（减伤） | ✅ 已实现 | `DamageReductionPercent = 10(L+2)`，每点伤害扣 60ms（`:6198-6204`、`:7165`） |
| SoulShield / BlessedArmour | 69/71 | 增益（防御） | ✅ 已实现 | 落点 7×7、`Level/7 + 4` 的 `MaxMAC`/`MaxAC`（`Map.cs:1046-1097`） |
| EnergyShield | 84 | 增益（受击回血） | ✅ 已实现 | `EnergyShieldPercent = round(100/chance)`、`30 + 50L` 秒（`:4751-4783`） |
| LightBody | 95 | 增益（敏捷） | ✅ 已实现 | `Agility += 2(L+1)`，`30(L+1)` 秒（`:6187-6191`） |
| Fury | 16 | 增益（攻速） | ✅ 已实现 | `AttackSpeed += 4`，`60 + 10L` 秒（`:6160-6164`） |
| MagicBooster | 51 | 增益（魔力） | ✅ 已实现 | `MinMC/MaxMC += 6 + 6L`、耗蓝 +`6 + L`%，60 秒（`:4345-4350`、`:6228-6240`） |
| Rage / ImmortalSkin / UltimateEnhancer / Hiding / PetEnhancer / ElementalBarrier | 13/17/77/67/85/131 | 增益 | ⛔ 登记 | 见 `magic::REGISTERED_UNSUPPORTED`（`docs/world/buffs.md` §6） |
| 其余（FireWall、Lightning、Teleport、Summon*、Poison* 等） | — | — | ❌ 未实现 | 模板能查到，施法返回 `UnsupportedSpell`；若模板缺失则 `UnknownSpell` |

「✅ 已实现」的 9 个增益技能走 `SpellKind::Buff`：施法照常扣蓝、写冷却、发
`S.Magic`/`S.ObjectMagic`，然后按 Crystal 的时机套 buff（`docs/world/buffs.md` 有逐条公式与包序）。

「⛔ 登记」的集合就是 `magic::REGISTERED_UNSUPPORTED`：可以被学会、进技能栏、
`MagicInfo` 查得到，但**施法直接返回 `UnsupportedSpell`，不扣蓝、不进冷却**。

---

## 10. 与 Crystal 的差异（都是有意为之）

1. **重复学同一本书会被拒绝**。Crystal 的 `CanUseItem` 里其实**有**查重
   （`HumanObject.cs:1237-1242` 的 `Info.Magics.Any(t => t.Spell == (Spell)item.Info.Shape)`），
   `UseItem` 在 `:5814` 就用它挡掉了。两边的可观察行为一致（拒绝 + 不消耗），
   只是本作的提示多一句「已经学会」。
2. **攻击类技能只认怪物**：PK/行会战未实现，`IsAttackTarget` 的玩家分支不可达，
   目标玩家返回 `InvalidTarget`。
3. **目标合法性在扣蓝之前严格校验**。Crystal 在扣蓝后才发现目标无效，
   此时仍算一次施法（`cast = true`、`targetID = 0`、无伤害）。
   本作选择「不合法就不扣蓝」，避免玩家被误扣。射程检查仍保持 Crystal 的
   「写在计时器之前、失败静默」。
4. **`C.Magic.SpellTargetLock` 未实现**：落点始终取包里的 `location`；
   `FireBang`/`MassHealing` 等不需要锁定目标的逻辑因此不受影响。
5. **`CanFly` 未实现**：Crystal 会检查目标格是否允许飞行道具（火球/灵魂火球），
   地图层没有这个属性，本作只按距离判定。
6. **不死系加成（ThunderBolt ×1.5）未实现**：`MonsterKind` 没有 `Undead` 字段
   （`mir-db` 的 `MonsterInfo.undead` 尚未桥接）。
7. **护身符/毒药消耗未实现**：`SoulFireBall` 不消耗护身符（Crystal 需要 1 个，
   `HumanObject.cs:4382-4396`）；本作还没有消耗品系统。
8. **`Stats[SkillGainMultiplier]` 固定为 1**：Crystal 会乘这个属性
   （`HumanObject.cs:6924`），本作没有该属性。
9. **`RefreshSkills` 未移植**（`HumanObject.cs:2294-`）：它按装备/等级调整
   战士技能等级，涉及的都是未实现的技能。
10. **`FillMagicInfoList` 未移植**（`Envir.cs:293-`）：Crystal 在数据库缺条目时补内置模板；
    本作只认数据库里的 111 条（缺模板 = 学不会/施法失败）。
11. **`SpellToggle` 只记日志**：`Thrusting`/`HalfMoon`/`FlamingSword` 等开关型技能都没实现效果，
    没有状态可翻。
12. **`MagicDelay` 的发送时机**：见 §8，只有升级时才发。任务书里「每次施法发 `S.MagicDelay`」
    与 Crystal 不符，本作以 Crystal 为准。
13. **失败提示是硬编码中文**（`World` 层没有语言目录）。客户端语言包/服务端 i18n 键位接入后再替换。
14. **治疗类的射程按目标实际位置判定**。Crystal 只检查包里的 `location`
    （`HumanObject.cs:3416`），目标对象本身不在射程检查内；本作对 `Healing` 直接取目标坐标判距离，
    更严格也更符合直觉。
15. **区域技能回显的 `target_id` 恒为 0**。Crystal 会把包里的 `TargetID` 原样回显
    （`HumanObject.cs:3778`），但 `SpellTargetLock` 未实现，落点一律取包里的 `location`，
    回显一个不参与结算的目标 ID 只会让客户端困惑。

---

## 11. 未做（TODO）

- 开关型技能（`SpellToggle`）：需要玩家级开关状态与攻击分支配合（`Thrusting`、`HalfMoon`、
  `FlamingSword`、`TwinDrakeBlade`…）。
- **仍登记但无效果的增益技能**：`Rage`、`ImmortalSkin`、`UltimateEnhancer`、`Hiding`、
  `PetEnhancer`、`ElementalBarrier`（都是 `AddBuff`，只是效果需要额外系统：狂暴/金刚不坏的
  百分比换算是纯数值、隐身要改视野、神兽强化要宠物）。
- 增益的护身符消耗：`SoulShield`/`BlessedArmour` 在 Crystal 里要 1 个护身符
  （`HumanObject.cs:4515`），本作还没把护身符接到技能上。
- 位移/传送类：`Teleport`、`Blink`、`ShoulderDash`、`BackStep`、`StormEscape`。
- 召唤/宠物：`SummonSkeleton`、`SummonShinsu`、`SummonHolyDeva` 等。
- 持续伤害/中毒：`Poisoning`、`PoisonCloud`、`FrostCrunch` 的减速/冰冻。
- 地图级持续技能对象（`SpellObject`）：`FireWall`、`ThunderStorm`、`TrapHexagon` 等。
- 英雄（`HeroObject`）技能：`S.HeroInformation.magics` 仍恒为空。
- 技能冷却的客户端下发：本作发 `ClientMagic.CastTime = 0`（Crystal 发的是
  `int.MinValue - Envir.Time` 的巨负值，语义等价）。

---

## 12. 测试清单

`crates/mir-server/tests/magic.rs`（21 个）覆盖学书、施法校验、伤害/治疗结算与持久化；
增益技能（9 个）与药水/卷轴的用例在 `crates/mir-server/tests/use_items.rs`
（18 个，见 `docs/world/buffs.md` §7 与 `docs/world/items.md` §14）。

| # | 用例 | 断言要点 |
|---|---|---|
| 1 | `book_learning_adds_magic_sends_new_magic_and_consumes_book` | 学会、0 级、消耗书、`S.NewMagic`、`S.UseItem{true}` |
| 2 | `learning_the_same_book_twice_is_rejected` | `AlreadyLearned`、失败包、聊天、书不被消耗 |
| 3 | `book_class_requirement_is_enforced` | `WrongClass` + 提示 |
| 4 | `book_level_requirement_is_enforced` | `LowLevel { required:10, level:5 }` + 提示 |
| 5 | `using_a_non_book_item_fails_without_learning` | `NotABook`、不学会 |
| 6 | `casting_without_mana_is_rejected_and_spends_nothing` | `NotEnoughMana`、不扣蓝、不排伤害 |
| 7 | `casting_before_action_time_ends_is_rejected` | `Busy`（`SpellTime` 未到） |
| 8 | `casting_during_cooldown_is_rejected` | `CoolingDown`（3 秒冷却夹具），之后允许 |
| 9 | `casting_out_of_range_is_rejected` | `OutOfRange { distance:3, range:2 }`、**不发包** |
| 10 | `fireball_damages_monster_through_magic_defence_after_delay` | `S.Magic` 给本人、`S.ObjectMagic` 给旁观者、600ms 延迟、MAC 减伤、扣蓝 |
| 11 | `magic_resist_causes_the_bolt_to_miss` | 魔法闪避 → Miss 指示、不掉血 |
| 12 | `heal_restores_hp_in_chunks_with_health_changed` | 500ms 延迟、`HealthChanged`、+7 分段、满血清零 |
| 13 | `mass_healing_heals_everyone_in_the_area` | 3×3 内两人都回血、区域外不动 |
| 14 | `killing_a_monster_grants_magic_experience` | 击杀 → `MagicLeveled` 带经验 |
| 15 | `magic_level_up_sends_magic_leveled_and_magic_delay` | 10 次施法升 1 级 + `MagicDelay=1700` |
| 16 | `hellfire_burns_a_line_of_cells` | 4 格逐格 100ms 推进 |
| 17 | `firebang_hits_the_whole_3x3_area` | 3 个目标全中、区域外不掉血 |
| 18 | `unsupported_spells_are_registered_but_apply_nothing` | `Rage`（仍在登记表里）→ `UnsupportedSpell`、不扣蓝、技能栏可见 |
| 19 | `magic_key_rebinding_clears_the_previous_owner` | 同键互斥、未学不能绑 |
| 20 | `magic_blob_roundtrips_through_crystal_layout` | 编解码往返、空块 |
| 21 | `real_database_exposes_magic_infos` | 真实数据：≥100 条、FireBall 的 delay/power/range 非 0、书→`Spell` 可映射 |

模块内单元测试（`src/magic.rs`）：记录布局、空块/截断/超限、`cost`/`delay`/`multiplier`、
威力与银行家舍入、`ClientMagic` 截断、`spell_kind` 覆盖（9 个增益技能都是 `SpellKind::Buff`、
`REGISTERED_UNSUPPORTED` 里的仍是 `Unsupported`）。

---

## 13. 增益技能的施法副作用（`World::cast_buff_spell`）

9 个增益技能共用 `SpellKind::Buff` 分支：扣蓝/冷却/`S.Magic` 与伤害类完全一样，
只是效果换成了 `add_buff` 或延迟队列里的 `MagicEffect::Buff`/`AreaBuff`。
逐条公式、生效时机、`S.AddBuff` 包序都在 `docs/world/buffs.md` §4，这里只列要点：

* **立刻生效**：`ProtectionField`、`EnergyShield`（Crystal 在 `Magic` 的 `switch` 里直接调用）；
* **延迟 500ms**：`Haste`、`LightBody`、`Fury`、`MagicShield`、`MagicBooster`
  （`DelayedAction(DelayedType.Magic, Envir.Time + 500, magic)`）；
* **延迟「距离 × 50 + 500」ms、作用 7×7**：`SoulShield`、`BlessedArmour`
  （`Map.cs:1046-1097`，加成按**受术者**等级 `Level/7 + 4`）；
* 数值（时长/加成）在**施法瞬间**抽签（`GetPower`/`GetAttackPower`），与 Crystal 一致；
* `MagicShield` 重复施放不会刷新（`HumanObject.cs:6200` 的 `if (HasBuff(...)) return;`），
  但**蓝照扣**（判定发生在 `CompleteMagic`，此时已经扣过蓝、发过 `S.Magic`）。

