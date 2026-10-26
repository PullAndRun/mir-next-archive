//! 音效事件的纯数据层：事件 → `resource/wav/<文件>.wav` 的映射。
//!
//! 这个模块**不依赖任何音频库**（音频后端只在 `mir-client`），只做三件事：
//!
//! 1. 定义客户端会用到的音效事件（[`SoundId`]）；
//! 2. 把事件翻译成 Crystal 真实使用的 WAV 文件名（[`sound_for_event`]）；
//! 3. 把技能号翻译成 Crystal 的施法音文件名（[`spell_sound`]）；
//! 4. 把投射物技能翻译成出生/命中音文件名（[`projectile_sound`]，见下）；
//! 5. 把服务器 `MapInfo.Music` 编号/地图名翻译成背景音乐文件
//!    （[`music_for_index`] / [`music_for_map`]，见下）。
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

/// 投射物音效的基准技能号：`Spell.GreatFireBall = 34`（`Shared/Enums.cs:1168`）。
///
/// 125 包（`S.ObjectProjectile`）的 `Spell` 字段是 **154 的 `FireBounce`**
/// （`Shared/Enums.cs:1264`），但 Crystal `GameScene.cs:4697` 播的出生音写的是
/// `20000 + (ushort)Spell.GreatFireBall * 10 + 1`——**用 34 当基准**，所以文件名是
/// `M34-1.wav`（出生）与 `M34-2.wav`（命中，`:4709` 的 `+ 2`），不是 `M154-…`。
pub const PROJECTILE_SOUND_BASE: u32 = 34;
/// 投射物出生音效编号：`20000 + 34 × 10 + 1 = 20341`（`GameScene.cs:4697`）。
pub const PROJECTILE_SOUND_SPAWN_INDEX: u32 = 20_000 + PROJECTILE_SOUND_BASE * 10 + 1;
/// 投射物命中音效编号：`20000 + 34 × 10 + 2 = 20342`（`GameScene.cs:4709` 的 `+ 2`）。
pub const PROJECTILE_SOUND_HIT_INDEX: u32 = 20_000 + PROJECTILE_SOUND_BASE * 10 + 2;

/// 投射物音效的播放时机。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectileSoundPhase {
    /// 出生：`GameScene.cs:4697` 的 `+ 1`（`M34-1.wav`）。
    Spawn,
    /// 命中：`GameScene.cs:4709` 的 `+ 2`（`M34-2.wav`）。
    Hit,
}

impl ProjectileSoundPhase {
    /// 该时机的音效编号（20341 / 20342，见 [`PROJECTILE_SOUND_SPAWN_INDEX`]）。
    #[must_use]
    pub const fn index(self) -> u32 {
        match self {
            Self::Spawn => PROJECTILE_SOUND_SPAWN_INDEX,
            Self::Hit => PROJECTILE_SOUND_HIT_INDEX,
        }
    }

    /// 该时机的文件名：出生 `M34-1.wav`、命中 `M34-2.wav`（`GameScene.cs:4697/4709`）。
    #[must_use]
    pub const fn file(self) -> &'static str {
        match self {
            Self::Spawn => "M34-1.wav",
            Self::Hit => "M34-2.wav",
        }
    }
}

/// 投射物技能 → 出生/命中音文件名。
///
/// Crystal 的投射物表现目前只有 `Spell.FireBounce` 一档（`GameScene.cs:4693` 的
/// `switch` 无 `default`）：它的出生/命中音按 [`PROJECTILE_SOUND_BASE`] 的
/// `GreatFireBall = 34` 算，所以 `FireBounce(154)` 映射到 `M34-1.wav` / `M34-2.wav`。
/// 其它技能没有投射物音效，返回 `None`。
#[must_use]
pub const fn projectile_sound(spell: Spell, phase: ProjectileSoundPhase) -> Option<&'static str> {
    match spell {
        Spell::FireBounce => Some(phase.file()),
        _ => None,
    }
}

/// 背景音乐编号 → 文件名表。
///
/// Crystal 的 `SoundList.lst` 在 `;Music` 段（`resource/community/gqf2008-crystal/Sound/SoundList.lst:583`）
/// 把三组音乐编号直接映射成同名文件：
///
/// ```text
/// 30001: 30001.wav   … 30011: 30011.wav     // SoundList.lst:584-594
/// 30101: 30101.wav   … 30113: 30113.wav     // SoundList.lst:596-608
/// 31001: 31001.wav   … 31006: 31006.wav     // SoundList.lst:610-615
/// ```
///
/// Crystal 的 `SoundManager.PlayMusic(index, loop)` 先 `StopMusic()`，再用
/// `SoundList.Indexes[index]` 找文件名并创建循环音源（`Client/MirSounds/SoundManager.cs:128-136`）。
/// 这里保留同样的「编号 → 文件」关系，但文件放在 `resource/community/mirfiles-crystal-client/Client/Sound`
/// （完整 30 首 BGM；`resource/wav` 只有音效，没有 300xx.wav）。
pub const MUSIC_TRACKS: [(u16, &str); 30] = [
    (30001, "30001.wav"),
    (30002, "30002.wav"),
    (30003, "30003.wav"),
    (30004, "30004.wav"),
    (30005, "30005.wav"),
    (30006, "30006.wav"),
    (30007, "30007.wav"),
    (30008, "30008.wav"),
    (30009, "30009.wav"),
    (30010, "30010.wav"),
    (30011, "30011.wav"),
    (30101, "30101.wav"),
    (30102, "30102.wav"),
    (30103, "30103.wav"),
    (30104, "30104.wav"),
    (30105, "30105.wav"),
    (30106, "30106.wav"),
    (30107, "30107.wav"),
    (30108, "30108.wav"),
    (30109, "30109.wav"),
    (30110, "30110.wav"),
    (30111, "30111.wav"),
    (30112, "30112.wav"),
    (30113, "30113.wav"),
    (31001, "31001.wav"),
    (31002, "31002.wav"),
    (31003, "31003.wav"),
    (31004, "31004.wav"),
    (31005, "31005.wav"),
    (31006, "31006.wav"),
];

/// 没有 `MapInfo.Music` 时使用的主城主题曲（`SoundList.lst:584` 的第一首）。
///
/// 本仓库的示例服务端数据库（389～463 张地图）里 `MapInfo.Music` **全部是 0**，
/// 而 Crystal 的 `Music == 0` 表示静音（`GameScene.cs:10454-10466`）。为了让客户端在
/// 示例服上也有背景音乐，`music_for_map` 在没有服务器编号时回退到这一首；`--no-music`
/// 可以关掉。**这不是 Crystal 的行为**，属于本作对示例数据的兜底，见文档「简化」。
pub const DEFAULT_MUSIC: &str = "30001.wav";

/// 背景音乐编号 → 文件名（见 [`MUSIC_TRACKS`]）。
///
/// Crystal 的编号来自服务器 `MapInfo.Music`：`S.MapInformation.Music` 在
/// `GameScene.cs:2146` 写进 `MapControl.Music`，`S.MapChanged.Music` 在
/// `GameScene.cs:3985` 写进同一个字段；`MapControl.LoadMap` 发现编号变化就
/// `SoundManager.PlayMusic(Music, true)`（`GameScene.cs:10454-10466`）。
/// 编号不在表里（包括 0）时 Crystal 静音（`SoundManager.cs:132` 的 `TryGetValue` 失败），
/// 这里返回 `None`。
#[must_use]
pub const fn music_for_index(index: u16) -> Option<&'static str> {
    let mut position = 0;
    while position < MUSIC_TRACKS.len() {
        let (candidate, file) = MUSIC_TRACKS[position];
        if candidate == index {
            return Some(file);
        }
        position += 1;
    }
    None
}

/// 地图文件名/标题 → 背景音乐文件。
///
/// Crystal 客户端**没有**地图文件名 → 音乐的静态表：音乐编号完全由服务器
/// `MapInfo.Music` 决定（`GameScene.cs:2146`、`:3985`），文件由 `SoundList.lst`
/// 的 `;Music` 段决定（`SoundList.lst:583-615`）。本函数在「服务器没给编号」时
/// 按地图标题给出一个可用的主题曲，供示例服/离线模式使用：
///
/// * 标题含 `比奇` 或文件名 `0` → `30001.wav`（`SoundList.lst:584`）；
/// * 标题含 `盟重` 或文件名 `3` → `30002.wav`（`SoundList.lst:585`）；
/// * 其它非空地图 → [`DEFAULT_MUSIC`]。
///
/// 其它地图的文件名不在 Crystal 源码里，而是经典 Mir2 地图库的常见配置；
/// 本作把它当作**兜底**，真正的优先级是服务器给的 [`music_for_index`]。
#[must_use]
pub fn music_for_map(map_file: &str, title: &str) -> Option<&'static str> {
    let file_name = map_file.trim();
    let title = title.trim();
    if file_name.is_empty() && title.is_empty() {
        return None;
    }
    // 简化：示例数据库 389～463 张地图的 `MapInfo.Music` 全为 0（Crystal 会静音），
    // 为了让客户端有 BGM，按经典地图名给一个主题曲兜底。原因：客户端没有服务端
    // 地图数据库，无法在本地复现 `MapInfo.Music`；真正的优先级是服务器编号。
    if file_name == "0" || title.contains("比奇") {
        return music_for_index(30_001);
    }
    if file_name == "3" || title.contains("盟重") {
        return music_for_index(30_002);
    }
    Some(DEFAULT_MUSIC)
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
    fn projectile_sound_uses_great_fire_ball_base_m34() {
        // 基准是 GreatFireBall = 34（Shared/Enums.cs:1168），不是 FireBounce = 154
        // （:1264）；GameScene.cs:4697 / :4709 的 +1 / +2 拼出 M34-1 / M34-2。
        assert_eq!(PROJECTILE_SOUND_BASE, 34);
        assert_eq!(Spell::GreatFireBall as u32, PROJECTILE_SOUND_BASE);
        assert_eq!(Spell::FireBounce as u32, 154);
        assert_eq!(
            projectile_sound(Spell::FireBounce, ProjectileSoundPhase::Spawn),
            Some("M34-1.wav")
        );
        assert_eq!(
            projectile_sound(Spell::FireBounce, ProjectileSoundPhase::Hit),
            Some("M34-2.wav")
        );
        assert_eq!(ProjectileSoundPhase::Spawn.file(), "M34-1.wav");
        assert_eq!(ProjectileSoundPhase::Hit.file(), "M34-2.wav");
        // 编号必须是 20000 + 34 * 10 + 1 / + 2，且与 SoundManager 的兜底公式一致。
        assert_eq!(PROJECTILE_SOUND_SPAWN_INDEX, 20_341);
        assert_eq!(PROJECTILE_SOUND_HIT_INDEX, 20_342);
        assert_eq!(ProjectileSoundPhase::Spawn.index(), 20_341);
        assert_eq!(ProjectileSoundPhase::Hit.index(), 20_342);
        assert_eq!(
            index_to_fallback_name(PROJECTILE_SOUND_SPAWN_INDEX),
            "M34-1.wav"
        );
        assert_eq!(
            index_to_fallback_name(PROJECTILE_SOUND_HIT_INDEX),
            "M34-2.wav"
        );
        // 不得用 FireBounce = 154 直接拼音效编号（那会得到 M154-1/M154-2）。
        assert_eq!(index_to_fallback_name(20_000 + 154 * 10 + 1), "M154-1.wav");
        assert_ne!(
            projectile_sound(Spell::FireBounce, ProjectileSoundPhase::Spawn),
            Some("M154-1.wav")
        );
        // 其它技能没有投射物音效。
        assert_eq!(
            projectile_sound(Spell::FireBall, ProjectileSoundPhase::Hit),
            None
        );
        assert_eq!(
            projectile_sound(Spell::None, ProjectileSoundPhase::Spawn),
            None
        );
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

    #[test]
    fn music_indices_match_the_sound_list_music_sections() {
        // `SoundList.lst:583-615` 的三组音乐编号；每组首尾都要能查到文件。
        assert_eq!(music_for_index(30_001), Some("30001.wav"));
        assert_eq!(music_for_index(30_011), Some("30011.wav"));
        assert_eq!(music_for_index(30_101), Some("30101.wav"));
        assert_eq!(music_for_index(30_113), Some("30113.wav"));
        assert_eq!(music_for_index(31_001), Some("31001.wav"));
        assert_eq!(music_for_index(31_006), Some("31006.wav"));
        // 0（Crystal 的 Music=0 静音）与表外编号都没有文件。
        assert_eq!(music_for_index(0), None);
        assert_eq!(music_for_index(30_012), None);
        assert_eq!(music_for_index(32_000), None);
        assert_eq!(MUSIC_TRACKS.len(), 30);
    }

    #[test]
    fn map_music_falls_back_to_the_default_theme() {
        // 服务器给了编号时用 `music_for_index`（Crystal GameScene.cs:2146/3985）。
        assert_eq!(music_for_map("0", "比奇省"), Some("30001.wav"));
        assert_eq!(music_for_map("3", "盟重省"), Some("30002.wav"));
        // 其它地图也要有兜底主题曲（示例服 MapInfo.Music 全为 0）。
        assert_eq!(music_for_map("D2083", "洞穴"), Some(DEFAULT_MUSIC));
        assert_eq!(music_for_map("", "未知地图"), Some(DEFAULT_MUSIC));
        // 完全不知道地图时才静音。
        assert_eq!(music_for_map("", ""), None);
        assert_eq!(music_for_map("   ", "  "), None);
    }
}
