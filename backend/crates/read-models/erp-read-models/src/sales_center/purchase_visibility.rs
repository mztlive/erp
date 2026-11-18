//! 销售侧查看受影响采购明细的资格。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::AuthorizationService;
use erp_permissions::purchase_order;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

// 确定销售侧受影响采购的适用条件时，当前账号仍可登录且具备采购列表权限时返回 `true`。
//
// 调用参数：
// * `rbac` - 当前授权源
// * `actor` - 当前账号；内部调用时为空
//
// 具备资格时返回 `true`；无账号上下文、账号停用或缺少权限时返回 `false`。
//
// 账号或授权查询失败时返回错误。
pub(crate) async fn can_list_purchases(
    _db: &Database,
    rbac: &Arc<AuthorizationService>,
    actor: Option<&AuditActor>,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let Some(actor) = actor else {
        return Ok(false);
    };
    Ok(rbac.has_permission(actor, &purchase_order::LIST, executor).await?)
}
