//! 资源根的解析与派生目录（`game-assets/` 优先，缺失时回落 `resource/`）。
//!
//! 优先级（高 → 低），与 `docs/assets/game-assets.md` 一致：
//!
//! 1. `--asset-dir <path>`（命令行）
//! 2. 环境变量 `MIR2_ASSET_DIR`
//! 3. `config/client.toml` 的 `asset_root`
//! 4. 默认 `game-assets`
//! 5. 回落 `resource/`（**默认值缺失只打 info，显式配置缺失打 warn**）
//!
//! 为什么要这一层：素材仓库 `resource/` 有 23GB 的原始归档，游戏实际只用到其中一部分；
//! `tools/assets/build_game_assets.py` 把这些抽成独立目录后，运行时只认这一份，
//! 用户也可以把素材放到任意盘符再用 `--asset-dir` / 环境变量指过去。

use std::path::{Path, PathBuf};

/// 从素材目录推拆包目录：`<asset_root>/data` → `<asset_root>/gfx`。
///
/// 拆包产物按库名分目录（`gfx/items/index.json`），所以知道 `data` 的父目录即可。
#[must_use]
pub fn gfx_dir_for_data_dir(data_dir: &Path) -> PathBuf {
    data_dir
        .parent()
        .map_or_else(|| PathBuf::from("gfx"), |root| root.join("gfx"))
}

/// 默认资源根（抽取器 [`tools/assets/build_game_assets.py`] 的默认输出）。
pub const DEFAULT_ASSET_ROOT: &str = "game-assets";
/// 没有抽取过时的回落目录（旧行为，保证 CI/测试不装素材也能跑）。
pub const FALLBACK_ASSET_ROOT: &str = "resource";
/// 环境变量：指定资源根。
pub const ASSET_ROOT_ENV: &str = "MIR2_ASSET_DIR";

/// 资源根是**从哪一档**解析出来的（测试与日志都要它）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetRootSource {
    /// `--asset-dir`
    Cli,
    /// `MIR2_ASSET_DIR`
    Env,
    /// `config/client.toml` 的 `asset_root`
    Config,
    /// 默认 `game-assets`
    Default,
    /// 默认值不存在，回落到 `resource`
    Fallback,
}

impl AssetRootSource {
    /// 是否属于「用户显式指定」——缺失时该打 warn 而不是 info。
    #[must_use]
    pub fn is_explicit(self) -> bool {
        matches!(self, Self::Cli | Self::Env | Self::Config)
    }

    /// 日志/测试用的短名。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Env => "env",
            Self::Config => "config",
            Self::Default => "default",
            Self::Fallback => "fallback",
        }
    }
}

/// 解析后的资源根与它的派生目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRoot {
    root: PathBuf,
    source: AssetRootSource,
}

impl AssetRoot {
    /// 按优先级解析资源根。
    ///
    /// - `cli` / `env` / `config` 传 `None` 表示该档没有配置；
    /// - 选中的路径**必须存在且是目录**才采用，否则回落 `resource`（并记录来源档位，
    ///   调用方据此决定打 `warn` 还是 `info`）。
    #[must_use]
    pub fn resolve(cli: Option<&Path>, env: Option<&str>, config: Option<&str>) -> Self {
        if let Some(path) = cli {
            return Self::adopt(path, AssetRootSource::Cli);
        }
        if let Some(raw) = env {
            return Self::adopt(Path::new(raw), AssetRootSource::Env);
        }
        if let Some(raw) = config {
            return Self::adopt(Path::new(raw), AssetRootSource::Config);
        }
        let default = PathBuf::from(DEFAULT_ASSET_ROOT);
        if default.is_dir() {
            return Self {
                root: default,
                source: AssetRootSource::Default,
            };
        }
        Self {
            root: PathBuf::from(FALLBACK_ASSET_ROOT),
            source: AssetRootSource::Fallback,
        }
    }

    /// 采用某个显式路径；不存在则回落 `resource`（保留来源档位，供调用方打 warn）。
    fn adopt(path: &Path, source: AssetRootSource) -> Self {
        if path.is_dir() {
            return Self {
                root: path.to_path_buf(),
                source,
            };
        }
        Self {
            root: PathBuf::from(FALLBACK_ASSET_ROOT),
            source: if source.is_explicit() {
                source
            } else {
                AssetRootSource::Fallback
            },
        }
    }

    /// 只给测试/工具用：不碰文件系统，直接构造。
    #[must_use]
    pub fn from_parts(root: impl Into<PathBuf>, source: AssetRootSource) -> Self {
        Self {
            root: root.into(),
            source,
        }
    }

    /// 资源根路径（`game-assets` 或 `resource`）。
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 解析来源档位。
    #[must_use]
    pub fn source(&self) -> AssetRootSource {
        self.source
    }

    /// 地图目录：`<root>/map`。
    #[must_use]
    pub fn map_dir(&self) -> PathBuf {
        self.root.join("map")
    }

    /// 客户端图库目录：`<root>/data`。
    #[must_use]
    pub fn data_dir(&self) -> PathBuf {
        self.root.join("data")
    }

    /// 音效目录：`<root>/wav`。
    #[must_use]
    pub fn wav_dir(&self) -> PathBuf {
        self.root.join("wav")
    }

    /// 字体目录：`<root>/fonts`。
    #[must_use]
    pub fn fonts_dir(&self) -> PathBuf {
        self.root.join("fonts")
    }

    /// 图库索引目录：`<root>/ld`。
    #[must_use]
    pub fn ld_dir(&self) -> PathBuf {
        self.root.join("ld")
    }

    /// 服务端数据库目录：`<root>/server`。
    #[must_use]
    pub fn server_dir(&self) -> PathBuf {
        self.root.join("server")
    }

    /// 打一条日志：默认值缺失用 `info`，显式配置缺失用 `warn`（不掩盖配置错误）。
    pub fn log_resolution(&self) {
        let root = self.root.display();
        match self.source {
            AssetRootSource::Fallback => {
                tracing::info!(
                    root = %root,
                    "未找到 game-assets/ → 回落到 resource/（行为与旧版一致）"
                );
            }
            AssetRootSource::Cli | AssetRootSource::Env | AssetRootSource::Config => {
                if self.root == Path::new(FALLBACK_ASSET_ROOT) {
                    tracing::warn!(
                        source = self.source.as_str(),
                        "指定的资源目录不存在 → 回落到 resource/（请检查 --asset-dir / MIR2_ASSET_DIR / asset_root）"
                    );
                } else {
                    tracing::info!(root = %root, source = self.source.as_str(), "资源根已采用");
                }
            }
            AssetRootSource::Default => {
                tracing::info!(root = %root, "资源根已采用（默认 game-assets/）");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mir2-asset-root-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("时钟")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        dir
    }

    #[test]
    fn cli_wins_over_env_and_config() {
        let cli = temp_dir("cli");
        let env = temp_dir("env");
        let config = temp_dir("config");
        let resolved = AssetRoot::resolve(
            Some(&cli),
            Some(env.to_str().expect("utf8")),
            Some(config.to_str().expect("utf8")),
        );
        assert_eq!(resolved.root(), cli.as_path());
        assert_eq!(resolved.source(), AssetRootSource::Cli);
    }

    #[test]
    fn env_wins_over_config() {
        let env = temp_dir("env2");
        let config = temp_dir("config2");
        let resolved = AssetRoot::resolve(
            None,
            Some(env.to_str().expect("utf8")),
            Some(config.to_str().expect("utf8")),
        );
        assert_eq!(resolved.root(), env.as_path());
        assert_eq!(resolved.source(), AssetRootSource::Env);
    }

    #[test]
    fn config_used_when_cli_and_env_absent() {
        let config = temp_dir("config3");
        let resolved = AssetRoot::resolve(None, None, Some(config.to_str().expect("utf8")));
        assert_eq!(resolved.root(), config.as_path());
        assert_eq!(resolved.source(), AssetRootSource::Config);
    }

    #[test]
    fn explicit_missing_falls_back_to_resource_but_keeps_source() {
        let missing = std::env::temp_dir().join("mir2-asset-root-does-not-exist-xyz");
        let resolved = AssetRoot::resolve(Some(&missing), None, None);
        assert_eq!(resolved.root(), Path::new(FALLBACK_ASSET_ROOT));
        assert_eq!(
            resolved.source(),
            AssetRootSource::Cli,
            "要保留来源档位，调用方才能打 warn"
        );
        assert!(resolved.source().is_explicit());
    }

    #[test]
    fn derived_dirs_are_under_the_root() {
        let root = AssetRoot::from_parts("D:/mir-assets", AssetRootSource::Cli);
        assert_eq!(root.map_dir(), PathBuf::from("D:/mir-assets/map"));
        assert_eq!(root.data_dir(), PathBuf::from("D:/mir-assets/data"));
        assert_eq!(root.wav_dir(), PathBuf::from("D:/mir-assets/wav"));
        assert_eq!(root.fonts_dir(), PathBuf::from("D:/mir-assets/fonts"));
        assert_eq!(root.ld_dir(), PathBuf::from("D:/mir-assets/ld"));
        assert_eq!(root.server_dir(), PathBuf::from("D:/mir-assets/server"));
    }

    #[test]
    fn default_or_fallback_depends_on_existence() {
        // 只断言"要么是默认、要么是回落"，避免依赖仓库里是否真的抽过素材。
        let resolved = AssetRoot::resolve(None, None, None);
        assert!(
            matches!(
                resolved.source(),
                AssetRootSource::Default | AssetRootSource::Fallback
            ),
            "未配置时应只在 Default/Fallback 之间"
        );
        if resolved.source() == AssetRootSource::Fallback {
            assert_eq!(resolved.root(), Path::new(FALLBACK_ASSET_ROOT));
        }
    }
}
