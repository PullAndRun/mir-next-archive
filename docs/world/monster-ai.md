# 怪物 AI：远程攻击 / 回位 / 掉落组（第 7 轮新增）

本文件记录第 7 轮补的怪物 AI 分支与掉落组概率语义。基础索敌/追击/两段式攻击见
[`docs/world/combat.md`](combat.md)，宠物 AI 见 [`docs/world/magic.md`](magic.md) §6.6。

## 1. 远程攻击 AI

`MonsterKind::ranged_attack_range`（`crates/mir-server/src/monster.rs:161`）按 Crystal 的
AI 编号给射程；`monster.rs:814` 的 `process_target` 在射程内走远程分支，否则走近：

| AI | 模板 | 射程 | 备注 |
|---|---|---|---|
| 8、164、175、199 | 弓箭手类 | **6** | `AttackRange` |
| 16 | `RedThunderZuma` | **9** | 贴身改近战（`melee_when_adjacent`，`:173`） |
| 31 / 32 | `RightGuard` / `LeftGuard` | **8** | 贴身改近战 |
| 57 | `TownArcher` | **10** | 不追、无 Route 站桩（`is_town_archer`，`:180`；Crystal `TownArcher.CanMove`，`TownArcher.cs:52-120`） |
| 80 | `ConquestArcher` | **10** | 攻城中箭塔只射敌对行会 |
| 113 | `ArcherGuard`（名字含 `ArcherGuard`/`弓箭守卫`） | `view_range` | 站桩远程守卫（`is_archer_guard`，`:180`；Crystal `ArcherGuard.cs:22-44`）；**Armadillo 等其它 113 模板仍不远程** |

`ranged_attack`（`monster.rs:1133`）广播 `S.ObjectRangeAttack`，伤害延迟
`max_distance * 50 + 500 ms`（Crystal `MonsterObject.ProjectileAttack`）。
Crystal 对照：`RightGuard.cs:10-53`、`RedThunderZuma.cs:8-56`。

**未做**：法师/直线/AOE 远程（`MonsterMagic`、AI 10/19/20/26/34/36/38/45/46/50/51/55/61/69/71/72/73/75/83/84/86/88/91/92/93/94/96/101/102/105/107/108/109/118/120/122/123/129/131/140/141/142/143/150/153/156/163/178/182/189/190/200/201 等子类技能）；
AI 113 在真实 Jev 库被用作 `Armadillo`，所以代码用**名字**再确认：只有
`ArcherGuard`/`弓箭守卫` 才按远程处理，其它 113 模板（如 Armadillo）仍走近战。

**客户端表现（第 7 轮已接）**：`net.rs` 的 `S.ObjectRangeAttack`(223) 分支会更新实体
坐标/朝向并进入 `Attacking`，播放 `SoundId::Swing`；`spell != None` 时在命中格推一条
`SpellEvent`（复用 `magic.wzl`/`magic2.wzl` 的命中闪光与闪电束）。物理弹道用挥砍音 +
命中特效近似，不做独立投射物精灵。

## 2. 追击与回位

* 目标丢失且离家超过 `view_range` → `process_return_home`（`monster.rs:701`）走回出生点；
* 没有显式的 `Chase/Return` 状态枚举（Crystal 基类只随机漫游，本作用 `home` 兜底）；
* `TownArcher` 无 Route 时**不追**（测试 `town_archer_does_not_chase_without_a_route`）。

## 3. 逃跑 / 召唤

**未做**：各 AI 子类特有的逃跑/召唤分支（本作没有 `MonsterKind` 子类脚本/召唤表）。
宠物召唤是玩家技能（`SummonSkeleton`/`SummonShinsu`），见 `magic.md` §6.6。

## 4. 掉落组概率语义（`crates/mir-db/src/drops.rs`）

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

## 5. 怪物 AI 测试（`crates/mir-server/tests/skills.rs`）

`ranged_ai_ranges_cover_the_data_archer_templates`、`ranged_monster_shoots_from_six_cells`、
`town_archer_does_not_chase_without_a_route`、`target_lost_monster_returns_home`。
