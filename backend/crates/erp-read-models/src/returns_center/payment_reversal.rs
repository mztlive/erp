//! PaymentReversal 详情与分页视图装配。
use super::approval::payment_reversal_approval_view;
use super::dto::PaymentReversalView;
use super::ReturnsReadService;
use crate::{Error, Result};
use erp_returns::repository::ReturnsExt;
use erp_workflow::service::document_registry::find_approval_binding;
use persistence_core::NoTransaction;

impl ReturnsReadService {
    // -----------------------------------------------------------------------

    /// 查询付款冲正详情。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    ///
    /// # 返回
    /// 返回冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单不存在
    pub async fn payment_reversal_detail(&self, id: &str) -> Result<PaymentReversalView> {
        self.payment_reversal_view(id.to_string()).await
    }

    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配付款冲正单视图。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    ///
    /// # 返回
    /// 返回冲正单视图（含只读审批结构）。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单不存在
    async fn payment_reversal_view(&self, id: String) -> Result<PaymentReversalView> {
        let reversal = self
            .db
            .payment_reversals()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("付款冲正单不存在".to_string()))?;
        let binding = match find_approval_binding(&self.db, &id, &mut NoTransaction)
            .await
            .map_err(crate::Error::from)
        {
            Ok(binding) => binding,
            Err(Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        Ok(PaymentReversalView {
            id: reversal.base.id.clone(),
            reversal_no: reversal.reversal_no,
            status: reversal.status,
            original_supplier_payment_id: reversal.original_supplier_payment_id.to_string(),
            reason_code: reversal.reason_code,
            reason_text: reversal.reason_text,
            amount: reversal.amount,
            handled_by: reversal.handled_by,
            reviewed_by: reversal.reviewed_by,
            occurred_at: reversal.occurred_at,
            version: reversal.base.version,
            created_at: reversal.base.created_at,
            approval: super::approval::load_runtime(
                &self.db,
                erp_workflow::entity::document_registry::DocumentType::PaymentReversal,
                &id,
                payment_reversal_approval_view(binding.as_ref(), None, reversal.status),
            )
            .await?,
        })
    }
}
