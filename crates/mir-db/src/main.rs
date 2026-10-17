//! `mir-db` 命令行：解析一个 `Server.MirDB` 并打印稳定报告。
//!
//! ```text
//! mir-db resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB
//! ```
//!
//! 与 `mir-tools mirdb-report --path <文件>` 输出相同的文本；库本身不依赖任何 CLI 框架。

use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("用法：mir-db <Server.MirDB 路径>");
        return ExitCode::from(2);
    };

    match mir_db::MirDb::load_with_leftover(&path) {
        Ok((db, leftover)) => {
            print!("{}", mir_db::render_report(&db, leftover));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("解析 {path} 失败：{error}");
            ExitCode::FAILURE
        }
    }
}
