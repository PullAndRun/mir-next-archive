-- 0006_guild_storage.sql — 行会仓库与行会增益（guild buff）。
--
-- 对应 Crystal `GuildInfo` 里两块此前没有迁移的状态（`Server/MirDatabase/GuildInfo.cs`）：
--
-- | Crystal | 本迁移 |
-- |---|---|
-- | `GuildStorageItem[] StoredItems = new GuildStorageItem[112]`（`:22`，存档 `:224-233`） | `guild_storage` |
-- | `List<GuildBuff> BuffList`（`:23`，存档 `:235-239`） | `guild_buffs` |
--
-- 仓库格子的 `item` 列是 **`crates/mir-server/src/items.rs` 的 `encode_slots` 字节块**：
-- 里面是「1 个槽位 + `UserItem`」，即 `encode_slots(&[Some(item)])`。之所以复用整块格式
-- 而不是单写一个 `UserItem`，是为了和角色背包/装备（`characters.inventory` / `equipment`）
-- 用同一套编解码与同一套上限校验（`decode_slots` / `ItemCodecError`）。
--
-- `stored_at` 是**本作扩展**：Crystal 的 `GuildStorageItem` 只有 `Item` 与 `UserId`
-- （`Shared/Data/GuildData.cs:44-60`），没有存放时间。加这一列只用于审计/排序，
-- 运行期不参与任何判定。
--
-- ## 关于「行会仓库金币」`storage_gold`
--
-- Crystal **没有**独立的仓库金币字段：`GuildInfo.Gold` 既是行会资金也是仓库金币，
-- `PlayerObject.GuildStorageGoldChange` 的存入/取出直接加减 `MyGuild.Gold`
-- （`Server/MirObjects/PlayerObject.cs:10075-10119`），行会 buff 激活也从同一份金币扣
-- （`GuildObject.cs:952-957`）。因此本迁移**不新增** `guilds.storage_gold`，
-- 直接复用 `guilds.gold`（迁移 0005），避免同一笔钱存在两份事实来源。

CREATE TABLE guild_storage (
    guild_id   INTEGER NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    -- 格子序号（Crystal 数组下标，0..=111，见 `GuildInfo.cs:22`）。
    slot       SMALLINT NOT NULL,
    -- `encode_slots(&[Some(UserItem)])` 的字节块（见文件头）。
    item       BYTEA NOT NULL,
    -- 存放者的角色主键（Crystal `GuildStorageItem.UserId`，`GuildData.cs:47`）。
    user_id    BIGINT NOT NULL,
    -- 存放时间（本作扩展，Crystal 无此字段）。
    stored_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, slot)
);

CREATE TABLE guild_buffs (
    guild_id              INTEGER NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    -- 增益编号（Crystal `GuildBuff.Id`，指向 `GuildSettings.ini` 的 `[Buff-N] Id`）。
    buff_id               INTEGER NOT NULL,
    -- `GuildBuff.Active`（`Shared/Data/GuildData.cs:221`）。
    active                BOOLEAN NOT NULL DEFAULT FALSE,
    -- `GuildBuff.ActiveTimeRemaining`（`:222`）；单位是**分钟**（客户端文案
    -- `ClientTextKeys.TimeRemainingMinutes`，`Client/MirScenes/Dialogs/GuildDialog.cs:1237`）。
    active_time_remaining INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (guild_id, buff_id)
);
