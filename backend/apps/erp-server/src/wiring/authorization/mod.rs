//! HTTP 与 worker 共用的授权装配入口；具体业务绑定由组合根提供。
use std::sync::Arc;

use erp_identity::{AccessFactsPort, AccessMapping, AuthorizationService, Result, SharedRbacService};
use erp_permissions::Permission;
use mongodb::Database;

/// 连接身份设施与完整业务实现，不声明授权政策或启动运行循环。
/// # 参数
/// db、rbac 为共享身份设施；facts 按资源提供唯一读取器，mappings 按权限提供唯一查询入口。
/// # 返回
/// 经 identity 校验的共享授权服务；Application 在进程启动时保存此实例。
/// # 错误
/// 缺失、重复、未知或能力错配绑定原样返回错误，不构造不完整服务。
pub fn build(
    db: Database,
    rbac: SharedRbacService,
    facts: impl IntoIterator<Item = (String, Arc<dyn AccessFactsPort>)>,
    mappings: impl IntoIterator<Item = (Permission, Arc<dyn AccessMapping>)>,
) -> Result<Arc<AuthorizationService>> {
    let mut builder = AuthorizationService::builder(db, rbac);
    for (resource, facts) in facts {
        builder = builder.facts(&resource, facts)?;
    }
    for (permission, mapping) in mappings {
        builder = builder.mapping(&permission, mapping)?;
    }
    Ok(Arc::new(builder.build()?))
}
