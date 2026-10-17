//! 多语言文本与语言包。
//!
//! 项目要求「默认全中文化，并能在游戏内切换其他语言」。这里分三层：
//!
//! 1. **编译期内置**：[`text`] 是 `(Language, TextKey)` 的穷举 `match`，保证任何语言
//!    都不会漏翻，也让默认值（简体中文）永远可用。
//! 2. **运行时语言包**：[`LanguageLibrary`] 从 `config/lang/*.json` 读取语言包，
//!    可以覆盖内置文本、补充地图名等内容文本，无需改代码重新编译。
//! 3. **会话级设置**：[`LanguageCatalog`] 保存每个玩家的当前语言，
//!    服务端在游戏内用 `/language <代码>` 切换（Crystal 协议没有对应包，
//!    这是我们自己的扩展，见 `docs/i18n.md`）。

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

/// 支持的语言。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Language {
    /// 简体中文（默认）。
    #[default]
    ZhCn,
    /// English.
    En,
}

impl Language {
    /// 全部语言。
    pub const ALL: &'static [Self] = &[Self::ZhCn, Self::En];

    /// 语言代码（BCP-47 的小写形式，指令里用它切换）。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-cn",
            Self::En => "en",
        }
    }

    /// 从代码解析（大小写不敏感，接受 `zh`/`zh-cn`/`cn`/`chs`/`en`/`en-us`）。
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code.trim().to_ascii_lowercase().as_str() {
            "zh" | "zh-cn" | "cn" | "zh_cn" | "chs" => Some(Self::ZhCn),
            "en" | "en-us" | "en_us" => Some(Self::En),
            _ => None,
        }
    }

    /// 语言的母语名（用于语言选择界面）。
    #[must_use]
    pub const fn native_name(self) -> &'static str {
        match self {
            Self::ZhCn => "简体中文",
            Self::En => "English",
        }
    }
}

/// 服务端发送给玩家的文本键。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextKey {
    /// 进入世界时的欢迎语。
    Welcome,
    /// 账号被封禁。
    AccountBanned,
    /// 口令错误。
    WrongPassword,
    /// 角色过多。
    TooManyCharacters,
    /// 语言切换成功（参数：语言名）。
    LanguageChanged,
    /// 未知的语言代码。
    UnknownLanguage,
    /// 服务器人数已满。
    ServerFull,
    /// 不支持的操作。
    UnsupportedCommand,
    /// 角色名已被占用。
    CharacterNameTaken,
    /// 角色名不合法。
    CharacterNameInvalid,
    /// 登录成功。
    LoginSuccess,
}

impl TextKey {
    /// 全部文本键（语言包与测试会用到）。
    pub const ALL: &'static [Self] = &[
        Self::Welcome,
        Self::AccountBanned,
        Self::WrongPassword,
        Self::TooManyCharacters,
        Self::LanguageChanged,
        Self::UnknownLanguage,
        Self::ServerFull,
        Self::UnsupportedCommand,
        Self::CharacterNameTaken,
        Self::CharacterNameInvalid,
        Self::LoginSuccess,
    ];

    /// 语言包里的键名（`config/lang/*.json` 的 `server` 段用这个名字）。
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Welcome => "welcome",
            Self::AccountBanned => "account_banned",
            Self::WrongPassword => "wrong_password",
            Self::TooManyCharacters => "too_many_characters",
            Self::LanguageChanged => "language_changed",
            Self::UnknownLanguage => "unknown_language",
            Self::ServerFull => "server_full",
            Self::UnsupportedCommand => "unsupported_command",
            Self::CharacterNameTaken => "character_name_taken",
            Self::CharacterNameInvalid => "character_name_invalid",
            Self::LoginSuccess => "login_success",
        }
    }
}

/// 取某个语言下的内置文本（语言包缺失时的兜底）。
///
/// `TextKey` 是穷举 `match`，新增键时会强制补全所有语言，不会出现「某种语言漏翻」。
#[must_use]
pub const fn text(language: Language, key: TextKey) -> &'static str {
    match (language, key) {
        (Language::ZhCn, TextKey::Welcome) => "欢迎来到传奇-Next。",
        (Language::ZhCn, TextKey::AccountBanned) => "该账号已被封禁。",
        (Language::ZhCn, TextKey::WrongPassword) => "账号或密码错误。",
        (Language::ZhCn, TextKey::TooManyCharacters) => "角色数量已达上限。",
        (Language::ZhCn, TextKey::LanguageChanged) => "语言已切换为简体中文。",
        (Language::ZhCn, TextKey::UnknownLanguage) => "未知的语言代码，可用：zh-cn、en。",
        (Language::ZhCn, TextKey::ServerFull) => "服务器人数已满，请稍后再试。",
        (Language::ZhCn, TextKey::UnsupportedCommand) => "不支持的命令。",
        (Language::ZhCn, TextKey::CharacterNameTaken) => "该角色名已被占用。",
        (Language::ZhCn, TextKey::CharacterNameInvalid) => {
            "角色名不合法：3-15 个汉字、字母、数字或下划线。"
        }
        (Language::ZhCn, TextKey::LoginSuccess) => "登录成功。",
        (Language::En, TextKey::Welcome) => "Welcome to Mir-Next.",
        (Language::En, TextKey::AccountBanned) => "This account is banned.",
        (Language::En, TextKey::WrongPassword) => "Incorrect account or password.",
        (Language::En, TextKey::TooManyCharacters) => "You have too many characters.",
        (Language::En, TextKey::LanguageChanged) => "Language switched to English.",
        (Language::En, TextKey::UnknownLanguage) => "Unknown language code. Available: zh-cn, en.",
        (Language::En, TextKey::ServerFull) => "The server is full, please try again later.",
        (Language::En, TextKey::UnsupportedCommand) => "Unsupported command.",
        (Language::En, TextKey::CharacterNameTaken) => "That character name is already taken.",
        (Language::En, TextKey::CharacterNameInvalid) => {
            "Invalid character name: 3-15 CJK characters, letters, digits or underscores."
        }
        (Language::En, TextKey::LoginSuccess) => "Login successful.",
    }
}

/// 语言包解析/加载错误。
#[derive(Debug, thiserror::Error)]
pub enum LanguagePackError {
    /// 读取文件失败。
    #[error("读取语言包 {path} 失败：{source}")]
    Io {
        /// 文件路径。
        path: String,
        /// 原始错误。
        source: std::io::Error,
    },
    /// JSON 解析失败。
    #[error("语言包 {path} 解析失败：{source}")]
    Parse {
        /// 文件路径。
        path: String,
        /// 原始错误。
        source: serde_json::Error,
    },
    /// 语言代码不认识。
    #[error("语言包 {path} 里的语言代码 {code} 不受支持")]
    UnknownLanguage {
        /// 文件路径。
        path: String,
        /// 语言代码。
        code: String,
    },
}

/// 一份语言包（`config/lang/<code>.json`）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LanguagePack {
    /// 语言代码，例如 `zh-cn`。
    pub language: String,
    /// 服务端文本：`TextKey::key()` → 文本。
    #[serde(default)]
    pub server: BTreeMap<String, String>,
    /// 内容文本：地图文件名的键（如 `"0"`）→ 地图标题。
    #[serde(default)]
    pub maps: BTreeMap<String, String>,
}

impl LanguagePack {
    /// 解析一份语言包。
    ///
    /// # Errors
    /// JSON 非法或语言代码不受支持时返回错误。
    pub fn from_json(source: &str) -> Result<Self, String> {
        let pack: Self = serde_json::from_str(source).map_err(|error| error.to_string())?;
        if Language::from_code(&pack.language).is_none() {
            return Err(format!("不支持的语言代码：{}", pack.language));
        }
        Ok(pack)
    }

    /// 该语言包对应的语言。
    #[must_use]
    pub fn language(&self) -> Language {
        Language::from_code(&self.language).unwrap_or_default()
    }
}

/// 全部语言包。
#[derive(Debug, Clone, Default)]
pub struct LanguageLibrary {
    packs: BTreeMap<String, LanguagePack>,
}

impl LanguageLibrary {
    /// 内置语言包（编译进二进制，保证开箱即用且默认中文）。
    ///
    /// # Panics
    /// 仅当仓库里的 `config/lang/*.json` 被改坏时才会 panic；这些文件由测试覆盖。
    #[must_use]
    pub fn embedded() -> Self {
        let mut library = Self::default();
        for source in [
            include_str!("../../../config/lang/zh-cn.json"),
            include_str!("../../../config/lang/en.json"),
        ] {
            let pack = LanguagePack::from_json(source).expect("内置语言包必须合法");
            library.packs.insert(pack.language.clone(), pack);
        }
        library
    }

    /// 从一个目录加载全部 `*.json` 语言包，覆盖同名的内置语言包。
    ///
    /// # Errors
    /// 目录读取失败、JSON 非法或语言代码不受支持时返回错误。
    pub fn load_dir(directory: &Path) -> Result<Self, LanguagePackError> {
        let mut library = Self::embedded();
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            // 目录不存在时不报错：内置语言包已经够用，便于部署时精简。
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(library),
            Err(source) => {
                return Err(LanguagePackError::Io {
                    path: directory.display().to_string(),
                    source,
                });
            }
        };
        for entry in entries {
            let entry = entry.map_err(|source| LanguagePackError::Io {
                path: directory.display().to_string(),
                source,
            })?;
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let source =
                std::fs::read_to_string(&path).map_err(|source| LanguagePackError::Io {
                    path: path.display().to_string(),
                    source,
                })?;
            let pack = serde_json::from_str::<LanguagePack>(&source).map_err(|source| {
                LanguagePackError::Parse {
                    path: path.display().to_string(),
                    source,
                }
            })?;
            if Language::from_code(&pack.language).is_none() {
                return Err(LanguagePackError::UnknownLanguage {
                    path: path.display().to_string(),
                    code: pack.language.clone(),
                });
            }
            library.packs.insert(pack.language.clone(), pack);
        }
        Ok(library)
    }

    /// 已加载的语言代码。
    #[must_use]
    pub fn languages(&self) -> Vec<Language> {
        self.packs
            .values()
            .map(LanguagePack::language)
            .collect::<Vec<_>>()
    }

    /// 取某个语言包；缺语言包时返回 `None`。
    #[must_use]
    pub fn pack(&self, language: Language) -> Option<&LanguagePack> {
        self.packs.get(language.code())
    }

    /// 取服务端文本：优先语言包，缺失时退回编译期内置文本。
    #[must_use]
    pub fn text(&self, language: Language, key: TextKey) -> String {
        if let Some(value) = self
            .pack(language)
            .and_then(|pack| pack.server.get(key.key()))
        {
            return value.clone();
        }
        text(language, key).to_owned()
    }

    /// 取地图标题：优先语言包，缺失时用地图文件名兜底。
    #[must_use]
    pub fn map_title(&self, language: Language, map_file: &str) -> String {
        if let Some(value) = self
            .pack(language)
            .or_else(|| self.pack(Language::ZhCn))
            .and_then(|pack| pack.maps.get(map_file))
        {
            return value.clone();
        }
        map_file.to_owned()
    }
}

/// 一个玩家的语言设置。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LanguageCatalog {
    language: Language,
}

impl LanguageCatalog {
    /// 用默认语言（简体中文）构造。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            language: Language::ZhCn,
        }
    }

    /// 当前语言。
    #[must_use]
    pub const fn language(&self) -> Language {
        self.language
    }

    /// 直接用枚举设置。
    pub fn set(&mut self, language: Language) {
        self.language = language;
    }

    /// 切换语言；代码不认识时返回 `false`，保持原语言。
    pub fn switch(&mut self, code: &str) -> bool {
        match Language::from_code(code) {
            Some(language) => {
                self.language = language;
                true
            }
            None => false,
        }
    }

    /// 取当前语言下的内置文本（不走语言包）。
    #[must_use]
    pub const fn text(&self, key: TextKey) -> &'static str {
        text(self.language, key)
    }

    /// 取当前语言下的文本，优先用语言包里的覆盖值。
    #[must_use]
    pub fn text_with(&self, library: &LanguageLibrary, key: TextKey) -> String {
        library.text(self.language, key)
    }

    /// 解析游戏内的 `/language` 指令。
    ///
    /// 支持 `/language`（显示当前语言）与 `/language en`（切换）。
    /// 返回值是「切换后的语言」，`None` 表示这条消息不是语言指令。
    pub fn parse_command(&mut self, command: &str) -> Option<Result<Language, ()>> {
        let rest = command.strip_prefix("/language")?;
        let rest = rest.trim();
        if rest.is_empty() {
            return Some(Ok(self.language));
        }
        if self.switch(rest) {
            Some(Ok(self.language))
        } else {
            Some(Err(()))
        }
    }

    /// 处理游戏内的 `/language` 指令，返回要回给玩家的内置文本。
    pub fn handle_command(&mut self, command: &str) -> Option<&'static str> {
        match self.parse_command(command)? {
            Ok(_) => Some(self.text(TextKey::LanguageChanged)),
            Err(()) => Some(self.text(TextKey::UnknownLanguage)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_language_is_simplified_chinese() {
        let catalog = LanguageCatalog::default();
        assert_eq!(catalog.language(), Language::ZhCn);
        assert_eq!(catalog.text(TextKey::Welcome), "欢迎来到传奇-Next。");
    }

    #[test]
    fn every_key_is_translated_in_every_language() {
        for language in Language::ALL {
            for key in TextKey::ALL {
                assert!(!text(*language, *key).is_empty(), "{language:?} {key:?}");
            }
        }
    }

    #[test]
    fn every_key_has_a_key_name() {
        for key in TextKey::ALL {
            assert!(!key.key().is_empty());
            assert!(key.key().is_ascii(), "语言包键名用 ASCII：{}", key.key());
        }
    }

    #[test]
    fn embedded_language_packs_cover_every_key() {
        let library = LanguageLibrary::embedded();
        assert_eq!(library.languages().len(), 2, "内置中英两份语言包");
        for language in Language::ALL {
            let pack = library.pack(*language).expect("语言包存在");
            assert_eq!(pack.language(), *language);
            for key in TextKey::ALL {
                assert!(
                    pack.server.contains_key(key.key()),
                    "{language:?} 语言包缺少 {}",
                    key.key()
                );
            }
        }
    }

    #[test]
    fn embedded_pack_matches_builtin_text() {
        // 语言包是内置文本的镜像；改一边忘了另一边时这条会拦住。
        let library = LanguageLibrary::embedded();
        for language in Language::ALL {
            for key in TextKey::ALL {
                assert_eq!(
                    library.text(*language, *key),
                    text(*language, *key),
                    "{language:?} {key:?}"
                );
            }
        }
    }

    #[test]
    fn missing_pack_falls_back_to_builtin_text() {
        let library = LanguageLibrary::default();
        assert_eq!(
            library.text(Language::En, TextKey::Welcome),
            "Welcome to Mir-Next."
        );
    }

    #[test]
    fn map_titles_are_localized() {
        let library = LanguageLibrary::embedded();
        assert_eq!(library.map_title(Language::ZhCn, "0"), "比奇省");
        assert_eq!(library.map_title(Language::En, "0"), "Bichon Province");
        // 未收录的地图用文件名兜底，不会 panic。
        assert_eq!(
            library.map_title(Language::En, "unknown-map"),
            "unknown-map"
        );
    }

    #[test]
    fn switching_by_code_works_and_rejects_garbage() {
        let mut catalog = LanguageCatalog::new();
        assert!(catalog.switch("EN"));
        assert_eq!(catalog.language(), Language::En);
        assert!(catalog.switch("zh-cn"));
        assert_eq!(catalog.language(), Language::ZhCn);
        assert!(!catalog.switch("klingon"));
        assert_eq!(catalog.language(), Language::ZhCn, "非法代码不应改变语言");
    }

    #[test]
    fn in_game_command_switches_language() {
        let mut catalog = LanguageCatalog::new();
        assert_eq!(
            catalog.handle_command("/language en"),
            Some("Language switched to English.")
        );
        assert_eq!(catalog.language(), Language::En);
        assert_eq!(
            catalog.handle_command("/language"),
            Some("Language switched to English.")
        );
        assert_eq!(
            catalog.handle_command("/language tlh"),
            Some("Unknown language code. Available: zh-cn, en.")
        );
        assert_eq!(catalog.handle_command("/say hi"), None);
    }

    #[test]
    fn language_pack_rejects_unknown_code() {
        let source = r#"{"language":"tlh","server":{},"maps":{}}"#;
        assert!(LanguagePack::from_json(source).is_err());
    }
}
