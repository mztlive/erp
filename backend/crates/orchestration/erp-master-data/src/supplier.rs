//! 拥有带审计外层事务的供应商流程。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::run_audited;
use erp_permissions::supplier;
use erp_supplier::{SupplierAccess, SupplierAccountId, SupplierExt};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 软删除供应商角色，并在同一事务中写入成功审计。
///
/// # 参数
/// * `db` - 供应商所在数据库。
/// * `access` - 使用当前授权源的供应商范围服务。
/// * `id` - 供应商角色主键。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 软删除和审计写入成功后返回。
///
/// # 错误
/// 无删除资格、供应商不存在、审计构造失败，或删除与审计事务失败时返回错误。
pub async fn delete_supplier(
    db: Database,
    access: SupplierAccess,
    id: String,
    actor: AuditActor,
) -> Result<()> {
    let audit = actor.clone().resource_log("supplier.delete", "supplier", id.clone())?;
    run_audited(&db, audit, move |db, executor| {
        Box::pin(async move { delete_stage(db, &access, &id, &actor, executor).await })
    })
    .await
}

/// 在成功审计所属事务中先授权，再读取当前供应商版本并软删除。
/// # 参数
/// db、access、id、actor 和 executor 均属于本次删除用例。
/// # 返回
/// 供应商软删除成功时返回。
/// # 错误
/// 无删除资格、供应商不存在、版本冲突或持久化失败时停止。
async fn delete_stage(
    db: &Database,
    access: &SupplierAccess,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    access.require_with(actor, &supplier::DELETE, id, executor).await?;
    let mut account = db
        .supplier()
        .account(&SupplierAccountId::new(id), executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".into()))?;
    db.supplier_accounts().soft_delete(&mut account, executor).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    /// 事务边界或当前资格不成立时，不能加载供应商或尝试删除。
    #[tokio::test]
    async fn deletion_requires_snapshot_and_authorization_before_business_reads_or_writes() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("supplier_deletion");
        let access = SupplierAccess::new(db.clone(), unavailable_authorization());
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);

        let error =
            delete_stage(&db, &access, "supplier", &actor, &mut ReadExecutor::default()).await.unwrap_err();
        assert!(matches!(error, Error::ValidationError(message)
            if message == "写入授权必须使用快照事务"));

        let error = delete_stage(&db, &access, "supplier", &actor, &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(message)
            if message == "测试授权设施不可用"));
    }
}
