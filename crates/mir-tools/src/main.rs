//! 《传奇-Next》工具链入口。
//!
//! 当前提供地图文件检查（`.map` 读取与统计）、协议表统计与 `Server.MirDB` 数据库报告；
//! 素材库（`.Lib`/`.wzl`）与补丁工具的实现在后续里程碑补齐，见 `README.md` 的路线图 M7。

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
