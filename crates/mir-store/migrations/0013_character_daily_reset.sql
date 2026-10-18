-- 0014_character_daily_reset.sql — 每日任务惰性重置时间戳。
--
-- Crystal 在服务器跨天（0 点）时对全部角色执行 `ClearDailyQuests`（`Envir.cs:2311-2315`、
-- `:4856-4866`、`:5009-5024`）；本作没有服务器定时器，改为在登录/任务交互时按角色
-- 惰性补做：`now - last_daily_reset_at >= 24h` 就重置（`quests::daily_reset_due`）。
--
-- 单位 Unix 毫秒；0 表示从未重置过（新角色/旧存档），首次判定即到期（重置是幂等的）。

ALTER TABLE characters
    ADD COLUMN last_daily_reset_at BIGINT NOT NULL DEFAULT 0;
