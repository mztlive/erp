//! 具体申请处理资格沿领域动作及精确对象范围重验。

use std::sync::Arc;

use application_core::AuditActor;
use erp_catalog::portal::NewProductDraft;
use erp_identity::{AuthorizationService, PortalActor};
use erp_permissions::{catalog as catalog_permissions, supplier_offering, supplier_portal};
use erp_supply::portal::OfferingApplication;
use mongodb::Database;
use persistence_core::Executor;

use super::SupplierPortalProcess;
use super::request_access::{RequestAction, RequestTarget, require_request};
use crate::wiring::CatalogServices;
use crate::{Error, Result};

impl SupplierPortalProcess {
    // 执行固定申请审核政策，拒绝时补查身份以保留原认证错误。
    async fn require_request_review(
        &self,
        actor: &AuditActor,
        target: RequestTarget,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let result =
            require_request(&self.authorization, actor, target, RequestAction::Review, executor).await;
        if matches!(&result, Err(Error::Forbidden(_))) {
            self.internal_actor(actor, executor).await?;
        }
        result
    }

    // 用申请维护人充当批量供给审核人，并重验其审核与查看权限。
    //
    // `app`：供给申请；`actor`：当前门户身份，用于解析既有供给归属；`executor`：调用方执行器。
    //
    // 维护人仍可登录且具备审核、查看和对象范围时无返回值。
    //
    // 维护人无效、缺少门户权限，或供给对象范围不足时返回对应错误。
    pub(super) async fn batch_offering_reviewer(
        &self,
        app: &OfferingApplication,
        actor: &PortalActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (owner, _) = self.application_owner(app, actor, executor).await?;
        let reviewer = self
            .reviewer_actor(
                &owner,
                &[supplier_portal::ADMIN_REVIEW, supplier_portal::ADMIN_IMPACTS],
                executor,
            )
            .await?;
        self.require_request_review(&reviewer, RequestTarget::offering(app), executor).await
    }

    // 用供应商当前维护人充当批量新品审核人，并重验其审核与查看权限。
    //
    // `draft`：新品草稿；`executor`：调用方执行器。
    //
    // 维护人仍可登录且具备新品审核、查看和创建资格时无返回值。
    //
    // 供应商停用、维护人无效、缺少门户权限，或商品与供给创建资格不足时返回对应错误。
    pub(super) async fn batch_new_product_reviewer(
        &self,
        draft: &NewProductDraft,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let supplier = self.active_supplier(&draft.supplier_id, executor).await?;
        let reviewer = self
            .reviewer_actor(
                &supplier.maintainer_user_id,
                &[supplier_portal::ADMIN_REVIEW, supplier_portal::ADMIN_IMPACTS],
                executor,
            )
            .await?;
        self.require_request_review(&reviewer, RequestTarget::new_product(draft), executor).await
    }

    // 在供给维护人资格之外，再证明其可更新商品目录。
    //
    // `owner`：指定的内部维护人账号 ID；`actor`：当前内部操作人；`executor`：调用方执行器。
    //
    // 返回已验证的维护人账号 ID 和业务组织 ID。
    //
    // 维护人无效、供给维护组织无法解析，或商品更新资格不足时返回对应错误。
    pub(super) async fn new_product_maintainer(
        &self,
        owner: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(String, String)> {
        let (owner, org) = self.offering_maintainer(owner, actor, executor).await?;
        let maintainer = self.reviewer_actor(&owner, &[], executor).await?;
        let catalog = &self.services.catalog_access;
        catalog.ensure_writable(&maintainer, &catalog_permissions::PRODUCT_PRODUCT_UPDATE, executor).await?;
        Ok((owner, org))
    }

    // 证明指定维护人仍可登录，并能更新解析出的供给维护组织。
    //
    // `owner`：指定的内部维护人账号 ID；`actor`：当前内部操作人；`executor`：调用方执行器。
    //
    // 返回维护人账号 ID 和业务组织 ID。
    //
    // 维护人无效、维护组织无法解析，或供给更新资格不足时返回对应错误。
    pub(super) async fn offering_maintainer(
        &self,
        owner: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(String, String)> {
        let maintainer = self.reviewer_actor(owner, &[], executor).await?;
        let access = &self.services.offering_access;
        let (owner, org) = access.maintainer_org(Some(owner), actor, executor).await?;
        access.ensure_writable(&maintainer, &supplier_offering::REVISE, executor).await?;
        Ok((owner, org))
    }
}

/// 为任务创建、转交和工作台提供同一具体申请业务处理资格。
///
/// 组合层数据库、权限来源、当前内部处理人、精确申请及调用方执行器。
///
/// 实际领域动作和对象范围均满足时为真；不存在或权限拒绝为假。
///
/// 身份、权限版本、配置或持久化错误保持失败关闭。
pub async fn request_reviewable(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    services: Arc<CatalogServices>,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let process = SupplierPortalProcess::new(db.clone(), authorization.clone(), services);
    let Some(target) = RequestTarget::load(db, id, executor).await? else {
        return Ok(false);
    };
    decision_access_result(process.require_request_review(actor, target, executor).await)
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
