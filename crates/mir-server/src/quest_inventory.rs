//! 任务物品栏（Crystal `CharacterInfo.QuestInventory`，40 格）。
//!
//! 与普通背包（[`crate::items`]）分开：任务中收集/被发放的物品进这块栏位，
//! 怪物掉落命中「有未完成收集任务」的物品时会被**截胡**进这里而不是落地
//! （Crystal `MonsterObject.cs:1130-1138` 的 `CheckGroupQuestItem`）。
//!
//! # 对照 Crystal
//!
//! | 本模块 | Crystal | 说明 |
//! |---|---|---|
//! | [`QUEST_INVENTORY_SIZE`] | `CharacterInfo.cs:75` 的 `QuestInventory = new UserItem[40]` | 固定 40 格 |
//! | [`QuestInventory::can_gain`] | `PlayerObject.CanGainQuestItem`（`:7744-7767`） | 满栏时按堆叠余量判断 |
//! | [`QuestInventory::add`] | `PlayerObject.AddQuestItem`（`:957-983`） | 先补堆叠，再找第一个空位 |
//! | [`QuestInventory::take`] | `PlayerObject.TakeQuestItem`（`:7778-7803`） | 跨格扣减并产出 `S.DeleteQuestItem` |
//! | [`QuestInventory::remove_unneeded`] | `PlayerObject.RecalculateQuestBag`（`:11592-11630`） | 任务结束后清掉不再需要的任务物品 |
//! | [`gained_quest_item_frame`] | `S.GainedQuestItem`（`ServerPackets.cs` 205） | 加物品时逐件下发 |
//! | [`delete_quest_item_frame`] | `S.DeleteQuestItem`（206） | 扣物品时按 `UniqueID`/`Count` 下发 |
//!
//! # 存档
//!
//! `characters.quest_inventory`（迁移 `0012_character_quest_inventory.sql`），编码与
//! `characters.storage` 同风格（Crystal `UserItem[]` 整块，`CharacterInfo.cs:449-456`）：
//! `int32 槽位数` + 每槽 `bool 有物品?` + `UserItem`。空栏是零长度 BYTEA。
//!
//! # 与 Crystal 的差异
//!
//! * `add` 是**原子**的：放不下就整件拒绝并保持原样（Crystal 假定调用方先 `CanGainQuestItem`，
//!   放不下时会把剩余的堆叠悄悄丢掉）；
//! * 组队截胡时每个队员各拿一件**独立克隆**（见 `World::check_group_quest_item`），
//!   Crystal 把同一个 `UserItem` 实例塞进多个队员的任务栏（共享 `UniqueID`，属于 bug）；
//! * 登录时下发整栏：`mir_protocol::manual::server::UserInformation` 已有
//!   `quest_inventory: Option<Vec<Option<UserItem>>>` 字段（对应 Crystal 同名字段），
//!   由 `session.rs` 在 `on_start_game` 里填真值；`S.GainedQuestItem` / `S.DeleteQuestItem`
//!   只负责之后的变化。

use std::collections::{BTreeMap, BTreeSet};

use mir_db::quest_script::QuestScript;
use mir_protocol::ServerPacket;
use mir_protocol::data::{ItemInfo, UserItem};
use mir_protocol::packets::server::{DeleteQuestItem, GainedQuestItem};

use crate::items::{ItemCodecError, decode_slots, encode_slots, fit_slots};
use crate::quests::{QuestProgress, item_matches_task};
use crate::world::World;

/// 任务物品栏格数：Crystal `CharacterInfo.cs:75` 的 `UserItem[40]`。
pub const QUEST_INVENTORY_SIZE: usize = 40;

/// 一次扣减任务物品产生的删除事件（对应一帧 `S.DeleteQuestItem`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestItemRemoval {
    /// 被扣物品实例的唯一编号（Crystal `UserItem.UniqueID`）。
    pub unique_id: u64,
    /// 从这一格扣掉的数量。
    pub count: u16,
}

/// 任务物品栏：固定 40 格，每格可空。
///
/// 不变量：`slots.len() == QUEST_INVENTORY_SIZE`（[`QuestInventory::new`] 与
/// [`QuestInventory::decode`] 都保证；不提供绕过它的公开构造）。
#[derive(Debug, Clone, PartialEq)]
pub struct QuestInventory {
    slots: Vec<Option<UserItem>>,
}

impl Default for QuestInventory {
    fn default() -> Self {
        Self::new()
    }
}

impl QuestInventory {
    /// 空任务物品栏（40 格全空）。
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: vec![None; QUEST_INVENTORY_SIZE],
        }
    }

    /// 全部格子（只读；空槽是 `None`）。
    #[must_use]
    pub fn slots(&self) -> &[Option<UserItem>] {
        &self.slots
    }

    /// 某一格；越界返回 `None`。
    #[must_use]
    pub fn slot(&self, index: usize) -> Option<&UserItem> {
        self.slots.get(index)?.as_ref()
    }

    /// 是否一件任务物品都没有。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    /// 已占用的格数。
    #[must_use]
    pub fn used_slots(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    /// 空格数。
    #[must_use]
    pub fn free_space(&self) -> usize {
        QUEST_INVENTORY_SIZE - self.used_slots()
    }

    /// 是否已满。
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.used_slots() == QUEST_INVENTORY_SIZE
    }

    /// 某个物品模板的总数量。
    #[must_use]
    pub fn count_of(&self, item_index: i32) -> u16 {
        self.slots
            .iter()
            .flatten()
            .filter(|item| item.item_index == item_index)
            .fold(0u16, |total, item| total.saturating_add(item.count))
    }

    /// 按物品模板聚合的数量表（供任务收集目标重算）。
    #[must_use]
    pub fn counts_by_index(&self) -> BTreeMap<i32, u16> {
        let mut counts: BTreeMap<i32, u16> = BTreeMap::new();
        for item in self.slots.iter().flatten() {
            let entry = counts.entry(item.item_index).or_insert(0);
            *entry = entry.saturating_add(item.count);
        }
        counts
    }

    /// 能否整件收下 `count` 个 `item_index`（Crystal `CanGainQuestItem`，`PlayerObject.cs:7744-7767`）。
    ///
    /// 有空格 → 一定能；没空格时只可能靠同模板的堆叠余量收下（`stack_size > 1`）。
    #[must_use]
    pub fn can_gain(&self, item_index: i32, count: u16, stack_size: u16) -> bool {
        if count == 0 {
            return true;
        }
        if self.free_space() > 0 {
            return true;
        }
        if stack_size <= 1 {
            return false;
        }
        let mut remaining = count;
        for item in self.slots.iter().flatten() {
            if item.item_index != item_index || item.count >= stack_size {
                continue;
            }
            let room = stack_size - item.count;
            if remaining <= room {
                return true;
            }
            remaining -= room;
        }
        false
    }

    /// 收下一个物品实例（Crystal `AddQuestItem`，`PlayerObject.cs:957-983`）。
    ///
    /// 先填已有堆叠，再放第一个空格。**原子**：收不下时整件拒绝并返回 `false`，
    /// 栏位保持原样（Crystal 会把剩余堆叠丢掉，见模块文档）。
    pub fn add(&mut self, item: &UserItem, stack_size: u16) -> bool {
        if !self.can_gain(item.item_index, item.count, stack_size) {
            return false;
        }
        let mut remaining = item.count;
        if stack_size > 1 {
            for slot in &mut self.slots {
                if remaining == 0 {
                    break;
                }
                let Some(existing) = slot else { continue };
                if existing.item_index != item.item_index || existing.count >= stack_size {
                    continue;
                }
                let room = stack_size - existing.count;
                let moved = remaining.min(room);
                existing.count += moved;
                remaining -= moved;
            }
        }
        if remaining > 0 {
            let Some(index) = self.slots.iter().position(Option::is_none) else {
                // `can_gain` 已经保证放得下，走到这里说明不变量被破坏。
                debug_assert!(false, "任务物品栏已满但 can_gain 判定可收");
                return false;
            };
            let mut cloned = item.clone();
            cloned.count = remaining;
            self.slots[index] = Some(cloned);
        }
        true
    }

    /// 扣掉 `count` 个某模板的任务物品（Crystal `TakeQuestItem`，`PlayerObject.cs:7778-7803`）。
    ///
    /// 跨格扣减，每格产出一条 [`QuestItemRemoval`]（调用方逐条发 `S.DeleteQuestItem`）。
    /// 不够时能扣多少扣多少（Crystal 的行为）。
    pub fn take(&mut self, item_index: i32, count: u16) -> Vec<QuestItemRemoval> {
        let mut removals = Vec::new();
        let mut remaining = count;
        for slot in &mut self.slots {
            if remaining == 0 {
                break;
            }
            let Some(item) = slot else { continue };
            if item.item_index != item_index {
                continue;
            }
            if remaining > item.count {
                removals.push(QuestItemRemoval {
                    unique_id: item.unique_id,
                    count: item.count,
                });
                remaining -= item.count;
                *slot = None;
                continue;
            }
            removals.push(QuestItemRemoval {
                unique_id: item.unique_id,
                count: remaining,
            });
            if remaining == item.count {
                *slot = None;
            } else {
                item.count -= remaining;
            }
            remaining = 0;
        }
        removals
    }

    /// 清掉不再被任何进行中任务需要的任务物品（Crystal `RecalculateQuestBag`，`PlayerObject.cs:11592-11630`）。
    ///
    /// `needed` 返回该模板是否仍需要（收集目标或携带物品）。按 Crystal 的顺序**从后往前**扫，
    /// 每清一格产出一条 [`QuestItemRemoval`]。
    pub fn remove_unneeded(&mut self, needed: impl Fn(i32) -> bool) -> Vec<QuestItemRemoval> {
        let mut removals = Vec::new();
        for slot in self.slots.iter_mut().rev() {
            let Some(item) = slot else { continue };
            if needed(item.item_index) {
                continue;
            }
            removals.push(QuestItemRemoval {
                unique_id: item.unique_id,
                count: item.count,
            });
            *slot = None;
        }
        removals
    }

    /// 编码成存档块（与 `characters.storage` 同格式，Crystal `CharacterInfo.cs:449-456`）。
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        encode_slots(&self.slots)
    }

    /// 从存档块解码；空块（新角色）得到空任务物品栏。
    ///
    /// 槽位数比 40 多时按 [`fit_slots`] 截断（并记 warning），比 40 少时补空。
    ///
    /// # Errors
    /// 字节流损坏或槽位数超过背包上限（86）时返回 [`ItemCodecError`]。
    pub fn decode(bytes: &[u8]) -> Result<Self, ItemCodecError> {
        if bytes.is_empty() {
            return Ok(Self::new());
        }
        let slots = fit_slots(decode_slots(bytes)?, QUEST_INVENTORY_SIZE);
        Ok(Self { slots })
    }
}

/// 构造 `S.GainedQuestItem`(205) 帧（Crystal `PlayerObject.GainQuestItem`，`PlayerObject.cs:7768-7777`）。
#[must_use]
pub fn gained_quest_item_frame(item: &UserItem) -> Vec<u8> {
    ServerPacket::GainedQuestItem(GainedQuestItem { item: item.clone() }).encode_frame()
}

/// 构造 `S.DeleteQuestItem`(206) 帧（Crystal `PlayerObject.TakeQuestItem`，`PlayerObject.cs:7778-7803`）。
#[must_use]
pub fn delete_quest_item_frame(unique_id: u64, count: u16) -> Vec<u8> {
    ServerPacket::DeleteQuestItem(DeleteQuestItem { unique_id, count }).encode_frame()
}

/// 一条任务进度是否还需要这个物品（Crystal `QuestProgressInfo.NeedItem`，`QuestProgressInfo.cs:366-369`）。
///
/// 条件：任务在进行中、脚本里有匹配的收集目标、且该目标还没收满（`item_collected < count`）。
/// 怪物掉落截胡只对满足这条的物品生效——已完成/已收满的目标不再截胡。
#[must_use]
pub fn task_needs_item(script: &QuestScript, progress: &QuestProgress, item_name: &str) -> bool {
    progress.is_in_progress()
        && script.item_tasks.iter().any(|task| {
            item_matches_task(&task.item, item_name)
                && i32::from(progress.item_count_of(&task.item)) < i32::from(task.count)
        })
}

// ---------------------------------------------------------------------------
// World：任务物品栏的运行时状态与结算
// ---------------------------------------------------------------------------

/// 组队截胡的判定距离：Crystal `CheckGroupQuestItem` 用 `Globals.DataRange = 16`
/// （`PlayerObject.cs:11513-11519`），与本作 [`crate::world::VIEW_RANGE`] 一致。
const GROUP_QUEST_ITEM_RANGE: i32 = crate::world::VIEW_RANGE;

impl World {
    /// 装入某个玩家的任务物品栏（登录时由会话解码 `characters.quest_inventory` 后调用）。
    pub fn set_quest_inventory(&mut self, player_id: u32, inventory: QuestInventory) {
        self.quest_inventories.insert(player_id, inventory);
    }

    /// 取某个玩家任务物品栏的只读视图（不在世界里返回 `None`）。
    #[must_use]
    pub fn quest_inventory(&self, player_id: u32) -> Option<&QuestInventory> {
        self.quest_inventories.get(&player_id)
    }

    /// 取走某个玩家的任务物品栏（下线存盘用；取走后世界侧不再持有）。
    #[must_use]
    pub fn take_quest_inventory(&mut self, player_id: u32) -> QuestInventory {
        self.quest_inventories
            .remove(&player_id)
            .unwrap_or_default()
    }

    /// 任务物品栏 + 普通背包里按模板名（归一化）聚合的数量。
    ///
    /// 与 Crystal 的差异：Crystal 的收集进度只统计 `Info.QuestInventory`
    /// （`QuestProgressInfo.ProcessItem`，`:398-408`）；本作把普通背包也算上，
    /// 这样改版前/交易来的任务物品仍然算数（旧存档兼容，见模块文档）。
    #[must_use]
    pub fn quest_item_counts(&self, player_id: u32) -> BTreeMap<String, u16> {
        let mut counts = self.inventory_item_counts(player_id);
        let Some(inventory) = self.quest_inventories.get(&player_id) else {
            return counts;
        };
        for item in inventory.slots().iter().flatten() {
            let Some(info) = self.item_info(item.item_index) else {
                continue;
            };
            let entry = counts
                .entry(crate::gamedata::normalize_item_name(&info.name))
                .or_insert(0);
            *entry = entry.saturating_add(item.count);
        }
        counts
    }

    /// 批量检查能否收下这些任务物品（接取任务发 `[@CARRYITEMS]` 前用）。
    ///
    /// `items` 是 `(物品模板, 数量)`；逐条模拟放进任务物品栏，任一条放不下返回 `false`。
    #[must_use]
    pub fn can_gain_quest_items(&self, player_id: u32, items: &[(ItemInfo, u16)]) -> bool {
        let mut staging = self
            .quest_inventories
            .get(&player_id)
            .cloned()
            .unwrap_or_default();
        for (info, count) in items {
            let item = UserItem {
                item_index: info.index,
                count: *count,
                ..UserItem::default()
            };
            if !staging.add(&item, info.stack_size) {
                return false;
            }
        }
        true
    }

    /// 收下一件任务物品并下发 `S.GainedQuestItem`(205)（Crystal `GainQuestItem`，`:7768-7777`）。
    ///
    /// 放不下返回 `false`（调用方决定是拒绝接取还是让它落地）。
    pub fn gain_quest_item(&mut self, player_id: u32, item: &UserItem) -> bool {
        let Some(info) = self.item_info(item.item_index).cloned() else {
            return false;
        };
        // 任务物品栏按需创建（会话登录时会 `set_quest_inventory`；测试/工具路径可能没有）。
        let inventory = self.quest_inventories.entry(player_id).or_default();
        if !inventory.add(item, info.stack_size) {
            return false;
        }
        let frame = gained_quest_item_frame(item);
        self.send_to_player(player_id, &frame);
        true
    }

    /// 截胡判定 + 入栏：这件掉落是否应该进任务物品栏（Crystal `CheckNeedQuestItem`，
    /// `PlayerObject.cs:11539-11561`）。
    ///
    /// 只有**有未完成收集任务**且任务物品栏放得下时才收下；收下后重算收集进度并发
    /// `S.ChangeQuest{Update}`。
    pub fn check_need_quest_item(&mut self, player_id: u32, item: &UserItem) -> bool {
        let Some(info) = self.item_info(item.item_index).cloned() else {
            return false;
        };
        let needed = self.players.get(&player_id).is_some_and(|player| {
            player
                .quests
                .iter()
                .filter(|progress| progress.is_in_progress())
                .any(|progress| {
                    self.quest_definition(progress.quest_index)
                        .is_some_and(|definition| {
                            task_needs_item(&definition.script, progress, &info.name)
                        })
                })
        });
        if !needed {
            return false;
        }
        let inventory = self.quest_inventories.entry(player_id).or_default();
        let added = inventory.add(item, info.stack_size);
        if !added {
            return false;
        }
        let frame = gained_quest_item_frame(item);
        self.send_to_player(player_id, &frame);
        self.refresh_quest_items(player_id);
        true
    }

    /// 怪物掉落的组队截胡：Crystal `CheckGroupQuestItem`（`PlayerObject.cs:11509-11537`）。
    ///
    /// * 单人不组队：只检查击杀者本人；
    /// * 组队：检查同图、`Globals.DataRange` 内、活着的队员（含本人）；
    /// * 任一队员收下就返回 `true`，调用方据此**跳过落地**（`MonsterObject.cs:1134-1137`）。
    ///
    /// 与 Crystal 的差异：每个队员各拿一件**独立克隆**（新 `UniqueID`），
    /// Crystal 把同一个 `UserItem` 实例塞进多个队员的任务栏（共享 `UniqueID` 是 bug）。
    pub fn check_group_quest_item(&mut self, owner: u32, item: &UserItem) -> bool {
        let Some(owner_player) = self.players.get(&owner) else {
            return false;
        };
        let map = owner_player.map.clone();
        let position = owner_player.position;
        let members: Vec<u32> = if self.in_group(owner) {
            self.group_members(owner)
                .iter()
                .copied()
                .filter(|member| {
                    self.players.get(member).is_some_and(|player| {
                        !player.dead
                            && player.map == map
                            && player.position.in_range(position, GROUP_QUEST_ITEM_RANGE)
                    })
                })
                .collect()
        } else {
            vec![owner]
        };
        let mut intercepted = false;
        for member in members {
            // 保留掉落实例的数量/耐久/附加属性（Crystal 传的是真实 `UserItem`），
            // 只给每个队员换一个新的 `UniqueID`，避免多个任务栏共享同一实例。
            let mut copy = item.clone();
            copy.unique_id = self.item_uids.next();
            if self.check_need_quest_item(member, &copy) {
                intercepted = true;
            }
        }
        intercepted
    }

    /// 从任务物品栏扣掉 `count` 个模板名为 `item_name` 的物品
    /// （Crystal `TakeQuestItem`，`PlayerObject.cs:7778-7803`），逐条发 `S.DeleteQuestItem`(206)。
    ///
    /// 任务物品栏不够时，剩下的从普通背包扣（兼容旧存档/交易来的物品），返回总扣除数。
    pub fn take_quest_items(&mut self, player_id: u32, item_name: &str, count: u16) -> u16 {
        let Some(info) = self.item_info_by_name(item_name).cloned() else {
            return 0;
        };
        let removals = self
            .quest_inventories
            .entry(player_id)
            .or_default()
            .take(info.index, count);
        let mut removed = 0u16;
        for removal in removals {
            removed = removed.saturating_add(removal.count);
            let frame = delete_quest_item_frame(removal.unique_id, removal.count);
            self.send_to_player(player_id, &frame);
        }
        if removed < count {
            removed = removed.saturating_add(self.take_inventory_items(
                player_id,
                item_name,
                count - removed,
            ));
        }
        removed
    }

    /// 清掉不再被任何进行中任务需要的任务物品（Crystal `RecalculateQuestBag`，
    /// `PlayerObject.cs:11592-11630`），每清一格发一帧 `S.DeleteQuestItem`。
    ///
    /// 返回清掉的格数。交付/放弃任务后调用。
    pub fn recalculate_quest_bag(&mut self, player_id: u32) -> usize {
        let Some(player) = self.players.get(&player_id) else {
            return 0;
        };
        let mut needed: BTreeSet<i32> = BTreeSet::new();
        for progress in player
            .quests
            .iter()
            .filter(|progress| progress.is_in_progress())
        {
            let Some(definition) = self.quest_definition(progress.quest_index) else {
                continue;
            };
            for task in &definition.script.carry_items {
                if let Some(info) = self.item_info_by_name(&task.item) {
                    needed.insert(info.index);
                }
            }
            for task in &definition.script.item_tasks {
                if let Some(info) = self.item_info_by_name(&task.item) {
                    needed.insert(info.index);
                }
            }
        }
        let removals = self
            .quest_inventories
            .entry(player_id)
            .or_default()
            .remove_unneeded(|item_index| needed.contains(&item_index));
        let count = removals.len();
        for removal in removals {
            let frame = delete_quest_item_frame(removal.unique_id, removal.count);
            self.send_to_player(player_id, &frame);
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use mir_db::quest_script::QuestItemTask;

    use super::*;
    use crate::quests::QuestProgress;

    fn item(unique_id: u64, item_index: i32, count: u16) -> UserItem {
        UserItem {
            unique_id,
            item_index,
            count,
            ..UserItem::default()
        }
    }

    fn script_with_item_task(item_name: &str, count: u16) -> QuestScript {
        QuestScript {
            item_tasks: vec![QuestItemTask {
                item: item_name.to_owned(),
                count,
                message: String::new(),
            }],
            ..QuestScript::default()
        }
    }

    /// 从帧里取包序号与负载（帧格式：u16 长度 + i16 编号 + 负载）。
    fn frame_parts(frame: &[u8]) -> (i16, &[u8]) {
        let index = i16::from_le_bytes([frame[2], frame[3]]);
        (index, &frame[4..])
    }

    #[test]
    fn new_inventory_has_forty_empty_slots() {
        let inventory = QuestInventory::new();
        assert_eq!(inventory.slots().len(), QUEST_INVENTORY_SIZE);
        assert_eq!(QUEST_INVENTORY_SIZE, 40);
        assert!(inventory.is_empty());
        assert_eq!(inventory.used_slots(), 0);
        assert_eq!(inventory.free_space(), 40);
        assert!(!inventory.is_full());
    }

    #[test]
    fn add_uses_first_empty_slot() {
        let mut inventory = QuestInventory::new();
        assert!(inventory.add(&item(1, 100, 1), 1));
        assert!(inventory.add(&item(2, 101, 1), 1));
        assert_eq!(inventory.slot(0).map(|item| item.unique_id), Some(1));
        assert_eq!(inventory.slot(1).map(|item| item.unique_id), Some(2));
        assert_eq!(inventory.free_space(), 38);
        assert!(!inventory.is_empty());
    }

    #[test]
    fn add_stacks_into_existing_stack_before_new_slot() {
        let mut inventory = QuestInventory::new();
        assert!(inventory.add(&item(1, 100, 3), 10));
        assert!(inventory.add(&item(2, 100, 4), 10));
        // 先补第一格（3 + 4 = 7），不占新格。
        assert_eq!(inventory.used_slots(), 1);
        assert_eq!(inventory.count_of(100), 7);
        assert_eq!(inventory.slot(0).map(|item| item.count), Some(7));
        // 再收 6 个：第一格补到 10，剩下 3 个进第二格（克隆原实例的 unique_id 不变）。
        assert!(inventory.add(&item(3, 100, 6), 10));
        assert_eq!(inventory.used_slots(), 2);
        assert_eq!(inventory.slot(0).map(|item| item.count), Some(10));
        assert_eq!(inventory.slot(1).map(|item| item.count), Some(3));
        assert_eq!(inventory.count_of(100), 13);
    }

    #[test]
    fn add_rejects_when_full_without_mutating() {
        let mut inventory = QuestInventory::new();
        // 40 个不堆叠物品把栏位占满。
        for index in 0..QUEST_INVENTORY_SIZE {
            assert!(inventory.add(&item(index as u64 + 1, 100, 1), 1));
        }
        assert!(inventory.is_full());
        assert!(!inventory.add(&item(99, 100, 1), 1));
        assert_eq!(inventory.used_slots(), QUEST_INVENTORY_SIZE);
        assert_eq!(inventory.count_of(100), 40);
    }

    #[test]
    fn can_gain_accepts_partial_stack_when_no_free_slot() {
        let mut inventory = QuestInventory::new();
        for index in 0..QUEST_INVENTORY_SIZE {
            // 全部塞满「另一种」物品，最后一格留一个可堆叠的空隙。
            let count = if index == 0 { 8 } else { 10 };
            assert!(inventory.add(&item(index as u64 + 1, 200, count), 10));
        }
        assert!(inventory.is_full());
        assert!(inventory.can_gain(200, 2, 10), "第一格还有 2 个余量");
        assert!(!inventory.can_gain(200, 3, 10), "余量只有 2");
        assert!(!inventory.can_gain(300, 1, 10), "别的模板没有余量");
        assert!(inventory.can_gain(300, 0, 10), "收 0 个永远成功");
    }

    #[test]
    fn take_removes_across_slots_and_reports_deletions() {
        let mut inventory = QuestInventory::new();
        // 第一格堆满 10 个，第二格 5 个（同模板），第三格是别的物品。
        assert!(inventory.add(&item(11, 100, 10), 10));
        assert!(inventory.add(&item(12, 100, 5), 10));
        assert!(inventory.add(&item(13, 200, 1), 1));
        assert_eq!(inventory.count_of(100), 15);
        let removals = inventory.take(100, 12);
        assert_eq!(
            removals,
            vec![
                QuestItemRemoval {
                    unique_id: 11,
                    count: 10
                },
                QuestItemRemoval {
                    unique_id: 12,
                    count: 2
                },
            ],
            "drain the whole stack first, then part of the next"
        );
        assert_eq!(inventory.count_of(100), 3);
        assert_eq!(inventory.slot(0), None);
        assert_eq!(inventory.slot(1).map(|item| item.count), Some(3));
        assert_eq!(inventory.count_of(200), 1, "别的模板不受影响");
        // 扣得比拥有的多：能扣多少扣多少。
        let removals = inventory.take(100, 99);
        assert_eq!(
            removals,
            vec![QuestItemRemoval {
                unique_id: 12,
                count: 3
            }]
        );
        assert_eq!(inventory.count_of(100), 0);
    }

    #[test]
    fn remove_unneeded_drops_only_unrequired_items() {
        let mut inventory = QuestInventory::new();
        assert!(inventory.add(&item(21, 100, 2), 10));
        assert!(inventory.add(&item(22, 200, 1), 1));
        let removals = inventory.remove_unneeded(|item_index| item_index == 100);
        assert_eq!(
            removals,
            vec![QuestItemRemoval {
                unique_id: 22,
                count: 1
            }]
        );
        assert_eq!(inventory.count_of(100), 2);
        assert_eq!(inventory.count_of(200), 0);
    }

    #[test]
    fn encode_decode_roundtrip() {
        let mut inventory = QuestInventory::new();
        assert!(inventory.add(&item(31, 100, 2), 10));
        assert!(inventory.add(&item(32, 101, 1), 1));
        let bytes = inventory.encode();
        assert!(bytes.len() > 4, "存档块应当包含槽位数据");
        let decoded = QuestInventory::decode(&bytes).expect("存档块应当可以解码");
        assert_eq!(decoded, inventory);
        assert_eq!(decoded.slots().len(), QUEST_INVENTORY_SIZE);
    }

    #[test]
    fn decode_empty_block_is_empty_and_oversize_is_truncated() {
        assert_eq!(
            QuestInventory::decode(&[]).expect("空块合法"),
            QuestInventory::new()
        );
        // 41 格（模拟旧存档/异常数据）→ 截断到 40。
        let slots: Vec<Option<UserItem>> = (0..41)
            .map(|index| Some(item(index as u64 + 1, 100, 1)))
            .collect();
        let bytes = encode_slots(&slots);
        let decoded = QuestInventory::decode(&bytes).expect("41 格应当容忍");
        assert_eq!(decoded.slots().len(), QUEST_INVENTORY_SIZE);
        assert_eq!(decoded.count_of(100), 40);
    }

    #[test]
    fn gained_and_delete_frames_use_crystal_indices() {
        let gained = gained_quest_item_frame(&item(41, 100, 2));
        let (index, payload) = frame_parts(&gained);
        assert_eq!(index, 205, "S.GainedQuestItem");
        let mut reader = mir_protocol::PacketReader::new(payload);
        let decoded = GainedQuestItem::decode(&mut reader).expect("负载应当可解码");
        assert_eq!(decoded.item, item(41, 100, 2));

        let deleted = delete_quest_item_frame(41, 2);
        let (index, payload) = frame_parts(&deleted);
        assert_eq!(index, 206, "S.DeleteQuestItem");
        let mut reader = mir_protocol::PacketReader::new(payload);
        let decoded = DeleteQuestItem::decode(&mut reader).expect("负载应当可解码");
        assert_eq!(decoded.unique_id, 41);
        assert_eq!(decoded.count, 2);
    }

    #[test]
    fn task_needs_item_only_for_unfinished_matching_task() {
        let script = script_with_item_task("Egg", 3);
        let mut progress = QuestProgress::new(7, 1_000, 0, &script);
        assert!(task_needs_item(&script, &progress, "egg"), "大小写不敏感");
        assert!(task_needs_item(&script, &progress, " Egg "), "归一化空白");
        assert!(!task_needs_item(&script, &progress, "Meat"), "别的物品");

        // 收满之后不再截胡。
        progress.item_collected[0].1 = 3;
        assert!(!task_needs_item(&script, &progress, "Egg"));
        // 任务完成后不再截胡（即使计数没满）。
        progress.item_collected[0].1 = 1;
        progress.state = crate::quests::QuestProgressState::Completed;
        assert!(!task_needs_item(&script, &progress, "Egg"));
        // 没接这个任务（不在进行中）也不截胡。
        let not_started = QuestProgress::default();
        assert!(!task_needs_item(&script, &not_started, "Egg"));
    }
}
