//! 《传奇-Next》客户端入口。
//!
//! 本地设置（语言）与 M6 地图窗口：`--map` / `--cell` 选择地图与相机中心，
//! 窗口内用方向键 / WASD 平移相机、`S` 截图、`F1` 调试叠层、`Q` 退出。
//! 渲染全部委托给 `mir-render`；本 crate 只负责窗口、输入与呈现。

mod app;
mod overlay;
mod settings;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use mir_core::i18n::LanguageLibrary;
use mir_core::{Language, Point, TextKey};
use mir_render::MapScene;

use crate::settings::ClientSettings;

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
}

impl Default for Options {
    fn default() -> Self {
        Self {
            map: "0".to_owned(),
            map_dir: PathBuf::from("resource/map"),
            data_dir: PathBuf::from("resource/data"),
            center: Point::new(330, 270),
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
    println!("  --map <名称|路径>       地图名（默认 0，即 resource/map/0.map）");
    println!("  --cell <x,y>            相机初始中心格（默认 330,270）");
    println!("  --map-dir <目录>        地图目录（默认 resource/map）");
    println!("  --data-dir <目录>       素材目录（默认 resource/data）");
    println!("  --lang <代码>           本次启动使用该语言（并保存）");
    println!("  --set-language <代码>   只修改并保存语言设置");
    println!("  --list-languages        列出支持的语言");
    println!("  --help                  显示本帮助");
    println!();
    println!("窗口内按键：");
    println!("  方向键 / W A D   平移相机（S 被截图占用，向下请用 ↓）");
    println!("  S                截图到 build/screenshot-<n>.png");
    println!("  F1               显示/隐藏中心格调试叠层");
    println!("  Q / Esc          退出");
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
}
