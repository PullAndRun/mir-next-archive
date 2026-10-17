//! 世界：在线玩家、视野广播、移动判定与动作计时。
//!
//! 规则按 Crystal 的服务器权威逻辑逐条移植（见 `docs/world/movement.md`）：
//!
//! * 服务器时钟是「启动以来的单调毫秒数」（Crystal `Envir.Time`）。
//! * 每个对象有 `action_time`：下一次允许开始动作的最早时刻；
//!   走/跑一次动作 **600ms**、转身 **350ms**，与职业、护甲重量无关。
//! * 走一步 `step_counter + 1`；**转身或停顿 700ms 以上清零**，
//!   而「跑」要求 `step_counter > 0`——所以站桩或转身后必须先走一步才能跑。
//! * 移动成功时：本人收到 `UserLocation`，周围玩家收到 `ObjectWalk`/`ObjectRun`；
//!   移动失败时：只给本人回一条 `UserLocation` 把角色拉回服务器坐标。

use std::collections::HashMap;
use std::time::Instant;

use mir_core::enums::{ChatType, MirClass, MirDirection, MirGender};
use mir_core::{MirRng, Point};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{
    ObjectChat, ObjectPlayer, ObjectRemove, ObjectRun, ObjectTurn, ObjectWalk, UserLocation,
};
use tokio::sync::mpsc::UnboundedSender;

/// 视野半径（Crystal `Globals.DataRange` 的默认值）。
pub const VIEW_RANGE: i32 = 10;
/// 聊天广播半径。
pub const CHAT_RANGE: i32 = 20;
/// 走/跑一次动作的耗时（Crystal `HumanObject.MoveDelay`）。
pub const MOVE_DELAY_MS: i64 = 600;
/// 转身耗时（Crystal `Turn` 里的 350ms）。
pub const TURN_DELAY_MS: i64 = 350;
/// 刚移动过的宽限：期间不阻挡别人（Crystal `CellTime = now + 500`）。
pub const CELL_GRACE_MS: i64 = 500;
/// 停顿多久算「站住了」，清零 `step_counter`（Crystal `CellTime + 700 < Time`）。
pub const IDLE_RESET_MS: i64 = 700;
/// 跑步计数上限：超过就扣 1 点血并回退 8（Crystal `_runCounter`）。
pub const RUN_COUNTER_LIMIT: i32 = 10;
/// 跑步计数每 1500ms 自然减 1。
pub const RUN_COUNTER_DECAY_MS: i64 = 1_500;

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
    /// 下一次可以开始动作的最早时刻（毫秒，服务器单调时钟）。
    pub action_time: i64,
    /// 刚移动过的宽限截止时刻。
    pub cell_time: i64,
    /// 连续行走步数：转身或停顿后清零，跑步要求它大于 0。
    pub step_counter: i32,
    /// 跑步计数（体力）：超过上限会掉血。
    pub run_counter: i32,
    /// 跑步计数下一次自然衰减的时刻。
    pub run_time: i64,
    /// 发包通道。
    pub sender: UnboundedSender<Vec<u8>>,
}

impl Player {
    /// 是否可以开始新动作（`CanMove`）。
    #[must_use]
    pub fn can_move(&self, now: i64) -> bool {
        now >= self.action_time
    }

    /// 是否允许跑（`CanRun`）：动作已结束且刚走过至少一步。
    #[must_use]
    pub fn can_run(&self, now: i64) -> bool {
        self.can_move(now) && self.step_counter > 0
    }
}

/// 移动失败原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MoveError {
    /// 对象不在世界里。
    #[error("对象 {0} 不在世界里")]
    NotInWorld(u32),
    /// 上一个动作还没结束。
    #[error("动作未结束，最早 {until}ms 之后才能动")]
    Busy {
        /// 可以开始下一个动作的时刻。
        until: i64,
    },
    /// 还没走过路，不能跑（Crystal `_stepCounter > 0`）。
    #[error("需要先走一步才能跑")]
    CannotRunYet,
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
    /// 服务器时钟起点（Crystal `Envir.Time` 的 0 点）。
    started: Instant,
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
            started: Instant::now(),
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
            started: Instant::now(),
        }
    }

    /// 当前地图（如果有）。
    #[must_use]
    pub fn map(&self) -> Option<&std::sync::Arc<mir_world::Map>> {
        self.map.as_ref()
    }

    /// 服务器时钟（毫秒，单调递增）。
    #[must_use]
    pub fn now(&self) -> i64 {
        self.started.elapsed().as_millis() as i64
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
        let appearance = appearance_packet(&player);
        let now = self.now();

        // 先让新玩家看到周围的人，再把自己的外观发给别人。
        let mut existing = Vec::new();
        for other in self.nearby(player.position, &player.map, VIEW_RANGE, None) {
            existing.push(other.clone());
        }
        for other in &existing {
            let _ = player.sender.send(appearance_packet(other));
        }

        for other in self.players.values() {
            if other.map == player.map
                && other.position.in_range(player.position, VIEW_RANGE)
                && other.object_id != player.object_id
            {
                let _ = other.sender.send(appearance.clone());
            }
        }

        let mut player = player;
        player.action_time = now;
        // 刚进世界还没移动过，`cell_time` 保持 0 → 会正常阻挡别人。
        player.cell_time = 0;
        self.players.insert(player.object_id, player);
    }

    /// 把玩家移出世界，并通知周围玩家移除。
    pub fn leave(&mut self, object_id: u32) -> Option<Player> {
        let player = self.players.remove(&object_id)?;
        let removal = ServerPacket::ObjectRemove(ObjectRemove { object_id }).encode_frame();
        for other in self.players.values() {
            if other.map == player.map && other.position.in_range(player.position, VIEW_RANGE) {
                let _ = other.sender.send(removal.clone());
            }
        }
        Some(player)
    }

    /// 转身。
    ///
    /// 对应 Crystal `PlayerObject.Turn`：`_stepCounter` 先清零（转身后不能立刻跑），
    /// 动作没结束时不改方向，但**无论如何**都回一条 `UserLocation` 给本人。
    pub fn turn(&mut self, object_id: u32, direction: MirDirection) -> Result<(), MoveError> {
        let now = self.now();
        self.refresh_idle(object_id, now);
        let (map, position, can_move) = {
            let player = self
                .players
                .get_mut(&object_id)
                .ok_or(MoveError::NotInWorld(object_id))?;
            player.step_counter = 0;
            (player.map.clone(), player.position, player.can_move(now))
        };
        if !can_move {
            self.send_user_location(object_id);
            return Err(MoveError::Busy {
                until: self.players[&object_id].action_time,
            });
        }
        if let Some(player) = self.players.get_mut(&object_id) {
            player.direction = direction;
            player.action_time = now + TURN_DELAY_MS;
        }
        let packet = ServerPacket::ObjectTurn(ObjectTurn {
            object_id,
            location: position,
            direction,
        })
        .encode_frame();
        self.broadcast(&map, position, Some(object_id), &packet, object_id);
        self.send_user_location(object_id);
        Ok(())
    }

    /// 走一步。
    ///
    /// 判定顺序与 Crystal `HumanObject.Walk` 一致：动作时间 → 边界/地形 → 占格。
    /// 任何一步失败都只把本人拉回服务器坐标。
    pub fn walk(&mut self, object_id: u32, direction: MirDirection) -> Result<Point, MoveError> {
        let now = self.now();
        self.refresh_idle(object_id, now);
        let Some(player) = self.players.get(&object_id) else {
            return Err(MoveError::NotInWorld(object_id));
        };
        if !player.can_move(now) {
            self.send_user_location(object_id);
            return Err(MoveError::Busy {
                until: player.action_time,
            });
        }
        let map = player.map.clone();
        let from = player.position;
        let to = from.adjacent(direction);
        if let Err(error) = self.check_cell(&map, to, object_id, now) {
            self.send_user_location(object_id);
            return Err(error);
        }

        if let Some(player) = self.players.get_mut(&object_id) {
            player.position = to;
            player.direction = direction;
            player.step_counter += 1;
            player.cell_time = now + CELL_GRACE_MS;
            player.action_time = now + MOVE_DELAY_MS;
        }
        let packet = ServerPacket::ObjectWalk(ObjectWalk {
            object_id,
            location: to,
            direction,
        })
        .encode_frame();
        self.broadcast(&map, to, Some(object_id), &packet, object_id);
        self.send_user_location(object_id);
        Ok(to)
    }

    /// 跑（一次动作跨 2 格）。
    ///
    /// Crystal 语义：`step_counter > 0` 才允许跑；**每一格**都要单独校验，
    /// 任一格不通过则整次跑失败、人物留在原地。骑乘与疾风步尚未实现，
    /// 因此固定 2 格（`steps = 2`）。
    pub fn run(&mut self, object_id: u32, direction: MirDirection) -> Result<Point, MoveError> {
        let now = self.now();
        self.refresh_idle(object_id, now);
        let Some(player) = self.players.get(&object_id) else {
            return Err(MoveError::NotInWorld(object_id));
        };
        if !player.can_move(now) {
            self.send_user_location(object_id);
            return Err(MoveError::Busy {
                until: player.action_time,
            });
        }
        if !player.can_run(now) {
            self.send_user_location(object_id);
            return Err(MoveError::CannotRunYet);
        }
        let map = player.map.clone();
        let from = player.position;
        let steps = 2;
        let mut destination = from;
        for step in 1..=steps {
            let cell = from.moved(direction, step);
            if let Err(error) = self.check_cell(&map, cell, object_id, now) {
                self.send_user_location(object_id);
                return Err(error);
            }
            destination = cell;
        }

        if let Some(player) = self.players.get_mut(&object_id) {
            player.position = destination;
            player.direction = direction;
            player.cell_time = now + CELL_GRACE_MS;
            player.action_time = now + MOVE_DELAY_MS;
            player.run_counter += 1;
            if player.run_counter > RUN_COUNTER_LIMIT {
                player.run_counter -= 8;
                // Crystal：`_runCounter > 10 → -= 8 且 ChangeHP(-1)`。
                player.hp -= 1;
            }
            if player.run_time == 0 {
                player.run_time = now + RUN_COUNTER_DECAY_MS;
            }
        }
        let packet = ServerPacket::ObjectRun(ObjectRun {
            object_id,
            location: destination,
            direction,
        })
        .encode_frame();
        self.broadcast(&map, destination, Some(object_id), &packet, object_id);
        self.send_user_location(object_id);
        Ok(destination)
    }

    /// 世界 tick：跑步计数衰减、停顿后清零 `step_counter`。
    ///
    /// 由服务端的定时任务每 100ms 调一次；后续怪物 AI、刷怪与回血也挂在这里。
    pub fn tick(&mut self) {
        let now = self.now();
        for player in self.players.values_mut() {
            if player.run_counter > 0 && now >= player.run_time {
                player.run_counter -= 1;
                player.run_time = now + RUN_COUNTER_DECAY_MS;
            }
            if player.cell_time + IDLE_RESET_MS < now {
                player.step_counter = 0;
            }
        }
    }

    /// 聊天广播，返回实际接收人数（含本人）。
    ///
    /// Crystal 的普通聊天是 `Enqueue(p)` + `Broadcast(p)`：本人与附近玩家都会收到
    /// `ObjectChat`，所以这里先把同一帧发给本人，再广播给其它人。
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
        let mut receivers = 0;
        if let Some(player) = self.players.get(&object_id) {
            if player.sender.send(packet.clone()).is_ok() {
                receivers += 1;
            }
        }
        Ok(receivers + self.broadcast(&map, position, None, &packet, object_id))
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

    /// 检查目标格是否可进入：边界、地形、占格。
    fn check_cell(&self, map: &str, cell: Point, mover: u32, now: i64) -> Result<(), MoveError> {
        if cell.x < 0 || cell.y < 0 || cell.x > self.bound || cell.y > self.bound {
            return Err(MoveError::OutOfBounds {
                x: cell.x,
                y: cell.y,
            });
        }
        if let Some(map_data) = &self.map {
            if !map_data.walkable(cell.x, cell.y) {
                return Err(MoveError::Blocked {
                    x: cell.x,
                    y: cell.y,
                });
            }
        }
        // Crystal：刚移动过的对象有 500ms 宽限（`CellTime >= Envir.Time` 时不算阻挡）。
        let occupied = self.players.values().any(|other| {
            other.map == map
                && other.position == cell
                && other.object_id != mover
                && other.cell_time < now
        });
        if occupied {
            return Err(MoveError::Occupied {
                x: cell.x,
                y: cell.y,
            });
        }
        Ok(())
    }

    /// 停顿检查：`cell_time + 700 < now` 时清零 `step_counter`。
    fn refresh_idle(&mut self, object_id: u32, now: i64) {
        if let Some(player) = self.players.get_mut(&object_id) {
            if player.cell_time + IDLE_RESET_MS < now {
                player.step_counter = 0;
            }
        }
    }

    /// 给本人发一条坐标回显（成功与失败都发，与 Crystal 一致）。
    fn send_user_location(&self, object_id: u32) {
        if let Some(player) = self.players.get(&object_id) {
            let packet = ServerPacket::UserLocation(UserLocation {
                location: player.position,
                direction: player.direction,
            })
            .encode_frame();
            let _ = player.sender.send(packet);
        }
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

    /// 同图、视野内的**其它**玩家接收 `packet`（Crystal `Broadcast` 不发给本人）。
    fn broadcast(
        &self,
        map: &str,
        position: Point,
        exclude: Option<u32>,
        packet: &[u8],
        actor: u32,
    ) -> usize {
        let mut sent = 0;
        for other in self.players.values() {
            if other.map != map || !other.position.in_range(position, VIEW_RANGE) {
                continue;
            }
            if exclude == Some(other.object_id) || other.object_id == actor {
                continue;
            }
            if other.sender.send(packet.to_vec()).is_ok() {
                sent += 1;
            }
        }
        sent
    }
}

fn appearance_packet(player: &Player) -> Vec<u8> {
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
        ..ObjectPlayer::default()
    })
    .encode_frame()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir_protocol::ServerPacket;
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
                action_time: 0,
                cell_time: 0,
                step_counter: 0,
                run_counter: 0,
                run_time: 0,
                sender,
            },
            receiver,
        )
    }

    /// 把 `Vec<u8>` 解成 `ServerPacket`，方便断言。
    fn decode(frame: &[u8]) -> ServerPacket {
        let index = i16::from_le_bytes([frame[2], frame[3]]);
        ServerPacket::decode(index, &frame[4..]).expect("解码服务端包")
    }

    fn drain(receiver: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<ServerPacket> {
        let mut packets = Vec::new();
        while let Ok(frame) = receiver.try_recv() {
            packets.push(decode(&frame));
        }
        packets
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
    fn walking_moves_and_sends_user_location_and_object_walk() {
        let mut world = World::new(999);
        let (walker, mut walker_rx) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(walker);
        let (observer, mut observer_rx) = player(2, "乙", Point::new(12, 10), "0");
        world.enter(observer);
        drain(&mut walker_rx);
        drain(&mut observer_rx);

        let position = world.walk(1, MirDirection::Right).expect("走路");
        assert_eq!(position, Point::new(11, 10));
        assert_eq!(world.player(1).unwrap().position, Point::new(11, 10));

        // 本人收到 UserLocation（新坐标），旁观者收到 ObjectWalk。
        match decode(&walker_rx.try_recv().expect("本人应收 UserLocation")) {
            ServerPacket::UserLocation(location) => {
                assert_eq!(location.location, Point::new(11, 10));
                assert_eq!(location.direction, MirDirection::Right);
            }
            other => panic!("期望 UserLocation，收到 {other:?}"),
        }
        match decode(&observer_rx.try_recv().expect("旁观者应收 ObjectWalk")) {
            ServerPacket::ObjectWalk(walk) => {
                assert_eq!(walk.object_id, 1);
                assert_eq!(walk.location, Point::new(11, 10));
                assert_eq!(walk.direction, MirDirection::Right);
            }
            other => panic!("期望 ObjectWalk，收到 {other:?}"),
        }
    }

    #[test]
    fn cannot_walk_onto_occupied_cell() {
        let mut world = World::new(999);
        let (first, _rx1) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(first);
        let (second, _rx2) = player(2, "乙", Point::new(11, 10), "0");
        world.enter(second);
        // 服务器时钟是毫秒级：等 2ms，让 `cell_time < now` 成立（刚进世界时 cell_time=0）。
        std::thread::sleep(std::time::Duration::from_millis(2));

        assert_eq!(
            world.walk(1, MirDirection::Right),
            Err(MoveError::Occupied { x: 11, y: 10 })
        );
    }

    #[test]
    fn cannot_walk_out_of_bounds() {
        let mut world = World::new(5);
        let (walker, _rx) = player(1, "甲", Point::new(5, 5), "0");
        world.enter(walker);
        assert_eq!(
            world.walk(1, MirDirection::Right),
            Err(MoveError::OutOfBounds { x: 6, y: 5 })
        );
    }

    #[test]
    fn walk_then_run_is_allowed_but_run_from_standstill_is_not() {
        let mut world = World::new(999);
        let (walker, mut rx) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(walker);
        drain(&mut rx);

        // 站桩状态：不能跑。
        assert_eq!(
            world.run(1, MirDirection::Right),
            Err(MoveError::CannotRunYet)
        );

        // 走一步之后可以跑（时间要等到 ActionTime 之后）。
        world.walk(1, MirDirection::Right).expect("走一步");
        let now = world.now();
        if let Some(player) = world.players.get_mut(&1) {
            player.action_time = now; // 模拟 600ms 已过
        }
        let destination = world.run(1, MirDirection::Right).expect("跑两格");
        assert_eq!(destination, Point::new(13, 10));
    }

    #[test]
    fn turn_resets_step_counter_so_run_needs_a_fresh_walk() {
        let mut world = World::new(999);
        let (walker, mut rx) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(walker);
        drain(&mut rx);
        world.walk(1, MirDirection::Right).expect("走一步");
        assert_eq!(world.player(1).unwrap().step_counter, 1);
        let now = world.now();
        if let Some(player) = world.players.get_mut(&1) {
            player.action_time = now;
        }

        world.turn(1, MirDirection::Up).expect("转身");
        assert_eq!(world.player(1).unwrap().step_counter, 0, "转身清零");
        assert_eq!(world.player(1).unwrap().direction, MirDirection::Up);

        let now = world.now();
        if let Some(player) = world.players.get_mut(&1) {
            player.action_time = now;
        }
        assert_eq!(
            world.run(1, MirDirection::Up),
            Err(MoveError::CannotRunYet),
            "转身后不能立刻跑"
        );
    }

    #[test]
    fn action_time_enforces_move_delay() {
        let mut world = World::new(999);
        let (walker, mut rx) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(walker);
        drain(&mut rx);

        world.walk(1, MirDirection::Right).expect("第一次走");
        // 立刻再走：动作未结束，应该被拒绝并且收到回拉包。
        match world.walk(1, MirDirection::Right) {
            Err(MoveError::Busy { .. }) => {}
            other => panic!("期望 Busy，收到 {other:?}"),
        }
        let packets = drain(&mut rx);
        assert!(
            packets
                .iter()
                .any(|packet| matches!(packet, ServerPacket::UserLocation(_))),
            "被拒绝时也应给本人回 UserLocation"
        );
        assert_eq!(
            world.player(1).unwrap().position,
            Point::new(11, 10),
            "被拒绝不应移动"
        );
    }

    #[test]
    fn run_stamina_penalty_costs_hp_after_ten_runs() {
        let mut world = World::new(999);
        let (walker, _rx) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(walker);
        // 直接置成「刚走过一步」，然后连续向下跑 11 次（每次 2 格，远未到边界）。
        if let Some(player) = world.players.get_mut(&1) {
            player.step_counter = 1;
        }
        for _ in 0..11 {
            let now = world.now();
            if let Some(player) = world.players.get_mut(&1) {
                player.action_time = now;
            }
            world.run(1, MirDirection::Down).expect("跑");
        }
        assert_eq!(world.player(1).unwrap().position, Point::new(10, 32));
        assert!(
            world.player(1).unwrap().hp < 15,
            "跑满 10 次以上应扣血，实际 HP = {}",
            world.player(1).unwrap().hp
        );
        assert_eq!(world.player(1).unwrap().run_counter, 3, "11 - 8 = 3");
    }

    #[test]
    fn tick_decays_run_counter_and_clears_steps_when_idle() {
        let mut world = World::new(999);
        let (mut walker, _rx) = player(1, "甲", Point::new(10, 10), "0");
        walker.step_counter = 3;
        walker.run_counter = 2;
        walker.run_time = 0;
        world.enter(walker);
        // 模拟「上次移动已经是 1 秒前」：`enter` 会把 cell_time 归零，这里直接调。
        let now = world.now();
        if let Some(player) = world.players.get_mut(&1) {
            player.step_counter = 3;
            player.run_counter = 2;
            player.run_time = 0;
            player.cell_time = now - 1_000;
        }
        world.tick();
        assert_eq!(world.player(1).unwrap().step_counter, 0, "停顿后清零");
        assert_eq!(world.player(1).unwrap().run_counter, 1, "跑步计数衰减 1");
    }

    #[test]
    fn chat_reaches_people_in_range_only() {
        let mut world = World::new(999);
        let (speaker, mut speaker_rx) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(speaker);
        let (near, mut near_rx) = player(2, "乙", Point::new(12, 10), "0");
        world.enter(near);
        let (far, mut far_rx) = player(3, "丙", Point::new(90, 90), "0");
        world.enter(far);
        drain(&mut speaker_rx);
        drain(&mut near_rx);
        drain(&mut far_rx);

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
        drain(&mut b_rx);
        assert!(world.leave(1).is_some());
        assert!(b_rx.try_recv().is_ok(), "乙应收到 ObjectRemove");
        assert_eq!(world.len(), 1);
    }
}
