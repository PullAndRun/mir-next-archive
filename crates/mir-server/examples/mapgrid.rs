//! 把一张地图的**服务端权威可走性**导出成位图，供 playtest 的 BFS 使用。
//!
//! 为什么要它：`tools/playtest/playtest.py` 里自己按「v0 固定 52 字节头 + 12 字节/格 + 三个
//! u16 的 bit15」解析 `.map`，遇到非 v0（v5 等）地图就会与服务端判断不一致——表现为
//! 「脚本认为能走、服务端返回 `target cell is a wall`」。这里直接用服务端同一套解析
//! （[`mir_world::Map`]）导出，彻底消除差异。
//!
//! 用法：
//! ```text
//! cargo run -q -p mir-server --example mapgrid -- 0
//! cargo run -q -p mir-server --example mapgrid -- 0103 --data resource/data
//! ```
//!
//! 输出 `target/mapgrid-<地图>.bin`：
//! ```text
//! 前 8 字节：width u32 (LE) + height u32 (LE)
//! 随后：位图，1 bit/格，1 = 可走；按 x 外层、y 内层（列优先，与服务端 cells 顺序一致），
//!       第 i 位的格坐标是 (i / height, i % height)，字节内低位在前。
//! ```

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let map = args.first().cloned().unwrap_or_else(|| "0".to_owned());
    let data_dir = args
        .iter()
        .position(|value| value == "--data")
        .and_then(|index| args.get(index + 1))
        .map_or_else(|| PathBuf::from("resource/data"), PathBuf::from);

    let map_path = data_dir.join("map").join(format!("{map}.map"));
    let bytes = match fs::read(&map_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("读取地图失败 {}：{error}", map_path.display());
            std::process::exit(1);
        }
    };
    let parsed = match mir_world::Map::from_bytes_named(&bytes, &map) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("解析地图失败 {}：{error}", map_path.display());
            std::process::exit(1);
        }
    };

    let width = parsed.width;
    let height = parsed.height;
    let total = (width as usize) * (height as usize);
    let mut bits = vec![0u8; (total + 7) / 8];
    for x in 0..width {
        for y in 0..height {
            if parsed.walkable(x as i32, y as i32) {
                let index = (x as usize) * (height as usize) + (y as usize);
                bits[index / 8] |= 1 << (index % 8);
            }
        }
    }

    let mut out = Vec::with_capacity(8 + bits.len());
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&bits);

    fs::create_dir_all("target").ok();
    let out_path = PathBuf::from(format!("target/mapgrid-{map}.bin"));
    if let Err(error) = fs::write(&out_path, &out) {
        eprintln!("写出失败 {}：{error}", out_path.display());
        std::process::exit(1);
    }
    let walkable = bits.iter().map(|byte| byte.count_ones() as usize).sum::<usize>();
    println!(
        "地图 {map}：{width}x{height}，可走 {walkable} 格 → {}（{} 字节）",
        out_path.display(),
        out.len()
    );
}
