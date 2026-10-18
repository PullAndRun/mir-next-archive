//! 音效事件的纯数据层：事件 → `resource/wav/<文件>.wav` 的映射。
//!
//! 这个模块**不依赖任何音频库**（音频后端只在 `mir-client`），只做三件事：
//!
//! 1. 定义客户端会用到的音效事件（[`SoundId`]）；
//! 2. 把事件翻译成 Crystal 真实使用的 WAV 文件名（[`sound_for_event`]）；
//! 3. 把技能号翻译成 Crystal 的施法音文件名（[`spell_sound`]）。
//!
//! # 文件名是怎么来的
//!
//! Crystal 的 `SoundManager.PlaySound(int index)` 先查 `Sound/SoundList.lst`
//! （`Client/MirSounds/SoundManager.cs:85-92`，`SoundList.LoadSoundList` 读的是
//! `索引: 文件名` 的文本表，`Client/MirSounds/SoundList.cs:13-31`）。查不到时才退回
//! **编号公式**（`Client/MirSounds/SoundManager.cs:87-89`）：
//!
//! ```text
//! index <= 20000 → "{index / 10:000}-{index % 10}.wav"   例如 10001 → "1000-1.wav"
//! index >  20000 → "M{(index - 20000) / 10}-{index % 10}.wav"
//! ```
//!
//! 语料里 `SoundList.lst` 把这些编号映射成更直观的名字（`1.wav`…`145.wav`
//! 对应移动/挥砍/受击/界面音，`levelup.wav`、`pet_pickup.wav` 单独命名），
//! 所以本模块直接用**表里的真实文件名**，而不是公式算出来的名字。
//!
//! # 事件 → 常量 → 文件
//!
//! | [`SoundId`] | Crystal 常量（`SoundList.cs`） | 编号 | `SoundList.lst` | 文件 |
//! |---|---|---|---|---|
//! | [`SoundId::Walk`] | `WalkGroundL`（`:61`） | 10001 | `:2` | `1.wav` |
//! | [`SoundId::Run`] | `RunGroundL`（`:63`） | 10003 | `:4` | `3.wav` |
//! | [`SoundId::Swing`] | `SwingShort`（`:102`） | 10050 | `:36` | `50.wav` |
//! | [`SoundId::Hit`] | `StruckBodySword`（`:119`） | 10070 | `:52` | `70.wav` |
//! | [`SoundId::Struck`] | `MaleFlinch` / `FemaleFlinch`（`:131-132`） | 10138 / 10139 | `:83-84` | `138.wav` / `139.wav` |
//! | [`SoundId::Die`] | `MaleDie` / `FemaleDie`（`:133-134`） | 10144 / 10145 | `:85-86` | `144.wav` / `145.wav` |
//! | [`SoundId::LevelUp`] | `LevelUp`（`:49`） | 10156 | `:93` | `levelup.wav` |
//! | [`SoundId::Pickup`] | `PetPickup`（`:177`） | 10520 | `:145` | `pet_pickup.wav` |
//! | [`SoundId::Gold`] | `Gold`（`:44`） | 10106 | `:68` | `106.wav` |
//! | [`SoundId::Drink`] | `ClickDrug`（`:46`） | 10108 | `:70` | `108.wav` |
//! | [`SoundId::Cast`] | 技能音 `20000 + Spell × 10`（`SoundManager.cs:87-89`） | 20000+ | — | `M<技能号>-0.wav`（缺省 `M31-0.wav` = 火球术） |
//! | [`SoundId::NpcDialog`] | `ButtonA`（`:41`，简化，见下） | 10103 | `:65` | `103.wav` |
//!
//! # 简化
//!
//! * **走路/跑步**：Crystal 按脚下地块选 8 种左右脚 + 走/跑的音（`PlayerObject.cs:3579-3599`
//!   调 `PlayWemadeStepSound` 等，`:3601-4208` 里按 `MapControl.MBack` 的
//!   `TileAnimationImage` 与地图名挑 `WalkStoneL` / `WalkCaveL` / …）。本作不解析地块音效表，
//!   统一用「普通地面」的 [`SoundId::Walk`] / [`SoundId::Run`]
//!   （`WalkGroundL` / `RunGroundL`，`SoundList.cs:61,63`）。
//! * **受击**：Crystal 按武器类型挑 `StruckBodySword` / `Axe` / `LongStick` / `Fist`
//!   （`PlayerObject.cs:4700-4762` 的 `StruckBodySword + add`）以及性别挑 `MaleFlinch` /
//!   `FemaleFlinch`（`PlayerObject.cs:4763`）。本作只有一个 [`SoundId::Struck`]，
//!   需要性别时用 [`sound_for_sex`] 取 `MaleFlinch` / `FemaleFlinch`
//!   （`PlayerObject.cs:624-625`：`DieSound`/`FlinchSound` 在建对象时按性别定）。
//! * **喝药**：`SoundList.EatDrug`（`SoundList.cs:45`，10107 → `107.wav`）在 Crystal 客户端
//!   **从未被调用**（全仓库只有定义）；真正响的是点药品时 `MirItemCell.cs:2048` 的
//!   `SoundList.ClickDrug`（`:46`，10108 → `108.wav`），所以这里用后者。
//! * **捡取**：Crystal 只有宠物拾取音 `SoundList.PetPickup`（`MonsterObject.cs:4266`），
//!   玩家捡东西没有专门音效；本作复用它当「拾取」提示音。
//! * **打开 NPC 对话**：Crystal 没有专用音效（`GameScene.cs:3891-3913` 只 Show/Hide 面板），
//!   面板按钮音是 `ButtonA`（`MirControl.cs:823-824` 点击时 `PlaySound(Sound)`，
//!   `MainDialogs.cs:1812` 给按钮配的就是 `SoundList.ButtonA`），这里复用它。
//! * **怪物攻击/死亡**：Crystal 用 `BaseSound = BaseImage * 10`（`MonsterObject.cs:217,601,1395`）
//!   加偏移（攻击 `+6`、死亡 `+9`，见 `MonsterObject.cs:3906,4087`），
//!   本作不按怪物外观取音，怪物攻击用 [`SoundId::Swing`]、死亡用 [`SoundId::Die`]。
//!
//! # 频控
//!
//! Crystal 没有「同一音效 N 毫秒内不重复」的冷却表；它的节流靠两件事：
//! `PlaySound(index, loop, delay)` 的延迟队列（`SoundManager.cs:79-82`）与
//! 30 秒一次的缓存回收（`SoundManager.cs:195-223`）。走路音效不糊是因为它只在
//! 步帧播放（`PlayerObject.cs:3596-3599`：跑动 `moveSound += 2`、
//! 第 4 帧 `moveSound++`）。本作要求更严格的 100 ms 冷却，常量见
//! [`SOUND_COOLDOWN_MS`]，由 `mir-client` 的播放器实现。

use mir_core::enums::Spell;

/// 同一音效两次播放之间的最小间隔（毫秒）。
///
/// 需求给的值；Crystal 自身没有等价的冷却表（见模块文档的「频控」一节）。
pub const SOUND_COOLDOWN_MS: u64 = 100;

/// 客户端音效事件。
///
/// 每个事件都对应 Crystal `Client/MirSounds/SoundList.cs` 里的一个（或一组）常量编号，
/// 具体文件见 [`sound_for_event`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SoundId {
    /// 走一步（`WalkGroundL`，`SoundList.cs:61`）。
    Walk,
    /// 跑一步（`RunGroundL`，`SoundList.cs:63`）。
    Run,
    /// 挥空（`SwingShort`，`SoundList.cs:102`）。
    Swing,
    /// 攻击命中/受击者的身体被打中（`StruckBodySword`，`SoundList.cs:119`）。
    Hit,
    /// 自己受击（`MaleFlinch` / `FemaleFlinch`，`SoundList.cs:131-132`）。
    Struck,
    /// 死亡（`MaleDie` / `FemaleDie`，`SoundList.cs:133-134`）。
    Die,
    /// 升级（`LevelUp`，`SoundList.cs:49`）。
    LevelUp,
    /// 拾取物品（`PetPickup`，`SoundList.cs:177`；简化，见模块文档）。
    Pickup,
    /// 得到/失去金币（`Gold`，`SoundList.cs:44`）。
    Gold,
    /// 喝药（`ClickDrug`，`SoundList.cs:46`）。
    Drink,
    /// 施法起手（技能音，`SoundManager.cs:87-89` 的 `M<技能号>-0.wav`）。
    Cast,
    /// 打开 NPC 对话（`ButtonA`，`SoundList.cs:41`；简化，见模块文档）。
    NpcDialog,
}

impl SoundId {
    /// 全部事件（固定顺序，测试与文档用）。
    pub const ALL: [Self; 12] = [
        Self::Walk,
        Self::Run,
        Self::Swing,
        Self::Hit,
        Self::Struck,
        Self::Die,
        Self::LevelUp,
        Self::Pickup,
        Self::Gold,
        Self::Drink,
        Self::Cast,
        Self::NpcDialog,
    ];

    /// 事件 → Crystal `SoundList` 里的编号（文档/日志用）。
    ///
    /// [`Self::Cast`] 的音效号随技能变化，这里返回「火球术」那一档
    /// `20000 + 31 × 10`（`SoundManager.cs:87-89`；具体技能请用 [`spell_sound`]）。
    #[must_use]
    pub const fn index(self) -> u32 {
        match self {
            Self::Walk => 10_001,      // SoundList.cs:61 WalkGroundL
            Self::Run => 10_003,       // SoundList.cs:63 RunGroundL
            Self::Swing => 10_050,     // SoundList.cs:102 SwingShort
            Self::Hit => 10_070,       // SoundList.cs:119 StruckBodySword
            Self::Struck => 10_138,    // SoundList.cs:131 MaleFlinch
            Self::Die => 10_144,       // SoundList.cs:133 MaleDie
            Self::LevelUp => 10_156,   // SoundList.cs:49 LevelUp
            Self::Pickup => 10_520,    // SoundList.cs:177 PetPickup
            Self::Gold => 10_106,      // SoundList.cs:44 Gold
            Self::Drink => 10_108,     // SoundList.cs:46 ClickDrug
            Self::Cast => 20_310,      // 20000 + Spell.FireBall(31) × 10，见 SoundManager.cs:87-89
            Self::NpcDialog => 10_103, // SoundList.cs:41 ButtonA
        }
    }

    /// 中文名（日志与 `docs/client.md` 用）。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Walk => "走路",
            Self::Run => "跑步",
            Self::Swing => "攻击挥空",
            Self::Hit => "攻击命中",
            Self::Struck => "受击",
            Self::Die => "死亡",
            Self::LevelUp => "升级",
            Self::Pickup => "拾取",
            Self::Gold => "金币",
            Self::Drink => "喝药",
            Self::Cast => "施法",
            Self::NpcDialog => "打开 NPC",
        }
    }
}

/// 事件 → 文件名（只返回文件名，不含目录；实际路径是 `<音效目录>/<文件名>`
/// = `resource/wav/<文件名>` 或 Crystal 完整音效集里的同名文件）。
///
/// 文件名来自 `SoundList.lst`（行号见模块文档的表）；施法音随技能变化，
/// 这里返回缺省的「火球术」那一档 `M31-0.wav`，要精确请用 [`spell_sound`]。
#[must_use]
pub const fn sound_for_event(event: SoundId) -> Option<&'static str> {
    match event {
        SoundId::Walk => Some("1.wav"), // SoundList.lst:2，10001 WalkGroundL
        SoundId::Run => Some("3.wav"),  // SoundList.lst:4，10003 RunGroundL
        SoundId::Swing => Some("50.wav"), // SoundList.lst:36，10050 SwingShort
        SoundId::Hit => Some("70.wav"), // SoundList.lst:52，10070 StruckBodySword
        SoundId::Struck => Some("138.wav"), // SoundList.lst:83，10138 MaleFlinch
        SoundId::Die => Some("144.wav"), // SoundList.lst:85，10144 MaleDie
        SoundId::LevelUp => Some("levelup.wav"), // SoundList.lst:93，10156 LevelUp
        SoundId::Pickup => Some("pet_pickup.wav"), // SoundList.lst:145，10520 PetPickup
        SoundId::Gold => Some("106.wav"), // SoundList.lst:68，10106 Gold
        SoundId::Drink => Some("108.wav"), // SoundList.lst:70，10108 ClickDrug
        SoundId::Cast => Some("M31-0.wav"), // SoundManager.cs:87-89，20000 + 31×10（FireBall）
        SoundId::NpcDialog => Some("103.wav"), // SoundList.lst:65，10103 ButtonA
    }
}

/// 按性别取受击音（`SoundList.cs:131-132`；Crystal 在
/// `PlayerObject.cs:624-625` 建对象时就把 `FlinchSound` 定成男女两档）。
#[must_use]
pub const fn sound_for_sex(event: SoundId, female: bool) -> Option<&'static str> {
    match (event, female) {
        // SoundList.lst:83-84（10138 MaleFlinch / 10139 FemaleFlinch）
        (SoundId::Struck, false) => Some("138.wav"),
        (SoundId::Struck, true) => Some("139.wav"),
        // SoundList.lst:85-86（10144 MaleDie / 10145 FemaleDie）
        (SoundId::Die, false) => Some("144.wav"),
        (SoundId::Die, true) => Some("145.wav"),
        _ => sound_for_event(event),
    }
}

/// 技能 → 施法音文件名。
///
/// Crystal 在 `PlayerObject.cs:1538` 等处调 `PlaySound(20000 + (ushort)Spell * 10)`，
/// `SoundManager.cs:87-89` 把它拼成 `M{Spell}-0.wav`（`index % 10 == 0`）。
/// `Spell::None` 没有音效，返回 `None`。
#[must_use]
pub fn spell_sound(spell: Spell) -> Option<String> {
    let index = u16::from(spell as u8);
    if index == 0 {
        return None;
    }
    Some(format!("M{index}-0.wav"))
}

/// 编号公式：`index` → Crystal 兜底文件名（`SoundManager.cs:87-89`）。
///
/// 只在 `SoundList.lst` 里查不到某个编号时才用到；这里保留一份是为了让
/// 「编号 ↔ 文件名」的规则可被测试钉死（例如 `10001 → "1000-1.wav"`）。
#[must_use]
pub fn index_to_fallback_name(index: u32) -> String {
    if index > 20_000 {
        format!("M{}-{}.wav", (index - 20_000) / 10, index % 10)
    } else {
        format!("{}-{}.wav", index / 10, index % 10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_has_a_file_and_a_label() {
        for event in SoundId::ALL {
            let file = sound_for_event(event).unwrap_or_else(|| panic!("{event:?} 没有文件"));
            assert!(
                std::path::Path::new(file)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("wav")),
                "{event:?} 的文件名 {file} 不是 wav"
            );
            assert!(!event.label().is_empty());
            assert!(event.index() >= 10_000, "{event:?} 的编号 {event:?} 太小");
        }
    }

    #[test]
    fn mapping_matches_crystal_sound_list_lines() {
        // 逐条钉死 `SoundList.cs` 行号与 `SoundList.lst` 行号，防止以后手滑改错。
        assert_eq!(sound_for_event(SoundId::Walk), Some("1.wav"));
        assert_eq!(sound_for_event(SoundId::Run), Some("3.wav"));
        assert_eq!(sound_for_event(SoundId::Swing), Some("50.wav"));
        assert_eq!(sound_for_event(SoundId::Hit), Some("70.wav"));
        assert_eq!(sound_for_event(SoundId::LevelUp), Some("levelup.wav"));
        assert_eq!(sound_for_event(SoundId::Pickup), Some("pet_pickup.wav"));
        assert_eq!(sound_for_event(SoundId::Gold), Some("106.wav"));
        assert_eq!(sound_for_event(SoundId::Drink), Some("108.wav"));
        assert_eq!(sound_for_event(SoundId::NpcDialog), Some("103.wav"));
        // 编号也钉死：它们直接对应 `SoundList.cs` 的常量值。
        assert_eq!(SoundId::Walk.index(), 10_001);
        assert_eq!(SoundId::LevelUp.index(), 10_156);
        assert_eq!(SoundId::Pickup.index(), 10_520);
    }

    #[test]
    fn male_and_female_sounds_use_different_files() {
        assert_eq!(sound_for_sex(SoundId::Struck, false), Some("138.wav"));
        assert_eq!(sound_for_sex(SoundId::Struck, true), Some("139.wav"));
        assert_eq!(sound_for_sex(SoundId::Die, false), Some("144.wav"));
        assert_eq!(sound_for_sex(SoundId::Die, true), Some("145.wav"));
        // 其它事件与性别无关。
        assert_eq!(sound_for_sex(SoundId::Gold, true), Some("106.wav"));
    }

    #[test]
    fn spell_sound_follows_crystal_naming() {
        // SoundManager.cs:87-89：index > 20000 时是 "M{(index-20000)/10}-{index%10}"。
        // `Spell.FireBall` 的枚举值是 31（`mir-core/src/enums/generated.rs:5559`）。
        assert_eq!(spell_sound(Spell::FireBall).as_deref(), Some("M31-0.wav"));
        assert_eq!(spell_sound(Spell::None), None);
        // 公式与 [`index_to_fallback_name`] 必须一致。
        for spell in [Spell::FireBall, Spell::Healing, Spell::FireWall] {
            let index = 20_000 + u32::from(spell as u8) * 10;
            assert_eq!(
                spell_sound(spell).as_deref(),
                Some(index_to_fallback_name(index).as_str())
            );
        }
    }

    #[test]
    fn fallback_name_matches_sound_manager_formula() {
        // 10001 / 10 = 1000、10001 % 10 = 1 → "1000-1.wav"（SoundManager.cs:89）。
        assert_eq!(index_to_fallback_name(10_001), "1000-1.wav");
        assert_eq!(index_to_fallback_name(10_156), "1015-6.wav");
        assert_eq!(index_to_fallback_name(20_010), "M1-0.wav");
        assert_eq!(index_to_fallback_name(20_130), "M13-0.wav");
    }

    #[test]
    fn cooldown_is_one_hundred_millis() {
        assert_eq!(SOUND_COOLDOWN_MS, 100);
    }
}
