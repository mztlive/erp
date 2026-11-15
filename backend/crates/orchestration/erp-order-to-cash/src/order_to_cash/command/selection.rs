//! 方案预填与创建事务内来源重验。
use application_core::AuditActor;
use erp_core::ids::CustomerAccountId;
use erp_permissions::{internal, sales_selection as selection_permissions};
use erp_sales::dto::sales_order::selection_basis::SelectionOrderBasis;
use erp_sales::entity::sales_order::BusinessType;
use erp_sales::entity::sales_order::selection_source::SalesSelectionSource;
use erp_sales::repository::SalesSelectionExt;
use erp_sales::repository::sales_selection::{
    SalesSelectionDomainRepository, SalesSelectionProposalSkuLineRepositoryExt,
};
use persistence_core::{Executor, Transactional};

use super::super::SalesOrderCommandProcess;
use crate::{Error, Result};

impl SalesOrderCommandProcess {
    /// 读取已授权按份采购方案的开单预填。
    /// # 参数
    /// `id` 为方案身份；`actor` 为当前认证操作人。
    /// # 返回
    /// 返回冻结的客户与 SKU 明细，不创建销售单。
    /// # 错误
    /// 方案或客户不可访问、模式不支持或明细不完整时拒绝。
    pub async fn selection_order_basis(&self, id: &str, actor: &AuditActor) -> Result<SelectionOrderBasis> {
        let this = self.clone();
        let id = id.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { this.selection_basis(&id, &actor, executor).await })
            })
            .await
    }

    /// 按固定开单预填资格读取方案和明细，并在交付前检查原查询。
    async fn selection_basis(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SelectionOrderBasis> {
        let access = self.services.selections.clone();
        let scope = access.resolve(actor, &internal::SALES_SELECTION_ORDER_BASIS, executor).await?;
        let proposal = SalesSelectionDomainRepository::new(&self.db)
            .find_authorized_proposal(id, &scope, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售方案不存在或无权查看".into()))?;
        let lines = self.db.sales_selection_proposal_sku_lines().list_by_proposal(id, executor).await?;
        let basis = SelectionOrderBasis::from_proposal(&proposal, lines)?;
        scope.validate(executor).await?;
        Ok(basis)
    }

    /// 在调用方执行器中核验并冻结方案来源。
    /// # 参数
    /// `id` 为方案，`actor` 为认证人，`customer` 与 `business_type` 为目标单据，`executor` 为当前事务。
    /// # 返回
    /// 返回服务端来源快照。
    /// # 错误
    /// 无来源访问权限、客户或类型不匹配时拒绝。
    pub(super) async fn selection_source(
        &self,
        id: &str,
        actor: &AuditActor,
        customer: &CustomerAccountId,
        business_type: BusinessType,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionSource> {
        let access = self.services.selections.clone();
        let proposal =
            access.require_proposal(actor, &selection_permissions::PROPOSAL_DETAIL, id, executor).await?;
        Ok(SalesSelectionSource::from_proposal(&proposal, customer, business_type)?)
    }
}
