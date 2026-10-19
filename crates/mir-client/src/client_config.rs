//! `config/client.toml`：统一设置（语言/声音/画面/键位）。
//!
//! 为什么不用 `toml` crate：工作区没有该依赖，而这里只需要一个**很小的固定
//! 子集**（`key = value`、`[keys]` 段、字符串/布尔/浮点）。自写解析器让
//! 「坏文件回落默认」的行为可控且零新增依赖。
//!
//! # 容错
//! - 文件不存在：返回默认值 + `Missing`（调用方负责生成）；
//! - 任一无法识别的行/类型错误：返回默认值 + `Invalid`（**不 panic**）；
//! - 缺字段：逐字段回落到默认值（老配置兼容）。

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// 窗口模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMode {
    /// 普通窗口。
    Windowed,
    /// 全屏。
    Fullscreen,
    /// 无边框窗口。
    Borderless,
}

impl WindowMode {
    /// 配置里的字符串。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Windowed => "windowed",
            Self::Fullscreen => "fullscreen",
            Self::Borderless => "borderless",
        }
    }

    /// 下一个模式（UI 循环切换）。
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Windowed => Self::Fullscreen,
            Self::Fullscreen => Self::Borderless,
            Self::Borderless => Self::Windowed,
        }
    }

    /// 从配置字符串解析。
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code.trim().to_ascii_lowercase().as_str() {
            "windowed" | "window" => Some(Self::Windowed),
            "fullscreen" => Some(Self::Fullscreen),
            "borderless" => Some(Self::Borderless),
            _ => None,
        }
    }
}

/// 可重绑的 6 个动作。
pub const KEY_ACTIONS: [&str; 6] = [
    "attack",
    "magic",
    "inventory",
    "group",
    "friends",
    "settings",
];

/// 动作 → 默认按键。
#[must_use]
pub fn default_key(action: &str) -> &'static str {
    match action {
        "attack" => "Space",
        "magic" => "F1",
        "inventory" => "I",
        "group" => "P",
        "friends" => "F",
        "settings" => "O",
        _ => "",
    }
}

/// 键位表（动作 → 按键名，如 `Space`/`F1`/`I`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBindings {
    map: BTreeMap<String, String>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        let mut map = BTreeMap::new();
        for action in KEY_ACTIONS {
            map.insert(action.to_owned(), default_key(action).to_owned());
        }
        Self { map }
    }
}

impl KeyBindings {
    /// 取一个动作的按键。
    #[must_use]
    pub fn get(&self, action: &str) -> &str {
        self.map.get(action).map_or("", String::as_str)
    }

    /// 设置按键；与其它动作冲突时拒绝并返回冲突动作名。
    pub fn set(&mut self, action: &str, key: &str) -> Result<(), String> {
        let key = key.trim();
        if key.is_empty() || key.len() > 32 {
            return Err("empty".to_owned());
        }
        if let Some((other, _)) = self
            .map
            .iter()
            .find(|(name, bound)| name.as_str() != action && bound.eq_ignore_ascii_case(key))
        {
            return Err(other.clone());
        }
        self.map.insert(action.to_owned(), key.to_owned());
        Ok(())
    }
}

/// 统一客户端配置。
#[derive(Debug, Clone, PartialEq)]
pub struct ClientConfig {
    /// 界面语言（`zh-cn` / `en`）。
    pub language: String,
    /// 主音量 0.0..=1.0。
    pub master_volume: f32,
    /// 音效音量 0.0..=1.0。
    pub sound_volume: f32,
    /// 音乐音量 0.0..=1.0。
    pub music_volume: f32,
    /// 窗口模式。
    pub window_mode: WindowMode,
    /// 渲染缩放（1.0 / 1.5 / 2.0）。
    pub scale: f32,
    /// 小地图是否显示传送点。
    pub show_minimap_gates: bool,
    /// 键位。
    pub keys: KeyBindings,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            language: "zh-cn".to_owned(),
            master_volume: 0.5,
            sound_volume: 0.5,
            music_volume: 0.5,
            window_mode: WindowMode::Windowed,
            scale: 1.0,
            show_minimap_gates: true,
            keys: KeyBindings::default(),
        }
    }
}

/// 读盘结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigIssue {
    /// 文件不存在（调用方应生成默认文件）。
    Missing,
    /// 解析失败（已回落默认值）。
    Invalid,
}

impl ClientConfig {
    /// 读配置；坏文件回落默认并返回 [`ConfigIssue::Invalid`]（不 panic）。
    #[must_use]
    pub fn load(path: impl AsRef<Path>) -> (Self, Option<ConfigIssue>) {
        let path = path.as_ref();
        let Ok(text) = std::fs::read_to_string(path) else {
            return (Self::default(), Some(ConfigIssue::Missing));
        };
        match Self::parse(&text) {
            Ok(config) => (config, None),
            Err(error) => {
                eprintln!("配置文件 {} 损坏（{error}），已回落默认值", path.display());
                (Self::default(), Some(ConfigIssue::Invalid))
            }
        }
    }

    /// 解析 TOML 子集。
    ///
    /// # Errors
    /// 行格式错误、类型错误、未知窗口模式或缩放不在 1/1.5/2 时返回错误。
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut config = Self::default();
        let mut section = String::new();
        for (line_number, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                section = name.trim().to_ascii_lowercase();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("第 {} 行缺少 '='", line_number + 1));
            };
            let key = key.trim();
            let value = value.trim();
            if section == "keys" {
                config
                    .keys
                    .set(key, &parse_string(value)?)
                    .map_err(|conflict| format!("键位冲突：{key} 与 {conflict}"))?;
                continue;
            }
            match key {
                "language" => config.language = normalise_language(&parse_string(value)?),
                "master_volume" => config.master_volume = clamp_volume(parse_number(value)?, 0.5),
                "sound_volume" => config.sound_volume = clamp_volume(parse_number(value)?, 0.5),
                "music_volume" => config.music_volume = clamp_volume(parse_number(value)?, 0.5),
                "window_mode" => {
                    config.window_mode = WindowMode::from_code(&parse_string(value)?)
                        .ok_or_else(|| format!("未知窗口模式：{value}"))?;
                }
                "scale" => {
                    let scale = parse_number(value)?;
                    if !matches!(scale, 1.0 | 1.5 | 2.0) {
                        return Err(format!("缩放只允许 1/1.5/2：{scale}"));
                    }
                    config.scale = scale;
                }
                "show_minimap_gates" => {
                    config.show_minimap_gates = match value {
                        "true" => true,
                        "false" => false,
                        _ => return Err(format!("布尔值错误：{value}")),
                    };
                }
                other => return Err(format!("未知键：{other}")),
            }
        }
        Ok(config)
    }

    /// 序列化为 TOML。
    #[must_use]
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# 《传奇-Next》客户端设置（自动生成）");
        let _ = writeln!(out, "language = \"{}\"", self.language);
        let _ = writeln!(out, "master_volume = {:.3}", self.master_volume);
        let _ = writeln!(out, "sound_volume = {:.3}", self.sound_volume);
        let _ = writeln!(out, "music_volume = {:.3}", self.music_volume);
        let _ = writeln!(out, "window_mode = \"{}\"", self.window_mode.code());
        let _ = writeln!(out, "scale = {:.1}", self.scale);
        let _ = writeln!(out, "show_minimap_gates = {}", self.show_minimap_gates);
        let _ = writeln!(out, "\n[keys]");
        for action in KEY_ACTIONS {
            let _ = writeln!(out, "{action} = \"{}\"", self.keys.get(action));
        }
        out
    }

    /// 写盘（自动建父目录）。
    ///
    /// # Errors
    /// 建目录或写文件失败时返回 IO 错误。
    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_toml())
    }

    /// 切换语言并返回是否变化。
    pub fn set_language(&mut self, language: &str) -> bool {
        let normalized = normalise_language(language);
        if self.language == normalized {
            return false;
        }
        self.language = normalized;
        true
    }
}

/// 默认配置路径：工作区 `config/client.toml`。
#[must_use]
pub fn default_path() -> PathBuf {
    PathBuf::from("config/client.toml")
}

fn normalise_language(code: &str) -> String {
    let lower = code.trim().to_ascii_lowercase();
    if matches!(lower.as_str(), "en" | "en-us" | "en-gb") {
        "en".to_owned()
    } else {
        "zh-cn".to_owned()
    }
}

fn clamp_volume(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

fn parse_string(value: &str) -> Result<String, String> {
    let Some(inner) = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        return Err(format!("需要字符串：{value}"));
    };
    Ok(inner.to_owned())
}

fn parse_number(value: &str) -> Result<f32, String> {
    value
        .parse::<f32>()
        .map_err(|_| format!("需要数字：{value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_complete() {
        let config = ClientConfig::default();
        assert_eq!(config.language, "zh-cn");
        assert_eq!(config.master_volume, 0.5);
        assert_eq!(config.window_mode, WindowMode::Windowed);
        assert_eq!(config.scale, 1.0);
        assert!(config.show_minimap_gates);
        for action in KEY_ACTIONS {
            assert_eq!(config.keys.get(action), default_key(action));
        }
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn roundtrips_through_toml_and_disk() {
        let mut config = ClientConfig::default();
        config.language = "en".to_owned();
        config.master_volume = 0.8;
        config.sound_volume = 0.2;
        config.window_mode = WindowMode::Borderless;
        config.scale = 1.5;
        config.show_minimap_gates = false;
        config.keys.set("attack", "A").expect("重绑");
        let text = config.to_toml();
        assert_eq!(ClientConfig::parse(&text).expect("解析"), config);

        let directory = std::env::temp_dir().join(format!(
            "mir-next-client-config-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.subsec_nanos())
        ));
        let path = directory.join("client.toml");
        config.save(&path).expect("写盘");
        let (loaded, issue) = ClientConfig::load(&path);
        assert_eq!(issue, None);
        assert_eq!(loaded, config);
        std::fs::remove_dir_all(directory).ok();
    }

    #[test]
    fn missing_and_bad_files_fall_back_without_panicking() {
        let directory =
            std::env::temp_dir().join(format!("mir-next-client-config-bad-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("目录");
        let missing = directory.join("nope.toml");
        let (config, issue) = ClientConfig::load(&missing);
        assert_eq!(config, ClientConfig::default());
        assert_eq!(issue, Some(ConfigIssue::Missing));

        let broken = directory.join("broken.toml");
        std::fs::write(&broken, "this is not toml\n").expect("写坏文件");
        let (config, issue) = ClientConfig::load(&broken);
        assert_eq!(config, ClientConfig::default());
        assert_eq!(issue, Some(ConfigIssue::Invalid));
        std::fs::remove_dir_all(directory).ok();
    }

    #[test]
    fn key_conflicts_are_rejected() {
        let mut keys = KeyBindings::default();
        assert_eq!(keys.set("attack", "I"), Err("inventory".to_owned()));
        assert_eq!(keys.get("attack"), "Space");
        assert!(keys.set("attack", "A").is_ok());
        assert_eq!(keys.get("attack"), "A");
    }

    #[test]
    fn language_switch_normalises_codes() {
        let mut config = ClientConfig::default();
        assert!(config.set_language("EN"));
        assert_eq!(config.language, "en");
        assert!(!config.set_language("en"));
        assert!(ClientConfig::parse("language = \"klingon\"").is_ok());
        assert_eq!(
            ClientConfig::parse("language = \"klingon\"")
                .unwrap()
                .language,
            "zh-cn"
        );
    }
}
