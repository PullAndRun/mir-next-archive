//! 素材索引/缺图清单（t84）。
//!
//! 复现：
//! ```text
//! cargo run --release -p mir-asset --example asset_scan -- \
//!     --data resource/data --out target/asset-missing.json
//! ```
//!
//! 扫描内容：
//! - 攻城器械候选帧：`objects.wzl` 的 `3 + kind`（t69 的约定）；
//! - 怪物图库：`mon*.wzl` 每库前 100 帧里非空帧数量，0 张或首帧为空 → 记入缺图清单；
//! - 玩家/武器/头发/特效库是否存在。
//!
//! 输出 JSON 供决定「保留色块回退还是换帧号」；工具只读素材，不写 resource。

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use mir_asset::ImageLibrary;

fn main() {
    let mut data = PathBuf::from("resource/data");
    let mut out = PathBuf::from("target/asset-missing.json");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data" => data = args.next().map(PathBuf::from).unwrap_or(data),
            "--out" => out = args.next().map(PathBuf::from).unwrap_or(out),
            other => eprintln!("忽略未知参数：{other}"),
        }
    }

    let objects = ImageLibrary::open(data.join("objects.wzl")).ok();
    let mut conquest = String::new();
    for (name, frame) in [("Gate", 3u32), ("Wall", 4), ("Archer", 5)] {
        let entry = objects.as_ref().and_then(|library| {
            library
                .info(usize::try_from(frame).ok()?)
                .ok()
                .filter(|info| info.width > 0 && info.height > 0)
        });
        let _ = writeln!(
            conquest,
            "    {{\"kind\": \"{name}\", \"library\": \"objects.wzl\", \"frame\": {frame}, \"present\": {}, \"width\": {}, \"height\": {}}},\n",
            entry.is_some(),
            entry.map_or(0, |info| info.width),
            entry.map_or(0, |info| info.height)
        );
    }

    let mut monsters_json = String::new();
    let mut mon_files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&data) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let lower = name.to_ascii_lowercase();
            let is_wzl = path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("wzl"));
            if lower.starts_with("mon") && is_wzl {
                mon_files.push(path);
            }
        }
    }
    mon_files.sort();
    for path in &mon_files {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("?")
            .to_owned();
        let Ok(library) = ImageLibrary::open(path) else {
            let _ = writeln!(
                monsters_json,
                "    {{\"library\": \"{name}\", \"frames\": 0, \"non_empty\": 0, \"first_non_empty\": 0, \"empty_intervals\": [], \"empty_standing_blocks_first10\": 0}},\n"
            );
            continue;
        };
        let limit = library.len();
        let mut non_empty = 0usize;
        let mut first_non_empty = None;
        let mut empty_intervals = Vec::new();
        let mut run_start = None;
        for index in 0..limit {
            let present = library
                .info(index)
                .is_ok_and(|info| info.width > 0 && info.height > 0);
            if present {
                non_empty += 1;
                first_non_empty.get_or_insert(index);
                if let Some(start) = run_start.take()
                    && index - start >= 8
                {
                    empty_intervals.push((start, index - 1));
                }
            } else if run_start.is_none() {
                run_start = Some(index);
            }
        }
        if let Some(start) = run_start
            && limit - start >= 8
        {
            empty_intervals.push((start, limit - 1));
        }
        // 每个图像块 430 帧，站立段起点 = block*430；统计前 10 块里站立帧为空的块。
        let mut empty_standing = 0usize;
        let blocks = (limit / 430).min(10);
        for block in 0..blocks {
            let base = block * 430;
            let present = library
                .info(base)
                .is_ok_and(|info| info.width > 0 && info.height > 0);
            if !present {
                empty_standing += 1;
            }
        }
        if non_empty == 0 || first_non_empty.unwrap_or(0) >= 100 || !empty_intervals.is_empty() {
            let intervals = empty_intervals
                .iter()
                .take(5)
                .map(|(start, end)| format!("[{start},{end}]"))
                .collect::<Vec<_>>()
                .join(",");
            let _ = writeln!(
                monsters_json,
                "    {{\"library\": \"{name}\", \"frames\": {limit}, \"non_empty\": {non_empty}, \"first_non_empty\": {}, \"empty_intervals\": [{intervals}], \"empty_standing_blocks_first10\": {empty_standing}}},\n",
                first_non_empty.unwrap_or(0)
            );
        }
    }

    let mut essential = String::new();
    for name in [
        "hum.wzl",
        "hum2.wzl",
        "weapon.wzl",
        "hair.wzl",
        "npc.wzl",
        "effect.wzl",
        "magic.wzl",
        "tiles.wzl",
        "objects.wzl",
    ] {
        let path = data.join(name);
        let present = LibraryPresence::check(&path);
        let _ = writeln!(
            essential,
            "    \"{name}\": {{\"present\": {}, \"frames\": {}}},\n",
            present.0, present.1
        );
    }

    let json = format!(
        "{{\n  \"data_dir\": \"{}\",\n  \"conquest_candidates\": [\n{}  ],\n  \"monster_libraries_audit\": [\n{}  ],\n  \"essential_libraries\": {{\n{}  }}\n}}\n",
        data.display(),
        conquest,
        monsters_json,
        essential
    );
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).expect("创建输出目录");
    }
    std::fs::write(&out, json).expect("写缺图清单");

    println!(
        "扫描 {} 个 mon*.wzl（首帧偏移/空帧区间/空站立块）；objects.wzl 候选帧见 {}",
        mon_files.len(),
        out.display()
    );
}

struct LibraryPresence(bool, usize);

impl LibraryPresence {
    fn check(path: &Path) -> Self {
        ImageLibrary::open(path).map_or(Self(false, 0), |library| Self(true, library.len()))
    }
}
