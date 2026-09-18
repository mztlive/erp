//! ReceiptReversal 详情与分页视图装配。
use erp_returns::repository::ReturnsExt;
use persistence_core::NoTransaction;

use super::ReturnsReadService;
use super::approval::receipt_reversal_approval_view;
use super::dto::ReceiptReversalView;
use crate::{Error, Result};

impl ReturnsReadService {
    // -----------------------------------------------------------------------

    /// 查询回款冲正详情。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    ///
    /// # 返回
    /// 返回冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单不存在
    pub async fn receipt_reversal_detail(&self, id: &str) -> Result<ReceiptReversalView> {
        self.receipt_reversal_view(id.to_string()).await
    }

    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配回款冲正单视图。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    ///
    /// # 返回
    /// 返回冲正单视图（含只读审批结构）。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单不存在
    async fn receipt_reversal_view(&self, id: String) -> Result<ReceiptReversalView> {
        let reversal = self
            .db
            .receipt_reversals()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("回款冲正单不存在".to_string()))?;
        let binding = super::approval::optional_approval_binding(&self.db, &id).await?;
        Ok(ReceiptReversalView {
            id: reversal.base.id.clone(),
            reversal_no: reversal.reversal_no,
            status: reversal.status,
            original_customer_receipt_id: reversal.original_customer_receipt_id.to_string(),
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
                erp_workflow::entity::document_registry::DocumentType::ReceiptReversal,
                &id,
                receipt_reversal_approval_view(binding.as_ref(), None, reversal.status),
            )
            .await?,
        })
    }
}
