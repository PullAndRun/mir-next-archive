//! 行会仓库：物品存取与金币存取。
//!
//! Crystal 的入口是 `PlayerObject.GuildStorageItemChange` / `GuildStorageGoldChange`
//! （`Server/MirObjects/PlayerObject.cs:10061-10282`），数据落在 `GuildInfo.StoredItems`
//! （`GuildStorageItem[112]`，`Server/MirDatabase/GuildInfo.cs:22`）与 `GuildInfo.Gold`
//! （仓库金币 = 行会资金，同一个字段，见 `migrations/0006_guild_storage.sql`）。
//!
//! # 包序
//!
//! | 语义 | 客户端包 | 服务端包 |
//! |---|---|---|
//! | 存物品 | `C.GuildStorageItemChange{Type=0, From=背包格, To=仓库格}`（85） | `S.GuildStorageItemChange{Type=0, User=角色 ID, Item, To, From}`（173，**广播全行会**） |
//! | 取物品 | `{Type=1, From=仓库格, To=背包格}` | `{Type=1, User, To, From}`（**没有 Item**，广播） |
//! | 仓库内移动 | `{Type=2, From, To}` | `{Type=2, User, Item=移动后目标格, To, From}`（广播） |
//! | 拉取列表 | `{Type=3}` | `S.GuildStorageList{Items[112]}`（174，**只给请求者**） |
//! | 失败 | — | `S.GuildStorageItemChange{Type=3+Type, From, To}`（只给请求者；见下方例外） |
//! | 存金币 | `C.GuildStorageGoldChange{Type=0, Amount}`（84） | `S.LoseGold`（本人）+ `S.GuildStorageGoldChange{Type=0, Name, Amount}`（广播，172） |
//! | 取金币 | `{Type=1, Amount}`（任何非 0 值都按「取」处理） | `S.GainedGold`（本人）+ `S.GuildStorageGoldChange{Type=1, Name, Amount}`（广播） |
//!
//! 例外（Crystal 原样）：**取物品没有权限时不发失败包**，只发一条系统聊天
//! （`PlayerObject.cs:10186-10191`，与存物品分支的 `Enqueue(p)` 不对称）。
//!
//! # 安全区
//!
//! Crystal 要求 `InSafeZone`（`MapObject.InSafeZone`，`Server/MirObjects/MapObject.cs:168`，
//! 由 `Map.GetSafeZone` 判定，`Server/MirEnvir/Map.cs:2386-2395`：安全区是
//! `SafeZoneInfo{X, Y, Size}` 的 Chebyshev 距离判定）。本作从 `MapInfo.SafeZones`
//! （`crates/mir-db/src/map.rs:30`）现算，见 [`World::in_safe_zone`]。
//!
//! # 简化
//!
//! * **`CheckItem` / `SendItemInfo` 没做**：Crystal 在每次物品变动后给全行会补发
//!   `S.NewItemInfo`（`GuildObject.SendItemInfo`，`GuildObject.cs:617-630` → `PlayerObject.CheckItem`），
//!   让客户端认识陌生物品。本作还没有「按需下发物品模板」的通道，客户端靠自己启动时
//!   加载的模板；这里只记一条 `debug!`。
//! * **`RefreshBagWeight` 不需要**：本作的负重是即时算的（`World::bag_weight`），没有缓存。

use mir_core::enums::{BindMode, GuildRankOptions};
use mir_protocol::ServerPacket;
use mir_protocol::data::GuildStorageItem;
use mir_protocol::packets::server::{
    GuildStorageGoldChange as GuildStorageGoldChangePacket, GuildStorageItemChange,
    GuildStorageList,
};

use crate::guild::{GUILD_STORAGE_SLOTS, GuildError, GuildStorageSlot, has_option};
use crate::items::INVENTORY_SIZE;
use crate::world::World;

/// 一条 `S.GuildStorageItemChange`（包号 173）。
fn item_change_frame(
    change_type: u8,
    to: i32,
    from: i32,
    user: i32,
    item: Option<GuildStorageItem>,
) -> Vec<u8> {
    ServerPacket::GuildStorageItemChange(GuildStorageItemChange {
        user,
        change_type,
        to,
        from,
        item,
    })
    .encode_frame()
}

/// 一条失败回执：Crystal 的 `p.Type = (byte)(3 + type)`，`Item = null`
/// （`PlayerObject.cs:10123`）。
fn failure_frame(change_type: u8, from: i32, to: i32) -> Vec<u8> {
    item_change_frame(change_type.wrapping_add(3), to, from, 0, None)
}

/// 一条 `S.GuildStorageList`（包号 174）。
fn list_frame(items: Vec<Option<GuildStorageItem>>) -> Vec<u8> {
    ServerPacket::GuildStorageList(GuildStorageList { items }).encode_frame()
}

/// 一条 `S.GuildStorageGoldChange`（包号 172）。
fn gold_frame(amount: u32, type_: u8, name: &str) -> Vec<u8> {
    ServerPacket::GuildStorageGoldChange(GuildStorageGoldChangePacket {
        amount,
        type_,
        name: name.to_owned(),
    })
    .encode_frame()
}

/// 物品是否被标记为「不能进仓库」（`BindMode.DontStore`）。
///
/// 对应 `PlayerObject.cs:10162-10171`：
/// `Info.Inventory[from].Info.Bind.HasFlag(BindMode.DontStore)` 与
/// `RentalInformation.BindingFlags.HasFlag(BindMode.DontStore)` 两处。
///
/// `check_rental` 只在**存物品**（Type=0）时为 `true`：Crystal 的取回/移动分支
/// 只检查模板的 `Bind`（`:10213`、`:10247`），不看 `RentalInformation`。
///
/// 模板缺失（本作的 `decode_slots` 不校验模板存在）时视为**没有限制**：
/// Crystal 那边 `Item.Info` 不可能为 `null`，这里偏保守地放行，避免老存档里的
/// 未知物品永远卡在仓库外。
fn cannot_store(world: &World, item: &mir_protocol::data::UserItem, check_rental: bool) -> bool {
    let template_blocked = world
        .item_info(item.item_index)
        .is_some_and(|info| info.bind.contains(BindMode::DontStore));
    let rental_blocked = check_rental
        && item
            .rental_information
            .as_ref()
            .is_some_and(|rental| rental.binding_flags.contains(BindMode::DontStore));
    template_blocked || rental_blocked
}

impl World {
    /// 玩家是否站在安全区内（Crystal `MapObject.InSafeZone`，`MapObject.cs:168`、
    /// `Map.cs:2386-2395`）。
    ///
    /// 判定是 **Chebyshev 距离**（`Shared/Functions/Functions.cs:71-74` 的
    /// `Math.Abs(a.X - b.X) <= i && Math.Abs(a.Y - b.Y) <= i`），`Size` 是安全区半径。
    /// 没有加载 `MapInfo`（测试里的合成地图）时返回 `false`——Crystal 没有数据库时
    /// 也不可能有安全区，行为一致。
    #[must_use]
    pub fn in_safe_zone(&self, object_id: u32) -> bool {
        let Some(player) = self.players.get(&object_id) else {
            return false;
        };
        let Some(data) = self.game_data() else {
            return false;
        };
        let Some(info) = data.map_info(&player.map) else {
            return false;
        };
        let position = player.position;
        info.safe_zones.iter().any(|zone| {
            let size = i32::from(zone.size);
            (position.x - zone.location.x).abs() <= size
                && (position.y - zone.location.y).abs() <= size
        })
    }

    /// `C.GuildStorageItemChange`（Crystal `PlayerObject.GuildStorageItemChange`，
    /// `PlayerObject.cs:10121-10282`）。
    ///
    /// `type_`：`0` 存、`1` 取、`2` 仓库内移动、`3` 拉取列表；其它值什么也不做
    /// （Crystal 的 `switch` 没有 `default`，失败回执 `p` 已经建好但不会发）。
    pub fn guild_storage_item_change(
        &mut self,
        player_id: u32,
        type_: u8,
        from: i32,
        to: i32,
    ) -> Result<(), GuildError> {
        let (guild_id, rank_index) = match self.my_guild(player_id) {
            Ok(found) => found,
            Err(error) => {
                // `Enqueue(p)` → `ReceiveChat(NotPartOfGuild)`（`:10124-10129`）。
                self.send_to_player(player_id, &failure_frame(type_, from, to));
                self.guild_error_to(player_id, &error);
                return Err(error);
            }
        };

        // `if (!InSafeZone && type != 3)`（`:10131-10136`）——拉取列表不要求安全区。
        if type_ != 3 && !self.in_safe_zone(player_id) {
            self.send_to_player(player_id, &failure_frame(type_, from, to));
            let error = GuildError::NotInSafeZone;
            self.guild_error_to(player_id, &error);
            return Err(error);
        }

        let options = self
            .guilds
            .get(&guild_id)
            .and_then(|guild| guild.rank(rank_index))
            .map_or(0, |rank| rank.options);

        match type_ {
            // -------------------------------------------------------------
            // 0：存物品（`:10140-10184`）
            // -------------------------------------------------------------
            0 => {
                if !has_option(options, GuildRankOptions::CanStoreItem) {
                    self.send_to_player(player_id, &failure_frame(0, from, to));
                    let error = GuildError::NoStorePermission;
                    self.guild_error_to(player_id, &error);
                    return Err(error);
                }
                let (Ok(from_slot), Ok(to_slot)) = (
                    usize::try_from(from).map_err(|_| ()),
                    usize::try_from(to).map_err(|_| ()),
                ) else {
                    self.send_to_player(player_id, &failure_frame(0, from, to));
                    return Err(GuildError::InvalidStorageSlot);
                };
                if from_slot >= INVENTORY_SIZE || to_slot >= GUILD_STORAGE_SLOTS {
                    self.send_to_player(player_id, &failure_frame(0, from, to));
                    return Err(GuildError::InvalidStorageSlot);
                }
                let Some(item) = self
                    .players
                    .get(&player_id)
                    .and_then(|player| player.inventory.get(from_slot).cloned().flatten())
                else {
                    self.send_to_player(player_id, &failure_frame(0, from, to));
                    return Err(GuildError::EmptySlot);
                };
                if cannot_store(self, &item, true) {
                    self.send_to_player(player_id, &failure_frame(0, from, to));
                    return Err(GuildError::ItemCannotBeStored);
                }
                let target_free = self
                    .guilds
                    .get(&guild_id)
                    .is_some_and(|guild| guild.storage_slot(to_slot).is_none());
                if !target_free {
                    // Crystal 这个分支是**先聊天、后失败包**（`:10174-10175`）：
                    // `ReceiveChat(TargetSlotNotEmpty); Enqueue(p);`。
                    let error = GuildError::TargetSlotNotEmpty;
                    self.guild_error_to(player_id, &error);
                    self.send_to_player(player_id, &failure_frame(0, from, to));
                    return Err(error);
                }
                let character_id = self.players[&player_id].character_id;
                let stored_at = self.wall_now();
                let Some(player) = self.players.get_mut(&player_id) else {
                    return Err(GuildError::NotInWorld(player_id));
                };
                player.inventory[from_slot] = None;
                let stored = GuildStorageItem {
                    item,
                    user_id: character_id,
                };
                let Some(guild) = self.guilds.get_mut(&guild_id) else {
                    return Err(GuildError::NotInGuild(player_id));
                };
                guild.storage[to_slot] = Some(GuildStorageSlot {
                    item: stored.clone(),
                    stored_at,
                });
                // `MyGuild.SendItemInfo(...)`（`:10181`）：见模块文档「简化」。
                tracing::debug!(
                    guild = guild_id,
                    slot = to_slot,
                    "行会仓库已存入物品（`CheckItem` 未实现）"
                );
                let frame = item_change_frame(
                    0,
                    to,
                    from,
                    i32::try_from(character_id).unwrap_or(0),
                    Some(stored),
                );
                self.send_to_guild(guild_id, &frame);
                self.mark_guild_dirty(guild_id);
                Ok(())
            }
            // -------------------------------------------------------------
            // 1：取物品（`:10185-10223`）
            // -------------------------------------------------------------
            1 => {
                if !has_option(options, GuildRankOptions::CanRetrieveItem) {
                    // Crystal 这里**没有** `Enqueue(p)`，只发聊天（`:10186-10191`）。
                    let error = GuildError::NoRetrievePermission;
                    self.guild_error_to(player_id, &error);
                    return Err(error);
                }
                let (Ok(from_slot), Ok(to_slot)) = (
                    usize::try_from(from).map_err(|_| ()),
                    usize::try_from(to).map_err(|_| ()),
                ) else {
                    self.send_to_player(player_id, &failure_frame(1, from, to));
                    return Err(GuildError::InvalidStorageSlot);
                };
                if from_slot >= GUILD_STORAGE_SLOTS || to_slot >= INVENTORY_SIZE {
                    self.send_to_player(player_id, &failure_frame(1, from, to));
                    return Err(GuildError::InvalidStorageSlot);
                }
                let target_free = self
                    .players
                    .get(&player_id)
                    .and_then(|player| player.inventory.get(to_slot))
                    .is_some_and(Option::is_none);
                if !target_free {
                    // 同存物品：`ReceiveChat(TargetSlotNotEmpty); Enqueue(p);`（`:10204-10205`）。
                    let error = GuildError::TargetSlotNotEmpty;
                    self.guild_error_to(player_id, &error);
                    self.send_to_player(player_id, &failure_frame(1, from, to));
                    return Err(error);
                }
                let Some(entry) = self
                    .guilds
                    .get(&guild_id)
                    .and_then(|guild| guild.storage_slot(from_slot))
                    .cloned()
                else {
                    self.send_to_player(player_id, &failure_frame(1, from, to));
                    return Err(GuildError::EmptySlot);
                };
                if cannot_store(self, &entry.item.item, false) {
                    self.send_to_player(player_id, &failure_frame(1, from, to));
                    return Err(GuildError::ItemCannotBeStored);
                }
                let character_id = self.players[&player_id].character_id;
                let Some(guild) = self.guilds.get_mut(&guild_id) else {
                    return Err(GuildError::NotInGuild(player_id));
                };
                guild.storage[from_slot] = None;
                let Some(player) = self.players.get_mut(&player_id) else {
                    return Err(GuildError::NotInWorld(player_id));
                };
                player.inventory[to_slot] = Some(entry.item.item);
                // `S.GuildStorageItemChange{Type=1, User, To, From}`：**没有 Item**（`:10220`）。
                let frame =
                    item_change_frame(1, to, from, i32::try_from(character_id).unwrap_or(0), None);
                self.send_to_guild(guild_id, &frame);
                self.mark_guild_dirty(guild_id);
                Ok(())
            }
            // -------------------------------------------------------------
            // 2：仓库内移动（`:10224-10266`）
            // -------------------------------------------------------------
            2 => {
                if !has_option(options, GuildRankOptions::CanStoreItem) {
                    self.send_to_player(player_id, &failure_frame(2, from, to));
                    let error = GuildError::NoStorageMovePermission;
                    self.guild_error_to(player_id, &error);
                    return Err(error);
                }
                let (Ok(from_slot), Ok(to_slot)) = (
                    usize::try_from(from).map_err(|_| ()),
                    usize::try_from(to).map_err(|_| ()),
                ) else {
                    self.send_to_player(player_id, &failure_frame(2, from, to));
                    return Err(GuildError::InvalidStorageSlot);
                };
                if from_slot >= GUILD_STORAGE_SLOTS || to_slot >= GUILD_STORAGE_SLOTS {
                    self.send_to_player(player_id, &failure_frame(2, from, to));
                    return Err(GuildError::InvalidStorageSlot);
                }
                let Some(entry) = self
                    .guilds
                    .get(&guild_id)
                    .and_then(|guild| guild.storage_slot(from_slot))
                    .cloned()
                else {
                    self.send_to_player(player_id, &failure_frame(2, from, to));
                    return Err(GuildError::EmptySlot);
                };
                if cannot_store(self, &entry.item.item, false) {
                    self.send_to_player(player_id, &failure_frame(2, from, to));
                    return Err(GuildError::ItemCannotBeStored);
                }
                let character_id = self.players[&player_id].character_id;
                let Some(guild) = self.guilds.get_mut(&guild_id) else {
                    return Err(GuildError::NotInGuild(player_id));
                };
                // `q = StoredItems[to]; StoredItems[to] = StoredItems[from]; StoredItems[from] = q`
                // ——就是交换（`:10252-10258`）。
                let swapped = guild.storage[to_slot].replace(entry);
                guild.storage[from_slot] = swapped;
                let moved = guild.storage[to_slot]
                    .as_ref()
                    .map(|slot| slot.item.clone());
                let frame =
                    item_change_frame(2, to, from, i32::try_from(character_id).unwrap_or(0), moved);
                self.send_to_guild(guild_id, &frame);
                self.mark_guild_dirty(guild_id);
                Ok(())
            }
            // -------------------------------------------------------------
            // 3：拉取列表（`:10267-10279`）
            // -------------------------------------------------------------
            3 => {
                // `if (!GuildCanRequestItems) return; GuildCanRequestItems = false;`（`:10268-10269`）。
                if self.guild_storage_requested.contains(&player_id) {
                    return Err(GuildError::StorageRequestThrottled);
                }
                self.guild_storage_requested.insert(player_id);
                let items: Vec<Option<GuildStorageItem>> = self
                    .guilds
                    .get(&guild_id)
                    .map(|guild| {
                        guild
                            .storage
                            .iter()
                            .map(|slot| slot.as_ref().map(|slot| slot.item.clone()))
                            .collect()
                    })
                    .unwrap_or_default();
                // `CheckItem(item)`（`:10276`）：见模块文档「简化」。
                tracing::debug!(
                    guild = guild_id,
                    items = items.iter().filter(|item| item.is_some()).count(),
                    "行会仓库列表已下发（`CheckItem` 未实现）"
                );
                self.send_to_player(player_id, &list_frame(items));
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// `C.GuildStorageGoldChange`（Crystal `PlayerObject.GuildStorageGoldChange`，
    /// `PlayerObject.cs:10061-10120`）。
    ///
    /// `type_ == 0` 是存入，**任何其它值都按取出处理**（Crystal 的 `else` 分支，`:10095`）：
    /// 客户端只会发 0/1；服务端自己广播的 2/3 是「花费/收入」，不走这个入口。
    pub fn guild_storage_gold_change(
        &mut self,
        player_id: u32,
        type_: u8,
        amount: u32,
    ) -> Result<(), GuildError> {
        let (guild_id, rank_index) = match self.my_guild(player_id) {
            Ok(found) => found,
            Err(error) => {
                // 金币分支只发聊天，没有失败包（`:10063-10067`）。
                self.guild_error_to(player_id, &error);
                return Err(error);
            }
        };
        // `if (!InSafeZone)`（`:10069-10073`）。
        if !self.in_safe_zone(player_id) {
            let error = GuildError::NotInSafeZone;
            self.guild_error_to(player_id, &error);
            return Err(error);
        }
        let player_gold = self.players[&player_id].gold;
        let guild_gold = self.guilds.get(&guild_id).map_or(0, |guild| guild.gold);

        if type_ == 0 {
            // ---- 存入（`:10075-10094`）----
            if player_gold < i64::from(amount) {
                let error = GuildError::NotEnoughGold {
                    required: amount,
                    gold: player_gold,
                };
                self.guild_error_to(player_id, &error);
                return Err(error);
            }
            // `if ((MyGuild.Gold + (ulong)amount) > uint.MaxValue)`（`:10083-10087`）。
            if u64::from(guild_gold) + u64::from(amount) > u64::from(u32::MAX) {
                let error = GuildError::GuildGoldLimitReached;
                self.guild_error_to(player_id, &error);
                return Err(error);
            }
            let name = self.players[&player_id].name.clone();
            // `Account.Gold -= amount; Enqueue(new S.LoseGold { Gold = amount });`（`:10089-10091`）。
            let _ = self.lose_gold(player_id, amount);
            if let Some(guild) = self.guilds.get_mut(&guild_id) {
                guild.gold = guild.gold.saturating_add(amount);
            }
            let frame = gold_frame(amount, 0, &name);
            self.send_to_guild(guild_id, &frame);
            self.mark_guild_dirty(guild_id);
            return Ok(());
        }

        // ---- 取出（`:10095-10119`）----
        if guild_gold < amount {
            let error = GuildError::GuildBankNotEnough {
                required: amount,
                gold: guild_gold,
            };
            self.guild_error_to(player_id, &error);
            return Err(error);
        }
        // `if (!CanGainGold(amount))` → `(ulong)gold + Account.Gold <= uint.MaxValue`（`:7638-7641`）。
        if player_gold.saturating_add(i64::from(amount)) > i64::from(u32::MAX) {
            let error = GuildError::PlayerGoldLimitReached;
            self.guild_error_to(player_id, &error);
            return Err(error);
        }
        // `if (MyGuildRank.Index != 0)`（`:10109-10113`）：**只有会长**能取。
        if rank_index != crate::guild::GUILD_LEADER_RANK_INDEX {
            let error = GuildError::LeaderOnly;
            self.guild_error_to(player_id, &error);
            return Err(error);
        }
        let name = self.players[&player_id].name.clone();
        if let Some(guild) = self.guilds.get_mut(&guild_id) {
            guild.gold = guild.gold.saturating_sub(amount);
        }
        // `GainGold(amount);` → `S.GainedGold`（本人），`:10116`。
        self.gain_gold(player_id, amount);
        let frame = gold_frame(amount, 1, &name);
        self.send_to_guild(guild_id, &frame);
        self.mark_guild_dirty(guild_id);
        Ok(())
    }
}
