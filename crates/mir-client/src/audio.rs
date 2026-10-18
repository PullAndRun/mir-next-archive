//! 音效播放后端：`rodio` + 100 ms 频控 + 无音频设备时的优雅降级。
//!
//! # 分工
//!
//! * 事件 → 文件名 的映射在 `mir-render::sound`（纯数据，不依赖音频库）；
//! * **播放**只在这里：`mir-render` 不引入任何音频依赖（需求硬性约束）。
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
//! # 素材目录
//!
//! [`SoundLibrary`] 按顺序在多个目录里找同名文件：
//!
//! 1. `resource/wav`——需求指定的音效目录（786 个经典 Mir2 WAV）；
//! 2. `resource/community/gqf2008-crystal/Sound`——Crystal 客户端完整音效集，
//!    补上 `resource/wav` 里缺的 `levelup.wav`、`pet_pickup.wav` 与 `M<技能>-*.wav`
//!    （`SoundList.lst` 见该目录）。
//!
//! 两个目录都没有该文件时静默跳过（只在第一次遇到时用 `tracing::debug!` 记一笔），
//! 不 panic、不刷屏。

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::{Path, PathBuf};

use mir_core::enums::Spell;
use mir_render::sound::{SOUND_COOLDOWN_MS, SoundId, sound_for_sex, spell_sound};
use rodio::mixer::Mixer;
use rodio::{Decoder, MixerDeviceSink, Source};

/// 默认音量（需求：0.5）。
pub const DEFAULT_VOLUME: f32 = 0.5;

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
}

impl SoundLibrary {
    /// 默认搜索目录：`resource/wav` + Crystal 完整音效集（见模块文档）。
    #[must_use]
    pub fn default_roots() -> Self {
        Self {
            roots: vec![
                PathBuf::from("resource/wav"),
                PathBuf::from("resource/community/gqf2008-crystal/Sound"),
            ],
        }
    }

    /// 用给定目录建库（`--sound-dir`）。
    #[must_use]
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self { roots }
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
            root.join("community").join("gqf2008-crystal").join("Sound"),
        ])
    }

    /// 按仓库根目录拼默认素材目录（测试用）：即 `<root>/resource/data` 的兄弟目录。
    #[must_use]
    pub fn from_repo_root(root: impl AsRef<Path>) -> Self {
        Self::around_data_dir(root.as_ref().join("resource").join("data"))
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
        self.roots
            .iter()
            .map(|root| root.join(file))
            .find(|path| path.is_file())
    }

    /// 这个文件能不能找到。
    #[must_use]
    pub fn contains(&self, file: &str) -> bool {
        self.resolve(file).is_some()
    }
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
    /// 是否启用（`--no-sound` / 设置文件关掉时为 `false`）。
    enabled: bool,
    /// 音频后端；`None` 表示没有声音（关掉了或设备打不开）。
    backend: Option<Backend>,
    /// 已经警告过的文件（避免同一个坏文件刷屏）。
    warned: HashSet<String>,
}

impl std::fmt::Debug for SoundPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoundPlayer")
            .field("roots", &self.library.roots)
            .field("volume", &self.volume)
            .field("enabled", &self.enabled)
            .field("backend", &self.backend.is_some())
            .field("throttle", &self.throttle.len())
            .field("warned", &self.warned.len())
            .finish()
    }
}

impl SoundPlayer {
    /// 建播放器：`enabled == false` 或设备打不开时都是「静音空操作」。
    ///
    /// 设备打不开只会打印**一次**警告（需求：CI 没有音频设备时窗口照常运行）。
    #[must_use]
    pub fn new(library: SoundLibrary, enabled: bool, volume: f32) -> Self {
        let mut player = Self::without_device(library, enabled, volume);
        if enabled {
            match rodio::DeviceSinkBuilder::open_default_sink() {
                Ok(mut sink) => {
                    // 默认 Drop 时会往 stderr 打一行提示，客户端退出时不需要。
                    sink.log_on_drop(false);
                    let mixer = sink.mixer().clone();
                    player.backend = Some(Backend { sink, mixer });
                }
                Err(error) => {
                    eprintln!(
                        "警告：无法打开音频设备（{error}）；音效已禁用，窗口与渲染照常运行。"
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
            enabled,
            backend: None,
            warned: HashSet::new(),
        }
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
        if !self.enabled || self.backend.is_none() {
            return false;
        }
        if !self.throttle.allow(file, now_ms) {
            return false;
        }
        let Some(path) = self.library.resolve(file) else {
            if self.warned.insert(file.to_owned()) {
                tracing::debug!(file, "音效素材缺失，跳过");
            }
            return false;
        };
        let source = match File::open(&path)
            .map_err(|error| error.to_string())
            .and_then(|handle| Decoder::try_from(handle).map_err(|error| error.to_string()))
        {
            Ok(decoder) => decoder.amplify(self.volume),
            Err(error) => {
                if self.warned.insert(file.to_owned()) {
                    eprintln!("警告：音效 {} 解码失败：{error}", path.display());
                }
                return false;
            }
        };
        let Some(backend) = self.backend.as_ref() else {
            return false;
        };
        backend.mixer.add(source);
        true
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
        std::fs::write(directory.join("1.wav"), b"RIFF____WAVE").expect("写假 wav");

        let mut library = SoundLibrary::with_roots(vec![directory.clone()]);
        assert_eq!(library.resolve("1.wav"), Some(directory.join("1.wav")));
        assert!(library.contains("1.wav"));
        assert!(library.resolve("nope.wav").is_none());

        // 前面的目录找不到时继续往后找。
        library.push_front(directory.join("missing"));
        assert_eq!(
            library.resolve("1.wav"),
            Some(directory.join("1.wav")),
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
    fn real_sound_files_resolve_when_the_corpus_is_present() {
        // `resource/` 被 .gitignore 忽略：CI 上没有素材时直接跳过。
        let library = SoundLibrary::default_roots();
        let Some(path) = library.resolve("1.wav") else {
            eprintln!("跳过：resource/wav 里没有 1.wav");
            return;
        };
        assert!(is_wav_file(&path), "{} 应当是 WAV", path.display());
    }
}
