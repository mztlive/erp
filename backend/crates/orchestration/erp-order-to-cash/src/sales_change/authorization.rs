//! 销售变更命令沿原销售单动作在原事务内重验对象范围。

use application_core::AuditActor;
use erp_permissions::Permission;
use erp_read_models::sales_center::access::SalesAccess;
use erp_sales::entity::sales_order::SalesOrder;
use persistence_core::Executor;

use super::SalesChangeProcess;
use crate::Result;

/// 命令上下文只保存请求身份；每次检查均重新解析服务端授权。
#[derive(Clone)]
pub(super) struct SalesChangeCommandAccess {
    /// 销售范围解析器。
    access: SalesAccess,
    /// 已认证操作人。
    actor: AuditActor,
    /// 本次命令对应的来源销售单动作。
    permission: Permission,
}

impl SalesChangeProcess {
    // 构造写命令范围检查器。授权源使用构造时注入的 RBAC，不退回路由级授权。
    //
    // `permission` 为对应的公共销售单权限常量。返回可在事务内重验来源销售单范围的检查器。
    //
    // 每次检查均重新解析服务端授权，不得复用列表上下文。
    pub(super) fn command_access(
        &self,
        actor: &AuditActor,
        permission: &Permission,
    ) -> SalesChangeCommandAccess {
        SalesChangeCommandAccess {
            access: SalesAccess::new(self.db.clone(), self.authorization.clone()),
            actor: actor.clone(),
            permission: permission.clone(),
        }
    }
}

impl SalesChangeCommandAccess {
    // 在调用方事务内读取当前可操作的来源销售单；历史参与不会产生写资格。
    //
    // `id` 为来源销售单主键。对象在范围内时返回销售单。不存在或越权时返回 NotFound，不泄露存在性。
    //
    // 写命令必须在原事务重验，不得把入口事前检查当作凭证。
    pub(super) async fn current(&self, id: &str, executor: &mut dyn Executor) -> Result<SalesOrder> {
        self.access
            .require_object(&self.actor, &self.permission, id, executor)
            .await
            .map_err(crate::Error::from)
    }
}
