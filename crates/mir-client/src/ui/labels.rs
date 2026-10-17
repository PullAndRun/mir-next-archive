//! 界面文案表：中文优先，`--lang en` 切到英文。
//!
//! 只有几十个固定标签，用穷举 `match` 比加载语言包更省事，也不会漏翻。玩家/物品/地图
//! 之类的动态文本来自服务端，不在这里。

use mir_core::Language;
use mir_core::enums::ChatType;

/// 界面固定标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Label {
    /// 生命。
    Hp,
    /// 魔法。
    Mp,
    /// 金币。
    Gold,
    /// 地图。
    Map,
    /// 聊天。
    Chat,
    /// 背包。
    Inventory,
    /// 商店。
    Shop,
    /// 对话。
    Dialog,
    /// 技能。
    Skills,
    /// 关闭。
    Close,
    /// 出售。
    Sell,
    /// 修理。
    Repair,
    /// 丢弃。
    Drop,
    /// 等级。
    Level,
    /// 价格。
    Price,
    /// 倍率。
    Rate,
    /// 空。
    Empty,
    /// 尚未学习技能。
    NoSkills,
    /// 按 T 聊天。
    ChatHint,
    /// S 出售 · R 修理。
    ShopHint,
    /// 左键选中 · D 丢弃。
    InventoryHint,
    /// 未知物品。
    UnknownItem,
    /// 未知地图。
    UnknownMap,
    /// 可出售。
    SellAllowed,
    /// 可修理。
    RepairAllowed,
    /// 自定义键提示（NPC 对话）。
    NpcHint,
    /// 连接中。
    Connecting,
    /// 已断开。
    Disconnected,
    /// 施法键提示。
    CastHint,
}

impl Label {
    /// 取当前语言下的文案。
    #[must_use]
    pub const fn text(self, language: Language) -> &'static str {
        match (self, language) {
            (Self::Hp, Language::ZhCn) => "生命",
            (Self::Hp, Language::En) => "HP",
            (Self::Mp, Language::ZhCn) => "魔法",
            (Self::Mp, Language::En) => "MP",
            (Self::Gold, Language::ZhCn) => "金币",
            (Self::Gold, Language::En) => "Gold",
            (Self::Map, Language::ZhCn) => "地图",
            (Self::Map, Language::En) => "Map",
            (Self::Chat, Language::ZhCn) => "聊天",
            (Self::Chat, Language::En) => "Chat",
            (Self::Inventory, Language::ZhCn) => "背包",
            (Self::Inventory, Language::En) => "Inventory",
            (Self::Shop, Language::ZhCn) => "商店",
            (Self::Shop, Language::En) => "Shop",
            (Self::Dialog, Language::ZhCn) => "对话",
            (Self::Dialog, Language::En) => "Dialog",
            (Self::Skills, Language::ZhCn) => "技能",
            (Self::Skills, Language::En) => "Skills",
            (Self::Close, Language::ZhCn) => "关闭",
            (Self::Close, Language::En) => "Close",
            (Self::Sell, Language::ZhCn) => "出售",
            (Self::Sell | Self::SellAllowed, Language::En) => "Sell",
            (Self::Repair, Language::ZhCn) => "修理",
            (Self::Repair | Self::RepairAllowed, Language::En) => "Repair",
            (Self::Drop, Language::ZhCn) => "丢弃",
            (Self::Drop, Language::En) => "Drop",
            (Self::Level, Language::ZhCn) => "等级",
            (Self::Level, Language::En) => "Lv",
            (Self::Price, Language::ZhCn) => "价格",
            (Self::Price, Language::En) => "Price",
            (Self::Rate, Language::ZhCn) => "倍率",
            (Self::Rate, Language::En) => "Rate",
            (Self::Empty, Language::ZhCn) => "空",
            (Self::Empty, Language::En) => "-",
            (Self::NoSkills, Language::ZhCn) => "尚未学习技能",
            (Self::NoSkills, Language::En) => "No skills learned",
            (Self::ChatHint, Language::ZhCn) => "按 T 聊天",
            (Self::ChatHint, Language::En) => "Press T to chat",
            (Self::ShopHint, Language::ZhCn) => "S 出售 · R 修理",
            (Self::ShopHint, Language::En) => "S sell · R repair",
            (Self::InventoryHint, Language::ZhCn) => "左键选中 · D 丢弃",
            (Self::InventoryHint, Language::En) => "Click select · D drop",
            (Self::UnknownItem, Language::ZhCn) => "未知物品",
            (Self::UnknownItem, Language::En) => "Unknown item",
            (Self::UnknownMap, Language::ZhCn) => "未知地图",
            (Self::UnknownMap, Language::En) => "Unknown map",
            (Self::SellAllowed, Language::ZhCn) => "可出售",
            (Self::RepairAllowed, Language::ZhCn) => "可修理",
            (Self::NpcHint, Language::ZhCn) => "点击按钮继续 · Esc 关闭",
            (Self::NpcHint, Language::En) => "Click a button · Esc to close",
            (Self::Connecting, Language::ZhCn) => "连接中…",
            (Self::Connecting, Language::En) => "Connecting…",
            (Self::Disconnected, Language::ZhCn) => "已断开",
            (Self::Disconnected, Language::En) => "Disconnected",
            (Self::CastHint, Language::ZhCn) => "按数字键施法",
            (Self::CastHint, Language::En) => "Press number keys",
        }
    }

    /// 便捷方法：`Label::Hp.text(language)` 的短写。
    #[must_use]
    pub const fn of(self, language: Language) -> &'static str {
        self.text(language)
    }
}

/// 千位分隔（`12345` → `12,345`）。
#[must_use]
pub fn format_number(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    let offset = digits.len() % 3;
    for (index, character) in digits.chars().enumerate() {
        if index != 0 && index % 3 == offset {
            out.push(',');
        }
        out.push(character);
    }
    out
}

/// `金币 1,234` 之类的文案。
#[must_use]
pub fn gold_text(language: Language, gold: u32) -> String {
    format!(
        "{} {}",
        Label::Gold.text(language),
        format_number(u64::from(gold))
    )
}

/// 未知物品的占位名：`未知物品 #12`。
#[must_use]
pub fn unknown_item(language: Language, item_index: i32) -> String {
    format!("{} #{item_index}", Label::UnknownItem.text(language))
}

/// 聊天行颜色（按频道）。
#[must_use]
pub const fn chat_color(kind: ChatType) -> [u8; 4] {
    match kind {
        ChatType::System | ChatType::System2 | ChatType::Hint => [255, 230, 120, 255],
        ChatType::Announcement => [255, 160, 80, 255],
        ChatType::Shout | ChatType::Shout2 | ChatType::Shout3 => [255, 120, 120, 255],
        ChatType::WhisperIn | ChatType::WhisperOut => [255, 160, 230, 255],
        ChatType::Group => [120, 200, 255, 255],
        ChatType::Guild => [140, 240, 150, 255],
        ChatType::LevelUp => [255, 220, 80, 255],
        ChatType::Trainer | ChatType::Mentor => [200, 180, 255, 255],
        ChatType::Relationship => [255, 180, 210, 255],
        ChatType::LineMessage => [220, 220, 220, 255],
        ChatType::Normal => [240, 240, 240, 255],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_is_the_default_and_english_switches_labels() {
        assert_eq!(Label::Hp.text(Language::ZhCn), "生命");
        assert_eq!(Label::Hp.text(Language::En), "HP");
        assert_eq!(Label::Inventory.text(Language::ZhCn), "背包");
        assert_eq!(Label::Shop.text(Language::En), "Shop");
    }

    #[test]
    fn numbers_use_thousands_separators() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1_000), "1,000");
        assert_eq!(format_number(12_345_678), "12,345,678");
        assert_eq!(gold_text(Language::ZhCn, 4_321), "金币 4,321");
    }

    #[test]
    fn chat_colors_differ_by_channel() {
        assert_ne!(
            chat_color(ChatType::System),
            chat_color(ChatType::Normal),
            "系统消息应与普通聊天不同色"
        );
        assert_ne!(chat_color(ChatType::Shout), chat_color(ChatType::Group));
    }

    #[test]
    fn unknown_items_are_labelled_with_the_template_index() {
        assert_eq!(unknown_item(Language::ZhCn, 7), "未知物品 #7");
        assert_eq!(unknown_item(Language::En, 7), "Unknown item #7");
    }
}
