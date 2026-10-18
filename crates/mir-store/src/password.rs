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
//! **存储前必须经过 [`encode_hash`]**：lossy 字符串可能含 NUL（0x00 是合法 UTF-8），
//! 直接写 PostgreSQL TEXT 会报 `22021`。Base64 保留全部字节，验证时重算再编码比较，
//! 语义与「直接比较 lossy 字符串」一致（刑部 t7 F4）。
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
///
/// 返回值可能含 NUL（0x00 是合法 UTF-8）与 U+FFFD；**不要**直接写 TEXT 列，
/// 存储前必须先经过 [`encode_hash`]（见迁移 `0010_password_hash_base64.sql`）。
pub fn hash_password(password: &str, salt: &[u8]) -> String {
    let mut derived = [0u8; HASH_LEN];
    pbkdf2_hmac::<Sha1>(password.as_bytes(), salt, ITERATIONS, &mut derived);
    String::from_utf8_lossy(&derived).into_owned()
}

/// 把 Crystal 的 lossy 散列字符串编码成可安全存 TEXT 的 ASCII（标准 Base64）。
///
/// 旧实现把 [`hash_password`] 的结果直接写 `accounts.password_hash TEXT`；当 24 字节
/// 派生物里出现 0x00（合法 UTF-8、概率约 9%）时 PostgreSQL 报
/// `22021 invalid byte sequence "UTF8": 0x00`，建号失败（刑部 t7 F4）。
///
/// Base64 保留 lossy 字符串的**全部 UTF-8 字节**（包括 U+FFFD 的 `EF BF BD` 与 NUL），
/// 输出只含 ASCII，且与迁移 0010 的 `encode(convert_to(...), 'base64')` 一一对应。
/// 验证时用同一函数重算再比较，所以「存储/重算两侧同一变换」的语义不变。
#[must_use]
pub fn encode_hash(lossy: &str) -> String {
    base64_encode(lossy.as_bytes())
}

/// 标准 Base64（RFC 4648，带 `=` 填充），与 PostgreSQL `encode(bytea, 'base64')` 一致。
///
/// 只用 64 个 ASCII 字符，输出不可能含 NUL；解码后逐字节还原输入。
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = u32::from(chunk[0]);
        let second = chunk.get(1).copied().map_or(0, u32::from);
        let third = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (first << 16) | (second << 8) | third;

        encoded.push(char::from(ALPHABET[((triple >> 18) & 0x3F) as usize]));
        encoded.push(char::from(ALPHABET[((triple >> 12) & 0x3F) as usize]));
        if chunk.len() > 1 {
            encoded.push(char::from(ALPHABET[((triple >> 6) & 0x3F) as usize]));
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(char::from(ALPHABET[(triple & 0x3F) as usize]));
        } else {
            encoded.push('=');
        }
    }
    encoded
}

/// 校验明文口令是否与账号里保存的散列一致。
///
/// 使用账号自带的 [`Account::salt`] 重算、再按 [`encode_hash`] 编码后做常量时间比较；
/// 盐或散列被改动都会返回 `false`。
pub fn verify_password(account: &Account, password: &str) -> bool {
    let candidate = encode_hash(&hash_password(password, &account.salt));
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
        self.password_hash = encode_hash(&hash_password(password, &self.salt));
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
            password_hash: encode_hash(&hash_password(password, &salt)),
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

    /// 固定向量：盐 `[8u8; 24]` + 口令 `hunter2` 的 PBKDF2 派生物是
    /// `53caf6c596febaad2e795c4f646f6859f3060903c0008180`，第 21 字节为 0x00。
    ///
    /// 旧代码把这个 lossy 字符串直接写 TEXT，PostgreSQL 报 22021（刑部 t7 F4）。
    /// 这里同时锁住两件事：① 该向量确实含 NUL；② `encode_hash` 之后不含 NUL
    /// 且 Base64 解码能逐字节还原。
    #[test]
    fn encode_hash_keeps_nul_out_of_text() {
        let salt = [8u8; SALT_LEN];
        let lossy = hash_password("hunter2", &salt);

        assert!(
            lossy.as_bytes().contains(&0),
            "固定向量必须含 NUL，否则这个回归测试失去意义",
        );

        let encoded = encode_hash(&lossy);
        assert!(
            !encoded.as_bytes().contains(&0),
            "Base64 输出不能含 NUL：{encoded:?}",
        );
        // 与 Python `base64.b64encode(lossy_bytes)` / PostgreSQL `encode(...,'base64')` 对照。
        assert_eq!(
            encoded, "U++/ve+/vcWW77+977+977+9LnlcT2RvaFnvv70GCQPvv70A77+977+9",
            "标准 Base64（带填充）必须逐字符一致",
        );

        let account = Account {
            salt: salt.to_vec(),
            password_hash: encoded,
            ..Account::default()
        };
        assert!(verify_password(&account, "hunter2"));
        assert!(!verify_password(&account, "hunter3"));
    }

    /// 旧行（迁移 0010 之前的 lossy 文本）经 `encode_hash` 变换后仍能验证同一口令。
    #[test]
    fn legacy_lossy_hash_still_verifies_after_encoding() {
        let salt = fixed_salt();
        let legacy = hash_password("hunter2", &salt);
        let migrated = encode_hash(&legacy);
        let account = Account {
            salt,
            password_hash: migrated,
            ..Account::default()
        };
        assert!(verify_password(&account, "hunter2"));
        assert!(!verify_password(&account, "wrong"));
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
