use clap::Parser;
use config::Config;
use services::iam::{AdminService, InitializeSuperAdminParams};
use tracing::info;
use web_api::core::tracing::{init_tracing, TracingConfig};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// 超级管理员初始化命令参数。
#[derive(Debug, Parser)]
#[command(name = "init_super_admin", version, about = "初始化或修复系统超级管理员账号")]
struct Args {
    /// 配置文件路径
    #[arg(short, long, default_value = "./config.toml")]
    config_path: String,
    /// 超级管理员账号
    #[arg(long)]
    account: String,
    /// 超级管理员密码
    #[arg(long)]
    password: String,
    /// 超级管理员名称
    #[arg(long)]
    name: String,
}

/// 初始化系统超级管理员。
///
/// # 返回值
/// 成功返回 `Ok(())`，失败返回错误。
///
/// # 错误
/// 当配置加载、数据库连接或初始化流程失败时返回错误。
#[tokio::main]
async fn main() -> Result<()> {
    init_tracing(TracingConfig::default())?;

    let args = Args::parse();
    let config = Config::from_file(&args.config_path).await?;
    let (_, db) = database::connect(&config.database.uri, &config.database.db_name).await?;
    database::ensure_transaction_support(&db).await?;
    database::ensure_indexes(&db).await?;
    let rbac = services::iam::shared_rbac_service(db.clone());

    let result = AdminService::new(db, rbac)
        .initialize_super_admin(InitializeSuperAdminParams {
            account: args.account,
            password: args.password,
            name: args.name,
        })
        .await?;

    info!(
        admin_id = %result.admin_id,
        account = %result.account,
        created = result.created,
        reactivated = result.reactivated,
        root_role_bound = result.root_role_bound,
        "super admin initialized",
    );

    println!(
        "super admin initialized: account={}, admin_id={}, created={}, reactivated={}, root_role_bound={}",
        result.account, result.admin_id, result.created, result.reactivated, result.root_role_bound
    );

    Ok(())
}
