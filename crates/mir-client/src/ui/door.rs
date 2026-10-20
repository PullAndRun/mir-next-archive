//! 客户端开门交互：点地图上的门格 → `C.Opendoor{door_index}`（index 136）。
//!
//! 门索引来自 `mir-world::MapCell::door_index`（服务器同一解析：v0 `o+8 > 0`，
//! `& 0x7F`；门号 0 也是合法门），不在这里重新解析 `.map`。
//!
//! Crystal `PlayerObject.Opendoor`（`PlayerObject.cs:7818-7826`）**没有距离校验**；
//! 本作保持同样语义：只要点到门格就发。若以后加距离限制，必须注明是本作扩展，
//! 且不能把它当安全边界（服务端不校验距离）。

use mir_core::Point;
use mir_world::Map;

/// `C.Opendoor` 的包编号（`crates/mir-protocol/src/packets/client.rs`，index 136）。
pub const OPENDOOR_PACKET_INDEX: i16 = 136;

/// 点中的门索引；非门格/越界返回 `None`。
///
/// 门号 0 合法：`Some(0)` 照样返回，不能按“非零”过滤。
#[must_use]
pub fn door_at(map: &Map, cell: Point) -> Option<u8> {
    map.cell(cell.x, cell.y).and_then(|cell| cell.door_index)
}

/// 一次点击的开门目标。
///
/// 与 Crystal 一致：**不做距离校验**；返回 `None` 只表示该格没有门/地图越界。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoorClick {
    /// 要发给服务端的门索引（0 也合法）。
    pub door_index: u8,
}

impl DoorClick {
    /// `C.Opendoor` 的 payload：1 字节门号（协议 index 136）。
    #[must_use]
    pub fn packet_payload(&self) -> [u8; 1] {
        [self.door_index]
    }
}

/// 解析左键点击：门格 → `Some(DoorClick)`；非门格 → `None`。
#[must_use]
pub fn click(map: &Map, cell: Point) -> Option<DoorClick> {
    door_at(map, cell).map(|door_index| DoorClick { door_index })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir_world::{MapCell, MapVersion};

    fn map() -> Map {
        let width = 6;
        let height = 6;
        let mut cells = vec![MapCell::default(); usize::try_from(width * height).unwrap()];
        // 列优先：x * height + y。
        let mut set = |x: i32, y: i32, door: Option<u8>| {
            let index = usize::try_from(x * height + y).unwrap();
            cells[index] = MapCell {
                door_index: door,
                ..MapCell::default()
            };
        };
        set(1, 1, Some(10));
        set(2, 2, Some(0));
        set(3, 3, None);
        Map {
            width,
            height,
            cells,
            file_name: "0".to_owned(),
            version: MapVersion::V0,
            doors: Vec::new(),
        }
    }

    #[test]
    fn known_door_returns_exact_index() {
        let click = click(&map(), Point::new(1, 1)).expect("门格");
        assert_eq!(click.door_index, 10);
        assert_eq!(OPENDOOR_PACKET_INDEX, 136);
        assert_eq!(click.packet_payload(), [10]);
    }

    #[test]
    fn door_index_zero_is_legal() {
        let click = click(&map(), Point::new(2, 2)).expect("门号 0 也是门");
        assert_eq!(click, DoorClick { door_index: 0 });
        assert_eq!(click.packet_payload(), [0]);
    }

    #[test]
    fn non_door_and_out_of_bounds_do_not_send() {
        let map = map();
        assert_eq!(click(&map, Point::new(3, 3)), None);
        assert_eq!(click(&map, Point::new(-1, 0)), None);
        assert_eq!(click(&map, Point::new(99, 99)), None);
    }

    #[test]
    fn far_door_still_sends_because_crystal_has_no_range_check() {
        // Crystal PlayerObject.Opendoor 无距离校验；本作保持同语义。
        assert_eq!(
            click(&map(), Point::new(1, 1)),
            Some(DoorClick { door_index: 10 })
        );
    }

    #[test]
    fn empty_or_doorless_map_is_silent() {
        let map = Map {
            width: 1,
            height: 1,
            cells: vec![MapCell::default()],
            file_name: "blank".to_owned(),
            version: MapVersion::V0,
            doors: Vec::new(),
        };
        assert_eq!(door_at(&map, Point::new(0, 0)), None);
        assert_eq!(click(&map, Point::new(0, 0)), None);
    }
}
