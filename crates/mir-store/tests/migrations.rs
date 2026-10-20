//! 存档迁移演练（第 17 轮 · JJC-20261020-004-sub-1f7a4a01）。
//!
//! 对应 `tools/dev/migrate_drill.ps1`：脚本负责**环境校验**（只认 `MIR2_DRILL_DATABASE_URL`、
//! 拒绝回退、库名前缀/本地 host 断言），本测试负责**真实的迁移演练**：
//!
//! 1. 在 `MIR2_DRILL_DATABASE_URL` 指向的服务器上建一个**唯一**演练库
//!    `mir2_drill_<pid>_<ts>`（`TEMPLATE template0`），跑完全部迁移；
//! 2. 逐表校验 `0012/0013/0015/0016` 的产物：
//!    `characters.pet_mode`、`characters.quest_inventory`、`characters.last_daily_reset_at`、
//!    `characters.friends`、`guilds.allies`、`conquest_objects`；
//! 3. 校验 **0014 空洞**（没有 0014 文件、`_sqlx_migrations` 里没有 version 14）、
//!    **0013 头注释偏差**（文件名 13、头注释写 0014 —— 改名遗留，按文件名核对并登记，不改文件）、
//!    `_sqlx_migrations` 的 version 与 **checksum** 和 `MIGRATOR` 一致；
//! 4. **二次幂等**：再跑一次 `migrate()`，行数与版本集合不变；
//! 5. **真实失败注入**：在同一事务里改 schema 再制造 SQL 错误 → 断言 schema 与
//!    `_sqlx_migrations` 一起回滚（并打印可复核的恢复步骤）；
//! 6. 对 **`mir2` 开发库**做**只读**前后快照（每表行数 + 数据目录大小）并比对相等，
//!    同时声明「并发写入会使该证据失效」；
//! 7. 结束时 `DROP DATABASE` 掉演练库，并确认没有 `mir2_drill_%` 残留。
//!
//! 没有设置 `MIR2_DRILL_DATABASE_URL` 时**跳过**（打印提示），所以普通 `cargo test` 不受影响；
//! 但脚本会在调用本测试之前就用 `exit 2` 拒绝缺失/非法配置。
//!
//! 输出约定：所有给脚本抓取的证据行都以 `[DRILL]` 开头。

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

/// 迁移文件的目录（编译期定位，避免依赖测试的工作目录）。
const MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");

/// `0013` 的已知偏差：文件名版本是 13，头注释却写着 `0014_character_daily_reset.sql`。
///
/// **不许改文件**（会改 checksum、破坏已部署库的 `_sqlx_migrations` 一致性），
/// 所以演练按文件名核对版本、把偏差作为证据打印出来。
const KNOWN_0013_HEADER: &str = "0014_character_daily_reset.sql";

/// 0012/0013/0015/0016 应该产出的 (表, 列)。
const EXPECTED_COLUMNS: &[(&str, &str)] = &[
    ("characters", "pet_mode"),
    ("characters", "quest_inventory"),
    ("characters", "last_daily_reset_at"),
    ("characters", "friends"),
    ("guilds", "allies"),
];

/// 解析出 `postgres://user:pass@host:port/db` 里我们关心的几段。
#[derive(Debug, Clone, PartialEq, Eq)]
struct DrillUrl {
    host: String,
    database: String,
}

fn parse_pg_url(url: &str) -> Result<DrillUrl, String> {
    let rest = url
        .strip_prefix("postgres://")
        .or_else(|| url.strip_prefix("postgresql://"))
        .ok_or_else(|| format!("只支持 postgres:// 或 postgresql:// 开头的连接串，收到：{url}"))?;
    // 去掉 `?params`
    let rest = rest.split('?').next().unwrap_or(rest);
    let (authority, database) = rest
        .rsplit_once('/')
        .ok_or_else(|| format!("连接串缺少库名：{url}"))?;
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host_port)| host_port);
    let host = if host_port.starts_with('[') {
        // IPv6 字面量 `[::1]:5432`
        host_port
            .split_once(']')
            .map_or_else(|| host_port.to_owned(), |(inner, _)| format!("{inner}]"))
    } else {
        host_port.split(':').next().unwrap_or(host_port).to_owned()
    };
    Ok(DrillUrl {
        host,
        database: database.to_owned(),
    })
}

/// host 是否「本地」：环回地址，或用户在 `MIR2_DRILL_ALLOWED_HOSTS` 里显式放行的主机
/// （团队开发库在 `nas` 上，属于受控内网；放行必须显式配置，绝不隐式接受）。
fn host_is_allowed(host: &str) -> bool {
    host_is_allowed_with(
        host,
        &std::env::var("MIR2_DRILL_ALLOWED_HOSTS").unwrap_or_default(),
    )
}

/// 纯函数版本：环回地址直接放行，其余必须出现在逗号分隔的白名单里。
fn host_is_allowed_with(host: &str, allowlist: &str) -> bool {
    let host_lower = host.to_ascii_lowercase();
    let loopback = matches!(
        host_lower.as_str(),
        "localhost" | "127.0.0.1" | "::1" | "[::1]"
    );
    if loopback {
        return true;
    }
    allowlist
        .split(',')
        .map(|entry| entry.trim().to_ascii_lowercase())
        .any(|entry| !entry.is_empty() && entry == host_lower)
}

/// 演练库名：`mir2_drill_<pid>_<毫秒时间戳>`（并发安全，天然唯一）。
fn unique_drill_database() -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    format!("mir2_drill_{}_{ts}", std::process::id())
}

/// 从仓库根 `.env` 里读某个键（本机 PG 在 `nas:3001`，常写在 `.env` 而不是环境变量里）。
///
/// 只用于**只读快照**的连接串兜底；演练目标库仍然只认 `MIR2_DRILL_DATABASE_URL`。
fn read_dotenv(key: &str) -> Option<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.env");
    let text = std::fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        if name.trim() == key {
            let value = value.trim().trim_matches('"').trim_matches('\'').to_owned();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// `.env` 里的 `DATABASE_URL`。
fn env_from_dotenv() -> Option<String> {
    read_dotenv("DATABASE_URL")
}

/// `.env` 里的 `MIR2_DEV_DATABASE_URL`（开发库）。
fn dev_env_from_dotenv() -> Option<String> {
    read_dotenv("MIR2_DEV_DATABASE_URL")
}

/// 把 `postgres://.../dbname` 换库名（保留账号/主机/端口）。
fn with_database(url: &str, database: &str) -> String {
    let (head, tail) = url.rsplit_once('/').expect("连接串里至少有 host/db 分隔符");
    let _ = tail;
    format!("{head}/{database}")
}

/// 只读快照：每张用户表行数 + 当前库大小（字节）。
///
/// **只读**：整个快照在 `READ ONLY` 事务里跑，不写任何东西。
async fn snapshot(pool: &PgPool) -> Result<(Vec<(String, i64)>, i64), String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("快照事务开启失败：{error}"))?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("设置只读事务失败：{error}"))?;
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_schema = 'public' AND table_type = 'BASE TABLE' \
         ORDER BY table_name",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| format!("列出表失败：{error}"))?;
    let mut counts = Vec::with_capacity(tables.len());
    for table in tables {
        // 表名来自 information_schema，用引号包起来防止大小写/保留字问题。
        let sql = format!("SELECT count(*)::bigint FROM \"{table}\"");
        let count: i64 = sqlx::query_scalar(&sql)
            .fetch_one(&mut *tx)
            .await
            .map_err(|error| format!("统计 {table} 行数失败：{error}"))?;
        counts.push((table, count));
    }
    let size: i64 = sqlx::query_scalar("SELECT pg_database_size(current_database())::bigint")
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| format!("查询库大小失败：{error}"))?;
    tx.commit()
        .await
        .map_err(|error| format!("快照事务提交失败：{error}"))?;
    Ok((counts, size))
}

/// 建库 / 删库（用 `TEMPLATE template0`，避免继承模板库里的连接与会话）。
async fn create_database(admin: &PgPool, name: &str) -> Result<(), String> {
    let sql = format!("CREATE DATABASE \"{name}\" TEMPLATE template0");
    sqlx::query(&sql)
        .execute(admin)
        .await
        .map(|_| ())
        .map_err(|error| format!("创建演练库 {name} 失败：{error}"))
}

async fn drop_database(admin: &PgPool, name: &str) -> Result<(), String> {
    // `WITH (FORCE)` 会踢掉残留连接；PG 13+ 支持。失败再退回普通 DROP。
    let force = format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)");
    if sqlx::query(&force).execute(admin).await.is_ok() {
        return Ok(());
    }
    let plain = format!("DROP DATABASE IF EXISTS \"{name}\"");
    sqlx::query(&plain)
        .execute(admin)
        .await
        .map(|_| ())
        .map_err(|error| format!("删除演练库 {name} 失败：{error}"))
}

/// `_sqlx_migrations` 快照：`(version, success, checksum)`。
async fn applied_migrations(pool: &PgPool) -> Result<Vec<(i64, bool, Vec<u8>)>, String> {
    let rows =
        sqlx::query("SELECT version, success, checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(pool)
            .await
            .map_err(|error| format!("读取 _sqlx_migrations 失败：{error}"))?;
    Ok(rows
        .iter()
        .map(|row| {
            (
                row.get::<i64, _>("version"),
                row.get::<bool, _>("success"),
                row.get::<Vec<u8>, _>("checksum"),
            )
        })
        .collect())
}

/// 演练本体；返回给脚本的证据行。
async fn drill_body(
    drill_url: &str,
    drill_db: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut evidence = Vec::new();
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&with_database(drill_url, drill_db))
        .await?;

    // ---- 0) 连接后断言 current_database() 前缀 ----
    let current: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&admin)
        .await?;
    assert!(
        current.starts_with("mir2_drill"),
        "current_database() 必须以 mir2_drill 开头，实际：{current}"
    );
    evidence.push(format!("[DRILL] base_database={current}"));

    // ---- 1) mir2 开发库只读前后快照 ----
    // 连接串优先级：`DATABASE_URL` → 仓库根 `.env` 里的 `DATABASE_URL` / `MIR2_DEV_DATABASE_URL`。
    // 注意：这里只是**只读快照**用的连接，**不参与**「演练目标库只认 MIR2_DRILL_DATABASE_URL」这条约束。
    let mir2_url = std::env::var("DATABASE_URL")
        .ok()
        .or_else(env_from_dotenv)
        .or_else(dev_env_from_dotenv);
    let mut before = None;
    if let Some(url) = &mir2_url {
        let pool = mir_store::connect(url).await?;
        let snap = snapshot(&pool).await?;
        evidence.push(format!(
            "[DRILL] mir2_before_at={} mir2_rows={} mir2_size={}",
            iso_now(),
            snap.0.len(),
            snap.1
        ));
        before = Some(snap);
        pool.close().await;
    } else {
        evidence.push(
            "[DRILL] mir2_snapshot=skipped（未设置 DATABASE_URL，且仓库根 .env 里也没有）"
                .to_owned(),
        );
    }

    // ---- 2) 建唯一演练库并跑迁移 ----
    let drill_name = unique_drill_database();
    create_database(&admin, &drill_name).await?;
    evidence.push(format!("[DRILL] created_database={drill_name}"));
    let drill_pool = mir_store::connect(&with_database(drill_url, &drill_name)).await?;
    mir_store::migrate(&drill_pool).await?;
    evidence.push("[DRILL] migrate=ok".to_owned());

    let applied = applied_migrations(&drill_pool).await?;
    let expected: Vec<i64> = mir_store::MIGRATOR.iter().map(|m| m.version).collect();
    let applied_versions: Vec<i64> = applied.iter().map(|(version, ..)| *version).collect();
    assert_eq!(
        applied_versions, expected,
        "已应用版本应与 MIGRATOR 一致（含 0014 空洞）"
    );
    assert!(
        applied.iter().all(|(_, success, _)| *success),
        "所有迁移都应 success = true"
    );
    assert!(
        !applied_versions.contains(&14),
        "0014 是空洞，_sqlx_migrations 里不该出现 version 14"
    );
    evidence.push(format!(
        "[DRILL] applied_versions={applied_versions:?} hole_0014=absent"
    ));

    // checksum 一致性：逐条与 MIGRATOR 比对
    for migration in mir_store::MIGRATOR.iter() {
        let row = applied
            .iter()
            .find(|(version, ..)| *version == migration.version)
            .unwrap_or_else(|| panic!("version {} 未落库", migration.version));
        assert_eq!(
            row.2,
            migration.checksum.to_vec(),
            "version {} 的 checksum 与 MIGRATOR 不一致",
            migration.version
        );
    }
    evidence.push("[DRILL] checksums=match".to_owned());

    // ---- 3) 0013 头注释偏差（按文件名核对，不改文件）----
    let mig_dir = Path::new(MIGRATIONS_DIR);
    let mut versions_on_disk = Vec::new();
    for entry in std::fs::read_dir(mig_dir)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Some(prefix) = name.split('_').next() {
            if let Ok(version) = prefix.parse::<i64>() {
                versions_on_disk.push(version);
            }
        }
    }
    versions_on_disk.sort_unstable();
    assert!(
        !versions_on_disk.contains(&14),
        "迁移目录里不该存在 0014（空洞必须保留）"
    );
    assert_eq!(
        versions_on_disk, expected,
        "磁盘上的迁移版本应与 MIGRATOR 一致"
    );

    let header_0013 = std::fs::read_to_string(mig_dir.join("0013_character_daily_reset.sql"))?;
    let first_line = header_0013.lines().next().unwrap_or_default();
    assert!(
        first_line.contains(KNOWN_0013_HEADER),
        "0013 的头注释偏差是本轮登记过的已知现象，实际首行：{first_line}"
    );
    evidence.push(format!(
        "[DRILL] deviation_0013=filename_version=13 header_mentions={KNOWN_0013_HEADER} (registered, file untouched)"
    ));

    // ---- 4) 逐表校验 0012/0013/0015/0016 的产物 ----
    for (table, column) in EXPECTED_COLUMNS {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
             WHERE table_schema = 'public' AND table_name = $1 AND column_name = $2)",
        )
        .bind(table)
        .bind(column)
        .fetch_one(&drill_pool)
        .await?;
        assert!(exists, "迁移后 {table}.{column} 应存在");
        evidence.push(format!("[DRILL] column_ok={table}.{column}"));
    }
    let conquest_columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = 'conquest_objects' ORDER BY ordinal_position",
    )
    .fetch_all(&drill_pool)
    .await?;
    assert!(
        !conquest_columns.is_empty(),
        "0016 应创建 conquest_objects 表"
    );
    for expected_column in ["conquest_index", "kind", "hp", "destroyed"] {
        assert!(
            conquest_columns.iter().any(|name| name == expected_column),
            "conquest_objects 缺少列 {expected_column}，实际：{conquest_columns:?}"
        );
    }
    evidence.push(format!(
        "[DRILL] table_ok=conquest_objects columns={conquest_columns:?}"
    ));

    // ---- 5) 二次幂等 ----
    let before_idempotent = applied_migrations(&drill_pool).await?;
    mir_store::migrate(&drill_pool).await?;
    let after_idempotent = applied_migrations(&drill_pool).await?;
    assert_eq!(
        before_idempotent, after_idempotent,
        "重复执行 migrate() 不应新增任何迁移行"
    );
    evidence.push("[DRILL] idempotent_second_run=ok".to_owned());

    // ---- 6) 真实失败注入：事务内改 schema + 制造错误 → 全回滚 ----
    let before_fail = applied_migrations(&drill_pool).await?;
    let mut tx = drill_pool.begin().await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN drill_probe TEXT")
        .execute(&mut *tx)
        .await?;
    sqlx::query("CREATE TABLE drill_probe_table (id INTEGER)")
        .execute(&mut *tx)
        .await?;
    // 真实的 SQL 错误：除零。事务会进入 aborted 状态，随后的 ROLLBACK 撤销前面两条 DDL。
    let injected = sqlx::query("SELECT 1 / 0").execute(&mut *tx).await;
    assert!(injected.is_err(), "注入的 SQL 必须真的失败");
    tx.rollback().await?;

    let probe_column: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = 'characters' AND column_name = 'drill_probe')",
    )
    .fetch_one(&drill_pool)
    .await?;
    let probe_table: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables \
         WHERE table_schema = 'public' AND table_name = 'drill_probe_table')",
    )
    .fetch_one(&drill_pool)
    .await?;
    assert!(!probe_column, "失败注入后 columns 改动必须回滚");
    assert!(!probe_table, "失败注入后建表必须回滚");
    let after_fail = applied_migrations(&drill_pool).await?;
    assert_eq!(
        before_fail, after_fail,
        "失败注入后 _sqlx_migrations 不应有残留"
    );
    evidence.push(
        "[DRILL] failure_injection=rolled_back schema_and_migrations_clean recover=重新执行 tools/dev/migrate_drill.ps1（会建新库重跑；真实升级失败时先 `SELECT version, success, checksum FROM _sqlx_migrations` 核对，再按失败版本手工修复并重跑 migrate）"
            .to_owned(),
    );

    // ---- 7) 演练库清理 + mir2 前后快照比对 ----
    drill_pool.close().await;
    drop_database(&admin, &drill_name).await?;
    let leftovers: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM pg_database WHERE datname LIKE 'mir2_drill_%'",
    )
    .fetch_one(&admin)
    .await?;
    assert_eq!(leftovers, 0, "演练结束后不应残留 mir2_drill_* 库");
    evidence.push(format!("[DRILL] dropped_database={drill_name} leftovers=0"));

    if let (Some(url), Some(before)) = (&mir2_url, before) {
        let pool = mir_store::connect(url).await?;
        let after = snapshot(&pool).await?;
        pool.close().await;
        assert_eq!(
            before.0, after.0,
            "mir2 前后行数应一致（快照只读；并发写入会使该证据失效）"
        );
        assert_eq!(
            before.1, after.1,
            "mir2 前后库大小应一致（并发写入会使该证据失效）"
        );
        evidence.push(format!(
            "[DRILL] mir2_after_at={} mir2_rows={} mir2_size={} unchanged=true（声明：并发写入会使该证据失效）",
            iso_now(),
            after.0.len(),
            after.1
        ));
    }

    admin.close().await;
    Ok(evidence)
}

/// 粗粒度的 UTC 时间戳（ISO-8601 秒级），用于证据里的「前后时间」。
fn iso_now() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (hours, minutes, seconds) = (rem / 3_600, (rem % 3_600) / 60, rem % 60);
    // 1970-01-01 起的天数 → 年月日（civil_from_days，Howard Hinnant 算法）
    let z = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

#[tokio::test]
async fn migration_drill_from_0013_to_0016() {
    let Ok(drill_url) = std::env::var("MIR2_DRILL_DATABASE_URL") else {
        println!("跳过迁移演练：未设置 MIR2_DRILL_DATABASE_URL");
        return;
    };
    // 禁止回退：本测试**只认** MIR2_DRILL_DATABASE_URL。
    let parsed = parse_pg_url(&drill_url).expect("MIR2_DRILL_DATABASE_URL 应可解析");
    assert!(
        host_is_allowed(&parsed.host),
        "MIR2_DRILL_DATABASE_URL 的 host 必须是本地（或显式列在 MIR2_DRILL_ALLOWED_HOSTS 里），实际：{}",
        parsed.host
    );
    assert!(
        parsed.database.starts_with("mir2_drill"),
        "MIR2_DRILL_DATABASE_URL 的库名必须以 mir2_drill 开头，实际：{}",
        parsed.database
    );

    let evidence = drill_body(&drill_url, &parsed.database)
        .await
        .expect("迁移演练应当成功");
    for line in evidence {
        println!("{line}");
    }
    println!("[DRILL] result=ok");
}

/// 基库引导：`MIR2_DRILL_DATABASE_URL` 指向的库若不存在，就在同一台服务器上建出来。
///
/// 演练**每次**都会另建唯一的 `mir2_drill_<pid>_<ts>`，基库只用作「管理连接」的落脚点，
/// 所以这里只保证它存在（`TEMPLATE template0`），不做任何数据操作。
#[tokio::test]
async fn drill_bootstrap_base_database() {
    let Ok(drill_url) = std::env::var("MIR2_DRILL_DATABASE_URL") else {
        println!("跳过引导：未设置 MIR2_DRILL_DATABASE_URL");
        return;
    };
    let parsed = parse_pg_url(&drill_url).expect("MIR2_DRILL_DATABASE_URL 应可解析");
    assert!(
        parsed.database.starts_with("mir2_drill"),
        "基库名必须以 mir2_drill 开头，实际：{}",
        parsed.database
    );
    // 先试直连；连不上说明基库不存在（或不可用）→ 用 `postgres` 库建它。
    if mir_store::connect(&drill_url).await.is_ok() {
        println!("[DRILL] bootstrap=exists database={}", parsed.database);
        return;
    }
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&with_database(&drill_url, "postgres"))
        .await
        .expect("应能连接 postgres 库以创建演练基库");
    create_database(&admin, &parsed.database)
        .await
        .expect("创建演练基库应成功");
    admin.close().await;
    println!("[DRILL] bootstrap=created database={}", parsed.database);
}

/// 只做「连上去看一眼 `current_database()`」的探针：给脚本在跑演练之前做前置断言用。
///
/// 成功时打印 `[DRILL] probe_ok=<db>`；库名前缀不对直接 panic（脚本映射为 exit 2）。
#[tokio::test]
async fn drill_probe_current_database() {
    let Ok(drill_url) = std::env::var("MIR2_DRILL_DATABASE_URL") else {
        println!("跳过探针：未设置 MIR2_DRILL_DATABASE_URL");
        return;
    };
    let parsed = parse_pg_url(&drill_url).expect("MIR2_DRILL_DATABASE_URL 应可解析");
    assert!(
        host_is_allowed(&parsed.host),
        "host 必须是本地或显式白名单，实际：{}",
        parsed.host
    );
    let pool = mir_store::connect(&drill_url)
        .await
        .expect("应能连接 MIR2_DRILL_DATABASE_URL");
    let current: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("查询 current_database()");
    pool.close().await;
    assert!(
        current.starts_with("mir2_drill"),
        "current_database() 必须以 mir2_drill 开头，实际：{current}"
    );
    println!("[DRILL] probe_ok={current}");
}

#[test]
fn drill_url_parser_and_host_allowlist() {
    // 解析：账号密码、端口、IPv6、查询参数
    let parsed = parse_pg_url("postgres://user:pa:ss@127.0.0.1:5432/mir2_drill?sslmode=disable")
        .expect("应能解析");
    assert_eq!(parsed.host, "127.0.0.1");
    assert_eq!(parsed.database, "mir2_drill");

    let parsed = parse_pg_url("postgresql://postgres@nas:3001/mir2_drill").expect("应能解析");
    assert_eq!(parsed.host, "nas");
    assert_eq!(parsed.database, "mir2_drill");

    let parsed = parse_pg_url("postgres://u@[::1]:5432/mir2_drill").expect("应能解析 IPv6");
    assert_eq!(parsed.host, "[::1]");

    assert!(
        parse_pg_url("mysql://u@localhost/db").is_err(),
        "非 PG 连接串要拒绝"
    );
    assert!(
        parse_pg_url("postgres://u@localhost").is_err(),
        "缺库名要拒绝"
    );

    // 本地 host 放行；`nas` 只有在白名单里显式放行才允许（团队开发库在 nas 上）。
    assert!(host_is_allowed_with("localhost", ""));
    assert!(host_is_allowed_with("127.0.0.1", ""));
    assert!(host_is_allowed_with("::1", ""));
    assert!(!host_is_allowed_with("nas", ""));
    assert!(host_is_allowed_with("nas", "nas, other-host"));
    assert!(
        host_is_allowed_with("NAS", " nas ,other-host"),
        "大小写/空白不敏感"
    );
    assert!(!host_is_allowed_with("evil.example.com", "nas"));
}
