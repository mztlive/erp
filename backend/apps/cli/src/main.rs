//! ERP 运维命令行入口。
//!
//! 本 crate 复用身份用例与领域增量索引入口，禁止依赖 `web-api`。

mod adapters;
mod indexes;
mod migrate_contract_templates;

mod error;
mod init_admin;
mod jd_vop;
mod password;
mod reset_password;
mod runtime;

use cli::args::{self, Cli, Command};
use error::Result;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::prelude::*;

// 进入 Tokio 运行时后执行运维命令；用例返回错误时写入标准错误并以状态码 1 退出。
#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

// 在接触配置或数据库前初始化日志，随后按脱敏规则解析参数，将配置来源交给选中的子命令。
async fn run() -> Result<()> {
    init_tracing();
    let cli = Cli::try_parse_safe_from(std::env::args_os()).unwrap_or_else(|error| error.exit());
    match cli.command {
        Command::JdVopCredentialImport(args) => jd_vop::import(&cli.config, args).await,
        Command::JdVopCredentialStatus => jd_vop::status(&cli.config).await,
        Command::MigrateContractTemplates => migrate_contract_templates::run(&cli.config).await,
        Command::InitAdmin(args) => init_admin::run(&cli.config, args).await,
        Command::ResetPassword(args) => reset_password::run(&cli.config, args).await,
    }
}

// CLI 启动时按 `RUST_LOG` 设置日志级别，缺失或无效时用 `info`。
// 始终屏蔽可能记录登录密码的 Nacos SDK 日志，不受运维日志级别影响。
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .finish()
        // SDK 的认证 debug 日志包含密码，必须独立于 RUST_LOG 强制过滤。
        .with(filter_fn(|metadata| !metadata.target().starts_with("nacos_sdk")))
        .init();
}
