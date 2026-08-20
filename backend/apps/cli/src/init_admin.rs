use services::iam::{InitializeSuperAdminParams, InitializeSuperAdminResult};
use tracing::info;

use crate::args::InitAdminArgs;
use crate::error::Result;
use crate::password::resolve_password;
use crate::runtime::AdminRuntime;

/// 执行超级管理员初始化。
///
/// # 参数
/// * `config_path` - TOML 配置文件路径
/// * `args` - 账号、名称与可选密码
///
/// # 返回值
/// 初始化成功返回 `Ok(())`。
///
/// # 错误
/// 配置、数据库、密码输入或服务编排失败时返回错误。
pub async fn run(config_path: &str, args: InitAdminArgs) -> Result<()> {
    let password = resolve_password(args.password)?;
    let runtime = AdminRuntime::connect(config_path).await?;
    let result = runtime
        .service
        .initialize_super_admin(InitializeSuperAdminParams {
            account: args.account,
            password,
            name: args.name,
        })
        .await?;
    print_result(&result);
    Ok(())
}

/// 向标准输出打印初始化结果，不包含密码。
///
/// # 参数
/// * `result` - 超级管理员初始化结果
///
/// # 返回值
/// 无。
///
/// # 错误
/// 无。
fn print_result(result: &InitializeSuperAdminResult) {
    let action = if result.created { "已创建" } else { "已更新" };
    info!(account = %result.account, admin_id = %result.admin_id, created = result.created, "超级管理员初始化完成");
    println!("{action}超级管理员");
    println!("账号: {}", result.account);
    println!("ID: {}", result.admin_id);
    println!("已恢复启用: {}", result.reactivated);
    println!("已绑定 root: {}", result.root_role_bound);
}
