//! 服务端配置。
//!
//! 默认值与 Crystal `Server/Settings.cs` 对齐，但**有意改掉两个会让服务器
//! 开箱即用不了的默认**（见 [`ServerConfig::default`] 的注释）：
//! `allow_start_game` 与 `check_version`。

use std::net::SocketAddr;

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
    /// 地图文件目录（`.map` 文件所在处）。
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
    /// 单会话每秒最多处理多少个客户端包（默认 200）。
    ///
    /// 超限的包**丢弃并记一条 warn**，不踢人（避免误伤网络抖动时的重发包）。
    /// `0` = 不限制。
    pub max_packets_per_second: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
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
            map_root: "resource/map".to_owned(),
            language_dir: "config/lang".to_owned(),
            demo_monsters: false,
            db_path: std::env::var("MIR2_DB_PATH").ok(),
            load_game_data: !matches!(
                std::env::var("MIR2_LOAD_GAME_DATA").ok().as_deref(),
                Some("0" | "false" | "no")
            ),
            gm_accounts: std::env::var(crate::gm::GM_ACCOUNTS_ENV)
                .map(|raw| crate::gm::parse_gm_accounts(&raw))
                .unwrap_or_default(),
            max_connections_per_ip: 5,
            max_connections_per_second: 10,
            idle_timeout_secs: 60,
            max_packets_per_second: 200,
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
        if let Ok(value) = std::env::var(crate::gm::GM_ACCOUNTS_ENV) {
            config.gm_accounts = crate::gm::parse_gm_accounts(&value);
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
            "默认必须放行进游戏，否则本地开发进不去"
        );
        assert!(!config.check_version, "默认不校验客户端版本");
        assert_eq!(config.max_character_count, 4);
    }
}
