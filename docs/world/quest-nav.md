# 任务导航（TP-1 服务端）

## 契约
`S.ChangeQuest.task_list` 末尾追加一行（按玩家语言）：

- 有导航目标：`前往 <MapInfo.title>(<MapInfo.file_name>)`（en: `Go to <title> (<file>)`）
- 没有导航目标：`该任务无导航目标`（en: `This quest has no navigation target`）

## 导航目标推导顺序
1. **显式地图标记（补充来源）**：解析 `[@TaskDescription]` 的 `{X/Color}`；只有 `X` 与
   `MapInfo.title`（去首尾空白、大小写不敏感）完全相等时才转成导航行；
   `{MasterMage}` / `{CannibalLeaves}` 这类 NPC/物品名反查不到地图，直接忽略。
2. **目标 NPC 地图（主来源）**：`World::quest_finisher_npc(quest_index)`（无交付 NPC 时退回
   `quest_publisher_npc`）→ `Npc.map` → `GameData::map_info` 的 title/file。
3. 两者都没有 → `QuestNavigationNone`。

服务端实际加载的是 `resource/community/suprcode-crystal-database/Jev/Server.MirDB` +
`Jev/Envir/Quests`；导航推导基于这份真实数据，不修改 `Server.MirDB`。

## 真实数据来源
- `resource/community/suprcode-crystal-database/Jev/Envir/Quests/BichonProvince/BorderVillage/5.txt:31-32`
  ```text
  // Pickup - Blacksmith_Smith 0 296 613
  // Hand In - Blacksmith_Smith 0 296 613
  ```
  → quest 5 的 handin NPC `Blacksmith_Smith` 在地图 0；`MapInfo.file_name = "0"`，
  测试断言 `task_list` 出现 `前往 <map0 title>(0)`。
- 参考 NPC 定义：`Jev/Server.MirDB` 的 `NpcInfo`（`Blacksmith_Smith`/`map_index=0`）。
- Jev quest 1/5/7 的 `[@TaskDescription]` 主要是物品/NPC/怪物名（如
  `{CannibalLeaves}`、`{CraftLady}`、`{Master_Wa}`），按显式标题规则不会误当地图；
  导航行由 handin/pickup NPC 地图兜底。

## 测试
- `quests::tests::navigation_only_converts_real_map_titles`（英文夹具 + en 渲染）
- `quests::tests::real_jev_quest_5_navigation_uses_handin_npc_map`（读真实 Jev，动态取 NPC/map，不硬编码中文）
- `quests::tests::empty_task_description_has_no_task_navigation_line`（空/非地图标记不产导航）
- 集成：`tests/quests.rs` 的 `accept_quest...`/`kill_progress...`/`timed_quest...` 断言
  `S.ChangeQuest.task_list` 含 `前往 <地图标题>(<mapfile>)`。
- 客户端解析：`crates/mir-client/src/ui/quest_nav.rs`（已按 `前往 地图名(mapfile)` 实现）。
