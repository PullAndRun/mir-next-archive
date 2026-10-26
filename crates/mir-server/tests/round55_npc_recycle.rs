//! R55-C：NPC 运行期回收原语 [`World::despawn_npc`] 的端到端验收。
//!
//! 覆盖子单要求的 6 条断言：
//! 1. 刷出后观察者恰好收到 1 次 `S.ObjectNPC`；
//! 2. 回收后 `S.ObjectRemove` 恰好 1 次且对象 ID 精确；
//! 3. 容器（[`World::npc`] / [`World::npcs_on`]）与 AOI 网格都查不到它；
//! 4. 未知对象 ID → `None` 且零包；
//! 5. 二次回收 → `None` 且零新增包；
//! 6. 非 NPC 对象 ID（怪物/玩家）不误删、零包。
//!
//! 另外验证关系清理：正在和该 NPC 对话的玩家退出对话、「已看见」集合被摘掉。
//!
//! 生产路径全部走 [`World::spawn_npcs`]（刷出 + 广播）与 [`World::enter`]（进图下发），
//! 不直接改容器造绿。

mod common;

use std::sync::Arc;

use mir_core::Point;
use mir_db::NpcInfo;
use mir_protocol::ServerPacket;
use mir_server::MonsterKind;
use mir_server::gamedata::GameData;
use mir_server::world::VIEW_RANGE;

use common::{Harness, MAP, MAP_INDEX, base_db, conquest_info};

/// 先入场的观察者：只看见刷出广播，不重新进图。
const OBSERVER: u32 = 100;
/// NPC 刷出后才入场的玩家：`visible_npcs` 里会记下这个 NPC。
const LATE_PLAYER: u32 = 200;
/// 世界事件用的 NPC 主键。
const NPC_INDEX: i32 = 10;
/// NPC 落点（合成图 60×60 内，观察者 `(10,10)` 的视野半径内）。
const NPC_POSITION: Point = Point::new(14, 10);

/// 从服务端包里挑出 `ObjectNPC` 的对象 ID。
fn npc_frames(packets: &[ServerPacket]) -> Vec<u32> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectNPC(npc) => Some(npc.object_id),
            _ => None,
        })
        .collect()
}

/// 从服务端包里挑出 `ObjectRemove` 的对象 ID。
fn removed_ids(packets: &[ServerPacket]) -> Vec<u32> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectRemove(remove) => Some(remove.object_id),
            _ => None,
        })
        .collect()
}

/// 沿用 `common::base_db` 的地图「3」（主键 172），额外加一个 NPC。
fn data_with_npc() -> Arc<GameData> {
    let mut db = base_db(conquest_info());
    db.npcs.push(NpcInfo {
        index: NPC_INDEX,
        map_index: MAP_INDEX,
        file_name: "round55-recycle.txt".to_owned(),
        name: "Recycle".to_owned(),
        location: NPC_POSITION,
        ..NpcInfo::default()
    });
    Arc::new(GameData::from_db(db, None))
}

#[test]
fn despawn_npc_recycles_once_and_clears_relations() {
    let mut harness = Harness::new(7);
    // 观察者先进图：此时数据库里没有 NPC，地图初始化不会刷出任何 NPC。
    harness.add_player(OBSERVER, "observer", 1, 0);
    harness.clear(OBSERVER);
    assert!(
        harness.world.npcs_on(MAP).is_empty(),
        "world should start without NPCs"
    );

    // 换上带一个 NPC 的数据库，再走生产刷出路径（`spawn_npcs` 会广播 `S.ObjectNPC`）。
    harness.world.set_game_data(data_with_npc());
    let spawned = harness.world.spawn_npcs(MAP);
    assert_eq!(spawned.len(), 1, "spawn_npcs should spawn exactly one NPC");
    let npc_id = spawned[0];
    let npc = harness
        .world
        .npc(npc_id)
        .expect("spawned NPC should be in the world");
    assert_eq!(npc.map, MAP);
    assert_eq!(npc.position, NPC_POSITION);
    // 先证明 AOI 查询不是恒空：刷出后网格里恰好就是它。
    assert_eq!(
        harness.world.npc_ids_near(MAP, NPC_POSITION, VIEW_RANGE),
        vec![npc_id],
        "the spawned NPC should be present in the AOI grid"
    );

    // ① 观察者恰好收到 1 次 `ObjectNPC`，对象 ID 精确。
    let packets = harness.packets(OBSERVER);
    assert_eq!(
        npc_frames(&packets),
        vec![npc_id],
        "observer should receive exactly one ObjectNPC for the spawned NPC"
    );

    // 后入场的玩家从进图视野里拿到这个 NPC，并把它记进 `visible_npcs`。
    harness.add_player(LATE_PLAYER, "late", 1, 0);
    assert!(
        harness
            .world
            .player(LATE_PLAYER)
            .expect("late player")
            .visible_npcs
            .contains(&npc_id),
        "entering after the spawn should record the NPC in visible_npcs"
    );
    let late_packets = harness.packets(LATE_PLAYER);
    assert_eq!(
        npc_frames(&late_packets),
        vec![npc_id],
        "late player should receive the NPC exactly once on enter"
    );
    harness.clear(OBSERVER); // 后入场玩家的外观广播

    // 模拟「正在和这个 NPC 对话」：本作没有可用的 NPC 脚本，直接写运行期关系字段。
    harness
        .world
        .player_mut(LATE_PLAYER)
        .expect("late player")
        .npc
        .npc_object_id = npc_id;

    // ② 回收：返回被摘掉的 NPC；两个观察者各恰好收到 1 次 `ObjectRemove` 且 ID 精确。
    let removed = harness
        .world
        .despawn_npc(npc_id)
        .expect("despawn should find the NPC");
    assert_eq!(removed.object_id, npc_id);
    assert_eq!(removed.map, MAP);
    assert_eq!(removed.position, NPC_POSITION);
    for player_id in [OBSERVER, LATE_PLAYER] {
        let packets = harness.packets(player_id);
        assert_eq!(
            removed_ids(&packets),
            vec![npc_id],
            "player {player_id} should receive exactly one ObjectRemove for the despawned NPC"
        );
        assert!(
            npc_frames(&packets).is_empty(),
            "despawn must not broadcast ObjectNPC to player {player_id}"
        );
    }

    // ③ 容器与 AOI 网格都查不到它；关系被清理。
    assert!(
        harness.world.npc(npc_id).is_none(),
        "despawned NPC should leave the container"
    );
    assert!(
        harness.world.npcs_on(MAP).is_empty(),
        "no NPC should remain on the map"
    );
    assert!(
        harness
            .world
            .npc_ids_near(MAP, NPC_POSITION, VIEW_RANGE)
            .is_empty(),
        "despawned NPC should leave the AOI grid"
    );
    assert!(
        !harness
            .world
            .player(LATE_PLAYER)
            .expect("late player")
            .visible_npcs
            .contains(&npc_id),
        "despawn should clear the visible NPC set"
    );
    assert_eq!(
        harness
            .world
            .player(LATE_PLAYER)
            .expect("late player")
            .npc
            .npc_object_id,
        0,
        "despawn should close the dialog relation"
    );

    // ④ 未知对象 ID：`None` 且零包。
    harness.clear(OBSERVER);
    assert!(
        harness.world.despawn_npc(9999).is_none(),
        "unknown object id should return None"
    );
    assert!(
        harness.packets(OBSERVER).is_empty(),
        "unknown object id should not emit any packet"
    );

    // ⑤ 二次回收：`None` 且零新增包。
    assert!(
        harness.world.despawn_npc(npc_id).is_none(),
        "second despawn should return None"
    );
    assert!(
        harness.packets(OBSERVER).is_empty(),
        "second despawn should not emit any packet"
    );

    // ⑥ 非 NPC 对象 ID（怪物/玩家）不误删、零包。
    let monster_id = harness
        .world
        .spawn_monster(MAP, MonsterKind::skeleton(), Point::new(12, 10));
    harness.clear(OBSERVER); // 怪物刷出广播
    assert!(
        harness.world.despawn_npc(monster_id).is_none(),
        "monster id should not be despawned as NPC"
    );
    assert!(
        harness.world.monster(monster_id).is_some(),
        "monster should survive despawn_npc"
    );
    assert!(
        harness.world.despawn_npc(OBSERVER).is_none(),
        "player id should not be despawned as NPC"
    );
    assert!(
        harness.world.player(OBSERVER).is_some(),
        "player should survive despawn_npc"
    );
    assert!(
        harness.packets(OBSERVER).is_empty(),
        "non-NPC object ids should not emit any packet"
    );
}
