//! 打印某个地图的刷怪点（用于给 playtest 找「出生点附近的真实怪区」）。
//!
//! 用法：
//! ```text
//! cargo run -p mir-server --example spawns -- 0 330 270 80
//! cargo run -p mir-server --example spawns -- 0 330 270 80 Hen
//! ```
//!
//! 参数：`<地图文件名> [玩家x] [玩家y] [最大距离] [怪物名过滤]`。
//! 输出按「距玩家的切比雪夫距离」升序，每行给出刷怪中心、怪物名、数量与等级。

use std::env;

use mir_server::gamedata::GameData;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let map = args.first().cloned().unwrap_or_else(|| "0".to_owned());
    let px: i32 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(330);
    let py: i32 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(270);
    let max_distance: i32 = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(80);
    let name_filter = args.get(4).cloned();

    let data = match GameData::discover() {
        Ok(data) => data,
        Err(error) => {
            eprintln!("加载真实数据失败：{error}");
            std::process::exit(1);
        }
    };

    let mut rows: Vec<(i32, String, usize, u16, i32, i32)> = Vec::new();
    for group in data.spawn_groups(&map) {
        let kind = &group.kind;
        if let Some(filter) = &name_filter {
            if !kind.name.to_lowercase().contains(&filter.to_lowercase()) {
                continue;
            }
        }
        let dx = (group.center.x - px).abs();
        let dy = (group.center.y - py).abs();
        let distance = dx.max(dy);
        if distance > max_distance {
            continue;
        }
        rows.push((
            distance,
            kind.name.clone(),
            group.count,
            kind.level,
            group.center.x,
            group.center.y,
        ));
    }
    rows.sort_by_key(|row| row.0);

    println!(
        "地图 {map}：玩家 ({px},{py}) {max_distance} 格内的刷怪点 {} 组",
        rows.len()
    );
    for (distance, name, count, level, x, y) in rows.iter().take(40) {
        println!("  dist={distance:>3}  ({x:>4},{y:>4})  {name} x{count}  lv{level}");
    }
}
