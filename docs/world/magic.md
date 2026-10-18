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
> 测试：`crates/mir-server/tests/magic.rs`（21 个确定性用例）+ `crates/mir-server/tests/skills.rs`
> （第 2 轮 7 个新技能，20 个用例）+ `magic.rs` 模块内单元测试。

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
- 本作已实现 **15 个伤害/治疗/位移/召唤/中毒技能**与 **9 个增益技能**：
  - 伤害/治疗：`FireBall`、`GreatFireBall`、`ThunderBolt`（含不死系 ×1.5）、`SoulFireBall`、
    `HellFire`、`FireBang`、`Healing`、`MassHealing`；
  - 第 2 轮新增：`FireWall`（持续伤害场）、`Lightning`（前方 6 格贯穿）、`Teleport`（瞬息移动）、
    `Hiding`（隐身）、`SummonSkeleton` / `SummonShinsu`（召唤）、`Poisoning`（施毒）；
  - 增益 9 个见 §13 与 `docs/world/buffs.md`；
  其余「登记但无效果」的原因见 §9、§11。

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
| 增益生效延迟 | 500ms | 字面量 `500`（`cast_buff_spell`） | `HumanObject.cs:3544-3546`、`:3590-3591`、`:3632-3633` |
| 盾类区域半径 | 3（7×7） | `buffs::SHIELD_AREA_RADIUS` | `Map.cs:1052-1057` |
| 火墙：格数 | 5（中心 + 上/右/下/左） | `magic::FIREWALL_CELLS` | `Map.cs:1145-1149` |
| 火墙：结算间隔 | 2000ms | `magic::FIREWALL_TICK_MS` | `Map.cs:1134`（`TickSpeed = 2000`） |
| 火墙：基础持久 | 10 秒 + 伤害/2 秒 | `magic::FIREWALL_BASE_SECONDS` | `Map.cs:1133`、`:1172`（`ExpireTime = now + (10 + value / 2) * 1000`） |
| 雷电：射程 | 6 格 | `magic::LIGHTNING_RANGE` | `Map.cs:1194` |
| 雷电：延迟 | 500ms | `magic::LIGHTNING_DELAY_MS` | `HumanObject.cs:4213` |
| 瞬息移动：延迟 | 200ms | `magic::TELEPORT_DELAY_MS` | `HumanObject.cs:3539` |
| 瞬息移动：尝试次数 | 200 | `magic::TELEPORT_ATTEMPTS` | `PlayerObject.cs:4639` |
| 瞬息移动：法力惩罚 | 30 秒 / +30% | `magic::TELEPORT_PENALTY_MS` / `TELEPORT_MANA_PENALTY_PERCENT` | `HumanObject.cs:6106`、`:3379-3385` |
| 隐身：延迟 | 500ms | `magic::HIDING_DELAY_MS` | `HumanObject.cs:4496` |
| 召唤：延迟 | 500ms | `magic::SUMMON_DELAY_MS` | `HumanObject.cs:4437`、`:4486` |
| 召唤：共享上限 | 2 只 | `magic::MAX_MONSTER_PETS` | `HumanObject.cs:4417`、`:4466` |
| 施毒：延迟 / 每跳间隔 | 500ms / 2000ms | `magic::POISON_DELAY_MS` / `POISON_TICK_MS` | `HumanObject.cs:4372-4374`、`:6058` |
| 不死系伤害倍率 | ×1.5 | `magic::UNDEAD_DAMAGE_PERCENT` | `HumanObject.cs:4126` |

本作沿用的既有常量：施法不消耗物品、不查负重（护身符消耗仍未接，见 §11）；
`RegenDelay = 10000`（`HumanObject.cs:186`、`:3450` 施法会重置 `RegenTime`）。

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
    // 第 2 轮新增（都放在 MagicState，避免给 Player 加字段——17 个测试与 session 直接构造 Player）：
    pub hidden: bool,                    // MapObject.Hidden（BuffType.Hiding）
    pub hidden_until: i64,               // Hiding buff 到期时刻
    pub teleport_penalty_until: i64,     // BuffType.TemporalFlux（+30% 耗蓝）
    pub poisons: Vec<PoisonState>,       // HumanObject.PoisonList
    pub last_target: Option<u32>,        // 本作扩展：召唤物优先打主人的目标
}
```

毒与法术场也各有一个运行期结构（都不入档）：

```rust
pub struct PoisonState {           // Crystal Poison（MapObject.cs:74）
    pub kind: PoisonType,          // 本作只做 Green / Red
    pub value: i32,                // 绿毒每跳伤害（红毒为 0）
    pub duration: u32,             // 总跳数（不是毫秒）
    pub ticks: u32,                // 已经跳过的次数
    pub tick_at: i64,              // 下一次结算的绝对时刻
    pub owner_id: u32,             // 施毒者；下线后整条毒移除
}

pub struct SpellField {            // Crystal SpellObject（Server/MirObjects/SpellObject.cs）
    pub object_id: u32,            // S.ObjectSpell / S.ObjectRemove 的对象 ID
    pub map: String,
    pub center: Point,
    pub radius: i32,               // 火墙为 0（只烧同格）
    pub damage: i32,               // SpellObject.Value
    pub caster_id: u32,
    pub expire_at: i64,
    pub next_tick: i64,
    pub tick_ms: i64,              // 2000
    pub spell: Spell,              // 目前只有 FireWall
    pub direction: MirDirection,
    pub cast_id: u32,              // CastInstanceId（FireWallLimit 按实例回收）
}
```

> 本作**没有** Crystal 的 `ObjectType.Spell` 对象层：火墙用「中心 + 半径 + 到期 + 结算间隔」
> 的 `SpellField` 表示，每 tick 用 AOI 查同格对象结算（`World::tick_spell_fields`），
> 客户端看到的是 `S.ObjectSpell` / `S.ObjectRemove`。

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
| 6 | 目标合法（本作扩展，见 §5.1、§10） | — | `:3460-3474` |
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
| `Bolt`（FireBall/GreatFireBall/ThunderBolt/SoulFireBall） | 怪物对象 | 必须有目标；距离 ≤ `Range` 且 ≤ 10（`FindObject` 搜索半径）。落点为空时取目标坐标。**不含玩家**（`attack_target_position` 只查怪物）。 |
| `HellFire` | 无（方向 + 自己脚下） | 落点为空时用自己位置 |
| `FireBang` | 落点坐标 | 落点为空 `(0,0)` 时退回自己脚下（Crystal 会照 `(0,0)` 施法） |
| `Heal` | 玩家（自己/他人） | 没给目标时默认自己（`DefaultMagicTarget`，`HumanObject.cs:3399`、`:3506-3512`） |
| `MassHealing` | 落点坐标（3×3） | 同上 |
| `FireWall` | 落点坐标 | 落点为空时退回自己脚下；`target_id` 恒 0（`:3565-3566`）；**伤害结算可命中玩家**（PK 规则，施法者除外） |
| `Lightning` | 无（方向） | 只用到方向与施法者位置（`:3568-3569`、`:4209-4215`）；**逐格结算可命中玩家**（PK 规则） |
| `Teleport` | 无（落点只做射程预检） | `info.range != 0` 时仍按 `HumanObject.cs:3416` 判一次 |
| `Hiding` / `Summon*` | 自己 | `target_id` 统一覆盖成施法者对象 ID |
| `Poisoning` | **怪物或按 PK 规则可攻击的玩家** | 走 `magic_attack_position`：先找怪物，再查「同图存活 + `is_attack_target_player`」的玩家（Crystal `IsAttackTarget`，`HumanObject.cs:4367`）；距离 ≤ 10 |
| `Buff` | 自己（`EnergyShield`/`SoulShield`/`BlessedArmour` 可给同图存活玩家） | 其余 6 个增益无视目标、作用在自己身上 |

目标规则与 PK 一致（`is_attack_target_player`）：

* **`Bolt` 类只选怪物**（目标玩家返回 `InvalidTarget`）；
* **`Poisoning` 可对玩家施毒**（与物理攻击共用 PK 规则；玩家毒走 `S.Poisoned` 包）；
* **`FireWall` 的中心格与 `Lightning` 的逐格结算**会命中玩家（同样按 PK 规则，施法者自己除外），
  见 §6.2、§6.3。

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
| `ThunderBolt` | 同上；**不死系 ×1.5**（`UNDEAD_DAMAGE_PERCENT`） | 500ms | `:4124-4128` |
| `SoulFireBall` | `GetAttackPower(MinSC, MaxSC)`（未消耗护身符） | `MaxDistance * 50 + 500` | `:4388-4392` |
| `HellFire` | `GetAttackPower(MinMC, MaxMC)` | 500ms 起，每格 +100ms，直线 4 格；3 级同时向左右 45° 各一道 | `:4103-4119` + `Map.cs:875-910` |
| `FireBang` | `GetAttackPower(MinMC, MaxMC)` | 500ms，落点 3×3 | `:4142-4148` + `Map.cs:951-991` |
| `Healing` | `GetDamage(GetAttackPower(MinSC, MaxSC) * 2) + Level` | 500ms | `:4355-4364` |
| `MassHealing` | `GetDamage(GetAttackPower(MinSC, MaxSC))` | 500ms，落点 3×3 | `:4526-4532` + `Map.cs:1255-1296` |
| `FireWall` | `GetDamage(GetAttackPower(MinMC, MaxMC))` | 500ms 落地，5 格持续场；每 2000ms 结算一次，持续 `10 + 伤害/2` 秒 | `:4149-4182` + `Map.cs:1101-1183` + `SpellObject.cs:128-135` |
| `Lightning` | `GetDamage(GetAttackPower(MinMC, MaxMC))` | 500ms，前方 6 格逐格打第一个目标 | `:4209-4215` + `Map.cs:1187-1218` |
| `Teleport` | —（位移） | 200ms 后随机传送；30 秒 +30% 施法耗蓝 | `:3537-3539` + `:6095-6109` + `PlayerObject.cs:4633-4648` |
| `Hiding` | 时长 = `GetAttackPower(MinSC, MaxSC) + (Level + 1) * 5` **秒** | 500ms | `:4489-4498` + `:6133-6145` |
| `SummonSkeleton` | 召唤「骷髅」（`BoneFamiliar` 模板，缺失时回退）、1 张护身符（未接） | 500ms | `:4399-4439` + `Map.cs:914-933` |
| `SummonShinsu` | 召唤「神兽」（`Shinsu` 模板，缺失时回退）、5 张护身符（未接） | 500ms | `:4448-4488` + `Map.cs:914-933` |
| `Poisoning` | `GetDamage(GetAttackPower(MinSC, MaxSC))`，绿毒伤害 = `power / 15 + Level + 1` | 500ms 落地，每 2000ms 一跳 | `:4365-4378` + `:6041-6077` |

HellFire 的「3 级三向」用 `(Direction + 1) % 8` / `(Direction - 1 + 8) % 8`
（`HumanObject.cs:4110-4119`）。

### 6.2 火墙（`FireWall`）

**施法瞬间**（扣蓝之后、排延迟动作之前）先做地图上限检查
（`World::can_cast_fire_wall`，Crystal `HumanObject.cs:4151-4175`）：
`MapInfo.FireWallLimit && FireWallCount > 0` 时统计本玩家在该图的火墙施法实例
（`cast_id`，去重）；达到上限就回收**最老的一次施法**
（`TryReplaceOldestFireWallCast`，`HumanObject.cs:4183-4207`），回收失败回聊天
「火墙数量已达上限，请稍后再试。」并且**不排延迟动作**（火墙不落地）。
注意 Crystal 此时 `cast` 仍为 `true`：蓝**照扣**、`S.Magic`/`S.ObjectMagic` 照发、
`CastTime` 照记——本作与之一致。

500ms 后落地（`World::create_fire_wall`）：

1. **铺 5 格**：中心格是落点，其余 4 格按 `Up → Right → Down → Left`
   各偏 1 格（`Map.cs:1145-1149`）；**越界或不可走**（`walkable_on`，对应 Crystal
   `Map.cs:637-640` 的 `Cell.Valid`）以及已被火墙占用的格跳过；
2. **持久**：`ExpireTime = now + (10 + 伤害/2) * 1000`（`Map.cs:1133`、`:1172`）；
3. **每 tick**（2000ms，首跳立即，`SpellObject.cs:111-112`）：同格内玩家（非施法者、
   可攻击目标）与怪物（非召唤物）各吃一次 `DefenceType.MAC` 伤害
   （`SpellObject.cs:128-135`），并 `BroadcastDamageIndicator`；
4. **移除**：到期或施法者下线/换图 → 每格广播 `S.ObjectRemove`（`SpellObject.cs:57`、
   `:82-90`）。

包序：

```text
S.Magic（本人）+ S.ObjectMagic（周围）        // 施法回显（cast=true）
[500ms] S.ObjectSpell × 5（每格一条）        // Spell=FireWall, ObjectID=新分配, Direction=Up
[每 2000ms] S.DamageIndicator / S.Struck     // 命中反馈
[到期] S.ObjectRemove × 5
```

Crystal 的 `FireWallLimit` 是**地图**字段（`MapInfo`），本作同样读 `MapInfo.firewall_limit` /
`firewall_count`；`FireWallCount == 0` 表示不限。

### 6.3 雷电（`Lightning`）

结算 `World::resolve_lightning`（Crystal `Map.cs:1187-1218`）：

* 从**施法者位置**沿施法方向推进最多 6 格（`LIGHTNING_RANGE`）；
* 每格按 `(y, x, object_id)` 顺序找**第一个**可攻击对象（怪物跳过守卫；
  玩家要求 `is_attack_target_player`），命中后**内层 `break`**，继续下一格；
* 不可走格/越界只是 `continue`（`walkable_on`；Crystal 是 `ValidPoint`，
  `Map.cs:1194-1198`），**没有「碰到墙就停」**；
* 全部走 `DefenceType.MAC`；任一目标掉血就 `LevelMagic`。

**表现**：Crystal 没有 `Lightning` 的 `SpellEffect` 变体，命中反馈靠
`S.ObjectMagic` + `S.ObjectStruck`（受击动作）/伤害飘字；本作同样不加额外特效包，
客户端 `spellfx` 把 `S.Magic`/`S.ObjectMagic` 的落点当锚点，画一条**施法者 → 落点**
的线（不按服务端的 6 格方向逐格重放；落点由客户端发包时的 `location` 决定）。

### 6.4 瞬息移动（`Teleport`）

`World::teleport_caster`（Crystal `PlayerObject.MagicTeleport`，`PlayerObject.cs:4633-4648`）：

1. `MapInfo.NoTeleport` → 回聊天「这张地图不能使用瞬息移动。」（`:6097-6101`）；
2. 在**矩形半径**内随机取点：`size_x = Width / (Level + 1)`、
   `size_y = Height / (Level + 1)`，即等级越高范围越小；
3. 最多 **200 次**尝试，只要求 `walkable_on`（Crystal `Teleport` 只校验可走点）；
4. 成功：广播 `S.ObjectTeleportOut` → 移动 → `S.ObjectTeleportIn`；
5. 加 30 秒 **TemporalFlux**：之后每次 `Teleport` 的耗蓝 +30%
   （`:6106` + `HumanObject.cs:3379-3385`；由 `MagicState::teleport_penalty_until` 记时）。

> **与 Crystal 的差异**：Crystal 以**回城点**（`BindMapIndex` + `BindLocation`）为中心，
> 回城点会在进安全区/复活/回城卷时更新（`:1064-1068`、`:4279-4280`、`:4589-4590`）；
> 本作 `Player` 还没有建模 bind 点（`characters.bind_*` 列存在），
> 改用**当前地图 + 当前坐标**为中心。这是行为差异，不是等价实现。

### 6.5 隐身（`Hiding`）

施法（`HumanObject.cs:4489-4498`）：500ms 后写入 `MagicState::hidden`，时长
`GetAttackPower(MinSC, MaxSC) + (Level + 1) * 5` **秒**。

包序（`World::start_hiding`）：

```text
S.ObjectHidden{ObjectID, Hidden=true}     // 广播给周围（不含自己）
S.AddBuff{BuffType.Hiding, Visible=false} // 只发本人
（HideFromTargets）                       // 范围内锁定自己的怪脱战
```

解除：

* **走路/跑步**（`HumanObject.cs:2460-2463`、`:2540-2545`）——与 Crystal 一致；
* **到期**（`BuffType.Hiding` 到期 → `Hidden = false`）；
* **死亡**（`world.rs` 的死亡清理把 `magic.hidden` 置假）。

攻击 / 施法 / 受击**不解除**隐身——Crystal 只在这几处解除 MoonLight/DarkBody
（`:2869-2886`、`:3418-3422`、`:7138-7142`、`:7256-7260`、`:7320-7324`），
本作与 Crystal 一致（任务书里的「移动/攻击解除」是概述，以源码行为为准）。

两个细节：

* **破隐怪**：带 `CoolEye` 且等级 ≥ 玩家等级的怪仍能看见隐身玩家
  （`MonsterObject.cs:1974`；`hiding_is_seen_by_cool_eye_monster_of_equal_level`）；
* **重复施放**只刷新时长（`hidden_until`），不会重复广播 `S.ObjectHidden`/`S.AddBuff`
  （`recasting_hiding_while_hidden_only_refreshes_duration`）。

状态只放在 `MagicState`（`hidden`/`hidden_until`），**不入存档**；施法本身照常
`cast = true`、扣蓝、进冷却。

Crystal 的隐身消耗 **1 张护身符**（`GetAmulet(1)`），本作消耗品系统未接，**不消耗**。

### 6.6 召唤（`SummonSkeleton` / `SummonShinsu`）

`World::summon_pet`（Crystal `HumanObject.cs:4399-4488`、`Map.cs:914-933`）：

1. `MapInfo.NoPets` → 回聊天「这张地图不能召唤宠物。」，召唤失败；
   与 Crystal 一致：`NoPets` 检查在 `SummonSkeleton` 内（`:4401-4405`），
   此时蓝已扣、`S.Magic` 已发（`summoning_on_a_no_pets_map_is_rejected_after_spending_mana`）；
2. **同名活宠**已在场 → 只「召回」（传送到主人身后一格），不再召唤（`:4408-4415`）；
3. 怪物类召唤物**共享上限 2 只**（`MAX_MONSTER_PETS`，`:4417`、`:4466`），
   达到上限直接返回（Crystal 无提示）；
4. 模板：`SummonSkeleton` → `BoneFamiliar`、`SummonShinsu` → `Shinsu`
   （Crystal 用 `Settings.SkeletonName` / `ShinsuName`）；数据库里没有时用
   `MonsterKind::bone_familiar()` / `MonsterKind::shinsu()` 兜底；
5. 落点：主人面前一格，不可走则落在主人脚下（`Map.cs:925-932`）；
6. 属性（`MonsterObject.RefreshAll`，`MonsterObject.cs:811-827`）：
   `HP + 20L`、`AC/MAC + 2L`、`DC + L`，移动/攻击间隔减 `MaxPetLevel × 130 / 70`
   （下限 400ms）；`PetLevel = 技能等级`、`MaxPetLevel = 4 + L`（骷髅）/ `1 + 2L`（神兽）；
7. 主人死亡 → 全部召唤物一起死；主人下线 → 全部移除；
   召唤物远离/跨图 → 自动召回（`MonsterObject.cs:1334-1375`）；
8. 召唤物**击杀**怪物时经验归主人（`MonsterObject.cs:952-958` 的 `GetAttacker`
   语义，`expo_owner = master`）；召唤物自己死亡**不给**主人经验
   （`MonsterObject.cs:982` 只在 `Master == null` 时 `WinExp`）。

> **简化**：护身符消耗未接（Crystal 骷髅 1 张、神兽 5 张）；
> 不做 `PetSave`（下线保留宠物）、不做 `PetExp`（宠物经验，Crystal 有 ×3 设定）。
> 召唤物优先打主人最近攻击的目标（`MagicState::last_target`，本作扩展；
> Crystal 由召唤物自己 `FindTarget`，`MonsterObject.cs:1918-1997`）。
>
> **野生怪反击召唤物（第 4 轮已实现，对齐 Crystal）**：
> 野生怪在 `World::find_target`（`monster.rs:1037`）里**主动**把宠物当候选
> （AOI `AoiKind::Monster` + `master.is_some()` 分支，`:1100-1110`；Crystal `FindTarget` 的 Monster
> 候选 `MonsterObject.cs:1940-1963`）；宠物被咬时 `World::damage_monster`（`world.rs:2442`）
> 再把 `pet_target = Some(attacker)` 并清 `operate_time`（`:2498`），下一 tick 由
> `process_target`（`monster.rs:763`）走近、`attack_monster`（`:982`）反击
> （Crystal `Attacked` → `Target = attacker`，`:2601-2610`）。
>
> 限制/差异（照 Crystal）：
> * **玩家也能打宠物、宠物会反击**：`World::damage_monster`（`world.rs:2442`）的
>   `pet_target` 写回不区分攻击者类型，是否合法交给 `pet_target_valid`（`monster.rs:901`：
>   主人 Peace/安全区/同组同会/主人自己都会拒绝；Crystal `Attacked(HumanObject)` 只在
>   `Target == null` 时设，`:2571-2574`）；和平模式主人的宠物不反击玩家；
> * **玩家打宠物受主人攻击模式限制**：`monster_is_attack_target_of_player`
>   （`world.rs:1982`）接入了 `melee_target` 与 `attack_target_position`（Bolt）/
>   `magic_attack_position`（Poisoning）：守卫不可打；宠物受主人 `AttackMode`/安全区/
>   同组同会/红棕名限制，**主人只有 `All` 模式才能打自己的宠物**，Peace 玩家不能打宠物
>   （Crystal `MonsterObject.cs:2430-2463`）；
> * 守卫 AI 6/113 只攻击**红名主人**（`PKPoints >= 200`）的宠物（Crystal `:2471-2475`）；
>   Tao Guard AI 58 不攻击**和平模式主人**的宠物（`:2476-2479`）；攻城箭塔 AI 80 不把宠物当目标；
> * 其余野生怪对**任何**宠物都可攻击，包括和平模式主人的宠物（Crystal `:2481-2484` 无条件 true）；
> * **野生怪目标覆盖**：非宠物目标被**怪物**攻击时无条件 `target = attacker`
>   （`world.rs:2502-2505` 注释；Crystal `:2601-2610`）；玩家攻击仍只在 `target` 为空时设置；
> * 宠物永不攻击自己的主人（`pet_target_valid` 显式排除）；没有「互相锁死」定时器：双方
>   锁定后只追击/攻击，目标死亡或出视野（宠物 8 格、野怪 `VIEW_RANGE` 16）即解除；
>   宠物死亡 `monster_died` 注销、主人死亡 `player_died` 清宠物；
> * **有意差异：宠物之间不互相攻击**——`find_pet_target` 只收 `master.is_none()` 的野生怪
>   （`monster.rs:965`），`pet_target_valid` 对 `target.master.is_some()` 直接返回 false
>   （`:932`）。Crystal 允许敌对主人的宠物互攻（`FindTarget` 的 Pet Vs Pet 分支，
>   `MonsterObject.cs:1978-1988`），本作按任务要求不实现，**不算已对齐**。
>
> **宠物命令（第 5 轮）**：客户端 `V` 键循环发 `C.ChangePMode`(45)（`P` 已被 Crystal 的
> 组队窗口占用）。`mir_core::enums::PetMode` 五态（wire 值）：
>
> | 模式 | 值 | 语义 |
> |---|---|---|
> | `Both` | 0 | 跟随 + 攻击（默认） |
> | `MoveOnly` | 1 | 只跟随，不攻击（清目标） |
> | `AttackOnly` | 2 | 只攻击，不跟随/不移动 |
> | `None` | 3 | 休息：不索敌、不跟随 |
> | `FocusMasterTarget` | 4 | 跟随，且只打主人当前目标 |
>
> 服务端：`World::set_pet_mode`（`world.rs:3565`）/ `pet_mode`（`:3575`）写**旁置表**
> `pet_modes`；`spawn_pet`、同名召回 `recall_pet`、宠物死亡（`monster.rs:1558-1561` 移除该
> 主人的模式、即回到默认 `Both`）、`kill_pets`/`despawn_pets`/`leave` 都会清理或重置。AI 在
> `monster.rs:832 process_pet_ai`：`None` 直接休息（`:855`）、`MoveOnly` 清目标（`:873`）、
> `AttackOnly` 不移动（`:902`）、仅 `Both/MoveOnly/Focus` 跟随（`:908-916`）；
> 服务端收到 `C.ChangePMode` 后写模式并无条件回 `S.ChangePMode`(63)
> （Crystal `MirConnection.cs:1441-1448`）。
>
> **存档差异**：`pet_modes` 是 **World 侧表、不随角色存档**；Crystal 的
> `CharacterInfo.PMode` 会落盘（重登后模式保留）。本作重登/召回后回到 `Both`。
> 客户端只暴露「跟随 / 攻击 / 休息」三态循环（`V` 键），`MoveOnly` 与
> `FocusMasterTarget` 服务端支持但暂无客户端入口。
>
> 测试：`tests/skills.rs` 的 `move_only_pet_follows_without_attacking`、
> `both_mode_pet_attacks_nearest_wild`、`attack_only_pet_locks_the_masters_target`、
> `rest_mode_pet_does_not_search_or_follow`、`pets_do_not_attack_each_other`、
> `pet_mode_resets_on_recall_and_pet_death`。
>
> 测试：`tests/skills.rs` 的 `wild_monster_targets_and_attacks_a_pet_then_pet_retaliates`、
> `pet_death_to_a_wild_monster_does_not_panic_the_master`、`guards_ignore_pets`、
> `peaceful_masters_pet_does_not_attack_players`、`pet_never_targets_its_own_master`、
> `pet_retaliates_when_attacked_by_a_player`、`peaceful_master_pet_does_not_retaliate_against_players`、
> `players_cannot_attack_pets_without_all_mode`、`wild_monster_switches_target_to_the_pet_that_bites_it`。

### 6.7 施毒（`Poisoning`）

施法（`HumanObject.cs:4365-4378`）：`power = GetDamage(GetAttackPower(MinSC, MaxSC))`，
500ms 后落地（`:6041-6077`）。

**目标**：走 `magic_attack_position`，可对**怪物**或**按 PK 规则可攻击的玩家**施毒
（`is_attack_target_player`，与物理攻击同一套规则；玩家中招走 `S.Poisoned`，
周围走 `S.ObjectPoisoned`）。

**毒型选择**：Crystal 由装备栏毒药的 `Shape` 决定（1 绿 / 2 红，`:6050-6070`）；
本作没有毒药消耗品，改为确定性策略：

* 目标**没有绿毒** → 施**绿毒**；
* 目标**已经有绿毒** → 施**红毒**。

红毒不扣血，效果是**降防御**：Crystal 每中一次红毒 `ArmourRate -= 0.10F`（玩家，
`HumanObject.cs:714-715`）/ `-= 0.5F`（怪物，`MonsterObject.cs:1536-1537`），结算时
`armour * ArmourRate`（`HumanObject.cs:7241`）。本作在 `World::player_armour_rate` /
`Monster::armour_rate` 里按红毒条数累减，并在生成战斗属性快照时缩放四项防御
（`world.rs` 的 `scale_armour`）。

数值（绿毒，逐字照抄）：

```text
Duration = power * 2 + (Level + 1) * 7      // 跳数（不是秒）
TickSpeed = 2000ms
Value     = power / 15 + Level + 1          // 每跳伤害（Crystal 还要 + Random.Next(PoisonAttack)，
                                            // 本作没有 Stat.PoisonAttack，省略）
```

每 2 秒结算：

* 玩家：**最后一跳仍然结算**（先扣血再移除，`HumanObject.cs:668-693`）；
* 怪物：`Time++` 后 `Time >= Duration` 先移除再 `continue`，**最后一跳不结算**
  （`MonsterObject.cs:1476-1491`）；
* 同类毒覆盖规则：绿毒只接受更高 `value`、红毒只接受更长剩余时长（`:2805-2813`）；
* 施毒者下线 → 整条毒移除（`MonsterObject.cs:1464-1474`）。

包序：本人 `S.Poisoned{Poison}`、周围 `S.ObjectPoisoned{ObjectID, Poison}`（`HumanObject.cs:727-732`）；
每跳 `S.DamageIndicator{Hit, -value}`。

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
| S→C | `S.ObjectSpell`（149） | 火墙落地（每格一条） | `object_id, location, spell, direction, param` | `Map.cs:1129-1141`（`Spawned()`） |
| S→C | `S.ObjectRemove`（26） | 火墙到期/施法者离开/回收 | `object_id` | `SpellObject.cs:77-78`、`Map.cs:4198` |
| S→C | `S.ObjectHidden`（147） | 隐身进入/解除（广播给周围，不含本人） | `object_id, hidden` | `MapObject.cs:80-92`、`:672-694` |
| S→C | `S.Poisoned`（96） | 玩家毒型变化（只给本人） | `poison` | `HumanObject.cs:727-732` |
| S→C | `S.ObjectPoisoned`（97） | 毒型变化（广播给周围） | `object_id, poison` | 同上 |
| S→C | `S.ObjectTeleportOut`（99）/ `ObjectTeleportIn`（100） | 瞬息移动 | `object_id, type_` | `MapObject.cs:784-786` |
| S→C | `S.ObjectMonster`（71） | 召唤物出生/外观更新（带 `MasterObjectId`） | `object_id, monster, …` | `Map.cs:914-933` |
| S→C | `S.ObjectStruck`（74） | 受击动作（雷电/火墙命中的表现） | `object_id, location, direction` | `MapObject.cs` 受击分支 |
| S→C | `S.Chat`（30） | 学书失败、火墙上限、地图禁止传送/召唤提示 | `message, type_:System` | `ReceiveChat`（`HumanObject.cs:1047` 等） |

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
| ThunderBolt | 36 | 单体 | ✅ | **不死系 ×1.5 已实现**（`MonsterKind.undead`，`HumanObject.cs:4124-4128`） |
| SoulFireBall | 64 | 单体 | ✅ | 未消耗护身符（`HumanObject.cs:4382-4396`） |
| HellFire | 35 | 前方贯穿 | ✅ | 直线 4 格、每格 100ms；3 级三向 |
| FireBang | 38 | 3×3 区域 | ✅ | 落点来自包里的 `location` |
| Healing | 61 | 单体治疗 | ✅ | 分段生效（§7） |
| MassHealing | 75 | 3×3 治疗 | ✅ | 分段生效（§7） |
| **FireWall** | 39 | 持续伤害场 | ✅ **第 2 轮新增** | 5 格、每 2s 结算、`10 + 伤害/2` 秒；`S.ObjectSpell`/`S.ObjectRemove`（§6.2） |
| **Lightning** | 40 | 前方贯穿 | ✅ **第 2 轮新增** | 6 格逐格打第一个目标，无 `SpellEffect` 变体（§6.3） |
| **Teleport** | 37 | 位移 | ✅ **第 2 轮新增** | 随机落点以**当前坐标**为中心（Crystal 用回城点，§6.4 差异） |
| **Hiding** | 67 | 自身状态 | ✅ **第 2 轮新增** | 走/跑、到期、死亡解除（与 Crystal 一致，§6.5） |
| **SummonSkeleton** | 65 | 召唤 | ✅ **第 2 轮新增** | 骷髅；共享上限 2；不耗护身符（§6.6） |
| **SummonShinsu** | 78 | 召唤 | ✅ **第 2 轮新增** | 神兽；共享上限 2；不耗护身符（§6.6） |
| **Poisoning** | 63 | 持续伤害 | ✅ **第 2 轮新增** | 绿/红毒确定性选择；2s 一跳；可对玩家施毒（PK 规则，§6.7） |
| ProtectionField | 12 | 增益（护盾） | ✅ 已实现 | `MinAC/MaxAC += round(MaxAC × (0.2 + 0.03L))`，`45 + 15L` 秒（`HumanObject.cs:4955-4963`） |
| Haste | 93 | 增益（攻速） | ✅ 已实现 | `AttackSpeed += 2L + 2`，`25 + 15L` 秒（`:6149-6153`） |
| MagicShield | 43 | 增益（减伤） | ✅ 已实现 | `DamageReductionPercent = 10(L+2)`，每点伤害扣 60ms（`:6198-6204`、`:7165`） |
| SoulShield / BlessedArmour | 69/71 | 增益（防御） | ✅ 已实现 | 落点 7×7、`Level/7 + 4` 的 `MaxMAC`/`MaxAC`（`Map.cs:1046-1097`） |
| EnergyShield | 84 | 增益（受击回血） | ✅ 已实现 | `EnergyShieldPercent = round(100/chance)`、`30 + 50L` 秒（`:4751-4783`） |
| LightBody | 95 | 增益（敏捷） | ✅ 已实现 | `Agility += 2(L+1)`，`30(L+1)` 秒（`:6187-6191`） |
| Fury | 16 | 增益（攻速） | ✅ 已实现 | `AttackSpeed += 4`，`60 + 10L` 秒（`:6160-6164`） |
| MagicBooster | 51 | 增益（魔力） | ✅ 已实现 | `MinMC/MaxMC += 6 + 6L`、耗蓝 +`6 + L`%，60 秒（`:4345-4350`、`:6228-6240`） |
| Rage / ImmortalSkin / UltimateEnhancer / PetEnhancer / ElementalBarrier | 13/17/77/85/131 | 增益 | ⛔ 登记 | 见 `magic::REGISTERED_UNSUPPORTED`（`docs/world/buffs.md` §6） |
| 其余（`Blink`、`ShoulderDash`、`ThunderStorm`、`PoisonCloud`、`SummonHolyDeva`、`TrapHexagon`、`IceStorm` 等） | — | — | ❌ 未实现 | 模板能查到，施法返回 `UnsupportedSpell`；若模板缺失则 `UnknownSpell` |

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
2. **攻击魔法的目标范围**：`Bolt` 类只认怪物（`attack_target_position` 只查怪物，
   目标玩家返回 `InvalidTarget`）；`Poisoning` 走 `magic_attack_position`，可对
   **按 PK 规则可攻击的玩家**施毒（与物理攻击共用 `is_attack_target_player`）；
   `FireWall` 中心格与 `Lightning` 逐格的**范围结算**也会命中玩家（同样按 PK 规则，
   排除施法者自己）。这是第 2 轮补的玩家伤害路径。
3. **目标合法性在扣蓝之前严格校验**。Crystal 在扣蓝后才发现目标无效，
   此时仍算一次施法（`cast = true`、`targetID = 0`、无伤害）。
   本作选择「不合法就不扣蓝」，避免玩家被误扣。射程检查仍保持 Crystal 的
   「写在计时器之前、失败静默」。
4. **`C.Magic.SpellTargetLock` 未实现**：落点始终取包里的 `location`；
   `FireBang`/`MassHealing`/`FireWall` 等不需要锁定目标的逻辑因此不受影响。
5. **`CanFly` 未实现**：Crystal 会检查目标格是否允许飞行道具（火球/灵魂火球），
   地图层没有这个属性，本作只按距离判定。
6. **不死系加成（ThunderBolt ×1.5）已实现**：`MonsterKind.undead` 已从
   `mir-db` 桥接，`HumanObject.cs:4126` 的 `(int)(damage * 1.5F)` 用 f32 等比换算。
7. **护身符/毒药消耗未实现**：`SoulFireBall`（1 张）、`Hiding`（1 张）、
   `SummonSkeleton`（1 张）、`SummonShinsu`（5 张）、`SoulShield`/`BlessedArmour`（各 1 张）
   在 Crystal 都要消耗护身符/毒药（`GetAmulet`/`GetPoison`），本作还没有消耗品系统，
   **一律不消耗**。`Poisoning` 也不消耗毒药（见第 18 条）。
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
13. **失败提示是硬编码中文**（`World` 层没有语言目录）。第 2 轮新增的
    「火墙数量已达上限」「这张地图不能使用瞬息移动」「这张地图不能召唤宠物」同样如此，
    已登记在 [`docs/i18n.md`](../i18n.md) §7.2。
14. **治疗类的射程按目标实际位置判定**。Crystal 只检查包里的 `location`
    （`HumanObject.cs:3416`），目标对象本身不在射程检查内；本作对 `Healing` 直接取目标坐标判距离，
    更严格也更符合直觉。
15. **区域技能回显的 `target_id` 恒为 0**。Crystal 会把包里的 `TargetID` 原样回显
    （`HumanObject.cs:3778`），但 `SpellTargetLock` 未实现，落点一律取包里的 `location`，
    回显一个不参与结算的目标 ID 只会让客户端困惑。
16. **瞬息移动以当前坐标为中心**（§6.4）。Crystal 用 `BindMapIndex` + `BindLocation`
    （回城点），本作 `Player` 尚未建模 bind 点；随机半径 `Width/(Level+1)`、
    `Height/(Level+1)` 与 200 次尝试与 Crystal 相同。这是**行为差异**。
17. **隐身的解除条件与 Crystal 一致**（§6.5）：只有走路/跑步、到期、死亡会解除；
    攻击/施法/受击都不解除（Crystal 只在这几处解除 MoonLight/DarkBody）。
    任务书里的「移动/攻击解除」按源码口径收窄为「移动解除」。
18. **施毒的绿/红毒选择是确定性的**（§6.7）。Crystal 由装备毒药的 `Shape` 决定
    （1 绿 / 2 红，`HumanObject.cs:6050-6070`）；本作「目标无绿毒则绿、有绿毒则红」。
    另外 `Value` 里省略了 `+ Random.Next(Stats[Stat.PoisonAttack])`（本作没有该属性）。
19. **召唤的简化**：共享上限 2 只（与 Crystal 一致），但**不做** `PetSave`
    （下线保留宠物）、`PetExp`（宠物经验，Crystal 有 ×3 设定）、护身符消耗；
    召唤物优先打主人最近攻击的目标（`MagicState::last_target`，本作扩展；
    Crystal 由召唤物自己 `FindTarget`，`MonsterObject.cs:1918-1997`）。
20. **火墙没有 `ObjectType.Spell` 对象层**：本作用 `SpellField`（中心 + 半径 + 到期）表示，
    每 tick 用 AOI 查同格对象；Crystal 是真实 `SpellObject` 挂在地图格上。
    可观察行为（5 格、2s 结算、到期消失）一致，网络包是 `S.ObjectSpell`/`S.ObjectRemove`。
21. **地形校验口径不一致（第 1 轮遗留）**：`HellFire`/`FireBang` 的格子判定仍只查边界
    （`in_bounds`），不查地形可走（Crystal 的 `ValidPoint` 含 `Cell.Valid`）；
    第 2 轮新写的 `FireWall`/`Lightning` 已统一用 `walkable_on` 对齐 Crystal。
22. **不死系标记在刷怪时回填**：`MonsterKind` 没有加字段，`Monster.undead` 在
    `World::spawn_monster` 里从 `MonsterInfo.Undead` 回查填入（`ThunderBolt` 用它）；
    `monster_kind` 夹具默认 `false`。
23. **野生怪反击召唤物（第 4 轮已实现，不再是差异）**：野生怪会在 `find_target` 里主动把
    宠物当目标，宠物被咬（或被玩家攻击）后下一 tick 反击（Crystal `MonsterObject.cs:2481-2484`、
    `:1940-1963`、`:2601-2610`）；守卫/箭塔的例外、玩家 `All` 模式限制与「宠物不咬主人」
    等细节见 §6.6。**有意差异**：宠物之间不互相攻击（`find_pet_target` 只扫野生怪；
    Crystal 的 Pet Vs Pet 分支不实现，见 §6.6）。

---

## 11. 未做（TODO）

- 开关型技能（`SpellToggle`）：需要玩家级开关状态与攻击分支配合（`Thrusting`、`HalfMoon`、
  `FlamingSword`、`TwinDrakeBlade`…）。
- **仍登记但无效果的增益技能**：`Rage`、`ImmortalSkin`、`UltimateEnhancer`、
  `PetEnhancer`、`ElementalBarrier`（都是 `AddBuff`，只是效果需要额外系统：
  狂暴/金刚不坏的百分比换算是纯数值、神兽强化要宠物）。
- 护身符/毒药消耗：见 §10 第 7 条（`SoulFireBall`/`Hiding`/`Summon*`/`SoulShield`/
  `BlessedArmour`/`Poisoning` 都不消耗）。
- 位移/传送类：`Blink`、`ShoulderDash`、`BackStep`、`StormEscape`
  （`Teleport` 已实现，见 §6.4）。
- 召唤/宠物：`SummonHolyDeva`；`SummonSkeleton`/`SummonShinsu` 已实现（§6.6），
  但 `PetSave`（下线保留）、`PetExp`（宠物经验）未做。
- 持续伤害/中毒：`PoisonCloud`、`FrostCrunch` 的减速/冰冻（`Poisoning` 已实现，§6.7）。
- 地图级持续技能对象：`ThunderStorm`、`TrapHexagon`、`HealingCircle` 等
  （`FireWall` 已用 `SpellField` 实现，§6.2）。
- 英雄（`HeroObject`）技能：`S.HeroInformation.magics` 仍恒为空。
- 技能冷却的客户端下发：本作发 `ClientMagic.CastTime = 0`（Crystal 发的是
  `int.MinValue - Envir.Time` 的巨负值，语义等价）。

---

## 12. 测试清单

`crates/mir-server/tests/magic.rs`（21 个）覆盖学书、施法校验、伤害/治疗结算与持久化；
第 2 轮的 7 个新技能在 `crates/mir-server/tests/skills.rs`（20 个用例，逐条见下表）；
增益技能（9 个）与药水/卷轴的用例在 `crates/mir-server/tests/use_items.rs`
（18 个，见 `docs/world/buffs.md` §7 与 `docs/world/items.md` §14）。

`tests/skills.rs`（第 2 轮新增）：

| # | 用例 | 断言要点 |
|---|---|---|
| 1 | `fire_wall_spawns_five_cells_and_burns_each_cell_every_two_seconds` | 5 格 `S.ObjectSpell`、每格 2s 结算、`MAC` 伤害 |
| 2 | `fire_wall_without_mana_is_rejected` | 缺蓝拒绝、不落地 |
| 3 | `fire_wall_limit_recycles_the_oldest_cast` | 地图 `FireWallLimit` 达上限回收最老一次施法（`S.ObjectRemove`） |
| 4 | `lightning_pierces_six_cells_hitting_first_target_per_cell` | 6 格逐格首个目标、内层 `break`、外层继续 |
| 5 | `lightning_without_mana_is_rejected` | 缺蓝拒绝 |
| 6 | `teleport_moves_the_caster_at_200ms_and_applies_mana_penalty` | 200ms 延迟、`ObjectTeleportOut/In`、30 秒 +30% 耗蓝 |
| 7 | `teleport_without_mana_is_rejected_and_keeps_position` | 缺蓝拒绝、位置不变 |
| 8 | `hiding_hides_from_monsters_and_breaks_on_walking_only` | `S.ObjectHidden` 广播、走一步解除、攻击不解除 |
| 9 | `hiding_is_seen_by_cool_eye_monster_of_equal_level` | `CoolEye` 且等级相同的怪能破隐 |
| 10 | `hiding_without_mana_is_rejected` | 缺蓝拒绝 |
| 11 | `recasting_hiding_while_hidden_only_refreshes_duration` | 重复施放只刷新时长 |
| 12 | `summon_skeleton_creates_a_pet_that_attacks_the_masters_target` | 召唤物出生、跟随主人目标 |
| 13 | `summoning_the_same_pet_again_recalls_instead_of_creating_a_second` | 同名活宠只召回 |
| 14 | `pets_disappear_when_the_master_logs_out_or_dies` | 主人下线/死亡清宠物 |
| 15 | `summoned_pets_do_not_grant_experience` | 召唤物死亡不给主人经验 |
| 16 | `summoning_on_a_no_pets_map_is_rejected_after_spending_mana` | `NoPets` 地图：已扣蓝、召唤失败（与 Crystal 一致） |
| 17 | `poisoning_applies_green_then_red_and_ticks_each_two_seconds` | 先绿后红、每 2s 扣血 |
| 18 | `poisoning_requires_a_target_and_does_not_spend_mana` | 目标非法：不扣蓝 |
| 19 | `thunderbolt_gets_fifty_percent_bonus_against_undead` | 不死系 ×1.5 |
| 20 | `undead_flag_is_bridged_from_monster_info_at_spawn` | `MonsterInfo.Undead` 在刷怪时回填 `Monster.undead` |

`tests/magic.rs`（第 1 轮 21 个）：

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
第 2 轮的 7 个新技能各走自己的 `SpellKind`，`REGISTERED_UNSUPPORTED` 里的仍是 `Unsupported`）。

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

