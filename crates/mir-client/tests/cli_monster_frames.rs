//! R49-X1-03：CLI `--server` 联机路径的 session 帧表必须与渲染同源。
//!
//! 背景：`run_connected` 曾用 [`ResolvedAssets::discover_for_data_dir`]（只认
//! `MIR2_ASSET_MODE` 环境变量与 `auto`）重新发现素材根，而渲染用的是 `main` 里按
//! `--asset-dir`/`--asset-mode` 解析出的 [`ResolvedAssets`]。dev 与 packs 并存时
//! 会出现「session 读 dev、渲染读 pack（或反之）」。
//!
//! 修复后 `run_connected` 调用
//! [`mir_client::assets::load_monster_frames_from_resolved`]，把与
//! `MapScene::load_with_sources` **同一组**参数（`data_dir`、`asset_root`、
//! `resolved`、`allow_legacy`）交给帧表加载器。本测试按 CLI 的真实解析链
//! （[`AssetRoot::resolve`] → [`ResolvedAssets::load`] → 唯一同源入口）在真实
//! `game-assets/` 上验证：
//!
//! 1. session 帧表与渲染 `EntityLibraries` 逐怪相等（同一素材根、同一 asset-mode）；
//! 2. `8` 方向帧下标 / `count` / `interval` 与真实 `frames.json` 一致（不是默认值）；
//! 3. `auto`/`dev`/`release` 三种 `--asset-mode` 语义都非空且帧表互等；
//! 4. 同一份表装进 `ConnectOptions::monster_frames` 后仍携带真实值。
//!
//! 真实素材缺失时必须失败（不允许空过）。两测试均带 `#[ignore]`（真实素材较慢），
//! 显式运行：`cargo test -p mir-client --test cli_monster_frames -- --ignored`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mir_asset::FrameSet;
use mir_client::assets::{
    AssetMode, AssetRoot, ResolvedAssets, gfx_dir_for_data_dir, load_monster_frames,
    load_monster_frames_from_resolved,
};
use mir_client::net::ConnectOptions;
use mir_render::MapScene;

/// 仓库根（`crates/mir-client` 的上两级）。
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/mir-client 的上两级是仓库根")
        .to_path_buf()
}

/// 与 CLI 相同的素材解析结果：`--asset-dir game-assets` + `--asset-mode <mode>`。
struct CliAssets {
    root: PathBuf,
    map_dir: PathBuf,
    data_dir: PathBuf,
    resolved: ResolvedAssets,
    allow_legacy: bool,
}

/// 照抄 `main` 的解析链：`AssetRoot::resolve`（显式 CLI 路径）→
/// `ResolvedAssets::load(root, mode)`；`allow_legacy` 与 `run_connected` 一样取
/// 请求的 mode（不是 `effective()`）。
fn resolve_like_cli(mode: AssetMode) -> CliAssets {
    let game_assets = workspace_root().join("game-assets");
    assert!(
        game_assets.is_dir(),
        "缺 {}（真实素材；本测试带 #[ignore]，缺素材必须失败而不是空过）",
        game_assets.display()
    );
    let root = AssetRoot::resolve(Some(&game_assets), None, None);
    assert_eq!(
        root.source().as_str(),
        "cli",
        "显式 --asset-dir 应被采用（实际来源 {:?}）",
        root.source()
    );
    let resolved = ResolvedAssets::load(root.root(), mode);
    CliAssets {
        root: root.root().to_path_buf(),
        map_dir: root.map_dir(),
        data_dir: root.data_dir(),
        allow_legacy: mode.allow_legacy(),
        resolved,
    }
}

/// `run_connected` 修复后的 session 帧表加载（渲染与 session 的唯一同源入口）。
fn session_frames(cli: &CliAssets) -> BTreeMap<u16, FrameSet> {
    load_monster_frames_from_resolved(
        &cli.data_dir,
        Some(cli.root.as_path()),
        &cli.resolved,
        cli.allow_legacy,
    )
}

/// 用 `run_connected`/`load_scene_with_fallback` 相同的 7 个参数打开渲染场景，
/// 这样才能从 `entity_libraries()` 取渲染实际使用的帧表做对照。
fn render_scene(cli: &CliAssets) -> MapScene {
    let map_path = cli.map_dir.join("0.map");
    assert!(
        map_path.is_file() || map_path.with_extension("mnm").is_file(),
        "缺地图 {}（真实素材）",
        map_path.display()
    );
    let gfx_dir = gfx_dir_for_data_dir(&cli.data_dir);
    MapScene::load_with_sources(
        &map_path,
        &cli.data_dir,
        Some(&gfx_dir),
        Some(cli.root.as_path()),
        cli.resolved.map.as_ref(),
        cli.resolved.packs.as_ref(),
        cli.allow_legacy,
    )
    .unwrap_or_else(|error| {
        panic!(
            "渲染场景应能用同一份 ResolvedAssets 打开 {}：{error}",
            map_path.display()
        )
    })
}

/// `monster-native/monster-XXX/frames.json` 路径 → 外观号（`frames.json` 顶层的
/// `image` 是字符串，这里从目录名取数）。
fn image_from_path(path: &Path) -> u16 {
    let dir = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| panic!("frames.json 的目录名：{}", path.display()));
    let digits = dir
        .strip_prefix("monster-")
        .unwrap_or_else(|| panic!("怪物目录名应为 monster-XXX：{dir}"));
    digits
        .parse::<u16>()
        .unwrap_or_else(|error| panic!("怪物外观号 {digits}：{error}"))
}

#[test]
#[ignore = "需要真实 game-assets 原生怪物拆包（118 个 frames.json）；显式：cargo test -p mir-client --test cli_monster_frames -- --ignored"]
fn cli_session_frames_share_render_asset_root_and_real_interval() {
    let mut tables = BTreeMap::new();
    for mode in [AssetMode::Auto, AssetMode::Dev, AssetMode::Release] {
        let cli = resolve_like_cli(mode);
        let expected = if mode == AssetMode::Release {
            AssetMode::Release
        } else {
            AssetMode::Dev
        };
        assert_eq!(
            cli.resolved.effective(),
            Some(expected),
            "--asset-mode {} 应解析成 {}（本仓库 dev 与 packs 并存）",
            mode.as_str(),
            expected.as_str()
        );
        if expected == AssetMode::Release {
            assert!(
                cli.resolved
                    .packs
                    .as_ref()
                    .is_some_and(|packs| !packs.is_empty()),
                "--asset-mode release 应真的打开 packs/"
            );
        }

        let session = session_frames(&cli);
        assert!(
            session.len() >= 100,
            "--asset-mode {} 只读到 {} 个原生怪物帧表",
            mode.as_str(),
            session.len()
        );

        // 单点证据：渲染从同一份 ResolvedAssets 读到的帧表必须与 session 逐怪相等。
        let scene = render_scene(&cli);
        let render = scene.entity_libraries();
        for (image, frames) in &session {
            let key = format!("monster-{image:03}");
            let from_render = render.monster_frames(&key).unwrap_or_else(|| {
                panic!("session 有 {key} 帧表，渲染（同一份 ResolvedAssets）却读不到：素材根不一致")
            });
            assert_eq!(
                *from_render,
                *frames,
                "{key}：session 与渲染帧表不一致（asset-mode {}）",
                mode.as_str()
            );
        }
        tables.insert(mode.as_str(), session);
    }

    let auto = tables.get("auto").expect("auto 模式帧表").clone();
    assert_eq!(
        &auto,
        tables.get("dev").expect("dev 模式帧表"),
        "dev 与 packs 并存时 auto 应等价于 dev"
    );
    assert_eq!(
        &auto,
        tables.get("release").expect("release 模式帧表"),
        "auto 与 release 的 session 帧表应来自同一份真实素材"
    );

    verify_real_frames_json(&resolve_like_cli(AssetMode::Auto), &auto);

    // `run_connected` 的做法：同一份表直接进 ConnectOptions::monster_frames。
    let options = ConnectOptions {
        server: "127.0.0.1:7000".to_owned(),
        account: "cli".to_owned(),
        password: "cli".to_owned(),
        character: "cli".to_owned(),
        create: None,
        auto_register: true,
        manual_select: false,
        monster_frames: auto.clone(),
    };
    assert_eq!(
        options.monster_frames, auto,
        "ConnectOptions::monster_frames 应携带真实帧表"
    );
}

/// 用真实 `frames.json` 反查 session 帧表：8 方向帧下标与 `count`/`interval`
/// 必须来自文件，不能是 `DefaultMonster` 的兜底值。
fn verify_real_frames_json(cli: &CliAssets, session: &BTreeMap<u16, FrameSet>) {
    let native = cli.root.join("gfx/sprites/monster-native");
    assert!(native.is_dir(), "缺 {}", native.display());
    let mut files: Vec<PathBuf> = std::fs::read_dir(&native)
        .expect("读 monster-native")
        .flatten()
        .map(|entry| entry.path().join("frames.json"))
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    assert!(
        files.len() >= 3,
        "monster-native 下 frames.json 太少：{}",
        files.len()
    );

    let mut checked_monsters = 0usize;
    for path in files {
        let image = image_from_path(&path);
        let Some(set) = session.get(&image) else {
            continue;
        };
        let text = std::fs::read_to_string(&path).expect("读真实 frames.json");
        let json: serde_json::Value =
            serde_json::from_str(&text).expect("真实 frames.json 应是合法 JSON");
        let actions = json["actions"].as_array().expect("frames.json.actions");
        assert!(!actions.is_empty(), "{} 没有动作", path.display());

        for action in actions {
            let raw = u8::try_from(
                action["raw_action"]
                    .as_u64()
                    .expect("frames.json 动作 raw_action"),
            )
            .expect("raw_action 范围");
            let frame = set.get(raw).unwrap_or_else(|| {
                panic!("session 缺外观 {image} 的动作 {raw}（{}）", path.display())
            });
            assert_eq!(
                frame.start,
                i32::try_from(action["start"].as_i64().expect("start")).expect("start 范围")
            );
            assert_eq!(
                frame.count,
                i32::try_from(action["count"].as_i64().expect("count")).expect("count 范围")
            );
            assert_eq!(
                frame.skip,
                i32::try_from(action["skip"].as_i64().expect("skip")).expect("skip 范围")
            );
            assert_eq!(
                frame.interval,
                i32::try_from(action["interval"].as_i64().expect("interval"))
                    .expect("interval 范围")
            );
            assert_eq!(
                frame.effect_start,
                i32::try_from(action["effect_start"].as_i64().expect("effect_start"))
                    .expect("effect_start 范围")
            );
            assert_eq!(
                frame.effect_count,
                i32::try_from(action["effect_count"].as_i64().expect("effect_count"))
                    .expect("effect_count 范围")
            );
            assert_eq!(
                frame.effect_skip,
                i32::try_from(action["effect_skip"].as_i64().expect("effect_skip"))
                    .expect("effect_skip 范围")
            );
            assert_eq!(
                frame.effect_interval,
                i32::try_from(action["effect_interval"].as_i64().expect("effect_interval"))
                    .expect("effect_interval 范围")
            );
            assert_eq!(frame.reverse, action["reverse"].as_bool().expect("reverse"));
            assert_eq!(frame.blend, action["blend"].as_bool().expect("blend"));

            let directions = action["frames_by_direction"]
                .as_array()
                .expect("frames_by_direction");
            assert_eq!(
                directions.len(),
                8,
                "外观 {image} 动作 {raw} 必须是 8 方向帧表"
            );
            for (direction, expected) in directions.iter().enumerate() {
                let expected: Vec<i32> = expected
                    .as_array()
                    .expect("方向帧数组")
                    .iter()
                    .map(|value| {
                        i32::try_from(value.as_i64().expect("帧下标")).expect("帧下标范围")
                    })
                    .collect();
                let range = frame.frame_indices(direction).unwrap_or_else(|| {
                    panic!("外观 {image} 动作 {raw} 方向 {direction} 没有帧下标范围")
                });
                assert_eq!(
                    range.collect::<Vec<i32>>(),
                    expected,
                    "外观 {image} 动作 {raw} 方向 {direction}：session 帧下标与真实 frames.json 不一致"
                );
            }
        }
        checked_monsters += 1;
        if checked_monsters >= 3 {
            break;
        }
    }
    assert!(
        checked_monsters >= 3,
        "只校验了 {checked_monsters} 个真实 frames.json"
    );
}

#[test]
#[ignore = "需要真实 game-assets 原生怪物拆包（118 个 frames.json）；显式：cargo test -p mir-client --test cli_monster_frames -- --ignored"]
fn cli_session_frames_follow_resolved_root_not_data_dir_parent() {
    let cli = resolve_like_cli(AssetMode::Dev);
    // 不用系统 %TEMP%：本机沙箱对默认临时目录写盘返回 Os error 5（拒绝访问），
    // 这里改用仓库 target/（已 gitignore），保证测试在受限环境也能跑。
    let probe_root = workspace_root()
        .join("target/r49")
        .join(format!("x1-03-probe-{}", std::process::id()));
    let probe_data = probe_root.join("data");
    let _ = std::fs::remove_dir_all(&probe_root);
    std::fs::create_dir_all(&probe_data).expect("建临时 probe/data");

    // 显式 asset_root（CLI 解析出的 game-assets）应生效：data_dir 在别的目录也能
    // 读到真实帧表，证明窗口不再依赖 `data_dir.parent()` 隐式发现。
    let frames = load_monster_frames_from_resolved(
        &probe_data,
        Some(cli.root.as_path()),
        &cli.resolved,
        cli.allow_legacy,
    );
    assert!(
        frames.len() >= 100,
        "显式 asset_root 未生效：只读到 {} 个怪物帧表",
        frames.len()
    );

    // 旧行为（仅按 data_dir.parent() 发现）在 probe 根上必然读空，正是修复点。
    let discovered = ResolvedAssets::discover_for_data_dir(&probe_data);
    assert!(
        discovered.map.is_none() && discovered.packs.is_none(),
        "probe 根不应被发现出素材：{discovered:?}"
    );
    let old_style = load_monster_frames(&probe_data);
    assert!(
        old_style.is_empty(),
        "旧 discover 入口在 probe 根上应读空，却读到 {} 个",
        old_style.len()
    );

    let _ = std::fs::remove_dir_all(&probe_root);
}
