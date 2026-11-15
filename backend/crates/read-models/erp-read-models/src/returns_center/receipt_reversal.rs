//! ReceiptReversal 详情与分页视图装配。
use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::returns;
use erp_returns::repository::ReturnsExt;
use persistence_core::{Executor, Transactional};

use super::ReturnsReadService;
use super::approval::receipt_reversal_approval_view;
use super::dto::ReceiptReversalView;
use crate::{Error, Result};

impl ReturnsReadService {
    // -----------------------------------------------------------------------

    /// 查询回款冲正详情。
    ///
    /// # 参数
    /// * `actor` - 本次已认证账号，按固定详情入口检查功能资格。
    /// * `id` - 冲正单 ID
    ///
    /// # 返回
    /// 返回冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单不存在
    ///
    /// 仓储或审批绑定读取失败时返回对应错误。
    pub async fn receipt_reversal_detail(&self, id: &str, actor: &AuditActor) -> Result<ReceiptReversalView> {
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
                            &returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_DETAIL,
                            AccessTarget::Operation,
                            executor,
                        )
                        .await?;
                    this.load_receipt_reversal_view(&id, executor).await
                })
            })
            .await
    }

    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 调用方必须先完成所属命令授权，并传入原执行器。
    /// 装配回款冲正单视图。
    ///
    /// # 参数
    /// * `executor` - 已完成所属命令授权的原读取或事务执行器。
    /// * `id` - 冲正单 ID
    ///
    /// # 返回
    /// 返回冲正单视图（含只读审批结构）。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单不存在
    ///
    /// 仓储或审批绑定读取失败时返回对应错误。
    pub async fn load_receipt_reversal_view(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ReceiptReversalView> {
        Self::ensure_command_snapshot(executor)?;
        let id = id.to_owned();
        let reversal = self
            .db
            .receipt_reversals()
            .find_by_id(&id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("回款冲正单不存在".to_string()))?;
        let binding = super::approval::optional_approval_binding(&self.db, &id, executor).await?;
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
                executor,
            )
            .await?,
        })
    }
}
