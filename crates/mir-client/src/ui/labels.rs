//! 界面文案表：中文优先，`--lang en` 切到英文。
//!
//! 只有几十个固定标签，用穷举 `match` 比加载语言包更省事，也不会漏翻。玩家/物品/地图
//! 之类的动态文本来自服务端，不在这里。

use mir_core::Language;
use mir_core::enums::{BuffType, ChatType, EquipmentSlot, ItemGrade, ItemType, RequiredType};

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
    /// 装备面板标题。
    Equipment,
    /// 装备（动词）。
    Equip,
    /// 使用。
    Use,
    /// 类型。
    Type,
    /// 需求。
    Require,
    /// 持久度。
    Durability,
    /// 堆叠。
    Stack,
    /// 攻击（DC）。
    Attack,
    /// 防御（AC）。
    Defence,
    /// 魔防（MAC）。
    MagicDefence,
    /// 魔法（MC）。
    MagicAttack,
    /// 道术（SC）。
    Taoism,
    /// 准确。
    Accuracy,
    /// 敏捷。
    Agility,
    /// 攻击速度。
    AttackSpeed,
    /// 幸运。
    Luck,
    /// 属性汇总。
    Totals,
    /// 增益栏。
    Buffs,
    /// 秒。
    Seconds,
    /// 永久。
    Infinite,
    /// 暂停。
    Paused,
    /// 闪避。
    Miss,
    /// 暴击。
    Critical,
    /// 世界点击走路提示。
    WorldHint,
    /// 背包交互提示（双击/右键）。
    InventoryTooltipHint,
    /// 小地图面板标题。
    Minimap,
    /// 没有小地图素材时的占位文字。
    NoMinimap,
    /// 小地图面板底部的按键提示。
    MinimapHint,
    /// 没有大地图素材时的占位文字。
    NoBigmap,
    /// 大地图面板底部的按键提示。
    BigmapHint,
}

impl Label {
    /// 取当前语言下的文案。
    #[must_use]
    pub const fn text(self, language: Language) -> &'static str {
        match (self, language) {
            (Self::Hp, Language::ZhCn) => "生命",
            (Self::Hp, Language::En) => "HP",
            (Self::Mp | Self::MagicAttack, Language::ZhCn) => "魔法",
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
            (Self::Equipment | Self::Equip, Language::ZhCn) => "装备",
            (Self::Equipment, Language::En) => "Equipment",
            (Self::Equip, Language::En) => "Equip",
            (Self::Use, Language::ZhCn) => "使用",
            (Self::Use, Language::En) => "Use",
            (Self::Type, Language::ZhCn) => "类型",
            (Self::Type, Language::En) => "Type",
            (Self::Require, Language::ZhCn) => "需求",
            (Self::Require, Language::En) => "Require",
            (Self::Durability, Language::ZhCn) => "持久",
            (Self::Durability, Language::En) => "Durability",
            (Self::Stack, Language::ZhCn) => "堆叠",
            (Self::Stack, Language::En) => "Stack",
            (Self::Attack, Language::ZhCn) => "攻击",
            (Self::Attack, Language::En) => "DC",
            (Self::Defence, Language::ZhCn) => "防御",
            (Self::Defence, Language::En) => "AC",
            (Self::MagicDefence, Language::ZhCn) => "魔防",
            (Self::MagicDefence, Language::En) => "MAC",
            (Self::MagicAttack, Language::En) => "MC",
            (Self::Taoism, Language::ZhCn) => "道术",
            (Self::Taoism, Language::En) => "SC",
            (Self::Accuracy, Language::ZhCn) => "准确",
            (Self::Accuracy, Language::En) => "Accuracy",
            (Self::Agility, Language::ZhCn) => "敏捷",
            (Self::Agility, Language::En) => "Agility",
            (Self::AttackSpeed, Language::ZhCn) => "攻速",
            (Self::AttackSpeed, Language::En) => "Atk Speed",
            (Self::Luck, Language::ZhCn) => "幸运",
            (Self::Luck, Language::En) => "Luck",
            (Self::Totals, Language::ZhCn) => "属性汇总",
            (Self::Totals, Language::En) => "Totals",
            (Self::Buffs, Language::ZhCn) => "增益",
            (Self::Buffs, Language::En) => "Buffs",
            (Self::Seconds, Language::ZhCn) => "秒",
            (Self::Seconds, Language::En) => "s",
            (Self::Infinite, Language::ZhCn) => "永久",
            (Self::Infinite, Language::En) => "infinite",
            (Self::Paused, Language::ZhCn) => "暂停",
            (Self::Paused, Language::En) => "paused",
            (Self::Miss, Language::ZhCn) => "闪避",
            (Self::Miss, Language::En) => "Miss",
            (Self::Critical, Language::ZhCn) => "暴击",
            (Self::Critical, Language::En) => "Crit",
            (Self::WorldHint, Language::ZhCn) => "点地走路 · 按住连续走",
            (Self::WorldHint, Language::En) => "Click ground to walk · hold to continue",
            (Self::InventoryTooltipHint, Language::ZhCn) => "双击使用 · 右键菜单 · 拖动装备",
            (Self::InventoryTooltipHint, Language::En) => {
                "Double-click use · right-click menu · drag to equip"
            }
            (Self::Minimap, Language::ZhCn) => "小地图",
            (Self::Minimap, Language::En) => "Minimap",
            (Self::NoMinimap, Language::ZhCn) => "无小地图素材",
            (Self::NoMinimap, Language::En) => "No minimap art",
            (Self::MinimapHint, Language::ZhCn) => "M 关闭",
            (Self::MinimapHint, Language::En) => "M to close",
            (Self::NoBigmap, Language::ZhCn) => "无大地图素材",
            (Self::NoBigmap, Language::En) => "No big map art",
            (Self::BigmapHint, Language::ZhCn) => "B / Esc 关闭",
            (Self::BigmapHint, Language::En) => "B / Esc to close",
        }
    }

    /// 便捷方法：`Label::Hp.text(language)` 的短写。
    #[must_use]
    pub const fn of(self, language: Language) -> &'static str {
        self.text(language)
    }
}

/// 中英对照的小工具：`(zh, en)` → 当前语言。
const fn pair(language: Language, zh: &'static str, en: &'static str) -> &'static str {
    match language {
        Language::ZhCn => zh,
        Language::En => en,
    }
}

/// 物品类型名（tooltip 用；`ItemType` 的 43 个成员全部覆盖）。
#[must_use]
pub fn item_type_name(item_type: ItemType, language: Language) -> &'static str {
    let (zh, en) = match item_type {
        ItemType::Nothing => ("杂物", "Misc"),
        ItemType::Weapon => ("武器", "Weapon"),
        ItemType::Armour => ("衣服", "Armour"),
        ItemType::Helmet => ("头盔", "Helmet"),
        ItemType::Necklace => ("项链", "Necklace"),
        ItemType::Bracelet => ("手镯", "Bracelet"),
        ItemType::Ring => ("戒指", "Ring"),
        ItemType::Amulet => ("护身符", "Amulet"),
        ItemType::Belt => ("腰带", "Belt"),
        ItemType::Boots => ("靴子", "Boots"),
        ItemType::Stone => ("宝石", "Stone"),
        ItemType::Torch => ("火把", "Torch"),
        ItemType::Potion => ("药品", "Potion"),
        ItemType::Ore => ("矿石", "Ore"),
        ItemType::Meat => ("肉类", "Meat"),
        ItemType::CraftingMaterial => ("材料", "Material"),
        ItemType::Scroll => ("卷轴", "Scroll"),
        ItemType::Gem => ("宝石", "Gem"),
        ItemType::Mount => ("坐骑", "Mount"),
        ItemType::Book => ("技能书", "Skill book"),
        ItemType::Script => ("脚本", "Script"),
        ItemType::Reins => ("缰绳", "Reins"),
        ItemType::Bells => ("铃铛", "Bells"),
        ItemType::Saddle => ("马鞍", "Saddle"),
        ItemType::Ribbon => ("丝带", "Ribbon"),
        ItemType::Mask => ("面具", "Mask"),
        ItemType::Food => ("食物", "Food"),
        ItemType::Hook => ("鱼钩", "Hook"),
        ItemType::Float => ("浮漂", "Float"),
        ItemType::Bait => ("鱼饵", "Bait"),
        ItemType::Finder => ("探鱼器", "Finder"),
        ItemType::Reel => ("卷线器", "Reel"),
        ItemType::Fish => ("鱼", "Fish"),
        ItemType::Quest => ("任务物品", "Quest item"),
        ItemType::Awakening => ("觉醒材料", "Awakening"),
        ItemType::Pets => ("宠物物品", "Pet item"),
        ItemType::Transform => ("变身物品", "Transform"),
        ItemType::Deco => ("装饰", "Decoration"),
        ItemType::Socket => ("镶嵌", "Socket"),
        ItemType::MonsterSpawn => ("怪物刷新", "Monster spawn"),
        ItemType::SiegeAmmo => ("攻城弹药", "Siege ammo"),
        ItemType::SealedHero => ("封印英雄", "Sealed hero"),
    };
    pair(language, zh, en)
}

/// 装备槽名（纸娃娃面板与 tooltip 用）。
#[must_use]
pub fn equipment_slot_name(slot: EquipmentSlot, language: Language) -> &'static str {
    let (zh, en) = match slot {
        EquipmentSlot::Weapon => ("武器", "Weapon"),
        EquipmentSlot::Armour => ("衣服", "Armour"),
        EquipmentSlot::Helmet => ("头盔", "Helmet"),
        EquipmentSlot::Torch => ("火把", "Torch"),
        EquipmentSlot::Necklace => ("项链", "Necklace"),
        EquipmentSlot::BraceletL => ("左手镯", "L. Bracelet"),
        EquipmentSlot::BraceletR => ("右手镯", "R. Bracelet"),
        EquipmentSlot::RingL => ("左戒指", "L. Ring"),
        EquipmentSlot::RingR => ("右戒指", "R. Ring"),
        EquipmentSlot::Amulet => ("护身符", "Amulet"),
        EquipmentSlot::Belt => ("腰带", "Belt"),
        EquipmentSlot::Boots => ("靴子", "Boots"),
        EquipmentSlot::Stone => ("宝石", "Stone"),
        EquipmentSlot::Mount => ("坐骑", "Mount"),
    };
    pair(language, zh, en)
}

/// 需求属性名（tooltip 的「需求」行）。
#[must_use]
pub fn required_type_name(required: RequiredType, language: Language) -> &'static str {
    let (zh, en) = match required {
        RequiredType::Level => ("等级", "Level"),
        RequiredType::MaxAC => ("防御上限", "Max AC"),
        RequiredType::MaxMAC => ("魔防上限", "Max MAC"),
        RequiredType::MaxDC => ("攻击上限", "Max DC"),
        RequiredType::MaxMC => ("魔法上限", "Max MC"),
        RequiredType::MaxSC => ("道术上限", "Max SC"),
        RequiredType::MaxLevel => ("最高等级", "Max level"),
        RequiredType::MinAC => ("防御下限", "Min AC"),
        RequiredType::MinMAC => ("魔防下限", "Min MAC"),
        RequiredType::MinDC => ("攻击下限", "Min DC"),
        RequiredType::MinMC => ("魔法下限", "Min MC"),
        RequiredType::MinSC => ("道术下限", "Min SC"),
    };
    pair(language, zh, en)
}

/// 增益名；未收录的增益返回 `None`（调用方退化成 `增益 #n`）。
#[must_use]
pub fn buff_name(buff: BuffType, language: Language) -> Option<&'static str> {
    let (zh, en) = match buff {
        BuffType::None => ("无", "None"),
        BuffType::TemporalFlux => ("时空领域", "Temporal Flux"),
        BuffType::Hiding => ("隐身", "Hiding"),
        BuffType::Haste => ("加速", "Haste"),
        BuffType::SwiftFeet => ("疾风步", "Swift Feet"),
        BuffType::Fury => ("狂暴", "Fury"),
        BuffType::SoulShield => ("灵魂盾", "Soul Shield"),
        BuffType::BlessedArmour => ("神圣战甲", "Blessed Armour"),
        BuffType::LightBody => ("轻身术", "Light Body"),
        BuffType::UltimateEnhancer => ("强化术", "Ultimate Enhancer"),
        BuffType::ProtectionField => ("保护领域", "Protection Field"),
        BuffType::Rage => ("愤怒", "Rage"),
        BuffType::Curse => ("诅咒", "Curse"),
        BuffType::MoonLight => ("月光", "Moon Light"),
        BuffType::DarkBody => ("暗影体", "Dark Body"),
        BuffType::Concentration => ("专注", "Concentration"),
        BuffType::VampireShot => ("吸血箭", "Vampire Shot"),
        BuffType::PoisonShot => ("毒箭", "Poison Shot"),
        BuffType::CounterAttack => ("反击", "Counter Attack"),
        BuffType::MentalState => ("精神集中", "Mental State"),
        BuffType::EnergyShield => ("能量盾", "Energy Shield"),
        BuffType::MagicBooster => ("魔力强化", "Magic Booster"),
        BuffType::PetEnhancer => ("宠物强化", "Pet Enhancer"),
        BuffType::ImmortalSkin => ("不灭之躯", "Immortal Skin"),
        BuffType::MagicShield => ("魔法盾", "Magic Shield"),
        BuffType::ElementalBarrier => ("元素结界", "Elemental Barrier"),
        BuffType::Blindness => ("失明", "Blindness"),
        BuffType::GameMaster => ("管理员", "Game Master"),
        BuffType::General => ("将军", "General"),
        BuffType::Exp => ("经验加成", "EXP"),
        BuffType::Drop => ("掉落加成", "Drop"),
        BuffType::Gold => ("金币加成", "Gold"),
        BuffType::BagWeight => ("负重", "Bag Weight"),
        BuffType::Transform => ("变身", "Transform"),
        BuffType::Lover => ("恋人", "Lover"),
        BuffType::Mentee => ("徒弟", "Mentee"),
        _ => return None,
    };
    Some(pair(language, zh, en))
}

/// 品级颜色（tooltip 物品名）：无品级为普通白，越高越亮。
#[must_use]
pub const fn grade_color(grade: ItemGrade) -> [u8; 4] {
    match grade {
        ItemGrade::None => [236, 240, 246, 255],
        ItemGrade::Common => [180, 220, 255, 255],
        ItemGrade::Rare => [120, 220, 120, 255],
        ItemGrade::Legendary => [255, 200, 80, 255],
        ItemGrade::Mythical => [255, 140, 80, 255],
        ItemGrade::Heroic => [240, 120, 255, 255],
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

    #[test]
    fn item_type_slot_and_buff_names_are_localized() {
        assert_eq!(item_type_name(ItemType::Weapon, Language::ZhCn), "武器");
        assert_eq!(item_type_name(ItemType::Weapon, Language::En), "Weapon");
        assert_eq!(item_type_name(ItemType::Potion, Language::ZhCn), "药品");
        assert_eq!(
            equipment_slot_name(EquipmentSlot::RingR, Language::ZhCn),
            "右戒指"
        );
        assert_eq!(
            equipment_slot_name(EquipmentSlot::RingR, Language::En),
            "R. Ring"
        );
        assert_eq!(
            buff_name(BuffType::MagicShield, Language::ZhCn),
            Some("魔法盾")
        );
        assert_eq!(
            buff_name(BuffType::MagicShield, Language::En),
            Some("Magic Shield")
        );
        assert_eq!(buff_name(BuffType::HornedArcherBuff, Language::ZhCn), None);
        assert_eq!(
            required_type_name(RequiredType::Level, Language::ZhCn),
            "等级"
        );
        assert_ne!(grade_color(ItemGrade::Rare), grade_color(ItemGrade::None));
    }

    #[test]
    fn new_ui_labels_have_both_languages() {
        for label in [
            Label::Equipment,
            Label::Equip,
            Label::Use,
            Label::Type,
            Label::Require,
            Label::Durability,
            Label::Stack,
            Label::Attack,
            Label::Defence,
            Label::MagicDefence,
            Label::MagicAttack,
            Label::Taoism,
            Label::Accuracy,
            Label::Agility,
            Label::Totals,
            Label::Buffs,
            Label::Miss,
            Label::Critical,
            Label::WorldHint,
            Label::InventoryTooltipHint,
            Label::Minimap,
            Label::NoMinimap,
            Label::MinimapHint,
            Label::NoBigmap,
            Label::BigmapHint,
        ] {
            let zh = label.text(Language::ZhCn);
            let en = label.text(Language::En);
            assert!(!zh.is_empty(), "{label:?} 缺中文");
            assert!(!en.is_empty(), "{label:?} 缺英文");
            assert_ne!(zh, en, "{label:?} 中英文不应相同");
        }
    }
}
