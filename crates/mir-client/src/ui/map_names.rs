//! 地图名的客户端本地化（HUD / 小地图 / 大地图的标题）。
//!
//! 地图名是**数据**：放在 `config/lang/{zh-cn,en}.json` 的 `maps` 段（键 = 地图文件名，
//! 不含 `.map`），由 `mir_core::i18n::LanguageLibrary::embedded()` 用 `include_str!`
//! 编译进二进制（`crates/mir-client/src/main.rs` 也用它打印欢迎语）。本模块只负责查表与
//! 回退，**不硬编码任何地图名**；增删地图名只改语言包，不用改代码
//! （`docs/i18n.md` 的「地图名走语言包」口径）。
//!
//! 服务端 `S.NewMapInfo.title` / `S.MapChanged.title` 直接来自 `Server.MirDB` 的
//! `MapInfo.Title`（`docs/store/mirdb-format.md` §4.1），而语料里同时存在中文库与英文库
//! （`resource/community/suprcode-crystal-database/{cjlaaa,Jev}/Server.MirDB`：
//! `比奇省` / `BichonProvince`）。所以客户端标题按
//! 「语言包 → 服务端标题 → 文件名 → 未知地图」回退；
//! 只有标题、没有 mapfile 的两处（小地图传送点悬停、组队面板队友地图）用标题反查语言包。

use std::sync::OnceLock;

use mir_core::Language;
use mir_core::i18n::LanguageLibrary;

use crate::ui::labels::Label;

/// 编译进二进制的语言包。
///
/// `LanguageLibrary::embedded()` 每次调用都会解析两份 JSON；地图名在每帧绘制路径上
/// （HUD/小地图/大地图），所以这里只解析一次。
fn library() -> &'static LanguageLibrary {
    static LIBRARY: OnceLock<LanguageLibrary> = OnceLock::new();
    LIBRARY.get_or_init(LanguageLibrary::embedded)
}

/// 地图显示名：语言包 → 服务端标题 → 文件名 → 「未知地图」。
///
/// `data_title` 是服务端 `MapSnapshot.title`（可能为空、可能是另一种语言）。
#[must_use]
pub fn localized(file_name: &str, data_title: &str, language: Language) -> String {
    let file_name = file_name.trim();
    let data_title = data_title.trim();
    // `LanguageLibrary::map_title` 未收录时**原样返回文件名**
    // （`crates/mir-core/src/i18n.rs` 的 `map_title`），据此判断是否命中语言包。
    let from_pack = library().map_title(language, file_name);
    if !file_name.is_empty() && from_pack != file_name {
        return from_pack;
    }
    if !data_title.is_empty() {
        return data_title.to_owned();
    }
    if !file_name.is_empty() {
        return file_name.to_owned();
    }
    Label::UnknownMap.text(language).to_owned()
}

/// 只有服务端标题、没有 mapfile 时的显示名（`S.NewMapInfo.movements[].title`、
/// `S.GroupMembersMap` 都只带标题字符串）。
///
/// 用标题反查语言包两种语言的 `maps`：比较时忽略大小写与空格/下划线/连字符
/// （英文库里是 `BichonProvince`，`config/lang/en.json` 里是 `Bichon Province`）。
/// 命中就按当前界面语言显示；未命中原样返回（空串仍是空串）。
#[must_use]
pub fn localized_title(data_title: &str, language: Language) -> String {
    let title = data_title.trim();
    if title.is_empty() {
        return String::new();
    }
    let needle = normalize_title(title);
    let library = library();
    for pack_language in [Language::ZhCn, Language::En] {
        let Some(pack) = library.pack(pack_language) else {
            continue;
        };
        for (file, name) in &pack.maps {
            if normalize_title(name) == needle || normalize_title(file) == needle {
                return library.map_title(language, file);
            }
        }
    }
    title.to_owned()
}

/// 标题比较用的规范化：只留字母/数字（含中文），并转小写。
fn normalize_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn map_titles_come_from_the_language_packs() {
        let library = LanguageLibrary::embedded();
        assert_eq!(library.map_title(Language::ZhCn, "0"), "比奇省");
        assert_eq!(library.map_title(Language::En, "0"), "Bichon Province");
        assert_eq!(library.map_title(Language::ZhCn, "0104"), "书店");
        assert_eq!(library.map_title(Language::En, "0116"), "S_ValleyTavern");
        assert_eq!(library.map_title(Language::ZhCn, "0157"), "祖玛寺庙入口");

        // 文件名带首尾空白也要命中语言包（`localized` 会 trim）；未收录的图保留文件名。
        assert_eq!(localized(" 0108 ", "", Language::ZhCn), "药店");
        assert_eq!(localized("9999", "", Language::ZhCn), "9999");

        // 中文界面 + 英文库标题 / 英文界面 + 中文库标题都按语言包显示。
        assert_eq!(localized("0", "BichonProvince", Language::ZhCn), "比奇省");
        assert_eq!(localized("0", "比奇省", Language::En), "Bichon Province");
        assert_eq!(localized("0104", "Library", Language::ZhCn), "书店");
    }

    #[test]
    fn both_language_packs_cover_the_same_maps() {
        let library = library();
        let zh = library.pack(Language::ZhCn).expect("中文语言包");
        let en = library.pack(Language::En).expect("英文语言包");
        let zh_keys: BTreeSet<&String> = zh.maps.keys().collect();
        let en_keys: BTreeSet<&String> = en.maps.keys().collect();
        assert_eq!(zh_keys, en_keys, "两份语言包的 maps 键集合必须一致");
        assert!(!zh.maps.is_empty(), "语言包的 maps 段不应为空");
        for (file, title) in &zh.maps {
            assert!(!title.is_empty(), "{file} 缺中文名");
            let english = en.maps.get(file).expect("英文包应有同一个键");
            assert!(!english.is_empty(), "{file} 缺英文名");
            let has_cjk = english
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
            assert!(!has_cjk, "{file} english name contains CJK: {english}");
        }
    }

    #[test]
    fn unknown_maps_fall_back_to_the_server_title_then_the_file_name() {
        assert_eq!(
            localized("9999", "SomeDungeon", Language::ZhCn),
            "SomeDungeon"
        );
        assert_eq!(localized("9999", "", Language::En), "9999");
        assert_eq!(localized("", "SomeDungeon", Language::En), "SomeDungeon");
        assert_eq!(localized("", "", Language::ZhCn), "未知地图");
        assert_eq!(localized("", "", Language::En), "Unknown map");
    }

    #[test]
    fn titles_without_a_file_name_are_reverse_looked_up() {
        assert_eq!(localized_title("BichonProvince", Language::ZhCn), "比奇省");
        assert_eq!(localized_title("比奇省", Language::En), "Bichon Province");
        assert_eq!(localized_title("Library", Language::ZhCn), "书店");
        assert_eq!(
            localized_title("LobbyofZumaTemple", Language::En),
            "LobbyofZumaTemple"
        );
        // 未收录的标题原样保留；空串仍是空串。
        assert_eq!(
            localized_title("SomeDungeon", Language::ZhCn),
            "SomeDungeon"
        );
        assert_eq!(localized_title("  ", Language::En), "");
    }
}
