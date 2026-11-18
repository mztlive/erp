//! 拥有带审计外层事务的客户流程。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::run_audited;
use erp_customer::repository::prelude::*;
use erp_customer::{CustomerAccess, CustomerExt};
use erp_permissions::customer as customer_permissions;
use mongodb::Database;

use crate::{Error, Result};

/// 在同一写入事务内重验删除范围并软删除客户角色。
///
/// `db`：客户集合所在数据库；`access`：使用当前授权源的客户范围服务；`id`：目标客户稳定主键。
///
/// 删除成功且审计写入同一事务后返回。
///
/// 客户不存在、无删除资格或事务失败时拒绝。
///
/// handler 事前检查不能代替事务内 `require_with`；历史参与不授予删除。
pub async fn delete_customer(
    db: Database,
    access: CustomerAccess,
    id: String,
    actor: AuditActor,
) -> Result<()> {
    let audit = actor.clone().resource_log("customer.delete", "customer", id.clone())?;
    run_audited(&db, audit, move |db, executor| {
        Box::pin(async move {
            access.require_with(actor, &customer_permissions::DELETE, &id, executor).await?;
            let mut account = db
                .customer_accounts()
                .find_customer(&id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("客户不存在".into()))?;
            db.customer_accounts().soft_delete(&mut account, executor).await?;
            Ok::<(), Error>(())
        })
    })
    .await
}
