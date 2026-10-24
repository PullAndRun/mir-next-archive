//! R49 A2：用真实 `.Lib` 验证「导入 → frames.json → pack → 只读 pack」链路。
//!
//! 这个测试不碰客户端/渲染，也不在运行时回读 `resource/`：导入阶段只由
//! `mir-tools assets unpack` 读 `.Lib`，之后帧表和图像全部从 `game-assets/packs`
//! 打开。默认路径和发布路径读的是同一份 `frames.json`/PNG，因此这里对 pack
//! 的断言就是发布消费契约。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use mir_asset::PackSet;
use serde_json::Value;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn source_root() -> PathBuf {
    workspace_root()
        .join("resource/community/mirfiles-alliance/Alliance/Data/Data(Mount-Npc...)")
        .join("Monster")
}

fn tool_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mir-tools"))
}

fn run(args: &[&str]) {
    let output = Command::new(tool_bin())
        .args(args)
        .output()
        .expect("run mir-tools");
    assert!(
        output.status.success(),
        "mir-tools {:?} failed:\nstdout={}\nstderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read json")).expect("parse json")
}

#[test]
#[ignore = "真实 game-assets 核心三库计数，需先执行 R49 A2 导入"]
fn core_three_libraries_have_expected_counts() {
    let assets = workspace_root().join("game-assets/gfx/sprites/monster-native");
    for (key, expected) in [
        ("monster-005", 234),
        ("monster-022", 244),
        ("monster-023", 224),
    ] {
        let index = read_json(&assets.join(key).join("index.json"));
        assert_eq!(index["count"], expected, "{key}");
        assert_eq!(index["indices_out_of_range"], 0, "{key}");
        let written = index["written_png"].as_u64().expect("written_png");
        let skipped = index["skipped_empty"].as_u64().expect("skipped_empty");
        assert_eq!(written + skipped, expected, "{key}");
        let frames = read_json(&assets.join(key).join("frames.json"));
        assert_eq!(frames["frame_source"], "native", "{key}");
        assert_eq!(
            frames["source_hash"].as_str().map(str::len),
            Some(64),
            "{key}"
        );
    }
}

#[test]
#[ignore = "真实 005/022 导入、pack 往返，需 resource 与几秒磁盘 IO"]
fn real_monster_native_import_pack_round_trip_without_source_readback() {
    let root = workspace_root();
    let work = root.join("target/r49-test-monster-native");
    let assets = work.join("assets");
    if work.exists() {
        fs::remove_dir_all(&work).expect("clean test work");
    }
    fs::create_dir_all(&assets).expect("create test assets");

    let plan = serde_json::json!({
        "source_root": source_root(),
        "libraries": [
            {"lib": "005.Lib", "library_key": "monster-005", "dest": "gfx/sprites/monster-native/monster-005", "indices": ["0-233"]},
            {"lib": "022.Lib", "library_key": "monster-022", "dest": "gfx/sprites/monster-native/monster-022", "indices": ["0-243"]},
        ]
    });
    let plan_path = work.join("plan.json");
    fs::write(&plan_path, serde_json::to_vec_pretty(&plan).unwrap()).expect("write plan");
    let plan_s = plan_path.to_string_lossy().to_string();
    let assets_s = assets.to_string_lossy().to_string();
    run(&[
        "assets",
        "unpack",
        "--plan",
        &plan_s,
        "--out-root",
        &assets_s,
    ]);

    let expected: [(&str, usize, usize); 2] = [("monster-005", 234, 7), ("monster-022", 244, 7)];
    for (key, image_count, frame_count) in expected {
        let dir = assets.join("gfx/sprites/monster-native").join(key);
        let index = read_json(&dir.join("index.json"));
        assert_eq!(index["count"], image_count);
        assert_eq!(index["written_png"], image_count);
        assert_eq!(index["skipped_empty"], 0);
        let frames = read_json(&dir.join("frames.json"));
        assert_eq!(frames["schema_version"], 1);
        assert_eq!(frames["image"], key);
        assert_eq!(frames["library_key"], key);
        assert_eq!(frames["frame_source"], "native", "{key}");
        assert_eq!(frames["layout"], "v3_frames", "{key}");
        assert_eq!(frames["count"], frame_count, "{key}");
        assert_eq!(frames["frame_count"], frame_count, "{key}");
        assert!(
            frames["source_hash"]
                .as_str()
                .is_some_and(|hash| hash.len() == 64),
            "{key} source_hash"
        );
        assert_eq!(frames["source_hash"], frames["source_sha256"], "{key}");
        let actions = frames["actions"].as_array().expect("actions");
        assert_eq!(actions.len(), 7, "{key} must keep all 7 native actions");
        let names: Vec<&str> = actions
            .iter()
            .map(|action| action["action"].as_str().expect("action name"))
            .collect();
        assert_eq!(
            names,
            [
                "Standing", "Walking", "Attack1", "Struck", "Die", "Dead", "Revive"
            ],
            "{key}"
        );
        for action in actions {
            let count = action["count"].as_i64().expect("count");
            let skip = action["skip"].as_i64().expect("skip");
            let frames = action["frames_by_direction"]
                .as_array()
                .expect("frames_by_direction");
            assert_eq!(frames.len(), 8, "{key} {}: 8 directions", action["action"]);
            for (direction, indices) in frames.iter().enumerate() {
                let indices = indices.as_array().expect("indices");
                assert_eq!(indices.len() as i64, count, "{key} dir {direction}");
                let first = indices[0].as_i64().expect("first index");
                assert_eq!(
                    first,
                    action["start"].as_i64().unwrap() + direction as i64 * (count + skip),
                    "{key} dir {direction}"
                );
            }
        }
    }
    assert!(
        !assets
            .join("gfx/sprites/monster-native/monster-000")
            .exists()
    );

    // 发布路径：只打开 packs；测试不读 `resource/` 或 unpacked PNG 作为运行时输入。
    let packs = assets.join("packs");
    run(&[
        "assets",
        "pack",
        "--input",
        &assets_s,
        "--out",
        &packs.to_string_lossy(),
    ]);
    let packset = PackSet::open_unverified(&packs).expect("open release packs");
    assert!(
        packset
            .manifest()
            .packs
            .iter()
            .any(|p| p.category == "sprites")
    );
    assert!(
        packset
            .names()
            .any(|name| name.ends_with("monster-005/frames.json"))
    );
    assert!(
        packset
            .names()
            .any(|name| name.ends_with("monster-022/frames.json"))
    );
    assert!(!packset.names().any(|name| name.contains("monster-000/")));

    let packed_frames = packset
        .data("gfx/sprites/monster-native/monster-005/frames.json")
        .expect("frames.json must be a raw pack entry");
    let packed: Value = serde_json::from_slice(&packed_frames).expect("parse packed frames");
    assert_eq!(packed["frame_source"], "native");
    assert_eq!(packed["actions"].as_array().unwrap().len(), 7);

    let disk_png = assets.join("gfx/sprites/monster-native/monster-005/0000.png");
    let disk_image = packset
        .image("gfx/sprites/monster-native/monster-005/0000.png")
        .expect("packed png");
    let (disk_rgba, width, height) = decode_png(&disk_png);
    assert_eq!(
        disk_image.pixels, disk_rgba,
        "pack must preserve PNG pixels"
    );
    assert_eq!(u32::from(disk_image.info.width), width);
    assert_eq!(u32::from(disk_image.info.height), height);
    let index = read_json(&assets.join("gfx/sprites/monster-native/monster-005/index.json"));
    let packed_image = packset
        .image("gfx/sprites/monster-native/monster-005/0000.png")
        .expect("packed image metadata");
    assert_eq!(
        i32::from(packed_image.info.offset_x),
        index["images"][0]["offset_x"].as_i64().unwrap() as i32,
        "pack must preserve image X offset"
    );
    assert_eq!(
        i32::from(packed_image.info.offset_y),
        index["images"][0]["offset_y"].as_i64().unwrap() as i32,
        "pack must preserve image Y offset"
    );
}

fn decode_png(path: &Path) -> (Vec<u8>, u32, u32) {
    let file = std::io::BufReader::new(fs::File::open(path).expect("open png"));
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().expect("png info");
    let mut buf = vec![0; reader.output_buffer_size().expect("png buffer size")];
    let info = reader.next_frame(&mut buf).expect("png frame");
    assert_eq!(info.color_type, png::ColorType::Rgba);
    buf.truncate(info.buffer_size());
    (buf, info.width, info.height)
}

#[test]
#[ignore = "真实库 verify 往返"]
fn real_monster_native_verify_checks_frames_json() {
    let root = workspace_root();
    let work = root.join("target/r49-test-monster-native-verify");
    let assets = work.join("assets");
    if work.exists() {
        fs::remove_dir_all(&work).expect("clean test work");
    }
    fs::create_dir_all(&assets).expect("create test assets");
    let plan = serde_json::json!({
        "source_root": source_root(),
        "libraries": [
            {"lib": "005.Lib", "library_key": "monster-005", "dest": "gfx/sprites/monster-native/monster-005", "indices": ["0-233"]}
        ]
    });
    let plan_path = work.join("plan.json");
    fs::write(&plan_path, serde_json::to_vec_pretty(&plan).unwrap()).expect("write plan");
    let plan_s = plan_path.to_string_lossy().to_string();
    let assets_s = assets.to_string_lossy().to_string();
    run(&[
        "assets",
        "unpack",
        "--plan",
        &plan_s,
        "--out-root",
        &assets_s,
    ]);
    run(&[
        "assets",
        "unpack",
        "--plan",
        &plan_s,
        "--out-root",
        &assets_s,
        "--verify",
    ]);
    let frames_path = assets.join("gfx/sprites/monster-native/monster-005/frames.json");
    let frames = read_json(&frames_path);
    assert_eq!(frames["frame_source"], "native");
    assert_eq!(frames["actions"].as_array().unwrap().len(), 7);
    fs::write(&frames_path, b"{\"frame_source\":\"native_frames\"}").expect("tamper");
    let output = Command::new(tool_bin())
        .args([
            "assets",
            "unpack",
            "--plan",
            &plan_s,
            "--out-root",
            &assets_s,
            "--verify",
        ])
        .output()
        .expect("run verify");
    assert!(
        !output.status.success(),
        "tampered frames.json must fail verify"
    );
}

#[test]
#[ignore = "读取已构建的 118 库 packs；不重新导入"]
fn all_a0_native_monsters_in_release_packs() {
    let root = workspace_root();
    let plan = read_json(&root.join("target/native-layout/native-monster-all-plan.json"));
    let libs = plan["libraries"].as_array().expect("libraries");
    assert_eq!(
        libs.len(),
        118,
        "A0 全部 118 个 v3 库都必须进入 native plan"
    );
    let packset =
        PackSet::open_unverified(root.join("game-assets/packs")).expect("open release packs");
    for lib in libs {
        let key = lib["library_key"].as_str().expect("key");
        let prefix = format!("gfx/sprites/monster-native/{key}/");
        let frames_name = format!("{prefix}frames.json");
        let frames_data = packset
            .data(&frames_name)
            .unwrap_or_else(|_| panic!("{key}: missing packed frames.json"));
        let frames: Value = serde_json::from_slice(&frames_data).expect("parse frames");
        assert_eq!(frames["frame_source"], "native", "{key}");
        assert!(frames["actions"].as_array().unwrap().len() >= 2, "{key}");
        assert!(
            packset
                .names()
                .any(|name| name == format!("{prefix}0000.png")),
            "{key}: first frame missing"
        );
    }
    // t46：stride=0 的 10 个库必须已经进入 native pack，不能再被拒绝。
    for key in [
        "monster-010",
        "monster-041",
        "monster-043",
        "monster-062",
        "monster-117",
        "monster-130",
        "monster-131",
        "monster-132",
        "monster-164",
        "monster-218",
    ] {
        assert!(
            packset
                .names()
                .any(|name| name == format!("gfx/sprites/monster-native/{key}/frames.json")),
            "{key}: stride=0 library must be packed"
        );
    }
}
