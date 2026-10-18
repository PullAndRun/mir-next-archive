-- 0012_character_quest_inventory.sql — 任务物品栏（Crystal `CharacterInfo.QuestInventory`，40 格）。
--
-- 格式与 `characters.storage` / `characters.quests` 一致：Crystal `UserItem[]` 整块 BYTEA。
-- 编码/解码在 `mir-server` 侧：`quest_inventory::encode_quest_inventory` /
-- `decode_quest_inventory`（逐字对照 `CharacterInfo.Save`，`Server/MirDatabase/CharacterInfo.cs:449-456`：
--   int32 槽位数 + 每槽 `bool 有物品?` + `UserItem.Save`）。
--
-- 40 格是 Crystal 的默认长度（`CharacterInfo.cs:75` 的 `QuestInventory = new UserItem[40]`）；
-- 解码时按 40 截断/补齐。空数组（没有任务物品）用零长度 BYTEA 表示。
--
-- 与 Crystal 的差异：Crystal 把 `QuestInventory` 和背包/装备/仓库一起写在角色块里，
-- 本作沿用 `characters.*` 一列一块的约定，单独一列便于迁移与排障。

ALTER TABLE characters
    ADD COLUMN quest_inventory BYTEA NOT NULL DEFAULT '\x'::bytea;
