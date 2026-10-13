//! 销售侧查看受影响采购明细的资格。

use application_core::AuditActor;
use erp_identity::{AccessControlExt, Permission, SharedRbacService, subject};
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::Result;

/// 查看受影响采购明细所需的权限。
const PURCHASE_LIST_PERMISSION: &str = "purchase_order:list";

/// 当前账号仍可登录且具备采购列表权限时返回 `true`。
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `rbac` - 当前授权源
/// * `actor` - 当前账号；内部调用时为空
///
/// # 返回
/// 具备资格时返回 `true`；无账号上下文、账号停用或缺少权限时返回 `false`。
///
/// # 错误
/// 账号或授权查询失败时返回错误。
pub(crate) async fn can_list_purchases(
    db: &Database,
    rbac: &SharedRbacService,
    actor: Option<&AuditActor>,
) -> Result<bool> {
    let Some(actor) = actor else {
        return Ok(false);
    };
    let account = db.accounts().find_by_id(actor.id(), &mut NoTransaction).await?;
    let Some(account) = account.filter(|account| account.kind == actor.kind() && account.can_login()) else {
        return Ok(false);
    };
    let permission = Permission::parse(PURCHASE_LIST_PERMISSION)?;
    Ok(rbac.enforce(&subject(account.kind, &account.base.id), &permission).await?)
}
