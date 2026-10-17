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
