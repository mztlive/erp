//! SupplierRefund 详情与分页视图装配。
use erp_returns::repository::ReturnsExt;
use erp_workflow::service::document_registry::find_approval_binding;
use persistence_core::NoTransaction;

use super::ReturnsReadService;
use super::approval::supplier_refund_approval_view;
use super::dto::SupplierRefundView;
use crate::{Error, Result};

impl ReturnsReadService {
    // -----------------------------------------------------------------------

    /// 查询供应商退款详情。
    ///
    /// # 参数
    /// * `id` - 退款单 ID
    ///
    /// # 返回
    /// 返回退款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    pub async fn supplier_refund_detail(&self, id: &str) -> Result<SupplierRefundView> {
        self.supplier_refund_view(id.to_string()).await
    }

    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配供应商退款单视图。
    ///
    /// # 参数
    /// * `id` - 退款单 ID
    ///
    /// # 返回
    /// 返回退款单视图（含只读审批结构）。
    ///
    /// # 错误
    /// * `NotFound` - 退款单不存在
    async fn supplier_refund_view(&self, id: String) -> Result<SupplierRefundView> {
        let refund = self
            .db
            .supplier_refunds()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商退款单不存在".to_string()))?;
        let binding = match find_approval_binding(&self.db, &id, &mut NoTransaction)
            .await
            .map_err(crate::Error::from)
        {
            Ok(binding) => binding,
            Err(Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        Ok(SupplierRefundView {
            id: refund.base.id.clone(),
            refund_no: refund.refund_no,
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
            )
            .await?,
        })
    }
}
