use config::ConfigArgs;
use erp_identity::{InitializeSuperAdminParams, InitializeSuperAdminResult};
use tracing::info;

use crate::args::InitAdminArgs;
use crate::error::Result;
use crate::password::resolve_password;
use crate::runtime::AdminRuntime;

/// 处理 `init-admin`：通过身份域用例创建超级管理员，或更新已有管理员的显示名称和密码、
/// 恢复启用并补绑 `role-root`。恢复管理员权限时用此命令；只改密码则用 `reset-password`。
pub async fn run(source: &ConfigArgs, args: InitAdminArgs) -> Result<()> {
    let password = resolve_password(args.password)?;
    let runtime = AdminRuntime::connect(source).await?;
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

// 初始化成功后向运维人员汇报账号是新建还是修复，以及是否恢复启用、补绑 root；不输出密码。
fn print_result(result: &InitializeSuperAdminResult) {
    let action = if result.created { "已创建" } else { "已更新" };
    info!(account = %result.account, admin_id = %result.admin_id, created = result.created, "超级管理员初始化完成");
    println!("{action}超级管理员");
    println!("账号: {}", result.account);
    println!("ID: {}", result.admin_id);
    println!("已恢复启用: {}", result.reactivated);
    println!("已绑定 root: {}", result.root_role_bound);
}
