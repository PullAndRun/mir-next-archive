-- 0015_guild_allies_and_friends.sql — guild alliances + character friends/block list.
--
-- Crystal `GuildInfo.AllyGuilds` (`GuildObject.cs:63`) and `CharacterInfo.Friends`
-- (`CharacterInfo.cs:86`; `FriendInfo` at `:712`: int32 index + bool blocked + string memo).
-- This project stores both as compact blobs on the owning row (same style as `storage`):
--   guilds.allies      : int32 count + int32 guild ids
--   characters.friends : int32 count + (int32 character_id, bool blocked, string memo)
-- Empty blob = no allies / no friends.

ALTER TABLE guilds
    ADD COLUMN allies BYTEA NOT NULL DEFAULT '\x'::bytea;

ALTER TABLE characters
    ADD COLUMN friends BYTEA NOT NULL DEFAULT '\x'::bytea;
