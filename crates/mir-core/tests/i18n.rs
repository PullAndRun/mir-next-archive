//! `mir_core::i18n` 的公共 API 契约测试。
//!
//! 单元测试覆盖内部实现细节（`render_template` 的转义等）；这里从 crate 外部
//! 验证三条扫描线要用的公共接口：
//!
//! 1. `TextKey::ALL` / `TextKey::key()` / `TextKey::args()` 完整且无重复；
//! 2. `config/lang/{zh-cn,en}.json` 的 `server` 段覆盖全部键，且与内置模板逐字一致；
//! 3. `format_text` 的替换 / 缺参保留 / 多余实参忽略语义；
//! 4. 第 1 轮 11 个键的文案保持不变。

use std::collections::{BTreeSet, HashSet};

use mir_core::i18n::{Language, LanguageLibrary, LanguagePack, TextKey, format_text, text};

/// 仓库里的两份语言包原文（与 `LanguageLibrary::embedded` 同一来源）。
const ZH_CN: &str = include_str!("../../../config/lang/zh-cn.json");
const EN: &str = include_str!("../../../config/lang/en.json");

fn parsed_pack(source: &str, language: Language) -> LanguagePack {
    let pack = LanguagePack::from_json(source).expect("内置语言包必须合法");
    assert_eq!(pack.language(), language);
    pack
}

#[test]
fn text_key_all_has_unique_names() {
    let names: BTreeSet<&str> = TextKey::ALL.iter().map(|key| key.key()).collect();
    assert_eq!(names.len(), TextKey::ALL.len(), "TextKey::key() 有重复");
    let keys: HashSet<_> = TextKey::ALL.iter().collect();
    assert_eq!(keys.len(), TextKey::ALL.len(), "TextKey::ALL 有重复");
}

#[test]
fn both_packs_carry_every_key_with_the_builtin_template() {
    for (language, source) in [(Language::ZhCn, ZH_CN), (Language::En, EN)] {
        let pack = parsed_pack(source, language);
        for key in TextKey::ALL {
            let stored = pack
                .server
                .get(key.key())
                .unwrap_or_else(|| panic!("{language:?} 语言包缺少 {}", key.key()));
            assert_eq!(
                stored,
                text(language, *key),
                "{language:?} 语言包里的 {} 与内置模板不一致",
                key.key()
            );
            assert!(!stored.is_empty(), "{language:?} 的 {} 是空文本", key.key());
        }
    }
}

#[test]
fn both_packs_share_the_same_key_set() {
    let zh = parsed_pack(ZH_CN, Language::ZhCn);
    let en = parsed_pack(EN, Language::En);
    let zh_keys: BTreeSet<&str> = zh.server.keys().map(String::as_str).collect();
    let en_keys: BTreeSet<&str> = en.server.keys().map(String::as_str).collect();
    assert_eq!(zh_keys, en_keys, "中英语言包的键集合必须完全一致");

    let declared: BTreeSet<&str> = TextKey::ALL.iter().map(|key| key.key()).collect();
    assert_eq!(en_keys, declared, "语言包与 TextKey::ALL 的键集合必须一致");
}

#[test]
fn format_text_substitutes_known_placeholders_and_keeps_unknown_ones() {
    let key = TextKey::MailNotEnoughGold;
    assert_eq!(
        format_text(Language::ZhCn, key, &[("needed", "100"), ("held", "40")]),
        "金币不足：需要 100，你只有 40。"
    );
    assert_eq!(
        format_text(Language::En, key, &[("needed", "100"), ("held", "40")]),
        "Not enough Gold: requires 100, you have 40."
    );
    // 漏参：未提供的占位符原样保留。
    assert_eq!(
        format_text(Language::ZhCn, key, &[("needed", "100")]),
        "金币不足：需要 100，你只有 {held}。"
    );
    assert_eq!(
        format_text(Language::En, key, &[("held", "40")]),
        "Not enough Gold: requires {needed}, you have 40."
    );
}

#[test]
fn format_text_ignores_extra_arguments() {
    assert_eq!(
        format_text(Language::ZhCn, TextKey::TradeSuccess, &[("name", "谁")],),
        text(Language::ZhCn, TextKey::TradeSuccess)
    );
    assert_eq!(
        format_text(
            Language::En,
            TextKey::MarketBought,
            &[
                ("gold", "100"),
                ("item", "Hellblade"),
                ("unused", "ignored")
            ]
        ),
        "You bought Hellblade for 100 Gold."
    );
}

#[test]
fn every_key_renders_all_of_its_declared_placeholders() {
    for key in TextKey::ALL {
        let names = key.args();
        let values: Vec<String> = names.iter().map(|name| format!("<{name}>")).collect();
        let args: Vec<(&str, &str)> = names
            .iter()
            .copied()
            .zip(values.iter().map(String::as_str))
            .collect();
        for language in Language::ALL {
            let rendered = format_text(*language, *key, &args);
            assert!(!rendered.is_empty(), "{} {language:?}", key.key());
            assert!(
                !rendered.contains('{'),
                "{} {language:?} 渲染后还有未替换的占位符：{rendered}",
                key.key()
            );
            for value in &values {
                assert!(
                    rendered.contains(value.as_str()),
                    "{} {language:?} 的渲染结果里没有 {value}",
                    key.key()
                );
            }
        }
    }
}

#[test]
fn language_library_embedded_matches_the_config_files() {
    let library = LanguageLibrary::embedded();
    for (language, source) in [(Language::ZhCn, ZH_CN), (Language::En, EN)] {
        let expected = parsed_pack(source, language);
        let loaded = library.pack(language).expect("内置语言包存在");
        assert_eq!(
            loaded.server, expected.server,
            "{language:?} 的 embedded 语言包与 config/lang 文件不一致"
        );
    }
}

#[test]
fn language_library_format_matches_format_text() {
    let library = LanguageLibrary::embedded();
    let args = [("item", "炼狱"), ("gold", "100")];
    for key in TextKey::ALL {
        for language in Language::ALL {
            assert_eq!(
                library.format(*language, *key, &args),
                format_text(*language, *key, &args),
                "{} {language:?}",
                key.key()
            );
        }
    }
}

#[test]
fn round_one_keys_keep_their_wording() {
    let expected = [
        (
            TextKey::Welcome,
            "欢迎来到传奇-Next。",
            "Welcome to Mir-Next.",
        ),
        (
            TextKey::AccountBanned,
            "该账号已被封禁。",
            "This account is banned.",
        ),
        (
            TextKey::WrongPassword,
            "账号或密码错误。",
            "Incorrect account or password.",
        ),
        (
            TextKey::TooManyCharacters,
            "角色数量已达上限。",
            "You have too many characters.",
        ),
        (
            TextKey::LanguageChanged,
            "语言已切换为简体中文。",
            "Language switched to English.",
        ),
        (
            TextKey::UnknownLanguage,
            "未知的语言代码，可用：zh-cn、en。",
            "Unknown language code. Available: zh-cn, en.",
        ),
        (
            TextKey::ServerFull,
            "服务器人数已满，请稍后再试。",
            "The server is full, please try again later.",
        ),
        (
            TextKey::UnsupportedCommand,
            "不支持的命令。",
            "Unsupported command.",
        ),
        (
            TextKey::CharacterNameTaken,
            "该角色名已被占用。",
            "That character name is already taken.",
        ),
        (
            TextKey::CharacterNameInvalid,
            "角色名不合法：3-15 个汉字、字母、数字或下划线。",
            "Invalid character name: 3-15 CJK characters, letters, digits or underscores.",
        ),
        (TextKey::LoginSuccess, "登录成功。", "Login successful."),
    ];
    for (key, zh, en) in expected {
        assert!(key.args().is_empty(), "{} 不该有参数", key.key());
        assert_eq!(text(Language::ZhCn, key), zh, "{}", key.key());
        assert_eq!(text(Language::En, key), en, "{}", key.key());
    }
}
