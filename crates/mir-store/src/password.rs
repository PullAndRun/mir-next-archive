//! 与 Crystal 完全一致的口令散列。
//!
//! Crystal `Server/Utils/Crypto.cs` 的算法：
//!
//! ```text
//! Rfc2898DeriveBytes(password, salt, iterations: 50, HashAlgorithmName.SHA1).GetBytes(24)
//! → Encoding.UTF8.GetString(bytes)  // 把 24 个随机字节当 UTF-8 解码
//! ```
//!
//! 24 个随机字节几乎必然不是合法 UTF-8，`Encoding.UTF8.GetString` 会按
//! Unicode 的「最大子部分替换」规则把非法序列换成 `U+FFFD`，因此最终字符串长度是
//! 6~24 个 `char`、重新编码后 24~72 字节，且**不可逆**。这里用
//! [`String::from_utf8_lossy`] 复刻同一规则（.NET Core 3.0 起与 Unicode 一致，
//! 见 <https://learn.microsoft.com/dotnet/core/compatibility/2.2-3.0>），
//! 保证新库与 Crystal 存档里的口令串可以互相校验。
//!
//! 比较口令时使用常量时间比较；Crystal 用的是 `string.CompareOrdinal`。

use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha1::Sha1;

use crate::models::Account;

/// 盐长度（Crystal `Crypto.SaltSize`）。
pub const SALT_LEN: usize = 24;

/// 派生结果长度（Crystal `Crypto.HashSize`）。
pub const HASH_LEN: usize = 24;

/// PBKDF2 迭代次数（Crystal `Crypto.Iterations`）。
pub const ITERATIONS: u32 = 50;

/// 生成 24 字节随机盐（Crystal `Crypto.GenerateSalt`）。
///
/// 线程本地 CSPRNG（`rand::rng()`）以操作系统熵为种子。
pub fn generate_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    rand::rng().fill_bytes(&mut salt);
    salt
}

/// 按 Crystal 的方式散列口令：PBKDF2-HMAC-SHA1（50 次、24 字节输出）+ UTF-8 替换解码。
///
/// 同一组 `(password, salt)` 结果稳定；`salt` 长度不受限制（Crystal 固定 24 字节）。
pub fn hash_password(password: &str, salt: &[u8]) -> String {
    let mut derived = [0u8; HASH_LEN];
    pbkdf2_hmac::<Sha1>(password.as_bytes(), salt, ITERATIONS, &mut derived);
    String::from_utf8_lossy(&derived).into_owned()
}

/// 校验明文口令是否与账号里保存的散列一致。
///
/// 使用账号自带的 [`Account::salt`] 重算后再做常量时间比较；盐或散列被改动都会返回 `false`。
pub fn verify_password(account: &Account, password: &str) -> bool {
    let candidate = hash_password(password, &account.salt);
    constant_time_eq(candidate.as_bytes(), account.password_hash.as_bytes())
}

/// 逐字节比较，避免提前返回泄露前缀信息。
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right) {
        difference |= a ^ b;
    }

    difference == 0
}

impl Account {
    /// 重新设置明文口令：生成新盐并散列。
    ///
    /// 对应 Crystal `AccountInfo.Password` 的 setter（`AccountInfo.cs:21-30`），
    /// 每次赋值都会重新 `GenerateSalt()` 再散列。改完要调用 [`crate::save_account`] 才会写库。
    pub fn set_password(&mut self, password: &str) {
        self.salt = generate_salt().to_vec();
        self.password_hash = hash_password(password, &self.salt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 0x00..0x17 共 24 字节，便于人工核对。
    fn fixed_salt() -> Vec<u8> {
        (0u8..24).collect()
    }

    /// 固定盐 + 固定口令的期望值。
    ///
    /// 参考值由两路独立实现算出并逐字符比对过：
    /// 1. `Rfc2898DeriveBytes("hunter2", salt, 50, SHA1).GetBytes(24)` +
    ///    `Encoding.UTF8.GetString`（.NET 运行时直接执行 Crystal 的原始 C# 语义）；
    /// 2. Python `hashlib.pbkdf2_hmac("sha1", ...)` + UTF-8 替换解码。
    ///
    /// 两者的派生字节都是 `9d3c0a8494e89b61d070ea1574b53e4207e33270a26aa73e`，
    /// 解码结果如下（`U+FFFD` 即非法 UTF-8 序列的替换字符）。
    const CRYSTAL_VECTOR_HUNTER2: &str = "\u{FFFD}<\n\u{FFFD}\u{FFFD}\u{FFFD}a\u{FFFD}p\u{FFFD}\u{15}t\u{FFFD}>B\u{7}\u{FFFD}2p\u{FFFD}j\u{FFFD}>";

    /// 非 ASCII 口令走 UTF-8 编码的固定向量（盐同上）。
    const CRYSTAL_VECTOR_UNICODE: &str = "\u{FFFD}e\u{16}X\u{FFFD}e6A\u{FFFD}\u{FFFD}\u{FFFD}\u{13}\u{FFFD})\u{FFFD}~\u{FFFD}f\u{FFFD}A\u{FFFD}\u{5}\u{FFFD}+";

    fn account_with(password: &str) -> Account {
        let salt = fixed_salt();
        Account {
            password_hash: hash_password(password, &salt),
            salt,
            ..Account::default()
        }
    }

    #[test]
    fn hash_password_is_deterministic() {
        let salt = fixed_salt();
        let first = hash_password("hunter2", &salt);
        let second = hash_password("hunter2", &salt);

        assert_eq!(first, second, "同一口令与盐必须产生同一散列");
        assert_eq!(
            hash_password("hunter2", &[0u8; SALT_LEN]),
            hash_password("hunter2", &[0u8; SALT_LEN]),
        );
        assert_ne!(
            first,
            hash_password("hunter2", &[1u8; SALT_LEN]),
            "换盐必须换散列",
        );
        assert_ne!(first, hash_password("hunter3", &salt), "换口令必须换散列");
    }

    #[test]
    fn hash_password_matches_crystal_vector() {
        assert_eq!(
            hash_password("hunter2", &fixed_salt()),
            CRYSTAL_VECTOR_HUNTER2
        );
    }

    #[test]
    fn hash_password_encodes_unicode_password_as_utf8() {
        assert_eq!(
            hash_password("密码abc", &fixed_salt()),
            CRYSTAL_VECTOR_UNICODE,
        );
    }

    #[test]
    fn hash_password_produces_replacement_characters() {
        let hash = hash_password("hunter2", &fixed_salt());

        assert!(
            hash.chars().any(|c| c == '\u{FFFD}'),
            "24 个随机字节里的非法 UTF-8 必须被替换成 U+FFFD",
        );
        assert!(
            hash.len() > HASH_LEN,
            "替换字符的 UTF-8 编码是 3 字节，散列串因此长于 24 字节",
        );
    }

    #[test]
    fn verify_password_accepts_only_the_original_password() {
        let account = account_with("hunter2");

        assert!(verify_password(&account, "hunter2"));
        assert!(!verify_password(&account, "hunter3"));
        assert!(!verify_password(&account, ""));
        assert!(!verify_password(&account, "HUNTER2"), "口令区分大小写");
    }

    #[test]
    fn verify_password_rejects_tampered_hash_or_salt() {
        let mut tampered_hash = account_with("hunter2");
        tampered_hash.password_hash.push('x');
        assert!(!verify_password(&tampered_hash, "hunter2"));

        let mut tampered_salt = account_with("hunter2");
        tampered_salt.salt[0] ^= 0xFF;
        assert!(!verify_password(&tampered_salt, "hunter2"));
    }

    #[test]
    fn generate_salt_returns_24_fresh_bytes() {
        let first = generate_salt();
        let second = generate_salt();

        assert_eq!(first.len(), SALT_LEN);
        assert_ne!(first, second, "两次生成的盐不能相同");
    }

    #[test]
    fn set_password_regenerates_salt_and_hash() {
        let mut account = account_with("hunter2");
        let old_salt = account.salt.clone();
        let old_hash = account.password_hash.clone();

        account.set_password("newsecret");

        assert_ne!(account.salt, old_salt, "改密必须换新盐");
        assert_ne!(account.password_hash, old_hash, "改密必须换新散列");
        assert_eq!(account.salt.len(), SALT_LEN);
        assert!(verify_password(&account, "newsecret"));
        assert!(!verify_password(&account, "hunter2"));
    }
}
