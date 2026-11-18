//! SupplierRefund 详情与分页视图装配。
use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::returns;
use erp_returns::repository::ReturnsExt;
use persistence_core::{Executor, Transactional};

use super::ReturnsReadService;
use super::approval::supplier_refund_approval_view;
use super::dto::SupplierRefundView;
use crate::{Error, Result};

impl ReturnsReadService {
    // -----------------------------------------------------------------------

    /// 查询供应商退款详情。
    ///
    /// # 参数
    /// * `actor` - 本次已认证账号，按固定详情入口检查功能资格。
    /// * `id` - 退款单 ID
    ///
    /// # 返回
    /// 返回退款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    ///
    /// 仓储或审批绑定读取失败时返回对应错误。
    pub async fn supplier_refund_detail(&self, id: &str, actor: &AuditActor) -> Result<SupplierRefundView> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.authorization
                        .require(
                            &actor,
                            &returns::SUPPLIER_REFUND_SUPPLIER_REFUND_DETAIL,
                            AccessTarget::Operation,
                            executor,
                        )
                        .await?;

                    this.load_supplier_refund_view(&id, executor).await
                })
            })
            .await
    }

    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 调用方必须先完成所属命令授权，并传入原执行器。
    /// 装配供应商退款单视图。
    ///
    /// # 参数
    /// * `executor` - 已完成所属命令授权的原读取或事务执行器。
    /// * `id` - 退款单 ID
    ///
    /// # 返回
    /// 返回退款单视图（含只读审批结构）。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    ///
    /// 仓储或审批绑定读取失败时返回对应错误。
    pub async fn load_supplier_refund_view(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierRefundView> {
        Self::ensure_command_snapshot(executor)?;
        let id = id.to_owned();

        let refund = self
            .db
            .supplier_refunds()
            .find_by_id(&id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商退款单不存在".to_string()))?;
        let binding = super::approval::optional_approval_binding(&self.db, &id, executor).await?;

        Ok(SupplierRefundView {
            id: refund.base.id.clone(),
            refund_no: refund.refund_no,
            settlement_method: refund.settlement_method,
            source_purchase_order_id: refund.source_purchase_order_id,
            settlement_confirmation: refund.settlement_confirmation,
            status: refund.status,
            purchase_return_order_id: refund.purchase_return_order_id.map(|id| id.to_string()),
            supplier_id: refund.supplier_id.to_string(),
            original_payment_id: refund.original_payment_id.map(|id| id.to_string()),
            original_payable_entry_id: refund.original_payable_entry_id.map(|id| id.to_string()),
            reason_code: refund.reason_code,
            reason_text: refund.reason_text,
            amount: refund.amount,
            handled_by: refund.handled_by,
            reviewed_by: refund.reviewed_by,
            occurred_at: refund.occurred_at,
            version: refund.base.version,
            created_at: refund.base.created_at,
            approval: super::approval::load_runtime(
                &self.db,
                erp_workflow::entity::document_registry::DocumentType::SupplierRefund,
                &id,
                supplier_refund_approval_view(binding.as_ref(), None, refund.status),
                executor,
            )
            .await?,
        })
    }
}
