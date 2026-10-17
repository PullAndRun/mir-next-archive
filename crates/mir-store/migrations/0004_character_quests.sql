-- 0004_character_quests.sql — 角色任务进度（`CharacterInfo.CurrentQuests` + `CompletedQuests`）。
--
-- 格式对应 Crystal `CharacterInfo.Save`（`Server/MirDatabase/CharacterInfo.cs:488-492`、
-- `:514-518`）与 `QuestProgressInfo.Save`（`Server/MirDatabase/QuestProgressInfo.cs:291-318`），
-- 但把「模板 ID」换成模板名（Mir2 的怪物/物品模板在 Crystal 里靠对象引用对齐，
-- 本项目没有那套稳定 ID 映射），完整布局见
-- `crates/mir-server/src/quests.rs` 的 `encode_quests`：
--
--   int32  任务条数
--   每条：int32 Index / u8 State / u8 ObjectivesMet / i64 StartDateTime / i64 ExpiresAt
--         int32 KillTaskCount   + 每条 { string 怪物名 / int32 计数 }
--         int32 ItemTaskCount   + 每条 { string 物品名 / u16 计数 }
--         int32 FlagTaskCount   + 每条 { int32 标记号 }
--   int32  已完成任务条数 + 每条 int32
--
-- 与背包/装备/技能一样用 BYTEA 整块保存：任务进度只在登录/下线时整块读写，
-- 没有按行查询的需求。空数组（新角色）用零长度 BYTEA 表示。

ALTER TABLE characters
    ADD COLUMN quests BYTEA NOT NULL DEFAULT '\x'::bytea;
