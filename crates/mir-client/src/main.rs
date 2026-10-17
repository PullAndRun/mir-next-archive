//! 《传奇-Next》客户端入口（骨架）。
//!
//! 里程碑 M6 之前这里只保留工程骨架与**本地设置**：语言选择存在本地，
//! 默认简体中文，可用 `--lang` 或 `--set-language` 切换。
//! 渲染与界面（winit + wgpu）在后续里程碑接入。

mod settings;

use anyhow::Result;
use mir_core::i18n::LanguageLibrary;
use mir_core::{Language, TextKey};

use crate::settings::ClientSettings;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let path = ClientSettings::default_path();
    let mut client_settings = ClientSettings::load(&path);

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
                client_settings.save(&path)?;
            }
            "--set-language" => {
                let Some(code) = args.next() else {
                    anyhow::bail!("--set-language 需要一个语言代码，例如 --set-language en");
                };
                if !client_settings.set_language(&code) {
                    anyhow::bail!("不支持的语言代码：{code}（可用：zh-cn、en）");
                }
                client_settings.save(&path)?;
                println!("已保存语言设置：{}", client_settings.language);
            }
            "--list-languages" => {
                for language in Language::ALL {
                    println!("{}\t{}", language.code(), language.native_name());
                }
                return Ok(());
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
    tracing::info!(?language, path = %path.display(), "客户端骨架启动");
    println!("{}", library.text(language, TextKey::Welcome));
    println!(
        "当前语言：{}（{}）· 设置文件：{}",
        language.native_name(),
        language.code(),
        path.display()
    );
    println!("渲染、界面与输入将在里程碑 M6 实现；语言设置已可本地切换。");
    Ok(())
}

fn print_help() {
    println!("传奇-Next 客户端（骨架）");
    println!();
    println!("用法：mir-client [选项]");
    println!();
    println!("  --lang <代码>           本次启动使用该语言（并保存）");
    println!("  --set-language <代码>   只修改并保存语言设置");
    println!("  --list-languages        列出支持的语言");
    println!("  --help                  显示本帮助");
    println!();
    println!("默认语言：zh-cn（简体中文）。语言设置保存在本地，可随时切换。");
}
