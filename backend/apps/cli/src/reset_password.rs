use config::ConfigArgs;
use erp_identity::{ResetAdminPasswordParams, ResetAdminPasswordResult};
use tracing::info;

use crate::args::ResetPasswordArgs;
use crate::error::Result;
use crate::password::resolve_password;
use crate::runtime::AdminRuntime;

/// 处理 `reset-password`，供运维人员为已有且未删除的管理员（含超级管理员）更换密码。
/// 此命令不新建账号、不改变启用状态或角色；需要恢复超级管理员时用 `init-admin`。
pub async fn run(source: &ConfigArgs, args: ResetPasswordArgs) -> Result<()> {
    let password = resolve_password(args.password)?;
    let runtime = AdminRuntime::connect(source).await?;
    let result = runtime
        .service
        .reset_admin_password(ResetAdminPasswordParams { account: args.account, password })
        .await?;
    print_result(&result);
    Ok(())
}

// 报告已重置的账号和当前启用状态但不输出密码；账号仍停用时提醒运维人员无法登录。
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
