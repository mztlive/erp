//! 销售变更原单草稿的授权与事务组合。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_identity::AccessTarget;
use erp_permissions::{internal, sales_order, sales_review};
use erp_read_models::sales_center::access::SalesAccess;
use erp_sales::dto::sales_review::{SalesChangeDraftView, SaveSalesChangeDraftRequest};
use erp_sales::repository::{SalesOrderExt, SalesOrderRepositoryScopeExt, SalesReviewExt};
use erp_sales::service::sales_review::SalesReviewService;
use persistence_core::{Executor, Transactional};

use super::SalesChangeProcess;
use crate::order_to_cash::adapters::catalog::CatalogQualificationAdapter;
use crate::{Error, Result};

impl SalesChangeProcess {
    /// 沿来源销售单修改权限读取原变更草稿。
    ///
    /// # 参数
    /// * `id` - 原销售变更单身份
    /// * `actor` - 当前认证账号
    ///
    /// # 返回
    /// 返回可编辑的完整目标内容。
    ///
    /// # 错误
    /// 无动作权限、不可见、缺单或尚未撤回时拒绝。
    pub async fn draft(&self, id: &str, actor: &AuditActor) -> Result<SalesChangeDraftView> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.draft_view(&id, &actor, executor).await })
            })
            .await
    }

    /// 查询凭证覆盖来源版本核对、草稿装配和最终交付，不能刷新检查时点掩盖到期。
    async fn draft_view(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SalesChangeDraftView> {
        let (access, scope) = SalesAccess::new(self.db.clone(), self.authorization.clone())
            .resolve(actor, &internal::SALES_CHANGE_DRAFT_READ, executor)
            .await?;
        let change = self
            .db
            .sales_change_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在或无权操作".into()))?;
        let order = self
            .db
            .sales_orders()
            .find_authorized(change.sales_order_id.as_ref(), &scope, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在或无权操作".into()))?;
        if order.current_revision_id() != Some(change.base_revision_id.as_ref()) {
            return Err(Error::ConflictError("销售单当前版本已变化，请重新核对变更基准".into()));
        }
        let view = SalesReviewService::new(self.db.clone()).draft(id, executor).await?;
        access.validate(executor).await?;
        Ok(view)
    }

    /// 在唯一事务中重验来源权限并保存原变更草稿。
    ///
    /// # 参数
    /// * `id` - 原销售变更单身份
    /// * `request` - 两项期望版本与完整目标内容
    /// * `actor` - 当前认证账号
    ///
    /// # 返回
    /// 返回保存后的原变更草稿，后续仍需提交审批。
    ///
    /// # 错误
    /// 权限失效、状态或版本变化、目标内容非法、商品不可售或写入失败时拒绝。
    pub async fn save_draft(
        &self,
        id: &str,
        request: SaveSalesChangeDraftRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeDraftView> {
        let db = self.db.clone();
        let process = Self::new(db.clone(), self.authorization.clone(), self.services.clone());
        let id = id.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.authorize_draft(&id, &actor, executor).await?;
                    let result = SalesReviewService::new(db.clone())
                        .save_draft(
                            &id,
                            request,
                            actor.id(),
                            &CatalogQualificationAdapter::new(process.services.catalog.clone()),
                            executor,
                        )
                        .await?;
                    let audit = actor.resource_log("sales_change_order.update", "sales_change_order", id)?;
                    persist_log(&db, &audit, executor).await?;
                    Ok(result)
                })
            })
            .await
    }

    /// 在当前执行器中沿原销售单重验编辑资格与基准版本。
    async fn authorize_draft(&self, id: &str, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        self.authorization
            .require(actor, &sales_review::SALES_CHANGE_DRAFT, AccessTarget::Operation, executor)
            .await?;
        let change = self
            .db
            .sales_change_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在或无权操作".into()))?;
        let order = self
            .command_access(actor, &sales_order::HANDOVER)
            .current(change.sales_order_id.as_ref(), executor)
            .await?;
        if order.current_revision_id() != Some(change.base_revision_id.as_ref()) {
            return Err(Error::ConflictError("销售单当前版本已变化，请重新核对变更基准".into()));
        }
        Ok(())
    }
}
