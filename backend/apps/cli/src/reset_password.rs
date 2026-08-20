use services::iam::{ResetAdminPasswordParams, ResetAdminPasswordResult};
use tracing::info;

use crate::args::ResetPasswordArgs;
use crate::error::Result;
use crate::password::resolve_password;
use crate::runtime::AdminRuntime;

/// 执行已有管理员密码重置。
///
/// # 参数
/// * `config_path` - TOML 配置文件路径
/// * `args` - 账号与可选密码
///
/// # 返回值
/// 重置成功返回 `Ok(())`。
///
/// # 错误
/// 配置、数据库、密码输入或服务编排失败时返回错误。
pub async fn run(config_path: &str, args: ResetPasswordArgs) -> Result<()> {
    let password = resolve_password(args.password)?;
    let runtime = AdminRuntime::connect(config_path).await?;
    let result = runtime
        .service
        .reset_admin_password(ResetAdminPasswordParams {
            account: args.account,
            password,
        })
        .await?;
    print_result(&result);
    Ok(())
}

/// 向标准输出打印重置结果，不包含密码。
///
/// # 参数
/// * `result` - 密码重置结果
///
/// # 返回值
/// 无。
///
/// # 错误
/// 无。
fn print_result(result: &ResetAdminPasswordResult) {
    info!(account = %result.account, admin_id = %result.admin_id, active = result.active, "管理员密码已重置");
    println!("已重置管理员密码");
    println!("账号: {}", result.account);
    println!("ID: {}", result.admin_id);
    println!("当前启用: {}", result.active);
    if !result.active {
        println!("提示: 账号当前未启用，仍无法登录。超级管理员请改用 init-admin 修复。");
    }
}
