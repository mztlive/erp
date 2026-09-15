//! Named customer processes that own audited outer transactions.

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_customer::CustomerExt;
use erp_identity::SharedRbacService;
use mongodb::Database;

use crate::Result;
use crate::adapters::{customer_access, customer_service};
use crate::audit::run_audited;

/// Process module name.
pub fn process_name() -> &'static str {
    "customer"
}

/// 在同一写入事务内重验删除范围并软删除客户角色。
///
/// # 参数
/// * `db` - 客户集合所在数据库
/// * `rbac` - 当前 RBAC 快照
/// * `id` - 目标客户稳定主键
/// * `actor` - 已认证操作人
///
/// # 返回
/// 删除成功且审计写入同一事务后返回。
///
/// # 错误
/// 客户不存在、无删除资格或事务失败时拒绝。
///
/// # 关键业务约束
/// handler 事前检查不能代替事务内 `require_with`；历史参与不授予删除。
pub async fn delete_customer(
    db: Database,
    rbac: SharedRbacService,
    id: String,
    actor: AuditActor,
) -> Result<()> {
    let mut account = customer_service(db.clone()).load_customer(&id).await?;
    let audit = actor.clone().resource_log("customer.delete", "customer", account.base.id.clone())?;
    let actor_for_tx = actor.clone();
    let customer_id = id.clone();
    run_audited(&db, audit, move |db, session| {
        Box::pin(async move {
            customer_access(db.clone(), rbac)
                .require_with(actor_for_tx, "delete", &customer_id, session)
                .await?;
            db.customer_accounts().soft_delete(&mut account, session).await?;
            Ok(())
        })
    })
    .await?;
    Ok(())
}
