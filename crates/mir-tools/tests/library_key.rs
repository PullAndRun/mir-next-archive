//! `assets unpack --plan` 的库键回归：撞名库必须显式区分，缺省仍回退到规范化 stem。

use std::path::{Path, PathBuf};
use std::process::Command;

fn temp_dir(tag: &str) -> PathBuf {
    let base = std::env::var("CARGO_TARGET_TMPDIR").map_or_else(
        |_| PathBuf::from("target/tmp-library-key-tests"),
        PathBuf::from,
    );
    let dir = base.join(format!(
        "mir2-library-key-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn run_plan(dir: &Path, plan: &serde_json::Value) -> std::process::Output {
    let plan_path = dir.join("plan.json");
    std::fs::write(
        &plan_path,
        serde_json::to_string_pretty(plan).expect("plan json"),
    )
    .expect("write plan");
    Command::new(env!("CARGO_BIN_EXE_mir-tools"))
        .args([
            "assets",
            "unpack",
            "--plan",
            plan_path.to_str().expect("plan path"),
            "--out-root",
            dir.to_str().expect("out root"),
            "--map-only",
        ])
        .output()
        .expect("run mir-tools")
}

#[test]
fn explicit_keys_keep_carmour_aarmour_ararmour_apart_and_defaults_to_stem() {
    let dir = temp_dir("explicit");
    let plan = serde_json::json!({
        "source_root": "resource/data",
        "libraries": [
            {"lib":"CArmour/00.Lib","library_key":"carmour-00","dest":"gfx/player-native/carmour-00"},
            {"lib":"AArmour/00.Lib","library_key":"aarmour-00","dest":"gfx/player-native/aarmour-00"},
            {"lib":"ARArmour/00.Lib","library_key":"ararmour-00","dest":"gfx/player-native/ararmour-00"},
            {"lib":"AWeapon/00 L.Lib","library_key":"aweapon-00-l","dest":"gfx/player-native/aweapon-00-l"},
            {"lib":"AWeapon/00 R.Lib","library_key":"aweapon-00-r","dest":"gfx/player-native/aweapon-00-r"},
            {"lib":"ARWeapon/00.Lib","library_key":"arweapon-00","dest":"gfx/player-native/arweapon-00"},
            {"lib":"ARWeapon/00 S.Lib","library_key":"arweapon-00-s","dest":"gfx/player-native/arweapon-00-s"},
            {"lib":"CHair/01.Lib","dest":"gfx/player-native/01"}
        ]
    });
    let output = run_plan(&dir, &plan);
    assert!(
        output.status.success(),
        "plan 失败: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let map: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("library-map.json")).expect("map"))
            .expect("map json");
    let libraries = map["libraries"].as_object().expect("libraries");
    for key in [
        "carmour-00",
        "aarmour-00",
        "ararmour-00",
        "aweapon-00-l",
        "aweapon-00-r",
        "arweapon-00",
        "arweapon-00-s",
    ] {
        assert!(libraries.contains_key(key), "缺少显式键 {key}");
    }
    assert!(
        libraries.contains_key("01"),
        "缺省键应回退到规范化 file_stem"
    );
    assert!(!libraries.contains_key("00"), "不能把撞名库都写成 00");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn duplicate_key_for_different_libs_fails_loudly() {
    let dir = temp_dir("collision");
    let plan = serde_json::json!({
        "source_root": "resource/data",
        "libraries": [
            {"lib":"CArmour/00.Lib","library_key":"same","dest":"gfx/a"},
            {"lib":"AArmour/00.Lib","library_key":"same","dest":"gfx/b"}
        ]
    });
    let output = run_plan(&dir, &plan);
    assert!(!output.status.success(), "撞键必须失败");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("同时指向") && stderr.contains("CArmour/00.Lib"),
        "错误信息应点出两个库: {stderr}"
    );
    std::fs::remove_dir_all(&dir).ok();
}
