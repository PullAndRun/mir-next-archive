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

use std::collections::{BTreeMap, BTreeSet};
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
    let mut db =
        MirDb::from_bytes(&db_bytes).with_context(|| format!("解析 {}", db_path.display()))?;
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
    } else {
        eprintln!("警告：{} 不存在，只打包 DB 段", envir_dir.display());
    }

    // 数据清洗：无图标且没有任何引用的物品从 .mnd 剔除；无图标但被引用的保留并 WARN。
    let icon_root = PathBuf::from("game-assets/gfx/sprites/item");
    let clean_report = clean_items(&mut db, &scripts, &icon_root, &db_path);
    let mut report_json = clean_report.to_json(&icon_root);

    // M2：DB 段是 mir-db 结构的原生 bincode，不再装老 MirDB 字节。
    let native_db = encode_db(&db).context("编码原生 DB 段")?;
    let native_db_bytes = native_db.len();
    let mut file = MndFile::new();
    file.push(MndSection::new(TAG_DB_BIN, "db.bin", native_db));
    if !scripts.is_empty() {
        let bundle = encode_envir(&scripts).context("编码 Envir bundle")?;
        file.push(MndSection::new(TAG_ENVIR_BUNDLE, "envir.bin", bundle));
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
    report_json["mnd_sha256"] = json!(sha256);
    write_json_file(
        &PathBuf::from("target/data-clean-report.json"),
        &report_json,
    )
    .context("写出 target/data-clean-report.json")?;
    write_json_file(&out.join("clean-report.json"), &report_json)
        .with_context(|| format!("写出 {}/clean-report.json", out.display()))?;
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

#[derive(Debug, Default)]
struct CleanReport {
    before: usize,
    after: usize,
    kept: Vec<serde_json::Value>,
    pruned: Vec<serde_json::Value>,
    warn: Vec<serde_json::Value>,
}

impl CleanReport {
    fn to_json(&self, icon_root: &Path) -> serde_json::Value {
        json!({
            "schema": 1,
            "icon_root": icon_root.display().to_string(),
            "counts": {
                "items_before": self.before,
                "items_after": self.after,
                "kept": self.kept.len(),
                "pruned": self.pruned.len(),
                "warn": self.warn.len(),
            },
            "kept": self.kept,
            "pruned": self.pruned,
            "warn": self.warn,
        })
    }
}

/// 无图标且无引用的物品从 DB 中删除；无图标但被引用的保留并记录 WARN。
///
/// 引用集合覆盖：`StartItem`、`GameShop`、`Drops/**`（含 `#INSERT` 后的实际文件）、
/// NPC 脚本（`[Trade]` 等）、`Goods/*.msd`、`Quests/**`、`GM-*.txt`。
/// 这里按「文件全文包含物品名」做保守匹配：宁可多保留，不误删。
fn clean_items(
    db: &mut MirDb,
    scripts: &[(String, Vec<u8>)],
    icon_root: &Path,
    db_path: &Path,
) -> CleanReport {
    let before = db.items.len();
    let mut candidates = Vec::new();
    for (slot, item) in db.items.iter().enumerate() {
        if icon_available(icon_root, item.image) {
            continue;
        }
        candidates.push((
            slot,
            item.name.clone(),
            normalize_item_name(&item.name),
            item.image,
            format!("{:?}", item.item_type),
        ));
    }

    let mut references: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    for (slot, item) in db.items.iter().enumerate() {
        if !candidates.iter().any(|(candidate, ..)| *candidate == slot) {
            continue;
        }
        if item.start_item {
            references
                .entry(slot)
                .or_default()
                .insert("StartItem=true".to_owned());
        }
    }
    for shop in &db.gameshop {
        let slot = usize::try_from(shop.item_index).ok();
        if let Some(slot) = slot {
            if candidates.iter().any(|(candidate, ..)| *candidate == slot) {
                references
                    .entry(slot)
                    .or_default()
                    .insert(format!("GameShop#{}", shop.g_index));
            }
        }
    }

    // 文件全文匹配：Drops / NPC / Quests / Goods / GM 列表。
    // `.msd` 等可能是二进制，按 lossy UTF-8 扫描，宁可多保留。
    let normalized_texts: Vec<(&str, String)> = scripts
        .iter()
        .map(|(path, bytes)| {
            (
                path.as_str(),
                normalize_item_name(&String::from_utf8_lossy(bytes)),
            )
        })
        .collect();
    for (path, text) in &normalized_texts {
        let Some(kind) = reference_kind(path) else {
            continue;
        };
        for (slot, _, normalized_name, _, _) in &candidates {
            if normalized_name.is_empty() || !text.contains(normalized_name.as_str()) {
                continue;
            }
            references
                .entry(*slot)
                .or_default()
                .insert(format!("{kind}:{path}"));
        }
    }

    let mut keep = vec![true; db.items.len()];
    let mut pruned = Vec::new();
    let mut warn = Vec::new();
    for (slot, name, _, image, item_type) in &candidates {
        let sources = references.get(slot).cloned().unwrap_or_default();
        if sources.is_empty() {
            keep[*slot] = false;
            pruned.push(json!({
                "index": slot + 1,
                "slot": slot,
                "name": name,
                "image": image,
                "item_type": item_type,
                "reason": "missing_icon_unreferenced",
            }));
        } else {
            warn.push(json!({
                "index": slot + 1,
                "slot": slot,
                "name": name,
                "image": image,
                "item_type": item_type,
                "reason": "missing_icon_referenced",
                "references": sources.iter().collect::<Vec<_>>(),
            }));
        }
    }
    db.items = db
        .items
        .iter()
        .enumerate()
        .filter(|(slot, _)| keep[*slot])
        .map(|(_, item)| item.clone())
        .collect();
    let kept = db
        .items
        .iter()
        .enumerate()
        .map(|(slot, item)| {
            json!({
                "index": slot + 1,
                "name": item.name,
                "image": item.image,
                "item_type": format!("{:?}", item.item_type),
            })
        })
        .collect::<Vec<_>>();

    for item in &warn {
        eprintln!(
            "WARN 无图标但被引用，保留：{}",
            serde_json::to_string(item).unwrap_or_default()
        );
    }
    for item in &pruned {
        eprintln!(
            "PRUNED 无图标且无引用，剔除：{}",
            serde_json::to_string(item).unwrap_or_default()
        );
    }
    println!(
        "数据清洗（{}）：items {} -> {}，pruned {}，warn {}",
        db_path.display(),
        before,
        db.items.len(),
        pruned.len(),
        warn.len()
    );
    CleanReport {
        before,
        after: db.items.len(),
        kept,
        pruned,
        warn,
    }
}

/// 图标目录存在非空 PNG，且 `index.json` 没有把所有帧标成 `empty`。
fn icon_available(root: &Path, image: u16) -> bool {
    let dir = root.join(format!("icon-{image:04}"));
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return false;
    };
    let has_png = entries.flatten().any(|entry| {
        entry
            .path()
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    });
    if !has_png {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(dir.join("index.json")) else {
        return true;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return true;
    };
    let Some(images) = value.get("images").and_then(serde_json::Value::as_array) else {
        return true;
    };
    if images.is_empty() {
        return true;
    }
    !images
        .iter()
        .all(|image| image.get("empty").and_then(serde_json::Value::as_bool) == Some(true))
}

fn reference_kind(path: &str) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    if path
        .rsplit('/')
        .next()
        .is_some_and(|name| name.to_ascii_uppercase().starts_with("GM-"))
    {
        Some("GM")
    } else if lower.starts_with("drops/") {
        Some("Drops")
    } else if lower.starts_with("npcs/") {
        Some("NPC")
    } else if lower.starts_with("quests/") {
        Some("Quest")
    } else if lower.starts_with("goods/") {
        Some("Goods")
    } else {
        None
    }
}

fn normalize_item_name(name: &str) -> String {
    name.chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "mir-next-data-import-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("临时目录");
        root
    }

    #[test]
    fn icon_available_requires_png_and_not_all_empty() {
        let root = temp_root("icon");
        let icons = root.join("icons");
        assert!(!icon_available(&icons, 42), "目录不存在应判缺失");
        let dir = icons.join("icon-0042");
        std::fs::create_dir_all(&dir).expect("图标目录");
        std::fs::write(dir.join("index.json"), r#"{"images":[]}"#).expect("index");
        assert!(!icon_available(&icons, 42), "无 PNG 应判缺失");
        std::fs::write(dir.join("0042.png"), b"png").expect("png");
        assert!(icon_available(&icons, 42), "有 PNG 且 index 空应可用");
        std::fs::write(dir.join("index.json"), r#"{"images":[{"empty":true}]}"#).expect("empty");
        assert!(!icon_available(&icons, 42), "所有帧 empty=true 应判缺失");
        std::fs::write(dir.join("index.json"), r#"{"images":[{"empty":false}]}"#)
            .expect("non-empty");
        assert!(icon_available(&icons, 42), "非空帧应可用");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reference_kind_covers_required_sources() {
        assert_eq!(reference_kind("Drops/Province/Hen.txt"), Some("Drops"));
        assert_eq!(reference_kind("NPCs/GM/GM-Ring.txt"), Some("GM"));
        assert_eq!(reference_kind("Quests/1.txt"), Some("Quest"));
        assert_eq!(reference_kind("Goods/Goods.msd"), Some("Goods"));
        assert_eq!(reference_kind("GM-Ring.txt"), Some("GM"));
        assert_eq!(reference_kind("Map Info/0.map"), None);
    }

    #[test]
    fn name_normalization_ignores_case_and_whitespace() {
        assert_eq!(normalize_item_name(" Hardened Ring "), "hardenedring");
        assert_eq!(normalize_item_name("古铜戒指"), "古铜戒指");
    }
}
