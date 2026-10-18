-- 0008_conquests.sql — 攻城 / 城主任命（conquest，沙巴克攻城）持久化。
--
-- 对应 Crystal 的 `ConquestGuildInfo`（`Server/MirDatabase/ConquestGuildInfo.cs:7-92`）：
-- 那边的存档是 `Conquests/<ConquestInfo.Index>.mcd` 一个二进制文件（`Envir.SaveConquests`，
-- `Server/MirEnvir/Envir.cs:2734-2747`；读入 `Envir.LoadConquests`，`:3119-3170`），
-- 静态数据在 `Server.MirDB` 的 `ConquestInfo` 段里（`Server/MirDatabase/ConquestInfo.cs:6-51`）。
--
-- 本迁移只存**运行期状态**：静态数据（名字/地图/时间表/守卫/城门/城墙/旗子/控制点的坐标）
-- 仍然来自 `db.conquests`，改一行 SQL 不会覆盖策划配表。
--
-- ## 字段对照
--
-- | Crystal | 本迁移 | 说明 |
-- |---|---|---|
-- | （无，主键来自 `ConquestInfo.Index`，也是文件名） | `conquest_index` | 主键；一个攻城区一行 |
-- | `ConquestGuildInfo.Owner`（`:17`，int，`-1` = 无主） | `owner_guild_id` | 城主行会；`NULL` = 无主 |
-- | `ConquestGuildInfo.AttackerID`（`:19`，int，`-1` = 无人申请） | `attacker_guild_id` | 已排队的攻打方；`NULL` = 没人申请 |
-- | `ConquestGuildInfo.GoldStorage`（`:18`，uint） | `gold_storage` | 城主税收池（NPC 加价部分，`NPCScript.cs:1213`） |
-- | `ConquestGuildInfo.NPCRate`（`:20`，byte） | `npc_rate` | 城主设置的加价百分比（`SetConquestRate`，`NPCSegment.cs:4237-4249`） |
-- | `ConquestObject.AtWar`（`ConquestObject.cs:63`，`WarIsOn`） | `war_is_on` | 是否正在开战 |
-- | `ConquestObject.WarStartTime` / `WarEndTime`（`:65-66`） | `war_start_time` / `war_end_time` | 仅 `ConquestType.Forced` 用得上（`:320-325`） |
-- | `ConquestObject.StartType` / `GameType`（`:68-69`） | `start_type` / `game_type` | 本次战争的类型与玩法 |
-- | `ConquestObject.KingPoints`（`:77`） | `king_points` | 「王者之城」计分：`行会ID:分数`，逗号分隔 |
-- | `ConquestObject.ControlPoints` 的归属部分（`:52-56`） | `flag_owners` | 控制点归属：`控制点ID:行会ID`，逗号分隔 |
-- | `ConquestInfo.StartHour` / `WarLength` / 七个星期布尔 | `schedule_start_hour` / `schedule_war_length` / `schedule_days` | 时间表快照（初值取自配表，`ConquestObject.AutoSchedule`，`:465-506`） |
-- | ——（**Crystal 没有**「下一场战争的时刻」这种字段，靠每分钟现算） | `next_war_at` | 本作扩展：缓存下一次开战时刻，便于 GM/监控看 |
--
-- ## 存档里没有的东西
--
-- `ConquestGuildInfo` 的 `ArcherList`/`GateList`/`WallList`/`SiegeList`/`FlagList`
-- （守卫/城门/城墙/器械/旗子的**血量**）没有迁移：本作还没实现这些建筑对象，
-- 见 `docs/world/conquest.md` 的「未做」。
--
-- `ConquestInfo.ConquestGuards` 的 `RepairCost` 与 `WarLength` 等单位都是配表字段，不进本表。

CREATE TABLE conquests (
    -- 攻城区主键 = Crystal `ConquestInfo.Index`（`ConquestInfo.cs:8`，也是存档文件名）。
    conquest_index      INTEGER PRIMARY KEY,
    -- 城主行会（`ConquestGuildInfo.Owner`，`-1` 落成 NULL）。行会被解散时自动清空。
    owner_guild_id      INTEGER REFERENCES guilds (id) ON DELETE SET NULL,
    -- 攻打方（`ConquestGuildInfo.AttackerID`，`-1` 落成 NULL）。
    attacker_guild_id   INTEGER REFERENCES guilds (id) ON DELETE SET NULL,
    -- 城主税收池（`ConquestGuildInfo.GoldStorage`，C# 是 uint）。
    gold_storage        BIGINT  NOT NULL DEFAULT 0,
    -- 城主设置的加价百分比（`ConquestGuildInfo.NPCRate`，C# 是 byte）。
    npc_rate            SMALLINT NOT NULL DEFAULT 0,
    -- 是否正在开战（`ConquestObject.WarIsOn`）。
    war_is_on           BOOLEAN NOT NULL DEFAULT FALSE,
    -- 本场战争的起止时刻（`ConquestObject.WarStartTime` / `WarEndTime`）。
    war_start_time      TIMESTAMPTZ,
    war_end_time        TIMESTAMPTZ,
    -- `ConquestType`（`Shared/Enums.cs:1881-1886`）：0 Request / 1 Auto / 2 Forced。
    start_type          SMALLINT NOT NULL DEFAULT 0,
    -- `ConquestGame`（`Shared/Enums.cs:1888-1895`）：0 CapturePalace / 1 KingOfHill /
    -- 2 Random / 3 Classic / 4 ControlPoints。
    game_type           SMALLINT NOT NULL DEFAULT 0,
    -- 时间表快照（`ConquestInfo.StartHour` / `WarLength` / 星期布尔掩码，周一位 0）。
    schedule_start_hour SMALLINT NOT NULL DEFAULT 0,
    schedule_war_length INTEGER NOT NULL DEFAULT 60,
    schedule_days       SMALLINT NOT NULL DEFAULT 0,
    -- 「王者之城」计分（`ConquestObject.KingPoints`，`:77`）：`行会ID:分数` 逗号分隔。
    king_points         TEXT NOT NULL DEFAULT '',
    -- 控制点归属（`ConquestObject.ControlPoints` 的归属部分，`:52-56`）：`控制点ID:行会ID` 逗号分隔。
    flag_owners         TEXT NOT NULL DEFAULT '',
    -- 本作扩展：下一次自动开战时刻（Crystal 每分钟现算 `AutoSchedule`，不落盘）。
    next_war_at         TIMESTAMPTZ,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- 上界照抄 C# 类型：`GoldStorage` 是 uint、`NPCRate` 是 byte（`ConquestGuildInfo.cs:18-20`）。
    CONSTRAINT conquests_gold_storage_uint
        CHECK (gold_storage >= 0 AND gold_storage <= 4294967295),
    CONSTRAINT conquests_npc_rate_byte
        CHECK (npc_rate >= 0 AND npc_rate <= 255)
);

-- 城主查行会（`Envir.LoadConquests` 的反向绑定，`Envir.cs:3147-3152`：按 Owner 找行会）。
CREATE INDEX conquests_owner_guild_id_idx ON conquests (owner_guild_id);
