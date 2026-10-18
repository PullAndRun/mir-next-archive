-- 0007_mail.sql — 邮件（邮箱）。
--
-- 对应 Crystal `CharacterInfo.Mail`（`Server/MirDatabase/CharacterInfo.cs:85` 的
-- `List<MailInfo> Mail`；存档 `:500-503`、读入 `:288-291`）与 `MailInfo`
-- （`Server/MirDatabase/MailInfo.cs:5-167`）。
--
-- ## 字段对照
--
-- | Crystal | 本迁移 | 说明 |
-- |---|---|---|
-- | `MailID`（`MailInfo.cs:12`，`ulong`，`++Envir.NextMailID`，`PlayerObject.cs:11835`） | `id` | 全局唯一；服务器层按「毫秒时间戳 << 20」自己分配（见 `crates/mir-server/src/mail.rs`），显式写号后会推进序列 |
-- | `RecipientIndex`（`:16`，收件角色主键） | `character_id` | 外键；角色删除时级联删邮件 |
-- | `Sender`（`:14`） | `sender_name` | 发件人角色名（Crystal 只存名字，不存角色主键） |
-- | ——（**Crystal 没有主题字段**） | `subject` | 本作扩展列，恒为空串（`C.SendMail` 线格式只有 `Name`/`Message`，`Shared/ClientPackets.cs:2122-2158`） |
-- | `Message`（`:19`） | `message` | 正文 |
-- | `Gold`（`:20`，`uint`） | `gold` | 附件金币（**不含邮费**，`PlayerObject.cs:11838`） |
-- | `Items`（`:21`，`List<UserItem>`） | `items` | `encode_slots(&[Some(item), …])` 字节块（与背包/装备同一套 `UserItem[]` 编解码） |
-- | `DateSent`（`:23`） | `date_sent` | 发送时间（`Envir.Now`）；本作落 `timestamptz` |
-- | `Opened`（计算属性，`DateOpened > DateTime.MinValue`，`:30-33`） | `opened` | 阅读一次就置真（`ReadMail`，`:11853`） |
-- | `Locked`（`:35`） | `locked` | 锁定 |
-- | `Collected`（`:37`） | `collected` | 「包裹已被邮局受理」标志；发送时按 `MailAutoSendGold/Items` 决定初值（`MailInfo.cs:99-143`） |
--
-- ## 没有迁移的字段
--
-- * `DateOpened`（`MailInfo.cs:23`）：本作只保留 `opened` 布尔，不记具体阅读时间。
-- * `CanReply`（`:44`）：玩家之间发送的邮件恒为 `true`（`new MailInfo(recipientIndex, true)`，
--   `PlayerObject.cs:11717`、`:11833`）；系统邮件（`Envir.MailCharacter`）本作不做，
--   因此不需要这一列。
-- * `RecipientInfo`（`:17`）：运行期引用，Crystal 也不落盘。
--
-- ## 过期
--
-- Crystal **没有**邮件过期机制（`Settings.cs:241-247` 只有 `MailAutoSend*`/`MailFreeWithStamp`/
-- `MailCostPer1KGold`/`MailItemInsurancePercentage`/`MailCapacity`，`MailInfo` 也没有过期字段）。
-- 本作按任务要求在查询时过滤「超过 30 天」的邮件（见 `crates/mir-server/src/mail.rs` 的
-- `MAIL_EXPIRE_DAYS`，带 `// 扩展：` 说明），因此**不加列**：过期判定只用 `date_sent`。

CREATE TABLE mail (
    -- Crystal MailID（全局唯一，ulong）；显式写号由 `create_mail` 负责推进序列。
    id           BIGSERIAL   PRIMARY KEY,
    -- Crystal MailInfo.RecipientIndex：收件角色（`CharacterInfo.Mail` 是角色级列表）。
    character_id BIGINT      NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    -- Crystal MailInfo.Sender：发件人角色名（可能对应一个已删除的角色）。
    sender_name  TEXT        NOT NULL DEFAULT '',
    -- 本作扩展：主题（Crystal 无此字段，玩家邮件恒为空串）。
    subject      TEXT        NOT NULL DEFAULT '',
    -- Crystal MailInfo.Message。
    message      TEXT        NOT NULL DEFAULT '',
    -- `UserItem[]` 字节块（Crystal MailInfo.Items），空列表存零长度。
    items        BYTEA       NOT NULL DEFAULT '\x'::bytea,
    -- Crystal MailInfo.Gold（uint）：附件金币，邮费不在这里（发送时已单独扣掉）。
    gold         BIGINT      NOT NULL DEFAULT 0,
    -- Crystal MailInfo.DateSent（Envir.Now）。
    date_sent    TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Crystal MailInfo.Opened / Locked / Collected。
    opened       BOOLEAN     NOT NULL DEFAULT FALSE,
    locked       BOOLEAN     NOT NULL DEFAULT FALSE,
    collected    BOOLEAN     NOT NULL DEFAULT FALSE,

    -- Crystal 的 Gold 是 uint（`MailInfo.cs:20`），落 BIGINT 只为了不与 Rust 侧 i64 反复转换；
    -- 上界仍按 uint 卡住，避免脏数据把客户端 u32 字段截断。
    CONSTRAINT mail_gold_uint CHECK (gold >= 0 AND gold <= 4294967295)
);

-- 收件箱查询：按角色取全部邮件，再按 MailID 升序（Crystal 的 `List<MailInfo>` 就是追加序，
-- `PlayerObject.GetMail` 取列表尾部 `MailCapacity` 条，`:11959-11988`）。
CREATE INDEX mail_character_id_idx ON mail (character_id, id);
