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
        config
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
