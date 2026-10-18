# `TextKey` 对照表（§7.2 → 键名 → 变体 → 中英模板）

> 本文件由 `py tools/i18n/verify_text_keys.py --report tools/i18n/textkeys-report.md` 生成，请勿手改。
> 核对对象：`docs/i18n.md` §7.2（收口清单）、`crates/mir-core/src/i18n.rs`、`config/lang/{zh-cn,en}.json`。

## 统计

| 项 | 数量 |
| --- | --- |
| §7.2 表格行 | 192 |
| §7.2 唯一键 | 185（7 行是重复出现的键） |
| 第 1 轮无参键 | 11 |
| 扩展键（行会权限，不在 §7.2 内） | 4 |
| `TextKey` 变体 / `ALL` | 200 / 200 |
| 带参数的键 | 78 |
| 语言包 server 键（zh / en） | 200 / 200 |

核对结果：**PASS**（§7.2 192 行 → 185 键；加 11 个第 1 轮键与 4 个扩展键共 200 个 `TextKey`；无漏键、无孤儿键、中英占位一致）。

## 重复出现的键（192 行 → 185 键）

同一键名在 §7.2 里出现多次（同一句提示在多个分支复用），实现里合并为一个变体。

| 键名 | 出现次数 | §7.2 行 |
| --- | --- | --- |
| `item_potion_no_effect_full` | 2 | `items.rs:1867`、`items.rs:1889` |
| `market_label_sold` | 2 | `market.rs:544`、`market.rs:558` |
| `market_label_expired` | 2 | `market.rs:546`、`market.rs:560` |
| `trade_already_trading` | 3 | `trade.rs:321`、`trade.rs:375`、`trade.rs:472` |
| `trade_not_facing` | 3 | `trade.rs:337`、`trade.rs:343`、`trade.rs:392` |

## §7.2 逐行对照（192 行）

| §7.2 行 | 键名 | 变体 | zh 模板 | en 模板 |
| --- | --- | --- | --- | --- |
| `conquest.rs:390` | `conquest_not_in_world` | `ConquestNotInWorld` | `对象 {object_id} 不在世界里` | `Object {object_id} is not in the world.` |
| `conquest.rs:393` | `conquest_not_in_guild` | `ConquestNotInGuild` | `你不在行会中` | `You are not in a guild.` |
| `conquest.rs:394` | `conquest_not_leader` | `ConquestNotLeader` | `只有会长才能申请攻城` | `Only the guild leader can request a siege.` |
| `conquest.rs:397` | `conquest_area_not_found` | `ConquestAreaNotFound` | `找不到攻城区域 {area}` | `Conquest area {area} could not be found.` |
| `conquest.rs:400` | `conquest_already_owner` | `ConquestAlreadyOwner` | `你的行会已经是这座城的城主` | `Your guild already owns this castle.` |
| `conquest.rs:401` | `conquest_at_war` | `ConquestAtWar` | `这座城正在开战，无法申请` | `This castle is at war; you cannot apply for it.` |
| `conquest.rs:402` | `conquest_already_scheduled` | `ConquestAlreadyScheduled` | `已经有一个行会申请了这座城的攻城` | `Another guild has already applied to lay siege to this castle.` |
| `conquest.rs:405` | `conquest_level_too_low` | `ConquestLevelTooLow` | `申请攻城需要行会等级 {required}（当前 {level}）` | `A siege request requires guild level {required} (currently {level}).` |
| `conquest.rs:413` | `conquest_not_enough_gold` | `ConquestNotEnoughGold` | `申请攻城需要 {required} 金币（行会现有 {gold}）` | `A siege request costs {required} Gold (the guild has {gold}).` |
| `conquest.rs:419` | `conquest_not_owner_guild` | `ConquestNotOwnerGuild` | `只有城主行会才能执行这个操作` | `Only the owning guild can do that.` |
| `conquest.rs:1176` | `conquest_war_started` | `ConquestWarStarted` | `【攻城】{name} 攻城战已经开始！` | `[Siege] The siege of {name} has begun!` |
| `conquest.rs:1225` | `conquest_war_ended` | `ConquestWarEnded` | `【攻城】{name} 攻城战已经结束。` | `[Siege] The siege of {name} has ended.` |
| `conquest.rs:1485` | `conquest_no_guild` | `ConquestNoGuild` | `（无行会）` | `(No guild)` |
| `conquest.rs:1488` | `conquest_point_captured` | `ConquestPointCaptured` | `【攻城】{guild} 占领了控制点 {point}。` | `[Siege] {guild} has captured control point {point}.` |
| `conquest.rs:1582` | `conquest_new_owner` | `ConquestNewOwner` | `【攻城】{conquest} 的新城主是 {guild}。` | `[Siege] The new owner of {conquest} is {guild}.` |
| `group.rs:487` | `group_invite_not_leader` | `GroupInviteNotLeader` | `你不是队长，不能邀请队员。` | `You are not the group leader and cannot invite members.` |
| `group.rs:493` | `group_invite_full` | `GroupInviteFull` | `你的队伍人数已满。` | `Your group is full.` |
| `group.rs:501` | `group_invite_map_forbidden` | `GroupInviteMapForbidden` | `当前地图禁止组队，无法邀请。` | `Grouping is not allowed on this map; you cannot invite.` |
| `group.rs:512` | `group_invite_target_missing` | `GroupInviteTargetMissing` | `找不到玩家 {name}。` | `Player {name} could not be found.` |
| `group.rs:524` | `group_invite_self` | `GroupInviteSelf` | `不能邀请自己加入队伍。` | `You cannot invite yourself to a group.` |
| `group.rs:535` | `group_invite_target_disallowed` | `GroupInviteTargetDisallowed` | `{target_name} 不允许组队。` | `{target_name} does not allow grouping.` |
| `group.rs:547` | `group_invite_target_in_group` | `GroupInviteTargetInGroup` | `{target_name} 已经在队伍中。` | `{target_name} is already in a group.` |
| `group.rs:569` | `group_invite_target_busy` | `GroupInviteTargetBusy` | `{target_name} 正在接收其他人的组队邀请。` | `{target_name} is already receiving a group invitation from someone else.` |
| `group.rs:582` | `group_invite_target_map_forbidden` | `GroupInviteTargetMapForbidden` | `{target_name} 所在的地图禁止组队。` | `{target_name} is on a map where grouping is not allowed.` |
| `group.rs:588` | `group_join_map_forbidden` | `GroupJoinMapForbidden` | `当前地图禁止组队，无法接受邀请。` | `Grouping is not allowed on this map; you cannot accept the invitation.` |
| `group.rs:637` | `group_join_no_invite` | `GroupJoinNoInvite` | `你没有收到组队邀请。` | `You have not received a group invitation.` |
| `group.rs:651` | `group_join_declined` | `GroupJoinDeclined` | `{invitee_name} 拒绝了你的组队邀请。` | `{invitee_name} declined your group invitation.` |
| `group.rs:664` | `group_join_expired` | `GroupJoinExpired` | `组队邀请已过期。` | `The group invitation has expired.` |
| `group.rs:681` | `group_join_already_in_group` | `GroupJoinAlreadyInGroup` | `你已经在队伍中，无法加入 {inviter_name} 的队伍。` | `You are already in a group and cannot join {inviter_name}'s group.` |
| `group.rs:693` | `group_join_inviter_offline` | `GroupJoinInviterOffline` | `邀请者已经下线。` | `The inviter has gone offline.` |
| `group.rs:711` | `group_join_inviter_not_leader` | `GroupJoinInviterNotLeader` | `{inviter_name} 已经不是队长了。` | `{inviter_name} is no longer the group leader.` |
| `group.rs:724` | `group_join_inviter_full` | `GroupJoinInviterFull` | `{inviter_name} 的队伍人数已满。` | `{inviter_name}'s group is full.` |
| `group.rs:738` | `group_join_inviter_disallowed` | `GroupJoinInviterDisallowed` | `{inviter_name} 不再允许组队。` | `{inviter_name} no longer allows grouping.` |
| `group.rs:751` | `group_join_inviter_map_forbidden` | `GroupJoinInviterMapForbidden` | `{inviter_name} 在地图禁止组队的区域。` | `{inviter_name} is on a map where grouping is not allowed.` |
| `group.rs:757` | `group_map_forbidden` | `GroupMapForbidden` | `当前地图禁止组队。` | `Grouping is not allowed on this map.` |
| `group.rs:935` | `group_kick_not_in_group` | `GroupKickNotInGroup` | `你不在队伍中。` | `You are not in a group.` |
| `group.rs:943` | `group_kick_not_leader` | `GroupKickNotLeader` | `你不是队长，不能移除队员。` | `You are not the group leader and cannot remove members.` |
| `group.rs:956` | `group_kick_not_member` | `GroupKickNotMember` | `{name} 不在你的队伍中。` | `{name} is not in your group.` |
| `guild.rs:1202` | `guild_not_in_world` | `GuildNotInWorld` | `对象 {object_id} 不在世界里` | `Object {object_id} is not in the world.` |
| `guild.rs:1205` | `guild_not_in_guild` | `GuildNotInGuild` | `你不在行会中` | `You are not in a guild.` |
| `guild.rs:1206` | `guild_already_in_guild` | `GuildAlreadyInGuild` | `你已经在行会中` | `You are already in a guild.` |
| `guild.rs:1208` | `guild_name_taken` | `GuildNameTaken` | `行会 {name} 已经存在` | `Guild {name} already exists.` |
| `guild.rs:1212` | `guild_name_length` | `GuildNameLength` | `行会名长度必须是 {min}-{max} 个字符` | `Guild names must be {min}-{max} characters long.` |
| `guild.rs:1218` | `guild_name_invalid` | `GuildNameInvalid` | `行会名不能包含反斜杠或换行` | `Guild names cannot contain backslashes or line breaks.` |
| `guild.rs:1221` | `guild_create_level_too_low` | `GuildCreateLevelTooLow` | `创建行会需要 {required} 级（你当前 {level} 级）` | `Creating a guild requires level {required} (you are level {level}).` |
| `guild.rs:1229` | `guild_create_not_enough_gold` | `GuildCreateNotEnoughGold` | `创建行会需要 {required} 金币（你只有 {gold}）` | `Creating a guild costs {required} Gold (you only have {gold}).` |
| `guild.rs:1252` | `guild_no_permission` | `GuildNoPermission` | `你的职位没有 {permission} 权限` | `Your rank does not have permission to {permission}.` |
| `guild.rs:1259` | `guild_member_not_found` | `GuildMemberNotFound` | `行会里没有成员 {name}` | `There is no member named {name} in the guild.` |
| `guild.rs:1263` | `guild_target_already_in_guild` | `GuildTargetAlreadyInGuild` | `{name} 已经在行会中` | `{name} is already in a guild.` |
| `guild.rs:1268` | `guild_target_already_invited` | `GuildTargetAlreadyInvited` | `{name} 已经收到一个行会邀请` | `{name} already has a pending guild invitation.` |
| `guild.rs:1271` | `guild_cannot_invite_self` | `GuildCannotInviteSelf` | `不能邀请自己` | `You cannot invite yourself.` |
| `guild.rs:1272` | `guild_full` | `GuildFull` | `行会人数已满` | `The guild is full.` |
| `guild.rs:1273` | `guild_no_invitation` | `GuildNoInvitation` | `没有待接受的行会邀请` | `You have no pending guild invitation.` |
| `guild.rs:1276` | `guild_leader_level_too_low` | `GuildLeaderLevelTooLow` | `对方等级不足以成为会长（需要 {required} 级）` | `That player's level is too low to become leader (requires level {required}).` |
| `guild.rs:1279` | `guild_needs_two_leaders` | `GuildNeedsTwoLeaders` | `行会至少要有两个会长才能降职` | `The guild needs at least two leaders before one can be demoted.` |
| `guild.rs:1281` | `guild_need_one_leader_online` | `GuildNeedOneLeaderOnline` | `需要至少一个会长在线` | `At least one leader must be online.` |
| `guild.rs:1285` | `guild_rank_not_found` | `GuildRankNotFound` | `职位 {rank} 不存在` | `Rank {rank} does not exist.` |
| `guild.rs:1290` | `guild_rank_name_too_short` | `GuildRankNameTooShort` | `职位名至少 {min} 个字符` | `Rank names must be at least {min} characters long.` |
| `guild.rs:1295` | `guild_rank_name_too_long` | `GuildRankNameTooLong` | `职位名最多 {max} 个字符` | `Rank names can be at most {max} characters long.` |
| `guild.rs:1299` | `guild_cannot_change_own_rank` | `GuildCannotChangeOwnRank` | `不能修改自己职位的权限` | `You cannot change the permissions of your own rank.` |
| `guild.rs:1301` | `guild_too_many_ranks` | `GuildTooManyRanks` | `职位数量已达上限` | `The maximum number of ranks has been reached.` |
| `guild.rs:1302` | `guild_rank_not_adequate` | `GuildRankNotAdequate` | `你的职位不够高` | `Your rank is not high enough.` |
| `guild.rs:1305` | `guild_notice_too_long` | `GuildNoticeTooLong` | `公告最多 {lines} 行` | `The guild notice can be at most {lines} line(s) long.` |
| `guild.rs:1308` | `guild_storage_outside_safezone` | `GuildStorageOutsideSafezone` | `只能在安全区内使用行会仓库` | `The guild storage can only be used in a safe zone.` |
| `guild.rs:1310` | `guild_storage_no_store_permission` | `GuildStorageNoStorePermission` | `你的职位没有往行会仓库存物品的权限` | `Your rank does not have permission to store items in the guild storage.` |
| `guild.rs:1313` | `guild_storage_no_retrieve_permission` | `GuildStorageNoRetrievePermission` | `你的职位没有从行会仓库取物品的权限` | `Your rank does not have permission to take items from the guild storage.` |
| `guild.rs:1316` | `guild_storage_no_move_permission` | `GuildStorageNoMovePermission` | `你的职位没有整理行会仓库的权限` | `Your rank does not have permission to rearrange the guild storage.` |
| `guild.rs:1319` | `guild_storage_invalid_slot` | `GuildStorageInvalidSlot` | `格子序号不在范围内` | `That slot number is out of range.` |
| `guild.rs:1321` | `guild_storage_empty_slot` | `GuildStorageEmptySlot` | `格子里没有物品` | `There is no item in that slot.` |
| `guild.rs:1323` | `guild_storage_target_slot_not_empty` | `GuildStorageTargetSlotNotEmpty` | `目标格子不是空的` | `The target slot is not empty.` |
| `guild.rs:1326` | `guild_storage_throttled` | `GuildStorageThrottled` | `仓库列表请求过于频繁` | `Storage list requests are being sent too quickly.` |
| `guild.rs:1329` | `guild_storage_item_cannot_store` | `GuildStorageItemCannotStore` | `这件物品不能存入行会仓库` | `This item cannot be stored in the guild storage.` |
| `guild.rs:1332` | `guild_gold_limit_reached` | `GuildGoldLimitReached` | `行会资金已达上限` | `The guild funds have reached their limit.` |
| `guild.rs:1335` | `guild_player_gold_limit_reached` | `GuildPlayerGoldLimitReached` | `你的金币已达上限` | `You have reached the Gold limit.` |
| `guild.rs:1339` | `guild_bank_not_enough_gold` | `GuildBankNotEnoughGold` | `行会资金不足（需要 {required}，现有 {gold}）` | `Not enough guild funds (requires {required}, available {gold}).` |
| `guild.rs:1345` | `guild_bank_leader_only` | `GuildBankLeaderOnly` | `只有会长可以从行会仓库取金币` | `Only the guild leader can withdraw Gold from the guild storage.` |
| `guild.rs:1346` | `guild_buff_no_permission` | `GuildBuffNoPermission` | `你的职位没有激活行会增益的权限` | `Your rank does not have permission to activate guild buffs.` |
| `guild.rs:1349` | `guild_buff_not_exist` | `GuildBuffNotExist` | `行会增益 {buff} 不存在` | `Guild buff {buff} does not exist.` |
| `guild.rs:1354` | `guild_buff_already_obtained` | `GuildBuffAlreadyObtained` | `行会已经拥有增益 {buff}` | `The guild already has buff {buff}.` |
| `guild.rs:1359` | `guild_buff_not_obtained` | `GuildBuffNotObtained` | `行会还没有购买增益 {buff}` | `The guild has not purchased buff {buff} yet.` |
| `guild.rs:1364` | `guild_level_too_low` | `GuildLevelTooLow` | `行会等级不足（需要 {required} 级，当前 {level} 级）` | `Guild level too low (requires level {required}, currently level {level}).` |
| `guild.rs:1375` | `guild_points_insufficient` | `GuildPointsInsufficient` | `行会升级点不足（需要 {required} 点，现有 {remaining} 点）` | `Not enough guild upgrade points (requires {required}, available {remaining}).` |
| `guild.rs:1382` | `guild_last_leader_cannot_leave` | `GuildLastLeaderCannotLeave` | `你是最后一个会长：请先转让会长或解散行会` | `You are the last leader: transfer leadership or disband the guild first.` |
| `guild.rs:1666` | `guild_removed` | `GuildRemoved` | `你已经被移出行会。` | `You have been removed from your guild.` |
| `guild.rs:2286` | `guild_disbanded` | `GuildDisbanded` | `你已经解散了行会。` | `You have disbanded the guild.` |
| `guild.rs:2342` | `guild_left` | `GuildLeft` | `你离开了行会。` | `You have left the guild.` |
| `guild.rs:2344` | `guild_kicked` | `GuildKicked` | `你被移出了行会。` | `You have been kicked out of the guild.` |
| `guild_buffs.rs:280` | `guild_buff_bought_points_gold` | `GuildBuffBoughtPointsGold` | `「{player}」花费 {points} 升级点与 {gold} 金币为行会购买了增益「{buff}」。` | `{player} purchased guild buff "{buff}", spending {points} points and {gold} Gold.` |
| `guild_buffs.rs:281` | `guild_buff_bought_gold` | `GuildBuffBoughtGold` | `「{player}」花费 {gold} 金币为行会购买了增益「{buff}」。` | `{player} purchased guild buff "{buff}", spending {gold} Gold.` |
| `guild_buffs.rs:280` | `guild_buff_bought_points` | `GuildBuffBoughtPoints` | `「{player}」花费 {points} 升级点为行会购买了增益「{buff}」。` | `{player} purchased guild buff "{buff}", spending {points} points.` |
| `guild_buffs.rs:283` | `guild_buff_bought_free` | `GuildBuffBoughtFree` | `「{player}」为行会购买了增益「{buff}」。` | `{player} purchased guild buff "{buff}" for the guild.` |
| `items.rs:1163` | `item_unknown_placeholder` | `ItemUnknownPlaceholder` | `<未知物品 {item_index}>` | `<Unknown item {item_index}>` |
| `items.rs:1792` | `item_effect_not_implemented` | `ItemEffectNotImplemented` | `「{item}」的效果尚未实现，物品没有被消耗。` | `The effect of "{item}" is not implemented yet, so the item was not consumed.` |
| `items.rs:1810` | `item_mount_not_implemented` | `ItemMountNotImplemented` | `本作还不能骑乘，食物暂时用不上。` | `You cannot mount yet, so food is useless for now.` |
| `items.rs:1867` | `item_potion_no_effect_full` | `ItemPotionNoEffectFull` | `生命和魔法都是满的，药水没有效果。` | `Health and Mana are already full; the potion has no effect.` |
| `items.rs:1889` | `item_potion_no_effect_full` | `ItemPotionNoEffectFull` | `生命和魔法都是满的，药水没有效果。` | `Health and Mana are already full; the potion has no effect.` |
| `items.rs:1908` | `item_curse_lifted` | `ItemCurseLifted` | `你已经可以卸下诅咒装备了。` | `You can already unequip cursed equipment.` |
| `items.rs:1914` | `item_curse_lifted_now` | `ItemCurseLiftedNow` | `你现在可以卸下诅咒装备了。` | `You can now unequip cursed equipment.` |
| `items.rs:1979` | `item_wrong_gender` | `ItemWrongGender` | `你的性别无法使用这件物品。` | `Your gender cannot use this item.` |
| `items.rs:1984` | `item_wrong_class` | `ItemWrongClass` | `你的职业无法使用这件物品。` | `Your class cannot use this item.` |
| `items.rs:1995` | `item_level_too_low` | `ItemLevelTooLow` | `等级不够：需要 {required} 级。` | `Level requirement not met: requires level {required}.` |
| `items.rs:2007` | `item_level_too_high` | `ItemLevelTooHigh` | `等级过高：最多 {max} 级可用。` | `Level too high: usable up to level {max}.` |
| `items.rs:2016` | `item_revive_not_dead` | `ItemReviveNotDead` | `你还没有死亡，不能使用复活卷轴。` | `You are not dead yet, so you cannot use a revival scroll.` |
| `magic.rs:960` | `magic_already_learned` | `MagicAlreadyLearned` | `你已经学会了「{spell}」，不需要再学一次。` | `You have already learned "{spell}" and do not need to learn it again.` |
| `magic.rs:2184` | `magic_firewall_limit` | `MagicFirewallLimit` | `火墙数量已达上限，请稍后再试。` | `The maximum number of Fire Walls has been reached. Please try again later.` |
| `magic.rs:2473` | `magic_teleport_forbidden` | `MagicTeleportForbidden` | `这张地图不能使用瞬息移动。` | `Teleport cannot be used on this map.` |
| `magic.rs:2664` | `magic_summon_forbidden` | `MagicSummonForbidden` | `这张地图不能召唤宠物。` | `Pets cannot be summoned on this map.` |
| `mail.rs:339` | `mail_not_in_world` | `MailNotInWorld` | `你不在游戏世界里。` | `You are not in the game world.` |
| `mail.rs:341` | `mail_text_too_long` | `MailTextTooLong` | `邮件正文不能超过 {max} 字。` | `The mail body cannot exceed {max} characters.` |
| `mail.rs:342` | `mail_recipient_missing` | `MailRecipientMissing` | `找不到该玩家。` | `That player could not be found.` |
| `mail.rs:343` | `mail_mailbox_full` | `MailMailboxFull` | `收件人的邮箱已满。` | `The recipient's mailbox is full.` |
| `mail.rs:345` | `mail_not_enough_gold` | `MailNotEnoughGold` | `金币不足：需要 {needed}，你只有 {held}。` | `Not enough Gold: requires {needed}, you have {held}.` |
| `mail.rs:347` | `mail_item_cannot_mail` | `MailItemCannotMail` | `这件物品不能邮寄。` | `This item cannot be mailed.` |
| `mail.rs:348` | `mail_not_found` | `MailNotFound` | `找不到这封邮件。` | `That mail could not be found.` |
| `mail.rs:349` | `mail_not_collected` | `MailNotCollected` | `请先到邮局受理包裹。` | `Please accept the parcel at the post office first.` |
| `mail.rs:350` | `mail_inventory_full` | `MailInventoryFull` | `背包已满，无法领取附件。` | `Your bag is full, so the attachment cannot be collected.` |
| `mail.rs:351` | `mail_locked` | `MailLocked` | `邮件已锁定，不能删除。` | `This mail is locked and cannot be deleted.` |
| `mail.rs:352` | `mail_uncollected_attachments` | `MailUncollectedAttachments` | `附件还没领取，不能删除。` | `The attachments have not been collected, so this mail cannot be deleted.` |
| `mail.rs:353` | `mail_attachment_corrupted` | `MailAttachmentCorrupted` | `邮件附件数据损坏。` | `The mail attachment data is corrupted.` |
| `mail.rs:531` | `mail_new_mail` | `MailNewMail` | `你有新邮件。` | `You have new mail.` |
| `mail.rs:814` | `mail_gameshop_sender` | `MailGameshopSender` | `商城` | `Shop` |
| `mail.rs:816` | `mail_gameshop_body` | `MailGameshopBody` | `感谢您购买商城商品。` | `Thank you for your purchase from the shop.` |
| `mail.rs:874` | `mail_system_sender` | `MailSystemSender` | `比奇管理员` | `Bichon Administrator` |
| `mail.rs:907` | `mail_trade_item_returned` | `MailTradeItemReturned` | `由于以下原因，物品已通过邮件寄给你：\r\n\r\n交易结束后无法放回背包。` | `For the following reason, the item was sent to you by mail:\r\n\r\nIt could not be put back into your bag after the trade ended.` |
| `market.rs:402` | `gameshop_not_in_world` | `GameshopNotInWorld` | `你不在游戏世界里。` | `You are not in the game world.` |
| `market.rs:403` | `gameshop_unavailable` | `GameshopUnavailable` | `商城暂未开放。` | `The shop is not available yet.` |
| `market.rs:404` | `gameshop_product_not_found` | `GameshopProductNotFound` | `商城里没有这件商品。` | `The shop does not carry this item.` |
| `market.rs:405` | `gameshop_invalid_quantity` | `GameshopInvalidQuantity` | `购买数量不对。` | `Invalid purchase quantity.` |
| `market.rs:407` | `gameshop_too_many_stacks` | `GameshopTooManyStacks` | `一次最多买 {max} 组物品（这次要 {stacks} 组）。` | `You can buy at most {max} stacks at a time ({stacks} requested).` |
| `market.rs:409` | `gameshop_wrong_class` | `GameshopWrongClass` | `你的职业不能购买这件商品。` | `Your class cannot buy this item.` |
| `market.rs:410` | `gameshop_wrong_level` | `GameshopWrongLevel` | `等级不够：需要 {required} 级。` | `Level requirement not met: requires level {required}.` |
| `market.rs:411` | `gameshop_out_of_stock` | `GameshopOutOfStock` | `库存不足。` | `Out of stock.` |
| `market.rs:413` | `gameshop_payment_not_allowed` | `GameshopPaymentNotAllowed` | `这件商品不能用这种方式购买。` | `This item cannot be bought this way.` |
| `market.rs:415` | `gameshop_not_enough_credit` | `GameshopNotEnoughCredit` | `元宝不足。` | `Not enough Credits.` |
| `market.rs:416` | `gameshop_not_enough_gold` | `GameshopNotEnoughGold` | `金币不足。` | `Not enough Gold.` |
| `market.rs:417` | `gameshop_unsupported_payment` | `GameshopUnsupportedPayment` | `不支持的支付方式。` | `Unsupported payment method.` |
| `market.rs:418` | `gameshop_cannot_gain` | `GameshopCannotGain` | `背包放不下。` | `Your bag is full.` |
| `market.rs:544` | `market_label_sold` | `MarketLabelSold` | `已售出` | `Sold` |
| `market.rs:546` | `market_label_expired` | `MarketLabelExpired` | `已过期` | `Expired` |
| `market.rs:548` | `market_label_on_sale` | `MarketLabelOnSale` | `出售中` | `On sale` |
| `market.rs:558` | `market_label_sold` | `MarketLabelSold` | `已售出` | `Sold` |
| `market.rs:560` | `market_label_expired` | `MarketLabelExpired` | `已过期` | `Expired` |
| `market.rs:562` | `market_label_has_bid` | `MarketLabelHasBid` | `已有出价` | `Has bids` |
| `market.rs:564` | `market_label_no_bid` | `MarketLabelNoBid` | `无人出价` | `No bids` |
| `market.rs:946` | `market_mail_bid_refund_expired` | `MarketMailBidRefundExpired` | `你在 {item} 上的出价已退回：拍卖已过期（{gold} 金币）。` | `Your bid on {item} has been refunded: the auction expired ({gold} Gold).` |
| `market.rs:951` | `market_mail_item_expired` | `MarketMailItemExpired` | `你挂出的 {item} 已过期，物品已通过邮件退回。` | `Your listing of {item} has expired; the item has been returned by mail.` |
| `market.rs:1361` | `market_mail_sold` | `MarketMailSold` | `你挂出的 {item} 已售出，收入 {gold} 金币。` | `Your listing of {item} has sold for {gold} Gold.` |
| `market.rs:1365` | `market_bought` | `MarketBought` | `你以 {gold} 金币买下了 {item}。` | `You bought {item} for {gold} Gold.` |
| `market.rs:1383` | `market_mail_outbid_refund` | `MarketMailOutbidRefund` | `你在 {item} 上的出价被超过，{gold} 金币已通过邮件退回。` | `You were outbid on {item}; {gold} Gold has been returned to you by mail.` |
| `market.rs:1406` | `market_mail_new_bid` | `MarketMailNewBid` | `有人为 {item} 出价 {bid} 金币。` | `Someone bid {bid} Gold on {item}.` |
| `market.rs:1410` | `market_bid_placed` | `MarketBidPlaced` | `你为 {item} 出价 {bid} 金币。` | `You bid {bid} Gold on {item}.` |
| `market.rs:1485` | `market_mail_auction_won` | `MarketMailAuctionWon` | `你在拍卖中赢得了 {item}，成交价 {cost} 金币。` | `You won {item} in the auction for {cost} Gold.` |
| `market.rs:1493` | `market_sold_now` | `MarketSoldNow` | `你以 {cost} 金币卖出了 {item}，扣除佣金后收入 {gold} 金币。` | `You sold {item} for {cost} Gold and received {gold} Gold after commission.` |
| `market.rs:1842` | `market_mail_item_withdrawn` | `MarketMailItemWithdrawn` | `你挂出的 {item} 已被卖家下架，{gold} 金币已通过邮件退回。` | `Your listing of {item} was withdrawn by the seller; {gold} Gold has been returned to you by mail.` |
| `market.rs:1866` | `market_mail_sold_commission` | `MarketMailSoldCommission` | `你卖出了 {item}，成交价 {cost}，扣除佣金后收入 {gold} 金币。` | `You sold {item} for {cost} and received {gold} Gold after commission.` |
| `market.rs:2097` | `gameshop_mailed_items` | `GameshopMailedItems` | `背包放不下的 {count} 份物品已寄到邮箱，请到邮局领取。` | `{count} item(s) that did not fit in your bag have been mailed to you. Please collect them at the post office.` |
| `npc.rs:1203` | `npc_guild_created` | `NpcGuildCreated` | `行会 {name} 创建成功。` | `Guild {name} has been created.` |
| `npc.rs:1329` | `npc_no_conquest_gold` | `NpcNoConquestGold` | `城里没有可收取的金币。` | `There is no Gold to collect in this castle.` |
| `npc.rs:1734` | `npc_none` | `NpcNone` | `无` | `None` |
| `npc.rs:2110` | `npc_wont_buy_type` | `NpcWontBuyType` | `这个 NPC 不收购这种物品。` | `This NPC does not buy that kind of item.` |
| `npc.rs:2209` | `npc_cannot_repair` | `NpcCannotRepair` | `这件物品无法修理。` | `This item cannot be repaired.` |
| `npc.rs:2214` | `npc_repair_type_not_allowed` | `NpcRepairTypeNotAllowed` | `这里不能修理这种物品。` | `This type of item cannot be repaired here.` |
| `quests.rs:1012` | `quest_share_needs_party` | `QuestShareNeedsParty` | `组队并且附近有队友时才能分享任务。` | `You can only share a quest while in a group with a member nearby.` |
| `quests.rs:1157` | `quest_killed_monster` | `QuestKilledMonster` | `你杀死了 {monster}` | `You killed {monster}` |
| `quests.rs:1484` | `quest_task_done_suffix` | `QuestTaskDoneSuffix` | `（已完成）` | ` (done)` |
| `quests.rs:1501` | `quest_task_kill` | `QuestTaskKill` | `击杀 {monster} {count}/{total}` | `Kill {monster} {count}/{total}` |
| `quests.rs:1528` | `quest_task_item` | `QuestTaskItem` | `收集 {item} {count}/{total}` | `Collect {item} {count}/{total}` |
| `quests.rs:1554` | `quest_task_flag` | `QuestTaskFlag` | `激活标记 {number}{done_suffix}` | `Activate flag {number}{done_suffix}` |
| `session.rs:2119` | `gameshop_store_unavailable` | `GameshopStoreUnavailable` | `商城暂时不可用，请稍后再试。` | `The shop is temporarily unavailable. Please try again later.` |
| `session.rs:2146` | `gameshop_bought_market_success` | `GameshopBoughtMarketSuccess` | `你购买了 {item} × {count}。` | `You bought {item} × {count}.` |
| `session.rs:2171` | `gameshop_rolled_back` | `GameshopRolledBack` | `商城购买失败：已退回支付的货币与物品，请稍后再试。` | `Purchase failed: your payment and items have been refunded. Please try again later.` |
| `trade.rs:321` | `trade_already_trading` | `TradeAlreadyTrading` | `你已经在交易中。` | `You are already trading.` |
| `trade.rs:337` | `trade_not_facing` | `TradeNotFacing` | `请面对你要交易的对象。` | `Please face the player you want to trade with.` |
| `trade.rs:343` | `trade_not_facing` | `TradeNotFacing` | `请面对你要交易的对象。` | `Please face the player you want to trade with.` |
| `trade.rs:375` | `trade_already_trading` | `TradeAlreadyTrading` | `你已经在交易中。` | `You are already trading.` |
| `trade.rs:392` | `trade_not_facing` | `TradeNotFacing` | `请面对你要交易的对象。` | `Please face the player you want to trade with.` |
| `trade.rs:398` | `trade_self` | `TradeSelf` | `不能和自己交易。` | `You cannot trade with yourself.` |
| `trade.rs:403` | `trade_dead` | `TradeDead` | `死亡状态不能交易。` | `You cannot trade while dead.` |
| `trade.rs:408` | `trade_target_already_invited` | `TradeTargetAlreadyInvited` | `{target_name} 已经收到了交易邀请。` | `{target_name} has already received a trade request.` |
| `trade.rs:413` | `trade_target_disallowed` | `TradeTargetDisallowed` | `{target_name} 不允许交易。` | `{target_name} does not allow trading.` |
| `trade.rs:419` | `trade_target_out_of_range` | `TradeTargetOutOfRange` | `{target_name} 不在交易范围内。` | `{target_name} is out of trading range.` |
| `trade.rs:424` | `trade_target_already_trading` | `TradeTargetAlreadyTrading` | `{target_name} 已经在交易中。` | `{target_name} is already trading.` |
| `trade.rs:467` | `trade_declined` | `TradeDeclined` | `{invitee_name} 拒绝交易。` | `{invitee_name} declined the trade.` |
| `trade.rs:472` | `trade_already_trading` | `TradeAlreadyTrading` | `你已经在交易中。` | `You are already trading.` |
| `trade.rs:477` | `trade_inviter_already_trading` | `TradeInviterAlreadyTrading` | `{inviter_name} 已经在交易中。` | `{inviter_name} is already trading.` |
| `trade.rs:711` | `trade_partner_waiting` | `TradePartnerWaiting` | `{name} 正在等待你确认交易。` | `{name} is waiting for you to confirm the trade.` |
| `trade.rs:756` | `trade_success` | `TradeSuccess` | `交易成功。` | `Trade successful.` |
| `trade.rs:958` | `trade_partner_gold_overflow` | `TradePartnerGoldOverflow` | `你的交易对方无法接受更多金币。` | `Your trading partner cannot accept any more Gold.` |
| `trade.rs:958` | `trade_gold_overflow` | `TradeGoldOverflow` | `你无法接受更多金币。` | `You cannot accept any more Gold.` |
| `trade.rs:960` | `trade_partner_items_overflow` | `TradePartnerItemsOverflow` | `你的交易对方无法接受全部物品。` | `Your trading partner cannot accept all of the items.` |
| `trade.rs:960` | `trade_items_overflow` | `TradeItemsOverflow` | `你无法接受全部物品。` | `You cannot accept all of the items.` |
| `world.rs:2688` | `pk_protected_by_law` | `PkProtectedByLaw` | `你受到了法律的保护。` | `You are protected by the law.` |

## 扩展键（4 个，不属于 §7.2）

用于替换 `GuildError::Permission` 会泄漏给玩家的内部标识符（Crystal `ServerTextKeys.NotAllowed*`）；§7.2 的 `guild_no_permission` 仍保留，作未知权限兜底。

| 键名 | 变体 | zh 模板 | en 模板 |
| --- | --- | --- | --- |
| `guild_not_allowed_recruit` | `GuildNotAllowedRecruit` | `你的职位没有招收成员的权限` | `Your rank is not allowed to recruit members.` |
| `guild_not_allowed_kick` | `GuildNotAllowedKick` | `你的职位没有踢出成员的权限` | `Your rank is not allowed to remove members.` |
| `guild_not_allowed_change_rank` | `GuildNotAllowedChangeRank` | `你的职位没有修改成员职位的权限` | `Your rank is not allowed to change member ranks.` |
| `guild_not_allowed_change_notice` | `GuildNotAllowedChangeNotice` | `你的职位没有修改行会公告的权限` | `Your rank is not allowed to change the guild notice.` |

## 第 1 轮 11 个无参键（不在 §7.2 内，文案冻结）

| 键名 | 变体 | zh 模板 | en 模板 |
| --- | --- | --- | --- |
| `welcome` | `Welcome` | `欢迎来到传奇-Next。` | `Welcome to Mir-Next.` |
| `account_banned` | `AccountBanned` | `该账号已被封禁。` | `This account is banned.` |
| `wrong_password` | `WrongPassword` | `账号或密码错误。` | `Incorrect account or password.` |
| `too_many_characters` | `TooManyCharacters` | `角色数量已达上限。` | `You have too many characters.` |
| `language_changed` | `LanguageChanged` | `语言已切换为简体中文。` | `Language switched to English.` |
| `unknown_language` | `UnknownLanguage` | `未知的语言代码，可用：zh-cn、en。` | `Unknown language code. Available: zh-cn, en.` |
| `server_full` | `ServerFull` | `服务器人数已满，请稍后再试。` | `The server is full, please try again later.` |
| `unsupported_command` | `UnsupportedCommand` | `不支持的命令。` | `Unsupported command.` |
| `character_name_taken` | `CharacterNameTaken` | `该角色名已被占用。` | `That character name is already taken.` |
| `character_name_invalid` | `CharacterNameInvalid` | `角色名不合法：3-15 个汉字、字母、数字或下划线。` | `Invalid character name: 3-15 CJK characters, letters, digits or underscores.` |
| `login_success` | `LoginSuccess` | `登录成功。` | `Login successful.` |
