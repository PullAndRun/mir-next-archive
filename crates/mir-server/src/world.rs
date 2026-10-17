//! 极简世界：在线玩家、视野广播与移动校验。
//!
//! 这一版只做「单地图、切比雪夫视野、无阻挡地图」的最小闭环，
//! 让「登录 → 选角 → 进世界 → 走路 → 聊天 → 下线」这条链路真正跑通。
//! 地形阻挡、怪物、战斗等在拿到 [`mir_world::Map`] 之后接入。

use std::collections::HashMap;

use mir_core::enums::{ChatType, MirClass, MirDirection, MirGender};
use mir_core::{MirRng, Point};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{
    ObjectChat, ObjectPlayer, ObjectRemove, ObjectRun, ObjectTurn, ObjectWalk,
};
use tokio::sync::mpsc::UnboundedSender;

/// 视野半径（Crystal `Globals.DataRange` 的默认值）。
pub const VIEW_RANGE: i32 = 10;
/// 聊天广播半径。
pub const CHAT_RANGE: i32 = 20;

/// 一个在线玩家。
#[derive(Debug, Clone)]
pub struct Player {
    /// 对象 ID（世界内唯一）。
    pub object_id: u32,
    /// 连接 ID。
    pub session_id: u64,
    /// 角色 ID。
    pub character_id: i64,
    /// 角色名。
    pub name: String,
    /// 地图名。
    pub map: String,
    /// 坐标。
    pub position: Point,
    /// 朝向。
    pub direction: MirDirection,
    /// 等级。
    pub level: u16,
    /// 职业。
    pub class: MirClass,
    /// 性别。
    pub gender: MirGender,
    /// 发型。
    pub hair: u8,
    /// 生命。
    pub hp: i32,
    /// 魔法。
    pub mp: i32,
    /// 发包通道。
    pub sender: UnboundedSender<Vec<u8>>,
}

/// 移动失败原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MoveError {
    /// 对象不在世界里。
    #[error("对象 {0} 不在世界里")]
    NotInWorld(u32),
    /// 目标格被占用。
    #[error("目标格 ({x}, {y}) 已被占用")]
    Occupied {
        /// 目标 X。
        x: i32,
        /// 目标 Y。
        y: i32,
    },
    /// 越界。
    #[error("目标格 ({x}, {y}) 越界")]
    OutOfBounds {
        /// 目标 X。
        x: i32,
        /// 目标 Y。
        y: i32,
    },
    /// 地形不可走（墙）。
    #[error("目标格 ({x}, {y}) 是墙")]
    Blocked {
        /// 目标 X。
        x: i32,
        /// 目标 Y。
        y: i32,
    },
}

/// 世界状态。
#[derive(Debug)]
pub struct World {
    players: HashMap<u32, Player>,
    next_object_id: u32,
    rng: MirRng,
    /// 允许的坐标范围（0..=bound）。没有加载真实地图时用它兜底。
    pub bound: i32,
    /// 已加载的地图；有它时行走按真实地形的可走性判定。
    map: Option<std::sync::Arc<mir_world::Map>>,
}

impl Default for World {
    fn default() -> Self {
        Self::new(999)
    }
}

impl World {
    /// 用坐标上界构造（无地图，仅做边界与占格校验）。
    #[must_use]
    pub fn new(bound: i32) -> Self {
        Self {
            players: HashMap::new(),
            next_object_id: 1,
            rng: MirRng::from_entropy(),
            bound,
            map: None,
        }
    }

    /// 用真实地图构造：行走判定会查地图的可走性。
    #[must_use]
    pub fn with_map(map: std::sync::Arc<mir_world::Map>) -> Self {
        let bound = map.width.max(map.height).saturating_sub(1);
        Self {
            players: HashMap::new(),
            next_object_id: 1,
            rng: MirRng::from_entropy(),
            bound,
            map: Some(map),
        }
    }

    /// 当前地图（如果有）。
    #[must_use]
    pub fn map(&self) -> Option<&std::sync::Arc<mir_world::Map>> {
        self.map.as_ref()
    }

    /// 在线人数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.players.len()
    }

    /// 是否空世界。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }

    /// 分配一个对象 ID。
    pub fn allocate_object_id(&mut self) -> u32 {
        let id = self.next_object_id;
        self.next_object_id = self.next_object_id.wrapping_add(1).max(1);
        id
    }

    /// 按对象 ID 取玩家。
    #[must_use]
    pub fn player(&self, object_id: u32) -> Option<&Player> {
        self.players.get(&object_id)
    }

    /// 把玩家放进世界，并把它的外观广播给同图可见玩家。
    pub fn enter(&mut self, player: Player) {
        let appearance = ServerPacket::ObjectPlayer(ObjectPlayer {
            object_id: player.object_id,
            name: player.name.clone(),
            class: player.class,
            gender: player.gender,
            level: player.level,
            location: player.position,
            direction: player.direction,
            hair: player.hair,
            light: 0,
            dead: false,
            hidden: false,
            extra: false,
            riding_mount: false,
            fishing: false,
            level_effects: mir_core::enums::LevelEffects::None,
        })
        .encode_frame();

        // 先让新玩家看到周围的人，再把自己的外观发给别人。
        let mut existing = Vec::new();
        for other in self.nearby(player.position, &player.map, VIEW_RANGE, None) {
            existing.push(other.clone());
        }
        for other in &existing {
            let _ = player.sender.send(spawn_packet(other));
        }

        for other in self.players.values() {
            if other.map == player.map
                && other.position.in_range(player.position, VIEW_RANGE)
                && other.object_id != player.object_id
            {
                let _ = other.sender.send(appearance.clone());
            }
        }

        self.players.insert(player.object_id, player);
    }

    /// 把玩家移出世界，并通知周围玩家移除。
    pub fn leave(&mut self, object_id: u32) -> Option<Player> {
        let player = self.players.remove(&object_id)?;
        let removal = ServerPacket::ObjectRemove(ObjectRemove {
            object_id,
            ..ObjectRemove::default()
        })
        .encode_frame();
        for other in self.players.values() {
            if other.map == player.map && other.position.in_range(player.position, VIEW_RANGE) {
                let _ = other.sender.send(removal.clone());
            }
        }
        Some(player)
    }

    /// 转身。
    pub fn turn(&mut self, object_id: u32, direction: MirDirection) -> Result<(), MoveError> {
        let (map, position) = {
            let player = self
                .players
                .get_mut(&object_id)
                .ok_or(MoveError::NotInWorld(object_id))?;
            player.direction = direction;
            (player.map.clone(), player.position)
        };
        let packet = ServerPacket::ObjectTurn(ObjectTurn {
            object_id,
            location: position,
            direction,
        })
        .encode_frame();
        self.broadcast(&map, position, Some(object_id), &packet, true, object_id);
        Ok(())
    }

    /// 走或跑一步。
    pub fn walk(
        &mut self,
        object_id: u32,
        direction: MirDirection,
        running: bool,
    ) -> Result<Point, MoveError> {
        let (map, from, to) = {
            let player = self
                .players
                .get(&object_id)
                .ok_or(MoveError::NotInWorld(object_id))?;
            let from = player.position;
            (player.map.clone(), from, from.adjacent(direction))
        };
        if to.x < 0 || to.y < 0 || to.x > self.bound || to.y > self.bound {
            return Err(MoveError::OutOfBounds { x: to.x, y: to.y });
        }
        if let Some(map) = &self.map {
            if !map.walkable(to.x, to.y) {
                return Err(MoveError::Blocked { x: to.x, y: to.y });
            }
        }
        if self
            .players
            .values()
            .any(|other| other.map == map && other.position == to && other.object_id != object_id)
        {
            return Err(MoveError::Occupied { x: to.x, y: to.y });
        }
        if let Some(player) = self.players.get_mut(&object_id) {
            player.position = to;
            player.direction = direction;
        }

        let packet = if running {
            ServerPacket::ObjectRun(ObjectRun {
                object_id,
                location: from,
                direction,
            })
        } else {
            ServerPacket::ObjectWalk(ObjectWalk {
                object_id,
                location: from,
                direction,
            })
        }
        .encode_frame();
        self.broadcast(&map, to, None, &packet, true, object_id);
        Ok(to)
    }

    /// 聊天广播，返回实际接收人数。
    pub fn chat(&mut self, object_id: u32, message: &str) -> Result<usize, MoveError> {
        let (map, position) = {
            let player = self
                .players
                .get(&object_id)
                .ok_or(MoveError::NotInWorld(object_id))?;
            (player.map.clone(), player.position)
        };
        let packet = ServerPacket::ObjectChat(ObjectChat {
            object_id,
            text: message.to_owned(),
            type_: ChatType::Normal,
        })
        .encode_frame();
        Ok(self.broadcast(&map, position, None, &packet, true, object_id))
    }

    fn nearby(&self, position: Point, map: &str, range: i32, exclude: Option<u32>) -> Vec<&Player> {
        self.players
            .values()
            .filter(|other| {
                other.map == map
                    && other.position.in_range(position, range)
                    && exclude != Some(other.object_id)
            })
            .collect()
    }

    /// 同图、视野内的玩家接收 `packet`；`include_self` 决定对象自己是否也收到。
    fn broadcast(
        &self,
        map: &str,
        position: Point,
        exclude: Option<u32>,
        packet: Vec<u8>,
        include_self: bool,
        actor: u32,
    ) -> usize {
        let mut sent = 0;
        for other in self.players.values() {
            if other.map != map || !other.position.in_range(position, VIEW_RANGE) {
                continue;
            }
            if exclude == Some(other.object_id) {
                continue;
            }
            if !include_self && other.object_id == actor {
                continue;
            }
            if other.sender.send(packet.to_vec()).is_ok() {
                sent += 1;
            }
        }
        sent
    }

    /// 给新入场的玩家随机一个出生点微偏移，避免叠在一起。
    pub fn jitter(&mut self, point: Point, radius: i32) -> Point {
        let dx = self.rng.next_range(-radius, radius);
        let dy = self.rng.next_range(-radius, radius);
        Point::new((point.x + dx).max(0), (point.y + dy).max(0))
    }

    /// 随机数发生器（战斗/掉落会用到）。
    pub fn rng(&mut self) -> &mut MirRng {
        &mut self.rng
    }
}

fn spawn_packet(player: &Player) -> Vec<u8> {
    ServerPacket::ObjectPlayer(ObjectPlayer {
        object_id: player.object_id,
        name: player.name.clone(),
        class: player.class,
        gender: player.gender,
        level: player.level,
        location: player.position,
        direction: player.direction,
        hair: player.hair,
        light: 0,
        dead: false,
        hidden: false,
        extra: false,
        riding_mount: false,
        fishing: false,
        level_effects: mir_core::enums::LevelEffects::None,
    })
    .encode_frame()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn player(
        object_id: u32,
        name: &str,
        position: Point,
        map: &str,
    ) -> (Player, mpsc::UnboundedReceiver<Vec<u8>>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            Player {
                object_id,
                session_id: u64::from(object_id),
                character_id: i64::from(object_id),
                name: name.to_owned(),
                map: map.to_owned(),
                position,
                direction: MirDirection::Down,
                level: 1,
                class: MirClass::Warrior,
                gender: MirGender::Male,
                hair: 1,
                hp: 15,
                mp: 0,
                sender,
            },
            receiver,
        )
    }

    #[test]
    fn entering_world_notifies_both_directions() {
        let mut world = World::new(999);
        let (first, mut first_rx) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(first);
        let (second, mut second_rx) = player(2, "乙", Point::new(11, 10), "0");
        world.enter(second);

        assert!(first_rx.try_recv().is_ok(), "先入场的人应该看到后来者");
        assert!(second_rx.try_recv().is_ok(), "后入场的人应该看到先入场的人");
    }

    #[test]
    fn walking_moves_and_broadcasts() {
        let mut world = World::new(999);
        let (walker, mut walker_rx) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(walker);
        let (observer, mut observer_rx) = player(2, "乙", Point::new(12, 10), "0");
        world.enter(observer);
        // 清掉入场通知。
        while walker_rx.try_recv().is_ok() {}
        while observer_rx.try_recv().is_ok() {}

        let position = world.walk(1, MirDirection::Right, false).expect("走路");
        assert_eq!(position, Point::new(11, 10));
        assert_eq!(world.player(1).unwrap().position, Point::new(11, 10));
        assert!(observer_rx.try_recv().is_ok(), "旁观者应收到 ObjectWalk");
        assert!(
            walker_rx.try_recv().is_ok(),
            "移动者本人也应收到 ObjectWalk"
        );
    }

    #[test]
    fn cannot_walk_onto_occupied_cell() {
        let mut world = World::new(999);
        let (first, _rx1) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(first);
        let (second, _rx2) = player(2, "乙", Point::new(11, 10), "0");
        world.enter(second);

        assert_eq!(
            world.walk(1, MirDirection::Right, false),
            Err(MoveError::Occupied { x: 11, y: 10 })
        );
    }

    #[test]
    fn cannot_walk_out_of_bounds() {
        let mut world = World::new(5);
        let (walker, _rx) = player(1, "甲", Point::new(5, 5), "0");
        world.enter(walker);
        assert_eq!(
            world.walk(1, MirDirection::Right, false),
            Err(MoveError::OutOfBounds { x: 6, y: 5 })
        );
    }

    #[test]
    fn chat_reaches_people_in_range_only() {
        let mut world = World::new(999);
        let (speaker, _rx1) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(speaker);
        let (near, mut near_rx) = player(2, "乙", Point::new(12, 10), "0");
        world.enter(near);
        let (far, mut far_rx) = player(3, "丙", Point::new(90, 90), "0");
        world.enter(far);
        while near_rx.try_recv().is_ok() {}
        while far_rx.try_recv().is_ok() {}

        let receivers = world.chat(1, "你好").expect("聊天");
        assert_eq!(receivers, 2, "自己与近处的人各收到一条");
        assert!(near_rx.try_recv().is_ok());
        assert!(far_rx.try_recv().is_err(), "远处的人不该收到");
    }

    #[test]
    fn leaving_notifies_observers() {
        let mut world = World::new(999);
        let (a, _rx1) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(a);
        let (b, mut b_rx) = player(2, "乙", Point::new(11, 10), "0");
        world.enter(b);
        while b_rx.try_recv().is_ok() {}
        assert!(world.leave(1).is_some());
        assert!(b_rx.try_recv().is_ok(), "乙应收到 ObjectRemove");
        assert_eq!(world.len(), 1);
    }
}
