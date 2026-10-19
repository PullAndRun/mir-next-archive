//! 打印某地图的传送点（`MapInfo.Movements`），用于给 playtest 找「进/出城的自然传送坐标」。
//!
//! 用法：
//! ```text
//! cargo run -q -p mir-server --example movements -- 0
//! cargo run -q -p mir-server --example movements -- 0103
//! cargo run -q -p mir-server --example movements -- 0103 0
//! ```
//!
//! 参数：`<地图文件名> [只显示目标地图文件名的点]`。
//! 每行给出源坐标、目标地图文件名/索引、目标坐标、以及是否 `ShowOnBigMap`。

use std::env;

use mir_server::gamedata::GameData;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let map = args.first().cloned().unwrap_or_else(|| "0".to_owned());
    let only_target = args.get(1).cloned();

    let data = match GameData::discover() {
        Ok(data) => data,
        Err(error) => {
            eprintln!("加载真实数据失败：{error}");
            std::process::exit(1);
        }
    };
    let Some(info) = data.map_info(&map) else {
        eprintln!("地图 {map} 不在 MirDB 里");
        std::process::exit(1);
    };
    // 目标图文件名（用索引反查），找不到就打印 `?#index`。
    let target_name = |index: i32| -> String {
        data.map_info_by_index(index)
            .map(|info| info.file_name.clone())
            .unwrap_or_else(|| format!("?#{index}"))
    };

    println!(
        "地图 {map}（index {}）：{} 条传送点",
        info.index,
        info.movements.len()
    );
    let mut shown = 0;
    for movement in &info.movements {
        let target = target_name(movement.map_index);
        if let Some(only) = &only_target {
            if &target != only {
                continue;
            }
        }
        shown += 1;
        println!(
            "  ({:>4},{:>4}) -> {target}(#{}) ({:>4},{:>4})  bigmap={} hole={} move={}",
            movement.source.x,
            movement.source.y,
            movement.map_index,
            movement.destination.x,
            movement.destination.y,
            movement.show_on_big_map,
            movement.need_hole,
            movement.need_move,
        );
    }
    println!("  共显示 {shown} 条");
}
