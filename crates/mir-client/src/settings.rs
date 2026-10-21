//! 客户端本地设置。
//!
//! 语言选择属于本地设置：不依赖服务端也能切换界面语言，默认简体中文。
//! 音效开关/音量与背景音乐开关/音量也在这里（都默认开启、音量 0.5，两者独立）；
//! 命令行 `--no-sound` / `--volume` / `--no-music` / `--music-volume` 只覆盖本次运行，
//! 设置文件字段是 `sound` / `volume` / `music` / `music_volume`。
//!
//! 第 2 轮加入设置面板后，这里还保存**服务器地址**（默认 `127.0.0.1:7000`）、
//! **显示**（分辨率预设 `resolution` + 渲染缩放 `scale`）与**角色装备摘要缓存**
//! （`equipment`：角色名 → 上次进入世界时的装备摘要，供选角页显示）。
//! 每个设置动作由窗口层立即写回本文件（`ClientSettings::save`），重启后仍生效。
//!
//! 设置文件放在 `%APPDATA%\mir-next\settings.json`（其它平台放在用户目录下的
//! `.mir-next/settings.json`），读写失败时回退到默认值而不是中断启动。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::audio::{DEFAULT_MUSIC_VOLUME, DEFAULT_VOLUME};
use crate::ui::settings::{RESOLUTIONS, SCALES, SettingsState, resolution_text};
use mir_core::Language;
use serde::{Deserialize, Serialize};

/// 默认服务器地址（与服务端默认端口一致）。
pub const DEFAULT_SERVER: &str = "127.0.0.1:7000";
/// 默认分辨率（窗口像素）。
pub const DEFAULT_RESOLUTION: (u32, u32) = RESOLUTIONS[0];
/// 默认渲染缩放。
pub const DEFAULT_SCALE: u32 = 1;

/// 客户端设置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientSettings {
    /// 界面语言代码（`zh-cn` / `en`）。
    #[serde(default = "default_language")]
    pub language: String,
    /// 是否播放音效（默认开启；`--no-sound` 关掉）。
    #[serde(default = "default_sound")]
    pub sound: bool,
    /// 音效音量（0.0..=1.0，默认 0.5）。
    #[serde(default = "default_volume")]
    pub volume: f32,
    /// 是否播放背景音乐（默认开启；`--no-music` 关掉）。
    #[serde(default = "default_music")]
    pub music: bool,
    /// 背景音乐音量（0.0..=1.0，默认 0.5；与音效音量独立）。
    #[serde(default = "default_music_volume")]
    pub music_volume: f32,
    /// 服务器地址（`host:port`，默认 [`DEFAULT_SERVER`]）。
    #[serde(default = "default_server")]
    pub server: String,
    /// 窗口分辨率（`[宽, 高]`，默认 800×600）。
    #[serde(default = "default_resolution")]
    pub resolution: [u32; 2],
    /// 渲染缩放（1..=3，默认 1）。
    #[serde(default = "default_scale")]
    pub scale: u32,
    /// 资源根目录（`game-assets/` 或自定义路径）。
    ///
    /// `None` = 没配置 → 按 `--asset-dir` > `MIR2_ASSET_DIR` > 默认 `game-assets` >
    /// 回落 `resource/` 的顺序解析（见 [`crate::assets::AssetRoot`]）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_root: Option<String>,
    /// 素材形态（`auto`/`dev`/`release`；`None` = `auto`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_mode: Option<String>,
    /// 角色装备摘要缓存（角色名 → 「木剑 / 布衣」），选角页显示用。
    #[serde(default)]
    pub equipment: BTreeMap<String, String>,
}

fn default_language() -> String {
    // 项目要求默认全中文化。
    Language::ZhCn.code().to_owned()
}

/// 音效默认开启（需求：默认开启，音量 0.5）。
const fn default_sound() -> bool {
    true
}

/// 默认音量。
const fn default_volume() -> f32 {
    DEFAULT_VOLUME
}

/// 背景音乐默认开启（需求：有 BGM；`--no-music` 可关）。
const fn default_music() -> bool {
    true
}

/// 背景音乐默认音量（与音效独立，默认 0.5）。
const fn default_music_volume() -> f32 {
    DEFAULT_MUSIC_VOLUME
}

/// 默认服务器地址。
fn default_server() -> String {
    DEFAULT_SERVER.to_owned()
}

/// 默认窗口分辨率。
const fn default_resolution() -> [u32; 2] {
    [DEFAULT_RESOLUTION.0, DEFAULT_RESOLUTION.1]
}

/// 默认渲染缩放。
const fn default_scale() -> u32 {
    DEFAULT_SCALE
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            language: default_language(),
            sound: default_sound(),
            volume: default_volume(),
            music: default_music(),
            music_volume: default_music_volume(),
            server: default_server(),
            resolution: default_resolution(),
            scale: default_scale(),
            asset_root: None,
            asset_mode: None,
            equipment: BTreeMap::new(),
        }
    }
}

impl ClientSettings {
    /// 当前语言（无法识别时退回默认的简体中文）。
    #[must_use]
    pub fn language(&self) -> Language {
        Language::from_code(&self.language).unwrap_or_default()
    }

    /// 归一化音效音量到 `0.0..=1.0`。
    #[must_use]
    pub fn sound_volume(&self) -> f32 {
        if self.volume.is_finite() {
            self.volume.clamp(0.0, 1.0)
        } else {
            DEFAULT_VOLUME
        }
    }

    /// 归一化背景音乐音量到 `0.0..=1.0`。
    #[must_use]
    pub fn music_volume(&self) -> f32 {
        if self.music_volume.is_finite() {
            self.music_volume.clamp(0.0, 1.0)
        } else {
            DEFAULT_MUSIC_VOLUME
        }
    }

    /// 设置语言（只接受受支持的代码）。
    ///
    /// # Errors
    /// 语言代码不受支持时返回 `false`。
    pub fn set_language(&mut self, code: &str) -> bool {
        match Language::from_code(code) {
            Some(language) => {
                language.code().clone_into(&mut self.language);
                true
            }
            None => false,
        }
    }

    /// 服务器地址（空值退回默认地址）。
    #[must_use]
    pub fn server(&self) -> String {
        let server = self.server.trim();
        if server.is_empty() {
            DEFAULT_SERVER.to_owned()
        } else {
            server.to_owned()
        }
    }

    /// 设置服务器地址（空值写回默认地址）。
    pub fn set_server(&mut self, server: &str) {
        let server = server.trim();
        self.server = if server.is_empty() {
            DEFAULT_SERVER.to_owned()
        } else {
            server.to_owned()
        };
    }

    /// 窗口分辨率（夹到 320×240 以上）。
    #[must_use]
    pub fn resolution(&self) -> (u32, u32) {
        (self.resolution[0].max(320), self.resolution[1].max(240))
    }

    /// 设置窗口分辨率。
    pub fn set_resolution(&mut self, width: u32, height: u32) {
        self.resolution = [width.max(320), height.max(240)];
    }

    /// 渲染缩放（只接受 [`SCALES`]，非法值退回 1）。
    #[must_use]
    pub fn scale(&self) -> u32 {
        if SCALES.contains(&self.scale) {
            self.scale
        } else {
            DEFAULT_SCALE
        }
    }

    /// 设置渲染缩放。
    pub fn set_scale(&mut self, scale: u32) {
        self.scale = if SCALES.contains(&scale) {
            scale
        } else {
            DEFAULT_SCALE
        };
    }

    /// 用设置面板的当前状态覆盖设置文件的对应字段（保存前调用）。
    pub fn absorb_panel(&mut self, panel: &SettingsState) {
        self.sound = panel.sound_on;
        self.music = panel.music_on;
        self.volume = SettingsState::fraction(panel.sound_volume);
        self.music_volume = SettingsState::fraction(panel.music_volume);
        self.set_language(panel.language.code());
        self.set_resolution(panel.resolution.0, panel.resolution.1);
        self.set_scale(panel.scale);
    }

    /// 转成设置面板状态（打开设置面板时同步显示）。
    #[must_use]
    pub fn panel(&self) -> SettingsState {
        SettingsState {
            sound_on: self.sound,
            music_on: self.music,
            sound_volume: SettingsState::percent(self.sound_volume()),
            music_volume: SettingsState::percent(self.music_volume()),
            language: self.language(),
            resolution: self.resolution(),
            scale: self.scale(),
            master_volume: 50,
            window_mode: crate::client_config::WindowMode::Windowed,
            show_minimap_gates: true,
            keys: crate::client_config::KeyBindings::default(),
            rebinding: None,
            rebind_error: None,
        }
    }

    /// 分辨率文案（诊断日志用）。
    #[must_use]
    pub fn resolution_text(&self) -> String {
        resolution_text(self.resolution())
    }

    /// 设置文件路径。
    #[must_use]
    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("mir-next").join("settings.json")
    }

    /// 从磁盘加载；文件不存在或内容非法时用默认设置（默认中文）。
    #[must_use]
    pub fn load(path: &Path) -> Self {
        let Ok(source) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let mut settings: Self = serde_json::from_str(&source).unwrap_or_default();
        // 归一化：把 `ZH` 之类的别名写成标准代码，非法值退回中文。
        let language = Language::from_code(&settings.language).unwrap_or_default();
        language.code().clone_into(&mut settings.language);
        settings
    }

    /// 保存到磁盘（自动创建父目录）。
    ///
    /// # Errors
    /// 目录创建失败或写入失败时返回 IO 错误。
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let source = serde_json::to_string_pretty(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        std::fs::write(path, source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_simplified_chinese() {
        let settings = ClientSettings::default();
        assert_eq!(settings.language(), Language::ZhCn);
        assert_eq!(settings.language, "zh-cn");
    }

    #[test]
    fn sound_defaults_on_at_half_volume() {
        let settings = ClientSettings::default();
        assert!(settings.sound, "音效默认开启");
        assert!((settings.sound_volume() - 0.5).abs() < f32::EPSILON);
        assert!((settings.sound_volume() - crate::audio::DEFAULT_VOLUME).abs() < f32::EPSILON);
    }

    #[test]
    fn music_defaults_on_at_half_volume_independently() {
        let settings = ClientSettings::default();
        assert!(settings.music, "背景音乐默认开启");
        assert!((settings.music_volume() - 0.5).abs() < f32::EPSILON);
        assert!(
            (settings.music_volume() - crate::audio::DEFAULT_MUSIC_VOLUME).abs() < f32::EPSILON
        );
        let broken = ClientSettings {
            music_volume: f32::NAN,
            ..ClientSettings::default()
        };
        assert!((broken.music_volume() - DEFAULT_MUSIC_VOLUME).abs() < f32::EPSILON);
    }

    #[test]
    fn old_settings_files_without_sound_fields_still_load() {
        // 老版本设置文件只有 `language`：serde 默认值补上音效两项。
        let directory =
            std::env::temp_dir().join(format!("mir-next-settings-old-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("建临时目录");
        let path = directory.join("settings.json");
        std::fs::write(&path, "{\"language\":\"en\"}").expect("写设置");
        let settings = ClientSettings::load(&path);
        assert_eq!(settings.language, "en");
        assert!(settings.sound);
        assert!((settings.sound_volume() - 0.5).abs() < f32::EPSILON);
        // 老设置文件没有音乐两项：默认开启、音量 0.5。
        assert!(settings.music);
        assert!((settings.music_volume() - 0.5).abs() < f32::EPSILON);
        // 第 2 轮新增的服务器/显示三项同样有默认值。
        assert_eq!(settings.server(), DEFAULT_SERVER);
        assert_eq!(settings.resolution(), DEFAULT_RESOLUTION);
        assert_eq!(settings.scale(), DEFAULT_SCALE);
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn server_resolution_and_scale_roundtrip_through_disk() {
        let directory =
            std::env::temp_dir().join(format!("mir-next-settings-display-{}", std::process::id()));
        let path = directory.join("settings.json");
        let mut settings = ClientSettings::default();
        settings.set_server("10.0.0.5:7000");
        settings.set_resolution(1280, 720);
        settings.set_scale(2);
        settings
            .equipment
            .insert("张三".to_owned(), "木剑 / 布衣".to_owned());
        settings.save(&path).expect("保存设置");

        let loaded = ClientSettings::load(&path);
        assert_eq!(loaded, settings);
        assert_eq!(loaded.server(), "10.0.0.5:7000");
        assert_eq!(loaded.resolution(), (1280, 720));
        assert_eq!(loaded.scale(), 2);
        assert_eq!(
            loaded.equipment.get("张三").map(String::as_str),
            Some("木剑 / 布衣")
        );

        // 非法值会被归一化。
        let broken = ClientSettings {
            server: "  ".to_owned(),
            resolution: [10, 10],
            scale: 9,
            ..ClientSettings::default()
        };
        assert_eq!(broken.server(), DEFAULT_SERVER);
        assert_eq!(broken.resolution(), (320, 240));
        assert_eq!(broken.scale(), 1);
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn panel_state_roundtrips_into_settings() {
        let mut settings = ClientSettings::default();
        let mut panel = settings.panel();
        assert_eq!(panel.sound_volume, 50);
        panel.sound_on = false;
        panel.music_on = false;
        panel.sound_volume = 20;
        panel.music_volume = 80;
        panel.language = Language::En;
        panel.resolution = (1600, 900);
        panel.scale = 3;
        settings.absorb_panel(&panel);
        assert!(!settings.sound);
        assert!(!settings.music);
        assert!((settings.sound_volume() - 0.2).abs() < f32::EPSILON);
        assert!((settings.music_volume() - 0.8).abs() < f32::EPSILON);
        assert_eq!(settings.language(), Language::En);
        assert_eq!(settings.resolution(), (1600, 900));
        assert_eq!(settings.scale(), 3);
        assert_eq!(settings.panel(), panel);
    }

    #[test]
    fn volume_is_clamped_and_nan_falls_back() {
        let settings = ClientSettings {
            volume: 3.0,
            ..ClientSettings::default()
        };
        assert!((settings.sound_volume() - 1.0).abs() < f32::EPSILON);
        let settings = ClientSettings {
            volume: -2.0,
            ..ClientSettings::default()
        };
        assert!(settings.sound_volume().abs() < f32::EPSILON);
        let settings = ClientSettings {
            volume: f32::NAN,
            ..ClientSettings::default()
        };
        assert!((settings.sound_volume() - crate::audio::DEFAULT_VOLUME).abs() < f32::EPSILON);
    }

    #[test]
    fn set_language_normalises_aliases() {
        let mut settings = ClientSettings::default();
        assert!(settings.set_language("EN"));
        assert_eq!(settings.language, "en");
        assert!(settings.set_language("zh"));
        assert_eq!(settings.language, "zh-cn");
        assert!(!settings.set_language("klingon"));
        assert_eq!(settings.language, "zh-cn");
    }

    #[test]
    fn roundtrip_through_disk() {
        let directory =
            std::env::temp_dir().join(format!("mir-next-settings-{}", std::process::id()));
        let path = directory.join("settings.json");
        let mut settings = ClientSettings::default();
        settings.set_language("en");
        settings.save(&path).expect("保存设置");
        assert_eq!(ClientSettings::load(&path), settings);
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn missing_or_broken_file_falls_back_to_default() {
        let path = std::env::temp_dir().join("mir-next-settings-missing.json");
        std::fs::remove_file(&path).ok();
        assert_eq!(ClientSettings::load(&path), ClientSettings::default());

        std::fs::write(&path, "\"not an object\"").expect("写坏文件");
        assert_eq!(ClientSettings::load(&path), ClientSettings::default());
        std::fs::remove_file(&path).ok();
    }
}
