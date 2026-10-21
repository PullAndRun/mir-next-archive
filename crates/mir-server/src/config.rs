//! 服务端配置。
//!
//! 默认值与 Crystal `Server/Settings.cs` 对齐，但**有意改掉两个会让服务器
//! 开箱即用不了的默认**（见 [`ServerConfig::default`] 的注释）：
//! `allow_start_game` 与 `check_version`。

use std::net::SocketAddr;
use std::path::Path;

/// 默认资源根（`tools/assets/build_game_assets.py` 的默认输出）。
pub const DEFAULT_ASSET_ROOT: &str = "game-assets";
/// 没有抽取过 `game-assets/` 时的回落目录（旧行为，保证测试/CI 不装素材也能跑）。
pub const FALLBACK_ASSET_ROOT: &str = "resource";
/// 环境变量：指定资源根（优先级高于配置、低于命令行显式赋值）。
pub const ASSET_ROOT_ENV: &str = "MIR2_ASSET_DIR";
/// 环境变量：显式指定地图目录（给了就不再从 `asset_root` 派生）。
pub const MAP_ROOT_ENV: &str = "MIR2_MAP_ROOT";

/// 资源根是从哪一档解析出来的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetRootSource {
    /// 配置里显式写了 `asset_root`
    Config,
    /// 环境变量 `MIR2_ASSET_DIR`
    Env,
    /// 默认 `game-assets`
    Default,
    /// 默认值不存在，回落到 `resource`
    Fallback,
}

impl AssetRootSource {
    /// 是否属于「用户显式指定」——缺失时该打 warn 而不是 info。
    #[must_use]
    pub fn is_explicit(self) -> bool {
        matches!(self, Self::Config | Self::Env)
    }

    /// 日志用的短名。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::Env => "env",
            Self::Default => "default",
            Self::Fallback => "fallback",
        }
    }
}

/// 解析资源根：显式配置 > 环境变量 > 默认 `game-assets` > 回落 `resource`。
///
/// 返回的路径**保证是存在的目录**（或回落的 `resource`）；
/// 第二个返回值告诉调用方"这一档是不是显式指定的"，用于决定日志级别。
#[must_use]
pub fn resolve_asset_root(configured: Option<&str>) -> (String, AssetRootSource) {
    if let Some(raw) = configured {
        return adopt_asset_root(raw, AssetRootSource::Config);
    }
    if let Ok(raw) = std::env::var(ASSET_ROOT_ENV) {
        if !raw.trim().is_empty() {
            return adopt_asset_root(raw.trim(), AssetRootSource::Env);
        }
    }
    if Path::new(DEFAULT_ASSET_ROOT).is_dir() {
        return (DEFAULT_ASSET_ROOT.to_owned(), AssetRootSource::Default);
    }
    tracing::info!(
        fallback = FALLBACK_ASSET_ROOT,
        "未找到 game-assets/ → 回落到 resource/（行为与旧版一致；可用 MIR2_ASSET_DIR 指定）"
    );
    (FALLBACK_ASSET_ROOT.to_owned(), AssetRootSource::Fallback)
}

/// 采用显式路径；不存在则回落 `resource`，但**保留来源档位**让调用方打 warn。
fn adopt_asset_root(raw: &str, source: AssetRootSource) -> (String, AssetRootSource) {
    if Path::new(raw).is_dir() {
        return (raw.to_owned(), source);
    }
    tracing::warn!(
        source = source.as_str(),
        %raw,
        fallback = FALLBACK_ASSET_ROOT,
        "指定的资源目录不存在 → 回落到 resource/（请检查 MIR2_ASSET_DIR / asset_root 配置）"
    );
    (FALLBACK_ASSET_ROOT.to_owned(), source)
}

/// 从资源根派生默认地图目录：`<asset_root>/map`。
#[must_use]
pub fn derived_map_root(asset_root: &str) -> String {
    format!("{asset_root}/map")
}

/// 从资源根派生默认数据库路径：`<asset_root>/server/Server.MirDB`（存在才返回）。
#[must_use]
pub fn derived_db_path(asset_root: &str) -> Option<String> {
    let candidate = format!("{asset_root}/server/Server.MirDB");
    Path::new(&candidate).is_file().then_some(candidate)
}

/// 服务端运行配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    /// 监听地址。
    pub listen: SocketAddr,
    /// 是否校验客户端版本哈希。
    pub check_version: bool,
    /// 允许的版本哈希（`check_version` 为真时使用）。
    pub version_hashes: Vec<Vec<u8>>,
    /// 是否允许登录。
    pub allow_login: bool,
    /// 是否允许注册。
    pub allow_new_account: bool,
    /// 是否允许建角色。
    pub allow_new_character: bool,
    /// 是否允许删角色。
    pub allow_delete_character: bool,
    /// 是否允许进入游戏。
    pub allow_start_game: bool,
    /// 每个账号最多角色数（Crystal `Globals.MaxCharacterCount`）。
    pub max_character_count: usize,
    /// 上报给客户端的最大分辨率。
    pub allowed_resolution: i32,
    /// 新角色的地图名。
    pub start_map: String,
    /// 新角色的出生坐标。
    pub start_point: (i32, i32),
    /// 资源根（`game-assets/` 或自定义路径）。
    ///
    /// 解析顺序：配置显式值 > `MIR2_ASSET_DIR` > 默认 `game-assets` > 回落 `resource`；
    /// 见 [`resolve_asset_root`]。
    pub asset_root: String,
    /// 资源根是从哪一档来的（测试与日志用）。
    pub asset_root_source: AssetRootSource,
    /// 地图文件目录（`.map` 文件所在处）。
    ///
    /// 默认由 [`Self::asset_root`] 派生为 `<asset_root>/map`；
    /// 显式赋值（或 `MIR2_MAP_ROOT`）优先。
    pub map_root: String,
    /// 语言包目录（`*.json`）；目录不存在时使用编译期内置语言包。
    pub language_dir: String,
    /// 是否在出生点附近刷一批**演示怪物**（占位数值，见 `monster::MonsterKind`）。
    ///
    /// Crystal 的刷怪完全由数据库的 `RespawnInfo` 驱动；接入 `Server.MirDB` 之后，
    /// 真实刷怪由 [`Self::db_path`] 提供，这个开关只用于「没有数据库时也能看到战斗」。
    pub demo_monsters: bool,
    /// `Server.MirDB` 路径。`None` 时按 `gamedata::DB_CANDIDATES` 依次查找索引里的样本。
    pub db_path: Option<String>,
    /// 是否从 `Server.MirDB` 读怪物/刷怪点填充世界。
    ///
    /// 测试里关掉它，世界就只包含测试自己放的怪物，断言才不会被真实数据打乱。
    pub load_game_data: bool,
    /// GM 账号名单（`MIR2_GM_ACCOUNTS`，逗号分隔账号名）。
    ///
    /// 与 `accounts.admin`（Crystal `AccountInfo.AdminAccount`）取**或**：名单里的账号
    /// 即使 `admin = false` 也能用 `@` 命令（运维临时授权）。判定见
    /// [`crate::gm::is_gm`]。默认空 = 只认 `accounts.admin`。
    pub gm_accounts: Vec<String>,
    /// **抗刷**：同一个 IP 允许的并发连接数（默认 5）。超限的连接会被立刻关闭。
    ///
    /// Crystal 没有这一层（`Server.cs` 只做 accept）；本作加上是为了挡掉端口扫描与
    /// 单机多开刷号。`0` = 不限制。
    pub max_connections_per_ip: usize,
    /// **抗刷**：同一个 IP 每秒最多新建多少条连接（默认 10）。超限的连接会被立刻关闭。
    ///
    /// `0` = 不限制。
    pub max_connections_per_second: usize,
    /// **心跳超时**：多少秒收不到客户端的**任何**包就断开（默认 60）。
    ///
    /// Crystal 的客户端每分钟发一次 `C.KeepAlive`（`MirConnection.cs`），所以「有包」
    /// 就等于「客户端还活着」；`0` = 不超时。
    pub idle_timeout_secs: u64,
    /// `/metrics` 端点监听地址（`MIR2_METRICS_LISTEN`）；`None` = **默认关闭**。
    ///
    /// 只读端点（Prometheus 文本），不鉴权：生产环境请绑内网/回环地址。
    pub metrics_listen: Option<SocketAddr>,
    /// 单会话每秒最多处理多少个客户端包（默认 200）。
    ///
    /// 超限的包**丢弃并记一条 warn**，不踢人（避免误伤网络抖动时的重发包）。
    /// `0` = 不限制。
    pub max_packets_per_second: u32,
    /// 好友/黑名单上限（`MIR2_MAX_FRIENDS`，默认 100）。
    ///
    /// `0` = 不限（与本文件其它抗刷阈值口径一致）。
    /// **本作扩展**：Crystal `CharacterInfo.Friends` 是无上限 `List<FriendInfo>`。
    pub max_friends: usize,
    /// **周期自动存档**：每隔多少秒把**脏**角色行刷一次库（`MIR2_AUTOSAVE_SECS`，默认 30）。
    ///
    /// `0` = 关闭（只在 LogOut/断线/市场事务里落盘）。取 N 意味着**进程崩溃最多丢
    /// N + 1 秒的改动**（RPO 有界；多出来的 1 秒是 autosave 轮询粒度）。
    ///
    /// Crystal 没有周期存档（`Envir.cs` 只在 `PlayerObject.LogOut` 写 `Characters` 表），
    /// 本作是**扩展**：崩服回档到登录时刻是不可接受的玩家资产问题（见 `docs/world/persistence.md`）。
    pub autosave_secs: u64,
    /// 行会盟友上限（`MIR2_MAX_GUILD_ALLIES`，默认 5）。
    ///
    /// `0` = 不限。**本作扩展**：Crystal `GuildObject.AllyGuilds` 是无上限 `List`，
    /// `Globals` 也没有 `MaxGuildAllies` 常量。
    pub max_guild_allies: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        let (asset_root, asset_root_source) = resolve_asset_root(None);
        let map_root = std::env::var(MAP_ROOT_ENV)
            .ok()
            .filter(|raw| !raw.trim().is_empty())
            .unwrap_or_else(|| derived_map_root(&asset_root));
        let db_path = std::env::var("MIR2_DB_PATH")
            .ok()
            .filter(|raw| !raw.trim().is_empty())
            .or_else(|| derived_db_path(&asset_root));
        Self {
            asset_root,
            asset_root_source,
            listen: "0.0.0.0:7000".parse().expect("固定地址字面量总能解析"),
            // Crystal 默认 `CheckVersion = true` 且要求 `Configs/Setup.ini` 里配好
            // 客户端 exe；在只跑服务端的场景下那会让所有连接被拒，因此默认关闭。
            check_version: false,
            version_hashes: Vec::new(),
            allow_login: true,
            allow_new_account: true,
            allow_new_character: true,
            allow_delete_character: true,
            // Crystal 默认 `AllowStartGame = false`（只有 GM 能进游戏）。
            // 这里默认放行，否则单人本地开发根本进不去世界。
            allow_start_game: true,
            max_character_count: 4,
            allowed_resolution: 1024,
            start_map: "0".to_owned(),
            start_point: (330, 270),
            map_root,
            language_dir: "config/lang".to_owned(),
            demo_monsters: false,
            db_path,
            load_game_data: !matches!(
                std::env::var("MIR2_LOAD_GAME_DATA").ok().as_deref(),
                Some("0" | "false" | "no")
            ),
            gm_accounts: std::env::var(crate::gm::GM_ACCOUNTS_ENV)
                .map(|raw| crate::gm::parse_gm_accounts(&raw))
                .unwrap_or_default(),
            metrics_listen: std::env::var("MIR2_METRICS_LISTEN")
                .ok()
                .and_then(|raw| match raw.parse::<SocketAddr>() {
                    Ok(address) => Some(address),
                    Err(error) => {
                        tracing::warn!(%raw, %error, "MIR2_METRICS_LISTEN 解析失败，metrics 不启动");
                        None
                    }
                }),
            max_connections_per_ip: 5,
            max_connections_per_second: 10,
            idle_timeout_secs: 60,
            max_packets_per_second: 200,
            max_friends: 100,
            autosave_secs: 30,
            max_guild_allies: 5,
        }
    }
}

impl ServerConfig {
    /// 从环境变量覆盖默认值：`MIR2_LISTEN`、`MIR2_START_MAP` 等。
    #[must_use]
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Ok(value) = std::env::var("MIR2_LISTEN") {
            match value.parse() {
                Ok(address) => config.listen = address,
                Err(error) => {
                    tracing::warn!(%value, %error, "MIR2_LISTEN 解析失败，沿用默认监听地址");
                }
            }
        }
        if let Ok(value) = std::env::var("MIR2_CHECK_VERSION") {
            config.check_version = matches!(value.as_str(), "1" | "true" | "yes");
        }
        if let Ok(value) = std::env::var("MIR2_ALLOW_START_GAME") {
            config.allow_start_game = matches!(value.as_str(), "1" | "true" | "yes");
        }
        if let Ok(value) = std::env::var("MIR2_START_MAP") {
            config.start_map = value;
        }
        if let Ok(value) = std::env::var("MIR2_DEMO_MONSTERS") {
            config.demo_monsters = matches!(value.as_str(), "1" | "true" | "yes");
        }
        // 资源根：`MIR2_ASSET_DIR` 优先于默认；派生目录只在用户没显式指定时更新。
        if let Ok(raw) = std::env::var(ASSET_ROOT_ENV) {
            if !raw.trim().is_empty() {
                let explicit_map = std::env::var(MAP_ROOT_ENV).ok();
                let explicit_db = std::env::var("MIR2_DB_PATH").ok();
                let (root, source) = adopt_asset_root(raw.trim(), AssetRootSource::Env);
                let old_derived = derived_map_root(&config.asset_root);
                config.asset_root = root;
                config.asset_root_source = source;
                if explicit_map.is_none() && config.map_root == old_derived {
                    config.map_root = derived_map_root(&config.asset_root);
                }
                if explicit_db.is_none() {
                    config.db_path = derived_db_path(&config.asset_root);
                }
            }
        }
        if let Ok(value) = std::env::var(crate::gm::GM_ACCOUNTS_ENV) {
            config.gm_accounts = crate::gm::parse_gm_accounts(&value);
        }
        if let Ok(value) = std::env::var("MIR2_METRICS_LISTEN") {
            match value.parse::<SocketAddr>() {
                Ok(address) => config.metrics_listen = Some(address),
                Err(error) => {
                    tracing::warn!(%value, %error, "MIR2_METRICS_LISTEN 解析失败，沿用默认（关闭）");
                }
            }
        }
        if let Some(value) = parse_env_usize("MIR2_MAX_CONN_PER_IP") {
            config.max_connections_per_ip = value;
        }
        if let Some(value) = parse_env_usize("MIR2_MAX_CONN_PER_SEC") {
            config.max_connections_per_second = value;
        }
        if let Some(value) = parse_env_usize("MIR2_IDLE_TIMEOUT_SECS") {
            config.idle_timeout_secs = u64::try_from(value).unwrap_or(u64::MAX);
        }
        if let Some(value) = parse_env_usize("MIR2_MAX_PACKETS_PER_SEC") {
            config.max_packets_per_second = u32::try_from(value).unwrap_or(u32::MAX);
        }
        if let Some(value) = parse_env_usize("MIR2_MAX_FRIENDS") {
            config.max_friends = value;
        }
        if let Some(value) = parse_env_usize("MIR2_MAX_GUILD_ALLIES") {
            config.max_guild_allies = value;
        }
        if let Some(value) = parse_env_usize("MIR2_AUTOSAVE_SECS") {
            config.autosave_secs = u64::try_from(value).unwrap_or(u64::MAX);
        }
        config
    }
}

/// 读一个无符号环境变量；缺失/非法返回 `None` 并记一条 warn。
fn parse_env_usize(name: &str) -> Option<usize> {
    let raw = std::env::var(name).ok()?;
    match raw.parse::<usize>() {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::warn!(%name, %raw, %error, "环境变量不是合法整数，沿用默认值");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_playable() {
        let config = ServerConfig::default();
        assert!(
            config.allow_start_game,
            "default must allow entering the game for local development"
        );
        assert!(!config.check_version, "version check defaults to off");
        assert_eq!(config.max_character_count, 4);
    }

    fn temp_asset_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mir2-server-asset-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn explicit_asset_root_wins_and_is_adopted() {
        let dir = temp_asset_dir("explicit");
        let raw = dir.to_str().expect("utf8").to_owned();
        let (root, source) = resolve_asset_root(Some(&raw));
        assert_eq!(root, raw);
        assert_eq!(source, AssetRootSource::Config);
        assert!(source.is_explicit());
    }

    #[test]
    fn missing_explicit_asset_root_falls_back_to_resource() {
        let missing = std::env::temp_dir().join("mir2-server-asset-does-not-exist-xyz");
        let (root, source) = resolve_asset_root(missing.to_str());
        assert_eq!(root, FALLBACK_ASSET_ROOT);
        assert_eq!(
            source,
            AssetRootSource::Config,
            "keep the resolution tier so callers can warn"
        );
        assert!(source.is_explicit());
    }

    #[test]
    fn map_root_derives_from_asset_root() {
        let dir = temp_asset_dir("derive");
        let raw = dir.to_str().expect("utf8").to_owned();
        assert_eq!(derived_map_root(&raw), format!("{raw}/map"));
        assert_eq!(
            derived_db_path(&raw),
            None,
            "empty dir has no server/Server.MirDB"
        );

        // 造一个 server/Server.MirDB，派生路径应当出现
        std::fs::create_dir_all(dir.join("server")).expect("create server dir");
        std::fs::write(dir.join("server").join("Server.MirDB"), b"mir2").expect("write db file");
        assert_eq!(
            derived_db_path(&raw),
            Some(format!("{raw}/server/Server.MirDB"))
        );
    }

    /// 复刻 `from_env` 的派生逻辑（正式实现里内联在 `from_env`，这里只验证规则）：
    /// 显式 `map_root` 与旧派生值不同 → 换资源根时**不覆盖**。
    fn adopt_asset_root(config: &mut ServerConfig, root: &str) {
        let old_derived = derived_map_root(&config.asset_root);
        config.asset_root = root.to_owned();
        if config.map_root == old_derived {
            config.map_root = derived_map_root(&config.asset_root);
        }
        config.db_path = derived_db_path(&config.asset_root);
    }

    #[test]
    fn explicit_map_root_is_not_overwritten_by_derivation() {
        let dir = temp_asset_dir("keep-map");
        let raw = dir.to_str().expect("utf8").to_owned();
        let mut config = ServerConfig {
            map_root: "D:/custom-maps".to_owned(),
            ..ServerConfig::default()
        };
        adopt_asset_root(&mut config, &raw);
        assert_eq!(
            config.map_root, "D:/custom-maps",
            "explicit map_root must win"
        );
        assert_eq!(config.asset_root, raw);

        let mut derived = ServerConfig::default();
        adopt_asset_root(&mut derived, &raw);
        assert_eq!(derived.map_root, format!("{raw}/map"));
    }
}
