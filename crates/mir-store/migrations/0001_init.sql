-- 0001_init.sql — 账号与角色的 MVP 表结构。
--
-- 字段对应 Crystal 的 Server/MirDatabase/AccountInfo.cs 与 CharacterInfo.cs，
-- 语义说明见 docs/store/schema.md。与 Crystal 的差异以列为单位标注：
--
--   * 时间统一用 timestamptz；Crystal 的 DateTime.MinValue（二进制 0）在这里写成 NULL。
--   * 物品/装备/仓库在 MVP 里以 BYTEA 整块保存（Crystal 的 UserItem[] 二进制），
--     后续拆成 user_items 表时只需迁移这三列，不影响账号/角色行的语义。
--   * Crystal 的账号名与角色名判定都是 OrdinalIgnoreCase（Envir.cs:4124、Envir.cs:4133），
--     因此唯一性用 lower(...) 唯一索引表达。

CREATE TABLE accounts (
    -- Crystal AccountInfo.Index（NextAccountID 单调分配）；bigserial 保持同样的单调性。
    id                      BIGSERIAL   PRIMARY KEY,
    -- Crystal AccountInfo.AccountID，登录名；唯一性不区分大小写。
    username                TEXT        NOT NULL,
    -- Crystal AccountInfo.password：PBKDF2-HMAC-SHA1 结果的 UTF-8 替换解码串（见 src/password.rs）。
    password_hash           TEXT        NOT NULL,
    -- Crystal AccountInfo.Salt，固定 24 字节。
    salt                    BYTEA       NOT NULL,
    -- Crystal AccountInfo.EMailAddress。
    email                   TEXT        NOT NULL DEFAULT '',
    -- Crystal AccountInfo.CreationIP / CreationDate。
    creation_ip             TEXT        NOT NULL DEFAULT '',
    creation_date           TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Crystal AccountInfo.Banned / BanReason / ExpiryDate（NULL = 无到期时间，即永久）。
    banned                  BOOLEAN     NOT NULL DEFAULT FALSE,
    ban_reason              TEXT        NOT NULL DEFAULT '',
    expiry_date             TIMESTAMPTZ,
    -- Crystal AccountInfo.RequirePasswordChange：命中后只允许走改密流程（Envir.cs:3905-3909）。
    require_password_change BOOLEAN     NOT NULL DEFAULT FALSE,
    -- Crystal AccountInfo.WrongPasswordCount（原版不落盘，重启清零；这里落库，见 schema.md §7.4）。
    wrong_password_count    INTEGER     NOT NULL DEFAULT 0,
    -- Crystal AccountInfo.LastIP / LastDate（最后登录）。
    last_ip                 TEXT        NOT NULL DEFAULT '',
    last_date               TIMESTAMPTZ,
    -- Crystal AccountInfo.AdminAccount。
    admin                   BOOLEAN     NOT NULL DEFAULT FALSE,

    CONSTRAINT accounts_salt_length CHECK (octet_length(salt) = 24)
);

CREATE UNIQUE INDEX accounts_username_ci_key ON accounts (lower(username));

CREATE TABLE characters (
    -- Crystal CharacterInfo.Index（NextCharacterID 单调分配）。
    id                  BIGSERIAL   PRIMARY KEY,
    -- Crystal CharacterInfo.AccountInfo：角色归属账号，账号删除时级联删除角色。
    account_id          BIGINT      NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    -- Crystal CharacterInfo.Name；唯一性不区分大小写。
    name                TEXT        NOT NULL,
    -- Crystal CharacterInfo.Level（ushort）、Class / Gender / Hair（枚举与 byte）。
    level               SMALLINT    NOT NULL DEFAULT 1,
    class               SMALLINT    NOT NULL DEFAULT 0,
    gender              SMALLINT    NOT NULL DEFAULT 0,
    hair                SMALLINT    NOT NULL DEFAULT 0,
    -- Crystal CharacterInfo.CreationIP / CreationDate。
    creation_ip         TEXT        NOT NULL DEFAULT '',
    creation_date       TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Crystal CharacterInfo.Banned / BanReason / ExpiryDate。
    banned              BOOLEAN     NOT NULL DEFAULT FALSE,
    ban_reason          TEXT        NOT NULL DEFAULT '',
    expiry_date         TIMESTAMPTZ,
    -- Crystal CharacterInfo.LastIP / LastLoginDate / LastLogoutDate。
    last_ip             TEXT        NOT NULL DEFAULT '',
    last_login_date     TIMESTAMPTZ,
    last_logout_date    TIMESTAMPTZ,
    -- Crystal CharacterInfo.Deleted / DeleteDate：软删除，Crystal 会保留一个月再归档。
    deleted             BOOLEAN     NOT NULL DEFAULT FALSE,
    delete_date         TIMESTAMPTZ,
    -- Crystal CharacterInfo.CurrentMapIndex / CurrentLocation / Direction。
    current_map_index   INTEGER     NOT NULL DEFAULT 0,
    current_x           INTEGER     NOT NULL DEFAULT 0,
    current_y           INTEGER     NOT NULL DEFAULT 0,
    direction           SMALLINT    NOT NULL DEFAULT 0,
    -- Crystal CharacterInfo.BindMapIndex / BindLocation（回城点）。
    bind_map_index      INTEGER     NOT NULL DEFAULT 0,
    bind_x              INTEGER     NOT NULL DEFAULT 0,
    bind_y              INTEGER     NOT NULL DEFAULT 0,
    -- Crystal CharacterInfo.HP / MP / Experience。
    hp                  INTEGER     NOT NULL DEFAULT 0,
    mp                  INTEGER     NOT NULL DEFAULT 0,
    experience          BIGINT      NOT NULL DEFAULT 0,
    -- 任务要求放在角色行的货币；Crystal 的 Gold/Credit 实际挂在 AccountInfo 上（见 crate 文档）。
    gold                BIGINT      NOT NULL DEFAULT 0,
    credit              BIGINT      NOT NULL DEFAULT 0,
    -- Crystal CharacterInfo.AMode / PMode / PKPoints。
    attack_mode         SMALLINT    NOT NULL DEFAULT 0,
    pet_mode            SMALLINT    NOT NULL DEFAULT 0,
    pk_points           INTEGER     NOT NULL DEFAULT 0,
    -- Crystal CharacterInfo.AllowGroup / AllowTrade / AllowObserve。
    allow_group         BOOLEAN     NOT NULL DEFAULT FALSE,
    allow_trade         BOOLEAN     NOT NULL DEFAULT FALSE,
    allow_observe       BOOLEAN     NOT NULL DEFAULT FALSE,
    -- Crystal CharacterInfo.GuildIndex（-1 = 无行会）。
    guild_index         INTEGER     NOT NULL DEFAULT -1,
    -- Crystal CharacterInfo.Inventory / Equipment，以及任务要求同行的仓库（Crystal 挂在账号上）。
    -- MVP 里是 Crystal UserItem[] 的二进制整块，空数组用零长度 BYTEA 表示。
    inventory           BYTEA       NOT NULL DEFAULT '\x'::bytea,
    equipment           BYTEA       NOT NULL DEFAULT '\x'::bytea,
    storage             BYTEA       NOT NULL DEFAULT '\x'::bytea,
    -- Crystal CharacterInfo.Married / MarriedDate / Mentor / MentorDate / IsMentor / MentorExp。
    married             INTEGER     NOT NULL DEFAULT 0,
    married_date        TIMESTAMPTZ,
    mentor              INTEGER     NOT NULL DEFAULT 0,
    mentor_date         TIMESTAMPTZ,
    is_mentor           BOOLEAN     NOT NULL DEFAULT FALSE,
    mentor_exp          BIGINT      NOT NULL DEFAULT 0
);

-- 角色名全局唯一（不区分大小写），对应 Crystal CharacterExists（Envir.cs:4130-4136）。
CREATE UNIQUE INDEX characters_name_ci_key ON characters (lower(name));
-- 选角列表按账号取角色（AND NOT deleted）。
CREATE INDEX characters_account_id_idx ON characters (account_id);
