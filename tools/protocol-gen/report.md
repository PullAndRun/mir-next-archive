# 协议生成报告

## client: 149/153 自动生成，4 个待手写

- `MarketSearch`: unknown field 'MaxShape'
- `SendMail`: unsupported write statement: for (int i = 0; i < 5; i++) { writer.Write(ItemsIdx[i]); }
- `MailCost`: unsupported write statement: for (int i = 0; i < 5; i++) { writer.Write(ItemsIdx[i]); }
- `UpdateIntelligentCreature`: unsupported field type 'ClientIntelligentCreature' for 'Creature'

## server: 238/277 自动生成，39 个待手写

- `NewMapInfo`: unsupported field type 'ClientMapInfo' for 'Info'
- `WorldMapSetupInfo`: unsupported field type 'WorldMapSetup' for 'Setup'
- `UserSlotsRefresh`: unsupported optional body: { Inventory = new UserItem[reader.ReadInt32()]; for (int i = 0; i < Inventory.Length; i++) { if (!re
- `NewItemInfo`: unsupported field type 'ItemInfo' for 'Info'
- `NewHeroInfo`: unsupported field type 'ClientHeroInformation' for 'Info'
- `NewMagic`: unsupported field type 'ClientMagic' for 'Magic'
- `Magic`: unsupported collection loop: foreach (var targetID in SecondaryTargetIDs) { writer.Write(targetID); }
- `ObjectMagic`: unsupported collection loop: foreach (var targetID in SecondaryTargetIDs) { writer.Write(targetID); }
- `UserStorage`: unsupported optional body: return
- `AddBuff`: unsupported field type 'ClientBuff' for 'Buff'
- `NPCMarket`: unsupported field type 'List<ClientAuction>' for 'Listings'
- `NPCMarketPage`: unsupported field type 'List<ClientAuction>' for 'Listings'
- `GuildTerritoryPage`: unsupported field type 'List<ClientGTMap>' for 'Listings'
- `BaseStatsInfo`: unsupported field type 'BaseStats' for 'Stats'
- `HeroBaseStatsInfo`: unsupported field type 'BaseStats' for 'Stats'
- `ChatItemStats`: unsupported write statement: if (Stats != null)  Stats.Save(writer)
- `GuildNoticeChange`: unsupported fixed loop body: notice.Add(reader.ReadString())
- `GuildMemberChange`: unsupported read statement: if (Status > 5) { int rankcount = reader.ReadInt32(); for (int i = 0; i < rankcount; i++) Ranks.Add(new GuildRank(reader)); }
- `GuildStorageItemChange`: unsupported optional body: return
- `GuildStorageList`: unsupported field type 'GuildStorageItem[]' for 'Items'
- `HeroCreateRequest`: unsupported read expression: new bool[count]
- `ManageHeroes`: unsupported field type 'ClientHeroInformation' for 'CurrentHero'
- `ChangeQuest`: unsupported field type 'ClientQuestProgress' for 'Quest'
- `CompleteQuest`: unsupported collection loop: foreach (int q in CompletedQuests)  writer.Write(q);
- `NewQuestInfo`: unsupported field type 'ClientQuestInfo' for 'Info'
- `GameShopInfo`: unsupported field type 'GameShopItem' for 'Item'
- `AwakeningNeedMaterials`: unsupported optional body: return
- `ReceiveMail`: unsupported field type 'List<ClientMail>' for 'Mail'
- `NewIntelligentCreature`: unsupported field type 'ClientIntelligentCreature' for 'Creature'
- `UpdateIntelligentCreatureList`: unsupported field type 'List<ClientIntelligentCreature>' for 'CreatureList'
- `FriendUpdate`: unsupported field type 'List<ClientFriend>' for 'Friends'
- `GuildBuffList`: unsupported field type 'List<GuildBuff>' for 'ActiveBuffs'
- `Rankings`: unsupported field type 'List<RankCharacterInfo>' for 'ListingDetails'
- `GetRentedItems`: unsupported field type 'List<ItemRentalInformation>' for 'RentedItems'
- `UpdateRentalItem`: unsupported read statement: if (HasData)  LoanItem = new UserItem(reader)
- `NewRecipeInfo`: unsupported field type 'ClientRecipeInfo' for 'Info'
- `UpdateNotice`: unsupported field type 'Notice' for 'Notice'
- `NewMonsterInfo`: unsupported field type 'ClientMonsterInfo' for 'Info'
- `NewNPCInfo`: unsupported field type 'ClientNPCInfo' for 'Info'
