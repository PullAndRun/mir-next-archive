//! 《传奇-Next》客户端入口（占位）。
//!
//! 里程碑 M6 之前这里只保留工程骨架：网络层可以连服务端并完成握手，
//! 渲染与界面（winit + wgpu）在后续里程碑接入。

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    tracing::info!("mir-client 尚未实现渲染与界面，见 README 路线图 M6");
    println!("传奇-Next 客户端骨架：渲染、界面与输入将在里程碑 M6 实现");
    Ok(())
}
