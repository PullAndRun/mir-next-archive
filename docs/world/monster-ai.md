# 怪物 AI：远程/魔法攻击、回位与掉落组（第 7/8 轮）

本文件记录第 7/8 轮补的怪物 AI 分支与掉落组概率语义。基础索敌/追击/两段式攻击见
[`docs/world/combat.md`](combat.md)，宠物 AI 见 [`docs/world/magic.md`](magic.md) §6.6。

## 1. 远程物理攻击 AI

`MonsterKind::ranged_attack_range`（`crates/mir-server/src/monster.rs:161`）按 Crystal 的
AI 编号给射程；`monster.rs:814` 的 `process_target` 在射程内走远程分支，否则走近：

| AI | 模板 | 射程 | 备注 |
|---|---|---|---|
| 8、164、175、199 | 弓箭手类 | **6** | `AttackRange` |
| 16 | `RedThunderZuma` | **9** | 贴身改近战（`melee_when_adjacent`，`:173`） |
| 31 / 32 | `RightGuard` / `LeftGuard` | **8** | 贴身改近战 |
| 57 | `TownArcher` | **10** | 不追、无 Route 站桩（`is_town_archer`，`:180`；Crystal `TownArcher.CanMove`，`TownArcher.cs:52-120`）；**只锁定红名**（`pk_points >= 200`，Crystal `TownArcher.cs:96-112`），白名新角色不会被出生点旁的 Royal_Archer 秒杀 |
| 80 | `ConquestArcher` | **10** | 攻城中箭塔只射敌对行会 |
| 113 | `ArcherGuard`（名字含 `ArcherGuard`/`弓箭守卫`） | `view_range` | 站桩远程守卫（`is_archer_guard`，`:180`；Crystal `ArcherGuard.cs:22-44`）；**Armadillo 等其它 113 模板仍不远程** |

`ranged_attack`（`monster.rs:1133`）广播 `S.ObjectRangeAttack`，伤害延迟
`max_distance * 50 + 500 ms`（Crystal `MonsterObject.ProjectileAttack`）。
Crystal 对照：`RightGuard.cs:10-53`、`RedThunderZuma.cs:8-56`。

**第 8 轮已补**：真正走 `S.ObjectMagic` 的 **AI 55 `Clone`/`HumanWizard`**（见 §2）；
另有按 Crystal 参考类**预留**的 `AI 215 SepWizard`（GreatFireBall）、`AI 216 SepTaoist`
（SoulFireBall）、`AI 220 SepHighWizard`（FireBang AOE）——采样 12 份 `Server.MirDB` 里
都不存在，导入新版数据后可直接生效。物理 AOE/直线（AI 13/29/**14/50/100**）也在 §2。
仍然**未做**的：`AI 26/31/32/36/45/46/101…` 等其余 `ObjectMagic` 子类；
`AI 93/94/96` **不是魔法**——它们是 `S.ObjectRangeAttack`/`S.ObjectAttack` 物理攻击；
AI 14 的显隐/回血状态机、Slow/Paralysis 的完整移动/攻速表现（第 8 轮已接 `BuffKind::Slow`/
`Paralysis` 的基础行为，见 §2.5）、毒抗与 `S.ObjectEffect` GreatFoxSpirit 特效、
毒效果在出招时结算（Crystal 在伤害落地时）。
AI 113 在真实 Jev 库被用作 `Armadillo`，所以代码用**名字**再确认：只有
`ArcherGuard`/`弓箭守卫` 才按远程处理，其它 113 模板（如 Armadillo）仍走近战。

**客户端表现（第 7 轮已接）**：`net.rs` 的 `S.ObjectRangeAttack`(223) 分支会更新实体
坐标/朝向并进入 `Attacking`，播放 `SoundId::Swing`；`spell != None` 时在命中格推一条
`SpellEvent`（复用 `magic.wzl`/`magic2.wzl` 的命中闪光与闪电束）。物理弹道用挥砍音 +
命中特效近似，不做独立投射物精灵。

## 2. 怪物魔法攻击（第 8 轮）

### 2.1 真正走 `S.ObjectMagic`：AI 55 `Clone` / `HumanWizard`

| 项 | 值 |
|---|---|
| 技能 | `Spell::ThunderBolt`（单体） |
| 射程 | **6** 格 |
| 冷却 | `attack_speed`（默认 **2500 ms**） |
| 伤害 | `base = GetAttackPower(MinMC, MaxMC)` → `UserMagic::damage(ThunderBolt MagicInfo)`（复用 `magic.rs` 公式），按 **MAC** 结算；**与 Crystal 不同**：`HumanWizard.Attack` 是直接 `GetAttackPower`、不套 MagicInfo —— 本项目有意差异 |
| 包 | `S.ObjectMagic`（`level = 3`，`secondary_target_ids` 为溅射目标） |
| 延迟 | 施法后 **500 ms** 结算 `PendingHit` |
| 风筝 | 前 **5 秒**（`FearTime`）只按 `dist < 6` 走位不施法，窗口结束后才放技能 |

实现：`monster.rs:264 monster_magic`、`:1370 process_magic_target`、
`:1415 cast_monster_magic`（伤害在 `monster.rs:1457` 附近，复用 `UserMagic::damage`）。
Crystal `Monsters/HumanWizard.cs:25-120`。

**预留的 Sep 系魔法 AI**（`monster.rs:295` 附近；采样 12 份 `Server.MirDB` 里都不存在，
按 Crystal 参考类预留，导入新版数据后生效）：

| AI | 模板 | 技能 | 射程 | 形态 |
|---|---|---|---|---|
| 215 | `SepWizard` | `GreatFireBall` | 5 | 单体 |
| 216 | `SepTaoist` | `SoulFireBall` | 5 | 单体 |
| 220 | `SepHighWizard` | `FireBang` | 6 | AOE 半径 1 |

注意：`AI 93/94/96` **不是** `ObjectMagic` 系——它们走 `S.ObjectRangeAttack`/`S.ObjectAttack`
物理攻击，不在本表。

### 2.2 物理特殊（走 `S.ObjectAttack` / `S.ObjectRangeAttack`，不是 MonsterMagic）

| AI | 模板 | 形态 | 说明 |
|---|---|---|---|
| 13 | `RedMoonEvil` | 自身为中心 AOE，半径/射程 = `ViewRange`（7），站桩 | 收集玩家 + 召唤物；排除安全区/`NoFight`；DC + `ACAgility`；延迟 **300 ms**（Crystal `RedMoonEvil.cs:30-60`） |
| 29 | `BoneSpearman` | 直线穿透 2 格（蛇形邻域） | 邻域判定 `x<=2 && y<=2 && ((x<=1&&y<=1)||x==y||x%2==y%2)`（`monster.rs:125 in_snake_neighborhood`）；DC + `ACAgility`；每目标延迟 `距离*50 + 250`（第一格 300、第二格 350）；Crystal `BoneSpearman.cs:14-45`、`MonsterObject.LineAttack` |
| 14 | `EvilCentipede` | 站桩、射程 7、自身为中心 AOE 半径 7 | `S.ObjectAttack` 物理 AOE；命中后**绿毒**（1/5 概率、15 跳、2000ms）+ **麻痹**（1/15、5 跳、2000ms）；不是 `Poisoning` 魔法（Crystal `EvilCentipede.cs:74-84`） |
| 50 | `GreatFoxSpirit` | 站桩；距离 >2 远程 AOE 半径 7（广播 `S.ObjectRangeAttack`），否则半径 2 + `S.ObjectAttack` | 命中后 **Slow**（1/5、15 跳、1000ms）+ **Paralysis**（1/5、5 跳、1000ms）；Crystal `GreatFoxSpirit.cs:100-133`（远程形态的 `S.ObjectEffect` 特效本作未发） |
| 100 | `VenomSpider` | 蛇形邻域、直线 2 格，`S.ObjectAttack` | `LineAttack(2, 300, MACAgility)`；命中后**绿毒**（1/8、5 跳、1000ms）；Crystal `VenomSpider.cs:14-47` |

实现：`monster.rs:288 monster_special`、`process_special_target`、`magic_targets`（`:1488`）。

**未做（AI 14 相关）**：显隐/回血状态机；Slow/Paralysis 的移动/攻速行为（`world.rs` 归兵部，
当前只挂毒状态）；毒抗与 `S.ObjectEffect` GreatFoxSpirit 特效；毒效果在出招时结算
（Crystal 在伤害落地时）。

### 2.3 目标集合与包序

* `magic_targets` 收 **玩家 + 召唤物**（`master.is_some()`），排除安全区与 `NoFight`；
  Peace/组队/行会**不参与**怪物 AOE 的目标判定（Crystal 野生怪同口径）；
* MonsterMagic：`S.ObjectMagic` → 500ms 后 `PendingHit(MAC)`；
* 物理 AOE/直线：`S.ObjectAttack` → `距离*50 + delay` 后 `PendingHit`；
  **AI 50 距离 > 2** 例外：广播 `S.ObjectRangeAttack`（半径 7），贴身才用 `S.ObjectAttack`（半径 2）；
* 直线遇到不可走格 `continue`（跳过该格，不截断后续格）。

### 2.4 Slow / Paralysis（第 8 轮）

`BuffKind` 新增 `Slow` / `Paralysis`（`buff_type` 映射 `BuffType::None`，因为 Crystal 把它们
归在 `PoisonType`）：

* `Slow`：`walk`/`run` 读 `values[0]` 百分比，移动间隔变为 `600 * (100 + pct) / 100`；
* `Paralysis`：`walk`/`attack` 直接返回 `Busy`（不能移动/攻击），到期由 buff tick 清理；
* 读取入口 `buff_value`。

AI 14/50 命中时按 §2.2 的概率挂上这两种状态。

### 2.5 测试（`tests/skills.rs`，66 条）

基础 9 条：`magic_monster_does_not_cast_out_of_range`、`magic_monster_casts_and_damages_the_target`、
`magic_monster_respects_attack_speed_cooldown`、`magic_line_attack_penetrates_two_players`、
`magic_aoe_hits_multiple_targets_but_not_outside_radius`、
`magic_cast_is_interrupted_when_target_dies`、
`bone_spearman_line_damage_is_delayed_by_distance`、
`aoe_does_not_hit_players_on_no_fight_maps`、`magic_monster_kites_when_adjacent`。

第 8 轮特殊 AI 6 条：`evil_centipede_is_physical_aoe_with_green_and_paralysis`、
`great_fox_spirit_close_range_uses_radius_two`、
`great_fox_spirit_ranged_form_uses_radius_seven_and_range_attack`、
`venom_spider_line_hits_two_cells_and_poisons`、
`evil_centipede_aoe_does_not_fire_on_no_fight_maps`、
`great_fox_spirit_aoe_does_not_fire_on_no_fight_maps`。

预留 Sep 魔法 AI 2 条：`sep_wizard_casts_great_fire_ball_and_deals_damage`、
`sep_high_wizard_fire_bang_aoe_hits_multiple_targets`。

## 3. 追击与回位

* 目标丢失且离家超过 `view_range` → `process_return_home`（`monster.rs:701`）走回出生点；
* 没有显式的 `Chase/Return` 状态枚举（Crystal 基类只随机漫游，本作用 `home` 兜底）；
* `TownArcher` 无 Route 时**不追**（测试 `town_archer_does_not_chase_without_a_route`）。

## 4. 逃跑 / 召唤

**未做**：各 AI 子类特有的逃跑/召唤分支（本作没有 `MonsterKind` 子类脚本/召唤表）。
宠物召唤是玩家技能（`SummonSkeleton`/`SummonShinsu`），见 `magic.md` §6.6。

## 5. 掉落组概率语义（`crates/mir-db/src/drops.rs`）

* `DropInfo::roll`（`:289`）用 `rng.next_bounded(chance)` 对齐 Crystal
  `Random.Next(rate)` 的**左闭右开**；金币 `[Gold/2, Gold + Gold/2)` 同样用
  `next_bounded`（`:317` 附近）；
* `Group` 子规则**各自独立掷骰**（Crystal `AttemptDrop`，`MonsterInfo.cs:547-566`）；
* `GROUP^`：命中第一条即停止；`GROUP*`：每组随机挑一条——但 Crystal 的 `Random` 字段
  实际未被 `AttemptDrop` 使用，本作照抄（只影响数据语义，行为等价）；
* `is_guaranteed`（`:281`）= `chance == 1` 必掉；
* `Q` 标记（`QuestRequired`）先截胡进任务物品栏、无人需要才不落地，见
  [`docs/world/quests.md`](quests.md) §10。

测试（`crates/mir-db`）：`group_children_are_independent_probabilities`、
`guaranteed_rule_and_rate_use_right_open_rng`。

## 6. 怪物 AI 测试（`crates/mir-server/tests/skills.rs`）

`ranged_ai_ranges_cover_the_data_archer_templates`、`ranged_monster_shoots_from_six_cells`、
`town_archer_does_not_chase_without_a_route`、`target_lost_monster_returns_home`。

## 7. 第 15 轮：TrapRock 子石、场景 AI 与巡检（t118）

### 7.1 AI 47 `TrapRock`

对照 Crystal `Monsters/TrapRock.cs` 的 `Show()`：**`Show()` 生成角落子石，不是死亡分裂**。
本作 `World::process_trap_rock`：父石只触发一次，向目标周围 4 个角落方向取 3 个
（随机跳过 `SpawnCorner`，与 Crystal `i=0..6 step2` 一致）；子石继承目标，**5 秒**后由
`tick_trap_rocks` 到期 `monster_died` 消失。测试 `trap_rock_show_spawns_children_once`、
`trap_rock_children_expire_after_five_seconds`。

### 7.2 AI 3 Tree / AI 56 Trainer

* **AI 3 Tree（场景树）**：Crystal `Tree.cs:5-9` 的 `CanMove/CanAttack/CanRegen=false` →
  固定不动、不索敌；真实 DB 有 **7** 个模板；
* **AI 56 Trainer（TrainingMob）**：Crystal `Trainer.cs:9-12` 的 `CanMove/CanAttack=false` →
  固定不动、可被攻击；真实 DB 有 **2** 个模板；
* 测试 `tree_and_trainer_are_stationary_scenery`。

### 7.3 AI 巡检工具

新增 `crates/mir-server/examples/ai_audit.rs`：按真实 `Server.MirDB` 的 `MonsterInfo.AI`
分组输出 `AI / 状态 / 怪物数 / 怪物名`；状态由 `MonsterKind::ai_support()` 统一分类
（已实现/部分/未实现），稳定排序（BTreeMap + 名称排序去重）。

第 13 轮时的结果（**历史快照，最新数字见 §9**）：**已实现 AI 覆盖 480/537 个怪物模板 = 89.4%**；
AI0 仍是「部分」（278 个模板），另有 **47 个 AI 编号标「未实现」**。测试 `audit_support_labels_and_key_ais_are_stable`
锁定 AI3/12/47/56 = Full、AI0 = Partial、AI53 = Stub 与标签。

运行：

```text
cargo run -p mir-server --example ai_audit
```

## 8. 第 13 轮收口（t121）

| AI | 模板数 | Crystal 对照 | 本作行为 |
|---|---:|---|---|
| 15 | 20 | `ZumaMonster.cs:15-60`（石化，靠近 2 格才 Wake） | 无目标时站桩，不移动/不索敌；`newly_audited_scenery_ais_stay_stationary` |
| 47 | 2 | `TrapRock.cs` 的 `Show()` | 父石只触发一次，目标周围 3 个角落子石，5 秒存活后消失 |
| 97 | 4 | `HellKnight.cs:8-24`（只有 `Summoned` 标记，其余走基类） | 基类索敌/近战；`hellknight_uses_generic_melee_path` |
| 99 | 3 | `HellBomb.cs:11-35`（CanMove=false，到点自爆） | 站桩、不移动；`newly_audited_scenery_ais_stay_stationary` |
| 102 | 2 | `IceGuard.cs:8-85`（射程 8） | `ranged_attack_range=8`；`horned_archer_and_warrior_have_audited_ranges` |
| 119 / 120 | 2 | `Jar1.cs` / `Jar2.cs`（CanMove=false） | 站桩；`newly_audited_scenery_ais_stay_stationary` |
| 128 | 1 | `TucsonEgg.cs`（CanMove=false） | 站桩；同上 |
| 164 | 1 | `HornedArcher.cs`（继承 AxeSkeleton 射程 6） | `ranged_attack_range=6` |
| 165 | 1 | `HornedWarrior.cs:8`（AttackRange=4） | `ranged_attack_range=4` |
| 181 | 2 | `WaterDragon.cs`（远程用 ViewRange） | `ranged_attack_range=view_range` |
| 210 | 4 | `HoodedSummonerScrolls.cs`（远程用 ViewRange） | `ranged_attack_range=view_range` |

巡检结果（双口径，禁止混用）：
- **全模板口径**：Full+Partial 覆盖 **516/537 = 96.09%**（Partial 390 个也计入，只表示“基类可战”，不是专属 AI 实现率）。
- **AI 号 Full 口径**：implemented **46**、partial 44、missing **21**，分母 implemented+missing=**67**，Full 覆盖率 **46/67 = 68.66%**（满足 ≥44/66 的目标；Partial 不计分子/分母）。
- `AI_BEHAVIOURS` 是 Full 的唯一来源，`ai_support()` 从表派生；`full_ai_table_entries_have_branch_and_named_test` 反向守卫遍历表，并要求 Crystal 出处、行为族、测试名齐全。`ai_audit` 还会输出「Full 但无用例」计数（当前 0）。

Full 表新增 AI（第17轮）：4/70/72/79/97/99/102/116/117/118/119/120/121/124/125/126/127/128/163/164/165/169/170/173/181/192/210（另有既有 2/3/6/8/12/13/14/15/16/29/31/32/47/50/55/56/57/58/80/100/113）。第18轮再补 69/115/122/130/179/187/190，见 §9。每个 Full 条目都给出 Crystal 出处与同名行为测试；AI97 的 Crystal 子类本身只有 `Summoned` 标记，行为等价基类，按基类路径计入。
仍未做的 AI（第18轮后剩 14 个）：53/86/88/98/112/123/131/171/174/180/182/186/188/189 等
（详见 `ai_audit` 输出；其中不少是 `ObjectMagic`/复杂 Boss 行为，不在本轮简单 AI 范围）。

## 9. 第 18 轮：7 个 missing AI 收口（t92b496a3）

| AI | 模板 | Crystal 专属出处 | 行为维度 | 用例 |
|---:|---|---|---|---|
| 69 | PoisonHugger | `Monsters/PoisonHugger.cs:8-50` | Ranged(5) | `poison_hugger_has_ranged_five` |
| 115 | SandSnail | `Monsters/SandSnail.cs:8-45` | Aoe(1) | `sand_snail_aoe_radius_one` |
| 122 | RestlessJar | `Monsters/RestlessJar.cs:8-45` | Ranged(6)+Stationary | `restless_jar_stays_stationary_and_ranged_six` |
| 130 | CannibalTentacles | `Monsters/CannibalTentacles.cs:8-45` | Ranged(ViewRange) | `cannibal_tentacles_uses_view_range_ranged` |
| 179 | SnowWolf | `Monsters/SnowWolf.cs:8-45` | Aoe(2) | `snow_wolf_aoe_radius_two` |
| 187 | FrozenMiner | `Monsters/FrozenMiner.cs:8-45` | Aoe(1) | `frozen_miner_aoe_radius_one` |
| 190 | SnowYeti | `Monsters/SnowYeti.cs:8-35` | Ranged(9) | `snow_yeti_has_ranged_nine` |

`AiBehaviour` 现含 `Stationary/Flee/Ranged/Guardian/Summon/TrapRock/Magic/Special/Aoe/Delayed/Immune/Generic`，本轮新增 Full 全部映射非 `Generic` 变体；RestlessJar 的站桩断言独立命名，不复用旧站桩用例。

巡检（第 18 轮）：AI 数 **implemented=53 / partial=44 / missing=14**，AI 号 Full 覆盖率 **53/67 = 79.10%**；模板 **implemented=133 / partial=390 / missing=14**，全模板口径 **97.39%（523/537）**；`Full 但无用例=0`。

负向自测（临时从 `AI_BEHAVIOURS` 移除表项 → 自己的用例变红 → 恢复）：

| AI | 用例 | 移除后 |
|---:|---|---|
| 69 | `poison_hugger_has_ranged_five` | FAILED（`ai_support` 不是 Full） |
| 179 | `snow_wolf_aoe_radius_two` | FAILED（`ai_support` 不是 Full） |
| 187 | `frozen_miner_aoe_radius_one` | FAILED（`ai_support` 不是 Full） |

