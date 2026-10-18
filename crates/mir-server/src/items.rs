//! 物品系统：模板查找、地面物品、拾取 / 丢弃 / 使用 / 丢金币 / 背包交换 / 装备穿戴、
//! 持久化编解码与 UUID。
//!
//! 规则逐条对照 Crystal（参考根目录 `github\Crystal`，下文只写相对路径）：
//!
//! | 本模块 | Crystal |
//! |---|---|
//! | [`ItemUidGenerator`] | `Envir.NextUserItemID`（`Server/MirEnvir/Envir.cs:127,4394,4413`）与 `Envir.CreateDropItem`（`:4407-4424`） |
//! | [`GroundItem`] | `ItemObject`（`Server/MirObjects/ItemObject.cs:39-127`） |
//! | `World::drop_cell` | `ItemObject.Drop(distance)`（`ItemObject.cs:190-275`） |
//! | `World::drop_monster_loot` | `MonsterObject.Drop` / `DropItem` / `DropGold`（`MonsterObject.cs:1093-1190`） |
//! | `World::pickup` | `PlayerObject.PickUp`（`Server/MirObjects/PlayerObject.cs:7483-7537`） |
//! | `World::drop_item` | `PlayerObject.DropItem`（`:7359-7472`）+ `HumanObject.DropItem`（`HumanObject.cs:1380-1390`） |
//! | [`World::drop_gold`] | `PlayerObject.DropGold`（`:7473-7482`） |
//! | [`World::use_item`] | `PlayerObject.UseItem`（`:5799-6338`） |
//! | [`World::can_use_requirements`] | `HumanObject.CanUseItem`（`HumanObject.cs:1020-1235`） |
//! | `World::move_item`（背包） | `PlayerObject.MoveItem`（`:5350-5374`） |
//! | `World::move_item`（装备） | `PlayerObject.EquipItem`（`:5527-5693`）+ `PlayerObject.RemoveItem`（`:5013-5135`）+ `HumanObject.CanEquipItem`（`HumanObject.cs:7661-7823`） |
//! | `World::refresh_stats` | `HumanObject.RefreshStats` / `RefreshEquipmentStats`（`HumanObject.cs:1730-1936`） |
//! | `World::can_gain_item` | `HumanObject.CanGainItem`（`HumanObject.cs:7586-7628`） |
//! | `World::add_item_to_inventory` | `HumanObject.AddItem`（`HumanObject.cs:1577-1630`） |
//! | [`encode_slots`] / [`decode_slots`] | `CharacterInfo.Save`（`Server/MirDatabase/CharacterInfo.cs:431-456`）与读入（`:190-224`） |
//!
//! 使用物品（药水/卷轴/技能书）与增益的细节见 `docs/world/items.md` §15 与
//! `docs/world/buffs.md`。
//!
//! # 与 Crystal 的**有意**差异（都在函数上写了 `// 扩展：` 或 `// 简化：`）
//!
//! 1. **拾取时检查负重**：Crystal 的 `CanGainItem` 只检查背包空位与可堆叠，重量只在
//!    「能不能跑」里起作用（`HumanObject.cs:135,2516`）。本作按需求在拾取时也拒绝超重，
//!    可用 [`World::set_enforce_bag_weight`] 关掉。
//! 2. **地面物品的存活时间**：Crystal `Settings.ItemTimeOut = 30` 分钟（`Settings.cs:115`），
//!    本作默认 3 分钟（[`ITEM_EXPIRE_MS`]），可用 [`World::set_item_expire_ms`] 调整。
//! 3. **掉落落点**：Crystal 是**确定性**的环形扫描（`ItemObject.Drop`），不是随机偏移；
//!    本作照抄扫描逻辑（见 `World::drop_cell` 的说明），没有引入随机数。
//! 4. **满血/满蓝喝药水直接拒绝**（[`ItemError::PotionNoEffect`]）：Crystal 照喝，
//!    `PotHealthAmount` 随后在 `ProcessRegen` 里被清零（`HumanObject.cs:641-645`），等于白喝。
//! 5. **未实现的卷轴/食物拒绝且不消耗**：Crystal 的内层 `switch` 没有 `default`，
//!    不认识的花色会被静默吃掉（`PlayerObject.cs:5826-6075`）。
//! 6. **玩家丢 0 金币不落地**：Crystal 会生成一堆 0 金币（`PlayerObject.cs:7475-7479`）。
//!
//! # 掉落物的 UUID
//!
//! Crystal 用全局自增的 `Envir.NextUserItemID`（`Envir.cs:127`），并把它写在**账号库**的
//! 文件头里（`Envir.cs:2579` 写、`:2975` 读），因此重启后仍单调。本作的角色库没有这个
//! 全局计数器，于是用 [`ItemUidGenerator`]：`AtomicU64` 自增，起点取「Unix 毫秒 << 20」
//! 或在登录时按数据库里已存在的最大 UUID 往上抬（[`World::observe_item_uid`]）。
//! 差异：Crystal 的 ID 是 1、2、3… 的紧凑序列，本作是从时间戳起跳的大数——只要保证
//! **不重复**且能持久化（UUID 跟着 `UserItem` 存进角色行），对客户端没有可观察差异。

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};

use mir_core::Point;
use mir_core::enums::{
    BindMode, ItemType, MirClass, MirGender, MirGridType, RequiredClass, RequiredGender,
    RequiredType, Stat,
};
use mir_core::{Language, TextKey};
use mir_protocol::data::{ItemInfo, UserItem};
use mir_protocol::packets::server::{
    DeleteItem as DeleteItemPacket, DropItem as DropItemPacket, EquipItem as EquipItemPacket,
    GainedGold, GainedItem, LoseGold, MoveItem as MoveItemPacket, ObjectGold, ObjectItem,
    PlayerUpdate, RemoveItem as RemoveItemPacket, UseItem as UseItemPacket,
};
use mir_protocol::{PacketReader, PacketWriter, ProtocolError, ServerPacket};

use crate::aoi::AoiKind;
use crate::buffs::{BuffKind, minutes_to_ms};
use crate::combat::{self, CombatStats};
use crate::gamedata::GameData;
use crate::magic::{MagicError, UserMagic};
use crate::world::{MINUTE_MS, VIEW_RANGE, World};

/// 背包格数：Crystal `CharacterInfo.Inventory = new UserItem[46]`（`CharacterInfo.cs:75`）。
///
/// 任务书里写的是「40 格」，那是 Crystal 的**任务物品栏**（`QuestInventory = new UserItem[40]`，
/// 同一行）；真正的背包是 46 格，客户端也是 46（`Client/MirObjects/UserObject.cs:37`）。
pub const INVENTORY_SIZE: usize = 46;
/// 装备槽数：Crystal `EquipmentSlot` 共 14 个（`Shared/Enums.cs:954-970`，`Weapon` … `Mount`）。
pub const EQUIPMENT_SLOTS: usize = 14;
/// 背包里「腰带区」的结束下标：Crystal `HumanObject.BeltSize => 6`（`HumanObject.cs:182`）。
pub const BELT_SIZE: usize = 6;
/// 药水优先落位区间上界：Crystal `PotionBeltMaximum => 4`（`HumanObject.cs:179`）。
pub const POTION_BELT_MAX: usize = 4;
/// 护身符优先落位区间：`AmuletBeltMinimum => 4`、`AmuletBeltMaximum => 6`（`HumanObject.cs:180-181`）。
pub const AMULET_BELT_MIN: usize = 4;
/// 见 [`AMULET_BELT_MIN`]。
pub const AMULET_BELT_MAX: usize = 6;
/// 背包最多能扩到多少格：Crystal `CharacterInfo.ResizeInventory` 的上限（`CharacterInfo.cs:596`）。
///
/// 本作**还没有**扩容背包的功能，这里只用来给解码器一个上界。
pub const MAX_INVENTORY_SIZE: usize = 86;
/// 地面物品存活时间：**本作默认 3 分钟**。
///
/// Crystal 的对应物是 `Settings.ItemTimeOut = 30`（分钟，`Settings.cs:115`），
/// `ItemObject` 构造函数里 `ExpireTime = Envir.Time + Settings.ItemTimeOut * Settings.Minute`
/// （`ItemObject.cs:56-58`、`:85`、`:112`、`:121`）。3 分钟是本项目的调试友好默认值。
pub const ITEM_EXPIRE_MS: i64 = 3 * MINUTE_MS;
/// 玩家死亡掉落物的存活时间：Crystal `Settings.PlayerDiedItemTimeOut = 120` 分钟
/// （`Server/Settings.cs:116`），`ItemObject` 的 `deathDrop` 构造用它
/// （`Server/MirObjects/ItemObject.cs:53-56`）。
///
/// 这个值**不**跟随 [`World::set_item_expire_ms`]：Crystal 的死亡掉落与普通掉落是两套时限。
pub const PLAYER_DIED_ITEM_EXPIRE_MS: i64 = 120 * MINUTE_MS;
/// 掉落物的归属保护期：Crystal `OwnerTime = Envir.Time + Settings.Minute`
/// （`MonsterObject.cs:1152-1156`）；过期后 `ItemObject.Process` 清空 `Owner`（`ItemObject.cs:138-139`）。
pub const GROUND_ITEM_OWNER_MS: i64 = MINUTE_MS;
/// 掉落落点的搜索半径：Crystal `Settings.DropRange = 4`（`Settings.cs:117`），
/// 由 `MonsterObject.DropItem` 传给 `ItemObject.Drop(Settings.DropRange)`（`MonsterObject.cs:1159`）。
pub const DROP_RANGE: i32 = 4;
/// 同一格最多叠几个地面物品：Crystal `Settings.DropStackSize = 5`（`Settings.cs:118`）。
pub const DROP_STACK_SIZE: usize = 5;
/// 单堆金币上限：Crystal `Settings.MaxDropGold = 2000`（`Settings.cs:228`）。
pub const MAX_DROP_GOLD: u32 = 2000;
/// 丢金币时的落地搜索半径：Crystal `ob.Drop(5)`（`PlayerObject.cs:7479`）。
///
/// 注意它与掉落物品的 [`DROP_RANGE`]（`Settings.DropRange = 4`）不是同一个数——
/// 玩家丢金币是写死的 5。
pub const DROP_GOLD_RANGE: i32 = 5;
/// 全局掉落率倍率：Crystal `Settings.DropRate = 1F`（`Settings.cs:112`）。
pub const DROP_RATE: u32 = 1;

/// 一件掉在地上的物品（Crystal `ItemObject`，`Server/MirObjects/ItemObject.cs:8-127`）。
///
/// Crystal 的 `ItemObject` 同时有 `Item` 与 `Gold` 两个字段，但**永远只有一个有效**
/// （`ItemObject.cs:39-40`：金币构造只填 `Gold`，物品构造只填 `Item`）。
/// 本结构照抄这个约定：`item.is_none()` 就表示这是一堆金币，金额在 [`Self::gold`]。
#[derive(Debug, Clone, PartialEq)]
pub struct GroundItem {
    /// 对象 ID（与玩家、怪物共用同一个 ID 空间，Crystal `Envir.Objects` 也是这样）。
    pub object_id: u32,
    /// 物品实例；`None` 表示金币堆。
    pub item: Option<UserItem>,
    /// 金币数量；物品堆恒为 0。
    pub gold: u32,
    /// 所在格。
    pub position: Point,
    /// 地图名。
    pub map: String,
    /// 消失时刻（毫秒，服务器单调时钟）。Crystal `ItemObject.ExpireTime`。
    pub expire_time: i64,
    /// 归属玩家（怪物掉落时是 `EXPOwner`，Crystal `ItemObject.Owner`）。
    pub owner: Option<u32>,
    /// 归属保护截止时刻（Crystal `ItemObject.OwnerTime`）。
    pub owner_time: i64,
}

impl GroundItem {
    /// 是否是金币堆。
    #[must_use]
    pub fn is_gold(&self) -> bool {
        self.item.is_none()
    }

    /// 客户端看到的显示名：Crystal `ItemObject.GetInfo` 里
    /// `Name = Count > 1 ? $"{Name} ({Count})" : Name`（`ItemObject.cs:371`）。
    ///
    /// 名字本身来自模板（`ItemInfo.Name`），所以这里需要一份模板表；由
    /// [`World::ground_item_frame`] 传入。
    #[must_use]
    pub fn display_name(info_name: &str, count: u16) -> String {
        if count > 1 {
            format!("{info_name} ({count})")
        } else {
            info_name.to_owned()
        }
    }
}

/// 物品系统错误。
///
/// 所有「操作被拒绝」的路径都返回它；调用方（[`crate::session`]）负责记日志，
/// **失败包**已经在函数内部按 Crystal 的位置发过了。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ItemError {
    /// 对象不在世界里。
    #[error("对象 {0} 不在世界里")]
    NotInWorld(u32),
    /// 对象已死亡（Crystal `PickUp`/`DropItem` 的第一道门，`PlayerObject.cs:7485-7489`、`:7362-7366`）。
    #[error("对象 {object_id} 已死亡")]
    Dead {
        /// 对象 ID。
        object_id: u32,
    },
    /// 脚下没有可拾取的地面物品。
    #[error("脚下没有可拾取的地面物品")]
    NoGroundItem,
    /// 地面物品属于别人（Crystal `CannotPickupNotOwner`，`PlayerObject.cs:7501-7505`）。
    #[error("地面物品属于玩家 {owner}")]
    NotOwner {
        /// 归属玩家。
        owner: u32,
    },
    /// 背包没有空位（Crystal `CanGainItem` 为假，`HumanObject.cs:7586-7628`）。
    #[error("背包已满")]
    InventoryFull,
    /// 背包超重（**本作扩展**，Crystal 拾取时不查重量）。
    #[error("超重：{weight} > 上限 {limit}")]
    TooHeavy {
        /// 拾取后的背包总重。
        weight: u32,
        /// 负重上限（`Stat.BagWeight`）。
        limit: u32,
    },
    /// 背包里找不到这个 UUID 的物品。
    #[error("背包里没有 UUID {unique_id} 的物品")]
    ItemNotFound {
        /// 物品 UUID。
        unique_id: u64,
    },
    /// 数量非法（Crystal `count < 1 || count > item.Count`，`PlayerObject.cs:7410-7414`）。
    #[error("数量非法：{count}（持有 {available}）")]
    BadCount {
        /// 请求数量。
        count: u16,
        /// 实际持有数量。
        available: u16,
    },
    /// 下标越界。
    #[error("{grid:?} 的下标 {index} 越界")]
    SlotOutOfRange {
        /// 网格。
        grid: MirGridType,
        /// 下标。
        index: i32,
    },
    /// 格子是空的。
    #[error("{grid:?} 的 {index} 号格是空的")]
    EmptySlot {
        /// 网格。
        grid: MirGridType,
        /// 下标。
        index: i32,
    },
    /// 目标格已经被占用（装备脱下到有物品的背包格）。
    #[error("{grid:?} 的 {index} 号格已被占用")]
    SlotOccupied {
        /// 网格。
        grid: MirGridType,
        /// 下标。
        index: i32,
    },
    /// 物品不能装备到这个槽（Crystal `CanEquipItem` 返回 false）。
    #[error("物品 {item_index} 不能装备到 {slot} 号槽")]
    CannotEquip {
        /// 物品模板编号。
        item_index: i32,
        /// 装备槽。
        slot: i32,
    },
    /// 物品模板不存在（掉落/拾取时数据缺失）。
    #[error("物品模板 {0} 不存在")]
    UnknownItem(i32),
    /// 物品不允许丢弃（Crystal `BindMode.DontDrop`，`PlayerObject.cs:7416-7420`）。
    #[error("物品 {unique_id} 不允许丢弃")]
    CannotDrop {
        /// 物品 UUID。
        unique_id: u64,
    },
    /// 不支持的操作网格。
    #[error("不支持的物品网格 {0:?}")]
    UnsupportedGrid(MirGridType),
    /// 附近找不到可放置的空格（Crystal `ItemObject.Drop` 返回 false）。
    #[error("附近没有可放置的空格")]
    NoDropCell,
    /// 金币数量溢出（Crystal `CanGainGold`，`PlayerObject.cs:7638-7641`）。
    #[error("金币数量溢出")]
    GoldOverflow,
    /// 金币不够（Crystal `if (Account.Gold < gold) return;`，`PlayerObject.cs:7475`）。
    #[error("金币不够：想丢 {requested}，只有 {held}")]
    NotEnoughGold {
        /// 想丢的数量。
        requested: u32,
        /// 实际持有。
        held: u32,
    },
    /// 性别不符（Crystal `HumanObject.CanUseItem`，`HumanObject.cs:1024-1040`）。
    #[error("性别不符")]
    WrongGender,
    /// 职业不符（`HumanObject.cs:1042-1079`）。
    #[error("职业不符：需要 {required:?}，当前 {class:?}")]
    WrongClass {
        /// 物品要求的职业位。
        required: RequiredClass,
        /// 玩家职业。
        class: MirClass,
    },
    /// 等级不满足 `RequiredType::Level` / `MaxLevel`（`HumanObject.cs:1081-1131`）。
    #[error("等级不符：要求 {required} 级，当前 {level} 级")]
    LowLevel {
        /// 要求等级。
        required: u8,
        /// 当前等级。
        level: u16,
    },
    /// 满血/满蓝时喝药水。
    ///
    /// **本作扩展**：Crystal 在满血时照样把药水喝掉（`PotHealthAmount` 会在下一次
    /// `ProcessRegen` 里被清零，`HumanObject.cs:641-645`），等于白白浪费一瓶；
    /// 这里直接拒绝并提示，见 `docs/world/items.md` 的「与 Crystal 的差异」。
    #[error("药水没有效果：生命/魔法已满")]
    PotionNoEffect,
    /// 本作还不支持的物品类型/形状（传送卷、马粮、变身卷…）。
    #[error("物品 {item_index}（shape {shape}）的效果尚未实现")]
    UnsupportedItem {
        /// 物品模板编号。
        item_index: i32,
        /// 物品形状。
        shape: i16,
    },
    /// 用书失败（转自技能系统的 [`MagicError`]）。
    #[error("用书失败：{0}")]
    MagicBook(#[from] MagicError),
}

/// 玩家的性别位：Crystal `RequiredGender` 的 `Male = 1`、`Female = 2`
/// （`Shared/Enums.cs`；`CanUseItem` 用 `HasFlag` 判定，`HumanObject.cs:1024-1040`）。
pub(crate) const fn gender_flag(gender: MirGender) -> u8 {
    match gender {
        MirGender::Male => 1,
        MirGender::Female => 2,
    }
}

/// 玩家职业对应的 `RequiredClass` 位（`Warrior=1, Wizard=2, Taoist=4, Assassin=8, Archer=16`）。
pub(crate) const fn class_flag(class: MirClass) -> u8 {
    match class {
        MirClass::Warrior => 1,
        MirClass::Wizard => 2,
        MirClass::Taoist => 4,
        MirClass::Assassin => 8,
        MirClass::Archer => 16,
    }
}

/// 药水喝下去到底有没有效果。
///
/// 某一项属性 `<= 0` 视为「这瓶药不管这项」，另一项满了同样算「这一项没效果」，
/// 两项都没效果时返回 `false`（[`ItemError::PotionNoEffect`]）。
///
/// **本作扩展**：Crystal 没有这个判定（满血也照喝，`PlayerObject.cs:5831-5838`），
/// 见 `docs/world/items.md` 的「与 Crystal 的差异」。
fn potion_has_effect(
    hp_amount: i32,
    hp: i32,
    max_hp: i32,
    mp_amount: i32,
    mp: i32,
    max_mp: i32,
) -> bool {
    let hp_useful = hp_amount > 0 && hp < max_hp;
    let mp_useful = mp_amount > 0 && mp < max_mp;
    hp_useful || mp_useful
}

/// `item.GetTotal(stat)`：模板属性 + 物品的附加属性（`Shared/Data/ItemData.cs:170-180`）。
fn item_total(info: &ItemInfo, item: &UserItem, stat: Stat) -> i32 {
    info.stats.get(stat) + item.added_stats.get(stat)
}

/// 药水增益（`ItemInfo.Shape == 3`）要套哪些 buff。
///
/// 逐条对照 `PlayerObject.cs:5849-5879`：每个「属性组」只在组内任一属性 `> 0` 时添加，
/// 顺序就是源码里的书写顺序（这决定了 `S.AddBuff` 的先后）。
fn buff_potion_groups(
    info: &ItemInfo,
    item: &UserItem,
) -> Vec<(BuffKind, mir_protocol::data::Stats)> {
    use mir_protocol::data::Stats;

    let mut groups: Vec<(BuffKind, Stats)> = Vec::new();
    let mut group = |kind: BuffKind, pairs: &[(Stat, i32)]| {
        let mut stats = Stats::default();
        for &(stat, value) in pairs {
            stats.set(stat, value);
        }
        groups.push((kind, stats));
    };

    // `if (item.GetTotal(Stat.MaxDC) > 0 || item.GetTotal(Stat.MinDC) > 0) AddBuff(BuffType.Impact, …)`
    let max_dc = item_total(info, item, Stat::MaxDC);
    let min_dc = item_total(info, item, Stat::MinDC);
    if max_dc > 0 || min_dc > 0 {
        group(
            BuffKind::Impact,
            &[(Stat::MaxDC, max_dc), (Stat::MinDC, min_dc)],
        );
    }
    let max_mc = item_total(info, item, Stat::MaxMC);
    let min_mc = item_total(info, item, Stat::MinMC);
    if max_mc > 0 || min_mc > 0 {
        group(
            BuffKind::Magic,
            &[(Stat::MaxMC, max_mc), (Stat::MinMC, min_mc)],
        );
    }
    let max_sc = item_total(info, item, Stat::MaxSC);
    let min_sc = item_total(info, item, Stat::MinSC);
    if max_sc > 0 || min_sc > 0 {
        group(
            BuffKind::Taoist,
            &[(Stat::MaxSC, max_sc), (Stat::MinSC, min_sc)],
        );
    }
    let attack_speed = item_total(info, item, Stat::AttackSpeed);
    if attack_speed > 0 {
        group(BuffKind::Storm, &[(Stat::AttackSpeed, attack_speed)]);
    }
    let hp = item_total(info, item, Stat::HP);
    if hp > 0 {
        group(BuffKind::HealthAid, &[(Stat::HP, hp)]);
    }
    let mp = item_total(info, item, Stat::MP);
    if mp > 0 {
        group(BuffKind::ManaAid, &[(Stat::MP, mp)]);
    }
    let max_ac = item_total(info, item, Stat::MaxAC);
    let min_ac = item_total(info, item, Stat::MinAC);
    if max_ac > 0 || min_ac > 0 {
        group(
            BuffKind::Defence,
            &[(Stat::MaxAC, max_ac), (Stat::MinAC, min_ac)],
        );
    }
    let max_mac = item_total(info, item, Stat::MaxMAC);
    let min_mac = item_total(info, item, Stat::MinMAC);
    if max_mac > 0 || min_mac > 0 {
        group(
            BuffKind::MagicDefence,
            &[(Stat::MaxMAC, max_mac), (Stat::MinMAC, min_mac)],
        );
    }
    let bag_weight = item_total(info, item, Stat::BagWeight);
    if bag_weight > 0 {
        group(BuffKind::BagWeight, &[(Stat::BagWeight, bag_weight)]);
    }
    groups
}

/// 物品在客户端上的显示名：Crystal `ItemInfo.FriendlyName`（`Shared/Data/ItemData.cs:52-62`）。
///
/// Crystal 用两条正则加工模板名：
/// * `\d+$` → 去掉**结尾的连续数字**（例如 `金创药(小量)10` → `金创药(小量)`）；
/// * `\[[^\]]*\]` → 删掉所有方括号段（例如 `[7天]屠龙` → `屠龙`）。
///
/// 这只是显示层，物品查找仍然用原始 `Name`（`Envir.GetItemInfo(string)`，`Envir.cs:4745-4754`）。
/// 简化：`\d` 在 .NET 里匹配 Unicode 十进制数字，这里只去 ASCII 数字——传奇的物品名里
/// 结尾数字都是 ASCII。
#[must_use]
pub fn friendly_name(name: &str) -> String {
    let mut temp = name
        .trim_end_matches(|ch: char| ch.is_ascii_digit())
        .to_owned();
    while let Some(start) = temp.find('[') {
        let Some(offset) = temp[start..].find(']') else {
            break;
        };
        temp.replace_range(start..=start + offset, "");
    }
    temp
}

/// 物品图标编号：Crystal `UserItem.Image`（`Shared/Data/ItemData.cs:641-677`）。///
/// 绝大多数物品就是模板的 `Image`；只有**护身符/毒药**按数量换图标（`:648-670`）。
#[must_use]
pub fn item_image(info: &ItemInfo, count: u16) -> u16 {
    if info.item_type == ItemType::Amulet && info.stack_size > 0 {
        match info.shape {
            // `Shape == 0`：护身符
            0 => match count {
                300.. => 3662,
                200..=299 => 3661,
                _ => 3660,
            },
            // `Shape == 1`：灰毒
            1 => match count {
                150.. => 3675,
                100..=149 => 2960,
                50..=99 => 3674,
                _ => 3673,
            },
            // `Shape == 2`：黄毒
            2 => match count {
                150.. => 3672,
                100..=149 => 2961,
                50..=99 => 3671,
                _ => 3670,
            },
            _ => info.image,
        }
    } else {
        info.image
    }
}

/// 一次拾取的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct PickupReport {
    /// 被拾取的地面物品对象 ID。
    pub ground: u32,
    /// 拾到的物品（金币时为 `None`）。
    pub item: Option<UserItem>,
    /// 拾到的金币。
    pub gold: u32,
}

/// 一次「使用物品」的结果（Crystal `PlayerObject.UseItem` 的可观察效果）。
///
/// 所有字段都是**已经发生**的效果：调用方（会话层 / 测试）只需要记录或断言，
/// 不需要再发包——包已经在 [`World::use_item`] 内部按 Crystal 的顺序发完了。
#[derive(Debug, Clone, PartialEq)]
pub struct UseItemReport {
    /// 被使用的物品 UUID。
    pub unique_id: u64,
    /// 物品模板编号。
    pub item_index: i32,
    /// 物品名（模板里的原始名）。
    pub item_name: String,
    /// 立即回复的生命（太阳水一类的 `Shape == 1`）。
    pub hp: i32,
    /// 立即回复的魔法。
    pub mp: i32,
    /// 累加进「药水回复池」的生命（`Shape == 0`，随后由 `ProcessRegen` 分批放出）。
    pub pot_health: u32,
    /// 累加进「药水回复池」的魔法。
    pub pot_mana: u32,
    /// 本次套上的增益（药水增益，按 Crystal 的 `AddBuff` 顺序）。
    pub buffs: Vec<BuffKind>,
    /// 若是技能书，这里带着学会的技能。
    pub learned: Option<UserMagic>,
    /// 消耗一件之后这一格的剩余内容（`None` = 整格清空）。
    pub remaining: Option<UserItem>,
}

/// 角色外观里随装备变化的那部分（Crystal `HumanObject.Looks_*`，`HumanObject.cs:164-165`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Looks {
    /// `Looks_Armour`：盔甲外观编号（`ObjectPlayer.Armour`）。
    pub armour: i16,
    /// `Looks_Weapon`：武器外观编号，初始 -1 表示空手。
    pub weapon: i16,
    /// `Looks_WeaponEffect`：武器特效编号。
    pub weapon_effect: i16,
    /// `Looks_Wings`：翅膀特效（穿盔甲时取 `Effect`）。
    pub wings: u8,
    /// 自带光照。
    pub light: u8,
}

impl Default for Looks {
    /// Crystal 的字段初值就是 `Looks_Armour = 0, Looks_Weapon = -1, Looks_WeaponEffect = 0`
    /// （`HumanObject.cs:164`），派生 `Default` 会得到 0，所以手写。
    fn default() -> Self {
        Self {
            armour: 0,
            weapon: -1,
            weapon_effect: 0,
            wings: 0,
            light: 0,
        }
    }
}

/// 物品 UUID 发生器（Crystal `Envir.NextUserItemID`，`Envir.cs:127`）。
///
/// Crystal 是 `UniqueID = ++NextUserItemID`（`Envir.cs:4394`、`:4413`），计数器从 0 起、
/// 存在账号库文件头里（`:2579`/`:2975`）。本作没有那份全局文件，所以：
///
/// * 起点可以是「Unix 毫秒 << 20」——跨进程重启也不会撞号；
/// * 也可以在登录读档后用 [`ItemUidGenerator::observe`] 把数据库里的最大值抬上来；
/// * `fetch_add(1) + 1` 保证同一进程内严格递增（对应 Crystal 的 `++`）。
#[derive(Debug)]
pub struct ItemUidGenerator {
    next: AtomicU64,
}

impl ItemUidGenerator {
    /// 从给定值之后开始发号（`new(0)` 等价于 Crystal 的「第一个 ID 是 1」）。
    #[must_use]
    pub const fn new(start: u64) -> Self {
        Self {
            next: AtomicU64::new(start),
        }
    }

    /// 用「Unix 毫秒 << 20」当起点。
    ///
    /// 低 20 位留给同一毫秒内的自增，按每秒 100 万件物品的极端速率也要 1048 秒才会撞到
    /// 下一个毫秒段的起点，实际不可能发生。系统时间早于 1970 年时退化成 0（仍可用）。
    #[must_use]
    pub fn from_timestamp() -> Self {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis() as u64);
        Self::new(millis.saturating_mul(1 << 20))
    }

    /// 分配下一个 UUID。
    pub fn next(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed).saturating_add(1)
    }

    /// 把「已经用掉的 UUID」计入，保证后续发号不会撞上它（读档时调用）。
    pub fn observe(&self, used: u64) {
        let mut current = self.next.load(Ordering::Relaxed);
        while used > current {
            match self.next.compare_exchange_weak(
                current,
                used,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(actual) => current = actual,
            }
        }
    }

    /// 当前已经发到的最大值（下一个会分到 `peek() + 1`）。
    #[must_use]
    pub fn peek(&self) -> u64 {
        self.next.load(Ordering::Relaxed)
    }
}

impl Default for ItemUidGenerator {
    fn default() -> Self {
        Self::from_timestamp()
    }
}

/// 把 `Stat` 累加进 [`CombatStats`]；本作没有建模的属性直接忽略。
///
/// Crystal 里这是一次 `Stats.Add(realItem.Stats)`（`HumanObject.cs:1858`）——
/// 那个 `Stats` 是「所有属性的字典」，本作只建模了参与战斗/负重的那部分。
///
/// `pub(crate)`：行会增益（`crate::guild_buffs`）复用同一张映射表。
pub(crate) fn add_stat(target: &mut CombatStats, stat: Stat, value: i32) {
    match stat {
        Stat::HP => target.hp += value,
        Stat::MP => target.mp += value,
        Stat::MinAC => target.min_ac += value,
        Stat::MaxAC => target.max_ac += value,
        Stat::MinMAC => target.min_mac += value,
        Stat::MaxMAC => target.max_mac += value,
        Stat::MinDC => target.min_dc += value,
        Stat::MaxDC => target.max_dc += value,
        Stat::MinMC => target.min_mc += value,
        Stat::MaxMC => target.max_mc += value,
        Stat::MinSC => target.min_sc += value,
        Stat::MaxSC => target.max_sc += value,
        Stat::Accuracy => target.accuracy += value,
        Stat::Agility => target.agility += value,
        Stat::Luck => target.luck += value,
        Stat::AttackSpeed => target.attack_speed += value,
        Stat::CriticalRate => target.critical_rate += value,
        Stat::CriticalDamage => target.critical_damage += value,
        Stat::MagicResist => target.magic_resist += value,
        Stat::Reflect => target.reflect += value,
        Stat::BagWeight => target.bag_weight += value,
        Stat::WearWeight => target.wear_weight += value,
        Stat::HandWeight => target.hand_weight += value,
        // 增益类属性（魔法盾/气功波/魔力增幅）：Crystal 的 `Stats` 字典里同样是普通键，
        // 由 `Attacked`/`MagicCost` 读取（`HumanObject.cs:7245`、`:7262`、`:3387`）。
        Stat::DamageReductionPercent => target.damage_reduction_percent += value,
        Stat::EnergyShieldPercent => target.energy_shield_percent += value,
        Stat::EnergyShieldHPGain => target.energy_shield_hp_gain += value,
        Stat::ManaPenaltyPercent => target.mana_penalty_percent += value,
        // 简化：毒/冰冻/圣/吸血/上限百分比等属性尚未进入战斗结算，叠上去也观察不到。
        _ => {}
    }
}

/// 把一件装备的属性叠到 `target` 上（Crystal `RefreshEquipmentStats` 的内循环，
/// `HumanObject.cs:1825-1915`）。
///
/// 照抄的部分：耐久为 0 且模板有耐久的装备**不提供属性**（`:1836`），
/// 模板属性与 `AddedStats` 都要叠（`:1858-1859`）。
///
/// 简化：`Awake`（觉醒）属性（`:1861-1874`）、套装/套装加成（`:1893-1914`、`RefreshItemSetStats`）、
/// 镶嵌宝石（`RefreshSocketStats`）、职业/等级相关属性（`Functions.GetRealItem` 的
/// `ClassBased`/`LevelBased` 替换）都未实现——本作还没有这些系统。
pub fn apply_equipment_stats(target: &mut CombatStats, info: &ItemInfo, item: &UserItem) {
    if item.current_dura == 0 && info.durability > 0 {
        return;
    }
    for (&stat, &value) in info.stats.values() {
        add_stat(target, stat, value);
    }
    for (&stat, &value) in item.added_stats.values() {
        add_stat(target, stat, value);
    }
}

/// 物品在背包里占的重量：Crystal `UserItem.Weight`（`Shared/Data/ItemData.cs:317-320`）。
///
/// 箭袋/鱼饵按**件**计重（不乘数量），其余按 `Weight * Count`。
#[must_use]
pub fn item_weight(info: &ItemInfo, item: &UserItem) -> i32 {
    let count = i32::from(item.count);
    match info.item_type {
        ItemType::Amulet | ItemType::Bait => i32::from(info.weight),
        _ => i32::from(info.weight) * count,
    }
}

/// 物品类型能否放进这个装备槽（Crystal `HumanObject.CanEquipItem` 的第一个 `switch`，
/// `HumanObject.cs:7663-7721`）。
///
/// 注意两处 Crystal 的「同一个槽收两种类型」：
/// * `BraceletR` 收 `Bracelet` **和** `Amulet`（`:7689-7692`）；
/// * `RingL`/`RingR` 都收 `Ring`（`:7693-7697`）。
#[must_use]
pub fn slot_accepts(item_type: ItemType, slot: usize) -> bool {
    match slot {
        // EquipmentSlot::Weapon = 0
        0 => item_type == ItemType::Weapon,
        // Armour = 1
        1 => item_type == ItemType::Armour,
        // Helmet = 2
        2 => item_type == ItemType::Helmet,
        // Torch = 3
        3 => item_type == ItemType::Torch,
        // Necklace = 4
        4 => item_type == ItemType::Necklace,
        // BraceletL = 5
        5 => item_type == ItemType::Bracelet,
        // BraceletR = 6
        6 => item_type == ItemType::Bracelet || item_type == ItemType::Amulet,
        // RingL = 7 / RingR = 8
        7 | 8 => item_type == ItemType::Ring,
        // Amulet = 9
        9 => item_type == ItemType::Amulet,
        // Belt = 10
        10 => item_type == ItemType::Belt,
        // Boots = 11
        11 => item_type == ItemType::Boots,
        // Stone = 12
        12 => item_type == ItemType::Stone,
        // Mount = 13
        13 => item_type == ItemType::Mount,
        _ => false,
    }
}

/// 物品类型的**默认**装备槽（供文档/测试使用）。
///
/// Crystal 里槽位是**客户端指定**的（`C.EquipItem.To` / `C.RemoveItem.To`，
/// `Server/MirNetwork/MirConnection.cs:1297-1307`），服务端只用 [`slot_accepts`] 校验，
/// 所以「双戒指」「双护腕」这类槽位由玩家自己挑。这张表只是「第一次点装备时该去哪个槽」。
#[must_use]
pub fn equipment_slot_for(item_type: ItemType) -> Option<usize> {
    match item_type {
        ItemType::Weapon => Some(0),
        ItemType::Armour => Some(1),
        ItemType::Helmet => Some(2),
        ItemType::Torch => Some(3),
        ItemType::Necklace => Some(4),
        ItemType::Bracelet => Some(5),
        ItemType::Ring => Some(7),
        ItemType::Amulet => Some(9),
        ItemType::Belt => Some(10),
        ItemType::Boots => Some(11),
        ItemType::Stone => Some(12),
        ItemType::Mount => Some(13),
        _ => None,
    }
}

/// 持久化编解码错误。
#[derive(Debug, thiserror::Error)]
pub enum ItemCodecError {
    /// 协议层错误（读到文件尾、计数非法、字符串非法……）。
    #[error("物品块解析失败：{0}")]
    Protocol(#[from] ProtocolError),
    /// 槽位数量超过本作支持的上限。
    #[error("物品块的槽位数 {count} 超过上限 {limit}")]
    TooManySlots {
        /// 声明/解出的槽位数。
        count: usize,
        /// 允许的上限。
        limit: usize,
    },
}

/// 空背包。
#[must_use]
pub fn new_inventory() -> Vec<Option<UserItem>> {
    vec![None; INVENTORY_SIZE]
}

/// 空装备栏。
#[must_use]
pub fn new_equipment() -> Vec<Option<UserItem>> {
    vec![None; EQUIPMENT_SLOTS]
}

/// 编码 Crystal 的 `UserItem[]` 整块：`int32 槽位数` + 每槽 `bool 有物品?` + `UserItem`。
///
/// 逐字对照 `CharacterInfo.Save`（`CharacterInfo.cs:431-447`）：
/// 空槽只写一个 `false`，有物品时先写 `true` 再写 `UserItem.Save`。
#[must_use]
pub fn encode_slots(slots: &[Option<UserItem>]) -> Vec<u8> {
    let mut writer = PacketWriter::with_capacity(slots.len() * 8 + 4);
    writer.write_count(slots.len());
    for slot in slots {
        writer.write_bool(slot.is_some());
        if let Some(item) = slot {
            item.encode(&mut writer);
        }
    }
    writer.into_inner()
}

/// 解码 Crystal 的 `UserItem[]` 整块，见 [`encode_slots`]。
///
/// 对照 `CharacterInfo` 的读入（`CharacterInfo.cs:190-213`）：
/// 先读 `int32 count`，再按 `count` 读 `bool + UserItem`。
///
/// 与 Crystal 的差异：Crystal 读档时会查 `Envir.BindItem` 校验模板是否存在
/// （`:198`、`:209`、`:220`），本作把「模板缺失」留给上层处理（[`World::pickup`] 之类的
/// 地方会跳过），解码器只保证字节层正确。
pub fn decode_slots(bytes: &[u8]) -> Result<Vec<Option<UserItem>>, ItemCodecError> {
    decode_slots_with_limit(bytes, MAX_INVENTORY_SIZE)
}

/// 同 [`decode_slots`]，但允许调用方指定槽位上限。
///
/// 背包/装备用 [`MAX_INVENTORY_SIZE`]（86）；仓库的存档可能是扩建后的 160 格
/// （Crystal `AccountInfo.ExpandStorage`，`AccountInfo.cs:266-275`），
/// 因此 [`crate::storage::decode_storage`] 用 160 的上限解码、再裁到 80。
pub fn decode_slots_with_limit(
    bytes: &[u8],
    limit: usize,
) -> Result<Vec<Option<UserItem>>, ItemCodecError> {
    let mut reader = PacketReader::new(bytes);
    let count = reader.read_count()?;
    if count > limit {
        return Err(ItemCodecError::TooManySlots { count, limit });
    }
    let mut slots = Vec::with_capacity(count);
    for _ in 0..count {
        if !reader.read_bool()? {
            slots.push(None);
            continue;
        }
        slots.push(Some(UserItem::decode(&mut reader)?));
    }
    Ok(slots)
}

/// 把解出来的槽位补齐/截断成固定长度。
///
/// Crystal 的 `Array.Resize(ref Inventory, count)`（`CharacterInfo.cs:192`）允许存档里的背包
/// 比默认长（扩容背包最多 86 格，`:596`）。本作还没有扩容功能，所以：
/// **多的槽位丢掉**（取前 `size` 个），**少的补空**。
#[must_use]
pub fn fit_slots(mut slots: Vec<Option<UserItem>>, size: usize) -> Vec<Option<UserItem>> {
    if slots.len() > size {
        tracing::warn!(
            from = slots.len(),
            to = size,
            "存档里的物品槽位比本作支持的多，超出部分已丢弃（还没有扩容背包）"
        );
        slots.truncate(size);
    }
    slots.resize(size, None);
    slots
}

/// 解码背包字节块并补齐到 [`INVENTORY_SIZE`] 格；空块（新角色）得到空背包。
pub fn decode_inventory(bytes: &[u8]) -> Result<Vec<Option<UserItem>>, ItemCodecError> {
    if bytes.is_empty() {
        return Ok(new_inventory());
    }
    Ok(fit_slots(decode_slots(bytes)?, INVENTORY_SIZE))
}

/// 解码装备字节块并补齐到 [`EQUIPMENT_SLOTS`] 槽；空块得到空装备栏。
pub fn decode_equipment(bytes: &[u8]) -> Result<Vec<Option<UserItem>>, ItemCodecError> {
    if bytes.is_empty() {
        return Ok(new_equipment());
    }
    Ok(fit_slots(decode_slots(bytes)?, EQUIPMENT_SLOTS))
}

impl World {
    /// 装入游戏数据（物品模板 + 掉落表）。生产路径在 `server::build_world` 里调用。
    pub fn set_game_data(&mut self, data: std::sync::Arc<GameData>) {
        self.game_data = Some(data);
    }

    /// 当前游戏数据。
    #[must_use]
    pub fn game_data(&self) -> Option<&std::sync::Arc<GameData>> {
        self.game_data.as_ref()
    }

    /// 按模板编号查物品（Crystal `Envir.GetItemInfo(int)`，`Envir.cs:4734-4743`）。
    #[must_use]
    pub fn item_info(&self, index: i32) -> Option<&ItemInfo> {
        self.game_data.as_ref()?.item(index)
    }

    /// 按名字查物品（Crystal `Envir.GetItemInfo(string)`，`Envir.cs:4745-4754`）。
    #[must_use]
    pub fn item_info_by_name(&self, name: &str) -> Option<&ItemInfo> {
        self.game_data.as_ref()?.item_by_name(name)
    }

    /// 按 UUID 在背包里找 `(下标, 物品)`。
    #[must_use]
    pub fn find_inventory_item(&self, object_id: u32, unique_id: u64) -> Option<(usize, UserItem)> {
        let player = self.players.get(&object_id)?;
        player
            .inventory
            .iter()
            .enumerate()
            .find_map(|(index, slot)| match slot {
                Some(item) if item.unique_id == unique_id => Some((index, item.clone())),
                _ => None,
            })
    }

    /// 地面物品的存活时间（毫秒）。
    #[must_use]
    pub fn item_expire_ms(&self) -> i64 {
        self.item_expire_ms
    }

    /// 设置地面物品的存活时间（毫秒），见 [`ITEM_EXPIRE_MS`]。
    ///
    /// # Panics
    /// 传入非正数时 panic（配置错误应当立刻暴露）。
    pub fn set_item_expire_ms(&mut self, ms: i64) {
        assert!(ms > 0, "地面物品存活时间必须是正数");
        self.item_expire_ms = ms;
    }

    /// 是否在拾取时检查背包负重（**本作扩展**，Crystal 不检查）。
    #[must_use]
    pub fn enforce_bag_weight(&self) -> bool {
        self.enforce_bag_weight
    }

    /// 打开/关闭拾取时的负重检查。
    pub fn set_enforce_bag_weight(&mut self, enforce: bool) {
        self.enforce_bag_weight = enforce;
    }

    /// 物品 UUID 发生器。
    #[must_use]
    pub fn item_uids(&self) -> &ItemUidGenerator {
        &self.item_uids
    }

    /// 把数据库里已经存在的 UUID 计入发生器（登录读档后调用）。
    pub fn observe_item_uid(&self, unique_id: u64) {
        self.item_uids.observe(unique_id);
    }

    /// 把一组物品的 UUID 全部计入发生器。
    pub fn observe_item_uids<'a>(&self, items: impl IntoIterator<Item = &'a UserItem>) {
        for item in items {
            self.item_uids.observe(item.unique_id);
        }
    }

    /// 全部分配一个物品 UUID。
    pub fn allocate_item_uid(&self) -> u64 {
        self.item_uids.next()
    }

    /// 地面物品列表（生成顺序）。
    #[must_use]
    pub fn ground_items(&self) -> &[GroundItem] {
        &self.ground_items
    }

    /// 按对象 ID 取地面物品（用 `ground_item_slots` 下标，避免线性扫描）。
    #[must_use]
    pub fn ground_item(&self, object_id: u32) -> Option<&GroundItem> {
        let index = *self.ground_item_slots.get(&object_id)?;
        self.ground_items
            .get(index)
            .filter(|item| item.object_id == object_id)
    }

    /// 玩家的背包总重（Crystal `HumanObject.CurrentBagWeight`，`HumanObject.cs:1782-1794`）。
    #[must_use]
    pub fn bag_weight(&self, object_id: u32) -> u32 {
        let Some(player) = self.players.get(&object_id) else {
            return 0;
        };
        let mut total = 0i64;
        for item in player.inventory.iter().flatten() {
            let Some(info) = self.item_info(item.item_index) else {
                continue;
            };
            total += i64::from(item_weight(info, item));
        }
        u32::try_from(total.max(0)).unwrap_or(u32::MAX)
    }

    /// 玩家装备的三个重量（手 / 身穿；Crystal `RefreshEquipmentStats` 的
    /// `CurrentHandWeight` / `CurrentWearWeight`，`HumanObject.cs:1831-1834`）。
    ///
    /// 武器与火把算「手」，其余算「身穿」。
    #[must_use]
    pub fn equipment_weights(&self, object_id: u32) -> (i32, i32) {
        let Some(player) = self.players.get(&object_id) else {
            return (0, 0);
        };
        let mut hand = 0;
        let mut wear = 0;
        for item in player.equipment.iter().flatten() {
            let Some(info) = self.item_info(item.item_index) else {
                continue;
            };
            let weight = item_weight(info, item);
            if matches!(info.item_type, ItemType::Weapon | ItemType::Torch) {
                hand += weight;
            } else {
                wear += weight;
            }
        }
        (hand, wear)
    }

    /// 玩家外观（装备决定）：Crystal `Looks_Armour` / `Looks_Weapon` / `Looks_Wings` / `Light`
    /// （`HumanObject.cs:1838-1876`）。
    ///
    /// 简化：只取「最后一个」盔甲与武器的 `Shape`/`Effect`（Crystal 遍历也是覆盖式赋值），
    /// 并且忽略觉醒/镶嵌带来的外观。
    #[must_use]
    pub fn looks(&self, object_id: u32) -> Looks {
        let Some(player) = self.players.get(&object_id) else {
            return Looks::default();
        };
        let mut looks = Looks::default();
        for item in player.equipment.iter().flatten() {
            let Some(info) = self.item_info(item.item_index) else {
                continue;
            };
            if item.current_dura == 0 && info.durability > 0 {
                continue;
            }
            match info.item_type {
                ItemType::Armour => {
                    looks.armour = info.shape;
                    looks.wings = info.effect;
                }
                ItemType::Weapon => {
                    looks.weapon = info.shape;
                    looks.weapon_effect = i16::from(info.effect);
                }
                _ => {}
            }
            looks.light = looks.light.max(info.light);
        }
        looks
    }

    /// 重算玩家属性：基础值（等级/职业曲线）+ 装备加成 + buff 加成 + 行会 buff 加成 + 职业上限。
    ///
    /// 对应 Crystal `HumanObject.RefreshStats`（`HumanObject.cs:1730-1769`）里
    /// `RefreshLevelStats()`（`:1740`）→ `RefreshEquipmentStats()`（`:1742`）→
    /// `RefreshBuffs()`（`:1746`）→ `RefreshGuildBuffs()`（`:1747`）→
    /// `RefreshStatCaps()`（`:1761`）→ `if (HP > Stats[HP]) SetHP(...)`（`:1763-1764`）几步。
    ///
    /// 简化：跳过 `RefreshItemSetStats`（套装）、`RefreshMirSetStats`、`RefreshSkills`
    /// （`:1743-1745`）以及末尾的百分比加成（`:1751-1759`）——
    /// 这些系统本作都还没有；它们只影响数值，不影响属性刷新的**时机**。
    ///
    /// 上限（`Stat.HP`/`Stat.MP`）变化时补发 `S.HealthChanged` 与 `S.ObjectHealth`：
    /// Crystal 靠 `RefreshBuff` 之后客户端自己按 `S.AddBuff` 里的属性重算，本作额外
    /// 兜一条血量同步，免得增益到期时客户端血条停在旧上限（**本作扩展**，见 `docs/world/buffs.md`）。
    pub fn refresh_stats(&mut self, object_id: u32) {
        let Some(player) = self.players.get(&object_id) else {
            return;
        };
        let hp = player.hp;
        let mp = player.mp;
        let mut stats = combat::player_base_stats(player.level, player.class);
        for item in player.equipment.iter().flatten() {
            let Some(info) = self.item_info(item.item_index) else {
                continue;
            };
            apply_equipment_stats(&mut stats, info, item);
        }
        // `RefreshBuffs`：把每个生效 buff 的属性表累加进来（`HumanObject.cs:2326-2333`）。
        // 注意顺序——buff 在装备之后、`RefreshStatCaps` 之前。
        for buff in &player.buffs {
            for (&stat, &value) in buff.stats.values() {
                add_stat(&mut stats, stat, value);
            }
        }
        // `RefreshGuildBuffs`：行会已激活增益的属性（`PlayerObject.cs:1824-1836`），
        // 排在 `RefreshBuffs` 之后、上限夹取之前（`HumanObject.cs:1746-1751`）。
        crate::guild_buffs::apply_guild_buffs(self, player.guild, &mut stats);
        stats.apply_caps();
        let max_changed = stats.hp != player.stats.hp || stats.mp != player.stats.mp;
        if let Some(player) = self.players.get_mut(&object_id) {
            player.stats = stats;
        }
        // `if (HP > Stats[HP]) SetHP(Stats[HP]); if (MP > Stats[MP]) SetMP(Stats[MP]);`（`:1763-1764`）
        // ——`SetHP` 自己会在值没变时不发 `S.HealthChanged`（`HumanObject.cs:858-869`）。
        self.set_player_hp(object_id, hp.min(stats.hp));
        self.set_player_mp(object_id, mp.min(stats.mp));
        if max_changed {
            self.send_health_changed(object_id);
            self.broadcast_health_change(object_id);
        }
    }

    /// 生成一件掉落物实例：Crystal `Envir.CreateDropItem`（`Envir.cs:4407-4424`）。
    ///
    /// 随机数顺序与 Crystal 完全一致：先 `Random.Next(Durability)`，再
    /// `CurrentDura = min(Durability, 抽到的值 + 1000)`，最后 `if (!NeedIdentify) Identified = true`。
    pub fn create_drop_item(&mut self, info: &ItemInfo) -> UserItem {
        let durability = info.durability;
        let rolled = self.rng.next_bounded(u32::from(durability));
        let current = (rolled + 1000).min(u32::from(durability));
        let mut item = UserItem {
            unique_id: self.allocate_item_uid(),
            item_index: info.index,
            current_dura: u16::try_from(current).unwrap_or(u16::MAX),
            max_dura: durability,
            ..UserItem::default()
        };
        if !info.need_identify {
            item.identified = true;
        }
        item
    }

    /// 生成一件**满耐久**的物品实例：Crystal `Envir.CreateFreshItem`（`Envir.cs:4390-4402`）。
    pub fn create_fresh_item(&self, info: &ItemInfo) -> UserItem {
        UserItem {
            unique_id: self.allocate_item_uid(),
            item_index: info.index,
            current_dura: info.durability,
            max_dura: info.durability,
            ..UserItem::default()
        }
    }

    /// 地面物品对外的 `GetInfo` 包（Crystal `ItemObject.GetInfo`，`ItemObject.cs:365-383`）。
    ///
    /// 物品发 `S.ObjectItem`、金币发 `S.ObjectGold`——Crystal 就是这么分的两条包。
    #[must_use]
    pub fn ground_item_frame(&self, ground: &GroundItem) -> Vec<u8> {
        if let Some(item) = &ground.item {
            let info = self.item_info(item.item_index);
            let (name, image, grade) = match info {
                Some(info) => (
                    GroundItem::display_name(&friendly_name(&info.name), item.count),
                    item_image(info, item.count),
                    info.grade,
                ),
                // 模板缺失时给一个可读的占位名，至少不会崩。
                // 这是**全服共用**的 `S.ObjectItem`（没有查看者参数），用服务器默认语言；
                // 需要按查看者语言逐人编码时再改成 per-viewer 的帧生成。
                None => (
                    mir_core::i18n::format_text(
                        Language::ZhCn,
                        TextKey::ItemUnknownPlaceholder,
                        &[("item_index", &item.item_index.to_string())],
                    ),
                    0,
                    mir_core::enums::ItemGrade::default(),
                ),
            };
            return ServerPacket::ObjectItem(ObjectItem {
                object_id: ground.object_id,
                name,
                // Crystal 按品级选颜色（`ItemObject.cs:62-78`），这里用默认白色。
                name_colour: mir_core::Color(-1),
                location: ground.position,
                image,
                grade,
            })
            .encode_frame();
        }
        ServerPacket::ObjectGold(ObjectGold {
            object_id: ground.object_id,
            gold: ground.gold,
            location: ground.position,
        })
        .encode_frame()
    }

    /// 在指定格放一件地面物品（金币用 `gold > 0` + `item = None`），返回它的对象 ID。
    ///
    /// 对应 Crystal `ItemObject.Drop` 命中空格后的
    /// `CurrentLocation = ...; CurrentMap.AddObject(this); Spawned();`（`ItemObject.cs:249-255`）——
    /// `Spawned()` 会把 `GetInfo()` 广播给视野内的玩家（`MapObject.cs:341-355`）。
    pub fn spawn_ground_item(
        &mut self,
        map: &str,
        position: Point,
        item: Option<UserItem>,
        gold: u32,
        owner: Option<u32>,
    ) -> u32 {
        self.spawn_ground_item_with_expire(map, position, item, gold, owner, self.item_expire_ms)
    }

    /// 同 [`World::spawn_ground_item`]，但指定存活时长。
    ///
    /// 玩家死亡掉落用 [`PLAYER_DIED_ITEM_EXPIRE_MS`]（Crystal `ItemObject(dropper, item, deathDrop: true)`，
    /// `ItemObject.cs:53-58`）；其余路径沿用 `item_expire_ms`。
    fn spawn_ground_item_with_expire(
        &mut self,
        map: &str,
        position: Point,
        item: Option<UserItem>,
        gold: u32,
        owner: Option<u32>,
        expire_ms: i64,
    ) -> u32 {
        let now = self.now();
        let object_id = self.allocate_object_id();
        let ground = GroundItem {
            object_id,
            item,
            gold,
            position,
            map: map.to_owned(),
            expire_time: now + expire_ms,
            owner,
            owner_time: if owner.is_some() {
                now + GROUND_ITEM_OWNER_MS
            } else {
                0
            },
        };
        let frame = self.ground_item_frame(&ground);
        self.ground_items.push(ground);
        let index = self.ground_items.len() - 1;
        self.ground_item_slots.insert(object_id, index);
        // AOI：Crystal `ItemObject` 生成即挂进 `CurrentMap.GetCell(CurrentLocation)` 的对象表
        // （`ItemObject.cs:53-58`、`MapObject.cs:339-357`）。
        self.aoi_insert(map, object_id, AoiKind::Item, position);
        self.sync_ground_item_view(object_id, map, position, &frame);
        object_id
    }

    /// 新的地面物品进入视野：给视野内的玩家发 `ObjectItem`/`ObjectGold`。
    ///
    /// 与 [`World::sync_monster_view`] 同构（Crystal 的 `AddObject` → `Enqueue(GetInfo())`，
    /// `MapObject.cs:286-308`）。
    pub(crate) fn sync_ground_item_view(
        &mut self,
        object_id: u32,
        map: &str,
        position: Point,
        frame: &[u8],
    ) {
        // 只查落点 ±VIEW_RANGE 内的玩家（Crystal `MapObject.Spawned` → `BroadcastInfo`，
        // `MapObject.cs:339-357`；不再遍历全服玩家）。
        for player_id in self.aoi_ids_in_radius(map, position, VIEW_RANGE, AoiKind::Player) {
            let Some(player) = self.players.get_mut(&player_id) else {
                continue;
            };
            if player.map == map
                && player.position.in_range(position, VIEW_RANGE)
                && player.visible_items.insert(object_id)
            {
                let _ = player.sender.send(frame.to_vec());
            }
        }
    }

    /// 地面物品消失：给所有「看见过它」的玩家发 `ObjectRemove`
    /// （Crystal `Despawn` 里的 `Broadcast(S.ObjectRemove)`，`MapObject.cs:363`）。
    ///
    /// 同时把物品从 AOI 网格摘掉。
    pub(crate) fn forget_ground_item(&mut self, object_id: u32, map: &str, position: Point) {
        let removal =
            ServerPacket::ObjectRemove(mir_protocol::packets::server::ObjectRemove { object_id })
                .encode_frame();
        for player_id in self.aoi_ids_in_radius(map, position, VIEW_RANGE, AoiKind::Player) {
            let Some(player) = self.players.get_mut(&player_id) else {
                continue;
            };
            if player.visible_items.remove(&object_id) {
                let _ = player.sender.send(removal.clone());
            }
        }
        self.aoi_remove(map, AoiKind::Item, object_id);
    }

    /// 从世界里删掉一件地面物品（拾取、到期、被顶掉），并广播 `ObjectRemove`。
    pub(crate) fn remove_ground_item(&mut self, object_id: u32) {
        let Some((map, position)) = self
            .ground_item(object_id)
            .map(|ground| (ground.map.clone(), ground.position))
        else {
            return;
        };
        self.ground_items.retain(|item| item.object_id != object_id);
        self.rebuild_ground_item_slots();
        self.forget_ground_item(object_id, &map, position);
    }

    /// 地面物品的周期维护：到期消失、归属保护过期。
    ///
    /// Crystal `ItemObject.Process`（`ItemObject.cs:129-142`）：
    /// `if (Envir.Time > ExpireTime) { RemoveObject; Despawn; }` 与
    /// `if (Owner != null && Envir.Time > OwnerTime) Owner = null;`。
    pub(crate) fn tick_ground_items(&mut self, now: i64) {
        let mut expired = Vec::new();
        for ground in &mut self.ground_items {
            if now > ground.expire_time {
                expired.push(ground.object_id);
                continue;
            }
            if ground.owner.is_some() && now > ground.owner_time {
                ground.owner = None;
            }
        }
        for object_id in expired {
            self.remove_ground_item(object_id);
        }
    }

    /// 玩家移动后刷新自己的地面物品视野（与 [`World::sync_player_monsters`] 同构）。
    pub(crate) fn sync_player_ground_items(&mut self, player_id: u32) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let map = player.map.clone();
        let position = player.position;
        // AOI 网格按 `(y,x,ID)` 返回视野内的地面物品（等价 Crystal 33×33 逐格扫描）；
        // ID 升序与地面物品的生成顺序一致，保持既有发包顺序。
        let visible: BTreeSet<u32> = self
            .aoi_ids_in_radius(&map, position, VIEW_RANGE, AoiKind::Item)
            .into_iter()
            .collect();
        let removals: Vec<u32> = player.visible_items.difference(&visible).copied().collect();
        let adds: Vec<Vec<u8>> = visible
            .difference(&player.visible_items)
            .filter_map(|object_id| {
                self.ground_item(*object_id)
                    .map(|ground| self.ground_item_frame(ground))
            })
            .collect();

        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        for object_id in removals {
            let _ = player.sender.send(
                ServerPacket::ObjectRemove(mir_protocol::packets::server::ObjectRemove {
                    object_id,
                })
                .encode_frame(),
            );
        }
        for frame in adds {
            let _ = player.sender.send(frame);
        }
        player.visible_items = visible;
    }

    /// 背包还有几个空格（Crystal `HumanObject.FreeSpace`，`HumanObject.cs:1568-1576`）。
    #[must_use]
    pub fn free_space(&self, object_id: u32) -> usize {
        self.players.get(&object_id).map_or(0, |player| {
            player
                .inventory
                .iter()
                .filter(|slot| slot.is_none())
                .count()
        })
    }

    /// 能否把这件物品放进背包：Crystal `HumanObject.CanGainItem`（`HumanObject.cs:7586-7628`）。
    ///
    /// 只要有一个空格就直接成功；否则只有**可堆叠**物品在「现有堆 + 新数量 ≤ StackSize」时才算成功。
    /// 注意 Crystal 这里**不检查重量**（重量只影响奔跑，`HumanObject.cs:135`）。
    #[must_use]
    pub fn can_gain_item(&self, object_id: u32, info: &ItemInfo, count: u16) -> bool {
        let Some(player) = self.players.get(&object_id) else {
            return false;
        };
        if player.inventory.iter().any(Option::is_none) {
            return true;
        }
        if info.stack_size <= 1 {
            return false;
        }
        let mut remaining = count;
        for slot in player.inventory.iter().flatten() {
            if slot.item_index != info.index {
                continue;
            }
            if slot.count.saturating_add(remaining) <= info.stack_size {
                return true;
            }
            remaining -= info.stack_size.saturating_sub(slot.count);
        }
        false
    }

    /// 把物品塞进背包：Crystal `HumanObject.AddItem`（`HumanObject.cs:1577-1630`）。
    ///
    /// 顺序照抄：
    /// 1. 可堆叠物品先并入已有堆（并完还有剩就继续）；
    /// 2. 药水/卷轴/`Script+Effect==1` 优先用 `0..4` 号格（腰带第一排）；
    /// 3. 护身符优先用 `4..6` 号格；
    /// 4. 其余物品从 6 号格开始找空位；
    /// 5. 都放不下就从 0 号格再找一遍。
    ///
    /// 返回是否真的放进了背包。
    pub fn add_item_to_inventory(&mut self, object_id: u32, item: UserItem) -> bool {
        let Some(info) = self.item_info(item.item_index).cloned() else {
            return false;
        };
        let Some(player) = self.players.get_mut(&object_id) else {
            return false;
        };
        let mut item = item;
        if info.stack_size > 1 {
            for slot in &mut player.inventory {
                let Some(existing) = slot else { continue };
                let same = existing.item_index == info.index;
                if !same || existing.count >= info.stack_size {
                    continue;
                }
                let room = info.stack_size - existing.count;
                if item.count <= room {
                    existing.count += item.count;
                    return true;
                }
                item.count -= room;
                existing.count = info.stack_size;
            }
        }
        let potion = matches!(info.item_type, ItemType::Potion | ItemType::Scroll)
            || (info.item_type == ItemType::Script && info.effect == 1);
        let ranges: [(usize, usize); 3] = if potion {
            [
                (0, POTION_BELT_MAX),
                (BELT_SIZE, INVENTORY_SIZE),
                (0, INVENTORY_SIZE),
            ]
        } else if info.item_type == ItemType::Amulet {
            [
                (AMULET_BELT_MIN, AMULET_BELT_MAX),
                (BELT_SIZE, INVENTORY_SIZE),
                (0, INVENTORY_SIZE),
            ]
        } else {
            [
                (BELT_SIZE, INVENTORY_SIZE),
                (0, INVENTORY_SIZE),
                (BELT_SIZE, INVENTORY_SIZE),
            ]
        };
        for (start, end) in ranges {
            let end = end.min(player.inventory.len());
            for index in start..end {
                if player.inventory[index].is_none() {
                    player.inventory[index] = Some(item);
                    return true;
                }
            }
        }
        false
    }

    /// 给玩家一件物品：Crystal `HumanObject.GainItem`（`HumanObject.cs:7824-7835`）。
    ///
    /// 顺序照抄：
    /// 1. 先发 `S.GainedItem`（发**改动前**的副本——Crystal 的 `Clone` 就是为了这个，
    ///    `HumanObject.cs:7829-7831`），因为并入堆叠时物品数量会被改写；
    /// 2. 再走 [`World::add_item_to_inventory`]（Crystal `AddItem`，`:1577-1630`）；
    /// 3. 背包放不下时兜底寄系统邮件（Crystal `PlayerObject.GainItemMail`，
    ///    `PlayerObject.cs:7695-7698`），避免物品凭空消失。正常情况下调用方已经用
    ///    `can_gain_item`/`trade_can_gain_items` 检查过，走不到这里。
    ///
    /// 返回是否直接放进了背包。
    pub fn gain_item(&mut self, object_id: u32, item: UserItem) -> bool {
        // `Enqueue(new S.GainedItem { Item = clonedItem });`（`HumanObject.cs:7831`）
        self.send_to_player(
            object_id,
            &ServerPacket::GainedItem(mir_protocol::packets::server::GainedItem {
                item: item.clone(),
            })
            .encode_frame(),
        );
        if self.add_item_to_inventory(object_id, item.clone()) {
            return true;
        }
        tracing::warn!(
            player = object_id,
            unique_id = item.unique_id,
            "背包放不下，物品改走系统邮件（调用方本应先用 CanGainItem 检查）"
        );
        self.mail_item_to_self(object_id, item);
        false
    }

    /// 拾取脚下（**只扫当前格**）的地面物品：Crystal `PlayerObject.PickUp`（`:7483-7537`）。
    ///
    /// 逐条照抄的判定顺序：
    /// 1. `if (Dead) return;`（`:7485-7489`）→ [`ItemError::Dead`]；
    /// 2. 只看 `CurrentMap.GetCell(CurrentLocation)` 这一格（`:7491-7497`）——
    ///    Crystal **不会**顺手捡相邻格，本作也不；
    /// 3. 归属不属于自己且不是队友 → 记 `sendFail` 并看下一个（`:7501-7505`）；
    /// 4. 物品：`CanGainItem` 为假就跳过（`:7510`）；成功则 `GainItem`（本人 `S.GainedItem`，
    ///    `HumanObject.cs:7824-7835`）→ 移除地面物品；
    /// 5. 金币：`CanGainGold` 为假就跳过（`:7526`）；成功则 `GainGold`（本人 `S.GainedGold`，
    ///    `PlayerObject.cs:7673-7683`）→ 移除地面物品；
    /// 6. 一件都没捡到且有归属失败 → Crystal 发系统聊天「不能捡别人的东西」（`:7534-7535`），
    ///    本作返回 [`ItemError::NotOwner`] 交给会话层。
    ///
    /// 扩展：第 4 步里额外检查负重（见 [`World::enforce_bag_weight`]）。
    pub fn pickup(&mut self, object_id: u32) -> Result<PickupReport, ItemError> {
        let now = self.now();
        let Some(player) = self.players.get(&object_id) else {
            return Err(ItemError::NotInWorld(object_id));
        };
        if player.dead {
            return Err(ItemError::Dead { object_id });
        }
        let cell = player.position;
        let map = player.map.clone();
        // 只有**站在物品上**才能捡（Crystal `CurrentMap.GetCell(CurrentLocation).Objects`，
        // `PlayerObject.cs:7491-7497`）；用 AOI 网格查当前格，ID 升序 = 生成顺序。
        let candidates: Vec<u32> = self
            .aoi_ids_in_cell(&map, cell, AoiKind::Item)
            .into_iter()
            .filter(|object_id| {
                self.ground_item(*object_id)
                    .is_some_and(|ground| ground.map == map && ground.position == cell)
            })
            .collect();
        if candidates.is_empty() {
            return Err(ItemError::NoGroundItem);
        }

        let mut blocked_owner = None;
        let mut inventory_full = false;
        for ground_id in candidates {
            let Some(ground) = self.ground_item(ground_id).cloned() else {
                continue;
            };
            if ground.is_gold() {
                let gold = ground.gold;
                if gold == 0 {
                    continue;
                }
                // `if (!CanGainGold(item.Gold)) continue;`（`:7526`）
                let Some(player) = self.players.get(&object_id) else {
                    return Err(ItemError::NotInWorld(object_id));
                };
                if player.gold + i64::from(gold) > i64::from(u32::MAX) {
                    continue;
                }
                self.gain_gold(object_id, gold);
                self.remove_ground_item(ground_id);
                return Ok(PickupReport {
                    ground: ground_id,
                    item: None,
                    gold,
                });
            }
            let Some(item) = ground.item.clone() else {
                continue;
            };
            // 归属：`ob.Owner != null && ob.Owner != this && !IsGroupMember(ob.Owner)`（`:7501-7505`）。
            if let Some(owner) = ground.owner {
                if owner != object_id && now <= ground.owner_time {
                    blocked_owner = Some(owner);
                    continue;
                }
            }
            let Some(info) = self.item_info(item.item_index).cloned() else {
                tracing::warn!(
                    item_index = item.item_index,
                    "地面物品的模板不存在，跳过（数据与物品库不一致）"
                );
                continue;
            };
            // `if (!CanGainItem(item.Item)) continue;`（`:7510`）
            if !self.can_gain_item(object_id, &info, item.count) {
                inventory_full = true;
                continue;
            }
            // 扩展：Crystal 拾取时不查重量，本作查（可用 `set_enforce_bag_weight(false)` 关掉）。
            if self.enforce_bag_weight {
                let added = u32::try_from(item_weight(&info, &item).max(0)).unwrap_or(0);
                let limit =
                    u32::try_from(self.players[&object_id].stats.bag_weight.max(0)).unwrap_or(0);
                let total = self.bag_weight(object_id).saturating_add(added);
                if total > limit {
                    return Err(ItemError::TooHeavy {
                        weight: total,
                        limit,
                    });
                }
            }
            // `Enqueue(new S.GainedItem { Item = clonedItem });`（`HumanObject.cs:7831`）——
            // 先发**未改动**的副本，再并入背包（Crystal 的 Clone 就是为了这个）。
            self.send_to_player(
                object_id,
                &ServerPacket::GainedItem(GainedItem { item: item.clone() }).encode_frame(),
            );
            if !self.add_item_to_inventory(object_id, item.clone()) {
                // 理论上 `can_gain_item` 已经保证了；真走到这里说明数据在两次检查之间变了。
                return Err(ItemError::InventoryFull);
            }
            // 收集类任务目标：按背包里的实时数量重算（Crystal `CheckGroupQuestItem` →
            // `CheckNeedQuestItem`，`PlayerObject.cs:11509-11561`；本作没有任务物品栏，
            // 所以不「截胡」物品，只在拾取后刷新计数）。
            self.refresh_quest_items(object_id);
            self.remove_ground_item(ground_id);
            return Ok(PickupReport {
                ground: ground_id,
                item: Some(item),
                gold: 0,
            });
        }

        if let Some(owner) = blocked_owner {
            return Err(ItemError::NotOwner { owner });
        }
        // 简化：Crystal 对「背包满」是静默跳过（`:7510` 的 `continue`，
        // 一件都没捡到就什么都不发）；本作返回更具体的错误，方便会话层/日志定位。
        if inventory_full {
            return Err(ItemError::InventoryFull);
        }
        Err(ItemError::NoGroundItem)
    }

    /// 加金币：Crystal `PlayerObject.GainGold`（`:7673-7683`）。
    ///
    /// 满 `u32::MAX` 时截断（Crystal 的 `CanGainGold` 也是这个上限，`:7638-7641`），
    /// 并给本人发 `S.GainedGold`（Crystal 是 `Enqueue`，只给本人，不是广播）。
    pub fn gain_gold(&mut self, object_id: u32, gold: u32) {
        if gold == 0 {
            return;
        }
        let Some(player) = self.players.get_mut(&object_id) else {
            return;
        };
        let mut gold = gold;
        if player.gold + i64::from(gold) > i64::from(u32::MAX) {
            gold = u32::try_from(i64::from(u32::MAX) - player.gold).unwrap_or(0);
        }
        // `if (gold == 0) return;`（`:7675`）——溢满时截成 0 就不发包。
        if gold == 0 {
            return;
        }
        player.gold += i64::from(gold);
        let _ = player
            .sender
            .send(ServerPacket::GainedGold(GainedGold { gold }).encode_frame());
    }

    /// 扣金币：Crystal `PlayerObject.DropGold`（`:7473-7482`）里的
    /// `Account.Gold -= gold; Enqueue(new S.LoseGold { Gold = gold });`。
    ///
    /// 真正的「丢金币落地」入口是 [`World::drop_gold`]；这个函数只负责扣钱与发包。
    pub fn lose_gold(&mut self, object_id: u32, gold: u32) -> Result<(), ItemError> {
        let Some(player) = self.players.get_mut(&object_id) else {
            return Err(ItemError::NotInWorld(object_id));
        };
        if player.gold < i64::from(gold) {
            return Err(ItemError::GoldOverflow);
        }
        player.gold -= i64::from(gold);
        let _ = player
            .sender
            .send(ServerPacket::LoseGold(LoseGold { gold }).encode_frame());
        Ok(())
    }

    /// 丢金币到地上：Crystal `PlayerObject.DropGold`（`PlayerObject.cs:7473-7482`）。
    ///
    /// ```csharp
    /// if (Account.Gold < gold) return;            // 钱不够：静默
    /// ItemObject ob = new ItemObject(this, gold); // 落点先按自己脚下
    /// if (!ob.Drop(5)) return;                    // 半径 5 找空格，失败也静默
    /// Account.Gold -= gold;
    /// Enqueue(new S.LoseGold { Gold = gold });
    /// ```
    ///
    /// 金币堆复用现有的 [`GroundItem`]（`item = None`、金额在 `gold`），
    /// 落地时给视野内玩家发 `S.ObjectGold`（见 [`World::spawn_ground_item`]）。
    /// 顺序照抄：**先落地再扣钱**——落点找不到时玩家一分钱都不会掉。
    ///
    /// 简化：`gold == 0` 时不落地（Crystal 会生成一堆 0 金币的地面物品，
    /// 见 `docs/world/items.md` 的异常清单）。
    pub fn drop_gold(&mut self, object_id: u32, gold: u32) -> Result<Option<u32>, ItemError> {
        let Some(player) = self.players.get(&object_id) else {
            return Err(ItemError::NotInWorld(object_id));
        };
        let held = u32::try_from(player.gold.clamp(0, i64::from(u32::MAX))).unwrap_or(0);
        if player.gold < i64::from(gold) {
            return Err(ItemError::NotEnoughGold {
                requested: gold,
                held,
            });
        }
        if gold == 0 {
            return Ok(None);
        }
        let map = player.map.clone();
        let position = player.position;
        // `ob.Drop(5)`：半径是 **5**（不是掉落物品的 `Settings.DropRange = 4`）。
        let Some(cell) = self.drop_cell_at(&map, position, DROP_GOLD_RANGE) else {
            return Ok(None);
        };
        // `ob.Drop()` 成功之后才 `Account.Gold -= gold`（`:7479-7480`）。
        let ground = self.spawn_ground_item(&map, cell, None, gold, None);
        self.lose_gold(object_id, gold)?;
        Ok(Some(ground))
    }

    /// 用物品（`C.UseItem`，Crystal `PlayerObject.UseItem`，`PlayerObject.cs:5799-6338`）。
    ///
    /// 判定顺序逐条照抄：
    ///
    /// ```csharp
    /// 找 UUID（背包 46 格） → CanUseItem（性别/职业/等级）→ Dead 检查  // :5806-5824
    /// switch (item.Info.Type) { Potion / Scroll / Book / Food / … }     // :5826-6328
    /// if (item.Count > 1) item.Count--; else Info.Inventory[index] = null;  // :6330-6331
    /// Report.ItemChanged(item, 1, 1);                                  // :6334（服务端统计，不上行）
    /// p.Success = true; Enqueue(p);                                    // :6336-6337
    /// ```
    ///
    /// 失败一律发 `S.UseItem{Success=false}`（Crystal 的 `Enqueue(p)`，`p.Success` 默认 false），
    /// **并且不消耗任何东西**。药水/卷轴/书的细节见 `docs/world/items.md` §「使用物品」。
    ///
    /// 简化（都在对应分支标注了 `// 简化：`）：
    /// * 传送/回城/随机传送卷（`Scroll` 的 shape 0/1/2 等）需要地图绑定与传送系统；
    /// * `Food` 只在有坐骑时才有意义（喂坐骑耐久，`PlayerObject.cs:6093-6117`）；
    /// * 护身符/毒药的消耗不在本函数里（技能那一侧仍未实现）。
    pub fn use_item(&mut self, object_id: u32, unique_id: u64) -> Result<UseItemReport, ItemError> {
        if !self.players.contains_key(&object_id) {
            return Err(ItemError::NotInWorld(object_id));
        }
        // `item == null || index == -1 || !CanUseItem(item)`（`:5814-5818`）。
        let Some((index, item)) = self.find_inventory_item(object_id, unique_id) else {
            self.send_use_item_failure(object_id, unique_id);
            return Err(ItemError::ItemNotFound { unique_id });
        };
        let Some(info) = self.item_info(item.item_index).cloned() else {
            self.send_use_item_failure(object_id, unique_id);
            return Err(ItemError::UnknownItem(item.item_index));
        };
        if let Err(error) = self.can_use_requirements(object_id, &info) {
            self.send_use_item_failure(object_id, unique_id);
            return Err(error);
        }
        // 复活卷轴是唯一「死亡时也能用」的物品（`:5820-5824`）。
        let resurrect = info.item_type == ItemType::Scroll && info.shape == 6;
        if self.players[&object_id].dead && !resurrect {
            self.send_use_item_failure(object_id, unique_id);
            return Err(ItemError::Dead { object_id });
        }

        let mut report = UseItemReport {
            unique_id,
            item_index: info.index,
            item_name: info.name.clone(),
            hp: 0,
            mp: 0,
            pot_health: 0,
            pot_mana: 0,
            buffs: Vec::new(),
            learned: None,
            remaining: None,
        };
        match info.item_type {
            ItemType::Potion => {
                if let Err(error) = self.use_potion(object_id, &info, &item, &mut report) {
                    // 药水分支里的拒绝（满血/满蓝）还没发过包，这里补上失败包。
                    self.send_use_item_failure(object_id, unique_id);
                    return Err(error);
                }
            }
            ItemType::Scroll if resurrect => {
                // `MP = Stats[Stat.MP]; Revive(MaxHealth, true);`（`:5986-5987`）。
                let max_mp = self.players[&object_id].stats.mp;
                self.set_player_mp(object_id, max_mp);
                self.revive_player(object_id);
            }
            ItemType::Scroll => {
                // 简化：传送/回城/祝福油/修复油等卷轴需要传送、武器幸运、修理子系统，
                // 本作没有；直接拒绝并提示，不消耗物品（Crystal 会把不认识的花色照样吃掉）。
                let message = self.format(
                    object_id,
                    TextKey::ItemEffectNotImplemented,
                    &[("item", &info.name)],
                );
                self.send_system_chat(object_id, &message);
                self.send_use_item_failure(object_id, unique_id);
                return Err(ItemError::UnsupportedItem {
                    item_index: info.index,
                    shape: info.shape,
                });
            }
            ItemType::Book => {
                // 书走技能系统（`PlayerObject.cs:6077-6089`），它会自己发 `S.NewMagic`
                // 与 `S.UseItem{Success=true}` 并扣掉一本。
                let learned = self.use_book(object_id, unique_id)?;
                report.learned = Some(learned.magic);
                report.remaining = learned.book;
                return Ok(report);
            }
            ItemType::Food => {
                // 简化：食物只用来喂坐骑（`PlayerObject.cs:6093-6117`），本作没有坐骑。
                let message = self.text(object_id, TextKey::ItemMountNotImplemented);
                self.send_system_chat(object_id, &message);
                self.send_use_item_failure(object_id, unique_id);
                return Err(ItemError::UnsupportedItem {
                    item_index: info.index,
                    shape: info.shape,
                });
            }
            _ => {
                // Crystal 的 `default: return;`（`:6326-6327`）：什么都不发、也不消耗。
                // 本作发一条失败的 `S.UseItem`，让客户端的物品格子解锁（差异见 docs）。
                self.send_use_item_failure(object_id, unique_id);
                return Err(ItemError::UnsupportedItem {
                    item_index: info.index,
                    shape: info.shape,
                });
            }
        }

        // `if (item.Count > 1) item.Count--; else Info.Inventory[index] = null;`（`:6330-6331`）。
        report.remaining = self.consume_one(object_id, index);
        // `p.Success = true; Enqueue(p);`（`:6336-6337`）。
        self.send_use_item_success(object_id, unique_id);
        Ok(report)
    }

    /// 药水分支（Crystal `PlayerObject.cs:5828-5894`）。
    ///
    /// `ItemInfo.Shape` 的含义：
    ///
    /// | Shape | Crystal | 效果 |
    /// |---|---|---|
    /// | 0 | `NormalPotion` | `PotHealthAmount/PotManaAmount += Stats[HP/MP]`，之后由 `ProcessRegen` 分批放（`:5831-5834`） |
    /// | 1 | `SunPotion` | 立刻 `ChangeHP/ChangeMP(Stats[HP/MP])`（`:5835-5838`） |
    /// | 2 | `MysteryWater` | 解除诅咒装备的限制（`:5839-5848`），本作只置标记并提示 |
    /// | 3 | `Buff` | 按属性表套若干个 `BuffType.Impact/Magic/Taoist/Storm/HealthAid/ManaAid/Defence/MagicDefence/BagWeight`（`:5849-5879`） |
    /// | 4 | `Exp` | `BuffType.Exp`，`Stat.ExpRatePercent = 物品的 Luck`（`:5881-5886`） |
    /// | 5 | `Drop` | `BuffType.Drop`，`Stat.ItemDropRatePercent = 物品的 Luck`（`:5887-5892`） |
    ///
    /// 注意 Crystal 的「太阳水」是**固定值**（模板 `Stats[Stat.HP]`），
    /// 不是百分比——模板里没有百分比字段（`Shared/Data/ItemData.cs` 的属性表就是 `Stats`）。
    fn use_potion(
        &mut self,
        object_id: u32,
        info: &ItemInfo,
        item: &UserItem,
        report: &mut UseItemReport,
    ) -> Result<(), ItemError> {
        let hp_amount = info.stats.get(Stat::HP);
        let mp_amount = info.stats.get(Stat::MP);
        let (hp, max_hp, mp, max_mp) = {
            let player = &self.players[&object_id];
            (player.hp, player.stats.hp, player.mp, player.stats.mp)
        };
        match info.shape {
            0 => {
                // 满血/满蓝喝下去毫无效果 → 本作直接拒绝（**扩展**，Crystal 会白喝）。
                if !potion_has_effect(hp_amount, hp, max_hp, mp_amount, mp, max_mp) {
                    let message = self.text(object_id, TextKey::ItemPotionNoEffectFull);
                    self.send_system_chat(object_id, &message);
                    return Err(ItemError::PotionNoEffect);
                }
                let (pot_hp, pot_mp) = {
                    let Some(player) = self.players.get_mut(&object_id) else {
                        return Err(ItemError::NotInWorld(object_id));
                    };
                    // `PotHealthAmount = (ushort)Math.Min(ushort.MaxValue, PotHealthAmount + …)`
                    // （`:5832-5833`）。
                    player.pot_health_amount = player
                        .pot_health_amount
                        .saturating_add(u16::try_from(hp_amount.max(0)).unwrap_or(u16::MAX));
                    player.pot_mana_amount = player
                        .pot_mana_amount
                        .saturating_add(u16::try_from(mp_amount.max(0)).unwrap_or(u16::MAX));
                    (player.pot_health_amount, player.pot_mana_amount)
                };
                report.pot_health = u32::from(pot_hp);
                report.pot_mana = u32::from(pot_mp);
            }
            1 => {
                if !potion_has_effect(hp_amount, hp, max_hp, mp_amount, mp, max_mp) {
                    let message = self.text(object_id, TextKey::ItemPotionNoEffectFull);
                    self.send_system_chat(object_id, &message);
                    return Err(ItemError::PotionNoEffect);
                }
                // `ChangeHP(item.Info.Stats[Stat.HP]); ChangeMP(item.Info.Stats[Stat.MP]);`
                // （`:5836-5837`）——两个都走 `SendHealthChanged`。
                if hp_amount > 0 {
                    let before = self.players[&object_id].hp;
                    self.change_player_hp(object_id, hp_amount);
                    report.hp = self.players[&object_id].hp - before;
                }
                if mp_amount > 0 {
                    let before = self.players[&object_id].mp;
                    self.change_player_mp(object_id, mp_amount);
                    report.mp = self.players[&object_id].mp - before;
                }
            }
            2 => {
                // `UnlockCurse = true;`（`:5847`）+ 两条提示（`:5840-5846`）。
                if self.players[&object_id].unlock_curse {
                    let message = self.text(object_id, TextKey::ItemCurseLifted);
                    self.send_system_chat(object_id, &message);
                    return Err(ItemError::PotionNoEffect);
                }
                if let Some(player) = self.players.get_mut(&object_id) {
                    player.unlock_curse = true;
                }
                let message = self.text(object_id, TextKey::ItemCurseLiftedNow);
                self.send_system_chat(object_id, &message);
            }
            3 => {
                // `int time = item.Info.Durability;`（`:5850`），单位是**分钟**。
                let duration = minutes_to_ms(info.durability);
                let groups = buff_potion_groups(info, item);
                for (kind, stats) in groups {
                    self.add_buff(object_id, kind, duration, stats, object_id);
                    report.buffs.push(kind);
                }
            }
            4 => {
                // `AddBuff(BuffType.Exp, this, Settings.Minute * time,
                //          new Stats { [Stat.ExpRatePercent] = item.GetTotal(Stat.Luck) })`（`:5881-5886`）。
                let mut stats = mir_protocol::data::Stats::default();
                stats.set(Stat::ExpRatePercent, item_total(info, item, Stat::Luck));
                self.add_buff(
                    object_id,
                    BuffKind::Exp,
                    minutes_to_ms(info.durability),
                    stats,
                    object_id,
                );
                report.buffs.push(BuffKind::Exp);
            }
            5 => {
                // `AddBuff(BuffType.Drop, …, new Stats { [Stat.ItemDropRatePercent] = … })`（`:5887-5892`）。
                let mut stats = mir_protocol::data::Stats::default();
                stats.set(
                    Stat::ItemDropRatePercent,
                    item_total(info, item, Stat::Luck),
                );
                self.add_buff(
                    object_id,
                    BuffKind::Drop,
                    minutes_to_ms(info.durability),
                    stats,
                    object_id,
                );
                report.buffs.push(BuffKind::Drop);
            }
            _ => {
                // Crystal 的内层 `switch` 没有 `default`：不认识的花色照样被吃掉（`:5893-5894`）。
                // 本作照抄「消耗」这个结果，但不发任何效果。
            }
        }
        Ok(())
    }

    /// `HumanObject.CanUseItem` 里与「使用」有关的三关：性别、职业、等级
    /// （`HumanObject.cs:1020-1160`），外加复活卷轴的「必须处于死亡状态」（`:1195-1200`）。
    ///
    /// 失败时按 Crystal 的位置发一条 `S.Chat{System}`；调用方负责补 `S.UseItem{Success=false}`。
    ///
    /// 简化：`RequiredType` 的 AC/MAC/DC/MC/SC 分支（`:1090-1166`）与地图级开关
    /// （`NoDrug`/`NoEscape`/`NoTownTeleport`，`:1169-1235`）未实现——前者只用于装备，
    /// 后者需要地图的 `MapInfo` 运行期表；本作的药水/卷轴/书只用等级限制。
    pub fn can_use_requirements(&self, object_id: u32, info: &ItemInfo) -> Result<(), ItemError> {
        let Some(player) = self.players.get(&object_id) else {
            return Err(ItemError::NotInWorld(object_id));
        };
        // `RequiredGender`：`None = 3`（两位都允许）；`0` 视为「没有限制」
        // （Rust 侧的 `ItemInfo::default()` 是 0，而 C# 字段初值是 `None`）。
        let required_gender = info.required_gender.0;
        if required_gender != 0 && required_gender & gender_flag(player.gender) == 0 {
            let message = self.text(object_id, TextKey::ItemWrongGender);
            self.send_system_chat(object_id, &message);
            return Err(ItemError::WrongGender);
        }
        let required_class = info.required_class.0;
        if required_class != 0 && required_class & class_flag(player.class) == 0 {
            let message = self.text(object_id, TextKey::ItemWrongClass);
            self.send_system_chat(object_id, &message);
            return Err(ItemError::WrongClass {
                required: info.required_class,
                class: player.class,
            });
        }
        if info.required_type == RequiredType::Level
            && player.level < u16::from(info.required_amount)
        {
            let message = self.format(
                object_id,
                TextKey::ItemLevelTooLow,
                &[("required", &info.required_amount.to_string())],
            );
            self.send_system_chat(object_id, &message);
            return Err(ItemError::LowLevel {
                required: info.required_amount,
                level: player.level,
            });
        }
        if info.required_type == RequiredType::MaxLevel
            && player.level > u16::from(info.required_amount)
        {
            let message = self.format(
                object_id,
                TextKey::ItemLevelTooHigh,
                &[("max", &info.required_amount.to_string())],
            );
            self.send_system_chat(object_id, &message);
            return Err(ItemError::LowLevel {
                required: info.required_amount,
                level: player.level,
            });
        }
        // `case 6: if (!Dead) { …; return false; }`（`:1195-1200`）。
        if info.item_type == ItemType::Scroll && info.shape == 6 && !player.dead {
            let message = self.text(object_id, TextKey::ItemReviveNotDead);
            self.send_system_chat(object_id, &message);
            return Err(ItemError::UnsupportedItem {
                item_index: info.index,
                shape: info.shape,
            });
        }
        Ok(())
    }

    /// 消耗背包里的一件（`PlayerObject.cs:6330-6331`），返回这一格的剩余内容。
    pub(crate) fn consume_one(&mut self, object_id: u32, index: usize) -> Option<UserItem> {
        let player = self.players.get_mut(&object_id)?;
        let slot = player.inventory.get_mut(index)?;
        let stack = slot.as_mut()?;
        if stack.count > 1 {
            stack.count -= 1;
            Some(stack.clone())
        } else {
            *slot = None;
            None
        }
    }

    /// 发一条失败的 `S.UseItem`（Crystal `PlayerObject.UseItem` 里的 `Enqueue(p)`）。
    pub(crate) fn send_use_item_failure(&self, object_id: u32, unique_id: u64) {
        self.send_use_item_packet(object_id, unique_id, false);
    }

    /// 发一条成功的 `S.UseItem`（`p.Success = true; Enqueue(p);`，`:6336-6337`）。
    pub(crate) fn send_use_item_success(&self, object_id: u32, unique_id: u64) {
        self.send_use_item_packet(object_id, unique_id, true);
    }

    fn send_use_item_packet(&self, object_id: u32, unique_id: u64, success: bool) {
        self.send_to_player(
            object_id,
            &ServerPacket::UseItem(UseItemPacket {
                unique_id,
                success,
                grid: MirGridType::Inventory,
            })
            .encode_frame(),
        );
    }

    /// 丢弃物品：Crystal `PlayerObject.DropItem`（`:7359-7472`）。
    ///
    /// 判定顺序照抄：死亡 → 找 UUID → 数量夹取 → `BindMode.DontDrop` → 先找落点再动背包
    /// （Crystal 的 `if (!DropItem(temp)) { Enqueue(p); return; }` 在清空背包格**之前**，
    /// `:7430-7444`）。成功后本人收 `S.DropItem{Success = true}`（`:7459-7460`），
    /// 视野内所有人（含自己）收 `S.ObjectItem`/`S.ObjectGold`。
    ///
    /// 简化：
    /// * 地图的 `NoThrowItem`（`:7368-7373`）没做——本作还没有地图级开关；
    /// * 装备区/英雄背包的丢弃（`isHeroItem`）没做；
    /// * `RentalInformation` / `DestroyOnDrop` / `WeddingRing` 等绑定/租借规则没做（本作没有这些系统）。
    ///
    /// # Errors
    /// 对象不在世界/已死亡、物品不存在、数量非法、绑定禁止丢弃、附近没有空格时返回错误。
    pub fn drop_item(
        &mut self,
        object_id: u32,
        unique_id: u64,
        count: u16,
    ) -> Result<u32, ItemError> {
        let request = DropItemPacket {
            unique_id,
            count,
            hero_item: false,
            success: false,
        };
        let Some(player) = self.players.get(&object_id) else {
            self.send_to_player(object_id, &ServerPacket::DropItem(request).encode_frame());
            return Err(ItemError::NotInWorld(object_id));
        };
        if player.dead {
            self.send_to_player(object_id, &ServerPacket::DropItem(request).encode_frame());
            return Err(ItemError::Dead { object_id });
        }
        let map = player.map.clone();
        let position = player.position;
        let Some((index, stored)) = self.find_inventory_item(object_id, unique_id) else {
            self.send_to_player(object_id, &ServerPacket::DropItem(request).encode_frame());
            return Err(ItemError::ItemNotFound { unique_id });
        };
        // `if (count == 0 || count > item.Count) count = item.Count;`（`:7344`）
        let count = if count == 0 || count > stored.count {
            stored.count
        } else {
            count
        };
        if count < 1 {
            self.send_to_player(object_id, &ServerPacket::DropItem(request).encode_frame());
            return Err(ItemError::BadCount {
                count,
                available: stored.count,
            });
        }
        let Some(info) = self.item_info(stored.item_index).cloned() else {
            self.send_to_player(object_id, &ServerPacket::DropItem(request).encode_frame());
            return Err(ItemError::UnknownItem(stored.item_index));
        };
        // `if (temp.Info.Bind.HasFlag(BindMode.DontDrop))`（`:7416-7420`）
        if info.bind.contains(BindMode::DontDrop) {
            self.send_to_player(object_id, &ServerPacket::DropItem(request).encode_frame());
            return Err(ItemError::CannotDrop { unique_id });
        }
        // 先找落点：Crystal 的顺序是「落地成功才从背包里扣」。
        let Some(cell) = self.drop_cell(&map, position) else {
            self.send_to_player(object_id, &ServerPacket::DropItem(request).encode_frame());
            return Err(ItemError::NoDropCell);
        };

        let dropped = if count < stored.count {
            // 拆出一件新的：`Envir.CreateFreshItem(temp.Info); temp2.Count = count;`（`:7449-7450`）
            let mut part = self.create_fresh_item(&info);
            part.count = count;
            if let Some(player) = self.players.get_mut(&object_id) {
                if let Some(slot) = player.inventory[index].as_mut() {
                    slot.count -= count;
                }
            }
            part
        } else {
            let mut whole = stored;
            if let Some(player) = self.players.get_mut(&object_id) {
                player.inventory[index] = None;
            }
            // `HumanObject.DropItem`：肉会掉 2000 点耐久（`HumanObject.cs:1386-1387`）。
            if info.item_type == ItemType::Meat {
                whole.current_dura = whole.current_dura.saturating_sub(2000);
            }
            whole
        };

        let ground_id = self.spawn_ground_item(&map, cell, Some(dropped), 0, None);
        self.send_to_player(
            object_id,
            &ServerPacket::DropItem(DropItemPacket {
                success: true,
                ..request
            })
            .encode_frame(),
        );
        Ok(ground_id)
    }

    /// `C.MoveItem` 的总入口：按 `grid` 分派。
    ///
    /// * `Inventory` → 背包内交换（Crystal `PlayerObject.MoveItem`）；
    /// * `Equipment` → 穿上/脱下（Crystal `EquipItem`/`RemoveItem`，见 [`World::move_equipment`]）；
    /// * 其它网格（仓库/交易/英雄背包…）本作还没有，返回 [`ItemError::UnsupportedGrid`]。
    pub fn move_item(
        &mut self,
        object_id: u32,
        grid: MirGridType,
        from: i32,
        to: i32,
    ) -> Result<(), ItemError> {
        match grid {
            MirGridType::Inventory => self.move_inventory(object_id, from, to),
            MirGridType::Equipment => self.move_equipment(object_id, from, to),
            other => Err(ItemError::UnsupportedGrid(other)),
        }
    }

    /// 背包内交换两个格子：Crystal `PlayerObject.MoveItem`（`:5287-5375`）的 `Inventory` 分支。
    ///
    /// 判定照抄 `:5350-5374`：两个下标都要在范围内、`from` **必须有物品**、目标格可以是空。
    /// 成功本人收 `S.MoveItem{Success = true}`。
    ///
    /// 关于「UUID 不符」：`C.MoveItem` 只有 `Grid/From/To` 三个字段
    /// （`Shared/ClientPackets.cs`），**没有** UUID，所以这一层没有可校验的 UUID；
    /// 服务端只按格子搬运。需要按 UUID 定位的包是 `C.EquipItem` / `C.DropItem` / `C.UseItem`。
    pub fn move_inventory(&mut self, object_id: u32, from: i32, to: i32) -> Result<(), ItemError> {
        let request = MoveItemPacket {
            grid: MirGridType::Inventory,
            from,
            to,
            success: false,
        };
        let Some(player) = self.players.get_mut(&object_id) else {
            return Err(ItemError::NotInWorld(object_id));
        };
        let length = player.inventory.len() as i32;
        if from < 0 || to < 0 || from >= length || to >= length {
            let _ = player
                .sender
                .send(ServerPacket::MoveItem(request).encode_frame());
            return Err(ItemError::SlotOutOfRange {
                grid: MirGridType::Inventory,
                index: if from < 0 || from >= length { from } else { to },
            });
        }
        let (from_index, to_index) = (from as usize, to as usize);
        // `if (array[from] == null)`（`:5352-5358`）
        if player.inventory[from_index].is_none() {
            let _ = player
                .sender
                .send(ServerPacket::MoveItem(request).encode_frame());
            return Err(ItemError::EmptySlot {
                grid: MirGridType::Inventory,
                index: from,
            });
        }
        player.inventory.swap(from_index, to_index);
        let _ = player.sender.send(
            ServerPacket::MoveItem(MoveItemPacket {
                success: true,
                ..request
            })
            .encode_frame(),
        );
        Ok(())
    }

    /// 装备穿戴/脱下（用 `C.MoveItem{Grid = Equipment}` 表达）。
    ///
    /// Crystal 的服务端把这两件事分成两条客户端包：
    /// * 穿上 = `C.EquipItem{Grid = Inventory, UniqueID, To = 装备槽}` → `PlayerObject.EquipItem`
    ///   （`Server/MirNetwork/MirConnection.cs:1297-1301`、`PlayerObject.cs:5527-5693`）；
    /// * 脱下 = `C.RemoveItem{Grid = Equipment, UniqueID, To = 背包格}` → `PlayerObject.RemoveItem`
    ///   （`MirConnection.cs:1303-1307`、`PlayerObject.cs:5013-5135`）。
    ///
    /// 本作把这些动作都塞进 `C.MoveItem`，于是**方向要自己判**（按顺序尝试，第一条成立就执行）：
    ///
    /// 1. `to` 是装备槽、`from` 是背包格、且 [`World::can_equip_item`] 通过 → 穿上
    ///    （Crystal `EquipItem`，旧装备换回 `from` 号背包格，`:5674-5678`）；
    ///    这一条优先，避免「背包 5 号格的戒指戴到 5 号槽」被误判成脱下；
    /// 2. `from` 是有物品的装备槽、`to` 是背包格 → 脱下（Crystal `RemoveItem`）；
    ///    目标格有物品时返回 [`ItemError::SlotOccupied`]（Crystal 要求 `toArray[to] == null`，`:5118`）；
    /// 3. 以上都不成立：能诊断出「想穿但穿不上」就返回 [`ItemError::CannotEquip`]，
    ///    否则返回 [`ItemError::SlotOutOfRange`]，并给本人发一条 `Success = false` 的 `S.MoveItem`。
    ///
    /// 成功后，穿上发 `S.EquipItem`、脱下发 `S.RemoveItem`（Crystal 用的就是这两条包），
    /// 外观有变化时再广播 `S.PlayerUpdate`（`HumanObject.UpdateLooks`，`HumanObject.cs:7058-7061`）。
    pub fn move_equipment(&mut self, object_id: u32, from: i32, to: i32) -> Result<(), ItemError> {
        let fail = |world: &World, error: ItemError| {
            world.send_to_player(
                object_id,
                &ServerPacket::MoveItem(MoveItemPacket {
                    grid: MirGridType::Equipment,
                    from,
                    to,
                    success: false,
                })
                .encode_frame(),
            );
            error
        };
        if from < 0 || to < 0 {
            return Err(fail(
                self,
                ItemError::SlotOutOfRange {
                    grid: MirGridType::Equipment,
                    index: from,
                },
            ));
        }
        let (from, to) = (from as usize, to as usize);
        let Some(player) = self.players.get(&object_id) else {
            return Err(ItemError::NotInWorld(object_id));
        };
        if from >= INVENTORY_SIZE || to >= INVENTORY_SIZE {
            let index = if from >= INVENTORY_SIZE { from } else { to };
            return Err(fail(
                self,
                ItemError::SlotOutOfRange {
                    grid: MirGridType::Equipment,
                    index: index as i32,
                },
            ));
        }

        // 1) 穿上：背包格 → 装备槽（Crystal `EquipItem`，`:5658-5690`）。
        //    只要 `to` 是合法装备槽、`from` 是背包格且物品真的能穿，就优先按「穿」解释——
        //    避免「背包 5 号格的戒指要戴到 5 号槽」被误判成脱下。
        if to < player.equipment.len()
            && from < player.inventory.len()
            && player.inventory[from].is_some()
        {
            let item = player.inventory[from].as_ref().expect("刚检查过");
            if self.can_equip_item(object_id, item, to) {
                return self.equip_from(object_id, from, to);
            }
        }
        // 2) 脱下：装备槽 → 背包格（Crystal `RemoveItem`，`:5118-5124`）。
        if from < player.equipment.len()
            && player.equipment[from].is_some()
            && to < player.inventory.len()
        {
            if player.inventory[to].is_some() {
                return Err(ItemError::SlotOccupied {
                    grid: MirGridType::Inventory,
                    index: to as i32,
                });
            }
            return self.unequip_to(object_id, from, to);
        }
        // 3) 能走到的「想穿但穿不上」：给出准确的错误。
        if to < player.equipment.len() && from < player.inventory.len() {
            if let Some(item) = &player.inventory[from] {
                return Err(fail(
                    self,
                    ItemError::CannotEquip {
                        item_index: item.item_index,
                        slot: to as i32,
                    },
                ));
            }
        }
        Err(fail(
            self,
            ItemError::SlotOutOfRange {
                grid: MirGridType::Equipment,
                index: from as i32,
            },
        ))
    }

    /// 穿上：把 `inventory[from]` 与 `equipment[to]` 交换（Crystal `EquipItem` 的核心，`:5674-5678`）。
    fn equip_from(&mut self, object_id: u32, from: usize, to: usize) -> Result<(), ItemError> {
        let Some(player) = self.players.get(&object_id) else {
            return Err(ItemError::NotInWorld(object_id));
        };
        let Some(item) = player.inventory[from].clone() else {
            return Err(ItemError::EmptySlot {
                grid: MirGridType::Inventory,
                index: from as i32,
            });
        };
        if !self.can_equip_item(object_id, &item, to) {
            self.send_to_player(
                object_id,
                &ServerPacket::MoveItem(MoveItemPacket {
                    grid: MirGridType::Equipment,
                    from: from as i32,
                    to: to as i32,
                    success: false,
                })
                .encode_frame(),
            );
            return Err(ItemError::CannotEquip {
                item_index: item.item_index,
                slot: to as i32,
            });
        }
        let map = player.map.clone();
        let position = player.position;
        let before = self.looks(object_id);
        if let Some(player) = self.players.get_mut(&object_id) {
            let replaced = player.equipment[to].replace(item.clone());
            player.inventory[from] = replaced;
        }
        // `Enqueue(new S.EquipItem { Grid = grid, UniqueID = id, To = to, Success = true });`（`:5682-5683`）
        self.send_to_player(
            object_id,
            &ServerPacket::EquipItem(EquipItemPacket {
                grid: MirGridType::Inventory,
                unique_id: item.unique_id,
                to: to as i32,
                success: true,
            })
            .encode_frame(),
        );
        // `RefreshStats();`（`:5687`）
        self.refresh_stats(object_id);
        self.broadcast_looks_if_changed(object_id, &map, position, before);
        Ok(())
    }

    /// 脱下：把 `equipment[from]` 放进 `inventory[to]`（Crystal `RemoveItem` 的核心，`:5118-5122`）。
    fn unequip_to(&mut self, object_id: u32, from: usize, to: usize) -> Result<(), ItemError> {
        let Some(player) = self.players.get(&object_id) else {
            return Err(ItemError::NotInWorld(object_id));
        };
        let Some(item) = player.equipment[from].clone() else {
            return Err(ItemError::EmptySlot {
                grid: MirGridType::Equipment,
                index: from as i32,
            });
        };
        if player.inventory[to].is_some() {
            return Err(ItemError::SlotOccupied {
                grid: MirGridType::Inventory,
                index: to as i32,
            });
        }
        let map = player.map.clone();
        let position = player.position;
        let before = self.looks(object_id);
        if let Some(player) = self.players.get_mut(&object_id) {
            player.equipment[from] = None;
            player.inventory[to] = Some(item.clone());
        }
        // `Enqueue(new S.RemoveItem { Grid = grid, UniqueID = id, To = to, Success = true });`（`:5123-5124`）
        self.send_to_player(
            object_id,
            &ServerPacket::RemoveItem(RemoveItemPacket {
                grid: MirGridType::Equipment,
                unique_id: item.unique_id,
                to: to as i32,
                success: true,
            })
            .encode_frame(),
        );
        self.refresh_stats(object_id);
        self.broadcast_looks_if_changed(object_id, &map, position, before);
        Ok(())
    }

    /// 外观变了就广播 `S.PlayerUpdate`（Crystal `UpdateLooks`，`HumanObject.cs:7058-7061`）。
    fn broadcast_looks_if_changed(
        &self,
        object_id: u32,
        map: &str,
        position: Point,
        before: Looks,
    ) {
        let after = self.looks(object_id);
        if after == before {
            return;
        }
        // Crystal `Broadcast` 不发给发布者本人（`MapObject.cs:316-330`）。
        let packet = ServerPacket::PlayerUpdate(PlayerUpdate {
            object_id,
            light: after.light,
            weapon: after.weapon,
            weapon_effect: after.weapon_effect,
            armour: after.armour,
            wing_effect: after.wings,
        })
        .encode_frame();
        self.broadcast(map, position, Some(object_id), &packet, object_id);
    }

    /// 能否把这件装备穿到 `slot` 号槽：Crystal `HumanObject.CanEquipItem`（`HumanObject.cs:7661-7823`）。
    ///
    /// 检查顺序与源码一致：槽位类型 → 性别 → 职业 → `RequiredType`/`RequiredAmount` → 重量。
    ///
    /// 简化：`RidingMount` 时不能换装（`:7817-7820`）、诅咒/封印/灵魂绑定（`:5624-5656`）没做——
    /// 本作还没有骑乘与绑定系统。
    #[must_use]
    pub fn can_equip_item(&self, object_id: u32, item: &UserItem, slot: usize) -> bool {
        let Some(player) = self.players.get(&object_id) else {
            return false;
        };
        let Some(info) = self.item_info(item.item_index) else {
            return false;
        };
        if slot >= EQUIPMENT_SLOTS
            || slot >= player.equipment.len()
            || !slot_accepts(info.item_type, slot)
        {
            return false;
        }
        // 性别：`:7723-7733`
        let gender_ok = match player.gender {
            MirGender::Male => info.required_gender.contains(RequiredGender::Male),
            MirGender::Female => info.required_gender.contains(RequiredGender::Female),
        };
        if !gender_ok {
            return false;
        }
        // 职业：`:7736-7754`
        let class_bit = match player.class {
            MirClass::Warrior => RequiredClass::Warrior,
            MirClass::Wizard => RequiredClass::Wizard,
            MirClass::Taoist => RequiredClass::Taoist,
            MirClass::Assassin => RequiredClass::Assassin,
            MirClass::Archer => RequiredClass::Archer,
        };
        if !info.required_class.contains(class_bit) {
            return false;
        }
        // 需求属性：`:7756-7806`
        let amount = i32::from(info.required_amount);
        let level = i32::from(player.level);
        let value = match info.required_type {
            RequiredType::Level => level,
            RequiredType::MaxAC => player.stats.max_ac,
            RequiredType::MaxMAC => player.stats.max_mac,
            RequiredType::MaxDC => player.stats.max_dc,
            RequiredType::MaxMC => player.stats.max_mc,
            RequiredType::MaxSC => player.stats.max_sc,
            RequiredType::MaxLevel => {
                if level > amount {
                    return false;
                }
                level
            }
            RequiredType::MinAC => player.stats.min_ac,
            RequiredType::MinMAC => player.stats.min_mac,
            RequiredType::MinDC => player.stats.min_dc,
            RequiredType::MinMC => player.stats.min_mc,
            RequiredType::MinSC => player.stats.min_sc,
        };
        if info.required_type != RequiredType::MaxLevel && value < amount {
            return false;
        }
        // 重量：`:7808-7815`
        let weight = item_weight(info, item);
        let replaced = player.equipment[slot]
            .as_ref()
            .and_then(|old| {
                self.item_info(old.item_index)
                    .map(|old_info| item_weight(old_info, old))
            })
            .unwrap_or(0);
        let (hand, wear) = self.equipment_weights(object_id);
        if matches!(info.item_type, ItemType::Weapon | ItemType::Torch) {
            if weight - replaced + hand > player.stats.hand_weight {
                return false;
            }
        } else if weight - replaced + wear > player.stats.wear_weight {
            return false;
        }
        true
    }

    /// 怪物死亡的掉落：Crystal `MonsterObject.Drop`（`:1093-1145`）。
    ///
    /// 逐条照抄：
    /// * `if (Master == null && EXPOwner != null) Drop();`（`:997-998`）——没有击杀者就不掉；
    /// * 逐条 `AttemptDrop`（本地实现是 [`mir_db::DropTable::roll`]）；
    /// * 金币走 `DropGold`（`:1169-1190`），物品走 `DropItem`（`:1147-1167`），
    ///   物品落地时 `Owner = EXPOwner`、`OwnerTime = now + 1 分钟`；
    /// * **`DropItem` 找不到落点就 `return`**（`:1141`）——后面的掉落直接不掉了，这点也照抄。
    ///
    /// 截胡：Crystal `MonsterObject.cs:1130-1138` 在落地前调 `CheckGroupQuestItem`，
    /// 命中「有未完成收集任务」的掉落直接进任务物品栏（[`crate::quest_inventory`]），
    /// 不再落地；本作在 [`World::drop_monster_loot`] 的掉落循环里照做。
    ///
    /// 简化：`NoDropMonster` 地图开关、稀有度/会员掉率加成（`:1098-1102`）、
    /// `GMMade` 标记、`drop.QuestRequired`（数据层还没有这个字段）都未实现。
    pub(crate) fn drop_monster_loot(&mut self, monster_id: u32) {
        // 用 `World::monster`（monster_slots 下标）代替遍历全部怪物。
        let Some(monster) = self.monster(monster_id) else {
            return;
        };
        if !monster.dead {
            return;
        }
        let Some(owner) = monster.expo_owner else {
            return;
        };
        let map = monster.map.clone();
        let position = monster.position;
        let name = monster.kind.name.clone();
        let Some(data) = self.game_data.clone() else {
            return;
        };
        let Some(table) = data.monster_drop_table(&name) else {
            return;
        };
        let reward = table.roll(&mut self.rng, DROP_RATE);

        // `if (scaledGold > 0) DropGold(scaledGold);`（`:1114-1117`）
        if reward.gold > 0 {
            self.drop_monster_gold(&map, position, reward.gold, owner);
        }
        // `foreach (var dropItem in reward.Items) { ... if (!DropItem(item)) return; }`（`:1119-1142`）
        for item_name in &reward.items {
            let Some(info) = data.item_by_name(item_name).cloned() else {
                // Crystal 在载入掉落表时就用 `Envir.GetItemInfo` 过滤掉了未知物品
                // （`MonsterInfo.cs:356-357`），所以这里只可能是数据不一致。
                tracing::debug!(item = %item_name, "掉落表引用了不存在的物品模板，已跳过");
                continue;
            };
            let item = self.create_drop_item(&info);
            // Crystal `MonsterObject.cs:1130-1138`：击杀者（组队时含同图范围内的队员）有
            // **未完成的收集任务**时，这件掉落直接进任务物品栏，不再落地。
            if self.check_group_quest_item(owner, &item) {
                continue;
            }
            let Some(cell) = self.drop_cell(&map, position) else {
                tracing::warn!(monster = %name, "周围没有空格，本次掉落的剩余物品被丢弃");
                return;
            };
            self.spawn_ground_item(&map, cell, Some(item), 0, Some(owner));
        }
    }

    /// 怪物掉金币：Crystal `MonsterObject.DropGold`（`:1169-1190`）。
    ///
    /// `Settings.DropGold` 默认为 `true`（`Settings.cs:229`），所以金币**落在地上**而不是
    /// 直接进击杀者背包；按 `MaxDropGold = 2000` 拆成多堆（`Settings.cs:228`）。
    ///
    /// 与玩家主动丢金币的 [`World::drop_gold`] 区分：这里带归属（`owner`），
    /// 掉落半径按 `Settings.DropRange = 4`。
    fn drop_monster_gold(&mut self, map: &str, position: Point, gold: u32, owner: u32) {
        let count = if gold / MAX_DROP_GOLD == 0 {
            1
        } else {
            gold / MAX_DROP_GOLD + 1
        };
        for index in 0..count {
            let amount = if index + 1 == count {
                gold % MAX_DROP_GOLD
            } else {
                MAX_DROP_GOLD
            };
            // 简化：Crystal 会连 0 金币的那一堆也建出来（`gold % MaxDropGold == 0` 时是 0），
            // 客户端于是显示「金币 0」；本作跳过空堆，见 docs 的异常清单。
            if amount == 0 {
                continue;
            }
            let Some(cell) = self.drop_cell(map, position) else {
                return;
            };
            self.spawn_ground_item(map, cell, None, amount, Some(owner));
        }
    }

    /// 玩家死亡掉落：Crystal `HumanObject.DeathDrop`（`HumanObject.cs:1391-1567`）与
    /// `PlayerObject.RedDeathDrop`（`PlayerObject.cs:668-793`）。
    ///
    /// 两套规则（`PlayerObject.Die`，`:638-641`）：
    ///
    /// ```csharp
    /// if (PKPoints > 200) RedDeathDrop(LastHitter);
    /// else if (!InSafeZone) DeathDrop(LastHitter);
    /// ```
    ///
    /// * **普通死亡掉落**（`DeathDrop`）：
    ///   - 装备：堆叠 > 1 时按 `RandomomRange(10, 8)` 抽掉 `ceil(Count / 10 * 百分比)` 件；
    ///     单件按 `Random.Next(30) == 0`（1/30）掉；
    ///   - 背包：堆叠 > 1 时 `RandomomRange(10, 8)`（抽到 0 就跳过）；单件按 `Random.Next(10) == 0`。
    /// * **红名死亡掉落**（`RedDeathDrop`，`PKPoints > 200`）：
    ///   - 装备：堆叠 `RandomomRange(10, 4)`、单件 `Random.Next(10) == 0`（1/10）；
    ///   - 背包：**全部无条件掉光**（`PlayerObject.cs:750-790` 没有任何随机判定）。
    ///
    /// 两种掉落都跳过 `BindMode.DontDeathdrop`、婚戒、封印中与租借物品。
    ///
    /// 归属：Crystal 的 `ItemObject(dropper, item, deathDrop: true)` **不设置 `Owner`**
    /// （`ItemObject.cs:53-82`），所以死亡掉落的物品谁都能捡（凶手也可以）。
    /// 存活时长是 `Settings.PlayerDiedItemTimeOut = 120` 分钟（`ItemObject.cs:55-56`）。
    ///
    /// 简化（详见 `docs/world/pk.md` 的异常清单）：
    /// * `WeddingRing`、`SealedInfo`、租借物品一律跳过（Crystal 会按具体条件单独处理/邮寄）；
    /// * `BreakOnDeath` 的物品直接删除（Crystal 删完后还会继续走掉落分支，属于 bug）；
    /// * `GlobalDropNotify` 全服公告与 `Report.ItemChanged` 统计未接。
    pub(crate) fn drop_player_death_loot(&mut self, player_id: u32, red: bool) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        if player.dead {
            return;
        }
        let map = player.map.clone();
        let position = player.position;

        for slot in 0..EQUIPMENT_SLOTS {
            let Some(item) = self.slots_item(player_id, slot, true) else {
                continue;
            };
            let Some(info) = self.item_info(item.item_index).cloned() else {
                continue;
            };
            if !death_drop_allowed(&info, &item) {
                continue;
            }
            if info.bind.contains(BindMode::BreakOnDeath) {
                self.remove_death_drop_item(player_id, slot, true, &item, item.count);
                continue;
            }
            let count = if item.count > 1 {
                let percent = self.randomom_range(10, if red { 4 } else { 8 });
                count_for_percent(item.count, percent)
            } else {
                let chance = if red { 10 } else { 30 };
                if self.rng.next_bounded(chance) == 0 {
                    item.count
                } else {
                    0
                }
            };
            if count == 0 {
                continue;
            }
            self.death_drop_one(player_id, slot, true, &map, position, &item, &info, count);
        }

        for slot in 0..INVENTORY_SIZE {
            let Some(item) = self.slots_item(player_id, slot, false) else {
                continue;
            };
            let Some(info) = self.item_info(item.item_index).cloned() else {
                continue;
            };
            if !death_drop_allowed(&info, &item) {
                continue;
            }
            let count = if red {
                // `RedDeathDrop` 的背包分支没有任何随机判定：全部掉光（`PlayerObject.cs:750-790`）。
                item.count
            } else if item.count > 1 {
                let percent = self.randomom_range(10, 8);
                if percent == 0 {
                    continue;
                }
                count_for_percent(item.count, percent)
            } else if self.rng.next_bounded(10) == 0 {
                item.count
            } else {
                0
            };
            if count == 0 {
                continue;
            }
            self.death_drop_one(player_id, slot, false, &map, position, &item, &info, count);
        }

        // `RefreshStats()`（`HumanObject.cs:1566`）：装备掉了要重算属性。
        self.refresh_stats(player_id);
    }

    /// 死亡掉落的通用排除条件（`BindMode.DontDeathdrop`、婚戒、封印、租借）见
    /// [`death_drop_allowed`]。
    fn slots_item(&self, player_id: u32, slot: usize, equipment: bool) -> Option<UserItem> {
        let player = self.players.get(&player_id)?;
        let slots = if equipment {
            &player.equipment
        } else {
            &player.inventory
        };
        slots.get(slot)?.clone()
    }

    /// 从背包/装备里移除死亡掉落的一部分并通知本人。
    fn remove_death_drop_item(
        &mut self,
        player_id: u32,
        slot: usize,
        equipment: bool,
        item: &UserItem,
        count: u16,
    ) {
        if let Some(player) = self.players.get_mut(&player_id) {
            let slots = if equipment {
                &mut player.equipment
            } else {
                &mut player.inventory
            };
            let remove = slots
                .get(slot)
                .and_then(Option::as_ref)
                .is_some_and(|stored| count >= stored.count);
            if remove {
                if let Some(target) = slots.get_mut(slot) {
                    *target = None;
                }
            } else if let Some(stored) = slots.get_mut(slot).and_then(Option::as_mut) {
                stored.count -= count;
            }
        }
        // `Enqueue(new S.DeleteItem { UniqueID = item.UniqueID, Count = count })`。
        self.send_to_player(
            player_id,
            &ServerPacket::DeleteItem(DeleteItemPacket {
                unique_id: item.unique_id,
                count,
            })
            .encode_frame(),
        );
    }

    /// 把一件（或一部分）死亡掉落物放到地上。
    #[allow(clippy::too_many_arguments)]
    fn death_drop_one(
        &mut self,
        player_id: u32,
        slot: usize,
        equipment: bool,
        map: &str,
        position: Point,
        item: &UserItem,
        info: &ItemInfo,
        count: u16,
    ) {
        let Some(cell) = self.drop_cell(map, position) else {
            // `if (!DropItem(...)) continue;`（`HumanObject.cs:1473`）：找不到落点就保住这件物品。
            return;
        };
        let dropped = if count >= item.count {
            item.clone()
        } else {
            let mut fresh = self.create_fresh_item(info);
            fresh.count = count;
            fresh
        };
        self.spawn_ground_item_with_expire(
            map,
            cell,
            Some(dropped),
            0,
            None,
            PLAYER_DIED_ITEM_EXPIRE_MS,
        );
        self.remove_death_drop_item(player_id, slot, equipment, item, count);
    }

    /// Crystal `Envir.RandomomRange`（`Server/MirEnvir/Envir.cs:4531-4536`）：
    /// 掷 `count` 次 `Random.Next(rate)`，统计命中 0 的次数。
    fn randomom_range(&mut self, count: u32, rate: u32) -> u32 {
        let mut hits = 0;
        for _ in 0..count {
            if self.rng.next_bounded(rate) == 0 {
                hits += 1;
            }
        }
        hits
    }

    /// 掉落落点：Crystal `ItemObject.Drop(distance)`（`ItemObject.cs:190-275`）。
    ///
    /// 原文的扫描方式（物品掉落用 `distance = Settings.DropRange = 4`，
    /// 玩家丢金币用写死的 `5`，见 [`DROP_GOLD_RANGE`]）：
    ///
    /// ```csharp
    /// for (int d = 0; d <= distance; d++)
    ///   for (int y = Y - d; y <= Y + d; y++)
    ///     for (int x = X - d; x <= X + d; x += Math.Abs(y - Y) == d ? 1 : d * 2)
    /// ```
    ///
    /// 也就是从脚下开始一圈一圈往外扫：**第一个完全空的格子**直接返回；
    /// 全是「堆到上限（`DropStackSize = 5`）」或者「被阻挡」的格子时，
    /// 记住物品最少的那一格（`best`），扫完一圈都没有空格就放那里；再没有就返回 `None`
    /// （调用方按 Crystal 的语义放弃这件掉落）。
    ///
    /// 关于「随机 1..3 格」：这一版 Crystal **没有**随机落点，`ItemObject.Drop` 完全确定，
    /// 唯一带随机的是「掉不掉」（`AttemptDrop`）与「掉多少金币」。本作照抄确定性扫描，
    /// 好处是同一 seed 的掉落位置可回放。
    ///
    /// 简化：Crystal 还会跳过本图的 `Movement` 起点（传送门，`ItemObject.cs:211-220`）；
    /// 世界层没有 `MapInfo.Movements` 的运行时表，这里省略。
    fn drop_cell(&self, map: &str, center: Point) -> Option<Point> {
        self.drop_cell_at(map, center, DROP_RANGE)
    }

    /// 同 [`World::drop_cell`]，但可以指定搜索半径（玩家丢金币传 5）。
    fn drop_cell_at(&self, map: &str, center: Point, distance: i32) -> Option<Point> {
        let (max_x, max_y) = self.map_bounds(map);
        let mut best: Option<(Point, usize)> = None;
        for distance in 0..=distance {
            for y in (center.y - distance)..=(center.y + distance) {
                if y < 0 {
                    continue;
                }
                if y > max_y {
                    break;
                }
                let step = if (y - center.y).abs() == distance {
                    1
                } else {
                    distance * 2
                };
                if step <= 0 {
                    continue;
                }
                let mut x = center.x - distance;
                while x <= center.x + distance {
                    let cell = Point::new(x, y);
                    if x >= 0 && x <= max_x && self.walkable_on(map, cell) {
                        // Crystal `ItemObject.Drop` 逐格看 `cell.Objects`（`ItemObject.cs:190-275`）；
                        // 这里只查这一格，不遍历全部地面物品/玩家/怪物。
                        let entries = self.aoi_in_cell(map, cell);
                        let count = entries
                            .iter()
                            .filter(|entry| entry.kind == AoiKind::Item)
                            .count();
                        let blocked = entries.iter().any(|entry| match entry.kind {
                            AoiKind::Monster => self
                                .monster(entry.object_id)
                                .is_some_and(|monster| !monster.dead && monster.position == cell),
                            AoiKind::Player => self
                                .players
                                .get(&entry.object_id)
                                .is_some_and(|player| !player.dead && player.position == cell),
                            AoiKind::Npc | AoiKind::Item => false,
                        });
                        if !blocked && count < DROP_STACK_SIZE {
                            if count == 0 {
                                return Some(cell);
                            }
                            if best.is_none_or(|(_, best_count)| count < best_count) {
                                best = Some((cell, count));
                            }
                        }
                    }
                    x += step;
                }
            }
        }
        best.map(|(cell, _)| cell)
    }
}

/// 死亡掉落的通用排除条件：`BindMode.DontDeathdrop`（`HumanObject.cs:1407`、`:1502`
/// 与 `PlayerObject.cs:679`、`:757`）、婚戒、封印中、租借物品。
///
/// 简化：Crystal 对婚戒只跳过「左戒指槽那枚」、封印只看是否过期、租借会先邮寄给出租人；
/// 本作没有这三套系统，统一按「不参与死亡掉落」处理（物品留在身上）。
fn death_drop_allowed(info: &ItemInfo, item: &UserItem) -> bool {
    !info.bind.contains(BindMode::DontDeathdrop)
        && item.wedding_ring == -1
        && item.sealed_info.is_none()
        && item.rental_information.is_none()
}

/// 死亡掉落的数量：Crystal `(ushort)Math.Ceiling(item.Count / 10F * percent)`
/// （`HumanObject.cs:1441`、`:1518`，`PlayerObject.cs:700`）。
///
/// 用整数 `ceil(count * percent / 10)` 精确复现（`percent ∈ 0..=8`），再夹到 `count`。
fn count_for_percent(count: u16, percent: u32) -> u16 {
    let raw = u32::from(count) * percent;
    u16::try_from(raw.div_ceil(10).min(u32::from(count))).unwrap_or(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(index: i32, item_type: ItemType, shape: i16, stack_size: u16) -> ItemInfo {
        ItemInfo {
            index,
            name: format!("物品{index}"),
            item_type,
            shape,
            stack_size,
            ..ItemInfo::default()
        }
    }

    #[test]
    fn friendly_name_strips_trailing_digits_and_brackets() {
        // `ItemData.cs:52-62` 的两条正则。
        assert_eq!(friendly_name("金创药(小量)"), "金创药(小量)");
        assert_eq!(friendly_name("金创药(小量)10"), "金创药(小量)");
        assert_eq!(friendly_name("[7天]屠龙"), "屠龙");
        assert_eq!(friendly_name("[限时]祝福油[绝版]7"), "祝福油");
        assert_eq!(friendly_name("屠龙"), "屠龙");
    }

    #[test]
    fn amulet_images_follow_the_stack_thresholds() {
        let amulet = sample(1, ItemType::Amulet, 0, 300);
        // `ItemData.cs:653-657`：>=300 → 3662，>=200 → 3661，其余 3660。
        assert_eq!(item_image(&amulet, 300), 3662);
        assert_eq!(item_image(&amulet, 299), 3661);
        assert_eq!(item_image(&amulet, 199), 3660);

        let grey = sample(2, ItemType::Amulet, 1, 200);
        assert_eq!(item_image(&grey, 150), 3675);
        assert_eq!(item_image(&grey, 100), 2960);
        assert_eq!(item_image(&grey, 50), 3674);
        assert_eq!(item_image(&grey, 49), 3673);

        let yellow = sample(3, ItemType::Amulet, 2, 200);
        assert_eq!(item_image(&yellow, 150), 3672);
        assert_eq!(item_image(&yellow, 100), 2961);
        assert_eq!(item_image(&yellow, 50), 3671);
        assert_eq!(item_image(&yellow, 49), 3670);

        // 其它物品/其它 shape 就是模板图标。
        let sword = sample(4, ItemType::Weapon, 0, 1);
        assert_eq!(item_image(&sword, 7), sword.image);
        let unknown_shape = sample(5, ItemType::Amulet, 9, 10);
        assert_eq!(item_image(&unknown_shape, 300), unknown_shape.image);
    }

    #[test]
    fn slot_mapping_matches_crystal_can_equip_item() {
        // `HumanObject.cs:7663-7721` 的 switch。
        assert!(slot_accepts(ItemType::Weapon, 0));
        assert!(slot_accepts(ItemType::Armour, 1));
        assert!(slot_accepts(ItemType::Helmet, 2));
        assert!(slot_accepts(ItemType::Torch, 3));
        assert!(slot_accepts(ItemType::Necklace, 4));
        assert!(slot_accepts(ItemType::Bracelet, 5));
        assert!(slot_accepts(ItemType::Bracelet, 6));
        assert!(slot_accepts(ItemType::Amulet, 6), "BraceletR 也收护身符");
        assert!(slot_accepts(ItemType::Ring, 7));
        assert!(slot_accepts(ItemType::Ring, 8));
        assert!(slot_accepts(ItemType::Amulet, 9));
        assert!(slot_accepts(ItemType::Belt, 10));
        assert!(slot_accepts(ItemType::Boots, 11));
        assert!(slot_accepts(ItemType::Stone, 12));
        assert!(slot_accepts(ItemType::Mount, 13));
        // 不接受的情况。
        assert!(!slot_accepts(ItemType::Potion, 0));
        assert!(!slot_accepts(ItemType::Weapon, 1));
        assert!(!slot_accepts(ItemType::Amulet, 5), "护身符不能在左手镯");
        assert!(!slot_accepts(ItemType::Weapon, 14), "14 号不是装备槽");
        // 默认落位。
        assert_eq!(equipment_slot_for(ItemType::Weapon), Some(0));
        assert_eq!(equipment_slot_for(ItemType::Ring), Some(7));
        assert_eq!(equipment_slot_for(ItemType::Bracelet), Some(5));
        assert_eq!(equipment_slot_for(ItemType::Potion), None);
    }

    #[test]
    fn item_weight_counts_stacks_but_not_amulets() {
        let mut potion = sample(1, ItemType::Potion, 0, 20);
        potion.weight = 2;
        let mut item = UserItem {
            item_index: 1,
            count: 5,
            ..UserItem::default()
        };
        assert_eq!(
            item_weight(&potion, &item),
            10,
            "Weight * Count（ItemData.cs:319）"
        );

        let mut amulet = potion.clone();
        amulet.item_type = ItemType::Amulet;
        assert_eq!(item_weight(&amulet, &item), 2, "护身符按件计重");

        item.count = 0;
        assert_eq!(item_weight(&potion, &item), 0, "数量 0 时重量 0");
    }
}
