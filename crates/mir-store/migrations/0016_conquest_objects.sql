-- 0016_conquest_objects.sql — 攻城器械耐久持久化（本项目扩展）。
--
-- Crystal 把城门/城墙/箭塔的耐久放在 `ConquestGuildGateInfo` 等结构里，随
-- `Conquests/<index>.mcd` 存档；本作拆成独立表，一行一件器械。
-- 静态布点（坐标/类型）仍来自 `Server.MirDB` 的 ConquestInfo，本表只存运行期状态。

CREATE TABLE conquest_objects (
    conquest_index  INTEGER NOT NULL REFERENCES conquests (conquest_index) ON DELETE CASCADE,
    -- 0 Gate / 1 Wall / 2 Archer（与 S.ConquestObject.kind 一致）。
    kind            SMALLINT NOT NULL,
    x               INTEGER NOT NULL,
    y               INTEGER NOT NULL,
    -- MirDirection 数值（0..7）。
    direction       SMALLINT NOT NULL DEFAULT 0,
    hp              INTEGER NOT NULL,
    max_hp          INTEGER NOT NULL,
    -- 归属行会；NULL = 无主（C# 的 -1）。
    owner_guild_id  INTEGER REFERENCES guilds (id) ON DELETE SET NULL,
    destroyed       BOOLEAN NOT NULL DEFAULT FALSE,
    -- 城门开关状态（本项目扩展；Wall/Archer 恒 false）。
    open            BOOLEAN NOT NULL DEFAULT FALSE,

    PRIMARY KEY (conquest_index, kind, x, y),
    CONSTRAINT conquest_objects_kind_check CHECK (kind BETWEEN 0 AND 2),
    CONSTRAINT conquest_objects_direction_check CHECK (direction BETWEEN 0 AND 7),
    CONSTRAINT conquest_objects_hp_check CHECK (hp >= 0 AND max_hp >= 0 AND hp <= max_hp)
);

CREATE INDEX conquest_objects_conquest_index_idx ON conquest_objects (conquest_index);
