//! 具体申请处理资格沿领域动作及精确对象范围重验。

use application_core::AuditActor;
use erp_catalog::portal::{CatalogPortalExt, NewProductDraft};
use erp_identity::{PortalActor, SharedRbacService};
use erp_supplier::portal::CooperationRepository;
use erp_supply::portal::{OfferingApplication, PortalSupplyExt};
use mongodb::Database;
use persistence_core::Executor;

use super::SupplierPortalProcess;
use crate::adapters::{catalog_access, offering_access};
use crate::{Error, Result};

impl SupplierPortalProcess {
    pub(super) async fn offering_reviewer_access(
        &self,
        actor: &AuditActor,
        app: &OfferingApplication,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.internal_supplier(actor, "detail", &app.supplier_id, executor).await?;
        let access = offering_access(self.db.clone(), self.rbac.clone());
        if let Some((id, _, _)) = app.snapshot.target() {
            access.require_offering(actor, "update", id, executor).await?;
        } else {
            let supplier = self.active_supplier(&app.supplier_id, executor).await?;
            access
                .ensure_writable(
                    actor,
                    "create",
                    &supplier.maintainer_user_id,
                    &supplier.business_org_unit_id,
                    executor,
                )
                .await?;
        }
        Ok(())
    }

    pub(super) async fn new_product_reviewer_access(
        &self,
        actor: &AuditActor,
        draft: &NewProductDraft,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let supplier = self.internal_supplier(actor, "detail", &draft.supplier_id, executor).await?;
        let owner = &supplier.maintainer_user_id;
        let org = &supplier.business_org_unit_id;
        catalog_access(self.db.clone(), self.rbac.clone())
            .ensure_writable(actor, "create", owner, org, executor)
            .await?;
        offering_access(self.db.clone(), self.rbac.clone())
            .ensure_writable(actor, "create", owner, org, executor)
            .await?;
        Ok(())
    }

    pub(super) async fn batch_offering_reviewer(
        &self,
        app: &OfferingApplication,
        actor: &PortalActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (owner, _) = self.application_owner(app, actor, executor).await?;
        let reviewer = self.reviewer_actor(&owner, executor).await?;
        self.internal_permission(&reviewer, "supplier_portal.application_approve", executor).await?;
        self.internal_permission(&reviewer, "supplier_portal.application_detail", executor).await?;
        self.offering_reviewer_access(&reviewer, app, executor).await
    }

    pub(super) async fn batch_new_product_reviewer(
        &self,
        draft: &NewProductDraft,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let supplier = self.active_supplier(&draft.supplier_id, executor).await?;
        let reviewer = self.reviewer_actor(&supplier.maintainer_user_id, executor).await?;
        self.internal_permission(&reviewer, "supplier_portal.new_product_approve", executor).await?;
        self.internal_permission(&reviewer, "supplier_portal.application_detail", executor).await?;
        self.new_product_reviewer_access(&reviewer, draft, executor).await
    }

    pub(super) async fn new_product_maintainer(
        &self,
        owner: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(String, String)> {
        let (owner, org) = self.offering_maintainer(owner, actor, executor).await?;
        let maintainer = self.reviewer_actor(&owner, executor).await?;
        let catalog = catalog_access(self.db.clone(), self.rbac.clone());
        catalog.ensure_writable(&maintainer, "update", &owner, &org, executor).await?;
        Ok((owner, org))
    }

    pub(super) async fn offering_maintainer(
        &self,
        owner: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(String, String)> {
        let maintainer = self.reviewer_actor(owner, executor).await?;
        let access = offering_access(self.db.clone(), self.rbac.clone());
        let (owner, org) = access.maintainer_org(Some(owner), actor, executor).await?;
        access.ensure_writable(&maintainer, "update", &owner, &org, executor).await?;
        Ok((owner, org))
    }
}

/// 为任务创建、转交和工作台提供同一具体申请业务处理资格。
///
/// # 参数
/// 组合层数据库、权限来源、当前内部处理人、精确申请及调用方执行器。
/// # 返回
/// 实际领域动作和对象范围均满足时为真；不存在或权限拒绝为假。
/// # 错误
/// 身份、权限版本、配置或持久化错误保持失败关闭。
pub(crate) async fn request_reviewable(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let process = SupplierPortalProcess::new(db.clone(), rbac.clone());
    let result = if let Some(app) = db.portal_applications().find_by_id(id, executor).await? {
        process.offering_reviewer_access(actor, &app, executor).await
    } else if let Some(draft) = db.new_product_drafts().find_by_id(id, executor).await? {
        process.new_product_reviewer_access(actor, &draft, executor).await
    } else if let Some(app) = CooperationRepository::new(db).find_any(id, executor).await? {
        process.internal_supplier(actor, "update", &app.supplier_id, executor).await.map(|_| ())
    } else {
        return Ok(false);
    };
    decision_access_result(result)
}

fn decision_access_result(result: Result<()>) -> Result<bool> {
    match result {
        Ok(()) => Ok(true),
        Err(Error::Forbidden(_) | Error::NotFound(_)) => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualification_denials_are_distinct_from_policy_and_infrastructure_errors() {
        assert!(decision_access_result(Ok(())).unwrap());
        assert!(!decision_access_result(Err(Error::Forbidden("缺少供给创建资格".into()))).unwrap());
        assert!(!decision_access_result(Err(Error::NotFound("供给不在更新范围内".into()))).unwrap());
        assert!(matches!(
            decision_access_result(Err(Error::ConflictError("DATA_SCOPE_CHANGED".into()))),
            Err(Error::ConflictError(message)) if message == "DATA_SCOPE_CHANGED"
        ));
        assert!(matches!(
            decision_access_result(Err(Error::Internal("授权未装配".into()))),
            Err(Error::Internal(message)) if message == "授权未装配"
        ));
    }
}
