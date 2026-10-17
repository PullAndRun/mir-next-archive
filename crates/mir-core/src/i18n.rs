//! 多语言文本。
//!
//! 项目要求「默认全中文化，并能在游戏内切换其他语言」，因此这里把服务端
//! 会发给玩家的文本收成一张带键的表：默认语言是简体中文（`Language::ZhCn`），
//! 每种语言都覆盖全部键，缺项在编译期就被 [`text`] 的穷举 `match` 挡住。
//!
//! 客户端侧的语言切换属于本地设置；服务端在游戏内用聊天指令 `/language <代码>`
//! 切换（Crystal 协议里没有对应包，这是我们自己的扩展，见 `docs/i18n.md`）。

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

    /// 从代码解析（大小写不敏感，接受 `zh`/`zh-cn`/`cn`/`en`）。
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
}

/// 取某个语言下的文本。
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
        (Language::En, TextKey::Welcome) => "Welcome to Mir-Next.",
        (Language::En, TextKey::AccountBanned) => "This account is banned.",
        (Language::En, TextKey::WrongPassword) => "Incorrect account or password.",
        (Language::En, TextKey::TooManyCharacters) => "You have too many characters.",
        (Language::En, TextKey::LanguageChanged) => "Language switched to English.",
        (Language::En, TextKey::UnknownLanguage) => "Unknown language code. Available: zh-cn, en.",
        (Language::En, TextKey::ServerFull) => "The server is full, please try again later.",
        (Language::En, TextKey::UnsupportedCommand) => "Unsupported command.",
    }
}

/// 一个玩家的语言设置。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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

    /// 取当前语言下的文本。
    #[must_use]
    pub const fn text(&self, key: TextKey) -> &'static str {
        text(self.language, key)
    }

    /// 处理游戏内的 `/language` 指令，返回要回给玩家的文本。
    ///
    /// 支持 `/language`（显示当前语言）与 `/language en`（切换）。
    pub fn handle_command(&mut self, command: &str) -> Option<&'static str> {
        let rest = command.strip_prefix("/language")?.trim();
        if rest.is_empty() {
            return Some(self.text(TextKey::LanguageChanged));
        }
        if self.switch(rest) {
            Some(self.text(TextKey::LanguageChanged))
        } else {
            Some(self.text(TextKey::UnknownLanguage))
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
        // 空字符串会被当成「忘了翻译」。
        for language in Language::ALL {
            for key in [
                TextKey::Welcome,
                TextKey::AccountBanned,
                TextKey::WrongPassword,
                TextKey::TooManyCharacters,
                TextKey::LanguageChanged,
                TextKey::UnknownLanguage,
                TextKey::ServerFull,
                TextKey::UnsupportedCommand,
            ] {
                assert!(!text(*language, key).is_empty(), "{language:?} {key:?}");
            }
        }
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
}
