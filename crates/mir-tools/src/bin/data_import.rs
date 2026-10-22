//! `data_import`：把 Crystal 老数据（`Server.MirDB` + `Envir/*.txt`）打包成运行时自有的 `.mnd`。
//!
//! 用法：
//! ```text
//! cargo run -p mir-tools --bin data_import -- \
//!   --source resource/community/suprcode-crystal-database/cjlaaa \
//!   --out game-assets/data [--json] [--verify]
//! ```
//!
//! 产出：
//! * `<out>/game.mnd`：TLV 容器（DB 段 + Envir 文本段，逐段 zlib）；
//! * `<out>/manifest.json`：schema、来源、计数、sha256、段统计；
//! * `<out>/game.json`（`--json`）：调试索引（不含正文）。

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use mir_db::MirDb;
use mir_db::mnd::{
    MAGIC, MndFile, MndSection, SCHEMA_VERSION, TAG_CONFIG, TAG_DB_BIN, TAG_ENVIR, TAG_ENVIR_BUNDLE,
};
use mir_db::schema::{decode_db, decode_envir, encode_db, encode_envir};
use serde_json::json;
use sha2::{Digest, Sha256};

fn main() -> Result<()> {
    let started = Instant::now();
    let mut source = PathBuf::from("resource/community/suprcode-crystal-database/cjlaaa");
    let mut out = PathBuf::from("game-assets/data");
    let mut write_json = false;
    let mut verify = false;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--source" => {
                index += 1;
                source = PathBuf::from(args.get(index).context("--source 缺少路径")?);
            }
            "--out" => {
                index += 1;
                out = PathBuf::from(args.get(index).context("--out 缺少路径")?);
            }
            "--json" => write_json = true,
            "--verify" => verify = true,
            "--help" | "-h" => {
                println!(
                    "用法：data_import --source <数据集目录或 Server.MirDB> --out <目录> [--json] [--verify]"
                );
                return Ok(());
            }
            other => bail!("未知参数：{other}"),
        }
        index += 1;
    }

    let (db_path, envir_dir) = resolve_source(&source)?;
    let db_bytes =
        std::fs::read(&db_path).with_context(|| format!("读取 {}", db_path.display()))?;
    let db = MirDb::from_bytes(&db_bytes).with_context(|| format!("解析 {}", db_path.display()))?;
    // M2：DB 段是 mir-db 结构的原生 bincode，不再装老 MirDB 字节。
    let native_db = encode_db(&db).context("编码原生 DB 段")?;
    let native_db_bytes = native_db.len();
    let mut file = MndFile::new();
    file.push(MndSection::new(TAG_DB_BIN, "db.bin", native_db));
    let mut script_count = 0usize;
    let mut script_bytes = 0usize;
    let mut scripts: Vec<(String, Vec<u8>)> = Vec::new();
    if envir_dir.is_dir() {
        let mut files = Vec::new();
        collect_files(&envir_dir, &mut files)?;
        files.sort();
        for path in files {
            let relative = path
                .strip_prefix(&envir_dir)
                .expect("walk 只返回 envir 下的文件")
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = std::fs::read(&path).with_context(|| format!("读取 {}", path.display()))?;
            script_count += 1;
            script_bytes += bytes.len();
            scripts.push((relative, bytes));
        }
        if !scripts.is_empty() {
            let bundle = encode_envir(&scripts).context("编码 Envir bundle")?;
            file.push(MndSection::new(TAG_ENVIR_BUNDLE, "envir.bin", bundle));
        }
    } else {
        eprintln!("警告：{} 不存在，只打包 DB 段", envir_dir.display());
    }

    // Configs/*.ini：经验表、行会配置等运营参数；运行时优先从 .mnd 读。
    let configs_dir = source.join("Configs");
    let mut config_count = 0usize;
    let mut config_bytes = 0usize;
    if configs_dir.is_dir() {
        let mut files = Vec::new();
        collect_files(&configs_dir, &mut files)?;
        files.sort();
        for path in files {
            let relative = path
                .strip_prefix(&configs_dir)
                .expect("walk 只返回 Configs 下的文件")
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = std::fs::read(&path).with_context(|| format!("读取 {}", path.display()))?;
            config_count += 1;
            config_bytes += bytes.len();
            file.push(MndSection::new(
                TAG_CONFIG,
                format!("Configs/{relative}"),
                bytes,
            ));
        }
    }

    std::fs::create_dir_all(&out).with_context(|| format!("创建输出目录 {}", out.display()))?;
    let mnd_path = out.join("game.mnd");
    file.write_to(&mnd_path)
        .with_context(|| format!("写出 {}", mnd_path.display()))?;
    let mnd_bytes = std::fs::read(&mnd_path).context("回读 game.mnd")?;
    let sha256 = hex(&Sha256::digest(&mnd_bytes));
    let counts = json!({
        "maps": db.maps.len(),
        "items": db.items.len(),
        "monsters": db.monsters.len(),
        "npcs": db.npcs.len(),
        "quests": db.quests.len(),
        "magics": db.magics.len(),
        "gameshop": db.gameshop.len(),
        "conquests": db.conquests.len(),
        "gt_maps": db.gt_maps.as_ref().map_or(0, Vec::len),
    });
    let manifest = json!({
        "schema_version": SCHEMA_VERSION,
        "format_version": file.format_version,
        "magic": String::from_utf8_lossy(MAGIC),
        "source": {
            "db": db_path.display().to_string(),
            "envir": envir_dir.display().to_string(),
        },
        "created_unix": SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()),
        "counts": counts,
        "db_encoding": "bincode",
        "db_bytes": native_db_bytes,
        "envir_bundle": true,
        "sections": {
            "total": file.sections.len(),
            "db": 1,
            "envir_files": script_count,
            "envir_bytes": script_bytes,
            "configs": config_count,
            "config_bytes": config_bytes,
        },
        "size_bytes": mnd_bytes.len(),
        "sha256": sha256,
    });
    write_json_file(&out.join("manifest.json"), &manifest)?;

    if write_json {
        let sections: Vec<_> = file
            .sections
            .iter()
            .map(|section| {
                json!({
                    "tag": section.tag,
                    "name": section.name,
                    "raw_bytes": section.data.len(),
                    "sha256": hex(&Sha256::digest(&section.data)),
                })
            })
            .collect();
        let debug = json!({
            "schema_version": SCHEMA_VERSION,
            "counts": manifest["counts"],
            "sections": sections,
        });
        write_json_file(&out.join("game.json"), &debug)?;
    }

    if verify {
        verify_roundtrip(&mnd_path, &db, script_count)?;
    }

    println!(
        "已写出 {}（{} KiB，DB 1 段 + Envir bundle {} 份，sha256={}），耗时 {:?}",
        mnd_path.display(),
        mnd_bytes.len() / 1024,
        script_count,
        hex(&Sha256::digest(&mnd_bytes)),
        started.elapsed()
    );
    Ok(())
}

fn resolve_source(source: &Path) -> Result<(PathBuf, PathBuf)> {
    if source.is_file() {
        let envir = source.parent().context("数据库没有父目录")?.join("Envir");
        return Ok((source.to_path_buf(), envir));
    }
    let db = source.join("Server.MirDB");
    if !db.is_file() {
        bail!(
            "{} 下没有 Server.MirDB（--source 需要数据集目录或数据库文件）",
            source.display()
        );
    }
    Ok((db, source.join("Envir")))
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("列目录 {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn write_json_file(path: &Path, value: &serde_json::Value) -> Result<()> {
    let text = serde_json::to_string_pretty(value)?;
    std::fs::write(path, text).with_context(|| format!("写出 {}", path.display()))?;
    Ok(())
}

fn verify_roundtrip(path: &Path, expected: &MirDb, expected_scripts: usize) -> Result<()> {
    let back = MndFile::read_from(path).context("回读 .mnd")?;
    let (db, encoding) = if let Some(section) = back
        .sections
        .iter()
        .find(|section| section.tag == TAG_DB_BIN)
    {
        (
            decode_db(&section.data).context("解码原生 DB 段")?,
            "bincode",
        )
    } else {
        let section = back.get("db").context("缺少 db/db.bin 段")?;
        (
            MirDb::from_bytes(&section.data).context("解析 M1 DB 段")?,
            "legacy",
        )
    };
    let same = expected.header == db.header
        && expected.maps == db.maps
        && expected.items == db.items
        && expected.monsters == db.monsters
        && expected.npcs == db.npcs
        && expected.quests == db.quests
        && expected.dragon == db.dragon
        && expected.magics == db.magics
        && expected.gameshop == db.gameshop
        && expected.conquests == db.conquests
        && expected.respawn == db.respawn
        && expected.gt_maps == db.gt_maps;
    if !same {
        bail!("校验失败：{encoding} DB 与老库关键字段不一致");
    }
    let scripts = if let Some(section) = back
        .sections
        .iter()
        .find(|section| section.tag == TAG_ENVIR_BUNDLE)
    {
        decode_envir(&section.data)
            .context("解码 Envir bundle")?
            .len()
    } else {
        back.sections
            .iter()
            .filter(|section| section.tag == TAG_ENVIR)
            .count()
    };
    if scripts != expected_scripts {
        bail!("校验失败：Envir 段旧={expected_scripts} 新={scripts}");
    }
    let configs = back
        .sections
        .iter()
        .filter(|section| section.tag == TAG_CONFIG)
        .count();
    if configs == 0 {
        bail!("校验失败：.mnd 没有 Configs 段");
    }
    println!("校验通过：{encoding} DB 关键字段逐项一致，Envir {scripts} 份，Configs {configs} 份");
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}
