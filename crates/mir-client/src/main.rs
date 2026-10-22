//! 《传奇-Next》客户端入口。
//!
//! 两种运行模式：
//!
//! * 离线地图：`--map` / `--cell` 选择地图与相机中心；
//! * 联机世界：`--connect 127.0.0.1:7000 --account NAME --password PASS --character NAME`，
//!   角色不存在时加 `--create-character NAME --class N --gender M` 自动建角。
//!
//! 窗口内按键：离线用方向键/WASD 平移相机、`S` 截图、`F1` 叠层、`Q` 退出；
//! 联机用方向键/WASD 转身/走/跑（Shift 跑）、`Space` 攻击、`P` 捡取、`T` 聊天、
//! `I` 背包、`E` 装备、`M` 小地图、`B` 大地图、`Q` 任务、`N` 邮件、`G` 行会、
//! `F3` 技能、`1..9` 施法、鼠标点 NPC/面板按钮、`Esc` 关面板、
//! `F1` 叠层、`F2` 截图、`F10` 退出。
//! 渲染全部委托给 `mir-render`，界面在 `mir-client::ui`；本 crate 只负责窗口、输入与网络会话。

mod app;
mod overlay;

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use mir_client::audio::{SoundLibrary, SoundPlayer};
use mir_client::net::{ConnectOptions, CreateCharacter, NetClient};
use mir_core::enums::{MirClass, MirGender};
use mir_core::i18n::LanguageLibrary;
use mir_core::{Language, Point, TextKey};
use mir_render::{EntityLibraries, MapScene, TileLibraries, Viewport};
use mir_world::{Map, MapCell, MapVersion};

use mir_client::settings::ClientSettings;
/// 联机登录等待上限。
const CONNECT_LOGIN_TIMEOUT: Duration = Duration::from_secs(15);

/// 命令行选项。
struct Options {
    /// 地图名（不带 `.map`）或 `.map` 文件路径。
    map: String,
    /// 地图目录。
    map_dir: PathBuf,
    /// 素材目录。
    data_dir: PathBuf,
    /// 相机初始中心格。
    center: Point,
    /// `--connect` 的服务器地址。
    connect: Option<String>,
    /// `--server`：保存到设置文件的默认服务器地址（不直接连接）。
    server: Option<String>,
    /// `--login`：打开登录窗口（服务器选择 → 角色选择 → 进入世界）。
    login: bool,
    /// 账号。
    account: String,
    /// 口令。
    password: String,
    /// 角色名。
    character: String,
    /// 角色不存在时自动创建。
    create: Option<CreateCharacter>,
    /// 账号不存在时自动注册。
    auto_register: bool,
    /// `--no-sound`：本次不播音效。
    no_sound: bool,
    /// `--volume`：覆盖音效音量（0.0..=1.0）。
    volume: Option<f32>,
    /// `--no-music`：本次不播背景音乐。
    no_music: bool,
    /// `--music-volume`：覆盖背景音乐音量（0.0..=1.0，独立于音效）。
    music_volume: Option<f32>,
    /// `--sound-dir`：额外的音效目录（优先于默认目录）。
    sound_dirs: Vec<PathBuf>,
    /// `--asset-dir`：资源根（`game-assets/` 的替代路径）。
    asset_dir: Option<PathBuf>,
    /// `--asset-mode`：素材形态（`auto`/`dev`/`release`）。
    asset_mode: Option<String>,
    /// 用户是否显式给了 `--map-dir`（给了就不再被 `--asset-dir` 覆盖）。
    map_dir_explicit: bool,
    /// 用户是否显式给了 `--data-dir`。
    data_dir_explicit: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            map: "0".to_owned(),
            map_dir: PathBuf::from("game-assets/maps"),
            data_dir: PathBuf::from("game-assets/data"),
            center: Point::new(330, 270),
            connect: None,
            server: None,
            login: false,
            account: String::new(),
            password: String::new(),
            character: String::new(),
            create: None,
            auto_register: true,
            no_sound: false,
            volume: None,
            no_music: false,
            music_volume: None,
            sound_dirs: Vec::new(),
            asset_dir: None,
            asset_mode: None,
            map_dir_explicit: false,
            data_dir_explicit: false,
        }
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let settings_path = ClientSettings::default_path();
    let mut client_settings = ClientSettings::load(&settings_path);
    let mut options = Options::default();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--lang" => {
                let Some(code) = args.next() else {
                    anyhow::bail!("--lang 需要一个语言代码，例如 --lang en");
                };
                if !client_settings.set_language(&code) {
                    anyhow::bail!("不支持的语言代码：{code}（可用：zh-cn、en）");
                }
                client_settings.save(&settings_path)?;
            }
            "--set-language" => {
                let Some(code) = args.next() else {
                    anyhow::bail!("--set-language 需要一个语言代码，例如 --set-language en");
                };
                if !client_settings.set_language(&code) {
                    anyhow::bail!("不支持的语言代码：{code}（可用：zh-cn、en）");
                }
                client_settings.save(&settings_path)?;
                println!("已保存语言设置：{}", client_settings.language);
                return Ok(());
            }
            "--list-languages" => {
                for language in Language::ALL {
                    println!("{}\t{}", language.code(), language.native_name());
                }
                return Ok(());
            }
            "--map" => {
                let Some(name) = args.next() else {
                    anyhow::bail!("--map 需要地图名或 .map 路径，例如 --map 0");
                };
                options.map = name;
            }
            "--cell" => {
                let Some(value) = args.next() else {
                    anyhow::bail!("--cell 需要 x,y，例如 --cell 330,270");
                };
                options.center = parse_cell(&value)?;
            }
            "--map-dir" => {
                let Some(value) = args.next() else {
                    anyhow::bail!("--map-dir 需要一个目录");
                };
                options.map_dir = PathBuf::from(value);
                options.map_dir_explicit = true;
            }
            "--asset-dir" => {
                let Some(value) = args.next() else {
                    anyhow::bail!("--asset-dir 需要一个目录（例如 game-assets 或 D:/mir-assets）");
                };
                options.asset_dir = Some(PathBuf::from(value));
            }
            "--asset-mode" => {
                let Some(value) = args.next() else {
                    anyhow::bail!("--asset-mode 需要 auto|dev|release");
                };
                if mir_client::assets::AssetMode::parse(&value).is_none() {
                    anyhow::bail!("--asset-mode 只支持 auto|dev|release|legacy（收到 {value}）");
                }
                options.asset_mode = Some(value);
            }
            "--data-dir" => {
                let Some(value) = args.next() else {
                    anyhow::bail!("--data-dir 需要一个目录");
                };
                options.data_dir = PathBuf::from(value);
                options.data_dir_explicit = true;
            }
            "--connect" => {
                let Some(value) = args.next() else {
                    anyhow::bail!("--connect 需要服务器地址，例如 --connect 127.0.0.1:7000");
                };
                options.connect = Some(value);
            }
            "--server" => {
                let Some(value) = args.next() else {
                    anyhow::bail!("--server 需要服务器地址，例如 --server 127.0.0.1:7000");
                };
                options.server = Some(value);
            }
            "--login" => options.login = true,
            "--account" => {
                let Some(value) = args.next() else {
                    anyhow::bail!("--account 需要一个账号名");
                };
                options.account = value;
            }
            "--password" => {
                let Some(value) = args.next() else {
                    anyhow::bail!("--password 需要一个口令");
                };
                options.password = value;
            }
            "--character" => {
                let Some(value) = args.next() else {
                    anyhow::bail!("--character 需要一个角色名");
                };
                options.character = value;
            }
            "--create-character" => {
                let Some(value) = args.next() else {
                    anyhow::bail!("--create-character 需要一个角色名");
                };
                if options.character.is_empty() {
                    options.character.clone_from(&value);
                }
                let class = options
                    .create
                    .as_ref()
                    .map_or(MirClass::Warrior, |c| c.class);
                let gender = options
                    .create
                    .as_ref()
                    .map_or(MirGender::Male, |c| c.gender);
                options.create = Some(CreateCharacter {
                    name: value,
                    class,
                    gender,
                });
            }
            "--class" => {
                let Some(value) = args.next() else {
                    anyhow::bail!(
                        "--class 需要职业名或编号（0=战士 1=法师 2=道士 3=刺客 4=弓箭手）"
                    );
                };
                let class = parse_class(&value)?;
                if let Some(create) = &mut options.create {
                    create.class = class;
                } else {
                    options.create = Some(CreateCharacter {
                        name: options.character.clone(),
                        class,
                        gender: MirGender::Male,
                    });
                }
            }
            "--gender" => {
                let Some(value) = args.next() else {
                    anyhow::bail!("--gender 需要 male/female 或 0/1");
                };
                let gender = parse_gender(&value)?;
                if let Some(create) = &mut options.create {
                    create.gender = gender;
                } else {
                    options.create = Some(CreateCharacter {
                        name: options.character.clone(),
                        class: MirClass::Warrior,
                        gender,
                    });
                }
            }
            "--no-register" => options.auto_register = false,
            "--no-sound" => options.no_sound = true,
            "--volume" => {
                let Some(value) = args.next() else {
                    anyhow::bail!("--volume 需要 0.0..=1.0 之间的数值，例如 --volume 0.5");
                };
                let volume = value
                    .trim()
                    .parse::<f32>()
                    .with_context(|| format!("--volume 不是数字：{value}"))?;
                if !(0.0..=1.0).contains(&volume) {
                    anyhow::bail!("--volume 必须在 0.0..=1.0 之间，收到 {volume}");
                }
                options.volume = Some(volume);
            }
            "--no-music" => options.no_music = true,
            "--music-volume" => {
                let Some(value) = args.next() else {
                    anyhow::bail!(
                        "--music-volume 需要 0.0..=1.0 之间的数值，例如 --music-volume 0.3"
                    );
                };
                let volume = value
                    .trim()
                    .parse::<f32>()
                    .with_context(|| format!("--music-volume 不是数字：{value}"))?;
                if !(0.0..=1.0).contains(&volume) {
                    anyhow::bail!("--music-volume 必须在 0.0..=1.0 之间，收到 {volume}");
                }
                options.music_volume = Some(volume);
            }
            "--sound-dir" => {
                let Some(value) = args.next() else {
                    anyhow::bail!("--sound-dir 需要一个目录（例如 resource/wav）");
                };
                options.sound_dirs.push(PathBuf::from(value));
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            other => anyhow::bail!("未知参数：{other}（--help 查看用法）"),
        }
    }

    let library = LanguageLibrary::embedded();
    let language = client_settings.language();
    tracing::info!(?language, path = %settings_path.display(), "客户端启动");
    println!("{}", library.text(language, TextKey::Welcome));
    println!(
        "当前语言：{}（{}）· 设置文件：{}",
        language.native_name(),
        language.code(),
        settings_path.display()
    );
    println!(
        "显示：{} · 缩放 {}x · 服务器 {}",
        client_settings.resolution_text(),
        client_settings.scale(),
        client_settings.server()
    );

    // `--server` 只更新设置文件里的默认地址（登录窗口/下次启动用）。
    if let Some(server) = options.server.clone() {
        client_settings.set_server(&server);
        client_settings.save(&settings_path)?;
        println!("已保存服务器地址：{}", client_settings.server());
    }

    // 资源根解析（`--asset-dir` > `MIR2_ASSET_DIR` > client.toml 的 asset_root >
    // 默认 `game-assets` > 回落 `resource`）；`--map-dir`/`--data-dir` 显式给了就优先。
    let assets = mir_client::assets::AssetRoot::resolve(
        options.asset_dir.as_deref(),
        std::env::var(mir_client::assets::ASSET_ROOT_ENV)
            .ok()
            .as_deref(),
        client_settings.asset_root.as_deref(),
    );
    assets.log_resolution();
    // 素材形态：`--asset-mode` > `MIR2_ASSET_MODE` > `client.toml` 的 `asset_mode` > `auto`
    let asset_mode = options
        .asset_mode
        .as_deref()
        .and_then(mir_client::assets::AssetMode::parse)
        .or_else(|| {
            std::env::var(mir_client::assets::ASSET_MODE_ENV)
                .ok()
                .as_deref()
                .and_then(mir_client::assets::AssetMode::parse)
        })
        .or_else(|| {
            client_settings
                .asset_mode
                .as_deref()
                .and_then(mir_client::assets::AssetMode::parse)
        })
        .unwrap_or(mir_client::assets::AssetMode::Auto);
    let resolved_assets = mir_client::assets::ResolvedAssets::load(assets.root(), asset_mode);
    resolved_assets.log(assets.root());
    if asset_mode.allow_legacy() {
        eprintln!(
            "警告：素材形态 legacy —— 允许回退老格式；按项目规则运行时只应读自有格式，此模式仅供开发兼容"
        );
    }
    if !options.map_dir_explicit {
        options.map_dir = assets.map_dir();
    }
    if !options.data_dir_explicit {
        options.data_dir = assets.data_dir();
    }
    println!(
        "资源根：{}（来源 {}）· 地图 {} · 素材 {}",
        assets.root().display(),
        assets.source().as_str(),
        options.map_dir.display(),
        options.data_dir.display()
    );

    // 素材目录自动探测：`--data-dir` 指向的目录不存在时依次尝试 `resource/data`、
    // `resource/ld/data`（不同发行包的客户端目录结构不同）。
    let detected = detect_data_dir(&options.data_dir);
    if detected != options.data_dir {
        println!(
            "素材目录：{} 不存在，改用 {}",
            options.data_dir.display(),
            detected.display()
        );
        options.data_dir = detected;
    }
    println!("素材目录：{}", options.data_dir.display());

    if options.login {
        run_login_window(&options, &client_settings, &settings_path);
        return Ok(());
    }

    if let Some(server) = options.connect.clone() {
        return run_connected(
            &options,
            &server,
            language,
            &client_settings,
            &settings_path,
            &LiveAssets {
                root: assets.root(),
                resolved: &resolved_assets,
                allow_legacy: asset_mode.allow_legacy(),
            },
        );
    }

    let map_path = resolve_map_path(&options.map, &options.map_dir);
    let gfx_dir = mir_client::assets::gfx_dir_for_data_dir(&options.data_dir);
    let scene = MapScene::load_with_sources(
        &map_path,
        &options.data_dir,
        Some(gfx_dir.as_path()),
        Some(assets.root()),
        resolved_assets.map.as_ref(),
        resolved_assets.packs.as_ref(),
        asset_mode.allow_legacy(),
    )
    .with_context(|| {
        format!(
            "加载地图 {}（素材根 {}）失败：请确认 game-assets/maps 存在，或使用 --map-dir / --data-dir",
            map_path.display(),
            assets.root().display()
        )
    })?;
    println!(
        "地图 {}：{}×{} · 图库 {} 个（+WIL {} 个）· 中心格 ({}, {})",
        scene.map.file_name,
        scene.map.width,
        scene.map.height,
        scene.libraries().len(),
        scene.libraries().wil_len(),
        options.center.x,
        options.center.y
    );
    println!("方向键/WASD 平移 · S 截图 · F1 调试叠层 · Q 退出");
    app::run(scene, options.center, language);
    Ok(())
}

/// 登录窗口：服务器选择 → 角色选择 → 进入世界。
///
/// 地址取设置文件里的 `server`（默认 `127.0.0.1:7000`，可用 `--server` 修改）；
/// 命令行 `--account` / `--password` 只用于预填。
fn run_login_window(options: &Options, client_settings: &ClientSettings, settings_path: &Path) {
    println!(
        "登录窗口：服务器 {} · 语言 {}",
        client_settings.server(),
        client_settings.language().native_name()
    );
    let legacy_wav = std::env::var(mir_client::assets::ASSET_MODE_ENV)
        .ok()
        .as_deref()
        .and_then(mir_client::assets::AssetMode::parse)
        .is_some_and(mir_client::assets::AssetMode::allow_legacy);
    let sound = build_sound_player(options, client_settings, legacy_wav);
    app::run_login(app::LoginOptions {
        map_dir: options.map_dir.clone(),
        data_dir: options.data_dir.clone(),
        sound,
        settings: client_settings.clone(),
        settings_path: settings_path.to_path_buf(),
        language: client_settings.language(),
        account: options.account.clone(),
        password: options.password.clone(),
    });
}

/// 联机模式用的素材上下文（避免参数表过长）。
struct LiveAssets<'a> {
    /// 素材根。
    root: &'a Path,
    /// 解析出来的散装/小包来源。
    resolved: &'a mir_client::assets::ResolvedAssets,
    /// 是否允许老的 `.wzl` 回退（仅 `asset_mode = legacy`）。
    allow_legacy: bool,
}

/// 联机模式：连接 → 登录/建角 → 进世界 → 按服务器地图加载本地地形 → 开窗口。
fn run_connected(
    options: &Options,
    server: &str,
    language: Language,
    client_settings: &ClientSettings,
    settings_path: &Path,
    assets: &LiveAssets<'_>,
) -> Result<()> {
    if options.account.is_empty() || options.password.is_empty() || options.character.is_empty() {
        anyhow::bail!(
            "联机模式需要 --account NAME --password PASS --character NAME（角色不存在时加 --create-character）"
        );
    }
    let connect = ConnectOptions {
        server: server.to_owned(),
        account: options.account.clone(),
        password: options.password.clone(),
        character: options.character.clone(),
        create: options.create.clone(),
        auto_register: options.auto_register,
        manual_select: false,
    };
    println!("连接服务器 {server} …");
    let client = NetClient::connect(connect.clone())?;
    let snapshot = client
        .wait_until_in_game(CONNECT_LOGIN_TIMEOUT)
        .with_context(|| format!("连接 {server} 并登录失败"))?;
    let me = snapshot.self_player.as_ref();
    // 记录装备摘要（选角页显示；重启后仍在）。
    if let Some(player) = me {
        let summary = app::equipment_summary(&snapshot);
        if !summary.is_empty() && client_settings.equipment.get(&player.name) != Some(&summary) {
            let mut updated = client_settings.clone();
            updated.equipment.insert(player.name.clone(), summary);
            if let Err(error) = updated.save(settings_path) {
                eprintln!("保存设置失败（{}）：{error}", settings_path.display());
            }
        }
    }
    let map_name = snapshot
        .map
        .as_ref()
        .map(|map| map.file_name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| options.map.clone());
    let map_path = resolve_map_path(&map_name, &options.map_dir);
    let gfx_dir = mir_client::assets::gfx_dir_for_data_dir(&options.data_dir);
    let scene = load_scene_with_fallback(
        &map_path,
        &options.data_dir,
        Some(gfx_dir.as_path()),
        Some(assets.root),
        assets.resolved,
        assets.allow_legacy,
    );
    let center = me.map_or(options.center, |player| player.position);
    println!(
        "已进入世界：{} · 对象 {} 个 · 地图 {}（{}×{}）",
        me.map_or("?", |player| player.name.as_str()),
        snapshot.entities.len(),
        scene.map.file_name,
        scene.map.width,
        scene.map.height
    );
    println!(
        "方向键/WASD 走（Shift 跑，反向先转身）· Space 攻击 · P 捡取 · T 聊天 · I 背包 · M 小地图 · Q 任务 · N 邮件 · G 行会 · F3 技能 · 1..9 施法 · O 设置"
    );
    println!("鼠标点 NPC / 面板按钮 · Esc 关面板 · F1 叠层 · F2 截图 · F10 退出");
    let sound = build_sound_player(options, client_settings, assets.allow_legacy);
    if sound.is_active() {
        println!(
            "音效：开启（音量 {:.2}，素材目录 {}）",
            sound.volume(),
            sound
                .library()
                .roots()
                .iter()
                .map(|root| root.display().to_string())
                .collect::<Vec<_>>()
                .join(" / ")
        );
    } else if options.no_sound || !client_settings.sound {
        println!("音效：关闭（--no-sound 或设置文件里的 sound = false）");
    }
    if sound.is_music_active() {
        println!(
            "背景音乐：开启（音量 {:.2}；切图时按 MapInfo.Music 切换）",
            sound.music_volume()
        );
    } else if options.no_music || !client_settings.music {
        println!("背景音乐：关闭（--no-music 或设置文件里的 music = false）");
    }
    app::run_live(
        scene,
        center,
        client,
        snapshot,
        language,
        connect,
        app::LiveOptions {
            map_dir: options.map_dir.clone(),
            data_dir: options.data_dir.clone(),
            sound,
            settings: client_settings.clone(),
            settings_path: settings_path.to_path_buf(),
        },
    );
    Ok(())
}

/// 按 CLI + 设置文件构造播放器（默认开启、音量 0.5）。
///
/// 没有音频设备时 [`SoundPlayer::new`] 会打印一次警告并退化成空操作
/// （CI 上没有声卡也能照常跑窗口）。
fn build_sound_player(
    options: &Options,
    settings: &ClientSettings,
    allow_legacy_wav: bool,
) -> SoundPlayer {
    // 音量以统一配置 `config/client.toml` 为准（缺失/损坏时 ClientConfig 已回落默认）；
    // 命令行 `--volume`/`--music-volume` 只覆盖本次运行。
    let (config, _issue) =
        mir_client::client_config::ClientConfig::load(mir_client::client_config::default_path());
    let enabled = settings.sound && !options.no_sound;
    let volume = options.volume.unwrap_or(config.sound_volume);
    let music_enabled = settings.music && !options.no_music;
    let music_volume = options.music_volume.unwrap_or(config.music_volume);
    // 素材目录：`--sound-dir` 最优先，其次是按 `--data-dir` 推出来的
    // `resource/wav` 与 Crystal 完整音效/音乐集，最后是相对当前目录的默认值兜底。
    let mut library =
        SoundLibrary::around_data_dir(&options.data_dir).with_legacy_wav(allow_legacy_wav);
    library.push_back(&SoundLibrary::default_roots());
    for directory in options.sound_dirs.iter().rev() {
        library.push_front(directory.clone());
    }
    let mut player =
        SoundPlayer::new_with_music(library, enabled, volume, music_enabled, music_volume);
    player.set_master_volume(config.master_volume);
    player
}

/// 联机模式的地图加载：失败时退化成空白地图，但仍然尝试加载实体图库。
fn load_scene_with_fallback(
    map_path: &Path,
    data_dir: &Path,
    gfx_dir: Option<&Path>,
    asset_root: Option<&Path>,
    assets: &mir_client::assets::ResolvedAssets,
    allow_legacy: bool,
) -> MapScene {
    match MapScene::load_with_sources(
        map_path,
        data_dir,
        gfx_dir,
        asset_root,
        assets.map.as_ref(),
        assets.packs.as_ref(),
        allow_legacy,
    ) {
        Ok(scene) => {
            println!(
                "素材：{} · 地图库 {} 个（+WIL {} 个）· 地图 {}（{}×{}）",
                data_dir.display(),
                scene.libraries().len(),
                scene.libraries().wil_len(),
                map_path.display(),
                scene.map.width,
                scene.map.height,
            );
            scene
        }
        Err(error) => {
            eprintln!(
                "警告：加载地图 {} 失败：{error}；使用空白地图继续（实体仍会尝试绘制）",
                map_path.display()
            );
            let libraries = TileLibraries::open_with_sources(
                data_dir,
                gfx_dir,
                asset_root,
                assets.map.as_ref(),
                assets.packs.as_ref(),
                allow_legacy,
            )
            .unwrap_or_else(|_| TileLibraries::empty(data_dir));
            let entities = EntityLibraries::open_with_sources(
                data_dir,
                gfx_dir,
                asset_root,
                assets.map.as_ref(),
                assets.packs.as_ref(),
                allow_legacy,
            );
            MapScene::from_parts_with_entities(blank_map(), libraries, Viewport::DEFAULT, entities)
        }
    }
}

/// 无地图素材时的空白地图（可走、无图块）。
fn blank_map() -> Map {
    let width = 1024;
    let height = 1024;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "blank".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
}

/// 解析 `x,y` 形式的格子坐标。
fn parse_cell(value: &str) -> Result<Point> {
    let Some((x, y)) = value.split_once(',') else {
        anyhow::bail!("--cell 的格式是 x,y，例如 --cell 330,270");
    };
    let x = x
        .trim()
        .parse::<i32>()
        .with_context(|| format!("--cell 的 x 不是整数：{x}"))?;
    let y = y
        .trim()
        .parse::<i32>()
        .with_context(|| format!("--cell 的 y 不是整数：{y}"))?;
    Ok(Point::new(x, y))
}

/// 职业参数：名字或 0..=4。
fn parse_class(value: &str) -> Result<MirClass> {
    match value.to_ascii_lowercase().as_str() {
        "0" | "warrior" | "战士" => Ok(MirClass::Warrior),
        "1" | "wizard" | "法师" => Ok(MirClass::Wizard),
        "2" | "taoist" | "道士" => Ok(MirClass::Taoist),
        "3" | "assassin" | "刺客" => Ok(MirClass::Assassin),
        "4" | "archer" | "弓箭手" => Ok(MirClass::Archer),
        other => anyhow::bail!("不支持的职业：{other}（0=战士 1=法师 2=道士 3=刺客 4=弓箭手）"),
    }
}

/// 性别参数：`male`/`female` 或 `0`/`1`。
fn parse_gender(value: &str) -> Result<MirGender> {
    match value.to_ascii_lowercase().as_str() {
        "0" | "male" | "男" => Ok(MirGender::Male),
        "1" | "female" | "女" => Ok(MirGender::Female),
        other => anyhow::bail!("不支持的性别：{other}（male/female 或 0/1）"),
    }
}

/// 把 `--map` 的值解析成 `.map` 路径：带 `.map` 后缀或路径已存在时按路径处理，
/// 否则拼成 `<map_dir>/<name>.map`。
fn resolve_map_path(map: &str, map_dir: &Path) -> PathBuf {
    let as_path = Path::new(map);
    if map.to_ascii_lowercase().ends_with(".map") || as_path.is_file() {
        return as_path.to_path_buf();
    }
    map_dir.join(format!("{map}.map"))
}

/// 素材目录自动探测：配置目录存在就用它，否则依次尝试常见客户端数据目录。
///
/// 只在**目录存在**时替换；都不存在时保留原值，让后续加载错误信息里能看到用户
/// 传入的路径（而不是一个猜出来的目录）。
fn detect_data_dir(configured: &Path) -> PathBuf {
    if configured.is_dir() {
        return configured.to_path_buf();
    }
    ["game-assets/data"]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_dir())
        .unwrap_or_else(|| configured.to_path_buf())
}

fn print_help() {
    println!("传奇-Next 客户端");
    println!();
    println!("用法：mir-client [选项]");
    println!();
    println!("离线地图：");
    println!("  --map <名称|路径>       地图名（默认 0，即 resource/map/0.map）");
    println!("  --cell <x,y>            相机初始中心格（默认 330,270）");
    println!("  --map-dir <目录>        地图目录（默认 resource/map）");
    println!(
        "  --data-dir <目录>       素材目录（默认 resource/data；缺失时自动探测 resource/ld/data）"
    );
    println!();
    println!("联机世界：");
    println!("  --login                 打开登录窗口（服务器选择 → 角色选择 → 进入世界）");
    println!("  --server <地址>         保存默认服务器地址（默认 127.0.0.1:7000）");
    println!("  --connect <地址>        直接连接服务器，例如 --connect 127.0.0.1:7000");
    println!("  --account <账号>        账号（不存在时默认自动注册）");
    println!("  --password <口令>       口令");
    println!("  --character <角色名>    要进入世界的角色");
    println!("  --create-character <名> 角色不存在时自动创建");
    println!("  --class <职业>          0=战士 1=法师 2=道士 3=刺客 4=弓箭手（默认 0）");
    println!("  --gender <性别>         male/female 或 0/1（默认 male）");
    println!("  --no-register           账号不存在时不自动注册，直接报错");
    println!();
    println!("设置面板（游戏内按 O）：音量 / 语言 / 分辨率与缩放 / 按键说明；");
    println!("改动即时生效并写回设置文件，重启后仍生效。");
    println!("音效与音乐：");
    println!("  --no-sound              本次运行不播音效（不写设置文件）");
    println!("  --volume <0..1>         本次运行音效音量（默认 0.5，不写设置文件）");
    println!("  --no-music              本次运行不播背景音乐（不写设置文件）");
    println!("  --music-volume <0..1>   本次运行背景音乐音量（默认 0.5，独立于音效）");
    println!("  --sound-dir <目录>      额外音效/音乐目录（可多次；优先于 resource/wav）");
    println!("  --asset-dir <目录>      资源根（默认 game-assets/；也可用 MIR2_ASSET_DIR）");
    println!(
        "  --asset-mode <形态>     素材形态 auto|dev|release|legacy（默认 auto；legacy 仅供开发，允许回退老格式）"
    );
    println!();
    println!("其它：");
    println!("  --lang <代码>           本次启动使用该语言（并保存）");
    println!("  --set-language <代码>   只修改并保存语言设置");
    println!("  --list-languages        列出支持的语言");
    println!("  --help                  显示本帮助");
    println!();
    println!("窗口内按键：");
    println!("  离线：方向键 / W A D 平移（S 截图）；S 截图；F1 叠层；Q/Esc 退出");
    println!("  联机：方向键 / WASD 走（Shift 跑，反向先转身）；Space 攻击；P 捡取；");
    println!("        T 聊天（回车发送、Esc 取消）；I 背包；M 小地图；Q 任务；N 邮件；G 行会；");
    println!("        F3 技能；1..9 施法；仓库面板由 NPC 仓库页自动打开；");
    println!("        商店里 S 出售 / R 修理选中物品；背包里 D 丢弃选中物品；");
    println!("        鼠标点 NPC 对话、点按钮/货架/背包格；Esc 关面板；F1 叠层；F2 截图；F10 退出");
    println!();
    println!("默认语言：zh-cn（简体中文）。语言设置保存在本地，可随时切换。");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cell_with_spaces() {
        assert_eq!(parse_cell(" 12, -34 ").unwrap(), Point::new(12, -34));
    }

    #[test]
    fn rejects_bad_cell_values() {
        assert!(parse_cell("12").is_err());
        assert!(parse_cell("a,b").is_err());
        assert!(parse_cell("1,2,3").is_err());
    }

    #[test]
    fn resolves_map_names_and_paths() {
        assert_eq!(
            resolve_map_path("0", Path::new("resource/map")),
            PathBuf::from("resource/map/0.map")
        );
        assert_eq!(
            resolve_map_path("some/where/abc.MAP", Path::new("ignored")),
            PathBuf::from("some/where/abc.MAP")
        );
    }

    #[test]
    fn data_dir_detection_keeps_existing_and_falls_back() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let real = root.join("resource/data");
        if real.is_dir() {
            // 已存在的目录原样保留。
            assert_eq!(detect_data_dir(&real), real);
        }
        // 不存在且候选也不存在时保留原值（错误信息里能看到用户传入的路径）。
        let missing = root.join("target/definitely-missing-data-dir");
        assert_eq!(detect_data_dir(&missing), missing);
    }

    #[test]
    fn parses_class_and_gender_names() {
        assert_eq!(parse_class("wizard").unwrap(), MirClass::Wizard);
        assert_eq!(parse_class("3").unwrap(), MirClass::Assassin);
        assert_eq!(parse_class("弓箭手").unwrap(), MirClass::Archer);
        assert!(parse_class("knight").is_err());
        assert_eq!(parse_gender("female").unwrap(), MirGender::Female);
        assert_eq!(parse_gender("0").unwrap(), MirGender::Male);
        assert!(parse_gender("2").is_err());
    }
}
