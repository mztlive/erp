//! 拥有带审计外层事务的客户流程。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::run_audited;
use erp_customer::{CustomerAccess, CustomerExt, CustomerService};
use mongodb::Database;

use crate::{Error, Result};

/// 返回客户流程模块名。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回稳定模块名 `customer`。
///
/// # 错误
/// 不返回错误。
pub fn process_name() -> &'static str {
    "customer"
}

/// 在同一写入事务内重验删除范围并软删除客户角色。
///
/// # 参数
/// * `db` - 客户集合所在数据库
/// * `service` - 应用装配的客户领域服务。
/// * `access` - 使用当前授权源的客户范围服务。
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
    service: CustomerService,
    access: CustomerAccess,
    id: String,
    actor: AuditActor,
) -> Result<()> {
    let mut account = service.load_customer(&id).await?;
    let audit = actor.clone().resource_log("customer.delete", "customer", account.base.id.clone())?;
    let actor_for_tx = actor.clone();
    let customer_id = id.clone();
    run_audited(&db, audit, move |db, executor| {
        Box::pin(async move {
            access.require_with(actor_for_tx, "delete", &customer_id, executor).await?;
            db.customer_accounts().soft_delete(&mut account, executor).await?;
            Ok::<(), Error>(())
        })
    })
    .await?;
    Ok(())
}
