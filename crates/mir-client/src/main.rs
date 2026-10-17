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
//! `Esc` 关聊天/NPC 对话、`F1` 叠层、`F2` 截图、`Q` 退出。
//! 渲染全部委托给 `mir-render`；本 crate 只负责窗口、输入与网络会话。

mod app;
mod overlay;
mod settings;

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use mir_client::net::{ConnectOptions, CreateCharacter, NetClient};
use mir_core::enums::{MirClass, MirGender};
use mir_core::i18n::LanguageLibrary;
use mir_core::{Language, Point, TextKey};
use mir_render::{EntityLibraries, MapScene, TileLibraries, Viewport};
use mir_world::{Map, MapCell, MapVersion};

use crate::settings::ClientSettings;

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
}

impl Default for Options {
    fn default() -> Self {
        Self {
            map: "0".to_owned(),
            map_dir: PathBuf::from("resource/map"),
            data_dir: PathBuf::from("resource/data"),
            center: Point::new(330, 270),
            connect: None,
            account: String::new(),
            password: String::new(),
            character: String::new(),
            create: None,
            auto_register: true,
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
            }
            "--data-dir" => {
                let Some(value) = args.next() else {
                    anyhow::bail!("--data-dir 需要一个目录");
                };
                options.data_dir = PathBuf::from(value);
            }
            "--connect" => {
                let Some(value) = args.next() else {
                    anyhow::bail!("--connect 需要服务器地址，例如 --connect 127.0.0.1:7000");
                };
                options.connect = Some(value);
            }
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

    if let Some(server) = options.connect.clone() {
        return run_connected(&options, &server);
    }

    let map_path = resolve_map_path(&options.map, &options.map_dir);
    let scene = MapScene::load(&map_path, &options.data_dir).with_context(|| {
        format!(
            "加载地图 {}（素材目录 {}）失败：请确认 resource/ 存在，或使用 --map-dir / --data-dir",
            map_path.display(),
            options.data_dir.display()
        )
    })?;
    println!(
        "地图 {}：{}×{} · 图库 {} 个 · 中心格 ({}, {})",
        scene.map.file_name,
        scene.map.width,
        scene.map.height,
        scene.libraries().len(),
        options.center.x,
        options.center.y
    );
    println!("方向键/WASD 平移 · S 截图 · F1 调试叠层 · Q 退出");
    app::run(scene, options.center);
    Ok(())
}

/// 联机模式：连接 → 登录/建角 → 进世界 → 按服务器地图加载本地地形 → 开窗口。
fn run_connected(options: &Options, server: &str) -> Result<()> {
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
    };
    println!("连接服务器 {server} …");
    let client = NetClient::connect(connect)?;
    let snapshot = client
        .wait_until_in_game(CONNECT_LOGIN_TIMEOUT)
        .with_context(|| format!("连接 {server} 并登录失败"))?;
    let me = snapshot.self_player.as_ref();
    let map_name = snapshot
        .map
        .as_ref()
        .map(|map| map.file_name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| options.map.clone());
    let map_path = resolve_map_path(&map_name, &options.map_dir);
    let scene = load_scene_with_fallback(&map_path, &options.data_dir);
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
        "方向键/WASD 走（Shift 跑，反向先转身）· Space 攻击 · P 捡取 · T 聊天 · Esc 关对话 · F1 叠层 · F2 截图 · Q 退出"
    );
    app::run_live(scene, center, client, snapshot);
    Ok(())
}

/// 联机模式的地图加载：失败时退化成空白地图，但仍然尝试加载实体图库。
fn load_scene_with_fallback(map_path: &Path, data_dir: &Path) -> MapScene {
    match MapScene::load(map_path, data_dir) {
        Ok(scene) => scene,
        Err(error) => {
            eprintln!(
                "警告：加载地图 {} 失败：{error}；使用空白地图继续（实体仍会尝试绘制）",
                map_path.display()
            );
            let libraries = TileLibraries::open_dir(data_dir)
                .unwrap_or_else(|_| TileLibraries::empty(data_dir));
            let entities = EntityLibraries::open_dir(data_dir);
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

fn print_help() {
    println!("传奇-Next 客户端");
    println!();
    println!("用法：mir-client [选项]");
    println!();
    println!("离线地图：");
    println!("  --map <名称|路径>       地图名（默认 0，即 resource/map/0.map）");
    println!("  --cell <x,y>            相机初始中心格（默认 330,270）");
    println!("  --map-dir <目录>        地图目录（默认 resource/map）");
    println!("  --data-dir <目录>       素材目录（默认 resource/data）");
    println!();
    println!("联机世界：");
    println!("  --connect <地址>        连接服务器，例如 --connect 127.0.0.1:7000");
    println!("  --account <账号>        账号（不存在时默认自动注册）");
    println!("  --password <口令>       口令");
    println!("  --character <角色名>    要进入世界的角色");
    println!("  --create-character <名> 角色不存在时自动创建");
    println!("  --class <职业>          0=战士 1=法师 2=道士 3=刺客 4=弓箭手（默认 0）");
    println!("  --gender <性别>         male/female 或 0/1（默认 male）");
    println!("  --no-register           账号不存在时不自动注册，直接报错");
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
    println!("        T 聊天（回车发送、Esc 取消）；Esc 关 NPC 对话；F1 叠层；F2 截图；Q 退出");
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
