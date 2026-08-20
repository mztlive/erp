//! ERP 运维命令行入口。
//!
//! 本 crate 只编排 `services` 已有账号用例，禁止依赖 `web-api`。

mod args;
mod error;
mod init_admin;
mod password;
mod reset_password;
mod runtime;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use args::{Cli, Command};
use error::Result;

/// 程序入口。
///
/// # 返回值
/// 无；失败时向 stderr 打印错误并以状态码 1 退出。
///
/// # 错误
/// 命令解析、配置、数据库或服务编排失败时进程退出码为 1。
#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

/// 解析参数并分发子命令。
///
/// # 返回值
/// 子命令执行成功返回 `Ok(())`。
///
/// # 错误
/// 配置、数据库、密码输入或服务编排失败时返回错误。
async fn run() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();
    match cli.command {
        Command::InitAdmin(args) => init_admin::run(&cli.config_path, args).await,
        Command::ResetPassword(args) => reset_password::run(&cli.config_path, args).await,
    }
}

/// 按 `RUST_LOG` 初始化 tracing，默认 `info`。
///
/// # 参数
/// 无。
///
/// # 返回值
/// 无。
///
/// # 错误
/// 无；环境变量缺失时回退到 `info`。
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
