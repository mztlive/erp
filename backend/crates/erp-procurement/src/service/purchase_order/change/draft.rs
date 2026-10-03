//! 在调用方执行器中恢复原采购变更的完整编辑快照。

use persistence_core::Executor;

use crate::dto::purchase_order::PurchaseChangeDraftView;
use crate::entity::purchase_order::{PurchaseChangeOrder, PurchaseOrder};
use crate::repository::PurchaseOrderExt;
use crate::service::purchase_order::PurchaseOrderService;
use crate::{Error, Result};

impl PurchaseOrderService {
    /// 恢复原采购变更目标，已提交过的草稿优先采用最后冻结提交。
    ///
    /// # 参数
    /// * `change` - 同执行器加载的变更单。
    /// * `order` - 已证明完整更新、提交资格的来源采购单。
    /// * `executor` - 调用方授权快照使用的执行器。
    /// # 返回
    /// 返回可原单重提的完整付款条件和全部行。
    /// # 错误
    /// 非草稿、基准失效、冻结提交缺失或来源内容不一致时拒绝。
    pub async fn change_draft(
        &self,
        change: &PurchaseChangeOrder,
        order: &PurchaseOrder,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseChangeDraftView> {
        change.ensure_draft_origin(order).map_err(|error| Error::ConflictError(error.to_string()))?;
        if let Some(submission_id) = &change.current_submission_id {
            let submission = self
                .db
                .purchase_change_submissions()
                .find_by_id(submission_id.as_ref(), executor)
                .await?
                .ok_or_else(|| Error::ConflictError("采购变更当前冻结提交不存在".into()))?;
            let lines =
                self.db.purchase_order().list_change_submission_lines(submission_id, executor).await?;
            return PurchaseChangeDraftView::from_submission(change, &submission, &lines)
                .map_err(|error| Error::ConflictError(error.to_string()));
        }
        let revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(change.base_revision_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("采购变更基准版本不存在".into()))?;
        let lines = self.db.purchase_order().list_revision_lines(&change.base_revision_id, executor).await?;
        PurchaseChangeDraftView::from_base(change, &revision, &lines)
            .map_err(|error| Error::ConflictError(error.to_string()))
    }
}
