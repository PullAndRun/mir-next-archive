# 任务发布绑定审计（只读，2026-10-21）

## 结论
`QuestInfo` 在**数据库里没有 `NpcIndex` / `FinishNpcIndex` 字段**（Crystal `QuestInfo.cs:85-110` 也不从 DB 读这两个；它们由 `NPCScript.ParseQuests` 在加载时回填）。本作 `QuestInfo` 结构同样没有这两个字段，绑定来源是：
1. `NpcInfo.CollectQuestIndexes` / `FinishQuestIndexes`（Jev 样本全为空）；
2. NPC 脚本 `[Quests]` 段：正数=发布、负数=交付（Crystal `NPCScript.cs:711-714`）。

Jev 实际数据（`GameData::discover()` 读取 `suprcode-crystal-database/Jev/Server.MirDB`）：

| 任务 | NpcIndex（由脚本反推） | NPC 名/NPCInfo.Index | DB CollectQuestIndexes | 脚本 [Quests] | 结论 |
|---|---|---|---|---|---|
| quest 1 | 3 | Assistant_Jane / 3 | `[]` | `Jane.txt:19`：`1, -2, 3, 7, 10, 13, 16, 19` | A：数据由脚本绑定，发布者 Jane |
| quest 5 | 5 | Blacksmith_Smith / 5 | `[]` | `Blacksmith.txt:68`：`5, -5, 6, -6` | A：数据由脚本绑定，发布者 Smith；NPC 19 不含 5 |
| quest 7 | 3 | Assistant_Jane / 3 | `[]` | `Jane.txt:19`：`7` | A：发布者 Jane，交付者 `Wa-Master.txt:8` 的 `-7`（Master_Wa，Index16） |

- **B（解析漏）不成立**：`NpcScript::publishes_quest` 对上述 `[Quests]` 正数均返回 true；`npc_gives_quest` 已优先检查脚本。
- **C（判定过严）不成立**：Crystal 也以 `NPCScript.ParseQuests` 回填 `QuestInfo.NpcIndex`；本作没有额外的 DB `NpcIndex` 可回退。
- gate 的 `NPC 19 does not offer quest 5`：Jev 的 `NpcInfo.Index=19` 是 `HighAssassin_Cloud`（`SinCloud.txt:9`：`-16, 17, -17, 18`），确实不发布 quest 5。**这是 gate 选错 NPC / 走到了错 NPC，不是数据或解析 bug。**

## 证据命令
```text
cargo run -p mir-server --example quest_bind_audit
# QUEST 1/5/7 输出；NPC 19 = HighAssassin_Cloud
```
相关数据：
- `Jev/Envir/NPCs/BichonProvince/BorderVillage/Jane.txt:19`
- `Jev/Envir/NPCs/BichonProvince/BorderVillage/Blacksmith.txt:68`
- `Jev/Envir/NPCs/BichonProvince/Wa-Master.txt:8`
- `Jev/Envir/NPCs/BichonProvince/SinCloud.txt:9`
- `Jev/Envir/Quests/BichonProvince/BorderVillage/5.txt:31-32`（Pickup/Hand In Smith 0 296 613）
