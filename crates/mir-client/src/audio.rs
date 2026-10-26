//! 音效播放后端：`rodio` + 100 ms 频控 + 无音频设备时的优雅降级。
//!
//! # 分工
//!
//! * 事件 → 文件名 的映射在 `mir-render::sound`（纯数据，不依赖音频库）；
//! * **播放**只在这里：`mir-render` 不引入任何音频依赖（需求硬性约束）；
//! * 背景音乐用独立的 [`SoundPlayer::play_music`]（循环、独立音量 `music_volume`、
//!   切图时换曲），与音效共用同一个 `Mixer`，但用单独的 `rodio::Player` 持有当前曲目。
//!
//! # 降级策略
//!
//! `rodio` 打开默认输出设备失败时（CI 没有音频设备、容器里没有声卡、Windows 上
//! WASAPI 初始化失败……）[`SoundPlayer::new`] 只**打印一次**警告，然后把
//! [`SoundPlayer::is_active`] 置为 `false`：之后所有 `play*` 调用都是空操作，
//! 窗口与渲染完全照常运行。
//!
//! # 频控（100 ms）
//!
//! 同一音效在 [`mir_render::SOUND_COOLDOWN_MS`]（100 ms）内只播一次，
//! 避免走路音效糊成一片。Crystal 没有等价的冷却表（它的节流靠步帧播放
//! `PlayerObject.cs:3596-3599` 与延迟队列 `SoundManager.cs:79-82`），
//! 这里按需求加一层 [`SoundThrottle`]。
//!
//! 频控也顺带解决了「自己走路包与本地预播重复」的问题：本客户端在发包时先播一次
//! 本地音效（服务器不会把 `ObjectWalk`/`ObjectAttack` 回给自己，见
//! `mir-server/src/world.rs:2428-2449` 的 `broadcast` 注释），如果哪天服务器改成
//! 回发，两次调用相隔几毫秒，第二次会被冷却挡掉。
//!
//! # 125 投射物音效（绕过频控）
//!
//! 投射物出生/命中音（`M34-1.wav`/`M34-2.wav`）走 [`SoundPlayer::play_projectile_cue`]：
//! **每次 cue 都提交**，不看也不写 [`SoundPlayer::play`] 的 100 ms 频控表。依据是
//! Crystal `Client/MirSounds/SoundManager.cs:94-105` 的 `PlaySound` 对每次播放都新建
//! `OneShotProvider` 塞进 mixer，同一文件在同一时刻播 2 次就真的叠 2 条 source，没有
//! 同文件 100 ms 限流；125 的同一帧也可能出现同一文件的多个投射物。普通音效
//! （`play`/`play_event`/`play_spell`）继续保留本作自己的 100 ms 频控，两套策略互不
//! 污染（125 不刷新普通冷却，普通冷却也不会吞掉 125）。
//!
//! # 素材目录
//!
//! [`SoundLibrary`] 按顺序在多个目录里找同名文件：
//!
//! 1. `resource/wav`——需求指定的音效目录（786 个经典 Mir2 WAV，没有背景音乐）；
//! 2. `resource/community/mirfiles-crystal-client/Client/Sound`——Crystal 客户端完整
//!    音效/音乐集，含 30 首 `300xx.wav` 背景音乐（`SoundList.lst:583-615`）；
//! 3. `resource/community/gqf2008-crystal/Sound`——另一份完整音效集，补上
//!    `resource/wav` 里缺的 `levelup.wav`、`pet_pickup.wav` 与 `M<技能>-*.wav`。
//!
//! 三个目录都没有该文件时静默跳过（只在第一次遇到时用 `tracing::debug!` 记一笔），
//! 不 panic、不刷屏。

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::{Path, PathBuf};

use mir_core::enums::Spell;
use mir_render::sound::{SOUND_COOLDOWN_MS, SoundId, sound_for_sex, spell_sound};
use rodio::mixer::Mixer;
use rodio::{Decoder, MixerDeviceSink, Player, Source};

/// 默认音量（需求：0.5）。
pub const DEFAULT_VOLUME: f32 = 0.5;
/// 背景音乐默认音量（独立于音效；需求：`--music-volume`，默认 0.5）。
pub const DEFAULT_MUSIC_VOLUME: f32 = 0.5;

/// 登录音（Crystal `SoundList.LoginEffect = 10100`，`SoundList.lst` 映射到 `100.wav`）。
pub const LOGIN_SOUND_FILE: &str = "100.wav";
/// 切图音（Crystal `SoundList.SelectMusic = 10147`，`SoundList.lst` 映射到 `Select2.wav`）。
pub const MAP_CHANGE_SOUND_FILE: &str = "Select2.wav";

/// 100 ms 频控表：`文件名 → 上次播放时间（Unix 毫秒）`。
#[derive(Debug, Default, Clone)]
pub struct SoundThrottle {
    /// 上次播放时间。
    last_played: HashMap<String, u64>,
}

impl SoundThrottle {
    /// 空表（什么都没播过）。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 判断现在能不能播 `file`；能播就顺手记下时间。
    ///
    /// `now_ms` 是 Unix 毫秒。时间倒退（`now < last`）时视为可以播放，
    /// 避免注入式测试里时钟乱跳把音效永久锁死。
    pub fn allow(&mut self, file: &str, now_ms: u64) -> bool {
        if let Some(last) = self.last_played.get(file) {
            if now_ms.saturating_sub(*last) < SOUND_COOLDOWN_MS && now_ms >= *last {
                return false;
            }
        }
        self.last_played.insert(file.to_owned(), now_ms);
        true
    }

    /// 已记录的文件数（测试/诊断用）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.last_played.len()
    }

    /// 是否一条记录都没有。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.last_played.is_empty()
    }

    /// 清空冷却（切图/重连时可以把上一张图的冷却状态丢掉）。
    pub fn clear(&mut self) {
        self.last_played.clear();
    }
}

/// 音效素材目录集合。
#[derive(Debug, Clone, Default)]
pub struct SoundLibrary {
    /// 按优先级排序的搜索目录。
    roots: Vec<PathBuf>,
    /// 是否允许回退 `.wav`（只有 `asset_mode = legacy` 为真；用户规则：运行只读自有格式）。
    allow_legacy_wav: bool,
}

impl SoundLibrary {
    /// 默认搜索目录：`resource/wav` + Crystal 完整音效/音乐集（见模块文档）。
    #[must_use]
    pub fn default_roots() -> Self {
        Self {
            allow_legacy_wav: false,
            // 运行期只读自有音频目录（`game-assets/audio`）；老 `.wav` 目录不再内置。
            roots: vec![
                PathBuf::from("game-assets/audio"),
                PathBuf::from("game-assets/audio/sfx"),
            ],
        }
    }

    /// 用给定目录建库（`--sound-dir`）。
    #[must_use]
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self {
            roots,
            allow_legacy_wav: false,
        }
    }

    /// 是否允许回退 `.wav`（仅 `asset_mode = legacy`；默认 `false`，只认 `.mna`）。
    #[must_use]
    pub fn with_legacy_wav(mut self, allow: bool) -> Self {
        self.allow_legacy_wav = allow;
        self
    }

    /// 按素材目录推音效目录：`<data_dir>/../wav` 与
    /// `<data_dir>/../community/gqf2008-crystal/Sound`。
    ///
    /// 这样从任意工作目录启动（只要 `--data-dir` 对）都能找到音效，
    /// 不依赖进程 CWD——测试从 `crates/mir-client` 跑也能用
    /// [`Self::from_repo_root`] 或这个函数拼出绝对路径。
    #[must_use]
    pub fn around_data_dir(data_dir: impl AsRef<Path>) -> Self {
        let data_dir = data_dir.as_ref();
        let root = data_dir.parent().unwrap_or_else(|| Path::new("."));
        Self::with_roots(vec![
            root.join("wav"),
            root.join("community")
                .join("mirfiles-crystal-client")
                .join("Client")
                .join("Sound"),
            root.join("community").join("gqf2008-crystal").join("Sound"),
        ])
    }

    /// 按仓库根目录拼默认素材目录（测试用）：即 `<root>/resource/data` 的兄弟目录。
    #[must_use]
    pub fn from_repo_root(root: impl AsRef<Path>) -> Self {
        Self::around_data_dir(root.as_ref().join("game-assets").join("data"))
    }

    /// 在默认目录前插入一个高优先级目录（已在列表里就提到最前面）。
    pub fn push_front(&mut self, directory: impl Into<PathBuf>) {
        let directory = directory.into();
        self.roots.retain(|root| root != &directory);
        self.roots.insert(0, directory);
    }

    /// 把另一套目录追加到末尾（低优先级；重复的直接跳过）。
    pub fn push_back(&mut self, other: &Self) {
        for root in &other.roots {
            if !self.roots.contains(root) {
                self.roots.push(root.clone());
            }
        }
    }

    /// 搜索目录（诊断/日志用）。
    #[must_use]
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// 按顺序找文件；都没有时返回 `None`。
    #[must_use]
    pub fn resolve(&self, file: &str) -> Option<PathBuf> {
        // 1) `.mna`（自有格式，永远优先）
        let mna = Path::new(file).with_extension("mna");
        if let Some(path) = self
            .roots
            .iter()
            .map(|root| root.join(&mna))
            .find(|path| path.is_file())
        {
            return Some(path);
        }
        // 2) 原 `.wav`/`.mp3`：只有 legacy 模式允许（用户规则：运行只读自有格式）
        if !self.allow_legacy_wav {
            return None;
        }
        self.roots
            .iter()
            .map(|root| root.join(file))
            .find(|path| path.is_file())
    }

    /// 这个文件能不能找到。
    #[must_use]
    pub fn has_any_audio_file(&self) -> bool {
        self.roots.iter().any(|root| {
            std::fs::read_dir(root).is_ok_and(|entries| {
                entries.flatten().any(|entry| {
                    entry.path().extension().is_some_and(|ext| {
                        ext.eq_ignore_ascii_case("mna")
                            || (self.allow_legacy_wav
                                && (ext.eq_ignore_ascii_case("wav")
                                    || ext.eq_ignore_ascii_case("mp3")))
                    })
                })
            })
        })
    }

    pub fn contains(&self, file: &str) -> bool {
        self.resolve(file).is_some()
    }
}

/// 一层 `Source` 包装：对混音器隐藏解码器的 `current_span_len`。
///
/// rodio 0.22.2 的 lewton `VorbisDecoder::current_span_len()` 在 OGG/.mna 流刚打开时可能
/// 返回 `Some(0)`（头部包没有音频样本）；`rodio::mixer` 的 `UniformSourceIterator` 会把
/// 它包成 `Take { n: Some(0) }`，于是这条 source **在混音器里一个样本都取不出来**，
/// 声音被静音（直接 `next()` 解码器则正常，所以离线/设备播放都会踩到）。`.mna` 是本作
/// 发布音效格式（vorbis），因此这里把 span 长度对外隐藏成 `None`（`Source` 的默认值），
/// 混音器改为逐样本做声道/采样率转换；音质、循环与叠加语义不变。
struct HiddenSpanSource<S> {
    inner: S,
}

impl<S: Source<Item = f32>> Iterator for HiddenSpanSource<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source<Item = f32>> Source for HiddenSpanSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(pos)
    }
}

/// 打开音频文件成解码器：`.mna` 取内嵌压缩流，其它交给 rodio 按扩展名猜。
fn decoder_for(path: &Path) -> Result<Box<dyn Source<Item = f32> + Send>, String> {
    let is_mna = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mna"));
    if is_mna {
        let clip = mir_asset::AudioClip::open(path).map_err(|error| error.to_string())?;
        let decoder =
            Decoder::new(std::io::Cursor::new(clip.data)).map_err(|error| error.to_string())?;
        return Ok(Box::new(HiddenSpanSource { inner: decoder }));
    }
    let handle = File::open(path).map_err(|error| error.to_string())?;
    let decoder = Decoder::try_from(handle).map_err(|error| error.to_string())?;
    Ok(Box::new(HiddenSpanSource { inner: decoder }))
}

/// 已打开的音频输出。
struct Backend {
    /// 输出流（只要这个句柄活着就一直在播；`Drop` 会停止播放，所以必须一直持有）。
    #[allow(dead_code)] // 只为保活：字段本身不需要读。
    sink: MixerDeviceSink,
    /// 混音器句柄（每个音效各自 `add`，天然支持重叠播放）。
    mixer: Mixer,
}

/// 音效播放器。
pub struct SoundPlayer {
    /// 素材目录。
    library: SoundLibrary,
    /// 100 ms 频控。
    throttle: SoundThrottle,
    /// 音量（0.0..=1.0）。
    volume: f32,
    /// 主音量（`client.toml` 的 `master_volume`，0.0..=1.0；与音效/音乐音量乘算）。
    master_volume: f32,
    /// 是否启用音效（`--no-sound` / 设置文件关掉时为 `false`）。
    enabled: bool,
    /// 是否启用背景音乐（`--no-music` / 设置文件关掉时为 `false`）。
    music_enabled: bool,
    /// 背景音乐音量（0.0..=1.0，独立于音效）。
    music_volume: f32,
    /// 当前背景音乐的循环播放器；`None` 表示没有在放。
    music: Option<Player>,
    /// 当前背景音乐文件（避免同一张图反复重开）。
    music_path: Option<PathBuf>,
    /// 音频后端；`None` 表示没有声音（关掉了或设备打不开）。
    backend: Option<Backend>,
    /// 调用方提供的离线混音器（[`Self::with_offline_mixer`]）。
    ///
    /// 无音频设备的离线/嵌入场景与 M4 证据链用它替代设备输出：仍是**真实**的
    /// `rodio` 混音器（真实解码、真实 `amplify`、真实采样），生产 `main.rs` 的
    /// [`Self::new`]/[`Self::new_with_music`] 不会设置它。
    offline_mixer: Option<Mixer>,
    /// 已经警告过的文件（避免同一个坏文件刷屏）。
    warned: HashSet<String>,
    /// 测试注入的假混音器：记录「真正送进混音器」的音效次数。
    ///
    /// CI 没有音频设备，`backend` 恒为 `None`；单元测试通过它计数
    /// （[`SoundPlayer::set_test_sink`]），生产构建里这个字段不存在。
    #[cfg(test)]
    test_sink: Option<std::rc::Rc<std::cell::Cell<usize>>>,
}

impl std::fmt::Debug for SoundPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("SoundPlayer");
        debug
            .field("roots", &self.library.roots)
            .field("volume", &self.volume)
            .field("master_volume", &self.master_volume)
            .field("enabled", &self.enabled)
            .field("music_volume", &self.music_volume)
            .field("music_enabled", &self.music_enabled)
            .field("music", &self.music_path)
            .field("backend", &self.backend.is_some())
            .field("offline_mixer", &self.offline_mixer.is_some())
            .field("throttle", &self.throttle.len())
            .field("warned", &self.warned.len());
        #[cfg(test)]
        debug.field("test_sink", &self.test_sink.is_some());
        debug.finish()
    }
}

impl SoundPlayer {
    /// 建播放器：音效 `enabled == false` 或设备打不开时都是「静音空操作」。
    ///
    /// 设备打不开只会打印**一次**警告（需求：CI 没有音频设备时窗口照常运行）。
    /// 这个构造器不启用背景音乐（音乐开关见 [`Self::new_with_music`]），
    /// 保持既有测试/调用方的行为。
    #[must_use]
    pub fn new(library: SoundLibrary, enabled: bool, volume: f32) -> Self {
        Self::new_with_music(library, enabled, volume, false, DEFAULT_MUSIC_VOLUME)
    }

    /// 建播放器，并一起设置背景音乐开关/音量。
    ///
    /// 只要音效或音乐有一个启用就打开输出设备；`--no-sound` 只关音效，
    /// 不会顺带把背景音乐也关掉。
    #[must_use]
    pub fn new_with_music(
        library: SoundLibrary,
        enabled: bool,
        volume: f32,
        music_enabled: bool,
        music_volume: f32,
    ) -> Self {
        let mut player =
            Self::without_device(library, enabled, volume).with_music(music_enabled, music_volume);
        if enabled && !player.library.has_any_audio_file() {
            eprintln!("音频不可用：未找到音效素材（检查 resource/wav 或 --sound-dir）");
        }
        if enabled || music_enabled {
            match rodio::DeviceSinkBuilder::open_default_sink() {
                Ok(mut sink) => {
                    // 默认 Drop 时会往 stderr 打一行提示，客户端退出时不需要。
                    sink.log_on_drop(false);
                    let mixer = sink.mixer().clone();
                    player.backend = Some(Backend { sink, mixer });
                }
                Err(error) => {
                    eprintln!(
                        "音频不可用：无法打开音频设备（{error}）；音效/音乐已禁用，窗口与渲染照常运行。"
                    );
                }
            }
        }
        player
    }

    /// 不带设备的播放器（永远不会出声；测试与 `--no-sound` 用）。
    #[must_use]
    pub fn without_device(library: SoundLibrary, enabled: bool, volume: f32) -> Self {
        Self {
            library,
            throttle: SoundThrottle::new(),
            volume: volume.clamp(0.0, 1.0),
            master_volume: 1.0,
            enabled,
            music_enabled: true,
            music_volume: DEFAULT_MUSIC_VOLUME,
            music: None,
            music_path: None,
            backend: None,
            offline_mixer: None,
            warned: HashSet::new(),
            #[cfg(test)]
            test_sink: None,
        }
    }

    /// 建一个把音效提交到**调用方提供的** `rodio::Mixer` 的播放器（不打开设备）。
    ///
    /// 用于无音频设备的离线混音/嵌入场景与 R61 M4 证据链：输出端是真实的
    /// `rodio` 混音器（真实解码、真实 `amplify`、真实采样），不是测试计数器。
    /// 生产 `main.rs` 只用 [`Self::new`]/[`Self::new_with_music`]，不会走这个入口；
    /// [`Self::is_active`] 仍为 `false`（这不是真实设备），但 [`Self::play`] 会真的
    /// 把音效送进这个混音器。
    #[must_use]
    pub fn with_offline_mixer(
        library: SoundLibrary,
        enabled: bool,
        volume: f32,
        mixer: Mixer,
    ) -> Self {
        let mut player = Self::without_device(library, enabled, volume);
        player.offline_mixer = Some(mixer);
        player
    }

    /// 测试注入假混音器：`play` 成功解码后会把它计数加一（不依赖真实音频设备）。
    #[cfg(test)]
    pub fn set_test_sink(&mut self, counter: std::rc::Rc<std::cell::Cell<usize>>) {
        self.test_sink = Some(counter);
    }

    /// 是否有可用的输出端（真实设备、离线混音器或测试假混音器）。
    fn has_output(&self) -> bool {
        if self.backend.is_some() || self.offline_mixer.is_some() {
            return true;
        }
        #[cfg(test)]
        {
            self.test_sink.is_some()
        }
        #[cfg(not(test))]
        {
            false
        }
    }

    /// 设置背景音乐开关与音量（在 [`Self::new`] 之前调用）。
    #[must_use]
    pub fn with_music(mut self, enabled: bool, volume: f32) -> Self {
        self.music_enabled = enabled;
        self.music_volume = volume.clamp(0.0, 1.0);
        self
    }

    /// 是否真的有声音（启用了且设备打开了）。
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.enabled && self.backend.is_some()
    }

    /// 是否启用音效（不看设备）。
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// 开关音效（运行时可切；重新打开**不会**重新申请设备）。
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// 音量（0.0..=1.0）。
    #[must_use]
    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// 设置音量（夹到 0.0..=1.0；已经在播的音效不受影响——rodio 的 `Mixer`
    /// 是「每个音效一条 source」，改音量只对之后播放的生效）。
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    /// 音效的实际增益：`sound_volume × master_volume`（0 表示静音）。
    #[must_use]
    pub fn effective_sound_volume(&self) -> f32 {
        if self.enabled {
            self.volume * self.master_volume
        } else {
            0.0
        }
    }

    /// 背景音乐的实际增益：`music_volume × master_volume`（0 表示静音）。
    #[must_use]
    pub fn effective_music_volume(&self) -> f32 {
        if self.music_enabled {
            self.music_volume * self.master_volume
        } else {
            0.0
        }
    }

    /// 是否启用背景音乐（不看设备）。
    #[must_use]
    pub fn is_music_enabled(&self) -> bool {
        self.music_enabled
    }

    /// 背景音乐是否真的有声音（启用了且设备打开了）。
    #[must_use]
    pub fn is_music_active(&self) -> bool {
        self.music_enabled && self.backend.is_some()
    }

    /// 设置主音量（`client.toml` 的 `master_volume`，夹到 0.0..=1.0）。
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.clamp(0.0, 1.0);
        if let Some(player) = &self.music {
            player.set_volume(self.music_volume * self.master_volume);
        }
    }

    /// 主音量（0.0..=1.0）。
    #[must_use]
    pub fn master_volume(&self) -> f32 {
        self.master_volume
    }

    /// 背景音乐音量（0.0..=1.0）。
    #[must_use]
    pub fn music_volume(&self) -> f32 {
        self.music_volume
    }

    /// 设置背景音乐音量；正在播的音乐立即生效（`Player::set_volume`）。
    pub fn set_music_volume(&mut self, volume: f32) {
        self.music_volume = volume.clamp(0.0, 1.0);
        if let Some(player) = &self.music {
            player.set_volume(self.music_volume * self.master_volume);
        }
    }

    /// 开关背景音乐；关掉时立即停止当前曲目。
    pub fn set_music_enabled(&mut self, enabled: bool) {
        self.music_enabled = enabled;
        if !enabled {
            self.stop_music();
        }
    }

    /// 当前背景音乐文件（测试/诊断用）。
    #[must_use]
    pub fn music_path(&self) -> Option<&Path> {
        self.music_path.as_deref()
    }

    /// 循环播放一个背景音乐文件；返回是否真的送进了混音器。
    ///
    /// * 同一个路径已经在播时直接返回 `true`（不重新起播）；
    /// * 新路径先 `StopMusic` 再循环播放（Crystal `SoundManager.PlayMusic` 先
    ///   `StopMusic()`，`SoundManager.cs:128-136`）；
    /// * 音乐关闭、没有设备、文件缺失/解码失败时安静返回 `false`。
    ///
    /// 循环用 `Source::repeat_infinite()`；音量用独立的 [`Self::music_volume`]。
    pub fn play_music(&mut self, path: impl AsRef<Path>) -> bool {
        if !self.music_enabled || self.backend.is_none() {
            return false;
        }
        let requested = path.as_ref();
        // 允许传 `30001.wav` 这样的裸文件名：先按素材目录解析（与音效一致），
        // 找不到时再按调用方给的路径打开。
        let resolved = if requested.is_file() {
            requested.to_path_buf()
        } else {
            self.library
                .resolve(&requested.to_string_lossy())
                .unwrap_or_else(|| requested.to_path_buf())
        };
        if self.music_path.as_deref() == Some(resolved.as_path()) && self.music.is_some() {
            return true;
        }
        self.stop_music();
        let path = resolved.as_path();
        let source = match decoder_for(path) {
            Ok(decoder) => decoder
                .repeat_infinite()
                .amplify(self.music_volume * self.master_volume),
            Err(error) => {
                eprintln!("警告：背景音乐 {} 解码失败：{error}", path.display());
                return false;
            }
        };
        let Some(backend) = self.backend.as_ref() else {
            return false;
        };
        let player = Player::connect_new(&backend.mixer);
        player.append(source);
        player.play();
        self.music = Some(player);
        self.music_path = Some(path.to_path_buf());
        true
    }

    /// 按文件名在素材目录里找并播放背景音乐（见 [`SoundLibrary::resolve`]）。
    pub fn play_music_file(&mut self, file: &str) -> bool {
        let Some(path) = self.library.resolve(file) else {
            return false;
        };
        self.play_music(path)
    }

    /// 停止背景音乐（保留音量/开关设置）。
    pub fn stop_music(&mut self) {
        if let Some(player) = self.music.take() {
            player.stop();
        }
        self.music_path = None;
    }

    /// 素材目录。
    #[must_use]
    pub fn library(&self) -> &SoundLibrary {
        &self.library
    }

    /// 频控表（测试用）。
    #[must_use]
    pub fn throttle(&self) -> &SoundThrottle {
        &self.throttle
    }

    /// 播放一个文件；返回是否真的送进了混音器。
    ///
    /// 跳过的情况：音效关闭、没有设备、100 ms 内播过同一个文件、素材缺失、解码失败。
    pub fn play(&mut self, file: &str, now_ms: u64) -> bool {
        let gain = self.volume * self.master_volume;
        if !self.enabled || gain <= 0.0 || !self.has_output() {
            return false;
        }
        if !self.throttle.allow(file, now_ms) {
            return false;
        }
        self.submit_file(file, gain)
    }

    /// 提交一条 125 投射物音效 cue：**不经过 100 ms 频控**，每次调用都尝试提交。
    ///
    /// Crystal `SoundManager.PlaySound`（`Client/MirSounds/SoundManager.cs:94-105`）对每次
    /// 播放都新建 `OneShotProvider` 送进 mixer，没有同文件 100 ms 限流；125 的出生/命中
    /// 音必须按事件逐条提交（同一时刻同文件 2 次、33 次都要各自入混音器）。这里既不查
    /// 也不写 [`SoundThrottle`]，因此 125 流量不会污染普通音效的频控，普通频控也不会
    /// 吞掉 125。返回是否真的送进了混音器（关闭/静音/无输出/缺素材/解码失败为 `false`，
    /// 属正常跳过，不是错误）。
    pub fn play_projectile_cue(&mut self, file: &'static str) -> bool {
        let gain = self.volume * self.master_volume;
        if !self.enabled || gain <= 0.0 || !self.has_output() {
            return false;
        }
        self.submit_file(file, gain)
    }

    /// 批量提交 125 投射物音效 cue；返回真正送进混音器的条数。
    ///
    /// 生产路径把 [`crate::ui::UiState::take_projectile_sound_cues`] 的结果逐个交给
    /// [`Self::play_projectile_cue`]，因此同一帧同文件的多个 cue 全部提交。
    pub fn play_projectile_cues(&mut self, cues: impl IntoIterator<Item = &'static str>) -> usize {
        let mut submitted = 0;
        for file in cues {
            if self.play_projectile_cue(file) {
                submitted += 1;
            }
        }
        submitted
    }

    /// 共享提交链：查素材 → 解码 → `amplify(gain)` → 送进输出端（设备/离线混音器/测试注入）。
    ///
    /// [`Self::play`] 与 [`Self::play_projectile_cue`] 都走这里，保证 125 专用策略与普通
    /// 音效共享同一条 decoder/amplify 路径（R61 M4）。
    fn submit_file(&mut self, file: &str, gain: f32) -> bool {
        let Some(path) = self.library.resolve(file) else {
            if self.warned.insert(file.to_owned()) {
                tracing::debug!(file, "音效素材缺失，跳过");
            }
            return false;
        };
        let source = match decoder_for(&path) {
            Ok(decoder) => decoder.amplify(gain),
            Err(error) => {
                if self.warned.insert(file.to_owned()) {
                    eprintln!("警告：音效 {} 解码失败：{error}", path.display());
                }
                return false;
            }
        };
        if let Some(backend) = self.backend.as_ref() {
            backend.mixer.add(source);
            return true;
        }
        if let Some(mixer) = self.offline_mixer.as_ref() {
            mixer.add(source);
            return true;
        }
        #[cfg(test)]
        if let Some(counter) = &self.test_sink {
            counter.set(counter.get().saturating_add(1));
            return true;
        }
        false
    }

    /// 播放一个事件（男女两档音效按 `female` 选，见 `mir_render::sound::sound_for_sex`）。
    pub fn play_event(&mut self, event: SoundId, female: bool, now_ms: u64) -> bool {
        match sound_for_sex(event, female) {
            Some(file) => self.play(file, now_ms),
            None => false,
        }
    }

    /// 播放某个技能的施法音（`M<技能号>-0.wav`，`SoundManager.cs:87-89`）。
    pub fn play_spell(&mut self, spell: Spell, now_ms: u64) -> bool {
        match spell_sound(spell) {
            Some(file) => self.play(&file, now_ms),
            None => false,
        }
    }
}

/// 判断 `path` 是否像一个 WAV（`RIFF....WAVE` 头）。
///
/// 不需要打开设备就能验证素材格式，测试与「素材自检」用。
#[must_use]
pub fn is_wav_file(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE"
}

#[cfg(test)]
mod tests {
    /// `.wav` 找不到时回退同名 `.mna`（发布期音频格式）。
    #[test]
    fn library_resolves_mna_when_wav_is_absent() {
        let dir = std::env::temp_dir().join(format!(
            "mir2-audio-mna-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("100.mna"), b"fake").expect("write");
        let library = SoundLibrary::with_roots(vec![dir.clone()]);
        let resolved = library
            .resolve("100.wav")
            .expect("should fall back to .mna");
        assert_eq!(resolved, dir.join("100.mna"));
        assert!(library.has_any_audio_file(), "有 .mna 也算有素材");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 只有 `.wav` 时：默认（非 legacy）不认，`with_legacy_wav(true)` 才回退。
    #[test]
    fn wav_is_only_used_in_legacy_mode() {
        let dir = std::env::temp_dir().join(format!(
            "mir2-audio-wav-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("100.wav"), b"fake").expect("write");
        let strict = SoundLibrary::with_roots(vec![dir.clone()]);
        assert!(
            strict.resolve("100.wav").is_none(),
            "默认（auto/dev/release）不许读 .wav"
        );
        let legacy = SoundLibrary::with_roots(vec![dir.clone()]).with_legacy_wav(true);
        assert_eq!(
            legacy.resolve("100.wav"),
            Some(dir.join("100.wav")),
            "legacy 模式才允许 .wav"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 真实 `.mna` 能被解码器打开（仓库里有 audio 素材时才跑）。
    #[test]
    fn real_mna_decodes() {
        let path = std::path::Path::new("../../game-assets/audio/sfx/levelup.mna");
        if !path.is_file() {
            eprintln!("skip: no game-assets/audio/sfx/levelup.mna");
            return;
        }
        let decoder = super::decoder_for(path).expect("decode .mna");
        let frames = decoder.take(2048).count();
        assert!(frames > 0, "解码器必须能吐出样本");
    }

    use super::*;

    #[test]
    fn throttle_blocks_within_one_hundred_millis() {
        let mut throttle = SoundThrottle::new();
        assert!(throttle.allow("1.wav", 1_000));
        // 99 ms 后仍然挡着。
        assert!(!throttle.allow("1.wav", 1_099));
        // 正好 100 ms 放行。
        assert!(throttle.allow("1.wav", 1_100));
        assert_eq!(throttle.len(), 1);
    }

    #[test]
    fn throttle_is_per_file() {
        let mut throttle = SoundThrottle::new();
        assert!(throttle.allow("1.wav", 1_000));
        assert!(throttle.allow("3.wav", 1_001), "不同音效互不影响");
        assert!(!throttle.allow("1.wav", 1_050));
        assert_eq!(throttle.len(), 2);
        throttle.clear();
        assert!(throttle.is_empty());
        assert!(throttle.allow("1.wav", 1_050));
    }

    #[test]
    fn throttle_tolerates_clock_going_backwards() {
        let mut throttle = SoundThrottle::new();
        assert!(throttle.allow("1.wav", 5_000));
        // 时钟倒退：不锁死，直接放行。
        assert!(throttle.allow("1.wav", 1_000));
        assert!(!throttle.allow("1.wav", 1_050));
    }

    #[test]
    fn disabled_player_is_a_no_op() {
        let mut player = SoundPlayer::without_device(SoundLibrary::default_roots(), false, 0.5);
        assert!(!player.is_active());
        assert!(!player.is_enabled());
        assert!(!player.play("1.wav", 0));
        assert!(!player.play_event(SoundId::Walk, false, 0));
        assert!(!player.play_spell(Spell::FireBall, 0));
        // 关掉时连频控都不记录。
        assert!(player.throttle().is_empty());
    }

    #[test]
    fn volume_is_clamped() {
        let mut player = SoundPlayer::without_device(SoundLibrary::default_roots(), false, 2.0);
        assert!((player.volume() - 1.0).abs() < f32::EPSILON);
        player.set_volume(-1.0);
        assert!(player.volume().abs() < f32::EPSILON);
        player.set_volume(DEFAULT_VOLUME);
        assert!((player.volume() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn library_prefers_the_first_existing_root() {
        let directory = std::env::temp_dir().join(format!("mir-sound-lib-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("建临时目录");
        // 自有格式是 `.mna`；`.wav` 只有 legacy 模式才认。
        std::fs::write(directory.join("1.mna"), b"MNA1fake").expect("写假 mna");

        let mut library = SoundLibrary::with_roots(vec![directory.clone()]);
        assert_eq!(library.resolve("1.wav"), Some(directory.join("1.mna")));
        assert!(library.contains("1.wav"));
        assert!(library.resolve("nope.wav").is_none());

        // 前面的目录找不到时继续往后找。
        library.push_front(directory.join("missing"));
        assert_eq!(
            library.resolve("1.wav"),
            Some(directory.join("1.mna")),
            "排在后面的目录也要能被找到"
        );
        assert_eq!(library.roots().len(), 2);

        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn wav_header_check_rejects_garbage() {
        let directory =
            std::env::temp_dir().join(format!("mir-sound-header-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("建临时目录");
        let good = directory.join("good.wav");
        std::fs::write(&good, b"RIFF\x00\x00\x00\x00WAVEfmt ").expect("写 wav 头");
        let bad = directory.join("bad.wav");
        std::fs::write(&bad, b"not a wav at all").expect("写垃圾");
        assert!(is_wav_file(&good));
        assert!(!is_wav_file(&bad));
        assert!(!is_wav_file(&directory.join("missing.wav")));
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn music_defaults_and_volume_are_independent_of_sound_effects() {
        let mut player = SoundPlayer::without_device(SoundLibrary::default_roots(), true, 0.2)
            .with_music(true, 0.8);
        assert!(player.is_music_enabled());
        assert!(!player.is_music_active(), "没有设备时不应报告在播");
        assert!((player.music_volume() - 0.8).abs() < f32::EPSILON);
        assert!((player.volume() - 0.2).abs() < f32::EPSILON, "音效音量独立");
        player.set_music_volume(2.0);
        assert!((player.music_volume() - 1.0).abs() < f32::EPSILON);
        player.set_music_enabled(false);
        assert!(!player.is_music_enabled());
        assert!(player.music_path().is_none());
        // 没有设备时 `play_music` 是安静空操作。
        assert!(!player.play_music("does-not-exist.wav"));
        assert!(!player.play_music_file("30001.wav"));
    }

    #[test]
    fn music_files_resolve_in_the_crystal_sound_set() {
        // `resource/` 被 .gitignore 忽略：CI 上没有素材时直接跳过。
        let library = SoundLibrary::default_roots();
        let Some(file) = mir_render::music_for_index(30_001) else {
            eprintln!("跳过：music_for_index(30001) 应存在");
            return;
        };
        let Some(path) = library.resolve(file) else {
            eprintln!("跳过：音效目录里没有 {file}");
            return;
        };
        assert!(is_wav_file(&path), "{} 应当是 WAV", path.display());
    }

    #[test]
    fn real_sound_files_resolve_when_the_corpus_is_present() {
        // `resource/` 被 .gitignore 忽略：CI 上没有素材时直接跳过。
        let library = SoundLibrary::default_roots();
        let Some(path) = library.resolve("1.wav") else {
            eprintln!("跳过：resource/wav 里没有 1.wav");
            return;
        };
        assert!(is_wav_file(&path), "{} 应当是 WAV", path.display());
    }

    /// 写一个 8 位单声道静音 WAV（44 字节头 + 8 个采样）。
    fn write_silent_wav(path: &Path) {
        let data_len = 8u32;
        let mut bytes = Vec::with_capacity(44 + data_len as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&8_000u32.to_le_bytes());
        bytes.extend_from_slice(&8_000u32.to_le_bytes()); // byte rate
        bytes.extend_from_slice(&1u16.to_le_bytes()); // block align
        bytes.extend_from_slice(&8u16.to_le_bytes()); // bits
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        bytes.resize(44 + data_len as usize, 128u8);
        std::fs::write(path, bytes).expect("写测试 WAV");
    }

    #[test]
    fn play_events_are_counted_throttled_and_silent_without_assets() {
        let dir = std::env::temp_dir().join(format!("mir-audio-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let wav = dir.join("fx.wav");
        write_silent_wav(&wav);
        // 这个测试验证节流/计数，不验证格式规则：显式开 legacy 允许 `.wav`。
        let library = SoundLibrary::with_roots(vec![dir.clone()]).with_legacy_wav(true);
        let counter = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let mut player = SoundPlayer::without_device(library, true, 0.5);
        player.set_test_sink(counter.clone());

        // 第一次送进混音器；100 ms 内同文件被节流；到点后放行。
        assert!(player.play("fx.wav", 1_000));
        assert_eq!(counter.get(), 1);
        assert!(!player.play("fx.wav", 1_050), "同一音效 100 ms 内节流");
        assert_eq!(counter.get(), 1);
        assert!(player.play("fx.wav", 1_100));
        assert_eq!(counter.get(), 2);

        // 缺文件：静默跳过，不计数、不 panic。
        assert!(!player.play("missing.wav", 1_200));
        assert_eq!(counter.get(), 2);

        // 关掉音效：连频控都不记录。
        player.set_enabled(false);
        assert!(!player.play("fx.wav", 1_300));
        assert_eq!(counter.get(), 2);
        player.set_volume(0.0);
        player.set_enabled(true);
        // 音量 0 = 静音：不送进混音器（需求：sound_volume=0 静音）。
        assert!(!player.play("fx.wav", 1_400));
        assert_eq!(counter.get(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn audio_mapping_uses_crystal_soundlist_ids() {
        // Crystal `SoundList.cs` 常量 → `SoundList.lst` 文件（不是 Mir1 sound.lst）。
        assert_eq!(SoundId::Hit.index(), 10_070);
        assert_eq!(sound_for_sex(SoundId::Hit, false), Some("70.wav")); // 10070 → 70.wav
        assert_eq!(SoundId::Pickup.index(), 10_520);
        assert_eq!(
            sound_for_sex(SoundId::Pickup, false),
            Some("pet_pickup.wav")
        ); // 10520
        assert_eq!(SoundId::LevelUp.index(), 10_156);
        assert_eq!(sound_for_sex(SoundId::LevelUp, false), Some("levelup.wav")); // 10156
        assert_eq!(SoundId::Struck.index(), 10_138);
        assert_eq!(sound_for_sex(SoundId::Struck, false), Some("138.wav")); // 10138
        assert_eq!(sound_for_sex(SoundId::Struck, true), Some("139.wav")); // 10139
        assert_eq!(SoundId::Die.index(), 10_144);
        assert_eq!(sound_for_sex(SoundId::Die, false), Some("144.wav")); // 10144
        assert_eq!(sound_for_sex(SoundId::Die, true), Some("145.wav")); // 10145
        // 登录/切图不在 SoundId（mir-render 不在本任务范围），常量在 audio.rs。
        assert_eq!(LOGIN_SOUND_FILE, "100.wav"); // 10100 LoginEffect
        assert_eq!(MAP_CHANGE_SOUND_FILE, "Select2.wav"); // 10147 SelectMusic
    }

    #[test]
    fn seven_events_reach_the_injectable_sink() {
        let dir = std::env::temp_dir().join(format!("mir-audio-seven-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        for file in [
            LOGIN_SOUND_FILE,
            MAP_CHANGE_SOUND_FILE,
            "70.wav",
            "pet_pickup.wav",
            "levelup.wav",
            "138.wav",
            "144.wav",
        ] {
            write_silent_wav(&dir.join(file));
        }
        // 这个测试验证节流/计数，不验证格式规则：显式开 legacy 允许 `.wav`。
        let library = SoundLibrary::with_roots(vec![dir.clone()]).with_legacy_wav(true);
        let counter = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let mut player = SoundPlayer::without_device(library, true, 1.0);
        player.set_master_volume(1.0);
        player.set_test_sink(counter.clone());
        assert!(player.play(LOGIN_SOUND_FILE, 1_000)); // 登录
        assert!(player.play(MAP_CHANGE_SOUND_FILE, 1_001)); // 切图
        assert!(player.play_event(SoundId::Hit, false, 1_002)); // 攻击命中
        assert!(player.play_event(SoundId::Pickup, false, 1_003)); // 拾取
        assert!(player.play_event(SoundId::LevelUp, false, 1_004)); // 升级
        assert!(player.play_event(SoundId::Struck, false, 1_005)); // 受伤
        assert!(player.play_event(SoundId::Die, false, 1_006)); // 死亡
        assert_eq!(counter.get(), 7, "七个事件各触发一次播放调用");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn master_volume_multiplies_and_zero_is_silent() {
        let mut player = SoundPlayer::without_device(SoundLibrary::default_roots(), true, 0.5);
        player.set_master_volume(0.5);
        assert!((player.effective_sound_volume() - 0.25).abs() < f32::EPSILON);
        player.set_master_volume(0.0);
        assert!(player.effective_sound_volume().abs() < f32::EPSILON);
        assert!(!player.play(LOGIN_SOUND_FILE, 0), "master=0 必须静音");
        player.set_master_volume(1.0);
        assert!((player.effective_sound_volume() - 0.5).abs() < f32::EPSILON);
        player.set_volume(1.0);
        assert!((player.effective_sound_volume() - 1.0).abs() < f32::EPSILON);
        player.set_volume(0.0);
        assert!(player.effective_sound_volume().abs() < f32::EPSILON);
        assert!(!player.play(LOGIN_SOUND_FILE, 0), "sound_volume=0 必须静音");
        player.set_music_volume(0.8);
        assert!((player.effective_music_volume() - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn missing_device_and_missing_assets_are_silent_without_panic() {
        let empty = std::env::temp_dir().join(format!("mir-audio-empty-{}", std::process::id()));
        std::fs::create_dir_all(&empty).expect("建临时目录");
        let mut player =
            SoundPlayer::without_device(SoundLibrary::with_roots(vec![empty.clone()]), true, 1.0);
        assert!(!player.library.has_any_audio_file());
        assert!(!player.play(LOGIN_SOUND_FILE, 0));
        assert!(!player.play_event(SoundId::Hit, false, 0));
        assert!(!player.is_active(), "没有设备/后端时 is_active=false");
        let _ = std::fs::remove_dir_all(&empty);
    }

    /// R61 D2 M3: 125 cues bypass the 100 ms cooldown and never touch the shared throttle.
    #[test]
    fn projectile_cue_entry_skips_the_cooldown_and_leaves_normal_throttle_untouched() {
        let dir = std::env::temp_dir().join(format!(
            "mir2-audio-125-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        write_silent_wav(&dir.join("fx.wav"));
        write_silent_wav(&dir.join("fx2.wav"));
        let library = SoundLibrary::with_roots(vec![dir.clone()]).with_legacy_wav(true);
        let counter = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let mut player = SoundPlayer::without_device(library, true, 1.0);
        player.set_test_sink(counter.clone());

        // The 125 entry submits every copy at the same instant and writes no cooldown.
        assert!(player.play_projectile_cue("fx.wav"));
        assert!(player.play_projectile_cue("fx.wav"));
        assert_eq!(counter.get(), 2);
        assert!(player.throttle().is_empty());
        assert_eq!(player.play_projectile_cues(vec!["fx.wav"; 33]), 33);
        assert_eq!(counter.get(), 35);
        assert!(player.throttle().is_empty());

        // The normal entry keeps its own cooldown; 125 traffic neither satisfies nor pollutes it.
        assert!(player.play("fx2.wav", 1_000));
        assert!(!player.play("fx2.wav", 1_000), "same instant is throttled");
        assert_eq!(counter.get(), 36);
        assert_eq!(player.throttle().len(), 1);
        assert!(player.play_projectile_cue("fx2.wav"));
        assert_eq!(counter.get(), 37);
        assert!(
            !player.play("fx2.wav", 1_050),
            "125 cue must not refresh the normal cooldown"
        );
        assert_eq!(counter.get(), 37);
        assert!(player.play("fx2.wav", 1_100), "exactly 100 ms is allowed");
        assert_eq!(counter.get(), 38);

        // Unsubmitted paths return false, do not count and do not record a cooldown.
        player.set_enabled(false);
        assert!(!player.play_projectile_cue("fx.wav"));
        assert_eq!(counter.get(), 38);
        player.set_enabled(true);
        player.set_volume(0.0);
        assert!(!player.play_projectile_cue("fx.wav"));
        assert_eq!(counter.get(), 38);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// R61 D2 M4: [`SoundPlayer::with_offline_mixer`] drives a real `rodio` mixer
    /// (real samples, no device and no test counter).
    #[test]
    fn offline_mixer_submits_real_cues_without_an_audio_device() {
        use rodio::Source as _;

        let sfx =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game-assets/audio/sfx");
        let library = SoundLibrary::with_roots(vec![sfx.clone()]);
        if library.resolve("M34-1.wav").is_none() {
            eprintln!(
                "SKIP: missing {} (asset evidence is PARTIAL)",
                sfx.join("M34-1.mna").display()
            );
            return;
        }
        let channels = rodio::ChannelCount::new(2).expect("mixer channels");
        let sample_rate = rodio::SampleRate::new(48_000).expect("mixer rate");
        let (mixer, mut source) = rodio::mixer::mixer(channels, sample_rate);
        let mut player = SoundPlayer::with_offline_mixer(library, true, 1.0, mixer);

        assert!(
            player.has_output(),
            "an injected offline mixer is a real output"
        );
        assert!(!player.is_active(), "an offline mixer is not a device");
        assert!(player.play_projectile_cue("M34-1.wav"));
        assert_eq!(source.channels().get(), 2);
        assert_eq!(source.sample_rate().get(), 48_000);
        let samples: Vec<f32> = source.by_ref().take(4_096).collect();
        assert!(
            !samples.is_empty(),
            "the offline mixer must yield real samples"
        );
        assert!(samples.iter().all(|sample| sample.is_finite()));
        assert!(samples.iter().any(|sample| sample.abs() > 0.0));
    }

    /// R61 D2 M3: failed 125 submissions are quiet `false` results, not panics.
    #[test]
    fn projectile_cue_missing_and_undecodable_assets_are_silent() {
        let counter = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let mut player =
            SoundPlayer::without_device(SoundLibrary::with_roots(Vec::new()), true, 1.0);
        player.set_test_sink(counter.clone());
        assert!(!player.play_projectile_cue("missing.wav"));
        assert_eq!(counter.get(), 0);
        assert!(
            player.throttle().is_empty(),
            "a failed cue never records a cooldown"
        );
        assert_eq!(player.play_projectile_cues(Vec::<&'static str>::new()), 0);

        let dir = std::env::temp_dir().join(format!(
            "mir2-audio-bad-125-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        std::fs::write(dir.join("bad.mna"), b"not audio").expect("写坏 mna");
        let mut broken =
            SoundPlayer::without_device(SoundLibrary::with_roots(vec![dir.clone()]), true, 1.0);
        broken.set_test_sink(counter);
        assert!(!broken.play_projectile_cue("bad.wav"));
        assert!(broken.throttle().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
