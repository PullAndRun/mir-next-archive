# 耐久（EQ-1 审计）

| 接线 | 本作 | Crystal | 结论 |
|---|---|---|---|
| 攻击命中磨损武器 | `session.rs:1460` `World::damage_weapon`；1..=4 | `HumanObject.cs:7112` 调用，`:7844-7847` 实现 | 一致；`SpecialItemMode.NoDuraLoss` 未实现（特殊戒指系统缺失） |
| 受击磨损护甲/其它槽 | `world.rs:2894/3020` 对受伤玩家 `World::damage_armour` | `HumanObject.cs:7208/7298/7370` 三处 `DamageDura()`，实现 `:7837-7842` | 一致；每槽固定 -1 |
| 空槽/0 耐久/Amulet | `items.rs:771-777` | `HumanObject.cs:7850` | 一致 |
| 结婚戒指免磨损 | `items.rs` 新增：左戒指且 `wedding_ring` 等于配偶角色 ID 时跳过 | `HumanObject.cs:7852` | 本轮补 |
| 耐久归零通知 | `items.rs:791-798` 发 `S.DuraChanged(76)` + 提示 | `HumanObject.cs:7854-7857` | 一致（Crystal 惰性发送；本作只归零时发） |
| 修理包顺序 | `npc.rs:2530` 先 `S.RepairItem(114)`，再扣金币，最后 `S.ItemRepaired(114)` | `PlayerObject.cs:8092-8157`（`:8094`、`:8157`） | 一致 |
| 修理上限磨损 | `repair_item` 用 `MaxDura - (MaxDura - CurrentDura)/30` | `PlayerObject.cs:8154` | 一致 |
| DB 往返 | `fault_pg.rs::durability_survives_a_real_postgres_round_trip` | `UserItem.CurrentDurability` 在角色字节块 | 已补 |

## 已知差异/缺口
1. `world.rs:2790` 对**怪物**目标调用 `damage_armour` 是无效调用（函数只处理玩家）；保留为结构占位，不影响玩家耐久。
2. 枯竭（0 耐久）后 `refresh_stats` 已在归零路径执行；修理后若物品在背包，装备时由 `apply_equipment_stats` 重新计算。
3. 客户端 `S.DuraChanged(76)` / `S.ItemRepaired(114)` 已有测试（`crates/mir-client/src/ui/equipment.rs:503-525`）。
