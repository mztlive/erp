//! 在调用方执行器中恢复原采购变更的完整编辑快照。

use persistence_core::Executor;

use crate::dto::purchase_order::PurchaseChangeDraftView;
use crate::entity::purchase_order::{
    PurchaseChangeOrder, PurchaseOrder, PurchaseOrderRevision, PurchaseOrderRevisionLine,
};
use crate::repository::PurchaseOrderExt;
use crate::service::purchase_order::PurchaseOrderService;
use crate::{Error, Result};

impl PurchaseOrderService {
    /// 在原授权快照内恢复可编辑的采购变更目标。
    /// 没有当前目标时读取冻结的基准版本；否则用当前提交指针恢复已保存的编辑副本或冻结提交，保留付款条件及明细。
    /// 仅接受草稿或已驳回；基准失效、当前目标缺失或来源错配时拒绝。
    pub async fn change_draft(
        &self,
        change: &PurchaseChangeOrder,
        order: &PurchaseOrder,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseChangeDraftView> {
        change.ensure_draft_origin(order).map_err(|error| Error::ConflictError(error.to_string()))?;

        let revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(change.base_revision_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("采购变更基准版本不存在".into()))?;
        let base_lines =
            self.db.purchase_order().list_revision_lines(&change.base_revision_id, executor).await?;

        let view = self.change_target_view(change, &revision, &base_lines, executor).await?;
        Ok(view.with_base(&revision, &base_lines))
    }

    // 按当前提交指针恢复已保存的编辑副本或冻结目标；只有未设置该指针时才从基准版本初始化。
    // 指针已设置但提交缺失时拒绝，不回退到基准。
    async fn change_target_view(
        &self,
        change: &PurchaseChangeOrder,
        revision: &PurchaseOrderRevision,
        base_lines: &[PurchaseOrderRevisionLine],
        executor: &mut dyn Executor,
    ) -> Result<PurchaseChangeDraftView> {
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
        PurchaseChangeDraftView::from_base(change, revision, base_lines)
            .map_err(|error| Error::ConflictError(error.to_string()))
    }
}
