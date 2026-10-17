-- 0003_character_magics.sql — 角色已学技能（`CharacterInfo.Magics`）的二进制整块。
--
-- 格式对应 Crystal `CharacterInfo.Save`（`Server/MirDatabase/CharacterInfo.cs:458-463`）：
--
--   int32 条数
--   每条：u8 Spell / u8 Level / u8 Key / u16 Experience / bool IsTempSpell / i64 CastTime
--          （`Server/MirDatabase/MagicInfo.cs:135-144`，共 14 字节）
--
-- 与背包/装备/仓库一样先用 BYTEA 整块保存：Crystal 的 `UserMagic` 只有标量字段，
-- 没有需要按行查询/更新的内容，拆表没有收益。空数组用零长度 BYTEA 表示（新角色）。
--
-- 读侧：`CharacterInfo.cs:226-234`——条数之后逐条 `new UserMagic(reader)`；
-- 模板（`MagicInfo`）查不到时 Crystal 会**丢弃这一条**（`:230`），
-- 本作在服务层保留原样、只在组装客户端视图时跳过（见 `docs/world/magic.md`）。

ALTER TABLE characters
    ADD COLUMN magics BYTEA NOT NULL DEFAULT '\x'::bytea;
