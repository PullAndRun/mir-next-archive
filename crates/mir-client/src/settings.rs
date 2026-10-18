//! 客户端本地设置。
//!
//! 语言选择属于本地设置：不依赖服务端也能切换界面语言，默认简体中文。
//! 音效开关与音量也在这里（默认开启、音量 0.5），命令行 `--no-sound` / `--volume`
//! 会覆盖并写回。
//! 设置文件放在 `%APPDATA%\mir-next\settings.json`（其它平台放在用户目录下的
//! `.mir-next/settings.json`），读写失败时回退到默认值而不是中断启动。

use std::path::{Path, PathBuf};

use mir_client::audio::DEFAULT_VOLUME;
use mir_core::Language;
use serde::{Deserialize, Serialize};

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

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            language: default_language(),
            sound: default_sound(),
            volume: default_volume(),
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
        assert!((settings.sound_volume() - mir_client::audio::DEFAULT_VOLUME).abs() < f32::EPSILON);
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
        std::fs::remove_dir_all(&directory).ok();
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
        assert!((settings.sound_volume() - mir_client::audio::DEFAULT_VOLUME).abs() < f32::EPSILON);
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
