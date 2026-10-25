//! 施法耗材（护身符 / 毒药）的查找与扣除原语。
//!
//! 本批（R52 B1）只把 Crystal 的两个耗材动作移植成**世界侧原语**，还不接技能：
//! `PoisonCloud` / `TrapHexagon` 的接线、`magic.rs` 的 `Unsupported` 改动属于 B2/B3。
//!
//! Crystal 对照（`github/Crystal/Server/MirObjects/HumanObject.cs`）：
//!
//! * `GetAmulet(int count, int shape = 0)`（`:6859-6871`）：从 `Info.Equipment` 的
//!   0 号槽开始**逐槽递增**，返回第一件 `Type == Amulet && Shape == shape && Count >= count`
//!   的装备；**不看耐久**，也**不跨堆合并**（单堆必须自己够数）。
//! * `GetPoison(int count, byte shape = 0)`（`:6873-6890`）：同一个循环，`shape == 0` 时
//!   匹配绿毒/红毒（`Shape == 1 || Shape == 2`），否则精确匹配 `Shape`。
//! * `ConsumeItem(UserItem item, byte cost)`（`:1346-1364`）：先 `item.Count -= cost`
//!   并入队 `S.DeleteItem{UniqueID, Count = cost}`（**部分扣除也发包**）；`Count` 归零
//!   才摘槽。摘槽顺序是 `Equipment`（含 `Slots` 子项）→ `Inventory`。
//!
//! 本作装备栏 14 槽（[`crate::items::EQUIPMENT_SLOTS`]），其中槽 6（`BraceletR`）与
//! 槽 9（`Amulet`）**都能放护身符**（[`crate::items::slot_accepts`] 的判断来自
//! `HumanObject.cs:7689-7692`），所以搜索必须扫全 14 槽，不能只查 9。
//!
//! 与 Crystal 的有意差异：
//!
//! * 只碰**根装备槽**，不扫装备的 `Slots` 子槽：堆叠耗材不会进镶嵌子槽，本作也还没做
//!   镶嵌；`ConsumeItem` 覆盖 `Slots` 是给带子项的武器/衣服用的，照抄反而可能误摘。
//! * `GetPoison(count, 0)` 的「绿毒或红毒」兜底留给 B2 调用方决定；`PoisonCloud` 在
//!   Crystal 里明确传 `shape = 1`（`:4546`），本原语按精确 `shape` 匹配（与 `GetAmulet`
//!   的语义一致，否则护身符 `shape = 0` 会和毒药撞车）。

use mir_core::enums::ItemType;
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::DeleteItem as DeleteItemPacket;

use crate::items::EQUIPMENT_SLOTS;
use crate::world::World;

// B1 只落原语、不接线：真正使用它们的是 B2/B3 的 `magic.rs` 耗材改造，所以这里显式
// 允许「暂未被生产代码调用」，否则 `-D warnings` 会把两个 `pub(crate)` 方法报成 dead_code。
// B2 把 `PoisonCloud`/`TrapHexagon` 接上之后删掉这个 `allow` 即可。
#[allow(dead_code)]
impl World {
    /// 在玩家的**根装备槽**里按槽号递增找一堆够数的施法耗材。
    ///
    /// * `shape` 精确匹配模板的 `Shape`（护身符 `0`、绿毒 `1`、红毒 `2`）；
    /// * 只认单堆 `Count >= count`，不跨槽合并、不回退背包；
    /// * 不看耐久（`0` 耐久的护身符照样能烧，Crystal 的 `GetAmulet` 就没有耐久判断）；
    /// * `count == 0` 返回 `None`：0 消耗不该走到这里，避免空扣出 `S.DeleteItem{Count:0}`。
    ///
    /// 返回槽号而不是物品本身，是为了让调用方先做「双材料共同预检」
    /// （[`World::consume_spell_material`] 需要 `&mut World`，两次 `find` 之间不会有
    /// 别的代码插进来，所以先 find 后 consume 在同一把世界锁内是原子的）。
    pub(crate) fn find_spell_material(
        &self,
        player_id: u32,
        count: u32,
        shape: u16,
    ) -> Option<usize> {
        if count == 0 {
            return None;
        }
        let player = self.players.get(&player_id)?;
        player
            .equipment
            .iter()
            .take(EQUIPMENT_SLOTS)
            .position(|slot| {
                slot.as_ref().is_some_and(|item| {
                    self.item_info(item.item_index).is_some_and(|info| {
                        info.item_type == ItemType::Amulet
                            && u16::try_from(info.shape).is_ok_and(|info_shape| info_shape == shape)
                            && u32::from(item.count) >= count
                    })
                })
            })
    }

    /// 从 `slot` 槽扣掉 `count` 张施法耗材；成功返回 `true`。
    ///
    /// 语义对应 Crystal `ConsumeItem`（`HumanObject.cs:1346-1364`）：
    ///
    /// * 部分扣数时槽位保留，并给本人发一帧 `S.DeleteItem{UniqueID, Count = count}`；
    /// * 扣到 0 时把槽位摘成 `None`，同样只发一帧 `S.DeleteItem`；
    /// * 失败（槽号错、模板不是护身符/毒药、数量不够）返回 `false` 且**不发任何包**。
    ///
    /// `count == 0` 视为失败（见 [`World::find_spell_material`] 的同款理由）。
    pub(crate) fn consume_spell_material(
        &mut self,
        player_id: u32,
        slot: usize,
        count: u32,
    ) -> bool {
        if count == 0 {
            return false;
        }
        let Some(item) = self
            .players
            .get(&player_id)
            .and_then(|player| player.equipment.get(slot))
            .and_then(Option::as_ref)
            .cloned()
        else {
            return false;
        };
        // 扣之前再确认槽里仍是「护身符/毒药 + 够数」：`find` 与 `consume` 之间没有
        // 别的代码能插进来（两个方法都要 `&mut World`），这里兜的是调用方传错槽号。
        let is_amulet = self
            .item_info(item.item_index)
            .is_some_and(|info| info.item_type == ItemType::Amulet);
        if !is_amulet || u32::from(item.count) < count {
            return false;
        }
        let Ok(count) = u16::try_from(count) else {
            // 单堆上限是 `u16`（`UserItem.count`），上面的 `>= count` 已保证成功。
            return false;
        };
        let Some(player) = self.players.get_mut(&player_id) else {
            return false;
        };
        let Some(stored) = player.equipment.get_mut(slot).and_then(Option::as_mut) else {
            return false;
        };
        stored.count -= count;
        let exhausted = stored.count == 0;
        let unique_id = stored.unique_id;
        if exhausted {
            player.equipment[slot] = None;
        }
        // Crystal `ConsumeItem`：部分扣除也入队，整次消耗只发一包。
        self.send_to_player(
            player_id,
            &ServerPacket::DeleteItem(DeleteItemPacket { unique_id, count }).encode_frame(),
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use mir_core::Point;
    use mir_core::enums::{MirClass, MirDirection, MirGender};
    use mir_db::MirDb;
    use mir_protocol::data::{ItemInfo, UserItem};
    use tokio::sync::mpsc;

    use super::*;
    use crate::gamedata::GameData;
    use crate::magic::MagicState;

    /// 测试护身符模板编号。
    const AMULET: i32 = 101;
    /// 测试绿毒（`Shape = 1`）模板编号。
    const POISON_GREEN: i32 = 102;
    /// 测试红毒（`Shape = 2`）模板编号。
    const POISON_RED: i32 = 103;

    fn item_template(index: i32, name: &str, shape: i16, stack_size: u16) -> ItemInfo {
        ItemInfo {
            index,
            name: name.to_owned(),
            item_type: ItemType::Amulet,
            shape,
            stack_size,
            // 有耐久模板：用来证明「0 耐久仍可耗」不是因为模板没耐久。
            durability: 100,
            ..ItemInfo::default()
        }
    }

    /// 30×30 全可走测试地图（与 `magic.rs`/`world.rs` 的测试地图一致）。
    fn test_map(name: &str) -> Arc<mir_world::Map> {
        mir_world::Map {
            width: 30,
            height: 30,
            cells: vec![mir_world::MapCell::default(); 900],
            file_name: name.to_owned(),
            version: mir_world::MapVersion::V0,
            doors: Vec::new(),
        }
        .into()
    }

    /// 自带三种耗材模板与一张空地地图的世界。
    fn test_world() -> World {
        let db = MirDb {
            items: vec![
                item_template(AMULET, "护身符", 0, 300),
                item_template(POISON_GREEN, "绿毒", 1, 200),
                item_template(POISON_RED, "红毒", 2, 200),
            ],
            ..MirDb::default()
        };
        let mut world = World::new(999);
        world.set_game_data(Arc::new(GameData::from_db(db, None)));
        world.insert_map("0", test_map("0"));
        world.advance_clock(0);
        world
    }

    /// 测试用玩家（字段与 `magic.rs` 的测试辅助一致）。
    fn test_player(
        object_id: u32,
        map: &str,
        position: Point,
        level: u16,
        dead: bool,
    ) -> (crate::world::Player, mpsc::UnboundedReceiver<Vec<u8>>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let stats = crate::combat::player_base_stats(level, MirClass::Warrior);
        (
            crate::world::Player {
                object_id,
                session_id: u64::from(object_id),
                character_id: i64::from(object_id),
                name: format!("玩家{object_id}"),
                map: map.to_owned(),
                position,
                direction: MirDirection::Down,
                level,
                experience: 0,
                max_experience: 0,
                class: MirClass::Warrior,
                gender: MirGender::Male,
                hair: 1,
                hp: stats.hp,
                mp: stats.mp,
                stats,
                action_time: 0,
                attack_time: 0,
                struck_time: 0,
                regen_time: 0,
                death_time: 0,
                revive_time: 0,
                cell_time: 0,
                step_counter: 0,
                run_counter: 0,
                run_time: 0,
                dead,
                attack_mode: mir_core::enums::AttackMode::Peace,
                pk_points: 0,
                brown_time: 0,
                pk_point_time: 0,
                last_hitter: None,
                last_hit_time: 0,
                name_colour: mir_core::Color(-1),
                visible_monsters: std::collections::BTreeSet::new(),
                visible_npcs: std::collections::BTreeSet::new(),
                movement_time: 0,
                inventory: crate::items::new_inventory(),
                equipment: crate::items::new_equipment(),
                storage: crate::storage::new_storage(),
                magics: Vec::new(),
                magic: MagicState::default(),
                gold: 0,
                credit: 0,
                visible_items: std::collections::BTreeSet::new(),
                quests: Vec::new(),
                completed_quests: std::collections::BTreeSet::new(),
                mail: crate::mail::MailState::default(),
                buffs: Vec::new(),
                pot_health_amount: 0,
                pot_mana_amount: 0,
                pot_time: 0,
                unlock_curse: false,
                npc: crate::npc::NpcState::default(),
                group_id: None,
                group_invitation: None,
                group_invite_time: 0,
                allow_group: true,
                guild: None,
                trade: crate::trade::TradeState::default(),
                trade_invitation: None,
                next_trade_time: 0,
                allow_trade: true,
                bind_map_index: 0,
                bind_point: Point::default(),
                sender,
            },
            receiver,
        )
    }

    /// 新世界 + 1 号测试玩家，返回世界与收包端。
    fn setup() -> (World, mpsc::UnboundedReceiver<Vec<u8>>) {
        let mut world = test_world();
        let (player, receiver) = test_player(1, "0", Point::new(10, 10), 20, false);
        world.enter(player);
        (world, receiver)
    }

    /// 造一件真实模板物品（走生产 `create_fresh_item`），放进主手/装备槽。
    fn give_equipment(
        world: &mut World,
        player_id: u32,
        slot: usize,
        index: i32,
        count: u16,
        unique_id: u64,
    ) -> UserItem {
        let info = world.item_info(index).expect("物品模板").clone();
        let mut item = world.create_fresh_item(&info);
        item.unique_id = unique_id;
        item.count = count;
        world.player_mut(player_id).expect("玩家在世界上").equipment[slot] = Some(item.clone());
        item
    }

    /// 解开收包端里累积的服务端包。
    fn take_packets(rx: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<ServerPacket> {
        let mut packets = Vec::new();
        while let Ok(frame) = rx.try_recv() {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
        }
        packets
    }

    /// 取出所有 `S.DeleteItem` 的 `(UniqueID, Count)`。
    fn delete_items(packets: &[ServerPacket]) -> Vec<(u64, u16)> {
        packets
            .iter()
            .filter_map(|packet| match packet {
                ServerPacket::DeleteItem(packet) => Some((packet.unique_id, packet.count)),
                _ => None,
            })
            .collect()
    }

    fn count_at(world: &World, player_id: u32, slot: usize) -> Option<u16> {
        world.player(player_id).expect("玩家").equipment[slot]
            .as_ref()
            .map(|item| item.count)
    }

    /// 双材料共同预检：Crystal `PoisonCloud`（`HumanObject.cs:4543-4552`）先
    /// `GetAmulet(5)` + `GetPoison(5, 1)`，两项都非空才轮到 `ConsumeItem`。
    ///
    /// 两个 `find` 都要 `&World`，返回槽号后由调用方在同一把锁内 `consume`；
    /// 任一为 `None` 就一项都不扣，这就是 B1 的原子性形状（真实施法在 B2）。
    fn precheck_poison_cloud(
        world: &World,
        player_id: u32,
        amulet_count: u32,
        poison_count: u32,
    ) -> Option<(usize, usize)> {
        let amulet_slot = world.find_spell_material(player_id, amulet_count, 0)?;
        let poison_slot = world.find_spell_material(player_id, poison_count, 1)?;
        Some((amulet_slot, poison_slot))
    }

    #[test]
    fn round52_consumables_equipment_order_atomic_and_exhaustion() {
        // --- 1) 槽序：槽 6（BraceletR）与槽 9（Amulet）都能放护身符，递增扫描先命中 6。
        let (mut world, mut rx) = setup();
        let _ = take_packets(&mut rx); // 进场包
        give_equipment(&mut world, 1, 9, AMULET, 5, 900);
        give_equipment(&mut world, 1, 6, AMULET, 5, 600);
        assert_eq!(
            world.find_spell_material(1, 5, 0),
            Some(6),
            "按 `Info.Equipment` 递增扫描：槽 6 先于槽 9"
        );
        assert_eq!(
            world.find_spell_material(1, 6, 0),
            None,
            "单堆 5 张不够 6 张：不跨槽合并"
        );

        // --- 2) 部分扣数：槽保留、背包栏不动、只发一帧精确 Count 的 DeleteItem。
        assert!(world.consume_spell_material(1, 6, 2));
        assert_eq!(count_at(&world, 1, 6), Some(3));
        assert_eq!(count_at(&world, 1, 9), Some(5), "别的槽不动");
        assert_eq!(delete_items(&take_packets(&mut rx)), vec![(600, 2)]);
        assert_eq!(
            world.find_spell_material(1, 3, 0),
            Some(6),
            "部分扣数后仍在槽 6"
        );

        // --- 3) 耗尽摘槽：槽 6 清空，下一次 find 落到槽 9。
        assert!(world.consume_spell_material(1, 6, 3));
        assert!(world.player(1).expect("玩家").equipment[6].is_none());
        assert_eq!(delete_items(&take_packets(&mut rx)), vec![(600, 3)]);
        assert_eq!(world.find_spell_material(1, 5, 0), Some(9));

        // --- 4) 失败不发包：空槽 / 数量不足 / 0 扣数 / 错误目标（世界外的玩家）。
        assert!(!world.consume_spell_material(1, 0, 1), "空槽扣不动");
        assert!(!world.consume_spell_material(1, 9, 6), "数量不足扣不动");
        assert!(!world.consume_spell_material(1, 9, 0), "0 扣数直接失败");
        assert!(
            !world.consume_spell_material(42, 9, 1),
            "世界外的玩家扣不动"
        );
        let untouched = world.player(1).expect("玩家").equipment[9]
            .clone()
            .expect("护身符");
        assert_eq!(
            (untouched.unique_id, untouched.count),
            (900, 5),
            "错误目标不动 UUID/数量"
        );
        assert_eq!(count_at(&world, 1, 9), Some(5), "失败后物品未变");
        assert!(take_packets(&mut rx).is_empty(), "失败不发 DeleteItem");

        // --- 5) 0 耐久仍可耗（`GetAmulet` 没有耐久判断）。
        world.player_mut(1).expect("玩家").equipment[9]
            .as_mut()
            .expect("护身符")
            .current_dura = 0;
        assert_eq!(
            world.find_spell_material(1, 5, 0),
            Some(9),
            "0 耐久的护身符照样能找到"
        );
        assert!(world.consume_spell_material(1, 9, 5));
        assert!(world.player(1).expect("玩家").equipment[9].is_none());
        assert_eq!(delete_items(&take_packets(&mut rx)), vec![(900, 5)]);

        // --- 6) 背包同模板不误扣：装备栏空了就去背包里找是 Crystal 没有的行为。
        let info = world.item_info(AMULET).expect("模板").clone();
        let mut backpack = world.create_fresh_item(&info);
        backpack.unique_id = 777;
        backpack.count = 10;
        world.player_mut(1).expect("玩家").inventory[0] = Some(backpack);
        assert_eq!(
            world.find_spell_material(1, 5, 0),
            None,
            "装备栏没有护身符：不补背包"
        );
        assert!(!world.consume_spell_material(1, 0, 1), "背包格不是装备槽");
        assert_eq!(
            world.player(1).expect("玩家").inventory[0]
                .as_ref()
                .expect("背包物品")
                .count,
            10,
            "背包物品没被动过"
        );

        // --- 7) 形状精确匹配：绿毒 `Shape = 1`、红毒 `Shape = 2` 不互相顶替。
        let (mut world, _rx) = setup();
        give_equipment(&mut world, 1, 7, POISON_RED, 5, 700);
        give_equipment(&mut world, 1, 8, POISON_GREEN, 5, 800);
        assert_eq!(world.find_spell_material(1, 5, 1), Some(8), "绿毒 shape=1");
        assert_eq!(world.find_spell_material(1, 5, 2), Some(7), "红毒 shape=2");
        assert_eq!(
            world.find_spell_material(1, 5, 3),
            None,
            "没有 shape=3 的耗材"
        );

        // --- 8) 双材料原子：毒不足 → 整体预检失败 → 一张符都不扣。
        let (mut world, mut rx) = setup();
        let _ = take_packets(&mut rx);
        give_equipment(&mut world, 1, 9, AMULET, 5, 900);
        give_equipment(&mut world, 1, 8, POISON_GREEN, 3, 800);
        assert_eq!(
            precheck_poison_cloud(&world, 1, 5, 5),
            None,
            "毒只有 3 张，整体预检失败"
        );
        // 调用方模式：预检 None 时一项都不 consume，等价于 M02 的「任一不足都不扣」。
        assert_eq!(count_at(&world, 1, 9), Some(5));
        assert_eq!(count_at(&world, 1, 8), Some(3));
        assert!(take_packets(&mut rx).is_empty(), "预检失败不扣、不发包");

        // 补足毒后预检通过，两项各扣 5：恰好两帧 DeleteItem，各带自己的 UUID/Count。
        give_equipment(&mut world, 1, 8, POISON_GREEN, 5, 801);
        let Some((amulet_slot, poison_slot)) = precheck_poison_cloud(&world, 1, 5, 5) else {
            panic!("符 5 张 + 绿毒 5 张，双项预检应当通过");
        };
        assert_eq!((amulet_slot, poison_slot), (9, 8));
        assert!(world.consume_spell_material(1, amulet_slot, 5));
        assert!(world.consume_spell_material(1, poison_slot, 5));
        assert!(world.player(1).expect("玩家").equipment[9].is_none());
        assert!(world.player(1).expect("玩家").equipment[8].is_none());
        assert_eq!(
            delete_items(&take_packets(&mut rx)),
            vec![(900, 5), (801, 5)],
            "每件只发一帧精确 Count 的 DeleteItem（先符后毒）"
        );
    }

    #[test]
    fn round52_consumables_find_ignores_players_outside_world() {
        let world = test_world();
        assert_eq!(world.find_spell_material(42, 1, 0), None);
    }
}
