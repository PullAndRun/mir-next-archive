//! 《传奇-Next》工具链入口。
//!
//! 当前提供地图文件检查（`.map` 读取与统计）、协议表统计、`Server.MirDB` 数据库报告，
//! 以及图像库（`.Lib` / `.wzl`+`.wzx`）的查看与 PNG 导出；补丁工具的实现在后续里程碑补齐，
//! 见 `README.md` 的路线图 M7。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("protocol-report") => {
            protocol_report();
            Ok(())
        }
        Some("map-info") => {
            let path = args
                .next()
                .map_or_else(|| PathBuf::from("resource/map/0.map"), PathBuf::from);
            map_info(&path)
        }
        Some("mirdb-report") => mirdb_report(&mut args),
        Some("image-info") => image_info(&mut args),
        Some("image-dump") => image_dump(&mut args),
        Some("assets") => assets_command(&mut args),
        None | Some("help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        Some(other) => {
            anyhow::bail!("未知子命令 {other}，运行 `mir-tools help` 查看用法");
        }
    }
}

fn print_help() {
    println!("用法：mir-tools <子命令>");
    println!();
    println!("子命令：");
    println!("  protocol-report  列出协议包数量（客户端→服务端 / 服务端→客户端）");
    println!("  map-info [路径]   解析地图文件并统计可走比例（默认 resource/map/0.map）");
    println!(
        "  mirdb-report [--path 路径]  解析 Server.MirDB 并打印版本、各段条数、样本与 leftover"
    );
    println!("                              （默认 sample：cjlaaa/Server.MirDB）");
    println!("  image-info [--path 路径] [--limit N]  打印图像库数量与前 N 张图的头信息");
    println!("                              （默认 resource/data/anitiles1.wzl、N=10）");
    println!("  image-dump [--path 路径] [--index N] [--out 路径]  解码一张图并写成 PNG");
    println!(
        "  assets unpack --lib <库> --out <目录> [--indices 0-9,12|文件|all] [--dry-run] [--verify]"
    );
    println!(
        "  assets unpack --plan <used-assets.json> --out-root <目录>  批量拆包（吃引用分析结果）"
    );
    println!(
        "  assets pack --input <game-assets> --out <packs> [--chunk-mb 16] [--verify|--list]  发布期自有分包(.mnp)"
    );
    println!("                              （默认 index=0、build/out.png）");
}

fn protocol_report() {
    let client = mir_protocol::ClientPacketIds::ALL.len();
    let server = mir_protocol::ServerPacketIds::ALL.len();
    println!("Crystal 协议：客户端→服务端 {client} 个包，服务端→客户端 {server} 个包");
}

fn map_info(path: &Path) -> Result<()> {
    let map = mir_world::Map::load(path)?;
    let total = usize::try_from(i64::from(map.width) * i64::from(map.height)).unwrap_or(0);
    let walkable = map.walkable_cells().count();
    println!(
        "{}: {}x{}（{} 格），版本 {:?}，可走 {} / 不可走 {}，门 {} 个",
        path.display(),
        map.width,
        map.height,
        total,
        map.version,
        walkable,
        total - walkable,
        map.doors.len()
    );
    Ok(())
}

/// 默认样本：`docs/store/mirdb-format.md` 里逐字段验证过的 105 版数据库。
const DEFAULT_MIRDB: &str = "resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB";

/// `mir-tools mirdb-report [--path <文件>]`：解析 `Server.MirDB` 并打印稳定报告。
///
/// 输出由 `mir_db::render_report` 生成（逐行 `key=value`，见该函数文档），
/// 因此可以当测试锚点用：版本、各段条数与字节范围、头部计数器、每段三条样本、leftover。
fn mirdb_report(args: &mut impl Iterator<Item = String>) -> Result<()> {
    let mut path = PathBuf::from(DEFAULT_MIRDB);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--path" => {
                path = args
                    .next()
                    .map(PathBuf::from)
                    .with_context(|| "--path 后面需要一个 Server.MirDB 文件路径".to_owned())?;
            }
            other => anyhow::bail!(
                "mirdb-report 不认识参数 {other}，用法：mir-tools mirdb-report --path <Server.MirDB>"
            ),
        }
    }

    let (db, leftover) = mir_db::MirDb::load_with_leftover(&path)?;
    print!("{}", mir_db::render_report(&db, leftover));
    if leftover != 0 {
        // 非 0 的 leftover 说明文件与解析器的版本/布局对不上，是 bug 信号。
        eprintln!(
            "警告：{} 尾部还有 {leftover} 字节未被消费（正常应为 0；文件版本或布局可能与本解析器不符）",
            path.display()
        );
    }
    Ok(())
}

/// `image-info` / `image-dump` 的默认样本：Shanda 风格的 `.wzl` + `.wzx` 地图动画图库。
const DEFAULT_IMAGE_LIBRARY: &str = "resource/data/anitiles1.wzl";

/// `mir-tools image-info [--path <图像库>] [--limit <N>]`：打印数量与前 N 张图的头信息。
///
/// 输出逐行 `key=value`，顺序固定，方便当测试锚点。
fn image_info(args: &mut impl Iterator<Item = String>) -> Result<()> {
    let mut path = PathBuf::from(DEFAULT_IMAGE_LIBRARY);
    let mut limit = 10usize;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--path" => {
                path = args
                    .next()
                    .map(PathBuf::from)
                    .with_context(|| "--path 后面需要一个图像库路径".to_owned())?;
            }
            "--limit" => {
                let value = args
                    .next()
                    .with_context(|| "--limit 后面需要一个数字".to_owned())?;
                limit = value
                    .parse()
                    .with_context(|| format!("--limit 不是数字：{value}"))?;
            }
            other => anyhow::bail!(
                "image-info 不认识参数 {other}，用法：mir-tools image-info --path <图像库> [--limit N]"
            ),
        }
    }

    let library = mir_asset::ImageLibrary::open(&path)?;
    println!("path={}", path.display());
    println!("kind={}", library.kind().as_str());
    println!("count={}", library.len());
    match library.declared_count() {
        Some(count) => println!("declared_count={count}"),
        None => println!("declared_count=-"),
    }
    for index in 0..limit.min(library.len()) {
        match library.info(index) {
            Ok(info) => println!(
                "i={index} width={} height={} x={} y={} format={} name={} data_offset={} data_len={}",
                info.width,
                info.height,
                info.offset_x,
                info.offset_y,
                info.format,
                info.format_name(),
                info.data_offset,
                info.data_len
            ),
            Err(error) => println!("i={index} error={error}"),
        }
    }
    Ok(())
}

/// `mir-tools image-dump [--path <图像库>] [--index <N>] [--out <PNG>]`：解码一张图并写成 PNG。
///
/// 输出逐行 `key=value`；`out=` 是调用方给的路径（不展开成绝对路径），便于脚本比对。
fn image_dump(args: &mut impl Iterator<Item = String>) -> Result<()> {
    let mut path = PathBuf::from(DEFAULT_IMAGE_LIBRARY);
    let mut index = 0usize;
    let mut out = PathBuf::from("build/out.png");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--path" => {
                path = args
                    .next()
                    .map(PathBuf::from)
                    .with_context(|| "--path 后面需要一个图像库路径".to_owned())?;
            }
            "--index" => {
                let value = args
                    .next()
                    .with_context(|| "--index 后面需要一个数字".to_owned())?;
                index = value
                    .parse()
                    .with_context(|| format!("--index 不是数字：{value}"))?;
            }
            "--out" => {
                out = args
                    .next()
                    .map(PathBuf::from)
                    .with_context(|| "--out 后面需要一个 PNG 路径".to_owned())?;
            }
            other => anyhow::bail!(
                "image-dump 不认识参数 {other}，用法：mir-tools image-dump --path <图像库> --index N --out <PNG>"
            ),
        }
    }

    let library = mir_asset::ImageLibrary::open(&path)?;
    let image = library.image(index)?;
    if image.info.is_empty() {
        anyhow::bail!(
            "{} 第 {index} 张是空图（{}×{}），无法导出 PNG",
            path.display(),
            image.info.width,
            image.info.height
        );
    }
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建目录 {} 失败", parent.display()))?;
        }
    }
    write_png(&out, &image)?;

    let png_bytes = std::fs::metadata(&out).map_or(0, |metadata| metadata.len());
    println!("path={}", path.display());
    println!("index={index}");
    println!("width={}", image.info.width);
    println!("height={}", image.info.height);
    println!("format={}", image.info.format);
    println!("name={}", image.info.format_name());
    println!("rgba_bytes={}", image.pixels.len());
    println!("out={}", out.display());
    println!("png_bytes={png_bytes}");
    Ok(())
}

/// 把 RGBA8 缓冲写成 PNG（RGBA、8 位深，与 `mir-asset` 的冒烟测试一致）。
fn write_png(path: &Path, image: &mir_asset::Image) -> Result<()> {
    let file =
        std::fs::File::create(path).with_context(|| format!("创建 {} 失败", path.display()))?;
    let mut encoder = png::Encoder::new(
        file,
        u32::from(image.info.width),
        u32::from(image.info.height),
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().context("写 PNG 头失败")?;
    writer
        .write_image_data(&image.pixels)
        .context("写 PNG 像素失败")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// `assets unpack`：把图像库拆成 `index.json` + 每图一张 PNG（素材粒度最细）
// ---------------------------------------------------------------------------

/// 用法（报错与 `help` 里都用它）。
const ASSETS_UNPACK_USAGE: &str = "usage: mir-tools assets unpack --lib <lib> --out <dir> [--indices 0-9,12|all|<json>] [--format png] [--reason text] [--dry-run] [--verify]\n       mir-tools assets unpack --plan <used-assets.json> --out-root <dir> [--dry-run] [--verify]";

/// 要拆哪些图。
#[derive(Debug, Clone, PartialEq, Eq)]
enum IndexSpec {
    /// 整库。
    All,
    /// 指定索引（已排序去重）。
    List(Vec<usize>),
}

fn assets_command(args: &mut impl Iterator<Item = String>) -> Result<()> {
    match args.next().as_deref() {
        Some("unpack") => assets_unpack(args),
        Some("pack") => assets_pack(args),
        None => anyhow::bail!("assets 需要子命令：unpack\n{ASSETS_UNPACK_USAGE}"),
        Some(other) => {
            anyhow::bail!("assets 不认识子命令 {other}（只有 unpack）\n{ASSETS_UNPACK_USAGE}")
        }
    }
}

fn next_arg(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .with_context(|| format!("{flag} 后面需要一个值"))
}

fn assets_unpack(args: &mut impl Iterator<Item = String>) -> Result<()> {
    let mut lib: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut indices: Option<String> = None;
    let mut plan: Option<PathBuf> = None;
    let mut out_root: Option<PathBuf> = None;
    let mut reason: Option<String> = None;
    let mut format = String::from("png");
    let mut dry_run = false;
    let mut verify = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--lib" => lib = Some(PathBuf::from(next_arg(args, "--lib")?)),
            "--out" => out = Some(PathBuf::from(next_arg(args, "--out")?)),
            "--indices" => indices = Some(next_arg(args, "--indices")?),
            "--plan" => plan = Some(PathBuf::from(next_arg(args, "--plan")?)),
            "--out-root" => out_root = Some(PathBuf::from(next_arg(args, "--out-root")?)),
            "--reason" => reason = Some(next_arg(args, "--reason")?),
            "--format" => format = next_arg(args, "--format")?,
            "--dry-run" => dry_run = true,
            "--verify" => verify = true,
            other => anyhow::bail!("assets unpack 不认识参数 {other}\n{ASSETS_UNPACK_USAGE}"),
        }
    }
    if format != "png" {
        anyhow::bail!("--format 目前只支持 png（收到 {format}）");
    }

    if let Some(plan_path) = plan {
        let out_root = out_root.with_context(|| "--plan 需要配合 --out-root".to_owned())?;
        return unpackage_plan(&plan_path, &out_root, dry_run, verify);
    }

    let lib_path = lib.with_context(|| "--lib 必须给出图像库路径".to_owned())?;
    let out_dir = out.with_context(|| "--out 必须给出输出目录".to_owned())?;
    let index_spec = match indices.as_deref() {
        None => IndexSpec::All,
        Some(raw) => parse_index_spec(raw)?,
    };
    unpack_one(
        &lib_path,
        &out_dir,
        index_spec,
        reason.as_deref(),
        dry_run,
        verify,
    )?;
    Ok(())
}

/// 批量：`--plan used-assets.json --out-root <dir>`，每库一个 `<out-root>/<stem>/`。
fn unpackage_plan(plan_path: &Path, out_root: &Path, dry_run: bool, verify: bool) -> Result<()> {
    let text = std::fs::read_to_string(plan_path)
        .with_context(|| format!("读取计划文件 {} 失败", plan_path.display()))?;
    let plan: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("{} 不是合法 JSON", plan_path.display()))?;
    let source_root = plan
        .get("source_root")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("resource/data")
        .to_owned();
    let libraries = plan
        .get("libraries")
        .and_then(serde_json::Value::as_array)
        .with_context(|| format!("{} 缺少 libraries 数组", plan_path.display()))?;

    let mut total_images = 0usize;
    let mut total_libraries = 0usize;
    let mut total_outputs = 0usize;
    let mut written_dirs: Vec<PathBuf> = Vec::new();
    for entry in libraries {
        let lib = entry
            .get("lib")
            .and_then(serde_json::Value::as_str)
            .with_context(|| "libraries[] 每项必须有 lib 字段".to_owned())?;
        let spec = library_spec(entry)?;
        if matches!(&spec, IndexSpec::List(list) if list.is_empty()) {
            println!("skip lib={lib} reason=empty-index-list");
            continue;
        }
        let reason = entry.get("reason").and_then(serde_json::Value::as_str);
        let lib_path = resolve_plan_lib(Path::new(&source_root), lib);
        let stem = Path::new(lib).file_stem().map_or_else(
            || "library".to_owned(),
            |value| value.to_string_lossy().to_string(),
        );
        let default_dir = entry
            .get("dest")
            .and_then(serde_json::Value::as_str)
            .map_or_else(
                || {
                    entry
                        .get("category")
                        .and_then(serde_json::Value::as_str)
                        .map_or_else(
                            || out_root.join("gfx").join(&stem),
                            |category| out_root.join("gfx").join(category).join(&stem),
                        )
                },
                |dest| out_root.join(dest),
            );

        // v1.1：一个库可以拆到多个目录（例如 mon14.wzl 里 10 个实体各一个目录）。
        // `outputs[]` 每项可带自己的 `indices`/`ranges`（缺省回退到库里级 indices）与 `dest`。
        let mut tasks: Vec<(IndexSpec, PathBuf)> = Vec::new();
        if let Some(list) = entry.get("outputs").and_then(serde_json::Value::as_array) {
            for output in list {
                let output_spec = match output.get("indices").or_else(|| output.get("ranges")) {
                    Some(raw) => index_spec_from_value(raw)?,
                    None => spec.clone(),
                };
                let dest = output
                    .get("dest")
                    .and_then(serde_json::Value::as_str)
                    .with_context(|| format!("outputs[] 每项必须有 dest（lib={lib}）"))?;
                let entity = output.get("entity").and_then(serde_json::Value::as_str);
                let _ = entity; // 目前只用于落目录命名；对账信息进 index.json 的 reason
                tasks.push((output_spec, out_root.join(dest)));
            }
        } else {
            tasks.push((spec, default_dir));
        }

        for (output_spec, out_dir) in tasks {
            if matches!(&output_spec, IndexSpec::List(list) if list.is_empty()) {
                println!(
                    "skip lib={lib} dest={} reason=empty-index-list",
                    out_dir.display()
                );
                continue;
            }
            // 同一次 plan 里两个输出落到同一目录 → 后者会覆盖前者，明确告警而不是静默
            if written_dirs.contains(&out_dir) {
                println!(
                    "WARN duplicate dest={} （后者会覆盖前者的 index.json）",
                    out_dir.display()
                );
            }
            written_dirs.push(out_dir.clone());
            total_images += unpack_one(&lib_path, &out_dir, output_spec, reason, dry_run, verify)?;
            total_outputs += 1;
        }
        total_libraries += 1;
    }
    println!(
        "plan={} libraries={total_libraries} outputs={total_outputs} images={total_images} out_root={}",
        plan_path.display(),
        out_root.display()
    );
    Ok(())
}

/// 库里级 `indices`/`ranges`（`outputs[]` 没写时回退到它）。
fn library_spec(entry: &serde_json::Value) -> Result<IndexSpec> {
    if let Some(raw) = entry.get("indices").or_else(|| entry.get("ranges")) {
        index_spec_from_value(raw)
    } else {
        Ok(IndexSpec::All)
    }
}

fn resolve_plan_lib(source_root: &Path, lib: &str) -> PathBuf {
    let candidate = Path::new(lib);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        source_root.join(lib)
    }
}

/// 拆一个库：写 `index.json` + `<index>.png`；返回拆出的图数（含空图）。
fn unpack_one(
    lib_path: &Path,
    out_dir: &Path,
    spec: IndexSpec,
    reason: Option<&str>,
    dry_run: bool,
    verify: bool,
) -> Result<usize> {
    if verify {
        return verify_unpacked(lib_path, out_dir);
    }
    let library = mir_asset::ImageLibrary::open(lib_path)
        .with_context(|| format!("打开图像库 {} 失败", lib_path.display()))?;
    let total = library.len();
    let indices: Vec<usize> = match spec {
        IndexSpec::All => (0..total).collect(),
        IndexSpec::List(list) => list,
    };
    // 越界索引不 fail：引用分析可能按旧版布局给号，记进 `missing` 供对账（hubu2 要求）。
    let mut missing: Vec<serde_json::Value> = Vec::new();
    let mut out_of_range = 0usize;
    let mut usable: Vec<usize> = Vec::with_capacity(indices.len());
    for index in indices {
        if index >= total {
            missing.push(serde_json::json!({"index": index, "reason": "out-of-range"}));
            out_of_range += 1;
        } else {
            usable.push(index);
        }
    }
    let indices = usable;
    if dry_run {
        let mut pixels = 0u64;
        let mut loaded = 0usize;
        for index in &indices {
            let info = library.info(*index)?;
            if info.is_empty() {
                continue;
            }
            loaded += 1;
            pixels += u64::from(info.width) * u64::from(info.height) * 4;
        }
        println!(
            "dry-run lib={} dest={} requested={} loaded={loaded} missing={} rgba_bytes={pixels}",
            lib_path.display(),
            out_dir.display(),
            indices.len(),
            indices.len() - loaded
        );
        return Ok(indices.len());
    }

    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("创建目录 {} 失败", out_dir.display()))?;
    let source_bytes = std::fs::metadata(lib_path).map_or(0, |meta| meta.len());
    let mut images = Vec::with_capacity(indices.len());
    let mut png_hashes: Vec<(String, String, u64)> = Vec::new();
    let mut written = 0usize;
    let mut skipped_empty = 0usize;
    for index in &indices {
        let mut image = library.image(*index)?;
        // 拆包时就把 palette8 的纯黑烘成透明（与渲染层 remove_black 同规则，幂等）。
        mir_asset::black_to_transparent(image.info.format, &mut image.pixels);
        let info = image.info;
        let empty = info.is_empty();
        if empty {
            skipped_empty += 1;
            missing.push(serde_json::json!({"index": index, "reason": "empty-frame"}));
        }
        let png_name = format!("{index:04}.png");
        if !empty {
            let png_path = out_dir.join(&png_name);
            write_png(&png_path, &image)?;
            png_hashes.push((
                png_name.clone(),
                sha256_file(&png_path)?,
                std::fs::metadata(&png_path).map_or(0, |m| m.len()),
            ));
            written += 1;
        }
        let png_sha = png_hashes
            .iter()
            .find(|(name, _, _)| name == &png_name)
            .map(|(_, hash, _)| hash.clone());
        images.push(serde_json::json!({
            "index": index,
            "png_sha256": png_sha,
            "png": if empty { serde_json::Value::Null } else { serde_json::Value::String(png_name) },
            "width": info.width,
            "height": info.height,
            "offset_x": info.offset_x,
            "offset_y": info.offset_y,
            "shadow_x": info.shadow_x,
            "shadow_y": info.shadow_y,
            "has_mask": info.has_mask,
            "format": info.format_name(),
            "format_code": info.format,
            "reserved": info.reserved,
            "data_offset": info.data_offset,
            "data_len": info.data_len,
            "empty": empty,
            "bytes": image.pixels.len(),
            "rgb_sha256": sha256_rgb(&image.pixels),
            "rgba_sha256": sha256_hex(&image.pixels),
            "pixels_sha256": sha256_hex(&image.pixels),
        }));
    }

    let index_json = serde_json::json!({
        "version": 1,
        "lib": lib_path.file_name().map(|name| name.to_string_lossy().to_string()),
        "source": lib_path.display().to_string(),
        "source_bytes": source_bytes,
        "source_sha256": sha256_file(lib_path)?,
        "count": indices.len(),
        "written_png": written,
        "skipped_empty": skipped_empty,
        "indices_out_of_range": out_of_range,
        "missing": missing,
        "reason": reason,
        "format": "png",
        "alpha": "black-to-transparent",
        "alpha_applied_formats": [0, 3],
        "hash_notes": {
            "rgb_sha256": "每像素去掉 alpha 的 RGB 字节哈希（跨实现对账用）",
            "rgba_sha256": "最终 RGBA（含 palette8 去黑）",
            "pixels_sha256": "rgba_sha256 的别名（兼容旧消费方）",
        },
        "images": images,
    });
    let index_path = out_dir.join("index.json");
    std::fs::write(
        &index_path,
        serde_json::to_string_pretty(&index_json).context("序列化 index.json 失败")?,
    )
    .with_context(|| format!("写 {} 失败", index_path.display()))?;
    println!(
        "lib={} total={total} selected={} png={written} index={}",
        lib_path.display(),
        indices.len(),
        index_path.display()
    );
    Ok(indices.len())
}

/// 校验已有拆包产物与库一致：逐图比 `pixels_sha256` 与 PNG 像素。
fn verify_unpacked(lib_path: &Path, out_dir: &Path) -> Result<usize> {
    let index_path = out_dir.join("index.json");
    let text = std::fs::read_to_string(&index_path)
        .with_context(|| format!("读取 {} 失败（还没拆过包？）", index_path.display()))?;
    let index: serde_json::Value =
        serde_json::from_str(&text).context("index.json 不是合法 JSON")?;
    let library = mir_asset::ImageLibrary::open(lib_path)
        .with_context(|| format!("打开图像库 {} 失败", lib_path.display()))?;
    let images = index
        .get("images")
        .and_then(serde_json::Value::as_array)
        .context("index.json 缺少 images 数组")?;
    let mut bad = 0usize;
    for entry in images {
        let index_no = entry
            .get("index")
            .and_then(serde_json::Value::as_u64)
            .context("缺少 index")?;
        let index_no = usize::try_from(index_no).unwrap_or(usize::MAX);
        let mut image = library.image(index_no)?;
        // 与拆包时同样的烘焙，才能和 PNG/index.json 对上
        mir_asset::black_to_transparent(image.info.format, &mut image.pixels);
        // 优先比 rgb（解码定稿判据）；rgba 比不过说明透明策略变了，同样算不一致。
        if let Some(expected_rgb) = entry.get("rgb_sha256").and_then(serde_json::Value::as_str) {
            let actual_rgb = sha256_rgb(&image.pixels);
            if actual_rgb != expected_rgb {
                println!(
                    "MISMATCH rgb index={index_no} expected={expected_rgb} actual={actual_rgb}"
                );
                bad += 1;
                continue;
            }
        }
        let expected_rgba = entry
            .get("rgba_sha256")
            .or_else(|| entry.get("pixels_sha256"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let actual_rgba = sha256_hex(&image.pixels);
        if actual_rgba != expected_rgba {
            println!(
                "MISMATCH rgba index={index_no} expected={expected_rgba} actual={actual_rgba}"
            );
            bad += 1;
            continue;
        }
        if let Some(png_name) = entry.get("png").and_then(serde_json::Value::as_str) {
            let png_path = out_dir.join(png_name);
            // 文件级：先把 PNG 字节的 sha256 对上（防"尾部加字节"这类篡改）
            if let Some(expected_png) = entry.get("png_sha256").and_then(serde_json::Value::as_str)
            {
                let actual_png = sha256_file(&png_path)?;
                if actual_png != expected_png {
                    println!(
                        "MISMATCH png-bytes index={index_no} expected={expected_png} actual={actual_png}"
                    );
                    bad += 1;
                    continue;
                }
            }
            let (width, height, pixels) = read_png_rgba(&png_path)?;
            if usize::try_from(width).ok() != Some(usize::from(image.info.width))
                || usize::try_from(height).ok() != Some(usize::from(image.info.height))
                || pixels != image.pixels
            {
                println!("MISMATCH png index={index_no} file={png_name}");
                bad += 1;
            }
        }
    }
    if bad > 0 {
        anyhow::bail!("verify 失败：{bad} 张图与库不一致（{}）", out_dir.display());
    }
    println!(
        "verify ok lib={} images={} out={}",
        lib_path.display(),
        images.len(),
        out_dir.display()
    );
    Ok(images.len())
}

fn read_png_rgba(path: &Path) -> Result<(u32, u32, Vec<u8>)> {
    let file =
        std::fs::File::open(path).with_context(|| format!("打开 {} 失败", path.display()))?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .with_context(|| format!("读 PNG 头 {} 失败", path.display()))?;
    let size = reader
        .output_buffer_size()
        .context("PNG 输出缓冲大小未知（异常图像）")?;
    let mut buffer = vec![0u8; size];
    let info = reader
        .next_frame(&mut buffer)
        .with_context(|| format!("解码 PNG {} 失败", path.display()))?;
    buffer.truncate(info.buffer_size());
    Ok((info.width, info.height, buffer))
}

/// 每个像素丢掉 alpha 后的 RGB 字节哈希（跨实现对账用）。
fn sha256_rgb(pixels: &[u8]) -> String {
    let mut rgb = Vec::with_capacity(pixels.len() / 4 * 3);
    for pixel in pixels.chunks_exact(4) {
        rgb.extend_from_slice(&pixel[..3]);
    }
    sha256_hex(&rgb)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("读 {} 失败", path.display()))?;
    Ok(sha256_hex(&bytes))
}

/// 解析 `--indices`：`all` / `0-9,12` / JSON 文件（数组、`indices`、`ranges`）。
fn parse_index_spec(raw: &str) -> Result<IndexSpec> {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("all") {
        return Ok(IndexSpec::All);
    }
    let looks_like_json = Path::new(trimmed)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
    if looks_like_json {
        let text = std::fs::read_to_string(trimmed)
            .with_context(|| format!("读取索引文件 {trimmed} 失败"))?;
        let value: serde_json::Value =
            serde_json::from_str(&text).with_context(|| format!("{trimmed} 不是合法 JSON"))?;
        return index_spec_from_value(&value);
    }
    Ok(IndexSpec::List(parse_range_list(trimmed)?))
}

fn index_spec_from_value(value: &serde_json::Value) -> Result<IndexSpec> {
    match value {
        serde_json::Value::String(text) if text.eq_ignore_ascii_case("all") => Ok(IndexSpec::All),
        serde_json::Value::Array(items) => {
            let mut indices = Vec::new();
            for item in items {
                if let Some(number) = item.as_u64() {
                    indices.push(usize::try_from(number).unwrap_or(usize::MAX));
                } else if let Some(text) = item.as_str() {
                    indices.extend(parse_range_list(text)?);
                } else {
                    anyhow::bail!("索引数组里只能是数字或 'a-b' 字符串");
                }
            }
            indices.sort_unstable();
            indices.dedup();
            Ok(IndexSpec::List(indices))
        }
        serde_json::Value::Object(object) => {
            if let Some(raw) = object.get("indices") {
                return index_spec_from_value(raw);
            }
            if let Some(raw) = object.get("ranges") {
                let mut indices = Vec::new();
                for range in raw.as_array().context("ranges 必须是数组")? {
                    let pair = range.as_array().context("ranges 每项必须是 [start, end]")?;
                    let start = pair
                        .first()
                        .and_then(serde_json::Value::as_u64)
                        .context("ranges 缺 start")?;
                    let end = pair
                        .get(1)
                        .and_then(serde_json::Value::as_u64)
                        .context("ranges 缺 end")?;
                    for index in start..=end {
                        indices.push(usize::try_from(index).unwrap_or(usize::MAX));
                    }
                }
                indices.sort_unstable();
                indices.dedup();
                return Ok(IndexSpec::List(indices));
            }
            anyhow::bail!("索引 JSON 对象里需要 indices 或 ranges 字段");
        }
        other => anyhow::bail!("不支持的索引 JSON 形态：{other}"),
    }
}

/// 解析 `0-9,12,20-25`。
fn parse_range_list(raw: &str) -> Result<Vec<usize>> {
    let mut indices = Vec::new();
    for part in raw.split(',').filter(|part| !part.trim().is_empty()) {
        let part = part.trim();
        if let Some((start, end)) = part.split_once('-') {
            let start: usize = start
                .trim()
                .parse()
                .with_context(|| format!("区间起点不是数字：{part}"))?;
            let end: usize = end
                .trim()
                .parse()
                .with_context(|| format!("区间终点不是数字：{part}"))?;
            if start > end {
                anyhow::bail!("区间 {part} 起点大于终点");
            }
            indices.extend(start..=end);
        } else {
            indices.push(
                part.parse()
                    .with_context(|| format!("索引不是数字：{part}"))?,
            );
        }
    }
    indices.sort_unstable();
    indices.dedup();
    Ok(indices)
}

// ---------------------------------------------------------------------------
// `assets pack`：开发期散装 → 发布期 `.mnp` 小包（≤ --chunk-mb）
// ---------------------------------------------------------------------------

const ASSETS_PACK_USAGE: &str = "usage: mir-tools assets pack --input <game-assets> --out <packs> \
[--chunk-mb 16] [--no-compress] [--verify] [--list]";

/// 待打包的一条素材。
#[derive(Debug)]
struct PackItem {
    /// 包内逻辑名（相对输入目录的 '/' 路径）。
    logical: String,
    /// 磁盘路径。
    path: PathBuf,
    /// 类别（`tiles`/`sprites`/`ui`/`maps`/`audio`/`data`）。
    category: String,
    /// 分组（`tiles0`/`monster`/`item`/空）。
    group: String,
    /// 图像元数据（非图像为 `None`）。
    info: Option<mir_asset::ImageInfo>,
}

fn assets_pack(args: &mut impl Iterator<Item = String>) -> Result<()> {
    let mut input = PathBuf::from("game-assets");
    let mut out = PathBuf::from("game-assets/packs");
    let mut chunk_mb = 16u64;
    let mut compress = true;
    let mut verify = false;
    let mut list = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input = PathBuf::from(next_arg(args, "--input")?),
            "--out" => out = PathBuf::from(next_arg(args, "--out")?),
            "--chunk-mb" => {
                let raw = next_arg(args, "--chunk-mb")?;
                chunk_mb = raw
                    .parse()
                    .with_context(|| format!("--chunk-mb 不是数字：{raw}"))?;
                anyhow::ensure!(chunk_mb >= 1, "--chunk-mb 至少 1");
            }
            "--no-compress" => compress = false,
            "--verify" => verify = true,
            "--list" => list = true,
            other => anyhow::bail!("assets pack 不认识参数 {other}\n{ASSETS_PACK_USAGE}"),
        }
    }
    if verify {
        return verify_packs(&out, &input);
    }
    if list {
        return list_packs(&out);
    }

    let items = discover_pack_items(&input)?;
    if items.is_empty() {
        anyhow::bail!(
            "{} 下没有可打包的素材（先跑 assets unpack？）",
            input.display()
        );
    }
    let limit = chunk_mb * 1024 * 1024;
    let mut groups: std::collections::BTreeMap<(String, String), Vec<PackItem>> =
        std::collections::BTreeMap::new();
    for item in items {
        groups
            .entry((item.category.clone(), item.group.clone()))
            .or_default()
            .push(item);
    }

    std::fs::create_dir_all(&out)?;
    let mut summaries = Vec::new();
    for ((category, group), items) in groups {
        let stem = if group.is_empty() {
            category.clone()
        } else {
            format!("{category}-{group}")
        };
        let mut bucket = 1usize;
        let mut cursor = 0usize;
        while cursor < items.len() {
            let file_name = format!("{stem}-{bucket}.mnp");
            let pack_path = out.join(&file_name);
            let mut writer = mir_asset::PackWriter::create(&pack_path, compress)?;
            let mut taken = 0usize;
            while cursor < items.len() {
                let item = &items[cursor];
                let upper = raw_upper_bound(item);
                // 包头 32B + 目录项 ≈ 64B/条 + 尾部 32B，留 1KB 余量
                if taken > 0 && writer.current_len() + upper + 1024 > limit {
                    break;
                }
                if let Some(info) = item.info {
                    let pixels = read_png_rgba(&item.path)?.2;
                    writer.add_image(&item.logical, &info, &pixels)?;
                } else {
                    let bytes = std::fs::read(&item.path)
                        .with_context(|| format!("读 {} 失败", item.path.display()))?;
                    writer.add_raw(&item.logical, &bytes)?;
                }
                cursor += 1;
                taken += 1;
            }
            anyhow::ensure!(taken > 0, "分组 {stem} 的第一条就超限（chunk-mb 太小？）");
            let summary = writer.finish(&category)?;
            println!(
                "pack={} category={category} entries={} bytes={}",
                summary.file, summary.entries, summary.bytes
            );
            summaries.push(summary);
            bucket += 1;
        }
    }

    let manifest = mir_asset::PackManifest {
        version: 1,
        generated_at: time_stamp(),
        chunk_mb: u32::try_from(chunk_mb).unwrap_or(u32::MAX),
        packs: summaries,
    };
    let manifest_path = out.join("manifest.packs.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest)? + "\n",
    )
    .with_context(|| format!("写 {} 失败", manifest_path.display()))?;
    let total: u64 = manifest.packs.iter().map(|pack| pack.bytes).sum();
    println!(
        "manifest={} packs={} bytes={total} chunk_mb={chunk_mb}",
        manifest_path.display(),
        manifest.packs.len()
    );
    Ok(())
}

fn empty_image_info() -> mir_asset::ImageInfo {
    mir_asset::ImageInfo {
        width: 0,
        height: 0,
        offset_x: 0,
        offset_y: 0,
        format: 0,
        shadow_x: 0,
        shadow_y: 0,
        reserved: [0; 3],
        has_mask: false,
        data_offset: 0,
        data_len: 0,
    }
}

fn raw_upper_bound(item: &PackItem) -> u64 {
    if let Some(info) = item.info {
        return u64::from(info.width) * u64::from(info.height) * 4;
    }
    std::fs::metadata(&item.path).map_or(0, |meta| meta.len())
}

fn time_stamp() -> String {
    // 不引入 chrono 依赖：用系统时间拼 ISO-8601（够工具记录用）。
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => format!("epoch+{}s", duration.as_secs()),
        Err(_) => "epoch+0s".to_owned(),
    }
}

fn discover_pack_items(input: &Path) -> Result<Vec<PackItem>> {
    let mut items = Vec::new();
    let mut index_cache: std::collections::BTreeMap<PathBuf, serde_json::Value> =
        std::collections::BTreeMap::new();
    walk_files(input, &mut |path| {
        let Ok(relative) = path.strip_prefix(input) else {
            return Ok(());
        };
        let logical = relative.to_string_lossy().replace('\\', "/");
        let parts: Vec<&str> = logical.split('/').collect();
        if parts.first() == Some(&"packs") {
            return Ok(());
        }
        let category = match parts.first() {
            Some(&"gfx") => parts.get(1).copied().unwrap_or("gfx").to_owned(),
            Some(&"maps") => "maps".to_owned(),
            Some(&"audio") => "audio".to_owned(),
            Some(&"data") => "data".to_owned(),
            _ => return Ok(()),
        };
        let group = match category.as_str() {
            "tiles" | "ui" | "sprites" => parts.get(2).copied().unwrap_or("").to_owned(),
            _ => String::new(),
        };
        let extension = path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let is_image =
            category != "maps" && category != "audio" && category != "data" && extension == "png";
        if !is_image && extension == "json" {
            return Ok(()); // index.json 本身不进包
        }
        let info = if is_image {
            let parent = path.parent().unwrap_or(input).to_path_buf();
            if !index_cache.contains_key(&parent) {
                let index_path = parent.join("index.json");
                let value = std::fs::read_to_string(&index_path)
                    .ok()
                    .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                    .unwrap_or(serde_json::Value::Null);
                index_cache.insert(parent.clone(), value);
            }
            let file_name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned());
            let mut found = None;
            if let Some(images) = index_cache[&parent]
                .get("images")
                .and_then(serde_json::Value::as_array)
            {
                for entry in images {
                    if entry.get("png").and_then(serde_json::Value::as_str) == file_name.as_deref()
                    {
                        found = Some(mir_asset::ImageInfo {
                            width: entry
                                .get("width")
                                .and_then(serde_json::Value::as_u64)
                                .unwrap_or(0) as u16,
                            height: entry
                                .get("height")
                                .and_then(serde_json::Value::as_u64)
                                .unwrap_or(0) as u16,
                            offset_x: entry
                                .get("offset_x")
                                .and_then(serde_json::Value::as_i64)
                                .unwrap_or(0) as i16,
                            offset_y: entry
                                .get("offset_y")
                                .and_then(serde_json::Value::as_i64)
                                .unwrap_or(0) as i16,
                            format: entry
                                .get("format_code")
                                .and_then(serde_json::Value::as_u64)
                                .unwrap_or(0) as u8,
                            shadow_x: entry
                                .get("shadow_x")
                                .and_then(serde_json::Value::as_i64)
                                .unwrap_or(0) as i16,
                            shadow_y: entry
                                .get("shadow_y")
                                .and_then(serde_json::Value::as_i64)
                                .unwrap_or(0) as i16,
                            reserved: [0; 3],
                            has_mask: entry
                                .get("has_mask")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false),
                            data_offset: 0,
                            data_len: 0,
                        });
                        break;
                    }
                }
            }
            Some(found.unwrap_or_else(empty_image_info))
        } else {
            None
        };
        items.push(PackItem {
            logical,
            path: path.to_path_buf(),
            category,
            group,
            info,
        });
        Ok(())
    })?;
    items.sort_by(|left, right| left.logical.cmp(&right.logical));
    Ok(items)
}

fn walk_files(dir: &Path, visit: &mut impl FnMut(&Path) -> Result<()>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_files(&path, visit)?;
        } else {
            visit(&path)?;
        }
    }
    Ok(())
}

fn list_packs(out: &Path) -> Result<()> {
    let set = mir_asset::PackSet::open(out)?;
    println!("packs={} entries={}", set.pack_count(), set.len());
    for summary in &set.manifest().packs {
        println!(
            "  {} category={} entries={} bytes={} sha256={}",
            summary.file,
            summary.category,
            summary.entries,
            summary.bytes,
            &summary.sha256[..16]
        );
    }
    Ok(())
}

fn verify_packs(out: &Path, input: &Path) -> Result<()> {
    let set = mir_asset::PackSet::open(out)?;
    let manifest = set.manifest().clone();
    let mut problems = Vec::new();
    for summary in &manifest.packs {
        let path = out.join(&summary.file);
        let bytes = std::fs::metadata(&path).map_or(0, |meta| meta.len());
        if bytes != summary.bytes {
            problems.push(format!(
                "{} 字节数 {} ≠ manifest {}",
                summary.file, bytes, summary.bytes
            ));
        }
        let pack = mir_asset::PackFile::open(&path)?;
        if pack.len() as u32 != summary.entries {
            problems.push(format!(
                "{} 条目数 {} ≠ manifest {}",
                summary.file,
                pack.len(),
                summary.entries
            ));
        }
        println!(
            "  ok {} category={} entries={} bytes={} sha256={}",
            summary.file,
            summary.category,
            summary.entries,
            summary.bytes,
            &summary.sha256[..16]
        );
    }
    // 输入目录里的每个文件都必须在某个包里
    let items = discover_pack_items(input)?;
    for item in &items {
        if !set.contains(&item.logical) {
            problems.push(format!("包集合缺条目 {}", item.logical));
        }
    }
    if !problems.is_empty() {
        for problem in &problems {
            eprintln!("MISMATCH {problem}");
        }
        anyhow::bail!("verify 失败：{} 处不一致", problems.len());
    }
    println!(
        "verify ok packs={} entries={}（输入目录 {} 个文件全部覆盖）",
        set.pack_count(),
        set.len(),
        items.len()
    );
    Ok(())
}
