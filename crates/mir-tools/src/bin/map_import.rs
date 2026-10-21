//! `map_import`：老 `.map` → 自有 `.mnm` 转换器（独立 bin，不改 `main.rs`）。
//!
//! ```text
//! cargo run -p mir-tools --bin map_import -- --input <dir|glob> --out game-assets/maps --format mnm [--dry-run] [--verify]
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use mir_world::map::Map;

#[allow(clippy::cast_precision_loss)]
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut input = None;
    let mut out = None;
    let mut format = "mnm".to_owned();
    let mut dry_run = false;
    let mut verify = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--input" => {
                input = args.get(i + 1).cloned();
                i += 2;
            }
            "--out" => {
                out = args.get(i + 1).cloned();
                i += 2;
            }
            "--format" => {
                format = args.get(i + 1).cloned().unwrap_or_default();
                i += 2;
            }
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            "--verify" => {
                verify = true;
                i += 1;
            }
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            other => bail!("unknown argument {other}; use --help"),
        }
    }
    if format != "mnm" {
        bail!("only --format mnm is supported");
    }
    let input = input.context("missing --input")?;
    let out = PathBuf::from(out.context("missing --out")?);
    let maps = discover(&input)?;
    if maps.is_empty() {
        bail!("no .map files under {input}");
    }
    if !dry_run {
        std::fs::create_dir_all(&out)?;
    }
    let mut total_src = 0_u64;
    let mut total_mnm = 0_u64;
    let mut converted = 0_usize;
    for source in maps {
        let map =
            Map::load_legacy(&source).with_context(|| format!("load {}", source.display()))?;
        let encoded =
            mir_world::mnm::encode(&map).with_context(|| format!("encode {}", source.display()))?;
        let decoded = mir_world::mnm::decode(&encoded)
            .with_context(|| format!("decode {}", source.display()))?;
        if verify {
            verify_equal(&map, &decoded).with_context(|| format!("verify {}", source.display()))?;
        }
        let src_len = std::fs::metadata(&source)?.len();
        total_src += src_len;
        total_mnm += u64::try_from(encoded.len()).unwrap_or(0);
        converted += 1;
        let target = out
            .join(source.file_stem().unwrap_or_default())
            .with_extension("mnm");
        println!(
            "{} -> {}  {} B -> {} B",
            source.display(),
            target.display(),
            src_len,
            encoded.len()
        );
        if !dry_run {
            std::fs::write(&target, &encoded)?;
        }
    }
    let ratio = if total_src == 0 {
        0.0
    } else {
        total_mnm as f64 / total_src as f64
    };
    println!(
        "converted={converted} src={total_src} mnm={total_mnm} ratio={ratio:.3} (dry_run={dry_run}, verify={verify})"
    );
    Ok(())
}

fn print_help() {
    println!("map_import --input <dir|glob> --out <dir> --format mnm [--dry-run] [--verify]");
}

fn discover(input: &str) -> Result<Vec<PathBuf>> {
    let path = Path::new(input);
    if path.is_dir() {
        let mut out = Vec::new();
        walk(path, &mut out)?;
        out.sort();
        return Ok(out);
    }
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    // 极简 glob：只支持 `dir/*.map` 或 `dir/*`，避免为工具引入额外依赖。
    if let Some((parent, pattern)) = input.rsplit_once(['/', '\\']) {
        let dir = Path::new(parent);
        if dir.is_dir() {
            let mut out = Vec::new();
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if wildcard_match(pattern, &name) && name.to_ascii_lowercase().ends_with(".map") {
                    out.push(entry.path());
                }
            }
            out.sort();
            return Ok(out);
        }
    }
    bail!("input not found: {input} (use dir, file, or dir/*.map)")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("map"))
        {
            out.push(path);
        }
    }
    Ok(())
}

fn wildcard_match(pattern: &str, name: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix('*') {
        return name.ends_with(suffix);
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return name.starts_with(prefix);
    }
    pattern.eq_ignore_ascii_case(name)
}

fn verify_equal(source: &Map, decoded: &Map) -> Result<()> {
    if source.width != decoded.width || source.height != decoded.height {
        bail!(
            "dimensions {}x{} != {}x{}",
            source.width,
            source.height,
            decoded.width,
            decoded.height
        );
    }
    if source.file_name != decoded.file_name {
        bail!("file_name {} != {}", source.file_name, decoded.file_name);
    }
    if source.version != decoded.version {
        bail!("version {:?} != {:?}", source.version, decoded.version);
    }
    if source.cells.len() != decoded.cells.len() {
        bail!("cells {} != {}", source.cells.len(), decoded.cells.len());
    }
    for (index, (a, b)) in source.cells.iter().zip(&decoded.cells).enumerate() {
        if a != b {
            bail!("cell {index} differs: {a:?} != {b:?}");
        }
    }
    if source.doors != decoded.doors {
        bail!("door table differs");
    }
    Ok(())
}
