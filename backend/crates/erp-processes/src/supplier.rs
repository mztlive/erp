//! 拥有带审计外层事务的供应商流程。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_supplier::SupplierExt;
use mongodb::Database;

use crate::Result;
use crate::adapters::supplier_service;
use crate::audit::run_audited;

/// 返回供应商流程模块名。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回稳定模块名 `supplier`。
///
/// # 错误
/// 不返回错误。
pub fn process_name() -> &'static str {
    "supplier"
}

/// 软删除供应商角色，并在同一事务中写入成功审计。
///
/// # 参数
/// * `db` - 供应商所在数据库。
/// * `id` - 供应商角色主键。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 软删除和审计写入成功后返回。
///
/// # 错误
/// 无删除资格、供应商不存在、审计构造失败，或删除与审计事务失败时返回错误。
pub async fn delete_supplier(db: Database, id: String, actor: AuditActor) -> Result<()> {
    crate::adapters::supplier_access(db.clone(), crate::adapters::identity::shared_rbac_service(db.clone()))
        .require(&actor, "delete", &id)
        .await?;
    let mut supplier = supplier_service(db.clone()).load_supplier(&id).await?;
    let audit = actor.clone().resource_log("supplier.delete", "supplier", supplier.base.id.clone())?;
    run_audited(&db, audit, move |db, executor| {
        Box::pin(async move {
            db.supplier_accounts().soft_delete(&mut supplier, executor).await?;
            Ok(())
        })
    })
    .await?;
    Ok(())
}
