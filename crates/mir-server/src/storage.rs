//! 玩家仓库（个人仓库，非行会仓库）。
//!
//! 逐条对照 Crystal（参考根目录 `github\Crystal`，下文只写相对路径）：
//!
//! | 本模块 | Crystal |
//! |---|---|
//! | [`STORAGE_SIZE`] | `AccountInfo.Storage = new UserItem[80]`（`Server/MirDatabase/AccountInfo.cs:69`）+ `Globals.StorageGridSize = 80`（`Shared/Globals.cs:19`） |
//! | [`decode_storage`] / [`encode_storage`] | `AccountInfo` 读档 `Array.Resize(ref Storage, count)` + 逐槽 bool/`UserItem`（`AccountInfo.cs:176-186`）、存档 `:241-248` |
//! | [`World::open_storage`] | `NPCScript.ProcessSpecial` 的 `case StorageKey:`（`Server/MirObjects/NPC/NPCScript.cs:1002-1006`）→ `PlayerObject.SendStorage`（`PlayerObject.cs:8161-8181`） |
//! | [`World::storage_store_item`] | `PlayerObject.StoreItem`（`Server/MirObjects/PlayerObject.cs:5376-5456`） |
//! | [`World::storage_take_back_item`] | `PlayerObject.TakeBackItem`（`:5457-5526`） |
//! | 仓库页面判定 | `NPCPage.Key == NPCScript.StorageKey`（`NPCScript.cs:55`、`PlayerObject.cs:5380`、`:5461`） |
//! | NPC 距离判定 | `Functions.InRange(ob.CurrentLocation, CurrentLocation, Globals.DataRange)`（`PlayerObject.cs:5393`、`:5474`；`Globals.DataRange = 16`，`Shared/Globals.cs:37`） |
//!
//! # 包序
//!
//! | 语义 | 客户端包 | 服务端包 | Crystal |
//! |---|---|---|---|
//! | 打开仓库 | `C.CallNPC{Key="[@STORAGE]"}` | `S.UserStorage`（130，每个连接**只发一次**）→ `S.NPCStorage`（110） | `PlayerObject.cs:8169-8180`、`NPCScript.cs:1002-1006` |
//! | 存物品 | `C.StoreItem{From=背包格, To=仓库格}`（15） | `S.StoreItem{From, To, Success}`（43） | `PlayerObject.cs:5376-5456` |
//! | 取物品 | `C.TakeBackItem{From=仓库格, To=背包格}`（16） | `S.TakeBackItem{From, To, Success}`（42） | `:5457-5526` |
//!
//! 失败没有任何聊天提示：Crystal 在每条拒绝分支上只 `Enqueue(p)`（`p.Success` 保持
//! `false`），所以本作同样只回失败包（见 `PlayerObject.cs:5380-5455`、`:5461-5525`）。
//!
//! # 容量与「超容量」
//!
//! Crystal 的仓库是 `UserItem[80]`，`IsValidStorageIndex(to)`（`AccountInfo.cs:277-286`）
//! 在**没有**扩充仓库（`HasExpandedStorage == false`）时只允许 `0..80`；买过扩充是 `0..160`
//! （`ExpandStorage`，`:266-275`）。本作不做扩充道具（见 `docs/world/storage.md` 的未做清单），
//! 所以容量恒为 [`STORAGE_SIZE`] = 80；`to >= 80` 一律拒绝。
//!
//! # 与 Crystal 的**有意**差异
//!
//! 1. **仓库挂在角色上**：Crystal 是 `AccountInfo.Storage`（同账号多角色共享）；
//!    本作按任务要求存 `characters.storage`（`migrations/0001_init.sql:101`），
//!    每个角色一个仓库。
//! 2. **金币存取没有实现**：Crystal 的个人仓库**没有金币功能**——`C.ClientPackets`
//!    里没有 `DepositGold`/`TakeBackGold`，`AccountInfo` 也没有「仓库金币」字段，
//!    客户端 `StorageDialog` 只有物品格子（`Client/MirScenes/Dialogs/NPCDialogs.cs:2798`）。
//!    金币仓库是**行会**才有的（`C.GuildStorageGoldChange`，见 [`crate::guild_storage`]）。
//!    本作不发明协议，见 `docs/world/storage.md` §6。
//! 3. **仓库密码与扩充仓库没有实现**：`Settings.RequireStoragePassword` 恒为假，
//!    所以 `CanAccessStorage()`（`PlayerObject.cs:8208-8213`）恒真；
//!    `HasExpandedStorage` 恒假（`S.UserInformation.HasExpandedStorage` 也发 false，
//!    见 `session.rs` 的 `user_information`）。
//! 4. **`CheckItem` 没做**：Crystal 在 `SendStorage` 里对每件仓库物品调
//!    `PlayerObject.CheckItem`（`PlayerObject.cs:8172-8178`），给客户端补发未知物品模板
//!    （`S.NewItemInfo`）。本作没有「按需下发模板」的通道，客户端靠自己启动时加载的模板；
//!    这里只记一条 `debug!`。
//! 5. **`RefreshBagWeight` 不需要**：本作的负重是即时算的（[`World::bag_weight`]），
//!    没有缓存（Crystal 每格缓存一次，`PlayerObject.cs:5447`、`:5520`）。

use mir_core::enums::BindMode;
use mir_protocol::ServerPacket;
use mir_protocol::data::UserItem;
use mir_protocol::packets::server::{
    NPCStorage, StoreItem as StoreItemPacket, TakeBackItem as TakeBackItemPacket, UserStorage,
};

use crate::items::{
    INVENTORY_SIZE, ItemCodecError, decode_slots_with_limit, encode_slots, fit_slots,
};
use crate::npc::{NPC_TALK_RANGE, STORAGE_KEY};
use crate::world::World;

/// 仓库格数：Crystal `AccountInfo.Storage = new UserItem[80]`（`AccountInfo.cs:69`）
/// 与 `Globals.StorageGridSize = 80`（`Globals.cs:19`）。
///
/// 注意它不是任务书里提到的 112——112 是**行会仓库**的格数
/// （`GuildInfo.StoredItems = new GuildStorageItem[112]`，`Server/MirDatabase/GuildInfo.cs:22`）。
pub const STORAGE_SIZE: usize = 80;

/// 仓库存档可能出现的最大格数：Crystal 允许把 80 格扩建到 160
/// （`AccountInfo.ExpandStorage` 的 `Storage.Length + Globals.StorageGridSize`，`AccountInfo.cs:266-275`）。
///
/// 本作**没有**扩充仓库，所以解码时允许读到 160 格、再裁到 [`STORAGE_SIZE`]（多的记日志丢弃）；
/// 这样老存档（别的服务端扩建过）不会让登录直接失败。
pub const STORAGE_MAX_SIZE: usize = 160;

/// 仓库存取失败原因。
///
/// 每个变体都对应 Crystal `StoreItem`/`TakeBackItem` 里的一次 `Enqueue(p); return;`
/// （失败包已由 [`World`] 内部发出），或本作解码器的一道防线。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StorageError {
    /// 玩家不在世界里。
    #[error("玩家 {0} 不在世界里")]
    NotInWorld(u32),
    /// 玩家已死亡（Crystal 的 `C.StoreItem` 入口不单独查 `Dead`，但 NPC 对话本身要求活着）。
    #[error("玩家 {player} 已死亡")]
    Dead {
        /// 玩家对象 ID。
        player: u32,
    },
    /// 当前不在 `[@STORAGE]` 页面（Crystal `NPCPage == null || NPCPage.Key != StorageKey`，
    /// `PlayerObject.cs:5380-5384`、`:5461-5465`）。
    #[error("玩家不在仓库对话中")]
    NoStorageDialog,
    /// `NPCObjectID` 指向的 NPC 不在当前地图上（`PlayerObject.cs:5386-5396`）。
    #[error("NPC {object_id} 不在当前地图上")]
    NoSuchNpc {
        /// NPC 对象 ID。
        object_id: u32,
    },
    /// 离仓库 NPC 太远（`PlayerObject.cs:5393`，`Globals.DataRange = 16`）。
    #[error("离仓库 NPC {object_id} 太远（上限 {range} 格）")]
    OutOfRange {
        /// NPC 对象 ID。
        object_id: u32,
        /// 距离上限。
        range: i32,
    },
    /// 背包下标越界（`from < 0 || from >= Info.Inventory.Length`，`PlayerObject.cs:5405`、`:5498`）。
    #[error("背包下标 {index} 越界（共 {size} 格）")]
    InventoryIndex {
        /// 请求的下标。
        index: i32,
        /// 背包格数。
        size: usize,
    },
    /// 仓库下标越界（`to < 0 || to >= Account.Storage.Length`，`PlayerObject.cs:5411`、`:5486`）。
    #[error("仓库下标 {index} 越界（共 {size} 格）")]
    StorageIndex {
        /// 请求的下标。
        index: i32,
        /// 仓库格数。
        size: usize,
    },
    /// 仓库格已被占用（`Account.Storage[to] != null`，`PlayerObject.cs:5443-5455`）。
    #[error("仓库 {index} 号格已被占用")]
    TargetOccupied {
        /// 仓库格。
        index: i32,
    },
    /// 源格是空的（`temp == null`，`PlayerObject.cs:5425-5429`、`:5506-5510`）。
    #[error("{grid} 的 {index} 号格是空的")]
    EmptySlot {
        /// `"Inventory"` 或 `"Storage"`（只用于日志 Display）。
        grid: &'static str,
        /// 格子下标。
        index: i32,
    },
    /// 物品被标记为不能进仓库（`BindMode.DontStore`，`PlayerObject.cs:5431-5441`）。
    #[error("物品 {unique_id} 不允许存入仓库")]
    CannotStore {
        /// 物品 UUID。
        unique_id: u64,
    },
    /// 仓库字节块解析失败（本作解码器的防线，Crystal 不会遇到）。
    #[error("仓库字节块解析失败：{0}")]
    Codec(String),
}

impl From<ItemCodecError> for StorageError {
    fn from(error: ItemCodecError) -> Self {
        Self::Codec(error.to_string())
    }
}

/// 空仓库（80 格）。
#[must_use]
pub fn new_storage() -> Vec<Option<UserItem>> {
    vec![None; STORAGE_SIZE]
}

/// 解码 `characters.storage` 字节块并补齐到 [`STORAGE_SIZE`] 格。
///
/// Crystal 的读档是 `Array.Resize(ref Storage, count)` 后逐槽读（`AccountInfo.cs:176-186`），
/// 空块（新角色）得到 80 格空仓库；多的槽位丢弃、少的补空（同 [`fit_slots`]）。
pub fn decode_storage(bytes: &[u8]) -> Result<Vec<Option<UserItem>>, StorageError> {
    if bytes.is_empty() {
        return Ok(new_storage());
    }
    Ok(fit_slots(
        decode_slots_with_limit(bytes, STORAGE_MAX_SIZE)?,
        STORAGE_SIZE,
    ))
}

/// 编码仓库为 Crystal 的 `UserItem[]` 字节块（`AccountInfo.Save`，`AccountInfo.cs:241-248`）。
#[must_use]
pub fn encode_storage(slots: &[Option<UserItem>]) -> Vec<u8> {
    encode_slots(slots)
}

/// 一件物品是否禁止进仓库（模板 `BindMode.DontStore`，以及租借信息上的同一位）。
///
/// 对应 `PlayerObject.cs:5431-5441`。本作没有租借系统，`RentalInformation` 恒为空，
/// 但协议里的 `UserItem` 字段还在，所以照样检查（与 `guild_storage::cannot_store` 同规则）。
///
/// 模板缺失时**放行**：Crystal 的 `Item.Info` 不可能为 null，本作老存档里的未知物品
/// 不应该永远卡在仓库外。
fn cannot_store(world: &World, item: &UserItem) -> bool {
    let template_blocked = world
        .item_info(item.item_index)
        .is_some_and(|info| info.bind.contains(BindMode::DontStore));
    let rental_blocked = item
        .rental_information
        .as_ref()
        .is_some_and(|rental| rental.binding_flags.contains(BindMode::DontStore));
    template_blocked || rental_blocked
}

impl World {
    /// 仓库列表（只读；测试与 UI 用）。
    #[must_use]
    pub fn storage(&self, object_id: u32) -> Option<&[Option<UserItem>]> {
        self.players
            .get(&object_id)
            .map(|player| player.storage.as_slice())
    }

    /// 仓库某一格。
    #[must_use]
    pub fn storage_slot(&self, object_id: u32, index: usize) -> Option<&UserItem> {
        self.players.get(&object_id)?.storage.get(index)?.as_ref()
    }

    /// 校验「玩家此刻能访问仓库」：`[@STORAGE]` 页面 + NPC 在同图范围内。
    ///
    /// 逐条对应 Crystal `StoreItem`/`TakeBackItem` 的开头两次 `Enqueue(p); return;`
    /// （`PlayerObject.cs:5380-5403`、`:5461-5484`）。`CanAccessStorage()`
    /// （仓库密码）恒真，见模块文档的差异 3。
    fn check_storage_access(&self, player_id: u32) -> Result<(), StorageError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(StorageError::NotInWorld(player_id));
        };
        if player.dead {
            return Err(StorageError::Dead { player: player_id });
        }
        // `NPCPage == null || NPCPage.Key != NPCScript.StorageKey`（`:5380`、`:5461`）。
        if !player.npc.page_key.eq_ignore_ascii_case(STORAGE_KEY) {
            return Err(StorageError::NoStorageDialog);
        }
        // 在**当前地图**里按 `NPCObjectID` 找 NPC（`:5386-5397`、`:5466-5478`）。
        let npc_object_id = player.npc.npc_object_id;
        let player_map = player.map.clone();
        let player_position = player.position;
        let Some(npc) = self
            .npcs
            .iter()
            .find(|npc| npc.object_id == npc_object_id && npc.map == player_map)
        else {
            return Err(StorageError::NoSuchNpc {
                object_id: npc_object_id,
            });
        };
        if !npc.position.in_range(player_position, NPC_TALK_RANGE) {
            return Err(StorageError::OutOfRange {
                object_id: npc_object_id,
                range: NPC_TALK_RANGE,
            });
        }
        Ok(())
    }

    /// 判断玩家能否访问仓库（不查 NPC，只看状态；供 NPC 面板与测试用）。
    #[must_use]
    pub fn can_access_storage(&self, object_id: u32) -> bool {
        self.check_storage_access(object_id).is_ok()
    }

    /// 打开仓库界面：`S.UserStorage`（每个连接只发一次）+ `S.NPCStorage`。
    ///
    /// 对应 Crystal `NPCScript.cs:1002-1006`：
    ///
    /// ```csharp
    /// case StorageKey:
    ///     player.ResetStorageUnlock();
    ///     player.SendStorage();      // Enqueue(new S.UserStorage { Storage = Account.Storage });
    ///     player.Enqueue(new S.NPCStorage());
    ///     break;
    /// ```
    ///
    /// `SendStorage` 有 `if (Connection.StorageSent) return;`（`PlayerObject.cs:8169-8170`），
    /// 即**同一个连接只下发一次**仓库列表；之后重复打开只补 `S.NPCStorage`。
    /// 本作按对象 ID 记这个一次性的「已发」标志（下线/重登即失效，与 Crystal 的
    /// 每连接 `Connection.StorageSent` 等价）。
    pub fn open_storage(&mut self, player_id: u32) -> Result<(), StorageError> {
        self.check_storage_access(player_id)?;
        self.send_storage_panels(player_id);
        Ok(())
    }

    /// 真正的发包部分（`S.UserStorage` + `S.NPCStorage`）。
    ///
    /// NPC 面板在 `call_npc` 里打开时，`player.npc.page_key` 要到页面跑完才写入
    /// （`npc.rs` 的 [`World::call_npc`]），所以面板路径直接调这里、不再重复做页面检查——
    /// 能走到面板就说明已经通过 `call_npc` 的 NPC/距离校验（`NPCScript.cs:1002-1006`）。
    pub(crate) fn send_storage_panels(&mut self, player_id: u32) {
        if self.storage_sent.insert(player_id) {
            let storage = self.players[&player_id].storage.clone();
            self.send_to_player(
                player_id,
                &ServerPacket::UserStorage(UserStorage {
                    storage: Some(storage),
                })
                .encode_frame(),
            );
        } else {
            tracing::debug!(
                player = player_id,
                "仓库列表本连接已发过，只补 S.NPCStorage"
            );
        }
        self.send_to_player(
            player_id,
            &ServerPacket::NPCStorage(NPCStorage {}).encode_frame(),
        );
    }

    /// `C.StoreItem`：背包 → 仓库（Crystal `PlayerObject.StoreItem`，`:5376-5456`）。
    ///
    /// 校验顺序与 Crystal 完全一致，任何一步失败都只回一条 `S.StoreItem{Success=false}`。
    pub fn storage_store_item(
        &mut self,
        player_id: u32,
        from: i32,
        to: i32,
    ) -> Result<(), StorageError> {
        match self.store_item_inner(player_id, from, to) {
            Ok(()) => {
                self.send_to_player(
                    player_id,
                    &ServerPacket::StoreItem(StoreItemPacket {
                        from,
                        to,
                        success: true,
                    })
                    .encode_frame(),
                );
                Ok(())
            }
            Err(error) => {
                self.send_to_player(
                    player_id,
                    &ServerPacket::StoreItem(StoreItemPacket {
                        from,
                        to,
                        success: false,
                    })
                    .encode_frame(),
                );
                Err(error)
            }
        }
    }

    fn store_item_inner(&mut self, player_id: u32, from: i32, to: i32) -> Result<(), StorageError> {
        self.check_storage_access(player_id)?;
        let from_index = usize::try_from(from).map_err(|_| StorageError::InventoryIndex {
            index: from,
            size: INVENTORY_SIZE,
        })?;
        if from_index >= INVENTORY_SIZE {
            return Err(StorageError::InventoryIndex {
                index: from,
                size: INVENTORY_SIZE,
            });
        }
        let to_index = usize::try_from(to).map_err(|_| StorageError::StorageIndex {
            index: to,
            size: STORAGE_SIZE,
        })?;
        if to_index >= STORAGE_SIZE {
            return Err(StorageError::StorageIndex {
                index: to,
                size: STORAGE_SIZE,
            });
        }

        let item = self
            .players
            .get(&player_id)
            .and_then(|player| player.inventory.get(from_index).cloned().flatten())
            .ok_or(StorageError::EmptySlot {
                grid: "Inventory",
                index: from,
            })?;
        if cannot_store(self, &item) {
            return Err(StorageError::CannotStore {
                unique_id: item.unique_id,
            });
        }
        if self
            .players
            .get(&player_id)
            .and_then(|player| player.storage.get(to_index))
            .is_some_and(Option::is_some)
        {
            return Err(StorageError::TargetOccupied { index: to });
        }

        // `Account.Storage[to] = temp; Info.Inventory[from] = null; RefreshBagWeight();`
        // （`PlayerObject.cs:5443-5447`）。
        if let Some(player) = self.players.get_mut(&player_id) {
            player.inventory[from_index] = None;
            player.storage[to_index] = Some(item);
        }
        Ok(())
    }

    /// `C.TakeBackItem`：仓库 → 背包（Crystal `PlayerObject.TakeBackItem`，`:5457-5526`）。
    pub fn storage_take_back_item(
        &mut self,
        player_id: u32,
        from: i32,
        to: i32,
    ) -> Result<(), StorageError> {
        match self.take_back_item_inner(player_id, from, to) {
            Ok(()) => {
                self.send_to_player(
                    player_id,
                    &ServerPacket::TakeBackItem(TakeBackItemPacket {
                        from,
                        to,
                        success: true,
                    })
                    .encode_frame(),
                );
                Ok(())
            }
            Err(error) => {
                self.send_to_player(
                    player_id,
                    &ServerPacket::TakeBackItem(TakeBackItemPacket {
                        from,
                        to,
                        success: false,
                    })
                    .encode_frame(),
                );
                Err(error)
            }
        }
    }

    fn take_back_item_inner(
        &mut self,
        player_id: u32,
        from: i32,
        to: i32,
    ) -> Result<(), StorageError> {
        self.check_storage_access(player_id)?;
        let from_index = usize::try_from(from).map_err(|_| StorageError::StorageIndex {
            index: from,
            size: STORAGE_SIZE,
        })?;
        if from_index >= STORAGE_SIZE {
            return Err(StorageError::StorageIndex {
                index: from,
                size: STORAGE_SIZE,
            });
        }
        let to_index = usize::try_from(to).map_err(|_| StorageError::InventoryIndex {
            index: to,
            size: INVENTORY_SIZE,
        })?;
        if to_index >= INVENTORY_SIZE {
            return Err(StorageError::InventoryIndex {
                index: to,
                size: INVENTORY_SIZE,
            });
        }

        let item = self
            .players
            .get(&player_id)
            .and_then(|player| player.storage.get(from_index).cloned().flatten())
            .ok_or(StorageError::EmptySlot {
                grid: "Storage",
                index: from,
            })?;
        // `Info.Inventory[to] == null` 才成功（`:5512-5525`）。
        if self
            .players
            .get(&player_id)
            .and_then(|player| player.inventory.get(to_index))
            .is_some_and(Option::is_some)
        {
            return Err(StorageError::TargetOccupied { index: to });
        }
        if let Some(player) = self.players.get_mut(&player_id) {
            player.storage[from_index] = None;
            player.inventory[to_index] = Some(item);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::encode_slots;

    #[test]
    fn empty_blob_decodes_to_eighty_empty_slots() {
        let slots = decode_storage(&[]).expect("空块");
        assert_eq!(slots.len(), STORAGE_SIZE);
        assert!(slots.iter().all(Option::is_none));
    }

    #[test]
    fn storage_blob_round_trips_and_is_clamped() {
        let item = UserItem {
            unique_id: 7,
            item_index: 42,
            count: 1,
            ..UserItem::default()
        };
        let bytes = encode_storage(&[Some(item.clone())]);
        let slots = decode_storage(&bytes).expect("解码");
        assert_eq!(slots.len(), STORAGE_SIZE);
        assert_eq!(slots[0].as_ref().map(|slot| slot.unique_id), Some(7));

        // 比 80 长的块被截断（本作没有扩充仓库，Crystal 的上限是 160）。
        let long = encode_slots(&vec![Some(item); 90]);
        assert_eq!(decode_storage(&long).expect("解码长块").len(), STORAGE_SIZE);
    }

    #[test]
    fn corrupt_blob_is_an_error() {
        assert!(decode_storage(&[0xFF, 0xFF]).is_err());
    }
}
