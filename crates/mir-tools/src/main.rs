//! 《传奇-Next》工具链入口。
//!
//! 当前提供地图文件检查（`.map` 读取与统计）与协议表统计；素材库（`.Lib`/`.wzl`）
//! 与补丁工具的实现在后续里程碑补齐，见 `README.md` 的路线图 M7。

use std::path::{Path, PathBuf};

use anyhow::Result;

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
